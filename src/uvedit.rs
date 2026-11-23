//! Mode édition : topologie en polygones, IDs de matériau par face, coutures (seams),
//! dépliage UV (LSCM), rangement des îlots (packing) et densité de texels.
//!
//! Tout est gardé à côté du modèle dans `.app/meshdata/<id>.json` : le fichier d'origine
//! ne change pas, et l'export (OBJ, GLB) écrit les UV et les matériaux.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

pub type V3 = [f32; 3];
pub type V2 = [f32; 2];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
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
    let l = len(a).max(1e-12);
    [a[0] / l, a[1] / l, a[2] / l]
}

pub fn key(a: u32, b: u32) -> (u32, u32) {
    if a < b { (a, b) } else { (b, a) }
}

/// Maillage en polygones (quads, n-gones), sommets soudés.
pub struct Topo {
    pub pos: Vec<V3>,
    pub polys: Vec<Vec<u32>>,
    /// arête → faces qui la touchent
    pub edge_faces: HashMap<(u32, u32), Vec<u32>>,
}

impl Topo {
    pub fn new(pos: &[V3], tris: &[[u32; 3]], edges: &[u8]) -> Topo {
        let (pos, polys, _) = crate::model3d::rebuild_polygons(pos, tris, edges);
        let mut edge_faces: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        for (fi, f) in polys.iter().enumerate() {
            for i in 0..f.len() {
                edge_faces.entry(key(f[i], f[(i + 1) % f.len()])).or_default().push(fi as u32);
            }
        }
        Topo { pos, polys, edge_faces }
    }

    pub fn normal(&self, f: usize) -> V3 {
        // Newell : robuste pour les n-gones
        let p = &self.polys[f];
        let mut n = [0.0f32; 3];
        for i in 0..p.len() {
            let (a, b) = (self.pos[p[i] as usize], self.pos[p[(i + 1) % p.len()] as usize]);
            n[0] += (a[1] - b[1]) * (a[2] + b[2]);
            n[1] += (a[2] - b[2]) * (a[0] + b[0]);
            n[2] += (a[0] - b[0]) * (a[1] + b[1]);
        }
        norm(n)
    }

    pub fn area(&self, f: usize) -> f32 {
        let p = &self.polys[f];
        let a = self.pos[p[0] as usize];
        (1..p.len() - 1).map(|k| len(cross(sub(self.pos[p[k] as usize], a), sub(self.pos[p[k + 1] as usize], a))) * 0.5).sum()
    }

    #[allow(dead_code)]
    pub fn center(&self, f: usize) -> V3 {
        let p = &self.polys[f];
        mul(p.iter().fold([0.0; 3], |s, &v| add(s, self.pos[v as usize])), 1.0 / p.len() as f32)
    }

