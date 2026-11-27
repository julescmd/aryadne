//! Fumée et feu : le solveur, et sa version processeur.
//!
//! Un pas de fumée, dans l'ordre :
//!
//! 1. **Advection** de la fumée puis de la vitesse : MacCormack (un aller, un retour, on corrige la
//!    moitié de l'erreur, bornée par les voisins) ou semi-lagrangienne simple.
//! 2. **Opérations en attente** : les sources (`Emit`), puis la combustion (`Burn`), la
//!    dissipation et le refroidissement.
//! 3. **Forces** : flottabilité (le chaud monte, la suie pèse), tourbillons rendus (confinement
//!    de vorticité), vent, amortissement, et vitesse nulle contre les obstacles.
//! 4. **Pression** : on cherche la pression qui rend la vitesse sans divergence (ou égale à
//!    l'expansion des gaz qui brûlent). Gradient conjugué préconditionné par un cycle en V de
//!    multigrille (MGPCG), sans matrice : le système est lu dans la grille des types de voxels.
//!    La pression du pas d'avant sert de point de départ.
//! 5. **Projection** : la vitesse perd le gradient de cette pression.
//!
//! La grille est **décalée** : la vitesse vit sur les faces des voxels. La projection est alors
//! exacte : la divergence qui reste est le résidu du solveur, rien d'autre.
//!
//! Types de voxels : fluide, solide (obstacle, sol), ouvert (bord où l'air entre et sort,
//! pression nulle). La couche de voxels du bord de la grille n'est jamais fluide.
//!
//! Ce fichier tient la version processeur, qui est la référence : `gpusmoke` fait les mêmes
//! calculs sur la carte, noyau par noyau, et les tests comparent les deux. Les champs sont en
//! demi-précision des deux côtés (voir `volume`).

use crate::volume::{from_half, pack, to_half, unpack, Grids};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub const FLUID: u32 = 0;
pub const SOLID: u32 = 1;
pub const OPEN: u32 = 2;

/// Balayages de lissage avant et après chaque niveau, et sur le niveau le plus grossier
pub const SWEEPS: usize = 2;
pub const COARSE_SWEEPS: usize = 8;

/// Réglages d'un pas
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Params {
    pub substeps: usize,
    /// Itérations de pression (MGPCG) par sous-pas
    pub iterations: usize,
    /// Advection MacCormack (sinon semi-lagrangienne simple)
    pub maccormack: bool,
    /// Poussée vers le haut par unité de température, poids par unité de densité
    pub buoyancy: f32,
    pub weight: f32,
    /// Force des tourbillons rendus (0 : aucun)
    pub vorticity: f32,
    /// Part de la fumée qui disparaît par seconde, de la chaleur perdue par seconde
    pub dissipation: f32,
    pub cooling: f32,
    /// Frein de l'air, par seconde
    pub damping: f32,
    pub wind: [f32; 3],
    /// Bords : 0 ouverts sauf le sol, 1 tous ouverts, 2 fermés sauf le haut
    pub borders: u32,
    pub ball: Option<([f32; 3], f32)>,
    /// Boîte : centre et demi-tailles
    pub cube: Option<([f32; 3], [f32; 3])>,
}

impl Default for Params {
    fn default() -> Self {
        Params { substeps: 1, iterations: 4, maccormack: true, buoyancy: 4.0, weight: 0.5, vorticity: 2.0, dissipation: 0.1, cooling: 0.4, damping: 0.0, wind: [0.0; 3], borders: 0, ball: None, cube: None }
    }
}

/// Zone d'une source
#[derive(Clone, Debug)]
pub enum Shape {
    Ball([f32; 3], f32),
    /// Centre et demi-tailles
    Cube([f32; 3], [f32; 3]),
    /// Une part par voxel, de 0 à 1 (des points, une géométrie)
    Mask(Arc<Mask>),
}

#[derive(Debug)]
pub struct Mask {
    pub w: Vec<f32>,
    /// Identité : deux masques de même numéro ont le même contenu
    pub id: u64,
}

/// Une source : ce qu'elle pose dans les voxels de sa zone
#[derive(Clone, Debug)]
pub struct Emit {
    pub shape: Shape,
    pub density: f32,
    pub temperature: f32,
    pub fuel: f32,
    /// Vitesse donnée à l'air (rien si nulle)
    pub velocity: [f32; 3],
    /// Ajouter (valeurs par seconde) ou fixer (au moins ces valeurs)
    pub add: bool,
    /// Largeur du bord doux de la zone
    pub soft: f32,
    /// Irrégularité de la source (0 à 1) et sa graine, qui change à chaque image
    pub noise: f32,
    pub seed: u32,
    pub dt: f32,
}

/// La combustion : le carburant plus chaud que `ignition` brûle
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Burn {
    pub ignition: f32,
    /// Carburant brûlé par seconde
    pub rate: f32,
    /// Chaleur et fumée données par unité de carburant brûlé
    pub heat: f32,
    pub smoke: f32,
    /// Gonflement des gaz par unité de carburant brûlé
    pub expansion: f32,
    /// Refroidissement par rayonnement (en T⁴ : les flammes très chaudes refroidissent vite)
    pub cooling: f32,
    pub dt: f32,
}

#[derive(Clone, Debug)]
pub enum Op {
    Emit(Emit),
    Burn(Burn),
}

/// Dimensions d'une grille, et ses numéros de voxel
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Dim {
    pub nx: usize,
    pub ny: usize,
    pub nz: usize,
}

