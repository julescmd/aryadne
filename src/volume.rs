//! Données volumiques : fumée, feu.
//!
//! Un volume est une **grille dense** posée dans l'espace : son coin (`origin`), la taille d'un
//! voxel, son nombre de voxels par axe, et des **champs nommés**.
//!
//! | Champ | Sens |
//! |---|---|
//! | `densité` | La fumée (la suie) : ce qui cache la lumière |
//! | `température` | La chaleur, au-dessus de l'air ambiant (1 ≈ une flamme) |
//! | `carburant` | Ce qui reste à brûler |
//! | `flamme` | L'intensité de la combustion, de 0 à 1 : ce qui brille |
//! | `vitesse` | Le mouvement de l'air, sur les faces des voxels (grille décalée) |
//!
//! - **En demi-précision.** Les champs sont rangés en nombres de 16 bits (f16), par paquets de
//!   quatre : 16 octets par voxel pour les huit canaux, en mémoire, dans le cache et sur la carte.
//! - **Paresseux.** Un volume qui sort d'un solveur sur carte graphique y reste : il n'est relu que
//!   si quelqu'un lit ses champs (`grids`). L'aperçu le dessine depuis la carte, sans relecture.
//! - **Opérations en attente.** Les nodes « Source » et « Combustion » ne touchent pas aux
//!   voxels : ils notent ce qu'il y a à faire (`ops`). Le solveur le fait dans son pas, sur la
//!   carte. Sans solveur derrière, c'est fait ici, à la première lecture.
//! - **Rendu.** `march` traverse le volume rayon par rayon, sur le processeur : il sert au rendu
//!   final et à l'export vidéo, et à l'aperçu quand la carte ne peut pas dessiner.

use crate::smoke::{Op, Shot};
use std::sync::{Arc, OnceLock};

/// Les champs d'un volume : nom, nombre de composantes
pub const FIELDS: [(&str, usize); 5] = [("densité", 1), ("température", 1), ("carburant", 1), ("flamme", 1), ("vitesse", 3)];

/// Nombre de 32 bits vers 16 bits, arrondi au plus proche. Hors de portée : la plus grande valeur
/// finie (une simulation ne doit pas fabriquer d'infinis). Pas un nombre : zéro.
pub fn to_half(x: f32) -> u16 {
    if x.is_nan() {
        return 0;
    }
    let b = x.clamp(-65504.0, 65504.0).to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let e = ((b >> 23) & 0xff) as i32 - 112;
    let man = b & 0x7f_ffff;
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = man | 0x80_0000;
        let shift = (14 - e) as u32;
        let (half, rem, mid) = (m >> shift, m & ((1 << shift) - 1), 1u32 << (shift - 1));
        return sign | (half + (rem > mid || (rem == mid && half & 1 == 1)) as u32) as u16;
    }
    let half = ((e as u32) << 10) | (man >> 13);
    let rem = man & 0x1fff;
    sign | (half + (rem > 0x1000 || (rem == 0x1000 && half & 1 == 1)) as u32) as u16
}

fn half_bits(h: u16) -> f32 {
    let sign = ((h as u32) & 0x8000) << 16;
    let (exp, man) = (((h >> 10) & 0x1f) as u32, (h & 0x3ff) as u32);
    f32::from_bits(if exp == 0 {
        if man == 0 {
            sign
        } else {
            let lz = man.leading_zeros() - 22;
            sign | ((112 - lz) << 23) | (((man << (lz + 1)) & 0x3ff) << 13)
        }
    } else if exp == 31 {
        sign | 0x7f80_0000 | (man << 13)
    } else {
        sign | ((exp + 112) << 23) | (man << 13)
    })
}

/// Nombre de 16 bits vers 32 bits (par une table : c'est lu des millions de fois par pas)
#[inline]
pub fn from_half(h: u16) -> f32 {
    static TABLE: OnceLock<Vec<f32>> = OnceLock::new();
    TABLE.get_or_init(|| (0..=u16::MAX).map(half_bits).collect())[h as usize]
}

