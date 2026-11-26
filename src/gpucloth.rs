//! Tissu en « Vertex Block Descent » (Chen, Macklin, Kim, Yuksel 2024), sur carte graphique, avec
//! sa version processeur qui sert de repli et de référence.
//!
//! Le pas de temps implicite est une minimisation : pour chaque sommet, on cherche la position qui
//! équilibre son inertie et les forces de ses voisins. VBD le fait un sommet à la fois, par un pas
//! de Newton sur un bloc 3×3, les voisins étant figés. Deux sommets sans lien entre eux peuvent
//! être traités en même temps : on colorie donc les sommets au chargement, et chaque couleur est
//! une passe sur la carte.
//!
//! Énergies :
//! - membrane : un ressort par arête (raideur en N/m) ;
//! - flexion : pour chaque sommet et chaque paire d'arêtes opposées (i au milieu de j et k),
//!   ½·D·|(1−t)·xj + t·xk − xi|². Nulle quand les trois points sont alignés, quadratique, donc sa
//!   dérivée seconde est constante : c'est la moins chère des flexions pour ce solveur.
//!
//! Ce qui garde la mémoire sobre (c'est elle qui borne la vitesse) :
//! - les sommets sont rangés par couleur, puis le long d'une courbe de Morton : une passe lit une
//!   plage contiguë, et ses voisins sont proches en mémoire ;
//! - le voisinage est une liste par sommet (rassemblement, aucune écriture concurrente) : 8 octets
//!   par ressort, 16 par terme de flexion ;
//! - les positions sont relatives au centre du tissu : en f32, un tissu posé à 100 m de l'origine
//!   garde la précision d'un tissu posé à l'origine.
//!
//! Accélération de Tchebychev entre les itérations (sauf sur les sommets au contact, où elle est
//! instable). Obstacles : sol, boule, boîte, résolus comme des contraintes dans le bloc du sommet.
//! L'auto-collision n'est pas faite.

use crate::gpusim::{self, Bind, Column, Gpu, Kernels};
use rayon::prelude::*;

/// Masse surfacique prise quand la géométrie ne donne pas de masse (kg/m², un coton léger)
pub const DENSITY: f32 = 0.2;

/// Rayon spectral supposé pour l'accélération de Tchebychev
pub const RHO: f32 = 0.9;

/// Réglages d'un pas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// Raideur des arêtes, en N/m
    pub stiffness: f32,
    /// Raideur de flexion, en N·m
    pub bending: f32,
    /// Part de vitesse perdue par image (0 : aucune)
    pub damping: f32,
    /// Passes sur tous les sommets, par image (réparties entre les sous-pas)
    pub iterations: usize,
    pub substeps: usize,
    /// Accélération de Tchebychev : rayon spectral supposé (0 : sans)
    pub rho: f32,
    pub floor: Option<f32>,
    /// Boule : centre, rayon
    pub ball: Option<([f32; 3], f32)>,
    /// Boîte : centre, demi-tailles
    pub cube: Option<([f32; 3], [f32; 3])>,
    /// Adhérence au contact (0 : glisse, 1 : colle)
    pub grip: f32,
}

impl Default for Params {
    fn default() -> Self {
        Params { stiffness: 1000.0, bending: 1e-4, damping: 0.02, iterations: 16, substeps: 8, rho: RHO, floor: None, ball: None, cube: None, grip: 0.6 }
    }
}

impl Params {
    /// Sous-pas et itérations par sous-pas réellement faits
    pub fn split(&self) -> (usize, usize) {
        let sub = self.substeps.clamp(1, 256);
        (sub, self.iterations.max(1).div_ceil(sub).min(MAX_PER))
    }

    /// Rayon spectral réellement pris pour l'accélération. Avec peu d'itérations par sous-pas,
    /// l'extrapolation n'a pas le temps de se stabiliser et le tissu explose : on la bride, et on
    /// s'en passe en dessous de quatre itérations.
    pub fn rho_used(&self) -> f32 {
        let per = self.split().1;
        if per < 4 || self.rho <= 0.0 { 0.0 } else { self.rho.min(1.0 - 2.4 / per as f32).clamp(0.0, 0.98) }
    }
}

/// Les réglages tels que les noyaux les lisent (mêmes champs sur la carte et sur le processeur).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Consts {
    h: f32,
    inv_h2: f32,
    k: f32,
    d: f32,
    keep: f32,
    slide: f32,
    floor_y: f32,
    floor_on: f32,
    /// Centre, rayon (0 : pas de boule)
    ball: [f32; 4],
    /// Centre, puis 1 si la boîte existe
    cube_c: [f32; 4],
    cube_h: [f32; 4],
}

impl Consts {
    fn new(p: &Params, dt: f32, origin: [f32; 3]) -> Consts {
        let (sub, _) = p.split();
        let h = dt / sub as f32;
        let rel = |c: [f32; 3]| [c[0] - origin[0], c[1] - origin[1], c[2] - origin[2]];
        let mut c = Consts { h, inv_h2: 1.0 / (h * h), k: p.stiffness.max(0.0), d: p.bending.max(0.0), keep: (1.0 - p.damping).clamp(0.0, 1.0).powf(1.0 / sub as f32), slide: 1.0 - p.grip.clamp(0.0, 1.0), ..Default::default() };
        if let Some(y) = p.floor {
            c.floor_y = y - origin[1];
            c.floor_on = 1.0;
        }
        if let Some((ctr, r)) = p.ball.filter(|b| b.1 > 0.0) {
            let q = rel(ctr);
            c.ball = [q[0], q[1], q[2], r];
        }
        if let Some((ctr, half)) = p.cube.filter(|b| b.1.iter().all(|h| *h > 0.0)) {
            let q = rel(ctr);
            c.cube_c = [q[0], q[1], q[2], 1.0];
            c.cube_h = [half[0], half[1], half[2], 0.0];
        }
        c
    }
}

/// Poids de Tchebychev de l'itération `k` (à partir de 1), connaissant le précédent.
fn omega(k: usize, prev: f32, rho: f32) -> f32 {
    match k {
        0 | 1 => 1.0,
        2 => 2.0 / (2.0 - rho * rho),
        _ => 4.0 / (4.0 - rho * rho * prev),
    }
}

// ---------- topologie ----------

/// Tout ce qui ne dépend que du maillage au repos : ordre des sommets, couleurs, voisinages.
/// Construit une fois au chargement ; les mêmes tableaux servent au processeur et à la carte.
pub struct Topo {
    pub n: usize,
    /// Rang d'origine du sommet rangé en `k`
    pub perm: Vec<u32>,
    /// Rang de rangement du sommet d'origine `i`
    #[allow(dead_code)]
    pub inv: Vec<u32>,
    /// Plage de chaque couleur : début, nombre. Les sommets fixes sont après la dernière.
    pub colors: Vec<[u32; 2]>,
    /// Par sommet (n + 1 cases) : début de ses ressorts, début de ses termes de flexion
    pub off: Vec<[u32; 2]>,
    /// Ressort : voisin, longueur au repos (bits d'un f32)
    pub springs: Vec<[u32; 2]>,
    /// Flexion : deux autres sommets a et b, poids du sommet lui-même et de a (bits de f32). Le poids de b est l'opposé de leur somme.
    pub bends: Vec<[u32; 4]>,
    /// Masse (0 : sommet fixe)
    pub mass: Vec<f32>,
}

fn part1by2(x: u32) -> u32 {
    let mut x = x & 0x3ff;
    x = (x | (x << 16)) & 0x030000ff;
    x = (x | (x << 8)) & 0x0300f00f;
    x = (x | (x << 4)) & 0x030c30c3;
    (x | (x << 2)) & 0x09249249
}

