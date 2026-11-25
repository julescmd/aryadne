//! Sculpture : niveaux de multirésolution (on monte et on descend sans perdre le détail) et
//! remaillage dynamique, qui subdivise la surface sous le pinceau pendant qu'on sculpte.

use crate::meshedit::EMesh;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
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

/// Au-delà, on refuse d'ajouter un niveau (la vue et la mémoire ne suivraient plus)
pub const MAX_FACES: usize = 1_500_000;

/// Un pas de Catmull-Clark sur des polygones. L'ordre des sommets est fixe : ceux d'origine, puis
/// un point par face, puis un point par arête (dans l'ordre où les faces les rencontrent). C'est ce
/// qui permet de ranger un détail par sommet et de le retrouver après avoir refait la subdivision.
pub fn subdivide(m: &EMesh) -> EMesh {
    let nv = m.pos.len();
    let ok = |p: &Vec<u32>| p.len() >= 3 && p.iter().all(|&v| (v as usize) < nv);
    let fp: Vec<V3> = m
        .polys
        .iter()
        .map(|p| if ok(p) { mul(p.iter().fold([0.0; 3], |a, &v| add(a, m.pos[v as usize])), 1.0 / p.len() as f32) } else { [0.0; 3] })
        .collect();
    // Arêtes : extrémités, somme des points des faces voisines, nombre de faces
    let mut edge_id: HashMap<(u32, u32), u32> = HashMap::new();
    let mut edges: Vec<(u32, u32, V3, u32)> = vec![];
    for (f, p) in m.polys.iter().enumerate().filter(|(_, p)| ok(p)) {
        for i in 0..p.len() {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            let key = (a.min(b), a.max(b));
            let id = *edge_id.entry(key).or_insert_with(|| {
                edges.push((key.0, key.1, [0.0; 3], 0));
                edges.len() as u32 - 1
            });
            let e = &mut edges[id as usize];
            e.2 = add(e.2, fp[f]);
            e.3 += 1;
        }
    }
    let nf = m.polys.len();
    let mut pos = Vec::with_capacity(nv + nf + edges.len());
    // Sommets d'origine : moyenne pondérée des faces, des arêtes et du sommet (bord : le long du bord seulement)
    let mut face_sum = vec![[0.0f32; 3]; nv];
    let mut face_n = vec![0u32; nv];
    for (f, p) in m.polys.iter().enumerate().filter(|(_, p)| ok(p)) {
        for &v in p {
            face_sum[v as usize] = add(face_sum[v as usize], fp[f]);
            face_n[v as usize] += 1;
        }
    }
    let mut mid_sum = vec![[0.0f32; 3]; nv];
    let mut mid_n = vec![0u32; nv];
    let mut border_sum = vec![[0.0f32; 3]; nv];
    let mut border_n = vec![0u32; nv];
    for &(a, b, _, n) in &edges {
        let mid = mul(add(m.pos[a as usize], m.pos[b as usize]), 0.5);
        for (v, o) in [(a, b), (b, a)] {
            mid_sum[v as usize] = add(mid_sum[v as usize], mid);
            mid_n[v as usize] += 1;
            if n != 2 {
                border_sum[v as usize] = add(border_sum[v as usize], m.pos[o as usize]);
                border_n[v as usize] += 1;
            }
        }
    }
    for v in 0..nv {
        let p = m.pos[v];
        pos.push(if border_n[v] == 2 {
            add(mul(p, 0.75), mul(border_sum[v], 0.125))
        } else if border_n[v] > 0 || face_n[v] == 0 || mid_n[v] < 3 {
            p
        } else {
            let n = mid_n[v] as f32;
            let f = mul(face_sum[v], 1.0 / face_n[v] as f32);
            let r = mul(mid_sum[v], 1.0 / n);
            mul(add(add(f, mul(r, 2.0)), mul(p, n - 3.0)), 1.0 / n)
        });
    }
    pos.extend(fp.iter().copied());
    for &(a, b, s, n) in &edges {
        let ends = add(m.pos[a as usize], m.pos[b as usize]);
        pos.push(if n == 2 { mul(add(ends, s), 0.25) } else { mul(ends, 0.5) });
    }
    let mut polys = Vec::with_capacity(m.polys.iter().map(|p| p.len()).sum());
    for (f, p) in m.polys.iter().enumerate().filter(|(_, p)| ok(p)) {
        let k = p.len();
        let e = |a: u32, b: u32| (nv + nf) as u32 + edge_id[&(a.min(b), a.max(b))];
        for i in 0..k {
            polys.push(vec![p[i], e(p[i], p[(i + 1) % k]), (nv + f) as u32, e(p[(i + k - 1) % k], p[i])]);
        }
    }
    EMesh { pos, polys }
}