#[inline]
pub fn pack(v: [f32; 4]) -> [u16; 4] {
    [to_half(v[0]), to_half(v[1]), to_half(v[2]), to_half(v[3])]
}

#[inline]
pub fn unpack(v: [u16; 4]) -> [f32; 4] {
    [from_half(v[0]), from_half(v[1]), from_half(v[2]), from_half(v[3])]
}

/// Les voxels d'un volume. `sca` : densité, température, carburant, flamme. `vel` : la vitesse
/// sur les trois faces basses du voxel (x, y, z), et l'expansion des gaz qui brûlent.
#[derive(Clone, Default, PartialEq)]
pub struct Grids {
    pub sca: Vec<[u16; 4]>,
    pub vel: Vec<[u16; 4]>,
}

impl Grids {
    pub fn zero(cells: usize) -> Grids {
        Grids { sca: vec![[0; 4]; cells], vel: vec![[0; 4]; cells] }
    }
}

#[derive(Clone)]
enum Base {
    Empty,
    Data(Arc<Grids>),
    /// L'état d'un solveur, peut-être encore sur la carte
    Shot(Arc<Shot>),
}

/// Un volume : voir l'en-tête du module.
#[derive(Clone)]
pub struct Volume {
    /// Voxels par axe, bords compris
    pub res: [usize; 3],
    /// Coin bas de la grille, dans le repère de la géométrie
    pub origin: [f32; 3],
    /// Taille d'un voxel
    pub voxel: f32,
    /// Change à chaque modification
    pub version: u64,
    base: Base,
    /// Ce que les nodes ont demandé depuis `base`, dans l'ordre
    pub ops: Vec<Op>,
    /// Son aspect à l'affichage et au rendu
    pub look: Look,
    done: Arc<OnceLock<Arc<Grids>>>,
}

impl std::fmt::Debug for Volume {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Volume({:?}, v{})", self.res, self.version)
    }
}

impl Volume {
    /// Une boîte vide. `res` : voxels par axe.
    pub fn empty(res: [usize; 3], origin: [f32; 3], voxel: f32) -> Volume {
        Volume { res, origin, voxel, version: crate::geo::next_version(), base: Base::Empty, ops: vec![], look: Look::default(), done: Default::default() }
    }

    pub fn from_grids(res: [usize; 3], origin: [f32; 3], voxel: f32, g: Grids) -> Volume {
        debug_assert_eq!(g.sca.len(), res[0] * res[1] * res[2]);
        Volume { base: Base::Data(Arc::new(g)), ..Volume::empty(res, origin, voxel) }
    }

    /// L'état rendu par un solveur
    pub fn from_shot(res: [usize; 3], origin: [f32; 3], voxel: f32, shot: Arc<Shot>) -> Volume {
        Volume { base: Base::Shot(shot), ..Volume::empty(res, origin, voxel) }
    }

    pub fn cells(&self) -> usize {
        self.res[0] * self.res[1] * self.res[2]
    }