/// Liste de voisins par sommet à partir de paires orientées : (débuts, voisins).
fn csr<T: Copy + Default>(n: usize, pairs: impl Iterator<Item = (u32, T)> + Clone) -> (Vec<u32>, Vec<T>) {
    let mut start = vec![0u32; n + 1];
    for (a, _) in pairs.clone() {
        start[a as usize + 1] += 1;
    }
    for i in 0..n {
        start[i + 1] += start[i];
    }
    let mut cur = start.clone();
    let mut out = vec![T::default(); start[n] as usize];
    for (a, v) in pairs {
        out[cur[a as usize] as usize] = v;
        cur[a as usize] += 1;
    }
    (start, out)
}

impl Topo {
    /// `pos` : positions au repos. `rest` : longueur au repos de chaque arête. `mass` : masse par
    /// sommet si la géométrie en donne une, sinon elle est déduite de la taille des mailles.
    pub fn build(n: usize, edges: &[[u32; 2]], rest: &[f32], pos: &[Vec<f32>; 3], pinned: &[bool], mass: Option<&[f32]>) -> Topo {
        let at = |i: u32| [pos[0][i as usize], pos[1][i as usize], pos[2][i as usize]];
        let ok = |e: &[u32; 2]| (e[0] as usize) < n && (e[1] as usize) < n && e[0] != e[1];
        // Arêtes autour de chaque sommet
        let around = edges.iter().zip(rest).filter(|(e, _)| ok(e)).flat_map(|(e, r)| [(e[0], (e[1], *r)), (e[1], (e[0], *r))]);
        let (nb_start, nb) = csr(n, around);
        let ring = |i: usize| &nb[nb_start[i] as usize..nb_start[i + 1] as usize];
        // Flexion : paires d'arêtes opposées autour d'un sommet (i au milieu de j et k)
        struct Trip {
            v: [u32; 3],
            t: f32,
            s: f32,
        }
        let mut trips = Vec::new();
        for i in 0..n {
            let (r, pi) = (ring(i), at(i as u32));
            for (a, &(j, lj)) in r.iter().enumerate() {
                for &(k, lk) in &r[a + 1..] {
                    if j == k || lj < 1e-9 || lk < 1e-9 {
                        continue;
                    }
                    let (pj, pk) = (at(j), at(k));
                    let (dj, dk) = ([pj[0] - pi[0], pj[1] - pi[1], pj[2] - pi[2]], [pk[0] - pi[0], pk[1] - pi[1], pk[2] - pi[2]]);
                    let dot = dj[0] * dk[0] + dj[1] * dk[1] + dj[2] * dk[2];
                    let len = ((dj[0] * dj[0] + dj[1] * dj[1] + dj[2] * dj[2]) * (dk[0] * dk[0] + dk[1] * dk[1] + dk[2] * dk[2])).sqrt();
                    if len < 1e-18 || dot / len > -0.95 {
                        continue;
                    }
                    // Courbure ≈ 2·|c| / (lj·lk), sur une bande de largeur et de longueur ≈ (lj + lk) / 2.
                    // Chaque direction du tissu est comptée deux fois (droites et diagonales) : d'où le ½.
                    let s = (lj + lk) / (std::f32::consts::SQRT_2 * lj * lk);
                    trips.push(Trip { v: [i as u32, j, k], t: lj / (lj + lk), s });
                }
            }
        }
        // Couleurs : deux sommets liés par un ressort ou par un terme de flexion n'ont jamais la même
        let links = edges.iter().filter(|e| ok(e)).flat_map(|e| [(e[0], e[1]), (e[1], e[0])]).chain(trips.iter().flat_map(|t| [(t.v[1], t.v[2]), (t.v[2], t.v[1])]));
        let (cp_start, cp) = csr(n, links);
        const FIXED: u32 = u32::MAX;
        let mut color = vec![FIXED; n];
        let mut count = 0u32;
        for i in 0..n {
            if pinned.get(i).copied().unwrap_or(false) {
                continue;
            }
            let mut used = 0u128;
            for &j in &cp[cp_start[i] as usize..cp_start[i + 1] as usize] {
                if color[j as usize] < 128 {
                    used |= 1 << color[j as usize];
                }
            }
            color[i] = (!used).trailing_zeros().min(127);
            count = count.max(color[i] + 1);
        }
        // Rangement : par couleur, puis le long d'une courbe de Morton
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for c in 0..3 {
            for &x in &pos[c][..n] {
                lo[c] = lo[c].min(x);
                hi[c] = hi[c].max(x);
            }
        }
        let cell = |x: f32, c: usize| if hi[c] > lo[c] { ((x - lo[c]) / (hi[c] - lo[c]) * 1023.0) as u32 } else { 0 };
        let mut order: Vec<(u32, u32, u32)> = (0..n as u32)
            .map(|i| {
                let p = at(i);
                (color[i as usize], part1by2(cell(p[0], 0)) | part1by2(cell(p[1], 1)) << 1 | part1by2(cell(p[2], 2)) << 2, i)
            })
            .collect();
        order.sort_unstable();
        let perm: Vec<u32> = order.iter().map(|o| o.2).collect();
        let mut inv = vec![0u32; n];
        for (k, &i) in perm.iter().enumerate() {
            inv[i as usize] = k as u32;
        }
        let mut colors = vec![[0u32; 2]; count as usize];
        for (k, o) in order.iter().enumerate() {
            if o.0 != FIXED {
                let c = &mut colors[o.0 as usize];
                if c[1] == 0 {
                    c[0] = k as u32;
                }
                c[1] += 1;
            }
        }
        // Voisinages, dans l'ordre de rangement
        let (tr_start, tr) = csr(n, trips.iter().enumerate().flat_map(|(ti, t)| [(t.v[0], (ti as u32, 0u8)), (t.v[1], (ti as u32, 1)), (t.v[2], (ti as u32, 2))]));
        let mut off = Vec::with_capacity(n + 1);
        let mut springs = Vec::with_capacity(nb.len());
        let mut bends = Vec::with_capacity(tr.len());
        let mut m = Vec::with_capacity(n);
        for &i in &perm {
            let i = i as usize;
            off.push([springs.len() as u32, bends.len() as u32]);
            let mut shortest = f32::MAX;
            for &(j, r) in ring(i) {
                springs.push([inv[j as usize], r.to_bits()]);
                shortest = shortest.min(r);
            }
            for &(ti, role) in &tr[tr_start[i] as usize..tr_start[i + 1] as usize] {
                let t = &trips[ti as usize];
                // Poids des trois sommets dans c = (1−t)·xj + t·xk − xi, mis à l'échelle
                let w = [-t.s, (1.0 - t.t) * t.s, t.t * t.s];
                let (a, b) = match role {
                    0 => (1, 2),
                    1 => (0, 2),
                    _ => (0, 1),
                };
                bends.push([inv[t.v[a] as usize], inv[t.v[b] as usize], w[role as usize].to_bits(), w[a].to_bits()]);
            }
            m.push(if color[i] == FIXED {
                0.0
            } else {
                match mass {
                    Some(mm) => mm[i].max(1e-9),
                    None if shortest < f32::MAX => (DENSITY * shortest * shortest).max(1e-9),
                    None => 1.0,
                }
            });
        }
        off.push([springs.len() as u32, bends.len() as u32]);
        Topo { n, perm, inv, colors, off, springs, bends, mass: m }
    }

    /// Octets que la carte garde pour ce tissu
    pub fn gpu_bytes(&self) -> u64 {
        (self.n * 16 * 6 + self.off.len() * 8 + self.springs.len() * 8 + self.bends.len() * 16) as u64
    }

    /// Octets lus et écrits par passe complète sur tous les sommets (pour estimer le débit)
    pub fn bytes_per_iteration(&self) -> u64 {
        // Par sommet : ses réglages (cible, précédent, débuts), sa position lue et écrite, et pour
        // chaque ressort ou terme de flexion l'entrée de la liste et les positions voisines
        (self.n * (16 + 16 + 16 + 32) + self.springs.len() * (8 + 16) + self.bends.len() * (16 + 32) + self.n * 64) as u64
    }
}

// ---------- noyaux, version processeur ----------

