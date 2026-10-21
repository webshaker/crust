//! `cargo crust fmt` : stabilité, sens préservé, rendu attendu (section 21).

mod common;

use common::*;
use crust_syntax::fmt::format;
use crust_syntax::{Layout, NoResolver, transpile};

fn rustfmt(code: &str) -> String {
    use std::io::Write;
    let mut child = std::process::Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(code.as_bytes()).unwrap();
    String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap()
}

/// Formater deux fois donne le même texte, et le Rust généré ne change pas
/// (au formatage près) : le formatage ne touche jamais au sens.
#[test]
fn stable_et_sans_effet_sur_le_sens() {
    let mut failures = Vec::new();
    for path in fixtures("transpile").into_iter().chain(fixtures("fmt")) {
        let src = read(&path);
        if src.contains("//@ no-compile") {
            continue;
        }
        let once = match format(&src, "2024", None) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{} : {e}", path.display()));
                continue;
            }
        };
        let twice = format(&once, "2024", None).unwrap();
        if once != twice {
            failures.push(format!(
                "{} : pas stable\n--- 1\n{once}\n--- 2\n{twice}",
                path.display()
            ));
        }
        let before = rustfmt(&transpile(&src, Layout::Natural, &NoResolver).code);
        let after = rustfmt(&transpile(&once, Layout::Natural, &NoResolver).code);
        if strip_comments(&before) != strip_comments(&after) {
            failures.push(format!(
                "{} : le Rust généré change\n--- avant\n{before}\n--- après\n{after}",
                path.display()
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

/// Les commentaires entre champs ne sont pas recopiés dans la disposition naturelle.
fn strip_comments(code: &str) -> String {
    code.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .map(|l| l.split(" //").next().unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn rendu_attendu() {
    for path in fixtures("fmt") {
        let out = format(&read(&path), "2024", None).unwrap();
        if let Err(e) = snapshot(&path.with_extension("attendu"), &out) {
            panic!("{e}");
        }
    }
}

#[test]
fn refuse_un_fichier_avec_erreurs() {
    assert!(format("pub class A over B, B {}\n", "2024", None).is_err());
}