    /// Angle (degrés) entre les deux faces d'une arête ; None pour un bord.
    pub fn dihedral(&self, e: (u32, u32)) -> Option<f32> {
        let fs = self.edge_faces.get(&e)?;
        if fs.len() != 2 {
            return None;
        }
        Some(dot(self.normal(fs[0] as usize), self.normal(fs[1] as usize)).clamp(-1.0, 1.0).acos().to_degrees())
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Slot {
    pub name: String,
    pub color: [u8; 3],
    /// Matériau en nodes de cet ID (chemin du fichier .mat.json), vide si aucun
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub material: String,
}

pub const PALETTE: [[u8; 3]; 10] = [[200, 200, 205], [231, 111, 81], [42, 157, 143], [233, 196, 106], [69, 123, 157], [168, 98, 196], [244, 162, 97], [86, 180, 233], [140, 190, 80], [220, 90, 140]];

/// Ce qui est enregistré à côté du modèle.
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct EditData {
    /// Nombre de polygones au moment de l'enregistrement (pour vérifier que le modèle n'a pas changé)
    pub polys: usize,
    pub face_mat: Vec<u16>,
    pub slots: Vec<Slot>,
    pub seams: BTreeSet<(u32, u32)>,
    /// UV par coin de polygone
    pub uv: Vec<Vec<V2>>,
    /// Îlot de chaque polygone
    pub chart: Vec<u32>,
    #[serde(default = "tex_default")]
    pub texture_px: u32,
    #[serde(default = "margin_default")]
    pub margin_px: f32,
    /// Guides de retopologie (traits dessinés sur la surface, espace d'affichage)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<Vec<V3>>,
    /// Retopologie : touches de densité peintes (x, y, z, rayon, facteur)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub density: Vec<[f32; 5]>,
    /// Coins d'UV épinglés (face, coin) : le dépliage les laisse où ils sont
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub pins: BTreeSet<(u32, u32)>,
}

fn tex_default() -> u32 {
    2048
}
fn margin_default() -> f32 {
    8.0
}

impl EditData {
    pub fn fresh(t: &Topo) -> EditData {
        EditData { polys: t.polys.len(), face_mat: vec![0; t.polys.len()], slots: vec![Slot { name: "matériau 1".into(), color: PALETTE[0], material: String::new() }], seams: BTreeSet::new(), uv: vec![], chart: vec![], texture_px: 2048, margin_px: 8.0, guides: vec![], density: vec![], pins: BTreeSet::new() }
    }

    pub fn fits(&self, t: &Topo) -> bool {
        self.polys == t.polys.len() && self.face_mat.len() == t.polys.len()
    }

    pub fn has_uv(&self) -> bool {
        !self.uv.is_empty()
    }
}

// ---------- coutures ----------

/// Coutures sur les arêtes plus vives que `angle` (degrés).
pub fn seams_by_angle(t: &Topo, angle: f32) -> BTreeSet<(u32, u32)> {
    t.edge_faces.keys().filter(|e| t.dihedral(**e).is_some_and(|a| a > angle)).copied().collect()
}

/// Coutures « intelligentes » : on regroupe les faces en îlots presque plats (croissance de
/// régions selon les normales, comme le Smart UV Project de Blender), en partant des plus
/// grandes faces, puis on fusionne les îlots minuscules. Les coutures suivent les bords des îlots.
pub fn smart_seams(t: &Topo, angle: f32, min_faces: usize) -> BTreeSet<(u32, u32)> {
    let n = t.polys.len();
    let normals: Vec<V3> = (0..n).map(|f| t.normal(f)).collect();
    let areas: Vec<f32> = (0..n).map(|f| t.area(f)).collect();
    let cos = angle.to_radians().cos();
    let mut group = vec![u32::MAX; n];
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|a, b| areas[*b].partial_cmp(&areas[*a]).unwrap_or(std::cmp::Ordering::Equal));
    let neighbors = |f: usize| -> Vec<usize> {
        let p = &t.polys[f];
        let mut out = vec![];
        for i in 0..p.len() {
            if let Some(fs) = t.edge_faces.get(&key(p[i], p[(i + 1) % p.len()])) {
                out.extend(fs.iter().map(|&x| x as usize).filter(|&x| x != f));
            }
        }
        out
    };
    let mut gid = 0u32;
    let mut sizes = vec![];
    for &seed in &order {
        if group[seed] != u32::MAX {
            continue;
        }
        let mut avg = mul(normals[seed], areas[seed].max(1e-9));
        let mut stack = vec![seed];
        group[seed] = gid;
        let mut count = 1;
        while let Some(f) = stack.pop() {
            for g in neighbors(f) {
                if group[g] != u32::MAX {
                    continue;
                }
                if dot(norm(avg), normals[g]) >= cos {
                    group[g] = gid;
                    avg = add(avg, mul(normals[g], areas[g].max(1e-9)));
                    stack.push(g);
                    count += 1;
                }
            }
        }
        sizes.push(count);
        gid += 1;
    }
    // Îlots trop petits : rattachés au voisin le plus proche en normale
    for _ in 0..3 {
        for f in 0..n {
            let gf = group[f] as usize;
            if sizes[gf] >= min_faces {
                continue;
            }
            let best = neighbors(f).into_iter().filter(|&g| group[g] != group[f]).max_by(|a, b| dot(normals[f], normals[*a]).partial_cmp(&dot(normals[f], normals[*b])).unwrap_or(std::cmp::Ordering::Equal));
            if let Some(g) = best {
                sizes[gf] -= 1;
                group[f] = group[g];
                sizes[group[g] as usize] += 1;
            }
        }
    }
    t.edge_faces.iter().filter(|(_, fs)| fs.len() == 2 && group[fs[0] as usize] != group[fs[1] as usize]).map(|(e, _)| *e).collect()
}

// ---------- îlots et sommets UV ----------

/// Îlots : faces reliées par des arêtes sans couture.
pub fn charts(t: &Topo, seams: &BTreeSet<(u32, u32)>) -> Vec<u32> {
    let n = t.polys.len();
    let mut chart = vec![u32::MAX; n];
    let mut c = 0;
    for s in 0..n {
        if chart[s] != u32::MAX {
            continue;
        }
        chart[s] = c;
        let mut stack = vec![s];
        while let Some(f) = stack.pop() {
            let p = &t.polys[f];
            for i in 0..p.len() {
                let e = key(p[i], p[(i + 1) % p.len()]);
                if seams.contains(&e) {
                    continue;
                }
                if let Some(fs) = t.edge_faces.get(&e) {
                    if fs.len() == 2 {
                        for &g in fs {
                            if chart[g as usize] == u32::MAX {
                                chart[g as usize] = c;
                                stack.push(g as usize);
                            }
                        }
                    }
                }
            }
        }
        c += 1;
    }
    chart
}

/// Sommets UV d'un îlot : un coin (face, i) par sommet, fusionnés à travers les arêtes
/// sans couture (une fente dans un îlot garde donc deux côtés séparés).
fn chart_vertices(t: &Topo, faces: &[usize], seams: &BTreeSet<(u32, u32)>) -> (Vec<(usize, usize)>, Vec<usize>) {
    let mut corners = vec![];
    let mut idx: HashMap<(usize, usize), usize> = HashMap::new();
    for &f in faces {
        for i in 0..t.polys[f].len() {
            idx.insert((f, i), corners.len());
            corners.push((f, i));
        }
    }
    let mut parent: Vec<usize> = (0..corners.len()).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let in_chart: std::collections::HashSet<usize> = faces.iter().copied().collect();
    for &f in faces {
        let p = &t.polys[f];
        for i in 0..p.len() {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            let e = key(a, b);
            if seams.contains(&e) {
                continue;
            }
            let Some(fs) = t.edge_faces.get(&e) else { continue };
            if fs.len() != 2 {
                continue;
            }
            let g = if fs[0] as usize == f { fs[1] as usize } else { fs[0] as usize };
            if !in_chart.contains(&g) {
                continue;
            }
            let q = &t.polys[g];
            for v in [a, b] {
                let (Some(ci), Some(cj)) = (p.iter().position(|&x| x == v), q.iter().position(|&x| x == v)) else { continue };
                let (x, y) = (find(&mut parent, idx[&(f, ci)]), find(&mut parent, idx[&(g, cj)]));
                parent[x] = y;
            }
        }
    }
    let mut class: HashMap<usize, usize> = HashMap::new();
    let mut corner_vert = vec![0; corners.len()];
    for k in 0..corners.len() {
        let r = find(&mut parent, k);
        let n = class.len();
        corner_vert[k] = *class.entry(r).or_insert(n);
    }
    (corners, corner_vert)
}

// ---------- LSCM ----------

/// Moindres carrés creux résolus par gradient conjugué (équations normales).
fn solve_cg(rows: &[Vec<(usize, f32)>], rhs: &[f32], n: usize, iters: usize) -> Vec<f32> {
    let at = |x: &[f32]| -> Vec<f32> {
        // A^T (A x)
        let mut out = vec![0.0f32; n];
        for (r, row) in rows.iter().enumerate() {
            let _ = r;
            let ax: f32 = row.iter().map(|(c, v)| v * x[*c]).sum();
            for (c, v) in row {
                out[*c] += v * ax;
            }
        }
        out
    };
    let mut b = vec![0.0f32; n];
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row {
            b[*c] += v * rhs[r];
        }
    }
    let mut x = vec![0.0f32; n];
    let mut r = b.clone();
    let mut p = r.clone();
    let mut rs: f64 = r.iter().map(|v| (*v as f64) * (*v as f64)).sum();
    let tol = rs * 1e-14;
    for _ in 0..iters {
        if rs <= tol || rs == 0.0 {
            break;
        }
        let ap = at(&p);
        let pap: f64 = p.iter().zip(&ap).map(|(a, b)| (*a as f64) * (*b as f64)).sum();
        if pap.abs() < 1e-30 {
            break;
        }
        let alpha = (rs / pap) as f32;
        for i in 0..n {
            x[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
        }
        let rs2: f64 = r.iter().map(|v| (*v as f64) * (*v as f64)).sum();
        let beta = (rs2 / rs) as f32;
        for i in 0..n {
            p[i] = r[i] + beta * p[i];
        }
        rs = rs2;
    }
    x
}

/// Déplie un îlot. Retourne les UV de chaque sommet UV.
fn lscm(t: &Topo, faces: &[usize], corners: &[(usize, usize)], corner_vert: &[usize]) -> Vec<V2> {
    lscm_pinned(t, faces, corners, corner_vert, &HashMap::new())
}

/// LSCM avec des sommets épinglés (au moins deux pour fixer l'îlot ; sinon épingles automatiques).
fn lscm_pinned(t: &Topo, faces: &[usize], corners: &[(usize, usize)], corner_vert: &[usize], user_pins: &HashMap<usize, V2>) -> Vec<V2> {
    let nv = corner_vert.iter().copied().max().map(|m| m + 1).unwrap_or(0);
    let mut pos3 = vec![[0.0f32; 3]; nv];
    for (k, &(f, i)) in corners.iter().enumerate() {
        pos3[corner_vert[k]] = t.pos[t.polys[f][i] as usize];
    }
    // Index du coin (f, i) → sommet UV
    let mut cv: HashMap<(usize, usize), usize> = HashMap::new();
    for (k, c) in corners.iter().enumerate() {
        cv.insert(*c, corner_vert[k]);
    }
    // Épingles : les deux sommets les plus éloignés le long de l'axe principal
    let c0 = pos3.iter().fold([0.0; 3], |s, p| add(s, *p));
    let c0 = mul(c0, 1.0 / nv.max(1) as f32);
    let far = |from: V3| (0..nv).max_by(|a, b| len(sub(pos3[*a], from)).partial_cmp(&len(sub(pos3[*b], from))).unwrap_or(std::cmp::Ordering::Equal)).unwrap_or(0);
    let p0 = far(c0);
    let p1 = far(pos3[p0]);
    if nv < 3 || p0 == p1 {
        return planar(&pos3, t, faces);
    }
    let d = len(sub(pos3[p1], pos3[p0]));
    let pinned: HashMap<usize, V2> = if user_pins.len() >= 2 { user_pins.clone() } else { [(p0, [0.0, 0.0]), (p1, [d, 0.0])].into_iter().collect() };
    // Inconnues : u et v de chaque sommet non épinglé
    let mut var = vec![usize::MAX; nv];
    let mut nfree = 0;
    for v in 0..nv {
        if !pinned.contains_key(&v) {
            var[v] = nfree;
            nfree += 1;
        }
    }
    let n = nfree * 2;
    let mut rows: Vec<Vec<(usize, f32)>> = vec![];
    let mut rhs: Vec<f32> = vec![];
    for &f in faces {
        let p = &t.polys[f];
        for k in 1..p.len() - 1 {
            let ids = [cv[&(f, 0)], cv[&(f, k)], cv[&(f, k + 1)]];
            let (a, b, c) = (pos3[ids[0]], pos3[ids[1]], pos3[ids[2]]);
            // Repère local du triangle
            let ex = norm(sub(b, a));
            let nn = cross(sub(b, a), sub(c, a));
            let area2 = len(nn);
            if area2 < 1e-12 {
                continue;
            }
            let ey = norm(cross(norm(nn), ex));
            let xy = [[0.0f32, 0.0], [dot(sub(b, a), ex), dot(sub(b, a), ey)], [dot(sub(c, a), ex), dot(sub(c, a), ey)]];
            // W_j = (x_k - x_l) complexes, dT = sqrt(aire*2)
            let dt = area2.sqrt();
            let w = [
                [(xy[2][0] - xy[1][0]) / dt, (xy[2][1] - xy[1][1]) / dt],
                [(xy[0][0] - xy[2][0]) / dt, (xy[0][1] - xy[2][1]) / dt],
                [(xy[1][0] - xy[0][0]) / dt, (xy[1][1] - xy[0][1]) / dt],
            ];
            // Parties réelle et imaginaire de sum W_j * (u_j + i v_j) = 0
            for part in 0..2 {
                let mut row = vec![];
                let mut r = 0.0f32;
                for j in 0..3 {
                    let (wr, wi) = (w[j][0], w[j][1]);
                    let (cu, cv_) = if part == 0 { (wr, -wi) } else { (wi, wr) };
                    match pinned.get(&ids[j]) {
                        Some(uv) => r -= cu * uv[0] + cv_ * uv[1],
                        None => {
                            row.push((var[ids[j]] * 2, cu));
                            row.push((var[ids[j]] * 2 + 1, cv_));
                        }
                    }
                }
                rows.push(row);
                rhs.push(r);
            }
        }
    }
    let x = solve_cg(&rows, &rhs, n, (n * 2).clamp(200, 4000));
    let mut uv = vec![[0.0f32; 2]; nv];
    for v in 0..nv {
        uv[v] = match pinned.get(&v) {
            Some(p) => *p,
            None => [x[var[v] * 2], x[var[v] * 2 + 1]],
        };
    }
    if uv.iter().any(|p| !p[0].is_finite() || !p[1].is_finite()) {
        return planar(&pos3, t, faces);
    }
    // Faces retournées en majorité : miroir
    let mut signed = 0.0;
    for &f in faces {
        let p = &t.polys[f];
        for k in 1..p.len() - 1 {
            let (a, b, c) = (uv[cv[&(f, 0)]], uv[cv[&(f, k)]], uv[cv[&(f, k + 1)]]);
            signed += (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        }
    }
    if signed < 0.0 && user_pins.len() < 2 {
        for p in &mut uv {
            p[0] = -p[0];
        }
    }
    uv
}

/// Repli : projection plane le long de la normale moyenne.
fn planar(pos3: &[V3], t: &Topo, faces: &[usize]) -> Vec<V2> {
    let n = norm(faces.iter().fold([0.0; 3], |s, &f| add(s, mul(t.normal(f), t.area(f)))));
    let up = if n[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    let ex = norm(cross(up, n));
    let ey = cross(n, ex);
    pos3.iter().map(|p| [dot(*p, ex), dot(*p, ey)]).collect()
}

/// Déplie chaque îlot (sans les ranger). Remplit `uv` et `chart`.
pub fn unwrap_charts(t: &Topo, d: &mut EditData) {
    d.chart = charts(t, &d.seams);
    let nc = d.chart.iter().copied().max().map(|m| m as usize + 1).unwrap_or(0);
    let mut by_chart: Vec<Vec<usize>> = vec![vec![]; nc];
    for (f, &c) in d.chart.iter().enumerate() {
        by_chart[c as usize].push(f);
    }
    let mut uv: Vec<Vec<V2>> = t.polys.iter().map(|p| vec![[0.0; 2]; p.len()]).collect();
    let results: Vec<(Vec<(usize, usize)>, Vec<usize>, Vec<V2>)> = {
        use rayon::prelude::*;
        by_chart
            .par_iter()
            .map(|faces| {
                let (corners, cvert) = chart_vertices(t, faces, &d.seams);
                let u = lscm(t, faces, &corners, &cvert);
                (corners, cvert, u)
            })
            .collect()
    };
    for (corners, cvert, u) in results {
        for (k, (f, i)) in corners.iter().enumerate() {
            uv[*f][*i] = u[cvert[k]];
        }
    }
    d.uv = uv;
}

/// Déplie tout le modèle selon les coutures, puis range les îlots.
#[allow(dead_code)]
pub fn unwrap(t: &Topo, d: &mut EditData) {
    unwrap_charts(t, d);
    pack(t, d, &PackOptions::default());
}

// ---------- rangement (packing) et densité ----------

#[derive(Clone, Copy)]
pub struct PackOptions {
    /// Essayer plusieurs ordres, rotations et largeurs et garder le meilleur
    pub optimize: bool,
    pub rotate: bool,
    /// Échelle fixe (densité imposée) : les îlots ne sont pas réduits pour tenir dans 0-1
    pub fixed_scale: Option<f32>,
}

impl Default for PackOptions {
    fn default() -> Self {
        PackOptions { optimize: true, rotate: true, fixed_scale: None }
    }
}

fn chart_faces(d: &EditData) -> Vec<Vec<usize>> {
    let nc = d.chart.iter().copied().max().map(|m| m as usize + 1).unwrap_or(0);
    let mut by: Vec<Vec<usize>> = vec![vec![]; nc];
    for (f, &c) in d.chart.iter().enumerate() {
        by[c as usize].push(f);
    }
    by
}

fn uv_area(d: &EditData, faces: &[usize]) -> f32 {
    faces
        .iter()
        .map(|&f| {
            let p = &d.uv[f];
            (1..p.len() - 1).map(|k| (((p[k][0] - p[0][0]) * (p[k + 1][1] - p[0][1]) - (p[k + 1][0] - p[0][0]) * (p[k][1] - p[0][1])) * 0.5).abs()).sum::<f32>()
        })
        .sum()
}

/// Proportion de la texture utilisée (0-1).
pub fn fill_rate(d: &EditData) -> f32 {
    let all: Vec<usize> = (0..d.uv.len()).collect();
    uv_area(d, &all).min(1.0)
}

/// Densité de texels de chaque îlot (pixels par unité 3D, `scale` = taille réelle de l'objet).
pub fn densities(t: &Topo, d: &EditData, scale: f32) -> Vec<f32> {
    chart_faces(d)
        .iter()
        .map(|faces| {
            let a3: f32 = faces.iter().map(|&f| t.area(f)).sum::<f32>() * scale * scale;
            let a2 = uv_area(d, faces);
            if a3 > 0.0 { (a2 / a3).sqrt() * d.texture_px as f32 } else { 0.0 }
        })
        .collect()
}

/// Range les îlots dans le carré 0-1 : même densité pour tous (îlots remis à l'échelle de
/// leur surface 3D), rotation qui minimise la boîte, puis rangement en étagères.
pub fn pack(t: &Topo, d: &mut EditData, o: &PackOptions) {
    let groups = chart_faces(d);
    if groups.is_empty() {
        return;
    }
    // 1. Même densité : chaque îlot à l'échelle de sa surface 3D
    for faces in &groups {
        let a3: f32 = faces.iter().map(|&f| t.area(f)).sum();
        let a2 = uv_area(d, faces);
        if a2 <= 1e-12 || a3 <= 0.0 {
            continue;
        }
        let s = (a3 / a2).sqrt();
        for &f in faces {
            for p in &mut d.uv[f] {
                p[0] *= s;
                p[1] *= s;
            }
        }
    }
    // 2. Rotation : boîte la plus petite (pas de 3°), îlot posé à plat
    let bbox = |d: &EditData, faces: &[usize]| -> (V2, V2) {
        let mut lo = [f32::MAX; 2];
        let mut hi = [f32::MIN; 2];
        for &f in faces {
            for p in &d.uv[f] {
                lo = [lo[0].min(p[0]), lo[1].min(p[1])];
                hi = [hi[0].max(p[0]), hi[1].max(p[1])];
            }
        }
        (lo, hi)
    };
    if o.rotate {
        for faces in &groups {
            let pts: Vec<V2> = faces.iter().flat_map(|&f| d.uv[f].iter().copied()).step_by(1).take(4000).collect();
            let mut best = (f32::MAX, 0.0f32);
            for k in 0..30 {
                let a = (k as f32 * 3.0).to_radians();
                let (s, c) = a.sin_cos();
                let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
                for p in &pts {
                    let q = [p[0] * c - p[1] * s, p[0] * s + p[1] * c];
                    lo = [lo[0].min(q[0]), lo[1].min(q[1])];
                    hi = [hi[0].max(q[0]), hi[1].max(q[1])];
                }
                let area = (hi[0] - lo[0]) * (hi[1] - lo[1]);
                if area < best.0 {
                    best = (area, a);
                }
            }
            let (s, c) = best.1.sin_cos();
            for &f in faces {
                for p in &mut d.uv[f] {
                    *p = [p[0] * c - p[1] * s, p[0] * s + p[1] * c];
                }
            }
        }
    }
    // 3. Étagères
    let boxes: Vec<(V2, V2)> = groups.iter().map(|g| bbox(d, g)).collect();
    let total_area: f32 = boxes.iter().map(|(lo, hi)| (hi[0] - lo[0]) * (hi[1] - lo[1])).sum();
    let margin_rel = d.margin_px / d.texture_px.max(1) as f32;
    // Essais : (ordre, rotation de 90° des îlots hauts, largeur relative)
    let orders: &[u8] = if o.optimize { &[0, 1, 2] } else { &[0] };
    let flips: &[bool] = if o.optimize { &[false, true] } else { &[false] };
    let widths: &[f32] = if o.optimize { &[0.8, 0.9, 1.0, 1.1, 1.25, 1.4] } else { &[1.0] };
    let mut best: Option<(f32, Vec<(V2, bool)>, f32)> = None; // (côté, positions, échelle)
    for &ord in orders {
        for &flip in flips {
            for &wk in widths {
                let mut sizes: Vec<(usize, V2, bool)> = boxes
                    .iter()
                    .enumerate()
                    .map(|(i, (lo, hi))| {
                        let (w, h) = (hi[0] - lo[0], hi[1] - lo[1]);
                        if flip && h > w { (i, [h, w], true) } else { (i, [w, h], false) }
                    })
                    .collect();
                match ord {
                    0 => sizes.sort_by(|a, b| b.1[1].partial_cmp(&a.1[1]).unwrap_or(std::cmp::Ordering::Equal)),
                    1 => sizes.sort_by(|a, b| (b.1[0] * b.1[1]).partial_cmp(&(a.1[0] * a.1[1])).unwrap_or(std::cmp::Ordering::Equal)),
                    _ => sizes.sort_by(|a, b| b.1[0].max(b.1[1]).partial_cmp(&a.1[0].max(a.1[1])).unwrap_or(std::cmp::Ordering::Equal)),
                }
                // L'échelle finale n'est connue qu'à la fin : la marge est estimée sur la racine de la surface
                let side0 = total_area.sqrt().max(1e-9);
                let m = margin_rel * side0 * 1.2;
                let shelf_w = (side0 * wk).max(sizes.iter().map(|s| s.1[0] + m).fold(0.0, f32::max));
                let mut pos = vec![([0.0f32; 2], false); boxes.len()];
                let (mut x, mut y, mut row_h, mut used_w) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
                for (i, sz, fl) in &sizes {
                    if x + sz[0] + m > shelf_w && x > 0.0 {
                        y += row_h;
                        x = 0.0;
                        row_h = 0.0;
                    }
                    pos[*i] = ([x + m * 0.5, y + m * 0.5], *fl);
                    x += sz[0] + m;
                    used_w = used_w.max(x);
                    row_h = row_h.max(sz[1] + m);
                }
                let side = used_w.max(y + row_h);
                if best.as_ref().is_none_or(|b| side < b.0) {
                    best = Some((side, pos, 1.0 / side));
                }
                // Skyline (bas-gauche) : remplit les creux laissés par les étagères
                let (spos, sside) = skyline(&sizes, shelf_w, m, true);
                if best.as_ref().is_none_or(|b| sside < b.0) {
                    best = Some((sside, spos, 1.0 / sside));
                }
            }
        }
    }
    let Some((_, pos, scale)) = best else { return };
    let scale = o.fixed_scale.unwrap_or(scale);
    for (gi, faces) in groups.iter().enumerate() {
        let (lo, hi) = boxes[gi];
        let (at, fl) = pos[gi];
        for &f in faces {
            for p in &mut d.uv[f] {
                let mut q = [p[0] - lo[0], p[1] - lo[1]];
                if fl {
                    // Rotation de 90° : (x, y) → (h - y, x)
                    q = [(hi[1] - lo[1]) - q[1], q[0]];
                }
                *p = [(at[0] + q[0]) * scale, (at[1] + q[1]) * scale];
            }
        }
    }
}

/// Rangement « skyline » : chaque îlot va là où il finit le plus bas (puis le plus à gauche),
/// tourné de 90° si ça tient mieux. Retourne (positions, côté du carré).
fn skyline(sizes: &[(usize, V2, bool)], width: f32, m: f32, rotate: bool) -> (Vec<(V2, bool)>, f32) {
    let n = sizes.iter().map(|s| s.0).max().map(|x| x + 1).unwrap_or(0);
    let mut pos = vec![([0.0f32; 2], false); n];
    // Segments (x, largeur, hauteur)
    let mut sky: Vec<(f32, f32, f32)> = vec![(0.0, width, 0.0)];
    let mut top = 0.0f32;
    let mut right = 0.0f32;
    let fit = |sky: &[(f32, f32, f32)], i: usize, w: f32| -> Option<f32> {
        let x0 = sky[i].0;
        if x0 + w > width + 1e-6 {
            return None;
        }
        let mut y = 0.0f32;
        let mut cover = 0.0;
        let mut j = i;
        while cover < w - 1e-9 && j < sky.len() {
            y = y.max(sky[j].2);
            cover += sky[j].1 - if j == i { 0.0 } else { 0.0 };
            j += 1;
        }
        (cover >= w - 1e-6).then_some(y)
    };
    for (id, sz, fl) in sizes {
        let mut best: Option<(f32, f32, usize, V2, bool)> = None; // (y+h, x, i, taille, retourné)
        let orients: Vec<(V2, bool)> = if rotate { vec![(*sz, *fl), ([sz[1], sz[0]], !*fl)] } else { vec![(*sz, *fl)] };
        for (dim, f2) in orients {
            let (w, h) = (dim[0] + m, dim[1] + m);
            for i in 0..sky.len() {
                if let Some(y) = fit(&sky, i, w) {
                    let cand = (y + h, sky[i].0, i, dim, f2);
                    if best.as_ref().is_none_or(|b| cand.0 < b.0 - 1e-9 || ((cand.0 - b.0).abs() < 1e-9 && cand.1 < b.1)) {
                        best = Some(cand);
                    }
                }
            }
        }
        let Some((yh, x, _, dim, f2)) = best else { continue };
        let (w, h) = (dim[0] + m, dim[1] + m);
        let y = yh - h;
        pos[*id] = ([x + m * 0.5, y + m * 0.5], f2);
        top = top.max(yh);
        right = right.max(x + w);
        // Mise à jour de la ligne d'horizon
        let mut next: Vec<(f32, f32, f32)> = vec![];
        for seg in &sky {
            let (sx, sw, sh) = *seg;
            let (a, b) = (sx, sx + sw);
            if b <= x + 1e-9 || a >= x + w - 1e-9 {
                next.push(*seg);
            } else {
                if a < x {
                    next.push((a, x - a, sh));
                }
                if b > x + w {
                    next.push((x + w, b - x - w, sh));
                }
            }
        }
        next.push((x, w, yh));
        next.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap_or(std::cmp::Ordering::Equal));
        // Fusion des segments de même hauteur
        let mut merged: Vec<(f32, f32, f32)> = vec![];
        for seg in next {
            if let Some(last) = merged.last_mut() {
                if (last.2 - seg.2).abs() < 1e-9 && (last.0 + last.1 - seg.0).abs() < 1e-6 {
                    last.1 += seg.1;
                    continue;
                }
            }
            merged.push(seg);
        }
        sky = merged;
    }
    (pos, top.max(right))
}

/// Impose une densité (pixels par unité réelle) : tous les îlots à la même échelle,
/// rangés sans être réduits (ils peuvent déborder du carré si la densité est trop forte).
pub fn set_density(t: &Topo, d: &mut EditData, px_per_unit: f32, scale: f32) {
    pack(t, d, &PackOptions { fixed_scale: Some(1.0), ..PackOptions::default() });
    // Après pack à échelle 1 : 1 unité UV = 1 unité 3D normalisée
    let k = px_per_unit * scale / d.texture_px.max(1) as f32;
    for f in &mut d.uv {
        for p in f.iter_mut() {
            p[0] *= k;
            p[1] *= k;
        }
    }
}

// ---------- maillage d'affichage ----------

/// Triangles pour l'affichage et l'export : (positions, triangles, polygone de chaque triangle,
/// masque des arêtes visibles, UV par coin de triangle).
/// Découpe d'un polygone en triangles (indices dans le polygone). Un éventail depuis un coin
/// suffit pour un polygone convexe ; pour un polygone creux il retourne des triangles, d'où des
/// éclats blancs à l'affichage. On essaie donc chaque coin, puis on découpe « par oreilles ».
fn poly_tris(pos: &[V3], p: &[u32]) -> Vec<[usize; 3]> {
    let n = p.len();
    if n < 3 {
        return vec![];
    }
    if n == 3 {
        return vec![[0, 1, 2]];
    }
    let at = |i: usize| pos[p[i] as usize];
    let sub = |a: V3, b: V3| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: V3, b: V3| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let dot = |a: V3, b: V3| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    // Normale du polygone (Newell)
    let mut nrm = [0.0f32; 3];
    for i in 0..n {
        let (a, b) = (at(i), at((i + 1) % n));
        nrm[0] += (a[1] - b[1]) * (a[2] + b[2]);
        nrm[1] += (a[2] - b[2]) * (a[0] + b[0]);
        nrm[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let good = |i: usize, j: usize, k: usize| dot(cross(sub(at(j), at(i)), sub(at(k), at(i))), nrm) >= -1e-12;
    for s in 0..n {
        if (1..n - 1).all(|k| good(s, (s + k) % n, (s + k + 1) % n)) {
            return (1..n - 1).map(|k| [s, (s + k) % n, (s + k + 1) % n]).collect();
        }
    }
    // Oreilles
    let mut idx: Vec<usize> = (0..n).collect();
    let mut out = vec![];
    let inside = |q: V3, a: V3, b: V3, c: V3| {
        let s = |x: V3, y: V3| dot(cross(sub(y, x), sub(q, x)), nrm);
        s(a, b) > 0.0 && s(b, c) > 0.0 && s(c, a) > 0.0
    };
    while idx.len() > 3 {
        let m = idx.len();
        let ear = (0..m).find(|&e| {
            let (i, j, k) = (idx[(e + m - 1) % m], idx[e], idx[(e + 1) % m]);
            good(i, j, k) && !idx.iter().any(|&o| o != i && o != j && o != k && inside(at(o), at(i), at(j), at(k)))
        });
        let e = ear.unwrap_or(0);
        out.push([idx[(e + m - 1) % m], idx[e], idx[(e + 1) % m]]);
        idx.remove(e);
    }
    out.push([idx[0], idx[1], idx[2]]);
    out
}

pub fn triangulate(t: &Topo, d: &EditData) -> (Vec<[u32; 3]>, Vec<u32>, Vec<u8>, Vec<[V2; 3]>) {
    let mut tris = vec![];
    let mut owner = vec![];
    let mut edges = vec![];
    let mut uvs = vec![];
    for (f, p) in t.polys.iter().enumerate() {
        let n = p.len();
        for tr in poly_tris(&t.pos, p) {
            tris.push([p[tr[0]], p[tr[1]], p[tr[2]]]);
            owner.push(f as u32);
            // Une arête est visible si elle est un côté du polygone
            let mut mask = 0u8;
            for k in 0..3 {
                let (a, b) = (tr[k], tr[(k + 1) % 3]);
                if (a + 1) % n == b || (b + 1) % n == a {
                    mask |= 1 << k;
                }
            }
            edges.push(mask);
            if d.has_uv() && d.uv.len() == t.polys.len() {
                let u = &d.uv[f];
                uvs.push([u[tr[0]], u[tr[1]], u[tr[2]]]);
            }
        }
    }
    (tris, owner, edges, uvs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bake_des_textures() {
        // Cube déplié automatiquement, deux IDs, un matériau « métal peint usé » sur le second
        let t = cube();
        let n = t.polys.len();
        let mut e = Edit { topo: t, ..Edit::open(&crate::modifiers::Mesh { pos: vec![], tris: vec![], colors: vec![], edges: vec![] }, std::env::temp_dir().join("aryadne-bake-test.json"), 1.0) };
        e.data = EditData::fresh(&e.topo);
        e.auto_now(true);
        assert!(e.data.has_uv());
        e.data.slots.push(Slot { name: "usé".into(), color: [200, 30, 30], material: String::new() });
        e.data.face_mat = (0..n).map(|f| (f % 2) as u16).collect();
        e.bake_size = 128;
        let prog = crate::nodes::Program::compile(&crate::nodes::material_preset("métal peint usé").unwrap(), "", 64).ok().unwrap();
        let dir = std::env::temp_dir().join("aryadne-bake");
        e.bake_rays = 8;
        let files = e.bake(&[None, Some(std::sync::Arc::new(prog))], &dir, "cube", None).unwrap();
        assert!(files.len() >= 7);
        // Un cube convexe n'est ombré par rien : occlusion blanche
        let ao = image::open(files.iter().find(|f| f.to_string_lossy().contains("occlusion")).unwrap()).unwrap().to_luma8();
        assert!(ao.pixels().filter(|p| p.0[0] > 240).count() > ao.pixels().count() * 9 / 10);
        let col = image::open(&files[0]).unwrap().to_rgb8();
        // Les UV couvrent une bonne partie de la texture, et les deux IDs donnent des couleurs différentes
        let distinct: std::collections::HashSet<[u8; 3]> = col.pixels().map(|p| p.0).collect();
        assert!(distinct.len() > 2, "{} couleurs", distinct.len());
        let nrm = image::open(files.iter().find(|f| f.to_string_lossy().contains("normale")).unwrap()).unwrap().to_rgb8();
        assert!(nrm.pixels().any(|p| p.0[0] > 200) && nrm.pixels().any(|p| p.0[1] > 200));
    }

    #[test]
    fn polygone_creux_sans_triangle_retourne() {
        // Flèche : le coin 3 rentre dans le polygone
        let pos: Vec<V3> = vec![[0., 0., 0.], [2., 0., 0.], [2., 2., 0.], [1.6, 0.4, 0.], [0., 2., 0.]];
        let p: Vec<u32> = vec![0, 1, 2, 3, 4];
        let tr = poly_tris(&pos, &p);
        assert_eq!(tr.len(), 3);
        let mut area = 0.0;
        for t in &tr {
            let (a, b, c) = (pos[t[0]], pos[t[1]], pos[t[2]]);
            let z = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            assert!(z >= 0.0, "triangle retourné : {t:?}");
            area += z / 2.0;
        }
        assert!((area - 2.4).abs() < 1e-4, "aire {area}");
    }

    fn cube() -> Topo {
        let pos: Vec<V3> = vec![[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.], [0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]];
        let quads = [[0u32, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];
        let mut tris = vec![];
        let mut edges = vec![];
        for q in quads {
            for k in 1..3 {
                tris.push([q[0], q[k], q[k + 1]]);
                edges.push(crate::model3d::fan_mask(k, 4));
            }
        }
        Topo::new(&pos, &tris, &edges)
    }

    #[test]
    fn cube_deplie() {
        let t = cube();
        assert_eq!(t.polys.len(), 6);
        let seams = seams_by_angle(&t, 45.0);
        assert_eq!(seams.len(), 12);
        let mut d = EditData::fresh(&t);
        d.seams = seams;
        unwrap(&t, &mut d);
        assert_eq!(d.chart.iter().max(), Some(&5), "6 îlots");
        // Tous dans le carré, sans chevauchement grossier, densité égale
        for f in &d.uv {
            for p in f {
                assert!(p[0] >= -1e-4 && p[0] <= 1.0001 && p[1] >= -1e-4 && p[1] <= 1.0001, "{p:?}");
            }
        }
        let dens = densities(&t, &d, 1.0);
        let (mn, mx) = dens.iter().fold((f32::MAX, 0.0f32), |a, &x| (a.0.min(x), a.1.max(x)));
        assert!(mx / mn < 1.01, "densité égale : {dens:?}");
        assert!(fill_rate(&d) > 0.3, "remplissage {}", fill_rate(&d));
        // Faces carrées restent carrées (conforme)
        let u = &d.uv[0];
        let e1 = ((u[1][0] - u[0][0]).powi(2) + (u[1][1] - u[0][1]).powi(2)).sqrt();
        let e2 = ((u[2][0] - u[1][0]).powi(2) + (u[2][1] - u[1][1]).powi(2)).sqrt();
        assert!((e1 / e2 - 1.0).abs() < 0.02, "{e1} {e2}");
    }

    #[test]
    fn coutures_intelligentes() {
        let t = cube();
        let s = smart_seams(&t, 60.0, 1);
        assert_eq!(s.len(), 12);
        // Une seule couture verticale : le tour du cube devient une bande dépliée
        let mut d = EditData::fresh(&t);
        d.seams = seams_by_angle(&t, 45.0);
        d.seams.remove(&key(1, 5));
        d.seams.remove(&key(2, 6));
        d.seams.remove(&key(3, 7));
        unwrap(&t, &mut d);
        let n = d.chart.iter().max().unwrap() + 1;
        assert_eq!(n, 3, "bande des 4 côtés + dessus + dessous");
        assert!(d.uv.iter().flatten().all(|p| p[0].is_finite() && p[1].is_finite()));
    }
}

#[cfg(test)]
mod vrai {
    use super::*;
    /// ARYADNE_BAKE_OBJ=bas.obj [ARYADNE_BAKE_HIGH=haut.obj] cargo test bake_reel -- --nocapture
    #[test]
    fn bake_reel() {
        let Ok(p) = std::env::var("ARYADNE_BAKE_OBJ") else { return };
        let m = crate::model3d::load(std::path::Path::new(&p)).unwrap();
        let mesh = crate::modifiers::Mesh::from_model(&m, false);
        let mut e = Edit::open(&mesh, std::env::temp_dir().join("aryadne-bake-reel.json"), 1.0);
        e.data = EditData::fresh(&e.topo);
        e.auto_now(true);
        e.bake_size = 1024;
        e.bake_rays = std::env::var("ARYADNE_BAKE_RAYS").ok().and_then(|v| v.parse().ok()).unwrap_or(24);
        let high = std::env::var("ARYADNE_BAKE_HIGH").ok().map(|h| {
            let hm = crate::modifiers::Mesh::from_model(&crate::model3d::load(std::path::Path::new(&h)).unwrap(), false);
            (hm.pos, hm.tris)
        });
        let t0 = std::time::Instant::now();
        let files = e.bake(&[None], &std::env::temp_dir().join("aryadne-bake-reel"), "m", high.as_ref()).unwrap();
        println!("{} cartes en {:?}", files.len(), t0.elapsed());
    }
    /// ARYADNE_UV_OBJ=chemin.obj cargo test uv_reel -- --nocapture
    #[test]
    fn uv_reel() {
        let Ok(p) = std::env::var("ARYADNE_UV_OBJ") else { return };
        let m = crate::model3d::load(std::path::Path::new(&p)).unwrap();
        let mesh = crate::modifiers::Mesh::from_model(&m, false);
        let t0 = std::time::Instant::now();
        let t = Topo::new(&mesh.pos, &mesh.tris, &mesh.edges);
        let mut d = EditData::fresh(&t);
        if std::env::var("ARYADNE_UV_SMART").is_ok() {
            d.seams = smart_seams(&t, 66.0, 4);
            unwrap(&t, &mut d);
        } else {
            let md: f32 = std::env::var("ARYADNE_UV_DIST").ok().and_then(|v| v.parse().ok()).unwrap_or(0.12);
            auto_unwrap(&t, &mut d, &AutoOptions { max_distortion: md, ..AutoOptions::default() }, &PackOptions::default());
        }
        let nc = d.chart.iter().max().unwrap() + 1;
        // Distorsion : écart d'aire relative par face
        let mut worst = 0.0f32;
        let total3: f32 = (0..t.polys.len()).map(|f| t.area(f)).sum();
        let total2 = fill_rate(&d);
        for f in 0..t.polys.len() {
            let a3 = t.area(f) / total3;
            let a2 = uv_area(&d, &[f]) / total2;
            if a3 > 1e-6 {
                worst = worst.max((a2 / a3).max(a3 / a2.max(1e-9)));
            }
        }
        println!("{} polys, {} îlots, remplissage {:.2}, pire étirement d'aire ×{:.2}, {:?}", t.polys.len(), nc, total2, worst, t0.elapsed());
        assert!(d.uv.iter().flatten().all(|p| p[0].is_finite()));
    }
}


// ---------- coutures et UV automatiques avancés ----------
//
// 1. Îlots de départ : arêtes très vives (et frontières de matériaux).
// 2. Chaque îlot devient un disque avec une coupe minimale (arbre / co-arbre) : un cylindre
//    reçoit une seule couture, un tore deux, au lieu d'être découpé en bandes.
//    Les coupes passent de préférence par les arêtes vives, concaves et tournées vers le bas.
// 3. Dépliage LSCM, mesure de la distorsion ; un îlot trop étiré est coupé en deux
//    (croissance depuis deux faces éloignées, frontière le long des plis), et on recommence.
// 4. Les îlots voisins sont fusionnés quand le résultat reste peu déformé.

#[derive(Clone, Copy)]
pub struct AutoOptions {
    /// Angle au-delà duquel une arête est toujours une couture
    pub hard_angle: f32,
    /// Distorsion tolérée (0,05 : très peu ; 0,4 : beaucoup)
    pub max_distortion: f32,
    /// Séparer les matériaux
    pub split_materials: bool,
    /// Préférer les coutures cachées (dessous, creux)
    pub hide_seams: bool,
}

impl Default for AutoOptions {
    fn default() -> Self {
        AutoOptions { hard_angle: 75.0, max_distortion: 0.2, split_materials: true, hide_seams: true }
    }
}

/// Coût de couper une arête (bas = bon endroit pour une couture).
fn cut_cost(t: &Topo, e: (u32, u32), o: &AutoOptions) -> f32 {
    let l = len(sub(t.pos[e.0 as usize], t.pos[e.1 as usize])).max(1e-9);
    let Some(fs) = t.edge_faces.get(&e) else { return l };
    if fs.len() != 2 {
        return l * 0.1;
    }
    let (n0, n1) = (t.normal(fs[0] as usize), t.normal(fs[1] as usize));
    let ang = dot(n0, n1).clamp(-1.0, 1.0).acos(); // radians
    let mut f = 1.0 / (1.0 + 6.0 * (ang / std::f32::consts::FRAC_PI_2).powi(2));
    // Pli concave (creux) : couture moins visible
    let c0 = t.center(fs[0] as usize);
    let c1 = t.center(fs[1] as usize);
    let concave = dot(n0, sub(c1, c0)) > 0.0;
    if concave && ang > 0.05 {
        f *= 0.6;
    }
    if o.hide_seams {
        let ny = (n0[1] + n1[1]) * 0.5;
        if ny < -0.2 {
            f *= 0.75; // dessous de l'objet
        }
    }
    l * f
}

/// Arêtes intérieures (non coupées) d'un ensemble de faces.
fn interior_edges(t: &Topo, faces: &[usize], seams: &BTreeSet<(u32, u32)>, inset: &std::collections::HashSet<usize>) -> Vec<((u32, u32), usize, usize)> {
    let mut out = vec![];
    let mut seen = std::collections::HashSet::new();
    for &f in faces {
        let p = &t.polys[f];
        for i in 0..p.len() {
            let e = key(p[i], p[(i + 1) % p.len()]);
            if seams.contains(&e) || !seen.insert(e) {
                continue;
            }
            if let Some(fs) = t.edge_faces.get(&e) {
                if fs.len() == 2 && inset.contains(&(fs[0] as usize)) && inset.contains(&(fs[1] as usize)) {
                    out.push((e, fs[0] as usize, fs[1] as usize));
                }
            }
        }
    }
    out
}

/// Coupe minimale qui rend un îlot homéomorphe à un disque : arbre des plus courts chemins
/// (depuis le bord, au coût de couture), puis co-arbre dans le dual ; chaque arête restante
/// donne une coupe = cette arête + les deux chemins de l'arbre jusqu'au bord. Les coupes sont
/// donc courtes et droites (une seule couture rectiligne pour un cylindre).
fn disk_cut(t: &Topo, faces: &[usize], seams: &BTreeSet<(u32, u32)>, o: &AutoOptions) -> Vec<(u32, u32)> {
    let inset: std::collections::HashSet<usize> = faces.iter().copied().collect();
    let inner = interior_edges(t, faces, seams, &inset);
    // Bord de l'îlot
    let mut boundary_v: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut has_boundary = false;
    for &f in faces {
        let p = &t.polys[f];
        for i in 0..p.len() {
            let e = key(p[i], p[(i + 1) % p.len()]);
            let inside = t.edge_faces.get(&e).map(|fs| fs.iter().filter(|g| inset.contains(&(**g as usize))).count()).unwrap_or(0);
            if seams.contains(&e) || inside < 2 {
                boundary_v.insert(e.0);
                boundary_v.insert(e.1);
                has_boundary = true;
            }
        }
    }
    let mut adj: HashMap<u32, Vec<(u32, f32, (u32, u32))>> = HashMap::new();
    for (e, _, _) in &inner {
        let c = cut_cost(t, *e, o);
        adj.entry(e.0).or_default().push((e.1, c, *e));
        adj.entry(e.1).or_default().push((e.0, c, *e));
    }
    if !has_boundary {
        // Sphère : une fente au plus bas coût ; tore fermé : la racine est un sommet
        let Some(&root) = adj.keys().min() else { return vec![] };
        boundary_v.insert(root);
    }
    // Arbre des plus courts chemins (Dijkstra multi-source depuis le bord)
    #[derive(PartialEq)]
    struct N(f32, u32);
    impl Eq for N {}
    impl PartialOrd for N {
        fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(o))
        }
    }
    impl Ord for N {
        fn cmp(&self, o: &Self) -> std::cmp::Ordering {
            o.0.partial_cmp(&self.0).unwrap_or(std::cmp::Ordering::Equal)
        }
    }
    let mut dist: HashMap<u32, f32> = HashMap::new();
    let mut parent: HashMap<u32, (u32, (u32, u32))> = HashMap::new();
    let mut heap = std::collections::BinaryHeap::new();
    for &v in &boundary_v {
        dist.insert(v, 0.0);
        heap.push(N(0.0, v));
    }
    while let Some(N(d, v)) = heap.pop() {
        if d > *dist.get(&v).unwrap_or(&f32::MAX) {
            continue;
        }
        for (w, c, e) in adj.get(&v).cloned().unwrap_or_default() {
            if boundary_v.contains(&w) {
                continue;
            }
            let nd = d + c;
            if nd < *dist.get(&w).unwrap_or(&f32::MAX) {
                dist.insert(w, nd);
                parent.insert(w, (v, e));
                heap.push(N(nd, w));
            }
        }
    }
    let tree: std::collections::HashSet<(u32, u32)> = parent.values().map(|p| p.1).collect();
    // Co-arbre : arbre couvrant des faces par les arêtes hors de l'arbre primal
    let idx: HashMap<usize, usize> = faces.iter().enumerate().map(|(i, f)| (*f, i)).collect();
    let mut uf: Vec<usize> = (0..faces.len()).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let mut left: Vec<(u32, u32)> = vec![];
    let mut cand: Vec<&((u32, u32), usize, usize)> = inner.iter().filter(|(e, _, _)| !tree.contains(e)).collect();
    // Le co-arbre garde d'abord les arêtes coûteuses à couper
    cand.sort_by(|a, b| cut_cost(t, b.0, o).partial_cmp(&cut_cost(t, a.0, o)).unwrap_or(std::cmp::Ordering::Equal));
    for (e, a, b) in cand {
        let (ra, rb) = (find(&mut uf, idx[a]), find(&mut uf, idx[b]));
        if ra != rb {
            uf[ra] = rb;
        } else {
            left.push(*e);
        }
    }
    // Boucles de bord (pour reconnaître une coupe qui relie deux bords différents)
    let mut loop_of: HashMap<u32, u32> = HashMap::new();
    {
        let bv: Vec<u32> = boundary_v.iter().copied().collect();
        let bidx: HashMap<u32, usize> = bv.iter().enumerate().map(|(i, v)| (*v, i)).collect();
        let mut lp: Vec<usize> = (0..bv.len()).collect();
        for &f in faces {
            let p = &t.polys[f];
            for i in 0..p.len() {
                let e = key(p[i], p[(i + 1) % p.len()]);
                let inside = t.edge_faces.get(&e).map(|fs| fs.iter().filter(|g| inset.contains(&(**g as usize))).count()).unwrap_or(0);
                if (seams.contains(&e) || inside < 2) && bidx.contains_key(&e.0) && bidx.contains_key(&e.1) {
                    let (a, b) = (find(&mut lp, bidx[&e.0]), find(&mut lp, bidx[&e.1]));
                    lp[a] = b;
                }
            }
        }
        for (i, v) in bv.iter().enumerate() {
            loop_of.insert(*v, find(&mut lp, i) as u32);
        }
    }
    let root_of = |mut v: u32, out: &mut Vec<(u32, u32)>| -> u32 {
        let mut guard = 0;
        while let Some((u, pe)) = parent.get(&v) {
            out.push(*pe);
            v = *u;
            guard += 1;
            if guard > 1_000_000 {
                break;
            }
        }
        v
    };
    // Coupes : arête restante + chemins jusqu'au bord
    let mut cut: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    for e in left {
        let (mut pa, mut pb) = (vec![], vec![]);
        let ra = root_of(e.0, &mut pa);
        let rb = root_of(e.1, &mut pb);
        let (la, lb) = (loop_of.get(&ra).copied(), loop_of.get(&rb).copied());
        if has_boundary && la.is_some() && lb.is_some() && la != lb {
            // Deux bords différents : le plus court chemin de l'un à l'autre (coupe droite)
            let src: Vec<u32> = loop_of.iter().filter(|(_, l)| Some(**l) == la).map(|(v, _)| *v).collect();
            let mut dist2: HashMap<u32, f32> = HashMap::new();
            let mut par2: HashMap<u32, (u32, (u32, u32))> = HashMap::new();
            let mut heap2 = std::collections::BinaryHeap::new();
            for v in src {
                dist2.insert(v, 0.0);
                heap2.push(N(0.0, v));
            }
            let mut hit = None;
            while let Some(N(d, v)) = heap2.pop() {
                if d > *dist2.get(&v).unwrap_or(&f32::MAX) {
                    continue;
                }
                if loop_of.get(&v).copied() == lb {
                    hit = Some(v);
                    break;
                }
                for (w, c, ed) in adj.get(&v).cloned().unwrap_or_default() {
                    if loop_of.get(&w).is_some_and(|l| Some(*l) == la) {
                        continue;
                    }
                    let nd = d + c;
                    if nd < *dist2.get(&w).unwrap_or(&f32::MAX) {
                        dist2.insert(w, nd);
                        par2.insert(w, (v, ed));
                        heap2.push(N(nd, w));
                    }
                }
            }
            if let Some(mut v) = hit {
                while let Some((u, ed)) = par2.get(&v) {
                    cut.insert(*ed);
                    v = *u;
                }
                continue;
            }
        }
        cut.insert(e);
        cut.extend(pa);
        cut.extend(pb);
    }
    let mut out: Vec<(u32, u32)> = cut.into_iter().collect();
    if !has_boundary && out.is_empty() {
        out = cheapest_path_cut(t, faces, seams, o);
    }
    out
}

/// Fente entre deux sommets éloignés, au coût de couture minimal (Dijkstra).
fn cheapest_path_cut(t: &Topo, faces: &[usize], seams: &BTreeSet<(u32, u32)>, o: &AutoOptions) -> Vec<(u32, u32)> {
    let inset: std::collections::HashSet<usize> = faces.iter().copied().collect();
    let edges = interior_edges(t, faces, seams, &inset);
    let mut adj: HashMap<u32, Vec<(u32, f32, (u32, u32))>> = HashMap::new();
    for (e, _, _) in &edges {
        let c = cut_cost(t, *e, o);
        adj.entry(e.0).or_default().push((e.1, c, *e));
        adj.entry(e.1).or_default().push((e.0, c, *e));
    }
    let verts: Vec<u32> = adj.keys().copied().collect();
    if verts.len() < 2 {
        return vec![];
    }
    let c: V3 = mul(verts.iter().fold([0.0; 3], |s, v| add(s, t.pos[*v as usize])), 1.0 / verts.len() as f32);
    let a = *verts.iter().max_by(|x, y| len(sub(t.pos[**x as usize], c)).partial_cmp(&len(sub(t.pos[**y as usize], c))).unwrap()).unwrap();
    let b = *verts.iter().max_by(|x, y| len(sub(t.pos[**x as usize], t.pos[a as usize])).partial_cmp(&len(sub(t.pos[**y as usize], t.pos[a as usize]))).unwrap()).unwrap();
    // Dijkstra
    let mut dist: HashMap<u32, f32> = HashMap::new();
    let mut prev: HashMap<u32, (u32, (u32, u32))> = HashMap::new();
    let mut heap = std::collections::BinaryHeap::new();
    #[derive(PartialEq)]
    struct N(f32, u32);
    impl Eq for N {}
    impl PartialOrd for N {
        fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(o))
        }
    }
    impl Ord for N {
        fn cmp(&self, o: &Self) -> std::cmp::Ordering {
            o.0.partial_cmp(&self.0).unwrap_or(std::cmp::Ordering::Equal)
        }
    }
    dist.insert(a, 0.0);
    heap.push(N(0.0, a));
    while let Some(N(d, v)) = heap.pop() {
        if v == b {
            break;
        }
        if d > *dist.get(&v).unwrap_or(&f32::MAX) {
            continue;
        }
        for (w, c, e) in adj.get(&v).cloned().unwrap_or_default() {
            let nd = d + c;
            if nd < *dist.get(&w).unwrap_or(&f32::MAX) {
                dist.insert(w, nd);
                prev.insert(w, (v, e));
                heap.push(N(nd, w));
            }
        }
    }
    let mut out = vec![];
    let mut v = b;
    while let Some((u, e)) = prev.get(&v) {
        out.push(*e);
        v = *u;
        if v == a {
            break;
        }
    }
    out
}

/// Distorsion d'un îlot déplié : écart moyen à la conformité et à l'aire, faces retournées.
fn chart_distortion(t: &Topo, faces: &[usize], uv: &HashMap<(usize, usize), V2>) -> f32 {
    let mut a3 = 0.0f32;
    let mut a2 = 0.0f32;
    let mut tri: Vec<(f32, f32, f32, f32)> = vec![]; // (aire 3D, σ1, σ2, det signe)
    for &f in faces {
        let p = &t.polys[f];
        for k in 1..p.len() - 1 {
            let (x0, x1, x2) = (t.pos[p[0] as usize], t.pos[p[k] as usize], t.pos[p[k + 1] as usize]);
            let (Some(u0), Some(u1), Some(u2)) = (uv.get(&(f, 0)), uv.get(&(f, k)), uv.get(&(f, k + 1))) else { continue };
            let e1 = sub(x1, x0);
            let e2 = sub(x2, x0);
            let n = cross(e1, e2);
            let area = len(n) * 0.5;
            if area < 1e-14 {
                continue;
            }
            let ex = norm(e1);
            let ey = norm(cross(norm(n), ex));
            // X = [[e1.x, e2.x], [e1.y, e2.y]] en local, U = [[du1, du2], [dv1, dv2]]
            let (a, b, c, d) = (dot(e1, ex), dot(e2, ex), dot(e1, ey), dot(e2, ey));
            let det_x = a * d - b * c;
            if det_x.abs() < 1e-14 {
                continue;
            }
            let (p1, p2, q1, q2) = (u1[0] - u0[0], u2[0] - u0[0], u1[1] - u0[1], u2[1] - u0[1]);
            // J = U X^-1
            let inv = [d / det_x, -b / det_x, -c / det_x, a / det_x];
            let j = [p1 * inv[0] + p2 * inv[2], p1 * inv[1] + p2 * inv[3], q1 * inv[0] + q2 * inv[2], q1 * inv[1] + q2 * inv[3]];
            let det_j = j[0] * j[3] - j[1] * j[2];
            let fro = j.iter().map(|x| x * x).sum::<f32>();
            let disc = (fro * fro - 4.0 * det_j * det_j).max(0.0).sqrt();
            let s1 = ((fro + disc) * 0.5).max(0.0).sqrt();
            let s2 = ((fro - disc) * 0.5).max(0.0).sqrt();
            a3 += area;
            a2 += det_j.abs() * area;
            tri.push((area, s1, s2, det_j.signum()));
        }
    }
    if a3 <= 0.0 || tri.is_empty() {
        return 0.0;
    }
    let scale = (a2 / a3).sqrt().max(1e-12);
    let mut conf = 0.0;
    let mut area_err = 0.0;
    let mut flipped = 0.0;
    for (area, s1, s2, sign) in &tri {
        let w = area / a3;
        conf += w * (s1 / s2.max(1e-9) - 1.0).min(10.0);
        let r = s1 * s2 / (scale * scale);
        area_err += w * (r.max(1e-6).ln().abs()).min(4.0);
        if *sign < 0.0 {
            flipped += w;
        }
    }
    conf + 0.5 * area_err + 10.0 * flipped
}

/// Déplie un îlot (avec ses fentes) et donne sa distorsion.
fn try_chart(t: &Topo, faces: &[usize], seams: &BTreeSet<(u32, u32)>) -> (f32, HashMap<(usize, usize), V2>) {
    let (corners, cvert) = chart_vertices(t, faces, seams);
    let u = lscm(t, faces, &corners, &cvert);
    let map: HashMap<(usize, usize), V2> = corners.iter().enumerate().map(|(k, c)| (*c, u[cvert[k]])).collect();
    (chart_distortion(t, faces, &map), map)
}

/// Coupe un îlot en deux : deux faces éloignées, croissance par distance pondérée par les plis.
fn split_chart(t: &Topo, faces: &[usize], seams: &BTreeSet<(u32, u32)>) -> Vec<(u32, u32)> {
    let inset: std::collections::HashSet<usize> = faces.iter().copied().collect();
    let c = mul(faces.iter().fold([0.0; 3], |s, f| add(s, t.center(*f))), 1.0 / faces.len() as f32);
    let far = |from: V3| *faces.iter().max_by(|a, b| len(sub(t.center(**a), from)).partial_cmp(&len(sub(t.center(**b), from))).unwrap()).unwrap();
    let sa = far(c);
    let sb = far(t.center(sa));
    if sa == sb {
        return vec![];
    }
    let mut owner: HashMap<usize, (u8, f32)> = HashMap::new();
    let mut heap = std::collections::BinaryHeap::new();
    #[derive(PartialEq)]
    struct N(f32, usize, u8);
    impl Eq for N {}
    impl PartialOrd for N {
        fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(o))
        }
    }
    impl Ord for N {
        fn cmp(&self, o: &Self) -> std::cmp::Ordering {
            o.0.partial_cmp(&self.0).unwrap_or(std::cmp::Ordering::Equal)
        }
    }
    heap.push(N(0.0, sa, 0));
    heap.push(N(0.0, sb, 1));
    while let Some(N(d, f, g)) = heap.pop() {
        if owner.contains_key(&f) {
            continue;
        }
        owner.insert(f, (g, d));
        let p = &t.polys[f];
        let nf = t.normal(f);
        for i in 0..p.len() {
            let e = key(p[i], p[(i + 1) % p.len()]);
            if seams.contains(&e) {
                continue;
            }
            if let Some(fs) = t.edge_faces.get(&e) {
                for &h in fs {
                    let h = h as usize;
                    if h != f && inset.contains(&h) && !owner.contains_key(&h) {
                        let bend = 1.0 - dot(nf, t.normal(h));
                        let w = len(sub(t.center(h), t.center(f))) * (1.0 + 8.0 * bend);
                        heap.push(N(d + w, h, g));
                    }
                }
            }
        }
    }
    let mut out = vec![];
    for (e, fs) in &t.edge_faces {
        if fs.len() == 2 {
            let (a, b) = (fs[0] as usize, fs[1] as usize);
            if let (Some(x), Some(y)) = (owner.get(&a), owner.get(&b)) {
                if x.0 != y.0 && !seams.contains(e) {
                    out.push(*e);
                }
            }
        }
    }
    out
}

