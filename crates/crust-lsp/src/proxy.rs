//! Routage des messages entre l'éditeur et rust-analyzer (section 20.2).
//!
//! - Les `.rs` passent sans transformation.
//! - Un `.rsc` n'est jamais envoyé tel quel à rust-analyzer : il voit le code
//!   généré (overlay), et toutes les positions sont converties dans les deux sens.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use crust_syntax::lexer::{TokenKind, tokenize};
use serde_json::{Map, Value, json};

use crate::rpc::Writer;
use crate::workspace::{Encoding, FileState, Package, discover, offset_of, path_of_uri, position_of, uri_of_path};

pub struct Proxy {
    client: Writer,
    server: Writer,
    state: Mutex<State>,
}

struct Pending {
    method: String,
    /// `.rsc` concerné par la requête (réponses sans URI).
    src: Option<PathBuf>,
}

struct State {
    enc: Encoding,
    roots: Vec<PathBuf>,
    packages: Vec<Package>,
    /// Textes ouverts dans l'éditeur : `.rsc` et `.rs` racines gérés par Crust.
    docs: HashMap<PathBuf, String>,
    /// Documents envoyés à rust-analyzer : chemin → (version, texte).
    sent: HashMap<PathBuf, (i64, String)>,
    pending: HashMap<String, Pending>,
    /// Indice du type `keyword` dans la légende des tokens sémantiques.
    keyword_token: Option<u64>,
    ready: bool,
}

const RANGE_KEYS: &[&str] = &[
    "range",
    "selectionRange",
    "targetRange",
    "targetSelectionRange",
    "fullRange",
    "insert",
    "replace",
];

fn is_rsc(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "rsc")
}

/// `.rs` racine dont le contenu est fourni par Crust : `main.rs` à côté de
/// `main.rsc`, `lib.rs` / `lib.rsc`, `bin/<nom>.rs` / `bin/<nom>.rsc`.
fn is_managed_root(path: &Path) -> bool {
    let root_name = matches!(path.file_name().and_then(|n| n.to_str()), Some("main.rs" | "lib.rs"));
    let in_bin = path.parent().and_then(Path::file_name).is_some_and(|d| d == "bin");
    path.extension().is_some_and(|e| e == "rs") && (root_name || in_bin) && path.with_extension("rsc").is_file()
}

fn uri_path(v: &Value) -> Option<PathBuf> {
    v.as_str().and_then(path_of_uri)
}

impl State {
    fn file(&self, src: &Path) -> Option<&FileState> {
        self.packages.iter().find_map(|p| p.files.get(src))
    }

    fn by_doc(&self, path: &Path) -> Option<(&FileState, bool)> {
        self.packages.iter().find_map(|p| p.by_doc(path))
    }

    fn package_of(&self, path: &Path) -> Option<usize> {
        self.packages
            .iter()
            .position(|p| path.starts_with(p.manifest_dir.join("src")))
    }
}

