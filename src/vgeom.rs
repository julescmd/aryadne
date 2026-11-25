//! Géométrie du vectoriel faite sur une grille fine : décalage de tracé, Concepteur de formes
//! (fusionner, effacer des zones) et contour à largeur variable (outil Largeur).
//! Les formes passent en silhouettes rasterisées, on calcule sur les cellules, puis on revectorise.

use crate::vector::{path_data, Doc, Kind, Node, Paint, Shape};
use std::fmt::Write;

/// Une grille posée sur le monde : `k` cellules par unité, coin haut-gauche en `org`
#[derive(Clone, Debug)]
pub struct Grid {
    pub w: usize,
    pub h: usize,
    pub org: [f32; 2],
    pub k: f32,
}

impl Grid {
    /// Une grille qui couvre ces formes, avec une marge (unités du monde), au plus `max` cellules de côté
    pub fn around(doc: &Doc, ids: &[u64], pad: f32, max: u32) -> Option<Grid> {
        let b = doc.bounds_of(ids)?;
        let (bw, bh) = (b[2] - b[0] + 2.0 * pad, b[3] - b[1] + 2.0 * pad);
        if bw <= 0.0 || bh <= 0.0 {
            return None;
        }
        let k = (max as f32 / bw.max(bh)).min(8.0);
        Some(Grid { w: (bw * k).ceil() as usize + 1, h: (bh * k).ceil() as usize + 1, org: [b[0] - pad, b[1] - pad], k })
    }

    pub fn cell_of(&self, p: [f32; 2]) -> Option<usize> {
        let (x, y) = (((p[0] - self.org[0]) * self.k) as i64, ((p[1] - self.org[1]) * self.k) as i64);
        (x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h).then(|| y as usize * self.w + x as usize)
    }

    /// Les cellules couvertes par une forme (son contenu s'il s'agit d'un groupe)
    pub fn mask(&self, doc: &Doc, id: u64) -> Vec<bool> {
        let mut body = String::new();
        if let Some(s) = doc.get(id) {
            silhouette(doc, s, &mut body);
        }
        let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="{} {} {} {}">{body}</svg>"#, self.org[0], self.org[1], self.w as f32 / self.k, self.h as f32 / self.k, w = self.w, h = self.h);
        match crate::vector::rasterize(&svg, self.w as u32, self.h as u32) {
            Some(px) => px.chunks_exact(4).map(|c| c[3] > 127).collect(),
            None => vec![false; self.w * self.h],
        }
    }

    /// Les cellules couvertes par plusieurs formes, d'un seul rendu
    pub fn mask_all(&self, doc: &Doc, ids: &[u64]) -> Vec<bool> {
        let mut body = String::new();
        for id in ids {
            if let Some(s) = doc.get(*id) {
                silhouette(doc, s, &mut body);
            }
        }
        let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="{} {} {} {}">{body}</svg>"#, self.org[0], self.org[1], self.w as f32 / self.k, self.h as f32 / self.k, w = self.w, h = self.h);
        match crate::vector::rasterize(&svg, self.w as u32, self.h as u32) {
            Some(px) => px.chunks_exact(4).map(|c| c[3] > 127).collect(),
            None => vec![false; self.w * self.h],
        }
    }

    /// Revectorise des cellules : un tracé par contour, en coordonnées du monde
    pub fn trace(&self, cells: &[bool]) -> Vec<Vec<Node>> {
        let mut img = image::RgbaImage::new(self.w as u32, self.h as u32);
        for (i, on) in cells.iter().enumerate() {
            if *on {
                img.put_pixel((i % self.w) as u32, (i / self.w) as u32, image::Rgba([0, 0, 0, 255]));
            }
        }
        let (loops, _) = crate::vector::trace(&img, 0.5, 1.4);
        let k = self.k;
        let map = |p: [f32; 2]| [p[0] / k, p[1] / k];
        loops.into_iter().map(|lp| lp.into_iter().map(|n| Node { p: map(n.p), i: map(n.i), o: map(n.o) }).collect()).collect()
    }
}

