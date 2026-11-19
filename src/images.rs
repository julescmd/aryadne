//! Chargement d'images en arrière-plan, avec cache de miniatures et extraction de palette.
//! Indépendant de l'extension (majuscules, mauvaise extension), gère les très grandes
//! images et les formats 3D courants (TGA, TIFF, EXR, HDR).
use eframe::egui::{self, Color32, ColorImage, TextureHandle, TextureOptions};
use image::DynamicImage;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

pub const EXT: [&str; 12] = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "tga", "hdr", "exr", "jfif"];
const WORKERS: usize = 4;

pub fn is_image(rel: &str) -> bool {
    let e = rel.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    EXT.contains(&e.as_str())
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Size {
    Thumb,
    Full,
}

impl Size {
    fn max_side(self) -> u32 {
        match self {
            Size::Thumb => 360,
            Size::Full => 2048,
        }
    }
}

pub struct Loaded {
    pub tex: TextureHandle,
    /// Dimensions d'origine du fichier.
    pub dims: [u32; 2],
    pub palette: Vec<Color32>,
}

#[derive(Clone)]
pub enum Slot {
    Loading,
    Ready(Arc<Loaded>),
    Failed(String),
}

type Msg = ((String, Size), Result<Loaded, String>);

pub struct Images {
    root: String,
    slots: HashMap<(String, Size), Slot>,
    queue: VecDeque<(String, Size)>,
    inflight: usize,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl Default for Images {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { root: String::new(), slots: HashMap::new(), queue: VecDeque::new(), inflight: 0, tx, rx }
    }
}

impl Images {
    pub fn set_root(&mut self, root: &str) {
        if self.root != root {
            *self = Self { root: root.to_string(), ..Default::default() };
        }
    }

    /// État de l'image ; lance le chargement si besoin.
    pub fn get(&mut self, rel: &str, size: Size) -> Slot {
        let key = (rel.to_string(), size);
        if let Some(s) = self.slots.get(&key) {
            return s.clone();
        }
        self.slots.insert(key.clone(), Slot::Loading);
        // Les grandes images passent devant : elles sont demandées quand on les regarde.
        if size == Size::Full {
            self.queue.push_front(key);
        } else {
            self.queue.push_back(key);
        }
        Slot::Loading
    }

    /// Image déjà prête, sans en lancer le chargement.
    pub fn peek(&self, rel: &str, size: Size) -> Option<Arc<Loaded>> {
        match self.slots.get(&(rel.to_string(), size)) {
            Some(Slot::Ready(l)) => Some(l.clone()),
            _ => None,
        }
    }

    pub fn forget(&mut self, rel: &str) {
        self.slots.retain(|(r, _), _| r != rel);
        self.queue.retain(|(r, _)| r != rel);
    }

    /// À appeler à chaque image : récupère les résultats et lance de nouveaux chargements.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((key, r)) = self.rx.try_recv() {
            self.inflight -= 1;
            let slot = match r {
                Ok(l) => Slot::Ready(Arc::new(l)),
                Err(e) => Slot::Failed(e),
            };
            if self.slots.contains_key(&key) {
                self.slots.insert(key, slot);
            }
        }
        while self.inflight < WORKERS {
            let Some(key) = self.queue.pop_front() else { break };
            if !matches!(self.slots.get(&key), Some(Slot::Loading)) {
                continue;
            }
            self.inflight += 1;
            let (tx, ctx, path) = (self.tx.clone(), ctx.clone(), Path::new(&self.root).join(&key.0));
            std::thread::spawn(move || {
                let r = load(&ctx, &path, &key.0, key.1);
                let _ = tx.send((key, r));
                ctx.request_repaint();
            });
        }
        if self.inflight > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}

fn load(ctx: &egui::Context, path: &Path, rel: &str, size: Size) -> Result<Loaded, String> {
    let reader = image::ImageReader::open(path).map_err(|e| e.to_string())?.with_guessed_format().map_err(|e| e.to_string())?;
    let mut reader = reader;
    reader.no_limits();
    let img = reader.decode().map_err(|e| format!("Image illisible : {e}"))?;
    let dims = [img.width(), img.height()];
    let img = to_display(img);
    let max = size.max_side();
    let small = if img.width() > max || img.height() > max { img.thumbnail(max, max) } else { img };
    let rgba = small.to_rgba8();
    let color = ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
    let palette = palette(&small);
    let tex = ctx.load_texture(format!("{rel}#{size:?}"), color, TextureOptions::LINEAR);
    Ok(Loaded { tex, dims, palette })
}

