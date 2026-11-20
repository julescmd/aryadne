//! Client LSP minimal : complétion, erreurs, aller à la définition, survol.
//!
//! Utilise les serveurs déjà installés sur l'ordinateur (rust-analyzer, pyright, pylsp,
//! typescript-language-server, clangd, gopls, lua-language-server, bash-language-server).
//! Un serveur par langage et par projet, partagé par tous les fichiers ouverts.
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, LazyLock, Mutex};

/// (extensions, languageId, commandes possibles)
const SERVERS: &[(&[&str], &str, &[&[&str]])] = &[
    (&["rs"], "rust", &[&["rust-analyzer"]]),
    (&["py", "pyw"], "python", &[&["pyright-langserver", "--stdio"], &["basedpyright-langserver", "--stdio"], &["pylsp"]]),
    (&["ts", "mts", "cts"], "typescript", &[&["typescript-language-server", "--stdio"]]),
    (&["tsx"], "typescriptreact", &[&["typescript-language-server", "--stdio"]]),
    (&["js", "mjs", "cjs"], "javascript", &[&["typescript-language-server", "--stdio"]]),
    (&["jsx"], "javascriptreact", &[&["typescript-language-server", "--stdio"]]),
    (&["c", "h"], "c", &[&["clangd"]]),
    (&["cpp", "cc", "cxx", "hpp", "hh", "hxx"], "cpp", &[&["clangd"]]),
    (&["go"], "go", &[&["gopls"]]),
    (&["lua"], "lua", &[&["lua-language-server"]]),
    (&["sh", "bash"], "shellscript", &[&["bash-language-server", "start"]]),
];

