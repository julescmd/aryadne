//! Retopologie automatique en quads, guidée par des traits dessinés sur le modèle.
//!
//! Méthode inspirée d'Instant Meshes (Jakob et al. 2015) :
//! 1. le maillage est densifié jusqu'à environ la moitié de la taille de quad visée ;
//! 2. un champ d'orientation 4-symétrique est lissé sur la surface, les sommets proches
//!    d'un guide étant forcés à suivre la direction du trait ;
//! 3. un champ de positions (grille locale de pas h) est lissé de la même manière ;
//! 4. les sommets qui tombent sur le même point de grille sont fusionnés, les voisins à
//!    un pas de grille deviennent des arêtes, puis les faces sont lues en tournant autour
//!    de chaque sommet.

use std::collections::{HashMap, HashSet};

type V3 = [f32; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}
fn norm(a: V3) -> V3 {
    let l = len(a);
    if l > 1e-20 { mul(a, 1.0 / l) } else { [1.0, 0.0, 0.0] }
}
fn tangent(v: V3, n: V3) -> V3 {
    sub(v, mul(n, dot(n, v)))
}

#[derive(Clone)]
pub struct Options {
    /// Nombre de faces visé
    pub faces: usize,
    /// Itérations de lissage
    pub iterations: usize,
    /// Rayon d'influence d'un guide, en tailles de quad
    pub guide_radius: f32,
    /// 100 % quads : extraction à double taille puis subdivision de chaque face en quads
    pub pure_quads: bool,
    /// Supprimer les dislocations du champ de positions (beaucoup moins de pôles)
    pub consistent: bool,
    /// Force d'alignement du flux sur les courbures principales (0 : aucun)
    pub curvature: f32,
}

impl Default for Options {
    fn default() -> Self {
        Options { faces: 1500, iterations: 160, guide_radius: 1.2, pure_quads: true, consistent: true, curvature: 1.0 }
    }
}

/// Mesures internes, pour le banc d'essai et les messages.
#[derive(Default, Clone, Debug)]
pub struct Stats {
    pub dense: usize,
    /// Singularités du champ d'orientation (pôles inévitables)
    pub field_sing: usize,
    /// Dislocations du champ de positions avant correction
    pub defects: usize,
    pub unfixed: usize,
    pub flips: usize,
}

pub struct Result {
    pub pos: Vec<V3>,
    pub faces: Vec<Vec<u32>>,
    pub stats: Stats,
}

impl Result {
    pub fn quads(&self) -> usize {
        self.faces.iter().filter(|f| f.len() == 4).count()
    }
}

/// Soude les sommets confondus (OBJ et STL les dupliquent souvent) et retire les triangles dégénérés.
fn weld(pos: &[V3], tris: &[[u32; 3]]) -> (Vec<V3>, Vec<[u32; 3]>) {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let eps = (len(sub(hi, lo)) * 1e-6).max(1e-12);
    let mut map: HashMap<[i64; 3], u32> = HashMap::new();
    let mut remap = vec![u32::MAX; pos.len()];
    let mut out = vec![];
    let mut used = vec![false; pos.len()];
    for t in tris {
        for &i in t {
            used[i as usize] = true;
        }
    }
    for (i, p) in pos.iter().enumerate() {
        if !used[i] {
            continue;
        }
        let k = [((p[0] - lo[0]) / eps).round() as i64, ((p[1] - lo[1]) / eps).round() as i64, ((p[2] - lo[2]) / eps).round() as i64];
        remap[i] = *map.entry(k).or_insert_with(|| {
            out.push(*p);
            (out.len() - 1) as u32
        });
    }
    let tris = tris.iter().map(|t| [remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize]]).filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]).collect();
    (out, tris)
}

/// Coupe les arêtes plus longues que `max_len` (découpe conforme 1 → 2, 3 ou 4 triangles).
fn refine(pos: Vec<V3>, tris: Vec<[u32; 3]>, max_len: f32) -> (Vec<V3>, Vec<[u32; 3]>) {
    refine_n(pos, tris, max_len, 20)
}

#[cfg(test)]
fn refine_once(pos: Vec<V3>, tris: Vec<[u32; 3]>) -> (Vec<V3>, Vec<[u32; 3]>) {
    refine_n(pos, tris, 0.0, 1)
}

fn refine_n(mut pos: Vec<V3>, mut tris: Vec<[u32; 3]>, max_len: f32, passes: usize) -> (Vec<V3>, Vec<[u32; 3]>) {
    for _ in 0..passes {
        let mut mid: HashMap<(u32, u32), u32> = HashMap::new();
        let mut out = Vec::with_capacity(tris.len() * 2);
        for t in &tris {
            let mut m = [u32::MAX; 3];
            let mut n = 0;
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                if len(sub(pos[a as usize], pos[b as usize])) > max_len {
                    m[k] = *mid.entry((a.min(b), a.max(b))).or_insert_with(|| {
                        pos.push(mul(add(pos[a as usize], pos[b as usize]), 0.5));
                        (pos.len() - 1) as u32
                    });
                    n += 1;
                }
            }
            match n {
                0 => out.push(*t),
                3 => out.extend([[t[0], m[0], m[2]], [t[1], m[1], m[0]], [t[2], m[2], m[1]], [m[0], m[1], m[2]]]),
                1 => {
                    let k = (0..3).find(|&k| m[k] != u32::MAX).unwrap();
                    let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
                    out.extend([[a, m[k], c], [m[k], b, c]]);
                }
                _ => {
                    // arêtes k et k+1 coupées
                    let k = (0..3).find(|&k| m[k] != u32::MAX && m[(k + 1) % 3] != u32::MAX).unwrap();
                    let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
                    let (m1, m2) = (m[k], m[(k + 1) % 3]);
                    out.extend([[m1, b, m2], [a, m1, m2], [a, m2, c]]);
                }
            }
        }
        let done = mid.is_empty();
        tris = out;
        if done || tris.len() > 1_600_000 {
            break;
        }
    }
    (pos, tris)
}

fn compat_orient(q0: V3, n0: V3, q1: V3, n1: V3) -> (V3, V3) {
    let a = [q0, cross(n0, q0)];
    let b = [q1, cross(n1, q1)];
    let mut best = (0, 0, -1.0f32);
    for (i, ai) in a.iter().enumerate() {
        for (j, bj) in b.iter().enumerate() {
            let d = dot(*ai, *bj).abs();
            if d > best.2 {
                best = (i, j, d);
            }
        }
    }
    let s = dot(a[best.0], b[best.1]).signum();
    (a[best.0], mul(b[best.1], s))
}

fn middle_point(p0: V3, n0: V3, p1: V3, n1: V3) -> V3 {
    let (n0p0, n0p1, n1p0, n1p1, n0n1) = (dot(n0, p0), dot(n0, p1), dot(n1, p0), dot(n1, p1), dot(n0, n1));
    let denom = 1.0 / (1.0 - n0n1 * n0n1 + 1e-4);
    let l0 = 2.0 * (n0p1 - n0p0 - n0n1 * (n1p0 - n1p1)) * denom;
    let l1 = 2.0 * (n1p0 - n1p1 - n0n1 * (n0p1 - n0p0)) * denom;
    sub(mul(add(p0, p1), 0.5), mul(add(mul(n0, l0), mul(n1, l1)), 0.25))
}

fn floor4(o: V3, q: V3, t: V3, target: V3, h: f32) -> V3 {
    let d = sub(target, o);
    add(o, add(mul(q, (dot(q, d) / h).floor() * h), mul(t, (dot(t, d) / h).floor() * h)))
}

fn round4(o: V3, q: V3, n: V3, p: V3, h: f32) -> V3 {
    let t = cross(n, q);
    let d = sub(p, o);
    add(o, add(mul(q, (dot(q, d) / h).round() * h), mul(t, (dot(t, d) / h).round() * h)))
}

#[allow(clippy::too_many_arguments)]
fn compat_pos(p0: V3, n0: V3, q0: V3, o0: V3, p1: V3, n1: V3, q1: V3, o1: V3, h: f32) -> (V3, V3) {
    let (t0, t1) = (cross(n0, q0), cross(n1, q1));
    let m = middle_point(p0, n0, p1, n1);
    let a = floor4(o0, q0, t0, m, h);
    let b = floor4(o1, q1, t1, m, h);
    let mut best = (a, b, f32::MAX);
    for i in 0..4 {
        let at = add(a, mul(add(mul(q0, (i & 1) as f32), mul(t0, ((i & 2) >> 1) as f32)), h));
        for j in 0..4 {
            let bt = add(b, mul(add(mul(q1, (j & 1) as f32), mul(t1, ((j & 2) >> 1) as f32)), h));
            let c = dot(sub(at, bt), sub(at, bt));
            if c < best.2 {
                best = (at, bt, c);
            }
        }
    }
    (best.0, best.1)
}

struct Uf(Vec<u32>);
impl Uf {
    fn find(&mut self, mut x: u32) -> u32 {
        while self.0[x as usize] != x {
            self.0[x as usize] = self.0[self.0[x as usize] as usize];
            x = self.0[x as usize];
        }
        x
    }
    fn union(&mut self, a: u32, b: u32) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a.max(b) as usize] = a.min(b);
        }
    }
}

/// Point le plus proche sur un segment.
fn seg_dist(p: V3, a: V3, b: V3) -> (f32, V3) {
    let ab = sub(b, a);
    let t = (dot(sub(p, a), ab) / dot(ab, ab).max(1e-20)).clamp(0.0, 1.0);
    (len(sub(p, add(a, mul(ab, t)))), ab)
}

// ---------------------------------------------------------------------------------------
// Cohérence entière (d'après QuadriFlow, Huang et al. 2018).
// Chaque sommet dense i porte un repère (e_i, f_i) et une origine de grille o_i. Pour une
// arête i→j : k_ij (rotation de 90°·k qui amène le repère de j sur celui de i) et d_ij ∈ Z²
// (le point de grille x de j est le point d_ij + T(k_ij)·x de i). Un triangle régulier dont
// la somme des d n'est pas nulle contient une dislocation : une rangée de quads qui s'arrête
// net, donc une paire de pôles 3-5 inutile. On les fait glisser jusqu'à ce qu'elles
// s'annulent entre elles ou tombent dans une vraie singularité du champ d'orientation.
// ---------------------------------------------------------------------------------------

fn trot(k: u8, v: [i32; 2]) -> [i32; 2] {
    match k & 3 {
        0 => v,
        1 => [v[1], -v[0]],
        2 => [-v[0], -v[1]],
        _ => [-v[1], v[0]],
    }
}

struct Graph {
    /// Extrémités (i < j)
    ends: Vec<(u32, u32)>,
    k: Vec<u8>,
    d: Vec<[i32; 2]>,
    /// Faces de part et d'autre (u32::MAX : aucune)
    ef: Vec<[u32; 2]>,
    /// Arête non variété (plus de deux faces) : traitée comme un bord
    nm: Vec<bool>,
    /// Arêtes de chaque face : (a,b), (b,c), (c,a)
    fe: Vec<[u32; 3]>,
    map: HashMap<(u32, u32), u32>,
    /// Racine du poids de chaque arête dans les moindres carrés
    w: Vec<f64>,
}

impl Graph {
    fn new(f: &[[u32; 3]]) -> Graph {
        let mut g = Graph { ends: vec![], k: vec![], d: vec![], ef: vec![], nm: vec![], fe: Vec::with_capacity(f.len()), map: HashMap::new(), w: vec![] };
        for (fi, t) in f.iter().enumerate() {
            let mut es = [0u32; 3];
            for s in 0..3 {
                let (a, b) = (t[s], t[(s + 1) % 3]);
                let key = (a.min(b), a.max(b));
                let e = *g.map.entry(key).or_insert_with(|| {
                    g.ends.push(key);
                    g.ef.push([u32::MAX; 2]);
                    g.nm.push(false);
                    (g.ends.len() - 1) as u32
                });
                let slot = &mut g.ef[e as usize];
                if slot[0] == u32::MAX {
                    slot[0] = fi as u32;
                } else if slot[1] == u32::MAX {
                    slot[1] = fi as u32;
                } else {
                    g.nm[e as usize] = true;
                }
                es[s] = e;
            }
            g.fe.push(es);
        }
        g.k = vec![0; g.ends.len()];
        g.d = vec![[0; 2]; g.ends.len()];
        g.w = vec![1.0; g.ends.len()];
        g
    }
    /// (k, d) de l'arête `e` parcourue depuis le sommet `from`.
    fn dir(&self, e: u32, from: u32) -> (u8, [i32; 2]) {
        let (k, d) = (self.k[e as usize], self.d[e as usize]);
        if self.ends[e as usize].0 == from {
            (k, d)
        } else {
            let kr = (4 - k) & 3;
            let r = trot(kr, d);
            (kr, [-r[0], -r[1]])
        }
    }
    fn set_dir(&mut self, e: u32, from: u32, d: [i32; 2]) {
        if self.ends[e as usize].0 == from {
            self.d[e as usize] = d;
        } else {
            let r = trot(self.k[e as usize], d);
            self.d[e as usize] = [-r[0], -r[1]];
        }
    }
    /// Somme des rotations (mod 4), défaut de fermeture, et rotations cumulées avant chaque arête.
    fn closure(&self, t: &[u32; 3], fi: usize) -> (u8, [i32; 2], [u8; 3]) {
        let es = self.fe[fi];
        let (k1, d1) = self.dir(es[0], t[0]);
        let (k2, d2) = self.dir(es[1], t[1]);
        let (k3, d3) = self.dir(es[2], t[2]);
        let (a, b) = (trot(k1, d2), trot(k1 + k2, d3));
        ((k1 + k2 + k3) & 3, [d1[0] + a[0] + b[0], d1[1] + a[1] + b[1]], [0, k1, (k1 + k2) & 3])
    }
    /// Annule le défaut de la face `fi` en modifiant son arête n° `slot` (le défaut passe à la face voisine).
    fn push(&mut self, t: &[u32; 3], fi: usize, slot: usize) {
        let c = self.closure(t, fi).1;
        self.push_vec(t, fi, slot, c);
    }
    /// Retire le vecteur `c` de la fermeture de la face `fi` en modifiant son arête n° `slot`.
    fn push_vec(&mut self, t: &[u32; 3], fi: usize, slot: usize, c: [i32; 2]) {
        let pre = self.closure(t, fi).2;
        let e = self.fe[fi][slot];
        let (_, d) = self.dir(e, t[slot]);
        let r = trot((4 - pre[slot]) & 3, c);
        self.set_dir(e, t[slot], [d[0] - r[0], d[1] - r[1]]);
    }
}

