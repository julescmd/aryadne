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

pub fn is_psd(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("psd"))
}

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
    ColorBurn,
    LinearBurn,
    ColorDodge,
    SoftLight,
    HardLight,
    VividLight,
    LinearLight,
    PinLight,
    Exclusion,
    Subtract,
    Divide,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl Blend {
    /// Dans l'ordre de Photoshop, par groupes (un `None` sépare les groupes)
    pub const MENU: [Option<(Blend, &'static str)>; 28] = [
        Some((Blend::Normal, "Normal")),
        None,
        Some((Blend::Darken, "Obscurcir")),
        Some((Blend::Multiply, "Produit")),
        Some((Blend::ColorBurn, "Densité couleur +")),
        Some((Blend::LinearBurn, "Densité linéaire +")),
        None,
        Some((Blend::Lighten, "Éclaircir")),
        Some((Blend::Screen, "Superposition")),
        Some((Blend::ColorDodge, "Densité couleur -")),
        Some((Blend::Add, "Densité linéaire - (ajout)")),
        None,
        Some((Blend::Overlay, "Incrustation")),
        Some((Blend::SoftLight, "Lumière tamisée")),
        Some((Blend::HardLight, "Lumière crue")),
        Some((Blend::VividLight, "Lumière vive")),
        Some((Blend::LinearLight, "Lumière linéaire")),
        Some((Blend::PinLight, "Lumière ponctuelle")),
        None,
        Some((Blend::Difference, "Différence")),
        Some((Blend::Exclusion, "Exclusion")),
        Some((Blend::Subtract, "Soustraction")),
        Some((Blend::Divide, "Division")),
        None,
        Some((Blend::Hue, "Teinte")),
        Some((Blend::Saturation, "Saturation")),
        Some((Blend::Color, "Couleur")),
        Some((Blend::Luminosity, "Luminosité")),
    ];

    pub fn name(self) -> &'static str {
        Blend::MENU.iter().flatten().find(|b| b.0 == self).map(|b| b.1).unwrap_or("Normal")
    }

    /// Mélange une couleur `s` (calque) sur `d` (dessous), en 0..1
    fn mix(self, d: [f32; 3], s: [f32; 3]) -> [f32; 3] {
        let ch = |f: fn(f32, f32) -> f32| [f(d[0], s[0]), f(d[1], s[1]), f(d[2], s[2])];
        match self {
            Blend::Normal => s,
            Blend::Multiply => ch(|d, s| d * s),
            Blend::Screen => ch(|d, s| 1.0 - (1.0 - d) * (1.0 - s)),
            Blend::Overlay => ch(|d, s| if d < 0.5 { 2.0 * d * s } else { 1.0 - 2.0 * (1.0 - d) * (1.0 - s) }),
            Blend::Add => ch(|d, s| (d + s).min(1.0)),
            Blend::Darken => ch(f32::min),
            Blend::Lighten => ch(f32::max),
            Blend::Difference => ch(|d, s| (d - s).abs()),
            Blend::ColorBurn => ch(|d, s| if d >= 1.0 { 1.0 } else if s <= 0.0 { 0.0 } else { 1.0 - ((1.0 - d) / s).min(1.0) }),
            Blend::LinearBurn => ch(|d, s| (d + s - 1.0).max(0.0)),
            Blend::ColorDodge => ch(|d, s| if d <= 0.0 { 0.0 } else if s >= 1.0 { 1.0 } else { (d / (1.0 - s)).min(1.0) }),
            Blend::SoftLight => ch(|d, s| {
                if s <= 0.5 {
                    d - (1.0 - 2.0 * s) * d * (1.0 - d)
                } else {
                    let g = if d <= 0.25 { ((16.0 * d - 12.0) * d + 4.0) * d } else { d.sqrt() };
                    d + (2.0 * s - 1.0) * (g - d)
                }
            }),
            Blend::HardLight => ch(|d, s| if s < 0.5 { 2.0 * d * s } else { 1.0 - 2.0 * (1.0 - d) * (1.0 - s) }),
            Blend::VividLight => ch(|d, s| {
                if s < 0.5 {
                    if s <= 0.0 { 0.0 } else { (1.0 - (1.0 - d) / (2.0 * s)).max(0.0) }
                } else if s >= 1.0 {
                    1.0
                } else {
                    (d / (2.0 * (1.0 - s))).min(1.0)
                }
            }),
            Blend::LinearLight => ch(|d, s| (d + 2.0 * s - 1.0).clamp(0.0, 1.0)),
            Blend::PinLight => ch(|d, s| if s < 0.5 { d.min(2.0 * s) } else { d.max(2.0 * s - 1.0) }),
            Blend::Exclusion => ch(|d, s| d + s - 2.0 * d * s),
            Blend::Subtract => ch(|d, s| (d - s).max(0.0)),
            Blend::Divide => ch(|d, s| if s <= 0.0 { 1.0 } else { (d / s).min(1.0) }),
            Blend::Hue => set_lum(set_sat(s, sat(d)), lum(d)),
            Blend::Saturation => set_lum(set_sat(d, sat(s)), lum(d)),
            Blend::Color => set_lum(s, lum(d)),
            Blend::Luminosity => set_lum(d, lum(s)),
        }
    }
}

// Modes non séparables (teinte, saturation, couleur, luminosité), comme dans la norme de mélange des PDF
fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn clip_color(c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let (n, x) = (c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2]));
    let mut c = c;
    if n < 0.0 {
        c = c.map(|v| l + (v - l) * l / (l - n).max(1e-6));
    }
    if x > 1.0 {
        c = c.map(|v| l + (v - l) * (1.0 - l) / (x - l).max(1e-6));
    }
    c
}

fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip_color(c.map(|v| v + d))
}

fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let (n, x) = (c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2]));
    if x - n <= 1e-6 {
        return [0.0; 3];
    }
    c.map(|v| (v - n) * s / (x - n))
}

/// Calque de réglage : il change la couleur de tout ce qui est dessous (sans toucher aux pixels)
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum Adj {
    BrightContrast { b: f32, c: f32 },
    HueSat { h: f32, s: f32, l: f32 },
    Levels { black: f32, white: f32, gamma: f32 },
    Exposure { ev: f32 },
    Invert,
    BlackWhite,
    Threshold { t: f32 },
    Posterize { levels: f32 },
    /// Grain photo : quantité, taille des grains (pixels)
    Grain { amount: f32, size: f32 },
    /// Clarté : contraste des tons moyens (les noirs et blancs bougent peu)
    Clarity { amount: f32 },
    /// Voile : retire (ou ajoute) la brume claire qui délave l'image
    Dehaze { amount: f32 },
    /// Vibrance : sature surtout les couleurs ternes
    Vibrance { amount: f32 },
}

impl Adj {
    pub const NEW: [(&'static str, Adj); 12] = [
        ("Luminosité/Contraste", Adj::BrightContrast { b: 0.0, c: 0.0 }),
        ("Niveaux", Adj::Levels { black: 0.0, white: 1.0, gamma: 1.0 }),
        ("Exposition", Adj::Exposure { ev: 0.0 }),
        ("Teinte/Saturation", Adj::HueSat { h: 0.0, s: 0.0, l: 0.0 }),
        ("Noir et blanc", Adj::BlackWhite),
        ("Inverser", Adj::Invert),
        ("Seuil", Adj::Threshold { t: 0.5 }),
        ("Isohélie", Adj::Posterize { levels: 4.0 }),
        ("Grain", Adj::Grain { amount: 0.25, size: 1.5 }),
        ("Clarté", Adj::Clarity { amount: 0.4 }),
        ("Voile", Adj::Dehaze { amount: 0.3 }),
        ("Vibrance", Adj::Vibrance { amount: 0.4 }),
    ];

    pub fn name(&self) -> &'static str {
        Adj::NEW.iter().find(|(_, a)| std::mem::discriminant(a) == std::mem::discriminant(self)).map(|x| x.0).unwrap_or("Réglage")
    }

    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        match *self {
            Adj::BrightContrast { b, c: k } => c.map(|x| ((x - 0.5) * (1.0 + k) + 0.5 + b).clamp(0.0, 1.0)),
            Adj::HueSat { h, s, l } => {
                let mut hsv = egui::ecolor::Hsva::from_rgb(c);
                hsv.h = (hsv.h + h).rem_euclid(1.0);
                hsv.s = (hsv.s * (1.0 + s)).clamp(0.0, 1.0);
                let o = hsv.to_rgb();
                if l >= 0.0 { o.map(|x| x + (1.0 - x) * l) } else { o.map(|x| x * (1.0 + l)) }
            }
            Adj::Levels { black, white, gamma } => c.map(|x| (((x - black) / (white - black).max(1e-3)).clamp(0.0, 1.0)).powf(1.0 / gamma.max(0.05))),
            Adj::Exposure { ev } => c.map(|x| (x * 2f32.powf(ev)).clamp(0.0, 1.0)),
            Adj::Invert => c.map(|x| 1.0 - x),
            Adj::BlackWhite => [lum(c); 3],
            Adj::Threshold { t } => [if lum(c) >= t { 1.0 } else { 0.0 }; 3],
            Adj::Posterize { levels } => {
                let n = levels.round().max(2.0) - 1.0;
                c.map(|x| (x * n).round() / n)
            }
            Adj::Grain { .. } => c,
            Adj::Clarity { amount } => {
                // Courbe en S centrée sur la luminance, pesée vers les tons moyens
                let l = lum(c);
                let w = 4.0 * l * (1.0 - l);
                let s = (l - 0.5) * amount * w;
                c.map(|x| (x + s).clamp(0.0, 1.0))
            }
            Adj::Dehaze { amount } => {
                // Modèle de brume : c = j·(1 - a) + a (a : épaisseur du voile clair)
                let a = amount.clamp(-0.9, 0.9);
                if a >= 0.0 {
                    c.map(|x| ((x - a) / (1.0 - a)).clamp(0.0, 1.0))
                } else {
                    c.map(|x| (x * (1.0 + a) - a).clamp(0.0, 1.0))
                }
            }
            Adj::Vibrance { amount } => {
                let mut hsv = egui::ecolor::Hsva::from_rgb(c);
                hsv.s = (hsv.s + amount * (1.0 - hsv.s) * hsv.s.max(0.15)).clamp(0.0, 1.0);
                if amount < 0.0 {
                    hsv.s = (hsv.s * (1.0 + amount)).max(0.0);
                }
                hsv.to_rgb()
            }
        }
    }

    /// Comme `apply`, pour un pixel précis du document (le grain dépend de la position)
    pub fn apply_at(&self, c: [f32; 3], x: i32, y: i32) -> [f32; 3] {
        match *self {
            Adj::Grain { amount, size } => {
                let s = size.max(0.5);
                let n = grain_noise((x as f32 / s) as i32, (y as f32 / s) as i32) - 0.5;
                // Plus visible dans les tons moyens, comme un film
                let l = lum(c);
                let w = 0.35 + 2.6 * l * (1.0 - l);
                c.map(|v| (v + n * amount * w).clamp(0.0, 1.0))
            }
            _ => self.apply(c),
        }
    }
}

/// Styles de calque (comme Photoshop) : ombre portée, lueur externe, incrustation couleur, contour
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Fx {
    #[serde(default)]
    pub on: bool,
    pub shadow: Option<FxShadow>,
    pub glow: Option<FxGlow>,
    pub overlay: Option<FxOverlay>,
    pub stroke: Option<FxStroke>,
    /// Ombre interne (mêmes réglages que l'ombre portée)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner: Option<FxShadow>,
    /// Biseau et estampage : taille, profondeur, angle de la lumière (degrés)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bevel: Option<FxBevel>,
    /// Incrustation de dégradé : de `a` à `b`, angle (degrés)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<FxGradient>,
    /// Fusion conditionnelle (Blend If) : luminance de dessous où le calque se montre [de, à]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend_if: Option<[f32; 2]>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct FxShadow {
    pub dx: f32,
    pub dy: f32,
    pub blur: f32,
    pub color: [u8; 3],
    pub opacity: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct FxBevel {
    pub size: f32,
    pub depth: f32,
    pub angle: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct FxGradient {
    pub a: [u8; 3],
    pub b: [u8; 3],
    pub angle: f32,
    pub opacity: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct FxGlow {
    pub size: f32,
    pub color: [u8; 3],
    pub opacity: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct FxOverlay {
    pub color: [u8; 3],
    pub opacity: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct FxStroke {
    pub size: f32,
    pub color: [u8; 3],
    pub opacity: f32,
}

impl Fx {
    fn is_plain(&self) -> bool {
        *self == Fx::default()
    }

    pub fn active(&self) -> bool {
        self.on && (self.shadow.is_some() || self.glow.is_some() || self.overlay.is_some() || self.stroke.is_some() || self.inner.is_some() || self.bevel.is_some() || self.gradient.is_some())
    }

    /// Ce qui demande les pixels voisins (à précalculer)
    fn needs_maps(&self) -> bool {
        self.on && (self.shadow.is_some() || self.glow.is_some() || self.stroke.is_some() || self.inner.is_some() || self.bevel.is_some())
    }
}

/// Couvertures précalculées des effets, à la taille du document (0..255)
#[derive(Default)]
pub struct FxMaps {
    w: usize,
    shadow: Vec<u8>,
    glow: Vec<u8>,
    stroke: Vec<u8>,
    inner: Vec<u8>,
    /// Biseau : éclairage (128 = neutre, plus clair au-dessus, plus sombre en dessous)
    bevel: Vec<u8>,
}

impl FxMaps {
    fn get(v: &[u8], w: usize, x: i32, y: i32) -> f32 {
        if v.is_empty() || x < 0 || y < 0 || x as usize >= w {
            return 0.0;
        }
        v.get(y as usize * w + x as usize).map(|a| *a as f32 / 255.0).unwrap_or(0.0)
    }
}

/// Maximum glissant (rayon r) en deux passes : épaissit une couverture
fn dilate(a: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    if r == 0 {
        return a.to_vec();
    }
    let mut t = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let (x0, x1) = (x.saturating_sub(r), (x + r).min(w - 1));
            t[y * w + x] = a[y * w + x0..=y * w + x1].iter().cloned().fold(0.0, f32::max);
        }
    }
    let mut o = vec![0.0f32; w * h];
    for x in 0..w {
        for y in 0..h {
            let (y0, y1) = (y.saturating_sub(r), (y + r).min(h - 1));
            o[y * w + x] = (y0..=y1).map(|k| t[k * w + x]).fold(0.0, f32::max);
        }
    }
    o
}

/// Flou (trois passes de moyenne glissante, proche d'un flou gaussien)
fn blur(a: &[f32], w: usize, h: usize, radius: f32) -> Vec<f32> {
    let r = (radius / 3.0).round() as usize;
    if r == 0 {
        return a.to_vec();
    }
    let mut cur = a.to_vec();
    for _ in 0..3 {
        let mut t = vec![0.0f32; w * h];
        for y in 0..h {
            let row = &cur[y * w..(y + 1) * w];
            let mut acc = 0.0f32;
            let n = (2 * r + 1) as f32;
            for k in 0..=r.min(w - 1) {
                acc += row[k];
            }
            acc += row[0] * r as f32;
            for x in 0..w {
                t[y * w + x] = acc / n;
                let add = row[(x + r + 1).min(w - 1)];
                let sub = row[x.saturating_sub(r)];
                acc += add - sub;
            }
        }
        let mut o = vec![0.0f32; w * h];
        for x in 0..w {
            let mut acc = 0.0f32;
            let n = (2 * r + 1) as f32;
            for k in 0..=r.min(h - 1) {
                acc += t[k * w + x];
            }
            acc += t[x] * r as f32;
            for y in 0..h {
                o[y * w + x] = acc / n;
                acc += t[(y + r + 1).min(h - 1) * w + x] - t[y.saturating_sub(r) * w + x];
            }
        }
        cur = o;
    }
    cur
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
    /// Masque de fusion : un calque gris (blanc montre, noir cache), à la taille du document
    pub mask: Option<Box<Layer>>,
    /// Masque actif (Maj+clic sur sa vignette le désactive : croix rouge)
    pub mask_on: bool,
    /// Fond (comme dans Photoshop : l'opacité du contenu, sans les effets)
    pub fill: f32,
    /// Verrou de la transparence : on ne peint que là où il y a déjà des pixels
    pub lock_alpha: bool,
    /// Verrou complet : ni peinture ni déplacement
    pub lock_all: bool,
    /// Masque d'écrêtage : le calque n'apparaît que sur les pixels du calque de dessous
    pub clip: bool,
    /// Calque de réglage (sans pixels)
    pub adj: Option<Adj>,
    /// Groupe du calque (0 : aucun)
    pub group: u32,
    /// Styles de calque, et leurs couvertures précalculées
    pub fx: Fx,
    pub fx_maps: Option<std::sync::Arc<FxMaps>>,
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
    #[serde(default)]
    mask: bool,
    #[serde(default = "yes")]
    mask_on: bool,
    #[serde(default = "one")]
    fill: f32,
    #[serde(default)]
    lock_alpha: bool,
    #[serde(default)]
    lock_all: bool,
    #[serde(default)]
    clip: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    adj: Option<Adj>,
    #[serde(default)]
    group: u32,
    #[serde(default, skip_serializing_if = "Fx::is_plain")]
    fx: Fx,
}

fn yes() -> bool {
    true
}

fn one() -> f32 {
    1.0
}

#[derive(Serialize, Deserialize)]
struct Head {
    version: u32,
    w: usize,
    h: usize,
    active: usize,
    layers: Vec<LayerHead>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    groups: Vec<Group>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    guides: Vec<Guide>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    anim: Option<Anim>,
}

impl Layer {
    pub fn blank(name: &str, w: usize, h: usize) -> Layer {
        Layer { name: name.to_string(), visible: true, opacity: 1.0, blend: Blend::Normal, off: [0, 0], w, h, px: vec![0; w * h * 4], mask: None, mask_on: true, fill: 1.0, lock_alpha: false, lock_all: false, clip: false, adj: None, group: 0, fx: Fx::default(), fx_maps: None }
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

    /// Ce que le masque laisse voir aux coordonnées du document (1 sans masque ; blanc hors du masque)
    pub fn mask_at(&self, x: i32, y: i32) -> f32 {
        match (&self.mask, self.mask_on) {
            (Some(m), true) => {
                let (lx, ly) = (x - m.off[0], y - m.off[1]);
                if lx < 0 || ly < 0 || lx as usize >= m.w || ly as usize >= m.h {
                    return 1.0;
                }
                m.px[(ly as usize * m.w + lx as usize) * 4] as f32 / 255.0
            }
            _ => 1.0,
        }
    }

    /// Précalcule les couvertures des styles (ombre, lueur, contour) pour un document w × h
    pub fn bake_fx(&mut self, w: usize, h: usize) {
        if !self.fx.needs_maps() || w == 0 || h == 0 {
            self.fx_maps = None;
            return;
        }
        let mut a = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                a[y * w + x] = self.at(x as i32, y as i32)[3] as f32 / 255.0 * self.mask_at(x as i32, y as i32);
            }
        }
        let q = |v: Vec<f32>| v.into_iter().map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8).collect::<Vec<u8>>();
        let mut m = FxMaps { w, ..Default::default() };
        if let Some(sh) = self.fx.shadow {
            // Décalée puis floue
            let (dx, dy) = (sh.dx.round() as i32, sh.dy.round() as i32);
            let mut moved = vec![0.0f32; w * h];
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let (sx, sy) = (x - dx, y - dy);
                    if sx >= 0 && sy >= 0 && (sx as usize) < w && (sy as usize) < h {
                        moved[y as usize * w + x as usize] = a[sy as usize * w + sx as usize];
                    }
                }
            }
            m.shadow = q(blur(&moved, w, h, sh.blur));
        }
        if let Some(g) = self.fx.glow {
            m.glow = q(blur(&dilate(&a, w, h, (g.size * 0.3).round() as usize), w, h, g.size));
        }
        if let Some(st) = self.fx.stroke {
            // Contour extérieur : ce que l'épaississement ajoute autour
            let d = dilate(&a, w, h, st.size.round().max(1.0) as usize);
            m.stroke = q(d.iter().zip(a.iter()).map(|(d, a)| (d - a).max(0.0)).collect());
        }
        if let Some(sh) = self.fx.inner {
            // L'intérieur que le bord décalé et flou ne couvre pas
            let (dx, dy) = (sh.dx.round() as i32, sh.dy.round() as i32);
            let mut moved = vec![0.0f32; w * h];
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let (sx, sy) = (x - dx, y - dy);
                    moved[y as usize * w + x as usize] = if sx >= 0 && sy >= 0 && (sx as usize) < w && (sy as usize) < h { a[sy as usize * w + sx as usize] } else { 0.0 };
                }
            }
            let b = blur(&moved, w, h, sh.blur);
            m.inner = q(a.iter().zip(b.iter()).map(|(a, b)| a * (1.0 - b)).collect());
        }
        if let Some(bv) = self.fx.bevel {
            // Relief : la couverture floue sert de hauteur, éclairée selon l'angle
            let hgt = blur(&a, w, h, bv.size.max(1.0));
            let (lx, ly) = (bv.angle.to_radians().cos(), -bv.angle.to_radians().sin());
            let mut out = vec![0.5f32; w * h];
            for y in 1..h.saturating_sub(1) {
                for x in 1..w.saturating_sub(1) {
                    let gx = hgt[y * w + x + 1] - hgt[y * w + x - 1];
                    let gy = hgt[(y + 1) * w + x] - hgt[(y - 1) * w + x];
                    // Pente tournée vers la lumière : plus clair
                    let shade = -(gx * lx + gy * ly) * bv.depth * bv.size.max(1.0) * 1.5;
                    out[y * w + x] = 0.5 + shade.clamp(-0.5, 0.5) * a[y * w + x];
                }
            }
            m.bevel = q(out);
        }
        self.fx_maps = Some(std::sync::Arc::new(m));
    }

    /// Un masque blanc (tout montrer) ou noir, à la taille du document
    pub fn new_mask(w: usize, h: usize, v: u8) -> Box<Layer> {
        Box::new(Layer::filled("masque", w, h, [v, v, v, 255]))
    }
}

/// Pose une couleur `sc` de couverture `a` sur `d` avec le mode `blend`
fn over(d: &mut [f32; 4], sc: [f32; 3], a: f32, blend: Blend) {
    if a <= 0.0 {
        return;
    }
    // Sur du vide, le mode de fusion n'a rien à mélanger : on garde la couleur du calque
    let c = if d[3] > 0.0 { blend.mix([d[0], d[1], d[2]], sc) } else { sc };
    let c = [0, 1, 2].map(|k| sc[k] + (c[k] - sc[k]) * d[3]);
    let oa = a + d[3] * (1.0 - a);
    for k in 0..3 {
        d[k] = if oa > 0.0 { (c[k] * a + d[k] * d[3] * (1.0 - a)) / oa } else { 0.0 };
    }
    d[3] = oa;
}

/// Mélange une suite de calques sur `d` (un pixel), `mul` dose leur couverture (opacité d'un groupe)
fn stack(d: &mut [f32; 4], layers: &[&Layer], x: i32, y: i32, mul: f32) {
    // Couverture du dernier calque de base (pour les calques écrêtés posés dessus)
    let mut base_a = 0.0f32;
    let mut base_on = false;
    for l in layers {
        if let Some(adj) = &l.adj {
            // Réglage : la couleur de dessous change, dosée par le masque, l'opacité et le fond
            if !l.visible || d[3] <= 0.0 || (l.clip && !base_on) {
                continue;
            }
            let k = l.mask_at(x, y) * l.opacity * l.fill * mul * if l.clip { base_a } else { 1.0 };
            if k <= 0.0 {
                continue;
            }
            let below = [d[0], d[1], d[2]];
            let c = l.blend.mix(below, adj.apply_at(below, x, y));
            for ch in 0..3 {
                d[ch] = below[ch] + (c[ch] - below[ch]) * k;
            }
            continue;
        }
        let s = l.at(x, y);
        let mut a = s[3] as f32 / 255.0 * l.mask_at(x, y);
        if l.clip {
            if !base_on {
                continue;
            }
            a *= base_a;
        } else {
            base_a = if l.visible { a } else { 0.0 };
            base_on = l.visible;
        }
        if !l.visible {
            continue;
        }
        let mut sc = [s[0] as f32 / 255.0, s[1] as f32 / 255.0, s[2] as f32 / 255.0];
        // Fusion conditionnelle : le calque ne se montre que sur les tons de dessous choisis
        if let Some([lo, hi]) = l.fx.blend_if {
            if d[3] > 0.0 {
                a *= blend_if_k(lo, hi, lum([d[0], d[1], d[2]]));
            }
        }
        if l.fx.active() {
            let maps = l.fx_maps.as_deref();
            let col = |c: [u8; 3]| c.map(|v| v as f32 / 255.0);
            let op = l.opacity * mul;
            if let (Some(sh), Some(m)) = (l.fx.shadow, maps) {
                over(d, col(sh.color), FxMaps::get(&m.shadow, m.w, x, y) * sh.opacity * op, Blend::Multiply);
            }
            if let (Some(g), Some(m)) = (l.fx.glow, maps) {
                over(d, col(g.color), FxMaps::get(&m.glow, m.w, x, y) * g.opacity * op, Blend::Screen);
            }
            if let Some(g) = l.fx.gradient {
                // Le long de l'angle, sur le cadre du calque
                let (cx, cy) = (l.off[0] as f32 + l.w as f32 / 2.0, l.off[1] as f32 + l.h as f32 / 2.0);
                let (ux, uy) = (g.angle.to_radians().cos(), -g.angle.to_radians().sin());
                let half = (l.w as f32 * ux.abs() + l.h as f32 * uy.abs()) / 2.0;
                let t = (((x as f32 - cx) * ux + (y as f32 - cy) * uy) / half.max(1.0) * 0.5 + 0.5).clamp(0.0, 1.0);
                let (ca, cb) = (col(g.a), col(g.b));
                let gc = [0, 1, 2].map(|k| ca[k] + (cb[k] - ca[k]) * t);
                sc = [0, 1, 2].map(|k| sc[k] + (gc[k] - sc[k]) * g.opacity);
            }
            if let Some(o) = l.fx.overlay {
                let oc = col(o.color);
                sc = [0, 1, 2].map(|k| sc[k] + (oc[k] - sc[k]) * o.opacity);
            }
            if let (Some(sh), Some(m)) = (l.fx.inner, maps) {
                let k = FxMaps::get(&m.inner, m.w, x, y) * sh.opacity;
                let c = col(sh.color);
                sc = [0, 1, 2].map(|i| sc[i] * (1.0 - k) + sc[i] * c[i] * k);
            }
            if let (Some(_), Some(m)) = (l.fx.bevel, maps) {
                if !m.bevel.is_empty() {
                    let v = FxMaps::get(&m.bevel, m.w, x, y) - 0.5;
                    sc = if v >= 0.0 { sc.map(|c| c + (1.0 - c) * v * 2.0) } else { sc.map(|c| c * (1.0 + v * 2.0)) };
                }
            }
            over(d, sc, a * l.opacity * l.fill * mul, l.blend);
            if let (Some(st), Some(m)) = (l.fx.stroke, maps) {
                over(d, col(st.color), FxMaps::get(&m.stroke, m.w, x, y) * st.opacity * op, Blend::Normal);
            }
            continue;
        }
        over(d, sc, a * l.opacity * l.fill * mul, l.blend);
    }
}

/// Un état de l'historique : le document entier sauf les pixels partagés, et son nom
#[derive(Clone)]
struct Snap {
    layers: Vec<Layer>,
    active: usize,
    groups: Vec<Group>,
    w: usize,
    h: usize,
    label: String,
}

/// Formats proposés au recadrage (0 : libre, 1 : celui de l'image)
const CROP_RATIOS: [(&str, f32); 7] = [("libre", 0.0), ("format de l'image", -1.0), ("1:1", 1.0), ("4:3", 4.0 / 3.0), ("16:9", 16.0 / 9.0), ("9:16", 9.0 / 16.0), ("4:5", 0.8)];

#[derive(Clone)]
pub struct Doc {
    pub w: usize,
    pub h: usize,
    pub layers: Vec<Layer>,
    pub active: usize,
    /// Groupes de calques (dossiers) : les calques d'un groupe se suivent dans la pile
    pub groups: Vec<Group>,
    /// Repères tirés des règles
    pub guides: Vec<Guide>,
    /// Animation image par image : les calques d'un groupe sont les images
    pub anim: Option<Anim>,
}

/// Réglages de l'animation : le groupe des images, la cadence, la pelure d'oignon
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Anim {
    pub group: u32,
    pub fps: f32,
    #[serde(default = "yes")]
    pub onion: bool,
}

/// Un repère : une ligne verticale (à x) ou horizontale (à y), en pixels du document
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct Guide {
    pub vertical: bool,
    pub at: f32,
}

/// Un groupe de calques, comme un dossier de Photoshop
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Group {
    pub id: u32,
    pub name: String,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default = "one")]
    pub opacity: f32,
    /// None : « Traverser » (les calques du groupe se mélangent directement avec le dessous)
    #[serde(default)]
    pub blend: Option<Blend>,
    #[serde(default)]
    pub collapsed: bool,
    /// Groupe parent (0 : à la racine) : les groupes s'emboîtent comme les dossiers de Photoshop
    #[serde(default, skip_serializing_if = "is_zero")]
    pub parent: u32,
    /// Masque du groupe (à la taille du document) ; dans le fichier, son PNG suit ceux des calques
    #[serde(skip)]
    pub mask: GroupMask,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_mask: bool,
    #[serde(default = "yes")]
    pub mask_on: bool,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// Le masque d'un groupe (une enveloppe : comparée et affichée sans ses pixels)
#[derive(Clone, Default)]
pub struct GroupMask(pub Option<Box<Layer>>);

impl PartialEq for GroupMask {
    fn eq(&self, o: &Self) -> bool {
        match (&self.0, &o.0) {
            (None, None) => true,
            (Some(a), Some(b)) => a.px == b.px,
            _ => false,
        }
    }
}

impl std::fmt::Debug for GroupMask {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", if self.0.is_some() { "masque" } else { "-" })
    }
}

impl Doc {
    /// Les calques-images de l'animation, de la première à la dernière (de bas en haut).
    pub fn frames(&self) -> Vec<usize> {
        match &self.anim {
            Some(a) => (0..self.layers.len()).filter(|&i| self.layers[i].group == a.group).collect(),
            None => vec![],
        }
    }
}

impl Group {
    pub fn new(id: u32, name: String, parent: u32) -> Group {
        Group { id, name, visible: true, opacity: 1.0, blend: None, collapsed: false, parent, mask: GroupMask(None), has_mask: false, mask_on: true }
    }
}

impl Doc {
    /// Les groupes d'un calque, du plus extérieur au plus intérieur
    pub fn chain(&self, gid: u32) -> Vec<u32> {
        let mut v = vec![];
        let mut g = gid;
        while g != 0 && v.len() < 64 && !v.contains(&g) {
            v.push(g);
            g = self.groups.iter().find(|x| x.id == g).map(|x| x.parent).unwrap_or(0);
        }
        v.reverse();
        v
    }

    /// `g` est-il `anc` ou l'un de ses sous-groupes ?
    #[cfg(test)]
    pub fn within(&self, g: u32, anc: u32) -> bool {
        anc == 0 || self.chain(g).contains(&anc)
    }

    /// Mélange les calques `idx` (tous dans le groupe `level` ou ses sous-groupes) sur le pixel `d`
    fn comp_level(&self, d: &mut [f32; 4], idx: &[usize], chains: &[Vec<u32>], level: usize, x: i32, y: i32, mul: f32) {
        let mut i = 0;
        while i < idx.len() {
            // Le groupe enfant de ce niveau où se trouve ce calque (None : calque direct)
            let child = |k: usize| chains[idx[k]].get(level).copied();
            let c = child(i);
            let mut j = i + 1;
            while j < idx.len() && child(j) == c {
                j += 1;
            }
            match c.and_then(|g| self.groups.iter().find(|x| x.id == g)) {
                None => {
                    let run: Vec<&Layer> = idx[i..j].iter().map(|&k| &self.layers[k]).collect();
                    stack(d, &run, x, y, mul);
                }
                Some(g) if !g.visible => {}
                Some(g) => {
                    let m = match (&g.mask.0, g.mask_on) {
                        (Some(mk), true) => mk.at(x, y)[0] as f32 / 255.0,
                        _ => 1.0,
                    };
                    match g.blend {
                        // Traverser : ses calques se mélangent avec le dessous, dosés par l'opacité et le masque
                        None => self.comp_level(d, &idx[i..j], chains, level + 1, x, y, mul * g.opacity * m),
                        // Isolé : mélangé à part, puis posé comme un seul calque
                        Some(b) => {
                            let mut t = [0.0f32; 4];
                            self.comp_level(&mut t, &idx[i..j], chains, level + 1, x, y, 1.0);
                            over(d, [t[0], t[1], t[2]], t[3] * g.opacity * m * mul, b);
                        }
                    }
                }
            }
            i = j;
        }
    }