type V4 = [f32; 4];

/// Position d'un sommet après obstacles, et 1 s'il en touche un. `pv` : sa position au début du sous-pas.
fn collide(mut x: [f32; 3], pv: [f32; 3], c: &Consts) -> V4 {
    let mut hit = 0.0;
    if c.floor_on > 0.5 && x[1] < c.floor_y {
        x[1] = c.floor_y;
        x[0] = pv[0] + (x[0] - pv[0]) * c.slide;
        x[2] = pv[2] + (x[2] - pv[2]) * c.slide;
        hit = 1.0;
    }
    if c.ball[3] > 0.0 {
        let (ctr, r) = ([c.ball[0], c.ball[1], c.ball[2]], c.ball[3]);
        let d = [x[0] - ctr[0], x[1] - ctr[1], x[2] - ctr[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if len < r && len > 1e-9 {
            let nrm = [d[0] / len, d[1] / len, d[2] / len];
            let mv = [x[0] - pv[0], x[1] - pv[1], x[2] - pv[2]];
            let along = mv[0] * nrm[0] + mv[1] * nrm[1] + mv[2] * nrm[2];
            for k in 0..3 {
                // Sorti de la boule, puis freiné le long de sa surface
                x[k] = ctr[k] + nrm[k] * r - (mv[k] - along * nrm[k]) * (1.0 - c.slide);
            }
            // Le frein ne doit pas le faire rentrer
            let d = [x[0] - ctr[0], x[1] - ctr[1], x[2] - ctr[2]];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if len < r && len > 1e-9 {
                for k in 0..3 {
                    x[k] = ctr[k] + d[k] / len * r;
                }
            }
            hit = 1.0;
        }
    }
    if c.cube_c[3] > 0.5 {
        let q = [x[0] - c.cube_c[0], x[1] - c.cube_c[1], x[2] - c.cube_c[2]];
        let pen = [c.cube_h[0] - q[0].abs(), c.cube_h[1] - q[1].abs(), c.cube_h[2] - q[2].abs()];
        if pen[0] > 0.0 && pen[1] > 0.0 && pen[2] > 0.0 {
            // Sorti par la face la plus proche, freiné le long de cette face
            let axis = if pen[0] <= pen[1] && pen[0] <= pen[2] {
                0
            } else if pen[1] <= pen[2] {
                1
            } else {
                2
            };
            for k in 0..3 {
                x[k] = pv[k] + (x[k] - pv[k]) * c.slide;
            }
            x[axis] = c.cube_c[axis] + if q[axis] < 0.0 { -c.cube_h[axis] } else { c.cube_h[axis] };
            hit = 1.0;
        }
    }
    [x[0], x[1], x[2], hit]
}

/// Le cœur de VBD : nouvelle position du sommet `i`, ses voisins étant figés. `at` lit la position d'un voisin.
fn solve_vertex(t: &Topo, i: usize, xi: [f32; 3], tgt: V4, pv: [f32; 3], c: &Consts, at: impl Fn(u32) -> [f32; 3]) -> V4 {
    let m = tgt[3] * c.inv_h2;
    // Force et dérivée seconde (symétrique : diagonale, puis xy, xz, yz)
    let mut f = [-m * (xi[0] - tgt[0]), -m * (xi[1] - tgt[1]), -m * (xi[2] - tgt[2])];
    let mut hd = [m; 3];
    let mut ho = [0.0f32; 3];
    let (o0, o1) = (t.off[i], t.off[i + 1]);
    for sp in &t.springs[o0[0] as usize..o1[0] as usize] {
        let xj = at(sp[0]);
        let d = [xi[0] - xj[0], xi[1] - xj[1], xi[2] - xj[2]];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if l > 1e-12 {
            let rest = f32::from_bits(sp[1]);
            let nrm = [d[0] / l, d[1] / l, d[2] / l];
            let pull = c.k * (l - rest);
            // Un ressort comprimé a une dérivée seconde négative en travers : on la coupe à zéro
            let iso = c.k * (1.0 - rest / l).max(0.0);
            let ani = c.k - iso;
            for k in 0..3 {
                f[k] -= pull * nrm[k];
                hd[k] += iso + ani * nrm[k] * nrm[k];
            }
            ho[0] += ani * nrm[0] * nrm[1];
            ho[1] += ani * nrm[0] * nrm[2];
            ho[2] += ani * nrm[1] * nrm[2];
        }
    }
    for b in &t.bends[o0[1] as usize..o1[1] as usize] {
        let (ws, wa) = (f32::from_bits(b[2]), f32::from_bits(b[3]));
        let wb = -(ws + wa);
        let (xa, xb) = (at(b[0]), at(b[1]));
        let g = c.d * ws;
        for k in 0..3 {
            f[k] -= g * (ws * xi[k] + wa * xa[k] + wb * xb[k]);
            hd[k] += g * ws;
        }
    }
    // Pas de Newton : résolution du bloc 3×3, mis à l'échelle pour rester loin des bornes du f32
    let s = 1.0 / (hd[0] + hd[1] + hd[2]);
    let (a, b, cc, d, e, g) = (hd[0] * s, ho[0] * s, ho[1] * s, hd[1] * s, ho[2] * s, hd[2] * s);
    let f = [f[0] * s, f[1] * s, f[2] * s];
    let (c00, c01, c02) = (d * g - e * e, cc * e - b * g, b * e - cc * d);
    let (c11, c12, c22) = (a * g - cc * cc, b * cc - a * e, a * d - b * b);
    let det = a * c00 + b * c01 + cc * c02;
    let mut x = xi;
    if det > 1e-12 {
        let r = 1.0 / det;
        x[0] += r * (c00 * f[0] + c01 * f[1] + c02 * f[2]);
        x[1] += r * (c01 * f[0] + c11 * f[1] + c12 * f[2]);
        x[2] += r * (c02 * f[0] + c12 * f[1] + c22 * f[2]);
    }
    collide(x, pv, c)
}

/// Un pas d'image sur le processeur. `pos` et `vel` sont dans l'ordre de rangement, positions
/// relatives à l'origine du tissu. Même suite d'opérations que sur la carte.
fn step_cpu(t: &Topo, pos: &mut [V4], vel: &mut [V4], c: &Consts, p: &Params) {
    let n = t.n;
    let (sub, per) = p.split();
    let rho = p.rho_used();
    let mut tgt: Vec<V4> = t.mass.iter().map(|m| [0.0, 0.0, 0.0, *m]).collect();
    let mut prev = vec![[0.0f32; 4]; n];
    let mut s = [vec![[0.0f32; 4]; n], vec![[0.0f32; 4]; n]];
    for _ in 0..sub {
        // Prédiction : où l'inertie seule mènerait chaque sommet
        (pos.par_iter_mut(), vel.par_iter(), tgt.par_iter_mut(), prev.par_iter_mut(), s[0].par_iter_mut()).into_par_iter().with_min_len(1024).for_each(|(x, v, y, pv, s0)| {
            let m = y[3];
            *pv = [x[0], x[1], x[2], 0.0];
            let q = if m > 0.0 { [x[0] + c.h * v[0], x[1] + c.h * v[1], x[2] + c.h * v[2]] } else { [x[0], x[1], x[2]] };
            *y = [q[0], q[1], q[2], m];
            *x = [q[0], q[1], q[2], if m > 0.0 { 0.0 } else { 1.0 }];
            *s0 = *x;
        });
        let mut w = 1.0;
        for k in 1..=per {
            for col in &t.colors {
                let (start, count) = (col[0] as usize, col[1] as usize);
                // Les voisins d'un sommet sont tous d'une autre couleur : on les lit pendant qu'on écrit celle-ci
                let (lo, rest) = pos.split_at_mut(start);
                let (mid, hi) = rest.split_at_mut(count);
                let (lo, hi) = (&*lo, &*hi);
                // Un petit lot par fil au moins : sur un petit tissu, se partager le travail coûterait plus que le faire
                mid.par_iter_mut().with_min_len(256).enumerate().for_each(|(q, x)| {
                    let i = start + q;
                    let at = |j: u32| {
                        let j = j as usize;
                        let v = if j < start { lo[j] } else { hi[j - start - count] };
                        [v[0], v[1], v[2]]
                    };
                    *x = solve_vertex(t, i, [x[0], x[1], x[2]], tgt[i], [prev[i][0], prev[i][1], prev[i][2]], c, at);
                });
            }
            if rho > 0.0 {
                w = omega(k, w, rho);
                pos.par_iter_mut().zip(s[k % 2].par_iter_mut()).with_min_len(1024).for_each(|(x, o)| {
                    if x[3] < 0.5 {
                        for a in 0..3 {
                            x[a] = o[a] + w * (x[a] - o[a]);
                        }
                    }
                    *o = [x[0], x[1], x[2], 0.0];
                });
            }
        }
        (pos.par_iter(), vel.par_iter_mut(), prev.par_iter(), tgt.par_iter()).into_par_iter().with_min_len(1024).for_each(|(x, v, pv, y)| {
            *v = if y[3] > 0.0 { [(x[0] - pv[0]) / c.h * c.keep, (x[1] - pv[1]) / c.h * c.keep, (x[2] - pv[2]) / c.h * c.keep, 0.0] } else { [0.0; 4] };
        });
    }
}

// ---------- noyaux, version carte ----------

const STRUCTS: &str = r#"
struct C {
    h: f32, inv_h2: f32, k: f32, d: f32,
    keep: f32, slide: f32, floor_y: f32, floor_on: f32,
    ball: vec4<f32>,
    cube_c: vec4<f32>,
    cube_h: vec4<f32>,
}
struct S { start: u32, count: u32, omega: f32, pad: u32 }
"#;

/// Le pas de Newton par sommet, une couleur à la fois.
const SOLVE: &str = r#"
@group(0) @binding(0) var<uniform> c: C;
@group(0) @binding(1) var<uniform> s: S;
@group(0) @binding(2) var<storage, read_write> pos: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> tgt: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> prev: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read> off: array<vec2<u32>>;
@group(0) @binding(6) var<storage, read> springs: array<vec2<u32>>;
@group(0) @binding(7) var<storage, read> bends: array<vec4<u32>>;

fn collide(x0: vec3<f32>, pv: vec3<f32>) -> vec4<f32> {
    var x = x0;
    var hit = 0.0;
    if (c.floor_on > 0.5 && x.y < c.floor_y) {
        x.y = c.floor_y;
        x.x = pv.x + (x.x - pv.x) * c.slide;
        x.z = pv.z + (x.z - pv.z) * c.slide;
        hit = 1.0;
    }
    if (c.ball.w > 0.0) {
        let ctr = c.ball.xyz;
        let r = c.ball.w;
        let d = x - ctr;
        let len = length(d);
        if (len < r && len > 1e-9) {
            let nrm = d / len;
            let mv = x - pv;
            let along = dot(mv, nrm);
            x = ctr + nrm * r - (mv - along * nrm) * (1.0 - c.slide);
            let d2 = x - ctr;
            let len2 = length(d2);
            if (len2 < r && len2 > 1e-9) { x = ctr + d2 / len2 * r; }
            hit = 1.0;
        }
    }
    if (c.cube_c.w > 0.5) {
        let q = x - c.cube_c.xyz;
        let pen = c.cube_h.xyz - abs(q);
        if (pen.x > 0.0 && pen.y > 0.0 && pen.z > 0.0) {
            let side = c.cube_c.xyz + select(c.cube_h.xyz, -c.cube_h.xyz, q < vec3<f32>(0.0));
            x = pv + (x - pv) * c.slide;
            if (pen.x <= pen.y && pen.x <= pen.z) { x.x = side.x; }
            else if (pen.y <= pen.z) { x.y = side.y; }
            else { x.z = side.z; }
            hit = 1.0;
        }
    }
    return vec4<f32>(x, hit);
}

@compute @workgroup_size(WG)
fn solve(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let q = lin(gid, nwg);
    if (q >= s.count) { return; }
    let i = s.start + q;
    let ty = tgt[i];
    let m = ty.w * c.inv_h2;
    let xi = pos[i].xyz;
    var f = -m * (xi - ty.xyz);
    var hd = vec3<f32>(m);
    var ho = vec3<f32>(0.0);
    let o0 = off[i];
    let o1 = off[i + 1u];
    for (var e = o0.x; e < o1.x; e++) {
        let sp = springs[e];
        let d = xi - pos[sp.x].xyz;
        let l = length(d);
        if (l > 1e-12) {
            let rest = bitcast<f32>(sp.y);
            let nrm = d / l;
            let iso = c.k * max(1.0 - rest / l, 0.0);
            let ani = c.k - iso;
            f -= c.k * (l - rest) * nrm;
            hd += vec3<f32>(iso) + ani * nrm * nrm;
            ho += ani * vec3<f32>(nrm.x * nrm.y, nrm.x * nrm.z, nrm.y * nrm.z);
        }
    }
    for (var e = o0.y; e < o1.y; e++) {
        let b = bends[e];
        let ws = bitcast<f32>(b.z);
        let wa = bitcast<f32>(b.w);
        let g = c.d * ws;
        f -= g * (ws * xi + wa * pos[b.x].xyz - (ws + wa) * pos[b.y].xyz);
        hd += vec3<f32>(g * ws);
    }
    let sc = 1.0 / (hd.x + hd.y + hd.z);
    let a = hd.x * sc; let b = ho.x * sc; let cc = ho.y * sc;
    let d = hd.y * sc; let e = ho.z * sc; let g = hd.z * sc;
    f = f * sc;
    let c00 = d * g - e * e; let c01 = cc * e - b * g; let c02 = b * e - cc * d;
    let c11 = a * g - cc * cc; let c12 = b * cc - a * e; let c22 = a * d - b * b;
    let det = a * c00 + b * c01 + cc * c02;
    var x = xi;
    if (det > 1e-12) {
        x += vec3<f32>(c00 * f.x + c01 * f.y + c02 * f.z, c01 * f.x + c11 * f.y + c12 * f.z, c02 * f.x + c12 * f.y + c22 * f.z) / det;
    }
    pos[i] = collide(x, prev[i].xyz);
}
"#;

/// Prédiction, accélération de Tchebychev, vitesse finale : des passes simples, élément par élément.
const MOVE: &str = r#"
@group(0) @binding(0) var<uniform> c: C;
@group(0) @binding(1) var<uniform> s: S;
@group(0) @binding(2) var<storage, read_write> pos: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> vel: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> tgt: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read_write> prev: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> sa: array<vec4<f32>>;
@group(0) @binding(7) var<storage, read_write> sb: array<vec4<f32>>;

@compute @workgroup_size(WG)
fn predict(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= s.count) { return; }
    let x = pos[i].xyz;
    let m = tgt[i].w;
    var y = x;
    var fixed = 1.0;
    if (m > 0.0) { y = x + c.h * vel[i].xyz; fixed = 0.0; }
    prev[i] = vec4<f32>(x, 0.0);
    tgt[i] = vec4<f32>(y, m);
    pos[i] = vec4<f32>(y, fixed);
    sa[i] = vec4<f32>(y, fixed);
}

@compute @workgroup_size(WG)
fn accel(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= s.count) { return; }
    let cur = pos[i];
    var x = cur.xyz;
    if (cur.w < 0.5) {
        let o = sb[i].xyz;
        x = o + s.omega * (x - o);
        pos[i] = vec4<f32>(x, cur.w);
    }
    sb[i] = vec4<f32>(x, 0.0);
}

