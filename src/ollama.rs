//! IA locale sans droits admin : Aryadne trouve, installe, lance et met à jour Ollama
//! dans le dossier de l'utilisateur, puis télécharge les modèles.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const WIN_URL: &str = "https://github.com/ollama/ollama/releases/latest/download/ollama-windows-amd64.zip";
const LINUX_URL: &str = "https://github.com/ollama/ollama/releases/latest/download/ollama-linux-amd64.tar.zst";

/// Ce qu'on sait de l'Ollama local (mis à jour en arrière-plan).
#[derive(Clone, Default)]
pub struct State {
    pub checked: bool,
    pub binary: Option<PathBuf>,
    pub running: bool,
    pub models: Vec<String>,
    /// Tâche en cours : (message, progression 0..1)
    pub task: Option<(String, Option<f32>)>,
    pub error: Option<String>,
}

#[derive(Clone, Default)]
pub struct Local {
    pub state: Arc<Mutex<State>>,
    child: Arc<Mutex<Option<Child>>>,
}

/// Dossier d'installation propre à Aryadne (aucun admin requis).
pub fn install_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("aryadne").join("ollama")
}

/// « http://localhost:11434/v1 » → « http://localhost:11434 »
pub fn host(api_url: &str) -> String {
    api_url.trim_end_matches('/').trim_end_matches("/v1").to_string()
}

pub fn is_local(api_url: &str) -> bool {
    let h = host(api_url);
    h.contains("localhost:11434") || h.contains("127.0.0.1:11434")
}

/// Client HTTP qui fait confiance aux certificats du système (proxys d'entreprise ou d'école compris).
pub fn agent(timeout: u64) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(timeout))).tls_config(tls).build().new_agent()
}

fn find_binary() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "ollama.exe" } else { "ollama" };
    let dir = install_dir();
    let mut candidates = vec![dir.join(exe), dir.join("bin").join(exe)];
    if cfg!(windows) {
        if let Some(local) = dirs::data_local_dir() {
            candidates.push(local.join("Programs").join("Ollama").join(exe));
        }
    } else {
        if let Some(home) = dirs::home_dir() {
            candidates.push(home.join(".local/bin/ollama"));
        }
        candidates.push("/usr/local/bin/ollama".into());
        candidates.push("/usr/bin/ollama".into());
    }
    if let Some(p) = candidates.into_iter().find(|p| p.is_file()) {
        return Some(p);
    }
    // Dans le PATH ?
    Command::new(exe).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().ok().filter(|s| s.success()).map(|_| PathBuf::from(exe))
}

fn ping(host: &str) -> bool {
    agent(2).get(&format!("{host}/api/version")).call().is_ok()
}

fn list_models(host: &str) -> Vec<String> {
    let Ok(mut r) = agent(4).get(&format!("{host}/api/tags")).call() else { return vec![] };
    let v: Value = r.body_mut().read_json().unwrap_or_default();
    v["models"].as_array().map(|a| a.iter().filter_map(|m| m["name"].as_str().map(str::to_string)).collect()).unwrap_or_default()
}

impl Local {
    fn set(&self, f: impl FnOnce(&mut State)) {
        if let Ok(mut s) = self.state.lock() {
            f(&mut s);
        }
    }

