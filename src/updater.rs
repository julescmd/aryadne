//! Mises à jour depuis l'application : lit la dernière Release GitHub, télécharge
//! l'exécutable de la plateforme et remplace celui en cours (aucun admin requis tant
//! qu'Aryadne est dans un dossier de l'utilisateur).
use serde_json::Value;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

/// Repo qui publie les Releases. S'il est privé, un jeton GitHub en lecture est nécessaire.
pub const REPO: &str = "julescmd/aryadne";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

fn asset_name() -> &'static str {
    if cfg!(windows) { "aryadne-windows-x64.exe" } else { "aryadne-linux-x64" }
}

#[derive(Clone)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub asset_api_url: String,
    pub size: u64,
}

#[derive(Clone, Default)]
pub struct State {
    pub checking: bool,
    pub checked: bool,
    pub latest: Option<Release>,
    pub progress: Option<f32>,
    pub installed: bool,
    pub error: Option<String>,
}

#[derive(Clone, Default)]
pub struct Updater {
    pub state: Arc<Mutex<State>>,
}

/// « v0.5.1 » → (0, 5, 1)
pub fn parse(v: &str) -> (u32, u32, u32) {
    let mut it = v.trim().trim_start_matches('v').split(['.', '-']).map(|x| x.parse::<u32>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    parse(candidate) > parse(current)
}

fn get(url: &str, token: &str, accept: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
    let mut req = crate::ollama::agent(3600).get(url).header("User-Agent", "aryadne-updater").header("Accept", accept);
    if !token.trim().is_empty() {
        req = req.header("Authorization", format!("Bearer {}", token.trim()));
    }
    req.call().map_err(|e| match e {
        ureq::Error::StatusCode(404) => {
            "Aucune release trouvée. Si le repo est privé, ajoute un jeton GitHub (lecture seule) dans les réglages.".to_string()
        }
        ureq::Error::StatusCode(401) | ureq::Error::StatusCode(403) => "Jeton GitHub refusé ou limite d'API atteinte.".to_string(),
        other => format!("Vérification impossible : {other}"),
    })
}

impl Updater {
    fn set(&self, f: impl FnOnce(&mut State)) {
        if let Ok(mut s) = self.state.lock() {
            f(&mut s);
        }
    }

    pub fn snapshot(&self) -> State {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn update_available(&self) -> Option<Release> {
        let s = self.snapshot();
        s.latest.filter(|r| is_newer(&r.version, CURRENT) && !s.installed)
    }

    pub fn check(&self, token: String, ctx: &eframe::egui::Context) {
        if self.snapshot().checking {
            return;
        }
        self.set(|s| {
            s.checking = true;
            s.error = None;
        });
        let (me, ctx) = (self.clone(), ctx.clone());
        std::thread::spawn(move || {
            let r = (|| -> Result<Release, String> {
                let mut resp = get(&format!("https://api.github.com/repos/{REPO}/releases/latest"), &token, "application/vnd.github+json")?;
                let v: Value = resp.body_mut().read_json().map_err(|e| e.to_string())?;
                let version = v["tag_name"].as_str().unwrap_or("").to_string();
                let asset = v["assets"].as_array().and_then(|a| a.iter().find(|x| x["name"].as_str() == Some(asset_name())));
                let asset = asset.ok_or_else(|| format!("La release {version} ne contient pas {}.", asset_name()))?;
                Ok(Release {
                    version,
                    notes: v["body"].as_str().unwrap_or("").to_string(),
                    asset_api_url: asset["url"].as_str().unwrap_or("").to_string(),
                    size: asset["size"].as_u64().unwrap_or(0),
                })
            })();
            me.set(|s| {
                s.checking = false;
                s.checked = true;
                match r {
                    Ok(rel) => s.latest = Some(rel),
                    Err(e) => s.error = Some(e),
                }
            });
            ctx.request_repaint();
        });
    }

    /// Télécharge la nouvelle version et remplace l'exécutable actuel.
    pub fn install(&self, token: String, ctx: &eframe::egui::Context) {
        let Some(rel) = self.update_available() else { return };
        self.set(|s| {
            s.progress = Some(0.0);
            s.error = None;
        });
        let (me, ctx) = (self.clone(), ctx.clone());
        std::thread::spawn(move || {
            let r = (|| -> Result<(), String> {
                let mut resp = get(&rel.asset_api_url, &token, "application/octet-stream")?;
                let tmp = std::env::temp_dir().join(format!("aryadne-{}-{}", rel.version, std::process::id()));
                let mut out = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
                let mut reader = resp.body_mut().with_config().limit(u64::MAX).reader();
                let mut buf = vec![0u8; 1 << 16];
                let mut done = 0u64;
                loop {
                    let n = reader.read(&mut buf).map_err(|e| format!("Téléchargement interrompu : {e}"))?;
                    if n == 0 {
                        break;
                    }
                    out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                    done += n as u64;
                    let f = if rel.size > 0 { done as f32 / rel.size as f32 } else { 0.0 };
                    me.set(|s| s.progress = Some(f));
                    ctx.request_repaint();
                }
                drop(out);
                if rel.size > 0 && done != rel.size {
                    return Err("Téléchargement incomplet, réessaie.".into());
                }
                self_replace::self_replace(&tmp).map_err(|e| format!("Remplacement impossible ({e}). Aryadne est-il dans un dossier protégé ?"))?;
                let _ = std::fs::remove_file(&tmp);
                Ok(())
            })();
            me.set(|s| {
                s.progress = None;
                match r {
                    Ok(()) => s.installed = true,
                    Err(e) => s.error = Some(e),
                }
            });
            ctx.request_repaint();
        });
    }
}

/// Relance Aryadne (nouvelle version) et ferme celle-ci.
pub fn restart(ctx: &eframe::egui::Context) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).spawn();
    }
    ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("v0.5.1", "0.5.0"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("v0.5.0", "0.5.0"));
        assert!(!is_newer("v0.4.9", "0.5.0"));
    }
}
