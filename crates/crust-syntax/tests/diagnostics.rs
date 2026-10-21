//! Positions des erreurs rustc ramenées dans le `.rsc` (section 27.3, tests T8/T9).
//!
//! Annotation sur la ligne de l'erreur : `//~ ERROR <code> <extrait>`. La colonne
//! attendue est celle de la première occurrence de `<extrait>` dans la ligne.
//! - `ERROR` : la position brute de rustc (mode dégradé, sans remappage) doit
//!   déjà être exacte, grâce à la disposition préservant les lignes ;
//! - `ERROR@map` : exacte seulement après remappage par la source map
//!   (erreurs situées dans la `struct` ou les en-têtes générés).
//!
//! Pour les erreurs Crust (`CRxxxx`), la position du diagnostic Crust doit être
//! exacte et l'erreur `compile_error!` de rustc doit tomber sur la même ligne.

mod common;

use common::*;
use crust_syntax::{Layout, LineIndex, NoResolver, transpile};

#[derive(Debug)]
struct Expect {
    line: usize,
    col: usize,
    code: String,
    raw_exact: bool,
}

fn expectations(src: &str) -> Vec<Expect> {
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let Some(at) = line.find("//~ ERROR") else { continue };
        let rest = &line[at + "//~ ERROR".len()..];
        let (raw_exact, rest) = match rest.strip_prefix("@map") {
            Some(r) => (false, r),
            None => (true, rest),
        };
        let rest = rest.trim();
        let (code, snippet) = rest.split_once(' ').expect("annotation : code puis extrait");
        let col_byte = line[..at]
            .find(snippet.trim())
            .unwrap_or_else(|| panic!("ligne {} : extrait {snippet:?} introuvable", i + 1));
        out.push(Expect {
            line: i,
            col: line[..col_byte].chars().count(),
            code: code.to_string(),
            raw_exact,
        });
    }
    out
}

#[derive(Debug)]
struct Reported {
    code: String,
    raw: (usize, usize),
    mapped: (usize, usize),
}

#[test]
fn positions_des_erreurs() {
    let dir = scratch_dir("diagnostics");
    let mut failures = Vec::new();
    for path in fixtures("diagnostics") {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let src = read(&path);
        let out = transpile(&src, Layout::Lines, &NoResolver);
        let gen_path = dir.join(&name);
        std::fs::write(&gen_path, &out.code).unwrap();
        let r = rustc(&gen_path, "lib", &dir, true);
        assert!(!r.success, "{name} : la compilation aurait dû échouer");

        let src_idx = LineIndex::new(&src);
        let mut reported = Vec::new();
        for line in r.stderr.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if v["level"] != "error" {
                continue;
            }
            let message = v["message"].as_str().unwrap_or_default();
            let code = match v["code"]["code"].as_str() {
                Some(c) => c.to_string(),
                None if message.starts_with("[CR") => message[1..7].to_string(),
                None => continue, // résumé « aborting due to … »
            };
            let Some(span) = v["spans"]
                .as_array()
                .and_then(|s| s.iter().find(|s| s["is_primary"] == true))
            else {
                continue;
            };
            let raw = (
                span["line_start"].as_u64().unwrap() as usize - 1,
                span["column_start"].as_u64().unwrap() as usize - 1,
            );
            let byte = span["byte_start"].as_u64().unwrap() as usize;
            let mapped = out.map.gen_to_src(byte).expect("position générée sans correspondance");
            let lc = src_idx.line_col(mapped.span.start);
            reported.push(Reported {
                code,
                raw,
                mapped: (lc.line, lc.col_char),
            });
        }

        let expects = expectations(&src);
        for e in &expects {
            let is_crust = e.code.starts_with("CR");
            if is_crust {
                let found = out.diagnostics.iter().any(|d| {
                    let lc = src_idx.line_col(d.span.start);
                    d.code.as_str() == e.code && (lc.line, lc.col_char) == (e.line, e.col)
                });
                if !found {
                    failures.push(format!(
                        "{name}:{} : diagnostic Crust {} attendu en colonne {}, obtenus {:?}",
                        e.line + 1,
                        e.code,
                        e.col + 1,
                        out.diagnostics
                    ));
                }
            }
            let ok = reported.iter().any(|r| {
                r.code == e.code
                    && if is_crust {
                        r.mapped.0 == e.line && r.raw.0 == e.line
                    } else {
                        r.mapped == (e.line, e.col) && (!e.raw_exact || r.raw == (e.line, e.col))
                    }
            });
            if !ok {
                failures.push(format!(
                    "{name}:{}:{} : {} attendu ; rustc a signalé {reported:?}",
                    e.line + 1,
                    e.col + 1,
                    e.code
                ));
            }
        }
        for r in &reported {
            if !expects.iter().any(|e| e.code == r.code && e.line == r.mapped.0) {
                failures.push(format!("{name} : erreur inattendue {r:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