@compute @workgroup_size(WG)
fn finish(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= s.count) { return; }
    var v = vec3<f32>(0.0);
    if (tgt[i].w > 0.0) { v = (pos[i].xyz - prev[i].xyz) / c.h * c.keep; }
    vel[i] = vec4<f32>(v, 0.0);
}
"#;

/// Le tissu sur la carte : ses colonnes, ses noyaux, ses branchements.
struct GpuCloth {
    pos: Column,
    vel: Column,
    consts: wgpu::Buffer,
    slots: wgpu::Buffer,
    slot: u64,
    /// Nombre d'itérations par sous-pas pour lequel les cases de réglages sont remplies
    slots_for: (usize, f32),
    solve: &'static Kernels,
    mv: &'static Kernels,
    g_solve: wgpu::BindGroup,
    /// Les deux branchements des passes simples : les tampons de Tchebychev y sont échangés
    g_move: [wgpu::BindGroup; 2],
    /// Colonnes gardées en vie (branchées, jamais relues ici)
    _keep: Vec<Column>,
}

/// Itérations par sous-pas que les cases de réglages peuvent décrire
const MAX_PER: usize = 4096;

impl GpuCloth {
    /// `None` si ce tissu ne tient pas sur cette carte (une colonne dépasserait ce qu'un noyau peut brancher).
    fn new(g: &Gpu, t: &Topo) -> Option<GpuCloth> {
        let biggest = (t.n * 16).max(t.off.len() * 8).max(t.springs.len() * 8).max(t.bends.len() * 16) as u64;
        if t.n == 0 || biggest > g.max_binding.min(g.max_buffer) || t.gpu_bytes() > g.max_buffer.saturating_mul(8) {
            return None;
        }
        let v4 = |label: &str| g.column(label, t.n, 16);
        let (pos, vel, tgt, prev, s0, s1) = (v4("tissu-position"), v4("tissu-vitesse"), v4("tissu-cible"), v4("tissu-precedent"), v4("tissu-cheb-0"), v4("tissu-cheb-1"));
        let off = g.column("tissu-debuts", t.off.len(), 8);
        let springs = g.column("tissu-ressorts", t.springs.len().max(1), 8);
        let bends = g.column("tissu-flexion", t.bends.len().max(1), 16);
        let masses: Vec<V4> = t.mass.iter().map(|m| [0.0, 0.0, 0.0, *m]).collect();
        g.write(&tgt, &masses);
        g.write(&off, &t.off);
        if !t.springs.is_empty() {
            g.write(&springs, &t.springs);
        }
        if !t.bends.is_empty() {
            g.write(&bends, &t.bends);
        }
        let slot = g.slot_size(16);
        let consts = g.uniform("tissu-reglages", std::mem::size_of::<Consts>() as u64);
        let slots = g.uniform("tissu-cases", slot * (t.colors.len() + MAX_PER) as u64);
        // Les noyaux sont les mêmes pour tous les tissus : compilés une fois, gardés
        static KERNELS: std::sync::OnceLock<(Kernels, Kernels)> = std::sync::OnceLock::new();
        let (solve, mv) = KERNELS.get_or_init(|| {
            (
                g.kernels("tissu-vbd", &format!("{STRUCTS}{SOLVE}"), &[Bind::Uniform, Bind::Slot(16), Bind::Write, Bind::Read, Bind::Read, Bind::Read, Bind::Read, Bind::Read], &["solve"]),
                g.kernels("tissu-pas", &format!("{STRUCTS}{MOVE}"), &[Bind::Uniform, Bind::Slot(16), Bind::Write, Bind::Write, Bind::Write, Bind::Write, Bind::Write, Bind::Write], &["predict", "accel", "finish"]),
            )
        });
        let one = |c: &Column| c.pages[0].clone();
        let (bp, bv, bt, bpr, b0, b1) = (one(&pos), one(&vel), one(&tgt), one(&prev), one(&s0), one(&s1));
        let g_solve = g.group(solve, &[&consts, &slots, &bp, &bt, &bpr, &off.pages[0], &springs.pages[0], &bends.pages[0]]);
        let g_move = [g.group(mv, &[&consts, &slots, &bp, &bv, &bt, &bpr, &b0, &b1]), g.group(mv, &[&consts, &slots, &bp, &bv, &bt, &bpr, &b1, &b0])];
        Some(GpuCloth { pos, vel, consts, slots, slot, slots_for: (0, -1.0), solve, mv, g_solve, g_move, _keep: vec![tgt, prev, s0, s1, off, springs, bends] })
    }

