//! `cargo crust eject` : convertit le paquet en Rust standard (section 17.6).
//!
//! Chaque `.rsc` devient un `.rs` lisible (`struct` puis `impl`, formaté par
//! rustfmt), de vrais `mod` remplacent les lignes `include!`, et `crust-build`
//! est retiré. Le résultat compile avec cargo seul et se comporte à l'identique.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use crust_build::{DirKind, Project};

enum Action {
    Write(PathBuf, String),
    Delete(PathBuf),
}

fn rel_to(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Déclarations Rust standard d'un répertoire de modules, écrites dans `decl_file`.
fn decls(p: &Project, rel: &Path, decl_file: &Path) -> String {
    let d = &p.dirs[rel];
    let base = decl_file.parent().unwrap_or(&p.src);
    let mut code = String::new();
    for &i in &d.rsc {
        let r = &p.rsc[i];
        if !r.module.is_empty() {
            let _ = writeln!(
                code,
                "pub mod {};\n#[allow(unused_imports)]\npub use self::{}::*;",
                r.name, r.name
            );
        }
    }
    for s in &d.subdirs {
        if p.dirs[s].kind == DirKind::Generated {
            let _ = writeln!(code, "pub mod {};", s.file_name().unwrap_or_default().to_string_lossy());
        }
    }
    for &i in &d.auto_rs {
        let a = &p.auto_rs[i];
        if a.module.is_empty() {
            continue;
        }
        if a.file
            .file_name()
            .is_some_and(|f| f.to_string_lossy() == format!("{}.rs", a.name))
        {
            let _ = writeln!(code, "pub mod {};", a.name);
        } else {
            let _ = writeln!(code, "#[path = {:?}]\npub mod {};", rel_to(&a.file, base), a.name);
        }
    }
    code
}

/// Remplace l'instruction `include!` de `target` dans `file` par `replacement`.
/// `head` (attributs et doc internes) va en tête du fichier, seule place permise.
fn replace_include(file: &Path, target: &str, head: &str, replacement: &str) -> Result<String, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{} : {e}", file.display()))?;
    let (start, end) = crust_build::include_statement(&text, target)
        .ok_or_else(|| format!("{} : ligne `include!(…/crust/{target})` introuvable", file.display()))?;
    let head = if head.trim().is_empty() {
        String::new()
    } else {
        format!("{}\n", head.trim_end())
    };
    Ok(format!(
        "{head}{}{}{}",
        &text[..start],
        replacement.trim_end(),
        &text[end..]
    ))
}

/// Sépare les attributs et la doc internes (`//!`, `#![...]`) en tête d'une racine.
fn split_inner(code: &str) -> (String, String) {
    let mut head = String::new();
    let mut rest = code;
    while let Some(line) = rest.split_inclusive('\n').next() {
        let t = line.trim_start();
        if t.starts_with("//!") || t.starts_with("#![") || t.trim().is_empty() {
            head.push_str(line);
            rest = &rest[line.len()..];
        } else {
            break;
        }
    }
    (head, rest.to_string())
}

