//! Snapshots de traduction (section 27.1), invariants, et compilation des sorties.

mod common;

use common::*;
use crust_syntax::{Layout, NoResolver, transpile};

#[test]
fn snapshots_et_invariants() {
    let mut failures = Vec::new();
    for path in fixtures("transpile") {
        let src = read(&path);
        let lines = transpile(&src, Layout::Lines, &NoResolver);
        let natural = transpile(&src, Layout::Natural, &NoResolver);
        let mut diags = String::new();
        for d in &lines.diagnostics {
            diags += &format!("{} {}..{} {}\n", d.code.as_str(), d.span.start, d.span.end, d.message);
        }
        for (ext, content) in [
            ("lines.rs", &lines.code),
            ("natural.rs", &natural.code),
            ("diags.txt", &diags),
        ] {
            if let Err(e) = snapshot(&path.with_extension(ext), content) {
                failures.push(e);
            }
        }
        if let Err(e) = check_invariants(&src, &lines) {
            failures.push(format!("{} : {e}", path.display()));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

/// Chaque fixture sans `//@ no-compile` doit compiler avec rustc, dans les deux dispositions.
#[test]
fn les_sorties_compilent() {
    let dir = scratch_dir("compile-transpile");
    let mut failures = Vec::new();
    for path in fixtures("transpile") {
        let src = read(&path);
        if src.contains("//@ no-compile") {
            continue;
        }
        for layout in [Layout::Lines, Layout::Natural] {
            let out = transpile(&src, layout, &NoResolver);
            assert!(out.diagnostics.is_empty(), "{}: {:?}", path.display(), out.diagnostics);
            let gen_path = dir.join(path.file_name().unwrap());
            std::fs::write(&gen_path, &out.code).unwrap();
            let r = rustc(&gen_path, "lib", &dir, false);
            if !r.success {
                failures.push(format!("{} ({layout:?}) :\n{}", path.display(), r.stderr));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// C1 sur un fichier contenant les mots `class`, `over`, `import`, `from` comme identifiants.
#[test]
fn mots_cles_contextuels_inchanges() {
    let path = fixtures("transpile")
        .into_iter()
        .find(|p| p.ends_with("t10_contextual.rsc"))
        .unwrap();
    let src = read(&path);
    let out = transpile(&src, Layout::Lines, &NoResolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.code, src);
}
