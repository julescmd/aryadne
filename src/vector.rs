//! Dessin vectoriel (.aryvec), dans l'esprit d'Illustrator et de Figma : des plans de travail sur une toile
//! infinie, des formes (rectangles, ellipses, polygones, étoiles, lignes, tracés de Bézier, textes, images),
//! remplissages et contours, groupes et opérations booléennes non destructives.
//!
//! Le document s'écrit en SVG ; ce même SVG sert au rendu (resvg), à l'export et aux aperçus.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::path::Path;

pub fn is_vector(rel: &str) -> bool {
    rel.to_lowercase().ends_with(".aryvec")
}

/// Un plan de travail (un cadre de Figma) : une zone exportable de la toile
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Board {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Fond (None : transparent)
    #[serde(default)]
    pub bg: Option<[u8; 4]>,
}

/// Un point d'un tracé : le point, ses poignées d'entrée et de sortie (positions absolues, repère de la forme)
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Node {
    pub p: [f32; 2],
    pub i: [f32; 2],
    pub o: [f32; 2],
}

impl Node {
    pub fn corner(p: [f32; 2]) -> Node {
        Node { p, i: p, o: p }
    }
    pub fn is_corner(&self) -> bool {
        self.i == self.p && self.o == self.p
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Kind {
    /// Rectangle de w × h, coins arrondis (haut-gauche, haut-droit, bas-droit, bas-gauche)
    Rect { w: f32, h: f32, #[serde(default)] r: [f32; 4] },
    Ellipse { w: f32, h: f32 },
    /// Polygone régulier inscrit dans un cercle de rayon r
    Polygon { sides: u32, r: f32 },
    /// Étoile : branches, rayons extérieur et intérieur
    Star { points: u32, r1: f32, r2: f32 },
    /// Segment vers (x2, y2), depuis l'origine de la forme
    Line { x2: f32, y2: f32 },
    Path { nodes: Vec<Node>, #[serde(default)] closed: bool },
    /// Texte (vectorisé au rendu) ; `width` : largeur de la zone (retour à la ligne), sinon texte de point
    Text { text: String, size: f32, #[serde(default = "default_font")] font: String, #[serde(default)] align: u8, #[serde(default)] width: Option<f32>, #[serde(default)] spacing: f32, #[serde(default = "one")] leading: f32 },
    /// Image du coffre, posée en w × h
    Image { path: String, w: f32, h: f32 },
    /// Groupe : ses formes (identifiants) ; `op` : 0 simple, 1 union, 2 soustraction, 3 intersection, 4 exclusion
    Group { children: Vec<u64>, #[serde(default)] op: u8 },
}

fn default_font() -> String {
    "inter".into()
}

fn one() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}

/// Une peinture : couleur unie ou dégradé (angle en degrés pour le linéaire)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Paint {
    Solid { c: [u8; 4] },
    Linear { stops: Vec<(f32, [u8; 4])>, angle: f32 },
    Radial { stops: Vec<(f32, [u8; 4])> },
}

impl Paint {
    pub fn solid(c: [u8; 4]) -> Paint {
        Paint::Solid { c }
    }
    pub fn first(&self) -> [u8; 4] {
        match self {
            Paint::Solid { c } => *c,
            Paint::Linear { stops, .. } | Paint::Radial { stops } => stops.first().map(|s| s.1).unwrap_or([0, 0, 0, 255]),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Stroke {
    pub paint: Paint,
    pub width: f32,
    /// 0 plat, 1 rond, 2 carré
    #[serde(default)]
    pub cap: u8,
    /// 0 pointu, 1 rond, 2 biseauté
    #[serde(default)]
    pub join: u8,
    /// Pointillés : longueurs alternées (vide : plein)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dash: Vec<f32>,
    /// Flèche au bout d'une ligne ou d'un tracé ouvert
    #[serde(default)]
    pub arrow: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Shadow {
    pub dx: f32,
    pub dy: f32,
    pub blur: f32,
    pub c: [u8; 4],
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Shape {
    pub id: u64,
    pub name: String,
    pub kind: Kind,
    /// Position de l'origine (coin haut-gauche pour les rectangles, ellipses et images ; centre pour polygones et étoiles)
    pub x: f32,
    pub y: f32,
    /// Rotation en degrés autour de l'origine
    #[serde(default)]
    pub rot: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<Paint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strokes: Vec<Stroke>,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    /// Groupe qui contient la forme (0 : à la racine)
    #[serde(default)]
    pub parent: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<Shadow>,
    /// Flou de la forme, en px
    #[serde(default)]
    pub blur: f32,
    /// Mode de fusion CSS (normal, multiply, screen…)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub blend: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Doc {
    pub boards: Vec<Board>,
    /// Ordre de dessin : de l'arrière vers l'avant
    pub shapes: Vec<Shape>,
    #[serde(default)]
    pub next_id: u64,
    /// Nuances du document (palette)
    #[serde(default)]
    pub swatches: Vec<[u8; 4]>,
}

impl Doc {
    pub fn starter() -> Doc {
        Doc {
            boards: vec![Board { name: "Plan de travail 1".into(), x: 0.0, y: 0.0, w: 1080.0, h: 1080.0, bg: Some([255, 255, 255, 255]) }],
            shapes: vec![],
            next_id: 1,
            swatches: vec![[24, 24, 27, 255], [255, 255, 255, 255], [236, 108, 89, 255], [244, 191, 80, 255], [86, 166, 122, 255], [84, 128, 220, 255], [150, 110, 210, 255]],
        }
    }

    pub fn load(path: &Path) -> Result<Doc, String> {
        let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        if s.trim().is_empty() {
            return Ok(Doc::starter());
        }
        serde_json::from_str(&s).map_err(|e| format!("Dessin illisible : {e}"))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        crate::vault::atomic_write(path, s).map_err(|e| e.to_string())
    }

    pub fn new_id(&mut self) -> u64 {
        self.next_id = self.next_id.max(self.shapes.iter().map(|s| s.id).max().unwrap_or(0)) + 1;
        self.next_id
    }

    pub fn index(&self, id: u64) -> Option<usize> {
        self.shapes.iter().position(|s| s.id == id)
    }

    pub fn get(&self, id: u64) -> Option<&Shape> {
        self.shapes.iter().find(|s| s.id == id)
    }

    /// Les formes d'un groupe, et de ses sous-groupes
    pub fn descendants(&self, id: u64) -> Vec<u64> {
        let mut out = vec![];
        if let Some(Shape { kind: Kind::Group { children, .. }, .. }) = self.get(id) {
            for &c in children {
                out.push(c);
                out.extend(self.descendants(c));
            }
        }
        out
    }

    /// Le cadre (monde) d'une forme, rotation comprise
    pub fn bounds(&self, s: &Shape) -> [f32; 4] {
        if let Kind::Group { children, .. } = &s.kind {
            return children.iter().filter_map(|c| self.get(*c)).map(|c| self.bounds(c)).fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], union);
        }
        let local = local_bounds(s);
        let (cs, sn) = (s.rot.to_radians().cos(), s.rot.to_radians().sin());
        let pts = [[local[0], local[1]], [local[2], local[1]], [local[2], local[3]], [local[0], local[3]]].map(|[x, y]| [s.x + x * cs - y * sn, s.y + x * sn + y * cs]);
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for p in pts {
            b = union(b, [p[0], p[1], p[0], p[1]]);
        }
        let sw = s.strokes.iter().map(|k| k.width).fold(0.0f32, f32::max) / 2.0;
        [b[0] - sw, b[1] - sw, b[2] + sw, b[3] + sw]
    }

    /// Le cadre de toutes les formes données
    pub fn bounds_of(&self, ids: &[u64]) -> Option<[f32; 4]> {
        let b = ids.iter().filter_map(|i| self.get(*i)).map(|s| self.bounds(s)).fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], union);
        (b[0] <= b[2]).then_some(b)
    }

    /// Déplace une forme (et le contenu d'un groupe)
    pub fn translate(&mut self, id: u64, dx: f32, dy: f32) {
        let mut ids = vec![id];
        ids.extend(self.descendants(id));
        for s in self.shapes.iter_mut().filter(|s| ids.contains(&s.id)) {
            s.x += dx;
            s.y += dy;
        }
    }

    /// Supprime des formes (et leur contenu), les retire de leurs groupes
    pub fn remove(&mut self, ids: &[u64]) {
        let mut all: Vec<u64> = ids.to_vec();
        for &i in ids {
            all.extend(self.descendants(i));
        }
        self.shapes.retain(|s| !all.contains(&s.id));
        for s in &mut self.shapes {
            if let Kind::Group { children, .. } = &mut s.kind {
                children.retain(|c| !all.contains(c));
            }
        }
    }

    /// Regroupe des formes (racines) dans un nouveau groupe, à la place de la plus haute ; rend son identifiant
    pub fn group(&mut self, ids: &[u64], op: u8) -> Option<u64> {
        let mut members: Vec<usize> = ids.iter().filter_map(|i| self.index(*i)).collect();
        if members.is_empty() {
            return None;
        }
        members.sort();
        let parent = self.shapes[members[0]].parent;
        let gid = self.new_id();
        let name = match op {
            1 => "Union",
            2 => "Soustraction",
            3 => "Intersection",
            4 => "Exclusion",
            _ => "Groupe",
        };
        let children: Vec<u64> = members.iter().map(|&k| self.shapes[k].id).collect();
        // Le groupe prend l'origine de son cadre ; ses enfants gardent leur place
        let b = self.bounds_of(&children).unwrap_or([0.0; 4]);
        for s in self.shapes.iter_mut().filter(|s| children.contains(&s.id)) {
            s.parent = gid;
        }
        if parent != 0 {
            if let Some(Shape { kind: Kind::Group { children: pc, .. }, .. }) = self.shapes.iter_mut().find(|s| s.id == parent) {
                pc.retain(|c| !children.contains(c));
                pc.push(gid);
            }
        }
        let g = Shape { id: gid, name: name.into(), kind: Kind::Group { children, op }, x: b[0], y: b[1], parent, ..Shape::base() };
        let at = *members.last().unwrap() + 1;
        self.shapes.insert(at.min(self.shapes.len()), g);
        Some(gid)
    }

    /// Défait un groupe : ses formes reviennent à son parent
    pub fn ungroup(&mut self, gid: u64) {
        let Some(Shape { kind: Kind::Group { children, .. }, parent, .. }) = self.get(gid).cloned() else { return };
        for s in self.shapes.iter_mut().filter(|s| children.contains(&s.id)) {
            s.parent = parent;
        }
        if parent != 0 {
            if let Some(Shape { kind: Kind::Group { children: pc, .. }, .. }) = self.shapes.iter_mut().find(|s| s.id == parent) {
                pc.retain(|c| *c != gid);
                pc.extend(children.iter().copied());
            }
        }
        self.shapes.retain(|s| s.id != gid);
    }
}

fn union(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

impl Shape {
    pub fn base() -> Shape {
        Shape { id: 0, name: String::new(), kind: Kind::Rect { w: 100.0, h: 100.0, r: [0.0; 4] }, x: 0.0, y: 0.0, rot: 0.0, fills: vec![], strokes: vec![], opacity: 1.0, visible: true, locked: false, parent: 0, shadow: None, blur: 0.0, blend: String::new() }
    }

    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            Kind::Rect { .. } => "Rectangle",
            Kind::Ellipse { .. } => "Ellipse",
            Kind::Polygon { .. } => "Polygone",
            Kind::Star { .. } => "Étoile",
            Kind::Line { .. } => "Ligne",
            Kind::Path { .. } => "Tracé",
            Kind::Text { .. } => "Texte",
            Kind::Image { .. } => "Image",
            Kind::Group { op: 0, .. } => "Groupe",
            Kind::Group { .. } => "Booléen",
        }
    }
}

/// Cadre d'une forme dans son propre repère (avant rotation, origine en (x, y))
pub fn local_bounds(s: &Shape) -> [f32; 4] {
    match &s.kind {
        Kind::Rect { w, h, .. } | Kind::Ellipse { w, h } | Kind::Image { w, h, .. } => [0.0, 0.0, *w, *h],
        Kind::Polygon { r, .. } => [-r, -r, *r, *r],
        Kind::Star { r1, r2, .. } => {
            let r = r1.max(*r2);
            [-r, -r, r, r]
        }
        Kind::Line { x2, y2 } => [x2.min(0.0), y2.min(0.0), x2.max(0.0), y2.max(0.0)],
        Kind::Path { nodes, closed } => {
            // La courbe elle-même (échantillonnée), pas ses poignées
            let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
            let n = nodes.len();
            let segs = if *closed && n > 2 { n } else { n.saturating_sub(1) };
            for k in 0..segs {
                let (a, c) = (nodes[k], nodes[(k + 1) % n]);
                for step in 0..=12 {
                    let t = step as f32 / 12.0;
                    let u = 1.0 - t;
                    let q = [0, 1].map(|j| u * u * u * a.p[j] + 3.0 * u * u * t * a.o[j] + 3.0 * u * t * t * c.i[j] + t * t * t * c.p[j]);
                    b = union(b, [q[0], q[1], q[0], q[1]]);
                }
            }
            for nd in nodes {
                b = union(b, [nd.p[0], nd.p[1], nd.p[0], nd.p[1]]);
            }
            if b[0] > b[2] {
                [0.0; 4]
            } else {
                b
            }
        }
        Kind::Text { .. } => text_outline(s).1,
        Kind::Group { .. } => [0.0; 4],
    }
}

/// Le tracé SVG (attribut d) d'une forme, dans son repère
pub fn path_data(s: &Shape) -> String {
    let mut d = String::new();
    match &s.kind {
        Kind::Rect { w, h, r } => {
            let m = w.min(*h) / 2.0;
            let [a, b, c, e] = r.map(|x| x.clamp(0.0, m));
            let _ = write!(d, "M{a} 0H{}", w - b);
            if b > 0.0 {
                let _ = write!(d, "A{b} {b} 0 0 1 {w} {b}");
            }
            let _ = write!(d, "V{}", h - c);
            if c > 0.0 {
                let _ = write!(d, "A{c} {c} 0 0 1 {} {h}", w - c);
            }
            let _ = write!(d, "H{e}");
            if e > 0.0 {
                let _ = write!(d, "A{e} {e} 0 0 1 0 {}", h - e);
            }
            let _ = write!(d, "V{a}");
            if a > 0.0 {
                let _ = write!(d, "A{a} {a} 0 0 1 {a} 0");
            }
            d.push('Z');
        }
        Kind::Ellipse { w, h } => {
            let (rx, ry) = (w / 2.0, h / 2.0);
            let _ = write!(d, "M0 {ry}A{rx} {ry} 0 1 1 {w} {ry}A{rx} {ry} 0 1 1 0 {ry}Z");
        }
        Kind::Polygon { sides, r } => {
            let n = (*sides).max(3);
            for k in 0..n {
                let a = -std::f32::consts::FRAC_PI_2 + k as f32 / n as f32 * std::f32::consts::TAU;
                let _ = write!(d, "{}{} {}", if k == 0 { 'M' } else { 'L' }, r * a.cos(), r * a.sin());
            }
            d.push('Z');
        }
        Kind::Star { points, r1, r2 } => {
            let n = (*points).max(2) * 2;
            for k in 0..n {
                let a = -std::f32::consts::FRAC_PI_2 + k as f32 / n as f32 * std::f32::consts::TAU;
                let r = if k % 2 == 0 { r1 } else { r2 };
                let _ = write!(d, "{}{} {}", if k == 0 { 'M' } else { 'L' }, r * a.cos(), r * a.sin());
            }
            d.push('Z');
        }
        Kind::Line { x2, y2 } => {
            let _ = write!(d, "M0 0L{x2} {y2}");
        }
        Kind::Path { nodes, closed } => d = nodes_data(nodes, *closed),
        Kind::Text { .. } => d = text_outline(s).0,
        Kind::Image { .. } | Kind::Group { .. } => {}
    }
    d
}

pub fn nodes_data(nodes: &[Node], closed: bool) -> String {
    let mut d = String::new();
    let Some(first) = nodes.first() else { return d };
    let _ = write!(d, "M{} {}", first.p[0], first.p[1]);
    let seg = |d: &mut String, a: &Node, b: &Node| {
        if a.o == a.p && b.i == b.p {
            let _ = write!(d, "L{} {}", b.p[0], b.p[1]);
        } else {
            let _ = write!(d, "C{} {} {} {} {} {}", a.o[0], a.o[1], b.i[0], b.i[1], b.p[0], b.p[1]);
        }
    };
    for w in nodes.windows(2) {
        seg(&mut d, &w[0], &w[1]);
    }
    if closed && nodes.len() > 2 {
        seg(&mut d, nodes.last().unwrap(), first);
        d.push('Z');
    }
    d
}

/// Le texte vectorisé : son tracé et son cadre (police intégrée, retour à la ligne dans la zone)
pub fn text_outline(s: &Shape) -> (String, [f32; 4]) {
    use ab_glyph::{Font, ScaleFont};
    let Kind::Text { text, size, font, align, width, spacing, leading } = &s.kind else { return (String::new(), [0.0; 4]) };
    let f = crate::motion::font(font);
    let sf = f.as_scaled(*size);
    let line_h = (sf.ascent() - sf.descent() + sf.line_gap()) * leading.max(0.3);
    // Coupe en lignes : les retours, puis la largeur de la zone
    let advance = |t: &str| -> f32 { t.chars().map(|c| sf.h_advance(f.glyph_id(c)) + spacing).sum::<f32>() };
    let mut lines: Vec<String> = vec![];
    for para in text.split('\n') {
        match width {
            Some(wmax) if *wmax > 10.0 => {
                let mut cur = String::new();
                for word in para.split(' ') {
                    let cand = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
                    if advance(&cand) > *wmax && !cur.is_empty() {
                        lines.push(std::mem::take(&mut cur));
                        cur = word.to_string();
                    } else {
                        cur = cand;
                    }
                }
                lines.push(cur);
            }
            _ => lines.push(para.to_string()),
        }
    }
    let box_w = width.unwrap_or_else(|| lines.iter().map(|l| advance(l)).fold(0.0, f32::max));
    let mut d = String::new();
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for (li, line) in lines.iter().enumerate() {
        let lw = advance(line);
        let mut x = match align {
            1 => (box_w - lw) / 2.0,
            2 => box_w - lw,
            _ => 0.0,
        };
        let base = sf.ascent() + li as f32 * line_h;
        let mut prev = None;
        for c in line.chars() {
            let id = f.glyph_id(c);
            if let Some(p) = prev {
                x += sf.kern(p, id);
            }
            if let Some(o) = f.outline(id) {
                let (kx, ky) = (sf.h_scale_factor(), sf.v_scale_factor());
                let tr = |p: ab_glyph::Point| [x + p.x * kx, base - p.y * ky];
                let mut last: Option<[f32; 2]> = None;
                for curve in &o.curves {
                    use ab_glyph::OutlineCurve::*;
                    let (start, end) = match curve {
                        Line(a, b) => (tr(*a), tr(*b)),
                        Quad(a, _, b) => (tr(*a), tr(*b)),
                        Cubic(a, _, _, b) => (tr(*a), tr(*b)),
                    };
                    if last.is_none_or(|l| (l[0] - start[0]).abs() > 0.01 || (l[1] - start[1]).abs() > 0.01) {
                        if last.is_some() {
                            d.push('Z');
                        }
                        let _ = write!(d, "M{:.2} {:.2}", start[0], start[1]);
                    }
                    match curve {
                        Line(_, _) => {
                            let _ = write!(d, "L{:.2} {:.2}", end[0], end[1]);
                        }
                        Quad(_, c, _) => {
                            let c = tr(*c);
                            let _ = write!(d, "Q{:.2} {:.2} {:.2} {:.2}", c[0], c[1], end[0], end[1]);
                        }
                        Cubic(_, c1, c2, _) => {
                            let (c1, c2) = (tr(*c1), tr(*c2));
                            let _ = write!(d, "C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2}", c1[0], c1[1], c2[0], c2[1], end[0], end[1]);
                        }
                    }
                    b = union(b, [start[0], start[1], start[0], start[1]]);
                    b = union(b, [end[0], end[1], end[0], end[1]]);
                    last = Some(end);
                }
                if last.is_some() {
                    d.push('Z');
                }
            }
            x += sf.h_advance(id) + spacing;
            prev = Some(id);
        }
    }
    let total_h = lines.len().max(1) as f32 * line_h;
    let frame = [0.0, 0.0, box_w.max(1.0), total_h];
    if b[0] > b[2] {
        b = frame;
    }
    (d, union(frame, b))
}

fn hex(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Écrit le SVG du document. `area` : la zone du monde à montrer (x, y, w, h) ; `px` : taille de sortie en pixels.
/// `root` : le coffre (pour les images).
pub fn to_svg(doc: &Doc, area: [f32; 4], px: [u32; 2], root: &str, with_boards: bool) -> String {
    let mut s = String::new();
    let _ = write!(s, r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{}" height="{}" viewBox="{} {} {} {}">"#, px[0], px[1], area[0], area[1], area[2], area[3]);
    s.push_str("<defs>");
    let mut defs = String::new();
    let mut body = String::new();
    if with_boards {
        for b in &doc.boards {
            if let Some(c) = b.bg {
                let _ = write!(body, r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" fill-opacity="{}"/>"#, b.x, b.y, b.w, b.h, hex(c), c[3] as f32 / 255.0);
            }
        }
    }
    let mut cx = Gen { doc, root, defs: &mut defs, n: 0 };
    for sh in doc.shapes.iter().filter(|s| s.parent == 0) {
        cx.shape(&mut body, sh);
    }
    s.push_str(&defs);
    s.push_str("</defs>");
    s.push_str(&body);
    s.push_str("</svg>");
    s
}

struct Gen<'a> {
    doc: &'a Doc,
    root: &'a str,
    defs: &'a mut String,
    n: usize,
}

impl Gen<'_> {
    fn id(&mut self, p: &str) -> String {
        self.n += 1;
        format!("{p}{}", self.n)
    }

    fn paint(&mut self, p: &Paint, bounds: [f32; 4]) -> (String, f32) {
        match p {
            Paint::Solid { c } => (hex(*c), c[3] as f32 / 255.0),
            Paint::Linear { stops, angle } => {
                let id = self.id("g");
                let a = angle.to_radians();
                let (dx, dy) = (a.cos() * 0.5, a.sin() * 0.5);
                let _ = write!(self.defs, r#"<linearGradient id="{id}" x1="{}" y1="{}" x2="{}" y2="{}">"#, 0.5 - dx, 0.5 - dy, 0.5 + dx, 0.5 + dy);
                for (o, c) in stops {
                    let _ = write!(self.defs, r#"<stop offset="{o}" stop-color="{}" stop-opacity="{}"/>"#, hex(*c), c[3] as f32 / 255.0);
                }
                self.defs.push_str("</linearGradient>");
                let _ = bounds;
                (format!("url(#{id})"), 1.0)
            }
            Paint::Radial { stops } => {
                let id = self.id("g");
                let _ = write!(self.defs, r#"<radialGradient id="{id}">"#);
                for (o, c) in stops {
                    let _ = write!(self.defs, r#"<stop offset="{o}" stop-color="{}" stop-opacity="{}"/>"#, hex(*c), c[3] as f32 / 255.0);
                }
                self.defs.push_str("</radialGradient>");
                (format!("url(#{id})"), 1.0)
            }
        }
    }

    /// Les attributs d'effets et de transformation d'une forme
    fn wrap_open(&mut self, out: &mut String, sh: &Shape) {
        let mut style = String::new();
        if !sh.blend.is_empty() && sh.blend != "normal" {
            let _ = write!(style, "mix-blend-mode:{};", sh.blend);
        }
        let mut filter = String::new();
        if sh.shadow.is_some() || sh.blur > 0.0 {
            let id = self.id("f");
            let _ = write!(self.defs, r#"<filter id="{id}" x="-50%" y="-50%" width="200%" height="200%">"#);
            if let Some(d) = &sh.shadow {
                let _ = write!(self.defs, r#"<feDropShadow dx="{}" dy="{}" stdDeviation="{}" flood-color="{}" flood-opacity="{}"/>"#, d.dx, d.dy, d.blur / 2.0, hex(d.c), d.c[3] as f32 / 255.0);
            }
            if sh.blur > 0.0 {
                let _ = write!(self.defs, r#"<feGaussianBlur stdDeviation="{}"/>"#, sh.blur / 2.0);
            }
            self.defs.push_str("</filter>");
            filter = format!(r#" filter="url(#{id})""#);
        }
        let _ = write!(out, r#"<g transform="translate({} {}) rotate({})" opacity="{}"{}{}>"#, sh.x, sh.y, sh.rot, sh.opacity, filter, if style.is_empty() { String::new() } else { format!(r#" style="{style}""#) });
    }

    fn shape(&mut self, out: &mut String, sh: &Shape) {
        if !sh.visible {
            return;
        }
        if let Kind::Group { children, op } = &sh.kind {
            let kids: Vec<&Shape> = children.iter().filter_map(|c| self.doc.get(*c)).filter(|c| c.visible).collect();
            let _ = write!(out, r#"<g opacity="{}">"#, sh.opacity);
            match (op, kids.split_first()) {
                // Booléens : la première forme (la plus en arrière), masquée par les autres
                (2..=4, Some((first, rest))) if !rest.is_empty() => {
                    let mid = self.id("m");
                    let mut mask = String::new();
                    match op {
                        2 => {
                            let _ = write!(mask, r#"<mask id="{mid}" maskUnits="userSpaceOnUse" x="-100000" y="-100000" width="200000" height="200000"><rect x="-100000" y="-100000" width="200000" height="200000" fill="white"/>"#);
                            for k in rest {
                                self.silhouette(&mut mask, k, "black");
                            }
                        }
                        3 => {
                            let _ = write!(mask, r#"<mask id="{mid}" maskUnits="userSpaceOnUse" x="-100000" y="-100000" width="200000" height="200000">"#);
                            for k in rest {
                                self.silhouette(&mut mask, k, "white");
                            }
                        }
                        _ => {
                            let _ = write!(mask, r#"<mask id="{mid}" maskUnits="userSpaceOnUse" x="-100000" y="-100000" width="200000" height="200000"><rect x="-100000" y="-100000" width="200000" height="200000" fill="white"/>"#);
                            for k in rest {
                                self.silhouette(&mut mask, k, "black");
                            }
                        }
                    }
                    mask.push_str("</mask>");
                    self.defs.push_str(&mask);
                    let _ = write!(out, r#"<g mask="url(#{mid})">"#);
                    self.shape(out, first);
                    out.push_str("</g>");
                    // Exclusion : les autres, privées de la première, s'ajoutent
                    if *op == 4 {
                        let m2 = self.id("m");
                        let mut mask2 = format!(r#"<mask id="{m2}" maskUnits="userSpaceOnUse" x="-100000" y="-100000" width="200000" height="200000"><rect x="-100000" y="-100000" width="200000" height="200000" fill="white"/>"#);
                        self.silhouette(&mut mask2, first, "black");
                        mask2.push_str("</mask>");
                        self.defs.push_str(&mask2);
                        let _ = write!(out, r#"<g mask="url(#{m2})">"#);
                        for k in rest {
                            self.shape_with_paint(out, k, first);
                        }
                        out.push_str("</g>");
                    }
                }
                // Union : toutes avec la peinture de la première
                (1, Some((first, rest))) => {
                    self.shape(out, first);
                    for k in rest {
                        self.shape_with_paint(out, k, first);
                    }
                }
                _ => {
                    for k in kids {
                        self.shape(out, k);
                    }
                }
            }
            out.push_str("</g>");
            return;
        }
        self.wrap_open(out, sh);
        let bounds = local_bounds(sh);
        match &sh.kind {
            Kind::Image { path, w, h } => {
                let p = std::path::Path::new(self.root).join(path);
                let mime = if path.to_lowercase().ends_with(".png") { "png" } else { "jpeg" };
                if let Ok(bytes) = std::fs::read(crate::bridge::media(&p)) {
                    use base64::Engine;
                    let _ = write!(out, r#"<image width="{w}" height="{h}" preserveAspectRatio="none" xlink:href="data:image/{mime};base64,{}"/>"#, base64::engine::general_purpose::STANDARD.encode(bytes));
                }
            }
            _ => {
                let d = path_data(sh);
                let closed = !matches!(sh.kind, Kind::Line { .. } | Kind::Path { closed: false, .. });
                for f in &sh.fills {
                    if !closed && !matches!(sh.kind, Kind::Path { .. }) {
                        continue;
                    }
                    let (p, o) = self.paint(f, bounds);
                    let _ = write!(out, r#"<path d="{d}" fill="{p}" fill-opacity="{o}" fill-rule="nonzero"/>"#);
                }
                for k in &sh.strokes {
                    let (p, o) = self.paint(&k.paint, bounds);
                    let cap = ["butt", "round", "square"][k.cap.min(2) as usize];
                    let join = ["miter", "round", "bevel"][k.join.min(2) as usize];
                    let dash = if k.dash.is_empty() { String::new() } else { format!(r#" stroke-dasharray="{}""#, k.dash.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ")) };
                    let _ = write!(out, r#"<path d="{d}" fill="none" stroke="{p}" stroke-opacity="{o}" stroke-width="{}" stroke-linecap="{cap}" stroke-linejoin="{join}"{dash}/>"#, k.width);
                    if k.arrow && !closed {
                        if let Some((tip, dir)) = end_dir(sh) {
                            let l = k.width * 3.5 + 6.0;
                            let (nx, ny) = (-dir[1], dir[0]);
                            let a = [tip[0] - dir[0] * l + nx * l * 0.55, tip[1] - dir[1] * l + ny * l * 0.55];
                            let b = [tip[0] - dir[0] * l - nx * l * 0.55, tip[1] - dir[1] * l - ny * l * 0.55];
                            let _ = write!(out, r#"<path d="M{} {}L{} {}L{} {}Z" fill="{p}" fill-opacity="{o}"/>"#, tip[0], tip[1], a[0], a[1], b[0], b[1]);
                        }
                    }
                }
            }
        }
        out.push_str("</g>");
    }

    /// Une forme peinte comme une autre (les membres d'une union prennent la peinture de la première)
    fn shape_with_paint(&mut self, out: &mut String, sh: &Shape, like: &Shape) {
        let mut c = sh.clone();
        c.fills = like.fills.clone();
        c.strokes = like.strokes.clone();
        self.shape(out, &c);
    }

    /// La silhouette pleine d'une forme (pour les masques des booléens)
    fn silhouette(&mut self, out: &mut String, sh: &Shape, color: &str) {
        if let Kind::Group { children, .. } = &sh.kind {
            for c in children.clone() {
                if let Some(k) = self.doc.get(c) {
                    self.silhouette(out, k, color);
                }
            }
            return;
        }
        let d = path_data(sh);
        let _ = write!(out, r#"<path transform="translate({} {}) rotate({})" d="{d}" fill="{color}"/>"#, sh.x, sh.y, sh.rot);
    }
}

/// Le bout d'une ligne ou d'un tracé ouvert, et la direction d'arrivée (unitaire)
fn end_dir(sh: &Shape) -> Option<([f32; 2], [f32; 2])> {
    let (tip, from) = match &sh.kind {
        Kind::Line { x2, y2 } => ([*x2, *y2], [0.0, 0.0]),
        Kind::Path { nodes, .. } if nodes.len() > 1 => {
            let last = nodes[nodes.len() - 1];
            let prev = if last.i != last.p { last.i } else { nodes[nodes.len() - 2].o };
            (last.p, prev)
        }
        _ => return None,
    };
    let (dx, dy) = (tip[0] - from[0], tip[1] - from[1]);
    let l = (dx * dx + dy * dy).sqrt();
    (l > 1e-3).then(|| (tip, [dx / l, dy / l]))
}

/// Rend le SVG en RVBA (prémultiplié par resvg, rendu en couleurs droites ici)
pub fn rasterize(svg: &str, w: u32, h: u32) -> Option<Vec<u8>> {
    use resvg::{tiny_skia, usvg};
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg, &opt).ok()?;
    let mut pm = tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pm.as_mut());
    let mut px = pm.take();
    for c in px.chunks_exact_mut(4) {
        let a = c[3] as f32 / 255.0;
        if a > 0.0 && a < 1.0 {
            for k in 0..3 {
                c[k] = (c[k] as f32 / a).round().min(255.0) as u8;
            }
        }
    }
    Some(px)
}

/// Exporte un plan de travail en SVG (fichier autonome)
pub fn export_svg(doc: &Doc, board: usize, root: &str) -> Option<String> {
    let b = doc.boards.get(board)?;
    Some(to_svg_board(doc, b, root, 1.0))
}

fn to_svg_board(doc: &Doc, b: &Board, root: &str, scale: f32) -> String {
    let mut d = doc.clone();
    d.boards = vec![b.clone()];
    to_svg(&d, [b.x, b.y, b.w, b.h], [(b.w * scale).round().max(1.0) as u32, (b.h * scale).round().max(1.0) as u32], root, true)
}

/// Un plan de travail en image, à `scale` (1, 2, 3…)
pub fn export_png(doc: &Doc, board: usize, root: &str, scale: f32) -> Option<image::RgbaImage> {
    let b = doc.boards.get(board)?;
    let (w, h) = ((b.w * scale).round().max(1.0) as u32, (b.h * scale).round().max(1.0) as u32);
    let px = rasterize(&to_svg_board(doc, b, root, scale), w, h)?;
    image::RgbaImage::from_raw(w, h, px)
}

/// Aperçu d'un fichier .aryvec : le premier plan de travail (pour les miniatures, le board, le montage)
pub fn thumbnail(path: &Path, max: u32) -> Result<image::RgbaImage, String> {
    let doc = Doc::load(path)?;
    let root = crate::nodes::vault_root();
    let b = doc.boards.first().ok_or("aucun plan de travail")?;
    let k = (max as f32 / b.w.max(b.h).max(1.0)).min(4.0);
    export_png(&doc, 0, &root, k).ok_or_else(|| "rendu impossible".to_string())
}

/// Importe un SVG : chaque tracé devient une forme (couleurs unies, contours) ; rend le nombre de formes
pub fn import_svg(doc: &mut Doc, svg: &str, at: [f32; 2]) -> Result<usize, String> {
    use resvg::usvg;
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let mut n = 0;
    fn walk(g: &usvg::Group, doc: &mut Doc, at: [f32; 2], n: &mut usize) {
        for node in g.children() {
            match node {
                usvg::Node::Group(gg) => walk(gg, doc, at, n),
                usvg::Node::Path(p) => {
                    let t = p.abs_transform();
                    let mut nodes: Vec<Node> = vec![];
                    let mut subpaths: Vec<(Vec<Node>, bool)> = vec![];
                    let tp = |x: f32, y: f32| [t.sx * x + t.kx * y + t.tx + at[0], t.ky * x + t.sy * y + t.ty + at[1]];
                    for seg in p.data().segments() {
                        use usvg::tiny_skia_path::PathSegment::*;
                        match seg {
                            MoveTo(a) => {
                                if !nodes.is_empty() {
                                    subpaths.push((std::mem::take(&mut nodes), false));
                                }
                                nodes.push(Node::corner(tp(a.x, a.y)));
                            }
                            LineTo(a) => nodes.push(Node::corner(tp(a.x, a.y))),
                            QuadTo(c, a) => {
                                let last = nodes.last().map(|n| n.p).unwrap_or([0.0, 0.0]);
                                let c = tp(c.x, c.y);
                                let e = tp(a.x, a.y);
                                let c1 = [last[0] + (c[0] - last[0]) * 2.0 / 3.0, last[1] + (c[1] - last[1]) * 2.0 / 3.0];
                                let c2 = [e[0] + (c[0] - e[0]) * 2.0 / 3.0, e[1] + (c[1] - e[1]) * 2.0 / 3.0];
                                if let Some(l) = nodes.last_mut() {
                                    l.o = c1;
                                }
                                nodes.push(Node { p: e, i: c2, o: e });
                            }
                            CubicTo(c1, c2, a) => {
                                if let Some(l) = nodes.last_mut() {
                                    l.o = tp(c1.x, c1.y);
                                }
                                let e = tp(a.x, a.y);
                                nodes.push(Node { p: e, i: tp(c2.x, c2.y), o: e });
                            }
                            Close => {
                                // Le point de fermeture répète le premier : on le fond
                                if nodes.len() > 2 && nodes.last().map(|l| l.p) == nodes.first().map(|f| f.p) {
                                    let l = nodes.pop().unwrap();
                                    nodes[0].i = l.i;
                                }
                                subpaths.push((std::mem::take(&mut nodes), true));
                            }
                        }
                    }
                    if !nodes.is_empty() {
                        subpaths.push((nodes, false));
                    }
                    let fill = p.fill().and_then(|f| match f.paint() {
                        usvg::Paint::Color(c) => Some(Paint::solid([c.red, c.green, c.blue, (f.opacity().get() * 255.0) as u8])),
                        _ => Some(Paint::solid([128, 128, 128, 255])),
                    });
                    let stroke = p.stroke().map(|s| {
                        let c = match s.paint() {
                            usvg::Paint::Color(c) => [c.red, c.green, c.blue, (s.opacity().get() * 255.0) as u8],
                            _ => [0, 0, 0, 255],
                        };
                        Stroke { paint: Paint::solid(c), width: s.width().get() * t.sx.abs().max(t.sy.abs()), cap: 1, join: 1, dash: vec![], arrow: false }
                    });
                    for (sp, closed) in subpaths {
                        if sp.len() < 2 {
                            continue;
                        }
                        let id = doc.new_id();
                        doc.shapes.push(Shape { id, name: format!("Tracé {id}"), kind: Kind::Path { nodes: sp, closed }, fills: fill.clone().into_iter().collect(), strokes: stroke.clone().into_iter().collect(), ..Shape::base() });
                        *n += 1;
                    }
                }
                _ => {}
            }
        }
    }
    walk(tree.root(), doc, at, &mut n);
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendu_et_svg() {
        let mut d = Doc::starter();
        let id = d.new_id();
        d.shapes.push(Shape { id, name: "carré".into(), kind: Kind::Rect { w: 200.0, h: 100.0, r: [10.0; 4] }, x: 100.0, y: 100.0, fills: vec![Paint::solid([255, 0, 0, 255])], ..Shape::base() });
        let id2 = d.new_id();
        d.shapes.push(Shape { id: id2, name: "rond".into(), kind: Kind::Ellipse { w: 100.0, h: 100.0 }, x: 150.0, y: 100.0, fills: vec![Paint::solid([0, 0, 255, 255])], ..Shape::base() });
        let svg = export_svg(&d, 0, "/").unwrap();
        assert!(svg.contains("<path") && svg.contains("#ff0000"));
        let img = export_png(&d, 0, "/", 0.5).unwrap();
        assert_eq!(img.width(), 540);
        // Rouge dans le rectangle, à gauche du rond ; fond blanc ailleurs
        assert_eq!(img.get_pixel(60, 75).0[..3], [255, 0, 0]);
        assert_eq!(img.get_pixel(10, 10).0[..3], [255, 255, 255]);
        // Soustraction : le rond creuse le rectangle (il devient blanc là où il était bleu)
        let g = d.group(&[id, id2], 2).unwrap();
        let img = export_png(&d, 0, "/", 0.5).unwrap();
        let px = img.get_pixel(100, 75).0;
        assert!(px[0] > 240 && px[1] > 240 && px[2] > 240, "{px:?}");
        assert_eq!(img.get_pixel(60, 75).0[..3], [255, 0, 0]);
        d.ungroup(g);
        assert!(d.shapes.iter().all(|s| s.parent == 0) && d.shapes.len() == 2);
        // Le texte devient des tracés
        let tid = d.new_id();
        d.shapes.push(Shape { id: tid, name: "titre".into(), kind: Kind::Text { text: "Aurore".into(), size: 80.0, font: "inter-bold".into(), align: 0, width: None, spacing: 0.0, leading: 1.0 }, x: 300.0, y: 600.0, fills: vec![Paint::solid([0, 0, 0, 255])], ..Shape::base() });
        let t = d.get(tid).unwrap();
        let (path, b) = text_outline(t);
        assert!(path.len() > 100 && b[2] > 150.0);
        // Aller-retour JSON
        let back: Doc = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(back, d);
        // Import d'un SVG
        let mut e = Doc::starter();
        let n = import_svg(&mut e, r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><path d="M0 0L10 0L10 10Z" fill="red"/><circle cx="5" cy="5" r="3" fill="none" stroke="black"/></svg>"#, [0.0, 0.0]).unwrap();
        assert_eq!(n, 2);
        assert!(matches!(&e.shapes[0].kind, Kind::Path { closed: true, nodes } if nodes.len() == 3));
    }
}
