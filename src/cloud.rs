//! Coffre en ligne : synchronisation avec n'importe quel stockage compatible S3
//! (Cloudflare R2, Backblaze B2, AWS S3, MinIO…).
//!
//! Aucune adresse ni clé n'est écrite dans le code : chaque personne branche son propre
//! stockage dans les réglages. Les identifiants restent dans le dossier de config local
//! (jamais dans le coffre, donc jamais synchronisés ni partagés).
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Config {
    /// Ex. https://<id-de-compte>.r2.cloudflarestorage.com
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    /// « auto » pour R2.
    #[serde(default)]
    pub region: String,
    /// Synchroniser au démarrage puis toutes les N minutes (0 = à la main seulement).
    #[serde(default)]
    pub every_min: u32,
    /// Synchro complète du coffre (sinon : relais temporaire seulement).
    #[serde(default)]
    pub full_sync: bool,
}

impl Config {
    fn file() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("aryadne").join("cloud.json"))
    }
    pub fn load() -> Self {
        Self::file().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    pub fn save(&self) {
        if let Some(f) = Self::file() {
            let _ = std::fs::create_dir_all(f.parent().unwrap());
            let _ = std::fs::write(&f, serde_json::to_string_pretty(self).unwrap_or_default());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
    pub fn ready(&self) -> bool {
        !self.endpoint.trim().is_empty() && !self.bucket.trim().is_empty() && !self.access_key.trim().is_empty() && !self.secret_key.trim().is_empty()
    }
    fn region(&self) -> &str {
        if self.region.trim().is_empty() { "auto" } else { self.region.trim() }
    }
    fn host(&self) -> String {
        self.endpoint.trim().trim_end_matches('/').trim_start_matches("https://").trim_start_matches("http://").to_string()
    }
}

// ---------- signature AWS v4 ----------

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn sha(b: &[u8]) -> String {
    hex(&Sha256::digest(b))
}

fn hmac(key: &[u8], msg: &str) -> Vec<u8> {
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("hmac");
    m.update(msg.as_bytes());
    m.finalize().into_bytes().to_vec()
}

/// Encodage d'URI façon S3 (les « / » gardés si `path`).
fn enc(s: &str, path: bool) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(b as char),
            b'/' if path => o.push('/'),
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}

struct Signed {
    url: String,
    headers: Vec<(String, String)>,
}

fn sign(c: &Config, method: &str, key: &str, query: &[(&str, String)], body: &[u8]) -> Signed {
    let now = chrono::Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date = now.format("%Y%m%d").to_string();
    let host = c.host();
    let path = if key.is_empty() { format!("/{}", enc(c.bucket.trim(), false)) } else { format!("/{}/{}", enc(c.bucket.trim(), false), enc(key, true)) };
    let mut q: Vec<(String, String)> = query.iter().map(|(k, v)| (enc(k, false), enc(v, false))).collect();
    q.sort();
    let qs = q.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&");
    let payload = sha(body);
    let canonical_headers = format!("host:{host}\nx-amz-content-sha256:{payload}\nx-amz-date:{amz_date}\n");
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical = format!("{method}\n{path}\n{qs}\n{canonical_headers}\n{signed_headers}\n{payload}");
    let scope = format!("{date}/{}/s3/aws4_request", c.region());
    let to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}", sha(canonical.as_bytes()));
    let k = hmac(format!("AWS4{}", c.secret_key.trim()).as_bytes(), &date);
    let k = hmac(&k, c.region());
    let k = hmac(&k, "s3");
    let k = hmac(&k, "aws4_request");
    let sig = hex(&hmac(&k, &to_sign));
    let auth = format!("AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={sig}", c.access_key.trim());
    let scheme = if c.endpoint.trim().starts_with("http://") { "http" } else { "https" };
    let url = if qs.is_empty() { format!("{scheme}://{host}{path}") } else { format!("{scheme}://{host}{path}?{qs}") };
    Signed { url, headers: vec![("x-amz-date".into(), amz_date), ("x-amz-content-sha256".into(), payload), ("Authorization".into(), auth)] }
}

fn err_text(e: ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(403) => "accès refusé (vérifie les clés et les droits du jeton)".into(),
        ureq::Error::StatusCode(404) => "bucket introuvable".into(),
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Io(_) => "impossible de joindre le stockage (adresse ou connexion)".into(),
        other => other.to_string(),
    }
}

