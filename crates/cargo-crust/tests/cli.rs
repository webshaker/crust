//! `cargo crust` de bout en bout sur de vrais projets (phase P3).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst);
        } else if src.file_name().is_some_and(|n| n == "Cargo.toml") {
            let crust_build = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crust-build");
            let text = std::fs::read_to_string(&src).unwrap();
            std::fs::write(
                &dst,
                text.replace("{{CRUST_BUILD}}", &crust_build.display().to_string()),
            )
            .unwrap();
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

/// Copie fraîche du projet `p1_robot` de crust-build, sous le nom `name`.
fn robot(name: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crust-build/tests/projects/p1_robot");
    let dst = scratch(name);
    copy_dir(&src, &dst);
    // Nom de paquet unique : avec un CARGO_TARGET_DIR partagé, des paquets de même
    // nom partageraient le même OUT_DIR (cargo n'y inclut pas le chemin).
    let manifest = dst.join("Cargo.toml");
    let toml = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        toml.replace("name = \"p1_robot\"", &format!("name = \"robot_{name}\"")),
    )
    .unwrap();
    dst
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    dir
}

fn crust(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cargo-crust"))
        .arg("crust")
        .args(args)
        .current_dir(dir)
        .env(
            "CARGO_TARGET_DIR",
            Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-target"),
        )
        .env("NO_COLOR", "1")
        .env_remove("RUSTFLAGS")
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Ligne (1-based) et colonne (1-based) de `needle` dans `file`.
fn position(file: &Path, needle: &str) -> (usize, usize) {
    let src = std::fs::read_to_string(file).unwrap();
    for (i, line) in src.lines().enumerate() {
        if let Some(c) = line.find(needle) {
            return (i + 1, line[..c].chars().count() + 1);
        }
    }
    panic!("{needle} absent de {}", file.display());
}

fn inject_errors(dir: &Path) {
    let p = dir.join("src/robot/Moteur.rsc");
    let s = std::fs::read_to_string(&p)
        .unwrap()
        .replace(
            "    pub vitesse: u32,",
            "    pub vitesse: u32,\n    pub couple: Inconnu,",
        )
        .replace(
            "        self.vitesse += delta;",
            "        self.vitesse += delta;\n        self.vitesse = \"abc\";",
        );
    std::fs::write(p, s).unwrap();
}

#[test]
fn erreurs_ramenees_dans_le_rsc() {
    let dir = robot("erreurs");
    inject_errors(&dir);
    let out = crust(&dir, &["check"]);
    let stderr = text(&out.stderr);
    assert!(!out.status.success());
    let file = dir.join("src/robot/Moteur.rsc");
    let (l, c) = position(&file, "\"abc\"");
    assert!(
        stderr.contains(&format!("--> src/robot/Moteur.rsc:{l}:{c}")),
        "{stderr}"
    );
    assert!(stderr.contains("self.vitesse = \"abc\";"), "{stderr}");
    // Erreur située dans la `struct` générée : ramenée sur le champ.
    let (l, c) = position(&file, "Inconnu");
    assert!(
        stderr.contains(&format!("--> src/robot/Moteur.rsc:{l}:{c}")),
        "{stderr}"
    );
    assert!(!stderr.contains("/out/crust/"), "chemin généré visible :\n{stderr}");
}

#[test]
fn json_remappe_pour_les_outils() {
    let dir = robot("json");
    inject_errors(&dir);
    let out = crust(&dir, &["check", "--message-format=json"]);
    let file = dir.join("src/robot/Moteur.rsc");
    let (line, col) = position(&file, "\"abc\"");
    let mut found = false;
    for l in text(&out.stdout).lines() {
        let v: serde_json::Value = serde_json::from_str(l).expect("chaque ligne de stdout est du JSON");
        if v["reason"] != "compiler-message" || v["message"]["code"]["code"] != "E0308" {
            continue;
        }
        let span = v["message"]["spans"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["is_primary"] == true)
            .unwrap();
        assert_eq!(span["file_name"], "src/robot/Moteur.rsc");
        assert_eq!(
            (span["line_start"].as_u64(), span["column_start"].as_u64()),
            (Some(line as u64), Some(col as u64))
        );
        assert!(
            v["message"]["rendered"]
                .as_str()
                .unwrap()
                .contains("src/robot/Moteur.rsc")
        );
        found = true;
    }
    assert!(found, "{}", text(&out.stdout));
}

#[test]
fn run_et_test_transmettent_la_sortie() {
    let dir = robot("run");
    let out = crust(&dir, &["run", "-q"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        std::fs::read_to_string(dir.join("expected.stdout")).unwrap()
    );

    std::fs::write(
        dir.join("src/robot/Moteur.rsc"),
        std::fs::read_to_string(dir.join("src/robot/Moteur.rsc")).unwrap()
            + "\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn accelere() {\n        let mut m = super::Moteur::new();\n        m.accelerer(5);\n        assert_eq!(m.vitesse, 5);\n    }\n}\n",
    )
    .unwrap();
    let out = crust(&dir, &["test"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stdout).contains("test robot::moteur::tests::accelere ... ok"),
        "{}",
        text(&out.stdout)
    );
}

#[test]
fn new_et_init() {
    // Hors du dépôt : dans un workspace, `cargo new` ajoute le projet à ses `members`.
    let parent = std::env::temp_dir().join(format!("crust-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&parent);
    std::fs::create_dir_all(&parent).unwrap();
    let dir = parent.join("nouveau");
    let out = crust(&parent, &["new", "--vcs", "none", "nouveau"]);
    assert!(out.status.success(), "{}{}", text(&out.stdout), text(&out.stderr));
    std::fs::write(dir.join("src/Salut.rsc"), "pub class Salut { pub nom: String, }\n").unwrap();
    std::fs::write(
        dir.join("src/main.rsc"),
        "import Salut from \"./Salut.rsc\";\n\nfn main() {\n    println!(\"Bonjour {}\", Salut { nom: \"Crust\".into() }.nom);\n}\n",
    )
    .unwrap();
    let out = crust(&dir, &["run", "-q"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "Bonjour Crust\n");

    // `init` est idempotent.
    let out = crust(&dir, &["init"]);
    assert!(text(&out.stdout).contains("déjà installé"), "{}", text(&out.stdout));
    assert_eq!(
        std::fs::read_to_string(dir.join("src/main.rs")).unwrap(),
        "include!(concat!(env!(\"OUT_DIR\"), \"/crust/root_main.rs\"));\n"
    );
    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn explain() {
    let out = crust(Path::new("."), &["explain", "cr0002"]);
    assert!(out.status.success());
    assert!(text(&out.stdout).contains("over Moteur(base)"));
}

/// Copie d'un projet de crust-build sous un nom de répertoire `name`.
fn fixture(project: &str, name: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../crust-build/tests/projects")
        .join(project);
    let dst = scratch(name);
    copy_dir(&src, &dst);
    dst
}

fn cargo_plain(dir: &Path, args: &[&str]) -> Output {
    Command::new("cargo")
        .args(args)
        .current_dir(dir)
        .env(
            "CARGO_TARGET_DIR",
            Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-eject-target"),
        )
        .env_remove("RUSTFLAGS")
        .output()
        .unwrap()
}

fn assert_no_crust(dir: &Path) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                assert!(p.extension().is_none_or(|x| x != "rsc"), "reste {}", p.display());
                if p.file_name().is_some_and(|n| n == "Cargo.toml" || n == "build.rs")
                    || p.extension().is_some_and(|x| x == "rs")
                {
                    let text = std::fs::read_to_string(&p).unwrap();
                    assert!(
                        !text.contains("crust"),
                        "{} mentionne encore crust :\n{text}",
                        p.display()
                    );
                }
            }
        }
    }
}

/// T20 : `eject` donne un projet Rust standard qui se comporte à l'identique.
#[test]
fn eject_produit_du_rust_standard() {
    // p1 : racine main.rsc, .rs importé, `mod util;` avec sous-module.
    let dir = fixture("p1_robot", "eject_p1");
    let out = crust(&dir, &["eject"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_no_crust(&dir);
    let run = cargo_plain(&dir, &["run", "-q"]);
    assert!(run.status.success(), "{}", text(&run.stderr));
    assert_eq!(
        text(&run.stdout),
        std::fs::read_to_string(dir.join("expected.stdout")).unwrap()
    );

    // p2 : lib + bin en .rsc, utilisée par une crate Rust ordinaire.
    let ws = fixture("p2_workspace", "eject_p2");
    let out = crust(&ws.join("robotlib"), &["eject"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_no_crust(&ws);
    let run = cargo_plain(&ws, &["run", "-q", "-p", "robot-lib", "--bin", "robot-lib"]);
    assert_eq!(text(&run.stdout), "bin : meme moteur = true\n", "{}", text(&run.stderr));
    let run = cargo_plain(&ws, &["run", "-q", "-p", "app"]);
    assert_eq!(text(&run.stdout), "app : moteur 7\n", "{}", text(&run.stderr));
    let run = cargo_plain(&ws, &["run", "-q", "-p", "robot-lib", "--bin", "outil"]);
    assert_eq!(text(&run.stdout), "outil : moteur 7\n", "{}", text(&run.stderr));

    // p5 : dossier privé, `#![cfg]` de tête, include_str relatif, import de dossier.
    let dir = fixture("p5_retours", "eject_p5");
    let out = crust(&dir, &["eject"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_no_crust(&dir);
    let run = cargo_plain(&dir, &["run", "-q", "--features", "capture"]);
    assert_eq!(text(&run.stdout), "bonjour 4 7\ncapture\n", "{}", text(&run.stderr));
    let clippy = cargo_plain(&dir, &["clippy", "-q", "--all-targets", "--", "-D", "warnings"]);
    assert!(clippy.status.success(), "{}", text(&clippy.stderr));

    // p3 : projet Rust existant, répertoire mixte (include dans outils.rs).
    let dir = fixture("p3_mixed", "eject_p3");
    let out = crust(&dir, &["eject"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_no_crust(&dir);
    let run = cargo_plain(&dir, &["run", "-q"]);
    assert_eq!(
        text(&run.stdout),
        std::fs::read_to_string(dir.join("expected.stdout")).unwrap(),
        "{}",
        text(&run.stderr)
    );
}

#[test]
fn fmt_formate_les_rsc() {
    let dir = robot("fmt");
    let moteur = dir.join("src/robot/Moteur.rsc");
    let messy = std::fs::read_to_string(&moteur)
        .unwrap()
        .replace("    pub vitesse: u32,", "  pub vitesse :u32,");
    std::fs::write(&moteur, &messy).unwrap();

    let out = crust(&dir, &["fmt", "--check"]);
    assert!(!out.status.success());
    assert!(
        text(&out.stdout).contains("src/robot/Moteur.rsc"),
        "{}",
        text(&out.stdout)
    );
    assert_eq!(
        std::fs::read_to_string(&moteur).unwrap(),
        messy,
        "--check ne modifie rien"
    );

    let out = crust(&dir, &["fmt"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        std::fs::read_to_string(&moteur)
            .unwrap()
            .contains("    pub vitesse: u32,")
    );
    assert!(crust(&dir, &["fmt", "--check"]).status.success());
    let run = crust(&dir, &["run", "-q"]);
    assert_eq!(
        text(&run.stdout),
        std::fs::read_to_string(dir.join("expected.stdout")).unwrap()
    );
}