impl Proxy {
    pub fn new(client: Writer, server: Writer) -> Self {
        Proxy {
            client,
            server,
            state: Mutex::new(State {
                enc: Encoding::Utf16,
                roots: Vec::new(),
                packages: Vec::new(),
                docs: HashMap::new(),
                sent: HashMap::new(),
                pending: HashMap::new(),
                keyword_token: None,
                ready: false,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ------------------------------------------------------------- éditeur → RA

    /// Renvoie `true` après `exit`.
    pub fn on_client_message(self: &Arc<Self>, mut msg: Value) -> bool {
        let method = msg["method"].as_str().map(str::to_string);
        let id = msg.get("id").cloned();
        match (method.as_deref(), id) {
            (None, Some(_)) => self.server.send(&msg), // réponse à une requête de RA
            (Some(m), Some(id)) => self.client_request(m, id, msg),
            (Some("exit"), None) => {
                self.server.send(&msg);
                return true;
            }
            (Some("initialized"), None) => {
                self.server.send(&msg);
                let me = Arc::clone(self);
                std::thread::spawn(move || me.init_workspace());
            }
            (Some(m), None) => self.client_notification(m, &mut msg),
            (None, None) => {}
        }
        false
    }

    fn client_request(&self, method: &str, id: Value, mut msg: Value) {
        let mut st = self.lock();
        if method == "initialize" {
            let params = &msg["params"];
            let mut roots: Vec<PathBuf> = params["workspaceFolders"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|f| uri_path(&f["uri"]))
                .collect();
            if roots.is_empty()
                && let Some(p) = uri_path(&params["rootUri"])
            {
                roots.push(p);
            }
            st.roots = roots;
        }
        if matches!(method, "textDocument/formatting" | "textDocument/rangeFormatting")
            && let Some(path) = uri_path(&msg["params"]["textDocument"]["uri"]).filter(|p| is_rsc(p))
        {
            let result = if method == "textDocument/formatting" {
                Self::format_rsc(&st, &path)
            } else {
                json!([])
            };
            drop(st);
            self.client
                .send(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
            return;
        }
        let mut src = None;
        if msg.get("params").is_none() {
            st.pending.insert(
                id.to_string(),
                Pending {
                    method: method.to_string(),
                    src,
                },
            );
            drop(st);
            self.server.send(&msg);
            return;
        }
        let params = &mut msg["params"];
        if let Some(path) = uri_path(&params["textDocument"]["uri"])
            && is_rsc(&path)
        {
            let enc = st.enc;
            let Some(fs) = st.file(&path) else {
                // Pas encore prêt ou fichier hors paquet Crust : réponse vide.
                drop(st);
                self.client.send(&json!({ "jsonrpc": "2.0", "id": id, "result": null }));
                return;
            };
            params["textDocument"]["uri"] = Value::from(uri_of_path(&fs.doc_path));
            // `get_mut` : indexer un `Value` mutable créerait les clés absentes.
            if let Some(p) = params.get_mut("position") {
                let before = p.clone();
                *p = fs.src_to_doc(p, enc);
                if std::env::var_os("CRUST_LSP_TRACE").is_some() {
                    let line = fs
                        .doc_text
                        .lines()
                        .nth(p["line"].as_u64().unwrap_or(0) as usize)
                        .unwrap_or("");
                    eprintln!(
                        "crust-lsp : {method} {before} -> {p} dans {} : {line:?}",
                        fs.doc_path.display()
                    );
                }
            }
            if let Some(r) = params.get_mut("range") {
                map_range_to_doc(fs, r, enc);
            }
            let diags = params
                .get_mut("context")
                .and_then(|c| c.get_mut("diagnostics"))
                .and_then(Value::as_array_mut);
            for d in diags.into_iter().flatten() {
                if let Some(r) = d.get_mut("range") {
                    map_range_to_doc(fs, r, enc);
                }
            }
            if let Some(positions) = params.get_mut("positions").and_then(Value::as_array_mut) {
                for p in positions {
                    *p = fs.src_to_doc(p, enc);
                }
            }
            src = Some(path);
        } else if method.ends_with("/resolve") {
            // Élément opaque : le document d'origine est noté dans ses données.
            src = find_doc_uri(&params["data"]).and_then(|p| st.by_doc(&p).map(|(f, _)| f.src_path.clone()));
        }
        st.pending.insert(
            id.to_string(),
            Pending {
                method: method.to_string(),
                src,
            },
        );
        drop(st);
        self.server.send(&msg);
    }

    fn client_notification(&self, method: &str, msg: &mut Value) {
        let params = &msg["params"];
        let path = uri_path(&params["textDocument"]["uri"]);
        let managed = path.as_deref().is_some_and(|p| is_rsc(p) || is_managed_root(p));
        match method {
            "textDocument/didOpen" if managed => {
                let path = path.unwrap();
                let text = params["textDocument"]["text"].as_str().unwrap_or_default().to_string();
                let mut st = self.lock();
                st.docs.insert(path.clone(), text);
                self.refresh_file(&mut st, &path);
            }
            "textDocument/didChange" if managed => {
                let path = path.unwrap();
                let mut st = self.lock();
                let enc = st.enc;
                let mut text = st.docs.get(&path).cloned().unwrap_or_default();
                for change in params["contentChanges"].as_array().into_iter().flatten() {
                    let new = change["text"].as_str().unwrap_or_default();
                    if change.get("range").is_some_and(|r| r.is_object()) {
                        let s = offset_of(&text, &change["range"]["start"], enc);
                        let e = offset_of(&text, &change["range"]["end"], enc).max(s);
                        text.replace_range(s..e, new);
                    } else {
                        text = new.to_string();
                    }
                }
                st.docs.insert(path.clone(), text);
                self.refresh_file(&mut st, &path);
            }
            "textDocument/didClose" if managed => {
                let path = path.unwrap();
                let mut st = self.lock();
                st.docs.remove(&path);
                self.refresh_file(&mut st, &path);
            }
            "textDocument/didSave" if managed => {
                // rust-analyzer lance `cargo check` à l'enregistrement d'un fichier de la
                // crate : on lui signale celui du `.rs` racine du paquet.
                let path = path.unwrap();
                let st = self.lock();
                if let Some(pkg) = st.package_of(&path).map(|i| &st.packages[i]) {
                    let src = pkg.manifest_dir.join("src");
                    let root = if src.join("lib.rs").is_file() {
                        src.join("lib.rs")
                    } else {
                        src.join("main.rs")
                    };
                    let uri = uri_of_path(&root);
                    drop(st);
                    self.server.send(
                        &json!({ "jsonrpc": "2.0", "method": method, "params": { "textDocument": { "uri": uri } } }),
                    );
                }
            }
            "workspace/didChangeWatchedFiles" => {
                self.server.send(msg);
                let changed: Vec<PathBuf> = params["changes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| uri_path(&c["uri"]))
                    .filter(|p| p.extension().is_some_and(|e| e == "rsc" || e == "rs"))
                    .collect();
                let mut st = self.lock();
                let touched: HashSet<usize> = changed.iter().filter_map(|p| st.package_of(p)).collect();
                for i in touched {
                    self.rescan(&mut st, i);
                }
            }
            _ => self.server.send(msg),
        }
    }

    // ------------------------------------------------------------- RA → éditeur

    pub fn on_server_message(&self, mut msg: Value) {
        let method = msg["method"].as_str().map(str::to_string);
        match (method.as_deref(), msg.get("id").cloned()) {
            (None, Some(id)) => {
                let mut st = self.lock();
                if let Some(p) = st.pending.remove(&id.to_string())
                    && msg.get("result").is_some_and(|r| !r.is_null())
                {
                    if p.method == "initialize" {
                        Self::patch_capabilities(&mut st, &mut msg["result"]);
                    } else if p.method == "textDocument/semanticTokens/full" {
                        if let Some(fs) = p.src.as_deref().and_then(|s| st.file(s)) {
                            semantic_tokens(fs, &mut msg["result"], st.enc, st.keyword_token);
                        }
                    } else if let Some(result) = msg.get_mut("result") {
                        let ctx = p.src.as_deref().and_then(|s| st.file(s)).map(|f| (f, false));
                        map_value(&st, result, ctx);
                    }
                }
                drop(st);
                self.client.send(&msg);
            }
            (Some("workspace/applyEdit"), Some(_)) => {
                let st = self.lock();
                map_value(&st, &mut msg["params"]["edit"], None);
                drop(st);
                self.client.send(&msg);
            }
            (Some("textDocument/publishDiagnostics"), None) => self.publish_diagnostics(msg),
            _ => self.client.send(&msg),
        }
    }

    /// Formatage d'un `.rsc` (rust-analyzer ne sait pas le faire) : une seule
    /// modification qui remplace tout le document.
    fn format_rsc(st: &State, path: &Path) -> Value {
        let Some(text) = st
            .docs
            .get(path)
            .cloned()
            .or_else(|| std::fs::read_to_string(path).ok())
        else {
            return json!([]);
        };
        let pkg = st.package_of(path).map(|i| st.packages[i].manifest_dir.clone());
        let edition = pkg
            .as_ref()
            .and_then(|d| std::fs::read_to_string(d.join("Cargo.toml")).ok())
            .and_then(|t| {
                t.lines().find_map(|l| {
                    l.trim()
                        .strip_prefix("edition")
                        .map(|v| v.trim_start_matches([' ', '=', '"']).trim_end_matches('"').to_string())
                })
            })
            .unwrap_or_else(|| "2021".into());
        match crust_syntax::fmt::format(&text, &edition, pkg.as_deref()) {
            Ok(formatted) if formatted != text => {
                let end = position_of(&text, text.len(), st.enc);
                json!([{ "range": { "start": { "line": 0, "character": 0 }, "end": end }, "newText": formatted }])
            }
            Ok(_) => json!([]),
            Err(e) => {
                eprintln!("crust-lsp : formatage de {} impossible : {e}", path.display());
                json!([])
            }
        }
    }

    fn patch_capabilities(st: &mut State, result: &mut Value) {
        let caps = &mut result["capabilities"];
        if caps["positionEncoding"] == "utf-8" {
            st.enc = Encoding::Utf8;
        }
        if let Some(sem) = caps.get_mut("semanticTokensProvider").filter(|s| s.is_object()) {
            st.keyword_token = sem["legend"]["tokenTypes"]
                .as_array()
                .and_then(|t| t.iter().position(|x| x == "keyword"))
                .map(|i| i as u64);
            // Pas de deltas ni de plages : la conversion porte sur le document entier.
            sem["full"] = Value::Bool(true);
            if let Some(o) = sem.as_object_mut() {
                o.remove("range");
            }
        }
        result["serverInfo"]["name"] = Value::from("rust-analyzer (crust-lsp)");
    }

    fn publish_diagnostics(&self, mut msg: Value) {
        let st = self.lock();
        let Some(path) = uri_path(&msg["params"]["uri"]) else {
            drop(st);
            self.client.send(&msg);
            return;
        };
        let Some((fs, alias)) = st.by_doc(&path) else {
            drop(st);
            self.client.send(&msg);
            return;
        };
        let enc = st.enc;
        let mut for_src = Vec::new();
        let mut for_doc = Vec::new();
        for mut d in msg["params"]["diagnostics"].as_array().cloned().unwrap_or_default() {
            match fs.doc_to_src(&d["range"], enc, alias) {
                Some((range, _)) => {
                    d["range"] = range;
                    if let Some(related) = d.get_mut("relatedInformation") {
                        map_value(&st, related, Some((fs, alias)));
                    }
                    for_src.push(d);
                }
                // Partie du `.rs` racine située avant le code inséré : coordonnées inchangées.
                None if !alias && offset_of(&fs.doc_text, &d["range"]["end"], enc) <= fs.offset => for_doc.push(d),
                None => {}
            }
        }
        let src_uri = uri_of_path(&fs.src_path);
        let separate_doc = !alias && fs.doc_path != fs.gen_path;
        drop(st);
        self.client.send(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": { "uri": src_uri, "diagnostics": for_src },
        }));
        if separate_doc {
            msg["params"]["diagnostics"] = Value::from(for_doc);
            self.client.send(&msg);
        }
    }

    // ---------------------------------------------------------- overlays

    fn init_workspace(&self) {
        let roots = self.lock().roots.clone();
        let docs = self.lock().docs.clone();
        let mut packages = Vec::new();
        for root in roots {
            for (manifest_dir, gen_dir) in discover(&root) {
                match Package::load(&manifest_dir, &gen_dir, &docs) {
                    Ok(p) => packages.push(p),
                    Err(e) => eprintln!("crust-lsp : {} : {e}", manifest_dir.display()),
                }
            }
        }
        let mut st = self.lock();
        // Textes ouverts pendant le chargement : on retranspile avec l'état courant.
        let docs = st.docs.clone();
        for p in &mut packages {
            p.refresh(&docs);
        }
        let count = packages.len();
        st.packages = packages;
        st.ready = true;
        for i in 0..st.packages.len() {
            self.sync_package(&mut st, i);
        }
        drop(st);
        eprintln!("crust-lsp : {count} paquet(s) Crust chargé(s)");
        self.client.send(
            &json!({ "jsonrpc": "2.0", "method": "crust/status", "params": { "ready": true, "packages": count } }),
        );
    }

    /// Envoie à rust-analyzer un document (ouverture, ou remplacement complet).
    fn sync(&self, st: &mut State, path: &Path, text: &str) {
        let uri = uri_of_path(path);
        if std::env::var_os("CRUST_LSP_TRACE").is_some() {
            eprintln!("crust-lsp : overlay {} ({} octets)", path.display(), text.len());
        }
        match st.sent.get_mut(path) {
            Some((_, old)) if old == text => {}
            Some((version, old)) => {
                *version += 1;
                *old = text.to_string();
                let v = *version;
                self.server.send(&json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/didChange",
                    "params": { "textDocument": { "uri": uri, "version": v }, "contentChanges": [{ "text": text }] },
                }));
            }
            None => {
                st.sent.insert(path.to_path_buf(), (1, text.to_string()));
                self.server.send(&json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/didOpen",
                    "params": { "textDocument": { "uri": uri, "languageId": "rust", "version": 1, "text": text } },
                }));
            }
        }
    }