    pub fn new(w: usize, h: usize, white: bool) -> Doc {
        let base = if white { Layer::filled("fond", w, h, [255, 255, 255, 255]) } else { Layer::blank("calque 1", w, h) };
        Doc { w, h, layers: vec![base], active: 0, groups: vec![], guides: vec![], anim: None }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        // Un .psd reste un .psd : enregistré au format Photoshop
        if is_psd(path) {
            let (bytes, _) = crate::psd::write(self);
            let tmp = path.with_extension("psd.tmp");
            crate::vault::atomic_write(&tmp, bytes).map_err(|e| e.to_string())?;
            return std::fs::rename(&tmp, path).map_err(|e| e.to_string());
        }
        let head = Head {
            version: 1,
            w: self.w,
            h: self.h,
            active: self.active,
            layers: self.layers.iter().map(|l| LayerHead { name: l.name.clone(), visible: l.visible, opacity: l.opacity, blend: l.blend, off: l.off, w: l.w, h: l.h, mask: l.mask.is_some(), mask_on: l.mask_on, fill: l.fill, lock_alpha: l.lock_alpha, lock_all: l.lock_all, clip: l.clip, adj: l.adj, group: l.group, fx: l.fx.clone() }).collect(),
            groups: self.groups.iter().map(|g| Group { has_mask: g.mask.0.is_some(), mask: GroupMask(None), ..g.clone() }).collect(),
            guides: self.guides.clone(),
            anim: self.anim.clone(),
        };
        let json = serde_json::to_vec(&head).map_err(|e| e.to_string())?;
        let mut out = b"ARYIMG1\n".to_vec();
        out.extend((json.len() as u32).to_le_bytes());
        out.extend(json);
        // Chaque calque en PNG, suivi de son masque s'il en a un
        for l in &self.layers {
            for part in std::iter::once(l).chain(l.mask.as_deref()) {
                let mut png = vec![];
                let img = image::RgbaImage::from_raw(part.w as u32, part.h as u32, part.px.clone()).ok_or("calque invalide")?;
                img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| e.to_string())?;
                out.extend((png.len() as u32).to_le_bytes());
                out.extend(png);
            }
        }
        // Puis les masques des groupes, dans l'ordre des groupes
        for m in self.groups.iter().filter_map(|g| g.mask.0.as_deref()) {
            let mut png = vec![];
            let img = image::RgbaImage::from_raw(m.w as u32, m.h as u32, m.px.clone()).ok_or("masque invalide")?;
            img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| e.to_string())?;
            out.extend((png.len() as u32).to_le_bytes());
            out.extend(png);
        }
        // Écrit à côté puis remplace : un plantage pendant l'écriture ne casse pas le fichier
        let tmp = path.with_extension("aryimg.tmp");
        crate::vault::atomic_write(&tmp, out).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    pub fn load(path: &Path) -> Result<Doc, String> {
        let b = std::fs::read(path).map_err(|e| e.to_string())?;
        if is_psd(path) && b.starts_with(b"8BPS") {
            return crate::psd::read(&b);
        }
        if b.is_empty() {
            return Ok(Doc::new(1920, 1080, true));
        }
        if !b.starts_with(b"ARYIMG1\n") {
            // Une image ordinaire renommée : un seul calque
            let img = image::load_from_memory(&b).map_err(|_| "fichier .aryimg illisible".to_string())?.to_rgba8();
            let (w, h) = (img.width() as usize, img.height() as usize);
            return Ok(Doc { w, h, layers: vec![Layer { px: img.into_raw(), ..Layer::blank("calque 1", w, h) }], active: 0, groups: vec![], guides: vec![], anim: None });
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
        let png = |at: &mut usize| -> Result<(usize, usize, Vec<u8>), String> {
            let n = rd(at)?;
            let img = image::load_from_memory(b.get(*at..*at + n).ok_or("fichier coupé")?).map_err(|e| e.to_string())?.to_rgba8();
            *at += n;
            Ok((img.width() as usize, img.height() as usize, img.into_raw()))
        };
        for lh in head.layers {
            let (w, h, px) = png(&mut at)?;
            let mask = if lh.mask {
                let (mw, mh, mpx) = png(&mut at)?;
                Some(Box::new(Layer { px: mpx, ..Layer::blank("masque", mw, mh) }))
            } else {
                None
            };
            layers.push(Layer { name: lh.name, visible: lh.visible, opacity: lh.opacity, blend: lh.blend, off: lh.off, w, h, px, mask, mask_on: lh.mask_on, fill: lh.fill, lock_alpha: lh.lock_alpha, lock_all: lh.lock_all, clip: lh.clip, adj: lh.adj, group: lh.group, fx: lh.fx, fx_maps: None });
        }
        if layers.is_empty() {
            layers.push(Layer::blank("calque 1", head.w, head.h));
        }
        let mut groups = head.groups;
        for g in groups.iter_mut().filter(|g| g.has_mask) {
            if let Ok((mw, mh, mpx)) = png(&mut at) {
                g.mask = GroupMask(Some(Box::new(Layer { px: mpx, ..Layer::blank("masque", mw, mh) })));
            }
        }
        Ok(Doc { w: head.w, h: head.h, active: head.active.min(layers.len() - 1), layers, groups, guides: head.guides, anim: head.anim })
    }

    /// Mélange les calques visibles dans `out` (RGBA du document) pour la zone `r` (x0, y0, x1, y1) :
    /// opacité × fond × masque, masques d'écrêtage, calques de réglage, groupes, modes de Photoshop.
    pub fn composite(&self, out: &mut [u8], r: [usize; 4]) {
        let [x0, y0, x1, y1] = r;
        let chains: Vec<Vec<u32>> = self.layers.iter().map(|l| self.chain(l.group)).collect();
        let idx: Vec<usize> = (0..self.layers.len()).collect();
        for y in y0..y1.min(self.h) {
            for x in x0..x1.min(self.w) {
                let mut d = [0.0f32; 4];
                self.comp_level(&mut d, &idx, &chains, 0, x as i32, y as i32, 1.0);
                let i = (y * self.w + x) * 4;
                for k in 0..4 {
                    out[i + k] = (d[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
        }
    }

    pub fn flatten(&self) -> image::RgbaImage {
        let mut out = vec![0u8; self.w * self.h * 4];
        // Les styles ont besoin de leurs couvertures : calculées ici si elles manquent
        if self.layers.iter().any(|l| l.fx.needs_maps() && l.fx_maps.is_none()) {
            let mut d = self.clone();
            for l in &mut d.layers {
                if l.fx.needs_maps() && l.fx_maps.is_none() {
                    l.bake_fx(d.w, d.h);
                }
            }
            d.composite(&mut out, [0, 0, self.w, self.h]);
        } else {
            self.composite(&mut out, [0, 0, self.w, self.h]);
        }
        image::RgbaImage::from_raw(self.w as u32, self.h as u32, out).unwrap_or_default()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Move,
    Select,
    Ellipse,
    Lasso,
    Wand,
    /// Recadrage : le cadre se règle à la souris, Entrée l'applique
    Crop,
    Brush,
    /// Tampon de duplication : Alt+clic choisit la source, on peint ce qu'il y a là-bas
    Clone,
    /// Correcteur localisé : la zone peinte est reconstruite à partir de ses bords
    Heal,
    /// Doigt : étale la peinture dans le sens du trait
    Smudge,
    /// Fluidité : pousse les pixels dans le sens du trait (déformation)
    Liquify,
    Eraser,
    Fill,
    Gradient,
    Text,
    Picker,
}

/// Pinceaux prêts : (nom, diamètre, dureté, opacité, grain, pression → diamètre, pression → opacité, lissage)
const BRUSH_PRESETS: [(&str, f32, f32, f32, f32, bool, bool, f32); 7] = [
    ("crayon", 4.0, 0.9, 0.85, 0.55, true, true, 0.1),
    ("encre", 6.0, 0.95, 1.0, 0.0, true, false, 0.35),
    ("feutre", 14.0, 0.85, 0.9, 0.0, false, false, 0.1),
    ("craie", 28.0, 0.6, 0.9, 0.8, true, false, 0.0),
    ("aérographe", 90.0, 0.0, 0.25, 0.0, false, true, 0.0),
    ("peinture sèche", 40.0, 0.75, 0.8, 0.45, true, true, 0.0),
    ("pinceau rond", 22.0, 0.7, 1.0, 0.0, false, false, 0.0),
];

/// Texture du papier : un bruit de valeur fixe (entre 0 et 1), à grains de deux tailles.
/// Part d'un calque en fusion conditionnelle : 1 entre `lo` et `hi`, fondu doux de part et d'autre
pub fn blend_if_k(lo: f32, hi: f32, l: f32) -> f32 {
    const SOFT: f32 = 0.08;
    let a = if lo <= 0.0 { 1.0 } else { ((l - lo + SOFT) / SOFT).clamp(0.0, 1.0) };
    let b = if hi >= 1.0 { 1.0 } else { ((hi + SOFT - l) / SOFT).clamp(0.0, 1.0) };
    a * b
}

/// Remplissage qui ne passe pas par les trous plus petits que `gap` : la zone `open` (où l'on peut peindre)
/// est rétrécie de `gap`, remplie depuis la graine, puis regagne `gap` + 1 pixels sans sortir de `open`.
/// None si la graine tombe dans le trait épaissi (on remplit alors normalement).
pub fn gap_fill(open: &[bool], w: usize, h: usize, sx: usize, sy: usize, gap: usize) -> Option<Vec<bool>> {
    // Distance au premier pixel fermé (carrés successifs : passes de dilatation en croix et diagonales)
    let mut blocked: Vec<bool> = open.iter().map(|o| !o).collect();
    for _ in 0..gap {
        let prev = blocked.clone();
        for y in 0..h {
            for x in 0..w {
                if prev[y * w + x] {
                    continue;
                }
                let n = (x > 0 && prev[y * w + x - 1]) || (x + 1 < w && prev[y * w + x + 1]) || (y > 0 && prev[(y - 1) * w + x]) || (y + 1 < h && prev[(y + 1) * w + x]);
                if n {
                    blocked[y * w + x] = true;
                }
            }
        }
    }
    if blocked[sy * w + sx] {
        return None;
    }
    let mut region = vec![false; w * h];
    let mut stack = vec![(sx, sy)];
    while let Some((x, y)) = stack.pop() {
        let i = y * w + x;
        if region[i] || blocked[i] {
            continue;
        }
        region[i] = true;
        if x > 0 {
            stack.push((x - 1, y));
        }
        if x + 1 < w {
            stack.push((x + 1, y));
        }
        if y > 0 {
            stack.push((x, y - 1));
        }
        if y + 1 < h {
            stack.push((x, y + 1));
        }
    }
    // La zone regagne le terrain perdu, jusqu'au trait
    for _ in 0..=gap {
        let prev = region.clone();
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if prev[i] || !open[i] {
                    continue;
                }
                if (x > 0 && prev[i - 1]) || (x + 1 < w && prev[i + 1]) || (y > 0 && prev[i - w]) || (y + 1 < h && prev[i + w]) {
                    region[i] = true;
                }
            }
        }
    }
    Some(region)
}

/// Bruit blanc fixe (grain de film)
fn grain_noise(x: i32, y: i32) -> f32 {
    let mut n = (x as u32).wrapping_mul(2246822519) ^ (y as u32).wrapping_mul(3266489917) ^ 0x9e37_79b9;
    n = (n ^ (n >> 15)).wrapping_mul(2654435761);
    n ^= n >> 13;
    (n & 0xffff) as f32 / 65535.0
}

fn paper(x: i32, y: i32) -> f32 {
    fn h(x: i32, y: i32) -> f32 {
        let mut n = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263);
        n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        ((n ^ (n >> 16)) & 0xffff) as f32 / 65535.0
    }
    let v = |x: i32, y: i32, s: i32| {
        let (fx, fy) = (x.div_euclid(s), y.div_euclid(s));
        let (tx, ty) = ((x.rem_euclid(s)) as f32 / s as f32, (y.rem_euclid(s)) as f32 / s as f32);
        let a = h(fx, fy) + (h(fx + 1, fy) - h(fx, fy)) * tx;
        let b = h(fx, fy + 1) + (h(fx + 1, fy + 1) - h(fx, fy + 1)) * tx;
        a + (b - a) * ty
    };
    let n = v(x, y, 2) * 0.6 + v(x, y, 5) * 0.4;
    // Plus de contraste : des creux francs
    ((n - 0.35) * 1.8).clamp(0.0, 1.0)
}

const TOOLS: [(Tool, &str, &str, &str); 16] = [
    (Tool::Move, crate::lucide::MOVE, "déplacer le calque", "V"),
    (Tool::Select, crate::lucide::SQUARE_DASHED, "sélection rectangulaire", "M"),
    (Tool::Ellipse, crate::lucide::CIRCLE_DASHED, "sélection elliptique", "O"),
    (Tool::Lasso, crate::lucide::LASSO, "lasso", "L"),
    (Tool::Wand, crate::lucide::WAND_SPARKLES, "baguette magique", "W"),
    (Tool::Crop, crate::lucide::CROP, "recadrage", "C"),
    (Tool::Brush, crate::lucide::PAINTBRUSH, "pinceau", "B"),
    (Tool::Clone, crate::lucide::STAMP, "tampon de duplication (Alt+clic : source)", "S"),
    (Tool::Heal, crate::lucide::BANDAGE, "correcteur localisé : peindre sur une tache", "J"),
    (Tool::Smudge, crate::lucide::POINTER, "doigt : étaler la peinture (le flux règle la force)", "R"),
    (Tool::Liquify, crate::lucide::WAVES, "fluidité : pousser les pixels (déformer)", "U"),
    (Tool::Eraser, crate::lucide::ERASER, "gomme", "E"),
    (Tool::Fill, crate::lucide::PAINT_BUCKET, "pot de peinture", "G"),
    (Tool::Gradient, crate::lucide::BLEND, "dégradé", "G"),
    (Tool::Text, crate::lucide::TYPE, "texte", "T"),
    (Tool::Picker, crate::lucide::PIPETTE, "pipette", "I"),
];

/// Une sélection : un masque (0 à 255 par pixel du document), son cadre, et son contour à dessiner
#[derive(Clone)]
pub struct Sel {
    pub mask: Vec<u8>,
    pub bbox: [i32; 4],
    /// Contour en coordonnées du document (vide pour la baguette : on montre le cadre)
    pub outline: Vec<[f32; 2]>,
}

impl Sel {
    /// Sélection d'après une fonction de couverture (0 à 1) évaluée en 4 points par pixel, dans un cadre
    fn from_fn(w: usize, h: usize, bbox: [i32; 4], cover: impl Fn(f32, f32) -> bool, outline: Vec<[f32; 2]>) -> Option<Sel> {
        let b = [bbox[0].max(0), bbox[1].max(0), bbox[2].min(w as i32), bbox[3].min(h as i32)];
        if b[2] <= b[0] || b[3] <= b[1] {
            return None;
        }
        let mut mask = vec![0u8; w * h];
        for y in b[1]..b[3] {
            for x in b[0]..b[2] {
                let n = [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)].iter().filter(|(dx, dy)| cover(x as f32 + dx, y as f32 + dy)).count();
                mask[y as usize * w + x as usize] = (n * 255 / 4) as u8;
            }
        }
        Some(Sel { mask, bbox: b, outline })
    }

    pub fn rect(w: usize, h: usize, a: [f32; 2], b: [f32; 2]) -> Option<Sel> {
        let (x0, y0, x1, y1) = (a[0].min(b[0]).round(), a[1].min(b[1]).round(), a[0].max(b[0]).round(), a[1].max(b[1]).round());
        Sel::from_fn(w, h, [x0 as i32, y0 as i32, x1 as i32, y1 as i32], |x, y| x >= x0 && x < x1 && y >= y0 && y < y1, vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]])
    }

    pub fn ellipse(w: usize, h: usize, a: [f32; 2], b: [f32; 2]) -> Option<Sel> {
        let (cx, cy) = ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0);
        let (rx, ry) = (((a[0] - b[0]) / 2.0).abs().max(0.5), ((a[1] - b[1]) / 2.0).abs().max(0.5));
        let outline = (0..64).map(|k| {
            let t = k as f32 / 64.0 * std::f32::consts::TAU;
            [cx + rx * t.cos(), cy + ry * t.sin()]
        }).collect();
        Sel::from_fn(w, h, [(cx - rx).floor() as i32, (cy - ry).floor() as i32, (cx + rx).ceil() as i32, (cy + ry).ceil() as i32], |x, y| ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) <= 1.0, outline)
    }

    /// Polygone fermé (lasso), règle pair-impair
    pub fn poly(w: usize, h: usize, pts: &[[f32; 2]]) -> Option<Sel> {
        if pts.len() < 3 {
            return None;
        }
        let bb = pts.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |m, p| [m[0].min(p[0]), m[1].min(p[1]), m[2].max(p[0]), m[3].max(p[1])]);
        let inside = |x: f32, y: f32| {
            let mut c = false;
            let mut j = pts.len() - 1;
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[j]);
                if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
                    c = !c;
                }
                j = i;
            }
            c
        };
        Sel::from_fn(w, h, [bb[0].floor() as i32, bb[1].floor() as i32, bb[2].ceil() as i32, bb[3].ceil() as i32], inside, pts.to_vec())
    }

    /// Baguette magique : les pixels voisins de couleur proche (sur l'image composée)
    pub fn wand(w: usize, h: usize, comp: &[u8], at: [i32; 2], tol: f32) -> Option<Sel> {
        if at[0] < 0 || at[1] < 0 || at[0] as usize >= w || at[1] as usize >= h {
            return None;
        }
        let i0 = (at[1] as usize * w + at[0] as usize) * 4;
        let target = [comp[i0], comp[i0 + 1], comp[i0 + 2], comp[i0 + 3]];
        let near = |i: usize| (0..4).map(|k| (comp[i * 4 + k] as f32 - target[k] as f32).abs() / 255.0).fold(0.0f32, f32::max) <= tol;
        let mut mask = vec![0u8; w * h];
        let mut stack = vec![(at[0] as usize, at[1] as usize)];
        let mut bb = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
        while let Some((x, y)) = stack.pop() {
            let i = y * w + x;
            if mask[i] != 0 || !near(i) {
                continue;
            }
            mask[i] = 255;
            bb = [bb[0].min(x as i32), bb[1].min(y as i32), bb[2].max(x as i32 + 1), bb[3].max(y as i32 + 1)];
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
        Some(Sel { mask, bbox: bb, outline: vec![] })
    }

    pub fn all(w: usize, h: usize) -> Sel {
        Sel { mask: vec![255; w * h], bbox: [0, 0, w as i32, h as i32], outline: vec![[0.0, 0.0], [w as f32, 0.0], [w as f32, h as f32], [0.0, h as f32]] }
    }

    /// Ajoute (Maj) ou retire (Alt) une autre sélection
    pub fn combine(mut self, o: &Sel, subtract: bool) -> Sel {
        for (a, b) in self.mask.iter_mut().zip(o.mask.iter()) {
            *a = if subtract { (*a).min(255 - *b) } else { (*a).max(*b) };
        }
        if !subtract {
            self.bbox = [self.bbox[0].min(o.bbox[0]), self.bbox[1].min(o.bbox[1]), self.bbox[2].max(o.bbox[2]), self.bbox[3].max(o.bbox[3])];
        }
        // Le contour d'une combinaison n'est plus une forme simple : on montre le cadre
        self.outline.clear();
        self
    }

    pub fn invert(&self, w: usize, h: usize) -> Sel {
        Sel { mask: self.mask.iter().map(|v| 255 - v).collect(), bbox: [0, 0, w as i32, h as i32], outline: vec![] }
    }

    pub fn at(&self, w: usize, x: i32, y: i32) -> f32 {
        if x < self.bbox[0] || y < self.bbox[1] || x >= self.bbox[2] || y >= self.bbox[3] {
            return 0.0;
        }
        self.mask.get(y as usize * w + x as usize).map(|v| *v as f32 / 255.0).unwrap_or(0.0)
    }
}

/// Réglage en cours de prévisualisation : on part toujours du calque d'avant.
#[derive(Clone, Copy, PartialEq)]
enum Adjust {
    BrightContrast,
    HueSat,
}

/// Transformation libre en cours (Ctrl+T, comme Photoshop) : le calque d'avant, et la taille, la rotation et
/// le déplacement autour du centre de son contenu
#[derive(Clone)]
struct Xf {
    orig: Layer,
    /// Centre et demi-taille du contenu d'origine (px du document)
    c0: [f32; 2],
    half: [f32; 2],
    t: [f32; 2],
    s: [f32; 2],
    r: f32,
    /// Geste en cours : poignée (-1, 0, 1 en x et y ; (0, 0) : déplacer ; None : tourner), départ, état de départ
    drag: Option<(Option<[i8; 2]>, Pos2, [f32; 5])>,
    /// Déformation en grille : les points (WARP_N × WARP_N, px du document, ligne par ligne) et celui qu'on tire
    grid: Option<Vec<[f32; 2]>>,
    grid_drag: Option<usize>,
}

/// Points par côté de la grille de déformation
const WARP_N: usize = 4;

/// Catmull-Rom sur quatre valeurs, t entre p1 et p2
fn catmull(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t2 * t)
}

/// Point de la surface lisse qui passe par la grille ; u, v entre 0 et 1
fn warp_point(g: &[[f32; 2]], n: usize, u: f32, v: f32) -> [f32; 2] {
    let (fu, fv) = (u * (n - 1) as f32, v * (n - 1) as f32);
    let (iu, iv) = ((fu.floor() as usize).min(n - 2), (fv.floor() as usize).min(n - 2));
    let (tu, tv) = (fu - iu as f32, fv - iv as f32);
    // Au bord, on prolonge la grille en ligne droite
    let at = |i: isize, j: isize| -> [f32; 2] {
        let ci = |k: isize| k.clamp(0, n as isize - 1) as usize;
        let (a, b) = (ci(i), ci(j));
        let base = g[b * n + a];
        let (ei, ej) = (i - a as isize, j - b as isize);
        if ei == 0 && ej == 0 {
            return base;
        }
        // Reflet du voisin intérieur
        let ni = ci(a as isize - ei);
        let nj = ci(b as isize - ej);
        let inner = g[nj * n + ni];
        [2.0 * base[0] - inner[0], 2.0 * base[1] - inner[1]]
    };
    let mut rows = [[0.0f32; 2]; 4];
    for (r, row) in rows.iter_mut().enumerate() {
        let j = iv as isize - 1 + r as isize;
        for c in 0..2 {
            row[c] = catmull(at(iu as isize - 1, j)[c], at(iu as isize, j)[c], at(iu as isize + 1, j)[c], at(iu as isize + 2, j)[c], tu);
        }
    }
    [0, 1].map(|c| catmull(rows[0][c], rows[1][c], rows[2][c], rows[3][c], tv))
}

