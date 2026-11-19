//! Aperçu en direct : un petit serveur web local (127.0.0.1) qui sert les fichiers du
//! coffre. Les pages HTML reçoivent un script qui recharge le navigateur dès qu'un fichier
//! est enregistré dans Aryadne.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::RwLock;

static VERSION: AtomicU64 = AtomicU64::new(1);
static PORT: AtomicU16 = AtomicU16::new(0);
static ROOT: RwLock<String> = RwLock::new(String::new());

const RELOAD: &str = r#"<script>(function(){var v=null;function t(){fetch('/__aryadne/version',{cache:'no-store'}).then(function(r){return r.text()}).then(function(n){if(v!==null&&n!==v){location.reload()}v=n}).catch(function(){}).finally(function(){setTimeout(t,400)})}t()})();</script>"#;

/// À appeler à chaque enregistrement : les navigateurs ouverts se rechargent.
pub fn bump() {
    VERSION.fetch_add(1, Ordering::Relaxed);
}

pub fn is_previewable(rel: &str) -> bool {
    let l = rel.to_lowercase();
    l.ends_with(".html") || l.ends_with(".htm") || l.ends_with(".svg") || l.ends_with(".xhtml")
}

fn start() -> Result<u16, String> {
    let p = PORT.load(Ordering::Relaxed);
    if p != 0 {
        return Ok(p);
    }
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("Serveur d'aperçu : {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    PORT.store(port, Ordering::Relaxed);
    std::thread::spawn(move || {
        for s in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let _ = serve(s);
            });
        }
    });
    Ok(port)
}

/// Ouvre `rel` dans le navigateur par défaut, servi en direct.
pub fn open(root: &str, rel: &str) -> Result<String, String> {
    if let Ok(mut r) = ROOT.write() {
        *r = root.to_string();
    }
    let port = start()?;
    let path: String = rel.split('/').map(encode).collect::<Vec<_>>().join("/");
    let url = format!("http://127.0.0.1:{port}/{path}");
    open::that_detached(&url).map_err(|e| format!("Impossible d'ouvrir le navigateur : {e}"))?;
    Ok(url)
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~@".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn mime(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "html" | "htm" | "xhtml" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        "wasm" => "application/wasm",
        "md" | "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn respond(s: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) -> std::io::Result<()> {
    write!(s, "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len())?;
    s.write_all(body)
}

fn serve(mut s: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(s.try_clone()?);
    let mut first = String::new();
    reader.read_line(&mut first)?;
    // On lit et ignore les en-têtes
    loop {
        let mut l = String::new();
        if reader.read_line(&mut l)? == 0 || l == "\r\n" || l == "\n" {
            break;
        }
    }
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    if method != "GET" && method != "HEAD" {
        return respond(&mut s, "405 Method Not Allowed", "text/plain", b"");
    }
    let path = target.split(['?', '#']).next().unwrap_or("/");
    if path == "/__aryadne/version" {
        return respond(&mut s, "200 OK", "text/plain", VERSION.load(Ordering::Relaxed).to_string().as_bytes());
    }
    let rel = decode(path.trim_start_matches('/'));
    let root = ROOT.read().map(|r| r.clone()).unwrap_or_default();
    let file = match crate::vault::safe(&root, &rel) {
        Ok(f) => f,
        Err(_) => return respond(&mut s, "403 Forbidden", "text/plain", b"interdit"),
    };
    let file = if file.is_dir() { file.join("index.html") } else { file };
    match std::fs::read(&file) {
        Ok(mut body) => {
            let name = file.to_string_lossy().to_string();
            let ct = mime(&name);
            if ct.starts_with("text/html") {
                let mut html = String::from_utf8_lossy(&body).into_owned();
                match html.to_lowercase().rfind("</body>") {
                    Some(i) => html.insert_str(i, RELOAD),
                    None => html.push_str(RELOAD),
                }
                body = html.into_bytes();
            }
            if method == "HEAD" {
                body.clear();
            }
            respond(&mut s, "200 OK", ct, &body)
        }
        Err(_) => respond(&mut s, "404 Not Found", "text/plain; charset=utf-8", "introuvable".as_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn serves_and_injects() {
        let dir = std::env::temp_dir().join(format!("aryadne-live-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("site")).unwrap();
        std::fs::write(dir.join("site/a b.html"), "<html><body><h1>Salut</h1></body></html>").unwrap();
        *ROOT.write().unwrap() = dir.to_string_lossy().to_string();
        let port = start().unwrap();
        let get = |p: &str| {
            let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(c, "GET {p} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut out = String::new();
            c.read_to_string(&mut out).unwrap();
            out
        };
        let r = get("/site/a%20b.html");
        assert!(r.starts_with("HTTP/1.1 200"), "{r}");
        assert!(r.contains("__aryadne/version") && r.contains("<h1>Salut</h1>"));
        let v1 = get("/__aryadne/version");
        bump();
        let v2 = get("/__aryadne/version");
        assert_ne!(v1.lines().last(), v2.lines().last());
        assert!(get("/../etc/passwd").starts_with("HTTP/1.1 40"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