/// Coutures automatiques avancées. `progress` reçoit des messages d'étape.
pub fn auto_seams(t: &Topo, d: &EditData, o: &AutoOptions) -> BTreeSet<(u32, u32)> {
    // 1. Départ : arêtes très vives et frontières de matériaux
    let mut seams: BTreeSet<(u32, u32)> = seams_by_angle(t, o.hard_angle);
    if o.split_materials && d.face_mat.len() == t.polys.len() {
        for (e, fs) in &t.edge_faces {
            if fs.len() == 2 && d.face_mat[fs[0] as usize] != d.face_mat[fs[1] as usize] {
                seams.insert(*e);
            }
        }
    }
    // 2-3. Disque + distorsion, îlot par îlot (avec une pile de travail)
    let ch = charts(t, &seams);
    let nc = ch.iter().copied().max().map(|m| m as usize + 1).unwrap_or(0);
    let mut work: Vec<(Vec<usize>, u8)> = vec![vec![]; nc].into_iter().map(|v| (v, 0u8)).collect();
    for (f, &c) in ch.iter().enumerate() {
        work[c as usize].0.push(f);
    }
    let mut done: Vec<Vec<usize>> = vec![];
    while let Some((faces, depth)) = work.pop() {
        if faces.is_empty() {
            continue;
        }
        for e in disk_cut(t, &faces, &seams, o) {
            seams.insert(e);
        }
        if faces.len() < 3 || depth >= 10 {
            done.push(faces);
            continue;
        }
        let (dist, _) = try_chart(t, &faces, &seams);
        if dist <= o.max_distortion {
            done.push(faces);
            continue;
        }
        let cut = split_chart(t, &faces, &seams);
        if cut.is_empty() {
            done.push(faces);
            continue;
        }
        for e in cut {
            seams.insert(e);
        }
        // Les deux moitiés (ou plus) repartent dans la pile
        let inset: std::collections::HashSet<usize> = faces.iter().copied().collect();
        let sub_ch = charts(t, &seams);
        let mut groups: HashMap<u32, Vec<usize>> = HashMap::new();
        for &f in &faces {
            groups.entry(sub_ch[f]).or_default().push(f);
        }
        let _ = inset;
        for (_, g) in groups {
            work.push((g, depth + 1));
        }
    }
    // 4. Fusion des îlots voisins quand le résultat reste peu déformé
    for _round in 0..2 {
        let ch = charts(t, &seams);
        let nc = ch.iter().copied().max().map(|m| m as usize + 1).unwrap_or(0);
        if nc < 2 || nc > 400 {
            break;
        }
        let mut shared: HashMap<(u32, u32), (f32, Vec<(u32, u32)>)> = HashMap::new();
        for (e, fs) in &t.edge_faces {
            if fs.len() == 2 && seams.contains(e) {
                let (a, b) = (ch[fs[0] as usize], ch[fs[1] as usize]);
                if a != b {
                    // Pas de fusion à travers une arête très vive ni entre matériaux
                    if t.dihedral(*e).is_some_and(|x| x > o.hard_angle) {
                        continue;
                    }
                    if o.split_materials && d.face_mat.len() == t.polys.len() && d.face_mat[fs[0] as usize] != d.face_mat[fs[1] as usize] {
                        continue;
                    }
                    let k = key(a, b);
                    let ent = shared.entry(k).or_insert((0.0, vec![]));
                    ent.0 += len(sub(t.pos[e.0 as usize], t.pos[e.1 as usize]));
                    ent.1.push(*e);
                }
            }
        }
        let mut pairs: Vec<((u32, u32), (f32, Vec<(u32, u32)>))> = shared.into_iter().collect();
        pairs.sort_by(|a, b| b.1 .0.partial_cmp(&a.1 .0).unwrap_or(std::cmp::Ordering::Equal));
        let mut merged_any = false;
        let mut used = std::collections::HashSet::new();
        for ((a, b), (_, edges)) in pairs.into_iter().take(120) {
            if used.contains(&a) || used.contains(&b) {
                continue;
            }
            let faces: Vec<usize> = (0..t.polys.len()).filter(|f| ch[*f] == a || ch[*f] == b).collect();
            let mut trial = seams.clone();
            for e in &edges {
                trial.remove(e);
            }
            for e in disk_cut(t, &faces, &trial, o) {
                trial.insert(e);
            }
            let (dist, _) = try_chart(t, &faces, &trial);
            if dist <= o.max_distortion * 0.85 {
                seams = trial;
                used.insert(a);
                used.insert(b);
                merged_any = true;
            }
        }
        if !merged_any {
            break;
        }
    }
    seams
}