/// Entiers (k, d) de chaque arête à partir des champs d'orientation et de positions.
fn integers(g: &mut Graph, v: &[V3], n: &[V3], q: &[V3], o: &[V3], h: f32) {
    for e in 0..g.ends.len() {
        let (i, j) = (g.ends[e].0 as usize, g.ends[e].1 as usize);
        // k : rotation de q_j autour de n_j qui colle le mieux à q_i
        let tj = cross(n[j], q[j]);
        let cands = [q[j], tj, mul(q[j], -1.0), mul(tj, -1.0)];
        let k = (0..4).max_by(|&a, &b| dot(q[i], cands[a]).total_cmp(&dot(q[i], cands[b]))).unwrap() as u8;
        let (ti, tjj) = compat_pos_int(v[i], n[i], q[i], o[i], v[j], n[j], q[j], o[j], h);
        let r = trot(k, tjj);
        g.k[e] = k;
        g.d[e] = [ti[0] - r[0], ti[1] - r[1]];
    }
}

/// Comme `compat_pos`, mais renvoie les coordonnées entières des deux points de grille retenus.
#[allow(clippy::too_many_arguments)]
fn compat_pos_int(p0: V3, n0: V3, q0: V3, o0: V3, p1: V3, n1: V3, q1: V3, o1: V3, h: f32) -> ([i32; 2], [i32; 2]) {
    let (t0, t1) = (cross(n0, q0), cross(n1, q1));
    let m = middle_point(p0, n0, p1, n1);
    let (d0, d1) = (sub(m, o0), sub(m, o1));
    let f0 = [(dot(q0, d0) / h).floor() as i32, (dot(t0, d0) / h).floor() as i32];
    let f1 = [(dot(q1, d1) / h).floor() as i32, (dot(t1, d1) / h).floor() as i32];
    let mut best = (f0, f1, f32::MAX);
    for i in 0..4 {
        let a = [f0[0] + (i & 1), f0[1] + ((i & 2) >> 1)];
        let pa = add(o0, add(mul(q0, a[0] as f32 * h), mul(t0, a[1] as f32 * h)));
        for j in 0..4 {
            let b = [f1[0] + (j & 1), f1[1] + ((j & 2) >> 1)];
            let pb = add(o1, add(mul(q1, b[0] as f32 * h), mul(t1, b[1] as f32 * h)));
            let c = dot(sub(pa, pb), sub(pa, pb));
            if c < best.2 {
                best = (a, b, c);
            }
        }
    }
    (best.0, best.1)
}

/// Fait disparaître les dislocations. Renvoie (singularités d'orientation, dislocations trouvées, restantes).
/// Trois passes, du plus propre au plus brutal :
/// 1. deux dislocations opposées proches s'annulent (aucun effet de bord) ;
/// 2. une dislocation rejoint une autre même sans l'annuler tout à fait ;
/// 3. ce qui reste tombe dans la singularité d'orientation ou le bord le plus proche.
fn fix_defects(g: &mut Graph, f: &[[u32; 3]]) -> (usize, usize, usize) {
    let nf = f.len();
    let mut sing = vec![false; nf];
    let mut defect = vec![false; nf];
    for fi in 0..nf {
        let (ks, c, _) = g.closure(&f[fi], fi);
        sing[fi] = ks != 0;
        defect[fi] = ks == 0 && c != [0, 0];
    }
    let n_sing = sing.iter().filter(|x| **x).count();
    let n_def = defect.iter().filter(|x| **x).count();
    let mut stamp = vec![0u32; nf];
    let mut parent: Vec<(u32, u8)> = vec![(u32::MAX, 0); nf];
    let mut tick = 0u32;
    let l1 = |c: [i32; 2]| c[0].abs() + c[1].abs();
    for phase in 0..3 {
        let mut radius = 4usize;
        loop {
            let list: Vec<usize> = (0..nf).filter(|&i| defect[i]).collect();
            if list.is_empty() || radius > 4 * nf || (phase < 2 && radius > 96) {
                break;
            }
            for &start in &list {
                if !defect[start] {
                    continue;
                }
                tick += 1;
                stamp[start] = tick;
                let mut queue = std::collections::VecDeque::new();
                queue.push_back((start as u32, 0usize));
                // Candidats : (face d'arrivée, emplacement de sortie si c'est un bord)
                let mut cands: Vec<(u32, Option<u8>)> = vec![];
                while let Some((x, depth)) = queue.pop_front() {
                    if depth > radius || cands.len() >= 6 {
                        break;
                    }
                    for s in 0..3 {
                        let e = g.fe[x as usize][s] as usize;
                        let y = if g.nm[e] {
                            u32::MAX
                        } else if g.ef[e][0] == x {
                            g.ef[e][1]
                        } else {
                            g.ef[e][0]
                        };
                        if y == u32::MAX {
                            if phase == 2 {
                                cands.push((x, Some(s as u8)));
                            }
                            continue;
                        }
                        if stamp[y as usize] == tick {
                            continue;
                        }
                        stamp[y as usize] = tick;
                        parent[y as usize] = (x, s as u8);
                        if sing[y as usize] {
                            if phase == 2 {
                                cands.push((y, None));
                            }
                        } else if defect[y as usize] {
                            cands.push((y, None));
                        } else {
                            queue.push_back((y, depth + 1));
                        }
                    }
                }
                let before = l1(g.closure(&f[start], start).1);
                for (end, exit) in cands {
                    // Chemin start → end
                    let mut path = vec![];
                    let mut cur = end;
                    while cur as usize != start {
                        let (p, s) = parent[cur as usize];
                        path.push((p, s));
                        cur = p;
                    }
                    path.reverse();
                    let saved: Vec<(u32, [i32; 2])> = path.iter().map(|&(x, s)| g.fe[x as usize][s as usize]).chain(exit.map(|s| g.fe[end as usize][s as usize])).map(|e| (e, g.d[e as usize])).collect();
                    let end_before = if sing[end as usize] || exit.is_some() { 0 } else { l1(g.closure(&f[end as usize], end as usize).1) };
                    for &(x, s) in &path {
                        g.push(&f[x as usize], x as usize, s as usize);
                    }
                    if let Some(s) = exit {
                        g.push(&f[end as usize], end as usize, s as usize);
                    }
                    let end_after = if sing[end as usize] || exit.is_some() { 0 } else { l1(g.closure(&f[end as usize], end as usize).1) };
                    let ok = match phase {
                        0 => end_after == 0,
                        1 => end_after < before + end_before,
                        _ => true,
                    };
                    if ok {
                        defect[start] = false;
                        if !sing[end as usize] {
                            defect[end as usize] = end_after != 0;
                        }
                        break;
                    }
                    for (e, d) in saved {
                        g.d[e as usize] = d;
                    }
                }
            }
            radius *= 2;
        }
    }
    (n_sing, n_def, defect.iter().filter(|x| **x).count())
}

fn energy(g: &Graph, geo: &[[f64; 2]], u: &[[f64; 2]]) -> f64 {
    let mut en = 0.0;
    for e in 0..g.ends.len() {
        let (i, j) = (g.ends[e].0 as usize, g.ends[e].1 as usize);
        let t = trotf(g.k[e], u[j]);
        let r = [t[0] - u[i][0] - (geo[e][0] - g.d[e][0] as f64), t[1] - u[i][1] - (geo[e][1] - g.d[e][1] as f64)];
        en += r[0] * r[0] + r[1] * r[1];
    }
    en
}

/// Une singularité tombe sur un point de grille (sommet du maillage final) si sa fermeture
/// est « paire » ; sinon elle est au milieu d'une case et donnerait un triangle ou un pentagone.
fn odd(ks: u8, c: [i32; 2]) -> bool {
    match ks {
        0 => false,
        2 => c[0] & 1 != 0 || c[1] & 1 != 0,
        _ => (c[0] + c[1]) & 1 != 0,
    }
}

/// Triangles repliés dans la carte entière (aire orientée négative).
fn count_flips(g: &Graph, f: &[[u32; 3]]) -> usize {
    let mut n = 0;
    for (fi, t) in f.iter().enumerate() {
        let es = g.fe[fi];
        let (k1, d1) = g.dir(es[0], t[0]);
        let (_, d2) = g.dir(es[1], t[1]);
        let r = trot(k1, d2);
        let b = [d1[0] + r[0], d1[1] + r[1]];
        if d1[0] * b[1] - d1[1] * b[0] < 0 {
            n += 1;
        }
    }
    n
}

fn trotf(k: u8, v: [f64; 2]) -> [f64; 2] {
    match k & 3 {
        0 => v,
        1 => [v[1], -v[0]],
        2 => [-v[0], -v[1]],
        _ => [-v[1], v[0]],
    }
}

/// Chemin d'une singularité vers la racine (ou le bord) : faire glisser un vecteur le long de
/// ce chemin déplace la singularité par rapport à la grille sans casser la cohérence ailleurs.
struct ConePath {
    face: u32,
    steps: Vec<(u32, u8)>,
    /// Pour chaque arête du chemin : variation de d pour un vecteur poussé (1,0) puis (0,1)
    cols: Vec<(u32, [i32; 2], [i32; 2])>,
}

