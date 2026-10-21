//! `cargo crust` : sous-commande cargo des projets Crust.
//!
//! Phase P1 : `cargo crust expand <fichier.rsc>` affiche le Rust généré.

use std::io::Write;
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

use crust_syntax::{Diagnostic, FsResolver, Layout, LineIndex, NoResolver, Resolver, transpile};

const USAGE: &str = "\
usage : cargo crust expand <fichier.rsc> [--lines] [--map]

  expand    affiche le Rust standard équivalent au fichier .rsc
  --lines   disposition préservant les lignes (celle utilisée pour compiler)
  --map     affiche aussi la source map (avec --lines)";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // Appelé par cargo sous la forme `cargo-crust crust ...`.
    if args.first().is_some_and(|a| a == "crust") {
        args.remove(0);
    }
    match args.first().map(String::as_str) {
        Some("expand") => expand(&args[1..]),
        Some("--version" | "-V") => {
            println!("cargo-crust {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn expand(args: &[String]) -> ExitCode {
    let lines = args.iter().any(|a| a == "--lines");
    let show_map = args.iter().any(|a| a == "--map");
    let Some(file) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let path = Path::new(file);
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("erreur : impossible de lire `{file}` : {e}");
            return ExitCode::FAILURE;
        }
    };
    let fs_resolver = FsResolver::for_file(path);
    let resolver: &dyn Resolver = match &fs_resolver {
        Some(r) => r,
        None => &NoResolver,
    };
    let layout = if lines { Layout::Lines } else { Layout::Natural };
    let out = transpile(&src, layout, resolver);

    for d in &out.diagnostics {
        eprintln!("{}", render(d, file, &src));
    }
    let code = if lines {
        out.code.clone()
    } else {
        rustfmt(&out.code).unwrap_or(out.code.clone())
    };
    print!("{code}");
    if show_map {
        println!("\n// ---- source map ----");
        for s in &out.map.segments {
            println!(
                "// gen {:>6}+{:<4} -> src {:>6}+{:<4} {:?}{}",
                s.gen_start,
                s.gen_len,
                s.src_start,
                s.src_len,
                s.kind,
                if s.primary { " (primaire)" } else { "" }
            );
        }
    }
    if out.diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn rustfmt(code: &str) -> Option<String> {
    let mut child = Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(code.as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8(out.stdout).ok())
        .flatten()
}

/// Rendu d'un diagnostic au format rustc.
fn render(d: &Diagnostic, file: &str, src: &str) -> String {
    let idx = LineIndex::new(src);
    let mut spans = vec![(d.span, String::new(), '^')];
    spans.extend(d.labels.iter().map(|l| (l.span, l.message.clone(), '-')));
    let start = idx.line_col(d.span.start);
    let width = spans
        .iter()
        .map(|(s, ..)| idx.line_col(s.start).line + 1)
        .max()
        .unwrap_or(1)
        .to_string()
        .len();
    let pad = " ".repeat(width);
    let mut out = format!(
        "error[{}]: {}\n{pad}--> {}:{}:{}\n{pad} |\n",
        d.code.as_str(),
        d.message,
        file,
        start.line + 1,
        start.col_char + 1
    );
    spans.sort_by_key(|(s, ..)| s.start);
    let mut last_line = None;
    for (span, msg, mark) in &spans {
        let lc = idx.line_col(span.start);
        let line_text = src.lines().nth(lc.line).unwrap_or("");
        if last_line != Some(lc.line) {
            out += &format!("{:>width$} | {}\n", lc.line + 1, line_text);
            last_line = Some(lc.line);
        }
        let end = idx.line_col(span.end);
        let len = if end.line == lc.line {
            (end.col_char - lc.col_char).max(1)
        } else {
            line_text.chars().count().saturating_sub(lc.col_char).max(1)
        };
        out += &format!(
            "{pad} | {}{} {}\n",
            " ".repeat(lc.col_char),
            mark.to_string().repeat(len),
            msg
        );
    }
    if let Some(help) = &d.help {
        out += &format!("{pad} |\n{pad} = help: {help}\n");
    }
    out
}