    fn sync_package(&self, st: &mut State, i: usize) {
        let docs: Vec<(PathBuf, String)> = st.packages[i]
            .files
            .values()
            .map(|f| (f.doc_path.clone(), f.doc_text.clone()))
            .chain(st.packages[i].decls.iter().cloned())
            .collect();
        for (path, text) in docs {
            self.sync(st, &path, &text);
        }
    }

    fn rescan(&self, st: &mut State, i: usize) {
        let docs = st.docs.clone();
        let p = &st.packages[i];
        match Package::load(&p.manifest_dir, &p.gen_dir, &docs) {
            Ok(new) => st.packages[i] = new,
            Err(e) => eprintln!("crust-lsp : {e}"),
        }
        self.sync_package(st, i);
    }

    /// Après une modification d'un `.rsc` ou d'un `.rs` racine géré.
    fn refresh_file(&self, st: &mut State, path: &Path) {
        if !st.ready {
            return;
        }
        let Some(i) = st.package_of(path) else { return };
        let docs = st.docs.clone();
        let text = docs
            .get(path)
            .cloned()
            .or_else(|| std::fs::read_to_string(path).ok())
            .unwrap_or_default();
        if is_rsc(path) {
            if !st.packages[i].update(path, text, &docs) {
                // Nouveau fichier : l'arbre de modules change.
                self.rescan(st, i);
                return;
            }
            let fs = &st.packages[i].files[path];
            let (doc, doc_text) = (fs.doc_path.clone(), fs.doc_text.clone());
            self.sync(st, &doc, &doc_text);
        } else {
            st.packages[i].refresh(&docs);
            self.sync_package(st, i);
        }
    }
}