/// Cherche un programme dans le PATH (avec .exe / .cmd sous Windows).
pub fn which(name: &str) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ".bat", ""] } else { &[""] };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(h) = dirs::home_dir() {
        // Endroits fréquents qui ne sont pas toujours dans le PATH d'une app graphique
        dirs.extend([h.join(".cargo/bin"), h.join(".local/bin"), h.join("go/bin"), h.join(".npm-global/bin")]);
        if cfg!(windows) {
            if let Some(a) = dirs::data_dir() {
                dirs.push(a.join("npm"));
            }
        }
    }
    for d in dirs {
        for e in exts {
            let p = d.join(format!("{name}{e}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Serveur disponible pour cette extension : (languageId, commande).
pub fn server_for(ext: &str) -> Option<(&'static str, Vec<String>, PathBuf)> {
    let ext = ext.to_lowercase();
    let (_, lang, cmds) = SERVERS.iter().find(|(exts, _, _)| exts.contains(&ext.as_str()))?;
    for c in cmds.iter() {
        if let Some(p) = which(c[0]) {
            return Some((lang, c[1..].iter().map(|s| s.to_string()).collect(), p));
        }
    }
    None
}

/// Nom du serveur conseillé pour une extension (pour l'indiquer à l'utilisateur).
pub fn suggested(ext: &str) -> Option<&'static str> {
    SERVERS.iter().find(|(exts, _, _)| exts.contains(&ext)).map(|(_, _, c)| c[0][0])
}

pub fn to_uri(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let mut out = String::from(if s.starts_with('/') { "file://" } else { "file:///" });
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => out.push(b as char),
            b':' if cfg!(windows) => out.push(':'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn from_uri(u: &str) -> Option<PathBuf> {
    let rest = u.strip_prefix("file://")?;
    let mut bytes = vec![];
    let b = rest.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                bytes.push(v);
                i += 3;
                continue;
            }
        }
        bytes.push(b[i]);
        i += 1;
    }
    let s = String::from_utf8_lossy(&bytes).to_string();
    // Windows : « /C:/… » → « C:/… »
    let s = if s.len() > 2 && s.as_bytes()[0] == b'/' && s.as_bytes()[2] == b':' { s[1..].to_string() } else { s };
    Some(PathBuf::from(s))
}

/// Position LSP (ligne, colonne en UTF-16) d'un index d'octet.
pub fn position(text: &str, byte: usize) -> (u32, u32) {
    let b = byte.min(text.len());
    let ls = text[..b].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line = text[..ls].matches('\n').count() as u32;
    let col = text[ls..b].chars().map(|c| c.len_utf16() as u32).sum();
    (line, col)
}

// ---------- serveur ----------

pub struct Server {
    out: Sender<Value>,
    next: AtomicI64,
    pending: Mutex<HashMap<i64, Sender<Value>>>,
    pub diags: Mutex<HashMap<String, (u64, Vec<(usize, String)>)>>,
    pub dead: Mutex<Option<String>>,
    child: Mutex<Option<Child>>,
    pub name: String,
}

impl Server {
    fn notify(&self, method: &str, params: Value) {
        let _ = self.out.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }
    fn request(&self, method: &str, params: Value) -> Receiver<Value> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        if let Ok(mut p) = self.pending.lock() {
            p.insert(id, tx);
        }
        let _ = self.out.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        rx
    }
    fn kill(&self) {
        if let Ok(mut c) = self.child.lock() {
            if let Some(mut ch) = c.take() {
                let _ = ch.kill();
            }
        }
    }
}

static REGISTRY: LazyLock<Mutex<HashMap<(String, &'static str), Arc<Server>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static DIAG_TICK: AtomicI64 = AtomicI64::new(0);

/// Arrête tous les serveurs (à la fermeture d'Aryadne).
pub fn shutdown_all() {
    if let Ok(mut r) = REGISTRY.lock() {
        for s in r.values() {
            s.kill();
        }
        r.clear();
    }
}

fn read_message(r: &mut impl BufRead) -> Option<Value> {
    let mut len = 0usize;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let l = line.trim();
        if l.is_empty() {
            break;
        }
        if let Some(v) = l.strip_prefix("Content-Length:") {
            len = v.trim().parse().ok()?;
        }
    }
    let mut buf = vec![0; len];
    r.read_exact(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

fn project_root(file: &Path, vault_root: &Path) -> PathBuf {
    let markers = ["Cargo.toml", "package.json", "tsconfig.json", "pyproject.toml", "setup.py", "go.mod", "compile_commands.json", ".git"];
    let mut d = file.parent();
    while let Some(dir) = d {
        if markers.iter().any(|m| dir.join(m).exists()) {
            return dir.to_path_buf();
        }
        if dir == vault_root {
            break;
        }
        d = dir.parent();
    }
    file.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| vault_root.to_path_buf())
}

fn start(lang: &'static str, args: Vec<String>, exe: PathBuf, root: PathBuf, ctx: eframe::egui::Context) -> Result<Arc<Server>, String> {
    let mut cmd = Command::new(&exe);
    cmd.args(&args).current_dir(&root).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // pas de fenêtre de console
    }
    let mut child = cmd.spawn().map_err(|e| format!("{} : {e}", exe.display()))?;
    let mut stdin = child.stdin.take().ok_or("stdin")?;
    let stdout = child.stdout.take().ok_or("stdout")?;
    let (out_tx, out_rx) = channel::<Value>();
    let name = exe.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let server = Arc::new(Server { out: out_tx, next: AtomicI64::new(2), pending: Mutex::new(HashMap::new()), diags: Mutex::new(HashMap::new()), dead: Mutex::new(None), child: Mutex::new(Some(child)), name });
    // Lecture des réponses et des notifications
    let (init_tx, init_rx) = channel::<Value>();
    {
        let s = server.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut r = BufReader::new(stdout);
            while let Some(msg) = read_message(&mut r) {
                if let Some(id) = msg["id"].as_i64() {
                    if msg.get("method").is_some() {
                        // Requête du serveur (configuration, progression…) : réponse vide
                        let _ = s.out.send(json!({ "jsonrpc": "2.0", "id": msg["id"], "result": Value::Null }));
                        continue;
                    }
                    if id == 1 {
                        let _ = init_tx.send(msg);
                        continue;
                    }
                    if let Some(tx) = s.pending.lock().ok().and_then(|mut p| p.remove(&id)) {
                        let _ = tx.send(msg["result"].clone());
                    }
                    ctx.request_repaint();
                } else if msg["method"] == "textDocument/publishDiagnostics" {
                    let uri = msg["params"]["uri"].as_str().unwrap_or("").to_string();
                    let list: Vec<(usize, String)> = msg["params"]["diagnostics"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter(|d| d["severity"].as_i64().unwrap_or(1) == 1)
                                .map(|d| (d["range"]["start"]["line"].as_u64().unwrap_or(0) as usize, d["message"].as_str().unwrap_or("").lines().next().unwrap_or("").to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    let tick = DIAG_TICK.fetch_add(1, Ordering::Relaxed) as u64 + 1;
                    if let Ok(mut d) = s.diags.lock() {
                        d.insert(uri, (tick, list));
                    }
                    ctx.request_repaint();
                }
            }
            if let Ok(mut d) = s.dead.lock() {
                *d = Some("le serveur s'est arrêté".into());
            }
        });
    }
    // Écriture : poignée de main, puis la file des messages
    let root_uri = to_uri(&root);
    std::thread::spawn(move || {
        let write = |stdin: &mut std::process::ChildStdin, v: &Value| -> std::io::Result<()> {
            let body = serde_json::to_vec(v).unwrap_or_default();
            stdin.write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())?;
            stdin.write_all(&body)?;
            stdin.flush()
        };
        let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "processId": std::process::id(),
            "rootUri": root_uri,
            "workspaceFolders": [ { "uri": root_uri, "name": "aryadne" } ],
            "capabilities": {
                "textDocument": {
                    "synchronization": { "didSave": true },
                    "completion": { "completionItem": { "snippetSupport": false, "documentationFormat": ["plaintext"] } },
                    "hover": { "contentFormat": ["plaintext", "markdown"] },
                    "definition": { "linkSupport": false },
                    "publishDiagnostics": {}
                },
                "general": { "positionEncodings": ["utf-16"] }
            },
            "clientInfo": { "name": "aryadne" }
        }});
        if write(&mut stdin, &init).is_err() {
            return;
        }
        if init_rx.recv_timeout(std::time::Duration::from_secs(60)).is_err() {
            return;
        }
        let _ = write(&mut stdin, &json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
        let _ = lang;
        while let Ok(v) = out_rx.recv() {
            if write(&mut stdin, &v).is_err() {
                break;
            }
        }
    });
    Ok(server)
}