impl Dim {
    pub fn of(res: [usize; 3]) -> Dim {
        Dim { nx: res[0], ny: res[1], nz: res[2] }
    }
    pub fn cells(&self) -> usize {
        self.nx * self.ny * self.nz
    }
    #[inline]
    pub fn at(&self, c: usize) -> [i32; 3] {
        [(c % self.nx) as i32, ((c / self.nx) % self.ny) as i32, (c / (self.nx * self.ny)) as i32]
    }
    /// Numéro du voxel, coordonnées ramenées dans la grille
    #[inline]
    pub fn idx(&self, x: i32, y: i32, z: i32) -> usize {
        x.clamp(0, self.nx as i32 - 1) as usize + self.nx * (y.clamp(0, self.ny as i32 - 1) as usize + self.ny * z.clamp(0, self.nz as i32 - 1) as usize)
    }
    #[inline]
    pub fn inside(&self, x: i32, y: i32, z: i32) -> bool {
        x >= 0 && y >= 0 && z >= 0 && x < self.nx as i32 && y < self.ny as i32 && z < self.nz as i32
    }
    pub fn half(&self) -> Option<Dim> {
        (self.nx % 2 == 0 && self.ny % 2 == 0 && self.nz % 2 == 0 && self.nx >= 8 && self.ny >= 8 && self.nz >= 8).then_some(Dim { nx: self.nx / 2, ny: self.ny / 2, nz: self.nz / 2 })
    }
    /// Les niveaux de la multigrille, du plus fin au plus grossier
    pub fn levels(&self) -> Vec<Dim> {
        let mut v = vec![*self];
        while let Some(d) = v[v.len() - 1].half() {
            v.push(d);
        }
        v
    }
}

/// Nombre pseudo-aléatoire entre 0 et 1 : le même calcul, en entiers, que dans le noyau
#[inline]
pub fn rnd(c: u32, seed: u32) -> f32 {
    let mut x = c.wrapping_mul(0x9E37_79B9) ^ seed.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / 16_777_216.0
}

const DIRS: [[i32; 3]; 6] = [[-1, 0, 0], [1, 0, 0], [0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1]];

#[inline]
fn typ_at(t: &[u32], d: Dim, x: i32, y: i32, z: i32) -> u32 {
    if d.inside(x, y, z) {
        t[d.idx(x, y, z)]
    } else {
        SOLID
    }
}

/// Types des voxels : la couche du bord selon `borders`, les obstacles, le reste fluide
pub fn cell_types(d: Dim, origin: [f32; 3], h: f32, par: &Params) -> Vec<u32> {
    (0..d.cells())
        .into_par_iter()
        .map(|c| {
            let [x, y, z] = d.at(c);
            if x == 0 || y == 0 || z == 0 || x == d.nx as i32 - 1 || y == d.ny as i32 - 1 || z == d.nz as i32 - 1 {
                return match par.borders {
                    0 => if y == 0 { SOLID } else { OPEN },
                    1 => OPEN,
                    _ => if y == d.ny as i32 - 1 { OPEN } else { SOLID },
                };
            }
            let p = [origin[0] + (x as f32 + 0.5) * h, origin[1] + (y as f32 + 0.5) * h, origin[2] + (z as f32 + 0.5) * h];
            if let Some((c, r)) = par.ball {
                if (p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2) < r * r {
                    return SOLID;
                }
            }
            if let Some((c, half)) = par.cube {
                if (0..3).all(|k| (p[k] - c[k]).abs() < half[k]) {
                    return SOLID;
                }
            }
            FLUID
        })
        .collect()
}

/// Types du niveau plus grossier : ouvert si un des huit enfants l'est, sinon fluide si un l'est
pub fn coarsen_types(t: &[u32], d: Dim, dc: Dim) -> Vec<u32> {
    (0..dc.cells())
        .into_par_iter()
        .map(|c| {
            let [x, y, z] = dc.at(c);
            let (mut open, mut fluid) = (false, false);
            for k in 0..8 {
                let v = t[d.idx(2 * x + (k & 1), 2 * y + ((k >> 1) & 1), 2 * z + (k >> 2))];
                open |= v == OPEN;
                fluid |= v == FLUID;
            }
            if open { OPEN } else if fluid { FLUID } else { SOLID }
        })
        .collect()
}

/// Lecture trilinéaire d'une composante au point `q` (en voxels, dans le réseau de cette
/// composante). Rend la valeur, et le plus petit et le plus grand des huit voisins.
#[inline]
fn tri(a: &[[u16; 4]], d: Dim, q: [f32; 3], k: usize) -> (f32, f32, f32) {
    let qx = q[0].clamp(0.0, d.nx as f32 - 1.0);
    let qy = q[1].clamp(0.0, d.ny as f32 - 1.0);
    let qz = q[2].clamp(0.0, d.nz as f32 - 1.0);
    let (fx, fy, fz) = (qx.floor(), qy.floor(), qz.floor());
    let (tx, ty, tz) = (qx - fx, qy - fy, qz - fz);
    let (x0, y0, z0) = (fx as i32, fy as i32, fz as i32);
    let v = |x: i32, y: i32, z: i32| from_half(a[d.idx(x, y, z)][k]);
    let c = [v(x0, y0, z0), v(x0 + 1, y0, z0), v(x0, y0 + 1, z0), v(x0 + 1, y0 + 1, z0), v(x0, y0, z0 + 1), v(x0 + 1, y0, z0 + 1), v(x0, y0 + 1, z0 + 1), v(x0 + 1, y0 + 1, z0 + 1)];
    let mix = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let val = mix(mix(mix(c[0], c[1], tx), mix(c[2], c[3], tx), ty), mix(mix(c[4], c[5], tx), mix(c[6], c[7], tx), ty), tz);
    let (mut lo, mut hi) = (c[0], c[0]);
    for x in &c[1..] {
        lo = lo.min(*x);
        hi = hi.max(*x);
    }
    (val, lo, hi)
}

/// Vitesse au centre d'un voxel
#[inline]
fn vel_center(v: &[[u16; 4]], d: Dim, x: i32, y: i32, z: i32) -> [f32; 3] {
    let c = d.idx(x, y, z);
    [0.5 * (from_half(v[c][0]) + from_half(v[d.idx(x + 1, y, z)][0])), 0.5 * (from_half(v[c][1]) + from_half(v[d.idx(x, y + 1, z)][1])), 0.5 * (from_half(v[c][2]) + from_half(v[d.idx(x, y, z + 1)][2]))]
}

/// Vitesse au milieu de la face basse d'axe `k` d'un voxel
#[inline]
fn vel_face(v: &[[u16; 4]], d: Dim, x: i32, y: i32, z: i32, k: usize) -> [f32; 3] {
    let mut u = [0.0; 3];
    let e = |a: usize| [(a == 0) as i32, (a == 1) as i32, (a == 2) as i32];
    let ek = e(k);
    for (a, ua) in u.iter_mut().enumerate() {
        if a == k {
            *ua = from_half(v[d.idx(x, y, z)][k]);
        } else {
            let ea = e(a);
            let f = |dx: [i32; 3]| from_half(v[d.idx(x + dx[0], y + dx[1], z + dx[2])][a]);
            *ua = 0.25 * (f([0; 3]) + f([-ek[0], -ek[1], -ek[2]]) + f(ea) + f([ea[0] - ek[0], ea[1] - ek[1], ea[2] - ek[2]]));
        }
    }
    u
}