    /// Envoie à la carte tout un pas d'image : sous-pas, itérations, une passe par couleur.
    fn step(&mut self, g: &Gpu, t: &Topo, c: &Consts, p: &Params) {
        let (sub, per) = p.split();
        let rho = p.rho_used();
        let colors = t.colors.len();
        // Cases de réglages : une par couleur (sa plage), puis une par itération (son poids de Tchebychev)
        if self.slots_for != (per, rho) {
            let mut data = vec![0u8; (self.slot as usize) * (colors + per)];
            let mut put = |k: usize, v: [u32; 4]| data[k * self.slot as usize..][..16].copy_from_slice(gpusim::bytes_of(&v));
            for (k, col) in t.colors.iter().enumerate() {
                put(k, [col[0], col[1], 1.0f32.to_bits(), 0]);
            }
            let mut w = 1.0;
            for k in 1..=per {
                w = omega(k, w, rho);
                put(colors + k - 1, [0, t.n as u32, w.to_bits(), 0]);
            }
            g.queue.write_buffer(&self.slots, 0, &data);
            self.slots_for = (per, rho);
        }
        g.queue.write_buffer(&self.consts, 0, gpusim::bytes_of(std::slice::from_ref(c)));
        let at = |k: usize| [(k as u64 * self.slot) as u32];
        let all = at(colors);
        let mut enc = g.device.create_command_encoder(&Default::default());
        for _ in 0..sub {
            let mut pass = enc.begin_compute_pass(&Default::default());
            g.launch(&mut pass, &self.mv.pipelines[0], &self.g_move[0], &all, t.n as u64);
            for k in 1..=per {
                for (ci, col) in t.colors.iter().enumerate() {
                    g.launch(&mut pass, &self.solve.pipelines[0], &self.g_solve, &at(ci), col[1] as u64);
                }
                if rho > 0.0 {
                    // L'itération k relit l'itéré k−2 : il est dans le tampon « sb » d'un branchement sur deux
                    g.launch(&mut pass, &self.mv.pipelines[1], &self.g_move[(k + 1) % 2], &at(colors + k - 1), t.n as u64);
                }
            }
            g.launch(&mut pass, &self.mv.pipelines[2], &self.g_move[0], &all, t.n as u64);
        }
        g.queue.submit([enc.finish()]);
    }
}

// ---------- le tissu, vu de l'extérieur ----------

/// Un tissu prêt à simuler : sa topologie, son état, et sa copie sur la carte quand il y en a une.
/// L'état est gardé des deux côtés : si la carte rend une erreur, le pas est refait sur le processeur.
pub struct Cloth {
    pub topo: Topo,
    /// Centre du tissu au chargement : les positions des noyaux lui sont relatives
    origin: [f32; 3],
    /// État connu côté processeur, dans l'ordre de rangement
    pos: Vec<V4>,
    vel: Vec<V4>,
    gpu: Option<GpuCloth>,
    /// Ce que la carte n'a pas encore reçu (positions, vitesses)
    send: (bool, bool),
    /// Pas envoyés à la carte et pas encore relus : de quoi les refaire sur le processeur si elle échoue
    ahead: Vec<(Consts, Params)>,
}

impl Cloth {
    /// `positions` : l'état de départ, dans l'ordre de la géométrie. `on_gpu` : essayer la carte.
    pub fn new(topo: Topo, positions: &[Vec<f32>; 3], on_gpu: bool) -> Cloth {
        let n = topo.n;
        let mut origin = [0.0; 3];
        for c in 0..3 {
            let (lo, hi) = positions[c][..n].iter().fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
            if n > 0 {
                origin[c] = (lo + hi) * 0.5;
            }
        }
        let gpu = if on_gpu { gpusim::gpu().and_then(|g| GpuCloth::new(g, &topo)) } else { None };
        let mut cloth = Cloth { topo, origin, pos: vec![[0.0; 4]; n], vel: vec![[0.0; 4]; n], gpu, send: (true, true), ahead: Vec::new() };
        cloth.set_positions(positions);
        cloth
    }