// ---------- document ouvert ----------

pub enum Wait {
    Completion(Receiver<Value>, usize),
    Definition(Receiver<Value>),
    Hover(Receiver<Value>, usize),
}

pub struct Doc {
    pub server: Arc<Server>,
    pub uri: String,
    version: i64,
    last: u64,
    seen_diags: u64,
    pub waits: Vec<Wait>,
    /// Dernière complétion reçue : (curseur en octets, éléments).
    pub completion: Option<(usize, Vec<String>)>,
    pub hover: Option<(usize, String)>,
    pub hover_at: Option<(usize, std::time::Instant)>,
    pub hover_asked: Option<usize>,
    /// Définition dans un autre fichier : (chemin, ligne).
    pub goto: Option<(PathBuf, usize)>,
}

fn hash(t: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut h);
    h.finish()
}

impl Doc {
    /// Ouvre le fichier auprès du serveur du langage (démarré au besoin). None si aucun serveur.
    pub fn open(vault_root: &str, path: &Path, text: &str, ctx: &eframe::egui::Context) -> Option<Doc> {
        let ext = path.extension()?.to_string_lossy().to_string();
        let (lang, args, exe) = server_for(&ext)?;
        let root = project_root(path, Path::new(vault_root));
        let key = (root.to_string_lossy().to_string(), lang);
        let server = {
            let mut reg = REGISTRY.lock().ok()?;
            let alive = reg.get(&key).is_some_and(|s| s.dead.lock().map(|d| d.is_none()).unwrap_or(false));
            if !alive {
                let s = start(lang, args, exe, root, ctx.clone()).ok()?;
                reg.insert(key.clone(), s);
            }
            reg.get(&key)?.clone()
        };
        let uri = to_uri(path);
        server.notify("textDocument/didOpen", json!({ "textDocument": { "uri": uri, "languageId": lang, "version": 1, "text": text } }));
        Some(Doc { server, uri, version: 1, last: hash(text), seen_diags: 0, waits: vec![], completion: None, hover: None, hover_at: None, hover_asked: None, goto: None })
    }