/// La silhouette pleine d'une forme, en SVG (le contour compte si la forme n'a pas de fond)
fn silhouette(doc: &Doc, s: &Shape, out: &mut String) {
    if !s.visible {
        return;
    }
    if let Kind::Group { children, .. } = &s.kind {
        for c in children {
            if let Some(k) = doc.get(*c) {
                silhouette(doc, k, out);
            }
        }
        return;
    }
    let d = if s.widths.is_empty() { path_data(s) } else { width_outline(s).unwrap_or_default() };
    let open = matches!(s.kind, Kind::Line { .. } | Kind::Path { closed: false, .. });
    let stroke = s.strokes.first().map(|k| k.width).unwrap_or(0.0);
    let fill = if (s.fills.is_empty() || open) && s.widths.is_empty() { "none" } else { "black" };
    let st = if s.widths.is_empty() && stroke > 0.0 { format!(r#" stroke="black" stroke-width="{stroke}" stroke-linejoin="round""#) } else { String::new() };
    let _ = write!(out, r#"<path transform="translate({} {}) rotate({})" d="{d}" fill="{fill}" fill-rule="evenodd"{st}/>"#, s.x, s.y, s.rot);
}

/// Distance (en cellules) de chaque cellule à la cellule `true` la plus proche (transformée exacte, Felzenszwalb)
pub fn distance(cells: &[bool], w: usize, h: usize) -> Vec<f32> {
    const INF: f32 = 1e12;
    let mut g: Vec<f32> = cells.iter().map(|c| if *c { 0.0 } else { INF }).collect();
    fn pass(f: &[f32], out: &mut [f32]) {
        let n = f.len();
        let (mut v, mut z) = (vec![0usize; n], vec![0f32; n + 1]);
        let mut k = 0;
        z[0] = -INF;
        z[1] = INF;
        for q in 1..n {
            loop {
                let p = v[k];
                let s = ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * q as f32 - 2.0 * p as f32);
                if s <= z[k] && k > 0 {
                    k -= 1;
                    continue;
                }
                if s <= z[k] {
                    // k == 0 : on remplace la première parabole
                    v[0] = q;
                    z[0] = -INF;
                    z[1] = INF;
                    break;
                }
                k += 1;
                v[k] = q;
                z[k] = s;
                z[k + 1] = INF;
                break;
            }
        }
        k = 0;
        for (q, o) in out.iter_mut().enumerate() {
            while z[k + 1] < q as f32 {
                k += 1;
            }
            let p = v[k];
            *o = (q as f32 - p as f32).powi(2) + f[p];
        }
    }
    let mut col = vec![0f32; h];
    let mut res = vec![0f32; h];
    for x in 0..w {
        for y in 0..h {
            col[y] = g[y * w + x];
        }
        pass(&col, &mut res);
        for y in 0..h {
            g[y * w + x] = res[y];
        }
    }
    let mut row = vec![0f32; w];
    for y in 0..h {
        pass(&g[y * w..(y + 1) * w], &mut row);
        g[y * w..(y + 1) * w].copy_from_slice(&row);
    }
    g.into_iter().map(|d| d.min(INF).sqrt()).collect()
}

/// Décale la silhouette de `d` unités (vers l'extérieur si positif, l'intérieur si négatif)
pub fn offset(cells: &[bool], g: &Grid, d: f32) -> Vec<bool> {
    let px = d * g.k;
    if px >= 0.0 {
        distance(cells, g.w, g.h).into_iter().map(|x| x <= px).collect()
    } else {
        let inv: Vec<bool> = cells.iter().map(|c| !c).collect();
        distance(&inv, g.w, g.h).into_iter().map(|x| x > -px).collect()
    }
}

/// Pose des contours dans le document, avec le style de `like` ; rend l'identifiant (tracé, ou groupe pair-impair)
pub fn add_traced(doc: &mut Doc, g: &Grid, loops: Vec<Vec<Node>>, like: &Shape, name: &str) -> Option<u64> {
    let mut ids = vec![];
    for nodes in loops {
        let id = doc.new_id();
        doc.shapes.push(Shape { id, name: name.to_string(), kind: Kind::Path { nodes, closed: true }, x: g.org[0], y: g.org[1], fills: like.fills.clone(), strokes: like.strokes.clone(), opacity: like.opacity, shadow: like.shadow.clone(), blur: like.blur, blend: like.blend.clone(), ..Shape::base() });
        ids.push(id);
    }
    match ids.len() {
        0 => None,
        1 => Some(ids[0]),
        _ => {
            let gid = doc.group(&ids, 5)?;
            if let Some(s) = doc.shapes.iter_mut().find(|s| s.id == gid) {
                s.name = name.to_string();
            }
            Some(gid)
        }
    }
}

/// Les zones du Concepteur de formes : chaque cellule porte la liste des formes qui la couvrent ;
/// une zone est un morceau d'un seul tenant de cellules à la même liste.
pub struct Regions {
    pub grid: Grid,
    pub ids: Vec<u64>,
    pub masks: Vec<Vec<bool>>,
    /// Zone de chaque cellule (0 : vide)
    pub label: Vec<u32>,
    /// Contours de chaque zone (index = étiquette - 1), pour la surbrillance
    pub outlines: Vec<Vec<Vec<Node>>>,
}

impl Regions {
    pub fn build(doc: &Doc, ids: &[u64]) -> Option<Regions> {
        let ids: Vec<u64> = ids.iter().copied().take(63).collect();
        let grid = Grid::around(doc, &ids, 4.0, 900)?;
        let masks: Vec<Vec<bool>> = ids.iter().map(|i| grid.mask(doc, *i)).collect();
        let n = grid.w * grid.h;
        let sig: Vec<u64> = (0..n).map(|c| masks.iter().enumerate().fold(0u64, |a, (k, m)| if m[c] { a | 1 << k } else { a })).collect();
        let mut label = vec![0u32; n];
        let mut next = 0u32;
        let mut sizes = vec![];
        let mut stack = vec![];
        for c0 in 0..n {
            if sig[c0] == 0 || label[c0] != 0 {
                continue;
            }
            next += 1;
            let mut size = 0;
            label[c0] = next;
            stack.push(c0);
            while let Some(c) = stack.pop() {
                size += 1;
                let (x, y) = (c % grid.w, c / grid.w);
                let mut nb = |cc: usize| {
                    if label[cc] == 0 && sig[cc] == sig[c0] {
                        label[cc] = next;
                        stack.push(cc);
                    }
                };
                if x > 0 {
                    nb(c - 1);
                }
                if x + 1 < grid.w {
                    nb(c + 1);
                }
                if y > 0 {
                    nb(c - grid.w);
                }
                if y + 1 < grid.h {
                    nb(c + grid.w);
                }
            }
            sizes.push(size);
        }
        // Les éclats d'anticrénelage (quelques cellules) rejoignent la zone voisine la plus grande
        for c in 0..n {
            let l = label[c];
            if l == 0 || sizes[l as usize - 1] > 6 {
                continue;
            }
            let (x, y) = (c % grid.w, c / grid.w);
            let mut best = (0, 0);
            for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx < 0 || ny < 0 || nx as usize >= grid.w || ny as usize >= grid.h {
                    continue;
                }
                let o = label[ny as usize * grid.w + nx as usize];
                if o != 0 && o != l && sizes[o as usize - 1] > best.1 {
                    best = (o, sizes[o as usize - 1]);
                }
            }
            if best.0 != 0 {
                label[c] = best.0;
            }
        }
        let outlines = (1..=next).map(|l| if sizes[l as usize - 1] > 6 { grid.trace(&label.iter().map(|x| *x == l).collect::<Vec<_>>()) } else { vec![] }).collect();
        Some(Regions { grid, ids, masks, label, outlines })
    }

    pub fn at(&self, p: [f32; 2]) -> u32 {
        self.grid.cell_of(p).map(|c| self.label[c]).unwrap_or(0)
    }

    /// Fusionne (ou efface, `erase`) ces zones : les formes touchées sont redécoupées ;
    /// rend les nouvelles formes (la fusionnée d'abord) et celles qui ont disparu
    pub fn apply(&self, doc: &mut Doc, picked: &[u32], erase: bool) -> (Vec<u64>, Vec<u64>) {
        if picked.is_empty() {
            return (vec![], vec![]);
        }
        let hit: Vec<bool> = self.label.iter().map(|l| *l != 0 && picked.contains(l)).collect();
        // Le style de la forme la plus en avant sous la première zone choisie
        let Some(first) = self.label.iter().position(|l| *l == picked[0]) else { return (vec![], vec![]) };
        let Some(style) = (0..self.ids.len()).filter(|k| self.masks[*k][first]).max_by_key(|k| doc.index(self.ids[*k]).unwrap_or(0)).and_then(|k| doc.get(self.ids[k]).cloned()) else { return (vec![], vec![]) };
        let mut new_ids = vec![];
        let mut gone = vec![];
        for (k, id) in self.ids.iter().enumerate() {
            let touched = self.masks[k].iter().zip(&hit).any(|(m, h)| *m && *h);
            if !touched {
                continue;
            }
            let Some(orig) = doc.get(*id).cloned() else { continue };
            let rest: Vec<bool> = self.masks[k].iter().zip(&hit).map(|(m, h)| *m && !*h).collect();
            let at = doc.index(*id).unwrap_or(doc.shapes.len());
            doc.remove(&[*id]);
            gone.push(*id);
            if let Some(n) = add_traced(doc, &self.grid, self.grid.trace(&rest), &orig, &orig.name) {
                // Le reste reprend la place de la forme dans la pile
                if let Some(i) = doc.index(n) {
                    let s = doc.shapes.remove(i);
                    doc.shapes.insert(at.min(doc.shapes.len()), s);
                }
                new_ids.push(n);
            }
        }
        if !erase {
            let name = style.name.clone();
            if let Some(m) = add_traced(doc, &self.grid, self.grid.trace(&hit), &style, &name) {
                new_ids.insert(0, m);
            }
        }
        (new_ids, gone)
    }
}

/// Points le long d'un tracé (courbes découpées), avec l'abscisse curviligne de chacun
pub fn flatten(nodes: &[Node], closed: bool) -> Vec<([f32; 2], f32)> {
    let mut pts: Vec<[f32; 2]> = vec![];
    let n = nodes.len();
    if n == 0 {
        return vec![];
    }
    let segs = if closed { n } else { n - 1 };
    pts.push(nodes[0].p);
    for s in 0..segs {
        let (a, b) = (&nodes[s], &nodes[(s + 1) % n]);
        // Les segments droits sont aussi découpés : la largeur peut y varier
        let chord = ((b.p[0] - a.p[0]).powi(2) + (b.p[1] - a.p[1]).powi(2)).sqrt();
        let steps = if a.o == a.p && b.i == b.p { ((chord / 4.0) as usize).clamp(1, 64) } else { 32 };
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            let u = 1.0 - t;
            let p = [0, 1].map(|c| u * u * u * a.p[c] + 3.0 * u * u * t * a.o[c] + 3.0 * u * t * t * b.i[c] + t * t * t * b.p[c]);
            pts.push(p);
        }
    }
    let mut out = Vec::with_capacity(pts.len());
    let mut acc = 0.0;
    for (i, p) in pts.iter().enumerate() {
        if i > 0 {
            acc += ((p[0] - pts[i - 1][0]).powi(2) + (p[1] - pts[i - 1][1]).powi(2)).sqrt();
        }
        out.push((*p, acc));
    }
    out
}

