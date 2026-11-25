//! Motion design : des compositions animées (`.arymotion`), comme After Effects en plus simple.
//!
//! Une composition a une taille, une cadence et une durée, et une pile de calques (le premier est
//! au-dessus). Chaque calque est un texte, une forme ou une image, avec quatre propriétés animables
//! (position, échelle, rotation, opacité) : sans clé, une valeur fixe ; avec des clés, la valeur
//! passe de l'une à l'autre en suivant le lissage de la clé de départ. Un texte peut aussi entrer
//! lettre par lettre ou mot par mot (« animateur de texte »).
//!
//! Le rendu est fait au processeur, le même pour l'aperçu et pour l'export MP4 (ffmpeg).

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

pub fn is_compo(rel: &str) -> bool {
    crate::formats::has(rel, "motion")
}

/// Les lissages sont ceux de toute l'app (voir keys.rs)
pub use crate::keys::{Ease, EASES};

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct Key {
    pub t: f32,
    pub v: [f32; 2],
    #[serde(default)]
    pub ease: Ease,
    /// Tangentes posées dans l'éditeur de courbes, par axe (gauche Δt Δv, droite Δt Δv) ; vide : le lissage
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tan: Vec<Option<[f32; 4]>>,
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
        // Des tangentes posées dans l'éditeur de courbes : chaque axe suit sa courbe
        if k.iter().any(|x| !x.tan.is_empty()) {
            return [0, 1].map(|c| crate::curves::eval(&self.curve(c), t));
        }
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

    /// Un axe de la propriété, en courbe (pour l'éditeur de courbes)
    pub fn curve(&self, c: usize) -> Vec<crate::curves::CK> {
        self.keys.iter().map(|k| crate::curves::CK { t: k.t, v: k.v[c], ease: k.ease, tan: k.tan.get(c).copied().flatten() }).collect()
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
            None => self.insert(Key { t, v, ease: Ease::Smooth, tan: vec![] }),
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
                self.insert(Key { t, v, ease: Ease::Smooth, tan: vec![] });
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
    /// Taille de départ de chaque morceau (1 : sa taille)
    #[serde(default = "one_f", skip_serializing_if = "is_one_f")]
    pub scale: f32,
    /// Départ en x (px), comme `rise` pour y
    #[serde(default, skip_serializing_if = "is_zero_f")]
    pub slide: f32,
    /// Ordre d'arrivée : 0 dans l'ordre, 1 à l'envers, 2 du centre vers les bords, 3 au hasard
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub order: u8,
}

fn one_f() -> f32 {
    1.0
}
fn is_one_f(v: &f32) -> bool {
    (*v - 1.0).abs() < 1e-6
}
fn is_zero_f(v: &f32) -> bool {
    *v == 0.0
}
fn is_zero_u8(v: &u8) -> bool {
    *v == 0
}

impl Reveal {
    /// Rang d'arrivée du morceau `u` sur `n`.
    pub fn rank(&self, u: usize, n: usize) -> f32 {
        let n = n.max(1);
        match self.order {
            1 => (n - 1 - u.min(n - 1)) as f32,
            2 => ((u as f32) - (n as f32 - 1.0) / 2.0).abs(),
            3 => {
                // Un mélange stable : la même place à chaque image
                let mut idx: Vec<usize> = (0..n).collect();
                idx.sort_by_key(|&i| (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17));
                idx.iter().position(|&i| i == u).unwrap_or(u) as f32
            }
            _ => u as f32,
        }
    }
}

impl Default for Reveal {
    fn default() -> Reveal {
        Reveal { by: Unit::Letter, delay: 0.04, rise: 60.0, dur: 0.5, ease: Ease::Bounce, scale: 1.0, slide: 0.0, order: 0 }
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
    /// Tracé : une ligne lissée qui passe par ces points (px, autour de l'origine du calque)
    Path {
        points: Vec<[f32; 2]>,
        #[serde(default)]
        closed: bool,
        width: f32,
        color: [u8; 3],
        /// Une pointe de flèche au bout (elle suit le tracé progressif)
        #[serde(default)]
        arrow: bool,
        /// Calque de forme : le dedans du tracé fermé est rempli de cette couleur
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<[u8; 3]>,
        /// La forme animée : ses points à certains instants (interpolés entre deux clés)
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        keys: Vec<PathKey>,
    },
    /// Précomposition : une autre composition Motion du coffre, jouée depuis le début du calque
    /// Objet nul : invisible, sert de parent et de poignée pour piloter d'autres calques
    Null,
    /// Bruit fractal animé (fumée, nuages, texture qui bouge), entre deux couleurs
    Noise {
        w: f32,
        h: f32,
        /// Taille des plus gros motifs, en px
        scale: f32,
        /// Évolution par seconde (0 : figé)
        speed: f32,
        contrast: f32,
        color: [u8; 3],
        color2: [u8; 3],
        /// Turbulent : des filaments plutôt que des nuages
        turbulent: bool,
        seed: u32,
    },
    Compo {
        /// Chemin dans le coffre
        path: String,
        /// Remappage du temps : vitesse (1 : normale, négative : à l'envers), départ dans la précomposition (s)
        #[serde(default = "unit_speed")]
        speed: f32,
        #[serde(default)]
        offset: f32,
        /// Rejoue en boucle au lieu de garder la dernière image
        #[serde(default)]
        looped: bool,
    },
}

impl Kind {
    pub fn label(&self) -> &'static str {
        match self {
            Kind::Text { .. } => "texte",
            Kind::Rect { .. } => "rectangle",
            Kind::Ellipse { .. } => "ellipse",
            Kind::Image { .. } => "image",
            Kind::Compo { .. } => "composition",
            Kind::Null => "nul",
            Kind::Path { .. } => "tracé",
            Kind::Noise { .. } => "bruit fractal",
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
    /// Tremblement : petits mouvements au hasard, doux, qui s'ajoutent à l'animation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wiggle: Option<Wiggle>,
    /// Avant la première clé et après la dernière : constante, en boucle, en va-et-vient
    #[serde(default, skip_serializing_if = "crate::curves::Loop::is_none")]
    pub cycle: crate::curves::Loop,
    /// Cache de piste (comme After Effects) : le calque n'apparaît qu'à travers celui du dessus
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matte: Option<Matte>,
    /// Tracé progressif (comme les « trim paths » d'After Effects) : fin et début de la partie visible, en %
    #[serde(default = "full_trim", skip_serializing_if = "is_full_trim")]
    pub trim: Track,
    /// Répétiteur (comme celui des calques de forme d'After Effects) : des copies décalées, tournées, réduites
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<Repeater>,
    /// Expressions par propriété (« pos », « scale », « rot », « opacity ») : une formule remplace la valeur
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub exprs: std::collections::BTreeMap<String, String>,
    /// Masques (comme After Effects) : des formes, animables, qui découpent le calque
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub masks: Vec<Mask>,
    /// Effets de déformation, dans l'ordre
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fx: Vec<Effect>,
    /// Orientation automatique : le calque tourne pour suivre la direction de son mouvement
    #[serde(default, skip_serializing_if = "is_false_b")]
    pub auto_orient: bool,
    /// Police variable d'un texte : valeur de chaque axe (wght, wdth, slnt…), animable (v[0])
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub font_axes: std::collections::BTreeMap<String, Track>,
    /// Rig 2D : IK à deux os posée sur le premier os (ce calque), voir `Ik`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ik: Option<Ik>,
    /// Calque 3D (2,5D) : sa profondeur (v[0], px ; plus grand = plus loin), vue par la caméra de la composition
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<Track>,
}

/// La forme d'un tracé ou d'un masque à un instant
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PathKey {
    pub t: f32,
    pub points: Vec<[f32; 2]>,
}

/// Les points à l'instant `t` : la forme de base, ou l'interpolation (adoucie) entre les deux clés voisines
pub fn points_at(base: &[[f32; 2]], keys: &[PathKey], t: f32) -> Vec<[f32; 2]> {
    if keys.is_empty() {
        return base.to_vec();
    }
    let i = keys.partition_point(|k| k.t <= t);
    if i == 0 {
        return keys[0].points.clone();
    }
    if i >= keys.len() {
        return keys[keys.len() - 1].points.clone();
    }
    let (a, b) = (&keys[i - 1], &keys[i]);
    let f = ((t - a.t) / (b.t - a.t).max(1e-6)).clamp(0.0, 1.0);
    let f = f * f * (3.0 - 2.0 * f);
    if a.points.len() != b.points.len() {
        return if f < 0.5 { a.points.clone() } else { b.points.clone() };
    }
    a.points.iter().zip(&b.points).map(|(p, q)| [p[0] + (q[0] - p[0]) * f, p[1] + (q[1] - p[1]) * f]).collect()
}

/// Change la forme à l'instant `t` : la base si rien n'est animé, sinon la clé de cet instant (créée au besoin)
pub fn set_points_at(base: &mut Vec<[f32; 2]>, keys: &mut Vec<PathKey>, t: f32, pts: Vec<[f32; 2]>) {
    if keys.is_empty() {
        *base = pts;
        return;
    }
    match keys.iter_mut().find(|k| (k.t - t).abs() < KEY_EPS) {
        Some(k) => k.points = pts,
        None => {
            keys.push(PathKey { t, points: pts });
            keys.sort_by(|a, b| a.t.total_cmp(&b.t));
        }
    }
}

/// Une clé de forme à l'instant `t` (la première clé reprend aussi la forme de base)
pub fn key_points(base: &[[f32; 2]], keys: &mut Vec<PathKey>, t: f32) {
    let now = points_at(base, keys, t);
    if keys.iter().any(|k| (k.t - t).abs() < KEY_EPS) {
        keys.retain(|k| (k.t - t).abs() >= KEY_EPS);
        return;
    }
    keys.push(PathKey { t, points: now });
    keys.sort_by(|a, b| a.t.total_cmp(&b.t));
}

/// Un masque de calque : une forme fermée (px du calque, autour de son origine)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Mask {
    pub points: Vec<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<PathKey>,
    /// Bord adouci (px)
    #[serde(default)]
    pub feather: f32,
    #[serde(default)]
    pub invert: bool,
    /// 0 ajouter, 1 soustraire, 2 intersection
    #[serde(default)]
    pub mode: u8,
    /// En %
    #[serde(default = "hundred")]
    pub opacity: f32,
    /// Lissé (courbe qui passe par les points) ou à angles vifs
    #[serde(default)]
    pub smooth: bool,
}

impl Mask {
    pub fn rect(w: f32, h: f32) -> Mask {
        let (a, b) = (w / 2.0, h / 2.0);
        Mask { points: vec![[-a, -b], [a, -b], [a, b], [-a, b]], keys: vec![], feather: 0.0, invert: false, mode: 0, opacity: 100.0, smooth: false }
    }
    pub fn ellipse(w: f32, h: f32) -> Mask {
        let pts = (0..8).map(|k| {
            let a = k as f32 / 8.0 * std::f32::consts::TAU;
            [a.cos() * w / 2.0, a.sin() * h / 2.0]
        });
        Mask { points: pts.collect(), smooth: true, ..Mask::rect(w, h) }
    }
    /// Le contour à l'instant `t`, prêt à remplir
    pub fn outline(&self, t: f32) -> Vec<[f32; 2]> {
        let p = points_at(&self.points, &self.keys, t);
        if self.smooth {
            smooth_path(&p, true)
        } else {
            p
        }
    }
}

/// La part de chaque pixel d'une grille (origine `o`, `k` px par unité) couverte par un polygone (pair-impair)
pub fn poly_coverage(poly: &[[f32; 2]], o: [f32; 2], k: f32, w: usize, h: usize) -> Vec<f32> {
    let mut cov = vec![0.0f32; w * h];
    if poly.len() < 3 {
        return cov;
    }
    const SUB: usize = 4;
    let mut xs: Vec<f32> = vec![];
    for y in 0..h {
        for s in 0..SUB {
            let py = o[1] + (y as f32 + (s as f32 + 0.5) / SUB as f32) / k;
            xs.clear();
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                if (a[1] <= py) != (b[1] <= py) {
                    let x = a[0] + (py - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                    xs.push((x - o[0]) * k);
                }
            }
            xs.sort_by(|a, b| a.total_cmp(b));
            for pair in xs.chunks_exact(2) {
                let (x0, x1) = (pair[0].max(0.0), pair[1].min(w as f32));
                if x0 >= x1 {
                    continue;
                }
                let (i0, i1) = (x0.floor() as usize, (x1.ceil() as usize).min(w));
                for x in i0..i1 {
                    let a = (x1.min(x as f32 + 1.0) - x0.max(x as f32)).clamp(0.0, 1.0);
                    cov[y * w + x] += a / SUB as f32;
                }
            }
        }
    }
    cov
}

/// Applique les masques d'un calque à son image (repère du calque)
fn apply_masks(bm: &mut Bitmap, masks: &[Mask], t: f32) {
    let (w, h) = (bm.img.width() as usize, bm.img.height() as usize);
    let start = if masks.first().is_some_and(|m| m.mode != 0) { 1.0 } else { 0.0 };
    let mut acc = vec![start; w * h];
    for m in masks {
        let mut c = poly_coverage(&m.outline(t), [bm.ox, bm.oy], bm.k, w, h);
        if m.feather > 0.0 {
            blur(&mut c, w, h, m.feather * bm.k / 2.0);
        }
        let op = (m.opacity / 100.0).clamp(0.0, 1.0);
        for (a, v) in acc.iter_mut().zip(c) {
            let v = (if m.invert { 1.0 - v.min(1.0) } else { v.min(1.0) }) * op;
            *a = match m.mode {
                1 => *a * (1.0 - v),
                2 => *a * v,
                _ => *a + v - *a * v,
            };
        }
    }
    for (px, a) in bm.img.pixels_mut().zip(acc) {
        px[3] = (px[3] as f32 * a.clamp(0.0, 1.0)).round() as u8;
    }
}

/// Copies d'un calque : chacune décalée de `pos`, tournée de `rot`, mise à l'échelle de `scale` % par rapport
/// à la précédente ; l'opacité va de 100 % à `fade` % sur la dernière
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Repeater {
    pub copies: u32,
    pub pos: [f32; 2],
    pub rot: f32,
    pub scale: f32,
    pub fade: f32,
}

impl Default for Repeater {
    fn default() -> Repeater {
        Repeater { copies: 5, pos: [120.0, 0.0], rot: 0.0, scale: 100.0, fade: 100.0 }
    }
}

impl Repeater {
    /// La transformation de la copie `k` (repère du calque) et son opacité relative
    pub fn copy(&self, k: u32) -> (Xform, f32) {
        let kf = k as f32;
        let sc = (self.scale / 100.0).powf(kf);
        let r = (self.rot * kf).to_radians();
        let (cs, sn) = (r.cos(), r.sin());
        let n = self.copies.max(1);
        let op = if n > 1 { 1.0 + (self.fade / 100.0 - 1.0) * kf / (n - 1) as f32 } else { 1.0 };
        (Xform { a: cs * sc, b: sn * sc, c: -sn * sc, d: cs * sc, e: self.pos[0] * kf, f: self.pos[1] * kf }, op.clamp(0.0, 1.0))
    }
}

fn unit_speed() -> f32 {
    1.0
}

/// Le temps d'une précomposition à l'instant `local` du calque (depuis son début)
pub fn remap_time(local: f32, speed: f32, offset: f32, looped: bool, dur: f32) -> f32 {
    let last = (dur - 1e-3).max(0.0);
    let t = offset + local * speed;
    if looped && dur > 0.0 {
        t.rem_euclid(dur)
    } else {
        t.clamp(0.0, last)
    }
}

fn full_trim() -> Track {
    Track::new(100.0, 0.0)
}

fn is_full_trim(t: &Track) -> bool {
    t.keys.is_empty() && t.v == [100.0, 0.0]
}

/// Comment le calque du dessus sert de cache
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Matte {
    Alpha,
    AlphaInverted,
    Luma,
    LumaInverted,
}

impl Matte {
    pub const ALL: [Matte; 4] = [Matte::Alpha, Matte::AlphaInverted, Matte::Luma, Matte::LumaInverted];

    pub fn name(self) -> &'static str {
        match self {
            Matte::Alpha => "Alpha",
            Matte::AlphaInverted => "Alpha inversé",
            Matte::Luma => "Luminance",
            Matte::LumaInverted => "Luminance inversée",
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Wiggle {
    /// Mouvements par seconde
    pub freq: f32,
    /// Écart maximal de position, en px
    pub pos: f32,
    /// Écart de rotation, en degrés
    pub rot: f32,
    /// Écart d'échelle, en %
    pub scale: f32,
    /// Change le tirage
    #[serde(default)]
    pub seed: u32,
}

impl Default for Wiggle {
    fn default() -> Wiggle {
        Wiggle { freq: 2.0, pos: 12.0, rot: 0.0, scale: 0.0, seed: 0 }
    }
}

/// Bruit doux de -1 à 1 : valeurs au hasard aux entiers, reliées en douceur (deux octaves)
pub fn smooth_noise(x: f32, channel: u32, seed: u64) -> f32 {
    let hash = |i: i64| {
        let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ seed.wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ (channel as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
        h ^= h >> 31;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 29;
        (h >> 11) as f32 / (1u64 << 53) as f32 * 2.0 - 1.0
    };
    let octave = |x: f32| {
        let i = x.floor();
        let f = x - i;
        let u = f * f * (3.0 - 2.0 * f);
        let (a, b) = (hash(i as i64), hash(i as i64 + 1));
        a + (b - a) * u
    };
    (octave(x) * 0.7 + octave(x * 2.03 + 17.0) * 0.3) / 1.0
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

/// Les courbes d'un calque pour l'éditeur de courbes : un axe par courbe, liées par propriété
pub fn layer_curves(l: &Layer) -> Vec<crate::curves::Curve> {
    use crate::curves::{Curve, W, X, Y, Z};
    let mut out = vec![];
    for (n, (pr, name)) in PROPS.iter().enumerate() {
        let tr = l.track(*pr);
        let axes: &[(&str, eframe::egui::Color32)] = match pr {
            Prop::Pos | Prop::Scale => &[("X", X), ("Y", Y)],
            Prop::Rot => &[("", Z)],
            Prop::Opacity => &[("", W)],
        };
        for (c, (ax, col)) in axes.iter().enumerate() {
            let label = if ax.is_empty() { crate::i18n::t(name) } else { format!("{} {ax}", crate::i18n::t(name)) };
            out.push(Curve { label, color: *col, keys: tr.curve(c), link: n as u32 });
        }
    }
    out
}

/// Remet les courbes éditées dans le calque (les instants et lissages viennent du premier axe de chaque lien)
pub fn apply_curves(l: &mut Layer, curves: &[crate::curves::Curve]) {
    for (n, (pr, _)) in PROPS.iter().enumerate() {
        let axes: Vec<&crate::curves::Curve> = curves.iter().filter(|c| c.link == n as u32).collect();
        let Some(first) = axes.first() else { continue };
        let tr = l.track_mut(*pr);
        let last = tr.keys.last().map(|k| k.v);
        tr.keys = first
            .keys
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let mut v = last.unwrap_or([0.0; 2]);
                for (c, a) in axes.iter().enumerate().take(2) {
                    if let Some(x) = a.keys.get(i) {
                        v[c] = x.v;
                    }
                }
                let tan: Vec<Option<[f32; 4]>> = axes.iter().map(|a| a.keys.get(i).and_then(|x| x.tan)).collect();
                Key { t: k.t, v, ease: k.ease, tan: if tan.iter().any(|x| x.is_some()) { tan } else { vec![] } }
            })
            .collect();
    }
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
            wiggle: None,
            cycle: Default::default(),
            matte: None,
            trim: full_trim(),
            exprs: Default::default(),
            masks: vec![],
            auto_orient: false,
            font_axes: Default::default(),
            ik: None,
            depth: None,
            fx: vec![],
            repeat: None,
        }
    }

    /// Nom court d'une propriété dans les expressions
    pub fn prop_key(p: Prop) -> &'static str {
        match p {
            Prop::Pos => "pos",
            Prop::Scale => "scale",
            Prop::Rot => "rot",
            Prop::Opacity => "opacity",
        }
    }

    /// La valeur de la propriété à l'instant `t`, expression comprise (si elle est fausse, la valeur animée reste)
    pub fn eval_prop(&self, p: Prop, t: f32) -> [f32; 2] {
        let v = self.value(self.track(p), t);
        self.apply_expr(p, v, t).unwrap_or(v)
    }

    /// Applique l'expression de la propriété ; None sans expression, Err si elle ne se calcule pas
    pub fn try_expr(&self, p: Prop, v: [f32; 2], t: f32) -> Option<Result<[f32; 2], String>> {
        let src = self.exprs.get(Self::prop_key(p)).filter(|s| !s.trim().is_empty())?;
        let value = match p {
            Prop::Rot | Prop::Opacity => crate::expr::Val::Num(v[0] as f64),
            _ => crate::expr::Val::Vec(vec![v[0] as f64, v[1] as f64]),
        };
        let cx = crate::expr::Ctx { time: t as f64, value, index: 0.0, fps: 30.0, seed: self.id };
        Some(crate::expr::eval(src, &cx).map(|r| {
            let q = r.pair();
            match (p, &r) {
                // Un nombre pour l'échelle : les deux axes ensemble
                (Prop::Rot | Prop::Opacity, _) => [q[0] as f32, v[1]],
                _ => [q[0] as f32, q[1] as f32],
            }
        }))
    }

    fn apply_expr(&self, p: Prop, v: [f32; 2], t: f32) -> Option<[f32; 2]> {
        self.try_expr(p, v, t).and_then(|r| r.ok())
    }

    /// Les bornes de l'animation du calque (première et dernière clé, toutes propriétés)
    pub fn key_span(&self) -> Option<(f32, f32)> {
        let ts = [&self.pos, &self.scale, &self.rot, &self.opacity, &self.trim].into_iter().flat_map(|t| t.keys.iter().map(|k| k.t));
        let (lo, hi) = ts.fold((f32::MAX, f32::MIN), |(a, b), t| (a.min(t), b.max(t)));
        (hi > lo).then_some((lo, hi))
    }

    /// Valeur d'une propriété du calque à l'instant `t`, bouclage compris
    pub fn value(&self, tr: &Track, t: f32) -> [f32; 2] {
        if self.cycle.is_none() || !tr.animated() {
            return tr.at(t);
        }
        let Some((t0, t1)) = self.key_span() else { return tr.at(t) };
        let (t2, n) = self.cycle.remap(t, t0, t1);
        let (a, b) = (tr.at(t0), tr.at(t1));
        let v = tr.at(t2);
        [v[0] + n * (b[0] - a[0]), v[1] + n * (b[1] - a[1])]
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
        self.xform_rot(t, None)
    }

    /// Comme `xform`, avec une rotation imposée (degrés) : celle d'une IK
    pub fn xform_rot(&self, t: f32, rot: Option<f32>) -> Xform {
        let mut p = self.eval_prop(Prop::Pos, t);
        let mut s = self.eval_prop(Prop::Scale, t);
        let mut r = rot.unwrap_or_else(|| self.eval_prop(Prop::Rot, t)[0]);
        if self.auto_orient {
            // La direction du mouvement autour de t (un pas d'image de part et d'autre)
            let dt = 1.0 / 60.0;
            let (a, b) = (self.eval_prop(Prop::Pos, t - dt), self.eval_prop(Prop::Pos, t + dt));
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            if dx * dx + dy * dy > 1e-6 {
                r += dy.atan2(dx).to_degrees();
            }
        }
        if let Some(w) = &self.wiggle {
            let x = t * w.freq.max(0.0);
            let seed = self.id ^ w.seed as u64;
            p[0] += smooth_noise(x, 0, seed) * w.pos;
            p[1] += smooth_noise(x, 1, seed) * w.pos;
            r += smooth_noise(x, 2, seed) * w.rot;
            let k = smooth_noise(x, 3, seed) * w.scale;
            s = [s[0] + k, s[1] + k];
        }
        let r = r.to_radians();
        let (cs, sn) = (r.cos(), r.sin());
        let (sx, sy) = (s[0] / 100.0, s[1] / 100.0);
        Xform { a: cs * sx, b: sn * sx, c: -sn * sy, d: cs * sy, e: p[0], f: p[1] }
    }
}

/// IK à deux os (comme Duik) : ce calque est le premier os, `lower` le second (son enfant), `end` le bout
/// (enfant du second) ; les deux os tournent pour que le bout atteigne le calque `target`
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Ik {
    pub lower: u64,
    pub end: u64,
    pub target: u64,
    /// Plier de l'autre côté (coude ou genou)
    #[serde(default)]
    pub flip: bool,
}

/// Les deux angles (degrés, chacun dans le repère de son parent) d'une IK à deux os : épaule en `s`,
/// coude en `elbow` (dans le repère du premier os), bout en `tip` (dans le repère du second), cible `target`
/// (tous dans le repère du parent du premier os)
pub fn two_bone_ik(s: [f32; 2], elbow: [f32; 2], tip: [f32; 2], target: [f32; 2], flip: bool) -> (f32, f32) {
    let (l1, l2) = (elbow[0].hypot(elbow[1]).max(1e-3), tip[0].hypot(tip[1]).max(1e-3));
    let (a1, a2) = (elbow[1].atan2(elbow[0]), tip[1].atan2(tip[0]));
    let (dx, dy) = (target[0] - s[0], target[1] - s[1]);
    let d = dx.hypot(dy).clamp((l1 - l2).abs() + 1e-3, l1 + l2 - 1e-3);
    let phi = dy.atan2(dx);
    let a = ((l1 * l1 + d * d - l2 * l2) / (2.0 * l1 * d)).clamp(-1.0, 1.0).acos();
    let b = ((l1 * l1 + l2 * l2 - d * d) / (2.0 * l1 * l2)).clamp(-1.0, 1.0).acos();
    let (th1, rel) = if flip { (phi + a, -(std::f32::consts::PI - b)) } else { (phi - a, std::f32::consts::PI - b) };
    ((th1 - a1).to_degrees(), (a1 + rel - a2).to_degrees())
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

    /// `self` après `o` : d'abord `o`, puis `self`
    pub fn after(&self, o: &Xform) -> Xform {
        Xform {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            e: self.a * o.e + self.c * o.f + self.e,
            f: self.b * o.e + self.d * o.f + self.f,
        }
    }

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
    /// Flou de mouvement (comme After Effects) : None, il est coupé
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blur: Option<MotionBlur>,
    /// Contrôles : des curseurs nommés, lus par leur nom dans les expressions des calques
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controls: Vec<Control>,
    /// Caméra (2,5D) : elle voit les calques 3D en perspective ; les calques 2D ne bougent pas
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<Camera>,
    /// Modèle : les réglages exposés (texte, couleur d'un calque), modifiables depuis la Vidéo
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
    /// Variante d'un modèle : le modèle d'origine (chemin du coffre)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
}

/// Un réglage exposé d'un modèle
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Param {
    pub name: String,
    /// Le calque (identifiant)
    pub layer: u64,
    /// true : la couleur du calque ; false : son texte
    pub color: bool,
}

/// La valeur d'un réglage exposé
#[derive(Clone, PartialEq, Debug)]
pub enum ParamVal {
    Text(String),
    Color([u8; 3]),
}

fn kind_color(k: &mut Kind) -> Option<&mut [u8; 3]> {
    match k {
        Kind::Text { color, .. } | Kind::Rect { color, .. } | Kind::Ellipse { color, .. } | Kind::Path { color, .. } | Kind::Noise { color, .. } => Some(color),
        _ => None,
    }
}

impl Compo {
    /// La valeur actuelle d'un réglage exposé
    pub fn param(&self, p: &Param) -> Option<ParamVal> {
        let l = self.layers.iter().find(|l| l.id == p.layer)?;
        let mut k = l.kind.clone();
        if p.color {
            kind_color(&mut k).map(|c| ParamVal::Color(*c))
        } else if let Kind::Text { text, .. } = k {
            Some(ParamVal::Text(text))
        } else {
            None
        }
    }

    pub fn set_param(&mut self, p: &Param, v: ParamVal) -> bool {
        let Some(l) = self.layers.iter_mut().find(|l| l.id == p.layer) else { return false };
        match (v, &mut l.kind) {
            (ParamVal::Text(t), Kind::Text { text, .. }) if !p.color => {
                *text = t;
                true
            }
            (ParamVal::Color(c), k) if p.color => kind_color(k).map(|x| *x = c).is_some(),
            _ => false,
        }
    }

    /// Expose (ou retire) le texte ou la couleur du calque `i` dans le modèle
    pub fn toggle_param(&mut self, i: usize, color: bool) {
        let Some(l) = self.layers.get(i) else { return };
        let id = l.id;
        if let Some(k) = self.params.iter().position(|p| p.layer == id && p.color == color) {
            self.params.remove(k);
        } else {
            let name = if color { format!("{} · {}", l.name, crate::i18n::t("couleur")) } else { l.name.clone() };
            self.params.push(Param { name, layer: id, color });
        }
    }
}

/// Caméra d'une composition : un panoramique, un travelling avant (dolly) et la profondeur de champ
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Camera {
    /// Décalage du regard (px)
    pub pan: Track,
    /// Avancée de la caméra (v[0], px)
    pub dolly: Track,
    /// Distance de la caméra au plan de profondeur 0 (px) : grand, perspective douce ; petit, forte
    pub zoom: f32,
    /// Profondeur de champ : distance nette (px de la caméra) et flou maximal (px) ; 0 : tout net
    pub focus: f32,
    pub blur: f32,
}

impl Camera {
    pub fn new(w: u32) -> Camera {
        // Comme un 50 mm d'After Effects
        let zoom = (w as f32 * 1.39).round();
        Camera { pan: Track::new(0.0, 0.0), dolly: Track::new(0.0, 0.0), zoom, focus: zoom, blur: 0.0 }
    }

    /// Distance d'un calque de profondeur `z` à la caméra, à l'instant `t`
    pub fn dist(&self, z: f32, t: f32) -> f32 {
        self.zoom + z - self.dolly.at(t)[0]
    }

    /// Le flou (px de composition) d'un calque à cette distance
    pub fn defocus(&self, dist: f32) -> f32 {
        if self.blur <= 0.0 { 0.0 } else { (self.blur * (dist - self.focus).abs() / self.focus.max(1.0)).min(self.blur) }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Control {
    pub name: String,
    pub value: f32,
    #[serde(default)]
    pub min: f32,
    #[serde(default = "hundred")]
    pub max: f32,
}

fn hundred() -> f32 {
    100.0
}

impl Compo {
    /// Rend les contrôles lisibles par les expressions (à appeler avant d'évaluer les calques).
    pub fn publish_controls(&self) {
        crate::expr::set_controls(self.controls.iter().map(|c| (c.name.clone(), c.value as f64)).collect());
    }
}

/// Flou de mouvement : l'image est la moyenne de plusieurs instants pendant que l'obturateur est ouvert
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub struct MotionBlur {
    /// Angle d'obturateur en degrés (180 : la moitié de la durée d'une image, comme au cinéma)
    pub angle: f32,
    /// Nombre d'instants moyennés
    pub samples: u32,
}

impl Default for MotionBlur {
    fn default() -> Self {
        MotionBlur { angle: 180.0, samples: 8 }
    }
}

impl Default for Compo {
    /// Une composition neuve : un titre qui entre lettre par lettre, souligné d'un trait qui s'étire
    fn default() -> Compo {
        let mut c = Compo { w: 1920, h: 1080, fps: 30.0, duration: 6.0, bg: [0x14, 0x10, 0x1a], layers: vec![], music: None, blur: None, camera: None, controls: vec![], params: vec![], template: None };
        let mut title = Layer::new("Titre", Kind::Text { text: "Titre".into(), size: 140.0, color: [0xf4, 0xee, 0xf1], font: "inter-bold".into(), tracking: -2.0, align: 1 }, &c);
        title.reveal = Some(Reveal::default());
        title.pos.v = [960.0, 500.0];
        let mut line = Layer::new("Soulignement", Kind::Rect { w: 420.0, h: 10.0, radius: 5.0, color: [0xcc, 0xac, 0xb9] }, &c);
        line.pos.v = [960.0, 610.0];
        line.scale.keys = vec![Key { t: 0.4, v: [0.0, 100.0], ease: Ease::Out, tan: vec![] }, Key { t: 1.2, v: [100.0, 100.0], ease: Ease::Smooth, tan: vec![] }];
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
            m = self.local(j, t).then_parent(&m);
            cur = self.layers[j].parent;
        }
        m
    }

    /// Le repère d'un calque dans celui de son parent, IK comprise
    pub fn local(&self, j: usize, t: f32) -> Xform {
        let l = &self.layers[j];
        match self.ik_angles(j, t) {
            Some(r) => l.xform_rot(t, Some(r)),
            None => l.xform(t),
        }
    }

    /// La rotation imposée par une IK au calque `j` (premier ou second os), s'il y en a une
    fn ik_angles(&self, j: usize, t: f32) -> Option<f32> {
        let id = self.layers[j].id;
        // Premier os : son IK ; second os : l'IK de son parent qui le nomme
        let (upper, second) = if self.layers[j].ik.is_some() {
            (j, false)
        } else {
            let p = self.index_of(self.layers[j].parent?)?;
            if self.layers[p].ik.as_ref()?.lower != id {
                return None;
            }
            (p, true)
        };
        let ik = self.layers[upper].ik.as_ref()?;
        let (lo, en, tg) = (self.index_of(ik.lower)?, self.index_of(ik.end)?, self.index_of(ik.target)?);
        // La cible ne doit pas dépendre de la chaîne (sinon elle se poursuit elle-même)
        if [upper, lo, en].contains(&tg) || self.layers[tg].parent.is_some_and(|p| [self.layers[upper].id, ik.lower, ik.end].contains(&p)) {
            return None;
        }
        let pw = self.parent_world(upper, t);
        let target = pw.inverse(self.world(tg, t).apply([0.0, 0.0]))?;
        let s = self.layers[upper].eval_prop(Prop::Pos, t);
        let elbow = self.layers[lo].eval_prop(Prop::Pos, t);
        let tip = self.layers[en].eval_prop(Prop::Pos, t);
        let (r1, r2) = two_bone_ik(s, elbow, tip, target, ik.flip);
        Some(if second { r2 } else { r1 })
    }

    /// Le repère d'un calque vu par la caméra (perspective des calques 3D) et son flou de profondeur de champ.
    /// None : derrière la caméra.
    pub fn projected(&self, i: usize, t: f32) -> Option<(Xform, f32)> {
        let x = self.world(i, t);
        let Some(z) = self.layers[i].depth.as_ref().map(|d| d.at(t)[0]) else { return Some((x, 0.0)) };
        let cam = self.camera.clone().unwrap_or_else(|| Camera::new(self.w));
        let dist = cam.dist(z, t);
        if dist < 1.0 {
            return None;
        }
        let k = cam.zoom / dist;
        let pan = cam.pan.at(t);
        let (cx, cy) = (self.w as f32 / 2.0, self.h as f32 / 2.0);
        let p = Xform { a: x.a * k, b: x.b * k, c: x.c * k, d: x.d * k, e: k * (x.e - cx - pan[0]) + cx, f: k * (x.f - cy - pan[1]) + cy };
        Some((p, cam.defocus(dist) * k))
    }

    /// L'ordre de dessin, du fond vers l'avant : celui de la pile, mais les calques 3D se rangent entre eux
    /// selon leur distance à la caméra (le plus loin d'abord)
    pub fn draw_order(&self, t: f32) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.layers.len()).rev().collect();
        let cam = self.camera.clone().unwrap_or_else(|| Camera::new(self.w));
        let slots: Vec<usize> = (0..order.len()).filter(|&k| self.layers[order[k]].depth.is_some()).collect();
        let mut threes: Vec<usize> = slots.iter().map(|&k| order[k]).collect();
        threes.sort_by(|&a, &b| {
            let da = cam.dist(self.layers[a].depth.as_ref().map(|d| d.at(t)[0]).unwrap_or(0.0), t);
            let db = cam.dist(self.layers[b].depth.as_ref().map(|d| d.at(t)[0]).unwrap_or(0.0), t);
            db.total_cmp(&da)
        });
        for (k, li) in slots.into_iter().zip(threes) {
            order[k] = li;
        }
        order
    }

    /// Pose une IK sur le calque `i` : son premier enfant est le second os, l'enfant de celui-ci le bout ;
    /// un objet nul « contrôleur » est créé là où est le bout. Rend l'indice du contrôleur.
    pub fn make_ik(&mut self, i: usize, t: f32) -> Option<usize> {
        let id = self.layers.get(i)?.id;
        let lower = self.layers.iter().position(|l| l.parent == Some(id))?;
        let lid = self.layers[lower].id;
        let end = self.layers.iter().position(|l| l.parent == Some(lid))?;
        let eid = self.layers[end].id;
        let at = self.world(end, t).apply([0.0, 0.0]);
        let mut ctl = Layer::new(&format!("{} · {}", crate::i18n::t("Contrôleur"), self.layers[end].name), Kind::Null, self);
        ctl.pos = Track::new(at[0], at[1]);
        ctl.start = self.layers[i].start;
        ctl.end = self.layers[i].end;
        let cid = ctl.id;
        self.layers.insert(0, ctl);
        let i = i + 1;
        self.layers[i].ik = Some(Ik { lower: lid, end: eid, target: cid, flip: false });
        Some(0)
    }

    /// Le repère d'un calque dans la composition, parents compris
    pub fn world(&self, i: usize, t: f32) -> Xform {
        self.parent_world(i, t).then_parent(&self.local(i, t))
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

/// Dossier du coffre, pour les polices qu'il contient (chemins relatifs)
static FONT_ROOT: OnceLock<std::sync::Mutex<PathBuf>> = OnceLock::new();

fn is_font_file(name: &str) -> bool {
    let n = name.to_lowercase();
    n.ends_with(".ttf") || n.ends_with(".otf")
}

/// La police d'un calque texte à l'instant `t` : son nom, ou pour un fichier de police, le fichier et
/// la valeur de ses axes variables (« chemin|wght=600;wdth=80 »)
pub fn font_spec(l: &Layer, fname: &str, t: f32) -> String {
    if !is_font_file(fname) {
        return fname.to_string();
    }
    let axes: Vec<String> = l.font_axes.iter().map(|(k, tr)| format!("{k}={:.0}", tr.at(t)[0])).collect();
    format!("{fname}|{}", axes.join(";"))
}

/// Les axes d'une police variable (fichier du coffre) : (étiquette, min, défaut, max)
pub fn font_file_axes(fname: &str) -> Vec<(String, f32, f32, f32)> {
    use ab_glyph::VariableFont;
    let Some(bytes) = font_bytes(fname) else { return vec![] };
    let Ok(f) = ab_glyph::FontRef::try_from_slice(bytes) else { return vec![] };
    f.variations().into_iter().map(|a| (String::from_utf8_lossy(&a.tag).to_string(), a.min_value, a.default_value, a.max_value)).collect()
}

/// Les octets d'un fichier de police, lus une fois et gardés
fn font_bytes(fname: &str) -> Option<&'static [u8]> {
    static FILES: OnceLock<std::sync::Mutex<HashMap<String, &'static [u8]>>> = OnceLock::new();
    let files = FILES.get_or_init(Default::default);
    if let Some(b) = files.lock().ok()?.get(fname) {
        return Some(b);
    }
    let path = if Path::new(fname).is_absolute() { PathBuf::from(fname) } else { FONT_ROOT.get()?.lock().ok()?.join(fname) };
    let data: &'static [u8] = Box::leak(std::fs::read(path).ok()?.into_boxed_slice());
    files.lock().ok()?.insert(fname.to_string(), data);
    Some(data)
}

pub fn font(name: &str) -> &'static ab_glyph::FontArc {
    use ab_glyph::FontArc;
    if is_font_file(name.split('|').next().unwrap_or("")) {
        // Une police du coffre, avec ses axes : une instance par réglage (arrondi à l'unité)
        static VARIANTS: OnceLock<std::sync::Mutex<HashMap<String, &'static FontArc>>> = OnceLock::new();
        let vars = VARIANTS.get_or_init(Default::default);
        if let Some(f) = vars.lock().ok().and_then(|m| m.get(name).copied()) {
            return f;
        }
        let (file, axes) = name.split_once('|').unwrap_or((name, ""));
        let made = font_bytes(file).and_then(|b| ab_glyph::FontRef::try_from_slice(b).ok()).map(|mut f| {
            use ab_glyph::VariableFont;
            for kv in axes.split(';').filter(|s| !s.is_empty()) {
                if let Some((k, v)) = kv.split_once('=') {
                    let tag: Vec<u8> = k.bytes().chain(std::iter::repeat(b' ')).take(4).collect();
                    let _ = f.set_variation(&[tag[0], tag[1], tag[2], tag[3]], v.parse().unwrap_or(0.0));
                }
            }
            &*Box::leak(Box::new(FontArc::new(f)))
        });
        if let Some(f) = made {
            if let Ok(mut m) = vars.lock() {
                // Une animation d'axe crée beaucoup d'instances : on borne (elles sont petites)
                if m.len() < 4000 {
                    m.insert(name.to_string(), f);
                }
            }
            return f;
        }
        return font("inter-bold");
    }
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
    /// Précompositions lues (chemin → composition)
    compos: HashMap<String, Option<Arc<Compo>>>,
    /// Profondeur des précompositions en cours (une composition qui se contient elle-même s'arrête là)
    depth: u32,
}