/// Un repère par sommet (tangente, bitangente, normale), tiré de la seule forme du maillage : un
/// détail rangé dans ce repère suit la surface quand on la déforme à un niveau plus bas.
fn frames(m: &EMesh) -> Vec<[V3; 3]> {
    let ns = crate::sculpt::normals(m);
    let mut first = vec![u32::MAX; m.pos.len()];
    for p in &m.polys {
        for i in 0..p.len() {
            if let Some(f) = first.get_mut(p[i] as usize).filter(|f| **f == u32::MAX) {
                *f = p[(i + 1) % p.len()];
            }
        }
    }
    (0..m.pos.len())
        .map(|i| {
            let n = ns[i];
            // Sommet isolé ou normale nulle : le repère du maillage
            if !(len(n) > 0.5) || first[i] as usize >= m.pos.len() {
                return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
            }
            let d = sub(m.pos[first[i] as usize], m.pos[i]);
            let mut t = sub(d, mul(n, dot(d, n)));
            if len(t) < 1e-9 {
                t = cross(n, if n[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] });
            }
            let t = mul(t, 1.0 / len(t).max(1e-12));
            [t, cross(n, t), n]
        })
        .collect()
}

/// Les niveaux d'un maillage sculpté : la forme de base, puis pour chaque niveau l'écart de chaque
/// sommet à la subdivision lisse du niveau d'en dessous, dans le repère de la surface.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Multires {
    pub base_pos: Vec<V3>,
    pub base_polys: Vec<Vec<u32>>,
    pub levels: Vec<Vec<V3>>,
    /// Niveau qu'on est en train de sculpter (0 : la base)
    pub current: usize,
}

impl Multires {
    pub fn new(base: &EMesh) -> Multires {
        Multires { base_pos: base.pos.clone(), base_polys: base.polys.clone(), levels: vec![], current: 0 }
    }

    /// Le niveau le plus fin
    pub fn top(&self) -> usize {
        self.levels.len()
    }

    /// Le maillage d'un niveau, tous les détails des niveaux jusqu'à lui compris
    pub fn mesh_at(&self, level: usize) -> EMesh {
        let mut m = EMesh { pos: self.base_pos.clone(), polys: self.base_polys.clone() };
        for d in self.levels.iter().take(level) {
            let mut s = subdivide(&m);
            if d.len() == s.pos.len() {
                let fr = frames(&s);
                for (i, p) in s.pos.iter_mut().enumerate() {
                    *p = add(*p, add(add(mul(fr[i][0], d[i][0]), mul(fr[i][1], d[i][1])), mul(fr[i][2], d[i][2])));
                }
            }
            m = s;
        }
        m
    }

    /// Le maillage donné est-il bien celui du niveau courant (même nombre de sommets, mêmes faces) ?
    /// Un maillage retouché en mode édition entre-temps ne correspond plus, même à nombre égal.
    pub fn matches(&self, mesh: &EMesh) -> bool {
        if self.current > self.top() {
            return false;
        }
        let m = self.mesh_at(self.current);
        m.pos.len() == mesh.pos.len() && m.polys == mesh.polys
    }

    /// Range le maillage sculpté comme état du niveau courant. Les niveaux plus fins gardent leur
    /// détail : il est exprimé par rapport à la surface, il suivra donc ce qu'on vient de changer.
    pub fn store(&mut self, mesh: &EMesh) -> bool {
        if self.current > self.top() {
            return false;
        }
        if self.current == 0 {
            if mesh.pos.len() != self.base_pos.len() || mesh.polys.len() != self.base_polys.len() {
                return false;
            }
            self.base_pos = mesh.pos.clone();
            return true;
        }
        let s = subdivide(&self.mesh_at(self.current - 1));
        if s.pos.len() != mesh.pos.len() {
            return false;
        }
        let fr = frames(&s);
        self.levels[self.current - 1] = (0..s.pos.len())
            .map(|i| {
                let d = sub(mesh.pos[i], s.pos[i]);
                let d = [dot(d, fr[i][0]), dot(d, fr[i][1]), dot(d, fr[i][2])];
                if d.iter().all(|x| x.is_finite()) { d } else { [0.0; 3] }
            })
            .collect();
        true
    }