/// Les points d'une forme qui suit un tracé (tracé ou ligne), dans le repère de la forme
pub fn spine(s: &Shape) -> Option<(Vec<([f32; 2], f32)>, bool)> {
    match &s.kind {
        Kind::Path { nodes, closed } if nodes.len() > 1 => Some((flatten(nodes, *closed), *closed)),
        Kind::Line { x2, y2 } => Some((flatten(&[Node::corner([0.0, 0.0]), Node::corner([*x2, *y2])], false), false)),
        _ => None,
    }
}

/// Largeur du contour à la position relative `t` (0 au départ, 1 à l'arrivée)
pub fn width_at(widths: &[[f32; 2]], base: f32, t: f32) -> f32 {
    let mut pts: Vec<[f32; 2]> = widths.to_vec();
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
    if pts.first().is_none_or(|p| p[0] > 0.0) {
        pts.insert(0, [0.0, base]);
    }
    if pts.last().is_some_and(|p| p[0] < 1.0) {
        pts.push([1.0, base]);
    }
    for w in pts.windows(2) {
        if t <= w[1][0] {
            let f = ((t - w[0][0]) / (w[1][0] - w[0][0]).max(1e-6)).clamp(0.0, 1.0);
            // Transition douce, comme les points de largeur d'Illustrator
            let f = f * f * (3.0 - 2.0 * f);
            return w[0][1] + (w[1][1] - w[0][1]) * f;
        }
    }
    pts.last().map(|p| p[1]).unwrap_or(base)
}