/// Advection de la fumée (densité, température, carburant, flamme). `dtx` : pas de temps divisé
/// par la taille d'un voxel. Rend le nouveau champ.
pub fn advect_sca(g: &Grids, d: Dim, dtx: f32, mac: bool) -> Vec<[u16; 4]> {
    let fwd: Vec<[u16; 4]> = (0..d.cells())
        .into_par_iter()
        .map(|c| {
            let [x, y, z] = d.at(c);
            let u = vel_center(&g.vel, d, x, y, z);
            let q = [x as f32 - u[0] * dtx, y as f32 - u[1] * dtx, z as f32 - u[2] * dtx];
            pack([0, 1, 2, 3].map(|k| tri(&g.sca, d, q, k).0))
        })
        .collect();
    if !mac {
        return fwd;
    }
    (0..d.cells())
        .into_par_iter()
        .map(|c| {
            let [x, y, z] = d.at(c);
            let u = vel_center(&g.vel, d, x, y, z);
            let back = [x as f32 - u[0] * dtx, y as f32 - u[1] * dtx, z as f32 - u[2] * dtx];
            let ahead = [x as f32 + u[0] * dtx, y as f32 + u[1] * dtx, z as f32 + u[2] * dtx];
            pack([0, 1, 2, 3].map(|k| {
                let (_, lo, hi) = tri(&g.sca, d, back, k);
                let again = tri(&fwd, d, ahead, k).0;
                (from_half(fwd[c][k]) + 0.5 * (from_half(g.sca[c][k]) - again)).clamp(lo, hi)
            }))
        })
        .collect()
}

/// Advection de la vitesse, composante par composante, chacune sur ses faces
pub fn advect_vel(g: &Grids, d: Dim, dtx: f32, mac: bool) -> Vec<[u16; 4]> {
    let fwd: Vec<[u16; 4]> = (0..d.cells())
        .into_par_iter()
        .map(|c| {
            let [x, y, z] = d.at(c);
            let mut out = [0.0; 4];
            for k in 0..3 {
                let u = vel_face(&g.vel, d, x, y, z, k);
                out[k] = tri(&g.vel, d, [x as f32 - u[0] * dtx, y as f32 - u[1] * dtx, z as f32 - u[2] * dtx], k).0;
            }
            pack(out)
        })
        .collect();
    if !mac {
        return fwd;
    }
    (0..d.cells())
        .into_par_iter()
        .map(|c| {
            let [x, y, z] = d.at(c);
            let mut out = [0.0; 4];
            for k in 0..3 {
                let u = vel_face(&g.vel, d, x, y, z, k);
                let (_, lo, hi) = tri(&g.vel, d, [x as f32 - u[0] * dtx, y as f32 - u[1] * dtx, z as f32 - u[2] * dtx], k);
                let again = tri(&fwd, d, [x as f32 + u[0] * dtx, y as f32 + u[1] * dtx, z as f32 + u[2] * dtx], k).0;
                out[k] = (from_half(fwd[c][k]) + 0.5 * (from_half(g.vel[c][k]) - again)).clamp(lo, hi);
            }
            pack(out)
        })
        .collect()
}

/// Part d'un voxel prise par une source (0 à 1), irrégularité comprise
#[inline]
fn coverage(e: &Emit, d: Dim, origin: [f32; 3], h: f32, x: i32, y: i32, z: i32) -> f32 {
    if !d.inside(x, y, z) {
        return 0.0;
    }
    let p = [origin[0] + (x as f32 + 0.5) * h, origin[1] + (y as f32 + 0.5) * h, origin[2] + (z as f32 + 0.5) * h];
    let soft = e.soft.max(h * 0.5);
    let w = match &e.shape {
        Shape::Ball(c, r) => ((r - ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt()) / soft).clamp(0.0, 1.0),
        Shape::Cube(c, half) => {
            let s = ((p[0] - c[0]).abs() - half[0]).max((p[1] - c[1]).abs() - half[1]).max((p[2] - c[2]).abs() - half[2]);
            (-s / soft).clamp(0.0, 1.0)
        }
        Shape::Mask(m) => m.w.get(d.idx(x, y, z)).copied().unwrap_or(0.0),
    };
    // Le hasard va par blocs de quatre voxels : des bouffées, pas du grain
    let block = (x as u32 >> 2) + 1024 * ((y as u32 >> 2) + 1024 * (z as u32 >> 2));
    w * (1.0 - e.noise * rnd(block, e.seed))
}

/// Une source, sur place. `scale` : part du pas de temps de la source faite ici (sous-pas).
pub fn emit(g: &mut Grids, d: Dim, origin: [f32; 3], h: f32, e: &Emit, scale: f32) {
    let dt = e.dt * scale;
    let moving = e.velocity.iter().any(|v| *v != 0.0);
    let target = [e.density, e.temperature, e.fuel];
    g.sca.par_iter_mut().zip(g.vel.par_iter_mut()).enumerate().for_each(|(c, (s, v))| {
        let [x, y, z] = d.at(c);
        let w = coverage(e, d, origin, h, x, y, z);
        if w > 0.0 {
            let mut a = unpack(*s);
            for k in 0..3 {
                a[k] = if e.add { a[k] + target[k] * w * dt } else { a[k].max(target[k] * w) };
            }
            *s = pack(a);
        }
        if moving {
            let mut u = unpack(*v);
            let wf = [0.5 * (w + coverage(e, d, origin, h, x - 1, y, z)), 0.5 * (w + coverage(e, d, origin, h, x, y - 1, z)), 0.5 * (w + coverage(e, d, origin, h, x, y, z - 1))];
            if wf.iter().any(|x| *x > 0.0) {
                for k in 0..3 {
                    u[k] += (e.velocity[k] - u[k]) * wf[k].min(1.0);
                }
                *v = pack(u);
            }
        }
    });
}

/// Réglages du noyau « réaction », déjà calculés pour un pas de temps
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct React {
    /// Ce qui reste de la fumée et de la chaleur après le pas
    pub keep_density: f32,
    pub keep_heat: f32,
    pub dt: f32,
    pub burn: Option<Burn>,
}

