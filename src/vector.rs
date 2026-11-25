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
    /// Grille de mise en page (colonnes), affichée et aimantée, jamais exportée
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<LayoutGrid>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct LayoutGrid {
    pub cols: u32,
    pub gutter: f32,
    pub margin: f32,
}

impl Default for LayoutGrid {
    fn default() -> Self {
        LayoutGrid { cols: 12, gutter: 20.0, margin: 40.0 }
    }
}

impl Board {
    /// Bords gauche et droit de chaque colonne (monde)
    pub fn columns(&self) -> Vec<(f32, f32)> {
        let Some(g) = self.grid else { return vec![] };
        let n = g.cols.max(1) as f32;
        let inner = (self.w - 2.0 * g.margin - (n - 1.0) * g.gutter).max(1.0);
        let cw = inner / n;
        (0..g.cols.max(1)).map(|k| {
            let x = self.x + g.margin + k as f32 * (cw + g.gutter);
            (x, x + cw)
        }).collect()
    }
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
    /// Groupe en mise en page automatique (comme Figma) : ses formes se suivent, avec un écart et des marges
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<AutoLayout>,
    /// Groupe maître d'un composant (comme Figma) : ses instances le suivent
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub component: bool,
    /// Instance du composant `instance_of` (0 : aucun)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub instance_of: u64,
    /// Points de largeur du contour (outil Largeur, comme Illustrator) : position relative, largeur
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub widths: Vec<[f32; 2]>,
    /// Contraintes quand le plan de travail change de taille (comme Figma) : horizontale, verticale.
    /// 0 début (gauche, haut), 1 fin (droite, bas), 2 les deux (étirer), 3 centre, 4 échelle
    #[serde(default, skip_serializing_if = "is_default_c")]
    pub constraint: [u8; 2],
    /// Style partagé utilisé (vide : aucun)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub style: String,
    /// Prototype : un clic sur la forme mène à ce plan de travail (son nom ; vide : aucun lien)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link: String,
}

fn is_default_c(c: &[u8; 2]) -> bool {
    *c == [0, 0]
}