/// Le contour à largeur variable d'une forme, en tracé SVG plein (repère de la forme)
pub fn width_outline(s: &Shape) -> Option<String> {
    let (pts, closed) = spine(s)?;
    let total = pts.last()?.1.max(1e-6);
    let base = s.strokes.first().map(|k| k.width).unwrap_or(2.0);
    let brush = s.strokes.first().map(|k| k.brush).unwrap_or(0);
    let n = pts.len();
    // Dispersion : des pastilles le long du tracé, taille et écart un peu au hasard (toujours le même)
    if brush == 3 {
        let mut d = String::new();
        let step = (base * 1.6).max(1.0);
        let mut at = 0.0f32;
        let mut k = 0u32;
        let mut seg = 0usize;
        while at <= total && k < 4000 {
            while seg + 1 < n - 1 && pts[seg + 1].1 < at {
                seg += 1;
            }
            let ((a, sa), (b, sb)) = (pts[seg], pts[(seg + 1).min(n - 1)]);
            let f = ((at - sa) / (sb - sa).max(1e-6)).clamp(0.0, 1.0);
            let h = |x: u32| ((x.wrapping_mul(2654435761) >> 8) & 0xffff) as f32 / 65535.0;
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l = (dx * dx + dy * dy).sqrt().max(1e-6);
            let off = (h(k * 3 + 1) - 0.5) * base * 1.2;
            let c = [a[0] + dx * f - dy / l * off, a[1] + dy * f + dx / l * off];
            let r = base * (0.3 + 0.45 * h(k * 3 + 2)) * width_at(&s.widths, 1.0, at / total);
            let _ = write!(d, "M{:.2} {:.2}a{r:.2} {r:.2} 0 1 0 {:.2} 0a{r:.2} {r:.2} 0 1 0 {:.2} 0Z", c[0] - r, c[1], 2.0 * r, -2.0 * r);
            at += step * (0.7 + 0.6 * h(k * 3 + 3));
            k += 1;
        }
        return Some(d);
    }
    let mut left = Vec::with_capacity(n);
    let mut right = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b) = if closed { (pts[(i + n - 2) % (n - 1)].0, pts[(i + 1) % (n - 1)].0) } else { (pts[i.saturating_sub(1)].0, pts[(i + 1).min(n - 1)].0) };
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = (dx * dx + dy * dy).sqrt().max(1e-6);
        let nrm = [-dy / l, dx / l];
        let t = pts[i].1 / total;
        let shape = match brush {
            // Plume tenue à 45° : épais en travers, fin dans le sens de la plume
            1 => 0.15 + 0.85 * (dy.atan2(dx) - std::f32::consts::FRAC_PI_4).sin().abs(),
            // Effilé aux deux bouts (un tracé fermé reste plein)
            2 if !closed => (std::f32::consts::PI * t).sin().max(0.0).powf(0.6).max(0.04),
            _ => 1.0,
        };
        let w = width_at(&s.widths, base, t) / 2.0 * shape;
        let p = pts[i].0;
        left.push([p[0] + nrm[0] * w, p[1] + nrm[1] * w]);
        right.push([p[0] - nrm[0] * w, p[1] - nrm[1] * w]);
    }
    let mut d = String::new();
    let line = |d: &mut String, pts: &[[f32; 2]]| {
        for (k, p) in pts.iter().enumerate() {
            let _ = write!(d, "{}{:.2} {:.2}", if k == 0 { 'M' } else { 'L' }, p[0], p[1]);
        }
    };
    if closed {
        line(&mut d, &left);
        d.push('Z');
        let r: Vec<[f32; 2]> = right.into_iter().rev().collect();
        line(&mut d, &r);
        d.push('Z');
    } else {
        let mut all = left;
        all.extend(right.into_iter().rev());
        line(&mut d, &all);
        d.push('Z');
    }
    Some(d)
}