impl Cache {
    pub fn new(root: &Path) -> Cache {
        if let Ok(mut r) = FONT_ROOT.get_or_init(Default::default).lock() {
            *r = root.to_path_buf();
        }
        Cache { images: HashMap::new(), bitmaps: HashMap::new(), root: root.to_path_buf(), compos: HashMap::new(), depth: 0 }
    }

    pub fn compo(&mut self, rel: &str) -> Option<Arc<Compo>> {
        let root = self.root.clone();
        self.compos
            .entry(rel.to_string())
            .or_insert_with(|| {
                let p = crate::vault::safe(&root.to_string_lossy(), rel).ok()?;
                Compo::read(&p).ok().map(Arc::new)
            })
            .clone()
    }

    /// Oublie les précompositions lues (l'une d'elles a pu changer)
    pub fn forget_compos(&mut self) {
        self.compos.clear();
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
            let t = layout_text(text, *size, &font_spec(l, font, l.start), *tracking, *align, Unit::Letter);
            [t.w, t.h]
        }
        Kind::Rect { w, h, .. } | Kind::Ellipse { w, h, .. } | Kind::Noise { w, h, .. } => [*w, *h],
        Kind::Image { path } => cache.image(path).map(|i| [i.width() as f32, i.height() as f32]).unwrap_or([200.0, 120.0]),
        Kind::Compo { path, .. } => cache.compo(path).map(|c| [c.w as f32, c.h as f32]).unwrap_or([400.0, 225.0]),
        Kind::Null => [100.0, 100.0],
        Kind::Path { points, width, arrow, keys, .. } => {
            // Le cadre est centré sur l'origine du calque : il couvre les points les plus éloignés (et la flèche),
            // à tous les instants d'une forme animée
            let (mx, my) = points.iter().chain(keys.iter().flat_map(|k| k.points.iter())).fold((0.0f32, 0.0f32), |(a, b), p| (a.max(p[0].abs()), b.max(p[1].abs())));
            let pad = if *arrow { width * 4.0 } else { *width };
            [2.0 * mx + pad, 2.0 * my + pad]
        }
    }
}

