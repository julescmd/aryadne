//! Débruitage par IA avec Intel Open Image Denoise (le débruiteur de Cycles, Arnold, V-Ray).
//! La bibliothèque est téléchargée une fois dans le dossier de l'utilisateur (sans droits admin)
//! et chargée à l'exécution : sans elle, le débruitage maison prend le relais.
use crate::ffmpeg::InstallState;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const VERSION: &str = "2.3.3";

fn url() -> String {
    if cfg!(windows) {
        format!("https://github.com/RenderKit/oidn/releases/download/v{VERSION}/oidn-{VERSION}.x64.windows.zip")
    } else if cfg!(target_os = "macos") {
        format!("https://github.com/RenderKit/oidn/releases/download/v{VERSION}/oidn-{VERSION}.arm64.macos.tar.gz")
    } else {
        format!("https://github.com/RenderKit/oidn/releases/download/v{VERSION}/oidn-{VERSION}.x86_64.linux.tar.gz")
    }
}

pub fn install_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("aryadne").join("oidn")
}

fn lib_name() -> &'static str {
    if cfg!(windows) {
        "OpenImageDenoise.dll"
    } else if cfg!(target_os = "macos") {
        "libOpenImageDenoise.2.dylib"
    } else {
        "libOpenImageDenoise.so.2"
    }
}

pub fn installed() -> bool {
    install_dir().join(lib_name()).exists()
}