    /// Passe à un autre niveau et rend son maillage
    pub fn go(&mut self, level: usize) -> EMesh {
        self.current = level.min(self.top());
        self.mesh_at(self.current)
    }

    /// Ajoute un niveau au-dessus du plus fin (chaque face devient autant de quads qu'elle a de
    /// côtés) et y passe. Rien si le maillage deviendrait trop lourd.
    pub fn add_level(&mut self) -> Option<EMesh> {
        let m = self.mesh_at(self.top());
        if m.polys.iter().map(|p| p.len()).sum::<usize>() > MAX_FACES || m.polys.is_empty() {
            return None;
        }
        let s = subdivide(&m);
        self.levels.push(vec![[0.0; 3]; s.pos.len()]);
        self.current = self.top();
        Some(s)
    }

    /// Le fichier des niveaux, à côté du maillage
    pub fn file_for(mesh_file: &Path) -> PathBuf {
        mesh_file.with_extension("multires")
    }

    pub fn load(mesh_file: &Path) -> Option<Multires> {
        let mr: Multires = serde_json::from_slice(&std::fs::read(Self::file_for(mesh_file)).ok()?).ok()?;
        // Fichier abîmé : des indices hors du maillage feraient tout rater plus loin
        let n = mr.base_pos.len();
        (mr.base_polys.iter().flatten().all(|&v| (v as usize) < n) && mr.base_pos.iter().flatten().all(|x| x.is_finite())).then_some(mr)
    }

    pub fn save(&self, mesh_file: &Path) -> Result<(), String> {
        std::fs::write(Self::file_for(mesh_file), serde_json::to_vec(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }
}

/// Distance d'un point au triangle (a, b, c) : le point le plus proche est cherché sur la face, ses côtés et ses coins
fn tri_dist(p: V3, a: V3, b: V3, c: V3) -> f32 {
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return len(ap);
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return len(bp);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return len(sub(ap, mul(ab, d1 / (d1 - d3).max(1e-30))));
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return len(cp);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return len(sub(ap, mul(ac, d2 / (d2 - d6).max(1e-30))));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6)).max(1e-30);
        return len(sub(bp, mul(sub(c, b), w)));
    }
    let den = (va + vb + vc).abs().max(1e-30);
    let (v, w) = (vb / den, vc / den);
    len(sub(ap, add(mul(ab, v), mul(ac, w))))
}

