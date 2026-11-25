//! Éléments vivants d'un board : les vidéos et les montages se lisent sur place (en boucle ou non),
//! les modèles 3D se tournent à la souris ou tournent tout seuls.

use crate::ffmpeg;
use eframe::egui;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};

pub fn is_playable(rel: &str) -> bool {
    crate::video::is_video(rel) || crate::montage::is_project(rel)
}

pub fn is_model(rel: &str) -> bool {
    crate::model3d::is_model(rel)
}

pub struct Player {
    child: Child,
    rx: Receiver<Vec<u8>>,
    size: [usize; 2],
    pub tex: Option<egui::TextureHandle>,
    pub ended: bool,
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Player {
    /// Lance la lecture, à la taille d'affichage (proportions `aspect` = largeur / hauteur).
    pub fn start(root: &str, rel: &str, aspect: f32, looping: bool, ctx: &egui::Context) -> Option<Player> {
        let ff = ffmpeg::find("ffmpeg")?;
        let path = Path::new(root).join(rel);
        let w = 480u32;
        let h = (((w as f32 / aspect.max(0.1)) / 2.0).round() as u32 * 2).max(2);
        let mut cmd = ffmpeg::command(ff);
        cmd.args(["-v", "error"]);
        if crate::montage::is_project(rel) {
            let p = crate::montage::Project::read(&path).ok()?;
            let (inputs, filter, _, _) = crate::montage::graph(&p, &path, 0.0, w, h, true, false);
            cmd.args(inputs).args(["-filter_complex", &filter, "-map", "[vout]"]);
        } else {
            cmd.arg("-re");
            if looping {
                cmd.args(["-stream_loop", "-1"]);
            }
            cmd.arg("-i").arg(&path).args(["-an", "-vf", &format!("scale={w}:{h}")]);
        }
        let mut child = cmd.args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
        let mut out = child.stdout.take()?;
        let (tx, rx) = sync_channel(2);
        let len = (w * h * 4) as usize;
        let c2 = ctx.clone();
        std::thread::spawn(move || loop {
            let mut buf = vec![0u8; len];
            if out.read_exact(&mut buf).is_err() || tx.send(buf).is_err() {
                c2.request_repaint();
                break;
            }
            c2.request_repaint();
        });
        Some(Player { child, rx, size: [w as usize, h as usize], tex: None, ended: false })
    }

    /// Prend la dernière image arrivée.
    pub fn poll(&mut self, ctx: &egui::Context, id: &str) {
        let mut last = None;
        loop {
            match self.rx.try_recv() {
                Ok(b) => last = Some(b),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.ended = true;
                    break;
                }
            }
        }
        if let Some(b) = last {
            let img = egui::ColorImage::from_rgba_unmultiplied(self.size, &b);
            match &mut self.tex {
                Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                None => self.tex = Some(ctx.load_texture(format!("board-play-{id}"), img, egui::TextureOptions::LINEAR)),
            }
        }
    }
}

/// Un modèle 3D posé sur le board : chargé en arrière-plan, rendu à la demande.
pub struct ModelView {
    prep: Option<crate::model3d::Prepared>,
    rx: Option<Receiver<Result<crate::model3d::Prepared, String>>>,
    pub yaw: f32,
    pub pitch: f32,
    pub spin: bool,
    pub error: Option<String>,
    tex: Option<egui::TextureHandle>,
    drawn: Option<(f32, f32, [usize; 2])>,
}

impl ModelView {
    pub fn load(root: &str, rel: &str) -> ModelView {
        let path = Path::new(root).join(rel);
        let (tx, rx) = sync_channel(1);
        std::thread::spawn(move || {
            let r = if crate::scene::is_scene(&path) {
                crate::scene::Scene::read(&path).and_then(|s| crate::scene::build(&s, &path).0)
            } else {
                crate::model3d::load(&path)
            };
            let _ = tx.send(r.map(|m| crate::model3d::Prepared::new(&m, m.z_up, 0)));
        });
        ModelView { prep: None, rx: Some(rx), yaw: 0.6, pitch: 0.35, spin: false, error: None, tex: None, drawn: None }
    }