/// Télécharge et décompresse la bibliothèque (seulement les fichiers utiles : bin/ ou lib/).
pub fn install(ctx: eframe::egui::Context) -> Arc<Mutex<InstallState>> {
    let st = Arc::new(Mutex::new(InstallState { message: "téléchargement du débruiteur…".into(), ..Default::default() }));
    let s2 = st.clone();
    std::thread::spawn(move || {
        let set = |f: &dyn Fn(&mut InstallState)| {
            if let Ok(mut s) = s2.lock() {
                f(&mut s);
            }
            ctx.request_repaint();
        };
        let r = (|| -> Result<(), String> {
            let dir = install_dir();
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let tmp = dir.with_extension("download");
            let mut resp = crate::ollama::agent(3600).get(&url()).call().map_err(|e| format!("Téléchargement impossible : {e}"))?;
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
                    s.message = format!("téléchargement du débruiteur… {mb} Mo");
                    s.progress = p;
                });
            }
            drop(out);
            set(&|s: &mut InstallState| {
                s.message = "extraction…".into();
                s.progress = None;
            });
            if cfg!(windows) {
                let mut z = zip::ZipArchive::new(std::fs::File::open(&tmp).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                for i in 0..z.len() {
                    let mut f = z.by_index(i).map_err(|e| e.to_string())?;
                    let full = f.name().map(|n| n.to_string()).unwrap_or_default();
                    let name = full.rsplit('/').next().unwrap_or("").to_string();
                    if full.contains("/bin/") && name.to_lowercase().ends_with(".dll") {
                        let mut o = std::fs::File::create(dir.join(&name)).map_err(|e| e.to_string())?;
                        std::io::copy(&mut f, &mut o).map_err(|e| e.to_string())?;
                    }
                }
            } else {
                let gz = flate2::read::GzDecoder::new(std::fs::File::open(&tmp).map_err(|e| e.to_string())?);
                let mut ar = tar::Archive::new(gz);
                for entry in ar.entries().map_err(|e| e.to_string())? {
                    let mut entry = entry.map_err(|e| e.to_string())?;
                    let path = entry.path().map_err(|e| e.to_string())?.to_string_lossy().to_string();
                    let name = path.rsplit('/').next().unwrap_or("").to_string();
                    if path.contains("/lib/") && (name.contains(".so") || name.ends_with(".dylib")) {
                        let _ = std::fs::remove_file(dir.join(&name));
                        entry.unpack(dir.join(&name)).map_err(|e| e.to_string())?;
                    }
                }
            }
            let _ = std::fs::remove_file(&tmp);
            if !installed() {
                return Err("Débruiteur introuvable dans l'archive.".into());
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

type Device = *mut std::ffi::c_void;
type Filter = *mut std::ffi::c_void;
type Buffer = *mut std::ffi::c_void;
const FORMAT_FLOAT3: i32 = 3;

/// La bibliothèque chargée et un appareil prêt (le meilleur disponible : GPU sinon CPU).
struct Lib {
    _lib: libloading::Library,
    /// Bibliothèques dont elle dépend, chargées avant elle (pas de chemin de recherche dans l'archive)
    _deps: Vec<libloading::Library>,
    device: Device,
    new_filter: unsafe extern "C" fn(Device, *const std::ffi::c_char) -> Filter,
    new_buffer: unsafe extern "C" fn(Device, usize) -> Buffer,
    write_buffer: unsafe extern "C" fn(Buffer, usize, usize, *const std::ffi::c_void),
    read_buffer: unsafe extern "C" fn(Buffer, usize, usize, *mut std::ffi::c_void),
    release_buffer: unsafe extern "C" fn(Buffer),
    set_image: unsafe extern "C" fn(Filter, *const std::ffi::c_char, Buffer, i32, usize, usize, usize, usize, usize),
    set_bool: unsafe extern "C" fn(Filter, *const std::ffi::c_char, bool),
    commit_filter: unsafe extern "C" fn(Filter),
    execute: unsafe extern "C" fn(Filter),
    release_filter: unsafe extern "C" fn(Filter),
    error: unsafe extern "C" fn(Device, *mut *const std::ffi::c_char) -> i32,
}

// L'appareil OIDN s'utilise depuis un seul fil à la fois : protégé par le Mutex ci-dessous.
unsafe impl Send for Lib {}

static LIB: Mutex<Option<Result<Lib, String>>> = Mutex::new(None);

fn load() -> Result<Lib, String> {
    let dir = install_dir();
    let path = dir.join(lib_name());
    unsafe {
        let mut deps = vec![];
        #[cfg(unix)]
        for d in ["libtbb.so.12", "libOpenImageDenoise_core.so.2.3.3", "libOpenImageDenoise_core.2.3.3.dylib", "libtbb.12.dylib"] {
            let p = dir.join(d);
            if p.exists() {
                let l = libloading::os::unix::Library::open(Some(&p), libloading::os::unix::RTLD_NOW | libloading::os::unix::RTLD_GLOBAL).map_err(|e| format!("débruiteur : {e}"))?;
                deps.push(libloading::Library::from(l));
            }
        }
        #[cfg(windows)]
        let lib: libloading::Library = libloading::os::windows::Library::load_with_flags(&path, libloading::os::windows::LOAD_WITH_ALTERED_SEARCH_PATH).map_err(|e| format!("débruiteur : {e}"))?.into();
        #[cfg(not(windows))]
        let lib = libloading::Library::new(&path).map_err(|e| format!("débruiteur : {e}"))?;
        macro_rules! sym {
            ($n:literal) => {
                *lib.get($n).map_err(|e| format!("débruiteur : {e}"))?
            };
        }
        let new_device: unsafe extern "C" fn(i32) -> Device = sym!(b"oidnNewDevice\0");
        let commit_device: unsafe extern "C" fn(Device) = sym!(b"oidnCommitDevice\0");
        let device = new_device(0);
        if device.is_null() {
            return Err("débruiteur : aucun appareil".into());
        }
        commit_device(device);
        let l = Lib {
            device,
            new_filter: sym!(b"oidnNewFilter\0"),
            new_buffer: sym!(b"oidnNewBuffer\0"),
            write_buffer: sym!(b"oidnWriteBuffer\0"),
            read_buffer: sym!(b"oidnReadBuffer\0"),
            release_buffer: sym!(b"oidnReleaseBuffer\0"),
            set_image: sym!(b"oidnSetFilterImage\0"),
            set_bool: sym!(b"oidnSetFilterBool\0"),
            commit_filter: sym!(b"oidnCommitFilter\0"),
            execute: sym!(b"oidnExecuteFilter\0"),
            release_filter: sym!(b"oidnReleaseFilter\0"),
            error: sym!(b"oidnGetDeviceError\0"),
            _lib: lib,
            _deps: deps,
        };
        let mut msg: *const std::ffi::c_char = std::ptr::null();
        if (l.error)(l.device, &mut msg) != 0 {
            return Err(format!("débruiteur : {}", if msg.is_null() { "erreur".into() } else { std::ffi::CStr::from_ptr(msg).to_string_lossy().to_string() }));
        }
        Ok(l)
    }
}

/// Le débruiteur est-il utilisable ? (le charge au premier appel)
pub fn available() -> bool {
    if !installed() {
        return false;
    }
    let mut g = LIB.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        *g = Some(load());
    }
    matches!(&*g, Some(Ok(_)))
}

/// Débruite une image HDR (lumière linéaire), guidée par la couleur de surface et les normales.
pub fn denoise(color: &[[f32; 3]], albedo: &[[f32; 3]], normal: &[[f32; 3]], w: usize, h: usize) -> Result<Vec<[f32; 3]>, String> {
    if !available() {
        return Err("débruiteur non installé".into());
    }
    let g = LIB.lock().unwrap_or_else(|e| e.into_inner());
    let Some(Ok(l)) = &*g else { return Err("débruiteur indisponible".into()) };
    let bytes = w * h * 12;
    unsafe {
        let filter = (l.new_filter)(l.device, c"RT".as_ptr());
        let mk = |data: &[[f32; 3]]| {
            let b = (l.new_buffer)(l.device, bytes);
            (l.write_buffer)(b, 0, bytes, data.as_ptr() as *const _);
            b
        };
        let (bc, ba, bn) = (mk(color), mk(albedo), mk(normal));
        let bo = (l.new_buffer)(l.device, bytes);
        for (name, b) in [(c"color", bc), (c"albedo", ba), (c"normal", bn), (c"output", bo)] {
            (l.set_image)(filter, name.as_ptr(), b, FORMAT_FLOAT3, w, h, 0, 0, 0);
        }
        (l.set_bool)(filter, c"hdr".as_ptr(), true);
        (l.commit_filter)(filter);
        (l.execute)(filter);
        let mut msg: *const std::ffi::c_char = std::ptr::null();
        let err = (l.error)(l.device, &mut msg);
        let mut out = vec![[0.0f32; 3]; w * h];
        if err == 0 {
            (l.read_buffer)(bo, 0, bytes, out.as_mut_ptr() as *mut _);
        }
        (l.release_filter)(filter);
        for b in [bc, ba, bn, bo] {
            (l.release_buffer)(b);
        }
        if err != 0 {
            return Err(format!("débruiteur : {}", if msg.is_null() { "erreur".into() } else { std::ffi::CStr::from_ptr(msg).to_string_lossy().to_string() }));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn debruiteur_ia() {
        // Seulement si la bibliothèque est déjà installée sur cette machine
        if !super::installed() {
            return;
        }
        assert!(super::available(), "{:?}", super::load().err());
        let (w, h) = (32, 32);
        let mut s = 1u32;
        let mut rnd = || {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s % 1000) as f32 / 1000.0
        };
        let color: Vec<[f32; 3]> = (0..w * h).map(|_| { let v = 0.5 + (rnd() - 0.5) * 0.6; [v, v, v] }).collect();
        let albedo = vec![[0.8f32; 3]; w * h];
        let normal = vec![[0.0f32, 0.0, 1.0]; w * h];
        let out = super::denoise(&color, &albedo, &normal, w, h).unwrap();
        let var = |v: &[[f32; 3]]| { let m = v.iter().map(|c| c[0]).sum::<f32>() / v.len() as f32; v.iter().map(|c| (c[0] - m).powi(2)).sum::<f32>() / v.len() as f32 };
        assert!(var(&out) < var(&color) * 0.2, "{} {}", var(&out), var(&color));
    }
}
