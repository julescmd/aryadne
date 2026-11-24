//! Maillage modifiable du mode édition : des sommets et des polygones (quads et n-gones gardés tels quels),
//! lus et écrits en OBJ pour rester ouvrables ailleurs. Les opérations (extruder, supprimer, remplir)
//! travaillent sur une sélection de sommets.

use std::collections::{HashMap, HashSet};
use std::path::Path;

type V3 = [f32; 3];

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
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
        crate::vault::atomic_write(path, t).map_err(|e| e.to_string())
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

    /// Voisins de chaque sommet (par les arêtes).
    pub fn neighbors(&self) -> Vec<Vec<u32>> {
        let mut n: Vec<Vec<u32>> = vec![vec![]; self.pos.len()];
        for (a, b) in self.edges() {
            n[a as usize].push(b);
            n[b as usize].push(a);
        }
        n
    }

    /// Le plus court chemin de `a` à `b` en suivant les arêtes (Ctrl+clic), longueurs réelles comprises.
    pub fn shortest_path(&self, a: u32, b: u32) -> Vec<u32> {
        let n = self.neighbors();
        let len = |x: u32, y: u32| {
            let (p, q) = (self.pos[x as usize], self.pos[y as usize]);
            ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
        };
        let mut dist = vec![f32::INFINITY; self.pos.len()];
        let mut prev = vec![u32::MAX; self.pos.len()];
        let mut done = vec![false; self.pos.len()];
        if a as usize >= dist.len() || b as usize >= dist.len() {
            return vec![];
        }
        dist[a as usize] = 0.0;
        // Dijkstra simple : les maillages qu'on modifie à la main restent petits
        let mut heap = std::collections::BinaryHeap::new();
        heap.push((std::cmp::Reverse(0u64), a));
        while let Some((_, v)) = heap.pop() {
            if done[v as usize] {
                continue;
            }
            done[v as usize] = true;
            if v == b {
                break;
            }
            for &w in &n[v as usize] {
                let d = dist[v as usize] + len(v, w);
                if d < dist[w as usize] {
                    dist[w as usize] = d;
                    prev[w as usize] = v;
                    heap.push((std::cmp::Reverse((d * 1e6) as u64), w));
                }
            }
        }
        if !dist[b as usize].is_finite() {
            return vec![];
        }
        let mut path = vec![b];
        let mut v = b;
        while v != a {
            v = prev[v as usize];
            path.push(v);
        }
        path.reverse();
        path
    }

    /// Boucle d'arêtes passant par l'arête (a, b) (Alt+clic) : on traverse les sommets à quatre voisins
    /// en prenant chaque fois l'arête d'en face, comme dans Blender.
    pub fn edge_loop(&self, a: u32, b: u32) -> Vec<u32> {
        let n = self.neighbors();
        // Faces qui touchent une arête
        let faces_of = |x: u32, y: u32| -> Vec<usize> { (0..self.polys.len()).filter(|&f| self.polys[f].contains(&x) && self.polys[f].contains(&y)).collect() };
        let walk = |from: u32, to: u32| -> Vec<u32> {
            let mut out = vec![];
            let (mut u, mut v) = (from, to);
            for _ in 0..self.pos.len() {
                if n[v as usize].len() != 4 {
                    break;
                }
                let fa = faces_of(u, v);
                // L'arête suivante ne partage aucune face avec celle d'où l'on vient
                let next = n[v as usize].iter().copied().find(|&w| w != u && faces_of(v, w).iter().all(|f| !fa.contains(f)));
                let Some(w) = next else { break };
                if w == from || out.contains(&w) {
                    break;
                }
                out.push(w);
                (u, v) = (v, w);
            }
            out
        };
        let mut fwd = walk(a, b);
        let back = walk(b, a);
        let mut all: Vec<u32> = back.into_iter().rev().collect();
        all.push(a);
        all.push(b);
        all.append(&mut fwd);
        all.dedup();
        all
    }

    /// Étend la sélection d'un cran (Ctrl+plus) : les voisins des sommets choisis.
    pub fn grow(&self, sel: &[bool]) -> Vec<bool> {
        let mut out: Vec<bool> = (0..self.pos.len()).map(|i| sel.get(i).copied().unwrap_or(false)).collect();
        for (a, b) in self.edges() {
            let (sa, sb) = (sel.get(a as usize).copied().unwrap_or(false), sel.get(b as usize).copied().unwrap_or(false));
            if sa || sb {
                out[a as usize] = true;
                out[b as usize] = true;
            }
        }
        out
    }

    /// Réduit la sélection d'un cran (Ctrl+moins) : on retire les sommets du bord de la sélection.
    pub fn shrink(&self, sel: &[bool]) -> Vec<bool> {
        let on = |i: u32| sel.get(i as usize).copied().unwrap_or(false);
        let mut out: Vec<bool> = (0..self.pos.len()).map(|i| on(i as u32)).collect();
        for (a, b) in self.edges() {
            if on(a) != on(b) {
                out[a as usize] = false;
                out[b as usize] = false;
            }
        }
        out
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

    /// Faces autour d'un sommet, dans l'ordre où l'on tourne : (face, sommet d'avant, sommet d'après) ;
    /// et si l'éventail est ouvert (sommet de bord). None si le voisinage n'est pas une surface simple.
    fn fan(&self, v: u32, faces_of: &[Vec<usize>]) -> Option<(Vec<(usize, u32, u32)>, bool)> {
        let around: Vec<(usize, u32, u32)> = faces_of[v as usize].iter().filter_map(|&f| {
            let p = &self.polys[f];
            let k = p.iter().position(|&x| x == v)?;
            Some((f, p[(k + p.len() - 1) % p.len()], p[(k + 1) % p.len()]))
        }).collect();
        if around.is_empty() {
            return None;
        }
        // Une face suit une autre quand son sommet d'avant est le sommet d'après de l'autre
        let succ = |a: &(usize, u32, u32)| around.iter().position(|b| b.1 == a.2);
        let pred = |a: &(usize, u32, u32)| around.iter().position(|b| b.2 == a.1);
        // Départ : une face sans prédécesseur (bord), sinon n'importe laquelle
        let start = (0..around.len()).find(|&i| pred(&around[i]).is_none());
        let open = start.is_some();
        let mut order = vec![start.unwrap_or(0)];
        loop {
            match succ(&around[*order.last().unwrap()]) {
                Some(n) if n == order[0] => break,
                Some(n) if order.contains(&n) => return None,
                Some(n) => order.push(n),
                None => break,
            }
        }
        if order.len() != around.len() {
            return None;
        }
        Some((order.into_iter().map(|i| around[i]).collect(), open))
    }

    /// Le plus grand écart de biseau possible sans que deux coupes se croisent.
    pub fn bevel_limit(&self, sel: &[bool]) -> f32 {
        let on = |v: u32| sel.get(v as usize).copied().unwrap_or(false);
        let mut lim = f32::MAX;
        let len = |a: u32, b: u32| {
            let d = sub(self.pos[a as usize], self.pos[b as usize]);
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        };
        for p in &self.polys {
            for k in 0..p.len() {
                let (a, b) = (p[k], p[(k + 1) % p.len()]);
                if on(a) || on(b) {
                    lim = lim.min(len(a, b) * if on(a) && on(b) { 0.5 } else { 0.95 });
                }
            }
        }
        if lim == f32::MAX { 0.0 } else { lim * 0.98 }
    }

    /// Biseau (Ctrl+B) des arêtes dont les deux bouts sont choisis : chaque arête devient une bande de
    /// `segments` faces arrondies, large de `width`. Les coins où se rencontrent trois arêtes ou plus
    /// reçoivent une face. Renvoie la sélection des nouvelles faces.
    pub fn bevel(&mut self, sel: &[bool], width: f32, segments: u32) -> Option<Vec<bool>> {
        let on = |v: u32| sel.get(v as usize).copied().unwrap_or(false);
        let segments = segments.clamp(1, 64) as usize;
        let mut faces_of: Vec<Vec<usize>> = vec![vec![]; self.pos.len()];
        for (f, p) in self.polys.iter().enumerate() {
            for &v in p {
                faces_of[v as usize].push(f);
            }
        }
        // Arêtes à biseauter : bouts choisis et deux faces de part et d'autre
        let mut count: HashMap<(u32, u32), u32> = HashMap::new();
        for p in &self.polys {
            for k in 0..p.len() {
                let (a, b) = (p[k], p[(k + 1) % p.len()]);
                *count.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let bev: HashSet<(u32, u32)> = count.iter().filter(|(e, n)| **n == 2 && on(e.0) && on(e.1)).map(|(e, _)| *e).collect();
        if bev.is_empty() || width <= 0.0 {
            return None;
        }
        let is_bev = |a: u32, b: u32| bev.contains(&(a.min(b), a.max(b)));
        let dir = |pos: &[V3], v: u32, w: u32| unit(sub(pos[w as usize], pos[v as usize]));
        let base = self.pos.clone();
        let sin_between = |a: V3, b: V3| {
            let c = cross(a, b);
            (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt().max(0.15)
        };
        // Pour chaque sommet touché : le sommet qui remplace v dans chaque face, et les sommets de part
        // et d'autre de chaque arête biseautée : (v, voisin) -> (côté d'avant, côté d'après)
        let mut in_face: HashMap<(usize, u32), Vec<u32>> = HashMap::new();
        let mut sides: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
        let mut corners: Vec<(u32, Vec<(u32, u32, u32)>, bool)> = vec![];
        let new_vert = |pos: &mut Vec<V3>, p: V3| {
            pos.push(p);
            pos.len() as u32 - 1
        };
        let verts: Vec<u32> = {
            let mut s: Vec<u32> = bev.iter().flat_map(|e| [e.0, e.1]).collect();
            s.sort_unstable();
            s.dedup();
            s
        };
        for &v in &verts {
            let Some((fan, open)) = self.fan(v, &faces_of) else { continue };
            let m = fan.len();
            // Arêtes autour : E_i = (v, avant de la face i) ; un éventail ouvert a une arête de plus au bout
            let mut edges: Vec<u32> = fan.iter().map(|x| x.1).collect();
            if open {
                edges.push(fan[m - 1].2);
            }
            let bevd: Vec<usize> = (0..edges.len()).filter(|&i| is_bev(v, edges[i]) && (!open || (i > 0 && i < m))).collect();
            if bevd.is_empty() {
                continue;
            }
            let pv = base[v as usize];
            let along = |e: u32, against: u32| -> V3 {
                let d = dir(&base, v, e);
                let s = sin_between(d, dir(&base, v, against));
                [pv[0] + d[0] * width / s, pv[1] + d[1] * width / s, pv[2] + d[2] * width / s]
            };
            if !open && bevd.len() == 1 {
                // Bout d'une suite d'arêtes : deux sommets glissent sur les arêtes voisines
                let s = bevd[0];
                if m < 3 {
                    continue;
                }
                let e_after = edges[(s + 1) % m];
                let e_before = edges[(s + m - 1) % m];
                let va = new_vert(&mut self.pos, along(e_after, edges[s]));
                let vb = new_vert(&mut self.pos, along(e_before, edges[s]));
                in_face.insert((fan[s].0, v), vec![va]);
                in_face.insert((fan[(s + m - 1) % m].0, v), vec![vb]);
                sides.insert((v, edges[s]), (vb, va));
                let mid1 = fan[(s + 1) % m].0;
                let mid2 = fan[(s + m - 2) % m].0;
                if m == 3 {
                    // La face d'en face : v disparaît, remplacé par le profil (rempli plus bas)
                    corners.push((v, vec![(edges[s], vb, va)], false));
                    in_face.insert((mid1, v), vec![u32::MAX, va, vb]);
                } else {
                    in_face.insert((mid1, v), vec![va, v]);
                    in_face.insert((mid2, v), vec![v, vb]);
                    corners.push((v, vec![(edges[s], vb, va)], true));
                }
                continue;
            }
            // Secteurs : les faces entre deux arêtes biseautées (ou le bord) partagent un sommet
            let nb = bevd.len();
            let mut sector_vert: Vec<Option<u32>> = vec![None; m];
            let mut list: Vec<(u32, u32, u32)> = vec![];
            let mut sector_bounds: Vec<(usize, usize)> = vec![];
            if open {
                let mut b = vec![0usize];
                b.extend(&bevd);
                b.push(m);
                for w in b.windows(2) {
                    sector_bounds.push((w[0], w[1]));
                }
            } else {
                for k in 0..nb {
                    let (s, t) = (bevd[k], bevd[(k + 1) % nb]);
                    sector_bounds.push((s, if t <= s { t + m } else { t }));
                }
            }
            let mut sec_of_edge_after: HashMap<usize, u32> = HashMap::new();
            let mut sec_of_edge_before: HashMap<usize, u32> = HashMap::new();
            for &(s, t) in &sector_bounds {
                let e = |i: usize| edges[i % edges.len().max(1)];
                let e_at = |i: usize| if open { edges[i] } else { edges[i % m] };
                let p = if open && s == 0 {
                    along(e_at(0), e_at(t))
                } else if open && t == m {
                    along(e_at(m), e_at(s))
                } else if t == s + 1 {
                    let (ds, dt) = (dir(&base, v, e_at(s)), dir(&base, v, e_at(t)));
                    let sn = sin_between(ds, dt);
                    [0, 1, 2].map(|k| pv[k] + (ds[k] + dt[k]) * width / sn)
                } else {
                    let mid = (s + t) / 2;
                    let _ = e;
                    along(e_at(mid), e_at(s))
                };
                let nv = new_vert(&mut self.pos, p);
                for i in s..t {
                    sector_vert[i % m] = Some(nv);
                }
                sec_of_edge_after.insert(s % edges.len(), nv);
                sec_of_edge_before.insert(t % if open { edges.len() } else { m }, nv);
            }
            for (i, f) in fan.iter().enumerate() {
                if let Some(nv) = sector_vert[i] {
                    in_face.insert((f.0, v), vec![nv]);
                }
            }
            for &bi in &bevd {
                let (Some(&before), Some(&after)) = (sec_of_edge_before.get(&bi), sec_of_edge_after.get(&bi)) else { continue };
                sides.insert((v, edges[bi]), (before, after));
                list.push((edges[bi], before, after));
            }
            if !open && nb >= 3 {
                corners.push((v, list, false));
            }
        }
        // Profils arrondis entre deux sommets de côté, avec v comme point de contrôle (partagés)
        let mut profiles: HashMap<(u32, u32, u32), Vec<u32>> = HashMap::new();
        let mut profile = |pos: &mut Vec<V3>, v: u32, a: u32, b: u32| -> Vec<u32> {
            if let Some(p) = profiles.get(&(v, a, b)) {
                return p.clone();
            }
            if let Some(p) = profiles.get(&(v, b, a)) {
                return p.iter().rev().copied().collect();
            }
            let (pa, pb, pc) = (pos[a as usize], pos[b as usize], base[v as usize]);
            let mut out = vec![a];
            for j in 1..segments {
                let t = j as f32 / segments as f32;
                let q = [0, 1, 2].map(|k| (1.0 - t) * (1.0 - t) * pa[k] + 2.0 * t * (1.0 - t) * pc[k] + t * t * pb[k]);
                pos.push(q);
                out.push(pos.len() as u32 - 1);
            }
            out.push(b);
            profiles.insert((v, a, b), out.clone());
            out
        };
        // Les faces d'origine prennent leurs nouveaux sommets
        let mut polys: Vec<Vec<u32>> = vec![];
        let mut new_faces: Vec<Vec<u32>> = vec![];
        for (f, p) in self.polys.clone().into_iter().enumerate() {
            let mut out = vec![];
            for &v in &p {
                match in_face.get(&(f, v)) {
                    Some(r) if r.first() == Some(&u32::MAX) => {
                        // Bout à trois arêtes : le profil (va … vb) prend la place de v
                        let (va, vb) = (r[1], r[2]);
                        let pr = profile(&mut self.pos, v, vb, va);
                        out.extend(pr.iter().rev());
                    }
                    Some(r) => out.extend(r),
                    None => out.push(v),
                }
            }
            polys.push(out);
        }
        // Les bandes le long des arêtes
        for &(a0, b0) in &bev {
            // F1 : la face où l'arête va de a vers b
            let (a, b) = if self.polys.iter().any(|p| (0..p.len()).any(|k| p[k] == a0 && p[(k + 1) % p.len()] == b0)) { (a0, b0) } else { (b0, a0) };
            let (Some(&(a_before, a_after)), Some(&(b_before, b_after))) = (sides.get(&(a, b)), sides.get(&(b, a))) else { continue };
            let pa = profile(&mut self.pos, a, a_before, a_after);
            let pb = profile(&mut self.pos, b, b_after, b_before);
            for i in 0..segments {
                new_faces.push(vec![pa[i], pa[i + 1], pb[i + 1], pb[i]]);
            }
        }
        // Les coins : une face faite des profils qui s'y rejoignent
        for (v, list, keep_v) in corners {
            let mut ring: Vec<u32> = vec![];
            for (_, before, after) in &list {
                let pr = profile(&mut self.pos, v, *before, *after);
                for x in pr {
                    if ring.last() != Some(&x) && ring.first() != Some(&x) {
                        ring.push(x);
                    }
                }
            }
            if list.len() == 1 && !keep_v {
                continue;
            }
            if keep_v {
                ring.push(v);
            }
            if ring.len() >= 3 {
                // Tournée vers l'extérieur, comme les faces autour de v
                let n_ring = self.normal_of(&ring);
                let n_around = faces_of[v as usize].iter().fold([0.0f32; 3], |acc, &f| {
                    let n = self.normal(&self.polys[f]);
                    [acc[0] + n[0], acc[1] + n[1], acc[2] + n[2]]
                });
                if n_ring[0] * n_around[0] + n_ring[1] * n_around[1] + n_ring[2] * n_around[2] < 0.0 {
                    ring.reverse();
                }
                if segments > 1 && !keep_v {
                    // Coin arrondi : un point central bombé vers l'ancien coin, et un éventail de
                    // triangles ; un seul polygone, pas plat, ferait un pli à l'éclairage
                    let nr = ring.len() as f32;
                    let avg = ring.iter().fold([0.0f32; 3], |a, &x| {
                        let q = self.pos[x as usize];
                        [a[0] + q[0] / nr, a[1] + q[1] / nr, a[2] + q[2] / nr]
                    });
                    let pv = base[v as usize];
                    let cpos = [0, 1, 2].map(|k| avg[k] + (pv[k] - avg[k]) * 0.42);
                    self.pos.push(cpos);
                    let cv = self.pos.len() as u32 - 1;
                    for k in 0..ring.len() {
                        new_faces.push(vec![ring[k], ring[(k + 1) % ring.len()], cv]);
                    }
                } else {
                    new_faces.push(ring);
                }
            }
        }
        let first_new = polys.len();
        polys.extend(new_faces);
        self.polys = polys;
        let mut out = vec![false; self.pos.len()];
        for p in &self.polys[first_new..] {
            for &v in p {
                out[v as usize] = true;
            }
        }
        let out = self.compact(out);
        Some(out)
    }

    fn normal_of(&self, p: &[u32]) -> V3 {
        let mut n = [0.0f32; 3];
        for k in 0..p.len() {
            let (a, b) = (self.pos[p[k] as usize], self.pos[p[(k + 1) % p.len()] as usize]);
            n[0] += (a[1] - b[1]) * (a[2] + b[2]);
            n[1] += (a[2] - b[2]) * (a[0] + b[0]);
            n[2] += (a[0] - b[0]) * (a[1] + b[1]);
        }
        n
    }

    /// Retire les sommets qui ne servent plus ; la sélection suit.
    fn compact(&mut self, sel: Vec<bool>) -> Vec<bool> {
        let mut used = vec![false; self.pos.len()];
        for p in &self.polys {
            for &v in p {
                used[v as usize] = true;
            }
        }
        let mut map = vec![u32::MAX; self.pos.len()];
        let (mut pos, mut out) = (vec![], vec![]);
        for i in 0..self.pos.len() {
            if used[i] {
                map[i] = pos.len() as u32;
                pos.push(self.pos[i]);
                out.push(sel.get(i).copied().unwrap_or(false));
            }
        }
        for p in &mut self.polys {
            for v in p.iter_mut() {
                *v = map[*v as usize];
            }
        }
        self.pos = pos;
        out
    }

    /// L'anneau d'arêtes qui passe par (a, b) : on traverse les quadrilatères par l'arête d'en face.
    /// Chaque arête est orientée pareil (premier sommet du même côté). Renvoie aussi si l'anneau se referme.
    pub fn edge_ring(&self, a: u32, b: u32) -> (Vec<(u32, u32)>, bool) {
        let mut faces_of: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
        for (f, p) in self.polys.iter().enumerate() {
            for k in 0..p.len() {
                let (x, y) = (p[k], p[(k + 1) % p.len()]);
                faces_of.entry((x.min(y), x.max(y))).or_default().push(f);
            }
        }
        let across = |e: (u32, u32), from: Option<usize>| -> Option<((u32, u32), usize)> {
            for &f in faces_of.get(&(e.0.min(e.1), e.0.max(e.1)))? {
                if Some(f) == from || self.polys[f].len() != 4 {
                    continue;
                }
                let p = &self.polys[f];
                let i = p.iter().position(|&x| x == e.0)?;
                let j = p.iter().position(|&x| x == e.1)?;
                // Le sommet d'en face de e.0 est son voisin qui n'est pas e.1
                let o0 = if p[(i + 1) % 4] == e.1 { p[(i + 3) % 4] } else { p[(i + 1) % 4] };
                let o1 = if p[(j + 1) % 4] == e.0 { p[(j + 3) % 4] } else { p[(j + 1) % 4] };
                return Some(((o0, o1), f));
            }
            None
        };
        let key = |e: (u32, u32)| (e.0.min(e.1), e.0.max(e.1));
        let mut fwd = vec![(a, b)];
        let mut seen: HashSet<(u32, u32)> = HashSet::from([key((a, b))]);
        let mut from = None;
        let mut closed = false;
        while let Some((e, f)) = across(*fwd.last().unwrap(), from) {
            if seen.contains(&key(e)) {
                closed = key(e) == key((a, b));
                break;
            }
            seen.insert(key(e));
            fwd.push(e);
            from = Some(f);
        }
        if !closed {
            // Dans l'autre sens depuis le départ
            let first_face = faces_of[&key((a, b))].iter().copied().find(|&f| {
                self.polys[f].len() == 4 && fwd.get(1).is_some_and(|e1| self.polys[f].contains(&e1.0) && self.polys[f].contains(&e1.1))
            });
            let mut back = vec![];
            let mut from = first_face;
            let mut cur = (a, b);
            while let Some((e, f)) = across(cur, from) {
                if seen.contains(&key(e)) {
                    break;
                }
                seen.insert(key(e));
                back.push(e);
                from = Some(f);
                cur = e;
            }
            back.reverse();
            back.extend(fwd);
            fwd = back;
        }
        (fwd, closed)
    }

    /// Coupe en boucle (Ctrl+R) : `cuts` nouvelles arêtes à travers les quadrilatères de l'anneau.
    /// `slide` de -1 à 1 décale les coupes vers un bord. Renvoie la sélection des nouvelles arêtes.
    pub fn loop_cut(&mut self, ring: &[(u32, u32)], cuts: u32, slide: f32) -> Option<Vec<bool>> {
        if ring.is_empty() || cuts == 0 {
            return None;
        }
        let n = cuts as usize;
        let step = 1.0 / (n as f32 + 1.0);
        let shift = slide.clamp(-1.0, 1.0) * step * 0.98;
        let mut mid: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        for &(a, b) in ring {
            let (pa, pb) = (self.pos[a as usize], self.pos[b as usize]);
            let mut vs = vec![];
            for j in 0..n {
                let t = (j as f32 + 1.0) * step + shift;
                self.pos.push([0, 1, 2].map(|k| pa[k] + (pb[k] - pa[k]) * t));
                vs.push(self.pos.len() as u32 - 1);
            }
            mid.insert((a, b), vs.clone());
            vs.reverse();
            mid.insert((b, a), vs);
        }
        let ring_set: HashSet<(u32, u32)> = ring.iter().map(|&(a, b)| (a.min(b), a.max(b))).collect();
        let mut polys = vec![];
        for p in std::mem::take(&mut self.polys) {
            let hits: Vec<usize> = (0..p.len()).filter(|&k| ring_set.contains(&{
                let (x, y) = (p[k], p[(k + 1) % p.len()]);
                (x.min(y), x.max(y))
            })).collect();
            if p.len() == 4 && hits.len() == 2 && (hits[1] - hits[0]) == 2 {
                // Quadrilatère traversé : coupé en n + 1 bandes
                let k = hits[0];
                let (x0, x1, y0, y1) = (p[k], p[(k + 1) % 4], p[(k + 2) % 4], p[(k + 3) % 4]);
                let m1 = &mid[&(x0, x1)];
                let m2 = &mid[&(y1, y0)];
                let mut l0 = vec![x0];
                l0.extend(m1);
                l0.push(x1);
                let mut l1 = vec![y1];
                l1.extend(m2);
                l1.push(y0);
                for j in 0..=n {
                    polys.push(vec![l0[j], l0[j + 1], l1[j + 1], l1[j]]);
                }
            } else if !hits.is_empty() {
                // Bout d'anneau (triangle, n-gone) : les nouveaux sommets s'insèrent sur l'arête
                let mut out = vec![];
                for k in 0..p.len() {
                    out.push(p[k]);
                    if let Some(m) = mid.get(&(p[k], p[(k + 1) % p.len()])) {
                        if hits.contains(&k) {
                            out.extend(m);
                        }
                    }
                }
                polys.push(out);
            } else {
                polys.push(p);
            }
        }
        self.polys = polys;
        let mut out = vec![false; self.pos.len()];
        for vs in mid.values() {
            for &v in vs {
                out[v as usize] = true;
            }
        }
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
    fn chemins_et_boucles() {
        // Grille de 4 × 4 quadrilatères (5 × 5 sommets)
        let mut m = EMesh::default();
        for y in 0..5 {
            for x in 0..5 {
                m.pos.push([x as f32, 0.0, y as f32]);
            }
        }
        for y in 0..4u32 {
            for x in 0..4u32 {
                let i = y * 5 + x;
                m.polys.push(vec![i, i + 1, i + 6, i + 5]);
            }
        }
        // Le plus court chemin d'un coin à l'autre de la même rangée : la rangée elle-même
        assert_eq!(m.shortest_path(0, 4), vec![0, 1, 2, 3, 4]);
        // La boucle passant par l'arête (11, 12) au milieu : toute la rangée du milieu
        let mut l = m.edge_loop(11, 12);
        l.sort();
        assert_eq!(l, vec![10, 11, 12, 13, 14]);
        // Étendre puis réduire revient au départ (loin des bords)
        let mut sel = vec![false; 25];
        sel[12] = true;
        let g = m.grow(&sel);
        assert_eq!(g.iter().filter(|x| **x).count(), 5);
        assert_eq!(m.shrink(&g), sel);
    }

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

    /// Chaque arête orientée a sa jumelle à l'envers : surface fermée et faces bien tournées.
    fn ferme(m: &EMesh) -> bool {
        let mut d: HashMap<(u32, u32), i32> = HashMap::new();
        for p in &m.polys {
            for k in 0..p.len() {
                *d.entry((p[k], p[(k + 1) % p.len()])).or_default() += 1;
            }
        }
        d.iter().all(|(&(a, b), &n)| n == 1 && d.get(&(b, a)) == Some(&1))
    }

    fn cube() -> EMesh {
        let pos = (0..8).map(|i| [(i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32]).collect();
        let polys = vec![vec![0, 2, 3, 1], vec![4, 5, 7, 6], vec![0, 1, 5, 4], vec![2, 6, 7, 3], vec![0, 4, 6, 2], vec![1, 3, 7, 5]];
        EMesh { pos, polys }
    }

    #[test]
    fn biseau_et_coupe() {
        // Une arête, un segment : 7 faces, surface toujours fermée et du même côté
        let mut m = cube();
        assert!(m.volume() > 0.0 && ferme(&m));
        let mut sel = vec![false; 8];
        sel[0] = true;
        sel[1] = true;
        assert!(m.bevel(&sel, 0.2, 1).is_some());
        assert_eq!(m.polys.len(), 7);
        assert!(ferme(&m), "{:?}", m.polys);
        assert!(m.volume() > 0.9 && m.volume() < 1.0);
        // Toutes les arêtes, trois segments : 6 faces + 12 bandes de 3 + 8 coins
        let mut m = cube();
        assert!(m.bevel(&[true; 8], 0.2, 3).is_some());
        // 6 faces, 12 bandes de 3, et 8 coins en éventail de 9 triangles
        assert_eq!(m.polys.len(), 6 + 36 + 8 * 9);
        assert!(ferme(&m));
        // Deux arêtes qui se suivent (coin à deux arêtes) et 2 segments
        let mut m = cube();
        let mut sel = vec![false; 8];
        for v in [0, 1, 3] {
            sel[v] = true;
        }
        let w = m.bevel_limit(&sel);
        assert!(w > 0.3 && w < 0.5, "{w}");
        assert!(m.bevel(&sel, 0.2, 2).is_some());
        assert!(ferme(&m), "{:?}", m.polys);
        // Coupe en boucle : l'anneau fait le tour, deux coupes
        let mut m = cube();
        let (ring, closed) = m.edge_ring(0, 1);
        assert!(closed && ring.len() == 4, "{ring:?}");
        assert!(m.loop_cut(&ring, 2, 0.0).is_some());
        assert_eq!(m.polys.len(), 2 + 4 * 3);
        assert!(ferme(&m));
        assert!((m.volume() - 1.0).abs() < 1e-4);
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
