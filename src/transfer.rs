//! Export en zip et transfert de fichiers entre deux ordinateurs du même réseau.
//!
//! Transfert : l'ordinateur qui reçoit ouvre un port et annonce sa présence sur le réseau
//! local ; celui qui envoie le choisit dans la liste (ou tape son adresse) et donne le code
//! à 4 chiffres affiché. Les fichiers arrivent dans le dossier « reçus/ » du coffre.
//! Pensé pour les gros fichiers 3D : envoi en flux, sans limite de taille, sans cloud.
use crate::vault::{self, Entry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const PORT: u16 = 47123;
const DISCOVERY_PORT: u16 = 47124;

// ---------- sélection des fichiers ----------

/// Développe des chemins (fichiers ou dossiers) en liste de fichiers.
/// `with_links` : ajoute les notes liées [[ ]], les images citées et le contenu des boards.
pub fn collect(root: &str, entries: &[Entry], rels: &[String], with_links: bool) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    let mut queue: Vec<String> = rels.to_vec();
    let md = vault::md_index(entries);
    let mut seen = BTreeSet::new();
    while let Some(rel) = queue.pop() {
        if !seen.insert(rel.clone()) {
            continue;
        }
        if let Some(e) = entries.iter().find(|e| e.rel == rel) {
            if e.is_dir {
                for c in entries.iter().filter(|c| !c.is_dir && c.rel.starts_with(&format!("{rel}/"))) {
                    out.insert(c.rel.clone());
                }
                continue;
            }
        }
        out.insert(rel.clone());
        if !with_links {
            continue;
        }
        if vault::is_md(&rel) {
            let text = vault::read(root, &rel).unwrap_or_default();
            for t in vault::links(&text) {
                if let Some(r) = md.get(&t.to_lowercase()) {
                    // Un seul niveau de liens : sinon tout le coffre y passe.
                    out.insert(r.clone());
                }
            }
            for e in entries.iter().filter(|e| !e.is_dir && crate::images::is_image(&e.rel)) {
                if text.contains(&e.name) {
                    out.insert(e.rel.clone());
                }
            }
        } else if crate::board::is_board(&rel) {
            if let Ok(b) = crate::board::Board::load(root, &rel) {
                for it in b.data.items {
                    if let crate::board::Kind::Image { rel, .. } | crate::board::Kind::Note { rel } = it.kind {
                        out.insert(rel);
                    }
                }
            }
        }
    }
    out.into_iter().collect()
}

pub fn total_size(root: &str, files: &[String]) -> u64 {
    files.iter().filter_map(|f| vault::safe(root, f).ok()).filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum()
}

pub fn human(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e9 {
        format!("{:.2} Go", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} Mo", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} Ko", b / 1e3)
    } else {
        format!("{bytes} o")
    }
}

/// Nom dans l'archive : « @Dossier lié/x » devient « Dossier lié/x ».
fn archive_name(rel: &str) -> String {
    rel.trim_start_matches('@').to_string()
}

// ---------- progression partagée ----------

#[derive(Clone, Default)]
pub struct Progress {
    pub label: String,
    pub done: u64,
    pub total: u64,
    pub finished: bool,
    pub error: Option<String>,
    pub result: Option<String>,
    /// Fichiers reçus (chemins du coffre).
    pub received: Vec<String>,
}

pub type Shared = Arc<Mutex<Progress>>;

fn set(p: &Shared, f: impl FnOnce(&mut Progress)) {
    if let Ok(mut g) = p.lock() {
        f(&mut g);
    }
}

/// Copie en flux avec progression.
fn pump(mut from: impl Read, mut to: impl Write, len: u64, prog: &Shared, ctx: &eframe::egui::Context) -> std::io::Result<()> {
    let mut buf = vec![0u8; 1 << 20];
    let mut left = len;
    let mut last = Instant::now();
    while left > 0 {
        let cap = left.min(buf.len() as u64) as usize;
        let n = from.read(&mut buf[..cap])?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "connexion coupée"));
        }
        to.write_all(&buf[..n])?;
        left -= n as u64;
        set(prog, |p| p.done += n as u64);
        if last.elapsed() > Duration::from_millis(80) {
            ctx.request_repaint();
            last = Instant::now();
        }
    }
    Ok(())
}

