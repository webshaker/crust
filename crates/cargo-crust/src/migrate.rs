//! `cargo crust migrate` : convertit un projet Rust existant vers Crust.
//!
//! Pour chaque module `.rs` monté par un simple `mod x;` :
//! 1. `struct` + `impl` → `class` (cas sans ambiguïté, `crust_syntax::migrate`) ;
//! 2. `x.rs` devient `x.rsc`, et son `mod x;` est retiré du module parent ;
//! 3. un module qui n'était pas `pub` est ajouté à `[package.metadata.crust] private`.
//!
//! `cargo check --all-targets` doit passer avant et après ; sinon, tout est restauré.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};

use crust_build::Project;

/// Contenus d'origine des fichiers touchés (`None` : le fichier n'existait pas).
#[derive(Default)]
struct Snapshot(BTreeMap<PathBuf, Option<String>>);

impl Snapshot {
    fn keep(&mut self, path: &Path) {
        self.0
            .entry(path.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(path).ok());
    }

    fn restore(&self) {
        for (path, content) in &self.0 {
            match content {
                Some(c) => {
                    let _ = std::fs::write(path, c);
                }
                None => {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }
}

struct Conversion {
    file: PathBuf,
    rsc: PathBuf,
    classes: Vec<String>,
    host: PathBuf,
    decl: crust_syntax::FileModInfo,
    /// Dossier du `.rsc`, relatif à `src/`.
    rel_dir: PathBuf,
}

fn cargo_check(dir: &Path) -> Output {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["check", "--all-targets", "--message-format=short"])
        .current_dir(dir)
        .output()
        .expect("cargo introuvable")
}

fn tail(bytes: &[u8], lines: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    let all: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim_start().starts_with("Checking") && !l.trim_start().starts_with("Compiling"))
        .collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// Modules convertibles : fichiers `.rs` montés par un `mod x;` simple.
fn plan(p: &Project, only: &BTreeSet<PathBuf>) -> Vec<Conversion> {
    let mut out = Vec::new();
    let roots: Vec<PathBuf> = p.roots.iter().map(|r| r.rs.clone()).collect();
    for m in &p.rs_modules {
        let Some(file) = &m.file else { continue };
        if m.target != p.owner || roots.contains(file) || file.file_name().is_some_and(|n| n == "mod.rs") {
            continue;
        }
        if p.rs_modules.iter().filter(|o| o.file.as_ref() == Some(file)).count() > 1 {
            continue; // monté deux fois
        }
        let Ok(rel) = file.strip_prefix(p.src.canonicalize().unwrap_or(p.src.clone())) else {
            continue;
        };
        if rel.starts_with("bin") || (!only.is_empty() && !only.contains(file)) {
            continue;
        }
        // Le module parent : la racine de la cible, ou le module .rs de chemin `parent`.
        let Some((parent, name)) = m.path.rsplit_once("::") else {
            continue;
        };
        let host = if parent == "crate" {
            p.root(m.target).map(|r| r.rs.clone())
        } else {
            p.rs_modules
                .iter()
                .find(|o| o.path == parent && o.target == m.target)
                .and_then(|o| o.file.clone())
        };
        let Some(host) = host else { continue };
        let Ok(host_text) = std::fs::read_to_string(&host) else {
            continue;
        };
        let Some(decl) = crust_syntax::file_mods(&host_text)
            .into_iter()
            .find(|d| d.name == name && d.inline.is_empty() && d.attrs.is_empty())
        else {
            continue; // `#[path]`, `#[cfg]`… ou module inline : on ne touche pas
        };
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        let migrated = crust_syntax::migrate::structs_to_classes(&text);
        if only.is_empty() && migrated.classes.is_empty() {
            continue;
        }
        out.push(Conversion {
            file: file.clone(),
            rsc: file.with_extension("rsc"),
            classes: migrated.classes,
            host,
            decl,
            rel_dir: rel.parent().unwrap_or(Path::new("")).to_path_buf(),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    out
}

/// Ajoute des entrées à `[package.metadata.crust] private`.
fn add_private(manifest: &Path, entries: &[String]) -> Result<(), String> {
    let toml = std::fs::read_to_string(manifest).map_err(|e| e.to_string())?;
    let list = entries.iter().map(|e| format!("{e:?}")).collect::<Vec<_>>().join(", ");
    let new = match toml.find("[package.metadata.crust]") {
        Some(i) => {
            let section_end = toml[i + 1..].find("\n[").map_or(toml.len(), |j| i + 1 + j);
            let section = &toml[i..section_end];
            match section.find("private") {
                Some(k) => {
                    let at = i + k;
                    let close = at + toml[at..].find(']').ok_or("liste `private` mal formée")?;
                    let before = toml[..close].trim_end();
                    let sep = if before.ends_with('[') { "" } else { ", " };
                    format!("{before}{sep}{list}{}", &toml[close..])
                }
                None => {
                    let header_end = i + "[package.metadata.crust]".len();
                    format!("{}\nprivate = [{list}]{}", &toml[..header_end], &toml[header_end..])
                }
            }
        }
        None => format!(
            "{}\n\n[package.metadata.crust]\n# Modules qui n'étaient pas `pub` avant la migration.\nprivate = [{list}]\n",
            toml.trim_end()
        ),
    };
    std::fs::write(manifest, new).map_err(|e| e.to_string())
}

pub fn migrate(args: &[String]) -> ExitCode {
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let force = args.iter().any(|a| a == "--force");
    let only: BTreeSet<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .filter_map(|a| std::fs::canonicalize(a).ok())
        .collect();
    let dir = std::env::current_dir().unwrap_or_default();
    if !dir.join("Cargo.toml").is_file() {
        eprintln!("erreur : lancer `cargo crust migrate` à la racine du paquet (Cargo.toml)");
        return ExitCode::FAILURE;
    }
    let dirty = Command::new("git")
        .args(["status", "--porcelain", "--", "."])
        .output()
        .is_ok_and(|o| o.status.success() && !o.stdout.is_empty());
    if dirty && !dry_run && !force {
        eprintln!("erreur : modifications non committées ; committez d'abord (ou --force)");
        return ExitCode::FAILURE;
    }

    // Plan (sur l'état actuel ; Crust n'a pas besoin d'être installé pour le calculer).
    let lib_name = crust_build::crate_lib_name(&dir);
    let project = match Project::scan(&dir.join("src"), &lib_name) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("erreur : {e}");
            return ExitCode::FAILURE;
        }
    };
    let conversions = plan(&project, &only);
    if conversions.is_empty() {
        println!("Rien à convertir (aucun module `.rs` monté par `mod x;` avec struct + impl).");
        return ExitCode::SUCCESS;
    }
    for c in &conversions {
        let shown = c.file.strip_prefix(&dir).unwrap_or(&c.file).display().to_string();
        let classes = if c.classes.is_empty() {
            "aucune classe".to_string()
        } else {
            c.classes.join(", ")
        };
        println!("  {shown} → .rsc ({classes})");
    }
    if dry_run {
        println!("(--dry-run : rien n'a été modifié)");
        return ExitCode::SUCCESS;
    }

    let before = cargo_check(&dir);
    if !before.status.success() {
        eprintln!(
            "erreur : le projet doit compiler avant la migration :\n{}",
            tail(&before.stderr, 15)
        );
        return ExitCode::FAILURE;
    }

    let mut snap = Snapshot::default();
    let manifest = dir.join("Cargo.toml");
    for f in ["Cargo.toml", "build.rs", "src/main.rs", "src/lib.rs"] {
        snap.keep(&dir.join(f));
    }
    if let Ok(o) = Command::new("cargo")
        .args(["locate-project", "--workspace", "--message-format", "plain"])
        .output()
    {
        snap.keep(Path::new(String::from_utf8_lossy(&o.stdout).trim()));
    }
    for c in &conversions {
        snap.keep(&c.file);
        snap.keep(&c.rsc);
        snap.keep(&c.host);
    }

    let result = (|| -> Result<(), String> {
        crate::init::init_package(&dir)?;
        let edition = std::fs::read_to_string(&manifest)
            .ok()
            .and_then(|t| {
                t.lines().find_map(|l| {
                    l.trim()
                        .strip_prefix("edition")
                        .map(|v| v.trim_start_matches([' ', '=', '"']).trim_end_matches('"').to_string())
                })
            })
            .unwrap_or_else(|| "2021".into());
        let mut hosts: BTreeMap<PathBuf, (String, Vec<crust_syntax::Span>, Option<PathBuf>)> = BTreeMap::new();
        let mut private = Vec::new();
        let mut empty_hosts: Vec<PathBuf> = Vec::new();
        for c in &conversions {
            let text = std::fs::read_to_string(&c.file).map_err(|e| e.to_string())?;
            let migrated = crust_syntax::migrate::structs_to_classes(&text).text;
            let formatted = crust_syntax::fmt::format(&migrated, &edition, Some(&dir)).unwrap_or(migrated);
            std::fs::write(&c.rsc, formatted).map_err(|e| e.to_string())?;
            std::fs::remove_file(&c.file).map_err(|e| e.to_string())?;
            let host_is_root = project.roots.iter().any(|r| r.rs == c.host);
            let entry = hosts
                .entry(c.host.clone())
                .or_insert_with(|| (std::fs::read_to_string(&c.host).unwrap_or_default(), Vec::new(), None));
            entry.1.push(c.decl.span);
            if !host_is_root {
                entry.2 = Some(c.rel_dir.clone());
            }
            if c.decl.vis.as_deref() != Some("pub") {
                private.push(
                    c.rel_dir
                        .join(c.file.file_stem().unwrap_or_default())
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
        // Modules parents : retrait des `mod x;`, et ligne d'inclusion du dossier.
        for (host, (text, mut spans, dir_rel)) in hosts {
            spans.sort_by_key(|s| std::cmp::Reverse(s.start));
            let mut text = text;
            for s in spans {
                let end = if text[s.end..].starts_with('\n') {
                    s.end + 1
                } else {
                    s.end
                };
                text.replace_range(s.start..end, "");
            }
            if let Some(rel) = dir_rel {
                if text.trim().is_empty() {
                    // Module qui ne faisait que déclarer des sous-modules : son dossier
                    // devient un dossier Crust ordinaire, sans fichier de module.
                    empty_hosts.push(host.clone());
                    std::fs::remove_file(&host).map_err(|e| e.to_string())?;
                    continue;
                }
                let key = format!("dir/{}.rs", rel.to_string_lossy().replace('\\', "/"));
                if crust_build::include_statement(&text, &key).is_none() {
                    text = format!(
                        "{}\n\ninclude!(concat!(env!(\"OUT_DIR\"), \"/crust/{key}\"));\n",
                        text.trim_end()
                    );
                }
            }
            std::fs::write(&host, text).map_err(|e| e.to_string())?;
        }
        // Leur `mod robot;` disparaît aussi de leur propre parent.
        for host in &empty_hosts {
            let Some(m) = project.rs_modules.iter().find(|m| m.file.as_ref() == Some(host)) else {
                continue;
            };
            let Some((parent, name)) = m.path.rsplit_once("::") else {
                continue;
            };
            let parent_file = if parent == "crate" {
                project.root(m.target).map(|r| r.rs.clone())
            } else {
                project
                    .rs_modules
                    .iter()
                    .find(|o| o.path == parent)
                    .and_then(|o| o.file.clone())
            };
            let Some(parent_file) = parent_file else { continue };
            snap.keep(&parent_file);
            let text = std::fs::read_to_string(&parent_file).map_err(|e| e.to_string())?;
            let Some(decl) = crust_syntax::file_mods(&text)
                .into_iter()
                .find(|d| d.name == name && d.inline.is_empty() && d.attrs.is_empty())
            else {
                return Err(format!("`mod {name};` introuvable dans {}", parent_file.display()));
            };
            let end = if text[decl.span.end..].starts_with('\n') {
                decl.span.end + 1
            } else {
                decl.span.end
            };
            let mut text = text;
            text.replace_range(decl.span.start..end, "");
            std::fs::write(&parent_file, text).map_err(|e| e.to_string())?;
            if decl.vis.as_deref() != Some("pub") {
                let rel = host.with_extension("");
                let rel = rel
                    .strip_prefix(project.src.canonicalize().unwrap_or(project.src.clone()))
                    .unwrap_or(&rel);
                private.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
        if !private.is_empty() {
            add_private(&manifest, &private)?;
        }
        Ok(())
    })();

    if let Err(e) = result {
        snap.restore();
        eprintln!("erreur : migration interrompue, tout est restauré : {e}");
        return ExitCode::FAILURE;
    }
    let after = cargo_check(&dir);
    if !after.status.success() {
        snap.restore();
        eprintln!(
            "erreur : le projet ne compile plus après conversion ; tout est restauré.\n{}",
            tail(&after.stderr, 25)
        );
        return ExitCode::FAILURE;
    }
    println!(
        "{} module(s) converti(s). Vérifiez avec `cargo crust test`, puis committez.",
        conversions.len()
    );
    ExitCode::SUCCESS
}
