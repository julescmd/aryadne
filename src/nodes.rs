//! Systèmes de nodes : matériaux (pour la visionneuse 3D) et compositing (pour les images).
//!
//! Un graphe est un fichier JSON du coffre (`*.mat.json` ou `*.comp.json`). Chaque node a des
//! entrées (reliées ou réglées à la main), des sorties et des réglages. Le node « shader »
//! exécute un petit langage d'expressions écrit par l'utilisateur, pixel par pixel.

use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, RichText, Sense, Stroke};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

pub type V = [f32; 4];

pub fn is_graph(rel: &str) -> bool {
    let l = rel.to_lowercase();
    l.ends_with(".mat.json") || l.ends_with(".comp.json")
}

// ---------- définitions des nodes ----------

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum GraphKind {
    Material,
    Compositor,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Sock {
    Float,
    Color,
}

#[derive(Clone, Copy)]
pub enum ParamDef {
    Choice(&'static str, &'static [&'static str]),
    Float(&'static str, f32, f32, f32),
    Text(&'static str),
    File(&'static str),
}

pub struct Def {
    pub op: &'static str,
    pub label: &'static str,
    pub cat: &'static str,
    pub inputs: &'static [(&'static str, Sock, V)],
    pub outputs: &'static [(&'static str, Sock)],
    pub params: &'static [ParamDef],
    /// Matériau, compositing ou les deux
    pub mat: bool,
    pub comp: bool,
}

const W: V = [1.0, 1.0, 1.0, 1.0];
const BLACK: V = [0.0, 0.0, 0.0, 1.0];
const GRAY: V = [0.5, 0.5, 0.5, 1.0];
const fn f(x: f32) -> V {
    [x, x, x, x]
}

const BLEND: &[&str] = &["mélange", "multiplier", "écran", "incrustation", "ajouter", "soustraire", "différence", "éclaircir", "assombrir", "diviser"];
const MATH: &[&str] = &["ajouter", "soustraire", "multiplier", "diviser", "puissance", "minimum", "maximum", "modulo", "sinus", "cosinus", "absolu", "arrondi inf.", "fraction", "racine", "inférieur à", "supérieur à", "lisse (smoothstep)"];

pub const DEFS: &[Def] = &[
    // Sorties
    Def { op: "mat_out", label: "Sortie matériau", cat: "sortie", inputs: &[("couleur", Sock::Color, [0.8, 0.5, 0.4, 1.0]), ("rugosité", Sock::Float, f(0.5)), ("métal", Sock::Float, f(0.0)), ("émission", Sock::Color, BLACK), ("transmission", Sock::Float, f(0.0)), ("indice de réfraction", Sock::Float, f(1.45)), ("vernis", Sock::Float, f(0.0)), ("bandes toon", Sock::Float, f(0.0)), ("reflets", Sock::Float, f(1.0))], outputs: &[], params: &[], mat: true, comp: false },
    Def { op: "comp_out", label: "Sortie", cat: "sortie", inputs: &[("image", Sock::Color, GRAY)], outputs: &[], params: &[], mat: false, comp: true },
    // Entrées
    Def { op: "image", label: "Image", cat: "entrée", inputs: &[], outputs: &[("image", Sock::Color), ("alpha", Sock::Float)], params: &[ParamDef::File("fichier")], mat: false, comp: true },
    Def { op: "color", label: "Couleur", cat: "entrée", inputs: &[("couleur", Sock::Color, [0.9, 0.4, 0.2, 1.0])], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "value", label: "Valeur", cat: "entrée", inputs: &[("valeur", Sock::Float, f(0.5))], outputs: &[("valeur", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "coords", label: "Coordonnées", cat: "entrée", inputs: &[], outputs: &[("u", Sock::Float), ("v", Sock::Float), ("uv", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "geometry", label: "Géométrie", cat: "entrée", inputs: &[], outputs: &[("normale", Sock::Color), ("position", Sock::Color), ("hauteur", Sock::Float), ("pente", Sock::Float)], params: &[], mat: true, comp: false },
    Def { op: "texture", label: "Texture image", cat: "entrée", inputs: &[("échelle", Sock::Float, f(1.0))], outputs: &[("couleur", Sock::Color), ("alpha", Sock::Float)], params: &[ParamDef::File("fichier"), ParamDef::Choice("projection", &["triplanaire", "dessus (XZ)", "face (XY)", "côté (YZ)", "sphérique"])], mat: true, comp: true },
    Def { op: "curvature", label: "Courbure", cat: "entrée", inputs: &[("intensité", Sock::Float, f(1.0))], outputs: &[("arêtes", Sock::Float), ("creux", Sock::Float), ("brute", Sock::Float)], params: &[], mat: true, comp: false },
    Def { op: "fresnel", label: "Fresnel", cat: "entrée", inputs: &[("puissance", Sock::Float, f(3.0))], outputs: &[("fresnel", Sock::Float)], params: &[], mat: true, comp: false },
    Def { op: "vcolor", label: "Couleur du fichier", cat: "entrée", inputs: &[], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: false },
    // Motifs
    Def { op: "noise", label: "Bruit", cat: "motif", inputs: &[("échelle", Sock::Float, f(4.0)), ("détail", Sock::Float, f(4.0)), ("rugosité", Sock::Float, f(0.5))], outputs: &[("valeur", Sock::Float), ("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "voronoi", label: "Voronoï", cat: "motif", inputs: &[("échelle", Sock::Float, f(5.0))], outputs: &[("distance", Sock::Float), ("cellule", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "checker", label: "Damier", cat: "motif", inputs: &[("échelle", Sock::Float, f(8.0)), ("couleur A", Sock::Color, W), ("couleur B", Sock::Color, [0.15, 0.15, 0.15, 1.0])], outputs: &[("couleur", Sock::Color), ("valeur", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "gradient", label: "Dégradé", cat: "motif", inputs: &[], outputs: &[("valeur", Sock::Float)], params: &[ParamDef::Choice("type", &["horizontal", "vertical", "radial", "hauteur (3D)"])], mat: true, comp: true },
    Def { op: "stripes", label: "Rayures", cat: "motif", inputs: &[("nombre", Sock::Float, f(10.0)), ("épaisseur", Sock::Float, f(0.5))], outputs: &[("valeur", Sock::Float)], params: &[ParamDef::Choice("axe", &["x", "y", "diagonale"])], mat: true, comp: true },
    // Couleur
    Def { op: "mix", label: "Mélange", cat: "couleur", inputs: &[("facteur", Sock::Float, f(0.5)), ("A", Sock::Color, [0.2, 0.2, 0.2, 1.0]), ("B", Sock::Color, [0.9, 0.9, 0.9, 1.0])], outputs: &[("couleur", Sock::Color)], params: &[ParamDef::Choice("mode", BLEND)], mat: true, comp: true },
    Def { op: "ramp", label: "Rampe de couleur", cat: "couleur", inputs: &[("facteur", Sock::Float, f(0.5)), ("couleur 1", Sock::Color, BLACK), ("couleur 2", Sock::Color, [0.95, 0.45, 0.2, 1.0]), ("couleur 3", Sock::Color, W)], outputs: &[("couleur", Sock::Color)], params: &[ParamDef::Float("milieu", 0.0, 1.0, 0.5)], mat: true, comp: true },
    Def { op: "hsv", label: "Teinte / saturation", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("teinte", Sock::Float, f(0.0)), ("saturation", Sock::Float, f(1.0)), ("valeur", Sock::Float, f(1.0))], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "brightcon", label: "Luminosité / contraste", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("luminosité", Sock::Float, f(0.0)), ("contraste", Sock::Float, f(0.0))], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "levels", label: "Niveaux", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("noir", Sock::Float, f(0.0)), ("blanc", Sock::Float, f(1.0)), ("gamma", Sock::Float, f(1.0))], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "invert", label: "Inverser", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY)], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "threshold", label: "Seuil", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("seuil", Sock::Float, f(0.5))], outputs: &[("valeur", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "gray", label: "Noir et blanc", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY)], outputs: &[("valeur", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "separate", label: "Séparer RVB", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY)], outputs: &[("R", Sock::Float), ("V", Sock::Float), ("B", Sock::Float), ("A", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "combine", label: "Combiner RVB", cat: "couleur", inputs: &[("R", Sock::Float, f(0.0)), ("V", Sock::Float, f(0.0)), ("B", Sock::Float, f(0.0)), ("A", Sock::Float, f(1.0))], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    // Maths
    Def { op: "math", label: "Math", cat: "math", inputs: &[("A", Sock::Float, f(0.5)), ("B", Sock::Float, f(0.5))], outputs: &[("valeur", Sock::Float)], params: &[ParamDef::Choice("opération", MATH)], mat: true, comp: true },
    Def { op: "map", label: "Remapper", cat: "math", inputs: &[("valeur", Sock::Float, f(0.5)), ("de min", Sock::Float, f(0.0)), ("de max", Sock::Float, f(1.0)), ("vers min", Sock::Float, f(0.0)), ("vers max", Sock::Float, f(1.0))], outputs: &[("valeur", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "shader", label: "Shader perso", cat: "shader", inputs: &[("A", Sock::Color, GRAY), ("B", Sock::Color, BLACK), ("F", Sock::Float, f(0.5))], outputs: &[("couleur", Sock::Color), ("F", Sock::Float)], params: &[ParamDef::Text("code")], mat: true, comp: true },
    // Filtres (compositing)
    Def { op: "blur", label: "Flou", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("rayon", 0.0, 100.0, 6.0)], mat: false, comp: true },
    Def { op: "sharpen", label: "Netteté", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("force", 0.0, 3.0, 0.8)], mat: false, comp: true },
    Def { op: "glow", label: "Halo (glow)", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("seuil", 0.0, 1.0, 0.7), ParamDef::Float("rayon", 1.0, 100.0, 20.0), ParamDef::Float("force", 0.0, 3.0, 1.0)], mat: false, comp: true },
    Def { op: "vignette", label: "Vignette", cat: "filtre", inputs: &[("image", Sock::Color, GRAY), ("force", Sock::Float, f(0.5))], outputs: &[("image", Sock::Color)], params: &[], mat: false, comp: true },
    Def { op: "grain", label: "Grain", cat: "filtre", inputs: &[("image", Sock::Color, GRAY), ("force", Sock::Float, f(0.08))], outputs: &[("image", Sock::Color)], params: &[], mat: false, comp: true },
    Def { op: "chroma", label: "Aberration chromatique", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("décalage (px)", 0.0, 30.0, 3.0)], mat: false, comp: true },
    Def { op: "pixelate", label: "Pixeliser", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("taille (px)", 1.0, 64.0, 8.0)], mat: false, comp: true },
    Def { op: "posterize", label: "Postériser", cat: "filtre", inputs: &[("image", Sock::Color, GRAY), ("niveaux", Sock::Float, f(5.0))], outputs: &[("image", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "edges", label: "Contours", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("contours", Sock::Float)], params: &[], mat: false, comp: true },
    // Motifs supplémentaires
    Def { op: "bricks", label: "Briques", cat: "motif", inputs: &[("échelle", Sock::Float, f(6.0)), ("mortier", Sock::Float, f(0.06)), ("brique", Sock::Color, [0.62, 0.25, 0.16, 1.0]), ("joint", Sock::Color, [0.8, 0.78, 0.74, 1.0]), ("variation", Sock::Float, f(0.25))], outputs: &[("couleur", Sock::Color), ("masque", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "wood", label: "Bois", cat: "motif", inputs: &[("échelle", Sock::Float, f(3.0)), ("cernes", Sock::Float, f(12.0)), ("distorsion", Sock::Float, f(0.6)), ("clair", Sock::Color, [0.78, 0.56, 0.34, 1.0]), ("foncé", Sock::Color, [0.45, 0.27, 0.13, 1.0])], outputs: &[("couleur", Sock::Color), ("valeur", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "marble", label: "Marbre", cat: "motif", inputs: &[("échelle", Sock::Float, f(3.0)), ("veines", Sock::Float, f(6.0)), ("turbulence", Sock::Float, f(4.0)), ("fond", Sock::Color, [0.92, 0.91, 0.88, 1.0]), ("veine", Sock::Color, [0.25, 0.25, 0.28, 1.0])], outputs: &[("couleur", Sock::Color), ("valeur", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "hexagons", label: "Hexagones", cat: "motif", inputs: &[("échelle", Sock::Float, f(8.0)), ("bord", Sock::Float, f(0.08))], outputs: &[("bord", Sock::Float), ("cellule", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "dots", label: "Pois", cat: "motif", inputs: &[("échelle", Sock::Float, f(10.0)), ("rayon", Sock::Float, f(0.3))], outputs: &[("valeur", Sock::Float)], params: &[ParamDef::Choice("grille", &["carrée", "décalée"])], mat: true, comp: true },
    Def { op: "waves", label: "Ondes", cat: "motif", inputs: &[("échelle", Sock::Float, f(6.0)), ("distorsion", Sock::Float, f(1.0))], outputs: &[("valeur", Sock::Float)], params: &[ParamDef::Choice("type", &["bandes", "anneaux"])], mat: true, comp: true },
    Def { op: "whitenoise", label: "Bruit blanc", cat: "motif", inputs: &[("échelle", Sock::Float, f(64.0))], outputs: &[("valeur", Sock::Float), ("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    // Couleur supplémentaire
    Def { op: "curve", label: "Courbe (contraste en S)", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("ombres", Sock::Float, f(0.0)), ("lumières", Sock::Float, f(0.0)), ("contraste S", Sock::Float, f(0.3))], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "whitebal", label: "Balance des blancs", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("température", Sock::Float, f(0.0)), ("teinte", Sock::Float, f(0.0))], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "exposure", label: "Exposition", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("EV", Sock::Float, f(0.0))], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "clamp", label: "Limiter", cat: "math", inputs: &[("valeur", Sock::Float, f(0.5)), ("min", Sock::Float, f(0.0)), ("max", Sock::Float, f(1.0))], outputs: &[("valeur", Sock::Float)], params: &[], mat: true, comp: true },
    // Filtres supplémentaires
    Def { op: "dirblur", label: "Flou directionnel", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("angle", -180.0, 180.0, 0.0), ParamDef::Float("longueur (px)", 0.0, 200.0, 20.0)], mat: false, comp: true },
    Def { op: "radialblur", label: "Flou radial (zoom)", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("force", 0.0, 0.5, 0.08)], mat: false, comp: true },
    Def { op: "lens", label: "Distorsion d'objectif", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("distorsion", -1.0, 1.0, 0.15)], mat: false, comp: true },
];

pub fn def(op: &str) -> Option<&'static Def> {
    DEFS.iter().find(|d| d.op == op)
}

// ---------- graphe ----------

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum Param {
    F(f32),
    S(String),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: u32,
    pub op: String,
    pub pos: [f32; 2],
    /// Valeurs des entrées non reliées
    #[serde(default)]
    pub values: Vec<V>,
    #[serde(default)]
    pub params: BTreeMap<String, Param>,
    /// Muet : laisse passer sa première entrée (M)
    #[serde(default)]
    pub muted: bool,
    /// Réduit : juste l'en-tête (H)
    #[serde(default)]
    pub collapsed: bool,
    /// Miniature du résultat sous le node
    #[serde(default = "yes")]
    pub preview: bool,
    /// Nom choisi par l'utilisateur (sinon celui du type)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Link {
    pub from: u32,
    pub out: usize,
    pub to: u32,
    pub inp: usize,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Graph {
    pub kind: GraphKind,
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    #[serde(default)]
    pub next_id: u32,
    /// Cadres de regroupement (visuels)
    #[serde(default)]
    pub frames: Vec<Frame>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Frame {
    pub rect: [f32; 4],
    pub label: String,
    pub color: [u8; 3],
}

impl Graph {
    pub fn add(&mut self, op: &str, pos: [f32; 2]) -> u32 {
        let d = def(op).expect("node inconnu");
        self.next_id = self.next_id.max(self.nodes.iter().map(|n| n.id + 1).max().unwrap_or(0));
        let id = self.next_id;
        self.next_id += 1;
        let mut params = BTreeMap::new();
        for p in d.params {
            match p {
                ParamDef::Choice(n, c) => {
                    params.insert(n.to_string(), Param::S(c[0].to_string()));
                }
                ParamDef::Float(n, _, _, v) => {
                    params.insert(n.to_string(), Param::F(*v));
                }
                ParamDef::Text(n) => {
                    params.insert(n.to_string(), Param::S(DEFAULT_SHADER.to_string()));
                }
                ParamDef::File(n) => {
                    params.insert(n.to_string(), Param::S(String::new()));
                }
            }
        }
        let preview = !matches!(op, "mat_out" | "comp_out");
        self.nodes.push(Node { id, op: op.into(), pos, values: d.inputs.iter().map(|i| i.2).collect(), params, muted: false, collapsed: false, preview, label: String::new() });
        id
    }

    pub fn connect(&mut self, from: u32, out: usize, to: u32, inp: usize) {
        if from == to || self.reaches(to, from) {
            return; // pas de boucle
        }
        self.links.retain(|l| !(l.to == to && l.inp == inp));
        self.links.push(Link { from, out, to, inp });
    }

    /// Vrai si `a` mène à `b` en suivant les liens.
    fn reaches(&self, a: u32, b: u32) -> bool {
        let mut stack = vec![a];
        let mut seen = std::collections::HashSet::new();
        while let Some(n) = stack.pop() {
            if n == b {
                return true;
            }
            if seen.insert(n) {
                stack.extend(self.links.iter().filter(|l| l.from == n).map(|l| l.to));
            }
        }
        false
    }

    /// Graphe réellement calculé : les nodes muets laissent passer leur première entrée.
    pub fn effective(&self) -> std::borrow::Cow<'_, Graph> {
        if !self.nodes.iter().any(|n| n.muted) {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut g = self.clone();
        for n in self.nodes.iter().filter(|n| n.muted) {
            let src = g.links.iter().find(|l| l.to == n.id && l.inp == 0).map(|l| (l.from, l.out));
            let outs: Vec<Link> = g.links.iter().filter(|l| l.from == n.id).cloned().collect();
            g.links.retain(|l| l.from != n.id);
            if let Some((f, o)) = src {
                for l in outs {
                    g.links.push(Link { from: f, out: o, to: l.to, inp: l.inp });
                }
            }
        }
        std::borrow::Cow::Owned(g)
    }

    /// Copie du graphe où la sortie montre la sortie `out` du node `id` (aperçus).
    pub fn probe(&self, id: u32, out: usize) -> Option<Graph> {
        let out_op = if self.kind == GraphKind::Material { "mat_out" } else { "comp_out" };
        let target = self.nodes.iter().find(|n| n.op == out_op)?.id;
        let mut g = self.effective().into_owned();
        g.links.retain(|l| !(l.to == target && l.inp == 0));
        g.links.push(Link { from: id, out, to: target, inp: 0 });
        if self.kind == GraphKind::Material {
            // Aperçu neutre : pas de métal, pas d'émission
            g.links.retain(|l| l.to != target || l.inp == 0);
            if let Some(n) = g.nodes.iter_mut().find(|n| n.id == target) {
                n.values = vec![[0.8, 0.8, 0.8, 1.0], [0.6; 4], [0.0; 4], [0.0, 0.0, 0.0, 1.0]];
            }
        }
        Some(g)
    }

    pub fn remove(&mut self, id: u32) {
        self.nodes.retain(|n| n.id != id);
        self.links.retain(|l| l.from != id && l.to != id);
    }

    pub fn new_material() -> Graph {
        let mut g = Graph { kind: GraphKind::Material, nodes: vec![], links: vec![], next_id: 0, frames: vec![] };
        let noise = g.add("noise", [0.0, 0.0]);
        let ramp = g.add("ramp", [220.0, 0.0]);
        let out = g.add("mat_out", [460.0, 40.0]);
        g.connect(noise, 0, ramp, 0);
        g.connect(ramp, 0, out, 0);
        g
    }

    pub fn new_compositor() -> Graph {
        let mut g = Graph { kind: GraphKind::Compositor, nodes: vec![], links: vec![], next_id: 0, frames: vec![] };
        let img = g.add("image", [0.0, 0.0]);
        let bc = g.add("brightcon", [220.0, 0.0]);
        let vig = g.add("vignette", [440.0, 20.0]);
        let out = g.add("comp_out", [660.0, 40.0]);
        g.connect(img, 0, bc, 0);
        g.connect(bc, 0, vig, 0);
        g.connect(vig, 0, out, 0);
        g
    }

    pub fn load(path: &Path) -> Result<Graph, String> {
        let t = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&t).map_err(|e| format!("Graphe illisible : {e}"))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, serde_json::to_string_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }
}

pub const SHADER_EXAMPLES: &[(&str, &str)] = &[
    ("mélange par bruit", DEFAULT_SHADER),
    ("dégradé radial", "d = len(x - 0.5, y - 0.5) * 2\nr = mix(ar, br, d)\ng = mix(ag, bg, d)\nb = mix(ab, bb, d)\n"),
    ("rayures animables (F = décalage)", "k = step(0.5, fract((x + y + f) * 10))\nr = mix(ar, br, k)\ng = mix(ag, bg, k)\nb = mix(ab, bb, k)\n"),
    ("noir et blanc contrasté", "l = ar * 0.2126 + ag * 0.7152 + ab * 0.0722\nl = smoothstep(0.2, 0.8, l)\nr = l ; g = l ; b = l\n"),
    ("teinte cinéma (orange / bleu)", "l = ar * 0.3 + ag * 0.6 + ab * 0.1\nr = mix(ar, l * 1.15 + 0.05, 0.35)\ng = mix(ag, l * 1.0, 0.35)\nb = mix(ab, l * 0.85 + (1 - l) * 0.2, 0.35)\n"),
    ("cel shading (matériau)", "d = max(0, nx * 0.5 + ny * 0.8 + nz * 0.3)\nk = if(d > 0.6, 1, if(d > 0.2, 0.6, 0.3))\nr = ar * k ; g = ag * k ; b = ab * k\n"),
    ("mousse sur le dessus (matériau)", "k = smoothstep(0.4, 0.8, ny + (fbm(px * 8, py * 8, pz * 8) - 0.5) * 0.6)\nr = mix(ar, 0.25, k)\ng = mix(ag, 0.45, k)\nb = mix(ab, 0.15, k)\n"),
    ("pixels (mosaïque de couleur B)", "s = 24\nk = rand(floor(x * s), floor(y * s))\nr = mix(ar, br, k) ; g = mix(ag, bg, k) ; b = mix(ab, bb, k)\n"),
    ("vignette ronde", "d = len(x - 0.5, y - 0.5)\nk = 1 - smoothstep(0.3, 0.75, d)\nr = ar * k ; g = ag * k ; b = ab * k\n"),
    ("bandes de scan", "k = 0.85 + 0.15 * step(0.5, fract(y * h / 3))\nr = ar * k ; g = ag * k ; b = ab * k\n"),
];

/// Matériaux prêts à l'emploi (menu « + » de l'éditeur).
pub fn material_preset(name: &str) -> Option<Graph> {
    let mut g = Graph { kind: GraphKind::Material, nodes: vec![], links: vec![], next_id: 0, frames: vec![] };
    let out = g.add("mat_out", [700.0, 40.0]);
    let set = |g: &mut Graph, id: u32, k: usize, v: V| {
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) {
            n.values[k] = v;
        }
    };
    match name {
        "bois" => {
            let st = g.add("stripes", [0.0, 0.0]);
            set(&mut g, st, 0, f(14.0));
            let no = g.add("noise", [0.0, 200.0]);
            set(&mut g, no, 0, f(3.0));
            let mx = g.add("mix", [230.0, 60.0]);
            let rp = g.add("ramp", [460.0, 40.0]);
            set(&mut g, rp, 1, [0.25, 0.12, 0.05, 1.0]);
            set(&mut g, rp, 2, [0.55, 0.32, 0.15, 1.0]);
            set(&mut g, rp, 3, [0.75, 0.52, 0.30, 1.0]);
            g.connect(st, 0, mx, 1);
            g.connect(no, 1, mx, 2);
            g.connect(mx, 0, rp, 0);
            g.connect(rp, 0, out, 0);
            set(&mut g, out, 1, f(0.6));
        }
        "pierre" => {
            let vo = g.add("voronoi", [0.0, 0.0]);
            set(&mut g, vo, 0, f(6.0));
            let no = g.add("noise", [0.0, 200.0]);
            set(&mut g, no, 0, f(12.0));
            let mx = g.add("mix", [230.0, 60.0]);
            set(&mut g, mx, 0, f(0.4));
            let rp = g.add("ramp", [460.0, 40.0]);
            set(&mut g, rp, 1, [0.18, 0.18, 0.19, 1.0]);
            set(&mut g, rp, 2, [0.45, 0.44, 0.42, 1.0]);
            set(&mut g, rp, 3, [0.70, 0.69, 0.66, 1.0]);
            g.connect(vo, 1, mx, 1);
            g.connect(no, 1, mx, 2);
            g.connect(mx, 0, rp, 0);
            g.connect(rp, 0, out, 0);
            set(&mut g, out, 1, f(0.85));
        }
        "métal rouillé" => {
            let no = g.add("noise", [0.0, 0.0]);
            set(&mut g, no, 0, f(5.0));
            set(&mut g, no, 1, f(6.0));
            let th = g.add("threshold", [230.0, 0.0]);
            set(&mut g, th, 1, f(0.55));
            let mx = g.add("mix", [460.0, 0.0]);
            set(&mut g, mx, 1, [0.62, 0.63, 0.66, 1.0]);
            set(&mut g, mx, 2, [0.45, 0.20, 0.08, 1.0]);
            let inv = g.add("invert", [460.0, 200.0]);
            g.connect(no, 1, th, 0);
            g.connect(th, 0, mx, 0);
            g.connect(mx, 0, out, 0);
            g.connect(th, 0, inv, 0);
            g.connect(inv, 0, out, 2);
            set(&mut g, out, 1, f(0.5));
        }
        "toon" => {
            let sh = g.add("shader", [300.0, 0.0]);
            if let Some(n) = g.nodes.iter_mut().find(|n| n.id == sh) {
                n.params.insert("code".into(), Param::S(SHADER_EXAMPLES[5].1.to_string()));
                n.values[0] = [0.95, 0.45, 0.25, 1.0];
            }
            g.connect(sh, 0, out, 0);
            set(&mut g, out, 1, f(1.0));
        }
        "céramique" => {
            set(&mut g, out, 0, [0.92, 0.9, 0.85, 1.0]);
            set(&mut g, out, 1, f(0.08));
        }
        "or" => {
            set(&mut g, out, 0, [1.0, 0.78, 0.34, 1.0]);
            set(&mut g, out, 1, f(0.25));
            set(&mut g, out, 2, f(1.0));
        }
        // Matériaux « intelligents » : ils lisent la forme du modèle (arêtes, creux)
        "métal peint usé" => {
            let cv = g.add("curvature", [0.0, 60.0]);
            set(&mut g, cv, 0, f(2.5));
            let mx = g.add("mix", [240.0, 20.0]);
            set(&mut g, mx, 1, [0.12, 0.35, 0.3, 1.0]);
            set(&mut g, mx, 2, [0.75, 0.72, 0.68, 1.0]);
            g.connect(cv, 0, mx, 0);
            let dirt = g.add("mix", [470.0, 20.0]);
            set(&mut g, dirt, 2, [0.05, 0.04, 0.03, 1.0]);
            g.connect(cv, 1, dirt, 0);
            g.connect(mx, 0, dirt, 1);
            g.connect(dirt, 0, out, 0);
            g.connect(cv, 0, out, 2);
            set(&mut g, out, 1, f(0.4));
        }
        "pierre aux creux sombres" => {
            let cv = g.add("curvature", [0.0, 60.0]);
            set(&mut g, cv, 0, f(3.0));
            let no = g.add("noise", [0.0, 240.0]);
            set(&mut g, no, 0, f(9.0));
            let base = g.add("mix", [240.0, 160.0]);
            set(&mut g, base, 1, [0.42, 0.4, 0.37, 1.0]);
            set(&mut g, base, 2, [0.62, 0.6, 0.55, 1.0]);
            g.connect(no, 0, base, 0);
            let dirt = g.add("mix", [470.0, 60.0]);
            set(&mut g, dirt, 2, [0.1, 0.09, 0.07, 1.0]);
            g.connect(cv, 1, dirt, 0);
            g.connect(base, 0, dirt, 1);
            g.connect(dirt, 0, out, 0);
            set(&mut g, out, 1, f(0.85));
        }
        "verre" => {
            set(&mut g, out, 0, [0.95, 0.98, 1.0, 1.0]);
            set(&mut g, out, 1, f(0.03));
            set(&mut g, out, 4, f(1.0));
            set(&mut g, out, 5, f(1.5));
        }
        "verre dépoli" => {
            set(&mut g, out, 0, [0.9, 0.95, 0.95, 1.0]);
            set(&mut g, out, 1, f(0.45));
            set(&mut g, out, 4, f(1.0));
            set(&mut g, out, 5, f(1.5));
        }
        "eau" => {
            // Teinte plus profonde là où on regarde la surface de face, reflets sur les bords
            let fr = g.add("fresnel", [0.0, 40.0]);
            set(&mut g, fr, 0, f(2.0));
            let rp = g.add("ramp", [230.0, 40.0]);
            set(&mut g, rp, 1, [0.05, 0.32, 0.42, 1.0]);
            set(&mut g, rp, 2, [0.55, 0.85, 0.9, 1.0]);
            g.connect(fr, 0, rp, 0);
            g.connect(rp, 0, out, 0);
            set(&mut g, out, 1, f(0.04));
            set(&mut g, out, 4, f(0.85));
            set(&mut g, out, 5, f(1.33));
        }
        "toon 3 bandes" => {
            set(&mut g, out, 0, [0.95, 0.45, 0.3, 1.0]);
            set(&mut g, out, 1, f(0.6));
            set(&mut g, out, 7, f(3.0));
            set(&mut g, out, 8, f(0.0));
        }
        "peinture de voiture" => {
            set(&mut g, out, 0, [0.6, 0.05, 0.08, 1.0]);
            set(&mut g, out, 1, f(0.45));
            set(&mut g, out, 2, f(0.6));
            set(&mut g, out, 6, f(1.0));
        }
        "plastique" => {
            set(&mut g, out, 0, [0.1, 0.35, 0.8, 1.0]);
            set(&mut g, out, 1, f(0.3));
        }
        "néon" => {
            set(&mut g, out, 0, [0.05, 0.05, 0.05, 1.0]);
            set(&mut g, out, 3, [0.2, 1.0, 0.8, 1.0]);
        }
        _ => return None,
    }
    Some(g)
}

pub const MATERIAL_PRESETS: [&str; 15] = ["métal peint usé", "pierre aux creux sombres", "bois", "pierre", "métal rouillé", "toon", "céramique", "or", "verre", "verre dépoli", "eau", "toon 3 bandes", "peinture de voiture", "plastique", "néon"];

pub const DEFAULT_SHADER: &str = "// r g b a : couleur A · ar ag ab : A · br bg bb : B · f : entrée F\n// x y : coordonnées 0..1 · nx ny nz / px py pz : normale et position (matériau)\n// fonctions : sin cos abs min max clamp mix step smoothstep pow sqrt fract floor noise fbm rand…\nk = smoothstep(0.3, 0.7, fbm(x * 6, y * 6, 0))\nr = mix(ar, br, k)\ng = mix(ag, bg, k)\nb = mix(ab, bb, k)\n";

// ---------- langage du shader perso ----------

#[derive(Clone, Debug)]
enum Expr {
    Num(f32),
    Var(usize),
    Neg(Box<Expr>),
    Bin(char, Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
}

#[derive(Clone, Copy, Debug)]
enum Func {
    Sin,
    Cos,
    Tan,
    Abs,
    Floor,
    Ceil,
    Fract,
    Sqrt,
    Exp,
    Log,
    Pow,
    Min,
    Max,
    Clamp,
    Mix,
    Step,
    Smooth,
    Noise,
    Fbm,
    Rand,
    Atan2,
    Sign,
    Mod,
    If,
    Len,
}

/// Variables prédéfinies (dans cet ordre).
const VARS: [&str; 24] = ["r", "g", "b", "a", "ar", "ag", "ab", "aa", "br", "bg", "bb", "ba", "f", "x", "y", "nx", "ny", "nz", "px", "py", "pz", "pi", "w", "h"];

#[derive(Clone)]
pub struct Shader {
    stmts: Vec<(usize, Expr)>,
    slots: usize,
}

struct Parser<'a> {
    toks: Vec<Tok<'a>>,
    i: usize,
    names: &'a mut Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tok<'a> {
    Num(f32),
    Id(&'a str),
    Op(char),
}

fn lex(s: &str) -> Result<Vec<Tok<'_>>, String> {
    let mut out = vec![];
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let st = i;
            while i < b.len() && ((b[i] as char).is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            out.push(Tok::Num(s[st..i].parse().map_err(|_| format!("nombre invalide : {}", &s[st..i]))?));
        } else if c.is_alphabetic() || c == '_' {
            let st = i;
            while i < b.len() && ((b[i] as char).is_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Tok::Id(&s[st..i]));
        } else if "+-*/^(),=<>%".contains(c) {
            out.push(Tok::Op(c));
            i += 1;
        } else {
            return Err(format!("caractère inattendu « {c} »"));
        }
    }
    Ok(out)
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<Tok<'a>> {
        self.toks.get(self.i).copied()
    }
    fn expr(&mut self, min: u8) -> Result<Expr, String> {
        let mut lhs = match self.peek() {
            Some(Tok::Num(n)) => {
                self.i += 1;
                Expr::Num(n)
            }
            Some(Tok::Op('-')) => {
                self.i += 1;
                Expr::Neg(Box::new(self.expr(4)?))
            }
            Some(Tok::Op('(')) => {
                self.i += 1;
                let e = self.expr(0)?;
                if self.peek() != Some(Tok::Op(')')) {
                    return Err("« ) » manquante".into());
                }
                self.i += 1;
                e
            }
            Some(Tok::Id(name)) => {
                self.i += 1;
                if self.peek() == Some(Tok::Op('(')) {
                    self.i += 1;
                    let mut args = vec![];
                    if self.peek() != Some(Tok::Op(')')) {
                        loop {
                            args.push(self.expr(0)?);
                            match self.peek() {
                                Some(Tok::Op(',')) => self.i += 1,
                                Some(Tok::Op(')')) => break,
                                _ => return Err(format!("« , » ou « ) » attendue dans {name}(…)")),
                            }
                        }
                    }
                    self.i += 1;
                    let (func, n) = match name {
                        "sin" => (Func::Sin, 1),
                        "cos" => (Func::Cos, 1),
                        "tan" => (Func::Tan, 1),
                        "abs" => (Func::Abs, 1),
                        "floor" => (Func::Floor, 1),
                        "ceil" => (Func::Ceil, 1),
                        "fract" => (Func::Fract, 1),
                        "sqrt" => (Func::Sqrt, 1),
                        "exp" => (Func::Exp, 1),
                        "log" => (Func::Log, 1),
                        "sign" => (Func::Sign, 1),
                        "pow" => (Func::Pow, 2),
                        "min" => (Func::Min, 2),
                        "max" => (Func::Max, 2),
                        "mod" => (Func::Mod, 2),
                        "step" => (Func::Step, 2),
                        "atan2" => (Func::Atan2, 2),
                        "rand" => (Func::Rand, 2),
                        "len" | "length" => (Func::Len, 2),
                        "clamp" => (Func::Clamp, 3),
                        "mix" | "lerp" => (Func::Mix, 3),
                        "smoothstep" => (Func::Smooth, 3),
                        "noise" => (Func::Noise, 3),
                        "fbm" => (Func::Fbm, 3),
                        "if" => (Func::If, 3),
                        _ => return Err(format!("fonction inconnue : {name}")),
                    };
                    if args.len() != n {
                        return Err(format!("{name} attend {n} argument(s)"));
                    }
                    Expr::Call(func, args)
                } else {
                    let slot = self.names.iter().position(|n| n == name).ok_or(format!("variable inconnue : {name}"))?;
                    Expr::Var(slot)
                }
            }
            t => return Err(format!("expression attendue, trouvé {t:?}")),
        };
        loop {
            let Some(Tok::Op(op)) = self.peek() else { break };
            let (prec, right) = match op {
                '<' | '>' => (1, false),
                '+' | '-' => (2, false),
                '*' | '/' | '%' => (3, false),
                '^' => (4, true),
                _ => break,
            };
            if prec < min {
                break;
            }
            self.i += 1;
            let rhs = self.expr(if right { prec } else { prec + 1 })?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }
}

impl Shader {
    pub fn compile(code: &str) -> Result<Shader, String> {
        let mut names: Vec<String> = VARS.iter().map(|s| s.to_string()).collect();
        let mut stmts = vec![];
        for (ln, raw) in code.lines().enumerate() {
            let line = raw.split("//").next().unwrap_or("").trim();
            for st in line.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                let toks = lex(st).map_err(|e| format!("ligne {} : {e}", ln + 1))?;
                let (Some(Tok::Id(target)), Some(Tok::Op('='))) = (toks.first().copied(), toks.get(1).copied()) else {
                    return Err(format!("ligne {} : il faut « nom = expression »", ln + 1));
                };
                let target = target.to_string();
                let mut p = Parser { toks: toks[2..].to_vec(), i: 0, names: &mut names };
                let e = p.expr(0).map_err(|e| format!("ligne {} : {e}", ln + 1))?;
                if p.i != p.toks.len() {
                    return Err(format!("ligne {} : texte en trop après l'expression", ln + 1));
                }
                let slot = match names.iter().position(|n| *n == target) {
                    Some(s) => s,
                    None => {
                        names.push(target);
                        names.len() - 1
                    }
                };
                stmts.push((slot, e));
            }
        }
        Ok(Shader { stmts, slots: names.len() })
    }

    fn eval(&self, vars: &mut [f32]) {
        for (slot, e) in &self.stmts {
            vars[*slot] = ev(e, vars);
        }
    }
}

fn ev(e: &Expr, v: &[f32]) -> f32 {
    match e {
        Expr::Num(n) => *n,
        Expr::Var(i) => v[*i],
        Expr::Neg(a) => -ev(a, v),
        Expr::Bin(op, a, b) => {
            let (x, y) = (ev(a, v), ev(b, v));
            match op {
                '+' => x + y,
                '-' => x - y,
                '*' => x * y,
                '/' => {
                    if y == 0.0 {
                        0.0
                    } else {
                        x / y
                    }
                }
                '%' => x.rem_euclid(if y == 0.0 { 1.0 } else { y }),
                '^' => x.powf(y),
                '<' => (x < y) as u8 as f32,
                '>' => (x > y) as u8 as f32,
                _ => 0.0,
            }
        }
        Expr::Call(fun, a) => {
            let g = |i: usize| ev(&a[i], v);
            match fun {
                Func::Sin => g(0).sin(),
                Func::Cos => g(0).cos(),
                Func::Tan => g(0).tan(),
                Func::Abs => g(0).abs(),
                Func::Floor => g(0).floor(),
                Func::Ceil => g(0).ceil(),
                Func::Fract => g(0).fract().rem_euclid(1.0),
                Func::Sqrt => g(0).max(0.0).sqrt(),
                Func::Exp => g(0).exp(),
                Func::Log => g(0).max(1e-9).ln(),
                Func::Sign => g(0).signum(),
                Func::Pow => g(0).abs().powf(g(1)),
                Func::Min => g(0).min(g(1)),
                Func::Max => g(0).max(g(1)),
                Func::Mod => g(0).rem_euclid(if g(1) == 0.0 { 1.0 } else { g(1) }),
                Func::Step => (g(1) >= g(0)) as u8 as f32,
                Func::Atan2 => g(0).atan2(g(1)),
                Func::Rand => hash2(g(0), g(1)),
                Func::Len => (g(0) * g(0) + g(1) * g(1)).sqrt(),
                Func::Clamp => g(0).clamp(g(1).min(g(2)), g(2).max(g(1))),
                Func::Mix => g(0) + (g(1) - g(0)) * g(2),
                Func::Smooth => smoothstep(g(0), g(1), g(2)),
                Func::Noise => noise3(g(0), g(1), g(2)),
                Func::Fbm => fbm(g(0), g(1), g(2), 5, 0.5),
                Func::If => {
                    if g(0) > 0.0 {
                        g(1)
                    } else {
                        g(2)
                    }
                }
            }
        }
    }
}

// ---------- bruit ----------

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash3(x: i32, y: i32, z: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6b343) ^ (y as u32).wrapping_mul(0xd8163841) ^ (z as u32).wrapping_mul(0xcb1ab31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1e995);
    h ^= h >> 15;
    (h & 0xffffff) as f32 / 16777215.0
}

fn hash2(x: f32, y: f32) -> f32 {
    hash3((x * 1000.0) as i32, (y * 1000.0) as i32, 7)
}

pub fn noise3(x: f32, y: f32, z: f32) -> f32 {
    let (xi, yi, zi) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let (fx, fy, fz) = (x - x.floor(), y - y.floor(), z - z.floor());
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (u, v, w) = (s(fx), s(fy), s(fz));
    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let c = |dx, dy, dz| hash3(xi + dx, yi + dy, zi + dz);
    l(l(l(c(0, 0, 0), c(1, 0, 0), u), l(c(0, 1, 0), c(1, 1, 0), u), v), l(l(c(0, 0, 1), c(1, 0, 1), u), l(c(0, 1, 1), c(1, 1, 1), u), v), w)
}

pub fn fbm(x: f32, y: f32, z: f32, octaves: usize, rough: f32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for _ in 0..octaves.clamp(1, 10) {
        sum += noise3(x * freq, y * freq, z * freq) * amp;
        norm += amp;
        amp *= rough;
        freq *= 2.0;
    }
    sum / norm
}

fn voronoi(x: f32, y: f32, z: f32) -> (f32, V) {
    let (xi, yi, zi) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let mut best = (f32::MAX, (0, 0, 0));
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (cx, cy, cz) = (xi + dx, yi + dy, zi + dz);
                let px = cx as f32 + hash3(cx, cy, cz);
                let py = cy as f32 + hash3(cx + 17, cy, cz);
                let pz = cz as f32 + hash3(cx, cy + 31, cz);
                let d = ((px - x).powi(2) + (py - y).powi(2) + (pz - z).powi(2)).sqrt();
                if d < best.0 {
                    best = (d, (cx, cy, cz));
                }
            }
        }
    }
    let (cx, cy, cz) = best.1;
    (best.0.min(1.0), [hash3(cx, cy, cz + 5), hash3(cx + 3, cy, cz), hash3(cx, cy + 9, cz), 1.0])
}

// ---------- couleur ----------

fn rgb2hsv(c: V) -> [f32; 3] {
    let (r, g, b) = (c[0], c[1], c[2]);
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let d = mx - mn;
    let h = if d == 0.0 {
        0.0
    } else if mx == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if mx == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    [h, if mx == 0.0 { 0.0 } else { d / mx }, mx]
}

fn hsv2rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

fn blend(mode: &str, a: V, b: V, t: f32) -> V {
    let mut out = [0.0; 4];
    for k in 0..3 {
        let (x, y) = (a[k], b[k]);
        let r = match mode {
            "multiplier" => x * y,
            "écran" => 1.0 - (1.0 - x) * (1.0 - y),
            "incrustation" => {
                if x < 0.5 {
                    2.0 * x * y
                } else {
                    1.0 - 2.0 * (1.0 - x) * (1.0 - y)
                }
            }
            "ajouter" => x + y,
            "soustraire" => x - y,
            "différence" => (x - y).abs(),
            "éclaircir" => x.max(y),
            "assombrir" => x.min(y),
            "diviser" => {
                if y <= 1e-6 {
                    1.0
                } else {
                    x / y
                }
            }
            _ => y,
        };
        out[k] = x + (r - x) * t;
    }
    out[3] = a[3] + (b[3] - a[3]) * t;
    out
}

fn lum(c: V) -> f32 {
    c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722
}

// ---------- évaluation ----------

/// Contexte d'un pixel (image) ou d'un point de surface (matériau).
/// Ce que la sortie matériau décrit pour un point de la surface.
pub struct Surf {
    pub color: V,
    pub rough: f32,
    pub metal: f32,
    pub emit: V,
    /// 0 : opaque ; 1 : on voit à travers (verre, eau)
    pub transmission: f32,
    pub ior: f32,
    /// Couche de vernis brillant par-dessus
    pub coat: f32,
    /// Nombre de bandes d'ombre (0 : ombrage normal)
    pub toon: f32,
    /// Force des reflets
    pub spec: f32,
}

#[derive(Clone, Copy, Default)]
pub struct Ctx {
    pub uv: [f32; 2],
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    pub view: [f32; 3],
    pub vcol: V,
    pub res: [f32; 2],
    /// Courbure de la surface en ce point (arêtes > 0, creux < 0)
    pub curv: f32,
}

/// Graphe compilé : nodes dans l'ordre d'exécution, entrées résolues.
#[derive(Clone)]
pub struct Program {
    steps: Vec<Step>,
    regs: usize,
    pub kind: GraphKind,
    /// Vrai si un node lit la courbure : le rendu ne la calcule que dans ce cas
    pub uses_curvature: bool,
}

#[derive(Clone)]
struct Step {
    op: &'static str,
    ins: Vec<Src>,
    base: usize,
    nout: usize,
    params: BTreeMap<String, Param>,
    shader: Option<Shader>,
    /// Image chargée (node « Image »)
    image: Option<std::sync::Arc<Img>>,
}

#[derive(Clone, Copy)]
enum Src {
    Reg(usize),
    Const(V),
}

pub struct Img {
    pub w: usize,
    pub h: usize,
    pub px: Vec<V>,
}

impl Img {
    fn sample(&self, u: f32, v: f32) -> V {
        let x = (u * self.w as f32 - 0.5).clamp(0.0, self.w as f32 - 1.0);
        let y = (v * self.h as f32 - 0.5).clamp(0.0, self.h as f32 - 1.0);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let p = |xx: usize, yy: usize| self.px[yy * self.w + xx];
        let (a, b, c, d) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
        let mut o = [0.0; 4];
        for k in 0..4 {
            o[k] = (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy;
        }
        o
    }
}

pub fn load_img(path: &Path, max: u32) -> Option<Img> {
    let i = image::open(path).ok()?;
    let i = if i.width() > max || i.height() > max { i.thumbnail(max, max) } else { i };
    let r = i.to_rgba32f();
    Some(Img { w: r.width() as usize, h: r.height() as usize, px: r.pixels().map(|p| [p[0], p[1], p[2], p[3]]).collect() })
}

fn pstr<'a>(p: &'a BTreeMap<String, Param>, k: &str) -> &'a str {
    match p.get(k) {
        Some(Param::S(s)) => s,
        _ => "",
    }
}
fn pnum(p: &BTreeMap<String, Param>, k: &str, d: f32) -> f32 {
    match p.get(k) {
        Some(Param::F(v)) => *v,
        _ => d,
    }
}

impl Program {
    /// `root` : dossier du coffre (pour les images), `max_img` : taille max des images chargées.
    pub fn compile(g: &Graph, root: &str, max_img: u32) -> Result<Program, (u32, String)> {
        // Ordre topologique depuis la sortie
        let out_op = if g.kind == GraphKind::Material { "mat_out" } else { "comp_out" };
        let out_node = g.nodes.iter().find(|n| n.op == out_op).map(|n| n.id);
        let mut order: Vec<u32> = vec![];
        let mut state: HashMap<u32, u8> = HashMap::new();
        fn visit(id: u32, g: &Graph, order: &mut Vec<u32>, st: &mut HashMap<u32, u8>) {
            if st.get(&id).copied().unwrap_or(0) != 0 {
                return;
            }
            st.insert(id, 1);
            for l in g.links.iter().filter(|l| l.to == id) {
                visit(l.from, g, order, st);
            }
            st.insert(id, 2);
            order.push(id);
        }
        match out_node {
            Some(o) => visit(o, g, &mut order, &mut state),
            None => return Err((u32::MAX, "il manque le node de sortie".into())),
        }
        let mut base: HashMap<u32, usize> = HashMap::new();
        let mut regs = 0;
        let mut steps = vec![];
        for id in order {
            let n = g.nodes.iter().find(|n| n.id == id).unwrap();
            let Some(d) = def(&n.op) else { return Err((id, format!("node inconnu : {}", n.op))) };
            let ins = (0..d.inputs.len())
                .map(|i| match g.links.iter().find(|l| l.to == id && l.inp == i) {
                    Some(l) => match base.get(&l.from) {
                        Some(b) => Src::Reg(b + l.out),
                        None => Src::Const(d.inputs[i].2),
                    },
                    None => Src::Const(n.values.get(i).copied().unwrap_or(d.inputs[i].2)),
                })
                .collect();
            let shader = if n.op == "shader" { Some(Shader::compile(pstr(&n.params, "code")).map_err(|e| (id, e))?) } else { None };
            let image = if n.op == "image" || n.op == "texture" {
                let rel = pstr(&n.params, "fichier");
                if rel.is_empty() {
                    None
                } else {
                    let p = crate::vault::safe(root, rel).map_err(|e| (id, e))?;
                    Some(std::sync::Arc::new(load_img(&p, max_img).ok_or((id, format!("image illisible : {rel}")))?))
                }
            } else {
                None
            };
            base.insert(id, regs);
            steps.push(Step { op: d.op, ins, base: regs, nout: d.outputs.len(), params: n.params.clone(), shader, image });
            regs += d.outputs.len().max(4);
        }
        let uses_curvature = steps.iter().any(|s| s.op == "curvature");
        Ok(Program { steps, regs, kind: g.kind, uses_curvature })
    }

    /// Matériau : couleur (sRGB), rugosité, métal, émission.
    pub fn material(&self, c: &Ctx, regs: &mut Vec<V>) -> Surf {
        regs.resize(self.regs, [0.0; 4]);
        for s in &self.steps {
            let ins: Vec<V> = s.ins.iter().map(|i| match i {
                Src::Reg(r) => regs[*r],
                Src::Const(v) => *v,
            }).collect();
            if s.op == "mat_out" {
                let x = |k: usize, d: f32| ins.get(k).map(|v| v[0]).unwrap_or(d);
                return Surf { color: ins[0], rough: ins[1][0].clamp(0.02, 1.0), metal: ins[2][0].clamp(0.0, 1.0), emit: ins[3], transmission: x(4, 0.0).clamp(0.0, 1.0), ior: x(5, 1.45).clamp(1.0, 3.0), coat: x(6, 0.0).clamp(0.0, 1.0), toon: x(7, 0.0).clamp(0.0, 16.0), spec: x(8, 1.0).clamp(0.0, 2.0) };
            }
            let o = pixel(s, &ins, c);
            for (k, v) in o.iter().enumerate().take(s.nout.min(4)) {
                regs[s.base + k] = *v;
            }
        }
        Surf { color: [0.8, 0.5, 0.4, 1.0], rough: 0.5, metal: 0.0, emit: [0.0; 4], transmission: 0.0, ior: 1.45, coat: 0.0, toon: 0.0, spec: 1.0 }
    }

    /// Compositing : calcule l'image de sortie à la taille donnée (ou celle de la première image).
    pub fn composite(&self, size: Option<[usize; 2]>) -> Img {
        let [w, h] = size.unwrap_or_else(|| self.steps.iter().find_map(|s| s.image.as_ref().map(|i| [i.w, i.h])).unwrap_or([512, 512]));
        let n = w * h;
        let mut bufs: Vec<Option<Vec<V>>> = vec![None; self.regs];
        use rayon::prelude::*;
        for s in &self.steps {
            let get = |src: &Src| -> Vec<V> {
                match src {
                    Src::Reg(r) => bufs[*r].clone().unwrap_or_else(|| vec![[0.0; 4]; n]),
                    Src::Const(v) => vec![*v; n],
                }
            };
            if s.op == "comp_out" {
                let px = get(&s.ins[0]);
                return Img { w, h, px };
            }
            let outs: Vec<Vec<V>> = match s.op {
                "blur" => vec![blur(&get(&s.ins[0]), w, h, pnum(&s.params, "rayon", 6.0) * w as f32 / 1000.0)],
                "glow" => {
                    let src = get(&s.ins[0]);
                    let th = pnum(&s.params, "seuil", 0.7);
                    let bright: Vec<V> = src.iter().map(|c| {
                        let k = ((lum(*c) - th) / (1.0 - th).max(0.01)).clamp(0.0, 1.0);
                        [c[0] * k, c[1] * k, c[2] * k, 1.0]
                    }).collect();
                    let b = blur(&bright, w, h, pnum(&s.params, "rayon", 20.0) * w as f32 / 1000.0);
                    let force = pnum(&s.params, "force", 1.0);
                    vec![src.iter().zip(&b).map(|(a, g)| [a[0] + g[0] * force, a[1] + g[1] * force, a[2] + g[2] * force, a[3]]).collect()]
                }
                "sharpen" => {
                    let src = get(&s.ins[0]);
                    let b = blur(&src, w, h, 1.5);
                    let k = pnum(&s.params, "force", 0.8);
                    vec![src.iter().zip(&b).map(|(a, c)| [a[0] + (a[0] - c[0]) * k, a[1] + (a[1] - c[1]) * k, a[2] + (a[2] - c[2]) * k, a[3]]).collect()]
                }
                "dirblur" | "radialblur" | "lens" => {
                    let src = get(&s.ins[0]);
                    let img = Img { w, h, px: src };
                    let ang = pnum(&s.params, "angle", 0.0).to_radians();
                    let len = pnum(&s.params, "longueur (px)", 20.0);
                    let force = pnum(&s.params, "force", 0.08);
                    let dist = pnum(&s.params, "distorsion", 0.15);
                    let out: Vec<V> = (0..n).into_par_iter().map(|i| {
                        let (u, v) = (((i % w) as f32 + 0.5) / w as f32, ((i / w) as f32 + 0.5) / h as f32);
                        match s.op {
                            "lens" => {
                                let (dx, dy) = (u - 0.5, v - 0.5);
                                let r2 = dx * dx + dy * dy;
                                let k = 1.0 + dist * r2 * 4.0;
                                img.sample(0.5 + dx * k, 0.5 + dy * k)
                            }
                            _ => {
                                let taps = 24;
                                let mut acc = [0.0f32; 4];
                                for t in 0..taps {
                                    let f = t as f32 / (taps - 1) as f32 - 0.5;
                                    let (su, sv) = if s.op == "dirblur" {
                                        (u + ang.cos() * len * f / w as f32, v + ang.sin() * len * f / h as f32)
                                    } else {
                                        let k = 1.0 - force * (f + 0.5);
                                        (0.5 + (u - 0.5) * k, 0.5 + (v - 0.5) * k)
                                    };
                                    let c = img.sample(su, sv);
                                    for ch in 0..4 {
                                        acc[ch] += c[ch];
                                    }
                                }
                                [acc[0] / taps as f32, acc[1] / taps as f32, acc[2] / taps as f32, acc[3] / taps as f32]
                            }
                        }
                    }).collect();
                    vec![out]
                }
                "chroma" | "pixelate" | "edges" => {
                    let src = get(&s.ins[0]);
                    let img = Img { w, h, px: src };
                    let amt = pnum(&s.params, "décalage (px)", 3.0);
                    let size = pnum(&s.params, "taille (px)", 8.0).max(1.0);
                    let out: Vec<V> = (0..n).into_par_iter().map(|i| {
                        let (x, y) = ((i % w) as f32 + 0.5, (i / w) as f32 + 0.5);
                        let (u, v) = (x / w as f32, y / h as f32);
                        match s.op {
                            "chroma" => {
                                let (dx, dy) = (u - 0.5, v - 0.5);
                                let len = (dx * dx + dy * dy).sqrt().max(1e-6);
                                let o = amt / w as f32 * len * 2.0;
                                let r = img.sample(u + dx / len * o, v + dy / len * o)[0];
                                let c = img.sample(u, v);
                                let b = img.sample(u - dx / len * o, v - dy / len * o)[2];
                                [r, c[1], b, c[3]]
                            }
                            "pixelate" => {
                                let (bx, by) = (((x / size).floor() + 0.5) * size, ((y / size).floor() + 0.5) * size);
                                img.sample(bx / w as f32, by / h as f32)
                            }
                            _ => {
                                let l = |ox: f32, oy: f32| lum(img.sample(u + ox / w as f32, v + oy / h as f32));
                                let gx = l(1.0, 0.0) - l(-1.0, 0.0);
                                let gy = l(0.0, 1.0) - l(0.0, -1.0);
                                let e = (gx * gx + gy * gy).sqrt() * 2.0;
                                [e, e, e, 1.0]
                            }
                        }
                    }).collect();
                    if s.op == "edges" { vec![out.clone(), out] } else { vec![out] }
                }
                "image" => match &s.image {
                    Some(img) => {
                        let px: Vec<V> = if img.w == w && img.h == h { img.px.clone() } else { (0..n).into_par_iter().map(|i| img.sample(((i % w) as f32 + 0.5) / w as f32, ((i / w) as f32 + 0.5) / h as f32)).collect() };
                        let alpha: Vec<V> = px.iter().map(|p| [p[3]; 4]).collect();
                        vec![px, alpha]
                    }
                    None => vec![vec![[0.5, 0.5, 0.5, 1.0]; n], vec![[1.0; 4]; n]],
                },
                _ => {
                    // Node pixel par pixel
                    let inputs: Vec<Vec<V>> = s.ins.iter().map(get).collect();
                    let res = [w as f32, h as f32];
                    let per: Vec<[V; 4]> = (0..n).into_par_iter().map(|i| {
                        let ins: Vec<V> = inputs.iter().map(|b| b[i]).collect();
                        let c = Ctx { uv: [((i % w) as f32 + 0.5) / w as f32, ((i / w) as f32 + 0.5) / h as f32], res, ..Default::default() };
                        pixel(s, &ins, &c)
                    }).collect();
                    (0..s.nout.min(4)).map(|k| per.iter().map(|o| o[k]).collect()).collect()
                }
            };
            for (k, o) in outs.into_iter().enumerate() {
                if s.base + k < bufs.len() {
                    bufs[s.base + k] = Some(o);
                }
            }
        }
        Img { w, h, px: vec![[0.5, 0.5, 0.5, 1.0]; n] }
    }

}

fn blur(src: &[V], w: usize, h: usize, radius: f32) -> Vec<V> {
    use rayon::prelude::*;
    let r = radius.round() as usize;
    if r == 0 {
        return src.to_vec();
    }
    let mut a = src.to_vec();
    for _ in 0..3 {
        // horizontal
        let b: Vec<V> = (0..h).into_par_iter().flat_map_iter(|y| {
            let row = &a[y * w..(y + 1) * w];
            let mut out = vec![[0.0; 4]; w];
            let mut acc = [0.0f32; 4];
            let get = |x: isize| row[x.clamp(0, w as isize - 1) as usize];
            for x in -(r as isize)..=(r as isize) {
                let c = get(x);
                for k in 0..4 {
                    acc[k] += c[k];
                }
            }
            let n = (2 * r + 1) as f32;
            for x in 0..w {
                out[x] = [acc[0] / n, acc[1] / n, acc[2] / n, acc[3] / n];
                let add = get(x as isize + r as isize + 1);
                let sub = get(x as isize - r as isize);
                for k in 0..4 {
                    acc[k] += add[k] - sub[k];
                }
            }
            out
        }).collect();
        // vertical
        let mut c = vec![[0.0f32; 4]; w * h];
        let cols: Vec<Vec<V>> = (0..w).into_par_iter().map(|x| {
            let get = |y: isize| b[(y.clamp(0, h as isize - 1) as usize) * w + x];
            let mut acc = [0.0f32; 4];
            for y in -(r as isize)..=(r as isize) {
                let v = get(y);
                for k in 0..4 {
                    acc[k] += v[k];
                }
            }
            let n = (2 * r + 1) as f32;
            let mut col = vec![[0.0; 4]; h];
            for y in 0..h {
                col[y] = [acc[0] / n, acc[1] / n, acc[2] / n, acc[3] / n];
                let add = get(y as isize + r as isize + 1);
                let sub = get(y as isize - r as isize);
                for k in 0..4 {
                    acc[k] += add[k] - sub[k];
                }
            }
            col
        }).collect();
        for (x, col) in cols.into_iter().enumerate() {
            for (y, v) in col.into_iter().enumerate() {
                c[y * w + x] = v;
            }
        }
        a = c;
    }
    a
}

/// Un node évalué en un point.
fn pixel(s: &Step, i: &[V], c: &Ctx) -> [V; 4] {
    let z = [0.0; 4];
    let one = |x: f32| [x, x, x, 1.0];
    let p3 = if c.pos == [0.0; 3] { [c.uv[0], c.uv[1], 0.0] } else { c.pos };
    match s.op {
        "color" | "value" => [i[0], z, z, z],
        "coords" => [one(c.uv[0]), one(c.uv[1]), [c.uv[0], c.uv[1], 0.0, 1.0], z],
        "geometry" => {
            let n = c.nrm;
            [[n[0] * 0.5 + 0.5, n[1] * 0.5 + 0.5, n[2] * 0.5 + 0.5, 1.0], [c.pos[0] + 0.5, c.pos[1] + 0.5, c.pos[2] + 0.5, 1.0], one(c.pos[1] + 0.5), one(1.0 - n[1].abs())]
        }
        "curvature" => {
            // Sur un modèle ramené à une taille de 1 : une arête nette donne quelques dixièmes
            let k = c.curv * i[0][0];
            [one(k.clamp(0.0, 1.0)), one((-k).clamp(0.0, 1.0)), one((0.5 + 0.5 * k).clamp(0.0, 1.0)), z]
        }
        "fresnel" => {
            let d = (c.nrm[0] * c.view[0] + c.nrm[1] * c.view[1] + c.nrm[2] * c.view[2]).abs();
            [one((1.0 - d).powf(i[0][0].max(0.1))), z, z, z]
        }
        "vcolor" => [c.vcol, z, z, z],
        "texture" => match &s.image {
            Some(img) => {
                let sc = i[0][0].max(1e-4);
                let w = |u: f32, v: f32| img.sample(u.rem_euclid(1.0), v.rem_euclid(1.0));
                let p = if c.pos == [0.0; 3] { [c.uv[0] - 0.5, 0.5 - c.uv[1], 0.0] } else { c.pos };
                let col = match pstr(&s.params, "projection") {
                    "dessus (XZ)" => w(p[0] * sc + 0.5, p[2] * sc + 0.5),
                    "face (XY)" => w(p[0] * sc + 0.5, 0.5 - p[1] * sc),
                    "côté (YZ)" => w(p[2] * sc + 0.5, 0.5 - p[1] * sc),
                    "sphérique" => {
                        let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-6);
                        w((p[0].atan2(p[2]) / std::f32::consts::TAU + 0.5) * sc, (p[1] / l).acos() / std::f32::consts::PI * sc)
                    }
                    _ => {
                        if c.pos == [0.0; 3] {
                            w(c.uv[0] * sc, c.uv[1] * sc)
                        } else {
                            // Triplanaire : trois projections mélangées selon la normale
                            let n = c.nrm;
                            let (ax, ay, az) = (n[0].abs().powi(4), n[1].abs().powi(4), n[2].abs().powi(4));
                            let t = (ax + ay + az).max(1e-6);
                            let (a, b, d) = (w(p[2] * sc + 0.5, 0.5 - p[1] * sc), w(p[0] * sc + 0.5, p[2] * sc + 0.5), w(p[0] * sc + 0.5, 0.5 - p[1] * sc));
                            let mut o = [0.0; 4];
                            for k in 0..4 {
                                o[k] = (a[k] * ax + b[k] * ay + d[k] * az) / t;
                            }
                            o
                        }
                    }
                };
                [col, one(col[3]), z, z]
            }
            None => [[0.5, 0.5, 0.5, 1.0], one(1.0), z, z],
        },
        "noise" => {
            let sc = i[0][0];
            let (x, y, zz) = (p3[0] * sc, p3[1] * sc, p3[2] * sc);
            let v = fbm(x, y, zz, i[1][0] as usize, i[2][0]);
            [one(v), [fbm(x + 13.1, y, zz, i[1][0] as usize, i[2][0]), v, fbm(x, y + 7.7, zz + 3.3, i[1][0] as usize, i[2][0]), 1.0], z, z]
        }
        "voronoi" => {
            let sc = i[0][0];
            let (d, col) = voronoi(p3[0] * sc, p3[1] * sc, p3[2] * sc);
            [one(d), col, z, z]
        }
        "checker" => {
            let sc = i[0][0];
            let k = ((p3[0] * sc).floor() + (p3[1] * sc).floor() + (p3[2] * sc).floor()) as i64 & 1;
            [if k == 0 { i[1] } else { i[2] }, one(k as f32), z, z]
        }
        "gradient" => {
            let v = match pstr(&s.params, "type") {
                "vertical" => c.uv[1],
                "radial" => 1.0 - ((c.uv[0] - 0.5).powi(2) + (c.uv[1] - 0.5).powi(2)).sqrt() * 2.0,
                "hauteur (3D)" => c.pos[1] + 0.5,
                _ => c.uv[0],
            };
            [one(v.clamp(0.0, 1.0)), z, z, z]
        }
        "stripes" => {
            let t = match pstr(&s.params, "axe") {
                "y" => p3[1],
                "diagonale" => (p3[0] + p3[1]) * 0.7071,
                _ => p3[0],
            };
            let v = ((t * i[0][0]).rem_euclid(1.0) < i[1][0]) as u8 as f32;
            [one(v), z, z, z]
        }
        "bricks" => {
            let sc = i[0][0].max(0.01);
            let (u, v) = (p3[0] * sc, p3[1] * sc * 2.0);
            let row = v.floor();
            let uu = u + if (row as i32).rem_euclid(2) == 1 { 0.5 } else { 0.0 };
            let (fx, fy) = (uu.fract().abs(), v.fract().abs());
            let m = i[1][0].clamp(0.0, 0.45);
            let mortar = fx < m || fx > 1.0 - m || fy < m * 2.0 || fy > 1.0 - m * 2.0;
            let h = hash2(uu.floor(), row) * 2.0 - 1.0;
            let k = 1.0 + h * i[4][0];
            let b = i[2];
            let col = if mortar { i[3] } else { [b[0] * k, b[1] * k, b[2] * k, 1.0] };
            [col, one(if mortar { 0.0 } else { 1.0 }), z, z]
        }
        "wood" => {
            let sc = i[0][0];
            let q = [p3[0] * sc, p3[1] * sc, p3[2] * sc];
            let d = fbm(q[0] * 0.8, q[1] * 0.8, q[2] * 0.8, 3, 0.5) * i[2][0];
            let r = ((q[0] * q[0] + q[2] * q[2]).sqrt() + d) * i[1][0];
            let t = (r.fract() * std::f32::consts::TAU).sin() * 0.5 + 0.5;
            let t = t.powf(1.6);
            let (a, b) = (i[3], i[4]);
            [[b[0] + (a[0] - b[0]) * t, b[1] + (a[1] - b[1]) * t, b[2] + (a[2] - b[2]) * t, 1.0], one(t), z, z]
        }
        "marble" => {
            let sc = i[0][0];
            let q = [p3[0] * sc, p3[1] * sc, p3[2] * sc];
            let tb = fbm(q[0], q[1], q[2], 5, 0.55) * i[2][0];
            let t = ((q[0] * i[1][0] * 0.3 + tb) * std::f32::consts::PI).sin().abs();
            let t = 1.0 - (1.0 - t).powf(6.0);
            let (a, b) = (i[3], i[4]);
            [[b[0] + (a[0] - b[0]) * t, b[1] + (a[1] - b[1]) * t, b[2] + (a[2] - b[2]) * t, 1.0], one(t), z, z]
        }
        "hexagons" => {
            let sc = i[0][0].max(0.01);
            let (x, y) = (p3[0] * sc, p3[1] * sc);
            // Grille hexagonale (deux grilles décalées)
            let r = [1.0f32, 1.7320508];
            let h = [r[0] * 0.5, r[1] * 0.5];
            let a = [x.rem_euclid(r[0]) - h[0], y.rem_euclid(r[1]) - h[1]];
            let b = [(x - h[0]).rem_euclid(r[0]) - h[0], (y - h[1]).rem_euclid(r[1]) - h[1]];
            let (g, id) = if a[0] * a[0] + a[1] * a[1] < b[0] * b[0] + b[1] * b[1] { (a, [(x / r[0]).floor(), (y / r[1]).floor()]) } else { (b, [((x - h[0]) / r[0]).floor() + 0.5, ((y - h[1]) / r[1]).floor() + 0.5]) };
            let (ax, ay) = (g[0].abs(), g[1].abs());
            let d = 0.5 - (ax * 0.5 + ay * 0.8660254).max(ax);
            let edge = if d < i[1][0] * 0.5 { 1.0 } else { 0.0 };
            let c = [hash2(id[0], id[1]), hash2(id[1] + 3.1, id[0]), hash2(id[0] * 1.7, id[1] + 9.2), 1.0];
            [one(edge), c, z, z]
        }
        "dots" => {
            let sc = i[0][0].max(0.01);
            let (mut x, y) = (p3[0] * sc, p3[1] * sc);
            if pstr(&s.params, "grille") == "décalée" && (y.floor() as i32).rem_euclid(2) == 1 {
                x += 0.5;
            }
            let (dx, dy) = (x.fract().abs() - 0.5, y.fract().abs() - 0.5);
            let d = (dx * dx + dy * dy).sqrt();
            [one(if d < i[1][0] { 1.0 } else { 0.0 }), z, z, z]
        }
        "waves" => {
            let sc = i[0][0];
            let q = [p3[0] * sc, p3[1] * sc, p3[2] * sc];
            let dist = fbm(q[0], q[1], q[2], 3, 0.5) * i[1][0];
            let t = if pstr(&s.params, "type") == "anneaux" { (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt() } else { q[0] + q[1] };
            [one(((t + dist) * std::f32::consts::TAU).sin() * 0.5 + 0.5), z, z, z]
        }
        "whitenoise" => {
            let sc = i[0][0].max(0.01);
            let (x, y, zz) = ((p3[0] * sc).floor(), (p3[1] * sc).floor(), (p3[2] * sc).floor());
            let v = hash2(x + zz * 17.0, y);
            [one(v), [v, hash2(y + 1.3, x + zz), hash2(x * 2.1, y + 7.7 + zz), 1.0], z, z]
        }
        "curve" => {
            let c = i[0];
            let (sh, hi, k) = (i[1][0], i[2][0], i[3][0]);
            let f = |x: f32| {
                let x = x.clamp(0.0, 1.0);
                // S autour de 0,5, puis relève des ombres et des lumières
                let s_ = x + k * (x - 0.5) * (1.0 - (2.0 * x - 1.0).abs());
                let s_ = s_ + sh * (1.0 - s_) * (1.0 - s_) * s_ * 2.0 + hi * s_ * s_ * (1.0 - s_) * 2.0;
                s_.clamp(0.0, 1.0)
            };
            [[f(c[0]), f(c[1]), f(c[2]), c[3]], z, z, z]
        }
        "whitebal" => {
            let c = i[0];
            let (t, g) = (i[1][0], i[2][0]);
            [[c[0] * (1.0 + t * 0.4), c[1] * (1.0 - g * 0.3), c[2] * (1.0 - t * 0.4), c[3]], z, z, z]
        }
        "exposure" => {
            let c = i[0];
            let k = 2f32.powf(i[1][0]);
            [[c[0] * k, c[1] * k, c[2] * k, c[3]], z, z, z]
        }
        "clamp" => [one(i[0][0].clamp(i[1][0].min(i[2][0]), i[2][0].max(i[1][0]))), z, z, z],
        "mix" => [blend(pstr(&s.params, "mode"), i[1], i[2], i[0][0].clamp(0.0, 1.0)), z, z, z],
        "ramp" => {
            let t = i[0][0].clamp(0.0, 1.0);
            let m = pnum(&s.params, "milieu", 0.5).clamp(0.01, 0.99);
            let l = |a: V, b: V, k: f32| [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k, a[3] + (b[3] - a[3]) * k];
            [if t < m { l(i[1], i[2], t / m) } else { l(i[2], i[3], (t - m) / (1.0 - m)) }, z, z, z]
        }
        "hsv" => {
            let h = rgb2hsv(i[0]);
            let o = hsv2rgb(h[0] + i[1][0], (h[1] * i[2][0]).clamp(0.0, 1.0), h[2] * i[3][0]);
            [[o[0], o[1], o[2], i[0][3]], z, z, z]
        }
        "brightcon" => {
            let (b, ct) = (i[1][0], i[2][0]);
            let k = 1.0 + ct;
            let f = |x: f32| (x - 0.5) * k + 0.5 + b;
            [[f(i[0][0]), f(i[0][1]), f(i[0][2]), i[0][3]], z, z, z]
        }
        "levels" => {
            let (lo, hi, g) = (i[1][0], i[2][0], i[3][0].max(0.01));
            let f = |x: f32| ((x - lo) / (hi - lo).max(1e-4)).clamp(0.0, 1.0).powf(1.0 / g);
            [[f(i[0][0]), f(i[0][1]), f(i[0][2]), i[0][3]], z, z, z]
        }
        "invert" => [[1.0 - i[0][0], 1.0 - i[0][1], 1.0 - i[0][2], i[0][3]], z, z, z],
        "threshold" => [one((lum(i[0]) >= i[1][0]) as u8 as f32), z, z, z],
        "gray" => [one(lum(i[0])), z, z, z],
        "separate" => [one(i[0][0]), one(i[0][1]), one(i[0][2]), one(i[0][3])],
        "combine" => [[i[0][0], i[1][0], i[2][0], i[3][0]], z, z, z],
        "posterize" => {
            let n = i[1][0].max(2.0);
            let f = |x: f32| (x * n).floor() / (n - 1.0);
            [[f(i[0][0]).min(1.0), f(i[0][1]).min(1.0), f(i[0][2]).min(1.0), i[0][3]], z, z, z]
        }
        "vignette" => {
            let d = ((c.uv[0] - 0.5).powi(2) + (c.uv[1] - 0.5).powi(2)).sqrt() * 1.414;
            let k = 1.0 - smoothstep(0.4, 1.1, d) * i[1][0];
            [[i[0][0] * k, i[0][1] * k, i[0][2] * k, i[0][3]], z, z, z]
        }
        "grain" => {
            let n = (hash3((c.uv[0] * c.res[0]) as i32, (c.uv[1] * c.res[1]) as i32, 3) - 0.5) * i[1][0];
            [[i[0][0] + n, i[0][1] + n, i[0][2] + n, i[0][3]], z, z, z]
        }
        "math" => {
            let (a, b) = (i[0][0], i[1][0]);
            let v = match pstr(&s.params, "opération") {
                "soustraire" => a - b,
                "multiplier" => a * b,
                "diviser" => {
                    if b == 0.0 {
                        0.0
                    } else {
                        a / b
                    }
                }
                "puissance" => a.abs().powf(b),
                "minimum" => a.min(b),
                "maximum" => a.max(b),
                "modulo" => a.rem_euclid(if b == 0.0 { 1.0 } else { b }),
                "sinus" => a.sin(),
                "cosinus" => a.cos(),
                "absolu" => a.abs(),
                "arrondi inf." => a.floor(),
                "fraction" => a.fract(),
                "racine" => a.max(0.0).sqrt(),
                "inférieur à" => (a < b) as u8 as f32,
                "supérieur à" => (a > b) as u8 as f32,
                "lisse (smoothstep)" => smoothstep(0.0, b.max(1e-4), a),
                _ => a + b,
            };
            [one(v), z, z, z]
        }
        "map" => {
            let t = (i[0][0] - i[1][0]) / (i[2][0] - i[1][0]).max(1e-6);
            [one(i[3][0] + t * (i[4][0] - i[3][0])), z, z, z]
        }
        "shader" => match &s.shader {
            Some(sh) => {
                let mut v = vec![0.0f32; sh.slots];
                let (a, b) = (i[0], i[1]);
                let pre = [a[0], a[1], a[2], a[3], a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3], i[2][0], c.uv[0], c.uv[1], c.nrm[0], c.nrm[1], c.nrm[2], c.pos[0], c.pos[1], c.pos[2], std::f32::consts::PI, c.res[0], c.res[1]];
                v[..pre.len()].copy_from_slice(&pre);
                sh.eval(&mut v);
                [[v[0], v[1], v[2], v[3]], one(v[12]), z, z]
            }
            None => [i[0], z, z, z],
        },
        _ => [z, z, z, z],
    }
}

// ---------- éditeur ----------

const NW: f32 = 190.0;
const ROW: f32 = 22.0;
const HEAD: f32 = 24.0;
const THUMB: f32 = 96.0;
const CATS: [&str; 7] = ["entrée", "motif", "couleur", "math", "filtre", "shader", "sortie"];

struct AddMenu {
    /// Position écran du menu et position dans le graphe du futur node
    at: Pos2,
    graph_at: [f32; 2],
    query: String,
    cat: usize,
    sel: usize,
    /// Lien en attente : la sortie qu'on glissait quand on a lâché dans le vide
    link: Option<(u32, usize)>,
}

pub struct Editor {
    pub path: PathBuf,
    root: String,
    pub graph: Graph,
    scene: Rect,
    drag_link: Option<(u32, usize)>,
    selected: std::collections::BTreeSet<u32>,
    dirty: Option<std::time::Instant>,
    program: Result<Program, (u32, String)>,
    preview: Option<egui::TextureHandle>,
    preview_key: u64,
    preview_rx: Option<std::sync::mpsc::Receiver<(u64, Img)>>,
    sphere: Option<std::sync::Arc<crate::model3d::Prepared>>,
    add: Option<AddMenu>,
    pub message: Option<String>,
    version: u64,
    /// Demande à l'IA d'écrire le code d'un node shader : (node, description)
    pub ai_request: Option<(u32, String)>,
    pub ai_rx: Option<std::sync::mpsc::Receiver<(u32, Result<String, String>)>>,
    ai_prompt: HashMap<u32, String>,
    box_start: Option<Pos2>,
    undo: Vec<Graph>,
    redo: Vec<Graph>,
    last: Graph,
    clipboard: Option<(Vec<Node>, Vec<Link>)>,
    thumbs: HashMap<u32, (u64, egui::TextureHandle)>,
    thumb_rx: Option<std::sync::mpsc::Receiver<(u64, u32, Img)>>,
    thumb_version: u64,
    rename: Option<(u32, String)>,
    frame_sel: bool,
    frame_rename: Option<(usize, String)>,
    frame_sel_request: bool,
    /// Aperçu du matériau : caméra, éclairage, modèle personnalisé
    pv_yaw: f32,
    pv_pitch: f32,
    pv_dist: f32,
    pv_env: usize,
    pv_model: Option<(String, std::sync::Arc<crate::model3d::Prepared>)>,
    pv_key: u64,
    /// Node dont la sortie remplace le matériau dans l'aperçu (« prévisualiser la node »)
    solo: Option<u32>,
}

enum Act {
    Pan(egui::Vec2),
    Deselect(u32),
    Move(egui::Vec2),
    MoveEnd,
    Connect(u32, usize, u32, usize),
    Unlink(u32, usize),
    RemoveLink(usize),
    Changed,
    Click(u32, bool),
    Clear,
    BoxSelect(Rect, bool),
    OpenAdd(Pos2, Option<(u32, usize)>),
    Node(u32, NodeOp),
}

#[derive(Clone, Copy)]
enum NodeOp {
    Duplicate,
    Delete,
    Mute,
    Collapse,
    Preview,
    Disconnect,
    Rename,
    Solo,
}

fn to_texture(ctx: &egui::Context, name: &str, img: &Img) -> egui::ColorImage {
    let _ = (ctx, name);
    let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let bytes: Vec<u8> = img.px.iter().flat_map(|c| [q(c[0]), q(c[1]), q(c[2]), q(c[3])]).collect();
    egui::ColorImage::from_rgba_unmultiplied([img.w, img.h], &bytes)
}

impl Editor {
    pub fn open(path: PathBuf, root: &str) -> Result<Editor, String> {
        let graph = Graph::load(&path)?;
        let program = Program::compile(&graph.effective(), root, 1024);
        Ok(Editor {
            path,
            root: root.to_string(),
            last: graph.clone(),
            graph,
            scene: Rect::ZERO,
            drag_link: None,
            selected: Default::default(),
            dirty: None,
            program,
            preview: None,
            preview_key: u64::MAX,
            preview_rx: None,
            sphere: None,
            add: None,
            message: None,
            version: 1,
            ai_request: None,
            ai_rx: None,
            ai_prompt: HashMap::new(),
            box_start: None,
            undo: vec![],
            redo: vec![],
            clipboard: None,
            thumbs: HashMap::new(),
            thumb_rx: None,
            thumb_version: 0,
            rename: None,
            frame_sel: false,
            frame_rename: None,
            frame_sel_request: false, pv_yaw: 0.4, pv_pitch: 0.25, pv_dist: 2.0, pv_env: 0, pv_model: None, pv_key: 0, solo: None,
        })
    }

    fn changed(&mut self) {
        self.dirty = Some(std::time::Instant::now());
        self.program = Program::compile(&self.graph.effective(), &self.root, 1024);
        self.version += 1;
        self.commit();
    }

    /// Point d'annulation si le graphe a changé depuis le dernier.
    fn commit(&mut self) {
        let a = serde_json::to_string(&self.graph).unwrap_or_default();
        let b = serde_json::to_string(&self.last).unwrap_or_default();
        if a != b {
            self.undo.push(std::mem::replace(&mut self.last, self.graph.clone()));
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }

    fn restore(&mut self, g: Graph) {
        self.graph = g;
        self.last = self.graph.clone();
        self.selected.retain(|id| self.graph.nodes.iter().any(|n| n.id == *id));
        self.dirty = Some(std::time::Instant::now());
        self.program = Program::compile(&self.graph.effective(), &self.root, 1024);
        self.version += 1;
    }

    fn undo(&mut self) {
        if let Some(g) = self.undo.pop() {
            self.redo.push(self.graph.clone());
            self.restore(g);
        }
    }

    fn redo(&mut self) {
        if let Some(g) = self.redo.pop() {
            self.undo.push(self.graph.clone());
            self.restore(g);
        }
    }

    pub fn save_if_due(&mut self, force: bool) {
        if let Some(t) = self.dirty {
            if force || t.elapsed() > std::time::Duration::from_millis(500) {
                if let Err(e) = self.graph.save(&self.path) {
                    self.message = Some(e);
                }
                self.dirty = None;
            }
        }
    }

/// Range les nodes en colonnes, de gauche (sources) à droite (sortie) : la sélection s'il y a
    /// au moins deux nodes choisis, sinon tout le graphe.
    fn arrange(&mut self) {
        let ids: Vec<u32> = if self.selected.len() >= 2 { self.selected.iter().copied().collect() } else { self.graph.nodes.iter().map(|n| n.id).collect() };
        if ids.len() < 2 {
            return;
        }
        let set: std::collections::BTreeSet<u32> = ids.iter().copied().collect();
        // Colonne = plus long chemin depuis une source
        let mut col: HashMap<u32, usize> = ids.iter().map(|&i| (i, 0)).collect();
        for _ in 0..ids.len() {
            let mut moved = false;
            for l in &self.graph.links {
                if set.contains(&l.from) && set.contains(&l.to) && col[&l.to] < col[&l.from] + 1 {
                    col.insert(l.to, col[&l.from] + 1);
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        let (mut x0, mut y0) = (f32::MAX, f32::MAX);
        for n in self.graph.nodes.iter().filter(|n| set.contains(&n.id)) {
            x0 = x0.min(n.pos[0]);
            y0 = y0.min(n.pos[1]);
        }
        // Dans chaque colonne : garder l'ordre vertical actuel
        let ncol = col.values().copied().max().unwrap_or(0) + 1;
        for c in 0..ncol {
            let mut members: Vec<(f32, u32)> = self.graph.nodes.iter().filter(|n| set.contains(&n.id) && col[&n.id] == c).map(|n| (n.pos[1], n.id)).collect();
            members.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut y = y0;
            for (_, id) in members {
                if let Some(n) = self.graph.nodes.iter_mut().find(|n| n.id == id) {
                    let h = def(&n.op).map(|d| Self::node_size(n, d)).unwrap_or(120.0);
                    n.pos = [x0 + c as f32 * 240.0, y];
                    y += h + 28.0;
                }
            }
        }
        self.changed();
    }

    fn is_output(&self, id: u32) -> bool {
        self.graph.nodes.iter().find(|n| n.id == id).is_some_and(|n| n.op == "mat_out" || n.op == "comp_out")
    }

    /// Copie les nodes choisis (avec les liens entre eux) décalés de `offset`.
    fn duplicate(&mut self, ids: &[u32], offset: [f32; 2]) -> Vec<u32> {
        let nodes: Vec<Node> = self.graph.nodes.iter().filter(|n| ids.contains(&n.id) && n.op != "mat_out" && n.op != "comp_out").cloned().collect();
        let links: Vec<Link> = self.graph.links.iter().filter(|l| ids.contains(&l.from) && ids.contains(&l.to)).cloned().collect();
        self.paste(nodes, links, offset)
    }

    fn paste(&mut self, nodes: Vec<Node>, links: Vec<Link>, offset: [f32; 2]) -> Vec<u32> {
        let mut map = HashMap::new();
        for n in &nodes {
            if def(&n.op).is_none() {
                continue;
            }
            let id = self.graph.add(&n.op, [n.pos[0] + offset[0], n.pos[1] + offset[1]]);
            if let Some(m) = self.graph.nodes.iter_mut().find(|m| m.id == id) {
                m.values = n.values.clone();
                m.params = n.params.clone();
                m.muted = n.muted;
                m.collapsed = n.collapsed;
                m.preview = n.preview;
                m.label = n.label.clone();
            }
            map.insert(n.id, id);
        }
        for l in links {
            if let (Some(&a), Some(&b)) = (map.get(&l.from), map.get(&l.to)) {
                self.graph.connect(a, l.out, b, l.inp);
            }
        }
        map.values().copied().collect()
    }

    /// Hauteur d'un node selon son type et son état.
    fn node_size(n: &Node, d: &Def) -> f32 {
        if n.collapsed {
            return HEAD;
        }
        let rows = d.outputs.len() + d.inputs.len() + d.params.iter().map(|pp| if matches!(pp, ParamDef::Text(_)) { 9 } else { 1 }).sum::<usize>();
        let thumb = if n.preview && !d.outputs.is_empty() { THUMB + 8.0 } else { 0.0 };
        HEAD + rows as f32 * ROW + 8.0 + thumb
    }

    fn bbox(&self, only: Option<&std::collections::BTreeSet<u32>>) -> Rect {
        let mut bb = Rect::NOTHING;
        for n in &self.graph.nodes {
            if only.is_some_and(|s| !s.contains(&n.id)) {
                continue;
            }
            let h = def(&n.op).map(|d| Self::node_size(n, d)).unwrap_or(100.0);
            bb = bb.union(Rect::from_min_size(pos2(n.pos[0], n.pos[1]), vec2(NW, h)));
        }
        bb
    }

    /// Calcule les miniatures en arrière-plan quand le graphe change.
    fn thumbs_tick(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.thumb_rx {
            loop {
                match rx.try_recv() {
                    Ok((v, id, img)) => {
                        let ci = to_texture(ctx, "thumb", &img);
                        match self.thumbs.get_mut(&id) {
                            Some((ver, t)) => {
                                t.set(ci, egui::TextureOptions::LINEAR);
                                *ver = v;
                            }
                            None => {
                                let t = ctx.load_texture(format!("node-thumb-{id}"), ci, egui::TextureOptions::LINEAR);
                                self.thumbs.insert(id, (v, t));
                            }
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        ctx.request_repaint_after(std::time::Duration::from_millis(80));
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.thumb_rx = None;
                        break;
                    }
                }
            }
        }
        if self.thumb_rx.is_some() || self.thumb_version == self.version {
            return;
        }
        self.thumb_version = self.version;
        let ids: Vec<(u32, usize)> = self.graph.nodes.iter().filter(|n| n.preview && !n.collapsed && def(&n.op).is_some_and(|d| !d.outputs.is_empty())).map(|n| (n.id, 0)).collect();
        if ids.is_empty() {
            return;
        }
        let g = self.graph.clone();
        let root = self.root.clone();
        let version = self.version;
        let sphere = if g.kind == GraphKind::Material { Some(self.sphere.get_or_insert_with(|| std::sync::Arc::new(crate::model3d::preview_sphere())).clone()) } else { None };
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            for (id, out) in ids {
                // Une texture se montre telle quelle, pas plaquée sur une sphère
                if let Some(n) = g.nodes.iter().find(|n| n.id == id).filter(|n| n.op == "texture" || n.op == "image") {
                    let file = pstr(&n.params, "fichier");
                    if let Some(img) = (!file.is_empty()).then(|| load_img(&Path::new(&root).join(file), 160)).flatten() {
                        if tx.send((version, id, img)).is_err() {
                            return;
                        }
                        ctx.request_repaint();
                        continue;
                    }
                }
                let Some(pg) = g.probe(id, out) else { continue };
                let Ok(prog) = Program::compile(&pg, &root, 256) else { continue };
                let img = match &sphere {
                    Some(s) => crate::model3d::render_material_preview(s, std::sync::Arc::new(prog), 96),
                    None => prog.composite(Some([144, 96])),
                };
                if tx.send((version, id, img)).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
        self.thumb_rx = Some(rx);
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let ctx = ui.ctx().clone();
        self.save_if_due(false);
        if let Some(rx) = &self.ai_rx {
            if let Ok((id, r)) = rx.try_recv() {
                match r {
                    Ok(code) => {
                        if let Some(n) = self.graph.nodes.iter_mut().find(|n| n.id == id) {
                            n.params.insert("code".into(), Param::S(code));
                        }
                        self.changed();
                        self.message = Some("shader écrit par l'IA".into());
                    }
                    Err(e) => self.message = Some(e),
                }
                self.ai_rx = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
        if self.dirty.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
        self.thumbs_tick(&ctx);
        let kind = self.graph.kind;
        // Barre
        let mut add_now: Option<&'static str> = None;
        let mut preset: Option<&'static str> = None;
        ui.horizontal(|ui| {
            egui::containers::menu::MenuButton::new(crate::i18n::t("＋ ajouter ▾")).ui(ui, |ui| {
                for cat in CATS {
                    let items: Vec<&Def> = DEFS.iter().filter(|d| d.cat == cat && if kind == GraphKind::Material { d.mat } else { d.comp }).collect();
                    if items.is_empty() {
                        continue;
                    }
                    ui.menu_button(crate::i18n::t(cat), |ui| {
                        for d in items {
                            if ui.button(crate::i18n::t(d.label)).clicked() {
                                add_now = Some(d.op);
                            }
                        }
                    });
                }
            });
            if kind == GraphKind::Material {
                egui::containers::menu::MenuButton::new(crate::i18n::t("préréglages ▾")).ui(ui, |ui| {
                    for name in MATERIAL_PRESETS {
                        if ui.button(name).on_hover_text(crate::i18n::t("remplace le graphe actuel")).clicked() {
                            preset = Some(name);
                        }
                    }
                });
            }
            if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("↶")).on_hover_text(crate::i18n::t("annuler (Ctrl+Z)")).clicked() {
                self.undo();
            }
            if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("↷")).on_hover_text(crate::i18n::t("rétablir (Ctrl+Maj+Z)")).clicked() {
                self.redo();
            }
            egui::containers::menu::MenuButton::new(crate::i18n::t("aide ▾")).ui(ui, |ui| {
                ui.label(RichText::new(crate::i18n::t("Maj+A ou clic droit : ajouter\nglisser dans le vide : sélection en rectangle (Maj : ajouter)\nclic molette ou clic droit glissé : se déplacer · molette : zoom\nglisser une sortie dans le vide : ajouter un node déjà relié\nCtrl+clic sur un lien : le couper\nA : tout · Alt+A : rien · X / Suppr : effacer\nMaj+D : dupliquer · Ctrl+C / Ctrl+V : copier, coller\nM : muet · H : réduire · P : miniature · F2 : renommer\nF : cadrer la sélection · Origine : tout cadrer\nCtrl+Z / Ctrl+Maj+Z : annuler, rétablir\nGlisser des images depuis l'ordinateur : nodes texture")).size(11.0));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if kind == GraphKind::Compositor && ui.button(crate::i18n::t("exporter PNG")).clicked() {
                    self.export();
                }
                if ui.button(crate::i18n::t("recadrer")).clicked() {
                    self.scene = Rect::ZERO;
                }
                let all_on = self.graph.nodes.iter().filter(|n| n.op != "mat_out" && n.op != "comp_out").all(|n| n.preview);
                if ui.selectable_label(all_on, crate::i18n::t("miniatures")).on_hover_text(crate::i18n::t("aperçu sous chaque node")).clicked() {
                    for n in self.graph.nodes.iter_mut().filter(|n| n.op != "mat_out" && n.op != "comp_out") {
                        n.preview = !all_on;
                    }
                    self.changed();
                }
                if let Some(m) = &self.message {
                    ui.label(RichText::new(m).color(p.muted).size(11.0));
                }
            });
        });
        if let Some(g) = preset.and_then(material_preset) {
            self.graph = g;
            self.scene = Rect::ZERO;
            self.changed();
        }
        if let Err((_, e)) = &self.program {
            ui.label(RichText::new(format!("⚠ {e}")).color(p.missing).size(11.5));
        }
        // Aperçu à gauche, graphe à droite
        let avail = ui.available_rect_before_wrap();
        let pw = (avail.width() * 0.3).clamp(200.0, 440.0);
        let prev_rect = Rect::from_min_size(avail.min, vec2(pw, pw.min(avail.height())));
        let graph_rect = Rect::from_min_max(pos2(avail.min.x + pw + 8.0, avail.min.y), avail.max);
        self.preview_ui(ui, prev_rect, &ctx);
        ui.painter().rect_filled(graph_rect, 0.0, p.panel);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(graph_rect));
        let over_graph = ctx.pointer_latest_pos().is_some_and(|pt| graph_rect.contains(pt));
        // Cadrage : tout (premier affichage, « recadrer », Origine) ou la sélection (F)
        let frame_all = self.scene == Rect::ZERO || !self.scene.is_finite();
        if frame_all || std::mem::take(&mut self.frame_sel) {
            let mut bb = if frame_all || self.selected.is_empty() { self.bbox(None) } else { self.bbox(Some(&self.selected)) };
            if !bb.is_finite() {
                bb = Rect::from_min_size(Pos2::ZERO, vec2(600.0, 400.0));
            }
            let bb = bb.expand(40.0);
            let aspect = graph_rect.width() / graph_rect.height().max(1.0);
            let (mut w, mut h) = (bb.width(), bb.height());
            if w / h < aspect {
                w = h * aspect;
            } else {
                h = w / aspect;
            }
            if w < graph_rect.width() {
                w = graph_rect.width();
                h = graph_rect.height();
            }
            self.scene = Rect::from_center_size(bb.center(), vec2(w, h));
        }
        let mut scene = self.scene;
        let mut pan = egui::Vec2::ZERO;
        let mut actions: Vec<Act> = vec![];
        egui::Scene::new().zoom_range(0.2..=2.5).drag_pan_buttons(egui::DragPanButtons::MIDDLE | egui::DragPanButtons::SECONDARY).show(&mut child, &mut scene, |ui| {
            self.draw_graph(ui, &mut actions);
        });
        self.scene = scene;
        let sc = self.scene;
        let to_graph = move |pt: Pos2| -> [f32; 2] {
            if !sc.is_finite() || sc.width() <= 0.0 {
                return [0.0, 0.0];
            }
            let k = sc.width() / graph_rect.width();
            [sc.min.x + (pt.x - graph_rect.min.x) * k, sc.min.y + (pt.y - graph_rect.min.y) * k]
        };
        let typing = ctx.egui_wants_keyboard_input();
        // Raccourcis (souris sur le graphe)
        if over_graph && !typing && self.add.is_none() {
            let (shift, cmd, alt) = ctx.input(|i| (i.modifiers.shift, i.modifiers.command, i.modifiers.alt));
            let key = |k: egui::Key| ctx.input(|i| i.key_pressed(k));
            if shift && key(egui::Key::A) {
                if let Some(pt) = ctx.pointer_latest_pos() {
                    self.open_add(pt, to_graph(pt), None);
                }
            } else if alt && key(egui::Key::A) {
                self.selected.clear();
            } else if !cmd && key(egui::Key::A) {
                self.selected = self.graph.nodes.iter().map(|n| n.id).collect();
            }
            if key(egui::Key::Delete) || key(egui::Key::X) && !cmd {
                for id in self.selected.clone() {
                    if !self.is_output(id) {
                        self.graph.remove(id);
                    }
                }
                self.selected.clear();
                self.changed();
            }
            if (shift && key(egui::Key::D)) || (cmd && key(egui::Key::D)) {
                let ids: Vec<u32> = self.selected.iter().copied().collect();
                let new = self.duplicate(&ids, [30.0, 30.0]);
                self.selected = new.into_iter().collect();
                self.changed();
            }
            if cmd && key(egui::Key::C) {
                let ids: Vec<u32> = self.selected.iter().copied().collect();
                let nodes: Vec<Node> = self.graph.nodes.iter().filter(|n| ids.contains(&n.id) && n.op != "mat_out" && n.op != "comp_out").cloned().collect();
                let links: Vec<Link> = self.graph.links.iter().filter(|l| ids.contains(&l.from) && ids.contains(&l.to)).cloned().collect();
                self.message = Some(format!("{} node(s) copié(s)", nodes.len()));
                self.clipboard = Some((nodes, links));
            }
            if cmd && key(egui::Key::V) {
                if let Some((nodes, links)) = self.clipboard.clone() {
                    let at = ctx.pointer_latest_pos().map(to_graph).unwrap_or([0.0, 0.0]);
                    let min = nodes.iter().fold([f32::MAX; 2], |m, n| [m[0].min(n.pos[0]), m[1].min(n.pos[1])]);
                    let new = self.paste(nodes, links, [at[0] - min[0], at[1] - min[1]]);
                    self.selected = new.into_iter().collect();
                    self.changed();
                }
            }
            if cmd && key(egui::Key::Z) {
                if shift {
                    self.redo();
                } else {
                    self.undo();
                }
            }
            if cmd && key(egui::Key::Y) {
                self.redo();
            }
            if !cmd && key(egui::Key::Q) {
                self.arrange();
            }
            for (k, op) in [(egui::Key::M, NodeOp::Mute), (egui::Key::H, NodeOp::Collapse), (egui::Key::P, NodeOp::Preview)] {
                if !cmd && !shift && key(k) {
                    for id in self.selected.clone() {
                        actions.push(Act::Node(id, op));
                    }
                }
            }
            if (std::mem::take(&mut self.frame_sel_request) || (cmd && key(egui::Key::J))) && !self.selected.is_empty() {
                let bb = self.bbox(Some(&self.selected)).expand(24.0);
                let n = self.graph.frames.len();
                let cols = [[90u8, 120, 200], [80, 160, 110], [200, 140, 60], [170, 90, 170], [200, 80, 80]];
                self.graph.frames.push(Frame { rect: [bb.min.x, bb.min.y - 26.0, bb.width(), bb.height() + 26.0], label: format!("{} {}", crate::i18n::t("cadre"), n + 1), color: cols[n % cols.len()] });
                self.changed();
            }
            if key(egui::Key::F2) {
                if let Some(&id) = self.selected.iter().next() {
                    actions.push(Act::Node(id, NodeOp::Rename));
                }
            }
            if !cmd && key(egui::Key::F) {
                self.frame_sel = true;
            }
            if key(egui::Key::Home) {
                self.scene = Rect::ZERO;
            }
        }
        // Images glissées depuis l'ordinateur : un node texture / image par fichier
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if !dropped.is_empty() && over_graph {
            let mut at = ctx.pointer_latest_pos().map(to_graph).unwrap_or([0.0, 0.0]);
            let op = if kind == GraphKind::Material { "texture" } else { "image" };
            let mut added = vec![];
            for f in dropped {
                let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                if !["png", "jpg", "jpeg", "webp", "tga", "tif", "tiff", "bmp", "exr", "hdr", "gif"].contains(&ext.as_str()) {
                    self.message = Some(format!("{} : seules les images vont dans un graphe", f.display()));
                    continue;
                }
                let rel = crate::vault::inside(&self.root, &f).map(Ok).unwrap_or_else(|| crate::vault::link_external(&self.root, &f));
                match rel {
                    Ok(rel) => {
                        let id = self.graph.add(op, at);
                        if let Some(n) = self.graph.nodes.iter_mut().find(|n| n.id == id) {
                            n.params.insert("fichier".into(), Param::S(rel));
                        }
                        added.push(id);
                        at[1] += 220.0;
                    }
                    Err(e) => self.message = Some(e),
                }
            }
            if !added.is_empty() {
                self.selected = added.into_iter().collect();
                self.changed();
            }
        }
        if let Some(op) = add_now {
            let at = to_graph(graph_rect.center());
            let id = self.graph.add(op, at);
            self.selected = [id].into_iter().collect();
            self.changed();
        }
        // Actions du graphe
        let mut changed = false;
        let mut solo_toggle: Option<u32> = None;
        let mut moved = false;
        for a in actions {
            match a {
                Act::Move(d) => {
                    let snap = ctx.input(|i| i.modifiers.command);
                    for n in self.graph.nodes.iter_mut().filter(|n| self.selected.contains(&n.id)) {
                        n.pos[0] += d.x;
                        n.pos[1] += d.y;
                        if snap {
                            n.pos[0] = (n.pos[0] / 20.0).round() * 20.0;
                            n.pos[1] = (n.pos[1] / 20.0).round() * 20.0;
                        }
                    }
                    self.dirty = Some(std::time::Instant::now());
                }
                Act::MoveEnd => moved = true,
                Act::Connect(a, o, b, i) => {
                    self.graph.connect(a, o, b, i);
                    changed = true;
                }
                Act::Unlink(b, i) => {
                    self.graph.links.retain(|l| !(l.to == b && l.inp == i));
                    changed = true;
                }
                Act::RemoveLink(k) => {
                    if k < self.graph.links.len() {
                        self.graph.links.remove(k);
                        changed = true;
                    }
                }
                Act::Changed => changed = true,
                Act::Click(id, add) => {
                    if add {
                        if !self.selected.remove(&id) {
                            self.selected.insert(id);
                        }
                    } else if !self.selected.contains(&id) {
                        self.selected = [id].into_iter().collect();
                    }
                }
                Act::Clear => self.selected.clear(),
                Act::Deselect(id) => {
                    self.selected.remove(&id);
                }
                Act::Pan(d) => pan += d,
                Act::BoxSelect(r, add) => {
                    if !add {
                        self.selected.clear();
                    }
                    for n in &self.graph.nodes {
                        let h = def(&n.op).map(|d| Self::node_size(n, d)).unwrap_or(100.0);
                        if r.intersects(Rect::from_min_size(pos2(n.pos[0], n.pos[1]), vec2(NW, h))) {
                            self.selected.insert(n.id);
                        }
                    }
                }
                Act::OpenAdd(gp, link) => {
                    // gp : position dans le graphe ; le menu s'ouvre sous la souris
                    let screen = ctx.pointer_latest_pos().unwrap_or(graph_rect.center());
                    self.open_add(screen, [gp.x, gp.y], link);
                }
                Act::Node(id, op) => {
                    match op {
                        NodeOp::Duplicate => {
                            let new = self.duplicate(&[id], [30.0, 30.0]);
                            self.selected = new.into_iter().collect();
                        }
                        NodeOp::Delete => {
                            if !self.is_output(id) {
                                self.graph.remove(id);
                                self.selected.remove(&id);
                            }
                        }
                        NodeOp::Disconnect => self.graph.links.retain(|l| l.from != id && l.to != id),
                        NodeOp::Rename => {
                            let cur = self.graph.nodes.iter().find(|n| n.id == id).map(|n| n.label.clone()).unwrap_or_default();
                            self.rename = Some((id, cur));
                        }
                        _ => {
                            if let Some(n) = self.graph.nodes.iter_mut().find(|n| n.id == id) {
                                match op {
                                    NodeOp::Mute => {
                                        if n.op != "mat_out" && n.op != "comp_out" {
                                            n.muted = !n.muted;
                                        }
                                    }
                                    NodeOp::Collapse => n.collapsed = !n.collapsed,
                                    NodeOp::Preview => n.preview = !n.preview,
                                    NodeOp::Solo => solo_toggle = Some(id),
                                    _ => {}
                                }
                            }
                        }
                    }
                    changed = true;
                }
            }
        }
        if let Some(id) = solo_toggle {
            self.solo = if self.solo == Some(id) { None } else { Some(id) };
            self.pv_key = 0;
        }
        if pan != egui::Vec2::ZERO {
            self.scene = self.scene.translate(-pan);
        }
        if changed {
            self.changed();
        } else if moved {
            self.commit();
        }
        self.add_menu_ui(&ctx);
        ui.allocate_rect(avail, Sense::hover());
    }

    fn open_add(&mut self, screen: Pos2, graph_at: [f32; 2], link: Option<(u32, usize)>) {
        self.add = Some(AddMenu { at: screen, graph_at, query: String::new(), cat: 0, sel: 0, link });
    }

    /// Menu d'ajout façon Blender : catégories à gauche (survol), nodes à droite, recherche.
    fn add_menu_ui(&mut self, ctx: &egui::Context) {
        let p = theme::pal();
        let kind = self.graph.kind;
        let Some(m) = &mut self.add else { return };
        let mut chosen: Option<&'static str> = None;
        let mut close = false;
        let usable = |d: &&Def| if kind == GraphKind::Material { d.mat } else { d.comp };
        // Avec un lien en attente : seulement les nodes qui ont une entrée
        let need_input = m.link.is_some();
        let area = egui::Area::new(egui::Id::unique("node-add")).order(egui::Order::Foreground).fixed_pos(m.at).show(ctx, |ui| {
            egui::Frame::new().fill(p.bg).stroke(Stroke::new(1.0, p.text)).inner_margin(6).show(ui, |ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut m.query).hint_text(crate::i18n::t("chercher un node…")).desired_width(360.0));
                r.request_focus();
                let q = crate::chat::fold(&m.query);
                if q.is_empty() {
                    ui.horizontal_top(|ui| {
                        // Catégories
                        ui.vertical(|ui| {
                            ui.set_width(110.0);
                            for (ci, cat) in CATS.iter().enumerate() {
                                let n = DEFS.iter().filter(usable).filter(|d| d.cat == *cat && (!need_input || !d.inputs.is_empty())).count();
                                if n == 0 {
                                    continue;
                                }
                                let active = m.cat == ci;
                                let r = ui.add(egui::Button::new(RichText::new(format!("{}  ›", crate::i18n::t(cat))).color(if active { p.on_accent } else { p.text })).fill(if active { p.accent } else { Color32::TRANSPARENT }).stroke(Stroke::NONE).min_size(vec2(110.0, 22.0)));
                                if r.hovered() || r.clicked() {
                                    if m.cat != ci {
                                        m.sel = 0;
                                    }
                                    m.cat = ci;
                                }
                            }
                        });
                        ui.separator();
                        ui.vertical(|ui| {
                            ui.set_width(220.0);
                            let items: Vec<&Def> = DEFS.iter().filter(usable).filter(|d| d.cat == CATS[m.cat] && (!need_input || !d.inputs.is_empty())).collect();
                            for (k, d) in items.iter().enumerate() {
                                let hi = k == m.sel;
                                let r = ui.add(egui::Button::new(RichText::new(crate::i18n::t(d.label)).color(if hi { p.accent } else { p.text })).fill(if hi { p.line.gamma_multiply(0.6) } else { Color32::TRANSPARENT }).stroke(Stroke::NONE).min_size(vec2(220.0, 22.0)));
                                if r.hovered() {
                                    m.sel = k;
                                }
                                if r.clicked() {
                                    chosen = Some(d.op);
                                }
                            }
                            let n = items.len();
                            if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) && n > 0 {
                                m.sel = (m.sel + 1) % n;
                            }
                            if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) && n > 0 {
                                m.sel = (m.sel + n - 1) % n;
                            }
                            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                chosen = items.get(m.sel).map(|d| d.op);
                            }
                        });
                    });
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                        m.cat = (m.cat + 1) % CATS.len();
                        m.sel = 0;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                        m.cat = (m.cat + CATS.len() - 1) % CATS.len();
                        m.sel = 0;
                    }
                } else {
                    // Chaque mot doit se retrouver dans le nom (français ou anglais), la catégorie ou le
                    // code du node ; les meilleurs d'abord : début de mot, puis milieu, puis lettres dans l'ordre.
                    let scored = search_defs(&q, kind == GraphKind::Material, need_input);
                    let list: Vec<&Def> = scored.into_iter().take(16).map(|x| x.1).collect();
                    if m.sel >= list.len() {
                        m.sel = 0;
                    }
                    for (k, d) in list.iter().enumerate() {
                        let hi = k == m.sel;
                        let r = ui.add(egui::Button::new(RichText::new(format!("{}   {}", crate::i18n::t(d.label), crate::i18n::t(d.cat))).color(if hi { p.accent } else { p.text })).fill(if hi { p.line.gamma_multiply(0.6) } else { Color32::TRANSPARENT }).stroke(Stroke::NONE).min_size(vec2(360.0, 22.0)));
                        if r.hovered() {
                            m.sel = k;
                        }
                        if r.clicked() {
                            chosen = Some(d.op);
                        }
                    }
                    if list.is_empty() {
                        ui.label(RichText::new(crate::i18n::t("aucun node")).color(p.muted));
                    }
                    let n = list.len();
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) && n > 0 {
                        m.sel = (m.sel + 1) % n;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) && n > 0 {
                        m.sel = (m.sel + n - 1) % n;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        chosen = list.get(m.sel).map(|d| d.op);
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close = true;
                }
            });
        });
        let (graph_at, link) = (m.graph_at, m.link);
        let outside = ctx.input(|i| i.pointer.any_pressed()) && !ctx.pointer_latest_pos().is_some_and(|pt| area.response.rect.contains(pt));
        if let Some(op) = chosen {
            let id = self.graph.add(op, graph_at);
            if let Some((from, out)) = link {
                self.graph.connect(from, out, id, 0);
            }
            self.selected = [id].into_iter().collect();
            self.add = None;
            self.changed();
        } else if close || outside {
            self.add = None;
        }
    }

    fn draw_graph(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Act>) {
        let p = theme::pal();
        let painter = ui.painter().clone();
        let err_node = self.program.as_ref().err().map(|e| e.0);
        // Le graphe est dans un calque zoomé : on ramène le pointeur dans ses coordonnées.
        let from_global = ui.ctx().layer_transform_from_global(ui.layer_id());
        let pointer = ui.input(|i| i.pointer.latest_pos()).map(|pt| from_global.map(|t| t * pt).unwrap_or(pt));
        let (shift, cmd) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
        // Fond : clic = rien, glisser = rectangle, clic droit = menu
        let bg_rect = Rect::from_center_size(Pos2::ZERO, vec2(1e7, 1e7));
        let bg = ui.interact(bg_rect, ui.scope_id().with("node-bg"), Sense::click_and_drag());
        // Grille de points
        let clip = ui.clip_rect();
        let step = 40.0;
        if clip.width() / step < 220.0 {
            let mut x = (clip.left() / step).floor() * step;
            while x < clip.right() {
                let mut y = (clip.top() / step).floor() * step;
                while y < clip.bottom() {
                    painter.circle_filled(pos2(x, y), 1.2, p.line);
                    y += step;
                }
                x += step;
            }
        }
        if bg.dragged_by(egui::PointerButton::Middle) {
            actions.push(Act::Pan(bg.drag_delta()));
        }
        if bg.drag_started_by(egui::PointerButton::Primary) {
            self.box_start = pointer;
        }
        if let (Some(a), Some(b)) = (self.box_start, pointer) {
            let r = Rect::from_two_pos(a, b);
            painter.rect_filled(r, 0.0, p.accent.gamma_multiply(0.08));
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
            if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
                actions.push(Act::BoxSelect(r, shift));
                self.box_start = None;
            }
        }
        if bg.clicked() && !shift {
            actions.push(Act::Clear);
        }
        bg.context_menu(|ui| {
            if ui.button(crate::i18n::t("ajouter un node…   Maj+A")).clicked() {
                if let Some(pt) = pointer {
                    actions.push(Act::OpenAdd(pt, None));
                }
                ui.close();
            }
            if ui.add_enabled(self.clipboard.is_some(), egui::Button::new(crate::i18n::t("coller   Ctrl+V"))).clicked() {
                if let (Some((nodes, links)), Some(pt)) = (self.clipboard.clone(), pointer) {
                    let min = nodes.iter().fold([f32::MAX; 2], |m, n| [m[0].min(n.pos[0]), m[1].min(n.pos[1])]);
                    let new = self.paste(nodes, links, [pt.x - min[0], pt.y - min[1]]);
                    self.selected = new.into_iter().collect();
                    actions.push(Act::Changed);
                }
                ui.close();
            }
            if ui.button(crate::i18n::t("tout sélectionner   A")).clicked() {
                self.selected = self.graph.nodes.iter().map(|n| n.id).collect();
                ui.close();
            }
            if !self.selected.is_empty() && ui.button(crate::i18n::t("cadre autour de la sélection   Ctrl+J")).clicked() {
                self.frame_sel_request = true;
                ui.close();
            }
            if ui.button(crate::i18n::t("tout cadrer   Origine")).clicked() {
                self.scene = Rect::ZERO;
                ui.close();
            }
        });
        // Cadres (derrière les nodes) : glisser le titre déplace le cadre et ce qu'il contient
        let mut frame_del = None;
        for k in 0..self.graph.frames.len() {
            let fr = self.graph.frames[k].clone();
            let r = Rect::from_min_size(pos2(fr.rect[0], fr.rect[1]), vec2(fr.rect[2], fr.rect[3]));
            let col = Color32::from_rgb(fr.color[0], fr.color[1], fr.color[2]);
            painter.rect_filled(r, 2.0, col.gamma_multiply(0.12));
            painter.rect_stroke(r, 2.0, Stroke::new(1.0, col.gamma_multiply(0.6)), egui::StrokeKind::Inside);
            let title = Rect::from_min_size(r.min, vec2(r.width(), 26.0));
            painter.rect_filled(title, 2.0, col.gamma_multiply(0.35));
            match &mut self.frame_rename {
                Some((fk, text)) if *fk == k => {
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(title.shrink(3.0)));
                    let te = child.add(egui::TextEdit::singleline(text).desired_width(title.width() - 10.0));
                    te.request_focus();
                    if te.lost_focus() || child.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.graph.frames[k].label = text.trim().to_string();
                        self.frame_rename = None;
                        actions.push(Act::Changed);
                    }
                }
                _ => {
                    painter.text(title.left_center() + vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, &fr.label, egui::FontId::monospace(13.0), p.text);
                }
            }
            let tr = ui.interact(title, ui.scope_id().with(("frame-title", k)), Sense::click_and_drag());
            if tr.dragged_by(egui::PointerButton::Primary) {
                let d = tr.drag_delta();
                let inside: Vec<u32> = self.graph.nodes.iter().filter(|n| r.contains(pos2(n.pos[0] + 20.0, n.pos[1] + 12.0))).map(|n| n.id).collect();
                for n in self.graph.nodes.iter_mut().filter(|n| inside.contains(&n.id)) {
                    n.pos[0] += d.x;
                    n.pos[1] += d.y;
                }
                let f = &mut self.graph.frames[k];
                f.rect[0] += d.x;
                f.rect[1] += d.y;
                self.dirty = Some(std::time::Instant::now());
            }
            if tr.drag_stopped() {
                actions.push(Act::MoveEnd);
            }
            if tr.double_clicked() {
                self.frame_rename = Some((k, fr.label.clone()));
            }
            tr.context_menu(|ui| {
                if ui.button(crate::i18n::t("renommer le cadre")).clicked() {
                    self.frame_rename = Some((k, fr.label.clone()));
                    ui.close();
                }
                ui.horizontal(|ui| {
                    for c in [[90u8, 120, 200], [80, 160, 110], [200, 140, 60], [170, 90, 170], [200, 80, 80], [120, 120, 120]] {
                        let (cr, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                        ui.painter().rect_filled(cr, 2.0, Color32::from_rgb(c[0], c[1], c[2]));
                        if resp.clicked() {
                            self.graph.frames[k].color = c;
                            actions.push(Act::Changed);
                        }
                    }
                });
                if ui.button(crate::i18n::t("supprimer le cadre")).clicked() {
                    frame_del = Some(k);
                    ui.close();
                }
            });
            // Poignée de redimensionnement
            let handle = Rect::from_min_size(r.max - vec2(14.0, 14.0), vec2(14.0, 14.0));
            painter.line_segment([handle.right_top() + vec2(-2.0, 4.0), handle.left_bottom() + vec2(4.0, -2.0)], Stroke::new(1.0, col));
            let hr = ui.interact(handle, ui.scope_id().with(("frame-size", k)), Sense::drag());
            if hr.dragged() {
                let d = hr.drag_delta();
                let f = &mut self.graph.frames[k];
                f.rect[2] = (f.rect[2] + d.x).max(80.0);
                f.rect[3] = (f.rect[3] + d.y).max(60.0);
                self.dirty = Some(std::time::Instant::now());
            }
            if hr.drag_stopped() {
                actions.push(Act::MoveEnd);
            }
        }
        if let Some(k) = frame_del {
            self.graph.frames.remove(k);
            actions.push(Act::Changed);
        }
        // Mise en page
        let mut out_pos: HashMap<(u32, usize), Pos2> = HashMap::new();
        let mut in_pos: HashMap<(u32, usize), Pos2> = HashMap::new();
        let layout: Vec<(u32, Rect, &'static Def)> = self.graph.nodes.iter().filter_map(|n| def(&n.op).map(|d| (n.id, Rect::from_min_size(pos2(n.pos[0], n.pos[1]), vec2(NW, Self::node_size(n, d))), d))).collect();
        let collapsed: std::collections::HashSet<u32> = self.graph.nodes.iter().filter(|n| n.collapsed).map(|n| n.id).collect();
        for (id, r, d) in &layout {
            let c = collapsed.contains(id);
            for k in 0..d.outputs.len() {
                out_pos.insert((*id, k), if c { pos2(r.right(), r.center().y) } else { pos2(r.right(), r.top() + HEAD + (k as f32 + 0.5) * ROW) });
            }
            for k in 0..d.inputs.len() {
                in_pos.insert((*id, k), if c { pos2(r.left(), r.center().y) } else { pos2(r.left(), r.top() + HEAD + (d.outputs.len() + k) as f32 * ROW + ROW * 0.5) });
            }
        }
        let sock_of = |id: u32, out: usize| -> Sock { layout.iter().find(|x| x.0 == id).and_then(|x| x.2.outputs.get(out)).map(|o| o.1).unwrap_or(Sock::Float) };
        // Liens (survol : surlignés ; Ctrl+clic ou clic droit : coupés)
        let bez = |a: Pos2, b: Pos2| -> [Pos2; 4] {
            let dx = ((b.x - a.x).abs() * 0.5).max(40.0);
            [a, a + vec2(dx, 0.0), b - vec2(dx, 0.0), b]
        };
        let near = |pts: &[Pos2; 4], pt: Pos2| -> bool {
            (0..=24).any(|i| {
                let t = i as f32 / 24.0;
                let u = 1.0 - t;
                let q = pts[0].to_vec2() * (u * u * u) + pts[1].to_vec2() * (3.0 * u * u * t) + pts[2].to_vec2() * (3.0 * u * t * t) + pts[3].to_vec2() * (t * t * t);
                q.to_pos2().distance(pt) < 6.0
            })
        };
        let mut hovered_link = None;
        for (k, l) in self.graph.links.iter().enumerate() {
            if let (Some(a), Some(b)) = (out_pos.get(&(l.from, l.out)), in_pos.get(&(l.to, l.inp))) {
                let pts = bez(*a, *b);
                let hov = self.drag_link.is_none() && hovered_link.is_none() && pointer.is_some_and(|pt| near(&pts, pt));
                if hov {
                    hovered_link = Some(k);
                }
                let muted = self.graph.nodes.iter().any(|n| (n.id == l.from || n.id == l.to) && n.muted);
                let col = if hov { p.accent } else if muted { p.muted.gamma_multiply(0.5) } else { sock_color(sock_of(l.from, l.out)).gamma_multiply(0.85) };
                painter.add(egui::epaint::CubicBezierShape::from_points_stroke(pts, false, Color32::TRANSPARENT, Stroke::new(if hov { 3.0 } else { 2.0 }, col)));
            }
        }
        if let Some(k) = hovered_link {
            let (clicked, rclick) = ui.input(|i| (i.pointer.button_clicked(egui::PointerButton::Primary), i.pointer.button_clicked(egui::PointerButton::Secondary)));
            if (clicked && cmd) || rclick {
                actions.push(Act::RemoveLink(k));
            }
            if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
                actions.push(Act::RemoveLink(k));
            }
        }
        // Nodes
        let mut hovered_in: Option<(u32, usize)> = None;
        let ids: Vec<u32> = self.graph.nodes.iter().map(|n| n.id).collect();
        let mut rename_done = None;
        for (id, r, d) in &layout {
            let ni = ids.iter().position(|x| x == id).unwrap();
            let selected = self.selected.contains(id);
            let (muted, is_collapsed, preview_on) = (self.graph.nodes[ni].muted, self.graph.nodes[ni].collapsed, self.graph.nodes[ni].preview);
            let cat_col = match d.cat {
                "sortie" => p.accent,
                "entrée" => Color32::from_rgb(70, 130, 200),
                "motif" => Color32::from_rgb(140, 100, 190),
                "couleur" => Color32::from_rgb(200, 150, 50),
                "filtre" => Color32::from_rgb(60, 160, 120),
                "shader" => Color32::from_rgb(210, 70, 110),
                _ => Color32::from_rgb(120, 120, 130),
            };
            let cat_col = if muted { Color32::from_gray(130) } else { cat_col };
            let hover_node = pointer.is_some_and(|pt| r.contains(pt));
            painter.rect_filled(r.translate(vec2(3.0, 3.0)), 0.0, Color32::from_black_alpha(30));
            painter.rect_filled(*r, 0.0, if muted { p.panel } else { p.bg });
            // Tout le node est cliquable (déclaré avant ses réglages et ses prises, qui gardent la priorité)
            let body = ui.interact(*r, ui.scope_id().with(("node-body", *id)), Sense::click_and_drag());
            let head = Rect::from_min_size(r.min, vec2(r.width(), HEAD));
            painter.rect_filled(head, 0.0, cat_col.gamma_multiply(0.85));
            // Flèche réduire, titre, miniature
            let arrow = Rect::from_min_size(head.min, vec2(20.0, HEAD));
            let ar = ui.interact(arrow, ui.scope_id().with(("node-arrow", *id)), Sense::click());
            painter.text(arrow.center(), egui::Align2::CENTER_CENTER, if is_collapsed { "▸" } else { "▾" }, egui::FontId::monospace(11.0), Color32::WHITE);
            if ar.clicked() {
                actions.push(Act::Node(*id, NodeOp::Collapse));
            }
            let has_out = !d.outputs.is_empty();
            let eye = Rect::from_min_size(pos2(head.right() - 22.0, head.top()), vec2(22.0, HEAD));
            if has_out && d.cat != "sortie" {
                let er = ui.interact(eye, ui.scope_id().with(("node-eye", *id)), Sense::click()).on_hover_text(crate::i18n::t("miniature (P)"));
                painter.text(eye.center(), egui::Align2::CENTER_CENTER, if preview_on { "◉" } else { "○" }, egui::FontId::monospace(11.0), Color32::WHITE);
                if er.clicked() {
                    actions.push(Act::Node(*id, NodeOp::Preview));
                }
            }
            let title_rect = Rect::from_min_max(pos2(arrow.right(), head.top()), pos2(eye.left(), head.bottom()));
            let label = &self.graph.nodes[ni].label;
            let title = if label.is_empty() { crate::i18n::t(d.label) } else { label.clone() };
            let title = if muted { format!("{title} (muet)") } else { title };
            match &mut self.rename {
                Some((rid, text)) if rid == id => {
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(title_rect.shrink(2.0)));
                    let te = child.add(egui::TextEdit::singleline(text).desired_width(title_rect.width() - 6.0).font(egui::FontId::monospace(11.0)));
                    te.request_focus();
                    if te.lost_focus() || child.input(|i| i.key_pressed(egui::Key::Enter)) {
                        rename_done = Some((*id, text.clone()));
                    }
                }
                _ => {
                    painter.with_clip_rect(title_rect).text(title_rect.left_center(), egui::Align2::LEFT_CENTER, title, egui::FontId::monospace(12.0), Color32::WHITE);
                }
            }
            let border = if err_node == Some(*id) {
                Stroke::new(2.0, p.missing)
            } else if selected {
                Stroke::new(2.0, p.accent)
            } else if hover_node {
                Stroke::new(1.5, p.muted)
            } else {
                Stroke::new(1.0, p.line)
            };
            painter.rect_stroke(*r, 0.0, border, egui::StrokeKind::Outside);
            let alt = ui.input(|i| i.modifiers.alt);
            let hr = ui.interact(title_rect, ui.scope_id().with(("node-head", *id)), Sense::click_and_drag());
            for resp in [&hr, &body] {
                if resp.clicked() && alt {
                    actions.push(Act::Deselect(*id));
                } else if resp.drag_started_by(egui::PointerButton::Primary) || resp.clicked() {
                    actions.push(Act::Click(*id, shift));
                }
                // Clic droit : sélectionne le node s'il ne l'était pas, puis ouvre le menu
                if resp.secondary_clicked() {
                    actions.push(Act::Click(*id, false));
                }
                if resp.dragged_by(egui::PointerButton::Primary) {
                    actions.push(Act::Move(resp.drag_delta()));
                }
                if resp.dragged_by(egui::PointerButton::Middle) {
                    actions.push(Act::Pan(resp.drag_delta()));
                }
                if resp.drag_stopped() {
                    actions.push(Act::MoveEnd);
                }
            }
            if hr.double_clicked() {
                actions.push(Act::Node(*id, NodeOp::Rename));
            }
            let solo_on = self.solo == Some(*id);
            let menu = |ui: &mut egui::Ui, actions: &mut Vec<Act>| {
                for (label, op) in [
                    ("renommer   F2", NodeOp::Rename),
                    ("dupliquer   Maj+D", NodeOp::Duplicate),
                    (if muted { "réactiver   M" } else { "rendre muet   M" }, NodeOp::Mute),
                    (if is_collapsed { "déplier   H" } else { "réduire   H" }, NodeOp::Collapse),
                    (if preview_on { "cacher la miniature   P" } else { "montrer la miniature   P" }, NodeOp::Preview),
                    (if solo_on { "revenir au matériau complet" } else { "prévisualiser la node" }, NodeOp::Solo),
                    ("déconnecter", NodeOp::Disconnect),
                    ("supprimer   X", NodeOp::Delete),
                ] {
                    if ui.button(crate::i18n::t(label)).clicked() {
                        actions.push(Act::Node(*id, op));
                        ui.close();
                    }
                }
            };
            hr.context_menu(|ui| menu(ui, actions));
            body.context_menu(|ui| menu(ui, actions));
            // Prises (toujours, même réduit)
            for (k, (name, sock)) in d.outputs.iter().enumerate() {
                let sp = out_pos[&(*id, k)];
                if !is_collapsed {
                    painter.text(pos2(r.right() - 12.0, sp.y), egui::Align2::RIGHT_CENTER, crate::i18n::t(name), egui::FontId::monospace(11.0), p.text);
                }
                let hov = pointer.is_some_and(|pt| sp.distance(pt) < 9.0);
                painter.circle(sp, if hov { 6.5 } else { 5.0 }, sock_color(*sock), Stroke::new(1.0, p.bg));
                let sr = ui.interact(Rect::from_center_size(sp, vec2(16.0, 16.0)), ui.scope_id().with(("out", *id, k)), Sense::drag());
                if sr.drag_started() {
                    self.drag_link = Some((*id, k));
                }
            }
            let node = &mut self.graph.nodes[ni];
            for (k, (name, sock, _)) in d.inputs.iter().enumerate() {
                let sp = in_pos[&(*id, k)];
                let linked = self.graph.links.iter().any(|l| l.to == *id && l.inp == k);
                let hov = pointer.is_some_and(|pt| sp.distance(pt) < 10.0 || (is_collapsed && r.contains(pt)));
                if hov && hovered_in.is_none() && (!is_collapsed || !linked) {
                    hovered_in = Some((*id, k));
                }
                painter.circle(sp, if hov && self.drag_link.is_some() { 6.5 } else { 5.0 }, sock_color(*sock), Stroke::new(1.0, p.bg));
                let sr = ui.interact(Rect::from_center_size(sp, vec2(16.0, 16.0)), ui.scope_id().with(("in", *id, k)), Sense::drag());
                if sr.drag_started() && linked {
                    if let Some(l) = self.graph.links.iter().find(|l| l.to == *id && l.inp == k) {
                        self.drag_link = Some((l.from, l.out));
                    }
                    actions.push(Act::Unlink(*id, k));
                }
                if is_collapsed {
                    continue;
                }
                let y = sp.y;
                let row = Rect::from_min_size(pos2(r.left() + 10.0, y - ROW * 0.5 + 1.0), vec2(NW - 20.0, ROW - 2.0));
                if linked {
                    painter.text(row.left_center(), egui::Align2::LEFT_CENTER, crate::i18n::t(name), egui::FontId::monospace(11.0), p.text);
                } else {
                    while node.values.len() <= k {
                        node.values.push(d.inputs[node.values.len()].2);
                    }
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)));
                    child.spacing_mut().interact_size.y = ROW - 4.0;
                    child.label(RichText::new(crate::i18n::t(name)).size(11.0));
                    let v = &mut node.values[k];
                    let resp = match sock {
                        Sock::Float => child.add(egui::DragValue::new(&mut v[0]).speed(0.01).max_decimals(3)),
                        Sock::Color => {
                            let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                            let mut c = Color32::from_rgba_unmultiplied(q(v[0]), q(v[1]), q(v[2]), q(v[3]));
                            let r = egui::color_picker::color_edit_button_srgba(&mut child, &mut c, egui::color_picker::Alpha::OnlyBlend);
                            if r.changed() {
                                let a = c.to_srgba_unmultiplied();
                                *v = [a[0] as f32 / 255.0, a[1] as f32 / 255.0, a[2] as f32 / 255.0, a[3] as f32 / 255.0];
                            }
                            r
                        }
                    };
                    if resp.changed() {
                        if *sock == Sock::Float {
                            let x = v[0];
                            *v = [x, x, x, x];
                        }
                        actions.push(Act::Changed);
                    }
                }
            }
            if is_collapsed {
                continue;
            }
            // Réglages
            let mut y = r.top() + HEAD + (d.outputs.len() + d.inputs.len()) as f32 * ROW;
            for pd in d.params {
                let (h, name) = match pd {
                    ParamDef::Text(n) => (9.0 * ROW, *n),
                    ParamDef::Choice(n, _) | ParamDef::Float(n, ..) | ParamDef::File(n) => (ROW, *n),
                };
                let row = Rect::from_min_size(pos2(r.left() + 8.0, y + 1.0), vec2(NW - 16.0, h - 2.0));
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)));
                let entry = node.params.entry(name.to_string()).or_insert(Param::S(String::new()));
                match (pd, entry) {
                    (ParamDef::Choice(_, opts), Param::S(cur)) => {
                        egui::ComboBox::from_id_salt(("choice", *id, name)).selected_text(RichText::new(cur.as_str()).size(11.0)).width(NW - 24.0).show_ui(&mut child, |ui| {
                            for o in *opts {
                                if ui.selectable_label(cur == o, *o).clicked() {
                                    *cur = o.to_string();
                                    actions.push(Act::Changed);
                                }
                            }
                        });
                    }
                    (ParamDef::Float(_, lo, hi, _), Param::F(v)) => {
                        child.label(RichText::new(crate::i18n::t(name)).size(11.0));
                        if child.add(egui::DragValue::new(v).range(*lo..=*hi).speed(0.05)).changed() {
                            actions.push(Act::Changed);
                        }
                    }
                    (ParamDef::Text(_), Param::S(code)) => {
                        let mut pick = None;
                        egui::ComboBox::from_id_salt(("snip", *id)).selected_text(RichText::new(crate::i18n::t("exemples")).size(10.5)).width(NW - 24.0).show_ui(&mut child, |ui| {
                            for (name, c) in SHADER_EXAMPLES {
                                if ui.button(*name).clicked() {
                                    pick = Some(c.to_string());
                                }
                            }
                        });
                        if let Some(c) = pick {
                            *code = c;
                            actions.push(Act::Changed);
                        }
                        let prompt = self.ai_prompt.entry(*id).or_default();
                        let busy = self.ai_rx.is_some();
                        child.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(prompt).hint_text(crate::i18n::t("décris l'effet…")).desired_width(NW - 64.0).font(egui::FontId::monospace(10.5)));
                            if ui.add_enabled(!busy && !prompt.trim().is_empty(), egui::Button::new(RichText::new(if busy { "…" } else { "IA" }).size(10.5))).on_hover_text(crate::i18n::t("L'IA écrit le shader")).clicked() {
                                self.ai_request = Some((*id, prompt.trim().to_string()));
                            }
                        });
                        let te = egui::TextEdit::multiline(code).code_editor().desired_width(NW - 18.0).desired_rows(7).font(egui::FontId::monospace(10.5));
                        if child.add(te).changed() {
                            actions.push(Act::Changed);
                        }
                    }
                    (ParamDef::File(_), Param::S(rel)) => {
                        let label = if rel.is_empty() { "choisir une image…".to_string() } else { rel.rsplit('/').next().unwrap_or(rel).to_string() };
                        if child.button(RichText::new(label).size(11.0)).on_hover_text(crate::i18n::t("ou glisse une image dans le graphe")).clicked() {
                            if let Some(f) = rfd::FileDialog::new().add_filter("images", &["png", "jpg", "jpeg", "webp", "tga", "tif", "tiff", "bmp", "exr", "hdr"]).pick_file() {
                                let root = self.root.clone();
                                let r = crate::vault::inside(&root, &f).map(Ok).unwrap_or_else(|| crate::vault::link_external(&root, &f));
                                match r {
                                    Ok(x) => *rel = x,
                                    Err(e) => self.message = Some(e),
                                }
                                actions.push(Act::Changed);
                            }
                        }
                    }
                    (pd, slot) => {
                        *slot = match pd {
                            ParamDef::Float(_, _, _, v) => Param::F(*v),
                            ParamDef::Choice(_, c) => Param::S(c[0].to_string()),
                            _ => Param::S(String::new()),
                        };
                    }
                }
                y += h;
            }
            // Miniature
            if preview_on && has_out {
                let tr = Rect::from_min_size(pos2(r.left() + 8.0, r.bottom() - THUMB - 6.0), vec2(NW - 16.0, THUMB));
                painter.rect_filled(tr, 0.0, Color32::from_gray(40));
                match self.thumbs.get(id) {
                    Some((_, t)) => {
                        let s = t.size_vec2();
                        let k = (tr.width() / s.x).min(tr.height() / s.y);
                        let ir = Rect::from_center_size(tr.center(), s * k);
                        painter.image(t.id(), ir, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        painter.text(tr.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::monospace(11.0), p.muted);
                    }
                }
            }
        }
        if let Some((id, text)) = rename_done {
            if let Some(n) = self.graph.nodes.iter_mut().find(|n| n.id == id) {
                n.label = text.trim().to_string();
            }
            self.rename = None;
            actions.push(Act::Changed);
        }
        // Lien en cours
        if let Some((from, out)) = self.drag_link {
            if let (Some(a), Some(pt)) = (out_pos.get(&(from, out)), pointer) {
                let col = sock_color(sock_of(from, out));
                painter.add(egui::epaint::CubicBezierShape::from_points_stroke(bez(*a, pt), false, Color32::TRANSPARENT, Stroke::new(2.5, col)));
            }
            if ui.input(|i| i.pointer.any_released()) {
                match hovered_in {
                    Some((to, inp)) => actions.push(Act::Connect(from, out, to, inp)),
                    // Lâché dans le vide : on propose un node déjà relié
                    None => {
                        if let Some(pt) = pointer {
                            if !layout.iter().any(|(_, r, _)| r.contains(pt)) {
                                actions.push(Act::OpenAdd(pt, Some((from, out))));
                            }
                        }
                    }
                }
                self.drag_link = None;
            }
        }
    }

    fn preview_ui(&mut self, ui: &mut egui::Ui, rect: Rect, ctx: &egui::Context) {
        let p = theme::pal();
        ui.painter().rect_filled(rect, 0.0, Color32::from_gray(30));
        // Résultat prêt ?
        if let Some(rx) = &self.preview_rx {
            if let Ok((key, img)) = rx.try_recv() {
                let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                let bytes: Vec<u8> = img.px.iter().flat_map(|c| [q(c[0]), q(c[1]), q(c[2]), q(c[3])]).collect();
                let ci = egui::ColorImage::from_rgba_unmultiplied([img.w, img.h], &bytes);
                match &mut self.preview {
                    Some(t) => t.set(ci, egui::TextureOptions::LINEAR),
                    None => self.preview = Some(ctx.load_texture("node-preview", ci, egui::TextureOptions::LINEAR)),
                }
                self.preview_key = key;
                self.preview_rx = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
            }
        }
        // Glisser dans l'aperçu : tourner le modèle ; molette : zoom ; modèle 3D déposé : il remplace la sphère
        let is_mat = self.graph.kind == GraphKind::Material;
        if is_mat {
            let resp = ui.interact(rect, ui.scope_id().with("mat-preview"), Sense::click_and_drag());
            if resp.dragged() {
                let d = resp.drag_delta();
                self.pv_yaw -= d.x * 0.01;
                self.pv_pitch = (self.pv_pitch + d.y * 0.01).clamp(-1.5, 1.5);
                self.pv_key = 0;
            }
            if resp.hovered() {
                let sc = ui.input(|i| i.smooth_scroll_delta.y);
                if sc != 0.0 {
                    self.pv_dist = (self.pv_dist * (-sc * 0.002).exp()).clamp(0.8, 8.0);
                    self.pv_key = 0;
                }
                let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
                for f in dropped {
                    if let Some(m) = crate::model3d::preview_model(&f) {
                        self.pv_model = Some((f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), std::sync::Arc::new(m)));
                        self.pv_key = 0;
                    }
                }
            }
        }
        // Relance le calcul si le graphe ou la vue ont changé
        if self.preview_rx.is_none() && (self.preview_key != self.version || self.pv_key == 0) {
            // « Prévisualiser la node » : sa sortie remplace le matériau complet
            let solo = self.solo.filter(|id| self.graph.nodes.iter().any(|n| n.id == *id));
            let solo_prog = solo.and_then(|id| self.graph.probe(id, 0)).and_then(|g| Program::compile(&g, &self.root, 1024).ok());
            let prog = solo_prog.map(Ok).unwrap_or_else(|| self.program.clone());
            if let Ok(prog) = prog {
                let key = self.version;
                self.pv_key = 1;
                let (tx, rx) = std::sync::mpsc::channel();
                let ctx2 = ctx.clone();
                match prog.kind {
                    GraphKind::Compositor => {
                        std::thread::spawn(move || {
                            let img = prog.composite(None);
                            let _ = tx.send((key, img));
                            ctx2.request_repaint();
                        });
                    }
                    GraphKind::Material => {
                        let model = match &self.pv_model {
                            Some((_, m)) => m.clone(),
                            None => self.sphere.get_or_insert_with(|| std::sync::Arc::new(crate::model3d::preview_sphere())).clone(),
                        };
                        let (yaw, pitch, dist, env) = (self.pv_yaw, self.pv_pitch, self.pv_dist, self.pv_env);
                        std::thread::spawn(move || {
                            let img = crate::model3d::render_material_view(&model, std::sync::Arc::new(prog), 360, yaw, pitch, dist, env);
                            let _ = tx.send((key, img));
                            ctx2.request_repaint();
                        });
                    }
                }
                self.preview_rx = Some(rx);
            }
        }
        if let Some(t) = &self.preview {
            let s = t.size_vec2();
            let k = (rect.width() / s.x).min(rect.height() / s.y);
            let r = Rect::from_center_size(rect.center(), s * k);
            if self.graph.kind == GraphKind::Compositor {
                // Damier sous la transparence
                ui.painter().rect_filled(r, 0.0, Color32::from_gray(60));
            }
            ui.painter().image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        if self.preview_rx.is_some() {
            ui.painter().text(rect.left_top() + vec2(8.0, 8.0), egui::Align2::LEFT_TOP, "calcul…", egui::FontId::monospace(11.0), p.muted);
        }
        if is_mat {
            // Barre sous l'aperçu : éclairage, modèle, node isolé
            let bar = Rect::from_min_size(rect.left_bottom() + vec2(0.0, 4.0), vec2(rect.width(), 24.0));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bar));
            child.horizontal(|ui| {
                egui::containers::menu::MenuButton::new(crate::i18n::t("lumière ▾")).ui(ui, |ui| {
                    for (k, l) in ["studio", "studio doux", "extérieur", "coucher de soleil", "nuit", "neutre"].iter().enumerate() {
                        if ui.radio(self.pv_env == k, crate::i18n::t(l)).clicked() {
                            self.pv_env = k;
                            self.pv_key = 0;
                            ui.close();
                        }
                    }
                });
                if self.pv_model.is_some() {
                    if ui.small_button(crate::i18n::t("sphère")).on_hover_text(crate::i18n::t("revenir à la sphère")).clicked() {
                        self.pv_model = None;
                        self.pv_key = 0;
                    }
                } else {
                    ui.label(RichText::new(crate::i18n::t("dépose un modèle 3D ici")).color(p.muted).size(10.5));
                }
                if let Some(id) = self.solo {
                    let name = self.graph.nodes.iter().find(|n| n.id == id).map(|n| if n.label.is_empty() { def(&n.op).map(|d| crate::i18n::t(d.label)).unwrap_or_default() } else { n.label.clone() }).unwrap_or_default();
                    if ui.small_button(format!("{} : {name} ×", crate::i18n::t("node"))).on_hover_text(crate::i18n::t("revenir au matériau complet")).clicked() {
                        self.solo = None;
                        self.pv_key = 0;
                    }
                }
            });
        }
    }

    fn export(&mut self) {
        let prog = match &self.program {
            Ok(_) => Program::compile(&self.graph.effective(), &self.root, 16384),
            Err(e) => Err(e.clone()),
        };
        match prog {
            Ok(prog) => {
                let img = prog.composite(None);
                let mut buf = image::RgbaImage::new(img.w as u32, img.h as u32);
                for (i, px) in buf.pixels_mut().enumerate() {
                    let c = img.px[i];
                    let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    *px = image::Rgba([q(c[0]), q(c[1]), q(c[2]), q(c[3])]);
                }
                let stem = self.path.file_name().map(|n| n.to_string_lossy().trim_end_matches(".comp.json").to_string()).unwrap_or_else(|| "composite".into());
                let out = self.path.with_file_name(format!("{stem}.png"));
                self.message = Some(match buf.save(&out) {
                    Ok(()) => format!("exporté : {}", out.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
                    Err(e) => format!("export impossible : {e}"),
                });
            }
            Err((_, e)) => self.message = Some(e),
        }
    }
}


