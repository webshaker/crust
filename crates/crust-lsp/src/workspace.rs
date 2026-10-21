//! Paquets Crust du workspace, documents vus par rust-analyzer, et conversion
//! des positions entre `.rsc` et code généré.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crust_syntax::{LineIndex, SourceMap, Span};
use serde_json::{Value, json};

/// Encodage des colonnes LSP négocié à l'initialisation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf16,
    Utf8,
}

pub fn offset_of(text: &str, pos: &Value, enc: Encoding) -> usize {
    let line = pos["line"].as_u64().unwrap_or(0) as usize;
    let col = pos["character"].as_u64().unwrap_or(0) as usize;
    let idx = LineIndex::new(text);
    match enc {
        Encoding::Utf16 => idx.offset_utf16(line, col),
        Encoding::Utf8 => {
            let start = idx.offset(line, 0).unwrap_or(text.len());
            let mut off = (start + col).min(text.len());
            while !text.is_char_boundary(off) {
                off -= 1;
            }
            off
        }
    }
}

pub fn position_of(text: &str, offset: usize, enc: Encoding) -> Value {
    let lc = LineIndex::new(text).line_col(offset);
    let character = match enc {
        Encoding::Utf16 => lc.col_utf16,
        Encoding::Utf8 => {
            let off = offset.min(text.len());
            off - text[..off].rfind('\n').map_or(0, |i| i + 1)
        }
    };
    json!({ "line": lc.line, "character": character })
}

pub fn path_of_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let mut out = Vec::with_capacity(rest.len());
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    Some(PathBuf::from(String::from_utf8_lossy(&out).into_owned()))
}

pub fn uri_of_path(path: &Path) -> String {
    let mut s = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => s.push(b as char),
            _ => s.push_str(&format!("%{b:02X}")),
        }
    }
    s
}

/// Un `.rsc` et le document correspondant côté rust-analyzer.
pub struct FileState {
    pub src_path: PathBuf,
    pub src_text: String,
    /// Document vu par rust-analyzer : le fichier généré, ou le `.rs` racine pour
    /// `main.rsc` / `lib.rsc` (texte inséré à la place de l'`include!`).
    pub doc_path: PathBuf,
    pub doc_text: String,
    /// Début du code généré dans `doc_text`.
    pub offset: usize,
    pub gen_len: usize,
    /// Fichier généré sur disque (le même que `doc_path`, sauf pour une racine).
    pub gen_path: PathBuf,
    pub map: SourceMap,
}

impl FileState {
    /// Position `.rsc` → position du document rust-analyzer.
    pub fn src_to_doc(&self, pos: &Value, enc: Encoding) -> Value {
        let src_off = offset_of(&self.src_text, pos, enc);
        let gen_off = self.map.src_to_gen(src_off).unwrap_or_else(|| {
            // Zone effacée (champ, attributs) : même ligne, même colonne, grâce à la
            // disposition préservant les lignes.
            let lc = LineIndex::new(&self.src_text).line_col(src_off);
            let generated = &self.doc_text[self.offset..self.offset + self.gen_len];
            LineIndex::new(generated).offset(lc.line, lc.col_char).unwrap_or(0)
        });
        position_of(&self.doc_text, self.offset + gen_off, enc)
    }

    /// Plage du document (dans `doc_text` ou, avec `alias`, dans le fichier généré
    /// sur disque) → plage `.rsc`, et exactitude.
    pub fn doc_to_src(&self, range: &Value, enc: Encoding, alias: bool) -> Option<(Value, bool)> {
        let (text, base) = if alias {
            (&self.doc_text[self.offset..self.offset + self.gen_len], 0)
        } else {
            (self.doc_text.as_str(), self.offset)
        };
        let start = offset_of(text, &range["start"], enc);
        let end = offset_of(text, &range["end"], enc);
        if start < base || end > base + self.gen_len {
            return None;
        }
        let m = self.map.gen_span_to_src(Span::new(start - base, end - base))?;
        let r = json!({
            "start": position_of(&self.src_text, m.span.start, enc),
            "end": position_of(&self.src_text, m.span.end, enc),
        });
        Some((r, m.exact))
    }
}

pub struct Package {
    pub manifest_dir: PathBuf,
    pub project: crust_build::Project,
    /// `<OUT_DIR de rust-analyzer>/crust`.
    pub gen_dir: PathBuf,
    pub files: HashMap<PathBuf, FileState>,
    /// Fichiers de déclarations (racines, répertoires mixtes) : chemin → texte.
    pub decls: Vec<(PathBuf, String)>,
}

impl Package {
    pub fn load(manifest_dir: &Path, gen_dir: &Path, docs: &HashMap<PathBuf, String>) -> Result<Package, String> {
        let project =
            crust_build::Project::scan(&manifest_dir.join("src"), &crust_build::crate_lib_name(manifest_dir))?;
        let mut p = Package {
            manifest_dir: manifest_dir.to_path_buf(),
            project,
            gen_dir: gen_dir.to_path_buf(),
            files: HashMap::new(),
            decls: Vec::new(),
        };
        p.refresh(docs);
        Ok(p)
    }