    /// Les deux coins de la boîte
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        (self.origin, [0, 1, 2].map(|k| self.origin[k] + self.res[k] as f32 * self.voxel))
    }

    /// Le même volume avec une opération de plus en attente
    pub fn with_op(&self, op: Op) -> Volume {
        let mut v = self.clone();
        v.ops.push(op);
        v.version = crate::geo::next_version();
        v.done = Default::default();
        v
    }

    /// Le même volume, montré autrement
    pub fn with_look(&self, look: Look) -> Volume {
        if look == self.look {
            return self.clone();
        }
        Volume { look, version: crate::geo::next_version(), ..self.clone() }
    }

    /// L'état de solveur dont ce volume part, s'il en vient
    pub fn shot(&self) -> Option<&Arc<Shot>> {
        match &self.base {
            Base::Shot(s) => Some(s),
            _ => None,
        }
    }

    /// De quoi dessiner depuis la carte, si ses tampons montrent encore exactement ce volume
    pub fn live(&self) -> Option<&crate::smoke::Live> {
        if !self.ops.is_empty() {
            return None;
        }
        self.shot().and_then(|s| s.live.as_ref()).filter(|l| l.current())
    }

    /// Les voxels sont-ils lisibles sans attendre la carte ? Ne bloque pas.
    pub fn ready(&self) -> bool {
        self.done.get().is_some() || self.shot().is_none_or(|s| s.ready())
    }

    /// Le volume est-il sûrement vide ? (Sans rien lire.)
    pub fn blank(&self) -> bool {
        matches!(self.base, Base::Empty) && self.ops.is_empty()
    }

    /// Les voxels, opérations en attente faites. La première demande peut attendre la carte.
    pub fn grids(&self) -> Arc<Grids> {
        self.done
            .get_or_init(|| {
                let base = match &self.base {
                    Base::Empty => Arc::new(Grids::zero(self.cells())),
                    Base::Data(g) => g.clone(),
                    Base::Shot(s) => s.grids(),
                };
                if self.ops.is_empty() || base.sca.len() != self.cells() {
                    return base;
                }
                let mut g = (*base).clone();
                crate::smoke::apply_ops(&mut g, self.res, self.origin, self.voxel, &self.ops, 1.0);
                Arc::new(g)
            })
            .clone()
    }

    /// Les voxels sans les opérations en attente (ce qu'un solveur reçoit, avec `ops` à part)
    pub fn base_grids(&self) -> Arc<Grids> {
        match &self.base {
            Base::Empty => Arc::new(Grids::zero(self.cells())),
            Base::Data(g) => g.clone(),
            Base::Shot(s) => s.grids(),
        }
    }

    /// Un champ par son nom, en nombres ordinaires : une liste par composante. La vitesse est
    /// ramenée au centre des voxels.
    pub fn field(&self, name: &str) -> Option<Vec<Vec<f32>>> {
        let g = self.grids();
        let [nx, ny, _] = self.res;
        let k = ["densité", "température", "carburant", "flamme"].iter().position(|n| *n == name);
        if let Some(k) = k {
            return Some(vec![g.sca.iter().map(|v| from_half(v[k])).collect()]);
        }
        (name == "vitesse").then(|| {
            let n = g.vel.len();
            let step = [1, nx, nx * ny];
            (0..3).map(|d| (0..n).map(|i| 0.5 * (from_half(g.vel[i][d]) + from_half(g.vel[(i + step[d]).min(n - 1)][d]))).collect()).collect()
        })
    }

    pub fn bytes(&self) -> usize {
        match (&self.base, self.done.get()) {
            (Base::Empty, None) => 64,
            _ => self.cells() * 16,
        }
    }

    /// Bloc du cache disque : dimensions, puis chaque champ par son nom, en f16.
    pub fn to_bytes(&self) -> Vec<u8> {
        let g = self.grids();
        let n = self.cells();
        let mut b = Vec::with_capacity(n * 14 + 128);
        for x in self.res {
            b.extend_from_slice(&(x as u32).to_le_bytes());
        }
        for x in self.origin.iter().chain([&self.voxel]) {
            b.extend_from_slice(&x.to_le_bytes());
        }
        b.extend_from_slice(&(FIELDS.len() as u32).to_le_bytes());
        for (f, (name, width)) in FIELDS.iter().enumerate() {
            b.extend_from_slice(&(name.len() as u32).to_le_bytes());
            b.extend_from_slice(name.as_bytes());
            b.extend_from_slice(&(*width as u32).to_le_bytes());
            for k in 0..*width {
                let src = if f < 4 { &g.sca } else { &g.vel };
                let comp = if f < 4 { f } else { k };
                for v in src.iter() {
                    b.extend_from_slice(&v[comp].to_le_bytes());
                }
            }
        }
        b
    }

    /// Relit un bloc écrit par `to_bytes`. Rend aussi le nombre d'octets lus. Un champ de nom
    /// inconnu est sauté : un cache plus récent se relit quand même.
    pub fn from_bytes(b: &[u8]) -> Option<(Volume, usize)> {
        let mut at = 0;
        let u32_at = |at: &mut usize| -> Option<u32> {
            let s = b.get(*at..*at + 4)?;
            *at += 4;
            Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        };
        let res = [u32_at(&mut at)? as usize, u32_at(&mut at)? as usize, u32_at(&mut at)? as usize];
        let f = [f32::from_bits(u32_at(&mut at)?), f32::from_bits(u32_at(&mut at)?), f32::from_bits(u32_at(&mut at)?), f32::from_bits(u32_at(&mut at)?)];
        let n = res[0].checked_mul(res[1])?.checked_mul(res[2])?;
        if n == 0 || n > (1 << 31) || b.len() < n {
            return None;
        }
        let mut g = Grids::zero(n);
        for _ in 0..u32_at(&mut at)? {
            let len = u32_at(&mut at)? as usize;
            let name = std::str::from_utf8(b.get(at..at + len)?).ok()?;
            at += len;
            let width = u32_at(&mut at)? as usize;
            let known = FIELDS.iter().position(|x| x.0 == name);
            for k in 0..width {
                let raw = b.get(at..at + n * 2)?;
                at += n * 2;
                let Some(fi) = known else { continue };
                let (dst, comp) = if fi < 4 { (&mut g.sca, fi) } else { (&mut g.vel, k.min(2)) };
                for (v, c) in dst.iter_mut().zip(raw.chunks_exact(2)) {
                    v[comp] = u16::from_le_bytes([c[0], c[1]]);
                }
            }
        }
        Some((Volume::from_grids(res, [f[0], f[1], f[2]], f[3], g), at))
    }
}

