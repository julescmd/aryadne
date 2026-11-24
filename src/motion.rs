//! Motion design : des compositions animées (`.arymotion`), comme After Effects en plus simple.
//!
//! Une composition a une taille, une cadence et une durée, et une pile de calques (le premier est
//! au-dessus). Chaque calque est un texte, une forme ou une image, avec quatre propriétés animables
//! (position, échelle, rotation, opacité) : sans clé, une valeur fixe ; avec des clés, la valeur
//! passe de l'une à l'autre en suivant le lissage de la clé de départ. Un texte peut aussi entrer
//! lettre par lettre ou mot par mot (« animateur de texte »).
//!
//! Le rendu est fait au processeur, le même pour l'aperçu et pour l'export MP4 (ffmpeg).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

pub fn is_compo(rel: &str) -> bool {
    crate::formats::has(rel, "motion")
}

/// Lissage d'une clé vers la suivante
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Debug, Hash)]
pub enum Ease {
    Linear,
    #[default]
    Smooth,
    Out,
    Bounce,
    Spring,
    /// La valeur reste jusqu'à la clé suivante
    Hold,
}

pub const EASES: [(Ease, &str); 5] = [(Ease::Linear, "Linéaire"), (Ease::Smooth, "Doux"), (Ease::Bounce, "Rebond"), (Ease::Spring, "Ressort"), (Ease::Hold, "Constant")];