/// Les EXR/HDR sont en lumière linéaire : tone mapping simple + gamma pour l'affichage.
fn to_display(img: DynamicImage) -> DynamicImage {
    match img {
        DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_) => {
            let f = img.to_rgba32f();
            let (w, h) = f.dimensions();
            let mut out = image::RgbaImage::new(w, h);
            for (o, p) in out.pixels_mut().zip(f.pixels()) {
                let map = |c: f32| {
                    let c = c.max(0.0);
                    ((c / (1.0 + c)).powf(1.0 / 2.2) * 255.0).round().clamp(0.0, 255.0) as u8
                };
                *o = image::Rgba([map(p[0]), map(p[1]), map(p[2]), (p[3].clamp(0.0, 1.0) * 255.0) as u8]);
            }
            DynamicImage::ImageRgba8(out)
        }
        other => other,
    }
}

/// 5 couleurs dominantes (k-moyennes sur une version réduite), de la plus présente à la moins présente.
pub fn palette(img: &DynamicImage) -> Vec<Color32> {
    let tiny = img.thumbnail(64, 64).to_rgb8();
    let px: Vec<[f32; 3]> = tiny.pixels().map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
    if px.is_empty() {
        return vec![];
    }
    let k = 5.min(px.len());
    let mut sorted = px.clone();
    sorted.sort_by(|a, b| (a[0] + a[1] + a[2]).total_cmp(&(b[0] + b[1] + b[2])));
    let mut centers: Vec<[f32; 3]> = (0..k).map(|i| sorted[(i * 2 + 1) * sorted.len() / (k * 2)]).collect();
    let mut counts = vec![0usize; k];
    for _ in 0..10 {
        let mut sums = vec![[0.0f32; 3]; k];
        counts = vec![0; k];
        for p in &px {
            let j = (0..k)
                .min_by(|&a, &b| dist2(p, &centers[a]).total_cmp(&dist2(p, &centers[b])))
                .unwrap();
            counts[j] += 1;
            for c in 0..3 {
                sums[j][c] += p[c];
            }
        }
        for j in 0..k {
            if counts[j] > 0 {
                centers[j] = [sums[j][0] / counts[j] as f32, sums[j][1] / counts[j] as f32, sums[j][2] / counts[j] as f32];
            }
        }
    }
    let mut order: Vec<usize> = (0..k).filter(|&j| counts[j] > 0).collect();
    order.sort_by(|&a, &b| counts[b].cmp(&counts[a]));
    order.iter().map(|&j| Color32::from_rgb(centers[j][0] as u8, centers[j][1] as u8, centers[j][2] as u8)).collect()
}

fn dist2(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Distance entre deux palettes (0 = identiques), pondérée par l'ordre d'importance.
pub fn palette_distance(a: &[Color32], b: &[Color32]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return f32::MAX;
    }
    let f = |c: &Color32| [c.r() as f32, c.g() as f32, c.b() as f32];
    let mut total = 0.0;
    let mut weight = 0.0;
    for (i, ca) in a.iter().enumerate() {
        let w = 1.0 / (i as f32 + 1.0);
        let best = b.iter().map(|cb| dist2(&f(ca), &f(cb))).fold(f32::MAX, f32::min);
        total += best.sqrt() * w;
        weight += w;
    }
    total / weight
}

pub fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_dominante() {
        let mut img = image::RgbImage::new(100, 100);
        for (x, _, p) in img.enumerate_pixels_mut() {
            *p = if x < 70 { image::Rgb([200, 20, 20]) } else { image::Rgb([20, 20, 200]) };
        }
        let pal = palette(&DynamicImage::ImageRgb8(img));
        assert!(pal[0].r() > 150 && pal[0].b() < 60, "la couleur dominante doit être le rouge");
        assert!(palette_distance(&pal, &pal) < 1.0);
        assert!(is_image("refs/PHOTO.JPG") && is_image("rendu.exr") && !is_image("note.md"));
    }
}