fn map_range_to_doc(fs: &FileState, range: &mut Value, enc: Encoding) {
    if range.is_object() {
        let start = fs.src_to_doc(&range["start"], enc);
        let end = fs.src_to_doc(&range["end"], enc);
        *range = json!({ "start": start, "end": end });
    }
}

fn find_doc_uri(v: &Value) -> Option<PathBuf> {
    match v {
        Value::String(s) if s.starts_with("file://") => path_of_uri(s),
        Value::Object(o) => o.values().find_map(find_doc_uri),
        Value::Array(a) => a.iter().find_map(find_doc_uri),
        _ => None,
    }
}

/// Convertit toutes les positions d'une réponse vers les `.rsc`.
///
/// Un objet porteur d'une URI (`uri`, `targetUri`, `textDocument.uri`) désigne
/// son document ; sinon on hérite de `ctx` (document de la requête). Renvoie
/// `false` si l'objet doit être retiré : modification de texte qui tomberait
/// dans du code généré.
fn map_value(st: &State, v: &mut Value, ctx: Option<(&FileState, bool)>) -> bool {
    match v {
        Value::Array(items) => {
            let mut keep = Vec::with_capacity(items.len());
            for mut item in std::mem::take(items) {
                // Doublons : un renommage de classe touche `struct`, `impl`… qui
                // reviennent tous au même nom dans le `.rsc`.
                let is_edit = item.get("newText").is_some() && item.get("label").is_none();
                if map_value(st, &mut item, ctx) && !(is_edit && keep.contains(&item)) {
                    keep.push(item);
                }
            }
            *items = keep;
            true
        }
        Value::Object(obj) => map_object(st, obj, ctx),
        _ => true,
    }
}

