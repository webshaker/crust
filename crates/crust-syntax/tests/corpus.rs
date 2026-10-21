//! Garantie C1 (section 27.4) : tout fichier Rust valide, traité comme `.rsc`,
//! ressort octet pour octet identique et sans diagnostic.
//!
//! Corpus : sources de la bibliothèque standard (`rustup component add rust-src`),
//! crates du registre cargo local, et répertoires listés dans `CRUST_CORPUS`
//! (séparés par `:`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use crust_syntax::{Layout, NoResolver, transpile_without_shortcut};

fn corpus_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(out) = Command::new("rustc").args(["--print", "sysroot"]).output() {
        let sysroot = String::from_utf8_lossy(&out.stdout).trim().to_string();
        roots.push(Path::new(&sysroot).join("lib/rustlib/src/rust/library"));
    }
    if let Some(home) = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".cargo")))
    {
        roots.push(home.join("registry/src"));
    }
    if let Some(extra) = std::env::var_os("CRUST_CORPUS") {
        roots.extend(std::env::split_paths(&extra));
    }
    roots.into_iter().filter(|r| r.is_dir()).collect()
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn corpus_identique() {
    let roots = corpus_roots();
    let mut files = Vec::new();
    for r in &roots {
        walk(r, &mut files);
    }
    if files.is_empty() {
        eprintln!("corpus C1 : aucun fichier trouvé (installer rust-src), test ignoré");
        return;
    }
    let mut failures = Vec::new();
    let mut bytes = 0usize;
    let mut checked = 0usize;
    let start = Instant::now();
    for f in &files {
        let Ok(src) = std::fs::read_to_string(f) else { continue };
        bytes += src.len();
        checked += 1;
        let out = transpile_without_shortcut(&src, Layout::Lines, &NoResolver);
        if out.code != src || !out.diagnostics.is_empty() {
            let line = src
                .lines()
                .zip(out.code.lines())
                .position(|(a, b)| a != b)
                .map_or(0, |l| l + 1);
            failures.push(format!(
                "{} (ligne {line}) : {:?}",
                f.display(),
                out.diagnostics.first().map(|d| &d.message)
            ));
        }
    }
    let secs = start.elapsed().as_secs_f64();
    eprintln!(
        "corpus C1 : {checked} fichiers, {:.1} Mo, {:.2} s ({:.0} Mo/s, build {}) — racines : {roots:?}",
        bytes as f64 / 1e6,
        secs,
        bytes as f64 / 1e6 / secs,
        if cfg!(debug_assertions) { "debug" } else { "release" },
    );
    assert!(
        failures.is_empty(),
        "{} fichier(s) modifié(s) :\n{}",
        failures.len(),
        failures.join("\n")
    );
}