/// Oriente chaque îlot sur ses arêtes dominantes (les quads deviennent droits),
/// sinon sur la boîte la plus petite.
fn orient_charts(d: &mut EditData) {
    let groups = chart_faces(d);
    for faces in &groups {
        let mut hist = [0.0f32; 90];
        let mut total = 0.0;
        for &f in faces {
            let p = &d.uv[f];
            for i in 0..p.len() {
                let (a, b) = (p[i], p[(i + 1) % p.len()]);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let l = (dx * dx + dy * dy).sqrt();
                if l < 1e-12 {
                    continue;
                }
                let ang = dy.atan2(dx).to_degrees().rem_euclid(90.0);
                hist[(ang as usize).min(89)] += l;
                total += l;
            }
        }
        if total <= 0.0 {
            continue;
        }
        // Lissage circulaire
        let smooth: Vec<f32> = (0..90).map(|i| hist[(i + 89) % 90] * 0.5 + hist[i] + hist[(i + 1) % 90] * 0.5).collect();
        let (peak, val) = smooth.iter().enumerate().fold((0, 0.0f32), |a, (i, v)| if *v > a.1 { (i, *v) } else { a });
        let strong = val > 2.5 * (total * 2.0 / 90.0);
        if !strong {
            continue; // le pack choisira la boîte minimale
        }
        let a = -(peak as f32 + 0.5).to_radians();
        let (s, c) = a.sin_cos();
        for &f in faces {
            for q in &mut d.uv[f] {
                *q = [q[0] * c - q[1] * s, q[0] * s + q[1] * c];
            }
        }
    }
}

