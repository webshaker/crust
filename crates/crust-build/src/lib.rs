//! Intégration Cargo de Crust (section 17 du cahier des charges).
//!
//! `build.rs` :
//!
//! ```ignore
//! fn main() {
//!     crust_build::run();
//! }
//! ```
//!
//! `src/main.rs` (ou `src/lib.rs` avec `root_lib.rs`) :
//!
//! ```ignore
//! include!(concat!(env!("OUT_DIR"), "/crust/root_main.rs"));
//! ```

mod tree;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crust_syntax::{Layout, Options, SourceInfo, transpile_with};

use tree::BuildResolver;
pub use tree::{AutoRs, Dir, DirKind, Project, Root, RsModule, RscFile, Target};

pub struct Config {
    /// Répertoire du paquet (celui de `Cargo.toml`).
    pub manifest_dir: PathBuf,
    /// `OUT_DIR` du build script.
    pub out_dir: PathBuf,
    /// Nom de la crate lib, pour les imports depuis le binaire.
    pub lib_name: String,
}

impl Config {
    pub fn from_env() -> Result<Config, String> {
        let var = |k: &str| std::env::var(k).map_err(|_| format!("variable `{k}` absente (appeler depuis build.rs)"));
        let manifest_dir = PathBuf::from(var("CARGO_MANIFEST_DIR")?);
        let lib_name =
            lib_name(&manifest_dir).unwrap_or_else(|| var("CARGO_PKG_NAME").unwrap_or_default().replace('-', "_"));
        Ok(Config {
            manifest_dir,
            out_dir: PathBuf::from(var("OUT_DIR")?),
            lib_name,
        })
    }
}

/// `[lib] name = "..."` dans `Cargo.toml`, s'il est précisé.
fn lib_name(manifest_dir: &Path) -> Option<String> {
    toml_name(manifest_dir, "[lib]")
}

/// `name = "..."` de la section `section` de `Cargo.toml`, tirets remplacés par `_`.
fn toml_name(manifest_dir: &Path, section: &str) -> Option<String> {
    let toml = std::fs::read_to_string(manifest_dir.join("Cargo.toml")).ok()?;
    let mut in_section = false;
    for line in toml.lines().map(str::trim) {
        if line.starts_with('[') {
            in_section = line == section;
        } else if in_section && let Some(value) = line.strip_prefix("name") {
            let value = value.trim_start().strip_prefix('=')?.trim().trim_matches('"');
            return Some(value.replace('-', "_"));
        }
    }
    None
}

/// Nom de la crate lib du paquet (hors build script, sans `CARGO_PKG_NAME`).
pub fn crate_lib_name(manifest_dir: &Path) -> String {
    lib_name(manifest_dir)
        .or_else(|| toml_name(manifest_dir, "[package]"))
        .unwrap_or_default()
}

/// Traduction d'un `.rsc` du projet, exactement comme pour la compilation :
/// sert aussi à recalculer la source map (`cargo crust`, `crust-lsp`).
pub fn transpile_rsc(project: &Project, r: &tree::RscFile) -> Result<crust_syntax::Output, String> {
    let text = std::fs::read_to_string(&r.abs).map_err(|e| format!("{} : {e}", r.abs.display()))?;
    Ok(transpile_rsc_text(project, r, &text))
}

/// Traduction lisible (`struct` puis `impl`) d'un `.rsc`, pour `cargo crust eject` :
/// les `mod x;` sont laissés tels quels (le fichier `.rs` produit prend la place
/// du `.rsc` dans `src/`).
pub fn transpile_rsc_natural(project: &Project, r: &tree::RscFile) -> Result<crust_syntax::Output, String> {
    let text = std::fs::read_to_string(&r.abs).map_err(|e| format!("{} : {e}", r.abs.display()))?;
    let resolver = BuildResolver {
        project,
        current: r.abs.clone(),
        target: r.target(project.owner),
    };
    let options = Options {
        layout: Layout::Natural,
        source: None,
    };
    Ok(transpile_with(&text, &options, &resolver))
}

/// [`transpile_rsc`] sur un texte en mémoire (document ouvert dans l'éditeur).
pub fn transpile_rsc_text(project: &Project, r: &tree::RscFile, text: &str) -> crust_syntax::Output {
    let resolver = BuildResolver {
        project,
        current: r.abs.clone(),
        target: r.target(project.owner),
    };
    let options = Options {
        layout: Layout::Lines,
        source: Some(SourceInfo {
            file: r.abs.clone(),
            crate_root: r.root.is_some(),
        }),
    };
    transpile_with(text, &options, &resolver)
}