    /// L'image du modèle à la taille demandée (recalculée quand la vue change).
    pub fn texture(&mut self, ctx: &egui::Context, id: &str, size: [usize; 2]) -> Option<&egui::TextureHandle> {
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(Ok(p)) => {
                    self.prep = Some(p);
                    self.rx = None;
                }
                Ok(Err(e)) => {
                    self.error = Some(e);
                    self.rx = None;
                }
                Err(_) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }
        if self.spin {
            self.yaw += 0.012;
            ctx.request_repaint();
        }
        let prep = self.prep.as_ref()?;
        let size = [size[0].clamp(32, 640), size[1].clamp(32, 640)];
        if self.drawn != Some((self.yaw, self.pitch, size)) {
            let cam = crate::model3d::Camera { yaw: self.yaw, pitch: self.pitch, ..Default::default() };
            let f = crate::model3d::render(prep, &cam, &crate::model3d::Look::default(), size[0], size[1]);
            let img = egui::ColorImage::from_rgba_unmultiplied([f.w, f.h], &f.rgba);
            match &mut self.tex {
                Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                None => self.tex = Some(ctx.load_texture(format!("board-model-{id}"), img, egui::TextureOptions::LINEAR)),
            }
            self.drawn = Some((self.yaw, self.pitch, size));
        }
        self.tex.as_ref()
    }
}

/// Un GIF animé : ses images (textures) et la durée de chacune (secondes)
pub struct Gif {
    pub frames: Vec<(egui::TextureHandle, f32)>,
    pub total: f32,
}

impl Gif {
    /// Décode le GIF (au plus 400 images, réduites à 1024 px de côté)
    pub fn load(ctx: &egui::Context, path: &std::path::Path, rel: &str) -> Option<Gif> {
        use image::AnimationDecoder;
        let f = std::io::BufReader::new(std::fs::File::open(path).ok()?);
        let dec = image::codecs::gif::GifDecoder::new(f).ok()?;
        let mut frames = vec![];
        let mut total = 0.0;
        for (k, fr) in dec.into_frames().take(400).enumerate() {
            let fr = fr.ok()?;
            let (n, d) = fr.delay().numer_denom_ms();
            // Les GIF sans délai passent à 10 images par seconde, comme les navigateurs
            let secs = if n == 0 { 0.1 } else { (n as f32 / d.max(1) as f32 / 1000.0).max(0.02) };
            let mut img = image::DynamicImage::ImageRgba8(fr.into_buffer());
            if img.width().max(img.height()) > 1024 {
                img = img.thumbnail(1024, 1024);
            }
            let rgba = img.to_rgba8();
            let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
            frames.push((ctx.load_texture(format!("gif-{rel}-{k}"), ci, egui::TextureOptions::LINEAR), secs));
            total += secs;
        }
        (!frames.is_empty()).then_some(Gif { frames, total })
    }

    /// L'image à montrer au temps `t` (secondes), et le temps avant la suivante
    pub fn at(&self, t: f64) -> (&egui::TextureHandle, f32) {
        let mut x = (t % self.total.max(0.01) as f64) as f32;
        for (tex, d) in &self.frames {
            if x < *d {
                return (tex, d - x);
            }
            x -= d;
        }
        (&self.frames[0].0, self.frames[0].1)
    }
}

/// L'état vivant d'un board : lecteurs et modèles, par fichier.
#[derive(Default)]
pub struct Live {
    pub players: HashMap<String, Player>,
    pub looping: HashMap<String, bool>,
    pub models: HashMap<String, ModelView>,
    /// GIF animés (None : illisible ou une seule image)
    pub gifs: HashMap<String, Option<Gif>>,
}

#[cfg(test)]
mod gif_tests {
    #[test]
    fn gif_anime() {
        use image::codecs::gif::{GifEncoder, Repeat};
        let path = std::env::temp_dir().join(format!("ary-anim-{}.gif", std::process::id()));
        {
            let f = std::fs::File::create(&path).unwrap();
            let mut enc = GifEncoder::new(f);
            enc.set_repeat(Repeat::Infinite).unwrap();
            for k in 0..3u8 {
                let img = image::RgbaImage::from_pixel(8, 8, image::Rgba([k * 80, 0, 0, 255]));
                enc.encode_frame(image::Frame::from_parts(img, 0, 0, image::Delay::from_numer_denom_ms(100, 1))).unwrap();
            }
        }
        let ctx = eframe::egui::Context::default();
        let g = super::Gif::load(&ctx, &path, "anim.gif").unwrap();
        assert_eq!(g.frames.len(), 3);
        assert!((g.total - 0.3).abs() < 1e-3);
        // 0,15 s : la deuxième image, encore 0,05 s avant la suivante ; ça boucle
        let (t, wait) = g.at(0.15);
        assert_eq!(t.id(), g.frames[1].0.id());
        assert!((wait - 0.05).abs() < 1e-3);
        assert_eq!(g.at(0.35).0.id(), g.frames[0].0.id());
        let _ = std::fs::remove_file(path);
    }
}