impl Ease {
    /// De 0 à 1 ; peut dépasser 1 en chemin (rebond, ressort) mais arrive toujours à 1
    pub fn apply(self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        match self {
            Ease::Linear => x,
            Ease::Smooth => {
                if x < 0.5 {
                    4.0 * x * x * x
                } else {
                    1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
                }
            }
            Ease::Out => 1.0 - (1.0 - x).powi(3),
            Ease::Bounce => {
                let (c1, c3) = (1.70158, 2.70158);
                1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
            }
            Ease::Spring => 1.0 - (-6.0 * x).exp() * (4.5 * std::f32::consts::PI * x).cos(),
            Ease::Hold => {
                if x >= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Key {
    pub t: f32,
    pub v: [f32; 2],
    #[serde(default)]
    pub ease: Ease,
}

/// Une propriété animable : une valeur fixe, ou des clés
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct Track {
    pub v: [f32; 2],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<Key>,
}

/// Deux clés plus proches que ça sont la même (une demi-image à 60 i/s)
pub const KEY_EPS: f32 = 1.0 / 120.0;

impl Track {
    pub fn new(a: f32, b: f32) -> Track {
        Track { v: [a, b], keys: vec![] }
    }

    pub fn animated(&self) -> bool {
        !self.keys.is_empty()
    }

    pub fn at(&self, t: f32) -> [f32; 2] {
        let k = &self.keys;
        match k.len() {
            0 => self.v,
            _ if t <= k[0].t => k[0].v,
            n if t >= k[n - 1].t => k[n - 1].v,
            _ => {
                let i = k.iter().rposition(|a| a.t <= t).unwrap_or(0);
                let (a, b) = (&k[i], &k[i + 1]);
                let e = a.ease.apply((t - a.t) / (b.t - a.t).max(1e-6));
                [a.v[0] + (b.v[0] - a.v[0]) * e, a.v[1] + (b.v[1] - a.v[1]) * e]
            }
        }
    }

    pub fn key_at(&self, t: f32) -> Option<usize> {
        self.keys.iter().position(|k| (k.t - t).abs() < KEY_EPS)
    }

    /// Change la valeur à l'instant `t` : la clé de cet instant (créée au besoin) si la propriété est animée,
    /// sinon la valeur fixe
    pub fn set(&mut self, t: f32, v: [f32; 2]) {
        if self.keys.is_empty() {
            self.v = v;
            return;
        }
        match self.key_at(t) {
            Some(i) => self.keys[i].v = v,
            None => self.insert(Key { t, v, ease: Ease::Smooth }),
        }
    }

    pub fn insert(&mut self, k: Key) {
        let i = self.keys.iter().position(|a| a.t > k.t).unwrap_or(self.keys.len());
        self.keys.insert(i, k);
    }

    /// Le losange de l'inspecteur : pose une clé à `t` avec la valeur actuelle, ou retire celle qui y est.
    /// La dernière clé retirée laisse sa valeur comme valeur fixe.
    pub fn toggle_key(&mut self, t: f32) {
        match self.key_at(t) {
            Some(i) => {
                let k = self.keys.remove(i);
                if self.keys.is_empty() {
                    self.v = k.v;
                }
            }
            None => {
                let v = self.at(t);
                self.insert(Key { t, v, ease: Ease::Smooth });
            }
        }
    }

    pub fn sort(&mut self) {
        self.keys.sort_by(|a, b| a.t.total_cmp(&b.t));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Debug, Hash)]
pub enum Unit {
    #[default]
    Letter,
    Word,
    Line,
}

/// Entrée d'un texte morceau par morceau : chaque morceau part `delay` s après le précédent,
/// monte de `rise` px et apparaît en `dur` s
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Reveal {
    #[serde(default)]
    pub by: Unit,
    pub delay: f32,
    pub rise: f32,
    pub dur: f32,
    #[serde(default)]
    pub ease: Ease,
}

impl Default for Reveal {
    fn default() -> Reveal {
        Reveal { by: Unit::Letter, delay: 0.04, rise: 60.0, dur: 0.5, ease: Ease::Bounce }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Kind {
    Text {
        text: String,
        size: f32,
        color: [u8; 3],
        #[serde(default)]
        font: String,
        /// Espace entre les lettres, en px
        #[serde(default)]
        tracking: f32,
        /// 0 à gauche, 1 centré, 2 à droite
        #[serde(default)]
        align: u8,
    },
    Rect {
        w: f32,
        h: f32,
        #[serde(default)]
        radius: f32,
        color: [u8; 3],
    },
    Ellipse {
        w: f32,
        h: f32,
        color: [u8; 3],
    },
    Image {
        /// Chemin dans le coffre
        path: String,
    },
}

impl Kind {
    pub fn label(&self) -> &'static str {
        match self {
            Kind::Text { .. } => "texte",
            Kind::Rect { .. } => "rectangle",
            Kind::Ellipse { .. } => "ellipse",
            Kind::Image { .. } => "image",
        }
    }
}

fn yes() -> bool {
    true
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Layer {
    pub name: String,
    pub kind: Kind,
    pub start: f32,
    pub end: f32,
    #[serde(default = "yes")]
    pub visible: bool,
    /// Centre du calque, en px de la composition
    pub pos: Track,
    /// En %
    pub scale: Track,
    /// En degrés (seule la première valeur compte)
    pub rot: Track,
    /// En % (seule la première valeur compte)
    pub opacity: Track,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reveal: Option<Reveal>,
}

/// Les propriétés animables, dans l'ordre de l'inspecteur et de la timeline
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Prop {
    Pos,
    Scale,
    Rot,
    Opacity,
}

pub const PROPS: [(Prop, &str); 4] = [(Prop::Pos, "Position"), (Prop::Scale, "Échelle"), (Prop::Rot, "Rotation"), (Prop::Opacity, "Opacité")];

impl Layer {
    pub fn new(name: &str, kind: Kind, c: &Compo) -> Layer {
        Layer {
            name: name.to_string(),
            kind,
            start: 0.0,
            end: c.duration,
            visible: true,
            pos: Track::new(c.w as f32 / 2.0, c.h as f32 / 2.0),
            scale: Track::new(100.0, 100.0),
            rot: Track::new(0.0, 0.0),
            opacity: Track::new(100.0, 0.0),
            reveal: None,
        }
    }

    pub fn track(&self, p: Prop) -> &Track {
        match p {
            Prop::Pos => &self.pos,
            Prop::Scale => &self.scale,
            Prop::Rot => &self.rot,
            Prop::Opacity => &self.opacity,
        }
    }

    pub fn track_mut(&mut self, p: Prop) -> &mut Track {
        match p {
            Prop::Pos => &mut self.pos,
            Prop::Scale => &mut self.scale,
            Prop::Rot => &mut self.rot,
            Prop::Opacity => &mut self.opacity,
        }
    }

    pub fn live(&self, t: f32) -> bool {
        self.visible && t >= self.start && t < self.end
    }

    /// Les instants de toutes les clés du calque (sans doublon)
    pub fn key_times(&self) -> Vec<f32> {
        let mut v: Vec<f32> = PROPS.iter().flat_map(|(p, _)| self.track(*p).keys.iter().map(|k| k.t)).collect();
        v.sort_by(f32::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < KEY_EPS);
        v
    }

    /// Passage du repère du calque (px, origine au centre) à celui de la composition, à l'instant `t`
    pub fn xform(&self, t: f32) -> Xform {
        let p = self.pos.at(t);
        let s = self.scale.at(t);
        let r = self.rot.at(t)[0].to_radians();
        Xform { pos: p, sx: s[0] / 100.0, sy: s[1] / 100.0, cos: r.cos(), sin: r.sin() }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Xform {
    pub pos: [f32; 2],
    pub sx: f32,
    pub sy: f32,
    pub cos: f32,
    pub sin: f32,
}

impl Xform {
    pub fn apply(&self, l: [f32; 2]) -> [f32; 2] {
        let (x, y) = (l[0] * self.sx, l[1] * self.sy);
        [self.pos[0] + x * self.cos - y * self.sin, self.pos[1] + x * self.sin + y * self.cos]
    }

    pub fn inverse(&self, w: [f32; 2]) -> Option<[f32; 2]> {
        if self.sx.abs() < 1e-5 || self.sy.abs() < 1e-5 {
            return None;
        }
        let (dx, dy) = (w[0] - self.pos[0], w[1] - self.pos[1]);
        let (x, y) = (dx * self.cos + dy * self.sin, -dx * self.sin + dy * self.cos);
        Some([x / self.sx, y / self.sy])
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Compo {
    pub w: u32,
    pub h: u32,
    pub fps: f32,
    pub duration: f32,
    pub bg: [u8; 3],
    pub layers: Vec<Layer>,
    /// Musique de la composition (chemin dans le coffre), jouée à l'export
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub music: Option<String>,
}

impl Default for Compo {
    /// Une composition neuve : un titre qui entre lettre par lettre, souligné d'un trait qui s'étire
    fn default() -> Compo {
        let mut c = Compo { w: 1920, h: 1080, fps: 30.0, duration: 6.0, bg: [0x14, 0x10, 0x1a], layers: vec![], music: None };
        let mut title = Layer::new("Titre", Kind::Text { text: "Titre".into(), size: 140.0, color: [0xf4, 0xee, 0xf1], font: "inter-bold".into(), tracking: -2.0, align: 1 }, &c);
        title.reveal = Some(Reveal::default());
        title.pos.v = [960.0, 500.0];
        let mut line = Layer::new("Soulignement", Kind::Rect { w: 420.0, h: 10.0, radius: 5.0, color: [0xcc, 0xac, 0xb9] }, &c);
        line.pos.v = [960.0, 610.0];
        line.scale.keys = vec![Key { t: 0.4, v: [0.0, 100.0], ease: Ease::Out }, Key { t: 1.2, v: [100.0, 100.0], ease: Ease::Smooth }];
        c.layers = vec![title, line];
        c
    }
}

impl Compo {
    pub fn read(path: &Path) -> Result<Compo, String> {
        let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| format!("{} : {e}", crate::i18n::t("composition illisible")))
    }

    pub fn write(&self, path: &Path) -> Result<(), String> {
        crate::vault::atomic_write(path, serde_json::to_string_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }

    pub fn frames(&self) -> u32 {
        (self.duration * self.fps).round().max(1.0) as u32
    }

    /// Un nom de calque qui n'existe pas encore (« Texte », « Texte 2 »…)
    pub fn fresh_name(&self, base: &str) -> String {
        let mut n = 1;
        loop {
            let name = if n == 1 { base.to_string() } else { format!("{base} {n}") };
            if !self.layers.iter().any(|l| l.name == name) {
                return name;
            }
            n += 1;
        }
    }
}

// ---------- Rendu ----------

fn font(name: &str) -> &'static ab_glyph::FontArc {
    use ab_glyph::FontArc;
    static FONTS: OnceLock<Vec<(&'static str, FontArc)>> = OnceLock::new();
    let list = FONTS.get_or_init(|| {
        let f = |b: &'static [u8]| FontArc::try_from_slice(b).expect("police intégrée");
        vec![
            ("inter-bold", f(include_bytes!("../assets/fonts/Inter-Bold.ttf"))),
            ("inter-semibold", f(include_bytes!("../assets/fonts/Inter-SemiBold.ttf"))),
            ("inter", f(include_bytes!("../assets/fonts/Inter-Regular.ttf"))),
            ("lato-bold", f(include_bytes!("../assets/fonts/Lato-Bold.ttf"))),
            ("lato", f(include_bytes!("../assets/fonts/Lato-Regular.ttf"))),
            ("mono", f(include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"))),
        ]
    });
    list.iter().find(|(n, _)| *n == name).map(|(_, f)| f).unwrap_or(&list[0].1)
}

/// Les polices proposées dans l'inspecteur : (identifiant, nom)
pub const FONTS: [(&str, &str); 6] = [("inter-bold", "Inter gras"), ("inter-semibold", "Inter demi-gras"), ("inter", "Inter"), ("lato-bold", "Lato gras"), ("lato", "Lato"), ("mono", "Mono")];

/// Un caractère d'un texte placé : position du pied (px du calque, origine en haut à gauche), et son morceau
struct Placed {
    ch: char,
    x: f32,
    y: f32,
    unit: usize,
}

struct TextLayout {
    w: f32,
    h: f32,
    glyphs: Vec<Placed>,
    units: usize,
}

fn layout_text(text: &str, size: f32, fname: &str, tracking: f32, align: u8, by: Unit) -> TextLayout {
    use ab_glyph::{Font, ScaleFont};
    let f = font(fname);
    let sf = f.as_scaled(ab_glyph::PxScale::from(size.max(1.0)));
    let line_h = sf.height() + sf.line_gap();
    let lines: Vec<&str> = if text.is_empty() { vec![""] } else { text.lines().collect() };
    let width = |l: &str| {
        let mut x = 0.0f32;
        let mut prev = None;
        for ch in l.chars() {
            let g = sf.glyph_id(ch);
            if let Some(p) = prev {
                x += sf.kern(p, g);
            }
            x += sf.h_advance(g) + tracking;
            prev = Some(g);
        }
        (x - tracking).max(0.0)
    };
    let widths: Vec<f32> = lines.iter().map(|l| width(l)).collect();
    let w = widths.iter().cloned().fold(0.0, f32::max);
    let mut glyphs = vec![];
    let (mut letter, mut word) = (0usize, 0usize);
    for (li, l) in lines.iter().enumerate() {
        let mut x = match align {
            1 => (w - widths[li]) / 2.0,
            2 => w - widths[li],
            _ => 0.0,
        };
        let y = li as f32 * line_h + sf.ascent();
        let mut prev = None;
        let mut in_word = false;
        for ch in l.chars() {
            let g = sf.glyph_id(ch);
            if let Some(p) = prev {
                x += sf.kern(p, g);
            }
            if ch.is_whitespace() {
                if in_word {
                    word += 1;
                }
                in_word = false;
            } else {
                in_word = true;
                let unit = match by {
                    Unit::Letter => letter,
                    Unit::Word => word,
                    Unit::Line => li,
                };
                glyphs.push(Placed { ch, x, y, unit });
                letter += 1;
            }
            x += sf.h_advance(g) + tracking;
            prev = Some(g);
        }
        if in_word {
            word += 1;
        }
    }
    let units = match by {
        Unit::Letter => letter,
        Unit::Word => word,
        Unit::Line => lines.len(),
    };
    TextLayout { w: w.max(size * 0.3), h: line_h * lines.len() as f32, glyphs, units }
}

/// Images du coffre déjà décodées
#[derive(Default)]
pub struct Cache {
    images: HashMap<String, Option<Arc<image::RgbaImage>>>,
    /// Contenus de calques déjà dessinés (formes, textes immobiles), par contenu et résolution
    bitmaps: HashMap<(u64, u32), Arc<Bitmap>>,
    root: PathBuf,
}

impl Cache {
    pub fn new(root: &Path) -> Cache {
        Cache { images: HashMap::new(), bitmaps: HashMap::new(), root: root.to_path_buf() }
    }

    pub fn image(&mut self, rel: &str) -> Option<Arc<image::RgbaImage>> {
        let root = self.root.clone();
        self.images
            .entry(rel.to_string())
            .or_insert_with(|| {
                let p = crate::vault::safe(&root.to_string_lossy(), rel).ok()?;
                let img = image::open(p).ok()?.to_rgba8();
                // Les très grandes images sont réduites : l'aperçu reste fluide
                let img = if img.width().max(img.height()) > 4096 { image::imageops::thumbnail(&img, img.width() * 4096 / img.width().max(img.height()), img.height() * 4096 / img.width().max(img.height())) } else { img };
                Some(Arc::new(img))
            })
            .clone()
    }
}

/// Taille d'un calque dans son repère (px), avant l'échelle
pub fn size(l: &Layer, cache: &mut Cache) -> [f32; 2] {
    match &l.kind {
        Kind::Text { text, size, font, tracking, align, .. } => {
            let t = layout_text(text, *size, font, *tracking, *align, Unit::Letter);
            [t.w, t.h]
        }
        Kind::Rect { w, h, .. } | Kind::Ellipse { w, h, .. } => [*w, *h],
        Kind::Image { path } => cache.image(path).map(|i| [i.width() as f32, i.height() as f32]).unwrap_or([200.0, 120.0]),
    }
}

/// Les quatre coins du calque dans la composition, à l'instant `t`
pub fn corners(l: &Layer, t: f32, cache: &mut Cache) -> [[f32; 2]; 4] {
    let [w, h] = size(l, cache);
    let x = l.xform(t);
    [x.apply([-w / 2.0, -h / 2.0]), x.apply([w / 2.0, -h / 2.0]), x.apply([w / 2.0, h / 2.0]), x.apply([-w / 2.0, h / 2.0])]
}

/// Le calque visible le plus haut sous ce point de la composition
pub fn pick(c: &Compo, t: f32, at: [f32; 2], cache: &mut Cache) -> Option<usize> {
    (0..c.layers.len()).find(|&i| {
        let l = &c.layers[i];
        if !l.live(t) {
            return false;
        }
        let [w, h] = size(l, cache);
        let pad = 6.0;
        l.xform(t).inverse(at).is_some_and(|p| p[0].abs() <= w / 2.0 + pad && p[1].abs() <= h / 2.0 + pad)
    })
}

/// Le contenu d'un calque en pixels : `img` couvre le repère du calque à partir de (ox, oy), `k` pixels par px
struct Bitmap {
    img: image::RgbaImage,
    ox: f32,
    oy: f32,
    k: f32,
}

fn shape_bitmap(w: f32, h: f32, k: f32, color: [u8; 3], cover: impl Fn(f32, f32) -> f32) -> Bitmap {
    let pad = 2.0 / k;
    let (bw, bh) = (((w + 2.0 * pad) * k).ceil().clamp(1.0, 8192.0) as u32, ((h + 2.0 * pad) * k).ceil().clamp(1.0, 8192.0) as u32);
    let (ox, oy) = (-w / 2.0 - pad, -h / 2.0 - pad);
    let mut img = image::RgbaImage::new(bw, bh);
    for (x, y, px) in img.enumerate_pixels_mut() {
        let lx = ox + (x as f32 + 0.5) / k;
        let ly = oy + (y as f32 + 0.5) / k;
        let a = cover(lx, ly);
        if a > 0.0 {
            *px = image::Rgba([color[0], color[1], color[2], (a.min(1.0) * 255.0) as u8]);
        }
    }
    Bitmap { img, ox, oy, k }
}

fn bitmap(l: &Layer, t: f32, k: f32, cache: &mut Cache) -> Option<Bitmap> {
    match &l.kind {
        Kind::Rect { w, h, radius, color } => {
            let (hw, hh) = (w / 2.0, h / 2.0);
            let r = radius.clamp(0.0, hw.min(hh));
            Some(shape_bitmap(*w, *h, k, *color, |x, y| {
                // Distance signée à un rectangle arrondi, en px du calque, puis en pixels
                let qx = x.abs() - (hw - r);
                let qy = y.abs() - (hh - r);
                let d = (qx.max(0.0).hypot(qy.max(0.0))) + qx.max(qy).min(0.0) - r;
                (0.5 - d * k).clamp(0.0, 1.0)
            }))
        }
        Kind::Ellipse { w, h, color } => {
            let (rx, ry) = ((w / 2.0).max(0.01), (h / 2.0).max(0.01));
            Some(shape_bitmap(*w, *h, k, *color, |x, y| {
                let n = (x / rx).hypot(y / ry);
                let d = (n - 1.0) * rx.min(ry);
                (0.5 - d * k).clamp(0.0, 1.0)
            }))
        }
        Kind::Image { path } => {
            let img = cache.image(path)?;
            let (w, h) = (img.width() as f32, img.height() as f32);
            Some(Bitmap { img: (*img).clone(), ox: -w / 2.0, oy: -h / 2.0, k: 1.0 })
        }
        Kind::Text { text, size, color, font: fname, tracking, align } => {
            use ab_glyph::{Font, ScaleFont};
            let by = l.reveal.as_ref().map(|r| r.by).unwrap_or_default();
            let lay = layout_text(text, *size, fname, *tracking, *align, by);
            let rise = l.reveal.as_ref().map(|r| r.rise.abs()).unwrap_or(0.0);
            let pad = size * 0.3 + rise;
            let (bw, bh) = (((lay.w + 2.0 * pad) * k).ceil().clamp(1.0, 8192.0) as u32, ((lay.h + 2.0 * pad) * k).ceil().clamp(1.0, 8192.0) as u32);
            let mut img = image::RgbaImage::from_pixel(bw, bh, image::Rgba([color[0], color[1], color[2], 0]));
            let f = font(fname);
            let sf = f.as_scaled(ab_glyph::PxScale::from(size * k));
            let local = t - l.start;
            for g in &lay.glyphs {
                let (dy, alpha) = match &l.reveal {
                    Some(r) => {
                        let x = (local - g.unit as f32 * r.delay) / r.dur.max(1e-3);
                        let e = r.ease.apply(x);
                        (r.rise * (1.0 - e), Ease::Linear.apply(x * 1.6))
                    }
                    None => (0.0, 1.0),
                };
                if alpha <= 0.0 {
                    continue;
                }
                let glyph = sf.scaled_glyph(g.ch);
                let glyph = ab_glyph::Glyph { position: ab_glyph::point((g.x + pad) * k, (g.y + dy + pad) * k), ..glyph };
                if let Some(o) = f.outline_glyph(glyph) {
                    let b = o.px_bounds();
                    o.draw(|gx, gy, cov| {
                        let (x, y) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                        if x >= 0 && y >= 0 && (x as u32) < bw && (y as u32) < bh {
                            let px = img.get_pixel_mut(x as u32, y as u32);
                            let a = (cov * alpha * 255.0).min(255.0) as u8;
                            px[3] = px[3].max(a);
                        }
                    });
                }
            }
            let _ = lay.units;
            Some(Bitmap { img, ox: -lay.w / 2.0 - pad, oy: -lay.h / 2.0 - pad, k })
        }
    }
}

fn sample(img: &image::RgbaImage, x: f32, y: f32) -> [f32; 4] {
    // Bilinéaire, en alpha prémultiplié ; hors de l'image : transparent
    let (w, h) = (img.width() as i32, img.height() as i32);
    let (x, y) = (x - 0.5, y - 0.5);
    let (x0, y0) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let mut out = [0.0f32; 4];
    for (dx, dy, wgt) in [(0, 0, (1.0 - fx) * (1.0 - fy)), (1, 0, fx * (1.0 - fy)), (0, 1, (1.0 - fx) * fy), (1, 1, fx * fy)] {
        let (px, py) = (x0 + dx, y0 + dy);
        if px < 0 || py < 0 || px >= w || py >= h || wgt <= 0.0 {
            continue;
        }
        let p = img.get_pixel(px as u32, py as u32);
        let a = p[3] as f32 / 255.0 * wgt;
        out[0] += p[0] as f32 * a;
        out[1] += p[1] as f32 * a;
        out[2] += p[2] as f32 * a;
        out[3] += a;
    }
    out
}

/// L'image de la composition à l'instant `t`, à l'échelle `scale` (1 : taille réelle)
pub fn render(c: &Compo, t: f32, scale: f32, cache: &mut Cache) -> image::RgbaImage {
    use rayon::prelude::*;
    let (w, h) = (((c.w as f32 * scale).round() as u32).max(1), ((c.h as f32 * scale).round() as u32).max(1));
    let mut out = image::RgbaImage::from_pixel(w, h, image::Rgba([c.bg[0], c.bg[1], c.bg[2], 255]));
    for l in c.layers.iter().rev() {
        if !l.live(t) {
            continue;
        }
        let op = (l.opacity.at(t)[0] / 100.0).clamp(0.0, 1.0);
        if op <= 0.0 {
            continue;
        }
        let x = l.xform(t);
        // Le contenu est calculé à la résolution où il sera vu (net même agrandi)
        let k = (scale * x.sx.abs().max(x.sy.abs())).clamp(0.02, 8.0);
        // Un contenu qui ne bouge pas à l'intérieur du calque se dessine une fois (l'export reste rapide)
        let still = match (&l.kind, &l.reveal) {
            (Kind::Text { text, .. }, Some(r)) => t - l.start >= text.chars().count() as f32 * r.delay + r.dur,
            _ => true,
        };
        let key = still.then(|| {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_string(&l.kind).unwrap_or_default().hash(&mut h);
            serde_json::to_string(&l.reveal).unwrap_or_default().hash(&mut h);
            (h.finish(), k.to_bits())
        });
        let cached = key.and_then(|k| cache.bitmaps.get(&k).cloned());
        let bm = match cached {
            Some(b) => b,
            None => {
                let Some(b) = bitmap(l, t, k, cache) else { continue };
                let b = Arc::new(b);
                if let Some(key) = key {
                    if cache.bitmaps.len() > 48 {
                        cache.bitmaps.clear();
                    }
                    cache.bitmaps.insert(key, b.clone());
                }
                b
            }
        };
        let (bw, bh) = (bm.img.width() as f32 / bm.k, bm.img.height() as f32 / bm.k);
        let pts = [[bm.ox, bm.oy], [bm.ox + bw, bm.oy], [bm.ox + bw, bm.oy + bh], [bm.ox, bm.oy + bh]].map(|p| x.apply(p));
        let x0 = (pts.iter().map(|p| p[0]).fold(f32::MAX, f32::min) * scale).floor().max(0.0) as u32;
        let x1 = (pts.iter().map(|p| p[0]).fold(f32::MIN, f32::max) * scale).ceil().min(w as f32) as u32;
        let y0 = (pts.iter().map(|p| p[1]).fold(f32::MAX, f32::min) * scale).floor().max(0.0) as u32;
        let y1 = (pts.iter().map(|p| p[1]).fold(f32::MIN, f32::max) * scale).ceil().min(h as f32) as u32;
        if x0 >= x1 || y0 >= y1 {
            continue;
        }
        let row_len = w as usize * 4;
        let raw: &mut [u8] = &mut out;
        raw.par_chunks_mut(row_len).enumerate().filter(|(y, _)| (*y as u32) >= y0 && (*y as u32) < y1).for_each(|(py, row)| {
            for px in x0..x1 {
                let world = [(px as f32 + 0.5) / scale, (py as f32 + 0.5) / scale];
                let Some(lp) = x.inverse(world) else { return };
                let s = sample(&bm.img, (lp[0] - bm.ox) * bm.k, (lp[1] - bm.oy) * bm.k);
                let a = s[3] * op;
                if a <= 0.001 {
                    continue;
                }
                let i = px as usize * 4;
                for ch in 0..3 {
                    let src = s[ch] / s[3].max(1e-6);
                    row[i + ch] = (row[i + ch] as f32 * (1.0 - a) + src * a).round().clamp(0.0, 255.0) as u8;
                }
            }
        });
    }
    out
}

/// Exporte la composition en MP4 (H.264), avec sa musique s'il y en a une. Rend le fichier écrit.
pub fn export(c: &Compo, out: &Path, root: &Path, progress: impl Fn(f32)) -> Result<PathBuf, String> {
    use std::io::Write;
    let ff = crate::ffmpeg::find("ffmpeg").ok_or(crate::i18n::t("ffmpeg n'est pas installé."))?;
    // H.264 demande des dimensions paires
    let (w, h) = (c.w & !1, c.h & !1);
    let mut cmd = crate::ffmpeg::command(ff);
    cmd.args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{w}x{h}"), "-r", &format!("{}", c.fps), "-i", "-"]);
    let music = c.music.as_ref().and_then(|m| crate::vault::safe(&root.to_string_lossy(), m).ok()).filter(|p| p.exists());
    if let Some(m) = &music {
        cmd.arg("-i").arg(m).args(["-map", "0:v", "-map", "1:a", "-c:a", "aac", "-b:a", "192k"]);
    }
    cmd.args(["-t", &format!("{:.3}", c.duration), "-c:v", "libx264", "-preset", "medium", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart"]).arg(out);
    cmd.stdin(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).stdout(std::process::Stdio::null());
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let mut cache = Cache::new(root);
    let n = c.frames();
    {
        let mut stdin = child.stdin.take().ok_or("ffmpeg")?;
        for f in 0..n {
            let mut img = render(c, f as f32 / c.fps, 1.0, &mut cache);
            if img.width() != w || img.height() != h {
                img = image::imageops::crop_imm(&img, 0, 0, w, h).to_image();
            }
            if stdin.write_all(img.as_raw()).is_err() {
                break;
            }
            progress((f + 1) as f32 / n as f32);
        }
    }
    let o = child.wait_with_output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!("ffmpeg : {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("erreur")));
    }
    Ok(out.to_path_buf())
}

/// Préréglages d'animation : ils posent des clés sur le calque, à partir de l'instant `t`
pub const PRESETS: [&str; 6] = ["Entrée lettre par lettre", "Rebond", "Glissement doux", "Fondu d'entrée", "Sortie en fondu", "Tour complet"];

pub fn apply_preset(l: &mut Layer, name: &str, t: f32) {
    let t = t.max(l.start);
    match name {
        "Entrée lettre par lettre" => {
            if matches!(l.kind, Kind::Text { .. }) {
                // L'animateur part du début du calque
                l.reveal = Some(Reveal::default());
                l.start = l.start.min(t);
            }
        }
        "Rebond" => {
            let s = l.scale.at(t);
            l.scale.keys.retain(|k| k.t < t - KEY_EPS || k.t > t + 0.6 + KEY_EPS);
            l.scale.insert(Key { t, v: [0.0, 0.0], ease: Ease::Bounce });
            l.scale.insert(Key { t: t + 0.6, v: s, ease: Ease::Smooth });
        }
        "Glissement doux" => {
            let p = l.pos.at(t);
            l.pos.keys.retain(|k| k.t < t - KEY_EPS || k.t > t + 0.8 + KEY_EPS);
            l.pos.insert(Key { t, v: [p[0] - 240.0, p[1]], ease: Ease::Out });
            l.pos.insert(Key { t: t + 0.8, v: p, ease: Ease::Smooth });
            fade(l, t, 0.5, true);
        }
        "Fondu d'entrée" => fade(l, t, 0.6, true),
        "Sortie en fondu" => {
            let e = l.end;
            fade(l, (e - 0.6).max(l.start), 0.6, false)
        }
        "Tour complet" => {
            let r = l.rot.at(t)[0];
            l.rot.insert(Key { t, v: [r, 0.0], ease: Ease::Smooth });
            l.rot.insert(Key { t: t + 1.0, v: [r + 360.0, 0.0], ease: Ease::Smooth });
            l.rot.sort();
        }
        _ => {}
    }
}

fn fade(l: &mut Layer, t: f32, d: f32, enter: bool) {
    let full = if l.opacity.animated() { l.opacity.keys.iter().map(|k| k.v[0]).fold(0.0, f32::max).max(1.0) } else { l.opacity.v[0].max(1.0) };
    let (a, b) = if enter { (0.0, full) } else { (full, 0.0) };
    l.opacity.keys.retain(|k| k.t < t - KEY_EPS || k.t > t + d + KEY_EPS);
    l.opacity.insert(Key { t, v: [a, 0.0], ease: Ease::Smooth });
    l.opacity.insert(Key { t: t + d, v: [b, 0.0], ease: Ease::Smooth });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lissages_et_cles() {
        for (e, _) in EASES {
            assert!((e.apply(1.0) - 1.0).abs() < 1e-3, "{e:?} doit arriver à 1");
            assert!(e.apply(0.0).abs() < 1e-3);
        }
        assert!(Ease::Bounce.apply(0.8) > 1.0, "le rebond dépasse");
        let mut t = Track::new(5.0, 0.0);
        assert_eq!(t.at(3.0), [5.0, 0.0]);
        t.toggle_key(0.0);
        t.set(1.0, [15.0, 0.0]);
        assert_eq!(t.keys.len(), 2);
        assert_eq!(t.at(2.0)[0], 15.0);
        assert!((t.at(0.5)[0] - 10.0).abs() < 1e-3, "doux : moitié à mi-chemin");
        t.toggle_key(1.0);
        t.toggle_key(0.0);
        assert!(!t.animated());
        assert_eq!(t.v[0], 5.0);
    }

    #[test]
    fn rendu_de_composition() {
        let mut c = Compo::default();
        let dir = std::env::temp_dir();
        let mut cache = Cache::new(&dir);
        // À 3 s le titre est entré et le trait est étiré : de la couleur au centre
        let img = render(&c, 3.0, 0.25, &mut cache);
        assert_eq!((img.width(), img.height()), (480, 270));
        let lit = |img: &image::RgbaImage| img.pixels().filter(|p| p[0] > 120).count();
        let full = lit(&img);
        assert!(full > 200, "texte et trait visibles ({full})");
        // Au tout début, rien n'est encore entré
        let empty = lit(&render(&c, 0.0, 0.25, &mut cache));
        assert!(empty < full / 4, "{empty} / {full}");
        // On clique sur le trait (en bas) puis sur le titre
        assert_eq!(pick(&c, 3.0, [960.0, 610.0], &mut cache), Some(1));
        assert_eq!(pick(&c, 3.0, [960.0, 500.0], &mut cache), Some(0));
        apply_preset(&mut c.layers[1], "Sortie en fondu", 0.0);
        assert!(c.layers[1].opacity.at(c.duration)[0] < 1.0);
        let s = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<Compo>(&s).unwrap(), c);
    }
}
