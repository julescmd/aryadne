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
    /// Grille de perspective (deux points de fuite), affichée sur le plan, jamais exportée
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persp: Option<Persp>,
    /// Cadre d'appareil autour du plan (0 aucun, 1 iPhone, 2 Android, 3 écran), avec barre d'état
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub device: u8,
    /// Mode des variables affiché dans ce plan (0 : le premier, souvent « clair »)
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub mode: u8,
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
    /// Motif répété : genre (voir `MOTIFS`), couleur du motif et du fond, taille d'une case (unités), angle
    Pattern { motif: u8, fg: [u8; 4], bg: [u8; 4], size: f32, angle: f32 },
    /// Dégradé à points libres : des couleurs posées n'importe où (0..1 dans le cadre de la forme), fondues entre elles
    Freeform { pts: Vec<([f32; 2], [u8; 4])> },
    /// Filet de dégradé : une grille de `cols` × `rows` cases dont chaque nœud (rangée par rangée, (cols+1) × (rows+1))
    /// porte une position (0..1 dans le cadre de la forme) et une couleur ; la couleur coule en courbe d'un nœud à l'autre
    Mesh { cols: u8, rows: u8, pts: Vec<([f32; 2], [u8; 4])> },
}

/// Un filet de dégradé régulier : la couleur `a` en haut à gauche qui glisse vers `b` en bas à droite
pub fn mesh_new(cols: u8, rows: u8, a: [u8; 4], b: [u8; 4]) -> Paint {
    let (cols, rows) = (cols.clamp(1, 8), rows.clamp(1, 8));
    let mut pts = vec![];
    for j in 0..=rows {
        for i in 0..=cols {
            let (u, v) = (i as f32 / cols as f32, j as f32 / rows as f32);
            let t = (u + v) / 2.0;
            pts.push(([u, v], [0, 1, 2, 3].map(|k| (a[k] as f32 + (b[k] as f32 - a[k] as f32) * t).round() as u8)));
        }
    }
    Paint::Mesh { cols, rows, pts }
}

/// Le filet est-il entier (autant de nœuds que la grille en demande) ?
pub fn mesh_ok(cols: u8, rows: u8, pts: &[([f32; 2], [u8; 4])]) -> bool {
    cols >= 1 && rows >= 1 && pts.len() == (cols as usize + 1) * (rows as usize + 1)
}

/// Le point du filet en (s, t), s de 0 à cols et t de 0 à rows : position (0..1) et couleur.
/// Courbe de Catmull-Rom dans les deux sens ; au bord, le voisin manquant est le reflet du dernier.
pub fn mesh_at(cols: u8, rows: u8, pts: &[([f32; 2], [u8; 4])], s: f32, t: f32) -> ([f32; 2], [f32; 4]) {
    if !mesh_ok(cols, rows, pts) {
        return ([0.0; 2], [0.0; 4]);
    }
    let (nc, nr) = (cols as i32, rows as i32);
    let (s, t) = (s.clamp(0.0, nc as f32), t.clamp(0.0, nr as f32));
    let (i, j) = ((s.floor() as i32).min(nc - 1), (t.floor() as i32).min(nr - 1));
    let (fs, ft) = (s - i as f32, t - j as f32);
    let w = |f: f32| {
        let (f2, f3) = (f * f, f * f * f);
        [(-f3 + 2.0 * f2 - f) * 0.5, (3.0 * f3 - 5.0 * f2 + 2.0) * 0.5, (-3.0 * f3 + 4.0 * f2 + f) * 0.5, (f3 - f2) * 0.5]
    };
    let raw = |i: i32, j: i32| -> [f32; 6] {
        let (p, c) = pts[(j * (nc + 1) + i) as usize];
        // Un nœud parti à l'infini (fichier abîmé) est ramené près de la forme
        let fix = |v: f32| if v.is_finite() { v.clamp(-2.0, 3.0) } else { 0.5 };
        [fix(p[0]), fix(p[1]), c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32]
    };
    // Un nœud hors de la grille : le reflet à travers le bord (une grille régulière reste régulière)
    let get = |i: i32, j: i32| -> [f32; 6] {
        let (ci, cj) = (i.clamp(0, nc), j.clamp(0, nr));
        if ci == i && cj == j {
            return raw(i, j);
        }
        let (mi, mj) = ((2 * ci - i).clamp(0, nc), (2 * cj - j).clamp(0, nr));
        let (e, m) = (raw(ci, cj), raw(mi, mj));
        std::array::from_fn(|k| 2.0 * e[k] - m[k])
    };
    let (ws, wt) = (w(fs), w(ft));
    let mut acc = [0.0f32; 6];
    for (b, wb) in wt.iter().enumerate() {
        for (a, wa) in ws.iter().enumerate() {
            let v = get(i - 1 + a as i32, j - 1 + b as i32);
            for k in 0..6 {
                acc[k] += v[k] * wa * wb;
            }
        }
    }
    ([acc[0], acc[1]], [acc[2].clamp(0.0, 255.0), acc[3].clamp(0.0, 255.0), acc[4].clamp(0.0, 255.0), acc[5].clamp(0.0, 255.0)])
}

/// Le filet découpé en petits carreaux d'une seule couleur (coins en 0..1, couleur du milieu) : `n` carreaux par case
pub fn mesh_quads(cols: u8, rows: u8, pts: &[([f32; 2], [u8; 4])], n: usize) -> Vec<([[f32; 2]; 4], [u8; 4])> {
    if !mesh_ok(cols, rows, pts) {
        return vec![];
    }
    let n = n.max(1);
    let (w, h) = (cols as usize * n, rows as usize * n);
    let at = |x: f32, y: f32| mesh_at(cols, rows, pts, x / n as f32, y / n as f32);
    let grid: Vec<[f32; 2]> = (0..=h).flat_map(|y| (0..=w).map(move |x| (x, y))).map(|(x, y)| at(x as f32, y as f32).0).collect();
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let c = at(x as f32 + 0.5, y as f32 + 0.5).1.map(|v| v.round() as u8);
            let k = y * (w + 1) + x;
            out.push(([grid[k], grid[k + 1], grid[k + w + 2], grid[k + w + 1]], c));
        }
    }
    out
}

/// Le même filet avec une autre grille : les nœuds sont relus sur les courbes de l'ancien
pub fn mesh_resize(cols: u8, rows: u8, pts: &[([f32; 2], [u8; 4])], ncols: u8, nrows: u8) -> Paint {
    let (ncols, nrows) = (ncols.clamp(1, 8), nrows.clamp(1, 8));
    if !mesh_ok(cols, rows, pts) {
        return mesh_new(ncols, nrows, pts.first().map(|p| p.1).unwrap_or([0, 0, 0, 255]), [255, 255, 255, 255]);
    }
    let mut out = vec![];
    for j in 0..=nrows {
        for i in 0..=ncols {
            let (p, c) = mesh_at(cols, rows, pts, i as f32 / ncols as f32 * cols as f32, j as f32 / nrows as f32 * rows as f32);
            out.push((p, c.map(|v| v.round() as u8)));
        }
    }
    Paint::Mesh { cols: ncols, rows: nrows, pts: out }
}

/// La couleur d'un dégradé à points libres au point (u, v) : moyenne pondérée par l'inverse du carré de la distance
pub fn freeform_at(pts: &[([f32; 2], [u8; 4])], u: f32, v: f32, aspect: f32) -> [u8; 4] {
    let mut acc = [0.0f32; 4];
    let mut sw = 0.0;
    for (p, c) in pts {
        let d2 = ((u - p[0]) * aspect).powi(2) + (v - p[1]).powi(2);
        if d2 < 1e-7 {
            return *c;
        }
        let w = 1.0 / (d2 * d2.sqrt());
        for i in 0..4 {
            acc[i] += c[i] as f32 * w;
        }
        sw += w;
    }
    if sw == 0.0 {
        return [0, 0, 0, 0];
    }
    acc.map(|x| (x / sw).round().clamp(0.0, 255.0) as u8)
}

/// Les motifs proposés (dans l'ordre de `Paint::Pattern::motif`)
pub const MOTIFS: [&str; 6] = ["pois", "rayures", "quadrillage", "damier", "vagues", "hachures croisées"];