/// Remaillage dynamique : les faces que touche la boule (`c`, `r`) deviennent des triangles, et
/// leurs arêtes plus longues que `detail` sont coupées en deux, autant de fois qu'il faut (quelques
/// passes par touche). Un pinceau plus petit qu'une face la découpe donc aussi. Une face voisine
/// qui ne touche pas la boule reçoit le nouveau sommet sur son côté, sans fente. Rend le nombre
/// de sommets ajoutés (au plus `max_new`).
pub fn refine(m: &mut EMesh, c: V3, r: f32, detail: f32, max_new: usize) -> usize {
    if !(r > 0.0) || !(detail > 0.0) || !c.iter().all(|x| x.is_finite()) {
        return 0;
    }
    let nv0 = m.pos.len();
    // La face touche-t-elle la boule ? (un sommet dedans, sinon la distance à ses triangles en éventail)
    let touches = |pos: &[V3], p: &[u32]| -> bool {
        if p.len() < 3 || p.iter().any(|&v| v as usize >= pos.len()) {
            return false;
        }
        p.iter().any(|&v| len(sub(pos[v as usize], c)) < r) || (1..p.len() - 1).any(|k| tri_dist(c, pos[p[0] as usize], pos[p[k] as usize], pos[p[k + 1] as usize]) < r)
    };
    for _ in 0..4 {
        if m.pos.len() - nv0 >= max_new {
            break;
        }
        // 1. Les faces qui touchent la boule deviennent des triangles
        let np = m.polys.len();
        for f in 0..np {
            if m.polys[f].len() <= 3 || !touches(&m.pos, &m.polys[f]) {
                continue;
            }
            let p = m.polys[f].clone();
            if p.len() == 4 {
                // Le quad se coupe par sa diagonale la plus courte
                let d02 = len(sub(m.pos[p[0] as usize], m.pos[p[2] as usize]));
                let d13 = len(sub(m.pos[p[1] as usize], m.pos[p[3] as usize]));
                let (a, b) = if d02 <= d13 { ([p[0], p[1], p[2]], [p[0], p[2], p[3]]) } else { ([p[0], p[1], p[3]], [p[1], p[2], p[3]]) };
                m.polys[f] = a.to_vec();
                m.polys.push(b.to_vec());
            } else {
                m.polys[f] = vec![p[0], p[1], p[2]];
                for k in 2..p.len() - 1 {
                    m.polys.push(vec![p[0], p[k], p[k + 1]]);
                }
            }
        }
        // 2. Les arêtes trop longues de ces triangles : un sommet au milieu
        let mut mids: HashMap<(u32, u32), u32> = HashMap::new();
        let mut touched = vec![false; m.pos.len()];
        for f in 0..m.polys.len() {
            if m.polys[f].len() != 3 || !touches(&m.pos, &m.polys[f]) {
                continue;
            }
            let p = [m.polys[f][0], m.polys[f][1], m.polys[f][2]];
            for i in 0..3 {
                let (a, b) = (p[i], p[(i + 1) % 3]);
                let key = (a.min(b), a.max(b));
                let (pa, pb) = (m.pos[a as usize], m.pos[b as usize]);
                if len(sub(pa, pb)) > detail && !mids.contains_key(&key) && m.pos.len() - nv0 < max_new {
                    mids.insert(key, m.pos.len() as u32);
                    m.pos.push(mul(add(pa, pb), 0.5));
                    touched[a as usize] = true;
                    touched[b as usize] = true;
                }
            }
        }
        if mids.is_empty() {
            break;
        }
        // 3. Chaque face qui porte une arête coupée reçoit le sommet ; les triangles sont redécoupés
        let np = m.polys.len();
        for f in 0..np {
            let p = &m.polys[f];
            if !p.iter().any(|&v| touched.get(v as usize).copied().unwrap_or(false)) {
                continue;
            }
            let k = p.len();
            let mut lp: Vec<(u32, bool)> = Vec::with_capacity(k * 2);
            for i in 0..k {
                let (a, b) = (p[i], p[(i + 1) % k]);
                lp.push((a, false));
                if let Some(&mv) = mids.get(&(a.min(b), a.max(b))) {
                    lp.push((mv, true));
                }
            }
            if lp.len() == k {
                continue;
            }
            if k != 3 {
                m.polys[f] = lp.into_iter().map(|x| x.0).collect();
                continue;
            }
            let mut out: Vec<[u32; 3]> = vec![];
            // Un coin pris entre deux milieux se détache en triangle
            while lp.len() > 3 {
                let n = lp.len();
                let Some(i) = (0..n).find(|&i| !lp[i].1 && lp[(i + n - 1) % n].1 && lp[(i + 1) % n].1) else { break };
                out.push([lp[(i + n - 1) % n].0, lp[i].0, lp[(i + 1) % n].0]);
                lp.remove(i);
            }
            if lp.len() == 3 {
                out.push([lp[0].0, lp[1].0, lp[2].0]);
            } else if lp.len() == 4 {
                // Ce qui reste est un quad : la diagonale la plus courte qui ne fait pas de triangle plat
                let q: Vec<u32> = lp.iter().map(|x| x.0).collect();
                let area = |a: u32, b: u32, cc: u32| len(cross(sub(m.pos[b as usize], m.pos[a as usize]), sub(m.pos[cc as usize], m.pos[a as usize])));
                let total = area(q[0], q[1], q[2]) + area(q[0], q[2], q[3]);
                let d = |i: usize, j: usize| len(sub(m.pos[q[i] as usize], m.pos[q[j] as usize]));
                let ok02 = area(q[0], q[1], q[2]).min(area(q[0], q[2], q[3])) > total * 1e-4;
                let ok13 = area(q[0], q[1], q[3]).min(area(q[1], q[2], q[3])) > total * 1e-4;
                if ok02 && (!ok13 || d(0, 2) <= d(1, 3)) {
                    out.extend([[q[0], q[1], q[2]], [q[0], q[2], q[3]]]);
                } else {
                    out.extend([[q[0], q[1], q[3]], [q[1], q[2], q[3]]]);
                }
            } else {
                // Ne devrait pas arriver pour un triangle : on garde le polygone entier plutôt que de trouer
                m.polys[f] = lp.into_iter().map(|x| x.0).collect();
                continue;
            }
            m.polys[f] = out[0].to_vec();
            m.polys.extend(out[1..].iter().map(|t| t.to_vec()));
        }
    }
    m.pos.len() - nv0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> EMesh {
        let pos = (0..8).map(|i| [if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 }, if i & 4 == 0 { -1.0 } else { 1.0 }]).collect();
        let polys = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]].iter().map(|q| q.to_vec()).collect();
        EMesh { pos, polys }
    }

    fn grid(n: usize) -> EMesh {
        let mut pos = vec![];
        for j in 0..=n {
            for i in 0..=n {
                pos.push([i as f32 / n as f32, 0.0, j as f32 / n as f32]);
            }
        }
        let mut polys = vec![];
        for j in 0..n {
            for i in 0..n {
                let a = (j * (n + 1) + i) as u32;
                polys.push(vec![a, a + n as u32 + 1, a + n as u32 + 2, a + 1]);
            }
        }
        EMesh { pos, polys }
    }

    /// Nombre de faces par arête : (arêtes de bord, arêtes partagées par plus de deux faces)
    fn edge_use(m: &EMesh) -> (usize, usize) {
        let mut n: HashMap<(u32, u32), u32> = HashMap::new();
        for p in &m.polys {
            for i in 0..p.len() {
                let (a, b) = (p[i], p[(i + 1) % p.len()]);
                *n.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        (n.values().filter(|&&c| c == 1).count(), n.values().filter(|&&c| c > 2).count())
    }

    fn area(m: &EMesh) -> f32 {
        m.polys.iter().map(|p| (1..p.len() - 1).map(|k| len(cross(sub(m.pos[p[k] as usize], m.pos[p[0] as usize]), sub(m.pos[p[k + 1] as usize], m.pos[p[0] as usize]))) * 0.5).sum::<f32>()).sum()
    }

    #[test]
    fn subdivision_stable_et_fermee() {
        let c = cube();
        let s = subdivide(&c);
        assert_eq!((s.pos.len(), s.polys.len()), (8 + 6 + 12, 24));
        assert_eq!(edge_use(&s), (0, 0), "un cube subdivisé reste fermé");
        // Même entrée, même ordre de sommets (les détails par sommet en dépendent)
        assert_eq!(s, subdivide(&c));
        // Le cube s'arrondit : les coins rentrent
        assert!(s.pos[..8].iter().all(|p| p[0].abs() < 0.99));
        // Bord ouvert : la grille reste plate et ne déborde pas ; un sommet au milieu d'un bord droit y reste
        let g = subdivide(&grid(2));
        assert!(g.pos.iter().all(|p| p[1] == 0.0 && (-1e-6..=1.0 + 1e-6).contains(&p[0])));
        assert!(g.pos[1][2].abs() < 1e-6 && (g.pos[1][0] - 0.5).abs() < 1e-6);
        // Maillage vide, face invalide : rien ne panique
        assert_eq!(subdivide(&EMesh::default()).pos.len(), 0);
        let bad = subdivide(&EMesh { pos: vec![[0.0; 3]; 3], polys: vec![vec![0, 1], vec![0, 1, 9], vec![0, 1, 2]] });
        assert_eq!(bad.polys.len(), 3);
    }

    #[test]
    fn niveaux_sans_perdre_le_detail() {
        let mut mr = Multires::new(&cube());
        let l1 = mr.add_level().unwrap();
        assert_eq!((mr.current, mr.top(), l1.polys.len()), (1, 1, 24));
        let mut l2 = mr.add_level().unwrap();
        assert_eq!((mr.current, l2.polys.len()), (2, 96));
        // Un détail au niveau 2 : un sommet sort de 0,2 le long de sa normale
        let k = 40;
        let n = crate::sculpt::normals(&l2)[k];
        let before = l2.pos[k];
        l2.pos[k] = add(before, mul(n, 0.2));
        assert!(mr.matches(&l2));
        assert!(mr.store(&l2));
        // Descendre à la base : le maillage redevient le cube, et le détail est toujours rangé
        let mut base = mr.go(0);
        assert_eq!(base.pos.len(), 8);
        // On étire la base (tout ce qui est en +X part plus loin), puis on remonte
        for p in base.pos.iter_mut().filter(|p| p[0] > 0.0) {
            p[0] += 1.0;
        }
        assert!(mr.store(&base));
        let up = mr.go(2);
        assert_eq!(up.pos.len(), l2.pos.len());
        // La forme a suivi la base…
        let (max0, max1) = (l2.pos.iter().map(|p| p[0]).fold(f32::MIN, f32::max), up.pos.iter().map(|p| p[0]).fold(f32::MIN, f32::max));
        assert!(max1 > max0 + 0.5, "{max0} {max1}");
        // … et le détail est toujours là : le sommet dépasse encore sa surface lisse d'environ 0,2
        let mut flat = mr.clone();
        flat.levels[1] = vec![[0.0; 3]; up.pos.len()];
        let smooth = flat.mesh_at(2);
        let d = len(sub(up.pos[k], smooth.pos[k]));
        assert!((d - 0.2).abs() < 0.02, "{d}");
        let others = (0..up.pos.len()).filter(|&i| i != k).map(|i| len(sub(up.pos[i], smooth.pos[i]))).fold(0.0f32, f32::max);
        assert!(others < 1e-4, "{others}");
        // Un maillage d'une autre taille (topologie changée ailleurs) est refusé
        assert!(!mr.store(&cube()));
        assert!(!mr.matches(&cube()));
        // Même nombre de sommets et de faces, mais des faces tournées autrement : ce n'est plus ce maillage
        let mut turned = up.clone();
        turned.polys[0].rotate_left(1);
        assert!(mr.matches(&up) && !mr.matches(&turned));
    }

    #[test]
    fn niveaux_sur_le_disque() {
        let dir = std::env::temp_dir().join(format!("ary-multires-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("objet.obj");
        let mut mr = Multires::new(&cube());
        mr.add_level().unwrap();
        mr.save(&f).unwrap();
        assert_eq!(Multires::load(&f), Some(mr));
        // Fichier abîmé : refusé sans panique
        std::fs::write(Multires::file_for(&f), br#"{"base_pos":[[0,0,0]],"base_polys":[[0,5,7]],"levels":[],"current":0}"#).unwrap();
        assert_eq!(Multires::load(&f), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn remaillage_sous_le_pinceau() {
        let mut m = grid(4);
        let (n0, a0) = (m.pos.len(), area(&m));
        let border0 = edge_use(&m).0;
        let added = refine(&mut m, [0.5, 0.0, 0.5], 0.3, 0.06, 100_000);
        assert!(added > 20 && m.pos.len() == n0 + added, "{added}");
        // Pas de fente ni de face en trop : chaque arête a au plus deux faces, et l'aire est la même
        let (border, over) = edge_use(&m);
        assert_eq!(over, 0);
        assert!(border >= border0);
        assert!((area(&m) - a0).abs() < 1e-4, "{} {a0}", area(&m));
        // Plusieurs touches au même endroit finissent par s'arrêter : plus d'arête trop longue au cœur de la boule
        for _ in 0..6 {
            refine(&mut m, [0.5, 0.0, 0.5], 0.3, 0.06, 100_000);
        }
        assert_eq!(refine(&mut m, [0.5, 0.0, 0.5], 0.3, 0.06, 100_000), 0);
        for p in m.polys.iter().filter(|p| p.len() == 3) {
            let q = [0, 1, 2].map(|i| m.pos[p[i] as usize]);
            if tri_dist([0.5, 0.0, 0.5], q[0], q[1], q[2]) < 0.3 {
                assert!((0..3).all(|i| len(sub(q[i], q[(i + 1) % 3])) <= 0.06 + 1e-5));
            }
        }
        // Un pinceau plus petit qu'une face la découpe quand même
        let mut big = grid(1);
        assert!(refine(&mut big, [0.4, 0.0, 0.3], 0.05, 0.05, 100_000) > 0);
        assert!((area(&big) - 1.0).abs() < 1e-4 && edge_use(&big).1 == 0);
        assert!((tri_dist([0.25, 1.0, 0.25], [0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]) - 1.0).abs() < 1e-6);
        assert!((tri_dist([2.0, 0.0, 0.0], [0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]) - 1.0).abs() < 1e-6);
        assert!((area(&m) - a0).abs() < 1e-4);
        // Loin de la boule, les faces ne sont pas découpées : le coin n'est pas devenu des triangles
        assert!(m.polys.iter().any(|p| p.len() >= 4 && p.contains(&0)));
        // Le plafond de sommets est respecté ; un rayon nul ou un centre NaN ne font rien
        let mut g = grid(4);
        assert!(refine(&mut g, [0.5, 0.0, 0.5], 0.4, 0.001, 50) <= 50);
        assert_eq!(refine(&mut g, [f32::NAN, 0.0, 0.0], 0.4, 0.01, 50), 0);
        assert_eq!(refine(&mut g, [0.5, 0.0, 0.5], 0.0, 0.01, 50), 0);
    }
}