/// Paramétrisation pour des entiers fixés : u_i = coordonnées de grille du sommet i dans sa
/// propre carte. On minimise Σ |T(k)·u_j − u_i − (g_ij − d_ij) + Σ M·λ|² par gradient conjugué
/// (CGLS). Les λ (un vecteur réel par chemin) laissent les singularités glisser librement :
/// c'est la relaxation continue du problème entier, comme dans Mixed-Integer Quadrangulation.
fn solve_ext(g: &Graph, geo: &[[f64; 2]], u: &mut [[f64; 2]], paths: &[&ConePath], lam: &mut [[f64; 2]], max_iter: usize, tol: f64) -> usize {
    let (n, ne, np) = (u.len(), g.ends.len(), paths.len());
    // Mise à l'échelle des colonnes (préconditionneur de Jacobi) : x = S·y
    let mut deg = vec![0.0f64; n];
    for (e, &(i, j)) in g.ends.iter().enumerate() {
        deg[i as usize] += g.w[e] * g.w[e];
        deg[j as usize] += g.w[e] * g.w[e];
    }
    let sc_u: Vec<f64> = deg.iter().map(|d| 1.0 / d.max(1.0).sqrt()).collect();
    let sc_l: Vec<f64> = paths.iter().map(|p| 1.0 / p.cols.iter().map(|c| g.w[c.0 as usize] * g.w[c.0 as usize]).sum::<f64>().max(1.0).sqrt()).collect();
    // B·S·y
    let fwd = |yu: &[[f64; 2]], yl: &[[f64; 2]], out: &mut [[f64; 2]]| {
        for e in 0..ne {
            let (i, j) = (g.ends[e].0 as usize, g.ends[e].1 as usize);
            let t = trotf(g.k[e], yu[j]);
            out[e] = [t[0] * sc_u[j] - yu[i][0] * sc_u[i], t[1] * sc_u[j] - yu[i][1] * sc_u[i]];
        }
        for (s, p) in paths.iter().enumerate() {
            let l = [yl[s][0] * sc_l[s], yl[s][1] * sc_l[s]];
            for &(e, dx, dy) in &p.cols {
                out[e as usize][0] += dx[0] as f64 * l[0] + dy[0] as f64 * l[1];
                out[e as usize][1] += dx[1] as f64 * l[0] + dy[1] as f64 * l[1];
            }
        }
        for e in 0..ne {
            out[e][0] *= g.w[e];
            out[e][1] *= g.w[e];
        }
    };
    // S·Bᵀ·r
    let adjoint = |r: &[[f64; 2]], ou: &mut [[f64; 2]], ol: &mut [[f64; 2]]| {
        for o in ou.iter_mut() {
            *o = [0.0; 2];
        }
        for e in 0..ne {
            let (i, j) = (g.ends[e].0 as usize, g.ends[e].1 as usize);
            let re = [r[e][0] * g.w[e], r[e][1] * g.w[e]];
            ou[i][0] -= re[0];
            ou[i][1] -= re[1];
            let t = trotf((4 - g.k[e]) & 3, re);
            ou[j][0] += t[0];
            ou[j][1] += t[1];
        }
        for i in 0..n {
            ou[i][0] *= sc_u[i];
            ou[i][1] *= sc_u[i];
        }
        for (s, p) in paths.iter().enumerate() {
            let mut acc = [0.0; 2];
            for &(e, dx, dy) in &p.cols {
                let re = [r[e as usize][0] * g.w[e as usize], r[e as usize][1] * g.w[e as usize]];
                acc[0] += dx[0] as f64 * re[0] + dx[1] as f64 * re[1];
                acc[1] += dy[0] as f64 * re[0] + dy[1] as f64 * re[1];
            }
            ol[s] = [acc[0] * sc_l[s], acc[1] * sc_l[s]];
        }
    };
    let dot2 = |a: &[[f64; 2]], b: &[[f64; 2]]| -> f64 { a.iter().zip(b).map(|(x, y)| x[0] * y[0] + x[1] * y[1]).sum() };
    let mut yu: Vec<[f64; 2]> = (0..n).map(|i| [u[i][0] / sc_u[i], u[i][1] / sc_u[i]]).collect();
    let mut yl: Vec<[f64; 2]> = (0..np).map(|s| [lam[s][0] / sc_l[s], lam[s][1] / sc_l[s]]).collect();
    let mut r = vec![[0.0f64; 2]; ne];
    fwd(&yu, &yl, &mut r);
    let mut c2 = 0.0;
    for e in 0..ne {
        let c = [(geo[e][0] - g.d[e][0] as f64) * g.w[e], (geo[e][1] - g.d[e][1] as f64) * g.w[e]];
        c2 += c[0] * c[0] + c[1] * c[1];
        r[e] = [c[0] - r[e][0], c[1] - r[e][1]];
    }
    let (mut su, mut sl) = (vec![[0.0f64; 2]; n], vec![[0.0f64; 2]; np]);
    adjoint(&r, &mut su, &mut sl);
    let (mut pu, mut pl) = (su.clone(), sl.clone());
    let mut gamma = dot2(&su, &su) + dot2(&sl, &sl);
    let stop = c2 * tol + 1e-24;
    let mut q = vec![[0.0f64; 2]; ne];
    let mut it = 0;
    while it < max_iter && gamma > stop {
        fwd(&pu, &pl, &mut q);
        let alpha = gamma / dot2(&q, &q).max(1e-300);
        for i in 0..n {
            yu[i][0] += alpha * pu[i][0];
            yu[i][1] += alpha * pu[i][1];
        }
        for s in 0..np {
            yl[s][0] += alpha * pl[s][0];
            yl[s][1] += alpha * pl[s][1];
        }
        for e in 0..ne {
            r[e][0] -= alpha * q[e][0];
            r[e][1] -= alpha * q[e][1];
        }
        adjoint(&r, &mut su, &mut sl);
        let g2 = dot2(&su, &su) + dot2(&sl, &sl);
        let beta = g2 / gamma;
        gamma = g2;
        for i in 0..n {
            pu[i] = [su[i][0] + beta * pu[i][0], su[i][1] + beta * pu[i][1]];
        }
        for s in 0..np {
            pl[s] = [sl[s][0] + beta * pl[s][0], sl[s][1] + beta * pl[s][1]];
        }
        it += 1;
    }
    for i in 0..n {
        u[i] = [yu[i][0] * sc_u[i], yu[i][1] * sc_u[i]];
    }
    for s in 0..np {
        lam[s] = [yl[s][0] * sc_l[s], yl[s][1] * sc_l[s]];
    }
    it
}

fn solve_u(g: &Graph, geo: &[[f64; 2]], u: &mut [[f64; 2]], max_iter: usize) -> usize {
    solve_ext(g, geo, u, &[], &mut [], max_iter, 1e-10)
}

/// Degrés de liberté entiers de la grille : un chemin par singularité (arbre couvrant du
/// graphe dual, racine = un bord s'il y en a, sinon une singularité) et une boucle par anse
/// de la surface (arêtes hors de l'arbre dual et de l'arbre primal : « tree-cotree »).
fn cone_paths(g: &mut Graph, f: &[[u32; 3]]) -> Vec<ConePath> {
    let nf = f.len();
    let sing: Vec<bool> = (0..nf).map(|fi| g.closure(&f[fi], fi).0 != 0).collect();
    let other = |g: &Graph, x: u32, s: usize| -> u32 {
        let e = g.fe[x as usize][s] as usize;
        if g.nm[e] {
            u32::MAX
        } else if g.ef[e][0] == x {
            g.ef[e][1]
        } else {
            g.ef[e][0]
        }
    };
    let mut visited = vec![false; nf];
    // parent[y] = (face plus proche de la racine, emplacement dans y de l'arête commune)
    let mut parent: Vec<(u32, u8)> = vec![(u32::MAX, 0); nf];
    let mut depth = vec![0u32; nf];
    let mut exit: Vec<u8> = vec![u8::MAX; nf];
    let mut crossed = vec![false; g.ends.len()];
    let mut queue = std::collections::VecDeque::new();
    let mut cones = vec![];
    let mut seed = 0;
    loop {
        while let Some(x) = queue.pop_front() {
            for s in 0..3 {
                let y = other(g, x, s);
                if y == u32::MAX || visited[y as usize] {
                    continue;
                }
                visited[y as usize] = true;
                let e = g.fe[x as usize][s];
                crossed[e as usize] = true;
                let sy = (0..3).find(|&k| g.fe[y as usize][k] == e).unwrap_or(0);
                parent[y as usize] = (x, sy as u8);
                depth[y as usize] = depth[x as usize] + 1;
                if sing[y as usize] {
                    cones.push(y);
                } else {
                    queue.push_back(y);
                }
            }
        }
        // Nouvelle composante : racine sur un bord, sinon sur une singularité, sinon n'importe où
        while seed < nf && visited[seed] {
            seed += 1;
        }
        if seed >= nf {
            break;
        }
        // Composante de `seed` : chercher la meilleure racine
        let mut comp = vec![seed as u32];
        let mut mark: HashSet<u32> = comp.iter().copied().collect();
        let mut k = 0;
        while k < comp.len() {
            let x = comp[k];
            k += 1;
            for s in 0..3 {
                let y = other(g, x, s);
                if y != u32::MAX && mark.insert(y) {
                    comp.push(y);
                }
            }
        }
        let border = comp.iter().copied().find(|&x| !sing[x as usize] && (0..3).any(|s| other(g, x, s) == u32::MAX));
        let root = border.or_else(|| comp.iter().copied().find(|&x| sing[x as usize])).unwrap_or(seed as u32);
        visited[root as usize] = true;
        if let Some(b) = border {
            exit[b as usize] = (0..3).find(|&s| other(g, b, s) == u32::MAX).unwrap() as u8;
        }
        queue.push_back(root);
    }
    // Chemin d'une face jusqu'à sa racine : suite de (face, emplacement de sortie)
    let up = |from: u32, stop: u32| -> Vec<(u32, u8)> {
        let mut steps = vec![];
        let mut cur = from;
        while cur != stop {
            let (x, sy) = parent[cur as usize];
            if x == u32::MAX {
                if exit[cur as usize] != u8::MAX {
                    steps.push((cur, exit[cur as usize]));
                }
                break;
            }
            steps.push((cur, sy));
            cur = x;
        }
        steps
    };
    let slot_of = |g: &Graph, x: u32, e: u32| -> u8 { (0..3).find(|&k| g.fe[x as usize][k] == e).unwrap_or(0) as u8 };
    let mut list: Vec<(u32, Vec<(u32, u8)>)> = vec![];
    for c in cones {
        let steps = up(c, u32::MAX);
        if !steps.is_empty() {
            list.push((c, steps));
        }
    }
    // Boucles des anses : arbre primal sur les arêtes non traversées, en prenant d'abord
    // celles des triangles singuliers pour que les boucles ne passent pas par eux.
    let nv = g.ends.iter().map(|e| e.1).max().map(|m| m as usize + 1).unwrap_or(0);
    let mut uf = Uf((0..nv as u32).collect());
    let mut in_tree = vec![false; g.ends.len()];
    let mut near_sing = vec![false; g.ends.len()];
    for fi in 0..nf {
        if sing[fi] {
            for &e in &g.fe[fi] {
                near_sing[e as usize] = true;
            }
        }
    }
    for pass in 0..2 {
        for e in 0..g.ends.len() {
            if crossed[e] || near_sing[e] != (pass == 0) {
                continue;
            }
            let (a, b) = g.ends[e];
            if uf.find(a) != uf.find(b) {
                uf.union(a, b);
                in_tree[e] = true;
            }
        }
    }
    for e in 0..g.ends.len() {
        if crossed[e] || in_tree[e] || g.nm[e] {
            continue;
        }
        let [fa, fb] = g.ef[e];
        if fa == u32::MAX || fb == u32::MAX || sing[fa as usize] || sing[fb as usize] {
            continue;
        }
        // Ancêtre commun
        let (mut x, mut y) = (fa, fb);
        let mut same = true;
        while x != y {
            if depth[x as usize] < depth[y as usize] {
                std::mem::swap(&mut x, &mut y);
            }
            let p = parent[x as usize].0;
            if p == u32::MAX {
                same = false;
                break;
            }
            x = p;
        }
        if !same {
            continue;
        }
        let top = x;
        // top → … → fa (descente), traversée de e, fb → … → top (montée), puis top → racine
        let down: Vec<(u32, u8)> = {
            let ups = up(fa, top);
            // ups : (enfant, emplacement dans l'enfant) ; pour descendre il faut l'emplacement dans le parent
            let mut v: Vec<(u32, u8)> = ups.iter().map(|&(child, sy)| { let par = parent[child as usize].0; (par, slot_of(g, par, g.fe[child as usize][sy as usize])) }).collect();
            v.reverse();
            v
        };
        let mut steps = down;
        steps.push((fa, slot_of(g, fa, e as u32)));
        steps.extend(up(fb, top));
        let loop_len = steps.len();
        steps.extend(up(top, u32::MAX));
        if steps.len() >= 3 && steps[0].0 == top || loop_len == steps.len() {
            list.push((u32::MAX, steps));
        }
    }
    let saved = std::mem::replace(&mut g.d, vec![[0; 2]; g.ends.len()]);
    let mut out = vec![];
    for (c, steps) in list {
        let mut edges: Vec<u32> = steps.iter().map(|&(x, s)| g.fe[x as usize][s as usize]).collect();
        edges.sort_unstable();
        edges.dedup();
        let mut cols: Vec<(u32, [i32; 2], [i32; 2])> = edges.iter().map(|&e| (e, [0; 2], [0; 2])).collect();
        let mut valid = true;
        for (bi, b) in [[1, 0], [0, 1]].into_iter().enumerate() {
            push_path(g, f, &steps, b);
            // Une boucle doit se refermer : aucune face régulière ne garde de défaut
            if c == u32::MAX {
                let x0 = steps[0].0 as usize;
                if !sing[x0] && g.closure(&f[x0], x0).1 != [0, 0] {
                    valid = false;
                }
            }
            for col in cols.iter_mut() {
                if bi == 0 {
                    col.1 = g.d[col.0 as usize];
                } else {
                    col.2 = g.d[col.0 as usize];
                }
                g.d[col.0 as usize] = [0; 2];
            }
        }
        if valid {
            out.push(ConePath { face: c, steps, cols });
        }
    }
    g.d = saved;
    out
}

/// Pousse le vecteur `b` hors de la première face du chemin, jusqu'au bout.
fn push_path(g: &mut Graph, f: &[[u32; 3]], steps: &[(u32, u8)], b: [i32; 2]) {
    for (n, &(x, s)) in steps.iter().enumerate() {
        if n == 0 {
            g.push_vec(&f[x as usize], x as usize, s as usize, b);
        } else {
            g.push(&f[x as usize], x as usize, s as usize);
        }
    }
}

/// Aire orientée de chaque triangle dans la carte (en cases), rapportée à son aire réelle :
/// ≤ 0 = triangle replié, proche de 1 = sans déformation.
fn param_area_ratio(g: &Graph, f: &[[u32; 3]], u: &[[f64; 2]], geo_area: &[f64], fi: usize) -> f64 {
    let t = &f[fi];
    let es = g.fe[fi];
    let (k1, d1) = g.dir(es[0], t[0]);
    let (k2, d2) = g.dir(es[1], t[1]);
    let (ua, ub, uc) = (u[t[0] as usize], u[t[1] as usize], u[t[2] as usize]);
    let tb = trotf(k1, ub);
    let pb = [d1[0] as f64 + tb[0] - ua[0], d1[1] as f64 + tb[1] - ua[1]];
    let tc = trotf(k2, uc);
    let inner = trotf(k1, [d2[0] as f64 + tc[0], d2[1] as f64 + tc[1]]);
    let pc = [d1[0] as f64 + inner[0] - ua[0], d1[1] as f64 + inner[1] - ua[1]];
    0.5 * (pb[0] * pc[1] - pb[1] * pc[0]) / geo_area[fi].max(1e-12)
}