fn agent() -> ureq::Agent {
    crate::ollama::agent(300)
}

pub struct Remote {
    pub etag: String,
    pub size: u64,
    /// Date ISO 8601 (comparable comme texte).
    pub modified: String,
}

fn unxml(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

fn tag<'a>(s: &'a str, t: &str) -> Option<&'a str> {
    let a = s.find(&format!("<{t}>"))? + t.len() + 2;
    let b = s[a..].find(&format!("</{t}>"))? + a;
    Some(&s[a..b])
}

pub fn list(c: &Config, prefix: &str) -> Result<BTreeMap<String, Remote>, String> {
    let mut out = BTreeMap::new();
    let mut token: Option<String> = None;
    loop {
        let mut q = vec![("list-type", "2".to_string()), ("prefix", prefix.to_string())];
        if let Some(t) = &token {
            q.push(("continuation-token", t.clone()));
        }
        let s = sign(c, "GET", "", &q, b"");
        let mut req = agent().get(&s.url);
        for (k, v) in &s.headers {
            req = req.header(k, v);
        }
        let mut resp = req.call().map_err(err_text)?;
        let body = resp.body_mut().with_config().limit(64 << 20).read_to_string().map_err(|e| e.to_string())?;
        for part in body.split("<Contents>").skip(1) {
            let Some(key) = tag(part, "Key") else { continue };
            let key = unxml(key);
            let etag = tag(part, "ETag").map(|e| unxml(e).trim_matches('"').to_string()).unwrap_or_default();
            let size = tag(part, "Size").and_then(|s| s.parse().ok()).unwrap_or(0);
            let modified = tag(part, "LastModified").unwrap_or("").to_string();
            if let Some(rel) = key.strip_prefix(prefix) {
                if !rel.is_empty() && !rel.ends_with('/') {
                    out.insert(rel.to_string(), Remote { etag, size, modified });
                }
            }
        }
        if tag(&body, "IsTruncated") == Some("true") {
            token = tag(&body, "NextContinuationToken").map(unxml);
            if token.is_none() {
                break;
            }
        } else {
            break;
        }
    }
    Ok(out)
}

pub fn put(c: &Config, key: &str, data: &[u8]) -> Result<String, String> {
    let s = sign(c, "PUT", key, &[], data);
    let mut req = agent().put(&s.url);
    for (k, v) in &s.headers {
        req = req.header(k, v);
    }
    let resp = req.send(data).map_err(err_text)?;
    Ok(resp.headers().get("etag").and_then(|v| v.to_str().ok()).unwrap_or("").trim_matches('"').to_string())
}

pub fn get(c: &Config, key: &str) -> Result<Vec<u8>, String> {
    let s = sign(c, "GET", key, &[], b"");
    let mut req = agent().get(&s.url);
    for (k, v) in &s.headers {
        req = req.header(k, v);
    }
    let mut resp = req.call().map_err(err_text)?;
    resp.body_mut().with_config().limit(1 << 31).read_to_vec().map_err(|e| e.to_string())
}

pub fn delete(c: &Config, key: &str) -> Result<(), String> {
    let s = sign(c, "DELETE", key, &[], b"");
    let mut req = agent().delete(&s.url);
    for (k, v) in &s.headers {
        req = req.header(k, v);
    }
    req.call().map_err(err_text)?;
    Ok(())
}

/// Vérifie la connexion (liste un seul objet).
pub fn test(c: &Config) -> Result<usize, String> {
    list(c, &format!("{}/", "aryadne-test-inexistant")).map(|m| m.len())
}

// ---------- synchronisation ----------

/// État au dernier passage : pour chaque fichier, l'etag distant et la taille/date locales.
#[derive(Default, Serialize, Deserialize, Clone)]
struct Known {
    etag: String,
    size: u64,
    mtime: u64,
}

const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", "__pycache__", ".venv", "cache", "backups", "history", "thumbs"];
const MAX_SIZE: u64 = 500 << 20;