/// Le tracé lissé (Catmull-Rom) : passe par chaque point, sans angle
pub fn smooth_path(pts: &[[f32; 2]], closed: bool) -> Vec<[f32; 2]> {
    let n = pts.len();
    if n < 3 {
        return pts.to_vec();
    }
    let get = |i: isize| -> [f32; 2] {
        if closed {
            pts[i.rem_euclid(n as isize) as usize]
        } else {
            pts[i.clamp(0, n as isize - 1) as usize]
        }
    };
    let spans = if closed { n } else { n - 1 };
    let mut out = Vec::with_capacity(spans * 8 + 1);
    for i in 0..spans as isize {
        let (p0, p1, p2, p3) = (get(i - 1), get(i), get(i + 1), get(i + 2));
        for s in 0..8 {
            let t = s as f32 / 8.0;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f32, b: f32, c: f32, d: f32| 0.5 * (2.0 * b + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3);
            out.push([f(p0[0], p1[0], p2[0], p3[0]), f(p0[1], p1[1], p2[1], p3[1])]);
        }
    }
    out.push(if closed { pts[0] } else { pts[n - 1] });
    out
}

/// La partie [a, b] (fractions de la longueur) d'une ligne brisée
pub fn trim_path(pts: &[[f32; 2]], a: f32, b: f32) -> Vec<[f32; 2]> {
    let (a, b) = (a.clamp(0.0, 1.0).min(b.clamp(0.0, 1.0)), b.clamp(0.0, 1.0).max(a.clamp(0.0, 1.0)));
    if pts.len() < 2 || b <= a {
        return vec![];
    }
    let seg: Vec<f32> = pts.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).collect();
    let total: f32 = seg.iter().sum();
    if total <= 0.0 {
        return vec![];
    }
    let (la, lb) = (a * total, b * total);
    let lerp = |p: [f32; 2], q: [f32; 2], t: f32| [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t];
    let mut out = vec![];
    let mut acc = 0.0;
    for (i, &l) in seg.iter().enumerate() {
        let (s0, s1) = (acc, acc + l);
        acc = s1;
        if s1 < la || s0 > lb || l <= 0.0 {
            continue;
        }
        let t0 = ((la - s0) / l).max(0.0);
        let t1 = ((lb - s0) / l).min(1.0);
        if out.is_empty() {
            out.push(lerp(pts[i], pts[i + 1], t0));
        }
        out.push(lerp(pts[i], pts[i + 1], t1));
    }
    out
}