/// Raidit les arêtes autour des triangles repliés ou écrasés, puis relaxe de nouveau
/// (« stiffening » de Mixed-Integer Quadrangulation). Renvoie le nombre de replis restants.
fn stiffen(g: &mut Graph, f: &[[u32; 3]], geo: &[[f64; 2]], u: &mut [[f64; 2]], geo_area: &[f64], debug: bool) -> usize {
    let nf = f.len();
    let sing: Vec<bool> = (0..nf).map(|fi| g.closure(&f[fi], fi).0 != 0).collect();
    let mut left = 0;
    for round in 0..8 {
        let bad: Vec<usize> = (0..nf).filter(|&fi| !sing[fi] && param_area_ratio(g, f, u, geo_area, fi) < 0.15).collect();
        left = bad.len();
        if debug {
            eprintln!("  raidissement {round} : {left} triangles écrasés ou repliés");
            if round == 0 {
                let mut hist = [0usize; 8];
                for fi in 0..nf {
                    if !sing[fi] {
                        let r = param_area_ratio(g, f, u, geo_area, fi);
                        let b = if r < 0.0 { 0 } else if r < 0.15 { 1 } else if r < 0.5 { 2 } else if r < 0.8 { 3 } else if r < 1.25 { 4 } else if r < 2.0 { 5 } else if r < 4.0 { 6 } else { 7 };
                        hist[b] += 1;
                    }
                }
                eprintln!("  rapport d'aire : <0 {} | <0,15 {} | <0,5 {} | <0,8 {} | <1,25 {} | <2 {} | <4 {} | plus {}", hist[0], hist[1], hist[2], hist[3], hist[4], hist[5], hist[6], hist[7]);
            }
        }
        if bad.is_empty() {
            break;
        }
        let mut touched: HashSet<u32> = HashSet::new();
        for &fi in &bad {
            for &e in &g.fe[fi] {
                touched.insert(e);
                for &nb in &g.ef[e as usize] {
                    if nb != u32::MAX && !sing[nb as usize] {
                        touched.extend(g.fe[nb as usize]);
                    }
                }
            }
        }
        for e in touched {
            g.w[e as usize] = (g.w[e as usize] * 1.6).min(40.0);
        }
        solve_ext(g, geo, u, &[], &mut [], 1500, 1e-8);
    }
    left
}

/// Point le plus proche de `c` sur le réseau des fermetures « paires » d'une singularité.
fn nearest_even(ks: u8, c: [f64; 2]) -> [i32; 2] {
    if ks == 2 {
        return [2 * (c[0] / 2.0).round() as i32, 2 * (c[1] / 2.0).round() as i32];
    }
    let mut r = [c[0].round() as i32, c[1].round() as i32];
    if (r[0] + r[1]) & 1 != 0 {
        let (e0, e1) = (c[0] - r[0] as f64, c[1] - r[1] as f64);
        if e0.abs() > e1.abs() {
            r[0] += if e0 > 0.0 { 1 } else { -1 };
        } else {
            r[1] += if e1 > 0.0 { 1 } else { -1 };
        }
    }
    r
}

/// Place les singularités sur des points de grille, à la manière de Mixed-Integer
/// Quadrangulation (Bommes et al. 2009) : relaxation continue (singularités libres), puis
/// arrondi glouton par lots, les plus proches d'un point de grille d'abord, en relaxant
/// de nouveau entre chaque lot. Renvoie le nombre de singularités restées hors grille.
fn place_cones(g: &mut Graph, f: &[[u32; 3]], geo: &[[f64; 2]], u: &mut [[f64; 2]], geo_area: &[f64], debug: bool) -> usize {
    let paths = cone_paths(g, f);
    let mut lam = vec![[0.0f64; 2]; paths.len()];
    let mut free = vec![true; paths.len()];
    let mut round = 0;
    loop {
        let act: Vec<usize> = (0..paths.len()).filter(|&i| free[i]).collect();
        if act.is_empty() {
            break;
        }
        let refs: Vec<&ConePath> = act.iter().map(|&i| &paths[i]).collect();
        let mut l: Vec<[f64; 2]> = act.iter().map(|&i| lam[i]).collect();
        let tol: f64 = std::env::var("TOL").ok().and_then(|x| x.parse().ok()).unwrap_or(1e-7);
        let it = solve_ext(g, geo, u, &refs, &mut l, 3000, tol);
        for (k, &i) in act.iter().enumerate() {
            lam[i] = l[k];
        }
        // Vecteur entier à pousser pour chaque singularité libre, et son écart à l'optimum continu
        let mut cand: Vec<(f64, usize, [i32; 2])> = act
            .iter()
            .map(|&i| {
                if paths[i].face == u32::MAX {
                    // Anse : le décalage autour de la boucle doit être entier
                    let b = [lam[i][0].round() as i32, lam[i][1].round() as i32];
                    return (((lam[i][0] - b[0] as f64).powi(2) + (lam[i][1] - b[1] as f64).powi(2)).sqrt(), i, b);
                }
                let fi = paths[i].face as usize;
                let (ks, c, _) = g.closure(&f[fi], fi);
                let e = nearest_even(ks, [c[0] as f64 - lam[i][0], c[1] as f64 - lam[i][1]]);
                let b = [c[0] - e[0], c[1] - e[1]];
                (((lam[i][0] - b[0] as f64).powi(2) + (lam[i][1] - b[1] as f64).powi(2)).sqrt(), i, b)
            })
            .collect();
        cand.sort_by(|a, b| a.0.total_cmp(&b.0));
        let tau = cand[0].0.max(0.2);
        let take = cand.iter().take_while(|c| c.0 <= tau).count().max((cand.len() + 2) / 3).max(1);
        if debug {
            eprintln!("  lot {round} : {} libres, {it} itérations, {take} arrondies (écart max {:.2})", act.len(), cand[take - 1].0);
        }
        for &(_, i, b) in cand.iter().take(take) {
            if b != [0, 0] {
                push_path(g, f, &paths[i].steps, b);
            }
            free[i] = false;
        }
        round += 1;
    }
    solve_u(g, geo, u, 3000);
    stiffen(g, f, geo, u, geo_area, debug);
    regauge(g, u);
    (0..f.len()).filter(|&fi| { let (ks, c, _) = g.closure(&f[fi], fi); odd(ks, c) }).count()
}

/// Longueur de chaque arête en pas de grille, dans la carte de son premier sommet
/// (moyenne des repères des deux extrémités).
fn edge_geometry(g: &Graph, v: &[V3], n: &[V3], q: &[V3], h: f32) -> Vec<[f64; 2]> {
    (0..g.ends.len())
        .map(|e| {
            let (i, j) = (g.ends[e].0 as usize, g.ends[e].1 as usize);
            let d = sub(v[j], v[i]);
            let gi = [dot(d, q[i]) as f64, dot(d, cross(n[i], q[i])) as f64];
            let gj = trotf(g.k[e], [dot(d, q[j]) as f64, dot(d, cross(n[j], q[j])) as f64]);
            [(gi[0] + gj[0]) * 0.5 / h as f64, (gi[1] + gj[1]) * 0.5 / h as f64]
        })
        .collect()
}

/// Ramène chaque sommet sur son point de grille le plus proche (u dans [-½, ½]) et ajuste
/// les entiers : les fermetures des triangles ne changent pas.
fn regauge(g: &mut Graph, u: &mut [[f64; 2]]) {
    let s: Vec<[i32; 2]> = u.iter().map(|x| [x[0].round() as i32, x[1].round() as i32]).collect();
    for (x, si) in u.iter_mut().zip(&s) {
        x[0] -= si[0] as f64;
        x[1] -= si[1] as f64;
    }
    for e in 0..g.ends.len() {
        let (i, j) = (g.ends[e].0 as usize, g.ends[e].1 as usize);
        let t = trot(g.k[e], s[j]);
        g.d[e] = [g.d[e][0] - s[i][0] + t[0], g.d[e][1] - s[i][1] + t[1]];
    }
}