    /// Envoie le texte s'il a changé (synchro complète, simple et robuste).
    pub fn sync(&mut self, text: &str) {
        let h = hash(text);
        if h == self.last {
            return;
        }
        self.last = h;
        self.version += 1;
        self.server.notify("textDocument/didChange", json!({ "textDocument": { "uri": self.uri, "version": self.version }, "contentChanges": [ { "text": text } ] }));
    }

    pub fn saved(&self) {
        self.server.notify("textDocument/didSave", json!({ "textDocument": { "uri": self.uri } }));
    }

    fn pos(&self, text: &str, byte: usize) -> Value {
        let (line, character) = position(text, byte);
        json!({ "textDocument": { "uri": self.uri }, "position": { "line": line, "character": character } })
    }

    pub fn ask_completion(&mut self, text: &str, byte: usize) {
        self.sync(text);
        self.waits.retain(|w| !matches!(w, Wait::Completion(..)));
        let rx = self.server.request("textDocument/completion", self.pos(text, byte));
        self.waits.push(Wait::Completion(rx, byte));
    }

    pub fn ask_definition(&mut self, text: &str, byte: usize) {
        self.sync(text);
        let rx = self.server.request("textDocument/definition", self.pos(text, byte));
        self.waits.push(Wait::Definition(rx));
    }

    pub fn ask_hover(&mut self, text: &str, byte: usize) {
        self.sync(text);
        self.waits.retain(|w| !matches!(w, Wait::Hover(..)));
        let rx = self.server.request("textDocument/hover", self.pos(text, byte));
        self.waits.push(Wait::Hover(rx, byte));
    }

    /// Nouvelles erreurs du serveur depuis le dernier appel : (ligne, message).
    pub fn new_diags(&mut self) -> Option<Vec<(usize, String)>> {
        let d = self.server.diags.lock().ok()?;
        let (tick, list) = d.get(&self.uri)?;
        if *tick == self.seen_diags {
            return None;
        }
        self.seen_diags = *tick;
        Some(list.clone())
    }