/// La pointe de flèche au bout d'une ligne : la pointe, puis les deux coins de la base
fn arrow_head(line: &[[f32; 2]], width: f32) -> Option<[[f32; 2]; 3]> {
    let tip = *line.last()?;
    let len = width * 3.0;
    // Direction : depuis un point un peu en arrière (un bout de courbe très court ne décide pas seul)
    let mut back = None;
    for q in line.iter().rev().skip(1) {
        if (tip[0] - q[0]).hypot(tip[1] - q[1]) >= len * 0.5 {
            back = Some(*q);
            break;
        }
    }
    let q = back.or_else(|| line.iter().rev().nth(1).copied())?;
    let (dx, dy) = (tip[0] - q[0], tip[1] - q[1]);
    let d = dx.hypot(dy);
    if d < 1e-4 {
        return None;
    }
    let (ux, uy) = (dx / d, dy / d);
    let base = [tip[0] - ux * len, tip[1] - uy * len];
    let half = width * 1.6;
    Some([tip, [base[0] - uy * half, base[1] + ux * half], [base[0] + uy * half, base[1] - ux * half]])
}

/// Remplit un triangle (bords adoucis) dans l'image d'un calque
fn fill_triangle(bm: &mut Bitmap, tri: [[f32; 2]; 3], color: [u8; 3]) {
    let k = bm.k;
    let (w, h) = (bm.img.width(), bm.img.height());
    let x0 = ((tri.iter().map(|p| p[0]).fold(f32::MAX, f32::min) - bm.ox) * k).floor().max(0.0) as u32;
    let x1 = ((tri.iter().map(|p| p[0]).fold(f32::MIN, f32::max) - bm.ox) * k).ceil().min(w as f32) as u32;
    let y0 = ((tri.iter().map(|p| p[1]).fold(f32::MAX, f32::min) - bm.oy) * k).floor().max(0.0) as u32;
    let y1 = ((tri.iter().map(|p| p[1]).fold(f32::MIN, f32::max) - bm.oy) * k).ceil().min(h as f32) as u32;
    // Distance signée aux trois côtés (positive dedans), quel que soit le sens du triangle
    let area = (tri[1][0] - tri[0][0]) * (tri[2][1] - tri[0][1]) - (tri[1][1] - tri[0][1]) * (tri[2][0] - tri[0][0]);
    let sign = if area < 0.0 { -1.0 } else { 1.0 };
    for y in y0..y1 {
        for x in x0..x1 {
            let (px, py) = (bm.ox + (x as f32 + 0.5) / k, bm.oy + (y as f32 + 0.5) / k);
            let mut dmin = f32::MAX;
            for e in 0..3 {
                let (a, b) = (tri[e], tri[(e + 1) % 3]);
                let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
                let l = ex.hypot(ey).max(1e-6);
                dmin = dmin.min(sign * (ex * (py - a[1]) - ey * (px - a[0])) / l);
            }
            let cov = (dmin * k + 0.5).clamp(0.0, 1.0);
            if cov > 0.0 {
                let p = bm.img.get_pixel_mut(x, y);
                let a = (cov * 255.0) as u8;
                if a > p[3] {
                    *p = image::Rgba([color[0], color[1], color[2], a]);
                }
            }
        }
    }
}

