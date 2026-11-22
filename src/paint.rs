//! Éditeur d'images à calques (`.aryimg`), dans l'esprit de Photoshop : calques avec opacité et modes
//! de fusion, pinceau, gomme, pot de peinture, pipette, sélection rectangulaire, déplacement de calque,
//! réglages (luminosité, contraste, teinte, saturation, inverser, noir et blanc), annuler, export PNG / JPG.
//!
//! Format du fichier : « ARYIMG1\n », la longueur (u32, petit-boutiste) puis l'en-tête JSON
//! (taille du document et réglages des calques), puis chaque calque en PNG précédé de sa longueur.

use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, Sense, Stroke};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub fn is_image_doc(rel: &str) -> bool {
    crate::formats::has(rel, "image")
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub enum Blend {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Add,
    Darken,
    Lighten,
    Difference,
}

impl Blend {
    pub const ALL: [(Blend, &'static str); 8] = [
        (Blend::Normal, "fusion normale"),
        (Blend::Multiply, "produit"),
        (Blend::Screen, "superposition (écran)"),
        (Blend::Overlay, "incrustation"),
        (Blend::Add, "addition"),
        (Blend::Darken, "obscurcir"),
        (Blend::Lighten, "éclaircir"),
        (Blend::Difference, "différence"),
    ];

    fn mix(self, d: f32, s: f32) -> f32 {
        match self {
            Blend::Normal => s,
            Blend::Multiply => d * s,
            Blend::Screen => 1.0 - (1.0 - d) * (1.0 - s),
            Blend::Overlay => {
                if d < 0.5 {
                    2.0 * d * s
                } else {
                    1.0 - 2.0 * (1.0 - d) * (1.0 - s)
                }
            }
            Blend::Add => (d + s).min(1.0),
            Blend::Darken => d.min(s),
            Blend::Lighten => d.max(s),
            Blend::Difference => (d - s).abs(),
        }
    }
}

#[derive(Clone)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend: Blend,
    /// Position du coin haut-gauche dans le document
    pub off: [i32; 2],
    pub w: usize,
    pub h: usize,
    /// RGBA, alpha non prémultiplié
    pub px: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct LayerHead {
    name: String,
    visible: bool,
    opacity: f32,
    blend: Blend,
    off: [i32; 2],
    w: usize,
    h: usize,
}

#[derive(Serialize, Deserialize)]
struct Head {
    version: u32,
    w: usize,
    h: usize,
    active: usize,
    layers: Vec<LayerHead>,
}

impl Layer {
    pub fn blank(name: &str, w: usize, h: usize) -> Layer {
        Layer { name: name.to_string(), visible: true, opacity: 1.0, blend: Blend::Normal, off: [0, 0], w, h, px: vec![0; w * h * 4] }
    }

    pub fn filled(name: &str, w: usize, h: usize, c: [u8; 4]) -> Layer {
        let mut l = Layer::blank(name, w, h);
        for p in l.px.chunks_exact_mut(4) {
            p.copy_from_slice(&c);
        }
        l
    }

    /// Pixel du calque aux coordonnées du document (transparent en dehors)
    fn at(&self, x: i32, y: i32) -> [u8; 4] {
        let (lx, ly) = (x - self.off[0], y - self.off[1]);
        if lx < 0 || ly < 0 || lx as usize >= self.w || ly as usize >= self.h {
            return [0; 4];
        }
        let i = (ly as usize * self.w + lx as usize) * 4;
        [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]]
    }
}

#[derive(Clone)]
pub struct Doc {
    pub w: usize,
    pub h: usize,
    pub layers: Vec<Layer>,
    pub active: usize,
}

impl Doc {
    pub fn new(w: usize, h: usize, white: bool) -> Doc {
        let base = if white { Layer::filled("fond", w, h, [255, 255, 255, 255]) } else { Layer::blank("calque 1", w, h) };
        Doc { w, h, layers: vec![base], active: 0 }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let head = Head {
            version: 1,
            w: self.w,
            h: self.h,
            active: self.active,
            layers: self.layers.iter().map(|l| LayerHead { name: l.name.clone(), visible: l.visible, opacity: l.opacity, blend: l.blend, off: l.off, w: l.w, h: l.h }).collect(),
        };
        let json = serde_json::to_vec(&head).map_err(|e| e.to_string())?;
        let mut out = b"ARYIMG1\n".to_vec();
        out.extend((json.len() as u32).to_le_bytes());
        out.extend(json);
        for l in &self.layers {
            let mut png = vec![];
            let img = image::RgbaImage::from_raw(l.w as u32, l.h as u32, l.px.clone()).ok_or("calque invalide")?;
            img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| e.to_string())?;
            out.extend((png.len() as u32).to_le_bytes());
            out.extend(png);
        }
        // Écrit à côté puis remplace : un plantage pendant l'écriture ne casse pas le fichier
        let tmp = path.with_extension("aryimg.tmp");
        std::fs::write(&tmp, out).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    pub fn load(path: &Path) -> Result<Doc, String> {
        let b = std::fs::read(path).map_err(|e| e.to_string())?;
        if b.is_empty() {
            return Ok(Doc::new(1920, 1080, true));
        }
        if !b.starts_with(b"ARYIMG1\n") {
            // Une image ordinaire renommée : un seul calque
            let img = image::load_from_memory(&b).map_err(|_| "fichier .aryimg illisible".to_string())?.to_rgba8();
            let (w, h) = (img.width() as usize, img.height() as usize);
            return Ok(Doc { w, h, layers: vec![Layer { px: img.into_raw(), ..Layer::blank("calque 1", w, h) }], active: 0 });
        }
        let mut at = 8;
        let rd = |at: &mut usize| -> Result<usize, String> {
            let v = b.get(*at..*at + 4).ok_or("fichier coupé")?;
            *at += 4;
            Ok(u32::from_le_bytes([v[0], v[1], v[2], v[3]]) as usize)
        };
        let n = rd(&mut at)?;
        let head: Head = serde_json::from_slice(b.get(at..at + n).ok_or("fichier coupé")?).map_err(|e| e.to_string())?;
        at += n;
        let mut layers = vec![];
        for lh in head.layers {
            let n = rd(&mut at)?;
            let img = image::load_from_memory(b.get(at..at + n).ok_or("fichier coupé")?).map_err(|e| e.to_string())?.to_rgba8();
            at += n;
            let (w, h) = (img.width() as usize, img.height() as usize);
            layers.push(Layer { name: lh.name, visible: lh.visible, opacity: lh.opacity, blend: lh.blend, off: lh.off, w, h, px: img.into_raw() });
        }
        if layers.is_empty() {
            layers.push(Layer::blank("calque 1", head.w, head.h));
        }
        Ok(Doc { w: head.w, h: head.h, active: head.active.min(layers.len() - 1), layers })
    }

    /// Mélange les calques visibles dans `out` (RGBA du document) pour la zone `r` (x0, y0, x1, y1).
    pub fn composite(&self, out: &mut [u8], r: [usize; 4]) {
        let [x0, y0, x1, y1] = r;
        for y in y0..y1.min(self.h) {
            for x in x0..x1.min(self.w) {
                let mut d = [0.0f32; 4];
                for l in self.layers.iter().filter(|l| l.visible && l.opacity > 0.0) {
                    let s = l.at(x as i32, y as i32);
                    if s[3] == 0 {
                        continue;
                    }
                    let a = s[3] as f32 / 255.0 * l.opacity;
                    let sc = [s[0] as f32 / 255.0, s[1] as f32 / 255.0, s[2] as f32 / 255.0];
                    let mut c = [0.0f32; 3];
                    for k in 0..3 {
                        // Sur du vide, le mode de fusion n'a rien à mélanger : on garde la couleur du calque
                        let b = if d[3] > 0.0 { l.blend.mix(d[k], sc[k]) } else { sc[k] };
                        c[k] = b;
                    }
                    let oa = a + d[3] * (1.0 - a);
                    for k in 0..3 {
                        d[k] = if oa > 0.0 { (c[k] * a + d[k] * d[3] * (1.0 - a)) / oa } else { 0.0 };
                    }
                    d[3] = oa;
                }
                let i = (y * self.w + x) * 4;
                for k in 0..4 {
                    out[i + k] = (d[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
        }
    }

    pub fn flatten(&self) -> image::RgbaImage {
        let mut out = vec![0u8; self.w * self.h * 4];
        self.composite(&mut out, [0, 0, self.w, self.h]);
        image::RgbaImage::from_raw(self.w as u32, self.h as u32, out).unwrap_or_default()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Move,
    Select,
    Brush,
    Eraser,
    Fill,
    Picker,
}

const TOOLS: [(Tool, &str, &str, &str); 6] = [
    (Tool::Move, "✥", "déplacer le calque", "V"),
    (Tool::Select, "⬚", "sélection rectangulaire", "M"),
    (Tool::Brush, "✎", "pinceau", "B"),
    (Tool::Eraser, "⌫", "gomme", "E"),
    (Tool::Fill, "◧", "pot de peinture", "G"),
    (Tool::Picker, "⊙", "pipette", "I"),
];

/// Réglage en cours de prévisualisation : on part toujours du calque d'avant.
#[derive(Clone, Copy, PartialEq)]
enum Adjust {
    BrightContrast,
    HueSat,
}

pub struct Editor {
    pub path: PathBuf,
    pub doc: Doc,
    pub tool: Tool,
    pub color: Color32,
    pub size: f32,
    pub hardness: f32,
    pub flow: f32,
    pub tolerance: f32,
    sel: Option<[i32; 4]>,
    /// Vue : zoom (pixels écran par pixel image) et décalage
    zoom: f32,
    pan: egui::Vec2,
    fit: bool,
    tex: Option<egui::TextureHandle>,
    comp: Vec<u8>,
    /// Zone à recomposer (x0, y0, x1, y1), ou None
    dirty: Option<[usize; 4]>,
    // Trait en cours : calque de départ et couverture maximale par pixel
    stroke: Option<(Layer, Vec<f32>, Pos2)>,
    drag_from: Option<(Pos2, [i32; 2])>,
    undo: Vec<(Vec<Layer>, usize)>,
    redo: Vec<(Vec<Layer>, usize)>,
    pub save_due: Option<Instant>,
    pub message: Option<String>,
    adjust: Option<(Adjust, Layer, [f32; 2])>,
    renaming: Option<usize>,
}

impl Editor {
    pub fn open(path: PathBuf) -> Result<Editor, String> {
        let doc = Doc::load(&path)?;
        let n = doc.w * doc.h * 4;
        Ok(Editor {
            path,
            comp: vec![0; n],
            dirty: Some([0, 0, doc.w, doc.h]),
            doc,
            tool: Tool::Brush,
            color: Color32::from_rgb(30, 30, 34),
            size: 18.0,
            hardness: 0.7,
            flow: 1.0,
            tolerance: 0.12,
            sel: None,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            fit: true,
            tex: None,
            stroke: None,
            drag_from: None,
            undo: vec![],
            redo: vec![],
            save_due: None,
            message: None,
            adjust: None,
            renaming: None,
        })
    }

    pub fn save_if_due(&mut self, force: bool) {
        if let Some(t) = self.save_due {
            if force || t.elapsed().as_secs_f32() > 1.5 {
                if let Err(e) = self.doc.save(&self.path) {
                    self.message = Some(e);
                }
                self.save_due = None;
            }
        }
    }

    fn touched(&mut self, r: [usize; 4]) {
        self.dirty = Some(match self.dirty {
            Some(d) => [d[0].min(r[0]), d[1].min(r[1]), d[2].max(r[2]), d[3].max(r[3])],
            None => r,
        });
        self.save_due = Some(Instant::now());
    }

    fn all(&mut self) {
        let (w, h) = (self.doc.w, self.doc.h);
        self.touched([0, 0, w, h]);
    }

    fn checkpoint(&mut self) {
        self.undo.push((self.doc.layers.clone(), self.doc.active));
        if self.undo.len() > 40 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn undo(&mut self) {
        if let Some((l, a)) = self.undo.pop() {
            self.redo.push((std::mem::replace(&mut self.doc.layers, l), self.doc.active));
            self.doc.active = a.min(self.doc.layers.len().saturating_sub(1));
            self.all();
        }
    }

    fn redo(&mut self) {
        if let Some((l, a)) = self.redo.pop() {
            self.undo.push((std::mem::replace(&mut self.doc.layers, l), self.doc.active));
            self.doc.active = a.min(self.doc.layers.len().saturating_sub(1));
            self.all();
        }
    }

    fn in_sel(&self, x: i32, y: i32) -> bool {
        self.sel.is_none_or(|s| x >= s[0] && y >= s[1] && x < s[2] && y < s[3])
    }

    /// Un tampon du pinceau (ou de la gomme) centré en `c` (coordonnées du document).
    fn dab(&mut self, c: Pos2) {
        let Some((start, cover, _)) = self.stroke.as_mut() else { return };
        let l = &mut self.doc.layers[self.doc.active];
        let r = self.size / 2.0;
        let (x0, y0) = ((c.x - r).floor() as i32, (c.y - r).floor() as i32);
        let (x1, y1) = ((c.x + r).ceil() as i32, (c.y + r).ceil() as i32);
        let hard = self.hardness.clamp(0.0, 0.99);
        let col = self.color;
        let erase = self.tool == Tool::Eraser;
        let sel = self.sel;
        for y in y0..=y1 {
            for x in x0..=x1 {
                if sel.is_some_and(|s| x < s[0] || y < s[1] || x >= s[2] || y >= s[3]) {
                    continue;
                }
                let (lx, ly) = (x - l.off[0], y - l.off[1]);
                if lx < 0 || ly < 0 || lx as usize >= l.w || ly as usize >= l.h {
                    continue;
                }
                let d = ((x as f32 + 0.5 - c.x).powi(2) + (y as f32 + 0.5 - c.y).powi(2)).sqrt() / r.max(0.5);
                if d > 1.0 {
                    continue;
                }
                // Bord doux : plein jusqu'à la dureté, puis décroissance
                let k = if d <= hard { 1.0 } else { 1.0 - (d - hard) / (1.0 - hard) };
                let k = k * k * (3.0 - 2.0 * k) * self.flow;
                let idx = ly as usize * l.w + lx as usize;
                if k <= cover[idx] {
                    continue;
                }
                cover[idx] = k;
                let i = idx * 4;
                let s = [start.px[i], start.px[i + 1], start.px[i + 2], start.px[i + 3]];
                if erase {
                    l.px[i + 3] = (s[3] as f32 * (1.0 - k)) as u8;
                } else {
                    // Peinture par-dessus le calque d'avant le trait : pas d'accumulation dans un même trait
                    let a = col.a() as f32 / 255.0 * k;
                    let sa = s[3] as f32 / 255.0;
                    let oa = a + sa * (1.0 - a);
                    let src = [col.r(), col.g(), col.b()];
                    for ch in 0..3 {
                        let v = if oa > 0.0 { (src[ch] as f32 * a + s[ch] as f32 * sa * (1.0 - a)) / oa } else { 0.0 };
                        l.px[i + ch] = v as u8;
                    }
                    l.px[i + 3] = (oa * 255.0) as u8;
                }
            }
        }
        let clamp = |v: i32, m: usize| v.clamp(0, m as i32) as usize;
        let (w, h) = (self.doc.w, self.doc.h);
        self.touched([clamp(x0, w), clamp(y0, h), clamp(x1 + 1, w), clamp(y1 + 1, h)]);
    }

    fn fill(&mut self, at: [i32; 2]) {
        let tol = self.tolerance;
        let l = &self.doc.layers[self.doc.active];
        let (lx, ly) = (at[0] - l.off[0], at[1] - l.off[1]);
        if lx < 0 || ly < 0 || lx as usize >= l.w || ly as usize >= l.h || !self.in_sel(at[0], at[1]) {
            return;
        }
        self.checkpoint();
        let col = [self.color.r(), self.color.g(), self.color.b(), self.color.a()];
        let sel = self.sel;
        let l = &mut self.doc.layers[self.doc.active];
        let (w, h) = (l.w, l.h);
        let off = l.off;
        let i0 = (ly as usize * w + lx as usize) * 4;
        let target = [l.px[i0], l.px[i0 + 1], l.px[i0 + 2], l.px[i0 + 3]];
        let near = |p: &[u8]| (0..4).map(|k| (p[k] as f32 - target[k] as f32).abs() / 255.0).fold(0.0f32, f32::max) <= tol;
        let mut seen = vec![false; w * h];
        let mut stack = vec![(lx as usize, ly as usize)];
        while let Some((x, y)) = stack.pop() {
            let idx = y * w + x;
            if seen[idx] {
                continue;
            }
            seen[idx] = true;
            let (dx, dy) = (x as i32 + off[0], y as i32 + off[1]);
            if sel.is_some_and(|s| dx < s[0] || dy < s[1] || dx >= s[2] || dy >= s[3]) || !near(&l.px[idx * 4..idx * 4 + 4]) {
                continue;
            }
            l.px[idx * 4..idx * 4 + 4].copy_from_slice(&col);
            if x > 0 {
                stack.push((x - 1, y));
            }
            if y > 0 {
                stack.push((x, y - 1));
            }
            if x + 1 < w {
                stack.push((x + 1, y));
            }
            if y + 1 < h {
                stack.push((x, y + 1));
            }
        }
        self.all();
    }

    /// Applique une transformation de couleur au calque actif (dans la sélection s'il y en a une).
    fn map_active(&mut self, base: &Layer, f: impl Fn([f32; 3]) -> [f32; 3]) {
        let sel = self.sel;
        let l = &mut self.doc.layers[self.doc.active];
        for y in 0..l.h {
            for x in 0..l.w {
                let (dx, dy) = (x as i32 + l.off[0], y as i32 + l.off[1]);
                if sel.is_some_and(|s| dx < s[0] || dy < s[1] || dx >= s[2] || dy >= s[3]) {
                    continue;
                }
                let i = (y * l.w + x) * 4;
                let c = f([base.px[i] as f32 / 255.0, base.px[i + 1] as f32 / 255.0, base.px[i + 2] as f32 / 255.0]);
                for k in 0..3 {
                    l.px[i + k] = (c[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
        }
        self.all();
    }

    fn apply_adjust(&mut self) {
        let Some((kind, base, v)) = self.adjust.clone() else { return };
        match kind {
            Adjust::BrightContrast => self.map_active(&base, |c| c.map(|x| (x - 0.5) * (1.0 + v[1]) + 0.5 + v[0])),
            Adjust::HueSat => self.map_active(&base, |c| {
                let mut hsv = egui::ecolor::Hsva::from_rgb(c);
                hsv.h = (hsv.h + v[0]).rem_euclid(1.0);
                hsv.s = (hsv.s * (1.0 + v[1])).clamp(0.0, 1.0);
                hsv.to_rgb()
            }),
        }
    }

    fn export(&mut self) {
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "image".into());
        let mut d = rfd::FileDialog::new().set_title(crate::i18n::t("exporter l'image")).set_file_name(format!("{stem}.png")).add_filter("PNG", &["png"]).add_filter("JPEG", &["jpg", "jpeg"]).add_filter("WebP", &["webp"]);
        if let Some(dir) = self.path.parent() {
            if !dir.to_string_lossy().contains(".app/untitled") {
                d = d.set_directory(dir);
            }
        }
        let Some(mut out) = d.save_file() else { return };
        if out.extension().is_none() {
            out.set_extension("png");
        }
        let img = self.doc.flatten();
        let ext = out.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let r = if ext == "jpg" || ext == "jpeg" {
            // Le JPEG n'a pas de transparence : fond blanc
            let mut rgb = image::RgbImage::new(img.width(), img.height());
            for (p, q) in img.pixels().zip(rgb.pixels_mut()) {
                let a = p[3] as f32 / 255.0;
                *q = image::Rgb([0, 1, 2].map(|k| (p[k] as f32 * a + 255.0 * (1.0 - a)) as u8));
            }
            rgb.save(&out)
        } else {
            img.save(&out)
        };
        self.message = Some(match r {
            Ok(()) => format!("{} {}", crate::i18n::t("exporté :"), out.to_string_lossy()),
            Err(e) => e.to_string(),
        });
    }

    fn add_image_layer(&mut self) {
        let Some(f) = rfd::FileDialog::new().add_filter("images", &["png", "jpg", "jpeg", "webp", "tga", "tif", "tiff", "bmp"]).pick_file() else { return };
        self.add_image_file(&f);
    }

    /// Une image du disque devient un nouveau calque (bouton, ou glissée depuis l'explorateur).
    fn add_image_file(&mut self, f: &std::path::Path) {
        let f = f.to_path_buf();
        match image::open(&f) {
            Ok(img) => {
                let img = img.to_rgba8();
                let (w, h) = (img.width() as usize, img.height() as usize);
                self.checkpoint();
                let name = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "image".into());
                let mut l = Layer { px: img.into_raw(), ..Layer::blank(&name, w, h) };
                // Centrée dans le document
                l.off = [(self.doc.w as i32 - w as i32) / 2, (self.doc.h as i32 - h as i32) / 2];
                self.doc.layers.insert(self.doc.active + 1, l);
                self.doc.active += 1;
                self.all();
            }
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let ctx = ui.ctx().clone();
        // ---- Barre du haut : outil, réglages de l'outil, menus
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            egui::containers::menu::MenuButton::new(crate::i18n::t("image ▾")).ui(ui, |ui| {
                ui.set_min_width(230.0);
                if ui.button(crate::i18n::t("luminosité et contraste…")).clicked() {
                    self.checkpoint();
                    self.adjust = Some((Adjust::BrightContrast, self.doc.layers[self.doc.active].clone(), [0.0, 0.0]));
                    ui.close();
                }
                if ui.button(crate::i18n::t("teinte et saturation…")).clicked() {
                    self.checkpoint();
                    self.adjust = Some((Adjust::HueSat, self.doc.layers[self.doc.active].clone(), [0.0, 0.0]));
                    ui.close();
                }
                if ui.button(crate::i18n::t("inverser les couleurs   Ctrl+I")).clicked() {
                    self.checkpoint();
                    let base = self.doc.layers[self.doc.active].clone();
                    self.map_active(&base, |c| c.map(|x| 1.0 - x));
                    ui.close();
                }
                if ui.button(crate::i18n::t("noir et blanc")).clicked() {
                    self.checkpoint();
                    let base = self.doc.layers[self.doc.active].clone();
                    self.map_active(&base, |c| [c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722; 3]);
                    ui.close();
                }
                ui.separator();
                if ui.button(crate::i18n::t("exporter en PNG, JPG ou WebP…")).clicked() {
                    self.export();
                    ui.close();
                }
            });
            egui::containers::menu::MenuButton::new(crate::i18n::t("calque ▾")).ui(ui, |ui| {
                ui.set_min_width(230.0);
                if ui.button(crate::i18n::t("nouveau calque   Ctrl+Maj+N")).clicked() {
                    self.new_layer();
                    ui.close();
                }
                if ui.button(crate::i18n::t("calque depuis une image…")).clicked() {
                    self.add_image_layer();
                    ui.close();
                }
                if ui.button(crate::i18n::t("dupliquer le calque   Ctrl+J")).clicked() {
                    self.duplicate_layer();
                    ui.close();
                }
                if ui.button(crate::i18n::t("fusionner avec le dessous   Ctrl+E")).clicked() {
                    self.merge_down();
                    ui.close();
                }
                if ui.button(crate::i18n::t("supprimer le calque")).clicked() {
                    self.delete_layer();
                    ui.close();
                }
            });
            ui.separator();
            for (t, icon, label, key) in TOOLS {
                let on = self.tool == t;
                let b = egui::Button::new(egui::RichText::new(icon).size(14.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { p.panel }).min_size(vec2(26.0, 22.0));
                if ui.add(b).on_hover_text(format!("{} ({key})", crate::i18n::t(label))).clicked() {
                    self.tool = t;
                }
            }
            ui.separator();
            let mut c = self.color;
            if ui.color_edit_button_srgba(&mut c).changed() {
                self.color = c;
            }
            match self.tool {
                Tool::Brush | Tool::Eraser => {
                    ui.add(egui::DragValue::new(&mut self.size).range(0.1..=10000.0).speed(0.5).prefix(format!("{} ", crate::i18n::t("diamètre")))).on_hover_text("[ ]");
                    ui.add(egui::Slider::new(&mut self.hardness, 0.0..=1.0).text(crate::i18n::t("dureté")).show_value(false));
                    ui.add(egui::Slider::new(&mut self.flow, 0.05..=1.0).text(crate::i18n::t("opacité")).show_value(false));
                }
                Tool::Fill => {
                    ui.add(egui::Slider::new(&mut self.tolerance, 0.0..=1.0).text(crate::i18n::t("tolérance")));
                }
                _ => {}
            }
            if self.sel.is_some() && ui.button(crate::i18n::t("désélectionner (Ctrl+D)")).clicked() {
                self.sel = None;
            }
            ui.separator();
            ui.label(egui::RichText::new(format!("{} × {} px · {} %", self.doc.w, self.doc.h, (self.zoom * 100.0).round())).size(11.0).color(p.muted));
            if ui.button(crate::i18n::t("ajuster")).clicked() {
                self.fit = true;
            }
        });
        if let Some(m) = &self.message {
            ui.label(egui::RichText::new(m).size(11.0).color(p.muted));
        }
        // ---- Calques, à droite
        match crate::layout::offer("paint-layers", "calques (retouche)") {
            Some(r) => crate::layout::draw_in(ui, r, p.panel, 8.0, |ui| self.layers_panel(ui)),
            None => {
                egui::Panel::right(egui::Id::unique(("paint-layers", self.path.to_string_lossy().to_string()))).resizable(true).max_size(480.0).default_size(230.0).min_size(180.0).frame(egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).inner_margin(8)).show(ui, |ui| self.layers_panel(ui));
            }
        }
        // ---- Réglage en cours : fenêtre avec aperçu en direct
        if let Some((kind, _, mut v)) = self.adjust.clone() {
            let mut close = None;
            egui::Window::new(crate::i18n::t(if kind == Adjust::BrightContrast { "luminosité et contraste" } else { "teinte et saturation" })).collapsible(false).resizable(false).show(&ctx, |ui| {
                let (a, b) = if kind == Adjust::BrightContrast { ("luminosité", "contraste") } else { ("teinte", "saturation") };
                let r1 = if kind == Adjust::HueSat { -0.5..=0.5 } else { -1.0..=1.0 };
                let mut ch = ui.add(egui::Slider::new(&mut v[0], r1).clamping(egui::SliderClamping::Never).text(crate::i18n::t(a))).changed();
                ch |= ui.add(egui::Slider::new(&mut v[1], -1.0..=1.0).text(crate::i18n::t(b))).changed();
                if ch {
                    if let Some(x) = &mut self.adjust {
                        x.2 = v;
                    }
                    self.apply_adjust();
                }
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::t("appliquer")).clicked() {
                        close = Some(true);
                    }
                    if ui.button(crate::i18n::t("annuler")).clicked() {
                        close = Some(false);
                    }
                });
            });
            match close {
                Some(true) => self.adjust = None,
                Some(false) => {
                    if let Some((_, base, _)) = self.adjust.take() {
                        self.doc.layers[self.doc.active] = base;
                        self.undo.pop();
                        self.all();
                    }
                }
                None => {}
            }
        }
        // ---- Toile
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        // Image glissée depuis l'explorateur : un calque de plus
        if let Some(rel) = crate::drop_rel(&resp, crate::images::has_preview) {
            let abs = std::path::Path::new(&crate::nodes::vault_root()).join(rel);
            self.add_image_file(&abs);
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, p.bg);
        if self.fit && rect.width() > 10.0 {
            self.zoom = ((rect.width() - 40.0) / self.doc.w as f32).min((rect.height() - 40.0) / self.doc.h as f32).clamp(0.02, 8.0);
            self.pan = egui::Vec2::ZERO;
            self.fit = false;
        }
        let size = vec2(self.doc.w as f32, self.doc.h as f32) * self.zoom;
        let canvas = Rect::from_center_size(rect.center() + self.pan, size);
        let zoom = self.zoom;
        let to_doc = move |q: Pos2| pos2((q.x - canvas.left()) / zoom, (q.y - canvas.top()) / zoom);
        let to_screen = move |x: f32, y: f32| pos2(canvas.left() + x * zoom, canvas.top() + y * zoom);
        // Damier sous la transparence
        let cell = 12.0;
        let clip = canvas.intersect(rect);
        let mut y = clip.top();
        let mut row = ((clip.top() - canvas.top()) / cell) as i32;
        while y < clip.bottom() {
            let mut x = clip.left();
            let mut col = ((clip.left() - canvas.left()) / cell) as i32;
            while x < clip.right() {
                if (row + col) % 2 == 0 {
                    painter.rect_filled(Rect::from_min_size(pos2(x, y), vec2(cell, cell)).intersect(clip), 0.0, Color32::from_gray(205));
                } else {
                    painter.rect_filled(Rect::from_min_size(pos2(x, y), vec2(cell, cell)).intersect(clip), 0.0, Color32::from_gray(235));
                }
                x = canvas.left() + (col + 1) as f32 * cell;
                col += 1;
            }
            y = canvas.top() + (row + 1) as f32 * cell;
            row += 1;
        }
        // Recomposer ce qui a changé, puis envoyer à la carte graphique
        if let Some(d) = self.dirty.take() {
            let d = [d[0], d[1], d[2].min(self.doc.w), d[3].min(self.doc.h)];
            if self.comp.len() != self.doc.w * self.doc.h * 4 {
                self.comp = vec![0; self.doc.w * self.doc.h * 4];
            }
            self.doc.composite(&mut self.comp, d);
            match &mut self.tex {
                Some(t) if d[2] > d[0] && d[3] > d[1] && !(d[0] == 0 && d[1] == 0 && d[2] == self.doc.w && d[3] == self.doc.h) => {
                    let (w, h) = (d[2] - d[0], d[3] - d[1]);
                    let mut part = Vec::with_capacity(w * h * 4);
                    for y in d[1]..d[3] {
                        let i = (y * self.doc.w + d[0]) * 4;
                        part.extend_from_slice(&self.comp[i..i + w * 4]);
                    }
                    t.set_partial([d[0], d[1]], egui::ColorImage::from_rgba_unmultiplied([w, h], &part), egui::TextureOptions::NEAREST);
                }
                _ => {
                    let img = egui::ColorImage::from_rgba_unmultiplied([self.doc.w, self.doc.h], &self.comp);
                    self.tex = Some(ctx.load_texture(format!("paint-{}", self.path.to_string_lossy()), img, egui::TextureOptions::NEAREST));
                }
            }
        }
        if let Some(t) = &self.tex {
            painter.image(t.id(), canvas, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        painter.rect_stroke(canvas, 0.0, Stroke::new(1.0, p.line), egui::StrokeKind::Outside);
        // ---- Souris
        let hover = ui.input(|i| i.pointer.hover_pos());
        let space = ui.input(|i| i.key_down(egui::Key::Space));
        if resp.dragged_by(egui::PointerButton::Middle) || (space && resp.dragged_by(egui::PointerButton::Primary)) {
            self.pan += resp.drag_delta();
        } else {
            if resp.drag_started_by(egui::PointerButton::Primary) {
                if let Some(at) = ui.input(|i| i.pointer.press_origin()) {
                    let d = to_doc(at);
                    match self.tool {
                        Tool::Brush | Tool::Eraser if self.doc.layers[self.doc.active].visible => {
                            self.checkpoint();
                            let l = self.doc.layers[self.doc.active].clone();
                            let n = l.w * l.h;
                            self.stroke = Some((l, vec![0.0; n], d));
                            self.dab(d);
                        }
                        Tool::Move => {
                            self.checkpoint();
                            self.drag_from = Some((d, self.doc.layers[self.doc.active].off));
                        }
                        Tool::Select => self.drag_from = Some((d, [0, 0])),
                        _ => {}
                    }
                }
            }
            if resp.dragged_by(egui::PointerButton::Primary) {
                if let Some(at) = resp.interact_pointer_pos() {
                    let d = to_doc(at);
                    match self.tool {
                        Tool::Brush | Tool::Eraser => {
                            // Tampons espacés le long du mouvement
                            if let Some(last) = self.stroke.as_ref().map(|s| s.2) {
                                let dist = (d - last).length();
                                let step = (self.size * 0.2).max(0.5);
                                let n = (dist / step).floor() as usize;
                                for k in 1..=n {
                                    let q = last + (d - last) * (k as f32 * step / dist);
                                    self.dab(q);
                                }
                                if n > 0 {
                                    if let Some(s) = &mut self.stroke {
                                        s.2 = last + (d - last) * (n as f32 * step / dist);
                                    }
                                }
                            }
                        }
                        Tool::Move => {
                            if let Some((from, off)) = self.drag_from {
                                let l = &mut self.doc.layers[self.doc.active];
                                l.off = [off[0] + (d.x - from.x).round() as i32, off[1] + (d.y - from.y).round() as i32];
                                self.all();
                            }
                        }
                        Tool::Select => {
                            if let Some((from, _)) = self.drag_from {
                                let (a, b) = (from, d);
                                let s = [a.x.min(b.x).floor() as i32, a.y.min(b.y).floor() as i32, a.x.max(b.x).ceil() as i32, a.y.max(b.y).ceil() as i32];
                                let s = [s[0].max(0), s[1].max(0), s[2].min(self.doc.w as i32), s[3].min(self.doc.h as i32)];
                                self.sel = (s[2] > s[0] && s[3] > s[1]).then_some(s);
                            }
                        }
                        _ => {}
                    }
                }
            }
            if resp.drag_stopped() {
                self.stroke = None;
                self.drag_from = None;
            }
            if resp.clicked() {
                if let Some(at) = resp.interact_pointer_pos() {
                    let d = to_doc(at);
                    let px = [d.x.floor() as i32, d.y.floor() as i32];
                    match self.tool {
                        Tool::Fill => self.fill(px),
                        Tool::Picker => {
                            if px[0] >= 0 && px[1] >= 0 && (px[0] as usize) < self.doc.w && (px[1] as usize) < self.doc.h {
                                let i = (px[1] as usize * self.doc.w + px[0] as usize) * 4;
                                self.color = Color32::from_rgba_unmultiplied(self.comp[i], self.comp[i + 1], self.comp[i + 2], self.comp[i + 3].max(1));
                            }
                        }
                        Tool::Brush | Tool::Eraser => {
                            self.checkpoint();
                            let l = self.doc.layers[self.doc.active].clone();
                            let n = l.w * l.h;
                            self.stroke = Some((l, vec![0.0; n], d));
                            self.dab(d);
                            self.stroke = None;
                        }
                        Tool::Select => self.sel = None,
                        Tool::Move => {}
                    }
                }
            }
        }
        // Molette : zoom autour du pointeur
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                if let Some(at) = hover {
                    let before = to_doc(at);
                    let z = (self.zoom * (scroll * 0.0025).exp()).clamp(0.02, 32.0);
                    self.zoom = z;
                    // Le point sous le pointeur reste sous le pointeur
                    let after_canvas_min = at - before.to_vec2() * z;
                    let center = after_canvas_min + vec2(self.doc.w as f32, self.doc.h as f32) * z / 2.0;
                    self.pan = center - rect.center();
                }
            }
        }
        // Sélection : cadre en pointillés
        if let Some(s) = self.sel {
            let r = Rect::from_min_max(to_screen(s[0] as f32, s[1] as f32), to_screen(s[2] as f32, s[3] as f32));
            let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
            painter.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.0, Color32::BLACK), 5.0, 5.0));
            painter.extend(egui::Shape::dashed_line(&[pts[1], pts[2], pts[3], pts[4], pts[0]], Stroke::new(1.0, Color32::WHITE), 5.0, 5.0));
        }
        // Rond du pinceau sous le pointeur
        if let (Some(at), true) = (hover.filter(|q| rect.contains(*q)), matches!(self.tool, Tool::Brush | Tool::Eraser)) {
            painter.circle_stroke(at, (self.size * self.zoom / 2.0).max(1.0), Stroke::new(1.0, Color32::from_black_alpha(160)));
            painter.circle_stroke(at, (self.size * self.zoom / 2.0).max(1.0) + 1.0, Stroke::new(1.0, Color32::from_white_alpha(160)));
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        // ---- Clavier
        if !ctx.egui_wants_keyboard_input() && (resp.hovered() || resp.has_focus()) {
            let (cmd, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
            let k = |key: egui::Key| ui.input(|i| i.key_pressed(key));
            if cmd && k(egui::Key::Z) {
                if shift {
                    self.redo();
                } else {
                    self.undo();
                }
            } else if cmd && k(egui::Key::Y) {
                self.redo();
            } else if cmd && k(egui::Key::D) {
                self.sel = None;
            } else if cmd && k(egui::Key::A) {
                self.sel = Some([0, 0, self.doc.w as i32, self.doc.h as i32]);
            } else if cmd && k(egui::Key::J) {
                self.duplicate_layer();
            } else if cmd && k(egui::Key::E) {
                self.merge_down();
            } else if cmd && shift && k(egui::Key::N) {
                self.new_layer();
            } else if cmd && k(egui::Key::I) {
                self.checkpoint();
                let base = self.doc.layers[self.doc.active].clone();
                self.map_active(&base, |c| c.map(|x| 1.0 - x));
            } else if !cmd {
                for (t, _, _, key) in TOOLS {
                    if let Some(kk) = egui::Key::from_name(key) {
                        if k(kk) {
                            self.tool = t;
                        }
                    }
                }
                if k(egui::Key::OpenBracket) {
                    self.size = (self.size / 1.2).max(1.0);
                }
                if k(egui::Key::CloseBracket) {
                    self.size = (self.size * 1.2).min(500.0);
                }
                if (k(egui::Key::Delete) || k(egui::Key::Backspace)) && self.sel.is_some() {
                    // Efface la sélection dans le calque actif
                    self.checkpoint();
                    let s = self.sel.unwrap_or_default();
                    let l = &mut self.doc.layers[self.doc.active];
                    for y in s[1]..s[3] {
                        for x in s[0]..s[2] {
                            let (lx, ly) = (x - l.off[0], y - l.off[1]);
                            if lx >= 0 && ly >= 0 && (lx as usize) < l.w && (ly as usize) < l.h {
                                l.px[(ly as usize * l.w + lx as usize) * 4 + 3] = 0;
                            }
                        }
                    }
                    self.all();
                }
            }
        }
        if self.stroke.is_some() || self.drag_from.is_some() {
            ctx.request_repaint();
        }
        self.save_if_due(false);
        if self.save_due.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }

    fn new_layer(&mut self) {
        self.checkpoint();
        let n = self.doc.layers.len() + 1;
        self.doc.layers.insert(self.doc.active + 1, Layer::blank(&format!("calque {n}"), self.doc.w, self.doc.h));
        self.doc.active += 1;
        self.all();
    }

    fn duplicate_layer(&mut self) {
        self.checkpoint();
        let mut l = self.doc.layers[self.doc.active].clone();
        l.name = format!("{} copie", l.name);
        self.doc.layers.insert(self.doc.active + 1, l);
        self.doc.active += 1;
        self.all();
    }

    fn delete_layer(&mut self) {
        if self.doc.layers.len() < 2 {
            return;
        }
        self.checkpoint();
        self.doc.layers.remove(self.doc.active);
        self.doc.active = self.doc.active.saturating_sub(1).min(self.doc.layers.len() - 1);
        self.all();
    }

    /// Fusionne le calque actif dans celui du dessous (à la taille du document).
    fn merge_down(&mut self) {
        let a = self.doc.active;
        if a == 0 {
            return;
        }
        self.checkpoint();
        let pair = Doc { w: self.doc.w, h: self.doc.h, layers: vec![self.doc.layers[a - 1].clone(), self.doc.layers[a].clone()], active: 1 };
        let mut out = vec![0u8; self.doc.w * self.doc.h * 4];
        pair.composite(&mut out, [0, 0, self.doc.w, self.doc.h]);
        let below = &self.doc.layers[a - 1];
        let merged = Layer { name: below.name.clone(), visible: true, opacity: 1.0, blend: below.blend, off: [0, 0], w: self.doc.w, h: self.doc.h, px: out };
        self.doc.layers[a - 1] = merged;
        self.doc.layers.remove(a);
        self.doc.active = a - 1;
        self.all();
    }

    fn layers_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        ui.label(crate::theme::section(&crate::i18n::t("CALQUES")));
        // Réglages du calque actif
        let a = self.doc.active;
        let mut changed = false;
        {
            let l = &mut self.doc.layers[a];
            ui.horizontal(|ui| {
                let cur = Blend::ALL.iter().find(|b| b.0 == l.blend).map(|b| b.1).unwrap_or("fusion normale");
                egui::ComboBox::from_id_salt("paint-blend").selected_text(crate::i18n::t(cur)).width(140.0).show_ui(ui, |ui| {
                    for (b, name) in Blend::ALL {
                        if ui.selectable_label(l.blend == b, crate::i18n::t(name)).clicked() {
                            l.blend = b;
                            changed = true;
                        }
                    }
                });
            });
            changed |= ui.add(egui::Slider::new(&mut l.opacity, 0.0..=1.0).text(crate::i18n::t("opacité"))).changed();
        }
        if changed {
            self.all();
        }
        ui.horizontal(|ui| {
            if ui.small_button("＋").on_hover_text(crate::i18n::t("nouveau calque")).clicked() {
                self.new_layer();
            }
            if ui.small_button("⧉").on_hover_text(crate::i18n::t("dupliquer")).clicked() {
                self.duplicate_layer();
            }
            if ui.small_button("⤓").on_hover_text(crate::i18n::t("fusionner avec le dessous")).clicked() {
                self.merge_down();
            }
            if ui.small_button("▲").on_hover_text(crate::i18n::t("monter")).clicked() && a + 1 < self.doc.layers.len() {
                self.checkpoint();
                self.doc.layers.swap(a, a + 1);
                self.doc.active = a + 1;
                self.all();
            }
            if ui.small_button("▼").on_hover_text(crate::i18n::t("descendre")).clicked() && a > 0 {
                self.checkpoint();
                self.doc.layers.swap(a, a - 1);
                self.doc.active = a - 1;
                self.all();
            }
            if ui.small_button("✕").on_hover_text(crate::i18n::t("supprimer")).clicked() {
                self.delete_layer();
            }
        });
        ui.separator();
        // La liste : le calque du haut en premier, comme dans Photoshop
        let mut toggle = None;
        let mut pick = None;
        egui::ScrollArea::vertical().id_salt("paint-layer-list").show(ui, |ui| {
            for i in (0..self.doc.layers.len()).rev() {
                let on = i == self.doc.active;
                ui.horizontal(|ui| {
                    let l = &mut self.doc.layers[i];
                    if ui.add(egui::Button::new(egui::RichText::new(if l.visible { "◉" } else { "○" }).size(13.0)).frame(false)).on_hover_text(crate::i18n::t("afficher / masquer")).clicked() {
                        toggle = Some(i);
                    }
                    if self.renaming == Some(i) {
                        let r = ui.add(egui::TextEdit::singleline(&mut l.name).desired_width(130.0));
                        r.request_focus();
                        if r.lost_focus() {
                            self.renaming = None;
                            self.save_due = Some(Instant::now());
                        }
                    } else {
                        let r = ui.add(egui::Button::selectable(on, egui::RichText::new(&l.name).size(12.0)).min_size(vec2(150.0, 22.0)));
                        if r.clicked() {
                            pick = Some(i);
                        }
                        if r.double_clicked() {
                            self.renaming = Some(i);
                        }
                    }
                });
            }
        });
        if let Some(i) = toggle {
            self.doc.layers[i].visible = !self.doc.layers[i].visible;
            self.all();
        }
        if let Some(i) = pick {
            self.doc.active = i;
        }
        ui.add_space(6.0);
        ui.label(egui::RichText::new(crate::i18n::t("double-clic : renommer")).size(10.0).color(p.muted));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calques_aller_retour_et_fusion() {
        let mut d = Doc::new(8, 6, true);
        let mut top = Layer::filled("rouge", 4, 4, [255, 0, 0, 255]);
        top.off = [2, 1];
        top.opacity = 0.5;
        d.layers.push(top);
        let path = std::env::temp_dir().join(format!("aryadne-paint-{}.aryimg", std::process::id()));
        d.save(&path).unwrap();
        let back = Doc::load(&path).unwrap();
        assert_eq!((back.w, back.h, back.layers.len()), (8, 6, 2));
        assert_eq!(back.layers[1].off, [2, 1]);
        let img = back.flatten();
        // Hors du calque rouge : blanc ; dedans : rose (rouge à moitié sur blanc)
        assert_eq!(img.get_pixel(0, 0).0, [255, 255, 255, 255]);
        let p = img.get_pixel(3, 2).0;
        assert!(p[0] == 255 && (p[1] as i32 - 128).abs() <= 1 && p[3] == 255, "{p:?}");
        // Mode produit : rouge × blanc = rouge
        let mut m = back.clone();
        m.layers[1].opacity = 1.0;
        m.layers[1].blend = Blend::Multiply;
        assert_eq!(m.flatten().get_pixel(3, 2).0, [255, 0, 0, 255]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn pinceau_et_pot() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("aryadne-paint2-{}.aryimg", std::process::id()));
        Doc::new(20, 20, false).save(&path).unwrap();
        let mut e = Editor::open(path.clone()).unwrap();
        e.color = Color32::from_rgb(0, 0, 255);
        e.size = 6.0;
        e.hardness = 0.9;
        let l = e.doc.layers[0].clone();
        e.stroke = Some((l, vec![0.0; 400], pos2(10.0, 10.0)));
        e.dab(pos2(10.0, 10.0));
        e.dab(pos2(10.2, 10.0));
        let px = &e.doc.layers[0].px[(10 * 20 + 10) * 4..(10 * 20 + 10) * 4 + 4];
        assert_eq!(px, &[0, 0, 255, 255]);
        e.stroke = None;
        // Pot de peinture : remplit le vide autour, pas le bleu
        e.color = Color32::from_rgb(255, 255, 0);
        e.fill([0, 0]);
        assert_eq!(&e.doc.layers[0].px[0..4], &[255, 255, 0, 255]);
        assert_eq!(e.doc.layers[0].px[(10 * 20 + 10) * 4 + 2], 255);
        let _ = std::fs::remove_file(&path);
    }
}