fn local_files(root: &Path) -> BTreeMap<String, (u64, u64)> {
    let mut out = BTreeMap::new();
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, (u64, u64)>, depth: usize) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Ok(meta) = std::fs::symlink_metadata(e.path()) else { continue };
            if meta.file_type().is_symlink() {
                continue; // dossiers liés : restent sur cet ordinateur
            }
            let rel = e.path().strip_prefix(base).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
            if meta.is_dir() {
                let top_hidden = name.starts_with('.') && name != ".app";
                if !top_hidden && !SKIP_DIRS.contains(&name.as_str()) && depth < 24 {
                    walk(base, &e.path(), out, depth + 1);
                }
            } else if !(name.starts_with(".~") || name.ends_with(".tmp") || name == ".DS_Store" || name == "Thumbs.db") && meta.len() <= MAX_SIZE {
                if rel == ".app/sync.json" || rel.starts_with(".app/links") {
                    continue;
                }
                let m = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
                out.insert(rel, (meta.len(), m));
            }
        }
    }
    walk(root, root, &mut out, 0);
    out
}

fn state_file(root: &Path) -> PathBuf {
    root.join(".app").join("sync.json")
}

#[derive(Default, Clone)]
pub struct Progress {
    pub running: bool,
    pub step: String,
    pub done: usize,
    pub total: usize,
    pub up: usize,
    pub down: usize,
    pub deleted: usize,
    pub conflicts: Vec<String>,
    pub error: Option<String>,
    pub finished_at: Option<std::time::Instant>,
}

#[derive(Clone, Default)]
pub struct Sync {
    pub progress: Arc<Mutex<Progress>>,
    pub stop: Arc<AtomicBool>,
}

impl Sync {
    pub fn snapshot(&self) -> Progress {
        self.progress.lock().map(|p| p.clone()).unwrap_or_default()
    }
    pub fn busy(&self) -> bool {
        self.snapshot().running
    }

    /// Lance une synchronisation complète en arrière-plan.
    pub fn start(&self, cfg: Config, root: String, prefix: String, ctx: eframe::egui::Context) {
        if self.busy() || !cfg.ready() {
            return;
        }
        self.stop.store(false, Ordering::Relaxed);
        if let Ok(mut p) = self.progress.lock() {
            *p = Progress { running: true, step: "liste des fichiers…".into(), ..Default::default() };
        }
        let (prog, stop) = (self.progress.clone(), self.stop.clone());
        std::thread::spawn(move || {
            let r = run(&cfg, Path::new(&root), &prefix, &prog, &stop, &ctx);
            if let Ok(mut p) = prog.lock() {
                p.running = false;
                p.step.clear();
                p.error = r.err();
                p.finished_at = Some(std::time::Instant::now());
            }
            ctx.request_repaint();
        });
    }
}

/// Dossier du coffre dans le bucket (un bucket peut garder plusieurs coffres).
pub fn prefix_for(root: &str) -> String {
    let name = Path::new(root).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "coffre".into());
    format!("coffres/{name}/")
}

fn conflict_name(rel: &str) -> String {
    let p = Path::new(rel);
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let stamp = chrono::Local::now().format("%Y-%m-%d %H%M");
    let file = match p.extension() {
        Some(e) => format!("{stem} (conflit {stamp}).{}", e.to_string_lossy()),
        None => format!("{stem} (conflit {stamp})"),
    };
    match p.parent().map(|d| d.to_string_lossy().replace('\\', "/")).filter(|d| !d.is_empty()) {
        Some(d) => format!("{d}/{file}"),
        None => file,
    }
}

enum Act {
    Up,
    Down,
    DelRemote,
    DelLocal,
    Conflict,
}