/// Dépliage complet automatique : coutures avancées, LSCM, orientation, rangement.
#[allow(dead_code)]
pub fn auto_unwrap(t: &Topo, d: &mut EditData, o: &AutoOptions, pack_opt: &PackOptions) {
    d.seams = auto_seams(t, d, o);
    unwrap_charts(t, d);
    orient_charts(d);
    pack(t, d, &PackOptions { rotate: false, ..*pack_opt });
}

// ---------- état d'édition et interface ----------

use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, RichText, Sense, Stroke};
use std::path::PathBuf;

#[path = "uvtools.rs"]
pub mod tools;

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Faces,
    Seams,
    Guides,
}

/// Retopologie qui tourne en arrière-plan
pub struct RetopoJob {
    pub progress: std::sync::Arc<std::sync::Mutex<f32>>,
    pub rx: std::sync::mpsc::Receiver<Option<crate::retopo::Result>>,
    pub started: std::time::Instant,
}

pub struct Edit {
    pub topo: Topo,
    pub data: EditData,
    pub sel: BTreeSet<u32>,
    pub tool: Tool,
    pub tab: u8,
    pub version: u64,
    pub seam_angle: f32,
    pub smart_angle: f32,
    pub min_faces: usize,
    pub optimize: bool,
    pub rotate: bool,
    pub density_target: f32,
    pub file: PathBuf,
    /// Taille réelle (plus grand côté) de l'objet, pour la densité
    pub scale: f32,
    pub message: Option<String>,
    pub owner: Vec<u32>,
    pub checker: bool,
    pub auto: AutoOptions,
    pub retopo_opt: crate::retopo::Options,
    pub retopo: Option<std::sync::Arc<crate::retopo::Result>>,
    pub retopo_job: Option<RetopoJob>,
    /// Demande à la vue d'exporter le résultat (elle connaît la transformation vers le monde)
    pub retopo_export: bool,
    pub retopo_show: bool,
    /// Trait de guide en cours
    pub guide_now: Vec<V3>,
    /// ID dont on veut créer ou ouvrir le matériau en nodes (traité par la vue 3D)
    pub mat_request: Option<usize>,
    /// ID de matériau de chaque triangle affiché
    pub tri_mat: Vec<u16>,
    /// Demande de bake des textures (traitée par la vue 3D, qui connaît les matériaux)
    pub bake_request: bool,
    pub bake_size: u32,
    /// Rayons par texel pour l'occlusion et l'épaisseur (0 : pas de calcul)
    pub bake_rays: u32,
    /// Modèle haute définition dont on reporte les détails dans la normale (chemin, vide si aucun)
    pub bake_high: String,
    /// Annuler / rétablir : l'état enregistré avant chaque action terminée
    pending: std::cell::Cell<bool>,
    /// Outils façon Blender (arêtes, éditeur UV, menus) : voir uvtools.rs
    pub tools: tools::State,
    last: Option<EditData>,
    undo: Vec<EditData>,
    redo: Vec<EditData>,
}

