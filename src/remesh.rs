//! Remaillage par voxels (comme le « Voxel Remesh » de Blender) : le volume du maillage est rempli dans
//! une grille, adouci, puis sa surface est refaite en quads réguliers (« surface nets »). Utile avant de
//! sculpter : une topologie égale partout, sans trous ni faces qui se croisent.

pub type V3 = [f32; 3];

/// Le maillage refait : positions, triangles (deux par quad) et arêtes visibles de chaque triangle
pub struct Out {
    pub pos: Vec<V3>,
    pub tris: Vec<[u32; 3]>,
    pub edges: Vec<u8>,
}

/// Remaille `pos`/`tris` avec des voxels de taille `cell` ; `smooth` passes de lissage à la fin.
/// La grille est bornée (environ 2 millions de cases) : une taille trop fine est agrandie.
pub fn remesh(pos: &[V3], tris: &[[u32; 3]], cell: f32, smooth: u32) -> Option<Out> {
    if tris.is_empty() {
        return None;
    }
    let (lo, hi) = bounds(pos);
    let g = Grid::new(lo, hi, cell);
    let field = mesh_field(pos, tris, &g)?;
    extract(&field, &g, smooth)
}

/// Le cadre d'un nuage de points
fn bounds(pos: &[V3]) -> (V3, V3) {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

/// Une grille de voxels qui couvre un cadre, avec deux cases de marge
struct Grid {
    n: [usize; 3],
    origin: V3,
    cell: f32,
}

impl Grid {
    fn new(lo: V3, hi: V3, cell: f32) -> Grid {
        let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
        let mut cell = cell.max(1e-5);
        let cells = |c: f32| (0..3).map(|k| (size[k] / c).ceil() as usize + 4).product::<usize>();
        while cells(cell) > 2_000_000 {
            cell *= 1.25;
        }
        let n: [usize; 3] = [0, 1, 2].map(|k| (size[k] / cell).ceil() as usize + 4);
        Grid { n, origin: [0, 1, 2].map(|k| lo[k] - 2.0 * cell), cell }
    }
    fn point(&self, i: usize, j: usize, k: usize) -> V3 {
        [self.origin[0] + i as f32 * self.cell, self.origin[1] + j as f32 * self.cell, self.origin[2] + k as f32 * self.cell]
    }
}

/// Forme simple pour les booléens : 0 cube, 1 sphère, 2 cylindre (axe Y) ; centre et demi-tailles
pub struct Shape {
    pub kind: u8,
    pub center: V3,
    pub half: V3,
}

impl Shape {
    /// Distance signée, positive dedans (comme le champ du maillage)
    fn sdf(&self, p: V3) -> f32 {
        let d = [p[0] - self.center[0], p[1] - self.center[1], p[2] - self.center[2]];
        let h = self.half.map(|x| x.abs().max(1e-5));
        match self.kind {
            1 => {
                // Ellipsoïde : approximation par mise à l'échelle
                let q = [d[0] / h[0], d[1] / h[1], d[2] / h[2]];
                let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
                (1.0 - l) * h[0].min(h[1]).min(h[2])
            }
            2 => {
                let r = ((d[0] / h[0]).powi(2) + (d[2] / h[2]).powi(2)).sqrt();
                let radial = (1.0 - r) * h[0].min(h[2]);
                let axial = h[1] - d[1].abs();
                radial.min(axial)
            }
            _ => {
                let q = [h[0] - d[0].abs(), h[1] - d[1].abs(), h[2] - d[2].abs()];
                if q.iter().all(|x| *x >= 0.0) {
                    q[0].min(q[1]).min(q[2])
                } else {
                    -(q.iter().map(|x| x.min(0.0).powi(2)).sum::<f32>()).sqrt()
                }
            }
        }
    }
}

/// Booléen par voxels entre le maillage et une forme : 0 union, 1 différence, 2 intersection
pub fn boolean(pos: &[V3], tris: &[[u32; 3]], shape: &Shape, op: u8, cell: f32, smooth: u32) -> Option<Out> {
    if tris.is_empty() {
        return None;
    }
    let (mut lo, mut hi) = bounds(pos);
    if op == 0 {
        for k in 0..3 {
            lo[k] = lo[k].min(shape.center[k] - shape.half[k].abs());
            hi[k] = hi[k].max(shape.center[k] + shape.half[k].abs());
        }
    }
    let g = Grid::new(lo, hi, cell);
    let mut field = mesh_field(pos, tris, &g)?;
    let n = g.n;
    for k in 0..n[2] {
        for j in 0..n[1] {
            for i in 0..n[0] {
                let idx = (k * n[1] + j) * n[0] + i;
                let s = shape.sdf(g.point(i, j, k));
                let f = field[idx];
                field[idx] = match op {
                    0 => f.max(s),
                    2 => f.min(s),
                    _ => f.min(-s),
                };
            }
        }
    }
    // Garde le champ hors de zéro (la surface reste fermée)
    for f in field.iter_mut() {
        if f.abs() < g.cell * 1e-3 {
            *f = -g.cell * 1e-3;
        }
    }
    extract(&field, &g, smooth)
}

/// Le champ signé du maillage dans la grille (positif dedans) ; None s'il n'enferme rien
fn mesh_field(pos: &[V3], tris: &[[u32; 3]], g: &Grid) -> Option<Vec<f32>> {
    let (n, origin, cell) = (g.n, g.origin, g.cell);
    let at = |i: usize, j: usize, k: usize| (k * n[1] + j) * n[0] + i;
    // 1. Dedans ou dehors : pour chaque colonne verticale (x, z), les traversées de la surface le long de y
    let mut cols: Vec<Vec<f32>> = vec![vec![]; n[0] * n[2]];
    for t in tris {
        let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        let (x0, x1) = (a[0].min(b[0]).min(c[0]), a[0].max(b[0]).max(c[0]));
        let (z0, z1) = (a[2].min(b[2]).min(c[2]), a[2].max(b[2]).max(c[2]));
        let gi0 = ((x0 - origin[0]) / cell).ceil().max(0.0) as usize;
        let gi1 = (((x1 - origin[0]) / cell).floor() as isize).min(n[0] as isize - 1);
        let gk0 = ((z0 - origin[2]) / cell).ceil().max(0.0) as usize;
        let gk1 = (((z1 - origin[2]) / cell).floor() as isize).min(n[2] as isize - 1);
        if gi1 < 0 || gk1 < 0 {
            continue;
        }
        // Barycentriques dans le plan xz (un léger décalage évite de tomber pile sur une arête)
        let det = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
        if det.abs() < 1e-12 {
            continue;
        }
        for gk in gk0..=gk1 as usize {
            for gi in gi0..=gi1 as usize {
                let x = origin[0] + gi as f32 * cell + cell * 1e-3;
                let z = origin[2] + gk as f32 * cell + cell * 1.7e-3;
                let l1 = ((b[2] - c[2]) * (x - c[0]) + (c[0] - b[0]) * (z - c[2])) / det;
                let l2 = ((c[2] - a[2]) * (x - c[0]) + (a[0] - c[0]) * (z - c[2])) / det;
                let l3 = 1.0 - l1 - l2;
                if l1 >= 0.0 && l2 >= 0.0 && l3 >= 0.0 {
                    cols[gk * n[0] + gi].push(l1 * a[1] + l2 * b[1] + l3 * c[1]);
                }
            }
        }
    }
    let mut field = vec![0.0f32; n[0] * n[1] * n[2]];
    for gk in 0..n[2] {
        for gi in 0..n[0] {
            let col = &mut cols[gk * n[0] + gi];
            if col.len() < 2 {
                continue;
            }
            col.sort_by(f32::total_cmp);
            // Règle pair-impair : entre la 1re et la 2e traversée on est dedans, etc.
            for pair in col.chunks_exact(2) {
                let j0 = ((pair[0] - origin[1]) / cell).ceil().max(0.0) as usize;
                let j1 = (((pair[1] - origin[1]) / cell).floor() as isize).min(n[1] as isize - 1);
                if j1 < 0 {
                    continue;
                }
                for gj in j0..=j1 as usize {
                    field[at(gi, gj, gk)] = 1.0;
                }
            }
        }
    }
    if !field.iter().any(|v| *v > 0.0) {
        return None;
    }
    // 2. Distance signée près de la surface : la vraie distance aux triangles (dedans positif), bornée à
    // deux cases ; plus loin, seul le côté compte. La surface retrouvée suit alors l'original de près.
    let band = 2.0 * cell;
    let mut dist = vec![band; field.len()];
    for t in tris {
        let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        let lo_i: [usize; 3] = [0, 1, 2].map(|k| (((a[k].min(b[k]).min(c[k]) - band - origin[k]) / cell).floor().max(0.0)) as usize);
        let hi_i: [usize; 3] = [0, 1, 2].map(|k| ((((a[k].max(b[k]).max(c[k]) + band - origin[k]) / cell).ceil()) as usize).min(n[k] - 1));
        for k in lo_i[2]..=hi_i[2] {
            for j in lo_i[1]..=hi_i[1] {
                for i in lo_i[0]..=hi_i[0] {
                    let p = [origin[0] + i as f32 * cell, origin[1] + j as f32 * cell, origin[2] + k as f32 * cell];
                    let d = point_tri(p, a, b, c);
                    let slot = &mut dist[at(i, j, k)];
                    if d < *slot {
                        *slot = d;
                    }
                }
            }
        }
    }
    for (f, d) in field.iter_mut().zip(dist.iter()) {
        // Jamais pile zéro : un point sur la surface est rangé d'un côté, la surface reste fermée
        let d = d.max(cell * 1e-3);
        *f = if *f > 0.5 { d } else { -d };
    }
    Some(field)
}

/// Surface nets sur le champ, puis lissage léger
fn extract(field: &[f32], g: &Grid, smooth: u32) -> Option<Out> {
    let (n, origin, cell) = (g.n, g.origin, g.cell);
    let at = |i: usize, j: usize, k: usize| (k * n[1] + j) * n[0] + i;
    // 3. Surface nets : un sommet par case traversée par l'iso 0,5, à la moyenne des points de passage
    let iso = 0.0f32;
    let mut vid = vec![u32::MAX; n[0] * n[1] * n[2]];
    let mut out_pos: Vec<V3> = vec![];
    const CORNERS: [[usize; 3]; 8] = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [1, 1, 0], [0, 0, 1], [1, 0, 1], [0, 1, 1], [1, 1, 1]];
    const EDGES: [[usize; 2]; 12] = [[0, 1], [2, 3], [4, 5], [6, 7], [0, 2], [1, 3], [4, 6], [5, 7], [0, 4], [1, 5], [2, 6], [3, 7]];
    for k in 0..n[2] - 1 {
        for j in 0..n[1] - 1 {
            for i in 0..n[0] - 1 {
                let v: [f32; 8] = CORNERS.map(|c| field[at(i + c[0], j + c[1], k + c[2])]);
                let inside = v.iter().filter(|x| **x > iso).count();
                if inside == 0 || inside == 8 {
                    continue;
                }
                let (mut acc, mut cnt) = ([0.0f32; 3], 0.0);
                for e in EDGES {
                    let (a, b) = (v[e[0]], v[e[1]]);
                    if (a > iso) != (b > iso) {
                        let t = (iso - a) / (b - a);
                        for d in 0..3 {
                            acc[d] += CORNERS[e[0]][d] as f32 + (CORNERS[e[1]][d] as f32 - CORNERS[e[0]][d] as f32) * t;
                        }
                        cnt += 1.0;
                    }
                }
                vid[at(i, j, k)] = out_pos.len() as u32;
                out_pos.push([origin[0] + (i as f32 + acc[0] / cnt) * cell, origin[1] + (j as f32 + acc[1] / cnt) * cell, origin[2] + (k as f32 + acc[2] / cnt) * cell]);
            }
        }
    }
    // Un quad par arête de la grille qui traverse la surface, entre les quatre cases qui la partagent
    let mut out_tris = vec![];
    let mut out_edges = vec![];
    for k in 1..n[2] - 1 {
        for j in 1..n[1] - 1 {
            for i in 1..n[0] - 1 {
                let a = field[at(i, j, k)] > iso;
                for axis in 0..3 {
                    let (ni, nj, nk) = match axis {
                        0 => (i + 1, j, k),
                        1 => (i, j + 1, k),
                        _ => (i, j, k + 1),
                    };
                    if ni >= n[0] || nj >= n[1] || nk >= n[2] {
                        continue;
                    }
                    let b = field[at(ni, nj, nk)] > iso;
                    if a == b {
                        continue;
                    }
                    // Les quatre cases autour de l'arête (i,j,k)-(ni,nj,nk)
                    let quad = match axis {
                        0 => [at(i, j - 1, k - 1), at(i, j, k - 1), at(i, j, k), at(i, j - 1, k)],
                        1 => [at(i - 1, j, k - 1), at(i - 1, j, k), at(i, j, k), at(i, j, k - 1)],
                        _ => [at(i - 1, j - 1, k), at(i, j - 1, k), at(i, j, k), at(i - 1, j, k)],
                    };
                    let q = quad.map(|c| vid[c]);
                    if q.contains(&u32::MAX) {
                        continue;
                    }
                    // Sens : la face regarde vers le dehors
                    let q = if a { q } else { [q[0], q[3], q[2], q[1]] };
                    out_tris.push([q[0], q[1], q[2]]);
                    out_tris.push([q[0], q[2], q[3]]);
                    // La diagonale du quad reste cachée
                    out_edges.push(0b011);
                    out_edges.push(0b110);
                }
            }
        }
    }
    // 4. Lissage léger (laplacien), en gardant la taille
    if smooth > 0 {
        let mut nb: Vec<Vec<u32>> = vec![vec![]; out_pos.len()];
        for t in &out_tris {
            for e in 0..3 {
                let (a, b) = (t[e], t[(e + 1) % 3]);
                if !nb[a as usize].contains(&b) {
                    nb[a as usize].push(b);
                    nb[b as usize].push(a);
                }
            }
        }
        for it in 0..smooth * 2 {
            // Alternance rétrécir / regonfler (Taubin) : lisse sans faire fondre la forme
            let f = if it % 2 == 0 { 0.5 } else { -0.53 };
            let old = out_pos.clone();
            for (i, p) in out_pos.iter_mut().enumerate() {
                if nb[i].is_empty() {
                    continue;
                }
                let mut c = [0.0f32; 3];
                for &q in &nb[i] {
                    for d in 0..3 {
                        c[d] += old[q as usize][d];
                    }
                }
                for d in 0..3 {
                    p[d] += (c[d] / nb[i].len() as f32 - old[i][d]) * f;
                }
            }
        }
    }
    if out_tris.is_empty() {
        return None;
    }
    Some(Out { pos: out_pos, tris: out_tris, edges: out_edges })
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Distance d'un point à un triangle (point le plus proche, méthode d'Ericson)
fn point_tri(p: V3, a: V3, b: V3, c: V3) -> f32 {
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    let closest = if d1 <= 0.0 && d2 <= 0.0 {
        a
    } else {
        let bp = sub(p, b);
        let (d3, d4) = (dot(ab, bp), dot(ac, bp));
        if d3 >= 0.0 && d4 <= d3 {
            b
        } else {
            let vc = d1 * d4 - d3 * d2;
            if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
                let v = d1 / (d1 - d3);
                [a[0] + ab[0] * v, a[1] + ab[1] * v, a[2] + ab[2] * v]
            } else {
                let cp = sub(p, c);
                let (d5, d6) = (dot(ab, cp), dot(ac, cp));
                if d6 >= 0.0 && d5 <= d6 {
                    c
                } else {
                    let vb = d5 * d2 - d1 * d6;
                    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
                        let w = d2 / (d2 - d6);
                        [a[0] + ac[0] * w, a[1] + ac[1] * w, a[2] + ac[2] * w]
                    } else {
                        let va = d3 * d6 - d5 * d4;
                        if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
                            let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
                            [b[0] + (c[0] - b[0]) * w, b[1] + (c[1] - b[1]) * w, b[2] + (c[2] - b[2]) * w]
                        } else {
                            let denom = 1.0 / (va + vb + vc);
                            let (v, w) = (vb * denom, vc * denom);
                            [a[0] + ab[0] * v + ac[0] * w, a[1] + ab[1] * v + ac[1] * w, a[2] + ab[2] * v + ac[2] * w]
                        }
                    }
                }
            }
        }
    };
    let d = sub(p, closest);
    dot(d, d).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Volume signé : positif quand les faces regardent dehors
    fn volume(p: &[V3], t: &[[u32; 3]]) -> f32 {
        t.iter()
            .map(|t| {
                let (a, b, c) = (p[t[0] as usize], p[t[1] as usize], p[t[2] as usize]);
                (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6.0
            })
            .sum()
    }

    #[test]
    fn sphere_reguliere() {
        // Sphère UV de rayon 1 : après remaillage, tous les sommets restent près du rayon
        let (nu, nv) = (48, 24);
        let mut p: Vec<V3> = vec![];
        for j in 0..=nv {
            let th = std::f32::consts::PI * j as f32 / nv as f32;
            for i in 0..nu {
                let ph = std::f32::consts::TAU * i as f32 / nu as f32;
                p.push([th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin()]);
            }
        }
        let mut t = vec![];
        for j in 0..nv {
            for i in 0..nu {
                let (a, b) = ((j * nu + i) as u32, (j * nu + (i + 1) % nu) as u32);
                let (c, d) = (a + nu as u32, b + nu as u32);
                t.push([a, b, c]);
                t.push([b, d, c]);
            }
        }
        let o = remesh(&p, &t, 0.06, 1).unwrap();
        let dev = o.pos.iter().map(|q| (dot(*q, *q).sqrt() - 1.0).abs()).fold(0.0f32, f32::max);
        eprintln!("écart max {dev}");
        assert!(dev < 0.02, "écart {dev}");
    }

    #[test]
    fn distance_point_triangle() {
        let (a, b, c) = ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert!((point_tri([0.2, 0.2, 0.5], a, b, c) - 0.5).abs() < 1e-6);
        assert!((point_tri([2.0, 0.0, 0.0], a, b, c) - 1.0).abs() < 1e-6);
        assert!((point_tri([1.0, 1.0, 0.0], a, b, c) - 0.5f32.sqrt()).abs() < 1e-6);
    }

    #[test]
    fn remaille_un_cube_ferme() {
        // Cube de 1 m (12 triangles, faces vers l'extérieur)
        let p: Vec<V3> = (0..8).map(|i| [(i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32]).collect();
        let t: Vec<[u32; 3]> = vec![[0, 2, 1], [1, 2, 3], [4, 5, 6], [5, 7, 6], [0, 1, 4], [1, 5, 4], [2, 6, 3], [3, 6, 7], [0, 4, 2], [2, 4, 6], [1, 3, 5], [3, 7, 5]];
        assert!((volume(&p, &t) - 1.0).abs() < 1e-4);
        let o = remesh(&p, &t, 0.05, 2).unwrap();
        assert!(o.tris.len() > 1000, "{} triangles", o.tris.len());
        assert_eq!(o.tris.len(), o.edges.len());
        let v = volume(&o.pos, &o.tris);
        // La distance signée colle à l'original : volume très proche, et positif (faces dehors)
        assert!(v > 0.93 && v < 1.03, "volume {v}");
        // Fermé : chaque arête est partagée par exactement deux triangles
        let mut count = std::collections::HashMap::new();
        for t in &o.tris {
            for e in 0..3 {
                let (a, b) = (t[e].min(t[(e + 1) % 3]), t[e].max(t[(e + 1) % 3]));
                *count.entry((a, b)).or_insert(0) += 1;
            }
        }
        assert!(count.values().all(|c| *c == 2), "surface ouverte");
    }
}

#[cfg(test)]
mod bool_tests {
    use super::*;

    fn cube() -> (Vec<V3>, Vec<[u32; 3]>) {
        let pos: Vec<V3> = (0..8).map(|i| [(i & 1) as f32 - 0.5, ((i >> 1) & 1) as f32 - 0.5, ((i >> 2) & 1) as f32 - 0.5]).collect();
        let q = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let tris = q.iter().flat_map(|f: &[u32; 4]| [[f[0], f[1], f[2]], [f[0], f[2], f[3]]]).collect();
        (pos, tris)
    }

    /// Volume d'un maillage fermé (théorème de la divergence)
    fn volume(o: &Out) -> f32 {
        o.tris.iter().map(|t| {
            let (a, b, c) = (o.pos[t[0] as usize], o.pos[t[1] as usize], o.pos[t[2] as usize]);
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6.0
        }).sum::<f32>().abs()
    }

    #[test]
    fn booleens_par_voxels() {
        let (pos, tris) = cube();
        let plain = remesh(&pos, &tris, 0.04, 0).unwrap();
        let v0 = volume(&plain);
        assert!((v0 - 1.0).abs() < 0.1, "{v0}");
        // Une sphère de rayon 0,3 au coin : la différence creuse, l'union ajoute, l'intersection garde peu
        let sh = Shape { kind: 1, center: [0.5, 0.5, 0.5], half: [0.3; 3] };
        let diff = volume(&boolean(&pos, &tris, &sh, 1, 0.04, 0).unwrap());
        let uni = volume(&boolean(&pos, &tris, &sh, 0, 0.04, 0).unwrap());
        let inter = volume(&boolean(&pos, &tris, &sh, 2, 0.04, 0).unwrap());
        let octant = 4.0 / 3.0 * std::f32::consts::PI * 0.027 / 8.0;
        assert!((v0 - diff - octant).abs() < 0.03, "{v0} {diff} {octant}");
        assert!((uni - v0 - 7.0 * octant).abs() < 0.05, "{uni}");
        assert!((inter - octant).abs() < 0.02, "{inter}");
        // Cube et cylindre au centre : un trou qui traverse
        let tube = Shape { kind: 2, center: [0.0; 3], half: [0.2, 1.0, 0.2] };
        let holed = volume(&boolean(&pos, &tris, &tube, 1, 0.03, 0).unwrap());
        let hole = std::f32::consts::PI * 0.04;
        assert!((v0 - holed - hole).abs() < 0.04, "{holed}");
    }
}
