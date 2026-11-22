//! Maillage modifiable du mode édition : des sommets et des polygones (quads et n-gones gardés tels quels),
//! lus et écrits en OBJ pour rester ouvrables ailleurs. Les opérations (extruder, supprimer, remplir)
//! travaillent sur une sélection de sommets.

use std::collections::{HashMap, HashSet};
use std::path::Path;

type V3 = [f32; 3];

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn unit(a: V3) -> V3 {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    if l > 1e-12 { [a[0] / l, a[1] / l, a[2] / l] } else { [0.0, 1.0, 0.0] }
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct EMesh {
    pub pos: Vec<V3>,
    pub polys: Vec<Vec<u32>>,
}

impl EMesh {
    /// Retrouve les polygones d'un modèle chargé (ses triangles en éventail et leurs côtés marqués),
    /// Y en haut, en soudant les sommets posés au même endroit pour que les faces se tiennent.
    pub fn from_model(m: &crate::model3d::Model) -> EMesh {
        let mut polys: Vec<Vec<u32>> = vec![];
        let mut open: Option<Vec<u32>> = None;
        for (ti, t) in m.tris.iter().enumerate() {
            let mask = m.edges.get(ti).copied().unwrap_or(7);
            match open.take() {
                // Suite d'un éventail : même premier sommet, et le triangle repart du dernier sommet posé
                Some(mut p) if p[0] == t[0] && p.last() == Some(&t[1]) && mask & 1 == 0 => {
                    p.push(t[2]);
                    if mask & 4 != 0 { polys.push(p) } else { open = Some(p) }
                }
                prev => {
                    polys.extend(prev);
                    if mask & 4 != 0 || mask & 1 == 0 { polys.push(t.to_vec()) } else { open = Some(t.to_vec()) }
                }
            }
        }
        polys.extend(open);
        let pos: Vec<V3> = m.positions.iter().map(|p| if m.z_up { [p[0], p[2], -p[1]] } else { *p }).collect();
        let mut e = EMesh { pos, polys };
        e.weld();
        e
    }

    /// Fusionne les sommets posés au même endroit et retire ceux qui ne servent plus.
    pub fn weld(&mut self) {
        let size = self.pos.iter().flatten().fold(0.0f32, |a, x| a.max(x.abs())).max(1e-6);
        let q = |p: &V3| p.map(|x| (x / size * 200_000.0).round() as i64);
        let mut first: HashMap<[i64; 3], u32> = HashMap::new();
        let mut map = vec![0u32; self.pos.len()];
        let mut pos = vec![];
        let used: HashSet<u32> = self.polys.iter().flatten().copied().collect();
        for (i, p) in self.pos.iter().enumerate() {
            if !used.contains(&(i as u32)) {
                continue;
            }
            map[i] = *first.entry(q(p)).or_insert_with(|| {
                pos.push(*p);
                pos.len() as u32 - 1
            });
        }
        for p in &mut self.polys {
            for v in p.iter_mut() {
                *v = map[*v as usize];
            }
            p.dedup();
            if p.len() > 1 && p.first() == p.last() {
                p.pop();
            }
        }
        self.polys.retain(|p| p.len() >= 3);
        self.pos = pos;
    }

    pub fn read_obj(path: &Path) -> Result<EMesh, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut m = EMesh::default();
        for line in text.lines() {
            let mut it = line.split_whitespace();
            match it.next() {
                Some("v") => {
                    let v: Vec<f32> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                    if v.len() == 3 {
                        m.pos.push([v[0], v[1], v[2]]);
                    }
                }
                Some("f") => {
                    let n = m.pos.len() as i64;
                    let p: Vec<u32> = it
                        .filter_map(|x| x.split('/').next()?.parse::<i64>().ok())
                        .map(|i| if i < 0 { n + i } else { i - 1 })
                        .filter(|i| *i >= 0 && *i < n)
                        .map(|i| i as u32)
                        .collect();
                    if p.len() >= 3 {
                        m.polys.push(p);
                    }
                }
                _ => {}
            }
        }
        Ok(m)
    }

    pub fn write_obj(&self, path: &Path) -> Result<(), String> {
        let mut t = String::with_capacity(self.pos.len() * 32 + self.polys.len() * 16);
        t += "# Aryadne\n";
        for p in &self.pos {
            t += &format!("v {} {} {}\n", p[0], p[1], p[2]);
        }
        for p in &self.polys {
            t += "f";
            for i in p {
                t += &format!(" {}", i + 1);
            }
            t += "\n";
        }
        std::fs::write(path, t).map_err(|e| e.to_string())
    }

    /// Normale d'un polygone (méthode de Newell : juste aussi pour un polygone qui n'est pas tout à fait plat).
    pub fn normal(&self, p: &[u32]) -> V3 {
        let mut n = [0.0f32; 3];
        for k in 0..p.len() {
            let (a, b) = (self.pos[p[k] as usize], self.pos[p[(k + 1) % p.len()] as usize]);
            n[0] += (a[1] - b[1]) * (a[2] + b[2]);
            n[1] += (a[2] - b[2]) * (a[0] + b[0]);
            n[2] += (a[0] - b[0]) * (a[1] + b[1]);
        }
        unit(n)
    }

    /// Les arêtes du maillage, chacune une seule fois.
    pub fn edges(&self) -> Vec<(u32, u32)> {
        let mut seen = HashSet::new();
        for p in &self.polys {
            for k in 0..p.len() {
                let (a, b) = (p[k], p[(k + 1) % p.len()]);
                seen.insert((a.min(b), a.max(b)));
            }
        }
        seen.into_iter().collect()
    }

    /// Supprime les sommets choisis et les faces qui s'en servaient.
    pub fn delete(&mut self, sel: &[bool]) {
        self.polys.retain(|p| !p.iter().any(|&v| sel.get(v as usize).copied().unwrap_or(false)));
        let mut map = vec![u32::MAX; self.pos.len()];
        let mut pos = vec![];
        for (i, p) in self.pos.iter().enumerate() {
            if !sel.get(i).copied().unwrap_or(false) {
                map[i] = pos.len() as u32;
                pos.push(*p);
            }
        }
        for p in &mut self.polys {
            for v in p.iter_mut() {
                *v = map[*v as usize];
            }
        }
        self.pos = pos;
    }

    /// Extrude les faces dont tous les sommets sont choisis : elles se détachent sur de nouveaux sommets,
    /// reliées à leur ancien contour par des faces de côté. Renvoie la nouvelle sélection et la direction
    /// moyenne des faces, ou None s'il n'y a aucune face entièrement choisie.
    pub fn extrude(&mut self, sel: &[bool]) -> Option<(Vec<bool>, V3)> {
        let on = |v: u32| sel.get(v as usize).copied().unwrap_or(false);
        let faces: Vec<usize> = (0..self.polys.len()).filter(|&f| self.polys[f].iter().all(|&v| on(v))).collect();
        if faces.is_empty() {
            return None;
        }
        let mut dir = [0.0f32; 3];
        let mut inner: HashSet<(u32, u32)> = HashSet::new();
        for &f in &faces {
            let n = self.normal(&self.polys[f]);
            dir = [dir[0] + n[0], dir[1] + n[1], dir[2] + n[2]];
            let p = &self.polys[f];
            for k in 0..p.len() {
                inner.insert((p[k], p[(k + 1) % p.len()]));
            }
        }
        // Le contour : les arêtes des faces choisies dont l'autre côté n'est pas une face choisie
        let border: Vec<(u32, u32)> = inner.iter().copied().filter(|&(a, b)| !inner.contains(&(b, a))).collect();
        let mut copy: HashMap<u32, u32> = HashMap::new();
        for &f in &faces {
            for k in 0..self.polys[f].len() {
                let v = self.polys[f][k];
                let n = *copy.entry(v).or_insert_with(|| {
                    self.pos.push(self.pos[v as usize]);
                    self.pos.len() as u32 - 1
                });
                self.polys[f][k] = n;
            }
        }
        for (a, b) in border {
            self.polys.push(vec![a, b, copy[&b], copy[&a]]);
        }
        let mut out = vec![false; self.pos.len()];
        for n in copy.values() {
            out[*n as usize] = true;
        }
        // Un sommet d'origine qui ne sert plus à rien (face isolée extrudée) reste : il porte les côtés
        Some((out, unit(dir)))
    }

    /// Crée une face avec ces sommets, pris dans cet ordre. Refusée si elle existe déjà.
    pub fn fill(&mut self, order: &[u32]) -> bool {
        if order.len() < 3 {
            return false;
        }
        let key = |p: &[u32]| {
            let mut k = p.to_vec();
            k.sort_unstable();
            k
        };
        if self.polys.iter().any(|p| key(p) == key(order)) {
            return false;
        }
        self.polys.push(order.to_vec());
        true
    }

    /// Faces dont tous les sommets sont choisis.
    pub fn faces_in(&self, sel: &[bool]) -> Vec<usize> {
        let on = |v: u32| sel.get(v as usize).copied().unwrap_or(false);
        (0..self.polys.len()).filter(|&f| self.polys[f].iter().all(|&v| on(v))).collect()
    }

    /// Insère une face plus petite dans chaque face choisie (I, comme dans Blender), reliée au bord par
    /// des quadrilatères. `t` : de 0 (rien) à 1 (jusqu'au centre). Renvoie la sélection des nouvelles faces.
    pub fn inset(&mut self, sel: &[bool], t: f32) -> Option<Vec<bool>> {
        let faces = self.faces_in(sel);
        if faces.is_empty() {
            return None;
        }
        let t = t.clamp(0.01, 0.95);
        let mut out = vec![false; self.pos.len()];
        let mut extra: Vec<Vec<u32>> = vec![];
        for &f in &faces {
            let p = self.polys[f].clone();
            let n = p.len() as f32;
            let c = p.iter().fold([0.0f32; 3], |a, &v| {
                let q = self.pos[v as usize];
                [a[0] + q[0] / n, a[1] + q[1] / n, a[2] + q[2] / n]
            });
            let inner: Vec<u32> = p.iter().map(|&v| {
                let q = self.pos[v as usize];
                self.pos.push([q[0] + (c[0] - q[0]) * t, q[1] + (c[1] - q[1]) * t, q[2] + (c[2] - q[2]) * t]);
                out.push(true);
                self.pos.len() as u32 - 1
            }).collect();
            for k in 0..p.len() {
                let k2 = (k + 1) % p.len();
                extra.push(vec![p[k], p[k2], inner[k2], inner[k]]);
            }
            self.polys[f] = inner;
        }
        self.polys.extend(extra);
        Some(out)
    }

    /// Fusionne les sommets choisis en un seul, à leur centre (M). Les faces qui s'aplatissent disparaissent.
    pub fn merge(&mut self, sel: &[bool]) -> Option<Vec<bool>> {
        let chosen: Vec<usize> = (0..self.pos.len()).filter(|&i| sel.get(i).copied().unwrap_or(false)).collect();
        if chosen.len() < 2 {
            return None;
        }
        let n = chosen.len() as f32;
        let c = chosen.iter().fold([0.0f32; 3], |a, &i| [a[0] + self.pos[i][0] / n, a[1] + self.pos[i][1] / n, a[2] + self.pos[i][2] / n]);
        let keep = chosen[0] as u32;
        self.pos[keep as usize] = c;
        for p in &mut self.polys {
            for v in p.iter_mut() {
                if sel.get(*v as usize).copied().unwrap_or(false) {
                    *v = keep;
                }
            }
            p.dedup();
            while p.len() > 1 && p.first() == p.last() {
                p.pop();
            }
        }
        self.polys.retain(|p| p.len() >= 3);
        // Les autres sommets choisis ne servent plus : on les retire
        let mut gone = vec![false; self.pos.len()];
        for &i in &chosen[1..] {
            gone[i] = true;
        }
        let mut map = vec![u32::MAX; self.pos.len()];
        let mut pos = vec![];
        for (i, q) in self.pos.iter().enumerate() {
            if !gone[i] {
                map[i] = pos.len() as u32;
                pos.push(*q);
            }
        }
        for p in &mut self.polys {
            for v in p.iter_mut() {
                *v = map[*v as usize];
            }
        }
        self.pos = pos;
        let mut out = vec![false; self.pos.len()];
        out[map[keep as usize] as usize] = true;
        Some(out)
    }

    /// Tout ce qui touche la sélection par des faces (L / Ctrl+L) : la pièce entière.
    pub fn linked(&self, sel: &[bool]) -> Vec<bool> {
        let mut out: Vec<bool> = (0..self.pos.len()).map(|i| sel.get(i).copied().unwrap_or(false)).collect();
        loop {
            let mut grew = false;
            for p in &self.polys {
                if p.iter().any(|&v| out[v as usize]) && !p.iter().all(|&v| out[v as usize]) {
                    for &v in p {
                        out[v as usize] = true;
                    }
                    grew = true;
                }
            }
            if !grew {
                return out;
            }
        }
    }

    #[cfg(test)]
    pub fn triangles(&self) -> (Vec<[u32; 3]>, Vec<u8>) {
        let (mut tris, mut edges) = (vec![], vec![]);
        for p in &self.polys {
            for k in 1..p.len() - 1 {
                tris.push([p[0], p[k], p[k + 1]]);
                edges.push(crate::model3d::fan_mask(k, p.len()));
            }
        }
        (tris, edges)
    }

    /// Volume signé (positif si les faces regardent vers l'extérieur).
    #[cfg(test)]
    fn volume(&self) -> f32 {
        let (tris, _) = self.triangles();
        tris.iter().map(|t| {
            let (a, b, c) = (self.pos[t[0] as usize], self.pos[t[1] as usize], self.pos[t[2] as usize]);
            a[0] * cross(b, c)[0] + a[1] * cross(b, c)[1] + a[2] * cross(b, c)[2]
        }).sum::<f32>() / 6.0
    }
}