// ---------- rendu sur le processeur ----------

/// Réglages de l'aspect d'un volume, les mêmes pour le dessin par la carte et pour ce rendu
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Opacité de la fumée, par unité de densité et par unité de longueur
    pub density: f32,
    /// Couleur de la fumée (linéaire)
    pub color: [f32; 3],
    /// Force de la lueur des flammes
    pub glow: f32,
    /// Ombre de la fumée sur elle-même (0 : aucune)
    pub shadow: f32,
}

impl Default for Look {
    fn default() -> Self {
        Look { density: 12.0, color: [0.72, 0.72, 0.74], glow: 5.0, shadow: 1.0 }
    }
}

/// Couleur d'une flamme selon sa température (1 ≈ rouge orangé, 2 ≈ jaune, 3 et plus ≈ blanc).
/// La même formule est écrite dans le noyau de dessin.
pub fn flame_color(t: f32) -> [f32; 3] {
    let t = t.max(0.0);
    [(t * 1.6).min(1.0), (t * t * 0.42).min(1.0), ((t - 1.6).max(0.0) * 0.55).min(1.0)]
}

/// Le volume vu par le processeur : de quoi lire un voxel et un point quelconque
pub struct Sampler {
    g: Arc<Grids>,
    res: [usize; 3],
    origin: [f32; 3],
    inv: f32,
    pub voxel: f32,
}

impl Sampler {
    pub fn new(v: &Volume) -> Sampler {
        Sampler { g: v.grids(), res: v.res, origin: v.origin, inv: 1.0 / v.voxel.max(1e-9), voxel: v.voxel }
    }

    /// Densité, température, carburant, flamme en un point (lecture trilinéaire ; zéro dehors)
    pub fn at(&self, p: [f32; 3]) -> [f32; 4] {
        let q = [0, 1, 2].map(|k| (p[k] - self.origin[k]) * self.inv - 0.5);
        let [nx, ny, nz] = self.res;
        if q[0] < -0.5 || q[1] < -0.5 || q[2] < -0.5 || q[0] > nx as f32 - 0.5 || q[1] > ny as f32 - 0.5 || q[2] > nz as f32 - 0.5 {
            return [0.0; 4];
        }
        let f = q.map(|x| x.floor());
        let t = [q[0] - f[0], q[1] - f[1], q[2] - f[2]];
        let cl = |x: f32, n: usize| (x as i32).clamp(0, n as i32 - 1) as usize;
        let (x0, y0, z0) = (cl(f[0], nx), cl(f[1], ny), cl(f[2], nz));
        let (x1, y1, z1) = (cl(f[0] + 1.0, nx), cl(f[1] + 1.0, ny), cl(f[2] + 1.0, nz));
        let s = |x: usize, y: usize, z: usize| unpack(self.g.sca[x + nx * (y + ny * z)]);
        let mut out = [0.0; 4];
        for (k, o) in out.iter_mut().enumerate() {
            let a = |x, y, z| s(x, y, z)[k];
            let c00 = a(x0, y0, z0) + (a(x1, y0, z0) - a(x0, y0, z0)) * t[0];
            let c10 = a(x0, y1, z0) + (a(x1, y1, z0) - a(x0, y1, z0)) * t[0];
            let c01 = a(x0, y0, z1) + (a(x1, y0, z1) - a(x0, y0, z1)) * t[0];
            let c11 = a(x0, y1, z1) + (a(x1, y1, z1) - a(x0, y1, z1)) * t[0];
            let (c0, c1) = (c00 + (c10 - c00) * t[1], c01 + (c11 - c01) * t[1]);
            *o = c0 + (c1 - c0) * t[2];
        }
        out
    }