impl Edit {
    /// Une action vient de finir (souris relâchée) : l'état d'avant entre dans l'historique.
    pub fn track(&mut self, ui: &egui::Ui) {
        if self.last.is_none() {
            self.last = Some(self.data.clone());
        }
        if self.pending.get() && !ui.input(|i| i.pointer.any_down()) {
            self.pending.set(false);
            if let Some(prev) = self.last.replace(self.data.clone()) {
                self.undo.push(prev);
                if self.undo.len() > 40 {
                    self.undo.remove(0);
                }
                self.redo.clear();
            }
        }
        let typing = ui.ctx().egui_wants_keyboard_input();
        let (z, y) = ui.input(|i| (i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::Z), i.modifiers.command && (i.modifiers.shift && i.key_pressed(egui::Key::Z) || i.key_pressed(egui::Key::Y))));
        if !typing && (z || y) {
            self.step(y);
        }
    }

    /// Revient un pas en arrière (ou en avant avec `forward`).
    pub fn step(&mut self, forward: bool) {
        let (from, to) = if forward { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        if let Some(d) = from.pop() {
            to.push(std::mem::replace(&mut self.data, d));
            self.last = Some(self.data.clone());
            self.pending.set(false);
            self.version += 1;
            self.write();
        }
    }

    pub fn open(mesh: &crate::modifiers::Mesh, file: PathBuf, scale: f32) -> Edit {
        let topo = Topo::new(&mesh.pos, &mesh.tris, &mesh.edges);
        let data = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<EditData>(&t).ok()).filter(|d| d.fits(&topo)).unwrap_or_else(|| EditData::fresh(&topo));
        let n = topo.polys.len();
        let checker = data.has_uv();
        Edit { topo, data, sel: BTreeSet::new(), tool: Tool::Faces, tab: 0, version: 1, seam_angle: 45.0, smart_angle: 66.0, min_faces: (n / 400).clamp(4, 300), optimize: true, rotate: true, density_target: 1024.0, file, scale, message: None, owner: vec![], checker, auto: AutoOptions::default(), retopo_opt: crate::retopo::Options { faces: (n * 2).clamp(200, 8000) / 100 * 100, ..Default::default() }, retopo: None, retopo_job: None, retopo_export: false, retopo_show: true, guide_now: vec![], mat_request: None, tri_mat: vec![], bake_request: false, bake_size: 1024, bake_rays: 24, bake_high: String::new(), pending: Default::default(), tools: tools::State::new(), last: None, undo: vec![], redo: vec![] }
    }

    pub fn save(&self) {
        self.pending.set(true);
        self.write();
    }

    fn write(&self) {
        if let Some(d) = self.file.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = crate::vault::atomic_write(&self.file, serde_json::to_string(&self.data).unwrap_or_default());
    }

    fn changed(&mut self) {
        self.version += 1;
        self.save();
    }

    /// Maillage d'affichage : couleur du matériau de chaque face, sélection en surbrillance.
    pub fn display(&mut self, accent: Color32) -> (crate::modifiers::Mesh, Vec<[V2; 3]>) {
        let (tris, owner, edges, uvs) = triangulate(&self.topo, &self.data);
        let colors = owner
            .iter()
            .map(|&f| {
                let m = self.data.face_mat.get(f as usize).copied().unwrap_or(0) as usize;
                let c = self.data.slots.get(m).map(|s| s.color).unwrap_or(PALETTE[0]);
                if self.sel.contains(&f) {
                    Color32::from_rgb(((c[0] as u16 + accent.r() as u16 * 2) / 3) as u8, ((c[1] as u16 + accent.g() as u16 * 2) / 3) as u8, ((c[2] as u16 + accent.b() as u16 * 2) / 3) as u8)
                } else {
                    Color32::from_rgb(c[0], c[1], c[2])
                }
            })
            .collect();
        self.tri_mat = owner.iter().map(|&f| self.data.face_mat.get(f as usize).copied().unwrap_or(0)).collect();
        self.owner = owner;
        (crate::modifiers::Mesh { pos: self.topo.pos.clone(), tris, colors, edges }, uvs)
    }

    /// Clic dans la vue 3D sur le triangle `tri` (points en espace d'affichage).
    pub fn click(&mut self, prep: &crate::model3d::Prepared, tri: usize, bary: [f32; 3], shift: bool, ctrl: bool, alt: bool) {
        let Some(&f) = self.owner.get(tri) else { return };
        match self.tool {
            Tool::Guides => {}
            Tool::Faces => {
                if alt {
                    // Alt+clic : l'îlot entier (faces reliées jusqu'aux coutures)
                    if !shift {
                        self.sel.clear();
                    }
                    self.sel.insert(f);
                    self.select_linked();
                } else if ctrl {
                    self.sel.remove(&f);
                } else if shift {
                    if !self.sel.remove(&f) {
                        self.sel.insert(f);
                    }
                } else {
                    self.sel = [f].into_iter().collect();
                }
            }
            Tool::Seams => {
                if let Some(e) = self.edge_at(prep, tri, bary) {
                    self.click_edge(e, shift, ctrl, alt);
                }
            }
        }
        self.version += 1;
    }

    /// Faces reliées à la sélection (même îlot de coutures).
    fn select_linked(&mut self) {
        let ch = charts(&self.topo, &self.data.seams);
        let wanted: BTreeSet<u32> = self.sel.iter().map(|&f| ch[f as usize]).collect();
        self.sel = (0..self.topo.polys.len() as u32).filter(|f| wanted.contains(&ch[*f as usize])).collect();
    }

    /// Faces voisines dont la normale est proche (« faces semblables »).
    fn grow(&mut self, angle: f32) {
        let cos = angle.to_radians().cos();
        let mut add_ = vec![];
        for &f in &self.sel {
            let n = self.topo.normal(f as usize);
            let p = &self.topo.polys[f as usize];
            for i in 0..p.len() {
                if let Some(fs) = self.topo.edge_faces.get(&key(p[i], p[(i + 1) % p.len()])) {
                    for &g in fs {
                        if !self.sel.contains(&g) && dot(n, self.topo.normal(g as usize)) >= cos {
                            add_.push(g);
                        }
                    }
                }
            }
        }
        self.sel.extend(add_);
    }

    /// Panneau de droite. Retourne vrai si l'affichage doit être recalculé.
    pub fn panel(&mut self, ui: &mut egui::Ui) -> bool {
        let p = crate::theme::pal();
        self.track(ui);
        let v0 = self.version;
        ui.horizontal(|ui| {
            for (k, l) in ["matériaux", "coutures", "UV", "retopo"].iter().enumerate() {
                if ui.selectable_label(self.tab == k as u8, crate::i18n::t(l)).clicked() {
                    self.tab = k as u8;
                    self.tool = match k {
                        1 => Tool::Seams,
                        3 => Tool::Guides,
                        _ => Tool::Faces,
                    };
                }
            }
        });
        ui.label(RichText::new(format!("{} faces · {} sélectionnée(s) · {} coutures", self.topo.polys.len(), self.sel.len(), self.data.seams.len())).color(p.muted).size(10.5));
        ui.separator();
        match self.tab {
            0 => self.tab_materials(ui),
            1 => self.tab_seams(ui),
            2 => self.tab_uv(ui),
            _ => self.tab_retopo(ui),
        }
        if let Some(m) = &self.message {
            ui.label(RichText::new(m).color(p.muted).size(10.5));
        }
        self.version != v0
    }

    fn selection_tools(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.small_button(crate::i18n::t("tout")).clicked() {
                self.sel = (0..self.topo.polys.len() as u32).collect();
                self.version += 1;
            }
            if ui.small_button(crate::i18n::t("rien")).clicked() {
                self.sel.clear();
                self.version += 1;
            }
            if ui.small_button(crate::i18n::t("inverser")).clicked() {
                self.sel = (0..self.topo.polys.len() as u32).filter(|f| !self.sel.contains(f)).collect();
                self.version += 1;
            }
            if ui.small_button(crate::i18n::t("agrandir")).on_hover_text(crate::i18n::t("faces voisines d'orientation proche")).clicked() {
                self.grow(30.0);
                self.version += 1;
            }
            if ui.small_button(crate::i18n::t("îlot")).on_hover_text(crate::i18n::t("faces reliées (jusqu'aux coutures)")).clicked() {
                self.select_linked();
                self.version += 1;
            }
        });
        ui.label(RichText::new(crate::i18n::t("clic : choisir · Maj+clic : ajouter · Ctrl+clic : retirer")).color(crate::theme::pal().muted).size(10.0));
    }

    fn tab_materials(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        self.selection_tools(ui);
        ui.add_space(6.0);
        let mut assign = None;
        let mut select = None;
        let mut remove = None;
        let counts: Vec<usize> = (0..self.data.slots.len()).map(|k| self.data.face_mat.iter().filter(|&&m| m as usize == k).count()).collect();
        let mut mat_req = None;
        for (k, s) in self.data.slots.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let mut c = Color32::from_rgb(s.color[0], s.color[1], s.color[2]);
                if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
                    s.color = [c.r(), c.g(), c.b()];
                    assign = Some(usize::MAX);
                }
                ui.label(RichText::new(format!("ID {k}")).size(10.5).color(p.muted));
                ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(90.0));
                ui.label(RichText::new(counts[k].to_string()).size(10.5).color(p.muted));
            });
            ui.horizontal(|ui| {
                ui.add_space(30.0);
                if ui.small_button(crate::i18n::t("assigner")).on_hover_text(crate::i18n::t("à la sélection")).clicked() {
                    assign = Some(k);
                }
                if ui.small_button(crate::i18n::t("choisir")).on_hover_text(crate::i18n::t("sélectionner ses faces")).clicked() {
                    select = Some(k);
                }
                let has = !s.material.is_empty();
                if ui.small_button(crate::i18n::t(if has { "nodes ✎" } else { "＋ nodes" })).on_hover_text(crate::i18n::t(if has { "ouvrir le matériau en nodes de cet ID" } else { "créer un matériau en nodes pour cet ID et l'ouvrir" })).clicked() {
                    mat_req = Some(k);
                }
                if has && ui.small_button("⌫").on_hover_text(crate::i18n::t("retirer le matériau en nodes")).clicked() {
                    s.material.clear();
                    assign = Some(usize::MAX);
                }
                if k > 0 && ui.small_button("×").clicked() {
                    remove = Some(k);
                }
            });
        }
        if mat_req.is_some() {
            self.mat_request = mat_req;
        }
        if let Some(k) = assign {
            if k != usize::MAX {
                for &f in &self.sel {
                    self.data.face_mat[f as usize] = k as u16;
                }
            }
            self.changed();
        }
        if let Some(k) = select {
            self.sel = (0..self.topo.polys.len() as u32).filter(|f| self.data.face_mat[*f as usize] as usize == k).collect();
            self.version += 1;
        }
        if let Some(k) = remove {
            self.data.slots.remove(k);
            for m in &mut self.data.face_mat {
                if *m as usize == k {
                    *m = 0;
                } else if *m as usize > k {
                    *m -= 1;
                }
            }
            self.changed();
        }
        if ui.button(crate::i18n::t("＋ matériau")).clicked() {
            let n = self.data.slots.len();
            self.data.slots.push(Slot { name: format!("matériau {}", n + 1), color: PALETTE[n % PALETTE.len()], material: String::new() });
            if !self.sel.is_empty() {
                for &f in &self.sel {
                    self.data.face_mat[f as usize] = n as u16;
                }
            }
            self.changed();
        }
        ui.label(RichText::new(crate::i18n::t("Les IDs sont exportés en OBJ (usemtl + .mtl) et en GLB (un matériau par ID).")).color(p.muted).size(10.0));
    }

    fn tab_seams(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        ui.label(RichText::new(crate::i18n::t("clic : choisir une arête · Maj : ajouter · Alt : boucle · Ctrl : chemin le plus court · clic droit ou Ctrl+E : menu")).color(p.muted).size(10.0));
        ui.horizontal_wrapped(|ui| {
            let n = self.tools.esel.len();
            ui.label(RichText::new(format!("{n} {}", crate::i18n::t("arête(s) choisie(s)"))).size(10.5));
            if ui.add_enabled(n > 0, egui::Button::new(crate::i18n::t("marquer comme couture"))).clicked() {
                self.mark_seams(true);
            }
            if ui.add_enabled(n > 0, egui::Button::new(crate::i18n::t("effacer la couture"))).clicked() {
                self.mark_seams(false);
            }
            if ui.small_button(crate::i18n::t("choisir les coutures")).clicked() {
                self.run("sel-seams");
            }
            if ui.add_enabled(self.data.has_uv(), egui::Button::new(crate::i18n::t("coutures depuis les îlots"))).clicked() {
                self.seams_from_islands();
            }
        });
        ui.add_space(4.0);
        ui.label(RichText::new(crate::i18n::t("automatique avancé")).size(11.0).strong());
        self.auto_options(ui);
        if ui.button(crate::i18n::t("coutures automatiques")).on_hover_text(crate::i18n::t("Coupe minimale pour que chaque îlot se déplie à plat (un cylindre = une couture droite), coutures dans les plis et les creux, découpe là où ça s'étire trop, puis fusion des îlots voisins")).clicked() {
            self.auto_now(false);
        }
        ui.add_space(6.0);
        ui.label(RichText::new(crate::i18n::t("autres méthodes")).size(11.0).strong());
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut self.smart_angle, 15.0..=89.0).clamping(egui::SliderClamping::Never).text(crate::i18n::t("angle")));
        });
        ui.add(egui::Slider::new(&mut self.min_faces, 1..=2000).clamping(egui::SliderClamping::Never).logarithmic(true).text(crate::i18n::t("taille min. d'îlot")));
        if ui.button(crate::i18n::t("coutures intelligentes")).on_hover_text(crate::i18n::t("Regroupe les faces en îlots presque plats (comme Smart UV Project), en partant des grandes surfaces, puis place les coutures sur leurs bords")).clicked() {
            self.data.seams = smart_seams(&self.topo, self.smart_angle, self.min_faces);
            self.message = Some(format!("{} coutures", self.data.seams.len()));
            self.changed();
        }
        ui.add_space(4.0);
        ui.add(egui::Slider::new(&mut self.seam_angle, 5.0..=150.0).clamping(egui::SliderClamping::Never).text(crate::i18n::t("angle vif")));
        ui.horizontal(|ui| {
            if ui.button(crate::i18n::t("sur les arêtes vives")).clicked() {
                self.data.seams = seams_by_angle(&self.topo, self.seam_angle);
                self.changed();
            }
            if ui.button(crate::i18n::t("ajouter les vives")).clicked() {
                self.data.seams.extend(seams_by_angle(&self.topo, self.seam_angle));
                self.changed();
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button(crate::i18n::t("contour de la sélection")).on_hover_text(crate::i18n::t("couture tout autour des faces sélectionnées")).clicked() {
                for (e, fs) in &self.topo.edge_faces {
                    let inside = fs.iter().filter(|f| self.sel.contains(f)).count();
                    if inside == 1 && fs.len() == 2 {
                        self.data.seams.insert(*e);
                    }
                }
                self.changed();
            }
            if ui.button(crate::i18n::t("tout effacer")).clicked() {
                self.data.seams.clear();
                self.changed();
            }
        });
        ui.add_space(6.0);
        if ui.button(crate::i18n::t("déplier avec ces coutures →")).clicked() {
            self.tab = 2;
            self.tool = Tool::Faces;
            self.unwrap_now();
        }
    }

    fn auto_options(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        ui.horizontal(|ui| {
            ui.label(RichText::new(crate::i18n::t("moins d'îlots")).size(10.5).color(p.muted));
            let mut v = 0.6 - self.auto.max_distortion;
            if ui.add(egui::Slider::new(&mut v, 0.0..=0.55).show_value(false)).changed() {
                self.auto.max_distortion = 0.6 - v;
            }
            ui.label(RichText::new(crate::i18n::t("moins d'étirement")).size(10.5).color(p.muted));
        });
        ui.add(egui::Slider::new(&mut self.auto.hard_angle, 30.0..=150.0).clamping(egui::SliderClamping::Never).text(crate::i18n::t("coupe toujours au-delà de (°)")));
        ui.checkbox(&mut self.auto.split_materials, crate::i18n::t("séparer les matériaux"));
        ui.checkbox(&mut self.auto.hide_seams, crate::i18n::t("cacher les coutures (creux, dessous)"));
    }

    pub(crate) fn auto_now(&mut self, then_unwrap: bool) {
        let t0 = std::time::Instant::now();
        self.data.seams = auto_seams(&self.topo, &self.data, &self.auto);
        if then_unwrap {
            self.unwrap_now();
            self.message = Some(format!("{} ({:.1} s en tout)", self.message.clone().unwrap_or_default(), t0.elapsed().as_secs_f32()));
        } else {
            let n = charts(&self.topo, &self.data.seams).iter().max().map(|m| m + 1).unwrap_or(0);
            self.message = Some(format!("{} coutures, {n} îlots, {:.1} s", self.data.seams.len(), t0.elapsed().as_secs_f32()));
            self.changed();
        }
    }

    fn unwrap_now(&mut self) {
        let t0 = std::time::Instant::now();
        unwrap_charts(&self.topo, &mut self.data);
        orient_charts(&mut self.data);
        pack(&self.topo, &mut self.data, &PackOptions { optimize: self.optimize, rotate: self.rotate, fixed_scale: None });
        self.checker = true;
        let n = self.data.chart.iter().max().map(|m| m + 1).unwrap_or(0);
        self.message = Some(format!("{n} îlots, remplissage {:.0} %, {:.1} s", fill_rate(&self.data) * 100.0, t0.elapsed().as_secs_f32()));
        self.changed();
    }

    /// Ajoute un point au guide en cours (glisser sur le modèle).
    pub fn guide_point(&mut self, prep: &crate::model3d::Prepared, tri: usize, bary: [f32; 3]) {
        let t = prep.triangle(tri);
        let p: Vec<V3> = t.iter().map(|&i| prep.vertex(i as usize)).collect();
        let hit = [0, 1, 2].map(|k| p[0][k] * bary[0] + p[1][k] * bary[1] + p[2][k] * bary[2]);
        // Pinceau de densité : des touches espacées d'un demi-rayon le long du trait
        if self.tools.density_brush {
            let (r, k) = (self.tools.brush_radius, self.tools.brush_factor);
            let far = self.data.density.last().is_none_or(|d| ((d[0] - hit[0]).powi(2) + (d[1] - hit[1]).powi(2) + (d[2] - hit[2]).powi(2)).sqrt() > r * 0.5);
            if far {
                self.data.density.push([hit[0], hit[1], hit[2], r, k]);
                self.version += 1;
            }
            return;
        }
        if self.guide_now.last().is_none_or(|l| ((l[0] - hit[0]).powi(2) + (l[1] - hit[1]).powi(2) + (l[2] - hit[2]).powi(2)).sqrt() > 0.004) {
            self.guide_now.push(hit);
        }
    }

    pub fn guide_end(&mut self) {
        if self.tools.density_brush {
            self.changed();
            return;
        }
        let g = std::mem::take(&mut self.guide_now);
        if g.len() >= 2 {
            self.data.guides.push(g);
            self.changed();
        }
    }

    pub fn retopo_poll(&mut self) -> bool {
        let Some(j) = &self.retopo_job else { return false };
        // Le fil de calcul s'est arrêté sans rien renvoyer
        let r = match j.rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => None,
            Err(_) => return false,
        };
        let Some(r) = r else {
            self.message = Some(crate::i18n::t("La retopologie a échoué sur ce modèle. Essaie avec moins de faces ou sans les arêtes vives."));
            self.retopo_job = None;
            return true;
        };
        {
            self.message = Some(format!("retopo : {} quads, {} pôles, {} sommets", r.quads(), r.stats.poles, r.pos.len()));
            self.retopo = Some(std::sync::Arc::new(r));
            self.retopo_job = None;
            self.retopo_show = true;
            true
        }
    }

    fn tab_retopo(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        ui.label(RichText::new(crate::i18n::t("Dessine des guides sur le modèle (glisser) : les boucles de quads suivront tes traits. Maj ou clic droit : tourner la vue.")).color(p.muted).size(11.0));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} {}", self.data.guides.len(), crate::i18n::t("guide(s)"))).strong());
            if ui.small_button(crate::i18n::t("annuler le dernier")).clicked() && self.data.guides.pop().is_some() {
                self.changed();
            }
            if ui.small_button(crate::i18n::t("effacer les guides")).clicked() {
                self.data.guides.clear();
                self.changed();
            }
        });
        ui.label(RichText::new(crate::i18n::t("Pour une boucle autour de la bouche, d'un œil, d'un genou ou d'un coude : dessine un trait fermé tout autour.")).color(p.muted).size(10.5));
        ui.separator();
        // Pinceau de densité
        ui.horizontal(|ui| {
            ui.toggle_value(&mut self.tools.density_brush, crate::i18n::t("pinceau de densité")).on_hover_text(crate::i18n::t("Peins sur le modèle là où il faut plus de quads (visage, mains, articulations) ou moins (dos, crâne)."));
            ui.label(RichText::new(format!("{} {}", self.data.density.len(), crate::i18n::t("touche(s)"))).size(10.5).color(p.muted));
            if ui.small_button(crate::i18n::t("effacer la densité")).clicked() {
                self.data.density.clear();
                self.changed();
            }
        });
        if self.tools.density_brush {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("rayon"));
                ui.add(egui::DragValue::new(&mut self.tools.brush_radius).speed(0.002).range(0.005..=1.0));
                ui.label(crate::i18n::t("densité"));
                for (k, l) in [(0.5f32, "×0,5"), (2.0, "×2"), (3.0, "×3"), (4.0, "×4")] {
                    ui.selectable_value(&mut self.tools.brush_factor, k, l);
                }
            });
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(crate::i18n::t("faces visées"));
            ui.add(egui::DragValue::new(&mut self.retopo_opt.faces).range(50..=10_000_000).speed(20.0));
        });
        ui.horizontal(|ui| {
            let mut on = self.retopo_opt.symmetry != 0;
            if ui.checkbox(&mut on, crate::i18n::t("symétrie")).on_hover_text(crate::i18n::t("Le maillage est calculé sur une moitié puis recopié en miroir, avec une boucle sur le plan.")).changed() {
                self.retopo_opt.symmetry = if on { 1 } else { 0 };
            }
            if on {
                for (k, l) in [(1u8, "X"), (2, "Y"), (3, "Z")] {
                    ui.selectable_value(&mut self.retopo_opt.symmetry, k, l);
                }
            }
        });
        ui.add(egui::Slider::new(&mut self.retopo_opt.curvature, 0.0..=2.0).clamping(egui::SliderClamping::Never).text(crate::i18n::t("suivre les courbures"))).on_hover_text(crate::i18n::t("0 : flux le plus régulier possible. Plus haut : les boucles suivent les formes (membres, plis), au prix de quelques pôles en plus."));
        ui.add(egui::Slider::new(&mut self.retopo_opt.adaptive, 0.0..=1.0).text(crate::i18n::t("taille adaptative"))).on_hover_text(crate::i18n::t("Les quads rétrécissent là où les boucles se resserrent au lieu de se déformer."));
        ui.add(egui::Slider::new(&mut self.retopo_opt.sharp_angle, 0.0..=90.0).clamping(egui::SliderClamping::Never).suffix("°").text(crate::i18n::t("arêtes vives"))).on_hover_text(crate::i18n::t("Au-delà de cet angle, une arête devient une boucle nette du maillage (0 : désactivé). Les bords ouverts sont toujours suivis."));
        ui.add(egui::Slider::new(&mut self.retopo_opt.guide_radius, 0.5..=4.0).clamping(egui::SliderClamping::Never).text(crate::i18n::t("portée des guides")));
        ui.horizontal(|ui| {
            ui.label(crate::i18n::t("objets séparés"));
            let mut by_detail = self.retopo_opt.detail_density > 0.0;
            ui.selectable_value(&mut by_detail, false, crate::i18n::t("même densité"));
            ui.selectable_value(&mut by_detail, true, crate::i18n::t("selon les détails"));
            self.retopo_opt.detail_density = match (by_detail, self.retopo_opt.detail_density > 0.0) {
                (false, _) => 0.0,
                (true, false) => 0.7,
                (true, true) => self.retopo_opt.detail_density,
            };
            if by_detail {
                ui.add(egui::Slider::new(&mut self.retopo_opt.detail_density, 0.1..=1.5).text(crate::i18n::t("force"))).on_hover_text(crate::i18n::t("Chaque objet non relié est retopologisé à part. Plus la force est haute, plus un objet chargé de détails reçoit de quads par rapport à un objet lisse."));
            }
        });
        ui.checkbox(&mut self.retopo_opt.auto_loops, crate::i18n::t("boucles automatiques")).on_hover_text(crate::i18n::t("Trouve les rebords qui font le tour d'un détail (yeux, bouche, narines, base d'une corne) et y met des boucles, comme un guide dessiné."));
        ui.add_space(6.0);
        let mut cancel = false;
        if let Some(j) = &self.retopo_job {
            let f = j.progress.lock().map(|g| *g).unwrap_or(0.0);
            let stage = if f < 0.05 { "préparation du maillage" } else if f < 0.35 { "calcul du flux" } else if f < 0.92 { "placement des pôles" } else { "construction des quads" };
            ui.add(egui::ProgressBar::new(f).text(format!("{}… {} s", crate::i18n::t(stage), j.started.elapsed().as_secs())));
            if ui.small_button(crate::i18n::t("annuler")).clicked() {
                cancel = true;
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        } else if ui.button(RichText::new(crate::i18n::t("▶ lancer la retopologie")).strong()).clicked() {
            let pos = self.topo.pos.clone();
            let mut tris = vec![];
            for f in &self.topo.polys {
                for k in 1..f.len().saturating_sub(1) {
                    tris.push([f[0], f[k], f[k + 1]]);
                }
            }
            let guides = self.data.guides.clone();
            let mut opt = self.retopo_opt.clone();
            opt.density = self.data.density.clone();
            let progress = std::sync::Arc::new(std::sync::Mutex::new(0.0f32));
            let pr = progress.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                // Une erreur dans le calcul ne doit pas laisser la barre tourner pour toujours
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::retopo::remesh(&pos, &tris, &guides, &opt, &|f| {
                        if let Ok(mut g) = pr.lock() {
                            *g = f;
                        }
                    })
                }));
                let _ = tx.send(r.ok());
                ctx.request_repaint();
            });
            self.retopo_job = Some(RetopoJob { progress, rx, started: std::time::Instant::now() });
        }
        if cancel {
            self.retopo_job = None;
            self.message = Some(crate::i18n::t("retopologie annulée"));
        }
        if let Some(r) = &self.retopo {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("{} quads · {} pôles · {} sommets", r.quads(), r.stats.poles, r.pos.len())).color(p.muted).size(11.0));
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.retopo_show, crate::i18n::t("afficher"));
                if ui.button(crate::i18n::t("exporter en OBJ")).clicked() {
                    self.retopo_export = true;
                }
            });
        }
    }

    /// Écrit les textures du modèle à partir de ses UV : couleur, rugosité, métal, émission
    /// (matériaux en nodes de chaque ID, ou couleur de l'ID), normale et courbure.
    /// Les texels sont calculés en parallèle ; le bord des îlots est étendu pour éviter les coutures visibles.
    pub fn bake(&self, progs: &[Option<std::sync::Arc<crate::nodes::Program>>], dir: &std::path::Path, stem: &str, high: Option<&(Vec<V3>, Vec<[u32; 3]>)>) -> Result<Vec<PathBuf>, String> {
        use rayon::prelude::*;
        if !self.data.has_uv() || self.data.uv.len() != self.topo.polys.len() {
            return Err("déplie d'abord les UV (onglet UV › auto)".into());
        }
        let n = self.bake_size.clamp(64, 4096) as usize;
        let pos = &self.topo.pos;
        // Normales lisses et courbure par sommet
        let mut all_tris: Vec<[u32; 3]> = vec![];
        let mut rec: Vec<(usize, [usize; 3])> = vec![];
        for (f, p) in self.topo.polys.iter().enumerate() {
            for tr in poly_tris(pos, p) {
                all_tris.push([p[tr[0]], p[tr[1]], p[tr[2]]]);
                rec.push((f, tr));
            }
        }
        let mut nrm = vec![[0.0f32; 3]; pos.len()];
        for t in &all_tris {
            let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
            let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
            let fnm = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            for &i in t {
                for k in 0..3 {
                    nrm[i as usize][k] += fnm[k];
                }
            }
        }
        for x in nrm.iter_mut() {
            let l = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt().max(1e-12);
            *x = [x[0] / l, x[1] / l, x[2] / l];
        }
        let curv = crate::model3d::vertex_curvature(pos, &nrm, &all_tris);
        // Quel triangle couvre chaque texel (coordonnées barycentriques)
        let mut cover: Vec<Option<(u32, [f32; 3])>> = vec![None; n * n];
        for (ti, (f, tr)) in rec.iter().enumerate() {
            let uv = &self.data.uv[*f];
            let p: [[f32; 2]; 3] = [0, 1, 2].map(|k| [uv[tr[k]][0] * n as f32, (1.0 - uv[tr[k]][1]) * n as f32]);
            let d = (p[1][1] - p[2][1]) * (p[0][0] - p[2][0]) + (p[2][0] - p[1][0]) * (p[0][1] - p[2][1]);
            if d.abs() < 1e-12 {
                continue;
            }
            let (x0, x1) = (p.iter().map(|q| q[0]).fold(f32::MAX, f32::min).floor().max(0.0) as usize, (p.iter().map(|q| q[0]).fold(f32::MIN, f32::max).ceil() as usize).min(n - 1));
            let (y0, y1) = (p.iter().map(|q| q[1]).fold(f32::MAX, f32::min).floor().max(0.0) as usize, (p.iter().map(|q| q[1]).fold(f32::MIN, f32::max).ceil() as usize).min(n - 1));
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((p[1][1] - p[2][1]) * (px - p[2][0]) + (p[2][0] - p[1][0]) * (py - p[2][1])) / d;
                    let w1 = ((p[2][1] - p[0][1]) * (px - p[2][0]) + (p[0][0] - p[2][0]) * (py - p[2][1])) / d;
                    let w2 = 1.0 - w0 - w1;
                    if w0 >= -0.02 && w1 >= -0.02 && w2 >= -0.02 {
                        cover[y * n + x] = Some((ti as u32, [w0, w1, w2]));
                    }
                }
            }
        }
        let data = &self.data;
        // Lancer de rayons : le modèle lui-même (occlusion, épaisseur) et le modèle haute définition
        let rays = self.bake_rays.min(256) as usize;
        // Distances relatives à la taille du modèle
        let diag = {
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for q in pos.iter() {
                for k in 0..3 {
                    lo[k] = lo[k].min(q[k]);
                    hi[k] = hi[k].max(q[k]);
                }
            }
            ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt().max(1e-9)
        };
        let (eps, ao_far, th_far, cage) = (diag * 1e-4, diag * 0.25, diag * 0.2, diag * 0.02);
        let bvh = (rays > 0).then(|| crate::bvh::Bvh::new(pos, &all_tris));
        let high_data = high.map(|(hp, ht)| {
            let mut hn = vec![[0.0f32; 3]; hp.len()];
            for t in ht {
                let (a, b, c) = (hp[t[0] as usize], hp[t[1] as usize], hp[t[2] as usize]);
                let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
                let fnm = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
                for &i in t {
                    for k in 0..3 {
                        hn[i as usize][k] += fnm[k];
                    }
                }
            }
            (crate::bvh::Bvh::new(hp, ht), hn, ht)
        });
        // Tangente de chaque triangle (direction où u augmente), pour la normale en espace tangent
        let tangents: Vec<V3> = rec
            .iter()
            .zip(&all_tris)
            .map(|((f, tr), t)| {
                let uv = &self.data.uv[*f];
                let (p0, p1, p2) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
                let (e1, e2) = ([p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]], [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]]);
                let (du1, dv1, du2, dv2) = (uv[tr[1]][0] - uv[tr[0]][0], uv[tr[1]][1] - uv[tr[0]][1], uv[tr[2]][0] - uv[tr[0]][0], uv[tr[2]][1] - uv[tr[0]][1]);
                let det = du1 * dv2 - du2 * dv1;
                let k = if det.abs() > 1e-12 { 1.0 / det } else { 0.0 };
                [(e1[0] * dv2 - e2[0] * dv1) * k, (e1[1] * dv2 - e2[1] * dv1) * k, (e1[2] * dv2 - e2[2] * dv1) * k]
            })
            .collect();
        // Évaluation des matériaux, texel par texel
        let texels: Vec<Option<[[f32; 3]; 7]>> = cover
            .par_iter()
            .enumerate()
            .map_init(Vec::new, |regs, (i, c)| {
                let (ti, w) = (*c)?;
                let t = all_tris[ti as usize];
                let mix3 = |a: V3, b: V3, c: V3| [a[0] * w[0] + b[0] * w[1] + c[0] * w[2], a[1] * w[0] + b[1] * w[1] + c[1] * w[2], a[2] * w[0] + b[2] * w[1] + c[2] * w[2]];
                let p = mix3(pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
                let nn = mix3(nrm[t[0] as usize], nrm[t[1] as usize], nrm[t[2] as usize]);
                let l = (nn[0] * nn[0] + nn[1] * nn[1] + nn[2] * nn[2]).sqrt().max(1e-12);
                let nn = [nn[0] / l, nn[1] / l, nn[2] / l];
                let cv = curv[t[0] as usize] * w[0] + curv[t[1] as usize] * w[1] + curv[t[2] as usize] * w[2];
                let slot = data.face_mat.get(rec[ti as usize].0).copied().unwrap_or(0) as usize;
                let (col, rough, metal, emit) = match progs.get(slot).and_then(|p| p.as_ref()) {
                    Some(prog) => {
                        let ctx = crate::nodes::Ctx { uv: [(i % n) as f32 / n as f32, (i / n) as f32 / n as f32], pos: p, nrm: nn, view: nn, vcol: [1.0; 4], res: [n as f32, n as f32], curv: cv };
                        let sf = prog.material(&ctx, regs);
                        ([sf.color[0], sf.color[1], sf.color[2]], sf.rough, sf.metal, [sf.emit[0], sf.emit[1], sf.emit[2]])
                    }
                    None => {
                        let c = data.slots.get(slot).map(|s| s.color).unwrap_or(PALETTE[0]);
                        ([c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0], 0.5, 0.0, [0.0; 3])
                    }
                };
                let k = (0.5 + cv).clamp(0.0, 1.0);
                // Repère local autour de la normale
                let dotp = |a: V3, b: V3| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
                let crossp = |a: V3, b: V3| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
                let unit = |a: V3| { let l = dotp(a, a).sqrt().max(1e-12); [a[0] / l, a[1] / l, a[2] / l] };
                let tg = tangents[ti as usize];
                let tg = { let d = dotp(tg, nn); [tg[0] - nn[0] * d, tg[1] - nn[1] * d, tg[2] - nn[2] * d] };
                let tg = if dotp(tg, tg) > 1e-12 { unit(tg) } else { unit(crossp(nn, if nn[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] })) };
                let bt = crossp(nn, tg);
                // Normale géométrique du triangle : les rayons partent de ce côté, même si la normale lissée penche
                let ng = {
                    let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
                    let g = unit(crossp([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]));
                    if dotp(g, nn) < 0.0 { [-g[0], -g[1], -g[2]] } else { g }
                };
                // Occlusion (rayons vers l'extérieur) et épaisseur (vers l'intérieur)
                let (mut ao, mut thick) = (1.0f32, 1.0f32);
                if let Some(bvh) = &bvh {
                    let mut seed = (i as u32).wrapping_mul(747796405).wrapping_add(2891336453);
                    let mut rnd = || {
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        (seed >> 8) as f32 / 16777216.0
                    };
                    let (mut open, mut depth) = (0.0f32, 0.0f32);
                    for _ in 0..rays {
                        // Direction tirée selon le cosinus autour de la normale
                        let (r1, r2) = (rnd(), rnd());
                        let (rad, phi) = (r1.sqrt(), r2 * std::f32::consts::TAU);
                        let (x, y, z) = (rad * phi.cos(), rad * phi.sin(), (1.0 - r1).max(0.0).sqrt());
                        let mut d = [tg[0] * x + bt[0] * y + nn[0] * z, tg[1] * x + bt[1] * y + nn[1] * z, tg[2] * x + bt[2] * y + nn[2] * z];
                        let below = dotp(d, ng);
                        if below < 1e-3 {
                            let r = 2.0 * (1e-3 - below);
                            d = unit([d[0] + ng[0] * r, d[1] + ng[1] * r, d[2] + ng[2] * r]);
                        }
                        if !bvh.occluded([p[0] + ng[0] * eps, p[1] + ng[1] * eps, p[2] + ng[2] * eps], d, eps, ao_far) {
                            open += 1.0;
                        }
                        let inw = [-d[0], -d[1], -d[2]];
                        depth += bvh.closest([p[0] - ng[0] * eps, p[1] - ng[1] * eps, p[2] - ng[2] * eps], inw, eps, th_far).map(|h| h.t).unwrap_or(th_far) / th_far;
                    }
                    ao = open / rays as f32;
                    thick = depth / rays as f32;
                }
                // Détails du modèle haute définition, exprimés dans le repère tangent
                let mut tn = [0.5f32, 0.5, 1.0];
                if let Some((hb, hn, ht)) = &high_data {
                    let o = [p[0] + nn[0] * cage, p[1] + nn[1] * cage, p[2] + nn[2] * cage];
                    if let Some(h) = hb.closest(o, [-nn[0], -nn[1], -nn[2]], 0.0, cage * 2.0) {
                        let t = ht[h.tri as usize];
                        let w0 = 1.0 - h.u - h.v;
                        let n3 = [0, 1, 2].map(|k| hn[t[0] as usize][k] * w0 + hn[t[1] as usize][k] * h.u + hn[t[2] as usize][k] * h.v);
                        let n3 = unit(n3);
                        tn = [dotp(n3, tg) * 0.5 + 0.5, dotp(n3, bt) * 0.5 + 0.5, dotp(n3, nn).max(0.0) * 0.5 + 0.5];
                    }
                }
                Some([col, [rough, metal, k], emit, [nn[0] * 0.5 + 0.5, nn[1] * 0.5 + 0.5, nn[2] * 0.5 + 0.5], p, [ao, thick, 0.0], tn])
            })
            .collect();
        // Extension des bords : chaque texel vide prend la valeur d'un voisin rempli (8 passes)
        let mut tex = texels;
        for _ in 0..8 {
            let old = tex.clone();
            tex.par_iter_mut().enumerate().for_each(|(i, t)| {
                if t.is_some() {
                    return;
                }
                let (x, y) = ((i % n) as i64, (i / n) as i64);
                for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1), (1, -1), (-1, 1)] {
                    let (xx, yy) = (x + dx, y + dy);
                    if xx >= 0 && yy >= 0 && xx < n as i64 && yy < n as i64 {
                        if let Some(v) = old[yy as usize * n + xx as usize] {
                            *t = Some(v);
                            return;
                        }
                    }
                }
            });
        }
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        let any_emit = tex.iter().flatten().any(|t| t[2].iter().any(|x| *x > 0.003));
        let mut out = vec![];
        let (has_rays, has_high) = (rays > 0, high.is_some());
        let maps: [(&str, Box<dyn Fn(&[[f32; 3]; 7]) -> [u8; 3] + Sync>, [u8; 3], bool); 9] = [
            ("couleur", Box::new(move |t| [q(t[0][0]), q(t[0][1]), q(t[0][2])]), [128, 128, 128], true),
            ("rugosite", Box::new(move |t| [q(t[1][0]); 3]), [128; 3], true),
            ("metal", Box::new(move |t| [q(t[1][1]); 3]), [0; 3], true),
            ("emission", Box::new(move |t| [q(t[2][0]), q(t[2][1]), q(t[2][2])]), [0; 3], any_emit),
            ("normale", Box::new(move |t| [q(t[3][0]), q(t[3][1]), q(t[3][2])]), [128, 128, 255], true),
            ("courbure", Box::new(move |t| [q(t[1][2]); 3]), [128; 3], true),
            ("occlusion", Box::new(move |t| [q(t[5][0]); 3]), [255; 3], has_rays),
            ("epaisseur", Box::new(move |t| [q(t[5][1]); 3]), [255; 3], has_rays),
            ("normale_tangente", Box::new(move |t| [q(t[6][0]), q(t[6][1]), q(t[6][2])]), [128, 128, 255], has_high),
        ];
        for (name, f, empty, wanted) in maps.iter() {
            if !*wanted {
                continue;
            }
            let bytes: Vec<u8> = tex.iter().flat_map(|t| t.as_ref().map(|t| f(t)).unwrap_or(*empty)).collect();
            let path = dir.join(format!("{stem}_{name}.png"));
            image::RgbImage::from_raw(n as u32, n as u32, bytes).ok_or("image invalide")?.save(&path).map_err(|e| e.to_string())?;
            out.push(path);
        }
        Ok(out)
    }

    fn tab_uv(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        ui.horizontal(|ui| {
            ui.label(RichText::new(crate::i18n::t("haute définition")).size(11.0).color(p.muted));
            ui.add(egui::TextEdit::singleline(&mut self.bake_high).hint_text(crate::i18n::t("chemin du sculpt (facultatif)")).desired_width(220.0)).on_hover_text(crate::i18n::t("Les détails de ce modèle sont reportés dans une normale en espace tangent. Il doit occuper la même place que le modèle ouvert."));
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(self.data.has_uv(), egui::Button::new(crate::i18n::t("baker les textures"))).on_hover_text(crate::i18n::t("Écrit couleur, rugosité, métal, normale et courbure dans le dossier textures/, à partir des UV et des matériaux de chaque ID")).clicked() {
                self.bake_request = true;
            }
            egui::ComboBox::from_id_salt("bake-rays").selected_text(if self.bake_rays == 0 { crate::i18n::t("sans occlusion") } else { format!("{} {}", self.bake_rays, crate::i18n::t("rayons")) }).width(96.0).show_ui(ui, |ui| {
                for (r, l) in [(0u32, "sans occlusion"), (8, "8 (brouillon)"), (24, "24"), (64, "64"), (128, "128 (lisse)")] {
                    ui.selectable_value(&mut self.bake_rays, r, crate::i18n::t(l));
                }
            });
            egui::ComboBox::from_id_salt("bake-size").selected_text(format!("{} px", self.bake_size)).width(80.0).show_ui(ui, |ui| {
                for s in [512u32, 1024, 2048, 4096] {
                    ui.selectable_value(&mut self.bake_size, s, format!("{s} px"));
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::t("auto (coutures + UV)")).on_hover_text(crate::i18n::t("Coutures automatiques avancées, dépliage, îlots redressés et rangés")).clicked() {
                self.auto_now(true);
            }
            if ui.add_enabled(!self.data.seams.is_empty(), egui::Button::new(crate::i18n::t("déplier mes coutures"))).clicked() {
                self.unwrap_now();
            }
            if self.data.has_uv() && ui.button(crate::i18n::t("ranger")).on_hover_text(crate::i18n::t("packing : même densité, rotation, rangement serré")).clicked() {
                pack(&self.topo, &mut self.data, &PackOptions { optimize: self.optimize, rotate: self.rotate, fixed_scale: None });
                self.message = Some(format!("remplissage {:.0} %", fill_rate(&self.data) * 100.0));
                self.changed();
            }
            if ui.toggle_value(&mut self.checker, crate::i18n::t("damier")).changed() {
                self.version += 1;
            }
        });
        egui::CollapsingHeader::new(RichText::new(crate::i18n::t("réglages de l'auto")).size(11.0)).show(ui, |ui| self.auto_options(ui));
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.optimize, crate::i18n::t("rangement optimisé")).on_hover_text(crate::i18n::t("essaie plusieurs ordres, rotations et largeurs et garde le plus serré"));
            ui.checkbox(&mut self.rotate, crate::i18n::t("rotation"));
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new(crate::i18n::t("texture")).size(11.0));
            egui::ComboBox::from_id_salt("uv-tex").selected_text(format!("{} px", self.data.texture_px)).width(80.0).show_ui(ui, |ui| {
                for v in [512u32, 1024, 2048, 4096, 8192] {
                    ui.selectable_value(&mut self.data.texture_px, v, format!("{v}"));
                }
            });
            ui.label(RichText::new(crate::i18n::t("marge")).size(11.0));
            ui.add(egui::DragValue::new(&mut self.data.margin_px).range(0.0..=4096.0).suffix(" px"));
        });
        if !self.data.has_uv() {
            ui.label(RichText::new(crate::i18n::t("Pas encore d'UV : « déplier » pose des coutures automatiques si besoin, déplie (LSCM) et range les îlots.")).color(p.muted).size(10.5));
            return;
        }
        // Densité de texels
        let dens = densities(&self.topo, &self.data, self.scale);
        let (mn, mx) = dens.iter().fold((f32::MAX, 0.0f32), |a, &x| (a.0.min(x), a.1.max(x)));
        let avg = dens.iter().sum::<f32>() / dens.len().max(1) as f32;
        ui.label(RichText::new(format!("densité : {avg:.0} px/unité (min {mn:.0}, max {mx:.0}) · remplissage {:.0} %", fill_rate(&self.data) * 100.0)).size(10.5).color(p.muted));
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.density_target).range(1.0..=100000.0).speed(4.0).suffix(" px/u"));
            if ui.button(crate::i18n::t("imposer la densité")).on_hover_text(crate::i18n::t("tous les îlots à cette densité (ils peuvent déborder du carré si elle est trop forte)")).clicked() {
                set_density(&self.topo, &mut self.data, self.density_target, self.scale);
                self.changed();
            }
            if ui.small_button(crate::i18n::t("= actuelle")).clicked() {
                self.density_target = avg.round();
            }
        });
        ui.add_space(4.0);
        // Îlots sélectionnés : transformations manuelles
        let sel_charts: BTreeSet<u32> = self.sel.iter().filter_map(|f| self.data.chart.get(*f as usize).copied()).collect();
        if !sel_charts.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{} îlot(s) :", sel_charts.len())).size(10.5));
                let mut op: Option<u8> = None;
                for (k, l) in [(0u8, "↻ 90°"), (1, "↔ miroir"), (2, "＋ 10 %"), (3, "－ 10 %")] {
                    if ui.small_button(l).clicked() {
                        op = Some(k);
                    }
                }
                if let Some(op) = op {
                    self.transform_charts(&sel_charts, op, [0.0, 0.0]);
                }
            });
        }
        if self.tools.no_split {
            let side = ui.available_width().min(420.0);
            let (r, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
            self.uv_editor(ui, r);
        } else {
            ui.label(RichText::new(crate::i18n::t("L'éditeur UV est à gauche de la vue 3D.")).color(p.muted).size(10.5));
        }
        if ui.checkbox(&mut self.tools.no_split, crate::i18n::t("éditeur UV dans ce panneau")).changed() {
            self.version += 1;
        }
    }

    fn transform_charts(&mut self, charts_: &BTreeSet<u32>, op: u8, delta: V2) {
        let faces: Vec<usize> = (0..self.topo.polys.len()).filter(|f| self.data.chart.get(*f).is_some_and(|c| charts_.contains(c))).collect();
        let mut c = [0.0f32; 2];
        let mut n = 0.0;
        for &f in &faces {
            for q in &self.data.uv[f] {
                c[0] += q[0];
                c[1] += q[1];
                n += 1.0;
            }
        }
        if n == 0.0 {
            return;
        }
        c = [c[0] / n, c[1] / n];
        for &f in &faces {
            for q in &mut self.data.uv[f] {
                let d = [q[0] - c[0], q[1] - c[1]];
                *q = match op {
                    0 => [c[0] - d[1], c[1] + d[0]],
                    1 => [c[0] - d[0], q[1]],
                    2 => [c[0] + d[0] * 1.1, c[1] + d[1] * 1.1],
                    3 => [c[0] + d[0] / 1.1, c[1] + d[1] / 1.1],
                    _ => [q[0] + delta[0], q[1] + delta[1]],
                };
            }
        }
        self.changed();
    }

}