/// Décide quoi faire pour chaque fichier (fonction pure, testée).
fn plan(local: &BTreeMap<String, (u64, u64)>, remote: &BTreeMap<String, Remote>, known: &BTreeMap<String, Known>) -> Vec<(String, Act)> {
    let all: BTreeSet<&String> = local.keys().chain(remote.keys()).chain(known.keys()).collect();
    let mut out = vec![];
    for rel in all {
        let l = local.get(rel);
        let r = remote.get(rel);
        let k = known.get(rel);
        let local_changed = match (l, k) {
            (Some(&(size, mtime)), Some(k)) => size != k.size || mtime != k.mtime,
            (Some(_), None) => true,
            (None, _) => false,
        };
        let remote_changed = match (r, k) {
            (Some(r), Some(k)) => r.etag != k.etag,
            (Some(_), None) => true,
            (None, _) => false,
        };
        let act = match (l, r, k) {
            (Some(_), Some(_), None) => {
                // Les deux existent sans historique : même taille → on considère pareil
                if l.unwrap().0 == r.unwrap().size { None } else { Some(Act::Conflict) }
            }
            (Some(_), Some(_), Some(_)) => match (local_changed, remote_changed) {
                (false, false) => None,
                (true, false) => Some(Act::Up),
                (false, true) => Some(Act::Down),
                (true, true) => Some(Act::Conflict),
            },
            (Some(_), None, None) => Some(Act::Up),
            (Some(_), None, Some(_)) => if local_changed { Some(Act::Up) } else { Some(Act::DelLocal) },
            (None, Some(_), None) => Some(Act::Down),
            (None, Some(_), Some(_)) => if remote_changed { Some(Act::Down) } else { Some(Act::DelRemote) },
            (None, None, _) => None,
        };
        if let Some(a) = act {
            out.push((rel.clone(), a));
        }
    }
    out
}

fn run(c: &Config, root: &Path, prefix: &str, prog: &Arc<Mutex<Progress>>, stop: &AtomicBool, ctx: &eframe::egui::Context) -> Result<(), String> {
    let set = |f: &dyn Fn(&mut Progress)| {
        if let Ok(mut p) = prog.lock() {
            f(&mut p);
        }
        ctx.request_repaint();
    };
    let remote = list(c, prefix)?;
    let local = local_files(root);
    let mut known: BTreeMap<String, Known> = std::fs::read_to_string(state_file(root)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let actions = plan(&local, &remote, &known);
    // Garde-fou : refuse d'effacer d'un coup une grosse partie du coffre (mauvais bucket, disque débranché…)
    let del_local = actions.iter().filter(|a| matches!(a.1, Act::DelLocal)).count();
    let del_remote = actions.iter().filter(|a| matches!(a.1, Act::DelRemote)).count();
    if known.len() >= 10 && (del_local * 2 > known.len() || del_remote * 2 > known.len()) {
        return Err(format!("arrêté par sécurité : la synchro voulait supprimer {} fichiers. Vérifie le bucket et le dossier, ou réinitialise la synchro dans les réglages.", del_local.max(del_remote)));
    }
    let total = actions.len();
    set(&|p| p.total = total);
    let save_known = |k: &BTreeMap<String, Known>| {
        let _ = std::fs::create_dir_all(root.join(".app"));
        let _ = std::fs::write(state_file(root), serde_json::to_string(k).unwrap_or_default());
    };
    let stat = |rel: &str| -> (u64, u64) {
        let m = std::fs::metadata(root.join(rel)).ok();
        let size = m.as_ref().map(|m| m.len()).unwrap_or(0);
        let mt = m.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
        (size, mt)
    };
    let write_local = |rel: &str, data: &[u8]| -> Result<(), String> {
        let path = root.join(rel);
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("aryadne-sync.tmp");
        std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
    };
    // Unicode / séparateurs : les chemins sont toujours en « / »
    for (i, (rel, act)) in actions.into_iter().enumerate() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let key = format!("{prefix}{rel}");
        set(&|p| {
            p.done = i;
            p.step = rel.clone();
        });
        match act {
            Act::Up => {
                let data = std::fs::read(root.join(&rel)).map_err(|e| format!("{rel} : {e}"))?;
                let etag = put(c, &key, &data)?;
                let (size, mtime) = stat(&rel);
                known.insert(rel.clone(), Known { etag, size, mtime });
                set(&|p| p.up += 1);
            }
            Act::Down => {
                let data = get(c, &key)?;
                write_local(&rel, &data)?;
                let (size, mtime) = stat(&rel);
                let etag = remote.get(&rel).map(|r| r.etag.clone()).unwrap_or_default();
                known.insert(rel.clone(), Known { etag, size, mtime });
                set(&|p| p.down += 1);
            }
            Act::DelRemote => {
                delete(c, &key)?;
                known.remove(&rel);
                set(&|p| p.deleted += 1);
            }
            Act::DelLocal => {
                // Vers la corbeille du coffre plutôt qu'un effacement définitif
                let r = root.to_string_lossy().to_string();
                if crate::vault::trash(&r, &rel).is_err() {
                    let _ = std::fs::remove_file(root.join(&rel));
                }
                known.remove(&rel);
                set(&|p| p.deleted += 1);
            }
            Act::Conflict => {
                // On garde les deux : la version en ligne arrive à côté, la locale est envoyée
                let data = get(c, &key)?;
                let other = conflict_name(&rel);
                write_local(&other, &data)?;
                let mine = std::fs::read(root.join(&rel)).map_err(|e| format!("{rel} : {e}"))?;
                let etag = put(c, &key, &mine)?;
                let (size, mtime) = stat(&rel);
                known.insert(rel.clone(), Known { etag, size, mtime });
                set(&|p| p.conflicts.push(other.clone()));
            }
        }
        if i % 20 == 0 {
            save_known(&known);
        }
    }
    save_known(&known);
    set(&|p| p.done = p.total);
    Ok(())
}

