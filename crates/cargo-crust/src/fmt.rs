//! `cargo crust fmt [--check]` : formate les `.rsc` puis lance `cargo fmt` (section 21).

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde_json::Value;

fn rsc_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rsc_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rsc") {
            out.push(p);
        }
    }
}

/// (répertoire du paquet, édition) pour chaque paquet du workspace courant.
fn packages() -> Vec<(PathBuf, String)> {
    let Ok(out) = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
    else {
        return Vec::new();
    };
    let Ok(meta) = serde_json::from_slice::<Value>(&out.stdout) else {
        return Vec::new();
    };
    meta["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let dir = Path::new(p["manifest_path"].as_str()?).parent()?.to_path_buf();
            Some((dir, p["edition"].as_str().unwrap_or("2021").to_string()))
        })
        .collect()
}

pub fn fmt(args: &[String]) -> ExitCode {
    let check = args.iter().any(|a| a == "--check");
    let mut failed = false;
    for (dir, edition) in packages() {
        let mut files = Vec::new();
        rsc_files(&dir.join("src"), &mut files);
        files.sort();
        for file in files {
            let Ok(src) = std::fs::read_to_string(&file) else {
                continue;
            };
            let shown = file
                .strip_prefix(std::env::current_dir().unwrap_or_default())
                .unwrap_or(&file)
                .display()
                .to_string();
            match crust_syntax::fmt::format(&src, &edition, Some(&dir)) {
                Ok(formatted) if formatted == src => {}
                Ok(_) if check => {
                    println!("à formater : {shown}");
                    failed = true;
                }
                Ok(formatted) => {
                    if let Err(e) = std::fs::write(&file, formatted) {
                        eprintln!("erreur : {shown} : {e}");
                        failed = true;
                    }
                }
                Err(e) => {
                    eprintln!("erreur : {shown} non formaté : {}", e.trim());
                    failed = true;
                }
            }
        }
    }
    // Les `.rs` : rustfmt habituel.
    let mut cargo_fmt = Command::new("cargo");
    cargo_fmt.arg("fmt");
    if check {
        cargo_fmt.arg("--check");
    }
    if !cargo_fmt.status().is_ok_and(|s| s.success()) {
        failed = true;
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
