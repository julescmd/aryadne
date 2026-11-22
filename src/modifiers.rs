//! Pile de modificateurs, comme dans 3ds Max et Blender : chaque modificateur transforme
//! le maillage du précédent, sans toucher au fichier d'origine. On peut les activer,
//! les réordonner, les régler, puis exporter le résultat en OBJ.

use crate::model3d::{fan_mask, Model};
use eframe::egui::{self, Color32, RichText};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

type V3 = [f32; 3];

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum Kind {
    Mirror { axis: u8, merge: bool, #[serde(default)] extra: [bool; 3], #[serde(default)] bisect: bool },
    Array {
        count: u32,
        axis: u8,
        offset: f32,
        /// Décalage constant (en plus du relatif), rotation et échelle par copie
        #[serde(default)]
        constant: [f32; 3],
        #[serde(default)]
        rotate: [f32; 3],
        #[serde(default = "one")]
        scale: f32,
    },
    Subdivision {
        levels: u8,
        #[serde(default)]
        optimal: bool,
        #[serde(default)]
        simple: bool,
    },
    Decimate { ratio: f32 },
    Smooth { iterations: u32, factor: f32, #[serde(default)] preserve_volume: bool, #[serde(default)] axes: [bool; 3] },
    Displace {
        strength: f32,
        scale: f32,
        seed: f32,
        #[serde(default = "five")]
        octaves: u32,
        #[serde(default = "half")]
        roughness: f32,
        /// 0 : le long des normales, 1-3 : axe X, Y, Z
        #[serde(default)]
        direction: u8,
        #[serde(default)]
        midlevel: f32,
    },
    Twist { angle: f32, axis: u8 },
    Bend { angle: f32, axis: u8 },
    Taper { factor: f32, axis: u8 },
    Spherify { factor: f32, #[serde(default)] shape: u8, #[serde(default = "one_u8")] axis: u8 },
    Solidify { thickness: f32, #[serde(default = "neg_one")] offset: f32, #[serde(default = "yes")] rim: bool },
    Wave { amplitude: f32, frequency: f32, axis: u8, #[serde(default)] phase: f32, #[serde(default)] radial: bool },
    Transform { scale: f32, rotate: [f32; 3], offset: [f32; 3], #[serde(default = "one3")] scale3: [f32; 3] },
    Weld { distance: f32 },
    Triangulate,
    EdgeSplit { angle: f32 },
    Wireframe { thickness: f32, keep: bool },
    Explode { distance: f32, random: f32, seed: f32 },
    Inflate { distance: f32 },
    Randomize { amount: f32, seed: f32, along_normal: bool },
    Voxelize { step: f32 },
    Stretch { factor: f32, axis: u8 },
    Shear { factor: f32, axis: u8, along: u8 },
    Warp { from: V3, to: V3, radius: f32 },
    Bulge { amount: f32, center: V3, radius: f32 },
    Ripple { amplitude: f32, frequency: f32, decay: f32, axis: u8, phase: f32 },
    Screw { angle: f32, steps: u32, height: f32, axis: u8 },
    Build { fraction: f32, seed: f32, random: bool },
    Slice { axis: u8, position: f32, flip: bool },
    FlipNormals,
    Lattice { amount: f32, axis: u8, curve: u8 },
    Melt { amount: f32, axis: u8 },
    Crumple { amount: f32, scale: f32, seed: f32 },
}

fn one() -> f32 {
    1.0
}
fn half() -> f32 {
    0.5
}
fn five() -> u32 {
    5
}
fn one_u8() -> u8 {
    1
}
fn neg_one() -> f32 {
    -1.0
}
fn yes() -> bool {
    true
}
fn one3() -> [f32; 3] {
    [1.0; 3]
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Modifier {
    pub kind: Kind,
    pub on: bool,
    #[serde(default)]
    pub open: bool,
    /// Mélange avec le maillage d'avant (0 = sans effet, 1 = plein effet)
    #[serde(default = "one")]
    pub influence: f32,
    /// Limiter l'effet à une zone le long d'un axe (-1 = bas de l'objet, 1 = haut)
    #[serde(default)]
    pub limit_on: bool,
    #[serde(default = "one_u8")]
    pub limit_axis: u8,
    #[serde(default = "full_range")]
    pub limit: [f32; 2],
    #[serde(default = "falloff")]
    pub falloff: f32,
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub common_open: bool,
}

fn full_range() -> [f32; 2] {
    [-1.0, 1.0]
}
fn falloff() -> f32 {
    0.15
}

impl Modifier {
    pub fn new(kind: Kind) -> Self {
        Modifier { kind, on: true, open: true, influence: 1.0, limit_on: false, limit_axis: 1, limit: [-1.0, 1.0], falloff: 0.15, invert: false, common_open: false }
    }
}

/// (catégorie, nom, aide)
/// Catégories du menu d'ajout, dans l'ordre.
pub const CATS: [&str; 7] = ["édition", "générer", "déformer", "organique", "topologie", "normales", "effets"];

/// Ce que le panneau demande au-delà de la pile elle-même.
#[derive(Clone, Copy, PartialEq)]
pub enum Request {
    /// Appliquer les modificateurs jusqu'à celui-ci (inclus) au maillage
    ApplyUpTo(usize),
    ApplyAll,
    /// Revenir au maillage du fichier
    Restore,
}

pub const ALL: &[(&str, &str, &str)] = &[
    ("générer", "Miroir", "symétrie sur un ou plusieurs axes, avec coupe"),
    ("générer", "Réseau (array)", "copies avec décalage, rotation et échelle par copie"),
    ("générer", "Subdivision", "lisse et densifie (Catmull-Clark, comme Blender)"),
    ("topologie", "Décimer", "réduit le nombre de triangles"),
    ("déformer", "Lisser", "adoucit les formes (avec conservation du volume)"),
    ("organique", "Déplacement (bruit)", "relief procédural fractal"),
    ("déformer", "Torsion (twist)", "tord autour d'un axe"),
    ("déformer", "Courbure (bend)", "plie le long d'un axe"),
    ("déformer", "Effilement (taper)", "rétrécit vers un bout"),
    ("déformer", "Projeter (cast)", "tend vers une sphère, un cylindre ou un cube"),
    ("générer", "Épaissir (solidify)", "donne une épaisseur, avec bords fermés"),
    ("organique", "Onde", "vagues linéaires ou circulaires"),
    ("édition", "Transformation", "échelle, rotation, décalage"),
    ("topologie", "Souder (weld)", "fusionne les sommets proches"),
    ("topologie", "Trianguler", "montre tous les triangles"),
    ("normales", "Arêtes vives (edge split)", "facettes nettes au-delà d'un angle"),
    ("générer", "Fil de fer (wireframe)", "transforme les arêtes en tubes"),
    ("effets", "Exploser", "écarte les faces le long de leurs normales"),
    ("déformer", "Gonfler", "pousse le long des normales"),
    ("organique", "Aléatoire", "secoue les sommets"),
    ("effets", "Voxeliser", "aligne les sommets sur une grille (effet pixel)"),
    ("déformer", "Étirer (stretch)", "allonge en gardant le volume"),
    ("déformer", "Cisaillement (shear)", "penche le long d'un axe"),
    ("déformer", "Déformer vers (warp)", "tire une zone d'un point à un autre"),
    ("déformer", "Bosse (bulge)", "gonfle autour d'un point"),
    ("organique", "Ondulation (ripple)", "ronds dans l'eau autour du centre"),
    ("générer", "Vis (screw)", "copies en spirale autour d'un axe"),
    ("effets", "Construire (build)", "ne garde qu'une partie des faces"),
    ("édition", "Couper (slice)", "supprime ce qui dépasse d'un plan"),
    ("normales", "Inverser les normales", "retourne toutes les faces"),
    ("déformer", "Gonflement axial (lattice)", "renfle ou pince le milieu"),
    ("organique", "Fondre (melt)", "affaisse vers le sol"),
    ("organique", "Froisser (crumple)", "plis aléatoires"),
];

impl Kind {
    pub fn new(i: usize) -> Kind {
        match i {
            0 => Kind::Mirror { axis: 0, merge: true, extra: [false; 3], bisect: false },
            1 => Kind::Array { count: 3, axis: 0, offset: 1.1, constant: [0.0; 3], rotate: [0.0; 3], scale: 1.0 },
            2 => Kind::Subdivision { levels: 2, optimal: true, simple: false },
            3 => Kind::Decimate { ratio: 0.5 },
            4 => Kind::Smooth { iterations: 3, factor: 0.5, preserve_volume: true, axes: [true; 3] },
            5 => Kind::Displace { strength: 0.05, scale: 4.0, seed: 0.0, octaves: 5, roughness: 0.5, direction: 0, midlevel: 0.5 },
            6 => Kind::Twist { angle: 90.0, axis: 1 },
            7 => Kind::Bend { angle: 90.0, axis: 0 },
            8 => Kind::Taper { factor: -0.5, axis: 1 },
            9 => Kind::Spherify { factor: 0.5, shape: 0, axis: 1 },
            10 => Kind::Solidify { thickness: 0.03, offset: -1.0, rim: true },
            11 => Kind::Wave { amplitude: 0.03, frequency: 4.0, axis: 0, phase: 0.0, radial: false },
            12 => Kind::Transform { scale: 1.0, rotate: [0.0; 3], offset: [0.0; 3], scale3: [1.0; 3] },
            13 => Kind::Weld { distance: 0.001 },
            14 => Kind::Triangulate,
            15 => Kind::EdgeSplit { angle: 30.0 },
            16 => Kind::Wireframe { thickness: 0.01, keep: false },
            17 => Kind::Explode { distance: 0.1, random: 0.5, seed: 0.0 },
            18 => Kind::Inflate { distance: 0.03 },
            19 => Kind::Randomize { amount: 0.01, seed: 0.0, along_normal: false },
            20 => Kind::Voxelize { step: 0.05 },
            21 => Kind::Stretch { factor: 0.5, axis: 1 },
            22 => Kind::Shear { factor: 0.3, axis: 0, along: 1 },
            23 => Kind::Warp { from: [0.0, 0.5, 0.0], to: [0.0, 0.8, 0.3], radius: 0.4 },
            24 => Kind::Bulge { amount: 0.2, center: [0.0, 0.5, 0.0], radius: 0.4 },
            25 => Kind::Ripple { amplitude: 0.02, frequency: 8.0, decay: 1.0, axis: 1, phase: 0.0 },
            26 => Kind::Screw { angle: 360.0, steps: 8, height: 0.5, axis: 1 },
            27 => Kind::Build { fraction: 0.5, seed: 0.0, random: true },
            28 => Kind::Slice { axis: 1, position: 0.0, flip: false },
            29 => Kind::FlipNormals,
            30 => Kind::Lattice { amount: 0.3, axis: 1, curve: 0 },
            31 => Kind::Melt { amount: 0.3, axis: 1 },
            _ => Kind::Crumple { amount: 0.03, scale: 6.0, seed: 0.0 },
        }
    }
    pub fn index(&self) -> usize {
        match self {
            Kind::Mirror { .. } => 0,
            Kind::Array { .. } => 1,
            Kind::Subdivision { .. } => 2,
            Kind::Decimate { .. } => 3,
            Kind::Smooth { .. } => 4,
            Kind::Displace { .. } => 5,
            Kind::Twist { .. } => 6,
            Kind::Bend { .. } => 7,
            Kind::Taper { .. } => 8,
            Kind::Spherify { .. } => 9,
            Kind::Solidify { .. } => 10,
            Kind::Wave { .. } => 11,
            Kind::Transform { .. } => 12,
            Kind::Weld { .. } => 13,
            Kind::Triangulate => 14,
            Kind::EdgeSplit { .. } => 15,
            Kind::Wireframe { .. } => 16,
            Kind::Explode { .. } => 17,
            Kind::Inflate { .. } => 18,
            Kind::Randomize { .. } => 19,
            Kind::Voxelize { .. } => 20,
            Kind::Stretch { .. } => 21,
            Kind::Shear { .. } => 22,
            Kind::Warp { .. } => 23,
            Kind::Bulge { .. } => 24,
            Kind::Ripple { .. } => 25,
            Kind::Screw { .. } => 26,
            Kind::Build { .. } => 27,
            Kind::Slice { .. } => 28,
            Kind::FlipNormals => 29,
            Kind::Lattice { .. } => 30,
            Kind::Melt { .. } => 31,
            Kind::Crumple { .. } => 32,
        }
    }
    pub fn name(&self) -> &'static str {
        ALL[self.index()].1
    }
    /// Garde le même nombre de sommets dans le même ordre (déformation pure) :
    /// l'influence et les limites peuvent alors s'appliquer.
    pub fn is_deform(&self) -> bool {
        matches!(
            self,
            Kind::Smooth { .. } | Kind::Displace { .. } | Kind::Twist { .. } | Kind::Bend { .. } | Kind::Taper { .. } | Kind::Spherify { .. } | Kind::Wave { .. } | Kind::Transform { .. } | Kind::Inflate { .. } | Kind::Randomize { .. } | Kind::Voxelize { .. } | Kind::Stretch { .. } | Kind::Shear { .. } | Kind::Warp { .. } | Kind::Bulge { .. } | Kind::Ripple { .. } | Kind::Lattice { .. } | Kind::Melt { .. } | Kind::Crumple { .. }
        )
    }
}

/// Maillage de travail (espace normalisé, Y en haut).
#[derive(Clone)]
pub struct Mesh {
    pub pos: Vec<V3>,
    pub tris: Vec<[u32; 3]>,
    pub colors: Vec<Color32>,
    pub edges: Vec<u8>,
}

impl Mesh {
    /// Enregistre le maillage tel quel (pour garder ce qui a été appliqué d'une session à l'autre).
    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        let mut b: Vec<u8> = Vec::with_capacity(16 + self.pos.len() * 12 + self.tris.len() * 13 + self.colors.len() * 4);
        b.extend(b"ARYM");
        for n in [self.pos.len(), self.tris.len(), self.colors.len(), self.edges.len()] {
            b.extend((n as u32).to_le_bytes());
        }
        self.pos.iter().flatten().for_each(|x| b.extend(x.to_le_bytes()));
        self.tris.iter().flatten().for_each(|x| b.extend(x.to_le_bytes()));
        self.colors.iter().for_each(|c| b.extend(c.to_array()));
        b.extend(&self.edges);
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, b).map_err(|e| e.to_string())
    }

    pub fn load(path: &std::path::Path) -> Option<Mesh> {
        let b = std::fs::read(path).ok()?;
        if b.len() < 20 || &b[..4] != b"ARYM" {
            return None;
        }
        let n = |k: usize| u32::from_le_bytes(b[4 + k * 4..8 + k * 4].try_into().unwrap()) as usize;
        let (np, nt, nc, ne) = (n(0), n(1), n(2), n(3));
        if b.len() != 20 + np * 12 + nt * 12 + nc * 4 + ne {
            return None;
        }
        let f = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let u = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let (o1, o2, o3) = (20 + np * 12, 20 + np * 12 + nt * 12, 20 + np * 12 + nt * 12 + nc * 4);
        let pos = (0..np).map(|i| [f(20 + i * 12), f(24 + i * 12), f(28 + i * 12)]).collect();
        let tris: Vec<[u32; 3]> = (0..nt).map(|i| [u(o1 + i * 12), u(o1 + i * 12 + 4), u(o1 + i * 12 + 8)]).collect();
        if tris.iter().flatten().any(|&i| i as usize >= np) {
            return None;
        }
        let colors = (0..nc).map(|i| Color32::from_rgba_premultiplied(b[o2 + i * 4], b[o2 + i * 4 + 1], b[o2 + i * 4 + 2], b[o2 + i * 4 + 3])).collect();
        Some(Mesh { pos, tris, colors, edges: b[o3..].to_vec() })
    }

    pub fn from_model(m: &Model, z_up: bool) -> Mesh {
        Mesh { pos: m.normalized(z_up), tris: m.tris.clone(), colors: m.colors.clone(), edges: m.edges.clone() }
    }
    fn bbox(&self) -> (V3, V3) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for p in &self.pos {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        (lo, hi)
    }
}

/// Applique la pile (de haut en bas). `origin` : origine de l'objet dans l'espace normalisé.
pub fn apply(stack: &[Modifier], mut m: Mesh, origin: V3) -> Mesh {
    for md in stack.iter().filter(|m| m.on) {
        if m.tris.is_empty() {
            break;
        }
        let partial = md.kind.is_deform() && (md.influence < 0.999 || md.limit_on || md.invert);
        if !partial {
            m = apply_one(md.kind, m, origin);
            continue;
        }
        // Déformation partielle : on mélange avant / après selon un poids par sommet
        let before = m.pos.clone();
        let (lo, hi) = m.bbox();
        let a = (md.limit_axis as usize).min(2);
        let span = (hi[a] - lo[a]).max(1e-6);
        let f = md.falloff.max(1e-4);
        let weight = |p: &V3| -> f32 {
            let mut w = 1.0;
            if md.limit_on {
                let t = (p[a] - lo[a]) / span * 2.0 - 1.0;
                let (l0, l1) = (md.limit[0].min(md.limit[1]), md.limit[0].max(md.limit[1]));
                w = smooth(l0 - f, l0, t) * (1.0 - smooth(l1, l1 + f, t));
            }
            if md.invert {
                w = 1.0 - w;
            }
            w * md.influence.clamp(0.0, 1.0)
        };
        m = apply_one(md.kind, m, origin);
        if m.pos.len() == before.len() {
            for (p, b) in m.pos.iter_mut().zip(&before) {
                let w = weight(b);
                for k in 0..3 {
                    p[k] = b[k] + (p[k] - b[k]) * w;
                }
            }
        }
    }
    m
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash(x: u32) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn len(a: V3) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> V3 {
    let l = len(a).max(1e-12);
    [a[0] / l, a[1] / l, a[2] / l]
}

fn rot(p: V3, axis: usize, a: f32) -> V3 {
    let (s, c) = a.sin_cos();
    let (i, j) = ((axis + 1) % 3, (axis + 2) % 3);
    let mut q = p;
    q[i] = p[i] * c - p[j] * s;
    q[j] = p[i] * s + p[j] * c;
    q
}

fn neighbors(n: usize, tris: &[[u32; 3]]) -> Vec<Vec<u32>> {
    let mut nb: Vec<Vec<u32>> = vec![vec![]; n];
    for t in tris {
        for k in 0..3 {
            let (a, b) = (t[k] as usize, t[(k + 1) % 3]);
            if !nb[a].contains(&b) {
                nb[a].push(b);
            }
            if !nb[b as usize].contains(&t[k]) {
                nb[b as usize].push(t[k]);
            }
        }
    }
    nb
}

fn normals(pos: &[V3], tris: &[[u32; 3]]) -> Vec<V3> {
    let mut n = vec![[0.0f32; 3]; pos.len()];
    for t in tris {
        let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let f = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        for &i in t {
            for k in 0..3 {
                n[i as usize][k] += f[k];
            }
        }
    }
    n.iter()
        .map(|v| {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-12);
            [v[0] / l, v[1] / l, v[2] / l]
        })
        .collect()
}

/// Inverse le sens d'un triangle (et réordonne ses arêtes visibles).
fn flip(t: [u32; 3], e: u8) -> ([u32; 3], u8) {
    ([t[0], t[2], t[1]], ((e >> 2) & 1) | (e & 2) | ((e & 1) << 2))
}

fn apply_one(k: Kind, mut m: Mesh, origin: V3) -> Mesh {
    let (lo, hi) = m.bbox();
    let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let ax = |a: u8| (a as usize).min(2);
    match k {
        Kind::Mirror { axis, merge, extra, bisect } => {
            let mut axes = vec![ax(axis)];
            for (k, &on) in extra.iter().enumerate() {
                if on && !axes.contains(&k) {
                    axes.push(k);
                }
            }
            for a in axes {
                if bisect {
                    // On garde le côté positif avant de refléter (pas de doublons au milieu)
                    m = apply_one(Kind::Slice { axis: a as u8, position: (origin[a] - lo[a]) / size[a].max(1e-6) * 2.0 - 1.0, flip: true }, m, origin);
                }
                m = mirror(m, a, merge, origin);
            }
        }
        Kind::Triangulate => {
            m.edges = vec![7; m.tris.len()];
        }
        _ => m = apply_more(k, m, origin, lo, hi),
    }
    m
}

fn mirror(mut m: Mesh, a: usize, merge: bool, origin: V3) -> Mesh {
    let (lo, hi) = m.bbox();
    let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    {
        {
            let n = m.pos.len();
            let tol = size[0].max(size[1]).max(size[2]) * 1e-4;
            // Les sommets sur le plan de symétrie sont partagés (soudure)
            let mut map = Vec::with_capacity(n);
            for i in 0..n {
                let p = m.pos[i];
                if merge && (p[a] - origin[a]).abs() <= tol {
                    map.push(i as u32);
                } else {
                    let mut q = p;
                    q[a] = 2.0 * origin[a] - p[a];
                    m.pos.push(q);
                    map.push((m.pos.len() - 1) as u32);
                }
            }
            let nt = m.tris.len();
            for i in 0..nt {
                let t = m.tris[i];
                let (ft, fe) = flip([map[t[0] as usize], map[t[1] as usize], map[t[2] as usize]], m.edges[i]);
                m.tris.push(ft);
                m.edges.push(fe);
                m.colors.push(m.colors[i]);
            }
        }
    }
    m
}

fn apply_more(k: Kind, mut m: Mesh, origin: V3, lo: V3, hi: V3) -> Mesh {
    let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let ax = |a: u8| (a as usize).min(2);
    let center = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let diag = len(size).max(1e-6);
    match k {
        Kind::Array { count, axis, offset, constant, rotate, scale } => {
            let a = ax(axis);
            let n = m.pos.len() as u32;
            let nt = m.tris.len();
            let step = size[a] * offset;
            let count = count.clamp(1, 256);
            if (nt as u64) * count as u64 > crate::model3d::MAX_SUBDIV_TRIS as u64 {
                return m;
            }
            for c in 1..count {
                let cf = c as f32;
                let sc = scale.powf(cf);
                for i in 0..n as usize {
                    // Chaque copie : échelle et rotation autour de l'origine, puis décalage
                    let mut p = [(m.pos[i][0] - origin[0]) * sc, (m.pos[i][1] - origin[1]) * sc, (m.pos[i][2] - origin[2]) * sc];
                    for r in 0..3 {
                        if rotate[r] != 0.0 {
                            p = rot(p, r, (rotate[r] * cf).to_radians());
                        }
                    }
                    let mut p = [p[0] + origin[0] + constant[0] * cf, p[1] + origin[1] + constant[1] * cf, p[2] + origin[2] + constant[2] * cf];
                    p[a] += step * cf;
                    m.pos.push(p);
                }
                for i in 0..nt {
                    let t = m.tris[i];
                    m.tris.push([t[0] + n * c, t[1] + n * c, t[2] + n * c]);
                    m.edges.push(m.edges[i]);
                    m.colors.push(m.colors[i]);
                }
            }
        }
        Kind::Subdivision { levels, optimal, simple } => {
            if simple {
                // « Simple » : densifie sans lisser (comme Blender)
                let (p, t, c, e) = crate::model3d::split_flat(&m.pos, &m.tris, &m.colors, &m.edges, levels.min(5), optimal);
                m = Mesh { pos: p, tris: t, colors: c, edges: e };
            } else {
                let (p, t, c, e) = crate::model3d::catmull_clark(&m.pos, &m.tris, &m.colors, &m.edges, levels.min(5), optimal);
                m = Mesh { pos: p, tris: t, colors: c, edges: e };
            }
        }
        Kind::Decimate { ratio } => {
            let target = ((m.tris.len() as f32) * ratio.clamp(0.01, 1.0)) as usize;
            let (p, t, c) = crate::model3d::simplify_arrays(&m.pos, &m.tris, &m.colors, target.max(4));
            m.edges = vec![7; t.len()];
            m.pos = p;
            m.tris = t;
            m.colors = c;
        }
        Kind::Smooth { iterations, factor, preserve_volume, axes } => {
            let nb = neighbors(m.pos.len(), &m.tris);
            let axes = if axes == [false; 3] { [true; 3] } else { axes };
            for it in 0..(iterations.min(100) * if preserve_volume { 2 } else { 1 }) {
                // Taubin : un pas qui lisse, un pas qui regonfle (le volume ne fond pas)
                let factor = if preserve_volume && it % 2 == 1 { -factor * 1.05 } else { factor };
                let old = m.pos.clone();
                for (i, ns) in nb.iter().enumerate() {
                    if ns.is_empty() {
                        continue;
                    }
                    let mut avg = [0.0; 3];
                    for &j in ns {
                        for k in 0..3 {
                            avg[k] += old[j as usize][k];
                        }
                    }
                    for k in 0..3 {
                        avg[k] /= ns.len() as f32;
                        if axes[k] {
                            m.pos[i][k] = old[i][k] + (avg[k] - old[i][k]) * factor;
                        }
                    }
                }
            }
        }
        Kind::Displace { strength, scale, seed, octaves, roughness, direction, midlevel } => {
            let n = normals(&m.pos, &m.tris);
            for (p, nn) in m.pos.iter_mut().zip(n) {
                let d = (crate::nodes::fbm(p[0] * scale + seed, p[1] * scale, p[2] * scale - seed, octaves.clamp(1, 10) as usize, roughness.clamp(0.05, 0.95)) - midlevel) * 2.0 * strength;
                let dir = match direction {
                    1 => [1.0, 0.0, 0.0],
                    2 => [0.0, 1.0, 0.0],
                    3 => [0.0, 0.0, 1.0],
                    _ => nn,
                };
                for k in 0..3 {
                    p[k] += dir[k] * d;
                }
            }
        }
        Kind::Twist { angle, axis } => {
            let a = ax(axis);
            let len = size[a].max(1e-6);
            for p in &mut m.pos {
                let t = (p[a] - lo[a]) / len - 0.5;
                *p = rot(*p, a, angle.to_radians() * t);
            }
        }
        Kind::Bend { angle, axis } => {
            let a = ax(axis);
            let u = (a + 1) % 3;
            let len = size[a].max(1e-6);
            let ang = angle.to_radians();
            if ang.abs() > 1e-4 {
                let r = len / ang;
                let mid = (lo[a] + hi[a]) / 2.0;
                for p in &mut m.pos {
                    let th = (p[a] - mid) / len * ang;
                    let d = r - (p[u] - lo[u]);
                    p[a] = mid + d * th.sin();
                    p[u] = lo[u] + r - d * th.cos();
                }
            }
        }
        Kind::Taper { factor, axis } => {
            let a = ax(axis);
            let len = size[a].max(1e-6);
            let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
            for p in &mut m.pos {
                let t = (p[a] - lo[a]) / len;
                let s = (1.0 + factor * t).max(0.0);
                for k in 0..3 {
                    if k != a {
                        p[k] = c[k] + (p[k] - c[k]) * s;
                    }
                }
            }
        }
        Kind::Spherify { factor, shape, axis } => {
            let c = center;
            let a = ax(axis);
            // Composantes prises en compte : toutes (sphère), sans l'axe (cylindre)
            let mask = |k: usize| shape != 1 || k != a;
            let dist = |p: &V3| -> f32 { (0..3).filter(|&k| mask(k)).map(|k| (p[k] - c[k]).powi(2)).sum::<f32>().sqrt() };
            let r = m.pos.iter().map(dist).sum::<f32>() / m.pos.len().max(1) as f32;
            let f = factor.clamp(-2.0, 2.0);
            for p in &mut m.pos {
                let d = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
                let target: V3 = if shape == 2 {
                    // Cube : on pousse vers la plus grande composante
                    let mx = d.iter().map(|x| x.abs()).fold(0.0f32, f32::max).max(1e-6);
                    let s = r / mx;
                    [c[0] + d[0] * s, c[1] + d[1] * s, c[2] + d[2] * s]
                } else {
                    let l = dist(p).max(1e-6);
                    let mut t = *p;
                    for k in 0..3 {
                        if mask(k) {
                            t[k] = c[k] + d[k] / l * r;
                        }
                    }
                    t
                };
                for k in 0..3 {
                    p[k] += (target[k] - p[k]) * f;
                }
            }
        }
        Kind::Solidify { thickness, offset, rim } => {
            let n = m.pos.len() as u32;
            let nrm = normals(&m.pos, &m.tris);
            // offset -1 : vers l'intérieur, 0 : centré, 1 : vers l'extérieur
            let o = (offset.clamp(-1.0, 1.0) + 1.0) / 2.0;
            let (out_d, in_d) = (thickness * o, thickness * (1.0 - o));
            let inner: Vec<V3> = m.pos.iter().zip(&nrm).map(|(p, q)| [p[0] - q[0] * in_d, p[1] - q[1] * in_d, p[2] - q[2] * in_d]).collect();
            for (p, q) in m.pos.iter_mut().zip(&nrm) {
                for k in 0..3 {
                    p[k] += q[k] * out_d;
                }
            }
            m.pos.extend(inner);
            let nt = m.tris.len();
            // Arêtes de bord (un seul triangle) : on y ferme l'épaisseur
            let mut count: HashMap<(u32, u32), i32> = HashMap::new();
            for t in &m.tris {
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    *count.entry((a.min(b), a.max(b))).or_default() += 1;
                }
            }
            for i in 0..nt {
                let t = m.tris[i];
                let (ft, fe) = flip([t[0] + n, t[1] + n, t[2] + n], m.edges[i]);
                m.tris.push(ft);
                m.edges.push(fe);
                m.colors.push(m.colors[i]);
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    if rim && count.get(&(a.min(b), a.max(b))) == Some(&1) {
                        m.tris.push([b, a, a + n]);
                        m.tris.push([b, a + n, b + n]);
                        m.edges.push(fan_mask(1, 4));
                        m.edges.push(fan_mask(2, 4));
                        m.colors.push(m.colors[i]);
                        m.colors.push(m.colors[i]);
                    }
                }
            }
        }
        Kind::Wave { amplitude, frequency, axis, phase, radial } => {
            let a = ax(axis);
            let u = (a + 1) % 3;
            let l = size[a].max(1e-6);
            for p in &mut m.pos {
                let t = if radial {
                    let (i, j) = ((u + 1) % 3, a);
                    ((p[i] - center[i]).powi(2) + (p[j] - center[j]).powi(2)).sqrt() / l
                } else {
                    (p[a] - lo[a]) / l
                };
                p[u] += amplitude * ((t * frequency + phase) * std::f32::consts::TAU).sin();
            }
        }
        Kind::Transform { scale, rotate, offset, scale3 } => {
            for p in &mut m.pos {
                let mut q = [(p[0] - origin[0]) * scale * scale3[0], (p[1] - origin[1]) * scale * scale3[1], (p[2] - origin[2]) * scale * scale3[2]];
                for a in 0..3 {
                    if rotate[a] != 0.0 {
                        q = rot(q, a, rotate[a].to_radians());
                    }
                }
                *p = [q[0] + origin[0] + offset[0], q[1] + origin[1] + offset[1], q[2] + origin[2] + offset[2]];
            }
        }
        Kind::Weld { distance } => m = weld(m, distance.max(1e-7)),
        Kind::EdgeSplit { angle } => {
            // Chaque triangle reçoit ses propres sommets là où l'angle entre faces dépasse le seuil
            let fn_: Vec<V3> = m.tris.iter().map(|t| norm(cross(sub(m.pos[t[1] as usize], m.pos[t[0] as usize]), sub(m.pos[t[2] as usize], m.pos[t[0] as usize])))).collect();
            let cos_lim = angle.clamp(0.0, 180.0).to_radians().cos();
            let mut around: Vec<Vec<usize>> = vec![vec![]; m.pos.len()];
            for (ti, t) in m.tris.iter().enumerate() {
                for &v in t {
                    around[v as usize].push(ti);
                }
            }
            let mut pos = vec![];
            let mut tris = m.tris.clone();
            for (v, ts) in around.iter().enumerate() {
                // Regroupe les faces autour du sommet par normales proches
                let mut groups: Vec<(V3, u32)> = vec![];
                for &ti in ts {
                    let n = fn_[ti];
                    let gi = groups.iter().position(|(g, _)| g[0] * n[0] + g[1] * n[1] + g[2] * n[2] >= cos_lim);
                    let idx = match gi {
                        Some(g) => groups[g].1,
                        None => {
                            pos.push(m.pos[v]);
                            let id = (pos.len() - 1) as u32;
                            groups.push((n, id));
                            id
                        }
                    };
                    for k in 0..3 {
                        if m.tris[ti][k] == v as u32 {
                            tris[ti][k] = idx;
                        }
                    }
                }
            }
            m.pos = pos;
            m.tris = tris;
        }
        Kind::Wireframe { thickness, keep } => {
            let mut out = if keep { m.clone() } else { Mesh { pos: vec![], tris: vec![], colors: vec![], edges: vec![] } };
            let mut seen = std::collections::HashSet::new();
            let col = m.colors.first().copied().unwrap_or(Color32::WHITE);
            let r = thickness.max(1e-5) * 0.5;
            for (ti, t) in m.tris.iter().enumerate() {
                for k in 0..3 {
                    if m.edges.get(ti).is_some_and(|e| e & (1 << k) == 0) {
                        continue;
                    }
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    if !seen.insert((a.min(b), a.max(b))) {
                        continue;
                    }
                    // Prisme carré le long de l'arête
                    let (pa, pb) = (m.pos[a as usize], m.pos[b as usize]);
                    let d = norm(sub(pb, pa));
                    let up = if d[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
                    let u = norm(cross(d, up));
                    let v = cross(d, u);
                    let base = out.pos.len() as u32;
                    for p in [pa, pb] {
                        for (su, sv) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
                            out.pos.push([p[0] + (u[0] * su + v[0] * sv) * r, p[1] + (u[1] * su + v[1] * sv) * r, p[2] + (u[2] * su + v[2] * sv) * r]);
                        }
                    }
                    for s in 0..4u32 {
                        let (i0, i1) = (base + s, base + (s + 1) % 4);
                        out.tris.push([i0, i1, i1 + 4]);
                        out.tris.push([i0, i1 + 4, i0 + 4]);
                        out.edges.push(fan_mask(1, 4));
                        out.edges.push(fan_mask(2, 4));
                        out.colors.push(col);
                        out.colors.push(col);
                    }
                }
            }
            m = out;
        }
        Kind::Explode { distance, random, seed } => {
            let mut out = Mesh { pos: vec![], tris: vec![], colors: m.colors.clone(), edges: m.edges.clone() };
            for (ti, t) in m.tris.iter().enumerate() {
                let (a, b, c) = (m.pos[t[0] as usize], m.pos[t[1] as usize], m.pos[t[2] as usize]);
                let n = norm(cross(sub(b, a), sub(c, a)));
                let d = distance * diag * 0.5 * (1.0 - random.clamp(0.0, 1.0) * hash(ti as u32 ^ (seed * 1000.0) as u32));
                let base = out.pos.len() as u32;
                for p in [a, b, c] {
                    out.pos.push([p[0] + n[0] * d, p[1] + n[1] * d, p[2] + n[2] * d]);
                }
                out.tris.push([base, base + 1, base + 2]);
            }
            m = out;
        }
        Kind::Inflate { distance } => {
            let n = normals(&m.pos, &m.tris);
            for (p, q) in m.pos.iter_mut().zip(n) {
                for k in 0..3 {
                    p[k] += q[k] * distance;
                }
            }
        }
        Kind::Randomize { amount, seed, along_normal } => {
            let n = normals(&m.pos, &m.tris);
            let s = (seed * 997.0) as u32;
            for (i, (p, q)) in m.pos.iter_mut().zip(n).enumerate() {
                let r = |k: u32| hash((i as u32).wrapping_mul(3).wrapping_add(k) ^ s) * 2.0 - 1.0;
                if along_normal {
                    let d = r(0) * amount;
                    for k in 0..3 {
                        p[k] += q[k] * d;
                    }
                } else {
                    for k in 0..3 {
                        p[k] += r(k as u32) * amount;
                    }
                }
            }
        }
        Kind::Voxelize { step } => {
            let st = step.max(1e-4);
            for p in &mut m.pos {
                for k in 0..3 {
                    p[k] = (p[k] / st).round() * st;
                }
            }
        }
        Kind::Stretch { factor, axis } => {
            let a = ax(axis);
            let s = (1.0 + factor).max(0.05);
            let side = 1.0 / s.sqrt();
            for p in &mut m.pos {
                for k in 0..3 {
                    p[k] = center[k] + (p[k] - center[k]) * if k == a { s } else { side };
                }
            }
        }
        Kind::Shear { factor, axis, along } => {
            let (a, b) = (ax(axis), ax(along));
            if a != b {
                for p in &mut m.pos {
                    p[a] += (p[b] - center[b]) * factor;
                }
            }
        }
        Kind::Warp { from, to, radius } => {
            let r = radius.max(1e-4);
            let d = sub(to, from);
            for p in &mut m.pos {
                let w = 1.0 - smooth(0.0, r, len(sub(*p, from)));
                for k in 0..3 {
                    p[k] += d[k] * w;
                }
            }
        }
        Kind::Bulge { amount, center: c, radius } => {
            let r = radius.max(1e-4);
            for p in &mut m.pos {
                let v = sub(*p, c);
                let l = len(v);
                let w = 1.0 - smooth(0.0, r, l);
                if l > 1e-6 {
                    for k in 0..3 {
                        p[k] += v[k] / l * amount * w * r;
                    }
                }
            }
        }
        Kind::Ripple { amplitude, frequency, decay, axis, phase } => {
            let a = ax(axis);
            let (i, j) = ((a + 1) % 3, (a + 2) % 3);
            let l = size[i].max(size[j]).max(1e-6);
            for p in &mut m.pos {
                let d = ((p[i] - center[i]).powi(2) + (p[j] - center[j]).powi(2)).sqrt() / l;
                p[a] += amplitude * ((d * frequency + phase) * std::f32::consts::TAU).sin() * (-d * decay * 4.0).exp();
            }
        }
        Kind::Screw { angle, steps, height, axis } => {
            let a = ax(axis);
            let n = m.pos.len() as u32;
            let nt = m.tris.len();
            let steps = steps.clamp(1, 256);
            if (nt as u64) * (steps as u64 + 1) > crate::model3d::MAX_SUBDIV_TRIS as u64 {
                return m;
            }
            for c in 1..=steps {
                let t = c as f32 / steps as f32;
                for i in 0..n as usize {
                    let q = sub(m.pos[i], origin);
                    let mut q = rot(q, a, (angle * t).to_radians());
                    q[a] += height * size[a].max(0.1) * t * 2.0;
                    m.pos.push([q[0] + origin[0], q[1] + origin[1], q[2] + origin[2]]);
                }
                for i in 0..nt {
                    let tr = m.tris[i];
                    m.tris.push([tr[0] + n * c, tr[1] + n * c, tr[2] + n * c]);
                    m.edges.push(m.edges[i]);
                    m.colors.push(m.colors[i]);
                }
            }
        }
        Kind::Build { fraction, seed, random } => {
            let nt = m.tris.len();
            let keep = ((nt as f32) * fraction.clamp(0.0, 1.0)) as usize;
            let mut order: Vec<usize> = (0..nt).collect();
            if random {
                let s = (seed * 991.0) as u32;
                order.sort_by(|&x, &y| hash(x as u32 ^ s).partial_cmp(&hash(y as u32 ^ s)).unwrap_or(std::cmp::Ordering::Equal));
            }
            let mut set = vec![false; nt];
            for &i in order.iter().take(keep) {
                set[i] = true;
            }
            let mut k = 0;
            m.tris.retain(|_| {
                k += 1;
                set[k - 1]
            });
            let mut k = 0;
            m.edges.retain(|_| {
                k += 1;
                set[k - 1]
            });
            let mut k = 0;
            m.colors.retain(|_| {
                k += 1;
                set[k - 1]
            });
        }
        Kind::Slice { axis, position, flip: fl } => {
            let a = ax(axis);
            let cut = lo[a] + (position.clamp(-1.0, 1.0) + 1.0) / 2.0 * size[a];
            let keep = |p: &V3| if fl { p[a] >= cut - 1e-6 } else { p[a] <= cut + 1e-6 };
            // On garde les triangles entièrement du bon côté, et on ramène sur le plan
            // ceux qui le traversent (bord net, sans trous)
            let mut tris = vec![];
            let mut edges = vec![];
            let mut colors = vec![];
            for (i, t) in m.tris.iter().enumerate() {
                let inside = t.iter().filter(|&&v| keep(&m.pos[v as usize])).count();
                if inside > 0 {
                    tris.push(*t);
                    edges.push(m.edges[i]);
                    colors.push(m.colors[i]);
                }
            }
            for p in &mut m.pos {
                if !keep(p) {
                    p[a] = cut;
                }
            }
            m.tris = tris;
            m.edges = edges;
            m.colors = colors;
        }
        Kind::FlipNormals => {
            for i in 0..m.tris.len() {
                let (t, e) = flip(m.tris[i], m.edges[i]);
                m.tris[i] = t;
                m.edges[i] = e;
            }
        }
        Kind::Lattice { amount, axis, curve } => {
            let a = ax(axis);
            let l = size[a].max(1e-6);
            for p in &mut m.pos {
                let t = (p[a] - lo[a]) / l;
                let w = match curve {
                    1 => (t * std::f32::consts::TAU).sin(),
                    2 => 1.0 - (2.0 * t - 1.0).abs(),
                    _ => (t * std::f32::consts::PI).sin(),
                };
                let s = 1.0 + amount * w;
                for k in 0..3 {
                    if k != a {
                        p[k] = center[k] + (p[k] - center[k]) * s;
                    }
                }
            }
        }
        Kind::Melt { amount, axis } => {
            let a = ax(axis);
            let l = size[a].max(1e-6);
            for p in &mut m.pos {
                let t = 1.0 - (p[a] - lo[a]) / l;
                let s = 1.0 + amount * t * t * 2.0;
                p[a] = lo[a] + (p[a] - lo[a]) * (1.0 - amount.clamp(0.0, 1.0) * 0.5);
                for k in 0..3 {
                    if k != a {
                        p[k] = center[k] + (p[k] - center[k]) * s;
                    }
                }
            }
        }
        Kind::Crumple { amount, scale, seed } => {
            for p in &mut m.pos {
                let q = *p;
                for k in 0..3 {
                    let o = k as f32 * 17.3 + seed;
                    let v = crate::nodes::fbm(q[0] * scale + o, q[1] * scale - o, q[2] * scale + o * 0.5, 3, 0.6);
                    p[k] += (v - 0.5) * 2.0 * amount;
                }
            }
        }
        _ => {}
    }
    m
}

/// Fusionne les sommets plus proches que `dist` (grille de hachage).
fn weld(mut m: Mesh, dist: f32) -> Mesh {
    let cell = |p: &V3| [(p[0] / dist).round() as i64, (p[1] / dist).round() as i64, (p[2] / dist).round() as i64];
    let mut map: HashMap<[i64; 3], u32> = HashMap::new();
    let mut pos = vec![];
    let remap: Vec<u32> = m
        .pos
        .iter()
        .map(|p| {
            *map.entry(cell(p)).or_insert_with(|| {
                pos.push(*p);
                (pos.len() - 1) as u32
            })
        })
        .collect();
    let mut tris = vec![];
    let mut edges = vec![];
    let mut colors = vec![];
    for (i, t) in m.tris.iter().enumerate() {
        let t2 = [remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize]];
        if t2[0] != t2[1] && t2[1] != t2[2] && t2[0] != t2[2] {
            tris.push(t2);
            edges.push(m.edges[i]);
            colors.push(m.colors[i]);
        }
    }
    m.pos = pos;
    m.tris = tris;
    m.edges = edges;
    m.colors = colors;
    m
}

pub const EXPORT_FORMATS: &[&str] = &["obj", "fbx", "glb", "stl", "ply"];

/// Positions dans les unités et l'orientation du fichier d'origine.
fn world(m: &Mesh, model: &Model, z_up: bool) -> Vec<V3> {
    let s = model.size();
    let longest = s[0].max(s[1]).max(s[2]).max(1e-9);
    let c = [(model.min[0] + model.max[0]) / 2.0, (model.min[1] + model.max[1]) / 2.0, (model.min[2] + model.max[2]) / 2.0];
    m.pos
        .iter()
        .map(|p| {
            let o = if z_up { [p[0], -p[2], p[1]] } else { *p };
            [o[0] * longest + c[0], o[1] * longest + c[1], o[2] * longest + c[2]]
        })
        .collect()
}

/// Exporte selon l'extension : obj, glb, stl, ply.
pub fn export(m: &Mesh, model: &Model, z_up: bool, path: &std::path::Path) -> Result<(), String> {
    use std::io::Write;
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if ext == "fbx" {
        let e = crate::uvedit::Edit::open(m, std::path::PathBuf::new(), 1.0);
        let s = model.size();
        let longest = s[0].max(s[1]).max(s[2]).max(1e-9);
        let c = [(model.min[0] + model.max[0]) / 2.0, (model.min[1] + model.max[1]) / 2.0, (model.min[2] + model.max[2]) / 2.0];
        let w = move |p: V3| -> V3 {
            let o = if z_up { [p[0], -p[2], p[1]] } else { p };
            [o[0] * longest + c[0], o[1] * longest + c[1], o[2] * longest + c[2]]
        };
        return crate::fbxwrite::export(&e, &w, path);
    }
    let pos = world(m, model, z_up);
    let io = |e: std::io::Error| e.to_string();
    let f = std::fs::File::create(path).map_err(io)?;
    let mut w = std::io::BufWriter::new(f);
    match ext.as_str() {
        "obj" | "ply" => {
            // Quads et n-gones retrouvés (les diagonales de triangulation disparaissent)
            let (wp, polys, _) = crate::model3d::rebuild_polygons(&pos, &m.tris, &m.edges);
            if ext == "obj" {
                writeln!(w, "# exporté par Aryadne").map_err(io)?;
                for p in &wp {
                    writeln!(w, "v {} {} {}", p[0], p[1], p[2]).map_err(io)?;
                }
                for f in &polys {
                    let idx: Vec<String> = f.iter().map(|i| (i + 1).to_string()).collect();
                    writeln!(w, "f {}", idx.join(" ")).map_err(io)?;
                }
            } else {
                writeln!(w, "ply\nformat ascii 1.0\ncomment exporté par Aryadne\nelement vertex {}\nproperty float x\nproperty float y\nproperty float z\nelement face {}\nproperty list uchar int vertex_indices\nend_header", wp.len(), polys.len()).map_err(io)?;
                for p in &wp {
                    writeln!(w, "{} {} {}", p[0], p[1], p[2]).map_err(io)?;
                }
                for f in &polys {
                    let idx: Vec<String> = f.iter().map(|i| i.to_string()).collect();
                    writeln!(w, "{} {}", f.len(), idx.join(" ")).map_err(io)?;
                }
            }
        }
        "stl" => {
            let mut head = [0u8; 80];
            head[..20].copy_from_slice(b"exporte par Aryadne ");
            w.write_all(&head).map_err(io)?;
            w.write_all(&(m.tris.len() as u32).to_le_bytes()).map_err(io)?;
            for t in &m.tris {
                let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
                let n = norm(cross(sub(b, a), sub(c, a)));
                for v in [n, a, b, c] {
                    for k in 0..3 {
                        w.write_all(&v[k].to_le_bytes()).map_err(io)?;
                    }
                }
                w.write_all(&[0, 0]).map_err(io)?;
            }
        }
        "glb" => {
            // glTF 2.0 binaire : positions, normales lisses, indices
            let nrm = normals(&pos, &m.tris);
            let mut bin: Vec<u8> = vec![];
            for p in &pos {
                for k in 0..3 {
                    bin.extend(p[k].to_le_bytes());
                }
            }
            let nrm_off = bin.len();
            for n in &nrm {
                for k in 0..3 {
                    bin.extend(n[k].to_le_bytes());
                }
            }
            let idx_off = bin.len();
            for t in &m.tris {
                for &i in t {
                    bin.extend(i.to_le_bytes());
                }
            }
            while bin.len() % 4 != 0 {
                bin.push(0);
            }
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for p in &pos {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            let nv = pos.len();
            let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let json = serde_json::json!({
                "asset": { "version": "2.0", "generator": "Aryadne" },
                "scene": 0,
                "scenes": [ { "nodes": [0] } ],
                "nodes": [ { "mesh": 0, "name": name } ],
                "meshes": [ { "name": name, "primitives": [ { "attributes": { "POSITION": 0, "NORMAL": 1 }, "indices": 2, "mode": 4 } ] } ],
                "buffers": [ { "byteLength": bin.len() } ],
                "bufferViews": [
                    { "buffer": 0, "byteOffset": 0, "byteLength": nrm_off, "target": 34962 },
                    { "buffer": 0, "byteOffset": nrm_off, "byteLength": idx_off - nrm_off, "target": 34962 },
                    { "buffer": 0, "byteOffset": idx_off, "byteLength": m.tris.len() * 12, "target": 34963 }
                ],
                "accessors": [
                    { "bufferView": 0, "componentType": 5126, "count": nv, "type": "VEC3", "min": lo, "max": hi },
                    { "bufferView": 1, "componentType": 5126, "count": nv, "type": "VEC3" },
                    { "bufferView": 2, "componentType": 5125, "count": m.tris.len() * 3, "type": "SCALAR" }
                ]
            });
            let mut js = serde_json::to_vec(&json).map_err(|e| e.to_string())?;
            while js.len() % 4 != 0 {
                js.push(b' ');
            }
            let total = 12 + 8 + js.len() + 8 + bin.len();
            w.write_all(b"glTF").map_err(io)?;
            w.write_all(&2u32.to_le_bytes()).map_err(io)?;
            w.write_all(&(total as u32).to_le_bytes()).map_err(io)?;
            w.write_all(&(js.len() as u32).to_le_bytes()).map_err(io)?;
            w.write_all(b"JSON").map_err(io)?;
            w.write_all(&js).map_err(io)?;
            w.write_all(&(bin.len() as u32).to_le_bytes()).map_err(io)?;
            w.write_all(b"BIN\0").map_err(io)?;
            w.write_all(&bin).map_err(io)?;
        }
        other => return Err(format!("format « {other} » non pris en charge")),
    }
    w.flush().map_err(io)
}

// ---------- panneau ----------

/// Panneau de la pile. Renvoie vrai si la pile a changé.
pub fn panel(ui: &mut egui::Ui, stack: &mut Vec<Modifier>, applied: bool, request: &mut Option<Request>) -> bool {
    panel_with(ui, stack, applied, request, true)
}

/// `can_apply` : faux là où « appliquer » n'a pas de sens (pile d'un objet de scène).
pub fn panel_with(ui: &mut egui::Ui, stack: &mut Vec<Modifier>, applied: bool, request: &mut Option<Request>, can_apply: bool) -> bool {
    let p = crate::theme::pal();
    let mut changed = false;
    ui.spacing_mut().slider_width = 90.0;
    ui.set_max_width(ui.available_width());
    ui.horizontal(|ui| {
        ui.label(crate::theme::section(&crate::i18n::t("MODIFICATEURS")));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::containers::menu::MenuButton::new("⋯").ui(ui, |ui| {
                if ui.button(crate::i18n::t("tout déplier")).clicked() {
                    stack.iter_mut().for_each(|m| m.open = true);
                }
                if ui.button(crate::i18n::t("tout replier")).clicked() {
                    stack.iter_mut().for_each(|m| m.open = false);
                }
                if ui.button(crate::i18n::t("tout activer")).clicked() {
                    stack.iter_mut().for_each(|m| m.on = true);
                    changed = true;
                }
                if ui.button(crate::i18n::t("tout désactiver")).clicked() {
                    stack.iter_mut().for_each(|m| m.on = false);
                    changed = true;
                }
                if ui.button(crate::i18n::t("vider la pile")).clicked() {
                    stack.clear();
                    changed = true;
                }
                if !can_apply {
                    return;
                }
                ui.separator();
                if ui.add_enabled(stack.iter().any(|m| m.on), egui::Button::new(crate::i18n::t("tout appliquer"))).on_hover_text(crate::i18n::t("Le résultat devient le maillage de base et la pile se vide")).clicked() {
                    *request = Some(Request::ApplyAll);
                    ui.close();
                }
                if ui.add_enabled(applied, egui::Button::new(crate::i18n::t("revenir au maillage du fichier"))).on_hover_text(crate::i18n::t("Oublie ce qui a été appliqué")).clicked() {
                    *request = Some(Request::Restore);
                    ui.close();
                }
            });
            egui::containers::menu::MenuButton::new(crate::i18n::t("＋ ajouter ▾")).ui(ui, |ui| {
                ui.set_min_width(300.0);
                let id = egui::Id::unique("mod-search");
                let mut q: String = ui.data_mut(|d| d.get_temp(id).unwrap_or_default());
                ui.add(egui::TextEdit::singleline(&mut q).hint_text(crate::i18n::t("chercher…")).desired_width(280.0)).request_focus();
                let qf = crate::chat::fold(&q);
                let mut pick = None;
                if qf.is_empty() {
                    // Sans recherche : une catégorie par sous-menu, comme pour les nodes
                    for cat in CATS {
                        let count = ALL.iter().filter(|x| x.0 == cat).count();
                        ui.menu_button(format!("{}   {count}", crate::i18n::t(cat)), |ui| {
                            ui.set_min_width(240.0);
                            for (i, (_, name, help)) in ALL.iter().enumerate().filter(|(_, x)| x.0 == cat) {
                                if ui.button(crate::i18n::t(name)).on_hover_text(crate::i18n::t(help)).clicked() {
                                    pick = Some(i);
                                }
                            }
                        });
                    }
                } else {
                    egui::ScrollArea::vertical().max_height(440.0).show(ui, |ui| {
                        let mut any = false;
                        for cat in CATS {
                            let items: Vec<(usize, &(&str, &str, &str))> = ALL.iter().enumerate().filter(|(_, x)| x.0 == cat && (crate::chat::fold(&crate::i18n::t(x.1)).contains(&qf) || crate::chat::fold(x.1).contains(&qf) || crate::chat::fold(x.2).contains(&qf) || crate::chat::fold(cat).contains(&qf))).collect();
                            if items.is_empty() {
                                continue;
                            }
                            any = true;
                            ui.label(RichText::new(crate::i18n::t(cat).to_uppercase()).size(10.0).color(p.muted).family(crate::theme::bold()));
                            for (i, (_, name, help)) in items {
                                let r = ui.add(egui::Button::new(RichText::new(crate::i18n::t(name))).frame(false).min_size(egui::vec2(280.0, 0.0)));
                                if r.on_hover_text(crate::i18n::t(help)).clicked() {
                                    pick = Some(i);
                                }
                            }
                            ui.add_space(4.0);
                        }
                        if !any {
                            ui.label(RichText::new(crate::i18n::t("aucun modificateur trouvé")).color(p.muted).size(11.0));
                        }
                    });
                }
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) && pick.is_none() {
                    pick = if qf.is_empty() { None } else { ALL.iter().position(|x| crate::chat::fold(x.1).contains(&qf) || crate::chat::fold(&crate::i18n::t(x.1)).contains(&qf)) };
                }
                if let Some(i) = pick {
                    stack.push(Modifier::new(Kind::new(i)));
                    changed = true;
                    q.clear();
                    ui.close();
                }
                ui.data_mut(|d| d.insert_temp(id, q));
            });
        });
    });
    if stack.is_empty() {
        ui.label(RichText::new(crate::i18n::t("Aucun modificateur. Ils s'appliquent de haut en bas, sans changer le fichier d'origine.")).color(p.muted).size(11.0));
    }
    // (index, action) : -1 haut, 1 bas, 0 supprimer, 2 dupliquer
    let mut action: Option<(usize, i32)> = None;
    let n = stack.len();
    for (i, md) in stack.iter_mut().enumerate() {
        egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, if md.on { p.line } else { p.line.gamma_multiply(0.5) })).inner_margin(6).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(if md.open { "▾" } else { "▸" }).frame(false)).clicked() {
                    md.open = !md.open;
                }
                if ui.checkbox(&mut md.on, "").on_hover_text(crate::i18n::t("activer / désactiver")).changed() {
                    changed = true;
                }
                let t = RichText::new(crate::i18n::t(md.kind.name())).family(crate::theme::bold()).size(12.0);
                if ui.add(egui::Button::new(if md.on { t } else { t.color(p.muted).strikethrough() }).frame(false)).clicked() {
                    md.open = !md.open;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text(crate::i18n::t("supprimer")).clicked() {
                        action = Some((i, 0));
                    }
                    if can_apply && md.on && ui.small_button("✔").on_hover_text(crate::i18n::t("Appliquer : le résultat devient le maillage de base (avec les modificateurs au-dessus)")).clicked() {
                        *request = Some(Request::ApplyUpTo(i));
                    }
                    if ui.small_button("⧉").on_hover_text(crate::i18n::t("dupliquer")).clicked() {
                        action = Some((i, 2));
                    }
                    if i + 1 < n && ui.small_button("↓").on_hover_text(crate::i18n::t("descendre")).clicked() {
                        action = Some((i, 1));
                    }
                    if i > 0 && ui.small_button("↑").on_hover_text(crate::i18n::t("monter")).clicked() {
                        action = Some((i, -1));
                    }
                });
            });
            if md.open {
                changed |= params(ui, &mut md.kind);
                if md.kind.is_deform() {
                    let head = if md.influence < 0.999 || md.limit_on || md.invert { "influence et zone ●" } else { "influence et zone" };
                    if ui.add(egui::Button::new(RichText::new(format!("{} {head}", if md.common_open { "▾" } else { "▸" })).size(11.0).color(p.muted)).frame(false)).clicked() {
                        md.common_open = !md.common_open;
                    }
                    if md.common_open {
                        changed |= ui.add(egui::Slider::new(&mut md.influence, 0.0..=1.0).text(crate::i18n::t("influence"))).changed();
                        changed |= ui.checkbox(&mut md.limit_on, crate::i18n::t("limiter à une zone")).changed();
                        if md.limit_on {
                            changed |= axis_ui(ui, &mut md.limit_axis);
                            changed |= ui.add(egui::Slider::new(&mut md.limit[0], -1.0..=1.0).text(crate::i18n::t("début"))).changed();
                            changed |= ui.add(egui::Slider::new(&mut md.limit[1], -1.0..=1.0).text(crate::i18n::t("fin"))).changed();
                            changed |= ui.add(egui::Slider::new(&mut md.falloff, 0.0..=1.0).text(crate::i18n::t("fondu"))).changed();
                        }
                        changed |= ui.checkbox(&mut md.invert, crate::i18n::t("inverser la zone")).changed();
                    }
                }
            }
        });
        ui.add_space(4.0);
    }
    if let Some((i, a)) = action {
        match a {
            0 => {
                stack.remove(i);
            }
            2 => {
                let c = stack[i];
                stack.insert(i + 1, c);
            }
            -1 => stack.swap(i, i - 1),
            _ => stack.swap(i, i + 1),
        }
        changed = true;
    }
    changed
}

