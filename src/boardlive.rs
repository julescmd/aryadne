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

/// L'état vivant d'un board : lecteurs et modèles, par fichier.
#[derive(Default)]
pub struct Live {
    pub players: HashMap<String, Player>,
    pub looping: HashMap<String, bool>,
    pub models: HashMap<String, ModelView>,
}