fn map_object(st: &State, obj: &mut Map<String, Value>, ctx: Option<(&FileState, bool)>) -> bool {
    let enc = st.enc;
    let request_ctx = ctx;
    let mut local = ctx;
    let mut owned_uri = false;
    for key in ["uri", "targetUri"] {
        if let Some(path) = obj.get(key).and_then(uri_path) {
            owned_uri = true;
            local = st.by_doc(&path);
            if let Some((fs, _)) = local {
                obj.insert(key.to_string(), Value::from(uri_of_path(&fs.src_path)));
            }
        }
    }
    if let Some(td) = obj.get_mut("textDocument").and_then(Value::as_object_mut)
        && let Some(path) = td.get("uri").and_then(uri_path)
    {
        owned_uri = true;
        local = st.by_doc(&path);
        if let Some((fs, _)) = local {
            td.insert("uri".into(), Value::from(uri_of_path(&fs.src_path)));
            td.insert("version".into(), Value::Null);
        }
    }

    // WorkspaceEdit.changes : { uri: [TextEdit] }
    if let Some(changes) = obj.get_mut("changes").and_then(Value::as_object_mut) {
        let mut out = Map::new();
        for (uri, mut edits) in std::mem::take(changes) {
            let (key, fctx) = match path_of_uri(&uri).and_then(|p| st.by_doc(&p)) {
                Some((fs, alias)) => (uri_of_path(&fs.src_path), Some((fs, alias))),
                None => (uri, None),
            };
            map_value(st, &mut edits, fctx);
            match out.get_mut(&key).and_then(Value::as_array_mut) {
                Some(existing) => {
                    for e in edits.as_array().cloned().unwrap_or_default() {
                        if !existing.contains(&e) {
                            existing.push(e);
                        }
                    }
                }
                None => {
                    out.insert(key, edits);
                }
            }
        }
        *changes = out;
    }

    let is_edit = obj.contains_key("newText");
    let mut keep = true;
    if let Some((fs, alias)) = local.filter(|_| owned_uri || ctx.is_some()) {
        for key in RANGE_KEYS {
            let Some(range) = obj.get_mut(*key).filter(|r| r.get("start").is_some()) else {
                continue;
            };
            match fs.doc_to_src(range, enc, alias) {
                Some((r, exact)) => {
                    *range = r;
                    if is_edit && !exact {
                        keep = false;
                    }
                }
                None => {
                    // Hors du code `.rsc` (déclarations générées) : on retire l'objet
                    // s'il n'a de sens qu'avec sa plage, sinon seulement la plage.
                    if is_edit || owned_uri || obj.contains_key("message") {
                        keep = false;
                    } else {
                        obj.remove(*key);
                    }
                }
            }
        }
        if let Some(pos) = obj.get_mut("position").filter(|p| p.get("line").is_some()) {
            let range = json!({ "start": pos.clone(), "end": pos.clone() });
            if let Some((r, _)) = fs.doc_to_src(&range, enc, alias) {
                *pos = r["start"].clone();
            }
        }
    }
    // `originSelectionRange` (LocationLink) est dans le document de la requête.
    if let (Some(range), Some((fs, alias))) = (obj.get_mut("originSelectionRange"), request_ctx)
        && let Some((r, _)) = fs.doc_to_src(range, enc, alias)
    {
        *range = r;
    }

    for (key, child) in obj.iter_mut() {
        if matches!(key.as_str(), "data" | "changes" | "textDocument" | "command") || RANGE_KEYS.contains(&key.as_str())
        {
            continue;
        }
        if child.is_object() || child.is_array() {
            let child_ctx = if owned_uri { local } else { ctx };
            if !map_value(st, child, child_ctx) {
                *child = Value::Null;
            }
        }
    }
    keep
}

