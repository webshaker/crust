//! `cargo crust build|check|run|test|clippy|doc|bench` : cargo avec diagnostics remappés.

use std::io::{IsTerminal, Read, Write};
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

use serde_json::Value;

use crate::remap::Remapper;
use crate::render::render;

/// Préfixe des lignes JSON de cargo ; toute autre ligne de stdout vient du programme.
const JSON_PREFIX: &[u8] = b"{\"reason\":";

pub fn run(sub: &str, args: &[String]) -> ExitCode {
    // `--message-format` de l'utilisateur : JSON remappé (pour les outils).
    let mut user_json = false;
    let mut rest = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--" {
            rest.push(a.clone());
            rest.extend(it.by_ref().cloned());
            break;
        }
        if let Some(fmt) = a.strip_prefix("--message-format=") {
            user_json = fmt.starts_with("json");
        } else if a == "--message-format" {
            user_json = it.next().is_some_and(|f| f.starts_with("json"));
        } else {
            rest.push(a.clone());
        }
    }
    let color = !user_json
        && std::io::stderr().is_terminal()
        && std::env::var_os("NO_COLOR").is_none()
        && !args.windows(2).any(|w| w[0] == "--color" && w[1] == "never")
        && !args.iter().any(|a| a == "--color=never");
    let format = if color { "json-diagnostic-rendered-ansi" } else { "json" };

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut child = match Command::new(cargo)
        .arg(sub)
        .arg(format!("--message-format={format}"))
        .args(&rest)
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("erreur : impossible de lancer cargo : {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut remapper = Remapper::new();
    let mut stdout = child.stdout.take().expect("stdout du processus cargo");
    let mut out = std::io::stdout().lock();
    let mut line: Vec<u8> = Vec::new();
    let mut passthrough = false;
    let mut buf = [0u8; 8192];
    loop {
        let n = match stdout.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let mut start = 0;
        for i in 0..n {
            let b = buf[i];
            if passthrough {
                if b == b'\n' {
                    let _ = out.write_all(&buf[start..=i]);
                    start = i + 1;
                    passthrough = false;
                }
                continue;
            }
            line.push(b);
            start = i + 1;
            if b == b'\n' {
                handle_json(&line, &mut remapper, user_json, color, &mut out);
                line.clear();
            } else if !JSON_PREFIX.starts_with(&line[..line.len().min(JSON_PREFIX.len())]) {
                // Sortie du programme (cargo run / cargo test) : transmise telle quelle,
                // même sans fin de ligne (invite de saisie).
                let _ = out.write_all(&line);
                line.clear();
                passthrough = true;
            }
        }
        if passthrough && start < n {
            let _ = out.write_all(&buf[start..n]);
        }
        let _ = out.flush();
    }
    if !line.is_empty() {
        handle_json(&line, &mut remapper, user_json, color, &mut out);
    }
    let status = child.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1);
    ExitCode::from(u8::try_from(status).unwrap_or(1))
}

fn handle_json(line: &[u8], remapper: &mut Remapper, user_json: bool, color: bool, out: &mut impl Write) {
    let Ok(mut v) = serde_json::from_slice::<Value>(line) else {
        let _ = out.write_all(line);
        return;
    };
    if v["reason"] != "compiler-message" {
        if user_json {
            let _ = out.write_all(line);
        }
        return;
    }
    let manifest_dir = v["manifest_path"]
        .as_str()
        .and_then(|p| Path::new(p).parent())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let msg = &mut v["message"];
    if remapper.remap_message(msg, &manifest_dir) {
        msg["rendered"] = Value::from(render(msg, color));
    }
    if user_json {
        let _ = writeln!(out, "{v}");
    } else if let Some(rendered) = msg["rendered"].as_str() {
        let _ = out.flush();
        eprint!("{rendered}");
    }
}