// ---------- export zip ----------

pub fn export_zip(root: String, files: Vec<String>, dest: PathBuf, ctx: eframe::egui::Context) -> Shared {
    let prog: Shared = Arc::new(Mutex::new(Progress { label: "export zip".into(), total: total_size(&root, &files), ..Default::default() }));
    let p2 = prog.clone();
    std::thread::spawn(move || {
        let r = (|| -> Result<(), String> {
            let file = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(file));
            for rel in &files {
                let src = vault::safe(&root, rel)?;
                let len = std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
                // Images et archives sont déjà compressées : on les range telles quelles (plus rapide).
                let ext = rel.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
                let method = if ["png", "jpg", "jpeg", "webp", "zip", "7z", "mp4", "mov", "exr", "gz", "zst"].contains(&ext.as_str()) {
                    zip::CompressionMethod::Stored
                } else {
                    zip::CompressionMethod::Deflated
                };
                let opts = zip::write::SimpleFileOptions::default().compression_method(method).large_file(len >= u32::MAX as u64);
                zip.start_file(archive_name(rel), opts).map_err(|e| e.to_string())?;
                set(&p2, |p| p.label = format!("export : {rel}"));
                let f = std::fs::File::open(&src).map_err(|e| e.to_string())?;
                pump(f, &mut zip, len, &p2, &ctx).map_err(|e| e.to_string())?;
            }
            zip.finish().map_err(|e| e.to_string())?;
            Ok(())
        })();
        set(&p2, |p| {
            p.finished = true;
            match r {
                Ok(()) => p.result = Some(format!("{} fichiers exportés dans {}", files.len(), dest.display())),
                Err(e) => p.error = Some(e),
            }
        });
        ctx.request_repaint();
    });
    prog
}

// ---------- transfert réseau ----------

#[derive(Serialize, Deserialize)]
struct Header {
    code: String,
    from: String,
    files: Vec<FileHeader>,
}

#[derive(Serialize, Deserialize)]
struct FileHeader {
    path: String,
    size: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Peer {
    pub name: String,
    pub addr: String,
}

pub fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "cet ordinateur".into())
}

/// Adresse IP de cet ordinateur sur le réseau local (aucun paquet n'est envoyé).
pub fn local_ip() -> Option<String> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.168.1.1:80").or_else(|_| s.connect("10.0.0.1:80")).ok()?;
    s.local_addr().ok().map(|a| a.ip().to_string())
}

/// Côté réception : écoute, annonce, enregistre dans `reçus/`.
pub struct Receiver {
    pub code: String,
    pub port: u16,
    pub progress: Shared,
    stop: Arc<Mutex<bool>>,
}

impl Receiver {
    pub fn start(root: String, ctx: eframe::egui::Context) -> Result<Self, String> {
        let listener = TcpListener::bind(("0.0.0.0", PORT)).or_else(|_| TcpListener::bind(("0.0.0.0", 0))).map_err(|e| format!("Impossible d'ouvrir le port : {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(1234);
        let code = format!("{:04}", (nanos / 7) % 10000);
        let progress: Shared = Arc::new(Mutex::new(Progress { label: "en attente d'un envoi…".into(), ..Default::default() }));
        let stop = Arc::new(Mutex::new(false));

        // Annonce sur le réseau local toutes les secondes.
        let (stop_a, name) = (stop.clone(), hostname());
        std::thread::spawn(move || {
            let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else { return };
            let _ = sock.set_broadcast(true);
            let msg = serde_json::to_string(&serde_json::json!({ "app": "aryadne", "name": name, "port": port })).unwrap();
            while !*stop_a.lock().unwrap() {
                let _ = sock.send_to(msg.as_bytes(), ("255.255.255.255", DISCOVERY_PORT));
                std::thread::sleep(Duration::from_secs(1));
            }
        });

        let (stop_l, prog, code_l) = (stop.clone(), progress.clone(), code.clone());
        std::thread::spawn(move || {
            while !*stop_l.lock().unwrap() {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        if let Err(e) = receive(&root, stream, &code_l, &prog, &ctx) {
                            set(&prog, |p| p.error = Some(e));
                        }
                        ctx.request_repaint();
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(150)),
                }
            }
        });
        Ok(Self { code, port, progress, stop })
    }