/// Tokens sémantiques du document rust-analyzer → tokens du `.rsc`, plus les
/// mots-clés Crust (`class`, `over`, `import`, `from`) qui n'ont pas d'équivalent.
fn semantic_tokens(fs: &FileState, result: &mut Value, enc: Encoding, keyword: Option<u64>) {
    let Some(data) = result["data"].as_array() else { return };
    let nums: Vec<u64> = data.iter().filter_map(Value::as_u64).collect();
    let (mut line, mut col) = (0u64, 0u64);
    let mut out: Vec<(usize, usize, u64, u64, u64)> = Vec::new(); // (ligne, col, long, type, mods)
    let mut covered: Vec<(usize, usize)> = Vec::new(); // plages d'octets .rsc
    for t in nums.chunks_exact(5) {
        if t[0] > 0 {
            line += t[0];
            col = t[1];
        } else {
            col += t[1];
        }
        let doc_off = offset_of(&fs.doc_text, &json!({ "line": line, "character": col }), enc);
        let doc_end = offset_of(&fs.doc_text, &json!({ "line": line, "character": col + t[2] }), enc);
        if doc_off < fs.offset || doc_end > fs.offset + fs.gen_len || doc_end <= doc_off {
            continue;
        }
        let Some(m) = fs
            .map
            .gen_span_to_src(crust_syntax::Span::new(doc_off - fs.offset, doc_end - fs.offset))
        else {
            continue;
        };
        if !m.exact || fs.src_text.get(m.span.start..m.span.end) != fs.doc_text.get(doc_off..doc_end) {
            continue;
        }
        if covered.iter().any(|&(s, _)| s == m.span.start) {
            continue; // nom recopié plusieurs fois (struct, impl…)
        }
        covered.push((m.span.start, m.span.end));
        let p = position_of(&fs.src_text, m.span.start, enc);
        out.push((
            p["line"].as_u64().unwrap() as usize,
            p["character"].as_u64().unwrap() as usize,
            t[2],
            t[3],
            t[4],
        ));
    }
    if let Some(kw) = keyword {
        for tok in tokenize(&fs.src_text) {
            let text = &fs.src_text[tok.start..tok.end];
            if tok.kind == TokenKind::Ident
                && matches!(text, "class" | "over" | "import" | "from")
                && !covered.iter().any(|&(s, e)| s < tok.end && tok.start < e)
            {
                let p = position_of(&fs.src_text, tok.start, enc);
                out.push((
                    p["line"].as_u64().unwrap() as usize,
                    p["character"].as_u64().unwrap() as usize,
                    text.len() as u64,
                    kw,
                    0,
                ));
            }
        }
    }
    out.sort();
    let mut encoded = Vec::with_capacity(out.len() * 5);
    let (mut pl, mut pc) = (0usize, 0usize);
    for (l, c, len, ty, mods) in out {
        let dl = l - pl;
        let dc = if dl == 0 { c - pc } else { c };
        encoded.extend([dl as u64, dc as u64, len, ty, mods]);
        pl = l;
        pc = c;
    }
    result["data"] = json!(encoded);
    if let Some(o) = result.as_object_mut() {
        o.remove("resultId");
    }
}