/// Retopologie. `guides` : traits (suites de points sur la surface).
pub fn remesh(pos: &[V3], tris: &[[u32; 3]], guides: &[Vec<V3>], opt: &Options, progress: &dyn Fn(f32)) -> Result {
    let mut stats = Stats::default();
    let t0 = std::time::Instant::now();
    let debug = std::env::var("RETOPO_DEBUG").is_ok();
    let lap = |label: &str| {
        if debug {
            eprintln!("[{:6.2}s] {label}", t0.elapsed().as_secs_f32());
        }
    };
    let (pos, tris) = weld(pos, tris);
    let area: f32 = tris.iter().map(|t| len(cross(sub(pos[t[1] as usize], pos[t[0] as usize]), sub(pos[t[2] as usize], pos[t[0] as usize]))) * 0.5).sum();
    let double = opt.pure_quads && std::env::var("X2").is_ok();
    let target = if double { (opt.faces / 4).max(4) } else { opt.faces.max(4) };
    let h = (area / target as f32).sqrt().max(1e-6);
    let (v, f) = refine(pos, tris, h / 3.0);
    let n_v = v.len();
    stats.dense = n_v;

    // Normales et voisins
    let mut nrm = vec![[0.0f32; 3]; n_v];
    let mut adj: Vec<Vec<u32>> = vec![vec![]; n_v];
    for t in &f {
        let fnrm = cross(sub(v[t[1] as usize], v[t[0] as usize]), sub(v[t[2] as usize], v[t[0] as usize]));
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            nrm[a as usize] = add(nrm[a as usize], fnrm);
            if !adj[a as usize].contains(&b) {
                adj[a as usize].push(b);
            }
            if !adj[b as usize].contains(&a) {
                adj[b as usize].push(a);
            }
        }
    }
    let nrm: Vec<V3> = nrm.into_iter().map(norm).collect();

    // Orientation : initialisation stable + contraintes des guides
    let q: Vec<V3> = (0..n_v)
        .map(|i| {
            let n = nrm[i];
            let ax = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
            norm(tangent(ax, n))
        })
        .collect();
    let mut q = q;
    let mut fixed = vec![false; n_v];
    let r = h * opt.guide_radius;
    for g in guides {
        for s in g.windows(2) {
            let (lo, hi) = ([s[0][0].min(s[1][0]) - r, s[0][1].min(s[1][1]) - r, s[0][2].min(s[1][2]) - r], [s[0][0].max(s[1][0]) + r, s[0][1].max(s[1][1]) + r, s[0][2].max(s[1][2]) + r]);
            for i in 0..n_v {
                let p = v[i];
                if p[0] < lo[0] || p[1] < lo[1] || p[2] < lo[2] || p[0] > hi[0] || p[1] > hi[1] || p[2] > hi[2] {
                    continue;
                }
                let (d, dir) = seg_dist(p, s[0], s[1]);
                if d < r && dot(nrm[i], norm(dir)).abs() < 0.95 {
                    q[i] = norm(tangent(dir, nrm[i]));
                    fixed[i] = true;
                }
            }
        }
    }

    // Hiérarchie de maillages grossiers (convergence rapide, comme Instant Meshes)
    lap("préparation");
    let (cdir, cani) = if opt.curvature > 0.0 { curvature(&v, &nrm, &adj, h) } else { (vec![[0.0; 3]; n_v], vec![0.0; n_v]) };
    let s0: f32 = std::env::var("S0").ok().and_then(|x| x.parse().ok()).unwrap_or(0.3);
    let cw: Vec<f32> = cani.iter().map(|s| s * s / (s * s + s0 * s0)).collect();
    for i in 0..n_v {
        if !fixed[i] && cw[i] > 0.05 {
            q[i] = cdir[i];
        }
    }
    let mut levels = vec![Level { v: v.clone(), n: nrm.clone(), adj: adj.clone(), c: cdir, a: cw, parent: vec![] }];
    let mean_edge = {
        let (mut t, mut c) = (0.0, 0);
        for (i, l) in adj.iter().enumerate() {
            for &j in l {
                t += len(sub(v[i], v[j as usize]));
                c += 1;
            }
        }
        t / c.max(1) as f32
    };
    let mut cell = mean_edge * 2.0;
    while levels.last().unwrap().v.len() > 300 && levels.len() < 14 {
        let c = coarsen(levels.last_mut().unwrap(), cell);
        if c.v.len() as f32 > levels.last().unwrap().v.len() as f32 * 0.8 {
            cell *= 1.6;
            continue;
        }
        levels.push(c);
        cell *= 2.0;
    }
    let iters = (opt.iterations / 4).max(10);
    let total_steps = levels.len() as f32 * 2.0;
    let mut step = 0.0;

    // Orientation : du plus grossier au plus fin
    let mut fixed_l = vec![fixed.clone()];
    let mut q_l = vec![q.clone()];
    for l in 1..levels.len() {
        let mut fx = vec![false; levels[l].v.len()];
        let mut qq: Vec<V3> = (0..levels[l].v.len())
            .map(|i| {
                let n = levels[l].n[i];
                if levels[l].a[i] > 0.02 { levels[l].c[i] } else { norm(tangent(if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] }, n)) }
            })
            .collect();
        for (i, &pa) in levels[l - 1].parent.iter().enumerate() {
            if fixed_l[l - 1][i] && !fx[pa as usize] {
                fx[pa as usize] = true;
                qq[pa as usize] = norm(tangent(q_l[l - 1][i], levels[l].n[pa as usize]));
            }
        }
        fixed_l.push(fx);
        q_l.push(qq);
    }
    let top = levels.len() - 1;
    let mut cur = q_l[top].clone();
    for l in (0..levels.len()).rev() {
        if l < top {
            // Prolongation : chaque sommet hérite de l'orientation de son parent
            let par = &levels[l].parent;
            let mut next = q_l[l].clone();
            for i in 0..next.len() {
                if !fixed_l[l][i] {
                    next[i] = norm(tangent(cur[par[i] as usize], levels[l].n[i]));
                }
            }
            cur = next;
        }
        let it = if l == top { iters * 2 } else { iters / 2 };
        smooth_orient(&levels[l], &mut cur, &fixed_l[l], it, if std::env::var("GSC").is_ok() { opt.curvature } else { 0.0 });
        step += 1.0;
        progress(step / total_steps);
    }
    let mut q = cur;
    lap("orientation (lissage local)");
    if opt.curvature > 0.0 {
        // Optimum global : lisse à l'échelle de `ell` quads, collé aux courbures marquées et aux guides
        let ell: f32 = std::env::var("ELL").ok().and_then(|x| x.parse().ok()).unwrap_or(1.5);
        let eps_k: f32 = std::env::var("EPSK").ok().and_then(|x| x.parse().ok()).unwrap_or(0.01);
        let lam = 0.87 * (mean_edge / (h * ell)).powi(2) * opt.curvature;
        let mut w: Vec<f32> = levels[0].a.iter().map(|a| a * lam).collect();
        let mut tg = levels[0].c.clone();
        for i in 0..n_v {
            if fixed[i] {
                w[i] = lam * 50.0;
                tg[i] = q[i];
            }
        }
        q = linear_field(&v, &nrm, &adj, &q, &tg, &w, lam * eps_k);
    }

    lap("orientation (optimum global)");
    // Champ de positions, même principe
    let mut o_l: Vec<Vec<V3>> = vec![v.clone()];
    let mut q_coarse = vec![q.clone()];
    for l in 1..levels.len() {
        // Orientation des niveaux grossiers = celle du premier enfant (déjà lissée au plus fin)
        let mut qq = vec![[0.0f32; 3]; levels[l].v.len()];
        let mut set = vec![false; levels[l].v.len()];
        for (i, &pa) in levels[l - 1].parent.iter().enumerate() {
            if !set[pa as usize] {
                set[pa as usize] = true;
                qq[pa as usize] = norm(tangent(q_coarse[l - 1][i], levels[l].n[pa as usize]));
            }
        }
        q_coarse.push(qq);
        o_l.push(levels[l].v.clone());
    }
    let mut cur = o_l[top].clone();
    for l in (0..levels.len()).rev() {
        if l < top {
            let par = &levels[l].parent;
            let mut next = levels[l].v.clone();
            for i in 0..next.len() {
                let po = cur[par[i] as usize];
                let (vi, ni) = (levels[l].v[i], levels[l].n[i]);
                next[i] = sub(po, mul(ni, dot(ni, sub(po, vi))));
            }
            cur = next;
        }
        let it = if l == top { iters * 2 } else { iters / 2 };
        smooth_pos(&levels[l], &q_coarse[l], &mut cur, h, it);
        step += 1.0;
        progress((step / total_steps).min(0.95));
    }
    let o = cur;

    // Entiers par arête, puis suppression des dislocations
    let mut g = Graph::new(&f);
    integers(&mut g, &v, &nrm, &q, &o, h);
    lap("champ de positions");
    let report = |g: &Graph, label: &str| {
        if debug {
            let mut hist = [0usize; 4];
            for d in &g.d {
                hist[(d[0].abs().max(d[1].abs()) as usize).min(3)] += 1;
            }
            let bad = (0..f.len()).filter(|&fi| { let (ks, c, _) = g.closure(&f[fi], fi); ks == 0 && c != [0, 0] }).count();
            eprintln!("{label} : |d|inf {hist:?}  dislocations {bad}  replis {}", count_flips(g, &f));
        }
    };
    report(&g, "avant");
    // u : coordonnées de grille de chaque sommet dans sa carte
    let mut u: Vec<[f64; 2]> = (0..n_v)
        .map(|i| {
            let d = sub(v[i], o[i]);
            [(dot(d, q[i]) / h) as f64, (dot(d, cross(nrm[i], q[i])) / h) as f64]
        })
        .collect();
    if opt.consistent {
        let (ns, nd, left) = fix_defects(&mut g, &f);
        stats.field_sing = ns;
        stats.defects = nd;
        stats.unfixed = left;
        report(&g, "après transport");
        // Les trois arêtes d'un triangle singulier comptent beaucoup plus : sinon les moindres
        // carrés « déchirent » ce triangle pour loger la singularité ailleurs dans la grille.
        let stiff: f64 = std::env::var("STIFF").ok().and_then(|x| x.parse().ok()).unwrap_or(30.0);
        for fi in 0..f.len() {
            if g.closure(&f[fi], fi).0 != 0 {
                for &e in &g.fe[fi] {
                    g.w[e as usize] = stiff.sqrt();
                }
            }
        }
        let geo = edge_geometry(&g, &v, &nrm, &q, h);
        let it = solve_u(&g, &geo, &mut u, 3000);
        regauge(&mut g, &mut u);
        if debug {
            eprintln!("gradient conjugué : {it} itérations");
        }
        report(&g, "après relaxation");
        lap("dislocations");
        if opt.pure_quads && !double {
            let geo_area: Vec<f64> = f.iter().map(|t| (len(cross(sub(v[t[1] as usize], v[t[0] as usize]), sub(v[t[2] as usize], v[t[0] as usize]))) * 0.5 / (h * h)) as f64).collect();
            let left = place_cones(&mut g, &f, &geo, &mut u, &geo_area, debug);
            if debug {
                eprintln!("singularités restées au milieu d'une case : {left}");
            }
            report(&g, "après parité");
            lap("singularités sur la grille");
        }
        if debug {
            // Résidu par arête, et replis : distance au triangle singulier le plus proche
            let mut hist = [0usize; 6];
            for e in 0..g.ends.len() {
                let (i, j) = (g.ends[e].0 as usize, g.ends[e].1 as usize);
                let t = trotf(g.k[e], u[j]);
                let r = [t[0] - u[i][0] - (geo[e][0] - g.d[e][0] as f64), t[1] - u[i][1] - (geo[e][1] - g.d[e][1] as f64)];
                let m = (r[0] * r[0] + r[1] * r[1]).sqrt();
                hist[((m * 10.0) as usize).min(5)] += 1;
            }
            let mut energy = 0.0;
            for e in 0..g.ends.len() {
                let (i, j) = (g.ends[e].0 as usize, g.ends[e].1 as usize);
                let t = trotf(g.k[e], u[j]);
                let r = [t[0] - u[i][0] - (geo[e][0] - g.d[e][0] as f64), t[1] - u[i][1] - (geo[e][1] - g.d[e][1] as f64)];
                energy += r[0] * r[0] + r[1] * r[1];
            }
            eprintln!("résidus (pas de 0,1) : {hist:?} énergie {energy:.2}");
            let sing: Vec<usize> = (0..f.len()).filter(|&fi| g.closure(&f[fi], fi).0 != 0).collect();
            for (fi, t) in f.iter().enumerate() {
                let es = g.fe[fi];
                let (k1, d1) = g.dir(es[0], t[0]);
                let (_, d2) = g.dir(es[1], t[1]);
                let r = trot(k1, d2);
                let b = [d1[0] + r[0], d1[1] + r[1]];
                if d1[0] * b[1] - d1[1] * b[0] < 0 {
                    let c = v[t[0] as usize];
                    let near = sing.iter().map(|&s| len(sub(v[f[s][0] as usize], c)) / h).fold(f32::MAX, f32::min);
                    let (ks, cc, _) = g.closure(t, fi);
                    eprintln!("  repli face {fi} : d1 {d1:?} d2' {b:?} ks {ks} C {cc:?} ; singularité la plus proche à {near:.2} h");
                }
            }
            for &s in &sing {
                let (ks, cc, _) = g.closure(&f[s], s);
                eprintln!("  singularité face {s} ks {ks} C {cc:?}");
            }
        }
    } else {
        for fi in 0..f.len() {
            let (ks, c, _) = g.closure(&f[fi], fi);
            if ks != 0 {
                stats.field_sing += 1;
            } else if c != [0, 0] {
                stats.defects += 1;
            }
        }
        stats.unfixed = stats.defects;
    }
    stats.flips = count_flips(&g, &f);
    // Point de grille de chaque sommet
    let o: Vec<V3> = (0..n_v).map(|i| sub(v[i], add(mul(q[i], u[i][0] as f32 * h), mul(cross(nrm[i], q[i]), u[i][1] as f32 * h)))).collect();
    progress(0.96);

    // Extraction : les sommets d'un même point de grille fusionnent, d = ±1 donne une arête
    let mut uf = Uf((0..n_v as u32).collect());
    let mut unit_edges = vec![];
    for e in 0..g.ends.len() {
        let d = g.d[e];
        match d[0].abs() + d[1].abs() {
            0 => uf.union(g.ends[e].0, g.ends[e].1),
            1 => unit_edges.push(g.ends[e]),
            _ => {}
        }
    }
    let mut cluster: HashMap<u32, (V3, V3, f32)> = HashMap::new();
    for i in 0..n_v {
        let r = uf.find(i as u32);
        let e = cluster.entry(r).or_insert(([0.0; 3], [0.0; 3], 0.0));
        e.0 = add(e.0, o[i]);
        e.1 = add(e.1, nrm[i]);
        e.2 += 1.0;
    }
    let mut id: HashMap<u32, u32> = HashMap::new();
    let mut out_pos: Vec<V3> = vec![];
    let mut out_n: Vec<V3> = vec![];
    let mut keys: Vec<u32> = cluster.keys().copied().collect();
    keys.sort_unstable();
    for k in keys {
        let (s, n, c) = cluster[&k];
        id.insert(k, out_pos.len() as u32);
        out_pos.push(mul(s, 1.0 / c));
        out_n.push(norm(n));
    }
    let mut edges: HashSet<(u32, u32)> = HashSet::new();
    for (a, b) in unit_edges {
        let (a, b) = (id[&uf.find(a)], id[&uf.find(b)]);
        if a != b {
            edges.insert((a.min(b), a.max(b)));
        }
    }
    let cl: Vec<u32> = (0..n_v).map(|i| id[&uf.find(i as u32)]).collect();
    let rot = rotations(&f, &cl, out_pos.len(), &edges);
    if debug {
        eprintln!("groupes {} dont {} sans ordre topologique", out_pos.len(), rot.iter().filter(|r| r.is_none()).count());
        let mut size = vec![0; out_pos.len()];
        for &c in &cl {
            size[c as usize] += 1;
        }
        let mut nsing = vec![0; out_pos.len()];
        for (fi, t) in f.iter().enumerate() {
            if g.closure(t, fi).0 != 0 {
                let cs: Vec<u32> = t.iter().map(|&x| cl[x as usize]).collect();
                eprintln!("  singularité face {fi} : groupes {cs:?} fermeture {:?} u {:?}", g.closure(t, fi).1, t.iter().map(|&x| [(u[x as usize][0] * 100.0).round() / 100.0, (u[x as usize][1] * 100.0).round() / 100.0]).collect::<Vec<_>>());
                for c in cs {
                    nsing[c as usize] += 1;
                }
            }
        }
        let mut deg = vec![0; out_pos.len()];
        for &(a, b) in &edges {
            deg[a as usize] += 1;
            deg[b as usize] += 1;
        }
        for k in 0..out_pos.len() {
            if rot[k].is_none() || deg[k] != 4 {
                eprintln!("  groupe {k} : {} sommets, degré {}, ordre {}, coins de singularité {}", size[k], deg[k], rot[k].is_some(), nsing[k]);
            }
        }
    }
    let faces = extract_faces(&out_pos, &out_n, &edges, &rot);
    if debug {
        let mut hist = std::collections::BTreeMap::new();
        for f in &faces {
            *hist.entry(f.len()).or_insert(0) += 1;
        }
        let mut deg = std::collections::BTreeMap::new();
        let mut dv = vec![0; out_pos.len()];
        for &(a, b) in &edges {
            dv[a as usize] += 1;
            dv[b as usize] += 1;
        }
        for d in dv {
            *deg.entry(d).or_insert(0) += 1;
        }
        eprintln!("faces par taille {hist:?} ; degrés {deg:?} ; arêtes {} ; euler V-E+F = {}", edges.len(), out_pos.len() as i64 - edges.len() as i64 + faces.len() as i64);
    }
    let faces = if opt.pure_quads && !double { cleanup_quads(&mut out_pos, faces) } else { faces };
    let all_quads = faces.iter().all(|f| f.len() == 4);
    if debug {
        eprintln!("après nettoyage : {} faces dont {} quads", faces.len(), faces.iter().filter(|f| f.len() == 4).count());
    }
    let faces = if opt.pure_quads { faces } else { quadify(&out_pos, faces) };
    // Ne garder que les sommets utilisés
    let mut remap = vec![u32::MAX; out_pos.len()];
    let mut pos2 = vec![];
    let faces: Vec<Vec<u32>> = faces
        .into_iter()
        .map(|f| {
            f.into_iter()
                .map(|i| {
                    if remap[i as usize] == u32::MAX {
                        remap[i as usize] = pos2.len() as u32;
                        pos2.push(out_pos[i as usize]);
                    }
                    remap[i as usize]
                })
                .collect()
        })
        .collect();
    lap("extraction");
    let mut res = Result { pos: pos2, faces, stats };
    relax(&mut res, &v, &f, 3);
    if opt.pure_quads && (double || !all_quads) {
        res = split_quads(&res);
        relax(&mut res, &v, &f, 2);
    }
    lap("lissage final");
    progress(1.0);
    res
}

