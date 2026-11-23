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
    /// Taille des quads adaptée au flux (0 : tous de la même taille ; 1 : ils rétrécissent là où
    /// les lignes se resserrent, au lieu de se tordre)
    pub adaptive: f32,
    /// Force d'alignement du flux sur les courbures principales (0 : aucun)
    pub curvature: f32,
    /// Angle (degrés) à partir duquel une arête est vive et devient une boucle (0 : désactivé)
    pub sharp_angle: f32,
    /// Symétrie : 0 aucune, 1 plan X = 0, 2 plan Y = 0, 3 plan Z = 0
    pub symmetry: u8,
    /// Touches de densité peintes : centre x y z, rayon, facteur (2 : quads deux fois plus
    /// petits ; 0,5 : deux fois plus grands)
    pub density: Vec<[f32; 5]>,
    /// Toujours subdiviser chaque face en quads (usage interne : seconde passe à quart de cible)
    pub force_split: bool,
    /// Boucles automatiques autour des plis creux fermés (yeux, bouche, narines…)
    pub auto_loops: bool,
    /// Objets séparés : 0 = même densité partout ; 1 = densité selon les détails de chaque objet
    pub detail_density: f32,
    /// Usage interne : boules (centre, rayon) où le maillage dense est affiné
    pub zones: Vec<[f32; 4]>,
}

impl Default for Options {
    fn default() -> Self {
        Options { faces: 1500, iterations: 160, guide_radius: 1.2, pure_quads: true, adaptive: 0.5, curvature: 1.0, sharp_angle: 35.0, symmetry: 0, density: vec![], force_split: false, auto_loops: false, detail_density: 0.0, zones: vec![] }
    }
}

/// Mesures internes, pour le banc d'essai et les messages.
#[derive(Default, Clone, Debug)]
pub struct Stats {
    pub dense: usize,
    /// Singularités du champ d'orientation (pôles inévitables)
    pub field_sing: usize,
    pub unfixed: usize,
    pub flips: usize,
    /// Sommets intérieurs dont la valence n'est pas 4, dans le résultat
    pub poles: usize,
    /// L'extraction contenait d'autres faces que des quads : tout a été subdivisé (×4)
    pub split: bool,
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

/// Simplifie un maillage trop fin (sculpt de plusieurs millions de triangles) : les arêtes
/// plus courtes que `min_len` sont contractées en leur milieu, sans créer d'arête non
/// variété ni retourner de triangle. Le calcul qui suit ne dépend alors plus de la taille
/// du modèle d'origine, seulement du nombre de quads demandé.
fn decimate(pos: Vec<V3>, tris: Vec<[u32; 3]>, min_len: f32, sharp: &Features) -> (Vec<V3>, Vec<[u32; 3]>) {
    let (p, t, _) = decimate_ids(pos, tris, min_len, sharp);
    (p, t)
}

/// Simplification propre pour l'affichage et le modificateur « réduire » : contractions
/// d'arêtes qui ne retournent ni n'écrasent aucun triangle (pas d'échardes ni de trous).
/// Rend aussi, pour chaque triangle gardé, son indice dans `tris` (pour les couleurs).
pub fn simplify_to(pos: &[V3], tris: &[[u32; 3]], target: usize) -> (Vec<V3>, Vec<[u32; 3]>, Vec<u32>) {
    // Soudure des sommets confondus, en gardant l'indice d'origine de chaque triangle
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let eps = (len(sub(hi, lo)) * 1e-6).max(1e-12);
    let mut map: HashMap<[i64; 3], u32> = HashMap::new();
    let mut wp: Vec<V3> = vec![];
    let remap: Vec<u32> = pos
        .iter()
        .map(|p| {
            let k = [((p[0] - lo[0]) / eps).round() as i64, ((p[1] - lo[1]) / eps).round() as i64, ((p[2] - lo[2]) / eps).round() as i64];
            *map.entry(k).or_insert_with(|| {
                wp.push(*p);
                (wp.len() - 1) as u32
            })
        })
        .collect();
    drop(map);
    let mut ids = vec![];
    let mut wt = vec![];
    for (i, t) in tris.iter().enumerate() {
        let n = [remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize]];
        if n[0] != n[1] && n[1] != n[2] && n[0] != n[2] {
            wt.push(n);
            ids.push(i as u32);
        }
    }
    let (mut p, mut t) = (wp, wt);
    if t.len() <= target {
        return (p, t, ids);
    }
    let area: f32 = t.iter().map(|x| len(cross(sub(p[x[1] as usize], p[x[0] as usize]), sub(p[x[2] as usize], p[x[0] as usize]))) * 0.5).sum();
    // Longueur d'arête d'un maillage régulier de `target` triangles
    let mut min_len = (4.0 * area / (3f32.sqrt() * target as f32)).sqrt() * 0.6;
    for _ in 0..12 {
        let none = Features { edges: vec![], deg: vec![0; p.len()], tan: vec![] };
        let before = t.len();
        let (np, nt, kept) = decimate_ids(p, t, min_len, &none);
        ids = kept.iter().map(|&k| ids[k as usize]).collect();
        p = np;
        t = nt;
        if t.len() <= target || t.len() == before && min_len > 1e3 {
            break;
        }
        min_len *= 1.35;
    }
    (p, t, ids)
}

fn decimate_ids(mut pos: Vec<V3>, mut tris: Vec<[u32; 3]>, min_len: f32, sharp: &Features) -> (Vec<V3>, Vec<[u32; 3]>, Vec<u32>) {
    let nv = pos.len();
    let mut vf: Vec<Vec<u32>> = vec![vec![]; nv];
    for (fi, t) in tris.iter().enumerate() {
        for &v in t {
            vf[v as usize].push(fi as u32);
        }
    }
    let mut alive = vec![true; tris.len()];
    // Sommets de bord
    let mut count: HashMap<(u32, u32), u8> = HashMap::new();
    for t in &tris {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *count.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    let mut border = vec![false; nv];
    for (&(a, b), &c) in &count {
        if c != 2 {
            border[a as usize] = true;
            border[b as usize] = true;
        }
    }
    drop(count);
    let mut fset: HashSet<(u32, u32)> = sharp.edges.iter().copied().collect();
    let mut fdeg = sharp.deg.clone();
    let fnormal = |pos: &[V3], t: [u32; 3]| cross(sub(pos[t[1] as usize], pos[t[0] as usize]), sub(pos[t[2] as usize], pos[t[0] as usize]));
    for _ in 0..24 {
        let mut cand: Vec<(f32, u32, u32)> = vec![];
        for (fi, t) in tris.iter().enumerate() {
            if !alive[fi] {
                continue;
            }
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                if a < b {
                    let l = len(sub(pos[a as usize], pos[b as usize]));
                    if l < min_len {
                        cand.push((l, a, b));
                    }
                }
            }
        }
        if cand.is_empty() {
            break;
        }
        cand.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut locked = vec![false; nv];
        let mut done = 0;
        for (_, a, b) in cand {
            let (a, b) = (a as usize, b as usize);
            if locked[a] || locked[b] {
                continue;
            }
            let ring = |v: usize| -> Vec<u32> {
                let mut r: Vec<u32> = vf[v].iter().filter(|&&f| alive[f as usize]).flat_map(|&f| tris[f as usize]).filter(|&x| x as usize != v).collect();
                r.sort_unstable();
                r.dedup();
                r
            };
            let (ra, rb) = (ring(a), ring(b));
            let shared: Vec<u32> = vf[a].iter().copied().filter(|&f| alive[f as usize] && tris[f as usize].contains(&(b as u32))).collect();
            let common = ra.iter().filter(|x| rb.binary_search(x).is_ok()).count();
            if shared.is_empty() || shared.len() > 2 || common != shared.len() {
                continue;
            }
            if (border[a] || border[b]) && shared.len() != 1 {
                continue;
            }
            // Arête vive : on ne contracte que le long de la ligne, jamais un coin
            let on_line = fset.contains(&((a.min(b)) as u32, (a.max(b)) as u32));
            if (fdeg[a] > 0 || fdeg[b] > 0) && !(on_line && fdeg[a] == 2 && fdeg[b] == 2) {
                continue;
            }
            let m = mul(add(pos[a], pos[b]), 0.5);
            // Aucun triangle restant ne doit se retourner ni s'aplatir
            let mut ok = true;
            for (v, other) in [(a, b), (b, a)] {
                for &f in &vf[v] {
                    let t = tris[f as usize];
                    if !alive[f as usize] || t.contains(&(other as u32)) {
                        continue;
                    }
                    let n0 = fnormal(&pos, t);
                    let old = pos[v];
                    pos[v] = m;
                    let n1 = fnormal(&pos, t);
                    pos[v] = old;
                    if dot(n0, n1) <= 0.2 * len(n0) * len(n1) || len(n1) < 1e-12 {
                        ok = false;
                    }
                }
            }
            if !ok {
                continue;
            }
            if on_line {
                fset.remove(&((a.min(b)) as u32, (a.max(b)) as u32));
                for &x in &rb {
                    if fset.remove(&(x.min(b as u32), x.max(b as u32))) {
                        fset.insert((x.min(a as u32), x.max(a as u32)));
                    }
                }
                fdeg[b] = 0;
            }
            pos[a] = m;
            for &f in &shared {
                alive[f as usize] = false;
            }
            let moved: Vec<u32> = vf[b].iter().copied().filter(|&f| alive[f as usize]).collect();
            for f in moved {
                for x in tris[f as usize].iter_mut() {
                    if *x as usize == b {
                        *x = a as u32;
                    }
                }
                vf[a].push(f);
            }
            vf[b].clear();
            vf[a].retain(|&f| alive[f as usize]);
            border[a] = border[a] || border[b];
            locked[a] = true;
            for x in ra.into_iter().chain(rb) {
                locked[x as usize] = true;
            }
            done += 1;
        }
        if done == 0 {
            break;
        }
    }
    let mut remap = vec![u32::MAX; nv];
    let mut out_pos = vec![];
    let mut out = vec![];
    let mut kept = vec![];
    for (fi, t) in tris.iter().enumerate() {
        if !alive[fi] {
            continue;
        }
        kept.push(fi as u32);
        let mut nt = [0u32; 3];
        for k in 0..3 {
            let v = t[k] as usize;
            if remap[v] == u32::MAX {
                remap[v] = out_pos.len() as u32;
                out_pos.push(pos[v]);
            }
            nt[k] = remap[v];
        }
        out.push(nt);
    }
    tris.clear();
    (out_pos, out, kept)
}

/// Coupe les arêtes plus longues que `max_len` (découpe conforme 1 → 2, 3 ou 4 triangles).
fn refine(pos: Vec<V3>, tris: Vec<[u32; 3]>, max_len: f32, cap: usize) -> (Vec<V3>, Vec<[u32; 3]>) {
    refine_n(pos, tris, &|_| max_len, 20, cap)
}

#[cfg(test)]
fn refine_once(pos: Vec<V3>, tris: Vec<[u32; 3]>) -> (Vec<V3>, Vec<[u32; 3]>) {
    refine_n1(pos, tris)
}

#[cfg(test)]
fn refine_n1(pos: Vec<V3>, tris: Vec<[u32; 3]>) -> (Vec<V3>, Vec<[u32; 3]>) {
    refine_n(pos, tris, &|_| 0.0, 0, usize::MAX)
}

