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
}

impl Default for Options {
    fn default() -> Self {
        Options { faces: 1500, iterations: 160, guide_radius: 1.2, pure_quads: true }
    }
}

pub struct Result {
    pub pos: Vec<V3>,
    pub faces: Vec<Vec<u32>>,
}

impl Result {
    pub fn quads(&self) -> usize {
        self.faces.iter().filter(|f| f.len() == 4).count()
    }
}

/// Densifie (1 triangle → 4) jusqu'à ce que l'arête moyenne soit sous `target`.
fn refine(mut pos: Vec<V3>, mut tris: Vec<[u32; 3]>, target: f32) -> (Vec<V3>, Vec<[u32; 3]>) {
    for _ in 0..6 {
        let mut total = 0.0;
        for t in &tris {
            for k in 0..3 {
                total += len(sub(pos[t[k] as usize], pos[t[(k + 1) % 3] as usize]));
            }
        }
        let mean = total / (tris.len() * 3).max(1) as f32;
        if mean <= target || tris.len() * 4 > 600_000 {
            break;
        }
        (pos, tris) = split(pos, &tris);
    }
    (pos, tris)
}

fn split(mut pos: Vec<V3>, tris: &[[u32; 3]]) -> (Vec<V3>, Vec<[u32; 3]>) {
    {
        let mut mid: HashMap<(u32, u32), u32> = HashMap::new();
        let mut out = Vec::with_capacity(tris.len() * 4);
        let mut m = |a: u32, b: u32, pos: &mut Vec<V3>| -> u32 {
            let k = (a.min(b), a.max(b));
            *mid.entry(k).or_insert_with(|| {
                pos.push(mul(add(pos[a as usize], pos[b as usize]), 0.5));
                (pos.len() - 1) as u32
            })
        };
        for t in tris {
            let ab = m(t[0], t[1], &mut pos);
            let bc = m(t[1], t[2], &mut pos);
            let ca = m(t[2], t[0], &mut pos);
            out.extend([[t[0], ab, ca], [t[1], bc, ab], [t[2], ca, bc], [ab, bc, ca]]);
        }
        (pos, out)
    }
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

/// Retopologie. `guides` : traits (suites de points sur la surface).
pub fn remesh(pos: &[V3], tris: &[[u32; 3]], guides: &[Vec<V3>], opt: &Options, progress: &dyn Fn(f32)) -> Result {
    let area: f32 = tris.iter().map(|t| len(cross(sub(pos[t[1] as usize], pos[t[0] as usize]), sub(pos[t[2] as usize], pos[t[0] as usize]))) * 0.5).sum();
    let target = if opt.pure_quads { (opt.faces / 4).max(4) } else { opt.faces.max(4) };
    let h = (area / target as f32).sqrt().max(1e-6);
    let (v, f) = refine(pos.to_vec(), tris.to_vec(), h * 0.5);
    let n_v = v.len();

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
    let mut q: Vec<V3> = (0..n_v)
        .map(|i| {
            let n = nrm[i];
            let ax = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
            norm(tangent(ax, n))
        })
        .collect();
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
    let mut levels = vec![Level { v: v.clone(), n: nrm.clone(), adj: adj.clone(), parent: vec![] }];
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
    let iters = opt.iterations.max(10);
    let total_steps = levels.len() as f32 * 2.0;
    let mut step = 0.0;

    // Orientation : du plus grossier au plus fin
    let mut fixed_l = vec![fixed.clone()];
    let mut q_l = vec![q.clone()];
    for l in 1..levels.len() {
        let mut fx = vec![false; levels[l].v.len()];
        let mut qq: Vec<V3> = levels[l].n.iter().map(|n| norm(tangent(if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] }, *n))).collect();
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
        smooth_orient(&levels[l], &mut cur, &fixed_l[l], it);
        step += 1.0;
        progress(step / total_steps);
    }
    let q = cur;

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

    // Extraction : fusion des sommets sur le même point de grille, arêtes à un pas
    let mut uf = Uf((0..n_v as u32).collect());
    let mut unit_edges = vec![];
    for i in 0..n_v {
        for &j in &adj[i] {
            let j = j as usize;
            if j < i {
                continue;
            }
            // Point de grille de j vu dans le repère de i
            let ti = cross(nrm[i], q[i]);
            let dd = sub(o[j], o[i]);
            let (cx, cy) = ((dot(q[i], dd) / h).round() as i32, (dot(ti, dd) / h).round() as i32);
            match cx.abs() + cy.abs() {
                0 => uf.union(i as u32, j as u32),
                1 => unit_edges.push((i as u32, j as u32)),
                _ => {}
            }
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
    for (k, (s, n, c)) in &cluster {
        id.insert(*k, out_pos.len() as u32);
        out_pos.push(mul(*s, 1.0 / c));
        out_n.push(norm(*n));
    }
    let mut edges: HashSet<(u32, u32)> = HashSet::new();
    for (a, b) in unit_edges {
        let (a, b) = (id[&uf.find(a)], id[&uf.find(b)]);
        if a != b {
            edges.insert((a.min(b), a.max(b)));
        }
    }
    // Deux groupes trop proches représentent le même point de grille : fusion
    let mut uf2 = Uf((0..out_pos.len() as u32).collect());
    for &(a, b) in &edges {
        if len(sub(out_pos[a as usize], out_pos[b as usize])) < 0.4 * h {
            uf2.union(a, b);
        }
    }
    let mut acc: HashMap<u32, (V3, V3, f32)> = HashMap::new();
    for i in 0..out_pos.len() as u32 {
        let e = acc.entry(uf2.find(i)).or_insert(([0.0; 3], [0.0; 3], 0.0));
        e.0 = add(e.0, out_pos[i as usize]);
        e.1 = add(e.1, out_n[i as usize]);
        e.2 += 1.0;
    }
    for (k, (p, n, c)) in &acc {
        out_pos[*k as usize] = mul(*p, 1.0 / c);
        out_n[*k as usize] = norm(*n);
    }
    let edges: HashSet<(u32, u32)> = edges
        .into_iter()
        .filter_map(|(a, b)| {
            let (a, b) = (uf2.find(a), uf2.find(b));
            let l = len(sub(out_pos[a as usize], out_pos[b as usize]));
            (a != b && l < 1.8 * h).then(|| (a.min(b), a.max(b)))
        })
        .collect();

    let faces = quadify(&out_pos, extract_faces(&out_pos, &out_n, &edges));
    #[cfg(test)]
    eprintln!("dense {} sommets, h {h}, {} groupes, {} arêtes, {} faces", n_v, out_pos.len(), edges.len(), faces.len());
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
    let mut res = Result { pos: pos2, faces };
    relax(&mut res, &v, &f, 3);
    if opt.pure_quads {
        res = split_quads(&res);
        relax(&mut res, &v, &f, 2);
    }
    progress(1.0);
    res
}

struct Level {
    v: Vec<V3>,
    n: Vec<V3>,
    adj: Vec<Vec<u32>>,
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
    fine.parent = parent;
    Level { v, n, adj: adj.into_iter().map(|s| s.into_iter().collect()).collect(), parent: vec![] }
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

fn smooth_orient(l: &Level, q: &mut [V3], fixed: &[bool], iters: usize) {
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
            q[i] = qi;
        }
    }
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

/// Faces lues en tournant autour de chaque sommet (ordre angulaire dans le plan tangent).
fn extract_faces(pos: &[V3], nrm: &[V3], edges: &HashSet<(u32, u32)>) -> Vec<Vec<u32>> {
    let n = pos.len();
    let mut nb: Vec<Vec<u32>> = vec![vec![]; n];
    for &(a, b) in edges {
        nb[a as usize].push(b);
        nb[b as usize].push(a);
    }
    // Tri des voisins dans le sens trigonométrique autour de la normale
    for i in 0..n {
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
            for _ in 0..6 {
                if v == a {
                    ok = true;
                    break;
                }
                f.push(v);
                let Some(w) = next(u, v) else { break };
                u = v;
                v = w;
            }
            if ok && (3..=6).contains(&f.len()) {
                for k in 0..f.len() {
                    used.insert((f[k], f[(k + 1) % f.len()]));
                }
                // Vérifier l'orientation : normale de la face dans le sens des normales des sommets
                let c = f.iter().fold([0.0; 3], |acc, &i| add(acc, pos[i as usize]));
                let c = mul(c, 1.0 / f.len() as f32);
                let mut fn_ = [0.0; 3];
                for k in 0..f.len() {
                    fn_ = add(fn_, cross(sub(pos[f[k] as usize], c), sub(pos[f[(k + 1) % f.len()] as usize], c)));
                }
                let avg_n = f.iter().fold([0.0; 3], |acc, &i| add(acc, nrm[i as usize]));
                if dot(fn_, avg_n) < 0.0 {
                    f.reverse();
                }
                // Les pentagones et hexagones sont coupés en quads + triangles
                match f.len() {
                    5 => {
                        faces.push(vec![f[0], f[1], f[2], f[3]]);
                        faces.push(vec![f[0], f[3], f[4]]);
                    }
                    6 => {
                        faces.push(vec![f[0], f[1], f[2], f[3]]);
                        faces.push(vec![f[0], f[3], f[4], f[5]]);
                    }
                    _ => faces.push(f),
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
    Result { pos, faces }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sphere(n: usize) -> (Vec<V3>, Vec<[u32; 3]>) {
        // Icosphère
        let t = (1.0 + 5f32.sqrt()) / 2.0;
        let mut pos: Vec<V3> = [[-1.0, t, 0.0], [1.0, t, 0.0], [-1.0, -t, 0.0], [1.0, -t, 0.0], [0.0, -1.0, t], [0.0, 1.0, t], [0.0, -1.0, -t], [0.0, 1.0, -t], [t, 0.0, -1.0], [t, 0.0, 1.0], [-t, 0.0, -1.0], [-t, 0.0, 1.0]].iter().map(|p| norm(*p)).collect();
        let mut tris: Vec<[u32; 3]> = vec![[0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8], [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1]];
        for _ in 0..n {
            let (p, t) = split(pos, &tris);
            pos = p.into_iter().map(norm).collect();
            tris = t;
        }
        (pos, tris)
    }

    #[test]
    fn sphere_en_quads() {
        let (p, t) = sphere(3);
        let r = remesh(&p, &t, &[], &Options { faces: 600, iterations: 80, guide_radius: 1.2, pure_quads: false }, &|_| {});
        let quads = r.quads();
        eprintln!("{} faces, {} quads, {} sommets", r.faces.len(), quads, r.pos.len());
        let mut hist = std::collections::BTreeMap::new();
        for f in &r.faces { *hist.entry(f.len()).or_insert(0) += 1; }
        let mut val = std::collections::BTreeMap::new();
        let mut deg = vec![0; r.pos.len()];
        for f in &r.faces { for &i in f { deg[i as usize] += 1; } }
        for d in deg { *val.entry(d).or_insert(0) += 1; }
        eprintln!("tailles {hist:?} valences {val:?}");
        export_obj(&r, &|p| p, &std::env::temp_dir().join("retopo_sphere.obj")).unwrap();
        assert!(r.faces.len() > 300 && r.faces.len() < 1000, "{} faces", r.faces.len());
        assert!(quads as f32 / r.faces.len() as f32 > 0.7, "{quads} quads sur {}", r.faces.len());
        let pq = remesh(&p, &t, &[], &Options { faces: 600, iterations: 80, guide_radius: 1.2, pure_quads: true }, &|_| {});
        eprintln!("100 % quads : {} faces", pq.faces.len());
        assert_eq!(pq.quads(), pq.faces.len());
        assert!(pq.faces.len() > 300 && pq.faces.len() < 1000);
        export_obj(&pq, &|p| p, &std::env::temp_dir().join("retopo_pq.obj")).unwrap();
        // Tous les sommets restent sur la sphère
        assert!(r.pos.iter().all(|p| (len(*p) - 1.0).abs() < 0.08));
    }

    #[test]
    fn guide_oriente_les_boucles() {
        let (p, t) = sphere(3);
        // Guide le long de l'équateur : les arêtes près de l'équateur doivent suivre le trait
        let g: Vec<V3> = (0..=48).map(|k| {
            let a = k as f32 / 48.0 * std::f32::consts::TAU;
            [a.cos(), 0.0, a.sin()]
        }).collect();
        let r = remesh(&p, &t, &[g], &Options { faces: 600, iterations: 80, guide_radius: 1.5, pure_quads: true }, &|_| {});
        let mut aligned = 0;
        let mut total = 0;
        for f in &r.faces {
            for k in 0..f.len() {
                let (a, b) = (r.pos[f[k] as usize], r.pos[f[(k + 1) % f.len()] as usize]);
                if a[1].abs() < 0.08 && b[1].abs() < 0.08 {
                    total += 1;
                    let d = norm(sub(b, a));
                    if d[1].abs() < 0.3 {
                        aligned += 1;
                    }
                }
            }
        }
        eprintln!("{aligned}/{total} arêtes alignées sur l'équateur");
        assert!(total > 20 && aligned as f32 / total as f32 > 0.8);
    }
}