/// Un trait rond d'épaisseur `width` le long d'une ligne brisée, à la résolution `k`
fn stroke_bitmap(pts: &[[f32; 2]], width: f32, color: [u8; 3], k: f32, half: [f32; 2]) -> Bitmap {
    let pad = 2.0 / k;
    let (ox, oy) = (-half[0] - pad, -half[1] - pad);
    let (bw, bh) = (((2.0 * half[0] + 2.0 * pad) * k).ceil().clamp(1.0, 8192.0) as u32, ((2.0 * half[1] + 2.0 * pad) * k).ceil().clamp(1.0, 8192.0) as u32);
    let mut cov = vec![0.0f32; (bw * bh) as usize];
    let r = width / 2.0;
    // Chaque segment ne touche que les pixels de son cadre : rapide même pour un long tracé
    let segs: Vec<([f32; 2], [f32; 2])> = if pts.len() == 1 { vec![(pts[0], pts[0])] } else { pts.windows(2).map(|w| (w[0], w[1])).collect() };
    for (p, q) in segs {
        let x0 = (((p[0].min(q[0]) - r - ox) * k).floor().max(0.0)) as u32;
        let x1 = (((p[0].max(q[0]) + r - ox) * k).ceil().min(bw as f32)) as u32;
        let y0 = (((p[1].min(q[1]) - r - oy) * k).floor().max(0.0)) as u32;
        let y1 = (((p[1].max(q[1]) + r - oy) * k).ceil().min(bh as f32)) as u32;
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        let ll = (dx * dx + dy * dy).max(1e-9);
        for y in y0..y1 {
            for x in x0..x1 {
                let (lx, ly) = (ox + (x as f32 + 0.5) / k, oy + (y as f32 + 0.5) / k);
                let t = (((lx - p[0]) * dx + (ly - p[1]) * dy) / ll).clamp(0.0, 1.0);
                let d = (lx - p[0] - t * dx).hypot(ly - p[1] - t * dy);
                let a = (0.5 - (d - r) * k).clamp(0.0, 1.0);
                let c = &mut cov[(y * bw + x) as usize];
                if a > *c {
                    *c = a;
                }
            }
        }
    }
    let mut img = image::RgbaImage::new(bw, bh);
    for (i, px) in img.pixels_mut().enumerate() {
        if cov[i] > 0.0 {
            *px = image::Rgba([color[0], color[1], color[2], (cov[i] * 255.0) as u8]);
        }
    }
    Bitmap { img, ox, oy, k }
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

/// Flou de profondeur de champ : `r` px de composition, l'image s'agrandit pour que le flou déborde
fn defocused(b: Bitmap, r: f32) -> Bitmap {
    let rp = (r * b.k).min(80.0);
    let pad = (rp * 2.0).ceil() as u32;
    let (w, h) = b.img.dimensions();
    let mut big = image::RgbaImage::new(w + pad * 2, h + pad * 2);
    image::imageops::overlay(&mut big, &b.img, pad as i64, pad as i64);
    // Flou en alpha prémultiplié : pas de liseré sombre autour
    for px in big.pixels_mut() {
        let a = px[3] as u32;
        for c in 0..3 {
            px[c] = ((px[c] as u32 * a + 127) / 255) as u8;
        }
    }
    let mut img = image::imageops::fast_blur(&big, rp * 0.6);
    for px in img.pixels_mut() {
        let a = px[3] as u32;
        if a > 0 {
            for c in 0..3 {
                px[c] = ((px[c] as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Bitmap { img, ox: b.ox - pad as f32 / b.k, oy: b.oy - pad as f32 / b.k, k: b.k }
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
        Kind::Noise { w, h, scale, speed, contrast, color, color2, turbulent, seed } => {
            let (pw, ph) = (((w * k).round() as u32).clamp(1, 4096), ((h * k).round() as u32).clamp(1, 4096));
            let z = (t - l.start) * speed;
            let (sc, ct) = (scale.max(1.0), *contrast);
            let mut img = image::RgbaImage::new(pw, ph);
            img.par_chunks_mut(4 * pw as usize).enumerate().for_each(|(y, row)| {
                for x in 0..pw as usize {
                    let (fx, fy) = (x as f32 / k / sc, y as f32 / k / sc);
                    let v = fbm(fx, fy, z, *seed, *turbulent);
                    let v = ((v - 0.5) * ct + 0.5).clamp(0.0, 1.0);
                    let o = x * 4;
                    for c in 0..3 {
                        row[o + c] = (color[c] as f32 + (color2[c] as f32 - color[c] as f32) * v).round() as u8;
                    }
                    row[o + 3] = 255;
                }
            });
            Some(Bitmap { img, ox: -w / 2.0, oy: -h / 2.0, k })
        }
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
        Kind::Path { points, closed, width, color, arrow, fill, keys } => {
            let [w, h] = size(l, cache);
            let points = &points_at(points, keys, t);
            let tr = l.value(&l.trim, t);
            let line = trim_path(&smooth_path(points, *closed), tr[1] / 100.0, tr[0] / 100.0);
            let tip = if *arrow { arrow_head(&line, width.max(0.1)) } else { None };
            // Le trait s'arrête à la base de la pointe : le bout reste pointu
            let body = match tip {
                Some(tri) if line.len() >= 2 => {
                    let mut b = line.clone();
                    let n = b.len();
                    b[n - 1] = [(tri[1][0] + tri[2][0]) / 2.0, (tri[1][1] + tri[2][1]) / 2.0];
                    b
                }
                _ => line,
            };
            let mut bm = stroke_bitmap(&body, width.max(0.1), *color, k, [w / 2.0, h / 2.0]);
            if *width <= 0.0 {
                bm.img.pixels_mut().for_each(|p| p[3] = 0);
            }
            if let Some(tri) = tip {
                fill_triangle(&mut bm, tri, *color);
            }
            // Calque de forme : le fond passe sous le contour
            if let Some(fc) = fill {
                let (bw, bh) = (bm.img.width() as usize, bm.img.height() as usize);
                let cov = poly_coverage(&smooth_path(points, true), [bm.ox, bm.oy], bm.k, bw, bh);
                for (px, c) in bm.img.pixels_mut().zip(cov) {
                    let (sa, fa) = (px[3] as f32 / 255.0, c.min(1.0));
                    let a = sa + fa * (1.0 - sa);
                    if a > 0.0 {
                        for ch in 0..3 {
                            px[ch] = ((px[ch] as f32 * sa + fc[ch] as f32 * fa * (1.0 - sa)) / a).round() as u8;
                        }
                        px[3] = (a * 255.0).round() as u8;
                    }
                }
            }
            Some(bm)
        }
        Kind::Null => None,
        Kind::Compo { path, speed, offset, looped } => {
            if cache.depth >= 4 {
                return None;
            }
            let sub = cache.compo(path)?;
            // Le temps de la précomposition part du début du calque ; après sa fin, la dernière image reste
            let local = remap_time(t - l.start, *speed, *offset, *looped, sub.duration);
            cache.depth += 1;
            // Rendue sur fond transparent (son fond ne recouvre pas la composition)
            let img = render(&sub, local, k, cache);
            cache.depth -= 1;
            Some(Bitmap { img, ox: -(sub.w as f32) / 2.0, oy: -(sub.h as f32) / 2.0, k })
        }
        Kind::Text { text, size, color, font: fname, tracking, align } => {
            use ab_glyph::{Font, ScaleFont};
            let spec = font_spec(l, fname, t);
            let fname = &spec;
            let by = l.reveal.as_ref().map(|r| r.by).unwrap_or_default();
            let lay = layout_text(text, *size, fname, *tracking, *align, by);
            let rise = l.reveal.as_ref().map(|r| r.rise.abs().max(r.slide.abs()) + size * (r.scale - 1.0).max(0.0)).unwrap_or(0.0);
            let pad = size * 0.3 + rise;
            let (bw, bh) = (((lay.w + 2.0 * pad) * k).ceil().clamp(1.0, 8192.0) as u32, ((lay.h + 2.0 * pad) * k).ceil().clamp(1.0, 8192.0) as u32);
            let mut img = image::RgbaImage::from_pixel(bw, bh, image::Rgba([color[0], color[1], color[2], 0]));
            let f = font(fname);
            let sf = f.as_scaled(ab_glyph::PxScale::from(size * k));
            let local = t - l.start;
            for g in &lay.glyphs {
                let (dx, dy, gs, alpha) = match &l.reveal {
                    Some(r) => {
                        let x = (local - r.rank(g.unit, lay.units) * r.delay) / r.dur.max(1e-3);
                        let e = r.ease.apply(x);
                        (r.slide * (1.0 - e), r.rise * (1.0 - e), r.scale + (1.0 - r.scale) * e, Ease::Linear.apply(x * 1.6))
                    }
                    None => (0.0, 0.0, 1.0, 1.0),
                };
                if alpha <= 0.0 || gs <= 0.001 {
                    continue;
                }
                // Mise à l'échelle autour du milieu de la lettre
                let adv = sf.h_advance(f.glyph_id(g.ch));
                let cx = g.x + adv / 2.0;
                let cy = g.y - size * 0.35;
                let px = ab_glyph::PxScale::from(size * k * gs);
                let glyph = ab_glyph::Glyph { id: f.glyph_id(g.ch), scale: px, position: ab_glyph::point((cx + (g.x - cx) * gs + dx + pad) * k, (cy + (g.y - cy) * gs + dy + pad) * k) };
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
    c.publish_controls();
    // Flou de mouvement : seulement s'il y a quelque chose qui bouge
    if let Some(b) = c.blur.filter(|b| b.samples > 1 && b.angle > 0.0) {
        let moving = c.layers.iter().any(|l| l.live(t) && (l.pos.animated() || l.scale.animated() || l.rot.animated() || l.wiggle.is_some() || !l.cycle.is_none()));
        if moving {
            let open = b.angle.min(720.0) / 360.0 / c.fps.max(1.0);
            let n = b.samples.min(64);
            let mut acc: Vec<u32> = vec![];
            let mut size = (0, 0);
            for i in 0..n {
                // Obturateur centré sur l'image (comme la phase -angle/2 d'After Effects)
                let ti = t + open * (i as f32 / (n - 1) as f32 - 0.5);
                let img = render_one(c, ti, scale, cache);
                if acc.is_empty() {
                    acc = vec![0; img.as_raw().len()];
                    size = img.dimensions();
                }
                for (a, v) in acc.iter_mut().zip(img.as_raw()) {
                    *a += *v as u32;
                }
            }
            let raw: Vec<u8> = acc.iter().map(|a| ((*a + n / 2) / n) as u8).collect();
            return image::RgbaImage::from_raw(size.0, size.1, raw).unwrap_or_default();
        }
    }
    render_one(c, t, scale, cache)
}

/// Un calque prêt à poser : son image, sa transformation, son opacité et le cadre (px de sortie) qu'il touche
type Prepped = (Arc<Bitmap>, Xform, f32, [u32; 4]);

fn prep_layer(c: &Compo, li: usize, t: f32, scale: f32, cache: &mut Cache, (w, h): (u32, u32), as_matte: bool) -> Option<Prepped> {
    let l = &c.layers[li];
    // Un cache sert même masqué (comme dans After Effects) ; il doit seulement exister à cet instant
    let alive = if as_matte { t >= l.start && t < l.end } else { l.live(t) };
    if !alive {
        return None;
    }
    let op = (l.eval_prop(Prop::Opacity, t)[0] / 100.0).clamp(0.0, 1.0);
    if op <= 0.0 {
        return None;
    }
    let (x, defocus) = c.projected(li, t)?;
    // Le contenu est calculé à la résolution où il sera vu (net même agrandi)
    let k = (scale * x.max_scale()).clamp(0.02, 8.0);
    // Un contenu qui ne bouge pas à l'intérieur du calque se dessine une fois (l'export reste rapide)
    let still = l.masks.iter().all(|m| m.keys.is_empty())
        && l.fx.iter().all(|e| !e.moves())
        && match (&l.kind, &l.reveal) {
        (Kind::Compo { .. }, _) => false,
        (Kind::Noise { speed, .. }, _) if *speed != 0.0 => false,
        (Kind::Path { keys, .. }, _) => !l.trim.animated() && l.cycle.is_none() && keys.is_empty(),
        (Kind::Text { .. }, _) if l.font_axes.values().any(|a| a.animated()) => false,
        (Kind::Text { text, .. }, Some(r)) => t - l.start >= text.chars().count() as f32 * r.delay + r.dur,
        _ => true,
    };
    let key = still.then(|| {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(&l.kind).unwrap_or_default().hash(&mut h);
        serde_json::to_string(&l.reveal).unwrap_or_default().hash(&mut h);
        serde_json::to_string(&l.style).unwrap_or_default().hash(&mut h);
        serde_json::to_string(&l.trim).unwrap_or_default().hash(&mut h);
        serde_json::to_string(&l.masks).unwrap_or_default().hash(&mut h);
        serde_json::to_string(&l.fx).unwrap_or_default().hash(&mut h);
        serde_json::to_string(&l.font_axes).unwrap_or_default().hash(&mut h);
        ((defocus * 4.0).round() as i32).hash(&mut h);
        (h.finish(), k.to_bits())
    });
    let cached = key.and_then(|k| cache.bitmaps.get(&k).cloned());
    let bm = match cached {
        Some(b) => b,
        None => {
            let mut b = bitmap(l, t, k, cache)?;
            if !l.masks.is_empty() {
                apply_masks(&mut b, &l.masks, t);
            }
            for e in &l.fx {
                b = e.apply(b, t - l.start);
            }
            let b = if l.style.is_plain() { b } else { styled(b, &l.style) };
            let b = if defocus > 0.3 { defocused(b, defocus) } else { b };
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
    (x0 < x1 && y0 < y1).then_some((bm, x, op, [x0, y0, x1, y1]))
}

/// La carte d'un cache de piste (0 : caché, 1 : visible), par pixel de sortie
fn matte_map(c: &Compo, li: usize, t: f32, scale: f32, cache: &mut Cache, (w, h): (u32, u32), m: Matte) -> Vec<f32> {
    use rayon::prelude::*;
    let inverted = matches!(m, Matte::AlphaInverted | Matte::LumaInverted);
    let mut map = vec![if inverted { 1.0 } else { 0.0 }; (w * h) as usize];
    let Some((bm, x, op, [x0, y0, x1, y1])) = prep_layer(c, li, t, scale, cache, (w, h), true) else { return map };
    map.par_chunks_mut(w as usize).enumerate().filter(|(y, _)| (*y as u32) >= y0 && (*y as u32) < y1).for_each(|(py, row)| {
        for px in x0..x1 {
            let world = [(px as f32 + 0.5) / scale, (py as f32 + 0.5) / scale];
            let Some(lp) = x.inverse(world) else { return };
            let s = sample(&bm.img, (lp[0] - bm.ox) * bm.k, (lp[1] - bm.oy) * bm.k);
            let a = s[3] * op;
            // Luminance du calque posé sur du noir (couleurs prémultipliées)
            let luma = (0.2126 * s[0] + 0.7152 * s[1] + 0.0722 * s[2]) / 255.0 * op;
            row[px as usize] = match m {
                Matte::Alpha => a,
                Matte::AlphaInverted => 1.0 - a,
                Matte::Luma => luma,
                Matte::LumaInverted => 1.0 - luma,
            }
            .clamp(0.0, 1.0);
        }
    });
    map
}

fn render_one(c: &Compo, t: f32, scale: f32, cache: &mut Cache) -> image::RgbaImage {
    use rayon::prelude::*;
    let (w, h) = (((c.w as f32 * scale).round() as u32).max(1), ((c.h as f32 * scale).round() as u32).max(1));
    // Une précomposition se rend sur fond transparent
    let mut out = image::RgbaImage::from_pixel(w, h, image::Rgba([c.bg[0], c.bg[1], c.bg[2], if cache.depth > 0 { 0 } else { 255 }]));
    // Les calques qui servent de cache au calque d'en dessous ne se dessinent pas eux-mêmes
    let mattes: Vec<usize> = (1..c.layers.len()).filter(|&i| c.layers[i].matte.is_some()).map(|i| i - 1).collect();
    for li in c.draw_order(t) {
        if mattes.contains(&li) {
            continue;
        }
        let Some((bm, x0form, op0, bb0)) = prep_layer(c, li, t, scale, cache, (w, h), false) else { continue };
        let mask = match c.layers[li].matte {
            Some(m) if li > 0 => Some(matte_map(c, li - 1, t, scale, cache, (w, h), m)),
            _ => None,
        };
        // Le calque, ou chacune de ses copies (répétiteur), de la première à la dernière
        let copies: Vec<(Xform, f32, [u32; 4])> = match &c.layers[li].repeat {
            Some(rp) if rp.copies > 1 => (0..rp.copies.min(200))
                .filter_map(|k| {
                    let (tk, ok) = rp.copy(k);
                    let xk = x0form.after(&tk);
                    let (bw, bh) = (bm.img.width() as f32 / bm.k, bm.img.height() as f32 / bm.k);
                    let pts = [[bm.ox, bm.oy], [bm.ox + bw, bm.oy], [bm.ox + bw, bm.oy + bh], [bm.ox, bm.oy + bh]].map(|p| xk.apply(p));
                    let x0 = (pts.iter().map(|p| p[0]).fold(f32::MAX, f32::min) * scale).floor().max(0.0) as u32;
                    let x1 = (pts.iter().map(|p| p[0]).fold(f32::MIN, f32::max) * scale).ceil().min(w as f32) as u32;
                    let y0 = (pts.iter().map(|p| p[1]).fold(f32::MAX, f32::min) * scale).floor().max(0.0) as u32;
                    let y1 = (pts.iter().map(|p| p[1]).fold(f32::MIN, f32::max) * scale).ceil().min(h as f32) as u32;
                    (x0 < x1 && y0 < y1 && ok > 0.0).then_some((xk, op0 * ok, [x0, y0, x1, y1]))
                })
                .collect(),
            _ => vec![(x0form, op0, bb0)],
        };
        for (x, op, [x0, y0, x1, y1]) in copies {
        let row_len = w as usize * 4;
        let raw: &mut [u8] = &mut out;
        raw.par_chunks_mut(row_len).enumerate().filter(|(y, _)| (*y as u32) >= y0 && (*y as u32) < y1).for_each(|(py, row)| {
            for px in x0..x1 {
                let world = [(px as f32 + 0.5) / scale, (py as f32 + 0.5) / scale];
                let Some(lp) = x.inverse(world) else { return };
                let s = sample(&bm.img, (lp[0] - bm.ox) * bm.k, (lp[1] - bm.oy) * bm.k);
                let a = s[3] * op * mask.as_ref().map_or(1.0, |m| m[py * w as usize + px as usize]);
                if a <= 0.001 {
                    continue;
                }
                let i = px as usize * 4;
                // « Par-dessus » avec l'alpha du fond (opaque dans une composition, transparent dans une précomposition)
                let da = row[i + 3] as f32 / 255.0;
                let oa = a + da * (1.0 - a);
                for ch in 0..3 {
                    let src = s[ch] / s[3].max(1e-6);
                    row[i + ch] = ((src * a + row[i + ch] as f32 * da * (1.0 - a)) / oa.max(1e-6)).round().clamp(0.0, 255.0) as u8;
                }
                row[i + 3] = (oa * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        });
        }
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

// ---------- Rendu pour le montage ----------

/// La racine du coffre qui contient ce fichier (le dossier qui a un `.app`)
pub fn vault_root_of(p: &Path) -> Option<PathBuf> {
    p.ancestors().skip(1).find(|a| a.join(".app").is_dir()).map(|a| a.to_path_buf())
}

/// Où se range la vidéo rendue d'une composition posée dans un montage
pub fn cache_path(comp: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    comp.to_string_lossy().hash(&mut h);
    let stem = comp.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "compo".into());
    let base = vault_root_of(comp).map(|r| r.join(".app")).unwrap_or_else(|| comp.parent().map(|d| d.join(".aryadne")).unwrap_or_default());
    base.join("cache").join("motion").join(format!("{stem}-{:08x}.mp4", h.finish() as u32))
}

/// Compte les rendus terminés : un montage qui le voit changer relit les vidéos des compositions
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn generation() -> u64 {
    GENERATION.load(std::sync::atomic::Ordering::Relaxed)
}

/// Une vidéo du cache des compositions (ses vignettes deviennent périmées quand elle est rendue à nouveau)
pub fn is_cache_file(p: &Path) -> bool {
    let s = p.to_string_lossy().replace('\\', "/");
    s.contains("/cache/motion/")
}

pub(crate) fn running() -> &'static std::sync::Mutex<std::collections::HashSet<PathBuf>> {
    static R: OnceLock<std::sync::Mutex<std::collections::HashSet<PathBuf>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

/// Signale un rendu terminé (le montage relit les vidéos)
pub(crate) fn bump_generation() {
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn stale(comp: &Path, out: &Path) -> bool {
    let m = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    match (m(comp), m(out)) {
        (Some(a), Some(b)) => a > b,
        _ => true,
    }
}

fn render_now(comp: &Path, out: &Path) -> Result<(), String> {
    let c = Compo::read(comp)?;
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let root = vault_root_of(comp).unwrap_or_else(|| comp.parent().map(|d| d.to_path_buf()).unwrap_or_default());
    // Rendu à côté, puis remplacement d'un coup : l'ancienne version reste lisible pendant ce temps
    let part = out.with_extension("part.mp4");
    export(&c, &part, &root, |_| {})?;
    std::fs::rename(&part, out).map_err(|e| e.to_string())
}

/// La vidéo d'une composition, à jour : rendue tout de suite (`wait`) ou en arrière-plan si elle a changé.
/// Le chemin rendu est toujours celui du cache, même pendant qu'une nouvelle version se prépare.
pub fn render_cached(comp: &Path, wait: bool) -> PathBuf {
    let out = cache_path(comp);
    if !stale(comp, &out) {
        return out;
    }
    let busy = !running().lock().unwrap().insert(out.clone());
    if busy {
        if wait {
            while running().lock().unwrap().contains(&out) {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        return out;
    }
    let (c2, o2) = (comp.to_path_buf(), out.clone());
    let job = move || {
        if let Err(e) = render_now(&c2, &o2) {
            eprintln!("rendu de {} : {e}", c2.display());
        }
        running().lock().unwrap().remove(&o2);
        GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    };
    if wait {
        job();
    } else {
        std::thread::spawn(job);
    }
    out
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
            l.scale.insert(Key { t, v: [0.0, 0.0], ease: Ease::Bounce, tan: vec![] });
            l.scale.insert(Key { t: t + 0.6, v: s, ease: Ease::Smooth, tan: vec![] });
        }
        "Glissement doux" => {
            let p = l.pos.at(t);
            l.pos.keys.retain(|k| k.t < t - KEY_EPS || k.t > t + 0.8 + KEY_EPS);
            l.pos.insert(Key { t, v: [p[0] - 240.0, p[1]], ease: Ease::Out, tan: vec![] });
            l.pos.insert(Key { t: t + 0.8, v: p, ease: Ease::Smooth, tan: vec![] });
            fade(l, t, 0.5, true);
        }
        "Fondu d'entrée" => fade(l, t, 0.6, true),
        "Sortie en fondu" => {
            let e = l.end;
            fade(l, (e - 0.6).max(l.start), 0.6, false)
        }
        "Tour complet" => {
            let r = l.rot.at(t)[0];
            l.rot.insert(Key { t, v: [r, 0.0], ease: Ease::Smooth, tan: vec![] });
            l.rot.insert(Key { t: t + 1.0, v: [r + 360.0, 0.0], ease: Ease::Smooth, tan: vec![] });
            l.rot.sort();
        }
        _ => {}
    }
}

fn fade(l: &mut Layer, t: f32, d: f32, enter: bool) {
    let full = if l.opacity.animated() { l.opacity.keys.iter().map(|k| k.v[0]).fold(0.0, f32::max).max(1.0) } else { l.opacity.v[0].max(1.0) };
    let (a, b) = if enter { (0.0, full) } else { (full, 0.0) };
    l.opacity.keys.retain(|k| k.t < t - KEY_EPS || k.t > t + d + KEY_EPS);
    l.opacity.insert(Key { t, v: [a, 0.0], ease: Ease::Smooth, tan: vec![] });
    l.opacity.insert(Key { t: t + d, v: [b, 0.0], ease: Ease::Smooth, tan: vec![] });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flou_de_mouvement() {
        let mut c = Compo { layers: vec![], ..Compo::default() };
        (c.w, c.h, c.bg) = (200, 40, [0, 0, 0]);
        let mut r = Layer::new("r", Kind::Rect { w: 10.0, h: 10.0, radius: 0.0, color: [255, 255, 255] }, &c);
        r.pos.keys = vec![Key { t: 0.0, v: [10.0, 20.0], ease: Ease::Linear, tan: vec![] }, Key { t: 0.1, v: [190.0, 20.0], ease: Ease::Linear, tan: vec![] }];
        c.layers = vec![r];
        let mut cache = Cache::new(Path::new("."));
        let sharp = render(&c, 0.05, 1.0, &mut cache);
        c.blur = Some(MotionBlur::default());
        let soft = render(&c, 0.05, 1.0, &mut cache);
        // Net : le carré blanc fait 10 px ; flou : la traînée s'étale, plus large et moins blanche
        let lit = |img: &image::RgbaImage| (0..200).filter(|&x| img.get_pixel(x, 20)[0] > 8).count();
        assert!(lit(&sharp) <= 12, "{}", lit(&sharp));
        assert!(lit(&soft) > 12, "{}", lit(&soft));
        assert!(soft.get_pixel(100, 20)[0] < 255);
    }

    #[test]
    fn trace_progressif() {
        // Une ligne de 100 px : à moitié dessinée, elle s'arrête au milieu
        let line = vec![[-50.0, 0.0], [50.0, 0.0]];
        let half = trim_path(&line, 0.0, 0.5);
        assert_eq!(half.last().unwrap(), &[0.0, 0.0]);
        assert!(trim_path(&line, 0.3, 0.3).is_empty());
        // Le lissage passe par les points donnés
        let sm = smooth_path(&[[0.0, 0.0], [10.0, 10.0], [20.0, 0.0]], false);
        assert!(sm.iter().any(|p| (p[0] - 10.0).abs() < 1e-4 && (p[1] - 10.0).abs() < 1e-4));
        let mut c = Compo { layers: vec![], ..Compo::default() };
        (c.w, c.h, c.bg) = (120, 20, [0, 0, 0]);
        let mut l = Layer::new("t", Kind::Path { points: line, closed: false, width: 6.0, color: [255, 255, 255], arrow: false, fill: None, keys: vec![] }, &c);
        l.pos.v = [60.0, 10.0];
        l.trim = Track::new(50.0, 0.0);
        c.layers = vec![l];
        let img = render(&c, 0.0, 1.0, &mut Cache::new(Path::new(".")));
        assert!(img.get_pixel(30, 10)[0] > 200);
        assert!(img.get_pixel(90, 10)[0] < 10);
        // Avec une flèche : la pointe dépasse l'épaisseur du trait, juste avant le bout
        if let Kind::Path { arrow, .. } = &mut c.layers[0].kind {
            *arrow = true;
        }
        let img = render(&c, 0.0, 1.0, &mut Cache::new(Path::new(".")));
        assert!(img.get_pixel(46, 4)[0] > 200, "{:?}", img.get_pixel(46, 4));
        assert!(img.get_pixel(64, 10)[0] < 10);
    }

    #[test]
    fn precomposition() {
        let root = std::env::temp_dir().join(format!("ary-precomp-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        // La précomposition : 40 × 20, fond vert, un carré rouge de 10 px au centre
        let mut sub = Compo { layers: vec![], ..Compo::default() };
        (sub.w, sub.h, sub.bg, sub.duration) = (40, 20, [0, 255, 0], 2.0);
        let mut r = Layer::new("r", Kind::Rect { w: 10.0, h: 10.0, radius: 0.0, color: [255, 0, 0] }, &sub);
        r.pos.v = [20.0, 10.0];
        sub.layers = vec![r];
        sub.write(&root.join("sub.arymotion")).unwrap();
        let mut c = Compo { layers: vec![], ..Compo::default() };
        (c.w, c.h, c.bg) = (100, 40, [0, 0, 255]);
        let mut l = Layer::new("p", Kind::Compo { path: "sub.arymotion".into(), speed: 1.0, offset: 0.0, looped: false }, &c);
        l.pos.v = [50.0, 20.0];
        c.layers = vec![l];
        let mut cache = Cache::new(&root);
        let img = render(&c, 0.5, 1.0, &mut cache);
        assert_eq!(img.get_pixel(50, 20).0, [255, 0, 0, 255]);
        // Là où la précomposition n'a que son fond : le bleu de la composition (le vert ne recouvre pas)
        assert_eq!(img.get_pixel(35, 20).0, [0, 0, 255, 255]);
        assert_eq!(img.get_pixel(5, 5).0, [0, 0, 255, 255]);
        let _ = std::fs::remove_dir_all(&root);
        // Remappage : deux fois plus vite, en boucle sur 2 s ; à l'envers depuis la fin
        assert!((remap_time(1.5, 2.0, 0.0, true, 2.0) - 1.0).abs() < 1e-5);
        assert!((remap_time(0.5, -1.0, 2.0, false, 2.0) - 1.5).abs() < 1e-5);
        assert!(remap_time(9.0, 1.0, 0.0, false, 2.0) < 2.0);
    }

    #[test]
    fn cache_de_piste() {
        let mut c = Compo { layers: vec![], ..Compo::default() };
        (c.w, c.h, c.bg) = (100, 20, [0, 0, 0]);
        // En haut : un carré blanc à gauche (le cache) ; dessous : un rectangle rouge sur toute la largeur
        let mut m = Layer::new("cache", Kind::Rect { w: 20.0, h: 20.0, radius: 0.0, color: [255, 255, 255] }, &c);
        m.pos.v = [10.0, 10.0];
        let mut r = Layer::new("rouge", Kind::Rect { w: 100.0, h: 20.0, radius: 0.0, color: [255, 0, 0] }, &c);
        r.pos.v = [50.0, 10.0];
        r.matte = Some(Matte::Alpha);
        c.layers = vec![m, r];
        let mut cache = Cache::new(Path::new("."));
        let img = render(&c, 0.0, 1.0, &mut cache);
        // Rouge seulement sous le carré ; le cache lui-même n'est pas dessiné (pas de blanc)
        assert_eq!(img.get_pixel(10, 10).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(60, 10).0, [0, 0, 0, 255]);
        c.layers[1].matte = Some(Matte::AlphaInverted);
        let inv = render(&c, 0.0, 1.0, &mut cache);
        assert_eq!(inv.get_pixel(10, 10).0, [0, 0, 0, 255]);
        assert_eq!(inv.get_pixel(60, 10).0, [255, 0, 0, 255]);
    }

    #[test]
    fn tremblement() {
        // Doux (pas de saut d'une image à l'autre), borné, et différent d'un calque à l'autre
        let mut prev = smooth_noise(0.0, 0, 7);
        let mut lo = 0.0f32;
        let mut hi = 0.0f32;
        for k in 1..2000 {
            let v = smooth_noise(k as f32 * 0.01, 0, 7);
            assert!((v - prev).abs() < 0.08, "saut à {k}");
            assert!(v.abs() <= 1.0);
            lo = lo.min(v);
            hi = hi.max(v);
            prev = v;
        }
        assert!(hi - lo > 0.8, "bouge vraiment ({lo}..{hi})");
        assert_ne!(smooth_noise(3.3, 0, 7), smooth_noise(3.3, 0, 8));
        let c = Compo::default();
        let mut l = Layer::new("a", Kind::Rect { w: 10.0, h: 10.0, radius: 0.0, color: [255; 3] }, &c);
        let still = l.xform(1.0);
        l.wiggle = Some(Wiggle::default());
        let moved = l.xform(1.0);
        assert!((moved.e - still.e).abs() + (moved.f - still.f).abs() > 0.01);
    }

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
    fn video_pour_le_montage() {
        if crate::ffmpeg::find("ffmpeg").is_none() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("aryadne-motion-{}", new_id()));
        std::fs::create_dir_all(dir.join(".app")).unwrap();
        let comp = dir.join("Intro.arymotion");
        let mut c = Compo::default();
        (c.w, c.h, c.duration) = (320, 180, 0.5);
        c.write(&comp).unwrap();
        let out = render_cached(&comp, true);
        assert!(out.starts_with(dir.join(".app").join("cache")), "{out:?}");
        assert!(out.exists() && !stale(&comp, &out));
        // Le montage lit la composition par sa vidéo, et sait sa durée
        let proj = dir.join("Film.aryseq");
        assert_eq!(crate::montage::resolve(&proj, "Intro.arymotion"), out);
        let clip = crate::montage::import(&proj, &comp, 0, 0.0).unwrap();
        assert_eq!((clip.file.as_str(), clip.kind.as_str(), clip.media), ("Intro.arymotion", "video", 0.5));
        // Une composition changée est rendue à nouveau (attendre la fin avant de ranger)
        std::thread::sleep(std::time::Duration::from_millis(20));
        c.duration = 0.4;
        c.write(&comp).unwrap();
        let g = generation();
        render_cached(&comp, true);
        assert!(generation() > g && !stale(&comp, &out));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn masques_et_formes() {
        let mut c = Compo { layers: vec![], ..Compo::default() };
        (c.w, c.h, c.bg, c.duration) = (100, 40, [0, 0, 0], 2.0);
        let mut l = Layer::new("r", Kind::Rect { w: 100.0, h: 40.0, radius: 0.0, color: [255, 255, 255] }, &c);
        l.pos.v = [50.0, 20.0];
        // Un masque sur la moitié gauche, qui glisse jusqu'à la moitié droite en 1 s
        let mut m = Mask::rect(50.0, 40.0);
        m.points.iter_mut().for_each(|p| p[0] -= 25.0);
        let right: Vec<[f32; 2]> = m.points.iter().map(|p| [p[0] + 50.0, p[1]]).collect();
        m.keys = vec![PathKey { t: 0.0, points: m.points.clone() }, PathKey { t: 1.0, points: right }];
        l.masks = vec![m];
        c.layers = vec![l];
        let mut cache = Cache::new(Path::new("."));
        let img = render(&c, 0.0, 1.0, &mut cache);
        assert!(img.get_pixel(10, 20)[0] > 200 && img.get_pixel(90, 20)[0] < 10);
        let img = render(&c, 1.0, 1.0, &mut cache);
        assert!(img.get_pixel(10, 20)[0] < 10 && img.get_pixel(90, 20)[0] > 200);
        // Inversé et soustrait : le contraire
        c.layers[0].masks[0].invert = true;
        let img = render(&c, 1.0, 1.0, &mut cache);
        assert!(img.get_pixel(10, 20)[0] > 200 && img.get_pixel(90, 20)[0] < 10);
        // Calque de forme : un triangle rempli, sans contour
        let tri = vec![[-20.0, 15.0], [20.0, 15.0], [0.0, -15.0]];
        let mut f = Layer::new("f", Kind::Path { points: tri, closed: true, width: 0.0, color: [0, 0, 0], arrow: false, fill: Some([0, 255, 0]), keys: vec![] }, &c);
        f.pos.v = [50.0, 20.0];
        c.layers = vec![f];
        let img = render(&c, 0.0, 1.0, &mut cache);
        assert!(img.get_pixel(50, 25)[1] > 200, "{:?}", img.get_pixel(50, 25));
        assert!(img.get_pixel(5, 5)[1] < 10);
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

fn is_false_b(b: &bool) -> bool {
    !*b
}

#[cfg(test)]
mod orient_tests {
    use super::*;

    #[test]
    fn orientation_automatique() {
        let c = Compo::default();
        let mut l = Layer::new("avion", Kind::Rect { w: 10.0, h: 4.0, radius: 0.0, color: [255; 3] }, &c);
        // De (0, 0) à (100, 100) : 45° vers le bas à droite
        l.pos.keys = vec![Key { t: 0.0, v: [0.0, 0.0], ease: Ease::Linear, tan: vec![] }, Key { t: 1.0, v: [100.0, 100.0], ease: Ease::Linear, tan: vec![] }];
        assert!((l.xform(0.5).angle()).abs() < 1e-3);
        l.auto_orient = true;
        assert!((l.xform(0.5).angle() - 45.0).abs() < 0.5, "{}", l.xform(0.5).angle());
    }
}

/// Un SVG en calques de forme (un tracé par sous-chemin, avec son remplissage et son contour), centré dans
/// la composition. Les courbes et les lignes sont échantillonnées serré : le tracé lissé de Motion les suit.
pub fn svg_layers(svg: &str, compo: &Compo) -> Result<Vec<Layer>, String> {
    use resvg::usvg;
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).map_err(|e| e.to_string())?;
    // (points, fermé, remplissage, contour (largeur, couleur), nom)
    let mut shapes: Vec<(Vec<[f32; 2]>, bool, Option<[u8; 3]>, Option<(f32, [u8; 3])>, String)> = vec![];
    fn rgb(p: &usvg::Paint) -> Option<[u8; 3]> {
        match p {
            usvg::Paint::Color(c) => Some([c.red, c.green, c.blue]),
            // Dégradés et motifs : une couleur moyenne grise faute de mieux
            _ => Some([0x99, 0x99, 0x99]),
        }
    }
    fn walk(g: &usvg::Group, out: &mut Vec<(Vec<[f32; 2]>, bool, Option<[u8; 3]>, Option<(f32, [u8; 3])>, String)>) {
        for node in g.children() {
            match node {
                usvg::Node::Group(gg) => walk(gg, out),
                usvg::Node::Path(p) => {
                    let t = p.abs_transform();
                    let tp = |x: f32, y: f32| [t.sx * x + t.kx * y + t.tx, t.ky * x + t.sy * y + t.ty];
                    let fill = p.fill().and_then(|f| rgb(f.paint()));
                    let stroke = p.stroke().and_then(|s| rgb(s.paint()).map(|c| (s.width().get() * (t.sx.abs() + t.sy.abs()) / 2.0, c)));
                    if fill.is_none() && stroke.is_none() {
                        continue;
                    }
                    let name = if p.id().is_empty() { "SVG".to_string() } else { p.id().to_string() };
                    let mut cur: Vec<[f32; 2]> = vec![];
                    let mut closed = false;
                    // Points tous les 4 px environ, pour que le lissage garde les coins et les droites
                    let push_line = |cur: &mut Vec<[f32; 2]>, e: [f32; 2]| {
                        if let Some(&a) = cur.last() {
                            let n = (((e[0] - a[0]).hypot(e[1] - a[1])) / 4.0).ceil().max(1.0) as usize;
                            for k in 1..=n {
                                let f = k as f32 / n as f32;
                                cur.push([a[0] + (e[0] - a[0]) * f, a[1] + (e[1] - a[1]) * f]);
                            }
                        } else {
                            cur.push(e);
                        }
                    };
                    let mut flush = |cur: &mut Vec<[f32; 2]>, closed: &mut bool, out: &mut Vec<_>| {
                        if cur.len() >= 2 {
                            out.push((std::mem::take(cur), *closed || fill.is_some(), fill, stroke, name.clone()));
                        }
                        cur.clear();
                        *closed = false;
                    };
                    for seg in p.data().segments() {
                        use usvg::tiny_skia_path::PathSegment::*;
                        match seg {
                            MoveTo(a) => {
                                flush(&mut cur, &mut closed, out);
                                cur.push(tp(a.x, a.y));
                            }
                            LineTo(a) => push_line(&mut cur, tp(a.x, a.y)),
                            QuadTo(c, a) => {
                                let s0 = *cur.last().unwrap_or(&[0.0, 0.0]);
                                let (c, e) = (tp(c.x, c.y), tp(a.x, a.y));
                                let n = ((s0[0] - e[0]).hypot(s0[1] - e[1]) / 4.0).ceil().clamp(2.0, 64.0) as usize;
                                for k in 1..=n {
                                    let f = k as f32 / n as f32;
                                    let u = 1.0 - f;
                                    cur.push([u * u * s0[0] + 2.0 * u * f * c[0] + f * f * e[0], u * u * s0[1] + 2.0 * u * f * c[1] + f * f * e[1]]);
                                }
                            }
                            CubicTo(c1, c2, a) => {
                                let s0 = *cur.last().unwrap_or(&[0.0, 0.0]);
                                let (c1, c2, e) = (tp(c1.x, c1.y), tp(c2.x, c2.y), tp(a.x, a.y));
                                let n = ((s0[0] - e[0]).hypot(s0[1] - e[1]) / 4.0).ceil().clamp(3.0, 96.0) as usize;
                                for k in 1..=n {
                                    let f = k as f32 / n as f32;
                                    let u = 1.0 - f;
                                    let b = [u * u * u, 3.0 * u * u * f, 3.0 * u * f * f, f * f * f];
                                    cur.push([b[0] * s0[0] + b[1] * c1[0] + b[2] * c2[0] + b[3] * e[0], b[0] * s0[1] + b[1] * c1[1] + b[2] * c2[1] + b[3] * e[1]]);
                                }
                            }
                            Close => {
                                if let Some(&f0) = cur.first() {
                                    push_line(&mut cur, f0);
                                    cur.pop();
                                }
                                closed = true;
                                flush(&mut cur, &mut closed, out);
                            }
                        }
                    }
                    flush(&mut cur, &mut closed, out);
                }
                _ => {}
            }
        }
    }
    walk(tree.root(), &mut shapes);
    if shapes.is_empty() {
        return Err(crate::i18n::t("Aucun tracé dans ce SVG"));
    }
    // Tout le dessin est centré dans la composition, à une taille qui y tient
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for (pts, ..) in &shapes {
        for p in pts {
            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
        }
    }
    let size = [(hi[0] - lo[0]).max(1.0), (hi[1] - lo[1]).max(1.0)];
    let k = (compo.w as f32 * 0.6 / size[0]).min(compo.h as f32 * 0.6 / size[1]).min(8.0);
    let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    let mut layers = vec![];
    for (i, (pts, closed, fill, stroke, name)) in shapes.into_iter().enumerate() {
        // Chaque calque a son origine au centre de sa forme
        let (mut a, mut b) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in &pts {
            a = [a[0].min(p[0]), a[1].min(p[1])];
            b = [b[0].max(p[0]), b[1].max(p[1])];
        }
        let c = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        let local: Vec<[f32; 2]> = pts.iter().map(|p| [(p[0] - c[0]) * k, (p[1] - c[1]) * k]).collect();
        let (width, color) = stroke.map(|(w, c)| (w * k, c)).unwrap_or((0.0, fill.unwrap_or([255; 3])));
        let mut l = Layer::new(&format!("{name} {}", i + 1), Kind::Path { points: local, closed, width, color, arrow: false, fill, keys: vec![] }, compo);
        l.pos.v = [compo.w as f32 / 2.0 + (c[0] - mid[0]) * k, compo.h as f32 / 2.0 + (c[1] - mid[1]) * k];
        layers.push(l);
    }
    // Le premier tracé du SVG est dessous : dans Motion, le calque 0 est au-dessus
    layers.reverse();
    Ok(layers)
}

#[cfg(test)]
mod svg_tests {
    #[test]
    fn svg_en_calques() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"><rect id="fond" x="0" y="0" width="100" height="50" fill="#ff0000"/><path d="M10 10 C 20 0, 40 0, 50 10" stroke="#0000ff" stroke-width="2" fill="none"/></svg>"##;
        let c = super::Compo::default();
        let l = super::svg_layers(svg, &c).unwrap();
        assert_eq!(l.len(), 2);
        // Le rectangle (premier du SVG) est dessous, donc en dernier
        match &l[1].kind {
            super::Kind::Path { points, closed, fill, .. } => {
                assert!(*closed && *fill == Some([255, 0, 0]) && points.len() > 20);
            }
            _ => panic!(),
        }
        match &l[0].kind {
            super::Kind::Path { closed, fill, color, width, .. } => assert!(!*closed && fill.is_none() && *color == [0, 0, 255] && *width > 2.0),
            _ => panic!(),
        }
        // Centré dans la composition
        assert!((l[1].pos.v[0] - c.w as f32 / 2.0).abs() < 1.0);
    }
}


/// Bruit de valeur 3D (x, y, temps), entre 0 et 1
fn vnoise3(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let h = |i: i32, j: i32, k: i32| -> f32 {
        let mut n = (i as u32).wrapping_mul(0x8da6_b343) ^ (j as u32).wrapping_mul(0xd816_3841) ^ (k as u32).wrapping_mul(0xcb1a_b31f) ^ seed.wrapping_mul(0x9e37_79b9);
        n = (n ^ (n >> 13)).wrapping_mul(0x5bd1_e995);
        ((n ^ (n >> 15)) & 0xffff) as f32 / 65535.0
    };
    let (xi, yi, zi) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (fx, fy, fz) = (s(x - xi as f32), s(y - yi as f32), s(z - zi as f32));
    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let face = |k: i32| l(l(h(xi, yi, k), h(xi + 1, yi, k), fx), l(h(xi, yi + 1, k), h(xi + 1, yi + 1, k), fx), fy);
    l(face(zi), face(zi + 1), fz)
}

/// Bruit fractal (5 octaves) ; turbulent : la somme des écarts, en filaments
pub fn fbm(x: f32, y: f32, z: f32, seed: u32, turbulent: bool) -> f32 {
    let (mut sum, mut amp, mut f, mut norm) = (0.0, 0.5, 1.0, 0.0);
    for o in 0..5 {
        let v = vnoise3(x * f, y * f, z * (1.0 + o as f32 * 0.3), seed.wrapping_add(o * 101));
        sum += if turbulent { (v - 0.5).abs() * 2.0 } else { v } * amp;
        norm += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    sum / norm
}

/// Effets de déformation d'un calque (ondulation, turbulence, décalage), appliqués à son image
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Effect {
    /// Ondulation : amplitude (px), longueur d'onde (px), vitesse (ondes par seconde), sens vertical
    Wave { amp: f32, length: f32, speed: f32, #[serde(default)] vertical: bool },
    /// Turbulence : déplacement par un bruit fractal (quantité en px, taille des remous, évolution)
    Turbulence { amount: f32, size: f32, speed: f32 },
    /// Décalage : l'image glisse et revient par l'autre bord (px par seconde en x et y)
    Offset { x: f32, y: f32 },
    /// Marionnette : des épingles qu'on déplace, l'image se plie autour (sans se déchirer)
    Puppet { pins: Vec<Pin> },
}

/// Une épingle de marionnette : sa place sur l'image au repos, et où elle est tirée (animable, px du calque)
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Pin {
    pub rest: [f32; 2],
    pub at: Track,
}

/// Déformation rigide par moindres carrés mobiles : où va le point `v` quand les points `from` vont en `to`
pub fn mls_rigid(v: [f32; 2], from: &[[f32; 2]], to: &[[f32; 2]]) -> [f32; 2] {
    if from.is_empty() {
        return v;
    }
    let mut ws = Vec::with_capacity(from.len());
    for p in from {
        let d2 = (p[0] - v[0]).powi(2) + (p[1] - v[1]).powi(2);
        if d2 < 1e-8 {
            let i = ws.len();
            return to[i];
        }
        ws.push(1.0 / d2);
    }
    let sw: f32 = ws.iter().sum();
    let pc = [from.iter().zip(&ws).map(|(p, w)| p[0] * w).sum::<f32>() / sw, from.iter().zip(&ws).map(|(p, w)| p[1] * w).sum::<f32>() / sw];
    let qc = [to.iter().zip(&ws).map(|(q, w)| q[0] * w).sum::<f32>() / sw, to.iter().zip(&ws).map(|(q, w)| q[1] * w).sum::<f32>() / sw];
    let (mut a, mut b) = (0.0f32, 0.0f32);
    for ((p, q), w) in from.iter().zip(to).zip(&ws) {
        let (ph, qh) = ([p[0] - pc[0], p[1] - pc[1]], [q[0] - qc[0], q[1] - qc[1]]);
        a += w * (ph[0] * qh[0] + ph[1] * qh[1]);
        b += w * (ph[0] * qh[1] - ph[1] * qh[0]);
    }
    let th = if a.abs() + b.abs() > 1e-9 { b.atan2(a) } else { 0.0 };
    let (c, s) = (th.cos(), th.sin());
    let d = [v[0] - pc[0], v[1] - pc[1]];
    [c * d[0] - s * d[1] + qc[0], s * d[0] + c * d[1] + qc[1]]
}

/// La marionnette appliquée à une image de calque (épingles à l'instant `t`)
fn puppet(b: Bitmap, pins: &[Pin], t: f32) -> Bitmap {
    if pins.is_empty() {
        return b;
    }
    let rest: Vec<[f32; 2]> = pins.iter().map(|p| p.rest).collect();
    let now: Vec<[f32; 2]> = pins.iter().map(|p| p.at.at(t)).collect();
    if rest == now {
        return b;
    }
    let (w, h) = (b.img.width() as usize, b.img.height() as usize);
    // L'image grandit de ce que les épingles se déplacent (un bras levé sort du cadre d'origine)
    let reach = rest.iter().zip(&now).map(|(a, c)| (a[0] - c[0]).hypot(a[1] - c[1])).fold(0.0f32, f32::max);
    let pad = ((reach * b.k).ceil() as usize).min(2000);
    let (ow, oh) = (w + pad * 2, h + pad * 2);
    let (ox, oy) = (b.ox - pad as f32 / b.k, b.oy - pad as f32 / b.k);
    // Carte inverse (image déformée → image au repos) calculée sur une grille, puis interpolée
    const G: usize = 6;
    let (gw, gh) = (ow / G + 2, oh / G + 2);
    let grid: Vec<[f32; 2]> = (0..gw * gh)
        .into_par_iter()
        .map(|i| {
            let (gx, gy) = ((i % gw * G) as f32, (i / gw * G) as f32);
            let v = [ox + gx / b.k, oy + gy / b.k];
            let r = mls_rigid(v, &now, &rest);
            [(r[0] - b.ox) * b.k, (r[1] - b.oy) * b.k]
        })
        .collect();
    let src = &b.img;
    let mut out = vec![0u8; ow * oh * 4];
    out.par_chunks_mut(ow * 4).enumerate().for_each(|(y, row)| {
        for x in 0..ow {
            let (gx, gy) = (x as f32 / G as f32, y as f32 / G as f32);
            let (x0, y0) = (gx.floor() as usize, gy.floor() as usize);
            let (fx, fy) = (gx - x0 as f32, gy - y0 as f32);
            let g = |i: usize, j: usize| grid[(j.min(gh - 1)) * gw + i.min(gw - 1)];
            let (a, bb, c, d) = (g(x0, y0), g(x0 + 1, y0), g(x0, y0 + 1), g(x0 + 1, y0 + 1));
            let sx = (a[0] * (1.0 - fx) + bb[0] * fx) * (1.0 - fy) + (c[0] * (1.0 - fx) + d[0] * fx) * fy;
            let sy = (a[1] * (1.0 - fx) + bb[1] * fx) * (1.0 - fy) + (c[1] * (1.0 - fx) + d[1] * fx) * fy;
            let s = sample(src, sx, sy);
            // `sample` rend des couleurs prémultipliées
            let al = s[3];
            if al > 0.002 {
                for ch in 0..3 {
                    row[x * 4 + ch] = (s[ch] / al).round().clamp(0.0, 255.0) as u8;
                }
                row[x * 4 + 3] = (al * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    });
    Bitmap { img: image::RgbaImage::from_raw(ow as u32, oh as u32, out).unwrap_or_default(), ox, oy, k: b.k }
}

impl Effect {
    pub const ALL: [&'static str; 4] = ["Ondulation", "Turbulence", "Décalage", "Marionnette"];

    pub fn new(i: usize) -> Effect {
        match i {
            0 => Effect::Wave { amp: 12.0, length: 120.0, speed: 1.0, vertical: false },
            1 => Effect::Turbulence { amount: 20.0, size: 80.0, speed: 0.5 },
            3 => Effect::Puppet { pins: vec![] },
            _ => Effect::Offset { x: 60.0, y: 0.0 },
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Effect::Wave { .. } => Self::ALL[0],
            Effect::Turbulence { .. } => Self::ALL[1],
            Effect::Offset { .. } => Self::ALL[2],
            Effect::Puppet { .. } => Self::ALL[3],
        }
    }

    /// L'effet change avec le temps (le calque ne peut pas rester en cache)
    pub fn moves(&self) -> bool {
        match self {
            Effect::Wave { speed, .. } | Effect::Turbulence { speed, .. } => *speed != 0.0,
            Effect::Offset { x, y } => *x != 0.0 || *y != 0.0,
            Effect::Puppet { pins } => pins.iter().any(|p| p.at.animated()),
        }
    }

    /// Applique l'effet à l'image du calque (t : temps depuis le début du calque)
    fn apply(&self, b: Bitmap, t: f32) -> Bitmap {
        if let Effect::Puppet { pins } = self {
            return puppet(b, pins, t);
        }
        let (w, h) = (b.img.width() as usize, b.img.height() as usize);
        if w == 0 || h == 0 {
            return b;
        }
        let k = b.k;
        let src = b.img.as_raw();
        let sample = |x: f32, y: f32, wrap: bool| -> [u8; 4] {
            let (mut xi, mut yi) = (x.round() as i64, y.round() as i64);
            if wrap {
                xi = xi.rem_euclid(w as i64);
                yi = yi.rem_euclid(h as i64);
            } else if xi < 0 || yi < 0 || xi >= w as i64 || yi >= h as i64 {
                return [0; 4];
            }
            let i = (yi as usize * w + xi as usize) * 4;
            [src[i], src[i + 1], src[i + 2], src[i + 3]]
        };
        let mut out = vec![0u8; w * h * 4];
        out.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                let (fx, fy) = (x as f32, y as f32);
                let c = match *self {
                    Effect::Wave { amp, length, speed, vertical } => {
                        let ph = std::f32::consts::TAU * (if vertical { fx } else { fy } / k / length.max(1.0) - speed * t);
                        let d = ph.sin() * amp * k;
                        if vertical { sample(fx, fy + d, false) } else { sample(fx + d, fy, false) }
                    }
                    Effect::Turbulence { amount, size, speed } => {
                        let (u, v) = (fx / k / size.max(1.0), fy / k / size.max(1.0));
                        let dx = (fbm(u, v, t * speed, 7, false) - 0.5) * 2.0 * amount * k;
                        let dy = (fbm(u + 31.7, v + 11.3, t * speed, 9, false) - 0.5) * 2.0 * amount * k;
                        sample(fx + dx, fy + dy, false)
                    }
                    Effect::Offset { x: vx, y: vy } => sample(fx - vx * t * k, fy - vy * t * k, true),
                    Effect::Puppet { .. } => sample(fx, fy, false),
                };
                row[x * 4..x * 4 + 4].copy_from_slice(&c);
            }
        });
        Bitmap { img: image::RgbaImage::from_raw(w as u32, h as u32, out).unwrap_or_default(), ..b }
    }
}

#[cfg(test)]
mod fx_tests {
    use super::*;

    #[test]
    fn bruit_et_effets() {
        // Le bruit : entre 0 et 1, varie dans l'espace et le temps, reproductible
        let a = fbm(1.3, 2.7, 0.0, 1, false);
        assert!((0.0..=1.0).contains(&a));
        assert_ne!(a, fbm(5.1, 0.4, 0.0, 1, false));
        assert_ne!(a, fbm(1.3, 2.7, 0.8, 1, false));
        assert_eq!(a, fbm(1.3, 2.7, 0.0, 1, false));
        // Décalage de 10 px en une seconde : le pixel (0, 0) prend ce qui était en (w - 10, 0)
        let mut img = image::RgbaImage::new(40, 4);
        img.put_pixel(30, 0, image::Rgba([255, 0, 0, 255]));
        let b = Bitmap { img, ox: 0.0, oy: 0.0, k: 1.0 };
        let o = Effect::Offset { x: 10.0, y: 0.0 }.apply(b, 1.0);
        assert_eq!(o.img.get_pixel(0, 0)[0], 255);
        // L'ondulation garde la taille et déplace des pixels
        let mut img = image::RgbaImage::new(40, 40);
        for x in 0..40 {
            img.put_pixel(x, 20, image::Rgba([255; 4]));
        }
        let o = Effect::Wave { amp: 5.0, length: 20.0, speed: 0.0, vertical: true }.apply(Bitmap { img: img.clone(), ox: 0.0, oy: 0.0, k: 1.0 }, 0.0);
        assert_eq!(o.img.dimensions(), (40, 40));
        assert_ne!(o.img.as_raw(), img.as_raw());
    }
}

#[cfg(test)]
mod cam_tests {
    use super::*;
    #[test]
    fn camera_2_5d() {
        let mut c = Compo { layers: vec![], ..Compo::default() };
        c.camera = Some(Camera::new(c.w));
        let mut near = Layer::new("près", Kind::Ellipse { w: 300.0, h: 300.0, color: [255, 120, 60] }, &c);
        near.depth = Some(Track::new(-800.0, 0.0));
        let mut far = Layer::new("loin", Kind::Rect { w: 300.0, h: 300.0, radius: 20.0, color: [80, 140, 255] }, &c);
        far.depth = Some(Track::new(3000.0, 0.0));
        // Le loin est au-dessus dans la pile : il se dessine quand même derrière
        c.layers = vec![far, near];
        let order = c.draw_order(0.0);
        assert_eq!(order, vec![0, 1]);
        let (xn, _) = c.projected(1, 0.0).unwrap();
        let (xf, _) = c.projected(0, 0.0).unwrap();
        assert!(xn.max_scale() > 1.0 && xf.max_scale() < 1.0);
        // Un panoramique déplace plus ce qui est près (parallaxe)
        c.camera.as_mut().unwrap().pan = Track::new(100.0, 0.0);
        let (xn2, _) = c.projected(1, 0.0).unwrap();
        let (xf2, _) = c.projected(0, 0.0).unwrap();
        assert!((xn.e - xn2.e).abs() > (xf.e - xf2.e).abs());
        // Derrière la caméra : rien
        c.layers[1].depth = Some(Track::new(-5000.0, 0.0));
        assert!(c.projected(1, 0.0).is_none());
    }
}
#[cfg(test)]
mod varfont_tests {
    use super::*;
    #[test]
    fn police_variable() {
        let f = "/usr/share/fonts/truetype/google-fonts/ofl/montserrat/Montserrat[wght].ttf";
        if !Path::new(f).exists() {
            return;
        }
        let axes = font_file_axes(f);
        assert!(axes.iter().any(|a| a.0 == "wght" && a.1 < 200.0 && a.3 > 800.0), "{axes:?}");
        let mut c = Compo { layers: vec![], w: 1200, h: 600, ..Compo::default() };
        let mut out = vec![];
        for (k, wg) in [100.0, 500.0, 900.0].into_iter().enumerate() {
            let mut l = Layer::new("t", Kind::Text { text: "Aryadne".into(), size: 140.0, color: [240, 236, 240], font: f.into(), tracking: 0.0, align: 1 }, &c);
            l.pos = Track::new(600.0, 110.0 + k as f32 * 190.0);
            l.font_axes.insert("wght".into(), Track::new(wg, 0.0));
            c.layers.push(l);
            out.push(size(&c.layers[k], &mut Cache::new(Path::new("/tmp")))[0]);
        }
        // Plus gras, plus large
        assert!(out[0] < out[1] && out[1] < out[2], "{out:?}");
        if std::env::var("ARY_DEMO").is_ok() {
            render(&c, 0.5, 0.5, &mut Cache::new(Path::new("/tmp"))).save("/tmp/varfont.png").unwrap();
        }
    }
}
#[cfg(test)]
mod puppet_tests {
    use super::*;
    #[test]
    fn marionnette() {
        // Une épingle : translation pure ; deux épingles tournées : rotation rigide
        assert_eq!(mls_rigid([5.0, 5.0], &[[0.0, 0.0]], &[[10.0, 0.0]]), [15.0, 5.0]);
        let r = mls_rigid([5.0, 0.0], &[[0.0, 0.0], [10.0, 0.0]], &[[0.0, 0.0], [0.0, 10.0]]);
        assert!((r[0]).abs() < 1e-3 && (r[1] - 5.0).abs() < 1e-3, "{r:?}");
        // Une barre plie quand on tire son bout vers le haut
        let mut c = Compo { layers: vec![], w: 1000, h: 600, ..Compo::default() };
        let mut l = Layer::new("bras", Kind::Rect { w: 600.0, h: 80.0, radius: 40.0, color: [230, 160, 90] }, &c);
        l.pos = Track::new(500.0, 400.0);
        let mut end = Track::new(280.0, 0.0);
        end.insert(Key { t: 0.0, v: [280.0, 0.0], ease: Ease::Smooth, tan: vec![] });
        end.insert(Key { t: 1.0, v: [180.0, -260.0], ease: Ease::Smooth, tan: vec![] });
        l.fx.push(Effect::Puppet { pins: vec![Pin { rest: [-280.0, 0.0], at: Track::new(-280.0, 0.0) }, Pin { rest: [0.0, 0.0], at: Track::new(0.0, 0.0) }, Pin { rest: [280.0, 0.0], at: end }] });
        c.layers.push(l);
        let mut cache = Cache::new(Path::new("/tmp"));
        let a = render(&c, 0.0, 0.5, &mut cache);
        let b = render(&c, 1.0, 0.5, &mut cache);
        // Au repos, rien au-dessus de la barre ; plié, le bout monte
        let lit = |im: &image::RgbaImage, x: u32, y: u32| im.get_pixel(x, y)[0] > 150;
        assert!(!lit(&a, 335, 75) && lit(&b, 335, 75));
        if std::env::var("ARY_DEMO").is_ok() {
            let mut out = image::RgbaImage::new(a.width() * 2 + 10, a.height());
            image::imageops::overlay(&mut out, &a, 0, 0);
            image::imageops::overlay(&mut out, &b, a.width() as i64 + 10, 0);
            out.save("/tmp/puppet.png").unwrap();
        }
    }
}

#[cfg(test)]
mod template_tests {
    use super::*;
    #[test]
    fn modele_et_variante() {
        let mut c = Compo::default();
        let i = c.layers.iter().position(|l| matches!(l.kind, Kind::Text { .. })).unwrap();
        c.toggle_param(i, false);
        c.toggle_param(i, true);
        assert_eq!(c.params.len(), 2);
        let (pt, pc) = (c.params[0].clone(), c.params[1].clone());
        assert!(c.set_param(&pt, ParamVal::Text("Bonjour".into())));
        assert!(c.set_param(&pc, ParamVal::Color([255, 0, 0])));
        assert_eq!(c.param(&pt), Some(ParamVal::Text("Bonjour".into())));
        assert_eq!(c.param(&pc), Some(ParamVal::Color([255, 0, 0])));
        assert!(!c.set_param(&pt, ParamVal::Color([0, 0, 0])));
        c.toggle_param(i, true);
        assert_eq!(c.params.len(), 1);
    }
}

#[cfg(test)]
mod ik_tests {
    use super::*;
    #[test]
    fn ik_deux_os() {
        // Bras de 100 + 100 vers une cible à (100, 100) : le bout l'atteint
        let (r1, r2) = two_bone_ik([0.0, 0.0], [100.0, 0.0], [100.0, 0.0], [100.0, 100.0], false);
        let (a, b) = (r1.to_radians(), (r1 + r2).to_radians());
        let tip = [100.0 * a.cos() + 100.0 * b.cos(), 100.0 * a.sin() + 100.0 * b.sin()];
        assert!((tip[0] - 100.0).abs() < 0.5 && (tip[1] - 100.0).abs() < 0.5, "{tip:?}");
        // Dans une composition : épaule, avant-bras, main, puis le contrôleur tire la main
        let mut c = Compo { layers: vec![], ..Compo::default() };
        let mut up = Layer::new("bras", Kind::Rect { w: 100.0, h: 20.0, radius: 10.0, color: [200; 3] }, &c);
        up.pos = Track::new(500.0, 500.0);
        let mut lo = Layer::new("avant-bras", Kind::Rect { w: 100.0, h: 20.0, radius: 10.0, color: [200; 3] }, &c);
        lo.pos = Track::new(100.0, 0.0);
        lo.parent = Some(up.id);
        let mut en = Layer::new("main", Kind::Ellipse { w: 30.0, h: 30.0, color: [250; 3] }, &c);
        en.pos = Track::new(100.0, 0.0);
        en.parent = Some(lo.id);
        c.layers = vec![en, lo, up];
        let ctl = c.make_ik(2, 0.0).unwrap();
        let hand = c.index_of(c.layers[3].ik.as_ref().unwrap().end).unwrap();
        // Au départ la main est sur le contrôleur, et elle le suit
        let w = c.world(hand, 0.0).apply([0.0, 0.0]);
        assert!((w[0] - 700.0).abs() < 1.0 && (w[1] - 500.0).abs() < 1.0, "{w:?}");
        c.layers[ctl].pos = Track::new(560.0, 620.0);
        let w = c.world(hand, 0.0).apply([0.0, 0.0]);
        assert!((w[0] - 560.0).abs() < 1.0 && (w[1] - 620.0).abs() < 1.0, "{w:?}");
    }
}