fn refine_n(mut pos: Vec<V3>, mut tris: Vec<[u32; 3]>, lim: &dyn Fn(V3) -> f32, passes: usize, cap: usize) -> (Vec<V3>, Vec<[u32; 3]>) {
    for _ in 0..(passes * 3).max(1) {
        // Une arête n'est coupée que si elle est (presque) la plus longue d'un de ses triangles :
        // sans ça, les triangles très allongés se multiplient par centaines de milliers.
        let mut marked: HashSet<(u32, u32)> = HashSet::new();
        for t in &tris {
            let l = [0, 1, 2].map(|k| len(sub(pos[t[k] as usize], pos[t[(k + 1) % 3] as usize])));
            let longest = l[0].max(l[1]).max(l[2]);
            for k in 0..3 {
                let max_len = lim(mul(add(pos[t[k] as usize], pos[t[(k + 1) % 3] as usize]), 0.5));
                if l[k] > max_len && (l[k] >= 0.75 * longest || max_len <= 0.0) {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    marked.insert((a.min(b), a.max(b)));
                }
            }
        }
        let mut mid: HashMap<(u32, u32), u32> = HashMap::new();
        let mut out = Vec::with_capacity(tris.len() * 2);
        for t in &tris {
            let mut m = [u32::MAX; 3];
            let mut n = 0;
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                if marked.contains(&(a.min(b), a.max(b))) {
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
        if done || tris.len() > cap {
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
    /// Coordonnées imposées (lignes caractéristiques) : (sommet, composante, valeur)
    pins: Vec<(u32, u8, f64)>,
}

impl Graph {
    fn new(f: &[[u32; 3]]) -> Graph {
        let mut g = Graph { ends: vec![], k: vec![], d: vec![], ef: vec![], nm: vec![], fe: Vec::with_capacity(f.len()), map: HashMap::new(), w: vec![], pins: vec![] };
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
    solve_rows(g, geo, u, paths, lam, &[], max_iter, tol)
}

/// Contrainte de boucle : écart entier actuel autour de la boucle et, pour chaque chemin de
/// singularité libre (indice dans `paths`), ce que son λ ajoute à cet écart. Le solveur pousse
/// l'écart continu vers 0 : les singularités se placent pour que la boucle se referme.
struct LoopRow {
    m0: f64,
    coef: Vec<(usize, [f64; 2])>,
}

fn solve_rows(g: &Graph, geo: &[[f64; 2]], u: &mut [[f64; 2]], paths: &[&ConePath], lam: &mut [[f64; 2]], rows: &[LoopRow], max_iter: usize, tol: f64) -> usize {
    use rayon::prelude::*;
    let loopw: f64 = std::env::var("LOOPW").ok().and_then(|x| x.parse().ok()).unwrap_or(1.0);
    const CHUNK: usize = 8192;
    let (n, ne, np) = (u.len(), g.ends.len(), paths.len());
    // Arêtes de chaque sommet (pour paralléliser Bᵀ) : (arête, le sommet est-il la première extrémité ?)
    let mut off = vec![0u32; n + 1];
    for &(i, j) in &g.ends {
        off[i as usize + 1] += 1;
        off[j as usize + 1] += 1;
    }
    for i in 0..n {
        off[i + 1] += off[i];
    }
    let mut fill = off.clone();
    let mut inc = vec![(0u32, false); off[n] as usize];
    for (e, &(i, j)) in g.ends.iter().enumerate() {
        inc[fill[i as usize] as usize] = (e as u32, true);
        fill[i as usize] += 1;
        inc[fill[j as usize] as usize] = (e as u32, false);
        fill[j as usize] += 1;
    }
    // Mise à l'échelle des colonnes (préconditionneur de Jacobi) : x = S·y
    const PINW: f64 = 10.0;
    let mut pinned = vec![0.0f64; n];
    for &(i, _, _) in &g.pins {
        pinned[i as usize] = PINW * PINW;
    }
    let sc_u: Vec<f64> = (0..n).map(|i| 1.0 / (pinned[i] + inc[off[i] as usize..off[i + 1] as usize].iter().map(|&(e, _)| g.w[e as usize] * g.w[e as usize]).sum::<f64>()).max(1.0).sqrt()).collect();
    let npin = g.pins.len();
    let fwd_pins = |yu: &[[f64; 2]], out: &mut [f64]| {
        for (k, &(i, c, _)) in g.pins.iter().enumerate() {
            out[k] = PINW * yu[i as usize][c as usize] * sc_u[i as usize];
        }
    };
    let adj_pins = |rp: &[f64], ou: &mut [[f64; 2]]| {
        for (k, &(i, c, _)) in g.pins.iter().enumerate() {
            ou[i as usize][c as usize] += PINW * rp[k] * sc_u[i as usize];
        }
    };
    let dot1 = |a: &[f64], b: &[f64]| -> f64 { a.iter().zip(b).map(|(x, y)| x * y).sum() };
    let sc_l: Vec<f64> = paths.iter().map(|p| 1.0 / p.cols.iter().map(|c| g.w[c.0 as usize] * g.w[c.0 as usize]).sum::<f64>().max(1.0).sqrt()).collect();
    // B·S·y
    let fwd = |yu: &[[f64; 2]], yl: &[[f64; 2]], out: &mut [[f64; 2]]| {
        out.par_iter_mut().enumerate().with_min_len(CHUNK).for_each(|(e, o)| {
            let (i, j) = (g.ends[e].0 as usize, g.ends[e].1 as usize);
            let t = trotf(g.k[e], yu[j]);
            let w = g.w[e];
            *o = [(t[0] * sc_u[j] - yu[i][0] * sc_u[i]) * w, (t[1] * sc_u[j] - yu[i][1] * sc_u[i]) * w];
        });
        for (s, p) in paths.iter().enumerate() {
            let l = [yl[s][0] * sc_l[s], yl[s][1] * sc_l[s]];
            for &(e, dx, dy) in &p.cols {
                let w = g.w[e as usize];
                out[e as usize][0] += (dx[0] as f64 * l[0] + dy[0] as f64 * l[1]) * w;
                out[e as usize][1] += (dx[1] as f64 * l[0] + dy[1] as f64 * l[1]) * w;
            }
        }
    };
    // S·Bᵀ·r
    let adjoint = |r: &[[f64; 2]], ou: &mut [[f64; 2]], ol: &mut [[f64; 2]]| {
        ou.par_iter_mut().enumerate().with_min_len(CHUNK).for_each(|(i, o)| {
            let mut acc = [0.0f64; 2];
            for &(e, first) in &inc[off[i] as usize..off[i + 1] as usize] {
                let e = e as usize;
                let re = [r[e][0] * g.w[e], r[e][1] * g.w[e]];
                if first {
                    acc[0] -= re[0];
                    acc[1] -= re[1];
                } else {
                    let t = trotf((4 - g.k[e]) & 3, re);
                    acc[0] += t[0];
                    acc[1] += t[1];
                }
            }
            *o = [acc[0] * sc_u[i], acc[1] * sc_u[i]];
        });
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
    let dot2 = |a: &[[f64; 2]], b: &[[f64; 2]]| -> f64 { a.par_iter().zip(b.par_iter()).with_min_len(CHUNK).map(|(x, y)| x[0] * y[0] + x[1] * y[1]).sum() };
    let axpy = |x: &mut [[f64; 2]], a: f64, p: &[[f64; 2]]| {
        x.par_iter_mut().zip(p.par_iter()).with_min_len(CHUNK).for_each(|(x, p)| {
            x[0] += a * p[0];
            x[1] += a * p[1];
        });
    };
    let mut yu: Vec<[f64; 2]> = (0..n).map(|i| [u[i][0] / sc_u[i], u[i][1] / sc_u[i]]).collect();
    let mut yl: Vec<[f64; 2]> = (0..np).map(|s| [lam[s][0] / sc_l[s], lam[s][1] / sc_l[s]]).collect();
    let mut r = vec![[0.0f64; 2]; ne];
    let mut rp = vec![0.0f64; npin];
    let mut qp = vec![0.0f64; npin];
    let nr = rows.len();
    let fwd_rows = |yl: &[[f64; 2]], out: &mut [f64]| {
        for (k, row) in rows.iter().enumerate() {
            out[k] = loopw * row.coef.iter().map(|&(s, c)| (c[0] * yl[s][0] + c[1] * yl[s][1]) * sc_l[s]).sum::<f64>();
        }
    };
    let adj_rows = |rr: &[f64], ol: &mut [[f64; 2]]| {
        for (k, row) in rows.iter().enumerate() {
            for &(s, c) in &row.coef {
                ol[s][0] += loopw * c[0] * rr[k] * sc_l[s];
                ol[s][1] += loopw * c[1] * rr[k] * sc_l[s];
            }
        }
    };
    let mut rr = vec![0.0f64; nr];
    let mut qr = vec![0.0f64; nr];
    fwd_rows(&yl, &mut rr);
    for (k, row) in rows.iter().enumerate() {
        rr[k] = -loopw * row.m0 - rr[k];
    }
    fwd_pins(&yu, &mut rp);
    for (k, &(_, _, target)) in g.pins.iter().enumerate() {
        rp[k] = PINW * target - rp[k];
    }
    fwd(&yu, &yl, &mut r);
    for e in 0..ne {
        let c = [(geo[e][0] - g.d[e][0] as f64) * g.w[e], (geo[e][1] - g.d[e][1] as f64) * g.w[e]];
        r[e] = [c[0] - r[e][0], c[1] - r[e][1]];
    }
    let (mut su, mut sl) = (vec![[0.0f64; 2]; n], vec![[0.0f64; 2]; np]);
    adjoint(&r, &mut su, &mut sl);
    adj_pins(&rp, &mut su);
    adj_rows(&rr, &mut sl);
    let (mut pu, mut pl) = (su.clone(), sl.clone());
    let mut gamma = dot2(&su, &su) + dot2(&sl, &sl);
    let stop = ne as f64 * tol;
    let mut q = vec![[0.0f64; 2]; ne];
    let mut it = 0;
    while it < max_iter && gamma > stop {
        fwd(&pu, &pl, &mut q);
        fwd_pins(&pu, &mut qp);
        fwd_rows(&pl, &mut qr);
        let alpha = gamma / (dot2(&q, &q) + dot1(&qp, &qp) + dot1(&qr, &qr)).max(1e-300);
        for k in 0..npin {
            rp[k] -= alpha * qp[k];
        }
        for k in 0..nr {
            rr[k] -= alpha * qr[k];
        }
        axpy(&mut yu, alpha, &pu);
        axpy(&mut yl, alpha, &pl);
        axpy(&mut r, -alpha, &q);
        adjoint(&r, &mut su, &mut sl);
        adj_pins(&rp, &mut su);
        adj_rows(&rr, &mut sl);
        let g2 = dot2(&su, &su) + dot2(&sl, &sl);
        let beta = g2 / gamma;
        gamma = g2;
        pu.par_iter_mut().zip(su.par_iter()).with_min_len(CHUNK).for_each(|(p, s)| *p = [s[0] + beta * p[0], s[1] + beta * p[1]]);
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
        // Zone coupée du reste par des singularités : la racine choisie est déjà prise, on repart d'ici
        let root = if visited[root as usize] { seed as u32 } else { root };
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
    param_area_ratio_real(g, f, u, geo_area, fi, None)
}

/// Variante où les entiers reçoivent un complément réel `extra` (singularités encore libres).
fn param_area_ratio_real(g: &Graph, f: &[[u32; 3]], u: &[[f64; 2]], geo_area: &[f64], fi: usize, extra: Option<&[[f64; 2]]>) -> f64 {
    let t = &f[fi];
    let es = g.fe[fi];
    let dirf = |e: u32, from: u32| -> (u8, [f64; 2]) {
        let (k, d) = (g.k[e as usize], g.d[e as usize]);
        let x = extra.map(|x| x[e as usize]).unwrap_or([0.0; 2]);
        let d = [d[0] as f64 + x[0], d[1] as f64 + x[1]];
        if g.ends[e as usize].0 == from {
            (k, d)
        } else {
            let kr = (4 - k) & 3;
            let r = trotf(kr, d);
            (kr, [-r[0], -r[1]])
        }
    };
    let (k1, d1) = dirf(es[0], t[0]);
    let (k2, d2) = dirf(es[1], t[1]);
    let (ua, ub, uc) = (u[t[0] as usize], u[t[1] as usize], u[t[2] as usize]);
    let tb = trotf(k1, ub);
    let pb = [d1[0] + tb[0] - ua[0], d1[1] + tb[1] - ua[1]];
    let tc = trotf(k2, uc);
    let inner = trotf(k1, [d2[0] + tc[0], d2[1] + tc[1]]);
    let pc = [d1[0] + inner[0] - ua[0], d1[1] + inner[1] - ua[1]];
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
                if std::env::var("WHERE").is_ok() {
                    let mut near = [0usize; 6];
                    for fi in 0..nf {
                        if !sing[fi] && param_area_ratio(g, f, u, geo_area, fi) < 0.0 {
                            // distance (en faces) à la singularité la plus proche
                            let mut seen: HashSet<u32> = HashSet::new();
                            let mut front = vec![fi as u32];
                            seen.insert(fi as u32);
                            let mut dist = 5;
                            'o: for d in 0..5 {
                                let mut next = vec![];
                                for &x in &front {
                                    if sing[x as usize] {
                                        dist = d;
                                        break 'o;
                                    }
                                    for &e in &g.fe[x as usize] {
                                        for &y in &g.ef[e as usize] {
                                            if y != u32::MAX && seen.insert(y) {
                                                next.push(y);
                                            }
                                        }
                                    }
                                }
                                front = next;
                            }
                            near[dist] += 1;
                        }
                    }
                    eprintln!("  replis à 0,1,2,3,4 faces d'une singularité, ou plus loin : {near:?}");
                    let mut cells: HashMap<[i32; 2], (usize, u32)> = HashMap::new();
                    for fi in 0..nf {
                        if !sing[fi] && param_area_ratio(g, f, u, geo_area, fi) < 0.0 {
                            let a = f[fi][0];
                            let e = cells.entry([(a / 2000) as i32, 0]).or_insert((0, a));
                            e.0 += 1;
                        }
                    }
                    let mut l: Vec<_> = cells.into_values().collect();
                    l.sort();
                    l.reverse();
                    eprintln!("  sommets témoins : {:?}", l.iter().take(12).collect::<Vec<_>>());
                }
                if let Ok(path) = std::env::var("DUMP") {
                    use std::io::Write;
                    let mut w = std::fs::File::create(path).unwrap();
                    for fi in 0..nf {
                        if !sing[fi] && param_area_ratio(g, f, u, geo_area, fi) < 0.0 {
                            writeln!(w, "{}", fi).unwrap();
                        }
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
        solve_ext(g, geo, u, &[], &mut [], 1500, 1e-9);
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
#[allow(clippy::too_many_arguments)]
fn place_cones(g: &mut Graph, f: &[[u32; 3]], geo: &[[f64; 2]], u: &mut [[f64; 2]], geo_area: &[f64], curves: &mut [Curve], debug: bool, progress: &dyn Fn(f32)) -> usize {
    let paths = cone_paths(g, f);
    let mut lam = vec![[0.0f64; 2]; paths.len()];
    let mut free = vec![true; paths.len()];
    let mut round = 0;
    loop {
        let act: Vec<usize> = (0..paths.len()).filter(|&i| free[i]).collect();
        if act.is_empty() {
            break;
        }
        progress(0.35 + 0.45 * (1.0 - act.len() as f32 / paths.len().max(1) as f32));
        let refs: Vec<&ConePath> = act.iter().map(|&i| &paths[i]).collect();
        let mut l: Vec<[f64; 2]> = act.iter().map(|&i| lam[i]).collect();
        let tol: f64 = std::env::var("TOL").ok().and_then(|x| x.parse().ok()).unwrap_or(1e-9);
        let rows = loop_rows(g, curves, &act, &paths);
        let it = solve_rows(g, geo, u, &refs, &mut l, &rows, 3000, tol);
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
        if std::env::var("NOROUND").is_ok() {
            break;
        }
        let tau = cand[0].0.max(0.2);
        let take = cand.iter().take_while(|c| c.0 <= tau).count().max((cand.len() + 2) / 3).max(1);
        if debug {
            let mut extra = vec![[0.0f64; 2]; g.ends.len()];
            for &i in &act {
                for &(e, dx, dy) in &paths[i].cols {
                    extra[e as usize][0] += dx[0] as f64 * lam[i][0] + dy[0] as f64 * lam[i][1];
                    extra[e as usize][1] += dx[1] as f64 * lam[i][0] + dy[1] as f64 * lam[i][1];
                }
            }
            let flips = (0..f.len()).filter(|&fi| g.closure(&f[fi], fi).0 == 0 && param_area_ratio_real(g, f, u, geo_area, fi, Some(&extra)) < 0.0).count();
            eprintln!("  lot {round} : {} singularités libres, {it} itérations, {take} arrondies, {flips} triangles repliés", act.len());
        }
        for &(_, i, b) in cand.iter().take(take) {
            if b != [0, 0] {
                push_path(g, f, &paths[i].steps, b);
            }
            free[i] = false;
        }
        round += 1;
    }
    // Une boucle fermée (bord, plan de symétrie, arête vive circulaire) doit revenir sur la même
    // ligne de grille. Sinon on déplace une singularité d'une case en diagonale (sa parité ne
    // change pas) jusqu'à ce que le tour se referme.
    let mut moved = 0;
    // Écart total des autres boucles : un déplacement qui referme celle-ci ne doit pas en ouvrir une autre
    let total = |g: &Graph, curves: &[Curve], skip: usize| -> i32 { curves.iter().enumerate().filter(|(k, _)| *k != skip).filter_map(|(_, c)| curve_mismatch(g, c)).map(|m| m.abs()).sum() };
    for ci in 0..curves.len() {
        for _ in 0..40 {
            let Some(m) = curve_mismatch(g, &curves[ci]) else { break };
            if m == 0 {
                break;
            }
            let others = total(g, curves, ci);
            let c = &curves[ci];
            let on_curve: HashSet<u32> = c.links.iter().chain(c.extra.iter()).map(|l| l.2).collect();
            let mut done = false;
            'search: for p in paths.iter().filter(|p| p.face != u32::MAX && p.cols.iter().any(|col| on_curve.contains(&col.0))) {
                for b in [[1, 1], [1, -1], [-1, 1], [-1, -1]] {
                    let saved: Vec<(u32, [i32; 2])> = p.cols.iter().map(|col| (col.0, g.d[col.0 as usize])).collect();
                    push_path(g, f, &p.steps, b);
                    if curve_mismatch(g, &curves[ci]).is_some_and(|m2| m2.abs() < m.abs() && total(g, curves, ci) + m2.abs() < others + m.abs()) {
                        done = true;
                        moved += 1;
                        break 'search;
                    }
                    for (e, d) in saved {
                        g.d[e as usize] = d;
                    }
                }
            }
            if !done {
                break;
            }
        }
    }
    if std::env::var("ALIGN").is_ok() {
        solve_ext(g, geo, u, &[], &mut [], 1500, 1e-9);
        let n = align_cones(g, f, geo, u, &paths, curves, false);
        if std::env::var("ALIGN_DBG").is_ok() {
            solve_ext(g, geo, u, &[], &mut [], 1500, 1e-9);
            align_cones(g, f, geo, u, &paths, curves, true);
        }
        if debug {
            eprintln!("  singularités alignées sur une voisine : {n}");
        }
    }
    for c in curves.iter_mut() {
        c.ok = curve_mismatch(g, c) == Some(0);
    }
    if debug && std::env::var("RING_DBG").is_ok() {
        for (k, c) in curves.iter().enumerate() {
            if !c.pin {
                let mut o = c.clone();
                o.c0 = 1 - o.c0;
                eprintln!("  anneau {k} : écart final {:?} (autre composante {:?})", curve_mismatch(g, c), curve_mismatch(g, &o));
            }
        }
    }
    if debug && moved > 0 {
        eprintln!("  singularités déplacées pour refermer les boucles : {moved}");
    }
    if debug {
        eprintln!("  lignes qui ne se referment pas : {}", curves.iter().filter(|c| !c.ok).count());
    }
    // Les lignes caractéristiques ensuite (une fois tous les entiers des singularités connus) :
    // chacune est amenée sur la ligne de grille la plus proche, les mieux placées d'abord.
    loop {
        let open: Vec<usize> = (0..curves.len()).filter(|&c| curves[c].pin && curves[c].fixed.is_none() && curves[c].ok).collect();
        if open.is_empty() {
            break;
        }
        g.pins = curve_pins(g, curves);
        solve_ext(g, geo, u, &[], &mut [], 3000, 1e-9);
        let mut cc: Vec<(f64, usize, i32)> = open
            .iter()
            .map(|&c| {
                let tau = curve_offset(g, &curves[c], u);
                ((tau - tau.round()).abs(), c, tau.round() as i32)
            })
            .collect();
        cc.sort_by(|a, b| a.0.total_cmp(&b.0));
        let tau = cc[0].0.max(0.2);
        let n = cc.iter().take_while(|c| c.0 <= tau).count().max(1);
        if debug {
            eprintln!("  lignes : {} libres, {n} fixées (écart max {:.2})", open.len(), cc[n - 1].0);
        }
        for &(_, c, t) in cc.iter().take(n) {
            curves[c].fixed = Some(t);
        }
    }
    g.pins = curve_pins(g, curves);
    solve_ext(g, geo, u, &[], &mut [], 3000, 1e-9);
    stiffen(g, f, geo, u, geo_area, debug);
    g.pins.clear();
    regauge(g, u);
    (0..f.len()).filter(|&fi| { let (ks, c, _) = g.closure(&f[fi], fi); odd(ks, c) }).count()
}

/// Longueur de chaque arête en pas de grille, dans la carte de son premier sommet
/// (moyenne des repères des deux extrémités).
fn edge_geometry(g: &Graph, v: &[V3], n: &[V3], q: &[V3], h: f32, phi: &[f32]) -> Vec<[f64; 2]> {
    (0..g.ends.len())
        .map(|e| {
            let (i, j) = (g.ends[e].0 as usize, g.ends[e].1 as usize);
            let d = sub(v[j], v[i]);
            let l = len(d) as f64;
            // Projection sur le plan tangent, remise à la vraie longueur de l'arête : sans ça les
            // zones très courbées (plis, creux) sont mesurées trop courtes et la grille s'y replie.
            let unit = |a: [f64; 2]| {
                let m = (a[0] * a[0] + a[1] * a[1]).sqrt();
                if m > 1e-12 { [a[0] / m * l, a[1] / m * l] } else { [0.0, 0.0] }
            };
            let gi = unit([dot(d, q[i]) as f64, dot(d, cross(n[i], q[i])) as f64]);
            let gj = trotf(g.k[e], unit([dot(d, q[j]) as f64, dot(d, cross(n[j], q[j])) as f64]));
            let hh = (h * ((phi[i] + phi[j]) * 0.5).exp()) as f64;
            [(gi[0] + gj[0]) * 0.5 / hh, (gi[1] + gj[1]) * 0.5 / hh]
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
/// Retopologie d'un modèle. Un modèle fait de plusieurs pièces (corps, chapeau, boutons,
/// lunettes…) est traité pièce par pièce : chacune reçoit sa propre grille de quads, avec une
/// part des faces proportionnelle à sa surface. Sinon les pièces qui se touchent ou se
/// traversent se mélangent (trous, aileron, pôles énormes).
pub fn remesh(pos: &[V3], tris: &[[u32; 3]], guides: &[Vec<V3>], opt: &Options, progress: &dyn Fn(f32)) -> Result {
    let (pos, mut tris) = weld(pos, tris);
    // Arêtes partagées par plus de deux faces (une paroi intérieure qui rejoint la surface, comme
    // le bord d'un chapeau collé à sa calotte) : on garde les deux faces qui continuent le mieux
    // la surface et on détache les autres. Sinon le flux et la grille s'emmêlent à la jonction.
    for _ in 0..4 {
        let mut ef: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
        for (i, t) in tris.iter().enumerate() {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                ef.entry((a.min(b), a.max(b))).or_default().push(i);
            }
        }
        let fnrm = |i: usize| {
            let t = tris[i];
            norm(cross(sub(pos[t[1] as usize], pos[t[0] as usize]), sub(pos[t[2] as usize], pos[t[0] as usize])))
        };
        let mut drop = vec![false; tris.len()];
        let mut any = false;
        for l in ef.values() {
            let live: Vec<usize> = l.iter().copied().filter(|&f| !drop[f]).collect();
            if live.len() <= 2 {
                continue;
            }
            let mut best = (f32::MIN, 0, 1);
            for x in 0..live.len() {
                for y in x + 1..live.len() {
                    let d = dot(fnrm(live[x]), fnrm(live[y]));
                    if d > best.0 {
                        best = (d, x, y);
                    }
                }
            }
            for (k, &f) in live.iter().enumerate() {
                if k != best.1 && k != best.2 {
                    drop[f] = true;
                    any = true;
                }
            }
        }
        if !any {
            break;
        }
        let mut i = 0;
        tris.retain(|_| {
            i += 1;
            !drop[i - 1]
        });
    }
    let tris = tris;
    // Pièces : composantes reliées par les arêtes
    let mut uf = Uf((0..pos.len() as u32).collect());
    for t in &tris {
        uf.union(t[0], t[1]);
        uf.union(t[1], t[2]);
    }
    let mut piece_of: HashMap<u32, usize> = HashMap::new();
    let mut pieces: Vec<Vec<[u32; 3]>> = vec![];
    for t in &tris {
        let r = uf.find(t[0]);
        let k = *piece_of.entry(r).or_insert_with(|| {
            pieces.push(vec![]);
            pieces.len() - 1
        });
        pieces[k].push(*t);
    }
    if pieces.len() <= 1 {
        return remesh_piece(&pos, &tris, guides, opt, progress);
    }
    let area = |ts: &[[u32; 3]]| -> f32 { ts.iter().map(|t| len(cross(sub(pos[t[1] as usize], pos[t[0] as usize]), sub(pos[t[2] as usize], pos[t[0] as usize]))) * 0.5).sum() };
    let areas: Vec<f32> = pieces.iter().map(|p| area(p)).collect();
    // Part de chaque pièce : selon sa surface (densité égale partout), ou aussi selon ses
    // détails (une pièce très sculptée reçoit des quads plus petits qu'une pièce lisse)
    let weights: Vec<f32> = if opt.detail_density > 0.0 {
        let detail: Vec<f32> = pieces.iter().zip(&areas).map(|(ts, &a)| piece_detail(&pos, ts) / a.sqrt().max(1e-12)).collect();
        let mean = detail.iter().zip(&areas).map(|(d, a)| d * a).sum::<f32>() / areas.iter().sum::<f32>().max(1e-12);
        detail.iter().zip(&areas).map(|(d, a)| a * (d / mean.max(1e-12)).clamp(0.25, 4.0).powf(opt.detail_density)).collect()
    } else {
        areas.clone()
    };
    let total: f32 = weights.iter().sum::<f32>().max(1e-12);
    let sym = (1..=3).contains(&opt.symmetry).then(|| opt.symmetry as usize - 1);
    let done = std::sync::atomic::AtomicUsize::new(0);
    let n = pieces.len();
    // Une pièce après l'autre (chacune est déjà calculée en parallèle à l'intérieur)
    let results: Vec<Option<Result>> = {
        pieces
            .iter()
            .zip(areas.iter().zip(&weights))
            .map(|(ts, (a, wgt))| {
                // Sommets de la pièce, renumérotés
                let mut remap: HashMap<u32, u32> = HashMap::new();
                let mut p = vec![];
                let t2: Vec<[u32; 3]> = ts
                    .iter()
                    .map(|t| {
                        t.map(|v| {
                            *remap.entry(v).or_insert_with(|| {
                                p.push(pos[v as usize]);
                                (p.len() - 1) as u32
                            })
                        })
                    })
                    .collect();
                let mut o = opt.clone();
                // Part des faces : selon la surface, mais une pièce fine (anneau, lunettes, doigt)
                // en reçoit assez pour avoir une douzaine de quads autour de sa section. Son rayon
                // typique : 3 × volume / surface (r pour une sphère, 1,5 r pour un tore).
                let vol: f32 = t2.iter().map(|t| dot(p[t[0] as usize], cross(p[t[1] as usize], p[t[2] as usize])) / 6.0).sum::<f32>().abs();
                let rho = 3.0 * vol / a.max(1e-12);
                let share = opt.faces as f32 * wgt / total;
                // (seulement pour une pièce fermée : une paroi ouverte n'a pas de volume)
                let closed = {
                    let mut c: HashMap<(u32, u32), u8> = HashMap::new();
                    for t in &t2 {
                        for k in 0..3 {
                            let (x, y) = (t[k], t[(k + 1) % 3]);
                            let e = c.entry((x.min(y), x.max(y))).or_default();
                            *e = e.saturating_add(1);
                        }
                    }
                    c.values().filter(|&&n| n == 1).count() * 50 < c.len()
                };
                let need = if closed && rho > 1e-9 { a * 4.0 / (rho * rho) } else { 0.0 };
                o.faces = (share.max(need.min(opt.faces as f32 * 0.4)).round() as usize).max(24);
                // La symétrie ne vaut que pour une pièce qui traverse le plan
                if let Some(ax) = sym {
                    let (lo, hi) = p.iter().fold((f32::MAX, f32::MIN), |(l, h), q| (l.min(q[ax]), h.max(q[ax])));
                    let span = hi - lo;
                    if !(lo < -span * 0.05 && hi > span * 0.05) {
                        o.symmetry = 0;
                    }
                }
                let k0 = done.load(std::sync::atomic::Ordering::Relaxed) as f32;
                // Un guide ne vaut que pour la pièce sur laquelle il est dessiné (pas pour un œil
                // séparé qui passe juste à côté du contour de la paupière)
                let mine = guides_on(&p, guides, (a / o.faces.max(1) as f32).sqrt() * 0.6);
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| remesh_piece(&p, &t2, &mine, &o, &|f| progress((k0 + f) / n as f32)))).ok();
                done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if std::env::var("RETOPO_DEBUG_PIECES").is_ok() {
                    eprintln!("pièce : {} triangles, aire {:.1} %, fermée {closed}, rho {rho:.4}, visé {} → {} faces", t2.len(), a / total * 100.0, o.faces, r.as_ref().map(|r| r.faces.len()).unwrap_or(0));
                }
                r
            })
            .collect()
    };
    let mut out = Result { pos: vec![], faces: vec![], stats: Stats::default() };
    for r in results.into_iter().flatten() {
        let base = out.pos.len() as u32;
        out.pos.extend(r.pos);
        out.faces.extend(r.faces.into_iter().map(|f| f.into_iter().map(|v| v + base).collect()));
        out.stats.dense += r.stats.dense;
        out.stats.field_sing += r.stats.field_sing;
        out.stats.unfixed += r.stats.unfixed;
        out.stats.flips += r.stats.flips;
        out.stats.poles += r.stats.poles;
    }
    progress(1.0);
    out
}

/// Une pièce. Ses parties minces (bord de chapeau, parois, vêtement, feuille) sont d'abord
/// gonflées à l'épaisseur de deux quads : une grille ordinaire ne confond plus leurs deux côtés.
/// La retopologie se fait sur la version gonflée, puis chaque sommet revient sur la vraie surface
/// au même endroit relatif. Comme à la main, le rebord d'une paroi fine garde ainsi une ou deux
/// boucles, et le dessus et le dessous ont la même grille.
/// Quantité de détail d'une pièce : somme des angles dièdres × longueur des arêtes (courbure
/// totale), sans dimension une fois divisée par la racine de l'aire
fn piece_detail(pos: &[V3], ts: &[[u32; 3]]) -> f32 {
    let mut first: HashMap<(u32, u32), V3> = HashMap::new();
    let mut sum = 0.0;
    for t in ts {
        let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        let nn = norm(cross(sub(b, a), sub(c, a)));
        for k in 0..3 {
            let (x, y) = (t[k], t[(k + 1) % 3]);
            let key = (x.min(y), x.max(y));
            if let Some(m) = first.remove(&key) {
                sum += dot(m, nn).clamp(-1.0, 1.0).acos() * len(sub(pos[y as usize], pos[x as usize]));
            } else {
                first.insert(key, nn);
            }
        }
    }
    sum
}

/// Aligne les pôles voisins : quand deux singularités proches sont décalées d'une seule ligne
/// de grille, la ligne qui part de l'une passe juste à côté de l'autre et continue sa route
/// (les boucles qui « se promènent »). On déplace l'une d'un cran pour qu'elles partagent une
/// ligne : la ligne va alors d'un pôle à l'autre, comme dans une topologie faite à la main.
fn align_cones(g: &mut Graph, f: &[[u32; 3]], geo: &[[f64; 2]], u: &[[f64; 2]], paths: &[ConePath], curves: &[Curve], dry: bool) -> usize {
    let reach: f64 = std::env::var("ALIGN_R").ok().and_then(|x| x.parse().ok()).unwrap_or(8.0);
    let mut adj: Vec<Vec<(u32, u32)>> = vec![vec![]; g.k.len().max(1)];
    let nv = g.ends.iter().map(|e| e.0.max(e.1) as usize + 1).max().unwrap_or(0);
    adj.resize(nv, vec![]);
    for (e, &(i, j)) in g.ends.iter().enumerate() {
        adj[i as usize].push((j, e as u32));
        adj[j as usize].push((i, e as u32));
    }
    let rf = |k: u8, v: [f64; 2]| -> [f64; 2] {
        match k & 3 {
            0 => v,
            1 => [v[1], -v[0]],
            2 => [-v[0], -v[1]],
            _ => [-v[1], v[0]],
        }
    };
    // Position entière d'une singularité : le point de grille le plus proche du premier coin de
    // sa face (la face est plus petite qu'une case), dans le repère de ce coin. Les déplacements
    // décidés ici s'ajoutent ensuite (la paramétrisation n'est pas recalculée entre deux).
    let shift: std::cell::RefCell<HashMap<usize, [i32; 2]>> = std::cell::RefCell::new(HashMap::new());
    let pos_of = |g: &Graph, fi: usize| -> Option<[i32; 2]> {
        let (ks, _, _) = g.closure(&f[fi], fi);
        if ks == 0 || ks == 2 {
            return None;
        }
        let a = u[f[fi][0] as usize];
        let s = shift.borrow().get(&fi).copied().unwrap_or([0, 0]);
        Some([a[0].round() as i32 + s[0], a[1].round() as i32 + s[1]])
    };
    let cones: Vec<usize> = (0..paths.len()).filter(|&i| paths[i].face != u32::MAX && pos_of(g, paths[i].face as usize).is_some()).collect();
    let face_of: HashMap<u32, usize> = cones.iter().map(|&i| (f[paths[i].face as usize][0], i)).collect();
    let mismatch_total = |g: &Graph| -> i32 { curves.iter().filter_map(|c| curve_mismatch(g, c)).map(|m| m.abs()).sum() };
    // Voisines d'une singularité : (indice, Δ = position de l'autre moins la sienne, dans son repère)
    let neighbours = |g: &Graph, i: usize| -> Vec<(usize, [i32; 2])> {
        let fi = paths[i].face as usize;
        let Some(pa) = pos_of(g, fi) else { return vec![] };
        let a0 = f[fi][0];
        let mut seen: HashMap<u32, (u8, [i32; 2], f64)> = HashMap::new();
        seen.insert(a0, (0, [0, 0], 0.0));
        let mut queue = std::collections::VecDeque::from([a0]);
        let mut out = vec![];
        while let Some(x) = queue.pop_front() {
            let (kx, dx, dist) = seen[&x];
            if let Some(&j) = face_of.get(&x) {
                if j != i {
                    if let Some(pb) = pos_of(g, paths[j].face as usize) {
                        let r = trot(kx, pb);
                        out.push((j, [r[0] + dx[0] - pa[0], r[1] + dx[1] - pa[1]]));
                    }
                }
            }
            for &(y, e) in &adj[x as usize] {
                if seen.contains_key(&y) {
                    continue;
                }
                let l = (geo[e as usize][0].powi(2) + geo[e as usize][1].powi(2)).sqrt();
                if dist + l > reach {
                    continue;
                }
                let (k, d) = g.dir(e, x);
                let rd = trot(kx, d);
                seen.insert(y, ((kx + k) & 3, [dx[0] + rd[0], dx[1] + rd[1]], dist + l));
                queue.push_back(y);
            }
        }
        out
    };
    // Score : paires alignées (une coordonnée égale) moins paires décalées d'une ligne
    let score = |ns: &[(usize, [i32; 2])]| -> i32 {
        ns.iter()
            .map(|(_, d)| {
                if d[0] == 0 && d[1] == 0 {
                    -10
                } else if d[0] == 0 || d[1] == 0 {
                    1
                } else if d[0].abs() == 1 || d[1].abs() == 1 {
                    -1
                } else {
                    0
                }
            })
            .sum()
    };
    if std::env::var("ALIGN_DBG").is_ok() {
        let mut h = [0usize; 5];
        for &i in &cones {
            for (_, d) in neighbours(g, i) {
                let m = d[0].abs().min(d[1].abs()).min(4) as usize;
                h[m] += 1;
            }
        }
        eprintln!("  écart min entre singularités voisines (0,1,2,3,4+) : {h:?}");
    }
    if dry {
        return 0;
    }
    let mut moved = 0;
    let mut done: HashSet<usize> = HashSet::new();
    for &i in &cones {
        if done.contains(&i) {
            continue;
        }
        let ns = neighbours(g, i);
        let s0 = score(&ns);
        let m0 = mismatch_total(g);
        let fi = paths[i].face as usize;
        let mut best: Option<([i32; 2], i32)> = None;
        for &(_, d) in &ns {
            for dp in [[d[0], 0], [0, d[1]]] {
                if dp[0].abs() + dp[1].abs() != 1 {
                    continue;
                }
                // Déplacer p de dp : la fermeture change de (I − R)·dp, on pousse l'opposé
                let (ks, _, _) = g.closure(&f[fi], fi);
                let r = trot(ks, dp);
                let sg: i32 = if std::env::var("ALIGN_SIGN").is_ok() { 1 } else { -1 };
                let delta = [sg * (dp[0] - r[0]), sg * (dp[1] - r[1])];
                push_path(g, f, &paths[i].steps, delta);
                let old = shift.borrow().get(&fi).copied().unwrap_or([0, 0]);
                shift.borrow_mut().insert(fi, [old[0] + dp[0], old[1] + dp[1]]);
                let s1 = score(&neighbours(g, i));
                let ok = s1 > s0 && mismatch_total(g) <= m0;
                shift.borrow_mut().insert(fi, old);
                push_path(g, f, &paths[i].steps, [-delta[0], -delta[1]]);
                if ok && best.is_none_or(|b| s1 > b.1) {
                    best = Some((dp, s1));
                }
            }
        }
        if let Some((dp, _)) = best {
            let (ks, _, _) = g.closure(&f[fi], fi);
            let r = trot(ks, dp);
            let sg: i32 = if std::env::var("ALIGN_SIGN").is_ok() { 1 } else { -1 };
            push_path(g, f, &paths[i].steps, [sg * (dp[0] - r[0]), sg * (dp[1] - r[1])]);
            let old = shift.borrow().get(&fi).copied().unwrap_or([0, 0]);
            shift.borrow_mut().insert(fi, [old[0] + dp[0], old[1] + dp[1]]);
            moved += 1;
            done.insert(i);
            for (j, _) in neighbours(g, i) {
                done.insert(j);
            }
        }
    }
    moved
}

/// Coefficients de l'écart de chaque boucle : il est linéaire en les décalages entiers d des
/// arêtes de la boucle, et chaque chemin de singularité modifie ces d selon ses colonnes.
fn loop_rows(g: &mut Graph, curves: &[Curve], act: &[usize], paths: &[ConePath]) -> Vec<LoopRow> {
    if std::env::var("NOLOOPROWS").is_ok() {
        return vec![];
    }
    let mut rows = vec![];
    for c in curves.iter().filter(|c| !c.pin) {
        let Some(m0) = curve_mismatch(g, c) else { continue };
        let edges: Vec<u32> = c.links.iter().chain(c.extra.iter()).map(|l| l.2).collect();
        // Coefficient de chaque composante de d sur chaque arête (différences finies exactes)
        let mut ce: HashMap<u32, [f64; 2]> = HashMap::new();
        for &e in &edges {
            let saved = g.d[e as usize];
            let mut k = [0.0; 2];
            for j in 0..2 {
                g.d[e as usize][j] += 1;
                k[j] = (curve_mismatch(g, c).unwrap_or(m0) - m0) as f64;
                g.d[e as usize] = saved;
            }
            ce.insert(e, k);
        }
        let mut coef = vec![];
        for (s, &pi) in act.iter().enumerate() {
            let mut a = [0.0f64; 2];
            for &(e, dx, dy) in &paths[pi].cols {
                if let Some(k) = ce.get(&e) {
                    a[0] += k[0] * dx[0] as f64 + k[1] * dx[1] as f64;
                    a[1] += k[0] * dy[0] as f64 + k[1] * dy[1] as f64;
                }
            }
            if a != [0.0, 0.0] {
                coef.push((s, a));
            }
        }
        rows.push(LoopRow { m0: m0 as f64, coef });
    }
    rows
}

/// Morceaux des guides qui passent sur la surface `p` (à moins de `tol` d'un de ses sommets)
fn guides_on(p: &[V3], guides: &[Vec<V3>], tol: f32) -> Vec<Vec<V3>> {
    if guides.is_empty() || tol <= 0.0 {
        return vec![];
    }
    let key = |x: V3| ((x[0] / tol).floor() as i32, (x[1] / tol).floor() as i32, (x[2] / tol).floor() as i32);
    let mut grid: HashMap<(i32, i32, i32), Vec<u32>> = HashMap::new();
    for (i, &x) in p.iter().enumerate() {
        grid.entry(key(x)).or_default().push(i as u32);
    }
    let near = |x: V3| {
        let (a, b, c) = key(x);
        for da in -1..=1 {
            for db in -1..=1 {
                for dc in -1..=1 {
                    if let Some(l) = grid.get(&(a + da, b + db, c + dc)) {
                        if l.iter().any(|&i| len(sub(p[i as usize], x)) < tol) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    };
    let mut out = vec![];
    for g in guides {
        let mut run: Vec<V3> = vec![];
        for &x in g {
            if near(x) {
                run.push(x);
            } else if !run.is_empty() {
                if run.len() > 1 {
                    out.push(std::mem::take(&mut run));
                }
                run.clear();
            }
        }
        if run.len() > 1 {
            out.push(run);
        }
    }
    out
}

fn remesh_piece(pos: &[V3], tris: &[[u32; 3]], guides: &[Vec<V3>], opt: &Options, progress: &dyn Fn(f32)) -> Result {
    let debug = std::env::var("RETOPO_DEBUG").is_ok();
    let n = pos.len();
    if std::env::var("RETOPO_NO_SHELL").is_ok() || tris.len() < 32 {
        return remesh_one(pos, tris, guides, opt, progress);
    }
    let area: f32 = tris.iter().map(|t| len(cross(sub(pos[t[1] as usize], pos[t[0] as usize]), sub(pos[t[2] as usize], pos[t[0] as usize]))) * 0.5).sum();
    let h = (area / opt.faces.max(1) as f32).sqrt();
    let want = std::env::var("RETOPO_WANT").ok().and_then(|x| x.parse::<f32>().ok()).unwrap_or(2.0) * h;
    let fnrm: Vec<V3> = tris.iter().map(|t| norm(cross(sub(pos[t[1] as usize], pos[t[0] as usize]), sub(pos[t[2] as usize], pos[t[0] as usize])))).collect();
    let mut vn = vec![[0.0f32; 3]; n];
    for (i, t) in tris.iter().enumerate() {
        let w = cross(sub(pos[t[1] as usize], pos[t[0] as usize]), sub(pos[t[2] as usize], pos[t[0] as usize]));
        for &v in t {
            vn[v as usize] = add(vn[v as usize], w);
        }
        let _ = i;
    }
    let vn: Vec<V3> = vn.into_iter().map(norm).collect();
    let bvh = crate::bvh::Bvh::new(pos, tris);
    // Épaisseur sous chaque sommet : la paroi d'en face (normale opposée)
    let thick: Vec<f32> = {
        use rayon::prelude::*;
        (0..n)
            .into_par_iter()
            .map(|i| {
                let nn = vn[i];
                let mut t0 = h * 1e-3;
                for _ in 0..4 {
                    match bvh.closest(pos[i], mul(nn, -1.0), t0, want) {
                        Some(hit) if dot(fnrm[hit.tri as usize], nn) < -0.6 => return hit.t,
                        Some(hit) => t0 = hit.t + h * 1e-3,
                        None => break,
                    }
                }
                f32::MAX
            })
            .collect()
    };
    let mut push: Vec<f32> = thick.iter().map(|&t| if t < want { (want - t) * 0.5 } else { 0.0 }).collect();
    let n_thin = push.iter().filter(|&&d| d > 0.0).count();
    if debug {
        eprintln!("parois minces : {n_thin} sommets sur {n}");
    }
    if n_thin < n / 50 {
        return remesh_one(pos, tris, guides, opt, progress);
    }
    // Le rebord suit : chaque sommet prend au moins une part du gonflement de ses voisins
    let mut nb: Vec<Vec<u32>> = vec![vec![]; n];
    for t in tris {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            nb[a as usize].push(b);
            nb[b as usize].push(a);
        }
    }
    for _ in 0..6 {
        let prev = push.clone();
        for i in 0..n {
            let m = nb[i].iter().map(|&j| prev[j as usize]).fold(0.0f32, f32::max);
            let avg = (prev[i] + nb[i].iter().map(|&j| prev[j as usize]).sum::<f32>()) / (1 + nb[i].len()) as f32;
            push[i] = prev[i].max(avg).max(m * 0.5);
        }
    }
    let inflated: Vec<V3> = (0..n).map(|i| add(pos[i], mul(vn[i], push[i]))).collect();
    let r = remesh_one(&inflated, tris, guides, opt, progress);
    // Retour sur la vraie surface : le point le plus proche sur la version gonflée, même place
    // relative (coordonnées barycentriques) sur l'originale
    let cell = h.max(1e-6);
    let key3 = |p: V3| [(p[0] / cell).floor() as i64, (p[1] / cell).floor() as i64, (p[2] / cell).floor() as i64];
    let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        let c = mul(add(add(inflated[t[0] as usize], inflated[t[1] as usize]), inflated[t[2] as usize]), 1.0 / 3.0);
        grid.entry(key3(c)).or_default().push(i as u32);
    }
    let back = |x: V3| -> V3 {
        let k = key3(x);
        let mut best: Option<(f32, usize, [f32; 3])> = None;
        for ring in 1..=3i64 {
            for dx in -ring..=ring {
                for dy in -ring..=ring {
                    for dz in -ring..=ring {
                        if let Some(l) = grid.get(&[k[0] + dx, k[1] + dy, k[2] + dz]) {
                            for &ti in l {
                                let t = tris[ti as usize];
                                let (bc, d) = closest_bary(x, inflated[t[0] as usize], inflated[t[1] as usize], inflated[t[2] as usize]);
                                if best.is_none_or(|b| d < b.0) {
                                    best = Some((d, ti as usize, bc));
                                }
                            }
                        }
                    }
                }
            }
            if best.is_some() {
                break;
            }
        }
        match best {
            Some((_, ti, bc)) => {
                let t = tris[ti];
                add(add(mul(pos[t[0] as usize], bc[0]), mul(pos[t[1] as usize], bc[1])), mul(pos[t[2] as usize], bc[2]))
            }
            None => x,
        }
    };
    let out: Vec<V3> = {
        use rayon::prelude::*;
        r.pos.par_iter().map(|&x| back(x)).collect()
    };
    Result { pos: out, faces: r.faces, stats: r.stats }
}

/// Point du triangle (a, b, c) le plus proche de p : coordonnées barycentriques et distance.
fn closest_bary(p: V3, a: V3, b: V3, c: V3) -> ([f32; 3], f32) {
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    let bary = if d1 <= 0.0 && d2 <= 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        let bp = sub(p, b);
        let (d3, d4) = (dot(ab, bp), dot(ac, bp));
        if d3 >= 0.0 && d4 <= d3 {
            [0.0, 1.0, 0.0]
        } else {
            let cp = sub(p, c);
            let (d5, d6) = (dot(ab, cp), dot(ac, cp));
            if d6 >= 0.0 && d5 <= d6 {
                [0.0, 0.0, 1.0]
            } else {
                let vc = d1 * d4 - d3 * d2;
                let vb = d5 * d2 - d1 * d6;
                let va = d3 * d6 - d5 * d4;
                if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
                    let v = d1 / (d1 - d3);
                    [1.0 - v, v, 0.0]
                } else if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
                    let w = d2 / (d2 - d6);
                    [1.0 - w, 0.0, w]
                } else if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
                    let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
                    [0.0, 1.0 - w, w]
                } else {
                    let den = 1.0 / (va + vb + vc);
                    let (v, w) = (vb * den, vc * den);
                    [1.0 - v - w, v, w]
                }
            }
        }
    };
    let q = add(add(mul(a, bary[0]), mul(b, bary[1])), mul(c, bary[2]));
    (bary, len(sub(p, q)))
}

/// Retopologie d'une surface, au nombre de faces visé : si l'extraction a dû tout subdiviser
/// pour n'avoir que des quads (×4), on recommence au quart de la cible.
fn remesh_one(pos: &[V3], tris: &[[u32; 3]], guides: &[Vec<V3>], opt: &Options, progress: &dyn Fn(f32)) -> Result {
    let r = remesh_core(pos, tris, guides, opt, &|f| progress(f * 0.5));
    if !(opt.pure_quads && r.stats.split && !opt.force_split && opt.faces >= 64) {
        progress(1.0);
        return r;
    }
    let mut o = opt.clone();
    o.faces = (opt.faces / 4).max(16);
    o.force_split = true;
    let r2 = remesh_core(pos, tris, guides, &o, &|f| progress(0.5 + f * 0.5));
    let t = opt.faces as f32;
    let err = |n: usize| (n as f32 / t).ln().abs();
    if err(r2.faces.len()) < err(r.faces.len()) { r2 } else { r }
}

fn remesh_core(pos: &[V3], tris: &[[u32; 3]], guides: &[Vec<V3>], opt: &Options, progress: &dyn Fn(f32)) -> Result {
    let (pos0, tris0) = (pos, tris);
    let mut stats = Stats::default();
    let t0 = std::time::Instant::now();
    let debug = std::env::var("RETOPO_DEBUG").is_ok();
    let lap = |label: &str| {
        if debug {
            eprintln!("[{:6.2}s] {label}", t0.elapsed().as_secs_f32());
        }
    };
    let (pos, tris) = weld(pos, tris);
    let sym = (1..=3).contains(&opt.symmetry).then(|| opt.symmetry as usize - 1);
    // Surface complète, gardée pour la projection finale
    let (orig_v, orig_f) = (pos.clone(), tris.clone());
    let (pos, tris) = match sym {
        Some(ax) => clip_half(&pos, &tris, ax),
        None => (pos, tris),
    };
    let area: f32 = tris.iter().map(|t| len(cross(sub(pos[t[1] as usize], pos[t[0] as usize]), sub(pos[t[2] as usize], pos[t[0] as usize]))) * 0.5).sum();
    let target = (if sym.is_some() { opt.faces / 2 } else { opt.faces }).max(4);
    let h = (area / target as f32).sqrt().max(1e-6);
    let dens: f32 = std::env::var("DENS").ok().and_then(|x| x.parse().ok()).unwrap_or(2.5);
    let sharp0 = features(&pos, &tris, opt.sharp_angle, 0.0);
    let (pos, tris) = decimate(pos, tris, h / (dens * 2.15), &sharp0);
    lap("simplification");
    // Garde-fou : un maillage fait de triangles très allongés ne doit pas exploser en taille
    let (v, f) = refine(pos, tris, h / dens, (target * 60).max(40_000));
    // Zones où plusieurs singularités tombent à moins d'un quad l'une de l'autre (pointe de
    // corne, bout de doigt) : le maillage dense y est trop grossier pour les écarter d'un quad
    // chacune, l'extraction les entasse en une étoile. On y double la finesse.
    let (v, f) = if opt.zones.is_empty() {
        (v, f)
    } else {
        let zones = opt.zones.clone();
        let fine = h / (dens * 2.5);
        refine_n(v, f, &|m: V3| if zones.iter().any(|z| len(sub(m, [z[0], z[1], z[2]])) < z[3]) { fine } else { f32::MAX }, 20, (target * 120).max(80_000))
    };
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
    // Arêtes vives et bords : le champ les longe
    let ft = features(&v, &f, opt.sharp_angle, 3.0 * h);
    for i in 0..n_v {
        if ft.deg[i] == 2 {
            let t = tangent(ft.tan[i], nrm[i]);
            if dot(t, t) > 1e-8 {
                q[i] = norm(t);
                fixed[i] = true;
            }
        }
    }
    let (cdir, cani, kmin) = curvature(&v, &f, &nrm, &adj, h);

    // Boucles automatiques : les plis creux qui font le tour de quelque chose (orbite, bouche,
    // narine, base d'une corne) deviennent des guides, comme si on les avait dessinés
    let auto = if opt.auto_loops && std::env::var("NOAUTO").is_err() { auto_guides(&v, &nrm, &adj, &cdir, &cani, h) } else { vec![] };
    if debug && !auto.is_empty() {
        eprintln!("boucles automatiques : {}", auto.len());
    }
    let n_manual = guides.len();
    let guides: Vec<Vec<V3>> = guides.iter().cloned().chain(auto).collect();
    let (cdir, cani) = if opt.curvature > 0.0 { (cdir, cani) } else { (vec![[0.0; 3]; n_v], vec![0.0; n_v]) };
    let guides = &guides;
    let r = h * opt.guide_radius;
    // Au-delà du rayon fixe, un guide attire encore le flux en s'estompant : plusieurs boucles
    // parallèles au guide (anneaux autour d'un œil, d'une bouche) au lieu d'une seule ligne
    let reach: f32 = std::env::var("GREACH").ok().and_then(|x| x.parse().ok()).unwrap_or(10.0);
    let big = r.max(h * reach);
    let mut gsoft: Vec<(f32, V3, f32)> = vec![(f32::MAX, [0.0; 3], 1.0); n_v];
    // Une boucle trouvée automatiquement guide plus doucement qu'un trait dessiné, et ne fixe
    // rien : elle oriente le flux sans le forcer (sinon replis et pôles en trop)
    let auto_k: f32 = std::env::var("AUTO_K").ok().and_then(|x| x.parse().ok()).unwrap_or(0.25);
    for (gi, g) in guides.iter().enumerate() {
        let is_auto = gi >= n_manual;
        for s in g.windows(2) {
            let (lo, hi) = ([s[0][0].min(s[1][0]) - big, s[0][1].min(s[1][1]) - big, s[0][2].min(s[1][2]) - big], [s[0][0].max(s[1][0]) + big, s[0][1].max(s[1][1]) + big, s[0][2].max(s[1][2]) + big]);
            for i in 0..n_v {
                let p = v[i];
                if p[0] < lo[0] || p[1] < lo[1] || p[2] < lo[2] || p[0] > hi[0] || p[1] > hi[1] || p[2] > hi[2] {
                    continue;
                }
                let (d, dir) = seg_dist(p, s[0], s[1]);
                if dot(nrm[i], norm(dir)).abs() >= 0.95 {
                    continue;
                }
                if d < r && !is_auto {
                    q[i] = norm(tangent(dir, nrm[i]));
                    fixed[i] = true;
                } else if d < big && d < gsoft[i].0 {
                    gsoft[i] = (d.max(r), norm(tangent(dir, nrm[i])), if is_auto { auto_k } else { 1.0 });
                }
            }
        }
    }
    let gsoft: Vec<Option<(f32, V3)>> = gsoft.into_iter().map(|(d, t, k)| if d < big && dot(t, t) > 0.5 { Some(((1.0 - ((d - r) / (big - r).max(1e-9)).clamp(0.0, 1.0)) * k.sqrt(), t)) } else { None }).collect();
    for i in 0..n_v {
        if let (false, Some((_, t))) = (fixed[i], gsoft[i]) {
            q[i] = t;
        }
    }

    // Hiérarchie de maillages grossiers (convergence rapide, comme Instant Meshes)
    if let Ok(path) = std::env::var("DUMPMESH") {
        use std::io::Write;
        let mut w = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        for p in &v {
            writeln!(w, "v {} {} {}", p[0], p[1], p[2]).unwrap();
        }
        for t in &f {
            writeln!(w, "f {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1).unwrap();
        }
    }
    lap("préparation");

    // Une courbure même douce mais régulière (corps conique, bras, bec) guide les boucles : anneaux
    // autour des formes plutôt qu'une grille droite qui coupe en diagonale
    let s0: f32 = std::env::var("S0").ok().and_then(|x| x.parse().ok()).unwrap_or(0.15);
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
        progress(0.05 + 0.2 * step / total_steps);
    }
    let mut q = cur;
    lap("orientation (lissage local)");
    if opt.curvature > 0.0 {
        // Optimum global : lisse à l'échelle de `ell` quads, collé aux courbures marquées et aux guides.
        // À 8 quads, les plis fins de la peau (rides, petits replis) ne créent plus de paires de
        // singularités : deux fois moins de pôles, et presque plus de pôles 3-5 collés (qui font
        // partir les boucles en spirale)
        let ell: f32 = std::env::var("ELL").ok().and_then(|x| x.parse().ok()).unwrap_or(8.0);
        let eps_k: f32 = std::env::var("EPSK").ok().and_then(|x| x.parse().ok()).unwrap_or(0.01);
        let lam = 0.87 * (mean_edge / (h * ell)).powi(2) * opt.curvature;
        let mut w: Vec<f32> = levels[0].a.iter().map(|a| a * lam).collect();
        let mut tg = levels[0].c.clone();
        // Les guides gardent la force qu'ils avaient à l'échelle de 2 quads
        let gk: f32 = std::env::var("GSOFT").ok().and_then(|x| x.parse().ok()).unwrap_or(200.0) * (ell / 2.0).powi(2);
        for i in 0..n_v {
            if fixed[i] {
                w[i] = lam * 50.0;
                tg[i] = q[i];
            } else if let Some((f, t)) = gsoft[i] {
                let ws = lam * gk * f * f;
                if ws > w[i] {
                    w[i] = ws;
                    tg[i] = t;
                }
            }
        }
        q = linear_field(&v, &nrm, &adj, &q, &tg, &w, lam * eps_k);
    }
    {
        let dip: f32 = std::env::var("DIP").ok().and_then(|x| x.parse().ok()).unwrap_or(1.5);
        let before = singular_faces(&f, &nrm, &q).len();
        let removed = if dip > 0.0 { cancel_dipoles(&v, &nrm, &adj, &f, &mut q, h * dip, &fixed) } else { 0 };
        if debug {
            eprintln!("singularités du champ : {before}, dont {removed} annulées par paires");
            for (fi, k) in singular_faces(&f, &nrm, &q) {
                let c = v[f[fi as usize][0] as usize];
                eprintln!("  sing {k} en ({:.2}, {:.2}, {:.2})", c[0], c[1], c[2]);
            }
        }
    }

    if let Ok(path) = std::env::var("DUMPFIELD") {
        use std::io::Write;
        let mut w = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        for i in 0..n_v {
            let qq = if std::env::var("DUMPC").is_ok() { levels[0].c[i] } else { q[i] };
            writeln!(w, "{} {} {} {} {} {} {} {} {} {}", v[i][0], v[i][1], v[i][2], qq[0], qq[1], qq[2], nrm[i][0], nrm[i][1], nrm[i][2], if std::env::var("DUMPK").is_ok() { kmin[i] * h } else { levels[0].a[i] }).unwrap();
        }
    }
    progress(0.3);
    lap("orientation (optimum global)");
    // Transitions entre repères voisins : seule donnée entière de départ. Tous les décalages
    // de grille partent de zéro ; ce sont les singularités et les anses qui les fixeront.
    let mut g = Graph::new(&f);
    for e in 0..g.ends.len() {
        g.k[e] = matching(&nrm, &q, g.ends[e].0 as usize, g.ends[e].1 as usize);
    }
    // Les trois arêtes d'un triangle singulier comptent beaucoup plus : sinon les moindres
    // carrés « déchirent » ce triangle pour loger la singularité ailleurs dans la grille.
    let stiff: f64 = std::env::var("STIFF").ok().and_then(|x| x.parse().ok()).unwrap_or(30.0);
    for fi in 0..f.len() {
        if g.closure(&f[fi], fi).0 != 0 {
            stats.field_sing += 1;
            for &e in &g.fe[fi] {
                g.w[e as usize] = stiff.sqrt();
            }
        }
    }
    // Singularités serrées (moins d'un quad) : on recommence une fois avec un maillage dense plus
    // fin autour d'elles
    if opt.zones.is_empty() && std::env::var("NOZONES").is_err() {
        let mut sing: Vec<V3> = vec![];
        for fi in 0..f.len() {
            if g.closure(&f[fi], fi).0 != 0 {
                let t = f[fi];
                sing.push(mul(add(add(v[t[0] as usize], v[t[1] as usize]), v[t[2] as usize]), 1.0 / 3.0));
            }
        }
        let close: f32 = std::env::var("ZONE_D").ok().and_then(|x| x.parse().ok()).unwrap_or(2.0);
        if debug {
            let mut dmin = f32::MAX;
            for a in 0..sing.len() {
                for b in a + 1..sing.len() {
                    dmin = dmin.min(len(sub(sing[a], sing[b])));
                }
            }
            eprintln!("h {h:.3}, écart minimal entre singularités {dmin:.3}");
        }
        let mut zones: Vec<[f32; 4]> = vec![];
        for a in 0..sing.len() {
            for b in a + 1..sing.len() {
                if len(sub(sing[a], sing[b])) < h * close {
                    let m = mul(add(sing[a], sing[b]), 0.5);
                    if !zones.iter().any(|z| len(sub(m, [z[0], z[1], z[2]])) < h) {
                        zones.push([m[0], m[1], m[2], h * 2.0]);
                    }
                }
            }
        }
        if !zones.is_empty() {
            if debug {
                eprintln!("singularités serrées : {} zones affinées, on recommence", zones.len());
            }
            let mut o2 = opt.clone();
            o2.zones = zones;
            return remesh_core(pos0, tris0, guides, &o2, progress);
        }
    }
    if let Ok(fp) = std::env::var("FIELD_ONLY") {
        use std::io::Write;
        let mut w = std::fs::File::create(&fp).unwrap();
        for fi in 0..f.len() {
            let (c, _, _) = g.closure(&f[fi], fi);
            if c != 0 {
                let t = f[fi];
                let ce = mul(add(add(v[t[0] as usize], v[t[1] as usize]), v[t[2] as usize]), 1.0 / 3.0);
                writeln!(w, "{} {} {} {}", ce[0], ce[1], ce[2], c).unwrap();
            }
        }
        eprintln!("champ seul : {} singularités", stats.field_sing);
        std::process::exit(0);
    }
    let mut phi = scale_field(&g, &f, &v, &nrm, &q, opt.adaptive);
    // Densité peinte : les quads rétrécissent (ou grandissent) autour de chaque touche
    if !opt.density.is_empty() {
        for i in 0..n_v {
            let mut best = 0.0f32;
            for d in &opt.density {
                let dist = len(sub(v[i], [d[0], d[1], d[2]]));
                let r = d[3].max(1e-6);
                if dist < r {
                    let w = (1.0 - (dist / r).powi(2)).powi(2);
                    // Le solveur absorbe environ la moitié de la variation : on la double pour obtenir le facteur demandé
                    let k = 2.0 * d[4].clamp(0.05, 20.0).ln() * w;
                    if k.abs() > best.abs() {
                        best = k;
                    }
                }
            }
            phi[i] -= best;
        }
    }
    lap("échelle locale");
    let geo = edge_geometry(&g, &v, &nrm, &q, h, &phi);
    let geo_area: Vec<f64> = f
        .iter()
        .map(|t| {
            let s = (phi[t[0] as usize] + phi[t[1] as usize] + phi[t[2] as usize]) / 3.0;
            (len(cross(sub(v[t[1] as usize], v[t[0] as usize]), sub(v[t[2] as usize], v[t[0] as usize]))) * 0.5 / (h * h * (2.0 * s).exp())) as f64
        })
        .collect();
    let mut u = vec![[0.0f64; 2]; n_v];
    let mut curves = build_curves(&g, &ft, &nrm, &q);
    if std::env::var("NORINGS").is_err() {
        let rings = ring_curves(&g, &v, &nrm, &q, &adj, &levels[0].a, h);
        if debug {
            eprintln!("anneaux à refermer : {}", rings.len());
            if std::env::var("RING_DBG").is_ok() {
                for (k, c) in rings.iter().enumerate() {
                    let m = c.verts.len() as f32;
                    let ce = c.verts.iter().fold([0.0f32; 3], |a, &i| add(a, mul(v[i as usize], 1.0 / m)));
                    eprintln!("  anneau {} : {} sommets, centre ({:.1}, {:.1}, {:.1}), écart initial {:?}", curves.len() + k, c.verts.len(), ce[0], ce[1], ce[2], curve_mismatch(&g, c));
                }
            }
        }
        curves.extend(rings);
    }
    if debug {
        eprintln!("lignes caractéristiques : {} ({} arêtes)", curves.len(), ft.edges.len());
    }
    progress(0.35);
    stats.unfixed = place_cones(&mut g, &f, &geo, &mut u, &geo_area, &mut curves, debug, progress);
    progress(0.92);
    lap("singularités sur la grille");
    stats.flips = count_flips(&g, &f);
    if debug {
        let mut hist = [0usize; 4];
        for d in &g.d {
            hist[(d[0].abs().max(d[1].abs()) as usize).min(3)] += 1;
        }
        eprintln!("|d|inf {hist:?} ; replis {} ; singularités hors grille {}", stats.flips, stats.unfixed);
    }
    // Point de grille de chaque sommet
    let o: Vec<V3> = (0..n_v)
        .map(|i| {
            let hi = h * phi[i].exp();
            sub(v[i], add(mul(q[i], u[i][0] as f32 * hi), mul(cross(nrm[i], q[i]), u[i][1] as f32 * hi)))
        })
        .collect();
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
    // Par groupe : somme et nombre des sommets sur une ligne, et coin éventuel
    let mut fcl: HashMap<u32, (V3, f32, Option<V3>)> = HashMap::new();
    for i in 0..n_v {
        let r = uf.find(i as u32);
        let e = cluster.entry(r).or_insert(([0.0; 3], [0.0; 3], 0.0));
        e.0 = add(e.0, o[i]);
        e.1 = add(e.1, nrm[i]);
        e.2 += 1.0;
        if ft.deg[i] > 0 {
            let fe = fcl.entry(r).or_insert(([0.0; 3], 0.0, None));
            if ft.deg[i] == 2 {
                fe.0 = add(fe.0, o[i]);
                fe.1 += 1.0;
            } else {
                fe.2 = Some(v[i]);
            }
        }
    }
    let mut id: HashMap<u32, u32> = HashMap::new();
    let mut out_pos: Vec<V3> = vec![];
    let mut out_n: Vec<V3> = vec![];
    // 0 : sommet libre, 1 : sur une ligne caractéristique, 2 : coin (ne bouge pas)
    let mut out_flag: Vec<u8> = vec![];
    let mut keys: Vec<u32> = cluster.keys().copied().collect();
    keys.sort_unstable();
    for k in keys {
        let (s, n, c) = cluster[&k];
        id.insert(k, out_pos.len() as u32);
        match fcl.get(&k) {
            Some(&(_, _, Some(corner))) => {
                out_pos.push(corner);
                out_flag.push(2);
            }
            Some(&(fs, fc, None)) if fc > 0.0 => {
                out_pos.push(mul(fs, 1.0 / fc));
                out_flag.push(1);
            }
            _ => {
                out_pos.push(mul(s, 1.0 / c));
                out_flag.push(0);
            }
        }
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
    let (rot, open) = rotations(&f, &cl, out_pos.len(), &edges);
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
    let faces = extract_faces(&out_pos, &out_n, &edges, &rot, &open);
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
    let faces = cleanup_quads(&mut out_pos, faces);
    let all_quads = faces.iter().all(|f| f.len() == 4);
    if debug {
        eprintln!("après nettoyage : {} faces dont {} quads", faces.len(), faces.iter().filter(|f| f.len() == 4).count());
    }
    // Ne garder que les sommets utilisés
    out_flag.resize(out_pos.len(), 0);
    let mut remap = vec![u32::MAX; out_pos.len()];
    let mut pos2 = vec![];
    let mut flags: Vec<u8> = vec![];
    let faces: Vec<Vec<u32>> = faces
        .into_iter()
        .map(|f| {
            f.into_iter()
                .map(|i| {
                    if remap[i as usize] == u32::MAX {
                        remap[i as usize] = pos2.len() as u32;
                        pos2.push(out_pos[i as usize]);
                        flags.push(out_flag[i as usize]);
                    }
                    remap[i as usize]
                })
                .collect()
        })
        .collect();
    lap("extraction");
    let segs: Vec<(V3, V3)> = ft.edges.iter().map(|&(a, b)| (v[a as usize], v[b as usize])).collect();
    let mut res = Result { pos: pos2, faces, stats };
    relax(&mut res, &orig_v, &orig_f, 3, &flags, &segs, 0.4 * h);
    if opt.pure_quads && (!all_quads || opt.force_split) {
        res.stats.split = true;
        res = split_quads(&res);
        flags.resize(res.pos.len(), 0);
        relax(&mut res, &orig_v, &orig_f, 2, &flags, &segs, 0.4 * h);
    }
    if let Some(ax) = sym {
        mirror(&mut res, ax, h * 0.02);
        // Un coin de la demi-surface posé sur le plan devient un sommet de valence 2 : on le retire
        let mut faces = std::mem::take(&mut res.faces);
        for _ in 0..4 {
            if !remove_doublets(&mut faces) {
                break;
            }
        }
        res.faces = faces;
    }
    make_manifold(&mut res);
    fill_holes(&mut res, &orig_v, &orig_f, h);
    // Expérimental : n'arrive pas encore à défaire les étoiles des pointes (voir banc_stars)
    if std::env::var("STARS").is_ok() {
        fix_stars(&mut res);
    }
    // Pôles : sommets intérieurs dont la valence n'est pas 4
    {
        let mut val = vec![0u32; res.pos.len()];
        let mut ec: HashMap<(u32, u32), u8> = HashMap::new();
        for f in &res.faces {
            for k in 0..f.len() {
                val[f[k] as usize] += 1;
                let (a, b) = (f[k], f[(k + 1) % f.len()]);
                *ec.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        let mut border = vec![false; res.pos.len()];
        for (&(a, b), &c) in &ec {
            if c == 1 {
                border[a as usize] = true;
                border[b as usize] = true;
            }
        }
        res.stats.poles = (0..res.pos.len()).filter(|&i| !border[i] && val[i] != 4 && val[i] != 0).count();
    }
    lap("lissage final");
    progress(1.0);
    res
}

/// Étoiles : un sommet à 6 arêtes ou plus au bout d'une forme fine (pointe de corne, bout de
/// doigt), avec ses rayons qui finissent sur des pôles à 3 arêtes (plusieurs singularités sont
/// tombées sur le même point). On écrase la bande de quads entre deux rayons voisins, du
/// centre jusqu'aux deux pôles : le centre perd une arête et les deux pôles n'en font plus
/// qu'un, régulier. On répète jusqu'à un centre à 4 arêtes : un capuchon propre à 4 pôles.
fn fix_stars(r: &mut Result) {
    let mut any = false;
    let mut tried: HashSet<(u32, u32)> = HashSet::new();
    for _ in 0..200 {
        let nv = r.pos.len();
        let mut nb: Vec<HashSet<u32>> = vec![HashSet::new(); nv];
        let mut ef: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        for (fi, f) in r.faces.iter().enumerate() {
            for k in 0..f.len() {
                let (a, b) = (f[k], f[(k + 1) % f.len()]);
                nb[a as usize].insert(b);
                nb[b as usize].insert(a);
                ef.entry((a.min(b), a.max(b))).or_default().push(fi as u32);
            }
        }
        let mut border = vec![false; nv];
        for (&(a, b), fs) in &ef {
            if fs.len() != 2 {
                border[a as usize] = true;
                border[b as usize] = true;
            }
        }
        let val = |i: u32| nb[i as usize].len();
        let edge_faces = |a: u32, b: u32| ef.get(&(a.min(b), a.max(b))).cloned().unwrap_or_default();
        // Sommet suivant en ligne droite à travers un sommet régulier
        let straight = |prev: u32, x: u32| -> Option<u32> {
            if val(x) != 4 || border[x as usize] {
                return None;
            }
            let fa = edge_faces(prev, x);
            nb[x as usize].iter().copied().find(|&w| w != prev && !edge_faces(x, w).iter().any(|f| fa.contains(f)))
        };
        let quad_with = |a: u32, b: u32, c: u32, d: u32| -> Option<u32> {
            edge_faces(a, b).into_iter().find(|&f| {
                let g = &r.faces[f as usize];
                g.len() == 4 && g.contains(&c) && g.contains(&d)
            })
        };
        let mut plan: Option<(Vec<u32>, Vec<(u32, u32)>, (u32, u32))> = None;
        'find: for (fi, f) in r.faces.iter().enumerate() {
            if f.len() != 4 {
                continue;
            }
            for k in 0..4 {
                let c = f[k];
                if border[c as usize] || val(c) < 6 || tried.contains(&(c, fi as u32)) {
                    continue;
                }
                let (mut a, mut b) = (f[(k + 1) % 4], f[(k + 3) % 4]);
                let (mut pa, mut pb) = (c, c);
                let mut strip = vec![fi as u32];
                let mut pairs = vec![];
                for _ in 0..12 {
                    pairs.push((a, b));
                    if val(a) == 3 && val(b) == 3 && !border[a as usize] && !border[b as usize] {
                        plan = Some((strip, pairs, (c, fi as u32)));
                        break 'find;
                    }
                    let (Some(na), Some(nb2)) = (straight(pa, a), straight(pb, b)) else { break };
                    let Some(q) = quad_with(a, na, b, nb2) else { break };
                    strip.push(q);
                    (pa, pb, a, b) = (a, b, na, nb2);
                }
                tried.insert((c, fi as u32));
            }
        }
        let Some((strip, pairs, key)) = plan else { break };
        let saved = (r.pos.clone(), r.faces.clone());
        let mut strip = strip;
        strip.sort_unstable();
        for &fi in strip.iter().rev() {
            r.faces.swap_remove(fi as usize);
        }
        let map: HashMap<u32, u32> = pairs.iter().map(|&(a, b)| (b, a)).collect();
        for &(a, b) in &pairs {
            r.pos[a as usize] = mul(add(r.pos[a as usize], r.pos[b as usize]), 0.5);
        }
        for f in r.faces.iter_mut() {
            for x in f.iter_mut() {
                if let Some(&a) = map.get(x) {
                    *x = a;
                }
            }
        }
        // Garde-fou : pas de face dégénérée ni de sommet à moins de 3 arêtes
        let mut ok = r.faces.iter().all(|f| {
            let mut g = f.clone();
            g.sort_unstable();
            g.dedup();
            g.len() == f.len()
        });
        if ok {
            let mut nb2: HashMap<u32, HashSet<u32>> = HashMap::new();
            for f in &r.faces {
                for k in 0..f.len() {
                    let (a, b) = (f[k], f[(k + 1) % f.len()]);
                    if pairs.iter().any(|p| p.0 == a || p.0 == b) {
                        nb2.entry(a).or_default().insert(b);
                        nb2.entry(b).or_default().insert(a);
                    }
                }
            }
            ok = pairs.iter().all(|p| nb2.get(&p.0).is_some_and(|s| s.len() >= 3));
        }
        if !ok {
            (r.pos, r.faces) = saved;
            tried.insert(key);
            continue;
        }
        any = true;
        tried.clear();
    }
    if any {
        // Sommets devenus orphelins retirés
        let mut map = vec![u32::MAX; r.pos.len()];
        let mut pos = vec![];
        for f in r.faces.iter_mut() {
            for x in f.iter_mut() {
                if map[*x as usize] == u32::MAX {
                    map[*x as usize] = pos.len() as u32;
                    pos.push(r.pos[*x as usize]);
                }
                *x = map[*x as usize];
            }
        }
        r.pos = pos;
    }
}

/// Petits trous laissés par l'extraction (quelques faces manquantes) : refermés par des quads
/// autour d'un sommet central, sauf là où la surface de départ avait elle-même un bord.
fn fill_holes(r: &mut Result, ov: &[V3], of: &[[u32; 3]], h: f32) {
    let mut ec: HashMap<(u32, u32), u8> = HashMap::new();
    for t in of {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *ec.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    let cell = h * 2.0;
    let key = |x: V3| ((x[0] / cell).floor() as i32, (x[1] / cell).floor() as i32, (x[2] / cell).floor() as i32);
    let mut border: HashSet<(i32, i32, i32)> = HashSet::new();
    for (&(a, b), &c) in &ec {
        if c == 1 {
            border.insert(key(ov[a as usize]));
            border.insert(key(ov[b as usize]));
        }
    }
    let near_border = |x: V3| {
        let (a, b, c) = key(x);
        (-1..=1).any(|i| (-1..=1).any(|j| (-1..=1).any(|k| border.contains(&(a + i, b + j, c + k)))))
    };
    // Arêtes de bord orientées (sens de la face), parcourues à l'envers pour suivre le trou
    let mut cnt: HashMap<(u32, u32), u32> = HashMap::new();
    for f in &r.faces {
        for k in 0..f.len() {
            let (a, b) = (f[k], f[(k + 1) % f.len()]);
            *cnt.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    let mut next: HashMap<u32, u32> = HashMap::new();
    let mut bad: HashSet<u32> = HashSet::new();
    for f in &r.faces {
        for k in 0..f.len() {
            let (a, b) = (f[k], f[(k + 1) % f.len()]);
            if cnt[&(a.min(b), a.max(b))] == 1 && next.insert(b, a).is_some() {
                bad.insert(b);
            }
        }
    }
    let mut seen: HashSet<u32> = HashSet::new();
    let mut added = vec![];
    let starts: Vec<u32> = next.keys().copied().collect();
    for s in starts {
        if seen.contains(&s) || bad.contains(&s) {
            continue;
        }
        let mut lp = vec![s];
        let mut cur = next[&s];
        let mut ok = true;
        while cur != s {
            if lp.len() > 12 || bad.contains(&cur) || !next.contains_key(&cur) {
                ok = false;
                break;
            }
            lp.push(cur);
            cur = next[&cur];
        }
        for &x in &lp {
            seen.insert(x);
        }
        if !ok || lp.len() < 3 {
            continue;
        }
        let m = lp.len();
        let c = lp.iter().fold([0.0f32; 3], |a, &i| add(a, mul(r.pos[i as usize], 1.0 / m as f32)));
        if near_border(c) {
            continue;
        }
        match m {
            3 | 4 => added.push(lp),
            _ => {
                let ci = r.pos.len() as u32;
                r.pos.push(c);
                let mut i = 0;
                while i + 2 <= m {
                    if i + 2 == m {
                        added.push(vec![ci, lp[i], lp[(i + 1) % m]]);
                    } else {
                        added.push(vec![ci, lp[i], lp[i + 1], lp[(i + 2) % m]]);
                    }
                    i += 2;
                }
            }
        }
    }
    r.faces.extend(added);
}

/// Dernier filet : une arête partagée par plus de deux faces (aileron) perd ses faces les plus
/// petites, puis toutes les faces prennent le sens de leurs voisines (pas de face retournée).
fn make_manifold(r: &mut Result) {
    let area = |f: &Vec<u32>| -> f32 {
        let mut a = [0.0; 3];
        for k in 1..f.len().saturating_sub(1) {
            a = add(a, cross(sub(r.pos[f[k] as usize], r.pos[f[0] as usize]), sub(r.pos[f[k + 1] as usize], r.pos[f[0] as usize])));
        }
        len(a)
    };
    for _ in 0..3 {
        let mut ef: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
        for (fi, f) in r.faces.iter().enumerate() {
            for k in 0..f.len() {
                let (a, b) = (f[k], f[(k + 1) % f.len()]);
                ef.entry((a.min(b), a.max(b))).or_default().push(fi);
            }
        }
        let mut drop = vec![false; r.faces.len()];
        let mut any = false;
        for l in ef.values() {
            let live: Vec<usize> = l.iter().copied().filter(|&f| !drop[f]).collect();
            if live.len() > 2 {
                let mut by: Vec<(f32, usize)> = live.iter().map(|&f| (area(&r.faces[f]), f)).collect();
                by.sort_by(|x, y| y.0.total_cmp(&x.0));
                for &(_, f) in &by[2..] {
                    drop[f] = true;
                    any = true;
                }
            }
        }
        if !any {
            break;
        }
        let mut i = 0;
        r.faces.retain(|_| {
            i += 1;
            !drop[i - 1]
        });
    }
    // Orientation cohérente par composante (parcours en largeur)
    let mut ef: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (fi, f) in r.faces.iter().enumerate() {
        for k in 0..f.len() {
            let (a, b) = (f[k], f[(k + 1) % f.len()]);
            ef.entry((a.min(b), a.max(b))).or_default().push(fi);
        }
    }
    let n = r.faces.len();
    let mut seen = vec![false; n];
    let dir = |f: &Vec<u32>, a: u32, b: u32| -> bool { (0..f.len()).any(|k| f[k] == a && f[(k + 1) % f.len()] == b) };
    for s in 0..n {
        if seen[s] {
            continue;
        }
        seen[s] = true;
        let mut comp = vec![s];
        let mut queue = std::collections::VecDeque::from([s]);
        while let Some(fi) = queue.pop_front() {
            let f = r.faces[fi].clone();
            for k in 0..f.len() {
                let (a, b) = (f[k], f[(k + 1) % f.len()]);
                for &g in &ef[&(a.min(b), a.max(b))] {
                    if g == fi || seen[g] {
                        continue;
                    }
                    seen[g] = true;
                    // Une voisine bien orientée parcourt l'arête commune dans l'autre sens
                    if dir(&r.faces[g], a, b) {
                        r.faces[g].reverse();
                    }
                    comp.push(g);
                    queue.push_back(g);
                }
            }
        }
        // La composante pointe vers l'extérieur si son volume signé est positif
        let mut vol = 0.0f32;
        for &fi in &comp {
            let f = &r.faces[fi];
            for k in 1..f.len() - 1 {
                vol += dot(r.pos[f[0] as usize], cross(r.pos[f[k] as usize], r.pos[f[k + 1] as usize]));
            }
        }
        if vol < 0.0 {
            for &fi in &comp {
                r.faces[fi].reverse();
            }
        }
    }
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
    // Le système est presque singulier sur certains maillages : au-delà de 1500 itérations le
    // champ ne bouge plus de façon visible (le départ, z = 1, est déjà le champ lissé)
    let tol: f64 = std::env::var("LFTOL").ok().and_then(|x| x.parse().ok()).unwrap_or(1e-10);
    let stop = dotv(&rhs, &rhs) * tol;
    let mut it = 0;
    while it < 1500 && dotv(&res, &res) > stop {
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

/// Échelle locale (logarithme de la taille des cases) qui rend le champ de croix intégrable.
/// Un repère e^{-φ}·R(θ) est le gradient d'une carte si et seulement si ∇φ = (∂θ/∂y, −∂θ/∂x)
/// (équations de Cauchy-Riemann) : là où les lignes du flux se resserrent, les quads doivent
/// rétrécir. Sans cette échelle, les moindres carrés écrasent ou replient la grille.
fn scale_field(g: &Graph, f: &[[u32; 3]], v: &[V3], n: &[V3], q: &[V3], strength: f32) -> Vec<f32> {
    let nv = v.len();
    if strength <= 0.0 {
        return vec![0.0; nv];
    }
    let ne = g.ends.len();
    let mut skip = vec![false; ne];
    for fi in 0..f.len() {
        if g.closure(&f[fi], fi).0 != 0 {
            for &e in &g.fe[fi] {
                skip[e as usize] = true;
            }
        }
    }
    // Rotation du champ le long de chaque arête (de i vers j), et arête dans les deux repères
    let mut rho = vec![0.0f32; ne];
    let mut ei = vec![[0.0f32; 2]; ne];
    let mut ej = vec![[0.0f32; 2]; ne];
    for e in 0..ne {
        let (i, j) = (g.ends[e].0 as usize, g.ends[e].1 as usize);
        let (ax, c) = (cross(n[j], n[i]), dot(n[j], n[i]));
        let mut x = if c > -0.99 { norm(add(add(mul(q[j], c), cross(ax, q[j])), mul(ax, dot(ax, q[j]) / (1.0 + c)))) } else { q[j] };
        for _ in 0..g.k[e] {
            x = cross(n[i], x);
        }
        let ti = cross(n[i], q[i]);
        rho[e] = dot(x, ti).atan2(dot(x, q[i]));
        let d = sub(v[j], v[i]);
        let l = len(d);
        let unit = |a: [f32; 2]| {
            let m = (a[0] * a[0] + a[1] * a[1]).sqrt();
            if m > 1e-12 { [a[0] / m * l, a[1] / m * l] } else { [0.0, 0.0] }
        };
        ei[e] = unit([dot(d, q[i]), dot(d, ti)]);
        ej[e] = unit([dot(d, q[j]), dot(d, cross(n[j], q[j]))]);
    }
    // Gradient de θ par sommet (moindres carrés sur son anneau), tourné d'un quart de tour
    let mut m = vec![[0.0f32; 3]; nv];
    let mut b = vec![[0.0f32; 2]; nv];
    for e in 0..ne {
        if skip[e] {
            continue;
        }
        let (i, j) = (g.ends[e].0 as usize, g.ends[e].1 as usize);
        let a = ei[e];
        m[i][0] += a[0] * a[0];
        m[i][1] += a[0] * a[1];
        m[i][2] += a[1] * a[1];
        b[i][0] += a[0] * rho[e];
        b[i][1] += a[1] * rho[e];
        // Vu depuis j : arête −e_j, rotation −ρ (le produit est le même)
        let a = ej[e];
        m[j][0] += a[0] * a[0];
        m[j][1] += a[0] * a[1];
        m[j][2] += a[1] * a[1];
        b[j][0] += a[0] * rho[e];
        b[j][1] += a[1] * rho[e];
    }
    let grad: Vec<[f32; 2]> = (0..nv)
        .map(|i| {
            let det = m[i][0] * m[i][2] - m[i][1] * m[i][1];
            if det.abs() < 1e-20 {
                return [0.0, 0.0];
            }
            let gx = (b[i][0] * m[i][2] - b[i][1] * m[i][1]) / det;
            let gy = (m[i][0] * b[i][1] - m[i][1] * b[i][0]) / det;
            [gy, -gx]
        })
        .collect();
    let sign: f32 = std::env::var("PHISIGN").ok().and_then(|x| x.parse().ok()).unwrap_or(1.0);
    let c: Vec<f64> = (0..ne)
        .map(|e| {
            if skip[e] {
                return 0.0;
            }
            let (i, j) = (g.ends[e].0 as usize, g.ends[e].1 as usize);
            (sign * 0.5 * (grad[i][0] * ei[e][0] + grad[i][1] * ei[e][1] + grad[j][0] * ej[e][0] + grad[j][1] * ej[e][1])) as f64
        })
        .collect();
    // Poisson : min Σ (φ_j − φ_i − c)² + μ Σ φ²
    let mu = 1e-4f64;
    let mut deg = vec![mu; nv];
    let mut rhs = vec![0.0f64; nv];
    for e in 0..ne {
        if skip[e] {
            continue;
        }
        let (i, j) = (g.ends[e].0 as usize, g.ends[e].1 as usize);
        deg[i] += 1.0;
        deg[j] += 1.0;
        rhs[i] -= c[e];
        rhs[j] += c[e];
    }
    let apply = |x: &[f64], out: &mut [f64]| {
        for i in 0..nv {
            out[i] = mu * x[i];
        }
        for e in 0..ne {
            if skip[e] {
                continue;
            }
            let (i, j) = (g.ends[e].0 as usize, g.ends[e].1 as usize);
            let d = x[i] - x[j];
            out[i] += d;
            out[j] -= d;
        }
    };
    let dotv = |a: &[f64], b: &[f64]| -> f64 { a.iter().zip(b).map(|(x, y)| x * y).sum() };
    let mut x = vec![0.0f64; nv];
    let mut r = rhs.clone();
    let mut z: Vec<f64> = r.iter().zip(&deg).map(|(a, d)| a / d).collect();
    let mut p = z.clone();
    let mut rz = dotv(&r, &z);
    let stop = dotv(&rhs, &rhs) * 1e-12 + 1e-30;
    let mut ap = vec![0.0f64; nv];
    for _ in 0..3000 {
        if dotv(&r, &r) < stop {
            break;
        }
        apply(&p, &mut ap);
        let alpha = rz / dotv(&p, &ap).max(1e-300);
        for i in 0..nv {
            x[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
            z[i] = r[i] / deg[i];
        }
        let rz2 = dotv(&r, &z);
        let beta = rz2 / rz;
        rz = rz2;
        for i in 0..nv {
            p[i] = z[i] + beta * p[i];
        }
    }
    // Force, bornes (cases de 2 fois plus petites à 2 fois plus grandes), puis recentrage pour
    // garder le nombre de faces demandé
    let mut area = vec![0.0f32; nv];
    for t in f {
        let a = len(cross(sub(v[t[1] as usize], v[t[0] as usize]), sub(v[t[2] as usize], v[t[0] as usize]))) / 6.0;
        for &i in t {
            area[i as usize] += a;
        }
    }
    let lim: f32 = std::env::var("PHILIM").ok().and_then(|x| x.parse().ok()).unwrap_or(0.7);
    let mut phi: Vec<f32> = x.iter().map(|&p| (p as f32 * strength).clamp(-lim, lim)).collect();
    for _ in 0..3 {
        let (mut num, mut den) = (0.0f64, 0.0f64);
        for i in 0..nv {
            num += (area[i] * (-2.0 * phi[i]).exp()) as f64;
            den += area[i] as f64;
        }
        let shift = 0.5 * (num / den.max(1e-30)).ln() as f32;
        for p in phi.iter_mut() {
            *p = (*p + shift).clamp(-lim, lim);
        }
    }
    phi
}

/// Rotation (0 à 3 quarts de tour) qui aligne le repère de j sur celui de i.
fn matching(n: &[V3], q: &[V3], i: usize, j: usize) -> u8 {
    let (ax, c) = (cross(n[j], n[i]), dot(n[j], n[i]));
    let qj = if c > -0.99 { norm(add(add(mul(q[j], c), cross(ax, q[j])), mul(ax, dot(ax, q[j]) / (1.0 + c)))) } else { q[j] };
    let tj = cross(n[i], qj);
    let cands = [qj, tj, mul(qj, -1.0), mul(tj, -1.0)];
    (0..4).max_by(|&a, &b| dot(q[i], cands[a]).total_cmp(&dot(q[i], cands[b]))).unwrap() as u8
}

/// Indice de singularité de chaque triangle (0 : régulier, 1 ou 3 : quart de tour dans un sens ou l'autre).
fn singular_faces(f: &[[u32; 3]], n: &[V3], q: &[V3]) -> Vec<(u32, u8)> {
    let mut out = vec![];
    for (fi, t) in f.iter().enumerate() {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        let k = (matching(n, q, a, b) + matching(n, q, b, c) + matching(n, q, c, a)) & 3;
        if k != 0 {
            out.push((fi as u32, k));
        }
    }
    out
}

/// Annule les paires de singularités opposées trop proches (« dipôles ») : dans un voisinage
/// de la paire, le champ est relissé en gardant fixe ce qui l'entoure. Une paire 3-5 collée
/// n'apporte rien au flux et finit en amas de pôles ; on préfère une zone régulière.
fn cancel_dipoles(v: &[V3], n: &[V3], adj: &[Vec<u32>], f: &[[u32; 3]], q: &mut [V3], radius: f32, pinned: &[bool]) -> usize {
    let mut removed = 0;
    for _ in 0..3 {
        let sing = singular_faces(f, n, q);
        if sing.len() < 2 {
            break;
        }
        let center = |fi: u32| { let t = f[fi as usize]; mul(add(add(v[t[0] as usize], v[t[1] as usize]), v[t[2] as usize]), 1.0 / 3.0) };
        // Paires opposées les plus proches d'abord
        let mut pairs: Vec<(f32, usize, usize)> = vec![];
        for a in 0..sing.len() {
            for b in a + 1..sing.len() {
                if (sing[a].1 + sing[b].1) & 3 == 0 {
                    let d = len(sub(center(sing[a].0), center(sing[b].0)));
                    if d < radius {
                        pairs.push((d, a, b));
                    }
                }
            }
        }
        if pairs.is_empty() {
            break;
        }
        pairs.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut used = vec![false; sing.len()];
        let mut region = vec![false; v.len()];
        let mut any = false;
        for (d, a, b) in pairs {
            if used[a] || used[b] {
                continue;
            }
            used[a] = true;
            used[b] = true;
            let mid = mul(add(center(sing[a].0), center(sing[b].0)), 0.5);
            let r = d * 0.5 + radius * 0.6;
            // Sommets à moins de r du milieu, par propagation depuis les deux triangles
            let mut stack: Vec<u32> = f[sing[a].0 as usize].iter().chain(f[sing[b].0 as usize].iter()).copied().collect();
            while let Some(x) = stack.pop() {
                if region[x as usize] || pinned[x as usize] || len(sub(v[x as usize], mid)) > r {
                    continue;
                }
                region[x as usize] = true;
                stack.extend(adj[x as usize].iter().copied());
            }
            any = true;
        }
        if !any {
            break;
        }
        let list: Vec<usize> = (0..v.len()).filter(|&i| region[i]).collect();
        for _ in 0..120 {
            for &i in &list {
                let ni = n[i];
                let mut qi = q[i];
                let mut ws = 0.0;
                for &j in &adj[i] {
                    let (a, b) = compat_orient(qi, ni, q[j as usize], n[j as usize]);
                    qi = add(mul(a, ws), b);
                    ws += 1.0;
                    qi = norm(tangent(qi, ni));
                }
                q[i] = qi;
            }
        }
        let after = singular_faces(f, n, q).len();
        removed += sing.len().saturating_sub(after);
    }
    removed
}

/// Directions principales de courbure et anisotropie |κ1 − κ2|·h (sans unité) par sommet.
/// Tenseur de Taubin par anneau, lissé à l'échelle d'un quad pour ignorer le bruit du sculpt.
fn curvature(v: &[V3], f: &[[u32; 3]], n: &[V3], adj: &[Vec<u32>], h: f32) -> (Vec<V3>, Vec<f32>, Vec<f32>) {
    // Tenseur du cycle normal (Cohen-Steiner & Morvan) : chaque arête apporte son angle dièdre
    // dans sa propre direction. Contrairement à un estimateur par voisins, il ne penche pas
    // selon le sens des diagonales de la triangulation (ce qui tordait les boucles en spirale).
    let nv = v.len();
    let mut t = vec![[0.0f32; 6]; nv];
    let mut area = vec![0.0f32; nv];
    let mut fnrm = Vec::with_capacity(f.len());
    let mut cen = Vec::with_capacity(f.len());
    let mut owner: HashMap<(u32, u32), u32> = HashMap::with_capacity(f.len() * 3 / 2);
    for (fi, tri) in f.iter().enumerate() {
        let (a, b, c) = (v[tri[0] as usize], v[tri[1] as usize], v[tri[2] as usize]);
        let cr = cross(sub(b, a), sub(c, a));
        let ar = len(cr) * 0.5;
        for &k in tri {
            area[k as usize] += ar / 3.0;
        }
        fnrm.push(norm(cr));
        cen.push(mul(add(add(a, b), c), 1.0 / 3.0));
        for r in 0..3 {
            let (x, y) = (tri[r], tri[(r + 1) % 3]);
            let key = (x.min(y), x.max(y));
            match owner.remove(&key) {
                Some(g) => {
                    let g = g as usize;
                    let e = sub(v[y as usize], v[x as usize]);
                    let l = len(e);
                    if l < 1e-12 {
                        continue;
                    }
                    let (n1, n2) = (fnrm[g], fnrm[fi]);
                    let ang = dot(n1, n2).clamp(-1.0, 1.0).acos();
                    let sign = if dot(sub(n2, n1), sub(cen[fi], cen[g])) >= 0.0 { 1.0 } else { -1.0 };
                    let w = 0.5 * sign * ang * l;
                    let d = mul(e, 1.0 / l);
                    let m = [w * d[0] * d[0], w * d[1] * d[1], w * d[2] * d[2], w * d[0] * d[1], w * d[0] * d[2], w * d[1] * d[2]];
                    for k in [x, y] {
                        for (o, x) in t[k as usize].iter_mut().zip(m) {
                            *o += x;
                        }
                    }
                }
                None => {
                    owner.insert(key, fi as u32);
                }
            }
        }
    }
    let (mut total, mut count) = (0.0f64, 0usize);
    for i in 0..nv {
        for &j in &adj[i] {
            total += len(sub(v[j as usize], v[i])) as f64;
            count += 1;
        }
    }
    let mean_edge = (total / count.max(1) as f64) as f32;
    let ps: f32 = std::env::var("PS").ok().and_then(|x| x.parse().ok()).unwrap_or(2.0);
    let passes = (((h / mean_edge.max(1e-9)).powi(2) * ps).ceil() as usize).clamp(2, 400);
    // Lissage du tenseur intégré et de l'aire ensemble, puis division : une moyenne pondérée par
    // l'aire, qui ne laisse pas les petits triangles (zones denses) dominer avec leur bruit
    for _ in 0..passes {
        let (old, olda) = (t.clone(), area.clone());
        for i in 0..nv {
            if adj[i].is_empty() {
                continue;
            }
            let mut acc = [0.0f32; 6];
            let mut aa = 0.0;
            for &j in &adj[i] {
                for k in 0..6 {
                    acc[k] += old[j as usize][k];
                }
                aa += olda[j as usize];
            }
            let inv = 0.5 / adj[i].len() as f32;
            for k in 0..6 {
                t[i][k] = 0.5 * old[i][k] + acc[k] * inv;
            }
            area[i] = 0.5 * olda[i] + aa * inv;
        }
    }
    for i in 0..nv {
        if area[i] > 0.0 {
            for x in t[i].iter_mut() {
                *x /= area[i];
            }
        }
    }
    let mut dir = vec![[0.0f32; 3]; nv];
    let mut ani = vec![0.0f32; nv];
    let mut kmin = vec![0.0f32; nv];
    for i in 0..nv {
        let nn = n[i];
        let b1 = norm(tangent(if nn[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] }, nn));
        let b2 = cross(nn, b1);
        let m = t[i];
        let tm = |a: V3, b: V3| -> f32 { a[0] * (m[0] * b[0] + m[3] * b[1] + m[4] * b[2]) + a[1] * (m[3] * b[0] + m[1] * b[1] + m[5] * b[2]) + a[2] * (m[4] * b[0] + m[5] * b[1] + m[2] * b[2]) };
        let (m11, m12, m22) = (tm(b1, b1), tm(b1, b2), tm(b2, b2));
        let th = 0.5 * (2.0 * m12).atan2(m11 - m22);
        dir[i] = add(mul(b1, th.cos()), mul(b2, th.sin()));
        // d = |k1 - k2| / 2 ; trace = k1 + k2
        let d = (((m11 - m22) * 0.5).powi(2) + m12 * m12).sqrt();
        // Part d'anisotropie : 1 sur un cylindre ou un cône, ~0 sur une sphère (où le bruit du
        // maillage donnerait des directions au hasard)
        let r = (2.0 * d / ((m11 + m22).abs() + 1e-12)).min(1.0);
        ani[i] = 2.0 * d * h * r;
        kmin[i] = (m11 + m22) * 0.5 - d;
    }
    (dir, ani, kmin)
}

/// Ordre cyclique (sens trigonométrique) des voisins de chaque point de grille, lu sur la
/// topologie du maillage dense : autour d'un groupe K, deux sommets extérieurs consécutifs
/// x → y forment un triangle (a, x, y) avec a dans K. C'est exact même quand la géométrie
/// est pincée ou repliée, contrairement à un tri par angles.
fn rotations(f: &[[u32; 3]], cl: &[u32], n: usize, edges: &HashSet<(u32, u32)>) -> (Vec<Option<Vec<u32>>>, Vec<bool>) {
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
    // Groupe au bord du maillage : ses voisins forment une chaîne ouverte, pas un cycle
    let open: Vec<bool> = (0..n).map(|k| { let targets: HashSet<u32> = arcs[k].values().copied().collect(); arcs[k].keys().any(|x| !targets.contains(x)) }).collect();
    let rot = (0..n)
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
        .collect();
    (rot, open)
}

/// Faces lues en tournant autour de chaque sommet. `rot` : ordre topologique des voisins
/// quand il est connu, sinon tri par angle dans le plan tangent.
fn extract_faces(pos: &[V3], nrm: &[V3], edges: &HashSet<(u32, u32)>, rot: &[Option<Vec<u32>>], open: &[bool]) -> Vec<Vec<u32>> {
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
                // Face « extérieure » : elle passe par l'ouverture d'un groupe de bord. C'est le
                // contour d'un trou du maillage d'origine, pas une face à créer.
                let outer = (0..f.len()).any(|i| {
                    let (prev, cur) = (f[(i + f.len() - 1) % f.len()], f[i]);
                    open.get(cur as usize).copied().unwrap_or(false) && rot.get(cur as usize).is_some_and(|r| r.is_some()) && nb[cur as usize].first() == Some(&prev)
                });
                if !outer {
                    faces.push(f);
                }
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
// Lignes caractéristiques : arêtes vives, bords, plan de symétrie. Le flux les suit et une
// ligne de la grille passe exactement dessus, comme les « hard edges » de Quad Remesher.
// ---------------------------------------------------------------------------------------

struct Features {
    edges: Vec<(u32, u32)>,
    /// Nombre d'arêtes caractéristiques par sommet (2 : sur une ligne ; autre : coin)
    deg: Vec<u8>,
    tan: Vec<V3>,
}

fn features(v: &[V3], f: &[[u32; 3]], angle_deg: f32, min_len: f32) -> Features {
    let mut map: HashMap<(u32, u32), (u8, V3)> = HashMap::new();
    let cosa = angle_deg.to_radians().cos();
    let mut edges = vec![];
    for t in f {
        let nf = norm(cross(sub(v[t[1] as usize], v[t[0] as usize]), sub(v[t[2] as usize], v[t[0] as usize])));
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let e = map.entry((a.min(b), a.max(b))).or_insert((0, nf));
            e.0 += 1;
            if e.0 == 2 && angle_deg > 0.0 && dot(e.1, nf) < cosa {
                edges.push((a.min(b), a.max(b)));
            }
        }
    }
    let mut border = 0;
    for (&k, &(c, _)) in &map {
        if c == 1 {
            edges.push(k);
            border += 1;
        }
    }
    edges.sort_unstable();
    // Les petits bouts isolés (bruit du sculpt) sont ignorés ; les bords sont toujours gardés
    if min_len > 0.0 && edges.len() > border {
        let mut uf = Uf((0..v.len() as u32).collect());
        for &(a, b) in &edges {
            uf.union(a, b);
        }
        let mut total: HashMap<u32, f32> = HashMap::new();
        for &(a, b) in &edges {
            *total.entry(uf.find(a)).or_insert(0.0) += len(sub(v[a as usize], v[b as usize]));
        }
        edges.retain(|&(a, b)| map[&(a, b)].0 == 1 || total[&uf.find(a)] >= min_len);
    }
    let mut deg = vec![0u8; v.len()];
    let mut ends: Vec<[u32; 2]> = vec![[u32::MAX; 2]; v.len()];
    for &(a, b) in &edges {
        for (x, y) in [(a, b), (b, a)] {
            let d = deg[x as usize] as usize;
            if d < 2 {
                ends[x as usize][d] = y;
            }
            deg[x as usize] = deg[x as usize].saturating_add(1);
        }
    }
    let tan = (0..v.len()).map(|i| if deg[i] == 2 { norm(sub(v[ends[i][0] as usize], v[ends[i][1] as usize])) } else { [0.0; 3] }).collect();
    Features { edges, deg, tan }
}

/// Garde la moitié du maillage du côté positif de l'axe ; la coupe tombe exactement sur le plan.
fn clip_half(pos: &[V3], tris: &[[u32; 3]], axis: usize) -> (Vec<V3>, Vec<[u32; 3]>) {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let eps = len(sub(hi, lo)) * 1e-5;
    let mut pos: Vec<V3> = pos.to_vec();
    for p in pos.iter_mut() {
        if p[axis].abs() < eps {
            p[axis] = 0.0;
        }
    }
    let mut cut: HashMap<(u32, u32), u32> = HashMap::new();
    let mut out = vec![];
    for t in tris {
        let ins = [pos[t[0] as usize][axis] >= 0.0, pos[t[1] as usize][axis] >= 0.0, pos[t[2] as usize][axis] >= 0.0];
        let n_in = ins.iter().filter(|x| **x).count();
        if n_in == 3 {
            out.push(*t);
            continue;
        }
        if n_in == 0 {
            continue;
        }
        let mut poly: Vec<u32> = vec![];
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            if ins[k] {
                poly.push(a);
            }
            if ins[k] != ins[(k + 1) % 3] {
                let key = (a.min(b), a.max(b));
                let id = match cut.get(&key) {
                    Some(&id) => id,
                    None => {
                        let (pa, pb) = (pos[a as usize], pos[b as usize]);
                        let s = pa[axis] / (pa[axis] - pb[axis]);
                        let mut p = add(pa, mul(sub(pb, pa), s));
                        p[axis] = 0.0;
                        pos.push(p);
                        cut.insert(key, (pos.len() - 1) as u32);
                        (pos.len() - 1) as u32
                    }
                };
                poly.push(id);
            }
        }
        poly.dedup();
        for k in 1..poly.len().saturating_sub(1) {
            if poly[0] != poly[k] && poly[k] != poly[k + 1] && poly[0] != poly[k + 1] {
                out.push([poly[0], poly[k], poly[k + 1]]);
            }
        }
    }
    weld(&pos, &out)
}

/// Recopie la moitié en miroir ; les sommets du plan sont partagés.
fn mirror(r: &mut Result, axis: usize, tol: f32) {
    let n = r.pos.len();
    // Les sommets du bord de la demi-surface proches du plan sont ramenés dessus
    let mut ec: HashMap<(u32, u32), u8> = HashMap::new();
    for f in &r.faces {
        for k in 0..f.len() {
            let (a, b) = (f[k], f[(k + 1) % f.len()]);
            *ec.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    let mut border = vec![false; n];
    for (&(a, b), &c) in &ec {
        if c == 1 {
            border[a as usize] = true;
            border[b as usize] = true;
        }
    }
    let mut map = vec![0u32; n];
    for i in 0..n {
        if r.pos[i][axis].abs() < tol || (border[i] && r.pos[i][axis].abs() < tol * 40.0) {
            r.pos[i][axis] = 0.0;
            map[i] = i as u32;
        } else {
            let mut p = r.pos[i];
            p[axis] = -p[axis];
            r.pos.push(p);
            map[i] = (r.pos.len() - 1) as u32;
        }
    }
    let mirrored: Vec<Vec<u32>> = r.faces.iter().filter(|f| f.iter().any(|&i| map[i as usize] != i)).map(|f| f.iter().rev().map(|&i| map[i as usize]).collect()).collect();
    r.faces.extend(mirrored);
}

/// Une ligne caractéristique : chaîne de sommets denses reliés par des arêtes vives ou de bord.
#[derive(Clone)]
struct Curve {
    verts: Vec<u32>,
    /// (indice dans verts, indice dans verts, arête) dans l'ordre d'un parcours depuis verts[0]
    links: Vec<(u32, u32, u32)>,
    /// Composante de u perpendiculaire à la ligne, dans la carte de verts[0]
    c0: u8,
    /// Ligne de grille choisie (dans la carte de verts[0]) une fois arrondie
    fixed: Option<i32>,
    /// Arêtes de la ligne hors de l'arbre de parcours (boucles fermées)
    extra: Vec<(u32, u32, u32)>,
    /// Faux si la boucle ne se referme pas sur la même ligne de grille (elle reste alors libre)
    ok: bool,
    /// Vrai : la ligne est posée sur une ligne de grille (arête vive, bord). Faux : on exige
    /// seulement qu'elle se referme (anneau autour d'un membre : pas de boucle en spirale)
    pin: bool,
}

/// Anneaux du flux : en suivant une direction du champ depuis des points bien orientés, on
/// cherche les lignes qui font le tour d'une forme (corps, membre, corne) et reviennent au
/// départ. Chacune devra se refermer sur la même ligne de grille : sans ça, l'arrondi des
/// singularités laisse un décalage d'une rangée par tour et la boucle part en spirale sur
/// tout le maillage.
/// Boucles de forme : en suivant la direction des arêtes vives douces (rebord d'une paupière,
/// lèvres, narine, base d'une corne), une ligne qui revient à son départ après un tour court
/// entoure un détail qui doit avoir ses propres boucles. Elle sert de guide, comme si on
/// l'avait dessinée. Le long d'un membre la direction suit l'axe et ne se referme pas : les
/// anneaux des membres viennent déjà de la courbure.
fn auto_guides(v: &[V3], n: &[V3], adj: &[Vec<u32>], dir: &[V3], ani: &[f32], h: f32) -> Vec<Vec<V3>> {
    let t: f32 = std::env::var("AUTO_T").ok().and_then(|x| x.parse().ok()).unwrap_or(0.35);
    if std::env::var("AUTO_DBG").is_ok() {
        let mut ks = ani.to_vec();
        ks.sort_by(|a, b| a.total_cmp(b));
        let pc = |x: f32| ks[((ks.len() - 1) as f32 * x) as usize];
        eprintln!("anisotropie : 50 % {:.2}, 80 % {:.2}, 95 % {:.2}, 99 % {:.2}", pc(0.5), pc(0.8), pc(0.95), pc(0.99));
    }
    let nv = v.len();
    let cell = h * 3.0;
    let mut best: HashMap<(i32, i32, i32), u32> = HashMap::new();
    for i in 0..nv {
        if ani[i] < t * 1.5 || adj[i].len() < 3 {
            continue;
        }
        let k = ((v[i][0] / cell).floor() as i32, (v[i][1] / cell).floor() as i32, (v[i][2] / cell).floor() as i32);
        let e = best.entry(k).or_insert(i as u32);
        if ani[*e as usize] < ani[i] {
            *e = i as u32;
        }
    }
    let mut seeds: Vec<u32> = best.into_values().collect();
    seeds.sort_by(|a, b| ani[*b as usize].total_cmp(&ani[*a as usize]));
    seeds.truncate(600);
    let mut covered = vec![false; nv];
    let mut out: Vec<Vec<V3>> = vec![];
    let mut centers: Vec<V3> = vec![];
    // Pas faibles permis d'affilée : environ 3 quads (coin de l'œil où les plis se rejoignent)
    let mean_edge = {
        let (mut t, mut c) = (0.0, 0);
        for i in (0..nv).step_by(7) {
            for &j in &adj[i] {
                t += len(sub(v[i], v[j as usize]));
                c += 1;
            }
        }
        t / c.max(1) as f32
    };
    let gap = ((h * 3.0 / mean_edge.max(1e-9)) as usize).max(3);
    for &s in &seeds {
        if covered[s as usize] {
            continue;
        }
        let mut d = dir[s as usize];
        let mut path = vec![s];
        let mut on: HashMap<u32, usize> = HashMap::new();
        on.insert(s, 0);
        let (mut cur, mut length, mut weak) = (s, 0.0f32, 0);
        let mut closed = false;
        for _ in 0..3000 {
            let c = cur as usize;
            // Refermer : de retour près du départ, en s'en approchant (un trou de 2 quads est permis,
            // le guide est une ligne et n'a pas besoin de suivre les arêtes)
            let back = sub(v[s as usize], v[c]);
            if length > h * 8.0 && len(back) < h * 2.0 && dot(back, d) > 0.0 {
                closed = true;
                break;
            }
            let mut nb = None;
            let mut bd = 0.5;
            for &w in &adj[c] {
                if on.contains_key(&w) {
                    continue;
                }
                let x = dot(norm(sub(v[w as usize], v[c])), d);
                if x > bd {
                    bd = x;
                    nb = Some(w);
                }
            }
            let Some(w) = nb else { break };
            let wi = w as usize;
            length += len(sub(v[wi], v[c]));
            let a = norm(tangent(dir[wi], n[wi]));
            d = if dot(a, d) >= 0.0 { a } else { mul(a, -1.0) };
            weak = if ani[wi] < t { weak + 1 } else { 0 };
            if weak > gap || length > h * 150.0 {
                break;
            }
            on.insert(w, path.len());
            path.push(w);
            cur = w;
        }
        if let Ok(fp) = std::env::var("AUTO_DUMP") {
            use std::io::Write;
            if let Ok(mut fh) = std::fs::OpenOptions::new().create(true).append(true).open(fp) {
                for &p in &path {
                    let _ = writeln!(fh, "{} {} {}", v[p as usize][0], v[p as usize][1], v[p as usize][2]);
                }
                let _ = writeln!(fh, "{}", if closed { "# closed" } else { "" });
            }
        }
        if !closed {
            continue;
        }
        // Un pli franc tout le long (pas une ride de la peau), assez grand, pas un doublon
        let mean_a = path.iter().map(|&p| ani[p as usize]).sum::<f32>() / path.len() as f32;
        let cen = path.iter().fold([0.0f32; 3], |a, &p| add(a, mul(v[p as usize], 1.0 / path.len() as f32)));
        if std::env::var("AUTO_DBG").is_ok() {
            eprintln!("boucle : longueur {:.1} h, anisotropie moyenne {mean_a:.2}, centre ({:.2}, {:.2}, {:.2})", length / h, cen[0], cen[1], cen[2]);
        }
        let amin: f32 = std::env::var("AUTO_A").ok().and_then(|x| x.parse().ok()).unwrap_or(1.15);
        if mean_a < t * amin || length < h * 9.0 || centers.iter().any(|c| len(sub(*c, cen)) < h * 3.0) {
            continue;
        }
        centers.push(cen);
        for &p in &path {
            covered[p as usize] = true;
            for &w in &adj[p as usize] {
                covered[w as usize] = true;
                for &x in &adj[w as usize] {
                    covered[x as usize] = true;
                }
            }
        }
        let mut pts: Vec<V3> = path.iter().map(|&i| v[i as usize]).collect();
        for _ in 0..6 {
            let old = pts.clone();
            let m = old.len();
            for i in 0..m {
                pts[i] = mul(add(add(old[(i + m - 1) % m], old[(i + 1) % m]), mul(old[i], 2.0)), 0.25);
            }
        }
        let first = pts[0];
        pts.push(first);
        out.push(pts);
    }
    out
}

fn ring_curves(g: &Graph, v: &[V3], n: &[V3], q: &[V3], adj: &[Vec<u32>], aniso: &[f32], h: f32) -> Vec<Curve> {
    let nv = v.len();
    let cell = h * 4.0;
    let amin: f32 = std::env::var("RING_A").ok().and_then(|x| x.parse().ok()).unwrap_or(0.02);
    let mut best: HashMap<(i32, i32, i32), u32> = HashMap::new();
    for i in 0..nv {
        if aniso[i] < amin || adj[i].len() < 3 {
            continue;
        }
        let k = ((v[i][0] / cell).floor() as i32, (v[i][1] / cell).floor() as i32, (v[i][2] / cell).floor() as i32);
        let e = best.entry(k).or_insert(i as u32);
        if aniso[*e as usize] < aniso[i] {
            *e = i as u32;
        }
    }
    let mut seeds: Vec<u32> = best.into_values().collect();
    seeds.sort_by(|a, b| aniso[*b as usize].total_cmp(&aniso[*a as usize]));
    seeds.truncate(1500);
    // Direction des anneaux déjà trouvés près de chaque sommet : un anneau dans l'autre sens
    // (le long du corps plutôt qu'autour) reste permis au même endroit
    let mut covered = vec![[0.0f32; 3]; nv];
    let mut out = vec![];
    let max_steps = 4000usize;
    for &s in &seeds {
        for pick in 0..2 {
            let mut dir = if pick == 0 { q[s as usize] } else { cross(n[s as usize], q[s as usize]) };
            if dot(covered[s as usize], dir).abs() > 0.7 {
                continue;
            }
            let mut path = vec![s];
            let mut on: HashMap<u32, usize> = HashMap::new();
            on.insert(s, 0);
            let mut cur = s;
            let mut length = 0.0;
            let mut closed: Option<usize> = None;
            for _ in 0..max_steps {
                let c = cur as usize;
                // Refermer : un des premiers sommets du chemin est voisin et devant nous
                if length > h * 6.0 {
                    let mut hit = None;
                    for &w in &adj[c] {
                        if let Some(&ix) = on.get(&w) {
                            if ix < 4 && dot(norm(sub(v[w as usize], v[c])), dir) > 0.2 {
                                hit = Some(ix);
                                break;
                            }
                        }
                    }
                    if let Some(ix) = hit {
                        closed = Some(ix);
                        break;
                    }
                    // Repassé à côté du départ sans le toucher : on raccorde par le plus court
                    // chemin sur les arêtes (quelques arêtes, hors du chemin déjà fait)
                    let back = sub(v[s as usize], v[c]);
                    if len(back) < h * 2.0 && dot(back, dir) > 0.0 {
                        let mut prev: HashMap<u32, u32> = HashMap::new();
                        let mut queue = std::collections::VecDeque::from([cur]);
                        prev.insert(cur, cur);
                        let mut found = false;
                        while let Some(x) = queue.pop_front() {
                            if x == s {
                                found = true;
                                break;
                            }
                            for &w in &adj[x as usize] {
                                if prev.contains_key(&w) || (on.contains_key(&w) && w != s) || len(sub(v[w as usize], v[s as usize])) > h * 2.5 {
                                    continue;
                                }
                                prev.insert(w, x);
                                queue.push_back(w);
                            }
                        }
                        if found {
                            let mut tail = vec![];
                            let mut x = prev[&s];
                            while x != cur {
                                tail.push(x);
                                x = prev[&x];
                            }
                            tail.reverse();
                            for w in tail {
                                on.insert(w, path.len());
                                path.push(w);
                            }
                            closed = Some(0);
                            break;
                        }
                    }
                }
                let mut nb = None;
                let mut bd = 0.5;
                for &w in &adj[c] {
                    if on.contains_key(&w) {
                        continue;
                    }
                    let d = dot(norm(sub(v[w as usize], v[c])), dir);
                    if d > bd {
                        bd = d;
                        nb = Some(w);
                    }
                }
                let Some(w) = nb else { break };
                length += len(sub(v[w as usize], v[c]));
                // Transport de la direction : celle des 4 du champ en w la plus proche
                let wi = w as usize;
                let (a, b) = (q[wi], cross(n[wi], q[wi]));
                let cand = [a, mul(a, -1.0), b, mul(b, -1.0)];
                dir = *cand.iter().max_by(|x, y| dot(**x, dir).total_cmp(&dot(**y, dir))).unwrap();
                on.insert(w, path.len());
                path.push(w);
                cur = w;
                if length > h * 400.0 {
                    break;
                }
            }
            let Some(ix) = closed else { continue };
            let path: Vec<u32> = path[ix..].to_vec();
            if path.len() < 6 {
                continue;
            }
            let m = path.len();
            let edge = |a: u32, b: u32| g.map.get(&(a.min(b), a.max(b))).copied();
            let mut links = vec![];
            let mut fine = true;
            for i in 0..m - 1 {
                match edge(path[i], path[i + 1]) {
                    Some(e) => links.push((i as u32, i as u32 + 1, e)),
                    None => fine = false,
                }
            }
            let Some(last) = edge(path[m - 1], path[0]) else { continue };
            if !fine {
                continue;
            }
            // Un anneau par bande de quelques quads : des anneaux trop serrés se contredisent
            let spread: f32 = std::env::var("RING_GAP").ok().and_then(|x| x.parse().ok()).unwrap_or(4.0);
            let mut front: Vec<u32> = path.clone();
            let mut mark: HashSet<u32> = HashSet::new();
            for i in 0..m {
                let t = norm(sub(v[path[(i + 1) % m] as usize], v[path[(i + m - 1) % m] as usize]));
                covered[path[i] as usize] = t;
                mark.insert(path[i]);
            }
            while !front.is_empty() {
                let mut next = vec![];
                for &x in &front {
                    for &w in &adj[x as usize] {
                        if !mark.contains(&w) && path.iter().any(|&p| len(sub(v[p as usize], v[w as usize])) < h * spread) {
                            mark.insert(w);
                            let t = covered[x as usize];
                            covered[w as usize] = norm(tangent(t, n[w as usize]));
                            next.push(w);
                        }
                    }
                }
                front = next;
            }
            // Composante perpendiculaire : celle où la boucle avance le moins, mesurée sur tout
            // le tour dans le repère du départ (une seule arête en zigzag peut tromper)
            let mut acc = [0i64; 2];
            let mut rot = 0u8;
            for i in 0..m {
                let (a, b) = (path[i], path[(i + 1) % m]);
                let ai = a as usize;
                let t = sub(v[b as usize], v[ai]);
                let loc = [(dot(t, q[ai]) / h * 1000.0) as i32, (dot(t, cross(n[ai], q[ai])) / h * 1000.0) as i32];
                let r = trot(rot, loc);
                acc[0] += r[0].abs() as i64;
                acc[1] += r[1].abs() as i64;
                if let Some(&e) = g.map.get(&(a.min(b), a.max(b))) {
                    rot = (rot + g.dir(e, a).0) & 3;
                }
            }
            let c0 = if acc[0] >= acc[1] { 1 } else { 0 };
            if let Ok(fp) = std::env::var("RING_DUMP") {
                use std::io::Write;
                if let Ok(mut fh) = std::fs::OpenOptions::new().create(true).append(true).open(fp) {
                    for &p in &path {
                        let _ = writeln!(fh, "{} {} {}", v[p as usize][0], v[p as usize][1], v[p as usize][2]);
                    }
                    let _ = writeln!(fh);
                }
            }
            out.push(Curve { verts: path, links, c0, fixed: None, extra: vec![(m as u32 - 1, 0, last)], ok: true, pin: false });
        }
    }
    out
}

fn build_curves(g: &Graph, ft: &Features, n: &[V3], q: &[V3]) -> Vec<Curve> {
    let nv = ft.deg.len();
    let mut adj: Vec<Vec<(u32, u32)>> = vec![vec![]; nv];
    for &(a, b) in &ft.edges {
        if ft.deg[a as usize] == 2 && ft.deg[b as usize] == 2 {
            if let Some(&e) = g.map.get(&(a, b)) {
                adj[a as usize].push((b, e));
                adj[b as usize].push((a, e));
            }
        }
    }
    let mut seen = vec![false; nv];
    let mut out = vec![];
    for s in 0..nv {
        if seen[s] || ft.deg[s] != 2 || adj[s].is_empty() {
            continue;
        }
        let mut verts = vec![s as u32];
        let mut index: HashMap<u32, u32> = HashMap::new();
        index.insert(s as u32, 0);
        seen[s] = true;
        let mut links = vec![];
        let mut extra = vec![];
        let mut used: HashSet<u32> = HashSet::new();
        let mut k = 0;
        while k < verts.len() {
            let a = verts[k];
            for &(b, e) in &adj[a as usize] {
                if !seen[b as usize] {
                    seen[b as usize] = true;
                    index.insert(b, verts.len() as u32);
                    verts.push(b);
                    links.push((k as u32, verts.len() as u32 - 1, e));
                    used.insert(e);
                } else if used.insert(e) {
                    extra.push((k as u32, index[&b], e));
                }
            }
            k += 1;
        }
        if verts.len() < 3 {
            continue;
        }
        let t = ft.tan[s];
        let c0 = if dot(t, q[s]).abs() >= dot(t, cross(n[s], q[s])).abs() { 1 } else { 0 };
        out.push(Curve { verts, links, c0, fixed: None, extra, ok: true, pin: true });
    }
    out
}

/// Pour chaque sommet de la ligne : (composante de u, signe, décalage entier) tels que la
/// coordonnée perpendiculaire dans la carte de la racine vaut signe·u[composante] + décalage.
fn curve_frames(g: &Graph, c: &Curve) -> Vec<(u8, f64, i32)> {
    let mut rot = vec![0u8; c.verts.len()];
    let mut tr = vec![[0i32; 2]; c.verts.len()];
    for &(a, b, e) in &c.links {
        let (k, d) = g.dir(e, c.verts[a as usize]);
        let r = trot(rot[a as usize], d);
        rot[b as usize] = (rot[a as usize] + k) & 3;
        tr[b as usize] = [tr[a as usize][0] + r[0], tr[a as usize][1] + r[1]];
    }
    (0..c.verts.len())
        .map(|i| {
            let (ex, ey) = (trot(rot[i], [1, 0]), trot(rot[i], [0, 1]));
            let (cx, cy) = (ex[c.c0 as usize], ey[c.c0 as usize]);
            if cx != 0 { (0, cx as f64, tr[i][c.c0 as usize]) } else { (1, cy as f64, tr[i][c.c0 as usize]) }
        })
        .collect()
}

/// Décalage (en lignes de grille) d'une boucle fermée après un tour ; None si même le repère
/// ne revient pas à l'identique.
fn curve_mismatch(g: &Graph, c: &Curve) -> Option<i32> {
    let mut rot = vec![0u8; c.verts.len()];
    let mut tr = vec![[0i32; 2]; c.verts.len()];
    for &(a, b, e) in &c.links {
        let (k, d) = g.dir(e, c.verts[a as usize]);
        let r = trot(rot[a as usize], d);
        rot[b as usize] = (rot[a as usize] + k) & 3;
        tr[b as usize] = [tr[a as usize][0] + r[0], tr[a as usize][1] + r[1]];
    }
    let mut m = 0;
    for &(a, b, e) in &c.extra {
        let (k, d) = g.dir(e, c.verts[a as usize]);
        if (rot[a as usize] + k) & 3 != rot[b as usize] {
            return None;
        }
        let r = trot(rot[a as usize], d);
        m += tr[a as usize][c.c0 as usize] + r[c.c0 as usize] - tr[b as usize][c.c0 as usize];
    }
    Some(m)
}

/// Une boucle fermée doit revenir sur la même ligne de grille après un tour.
#[allow(dead_code)]
fn curve_closes(g: &Graph, c: &Curve) -> bool {
    let mut rot = vec![0u8; c.verts.len()];
    let mut tr = vec![[0i32; 2]; c.verts.len()];
    for &(a, b, e) in &c.links {
        let (k, d) = g.dir(e, c.verts[a as usize]);
        let r = trot(rot[a as usize], d);
        rot[b as usize] = (rot[a as usize] + k) & 3;
        tr[b as usize] = [tr[a as usize][0] + r[0], tr[a as usize][1] + r[1]];
    }
    c.extra.iter().all(|&(a, b, e)| {
        let (k, d) = g.dir(e, c.verts[a as usize]);
        let r = trot(rot[a as usize], d);
        (rot[a as usize] + k) & 3 == rot[b as usize] && tr[a as usize][c.c0 as usize] + r[c.c0 as usize] == tr[b as usize][c.c0 as usize]
    })
}

fn curve_offset(g: &Graph, c: &Curve, u: &[[f64; 2]]) -> f64 {
    let fr = curve_frames(g, c);
    fr.iter().zip(&c.verts).map(|(&(k, s, t), &v)| s * u[v as usize][k as usize] + t as f64).sum::<f64>() / fr.len() as f64
}

/// Contraintes des lignes déjà arrondies : (sommet, composante, valeur).
fn curve_pins(g: &Graph, curves: &[Curve]) -> Vec<(u32, u8, f64)> {
    let mut pins = vec![];
    for c in curves.iter().filter(|c| c.pin) {
        if let Some(tau) = c.fixed {
            for (&(k, s, t), &v) in curve_frames(g, c).iter().zip(&c.verts) {
                pins.push((v, k, s * (tau - t) as f64));
            }
        }
    }
    pins
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

/// Lissage léger puis reprojection sur la surface d'origine (sommet dense le plus proche).
fn relax(r: &mut Result, v: &[V3], f: &[[u32; 3]], iters: usize, flags: &[u8], segs: &[(V3, V3)], snap: f32) {
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
            match flags.get(i).copied().unwrap_or(0) {
                2 => continue,
                1 => {
                    // Reste sur sa ligne : point le plus proche parmi les arêtes caractéristiques
                    let mut best = (f32::MAX, old[i]);
                    for &(a, b) in segs {
                        let ab = sub(b, a);
                        let t = (dot(sub(old[i], a), ab) / dot(ab, ab).max(1e-20)).clamp(0.0, 1.0);
                        let p = add(a, mul(ab, t));
                        let d = dot(sub(p, old[i]), sub(p, old[i]));
                        if d < best.0 {
                            best = (d, p);
                        }
                    }
                    // Trop loin de toute ligne : ce sommet n'en fait pas vraiment partie
                    if best.0.sqrt() <= snap {
                        r.pos[i] = best.1;
                        continue;
                    }
                }
                _ => {}
            }
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
        "--sym" => {
            opt.symmetry = val.and_then(|v| v.parse().ok()).unwrap_or(0);
            *i += 1;
        }
        "--sharp" => {
            opt.sharp_angle = val.and_then(|v| v.parse().ok()).unwrap_or(opt.sharp_angle);
            *i += 1;
        }
        "--adapt" => {
            opt.adaptive = val.and_then(|v| v.parse().ok()).unwrap_or(opt.adaptive);
            *i += 1;
        }
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

    /// Banc d'essai sur un vrai fichier : RETOPO_IN=a.obj RETOPO_OUT=b.obj RETOPO_FACES=6000
    #[test]
    #[ignore]
    fn banc_stars() {
        let src = std::fs::read_to_string(std::env::var("RETOPO_IN").unwrap()).unwrap();
        let mut r = Result { pos: vec![], faces: vec![], stats: Stats::default() };
        for l in src.lines() {
            let w: Vec<&str> = l.split_whitespace().collect();
            match w.first() {
                Some(&"v") => r.pos.push([w[1].parse().unwrap(), w[2].parse().unwrap(), w[3].parse().unwrap()]),
                Some(&"f") => r.faces.push(w[1..].iter().map(|t| t.split('/').next().unwrap().parse::<u32>().unwrap() - 1).collect()),
                _ => {}
            }
        }
        fix_stars(&mut r);
        export_obj(&r, &|p| p, std::path::Path::new(&std::env::var("RETOPO_OUT").unwrap())).unwrap();
    }

    #[test]
    #[ignore]
    fn banc_simplif() {
        let Ok(inp) = std::env::var("RETOPO_IN") else { return };
        let (pos, tris) = lire_obj(&inp);
        let t = std::time::Instant::now();
        let n = std::env::var("RETOPO_FACES").ok().and_then(|x| x.parse().ok()).unwrap_or(150_000);
        let (p, f, _) = simplify_to(&pos, &tris, n);
        eprintln!("{} -> {} triangles en {:.1} s", tris.len(), f.len(), t.elapsed().as_secs_f32());
        let r = Result { pos: p, faces: f.iter().map(|t| t.to_vec()).collect(), stats: Stats::default() };
        export_obj(&r, &|p| p, std::path::Path::new(&std::env::var("RETOPO_OUT").unwrap_or("/tmp/rt/simpl.obj".into()))).unwrap();
    }

    fn lire_obj(inp: &str) -> (Vec<V3>, Vec<[u32; 3]>) {
        let (mut pos, mut tris) = (vec![], vec![]);
        for l in std::fs::read_to_string(inp).unwrap().lines() {
            let mut it = l.split_whitespace();
            match it.next() {
                Some("v") => {
                    let c: Vec<f32> = it.take(3).map(|x| x.parse().unwrap()).collect();
                    pos.push([c[0], c[1], c[2]]);
                }
                Some("f") => {
                    let ix: Vec<u32> = it.map(|t| t.split('/').next().unwrap().parse::<u32>().unwrap() - 1).collect();
                    for k in 1..ix.len() - 1 {
                        tris.push([ix[0], ix[k], ix[k + 1]]);
                    }
                }
                _ => {}
            }
        }
        (pos, tris)
    }

    #[test]
    #[ignore]
    fn banc_obj() {
        let Ok(inp) = std::env::var("RETOPO_IN") else { return };
        let (pos, tris) = if inp.ends_with(".obj") {
            lire_obj(&inp)
        } else {
            let m = crate::model3d::load(std::path::Path::new(&inp)).unwrap();
            let pos = m.normalized(m.z_up);
            if let Ok(out) = std::env::var("RETOPO_DUMP") {
                let r = Result { pos: pos.clone(), faces: m.tris.iter().map(|t| t.to_vec()).collect(), stats: Stats::default() };
                export_obj(&r, &|p| p, std::path::Path::new(&out)).unwrap();
            }
            (pos, m.tris.clone())
        };
        let mut opt = Options::default();
        opt.faces = std::env::var("RETOPO_FACES").ok().and_then(|x| x.parse().ok()).unwrap_or(6000);
        opt.symmetry = std::env::var("RETOPO_SYM").ok().and_then(|x| x.parse().ok()).unwrap_or(0);
        if let Some(a) = std::env::var("RETOPO_ADAPT").ok().and_then(|x| x.parse().ok()) {
            opt.adaptive = a;
        }
        if let Some(a) = std::env::var("RETOPO_DETAIL").ok().and_then(|x| x.parse().ok()) {
            opt.detail_density = a;
        }
        let t = std::time::Instant::now();
        // Guides : fichier texte, « x y z » par ligne, une ligne vide entre deux courbes
        let mut guides: Vec<Vec<V3>> = vec![vec![]];
        if let Ok(gp) = std::env::var("RETOPO_GUIDES") {
            for l in std::fs::read_to_string(gp).unwrap().lines() {
                let c: Vec<f32> = l.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                if c.len() == 3 {
                    guides.last_mut().unwrap().push([c[0], c[1], c[2]]);
                } else if !guides.last().unwrap().is_empty() {
                    guides.push(vec![]);
                }
            }
        }
        guides.retain(|g| g.len() > 1);
        let r = remesh(&pos, &tris, &guides, &opt, &|_| {});
        eprintln!("{} faces ({} quads) en {:.1} s, {:?}", r.faces.len(), r.quads(), t.elapsed().as_secs_f32(), r.stats);
        if let Ok(out) = std::env::var("RETOPO_OUT") {
            export_obj(&r, &|p| p, std::path::Path::new(&out)).unwrap();
        }
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
        // Une sphère demande au minimum 8 pôles (topologie d'un cube)
        assert_eq!(pq.stats.poles, 8);
    }

    #[test]
    fn densite_peinte() {
        let (p, t) = sphere(3);
        // Plus de quads autour du pôle nord
        let opt = Options { faces: 600, density: vec![[0.0, 1.0, 0.0, 0.8, 2.0]], ..Default::default() };
        let r = remesh(&p, &t, &[], &opt, &|_| {});
        let near = r.faces.iter().filter(|f| f.iter().all(|&v| r.pos[v as usize][1] > 0.75)).count();
        let far = r.faces.iter().filter(|f| f.iter().all(|&v| r.pos[v as usize][1] < -0.75)).count();
        eprintln!("près {near}, loin {far}, total {}", r.faces.len());
        assert!(near as f32 > far as f32 * 1.6);
    }

    fn cube(n: usize) -> (Vec<V3>, Vec<[u32; 3]>) {
        let pos: Vec<V3> = (0..8).map(|i| [if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 }, if i & 4 == 0 { -1.0 } else { 1.0 }]).collect();
        let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let tris: Vec<[u32; 3]> = quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect();
        let (mut pos, mut tris) = (pos, tris);
        for _ in 0..n {
            let (p, t) = refine_once(pos, tris);
            pos = p;
            tris = t;
        }
        (pos, tris)
    }

    #[test]
    fn cube_aretes_vives() {
        let (p, t) = cube(4);
        let r = remesh(&p, &t, &[], &Options { faces: 600, ..Default::default() }, &|_| {});
        eprintln!("{} faces, {:?}", r.faces.len(), r.stats);
        assert_eq!(r.quads(), r.faces.len());
        // Huit coins de valence 3, rien d'autre, et les 8 coins du cube sont des sommets du résultat
        assert_eq!(r.stats.poles, 8);
        for c in 0..8 {
            let corner = [if c & 1 == 0 { -1.0 } else { 1.0 }, if c & 2 == 0 { -1.0 } else { 1.0 }, if c & 4 == 0 { -1.0f32 } else { 1.0 }];
            assert!(r.pos.iter().any(|p| len(sub(*p, corner)) < 1e-3), "coin {corner:?} perdu");
        }
        // Chaque sommet est sur la surface du cube
        assert!(r.pos.iter().all(|p| (p[0].abs().max(p[1].abs()).max(p[2].abs()) - 1.0).abs() < 0.02));
    }

    #[test]
    fn symetrie() {
        let (p, t) = sphere(3);
        if let Ok(path) = std::env::var("DUMPTEST") {
            use std::io::Write;
            let mut w = std::fs::File::create(path).unwrap();
            for x in &p {
                writeln!(w, "v {} {} {}", x[0], x[1], x[2]).unwrap();
            }
            for x in &t {
                writeln!(w, "f {} {} {}", x[0] + 1, x[1] + 1, x[2] + 1).unwrap();
            }
        }
        let r = remesh(&p, &t, &[], &Options { faces: 600, symmetry: 1, ..Default::default() }, &|_| {});
        assert_eq!(r.quads(), r.faces.len());
        // Chaque sommet a son image dans le miroir
        for a in &r.pos {
            let m = [-a[0], a[1], a[2]];
            assert!(r.pos.iter().any(|b| len(sub(*b, m)) < 1e-4), "pas de symétrique pour {a:?}");
        }
        // Maillage fermé : chaque arête a deux faces
        let mut ec: HashMap<(u32, u32), u8> = HashMap::new();
        for f in &r.faces {
            for k in 0..f.len() {
                let (a, b) = (f[k], f[(k + 1) % f.len()]);
                *ec.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        for (&(a, b), &c) in &ec {
            if c != 2 {
                eprintln!("arête {a}-{b} : {c} face(s) ; {:?} {:?}", r.pos[a as usize], r.pos[b as usize]);
            }
        }
        assert!(ec.values().all(|&c| c == 2));
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