/// Le dessin d'une case de motif (SVG, case de `s` de côté)
pub fn motif_svg(motif: u8, fg: &str, fo: f32, s: f32) -> String {
    let lw = (s * 0.12).max(0.5);
    match motif {
        0 => format!(r#"<circle cx="{h}" cy="{h}" r="{r}" fill="{fg}" fill-opacity="{fo}"/>"#, h = s / 2.0, r = s * 0.22),
        1 => format!(r#"<rect x="0" y="0" width="{w}" height="{s}" fill="{fg}" fill-opacity="{fo}"/>"#, w = s / 2.0),
        2 => format!(r#"<path d="M0 0H{s}M0 0V{s}" stroke="{fg}" stroke-opacity="{fo}" stroke-width="{lw}" fill="none"/>"#),
        3 => format!(r#"<rect width="{h}" height="{h}" fill="{fg}" fill-opacity="{fo}"/><rect x="{h}" y="{h}" width="{h}" height="{h}" fill="{fg}" fill-opacity="{fo}"/>"#, h = s / 2.0),
        4 => format!(r#"<path d="M0 {h} Q{q} {a} {h} {h} T{s} {h}" stroke="{fg}" stroke-opacity="{fo}" stroke-width="{lw}" fill="none"/>"#, h = s / 2.0, q = s / 4.0, a = s * 0.2),
        _ => format!(r#"<path d="M0 0L{s} {s}M{s} 0L0 {s}" stroke="{fg}" stroke-opacity="{fo}" stroke-width="{lw}" fill="none"/>"#),
    }
}

impl Paint {
    pub fn solid(c: [u8; 4]) -> Paint {
        Paint::Solid { c }
    }
    pub fn first(&self) -> [u8; 4] {
        match self {
            Paint::Solid { c } => *c,
            Paint::Linear { stops, .. } | Paint::Radial { stops } => stops.first().map(|s| s.1).unwrap_or([0, 0, 0, 255]),
            Paint::Pattern { fg, .. } => *fg,
            Paint::Freeform { pts } | Paint::Mesh { pts, .. } => pts.first().map(|p| p.1).unwrap_or([0, 0, 0, 255]),
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
    /// Forme de pinceau (voir BRUSHES) : 0 uniforme, 1 calligraphique, 2 effilé, 3 dispersion
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub brush: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Shadow {
    pub dx: f32,
    pub dy: f32,
    pub blur: f32,
    pub c: [u8; 4],
    /// Ombre interne (dans la forme, depuis ses bords) plutôt que portée
    #[serde(default)]
    pub inner: bool,
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
    /// Prototype : un clic sur la forme (en présentation) mène à ce plan de travail (son nom ; vide : aucun lien)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link: String,
    /// Texte sur un tracé : l'identifiant du tracé (ou de l'ellipse) que suit la ligne de base (0 : aucun)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub on_path: u64,
    /// Variable de couleur du remplissage (comme Figma) : sa valeur suit le mode du plan
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fill_var: String,
    /// État au survol, en présentation (composant interactif) : autre remplissage, opacité, ombre
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hover: Option<Hover>,
    /// Composant venu d'une bibliothèque partagée : « fichier du coffre#nom du composant »
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lib: String,
    /// Zone de peinture dynamique : son point de départ, dans le repère de la forme. Quand les traits autour
    /// changent, la zone est refaite depuis ce point (la peinture reste éditable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<[f32; 2]>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Hover {
    /// Remplissage au survol (aucun : inchangé)
    pub fill: Option<[u8; 4]>,
    pub opacity: f32,
    /// Ombre portée au survol (la forme se soulève)
    #[serde(default)]
    pub lift: bool,
}

impl Default for Hover {
    fn default() -> Self {
        Hover { fill: None, opacity: 1.0, lift: true }
    }
}

impl Doc {
    /// Le document tel qu'on le voit quand la souris survole la forme `id` (et ce qu'elle contient)
    pub fn hovered(&self, id: u64) -> Doc {
        let mut d = self.clone();
        let Some(h) = self.get(id).and_then(|s| s.hover.clone()) else { return d };
        if let Some(s) = d.shapes.iter_mut().find(|s| s.id == id) {
            if let Some(c) = h.fill {
                match s.fills.first_mut() {
                    Some(f) => *f = Paint::Solid { c },
                    None => s.fills.push(Paint::Solid { c }),
                }
                // Un groupe (bouton) : son fond, la première forme, prend la couleur
                if let Kind::Group { children, .. } = &s.kind {
                    if let Some(&first) = children.first() {
                        if let Some(bg) = d.shapes.iter_mut().find(|x| x.id == first) {
                            match bg.fills.first_mut() {
                                Some(f) => *f = Paint::Solid { c },
                                None => bg.fills.push(Paint::Solid { c }),
                            }
                        }
                    }
                }
            }
        }
        if let Some(s) = d.shapes.iter_mut().find(|s| s.id == id) {
            s.opacity *= h.opacity;
            if h.lift && s.shadow.is_none() {
                s.shadow = Some(Shadow { dx: 0.0, dy: 6.0, blur: 18.0, c: [0, 0, 0, 70], inner: false });
            }
        }
        d
    }
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

fn is_zero_u8(v: &u8) -> bool {
    *v == 0
}

/// Les formes de pinceau d'un contour
pub const BRUSHES: [&str; 4] = ["uniforme", "calligraphique", "effilé", "dispersion"];

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
    /// Commentaires épinglés sur la toile (jamais exportés)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<Comment>,
    /// Variables de couleur (comme Figma) : une valeur par mode
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vars: Vec<Variable>,
    /// Les modes des variables (« clair », « sombre »…) ; vide : un seul mode
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modes: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Variable {
    pub name: String,
    /// Une couleur par mode, dans l'ordre de `Doc::modes`
    pub values: Vec<[u8; 4]>,
}

impl Doc {
    /// Les noms des modes (au moins un)
    pub fn mode_names(&self) -> Vec<String> {
        if self.modes.is_empty() { vec![crate::i18n::t_mid("clair")] } else { self.modes.clone() }
    }

    /// La valeur d'une variable dans un mode (le premier si ce mode n'a pas de valeur)
    pub fn var_value(&self, name: &str, mode: usize) -> Option<[u8; 4]> {
        let v = self.vars.iter().find(|v| v.name == name)?;
        v.values.get(mode).or(v.values.first()).copied()
    }

    /// Repose les valeurs des variables sur les formes qui s'en servent, selon le mode du plan où elles sont
    pub fn apply_vars(&mut self) -> bool {
        if self.vars.is_empty() {
            return false;
        }
        let mut changed = false;
        for i in 0..self.shapes.len() {
            if self.shapes[i].fill_var.is_empty() {
                continue;
            }
            let b = self.bounds(&self.shapes[i]);
            let (cx, cy) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
            let mode = self.boards.iter().find(|bd| cx >= bd.x && cy >= bd.y && cx <= bd.x + bd.w && cy <= bd.y + bd.h).map(|bd| bd.mode as usize).unwrap_or(0);
            let Some(c) = self.var_value(&self.shapes[i].fill_var.clone(), mode) else { continue };
            let s = &mut self.shapes[i];
            match s.fills.first_mut() {
                Some(Paint::Solid { c: old }) if *old == c => {}
                Some(f) => {
                    *f = Paint::Solid { c };
                    changed = true;
                }
                None => {
                    s.fills.push(Paint::Solid { c });
                    changed = true;
                }
            }
        }
        changed
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Comment {
    pub x: f32,
    pub y: f32,
    pub text: String,
    #[serde(default)]
    pub done: bool,
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
            boards: vec![Board { name: format!("{} 1", crate::i18n::t("Plan de travail")), x: 0.0, y: 0.0, w: 1080.0, h: 1080.0, bg: Some([255, 255, 255, 255]), grid: None, persp: None, device: 0, mode: 0 }],
            shapes: vec![],
            next_id: 1,
            swatches: vec![[24, 24, 27, 255], [255, 255, 255, 255], [236, 108, 89, 255], [244, 191, 80, 255], [86, 166, 122, 255], [84, 128, 220, 255], [150, 110, 210, 255]],
            styles: vec![],
            comments: vec![],
            vars: vec![],
            modes: vec![],
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
        // Un texte sur un tracé occupe le tracé (élargi de la hauteur des lettres)
        if let (Kind::Text { size, .. }, true) = (&s.kind, s.on_path != 0) {
            if let Some(g) = self.get(s.on_path).and_then(guide_line) {
                let b = g.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, (p, _)| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])]);
                return [b[0] - size, b[1] - size, b[2] + size, b[3] + size];
            }
        }
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
            6 => "Masque",
            7 => "Masque d'opacité",
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
        let g = Shape { id: gid, name: crate::i18n::t(name), kind: Kind::Group { children, op }, x: b[0], y: b[1], parent, ..Shape::base() };
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
        Shape { id: 0, name: String::new(), kind: Kind::Rect { w: 100.0, h: 100.0, r: [0.0; 4] }, x: 0.0, y: 0.0, rot: 0.0, fills: vec![], strokes: vec![], opacity: 1.0, visible: true, locked: false, parent: 0, shadow: None, blur: 0.0, blend: String::new(), layout: None, component: false, instance_of: 0, widths: vec![], constraint: [0, 0], style: String::new(), link: String::new(), on_path: 0, fill_var: String::new(), hover: None, lib: String::new(), seed: None }
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

/// Rapport de contraste (WCAG) entre deux couleurs : de 1 à 21 ; 4,5 suffit pour un texte courant (AA)
pub fn contrast(a: [u8; 4], b: [u8; 4]) -> f32 {
    let lum = |c: [u8; 4]| {
        let ch = |v: u8| {
            let x = v as f32 / 255.0;
            if x <= 0.039_28 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * ch(c[0]) + 0.7152 * ch(c[1]) + 0.0722 * ch(c[2])
    };
    let (l1, l2) = (lum(a), lum(b));
    (l1.max(l2) + 0.05) / (l1.min(l2) + 0.05)
}

/// La forme en CSS (mode Dev de Figma) : taille, position dans son plan, fond, bordure, coins, ombre, police
pub fn css_of(d: &Doc, s: &Shape) -> String {
    let b = d.bounds(s);
    let board = d.boards.iter().find(|bd| b[0] >= bd.x && b[1] >= bd.y && b[0] <= bd.x + bd.w && b[1] <= bd.y + bd.h);
    let (ox, oy) = board.map(|bd| (bd.x, bd.y)).unwrap_or((0.0, 0.0));
    let rgba = |c: [u8; 4]| if c[3] == 255 { format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]) } else { format!("rgba({}, {}, {}, {:.2})", c[0], c[1], c[2], c[3] as f32 / 255.0) };
    let mut out = format!("/* {} */\nposition: absolute;\nleft: {:.0}px;\ntop: {:.0}px;\nwidth: {:.0}px;\nheight: {:.0}px;\n", s.name, b[0] - ox, b[1] - oy, b[2] - b[0], b[3] - b[1]);
    let is_text = matches!(s.kind, Kind::Text { .. });
    if let Some(f) = s.fills.first() {
        let v = match f {
            Paint::Solid { c } => rgba(*c),
            Paint::Linear { stops, angle } => format!("linear-gradient({:.0}deg, {})", angle + 90.0, stops.iter().map(|(t, c)| format!("{} {:.0}%", rgba(*c), t * 100.0)).collect::<Vec<_>>().join(", ")),
            Paint::Radial { stops } => format!("radial-gradient({})", stops.iter().map(|(t, c)| format!("{} {:.0}%", rgba(*c), t * 100.0)).collect::<Vec<_>>().join(", ")),
            Paint::Pattern { bg, .. } => rgba(*bg),
            Paint::Freeform { pts } | Paint::Mesh { pts, .. } => pts.first().map(|p| rgba(p.1)).unwrap_or_default(),
        };
        out.push_str(&format!("{}: {v};\n", if is_text { "color" } else if matches!(f, Paint::Solid { .. }) { "background-color" } else { "background" }));
    }
    if let Some(k) = s.strokes.first() {
        out.push_str(&format!("border: {:.0}px {} {};\n", k.width, if k.dash.is_empty() { "solid" } else { "dashed" }, rgba(k.paint.first())));
    }
    match &s.kind {
        Kind::Rect { r, .. } if r.iter().any(|x| *x > 0.0) => out.push_str(&if r.iter().all(|x| *x == r[0]) { format!("border-radius: {:.0}px;\n", r[0]) } else { format!("border-radius: {:.0}px {:.0}px {:.0}px {:.0}px;\n", r[0], r[1], r[2], r[3]) }),
        Kind::Ellipse { .. } => out.push_str("border-radius: 50%;\n"),
        Kind::Text { size, font, align, spacing, leading, .. } => {
            out.push_str(&format!("font-family: \"{font}\";\nfont-size: {size:.0}px;\nline-height: {leading:.2};\n"));
            if *spacing != 0.0 {
                out.push_str(&format!("letter-spacing: {spacing:.1}px;\n"));
            }
            out.push_str(&format!("text-align: {};\n", ["left", "center", "right"][(*align).min(2) as usize]));
        }
        _ => {}
    }
    if let Some(sh) = &s.shadow {
        out.push_str(&format!("box-shadow: {:.0}px {:.0}px {:.0}px {};\n", sh.dx, sh.dy, sh.blur, rgba(sh.c)));
    }
    if s.opacity < 1.0 {
        out.push_str(&format!("opacity: {:.2};\n", s.opacity));
    }
    if s.blur > 0.0 {
        out.push_str(&format!("filter: blur({:.0}px);\n", s.blur / 2.0));
    }
    if s.rot != 0.0 {
        out.push_str(&format!("transform: rotate({:.1}deg);\n", s.rot));
    }
    if !s.blend.is_empty() && s.blend != "normal" {
        out.push_str(&format!("mix-blend-mode: {};\n", s.blend));
    }
    out
}

/// Ce que le mode Dev sait écrire pour une forme
pub const CODE_STYLES: [&str; 3] = ["CSS", "SwiftUI", "egui"];

/// La forme en SwiftUI (mode Dev) : forme, remplissage, bordure, coins, ombre, taille et place dans le plan
pub fn swiftui_of(d: &Doc, s: &Shape) -> String {
    let b = d.bounds(s);
    let board = d.boards.iter().find(|bd| b[0] >= bd.x && b[1] >= bd.y && b[0] <= bd.x + bd.w && b[1] <= bd.y + bd.h);
    let (ox, oy) = board.map(|bd| (bd.x, bd.y)).unwrap_or((0.0, 0.0));
    let col = |c: [u8; 4]| format!("Color(red: {:.3}, green: {:.3}, blue: {:.3}, opacity: {:.2})", c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, c[3] as f32 / 255.0);
    let fill = s.fills.first().map(|f| match f {
        Paint::Solid { c } => col(*c),
        Paint::Linear { stops, angle } => {
            let a = (angle + 90.0).to_radians();
            format!("LinearGradient(stops: [{}], startPoint: UnitPoint(x: {:.2}, y: {:.2}), endPoint: UnitPoint(x: {:.2}, y: {:.2}))", stops.iter().map(|(t, c)| format!(".init(color: {}, location: {:.2})", col(*c), t)).collect::<Vec<_>>().join(", "), 0.5 - a.sin() * 0.5, 0.5 + a.cos() * 0.5, 0.5 + a.sin() * 0.5, 0.5 - a.cos() * 0.5)
        }
        Paint::Radial { stops } => format!("RadialGradient(stops: [{}], center: .center, startRadius: 0, endRadius: {:.0})", stops.iter().map(|(t, c)| format!(".init(color: {}, location: {:.2})", col(*c), t)).collect::<Vec<_>>().join(", "), (b[2] - b[0]).max(b[3] - b[1]) / 2.0),
        Paint::Pattern { bg, .. } => col(*bg),
        Paint::Freeform { pts } | Paint::Mesh { pts, .. } => pts.first().map(|p| col(p.1)).unwrap_or_else(|| "Color.clear".into()),
    });
    let mut out = format!("// {}
", s.name);
    match &s.kind {
        Kind::Text { text, size, font, align, .. } => {
            out.push_str(&format!("Text({:?})
    .font(.custom({font:?}, size: {size:.0}))
    .multilineTextAlignment(.{})
", text, ["leading", "center", "trailing"][(*align).min(2) as usize]));
            if let Some(f) = &fill {
                out.push_str(&format!("    .foregroundStyle({f})
"));
            }
        }
        k => {
            let shape = match k {
                Kind::Ellipse { .. } => "Ellipse()".to_string(),
                Kind::Rect { r, .. } if r.iter().all(|x| *x == r[0]) && r[0] > 0.0 => format!("RoundedRectangle(cornerRadius: {:.0})", r[0]),
                Kind::Rect { r, .. } if r.iter().any(|x| *x > 0.0) => format!("UnevenRoundedRectangle(topLeadingRadius: {:.0}, bottomLeadingRadius: {:.0}, bottomTrailingRadius: {:.0}, topTrailingRadius: {:.0})", r[0], r[3], r[2], r[1]),
                _ => "Rectangle()".to_string(),
            };
            out.push_str(&shape);
            out.push('\n');
            out.push_str(&format!("    .fill({})
", fill.clone().unwrap_or_else(|| "Color.clear".into())));
            if let Some(k) = s.strokes.first() {
                let c = k.paint.first();
                out.push_str(&format!("    .overlay({}.stroke({}, lineWidth: {:.0}))
", shape, col(c), k.width));
            }
        }
    }
    out.push_str(&format!("    .frame(width: {:.0}, height: {:.0})
", b[2] - b[0], b[3] - b[1]));
    if let Some(sh) = &s.shadow {
        out.push_str(&format!("    .shadow(color: {}, radius: {:.0}, x: {:.0}, y: {:.0})
", col(sh.c), sh.blur / 2.0, sh.dx, sh.dy));
    }
    if s.opacity < 1.0 {
        out.push_str(&format!("    .opacity({:.2})
", s.opacity));
    }
    if s.blur > 0.0 {
        out.push_str(&format!("    .blur(radius: {:.0})
", s.blur / 2.0));
    }
    if s.rot != 0.0 {
        out.push_str(&format!("    .rotationEffect(.degrees({:.1}))
", s.rot));
    }
    out.push_str(&format!("    .position(x: {:.0}, y: {:.0})
", (b[0] + b[2]) / 2.0 - ox, (b[1] + b[3]) / 2.0 - oy));
    out
}

/// La forme en egui (Rust, mode Dev) : un Frame aux mêmes couleurs, bordure, coins et ombre, ou un texte
pub fn egui_of(d: &Doc, s: &Shape) -> String {
    let b = d.bounds(s);
    let c32 = |c: [u8; 4]| if c[3] == 255 { format!("egui::Color32::from_rgb({}, {}, {})", c[0], c[1], c[2]) } else { format!("egui::Color32::from_rgba_unmultiplied({}, {}, {}, {})", c[0], c[1], c[2], c[3]) };
    let fill = s.fills.first().map(|f| match f {
        Paint::Solid { c } => *c,
        Paint::Linear { stops, .. } | Paint::Radial { stops } => stops.first().map(|x| x.1).unwrap_or([0, 0, 0, 0]),
        Paint::Pattern { bg, .. } => *bg,
        Paint::Freeform { pts } | Paint::Mesh { pts, .. } => pts.first().map(|p| p.1).unwrap_or([0, 0, 0, 0]),
    });
    let mut out = format!("// {}
", s.name);
    if let Kind::Text { text, size, .. } = &s.kind {
        out.push_str(&format!("ui.label(egui::RichText::new({:?}).size({size:.1})", text));
        if let Some(c) = fill {
            out.push_str(&format!(".color({})", c32(c)));
        }
        out.push_str(");
");
        return out;
    }
    out.push_str("egui::Frame::new()
");
    if let Some(c) = fill {
        out.push_str(&format!("    .fill({})
", c32(c)));
    }
    if let Some(k) = s.strokes.first() {
        out.push_str(&format!("    .stroke(egui::Stroke::new({:.1}, {}))
", k.width, c32(k.paint.first())));
    }
    match &s.kind {
        Kind::Rect { r, .. } if r.iter().any(|x| *x > 0.0) => out.push_str(&if r.iter().all(|x| *x == r[0]) { format!("    .corner_radius({:.0})
", r[0]) } else { format!("    .corner_radius(egui::CornerRadius {{ nw: {:.0}, ne: {:.0}, se: {:.0}, sw: {:.0} }})
", r[0], r[1], r[2], r[3]) }),
        Kind::Ellipse { .. } => out.push_str(&format!("    .corner_radius({:.0})
", (b[2] - b[0]).min(b[3] - b[1]) / 2.0)),
        _ => {}
    }
    if let Some(sh) = &s.shadow {
        out.push_str(&format!("    .shadow(egui::Shadow {{ offset: [{:.0}, {:.0}], blur: {:.0}, spread: 0, color: {} }})
", sh.dx, sh.dy, sh.blur, c32(sh.c)));
    }
    out.push_str(&format!("    .show(ui, |ui| {{
        ui.set_min_size(egui::vec2({:.0}.0, {:.0}.0));
    }});
", b[2] - b[0], b[3] - b[1]));
    if s.opacity < 1.0 {
        out.push_str(&format!("// opacité {:.0} % : ui.multiply_opacity({:.2}) avant
", s.opacity * 100.0, s.opacity));
    }
    out
}

/// Le code de la forme dans le style choisi (0 CSS, 1 SwiftUI, 2 egui)
pub fn code_of(d: &Doc, s: &Shape, style: usize) -> String {
    match style {
        1 => swiftui_of(d, s),
        2 => egui_of(d, s),
        _ => css_of(d, s),
    }
}

/// La ligne que suit un texte sur un tracé : ses points (monde) et leur abscisse curviligne
pub fn guide_line(p: &Shape) -> Option<Vec<([f32; 2], f32)>> {
    let (nodes, closed) = match &p.kind {
        Kind::Path { nodes, closed } => (nodes.clone(), *closed),
        Kind::Ellipse { w, h } => {
            let (rx, ry, k) = (w / 2.0, h / 2.0, 0.552_284_8);
            let c = [rx, ry];
            (
                vec![
                    Node { p: [c[0] - rx, c[1]], i: [c[0] - rx, c[1] + ry * k], o: [c[0] - rx, c[1] - ry * k] },
                    Node { p: [c[0], c[1] - ry], i: [c[0] - rx * k, c[1] - ry], o: [c[0] + rx * k, c[1] - ry] },
                    Node { p: [c[0] + rx, c[1]], i: [c[0] + rx, c[1] - ry * k], o: [c[0] + rx, c[1] + ry * k] },
                    Node { p: [c[0], c[1] + ry], i: [c[0] + rx * k, c[1] + ry], o: [c[0] - rx * k, c[1] + ry] },
                ],
                true,
            )
        }
        Kind::Line { x2, y2 } => (vec![Node::corner([0.0, 0.0]), Node::corner([*x2, *y2])], false),
        _ => return None,
    };
    let (sn, cs) = p.rot.to_radians().sin_cos();
    let t = |q: [f32; 2]| [p.x + q[0] * cs - q[1] * sn, p.y + q[0] * sn + q[1] * cs];
    let pts: Vec<([f32; 2], f32)> = crate::vgeom::flatten(&nodes, closed).into_iter().map(|(q, d)| (t(q), d)).collect();
    (pts.len() > 1).then_some(pts)
}

/// Un texte posé le long d'une ligne : chaque lettre suit la courbe, tournée selon sa pente (tracé SVG, monde)
pub fn text_on_path(s: &Shape, guide: &[([f32; 2], f32)]) -> String {
    use ab_glyph::{Font, ScaleFont};
    let Kind::Text { text, size, font, align, spacing, .. } = &s.kind else { return String::new() };
    let f = crate::motion::font(font);
    let sf = f.as_scaled(*size);
    let line: String = text.replace('\n', " ");
    let total: f32 = line.chars().map(|c| sf.h_advance(f.glyph_id(c)) + spacing).sum();
    let len = guide.last().map(|x| x.1).unwrap_or(0.0);
    let mut x = match align {
        1 => (len - total) / 2.0,
        2 => len - total,
        _ => 0.0,
    };
    // Point et pente de la ligne à l'abscisse `d`
    let at = |d: f32| -> ([f32; 2], f32) {
        let k = guide.partition_point(|g| g.1 < d).clamp(1, guide.len() - 1);
        let ((a, da), (b, db)) = (guide[k - 1], guide[k]);
        let u = ((d - da) / (db - da).max(1e-6)).clamp(0.0, 1.0);
        ([a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u], (b[1] - a[1]).atan2(b[0] - a[0]))
    };
    let mut d = String::new();
    let (kx, ky) = (sf.h_scale_factor(), sf.v_scale_factor());
    for c in line.chars() {
        let id = f.glyph_id(c);
        let adv = sf.h_advance(id);
        let mid = x + adv / 2.0;
        if mid >= 0.0 && mid <= len {
            let (o, ang) = at(mid);
            let (sn, cs) = ang.sin_cos();
            // La lettre, centrée sur son point, ligne de base sur la courbe
            let tr = |p: ab_glyph::Point| {
                let (lx, ly) = (p.x * kx - adv / 2.0, -p.y * ky);
                [o[0] + lx * cs - ly * sn, o[1] + lx * sn + ly * cs]
            };
            if let Some(ol) = f.outline(id) {
                let mut last: Option<[f32; 2]> = None;
                for curve in &ol.curves {
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
                        Quad(_, c1, _) => {
                            let c1 = tr(*c1);
                            let _ = write!(d, "Q{:.2} {:.2} {:.2} {:.2}", c1[0], c1[1], end[0], end[1]);
                        }
                        Cubic(_, c1, c2, _) => {
                            let (c1, c2) = (tr(*c1), tr(*c2));
                            let _ = write!(d, "C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2}", c1[0], c1[1], c2[0], c2[1], end[0], end[1]);
                        }
                    }
                    last = Some(end);
                }
                if last.is_some() {
                    d.push('Z');
                }
            }
        }
        x += adv + spacing;
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

/// Une forme dessinable : position et taille finies et raisonnables. Un fichier abîmé (taille de 1e30, position
/// infinie) ferait sinon tomber le rendu ; la forme reste dans le document, elle n'est simplement pas dessinée.
pub fn sane(s: &Shape) -> bool {
    const MAX: f32 = 1e7;
    let ok = |v: f32| v.is_finite() && v.abs() < MAX;
    ok(s.x) && ok(s.y) && ok(s.rot) && ok(s.blur) && ok(s.opacity) && (matches!(s.kind, Kind::Group { .. } | Kind::Text { .. }) || local_bounds(s).iter().all(|v| ok(*v)))
        && s.strokes.iter().all(|k| ok(k.width) && k.dash.iter().all(|d| ok(*d)))
        && s.shadow.as_ref().is_none_or(|h| ok(h.dx) && ok(h.dy) && ok(h.blur))
        && match &s.kind {
            Kind::Text { size, spacing, leading, width, .. } => ok(*size) && ok(*spacing) && ok(*leading) && width.is_none_or(ok),
            Kind::Polygon { sides, .. } => *sides <= 10_000,
            Kind::Star { points, .. } => *points <= 10_000,
            _ => true,
        }
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
                // Un écran de téléphone a les coins arrondis
                let rx = match b.device {
                    1 => 48.0,
                    2 => 30.0,
                    _ => 0.0,
                };
                let _ = write!(body, r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{rx}" fill="{}" fill-opacity="{}"/>"#, b.x, b.y, b.w, b.h, hex(c), c[3] as f32 / 255.0);
            }
        }
    }
    // Cadres d'appareils : la coque passe sous le plan (dessinée avant tout)
    if with_boards {
        let mut under = String::new();
        for b in doc.boards.iter().filter(|b| b.device != 0) {
            under.push_str(&device_frame(b));
        }
        body.insert_str(0, &under);
    }
    let scale = if area[2] > 0.0 && area[3] > 0.0 { (px[0] as f32 / area[2]).max(px[1] as f32 / area[3]) } else { 1.0 };
    let mut cx = Gen { doc, root, defs: &mut defs, n: 0, scale };
    for sh in doc.shapes.iter().filter(|s| s.parent == 0) {
        cx.shape(&mut body, sh);
    }
    // Puis la barre d'état et l'encoche, par-dessus le contenu
    if with_boards {
        for b in doc.boards.iter().filter(|b| b.device != 0) {
            body.push_str(&device_overlay(b));
        }
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
    /// Pixels rendus par unité du dessin (finesse du filet de dégradé)
    scale: f32,
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
            Paint::Freeform { pts } | Paint::Mesh { pts, .. } => {
                // Ailleurs qu'un remplissage (contour, texte sur tracé) : la couleur moyenne
                let n = pts.len().max(1) as u32;
                let avg = [0, 1, 2, 3].map(|i| (pts.iter().map(|p| p.1[i] as u32).sum::<u32>() / n) as u8);
                let _ = bounds;
                (hex(avg), avg[3] as f32 / 255.0)
            }
            Paint::Pattern { motif, fg, bg, size, angle } => {
                let id = self.id("m");
                let s = size.max(1.0);
                let _ = write!(self.defs, r#"<pattern id="{id}" patternUnits="userSpaceOnUse" width="{s}" height="{s}" patternTransform="rotate({angle})">"#);
                if bg[3] > 0 {
                    let _ = write!(self.defs, r#"<rect width="{s}" height="{s}" fill="{}" fill-opacity="{}"/>"#, hex(*bg), bg[3] as f32 / 255.0);
                }
                self.defs.push_str(&motif_svg(*motif, &hex(*fg), fg[3] as f32 / 255.0, s));
                self.defs.push_str("</pattern>");
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
            let mut last = "SourceGraphic";
            if let Some(d) = &sh.shadow {
                if d.inner {
                    // L'ombre entre par les bords : l'alpha décalé et flou, retourné, gardé dans la forme
                    let _ = write!(self.defs, r#"<feOffset in="SourceAlpha" dx="{}" dy="{}" result="io"/><feGaussianBlur in="io" stdDeviation="{}" result="ib"/><feComposite in="SourceAlpha" in2="ib" operator="out" result="ie"/><feFlood flood-color="{}" flood-opacity="{}" result="ic"/><feComposite in="ic" in2="ie" operator="in" result="is"/><feMerge result="s1"><feMergeNode in="SourceGraphic"/><feMergeNode in="is"/></feMerge>"#, d.dx, d.dy, d.blur / 2.0, hex(d.c), d.c[3] as f32 / 255.0);
                } else {
                    let _ = write!(self.defs, r#"<feDropShadow in="SourceGraphic" dx="{}" dy="{}" stdDeviation="{}" flood-color="{}" flood-opacity="{}" result="s1"/>"#, d.dx, d.dy, d.blur / 2.0, hex(d.c), d.c[3] as f32 / 255.0);
                }
                last = "s1";
            }
            if sh.blur > 0.0 {
                let _ = write!(self.defs, r#"<feGaussianBlur in="{last}" stdDeviation="{}"/>"#, sh.blur / 2.0);
            }
            self.defs.push_str("</filter>");
            filter = format!(r#" filter="url(#{id})""#);
        }
        let _ = write!(out, r#"<g transform="translate({} {}) rotate({})" opacity="{}"{}{}>"#, sh.x, sh.y, sh.rot, sh.opacity, filter, if style.is_empty() { String::new() } else { format!(r#" style="{style}""#) });
    }

    fn shape(&mut self, out: &mut String, sh: &Shape) {
        if !sh.visible || !sane(sh) {
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
                // Masque d'écrêtage (comme Figma) : la forme du dessous découpe celles du dessus
                (6, Some((first, rest))) if !rest.is_empty() => {
                    let mid = self.id("m");
                    let mut mask = format!(r#"<mask id="{mid}" maskUnits="userSpaceOnUse" x="-100000" y="-100000" width="200000" height="200000">"#);
                    self.silhouette(&mut mask, first, "white");
                    mask.push_str("</mask>");
                    self.defs.push_str(&mask);
                    let _ = write!(out, r#"<g mask="url(#{mid})">"#);
                    for k in rest {
                        self.shape(out, k);
                    }
                    out.push_str("</g>");
                }
                // Masque d'opacité : la luminosité de la forme du dessous (dégradé compris) règle l'opacité du dessus
                (7, Some((first, rest))) if !rest.is_empty() => {
                    let mid = self.id("m");
                    let mut mask = format!(r#"<mask id="{mid}" maskUnits="userSpaceOnUse" x="-100000" y="-100000" width="200000" height="200000">"#);
                    self.shape(&mut mask, first);
                    mask.push_str("</mask>");
                    self.defs.push_str(&mask);
                    let _ = write!(out, r#"<g mask="url(#{mid})">"#);
                    for k in rest {
                        self.shape(out, k);
                    }
                    out.push_str("</g>");
                }
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
        // Texte sur un tracé : les lettres suivent la ligne (en coordonnées du monde)
        if let (Kind::Text { .. }, true) = (&sh.kind, sh.on_path != 0) {
            if let Some(g) = self.doc.get(sh.on_path).and_then(guide_line) {
                let d = text_on_path(sh, &g);
                let b = self.doc.bounds(sh);
                let _ = write!(out, r#"<g opacity="{}">"#, sh.opacity);
                for f in sh.fills.clone() {
                    let (p, o) = self.paint(&f, b);
                    let _ = write!(out, r#"<path d="{d}" fill="{p}" fill-opacity="{o}"/>"#);
                }
                out.push_str("</g>");
                return;
            }
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
                    // Filet de dégradé : des petits carreaux d'une couleur, découpés par la forme (reste vectoriel)
                    if let Paint::Mesh { cols, rows, pts } = f {
                        if mesh_ok(*cols, *rows, pts) {
                            if pts.iter().all(|p| p.1[3] == 255) {
                                let (p, o) = self.paint(f, bounds);
                                let _ = write!(out, r#"<path d="{d}" fill="{p}" fill-opacity="{o}" fill-rule="nonzero"/>"#);
                            }
                            let id = self.id("c");
                            let _ = write!(self.defs, r#"<clipPath id="{id}"><path d="{d}"/></clipPath>"#);
                            let _ = write!(out, r#"<g clip-path="url(#{id})" shape-rendering="crispEdges">"#);
                            let (bw, bh) = (bounds[2] - bounds[0], bounds[3] - bounds[1]);
                            // Un carreau d'environ 4 px à l'écran : fondu à l'œil, sans multiplier les tracés pour une petite forme
                            let across = (bw.abs().max(bh.abs()) * self.scale / 4.0).clamp(16.0, 96.0);
                            let n = ((across / (*cols).max(*rows) as f32).ceil() as usize).clamp(2, 48);
                            for (q, c) in mesh_quads(*cols, *rows, pts, n) {
                                let q = q.map(|p| [bounds[0] + p[0] * bw, bounds[1] + p[1] * bh]);
                                let _ = write!(out, r#"<path d="M{:.2} {:.2}L{:.2} {:.2}L{:.2} {:.2}L{:.2} {:.2}Z" fill="{}""#, q[0][0], q[0][1], q[1][0], q[1][1], q[2][0], q[2][1], q[3][0], q[3][1], hex(c));
                                if c[3] < 255 {
                                    let _ = write!(out, r#" fill-opacity="{:.3}""#, c[3] as f32 / 255.0);
                                }
                                out.push_str("/>");
                            }
                            out.push_str("</g>");
                            continue;
                        }
                    }
                    // Dégradé à points libres : la couleur moyenne, puis un halo par point qui s'efface
                    if let Paint::Freeform { pts } = f {
                        let (p, o) = self.paint(f, bounds);
                        let _ = write!(out, r#"<path d="{d}" fill="{p}" fill-opacity="{o}" fill-rule="nonzero"/>"#);
                        let (bw, bh) = ((bounds[2] - bounds[0]).max(1e-3), (bounds[3] - bounds[1]).max(1e-3));
                        let r = (bw * bw + bh * bh).sqrt() * 0.45;
                        for (uv, c) in pts {
                            let id = self.id("g");
                            let (cx, cy) = (bounds[0] + uv[0] * bw, bounds[1] + uv[1] * bh);
                            let (hx, a) = (hex(*c), c[3] as f32 / 255.0);
                            let _ = write!(self.defs, r#"<radialGradient id="{id}" gradientUnits="userSpaceOnUse" cx="{cx}" cy="{cy}" r="{r}"><stop offset="0" stop-color="{hx}" stop-opacity="{a}"/><stop offset="0.35" stop-color="{hx}" stop-opacity="{}"/><stop offset="1" stop-color="{hx}" stop-opacity="0"/></radialGradient>"#, a * 0.8);
                            let _ = write!(out, r#"<path d="{d}" fill="url(#{id})" fill-rule="nonzero"/>"#);
                        }
                        continue;
                    }
                    let (p, o) = self.paint(f, bounds);
                    let _ = write!(out, r#"<path d="{d}" fill="{p}" fill-opacity="{o}" fill-rule="nonzero"/>"#);
                }
                // Contour à largeur variable ou en forme de pinceau : un tracé plein, peint comme le contour
                if !sh.widths.is_empty() || sh.strokes.first().is_some_and(|k| k.brush != 0) {
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
    // Une image démesurée (plan de travail abîmé, échelle folle) : refusée plutôt que d'épuiser la mémoire
    if w as u64 * h as u64 > 16384 * 16384 {
        return None;
    }
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
                        Stroke { paint: Paint::solid(c), width: s.width().get() * t.sx.abs().max(t.sy.abs()), cap: 1, join: 1, dash: vec![], arrow: false, brush: 0 }
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
            Paint::Pattern { fg, bg, .. } => {
                add(*fg);
                add(*bg);
            }
            Paint::Freeform { pts } | Paint::Mesh { pts, .. } => pts.iter().for_each(|p| add(p.1)),
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
                    Paint::Pattern { fg, bg, .. } => {
                        fix(fg);
                        fix(bg);
                    }
                    Paint::Freeform { pts } | Paint::Mesh { pts, .. } => pts.iter_mut().for_each(|p| fix(&mut p.1)),
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
    fn bibliotheque_partagee() {
        let dir = std::env::temp_dir().join(format!("aryadne-lib-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Le dessin qui publie : un composant « Bouton » (un groupe avec un fond)
        let mut lib = Doc::default();
        lib.shapes.push(Shape { id: 1, name: "Bouton".into(), kind: Kind::Group { children: vec![2], op: 0 }, component: true, ..Shape::base() });
        lib.shapes.push(Shape { id: 2, name: "fond".into(), parent: 1, fills: vec![Paint::Solid { c: [10, 20, 30, 255] }], ..Shape::base() });
        lib.next_id = 3;
        std::fs::write(dir.join("Lib.aryvec"), serde_json::to_string(&lib).unwrap()).unwrap();
        let root = dir.to_string_lossy().to_string();
        let items = library(&root, &dir.join("Moi.aryvec"));
        assert_eq!(items.len(), 1);
        let mut d = Doc::starter();
        let inst = d.place_from_library(&root, &items[0], [500.0, 500.0]).unwrap();
        assert!(d.get(inst).is_some_and(|s| s.instance_of != 0));
        // Deuxième pose : pas de second maître
        d.place_from_library(&root, &items[0], [300.0, 300.0]).unwrap();
        assert_eq!(d.shapes.iter().filter(|s| !s.lib.is_empty()).count(), 1);
        // La bibliothèque change : le maître et ses instances suivent
        lib.shapes[1].fills = vec![Paint::Solid { c: [200, 0, 0, 255] }];
        std::fs::write(dir.join("Lib.aryvec"), serde_json::to_string(&lib).unwrap()).unwrap();
        assert_eq!(d.update_library(&root), 1);
        let red = d.shapes.iter().filter(|s| s.fills.first() == Some(&Paint::Solid { c: [200, 0, 0, 255] })).count();
        assert!(red >= 3, "maître + 2 instances : {red}");
        let _ = std::fs::remove_dir_all(&dir);
    }


    #[test]
    fn etat_au_survol() {
        let mut d = Doc::default();
        d.shapes.push(Shape { id: 1, fills: vec![Paint::Solid { c: [10, 10, 10, 255] }], hover: Some(Hover { fill: Some([200, 0, 0, 255]), opacity: 0.5, lift: true }), ..Shape::base() });
        let h = d.hovered(1);
        assert_eq!(h.shapes[0].fills[0], Paint::Solid { c: [200, 0, 0, 255] });
        assert_eq!(h.shapes[0].opacity, 0.5);
        assert!(h.shapes[0].shadow.is_some());
        // L'original ne bouge pas
        assert_eq!(d.shapes[0].fills[0], Paint::Solid { c: [10, 10, 10, 255] });
    }


    #[test]
    fn animation_des_plans() {
        let mut d = Doc::default();
        for (i, x) in [0.0f32, 200.0].iter().enumerate() {
            d.boards.push(Board { name: format!("P{i}"), x: *x, y: 0.0, w: 40.0, h: 30.0, bg: Some([255; 4]), grid: None, persp: None, device: 0, mode: 0 });
            d.shapes.push(Shape { id: i as u64 + 1, name: "carré".into(), x: x + 5.0 + i as f32 * 20.0, y: 5.0, kind: Kind::Rect { w: 10.0, h: 10.0, r: [0.0; 4] }, fills: vec![Paint::Solid { c: [200, 0, 0, 255] }], ..Shape::base() });
        }
        let fr = anim_frames(&d, "", 1.0, 10.0, 0.5, 0.5);
        // 5 images tenues par plan, 4 de transition
        assert_eq!(fr.len(), 14);
        assert_eq!((fr[0].width(), fr[0].height()), (40, 30));
        let gif = std::env::temp_dir().join("aryadne-anim-test.gif");
        write_gif(&fr, 10.0, &gif).unwrap();
        assert!(std::fs::metadata(&gif).unwrap().len() > 100);
        let _ = std::fs::remove_file(&gif);
        if crate::ffmpeg::find("ffmpeg").is_some() {
            let mp4 = std::env::temp_dir().join("aryadne-anim-test.mp4");
            write_mp4(&fr, 10.0, &mp4).unwrap();
            assert!(std::fs::metadata(&mp4).unwrap().len() > 100);
            let _ = std::fs::remove_file(&mp4);
        }
    }


    #[test]
    fn variables_et_modes() {
        let mut d = Doc::default();
        d.modes = vec!["clair".into(), "sombre".into()];
        d.vars.push(Variable { name: "fond".into(), values: vec![[255, 255, 255, 255], [20, 20, 24, 255]] });
        d.boards.push(Board { name: "A".into(), x: 0.0, y: 0.0, w: 100.0, h: 100.0, bg: None, grid: None, persp: None, device: 0, mode: 0 });
        d.boards.push(Board { name: "B".into(), x: 200.0, y: 0.0, w: 100.0, h: 100.0, bg: None, grid: None, persp: None, device: 0, mode: 1 });
        for x in [10.0, 210.0] {
            d.shapes.push(Shape { x, y: 10.0, kind: Kind::Rect { w: 50.0, h: 50.0, r: [0.0; 4] }, fill_var: "fond".into(), ..Shape::base() });
        }
        assert!(d.apply_vars());
        assert_eq!(d.shapes[0].fills[0], Paint::Solid { c: [255, 255, 255, 255] });
        assert_eq!(d.shapes[1].fills[0], Paint::Solid { c: [20, 20, 24, 255] });
        assert!(!d.apply_vars());
    }


    #[test]
    fn mode_dev_trois_styles() {
        let mut d = Doc::default();
        let mut sh = Shape { x: 10.0, y: 20.0, kind: Kind::Rect { w: 100.0, h: 40.0, r: [0.0; 4] }, ..Shape::base() };
        sh.fills = vec![Paint::Solid { c: [255, 0, 0, 255] }];
        if let Kind::Rect { r, .. } = &mut sh.kind {
            *r = [8.0; 4];
        }
        d.shapes.push(sh.clone());
        let s = d.shapes.last().unwrap();
        assert!(code_of(&d, s, 0).contains("border-radius: 8px"));
        let sw = code_of(&d, s, 1);
        assert!(sw.contains("RoundedRectangle(cornerRadius: 8)") && sw.contains(".frame(width: 100, height: 40)"), "{sw}");
        let eg = code_of(&d, s, 2);
        assert!(eg.contains("from_rgb(255, 0, 0)") && eg.contains(".corner_radius(8)"), "{eg}");
    }


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
        let b = Board { name: String::new(), x: 0.0, y: 0.0, w: 1000.0, h: 100.0, bg: None, grid: Some(LayoutGrid { cols: 4, gutter: 20.0, margin: 40.0 }), persp: None, device: 0, mode: 0 };
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
    fn texte_sur_trace() {
        let mut d = Doc::starter();
        let p = d.new_id();
        d.shapes.push(Shape { id: p, kind: Kind::Path { nodes: vec![Node::corner([0.0, 100.0]), Node::corner([400.0, 100.0])], closed: false }, ..Shape::base() });
        let t = d.new_id();
        d.shapes.push(Shape { id: t, kind: Kind::Text { text: "Bonjour".into(), size: 40.0, font: "inter".into(), align: 0, width: None, spacing: 0.0, leading: 1.0 }, on_path: p, fills: vec![Paint::solid([0, 0, 0, 255])], ..Shape::base() });
        let g = guide_line(d.get(p).unwrap()).unwrap();
        let path = text_on_path(d.get(t).unwrap(), &g);
        assert!(path.starts_with('M') && path.contains('Z'));
        let b = d.bounds(d.get(t).unwrap());
        assert!(b[0] < 0.0 && b[2] > 400.0);
        assert!(to_svg(&d, [0.0, 0.0, 500.0, 300.0], [100, 60], ".", false).contains(&path[..20]));
    }

    #[test]
    fn contraste_css_masque() {
        assert!((contrast([0, 0, 0, 255], [255, 255, 255, 255]) - 21.0).abs() < 0.01);
        assert!(contrast([119, 119, 119, 255], [255, 255, 255, 255]) > 4.4);
        let mut d = Doc::starter();
        let id = d.new_id();
        d.shapes.push(Shape { id, name: "Bouton".into(), kind: Kind::Rect { w: 120.0, h: 40.0, r: [8.0; 4] }, x: 10.0, y: 20.0, fills: vec![Paint::solid([236, 108, 89, 255])], ..Shape::base() });
        let css = css_of(&d, d.get(id).unwrap());
        assert!(css.contains("left: 10px;") && css.contains("width: 120px;") && css.contains("background-color: #ec6c59;") && css.contains("border-radius: 8px;"), "{css}");
        // Masque : un cercle qui découpe un rectangle
        let c = d.new_id();
        d.shapes.push(Shape { id: c, kind: Kind::Ellipse { w: 50.0, h: 50.0 }, fills: vec![Paint::solid([0, 0, 0, 255])], ..Shape::base() });
        let g = d.group(&[c, id], 6).unwrap();
        assert!(to_svg(&d, [0.0, 0.0, 200.0, 200.0], [50, 50], ".", false).contains("mask="));
        assert_eq!(d.get(g).unwrap().name, "Masque");
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

// ---------- Déformations (enveloppes prédéfinies, comme Effet > Déformation d'Illustrator) ----------

pub const WARPS: [&str; 9] = ["Arc", "Arche", "Renflement", "Drapeau", "Vague", "Poisson", "Montée", "Gonfler", "Torsion"];

/// Où va le point `p` (repère de la forme, cadre `b`) avec la déformation `kind` d'intensité `bend` (-1 à 1)
pub fn warp_point(kind: usize, b: [f32; 4], p: [f32; 2], bend: f32) -> [f32; 2] {
    let (w, h) = ((b[2] - b[0]).max(1e-3), (b[3] - b[1]).max(1e-3));
    let (u, v) = ((p[0] - b[0]) / w, (p[1] - b[1]) / h);
    let (cu, cv) = (2.0 * u - 1.0, 2.0 * v - 1.0);
    let arc = 1.0 - cu * cu;
    let pi = std::f32::consts::PI;
    let (dx, dy) = match kind {
        0 => (0.0, -bend * h * 0.5 * arc),
        1 => (0.0, -bend * h * 0.5 * arc * (1.0 - v)),
        2 => (0.0, bend * h * 0.35 * arc * cv),
        3 => (0.0, bend * h * 0.25 * (2.0 * pi * u).sin()),
        4 => (0.0, bend * h * 0.2 * (2.0 * pi * u).sin() * (1.0 - cv.abs() * 0.5)),
        5 => (0.0, bend * h * 0.3 * cv * (pi * u).sin() * (1.0 - u)),
        6 => (0.0, -bend * h * 0.5 * u),
        7 => (bend * w * 0.25 * cu * (1.0 - cv * cv), bend * h * 0.25 * cv * (1.0 - cu * cu)),
        _ => {
            let (cx, cy) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
            let r = (((p[0] - cx) / (w / 2.0)).powi(2) + ((p[1] - cy) / (h / 2.0)).powi(2)).sqrt().min(1.0);
            let a = bend * pi * (1.0 - r);
            let (c, sn) = (a.cos(), a.sin());
            let (x, y) = (p[0] - cx, p[1] - cy);
            return [cx + c * x - sn * y, cy + sn * x + c * y];
        }
    };
    [p[0] + dx, p[1] + dy]
}

/// Les contours d'une forme en tracés (repère de la forme), quelle que soit sa nature
pub fn shape_subpaths(s: &Shape) -> Vec<(Vec<Node>, bool)> {
    if let Kind::Path { nodes, closed } = &s.kind {
        return vec![(nodes.clone(), *closed)];
    }
    let d = path_data(s);
    if d.is_empty() {
        return vec![];
    }
    let mut tmp = Doc { shapes: vec![], ..Doc::starter() };
    let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg"><path d="{d}" fill="black"/></svg>"#);
    if import_svg(&mut tmp, &svg, [0.0, 0.0]).is_err() {
        return vec![];
    }
    tmp.shapes.into_iter().filter_map(|sh| match sh.kind {
        Kind::Path { nodes, closed } => Some((nodes, closed)),
        _ => None,
    }).collect()
}

/// Coupe chaque segment en `n` (de Casteljau) : la déformation suit mieux les courbes
pub fn subdivide(nodes: &[Node], closed: bool, n: usize) -> Vec<Node> {
    let m = nodes.len();
    if m < 2 || n < 2 {
        return nodes.to_vec();
    }
    let lerp = |a: [f32; 2], b: [f32; 2], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    let segs = if closed { m } else { m - 1 };
    let mut out: Vec<Node> = vec![nodes[0]];
    for k in 0..segs {
        let (a, b) = (nodes[k], nodes[(k + 1) % m]);
        let (mut p0, mut p1, mut p2, p3) = (a.p, a.o, b.i, b.p);
        for j in 0..n {
            let rem = (n - j) as f32;
            let t = 1.0 / rem;
            // Le morceau [0, t] de la courbe restante
            let (q0, q1, q2) = (lerp(p0, p1, t), lerp(p1, p2, t), lerp(p2, p3, t));
            let (r0, r1) = (lerp(q0, q1, t), lerp(q1, q2, t));
            let s0 = lerp(r0, r1, t);
            if let Some(last) = out.last_mut() {
                last.o = q0;
            }
            let end = if j + 1 == n { p3 } else { s0 };
            out.push(Node { p: end, i: if j + 1 == n { p2 } else { r0 }, o: end });
            (p0, p1, p2) = (s0, r1, q2);
        }
    }
    if closed {
        // Le dernier point refait le premier : on garde sa poignée d'arrivée
        if let Some(last) = out.pop() {
            out[0].i = last.i;
        }
    } else if let Some(last) = out.last_mut() {
        last.o = nodes[m - 1].o;
    }
    out
}

/// Déforme une forme (devenue tracé) ; plusieurs contours (texte, trous) deviennent un groupe pair-impair.
/// Rend la forme obtenue.
pub fn warp_shape(doc: &mut Doc, id: u64, kind: usize, bend: f32) -> Option<u64> {
    remap_shape(doc, id, false, &|b, p| warp_point(kind, b, p, bend))
}

/// Grille de perspective d'un plan de travail : ligne d'horizon et deux points de fuite (fractions du plan)
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Persp {
    pub horizon: f32,
    pub vp: [f32; 2],
}

impl Default for Persp {
    fn default() -> Persp {
        Persp { horizon: 0.42, vp: [-0.35, 1.35] }
    }
}

/// Plaque une forme sur la grille de perspective du plan `b` : 0 mur de gauche, 1 mur de droite, 2 sol.
/// La forme devient un tracé dans le repère du monde.
pub fn persp_shape(doc: &mut Doc, id: u64, b: &Board, plane: u8) -> Option<u64> {
    let ps = b.persp.unwrap_or_default();
    let hz = b.y + ps.horizon * b.h;
    let (v1, v2) = (b.x + ps.vp[0] * b.w, b.x + ps.vp[1] * b.w);
    remap_shape(doc, id, true, &move |r, p| {
        match plane {
            0 | 1 => {
                // Le bord le plus loin du point de fuite garde sa taille, l'autre file vers lui
                let v = if plane == 0 { v1 } else { v2 };
                let near = if (r[0] - v).abs() > (r[2] - v).abs() { r[0] } else { r[2] };
                let f = ((p[0] - v) / (near - v)).clamp(0.02, 50.0);
                [p[0], hz + (p[1] - hz) * f]
            }
            _ => {
                let vc = (v1 + v2) / 2.0;
                let near = if (r[3] - hz).abs() > (r[1] - hz).abs() { r[3] } else { r[1] };
                let f = ((p[1] - hz) / (near - hz)).clamp(0.02, 50.0);
                [vc + (p[0] - vc) * f, p[1]]
            }
        }
    })
}

/// Redessine une forme en tracés par une fonction de points `f(cadre, point)` ; `world` : dans le repère
/// du monde (la forme perd sa position et sa rotation), sinon dans le sien
pub fn remap_shape(doc: &mut Doc, id: u64, world: bool, f: &dyn Fn([f32; 4], [f32; 2]) -> [f32; 2]) -> Option<u64> {
    let i = doc.index(id)?;
    let mut s = doc.shapes[i].clone();
    if matches!(s.kind, Kind::Group { .. } | Kind::Image { .. }) {
        return None;
    }
    let mut subs = shape_subpaths(&s);
    if world {
        let (sn, cs) = s.rot.to_radians().sin_cos();
        let t = |p: [f32; 2]| [s.x + p[0] * cs - p[1] * sn, s.y + p[0] * sn + p[1] * cs];
        for (n, _) in subs.iter_mut() {
            for x in n.iter_mut() {
                *x = Node { p: t(x.p), i: t(x.i), o: t(x.o) };
            }
        }
        (s.x, s.y, s.rot) = (0.0, 0.0, 0.0);
        doc.shapes[i].x = 0.0;
        doc.shapes[i].y = 0.0;
        doc.shapes[i].rot = 0.0;
    }
    let pts: Vec<[f32; 2]> = subs.iter().flat_map(|(n, _)| n.iter().map(|x| x.p)).collect();
    if pts.is_empty() {
        return None;
    }
    let b = [pts.iter().map(|p| p[0]).fold(f32::MAX, f32::min), pts.iter().map(|p| p[1]).fold(f32::MAX, f32::min), pts.iter().map(|p| p[0]).fold(f32::MIN, f32::max), pts.iter().map(|p| p[1]).fold(f32::MIN, f32::max)];
    let warped: Vec<(Vec<Node>, bool)> = subs
        .into_iter()
        .map(|(n, c)| {
            let fine = subdivide(&n, c, 4);
            (fine.into_iter().map(|x| Node { p: f(b, x.p), i: f(b, x.i), o: f(b, x.o) }).collect(), c)
        })
        .collect();
    let text_fill = matches!(s.kind, Kind::Text { .. }) && s.fills.is_empty();
    if warped.len() == 1 {
        let (nodes, closed) = warped.into_iter().next()?;
        let sh = &mut doc.shapes[i];
        sh.kind = Kind::Path { nodes, closed };
        if text_fill {
            sh.fills = vec![Paint::solid([24, 24, 27, 255])];
        }
        return Some(id);
    }
    let mut ids = vec![];
    doc.shapes.remove(i);
    for (k, (nodes, closed)) in warped.into_iter().enumerate() {
        let nid = doc.new_id();
        let mut sh = Shape { id: nid, kind: Kind::Path { nodes, closed }, name: format!("{} {}", s.name, k + 1), parent: 0, ..s.clone() };
        if text_fill {
            sh.fills = vec![Paint::solid([24, 24, 27, 255])];
        }
        doc.shapes.insert((i + k).min(doc.shapes.len()), sh);
        ids.push(nid);
    }
    let g = doc.group(&ids, 5)?;
    if let Some(gs) = doc.shapes.iter_mut().find(|x| x.id == g) {
        gs.name = s.name.clone();
    }
    Some(g)
}

#[cfg(test)]
mod warp_tests {
    use super::*;
    #[test]
    fn deformations() {
        // L'arc garde les coins et monte le milieu
        let b = [0.0, 0.0, 100.0, 50.0];
        assert_eq!(warp_point(0, b, [0.0, 0.0], 1.0), [0.0, 0.0]);
        assert!(warp_point(0, b, [50.0, 0.0], 1.0)[1] < -20.0);
        // Subdiviser garde la forme : le milieu d'un segment droit reste sur la droite
        let n = vec![Node::corner([0.0, 0.0]), Node::corner([100.0, 0.0]), Node::corner([100.0, 100.0])];
        let f = subdivide(&n, true, 4);
        assert_eq!(f.len(), 12);
        assert!(f.iter().all(|x| x.p[0] >= -1e-3 && x.p[0] <= 100.001));
        // Un rectangle déformé en drapeau devient un tracé ondulé
        let mut d = Doc::starter();
        d.shapes.clear();
        let id = d.new_id();
        d.shapes.push(Shape { id, kind: Kind::Rect { w: 200.0, h: 60.0, r: [0.0; 4] }, fills: vec![Paint::solid([200, 80, 80, 255])], ..Shape::base() });
        let r = warp_shape(&mut d, id, 3, 0.8).unwrap();
        let Kind::Path { nodes, closed } = &d.get(r).unwrap().kind else { panic!() };
        assert!(*closed && nodes.len() >= 16);
        assert!(nodes.iter().any(|n| n.p[1] < -5.0));
    }
}

#[cfg(test)]
mod persp_tests {
    use super::*;
    #[test]
    fn perspective() {
        let mut d = Doc::starter();
        d.shapes.clear();
        d.boards[0].persp = Some(Persp::default());
        let b = d.boards[0].clone();
        let id = d.new_id();
        d.shapes.push(Shape { id, kind: Kind::Rect { w: 300.0, h: 300.0, r: [0.0; 4] }, x: 200.0, y: 200.0, fills: vec![Paint::solid([0, 0, 0, 255])], ..Shape::base() });
        let r = persp_shape(&mut d, id, &b, 0).unwrap();
        let Kind::Path { nodes, .. } = &d.get(r).unwrap().kind else { panic!() };
        // Mur de gauche : le bord gauche (vers le point de fuite) rétrécit, le droit garde sa hauteur
        let h_at = |x: f32| {
            let ys: Vec<f32> = nodes.iter().filter(|n| (n.p[0] - x).abs() < 1.0).map(|n| n.p[1]).collect();
            ys.iter().cloned().fold(f32::MIN, f32::max) - ys.iter().cloned().fold(f32::MAX, f32::min)
        };
        assert!((h_at(500.0) - 300.0).abs() < 1.0);
        assert!(h_at(200.0) < 250.0);
    }
}

// ---------- PDF pour l'impression ----------

/// Réglages d'un PDF d'impression
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PrintOpts {
    /// Fonds perdus (mm) : le dessin dépasse du format coupé
    pub bleed_mm: f32,
    /// Traits de coupe aux coins
    pub marks: bool,
    /// Couleurs en CMJN (DeviceCMYK) plutôt qu'en RVB
    pub cmyk: bool,
    pub dpi: f32,
}

impl Default for PrintOpts {
    fn default() -> PrintOpts {
        PrintOpts { bleed_mm: 3.0, marks: true, cmyk: true, dpi: 300.0 }
    }
}

/// RVB (0..1) vers CMJN (0..1), sans profil : retrait du noir simple
pub fn rgb_to_cmyk(r: f32, g: f32, b: f32) -> [f32; 4] {
    let k = 1.0 - r.max(g).max(b);
    if k >= 0.9999 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    [(1.0 - r - k) / (1.0 - k), (1.0 - g - k) / (1.0 - k), (1.0 - b - k) / (1.0 - k), k]
}

/// Le plan de travail `board` en PDF d'impression : image à `dpi`, fonds perdus, traits de coupe,
/// boîtes TrimBox et BleedBox (comme un PDF/X simple)
pub fn print_pdf(doc: &Doc, board: usize, root: &str, o: PrintOpts) -> Result<Vec<u8>, String> {
    use std::io::Write as _;
    let b = doc.boards.get(board).ok_or("plan introuvable")?;
    // Unités du document : px à 96 par pouce ; PDF : points à 72 par pouce
    let px_mm = 96.0 / 25.4;
    let bleed = o.bleed_mm.max(0.0) * px_mm;
    let slug = if o.marks { 10.0 * px_mm } else { 0.0 };
    let area = [b.x - bleed, b.y - bleed, b.w + 2.0 * bleed, b.h + 2.0 * bleed];
    let k = o.dpi / 96.0;
    let (iw, ih) = (((area[2] * k).round() as u32).clamp(1, 20000), ((area[3] * k).round() as u32).clamp(1, 20000));
    let svg = to_svg(doc, area, [iw, ih], root, true);
    let px = rasterize(&svg, iw, ih).ok_or("rendu impossible")?;
    // Posé sur du blanc (le papier), puis en CMJN ou RVB
    let mut raw = Vec::with_capacity((iw * ih) as usize * if o.cmyk { 4 } else { 3 });
    for c in px.chunks_exact(4) {
        let a = c[3] as f32 / 255.0;
        let rgb = [0, 1, 2].map(|i| (c[i] as f32 / 255.0) * a + (1.0 - a));
        if o.cmyk {
            raw.extend(rgb_to_cmyk(rgb[0], rgb[1], rgb[2]).map(|v| (v * 255.0).round() as u8));
        } else {
            raw.extend(rgb.map(|v| (v * 255.0).round() as u8));
        }
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&raw).map_err(|e| e.to_string())?;
    let data = z.finish().map_err(|e| e.to_string())?;
    let pt = 0.75f32;
    let (tw, th, bl, sl) = (b.w * pt, b.h * pt, bleed * pt, slug * pt);
    let (pw, ph) = (tw + 2.0 * (bl + sl), th + 2.0 * (bl + sl));
    // Contenu : l'image, puis les traits de coupe dans la marge (en noir de repérage)
    let mut cs = format!("q {:.3} 0 0 {:.3} {:.3} {:.3} cm /Im0 Do Q\n", tw + 2.0 * bl, th + 2.0 * bl, sl, sl);
    if o.marks {
        let (x0, y0, x1, y1) = (sl + bl, sl + bl, sl + bl + tw, sl + bl + th);
        let (gap, len) = (bl + 3.0, (sl - 3.0).max(4.0));
        cs += if o.cmyk { "0.25 w 1 1 1 1 K\n" } else { "0.25 w 0 0 0 RG\n" };
        for (x, y, sx, sy) in [(x0, y0, -1.0, -1.0), (x1, y0, 1.0, -1.0), (x0, y1, -1.0, 1.0), (x1, y1, 1.0, 1.0)] {
            cs += &format!("{:.3} {:.3} m {:.3} {:.3} l S\n", x + sx * gap, y, x + sx * (gap + len), y);
            cs += &format!("{:.3} {:.3} m {:.3} {:.3} l S\n", x, y + sy * gap, x, y + sy * (gap + len));
        }
    }
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offs = vec![];
    let mut obj = |out: &mut Vec<u8>, body: &[u8]| {
        offs.push(out.len());
        out.extend(format!("{} 0 obj\n", offs.len()).as_bytes());
        out.extend(body);
        out.extend(b"\nendobj\n");
    };
    obj(&mut out, b"<< /Type /Catalog /Pages 2 0 R >>");
    obj(&mut out, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    let page = format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.3} {ph:.3}] /BleedBox [{:.3} {:.3} {:.3} {:.3}] /TrimBox [{:.3} {:.3} {:.3} {:.3}] /Resources << /XObject << /Im0 5 0 R >> >> /Contents 4 0 R >>",
        sl, sl, pw - sl, ph - sl, sl + bl, sl + bl, pw - sl - bl, ph - sl - bl
    );
    obj(&mut out, page.as_bytes());
    obj(&mut out, format!("<< /Length {} >>\nstream\n{cs}endstream", cs.len()).as_bytes());
    let mut img = format!("<< /Type /XObject /Subtype /Image /Width {iw} /Height {ih} /ColorSpace /{} /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n", if o.cmyk { "DeviceCMYK" } else { "DeviceRGB" }, data.len()).into_bytes();
    img.extend(&data);
    img.extend(b"\nendstream");
    obj(&mut out, &img);
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offs.len() + 1).as_bytes());
    for o in &offs {
        out.extend(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offs.len() + 1).as_bytes());
    Ok(out)
}

#[cfg(test)]
mod print_tests {
    use super::*;
    #[test]
    fn pdf_impression() {
        assert_eq!(rgb_to_cmyk(0.0, 0.0, 0.0), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(rgb_to_cmyk(1.0, 0.0, 0.0), [0.0, 1.0, 1.0, 0.0]);
        let mut d = Doc::starter();
        d.boards[0].w = 400.0;
        d.boards[0].h = 300.0;
        let pdf = print_pdf(&d, 0, "/tmp", PrintOpts { dpi: 72.0, ..PrintOpts::default() }).unwrap();
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.starts_with("%PDF-1.4") && s.contains("/TrimBox [") && s.contains("/DeviceCMYK") && s.trim_end().ends_with("%%EOF"));
        // TrimBox = le format coupé : 400 px = 300 pt
        let tb = s.split("/TrimBox [").nth(1).unwrap().split(']').next().unwrap();
        let v: Vec<f32> = tb.split_whitespace().map(|x| x.parse().unwrap()).collect();
        assert!((v[2] - v[0] - 300.0).abs() < 0.01 && (v[3] - v[1] - 225.0).abs() < 0.01, "{v:?}");
        if std::env::var("ARY_DEMO").is_ok() {
            std::fs::write("/tmp/print.pdf", &pdf).unwrap();
        }
    }
}
/// Ce qui change entre deux états d'un dessin : noms des formes ajoutées, retirées, modifiées, et plans de travail
#[derive(Debug, Default, PartialEq)]
pub struct DocDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
    /// Plans de travail : en plus, en moins, modifiés
    pub boards: [usize; 3],
}

impl DocDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty() && self.boards == [0; 3]
    }
}

/// Compare deux états d'un dessin, forme par forme (d'après leur identifiant)
pub fn doc_diff(old: &Doc, new: &Doc) -> DocDiff {
    let label = |s: &Shape| if s.name.is_empty() { format!("{} {}", crate::i18n::t(s.kind_label()), s.id) } else { s.name.clone() };
    let mut d = DocDiff::default();
    for s in &new.shapes {
        match old.get(s.id) {
            None => d.added.push(label(s)),
            Some(o) if o != s => d.changed.push(label(s)),
            _ => {}
        }
    }
    for s in &old.shapes {
        if new.get(s.id).is_none() {
            d.removed.push(label(s));
        }
    }
    let same = old.boards.iter().zip(&new.boards).filter(|(a, b)| a == b).count();
    let common = old.boards.len().min(new.boards.len());
    d.boards = [new.boards.len().saturating_sub(common), old.boards.len().saturating_sub(common), common - same];
    d
}

/// Image des différences entre deux rendus de même taille : le rendu `b` pâli, et en rose ce qui a changé
pub fn image_diff(a: &image::RgbaImage, b: &image::RgbaImage) -> image::RgbaImage {
    let mut out = b.clone();
    for (x, y, p) in out.enumerate_pixels_mut() {
        let q = if x < a.width() && y < a.height() { a.get_pixel(x, y).0 } else { [0; 4] };
        let diff = (0..4).map(|k| (p.0[k] as i32 - q[k] as i32).abs()).max().unwrap_or(0);
        if diff > 24 {
            p.0 = [236, 64, 122, 255];
        } else {
            // Pâli vers le blanc : le changement ressort
            let al = p.0[3] as f32 / 255.0;
            for k in 0..3 {
                p.0[k] = (255.0 - (255.0 - p.0[k] as f32 * al - 255.0 * (1.0 - al)) * 0.35) as u8;
            }
            p.0[3] = 255;
        }
    }
    out
}

/// Rendu du premier plan de travail d'un dessin donné en texte (une version de l'historique), `max` px de large au plus
pub fn render_text(json: &str, root: &str, max: u32) -> Option<(Doc, image::RgbaImage)> {
    let doc: Doc = serde_json::from_str(json).ok()?;
    let b = doc.boards.first()?;
    let scale = (max as f32 / b.w.max(b.h).max(1.0)).min(2.0);
    let img = export_png(&doc, 0, root, scale)?;
    Some((doc, img))
}

#[cfg(test)]
mod abime_tests {
    use super::*;
    #[test]
    fn fichier_abime_sans_panique() {
        // Un dessin dont un nombre a été abîmé (taille de 1e30, position infinie, plan de travail démesuré)
        let mut d = Doc::starter();
        let (bx, by) = (d.boards[0].x, d.boards[0].y);
        let id = d.new_id();
        d.shapes.push(Shape { id, kind: Kind::Ellipse { w: 300.0, h: 1e30 }, x: bx + 50.0, y: by + 50.0, fills: vec![Paint::solid([255, 0, 0, 255])], ..Shape::base() });
        let id = d.new_id();
        d.shapes.push(Shape { id, kind: Kind::Rect { w: 80.0, h: 80.0, r: [0.0; 4] }, x: f32::INFINITY, y: by, fills: vec![Paint::solid([0, 0, 255, 255])], ..Shape::base() });
        let id = d.new_id();
        d.shapes.push(Shape { id, kind: Kind::Rect { w: 80.0, h: 80.0, r: [0.0; 4] }, x: bx + 10.0, y: by + 10.0, fills: vec![Paint::solid([0, 160, 0, 255])], ..Shape::base() });
        assert!(!sane(&d.shapes[d.shapes.len() - 3]) && !sane(&d.shapes[d.shapes.len() - 2]) && sane(&d.shapes[d.shapes.len() - 1]));
        // Les formes saines sont rendues, les autres ignorées
        let img = export_png(&d, 0, "/tmp", 0.5).unwrap();
        assert_eq!(img.get_pixel(25, 25).0, [0, 160, 0, 255]);
        // Un plan de travail démesuré : pas d'image, pas de mémoire épuisée
        d.boards[0].w = 4e9;
        d.boards[0].h = 4e9;
        assert!(export_png(&d, 0, "/tmp", 1.0).is_none());
    }
}

#[cfg(test)]
mod diff_tests {
    use super::*;
    #[test]
    fn comparer_deux_versions() {
        let mut a = Doc::starter();
        a.shapes.clear();
        let (bx, by) = (a.boards[0].x, a.boards[0].y);
        for (k, name) in ["soleil", "nuage", "colline"].iter().enumerate() {
            let id = a.new_id();
            a.shapes.push(Shape { id, name: name.to_string(), kind: Kind::Ellipse { w: 120.0, h: 120.0 }, x: bx + 60.0 + 200.0 * k as f32, y: by + 80.0, fills: vec![Paint::solid([240, 180, 60, 255])], ..Shape::base() });
        }
        let mut b = a.clone();
        assert!(doc_diff(&a, &b).is_empty());
        b.shapes[0].x += 300.0;
        let gone = b.shapes[1].id;
        b.remove(&[gone]);
        let id = b.new_id();
        b.shapes.push(Shape { id, name: "oiseau".into(), kind: Kind::Rect { w: 40.0, h: 20.0, r: [0.0; 4] }, x: bx + 500.0, y: by + 400.0, fills: vec![Paint::solid([40, 40, 48, 255])], ..Shape::base() });
        b.boards.push(b.boards[0].clone());
        let d = doc_diff(&a, &b);
        assert_eq!((d.added, d.removed, d.changed, d.boards), (vec!["oiseau".to_string()], vec!["nuage".to_string()], vec!["soleil".to_string()], [1, 0, 0]));
        // Les rendus des deux versions, et l'image de ce qui a changé
        let ja = serde_json::to_string(&a).unwrap();
        let (_, ia) = render_text(&ja, "/tmp", 300).unwrap();
        let (_, ib) = render_text(&serde_json::to_string(&b).unwrap(), "/tmp", 300).unwrap();
        assert_eq!(ia.dimensions(), ib.dimensions());
        let di = image_diff(&ia, &ib);
        let rose = di.pixels().filter(|p| p.0 == [236, 64, 122, 255]).count();
        assert!(rose > 200, "le soleil déplacé, le nuage parti et l'oiseau se voient : {rose}");
        assert_eq!(image_diff(&ia, &ia).pixels().filter(|p| p.0 == [236, 64, 122, 255]).count(), 0);
        assert!(render_text("pas du json", "/tmp", 300).is_none());
    }
}

#[cfg(test)]
mod mesh_tests {
    use super::*;
    #[test]
    fn filet_de_degrade() {
        let Paint::Mesh { cols, rows, mut pts } = mesh_new(2, 2, [255, 0, 0, 255], [0, 0, 255, 255]) else { panic!() };
        assert!(mesh_ok(cols, rows, &pts));
        // La courbe passe par chaque nœud, et une grille régulière reste régulière entre les nœuds
        for j in 0..=2 {
            for i in 0..=2 {
                let (p, c) = mesh_at(cols, rows, &pts, i as f32, j as f32);
                let (q, d) = pts[j * 3 + i];
                assert!((p[0] - q[0]).abs() < 1e-5 && (p[1] - q[1]).abs() < 1e-5);
                assert!((0..4).all(|k| (c[k] - d[k] as f32).abs() < 0.01));
            }
        }
        let (p, _) = mesh_at(cols, rows, &pts, 0.25, 1.7);
        assert!((p[0] - 0.125).abs() < 1e-4 && (p[1] - 0.85).abs() < 1e-4, "{p:?}");
        // Un nœud du milieu déplacé et recoloré tire les carreaux voisins, sans sortir des couleurs possibles
        pts[4] = ([0.7, 0.4], [255, 255, 255, 255]);
        let q = mesh_quads(cols, rows, &pts, 8);
        assert_eq!(q.len(), 16 * 16);
        assert!(q.iter().any(|(_, c)| c[1] > 200), "le blanc du milieu se voit");
        assert_eq!(q[0].1[3], 255);
        // Une autre grille garde l'allure : mêmes coins, même milieu
        let Paint::Mesh { cols: c2, rows: r2, pts: p2 } = mesh_resize(cols, rows, &pts, 4, 4) else { panic!() };
        assert!(mesh_ok(c2, r2, &p2));
        assert_eq!(p2[0], pts[0]);
        assert_eq!(p2[24], pts[8]);
        assert_eq!(p2[12], pts[4]);
        // Un filet abîmé (fichier modifié à la main) ne fait rien paniquer
        assert!(mesh_quads(3, 3, &pts, 4).is_empty());
        let mut far = pts.clone();
        far[2].0 = [1.0, -1e9];
        assert!(mesh_quads(cols, rows, &far, 4).iter().all(|(q, _)| q.iter().all(|p| p[0].abs() < 10.0 && p[1].abs() < 10.0)));
        assert_eq!(mesh_at(3, 3, &pts, 1.0, 1.0).1, [0.0; 4]);
        // Rendu : la forme porte bien plusieurs couleurs, et le SVG reste lisible
        let mut d = Doc::starter();
        d.shapes.clear();
        let (bx, by) = (d.boards[0].x, d.boards[0].y);
        let id = d.new_id();
        d.shapes.push(Shape { id, kind: Kind::Ellipse { w: 600.0, h: 420.0 }, x: bx + 100.0, y: by + 100.0, fills: vec![Paint::Mesh { cols, rows, pts }], ..Shape::base() });
        let img = export_png(&d, 0, "/tmp", 0.5).unwrap();
        let at = |x: f32, y: f32| img.get_pixel(((100.0 + x) * 0.5) as u32, ((100.0 + y) * 0.5) as u32).0;
        assert!(at(420.0, 168.0)[1] > 200, "blanc au nœud déplacé : {:?}", at(420.0, 168.0));
        assert!(at(80.0, 150.0)[0] > at(80.0, 150.0)[2], "rouge à gauche");
        assert!(at(520.0, 300.0)[2] > at(520.0, 300.0)[0], "bleu à droite");
        if let Ok(out) = std::env::var("ARY_OUT") {
            img.save(out).unwrap();
        }
    }
}

#[cfg(test)]
mod freeform_tests {
    use super::*;
    #[test]
    fn points_libres() {
        let pts = vec![([0.0, 0.0], [255, 0, 0, 255]), ([1.0, 1.0], [0, 0, 255, 255])];
        assert_eq!(freeform_at(&pts, 0.0, 0.0, 1.0), [255, 0, 0, 255]);
        let mid = freeform_at(&pts, 0.5, 0.5, 1.0);
        assert!(mid[0] > 100 && mid[2] > 100);
        let mut d = Doc::starter();
        d.shapes.clear();
        let id = d.new_id();
        let (bx, by) = (d.boards[0].x, d.boards[0].y);
        d.shapes.push(Shape { id, kind: Kind::Ellipse { w: 600.0, h: 420.0 }, x: bx + 240.0, y: by + 120.0, fills: vec![Paint::Freeform { pts: vec![([0.2, 0.3], [236, 108, 89, 255]), ([0.8, 0.2], [244, 191, 80, 255]), ([0.7, 0.8], [84, 128, 220, 255]), ([0.25, 0.75], [150, 110, 210, 255]), ([0.5, 0.5], [255, 255, 255, 255])] }], ..Shape::base() });
        let id = d.new_id();
        d.shapes.push(Shape { id, kind: Kind::Text { text: "Aurore".into(), size: 190.0, font: "inter-bold".into(), align: 0, width: None, spacing: 0.0, leading: 1.2 }, x: bx + 220.0, y: by + 600.0, fills: vec![Paint::Freeform { pts: vec![([0.0, 0.5], [86, 166, 122, 255]), ([0.5, 0.0], [84, 128, 220, 255]), ([1.0, 0.6], [236, 108, 89, 255])] }], ..Shape::base() });
        let img = export_png(&d, 0, "/tmp", 0.6).unwrap();
        // Au centre de l'ellipse : presque blanc ; vers la gauche : plutôt rouge
        let c = img.get_pixel(((240.0 + 300.0) * 0.6) as u32, ((120.0 + 210.0) * 0.6) as u32);
        assert!(c[0] > 170 && c[1] > 150 && c[2] > 150, "{c:?}");
        if std::env::var("ARY_DEMO").is_ok() {
            img.save("/tmp/freeform.png").unwrap();
        }
    }
}
#[cfg(test)]
mod brush_tests {
    use super::*;
    #[test]
    fn pinceaux() {
        let mut d = Doc::starter();
        d.shapes.clear();
        let (bx, by) = (d.boards[0].x, d.boards[0].y);
        for (k, brush) in [0u8, 1, 2, 3].into_iter().enumerate() {
            let id = d.new_id();
            let y = 140.0 + k as f32 * 220.0;
            let nodes = vec![Node { p: [100.0, y], i: [100.0, y], o: [300.0, y - 160.0] }, Node { p: [600.0, y], i: [450.0, y + 160.0], o: [700.0, y - 80.0] }, Node { p: [950.0, y], i: [850.0, y - 100.0], o: [950.0, y] }];
            d.shapes.push(Shape { id, kind: Kind::Path { nodes, closed: false }, x: bx, y: by, strokes: vec![Stroke { paint: Paint::solid([30, 30, 36, 255]), width: 26.0, cap: 1, join: 1, dash: vec![], arrow: false, brush }], ..Shape::base() });
        }
        let img = export_png(&d, 0, "/tmp", 0.6).unwrap();
        let ink = |y0: u32, y1: u32| (y0..y1).flat_map(|y| (0..img.width()).map(move |x| (x, y))).filter(|&(x, y)| img.get_pixel(x, y)[0] < 128).count();
        // Chaque pinceau dépose de l'encre ; l'effilé moins que l'uniforme
        let bands: Vec<usize> = (0..4).map(|k| ink(k * 132, k * 132 + 132)).collect();
        assert!(bands.iter().all(|&n| n > 200), "{bands:?}");
        assert!(bands[2] < bands[0], "{bands:?}");
        if std::env::var("ARY_DEMO").is_ok() {
            img.save("/tmp/brushes.png").unwrap();
        }
    }
}
#[cfg(test)]
mod fx_tests2 {
    use super::*;
    #[test]
    fn ombre_interne_et_masque_d_opacite() {
        let mut d = Doc::starter();
        d.shapes.clear();
        let (bx, by) = (d.boards[0].x, d.boards[0].y);
        let id = d.new_id();
        d.shapes.push(Shape { id, kind: Kind::Rect { w: 420.0, h: 240.0, r: [40.0; 4] }, x: bx + 80.0, y: by + 80.0, fills: vec![Paint::solid([236, 236, 240, 255])], shadow: Some(Shadow { dx: 0.0, dy: 14.0, blur: 24.0, c: [0, 0, 0, 160], inner: true }), ..Shape::base() });
        // Masque d'opacité : un dégradé blanc vers noir sous un grand rectangle orange
        let m = d.new_id();
        d.shapes.push(Shape { id: m, kind: Kind::Rect { w: 800.0, h: 300.0, r: [0.0; 4] }, x: bx + 80.0, y: by + 500.0, fills: vec![Paint::Linear { stops: vec![(0.0, [255, 255, 255, 255]), (1.0, [0, 0, 0, 255])], angle: 0.0 }], ..Shape::base() });
        let t = d.new_id();
        d.shapes.push(Shape { id: t, kind: Kind::Rect { w: 800.0, h: 300.0, r: [0.0; 4] }, x: bx + 80.0, y: by + 500.0, fills: vec![Paint::solid([236, 108, 89, 255])], ..Shape::base() });
        d.group(&[m, t], 7).unwrap();
        let img = export_png(&d, 0, "/tmp", 0.5).unwrap();
        // L'ombre interne fonce le haut du bouton, pas son centre
        let (top, mid) = (img.get_pixel(((80 + 210) / 2) as u32, ((80 + 12) / 2) as u32)[0], img.get_pixel(((80 + 210) / 2) as u32, ((80 + 160) / 2) as u32)[0]);
        assert!(top + 20 < mid, "{top} {mid}");
        // Le masque : à gauche bien orange, à droite presque le fond blanc
        let (l, r) = (img.get_pixel(60, 320), img.get_pixel(430, 320));
        assert!(l[1] < 150 && r[1] > 220, "{l:?} {r:?}");
        if std::env::var("ARY_DEMO").is_ok() {
            img.save("/tmp/fx2.png").unwrap();
        }
    }
}

/// Les cadres d'appareils proposés (dans l'ordre de `Board::device`, à partir de 1)
pub const DEVICES: [&str; 3] = ["iPhone", "Android", "écran"];

/// La coque d'un appareil autour d'un plan (SVG, sous le plan)
pub fn device_frame(b: &Board) -> String {
    let (x, y, w, h) = (b.x, b.y, b.w, b.h);
    match b.device {
        1 | 2 => {
            let (e, r) = if b.device == 1 { (14.0, 62.0) } else { (10.0, 40.0) };
            format!(r##"<rect x="{}" y="{}" width="{}" height="{}" rx="{r}" fill="#1b1b1f"/><rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="none" stroke="#3a3a40" stroke-width="2"/>"##, x - e, y - e, w + 2.0 * e, h + 2.0 * e, x - e + 1.0, y - e + 1.0, w + 2.0 * e - 2.0, h + 2.0 * e - 2.0, r - 1.0)
        }
        _ => {
            let e = (w * 0.012).max(10.0);
            let (sw, sh) = (w * 0.18, h * 0.12);
            format!(r##"<rect x="{}" y="{}" width="{}" height="{}" fill="#2a2a2e"/><rect x="{}" y="{}" width="{}" height="{}" rx="6" fill="#1b1b1f"/><rect x="{}" y="{}" width="{}" height="{}" fill="#2a2a2e"/>"##, x + w / 2.0 - sw / 6.0, y + h + e, sw / 3.0, sh, x - e, y - e, w + 2.0 * e, h + 3.0 * e, x + w / 2.0 - sw / 2.0, y + h + e + sh, sw, e)
        }
    }
}

/// La barre d'état et l'encoche d'un téléphone (SVG, par-dessus le contenu du plan)
pub fn device_overlay(b: &Board) -> String {
    if b.device != 1 && b.device != 2 {
        return String::new();
    }
    let (x, y, w) = (b.x, b.y, b.w);
    let k = (w / 393.0).clamp(0.5, 3.0);
    let bar = 47.0 * k;
    let ink = b.bg.map(|c| if (c[0] as u32 + c[1] as u32 + c[2] as u32) < 380 { "#ffffff" } else { "#111111" }).unwrap_or("#111111");
    let mut s = String::new();
    // L'heure, à gauche
    // (en tracés, comme tous les textes : le rendu n'a pas besoin des polices du système)
    let clock = Shape { kind: Kind::Text { text: "9:41".into(), size: 17.0 * k, font: "inter-semibold".into(), align: 0, width: None, spacing: 0.0, leading: 1.2 }, ..Shape::base() };
    let (d, tb) = text_outline(&clock);
    let _ = write!(s, r#"<path transform="translate({} {})" d="{d}" fill="{ink}"/>"#, x + w * 0.16 - (tb[2] - tb[0]) / 2.0 - tb[0], y + bar * 0.5 - (tb[3] - tb[1]) / 2.0 - tb[1]);
    // Réseau, wifi et batterie, à droite
    let (rx, cy) = (x + w - 22.0 * k, y + bar * 0.5);
    let _ = write!(s, r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="none" stroke="{ink}" stroke-opacity="0.45" stroke-width="{}"/><rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{ink}"/>"#, rx - 25.0 * k, cy - 6.5 * k, 25.0 * k, 13.0 * k, 4.0 * k, k, rx - 23.0 * k, cy - 4.5 * k, 19.0 * k, 9.0 * k, 2.5 * k);
    for i in 0..4 {
        let hh = (4.0 + i as f32 * 2.5) * k;
        let _ = write!(s, r#"<rect x="{}" y="{}" width="{}" height="{hh}" rx="{}" fill="{ink}"/>"#, rx - 70.0 * k + i as f32 * 5.0 * k, cy + 5.0 * k - hh, 3.2 * k, k);
    }
    let _ = write!(s, r#"<path d="M{} {}a{r} {r} 0 0 1 {} 0" fill="none" stroke="{ink}" stroke-width="{}" stroke-linecap="round"/><circle cx="{}" cy="{}" r="{}" fill="{ink}"/>"#, rx - 44.0 * k, cy + 1.0 * k, 12.0 * k, 2.2 * k, rx - 38.0 * k, cy + 3.5 * k, 1.8 * k, r = 6.0 * k);
    // L'encoche (îlot sur iPhone, poinçon sur Android)
    if b.device == 1 {
        let _ = write!(s, r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="black"/>"#, x + w / 2.0 - 63.0 * k, y + 11.0 * k, 126.0 * k, 37.0 * k, 18.5 * k);
        // La barre d'accueil, en bas
        let _ = write!(s, r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{ink}"/>"#, x + w / 2.0 - 67.0 * k, y + b.h - 13.0 * k, 134.0 * k, 5.0 * k, 2.5 * k);
    } else {
        let _ = write!(s, r#"<circle cx="{}" cy="{}" r="{}" fill="black"/>"#, x + w / 2.0, y + 22.0 * k, 9.0 * k);
    }
    s
}
#[cfg(test)]
mod device_tests {
    use super::*;
    #[test]
    fn cadres_d_appareil() {
        let mut d = Doc::starter();
        d.shapes.clear();
        d.boards[0] = Board { name: "Écran".into(), x: 0.0, y: 0.0, w: 393.0, h: 852.0, bg: Some([246, 242, 238, 255]), grid: None, persp: None, device: 1, mode: 0 };
        let id = d.new_id();
        d.shapes.push(Shape { id, kind: Kind::Rect { w: 345.0, h: 220.0, r: [24.0; 4] }, x: 24.0, y: 120.0, fills: vec![Paint::solid([236, 108, 89, 255])], ..Shape::base() });
        let svg = to_svg(&d, [-40.0, -40.0, 473.0, 932.0], [473, 932], "/tmp", true);
        assert!(svg.contains("#1b1b1f") && svg.contains(r#"fill="black""#));
        if std::env::var("ARY_DEMO").is_ok() {
            let px = rasterize(&svg, 473, 932).unwrap();
            image::RgbaImage::from_raw(473, 932, px).unwrap().save("/tmp/device.png").unwrap();
        }
    }
}

// ---------- Bibliothèque partagée (comme Figma) : les composants des autres dessins du coffre ----------

/// Un composant publié par un autre dessin du coffre
#[derive(Clone, Debug, PartialEq)]
pub struct LibItem {
    /// Fichier du coffre (relatif)
    pub file: String,
    pub name: String,
    pub id: u64,
}

/// Les composants de tous les dessins du coffre sauf `except` (le dessin ouvert)
pub fn library(root: &str, except: &Path) -> Vec<LibItem> {
    let mut out = vec![];
    let base = Path::new(root);
    let mut stack = vec![base.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if is_vector(&name) && p != except {
                let Ok(doc) = Doc::load(&p) else { continue };
                let rel = p.strip_prefix(base).map(|r| r.to_string_lossy().to_string()).unwrap_or(name.clone());
                for s in doc.shapes.iter().filter(|s| s.component && s.lib.is_empty()) {
                    out.push(LibItem { file: rel.clone(), name: s.name.clone(), id: s.id });
                }
            }
        }
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.name.cmp(&b.name)));
    out
}

impl Doc {
    /// Copie la forme `id` de `src` (et son contenu) dans ce document ; rend l'identifiant de la copie
    pub fn import_tree(&mut self, src: &Doc, id: u64, parent: u64) -> Option<u64> {
        let s = src.get(id)?.clone();
        let nid = self.new_id();
        let mut c = s.clone();
        c.id = nid;
        c.parent = parent;
        c.instance_of = 0;
        if let Kind::Group { children, op } = &s.kind {
            let kids: Vec<u64> = children.iter().filter_map(|k| self.import_tree(src, *k, nid)).collect();
            c.kind = Kind::Group { children: kids, op: *op };
        }
        self.shapes.push(c);
        Some(nid)
    }

    /// Pose un composant de la bibliothèque : son maître (copié, gardant le lien) à droite des plans,
    /// et une instance à `at`. Si le maître est déjà là, seule une instance s'ajoute. Rend l'instance.
    pub fn place_from_library(&mut self, root: &str, item: &LibItem, at: [f32; 2]) -> Result<u64, String> {
        let key = format!("{}#{}", item.file, item.name);
        let mid = match self.shapes.iter().find(|s| s.lib == key).map(|s| s.id) {
            Some(m) => m,
            None => {
                let src = Doc::load(&Path::new(root).join(&item.file))?;
                let m = self.import_tree(&src, item.id, 0).ok_or("composant introuvable")?;
                // Le maître se range à droite de tout, hors des plans de travail
                let right = self.boards.iter().map(|b| b.x + b.w).fold(0.0f32, f32::max);
                let top = self.boards.iter().map(|b| b.y).fold(f32::MAX, f32::min).min(0.0);
                let b = self.get(m).map(|s| self.bounds(s)).unwrap_or([0.0; 4]);
                self.translate(m, right + 200.0 - b[0], top - b[1]);
                if let Some(s) = self.shapes.iter_mut().find(|s| s.id == m) {
                    s.component = true;
                    s.lib = key.clone();
                }
                m
            }
        };
        let iid = self.instantiate(mid).ok_or("composant introuvable")?;
        let b = self.get(iid).map(|s| self.bounds(s)).unwrap_or([0.0; 4]);
        self.translate(iid, at[0] - (b[0] + b[2]) / 2.0, at[1] - (b[1] + b[3]) / 2.0);
        Ok(iid)
    }

    /// Met à jour les composants venus de la bibliothèque (leurs instances suivent) ; rend le nombre mis à jour
    pub fn update_library(&mut self, root: &str) -> usize {
        let masters: Vec<(u64, String)> = self.shapes.iter().filter(|s| !s.lib.is_empty() && s.component).map(|s| (s.id, s.lib.clone())).collect();
        let mut n = 0;
        for (mid, key) in masters {
            let Some((file, name)) = key.split_once('#') else { continue };
            let Ok(src) = Doc::load(&Path::new(root).join(file)) else { continue };
            let Some(sid) = src.shapes.iter().find(|s| s.component && s.name == name).map(|s| s.id) else { continue };
            let Some(old) = self.get(mid).cloned() else { continue };
            // Le nouveau contenu, posé à la place de l'ancien
            let Some(fresh) = self.import_tree(&src, sid, old.parent) else { continue };
            let (ob, nb) = (self.bounds(&old), self.get(fresh).map(|s| self.bounds(s)).unwrap_or([0.0; 4]));
            self.translate(fresh, ob[0] - nb[0], ob[1] - nb[1]);
            let pre = self.preorder(mid);
            let kids = self.get(fresh).map(|s| s.kind.clone());
            let fresh_pre = self.preorder(fresh);
            // L'identifiant du maître ne change pas : ses instances le retrouvent
            self.remove(&pre[1..].to_vec());
            if let (Some(k), Some(i)) = (kids, self.index(mid)) {
                let f = self.get(fresh).cloned().unwrap();
                let s = &mut self.shapes[i];
                s.kind = k;
                s.fills = f.fills;
                s.strokes = f.strokes;
                s.shadow = f.shadow;
                s.x = f.x;
                s.y = f.y;
            }
            for c in fresh_pre.iter().skip(1) {
                if let Some(i) = self.index(*c) {
                    if self.shapes[i].parent == fresh {
                        self.shapes[i].parent = mid;
                    }
                }
            }
            if let Some(i) = self.index(fresh) {
                self.shapes.remove(i);
            }
            n += 1;
        }
        if n > 0 {
            self.sync_instances();
        }
        n
    }
}

// ---------- Animation exportée : les plans à la suite, avec leurs transitions ----------

/// Les images de l'animation : chaque plan de la taille du premier reste `hold` secondes, puis glisse vers
/// le suivant en `trans` secondes (formes de même nom animées, comme en présentation)
pub fn anim_frames(doc: &Doc, root: &str, scale: f32, fps: f32, hold: f32, trans: f32) -> Vec<image::RgbaImage> {
    let Some(first) = doc.boards.first() else { return vec![] };
    let order: Vec<usize> = (0..doc.boards.len()).filter(|&i| (doc.boards[i].w - first.w).abs() < 0.5 && (doc.boards[i].h - first.h).abs() < 0.5).collect();
    let mut out = vec![];
    let n_hold = (hold * fps).round().max(1.0) as usize;
    let n_trans = (trans * fps).round().max(1.0) as usize;
    for (k, &a) in order.iter().enumerate() {
        let Some(img) = export_png(doc, a, root, scale) else { continue };
        for _ in 0..n_hold {
            out.push(img.clone());
        }
        if let Some(&b) = order.get(k + 1) {
            for j in 1..n_trans {
                if let Some(img) = smart_frame(doc, a, b, j as f32 / n_trans as f32).and_then(|d| export_png(&d, 0, root, scale)) {
                    out.push(img);
                }
            }
        }
    }
    out
}

/// GIF animé, en boucle
pub fn write_gif(frames: &[image::RgbaImage], fps: f32, out: &Path) -> Result<(), String> {
    use image::codecs::gif::{GifEncoder, Repeat};
    let f = std::fs::File::create(out).map_err(|e| e.to_string())?;
    let mut enc = GifEncoder::new_with_speed(std::io::BufWriter::new(f), 10);
    enc.set_repeat(Repeat::Infinite).map_err(|e| e.to_string())?;
    let delay = image::Delay::from_numer_denom_ms((1000.0 / fps).round() as u32, 1);
    for fr in frames {
        enc.encode_frame(image::Frame::from_parts(fr.clone(), 0, 0, delay)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Vidéo MP4 (H.264) par ffmpeg, les images passées en PNG dans un tuyau
pub fn write_mp4(frames: &[image::RgbaImage], fps: f32, out: &Path) -> Result<(), String> {
    use std::io::Write;
    let ff = crate::ffmpeg::find("ffmpeg").ok_or("ffmpeg n'est pas installé.")?;
    let mut child = crate::ffmpeg::command(ff)
        .args(["-y", "-v", "error", "-f", "image2pipe", "-framerate", &format!("{fps}"), "-i", "-", "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p", "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2", "-movflags", "+faststart"])
        .arg(out)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    {
        let mut si = child.stdin.take().ok_or("ffmpeg")?;
        for fr in frames {
            let mut buf = std::io::Cursor::new(vec![]);
            fr.write_to(&mut buf, image::ImageFormat::Png).map_err(|e| e.to_string())?;
            si.write_all(buf.get_ref()).map_err(|e| e.to_string())?;
        }
    }
    let o = child.wait_with_output().map_err(|e| e.to_string())?;
    if o.status.success() { Ok(()) } else { Err(format!("ffmpeg : {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("erreur"))) }
}

// ---------- Transitions animées entre plans (comme Smart Animate) ----------

/// Le document à l'instant `t` (0..1) du passage du plan `a` au plan `b` : les formes de même nom glissent
/// et changent de taille et d'opacité, les autres apparaissent ou s'effacent. Rend un document à un seul plan.
pub fn smart_frame(doc: &Doc, a: usize, b: usize, t: f32) -> Option<Doc> {
    let (ba, bb) = (doc.boards.get(a)?.clone(), doc.boards.get(b)?.clone());
    let t = t.clamp(0.0, 1.0);
    let e = t * t * (3.0 - 2.0 * t);
    let center = |d: &Doc, s: &Shape| {
        let r = d.bounds(s);
        [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
    };
    let on = |d: &Doc, bd: &Board| -> Vec<u64> {
        d.shapes.iter().filter(|s| s.parent == 0 && s.visible).filter(|s| {
            let c = center(d, s);
            c[0] >= bd.x && c[0] <= bd.x + bd.w && c[1] >= bd.y && c[1] <= bd.y + bd.h
        }).map(|s| s.id).collect()
    };
    let (ia, ib) = (on(doc, &ba), on(doc, &bb));
    let mut out = doc.clone();
    out.boards = vec![bb.clone()];
    let shift = |out: &mut Doc, id: u64, dx: f32, dy: f32| {
        for k in out.deep(&[id]) {
            if let Some(s) = out.shapes.iter_mut().find(|s| s.id == k) {
                s.x += dx;
                s.y += dy;
            }
        }
    };
    let fade = |out: &mut Doc, id: u64, f: f32| {
        if let Some(s) = out.shapes.iter_mut().find(|s| s.id == id) {
            s.opacity *= f;
        }
    };
    let mut matched_a = vec![];
    for &idb in &ib {
        let sb = doc.get(idb)?.clone();
        let twin = ia.iter().copied().find(|ida| doc.get(*ida).is_some_and(|sa| !sa.name.is_empty() && sa.name == sb.name));
        match twin {
            Some(ida) => {
                matched_a.push(ida);
                let sa = doc.get(ida)?;
                let (ca, cb) = (center(doc, sa), center(doc, &sb));
                // Où la forme était sur l'autre plan, ramené sur celui-ci
                let from = [ca[0] - ba.x + bb.x, ca[1] - ba.y + bb.y];
                shift(&mut out, idb, (from[0] - cb[0]) * (1.0 - e), (from[1] - cb[1]) * (1.0 - e));
                if let Some(s) = out.shapes.iter_mut().find(|s| s.id == idb) {
                    s.opacity = sa.opacity + (sb.opacity - sa.opacity) * e;
                    s.rot = sa.rot + (sb.rot - sa.rot) * e;
                    // Taille : pour les rectangles et ellipses, gardée centrée
                    if let (Kind::Rect { w: wa, h: ha, .. } | Kind::Ellipse { w: wa, h: ha }, Kind::Rect { w, h, .. } | Kind::Ellipse { w, h }) = (&sa.kind, &mut s.kind) {
                        let (nw, nh) = (wa + (*w - wa) * e, ha + (*h - ha) * e);
                        s.x += (*w - nw) / 2.0;
                        s.y += (*h - nh) / 2.0;
                        (*w, *h) = (nw, nh);
                    }
                    if let (Some(fa), Some(Paint::Solid { c })) = (sa.fills.first().map(|f| f.first()), s.fills.first_mut()) {
                        *c = [0, 1, 2, 3].map(|i| (fa[i] as f32 + (c[i] as f32 - fa[i] as f32) * e).round() as u8);
                    }
                }
            }
            None => fade(&mut out, idb, e),
        }
    }
    for &ida in &ia {
        if matched_a.contains(&ida) {
            if let Some(s) = out.shapes.iter_mut().find(|s| s.id == ida) {
                s.visible = false;
            }
            continue;
        }
        // Ce qui n'existe que sur le premier plan s'efface sur place
        shift(&mut out, ida, bb.x - ba.x, bb.y - ba.y);
        fade(&mut out, ida, 1.0 - e);
    }
    Some(out)
}

#[cfg(test)]
mod smart_tests {
    use super::*;
    #[test]
    fn transition_animee() {
        let mut d = Doc::starter();
        d.shapes.clear();
        d.boards.push(Board { name: "Détail".into(), x: 1200.0, y: 0.0, w: 1080.0, h: 1080.0, bg: Some([255; 4]), grid: None, persp: None, device: 0, mode: 0 });
        for (bx, x, w) in [(0.0, 100.0, 200.0), (1200.0, 600.0, 400.0)] {
            let id = d.new_id();
            d.shapes.push(Shape { id, name: "Carte".into(), kind: Kind::Rect { w, h: 200.0, r: [0.0; 4] }, x: bx + x, y: 100.0, fills: vec![Paint::solid([200, 0, 0, 255])], ..Shape::base() });
        }
        let half = smart_frame(&d, 0, 1, 0.5).unwrap();
        let card = half.shapes.iter().find(|s| s.visible && s.x >= 1200.0).unwrap();
        // À mi-chemin : entre les deux places et les deux tailles
        let Kind::Rect { w, .. } = card.kind else { panic!() };
        assert!(w > 200.0 && w < 400.0, "{w}");
        let cx = card.x + w / 2.0 - 1200.0;
        assert!(cx > 200.0 && cx < 800.0, "{cx}");
        let end = smart_frame(&d, 0, 1, 1.0).unwrap();
        assert_eq!(end.shapes.iter().filter(|s| s.visible && s.x >= 1200.0).count(), 1);
    }
}