/// Le point du tracé le plus proche de `p` (repère de la forme) : position relative t, point, normale
pub fn nearest(s: &Shape, p: [f32; 2]) -> Option<(f32, [f32; 2], [f32; 2], f32)> {
    let (pts, _) = spine(s)?;
    let total = pts.last()?.1.max(1e-6);
    // Projection sur chaque segment
    let mut best: (f32, f32, [f32; 2], [f32; 2]) = (f32::MAX, 0.0, pts[0].0, [0.0, 1.0]);
    for w in pts.windows(2) {
        let ((a, sa), (b, sb)) = (w[0], w[1]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l2 = (dx * dx + dy * dy).max(1e-9);
        let u = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0);
        let q = [a[0] + dx * u, a[1] + dy * u];
        let d = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt();
        if d < best.0 {
            let l = l2.sqrt();
            best = (d, (sa + (sb - sa) * u) / total, q, [-dy / l, dx / l]);
        }
    }
    Some((best.1, best.2, best.3, best.0))
}

/// Le style d'un contour devenu une forme pleine : sa couleur passe en fond
pub fn stroke_as_fill(s: &Shape) -> Vec<Paint> {
    s.strokes.first().map(|k| vec![k.paint.clone()]).unwrap_or_else(|| s.fills.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_exacte() {
        let (w, h) = (9, 9);
        let mut c = vec![false; w * h];
        c[4 * w + 4] = true;
        let d = distance(&c, w, h);
        assert_eq!(d[4 * w + 4], 0.0);
        assert!((d[4 * w + 7] - 3.0).abs() < 1e-4);
        assert!((d[0] - (32f32).sqrt()).abs() < 1e-4);
    }

    #[test]
    fn decalage_et_concepteur() {
        let mut d = Doc::starter();
        let a = d.new_id();
        d.shapes.push(Shape { id: a, kind: Kind::Rect { w: 100.0, h: 100.0, r: [0.0; 4] }, x: 0.0, y: 0.0, fills: vec![Paint::solid([255, 0, 0, 255])], ..Shape::base() });
        let g = Grid::around(&d, &[a], 20.0, 400).unwrap();
        let m = g.mask(&d, a);
        let big = offset(&m, &g, 10.0);
        let like = d.get(a).unwrap().clone();
        let id = add_traced(&mut d, &g, g.trace(&big), &like, "décalé").unwrap();
        let b = d.bounds(d.get(id).unwrap());
        assert!((b[0] + 10.0).abs() < 1.5 && (b[2] - 110.0).abs() < 1.5, "{b:?}");
        // Deux carrés qui se chevauchent : trois zones ; en fusionner deux donne une forme de plus
        let mut d = Doc::starter();
        let a = d.new_id();
        d.shapes.push(Shape { id: a, kind: Kind::Rect { w: 100.0, h: 100.0, r: [0.0; 4] }, fills: vec![Paint::solid([255, 0, 0, 255])], ..Shape::base() });
        let b = d.new_id();
        d.shapes.push(Shape { id: b, kind: Kind::Rect { w: 100.0, h: 100.0, r: [0.0; 4] }, x: 50.0, fills: vec![Paint::solid([0, 0, 255, 255])], ..Shape::base() });
        let r = Regions::build(&d, &[a, b]).unwrap();
        let (za, zab, zb) = (r.at([20.0, 50.0]), r.at([75.0, 50.0]), r.at([130.0, 50.0]));
        assert!(za != 0 && zab != 0 && zb != 0 && za != zab && zab != zb);
        let m = r.apply(&mut d, &[za, zab], false).0[0];
        let bb = d.bounds(d.get(m).unwrap());
        assert!(bb[0] < 2.0 && (bb[2] - 100.0).abs() < 2.0, "{bb:?}");
        // Le carré bleu ne garde que sa part droite
        let rest: Vec<&Shape> = d.shapes.iter().filter(|s| s.fills.first().map(|f| f.first()) == Some([0, 0, 255, 255])).collect();
        assert_eq!(rest.len(), 1);
        let rb = d.bounds(rest[0]);
        assert!((rb[0] - 100.0).abs() < 2.0, "{rb:?}");
    }

    #[test]
    fn largeur_variable() {
        let mut s = Shape { kind: Kind::Line { x2: 100.0, y2: 0.0 }, ..Shape::base() };
        s.widths = vec![[0.5, 20.0]];
        assert!((width_at(&s.widths, 2.0, 0.5) - 20.0).abs() < 1e-4);
        assert!((width_at(&s.widths, 2.0, 0.0) - 2.0).abs() < 1e-4);
        assert!(width_outline(&s).unwrap().contains('Z'));
        let (t, _, _, dist) = nearest(&s, [30.0, 5.0]).unwrap();
        assert!((t - 0.3).abs() < 0.02 && (dist - 5.0).abs() < 0.5);
    }
}