impl React {
    pub fn new(par: &Params, burn: Option<Burn>, dt: f32) -> React {
        React { keep_density: (-par.dissipation.max(0.0) * dt).exp(), keep_heat: (-par.cooling.max(0.0) * dt).exp(), dt, burn }
    }
    /// Ce que la flamme garde d'un pas au suivant
    pub fn fade(&self) -> f32 {
        (-self.dt * 6.0).exp()
    }
}

/// Dissipation, refroidissement, combustion, sur place. Hors du fluide, tout est remis à zéro.
pub fn react(g: &mut Grids, typ: Option<&[u32]>, r: &React) {
    let fade = r.fade();
    g.sca.par_iter_mut().zip(g.vel.par_iter_mut()).enumerate().for_each(|(c, (s, v))| {
        if typ.is_some_and(|t| t[c] != FLUID) {
            *s = [0; 4];
            v[3] = 0;
            return;
        }
        let mut a = unpack(*s);
        a[0] *= r.keep_density;
        a[1] *= r.keep_heat;
        a[3] *= fade;
        let mut grow = 0.0;
        if let Some(b) = &r.burn {
            if a[2] > 0.0 && a[1] > b.ignition {
                let most = b.rate * r.dt;
                let burned = a[2].min(most);
                a[2] -= burned;
                a[1] += b.heat * burned;
                a[0] += b.smoke * burned;
                a[3] = a[3].max(burned / most.max(1e-9));
                grow = b.expansion * burned / r.dt.max(1e-9);
            }
            // Rayonnement : solution exacte de dT/dt = −k·T⁴
            let t = a[1].max(0.0);
            a[1] = t / (1.0 + 3.0 * b.cooling * r.dt * t * t * t).powf(1.0 / 3.0);
        }
        *s = pack(a);
        v[3] = to_half(grow);
    });
}