    /// Entrée et sortie d'un rayon dans la boîte (distances le long de `dir`), s'il la touche
    pub fn hit(&self, eye: [f32; 3], dir: [f32; 3]) -> Option<(f32, f32)> {
        let (mut t0, mut t1) = (0.0f32, f32::MAX);
        for k in 0..3 {
            let (lo, hi) = (self.origin[k], self.origin[k] + self.res[k] as f32 * self.voxel);
            if dir[k].abs() < 1e-9 {
                if eye[k] < lo || eye[k] > hi {
                    return None;
                }
                continue;
            }
            let (a, b) = ((lo - eye[k]) / dir[k], (hi - eye[k]) / dir[k]);
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
        }
        (t1 > t0).then_some((t0, t1))
    }

    /// Traverse le volume le long d'un rayon (`dir` unitaire), jusqu'à la distance `far` (ce
    /// qu'il y a derrière : une surface). Rend la couleur ajoutée (linéaire, déjà multipliée par
    /// sa présence) et ce qui reste visible de l'arrière-plan (1 : tout, 0 : rien).
    /// `light` : direction vers la lumière principale. `jitter` (0 à 1) décale le départ.
    pub fn march(&self, eye: [f32; 3], dir: [f32; 3], far: f32, light: [f32; 3], look: &Look, jitter: f32) -> ([f32; 3], f32) {
        let Some((t0, t1)) = self.hit(eye, dir) else { return ([0.0; 3], 1.0) };
        let t1 = t1.min(far);
        let step = self.voxel * 0.9;
        let (mut col, mut trans) = ([0.0f32; 3], 1.0f32);
        let mut t = t0 + step * jitter;
        while t < t1 && trans > 0.01 {
            let p = [eye[0] + dir[0] * t, eye[1] + dir[1] * t, eye[2] + dir[2] * t];
            let s = self.at(p);
            if s[0] > 1e-3 || s[3] > 1e-3 {
                let a = 1.0 - (-s[0] * look.density * step).exp();
                // Ombre : quelques pas vers la lumière
                let mut shade = 1.0;
                if look.shadow > 0.0 && a > 1e-3 {
                    let ls = self.voxel * 2.0;
                    let mut d = 0.0;
                    for k in 1..=6 {
                        let q = [p[0] + light[0] * ls * k as f32, p[1] + light[1] * ls * k as f32, p[2] + light[2] * ls * k as f32];
                        d += self.at(q)[0];
                    }
                    shade = (-d * look.density * ls * look.shadow * 0.6).exp();
                }
                let lit = 0.18 + 0.82 * shade;
                let fc = flame_color(s[1]);
                let glow = s[3] * look.glow * step / self.voxel * 0.25;
                for k in 0..3 {
                    col[k] += trans * (a * look.color[k] * lit + glow * fc[k]);
                }
                trans *= 1.0 - a;
            }
            t += step;
        }
        (col, trans)
    }
}

// ---------- matrices et composition ----------

/// Matrice 4×4, colonnes d'abord (comme `gpudraw`)
pub type Mat = [[f32; 4]; 4];