#[derive(Debug, Default)]
pub struct Report {
    pub written: usize,
    pub unchanged: usize,
    pub warnings: Vec<String>,
}

/// Point d'entrée de `build.rs`.
pub fn run() {
    println!("cargo::rerun-if-changed=src");
    let cfg = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("cargo::warning=crust-build : {e}");
            return;
        }
    };
    match generate(&cfg) {
        Ok(report) => {
            for w in &report.warnings {
                println!("cargo::warning={}", w.replace('\n', " "));
            }
        }
        Err(e) => {
            fallback(&cfg, &e);
            println!("cargo::warning=crust-build : {}", e.replace('\n', " "));
        }
    }
}

/// En cas d'échec, les fichiers racines contiennent l'erreur : `include!` ne
/// tombe pas sur un fichier absent et le message reste lisible.
fn fallback(cfg: &Config, error: &str) {
    let gen_dir = cfg.out_dir.join("crust");
    let _ = std::fs::create_dir_all(&gen_dir);
    let mut files = vec!["root_lib.rs".to_string(), "root_main.rs".to_string()];
    for e in std::fs::read_dir(cfg.manifest_dir.join("src/bin"))
        .into_iter()
        .flatten()
        .flatten()
    {
        let path = e.path();
        if path.extension().is_some_and(|x| x == "rs") {
            files.push(format!(
                "root_bin_{}.rs",
                path.file_stem().unwrap_or_default().to_string_lossy()
            ));
        }
    }
    for file in files {
        let _ = std::fs::write(
            gen_dir.join(file),
            format!("compile_error!({:?});\n", format!("crust-build : {error}")),
        );
    }
}

const FOOTER: &str = concat!("// @generated by crust-build ", env!("CARGO_PKG_VERSION"));

/// Génère tout le Rust standard du paquet dans `out_dir/crust/`.
pub fn generate(cfg: &Config) -> Result<Report, String> {
    let project = Project::scan(&cfg.manifest_dir.join("src"), &cfg.lib_name)?;
    let gen_dir = cfg.out_dir.join("crust");
    let mut report = Report {
        warnings: project.warnings.clone(),
        ..Report::default()
    };

    for r in &project.rsc {
        let out = transpile_rsc(&project, r)?;
        let mut code = out.code;
        if !code.ends_with('\n') {
            code.push('\n');
        }
        let _ = writeln!(code, "{FOOTER} from src/{}", r.rel.display());
        write_if_changed(&gen_path(&gen_dir, &r.rel), &code, &mut report)?;
    }

    for (path, code) in decl_files(&project, &gen_dir) {
        write_if_changed(&path, &code, &mut report)?;
    }
    // Pour les outils (débogueur, cargo crust) : répertoire source de ce paquet.
    let src_dir = format!("{}\n", project.src.display());
    write_if_changed(&gen_dir.join("source_dir"), &src_dir, &mut report)?;
    Ok(report)
}

/// Plage de l'instruction `include!(…"/crust/<cible>"…);` dans un `.rs`
/// (`cible` : `root_main.rs`, `root_lib.rs` ou `dir/<rép>.rs`).
pub fn include_statement(text: &str, target: &str) -> Option<(usize, usize)> {
    let at = text.find(&format!("/crust/{target}\""))?;
    let start = text[..at].rfind("include!")?;
    let end = at + text[at..].find(';')? + 1;
    Some((start, end))
}

/// Chemin du fichier généré pour le `.rsc` `src/<rel>`.
pub fn gen_path(gen_dir: &Path, rel: &Path) -> PathBuf {
    gen_dir.join("src").join(rel)
}