/// Rotationnel de la vitesse au centre des voxels, et sa norme. `inv_h` : 1 / taille d'un voxel.
pub fn curl(vel: &[[u16; 4]], d: Dim, inv_h: f32) -> Vec<[u16; 4]> {
    (0..d.cells())
        .into_par_iter()
        .map(|c| {
            let [x, y, z] = d.at(c);
            let u = |dx: i32, dy: i32, dz: i32| vel_center(vel, d, x + dx, y + dy, z + dz);
            let (xp, xm, yp, ym, zp, zm) = (u(1, 0, 0), u(-1, 0, 0), u(0, 1, 0), u(0, -1, 0), u(0, 0, 1), u(0, 0, -1));
            let k = 0.5 * inv_h;
            let w = [(yp[2] - ym[2] - zp[1] + zm[1]) * k, (zp[0] - zm[0] - xp[2] + xm[2]) * k, (xp[1] - xm[1] - yp[0] + ym[0]) * k];
            pack([w[0], w[1], w[2], (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt()])
        })
        .collect()
}

/// Force qui rend les tourbillons : vers le cœur de chaque tourbillon, tournée par lui.
/// `eps` : force × taille d'un voxel.
pub fn confine(w: &[[u16; 4]], d: Dim, eps: f32) -> Vec<[u16; 4]> {
    (0..d.cells())
        .into_par_iter()
        .map(|c| {
            let [x, y, z] = d.at(c);
            let m = |dx: i32, dy: i32, dz: i32| from_half(w[d.idx(x + dx, y + dy, z + dz)][3]);
            let n = [m(1, 0, 0) - m(-1, 0, 0), m(0, 1, 0) - m(0, -1, 0), m(0, 0, 1) - m(0, 0, -1)];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len < 1e-6 {
                return [0; 4];
            }
            let n = [n[0] / len, n[1] / len, n[2] / len];
            let o = unpack(w[c]);
            pack([eps * (n[1] * o[2] - n[2] * o[1]), eps * (n[2] * o[0] - n[0] * o[2]), eps * (n[0] * o[1] - n[1] * o[0]), 0.0])
        })
        .collect()
}

/// Réglages du noyau « forces », déjà calculés pour un pas de temps
#[derive(Clone, Copy, Debug)]
pub struct Forces {
    pub dt: f32,
    pub buoyancy: f32,
    pub weight: f32,
    pub keep: f32,
    pub wind: [f32; 3],
    /// Part de l'écart au vent rattrapée pendant le pas
    pub follow: f32,
}

impl Forces {
    pub fn new(par: &Params, dt: f32) -> Forces {
        let windy = par.wind.iter().any(|w| *w != 0.0);
        Forces { dt, buoyancy: par.buoyancy, weight: par.weight, keep: (-par.damping.max(0.0) * dt).exp(), wind: par.wind, follow: if windy { 1.0 - (-2.0 * dt).exp() } else { 0.0 } }
    }
}

/// Forces sur les faces, sur place : flottabilité, tourbillons (`force`, au centre des voxels),
/// vent, frein. Une face contre un solide, ou qui ne touche aucun voxel fluide, est mise à zéro.
pub fn forces(g: &mut Grids, d: Dim, typ: &[u32], force: Option<&[[u16; 4]]>, f: &Forces) {
    let sca = &g.sca;
    g.vel.par_iter_mut().enumerate().for_each(|(c, v)| {
        let [x, y, z] = d.at(c);
        let here = typ[c];
        let mut u = unpack(*v);
        for k in 0..3 {
            let (bx, by, bz) = (x - (k == 0) as i32, y - (k == 1) as i32, z - (k == 2) as i32);
            let below = typ_at(typ, d, bx, by, bz);
            if here == SOLID || below == SOLID || (here != FLUID && below != FLUID) {
                u[k] = 0.0;
                continue;
            }
            let b = d.idx(bx, by, bz);
            let mut acc = 0.0;
            if k == 1 {
                let (s0, s1) = (unpack(sca[c]), unpack(sca[b]));
                acc += f.buoyancy * 0.5 * (s0[1] + s1[1]) - f.weight * 0.5 * (s0[0] + s1[0]);
                acc += f.wind[1] * 2.0;
            }
            if let Some(fc) = force {
                acc += 0.5 * (from_half(fc[c][k]) + from_half(fc[b][k]));
            }
            u[k] += acc * f.dt;
            if k != 1 {
                u[k] += (f.wind[k] - u[k]) * f.follow;
            }
            u[k] *= f.keep;
        }
        *v = pack(u);
    });
}

/// Un niveau de la multigrille : ses types, son inconnue, son second membre
pub struct Level {
    pub d: Dim,
    pub typ: Vec<u32>,
    pub x: Vec<f32>,
    pub b: Vec<f32>,
}

/// Le système de pression en un voxel fluide : (diagonale, somme des inconnues des voisins fluides)
#[inline]
fn stencil(x: &[f32], typ: &[u32], d: Dim, c: usize) -> (f32, f32) {
    let [cx, cy, cz] = d.at(c);
    let (mut diag, mut sum) = (0.0, 0.0);
    for o in DIRS {
        let (nx, ny, nz) = (cx + o[0], cy + o[1], cz + o[2]);
        match typ_at(typ, d, nx, ny, nz) {
            FLUID => {
                diag += 1.0;
                sum += x[d.idx(nx, ny, nz)];
            }
            OPEN => diag += 1.0,
            _ => {}
        }
    }
    (diag, sum)
}

/// Ce que vaut A·x en un voxel fluide
#[inline]
fn apply(x: &[f32], typ: &[u32], d: Dim, c: usize) -> f32 {
    let (diag, sum) = stencil(x, typ, d, c);
    diag * x[c] - sum
}

/// Un demi-balayage de Gauss-Seidel rouge-noir. `first` : l'inconnue part de zéro.
pub fn smooth(l: &mut Level, color: u32, first: bool) {
    let (d, typ, b) = (l.d, &l.typ, &l.b);
    let old = &l.x;
    let new: Vec<f32> = (0..d.cells())
        .into_par_iter()
        .map(|c| {
            let [x, y, z] = d.at(c);
            let mine = ((x + y + z) & 1) as u32 == color;
            if typ[c] != FLUID || (first && !mine) {
                return if first { 0.0 } else { old[c] };
            }
            if !mine {
                return old[c];
            }
            let (diag, sum) = if first {
                (stencil(old, typ, d, c).0, 0.0)
            } else {
                stencil(old, typ, d, c)
            };
            if diag > 0.0 { (b[c] + sum) / diag } else { 0.0 }
        })
        .collect();
    l.x = new;
}

/// Second membre du niveau grossier : le résidu du niveau fin, réuni par huit
pub fn restrict(fine: &Level, coarse: &mut Level) {
    let (d, dc) = (fine.d, coarse.d);
    let ct = &coarse.typ;
    coarse.b = (0..dc.cells())
        .into_par_iter()
        .map(|c| {
            if ct[c] != FLUID {
                return 0.0;
            }
            let [x, y, z] = dc.at(c);
            let mut sum = 0.0;
            for k in 0..8 {
                let f = d.idx(2 * x + (k & 1), 2 * y + ((k >> 1) & 1), 2 * z + (k >> 2));
                if fine.typ[f] == FLUID {
                    sum += fine.b[f] - apply(&fine.x, &fine.typ, d, f);
                }
            }
            0.5 * sum
        })
        .collect();
}

/// La correction du niveau grossier, ajoutée au niveau fin
pub fn prolong(coarse: &Level, fine: &mut Level) {
    let (d, dc) = (fine.d, coarse.d);
    let typ = &fine.typ;
    fine.x.par_iter_mut().enumerate().for_each(|(c, x)| {
        if typ[c] == FLUID {
            let [fx, fy, fz] = d.at(c);
            *x += coarse.x[dc.idx(fx / 2, fy / 2, fz / 2)];
        }
    });
}

/// Un cycle en V à partir du niveau `l` : `x ≈ A⁻¹ b`, en partant de zéro. Symétrique (rouge
/// puis noir à la descente, noir puis rouge à la remontée) : c'est ce qu'exige le gradient conjugué.
pub fn vcycle(levels: &mut [Level], l: usize) {
    if l + 1 == levels.len() {
        for k in 0..COARSE_SWEEPS {
            smooth(&mut levels[l], 0, k == 0);
            smooth(&mut levels[l], 1, false);
        }
        for _ in 0..COARSE_SWEEPS {
            smooth(&mut levels[l], 1, false);
            smooth(&mut levels[l], 0, false);
        }
        return;
    }
    for k in 0..SWEEPS {
        smooth(&mut levels[l], 0, k == 0);
        smooth(&mut levels[l], 1, false);
    }
    let (a, b) = levels.split_at_mut(l + 1);
    restrict(&a[l], &mut b[0]);
    vcycle(levels, l + 1);
    let (a, b) = levels.split_at_mut(l + 1);
    prolong(&b[0], &mut a[l]);
    for _ in 0..SWEEPS {
        smooth(&mut levels[l], 1, false);
        smooth(&mut levels[l], 0, false);
    }
}

/// Ce qu'a donné la dernière résolution de pression
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    /// Plus grande divergence avant et après la résolution (par seconde)
    pub before: f32,
    pub after: f32,
    pub iterations: usize,
}

/// L'état du solveur sur le processeur
pub struct Cpu {
    pub d: Dim,
    pub origin: [f32; 3],
    pub h: f32,
    pub g: Grids,
    pub levels: Vec<Level>,
    /// Pression (gardée d'un pas à l'autre), résidu, direction
    pub p: Vec<f32>,
    pub r: Vec<f32>,
    pub s: Vec<f32>,
    typed_for: Option<Params>,
    pub stats: Stats,
}

impl Cpu {
    pub fn new(res: [usize; 3], origin: [f32; 3], h: f32) -> Cpu {
        let d = Dim::of(res);
        let levels = d.levels().into_iter().map(|d| Level { d, typ: vec![], x: vec![0.0; d.cells()], b: vec![0.0; d.cells()] }).collect();
        Cpu { d, origin, h, g: Grids::zero(d.cells()), levels, p: vec![0.0; d.cells()], r: vec![0.0; d.cells()], s: vec![0.0; d.cells()], typed_for: None, stats: Stats::default() }
    }