fn axis_ui(ui: &mut egui::Ui, a: &mut u8) -> bool {
    let mut c = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(crate::i18n::t("axe")).size(11.0));
        for (k, l) in ["X", "Y", "Z"].iter().enumerate() {
            c |= ui.selectable_value(a, k as u8, *l).changed();
        }
    });
    c
}

fn vec3_ui(ui: &mut egui::Ui, v: &mut V3, label: &str, speed: f32) -> bool {
    let mut c = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.0));
        for (k, l) in ["x", "y", "z"].iter().enumerate() {
            c |= ui.add(egui::DragValue::new(&mut v[k]).speed(speed).max_decimals(3).prefix(format!("{l} "))).changed();
        }
    });
    c
}

fn bool3_ui(ui: &mut egui::Ui, v: &mut [bool; 3], label: &str) -> bool {
    let mut c = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.0));
        for (k, l) in ["X", "Y", "Z"].iter().enumerate() {
            c |= ui.toggle_value(&mut v[k], *l).changed();
        }
    });
    c
}

fn params(ui: &mut egui::Ui, k: &mut Kind) -> bool {
    let mut c = false;
    fn sl(ui: &mut egui::Ui, v: &mut f32, r: std::ops::RangeInclusive<f32>, t: &str) -> bool {
        ui.add(egui::Slider::new(v, r).text(t).clamping(egui::SliderClamping::Never)).changed()
    }
    macro_rules! s {
        ($ui:expr, $v:expr, $r:expr, $t:expr) => {
            c |= sl($ui, $v, $r, $t)
        };
    }
    match k {
        Kind::Mirror { axis, merge, extra, bisect } => {
            c |= axis_ui(ui, axis);
            c |= bool3_ui(ui, extra, "axes en plus");
            c |= ui.checkbox(merge, crate::i18n::t("souder au milieu")).changed();
            c |= ui.checkbox(bisect, crate::i18n::t("couper avant (bisect)")).changed();
        }
        Kind::Array { count, axis, offset, constant, rotate, scale } => {
            c |= ui.add(egui::Slider::new(count, 1..=256).text(crate::i18n::t("copies")).logarithmic(true)).changed();
            c |= axis_ui(ui, axis);
            s!(ui, offset, -3.0..=3.0, "écart (× taille)");
            c |= vec3_ui(ui, constant, "décalage", 0.01);
            c |= vec3_ui(ui, rotate, "rotation °", 0.5);
            s!(ui, scale, 0.5..=1.5, "échelle par copie");
        }
        Kind::Subdivision { levels, optimal, simple } => {
            c |= ui.add(egui::Slider::new(levels, 0..=5).text(crate::i18n::t("niveaux"))).changed();
            ui.horizontal(|ui| {
                c |= ui.selectable_value(simple, false, crate::i18n::t("Catmull-Clark")).changed();
                c |= ui.selectable_value(simple, true, crate::i18n::t("simple")).changed();
            });
            c |= ui.checkbox(optimal, crate::i18n::t("affichage optimal")).on_hover_text(crate::i18n::t("Le fil de fer montre seulement les arêtes de la cage d'origine")).changed();
        }
        Kind::Decimate { ratio } => s!(ui, ratio, 0.01..=1.0, "proportion gardée"),
        Kind::Smooth { iterations, factor, preserve_volume, axes } => {
            c |= ui.add(egui::Slider::new(iterations, 1..=100).text(crate::i18n::t("passes"))).changed();
            s!(ui, factor, -1.0..=1.0, "force");
            c |= ui.checkbox(preserve_volume, crate::i18n::t("garder le volume (Taubin)")).changed();
            c |= bool3_ui(ui, axes, "axes");
        }
        Kind::Displace { strength, scale, seed, octaves, roughness, direction, midlevel } => {
            s!(ui, strength, -0.5..=0.5, "force");
            s!(ui, scale, 0.1..=40.0, "échelle");
            c |= ui.add(egui::Slider::new(octaves, 1..=10).text(crate::i18n::t("octaves"))).changed();
            s!(ui, roughness, 0.05..=0.95, "rugosité");
            s!(ui, midlevel, 0.0..=1.0, "niveau neutre");
            s!(ui, seed, 0.0..=100.0, "graine");
            ui.horizontal(|ui| {
                ui.label(RichText::new(crate::i18n::t("direction")).size(11.0));
                for (v, l) in [(0u8, "normale"), (1, "X"), (2, "Y"), (3, "Z")] {
                    c |= ui.selectable_value(direction, v, l).changed();
                }
            });
        }
        Kind::Twist { angle, axis } => {
            s!(ui, angle, -1080.0..=1080.0, "angle °");
            c |= axis_ui(ui, axis);
        }
        Kind::Bend { angle, axis } => {
            s!(ui, angle, -360.0..=360.0, "angle °");
            c |= axis_ui(ui, axis);
        }
        Kind::Taper { factor, axis } => {
            s!(ui, factor, -1.0..=3.0, "facteur");
            c |= axis_ui(ui, axis);
        }
        Kind::Spherify { factor, shape, axis } => {
            ui.horizontal(|ui| {
                for (v, l) in [(0u8, "sphère"), (1, "cylindre"), (2, "cube")] {
                    c |= ui.selectable_value(shape, v, l).changed();
                }
            });
            s!(ui, factor, -1.0..=2.0, "facteur");
            if *shape == 1 {
                c |= axis_ui(ui, axis);
            }
        }
        Kind::Solidify { thickness, offset, rim } => {
            s!(ui, thickness, -0.3..=0.3, "épaisseur");
            s!(ui, offset, -1.0..=1.0, "décalage");
            c |= ui.checkbox(rim, crate::i18n::t("fermer les bords")).changed();
        }
        Kind::Wave { amplitude, frequency, axis, phase, radial } => {
            s!(ui, amplitude, -0.3..=0.3, "amplitude");
            s!(ui, frequency, 0.1..=40.0, "fréquence");
            s!(ui, phase, 0.0..=1.0, "phase");
            c |= axis_ui(ui, axis);
            c |= ui.checkbox(radial, crate::i18n::t("circulaire")).changed();
        }
        Kind::Transform { scale, rotate, offset, scale3 } => {
            s!(ui, scale, 0.05..=5.0, "échelle");
            c |= vec3_ui(ui, scale3, "échelle xyz", 0.01);
            c |= vec3_ui(ui, rotate, "rotation °", 0.5);
            c |= vec3_ui(ui, offset, "décalage", 0.01);
        }
        Kind::Weld { distance } => {
            c |= ui.add(egui::Slider::new(distance, 0.00001..=0.2).logarithmic(true).text(crate::i18n::t("distance"))).changed();
        }
        Kind::Triangulate | Kind::FlipNormals => {
            ui.label(RichText::new(crate::i18n::t("sans réglage")).size(11.0));
        }
        Kind::EdgeSplit { angle } => s!(ui, angle, 0.0..=180.0, "angle °"),
        Kind::Wireframe { thickness, keep } => {
            c |= ui.add(egui::Slider::new(thickness, 0.0005..=0.1).logarithmic(true).text(crate::i18n::t("épaisseur"))).changed();
            c |= ui.checkbox(keep, crate::i18n::t("garder le maillage")).changed();
        }
        Kind::Explode { distance, random, seed } => {
            s!(ui, distance, 0.0..=1.0, "distance");
            s!(ui, random, 0.0..=1.0, "aléatoire");
            s!(ui, seed, 0.0..=100.0, "graine");
        }
        Kind::Inflate { distance } => s!(ui, distance, -0.3..=0.3, "distance"),
        Kind::Randomize { amount, seed, along_normal } => {
            s!(ui, amount, 0.0..=0.2, "quantité");
            s!(ui, seed, 0.0..=100.0, "graine");
            c |= ui.checkbox(along_normal, crate::i18n::t("le long des normales")).changed();
        }
        Kind::Voxelize { step } => {
            c |= ui.add(egui::Slider::new(step, 0.002..=0.5).logarithmic(true).text(crate::i18n::t("taille"))).changed();
        }
        Kind::Stretch { factor, axis } => {
            s!(ui, factor, -0.9..=3.0, "facteur");
            c |= axis_ui(ui, axis);
        }
        Kind::Shear { factor, axis, along } => {
            s!(ui, factor, -2.0..=2.0, "facteur");
            ui.label(RichText::new(crate::i18n::t("déplace")).size(11.0));
            c |= axis_ui(ui, axis);
            ui.label(RichText::new(crate::i18n::t("selon")).size(11.0));
            c |= axis_ui(ui, along);
        }
        Kind::Warp { from, to, radius } => {
            c |= vec3_ui(ui, from, "de", 0.01);
            c |= vec3_ui(ui, to, "vers", 0.01);
            s!(ui, radius, 0.01..=2.0, "rayon");
        }
        Kind::Bulge { amount, center, radius } => {
            s!(ui, amount, -1.0..=1.0, "force");
            c |= vec3_ui(ui, center, "centre", 0.01);
            s!(ui, radius, 0.01..=2.0, "rayon");
        }
        Kind::Ripple { amplitude, frequency, decay, axis, phase } => {
            s!(ui, amplitude, -0.2..=0.2, "amplitude");
            s!(ui, frequency, 0.5..=40.0, "fréquence");
            s!(ui, decay, 0.0..=5.0, "amortissement");
            s!(ui, phase, 0.0..=1.0, "phase");
            c |= axis_ui(ui, axis);
        }
        Kind::Screw { angle, steps, height, axis } => {
            s!(ui, angle, -1440.0..=1440.0, "angle total °");
            c |= ui.add(egui::Slider::new(steps, 1..=256).text(crate::i18n::t("étapes")).logarithmic(true)).changed();
            s!(ui, height, -2.0..=2.0, "hauteur");
            c |= axis_ui(ui, axis);
        }
        Kind::Build { fraction, seed, random } => {
            s!(ui, fraction, 0.0..=1.0, "proportion");
            c |= ui.checkbox(random, crate::i18n::t("ordre aléatoire")).changed();
            if *random {
                s!(ui, seed, 0.0..=100.0, "graine");
            }
        }
        Kind::Slice { axis, position, flip } => {
            c |= axis_ui(ui, axis);
            s!(ui, position, -1.0..=1.0, "position");
            c |= ui.checkbox(flip, crate::i18n::t("garder l'autre côté")).changed();
        }
        Kind::Lattice { amount, axis, curve } => {
            s!(ui, amount, -1.0..=1.0, "force");
            c |= axis_ui(ui, axis);
            ui.horizontal(|ui| {
                for (v, l) in [(0u8, "renfle"), (1, "S"), (2, "pointe")] {
                    c |= ui.selectable_value(curve, v, l).changed();
                }
            });
        }
        Kind::Melt { amount, axis } => {
            s!(ui, amount, 0.0..=1.0, "quantité");
            c |= axis_ui(ui, axis);
        }
        Kind::Crumple { amount, scale, seed } => {
            s!(ui, amount, 0.0..=0.2, "force");
            s!(ui, scale, 0.5..=30.0, "échelle");
            s!(ui, seed, 0.0..=100.0, "graine");
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_relus() {
        let dir = std::env::temp_dir().join(format!("aryadne-exp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("cube.obj");
        std::fs::write(&src, "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nv 1 0 1\nv 1 1 1\nv 0 1 1\nf 1 4 3 2\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n").unwrap();
        let model = crate::model3d::load(&src).unwrap();
        let mesh = Mesh::from_model(&model, false);
        for f in EXPORT_FORMATS {
            let out = dir.join(format!("x.{f}"));
            export(&mesh, &model, false, &out).unwrap();
            if *f != "ply" {
                let back = crate::model3d::load(&out).unwrap_or_else(|e| panic!("{f} : {e}"));
                assert_eq!(back.total_tris, 12, "{f}");
                let s = back.size();
                assert!((s[0] - 1.0).abs() < 1e-4, "{f} {s:?}");
            }
        }
        let obj = std::fs::read_to_string(dir.join("x.obj")).unwrap();
        assert_eq!(obj.lines().filter(|l| l.starts_with("f ")).count(), 6, "quads gardés");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn categories_et_maillage_applique() {
        // Chaque modificateur est rangé dans une catégorie du menu
        assert!(ALL.iter().all(|x| CATS.contains(&x.0)));
        assert!(CATS.iter().all(|c| ALL.iter().any(|x| x.0 == *c)));
        // Le maillage appliqué se relit à l'identique
        let m = Mesh { pos: vec![[0.0, 0.5, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], tris: vec![[0, 1, 2]], colors: vec![Color32::from_rgb(10, 20, 30)], edges: vec![7] };
        let f = std::env::temp_dir().join("aryadne-applique.mesh");
        m.save(&f).unwrap();
        let b = Mesh::load(&f).unwrap();
        assert_eq!((b.pos, b.tris, b.colors, b.edges), (m.pos, m.tris, m.colors, m.edges));
        std::fs::write(&f, b"ARYMxxxx").unwrap();
        assert!(Mesh::load(&f).is_none());
    }

    #[test]
    fn pile() {
        let obj = "v -1 -1 -1\nv 1 -1 -1\nv 1 1 -1\nv -1 1 -1\nv -1 -1 1\nv 1 -1 1\nv 1 1 1\nv -1 1 1\nf 1 4 3 2\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n";
        let model = crate::model3d::load_obj(obj.as_bytes()).unwrap();
        let base = Mesh::from_model(&model, false);
        let mk = |k| Modifier::new(k);
        // Réseau de 3 : trois fois plus de tout
        let m = apply(&[mk(Kind::Array { count: 3, axis: 0, offset: 1.0, constant: [0.0; 3], rotate: [0.0; 3], scale: 1.0 })], base.clone(), [0.0; 3]);
        assert_eq!((m.pos.len(), m.tris.len()), (24, 36));
        // Miroir sur un cube centré : aucun sommet sur le plan (x = ±0.5), donc doublé
        let m = apply(&[mk(Kind::Mirror { axis: 0, merge: true, extra: [false; 3], bisect: false })], base.clone(), [0.0; 3]);
        assert_eq!(m.tris.len(), 24);
        // Épaissir un plan ouvert : 2 faces + 4 côtés de 2 triangles
        let plane = crate::model3d::load_obj("v 0 0 0\nv 1 0 0\nv 1 0 1\nv 0 0 1\nf 1 2 3 4\n".as_bytes()).unwrap();
        let m = apply(&[mk(Kind::Solidify { thickness: 0.1, offset: -1.0, rim: true })], Mesh::from_model(&plane, false), [0.0; 3]);
        assert_eq!(m.tris.len(), 2 + 2 + 8);
        // Désactivé = sans effet ; sous-division ×4 ; décimer réduit
        let off = Modifier { on: false, ..Modifier::new(Kind::Subdivision { levels: 2, optimal: false, simple: false }) };
        assert_eq!(apply(&[off], base.clone(), [0.0; 3]).tris.len(), 12);
        let m = apply(&[mk(Kind::Subdivision { levels: 2, optimal: false, simple: false }), mk(Kind::Decimate { ratio: 0.5 })], base.clone(), [0.0; 3]);
        assert!(m.tris.len() < 192 && m.tris.len() > 20, "{}", m.tris.len());
        // Toutes les déformations gardent un maillage valide
        for i in 0..ALL.len() {
            assert_eq!(Kind::new(i).index(), i);
            // Avec influence partielle et zone limitée aussi
            let mut md = mk(Kind::new(i));
            md.influence = 0.5;
            md.limit_on = true;
            let half = apply(&[md], base.clone(), [0.0; 3]);
            assert!(half.pos.iter().all(|p| p.iter().all(|v| v.is_finite())));
            let m = apply(&[mk(Kind::new(i))], base.clone(), [0.0; 3]);
            assert!(m.pos.iter().all(|p| p.iter().all(|v| v.is_finite())), "{}", ALL[i].0);
            assert!(m.tris.iter().all(|t| t.iter().all(|&k| (k as usize) < m.pos.len())));
            assert_eq!(m.tris.len(), m.edges.len());
            assert_eq!(m.tris.len(), m.colors.len());
        }
        // JSON aller-retour
        let st = vec![mk(Kind::Twist { angle: 45.0, axis: 1 })];
        let t = serde_json::to_string(&st).unwrap();
        let back: Vec<Modifier> = serde_json::from_str(&t).unwrap();
        assert_eq!(back, st);
    }
}