pub fn mat_mul(a: &Mat, b: &Mat) -> Mat {
    let mut m = [[0.0; 4]; 4];
    for (c, col) in m.iter_mut().enumerate() {
        for (r, x) in col.iter_mut().enumerate() {
            *x = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    m
}

/// Un point passé par la matrice (avec sa division)
pub fn mat_point(m: &Mat, p: [f32; 3]) -> [f32; 3] {
    let f = |r: usize| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r];
    let w = f(3);
    let w = if w.abs() < 1e-20 { 1.0 } else { w };
    [f(0) / w, f(1) / w, f(2) / w]
}

/// Une direction passée par la matrice
pub fn mat_dir(m: &Mat, d: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|r| m[0][r] * d[0] + m[1][r] * d[1] + m[2][r] * d[2])
}

/// L'inverse, par élimination de Gauss. Rien si la matrice n'en a pas.
pub fn mat_inv(m: &Mat) -> Option<Mat> {
    // Lignes de [M | I], en double précision
    let mut a = [[0.0f64; 8]; 4];
    for r in 0..4 {
        for c in 0..4 {
            a[r][c] = m[c][r] as f64;
        }
        a[r][4 + r] = 1.0;
    }
    for i in 0..4 {
        let piv = (i..4).max_by(|x, y| a[*x][i].abs().total_cmp(&a[*y][i].abs()))?;
        if a[piv][i].abs() < 1e-30 {
            return None;
        }
        a.swap(i, piv);
        let d = a[i][i];
        for x in a[i].iter_mut() {
            *x /= d;
        }
        for r in 0..4 {
            if r != i {
                let f = a[r][i];
                for c in 0..8 {
                    a[r][c] -= f * a[i][c];
                }
            }
        }
    }
    let mut out = [[0.0f32; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            out[c][r] = a[r][4 + c] as f32;
        }
    }
    Some(out)
}

impl Volume {
    /// Du monde vers la grille, en voxels. `model` : place de la géométrie dans le monde.
    pub fn to_grid(&self, model: &Mat) -> Option<Mat> {
        let inv = mat_inv(model)?;
        let k = 1.0 / self.voxel.max(1e-12);
        let local: Mat = [[k, 0.0, 0.0, 0.0], [0.0, k, 0.0, 0.0], [0.0, 0.0, k, 0.0], [-self.origin[0] * k, -self.origin[1] * k, -self.origin[2] * k, 1.0]];
        Some(mat_mul(&local, &inv))
    }
}

/// Le point de vue d'un rendu sur le processeur
pub struct View<'a> {
    /// Le rayon d'un pixel (colonne, ligne) : départ, direction unitaire, distance de ce qui est
    /// déjà dessiné à ce pixel (`f32::MAX` : rien)
    pub rays: &'a (dyn Fn(usize, usize) -> ([f32; 3], [f32; 3], f32) + Sync),
    /// Direction vers la lumière principale
    pub light: [f32; 3],
}

