//! Le transpileur ne doit jamais paniquer, même sur du code tronqué (section 27.7),
//! et la disposition préservant les lignes doit garder ses invariants.

mod common;

use common::*;
use crust_syntax::{Layout, NoResolver, transpile_without_shortcut};

#[test]
fn prefixes_et_suffixes() {
    let mut failures = Vec::new();
    let mut count = 0;
    for dir in ["transpile", "diagnostics"] {
        for path in fixtures(dir) {
            let src = read(&path);
            let cuts: Vec<usize> = (0..=src.len()).filter(|&i| src.is_char_boundary(i)).collect();
            for &i in &cuts {
                for piece in [&src[..i], &src[i..]] {
                    count += 1;
                    let out = transpile_without_shortcut(piece, Layout::Lines, &NoResolver);
                    if let Err(e) = check_invariants(piece, &out) {
                        failures.push(format!("{} coupé en {i} : {e}", path.display()));
                    }
                    let _ = transpile_without_shortcut(piece, Layout::Natural, &NoResolver);
                }
            }
        }
    }
    assert!(count > 1000);
    failures.truncate(10);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
