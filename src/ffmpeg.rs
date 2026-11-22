//! ffmpeg sans droits admin : utilisé s'il est déjà installé, sinon téléchargé
//! (builds statiques BtbN sur GitHub) dans le dossier de l'utilisateur.
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

const WIN_URL: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";
const LINUX_URL: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz";

pub fn install_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("aryadne").join("ffmpeg")
}

fn exe(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

/// Chemin de ffmpeg ou ffprobe, s'il est disponible.
pub fn find(name: &str) -> Option<PathBuf> {
    let local = install_dir().join(exe(name));
    if local.is_file() {
        return Some(local);
    }
    let ok = command(PathBuf::from(exe(name))).arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
    ok.then(|| PathBuf::from(exe(name)))
}

/// Commande sans fenêtre de console sous Windows.
pub fn command(path: PathBuf) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c
}

#[derive(Clone, Default)]
pub struct InstallState {
    pub message: String,
    pub progress: Option<f32>,
    pub done: bool,
    pub error: Option<String>,
}

pub fn install(ctx: eframe::egui::Context) -> Arc<Mutex<InstallState>> {
    let st = Arc::new(Mutex::new(InstallState { message: "téléchargement de ffmpeg…".into(), ..Default::default() }));
    let s2 = st.clone();
    std::thread::spawn(move || {
        let set = |f: &dyn Fn(&mut InstallState)| {
            if let Ok(mut s) = s2.lock() {
                f(&mut s);
            }
            ctx.request_repaint();
        };
        let r = (|| -> Result<(), String> {
            let url = if cfg!(windows) { WIN_URL } else { LINUX_URL };
            let dir = install_dir();
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let tmp = dir.with_extension("download");
            let mut resp = crate::ollama::agent(3600).get(url).call().map_err(|e| format!("Téléchargement impossible : {e}"))?;
            let total = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
            let mut out = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
            let mut reader = resp.body_mut().with_config().limit(u64::MAX).reader();
            let mut buf = vec![0u8; 1 << 20];
            let mut got = 0u64;
            loop {
                let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                got += n as u64;
                let (mb, p) = (got / 1_000_000, total.map(|t| got as f32 / t as f32));
                set(&move |s: &mut InstallState| {
                    s.message = format!("téléchargement de ffmpeg… {mb} Mo");
                    s.progress = p;
                });
            }
            drop(out);
            set(&|s: &mut InstallState| {
                s.message = "extraction…".into();
                s.progress = None;
            });
            let wanted = [exe("ffmpeg"), exe("ffprobe"), exe("ffplay")];
            if cfg!(windows) {
                let mut z = zip::ZipArchive::new(std::fs::File::open(&tmp).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                for i in 0..z.len() {
                    let mut f = z.by_index(i).map_err(|e| e.to_string())?;
                    let full = f.name().map(|n| n.to_string()).unwrap_or_default();
                    let name = full.rsplit('/').next().unwrap_or("").to_string();
                    if full.contains("/bin/") && wanted.contains(&name) {
                        let mut o = std::fs::File::create(dir.join(&name)).map_err(|e| e.to_string())?;
                        std::io::copy(&mut f, &mut o).map_err(|e| e.to_string())?;
                    }
                }
            } else {
                // .tar.xz : on décompresse vers un .tar temporaire, puis on n'en garde que les binaires.
                let tar_path = dir.with_extension("tar");
                {
                    let mut input = std::io::BufReader::new(std::fs::File::open(&tmp).map_err(|e| e.to_string())?);
                    let mut output = std::io::BufWriter::new(std::fs::File::create(&tar_path).map_err(|e| e.to_string())?);
                    lzma_rs::xz_decompress(&mut input, &mut output).map_err(|e| format!("Archive illisible : {e}"))?;
                }
                let mut ar = tar::Archive::new(std::fs::File::open(&tar_path).map_err(|e| e.to_string())?);
                for entry in ar.entries().map_err(|e| e.to_string())? {
                    let mut entry = entry.map_err(|e| e.to_string())?;
                    let path = entry.path().map_err(|e| e.to_string())?.to_string_lossy().to_string();
                    let name = path.rsplit('/').next().unwrap_or("").to_string();
                    if path.contains("/bin/") && wanted.contains(&name) {
                        entry.unpack(dir.join(&name)).map_err(|e| e.to_string())?;
                    }
                }
                let _ = std::fs::remove_file(&tar_path);
            }
            let _ = std::fs::remove_file(&tmp);
            if !dir.join(exe("ffmpeg")).is_file() {
                return Err("ffmpeg introuvable dans l'archive.".into());
            }
            Ok(())
        })();
        set(&move |s: &mut InstallState| {
            s.done = true;
            s.progress = None;
            if let Err(e) = &r {
                s.error = Some(e.clone());
            }
        });
    });
    st
}