    pub fn snapshot(&self) -> State {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Revérifie l'état (binaire, serveur, modèles) en arrière-plan.
    pub fn refresh(&self, api_url: &str, ctx: &eframe::egui::Context) {
        let (me, host, ctx) = (self.clone(), host(api_url), ctx.clone());
        std::thread::spawn(move || {
            let binary = find_binary();
            let running = ping(&host);
            let models = if running { list_models(&host) } else { vec![] };
            me.set(|s| {
                s.checked = true;
                s.binary = binary;
                s.running = running;
                s.models = models;
            });
            ctx.request_repaint();
        });
    }

    /// Lance « ollama serve » en arrière-plan, sans fenêtre de console.
    pub fn start(&self) -> Result<(), String> {
        let bin = find_binary().ok_or("Ollama n'est pas installé.")?;
        let mut cmd = Command::new(&bin);
        cmd.arg("serve").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let child = cmd.spawn().map_err(|e| format!("Impossible de lancer Ollama : {e}"))?;
        if let Ok(mut c) = self.child.lock() {
            *c = Some(child);
        }
        Ok(())
    }

    /// Arrête l'Ollama lancé par Aryadne (pas celui lancé par ailleurs).
    pub fn stop(&self) {
        if let Ok(mut c) = self.child.lock() {
            if let Some(mut child) = c.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    /// Démarre Ollama s'il est installé mais pas lancé, puis attend qu'il réponde.
    pub fn ensure_running(&self, api_url: &str) -> Result<(), String> {
        let host = host(api_url);
        if ping(&host) {
            return Ok(());
        }
        self.start()?;
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(500));
            if ping(&host) {
                return Ok(());
            }
        }
        Err("Ollama a été lancé mais ne répond pas.".into())
    }

    /// Télécharge et installe Ollama dans le dossier de l'utilisateur.
    pub fn install(&self, api_url: &str, ctx: &eframe::egui::Context) {
        let (me, ctx, url) = (self.clone(), ctx.clone(), api_url.to_string());
        self.set(|s| {
            s.error = None;
            s.task = Some(("préparation…".into(), None));
        });
        std::thread::spawn(move || {
            let r = me.do_install(&ctx).and_then(|_| me.ensure_running(&url));
            me.set(|s| {
                s.task = None;
                s.error = r.err();
            });
            me.refresh(&url, &ctx);
        });
    }

    fn do_install(&self, ctx: &eframe::egui::Context) -> Result<(), String> {
        let url = if cfg!(windows) { WIN_URL } else { LINUX_URL };
        let dir = install_dir();
        let tmp = dir.with_extension("download");
        self.stop();
        let mut resp = agent(3600).get(url).call().map_err(|e| format!("Téléchargement impossible : {e}"))?;
        let total = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
        std::fs::create_dir_all(dir.parent().unwrap()).map_err(|e| e.to_string())?;
        let mut out = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        let mut reader = resp.body_mut().with_config().limit(u64::MAX).reader();
        let mut buf = vec![0u8; 1 << 20];
        let mut done = 0u64;
        loop {
            let n = reader.read(&mut buf).map_err(|e| format!("Téléchargement interrompu : {e}"))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            done += n as u64;
            let mb = done / 1_000_000;
            self.set(|s| s.task = Some((format!("téléchargement d'Ollama… {mb} Mo"), total.map(|t| done as f32 / t as f32))));
            ctx.request_repaint();
        }
        drop(out);

        self.set(|s| s.task = Some(("extraction…".into(), None)));
        ctx.request_repaint();
        let _ = std::fs::remove_dir_all(&dir); // les docs Ollama recommandent de repartir d'un dossier propre
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = std::fs::File::open(&tmp).map_err(|e| e.to_string())?;
        let r = if cfg!(windows) { extract_zip(file, &dir) } else { extract_tar_zst(file, &dir) };
        let _ = std::fs::remove_file(&tmp);
        r?;
        if find_binary().is_none() {
            return Err("Archive extraite, mais l'exécutable ollama est introuvable.".into());
        }
        Ok(())
    }

    /// Télécharge un modèle (ex. « llama3.2 ») avec progression.
    pub fn pull(&self, api_url: &str, model: &str, ctx: &eframe::egui::Context) {
        let (me, ctx, url, model) = (self.clone(), ctx.clone(), api_url.to_string(), model.trim().to_string());
        self.set(|s| {
            s.error = None;
            s.task = Some((format!("téléchargement de {model}…"), None));
        });
        std::thread::spawn(move || {
            let r = me.ensure_running(&url).and_then(|_| me.do_pull(&host(&url), &model, &ctx));
            me.set(|s| {
                s.task = None;
                s.error = r.err();
            });
            me.refresh(&url, &ctx);
        });
    }

    fn do_pull(&self, host: &str, model: &str, ctx: &eframe::egui::Context) -> Result<(), String> {
        let mut resp = agent(7200).post(&format!("{host}/api/pull")).send_json(json!({ "model": model, "stream": true })).map_err(|e| e.to_string())?;
        let reader = BufReader::new(resp.body_mut().with_config().limit(u64::MAX).reader());
        for line in reader.lines() {
            let line = line.map_err(|e| e.to_string())?;
            let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
            if let Some(err) = v["error"].as_str() {
                return Err(format!("Modèle « {model} » : {err}"));
            }
            let status = v["status"].as_str().unwrap_or("").to_string();
            let progress = match (v["completed"].as_f64(), v["total"].as_f64()) {
                (Some(c), Some(t)) if t > 0.0 => Some((c / t) as f32),
                _ => None,
            };
            let msg = match progress {
                Some(_) => format!("{model} : {} Mo / {} Mo", v["completed"].as_f64().unwrap_or(0.0) as u64 / 1_000_000, v["total"].as_f64().unwrap_or(0.0) as u64 / 1_000_000),
                None => format!("{model} : {status}"),
            };
            self.set(|s| s.task = Some((msg, progress)));
            ctx.request_repaint();
        }
        Ok(())
    }
}

fn extract_zip(file: std::fs::File, dir: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(BufReader::new(file)).map_err(|e| format!("Archive invalide : {e}"))?;
    zip.extract(dir).map_err(|e| format!("Extraction impossible : {e}"))
}

/// Lit une archive .tar.zst (éventuellement composée de plusieurs trames zstd).
fn extract_tar_zst(file: std::fs::File, dir: &Path) -> Result<(), String> {
    struct Frames<R: BufRead> {
        dec: Option<ruzstd::decoding::StreamingDecoder<R, ruzstd::decoding::FrameDecoder>>,
    }
    impl<R: BufRead> Read for Frames<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            loop {
                let Some(dec) = self.dec.as_mut() else { return Ok(0) };
                let n = dec.read(buf)?;
                if n > 0 {
                    return Ok(n);
                }
                // Fin de trame : y en a-t-il une autre ?
                let mut src = self.dec.take().unwrap().into_inner();
                if src.fill_buf()?.is_empty() {
                    return Ok(0);
                }
                self.dec = Some(new_decoder(src)?);
            }
        }
    }
    fn new_decoder<R: BufRead>(src: R) -> std::io::Result<ruzstd::decoding::StreamingDecoder<R, ruzstd::decoding::FrameDecoder>> {
        ruzstd::decoding::StreamingDecoder::new_with_max_window_size(src, 1 << 31).map_err(|e| std::io::Error::other(e.to_string()))
    }
    let src = BufReader::with_capacity(1 << 20, file);
    let frames = Frames { dec: Some(new_decoder(src).map_err(|e| format!("Archive invalide : {e}"))?) };
    tar::Archive::new(frames).unpack(dir).map_err(|e| format!("Extraction impossible : {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adresse_locale() {
        assert_eq!(host("http://localhost:11434/v1/"), "http://localhost:11434");
        assert!(is_local("http://127.0.0.1:11434/v1"));
        assert!(!is_local("https://api.openai.com/v1"));
    }
}