fn lit(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

/// Contenu de `root_<cible>.rs`, inclus par `src/main.rs` ou `src/lib.rs`.
pub fn root_content(p: &Project, gen_dir: &Path, target: Target) -> String {
    let mut code = root_decls(p, gen_dir, target);
    if let Some(r) = p.rsc.iter().find(|r| r.root == Some(target)) {
        let _ = writeln!(code, "include!({});", lit(&gen_path(gen_dir, &r.rel)));
    }
    let _ = writeln!(code, "{FOOTER}");
    code
}

/// Déclarations de modules de la cible, sans l'inclusion de la racine `.rsc`.
pub fn root_decls(p: &Project, gen_dir: &Path, target: Target) -> String {
    let mut code = format!("// Modules Crust de la cible {target:?}.\n");
    for e in &p.root_errors {
        let _ = writeln!(code, "compile_error!({e:?});");
    }
    if target == p.owner {
        dir_items(p, gen_dir, Path::new(""), &mut code);
    }
    code
}

/// Fichiers de déclarations générés (hors `.rsc`) : racines et répertoires mixtes.
pub fn decl_files(p: &Project, gen_dir: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    for root in &p.roots {
        files.push((gen_dir.join(&root.root_file), root_content(p, gen_dir, root.target)));
    }
    for (rel, dir) in &p.dirs {
        if matches!(dir.kind, DirKind::Hosted | DirKind::Generated) && !dir.module.is_empty() {
            let mut code = String::new();
            dir_items(p, gen_dir, rel, &mut code);
            let _ = writeln!(code, "{FOOTER}");
            files.push((dir_file(gen_dir, rel), code));
        }
    }
    files
}

/// Fichier de déclarations d'un répertoire : `<gen>/dir/<rép>.rs`. Monté par
/// `#[path]` (répertoire généré) ou inclus par le module `.rs` qui possède le
/// répertoire (répertoire mixte).
fn dir_file(gen_dir: &Path, rel: &Path) -> PathBuf {
    gen_dir.join("dir").join(rel).with_extension("rs")
}

/// Déclarations d'un répertoire de modules.
///
/// Ce texte est soit inclus par `include!`, soit chargé par `#[path]` : dans les
/// deux cas, un `mod x;` serait cherché à côté du fichier généré. Les `.rs` passent
/// donc par une enveloppe dont le `#[path]` désigne leur répertoire source, et
/// gardent ainsi la sémantique Rust pour leurs propres sous-modules.
///
/// Pas de module inline avec `#[path]` contenant un autre `#[path]` absolu :
/// rust-analyzer concatène les deux chemins (rustc, lui, les gère).
fn dir_items(p: &Project, gen_dir: &Path, rel: &Path, code: &mut String) {
    let d = &p.dirs[rel];
    for e in &d.errors {
        let _ = writeln!(code, "compile_error!({e:?});");
    }
    for &i in &d.rsc {
        let r = &p.rsc[i];
        if r.module.is_empty() {
            continue;
        }
        let _ = writeln!(
            code,
            "#[path = {}] pub mod {};",
            lit(&gen_path(gen_dir, &r.rel)),
            r.name
        );
        let _ = writeln!(code, "#[allow(unused_imports)] pub use self::{}::*;", r.name);
    }
    for s in &d.subdirs {
        let sub = &p.dirs[s];
        if matches!(sub.kind, DirKind::Unhosted(_)) {
            // Contenu non monté : on signale au moins ses erreurs (dont CR0010).
            for e in &sub.errors {
                let _ = writeln!(code, "compile_error!({e:?});");
            }
        }
        if sub.kind != DirKind::Generated || sub.module.is_empty() {
            continue;
        }
        let name = s.file_name().unwrap_or_default().to_string_lossy();
        let _ = writeln!(code, "#[path = {}] pub mod {name};", lit(&dir_file(gen_dir, s)));
    }
    for &i in &d.auto_rs {
        let a = &p.auto_rs[i];
        if a.module.is_empty() {
            continue;
        }
        let file_name = a.file.file_name().unwrap_or_default().to_string_lossy();
        let decl = if file_name == format!("{}.rs", a.name) {
            format!("pub mod {};", a.name)
        } else {
            format!("#[path = {file_name:?}] pub mod {};", a.name)
        };
        let dir = a.file.parent().unwrap_or(&p.src);
        let _ = writeln!(code, "#[path = {}] mod __crust_rs_{} {{ {decl} }}", lit(dir), a.name);
        let _ = writeln!(code, "pub use self::__crust_rs_{0}::{0};", a.name);
    }
}

fn write_if_changed(path: &Path, content: &str, report: &mut Report) -> Result<(), String> {
    if std::fs::read_to_string(path).ok().as_deref() == Some(content) {
        report.unchanged += 1;
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    }
    std::fs::write(path, content).map_err(|e| format!("{} : {e}", path.display()))?;
    report.written += 1;
    Ok(())
}
