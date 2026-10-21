//! `crust-lsp` + vrai rust-analyzer sur un projet Crust (phase P4, tests T10, T11, T17).
//!
//! Ignoré si aucun rust-analyzer n'est trouvé (voir `CRUST_RA_PATH`).

use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

fn read_msg(r: &mut impl std::io::BufRead) -> Option<Value> {
    let mut len = 0;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.strip_prefix("Content-Length:") {
            len = v.trim().parse().ok()?;
        }
    }
    let mut body = vec![0; len];
    r.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

struct Client {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Value>,
    next_id: i64,
    notifications: Vec<Value>,
}

impl Client {
    fn start(root: &Path) -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_crust-lsp"))
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            while let Some(m) = read_msg(&mut stdout) {
                if tx.send(m).is_err() {
                    break;
                }
            }
        });
        Client {
            child,
            stdin,
            rx,
            next_id: 1,
            notifications: Vec::new(),
        }
    }

    fn send(&mut self, v: &Value) {
        let body = v.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let m = self
                .rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("pas de réponse");
            if m.get("method").is_some() && m.get("id").is_some() {
                // Requête du serveur (configuration, progression…) : réponse vide.
                let rid = m["id"].clone();
                self.send(&json!({ "jsonrpc": "2.0", "id": rid, "result": null }));
            } else if m.get("method").is_some() {
                self.notifications.push(m);
            } else if m["id"] == id {
                return m;
            }
        }
    }

    fn wait_notification(&mut self, pred: impl Fn(&Value) -> bool, secs: u64) -> Option<Value> {
        if let Some(i) = self.notifications.iter().position(&pred) {
            return Some(self.notifications.remove(i));
        }
        let deadline = Instant::now() + Duration::from_secs(secs);
        while let Ok(m) = self.rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            if m.get("id").is_some() && m.get("method").is_some() {
                let rid = m["id"].clone();
                self.send(&json!({ "jsonrpc": "2.0", "id": rid, "result": null }));
            } else if pred(&m) {
                return Some(m);
            } else {
                self.notifications.push(m);
            }
        }
        None
    }

    /// Répète une requête jusqu'à obtenir un résultat qui satisfait `ok` (indexation).
    fn request_until(&mut self, method: &str, params: Value, ok: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let r = self.request(method, params.clone());
            if ok(&r["result"]) || Instant::now() > deadline {
                return r["result"].clone();
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

fn uri(p: &Path) -> String {
    format!("file://{}", p.display())
}

/// Position LSP (0-based) de la première occurrence de `needle` (+ `delta` caractères).
fn pos(text: &str, needle: &str, delta: usize) -> Value {
    let i = text.find(needle).unwrap_or_else(|| panic!("{needle} absent"));
    let before = &text[..i];
    let line = before.matches('\n').count();
    let col = before.rsplit('\n').next().unwrap().encode_utf16().count() + delta;
    json!({ "line": line, "character": col })
}

fn line_of(text: &str, needle: &str) -> u64 {
    text[..text.find(needle).unwrap()].matches('\n').count() as u64
}

fn labels(result: &Value) -> Vec<String> {
    let items = result.get("items").unwrap_or(result);
    items
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| i["label"].as_str().map(str::to_string))
        .collect()
}

fn project() -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crust-build/tests/projects/p1_robot");
    let dst = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lsp/robot_lsp");
    let _ = std::fs::remove_dir_all(&dst);
    copy_dir(&src, &dst);
    let manifest = dst.join("Cargo.toml");
    let crust_build = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../crust-build")
        .canonicalize()
        .unwrap();
    let toml = std::fs::read_to_string(&manifest)
        .unwrap()
        .replace("{{CRUST_BUILD}}", &crust_build.display().to_string())
        .replace("name = \"p1_robot\"", "name = \"robot_lsp\"");
    std::fs::write(&manifest, toml).unwrap();
    dst.canonicalize().unwrap()
}

