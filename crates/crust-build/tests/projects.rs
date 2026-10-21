//! Vrais projets Cargo compilés avec un simple `cargo build` (phase P2,
//! critères T1–T7, T12, T14 et C3/C6 du cahier des charges).
//!
//! Chaque projet de `tests/projects/` est copié dans un répertoire temporaire ;
//! `{{CRUST_BUILD}}` y est remplacé par le chemin de ce crate.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst);
        } else if src.file_name().is_some_and(|n| n == "Cargo.toml") {
            let crust_build = env!("CARGO_MANIFEST_DIR");
            std::fs::write(
                &dst,
                std::fs::read_to_string(&src)
                    .unwrap()
                    .replace("{{CRUST_BUILD}}", crust_build),
            )
            .unwrap();
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

/// Copie fraîche du projet `name`.
fn project(name: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/projects").join(name);
    let dst = Path::new(env!("CARGO_TARGET_TMPDIR")).join("projects").join(name);
    let _ = std::fs::remove_dir_all(&dst);
    copy_dir(&src, &dst);
    dst
}

fn cargo(dir: &Path, args: &[&str]) -> Output {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(args)
        .current_dir(dir)
        // Cible partagée : crust-build n'est compilé qu'une fois pour tous les projets.
        .env(
            "CARGO_TARGET_DIR",
            Path::new(env!("CARGO_TARGET_TMPDIR")).join("projects-target"),
        )
        .env_remove("RUSTFLAGS")
        .output()
        .expect("cargo introuvable")
}

fn stdout_ok(dir: &Path, args: &[&str]) -> String {
    let out = cargo(dir, args);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "cargo {args:?} a échoué dans {} :\n{stderr}",
        dir.display()
    );
    String::from_utf8(out.stdout).unwrap()
}

fn expected(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("expected.stdout")).unwrap()
}

#[test]
fn p1_robot_exemple_du_cahier_des_charges() {
    let dir = project("p1_robot");
    assert_eq!(stdout_ok(&dir, &["run", "-q"]), expected(&dir));

    // Sans modification : le build script n'est pas relancé.
    let out = cargo(&dir, &["build", "-v"]);
    let log = String::from_utf8_lossy(&out.stderr);
    assert!(
        !log.contains("build-script-build`"),
        "build script relancé sans changement :\n{log}"
    );

    // Modification d'un .rsc : prise en compte au build suivant.
    let main = dir.join("src/main.rsc");
    let text = std::fs::read_to_string(&main)
        .unwrap()
        .replace("version 1", "version 2");
    std::fs::write(&main, text).unwrap();
    assert!(stdout_ok(&dir, &["run", "-q"]).ends_with("version 2\n"));

    // Nouveau fichier .rsc : détecté et monté.
    std::fs::write(dir.join("src/robot/Nouveau.rsc"), "pub class Nouveau { pub x: u8, }\n").unwrap();
    std::fs::write(&main, std::fs::read_to_string(&main).unwrap().replace(
        "fn message() -> &'static str {",
        "fn _nouveau() -> crate::robot::Nouveau { crate::robot::Nouveau { x: 1 } }\n\nfn message() -> &'static str {",
    ))
    .unwrap();
    stdout_ok(&dir, &["build", "-q"]);
}

#[test]
fn p2_lib_bin_et_crate_rust_ordinaire() {
    let dir = project("p2_workspace");
    assert_eq!(
        stdout_ok(&dir, &["run", "-q", "-p", "robot-lib", "--bin", "robot-lib"]),
        "bin : meme moteur = true\n"
    );
    assert_eq!(stdout_ok(&dir, &["run", "-q", "-p", "app"]), "app : moteur 7\n");
    // Binaire supplémentaire src/bin/outil.rsc.
    assert_eq!(
        stdout_ok(&dir, &["run", "-q", "-p", "robot-lib", "--bin", "outil"]),
        "outil : moteur 7\n"
    );
}

#[test]
fn p3_projet_rust_existant_avec_fichiers_crust() {
    let dir = project("p3_mixed");
    assert_eq!(stdout_ok(&dir, &["run", "-q"]), expected(&dir));
}

#[test]
fn p4_erreurs_de_modules() {
    let dir = project("p4_errors");
    let out = cargo(&dir, &["build", "-q"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(
        stderr.contains("[CR0009]") && stderr.contains("Moteur.rsc") && stderr.contains("moteur.rsc"),
        "{stderr}"
    );
    assert!(
        stderr.contains("[CR0010]") && stderr.contains("/crust/dir/robot.rs"),
        "{stderr}"
    );
    assert!(
        stderr.contains("[CR0011]") && stderr.contains("crate::a, crate::b"),
        "{stderr}"
    );
    assert!(
        stderr.contains("[CR0015]") && stderr.contains("main.rsc:1:1"),
        "{stderr}"
    );
}