    /// Refait les types des voxels si les obstacles ou les bords ont changé
    pub fn set_types(&mut self, par: &Params) {
        let same = self.typed_for.as_ref().is_some_and(|o| o.borders == par.borders && o.ball == par.ball && o.cube == par.cube);
        if same {
            return;
        }
        self.levels[0].typ = cell_types(self.d, self.origin, self.h, par);
        for l in 1..self.levels.len() {
            self.levels[l].typ = coarsen_types(&self.levels[l - 1].typ, self.levels[l - 1].d, self.levels[l].d);
        }
        self.typed_for = Some(*par);
    }

    /// Résidu de départ : ce que la divergence devrait être, moins ce qu'elle est, moins A·p
    pub fn residual(&mut self) {
        let (d, h) = (self.d, self.h);
        let (typ, vel, p) = (&self.levels[0].typ, &self.g.vel, &self.p);
        self.r = (0..d.cells())
            .into_par_iter()
            .map(|c| {
                if typ[c] != FLUID {
                    return 0.0;
                }
                let [x, y, z] = d.at(c);
                let v = unpack(vel[c]);
                let out = from_half(vel[d.idx(x + 1, y, z)][0]) - v[0] + from_half(vel[d.idx(x, y + 1, z)][1]) - v[1] + from_half(vel[d.idx(x, y, z + 1)][2]) - v[2];
                v[3] * h - out - apply(p, typ, d, c)
            })
            .collect();
    }

    fn max_r(&self) -> f32 {
        self.r.par_iter().map(|x| x.abs()).reduce(|| 0.0, f32::max)
    }

    /// z = M⁻¹ r : un cycle en V, le résultat dans `levels[0].x`
    fn precondition(&mut self) {
        self.levels[0].b.copy_from_slice(&self.r);
        vcycle(&mut self.levels, 0);
    }

    fn dot_rz(&self) -> f32 {
        self.r.par_iter().zip(self.levels[0].x.par_iter()).map(|(a, b)| a * b).sum()
    }

    /// Résout la pression (MGPCG, `iters` itérations) puis projette la vitesse
    pub fn pressure(&mut self, iters: usize) {
        let d = self.d;
        self.residual();
        self.stats.before = self.max_r() / self.h;
        self.precondition();
        let mut rz = self.dot_rz();
        self.s.copy_from_slice(&self.levels[0].x);
        for it in 0..iters {
            let typ = &self.levels[0].typ;
            let s = &self.s;
            let q: Vec<f32> = (0..d.cells()).into_par_iter().map(|c| if typ[c] == FLUID { apply(s, typ, d, c) } else { 0.0 }).collect();
            let sq: f32 = s.par_iter().zip(q.par_iter()).map(|(a, b)| a * b).sum();
            let alpha = if sq > 1e-30 { rz / sq } else { 0.0 };
            self.p.par_iter_mut().zip(s.par_iter()).for_each(|(p, s)| *p += alpha * s);
            self.r.par_iter_mut().zip(q.par_iter()).for_each(|(r, q)| *r -= alpha * q);
            if it + 1 == iters {
                break;
            }
            self.precondition();
            let next = self.dot_rz();
            let beta = if rz > 1e-30 { next / rz } else { 0.0 };
            rz = next;
            let z = &self.levels[0].x;
            self.s.par_iter_mut().zip(z.par_iter()).for_each(|(s, z)| *s = z + beta * *s);
        }
        self.stats.after = self.max_r() / self.h;
        self.stats.iterations = iters;
        self.project();
    }

    /// La vitesse perd le gradient de la pression
    pub fn project(&mut self) {
        let d = self.d;
        let (typ, p) = (&self.levels[0].typ, &self.p);
        self.g.vel.par_iter_mut().enumerate().for_each(|(c, v)| {
            let [x, y, z] = d.at(c);
            let here = typ[c];
            let pc = if here == FLUID { p[c] } else { 0.0 };
            for k in 0..3 {
                let (bx, by, bz) = (x - (k == 0) as i32, y - (k == 1) as i32, z - (k == 2) as i32);
                let below = typ_at(typ, d, bx, by, bz);
                if here == SOLID || below == SOLID || (here != FLUID && below != FLUID) {
                    continue;
                }
                let pb = if below == FLUID { p[d.idx(bx, by, bz)] } else { 0.0 };
                v[k] = to_half(from_half(v[k]) - (pc - pb));
            }
        });
    }

    /// Plus grande divergence de la vitesse sur les voxels fluides, expansion des gaz déduite (par seconde)
    pub fn divergence(&self) -> f32 {
        let d = self.d;
        let (typ, vel) = (&self.levels[0].typ, &self.g.vel);
        (0..d.cells())
            .into_par_iter()
            .map(|c| {
                if typ[c] != FLUID {
                    return 0.0;
                }
                let [x, y, z] = d.at(c);
                let v = unpack(vel[c]);
                let out = from_half(vel[d.idx(x + 1, y, z)][0]) - v[0] + from_half(vel[d.idx(x, y + 1, z)][1]) - v[1] + from_half(vel[d.idx(x, y, z + 1)][2]) - v[2];
                (out / self.h - v[3]).abs()
            })
            .reduce(|| 0.0, f32::max)
    }

    /// Avance d'une image
    pub fn step(&mut self, dt: f32, par: &Params, ops: &[Op]) {
        self.set_types(par);
        let sub = par.substeps.clamp(1, 64);
        let (d, h) = (self.d, self.h);
        let burn = ops.iter().rev().find_map(|o| if let Op::Burn(b) = o { Some(*b) } else { None });
        for k in 0..sub {
            let dt = dt / sub as f32;
            let sca = advect_sca(&self.g, d, dt / h, par.maccormack);
            let vel = advect_vel(&self.g, d, dt / h, par.maccormack);
            self.g.sca = sca;
            self.g.vel = vel;
            for op in ops {
                if let Op::Emit(e) = op {
                    emit(&mut self.g, d, self.origin, h, &Emit { seed: e.seed.wrapping_add(k as u32), ..e.clone() }, 1.0 / sub as f32);
                }
            }
            react(&mut self.g, Some(&self.levels[0].typ), &React::new(par, burn.map(|b| Burn { dt, ..b }), dt));
            let force = (par.vorticity > 0.0).then(|| confine(&curl(&self.g.vel, d, 1.0 / h), d, par.vorticity * h));
            forces(&mut self.g, d, &self.levels[0].typ, force.as_deref(), &Forces::new(par, dt));
            self.pressure(par.iterations.max(1));
        }
    }
}