fn point_in_poly(q: V2, poly: &[V2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > q[1]) != (b[1] > q[1]) && q[0] < (b[0] - a[0]) * (q[1] - a[1]) / (b[1] - a[1]).max(1e-12).copysign(b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

// ---------- export avec UV et matériaux ----------

/// OBJ + MTL : sommets, UV (vt), un groupe par matériau (usemtl).
pub fn export_obj(e: &Edit, world: &dyn Fn(V3) -> V3, path: &std::path::Path) -> Result<(), String> {
    use std::io::Write;
    let io = |x: std::io::Error| x.to_string();
    let mtl = path.with_extension("mtl");
    let mtl_name = mtl.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let mut w = std::io::BufWriter::new(std::fs::File::create(path).map_err(io)?);
    writeln!(w, "# exporté par Aryadne\nmtllib {mtl_name}").map_err(io)?;
    for p in &e.topo.pos {
        let q = world(*p);
        writeln!(w, "v {} {} {}", q[0], q[1], q[2]).map_err(io)?;
    }
    let has_uv = e.data.has_uv();
    let mut vt_index: Vec<Vec<usize>> = vec![];
    if has_uv {
        let mut k = 0;
        for f in &e.data.uv {
            let mut row = vec![];
            for q in f {
                writeln!(w, "vt {} {}", q[0], q[1]).map_err(io)?;
                k += 1;
                row.push(k);
            }
            vt_index.push(row);
        }
    }
    for (m, slot) in e.data.slots.iter().enumerate() {
        let faces: Vec<usize> = (0..e.topo.polys.len()).filter(|f| e.data.face_mat[*f] as usize == m).collect();
        if faces.is_empty() {
            continue;
        }
        writeln!(w, "usemtl {}", slot.name.replace(' ', "_")).map_err(io)?;
        for f in faces {
            let idx: Vec<String> = e.topo.polys[f].iter().enumerate().map(|(i, v)| if has_uv { format!("{}/{}", v + 1, vt_index[f][i]) } else { (v + 1).to_string() }).collect();
            writeln!(w, "f {}", idx.join(" ")).map_err(io)?;
        }
    }
    w.flush().map_err(io)?;
    let mut m = std::io::BufWriter::new(std::fs::File::create(&mtl).map_err(io)?);
    for s in &e.data.slots {
        writeln!(m, "newmtl {}\nKd {} {} {}\n", s.name.replace(' ', "_"), s.color[0] as f32 / 255.0, s.color[1] as f32 / 255.0, s.color[2] as f32 / 255.0).map_err(io)?;
    }
    m.flush().map_err(io)
}

/// GLB : un primitive par ID de matériau, avec UV (TEXCOORD_0) et normales.
pub fn export_glb(e: &Edit, world: &dyn Fn(V3) -> V3, path: &std::path::Path) -> Result<(), String> {
    use std::io::Write;
    let has_uv = e.data.has_uv();
    // Sommets dédoublés par coin (une UV par coin)
    let mut pos: Vec<V3> = vec![];
    let mut uv: Vec<V2> = vec![];
    let mut prims: Vec<Vec<u32>> = vec![vec![]; e.data.slots.len().max(1)];
    for (f, poly) in e.topo.polys.iter().enumerate() {
        let base = pos.len() as u32;
        for (i, v) in poly.iter().enumerate() {
            pos.push(world(e.topo.pos[*v as usize]));
            let q = if has_uv { e.data.uv[f][i] } else { [0.0, 0.0] };
            uv.push([q[0], 1.0 - q[1]]);
        }
        let m = (e.data.face_mat[f] as usize).min(prims.len() - 1);
        for k in 1..poly.len() as u32 - 1 {
            prims[m].extend([base, base + k, base + k + 1]);
        }
    }
    let all: Vec<[u32; 3]> = prims.iter().flat_map(|p| p.chunks(3).map(|c| [c[0], c[1], c[2]])).collect();
    let nrm = {
        let mut n = vec![[0.0f32; 3]; pos.len()];
        // Normales lisses par position soudée
        let mut acc: HashMap<[u32; 3], V3> = HashMap::new();
        for t in &all {
            let fn_ = cross(sub(pos[t[1] as usize], pos[t[0] as usize]), sub(pos[t[2] as usize], pos[t[0] as usize]));
            for &i in t {
                let p = pos[i as usize];
                let k = [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
                let a = acc.entry(k).or_insert([0.0; 3]);
                *a = add(*a, fn_);
            }
        }
        for (i, p) in pos.iter().enumerate() {
            n[i] = norm(*acc.get(&[p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]).unwrap_or(&[0.0, 1.0, 0.0]));
        }
        n
    };
    let mut bin: Vec<u8> = vec![];
    let push3 = |bin: &mut Vec<u8>, v: &[V3]| {
        for p in v {
            for k in 0..3 {
                bin.extend(p[k].to_le_bytes());
            }
        }
    };
    push3(&mut bin, &pos);
    let nrm_off = bin.len();
    push3(&mut bin, &nrm);
    let uv_off = bin.len();
    for q in &uv {
        bin.extend(q[0].to_le_bytes());
        bin.extend(q[1].to_le_bytes());
    }
    let mut views = vec![
        serde_json::json!({ "buffer": 0, "byteOffset": 0, "byteLength": nrm_off, "target": 34962 }),
        serde_json::json!({ "buffer": 0, "byteOffset": nrm_off, "byteLength": uv_off - nrm_off, "target": 34962 }),
        serde_json::json!({ "buffer": 0, "byteOffset": uv_off, "byteLength": uv.len() * 8, "target": 34962 }),
    ];
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let mut accessors = vec![
        serde_json::json!({ "bufferView": 0, "componentType": 5126, "count": pos.len(), "type": "VEC3", "min": lo, "max": hi }),
        serde_json::json!({ "bufferView": 1, "componentType": 5126, "count": pos.len(), "type": "VEC3" }),
        serde_json::json!({ "bufferView": 2, "componentType": 5126, "count": uv.len(), "type": "VEC2" }),
    ];
    let mut primitives = vec![];
    let mut materials = vec![];
    for (m, idx) in prims.iter().enumerate() {
        if idx.is_empty() {
            continue;
        }
        let off = bin.len();
        for i in idx {
            bin.extend(i.to_le_bytes());
        }
        views.push(serde_json::json!({ "buffer": 0, "byteOffset": off, "byteLength": idx.len() * 4, "target": 34963 }));
        accessors.push(serde_json::json!({ "bufferView": views.len() - 1, "componentType": 5125, "count": idx.len(), "type": "SCALAR" }));
        let s = e.data.slots.get(m).cloned().unwrap_or(Slot { name: "matériau".into(), color: PALETTE[0], material: String::new() });
        let lin = |c: u8| (c as f32 / 255.0).powf(2.2);
        materials.push(serde_json::json!({ "name": s.name, "pbrMetallicRoughness": { "baseColorFactor": [lin(s.color[0]), lin(s.color[1]), lin(s.color[2]), 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.6 } }));
        let mut attrs = serde_json::json!({ "POSITION": 0, "NORMAL": 1 });
        if has_uv {
            attrs["TEXCOORD_0"] = serde_json::json!(2);
        }
        primitives.push(serde_json::json!({ "attributes": attrs, "indices": accessors.len() - 1, "material": materials.len() - 1, "mode": 4 }));
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let json = serde_json::json!({
        "asset": { "version": "2.0", "generator": "Aryadne" },
        "scene": 0, "scenes": [ { "nodes": [0] } ], "nodes": [ { "mesh": 0, "name": name } ],
        "meshes": [ { "name": name, "primitives": primitives } ],
        "materials": materials,
        "buffers": [ { "byteLength": bin.len() } ], "bufferViews": views, "accessors": accessors
    });
    let mut js = serde_json::to_vec(&json).map_err(|e| e.to_string())?;
    while js.len() % 4 != 0 {
        js.push(b' ');
    }
    let io = |x: std::io::Error| x.to_string();
    let mut w = std::io::BufWriter::new(std::fs::File::create(path).map_err(io)?);
    w.write_all(b"glTF").map_err(io)?;
    w.write_all(&2u32.to_le_bytes()).map_err(io)?;
    w.write_all(&((12 + 8 + js.len() + 8 + bin.len()) as u32).to_le_bytes()).map_err(io)?;
    w.write_all(&(js.len() as u32).to_le_bytes()).map_err(io)?;
    w.write_all(b"JSON").map_err(io)?;
    w.write_all(&js).map_err(io)?;
    w.write_all(&(bin.len() as u32).to_le_bytes()).map_err(io)?;
    w.write_all(b"BIN\0").map_err(io)?;
    w.write_all(&bin).map_err(io)?;
    w.flush().map_err(io)
}

#[cfg(test)]
mod export_tests {
    use super::*;

    #[test]
    fn export_uv_et_ids() {
        let dir = std::env::temp_dir().join(format!("aryadne-uvexp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("cube.obj");
        crate::vault::atomic_write(&src, "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nv 1 0 1\nv 1 1 1\nv 0 1 1\nf 1 4 3 2\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n").unwrap();
        let model = crate::model3d::load(&src).unwrap();
        let mesh = crate::modifiers::Mesh::from_model(&model, false);
        let mut e = Edit::open(&mesh, dir.join("data.json"), 1.0);
        e.data.seams = seams_by_angle(&e.topo, 45.0);
        e.unwrap_now();
        e.data.slots.push(Slot { name: "rouge".into(), color: [255, 0, 0], material: String::new() });
        e.data.face_mat[0] = 1;
        let id = |p: V3| p;
        export_obj(&e, &id, &dir.join("o.obj")).unwrap();
        let obj = std::fs::read_to_string(dir.join("o.obj")).unwrap();
        assert_eq!(obj.lines().filter(|l| l.starts_with("vt ")).count(), 24);
        assert_eq!(obj.lines().filter(|l| l.starts_with("usemtl")).count(), 2);
        assert!(std::fs::read_to_string(dir.join("o.mtl")).unwrap().contains("newmtl rouge"));
        export_glb(&e, &id, &dir.join("o.glb")).unwrap();
        let (doc, _, _) = gltf::import(dir.join("o.glb")).expect("glb valide");
        let prims: Vec<_> = doc.meshes().next().unwrap().primitives().collect();
        assert_eq!(prims.len(), 2);
        assert!(prims[0].get(&gltf::Semantic::TexCoords(0)).is_some());
        // Rechargé par notre propre lecteur aussi
        let back = crate::model3d::load(&dir.join("o.obj")).unwrap();
        assert_eq!(back.total_tris, 12);
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod auto_tests {
    use super::*;

    fn grid_topo(pos: Vec<V3>, quads: Vec<[u32; 4]>) -> Topo {
        let mut tris = vec![];
        let mut edges = vec![];
        for q in quads {
            for k in 1..3 {
                tris.push([q[0], q[k], q[k + 1]]);
                edges.push(crate::model3d::fan_mask(k, 4));
            }
        }
        Topo::new(&pos, &tris, &edges)
    }

    /// Cylindre ouvert (n segments × m anneaux) ou tore.
    fn cylinder(n: u32, m: u32) -> Topo {
        let mut pos = vec![];
        for j in 0..=m {
            for i in 0..n {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                pos.push([a.cos() * 0.3, j as f32 / m as f32, a.sin() * 0.3]);
            }
        }
        let mut quads = vec![];
        for j in 0..m {
            for i in 0..n {
                let (a, b) = (j * n + i, j * n + (i + 1) % n);
                quads.push([a, b, b + n, a + n]);
            }
        }
        grid_topo(pos, quads)
    }

    fn torus(n: u32, m: u32) -> Topo {
        let mut pos = vec![];
        for j in 0..m {
            let b = j as f32 / m as f32 * std::f32::consts::TAU;
            for i in 0..n {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                let r = 1.0 + 0.3 * b.cos();
                pos.push([a.cos() * r, 0.3 * b.sin(), a.sin() * r]);
            }
        }
        let mut quads = vec![];
        for j in 0..m {
            for i in 0..n {
                let (a, b) = (j * n + i, j * n + (i + 1) % n);
                let (c, d) = (((j + 1) % m) * n + (i + 1) % n, ((j + 1) % m) * n + i);
                quads.push([a, b, c, d]);
            }
        }
        grid_topo(pos, quads)
    }

    #[test]
    fn cylindre_une_couture() {
        let t = cylinder(24, 6);
        let mut d = EditData::fresh(&t);
        auto_unwrap(&t, &mut d, &AutoOptions::default(), &PackOptions::default());
        let n = d.chart.iter().max().unwrap() + 1;
        assert_eq!(n, 1, "un seul îlot");
        assert_eq!(d.seams.len(), 6, "une seule couture verticale");
        // Les quads restent droits : bords alignés sur les axes
        let u = &d.uv[0];
        let (dx, dy) = (u[1][0] - u[0][0], u[1][1] - u[0][1]);
        let ang = dy.atan2(dx).to_degrees().rem_euclid(90.0);
        assert!(ang < 3.0 || ang > 87.0, "orientation {ang}");
    }

    #[test]
    fn tore_peu_d_ilots() {
        let t = torus(32, 12);
        let mut d = EditData::fresh(&t);
        let o = AutoOptions { max_distortion: 0.5, ..AutoOptions::default() };
        auto_unwrap(&t, &mut d, &o, &PackOptions::default());
        let n = d.chart.iter().max().unwrap() + 1;
        assert!(n <= 2, "tore : {n} îlots");
        assert!(d.uv.iter().flatten().all(|p| p[0].is_finite() && p[0] >= -1e-3 && p[0] <= 1.001));
    }
}

#[cfg(test)]
mod blender_files {
    use super::*;
    /// ARYADNE_EXPORT_DIR=… : écrit tore avec UV et IDs (pour vérifier dans Blender)
    #[test]
    fn ecrire_pour_blender() {
        let Ok(dir) = std::env::var("ARYADNE_EXPORT_DIR") else { return };
        let dir = std::path::PathBuf::from(dir);
        let model = crate::model3d::load(std::path::Path::new("/home/claude/demo-vault/3d/tore.obj")).unwrap();
        let mesh = crate::modifiers::Mesh::from_model(&model, false);
        let mut e = Edit::open(&mesh, dir.join("d.json"), 1.0);
        e.auto_now(true);
        e.data.slots.push(Slot { name: "rouge".into(), color: [220, 40, 40], material: String::new() });
        for f in 0..40 {
            e.data.face_mat[f] = 1;
        }
        let id = |p: V3| p;
        export_obj(&e, &id, &dir.join("tore.obj")).unwrap();
        export_glb(&e, &id, &dir.join("tore.glb")).unwrap();
        #[allow(unused)]
        if let Ok(()) = crate::fbxwrite::export(&e, &id, &dir.join("tore.fbx")) {}
    }
}