struct Level {
    v: Vec<V3>,
    n: Vec<V3>,
    adj: Vec<Vec<u32>>,
    /// Direction souhaitée (courbure principale) et son poids (0 à 1)
    c: Vec<V3>,
    a: Vec<f32>,
    /// Pour chaque sommet de ce niveau : son parent dans le niveau plus grossier
    parent: Vec<u32>,
}

/// Regroupe les sommets par cellule de grille (et par orientation de normale, pour ne pas
/// fusionner les deux faces d'une paroi mince).
fn coarsen(fine: &mut Level, cell: f32) -> Level {
    let mut map: HashMap<[i32; 4], u32> = HashMap::new();
    let mut v: Vec<V3> = vec![];
    let mut n: Vec<V3> = vec![];
    let mut cnt: Vec<f32> = vec![];
    let mut parent = Vec::with_capacity(fine.v.len());
    for i in 0..fine.v.len() {
        let p = fine.v[i];
        let nn = fine.n[i];
        let ax = (0..3).max_by(|&a, &b| nn[a].abs().total_cmp(&nn[b].abs())).unwrap();
        let dir = ax as i32 * 2 + (nn[ax] > 0.0) as i32;
        let k = [(p[0] / cell).floor() as i32, (p[1] / cell).floor() as i32, (p[2] / cell).floor() as i32, dir];
        let id = *map.entry(k).or_insert_with(|| {
            v.push([0.0; 3]);
            n.push([0.0; 3]);
            cnt.push(0.0);
            (v.len() - 1) as u32
        });
        v[id as usize] = add(v[id as usize], p);
        n[id as usize] = add(n[id as usize], nn);
        cnt[id as usize] += 1.0;
        parent.push(id);
    }
    for i in 0..v.len() {
        v[i] = mul(v[i], 1.0 / cnt[i]);
        n[i] = norm(n[i]);
    }
    let mut adj: Vec<HashSet<u32>> = vec![HashSet::new(); v.len()];
    for (i, l) in fine.adj.iter().enumerate() {
        for &j in l {
            let (a, b) = (parent[i], parent[j as usize]);
            if a != b {
                adj[a as usize].insert(b);
            }
        }
    }
    // Direction souhaitée : moyenne 4-symétrique de celles des enfants
    let mut c = vec![[0.0f32; 3]; v.len()];
    let mut a = vec![0.0f32; v.len()];
    for i in 0..fine.v.len() {
        let p = parent[i] as usize;
        let w = fine.a[i];
        if w <= 0.0 {
            continue;
        }
        let ci = tangent(fine.c[i], n[p]);
        if dot(ci, ci) < 1e-12 {
            continue;
        }
        let ci = norm(ci);
        if a[p] <= 0.0 {
            c[p] = ci;
        } else {
            let (x, y) = compat_orient(c[p], n[p], ci, n[p]);
            c[p] = norm(tangent(add(mul(x, a[p]), mul(y, w)), n[p]));
        }
        a[p] += w;
    }
    for i in 0..v.len() {
        a[i] /= cnt[i];
    }
    fine.parent = parent;
    let mut adj: Vec<Vec<u32>> = adj.into_iter().map(|s| s.into_iter().collect()).collect();
    for l in adj.iter_mut() {
        l.sort_unstable();
    }
    Level { v, n, adj, c, a, parent: vec![] }
}

fn visit_order(n: usize) -> Vec<u32> {
    let mut order: Vec<u32> = (0..n as u32).collect();
    let mut seed = 0x9e3779b9u32;
    for i in (1..order.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        order.swap(i, seed as usize % (i + 1));
    }
    order
}

fn smooth_orient(l: &Level, q: &mut [V3], fixed: &[bool], iters: usize, strength: f32) {
    let order = visit_order(l.v.len());
    for _ in 0..iters {
        for &i in &order {
            let i = i as usize;
            if fixed[i] {
                continue;
            }
            let ni = l.n[i];
            let mut qi = q[i];
            let mut ws = 0.0;
            for &j in &l.adj[i] {
                let (a, b) = compat_orient(qi, ni, q[j as usize], l.n[j as usize]);
                qi = add(mul(a, ws), b);
                ws += 1.0;
                qi = norm(tangent(qi, ni));
            }
            if strength > 0.0 && l.a[i] > 0.0 {
                let w = strength * l.a[i] * ws.max(1.0);
                let (a, b) = compat_orient(qi, ni, l.c[i], ni);
                qi = norm(tangent(add(mul(a, ws.max(1.0)), mul(b, w)), ni));
            }
            q[i] = qi;
        }
    }
}

/// Champ de croix le plus lisse qui respecte les directions souhaitées (d'après Knöppel et
/// al. 2013, « Globally optimal direction fields »). Chaque croix est un nombre complexe
/// z = e^{4iθ} dans le repère de son sommet ; on résout (L + D) z = D z0 par gradient conjugué,
/// L étant le laplacien de connexion. Contrairement au lissage local, c'est un optimum global :
/// beaucoup moins de paires de singularités parasites.
fn linear_field(v: &[V3], n: &[V3], adj: &[Vec<u32>], base: &[V3], target: &[V3], weight: &[f32], eps: f32) -> Vec<V3> {
    let nv = v.len();
    let b2: Vec<V3> = (0..nv).map(|i| cross(n[i], base[i])).collect();
    // Transport e^{4iφ} du repère de j vers celui de i
    let mut off = vec![0usize; nv + 1];
    for i in 0..nv {
        off[i + 1] = off[i] + adj[i].len();
    }
    let mut r = vec![[0.0f64; 2]; off[nv]];
    for i in 0..nv {
        for (s, &j) in adj[i].iter().enumerate() {
            let j = j as usize;
            let (ax, c) = (cross(n[j], n[i]), dot(n[j], n[i]));
            let x = base[j];
            let xr = if c > -0.99 { add(add(mul(x, c), cross(ax, x)), mul(ax, dot(ax, x) / (1.0 + c))) } else { tangent(x, n[i]) };
            let phi = 4.0 * (dot(xr, b2[i]) as f64).atan2(dot(xr, base[i]) as f64);
            r[off[i] + s] = [phi.cos(), phi.sin()];
        }
    }
    let diag: Vec<f64> = (0..nv).map(|i| adj[i].len() as f64 + weight[i] as f64 + eps as f64).collect();
    let apply = |z: &[[f64; 2]], out: &mut [[f64; 2]]| {
        for i in 0..nv {
            let mut acc = [diag[i] * z[i][0], diag[i] * z[i][1]];
            for (s, &j) in adj[i].iter().enumerate() {
                let (rr, zj) = (r[off[i] + s], z[j as usize]);
                acc[0] -= rr[0] * zj[0] - rr[1] * zj[1];
                acc[1] -= rr[0] * zj[1] + rr[1] * zj[0];
            }
            out[i] = acc;
        }
    };
    // Second membre : directions souhaitées + un rappel très faible vers le champ de départ (z = 1)
    let rhs: Vec<[f64; 2]> = (0..nv)
        .map(|i| {
            let w = weight[i] as f64;
            if w > 0.0 {
                let th = 4.0 * (dot(target[i], b2[i]) as f64).atan2(dot(target[i], base[i]) as f64);
                [w * th.cos() + eps as f64, w * th.sin()]
            } else {
                [eps as f64, 0.0]
            }
        })
        .collect();
    let dotv = |a: &[[f64; 2]], b: &[[f64; 2]]| -> f64 { a.iter().zip(b).map(|(x, y)| x[0] * y[0] + x[1] * y[1]).sum() };
    let mut z = vec![[1.0f64, 0.0]; nv];
    let mut az = vec![[0.0f64; 2]; nv];
    apply(&z, &mut az);
    let mut res: Vec<[f64; 2]> = rhs.iter().zip(&az).map(|(b, a)| [b[0] - a[0], b[1] - a[1]]).collect();
    let mut pz: Vec<[f64; 2]> = res.iter().zip(&diag).map(|(x, d)| [x[0] / d, x[1] / d]).collect();
    let mut p = pz.clone();
    let mut rz = dotv(&res, &pz);
    let stop = dotv(&rhs, &rhs) * 1e-10;
    let mut it = 0;
    while it < 4000 && dotv(&res, &res) > stop {
        apply(&p, &mut az);
        let alpha = rz / dotv(&p, &az).max(1e-300);
        for i in 0..nv {
            z[i][0] += alpha * p[i][0];
            z[i][1] += alpha * p[i][1];
            res[i][0] -= alpha * az[i][0];
            res[i][1] -= alpha * az[i][1];
            pz[i] = [res[i][0] / diag[i], res[i][1] / diag[i]];
        }
        let rz2 = dotv(&res, &pz);
        let beta = rz2 / rz;
        rz = rz2;
        for i in 0..nv {
            p[i] = [pz[i][0] + beta * p[i][0], pz[i][1] + beta * p[i][1]];
        }
        it += 1;
    }
    if std::env::var("RETOPO_DEBUG").is_ok() {
        eprintln!("champ linéaire : {it} itérations");
    }
    (0..nv)
        .map(|i| {
            let th = z[i][1].atan2(z[i][0]) / 4.0;
            norm(add(mul(base[i], th.cos() as f32), mul(b2[i], th.sin() as f32)))
        })
        .collect()
}

/// Directions principales de courbure et anisotropie |κ1 − κ2|·h (sans unité) par sommet.
/// Tenseur de Taubin par anneau, lissé à l'échelle d'un quad pour ignorer le bruit du sculpt.
fn curvature(v: &[V3], n: &[V3], adj: &[Vec<u32>], h: f32) -> (Vec<V3>, Vec<f32>) {
    let nv = v.len();
    let mut t = vec![[0.0f32; 6]; nv];
    let (mut total, mut count) = (0.0f64, 0usize);
    for i in 0..nv {
        let mut acc = [0.0f32; 6];
        let mut w = 0.0;
        for &j in &adj[i] {
            let j = j as usize;
            let e = sub(v[j], v[i]);
            let l2 = dot(e, e);
            if l2 < 1e-20 {
                continue;
            }
            total += l2.sqrt() as f64;
            count += 1;
            let d = tangent(e, n[i]);
            let dl = dot(d, d);
            if dl < 1e-20 {
                continue;
            }
            let d = mul(d, 1.0 / dl.sqrt());
            let k = dot(sub(n[j], n[i]), e) / l2;
            acc[0] += k * d[0] * d[0];
            acc[1] += k * d[1] * d[1];
            acc[2] += k * d[2] * d[2];
            acc[3] += k * d[0] * d[1];
            acc[4] += k * d[0] * d[2];
            acc[5] += k * d[1] * d[2];
            w += 1.0;
        }
        if w > 0.0 {
            for x in acc.iter_mut() {
                *x /= w;
            }
        }
        t[i] = acc;
    }
    let mean_edge = (total / count.max(1) as f64) as f32;
    let ps: f32 = std::env::var("PS").ok().and_then(|x| x.parse().ok()).unwrap_or(1.0);
    let passes = (((h / mean_edge.max(1e-9)).powi(2) * ps).ceil() as usize).clamp(2, 400);
    for _ in 0..passes {
        let old = t.clone();
        for i in 0..nv {
            if adj[i].is_empty() {
                continue;
            }
            let mut acc = [0.0f32; 6];
            for &j in &adj[i] {
                for k in 0..6 {
                    acc[k] += old[j as usize][k];
                }
            }
            let inv = 0.5 / adj[i].len() as f32;
            for k in 0..6 {
                t[i][k] = 0.5 * old[i][k] + acc[k] * inv;
            }
        }
    }
    let mut dir = vec![[0.0f32; 3]; nv];
    let mut ani = vec![0.0f32; nv];
    for i in 0..nv {
        let nn = n[i];
        let b1 = norm(tangent(if nn[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] }, nn));
        let b2 = cross(nn, b1);
        let m = t[i];
        let tm = |a: V3, b: V3| -> f32 { a[0] * (m[0] * b[0] + m[3] * b[1] + m[4] * b[2]) + a[1] * (m[3] * b[0] + m[1] * b[1] + m[5] * b[2]) + a[2] * (m[4] * b[0] + m[5] * b[1] + m[2] * b[2]) };
        let (m11, m12, m22) = (tm(b1, b1), tm(b1, b2), tm(b2, b2));
        let th = 0.5 * (2.0 * m12).atan2(m11 - m22);
        dir[i] = add(mul(b1, th.cos()), mul(b2, th.sin()));
        ani[i] = 8.0 * (((m11 - m22) * 0.5).powi(2) + m12 * m12).sqrt() * h;
    }
    (dir, ani)
}

