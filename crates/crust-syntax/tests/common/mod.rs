#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use crust_syntax::{Output, SegmentKind};

pub fn fixtures(dir: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(dir);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "rsc"))
        .collect();
    files.sort();
    files
}

pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Compare à un fichier attendu ; `CRUST_BLESS=1` le (ré)écrit.
pub fn snapshot(path: &Path, actual: &str) -> Result<(), String> {
    if std::env::var_os("CRUST_BLESS").is_some() {
        std::fs::write(path, actual).unwrap();
        return Ok(());
    }
    let expected =
        std::fs::read_to_string(path).map_err(|_| format!("{} absent (lancer avec CRUST_BLESS=1)", path.display()))?;
    if expected == actual {
        return Ok(());
    }
    let line = expected
        .lines()
        .zip(actual.lines())
        .position(|(a, b)| a != b)
        .unwrap_or(0);
    Err(format!(
        "{} diffère à la ligne {} :\n  attendu : {:?}\n  obtenu  : {:?}",
        path.display(),
        line + 1,
        expected.lines().nth(line).unwrap_or(""),
        actual.lines().nth(line).unwrap_or("")
    ))
}

/// Invariants de la disposition préservant les lignes et de la source map.
pub fn check_invariants(src: &str, out: &Output) -> Result<(), String> {
    let lines = |s: &str| s.matches('\n').count();
    if lines(src) != lines(&out.code) {
        return Err(format!(
            "nombre de lignes : source {} / généré {}",
            lines(src),
            lines(&out.code)
        ));
    }
    let mut pos = 0;
    for s in &out.map.segments {
        if s.gen_start != pos {
            return Err(format!("source map non contiguë en {pos} : {s:?}"));
        }
        pos += s.gen_len;
        if s.src_start + s.src_len > src.len() {
            return Err(format!("segment hors de la source : {s:?}"));
        }
        if s.kind == SegmentKind::Copied {
            let g = &out.code[s.gen_start..s.gen_start + s.gen_len];
            let o = &src[s.src_start..s.src_start + s.src_len];
            if g != o {
                return Err(format!("segment copié différent : {g:?} / {o:?}"));
            }
        }
    }
    if pos != out.code.len() {
        return Err(format!("source map incomplète : {pos} / {}", out.code.len()));
    }
    Ok(())
}

pub fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub struct Rustc {
    pub success: bool,
    pub stderr: String,
}

/// Compile `file` comme racine de crate.
pub fn rustc(file: &Path, crate_type: &str, out_dir: &Path, json: bool) -> Rustc {
    let mut cmd = Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()));
    cmd.args([
        "--edition",
        "2024",
        "--crate-type",
        crate_type,
        "--crate-name",
        "fixture",
        "-A",
        "warnings",
    ])
    .arg("--out-dir")
    .arg(out_dir)
    .arg(file);
    if crate_type == "lib" {
        cmd.args(["--emit", "metadata"]);
    }
    if json {
        cmd.arg("--error-format=json");
    }
    let out = cmd.output().expect("rustc introuvable");
    Rustc {
        success: out.status.success(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}