fn plan(dir: &Path) -> Result<Vec<Action>, String> {
    let src = dir.join("src");
    let p = Project::scan(&src, &crust_build::crate_lib_name(dir))?;
    let mut actions = Vec::new();
    let mut errors = Vec::new();
    let mut natural: BTreeMap<PathBuf, String> = BTreeMap::new();
    for r in &p.rsc {
        let out = crust_build::transpile_rsc_natural(&p, r)?;
        for d in &out.diagnostics {
            errors.push(format!("src/{} : [{}] {}", r.rel.display(), d.code.as_str(), d.message));
        }
        natural.insert(r.rel.clone(), out.code);
    }
    for (rel, d) in &p.dirs {
        errors.extend(d.errors.iter().map(|e| format!("src/{} : {e}", rel.display())));
    }
    if !errors.is_empty() {
        return Err(format!(
            "corrigez d'abord les erreurs Crust :\n  {}",
            errors.join("\n  ")
        ));
    }

    // .rsc non racines → .rs à côté, nommés comme leur module.
    for r in p.rsc.iter().filter(|r| r.root.is_none() && !r.module.is_empty()) {
        let dest = r.abs.with_file_name(format!("{}.rs", r.name));
        if dest.exists() {
            return Err(format!("{} existe déjà", dest.display()));
        }
        actions.push(Action::Write(dest, natural[&r.rel].clone()));
    }
    // Répertoires : un fichier `src/<rép>.rs`, ou le module .rs qui les incluait.
    for (rel, d) in &p.dirs {
        if rel.as_os_str().is_empty() {
            continue;
        }
        match &d.kind {
            DirKind::Generated => {
                let file = src.join(rel).with_extension("rs");
                if file.exists() {
                    return Err(format!("{} existe déjà", file.display()));
                }
                actions.push(Action::Write(file.clone(), decls(&p, rel, &file)));
            }
            DirKind::Hosted => {
                let host = p
                    .rs_modules
                    .iter()
                    .find(|m| m.path == d.module && m.target == p.owner)
                    .and_then(|m| m.file.clone())
                    .ok_or_else(|| format!("module `{}` introuvable", d.module))?;
                let key = format!("dir/{}.rs", rel.to_string_lossy().replace('\\', "/"));
                actions.push(Action::Write(
                    host.clone(),
                    replace_include(&host, &key, "", &decls(&p, rel, &host))?,
                ));
            }
            DirKind::Root | DirKind::Unhosted(_) => {}
        }
    }
    // Racines : déclarations + contenu de main.rsc / lib.rsc à la place de l'include.
    for root in &p.roots {
        let target = root.target;
        let mut replacement = if target == p.owner {
            decls(&p, Path::new(""), &root.rs)
        } else {
            String::new()
        };
        let mut head = String::new();
        if let Some(r) = p.rsc.iter().find(|r| r.root == Some(target)) {
            let (inner, body) = split_inner(&natural[&r.rel]);
            head = inner;
            replacement.push('\n');
            replacement.push_str(&body);
        }
        let text = std::fs::read_to_string(&root.rs).unwrap_or_default();
        if crust_build::include_statement(&text, &root.root_file).is_none() {
            continue; // binaire en pur Rust, sans Crust
        }
        actions.push(Action::Write(
            root.rs.clone(),
            replace_include(&root.rs, &root.root_file, &head, &replacement)?,
        ));
    }
    for r in &p.rsc {
        actions.push(Action::Delete(r.abs.clone()));
    }

    // build.rs et Cargo.toml : plus de crust-build.
    let build = dir.join("build.rs");
    if let Ok(text) = std::fs::read_to_string(&build) {
        let without: String = text
            .lines()
            .filter(|l| !l.contains("crust_build::run"))
            .map(|l| format!("{l}\n"))
            .collect();
        let empty_main = without.split_whitespace().collect::<String>() == "fnmain(){}";
        actions.push(if empty_main {
            Action::Delete(build)
        } else {
            Action::Write(build, without)
        });
    }
    let manifest = dir.join("Cargo.toml");
    let toml = std::fs::read_to_string(&manifest).map_err(|e| e.to_string())?;
    let mut lines: Vec<&str> = toml
        .lines()
        .filter(|l| !l.trim_start().starts_with("crust-build"))
        .collect();
    // Section [build-dependencies] devenue vide.
    if let Some(i) = lines.iter().position(|l| l.trim() == "[build-dependencies]") {
        let empty = lines[i + 1..]
            .iter()
            .take_while(|l| !l.trim_start().starts_with('['))
            .all(|l| l.trim().is_empty());
        if empty {
            lines.remove(i);
        }
    }
    let mut toml = lines.join("\n");
    while toml.contains("\n\n\n") {
        toml = toml.replace("\n\n\n", "\n\n");
    }
    actions.push(Action::Write(manifest, format!("{}\n", toml.trim_end())));
    Ok(actions)
}

fn git_dirty(dir: &Path) -> bool {
    Command::new("git")
        .args(["status", "--porcelain", "--", "."])
        .current_dir(dir)
        .output()
        .is_ok_and(|o| o.status.success() && !o.stdout.is_empty())
}

pub fn eject(args: &[String]) -> ExitCode {
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let force = args.iter().any(|a| a == "--force");
    let dir = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let dir = std::path::absolute(&dir).unwrap_or(dir);
    if !dir.join("Cargo.toml").is_file() {
        eprintln!("erreur : pas de Cargo.toml dans {}", dir.display());
        return ExitCode::FAILURE;
    }
    if !dry_run && !force && git_dirty(&dir) {
        eprintln!(
            "erreur : modifications non committées dans {} ; committez d'abord (ou --force)",
            dir.display()
        );
        return ExitCode::FAILURE;
    }
    let actions = match plan(&dir) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("erreur : {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut to_format = Vec::new();
    for a in &actions {
        match a {
            Action::Write(path, content) => {
                println!("  écrit    {}", rel_to(path, &dir));
                if !dry_run {
                    if let Err(e) = std::fs::write(path, content) {
                        eprintln!("erreur : {} : {e}", path.display());
                        return ExitCode::FAILURE;
                    }
                    if path.extension().is_some_and(|e| e == "rs") {
                        to_format.push(path.clone());
                    }
                }
            }
            Action::Delete(path) => {
                println!("  supprime {}", rel_to(path, &dir));
                if !dry_run && let Err(e) = std::fs::remove_file(path) {
                    eprintln!("erreur : {} : {e}", path.display());
                    return ExitCode::FAILURE;
                }
            }
        }
    }
    if dry_run {
        println!("(--dry-run : rien n'a été modifié)");
        return ExitCode::SUCCESS;
    }
    if !to_format.is_empty() {
        let _ = Command::new("rustfmt")
            .args(["--edition", "2024"])
            .args(&to_format)
            .status();
    }
    println!("Projet converti en Rust standard : vérifiez avec `cargo build && cargo test`.");
    ExitCode::SUCCESS
}