    /// Relève les réponses arrivées. Retourne une ligne où sauter (définition dans ce fichier).
    pub fn poll(&mut self) -> Option<usize> {
        let mut jump = None;
        let waits = std::mem::take(&mut self.waits);
        for w in waits {
            match w {
                Wait::Completion(rx, at) => match rx.try_recv() {
                    Ok(v) => {
                        let items = v.get("items").unwrap_or(&v);
                        let mut words: Vec<(String, String)> = items
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|it| {
                                        let w = it["textEdit"]["newText"].as_str().or(it["insertText"].as_str()).or(it["label"].as_str())?;
                                        let w = w.split(['(', '$']).next().unwrap_or(w).trim().to_string();
                                        let sort = it["sortText"].as_str().unwrap_or(it["label"].as_str().unwrap_or("")).to_string();
                                        (!w.is_empty()).then_some((sort, w))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        words.sort();
                        let mut seen = std::collections::HashSet::new();
                        let list = words.into_iter().map(|(_, w)| w).filter(|w| seen.insert(w.clone())).take(200).collect();
                        self.completion = Some((at, list));
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => self.waits.push(Wait::Completion(rx, at)),
                    Err(_) => {}
                },
                Wait::Definition(rx) => match rx.try_recv() {
                    Ok(v) => {
                        let loc = if v.is_array() { v[0].clone() } else { v };
                        let uri = loc["uri"].as_str().or(loc["targetUri"].as_str()).unwrap_or("");
                        let range = if loc["range"].is_object() { &loc["range"] } else { &loc["targetSelectionRange"] };
                        let line = range["start"]["line"].as_u64().unwrap_or(0) as usize;
                        if uri == self.uri {
                            jump = Some(line);
                        } else if let Some(p) = from_uri(uri) {
                            self.goto = Some((p, line));
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => self.waits.push(Wait::Definition(rx)),
                    Err(_) => {}
                },
                Wait::Hover(rx, at) => match rx.try_recv() {
                    Ok(v) => {
                        let c = &v["contents"];
                        let text = if let Some(s) = c.as_str() {
                            s.to_string()
                        } else if let Some(s) = c["value"].as_str() {
                            s.to_string()
                        } else if let Some(a) = c.as_array() {
                            a.iter().filter_map(|x| x.as_str().or(x["value"].as_str())).collect::<Vec<_>>().join("\n")
                        } else {
                            String::new()
                        };
                        let text: String = text.lines().filter(|l| !l.trim_start().starts_with("```")).take(24).collect::<Vec<_>>().join("\n");
                        self.hover = (!text.trim().is_empty()).then(|| (at, text.trim().to_string()));
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => self.waits.push(Wait::Hover(rx, at)),
                    Err(_) => {}
                },
            }
        }
        jump
    }

    pub fn close(&self) {
        self.server.notify("textDocument/didClose", json!({ "textDocument": { "uri": self.uri } }));
    }
}

impl Drop for Doc {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Avec un vrai serveur : ARYADNE_LSP_TEST=1 cargo test serveur_python
    #[test]
    fn serveur_python() {
        if std::env::var("ARYADNE_LSP_TEST").is_err() || server_for("py").is_none() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("aryadne-lsp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.py");
        let text = "import os\n\ndef bonjour(nom):\n    return nom\n\nos.pa\nbonjour(1)\nx = = 2\n";
        std::fs::write(&path, text).unwrap();
        let ctx = eframe::egui::Context::default();
        let mut d = Doc::open(dir.to_str().unwrap(), &path, text, &ctx).expect("serveur");
        let t0 = std::time::Instant::now();
        d.ask_completion(text, text.find("os.pa").unwrap() + 5);
        d.ask_definition(text, text.find("bonjour(1)").unwrap() + 2);
        let mut diags = None;
        let mut jump = None;
        while t0.elapsed() < std::time::Duration::from_secs(60) && (d.completion.is_none() || jump.is_none() || diags.is_none()) {
            if let Some(j) = d.poll() {
                jump = Some(j);
            }
            if let Some(x) = d.new_diags() {
                diags = Some(x);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let (_, items) = d.completion.clone().expect("complétion");
        assert!(items.iter().any(|w| w == "path"), "{items:?}");
        assert_eq!(jump, Some(2));
        assert!(diags.unwrap().iter().any(|(l, _)| *l == 7));
        shutdown_all();
    }

    #[test]
    fn uri_et_positions() {
        let p = Path::new("/tmp/a b/é.rs");
        let u = to_uri(p);
        assert_eq!(u, "file:///tmp/a%20b/%C3%A9.rs");
        assert_eq!(from_uri(&u).unwrap(), p);
        assert_eq!(position("ab\ncé😀x", "ab\ncé😀".len()), (1, 4));
    }
}
