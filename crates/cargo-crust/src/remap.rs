//! Remappage des diagnostics rustc (JSON) vers les fichiers `.rsc` (section 18.2).
//!
//! Un span dans `…/out/crust/src/<rel>` désigne le fichier généré depuis
//! `src/<rel>`. La source map est recalculée en retranspilant le `.rsc` avec
//! l'arbre de modules du paquet ; si le résultat ne correspond pas au fichier
//! généré (build en retard sur la source), on se contente de changer le chemin :
//! la disposition préservant les lignes garde lignes et colonnes du code écrit.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crust_syntax::{LineIndex, SourceMap, Span};
use serde_json::Value;

const GEN_MARKER: &str = "/out/crust/src/";

struct FileMap {
    src_path: PathBuf,
    src_text: String,
    /// `None` : correspondance par lignes seulement.
    map: Option<SourceMap>,
}

#[derive(Default)]
pub struct Remapper {
    packages: HashMap<PathBuf, Option<crust_build::Project>>,
    files: HashMap<String, Option<FileMap>>,
    cwd: PathBuf,
}

impl Remapper {
    pub fn new() -> Self {
        Remapper {
            cwd: std::env::current_dir().unwrap_or_default(),
            ..Default::default()
        }
    }

    /// Remappe un message `compiler-message` de cargo. Renvoie `true` si au
    /// moins un span désignait un fichier généré.
    pub fn remap_message(&mut self, msg: &mut Value, manifest_dir: &Path) -> bool {
        let mut touched = false;
        if let Some(spans) = msg.get_mut("spans").and_then(Value::as_array_mut) {
            for span in spans {
                touched |= self.remap_span(span, manifest_dir);
            }
        }
        if let Some(children) = msg.get_mut("children").and_then(Value::as_array_mut) {
            for child in children {
                touched |= self.remap_message(child, manifest_dir);
            }
        }
        touched
    }

    fn remap_span(&mut self, span: &mut Value, manifest_dir: &Path) -> bool {
        let mut touched = false;
        if let Some(expansion) = span.get_mut("expansion").filter(|e| e.is_object()) {
            for key in ["span", "def_site_span"] {
                if let Some(s) = expansion.get_mut(key).filter(|s| s.is_object()) {
                    touched |= self.remap_span(s, manifest_dir);
                }
            }
        }
        let Some(file_name) = span["file_name"].as_str().map(str::to_string) else {
            return touched;
        };
        let Some(i) = file_name.find(GEN_MARKER) else {
            return touched;
        };
        let rel = file_name[i + GEN_MARKER.len()..].to_string();
        if !self.files.contains_key(&file_name) {
            let fm = self.load(&file_name, &rel, manifest_dir);
            self.files.insert(file_name.clone(), fm);
        }
        let Some(fm) = self.files.get(&file_name).and_then(Option::as_ref) else {
            return touched;
        };

        let gen_start = span["byte_start"].as_u64().unwrap_or(0) as usize;
        let gen_end = span["byte_end"].as_u64().unwrap_or(0) as usize;
        let idx = LineIndex::new(&fm.src_text);
        let (src, exact) = match &fm.map {
            Some(map) => match map.gen_span_to_src(Span::new(gen_start, gen_end)) {
                Some(m) => (m.span, m.exact),
                None => return touched,
            },
            None => {
                // Même ligne, même colonne (en caractères).
                let line = span["line_start"].as_u64().unwrap_or(1) as usize - 1;
                let col = span["column_start"].as_u64().unwrap_or(1) as usize - 1;
                let line_end = span["line_end"].as_u64().unwrap_or(1) as usize - 1;
                let col_end = span["column_end"].as_u64().unwrap_or(1) as usize - 1;
                let s = idx.offset(line, col).unwrap_or(0);
                let e = idx.offset(line_end, col_end).unwrap_or(s);
                (Span::new(s, e), false)
            }
        };
        let start = idx.line_col(src.start);
        let end = idx.line_col(src.end);
        let display = match fm.src_path.strip_prefix(&self.cwd) {
            Ok(p) => p.display().to_string(),
            Err(_) => fm.src_path.display().to_string(),
        };
        let lines: Vec<Value> = fm
            .src_text
            .lines()
            .enumerate()
            .skip(start.line)
            .take(end.line - start.line + 1)
            .map(|(n, text)| {
                let hs = if n == start.line { start.col_char + 1 } else { 1 };
                let he = if n == end.line {
                    end.col_char + 1
                } else {
                    text.chars().count() + 1
                };
                serde_json::json!({ "text": text, "highlight_start": hs, "highlight_end": he })
            })
            .collect();

        span["file_name"] = Value::from(display);
        span["byte_start"] = Value::from(src.start);
        span["byte_end"] = Value::from(src.end);
        span["line_start"] = Value::from(start.line + 1);
        span["line_end"] = Value::from(end.line + 1);
        span["column_start"] = Value::from(start.col_char + 1);
        span["column_end"] = Value::from(end.col_char + 1);
        span["text"] = Value::from(lines);
        if !exact && !span["suggested_replacement"].is_null() {
            // La correction proposée vise du code généré : elle ne s'applique pas au `.rsc`.
            span["suggested_replacement"] = Value::Null;
            span["suggestion_applicability"] = Value::Null;
        }
        true
    }

    fn load(&mut self, gen_file: &str, rel: &str, manifest_dir: &Path) -> Option<FileMap> {
        let src_path = manifest_dir.join("src").join(rel);
        let src_text = std::fs::read_to_string(&src_path).ok()?;
        let project = self
            .packages
            .entry(manifest_dir.to_path_buf())
            .or_insert_with(|| {
                crust_build::Project::scan(&manifest_dir.join("src"), &crust_build::crate_lib_name(manifest_dir)).ok()
            })
            .as_ref();
        let map = project.and_then(|p| {
            let r = p.rsc.iter().find(|r| r.rel == Path::new(rel))?;
            let out = crust_build::transpile_rsc(p, r).ok()?;
            let generated = std::fs::read_to_string(gen_file).ok()?;
            // Le fichier généré = code + pied de page `// @generated …`.
            generated.starts_with(&out.code).then_some(out.map)
        });
        Some(FileMap {
            src_path,
            src_text,
            map,
        })
    }
}