fn smooth_pos(l: &Level, q: &[V3], o: &mut [V3], h: f32, iters: usize) {
    let order = visit_order(l.v.len());
    for _ in 0..iters {
        for &i in &order {
            let i = i as usize;
            let (vi, ni, qi) = (l.v[i], l.n[i], q[i]);
            let mut sum = o[i];
            let mut ws = 0.0;
            for &j in &l.adj[i] {
                let j = j as usize;
                let (a, b) = compat_pos(vi, ni, qi, sum, l.v[j], l.n[j], q[j], o[j], h);
                sum = add(mul(a, ws), b);
                ws += 1.0;
                sum = mul(sum, 1.0 / ws);
            }
            sum = sub(sum, mul(ni, dot(ni, sub(sum, vi))));
            o[i] = round4(sum, qi, ni, vi, h);
        }
    }
}

/// Ordre cyclique (sens trigonométrique) des voisins de chaque point de grille, lu sur la
/// topologie du maillage dense : autour d'un groupe K, deux sommets extérieurs consécutifs
/// x → y forment un triangle (a, x, y) avec a dans K. C'est exact même quand la géométrie
/// est pincée ou repliée, contrairement à un tri par angles.
fn rotations(f: &[[u32; 3]], cl: &[u32], n: usize, edges: &HashSet<(u32, u32)>) -> Vec<Option<Vec<u32>>> {
    let mut arcs: Vec<HashMap<u32, u32>> = vec![HashMap::new(); n];
    let mut bad = vec![false; n];
    for t in f {
        for r in 0..3 {
            let (k, x, y) = (cl[t[r] as usize], cl[t[(r + 1) % 3] as usize], cl[t[(r + 2) % 3] as usize]);
            if x != k && y != k && x != y {
                if let Some(old) = arcs[k as usize].insert(x, y) {
                    if old != y {
                        bad[k as usize] = true;
                    }
                }
            }
        }
    }
    let mut unit: Vec<Vec<u32>> = vec![vec![]; n];
    for &(a, b) in edges {
        unit[a as usize].push(b);
        unit[b as usize].push(a);
    }
    (0..n)
        .map(|k| {
            if bad[k] || unit[k].is_empty() {
                return None;
            }
            let is_unit: HashSet<u32> = unit[k].iter().copied().collect();
            // Départ : un voisin sans prédécesseur (bord du maillage), sinon n'importe lequel
            let targets: HashSet<u32> = arcs[k].values().copied().collect();
            let start = arcs[k].keys().copied().filter(|x| !targets.contains(x)).min().or_else(|| unit[k].iter().copied().min())?;
            let mut order = vec![];
            let mut cur = start;
            for _ in 0..arcs[k].len() + 2 {
                if is_unit.contains(&cur) {
                    order.push(cur);
                }
                match arcs[k].get(&cur) {
                    Some(&nx) if nx != start => cur = nx,
                    _ => break,
                }
            }
            (order.len() == unit[k].len() && order.iter().collect::<HashSet<_>>().len() == order.len()).then_some(order)
        })
        .collect()
}

/// Faces lues en tournant autour de chaque sommet. `rot` : ordre topologique des voisins
/// quand il est connu, sinon tri par angle dans le plan tangent.
fn extract_faces(pos: &[V3], nrm: &[V3], edges: &HashSet<(u32, u32)>, rot: &[Option<Vec<u32>>]) -> Vec<Vec<u32>> {
    let n = pos.len();
    let mut nb: Vec<Vec<u32>> = vec![vec![]; n];
    for &(a, b) in edges {
        nb[a as usize].push(b);
        nb[b as usize].push(a);
    }
    for i in 0..n {
        if let Some(Some(r)) = rot.get(i) {
            nb[i] = r.clone();
            continue;
        }
        nb[i].sort_unstable();
        let nn = nrm[i];
        let first = nb[i].first().map(|&j| norm(tangent(sub(pos[j as usize], pos[i]), nn)));
        let Some(x) = first else { continue };
        let y = cross(nn, x);
        let p = pos[i];
        nb[i].sort_by(|&a, &b| {
            let ang = |j: u32| {
                let d = sub(pos[j as usize], p);
                dot(d, y).atan2(dot(d, x))
            };
            ang(a).total_cmp(&ang(b))
        });
    }
    // Arête suivante : en arrivant en v depuis u, prendre le voisin qui précède u (sens horaire)
    let next = |u: u32, v: u32| -> Option<u32> {
        let l = &nb[v as usize];
        let k = l.iter().position(|&x| x == u)?;
        Some(l[(k + l.len() - 1) % l.len()])
    };
    let mut used: HashSet<(u32, u32)> = HashSet::new();
    let mut faces = vec![];
    for a in 0..n as u32 {
        for &b in &nb[a as usize].clone() {
            if used.contains(&(a, b)) {
                continue;
            }
            let mut f = vec![a];
            let (mut u, mut v) = (a, b);
            let mut ok = false;
            for _ in 0..33 {
                if v == a {
                    ok = true;
                    break;
                }
                f.push(v);
                let Some(w) = next(u, v) else { break };
                u = v;
                v = w;
            }
            if ok && (3..=32).contains(&f.len()) {
                for k in 0..f.len() {
                    used.insert((f[k], f[(k + 1) % f.len()]));
                }
                faces.push(f);
            }
        }
    }
    // Doublons (même face trouvée deux fois dans des sens opposés)
    let mut seen = HashSet::new();
    faces.retain(|f| {
        let mut k = f.clone();
        k.sort();
        seen.insert(k)
    });
    faces
}

// ---------------------------------------------------------------------------------------
// Nettoyage topologique : garantit des quads purs sans quadrupler le nombre de faces.
// ---------------------------------------------------------------------------------------

/// Retire les sommets répétés ; une face pincée (a b c a d e) est coupée en deux.
fn sanitize(faces: Vec<Vec<u32>>) -> Vec<Vec<u32>> {
    let mut out = vec![];
    let mut stack = faces;
    while let Some(mut f) = stack.pop() {
        f.dedup();
        while f.len() > 1 && f.first() == f.last() {
            f.pop();
        }
        if f.len() < 3 {
            continue;
        }
        let mut seen: HashMap<u32, usize> = HashMap::new();
        let mut split = None;
        for (i, &v) in f.iter().enumerate() {
            if let Some(&j) = seen.get(&v) {
                split = Some((j, i));
                break;
            }
            seen.insert(v, i);
        }
        match split {
            Some((j, i)) => {
                let inner: Vec<u32> = f[j..i].to_vec();
                let mut outer: Vec<u32> = f[..j].to_vec();
                outer.extend_from_slice(&f[i..]);
                stack.push(inner);
                stack.push(outer);
            }
            None => out.push(f),
        }
    }
    out
}

/// Sommets intérieurs de valence 2 (« doublets ») : les deux faces qui les entourent fusionnent.
fn remove_doublets(faces: &mut Vec<Vec<u32>>) -> bool {
    let mut inc: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
    for (fi, f) in faces.iter().enumerate() {
        for (k, &v) in f.iter().enumerate() {
            inc.entry(v).or_default().push((fi as u32, k as u32));
        }
    }
    let mut dead = vec![false; faces.len()];
    let mut added = vec![];
    let mut keys: Vec<u32> = inc.keys().copied().collect();
    keys.sort_unstable();
    for v in keys {
        let l = &inc[&v];
        if l.len() != 2 || l[0].0 == l[1].0 || dead[l[0].0 as usize] || dead[l[1].0 as usize] {
            continue;
        }
        let (f1, f2) = (&faces[l[0].0 as usize], &faces[l[1].0 as usize]);
        let (n1, n2) = (f1.len(), f2.len());
        let (k1, k2) = (l[0].1 as usize, l[1].1 as usize);
        let (a1, b1) = (f1[(k1 + n1 - 1) % n1], f1[(k1 + 1) % n1]);
        let (a2, b2) = (f2[(k2 + n2 - 1) % n2], f2[(k2 + 1) % n2]);
        if a1 != b2 || b1 != a2 {
            continue;
        }
        // f1 = (v, b1, X…, a1) ; f2 = (v, a1, Y…, b1) → (b1, X…, a1, Y…)
        let mut m = vec![];
        for s in 1..n1 {
            m.push(f1[(k1 + s) % n1]);
        }
        for s in 2..n2 - 1 {
            m.push(f2[(k2 + s) % n2]);
        }
        dead[l[0].0 as usize] = true;
        dead[l[1].0 as usize] = true;
        added.push(m);
    }
    if added.is_empty() {
        return false;
    }
    let mut out: Vec<Vec<u32>> = faces.drain(..).enumerate().filter(|(i, _)| !dead[*i]).map(|(_, f)| f).collect();
    out.extend(added);
    *faces = sanitize(out);
    true
}

/// Polygones pairs de 6 côtés ou plus : éventail de quads depuis le meilleur coin.
fn split_even(pos: &[V3], faces: Vec<Vec<u32>>) -> Vec<Vec<u32>> {
    let mut out = Vec::with_capacity(faces.len());
    for f in faces {
        let n = f.len();
        if n < 6 || n % 2 == 1 {
            out.push(f);
            continue;
        }
        let cost = |k: usize| -> f32 { (1..n / 2 - 1 + 1).take(n / 2 - 1).map(|q| len(sub(pos[f[k] as usize], pos[f[(k + 2 * q + 1) % n] as usize]))).sum() };
        let k = (0..n).min_by(|&a, &b| cost(a).total_cmp(&cost(b))).unwrap();
        for q in 0..n / 2 - 1 {
            out.push(vec![f[k], f[(k + 2 * q + 1) % n], f[(k + 2 * q + 2) % n], f[(k + 2 * q + 3) % n]]);
        }
    }
    out
}

/// Relie les faces impaires deux à deux (ou à un bord) par un chemin de faces ; chaque arête
/// traversée reçoit un sommet au milieu : un triangle devient un quad, un pentagone un hexagone,
/// et les quads traversés restent des quads.
fn pair_odd(pos: &mut Vec<V3>, mut faces: Vec<Vec<u32>>) -> Vec<Vec<u32>> {
    for _ in 0..faces.len() {
        let Some(start) = faces.iter().position(|f| f.len() % 2 == 1) else { break };
        let mut by_edge: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        for (fi, f) in faces.iter().enumerate() {
            for k in 0..f.len() {
                let (a, b) = (f[k], f[(k + 1) % f.len()]);
                by_edge.entry((a.min(b), a.max(b))).or_default().push(fi as u32);
            }
        }
        // Parcours en largeur jusqu'à une autre face impaire ou un bord
        let mut prev: HashMap<u32, (u32, (u32, u32))> = HashMap::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(start as u32);
        prev.insert(start as u32, (u32::MAX, (0, 0)));
        let mut end: Option<(u32, Option<(u32, u32)>)> = None;
        'bfs: while let Some(x) = queue.pop_front() {
            let f = &faces[x as usize];
            for k in 0..f.len() {
                let (a, b) = (f[k], f[(k + 1) % f.len()]);
                let key = (a.min(b), a.max(b));
                let l = &by_edge[&key];
                if l.len() == 1 {
                    if x as usize != start || f.len() > 3 {
                        end = Some((x, Some(key)));
                        break 'bfs;
                    }
                    continue;
                }
                if l.len() != 2 {
                    continue;
                }
                let y = if l[0] == x { l[1] } else { l[0] };
                if y == x || prev.contains_key(&y) {
                    continue;
                }
                prev.insert(y, (x, key));
                if faces[y as usize].len() % 2 == 1 {
                    end = Some((y, None));
                    break 'bfs;
                }
                if faces[y as usize].len() == 4 {
                    queue.push_back(y);
                }
            }
        }
        let Some((last, border)) = end else { break };
        // Arêtes traversées, de la fin vers le début
        let mut chain: Vec<(u32, (u32, u32))> = vec![];
        let mut cur = last;
        while cur as usize != start {
            let (p, key) = prev[&cur];
            chain.push((cur, key));
            cur = p;
        }
        chain.reverse();
        // chain[i] = (face atteinte, arête d'entrée) ; la face de départ sort par chain[0].1
        let mut mids: HashMap<(u32, u32), u32> = HashMap::new();
        let mut mid = |key: (u32, u32), pos: &mut Vec<V3>| -> u32 {
            *mids.entry(key).or_insert_with(|| {
                pos.push(mul(add(pos[key.0 as usize], pos[key.1 as usize]), 0.5));
                (pos.len() - 1) as u32
            })
        };
        let slot = |f: &[u32], key: (u32, u32)| -> usize { (0..f.len()).find(|&k| { let (a, b) = (f[k], f[(k + 1) % f.len()]); (a.min(b), a.max(b)) == key }).unwrap_or(0) };
        let insert = |f: &[u32], key: (u32, u32), m: u32| -> Vec<u32> {
            let k = slot(f, key);
            let mut o = f[..=k].to_vec();
            o.push(m);
            o.extend_from_slice(&f[k + 1..]);
            o
        };
        // Sorties de chaque face du chemin
        let mut path: Vec<(u32, Option<(u32, u32)>, Option<(u32, u32)>)> = vec![];
        let mut inn = None;
        let mut face = start as u32;
        for &(next, key) in &chain {
            path.push((face, inn, Some(key)));
            inn = Some(key);
            face = next;
        }
        path.push((face, inn, border));
        let mut new_faces = vec![];
        let mut dead: HashSet<u32> = HashSet::new();
        for (fi, e_in, e_out) in path {
            let f = faces[fi as usize].clone();
            dead.insert(fi);
            match (e_in, e_out) {
                (Some(a), Some(b)) if f.len() == 4 => {
                    let (mi, mo) = (mid(a, pos), mid(b, pos));
                    let i = slot(&f, a);
                    let v = [f[i], f[(i + 1) % 4], f[(i + 2) % 4], f[(i + 3) % 4]];
                    match (slot(&f, b) + 4 - i) % 4 {
                        2 => {
                            new_faces.push(vec![v[0], mi, mo, v[3]]);
                            new_faces.push(vec![mi, v[1], v[2], mo]);
                        }
                        r => {
                            let c = mul(add(add(pos[v[0] as usize], pos[v[1] as usize]), add(pos[v[2] as usize], pos[v[3] as usize])), 0.25);
                            pos.push(c);
                            let c = (pos.len() - 1) as u32;
                            if r == 1 {
                                new_faces.push(vec![v[0], mi, c, v[3]]);
                                new_faces.push(vec![mi, v[1], mo, c]);
                                new_faces.push(vec![c, mo, v[2], v[3]]);
                            } else {
                                new_faces.push(vec![v[0], mi, c, mo]);
                                new_faces.push(vec![mi, v[1], v[2], c]);
                                new_faces.push(vec![c, v[2], v[3], mo]);
                            }
                        }
                    }
                }
                (a, b) => {
                    let mut g = f;
                    if let Some(a) = a {
                        g = insert(&g, a, mid(a, pos));
                    }
                    if let Some(b) = b {
                        g = insert(&g, b, mid(b, pos));
                    }
                    new_faces.push(g);
                }
            }
        }
        let mut out: Vec<Vec<u32>> = faces.into_iter().enumerate().filter(|(i, _)| !dead.contains(&(*i as u32))).map(|(_, f)| f).collect();
        out.extend(new_faces);
        faces = split_even(pos, out);
    }
    faces
}