/// Où va un segment [s0, s1] quand son cadre passe de [a0, a1] à [b0, b1], selon la contrainte
pub fn constrain(c: u8, s0: f32, s1: f32, a0: f32, a1: f32, b0: f32, b1: f32) -> (f32, f32) {
    match c {
        1 => (s0 + b1 - a1, s1 + b1 - a1),
        2 => (s0 + b0 - a0, s1 + b1 - a1),
        3 => {
            let d = ((b0 + b1) - (a0 + a1)) / 2.0;
            (s0 + d, s1 + d)
        }
        4 => {
            let k = (b1 - b0) / (a1 - a0).max(1e-3);
            (b0 + (s0 - a0) * k, b0 + (s1 - a0) * k)
        }
        _ => (s0 + b0 - a0, s1 + b0 - a0),
    }
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AutoLayout {
    /// 0 en ligne, 1 en colonne
    pub dir: u8,
    pub gap: f32,
    /// Marges : horizontale, verticale
    pub pad: [f32; 2],
    /// Alignement sur l'autre axe : 0 début, 1 centre, 2 fin
    #[serde(default)]
    pub align: u8,
    /// Fond du cadre (None : transparent) et rayon de ses coins
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<[u8; 4]>,
    #[serde(default)]
    pub radius: f32,
    /// Taille calculée du cadre (marges comprises)
    #[serde(default)]
    pub size: [f32; 2],
}

impl Default for AutoLayout {
    fn default() -> Self {
        AutoLayout { dir: 0, gap: 16.0, pad: [24.0, 16.0], align: 1, bg: None, radius: 8.0, size: [0.0; 2] }
    }
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
    /// Styles partagés (comme Figma) : changer un style change toutes les formes qui l'utilisent
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<NamedStyle>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NamedStyle {
    pub name: String,
    pub fills: Vec<Paint>,
    pub strokes: Vec<Stroke>,
    #[serde(default)]
    pub shadow: Option<Shadow>,
    #[serde(default)]
    pub blur: f32,
    #[serde(default = "one")]
    pub opacity: f32,
    /// Style de texte : police, taille, interligne, interlettrage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<(String, f32, f32, f32)>,
}

impl NamedStyle {
    pub fn of(name: &str, s: &Shape) -> NamedStyle {
        let text = match &s.kind {
            Kind::Text { font, size, leading, spacing, .. } => Some((font.clone(), *size, *leading, *spacing)),
            _ => None,
        };
        NamedStyle { name: name.to_string(), fills: s.fills.clone(), strokes: s.strokes.clone(), shadow: s.shadow.clone(), blur: s.blur, opacity: s.opacity, text }
    }

    /// Pose le style sur une forme
    pub fn apply(&self, s: &mut Shape) {
        s.fills = self.fills.clone();
        s.strokes = self.strokes.clone();
        s.shadow = self.shadow.clone();
        s.blur = self.blur;
        s.opacity = self.opacity;
        s.style = self.name.clone();
        if let (Some((f, z, l, sp)), Kind::Text { font, size, leading, spacing, .. }) = (&self.text, &mut s.kind) {
            (*font, *size, *leading, *spacing) = (f.clone(), *z, *l, *sp);
        }
    }
}

impl Doc {
    /// Enregistre le style (nouveau ou mis à jour) et le repose sur toutes les formes qui l'utilisent
    pub fn set_style(&mut self, st: NamedStyle) -> usize {
        match self.styles.iter_mut().find(|x| x.name == st.name) {
            Some(x) => *x = st.clone(),
            None => self.styles.push(st.clone()),
        }
        let mut n = 0;
        for s in self.shapes.iter_mut().filter(|s| s.style == st.name) {
            st.apply(s);
            n += 1;
        }
        n
    }
}

impl Doc {
    pub fn starter() -> Doc {
        Doc {
            boards: vec![Board { name: "Plan de travail 1".into(), x: 0.0, y: 0.0, w: 1080.0, h: 1080.0, bg: Some([255, 255, 255, 255]), grid: None }],
            shapes: vec![],
            next_id: 1,
            swatches: vec![[24, 24, 27, 255], [255, 255, 255, 255], [236, 108, 89, 255], [244, 191, 80, 255], [86, 166, 122, 255], [84, 128, 220, 255], [150, 110, 210, 255]],
            styles: vec![],
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

    /// Le contenu d'un groupe, en profondeur (le groupe exclu), dans l'ordre de ses enfants
    pub fn preorder(&self, id: u64) -> Vec<u64> {
        let mut out = vec![];
        if let Some(Shape { kind: Kind::Group { children, .. }, .. }) = self.get(id) {
            for c in children {
                if out.len() > 10_000 {
                    break;
                }
                out.push(*c);
                out.extend(self.preorder(*c));
            }
        }
        out
    }

    /// Recopie chaque composant dans ses instances, décalé à leur place.
    /// Le texte d'une instance reste le sien (comme un remplacement Figma).
    pub fn sync_instances(&mut self) {
        let insts: Vec<(u64, u64)> = self.shapes.iter().filter(|s| s.instance_of != 0).map(|s| (s.id, s.instance_of)).collect();
        for (iid, mid) in insts {
            let Some(m) = self.get(mid).filter(|m| m.component && matches!(m.kind, Kind::Group { .. })).cloned() else { continue };
            let mpre = self.preorder(mid);
            if mpre.contains(&iid) || self.preorder(iid).contains(&mid) {
                continue;
            }
            let Some(inst) = self.get(iid).cloned() else { continue };
            let (dx, dy) = (inst.x - m.x, inst.y - m.y);
            let ipre = self.preorder(iid);
            let same = ipre.len() == mpre.len()
                && mpre.iter().zip(&ipre).all(|(a, b)| match (self.get(*a), self.get(*b)) {
                    (Some(x), Some(y)) => std::mem::discriminant(&x.kind) == std::mem::discriminant(&y.kind) && matches!((&x.kind, &y.kind), (Kind::Group { children: c1, .. }, Kind::Group { children: c2, .. }) if c1.len() == c2.len()) == matches!(x.kind, Kind::Group { .. }),
                    _ => false,
                });
            let new_ids: Vec<u64> = if same { ipre.clone() } else { mpre.iter().map(|_| self.new_id()).collect() };
            let map = |id: u64| if id == mid { iid } else { mpre.iter().position(|x| *x == id).map(|k| new_ids[k]).unwrap_or(id) };
            let mut copies = vec![];
            for (k, src) in mpre.iter().enumerate() {
                let Some(mut c) = self.get(*src).cloned() else { continue };
                c.id = new_ids[k];
                c.parent = map(c.parent);
                c.x += dx;
                c.y += dy;
                c.component = false;
                if let Kind::Group { children, .. } = &mut c.kind {
                    *children = children.iter().map(|x| map(*x)).collect();
                }
                // Le texte remplacé dans l'instance est gardé
                if same {
                    if let (Kind::Text { text, .. }, Some(Shape { kind: Kind::Text { text: own, .. }, .. })) = (&mut c.kind, self.get(new_ids[k])) {
                        *text = own.clone();
                    }
                }
                copies.push(c);
            }
            if !same {
                self.remove(&ipre);
            }
            for c in copies {
                match self.index(c.id) {
                    Some(i) => self.shapes[i] = c,
                    None => self.shapes.push(c),
                }
            }
            if let Some(i) = self.index(iid) {
                let s = &mut self.shapes[i];
                if let (Kind::Group { children, op }, Kind::Group { children: mc, op: mop }) = (&mut s.kind, &m.kind) {
                    *children = mc.iter().map(|x| map(*x)).collect();
                    *op = *mop;
                }
                s.layout = m.layout.clone();
                s.fills = m.fills.clone();
                s.strokes = m.strokes.clone();
                s.shadow = m.shadow.clone();
                s.blur = m.blur;
                s.blend = m.blend.clone();
            }
        }
    }

    /// Une nouvelle instance du composant `mid`, posée à côté ; rend son identifiant
    pub fn instantiate(&mut self, mid: u64) -> Option<u64> {
        let m = self.get(mid).filter(|m| m.component)?.clone();
        let b = self.bounds(&m);
        let id = self.new_id();
        let name = format!("{} · {}", m.name, crate::i18n::t("instance"));
        self.shapes.push(Shape { id, name, kind: Kind::Group { children: vec![], op: 0 }, x: b[2] + 40.0, y: m.y, instance_of: mid, ..Shape::base() });
        self.sync_instances();
        Some(id)
    }

    /// Place les formes des groupes en mise en page automatique (les groupes intérieurs d'abord).
    pub fn apply_layouts(&mut self) {
        // Les plus profonds d'abord : un bouton dans une barre se calcule avant la barre
        let depth = |d: &Doc, mut id: u64| {
            let mut n = 0;
            while let Some(p) = d.get(id).map(|s| s.parent).filter(|p| *p != 0) {
                id = p;
                n += 1;
                if n > 64 {
                    break;
                }
            }
            n
        };
        let mut groups: Vec<(usize, u64)> = self.shapes.iter().filter(|s| s.layout.is_some() && matches!(s.kind, Kind::Group { .. })).map(|s| (depth(self, s.id), s.id)).collect();
        groups.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, gid) in groups {
            let Some(g) = self.get(gid).cloned() else { continue };
            let (Kind::Group { children, .. }, Some(l)) = (&g.kind, &g.layout) else { continue };
            let kids: Vec<u64> = children.iter().copied().filter(|c| self.get(*c).is_some_and(|s| s.visible)).collect();
            let sizes: Vec<[f32; 4]> = kids.iter().map(|c| self.bounds(self.get(*c).unwrap())).collect();
            let (main, cross) = if l.dir == 0 { (0, 1) } else { (1, 0) };
            let span = |b: &[f32; 4], a: usize| b[a + 2] - b[a];
            let cross_max = sizes.iter().map(|b| span(b, cross)).fold(0.0f32, f32::max);
            let mut cur = [g.x + l.pad[0], g.y + l.pad[1]];
            for (c, b) in kids.iter().zip(&sizes) {
                let free = cross_max - span(b, cross);
                let off = match l.align {
                    0 => 0.0,
                    2 => free,
                    _ => free / 2.0,
                };
                let mut want = cur;
                want[cross] += off;
                let (dx, dy) = (want[0] - b[0], want[1] - b[1]);
                if dx.abs() > 1e-3 || dy.abs() > 1e-3 {
                    self.translate(*c, dx, dy);
                }
                cur[main] += span(b, main) + l.gap;
            }
            let content = if kids.is_empty() { 0.0 } else { cur[main] - l.gap - (if main == 0 { g.x } else { g.y }) - l.pad[main] };
            let mut size = [0.0; 2];
            size[main] = content + 2.0 * l.pad[main];
            size[cross] = cross_max + 2.0 * l.pad[cross];
            if let Some(Shape { layout: Some(ll), .. }) = self.shapes.iter_mut().find(|s| s.id == gid) {
                ll.size = size;
            }
        }
    }

    /// Le cadre (monde) d'une forme, rotation comprise
    pub fn bounds(&self, s: &Shape) -> [f32; 4] {
        if let (Kind::Group { .. }, Some(l)) = (&s.kind, &s.layout) {
            if l.size[0] > 0.0 {
                return [s.x, s.y, s.x + l.size[0], s.y + l.size[1]];
            }
        }
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
            5 => "Pair-impair",
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
        Shape { id: 0, name: String::new(), kind: Kind::Rect { w: 100.0, h: 100.0, r: [0.0; 4] }, x: 0.0, y: 0.0, rot: 0.0, fills: vec![], strokes: vec![], opacity: 1.0, visible: true, locked: false, parent: 0, shadow: None, blur: 0.0, blend: String::new(), layout: None, component: false, instance_of: 0, widths: vec![], constraint: [0, 0], style: String::new(), link: String::new() }
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
            // Fond d'un cadre en mise en page automatique
            if let Some(l) = sh.layout.as_ref().filter(|l| l.size[0] > 0.0) {
                if let Some(c) = l.bg {
                    let _ = write!(out, r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="rgb({},{},{})" fill-opacity="{:.3}"/>"#, sh.x, sh.y, l.size[0], l.size[1], l.radius, c[0], c[1], c[2], c[3] as f32 / 255.0);
                }
            }
            match (op, kids.split_first()) {
                // Pair-impair : tous les contours en un seul tracé (les trous dans les formes restent vides)
                (5, Some((first, _))) => {
                    let mut d = String::new();
                    for k in &kids {
                        if let Kind::Path { nodes, .. } = &k.kind {
                            let (s, c) = k.rot.to_radians().sin_cos();
                            let tp = |p: [f32; 2]| [k.x + p[0] * c - p[1] * s, k.y + p[0] * s + p[1] * c];
                            if let Some(n0) = nodes.first() {
                                let a = tp(n0.p);
                                let _ = write!(d, "M{:.2} {:.2}", a[0], a[1]);
                                for w in 0..nodes.len() {
                                    let (n1, n2) = (&nodes[w], &nodes[(w + 1) % nodes.len()]);
                                    let (o, i, p) = (tp(n1.o), tp(n2.i), tp(n2.p));
                                    let _ = write!(d, "C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2}", o[0], o[1], i[0], i[1], p[0], p[1]);
                                }
                                d.push('Z');
                            }
                        }
                    }
                    let col = match first.fills.first() {
                        Some(Paint::Solid { c }) => *c,
                        _ => [0, 0, 0, 255],
                    };
                    let _ = write!(out, r#"<path d="{d}" fill-rule="evenodd" fill="rgb({},{},{})" fill-opacity="{:.3}"/>"#, col[0], col[1], col[2], col[3] as f32 / 255.0);
                }
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
                // Contour à largeur variable : un tracé plein, peint comme le contour
                if !sh.widths.is_empty() {
                    if let (Some(k), Some(wd)) = (sh.strokes.first(), crate::vgeom::width_outline(sh)) {
                        let (p, o) = self.paint(&k.paint, bounds);
                        let _ = write!(out, r#"<path d="{wd}" fill="{p}" fill-opacity="{o}" fill-rule="nonzero"/>"#);
                        out.push_str("</g>");
                        return;
                    }
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
    // Les images (PNG, JPEG, WebP) se décodent ici (resvg est compilé sans) : on dessine nœud par nœud,
    // dans l'ordre, chaque image à sa place entre les formes
    fn has_image(g: &usvg::Group) -> bool {
        g.children().iter().any(|n| match n {
            usvg::Node::Image(_) => true,
            usvg::Node::Group(gg) => has_image(gg),
            _ => false,
        })
    }
    fn draw(g: &usvg::Group, pm: &mut tiny_skia::Pixmap) {
        for node in g.children() {
            match node {
                usvg::Node::Group(gg) if has_image(gg) => draw(gg, pm),
                usvg::Node::Image(im) if im.is_visible() => {
                    let data: &[u8] = match im.kind() {
                        usvg::ImageKind::PNG(d) | usvg::ImageKind::JPEG(d) | usvg::ImageKind::WEBP(d) | usvg::ImageKind::GIF(d) => d,
                        _ => continue,
                    };
                    let Ok(img) = image::load_from_memory(data) else { continue };
                    let rgba = img.to_rgba8();
                    let Some(mut src) = tiny_skia::Pixmap::new(rgba.width(), rgba.height()) else { continue };
                    for (d, s) in src.pixels_mut().iter_mut().zip(rgba.pixels()) {
                        *d = tiny_skia::ColorU8::from_rgba(s[0], s[1], s[2], s[3]).premultiply();
                    }
                    let sz = im.size();
                    let t = im.abs_transform().pre_scale(sz.width() / rgba.width() as f32, sz.height() / rgba.height() as f32);
                    pm.draw_pixmap(0, 0, src.as_ref(), &tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() }, t, None);
                }
                n => {
                    if let Some(b) = n.abs_layer_bounding_box() {
                        // render_node retire le coin du cadre ; on le rend, et on ajoute le repère du parent
                        resvg::render_node(n, g.abs_transform().pre_translate(b.x(), b.y()), &mut pm.as_mut());
                    }
                }
            }
        }
    }
    if has_image(tree.root()) {
        draw(tree.root(), &mut pm);
    } else {
        resvg::render(&tree, tiny_skia::Transform::identity(), &mut pm.as_mut());
    }
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

// ---------- vectorisation d'une image ----------

/// Vectorise une image en noir et blanc : les zones plus sombres que `threshold` (0 à 1) deviennent des tracés.
/// `detail` : tolérance de simplification en pixels (petit : fidèle, grand : peu de points).
/// Renvoie les contours (en pixels de l'image) et la couleur moyenne des zones.
pub fn trace(img: &image::RgbaImage, threshold: f32, detail: f32) -> (Vec<Vec<Node>>, [u8; 3]) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let inside = |x: i32, y: i32| -> bool {
        if x < 0 || y < 0 || x >= w || y >= h {
            return false;
        }
        let p = img.get_pixel(x as u32, y as u32);
        let l = (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0;
        p[3] > 127 && l < threshold
    };
    // Couleur moyenne de l'encre
    let (mut acc, mut cnt) = ([0u64; 3], 0u64);
    for y in 0..h {
        for x in 0..w {
            if inside(x, y) {
                let p = img.get_pixel(x as u32, y as u32);
                for k in 0..3 {
                    acc[k] += p[k] as u64;
                }
                cnt += 1;
            }
        }
    }
    let color = if cnt == 0 { [0, 0, 0] } else { [0, 1, 2].map(|k| (acc[k] / cnt) as u8) };
    // Arêtes du bord des pixels, orientées (l'intérieur à gauche) : départ → arrivée
    let mut next: std::collections::HashMap<(i32, i32), Vec<(i32, i32)>> = std::collections::HashMap::new();
    for y in 0..h {
        for x in 0..w {
            if !inside(x, y) {
                continue;
            }
            if !inside(x, y - 1) {
                next.entry((x, y)).or_default().push((x + 1, y));
            }
            if !inside(x + 1, y) {
                next.entry((x + 1, y)).or_default().push((x + 1, y + 1));
            }
            if !inside(x, y + 1) {
                next.entry((x + 1, y + 1)).or_default().push((x, y + 1));
            }
            if !inside(x - 1, y) {
                next.entry((x, y + 1)).or_default().push((x, y));
            }
        }
    }
    // Enchaîne les arêtes en boucles
    let mut loops: Vec<Vec<[f32; 2]>> = vec![];
    let mut starts: Vec<(i32, i32)> = next.keys().copied().collect();
    starts.sort();
    for s0 in starts {
        while let Some(list) = next.get_mut(&s0) {
            let Some(mut cur) = list.pop() else { break };
            if list.is_empty() {
                next.remove(&s0);
            }
            let mut pts = vec![[s0.0 as f32, s0.1 as f32]];
            let mut guard = 0;
            while cur != s0 && guard < 4_000_000 {
                guard += 1;
                pts.push([cur.0 as f32, cur.1 as f32]);
                let Some(l) = next.get_mut(&cur) else { break };
                let Some(n) = l.pop() else { break };
                if l.is_empty() {
                    next.remove(&cur);
                }
                cur = n;
            }
            if pts.len() >= 4 {
                loops.push(pts);
            }
        }
    }
    // Simplifie (Ramer-Douglas-Peucker) puis arrondit les angles doux en courbes
    fn rdp(pts: &[[f32; 2]], eps: f32, out: &mut Vec<[f32; 2]>) {
        if pts.len() < 3 {
            out.extend_from_slice(&pts[..pts.len().saturating_sub(1)]);
            return;
        }
        let (a, b) = (pts[0], pts[pts.len() - 1]);
        let ab = [b[0] - a[0], b[1] - a[1]];
        let len = (ab[0] * ab[0] + ab[1] * ab[1]).sqrt().max(1e-6);
        let (mut far, mut fi) = (0.0f32, 0);
        for (i, p) in pts.iter().enumerate().skip(1).take(pts.len() - 2) {
            let d = ((p[0] - a[0]) * ab[1] - (p[1] - a[1]) * ab[0]).abs() / len;
            if d > far {
                far = d;
                fi = i;
            }
        }
        if far > eps {
            rdp(&pts[..=fi], eps, out);
            rdp(&pts[fi..], eps, out);
        } else {
            out.push(a);
        }
    }
    let mut out = vec![];
    for lp in loops {
        // Les toutes petites taches (poussière du scan) disparaissent
        let area: f32 = lp.iter().zip(lp.iter().cycle().skip(1)).map(|(a, b)| a[0] * b[1] - b[0] * a[1]).sum::<f32>().abs() / 2.0;
        if area < (detail * detail * 2.0).max(4.0) {
            continue;
        }
        let mut closed = lp.clone();
        closed.push(lp[0]);
        // Boucle fermée : on la coupe au point le plus loin du départ, chaque moitié se simplifie
        let m = (0..closed.len()).max_by(|&a, &b| {
            let d = |k: usize| (closed[k][0] - lp[0][0]).powi(2) + (closed[k][1] - lp[0][1]).powi(2);
            d(a).total_cmp(&d(b))
        }).unwrap_or(0);
        let mut simp = vec![];
        rdp(&closed[..=m], detail.max(0.3), &mut simp);
        rdp(&closed[m..], detail.max(0.3), &mut simp);
        if simp.len() < 3 {
            continue;
        }
        let n = simp.len();
        let nodes: Vec<Node> = (0..n)
            .map(|k| {
                let (a, p, b) = (simp[(k + n - 1) % n], simp[k], simp[(k + 1) % n]);
                let (d1, d2) = ([p[0] - a[0], p[1] - a[1]], [b[0] - p[0], b[1] - p[1]]);
                let (l1, l2) = ((d1[0] * d1[0] + d1[1] * d1[1]).sqrt().max(1e-6), (d2[0] * d2[0] + d2[1] * d2[1]).sqrt().max(1e-6));
                let cos = (d1[0] * d2[0] + d1[1] * d2[1]) / (l1 * l2);
                if cos < 0.3 {
                    // Angle franc : un coin
                    return Node::corner(p);
                }
                let t = [(b[0] - a[0]) / (l1 + l2), (b[1] - a[1]) / (l1 + l2)];
                Node { p, i: [p[0] - t[0] * l1 / 3.0, p[1] - t[1] * l1 / 3.0], o: [p[0] + t[0] * l2 / 3.0, p[1] + t[1] * l2 / 3.0] }
            })
            .collect();
        out.push(nodes);
    }
    (out, color)
}

/// Les contours d'une icône Lucide, en nœuds (repère : coin haut-gauche, `size` unités de côté)
pub fn icon_contours(ch: char, size: f32) -> Vec<Vec<Node>> {
    use ab_glyph::Font;
    let Ok(font) = ab_glyph::FontRef::try_from_slice(include_bytes!("../assets/fonts/Lucide.ttf")) else { return vec![] };
    let gid = font.glyph_id(ch);
    let Some(outline) = font.outline(gid) else { return vec![] };
    let em = font.units_per_em().unwrap_or(1000.0);
    let k = size / em;
    // Y vers le bas ; l'icône occupe le carré de l'em (ascendant en haut)
    let asc = font.ascent_unscaled();
    let tp = |p: ab_glyph::Point| [p.x * k, (asc - p.y) * k];
    let mut out: Vec<Vec<Node>> = vec![];
    let mut cur: Vec<Node> = vec![];
    let mut last: Option<[f32; 2]> = None;
    let close = |cur: &mut Vec<Node>, out: &mut Vec<Vec<Node>>| {
        if cur.len() > 2 {
            // Le dernier point rejoint le premier : un seul nœud
            let (f, l) = (cur[0].p, cur[cur.len() - 1].p);
            if (f[0] - l[0]).abs() < 1e-3 && (f[1] - l[1]).abs() < 1e-3 {
                let end = cur.pop().unwrap();
                cur[0].i = end.i;
            }
            out.push(std::mem::take(cur));
        } else {
            cur.clear();
        }
    };
    for c in &outline.curves {
        let (p0, rest): ([f32; 2], Vec<[f32; 2]>) = match c {
            ab_glyph::OutlineCurve::Line(a, b) => (tp(*a), vec![tp(*b)]),
            ab_glyph::OutlineCurve::Quad(a, b, e) => {
                let (a, b, e) = (tp(*a), tp(*b), tp(*e));
                // Quadratique en cubique
                let c1 = [a[0] + 2.0 / 3.0 * (b[0] - a[0]), a[1] + 2.0 / 3.0 * (b[1] - a[1])];
                let c2 = [e[0] + 2.0 / 3.0 * (b[0] - e[0]), e[1] + 2.0 / 3.0 * (b[1] - e[1])];
                (a, vec![c1, c2, e])
            }
            ab_glyph::OutlineCurve::Cubic(a, b, cc, e) => (tp(*a), vec![tp(*b), tp(*cc), tp(*e)]),
        };
        if last.is_none_or(|l| (l[0] - p0[0]).abs() > 1e-3 || (l[1] - p0[1]).abs() > 1e-3) {
            close(&mut cur, &mut out);
            cur.push(Node::corner(p0));
        }
        if rest.len() == 1 {
            cur.push(Node::corner(rest[0]));
            last = Some(rest[0]);
        } else {
            if let Some(n) = cur.last_mut() {
                n.o = rest[0];
            }
            cur.push(Node { p: rest[2], i: rest[1], o: rest[2] });
            last = Some(rest[2]);
        }
    }
    close(&mut cur, &mut out);
    out
}

/// Des points (un geste, un tracé aplati) deviennent peu de nœuds lisses : on garde les points
/// qui s'écartent de plus de `eps`, et chaque angle doux reçoit des poignées
pub fn fit_nodes(pts: &[[f32; 2]], eps: f32, closed: bool) -> Vec<Node> {
    fn rdp(pts: &[[f32; 2]], eps: f32, keep: &mut Vec<bool>, a: usize, b: usize) {
        if b <= a + 1 {
            return;
        }
        let (pa, pb) = (pts[a], pts[b]);
        let ab = [pb[0] - pa[0], pb[1] - pa[1]];
        let len = (ab[0] * ab[0] + ab[1] * ab[1]).sqrt();
        let (mut far, mut fi) = (0.0f32, a);
        for (i, p) in pts.iter().enumerate().take(b).skip(a + 1) {
            let d = if len < 1e-6 { ((p[0] - pa[0]).powi(2) + (p[1] - pa[1]).powi(2)).sqrt() } else { ((p[0] - pa[0]) * ab[1] - (p[1] - pa[1]) * ab[0]).abs() / len };
            if d > far {
                far = d;
                fi = i;
            }
        }
        if far > eps {
            keep[fi] = true;
            rdp(pts, eps, keep, a, fi);
            rdp(pts, eps, keep, fi, b);
        }
    }
    if pts.len() < 2 {
        return pts.iter().map(|p| Node::corner(*p)).collect();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    rdp(pts, eps, &mut keep, 0, pts.len() - 1);
    let mut simp: Vec<[f32; 2]> = pts.iter().zip(&keep).filter(|x| *x.1).map(|x| *x.0).collect();
    if closed && simp.len() > 2 && (simp[0][0] - simp[simp.len() - 1][0]).hypot(simp[0][1] - simp[simp.len() - 1][1]) < eps {
        simp.pop();
    }
    let n = simp.len();
    (0..n)
        .map(|k| {
            let p = simp[k];
            let (a, b) = if closed { (simp[(k + n - 1) % n], simp[(k + 1) % n]) } else { (simp[k.saturating_sub(1)], simp[(k + 1).min(n - 1)]) };
            if !closed && (k == 0 || k + 1 == n) {
                // Les bouts : une poignée vers le voisin seulement
                let q = if k == 0 { b } else { a };
                let h = [(q[0] - p[0]) / 3.0, (q[1] - p[1]) / 3.0];
                return if k == 0 { Node { p, i: p, o: [p[0] + h[0], p[1] + h[1]] } } else { Node { p, i: [p[0] + h[0], p[1] + h[1]], o: p } };
            }
            let (d1, d2) = ([p[0] - a[0], p[1] - a[1]], [b[0] - p[0], b[1] - p[1]]);
            let (l1, l2) = ((d1[0] * d1[0] + d1[1] * d1[1]).sqrt().max(1e-6), (d2[0] * d2[0] + d2[1] * d2[1]).sqrt().max(1e-6));
            if (d1[0] * d2[0] + d1[1] * d2[1]) / (l1 * l2) < 0.2 {
                return Node::corner(p);
            }
            let t = [(b[0] - a[0]) / (l1 + l2), (b[1] - a[1]) / (l1 + l2)];
            Node { p, i: [p[0] - t[0] * l1 / 3.0, p[1] - t[1] * l1 / 3.0], o: [p[0] + t[0] * l2 / 3.0, p[1] + t[1] * l2 / 3.0] }
        })
        .collect()
}

impl Doc {
    /// Les formes concernées (et le contenu des groupes) ; vide : tout le document
    fn deep(&self, ids: &[u64]) -> Vec<u64> {
        if ids.is_empty() {
            return self.shapes.iter().map(|s| s.id).collect();
        }
        let mut v: Vec<u64> = ids.to_vec();
        for i in ids {
            v.extend(self.descendants(*i));
        }
        v
    }

    /// Les couleurs utilisées (fonds, contours, arrêts de dégradé), de la plus fréquente à la plus rare
    pub fn colors_of(&self, ids: &[u64]) -> Vec<[u8; 4]> {
        let set = self.deep(ids);
        let mut count: Vec<([u8; 4], usize)> = vec![];
        let mut add = |c: [u8; 4]| match count.iter_mut().find(|x| x.0 == c) {
            Some(x) => x.1 += 1,
            None => count.push((c, 1)),
        };
        let mut paint = |p: &Paint| match p {
            Paint::Solid { c } => add(*c),
            Paint::Linear { stops, .. } | Paint::Radial { stops } => stops.iter().for_each(|s| add(s.1)),
        };
        for s in self.shapes.iter().filter(|s| set.contains(&s.id)) {
            s.fills.iter().for_each(&mut paint);
            s.strokes.iter().for_each(|k| paint(&k.paint));
        }
        count.sort_by(|a, b| b.1.cmp(&a.1));
        count.into_iter().map(|x| x.0).collect()
    }

    /// Remplace une couleur par une autre (même opacité gardée si `keep_alpha`) ; rend le nombre de remplacements
    pub fn replace_color(&mut self, ids: &[u64], from: [u8; 4], to: [u8; 4]) -> usize {
        let set = self.deep(ids);
        let mut n = 0;
        let mut fix = |c: &mut [u8; 4]| {
            if *c == from {
                *c = to;
                n += 1;
            }
        };
        for s in self.shapes.iter_mut().filter(|s| set.contains(&s.id)) {
            for p in s.fills.iter_mut().chain(s.strokes.iter_mut().map(|k| &mut k.paint)) {
                match p {
                    Paint::Solid { c } => fix(c),
                    Paint::Linear { stops, .. } | Paint::Radial { stops } => stops.iter_mut().for_each(|st| fix(&mut st.1)),
                }
            }
        }
        n
    }

    /// Les nœuds d'un tracé dans le repère du monde (position et rotation appliquées)
    pub fn path_world(&self, id: u64) -> Option<(Vec<Node>, bool)> {
        let s = self.get(id)?;
        let Kind::Path { nodes, closed } = &s.kind else { return None };
        let (sn, cs) = s.rot.to_radians().sin_cos();
        let t = |p: [f32; 2]| [s.x + p[0] * cs - p[1] * sn, s.y + p[0] * sn + p[1] * cs];
        Some((nodes.iter().map(|n| Node { p: t(n.p), i: t(n.i), o: t(n.o) }).collect(), *closed))
    }

    /// Moins de points pour un tracé, même allure (tolérance en unités)
    pub fn simplify_path(&mut self, id: u64, eps: f32) -> Option<usize> {
        let s = self.get(id)?.clone();
        let Kind::Path { nodes, closed } = &s.kind else { return None };
        let pts: Vec<[f32; 2]> = crate::vgeom::flatten(nodes, *closed).into_iter().map(|x| x.0).collect();
        let fitted = fit_nodes(&pts, eps, *closed);
        let n = fitted.len();
        if let Some(sh) = self.shapes.iter_mut().find(|x| x.id == id) {
            sh.kind = Kind::Path { nodes: fitted, closed: *closed };
        }
        Some(n)
    }

    /// Coupe un tracé au nœud `k` : un tracé fermé s'ouvre là, un tracé ouvert devient deux
    pub fn cut_path(&mut self, id: u64, k: usize) -> Option<u64> {
        let s = self.get(id)?.clone();
        let Kind::Path { nodes, closed } = &s.kind else { return None };
        if k >= nodes.len() {
            return None;
        }
        if *closed {
            let mut v: Vec<Node> = nodes[k..].iter().chain(nodes[..k].iter()).copied().collect();
            v.push(nodes[k]);
            if let Some(sh) = self.shapes.iter_mut().find(|x| x.id == id) {
                sh.kind = Kind::Path { nodes: v, closed: false };
            }
            return Some(id);
        }
        if k == 0 || k + 1 >= nodes.len() {
            return None;
        }
        let (a, b) = (nodes[..=k].to_vec(), nodes[k..].to_vec());
        if let Some(sh) = self.shapes.iter_mut().find(|x| x.id == id) {
            sh.kind = Kind::Path { nodes: a, closed: false };
        }
        let nid = self.new_id();
        let mut c = s.clone();
        c.id = nid;
        c.name = format!("{} 2", s.name);
        c.kind = Kind::Path { nodes: b, closed: false };
        let at = self.index(id).map(|i| i + 1).unwrap_or(self.shapes.len());
        self.shapes.insert(at, c);
        if s.parent != 0 {
            if let Some(Shape { kind: Kind::Group { children, .. }, .. }) = self.shapes.iter_mut().find(|x| x.id == s.parent) {
                children.push(nid);
            }
        }
        Some(nid)
    }

    /// Joint deux tracés ouverts par leurs bouts les plus proches (un seul : il se ferme) ; rend le tracé obtenu
    pub fn join_paths(&mut self, a: u64, b: Option<u64>) -> Option<u64> {
        let Some(b) = b else {
            if let Some(Shape { kind: Kind::Path { closed, .. }, .. }) = self.shapes.iter_mut().find(|x| x.id == a) {
                *closed = true;
            }
            return Some(a);
        };
        let (mut na, ca) = self.path_world(a)?;
        let (mut nb, cb) = self.path_world(b)?;
        if ca || cb || na.is_empty() || nb.is_empty() {
            return None;
        }
        let rev = |v: &mut Vec<Node>| {
            v.reverse();
            for n in v.iter_mut() {
                std::mem::swap(&mut n.i, &mut n.o);
            }
        };
        let d = |p: [f32; 2], q: [f32; 2]| (p[0] - q[0]).hypot(p[1] - q[1]);
        // Les quatre façons de mettre bout à bout : la plus courte
        let opts = [d(na[na.len() - 1].p, nb[0].p), d(na[na.len() - 1].p, nb[nb.len() - 1].p), d(na[0].p, nb[0].p), d(na[0].p, nb[nb.len() - 1].p)];
        let best = (0..4).min_by(|x, y| opts[*x].total_cmp(&opts[*y])).unwrap_or(0);
        match best {
            1 => rev(&mut nb),
            2 => rev(&mut na),
            3 => {
                rev(&mut na);
                rev(&mut nb);
            }
            _ => {}
        }
        // Bouts confondus : un seul nœud
        if d(na[na.len() - 1].p, nb[0].p) < 0.5 {
            let last = na.pop().unwrap();
            nb[0].i = last.i;
        }
        na.extend(nb);
        let s = self.get(a)?.clone();
        if let Some(sh) = self.shapes.iter_mut().find(|x| x.id == a) {
            (sh.x, sh.y, sh.rot) = (0.0, 0.0, 0.0);
            sh.kind = Kind::Path { nodes: na, closed: false };
        }
        let _ = s;
        self.remove(&[b]);
        Some(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_rendues() {
        let mut png = vec![];
        image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        use base64::Engine;
        let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="20" height="20" viewBox="0 0 40 40"><g transform="translate(0 0)"><rect x="4" y="4" width="36" height="36" fill="white"/></g><image x="10" y="10" width="20" height="20" preserveAspectRatio="none" xlink:href="data:image/png;base64,{}"/><rect x="24" y="24" width="8" height="8" fill="blue"/></svg>"#, base64::engine::general_purpose::STANDARD.encode(&png));
        let px = rasterize(&svg, 20, 20).unwrap();
        let at = |x: usize, y: usize| px[(y * 20 + x) * 4..(y * 20 + x) * 4 + 4].to_vec();
        assert_eq!(at(7, 7), vec![255, 0, 0, 255]);
        assert_eq!(at(13, 13), vec![0, 0, 255, 255]);
        assert_eq!(at(3, 3), vec![255, 255, 255, 255]);
        assert_eq!(at(0, 0)[3], 0);
    }

    #[test]
    fn mise_en_page_auto() {
        let mut d = Doc::starter();
        let mut ids = vec![];
        for (k, w) in [100.0f32, 60.0].iter().enumerate() {
            let id = d.new_id();
            d.shapes.push(Shape { id, kind: Kind::Rect { w: *w, h: 40.0 + k as f32 * 20.0, r: [0.0; 4] }, x: 500.0 * k as f32, y: 300.0, ..Shape::base() });
            ids.push(id);
        }
        let g = d.group(&ids, 0).unwrap();
        if let Some(s) = d.shapes.iter_mut().find(|s| s.id == g) {
            (s.x, s.y) = (0.0, 0.0);
            s.layout = Some(AutoLayout { gap: 10.0, pad: [20.0, 5.0], align: 1, ..Default::default() });
        }
        d.apply_layouts();
        let b0 = d.bounds(d.get(ids[0]).unwrap());
        let b1 = d.bounds(d.get(ids[1]).unwrap());
        assert_eq!([b0[0], b0[1]], [20.0, 15.0]);
        assert_eq!([b1[0], b1[1]], [130.0, 5.0]);
        assert_eq!(d.get(g).unwrap().layout.as_ref().unwrap().size, [210.0, 70.0]);
        // Le premier grandit : le second suit
        if let Some(Shape { kind: Kind::Rect { w, .. }, .. }) = d.shapes.iter_mut().find(|s| s.id == ids[0]) {
            *w = 150.0;
        }
        d.apply_layouts();
        assert_eq!(d.bounds(d.get(ids[1]).unwrap())[0], 180.0);
    }

    #[test]
    fn icone_lucide() {
        let ch = crate::lucide_all::ALL.iter().find(|x| x.0 == "circle").unwrap().1;
        let c = icon_contours(ch, 96.0);
        // Un cercle au trait : deux contours (dehors, dedans)
        assert_eq!(c.len(), 2, "{}", c.len());
        let xs: Vec<f32> = c[0].iter().map(|n| n.p[0]).collect();
        assert!(xs.iter().cloned().fold(f32::MIN, f32::max) <= 96.5 && xs.iter().cloned().fold(f32::MAX, f32::min) >= -0.5);
    }

    #[test]
    fn contraintes_et_grille() {
        // Plan de 0..100 qui passe à 0..200 : un objet en 80..90
        assert_eq!(constrain(0, 80.0, 90.0, 0.0, 100.0, 0.0, 200.0), (80.0, 90.0));
        assert_eq!(constrain(1, 80.0, 90.0, 0.0, 100.0, 0.0, 200.0), (180.0, 190.0));
        assert_eq!(constrain(2, 80.0, 90.0, 0.0, 100.0, 0.0, 200.0), (80.0, 190.0));
        assert_eq!(constrain(3, 45.0, 55.0, 0.0, 100.0, 0.0, 200.0), (95.0, 105.0));
        assert_eq!(constrain(4, 80.0, 90.0, 0.0, 100.0, 0.0, 200.0), (160.0, 180.0));
        let b = Board { name: String::new(), x: 0.0, y: 0.0, w: 1000.0, h: 100.0, bg: None, grid: Some(LayoutGrid { cols: 4, gutter: 20.0, margin: 40.0 }) };
        let c = b.columns();
        assert_eq!(c.len(), 4);
        assert_eq!(c[0], (40.0, 255.0));
        assert!((c[3].1 - 960.0).abs() < 1e-3);
    }

    #[test]
    fn styles_partages() {
        let mut d = Doc::starter();
        let mut ids = vec![];
        for _ in 0..2 {
            let id = d.new_id();
            d.shapes.push(Shape { id, fills: vec![Paint::solid([1, 1, 1, 255])], ..Shape::base() });
            ids.push(id);
        }
        let st = NamedStyle::of("Accent", d.get(ids[0]).unwrap());
        for id in &ids {
            let s = d.shapes.iter_mut().find(|s| s.id == *id).unwrap();
            st.apply(s);
        }
        let mut st2 = st.clone();
        st2.fills = vec![Paint::solid([200, 50, 50, 255])];
        assert_eq!(d.set_style(st2), 2);
        assert!(ids.iter().all(|i| d.get(*i).unwrap().fills[0].first() == [200, 50, 50, 255]));
    }

    #[test]
    fn recolorer() {
        let mut d = Doc::starter();
        for c in [[1, 2, 3, 255], [1, 2, 3, 255], [9, 9, 9, 255]] {
            let id = d.new_id();
            d.shapes.push(Shape { id, fills: vec![Paint::solid(c)], ..Shape::base() });
        }
        assert_eq!(d.colors_of(&[]), vec![[1, 2, 3, 255], [9, 9, 9, 255]]);
        assert_eq!(d.replace_color(&[], [1, 2, 3, 255], [200, 0, 0, 255]), 2);
        assert_eq!(d.colors_of(&[])[0], [200, 0, 0, 255]);
    }

    #[test]
    fn outils_de_trace() {
        // Un geste en arc : peu de nœuds, lisses
        let pts: Vec<[f32; 2]> = (0..=60).map(|k| { let a = k as f32 / 60.0 * std::f32::consts::PI; [100.0 * a.cos(), 100.0 * a.sin()] }).collect();
        let n = fit_nodes(&pts, 1.0, false);
        assert!(n.len() < 20 && n.len() >= 4, "{}", n.len());
        let mut d = Doc::starter();
        let a = d.new_id();
        d.shapes.push(Shape { id: a, kind: Kind::Path { nodes: vec![Node::corner([0.0, 0.0]), Node::corner([10.0, 0.0])], closed: false }, ..Shape::base() });
        let b = d.new_id();
        d.shapes.push(Shape { id: b, kind: Kind::Path { nodes: vec![Node::corner([30.0, 0.0]), Node::corner([10.0, 0.0])], closed: false }, x: 0.0, ..Shape::base() });
        let j = d.join_paths(a, Some(b)).unwrap();
        let Kind::Path { nodes, .. } = &d.get(j).unwrap().kind else { panic!() };
        assert_eq!(nodes.iter().map(|n| n.p[0]).collect::<Vec<_>>(), vec![0.0, 10.0, 30.0]);
        assert!(d.get(b).is_none());
        let c = d.cut_path(j, 1).unwrap();
        assert_ne!(c, j);
        assert!(matches!(&d.get(c).unwrap().kind, Kind::Path { nodes, .. } if nodes.len() == 2));
    }

    #[test]
    fn composants() {
        let mut d = Doc::starter();
        let a = d.new_id();
        d.shapes.push(Shape { id: a, kind: Kind::Rect { w: 80.0, h: 30.0, r: [6.0; 4] }, x: 10.0, y: 10.0, fills: vec![Paint::solid([255, 0, 0, 255])], ..Shape::base() });
        let t = d.new_id();
        d.shapes.push(Shape { id: t, kind: Kind::Text { text: "OK".into(), size: 14.0, font: "inter".into(), align: 0, width: None, spacing: 0.0, leading: 1.0 }, x: 20.0, y: 15.0, ..Shape::base() });
        let m = d.group(&[a, t], 0).unwrap();
        d.shapes.iter_mut().find(|s| s.id == m).unwrap().component = true;
        let i = d.instantiate(m).unwrap();
        let kids = d.preorder(i);
        assert_eq!(kids.len(), 2);
        let ib = d.bounds(d.get(i).unwrap());
        assert!((ib[0] - 130.0).abs() < 0.5, "{ib:?}");
        // Remplacer le texte de l'instance, puis changer la couleur du maître
        if let Some(Shape { kind: Kind::Text { text, .. }, .. }) = d.shapes.iter_mut().find(|s| s.id == kids[1]) {
            *text = "Annuler".into();
        }
        d.shapes.iter_mut().find(|s| s.id == a).unwrap().fills = vec![Paint::solid([0, 0, 255, 255])];
        d.sync_instances();
        assert_eq!(d.preorder(i), kids, "mêmes identifiants");
        assert_eq!(d.get(kids[0]).unwrap().fills[0].first(), [0, 0, 255, 255]);
        assert!(matches!(&d.get(kids[1]).unwrap().kind, Kind::Text { text, .. } if text == "Annuler"));
        // Une forme de plus dans le maître : l'instance est refaite
        let e = d.new_id();
        d.shapes.push(Shape { id: e, kind: Kind::Ellipse { w: 8.0, h: 8.0 }, x: 2.0, y: 2.0, parent: m, ..Shape::base() });
        if let Some(Shape { kind: Kind::Group { children, .. }, .. }) = d.shapes.iter_mut().find(|s| s.id == m) {
            children.push(e);
        }
        d.sync_instances();
        assert_eq!(d.preorder(i).len(), 3);
    }

    #[test]
    fn vectoriser() {
        // Un disque noir avec un trou : un contour extérieur et un intérieur
        let mut img = image::RgbaImage::from_pixel(100, 100, image::Rgba([255, 255, 255, 255]));
        for y in 0..100 {
            for x in 0..100 {
                let d = ((x as f32 - 50.0).powi(2) + (y as f32 - 50.0).powi(2)).sqrt();
                if d < 40.0 && d > 15.0 {
                    img.put_pixel(x, y, image::Rgba([10, 10, 10, 255]));
                }
            }
        }
        let (loops, col) = trace(&img, 0.5, 1.0);
        assert_eq!(loops.len(), 2);
        assert!(col[0] < 20);
        assert!(loops.iter().all(|l| l.len() > 8 && l.len() < 120));
    }

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
