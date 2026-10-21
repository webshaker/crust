//! `crust-lsp` : rust-analyzer pour les fichiers `.rsc` (section 20 du cahier des charges).
//!
//! Utilisation avec l'extension rust-analyzer de VS Code :
//! `"rust-analyzer.server.path": "crust-lsp"`. Le vrai rust-analyzer est cherché
//! dans `CRUST_RA_PATH`, puis via `rustup`, puis dans les extensions VS Code.

mod proxy;
mod rpc;
mod workspace;

use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::Arc;

fn works(ra: &Path) -> bool {
    Command::new(ra)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Le binaire rust-analyzer à piloter.
fn find_rust_analyzer() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("CRUST_RA_PATH") {
        return Some(PathBuf::from(p));
    }
    if let Ok(out) = Command::new("rustup")
        .args(["which", "rust-analyzer"])
        .stderr(Stdio::null())
        .output()
        && out.status.success()
    {
        let p = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
        if works(&p) {
            return Some(p);
        }
    }
    // Binaire livré avec l'extension VS Code (la version la plus récente).
    let home = PathBuf::from(std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?);
    let exe = if cfg!(windows) {
        "rust-analyzer.exe"
    } else {
        "rust-analyzer"
    };
    let mut candidates = Vec::new();
    for dir in [
        ".vscode-server/extensions",
        ".vscode/extensions",
        ".vscode-insiders/extensions",
        ".cursor/extensions",
    ] {
        let Ok(entries) = std::fs::read_dir(home.join(dir)) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("rust-lang.rust-analyzer-") {
                let p = e.path().join("server").join(exe);
                if p.is_file() {
                    candidates.push((name, p));
                }
            }
        }
    }
    candidates.sort();
    candidates.into_iter().map(|(_, p)| p).rev().find(|p| works(p))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(ra) = find_rust_analyzer() else {
        eprintln!(
            "crust-lsp : rust-analyzer introuvable (définir CRUST_RA_PATH, ou `rustup component add rust-analyzer`)"
        );
        return ExitCode::FAILURE;
    };
    if args.iter().any(|a| a == "--version" || a == "-V") {
        let out = Command::new(&ra)
            .arg("--version")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        println!("{} (crust-lsp {})", out.unwrap_or_default(), env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "crust-lsp {} : proxy rust-analyzer pour les fichiers .rsc\nrust-analyzer utilisé : {}",
            env!("CARGO_PKG_VERSION"),
            ra.display()
        );
        return ExitCode::SUCCESS;
    }
    // Autres sous-commandes de rust-analyzer (diagnostics, analysis-stats…) : déléguées.
    if !args.is_empty() {
        let status = Command::new(&ra).args(&args).status();
        return ExitCode::from(status.map(|s| s.code().unwrap_or(1) as u8).unwrap_or(1));
    }

    let mut child = match Command::new(&ra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("crust-lsp : impossible de lancer {} : {e}", ra.display());
            return ExitCode::FAILURE;
        }
    };
    let server_in = child.stdin.take().expect("stdin de rust-analyzer");
    let server_out = child.stdout.take().expect("stdout de rust-analyzer");
    let proxy = Arc::new(proxy::Proxy::new(
        rpc::Writer::new(std::io::stdout()),
        rpc::Writer::new(server_in),
    ));

    let on_server_message = Arc::clone(&proxy);
    let reader = std::thread::spawn(move || {
        let mut r = BufReader::new(server_out);
        while let Ok(Some(msg)) = rpc::read(&mut r) {
            on_server_message.on_server_message(msg);
        }
    });

    let mut stdin = BufReader::new(std::io::stdin());
    while let Ok(Some(msg)) = rpc::read(&mut stdin) {
        if proxy.on_client_message(msg) {
            break;
        }
    }
    let status = child.wait();
    let _ = reader.join();
    ExitCode::from(status.map(|s| s.code().unwrap_or(0) as u8).unwrap_or(1))
}