/// Inverse d'une matrice 3×3 donnée par ses colonnes (None si elle est aplatie).
pub fn inverse(c: [V3; 3]) -> Option<[V3; 3]> {
    let det = c[0][0] * cross(c[1], c[2])[0] + c[0][1] * cross(c[1], c[2])[1] + c[0][2] * cross(c[1], c[2])[2];
    if det.abs() < 1e-12 {
        return None;
    }
    let (r0, r1, r2) = (cross(c[1], c[2]), cross(c[2], c[0]), cross(c[0], c[1]));
    // Les lignes de l'inverse sont ces produits vectoriels divisés par le déterminant
    Some([[r0[0] / det, r1[0] / det, r2[0] / det], [r0[1] / det, r1[1] / det, r2[1] / det], [r0[2] / det, r1[2] / det, r2[2] / det]])
}

/// Applique une matrice donnée par ses colonnes.
pub fn apply(c: &[V3; 3], v: V3) -> V3 {
    [c[0][0] * v[0] + c[1][0] * v[1] + c[2][0] * v[2], c[0][1] * v[0] + c[1][1] * v[1] + c[2][1] * v[2], c[0][2] * v[0] + c[1][2] * v[1] + c[2][2] * v[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inset_fusion_lies() {
        let model = crate::scene::primitive("@cube").unwrap();
        let mut m = EMesh::from_model(&model);
        let (nv, nf) = (m.pos.len(), m.polys.len());
        // Une face choisie : quatre sommets de plus, quatre faces de plus
        let top = m.polys[0].clone();
        let mut sel = vec![false; nv];
        for &v in &top {
            sel[v as usize] = true;
        }
        let s2 = m.inset(&sel, 0.3).unwrap();
        assert_eq!((m.pos.len(), m.polys.len()), (nv + 4, nf + 4));
        assert_eq!(s2.iter().filter(|x| **x).count(), 4);
        // Fusionner les quatre sommets intérieurs : un seul sommet, la face du milieu disparaît
        let s3 = m.merge(&s2).unwrap();
        assert_eq!(m.pos.len(), nv + 1);
        assert_eq!(s3.iter().filter(|x| **x).count(), 1);
        assert!(m.polys.iter().all(|p| p.len() >= 3));
        // Lié : tout le cube depuis un seul sommet
        let mut one = vec![false; m.pos.len()];
        one[0] = true;
        assert!(m.linked(&one).iter().all(|x| *x));
    }

    #[test]
    fn cube_modifiable() {
        // Le cube de base : 24 sommets à l'affichage (faces nettes), 8 une fois soudé, 6 quads
        let model = crate::scene::primitive("@cube").unwrap();
        let mut m = EMesh::from_model(&model);
        assert_eq!((m.pos.len(), m.polys.len()), (8, 6));
        assert!(m.polys.iter().all(|p| p.len() == 4));
        assert_eq!(m.edges().len(), 12);
        assert!((m.volume() - 8.0).abs() < 1e-3);
        // Aller-retour en OBJ
        let f = std::env::temp_dir().join("aryadne-meshedit.obj");
        m.write_obj(&f).unwrap();
        assert_eq!(EMesh::read_obj(&f).unwrap(), m);
        // Extruder la face du dessus puis la monter d'une unité : le volume gagne 4 (2 × 2 × 1)
        let top: Vec<bool> = m.pos.iter().map(|p| p[1] > 0.5).collect();
        let (sel, dir) = m.extrude(&top).unwrap();
        assert!((dir[1] - 1.0).abs() < 1e-5);
        assert_eq!((m.pos.len(), m.polys.len(), sel.iter().filter(|s| **s).count()), (12, 10, 4));
        for (i, s) in sel.iter().enumerate() {
            if *s {
                m.pos[i][1] += 1.0;
            }
        }
        assert!((m.volume() - 12.0).abs() < 1e-3, "{}", m.volume());
        // Supprimer ces quatre sommets emporte la face du haut et ses quatre côtés : il reste un cube ouvert
        m.delete(&sel);
        assert_eq!((m.pos.len(), m.polys.len()), (8, 5));
        // Reboucher le trou avec les quatre sommets du haut ; la même face une seconde fois est refusée
        let ring: Vec<u32> = (0..8).filter(|&i| m.pos[i as usize][1] > 0.5).collect();
        assert!(m.fill(&ring) && !m.fill(&ring));
        // Une sélection sans face entière ne s'extrude pas
        let mut one = vec![false; m.pos.len()];
        one[0] = true;
        assert!(m.extrude(&one).is_none());
        // Les autres formes de base gardent leurs polygones (un cylindre : 32 quads et deux disques)
        let cyl = EMesh::from_model(&crate::scene::primitive("@cylinder").unwrap());
        assert_eq!(cyl.polys.iter().filter(|p| p.len() == 4).count(), 32);
        assert_eq!(cyl.polys.iter().filter(|p| p.len() == 32).count(), 2);
        // Matrice et inverse
        let c = [[2.0, 0.0, 0.0], [0.0, 0.0, 3.0], [0.0, -1.0, 0.0]];
        let back = apply(&inverse(c).unwrap(), apply(&c, [1.0, 2.0, 3.0]));
        assert!((back[0] - 1.0).abs() < 1e-5 && (back[1] - 2.0).abs() < 1e-5 && (back[2] - 3.0).abs() < 1e-5);
    }
}