    /// Le calcul se fait-il sur la carte ?
    pub fn on_gpu(&self) -> bool {
        self.gpu.is_some()
    }

    /// Rend la topologie (pour préparer le même tissu autrement sans la reconstruire)
    pub fn into_topo(self) -> Topo {
        self.topo
    }

    pub fn set_positions(&mut self, p: &[Vec<f32>; 3]) {
        self.sync();
        let o = self.origin;
        let perm = &self.topo.perm;
        self.pos.par_iter_mut().zip(perm.par_iter()).for_each(|(x, &i)| {
            let i = i as usize;
            *x = [p[0][i] - o[0], p[1][i] - o[1], p[2][i] - o[2], 0.0];
        });
        self.send.0 = true;
    }

    pub fn set_velocities(&mut self, v: &[Vec<f32>; 3]) {
        self.sync();
        let perm = &self.topo.perm;
        self.vel.par_iter_mut().zip(perm.par_iter()).for_each(|(x, &i)| {
            let i = i as usize;
            *x = [v[0][i], v[1][i], v[2][i], 0.0];
        });
        self.send.1 = true;
    }

    /// Avance d'une image. Sur la carte, rend la main dès que le travail est envoyé : `read`
    /// attend le résultat, `request` le demande sans attendre.
    pub fn step(&mut self, dt: f32, p: &Params) {
        if self.topo.n == 0 || dt <= 0.0 {
            return;
        }
        let c = Consts::new(p, dt, self.origin);
        if let (Some(gc), Some(g)) = (self.gpu.as_mut(), gpusim::gpu()) {
            if self.send.0 {
                g.write(&gc.pos, &self.pos);
            }
            if self.send.1 {
                g.write(&gc.vel, &self.vel);
            }
            self.send = (false, false);
            gc.step(g, &self.topo, &c, p);
            self.ahead.push((c, *p));
            return;
        }
        self.gpu = None;
        step_cpu(&self.topo, &mut self.pos, &mut self.vel, &c, p);
    }

    /// Demande à la carte l'état courant, sans attendre. `None` : rien à relire (calcul sur le processeur).
    pub fn request(&self) -> Option<Pending> {
        let (gc, g) = (self.gpu.as_ref()?, gpusim::gpu()?);
        (!self.ahead.is_empty()).then(|| Pending { pos: g.read(&gc.pos), vel: g.read(&gc.vel) })
    }

    /// Range dans l'état ce que la carte a rendu. Si elle a rendu une erreur, les pas en attente
    /// sont refaits sur le processeur, et le tissu y reste.
    pub fn accept(&mut self, pending: Option<Pending>) {
        if self.ahead.is_empty() {
            return;
        }
        let got = pending.and_then(|p| Some((p.pos.take::<V4>().ok()?, p.vel.take::<V4>().ok()?))).filter(|(p, v)| p.len() == self.topo.n && v.len() == self.topo.n);
        match got {
            Some((p, v)) => {
                self.pos = p;
                self.vel = v;
                self.ahead.clear();
            }
            None => {
                self.gpu = None;
                self.send = (true, true);
                for (c, p) in std::mem::take(&mut self.ahead) {
                    step_cpu(&self.topo, &mut self.pos, &mut self.vel, &c, &p);
                }
            }
        }
    }

    /// Met l'état du processeur à jour (attend la carte s'il le faut).
    pub fn sync(&mut self) {
        let pending = self.request();
        self.accept(pending);
    }

    /// Positions et vitesses courantes, dans l'ordre de la géométrie.
    pub fn read(&mut self) -> ([Vec<f32>; 3], [Vec<f32>; 3]) {
        self.sync();
        let (n, o) = (self.topo.n, self.origin);
        let mut p = [vec![0.0; n], vec![0.0; n], vec![0.0; n]];
        let mut v = [vec![0.0; n], vec![0.0; n], vec![0.0; n]];
        for (k, &i) in self.topo.perm.iter().enumerate() {
            let i = i as usize;
            for c in 0..3 {
                p[c][i] = self.pos[k][c] + o[c];
                v[c][i] = self.vel[k][c];
            }
        }
        (p, v)
    }
}

/// Une relecture de l'état demandée à la carte.
pub struct Pending {
    pos: gpusim::Readback,
    vel: gpusim::Readback,
}

impl Pending {
    /// Le résultat est-il arrivé ? Ne bloque pas.
    #[allow(dead_code)]
    pub fn ready(&self) -> bool {
        self.pos.ready() && self.vel.ready()
    }
}

// ---------- mesures ----------

/// Allongement des arêtes par rapport au repos : (le pire, la moyenne), 0,05 = 5 % trop long.
pub fn stretch(edges: &[[u32; 2]], rest: &[f32], p: &[Vec<f32>; 3]) -> (f32, f32) {
    let (mut worst, mut sum, mut count) = (0.0f32, 0.0f64, 0usize);
    for (e, r) in edges.iter().zip(rest) {
        let (a, b) = (e[0] as usize, e[1] as usize);
        if *r < 1e-9 || a >= p[0].len() || b >= p[0].len() {
            continue;
        }
        let l = ((p[0][a] - p[0][b]).powi(2) + (p[1][a] - p[1][b]).powi(2) + (p[2][a] - p[2][b]).powi(2)).sqrt();
        let s = l / r - 1.0;
        worst = worst.max(s);
        sum += s.abs() as f64;
        count += 1;
    }
    (worst, if count > 0 { (sum / count as f64) as f32 } else { 0.0 })
}

/// Longueur au repos de chaque arête, mesurée sur ces positions.
pub fn rest_lengths(edges: &[[u32; 2]], p: &[Vec<f32>; 3]) -> Vec<f32> {
    let n = p[0].len();
    edges.iter().map(|e| {
        let (a, b) = (e[0] as usize, e[1] as usize);
        if a >= n || b >= n { 0.0 } else { ((p[0][a] - p[0][b]).powi(2) + (p[1][a] - p[1][b]).powi(2) + (p[2][a] - p[2][b]).powi(2)).sqrt() }
    }).collect()
}

/// Une grille de tissu de `nx` × `nz` sommets, de côté `size`, à la hauteur `y`, avec ses diagonales :
/// positions et arêtes. Sert aux essais et au banc de mesure.
pub fn grid(nx: usize, nz: usize, size: f32, y: f32) -> ([Vec<f32>; 3], Vec<[u32; 2]>) {
    let mut p = [Vec::with_capacity(nx * nz), Vec::with_capacity(nx * nz), Vec::with_capacity(nx * nz)];
    let mut e = Vec::with_capacity(nx * nz * 4);
    for j in 0..nz {
        for i in 0..nx {
            p[0].push((i as f32 / (nx - 1).max(1) as f32 - 0.5) * size);
            p[1].push(y);
            p[2].push((j as f32 / (nz - 1).max(1) as f32 - 0.5) * size * (nz - 1).max(1) as f32 / (nx - 1).max(1) as f32);
            let a = (j * nx + i) as u32;
            if i + 1 < nx {
                e.push([a, a + 1]);
            }
            if j + 1 < nz {
                e.push([a, a + nx as u32]);
            }
            if i + 1 < nx && j + 1 < nz {
                e.push([a, a + nx as u32 + 1]);
                e.push([a + 1, a + nx as u32]);
            }
        }
    }
    (p, e)
}

// ---------- comparaison à budget égal ----------

/// Un solveur de référence branché de l'extérieur (le XPBD du processeur) : positions, vitesses,
/// arêtes, longueurs au repos, points fixes, pas de temps, réglages, souplesse équivalente à la raideur demandée.
pub type Other<'a> = &'a dyn Fn(&mut [Vec<f32>; 3], &mut [Vec<f32>; 3], &[[u32; 2]], &[f32], &[bool], f32, &Params, f32);