fn rust_analyzer_available() -> bool {
    Command::new(env!("CARGO_BIN_EXE_crust-lsp"))
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn editeur_sur_un_projet_crust() {
    if !rust_analyzer_available() {
        eprintln!("rust-analyzer introuvable : test ignoré");
        return;
    }
    let root = project();
    let mut c = Client::start(&root);
    let init = c.request(
        "initialize",
        json!({
            "processId": std::process::id(),
            "rootUri": uri(&root),
            "workspaceFolders": [{ "uri": uri(&root), "name": "robot" }],
            "capabilities": {
                "general": { "positionEncodings": ["utf-16"] },
                "textDocument": {
                    "completion": { "completionItem": { "snippetSupport": false } },
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "definition": { "linkSupport": true },
                    "diagnostic": {},
                    "publishDiagnostics": {},
                    "semanticTokens": {
                        "requests": { "full": { "delta": true }, "range": true },
                        "tokenTypes": ["keyword", "function", "method", "property", "struct", "variable", "parameter", "namespace"],
                        "tokenModifiers": [],
                        "formats": ["relative"]
                    }
                },
                "workspace": { "workspaceFolders": true, "workspaceEdit": { "documentChanges": false } },
                "experimental": { "serverStatusNotification": true }
            }
        }),
    );
    assert!(init["result"]["capabilities"].is_object(), "{init}");
    assert_eq!(init["result"]["capabilities"]["semanticTokensProvider"]["full"], true);
    c.notify("initialized", json!({}));
    let ready = c
        .wait_notification(|m| m["method"] == "crust/status", 300)
        .expect("crust-lsp jamais prêt");
    assert_eq!(ready["params"]["packages"], 1);

    let servo_path = root.join("src/robot/Servomoteur.rsc");
    let moteur_path = root.join("src/robot/Moteur.rsc");
    let main_path = root.join("src/main.rsc");
    let moteur = std::fs::read_to_string(&moteur_path).unwrap();
    let main = std::fs::read_to_string(&main_path).unwrap();
    let servo = std::fs::read_to_string(&servo_path).unwrap().replace(
        "        self.position = position;",
        "        self.position = position;\n        self.parent.",
    );
    c.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri(&servo_path), "languageId": "rust", "version": 1, "text": servo } }),
    );
    c.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri(&moteur_path), "languageId": "rust", "version": 1, "text": moteur } }),
    );

    // T11 : complétion sur le champ généré par `over`.
    let completion = c.request_until(
        "textDocument/completion",
        json!({ "textDocument": { "uri": uri(&servo_path) }, "position": pos(&servo, "self.parent.\n", 12) }),
        |r| labels(r).iter().any(|l| l.starts_with("accelerer")),
    );
    let l = labels(&completion);
    assert!(
        l.iter().any(|x| x.starts_with("accelerer")) && l.iter().any(|x| x == "vitesse"),
        "{l:?}"
    );

    // Survol du nom importé.
    let hover = c.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri(&servo_path) }, "position": pos(&servo, "Moteur from", 2) }),
    );
    assert!(hover["result"]["contents"].to_string().contains("Moteur"), "{hover}");
    assert_eq!(hover["result"]["range"]["start"]["line"], 0, "{hover}");

    // T17 : aller à la définition, d'un .rsc vers un autre .rsc.
    let def = c.request(
        "textDocument/definition",
        json!({ "textDocument": { "uri": uri(&servo_path) }, "position": pos(&servo, "Moteur::new", 9) }),
    );
    let target = &def["result"][0];
    let target_uri = target
        .get("targetUri")
        .unwrap_or(&target["uri"])
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(target_uri.ends_with("src/robot/Moteur.rsc"), "{def}");
    let target_range = target.get("targetSelectionRange").unwrap_or(&target["range"]);
    assert_eq!(target_range["start"]["line"], line_of(&moteur, "pub fn new"), "{def}");

    // D'un .rs vers un .rsc.
    let capteur_path = root.join("src/Capteur.rs");
    let capteur = std::fs::read_to_string(&capteur_path).unwrap();
    c.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri(&capteur_path), "languageId": "rust", "version": 1, "text": capteur } }),
    );
    let def = c.request(
        "textDocument/definition",
        json!({ "textDocument": { "uri": uri(&capteur_path) }, "position": pos(&capteur, "&Servomoteur", 2) }),
    );
    assert!(def["result"].to_string().contains("src/robot/Servomoteur.rsc"), "{def}");

    // Références : depuis Moteur.rsc jusque dans main.rsc (racine).
    let refs = c.request(
        "textDocument/references",
        json!({ "textDocument": { "uri": uri(&moteur_path) }, "position": pos(&moteur, "fn accelerer", 4), "context": { "includeDeclaration": true } }),
    );
    let refs_s = refs["result"].to_string();
    assert!(
        refs_s.contains("src/main.rsc") && refs_s.contains("src/robot/Moteur.rsc"),
        "{refs}"
    );
    assert!(!refs_s.contains("/out/crust/"), "chemin généré visible : {refs}");

    // T10 : complétion dans la racine main.rsc (insérée dans main.rs pour rust-analyzer).
    let main_edit = main.replace("    servo.aller_a(90.0);", "    servo.aller_a(90.0);\n    servo.");
    c.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri(&main_path), "languageId": "rust", "version": 1, "text": main_edit } }),
    );
    let completion = c.request_until(
        "textDocument/completion",
        json!({ "textDocument": { "uri": uri(&main_path) }, "position": pos(&main_edit, "servo.\n", 6) }),
        |r| labels(r).iter().any(|l| l.starts_with("aller_a")),
    );
    assert!(
        labels(&completion).iter().any(|l| l.starts_with("aller_a")),
        "{completion}"
    );

    // Diagnostics natifs : champ manquant, à la bonne position du .rsc.
    let servo_bad = servo
        .replace("self.parent.\n", "\n")
        .replace("parent: Moteur::new(),", "");
    c.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri(&servo_path), "version": 2 }, "contentChanges": [{ "text": servo_bad }] }),
    );
    let diags = c.request_until(
        "textDocument/diagnostic",
        json!({ "textDocument": { "uri": uri(&servo_path) } }),
        |r| {
            r["items"]
                .as_array()
                .is_some_and(|i| i.iter().any(|d| d["code"] == "E0063"))
        },
    );
    let d = diags["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "E0063")
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        d["range"]["start"]["line"],
        line_of(&servo_bad, "        Self {"),
        "{diags}"
    );
    assert_eq!(d["range"]["start"]["character"], 8, "{diags}");

    // Renommage d'un champ : modifications dans les .rsc concernés, sans doublon.
    let rename = c.request(
        "textDocument/rename",
        json!({ "textDocument": { "uri": uri(&moteur_path) }, "position": pos(&moteur, "pub vitesse", 5), "newName": "regime" }),
    );
    // `changes` ou `documentChanges` selon le serveur.
    let mut changes: std::collections::HashMap<String, Vec<Value>> = Default::default();
    for (u, edits) in rename["result"]["changes"].as_object().into_iter().flatten() {
        changes
            .entry(u.clone())
            .or_default()
            .extend(edits.as_array().cloned().unwrap_or_default());
    }
    for dc in rename["result"]["documentChanges"].as_array().into_iter().flatten() {
        let u = dc["textDocument"]["uri"].as_str().unwrap().to_string();
        changes
            .entry(u)
            .or_default()
            .extend(dc["edits"].as_array().cloned().unwrap_or_default());
    }
    let main_uri = uri(&main_path);
    let moteur_uri = uri(&moteur_path);
    assert!(
        changes.contains_key(&main_uri) && changes.contains_key(&moteur_uri),
        "{rename}"
    );
    let moteur_edits = &changes[&moteur_uri];
    // Déclaration + deux usages ; la copie dans la `struct` générée n'est pas dupliquée.
    assert_eq!(moteur_edits.len(), 3, "{rename}");
    assert!(moteur_edits.iter().all(|e| e["newText"] == "regime"), "{rename}");
    assert_eq!(changes[&main_uri].len(), 2, "{rename}");
    assert!(!rename.to_string().contains("/out/crust/"), "{rename}");

    // Coloration : `class` marqué comme mot-clé, `vitesse` coloré par rust-analyzer.
    let tokens = c.request(
        "textDocument/semanticTokens/full",
        json!({ "textDocument": { "uri": uri(&moteur_path) } }),
    );
    let data: Vec<u64> = tokens["result"]["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    let (mut line, mut col) = (0, 0);
    let mut decoded = Vec::new();
    for t in data.chunks_exact(5) {
        if t[0] > 0 {
            line += t[0];
            col = t[1];
        } else {
            col += t[1];
        }
        decoded.push((line, col, t[2]));
    }
    let class_pos = pos(&moteur, "class Moteur", 0);
    assert!(
        decoded.contains(&(
            class_pos["line"].as_u64().unwrap(),
            class_pos["character"].as_u64().unwrap(),
            5
        )),
        "{decoded:?}"
    );
    let vitesse = pos(&moteur, "vitesse: u32", 0);
    assert!(
        decoded
            .iter()
            .any(|&(l, c, _)| l == vitesse["line"].as_u64().unwrap() && c == vitesse["character"].as_u64().unwrap()),
        "{decoded:?}"
    );

    c.request("shutdown", json!(null));
    c.notify("exit", json!(null));
}
