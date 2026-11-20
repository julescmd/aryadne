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
    Def { op: "mat_out", label: "Sortie matériau", cat: "sortie", inputs: &[("couleur", Sock::Color, [0.8, 0.5, 0.4, 1.0]), ("rugosité", Sock::Float, f(0.5)), ("métal", Sock::Float, f(0.0)), ("émission", Sock::Color, BLACK)], outputs: &[], params: &[], mat: true, comp: false },
    Def { op: "comp_out", label: "Sortie", cat: "sortie", inputs: &[("image", Sock::Color, GRAY)], outputs: &[], params: &[], mat: false, comp: true },
    // Entrées
    Def { op: "image", label: "Image", cat: "entrée", inputs: &[], outputs: &[("image", Sock::Color), ("alpha", Sock::Float)], params: &[ParamDef::File("fichier")], mat: false, comp: true },
    Def { op: "color", label: "Couleur", cat: "entrée", inputs: &[("couleur", Sock::Color, [0.9, 0.4, 0.2, 1.0])], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "value", label: "Valeur", cat: "entrée", inputs: &[("valeur", Sock::Float, f(0.5))], outputs: &[("valeur", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "coords", label: "Coordonnées", cat: "entrée", inputs: &[], outputs: &[("u", Sock::Float), ("v", Sock::Float), ("uv", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "geometry", label: "Géométrie", cat: "entrée", inputs: &[], outputs: &[("normale", Sock::Color), ("position", Sock::Color), ("hauteur", Sock::Float), ("pente", Sock::Float)], params: &[], mat: true, comp: false },
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
        self.nodes.push(Node { id, op: op.into(), pos, values: d.inputs.iter().map(|i| i.2).collect(), params });
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

    pub fn remove(&mut self, id: u32) {
        self.nodes.retain(|n| n.id != id);
        self.links.retain(|l| l.from != id && l.to != id);
    }

    pub fn new_material() -> Graph {
        let mut g = Graph { kind: GraphKind::Material, nodes: vec![], links: vec![], next_id: 0 };
        let noise = g.add("noise", [0.0, 0.0]);
        let ramp = g.add("ramp", [220.0, 0.0]);
        let out = g.add("mat_out", [460.0, 40.0]);
        g.connect(noise, 0, ramp, 0);
        g.connect(ramp, 0, out, 0);
        g
    }

    pub fn new_compositor() -> Graph {
        let mut g = Graph { kind: GraphKind::Compositor, nodes: vec![], links: vec![], next_id: 0 };
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
    let mut g = Graph { kind: GraphKind::Material, nodes: vec![], links: vec![], next_id: 0 };
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
        _ => return None,
    }
    Some(g)
}

pub const MATERIAL_PRESETS: [&str; 6] = ["bois", "pierre", "métal rouillé", "toon", "céramique", "or"];

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
#[derive(Clone, Copy, Default)]
pub struct Ctx {
    pub uv: [f32; 2],
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    pub view: [f32; 3],
    pub vcol: V,
    pub res: [f32; 2],
}

/// Graphe compilé : nodes dans l'ordre d'exécution, entrées résolues.
#[derive(Clone)]
pub struct Program {
    steps: Vec<Step>,
    regs: usize,
    pub kind: GraphKind,
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
            let image = if n.op == "image" {
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
        Ok(Program { steps, regs, kind: g.kind })
    }

    /// Matériau : couleur (sRGB), rugosité, métal, émission.
    pub fn material(&self, c: &Ctx, regs: &mut Vec<V>) -> (V, f32, f32, V) {
        regs.resize(self.regs, [0.0; 4]);
        for s in &self.steps {
            let ins: Vec<V> = s.ins.iter().map(|i| match i {
                Src::Reg(r) => regs[*r],
                Src::Const(v) => *v,
            }).collect();
            if s.op == "mat_out" {
                return (ins[0], ins[1][0].clamp(0.02, 1.0), ins[2][0].clamp(0.0, 1.0), ins[3]);
            }
            let o = pixel(s, &ins, c);
            for (k, v) in o.iter().enumerate().take(s.nout.min(4)) {
                regs[s.base + k] = *v;
            }
        }
        ([0.8, 0.5, 0.4, 1.0], 0.5, 0.0, [0.0; 4])
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
        "fresnel" => {
            let d = (c.nrm[0] * c.view[0] + c.nrm[1] * c.view[1] + c.nrm[2] * c.view[2]).abs();
            [one((1.0 - d).powf(i[0][0].max(0.1))), z, z, z]
        }
        "vcolor" => [c.vcol, z, z, z],
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

pub struct Editor {
    pub path: PathBuf,
    root: String,
    pub graph: Graph,
    scene: Rect,
    drag_link: Option<(u32, usize)>,
    selected: Option<u32>,
    dirty: Option<std::time::Instant>,
    program: Result<Program, (u32, String)>,
    preview: Option<egui::TextureHandle>,
    preview_key: u64,
    preview_rx: Option<std::sync::mpsc::Receiver<(u64, Img)>>,
    sphere: Option<std::sync::Arc<crate::model3d::Prepared>>,
    add_at: Option<Pos2>,
    add_query: String,
    pub message: Option<String>,
    version: u64,
}

impl Editor {
    pub fn open(path: PathBuf, root: &str) -> Result<Editor, String> {
        let graph = Graph::load(&path)?;
        let program = Program::compile(&graph, root, 1024);
        Ok(Editor { path, root: root.to_string(), graph, scene: Rect::ZERO, drag_link: None, selected: None, dirty: None, program, preview: None, preview_key: u64::MAX, preview_rx: None, sphere: None, add_at: None, add_query: String::new(), message: None, version: 1 })
    }

    fn changed(&mut self) {
        self.dirty = Some(std::time::Instant::now());
        self.program = Program::compile(&self.graph, &self.root, 1024);
        self.version += 1;
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

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let ctx = ui.ctx().clone();
        self.save_if_due(false);
        if self.dirty.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
        let kind = self.graph.kind;
        // Barre
        let mut add: Option<&'static str> = None;
        let mut preset: Option<&'static str> = None;
        ui.horizontal(|ui| {
            egui::containers::menu::MenuButton::new("＋ ajouter ▾").ui(ui, |ui| {
                for cat in ["entrée", "motif", "couleur", "math", "filtre", "shader", "sortie"] {
                    let items: Vec<&Def> = DEFS.iter().filter(|d| d.cat == cat && if kind == GraphKind::Material { d.mat } else { d.comp }).collect();
                    if items.is_empty() {
                        continue;
                    }
                    ui.menu_button(cat, |ui| {
                        for d in items {
                            if ui.button(d.label).clicked() {
                                add = Some(d.op);
                            }
                        }
                    });
                }
            });
            if kind == GraphKind::Material {
                egui::containers::menu::MenuButton::new("préréglages ▾").ui(ui, |ui| {
                    for name in MATERIAL_PRESETS {
                        if ui.button(name).on_hover_text("remplace le graphe actuel").clicked() {
                            preset = Some(name);
                        }
                    }
                });
            }
            ui.label(RichText::new("Maj+A : ajouter · glisser une sortie sur une entrée · Suppr : effacer · molette : zoom").color(p.muted).size(11.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if kind == GraphKind::Compositor && ui.button("exporter PNG").clicked() {
                    self.export();
                }
                if ui.button("recadrer").clicked() {
                    self.scene = Rect::ZERO;
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
        let pw = (avail.width() * 0.32).clamp(220.0, 460.0);
        let prev_rect = Rect::from_min_size(avail.min, vec2(pw, pw.min(avail.height())));
        let graph_rect = Rect::from_min_max(pos2(avail.min.x + pw + 8.0, avail.min.y), avail.max);
        self.preview_ui(ui, prev_rect, &ctx);
        ui.painter().rect_filled(graph_rect, 0.0, p.panel);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(graph_rect));
        let shift_a = ctx.input(|i| i.modifiers.shift && i.key_pressed(egui::Key::A)) && child.rect_contains_pointer(graph_rect);
        if shift_a {
            self.add_at = ctx.pointer_latest_pos();
            self.add_query.clear();
        }
        // Premier affichage (ou « recadrer ») : on cadre tous les nodes.
        if self.scene == Rect::ZERO || !self.scene.is_finite() {
            let mut bb = Rect::NOTHING;
            for n in &self.graph.nodes {
                bb = bb.union(Rect::from_min_size(pos2(n.pos[0], n.pos[1]), vec2(190.0, 220.0)));
            }
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
            // Pas de zoom au-delà de 1:1
            if w < graph_rect.width() {
                w = graph_rect.width();
                h = graph_rect.height();
            }
            self.scene = Rect::from_center_size(bb.center(), vec2(w, h));
        }
        let scene_before = self.scene;
        let mut scene = self.scene;
        let mut actions: Vec<Act> = vec![];
        egui::Scene::new().zoom_range(0.25..=2.0).drag_pan_buttons(egui::DragPanButtons::PRIMARY | egui::DragPanButtons::MIDDLE).show(&mut child, &mut scene, |ui| {
            self.draw_graph(ui, &mut actions);
        });
        self.scene = scene;
        let _ = scene_before;
        // Ajout au centre ou au curseur
        let sc = self.scene;
        let to_graph = move |pt: Pos2| -> [f32; 2] {
            if !sc.is_finite() || sc.width() <= 0.0 {
                return [0.0, 0.0];
            }
            let k = sc.width() / graph_rect.width();
            [sc.min.x + (pt.x - graph_rect.min.x) * k, sc.min.y + (pt.y - graph_rect.min.y) * k]
        };
        if let Some(op) = add {
            let at = to_graph(graph_rect.center());
            let id = self.graph.add(op, at);
            self.selected = Some(id);
            self.changed();
        }
        if let Some(at) = self.add_at {
            let mut close = false;
            let mut chosen = None;
            let area = egui::Area::new(egui::Id::unique("node-add")).order(egui::Order::Foreground).fixed_pos(at).show(&ctx, |ui| {
                egui::Frame::new().fill(p.bg).stroke(Stroke::new(1.0, p.text)).inner_margin(8).show(ui, |ui| {
                    ui.set_width(220.0);
                    let r = ui.add(egui::TextEdit::singleline(&mut self.add_query).hint_text("ajouter un node…"));
                    r.request_focus();
                    let q = crate::chat::fold(&self.add_query);
                    let list: Vec<&Def> = DEFS.iter().filter(|d| (if kind == GraphKind::Material { d.mat } else { d.comp }) && crate::chat::fold(d.label).contains(&q)).take(12).collect();
                    for (k, d) in list.iter().enumerate() {
                        let t = RichText::new(format!("{}  ", d.label)).color(if k == 0 { p.accent } else { p.text });
                        if ui.add(egui::Button::new(t).frame(false)).clicked() {
                            chosen = Some(d.op);
                        }
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        chosen = list.first().map(|d| d.op);
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        close = true;
                    }
                });
            });
            if let Some(op) = chosen {
                let g = to_graph(at);
                let id = self.graph.add(op, g);
                self.selected = Some(id);
                self.changed();
                close = true;
            }
            if close || (ctx.input(|i| i.pointer.any_click()) && chosen.is_none() && !ctx.pointer_latest_pos().is_some_and(|pt| area.response.rect.contains(pt))) {
                self.add_at = None;
            }
        }
        // Actions du graphe
        let mut changed = false;
        for a in actions {
            match a {
                Act::Move(id, d) => {
                    if let Some(n) = self.graph.nodes.iter_mut().find(|n| n.id == id) {
                        n.pos[0] += d.x;
                        n.pos[1] += d.y;
                        self.dirty = Some(std::time::Instant::now());
                    }
                }
                Act::Connect(a, o, b, i) => {
                    self.graph.connect(a, o, b, i);
                    changed = true;
                }
                Act::Unlink(b, i) => {
                    self.graph.links.retain(|l| !(l.to == b && l.inp == i));
                    changed = true;
                }
                Act::Changed => changed = true,
                Act::Select(id) => self.selected = id,
            }
        }
        if let Some(sel) = self.selected {
            let typing = ctx.egui_wants_keyboard_input();
            if !typing && ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::X)) {
                let keep = self.graph.nodes.iter().find(|n| n.id == sel).is_some_and(|n| n.op == "mat_out" || n.op == "comp_out");
                if !keep {
                    self.graph.remove(sel);
                    self.selected = None;
                    changed = true;
                }
            }
            if !typing && ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::D)) {
                if let Some(n) = self.graph.nodes.iter().find(|n| n.id == sel).cloned() {
                    let id = self.graph.add(&n.op, [n.pos[0] + 30.0, n.pos[1] + 30.0]);
                    if let Some(m) = self.graph.nodes.iter_mut().find(|m| m.id == id) {
                        m.values = n.values.clone();
                        m.params = n.params.clone();
                    }
                    self.selected = Some(id);
                    changed = true;
                }
            }
        }
        if changed {
            self.changed();
        }
        ui.allocate_rect(avail, Sense::hover());
    }

    fn draw_graph(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Act>) {
        let p = theme::pal();
        let painter = ui.painter().clone();
        const NW: f32 = 190.0;
        const ROW: f32 = 22.0;
        const HEAD: f32 = 24.0;
        let err_node = self.program.as_ref().err().map(|e| e.0);
        // Positions des prises
        let mut out_pos: HashMap<(u32, usize), Pos2> = HashMap::new();
        let mut in_pos: HashMap<(u32, usize), Pos2> = HashMap::new();
        let layout: Vec<(u32, Rect, &'static Def, f32)> = self
            .graph
            .nodes
            .iter()
            .filter_map(|n| {
                let d = def(&n.op)?;
                let rows = d.outputs.len() + d.inputs.len() + d.params.iter().map(|pp| if matches!(pp, ParamDef::Text(_)) { 8 } else { 1 }).sum::<usize>();
                let h = HEAD + rows as f32 * ROW + 8.0;
                let r = Rect::from_min_size(pos2(n.pos[0], n.pos[1]), vec2(NW, h));
                Some((n.id, r, d, h))
            })
            .collect();
        for (id, r, d, _) in &layout {
            for k in 0..d.outputs.len() {
                out_pos.insert((*id, k), pos2(r.right(), r.top() + HEAD + (k as f32 + 0.5) * ROW));
            }
            for k in 0..d.inputs.len() {
                in_pos.insert((*id, k), pos2(r.left(), r.top() + HEAD + (d.outputs.len() + k) as f32 * ROW + ROW * 0.5));
            }
        }
        // Liens
        let wire = |a: Pos2, b: Pos2, col: Color32| {
            let dx = ((b.x - a.x).abs() * 0.5).max(40.0);
            let pts = [a, a + vec2(dx, 0.0), b - vec2(dx, 0.0), b];
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(pts, false, Color32::TRANSPARENT, Stroke::new(2.0, col)));
        };
        for l in &self.graph.links {
            if let (Some(a), Some(b)) = (out_pos.get(&(l.from, l.out)), in_pos.get(&(l.to, l.inp))) {
                wire(*a, *b, p.muted);
            }
        }
        // Nodes
        let mut hovered_in: Option<(u32, usize)> = None;
        // Le graphe est dans un calque zoomé : on ramène le pointeur dans ses coordonnées.
        let from_global = ui.ctx().layer_transform_from_global(ui.layer_id());
        let pointer = ui.input(|i| i.pointer.latest_pos()).map(|pt| from_global.map(|t| t * pt).unwrap_or(pt));
        let ids: Vec<u32> = self.graph.nodes.iter().map(|n| n.id).collect();
        for (id, r, d, _) in &layout {
            let ni = ids.iter().position(|x| x == id).unwrap();
            let selected = self.selected == Some(*id);
            let cat_col = match d.cat {
                "sortie" => p.accent,
                "entrée" => Color32::from_rgb(70, 130, 200),
                "motif" => Color32::from_rgb(140, 100, 190),
                "couleur" => Color32::from_rgb(200, 150, 50),
                "filtre" => Color32::from_rgb(60, 160, 120),
                "shader" => Color32::from_rgb(210, 70, 110),
                _ => Color32::from_rgb(120, 120, 130),
            };
            painter.rect_filled(r.translate(vec2(3.0, 3.0)), 0.0, Color32::from_black_alpha(30));
            painter.rect_filled(*r, 0.0, p.bg);
            let head = Rect::from_min_size(r.min, vec2(r.width(), HEAD));
            painter.rect_filled(head, 0.0, cat_col.gamma_multiply(0.85));
            painter.text(head.left_center() + vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, d.label, egui::FontId::monospace(12.0), Color32::WHITE);
            let border = if err_node == Some(*id) { Stroke::new(2.0, p.missing) } else if selected { Stroke::new(2.0, p.accent) } else { Stroke::new(1.0, p.line) };
            painter.rect_stroke(*r, 0.0, border, egui::StrokeKind::Outside);
            let hr = ui.interact(head, ui.scope_id().with(("node-head", *id)), Sense::click_and_drag());
            if hr.dragged() {
                actions.push(Act::Move(*id, hr.drag_delta()));
            }
            if hr.clicked() || hr.drag_started() {
                actions.push(Act::Select(Some(*id)));
            }
            // Sorties
            for (k, (name, sock)) in d.outputs.iter().enumerate() {
                let y = r.top() + HEAD + (k as f32 + 0.5) * ROW;
                painter.text(pos2(r.right() - 12.0, y), egui::Align2::RIGHT_CENTER, *name, egui::FontId::monospace(11.0), p.text);
                let sp = pos2(r.right(), y);
                painter.circle(sp, 5.0, sock_color(*sock), Stroke::new(1.0, p.bg));
                let sr = ui.interact(Rect::from_center_size(sp, vec2(14.0, 14.0)), ui.scope_id().with(("out", *id, k)), Sense::drag());
                if sr.drag_started() {
                    self.drag_link = Some((*id, k));
                }
            }
            // Entrées (avec valeur si non reliée)
            let node = &mut self.graph.nodes[ni];
            for (k, (name, sock, _)) in d.inputs.iter().enumerate() {
                let y = r.top() + HEAD + (d.outputs.len() + k) as f32 * ROW + ROW * 0.5;
                let sp = pos2(r.left(), y);
                painter.circle(sp, 5.0, sock_color(*sock), Stroke::new(1.0, p.bg));
                let linked = self.graph.links.iter().any(|l| l.to == *id && l.inp == k);
                let sr = ui.interact(Rect::from_center_size(sp, vec2(14.0, 14.0)), ui.scope_id().with(("in", *id, k)), Sense::drag());
                if pointer.is_some_and(|pt| sp.distance(pt) < 10.0) {
                    hovered_in = Some((*id, k));
                }
                if sr.drag_started() && linked {
                    if let Some(l) = self.graph.links.iter().find(|l| l.to == *id && l.inp == k) {
                        self.drag_link = Some((l.from, l.out));
                    }
                    actions.push(Act::Unlink(*id, k));
                }
                let row = Rect::from_min_size(pos2(r.left() + 10.0, y - ROW * 0.5 + 1.0), vec2(NW - 20.0, ROW - 2.0));
                if linked {
                    painter.text(row.left_center(), egui::Align2::LEFT_CENTER, *name, egui::FontId::monospace(11.0), p.text);
                } else {
                    while node.values.len() <= k {
                        node.values.push(d.inputs[node.values.len()].2);
                    }
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)));
                    child.spacing_mut().interact_size.y = ROW - 4.0;
                    child.label(RichText::new(*name).size(11.0));
                    let v = &mut node.values[k];
                    let resp = match sock {
                        Sock::Float => child.add(egui::DragValue::new(&mut v[0]).speed(0.01).max_decimals(3)),
                        Sock::Color => {
                            // Valeurs sRGB (comme un sélecteur de couleur habituel)
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
            // Réglages
            let mut y = r.top() + HEAD + (d.outputs.len() + d.inputs.len()) as f32 * ROW;
            for pd in d.params {
                let (h, name) = match pd {
                    ParamDef::Text(n) => (8.0 * ROW, *n),
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
                        child.label(RichText::new(name).size(11.0));
                        if child.add(egui::DragValue::new(v).range(*lo..=*hi).speed(0.05)).changed() {
                            actions.push(Act::Changed);
                        }
                    }
                    (ParamDef::Text(_), Param::S(code)) => {
                        let mut pick = None;
                        egui::ComboBox::from_id_salt(("snip", *id)).selected_text(RichText::new("exemples").size(10.5)).width(NW - 24.0).show_ui(&mut child, |ui| {
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
                        let te = egui::TextEdit::multiline(code).code_editor().desired_width(NW - 18.0).desired_rows(7).font(egui::FontId::monospace(10.5));
                        if child.add(te).changed() {
                            actions.push(Act::Changed);
                        }
                    }
                    (ParamDef::File(_), Param::S(rel)) => {
                        let label = if rel.is_empty() { "choisir une image…".to_string() } else { rel.rsplit('/').next().unwrap_or(rel).to_string() };
                        if child.button(RichText::new(label).size(11.0)).clicked() {
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
                        // Réglage d'un ancien format : on remet la valeur par défaut
                        *slot = match pd {
                            ParamDef::Float(_, _, _, v) => Param::F(*v),
                            ParamDef::Choice(_, c) => Param::S(c[0].to_string()),
                            _ => Param::S(String::new()),
                        };
                    }
                }
                y += h;
            }
        }
        // Lien en cours
        if let Some((from, out)) = self.drag_link {
            if let (Some(a), Some(pt)) = (out_pos.get(&(from, out)), pointer) {
                wire(*a, pt, p.accent);
            }
            if ui.input(|i| i.pointer.any_released()) {
                if let Some((to, inp)) = hovered_in {
                    actions.push(Act::Connect(from, out, to, inp));
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
        // Relance le calcul si le graphe a changé
        if self.preview_rx.is_none() && self.preview_key != self.version {
            if let Ok(prog) = &self.program {
                let prog = prog.clone();
                let key = self.version;
                let (tx, rx) = std::sync::mpsc::channel();
                let ctx2 = ctx.clone();
                match prog.kind {
                    GraphKind::Compositor => {
                        std::thread::spawn(move || {
                            let img = prog.composite(None);
                            // Aperçu réduit pour l'affichage
                            let _ = tx.send((key, img));
                            ctx2.request_repaint();
                        });
                    }
                    GraphKind::Material => {
                        let sphere = self.sphere.get_or_insert_with(|| std::sync::Arc::new(crate::model3d::preview_sphere())).clone();
                        std::thread::spawn(move || {
                            let img = crate::model3d::render_material_preview(&sphere, std::sync::Arc::new(prog), 360);
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
    }

    fn export(&mut self) {
        let prog = match &self.program {
            Ok(_) => Program::compile(&self.graph, &self.root, 16384),
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

enum Act {
    Move(u32, egui::Vec2),
    Connect(u32, usize, u32, usize),
    Unlink(u32, usize),
    Changed,
    Select(Option<u32>),
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
        let mut g = Graph { kind: GraphKind::Compositor, nodes: vec![], links: vec![], next_id: 0 };
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
        let (col, rough, _, _) = prog.material(&Ctx { pos: [0.1, 0.2, 0.3], nrm: [0.0, 1.0, 0.0], ..Default::default() }, &mut regs);
        assert!(col[0] >= 0.0 && rough == 0.5);
        // JSON aller-retour
        let t = serde_json::to_string(&m).unwrap();
        let back: Graph = serde_json::from_str(&t).unwrap();
        assert_eq!(back.nodes.len(), m.nodes.len());
    }
}