    /// Retranspile tout le paquet (textes ouverts dans l'éditeur prioritaires).
    pub fn refresh(&mut self, docs: &HashMap<PathBuf, String>) {
        self.files.clear();
        self.decls = crust_build::decl_files(&self.project, &self.gen_dir);
        for r in &self.project.rsc {
            let text = docs
                .get(&r.abs)
                .cloned()
                .or_else(|| std::fs::read_to_string(&r.abs).ok())
                .unwrap_or_default();
            if let Some(fs) = self.file_state(r, text, docs) {
                self.files.insert(r.abs.clone(), fs);
            }
        }
    }

    /// Met à jour un seul `.rsc` après une frappe.
    pub fn update(&mut self, src: &Path, text: String, docs: &HashMap<PathBuf, String>) -> bool {
        let Some(r) = self.project.rsc.iter().find(|r| r.abs == src) else {
            return false;
        };
        match self.file_state(r, text, docs) {
            Some(fs) => {
                self.files.insert(src.to_path_buf(), fs);
                true
            }
            None => false,
        }
    }

    fn file_state(&self, r: &crust_build::RscFile, text: String, docs: &HashMap<PathBuf, String>) -> Option<FileState> {
        let out = crust_build::transpile_rsc_text(&self.project, r, &text);
        let gen_path = crust_build::gen_path(&self.gen_dir, &r.rel);
        let gen_len = out.code.len();
        match r.root {
            None => Some(FileState {
                src_path: r.abs.clone(),
                src_text: text,
                doc_path: gen_path.clone(),
                doc_text: out.code,
                offset: 0,
                gen_len,
                gen_path,
                map: out.map,
            }),
            Some(target) => {
                // Racine : le `.rs` racine reçoit le code à la place de son `include!`.
                let root = self.project.root(target)?;
                let stub_path = root.rs.clone();
                let stub = docs
                    .get(&stub_path)
                    .cloned()
                    .or_else(|| std::fs::read_to_string(&stub_path).ok())?;
                let (start, end) = crust_build::include_statement(&stub, &root.root_file)?;
                let decls = crust_build::root_decls(&self.project, &self.gen_dir, target);
                let doc_text = format!("{}{}\n{decls}{}", &stub[..start], out.code, &stub[end..]);
                Some(FileState {
                    src_path: r.abs.clone(),
                    src_text: text,
                    doc_path: stub_path,
                    doc_text,
                    offset: start,
                    gen_len,
                    gen_path,
                    map: out.map,
                })
            }
        }
    }

    /// État du `.rsc` dont le document rust-analyzer (ou le fichier généré) est `path`.
    /// Le booléen indique un alias (fichier généré d'une racine, coordonnées sans décalage).
    pub fn by_doc(&self, path: &Path) -> Option<(&FileState, bool)> {
        self.files.values().find_map(|f| {
            if f.doc_path == path {
                Some((f, false))
            } else if f.gen_path == path {
                // Fichier généré d'une racine : rust-analyzer voit le `.rs` racine,
                // mais cargo check signale ses erreurs dans ce fichier.
                Some((f, true))
            } else {
                None
            }
        })
    }
}

/// Paquets du workspace qui utilisent `crust-build`, avec leur `OUT_DIR` tel que
/// rust-analyzer le voit (celui de `cargo check --all-targets`).
pub fn discover(root: &Path) -> Vec<(PathBuf, PathBuf)> {
    let Ok(out) = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    let Ok(meta) = serde_json::from_slice::<Value>(&out.stdout) else {
        return Vec::new();
    };
    let mut wanted: HashMap<String, PathBuf> = HashMap::new();
    for pkg in meta["packages"].as_array().into_iter().flatten() {
        let uses_crust = pkg["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|d| d["name"] == "crust-build" && d["kind"] == "build");
        if uses_crust && let (Some(id), Some(manifest)) = (pkg["id"].as_str(), pkg["manifest_path"].as_str()) {
            wanted.insert(
                id.to_string(),
                Path::new(manifest).parent().unwrap_or(root).to_path_buf(),
            );
        }
    }
    if wanted.is_empty() {
        return Vec::new();
    }
    // On n'attend que l'exécution des build scripts, puis on arrête cargo.
    let Ok(mut child) = Command::new("cargo")
        .args(["check", "--message-format=json", "--all-targets", "--workspace"])
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Vec::new();
    };
    let mut found = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if v["reason"] == "build-script-executed"
                && let (Some(id), Some(out_dir)) = (v["package_id"].as_str(), v["out_dir"].as_str())
                && let Some(manifest_dir) = wanted.remove(id)
            {
                found.push((manifest_dir, Path::new(out_dir).join("crust")));
                if wanted.is_empty() {
                    break;
                }
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    found
}