/// Fait sur le processeur les opérations en attente d'un volume que personne ne simule :
/// les sources, puis la combustion.
pub fn apply_ops(g: &mut Grids, res: [usize; 3], origin: [f32; 3], h: f32, ops: &[Op], scale: f32) {
    let d = Dim::of(res);
    for op in ops {
        match op {
            Op::Emit(e) => emit(g, d, origin, h, e, scale),
            Op::Burn(b) => react(g, None, &React { keep_density: 1.0, keep_heat: 1.0, dt: b.dt * scale, burn: Some(*b) }),
        }
    }
}

// ---------- l'état rendu par un solveur ----------

/// De quoi dessiner un volume depuis la carte : le tampon de la fumée, tel que le solveur l'écrit
#[derive(Clone)]
pub struct Live {
    pub sca: wgpu::Buffer,
    pub res: [usize; 3],
    epoch: Arc<AtomicU64>,
    at: u64,
}

impl Live {
    pub fn new(sca: wgpu::Buffer, res: [usize; 3], epoch: Arc<AtomicU64>) -> Live {
        let at = epoch.load(Ordering::Acquire);
        Live { sca, res, epoch, at }
    }
    /// Le tampon montre-t-il encore cet état ? Faux dès que le solveur a avancé.
    pub fn current(&self) -> bool {
        self.epoch.load(Ordering::Acquire) == self.at
    }
}

/// L'état d'une fumée à la fin d'un pas. Sur la carte, sa relecture est demandée mais pas
/// attendue : tant que personne n'appelle `grids`, rien n'est copié vers le processeur.
pub struct Shot {
    pub live: Option<Live>,
    cells: usize,
    pending: Mutex<Option<(crate::gpusim::Readback, crate::gpusim::Readback)>>,
    grids: OnceLock<Arc<Grids>>,
}

impl Shot {
    pub fn done(g: Grids) -> Shot {
        let s = Shot { live: None, cells: g.sca.len(), pending: Mutex::new(None), grids: OnceLock::new() };
        let _ = s.grids.set(Arc::new(g));
        s
    }

    pub fn waiting(live: Live, cells: usize, sca: crate::gpusim::Readback, vel: crate::gpusim::Readback) -> Shot {
        Shot { live: Some(live), cells, pending: Mutex::new(Some((sca, vel))), grids: OnceLock::new() }
    }

    /// `grids` rendrait-il la main sans attendre ? Ne bloque pas.
    pub fn ready(&self) -> bool {
        self.grids.get().is_some() || self.pending.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_none_or(|(a, b)| a.ready() && b.ready())
    }

    /// Attend la fin du calcul, sans rien copier
    pub fn wait(&self) {
        if let Some((a, b)) = self.pending.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            a.block();
            b.block();
        }
    }

    /// Les voxels. La première demande attend la carte et copie. Si la carte a échoué : un volume vide.
    pub fn grids(&self) -> Arc<Grids> {
        self.grids
            .get_or_init(|| {
                if std::env::var_os("ARY_DBG_RELECTURE").is_some() {
                    eprintln!("relecture d'un volume de fumée\n{}", std::backtrace::Backtrace::force_capture());
                }
                let got = self.pending.lock().unwrap_or_else(|e| e.into_inner()).take().and_then(|(a, b)| Some((a.take::<[u16; 4]>().ok()?, b.take::<[u16; 4]>().ok()?)));
                Arc::new(match got {
                    Some((sca, vel)) if sca.len() == self.cells && vel.len() == self.cells => Grids { sca, vel },
                    _ => Grids::zero(self.cells),
                })
            })
            .clone()
    }
}

/// Une fumée prête à avancer : sur la carte si elle y tient, sinon sur le processeur.
pub struct Sim {
    pub res: [usize; 3],
    pub origin: [f32; 3],
    pub voxel: f32,
    cpu: Option<Cpu>,
    gpu: Option<crate::gpusmoke::GpuSmoke>,
    /// Derniers chiffres connus de la pression, et durée du dernier pas fini
    stats: Stats,
    ms: f32,
}

impl Sim {
    pub fn new(res: [usize; 3], origin: [f32; 3], voxel: f32, on_gpu: bool) -> Sim {
        let gpu = if on_gpu { crate::gpusim::gpu().and_then(|g| crate::gpusmoke::GpuSmoke::new(g, res, origin, voxel)) } else { None };
        let cpu = gpu.is_none().then(|| Cpu::new(res, origin, voxel));
        Sim { res, origin, voxel, cpu, gpu, stats: Stats::default(), ms: 0.0 }
    }

    pub fn on_gpu(&self) -> bool {
        self.gpu.is_some()
    }

    /// Impose l'état (la pression gardée repart de zéro)
    pub fn load(&mut self, g: &Grids) {
        if let Some(gpu) = &mut self.gpu {
            gpu.load(g);
        }
        if let Some(cpu) = &mut self.cpu {
            cpu.g = g.clone();
            cpu.p.fill(0.0);
        }
    }

    /// La carte calcule-t-elle encore le pas d'avant ?
    pub fn busy(&mut self) -> bool {
        self.gpu.as_mut().is_some_and(|g| g.busy())
    }

    pub fn wait(&mut self) {
        if let Some(g) = &mut self.gpu {
            g.wait();
        }
    }

    /// Avance d'une image. Sur la carte, rend la main dès que le travail est envoyé.
    pub fn step(&mut self, dt: f32, par: &Params, ops: &[Op]) {
        let t0 = std::time::Instant::now();
        if let Some(g) = &mut self.gpu {
            g.step(dt, par, ops);
        }
        if let Some(c) = &mut self.cpu {
            c.step(dt, par, ops);
            self.stats = c.stats;
            self.ms = t0.elapsed().as_secs_f32() * 1000.0;
        }
    }

    /// L'état à la fin du dernier pas
    pub fn shot(&mut self) -> Arc<Shot> {
        if let Some(g) = &mut self.gpu {
            return g.shot();
        }
        Arc::new(Shot::done(self.cpu.as_ref().map(|c| c.g.clone()).unwrap_or_default()))
    }

    /// Chiffres de la pression et durée (ms) du dernier pas fini. Sur la carte, ils arrivent
    /// avec un pas de retard : on ne les attend jamais.
    pub fn stats(&mut self) -> (Stats, f32) {
        if let Some(g) = &mut self.gpu {
            if let Some(x) = g.stats() {
                (self.stats, self.ms) = x;
            }
        }
        (self.stats, self.ms)
    }