// ---------- relais temporaire ----------
//
// Pour envoyer des fichiers d'un ordinateur à l'autre par internet : ils passent par
// « relais/<id>/ » dans le bucket, puis sont effacés dès que l'autre les a reçus.
// Tout ce qui traîne depuis plus de 48 h est effacé au passage.

const RELAY: &str = "relais/";
const MARK: &str = ".colis.json";

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Package {
    #[serde(default)]
    pub id: String,
    pub from: String,
    pub files: usize,
    pub size: u64,
    pub date: String,
}

fn cleanup_old(c: &Config, all: &BTreeMap<String, Remote>) {
    let cutoff = (chrono::Utc::now() - chrono::Duration::hours(48)).format("%Y-%m-%dT%H:%M:%S").to_string();
    for (rel, r) in all {
        if !r.modified.is_empty() && r.modified < cutoff {
            let _ = delete(c, &format!("{RELAY}{rel}"));
        }
    }
}

/// Colis en attente (et ménage des vieux colis).
pub fn relay_pending(c: &Config) -> Result<Vec<Package>, String> {
    let all = list(c, RELAY)?;
    cleanup_old(c, &all);
    let mut out = vec![];
    for rel in all.keys().filter(|r| r.ends_with(&format!("/{MARK}"))) {
        let id = rel.trim_end_matches(&format!("/{MARK}")).to_string();
        let mut pk: Package = get(c, &format!("{RELAY}{rel}")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        pk.id = id;
        out.push(pk);
    }
    out.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(out)
}

/// Envoie des fichiers du coffre dans le relais. Le colis n'apparaît qu'une fois complet.
pub fn relay_send(c: &Config, root: &str, files: &[String], prog: &Arc<Mutex<Progress>>, stop: &AtomicBool, ctx: &eframe::egui::Context) -> Result<(), String> {
    let id = format!("{}-{:04x}", chrono::Utc::now().format("%Y%m%d%H%M%S"), std::process::id() as u16 ^ (chrono::Utc::now().timestamp_subsec_nanos() as u16));
    let mut size = 0;
    if let Ok(mut p) = prog.lock() {
        p.total = files.len();
    }
    for (i, rel) in files.iter().enumerate() {
        if stop.load(Ordering::Relaxed) {
            // Envoi annulé : on retire ce qui est déjà parti
            for r in &files[..i] {
                let _ = delete(c, &format!("{RELAY}{id}/{r}"));
            }
            return Err("envoi annulé".into());
        }
        if let Ok(mut p) = prog.lock() {
            p.done = i;
            p.step = rel.clone();
        }
        ctx.request_repaint();
        let path = crate::vault::safe(root, rel)?;
        let data = std::fs::read(&path).map_err(|e| format!("{rel} : {e}"))?;
        size += data.len() as u64;
        put(c, &format!("{RELAY}{id}/{rel}"), &data)?;
        if let Ok(mut p) = prog.lock() {
            p.up += 1;
        }
    }
    let pk = Package { id: id.clone(), from: crate::transfer::hostname(), files: files.len(), size, date: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string() };
    put(c, &format!("{RELAY}{id}/{MARK}"), serde_json::to_string(&pk).unwrap_or_default().as_bytes())?;
    if let Ok(mut p) = prog.lock() {
        p.done = files.len();
    }
    Ok(())
}

/// Reçoit un colis dans « reçus/ » puis l'efface du bucket.
pub fn relay_receive(c: &Config, root: &str, id: &str, prog: &Arc<Mutex<Progress>>, ctx: &eframe::egui::Context) -> Result<(), String> {
    let prefix = format!("{RELAY}{id}/");
    let all = list(c, &prefix)?;
    let files: Vec<&String> = all.keys().filter(|k| *k != MARK).collect();
    if let Ok(mut p) = prog.lock() {
        p.total = files.len();
    }
    let base = Path::new(root).join("reçus");
    for (i, rel) in files.iter().enumerate() {
        if let Ok(mut p) = prog.lock() {
            p.done = i;
            p.step = rel.to_string();
        }
        ctx.request_repaint();
        let data = get(c, &format!("{prefix}{rel}"))?;
        // Ne jamais écraser : « nom (2).ext »
        let mut dest = base.join(rel.as_str());
        let mut n = 2;
        while dest.exists() {
            let p = Path::new(rel.as_str());
            let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let name = match p.extension() {
                Some(e) => format!("{stem} ({n}).{}", e.to_string_lossy()),
                None => format!("{stem} ({n})"),
            };
            dest = base.join(p.parent().unwrap_or(Path::new(""))).join(name);
            n += 1;
        }
        if let Some(d) = dest.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&dest, &data).map_err(|e| e.to_string())?;
        if let Ok(mut p) = prog.lock() {
            p.down += 1;
        }
    }
    // Tout est arrivé : on efface le colis
    for rel in files {
        delete(c, &format!("{prefix}{rel}"))?;
    }
    delete(c, &format!("{prefix}{MARK}"))?;
    if let Ok(mut p) = prog.lock() {
        p.done = p.total;
    }
    Ok(())
}