/// Doublets, polygones pairs, puis faces impaires : il ne reste que des quads (sauf cas dégénéré).
fn cleanup_quads(pos: &mut Vec<V3>, faces: Vec<Vec<u32>>) -> Vec<Vec<u32>> {
    let mut faces = sanitize(faces);
    for _ in 0..12 {
        if !remove_doublets(&mut faces) {
            break;
        }
    }
    let faces = split_even(pos, faces);
    let mut faces = pair_odd(pos, faces);
    for _ in 0..12 {
        if !remove_doublets(&mut faces) {
            break;
        }
    }
    split_even(pos, faces)
}

/// Chaque face à n côtés devient n quads (centre + milieux des arêtes).
fn split_quads(r: &Result) -> Result {
    let mut pos = r.pos.clone();
    let mut mid: HashMap<(u32, u32), u32> = HashMap::new();
    let mut faces = Vec::with_capacity(r.faces.len() * 4);
    for f in &r.faces {
        let n = f.len();
        let c = f.iter().fold([0.0; 3], |a, &i| add(a, r.pos[i as usize]));
        pos.push(mul(c, 1.0 / n as f32));
        let ci = (pos.len() - 1) as u32;
        let m: Vec<u32> = (0..n)
            .map(|k| {
                let (a, b) = (f[k], f[(k + 1) % n]);
                *mid.entry((a.min(b), a.max(b))).or_insert_with(|| {
                    pos.push(mul(add(r.pos[a as usize], r.pos[b as usize]), 0.5));
                    (pos.len() - 1) as u32
                })
            })
            .collect();
        for k in 0..n {
            faces.push(vec![f[k], m[k], ci, m[(k + n - 1) % n]]);
        }
    }
    Result { pos, faces, stats: r.stats.clone() }
}

/// Fusionne les paires de triangles qui partagent leur plus longue arête (diagonale d'un quad).
fn quadify(pos: &[V3], faces: Vec<Vec<u32>>) -> Vec<Vec<u32>> {
    let elen = |a: u32, b: u32| len(sub(pos[a as usize], pos[b as usize]));
    let mut by_edge: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (fi, f) in faces.iter().enumerate() {
        if f.len() == 3 {
            for k in 0..3 {
                let (a, b) = (f[k], f[(k + 1) % 3]);
                by_edge.entry((a.min(b), a.max(b))).or_default().push(fi);
            }
        }
    }
    let longest = |f: &Vec<u32>| -> (u32, u32) {
        let mut best = (0.0, (0, 0));
        for k in 0..3 {
            let (a, b) = (f[k], f[(k + 1) % 3]);
            let l = elen(a, b);
            if l > best.0 {
                best = (l, (a.min(b), a.max(b)));
            }
        }
        best.1
    };
    // Candidats triés par longueur d'arête décroissante (les vraies diagonales d'abord)
    let mut cands: Vec<((u32, u32), usize, usize)> = by_edge
        .iter()
        .filter(|(_, v)| v.len() == 2)
        .filter(|(e, v)| longest(&faces[v[0]]) == **e || longest(&faces[v[1]]) == **e)
        .map(|(e, v)| (*e, v[0], v[1]))
        .collect();
    cands.sort_by(|a, b| elen(b.0 .0, b.0 .1).total_cmp(&elen(a.0 .0, a.0 .1)));
    let mut gone = vec![false; faces.len()];
    let mut out = vec![];
    for ((a, b), f1, f2) in cands {
        if gone[f1] || gone[f2] {
            continue;
        }
        let t1 = &faces[f1];
        let c = *faces[f2].iter().find(|&&x| x != a && x != b).unwrap();
        // Insérer c entre a et b dans t1, en respectant le sens de t1
        let k = (0..3).find(|&k| {
            let (x, y) = (t1[k], t1[(k + 1) % 3]);
            (x == a && y == b) || (x == b && y == a)
        });
        let Some(k) = k else { continue };
        let q = vec![t1[k], c, t1[(k + 1) % 3], t1[(k + 2) % 3]];
        // Quad convexe seulement (angles intérieurs < 165°)
        let convex = (0..4).all(|i| {
            let (p0, p1, p2) = (pos[q[(i + 3) % 4] as usize], pos[q[i] as usize], pos[q[(i + 1) % 4] as usize]);
            dot(norm(sub(p0, p1)), norm(sub(p2, p1))) > -0.966
        });
        if !convex {
            continue;
        }
        gone[f1] = true;
        gone[f2] = true;
        out.push(q);
    }
    out.extend(faces.into_iter().enumerate().filter(|(i, _)| !gone[*i]).map(|(_, f)| f));
    out
}

/// Lissage léger puis reprojection sur la surface d'origine (sommet dense le plus proche).
fn relax(r: &mut Result, v: &[V3], f: &[[u32; 3]], iters: usize) {
    let n = r.pos.len();
    let mut nb: Vec<HashSet<u32>> = vec![HashSet::new(); n];
    for face in &r.faces {
        for k in 0..face.len() {
            let (a, b) = (face[k], face[(k + 1) % face.len()]);
            nb[a as usize].insert(b);
            nb[b as usize].insert(a);
        }
    }
    // Grille spatiale des triangles denses pour la reprojection
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in v {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let cell = ((hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]) / 48.0).max(1e-6);
    let key = |p: V3| [((p[0] - lo[0]) / cell) as i32, ((p[1] - lo[1]) / cell) as i32, ((p[2] - lo[2]) / cell) as i32];
    let mut grid: HashMap<[i32; 3], Vec<u32>> = HashMap::new();
    for (ti, t) in f.iter().enumerate() {
        let c = mul(add(add(v[t[0] as usize], v[t[1] as usize]), v[t[2] as usize]), 1.0 / 3.0);
        grid.entry(key(c)).or_default().push(ti as u32);
    }
    let project = |p: V3| -> V3 {
        let k = key(p);
        let mut best = (f32::MAX, p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(l) = grid.get(&[k[0] + dx, k[1] + dy, k[2] + dz]) {
                        for &ti in l {
                            let t = f[ti as usize];
                            let q = closest_on_tri(p, v[t[0] as usize], v[t[1] as usize], v[t[2] as usize]);
                            let d = dot(sub(q, p), sub(q, p));
                            if d < best.0 {
                                best = (d, q);
                            }
                        }
                    }
                }
            }
        }
        best.1
    };
    for _ in 0..iters {
        let old = r.pos.clone();
        for i in 0..n {
            if nb[i].len() < 3 {
                continue;
            }
            let c = nb[i].iter().fold([0.0; 3], |a, &j| add(a, old[j as usize]));
            let c = mul(c, 1.0 / nb[i].len() as f32);
            r.pos[i] = project(add(mul(old[i], 0.5), mul(c, 0.5)));
        }
    }
}

fn closest_on_tri(p: V3, a: V3, b: V3, c: V3) -> V3 {
    // Ericson, Real-Time Collision Detection 5.1.5
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add(a, mul(ab, d1 / (d1 - d3)));
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add(a, mul(ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return add(b, mul(sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let denom = 1.0 / (va + vb + vc);
    add(a, add(mul(ab, vb * denom), mul(ac, vc * denom)))
}

/// Écrit le résultat en OBJ (positions passées par `world`).
pub fn export_obj(r: &Result, world: &dyn Fn(V3) -> V3, path: &std::path::Path) -> std::result::Result<(), String> {
    use std::io::Write;
    let io = |e: std::io::Error| e.to_string();
    let mut w = std::io::BufWriter::new(std::fs::File::create(path).map_err(io)?);
    writeln!(w, "# retopologie Aryadne : {} faces ({} quads)", r.faces.len(), r.quads()).map_err(io)?;
    for p in &r.pos {
        let q = world(*p);
        writeln!(w, "v {} {} {}", q[0], q[1], q[2]).map_err(io)?;
    }
    for f in &r.faces {
        let idx: Vec<String> = f.iter().map(|i| (i + 1).to_string()).collect();
        writeln!(w, "f {}", idx.join(" ")).map_err(io)?;
    }
    Ok(())
}


/// Options en ligne de commande pour le banc d'essai (hors application).
#[allow(dead_code)]
pub fn bench_option(opt: &mut Options, name: &str, val: Option<&str>, i: &mut usize) {
    match name {
        "--tri" => opt.pure_quads = false,
        "--noint" => opt.consistent = false,
        "--curv" => {
            opt.curvature = val.and_then(|v| v.parse().ok()).unwrap_or(opt.curvature);
            *i += 1;
        }
        "--iters" => {
            opt.iterations = val.and_then(|v| v.parse().ok()).unwrap_or(opt.iterations);
            *i += 1;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sphere(n: usize) -> (Vec<V3>, Vec<[u32; 3]>) {
        let t = (1.0 + 5f32.sqrt()) / 2.0;
        let pos: Vec<V3> = [[-1.0, t, 0.0], [1.0, t, 0.0], [-1.0, -t, 0.0], [1.0, -t, 0.0], [0.0, -1.0, t], [0.0, 1.0, t], [0.0, -1.0, -t], [0.0, 1.0, -t], [t, 0.0, -1.0], [t, 0.0, 1.0], [-t, 0.0, -1.0], [-t, 0.0, 1.0]].iter().map(|p| norm(*p)).collect();
        let tris: Vec<[u32; 3]> = vec![[0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8], [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1]];
        let (mut pos, mut tris) = (pos, tris);
        for _ in 0..n {
            let (p, t) = refine_once(pos, tris);
            pos = p.into_iter().map(norm).collect();
            tris = t;
        }
        (pos, tris)
    }

    #[test]
    fn sphere_en_quads() {
        let (p, t) = sphere(3);
        let pq = remesh(&p, &t, &[], &Options { faces: 600, ..Default::default() }, &|_| {});
        eprintln!("{} faces, {:?}", pq.faces.len(), pq.stats);
        assert_eq!(pq.quads(), pq.faces.len());
        assert!(pq.faces.len() > 300 && pq.faces.len() < 1000);
        assert!(pq.pos.iter().all(|p| (len(*p) - 1.0).abs() < 0.08));
        assert_eq!(pq.stats.unfixed, 0);
    }

    #[test]
    fn guide_oriente_les_boucles() {
        let (p, t) = sphere(3);
        let g: Vec<V3> = (0..=48).map(|k| {
            let a = k as f32 / 48.0 * std::f32::consts::TAU;
            [a.cos(), 0.0, a.sin()]
        }).collect();
        let r = remesh(&p, &t, &[g], &Options { faces: 600, guide_radius: 1.5, ..Default::default() }, &|_| {});
        let (mut aligned, mut total) = (0, 0);
        for f in &r.faces {
            for k in 0..f.len() {
                let (a, b) = (r.pos[f[k] as usize], r.pos[f[(k + 1) % f.len()] as usize]);
                if a[1].abs() < 0.08 && b[1].abs() < 0.08 {
                    total += 1;
                    if norm(sub(b, a))[1].abs() < 0.3 {
                        aligned += 1;
                    }
                }
            }
        }
        eprintln!("{aligned}/{total} arêtes alignées sur l'équateur");
        assert!(total > 20 && aligned as f32 / total as f32 > 0.8);
    }
}