    pub fn stop(&self) {
        if let Ok(mut s) = self.stop.lock() {
            *s = true;
        }
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        self.stop();
    }
}

fn receive(root: &str, stream: TcpStream, code: &str, prog: &Shared, ctx: &eframe::egui::Context) -> Result<(), String> {
    let mut reader = BufReader::with_capacity(1 << 20, stream.try_clone().map_err(|e| e.to_string())?);
    let mut writer = stream;
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let header: Header = serde_json::from_str(&line).map_err(|_| "Envoi invalide.".to_string())?;
    if header.code.trim() != code {
        let _ = writer.write_all(b"NO\n");
        return Err(format!("{} a tenté un envoi avec un mauvais code.", header.from));
    }
    let total: u64 = header.files.iter().map(|f| f.size).sum();
    writer.write_all(b"OK\n").map_err(|e| e.to_string())?;
    set(prog, |p| {
        p.done = 0;
        p.total = total;
        p.error = None;
        p.finished = false;
        p.received.clear();
        p.label = format!("réception depuis {}…", header.from);
    });
    for f in &header.files {
        // Les chemins reçus restent dans « reçus/ » : rien ne peut sortir du coffre.
        let clean: String = f.path.trim_start_matches('@').split('/').filter(|s| !s.is_empty() && *s != ".." && *s != ".").collect::<Vec<_>>().join("/");
        let rel = vault::join("reçus", &clean);
        let dest = vault::safe(root, &rel)?;
        if let Some(d) = dest.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        set(prog, |p| p.label = format!("réception : {clean}"));
        let out = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
        pump(&mut reader, std::io::BufWriter::new(out), f.size, prog, ctx).map_err(|e| e.to_string())?;
        set(prog, |p| p.received.push(rel.clone()));
    }
    let _ = writer.write_all(b"DONE\n");
    set(prog, |p| {
        p.finished = true;
        p.result = Some(format!("{} fichiers reçus de {} ({}) dans reçus/", header.files.len(), header.from, human(total)));
        p.label = "en attente d'un envoi…".into();
    });
    Ok(())
}

/// Écoute les annonces pendant ~2 s et retourne les ordinateurs trouvés.
pub fn discover(found: Arc<Mutex<Vec<Peer>>>, ctx: eframe::egui::Context) {
    std::thread::spawn(move || {
        let Ok(sock) = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)) else { return };
        let _ = sock.set_read_timeout(Some(Duration::from_millis(300)));
        let until = Instant::now() + Duration::from_millis(2500);
        let mut buf = [0u8; 512];
        while Instant::now() < until {
            if let Ok((n, from)) = sock.recv_from(&mut buf) {
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&buf[..n]) {
                    if v["app"] == "aryadne" {
                        let peer = Peer { name: v["name"].as_str().unwrap_or("?").to_string(), addr: format!("{}:{}", from.ip(), v["port"].as_u64().unwrap_or(PORT as u64)) };
                        if let Ok(mut f) = found.lock() {
                            if !f.contains(&peer) {
                                f.push(peer);
                            }
                        }
                        ctx.request_repaint();
                    }
                }
            }
        }
    });
}