/// Lance une opération de relais en arrière-plan avec la même barre de progression.
impl Sync {
    pub fn run_job(&self, ctx: eframe::egui::Context, job: impl FnOnce(&Arc<Mutex<Progress>>, &AtomicBool, &eframe::egui::Context) -> Result<(), String> + Send + 'static) {
        if self.busy() {
            return;
        }
        self.stop.store(false, Ordering::Relaxed);
        if let Ok(mut p) = self.progress.lock() {
            *p = Progress { running: true, step: "…".into(), ..Default::default() };
        }
        let (prog, stop) = (self.progress.clone(), self.stop.clone());
        std::thread::spawn(move || {
            let r = job(&prog, &stop, &ctx);
            if let Ok(mut p) = prog.lock() {
                p.running = false;
                p.step.clear();
                p.error = r.err();
                p.finished_at = Some(std::time::Instant::now());
            }
            ctx.request_repaint();
        });
    }
}

/// Oublie l'état de synchro (la prochaine fois, tout est comparé comme au premier passage).
pub fn reset(root: &str) {
    let _ = std::fs::remove_file(state_file(Path::new(root)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_aws() {
        // Exemple officiel AWS (GET Object, « Signature Version 4 examples »)
        let k = hmac(b"AWS4wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY", "20130524");
        let k = hmac(&k, "us-east-1");
        let k = hmac(&k, "s3");
        let k = hmac(&k, "aws4_request");
        let canonical = "GET\n/test.txt\n\nhost:examplebucket.s3.amazonaws.com\nrange:bytes=0-9\nx-amz-content-sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\nx-amz-date:20130524T000000Z\n\nhost;range;x-amz-content-sha256;x-amz-date\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let to_sign = format!("AWS4-HMAC-SHA256\n20130524T000000Z\n20130524/us-east-1/s3/aws4_request\n{}", sha(canonical.as_bytes()));
        assert_eq!(hex(&hmac(&k, &to_sign)), "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41");
        assert_eq!(enc("a b/é", true), "a%20b/%C3%A9");
    }

    #[test]
    fn plan_synchro() {
        let kn = |e: &str| Known { etag: e.into(), size: 3, mtime: 10 };
        let mut local = BTreeMap::new();
        let mut remote = BTreeMap::new();
        let mut known = BTreeMap::new();
        // inchangé
        local.insert("a".to_string(), (3, 10));
        remote.insert("a".to_string(), Remote { etag: "1".into(), size: 3, modified: String::new() });
        known.insert("a".to_string(), kn("1"));
        // modifié ici
        local.insert("b".to_string(), (4, 11));
        remote.insert("b".to_string(), Remote { etag: "1".into(), size: 3, modified: String::new() });
        known.insert("b".to_string(), kn("1"));
        // modifié en ligne
        local.insert("c".to_string(), (3, 10));
        remote.insert("c".to_string(), Remote { etag: "2".into(), size: 5, modified: String::new() });
        known.insert("c".to_string(), kn("1"));
        // supprimé ici
        remote.insert("d".to_string(), Remote { etag: "1".into(), size: 3, modified: String::new() });
        known.insert("d".to_string(), kn("1"));
        // supprimé en ligne
        local.insert("e".to_string(), (3, 10));
        known.insert("e".to_string(), kn("1"));
        // nouveau des deux côtés
        local.insert("f".to_string(), (1, 1));
        remote.insert("g".to_string(), Remote { etag: "9".into(), size: 1, modified: String::new() });
        // modifié des deux côtés
        local.insert("h".to_string(), (9, 99));
        remote.insert("h".to_string(), Remote { etag: "2".into(), size: 3, modified: String::new() });
        known.insert("h".to_string(), kn("1"));
        let p: Vec<(String, &str)> = plan(&local, &remote, &known)
            .into_iter()
            .map(|(r, a)| (r, match a { Act::Up => "up", Act::Down => "down", Act::DelRemote => "delr", Act::DelLocal => "dell", Act::Conflict => "conf" }))
            .collect();
        let want: Vec<(String, &str)> = [("b", "up"), ("c", "down"), ("d", "delr"), ("e", "dell"), ("f", "up"), ("g", "down"), ("h", "conf")].iter().map(|(a, b)| (a.to_string(), *b)).collect();
        assert_eq!(p, want);
    }
}

#[cfg(test)]
mod integration {
    use super::*;

    /// Lancé seulement avec ARYADNE_S3_TEST=http://localhost:5055 (serveur moto ou MinIO).
    #[test]
    fn aller_retour() {
        let Ok(endpoint) = std::env::var("ARYADNE_S3_TEST") else { return };
        let c = Config { endpoint, bucket: "testbucket".into(), access_key: "test".into(), secret_key: "test".into(), region: "us-east-1".into(), every_min: 0, full_sync: true };
        let tmp = std::env::temp_dir().join(format!("aryadne-sync-{}", std::process::id()));
        let (a, b) = (tmp.join("a"), tmp.join("b"));
        std::fs::create_dir_all(a.join("notes/sous dossier")).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("notes/sous dossier/été & co.md"), "bonjour").unwrap();
        std::fs::write(a.join("x.txt"), "x").unwrap();
        let ctx = eframe::egui::Context::default();
        let prog = Arc::new(Mutex::new(Progress::default()));
        let stop = AtomicBool::new(false);
        let pre_s = format!("coffres/t{}/", std::process::id());
        let pre = pre_s.as_str();
        run(&c, &a, pre, &prog, &stop, &ctx).unwrap();
        run(&c, &b, pre, &prog, &stop, &ctx).unwrap();
        assert_eq!(std::fs::read_to_string(b.join("notes/sous dossier/été & co.md")).unwrap(), "bonjour");
        // modif côté b, suppression côté b
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(b.join("notes/sous dossier/été & co.md"), "salut !").unwrap();
        std::fs::remove_file(b.join("x.txt")).unwrap();
        run(&c, &b, pre, &prog, &stop, &ctx).unwrap();
        run(&c, &a, pre, &prog, &stop, &ctx).unwrap();
        assert_eq!(std::fs::read_to_string(a.join("notes/sous dossier/été & co.md")).unwrap(), "salut !");
        assert!(!a.join("x.txt").exists());
        assert!(list(&c, pre).unwrap().len() == 1);
        // Relais : envoi depuis a, réception dans b, puis plus rien dans le bucket
        let ra = a.to_string_lossy().to_string();
        relay_send(&c, &ra, &["notes/sous dossier/été & co.md".to_string()], &prog, &stop, &ctx).unwrap();
        let pend = relay_pending(&c).unwrap();
        assert_eq!(pend.len(), 1);
        assert_eq!(pend[0].files, 1);
        relay_receive(&c, &b.to_string_lossy(), &pend[0].id, &prog, &ctx).unwrap();
        assert_eq!(std::fs::read_to_string(b.join("reçus/notes/sous dossier/été & co.md")).unwrap(), "salut !");
        assert!(list(&c, RELAY).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(tmp);
    }
}
