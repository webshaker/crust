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
    let dir = scratch("nouveau");
    let parent = dir.parent().unwrap();
    let out = crust(parent, &["new", "--vcs", "none", "nouveau"]);
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
}

#[test]
fn explain() {
    let out = crust(Path::new("."), &["explain", "cr0002"]);
    assert!(out.status.success());
    assert!(text(&out.stdout).contains("over Moteur(base)"));
}