/// Pose des volumes par-dessus une image déjà rendue (RGBA 8 bits, sRGB), pixel par pixel, en
/// s'arrêtant à ce qui est déjà dessiné. Chaque volume vient avec sa place dans l'espace des rayons.
pub fn composite(rgba: &mut [u8], w: usize, h: usize, vols: &[(Arc<Volume>, Mat)], view: &View) {
    use rayon::prelude::*;
    let items: Vec<(Sampler, Mat, [f32; 3], Look)> = vols
        .iter()
        .filter(|(v, _)| !v.blank())
        .filter_map(|(v, model)| {
            let inv = mat_inv(model)?;
            let l = mat_dir(&inv, view.light);
            let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt().max(1e-12);
            Some((Sampler::new(v), inv, [l[0] / n, l[1] / n, l[2] / n], v.look))
        })
        .collect();
    if items.is_empty() || rgba.len() < w * h * 4 {
        return;
    }
    let lin = |v: u8| (v as f32 / 255.0).powf(2.2);
    let srgb = |x: f32| (x.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0 + 0.5) as u8;
    rgba.par_chunks_mut(w * 4).take(h).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let (o, d, far) = (view.rays)(x, y);
            let jitter = crate::smoke::rnd((y * w + x) as u32, 17);
            for (s, inv, light, look) in &items {
                let (ol, dl) = (mat_point(inv, o), mat_dir(inv, d));
                let k = (dl[0] * dl[0] + dl[1] * dl[1] + dl[2] * dl[2]).sqrt().max(1e-12);
                let far = if far >= f32::MAX * 0.5 { f32::MAX } else { far * k };
                let (col, trans) = s.march(ol, [dl[0] / k, dl[1] / k, dl[2] / k], far, *light, look, jitter);
                if trans >= 1.0 && col == [0.0; 3] {
                    continue;
                }
                let px = &mut row[x * 4..x * 4 + 4];
                let back = px[3] as f32 / 255.0;
                for c in 0..3 {
                    px[c] = srgb(col[c] + trans * lin(px[c]) * back.max(if back > 0.0 { 1.0 } else { 0.0 }));
                }
                let cover = (1.0 - trans).max(col[0].max(col[1]).max(col[2])).clamp(0.0, 1.0);
                px[3] = ((back + (1.0 - back) * cover) * 255.0 + 0.5) as u8;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demi_precision_aller_retour() {
        for x in [0.0f32, 1.0, -1.0, 0.5, 0.1, 3.14159, 1000.0, 65504.0, 1e-5, 6e-8, -2.5e-3] {
            let h = from_half(to_half(x));
            assert!((h - x).abs() <= x.abs() * 6e-4 + 3e-8, "{x} → {h}");
        }
        // Chaque valeur de 16 bits finie revient telle quelle
        for h in (0..0x7c00u16).chain(0x8000..0xfc00) {
            assert_eq!(to_half(from_half(h)), h);
        }
        assert_eq!(from_half(to_half(1e9)), 65504.0);
        assert_eq!(to_half(f32::NAN), 0);
    }

    #[test]
    fn volume_et_cache() {
        let mut g = Grids::zero(4 * 3 * 2);
        for (i, v) in g.sca.iter_mut().enumerate() {
            *v = pack([i as f32 * 0.25, 1.5, 0.0, 0.5]);
        }
        g.vel[5] = pack([1.0, -2.0, 3.0, 0.0]);
        let v = Volume::from_grids([4, 3, 2], [1.0, 2.0, 3.0], 0.5, g.clone());
        let (back, used) = Volume::from_bytes(&v.to_bytes()).unwrap();
        assert_eq!(used, v.to_bytes().len());
        assert_eq!((back.res, back.origin, back.voxel), (v.res, v.origin, v.voxel));
        assert!(*back.grids() == g);
        assert_eq!(v.field("densité").unwrap()[0][4], 1.0);
        assert_eq!(v.field("vitesse").unwrap().len(), 3);
        // 14 octets par voxel dans le cache, contre 32 en nombres ordinaires
        assert!(v.to_bytes().len() < 24 * 14 + 200);
    }

    #[test]
    fn un_rayon_traverse_la_fumee() {
        let n = 8;
        let mut g = Grids::zero(n * n * n);
        for v in g.sca.iter_mut() {
            *v = pack([1.0, 0.0, 0.0, 0.0]);
        }
        let v = Volume::from_grids([n, n, n], [0.0; 3], 0.125, g);
        let s = Sampler::new(&v);
        let look = Look::default();
        let (col, trans) = s.march([0.5, 0.5, -1.0], [0.0, 0.0, 1.0], f32::MAX, [0.0, 1.0, 0.0], &look, 0.5);
        assert!(trans < 0.05 && col[0] > 0.05, "{col:?} {trans}");
        // Une surface devant la fumée la cache ; un rayon à côté ne voit rien
        assert_eq!(s.march([0.5, 0.5, -1.0], [0.0, 0.0, 1.0], 0.5, [0.0, 1.0, 0.0], &look, 0.5).1, 1.0);
        assert_eq!(s.march([5.0, 0.5, -1.0], [0.0, 0.0, 1.0], f32::MAX, [0.0, 1.0, 0.0], &look, 0.5).1, 1.0);
    }
}