    /// Les mêmes chiffres, en attendant la carte (tests, banc)
    pub fn stats_now(&mut self) -> (Stats, f32) {
        self.wait();
        self.stats()
    }

    /// Octets tenus sur la carte (zéro sur le processeur)
    pub fn gpu_bytes(&self) -> u64 {
        self.gpu.as_ref().map(|g| g.bytes()).unwrap_or(0)
    }

    /// La carte a rendu une erreur : on continue sur le processeur, depuis l'état donné
    pub fn fall_back(&mut self, g: &Grids) {
        self.gpu = None;
        let mut c = Cpu::new(self.res, self.origin, self.voxel);
        c.g = g.clone();
        self.cpu = Some(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn puff(n: usize) -> Cpu {
        let mut c = Cpu::new([n, n, n], [0.0; 3], 1.0 / n as f32);
        let e = Emit { shape: Shape::Ball([0.5, 0.3, 0.5], 0.15), density: 1.0, temperature: 1.0, fuel: 0.0, velocity: [0.0; 3], add: false, soft: 0.0, noise: 0.0, seed: 0, dt: 1.0 / 24.0 };
        emit(&mut c.g, c.d, c.origin, c.h, &e, 1.0);
        c
    }

    #[test]
    fn la_pression_converge_vite() {
        // Une bouffée chaude : une image de flottabilité, puis la pression. Chaque itération
        // doit diviser la divergence : c'est ce qui distingue la multigrille d'un Jacobi.
        let mut c = puff(32);
        let par = Params::default();
        c.set_types(&par);
        forces(&mut c.g, c.d, &c.levels[0].typ.clone(), None, &Forces::new(&par, 1.0 / 24.0));
        let start = c.g.clone();
        let mut last = f32::MAX;
        for iters in [1, 2, 4, 8] {
            c.g = start.clone();
            c.p.fill(0.0);
            c.pressure(iters);
            println!("{iters} itérations : divergence {:.2e} → {:.2e}", c.stats.before, c.stats.after);
            assert!(c.stats.after < last * 0.6, "{iters} : {} contre {last}", c.stats.after);
            last = c.stats.after;
        }
        assert!(c.stats.after < c.stats.before * 1e-4, "{:?}", c.stats);
        // La divergence mesurée sur la vitesse projetée est celle que le solveur annonce (à la
        // demi-précision des vitesses près)
        assert!(c.divergence() < c.stats.before * 2e-3, "{} {:?}", c.divergence(), c.stats);
    }

    #[test]
    fn la_pression_avec_un_obstacle_et_des_bords_fermes() {
        let mut c = puff(32);
        let par = Params { ball: Some(([0.5, 0.6, 0.5], 0.12)), cube: Some(([0.3, 0.5, 0.5], [0.05, 0.2, 0.2])), borders: 2, ..Default::default() };
        c.set_types(&par);
        forces(&mut c.g, c.d, &c.levels[0].typ.clone(), None, &Forces::new(&par, 1.0 / 24.0));
        c.pressure(8);
        assert!(c.stats.after < c.stats.before * 1e-3, "{:?}", c.stats);
        // Rien ne traverse un solide
        let t = &c.levels[0].typ;
        for i in 0..c.d.cells() {
            let [x, y, z] = c.d.at(i);
            if t[i] == SOLID {
                assert_eq!(c.g.vel[i][..3], [0; 3]);
                assert_eq!(c.g.vel[c.d.idx(x + 1, y, z)][0], 0);
                assert_eq!(c.g.vel[c.d.idx(x, y + 1, z)][1], 0);
            }
        }
    }

    #[test]
    fn la_fumee_monte_et_reste_finie() {
        let mut c = puff(32);
        let par = Params::default();
        let height = |c: &Cpu| {
            let (mut m, mut s) = (0.0f64, 0.0f64);
            for (i, v) in c.g.sca.iter().enumerate() {
                let w = from_half(v[0]) as f64;
                m += w;
                s += w * c.d.at(i)[1] as f64;
            }
            (s / m.max(1e-9), m)
        };
        let (h0, m0) = height(&c);
        for _ in 0..12 {
            c.step(1.0 / 24.0, &par, &[]);
        }
        let (h1, m1) = height(&c);
        assert!(h1 > h0 + 1.0, "la fumée chaude doit monter : {h0} → {h1}");
        assert!(m1 > m0 * 0.5 && m1 < m0 * 1.2, "masse {m0} → {m1}");
        assert!(c.g.vel.iter().all(|v| v.iter().all(|x| from_half(*x).is_finite())));
        assert!(c.stats.after < 0.05, "{:?}", c.stats);
    }

    #[test]
    fn le_carburant_brule_et_chauffe() {
        let n = 16;
        let mut g = Grids::zero(n * n * n);
        let d = Dim::of([n, n, n]);
        let e = Emit { shape: Shape::Cube([0.5, 0.5, 0.5], [0.2, 0.2, 0.2]), density: 0.0, temperature: 1.0, fuel: 1.0, velocity: [0.0, 1.0, 0.0], add: false, soft: 0.0, noise: 0.0, seed: 0, dt: 0.1 };
        emit(&mut g, d, [0.0; 3], 1.0 / n as f32, &e, 1.0);
        let c = d.idx(8, 8, 8);
        assert_eq!(unpack(g.sca[c])[2], 1.0);
        assert!(unpack(g.vel[c])[1] > 0.9);
        let b = Burn { ignition: 0.5, rate: 2.0, heat: 3.0, smoke: 0.5, expansion: 1.0, cooling: 0.0, dt: 0.1 };
        react(&mut g, None, &React { keep_density: 1.0, keep_heat: 1.0, dt: 0.1, burn: Some(b) });
        let s = unpack(g.sca[c]);
        assert!((s[2] - 0.8).abs() < 1e-3 && (s[1] - 1.6).abs() < 1e-3 && (s[0] - 0.1).abs() < 1e-3 && s[3] > 0.99, "{s:?}");
        assert!(unpack(g.vel[c])[3] > 1.9);
        // Un voxel froid ne brûle pas
        assert_eq!(unpack(g.sca[d.idx(1, 1, 1)])[3], 0.0);
    }
}