/// Une ligne de la comparaison : un solveur, un budget de passes par image.
#[derive(Clone, Debug)]
pub struct Row {
    pub vbd: bool,
    pub budget: usize,
    /// Tissu pendu : allongement total sous son poids (0,01 = 1 %)
    pub elongation: f32,
    /// Raideur obtenue, en part de la raideur demandée (1 : convergé)
    pub stiffness: f32,
    /// Énergie élastique du tissu pendu, en joules
    pub energy: f32,
    /// Tissu qui tombe sur une boule : pire allongement d'une arête pendant la chute, et moyenne à la fin
    pub worst: f32,
    pub mean: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Comparison {
    pub points: usize,
    pub stiffness: f32,
    /// Tissu pendu calculé jusqu'à convergence : allongement et énergie élastique
    pub reference: (f32, f32),
    pub rows: Vec<Row>,
}

/// VBD contre un autre solveur, à nombre égal de passes par image, sur un tissu de `n` × `n`
/// sommets et de 2 m de côté. Deux scènes : le tissu pendu par un bord (quelle raideur obtient-on
/// vraiment ?), et le tissu qui tombe sur une boule (de combien s'étire-t-il ?).
/// Le VBD tourne ici sur le processeur : la carte donne le même résultat (essai `carte_contre_processeur`).
pub fn compare(n: usize, budgets: &[usize], other: Other) -> Comparison {
    let k = 1000.0;
    let dt = 1.0 / 24.0;
    let dx = 2.0 / (n - 1) as f32;
    // Même problème pour l'autre solveur, qui compte en masses unité : souplesse = masse / raideur
    let compliance = DENSITY * dx * dx / k;
    let pinned: Vec<bool> = (0..n * n).map(|i| i / n == 0).collect();
    let (flat, edges) = grid(n, n, 2.0, 1.6);
    let rest = rest_lengths(&edges, &flat);
    // Le même tissu, pendu : la première rangée en haut, les autres dessous
    let hung = [flat[0].clone(), flat[2].iter().map(|z| 2.0 - (z + 1.0)).collect(), vec![0.0; n * n]];
    let run = |vbd: bool, start: &[Vec<f32>; 3], par: &Params, frames: usize, see: &mut dyn FnMut(usize, &[Vec<f32>; 3])| {
        let mut pos = start.clone();
        let mut vel = [vec![0.0; n * n], vec![0.0; n * n], vec![0.0; n * n]];
        let mut cloth = vbd.then(|| Cloth::new(Topo::build(n * n, &edges, &rest, start, &pinned, None), start, false));
        for f in 0..frames {
            for (i, y) in vel[1].iter_mut().enumerate() {
                if !pinned[i] {
                    *y -= 9.8 * dt;
                }
            }
            match cloth.as_mut() {
                Some(c) => {
                    c.set_velocities(&vel);
                    c.step(dt, par);
                    (pos, vel) = c.read();
                }
                None => other(&mut pos, &mut vel, &edges, &rest, &pinned, dt, par, compliance),
            }
            see(f, &pos);
        }
        pos
    };
    let elongation = |p: &[Vec<f32>; 3]| {
        let bottom: f32 = p[1][n * (n - 1)..].iter().sum::<f32>() / n as f32;
        (2.0 - bottom) / 2.0 - 1.0
    };
    let energy = |p: &[Vec<f32>; 3]| {
        edges.iter().zip(&rest).map(|(e, r)| {
            let (a, b) = (e[0] as usize, e[1] as usize);
            let l = ((p[0][a] - p[0][b]).powi(2) + (p[1][a] - p[1][b]).powi(2) + (p[2][a] - p[2][b]).powi(2)).sqrt();
            0.5 * k * (l - r) * (l - r)
        }).sum::<f32>()
    };
    // Tissu pendu : 72 images pour qu'il se pose, mesure moyennée sur les 12 dernières (la gravité
    // arrive une fois par image, le tissu frémit un peu entre deux)
    let hang = |vbd: bool, par: &Params| {
        let (mut e, mut j) = (0.0, 0.0);
        run(vbd, &hung, par, 72, &mut |f, p| {
            if f >= 60 {
                e += elongation(p) / 12.0;
                j += energy(p) / 12.0;
            }
        });
        (e, j)
    };
    let still = Params { stiffness: k, bending: 0.0, damping: 0.3, ..Default::default() };
    let fall = Params { stiffness: k, bending: 0.0, damping: 0.02, floor: Some(0.0), ball: Some(([0.0, 0.7, 0.0], 0.55)), ..Default::default() };
    // Référence : le tissu pendu, avec assez de passes pour que le résultat ne bouge plus
    let mut out = Comparison { points: n * n, stiffness: k, reference: hang(true, &Params { iterations: 2048, substeps: 16, ..still }), rows: vec![] };
    for &budget in budgets {
        for vbd in [false, true] {
            // L'autre solveur découpe lui-même ses passes en sous-pas ; le VBD prend le découpage
            // qui lui réussit le mieux à peu de passes : beaucoup de sous-pas, peu d'itérations
            let with = |p: Params| Params { iterations: budget, substeps: substeps_for(budget), ..p };
            let (e, j) = hang(vbd, &with(still));
            let mut worst = 0.0f32;
            let q = run(vbd, &flat, &with(fall), 60, &mut |_, p| worst = worst.max(stretch(&edges, &rest, p).0));
            out.rows.push(Row { vbd, budget, elongation: e, stiffness: if e > 1e-9 { (out.reference.0 / e).min(1.0) } else { 1.0 }, energy: j, worst, mean: stretch(&edges, &rest, &q).1 });
        }
    }
    out
}

/// Nombre de sous-pas conseillé pour un budget de passes par image.
pub fn substeps_for(budget: usize) -> usize {
    budget.clamp(1, 32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un tissu accroché par un bord, avec ses réglages d'essai
    fn hung(n: usize, on_gpu: bool) -> (Cloth, Vec<[u32; 2]>, Vec<f32>) {
        let (p, e) = grid(n, n, 2.0, 1.6);
        let rest = rest_lengths(&e, &p);
        let pinned: Vec<bool> = (0..n * n).map(|i| i / n == 0).collect();
        let topo = Topo::build(n * n, &e, &rest, &p, &pinned, None);
        (Cloth::new(topo, &p, on_gpu), e, rest)
    }

    /// Avance de `frames` images sous la gravité
    fn fall(c: &mut Cloth, frames: usize, par: &Params) -> [Vec<f32>; 3] {
        for _ in 0..frames {
            let (_, mut v) = c.read();
            v[1].iter_mut().for_each(|y| *y -= 9.8 / 24.0);
            c.set_velocities(&v);
            c.step(1.0 / 24.0, par);
        }
        c.read().0
    }

    fn worst_gap(a: &[Vec<f32>; 3], b: &[Vec<f32>; 3]) -> f32 {
        (0..3).flat_map(|k| a[k].iter().zip(&b[k]).map(|(x, y)| (x - y).abs())).fold(0.0, f32::max)
    }

    #[test]
    fn topologie_rangee_et_coloriee() {
        let (c, e, _) = hung(12, false);
        let t = &c.topo;
        let n = 144;
        // Rangement : une vraie permutation, et son inverse
        let mut seen = vec![false; n];
        for (k, &i) in t.perm.iter().enumerate() {
            assert!(!std::mem::replace(&mut seen[i as usize], true));
            assert_eq!(t.inv[i as usize], k as u32);
        }
        // Chaque arête apparaît chez ses deux sommets
        assert_eq!(t.springs.len(), e.len() * 2);
        assert!(!t.bends.is_empty() && t.colors.len() >= 4 && t.colors.len() <= 16, "{} couleurs", t.colors.len());
        // Les couleurs couvrent tous les sommets libres, les 12 fixes sont rangés après
        let free: u32 = t.colors.iter().map(|c| c[1]).sum();
        assert_eq!(free as usize, n - 12);
        let color_of = |k: u32| t.colors.iter().position(|c| k >= c[0] && k < c[0] + c[1]);
        for k in 0..n {
            let Some(mine) = color_of(k as u32) else {
                assert_eq!(t.mass[k], 0.0);
                continue;
            };
            assert!(t.mass[k] > 0.0);
            // Aucun voisin (ressort ou flexion) de la même couleur : c'est ce qui permet de les traiter ensemble
            for sp in &t.springs[t.off[k][0] as usize..t.off[k + 1][0] as usize] {
                assert_ne!(color_of(sp[0]), Some(mine));
            }
            for b in &t.bends[t.off[k][1] as usize..t.off[k + 1][1] as usize] {
                assert_ne!(color_of(b[0]), Some(mine));
                assert_ne!(color_of(b[1]), Some(mine));
            }
        }
    }

    #[test]
    fn tchebychev_bride_quand_peu_d_iterations() {
        let p = |it, sub| Params { iterations: it, substeps: sub, rho: 0.9, ..Default::default() };
        assert_eq!(p(16, 8).rho_used(), 0.0);
        assert!(p(64, 8).rho_used() > 0.5 && p(64, 8).rho_used() < 0.9);
        assert_eq!(p(256, 4).rho_used(), 0.9);
        assert_eq!(omega(1, 1.0, 0.9), 1.0);
        assert!(omega(2, 1.0, 0.9) > 1.0 && omega(3, omega(2, 1.0, 0.9), 0.9) < omega(2, 1.0, 0.9));
    }

    /// Le tissu tombe sur une boule et une boîte posées sur le sol : il ne les traverse pas, ne
    /// s'étire pas démesurément, et reste fini.
    #[test]
    fn tombe_et_drape_sans_traverser() {
        let (mut c, e, rest) = hung(24, false);
        let (ball, cube) = (([0.3, 0.7, 0.2], 0.5), ([-0.6, 0.3, 0.0], [0.25, 0.3, 0.4]));
        let par = Params { floor: Some(0.0), ball: Some(ball), cube: Some(cube), ..Default::default() };
        let p = fall(&mut c, 60, &par);
        let mut touched = 0;
        for i in 0..p[0].len() {
            let q = [p[0][i], p[1][i], p[2][i]];
            assert!(q.iter().all(|x| x.is_finite()));
            assert!(q[1] >= -1e-4, "sous le sol : {q:?}");
            let d = ((q[0] - ball.0[0]).powi(2) + (q[1] - ball.0[1]).powi(2) + (q[2] - ball.0[2]).powi(2)).sqrt();
            assert!(d >= ball.1 - 1e-3, "dans la boule : {d}");
            touched += (d < ball.1 + 1e-3) as usize;
            let inside = (0..3).all(|k| (q[k] - cube.0[k]).abs() < cube.1[k] - 1e-3);
            assert!(!inside, "dans la boîte : {q:?}");
        }
        assert!(touched > 10, "le tissu devrait reposer sur la boule ({touched} points)");
        let (worst, mean) = stretch(&e, &rest, &p);
        assert!(worst < 0.6 && mean < 0.05, "allongement {worst} / {mean}");
    }

    /// Plus de passes, moins d'allongement : la descente par blocs converge.
    #[test]
    fn plus_de_passes_moins_d_allongement() {
        let run = |it, sub| {
            let (mut c, e, rest) = hung(20, false);
            let p = fall(&mut c, 36, &Params { stiffness: 1e5, iterations: it, substeps: sub, ..Default::default() });
            stretch(&e, &rest, &p).1
        };
        let (few, some, many) = (run(8, 4), run(32, 8), run(128, 16));
        assert!(few > some * 1.5 && some > many * 1.5, "{few} {some} {many}");
    }

    /// Positions relatives : le même tissu posé à un kilomètre de l'origine se simule pareil.
    #[test]
    fn loin_de_l_origine_meme_resultat() {
        let run = |shift: [f32; 3]| {
            let (mut p, e) = grid(16, 16, 2.0, 1.6);
            for k in 0..3 {
                p[k].iter_mut().for_each(|x| *x += shift[k]);
            }
            let rest = rest_lengths(&grid(16, 16, 2.0, 1.6).1, &grid(16, 16, 2.0, 1.6).0);
            let pinned: Vec<bool> = (0..256).map(|i| i / 16 == 0).collect();
            let mut c = Cloth::new(Topo::build(256, &e, &rest, &p, &pinned, None), &p, false);
            let par = Params { floor: Some(shift[1]), ball: Some(([shift[0], shift[1] + 0.7, shift[2]], 0.5)), ..Default::default() };
            let mut out = fall(&mut c, 24, &par);
            for k in 0..3 {
                out[k].iter_mut().for_each(|x| *x -= shift[k]);
            }
            out
        };
        let gap = worst_gap(&run([0.0; 3]), &run([1000.0, 200.0, -500.0]));
        // À un kilomètre, un f32 ne distingue plus que 0,06 mm : l'écart vient de l'aller-retour, pas du calcul
        assert!(gap < 2e-3, "écart {gap}");
    }

    /// La règle du socle : le noyau de la carte et sa version processeur donnent le même tissu.
    #[test]
    fn carte_contre_processeur() {
        if gpusim::gpu().is_none() {
            return;
        }
        // Peu d'itérations (sans accélération), puis beaucoup (avec) ; sol, boule et boîte
        for (it, sub) in [(16, 8), (48, 4)] {
            let par = Params { iterations: it, substeps: sub, floor: Some(0.0), ball: Some(([0.3, 0.9, 0.2], 0.5)), cube: Some(([-0.6, 1.0, 0.0], [0.25, 0.3, 0.4])), ..Default::default() };
            let (mut cpu, _, _) = hung(20, false);
            let (mut gpu, _, _) = hung(20, true);
            assert!(gpu.on_gpu() && !cpu.on_gpu());
            let gap = worst_gap(&fall(&mut cpu, 8, &par), &fall(&mut gpu, 8, &par));
            assert!(gap < 5e-4, "écart {gap} ({it} passes, {sub} sous-pas)");
            assert!(gpu.on_gpu());
        }
    }

    /// Si la carte ne rend pas son résultat, le pas est refait sur le processeur, sans rien perdre.
    #[test]
    fn repli_sur_le_processeur() {
        if gpusim::gpu().is_none() {
            return;
        }
        let par = Params::default();
        let (mut cpu, _, _) = hung(12, false);
        let (mut gpu, _, _) = hung(12, true);
        let want = fall(&mut cpu, 3, &par);
        fall(&mut gpu, 2, &par);
        let (_, mut v) = gpu.read();
        v[1].iter_mut().for_each(|y| *y -= 9.8 / 24.0);
        gpu.set_velocities(&v);
        gpu.step(1.0 / 24.0, &par);
        // La relecture échoue : on fait comme si la carte n'avait rien rendu
        gpu.accept(None);
        assert!(!gpu.on_gpu());
        let got = gpu.read().0;
        assert!(worst_gap(&want, &got) < 5e-4);
        // Et la suite se fait sur le processeur
        fall(&mut gpu, 1, &par);
        assert!(gpu.read().0[1].iter().all(|y| y.is_finite()));
    }

    /// Relecture sans attendre : on envoie le pas, on demande le résultat, on tourne jusqu'à ce qu'il soit là.
    #[test]
    fn relecture_sans_attendre() {
        if gpusim::gpu().is_none() {
            return;
        }
        let (mut c, _, _) = hung(12, true);
        c.step(1.0 / 24.0, &Params::default());
        let pending = c.request().expect("un pas est en attente");
        let t0 = std::time::Instant::now();
        while !pending.ready() && t0.elapsed().as_secs() < 30 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(pending.ready());
        c.accept(Some(pending));
        assert!(c.on_gpu() && c.request().is_none());
    }
}