/// Le calque déformé : le cadre `b` (x0, y0, x1, y1) du calque d'origine suit la grille
pub fn warp_layer(orig: &Layer, b: [f32; 4], g: &[[f32; 2]], n: usize) -> Layer {
    const SUB: usize = 12;
    let m = (n - 1) * SUB;
    let dst: Vec<[f32; 2]> = (0..=m).flat_map(|j| (0..=m).map(move |i| (i, j))).map(|(i, j)| warp_point(g, n, i as f32 / m as f32, j as f32 / m as f32)).collect();
    let srcp = |i: usize, j: usize| [b[0] + (b[2] - b[0]) * i as f32 / m as f32, b[1] + (b[3] - b[1]) * j as f32 / m as f32];
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for d in &dst {
        x0 = x0.min(d[0]);
        y0 = y0.min(d[1]);
        x1 = x1.max(d[0]);
        y1 = y1.max(d[1]);
    }
    let (ox, oy) = (x0.floor() as i32, y0.floor() as i32);
    let (w, h) = (((x1.ceil() as i32 - ox).clamp(1, 16384)) as usize, ((y1.ceil() as i32 - oy).clamp(1, 16384)) as usize);
    let mut out = Layer::blank(&orig.name, w, h);
    out.off = [ox, oy];
    let sample = |x: f32, y: f32| -> [f32; 4] {
        let (fx, fy) = (x - orig.off[0] as f32 - 0.5, y - orig.off[1] as f32 - 0.5);
        let (ix, iy) = (fx.floor() as i32, fy.floor() as i32);
        let (ax, ay) = (fx - ix as f32, fy - iy as f32);
        let px = |x: i32, y: i32| -> [f32; 4] {
            if x < 0 || y < 0 || x as usize >= orig.w || y as usize >= orig.h {
                return [0.0; 4];
            }
            let i = (y as usize * orig.w + x as usize) * 4;
            let a = orig.px[i + 3] as f32 / 255.0;
            [orig.px[i] as f32 * a, orig.px[i + 1] as f32 * a, orig.px[i + 2] as f32 * a, a]
        };
        let (p00, p10, p01, p11) = (px(ix, iy), px(ix + 1, iy), px(ix, iy + 1), px(ix + 1, iy + 1));
        [0, 1, 2, 3].map(|k| (p00[k] * (1.0 - ax) + p10[k] * ax) * (1.0 - ay) + (p01[k] * (1.0 - ax) + p11[k] * ax) * ay)
    };
    // Chaque petit quadrilatère en deux triangles, remplis point par point (coordonnées barycentriques)
    let mut tri = |a: ([f32; 2], [f32; 2]), bb: ([f32; 2], [f32; 2]), c: ([f32; 2], [f32; 2])| {
        let (pa, pb, pc) = (a.0, bb.0, c.0);
        let den = (pb[1] - pc[1]) * (pa[0] - pc[0]) + (pc[0] - pb[0]) * (pa[1] - pc[1]);
        if den.abs() < 1e-6 {
            return;
        }
        let xs = (pa[0].min(pb[0]).min(pc[0]).floor() as i32).max(ox);
        let xe = (pa[0].max(pb[0]).max(pc[0]).ceil() as i32).min(ox + w as i32 - 1);
        let ys = (pa[1].min(pb[1]).min(pc[1]).floor() as i32).max(oy);
        let ye = (pa[1].max(pb[1]).max(pc[1]).ceil() as i32).min(oy + h as i32 - 1);
        for y in ys..=ye {
            for x in xs..=xe {
                let (qx, qy) = (x as f32 + 0.5, y as f32 + 0.5);
                let l1 = ((pb[1] - pc[1]) * (qx - pc[0]) + (pc[0] - pb[0]) * (qy - pc[1])) / den;
                let l2 = ((pc[1] - pa[1]) * (qx - pc[0]) + (pa[0] - pc[0]) * (qy - pc[1])) / den;
                let l3 = 1.0 - l1 - l2;
                if l1 < -1e-4 || l2 < -1e-4 || l3 < -1e-4 {
                    continue;
                }
                let sx = a.1[0] * l1 + bb.1[0] * l2 + c.1[0] * l3;
                let sy = a.1[1] * l1 + bb.1[1] * l2 + c.1[1] * l3;
                let v = sample(sx, sy);
                if v[3] <= 1e-4 {
                    continue;
                }
                let i = ((y - oy) as usize * w + (x - ox) as usize) * 4;
                out.px[i] = (v[0] / v[3]).round().clamp(0.0, 255.0) as u8;
                out.px[i + 1] = (v[1] / v[3]).round().clamp(0.0, 255.0) as u8;
                out.px[i + 2] = (v[2] / v[3]).round().clamp(0.0, 255.0) as u8;
                out.px[i + 3] = (v[3] * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    };
    for j in 0..m {
        for i in 0..m {
            let k = |i: usize, j: usize| (dst[j * (m + 1) + i], srcp(i, j));
            tri(k(i, j), k(i + 1, j), k(i + 1, j + 1));
            tri(k(i, j), k(i + 1, j + 1), k(i, j + 1));
        }
    }
    out.visible = orig.visible;
    out.opacity = orig.opacity;
    out.blend = orig.blend;
    out.fill = orig.fill;
    out.clip = orig.clip;
    out.group = orig.group;
    out.fx = orig.fx.clone();
    out.lock_alpha = orig.lock_alpha;
    out
}

impl Xf {
    /// Point local (unités du contenu d'origine, centré) → document
    fn map(&self, q: [f32; 2]) -> [f32; 2] {
        xf_map(self.c0, self.t, self.s, self.r, q)
    }
}

fn xf_map(c0: [f32; 2], t: [f32; 2], s: [f32; 2], r: f32, q: [f32; 2]) -> [f32; 2] {
    let (c, sn) = (r.cos(), r.sin());
    let (x, y) = (q[0] * s[0], q[1] * s[1]);
    [c0[0] + t[0] + c * x - sn * y, c0[1] + t[1] + sn * x + c * y]
}

/// Cadre du contenu visible d'un calque (px du document) : x0, y0, x1, y1
pub fn content_box(l: &Layer) -> Option<[i32; 4]> {
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for y in 0..l.h {
        for x in 0..l.w {
            if l.px[(y * l.w + x) * 4 + 3] > 0 {
                x0 = x0.min(x as i32);
                x1 = x1.max(x as i32 + 1);
                y0 = y0.min(y as i32);
                y1 = y1.max(y as i32 + 1);
            }
        }
    }
    (x1 > x0).then(|| [x0 + l.off[0], y0 + l.off[1], x1 + l.off[0], y1 + l.off[1]])
}

/// Le calque `orig` agrandi, tourné et déplacé (rééchantillonnage bilinéaire, couleurs prémultipliées
/// pour ne pas noircir les bords). Le masque, lui, reste où il est.
pub fn transform_layer(orig: &Layer, c0: [f32; 2], t: [f32; 2], s: [f32; 2], r: f32) -> Layer {
    use rayon::prelude::*;
    let (sx, sy) = (if s[0].abs() < 1e-4 { 1e-4 } else { s[0] }, if s[1].abs() < 1e-4 { 1e-4 } else { s[1] });
    // Cadre de sortie : les coins du calque d'origine transformés
    let corners = [[0.0, 0.0], [orig.w as f32, 0.0], [0.0, orig.h as f32], [orig.w as f32, orig.h as f32]]
        .map(|[x, y]| xf_map(c0, t, [sx, sy], r, [(x + orig.off[0] as f32 - c0[0]) / 1.0, (y + orig.off[1] as f32 - c0[1]) / 1.0]));
    let x0 = corners.iter().map(|c| c[0]).fold(f32::MAX, f32::min).floor() as i32;
    let y0 = corners.iter().map(|c| c[1]).fold(f32::MAX, f32::min).floor() as i32;
    let x1 = corners.iter().map(|c| c[0]).fold(f32::MIN, f32::max).ceil() as i32;
    let y1 = corners.iter().map(|c| c[1]).fold(f32::MIN, f32::max).ceil() as i32;
    let (w, h) = ((x1 - x0).clamp(1, 16384) as usize, (y1 - y0).clamp(1, 16384) as usize);
    let (c, sn) = (r.cos(), r.sin());
    let src = |x: i32, y: i32| -> [f32; 4] {
        if x < 0 || y < 0 || x as usize >= orig.w || y as usize >= orig.h {
            return [0.0; 4];
        }
        let i = (y as usize * orig.w + x as usize) * 4;
        let a = orig.px[i + 3] as f32 / 255.0;
        [orig.px[i] as f32 * a, orig.px[i + 1] as f32 * a, orig.px[i + 2] as f32 * a, a]
    };
    let mut px = vec![0u8; w * h * 4];
    px.par_chunks_mut(w * 4).enumerate().for_each(|(yy, row)| {
        for xx in 0..w {
            // Centre du pixel de sortie → point du contenu d'origine
            let (dx, dy) = (x0 as f32 + xx as f32 + 0.5 - c0[0] - t[0], y0 as f32 + yy as f32 + 0.5 - c0[1] - t[1]);
            let (lx, ly) = ((c * dx + sn * dy) / sx, (-sn * dx + c * dy) / sy);
            let (fx, fy) = (lx + c0[0] - orig.off[0] as f32 - 0.5, ly + c0[1] - orig.off[1] as f32 - 0.5);
            let (ix, iy) = (fx.floor(), fy.floor());
            let (ax, ay) = (fx - ix, fy - iy);
            let (ix, iy) = (ix as i32, iy as i32);
            let (p00, p10, p01, p11) = (src(ix, iy), src(ix + 1, iy), src(ix, iy + 1), src(ix + 1, iy + 1));
            let mut v = [0.0f32; 4];
            for k in 0..4 {
                v[k] = (p00[k] * (1.0 - ax) + p10[k] * ax) * (1.0 - ay) + (p01[k] * (1.0 - ax) + p11[k] * ax) * ay;
            }
            if v[3] > 1e-4 {
                let o = xx * 4;
                row[o] = (v[0] / v[3]).round().clamp(0.0, 255.0) as u8;
                row[o + 1] = (v[1] / v[3]).round().clamp(0.0, 255.0) as u8;
                row[o + 2] = (v[2] / v[3]).round().clamp(0.0, 255.0) as u8;
                row[o + 3] = (v[3] * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    });
    Layer { off: [x0, y0], w, h, px, fx_maps: None, ..orig.clone() }
}

pub struct Editor {
    pub path: PathBuf,
    pub doc: Doc,
    pub tool: Tool,
    pub color: Color32,
    /// Couleur d'arrière-plan (X échange, D remet noir et blanc)
    pub bg_color: Color32,
    /// Sélecteur de couleur ouvert (arrière-plan ?) et sa place
    swatch_pick: Option<bool>,
    psd_warned: Vec<String>,
    /// Styles de calque à recalculer ; fenêtre des styles ouverte (calque)
    fx_due: bool,
    fx_window: Option<usize>,
    swallow_click: bool,
    swatch_at: Pos2,
    pub size: f32,
    pub hardness: f32,
    pub flow: f32,
    pub tolerance: f32,
    /// Pot de peinture : trous du trait refermés jusqu'à cette taille (px), 0 : aucun
    pub fill_gap: f32,
    sel: Option<Sel>,
    /// Points du lasso en cours
    lasso: Vec<[f32; 2]>,
    /// Dégradé en cours : départ et arrivée
    grad: Option<(Pos2, Pos2)>,
    /// Sélection d'avant le geste, et le mode : 0 remplacer, 1 ajouter (Maj), 2 retirer (Alt)
    sel_base: Option<(Option<Sel>, u8)>,
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
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    /// Nom du prochain état de l'historique (sinon celui de l'outil)
    next_label: Option<&'static str>,
    /// Pression de la tablette (1 sans tablette) et ce qu'elle règle
    pressure: f32,
    press_size: bool,
    press_flow: bool,
    /// Lissage du trait (0 : aucun) et le point lissé du trait en cours
    smooth: f32,
    stab: Option<Pos2>,
    /// Grain du papier (0 : lisse) : le pinceau ne touche pas le fond des creux
    grain: f32,
    /// Symétrie du pinceau : sur l'axe vertical, l'axe horizontal (autour du centre du document)
    sym_x: bool,
    /// Enroulé : le document se répète tout autour, le trait passe d'un bord à l'autre (textures sans raccord)
    wrap: bool,
    sym_y: bool,
    /// Perspective : points de fuite (dans le document), affichage, aimantation du trait, point qu'on déplace
    persp: Vec<Pos2>,
    persp_on: bool,
    persp_snap: bool,
    persp_drag: Option<usize>,
    /// Départ du trait en cours (pour l'aimantation de perspective)
    stroke_from: Option<Pos2>,
    /// Lecture de l'animation : départ et image de départ
    anim_play: Option<(Instant, usize)>,
    /// Vignettes des images de l'animation : (calque, clé, texture)
    anim_thumbs: Vec<(usize, u64, egui::TextureHandle)>,
    /// Recadrage en cours (x0, y0, x1, y1 du document), le format et le geste (0 : nouveau cadre, 1 : déplacer)
    crop: Option<[f32; 4]>,
    crop_ratio: usize,
    crop_drag: Option<(u8, Pos2)>,
    /// Règles visibles (Ctrl+R) et repère qu'on tire
    rulers: bool,
    /// Avant / après : l'image à l'ouverture, et la séparation (part de la largeur de la toile à l'écran)
    before: Option<(egui::ColorImage, Option<egui::TextureHandle>)>,
    split: Option<f32>,
    guide_drag: Option<usize>,
    /// Vue tournée (radians) et en miroir : l'image ne change pas
    view_rot: f32,
    view_flip: bool,
    history_open: bool,
    /// Trait du correcteur en cours (affiché en surimpression jusqu'au relâché)
    heal_trail: Vec<Pos2>,
    /// Doigt : la peinture « sur le doigt » (RGBA, carré de côté 2R+1) ; fluidité : le tampon d'avant
    smudge_buf: Vec<[f32; 4]>,
    smudge_r: i32,
    prev_dab: Option<Pos2>,
    pub save_due: Option<Instant>,
    pub message: Option<String>,
    adjust: Option<(Adjust, Layer, [f32; 2])>,
    renaming: Option<usize>,
    /// Texte en cours de saisie, avant d'être posé en calque
    text: Option<TextDraft>,
    /// Largeur du panneau des calques à la dernière image
    panel_w: f32,
    /// On peint dans le masque du calque actif (sa vignette est encadrée)
    edit_mask: bool,
    /// Alt+clic sur la vignette du masque : on ne voit que le masque, en gris
    mask_view: bool,
    /// Vignettes des calques et des masques (calque, masque ?) et l'instant du dernier calcul
    thumbs: std::collections::HashMap<(usize, bool), egui::TextureHandle>,
    thumbs_due: bool,
    thumbs_at: Option<Instant>,
    /// Calque qu'on glisse dans la liste (pour le changer de place)
    drag_layer: Option<usize>,
    /// Groupe choisi dans la liste (ses réglages s'affichent en haut)
    sel_group: Option<u32>,
    pub text_font: usize,
    pub text_size: f32,
    /// Transformation libre en cours
    xf: Option<Xf>,
    /// Tampon : la source (Alt+clic) et le décalage, fixé au premier coup et gardé ensuite (aligné, comme Photoshop)
    clone_src: Option<Pos2>,
    clone_off: Option<egui::Vec2>,
}

/// Texte en cours : point d'ancrage (haut-gauche, px du document) et aperçu rendu
struct TextDraft {
    at: [f32; 2],
    s: String,
    preview: Option<(String, egui::TextureHandle, [i32; 2], [usize; 2])>,
    focus: bool,
}

/// Rend un texte (aligné à gauche, plusieurs lignes) : l'image RGBA et la position de son coin
/// haut-gauche par rapport à l'ancrage
pub fn render_text(text: &str, font: &str, size: f32, color: [u8; 4]) -> Option<(Vec<u8>, usize, usize, [i32; 2])> {
    use ab_glyph::{Font, ScaleFont};
    let f = crate::motion::font(font);
    let size = size.clamp(2.0, 2000.0);
    let sf = f.as_scaled(ab_glyph::PxScale::from(size));
    let line_h = sf.height() + sf.line_gap();
    let mut outlined = vec![];
    for (li, line) in text.lines().enumerate() {
        let mut x = 0.0f32;
        let y = li as f32 * line_h + sf.ascent();
        let mut prev = None;
        for ch in line.chars() {
            let id = sf.glyph_id(ch);
            if let Some(p) = prev {
                x += sf.kern(p, id);
            }
            if let Some(o) = f.outline_glyph(id.with_scale_and_position(ab_glyph::PxScale::from(size), ab_glyph::point(x, y))) {
                outlined.push(o);
            }
            x += sf.h_advance(id);
            prev = Some(id);
        }
    }
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for o in &outlined {
        let b = o.px_bounds();
        x0 = x0.min(b.min.x.floor() as i32);
        y0 = y0.min(b.min.y.floor() as i32);
        x1 = x1.max(b.max.x.ceil() as i32);
        y1 = y1.max(b.max.y.ceil() as i32);
    }
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let mut px = vec![0u8; w * h * 4];
    for o in &outlined {
        let b = o.px_bounds();
        o.draw(|gx, gy, cov| {
            let (x, y) = (b.min.x.floor() as i32 + gx as i32 - x0, b.min.y.floor() as i32 + gy as i32 - y0);
            if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
                return;
            }
            let i = (y as usize * w + x as usize) * 4;
            let a = (cov.min(1.0) * color[3] as f32).round() as u8;
            if a > px[i + 3] {
                px[i..i + 3].copy_from_slice(&color[..3]);
                px[i + 3] = a;
            }
        });
    }
    Some((px, w, h, [x0, y0]))
}

impl Editor {
    /// Un éditeur sur un document vide (tests)
    #[cfg(test)]
    pub fn new_test(w: usize, h: usize) -> Editor {
        let path = std::env::temp_dir().join(format!("ary-test-{}-{w}x{h}.aryimg", std::process::id()));
        Doc::new(w, h, false).save(&path).unwrap();
        Editor::open(path).unwrap()
    }

    pub fn open(path: PathBuf) -> Result<Editor, String> {
        let doc = Doc::load(&path)?;
        let n = doc.w * doc.h * 4;
        let before = {
            let img = doc.flatten();
            Some((egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()), None))
        };
        Ok(Editor {
            before,
            split: None,
            path,
            comp: vec![0; n],
            dirty: Some([0, 0, doc.w, doc.h]),
            doc,
            tool: Tool::Brush,
            bg_color: Color32::WHITE,
            swatch_pick: None,
            psd_warned: vec![],
            fx_due: true,
            fx_window: None,
            swallow_click: false,
            swatch_at: Pos2::ZERO,
            text: None,
            panel_w: 230.0,
            edit_mask: false,
            mask_view: false,
            thumbs: Default::default(),
            thumbs_due: true,
            thumbs_at: None,
            drag_layer: None,
            sel_group: None,
            text_font: 0,
            text_size: 64.0,
            color: Color32::from_rgb(30, 30, 34),
            size: 18.0,
            hardness: 0.7,
            flow: 1.0,
            grain: 0.0,
            sym_x: false,
            wrap: false,
            sym_y: false,
            persp: vec![],
            persp_on: false,
            persp_snap: true,
            persp_drag: None,
            stroke_from: None,
            anim_play: None,
            anim_thumbs: vec![],
            tolerance: 0.12,
            fill_gap: 0.0,
            sel: None,
            lasso: vec![],
            grad: None,
            sel_base: None,
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
            xf: None,
            next_label: None,
            pressure: 1.0,
            press_size: true,
            press_flow: false,
            smooth: 0.0,
            stab: None,
            crop: None,
            crop_ratio: 0,
            crop_drag: None,
            rulers: false,
            guide_drag: None,
            view_rot: 0.0,
            view_flip: false,
            history_open: false,
            heal_trail: vec![],
            smudge_buf: vec![],
            smudge_r: 0,
            prev_dab: None,
            clone_src: None,
            clone_off: None,
        })
    }

    pub fn save_if_due(&mut self, force: bool) {
        if let Some(t) = self.save_due {
            if force || t.elapsed().as_secs_f32() > 1.5 {
                // En PSD, ce que Photoshop ne recevra pas est dit une fois
                if is_psd(&self.path) {
                    let (_, lost) = crate::psd::write(&self.doc);
                    if !lost.is_empty() && self.psd_warned != lost {
                        self.message = Some(format!("{} {}", crate::i18n::t("Pas encore transmis au PSD :"), lost.join(", ")));
                        self.psd_warned = lost;
                    }
                }
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
        self.thumbs_due = true;
        self.fx_due = true;
    }

    fn all(&mut self) {
        let (w, h) = (self.doc.w, self.doc.h);
        self.touched([0, 0, w, h]);
    }

    /// Ctrl+T : transformation libre du calque actif
    fn start_xf(&mut self) {
        if self.xf.is_some() {
            return;
        }
        let l = &self.doc.layers[self.doc.active];
        if l.adj.is_some() || l.lock_all || self.edit_mask {
            self.message = Some(crate::i18n::t("Ce calque ne se transforme pas (réglage, verrouillé ou masque)"));
            return;
        }
        let Some(b) = content_box(l) else {
            self.message = Some(crate::i18n::t("Le calque est vide"));
            return;
        };
        let orig = l.clone();
        self.checkpoint();
        let c0 = [(b[0] + b[2]) as f32 / 2.0, (b[1] + b[3]) as f32 / 2.0];
        let half = [(b[2] - b[0]) as f32 / 2.0, (b[3] - b[1]) as f32 / 2.0];
        self.xf = Some(Xf { orig, c0, half, t: [0.0; 2], s: [1.0; 2], r: 0.0, drag: None, grid: None, grid_drag: None });
    }

    /// Déformation en grille : la transformation, avec une grille de points à tirer
    fn start_warp(&mut self) {
        self.start_xf();
        if let Some(x) = &mut self.xf {
            let n = WARP_N;
            let (x0, y0) = (x.c0[0] - x.half[0], x.c0[1] - x.half[1]);
            let (w, h) = (x.half[0] * 2.0, x.half[1] * 2.0);
            x.grid = Some((0..n * n).map(|k| [x0 + w * (k % n) as f32 / (n - 1) as f32, y0 + h * (k / n) as f32 / (n - 1) as f32]).collect());
        }
    }

    fn apply_xf(&mut self) {
        if let Some(x) = &self.xf {
            if let Some(g) = &x.grid {
                let b = [x.c0[0] - x.half[0], x.c0[1] - x.half[1], x.c0[0] + x.half[0], x.c0[1] + x.half[1]];
                let mut nl = warp_layer(&x.orig, b, g, WARP_N);
                nl.mask = x.orig.mask.clone();
                nl.mask_on = x.orig.mask_on;
                self.doc.layers[self.doc.active] = nl;
                self.all();
                return;
            }
            let mut nl = transform_layer(&x.orig, x.c0, x.t, x.s, x.r);
            // Le masque est lié au calque : il suit. Hors de sa zone, un masque montre tout (blanc)
            if let Some(m) = &x.orig.mask {
                let mut tm = transform_layer(m, x.c0, x.t, x.s, x.r);
                for px in tm.px.chunks_mut(4) {
                    let a = px[3] as f32 / 255.0;
                    let v = (px[0] as f32 * a + 255.0 * (1.0 - a)).round() as u8;
                    px.copy_from_slice(&[v, v, v, 255]);
                }
                tm.mask = None;
                nl.mask = Some(Box::new(tm));
            }
            self.doc.layers[self.doc.active] = nl;
            self.all();
        }
    }

    /// Entrée : on garde ; Échap : on revient au calque d'avant
    fn end_xf(&mut self, keep: bool) {
        if let Some(x) = self.xf.take() {
            if !keep {
                self.doc.layers[self.doc.active] = x.orig;
                self.undo.pop();
                self.all();
            }
        }
    }

    /// La grille de déformation : courbes lisses à l'écran, points à tirer ; le calque suit au relâché
    fn warp_ui(&mut self, ui: &egui::Ui, resp: &egui::Response, painter: &egui::Painter, to_doc: impl Fn(Pos2) -> Pos2, to_screen: impl Fn(f32, f32) -> Pos2) {
        let p = theme::pal();
        let Some(x) = self.xf.as_ref() else { return };
        let Some(g) = x.grid.clone() else { return };
        let n = WARP_N;
        // Les lignes de la grille, en courbes
        for k in 0..n {
            let t = k as f32 / (n - 1) as f32;
            let row: Vec<Pos2> = (0..=40).map(|i| warp_point(&g, n, i as f32 / 40.0, t)).map(|q| to_screen(q[0], q[1])).collect();
            let col: Vec<Pos2> = (0..=40).map(|i| warp_point(&g, n, t, i as f32 / 40.0)).map(|q| to_screen(q[0], q[1])).collect();
            painter.add(egui::Shape::line(row, Stroke::new(1.0, p.accent)));
            painter.add(egui::Shape::line(col, Stroke::new(1.0, p.accent)));
        }
        let pts: Vec<Pos2> = g.iter().map(|q| to_screen(q[0], q[1])).collect();
        for q in &pts {
            painter.circle(*q, 4.5, Color32::WHITE, Stroke::new(1.0, p.accent));
        }
        let hover = ui.input(|i| i.pointer.hover_pos());
        let near = |q: Pos2| pts.iter().position(|h| h.distance(q) < 9.0);
        if hover.and_then(near).is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            let hit = ui.input(|i| i.pointer.press_origin()).and_then(near);
            if let Some(xx) = &mut self.xf {
                xx.grid_drag = hit;
            }
        }
        if resp.dragged_by(egui::PointerButton::Primary) {
            if let (Some(at), Some(xx)) = (resp.interact_pointer_pos(), self.xf.as_mut()) {
                if let (Some(k), Some(g)) = (xx.grid_drag, xx.grid.as_mut()) {
                    let d = to_doc(at);
                    g[k] = [d.x, d.y];
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                }
            }
        }
        if resp.drag_stopped() && self.xf.as_ref().is_some_and(|x| x.grid_drag.is_some()) {
            if let Some(xx) = &mut self.xf {
                xx.grid_drag = None;
            }
            self.apply_xf();
        }
        let tip = egui::Align2::CENTER_TOP;
        let r = resp.rect;
        painter.text(pos2(r.center().x, r.top() + 8.0), tip, crate::i18n::t("Déformation : tirer les points · Entrée : garder · Échap : annuler"), egui::FontId::proportional(12.0), p.text);
    }

    /// Les gestes de la transformation : poignées (taille), dedans (déplacer), dehors (tourner)
    fn xf_ui(&mut self, ui: &egui::Ui, resp: &egui::Response, painter: &egui::Painter, to_doc: impl Fn(Pos2) -> Pos2, to_screen: impl Fn(f32, f32) -> Pos2) {
        let p = theme::pal();
        if self.xf.as_ref().is_some_and(|x| x.grid.is_some()) {
            return self.warp_ui(ui, resp, painter, to_doc, to_screen);
        }
        let Some(x) = self.xf.clone() else { return };
        let corner = |hx: f32, hy: f32| {
            let d = x.map([hx * x.half[0], hy * x.half[1]]);
            to_screen(d[0], d[1])
        };
        let poly: Vec<Pos2> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().map(|&(a, b)| corner(a, b)).collect();
        painter.add(egui::Shape::closed_line(poly.clone(), Stroke::new(1.0, p.accent)));
        let handles: Vec<([i8; 2], Pos2)> = [(-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0)].iter().map(|&(a, b)| ([a, b], corner(a as f32, b as f32))).collect();
        for (_, h) in &handles {
            let r = Rect::from_center_size(*h, vec2(8.0, 8.0));
            painter.rect_filled(r, 0.0, Color32::WHITE);
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
        }
        let c = corner(0.0, 0.0);
        painter.circle_stroke(c, 4.0, Stroke::new(1.0, p.accent));
        let inside = |q: Pos2| {
            // Point dans le quadrilatère (convexe) : même côté de chaque arête
            let mut sign = 0.0f32;
            for i in 0..4 {
                let (a, b) = (poly[i], poly[(i + 1) % 4]);
                let cr = (b - a).x * (q - a).y - (b - a).y * (q - a).x;
                if sign == 0.0 {
                    sign = cr.signum();
                } else if cr.signum() != sign && cr != 0.0 {
                    return false;
                }
            }
            true
        };
        // Curseur selon ce qu'il y a sous la souris
        let hover = ui.input(|i| i.pointer.hover_pos());
        let hit = |q: Pos2| -> Option<[i8; 2]> {
            if let Some((h, _)) = handles.iter().find(|(_, h)| h.distance(q) < 7.0) {
                Some(*h)
            } else if inside(q) {
                Some([0, 0])
            } else {
                None
            }
        };
        if let Some(q) = hover.filter(|q| resp.rect.contains(*q)) {
            ui.ctx().set_cursor_icon(match hit(q) {
                Some([0, 0]) => egui::CursorIcon::Move,
                Some([a, b]) if a != 0 && b != 0 => egui::CursorIcon::ResizeNeSw,
                Some([0, _]) => egui::CursorIcon::ResizeVertical,
                Some(_) => egui::CursorIcon::ResizeHorizontal,
                None => egui::CursorIcon::Alias,
            });
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            if let Some(at) = ui.input(|i| i.pointer.press_origin()) {
                if let Some(xx) = &mut self.xf {
                    xx.drag = Some((hit(at), to_doc(at), [x.t[0], x.t[1], x.s[0], x.s[1], x.r]));
                }
            }
        }
        if resp.dragged_by(egui::PointerButton::Primary) {
            let (shift, alt) = ui.input(|i| (i.modifiers.shift, i.modifiers.alt));
            if let (Some(at), Some(xx)) = (resp.interact_pointer_pos(), self.xf.as_mut()) {
                let m = to_doc(at);
                if let Some((h, from, st)) = xx.drag {
                    let (t0, s0, r0) = ([st[0], st[1]], [st[2], st[3]], st[4]);
                    let center = [xx.c0[0] + t0[0], xx.c0[1] + t0[1]];
                    match h {
                        Some([0, 0]) => xx.t = [t0[0] + m.x - from.x, t0[1] + m.y - from.y],
                        None => {
                            let a0 = (from.y - center[1]).atan2(from.x - center[0]);
                            let a1 = (m.y - center[1]).atan2(m.x - center[0]);
                            let mut r = r0 + a1 - a0;
                            // Maj : par pas de 15°
                            if shift {
                                let step = 15f32.to_radians();
                                r = (r / step).round() * step;
                            }
                            xx.r = r;
                        }
                        Some([hx, hy]) => {
                            let (hx, hy) = (hx as f32, hy as f32);
                            // Souris dans le repère tourné, depuis le centre de départ
                            let (c, sn) = (r0.cos(), r0.sin());
                            let (dx, dy) = (m.x - center[0], m.y - center[1]);
                            let (mx, my) = (c * dx + sn * dy, -sn * dx + c * dy);
                            // Point fixe : le côté opposé (ou le centre avec Alt)
                            let anchor = if alt { [0.0, 0.0] } else { [-hx * xx.half[0], -hy * xx.half[1]] };
                            let (ax, ay) = (anchor[0] * s0[0], anchor[1] * s0[1]);
                            let mut s = s0;
                            if hx != 0.0 {
                                s[0] = (mx - ax) / (hx * xx.half[0] - anchor[0]);
                            }
                            if hy != 0.0 {
                                s[1] = (my - ay) / (hy * xx.half[1] - anchor[1]);
                            }
                            // Coin : proportions gardées (comme Photoshop), Maj pour les libérer
                            if hx != 0.0 && hy != 0.0 && !shift {
                                let k = if (s[0] / s0[0]).abs() > (s[1] / s0[1]).abs() { s[0] / s0[0] } else { s[1] / s0[1] };
                                s = [s0[0] * k, s0[1] * k];
                            }
                            // Le point fixe ne bouge pas
                            let fix = |sc: [f32; 2]| {
                                let (x, y) = (anchor[0] * sc[0], anchor[1] * sc[1]);
                                [c * x - sn * y, sn * x + c * y]
                            };
                            let (a_old, a_new) = (fix(s0), fix(s));
                            xx.s = s;
                            xx.t = [t0[0] + a_old[0] - a_new[0], t0[1] + a_old[1] - a_new[1]];
                        }
                    }
                }
            }
            self.apply_xf();
        }
        if resp.drag_stopped() {
            if let Some(xx) = &mut self.xf {
                xx.drag = None;
            }
        }
    }

    /// Correcteur localisé : les pixels couverts par le trait sont refaits par diffusion depuis leurs bords
    /// (moyenne des voisins, répétée), puis on y remet le grain d'un morceau voisin
    fn heal_apply(&mut self) {
        let Some((start, cover, _)) = self.stroke.take() else { return };
        let (w, h) = (start.w, start.h);
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        for (i, c) in cover.iter().enumerate() {
            if *c > 0.0 {
                let (x, y) = (i % w, i / w);
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
        if x0 > x1 {
            return;
        }
        let (bx0, by0) = (x0.saturating_sub(2), y0.saturating_sub(2));
        let (bx1, by1) = ((x1 + 3).min(w), (y1 + 3).min(h));
        let (bw, bh) = (bx1 - bx0, by1 - by0);
        let at = |x: usize, y: usize| (y * w + x) * 4;
        // Valeurs de départ : la moyenne du bord de la zone
        let mut f: Vec<[f32; 4]> = vec![[0.0; 4]; bw * bh];
        let mut sum = [0.0f32; 4];
        let mut n = 0.0;
        for y in by0..by1 {
            for x in bx0..bx1 {
                let i = at(x, y);
                let v = [start.px[i] as f32, start.px[i + 1] as f32, start.px[i + 2] as f32, start.px[i + 3] as f32];
                f[(y - by0) * bw + x - bx0] = v;
                if cover[y * w + x] <= 0.0 {
                    for k in 0..4 {
                        sum[k] += v[k];
                    }
                    n += 1.0;
                }
            }
        }
        if n > 0.0 {
            for y in by0..by1 {
                for x in bx0..bx1 {
                    if cover[y * w + x] > 0.0 {
                        f[(y - by0) * bw + x - bx0] = sum.map(|s| s / n);
                    }
                }
            }
        }
        // Diffusion : chaque pixel de la zone devient la moyenne de ses quatre voisins
        let iters = (bw.max(bh) * 3).clamp(60, 600);
        for _ in 0..iters {
            for y in 1..bh - 1 {
                for x in 1..bw - 1 {
                    if cover[(y + by0) * w + x + bx0] > 0.0 {
                        let k = y * bw + x;
                        let (a, b, c, d) = (f[k - 1], f[k + 1], f[k - bw], f[k + bw]);
                        f[k] = [0, 1, 2, 3].map(|j| (a[j] + b[j] + c[j] + d[j]) * 0.25);
                    }
                }
            }
        }
        // Le grain : l'écart à la moyenne locale d'un morceau voisin, hors de la zone
        let dx = (x1 - x0 + 4) as i64;
        let side = if (x1 as i64 + dx) < w as i64 { dx } else { -dx };
        let px = |x: i64, y: i64, c: usize| -> f32 {
            let (x, y) = (x.clamp(0, w as i64 - 1) as usize, y.clamp(0, h as i64 - 1) as usize);
            start.px[at(x, y) + c] as f32
        };
        self.heal_trail.clear();
        let l = self.target();
        for y in by0..by1 {
            for x in bx0..bx1 {
                let c = cover[y * w + x];
                if c <= 0.0 {
                    continue;
                }
                let (sx, sy) = (x as i64 + side, y as i64);
                let i = at(x, y);
                let v = f[(y - by0) * bw + x - bx0];
                for k in 0..4 {
                    let local = (px(sx - 1, sy, k) + px(sx + 1, sy, k) + px(sx, sy - 1, k) + px(sx, sy + 1, k)) * 0.25;
                    let grain = if k < 3 { px(sx, sy, k) - local } else { 0.0 };
                    let healed = (v[k] + grain).clamp(0.0, 255.0);
                    l.px[i + k] = (start.px[i + k] as f32 * (1.0 - c) + healed * c).round() as u8;
                }
            }
        }
        let off = self.target_ref().off;
        let r = [(bx0 as i32 + off[0]).max(0) as usize, (by0 as i32 + off[1]).max(0) as usize, (bx1 as i32 + off[0]).max(0) as usize, (by1 as i32 + off[1]).max(0) as usize];
        self.touched(r);
    }

    fn snap(&self, label: String) -> Snap {
        Snap { layers: self.doc.layers.clone(), active: self.doc.active, groups: self.doc.groups.clone(), w: self.doc.w, h: self.doc.h, label }
    }

    /// Remet un état, et rend celui qu'on quitte (avec le nom de `keep_label`)
    fn restore(&mut self, s: Snap, keep_label: String) -> Snap {
        let old = Snap { layers: std::mem::replace(&mut self.doc.layers, s.layers), active: self.doc.active, groups: std::mem::replace(&mut self.doc.groups, s.groups), w: self.doc.w, h: self.doc.h, label: keep_label };
        self.doc.active = s.active.min(self.doc.layers.len().saturating_sub(1));
        if (s.w, s.h) != (self.doc.w, self.doc.h) {
            (self.doc.w, self.doc.h) = (s.w, s.h);
            self.sel = None;
            self.fit = true;
        }
        self.all();
        old
    }

    fn checkpoint(&mut self) {
        let label = match self.next_label.take() {
            Some(l) => crate::i18n::t(l),
            None => crate::i18n::t("modification"),
        };
        let s = self.snap(label);
        self.undo.push(s);
        if self.undo.len() > 40 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            let label = s.label.clone();
            let old = self.restore(s, label);
            self.redo.push(old);
        }
    }

    fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            let label = s.label.clone();
            let old = self.restore(s, label);
            self.undo.push(old);
        }
    }

    /// Point aimanté aux repères et aux bords du document (à 6 pixels d'écran près)
    fn snap_pt(&self, d: Pos2) -> Pos2 {
        let tol = 6.0 / self.zoom;
        let mut out = d;
        let (mut bx, mut by) = (tol, tol);
        let xs = self.doc.guides.iter().filter(|g| g.vertical).map(|g| g.at).chain([0.0, self.doc.w as f32, self.doc.w as f32 / 2.0]);
        for x in xs {
            if (d.x - x).abs() < bx {
                bx = (d.x - x).abs();
                out.x = x;
            }
        }
        let ys = self.doc.guides.iter().filter(|g| !g.vertical).map(|g| g.at).chain([0.0, self.doc.h as f32, self.doc.h as f32 / 2.0]);
        for y in ys {
            if (d.y - y).abs() < by {
                by = (d.y - y).abs();
                out.y = y;
            }
        }
        out
    }

    /// Cadre du recadrage contraint au format choisi, à partir de son coin de départ
    fn crop_box(&self, a: Pos2, b: Pos2) -> [f32; 4] {
        let r = match CROP_RATIOS[self.crop_ratio.min(CROP_RATIOS.len() - 1)].1 {
            x if x < 0.0 => self.doc.w as f32 / self.doc.h.max(1) as f32,
            x => x,
        };
        let b = if r > 0.0 {
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let w = dx.abs().max(dy.abs() * r);
            pos2(a.x + w * dx.signum(), a.y + w / r * if dy == 0.0 { 1.0 } else { dy.signum() })
        } else {
            b
        };
        [a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)]
    }

    /// Applique le recadrage : la toile change de taille, les pixels hors cadre restent dans les calques
    fn apply_crop(&mut self) {
        let Some(c) = self.crop.take() else { return };
        let (x0, y0) = (c[0].round() as i32, c[1].round() as i32);
        let (w, h) = ((c[2] - c[0]).round().max(1.0) as usize, (c[3] - c[1]).round().max(1.0) as usize);
        if (x0, y0, w, h) == (0, 0, self.doc.w, self.doc.h) {
            return;
        }
        self.next_label = Some("recadrage");
        self.checkpoint();
        for l in &mut self.doc.layers {
            l.off = [l.off[0] - x0, l.off[1] - y0];
            if let Some(m) = l.mask.as_deref_mut() {
                m.off = [m.off[0] - x0, m.off[1] - y0];
            }
            l.fx_maps = None;
        }
        for g in &mut self.doc.groups {
            if let Some(m) = g.mask.0.as_deref_mut() {
                m.off = [m.off[0] - x0, m.off[1] - y0];
            }
        }
        for g in &mut self.doc.guides {
            g.at -= if g.vertical { x0 as f32 } else { y0 as f32 };
        }
        (self.doc.w, self.doc.h) = (w, h);
        self.sel = None;
        self.fit = true;
        self.fx_due = true;
        self.all();
        self.save_due = Some(Instant::now());
    }

    /// Le panneau des calques, puis l'historique (comme le panneau Historique de Photoshop)
    fn side_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        egui::ScrollArea::vertical().id_salt("paint-side").auto_shrink([false, true]).max_height((ui.available_height() - if self.history_open { 170.0 } else { 26.0 }).max(80.0)).show(ui, |ui| self.layers_panel(ui));
        ui.separator();
        let head = egui::RichText::new(format!("{}  {}", if self.history_open { crate::lucide::CHEVRON_DOWN } else { crate::lucide::CHEVRON_RIGHT }, crate::i18n::t("Historique"))).size(12.0);
        if ui.add(egui::Button::new(head).frame(false)).on_hover_text(crate::i18n::t("Cliquer un état pour y revenir")).clicked() {
            self.history_open = !self.history_open;
        }
        if !self.history_open {
            return;
        }
        let mut go: Option<i32> = None;
        egui::ScrollArea::vertical().id_salt("paint-history").max_height(140.0).stick_to_bottom(true).show(ui, |ui| {
            let n = self.undo.len();
            for (k, s) in self.undo.iter().enumerate() {
                if ui.add(egui::Button::new(egui::RichText::new(format!("{}  {}", crate::lucide::HISTORY, s.label)).size(11.5)).frame(false)).clicked() {
                    go = Some(k as i32 - n as i32);
                }
            }
            ui.label(egui::RichText::new(format!("{}  {}", crate::lucide::CIRCLE_DOT, crate::i18n::t("état actuel"))).size(11.5).strong().color(p.accent));
            for (k, s) in self.redo.iter().rev().enumerate() {
                if ui.add(egui::Button::new(egui::RichText::new(format!("{}  {}", crate::lucide::HISTORY, s.label)).size(11.5).color(p.muted)).frame(false)).clicked() {
                    go = Some(k as i32 + 1);
                }
            }
        });
        match go {
            Some(g) if g < 0 => (0..-g).for_each(|_| self.undo()),
            Some(g) => (0..g).for_each(|_| self.redo()),
            None => {}
        }
    }

    fn in_sel(&self, x: i32, y: i32) -> bool {
        self.sel_k(x, y) >= 0.5
    }

    /// Part de la sélection en ce pixel du document (1 sans sélection)
    fn sel_k(&self, x: i32, y: i32) -> f32 {
        match &self.sel {
            None => 1.0,
            Some(s) => s.at(self.doc.w, x, y),
        }
    }

    /// Un tampon du pinceau (ou de la gomme) centré en `c` (coordonnées du document).
    /// Un tampon, et ses reflets si la symétrie est allumée (pinceau et gomme).
    fn dab(&mut self, c: Pos2) {
        let (w, h) = (self.doc.w as f32, self.doc.h as f32);
        let mut at = vec![c];
        if matches!(self.tool, Tool::Brush | Tool::Eraser) {
            if self.sym_x {
                at.push(pos2(w - c.x, c.y));
            }
            if self.sym_y {
                at.push(pos2(c.x, h - c.y));
            }
            if self.sym_x && self.sym_y {
                at.push(pos2(w - c.x, h - c.y));
            }
        }
        // Enroulé : ce qui dépasse d'un bord revient par l'autre
        if self.wrap && matches!(self.tool, Tool::Brush | Tool::Eraser | Tool::Clone) {
            let r = self.size / 2.0 + 1.0;
            let base = at.clone();
            for orig in base {
                let q = pos2(orig.x.rem_euclid(w), orig.y.rem_euclid(h));
                for ox in [-w, 0.0, w] {
                    for oy in [-h, 0.0, h] {
                        let p = q + vec2(ox, oy);
                        if (p - orig).length() > 0.5 && p.x > -r && p.x < w + r && p.y > -r && p.y < h + r {
                            at.push(p);
                        }
                    }
                }
            }
        }
        for q in at {
            self.dab_one(q);
        }
    }

    /// Aimantation de perspective : le point suit la droite du point de fuite le plus proche
    /// de la direction du trait (ou l'horizontale, la verticale).
    fn persp_snap_pt(&self, from: Pos2, d: Pos2) -> Pos2 {
        if !self.persp_on || !self.persp_snap || (d - from).length() < 4.0 {
            return d;
        }
        let dir = (d - from).normalized();
        let mut best: Option<(f32, egui::Vec2)> = None;
        let mut cands: Vec<egui::Vec2> = self.persp.iter().filter(|v| (**v - from).length() > 1.0).map(|v| (*v - from).normalized()).collect();
        // Avec un ou deux points de fuite, les verticales restent verticales
        cands.push(egui::vec2(0.0, 1.0));
        if self.persp.len() < 2 {
            cands.push(egui::vec2(1.0, 0.0));
        }
        for c in cands {
            let s = dir.dot(c).abs();
            if best.is_none_or(|b| s > b.0) {
                best = Some((s, c));
            }
        }
        match best {
            Some((_, c)) => from + c * (d - from).dot(c),
            None => d,
        }
    }

    /// Le calque que touche le trait en cours (le calque, son masque ou celui de son groupe)
    fn stroke_target(&mut self) -> &mut Layer {
        let edit_mask = self.painting_mask();
        match self.group_mask_target() {
            Some(gi) => self.doc.groups[gi].mask.0.as_deref_mut().unwrap(),
            None => {
                let la = &mut self.doc.layers[self.doc.active];
                if edit_mask && la.mask.is_some() {
                    la.mask.as_deref_mut().unwrap()
                } else {
                    la
                }
            }
        }
    }

    /// Doigt : dépose la peinture prise au tampon d'avant, puis en reprend là
    fn smudge_one(&mut self, c: Pos2) {
        let pr = self.pressure.clamp(0.02, 1.0);
        let r = (self.size / 2.0 * if self.press_size { pr } else { 1.0 }).max(1.0);
        let strength = (self.flow * 0.85 * if self.press_flow { pr } else { 1.0 }).clamp(0.0, 0.95);
        let hard = self.hardness.clamp(0.0, 0.99);
        let first = self.smudge_buf.is_empty();
        if first {
            self.smudge_r = (self.size / 2.0).ceil() as i32 + 1;
            let n = (2 * self.smudge_r + 1) as usize;
            self.smudge_buf = vec![[0.0; 4]; n * n];
        }
        let big = self.smudge_r;
        let side = (2 * big + 1) as usize;
        let sel: Vec<f32> = {
            let (cx, cy) = (c.x.floor() as i32, c.y.floor() as i32);
            (-big..=big).flat_map(|dy| (-big..=big).map(move |dx| (dx, dy))).map(|(dx, dy)| self.sel_k(cx + dx, cy + dy)).collect()
        };
        let lock_alpha = !self.painting_mask() && self.doc.layers[self.doc.active].lock_alpha;
        let mut buf = std::mem::take(&mut self.smudge_buf);
        let l = self.stroke_target();
        let (cx, cy) = (c.x.floor() as i32, c.y.floor() as i32);
        for dy in -big..=big {
            for dx in -big..=big {
                let bi = ((dy + big) as usize) * side + (dx + big) as usize;
                let (lx, ly) = (cx + dx - l.off[0], cy + dy - l.off[1]);
                if lx < 0 || ly < 0 || lx as usize >= l.w || ly as usize >= l.h {
                    continue;
                }
                let i = (ly as usize * l.w + lx as usize) * 4;
                let px = [l.px[i] as f32, l.px[i + 1] as f32, l.px[i + 2] as f32, l.px[i + 3] as f32];
                if first {
                    buf[bi] = px;
                    continue;
                }
                let d = (((dx * dx + dy * dy) as f32).sqrt()) / r;
                if d > 1.0 {
                    buf[bi] = px;
                    continue;
                }
                let k0 = if d <= hard { 1.0 } else { 1.0 - (d - hard) / (1.0 - hard) };
                let k = k0 * k0 * (3.0 - 2.0 * k0) * strength * sel[bi];
                let b = buf[bi];
                // Mélange pondéré par la couverture (le vide ne salit pas la couleur)
                let oa = px[3] + (b[3] - px[3]) * k;
                let mut out = [0.0f32; 4];
                for ch in 0..3 {
                    let w0 = px[3] * (1.0 - k);
                    let w1 = b[3] * k;
                    out[ch] = if w0 + w1 > 0.0 { (px[ch] * w0 + b[ch] * w1) / (w0 + w1) } else { px[ch] };
                }
                out[3] = if lock_alpha { px[3] } else { oa };
                for ch in 0..4 {
                    l.px[i + ch] = out[ch].round().clamp(0.0, 255.0) as u8;
                }
                // Le doigt garde une part de sa peinture et prend un peu de la nouvelle
                for ch in 0..4 {
                    buf[bi][ch] = b[ch] * strength + out[ch] * (1.0 - strength);
                }
            }
        }
        self.smudge_buf = buf;
        let (w, h) = (self.doc.w, self.doc.h);
        let clamp = |v: i32, m: usize| v.clamp(0, m as i32) as usize;
        self.touched([clamp(cx - big, w), clamp(cy - big, h), clamp(cx + big + 1, w), clamp(cy + big + 1, h)]);
    }

    /// Fluidité : les pixels du tampon suivent le mouvement depuis le tampon d'avant
    fn liquify_one(&mut self, c: Pos2) {
        let Some(prev) = self.prev_dab.replace(c) else { return };
        let delta = c - prev;
        if delta.length() < 0.01 {
            return;
        }
        let pr = self.pressure.clamp(0.02, 1.0);
        let r = (self.size / 2.0 * if self.press_size { pr } else { 1.0 }).max(1.0);
        let strength = (self.flow * if self.press_flow { pr } else { 1.0 }).clamp(0.0, 1.0);
        let (x0, y0, x1, y1) = ((c.x - r).floor() as i32, (c.y - r).floor() as i32, (c.x + r).ceil() as i32, (c.y + r).ceil() as i32);
        let selk: Vec<f32> = (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| (x, y))).map(|(x, y)| self.sel_k(x, y)).collect();
        let l = self.stroke_target();
        // Copie de la zone (élargie du déplacement) pour lire les pixels d'avant
        let m = delta.length().ceil() as i32 + 2;
        let (bx0, by0) = ((x0 - m - l.off[0]).max(0), (y0 - m - l.off[1]).max(0));
        let (bx1, by1) = ((x1 + m - l.off[0]).min(l.w as i32 - 1), (y1 + m - l.off[1]).min(l.h as i32 - 1));
        if bx1 < bx0 || by1 < by0 {
            return;
        }
        let bw = (bx1 - bx0 + 1) as usize;
        let mut copy = Vec::with_capacity(bw * (by1 - by0 + 1) as usize * 4);
        for y in by0..=by1 {
            let i = (y as usize * l.w + bx0 as usize) * 4;
            copy.extend_from_slice(&l.px[i..i + bw * 4]);
        }
        let bh = (by1 - by0 + 1) as usize;
        let sample = |fx: f32, fy: f32| -> [f32; 4] {
            // Bilinéaire dans la copie (coordonnées du calque)
            let (fx, fy) = ((fx - bx0 as f32 - 0.5).clamp(0.0, bw as f32 - 1.0), (fy - by0 as f32 - 0.5).clamp(0.0, bh as f32 - 1.0));
            let (ix, iy) = (fx.floor() as usize, fy.floor() as usize);
            let (tx, ty) = (fx - ix as f32, fy - iy as f32);
            let (ix1, iy1) = ((ix + 1).min(bw - 1), (iy + 1).min(bh - 1));
            let g = |x: usize, y: usize, ch: usize| copy[(y * bw + x) * 4 + ch] as f32;
            let mut o = [0.0; 4];
            for (ch, v) in o.iter_mut().enumerate() {
                let a = g(ix, iy, ch) + (g(ix1, iy, ch) - g(ix, iy, ch)) * tx;
                let b = g(ix, iy1, ch) + (g(ix1, iy1, ch) - g(ix, iy1, ch)) * tx;
                *v = a + (b - a) * ty;
            }
            o
        };
        let row = (x1 - x0 + 1) as usize;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = ((x as f32 + 0.5 - c.x).powi(2) + (y as f32 + 0.5 - c.y).powi(2)).sqrt() / r;
                if d > 1.0 {
                    continue;
                }
                let k0 = 1.0 - d;
                let k = k0 * k0 * (3.0 - 2.0 * k0) * strength * selk[(y - y0) as usize * row + (x - x0) as usize];
                if k <= 0.0 {
                    continue;
                }
                let (lx, ly) = (x - l.off[0], y - l.off[1]);
                if lx < 0 || ly < 0 || lx as usize >= l.w || ly as usize >= l.h {
                    continue;
                }
                let v = sample(lx as f32 + 0.5 - delta.x * k, ly as f32 + 0.5 - delta.y * k);
                let i = (ly as usize * l.w + lx as usize) * 4;
                for ch in 0..4 {
                    l.px[i + ch] = v[ch].round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        let (w, h) = (self.doc.w, self.doc.h);
        let clamp = |v: i32, m: usize| v.clamp(0, m as i32) as usize;
        self.touched([clamp(x0, w), clamp(y0, h), clamp(x1 + 1, w), clamp(y1 + 1, h)]);
    }

    fn dab_one(&mut self, c: Pos2) {
        match self.tool {
            Tool::Smudge => return self.smudge_one(c),
            Tool::Liquify => {
                if self.prev_dab.is_none() {
                    self.prev_dab = Some(c);
                }
                return self.liquify_one(c);
            }
            _ => {}
        }
        let edit_mask = self.painting_mask();
        let lock_alpha = !edit_mask && self.doc.layers[self.doc.active].lock_alpha;
        let col = self.paint_color();
        let gm = self.group_mask_target();
        let Some((start, cover, _)) = self.stroke.as_mut() else { return };
        let l: &mut Layer = match gm {
            // Le masque d'un groupe
            Some(gi) => self.doc.groups[gi].mask.0.as_deref_mut().unwrap(),
            None => {
                let la = &mut self.doc.layers[self.doc.active];
                match la.mask.as_deref_mut() {
                    Some(m) if edit_mask => m,
                    _ => la,
                }
            }
        };
        // Pression de la tablette : diamètre et/ou opacité
        let pr = self.pressure.clamp(0.02, 1.0);
        let r = self.size / 2.0 * if self.press_size { pr } else { 1.0 };
        let flow = self.flow * if self.press_flow { pr } else { 1.0 };
        let (x0, y0) = ((c.x - r).floor() as i32, (c.y - r).floor() as i32);
        let (x1, y1) = ((c.x + r).ceil() as i32, (c.y + r).ceil() as i32);
        let hard = self.hardness.clamp(0.0, 0.99);
        let grain = if matches!(self.tool, Tool::Brush | Tool::Eraser) { self.grain.clamp(0.0, 1.0) } else { 0.0 };
        let erase = self.tool == Tool::Eraser;
        let heal = self.tool == Tool::Heal;
        if heal {
            self.heal_trail.push(c);
        }
        // Tampon : chaque pixel prend la couleur du calque d'avant le trait, à l'endroit décalé
        let clone_off = (self.tool == Tool::Clone).then_some(self.clone_off).flatten().map(|o| (o.x.round() as i32, o.y.round() as i32));
        let sel = self.sel.as_ref();
        let dw = self.doc.w;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let sk = sel.map(|s| s.at(dw, x, y)).unwrap_or(1.0);
                if sk <= 0.0 {
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
                let mut k = k * k * (3.0 - 2.0 * k) * flow * sk;
                if grain > 0.0 {
                    // Le papier : un bruit fixe dans le document, les creux restent blancs
                    k *= (1.0 - grain * paper(x, y)).max(0.0);
                }
                let idx = ly as usize * l.w + lx as usize;
                if k <= cover[idx] {
                    continue;
                }
                cover[idx] = k;
                if heal {
                    continue;
                }
                let i = idx * 4;
                let s = [start.px[i], start.px[i + 1], start.px[i + 2], start.px[i + 3]];
                if erase && edit_mask {
                    // Gomme dans un masque : on y peint du blanc (on remontre)
                    for ch in 0..3 {
                        l.px[i + ch] = (s[ch] as f32 + (255.0 - s[ch] as f32) * k) as u8;
                    }
                } else if erase {
                    if !lock_alpha {
                        l.px[i + 3] = (s[3] as f32 * (1.0 - k)) as u8;
                    }
                } else if lock_alpha {
                    // Transparence verrouillée : la couleur change, la couverture reste
                    let a = col.a() as f32 / 255.0 * k;
                    let src = [col.r(), col.g(), col.b()];
                    for ch in 0..3 {
                        l.px[i + ch] = (src[ch] as f32 * a + s[ch] as f32 * (1.0 - a)) as u8;
                    }
                } else {
                    // Peinture par-dessus le calque d'avant le trait : pas d'accumulation dans un même trait
                    let (src, src_a) = match clone_off {
                        Some((ox, oy)) => {
                            let (sx, sy) = (lx + ox, ly + oy);
                            if sx < 0 || sy < 0 || sx as usize >= start.w || sy as usize >= start.h {
                                continue;
                            }
                            let j = (sy as usize * start.w + sx as usize) * 4;
                            ([start.px[j], start.px[j + 1], start.px[j + 2]], start.px[j + 3] as f32 / 255.0)
                        }
                        None => ([col.r(), col.g(), col.b()], col.a() as f32 / 255.0),
                    };
                    let a = src_a * k;
                    let sa = s[3] as f32 / 255.0;
                    let oa = a + sa * (1.0 - a);
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
        let gap = self.fill_gap.round() as usize;
        if self.locked() {
            return;
        }
        self.grow_to_doc();
        let l = self.target_ref();
        let (lx, ly) = (at[0] - l.off[0], at[1] - l.off[1]);
        if lx < 0 || ly < 0 || lx as usize >= l.w || ly as usize >= l.h || !self.in_sel(at[0], at[1]) {
            return;
        }
        self.checkpoint();
        let pc = self.paint_color();
        let col = [pc.r(), pc.g(), pc.b(), pc.a()];
        let sel = self.sel.clone();
        let dw = self.doc.w;
        let l = self.target();
        let (w, h) = (l.w, l.h);
        let off = l.off;
        let i0 = (ly as usize * w + lx as usize) * 4;
        let target = [l.px[i0], l.px[i0 + 1], l.px[i0 + 2], l.px[i0 + 3]];
        let near = |p: &[u8]| (0..4).map(|k| (p[k] as f32 - target[k] as f32).abs() / 255.0).fold(0.0f32, f32::max) <= tol;
        // Fermer les trous : le trait est épaissi le temps de trouver la zone, puis la zone revient jusqu'au trait
        if gap > 0 {
            let open: Vec<bool> = (0..w * h).map(|idx| {
                let (dx, dy) = ((idx % w) as i32 + off[0], (idx / w) as i32 + off[1]);
                sel.as_ref().is_none_or(|s| s.at(dw, dx, dy) >= 0.5) && near(&l.px[idx * 4..idx * 4 + 4])
            }).collect();
            if let Some(region) = gap_fill(&open, w, h, lx as usize, ly as usize, gap) {
                for (idx, on) in region.iter().enumerate() {
                    if *on {
                        l.px[idx * 4..idx * 4 + 4].copy_from_slice(&col);
                    }
                }
                self.all();
                return;
            }
        }
        let mut seen = vec![false; w * h];
        let mut stack = vec![(lx as usize, ly as usize)];
        while let Some((x, y)) = stack.pop() {
            let idx = y * w + x;
            if seen[idx] {
                continue;
            }
            seen[idx] = true;
            let (dx, dy) = (x as i32 + off[0], y as i32 + off[1]);
            if sel.as_ref().is_some_and(|s| s.at(dw, dx, dy) < 0.5) || !near(&l.px[idx * 4..idx * 4 + 4]) {
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
        let sel = self.sel.clone();
        let dw = self.doc.w;
        let l = &mut self.doc.layers[self.doc.active];
        for y in 0..l.h {
            for x in 0..l.w {
                let (dx, dy) = (x as i32 + l.off[0], y as i32 + l.off[1]);
                // Un bord de sélection adouci mélange l'ancien et le nouveau
                let sk = sel.as_ref().map(|s| s.at(dw, dx, dy)).unwrap_or(1.0);
                if sk <= 0.0 {
                    continue;
                }
                let i = (y * l.w + x) * 4;
                let old = [base.px[i] as f32 / 255.0, base.px[i + 1] as f32 / 255.0, base.px[i + 2] as f32 / 255.0];
                let c = f(old);
                for k in 0..3 {
                    l.px[i + k] = ((old[k] + (c[k].clamp(0.0, 1.0) - old[k]) * sk) * 255.0 + 0.5) as u8;
                }
            }
        }
        self.all();
    }

    /// La sélection après un geste : remplace, ajoute ou retire, d'après la sélection d'avant le geste
    fn mix_sel(&self, new: Option<Sel>) -> Option<Sel> {
        match (self.sel_base.clone(), new) {
            (Some((Some(base), 1)), Some(n)) => Some(base.combine(&n, false)),
            (Some((Some(base), 2)), Some(n)) => Some(base.combine(&n, true)),
            (Some((Some(base), _)), None) if self.sel_base.as_ref().is_some_and(|b| b.1 != 0) => Some(base),
            (_, n) => n,
        }
    }

    /// Dégradé linéaire de la couleur vers le transparent, de `a` à `b`, dans la sélection
    fn gradient(&mut self, a: Pos2, b: Pos2) {
        let d = b - a;
        let len2 = d.length_sq();
        if len2 < 1.0 {
            return;
        }
        self.checkpoint();
        if self.locked() {
            return;
        }
        self.grow_to_doc();
        let col = self.paint_color();
        let dw = self.doc.w;
        let sel = self.sel.clone();
        let l = self.target();
        for y in 0..l.h {
            for x in 0..l.w {
                let (dx, dy) = (x as i32 + l.off[0], y as i32 + l.off[1]);
                let sk = sel.as_ref().map(|s| s.at(dw, dx, dy)).unwrap_or(1.0);
                if sk <= 0.0 {
                    continue;
                }
                let t = (((dx as f32 + 0.5 - a.x) * d.x + (dy as f32 + 0.5 - a.y) * d.y) / len2).clamp(0.0, 1.0);
                let al = (1.0 - t) * col.a() as f32 / 255.0 * sk;
                let i = (y * l.w + x) * 4;
                let sa = l.px[i + 3] as f32 / 255.0;
                let oa = al + sa * (1.0 - al);
                let src = [col.r(), col.g(), col.b()];
                for ch in 0..3 {
                    l.px[i + ch] = if oa > 0.0 { ((src[ch] as f32 * al + l.px[i + ch] as f32 * sa * (1.0 - al)) / oa) as u8 } else { 0 };
                }
                l.px[i + 3] = (oa * 255.0) as u8;
            }
        }
        self.all();
    }

    /// Filtres du calque actif (dans la sélection) : flou, netteté, bruit, pixelisation
    fn filter(&mut self, kind: &str, amount: f32) {
        self.checkpoint();
        let base = self.doc.layers[self.doc.active].clone();
        let (w, h) = (base.w, base.h);
        let src: Vec<f32> = base.px.iter().map(|v| *v as f32).collect();
        // Flou : trois passes de moyenne glissante horizontale puis verticale, en couleurs prémultipliées
        let blurred = |r: usize| -> Vec<f32> {
            let mut a: Vec<f32> = src.chunks_exact(4).flat_map(|p| [p[0] * p[3] / 255.0, p[1] * p[3] / 255.0, p[2] * p[3] / 255.0, p[3]]).collect();
            let mut tmp = a.clone();
            for _ in 0..3 {
                for y in 0..h {
                    for c in 0..4 {
                        let mut acc = 0.0;
                        let n = (2 * r + 1) as f32;
                        for k in 0..=r.min(w - 1) {
                            acc += a[(y * w + k) * 4 + c];
                        }
                        acc += a[(y * w) * 4 + c] * r as f32;
                        for x in 0..w {
                            tmp[(y * w + x) * 4 + c] = acc / n;
                            let add = (x + r + 1).min(w - 1);
                            let sub = x.saturating_sub(r);
                            acc += a[(y * w + add) * 4 + c] - a[(y * w + sub) * 4 + c];
                        }
                    }
                }
                for x in 0..w {
                    for c in 0..4 {
                        let mut acc = 0.0;
                        let n = (2 * r + 1) as f32;
                        for k in 0..=r.min(h - 1) {
                            acc += tmp[(k * w + x) * 4 + c];
                        }
                        acc += tmp[x * 4 + c] * r as f32;
                        for y in 0..h {
                            a[(y * w + x) * 4 + c] = acc / n;
                            let add = (y + r + 1).min(h - 1);
                            let sub = y.saturating_sub(r);
                            acc += tmp[(add * w + x) * 4 + c] - tmp[(sub * w + x) * 4 + c];
                        }
                    }
                }
            }
            // Retour en couleurs non prémultipliées
            a.chunks_exact(4).flat_map(|p| if p[3] > 0.01 { [p[0] * 255.0 / p[3], p[1] * 255.0 / p[3], p[2] * 255.0 / p[3], p[3]] } else { [0.0; 4] }).collect()
        };
        let out: Vec<f32> = match kind {
            "blur" => blurred(((amount / 2.0).round() as usize).max(1)),
            "sharpen" => {
                let b = blurred(1);
                src.iter().zip(b.iter()).enumerate().map(|(i, (s, b))| if i % 4 == 3 { *s } else { s + (s - b) * amount }).collect()
            }
            "noise" => {
                let mut seed = 0x2545F4914F6CDD1Du64;
                src.chunks_exact(4)
                    .flat_map(|p| {
                        seed ^= seed << 13;
                        seed ^= seed >> 7;
                        seed ^= seed << 17;
                        let n = ((seed >> 40) as f32 / (1u64 << 24) as f32 - 0.5) * amount * 255.0;
                        [p[0] + n, p[1] + n, p[2] + n, p[3]]
                    })
                    .collect()
            }
            "pixelate" => {
                let k = (amount as usize).max(2);
                let mut o = src.clone();
                for by in (0..h).step_by(k) {
                    for bx in (0..w).step_by(k) {
                        let (mut acc, mut n) = ([0.0f32; 4], 0.0);
                        for y in by..(by + k).min(h) {
                            for x in bx..(bx + k).min(w) {
                                for c in 0..4 {
                                    acc[c] += src[(y * w + x) * 4 + c];
                                }
                                n += 1.0;
                            }
                        }
                        for y in by..(by + k).min(h) {
                            for x in bx..(bx + k).min(w) {
                                for c in 0..4 {
                                    o[(y * w + x) * 4 + c] = acc[c] / n;
                                }
                            }
                        }
                    }
                }
                o
            }
            _ => src.clone(),
        };
        let sel = self.sel.clone();
        let dw = self.doc.w;
        let l = &mut self.doc.layers[self.doc.active];
        for y in 0..h {
            for x in 0..w {
                let sk = sel.as_ref().map(|s| s.at(dw, x as i32 + l.off[0], y as i32 + l.off[1])).unwrap_or(1.0);
                if sk <= 0.0 {
                    continue;
                }
                let i = (y * w + x) * 4;
                for c in 0..4 {
                    l.px[i + c] = (src[i + c] + (out[i + c] - src[i + c]) * sk).round().clamp(0.0, 255.0) as u8;
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
        let mut d = rfd::FileDialog::new().set_title(crate::i18n::t("exporter l'image")).set_file_name(format!("{stem}.png")).add_filter("PNG", &["png"]).add_filter("JPEG", &["jpg", "jpeg"]).add_filter("WebP", &["webp"]).add_filter("Photoshop (calques)", &["psd"]);
        if let Some(dir) = self.path.parent() {
            if !dir.to_string_lossy().contains(".app/untitled") {
                d = d.set_directory(dir);
            }
        }
        let Some(mut out) = d.save_file() else { return };
        if out.extension().is_none() {
            out.set_extension("png");
        }
        let ext = out.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if ext == "psd" {
            // Photoshop : les calques, masques et groupes partent tels quels
            let (bytes, lost) = crate::psd::write(&self.doc);
            self.message = Some(match std::fs::write(&out, bytes) {
                Ok(()) if lost.is_empty() => format!("{} {}", crate::i18n::t("exporté :"), out.to_string_lossy()),
                Ok(()) => format!("{} {} · {} {}", crate::i18n::t("exporté :"), out.to_string_lossy(), crate::i18n::t("pas encore transmis :"), lost.join(", ")),
                Err(e) => e.to_string(),
            });
            return;
        }
        let img = self.doc.flatten();
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

    /// Reçoit une image envoyée d'un autre outil : un nouveau calque
    pub fn receive(&mut self, f: &std::path::Path) {
        self.add_image_file(f);
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
        // Elle passe à la ligne avant le panneau des calques (sinon elle l'écarte hors de la fenêtre)
        let docked = crate::layout::offer("paint-layers", "calques (retouche)").is_some();
        let bar_w = (ui.available_width() - if docked { 0.0 } else { self.panel_w + 12.0 }).max(200.0);
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(bar_w);
            ui.spacing_mut().item_spacing.x = 4.0;
            egui::containers::menu::MenuButton::new(crate::i18n::t("image ▾")).ui(ui, |ui| {
                ui.set_min_width(230.0);
                if ui.button(format!("{}  {}", crate::lucide::SCALE_3D_ICON, crate::i18n::t("transformation libre (Ctrl+T)"))).clicked() {
                    self.start_xf();
                    ui.close();
                }
                if ui.button(format!("{}  {}", crate::lucide::GRID_3X3, crate::i18n::t("déformation en grille"))).on_hover_text(crate::i18n::t("Tirer les points d'une grille pour courber le calque (tissus, reflets, étiquettes sur un objet)")).clicked() {
                    self.start_warp();
                    ui.close();
                }
                if ui.button(format!("{}  {}", crate::lucide::SUN, crate::i18n::t("luminosité et contraste…"))).clicked() {
                    self.checkpoint();
                    self.adjust = Some((Adjust::BrightContrast, self.doc.layers[self.doc.active].clone(), [0.0, 0.0]));
                    ui.close();
                }
                if ui.button(format!("{}  {}", crate::lucide::PALETTE, crate::i18n::t("teinte et saturation…"))).clicked() {
                    self.checkpoint();
                    self.adjust = Some((Adjust::HueSat, self.doc.layers[self.doc.active].clone(), [0.0, 0.0]));
                    ui.close();
                }
                if ui.button(format!("{}  {}", crate::lucide::BLEND, crate::i18n::t("inverser les couleurs   Ctrl+I"))).clicked() {
                    self.checkpoint();
                    let base = self.doc.layers[self.doc.active].clone();
                    self.map_active(&base, |c| c.map(|x| 1.0 - x));
                    ui.close();
                }
                if ui.button(format!("{}  {}", crate::lucide::CONTRAST, crate::i18n::t("noir et blanc"))).clicked() {
                    self.checkpoint();
                    let base = self.doc.layers[self.doc.active].clone();
                    self.map_active(&base, |c| [c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722; 3]);
                    ui.close();
                }
                ui.separator();
                if ui.button(format!("{}  {}", crate::lucide::DOWNLOAD, crate::i18n::t("exporter en PNG, JPG, WebP ou PSD…"))).clicked() {
                    self.export();
                    ui.close();
                }
                ui.separator();
                ui.label(egui::RichText::new(crate::i18n::t("envoyer l'image vers")).weak().size(11.0));
                for (id, label, icon) in crate::sendto::TARGETS {
                    if id != "retouche" && ui.button(format!("{icon}  {}", crate::i18n::t(label))).clicked() {
                        let name = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "image".into());
                        let root = crate::nodes::vault_root();
                        self.message = Some(match crate::sendto::stage(&root, &name, &self.doc.flatten()) {
                            Ok(rel) => {
                                crate::sendto::push(id, rel);
                                crate::i18n::t("Envoyé.")
                            }
                            Err(e) => e,
                        });
                        ui.close();
                    }
                }
            });
            egui::containers::menu::MenuButton::new(crate::i18n::t("sélection ▾")).ui(ui, |ui| {
                ui.set_min_width(230.0);
                if ui.button(format!("{}  {}", crate::lucide::CHECK_CHECK, crate::i18n::t("tout sélectionner   Ctrl+A"))).clicked() {
                    self.sel = Some(Sel::all(self.doc.w, self.doc.h));
                    ui.close();
                }
                if ui.add_enabled(self.sel.is_some(), egui::Button::new(crate::i18n::t("inverser la sélection   Ctrl+Maj+I"))).clicked() {
                    self.sel = self.sel.as_ref().map(|s| s.invert(self.doc.w, self.doc.h));
                    ui.close();
                }
                if ui.add_enabled(self.sel.is_some(), egui::Button::new(crate::i18n::t("désélectionner   Ctrl+D"))).clicked() {
                    self.sel = None;
                    ui.close();
                }
                ui.separator();
                ui.label(egui::RichText::new(crate::i18n::t("Maj : ajouter · Alt : retirer")).size(11.0).color(p.muted));
            });
            egui::containers::menu::MenuButton::new(crate::i18n::t("filtres ▾")).ui(ui, |ui| {
                ui.set_min_width(230.0);
                for (label, kind, amount) in [("flou léger", "blur", 4.0), ("flou fort", "blur", 16.0), ("netteté", "sharpen", 0.8), ("bruit", "noise", 0.12), ("pixeliser", "pixelate", 12.0)] {
                    if ui.button(crate::i18n::t(label)).clicked() {
                        self.filter(kind, amount);
                        ui.close();
                    }
                }
            });
            egui::containers::menu::MenuButton::new(crate::i18n::t("calque ▾")).ui(ui, |ui| {
                ui.set_min_width(230.0);
                if self.doc.anim.is_none() && ui.button(format!("{}  {}", crate::lucide::FILM, crate::i18n::t("animation image par image"))).on_hover_text(crate::i18n::t("Les calques d'un groupe « Animation » deviennent les images : pelure d'oignon, lecture, export GIF ou MP4")).clicked() {
                    self.anim_start();
                    ui.close();
                }
                if ui.button(format!("{}  {}", crate::lucide::PLUS, crate::i18n::t("nouveau calque   Ctrl+Maj+N"))).clicked() {
                    self.new_layer();
                    ui.close();
                }
                if ui.button(format!("{}  {}", crate::lucide::IMAGE_PLUS, crate::i18n::t("calque depuis une image…"))).clicked() {
                    self.add_image_layer();
                    ui.close();
                }
                if ui.button(format!("{}  {}", crate::lucide::COPY, crate::i18n::t("dupliquer le calque   Ctrl+J"))).clicked() {
                    self.duplicate_layer();
                    ui.close();
                }
                if ui.button(format!("{}  {}", crate::lucide::MERGE, crate::i18n::t("fusionner avec le dessous   Ctrl+E"))).clicked() {
                    self.merge_down();
                    ui.close();
                }
                if ui.button(format!("{}  {}", crate::lucide::TRASH_2, crate::i18n::t("supprimer le calque"))).clicked() {
                    self.delete_layer();
                    ui.close();
                }
            });
            ui.separator();
            // Transformation libre : ses valeurs à la place des réglages de l'outil
            if let Some(x) = self.xf.as_ref().filter(|x| x.grid.is_some()) {
                let _ = x;
                ui.label(egui::RichText::new(format!("{}  {}", crate::lucide::GRID_3X3, crate::i18n::t("déformation en grille"))).size(12.5));
                if ui.button(egui::RichText::new(crate::lucide::ROTATE_CCW).size(14.0)).on_hover_text(crate::i18n::t("remettre la grille droite")).clicked() {
                    let orig = self.xf.as_ref().map(|x| x.orig.clone());
                    if let (Some(o), Some(xx)) = (orig, self.xf.as_mut()) {
                        let n = WARP_N;
                        let (x0, y0, w, h) = (xx.c0[0] - xx.half[0], xx.c0[1] - xx.half[1], xx.half[0] * 2.0, xx.half[1] * 2.0);
                        xx.grid = Some((0..n * n).map(|k| [x0 + w * (k % n) as f32 / (n - 1) as f32, y0 + h * (k / n) as f32 / (n - 1) as f32]).collect());
                        self.doc.layers[self.doc.active] = o;
                        self.all();
                    }
                }
                if ui.button(egui::RichText::new(crate::lucide::CHECK).size(14.0).color(p.accent)).on_hover_text(crate::i18n::t("valider (Entrée)")).clicked() {
                    self.end_xf(true);
                }
                if ui.button(egui::RichText::new(crate::lucide::X).size(14.0)).on_hover_text(crate::i18n::t("annuler (Échap)")).clicked() {
                    self.end_xf(false);
                }
                return;
            }
            if let Some(mut x) = self.xf.clone() {
                ui.label(egui::RichText::new(format!("{}  {}", crate::lucide::SCALE_3D_ICON, crate::i18n::t("transformation"))).size(12.5));
                let mut w = x.s[0] * 100.0;
                let mut h = x.s[1] * 100.0;
                let mut a = x.r.to_degrees();
                let mut ch = false;
                ch |= ui.add(egui::DragValue::new(&mut w).speed(0.5).max_decimals(1).suffix(" %").prefix("L ")).on_hover_text(crate::i18n::t("largeur")).changed();
                ch |= ui.add(egui::DragValue::new(&mut h).speed(0.5).max_decimals(1).suffix(" %").prefix("H ")).on_hover_text(crate::i18n::t("hauteur")).changed();
                ch |= ui.add(egui::DragValue::new(&mut a).speed(0.5).max_decimals(1).suffix("°").prefix(format!("{} ", crate::lucide::ROTATE_CW))).on_hover_text(crate::i18n::t("angle (Maj : par 15°)")).changed();
                if ui.button(egui::RichText::new(crate::lucide::FLIP_HORIZONTAL_2).size(14.0)).on_hover_text(crate::i18n::t("retourner horizontalement")).clicked() {
                    w = -w;
                    ch = true;
                }
                if ui.button(egui::RichText::new(crate::lucide::FLIP_VERTICAL_2).size(14.0)).on_hover_text(crate::i18n::t("retourner verticalement")).clicked() {
                    h = -h;
                    ch = true;
                }
                if ch {
                    x.s = [w / 100.0, h / 100.0];
                    x.r = a.to_radians();
                    self.xf = Some(x);
                    self.apply_xf();
                }
                if ui.button(egui::RichText::new(crate::lucide::CHECK).size(14.0).color(p.accent)).on_hover_text(crate::i18n::t("valider (Entrée)")).clicked() {
                    self.end_xf(true);
                }
                if ui.button(egui::RichText::new(crate::lucide::X).size(14.0)).on_hover_text(crate::i18n::t("annuler (Échap)")).clicked() {
                    self.end_xf(false);
                }
                return;
            }
            // Le nom de l'outil en cours, puis ses réglages (les outils sont dans la colonne de gauche)
            if let Some((_, icon, label, _)) = TOOLS.iter().find(|t| t.0 == self.tool) {
                ui.label(egui::RichText::new(format!("{icon}  {}", crate::i18n::t(label))).size(12.5));
            }
            match self.tool {
                Tool::Brush | Tool::Eraser | Tool::Clone | Tool::Heal | Tool::Smudge | Tool::Liquify => {
                    ui.add(egui::DragValue::new(&mut self.size).range(0.1..=10000.0).speed(0.5).prefix(format!("{} ", crate::i18n::t("diamètre")))).on_hover_text("[ ]");
                    // Un curseur avec son nom ne passe pas à la ligne tout seul
                    if ui.max_rect().right() - ui.cursor().left() < 190.0 {
                        ui.end_row();
                    }
                    ui.add(egui::Slider::new(&mut self.hardness, 0.0..=1.0).text(crate::i18n::t("dureté")).show_value(false));
                    if ui.max_rect().right() - ui.cursor().left() < 190.0 {
                        ui.end_row();
                    }
                    ui.add(egui::Slider::new(&mut self.flow, 0.05..=1.0).text(crate::i18n::t("opacité")).show_value(false));
                    if ui.max_rect().right() - ui.cursor().left() < 190.0 {
                        ui.end_row();
                    }
                    ui.add(egui::Slider::new(&mut self.smooth, 0.0..=1.0).text(crate::i18n::t("lissage")).show_value(false)).on_hover_text(crate::i18n::t("Stabilisateur : le trait suit la souris avec du retard, pour des lignes propres"));
                    let tog = |ui: &mut egui::Ui, on: &mut bool, icon: &str, tip: &str| {
                        if ui.add(egui::Button::new(egui::RichText::new(icon).size(14.0)).selected(*on).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t(tip)).clicked() {
                            *on = !*on;
                        }
                    };
                    tog(ui, &mut self.press_size, crate::lucide::CIRCLE_DOT, "La pression de la tablette règle le diamètre");
                    tog(ui, &mut self.press_flow, crate::lucide::GAUGE, "La pression de la tablette règle l'opacité");
                    if matches!(self.tool, Tool::Brush | Tool::Eraser) {
                        if ui.max_rect().right() - ui.cursor().left() < 190.0 {
                            ui.end_row();
                        }
                        ui.add(egui::Slider::new(&mut self.grain, 0.0..=1.0).text(crate::i18n::t("grain")).show_value(false)).on_hover_text(crate::i18n::t("Grain du papier : le pinceau accroche comme un crayon ou une craie"));
                        // Préréglages façon Krita
                        egui::containers::menu::MenuButton::new(egui::RichText::new(crate::lucide::BRUSH).size(14.0)).ui(ui, |ui| {
                            ui.set_min_width(200.0);
                            for (name, size, hard, flow, grain, psize, pflow, smooth) in BRUSH_PRESETS {
                                if ui.button(crate::i18n::t(name)).clicked() {
                                    (self.size, self.hardness, self.flow, self.grain, self.press_size, self.press_flow, self.smooth) = (size, hard, flow, grain, psize, pflow, smooth);
                                    self.tool = Tool::Brush;
                                    ui.close();
                                }
                            }
                        })
                        .0
                        .on_hover_text(crate::i18n::t("Pinceaux prêts : crayon, encre, craie, aérographe…"));
                        tog(ui, &mut self.sym_x, crate::lucide::FLIP_HORIZONTAL_2, "Symétrie : le trait se reflète de l'autre côté de l'axe vertical");
                        tog(ui, &mut self.sym_y, crate::lucide::FLIP_VERTICAL_2, "Symétrie : le trait se reflète de l'autre côté de l'axe horizontal");
                        tog(ui, &mut self.wrap, crate::lucide::REPEAT, "Enroulé : l'image se répète autour, le trait passe d'un bord à l'autre (textures qui se raccordent)");
                    }
                    let was = self.persp_on;
                    tog(ui, &mut self.persp_on, crate::lucide::RULER_DIMENSION_LINE, "Perspective : points de fuite (à déplacer à la souris) ; le trait suit leurs lignes");
                    if self.persp_on && !was && self.persp.is_empty() {
                        let (w, h) = (self.doc.w as f32, self.doc.h as f32);
                        self.persp = vec![pos2(w * 0.1, h * 0.45), pos2(w * 0.9, h * 0.45)];
                    }
                    if self.persp_on {
                        egui::ComboBox::from_id_salt("paint-persp").selected_text(format!("{} pt", self.persp.len())).width(48.0).show_ui(ui, |ui| {
                            let (w, h) = (self.doc.w as f32, self.doc.h as f32);
                            for (n, pts) in [(1usize, vec![pos2(w * 0.5, h * 0.45)]), (2, vec![pos2(w * 0.1, h * 0.45), pos2(w * 0.9, h * 0.45)]), (3, vec![pos2(w * 0.1, h * 0.4), pos2(w * 0.9, h * 0.4), pos2(w * 0.5, h * 1.6)])] {
                                if ui.selectable_label(self.persp.len() == n, format!("{n} {}", crate::i18n::t_mid("point(s) de fuite"))).clicked() {
                                    self.persp = pts;
                                }
                            }
                        });
                        tog(ui, &mut self.persp_snap, crate::lucide::MAGNET, "Aimanter le trait sur les lignes de fuite");
                    }
                }
                Tool::Crop => {
                    egui::ComboBox::from_id_salt("paint-crop-ratio").selected_text(crate::i18n::t(CROP_RATIOS[self.crop_ratio].0)).show_ui(ui, |ui| {
                        for (k, (name, _)) in CROP_RATIOS.iter().enumerate() {
                            ui.selectable_value(&mut self.crop_ratio, k, crate::i18n::t(name));
                        }
                    });
                    if self.crop.is_some() {
                        if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::CHECK).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Appliquer le recadrage (Entrée)")).clicked() {
                            self.apply_crop();
                        }
                        if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::X).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Annuler (Échap)")).clicked() {
                            self.crop = None;
                        }
                    } else {
                        ui.label(egui::RichText::new(crate::i18n::t("glisser pour tracer le cadre · Entrée : appliquer")).size(11.0).color(p.muted));
                    }
                }
                Tool::Fill | Tool::Wand => {
                    if ui.max_rect().right() - ui.cursor().left() < 220.0 {
                        ui.end_row();
                    }
                    ui.add(egui::Slider::new(&mut self.tolerance, 0.0..=1.0).text(crate::i18n::t("tolérance")));
                    if self.tool == Tool::Fill {
                        ui.add(egui::Slider::new(&mut self.fill_gap, 0.0..=24.0).step_by(1.0).text(crate::i18n::t("fermer les trous (px)"))).on_hover_text(crate::i18n::t("Le remplissage ne passe pas par les ouvertures du trait plus petites que cette taille, et va jusqu'au trait"));
                    }
                }
                Tool::Gradient => {
                    ui.label(egui::RichText::new(crate::i18n::t("glisser : de la couleur vers le transparent")).size(11.0).color(p.muted));
                }
                Tool::Select | Tool::Ellipse | Tool::Lasso => {
                    ui.label(egui::RichText::new(crate::i18n::t("Maj : ajouter · Alt : retirer")).size(11.0).color(p.muted));
                }
                Tool::Text => {
                    let fonts = crate::motion::FONTS;
                    egui::ComboBox::from_id_salt("paint-font").selected_text(crate::i18n::t(fonts[self.text_font.min(fonts.len() - 1)].1)).show_ui(ui, |ui| {
                        for (k, (_, name)) in fonts.iter().enumerate() {
                            ui.selectable_value(&mut self.text_font, k, crate::i18n::t(name));
                        }
                    });
                    ui.add(egui::DragValue::new(&mut self.text_size).range(4.0..=2000.0).speed(0.5).suffix(" px")).on_hover_text(crate::i18n::t("taille du texte"));
                }
                _ => {}
            }
            if self.sel.is_some() && ui.button(crate::i18n::t("désélectionner (Ctrl+D)")).clicked() {
                self.sel = None;
            }
            ui.separator();

        });
        if let Some(m) = &self.message {
            ui.label(egui::RichText::new(m).size(11.0).color(p.muted));
        }
        // ---- Bande de l'animation, en bas
        if self.doc.anim.is_some() {
            self.anim_panel(ui);
        }
        // ---- Colonne des outils à gauche, comme Photoshop, avec les couleurs en bas
        egui::Panel::left(egui::Id::unique(("paint-tools", self.path.to_string_lossy().to_string()))).resizable(false).exact_size(44.0).frame(egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(4, 8))).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for (k, (t, icon, label, key)) in TOOLS.into_iter().enumerate() {
                    // Petits séparateurs entre les familles : déplacer, sélections, peinture, texte et pipette
                    if [1, 5, 6, 12].contains(&k) {
                        ui.add_space(4.0);
                        let (r, _) = ui.allocate_exact_size(vec2(26.0, 1.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, 0.0, p.line);
                        ui.add_space(4.0);
                    }
                    let on = self.tool == t;
                    let b = egui::Button::new(egui::RichText::new(icon).size(15.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { Color32::TRANSPARENT }).min_size(vec2(32.0, 28.0));
                    if ui.add(b).on_hover_text(format!("{} ({key})", crate::i18n::t(label))).clicked() {
                        self.tool = t;
                    }
                }
                ui.add_space(12.0);
                // Premier plan et arrière-plan, l'un sur l'autre
                let (r, _) = ui.allocate_exact_size(vec2(36.0, 36.0), egui::Sense::hover());
                let back = egui::Rect::from_min_size(r.min + vec2(12.0, 12.0), vec2(22.0, 22.0));
                let front = egui::Rect::from_min_size(r.min + vec2(2.0, 2.0), vec2(22.0, 22.0));
                for (rect, which) in [(back, true), (front, false)] {
                    let col = if which { self.bg_color } else { self.color };
                    ui.painter().rect_filled(rect, 0.0, col);
                    ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_black_alpha(160)), egui::StrokeKind::Inside);
                    ui.painter().rect_stroke(rect.expand(1.0), 0.0, Stroke::new(1.0, Color32::from_white_alpha(120)), egui::StrokeKind::Inside);
                    let resp = ui.interact(rect, ui.scope_id().with(("swatch", which)), egui::Sense::click());
                    if resp.clicked() {
                        self.swatch_pick = if self.swatch_pick == Some(which) { None } else { Some((which, rect.right_top())) }.map(|x| x.0);
                        self.swatch_at = rect.right_top();
                    }
                    resp.on_hover_text(crate::i18n::t(if which { "Couleur d'arrière-plan" } else { "Couleur de premier plan" }));
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    if ui.add(egui::Button::new(egui::RichText::new("⇄").size(11.0)).frame(false)).on_hover_text(crate::i18n::t("Échanger les couleurs (X)")).clicked() {
                        std::mem::swap(&mut self.color, &mut self.bg_color);
                    }
                    if ui.add(egui::Button::new(egui::RichText::new("◩").size(11.0)).frame(false)).on_hover_text(crate::i18n::t("Noir et blanc par défaut (D)")).clicked() {
                        (self.color, self.bg_color) = (Color32::BLACK, Color32::WHITE);
                    }
                });
            });
        });
        // Le sélecteur de la couleur cliquée, à droite de la colonne ; un clic ailleurs le ferme
        if let Some(which) = self.swatch_pick {
            let r = egui::Area::new(egui::Id::unique("paint-swatch-pop")).order(egui::Order::Foreground).fixed_pos(self.swatch_at + vec2(8.0, -10.0)).show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let c = if which { &mut self.bg_color } else { &mut self.color };
                    egui::color_picker::color_picker_color32(ui, c, egui::color_picker::Alpha::OnlyBlend);
                });
            });
            if r.response.clicked_elsewhere() && !ctx.input(|i| i.pointer.interact_pos().is_some_and(|p| (p - self.swatch_at).length() < 30.0)) {
                self.swatch_pick = None;
                // Ce clic ne fait que fermer : il ne peint pas sur l'image
                self.swallow_click = true;
            }
        }

        // ---- Calques, à droite
        match crate::layout::offer("paint-layers", "calques (retouche)") {
            Some(r) => crate::layout::draw_in(ui, r, p.panel, 8.0, |ui| self.side_panel(ui)),
            None => {
                egui::Panel::right(egui::Id::unique(("paint-layers", self.path.to_string_lossy().to_string()))).resizable(true).max_size(480.0).default_size(230.0).min_size(180.0).frame(egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).inner_margin(8)).show(ui, |ui| {
                    self.panel_w = ui.max_rect().width() + 16.0;
                    self.side_panel(ui)
                });
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
                    if ui.button(format!("{}  {}", crate::lucide::CHECK, crate::i18n::t("appliquer"))).clicked() {
                        close = Some(true);
                    }
                    if ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("annuler"))).clicked() {
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
        // Vue tournée et/ou en miroir autour du centre de l'image
        let (cs, sn) = (self.view_rot.cos(), self.view_rot.sin());
        let flip = self.view_flip;
        let turn = self.view_rot.rem_euclid(std::f32::consts::TAU);
        let plain = (turn < 1e-4 || std::f32::consts::TAU - turn < 1e-4) && !flip;
        let center = canvas.center();
        let (hw, hh) = (self.doc.w as f32 / 2.0, self.doc.h as f32 / 2.0);
        let to_doc = move |q: Pos2| {
            let (dx, dy) = (q.x - center.x, q.y - center.y);
            let mut vx = dx * cs + dy * sn;
            let vy = -dx * sn + dy * cs;
            if flip {
                vx = -vx;
            }
            pos2(vx / zoom + hw, vy / zoom + hh)
        };
        let to_screen = move |x: f32, y: f32| {
            let mut vx = (x - hw) * zoom;
            let vy = (y - hh) * zoom;
            if flip {
                vx = -vx;
            }
            pos2(center.x + vx * cs - vy * sn, center.y + vx * sn + vy * cs)
        };
        // Damier sous la transparence (vue tournée : des cases tournées, limitées à l'écran)
        if !plain {
            let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()].map(to_doc);
            let (mut a, mut b) = (pos2(f32::MAX, f32::MAX), pos2(f32::MIN, f32::MIN));
            for c in corners {
                a = a.min(c);
                b = b.max(c);
            }
            let step = 12.0 / zoom;
            let (x0, y0) = ((a.x.max(0.0) / step).floor() as i32, (a.y.max(0.0) / step).floor() as i32);
            let (x1, y1) = ((b.x.min(self.doc.w as f32) / step).ceil() as i32, (b.y.min(self.doc.h as f32) / step).ceil() as i32);
            let mut mesh = egui::Mesh::default();
            let quad = |mesh: &mut egui::Mesh, a: Pos2, b: Pos2, col: Color32| {
                let i = mesh.vertices.len() as u32;
                for (x, y) in [(a.x, a.y), (b.x, a.y), (b.x, b.y), (a.x, b.y)] {
                    mesh.colored_vertex(to_screen(x, y), col);
                }
                mesh.add_triangle(i, i + 1, i + 2);
                mesh.add_triangle(i, i + 2, i + 3);
            };
            quad(&mut mesh, pos2(0.0, 0.0), pos2(self.doc.w as f32, self.doc.h as f32), Color32::from_gray(245));
            if (x1 - x0) as i64 * (y1 - y0) as i64 <= 40_000 {
                for cy in y0.max(0)..y1 {
                    for cx in x0.max(0)..x1 {
                        if (cx + cy) % 2 == 0 {
                            let a = pos2(cx as f32 * step, cy as f32 * step);
                            let b = pos2(((cx + 1) as f32 * step).min(self.doc.w as f32), ((cy + 1) as f32 * step).min(self.doc.h as f32));
                            quad(&mut mesh, a, b, Color32::from_gray(205));
                        }
                    }
                }
            }
            painter.add(egui::Shape::mesh(mesh));
        }
        let cell = 12.0;
        let clip = if plain { canvas.intersect(rect) } else { Rect::NOTHING };
        let mut y = clip.top();
        let mut row = ((clip.top() - canvas.top()) / cell) as i32;
        while plain && y < clip.bottom() {
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
        // Styles : leurs couvertures sont recalculées quand un calque change (pas pendant un trait)
        if self.fx_due && self.stroke.is_none() && self.drag_from.is_none() {
            self.fx_due = false;
            let (w, h) = (self.doc.w, self.doc.h);
            let mut any = false;
            for l in &mut self.doc.layers {
                if l.fx.needs_maps() {
                    l.bake_fx(w, h);
                    any = true;
                } else {
                    l.fx_maps = None;
                }
            }
            if any {
                self.dirty = Some([0, 0, w, h]);
            }
        }
        // Recomposer ce qui a changé, puis envoyer à la carte graphique
        if let Some(d) = self.dirty.take() {
            let d = [d[0], d[1], d[2].min(self.doc.w), d[3].min(self.doc.h)];
            if self.comp.len() != self.doc.w * self.doc.h * 4 {
                self.comp = vec![0; self.doc.w * self.doc.h * 4];
            }
            self.doc.composite(&mut self.comp, d);
            // Pelure d'oignon : l'image d'avant en rouge, celle d'après en vert, en transparence
            if let (Some(a), None) = (&self.doc.anim, self.anim_play) {
                if a.onion {
                    let frames = self.doc.frames();
                    if let Some(k) = frames.iter().position(|&i| i == self.doc.active) {
                        let near: Vec<(usize, [f32; 3])> = [(k.checked_sub(1), [235.0, 70.0, 70.0]), (frames.get(k + 1).map(|_| k + 1), [70.0, 190.0, 90.0])].into_iter().filter_map(|(f, c)| f.map(|f| (frames[f], c))).collect();
                        for (li, tint) in near {
                            let l = &self.doc.layers[li];
                            for y in d[1]..d[3].min(self.doc.h) {
                                for x in d[0]..d[2].min(self.doc.w) {
                                    let s = l.at(x as i32, y as i32);
                                    if s[3] == 0 {
                                        continue;
                                    }
                                    let a = s[3] as f32 / 255.0 * 0.28;
                                    let i = (y * self.doc.w + x) * 4;
                                    for ch in 0..3 {
                                        self.comp[i + ch] = (self.comp[i + ch] as f32 * (1.0 - a) + tint[ch] * a) as u8;
                                    }
                                    self.comp[i + 3] = self.comp[i + 3].max((a * 255.0) as u8);
                                }
                            }
                        }
                    }
                }
            }
            // Alt+clic sur la vignette du masque : on voit le masque seul, en gris
            if self.mask_view {
                let l = &self.doc.layers[self.doc.active];
                if l.mask.is_some() {
                    for y in d[1]..d[3].min(self.doc.h) {
                        for x in d[0]..d[2].min(self.doc.w) {
                            let v = match &l.mask {
                                Some(m) => m.at(x as i32, y as i32)[0],
                                _ => 255,
                            };
                            let i = (y * self.doc.w + x) * 4;
                            self.comp[i..i + 4].copy_from_slice(&[v, v, v, 255]);
                        }
                    }
                }
            }
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
        let (dw, dh) = (self.doc.w as f32, self.doc.h as f32);
        // Enroulé : les copies autour, un peu assombries
        if let (true, true, Some(t)) = (self.wrap, plain, &self.tex) {
            let clip = painter.with_clip_rect(rect);
            for ox in [-1.0f32, 0.0, 1.0] {
                for oy in [-1.0f32, 0.0, 1.0] {
                    if ox != 0.0 || oy != 0.0 {
                        let r = canvas.translate(vec2(ox * canvas.width(), oy * canvas.height()));
                        clip.image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::from_gray(190));
                    }
                }
            }
        }
        if let Some(t) = &self.tex {
            if plain {
                painter.image(t.id(), canvas, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            } else {
                let mut mesh = egui::Mesh::with_texture(t.id());
                for (x, y, u, v) in [(0.0, 0.0, 0.0, 0.0), (dw, 0.0, 1.0, 0.0), (dw, dh, 1.0, 1.0), (0.0, dh, 0.0, 1.0)] {
                    mesh.vertices.push(egui::epaint::Vertex { pos: to_screen(x, y), uv: pos2(u, v), color: Color32::WHITE });
                }
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(0, 2, 3);
                painter.add(egui::Shape::mesh(mesh));
            }
        }
        // Avant / après : l'image d'ouverture à gauche de la séparation
        if let Some(f) = self.split {
            let same = self.before.as_ref().is_some_and(|b| b.0.size == [self.doc.w, self.doc.h]);
            if same {
                let bt = self.before.as_mut().unwrap();
                let tex = bt.1.get_or_insert_with(|| ctx.load_texture("paint-before", bt.0.clone(), egui::TextureOptions::NEAREST)).clone();
                let sx = rect.left() + rect.width() * f;
                let clip = painter.with_clip_rect(Rect::from_min_max(rect.min, pos2(sx, rect.bottom())));
                let mut mesh = egui::Mesh::with_texture(tex.id());
                for (x, y, u, v) in [(0.0, 0.0, 0.0, 0.0), (dw, 0.0, 1.0, 0.0), (dw, dh, 1.0, 1.0), (0.0, dh, 0.0, 1.0)] {
                    mesh.vertices.push(egui::epaint::Vertex { pos: to_screen(x, y), uv: pos2(u, v), color: Color32::WHITE });
                }
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(0, 2, 3);
                clip.add(egui::Shape::mesh(mesh));
                // La séparation, qu'on glisse
                let handle = Rect::from_center_size(pos2(sx, rect.center().y), vec2(14.0, rect.height()));
                let hr = ui.interact(handle, ui.scope_id().with("split-handle"), egui::Sense::drag());
                if hr.dragged() {
                    if let Some(q) = hr.interact_pointer_pos() {
                        self.split = Some(((q.x - rect.left()) / rect.width()).clamp(0.02, 0.98));
                    }
                }
                if hr.hovered() || hr.dragged() {
                    ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
                painter.line_segment([pos2(sx, rect.top()), pos2(sx, rect.bottom())], Stroke::new(2.0, Color32::WHITE));
                painter.circle(pos2(sx, rect.center().y), 9.0, p.panel, Stroke::new(1.5, Color32::WHITE));
                painter.text(pos2(sx, rect.center().y), egui::Align2::CENTER_CENTER, "↔", egui::FontId::proportional(11.0), p.text);
                for (x, t, al) in [(sx - 10.0, crate::i18n::t("avant"), egui::Align2::RIGHT_TOP), (sx + 10.0, crate::i18n::t("après"), egui::Align2::LEFT_TOP)] {
                    let g = painter.layout_no_wrap(t, egui::FontId::proportional(11.5), Color32::WHITE);
                    let r = al.anchor_size(pos2(x, rect.top() + 10.0), g.size() + vec2(10.0, 4.0));
                    painter.rect_filled(r, 3.0, Color32::from_black_alpha(150));
                    painter.galley(r.min + vec2(5.0, 2.0), g, Color32::WHITE);
                }
            } else {
                self.split = None;
                self.message = Some(crate::i18n::t("avant / après : la taille de l'image a changé depuis l'ouverture"));
            }
        }
        if plain {
            painter.rect_stroke(canvas, 0.0, Stroke::new(1.0, p.line), egui::StrokeKind::Outside);
        } else {
            let pts = [to_screen(0.0, 0.0), to_screen(dw, 0.0), to_screen(dw, dh), to_screen(0.0, dh)];
            painter.add(egui::Shape::closed_line(pts.to_vec(), Stroke::new(1.0, p.line)));
        }
        // ---- Souris
        let hover = ui.input(|i| i.pointer.hover_pos());
        let space = ui.input(|i| i.key_down(egui::Key::Space));
        let xf_on = self.xf.is_some();
        if xf_on {
            self.xf_ui(ui, &resp, &painter, to_doc, to_screen);
        }
        // Pression du stylet : arrive avec les évènements tactiles (Windows Ink) ; sans tablette, elle reste à 1
        if let Some(f) = ui.input(|i| i.events.iter().rev().find_map(|e| match e {
            egui::Event::Touch { force: Some(f), .. } => Some(*f),
            _ => None,
        })) {
            self.pressure = f;
        }
        // Règles : une bande en haut et à gauche ; on en tire les repères
        let ruler_w = 18.0;
        let top_band = Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + ruler_w));
        let left_band = Rect::from_min_max(rect.min, pos2(rect.left() + ruler_w, rect.bottom()));
        // Un point de fuite se déplace à la souris
        if self.persp_on && resp.drag_started_by(egui::PointerButton::Primary) {
            if let Some(o) = ui.input(|i| i.pointer.press_origin()) {
                self.persp_drag = self.persp.iter().position(|v| (to_screen(v.x, v.y) - o).length() < 10.0);
            }
        }
        if let Some(k) = self.persp_drag {
            if let (Some(at), true) = (resp.interact_pointer_pos(), k < self.persp.len()) {
                self.persp[k] = to_doc(at);
            }
            if !resp.dragged() {
                self.persp_drag = None;
            }
        }
        let persp_busy = self.persp_drag.is_some();
        if resp.drag_started_by(egui::PointerButton::Primary) && !xf_on && !space && !persp_busy {
            if let Some(o) = ui.input(|i| i.pointer.press_origin()) {
                let d = to_doc(o);
                if self.rulers && plain && top_band.contains(o) {
                    self.doc.guides.push(Guide { vertical: false, at: d.y });
                    self.guide_drag = Some(self.doc.guides.len() - 1);
                } else if self.rulers && plain && left_band.contains(o) {
                    self.doc.guides.push(Guide { vertical: true, at: d.x });
                    self.guide_drag = Some(self.doc.guides.len() - 1);
                } else if self.tool == Tool::Move {
                    self.guide_drag = self.doc.guides.iter().position(|g| (if g.vertical { d.x } else { d.y } - g.at).abs() * self.zoom < 4.0);
                }
            }
        }
        if let Some(gi) = self.guide_drag {
            if let (Some(at), Some(g)) = (resp.interact_pointer_pos(), self.doc.guides.get_mut(gi)) {
                let d = to_doc(at);
                g.at = (if g.vertical { d.x } else { d.y }).round();
                ctx.set_cursor_icon(if g.vertical { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical });
            }
            if resp.drag_stopped() {
                // Rendu à la règle ou sorti de la vue : le repère s'en va
                let out = resp.interact_pointer_pos().or(hover).is_none_or(|q| !rect.contains(q) || top_band.contains(q) || left_band.contains(q));
                if out && gi < self.doc.guides.len() {
                    self.doc.guides.remove(gi);
                }
                self.guide_drag = None;
                self.save_due = Some(Instant::now());
            }
        } else if resp.dragged_by(egui::PointerButton::Middle) || (space && resp.dragged_by(egui::PointerButton::Primary)) {
            self.pan += resp.drag_delta();
        } else if !xf_on && !persp_busy {
            if self.swallow_click || self.swatch_pick.is_some() {
                // Fermer le sélecteur de couleur : rien d'autre
            } else if resp.drag_started_by(egui::PointerButton::Primary) {
                if matches!(self.tool, Tool::Brush | Tool::Eraser | Tool::Clone | Tool::Heal | Tool::Smudge | Tool::Liquify | Tool::Move) {
                    self.locked();
                }
                if let Some(at) = ui.input(|i| i.pointer.press_origin()) {
                    let d = to_doc(at);
                    self.next_label = TOOLS.iter().find(|t| t.0 == self.tool).map(|t| t.2);
                    match self.tool {
                        Tool::Clone if ui.input(|i| i.modifiers.alt) => {
                            // Alt+clic : la source du tampon (le décalage se fixe au premier coup de pinceau)
                            self.clone_src = Some(d);
                            self.clone_off = None;
                        }
                        Tool::Clone if self.clone_src.is_none() => {
                            self.message = Some(crate::i18n::t("Alt+clic d'abord pour choisir où prendre les pixels"));
                        }
                        Tool::Brush | Tool::Eraser | Tool::Clone | Tool::Heal | Tool::Smudge | Tool::Liquify if self.doc.layers[self.doc.active].visible && !self.doc.layers[self.doc.active].lock_all => {
                            if self.tool == Tool::Clone && self.clone_off.is_none() {
                                if let Some(src) = self.clone_src {
                                    self.clone_off = Some(src - d);
                                }
                            }
                            self.checkpoint();
                            self.grow_to_doc();
                            let l = self.target_ref().clone();
                            let n = l.w * l.h;
                            self.stroke = Some((l, vec![0.0; n], d));
                            self.smudge_buf.clear();
                            self.prev_dab = None;
                            self.stab = Some(d);
                            self.stroke_from = Some(d);
                            self.dab(d);
                        }
                        Tool::Crop => {
                            match self.crop {
                                // Dans le cadre : on le déplace
                                Some(c) if d.x > c[0] && d.x < c[2] && d.y > c[1] && d.y < c[3] => self.crop_drag = Some((1, pos2(d.x - c[0], d.y - c[1]))),
                                _ => {
                                    let a = self.snap_pt(d);
                                    self.crop = Some([a.x, a.y, a.x, a.y]);
                                    self.crop_drag = Some((0, a));
                                }
                            }
                        }
                        Tool::Move if !self.doc.layers[self.doc.active].lock_all => {
                            self.checkpoint();
                            self.drag_from = Some((d, self.doc.layers[self.doc.active].off));
                        }
                        Tool::Select | Tool::Ellipse | Tool::Lasso => {
                            let m = ui.input(|i| if i.modifiers.shift { 1 } else if i.modifiers.alt { 2 } else { 0 });
                            self.sel_base = Some((self.sel.clone(), m));
                            let d = if self.tool == Tool::Lasso { d } else { self.snap_pt(d) };
                            self.drag_from = Some((d, [0, 0]));
                            self.lasso = vec![[d.x, d.y]];
                        }
                        Tool::Gradient => self.grad = Some((d, d)),
                        _ => {}
                    }
                }
            }
            if resp.dragged_by(egui::PointerButton::Primary) {
                if let Some(at) = resp.interact_pointer_pos() {
                    let d = to_doc(at);
                    match self.tool {
                        Tool::Brush | Tool::Eraser | Tool::Clone | Tool::Heal | Tool::Smudge | Tool::Liquify => {
                            let d = match self.stroke_from {
                                Some(f) => self.persp_snap_pt(f, d),
                                None => d,
                            };
                            // Lissage : le point suivi rattrape la souris avec du retard
                            let d = match self.stab {
                                Some(st) if self.smooth > 0.0 => {
                                    let q = st + (d - st) * (1.0 - 0.93 * self.smooth.powf(0.6));
                                    self.stab = Some(q);
                                    q
                                }
                                _ => d,
                            };
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
                                let new = [off[0] + (d.x - from.x).round() as i32, off[1] + (d.y - from.y).round() as i32];
                                // Le masque est lié au calque (la chaîne) : il suit
                                if let Some(m) = l.mask.as_deref_mut() {
                                    m.off = [m.off[0] + new[0] - l.off[0], m.off[1] + new[1] - l.off[1]];
                                }
                                l.off = new;
                                self.all();
                            }
                        }
                        Tool::Crop => match self.crop_drag {
                            Some((0, a)) => {
                                let b = self.snap_pt(d);
                                self.crop = Some(self.crop_box(a, b));
                            }
                            Some((_, off)) => {
                                if let Some(c) = &mut self.crop {
                                    let (w, h) = (c[2] - c[0], c[3] - c[1]);
                                    let o = pos2(d.x - off.x, d.y - off.y);
                                    *c = [o.x, o.y, o.x + w, o.y + h];
                                }
                            }
                            None => {}
                        },
                        Tool::Select | Tool::Ellipse | Tool::Lasso => {
                            let d = if self.tool == Tool::Lasso { d } else { self.snap_pt(d) };
                            if let Some((from, _)) = self.drag_from {
                                let (w, h) = (self.doc.w, self.doc.h);
                                let (mut a, b) = ([from.x, from.y], [d.x, d.y]);
                                // Maj sans sélection d'avant : un carré ou un cercle
                                let square = ui.input(|i| i.modifiers.shift) && self.sel_base.as_ref().is_some_and(|b| b.0.is_none());
                                let b = if square && self.tool != Tool::Lasso {
                                    let k = (b[0] - a[0]).abs().max((b[1] - a[1]).abs());
                                    [a[0] + k * (b[0] - a[0]).signum(), a[1] + k * (b[1] - a[1]).signum()]
                                } else {
                                    b
                                };
                                if ui.input(|i| i.modifiers.command) && self.tool == Tool::Ellipse {
                                    // Ctrl : depuis le centre
                                    a = [2.0 * a[0] - b[0], 2.0 * a[1] - b[1]];
                                }
                                let new = match self.tool {
                                    Tool::Select => Sel::rect(w, h, a, b),
                                    Tool::Ellipse => Sel::ellipse(w, h, a, b),
                                    _ => {
                                        if self.lasso.last().is_none_or(|l| (l[0] - d.x).hypot(l[1] - d.y) > 1.5 / self.zoom) {
                                            self.lasso.push([d.x, d.y]);
                                        }
                                        None
                                    }
                                };
                                if self.tool != Tool::Lasso {
                                    self.sel = self.mix_sel(new);
                                }
                            }
                        }
                        Tool::Gradient => {
                            if let Some(g) = &mut self.grad {
                                g.1 = d;
                            }
                        }
                        _ => {}
                    }
                }
            }
            if resp.drag_stopped() {
                // Le bout du trait : le pointeur a pu bouger depuis la dernière image avant d'être relâché
                if matches!(self.tool, Tool::Brush | Tool::Eraser | Tool::Clone) && self.smooth <= 0.0 {
                    if let (Some(at), Some(last)) = (resp.interact_pointer_pos().or(hover), self.stroke.as_ref().map(|s| s.2)) {
                        let d = to_doc(at);
                        let d = match self.stroke_from {
                            Some(f) => self.persp_snap_pt(f, d),
                            None => d,
                        };
                        let dist = (d - last).length();
                        let step = (self.size * 0.2).max(0.5);
                        let n = (dist / step).ceil() as usize;
                        for k in 1..=n {
                            self.dab(last + (d - last) * (k as f32 / n as f32));
                        }
                    }
                }
                if self.tool == Tool::Lasso && self.drag_from.is_some() {
                    let pts = std::mem::take(&mut self.lasso);
                    let new = Sel::poly(self.doc.w, self.doc.h, &pts);
                    self.sel = self.mix_sel(new);
                }
                if let Some((a, b)) = self.grad.take() {
                    self.next_label = Some("dégradé");
                    self.gradient(a, b);
                }
                self.sel_base = None;
                if self.tool == Tool::Heal {
                    self.heal_apply();
                }
                self.stroke = None;
                self.stab = None;
                self.drag_from = None;
                self.crop_drag = None;
                // Cadre trop petit : un simple clic, on l'oublie
                if self.crop.is_some_and(|c| (c[2] - c[0]) * self.zoom < 4.0 || (c[3] - c[1]) * self.zoom < 4.0) {
                    self.crop = None;
                }
            }
            if ui.input(|i| i.pointer.any_released()) && std::mem::take(&mut self.swallow_click) {
                // Le clic qui a fermé le sélecteur de couleur s'arrête là
            } else if resp.clicked() && self.swatch_pick.is_none() {
                if let Some(at) = resp.interact_pointer_pos() {
                    let d = to_doc(at);
                    let px = [d.x.floor() as i32, d.y.floor() as i32];
                    self.next_label = TOOLS.iter().find(|t| t.0 == self.tool).map(|t| t.2);
                    match self.tool {
                        Tool::Fill => self.fill(px),
                        Tool::Picker => {
                            if px[0] >= 0 && px[1] >= 0 && (px[0] as usize) < self.doc.w && (px[1] as usize) < self.doc.h {
                                let i = (px[1] as usize * self.doc.w + px[0] as usize) * 4;
                                self.color = Color32::from_rgba_unmultiplied(self.comp[i], self.comp[i + 1], self.comp[i + 2], self.comp[i + 3].max(1));
                            }
                        }
                        Tool::Clone if ui.input(|i| i.modifiers.alt) => {
                            self.clone_src = Some(d);
                            self.clone_off = None;
                        }
                        Tool::Clone if self.clone_src.is_none() => {
                            self.message = Some(crate::i18n::t("Alt+clic d'abord pour choisir où prendre les pixels"));
                        }
                        Tool::Brush | Tool::Eraser | Tool::Clone | Tool::Heal | Tool::Smudge | Tool::Liquify if !self.doc.layers[self.doc.active].lock_all => {
                            if self.tool == Tool::Clone && self.clone_off.is_none() {
                                if let Some(src) = self.clone_src {
                                    self.clone_off = Some(src - d);
                                }
                            }
                            self.checkpoint();
                            self.grow_to_doc();
                            let l = self.target_ref().clone();
                            let n = l.w * l.h;
                            self.stroke = Some((l, vec![0.0; n], d));
                            self.dab(d);
                            if self.tool == Tool::Heal {
                                self.heal_apply();
                            }
                            self.stroke = None;
                        }
                        Tool::Select | Tool::Ellipse | Tool::Lasso => self.sel = None,
                        Tool::Wand => {
                            let m = ui.input(|i| if i.modifiers.shift { 1 } else if i.modifiers.alt { 2 } else { 0 });
                            self.sel_base = Some((self.sel.clone(), m));
                            let new = Sel::wand(self.doc.w, self.doc.h, &self.comp, px, self.tolerance);
                            self.sel = self.mix_sel(new);
                            self.sel_base = None;
                        }
                        Tool::Text => {
                            // Un nouveau clic pose le texte en cours, puis en commence un autre
                            self.commit_text(ui.ctx());
                            self.text = Some(TextDraft { at: [d.x, d.y], s: String::new(), preview: None, focus: true });
                        }
                        Tool::Gradient | Tool::Move | Tool::Brush | Tool::Eraser | Tool::Clone | Tool::Heal | Tool::Smudge | Tool::Liquify | Tool::Crop => {}
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
                    // Le point sous le pointeur reste sous le pointeur (vue tournée comprise)
                    let mut vx = (before.x - hw) * z;
                    let vy = (before.y - hh) * z;
                    if flip {
                        vx = -vx;
                    }
                    self.pan = at - rect.center() - vec2(vx * cs - vy * sn, vx * sn + vy * cs);
                }
            }
        }
        // Sélection : cadre en pointillés
        // Fourmis : le contour de la forme, ou le cadre d'une sélection composée
        let ants = |pts: Vec<Pos2>| {
            painter.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.0, Color32::BLACK), 5.0, 5.0));
            painter.extend(egui::Shape::dashed_line(&pts.iter().skip(1).copied().chain(pts.first().copied()).collect::<Vec<_>>(), Stroke::new(1.0, Color32::WHITE), 5.0, 5.0));
        };
        if let Some(s) = &self.sel {
            let mut pts: Vec<Pos2> = if s.outline.is_empty() {
                let b = s.bbox;
                vec![to_screen(b[0] as f32, b[1] as f32), to_screen(b[2] as f32, b[1] as f32), to_screen(b[2] as f32, b[3] as f32), to_screen(b[0] as f32, b[3] as f32)]
            } else {
                s.outline.iter().map(|p| to_screen(p[0], p[1])).collect()
            };
            if let Some(f) = pts.first().copied() {
                pts.push(f);
            }
            ants(pts);
        }
        if self.tool == Tool::Lasso && self.lasso.len() > 1 {
            ants(self.lasso.iter().map(|p| to_screen(p[0], p[1])).collect());
        }
        if let Some((a, b)) = self.grad {
            let (sa, sb) = (to_screen(a.x, a.y), to_screen(b.x, b.y));
            painter.line_segment([sa, sb], Stroke::new(2.0, Color32::from_black_alpha(150)));
            painter.line_segment([sa, sb], Stroke::new(1.0, Color32::WHITE));
            painter.circle_filled(sa, 4.0, self.color);
            painter.circle_stroke(sb, 4.0, Stroke::new(1.0, Color32::WHITE));
        }
        // Texte en cours : aperçu sur l'image et petite fenêtre de saisie
        let mut text_done: Option<bool> = None;
        if self.tool != Tool::Text && self.text.is_some() {
            text_done = Some(true);
        }
        let (font_name, size, color) = (crate::motion::FONTS[self.text_font.min(crate::motion::FONTS.len() - 1)].0, self.text_size, self.color.to_srgba_unmultiplied());
        if let Some(t) = &mut self.text {
            let key = format!("{}|{font_name}|{size}|{color:?}", t.s);
            if t.preview.as_ref().is_none_or(|p| p.0 != key) {
                t.preview = render_text(&t.s, font_name, size, color).map(|(px, w, h, off)| {
                    let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &px);
                    (key.clone(), ctx.load_texture("paint-text", img, egui::TextureOptions::LINEAR), off, [w, h])
                });
                if t.preview.is_none() {
                    t.preview = Some((key, ctx.load_texture("paint-text", egui::ColorImage::new([1, 1], vec![Color32::TRANSPARENT]), egui::TextureOptions::LINEAR), [0, 0], [0, 0]));
                }
            }
            let anchor = to_screen(t.at[0], t.at[1]);
            let mut box_r = Rect::from_min_size(anchor, vec2(self.text_size * self.zoom * 0.6, self.text_size * self.zoom * 1.2));
            if let Some((_, tex, off, [w, h])) = &t.preview {
                if *w > 0 {
                    let r = Rect::from_min_size(to_screen(t.at[0] + off[0] as f32, t.at[1] + off[1] as f32), vec2(*w as f32 * self.zoom, *h as f32 * self.zoom));
                    painter.image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    box_r = box_r.union(r);
                }
            }
            painter.rect_stroke(box_r.expand(4.0), 0.0, Stroke::new(1.0, Color32::from_black_alpha(140)), egui::StrokeKind::Outside);
            painter.rect_stroke(box_r.expand(3.0), 0.0, Stroke::new(1.0, Color32::from_white_alpha(200)), egui::StrokeKind::Outside);
            egui::Area::new(egui::Id::unique("paint-text-box")).order(egui::Order::Foreground).fixed_pos(pos2(box_r.left() - 4.0, box_r.bottom() + 10.0)).show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let r = ui.add(egui::TextEdit::multiline(&mut t.s).desired_rows(2).desired_width(260.0).hint_text(crate::i18n::t("Ton texte")));
                    if t.focus {
                        r.request_focus();
                        t.focus = false;
                    }
                    ui.horizontal(|ui| {
                        if ui.button(format!("{}  {}", crate::lucide::CHECK, crate::i18n::t("poser le texte"))).on_hover_text("Ctrl+Entrée").clicked() {
                            text_done = Some(true);
                        }
                        if ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("annuler"))).on_hover_text(crate::i18n::t("Échap")).clicked() {
                            text_done = Some(false);
                        }
                    });
                    if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter)) {
                        text_done = Some(true);
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        text_done = Some(false);
                    }
                });
            });
            ctx.request_repaint();
        }
        match text_done {
            Some(true) => self.commit_text(&ctx),
            Some(false) => self.text = None,
            None => {}
        }
        // Tampon : une croix là où l'on prend les pixels
        if self.tool == Tool::Clone {
            let src = match (self.clone_off, hover.filter(|q| rect.contains(*q))) {
                (Some(o), Some(h)) => Some(to_doc(h) + o),
                _ => self.clone_src,
            };
            if let Some(sd) = src {
                let q = to_screen(sd.x, sd.y);
                for (a, b) in [(egui::vec2(-7.0, 0.0), egui::vec2(7.0, 0.0)), (egui::vec2(0.0, -7.0), egui::vec2(0.0, 7.0))] {
                    painter.line_segment([q + a, q + b], Stroke::new(3.0, Color32::from_black_alpha(140)));
                    painter.line_segment([q + a, q + b], Stroke::new(1.2, Color32::WHITE));
                }
                painter.circle_stroke(q, (self.size * self.zoom / 2.0).max(1.0), Stroke::new(1.0, Color32::from_white_alpha(110)));
            }
        }
        // Recadrage : l'extérieur assombri, le cadre et ses tiers
        if let Some(c) = self.crop {
            let q = [to_screen(c[0], c[1]), to_screen(c[2], c[1]), to_screen(c[2], c[3]), to_screen(c[0], c[3])];
            if plain {
                let r = Rect::from_two_pos(q[0], q[2]);
                let dim = Color32::from_black_alpha(130);
                painter.rect_filled(Rect::from_min_max(rect.min, pos2(rect.right(), r.top())), 0.0, dim);
                painter.rect_filled(Rect::from_min_max(pos2(rect.left(), r.bottom()), rect.max), 0.0, dim);
                painter.rect_filled(Rect::from_min_max(pos2(rect.left(), r.top()), pos2(r.left(), r.bottom())), 0.0, dim);
                painter.rect_filled(Rect::from_min_max(pos2(r.right(), r.top()), pos2(rect.right(), r.bottom())), 0.0, dim);
            }
            painter.add(egui::Shape::closed_line(q.to_vec(), Stroke::new(1.5, Color32::WHITE)));
            for k in [1.0f32, 2.0] {
                let (x, y) = (c[0] + (c[2] - c[0]) * k / 3.0, c[1] + (c[3] - c[1]) * k / 3.0);
                painter.line_segment([to_screen(x, c[1]), to_screen(x, c[3])], Stroke::new(1.0, Color32::from_white_alpha(90)));
                painter.line_segment([to_screen(c[0], y), to_screen(c[2], y)], Stroke::new(1.0, Color32::from_white_alpha(90)));
            }
            let txt = format!("{} × {}", (c[2] - c[0]).round(), (c[3] - c[1]).round());
            painter.text(q[2] + vec2(6.0, 6.0), egui::Align2::LEFT_TOP, txt, egui::FontId::proportional(11.0), Color32::WHITE);
        }
        // Trait du correcteur en cours
        if !self.heal_trail.is_empty() {
            let r = (self.size * self.zoom / 2.0).max(1.0);
            for q in &self.heal_trail {
                painter.circle_filled(to_screen(q.x, q.y), r, Color32::from_rgba_unmultiplied(230, 60, 90, 40));
            }
        }
        // Repères
        let guide_col = Color32::from_rgb(0, 200, 230);
        for g in &self.doc.guides {
            let (a, b) = if g.vertical { (to_screen(g.at, -1e5), to_screen(g.at, 1e5)) } else { (to_screen(-1e5, g.at), to_screen(1e5, g.at)) };
            painter.line_segment([a, b], Stroke::new(1.0, guide_col));
        }
        // Perspective : lignes de fuite et poignées des points
        if self.persp_on {
            let col = Color32::from_rgba_unmultiplied(120, 170, 255, 70);
            let (w, h) = (self.doc.w as f32, self.doc.h as f32);
            let reach = (w + h) * 2.0;
            for v in &self.persp {
                for k in 0..36 {
                    let a = k as f32 / 36.0 * std::f32::consts::TAU;
                    painter.line_segment([to_screen(v.x, v.y), to_screen(v.x + a.cos() * reach, v.y + a.sin() * reach)], Stroke::new(1.0, col));
                }
            }
            if self.persp.len() >= 2 {
                let (a, b) = (self.persp[0], self.persp[1]);
                let dd = (b - a).normalized() * reach;
                painter.line_segment([to_screen(a.x - dd.x, a.y - dd.y), to_screen(b.x + dd.x, b.y + dd.y)], Stroke::new(1.5, Color32::from_rgb(120, 170, 255)));
            }
            for v in &self.persp {
                let q = to_screen(v.x, v.y);
                painter.circle_filled(q, 6.0, Color32::from_rgb(120, 170, 255));
                painter.circle_stroke(q, 6.0, Stroke::new(1.5, Color32::WHITE));
            }
        }
        // Symétrie : l'axe
        if self.sym_x && matches!(self.tool, Tool::Brush | Tool::Eraser) {
            let x = self.doc.w as f32 / 2.0;
            painter.line_segment([to_screen(x, 0.0), to_screen(x, self.doc.h as f32)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 120, 200, 140)));
        }
        if self.sym_y && matches!(self.tool, Tool::Brush | Tool::Eraser) {
            let y = self.doc.h as f32 / 2.0;
            painter.line_segment([to_screen(0.0, y), to_screen(self.doc.w as f32, y)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 120, 200, 140)));
        }
        // Règles (vue droite seulement), graduées en pixels
        if self.rulers && plain {
            let bg = p.panel;
            painter.rect_filled(top_band, 0.0, bg);
            painter.rect_filled(left_band, 0.0, bg);
            painter.hline(top_band.x_range(), top_band.bottom(), Stroke::new(1.0, p.line));
            painter.vline(left_band.right(), left_band.y_range(), Stroke::new(1.0, p.line));
            let step = [1.0f32, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0].into_iter().find(|s| s * zoom >= 60.0).unwrap_or(10000.0);
            let font = egui::FontId::proportional(9.0);
            let (a, b) = (to_doc(top_band.left_top()), to_doc(top_band.right_bottom()));
            let (lo, hi) = (a.x.min(b.x), a.x.max(b.x));
            let mut x = (lo / step).floor() * step;
            while x <= hi {
                for k in 0..10 {
                    let xx = x + step * k as f32 / 10.0;
                    let sx = to_screen(xx, 0.0).x;
                    if sx > left_band.right() && sx < rect.right() {
                        let len = if k == 0 { ruler_w } else if k == 5 { 7.0 } else { 4.0 };
                        painter.vline(sx, (top_band.bottom() - len)..=top_band.bottom(), Stroke::new(1.0, p.muted));
                        if k == 0 {
                            painter.text(pos2(sx + 2.0, top_band.top() + 1.0), egui::Align2::LEFT_TOP, format!("{}", x as i64), font.clone(), p.muted);
                        }
                    }
                }
                x += step;
            }
            let (a, b) = (to_doc(left_band.left_top()), to_doc(left_band.right_bottom()));
            let (lo, hi) = (a.y.min(b.y), a.y.max(b.y));
            let mut y = (lo / step).floor() * step;
            while y <= hi {
                for k in 0..10 {
                    let yy = y + step * k as f32 / 10.0;
                    let sy = to_screen(0.0, yy).y;
                    if sy > top_band.bottom() && sy < rect.bottom() {
                        let len = if k == 0 { ruler_w } else if k == 5 { 7.0 } else { 4.0 };
                        painter.hline((left_band.right() - len)..=left_band.right(), sy, Stroke::new(1.0, p.muted));
                        if k == 0 {
                            painter.text(pos2(left_band.left() + 1.0, sy + 2.0), egui::Align2::LEFT_TOP, format!("{}", y as i64), font.clone(), p.muted);
                        }
                    }
                }
                y += step;
            }
            // Le pointeur sur les deux règles
            if let Some(h) = hover.filter(|q| rect.contains(*q)) {
                painter.vline(h.x, top_band.y_range(), Stroke::new(1.0, p.accent));
                painter.hline(left_band.x_range(), h.y, Stroke::new(1.0, p.accent));
            }
            painter.rect_filled(Rect::from_min_size(rect.min, vec2(ruler_w, ruler_w)), 0.0, bg);
        }
        // Rond du pinceau sous le pointeur
        if let (Some(at), true) = (hover.filter(|q| rect.contains(*q)), matches!(self.tool, Tool::Brush | Tool::Eraser | Tool::Clone | Tool::Heal | Tool::Smudge | Tool::Liquify)) {
            painter.circle_stroke(at, (self.size * self.zoom / 2.0).max(1.0), Stroke::new(1.0, Color32::from_black_alpha(160)));
            painter.circle_stroke(at, (self.size * self.zoom / 2.0).max(1.0) + 1.0, Stroke::new(1.0, Color32::from_white_alpha(160)));
            // Le point lissé que suit le trait
            if let (Some(st), true) = (self.stab, self.smooth > 0.0) {
                let q = to_screen(st.x, st.y);
                painter.line_segment([q, at], Stroke::new(1.0, Color32::from_white_alpha(140)));
                painter.circle_filled(q, 2.5, self.color);
            }
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        // Barre de vue en bas à droite de la toile : taille, zoom, ajuster, tourner, miroir, règles
        {
            let bar = Rect::from_min_size(pos2(rect.right() - 312.0, rect.bottom() - 30.0), vec2(306.0, 24.0));
            painter.rect_filled(bar, 0.0, p.panel.gamma_multiply(0.92));
            painter.rect_stroke(bar, 0.0, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
            ui.scope_builder(egui::UiBuilder::new().max_rect(bar.shrink2(vec2(6.0, 2.0))).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let vb = |ui: &mut egui::Ui, icon: &str, on: bool, tip: &str| ui.add(egui::Button::new(egui::RichText::new(icon).size(13.0)).selected(on).frame(on).min_size(egui::vec2(22.0, 20.0))).on_hover_text(crate::i18n::t(tip)).clicked();
                if vb(ui, crate::lucide::COLUMNS_2, self.split.is_some(), "Avant / après : l'image à l'ouverture à gauche, glisser la séparation") {
                    self.split = if self.split.is_some() { None } else { Some(0.5) };
                }
                if vb(ui, crate::lucide::RULER, self.rulers, "Règles et repères (Ctrl+R) : tirer depuis une règle pour poser un repère") {
                    self.rulers = !self.rulers;
                }
                if vb(ui, crate::lucide::FLIP_HORIZONTAL_2, self.view_flip, "Vue en miroir : l'image ne change pas") {
                    self.view_flip = !self.view_flip;
                }
                if vb(ui, crate::lucide::ROTATE_CW, false, "Tourner la vue de 15° (6)") {
                    self.view_rot += 15f32.to_radians();
                }
                if vb(ui, crate::lucide::ROTATE_CCW, false, "Tourner la vue de 15° (4) ; 5 la remet droite") {
                    self.view_rot -= 15f32.to_radians();
                }
                if vb(ui, crate::lucide::SCAN, false, "ajuster") {
                    self.fit = true;
                    self.view_rot = 0.0;
                }
                let deg = self.view_rot.to_degrees().rem_euclid(360.0).round();
                let txt = if deg == 0.0 { format!("{} × {} · {} %", self.doc.w, self.doc.h, (self.zoom * 100.0).round()) } else { format!("{} × {} · {} % · {deg}°", self.doc.w, self.doc.h, (self.zoom * 100.0).round()) };
                ui.label(egui::RichText::new(txt).size(10.5).color(p.muted));
            });
        }
        self.next_label = None;
        // ---- Clavier
        if !ctx.egui_wants_keyboard_input() && (resp.hovered() || resp.has_focus()) {
            let (cmd, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
            let k = |key: egui::Key| ui.input(|i| i.key_pressed(key));
            if self.crop.is_some() && k(egui::Key::Enter) {
                self.apply_crop();
            } else if self.crop.is_some() && k(egui::Key::Escape) {
                self.crop = None;
            } else if cmd && k(egui::Key::R) {
                self.rulers = !self.rulers;
            } else if self.xf.is_some() {
                if k(egui::Key::Enter) {
                    self.end_xf(true);
                } else if k(egui::Key::Escape) {
                    self.end_xf(false);
                }
            } else if cmd && k(egui::Key::T) {
                self.start_xf();
            } else if cmd && k(egui::Key::Z) {
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
                self.sel = Some(Sel::all(self.doc.w, self.doc.h));
            } else if cmd && shift && k(egui::Key::I) {
                self.sel = self.sel.as_ref().map(|s| s.invert(self.doc.w, self.doc.h));
            } else if cmd && k(egui::Key::G) {
                // Dans un groupe, le nouveau s'y emboîte
                self.group_active();
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
                    if let Some(kk) = egui::Key::from_name(key).filter(|_| key != "G") {
                        if k(kk) {
                            self.tool = t;
                        }
                    }
                }
                // G : pot de peinture, puis dégradé (comme dans Photoshop)
                if k(egui::Key::G) {
                    self.tool = if self.tool == Tool::Fill { Tool::Gradient } else { Tool::Fill };
                }
                if k(egui::Key::X) {
                    std::mem::swap(&mut self.color, &mut self.bg_color);
                }
                // Vue : 4 et 6 tournent, 5 remet droite (comme Krita)
                if k(egui::Key::Num4) {
                    self.view_rot -= 15f32.to_radians();
                }
                if k(egui::Key::Num6) {
                    self.view_rot += 15f32.to_radians();
                }
                if k(egui::Key::Num5) {
                    self.view_rot = 0.0;
                }

                if k(egui::Key::D) && !ui.input(|i| i.modifiers.any()) {
                    (self.color, self.bg_color) = (Color32::BLACK, Color32::WHITE);
                }
                if k(egui::Key::OpenBracket) {
                    self.size = (self.size / 1.2).max(1.0);
                }
                if k(egui::Key::CloseBracket) {
                    self.size = (self.size * 1.2).min(500.0);
                }
                if (k(egui::Key::Delete) || k(egui::Key::Backspace)) && self.sel.is_some() {
                    // Efface la sélection dans le calque actif (en douceur sur les bords)
                    self.checkpoint();
                    let s = self.sel.clone().unwrap();
                    let dw = self.doc.w;
                    let l = &mut self.doc.layers[self.doc.active];
                    for y in s.bbox[1]..s.bbox[3] {
                        for x in s.bbox[0]..s.bbox[2] {
                            let (lx, ly) = (x - l.off[0], y - l.off[1]);
                            if lx >= 0 && ly >= 0 && (lx as usize) < l.w && (ly as usize) < l.h {
                                let i = (ly as usize * l.w + lx as usize) * 4 + 3;
                                l.px[i] = (l.px[i] as f32 * (1.0 - s.at(dw, x, y))) as u8;
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

    /// Pose le texte en cours dans un nouveau calque, au-dessus du calque actif
    fn commit_text(&mut self, _ctx: &egui::Context) {
        let Some(t) = self.text.take() else { return };
        if t.s.trim().is_empty() {
            return;
        }
        let font = crate::motion::FONTS[self.text_font.min(crate::motion::FONTS.len() - 1)].0;
        let Some((px, w, h, off)) = render_text(&t.s, font, self.text_size, self.color.to_srgba_unmultiplied()) else { return };
        self.checkpoint();
        let name: String = t.s.lines().next().unwrap_or("").chars().take(24).collect();
        let layer = Layer { off: [t.at[0].round() as i32 + off[0], t.at[1].round() as i32 + off[1]], px, ..Layer::blank(&format!("T {name}"), w, h) };
        self.doc.layers.insert(self.doc.active + 1, layer);
        self.doc.active += 1;
        self.all();
        self.save_due = Some(Instant::now());
    }

    /// Passe en animation : un groupe « Animation » avec une première image vide au-dessus du calque actif.
    fn anim_start(&mut self) {
        self.checkpoint();
        let id = self.doc.groups.iter().map(|g| g.id).max().unwrap_or(0) + 1;
        self.doc.groups.push(Group::new(id, crate::i18n::t("Animation"), 0));
        let at = (self.doc.active + 1).min(self.doc.layers.len());
        self.doc.layers.insert(at, Layer { group: id, ..Layer::blank(&format!("{} 1", crate::i18n::t_mid("image")), self.doc.w, self.doc.h) });
        self.doc.active = at;
        self.doc.anim = Some(Anim { group: id, fps: 12.0, onion: true });
        self.set_frame(0);
    }

    /// Montre l'image `k` (les autres images se cachent) et la rend active.
    fn set_frame(&mut self, k: usize) {
        let frames = self.doc.frames();
        let Some(&li) = frames.get(k.min(frames.len().saturating_sub(1))) else { return };
        for &f in &frames {
            self.doc.layers[f].visible = f == li;
        }
        self.doc.active = li;
        self.all();
    }

    fn cur_frame(&self) -> usize {
        self.doc.frames().iter().position(|&i| i == self.doc.active).unwrap_or(0)
    }

    /// Nouvelle image après l'image actuelle : vide, ou copie de l'actuelle.
    fn anim_add(&mut self, copy: bool) {
        let Some(a) = self.doc.anim.clone() else { return };
        self.checkpoint();
        let frames = self.doc.frames();
        let k = self.cur_frame();
        let at = frames.get(k).map(|i| i + 1).unwrap_or(self.doc.layers.len());
        let n = frames.len() + 1;
        let l = if copy { Layer { name: format!("{} {n}", crate::i18n::t_mid("image")), ..self.doc.layers[frames[k]].clone() } } else { Layer { group: a.group, ..Layer::blank(&format!("{} {n}", crate::i18n::t_mid("image")), self.doc.w, self.doc.h) } };
        self.doc.layers.insert(at, l);
        self.set_frame(k + 1);
    }

    fn anim_delete(&mut self) {
        let frames = self.doc.frames();
        if frames.len() < 2 {
            return;
        }
        self.checkpoint();
        let k = self.cur_frame();
        self.doc.layers.remove(frames[k]);
        self.set_frame(k.saturating_sub(1));
    }

    /// Exporte l'animation : chaque image posée sur le reste du document (fond compris), puis GIF ou MP4.
    fn anim_export(&mut self, gif: bool) {
        let Some(a) = self.doc.anim.clone() else { return };
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ext = if gif { "gif" } else { "mp4" };
        let Some(out) = rfd::FileDialog::new().set_directory(self.path.parent().unwrap_or(std::path::Path::new(""))).set_file_name(format!("{stem}.{ext}")).add_filter(ext.to_uppercase(), &[ext]).save_file() else { return };
        let keep = (self.doc.active, self.doc.layers.iter().map(|l| l.visible).collect::<Vec<_>>());
        let tmp = std::env::temp_dir().join(format!("aryadne-anim-{}", std::process::id()));
        let r = (|| -> Result<(), String> {
            std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
            let frames = self.doc.frames();
            for (k, &li) in frames.iter().enumerate() {
                for &f in &frames {
                    self.doc.layers[f].visible = f == li;
                }
                let img = self.doc.flatten();
                // Dimensions paires pour l'encodeur vidéo
                let (w, h) = (img.width() & !1, img.height() & !1);
                image::imageops::crop_imm(&img, 0, 0, w.max(2), h.max(2)).to_image().save(tmp.join(format!("{k:05}.png"))).map_err(|e| e.to_string())?;
            }
            let ff = crate::ffmpeg::find("ffmpeg").ok_or("ffmpeg introuvable")?;
            let mut cmd = crate::ffmpeg::command(ff);
            cmd.args(["-y", "-v", "error", "-framerate", &format!("{}", a.fps.max(1.0)), "-i"]).arg(tmp.join("%05d.png"));
            if gif {
                cmd.args(["-vf", "split[a][b];[a]palettegen=reserve_transparent=0[p];[b][p]paletteuse", "-loop", "0"]);
            } else {
                cmd.args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18"]);
            }
            let o = cmd.arg(&out).output().map_err(|e| e.to_string())?;
            if !o.status.success() {
                return Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("échec de ffmpeg").to_string());
            }
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&tmp);
        for (l, v) in self.doc.layers.iter_mut().zip(keep.1) {
            l.visible = v;
        }
        self.doc.active = keep.0;
        self.all();
        self.message = Some(match r {
            Ok(()) => format!("{} {}", crate::i18n::t("exporté :"), out.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default()),
            Err(e) => e,
        });
    }

    /// La bande de l'animation : lecture, cadence, pelure d'oignon, les images en vignettes.
    fn anim_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let frames = self.doc.frames();
        if frames.is_empty() {
            self.doc.anim = None;
            return;
        }
        // Lecture
        if let Some((t0, from)) = self.anim_play {
            let fps = self.doc.anim.as_ref().map(|a| a.fps).unwrap_or(12.0).max(1.0);
            let k = (from + (t0.elapsed().as_secs_f32() * fps) as usize) % frames.len();
            if k != self.cur_frame() {
                self.set_frame(k);
            }
            ui.ctx().request_repaint();
        }
        let typing = ui.ctx().egui_wants_keyboard_input();
        if !typing {
            let (left, right) = ui.input(|i| (i.key_pressed(egui::Key::Comma), i.key_pressed(egui::Key::Period)));
            if left {
                let k = self.cur_frame();
                self.set_frame(if k == 0 { frames.len() - 1 } else { k - 1 });
            }
            if right {
                self.set_frame((self.cur_frame() + 1) % frames.len());
            }
        }
        egui::Panel::bottom(egui::Id::unique(("paint-anim", self.path.to_string_lossy().to_string()))).resizable(false).exact_size(96.0).frame(egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(8, 6))).show(ui, |ui| {
            ui.horizontal(|ui| {
                let b = |ui: &mut egui::Ui, icon: &str, tip: &str| ui.add(egui::Button::new(egui::RichText::new(icon).size(14.0)).min_size(vec2(26.0, 22.0))).on_hover_text(crate::i18n::t(tip)).clicked();
                let playing = self.anim_play.is_some();
                if b(ui, if playing { crate::lucide::PAUSE } else { crate::lucide::PLAY }, "lire l'animation") {
                    self.anim_play = if playing { None } else { Some((Instant::now(), self.cur_frame())) };
                    self.all();
                }
                if b(ui, crate::lucide::PLUS, "nouvelle image vide après celle-ci") {
                    self.anim_play = None;
                    self.anim_add(false);
                }
                if b(ui, crate::lucide::COPY, "copier cette image à la suite") {
                    self.anim_play = None;
                    self.anim_add(true);
                }
                if b(ui, crate::lucide::TRASH_2, "supprimer cette image") {
                    self.anim_play = None;
                    self.anim_delete();
                }
                ui.separator();
                if let Some(a) = &mut self.doc.anim {
                    ui.add(egui::DragValue::new(&mut a.fps).range(1.0..=60.0).speed(0.2).suffix(" i/s")).on_hover_text(crate::i18n::t("images par seconde"));
                    let on = a.onion;
                    if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::LAYERS).size(14.0)).selected(on).min_size(vec2(26.0, 22.0))).on_hover_text(crate::i18n::t("pelure d'oignon : l'image d'avant en rouge, celle d'après en vert")).clicked() {
                        a.onion = !on;
                        self.all();
                    }
                }
                ui.label(egui::RichText::new(format!("{} / {}", self.cur_frame() + 1, frames.len())).monospace().size(11.0).color(p.muted)).on_hover_text(crate::i18n::t(", et . : image précédente, suivante"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(crate::i18n::t("MP4…")).on_hover_text(crate::i18n::t("exporter l'animation en vidéo")).clicked() {
                        self.anim_export(false);
                    }
                    if ui.button(crate::i18n::t("GIF…")).on_hover_text(crate::i18n::t("exporter l'animation en GIF animé")).clicked() {
                        self.anim_export(true);
                    }
                });
            });
            // Les images
            egui::ScrollArea::horizontal().id_salt("paint-anim-strip").show(ui, |ui| {
                ui.horizontal(|ui| {
                    let th = 48.0;
                    let tw = th * self.doc.w as f32 / self.doc.h.max(1) as f32;
                    let cur = self.cur_frame();
                    let mut pick = None;
                    for (k, &li) in frames.iter().enumerate() {
                        // Vignette refaite quand le calque change (clé : taille et quelques pixels)
                        let l = &self.doc.layers[li];
                        let key = {
                            use std::hash::{Hash, Hasher};
                            let mut h = std::collections::hash_map::DefaultHasher::new();
                            (l.w, l.h, l.off).hash(&mut h);
                            for c in l.px.iter().step_by(97) {
                                c.hash(&mut h);
                            }
                            h.finish()
                        };
                        if !self.anim_thumbs.iter().any(|t| t.0 == li && t.1 == key) {
                            let s = (64.0 / l.w.max(1) as f32).min(64.0 / l.h.max(1) as f32).min(1.0);
                            let img = image::RgbaImage::from_raw(l.w as u32, l.h as u32, l.px.clone()).map(|i| image::imageops::thumbnail(&i, ((l.w as f32 * s) as u32).max(1), ((l.h as f32 * s) as u32).max(1)));
                            if let Some(img) = img {
                                let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                                let t = ui.ctx().load_texture(format!("anim-{li}"), ci, egui::TextureOptions::LINEAR);
                                self.anim_thumbs.retain(|t| t.0 != li);
                                self.anim_thumbs.push((li, key, t));
                            }
                        }
                        let (r, resp) = ui.allocate_exact_size(vec2(tw, th + 12.0), egui::Sense::click());
                        let img = Rect::from_min_size(r.min, vec2(tw, th));
                        ui.painter().rect_filled(img, 2.0, Color32::WHITE);
                        if let Some((_, _, t)) = self.anim_thumbs.iter().find(|t| t.0 == li) {
                            ui.painter().image(t.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                        }
                        ui.painter().rect_stroke(img, 2.0, Stroke::new(if k == cur { 2.0 } else { 1.0 }, if k == cur { p.accent } else { p.line }), egui::StrokeKind::Outside);
                        ui.painter().text(pos2(img.center().x, img.bottom() + 1.0), egui::Align2::CENTER_TOP, (k + 1).to_string(), egui::FontId::proportional(10.0), if k == cur { p.text } else { p.muted });
                        if resp.clicked() {
                            pick = Some(k);
                        }
                    }
                    if let Some(k) = pick {
                        self.anim_play = None;
                        self.set_frame(k);
                    }
                });
            });
        });
    }

    fn new_layer(&mut self) {
        self.checkpoint();
        let n = self.doc.layers.len() + 1;
        let group = self.doc.layers[self.doc.active].group;
        self.doc.layers.insert(self.doc.active + 1, Layer { group, ..Layer::blank(&format!("calque {n}"), self.doc.w, self.doc.h) });
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
        // Comme Photoshop : l'opacité, le masque et le mode du calque d'en dessous sont appliqués au résultat
        let mut base = self.doc.layers[a - 1].clone();
        base.blend = Blend::Normal;
        let mut top = self.doc.layers[a].clone();
        top.clip = false;
        let pair = Doc { w: self.doc.w, h: self.doc.h, layers: vec![base, top], active: 1, groups: vec![], guides: vec![], anim: None };
        let mut out = vec![0u8; self.doc.w * self.doc.h * 4];
        pair.composite(&mut out, [0, 0, self.doc.w, self.doc.h]);
        let below = &self.doc.layers[a - 1];
        let merged = Layer { px: out, clip: below.clip, ..Layer::blank(&below.name, self.doc.w, self.doc.h) };
        self.doc.layers[a - 1] = merged;
        self.doc.layers.remove(a);
        self.doc.active = a - 1;
        self.all();
    }

    /// Le calque où l'on peint : le masque du calque actif s'il est choisi, sinon ses pixels
    /// Le groupe dont on peint le masque (groupe choisi, mode masque, et il en a un)
    fn group_mask_target(&self) -> Option<usize> {
        let g = self.sel_group.filter(|_| self.edit_mask)?;
        self.doc.groups.iter().position(|x| x.id == g && x.mask.0.is_some())
    }

    fn target(&mut self) -> &mut Layer {
        if let Some(gi) = self.group_mask_target() {
            return self.doc.groups[gi].mask.0.as_deref_mut().unwrap();
        }
        let a = self.doc.active;
        let edit_mask = self.edit_mask && self.doc.layers[a].mask.is_some();
        let l = &mut self.doc.layers[a];
        if edit_mask {
            l.mask.as_deref_mut().unwrap()
        } else {
            l
        }
    }

    fn target_ref(&self) -> &Layer {
        if let Some(gi) = self.group_mask_target() {
            return self.doc.groups[gi].mask.0.as_deref().unwrap();
        }
        let l = &self.doc.layers[self.doc.active];
        match (&l.mask, self.edit_mask) {
            (Some(m), true) => m,
            _ => l,
        }
    }

    /// La couleur de peinture : dans un masque, son gris (noir cache, blanc montre)
    fn paint_color(&self) -> Color32 {
        if self.painting_mask() {
            let c = self.color;
            let g = (0.3 * c.r() as f32 + 0.59 * c.g() as f32 + 0.11 * c.b() as f32).round() as u8;
            Color32::from_rgba_unmultiplied(g, g, g, c.a())
        } else {
            self.color
        }
    }

    /// Le calque actif est verrouillé : on le dit et on ne touche à rien
    fn locked(&mut self) -> bool {
        let l = &self.doc.layers[self.doc.active];
        if l.lock_all {
            self.message = Some(crate::i18n::t("Ce calque est verrouillé."));
            return true;
        }
        false
    }

    /// Comme Photoshop : peindre à côté d'un petit calque l'agrandit (ici, jusqu'à couvrir le document)
    fn grow_to_doc(&mut self) {
        let (dw, dh) = (self.doc.w as i32, self.doc.h as i32);
        let l = self.target();
        let (x0, y0) = (l.off[0].min(0), l.off[1].min(0));
        let (x1, y1) = ((l.off[0] + l.w as i32).max(dw), (l.off[1] + l.h as i32).max(dh));
        let (nw, nh) = ((x1 - x0) as usize, (y1 - y0) as usize);
        if nw == l.w && nh == l.h {
            return;
        }
        let mut px = vec![0u8; nw * nh * 4];
        for y in 0..l.h {
            let dst = ((y as i32 + l.off[1] - y0) as usize * nw + (l.off[0] - x0) as usize) * 4;
            px[dst..dst + l.w * 4].copy_from_slice(&l.px[y * l.w * 4..(y + 1) * l.w * 4]);
        }
        (l.px, l.w, l.h, l.off) = (px, nw, nh, [x0, y0]);
    }

    fn painting_mask(&self) -> bool {
        self.group_mask_target().is_some() || (self.edit_mask && self.sel_group.is_none() && self.doc.layers[self.doc.active].mask.is_some())
    }

    /// Ajoute un masque au calque actif : tout montrer, ou montrer (ou cacher) la sélection
    fn add_mask(&mut self, from_sel: Option<bool>) {
        self.checkpoint();
        let (w, h) = (self.doc.w, self.doc.h);
        let mut m = Layer::new_mask(w, h, if from_sel.is_some() { 0 } else { 255 });
        if let (Some(reveal), Some(sel)) = (from_sel, &self.sel) {
            for y in 0..h {
                for x in 0..w {
                    let k = sel.at(w, x as i32, y as i32);
                    let v = if reveal { k } else { 1.0 - k };
                    let i = (y * w + x) * 4;
                    let g = (v * 255.0).round() as u8;
                    m.px[i..i + 3].copy_from_slice(&[g, g, g]);
                }
            }
        } else if from_sel.is_some() {
            m = Layer::new_mask(w, h, 255);
        }
        let l = &mut self.doc.layers[self.doc.active];
        l.mask = Some(m);
        l.mask_on = true;
        self.edit_mask = true;
        self.all();
    }

    /// Nouveau calque de réglage au-dessus du calque actif, avec un masque blanc (comme Photoshop)
    fn new_adjustment(&mut self, a: Adj) {
        self.checkpoint();
        let l = Layer { adj: Some(a), mask: Some(Layer::new_mask(self.doc.w, self.doc.h, 255)), group: self.doc.layers[self.doc.active].group, ..Layer::blank(&crate::i18n::t(a.name()), 1, 1) };
        self.doc.layers.insert(self.doc.active + 1, l);
        self.doc.active += 1;
        self.edit_mask = true;
        self.all();
    }

    /// Applique le masque aux pixels du calque (et le retire)
    fn apply_mask(&mut self) {
        self.checkpoint();
        let l = &mut self.doc.layers[self.doc.active];
        if l.mask.is_some() {
            let keep = l.mask_on;
            l.mask_on = true;
            for ly in 0..l.h {
                for lx in 0..l.w {
                    let k = l.mask_at(lx as i32 + l.off[0], ly as i32 + l.off[1]);
                    let i = (ly * l.w + lx) * 4 + 3;
                    l.px[i] = (l.px[i] as f32 * if keep { k } else { 1.0 }).round() as u8;
                }
            }
            l.mask = None;
        }
        self.edit_mask = false;
        self.mask_view = false;
        self.all();
    }

    /// Vignettes 40 × 40 (proportions du document) sur un damier ; recalculées au plus quatre fois par seconde
    fn refresh_thumbs(&mut self, ctx: &egui::Context) {
        if !self.thumbs_due || self.thumbs_at.is_some_and(|t| t.elapsed().as_millis() < 250) || self.stroke.is_some() {
            return;
        }
        self.thumbs_due = false;
        self.thumbs_at = Some(Instant::now());
        let (dw, dh) = (self.doc.w.max(1), self.doc.h.max(1));
        let k = 40.0 / dw.max(dh) as f32;
        let (tw, th) = (((dw as f32 * k).round() as usize).max(1), ((dh as f32 * k).round() as usize).max(1));
        self.thumbs.retain(|(i, _), _| *i < self.doc.layers.len());
        for (i, l) in self.doc.layers.iter().enumerate() {
            for part in [false, true] {
                let src: &Layer = match (part, &l.mask) {
                    (false, _) => l,
                    (true, Some(m)) => m,
                    (true, None) => {
                        self.thumbs.remove(&(i, true));
                        continue;
                    }
                };
                let mut img = egui::ColorImage::new([tw, th], vec![Color32::WHITE; tw * th]);
                for y in 0..th {
                    for x in 0..tw {
                        let (dx, dy) = (((x as f32 + 0.5) / k) as i32, ((y as f32 + 0.5) / k) as i32);
                        let c = src.at(dx, dy);
                        let chk = if ((x / 4) + (y / 4)) % 2 == 0 { 255.0 } else { 204.0 };
                        let a = c[3] as f32 / 255.0;
                        let px = |v: u8| (v as f32 * a + chk * (1.0 - a)) as u8;
                        img.pixels[y * tw + x] = Color32::from_rgb(px(c[0]), px(c[1]), px(c[2]));
                    }
                }
                match self.thumbs.get_mut(&(i, part)) {
                    Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                    None => {
                        let t = ctx.load_texture(format!("paint-thumb-{i}-{part}"), img, egui::TextureOptions::LINEAR);
                        self.thumbs.insert((i, part), t);
                    }
                }
            }
        }
    }

    /// Met le calque actif dans un nouveau groupe (Ctrl+G)
    fn group_active(&mut self) {
        self.checkpoint();
        let id = self.doc.groups.iter().map(|g| g.id).max().unwrap_or(0) + 1;
        let n = self.doc.groups.len() + 1;
        // Dans le groupe du calque : le nouveau dossier s'y emboîte
        let parent = self.doc.layers[self.doc.active].group;
        self.doc.groups.push(Group::new(id, format!("{} {n}", crate::i18n::t("Groupe")), parent));
        self.doc.layers[self.doc.active].group = id;
        self.sel_group = Some(id);
        self.all();
    }

    /// Dissout un groupe ; avec `delete`, ses calques partent aussi
    fn ungroup(&mut self, id: u32, delete: bool) {
        self.checkpoint();
        let parent = self.doc.groups.iter().find(|g| g.id == id).map(|g| g.parent).unwrap_or(0);
        if delete {
            // Le groupe, ses sous-groupes et tous leurs calques (il reste au moins un calque)
            let inside: Vec<bool> = self.doc.layers.iter().map(|l| l.group != 0 && self.doc.chain(l.group).contains(&id)).collect();
            if inside.iter().any(|x| !x) {
                let mut k = 0;
                self.doc.layers.retain(|_| {
                    k += 1;
                    !inside[k - 1]
                });
            }
            let gone: Vec<u32> = self.doc.groups.iter().filter(|g| self.doc.chain(g.id).contains(&id)).map(|g| g.id).collect();
            self.doc.groups.retain(|g| !gone.contains(&g.id));
            for l in &mut self.doc.layers {
                if gone.contains(&l.group) {
                    l.group = parent;
                }
            }
        }
        // Ses calques et sous-groupes remontent d'un cran
        for l in &mut self.doc.layers {
            if l.group == id {
                l.group = parent;
            }
        }
        for g in &mut self.doc.groups {
            if g.parent == id {
                g.parent = parent;
            }
        }
        self.doc.groups.retain(|g| g.id != id);
        self.doc.active = self.doc.active.min(self.doc.layers.len() - 1);
        self.sel_group = None;
        self.all();
    }

    fn layers_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let ctx = ui.ctx().clone();
        self.refresh_thumbs(&ctx);
        ui.label(crate::theme::section(&crate::i18n::t("CALQUES")));
        let a = self.doc.active;
        let mut changed = false;
        // Comme Photoshop : mode de fusion et opacité, puis verrous et fond
        if let Some(g) = self.sel_group.and_then(|id| self.doc.groups.iter_mut().find(|g| g.id == id)) {
            // Groupe choisi : son mode (« Traverser » par défaut) et son opacité
            ui.horizontal(|ui| {
                let cur = g.blend.map(|b| b.name()).unwrap_or("Traverser");
                egui::ComboBox::from_id_salt("paint-group-blend").selected_text(crate::i18n::t(cur)).width(118.0).show_ui(ui, |ui| {
                    if ui.selectable_label(g.blend.is_none(), crate::i18n::t("Traverser")).clicked() {
                        g.blend = None;
                        changed = true;
                    }
                    for item in Blend::MENU {
                        match item {
                            Some((b, name)) => {
                                if ui.selectable_label(g.blend == Some(b), crate::i18n::t(name)).clicked() {
                                    g.blend = Some(b);
                                    changed = true;
                                }
                            }
                            None => {
                                ui.separator();
                            }
                        }
                    }
                });
                let mut o = (g.opacity * 100.0).round();
                if ui.add(egui::DragValue::new(&mut o).range(0.0..=100.0).speed(0.5).suffix(" %").prefix(crate::i18n::t("Opacité : "))).changed() {
                    g.opacity = o / 100.0;
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(crate::i18n::t("Nom :")).size(11.5).color(p.muted));
                changed |= ui.add(egui::TextEdit::singleline(&mut g.name).desired_width(160.0)).changed();
            });
        } else {
            let l = &mut self.doc.layers[a];
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("paint-blend").selected_text(crate::i18n::t(l.blend.name())).width(118.0).show_ui(ui, |ui| {
                    for item in Blend::MENU {
                        match item {
                            Some((b, name)) => {
                                if ui.selectable_label(l.blend == b, crate::i18n::t(name)).clicked() {
                                    l.blend = b;
                                    changed = true;
                                }
                            }
                            None => {
                                ui.separator();
                            }
                        }
                    }
                });
                let mut o = (l.opacity * 100.0).round();
                if ui.add(egui::DragValue::new(&mut o).range(0.0..=100.0).speed(0.5).suffix(" %").prefix(crate::i18n::t("Opacité : "))).changed() {
                    l.opacity = o / 100.0;
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(crate::i18n::t("Verrou :")).size(11.5).color(p.muted));
                let lock = |ui: &mut egui::Ui, on: &mut bool, icon: &str, tip: &str| -> bool {
                    let r = ui.add(egui::Button::new(egui::RichText::new(icon).size(12.0).color(if *on { p.on_accent } else { p.text })).fill(if *on { p.accent } else { Color32::TRANSPARENT }).min_size(vec2(22.0, 20.0))).on_hover_text(crate::i18n::t(tip));
                    if r.clicked() {
                        *on = !*on;
                    }
                    r.clicked()
                };
                changed |= lock(ui, &mut l.lock_alpha, crate::lucide::GRID_2X2, "Verrouiller les pixels transparents");
                changed |= lock(ui, &mut l.lock_all, crate::lucide::LOCK, "Tout verrouiller");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut f = (l.fill * 100.0).round();
                    if ui.add(egui::DragValue::new(&mut f).range(0.0..=100.0).speed(0.5).suffix(" %").prefix(crate::i18n::t("Fond : "))).changed() {
                        l.fill = f / 100.0;
                        changed = true;
                    }
                });
            });
            // Fusion conditionnelle (pour les calques de pixels)
            if l.adj.is_none() {
                ui.horizontal(|ui| {
                    let mut on = l.fx.blend_if.is_some();
                    if ui.checkbox(&mut on, crate::i18n::t("fusion conditionnelle")).on_hover_text(crate::i18n::t("Montrer le calque seulement sur les tons de dessous choisis (Blend If) : sans masque, par la luminosité")).changed() {
                        l.fx.blend_if = on.then_some([0.0, 1.0]);
                        changed = true;
                    }
                });
                if let Some(b) = l.fx.blend_if.as_mut() {
                    changed |= ui.add(egui::Slider::new(&mut b[0], 0.0..=1.0).text(crate::i18n::t("à partir des tons"))).changed();
                    changed |= ui.add(egui::Slider::new(&mut b[1], 0.0..=1.0).text(crate::i18n::t("jusqu'aux tons"))).changed();
                    if b[0] > b[1] {
                        b.swap(0, 1);
                    }
                }
            }
        }
        // Propriétés du calque de réglage choisi
        if let Some(adj) = self.doc.layers[a].adj.as_mut() {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(crate::i18n::t(adj.name())).size(12.0).strong());
            let sl = |ui: &mut egui::Ui, v: &mut f32, r: std::ops::RangeInclusive<f32>, label: &str| ui.add(egui::Slider::new(v, r).text(crate::i18n::t(label))).changed();
            changed |= match adj {
                Adj::BrightContrast { b, c } => sl(ui, b, -1.0..=1.0, "luminosité") | sl(ui, c, -1.0..=1.0, "contraste"),
                Adj::HueSat { h, s, l } => sl(ui, h, -0.5..=0.5, "teinte") | sl(ui, s, -1.0..=1.0, "saturation") | sl(ui, l, -1.0..=1.0, "luminosité"),
                Adj::Levels { black, white, gamma } => sl(ui, black, 0.0..=0.99, "noirs") | sl(ui, gamma, 0.1..=3.0, "gamma") | sl(ui, white, 0.01..=1.0, "blancs"),
                Adj::Exposure { ev } => sl(ui, ev, -4.0..=4.0, "exposition (IL)"),
                Adj::Threshold { t } => sl(ui, t, 0.0..=1.0, "seuil"),
                Adj::Posterize { levels } => sl(ui, levels, 2.0..=32.0, "niveaux"),
                Adj::Grain { amount, size } => sl(ui, amount, 0.0..=1.0, "quantité") | sl(ui, size, 0.5..=6.0, "taille du grain"),
                Adj::Clarity { amount } => sl(ui, amount, -1.0..=1.0, "clarté"),
                Adj::Dehaze { amount } => sl(ui, amount, -0.6..=0.6, "voile (− : ajouter de la brume)"),
                Adj::Vibrance { amount } => sl(ui, amount, -1.0..=1.0, "vibrance"),
                Adj::Invert | Adj::BlackWhite => false,
            };
        }
        if changed {
            self.all();
        }
        ui.separator();
        // La liste : le calque du haut en premier ; chaque ligne a son œil, sa vignette, celle de son masque et son nom
        let mut toggle = None;
        let mut pick: Option<(usize, bool)> = None;
        let mut mask_toggle = None;
        let mut mask_view = None;
        let mut menu_act: Option<(usize, &'static str)> = None;
        let mut drop_at: Option<usize> = None;
        let mut group_act: Option<(u32, &'static str)> = None;
        let row_h = 46.0;
        let n = self.doc.layers.len();
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let released = ui.input(|i| i.pointer.any_released());
        egui::ScrollArea::vertical().id_salt("paint-layer-list").max_height(ui.available_height() - 40.0).show(ui, |ui| {
            for i in (0..n).rev() {
                let gid = self.doc.layers[i].group;
                // Les dossiers où l'on entre à ce calque (en lisant de haut en bas) : une ligne chacun, en retrait
                let chain = self.doc.chain(gid);
                let above = if i + 1 < n { self.doc.chain(self.doc.layers[i + 1].group) } else { vec![] };
                let common = chain.iter().zip(above.iter()).take_while(|(a, b)| a == b).count();
                let folded = |id: u32| self.doc.groups.iter().any(|g| g.id == id && g.collapsed);
                for depth in common..chain.len() {
                    if chain[..depth].iter().any(|&c| folded(c)) {
                        break;
                    }
                    let hg = chain[depth];
                    let Some(g) = self.doc.groups.iter().find(|g| g.id == hg).cloned() else { continue };
                    let dx = depth as f32 * 18.0;
                    let (row, gresp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), egui::Sense::click());
                    let painter = ui.painter_at(row);
                    let gon = self.sel_group == Some(hg);
                    painter.rect_filled(row, 2.0, if gon { p.accent.gamma_multiply(0.28) } else if gresp.hovered() { p.raised } else { p.panel });
                    painter.hline(row.x_range(), row.bottom() - 0.5, Stroke::new(1.0, p.line.gamma_multiply(0.7)));
                    let eye = egui::Rect::from_min_size(egui::pos2(row.left() + 4.0, row.center().y - 10.0), vec2(20.0, 20.0));
                    painter.text(eye.center(), egui::Align2::CENTER_CENTER, if g.visible { crate::lucide::EYE } else { "" }, egui::FontId::proportional(13.0), p.muted);
                    painter.rect_stroke(eye.shrink(1.0), 1.0, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
                    if ui.interact(eye, ui.scope_id().with(("geye", hg)), egui::Sense::click()).clicked() {
                        group_act = Some((hg, "eye"));
                    }
                    let caret = egui::Rect::from_min_size(egui::pos2(row.left() + 30.0 + dx, row.center().y - 10.0), vec2(16.0, 20.0));
                    painter.text(caret.center(), egui::Align2::CENTER_CENTER, if g.collapsed { crate::lucide::CHEVRON_RIGHT } else { crate::lucide::CHEVRON_DOWN }, egui::FontId::proportional(12.0), p.text);
                    if ui.interact(caret, ui.scope_id().with(("gcaret", hg)), egui::Sense::click()).clicked() {
                        group_act = Some((hg, "fold"));
                    }
                    painter.text(egui::pos2(row.left() + 50.0 + dx, row.center().y), egui::Align2::LEFT_CENTER, crate::lucide::FOLDER, egui::FontId::proportional(14.0), p.text);
                    painter.text(egui::pos2(row.left() + 72.0 + dx, row.center().y), egui::Align2::LEFT_CENTER, &g.name, egui::FontId::proportional(12.5), if g.visible { p.text } else { p.muted });
                    // Masque du groupe : sa vignette, cliquable pour le peindre ; barrée s'il est désactivé
                    if let Some(m) = g.mask.0.as_deref() {
                        let mr = egui::Rect::from_min_size(egui::pos2(row.right() - 34.0, row.center().y - 10.0), vec2(28.0, 20.0));
                        let editing = gon && self.edit_mask;
                        painter.rect_filled(mr, 1.0, Color32::BLACK);
                        // Aperçu en 8 × 4 cases du masque
                        for cy in 0..4 {
                            for cx in 0..8 {
                                let (sx, sy) = ((cx * m.w / 8 + m.w / 16) as i32, (cy * m.h / 4 + m.h / 8) as i32);
                                let v = m.at(sx + m.off[0], sy + m.off[1])[0];
                                let cell = egui::Rect::from_min_size(mr.min + vec2(cx as f32 * mr.width() / 8.0, cy as f32 * mr.height() / 4.0), vec2(mr.width() / 8.0, mr.height() / 4.0));
                                painter.rect_filled(cell, 0.0, Color32::from_gray(v));
                            }
                        }
                        painter.rect_stroke(mr, 1.0, Stroke::new(if editing { 2.0 } else { 1.0 }, if editing { p.accent } else { p.line }), egui::StrokeKind::Outside);
                        if !g.mask_on {
                            painter.line_segment([mr.left_top(), mr.right_bottom()], Stroke::new(1.5, Color32::from_rgb(220, 60, 60)));
                        }
                        let mresp = ui.interact(mr, ui.scope_id().with(("gmask", hg)), egui::Sense::click()).on_hover_text(crate::i18n::t("Masque du groupe : cliquer pour le peindre, Maj+clic pour l'activer ou le désactiver"));
                        if mresp.clicked() {
                            group_act = Some((hg, if ui.input(|k| k.modifiers.shift) { "mask_toggle" } else { "mask_edit" }));
                        }
                    }
                    if gresp.clicked() && group_act.is_none() {
                        group_act = Some((hg, "pick"));
                    }
                    let has_mask = g.mask.0.is_some();
                    gresp.context_menu(|ui| {
                        if !has_mask && ui.button(format!("{}  {}", crate::lucide::SQUARE_DASHED, crate::i18n::t("Ajouter un masque au groupe"))).clicked() {
                            group_act = Some((hg, "mask_add"));
                            ui.close();
                        }
                        if has_mask && ui.button(format!("{}  {}", crate::lucide::EYE_OFF, crate::i18n::t("Activer / désactiver le masque"))).clicked() {
                            group_act = Some((hg, "mask_toggle"));
                            ui.close();
                        }
                        if has_mask && ui.button(format!("{}  {}", crate::lucide::TRASH_2, crate::i18n::t("Supprimer le masque"))).clicked() {
                            group_act = Some((hg, "mask_del"));
                            ui.close();
                        }
                        ui.separator();
                        if ui.button(format!("{}  {}", crate::lucide::UNGROUP, crate::i18n::t("Dissocier le groupe"))).clicked() {
                            group_act = Some((hg, "ungroup"));
                            ui.close();
                        }
                        if ui.button(format!("{}  {}", crate::lucide::TRASH_2, crate::i18n::t("Supprimer le groupe et ses calques"))).clicked() {
                            group_act = Some((hg, "delete"));
                            ui.close();
                        }
                    });
                }
                if chain.iter().any(|&c| folded(c)) {
                    continue;
                }
                let indent = chain.len() as f32 * 18.0;
                let on = i == self.doc.active && self.sel_group.is_none();
                let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), egui::Sense::click_and_drag());
                let painter = ui.painter_at(row);
                if on {
                    painter.rect_filled(row, 2.0, p.accent.gamma_multiply(0.28));
                } else if resp.hovered() {
                    painter.rect_filled(row, 2.0, p.raised);
                }
                painter.hline(row.x_range(), row.bottom() - 0.5, Stroke::new(1.0, p.line.gamma_multiply(0.7)));
                // Glisser une ligne : un trait montre où elle ira
                if resp.drag_started() {
                    self.drag_layer = Some(i);
                }
                if let (Some(_), Some(pt)) = (self.drag_layer, pointer) {
                    if row.contains(pt) {
                        let above = pt.y < row.center().y;
                        let y = if above { row.top() } else { row.bottom() };
                        ui.painter().hline(row.x_range(), y, Stroke::new(2.0, p.accent));
                        if released {
                            // Au-dessus d'une ligne = plus haut dans la pile
                            drop_at = Some(if above { i + 1 } else { i });
                        }
                    }
                }
                let l = &self.doc.layers[i];
                let mut x = row.left() + 4.0 + indent;
                // Œil
                let eye = egui::Rect::from_min_size(egui::pos2(x, row.center().y - 10.0), vec2(20.0, 20.0));
                let er = ui.interact(eye, ui.scope_id().with(("eye", i)), egui::Sense::click());
                painter.text(eye.center(), egui::Align2::CENTER_CENTER, if l.visible { crate::lucide::EYE } else { "" }, egui::FontId::proportional(13.0), if er.hovered() { p.text } else { p.muted });
                painter.rect_stroke(eye.shrink(1.0), 1.0, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
                if er.clicked() {
                    toggle = Some(i);
                }
                x += 26.0;
                // Écrêtage : petite flèche coudée et retrait
                if l.clip {
                    painter.text(egui::pos2(x + 4.0, row.center().y), egui::Align2::LEFT_CENTER, "↳", egui::FontId::proportional(14.0), p.muted);
                    x += 16.0;
                }
                // Vignettes : calque, puis masque (avec la chaîne)
                let thumb = |painter: &egui::Painter, x: f32, key: (usize, bool), framed: bool| -> egui::Rect {
                    let r = egui::Rect::from_min_size(egui::pos2(x, row.center().y - 18.0), vec2(36.0, 36.0));
                    painter.rect_filled(r, 0.0, Color32::from_gray(60));
                    if let Some(t) = self.thumbs.get(&key) {
                        let sz = t.size_vec2();
                        let k = (36.0 / sz.x.max(sz.y)).min(36.0);
                        let ir = egui::Rect::from_center_size(r.center(), sz * k);
                        painter.image(t.id(), ir, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    if framed {
                        painter.rect_stroke(r.expand(2.0), 0.0, Stroke::new(1.5, p.text), egui::StrokeKind::Outside);
                    }
                    r
                };
                let editing_mask = on && self.edit_mask && l.mask.is_some();
                let lr = if l.adj.is_some() {
                    // Calque de réglage : son icône à la place de la vignette
                    let r = egui::Rect::from_min_size(egui::pos2(x, row.center().y - 18.0), vec2(36.0, 36.0));
                    painter.rect_filled(r, 0.0, p.raised);
                    painter.text(r.center(), egui::Align2::CENTER_CENTER, crate::lucide::CIRCLE_HALF, egui::FontId::proportional(18.0), p.text);
                    r
                } else {
                    thumb(&painter, x, (i, false), on && !editing_mask)
                };
                let lresp = ui.interact(lr, ui.scope_id().with(("lthumb", i)), egui::Sense::click());
                if lresp.clicked() {
                    pick = Some((i, false));
                }
                x += 40.0;
                if l.mask.is_some() {
                    painter.text(egui::pos2(x + 4.0, row.center().y), egui::Align2::CENTER_CENTER, crate::lucide::LINK, egui::FontId::proportional(10.0), p.muted);
                    x += 10.0;
                    let mr = thumb(&painter, x, (i, true), editing_mask);
                    if !l.mask_on {
                        // Masque désactivé : croix rouge, comme dans Photoshop
                        let red = Stroke::new(2.0, Color32::from_rgb(220, 40, 40));
                        painter.line_segment([mr.left_top(), mr.right_bottom()], red);
                        painter.line_segment([mr.right_top(), mr.left_bottom()], red);
                    }
                    let mresp = ui.interact(mr, ui.scope_id().with(("mthumb", i)), egui::Sense::click()).on_hover_text(crate::i18n::t("Clic : peindre dans le masque · Alt+clic : voir le masque seul · Maj+clic : activer ou désactiver"));
                    if mresp.clicked() {
                        let (alt, shift) = ui.input(|k| (k.modifiers.alt, k.modifiers.shift));
                        if shift {
                            mask_toggle = Some(i);
                        } else if alt {
                            mask_view = Some(i);
                        } else {
                            pick = Some((i, true));
                        }
                    }
                    x += 40.0;
                }
                x += 4.0;
                // Nom (double-clic : renommer)
                let name_r = egui::Rect::from_min_max(egui::pos2(x, row.top()), egui::pos2(row.right() - 4.0, row.bottom()));
                if self.renaming == Some(i) {
                    let mut name = self.doc.layers[i].name.clone();
                    let mut cu = ui.new_child(egui::UiBuilder::new().max_rect(name_r).layout(egui::Layout::left_to_right(egui::Align::Center)));
                    let r = cu.add(egui::TextEdit::singleline(&mut name).desired_width(name_r.width()));
                    r.request_focus();
                    self.doc.layers[i].name = name;
                    if r.lost_focus() {
                        self.renaming = None;
                        self.save_due = Some(Instant::now());
                    }
                } else {
                    let l = &self.doc.layers[i];
                    let font = egui::FontId::proportional(12.5);
                    let col = if l.visible { p.text } else { p.muted };
                    let galley = painter.layout(l.name.clone(), font, col, name_r.width());
                    painter.galley(egui::pos2(name_r.left(), row.center().y - galley.size().y / 2.0), galley, col);
                    if l.lock_all || l.lock_alpha {
                        painter.text(egui::pos2(row.right() - 8.0, row.center().y), egui::Align2::RIGHT_CENTER, crate::lucide::LOCK, egui::FontId::proportional(11.0), p.muted);
                    }
                    if l.fx.active() {
                        // « fx » comme dans Photoshop ; clic : ouvrir les styles
                        let fr = egui::Rect::from_center_size(egui::pos2(row.right() - 26.0, row.center().y), vec2(18.0, 16.0));
                        painter.text(fr.center(), egui::Align2::CENTER_CENTER, "fx", egui::FontId::proportional(11.5), p.text);
                        if ui.interact(fr, ui.scope_id().with(("fxbadge", i)), egui::Sense::click()).clicked() {
                            self.fx_window = Some(i);
                        }
                    }
                }
                if resp.clicked() && pick.is_none() && toggle.is_none() {
                    pick = Some((i, false));
                }
                if resp.double_clicked() {
                    self.renaming = Some(i);
                }
                // Clic droit : le menu du calque
                resp.context_menu(|ui| {
                    let has_mask = self.doc.layers[i].mask.is_some();
                    let items: Vec<(&'static str, bool)> = vec![
                        ("Dupliquer le calque", true),
                        ("Supprimer le calque", n > 1),
                        ("Fusionner avec le calque inférieur", i > 0),
                        ("", true),
                        ("Ajouter un masque (tout montrer)", !has_mask),
                        ("Ajouter un masque (montrer la sélection)", !has_mask && self.sel.is_some()),
                        ("Ajouter un masque (cacher la sélection)", !has_mask && self.sel.is_some()),
                        ("Activer / désactiver le masque", has_mask),
                        ("Appliquer le masque", has_mask),
                        ("Supprimer le masque", has_mask),
                        ("", true),
                        (if self.doc.layers[i].clip { "Annuler le masque d'écrêtage" } else { "Créer un masque d'écrêtage" }, i > 0),
                        ("Options de fusion (styles)…", self.doc.layers[i].adj.is_none()),
                        ("", true),
                        ("Grouper le calque (Ctrl+G)", true),
                        ("Sortir du groupe", self.doc.layers[i].group != 0),
                    ];
                    for (label, enabled) in items {
                        if label.is_empty() {
                            ui.separator();
                        } else if ui.add_enabled(enabled, egui::Button::new(crate::i18n::t(label))).clicked() {
                            menu_act = Some((i, label));
                            ui.close();
                        }
                    }
                });
            }
        });
        if released {
            if let (Some(from), Some(to)) = (self.drag_layer, drop_at) {
                let to = if to > from { to - 1 } else { to };
                if to != from && to < n {
                    self.checkpoint();
                    let mut l = self.doc.layers.remove(from);
                    // Il rejoint le groupe de l'endroit où on le pose
                    let near = if to < self.doc.layers.len() { self.doc.layers[to].group } else { self.doc.layers.last().map(|x| x.group).unwrap_or(0) };
                    let below = if to > 0 { self.doc.layers[to - 1].group } else { 0 };
                    // Entre deux calques du même groupe : il y entre ; sinon il est hors groupe (le groupe reste d'un seul tenant)
                    l.group = if near == below { near } else { 0 };
                    self.doc.layers.insert(to, l);
                    self.doc.active = to;
                    self.all();
                }
            }
            self.drag_layer = None;
        }
        if let Some(i) = toggle {
            // Alt+clic sur l'œil : ne montrer que ce calque (ou tout remontrer)
            if ui.input(|k| k.modifiers.alt) {
                let solo = self.doc.layers.iter().enumerate().all(|(k, l)| l.visible == (k == i));
                for (k, l) in self.doc.layers.iter_mut().enumerate() {
                    l.visible = solo || k == i;
                }
            } else {
                self.doc.layers[i].visible = !self.doc.layers[i].visible;
            }
            self.all();
        }
        if let Some((gid, act)) = group_act {
            match act {
                "eye" => {
                    if let Some(g) = self.doc.groups.iter_mut().find(|g| g.id == gid) {
                        g.visible = !g.visible;
                    }
                    self.all();
                }
                "fold" => {
                    if let Some(g) = self.doc.groups.iter_mut().find(|g| g.id == gid) {
                        g.collapsed = !g.collapsed;
                    }
                }
                "pick" => {
                    self.sel_group = Some(gid);
                    self.edit_mask = false;
                }
                "mask_add" => {
                    self.checkpoint();
                    let (w, h) = (self.doc.w, self.doc.h);
                    if let Some(g) = self.doc.groups.iter_mut().find(|g| g.id == gid) {
                        g.mask = GroupMask(Some(Layer::new_mask(w, h, 255)));
                        g.mask_on = true;
                    }
                    self.sel_group = Some(gid);
                    self.edit_mask = true;
                    self.all();
                }
                "mask_edit" => {
                    self.sel_group = Some(gid);
                    self.edit_mask = true;
                }
                "mask_toggle" => {
                    self.checkpoint();
                    if let Some(g) = self.doc.groups.iter_mut().find(|g| g.id == gid) {
                        g.mask_on = !g.mask_on;
                    }
                    self.all();
                }
                "mask_del" => {
                    self.checkpoint();
                    if let Some(g) = self.doc.groups.iter_mut().find(|g| g.id == gid) {
                        g.mask = GroupMask(None);
                    }
                    self.edit_mask = false;
                    self.all();
                }
                "ungroup" => self.ungroup(gid, false),
                _ => self.ungroup(gid, true),
            }
        }
        if pick.is_some() {
            self.sel_group = None;
        }
        if let Some((i, mask)) = pick {
            if self.doc.active != i || self.edit_mask != mask {
                self.mask_view = false;
                self.all();
            }
            self.doc.active = i;
            // Un calque de réglage n'a que son masque à peindre
            self.edit_mask = mask || self.doc.layers[i].adj.is_some();
        }
        if let Some(i) = mask_toggle {
            self.checkpoint();
            self.doc.layers[i].mask_on = !self.doc.layers[i].mask_on;
            self.all();
        }
        if let Some(i) = mask_view {
            self.doc.active = i;
            self.edit_mask = true;
            self.mask_view = !self.mask_view;
            self.all();
        }
        if let Some((i, act)) = menu_act {
            self.doc.active = i;
            match act {
                "Dupliquer le calque" => self.duplicate_layer(),
                "Supprimer le calque" => self.delete_layer(),
                "Fusionner avec le calque inférieur" => self.merge_down(),
                "Ajouter un masque (tout montrer)" => self.add_mask(None),
                "Ajouter un masque (montrer la sélection)" => self.add_mask(Some(true)),
                "Ajouter un masque (cacher la sélection)" => self.add_mask(Some(false)),
                "Activer / désactiver le masque" => {
                    self.checkpoint();
                    self.doc.layers[i].mask_on ^= true;
                    self.all();
                }
                "Appliquer le masque" => self.apply_mask(),
                "Options de fusion (styles)…" => self.fx_window = Some(i),
                "Grouper le calque (Ctrl+G)" => self.group_active(),
                "Sortir du groupe" => {
                    self.checkpoint();
                    // Il sort au-dessus du groupe pour que le groupe reste d'un seul tenant
                    let gid = self.doc.layers[i].group;
                    let parent = self.doc.groups.iter().find(|g| g.id == gid).map(|g| g.parent).unwrap_or(0);
                    let top = (0..self.doc.layers.len()).rev().find(|&k| self.doc.chain(self.doc.layers[k].group).contains(&gid)).unwrap_or(i);
                    let mut l = self.doc.layers.remove(i);
                    l.group = parent;
                    let at = top.min(self.doc.layers.len());
                    self.doc.layers.insert(at, l);
                    self.doc.active = at;
                    if !self.doc.layers.iter().any(|x| x.group != 0 && self.doc.chain(x.group).contains(&gid)) {
                        self.doc.groups.retain(|g| g.id != gid);
                    }
                    self.all();
                }
                "Supprimer le masque" => {
                    self.checkpoint();
                    self.doc.layers[i].mask = None;
                    self.edit_mask = false;
                    self.mask_view = false;
                    self.all();
                }
                _ => {
                    self.checkpoint();
                    self.doc.layers[i].clip ^= true;
                    self.all();
                }
            }
        }
        // Fenêtre des styles du calque
        if let Some(i) = self.fx_window.filter(|&i| i < self.doc.layers.len()) {
            let mut open = true;
            let mut ch = false;
            let name = self.doc.layers[i].name.clone();
            open = crate::dock::os_window(&ctx, "paint-fx", &format!("{} · {name}", crate::i18n::t("Style de calque")), vec2(320.0, 520.0), |ui| {
                let fx = &mut self.doc.layers[i].fx;
                ch |= ui.checkbox(&mut fx.on, crate::i18n::t("Styles actifs")).changed();
                ui.separator();
                let rgb = |ui: &mut egui::Ui, c: &mut [u8; 3]| ui.color_edit_button_srgb(c).changed();
                let pct = |ui: &mut egui::Ui, v: &mut f32| {
                    let mut o = (*v * 100.0).round();
                    let r = ui.add(egui::DragValue::new(&mut o).range(0.0..=100.0).suffix(" %")).changed();
                    if r {
                        *v = o / 100.0;
                    }
                    r
                };
                let mut on = fx.shadow.is_some();
                if ui.checkbox(&mut on, crate::i18n::t("Ombre portée")).changed() {
                    fx.shadow = on.then_some(FxShadow { dx: 6.0, dy: 8.0, blur: 10.0, color: [0, 0, 0], opacity: 0.6 });
                    fx.on = true;
                    ch = true;
                }
                if let Some(sh) = &mut fx.shadow {
                    ui.horizontal(|ui| {
                        ch |= rgb(ui, &mut sh.color);
                        ch |= pct(ui, &mut sh.opacity);
                        ch |= ui.add(egui::DragValue::new(&mut sh.dx).speed(0.5).prefix("x ")).changed();
                        ch |= ui.add(egui::DragValue::new(&mut sh.dy).speed(0.5).prefix("y ")).changed();
                        ch |= ui.add(egui::DragValue::new(&mut sh.blur).speed(0.3).range(0.0..=200.0).prefix(crate::i18n::t("taille ")).suffix(" px")).changed();
                    });
                }
                let mut on = fx.glow.is_some();
                if ui.checkbox(&mut on, crate::i18n::t("Lueur externe")).changed() {
                    fx.glow = on.then_some(FxGlow { size: 14.0, color: [255, 236, 170], opacity: 0.75 });
                    fx.on = true;
                    ch = true;
                }
                if let Some(g) = &mut fx.glow {
                    ui.horizontal(|ui| {
                        ch |= rgb(ui, &mut g.color);
                        ch |= pct(ui, &mut g.opacity);
                        ch |= ui.add(egui::DragValue::new(&mut g.size).speed(0.3).range(1.0..=200.0).prefix(crate::i18n::t("taille ")).suffix(" px")).changed();
                    });
                }
                let mut on = fx.overlay.is_some();
                if ui.checkbox(&mut on, crate::i18n::t("Incrustation couleur")).changed() {
                    fx.overlay = on.then_some(FxOverlay { color: [220, 60, 60], opacity: 1.0 });
                    fx.on = true;
                    ch = true;
                }
                if let Some(o) = &mut fx.overlay {
                    ui.horizontal(|ui| {
                        ch |= rgb(ui, &mut o.color);
                        ch |= pct(ui, &mut o.opacity);
                    });
                }
                let mut on = fx.gradient.is_some();
                if ui.checkbox(&mut on, crate::i18n::t("Incrustation de dégradé")).changed() {
                    fx.gradient = on.then_some(FxGradient { a: [40, 30, 60], b: [240, 180, 120], angle: 90.0, opacity: 1.0 });
                    fx.on = true;
                    ch = true;
                }
                if let Some(g) = &mut fx.gradient {
                    ui.horizontal(|ui| {
                        ch |= rgb(ui, &mut g.a);
                        ch |= rgb(ui, &mut g.b);
                        ch |= pct(ui, &mut g.opacity);
                        ch |= ui.add(egui::DragValue::new(&mut g.angle).speed(1.0).range(-180.0..=180.0).suffix("°")).changed();
                    });
                }
                let mut on = fx.inner.is_some();
                if ui.checkbox(&mut on, crate::i18n::t("Ombre interne")).changed() {
                    fx.inner = on.then_some(FxShadow { dx: 4.0, dy: 5.0, blur: 8.0, color: [0, 0, 0], opacity: 0.6 });
                    fx.on = true;
                    ch = true;
                }
                if let Some(sh) = &mut fx.inner {
                    ui.horizontal(|ui| {
                        ch |= rgb(ui, &mut sh.color);
                        ch |= pct(ui, &mut sh.opacity);
                        ch |= ui.add(egui::DragValue::new(&mut sh.dx).speed(0.5).prefix("x ")).changed();
                        ch |= ui.add(egui::DragValue::new(&mut sh.dy).speed(0.5).prefix("y ")).changed();
                        ch |= ui.add(egui::DragValue::new(&mut sh.blur).speed(0.3).range(0.0..=200.0).prefix(crate::i18n::t("taille ")).suffix(" px")).changed();
                    });
                }
                let mut on = fx.bevel.is_some();
                if ui.checkbox(&mut on, crate::i18n::t("Biseau et estampage")).changed() {
                    fx.bevel = on.then_some(FxBevel { size: 8.0, depth: 1.0, angle: 120.0 });
                    fx.on = true;
                    ch = true;
                }
                if let Some(b) = &mut fx.bevel {
                    ui.horizontal(|ui| {
                        ch |= ui.add(egui::DragValue::new(&mut b.size).speed(0.2).range(1.0..=100.0).prefix(crate::i18n::t("taille ")).suffix(" px")).changed();
                        ch |= ui.add(egui::DragValue::new(&mut b.depth).speed(0.02).range(0.1..=5.0).prefix(crate::i18n::t("profondeur "))).changed();
                        ch |= ui.add(egui::DragValue::new(&mut b.angle).speed(1.0).range(-180.0..=180.0).prefix(crate::i18n::t("lumière ")).suffix("°")).changed();
                    });
                }
                let mut on = fx.stroke.is_some();
                if ui.checkbox(&mut on, crate::i18n::t("Contour")).changed() {
                    fx.stroke = on.then_some(FxStroke { size: 3.0, color: [0, 0, 0], opacity: 1.0 });
                    fx.on = true;
                    ch = true;
                }
                if let Some(st) = &mut fx.stroke {
                    ui.horizontal(|ui| {
                        ch |= rgb(ui, &mut st.color);
                        ch |= pct(ui, &mut st.opacity);
                        ch |= ui.add(egui::DragValue::new(&mut st.size).speed(0.2).range(1.0..=100.0).prefix(crate::i18n::t("taille ")).suffix(" px")).changed();
                    });
                }
            });
            if ch {
                self.all();
            }
            if !open {
                self.fx_window = None;
            }
        }
        // Barre du bas, comme Photoshop : masque, réglage, nouveau calque, supprimer
        ui.separator();
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let b = |ui: &mut egui::Ui, icon: &str, tip: &str| ui.add(egui::Button::new(egui::RichText::new(icon).size(14.0)).frame(false).min_size(vec2(24.0, 22.0))).on_hover_text(crate::i18n::t(tip));
                if b(ui, crate::lucide::TRASH_2, "Supprimer le calque").clicked() {
                    self.delete_layer();
                }
                if ui.add(egui::Button::new(egui::RichText::new("fx").size(12.0).italics().strong()).frame(false).min_size(vec2(24.0, 22.0))).on_hover_text(crate::i18n::t("Styles de calque : ombres, lueur, biseau, incrustations, contour")).clicked() && self.doc.layers[self.doc.active].adj.is_none() {
                    self.fx_window = Some(self.doc.active);
                }
                if b(ui, crate::lucide::FOLDER, "Grouper le calque (Ctrl+G)").clicked() {
                    self.group_active();
                }
                if b(ui, crate::lucide::SQUARE_PLUS, "Nouveau calque (Ctrl+Maj+N)").clicked() {
                    self.new_layer();
                }
                if b(ui, crate::lucide::COPY, "Dupliquer le calque (Ctrl+J)").clicked() {
                    self.duplicate_layer();
                }
                let mut new_adj = None;
                ui.menu_button(egui::RichText::new(crate::lucide::CIRCLE_HALF).size(14.0), |ui| {
                    ui.label(egui::RichText::new(crate::i18n::t("Nouveau calque de réglage")).size(11.0).color(p.muted));
                    for (name, a) in Adj::NEW {
                        if ui.button(crate::i18n::t(name)).clicked() {
                            new_adj = Some(a);
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text(crate::i18n::t("Nouveau calque de réglage (non destructif)"));
                if let Some(a) = new_adj {
                    self.new_adjustment(a);
                }
                let has_mask = self.doc.layers[self.doc.active].mask.is_some();
                if b(ui, crate::lucide::SQUARE_DASHED, "Ajouter un masque de fusion (d'après la sélection s'il y en a une)").clicked() && !has_mask {
                    let from_sel = self.sel.is_some().then_some(true);
                    self.add_mask(from_sel);
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masques_ecretage_et_modes() {
        let mut d = Doc::new(4, 1, true);
        // Un calque rouge avec un masque qui cache la moitié droite
        let mut r = Layer::filled("rouge", 4, 1, [255, 0, 0, 255]);
        let mut m = Layer::new_mask(4, 1, 255);
        for x in 2..4 {
            m.px[x * 4..x * 4 + 3].copy_from_slice(&[0, 0, 0]);
        }
        r.mask = Some(m);
        d.layers.push(r);
        let img = d.flatten();
        assert_eq!(img.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(3, 0).0, [255, 255, 255, 255], "le noir du masque cache");
        // Désactivé : tout se voit ; enregistré puis relu : le masque revient
        d.layers[1].mask_on = false;
        assert_eq!(d.flatten().get_pixel(3, 0).0, [255, 0, 0, 255]);
        let path = std::env::temp_dir().join(format!("masque-{}.aryimg", std::process::id()));
        d.save(&path).unwrap();
        let back = Doc::load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(back.layers[1].mask.is_some() && !back.layers[1].mask_on);
        // Écrêtage : un calque bleu n'apparaît que sur le pixel couvert du calque du dessous
        let mut e = Doc::new(2, 1, false);
        let mut base = Layer::blank("base", 2, 1);
        base.px[0..4].copy_from_slice(&[0, 255, 0, 255]);
        e.layers[0] = base;
        let mut blue = Layer::filled("bleu", 2, 1, [0, 0, 255, 255]);
        blue.clip = true;
        e.layers.push(blue);
        let img = e.flatten();
        assert_eq!(img.get_pixel(0, 0).0, [0, 0, 255, 255]);
        assert_eq!(img.get_pixel(1, 0).0[3], 0);
        // Réglage : inverser tout ce qui est dessous, sauf là où son masque est noir
        let mut g = Doc::new(2, 1, true);
        let mut inv = Layer { adj: Some(Adj::Invert), mask: Some(Layer::new_mask(2, 1, 255)), ..Layer::blank("inv", 1, 1) };
        inv.mask.as_mut().unwrap().px[4..7].copy_from_slice(&[0, 0, 0]);
        g.layers.push(inv);
        let path2 = std::env::temp_dir().join(format!("reglage-{}.aryimg", std::process::id()));
        g.save(&path2).unwrap();
        let g = Doc::load(&path2).unwrap();
        let _ = std::fs::remove_file(&path2);
        let img = g.flatten();
        assert_eq!(img.get_pixel(0, 0).0, [0, 0, 0, 255]);
        assert_eq!(img.get_pixel(1, 0).0, [255, 255, 255, 255]);
        // Groupe isolé à 50 % : le rouge du groupe est mélangé à moitié sur le blanc
        let mut h = Doc::new(1, 1, true);
        h.layers.push(Layer { group: 7, ..Layer::filled("r", 1, 1, [255, 0, 0, 255]) });
        h.groups.push(Group { opacity: 0.5, blend: Some(Blend::Normal), ..Group::new(7, "g".into(), 0) });
        assert_eq!(h.flatten().get_pixel(0, 0).0, [255, 128, 128, 255]);
        h.groups[0].visible = false;
        assert_eq!(h.flatten().get_pixel(0, 0).0, [255, 255, 255, 255]);
        // Styles : un contour noir apparaît autour d'un carré rouge, et l'ombre se pose dessous
        let mut st = Doc::new(12, 12, true);
        let mut sq = Layer::blank("carré", 12, 12);
        for y in 4..8 {
            for x in 4..8 {
                sq.px[(y * 12 + x) * 4..(y * 12 + x) * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        sq.fx = Fx { on: true, stroke: Some(FxStroke { size: 2.0, color: [0, 0, 0], opacity: 1.0 }), ..Default::default() };
        st.layers.push(sq);
        let img = st.flatten();
        assert_eq!(img.get_pixel(5, 5).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(3, 5).0, [0, 0, 0, 255], "contour autour");
        assert_eq!(img.get_pixel(0, 0).0, [255, 255, 255, 255]);
        // Modes : écran et luminosité
        assert_eq!(Blend::Screen.mix([0.5; 3], [0.5; 3]), [0.75; 3]);
        let l = Blend::Luminosity.mix([1.0, 0.0, 0.0], [0.5, 0.5, 0.5]);
        assert!((lum(l) - 0.5).abs() < 1e-4 && l[0] > l[1]);
    }

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

    #[test]
    fn selections_degrade_et_filtres() {
        let (w, h) = (40, 40);
        // Ellipse : le centre est pris, les coins non
        let e = Sel::ellipse(w, h, [0.0, 0.0], [40.0, 40.0]).unwrap();
        assert_eq!(e.at(w, 20, 20), 1.0);
        assert_eq!(e.at(w, 0, 0), 0.0);
        // Lasso en triangle
        let t = Sel::poly(w, h, &[[0.0, 0.0], [40.0, 0.0], [0.0, 40.0]]).unwrap();
        assert_eq!(t.at(w, 5, 5), 1.0);
        assert_eq!(t.at(w, 35, 35), 0.0);
        // Retirer l'ellipse du tout, puis inverser : on retombe sur l'ellipse
        let ring = Sel::all(w, h).combine(&e, true);
        assert_eq!(ring.at(w, 20, 20), 0.0);
        assert_eq!(ring.invert(w, h).at(w, 20, 20), 1.0);
        // Baguette : sur une image moitié noire moitié blanche, elle prend la moitié cliquée
        let comp: Vec<u8> = (0..w * h).flat_map(|i| if i % w < 20 { [0, 0, 0, 255] } else { [255, 255, 255, 255] }).collect();
        let m = Sel::wand(w, h, &comp, [5, 5], 0.1).unwrap();
        assert_eq!((m.at(w, 19, 30), m.at(w, 20, 30)), (1.0, 0.0));
        assert_eq!(m.bbox, [0, 0, 20, 40]);

        let path = std::env::temp_dir().join(format!("aryadne-paint3-{}.aryimg", std::process::id()));
        Doc::new(w, h, false).save(&path).unwrap();
        let mut ed = Editor::open(path.clone()).unwrap();
        ed.color = Color32::from_rgb(255, 0, 0);
        // Dégradé limité à la moitié gauche
        ed.sel = Some(m);
        ed.gradient(pos2(0.0, 0.0), pos2(40.0, 0.0));
        let a = |e: &Editor, x: usize, y: usize| e.doc.layers[0].px[(y * w + x) * 4 + 3];
        assert!(a(&ed, 0, 10) > 240 && a(&ed, 15, 10) > 0 && a(&ed, 15, 10) < a(&ed, 0, 10));
        assert_eq!(a(&ed, 30, 10), 0, "hors de la sélection");
        // Le flou adoucit le bord du dégradé dans la sélection, pas ailleurs
        ed.sel = None;
        ed.filter("blur", 6.0);
        assert!(a(&ed, 21, 10) > 0, "le flou déborde de l'ancien bord");
        ed.filter("pixelate", 8.0);
        assert_eq!(ed.doc.layers[0].px[0..4], ed.doc.layers[0].px[4..8]);
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(test)]
mod reglages_tests {
    use super::*;

    #[test]
    fn nouveaux_reglages_et_fusion_conditionnelle() {
        // Voile : un gris délavé redevient plus contrasté
        let d = Adj::Dehaze { amount: 0.3 }.apply([0.4, 0.5, 0.9]);
        assert!(d[0] < 0.4 && d[2] > 0.85);
        // Clarté : les tons moyens s'écartent, le noir et le blanc restent
        let c = Adj::Clarity { amount: 0.8 };
        assert_eq!(c.apply([0.0; 3]), [0.0; 3]);
        assert!(c.apply([0.7; 3])[0] > 0.7 && c.apply([0.3; 3])[0] < 0.3);
        // Grain : varie d'un pixel à l'autre, mais toujours pareil au même endroit
        let g = Adj::Grain { amount: 0.5, size: 1.0 };
        let (a, b) = (g.apply_at([0.5; 3], 3, 4), g.apply_at([0.5; 3], 40, 9));
        assert_ne!(a, b);
        assert_eq!(a, g.apply_at([0.5; 3], 3, 4));
        // Vibrance : une couleur terne gagne plus qu'une couleur vive
        let v = Adj::Vibrance { amount: 0.6 };
        let sat = |c: [f32; 3]| egui::ecolor::Hsva::from_rgb(c).s;
        assert!(sat(v.apply([0.5, 0.45, 0.4])) - sat([0.5, 0.45, 0.4]) > sat(v.apply([0.9, 0.1, 0.1])) - sat([0.9, 0.1, 0.1]));
        // Fusion conditionnelle : visible entre les bornes, fondu autour
        assert_eq!(blend_if_k(0.3, 0.7, 0.5), 1.0);
        assert_eq!(blend_if_k(0.3, 0.7, 0.1), 0.0);
        assert!(blend_if_k(0.3, 0.7, 0.26) > 0.0 && blend_if_k(0.3, 0.7, 0.26) < 1.0);
        assert_eq!(blend_if_k(0.0, 1.0, 0.0), 1.0);
    }

    #[test]
    fn remplissage_qui_ferme_les_trous() {
        // Un carré dessiné avec une ouverture de 2 px sur le côté droit, dans une image de 30 px
        let (w, h) = (30usize, 30usize);
        let mut open = vec![true; w * h];
        for k in 5..25 {
            for (x, y) in [(5, k), (24, k), (k, 5), (k, 24)] {
                open[y * w + x] = false;
            }
        }
        open[14 * w + 24] = true;
        open[15 * w + 24] = true;
        // Sans fermeture, le remplissage du dedans s'échappe ; avec 2 px, il reste dedans et touche le trait
        let r = gap_fill(&open, w, h, 15, 15, 2).unwrap();
        assert!(r[15 * w + 15] && r[15 * w + 6] && r[6 * w + 15]);
        assert!(!r[15 * w + 28] && !r[2 * w + 2]);
    }

    #[test]
    fn deformation_en_grille() {
        let mut l = Layer::blank("l", 30, 20);
        for (k, px) in l.px.chunks_mut(4).enumerate() {
            px.copy_from_slice(&[(k % 30 * 8) as u8, (k / 30 * 12) as u8, 90, 255]);
        }
        let n = WARP_N;
        let b = [0.0, 0.0, 30.0, 20.0];
        let g: Vec<[f32; 2]> = (0..n * n).map(|k| [30.0 * (k % n) as f32 / (n - 1) as f32, 20.0 * (k / n) as f32 / (n - 1) as f32]).collect();
        // Grille au repos : la surface est le cadre, l'image ne bouge pas
        let p = warp_point(&g, n, 0.37, 0.81);
        assert!((p[0] - 11.1).abs() < 0.01 && (p[1] - 16.2).abs() < 0.01, "{p:?}");
        let o = warp_layer(&l, b, &g, n);
        assert_eq!(o.off, [0, 0]);
        assert_eq!(o.at(13, 7), l.at(13, 7));
        // Un point tiré vers la droite : le pixel sous lui vient de plus à gauche
        let mut g2 = g.clone();
        g2[n + 1][0] += 6.0;
        let o2 = warp_layer(&l, b, &g2, n);
        let x = g2[n + 1][0] as i32;
        let y = g2[n + 1][1] as i32;
        assert!(o2.at(x, y)[0] < l.at(x, y)[0], "{:?} {:?}", o2.at(x, y), l.at(x, y));
    }

    #[test]
    fn biseau_ombre_interne_degrade() {
        // Un carré blanc de 20 px au milieu d'un calque de 40 px
        let mut l = Layer::blank("carré", 40, 40);
        for y in 10..30 {
            for x in 10..30 {
                let i = (y * 40 + x) * 4;
                l.px[i..i + 4].copy_from_slice(&[128, 128, 128, 255]);
            }
        }
        l.fx = Fx { on: true, bevel: Some(FxBevel { size: 6.0, depth: 1.0, angle: 135.0 }), inner: Some(FxShadow { dx: 3.0, dy: 3.0, blur: 3.0, color: [0, 0, 0], opacity: 1.0 }), ..Default::default() };
        l.bake_fx(40, 40);
        let m = l.fx_maps.clone().unwrap();
        // Lumière en haut à gauche : le bord haut-gauche s'éclaire, le bas-droit s'assombrit
        assert!(FxMaps::get(&m.bevel, 40, 11, 11) > 0.55, "{}", FxMaps::get(&m.bevel, 40, 11, 11));
        assert!(FxMaps::get(&m.bevel, 40, 28, 28) < 0.45);
        // Ombre interne : sous le bord haut-gauche, rien au centre
        assert!(FxMaps::get(&m.inner, 40, 11, 11) > 0.3);
        assert!(FxMaps::get(&m.inner, 40, 20, 20) < 0.05);
        // Dégradé : du noir au blanc de gauche à droite
        let mut g = Layer::blank("g", 40, 40);
        g.px.chunks_mut(4).for_each(|p| p.copy_from_slice(&[128, 128, 128, 255]));
        g.fx = Fx { on: true, gradient: Some(FxGradient { a: [0, 0, 0], b: [255, 255, 255], angle: 0.0, opacity: 1.0 }), ..Default::default() };
        let (mut left, mut right) = ([0.0f32; 4], [0.0f32; 4]);
        stack(&mut left, &[&g], 1, 20, 1.0);
        stack(&mut right, &[&g], 38, 20, 1.0);
        assert!(left[0] < 0.1 && right[0] > 0.9, "{left:?} {right:?}");
    }
}

#[cfg(test)]
mod text_tests {
    #[test]
    fn groupes_imbriques() {
        use super::*;
        // Blanc dessous ; un groupe A (traverser, masque qui cache la moitié gauche) contient B (isolé, 50 %) avec du rouge
        let mut d = Doc::new(4, 1, true);
        let mut red = Layer::filled("rouge", 4, 1, [255, 0, 0, 255]);
        red.group = 2;
        d.layers.push(red);
        let mut m = Layer::new_mask(4, 1, 255);
        for x in 0..2 {
            m.px[x * 4..x * 4 + 3].copy_from_slice(&[0, 0, 0]);
        }
        d.groups.push(Group { mask: GroupMask(Some(m)), ..Group::new(1, "A".into(), 0) });
        d.groups.push(Group { opacity: 0.5, blend: Some(Blend::Normal), ..Group::new(2, "B".into(), 1) });
        assert_eq!(d.chain(2), vec![1, 2]);
        let img = d.flatten();
        assert_eq!(img.get_pixel(0, 0).0, [255, 255, 255, 255]);
        let p = img.get_pixel(3, 0).0;
        assert!((p[1] as i32 - 128).abs() <= 2 && p[0] == 255, "{p:?}");
        // Aller-retour .aryimg : parent et masque reviennent
        let path = std::env::temp_dir().join(format!("ary-nest-{}.aryimg", std::process::id()));
        d.save(&path).unwrap();
        let back = Doc::load(&path).unwrap();
        assert_eq!(back.groups.iter().find(|g| g.id == 2).unwrap().parent, 1);
        assert!(back.groups.iter().find(|g| g.id == 1).unwrap().mask.0.is_some());
        assert_eq!(back.flatten().get_pixel(3, 0).0, p);
        // Aller-retour PSD : les dossiers restent emboîtés, le masque du dossier aussi
        let (bytes, _) = crate::psd::write(&d);
        if let Ok(o) = std::env::var("PSD_OUT_NEST") {
            std::fs::write(o, &bytes).unwrap();
        }
        let back = crate::psd::read(&bytes).unwrap();
        let (gb, ga) = (back.groups.iter().find(|g| g.name == "B").unwrap(), back.groups.iter().find(|g| g.name == "A").unwrap());
        assert_eq!(gb.parent, ga.id);
        assert!(ga.mask.0.is_some());
        let q = back.flatten().get_pixel(3, 0).0;
        assert!(q.iter().zip(p.iter()).all(|(a, b)| (*a as i32 - *b as i32).abs() <= 2), "{q:?} {p:?}");
        assert_eq!(back.flatten().get_pixel(0, 0).0, [255, 255, 255, 255]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tampon() {
        use super::*;
        let mut ed = Editor::new_test(20, 10);
        // Moitié gauche rouge, moitié droite transparente ; on copie la gauche vers la droite
        let l = &mut ed.doc.layers[ed.doc.active];
        for y in 0..l.h {
            for x in 0..10 {
                let i = (y * l.w + x) * 4;
                l.px[i..i + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        ed.tool = Tool::Clone;
        (ed.size, ed.hardness, ed.flow) = (4.0, 0.99, 1.0);
        ed.clone_off = Some(egui::vec2(-10.0, 0.0));
        let start = ed.doc.layers[ed.doc.active].clone();
        let n = start.w * start.h;
        ed.stroke = Some((start, vec![0.0; n], egui::pos2(15.0, 5.0)));
        ed.dab(egui::pos2(15.0, 5.0));
        assert_eq!(ed.doc.layers[ed.doc.active].at(15, 5), [255, 0, 0, 255]);
        assert_eq!(ed.doc.layers[ed.doc.active].at(19, 5)[3], 0);
    }

    #[test]
    fn transformation_libre() {
        use super::*;
        let mut l = Layer::blank("x", 4, 2);
        for p in l.px.chunks_mut(4) {
            p.copy_from_slice(&[200, 10, 10, 255]);
        }
        l.off = [10, 10];
        let b = content_box(&l).unwrap();
        assert_eq!(b, [10, 10, 14, 12]);
        let c0 = [12.0, 11.0];
        // Doubler la taille autour du centre : 8 × 4, même centre
        let big = transform_layer(&l, c0, [0.0; 2], [2.0, 2.0], 0.0);
        assert_eq!(content_box(&big).unwrap(), [8, 9, 16, 13]);
        assert_eq!(big.at(12, 11), [200, 10, 10, 255]);
        // Un quart de tour : 2 × 4
        let rot = transform_layer(&l, c0, [5.0, 0.0], [1.0, 1.0], std::f32::consts::FRAC_PI_2);
        assert_eq!(content_box(&rot).unwrap(), [16, 9, 18, 13]);
        // Le masque suit : un masque noir à gauche, blanc à droite, retourné, passe noir à droite
        let mut m = Layer::blank("m", 4, 2);
        m.off = [10, 10];
        for (i, p) in m.px.chunks_mut(4).enumerate() {
            let v = if i % 4 < 2 { 0 } else { 255 };
            p.copy_from_slice(&[v, v, v, 255]);
        }
        let flipped = transform_layer(&m, c0, [0.0; 2], [-1.0, 1.0], 0.0);
        assert!(flipped.at(10, 10)[0] > 200 && flipped.at(13, 10)[0] < 50);
        // Sans changement : identique
        let same = transform_layer(&l, c0, [0.0; 2], [1.0; 2], 0.0);
        assert_eq!(content_box(&same).unwrap(), b);
        assert_eq!(same.px.chunks(4).filter(|p| p[3] == 255).count(), 8);
    }

    #[test]
    fn texte_rendu() {
        let (px, w, h, off) = super::render_text("Ab\nc", "inter", 40.0, [200, 10, 10, 255]).unwrap();
        assert!(w > 20 && h > 60, "{w}x{h}");
        assert!(off[1] >= 0 && off[1] < 20, "le haut des lettres est près de l'ancrage : {off:?}");
        assert!(px.chunks_exact(4).any(|p| p == [200, 10, 10, 255]));
        assert!(super::render_text("   ", "inter", 40.0, [0; 4]).is_none());
    }
}