/// Recherche de nodes : chaque mot doit se retrouver dans le nom (français ou anglais), la
/// catégorie ou le code ; début de mot d'abord, puis milieu, puis lettres dans l'ordre.
fn search_defs(q: &str, material: bool, need_input: bool) -> Vec<(i32, &'static Def)> {
    let q = crate::chat::fold(q);
    let words: Vec<&str> = q.split_whitespace().collect();
    let mut scored: Vec<(i32, &Def)> = DEFS
        .iter()
        .filter(|d| if material { d.mat } else { d.comp })
        .filter(|d| !need_input || !d.inputs.is_empty())
        .filter_map(|d| {
            let hay = crate::chat::fold(&format!("{} {} {} {} {}", d.label, crate::i18n::en(d.label), d.op, d.cat, crate::i18n::en(d.cat)));
            let mut total = 0;
            for w in &words {
                total += if hay.split_whitespace().any(|h| h.starts_with(w)) {
                    30
                } else if hay.contains(w) {
                    20
                } else {
                    let mut it = hay.chars();
                    if w.chars().all(|c| it.any(|h| h == c)) { 5 } else { return None }
                };
            }
            if hay.starts_with(q.trim()) {
                total += 20;
            }
            Some((total, d))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored
}

fn sock_color(s: Sock) -> Color32 {
    match s {
        Sock::Float => Color32::from_rgb(160, 160, 165),
        Sock::Color => Color32::from_rgb(230, 190, 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recherche_de_nodes() {
        let names = |q: &str| -> Vec<String> { search_defs(q, true, false).iter().take(8).map(|x| crate::i18n::en(x.1.label).to_lowercase()).collect() };
        eprintln!("{:?}", names("color"));
        eprintln!("{:?}", names("color ra"));
        assert!(names("color").iter().any(|n| n.contains("ramp")), "color doit trouver la rampe");
        assert!(names("color ra")[0].contains("ramp"));
        assert!(!names("  noise ").is_empty());
        assert!(!names("nois").is_empty() && !names("vrn").is_empty());
    }

    #[test]
    fn tous_les_nodes() {
        for d in DEFS.iter().filter(|d| !d.outputs.is_empty()) {
            for kind in [GraphKind::Compositor, GraphKind::Material] {
                if (kind == GraphKind::Material && !d.mat) || (kind == GraphKind::Compositor && !d.comp) {
                    continue;
                }
                let mut g = if kind == GraphKind::Material { Graph::new_material() } else { Graph::new_compositor() };
                let id = g.add(d.op, [0.0, 0.0]);
                let probe = g.probe(id, 0).unwrap();
                let prog = Program::compile(&probe, "/tmp", 64).unwrap_or_else(|e| panic!("{} : {}", d.op, e.1));
                if kind == GraphKind::Compositor {
                    let img = prog.composite(Some([16, 8]));
                    assert!(img.px.iter().all(|p| p.iter().all(|x| x.is_finite())), "{}", d.op);
                } else {
                    let sphere = crate::model3d::preview_sphere();
                    let img = crate::model3d::render_material_preview(&sphere, std::sync::Arc::new(prog), 24);
                    assert!(img.px.iter().all(|p| p.iter().all(|x| x.is_finite())), "{}", d.op);
                }
            }
        }
    }

    #[test]
    fn shader_et_graphes() {
        let sh = Shader::compile("k = x * 2 + 1\nr = clamp(k, 0, 2) ; g = if(x > 0.5, 1, 0)\nb = -2 ^ 2").unwrap();
        let mut v = vec![0.0; sh.slots];
        v[13] = 0.75; // x
        sh.eval(&mut v);
        assert_eq!((v[0], v[1], v[2]), (2.0, 1.0, -4.0));
        assert!(Shader::compile("r = foo(1)").is_err());
        assert!(Shader::compile("r = (1 + 2").is_err());
        assert!(Shader::compile(DEFAULT_SHADER).is_ok());

        // Compositing : couleur → inverser → sortie
        let mut g = Graph { kind: GraphKind::Compositor, nodes: vec![], links: vec![], next_id: 0, frames: vec![] };
        let c = g.add("color", [0.0, 0.0]);
        g.nodes[0].values[0] = [0.2, 0.4, 0.6, 1.0];
        let inv = g.add("invert", [0.0, 0.0]);
        let blur = g.add("blur", [0.0, 0.0]);
        let out = g.add("comp_out", [0.0, 0.0]);
        g.connect(c, 0, inv, 0);
        g.connect(inv, 0, blur, 0);
        g.connect(blur, 0, out, 0);
        g.connect(out, 0, c, 0); // boucle refusée (la sortie n'a pas de sortie de toute façon)
        g.connect(blur, 0, c, 0); // boucle refusée
        assert_eq!(g.links.len(), 3);
        let prog = Program::compile(&g, "/tmp", 64).unwrap();
        let img = prog.composite(Some([16, 8]));
        let px = img.px[40];
        assert!((px[0] - 0.8).abs() < 1e-4 && (px[2] - 0.4).abs() < 1e-4, "{px:?}");

        for name in MATERIAL_PRESETS {
            let g = material_preset(name).unwrap();
            assert!(Program::compile(&g, "/tmp", 64).is_ok(), "{name}");
        }
        for (_, c) in SHADER_EXAMPLES {
            assert!(Shader::compile(c).is_ok(), "{c}");
        }
        // Matériau par défaut
        let m = Graph::new_material();
        let prog = Program::compile(&m, "/tmp", 64).unwrap();
        let mut regs = vec![];
        let sf = prog.material(&Ctx { pos: [0.1, 0.2, 0.3], nrm: [0.0, 1.0, 0.0], ..Default::default() }, &mut regs);
        let (col, rough) = (sf.color, sf.rough);
        assert!(col[0] >= 0.0 && rough == 0.5);
        // JSON aller-retour
        let t = serde_json::to_string(&m).unwrap();
        let back: Graph = serde_json::from_str(&t).unwrap();
        assert_eq!(back.nodes.len(), m.nodes.len());
    }
}


/// Consigne donnée à l'IA pour écrire un shader dans le langage d'Aryadne.
pub const SHADER_AI_PROMPT: &str = "Tu écris du code pour le node « shader perso » d'Aryadne. Langage : une instruction par ligne, « nom = expression ». Variables en entrée : ar ag ab aa (couleur A), br bg bb ba (couleur B), f (nombre F), x y (coordonnées 0..1), nx ny nz (normale, matériau 3D), px py pz (position, matériau 3D, environ -0.5..0.5), w h (taille de l'image), pi. Sorties : r g b a (couleur, valeurs 0..1) et f. Opérateurs : + - * / % ^ < > et parenthèses. Fonctions : sin cos tan abs floor ceil fract sqrt exp log sign pow(a,b) min max mod step(seuil,x) atan2 rand(x,y) len(x,y) clamp(x,a,b) mix(a,b,t) smoothstep(a,b,x) noise(x,y,z) fbm(x,y,z) if(cond,a,b). Pas de boucles, pas de vecteurs, pas de commentaires obligatoires. Réponds UNIQUEMENT avec le code, sans ``` ni explication.";

/// Nettoie la réponse de l'IA (blocs ```, texte autour).
pub fn clean_ai_code(t: &str) -> String {
    let mut lines: Vec<&str> = t.lines().filter(|l| !l.trim_start().starts_with("```")).collect();
    lines.retain(|l| l.trim().is_empty() || l.contains('=') || l.trim_start().starts_with("//"));
    lines.join("\n").trim().to_string() + "\n"
}
