//! Résolution des `import` sur un faux projet (section 13, erreurs CR0006–CR0013).

mod common;

use std::path::Path;

use common::read;
use crust_syntax::{Code, FsResolver, Layout, transpile};

#[test]
fn imports_du_projet() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/project/src/robot/Servomoteur.rsc");
    let src = read(&file);
    let resolver = FsResolver::for_file(&file).unwrap();
    let out = transpile(&src, Layout::Lines, &resolver);
    let lines: Vec<&str> = out.code.lines().map(str::trim_end).collect();

    assert_eq!(lines[0], "use crate::robot::moteur::Moteur;");
    assert_eq!(lines[1], "use crate::robot::moteur::{Etat as EtatMoteur, creer};");
    assert_eq!(lines[2], "use crate::capteur::Capteur;");
    assert_eq!(lines[3], "pub use crate::robot::moteur::*;");

    let codes: Vec<(Code, usize)> = out
        .diagnostics
        .iter()
        .map(|d| (d.code, line_of(&src, d.span.start)))
        .collect();
    assert_eq!(
        codes,
        vec![
            (Code::CR0006, 5),  // fichier absent
            (Code::CR0007, 6),  // symbole absent
            (Code::CR0008, 7),  // .rsc et .rs
            (Code::CR0012, 8),  // nom de répertoire invalide
            (Code::CR0013, 9),  // hors de src/
            (Code::CR0006, 10), // préfixe manquant
        ]
    );
    let cr0007 = &out.diagnostics[1];
    assert_eq!(&src[cr0007.span.start..cr0007.span.end], "Moteurr");
    assert_eq!(cr0007.help.as_deref(), Some("vouliez-vous dire `Moteur` ?"));
    for (i, d) in out.diagnostics.iter().enumerate() {
        let line = lines[line_of(&src, d.span.start) - 1];
        assert!(line.starts_with("compile_error!(\"[CR"), "diagnostic {i} : {line}");
    }
}

#[test]
fn racine_de_crate() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/project/src/main.rsc");
    let out = transpile(&read(&file), Layout::Lines, &FsResolver::for_file(&file).unwrap());
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert!(out.code.starts_with("use crate::robot::servomoteur::Servomoteur;"));
}

fn line_of(src: &str, offset: usize) -> usize {
    src[..offset].matches('\n').count() + 1
}