/// Envoie des fichiers du coffre vers `addr` (« ip:port »).
pub fn send(root: String, files: Vec<String>, addr: String, code: String, ctx: eframe::egui::Context) -> Shared {
    let total = total_size(&root, &files);
    let prog: Shared = Arc::new(Mutex::new(Progress { label: format!("connexion à {addr}…"), total, ..Default::default() }));
    let p2 = prog.clone();
    std::thread::spawn(move || {
        let r = (|| -> Result<(), String> {
            let addr = if addr.contains(':') { addr.clone() } else { format!("{addr}:{PORT}") };
            let sock = addr.parse().map_err(|_| format!("Adresse invalide : {addr}"))?;
            let stream = TcpStream::connect_timeout(&sock, Duration::from_secs(6)).map_err(|e| format!("Connexion impossible ({e}). L'autre ordinateur est-il en mode « recevoir » ?"))?;
            let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
            let mut w = std::io::BufWriter::with_capacity(1 << 20, stream);
            let header = Header {
                code: code.trim().to_string(),
                from: hostname(),
                files: files.iter().map(|f| FileHeader { path: archive_name(f), size: vault::safe(&root, f).ok().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()).unwrap_or(0) }).collect(),
            };
            w.write_all(serde_json::to_string(&header).unwrap().as_bytes()).and_then(|_| w.write_all(b"\n")).and_then(|_| w.flush()).map_err(|e| e.to_string())?;
            let mut answer = String::new();
            reader.read_line(&mut answer).map_err(|e| e.to_string())?;
            if answer.trim() != "OK" {
                return Err("Code refusé par l'autre ordinateur.".into());
            }
            for (f, h) in files.iter().zip(&header.files) {
                set(&p2, |p| p.label = format!("envoi : {f}"));
                let src = std::fs::File::open(vault::safe(&root, f)?).map_err(|e| e.to_string())?;
                pump(src, &mut w, h.size, &p2, &ctx).map_err(|e| e.to_string())?;
            }
            w.flush().map_err(|e| e.to_string())?;
            let mut done = String::new();
            let _ = reader.read_line(&mut done);
            Ok(())
        })();
        set(&p2, |p| {
            p.finished = true;
            match r {
                Ok(()) => p.result = Some(format!("{} fichiers envoyés ({})", files.len(), human(p.total))),
                Err(e) => p.error = Some(e),
            }
        });
        ctx.request_repaint();
    });
    prog
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_et_transfert_local() {
        let dir = std::env::temp_dir().join("aryadne-transfer-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("notes")).unwrap();
        std::fs::create_dir_all(dir.join("refs")).unwrap();
        std::fs::write(dir.join("notes/A.md"), "Voir [[B]] et foret.png").unwrap();
        std::fs::write(dir.join("notes/B.md"), "b").unwrap();
        std::fs::write(dir.join("notes/C.md"), "pas liée").unwrap();
        std::fs::write(dir.join("refs/foret.png"), vec![7u8; 300_000]).unwrap();
        let root = dir.to_string_lossy().to_string();
        let entries: Vec<Entry> = vault::scan(&root).into_iter().filter(|e| !e.rel.starts_with('@')).collect();

        let files = collect(&root, &entries, &["notes/A.md".into()], true);
        assert_eq!(files, vec!["notes/A.md", "notes/B.md", "refs/foret.png"]);
        assert_eq!(collect(&root, &entries, &["notes".into()], false).len(), 3);

        let ctx = eframe::egui::Context::default();
        let zip_path = dir.join("export.zip");
        let p = export_zip(root.clone(), files.clone(), zip_path.clone(), ctx.clone());
        while !p.lock().unwrap().finished {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(p.lock().unwrap().error.is_none());
        let mut z = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
        assert_eq!(z.len(), 3);
        assert_eq!(z.by_name("refs/foret.png").unwrap().size(), 300_000);

        // Transfert vers un deuxième coffre sur la même machine
        let dir2 = std::env::temp_dir().join("aryadne-transfer-test-2");
        let _ = std::fs::remove_dir_all(&dir2);
        std::fs::create_dir_all(&dir2).unwrap();
        let rx = Receiver::start(dir2.to_string_lossy().to_string(), ctx.clone()).unwrap();
        let bad = send(root.clone(), files.clone(), format!("127.0.0.1:{}", rx.port), "0000x".into(), ctx.clone());
        while !bad.lock().unwrap().finished {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(bad.lock().unwrap().error.is_some(), "un mauvais code doit être refusé");
        let ok = send(root, files, format!("127.0.0.1:{}", rx.port), rx.code.clone(), ctx);
        while !ok.lock().unwrap().finished {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(ok.lock().unwrap().error.is_none(), "{:?}", ok.lock().unwrap().error);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(std::fs::read(dir2.join("reçus/refs/foret.png")).unwrap().len(), 300_000);
        assert!(dir2.join("reçus/notes/B.md").exists());
        rx.stop();
    }
}
