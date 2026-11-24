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
    /// Identifiant stable (pour la parenté)
    #[serde(default = "new_id")]
    pub id: u64,
    /// Le calque suit ce parent (son identifiant) : sa position, son échelle et sa rotation s'ajoutent
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<u64>,
    #[serde(default, skip_serializing_if = "Style::is_plain")]
    pub style: Style,
}

pub fn new_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
    (t ^ (N.fetch_add(1, Ordering::Relaxed).wrapping_mul(0x9E3779B97F4A7C15))) | 1
}

/// Contour, ombre portée et lueur d'un calque
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct Style {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<StrokeStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<Shadow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glow: Option<Glow>,
}

impl Style {
    pub fn is_plain(&self) -> bool {
        self.stroke.is_none() && self.shadow.is_none() && self.glow.is_none()
    }

    /// Marge autour du contenu, en px du calque
    fn pad(&self) -> f32 {
        let s = self.stroke.as_ref().map(|s| s.width).unwrap_or(0.0);
        let sh = self.shadow.as_ref().map(|d| d.blur * 1.5 + d.dx.abs().max(d.dy.abs())).unwrap_or(0.0);
        let g = self.glow.as_ref().map(|g| g.radius * 1.5).unwrap_or(0.0);
        s + sh.max(g)
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct StrokeStyle {
    pub width: f32,
    pub color: [u8; 3],
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Shadow {
    pub dx: f32,
    pub dy: f32,
    pub blur: f32,
    pub color: [u8; 3],
    /// De 0 à 1
    pub opacity: f32,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Glow {
    pub radius: f32,
    pub color: [u8; 3],
    pub strength: f32,
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
            id: new_id(),
            parent: None,
            style: Style::default(),
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

    /// Passage du repère du calque (px, origine au centre) à celui de son parent (ou de la composition), à l'instant `t`
    pub fn xform(&self, t: f32) -> Xform {
        let p = self.pos.at(t);
        let s = self.scale.at(t);
        let r = self.rot.at(t)[0].to_radians();
        let (cs, sn) = (r.cos(), r.sin());
        let (sx, sy) = (s[0] / 100.0, s[1] / 100.0);
        Xform { a: cs * sx, b: sn * sx, c: -sn * sy, d: cs * sy, e: p[0], f: p[1] }
    }
}

/// Transformation affine : x' = a·x + c·y + e, y' = b·x + d·y + f
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Xform {
    pub const ID: Xform = Xform { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn apply(&self, l: [f32; 2]) -> [f32; 2] {
        [self.a * l[0] + self.c * l[1] + self.e, self.b * l[0] + self.d * l[1] + self.f]
    }

    fn det(&self) -> f32 {
        self.a * self.d - self.b * self.c
    }

    pub fn inverse(&self, w: [f32; 2]) -> Option<[f32; 2]> {
        let det = self.det();
        if det.abs() < 1e-9 {
            return None;
        }
        let (x, y) = (w[0] - self.e, w[1] - self.f);
        Some([(self.d * x - self.c * y) / det, (-self.b * x + self.a * y) / det])
    }

    pub fn inverse_vec(&self, v: [f32; 2]) -> Option<[f32; 2]> {
        let det = self.det();
        (det.abs() >= 1e-9).then(|| [(self.d * v[0] - self.c * v[1]) / det, (-self.b * v[0] + self.a * v[1]) / det])
    }

    /// self ∘ o : applique `o`, puis `self`
    pub fn then_parent(&self, o: &Xform) -> Xform {
        Xform {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            e: self.a * o.e + self.c * o.f + self.e,
            f: self.b * o.e + self.d * o.f + self.f,
        }
    }

    /// La plus grande échelle (pour choisir la résolution du rendu)
    pub fn max_scale(&self) -> f32 {
        self.a.hypot(self.b).max(self.c.hypot(self.d))
    }

    /// Rotation, en degrés
    pub fn angle(&self) -> f32 {
        self.b.atan2(self.a).to_degrees()
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

    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.layers.iter().position(|l| l.id == id)
    }

    /// Le repère du parent d'un calque dans la composition (l'identité s'il n'en a pas)
    pub fn parent_world(&self, i: usize, t: f32) -> Xform {
        let mut m = Xform::ID;
        let mut cur = self.layers.get(i).and_then(|l| l.parent);
        // Une chaîne de parents bornée : une boucle ne bloque rien
        for _ in 0..32 {
            let Some(j) = cur.and_then(|id| self.index_of(id)) else { break };
            m = self.layers[j].xform(t).then_parent(&m);
            cur = self.layers[j].parent;
        }
        m
    }

    /// Le repère d'un calque dans la composition, parents compris
    pub fn world(&self, i: usize, t: f32) -> Xform {
        self.parent_world(i, t).then_parent(&self.layers[i].xform(t))
    }

    /// Les calques qui descendent de `i` (enfants, petits-enfants…)
    pub fn descendants(&self, i: usize) -> Vec<usize> {
        let mut out = vec![];
        let mut stack = vec![self.layers[i].id];
        while let Some(id) = stack.pop() {
            for (k, l) in self.layers.iter().enumerate() {
                if l.parent == Some(id) && !out.contains(&k) {
                    out.push(k);
                    stack.push(l.id);
                }
            }
        }
        out
    }

    /// Donne un parent au calque `i` sans qu'il bouge à l'instant `t` (ses valeurs passent dans le repère du parent)
    pub fn set_parent(&mut self, i: usize, parent: Option<u64>, t: f32) {
        if parent.is_some_and(|p| Some(p) == self.layers.get(i).map(|l| l.id) || self.descendants(i).iter().any(|k| self.layers[*k].id == p)) {
            return;
        }
        let old = self.parent_world(i, t);
        self.layers[i].parent = parent;
        let new = self.parent_world(i, t);
        let rot = old.angle() - new.angle();
        let sc = old.max_scale() / new.max_scale().max(1e-6);
        let l = &mut self.layers[i];
        let conv = |v: [f32; 2]| new.inverse(old.apply(v)).unwrap_or(v);
        l.pos.v = conv(l.pos.v);
        l.pos.keys.iter_mut().for_each(|k| k.v = conv(k.v));
        l.rot.v[0] += rot;
        l.rot.keys.iter_mut().for_each(|k| k.v[0] += rot);
        l.scale.v = [l.scale.v[0] * sc, l.scale.v[1] * sc];
        l.scale.keys.iter_mut().for_each(|k| k.v = [k.v[0] * sc, k.v[1] * sc]);
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
pub fn corners(c: &Compo, i: usize, t: f32, cache: &mut Cache) -> [[f32; 2]; 4] {
    let [w, h] = size(&c.layers[i], cache);
    let x = c.world(i, t);
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
        c.world(i, t).inverse(at).is_some_and(|p| p[0].abs() <= w / 2.0 + pad && p[1].abs() <= h / 2.0 + pad)
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

/// Flou d'un masque (trois passes de moyenne glissante : presque une gaussienne)
fn blur(mask: &mut [f32], w: usize, h: usize, r: f32) {
    let r = (r / 2.0).round() as usize;
    if r == 0 {
        return;
    }
    let mut tmp = vec![0.0f32; mask.len()];
    for _ in 0..3 {
        for y in 0..h {
            let row = &mask[y * w..(y + 1) * w];
            let mut acc: f32 = row[..r.min(w)].iter().sum();
            for x in 0..w {
                if x + r < w {
                    acc += row[x + r];
                }
                if x > r {
                    acc -= row[x - r - 1];
                }
                tmp[y * w + x] = acc / (2 * r + 1) as f32;
            }
        }
        for x in 0..w {
            let mut acc: f32 = (0..r.min(h)).map(|y| tmp[y * w + x]).sum();
            for y in 0..h {
                if y + r < h {
                    acc += tmp[(y + r) * w + x];
                }
                if y > r {
                    acc -= tmp[(y - r - 1) * w + x];
                }
                mask[y * w + x] = acc / (2 * r + 1) as f32;
            }
        }
    }
}

/// Distance (en pixels) de chaque pixel au contenu le plus proche (passes de chanfrein 3-4)
fn distance(alpha: &[f32], w: usize, h: usize) -> Vec<f32> {
    let big = 1e9f32;
    let mut d: Vec<f32> = alpha.iter().map(|a| if *a >= 0.5 { 0.0 } else { big }).collect();
    let at = |d: &Vec<f32>, x: isize, y: isize| if x < 0 || y < 0 || x >= w as isize || y >= h as isize { big } else { d[y as usize * w + x as usize] };
    for y in 0..h as isize {
        for x in 0..w as isize {
            let i = y as usize * w + x as usize;
            let v = d[i].min(at(&d, x - 1, y) + 1.0).min(at(&d, x, y - 1) + 1.0).min(at(&d, x - 1, y - 1) + 1.4142).min(at(&d, x + 1, y - 1) + 1.4142);
            d[i] = v;
        }
    }
    for y in (0..h as isize).rev() {
        for x in (0..w as isize).rev() {
            let i = y as usize * w + x as usize;
            let v = d[i].min(at(&d, x + 1, y) + 1.0).min(at(&d, x, y + 1) + 1.0).min(at(&d, x + 1, y + 1) + 1.4142).min(at(&d, x - 1, y + 1) + 1.4142);
            d[i] = v;
        }
    }
    d
}

/// Ajoute au contenu d'un calque sa lueur, son ombre et son contour (dessous), avec la marge qu'il leur faut
fn styled(bm: Bitmap, st: &Style) -> Bitmap {
    let k = bm.k;
    let pad = (st.pad() * k).ceil() as u32 + 2;
    let (w0, h0) = (bm.img.width(), bm.img.height());
    let (w, h) = ((w0 + 2 * pad).min(8192) as usize, (h0 + 2 * pad).min(8192) as usize);
    // Le contenu, en couleurs prémultipliées sur la nouvelle surface
    let mut rgb = vec![[0.0f32; 3]; w * h];
    let mut alpha = vec![0.0f32; w * h];
    for (x, y, p) in bm.img.enumerate_pixels() {
        let (xx, yy) = ((x + pad) as usize, (y + pad) as usize);
        if xx >= w || yy >= h {
            continue;
        }
        let a = p[3] as f32 / 255.0;
        alpha[yy * w + xx] = a;
        rgb[yy * w + xx] = [p[0] as f32 * a, p[1] as f32 * a, p[2] as f32 * a];
    }
    let mut out_rgb = vec![[0.0f32; 3]; w * h];
    let mut out_a = vec![0.0f32; w * h];
    // « Par-dessus » : ajoute une couche sous ce qui est déjà là
    let under = |orgb: &mut Vec<[f32; 3]>, oa: &mut Vec<f32>, i: usize, c: [u8; 3], a: f32| {
        let a = a.clamp(0.0, 1.0);
        let keep = 1.0 - oa[i];
        orgb[i][0] += c[0] as f32 * a * keep;
        orgb[i][1] += c[1] as f32 * a * keep;
        orgb[i][2] += c[2] as f32 * a * keep;
        oa[i] += a * keep;
    };
    // Le contenu d'abord (au-dessus de tout)
    out_rgb.clone_from(&rgb);
    out_a.clone_from(&alpha);
    if let Some(s) = &st.stroke {
        let d = distance(&alpha, w, h);
        let r = s.width * k;
        for i in 0..w * h {
            let cov = (r - d[i] + 0.5).clamp(0.0, 1.0);
            if cov > 0.0 {
                under(&mut out_rgb, &mut out_a, i, s.color, cov);
            }
        }
    }
    // La silhouette (contour compris) sert à l'ombre et à la lueur
    let shape = out_a.clone();
    if let Some(sh) = &st.shadow {
        let mut m = shape.clone();
        blur(&mut m, w, h, sh.blur * k);
        let (dx, dy) = ((sh.dx * k).round() as isize, (sh.dy * k).round() as isize);
        for y in 0..h as isize {
            for x in 0..w as isize {
                let (sx, sy) = (x - dx, y - dy);
                if sx < 0 || sy < 0 || sx >= w as isize || sy >= h as isize {
                    continue;
                }
                let a = m[sy as usize * w + sx as usize] * sh.opacity;
                if a > 0.002 {
                    under(&mut out_rgb, &mut out_a, y as usize * w + x as usize, sh.color, a);
                }
            }
        }
    }
    if let Some(g) = &st.glow {
        let mut m = shape;
        blur(&mut m, w, h, g.radius * k);
        for i in 0..w * h {
            let a = (m[i] * g.strength).min(1.0);
            if a > 0.002 {
                under(&mut out_rgb, &mut out_a, i, g.color, a);
            }
        }
    }
    let mut img = image::RgbaImage::new(w as u32, h as u32);
    for (i, p) in img.pixels_mut().enumerate() {
        let a = out_a[i].clamp(0.0, 1.0);
        if a > 0.0 {
            let c = out_rgb[i];
            *p = image::Rgba([(c[0] / a).min(255.0) as u8, (c[1] / a).min(255.0) as u8, (c[2] / a).min(255.0) as u8, (a * 255.0) as u8]);
        }
    }
    Bitmap { img, ox: bm.ox - pad as f32 / k, oy: bm.oy - pad as f32 / k, k }
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
    for (li, l) in c.layers.iter().enumerate().rev() {
        if !l.live(t) {
            continue;
        }
        let op = (l.opacity.at(t)[0] / 100.0).clamp(0.0, 1.0);
        if op <= 0.0 {
            continue;
        }
        let x = c.world(li, t);
        // Le contenu est calculé à la résolution où il sera vu (net même agrandi)
        let k = (scale * x.max_scale()).clamp(0.02, 8.0);
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
            serde_json::to_string(&l.style).unwrap_or_default().hash(&mut h);
            (h.finish(), k.to_bits())
        });
        let cached = key.and_then(|k| cache.bitmaps.get(&k).cloned());
        let bm = match cached {
            Some(b) => b,
            None => {
                let Some(b) = bitmap(l, t, k, cache) else { continue };
                let b = if l.style.is_plain() { b } else { styled(b, &l.style) };
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

// ---------- Musique ----------

/// Enveloppe d'un son (100 valeurs par seconde, de 0 à 1) et ses temps forts, en secondes
#[derive(Clone, Debug, Default)]
pub struct Wave {
    pub env: Vec<f32>,
    pub beats: Vec<f32>,
}

pub const WAVE_RATE: f32 = 100.0;

/// Lit un son avec ffmpeg et en tire l'enveloppe et les temps forts
pub fn analyse(path: &Path) -> Result<Wave, String> {
    let ff = crate::ffmpeg::find("ffmpeg").ok_or(crate::i18n::t("ffmpeg n'est pas installé."))?;
    let o = crate::ffmpeg::command(ff).args(["-v", "error", "-i"]).arg(path).args(["-ac", "1", "-ar", "8000", "-f", "s16le", "-"]).stdin(std::process::Stdio::null()).output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("ffmpeg").to_string());
    }
    let samples: Vec<f32> = o.stdout.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
    let win = (8000.0 / WAVE_RATE) as usize;
    let env: Vec<f32> = samples.chunks(win).map(|c| (c.iter().map(|x| x * x).sum::<f32>() / c.len().max(1) as f32).sqrt()).collect();
    let peak = env.iter().cloned().fold(1e-6, f32::max);
    let env: Vec<f32> = env.iter().map(|v| v / peak).collect();
    let beats = beats(&env);
    Ok(Wave { env, beats })
}

/// Temps forts : là où le son monte d'un coup, nettement plus que d'habitude autour (une seconde de chaque côté)
pub fn beats(env: &[f32]) -> Vec<f32> {
    let n = env.len();
    let flux: Vec<f32> = (0..n).map(|i| if i == 0 { 0.0 } else { (env[i] - env[i - 1]).max(0.0) }).collect();
    let r = WAVE_RATE as usize;
    let mut out: Vec<f32> = vec![];
    for i in 1..n.saturating_sub(1) {
        let (a, b) = (i.saturating_sub(r), (i + r).min(n));
        let w = &flux[a..b];
        let mean = w.iter().sum::<f32>() / w.len() as f32;
        let sd = (w.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / w.len() as f32).sqrt();
        let f = flux[i];
        if f > mean + 1.5 * sd && f > 0.05 && f >= flux[i - 1] && f >= flux[i + 1] {
            let t = i as f32 / WAVE_RATE;
            // Un temps fort tous les quarts de seconde au plus
            if out.last().is_none_or(|l| t - l >= 0.25) {
                out.push(t);
            }
        }
    }
    out
}

/// Cale les clés d'un calque sur les temps forts les plus proches (à moins d'une demi-seconde) ; rend le nombre de clés déplacées
pub fn snap_to_beats(l: &mut Layer, beats: &[f32]) -> usize {
    let mut n = 0;
    for (p, _) in PROPS {
        let tr = l.track_mut(p);
        let mut used: Vec<f32> = vec![];
        for k in &mut tr.keys {
            let near = beats.iter().cloned().filter(|b| !used.iter().any(|u| (u - b).abs() < KEY_EPS)).min_by(|a, b| (a - k.t).abs().total_cmp(&(b - k.t).abs()));
            if let Some(b) = near.filter(|b| (b - k.t).abs() < 0.5) {
                if (b - k.t).abs() > KEY_EPS {
                    n += 1;
                }
                k.t = b;
                used.push(b);
            }
        }
        tr.sort();
    }
    n
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
    fn temps_forts() {
        // Un coup toutes les 0,5 s sur un fond calme
        let mut env = vec![0.05f32; 400];
        for b in [50usize, 100, 150, 200, 250, 300, 350] {
            env[b] = 1.0;
            env[b + 1] = 0.6;
        }
        let found = beats(&env);
        assert_eq!(found.len(), 7, "{found:?}");
        assert!((found[0] - 0.5).abs() < 0.02);
        let mut c = Compo::default();
        let n = snap_to_beats(&mut c.layers[1], &found);
        assert_eq!(n, 2);
        let ks: Vec<f32> = c.layers[1].scale.keys.iter().map(|k| k.t).collect();
        assert_eq!(ks, vec![0.5, 1.0]);
    }

    #[test]
    fn parente_et_style() {
        let mut c = Compo::default();
        let mut cache = Cache::new(&std::env::temp_dir());
        let before = corners(&c, 1, 3.0, &mut cache);
        // Le trait devient l'enfant du titre sans bouger
        let pid = c.layers[0].id;
        c.set_parent(1, Some(pid), 3.0);
        let after = corners(&c, 1, 3.0, &mut cache);
        for (a, b) in before.iter().zip(after.iter()) {
            assert!((a[0] - b[0]).abs() < 0.01 && (a[1] - b[1]).abs() < 0.01, "{a:?} {b:?}");
        }
        // Le titre se déplace et tourne : l'enfant suit
        c.layers[0].pos.v = [1060.0, 500.0];
        c.layers[0].rot.v = [90.0, 0.0];
        let moved = c.world(1, 3.0).apply([0.0, 0.0]);
        assert!((moved[0] - 950.0).abs() < 0.5 && (moved[1] - 500.0).abs() < 0.5, "{moved:?}");
        // Pas de boucle : le titre ne peut pas devenir l'enfant de son enfant
        let cid = c.layers[1].id;
        c.set_parent(0, Some(cid), 3.0);
        assert_eq!(c.layers[0].parent, None);
        // Un contour et une ombre élargissent ce qui est dessiné
        let lit = |img: &image::RgbaImage| img.pixels().filter(|p| p[0] < 60 && p[2] > 200).count();
        c.layers[0].rot.v = [0.0, 0.0];
        c.layers[0].style.stroke = Some(StrokeStyle { width: 8.0, color: [10, 10, 250] });
        let img = render(&c, 3.0, 0.25, &mut cache);
        assert!(lit(&img) > 100, "contour bleu visible");
        let s = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<Compo>(&s).unwrap(), c);
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
