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
    crate::formats::has(rel, "mat") || crate::formats::has(rel, "comp") || crate::formats::has(rel, "geo")
}

// ---------- définitions des nodes ----------

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum GraphKind {
    Material,
    Compositor,
    /// Géométrie et simulation : des points et leurs attributs, rangés en colonnes
    Geometry,
}

/// Type d'une prise. Dans un matériau ou une image, toutes les valeurs voyagent en quatre nombres ;
/// le type sert à l'artiste (couleur de la prise, réglage proposé) et à refuser les liens absurdes.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum Sock {
    Float,
    Color,
    /// Trois nombres : position, direction, vitesse
    Vector,
    /// Vrai ou faux (0 ou 1)
    Bool,
    /// Nombre entier
    Int,
    /// Géométrie : points, attributs, arêtes
    Geo,
    /// Prise libre (renvoi, port à créer) : prend le type de ce qu'on y branche
    Any,
}

#[derive(Clone, Copy)]
pub enum ParamDef {
    Choice(&'static str, &'static [&'static str]),
    Float(&'static str, f32, f32, f32),
    Text(&'static str),
    File(&'static str),
    /// Dégradé à plusieurs couleurs, réglé à la souris comme dans Blender
    Ramp(&'static str),
    /// Courbes de réglage (ensemble, rouge, vert, bleu), tracées à la souris
    Curve(&'static str),
    /// Une ligne de texte (nom d'attribut, formule), avec sa valeur de départ
    Line(&'static str, &'static str),
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

impl Def {
    /// Ce node a-t-il sa place dans ce genre de graphe ? Un graphe de géométrie prend ses propres
    /// nodes, plus tous ceux qui calculent une valeur en un point sans lire d'image (les champs).
    pub fn fits(&self, kind: GraphKind) -> bool {
        match kind {
            GraphKind::Material => self.mat,
            GraphKind::Compositor => self.comp,
            GraphKind::Geometry => matches!(self.cat, "géométrie" | "simulation") || self.op == "geo_out" || (self.mat && self.comp && !matches!(self.cat, "sortie" | "shader" | "usure") && !matches!(self.op, "alphaover" | "param" | "video" | "texture" | "coords" | "texcoord" | "posterize" | "shape")),
        }
    }
}

const W: V = [1.0, 1.0, 1.0, 1.0];
const BLACK: V = [0.0, 0.0, 0.0, 1.0];
const GRAY: V = [0.5, 0.5, 0.5, 1.0];
const fn f(x: f32) -> V {
    [x, x, x, x]
}

const BLEND: &[&str] = &["mélange", "multiplier", "écran", "incrustation", "ajouter", "soustraire", "différence", "éclaircir", "assombrir", "diviser"];
const MATH: &[&str] = &["ajouter", "soustraire", "multiplier", "diviser", "puissance", "minimum", "maximum", "modulo", "sinus", "cosinus", "absolu", "arrondi inf.", "fraction", "racine", "inférieur à", "supérieur à", "lisse (smoothstep)", "tangente", "arc sinus", "arc cosinus", "arc tangente", "arc tangente (A / B)", "exponentielle", "logarithme", "signe", "arrondi", "arrondi sup.", "tronquer", "aimanter (pas B)", "boucler (0 à B)", "ping-pong (0 à B)", "inverse (1 / A)", "opposé", "en radians", "en degrés", "minimum doux", "maximum doux"];
const VMATH: &[&str] = &["ajouter", "soustraire", "multiplier", "diviser", "échelle (× facteur)", "produit scalaire", "produit vectoriel", "longueur", "distance", "normaliser", "mélange (facteur)", "réfléchir", "projeter", "minimum", "maximum", "absolu", "arrondi inf.", "fraction", "sinus", "cosinus"];
const COMPARE: &[&str] = &["égal", "différent", "inférieur", "inférieur ou égal", "supérieur", "supérieur ou égal"];
const LOGIC: &[&str] = &["et", "ou", "non", "ou exclusif", "non-et", "non-ou"];
const ZERO: V = [0.0, 0.0, 0.0, 0.0];

pub const DEFS: &[Def] = &[
    // Sorties
    Def { op: "mat_out", label: "Sortie matériau", cat: "sortie", inputs: &[("couleur", Sock::Color, [0.8, 0.5, 0.4, 1.0]), ("rugosité", Sock::Float, f(0.5)), ("métal", Sock::Float, f(0.0)), ("émission", Sock::Color, BLACK), ("transmission", Sock::Float, f(0.0)), ("indice de réfraction", Sock::Float, f(1.45)), ("vernis", Sock::Float, f(0.0)), ("bandes toon", Sock::Float, f(0.0)), ("reflets", Sock::Float, f(1.0)), ("force d'émission", Sock::Float, f(1.0)), ("occlusion", Sock::Float, f(1.0)), ("relief", Sock::Float, f(0.5)), ("force du relief", Sock::Float, f(1.0)), ("velours", Sock::Float, f(0.0)), ("sous-surface", Sock::Float, f(0.0)), ("teinte sous-surface", Sock::Color, [0.9, 0.3, 0.2, 1.0])], outputs: &[], params: &[], mat: true, comp: false },
    Def { op: "comp_out", label: "Sortie", cat: "sortie", inputs: &[("image", Sock::Color, GRAY)], outputs: &[], params: &[], mat: false, comp: true },
    // Entrées
    Def { op: "exr", label: "EXR multicouche", cat: "entrée", inputs: &[], outputs: &[("image", Sock::Color), ("alpha", Sock::Float)], params: &[ParamDef::File("fichier"), ParamDef::Text("calque")], mat: false, comp: true },
    Def { op: "image", label: "Image", cat: "entrée", inputs: &[], outputs: &[("image", Sock::Color), ("alpha", Sock::Float)], params: &[ParamDef::File("fichier")], mat: false, comp: true },
    Def { op: "color", label: "Couleur", cat: "entrée", inputs: &[("couleur", Sock::Color, [0.9, 0.4, 0.2, 1.0])], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "param", label: "Paramètre exposé", cat: "entrée", inputs: &[("valeur", Sock::Color, [0.5, 0.5, 0.5, 1.0])], outputs: &[("couleur", Sock::Color), ("valeur", Sock::Float)], params: &[ParamDef::Choice("type", &["couleur", "valeur"])], mat: true, comp: false },
    Def { op: "value", label: "Valeur", cat: "entrée", inputs: &[("valeur", Sock::Float, f(0.5))], outputs: &[("valeur", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "coords", label: "Coordonnées", cat: "entrée", inputs: &[], outputs: &[("u", Sock::Float), ("v", Sock::Float), ("uv", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "geometry", label: "Géométrie", cat: "entrée", inputs: &[], outputs: &[("normale", Sock::Color), ("position", Sock::Color), ("hauteur", Sock::Float), ("pente", Sock::Float)], params: &[], mat: true, comp: false },
    Def { op: "video", label: "Texture vidéo", cat: "entrée", inputs: &[("échelle", Sock::Float, f(1.0))], outputs: &[("couleur", Sock::Color), ("alpha", Sock::Float)], params: &[ParamDef::File("fichier"), ParamDef::Float("temps (s)", 0.0, 600.0, 0.0), ParamDef::Choice("projection", &["triplanaire", "dessus (XZ)", "face (XY)", "côté (YZ)", "sphérique"])], mat: true, comp: true },
    Def { op: "texture", label: "Texture image", cat: "entrée", inputs: &[("échelle", Sock::Float, f(1.0))], outputs: &[("couleur", Sock::Color), ("alpha", Sock::Float)], params: &[ParamDef::File("fichier"), ParamDef::Choice("projection", &["triplanaire", "dessus (XZ)", "face (XY)", "côté (YZ)", "sphérique"])], mat: true, comp: true },
    Def { op: "paintmask", label: "Masque peint", cat: "entrée", inputs: &[], outputs: &[("masque", Sock::Float)], params: &[ParamDef::Choice("canal", &["1", "2", "3", "4"])], mat: true, comp: false },
    Def { op: "curvature", label: "Courbure", cat: "entrée", inputs: &[("intensité", Sock::Float, f(1.0))], outputs: &[("arêtes", Sock::Float), ("creux", Sock::Float), ("brute", Sock::Float)], params: &[], mat: true, comp: false },
    Def { op: "fresnel", label: "Fresnel", cat: "entrée", inputs: &[("puissance", Sock::Float, f(3.0))], outputs: &[("fresnel", Sock::Float)], params: &[], mat: true, comp: false },
    Def { op: "vcolor", label: "Couleur du fichier", cat: "entrée", inputs: &[], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: false },
    // Motifs
    Def { op: "noise", label: "Bruit", cat: "motif", inputs: &[("échelle", Sock::Float, f(4.0)), ("détail", Sock::Float, f(4.0)), ("rugosité", Sock::Float, f(0.5)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("valeur", Sock::Float), ("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "voronoi", label: "Voronoï", cat: "motif", inputs: &[("échelle", Sock::Float, f(5.0)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("distance", Sock::Float), ("cellule", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "checker", label: "Damier", cat: "motif", inputs: &[("échelle", Sock::Float, f(8.0)), ("couleur A", Sock::Color, W), ("couleur B", Sock::Color, [0.15, 0.15, 0.15, 1.0]), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("couleur", Sock::Color), ("valeur", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "gradient", label: "Dégradé", cat: "motif", inputs: &[("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("valeur", Sock::Float)], params: &[ParamDef::Choice("type", &["horizontal", "vertical", "radial", "hauteur (3D)"])], mat: true, comp: true },
    Def { op: "stripes", label: "Rayures", cat: "motif", inputs: &[("nombre", Sock::Float, f(10.0)), ("épaisseur", Sock::Float, f(0.5)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("valeur", Sock::Float)], params: &[ParamDef::Choice("axe", &["x", "y", "diagonale"])], mat: true, comp: true },
    // Couleur
    Def { op: "mix", label: "Mélange", cat: "couleur", inputs: &[("facteur", Sock::Float, f(0.5)), ("A", Sock::Color, [0.2, 0.2, 0.2, 1.0]), ("B", Sock::Color, [0.9, 0.9, 0.9, 1.0])], outputs: &[("couleur", Sock::Color)], params: &[ParamDef::Choice("mode", BLEND)], mat: true, comp: true },
    Def { op: "colorramp", label: "Rampe de couleur", cat: "couleur", inputs: &[("facteur", Sock::Float, f(0.5))], outputs: &[("couleur", Sock::Color), ("alpha", Sock::Float)], params: &[ParamDef::Ramp("dégradé"), ParamDef::Choice("interpolation", &["linéaire", "lisse", "constante"])], mat: true, comp: true },
    Def { op: "curves", label: "Courbes RVB", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("force", Sock::Float, f(1.0))], outputs: &[("couleur", Sock::Color)], params: &[ParamDef::Curve("courbes")], mat: true, comp: true },
    Def { op: "ramp", label: "Rampe à 3 entrées", cat: "couleur", inputs: &[("facteur", Sock::Float, f(0.5)), ("couleur 1", Sock::Color, BLACK), ("couleur 2", Sock::Color, [0.95, 0.45, 0.2, 1.0]), ("couleur 3", Sock::Color, W)], outputs: &[("couleur", Sock::Color)], params: &[ParamDef::Float("milieu", 0.0, 1.0, 0.5)], mat: true, comp: true },
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
    Def { op: "bricks", label: "Briques", cat: "motif", inputs: &[("échelle", Sock::Float, f(6.0)), ("mortier", Sock::Float, f(0.06)), ("brique", Sock::Color, [0.62, 0.25, 0.16, 1.0]), ("joint", Sock::Color, [0.8, 0.78, 0.74, 1.0]), ("variation", Sock::Float, f(0.25)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("couleur", Sock::Color), ("masque", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "wood", label: "Bois", cat: "motif", inputs: &[("échelle", Sock::Float, f(3.0)), ("cernes", Sock::Float, f(12.0)), ("distorsion", Sock::Float, f(0.6)), ("clair", Sock::Color, [0.78, 0.56, 0.34, 1.0]), ("foncé", Sock::Color, [0.45, 0.27, 0.13, 1.0]), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("couleur", Sock::Color), ("valeur", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "marble", label: "Marbre", cat: "motif", inputs: &[("échelle", Sock::Float, f(3.0)), ("veines", Sock::Float, f(6.0)), ("turbulence", Sock::Float, f(4.0)), ("fond", Sock::Color, [0.92, 0.91, 0.88, 1.0]), ("veine", Sock::Color, [0.25, 0.25, 0.28, 1.0]), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("couleur", Sock::Color), ("valeur", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "hexagons", label: "Hexagones", cat: "motif", inputs: &[("échelle", Sock::Float, f(8.0)), ("bord", Sock::Float, f(0.08)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("bord", Sock::Float), ("cellule", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "dots", label: "Pois", cat: "motif", inputs: &[("échelle", Sock::Float, f(10.0)), ("rayon", Sock::Float, f(0.3)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("valeur", Sock::Float)], params: &[ParamDef::Choice("grille", &["carrée", "décalée"])], mat: true, comp: true },
    Def { op: "waves", label: "Ondes", cat: "motif", inputs: &[("échelle", Sock::Float, f(6.0)), ("distorsion", Sock::Float, f(1.0)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("valeur", Sock::Float)], params: &[ParamDef::Choice("type", &["bandes", "anneaux"])], mat: true, comp: true },
    Def { op: "whitenoise", label: "Bruit blanc", cat: "motif", inputs: &[("échelle", Sock::Float, f(64.0)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("valeur", Sock::Float), ("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    // Couleur supplémentaire
    Def { op: "curve", label: "Courbe (contraste en S)", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("ombres", Sock::Float, f(0.0)), ("lumières", Sock::Float, f(0.0)), ("contraste S", Sock::Float, f(0.3))], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "whitebal", label: "Balance des blancs", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("température", Sock::Float, f(0.0)), ("teinte", Sock::Float, f(0.0))], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "exposure", label: "Exposition", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("EV", Sock::Float, f(0.0))], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "clamp", label: "Limiter", cat: "math", inputs: &[("valeur", Sock::Float, f(0.5)), ("min", Sock::Float, f(0.0)), ("max", Sock::Float, f(1.0))], outputs: &[("valeur", Sock::Float)], params: &[], mat: true, comp: true },
    // Filtres supplémentaires
    Def { op: "dirblur", label: "Flou directionnel", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("angle", -180.0, 180.0, 0.0), ParamDef::Float("longueur (px)", 0.0, 200.0, 20.0)], mat: false, comp: true },
    Def { op: "radialblur", label: "Flou radial (zoom)", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("force", 0.0, 0.5, 0.08)], mat: false, comp: true },
    Def { op: "lens", label: "Distorsion d'objectif", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("distorsion", -1.0, 1.0, 0.15)], mat: false, comp: true },
    // Coordonnées : à brancher dans l'entrée « coordonnées » des motifs pour les déplacer, tourner ou déformer
    Def { op: "texcoord", label: "Coordonnées de texture", cat: "entrée", inputs: &[], outputs: &[("objet", Sock::Color), ("uv", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "mapping", label: "Placement (mapping)", cat: "entrée", inputs: &[("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0]), ("échelle", Sock::Float, f(1.0))], outputs: &[("coordonnées", Sock::Color)], params: &[ParamDef::Float("x", -5.0, 5.0, 0.0), ParamDef::Float("y", -5.0, 5.0, 0.0), ParamDef::Float("z", -5.0, 5.0, 0.0), ParamDef::Float("rotation X", -180.0, 180.0, 0.0), ParamDef::Float("rotation Y", -180.0, 180.0, 0.0), ParamDef::Float("rotation Z", -180.0, 180.0, 0.0), ParamDef::Float("étirer X", 0.05, 10.0, 1.0), ParamDef::Float("étirer Y", 0.05, 10.0, 1.0), ParamDef::Float("étirer Z", 0.05, 10.0, 1.0)], mat: true, comp: true },
    Def { op: "warp", label: "Déformer les coordonnées", cat: "entrée", inputs: &[("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0]), ("force", Sock::Float, f(0.2)), ("échelle", Sock::Float, f(3.0)), ("détail", Sock::Float, f(3.0))], outputs: &[("coordonnées", Sock::Color)], params: &[], mat: true, comp: true },
    // Motifs supplémentaires
    Def { op: "ridged", label: "Bruit crêté", cat: "motif", inputs: &[("échelle", Sock::Float, f(4.0)), ("détail", Sock::Float, f(5.0)), ("netteté", Sock::Float, f(2.0)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("valeur", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "cracks", label: "Fissures", cat: "motif", inputs: &[("échelle", Sock::Float, f(6.0)), ("largeur", Sock::Float, f(0.05)), ("distorsion", Sock::Float, f(0.3)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("fissures", Sock::Float), ("cellules", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "tiles", label: "Carrelage", cat: "motif", inputs: &[("échelle", Sock::Float, f(6.0)), ("joint", Sock::Float, f(0.05)), ("biseau", Sock::Float, f(0.1)), ("variation", Sock::Float, f(0.3)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("teinte", Sock::Float), ("joint", Sock::Float), ("hauteur", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "weave", label: "Tissage", cat: "motif", inputs: &[("échelle", Sock::Float, f(20.0)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("hauteur", Sock::Float), ("sens", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "shape", label: "Masque de forme", cat: "motif", inputs: &[("centre x", Sock::Float, f(0.5)), ("centre y", Sock::Float, f(0.5)), ("largeur", Sock::Float, f(0.5)), ("hauteur", Sock::Float, f(0.5)), ("douceur", Sock::Float, f(0.05))], outputs: &[("masque", Sock::Float)], params: &[ParamDef::Choice("forme", &["ellipse", "rectangle", "losange"])], mat: true, comp: true },
    // Usure : masques à la manière des générateurs de Substance
    Def { op: "edgewear", label: "Usure des arêtes", cat: "usure", inputs: &[("intensité", Sock::Float, f(1.0)), ("quantité", Sock::Float, f(0.5)), ("contraste", Sock::Float, f(0.6)), ("échelle du bruit", Sock::Float, f(14.0)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("masque", Sock::Float)], params: &[], mat: true, comp: false },
    Def { op: "cavdirt", label: "Saleté des creux", cat: "usure", inputs: &[("intensité", Sock::Float, f(1.0)), ("quantité", Sock::Float, f(0.5)), ("contraste", Sock::Float, f(0.5)), ("échelle du bruit", Sock::Float, f(8.0)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("masque", Sock::Float)], params: &[], mat: true, comp: false },
    Def { op: "dust", label: "Poussière (dessus)", cat: "usure", inputs: &[("quantité", Sock::Float, f(0.5)), ("contraste", Sock::Float, f(0.5)), ("échelle du bruit", Sock::Float, f(8.0)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("masque", Sock::Float)], params: &[], mat: true, comp: false },
    Def { op: "drips", label: "Coulures", cat: "usure", inputs: &[("échelle", Sock::Float, f(10.0)), ("longueur", Sock::Float, f(0.5)), ("quantité", Sock::Float, f(0.5)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("masque", Sock::Float)], params: &[], mat: true, comp: false },
    Def { op: "scratches", label: "Rayures d'usure", cat: "usure", inputs: &[("échelle", Sock::Float, f(6.0)), ("quantité", Sock::Float, f(0.5)), ("finesse", Sock::Float, f(0.5)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("masque", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "grunge", label: "Saleté (grunge)", cat: "usure", inputs: &[("échelle", Sock::Float, f(3.0)), ("contraste", Sock::Float, f(0.5)), ("distorsion", Sock::Float, f(1.0)), ("coordonnées", Sock::Color, [0.0, 0.0, 0.0, 0.0])], outputs: &[("masque", Sock::Float)], params: &[], mat: true, comp: true },
    // Couleur et étalonnage
    Def { op: "gamma", label: "Gamma", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("gamma", Sock::Float, f(1.0))], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "tint", label: "Teinter (bichromie)", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("ombres", Sock::Color, [0.1, 0.1, 0.25, 1.0]), ("lumières", Sock::Color, [1.0, 0.9, 0.7, 1.0]), ("force", Sock::Float, f(1.0))], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "vibrance", label: "Vibrance", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("vibrance", Sock::Float, f(0.5))], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "colorbalance", label: "Balance des couleurs", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("ombres (lift)", Sock::Color, GRAY), ("tons moyens (gamma)", Sock::Color, GRAY), ("lumières (gain)", Sock::Color, GRAY)], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "splittone", label: "Virage partiel", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("teinte des ombres", Sock::Color, [0.2, 0.4, 0.6, 1.0]), ("teinte des lumières", Sock::Color, [1.0, 0.7, 0.4, 1.0]), ("force", Sock::Float, f(0.3))], outputs: &[("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "tonemap", label: "Rendu des tons (tone mapping)", cat: "couleur", inputs: &[("couleur", Sock::Color, GRAY), ("EV", Sock::Float, f(0.0))], outputs: &[("couleur", Sock::Color)], params: &[ParamDef::Choice("courbe", &["ACES", "filmique", "Reinhard"])], mat: true, comp: true },
    Def { op: "lumakey", label: "Incrustation par luminance", cat: "couleur", inputs: &[("image", Sock::Color, GRAY), ("bas", Sock::Float, f(0.1)), ("haut", Sock::Float, f(0.3))], outputs: &[("image", Sock::Color), ("masque", Sock::Float)], params: &[ParamDef::Choice("garder", &["clair", "sombre"])], mat: false, comp: true },
    Def { op: "diffkey", label: "Incrustation par différence", cat: "couleur", inputs: &[("image", Sock::Color, GRAY), ("plaque vide", Sock::Color, GRAY), ("tolérance", Sock::Float, f(0.08)), ("douceur", Sock::Float, f(0.08))], outputs: &[("image", Sock::Color), ("masque", Sock::Float)], params: &[], mat: false, comp: true },
    Def { op: "despill", label: "Retirer le débordement", cat: "couleur", inputs: &[("image", Sock::Color, GRAY), ("force", Sock::Float, f(1.0))], outputs: &[("image", Sock::Color)], params: &[ParamDef::Choice("couleur", &["vert", "bleu"])], mat: false, comp: true },
    Def { op: "roto", label: "Masque roto (tracé)", cat: "motif", inputs: &[("douceur", Sock::Float, f(0.01))], outputs: &[("masque", Sock::Float)], params: &[ParamDef::Text("points"), ParamDef::Choice("mode", &["intérieur", "extérieur"])], mat: false, comp: true },
    Def { op: "chromakey", label: "Incrustation (fond vert)", cat: "couleur", inputs: &[("image", Sock::Color, GRAY), ("couleur clé", Sock::Color, [0.1, 0.8, 0.2, 1.0]), ("tolérance", Sock::Float, f(0.25)), ("douceur", Sock::Float, f(0.1))], outputs: &[("image", Sock::Color), ("masque", Sock::Float)], params: &[], mat: false, comp: true },
    Def { op: "alphaover", label: "Superposer (alpha)", cat: "couleur", inputs: &[("dessous", Sock::Color, GRAY), ("dessus", Sock::Color, W), ("opacité", Sock::Float, f(1.0))], outputs: &[("image", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "setalpha", label: "Définir l'alpha", cat: "couleur", inputs: &[("image", Sock::Color, GRAY), ("alpha", Sock::Float, f(1.0))], outputs: &[("image", Sock::Color)], params: &[], mat: false, comp: true },
    Def { op: "mixf", label: "Mélange de valeurs", cat: "math", inputs: &[("facteur", Sock::Float, f(0.5)), ("A", Sock::Float, f(0.0)), ("B", Sock::Float, f(1.0))], outputs: &[("valeur", Sock::Float)], params: &[], mat: true, comp: true },
    // Effets d'image
    Def { op: "halftone", label: "Trame (halftone)", cat: "filtre", inputs: &[("image", Sock::Color, GRAY), ("taille (px)", Sock::Float, f(6.0)), ("angle", Sock::Float, f(45.0))], outputs: &[("image", Sock::Color)], params: &[], mat: false, comp: true },
    Def { op: "scanlines", label: "Lignes de balayage", cat: "filtre", inputs: &[("image", Sock::Color, GRAY), ("force", Sock::Float, f(0.3)), ("nombre", Sock::Float, f(240.0))], outputs: &[("image", Sock::Color)], params: &[], mat: false, comp: true },
    Def { op: "letterbox", label: "Bandes cinéma", cat: "filtre", inputs: &[("image", Sock::Color, GRAY), ("format", Sock::Float, f(2.39))], outputs: &[("image", Sock::Color)], params: &[], mat: false, comp: true },
    Def { op: "transform", label: "Transformer", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("x", -1.0, 1.0, 0.0), ParamDef::Float("y", -1.0, 1.0, 0.0), ParamDef::Float("rotation", -180.0, 180.0, 0.0), ParamDef::Float("échelle", 0.1, 4.0, 1.0)], mat: false, comp: true },
    Def { op: "flip", label: "Retourner", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Choice("sens", &["horizontal", "vertical", "les deux"])], mat: false, comp: true },
    Def { op: "displace", label: "Déformer par une image", cat: "filtre", inputs: &[("image", Sock::Color, GRAY), ("carte", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("force (px)", 0.0, 200.0, 20.0)], mat: false, comp: true },
    Def { op: "maskblur", label: "Flou masqué (tilt-shift)", cat: "filtre", inputs: &[("image", Sock::Color, GRAY), ("masque", Sock::Float, f(1.0))], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("rayon", 0.0, 100.0, 12.0)], mat: false, comp: true },
    Def { op: "sunbeams", label: "Rayons de lumière", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("seuil", 0.0, 1.0, 0.7), ParamDef::Float("longueur", 0.0, 1.0, 0.4), ParamDef::Float("force", 0.0, 3.0, 1.0), ParamDef::Float("centre x", 0.0, 1.0, 0.5), ParamDef::Float("centre y", 0.0, 1.0, 0.3)], mat: false, comp: true },
    Def { op: "emboss", label: "Gaufrage", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("force", 0.0, 4.0, 1.0), ParamDef::Float("angle", -180.0, 180.0, 135.0)], mat: false, comp: true },
    Def { op: "glitch", label: "Glitch", cat: "filtre", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("force", 0.0, 1.0, 0.3), ParamDef::Float("blocs", 2.0, 80.0, 24.0), ParamDef::Float("graine", 0.0, 100.0, 1.0)], mat: false, comp: true },
    Def { op: "unmult", label: "Retirer le noir (unmult)", cat: "couleur", inputs: &[("image", Sock::Color, GRAY)], outputs: &[("image", Sock::Color)], params: &[], mat: false, comp: true },
    Def { op: "zdefocus", label: "Flou de profondeur (passe Z)", cat: "filtre", inputs: &[("image", Sock::Color, GRAY), ("profondeur", Sock::Float, f(0.0))], outputs: &[("image", Sock::Color)], params: &[ParamDef::Float("mise au point", 0.0, 1000.0, 5.0), ParamDef::Float("étendue nette", 0.0, 500.0, 1.0), ParamDef::Float("étalement", 0.01, 500.0, 4.0), ParamDef::Float("rayon max (px)", 0.0, 80.0, 14.0)], mat: false, comp: true },
    // Vecteurs, logique, temps : valables partout
    Def { op: "vmath", label: "Math vectoriel", cat: "math", inputs: &[("A", Sock::Vector, ZERO), ("B", Sock::Vector, ZERO), ("facteur", Sock::Float, f(1.0))], outputs: &[("vecteur", Sock::Vector), ("valeur", Sock::Float)], params: &[ParamDef::Choice("opération", VMATH)], mat: true, comp: true },
    Def { op: "sepxyz", label: "Séparer XYZ", cat: "math", inputs: &[("vecteur", Sock::Vector, ZERO)], outputs: &[("X", Sock::Float), ("Y", Sock::Float), ("Z", Sock::Float)], params: &[], mat: true, comp: true },
    Def { op: "combxyz", label: "Combiner XYZ", cat: "math", inputs: &[("X", Sock::Float, f(0.0)), ("Y", Sock::Float, f(0.0)), ("Z", Sock::Float, f(0.0))], outputs: &[("vecteur", Sock::Vector)], params: &[], mat: true, comp: true },
    Def { op: "compare", label: "Comparer", cat: "math", inputs: &[("A", Sock::Float, f(0.5)), ("B", Sock::Float, f(0.5)), ("tolérance", Sock::Float, f(0.001))], outputs: &[("résultat", Sock::Bool)], params: &[ParamDef::Choice("comparaison", COMPARE)], mat: true, comp: true },
    Def { op: "logic", label: "Logique", cat: "math", inputs: &[("A", Sock::Bool, f(0.0)), ("B", Sock::Bool, f(0.0))], outputs: &[("résultat", Sock::Bool)], params: &[ParamDef::Choice("opération", LOGIC)], mat: true, comp: true },
    Def { op: "switch", label: "Aiguillage", cat: "math", inputs: &[("condition", Sock::Bool, f(0.0)), ("si faux", Sock::Color, BLACK), ("si vrai", Sock::Color, W)], outputs: &[("résultat", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "random", label: "Aléatoire", cat: "math", inputs: &[("min", Sock::Float, f(0.0)), ("max", Sock::Float, f(1.0)), ("graine", Sock::Float, f(0.0)), ("coordonnées", Sock::Color, ZERO)], outputs: &[("valeur", Sock::Float), ("couleur", Sock::Color)], params: &[], mat: true, comp: true },
    Def { op: "expr", label: "Expression", cat: "math", inputs: &[("a", Sock::Float, f(0.5)), ("b", Sock::Float, f(0.5)), ("c", Sock::Float, f(0.0)), ("d", Sock::Float, f(0.0))], outputs: &[("valeur", Sock::Float)], params: &[ParamDef::Line("formule", "a * b")], mat: true, comp: true },
    Def { op: "time", label: "Temps", cat: "entrée", inputs: &[], outputs: &[("image", Sock::Float), ("secondes", Sock::Float)], params: &[], mat: true, comp: true },
    // Structure : ce qui organise le graphe sans rien calculer
    Def { op: "reroute", label: "Renvoi", cat: "structure", inputs: &[("", Sock::Any, ZERO)], outputs: &[("", Sock::Any)], params: &[], mat: true, comp: true },
    Def { op: "repeat_in", label: "Zone de répétition", cat: "structure", inputs: &[], outputs: &[], params: &[], mat: true, comp: true },
    Def { op: "repeat_out", label: "Fin de la répétition", cat: "interne", inputs: &[], outputs: &[], params: &[], mat: true, comp: true },
    Def { op: "sim_in", label: "Zone de simulation", cat: "simulation", inputs: &[], outputs: &[], params: &[], mat: false, comp: false },
    Def { op: "sim_out", label: "Fin de la simulation", cat: "interne", inputs: &[], outputs: &[], params: &[], mat: false, comp: false },
    Def { op: "group", label: "Groupe", cat: "interne", inputs: &[], outputs: &[], params: &[], mat: true, comp: true },
    Def { op: "group_in", label: "Entrées du groupe", cat: "interne", inputs: &[], outputs: &[], params: &[], mat: true, comp: true },
    Def { op: "group_out", label: "Sorties du groupe", cat: "interne", inputs: &[], outputs: &[], params: &[], mat: true, comp: true },
    // Géométrie : des points, leurs attributs, des arêtes. Les entrées de nombre, de vecteur et de
    // couleur sont des champs : on y branche n'importe quelle chaîne de nodes, calculée point par point.
    Def { op: "geo_out", label: "Sortie géométrie", cat: "sortie", inputs: &[("géométrie", Sock::Geo, ZERO)], outputs: &[], params: &[], mat: false, comp: false },
    Def { op: "geo_grid", label: "Grille", cat: "géométrie", inputs: &[("largeur", Sock::Float, f(2.0)), ("profondeur", Sock::Float, f(2.0)), ("points en largeur", Sock::Int, f(20.0)), ("points en profondeur", Sock::Int, f(20.0))], outputs: &[("géométrie", Sock::Geo)], params: &[ParamDef::Choice("plan", &["sol (XZ)", "mur (XY)"]), ParamDef::Choice("arêtes", &["oui", "avec diagonales", "non"])], mat: false, comp: false },
    Def { op: "geo_sphere", label: "Points sur une sphère", cat: "géométrie", inputs: &[("points", Sock::Int, f(500.0)), ("rayon", Sock::Float, f(1.0))], outputs: &[("géométrie", Sock::Geo)], params: &[], mat: false, comp: false },
    Def { op: "geo_circle", label: "Cercle", cat: "géométrie", inputs: &[("points", Sock::Int, f(32.0)), ("rayon", Sock::Float, f(1.0))], outputs: &[("géométrie", Sock::Geo)], params: &[], mat: false, comp: false },
    Def { op: "geo_line", label: "Ligne", cat: "géométrie", inputs: &[("points", Sock::Int, f(10.0)), ("départ", Sock::Vector, ZERO), ("arrivée", Sock::Vector, [0.0, 1.0, 0.0, 0.0])], outputs: &[("géométrie", Sock::Geo)], params: &[], mat: false, comp: false },
    Def { op: "geo_scatter", label: "Nuage de points", cat: "géométrie", inputs: &[("points", Sock::Int, f(1000.0)), ("taille", Sock::Vector, [2.0, 2.0, 2.0, 0.0]), ("graine", Sock::Float, f(0.0))], outputs: &[("géométrie", Sock::Geo)], params: &[], mat: false, comp: false },
    Def { op: "geo_transform", label: "Déplacer, tourner", cat: "géométrie", inputs: &[("géométrie", Sock::Geo, ZERO), ("déplacement", Sock::Vector, ZERO), ("rotation", Sock::Vector, ZERO), ("échelle", Sock::Vector, [1.0, 1.0, 1.0, 1.0])], outputs: &[("géométrie", Sock::Geo)], params: &[], mat: false, comp: false },
    Def { op: "geo_set_pos", label: "Placer les points", cat: "géométrie", inputs: &[("géométrie", Sock::Geo, ZERO), ("sélection", Sock::Bool, f(1.0)), ("position", Sock::Vector, ZERO), ("décalage", Sock::Vector, ZERO)], outputs: &[("géométrie", Sock::Geo)], params: &[], mat: false, comp: false },
    Def { op: "geo_attr", label: "Définir un attribut", cat: "géométrie", inputs: &[("géométrie", Sock::Geo, ZERO), ("valeur", Sock::Color, W), ("sélection", Sock::Bool, f(1.0))], outputs: &[("géométrie", Sock::Geo)], params: &[ParamDef::Line("nom", "Cd"), ParamDef::Choice("type", &["couleur", "nombre", "vecteur"])], mat: false, comp: false },
    Def { op: "geo_merge", label: "Réunir", cat: "géométrie", inputs: &[("géométrie A", Sock::Geo, ZERO), ("géométrie B", Sock::Geo, ZERO)], outputs: &[("géométrie", Sock::Geo)], params: &[], mat: false, comp: false },
    Def { op: "geo_delete", label: "Supprimer des points", cat: "géométrie", inputs: &[("géométrie", Sock::Geo, ZERO), ("sélection", Sock::Bool, f(0.0))], outputs: &[("géométrie", Sock::Geo)], params: &[], mat: false, comp: false },
    Def { op: "geo_copy", label: "Copier sur des points", cat: "géométrie", inputs: &[("points", Sock::Geo, ZERO), ("modèle", Sock::Geo, ZERO), ("taille", Sock::Float, f(1.0))], outputs: &[("géométrie", Sock::Geo)], params: &[], mat: false, comp: false },
    Def { op: "position", label: "Position", cat: "géométrie", inputs: &[], outputs: &[("position", Sock::Vector)], params: &[], mat: false, comp: false },
    Def { op: "index", label: "Numéro du point", cat: "géométrie", inputs: &[], outputs: &[("numéro", Sock::Float)], params: &[], mat: false, comp: false },
    Def { op: "attribute", label: "Lire un attribut", cat: "géométrie", inputs: &[], outputs: &[("valeur", Sock::Float), ("vecteur", Sock::Vector)], params: &[ParamDef::Line("nom", "v")], mat: false, comp: false },
    // Simulation : ces nodes vont dans une zone de simulation, où l'état passe d'une image à la suivante
    Def { op: "sim_forces", label: "Forces", cat: "simulation", inputs: &[("géométrie", Sock::Geo, ZERO), ("gravité", Sock::Vector, [0.0, -9.8, 0.0, 0.0]), ("vent", Sock::Vector, ZERO), ("frottement de l'air", Sock::Float, f(0.1)), ("pas de temps", Sock::Float, f(1.0 / 24.0))], outputs: &[("géométrie", Sock::Geo)], params: &[], mat: false, comp: false },
    Def { op: "sim_move", label: "Avancer", cat: "simulation", inputs: &[("géométrie", Sock::Geo, ZERO), ("pas de temps", Sock::Float, f(1.0 / 24.0))], outputs: &[("géométrie", Sock::Geo)], params: &[], mat: false, comp: false },
    Def { op: "sim_collide", label: "Collision", cat: "simulation", inputs: &[("géométrie", Sock::Geo, ZERO), ("hauteur du sol", Sock::Float, f(0.0)), ("rebond", Sock::Float, f(0.4)), ("adhérence", Sock::Float, f(0.2)), ("centre de la boule", Sock::Vector, ZERO), ("rayon de la boule", Sock::Float, f(0.0))], outputs: &[("géométrie", Sock::Geo)], params: &[], mat: false, comp: false },
    Def { op: "sim_emit", label: "Émettre des particules", cat: "simulation", inputs: &[("état", Sock::Geo, ZERO), ("source", Sock::Geo, ZERO), ("par image", Sock::Int, f(20.0)), ("vitesse", Sock::Vector, [0.0, 2.0, 0.0, 0.0]), ("dispersion", Sock::Float, f(0.5)), ("durée de vie (s)", Sock::Float, f(3.0)), ("pas de temps", Sock::Float, f(1.0 / 24.0))], outputs: &[("géométrie", Sock::Geo)], params: &[], mat: false, comp: false },
    Def { op: "sim_solver", label: "Solveur", cat: "simulation", inputs: &[("géométrie", Sock::Geo, ZERO), ("itérations", Sock::Int, f(16.0)), ("souplesse", Sock::Float, f(0.0)), ("amortissement", Sock::Float, f(0.02)), ("pas de temps", Sock::Float, f(1.0 / 24.0)), ("hauteur du sol", Sock::Float, f(0.0)), ("centre de la boule", Sock::Vector, ZERO), ("rayon de la boule", Sock::Float, f(0.0)), ("adhérence", Sock::Float, f(0.6))], outputs: &[("géométrie", Sock::Geo)], params: &[ParamDef::Choice("solveur", crate::geo::SOLVER_NAMES), ParamDef::Choice("sol", &["oui", "non"])], mat: false, comp: false },
    Def { op: "grainmatch", label: "Grain assorti (mesuré sur la plaque)", cat: "filtre", inputs: &[("image", Sock::Color, GRAY), ("plaque", Sock::Color, GRAY)], outputs: &[("image", Sock::Color), ("force mesurée", Sock::Float)], params: &[ParamDef::Float("taille du grain", 0.5, 4.0, 1.0), ParamDef::Float("dosage", 0.0, 2.0, 1.0)], mat: false, comp: true },
];

pub const DEFAULT_CURVES: &str = "0,0;1,1|0,0;1,1|0,0;1,1|0,0;1,1";

/// Lit quatre courbes (ensemble, rouge, vert, bleu) : des points « x,y » triés par x.
pub fn parse_curves(t: &str) -> [Vec<[f32; 2]>; 4] {
    let mut out: [Vec<[f32; 2]>; 4] = Default::default();
    for (k, part) in t.split('|').take(4).enumerate() {
        out[k] = part
            .split(';')
            .filter_map(|s| {
                let (x, y) = s.split_once(',')?;
                let (x, y): (f32, f32) = (x.trim().parse().ok()?, y.trim().parse().ok()?);
                (x.is_finite() && y.is_finite()).then(|| [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)])
            })
            .collect();
    }
    for c in &mut out {
        if c.len() < 2 {
            *c = vec![[0.0, 0.0], [1.0, 1.0]];
        }
        c.sort_by(|a, b| a[0].total_cmp(&b[0]));
    }
    out
}

pub fn curves_string(c: &[Vec<[f32; 2]>; 4]) -> String {
    c.iter().map(|pts| pts.iter().map(|p| format!("{:.4},{:.4}", p[0], p[1])).collect::<Vec<_>>().join(";")).collect::<Vec<_>>().join("|")
}

/// Valeur d'une courbe en `x` : passe par tous les points, en douceur, sans dépasser 0 à 1.
pub fn eval_curve(pts: &[[f32; 2]], x: f32) -> f32 {
    let x = if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.0 };
    let n = pts.len();
    if x <= pts[0][0] {
        return pts[0][1];
    }
    if x >= pts[n - 1][0] {
        return pts[n - 1][1];
    }
    let i = (0..n - 1).find(|&i| x <= pts[i + 1][0]).unwrap_or(n - 2);
    let (a, b) = (pts[i], pts[i + 1]);
    let w = (b[0] - a[0]).max(1e-6);
    // Pentes prises sur les voisins ; aux extrémités, celle du segment
    let slope = |j: usize| {
        if j == 0 || j == n - 1 {
            let (p, q) = if j == 0 { (pts[0], pts[1]) } else { (pts[n - 2], pts[n - 1]) };
            (q[1] - p[1]) / (q[0] - p[0]).max(1e-6)
        } else {
            (pts[j + 1][1] - pts[j - 1][1]) / (pts[j + 1][0] - pts[j - 1][0]).max(1e-6)
        }
    };
    let t = (x - a[0]) / w;
    let (t2, t3) = (t * t, t * t * t);
    let y = (2.0 * t3 - 3.0 * t2 + 1.0) * a[1] + (t3 - 2.0 * t2 + t) * w * slope(i) + (-2.0 * t3 + 3.0 * t2) * b[1] + (t3 - t2) * w * slope(i + 1);
    y.clamp(0.0, 1.0)
}

pub const DEFAULT_RAMP: &str = "0,0,0,0,1;1,1,1,1,1";

/// Lit un dégradé « position,r,v,b,a;… », trié par position.
pub fn parse_ramp(t: &str) -> Vec<(f32, V)> {
    let mut stops: Vec<(f32, V)> = t
        .split(';')
        .filter_map(|s| {
            let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            (v.len() == 5 && v.iter().all(|x| x.is_finite())).then(|| (v[0].clamp(0.0, 1.0), [v[1], v[2], v[3], v[4]]))
        })
        .collect();
    if stops.is_empty() {
        stops = vec![(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 1.0])];
    }
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    stops
}

pub fn ramp_string(stops: &[(f32, V)]) -> String {
    stops.iter().map(|(p, c)| format!("{:.4},{:.4},{:.4},{:.4},{:.4}", p, c[0], c[1], c[2], c[3])).collect::<Vec<_>>().join(";")
}

/// Couleur du dégradé en `t` (0 linéaire, 1 lisse, 2 constante).
pub fn eval_ramp(stops: &[(f32, V)], t: f32, mode: u8) -> V {
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    if t <= stops[0].0 {
        return stops[0].1;
    }
    for w in stops.windows(2) {
        if t <= w[1].0 {
            if mode == 2 {
                return w[0].1;
            }
            let k = ((t - w[0].0) / (w[1].0 - w[0].0).max(1e-6)).clamp(0.0, 1.0);
            let k = if mode == 1 { k * k * (3.0 - 2.0 * k) } else { k };
            return [0, 1, 2, 3].map(|c| w[0].1[c] + (w[1].1[c] - w[0].1[c]) * k);
        }
    }
    stops[stops.len() - 1].1
}

static ROOT: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Dossier du coffre (les matériaux à calques y retrouvent leurs maîtres).
pub fn set_vault_root(r: &str) {
    if let Ok(mut g) = ROOT.lock() {
        *g = r.to_string();
    }
}

pub fn vault_root() -> String {
    ROOT.lock().map(|g| g.clone()).unwrap_or_default()
}

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
    /// Prises propres à ce node (groupes, zones) : entrées puis sorties
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ins: Vec<crate::nodeflow::Port>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outs: Vec<crate::nodeflow::Port>,
}

impl Node {
    /// Définition de ce node : celle de son type, ou une définition sur mesure s'il porte ses propres prises.
    pub fn def(&self) -> Option<&'static Def> {
        crate::nodeflow::node_def(self)
    }
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
    /// Matériau à calques (atelier) : la pile ; le graphe effectif en est tiré
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<crate::atelier::Stack>,
    /// Groupes réutilisables : des sous-graphes avec leurs entrées et leurs sorties
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<crate::nodeflow::Group>,
    /// Réglages de lecture (géométrie et simulation) : images par seconde, première et dernière image
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<crate::nodeflow::TimeRange>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Frame {
    pub rect: [f32; 4],
    pub label: String,
    pub color: [u8; 3],
    /// Texte libre : le cadre sert alors de note posée sur le graphe
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl Graph {
    pub fn empty(kind: GraphKind) -> Graph {
        Graph { kind, nodes: vec![], links: vec![], next_id: 0, frames: vec![], stack: None, groups: vec![], time: None }
    }

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
                ParamDef::Ramp(n) => {
                    params.insert(n.to_string(), Param::S(DEFAULT_RAMP.to_string()));
                }
                ParamDef::Curve(n) => {
                    params.insert(n.to_string(), Param::S(DEFAULT_CURVES.to_string()));
                }
                ParamDef::Line(n, v) => {
                    params.insert(n.to_string(), Param::S(v.to_string()));
                }
            }
        }
        let preview = !is_out_op(op) && !matches!(op, "reroute" | "group_in" | "group_out" | "repeat_in" | "repeat_out" | "sim_in" | "sim_out") && d.cat != "simulation";
        self.nodes.push(Node { id, op: op.into(), pos, values: d.inputs.iter().map(|i| i.2).collect(), params, muted: false, collapsed: false, preview, label: String::new(), ins: vec![], outs: vec![] });
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

    /// Graphe réellement calculé. Les nodes muets, les renvois, les groupes et les zones sont
    /// traités à la compilation (`nodeflow::lower`) ; ici, seule la pile d'un matériau à calques est dépliée.
    pub fn effective(&self) -> std::borrow::Cow<'_, Graph> {
        if let Some(stack) = &self.stack {
            return std::borrow::Cow::Owned(crate::atelier::flatten(stack, &vault_root()));
        }
        std::borrow::Cow::Borrowed(self)
    }

    /// Nom du node de sortie de ce genre de graphe.
    pub fn out_op(&self) -> &'static str {
        match self.kind {
            GraphKind::Material => "mat_out",
            GraphKind::Compositor => "comp_out",
            GraphKind::Geometry => "geo_out",
        }
    }

    /// Copie du graphe où la sortie montre la sortie `out` du node `id` (aperçus).
    pub fn probe(&self, id: u32, out: usize) -> Option<Graph> {
        let out_op = self.out_op();
        let mut g = self.effective().into_owned();
        // À l'intérieur d'un groupe, il n'y a pas de sortie : on en pose une le temps de l'aperçu
        let target = match g.nodes.iter().find(|n| n.op == out_op) {
            Some(n) => n.id,
            None => g.add(out_op, [0.0, 0.0]),
        };
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
        let mut g = Graph::empty(GraphKind::Material);
        let noise = g.add("noise", [0.0, 0.0]);
        let ramp = g.add("ramp", [220.0, 0.0]);
        let out = g.add("mat_out", [460.0, 40.0]);
        g.connect(noise, 0, ramp, 0);
        g.connect(ramp, 0, out, 0);
        g
    }

    /// Matériau à calques (atelier) : une pile vide, le graphe vient de ses calques.
    pub fn new_layered() -> Graph {
        Graph { stack: Some(crate::atelier::Stack::default()), ..Graph::empty(GraphKind::Material) }
    }

    /// Graphe de géométrie de départ : un tissu simulé, à modifier ou à vider.
    pub fn new_geometry() -> Graph {
        crate::geo::starter()
    }

    pub fn new_compositor() -> Graph {
        let mut g = Graph::empty(GraphKind::Compositor);
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
        let mut g: Graph = serde_json::from_str(&t).map_err(|e| format!("Graphe illisible : {e}"))?;
        crate::nodeflow::sync(&mut g);
        Ok(g)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        crate::vault::atomic_write(path, serde_json::to_string_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
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
    let mut g = Graph::empty(GraphKind::Material);
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
        "carrelage en relief" => {
            let t = g.add("tiles", [0.0, 80.0]);
            let col = g.add("mix", [260.0, 40.0]);
            set(&mut g, col, 1, [0.75, 0.72, 0.66, 1.0]);
            set(&mut g, col, 2, [0.9, 0.88, 0.84, 1.0]);
            g.connect(t, 0, col, 0);
            let grout = g.add("mix", [470.0, 40.0]);
            set(&mut g, grout, 2, [0.25, 0.24, 0.22, 1.0]);
            g.connect(t, 1, grout, 0);
            g.connect(col, 0, grout, 1);
            g.connect(grout, 0, out, 0);
            let r = g.add("mixf", [470.0, 260.0]);
            set(&mut g, r, 1, f(0.12));
            set(&mut g, r, 2, f(0.9));
            g.connect(t, 1, r, 0);
            g.connect(r, 0, out, 1);
            g.connect(t, 2, out, 11);
            set(&mut g, out, 12, f(1.5));
        }
        "béton fissuré" => {
            let gr = g.add("grunge", [0.0, 40.0]);
            set(&mut g, gr, 0, f(4.0));
            set(&mut g, gr, 1, f(0.2));
            let col = g.add("mix", [260.0, 40.0]);
            set(&mut g, col, 1, [0.45, 0.45, 0.44, 1.0]);
            set(&mut g, col, 2, [0.66, 0.65, 0.62, 1.0]);
            g.connect(gr, 0, col, 0);
            let cr = g.add("cracks", [0.0, 260.0]);
            set(&mut g, cr, 0, f(5.0));
            set(&mut g, cr, 1, f(0.04));
            let dark = g.add("mix", [470.0, 40.0]);
            set(&mut g, dark, 2, [0.08, 0.08, 0.08, 1.0]);
            g.connect(cr, 0, dark, 0);
            g.connect(col, 0, dark, 1);
            g.connect(dark, 0, out, 0);
            let inv = g.add("math", [260.0, 300.0]);
            set(&mut g, inv, 0, f(1.0));
            g.nodes.iter_mut().find(|n| n.id == inv).unwrap().params.insert("opération".into(), Param::S("soustraire".into()));
            g.connect(cr, 0, inv, 1);
            g.connect(inv, 0, out, 11);
            set(&mut g, out, 12, f(2.0));
            set(&mut g, out, 1, f(0.9));
        }
        "métal brossé rayé" => {
            let sc = g.add("scratches", [0.0, 80.0]);
            set(&mut g, sc, 0, f(3.0));
            set(&mut g, sc, 1, f(0.5));
            let r = g.add("mixf", [260.0, 80.0]);
            set(&mut g, r, 1, f(0.22));
            set(&mut g, r, 2, f(0.6));
            g.connect(sc, 0, r, 0);
            g.connect(r, 0, out, 1);
            g.connect(sc, 0, out, 11);
            set(&mut g, out, 12, f(-0.25));
            set(&mut g, out, 0, [0.78, 0.79, 0.8, 1.0]);
            set(&mut g, out, 2, f(1.0));
        }
        "velours" => {
            set(&mut g, out, 0, [0.35, 0.04, 0.1, 1.0]);
            set(&mut g, out, 1, f(0.95));
            set(&mut g, out, 8, f(0.2));
            set(&mut g, out, 13, f(1.0));
        }
        "cire" => {
            set(&mut g, out, 0, [0.92, 0.82, 0.62, 1.0]);
            set(&mut g, out, 1, f(0.35));
            set(&mut g, out, 14, f(1.0));
            set(&mut g, out, 15, [1.0, 0.45, 0.2, 1.0]);
        }
        "tissu tissé" => {
            let w = g.add("weave", [0.0, 80.0]);
            set(&mut g, w, 0, f(26.0));
            let col = g.add("mix", [260.0, 40.0]);
            set(&mut g, col, 1, [0.1, 0.16, 0.3, 1.0]);
            set(&mut g, col, 2, [0.25, 0.36, 0.6, 1.0]);
            g.connect(w, 0, col, 0);
            g.connect(col, 0, out, 0);
            g.connect(w, 0, out, 11);
            set(&mut g, out, 12, f(1.2));
            set(&mut g, out, 1, f(0.85));
            set(&mut g, out, 13, f(0.5));
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

pub const MATERIAL_PRESETS: [&str; 21] = ["carrelage en relief", "béton fissuré", "métal brossé rayé", "velours", "cire", "tissu tissé", "métal peint usé", "pierre aux creux sombres", "bois", "pierre", "métal rouillé", "toon", "céramique", "or", "verre", "verre dépoli", "eau", "toon 3 bandes", "peinture de voiture", "plastique", "néon"];

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

/// Variables du node « Expression » : ses quatre entrées, la position, les coordonnées, pi, et le résultat `o`.
const EXPR_VARS: [&str; 11] = ["a", "b", "c", "d", "x", "y", "z", "u", "v", "pi", "o"];

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
        Self::compile_vars(code, &VARS)
    }

    /// Compile avec une autre liste de variables prédéfinies (node « Expression », champs de géométrie).
    pub fn compile_vars(code: &str, vars: &[&str]) -> Result<Shader, String> {
        let mut names: Vec<String> = vars.iter().map(|s| s.to_string()).collect();
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

/// Distances aux deux germes les plus proches (pour les fissures et les joints).
fn voronoi_f12(x: f32, y: f32, z: f32) -> (f32, f32) {
    let (xi, yi, zi) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let (mut a, mut b) = (f32::MAX, f32::MAX);
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (cx, cy, cz) = (xi + dx, yi + dy, zi + dz);
                let px = cx as f32 + hash3(cx, cy, cz);
                let py = cy as f32 + hash3(cx + 17, cy, cz);
                let pz = cz as f32 + hash3(cx, cy + 31, cz);
                let d = ((px - x).powi(2) + (py - y).powi(2) + (pz - z).powi(2)).sqrt();
                if d < a {
                    b = a;
                    a = d;
                } else if d < b {
                    b = d;
                }
            }
        }
    }
    (a, b)
}

/// Deux coordonnées à plat pour les motifs 2D : les UV d'une image, ou le plan qui fait face à la normale.
fn flat(c: &Ctx, p: [f32; 3]) -> (f32, f32) {
    let n = [c.nrm[0].abs(), c.nrm[1].abs(), c.nrm[2].abs()];
    if c.pos == [0.0; 3] || n == [0.0; 3] {
        (p[0], p[1])
    } else if n[1] >= n[0] && n[1] >= n[2] {
        (p[0], p[2])
    } else if n[0] >= n[2] {
        (p[2], p[1])
    } else {
        (p[0], p[1])
    }
}

/// Masque à partir d'une valeur, d'une quantité et d'un contraste (0 : fondu large, 1 : bord net).
fn mask(v: f32, contrast: f32) -> f32 {
    let w = (1.0 - contrast.clamp(0.0, 1.0)) * 0.5 + 0.01;
    smoothstep(0.5 - w, 0.5 + w, v)
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
    /// Occlusion : assombrit la lumière ambiante (1 : aucune)
    pub ao: f32,
    /// Hauteur pour le relief, et sa force
    pub bump: f32,
    pub bump_k: f32,
    /// Reflet doux en bordure (tissu, velours)
    pub sheen: f32,
    /// Lumière qui traverse la matière (peau, cire) et sa teinte
    pub sss: f32,
    pub sss_color: V,
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
    /// Masques peints à la main sur l'objet (quatre canaux, 0 quand rien n'est peint)
    pub masks: [f32; 4],
}

/// Graphe compilé : nodes dans l'ordre d'exécution, entrées résolues.
#[derive(Clone)]
pub struct Program {
    steps: Vec<Step>,
    regs: usize,
    pub kind: GraphKind,
    /// Vrai si un node lit la courbure : le rendu ne la calcule que dans ce cas
    pub uses_curvature: bool,
    /// Vrai si l'entrée « relief » de la sortie est branchée : le rendu évalue alors la pente
    pub uses_bump: bool,
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
    /// Dégradé déjà lu (node « Rampe de couleur »)
    ramp: Option<std::sync::Arc<Vec<(f32, V)>>>,
    /// Courbes déjà calculées en tables de 256 valeurs (node « Courbes RVB »)
    curves: Option<std::sync::Arc<[Vec<f32>; 4]>>,
    /// Empreinte de ce que ce node calcule : son type, ses réglages, et l'empreinte de ce qui y entre.
    /// Deux nodes de même empreinte donnent la même image : le compositing la reprend du cache.
    key: u64,
}

/// Cache du compositing : l'image sortie par chaque node, rangée sous son empreinte et sa taille.
/// Changer un réglage ne refait que ce node et ceux qui le suivent ; les miniatures et l'aperçu
/// partagent ce qu'ils ont en commun.
struct CompCache {
    map: HashMap<(u64, usize, usize), (Vec<std::sync::Arc<Vec<V>>>, u64)>,
    bytes: usize,
    tick: u64,
}

static COMP_CACHE: std::sync::Mutex<Option<CompCache>> = std::sync::Mutex::new(None);
/// Mémoire accordée au cache du compositing
const COMP_BUDGET: usize = 384 << 20;

thread_local! {
    /// Nombre de nodes réellement calculés par le dernier compositing de ce fil (pour les essais et les mesures)
    pub static COMP_COMPUTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn comp_cache_get(key: (u64, usize, usize)) -> Option<Vec<std::sync::Arc<Vec<V>>>> {
    let mut g = COMP_CACHE.lock().ok()?;
    let c = g.as_mut()?;
    c.tick += 1;
    let tick = c.tick;
    let e = c.map.get_mut(&key)?;
    e.1 = tick;
    Some(e.0.clone())
}

fn comp_cache_put(key: (u64, usize, usize), outs: &[std::sync::Arc<Vec<V>>]) {
    let Ok(mut g) = COMP_CACHE.lock() else { return };
    let c = g.get_or_insert_with(|| CompCache { map: HashMap::new(), bytes: 0, tick: 0 });
    let bytes: usize = outs.iter().map(|o| o.len() * 16).sum();
    if bytes > COMP_BUDGET / 4 {
        return;
    }
    c.tick += 1;
    if let Some(old) = c.map.insert(key, (outs.to_vec(), c.tick)) {
        c.bytes -= old.0.iter().map(|o| o.len() * 16).sum::<usize>();
    }
    c.bytes += bytes;
    // Trop plein : les images les moins récemment servies s'en vont
    while c.bytes > COMP_BUDGET {
        let Some(k) = c.map.iter().filter(|(k, _)| **k != key).min_by_key(|(_, v)| v.1).map(|(k, _)| *k) else { break };
        if let Some(old) = c.map.remove(&k) {
            c.bytes -= old.0.iter().map(|o| o.len() * 16).sum::<usize>();
        }
    }
}

#[derive(Clone, Copy)]
enum Src {
    Reg(usize),
    Const(V),
}

impl Program {
    /// Programme vide (un graphe de géométrie n'a pas de programme point par point).
    pub fn empty(kind: GraphKind) -> Program {
        Program { steps: vec![], regs: 0, kind, uses_curvature: false, uses_bump: false }
    }
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

/// Une vidéo ou un montage, utilisable comme texture (node « Texture vidéo »).
pub fn is_video_source(rel: &str) -> bool {
    crate::video::is_video(rel) || crate::montage::is_project(rel)
}

/// Image d'une vidéo ou d'un montage au temps `t` (secondes), enregistrée en PNG dans le cache du coffre.
/// Recalculée seulement si le fichier change ou si le temps change.
pub fn video_frame(root: &str, rel: &str, path: &Path, t: f32) -> Result<std::path::PathBuf, String> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    rel.hash(&mut h);
    ((t * 100.0).round() as i64).hash(&mut h);
    std::fs::metadata(path).and_then(|m| m.modified()).ok().hash(&mut h);
    let dir = Path::new(root).join(".app/cache/videoframes");
    let out = dir.join(format!("{:016x}.png", h.finish()));
    if out.exists() {
        return Ok(out);
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if crate::montage::is_project(rel) {
        let p = crate::montage::Project::read(path)?;
        crate::montage::frame(&p, path, t, 1024)?.save(&out).map_err(|e| e.to_string())?;
    } else {
        let ff = crate::ffmpeg::find("ffmpeg").ok_or("ffmpeg n'est pas installé.")?;
        let o = crate::ffmpeg::command(ff).args(["-v", "error", "-ss", &format!("{t:.3}"), "-i"]).arg(path).args(["-frames:v", "1", "-vf", "scale='min(1024,iw)':-2", "-y"]).arg(&out).output().map_err(|e| e.to_string())?;
        if !out.exists() {
            return Err(format!("image de la vidéo impossible : {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("?")));
        }
    }
    Ok(out)
}

/// Points d'un tracé roto : « x,y;x,y;… », coordonnées entre 0 et 1.
pub fn parse_points(s: &str) -> Vec<[f32; 2]> {
    s.split(';').filter_map(|p| {
        let mut it = p.split(',');
        Some([it.next()?.trim().parse().ok()?, it.next()?.trim().parse().ok()?])
    }).collect()
}

pub fn format_points(pts: &[[f32; 2]]) -> String {
    pts.iter().map(|p| format!("{:.4},{:.4}", p[0], p[1])).collect::<Vec<_>>().join(";")
}

fn point_in_poly(pts: &[[f32; 2]], q: [f32; 2]) -> bool {
    let mut inside = false;
    let n = pts.len();
    for k in 0..n {
        let (a, b) = (pts[k], pts[(k + n - 1) % n]);
        if (a[1] > q[1]) != (b[1] > q[1]) && q[0] < (b[0] - a[0]) * (q[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
    }
    inside
}

fn poly_distance(pts: &[[f32; 2]], q: [f32; 2]) -> f32 {
    let n = pts.len();
    (0..n).map(|k| {
        let (a, b) = (pts[k], pts[(k + 1) % n]);
        let ab = [b[0] - a[0], b[1] - a[1]];
        let t = (((q[0] - a[0]) * ab[0] + (q[1] - a[1]) * ab[1]) / (ab[0] * ab[0] + ab[1] * ab[1]).max(1e-12)).clamp(0.0, 1.0);
        ((q[0] - a[0] - ab[0] * t).powi(2) + (q[1] - a[1] - ab[1] * t).powi(2)).sqrt()
    }).fold(f32::MAX, f32::min)
}

/// Les calques (passes) d'un EXR : « diffuse », « ViewLayer.Combined »… (vide : le calque principal R G B A).
pub fn exr_passes(path: &Path) -> Result<Vec<String>, String> {
    let img = exr::prelude::read_all_flat_layers_from_file(path).map_err(|e| e.to_string())?;
    let mut out: Vec<String> = vec![];
    for layer in &img.layer_data {
        let lname = layer.attributes.layer_name.as_ref().map(|t| t.to_string()).unwrap_or_default();
        for ch in &layer.channel_data.list {
            let name = ch.name.to_string();
            let pass = name.rsplit_once('.').map(|(p, _)| p.to_string()).unwrap_or_default();
            let full = [lname.as_str(), pass.as_str()].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join(".");
            if !out.contains(&full) {
                out.push(full);
            }
        }
    }
    Ok(out)
}

/// Charge une passe d'un EXR multicouche. Les canaux R G B A (ou X Y Z, ou un seul canal) donnent la couleur.
pub fn load_exr_pass(path: &Path, pass: &str, max: u32) -> Result<Img, String> {
    let img = exr::prelude::read_all_flat_layers_from_file(path).map_err(|e| e.to_string())?;
    let want = pass.trim().to_lowercase();
    for layer in &img.layer_data {
        let lname = layer.attributes.layer_name.as_ref().map(|t| t.to_string()).unwrap_or_default();
        let (w, h) = (layer.size.0, layer.size.1);
        // Canaux de la passe : leur nom complet (calque.passe) correspond à ce qu'on cherche
        let chans: Vec<(String, &exr::prelude::FlatSamples)> = layer.channel_data.list.iter().filter_map(|ch| {
            let name = ch.name.to_string();
            let (p, c) = name.rsplit_once('.').map(|(p, c)| (p.to_string(), c.to_string())).unwrap_or((String::new(), name.clone()));
            let full = [lname.as_str(), p.as_str()].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join(".").to_lowercase();
            let ok = full == want || (!want.is_empty() && (p.to_lowercase() == want || full.ends_with(&format!(".{want}"))));
            ok.then_some((c.to_uppercase(), &ch.sample_data))
        }).collect();
        if chans.is_empty() {
            continue;
        }
        let pick = |names: &[&str]| chans.iter().find(|(c, _)| names.contains(&c.as_str())).map(|x| x.1);
        let single = if chans.len() == 1 { Some(chans[0].1) } else { None };
        let r = pick(&["R", "X", "U"]).or(single);
        let g = pick(&["G", "Y", "V"]).or(single).or(r);
        let b = pick(&["B", "Z", "W"]).or(single).or(r);
        let a = pick(&["A"]);
        let px: Vec<V> = (0..w * h).map(|i| {
            let v = |s: Option<&exr::prelude::FlatSamples>, d: f32| s.map(|s| s.value_by_flat_index(i).to_f32()).unwrap_or(d);
            [v(r, 0.0), v(g, 0.0), v(b, 0.0), v(a, 1.0)]
        }).collect();
        let full = Img { w, h, px };
        // Trop grande : réduite par échantillonnage
        if w as u32 > max || h as u32 > max {
            let k = max as f32 / w.max(h) as f32;
            let (nw, nh) = (((w as f32 * k) as usize).max(1), ((h as f32 * k) as usize).max(1));
            let px = (0..nw * nh).map(|i| full.sample(((i % nw) as f32 + 0.5) / nw as f32, ((i / nw) as f32 + 0.5) / nh as f32)).collect();
            return Ok(Img { w: nw, h: nh, px });
        }
        return Ok(full);
    }
    let list = exr_passes(path).unwrap_or_default();
    Err(format!("calque « {pass} » introuvable ; calques : {}", list.iter().map(|x| if x.is_empty() { "(principal)" } else { x.as_str() }).collect::<Vec<_>>().join(", ")))
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
        Self::compile_at(g, root, max_img, 0.0, 24.0)
    }

    /// Comme `compile`, à une image donnée de l'animation (le node « Temps » la lit).
    pub fn compile_at(g: &Graph, root: &str, max_img: u32, frame: f32, fps: f32) -> Result<Program, (u32, String)> {
        // Matériau à calques : on compile le graphe tiré de sa pile
        if let Some(stack) = &g.stack {
            return Self::compile_at(&crate::atelier::flatten(stack, root), root, max_img, frame, fps);
        }
        // Groupes dépliés, répétitions déroulées, renvois et nodes muets retirés
        let low = crate::nodeflow::lower(g, true)?;
        let out_node = low.graph.nodes.iter().find(|n| n.op == low.graph.out_op()).map(|n| n.id);
        Self::compile_flat(&low.graph, out_node, 0, &[], root, max_img, frame, fps).map_err(|(id, e)| (low.origin(id), e))
    }

    /// Champ : le petit programme qui calcule l'entrée `inp` du node `node`, élément par élément.
    /// `attrs` : les attributs que le graphe peut lire ; leurs valeurs occupent les premiers registres.
    pub fn compile_field(g: &Graph, node: u32, inp: usize, attrs: &[String], root: &str, frame: f32, fps: f32) -> Result<Program, (u32, String)> {
        Self::compile_flat(g, Some(node), inp + 1, attrs, root, 256, frame, fps)
    }

    /// Compile un graphe déjà mis à plat. `only` : 0 pour tout le node de sortie, sinon k + 1 pour
    /// ne calculer que son entrée k (un champ).
    #[allow(clippy::too_many_arguments)]
    fn compile_flat(g: &Graph, out_node: Option<u32>, only: usize, attrs: &[String], root: &str, max_img: u32, frame: f32, fps: f32) -> Result<Program, (u32, String)> {
        // Ordre topologique depuis la sortie
        let mut order: Vec<u32> = vec![];
        let mut state: HashMap<u32, u8> = HashMap::new();
        // Les liens rangés par node d'arrivée : un graphe déroulé peut compter des milliers de nodes
        let mut into: HashMap<u32, Vec<&Link>> = HashMap::new();
        for l in &g.links {
            into.entry(l.to).or_default().push(l);
        }
        let by_id: HashMap<u32, &Node> = g.nodes.iter().map(|n| (n.id, n)).collect();
        // Parcours sans récursion : une longue chaîne ne doit pas faire déborder la pile
        fn visit(start: u32, only: usize, into: &HashMap<u32, Vec<&Link>>, order: &mut Vec<u32>, st: &mut HashMap<u32, u8>) {
            let mut stack: Vec<(u32, usize)> = vec![(start, 0)];
            st.insert(start, 1);
            while let Some((id, k)) = stack.pop() {
                let ins = into.get(&id).map(|v| v.as_slice()).unwrap_or(&[]);
                let mut k = k;
                let mut pushed = false;
                while k < ins.len() {
                    let l = ins[k];
                    k += 1;
                    if id == start && only > 0 && l.inp != only - 1 {
                        continue;
                    }
                    if st.get(&l.from).copied().unwrap_or(0) == 0 {
                        st.insert(l.from, 1);
                        stack.push((id, k));
                        stack.push((l.from, 0));
                        pushed = true;
                        break;
                    }
                }
                if !pushed {
                    st.insert(id, 2);
                    order.push(id);
                }
            }
        }
        match out_node {
            Some(o) => visit(o, only, &into, &mut order, &mut state),
            None => return Err((u32::MAX, "il manque le node de sortie".into())),
        }
        let mut base: HashMap<u32, usize> = HashMap::new();
        let mut keys: HashMap<u32, u64> = HashMap::new();
        // Les attributs lus par un champ occupent les premiers registres
        let mut regs = attrs.len();
        let mut steps = vec![];
        for id in order {
            let Some(n) = by_id.get(&id).copied() else { continue };
            if only > 0 && Some(id) == out_node {
                // Champ : seule l'entrée demandée compte, le node lui-même n'est pas exécuté ici
                let src = match into.get(&id).and_then(|v| v.iter().find(|l| l.inp == only - 1)) {
                    Some(l) => base.get(&l.from).map(|b| Src::Reg(b + l.out)).unwrap_or(Src::Const([0.0; 4])),
                    None => Src::Const(n.values.get(only - 1).copied().or_else(|| n.def().and_then(|d| d.inputs.get(only - 1)).map(|i| i.2)).unwrap_or([0.0; 4])),
                };
                steps.push(Step { op: "field_out", ins: vec![src], base: regs, nout: 0, params: BTreeMap::new(), shader: None, image: None, ramp: None, curves: None, key: 0 });
                continue;
            }
            let Some(d) = n.def() else { return Err((id, format!("node inconnu : {}", n.op))) };
            // Un attribut lu par un champ : son registre est déjà rempli par l'appelant
            if n.op == "attribute" || n.op == "index" || n.op == "position" {
                let name = match n.op.as_str() {
                    "index" => "index",
                    "position" => "P",
                    _ => pstr(&n.params, "nom"),
                };
                match attrs.iter().position(|a| a == name) {
                    Some(k) => {
                        base.insert(id, regs);
                        steps.push(Step { op: "attr_read", ins: vec![Src::Reg(k)], base: regs, nout: d.outputs.len(), params: BTreeMap::new(), shader: None, image: None, ramp: None, curves: None, key: 0 });
                        regs += 4;
                        continue;
                    }
                    None if attrs.is_empty() => return Err((id, "ce node lit un attribut de géométrie : il va dans un graphe de géométrie".into())),
                    None => return Err((id, format!("attribut introuvable : {name}"))),
                }
            }
            let ins = (0..d.inputs.len())
                .map(|i| match into.get(&id).and_then(|v| v.iter().find(|l| l.inp == i)) {
                    Some(l) => match base.get(&l.from) {
                        Some(b) => Src::Reg(b + l.out),
                        None => Src::Const(d.inputs[i].2),
                    },
                    None => Src::Const(n.values.get(i).copied().unwrap_or(d.inputs[i].2)),
                })
                .collect();
            let shader = match n.op.as_str() {
                "shader" => Some(Shader::compile(pstr(&n.params, "code")).map_err(|e| (id, e))?),
                "expr" => Some(Shader::compile_vars(&format!("o = {}", pstr(&n.params, "formule").replace('\n', " ")), &EXPR_VARS).map_err(|e| (id, e.replace("ligne 1 : ", "")))?),
                _ => None,
            };
            let mut params = n.params.clone();
            if n.op == "time" {
                params.insert("_image".into(), Param::F(frame));
                params.insert("_secondes".into(), Param::F(frame / fps.max(1e-3)));
            }
            let image = if n.op == "exr" {
                let rel = pstr(&n.params, "fichier");
                if rel.is_empty() {
                    None
                } else {
                    let p = crate::vault::safe(root, rel).map_err(|e| (id, e))?;
                    Some(std::sync::Arc::new(load_exr_pass(&p, pstr(&n.params, "calque"), max_img).map_err(|e| (id, e))?))
                }
            } else if n.op == "image" || n.op == "texture" || n.op == "video" {
                let rel = pstr(&n.params, "fichier");
                if rel.is_empty() {
                    None
                } else {
                    let p = crate::vault::safe(root, rel).map_err(|e| (id, e))?;
                    // Une vidéo ou un montage : l'image au temps choisi, gardée en cache
                    let p = if is_video_source(rel) { video_frame(root, rel, &p, pnum(&n.params, "temps (s)", 0.0)).map_err(|e| (id, e))? } else { p };
                    Some(std::sync::Arc::new(load_img(&p, max_img).ok_or((id, format!("image illisible : {rel}")))?))
                }
            } else {
                None
            };
            base.insert(id, regs);
            let ramp = d.params.iter().find_map(|p| if let ParamDef::Ramp(k) = p { Some(std::sync::Arc::new(parse_ramp(pstr(&n.params, k)))) } else { None });
            let curves = d.params.iter().find_map(|p| {
                if let ParamDef::Curve(k) = p {
                    let c = parse_curves(pstr(&n.params, k));
                    Some(std::sync::Arc::new([0, 1, 2, 3].map(|ch| (0..256).map(|i| eval_curve(&c[ch], i as f32 / 255.0)).collect::<Vec<f32>>())))
                } else {
                    None
                }
            });
            // Empreinte du node : type, réglages, fichier lu (nom, date, taille), et ce qui entre dans chaque prise
            let key = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                d.op.hash(&mut h);
                for (k, v) in &params {
                    k.hash(&mut h);
                    match v {
                        Param::F(x) => x.to_bits().hash(&mut h),
                        Param::S(t) => t.hash(&mut h),
                    }
                }
                if image.is_some() {
                    max_img.hash(&mut h);
                    if let Ok(md) = crate::vault::safe(root, pstr(&n.params, "fichier")).and_then(|p| std::fs::metadata(p).map_err(|e| e.to_string())) {
                        md.len().hash(&mut h);
                        md.modified().ok().hash(&mut h);
                    }
                }
                for i in 0..d.inputs.len() {
                    match into.get(&id).and_then(|v| v.iter().find(|l| l.inp == i)) {
                        Some(l) => (1u8, keys.get(&l.from).copied().unwrap_or(0), l.out).hash(&mut h),
                        None => {
                            let v = n.values.get(i).copied().unwrap_or(d.inputs[i].2);
                            (0u8, v.map(|x| x.to_bits())).hash(&mut h);
                        }
                    }
                }
                h.finish().max(1)
            };
            keys.insert(id, key);
            steps.push(Step { op: d.op, ins, base: regs, nout: d.outputs.len(), params, shader, image, ramp, curves, key });
            regs += d.outputs.len().max(4);
        }
        let uses_curvature = steps.iter().any(|s| matches!(s.op, "curvature" | "edgewear" | "cavdirt"));
        let uses_bump = steps.iter().any(|s| s.op == "mat_out" && matches!(s.ins.get(11), Some(Src::Reg(_))));
        Ok(Program { steps, regs, kind: g.kind, uses_curvature, uses_bump })
    }

    /// Valeur d'un champ en un point. `regs` commence par les valeurs des attributs, dans l'ordre donné à `compile_field`.
    pub fn field(&self, c: &Ctx, regs: &mut Vec<V>) -> V {
        if regs.len() < self.regs {
            regs.resize(self.regs, [0.0; 4]);
        }
        let mut ins: Vec<V> = Vec::with_capacity(8);
        for s in &self.steps {
            ins.clear();
            ins.extend(s.ins.iter().map(|i| match i {
                Src::Reg(r) => regs[*r],
                Src::Const(v) => *v,
            }));
            match s.op {
                "field_out" => return ins[0],
                "attr_read" => {
                    let v = ins[0];
                    regs[s.base] = v;
                    regs[s.base + 1] = v;
                }
                _ => {
                    let o = pixel(s, &ins, c);
                    for (k, v) in o.iter().enumerate().take(s.nout.min(4)) {
                        regs[s.base + k] = *v;
                    }
                }
            }
        }
        [0.0; 4]
    }

    /// Vrai si ce champ donne la même valeur partout (aucun node, ou seulement des constantes).
    pub fn is_const(&self) -> Option<V> {
        match self.steps.as_slice() {
            [s] if s.op == "field_out" => match s.ins[0] {
                Src::Const(v) => Some(v),
                Src::Reg(_) => None,
            },
            _ => None,
        }
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
                let ek = x(9, 1.0).max(0.0);
                return Surf { ao: x(10, 1.0).clamp(0.0, 1.0), bump: x(11, 0.5), bump_k: x(12, 1.0), sheen: x(13, 0.0).clamp(0.0, 1.0), sss: x(14, 0.0).clamp(0.0, 1.0), sss_color: ins.get(15).copied().unwrap_or([0.9, 0.3, 0.2, 1.0]), color: ins[0], rough: ins[1][0].clamp(0.02, 1.0), metal: ins[2][0].clamp(0.0, 1.0), emit: [ins[3][0] * ek, ins[3][1] * ek, ins[3][2] * ek, ins[3][3]], transmission: x(4, 0.0).clamp(0.0, 1.0), ior: x(5, 1.45).clamp(1.0, 3.0), coat: x(6, 0.0).clamp(0.0, 1.0), toon: x(7, 0.0).clamp(0.0, 16.0), spec: x(8, 1.0).clamp(0.0, 2.0) };
            }
            let o = pixel(s, &ins, c);
            for (k, v) in o.iter().enumerate().take(s.nout.min(4)) {
                regs[s.base + k] = *v;
            }
        }
        Surf { color: [0.8, 0.5, 0.4, 1.0], rough: 0.5, metal: 0.0, emit: [0.0; 4], transmission: 0.0, ior: 1.45, coat: 0.0, toon: 0.0, spec: 1.0, ao: 1.0, bump: 0.5, bump_k: 0.0, sheen: 0.0, sss: 0.0, sss_color: [0.9, 0.3, 0.2, 1.0] }
    }

    /// Compositing : calcule l'image de sortie à la taille donnée (ou celle de la première image).
    pub fn composite(&self, size: Option<[usize; 2]>) -> Img {
        self.composite_over(size, None)
    }

    /// Table de couleurs 3D (format .cube) de ce graphe : chaque couleur d'entrée y trouve sa couleur de sortie.
    /// Sert à appliquer un étalonnage en nodes à une vidéo. Seuls les réglages de couleur y entrent :
    /// un flou ou une vignette dépendent de la place du pixel, pas de sa couleur.
    pub fn lut_cube(&self, n: usize) -> String {
        let n = n.clamp(2, 65);
        // Image d'identité : rouge en x, vert puis bleu en y
        let mut id = Img { w: n, h: n * n, px: Vec::with_capacity(n * n * n) };
        let f = |k: usize| k as f32 / (n - 1) as f32;
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    id.px.push([f(r), f(g), f(b), 1.0]);
                }
            }
        }
        let out = self.composite_over(Some([n, n * n]), Some(&id));
        let mut t = format!("TITLE \"Aryadne\"\nLUT_3D_SIZE {n}\n");
        for p in &out.px {
            let c = |x: f32| if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.0 };
            t += &format!("{:.5} {:.5} {:.5}\n", c(p[0]), c(p[1]), c(p[2]));
        }
        t
    }

    /// Comme `composite`, mais les nodes « Image » lisent `over` au lieu de leur fichier.
    pub fn composite_over(&self, size: Option<[usize; 2]>, over: Option<&Img>) -> Img {
        let [w, h] = size.unwrap_or_else(|| self.steps.iter().find_map(|s| s.image.as_ref().map(|i| [i.w, i.h])).unwrap_or([512, 512]));
        let n = w * h;
        let mut bufs: Vec<Option<std::sync::Arc<Vec<V>>>> = vec![None; self.regs];
        use rayon::prelude::*;
        COMP_COMPUTED.with(|c| c.set(0));
        // Une image imposée de l'extérieur (étalonnage d'un clip, table de couleurs) ne passe pas par le cache
        let cached = over.is_none();
        for s in &self.steps {
            let get = |src: &Src| -> Vec<V> {
                match src {
                    Src::Reg(r) => bufs[*r].as_ref().map(|b| b.as_ref().clone()).unwrap_or_else(|| vec![[0.0; 4]; n]),
                    Src::Const(v) => vec![*v; n],
                }
            };
            if s.op == "comp_out" {
                let px = get(&s.ins[0]);
                return Img { w, h, px };
            }
            // Déjà calculé avec les mêmes réglages et les mêmes entrées : on reprend l'image
            if cached && s.key != 0 {
                if let Some(outs) = comp_cache_get((s.key, w, h)) {
                    for (k, o) in outs.into_iter().enumerate() {
                        if s.base + k < bufs.len() {
                            bufs[s.base + k] = Some(o);
                        }
                    }
                    continue;
                }
            }
            COMP_COMPUTED.with(|c| c.set(c.get() + 1));
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
                "zdefocus" => {
                    let src = get(&s.ins[0]);
                    let depth = get(&s.ins[1]);
                    let img = Img { w, h, px: src };
                    let (focus, sharp, spread, rmax) = (pnum(&s.params, "mise au point", 5.0), pnum(&s.params, "étendue nette", 1.0), pnum(&s.params, "étalement", 4.0).max(1e-3), pnum(&s.params, "rayon max (px)", 14.0));
                    // Disque d'échantillons (spirale de Vogel), le rayon dépend de la distance à la mise au point
                    const TAPS: usize = 32;
                    let disk: Vec<[f32; 2]> = (0..TAPS).map(|k| {
                        let r = ((k as f32 + 0.5) / TAPS as f32).sqrt();
                        let a = k as f32 * 2.399_963;
                        [r * a.cos(), r * a.sin()]
                    }).collect();
                    let coc = |z: f32| (((z - focus).abs() - sharp).max(0.0) / spread).min(1.0) * rmax;
                    let out: Vec<V> = (0..n).into_par_iter().map(|i| {
                        let r = coc(depth[i][0]);
                        if r < 0.5 {
                            return img.px[i];
                        }
                        let (x, y) = ((i % w) as f32 + 0.5, (i / w) as f32 + 0.5);
                        let mut acc = [0.0f32; 4];
                        let mut wsum = 0.0;
                        for d in &disk {
                            let (sx, sy) = (x + d[0] * r, y + d[1] * r);
                            let (ix, iy) = ((sx as i32).clamp(0, w as i32 - 1) as usize, (sy as i32).clamp(0, h as i32 - 1) as usize);
                            let j = iy * w + ix;
                            // Un voisin net et plus proche ne bave pas sur le flou de derrière
                            let wk = if depth[j][0] < depth[i][0] && coc(depth[j][0]) < r * 0.5 { 0.15 } else { 1.0 };
                            for ch in 0..4 {
                                acc[ch] += img.px[j][ch] * wk;
                            }
                            wsum += wk;
                        }
                        acc.map(|v| v / wsum.max(1e-6))
                    }).collect();
                    vec![out]
                }
                "grainmatch" => {
                    let src = get(&s.ins[0]);
                    let plate = get(&s.ins[1]);
                    // Le grain de la plaque : l'écart type de ce que le léger flou retire
                    let b = blur(&plate, w, h, 1.5);
                    let step = (n / 200_000).max(1);
                    let (mut sum, mut sum2, mut cnt) = (0.0f64, 0.0f64, 0.0f64);
                    for k in (0..n).step_by(step) {
                        let d = (lum(plate[k]) - lum(b[k])) as f64;
                        sum += d;
                        sum2 += d * d;
                        cnt += 1.0;
                    }
                    let mean = sum / cnt.max(1.0);
                    let sd = ((sum2 / cnt.max(1.0) - mean * mean).max(0.0)).sqrt() as f32;
                    let size = pnum(&s.params, "taille du grain", 1.0).max(0.5);
                    let amt = sd * pnum(&s.params, "dosage", 1.0) * 3.46; // écart type d'un bruit uniforme centré : amplitude / √12
                    let out: Vec<V> = (0..n).into_par_iter().map(|i| {
                        let (x, y) = (((i % w) as f32 / size) as i32, ((i / w) as f32 / size) as i32);
                        let g = (hash3(x, y, 11) - 0.5) * amt;
                        let c = src[i];
                        [c[0] + g, c[1] + g, c[2] + g, c[3]]
                    }).collect();
                    vec![out, vec![[sd, sd, sd, 1.0]; n]]
                }
                "transform" | "flip" | "displace" | "emboss" | "glitch" | "sunbeams" => {
                    let src = get(&s.ins[0]);
                    let map = if s.op == "displace" { get(&s.ins[1]) } else { vec![] };
                    let img = Img { w, h, px: src };
                    let (tx, ty, rot, scale) = (pnum(&s.params, "x", 0.0), pnum(&s.params, "y", 0.0), pnum(&s.params, "rotation", 0.0).to_radians(), pnum(&s.params, "échelle", 1.0).max(0.01));
                    let sens = pstr(&s.params, "sens");
                    let force = pnum(&s.params, if s.op == "displace" { "force (px)" } else { "force" }, 1.0);
                    let ang = pnum(&s.params, "angle", 135.0).to_radians();
                    let (blocks, seed) = (pnum(&s.params, "blocs", 24.0).max(1.0), pnum(&s.params, "graine", 1.0) as i32);
                    let (th, len, cx, cy) = (pnum(&s.params, "seuil", 0.7), pnum(&s.params, "longueur", 0.4), pnum(&s.params, "centre x", 0.5), pnum(&s.params, "centre y", 0.3));
                    let aspect = w as f32 / h as f32;
                    let out: Vec<V> = (0..n).into_par_iter().map(|i| {
                        let (u, v) = (((i % w) as f32 + 0.5) / w as f32, ((i / w) as f32 + 0.5) / h as f32);
                        match s.op {
                            "transform" => {
                                let (dx, dy) = ((u - 0.5 - tx) * aspect, v - 0.5 - ty);
                                let (rx, ry) = ((dx * rot.cos() + dy * rot.sin()) / scale, (-dx * rot.sin() + dy * rot.cos()) / scale);
                                let (su, sv) = (rx / aspect + 0.5, ry + 0.5);
                                if (0.0..=1.0).contains(&su) && (0.0..=1.0).contains(&sv) { img.sample(su, sv) } else { [0.0; 4] }
                            }
                            "flip" => img.sample(if sens != "vertical" { 1.0 - u } else { u }, if sens == "vertical" || sens == "les deux" { 1.0 - v } else { v }),
                            "displace" => {
                                let m = map[i];
                                img.sample(u + (m[0] - 0.5) * 2.0 * force / w as f32, v + (m[1] - 0.5) * 2.0 * force / h as f32)
                            }
                            "emboss" => {
                                let (ox, oy) = (ang.cos() / w as f32, ang.sin() / h as f32);
                                let d = (lum(img.sample(u + ox, v + oy)) - lum(img.sample(u - ox, v - oy))) * force;
                                let g = (0.5 + d).clamp(0.0, 1.0);
                                [g, g, g, img.px[i][3]]
                            }
                            "glitch" => {
                                let row = (v * blocks) as i32;
                                let on = hash3(row, seed, 1) < force * 0.6;
                                let shift = if on { (hash3(row, seed, 2) - 0.5) * force * 0.3 } else { 0.0 };
                                let split = force * 0.01 * (1.0 + 4.0 * on as u8 as f32);
                                let c = img.sample(u + shift, v);
                                [img.sample(u + shift + split, v)[0], c[1], img.sample(u + shift - split, v)[2], c[3]]
                            }
                            _ => {
                                // Rayons : on remonte vers le centre en accumulant ce qui dépasse le seuil
                                let taps = 32;
                                let mut acc = [0.0f32; 3];
                                for t in 0..taps {
                                    let f = t as f32 / taps as f32 * len;
                                    let c = img.sample(u + (cx - u) * f, v + (cy - v) * f);
                                    let k = ((lum(c) - th) / (1.0 - th).max(0.01)).clamp(0.0, 1.0) * (1.0 - t as f32 / taps as f32);
                                    for ch in 0..3 {
                                        acc[ch] += c[ch] * k;
                                    }
                                }
                                let c = img.px[i];
                                let k = force * 2.0 / taps as f32;
                                [c[0] + acc[0] * k, c[1] + acc[1] * k, c[2] + acc[2] * k, c[3]]
                            }
                        }
                    }).collect();
                    vec![out]
                }
                "maskblur" => {
                    let src = get(&s.ins[0]);
                    let m = get(&s.ins[1]);
                    let b = blur(&src, w, h, pnum(&s.params, "rayon", 12.0) * w as f32 / 1000.0);
                    vec![src.iter().zip(&b).zip(&m).map(|((a, b), m)| {
                        let k = m[0].clamp(0.0, 1.0);
                        [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k, a[3]]
                    }).collect()]
                }
                "image" | "exr" => match over.or(s.image.as_deref()) {
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
            let outs: Vec<std::sync::Arc<Vec<V>>> = outs.into_iter().map(std::sync::Arc::new).collect();
            if cached && s.key != 0 {
                comp_cache_put((s.key, w, h), &outs);
            }
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
    // Une entrée « coordonnées » branchée remplace la position : c'est elle que le motif lit
    let linked = |k: usize| matches!(s.ins.get(k), Some(Src::Reg(_)));
    let p3 = match s.op {
        "mapping" | "warp" => if linked(0) { [i[0][0], i[0][1], i[0][2]] } else { p3 },
        "texcoord" | "shape" => p3,
        _ => match def(s.op) {
            Some(d) if matches!(d.cat, "motif" | "usure") && linked(d.inputs.len() - 1) => {
                let v = i[d.inputs.len() - 1];
                [v[0], v[1], v[2]]
            }
            _ => p3,
        },
    };
    match s.op {
        "texcoord" => [[p3[0], p3[1], p3[2], 1.0], [c.uv[0], c.uv[1], 0.0, 1.0], z, z],
        "mapping" => {
            let sc = i[1][0];
            let q = [(p3[0] - pnum(&s.params, "x", 0.0)) * sc / pnum(&s.params, "étirer X", 1.0).max(0.01), (p3[1] - pnum(&s.params, "y", 0.0)) * sc / pnum(&s.params, "étirer Y", 1.0).max(0.01), (p3[2] - pnum(&s.params, "z", 0.0)) * sc / pnum(&s.params, "étirer Z", 1.0).max(0.01)];
            let (sx, cx) = pnum(&s.params, "rotation X", 0.0).to_radians().sin_cos();
            let q = [q[0], q[1] * cx - q[2] * sx, q[1] * sx + q[2] * cx];
            let (sy, cy) = pnum(&s.params, "rotation Y", 0.0).to_radians().sin_cos();
            let q = [q[0] * cy + q[2] * sy, q[1], -q[0] * sy + q[2] * cy];
            let (sz, cz) = pnum(&s.params, "rotation Z", 0.0).to_radians().sin_cos();
            [[q[0] * cz - q[1] * sz, q[0] * sz + q[1] * cz, q[2], 1.0], z, z, z]
        }
        "warp" => {
            let (k, sc, oct) = (i[1][0], i[2][0], (i[3][0] as usize).clamp(1, 8));
            let w = |ox: f32, oy: f32, oz: f32| (fbm(p3[0] * sc + ox, p3[1] * sc + oy, p3[2] * sc + oz, oct, 0.5) - 0.5) * 2.0 * k;
            [[p3[0] + w(0.0, 0.0, 0.0), p3[1] + w(5.2, 1.3, 8.1), p3[2] + w(2.4, 7.9, 3.3), 1.0], z, z, z]
        }
        "ridged" => {
            let (sc, oct, sharp) = (i[0][0], (i[1][0] as usize).clamp(1, 8), i[2][0].max(0.1));
            let (mut sum, mut amp, mut freq, mut norm) = (0.0f32, 1.0f32, 1.0f32, 0.0f32);
            for o in 0..oct {
                let n = noise3(p3[0] * sc * freq + o as f32 * 7.3, p3[1] * sc * freq, p3[2] * sc * freq);
                sum += (1.0 - (2.0 * n - 1.0).abs()).powf(sharp) * amp;
                norm += amp;
                amp *= 0.5;
                freq *= 2.0;
            }
            [one(sum / norm), z, z, z]
        }
        "cracks" => {
            let sc = i[0][0];
            let d = i[2][0];
            let q = [p3[0] * sc, p3[1] * sc, p3[2] * sc];
            let w = [(fbm(q[0], q[1], q[2], 3, 0.5) - 0.5) * d, (fbm(q[0] + 5.2, q[1] + 1.3, q[2], 3, 0.5) - 0.5) * d, (fbm(q[0], q[1] + 9.1, q[2] + 2.7, 3, 0.5) - 0.5) * d];
            let (a, b) = voronoi_f12(q[0] + w[0], q[1] + w[1], q[2] + w[2]);
            let e = b - a;
            let wd = i[1][0].max(1e-4);
            [one(1.0 - smoothstep(wd * 0.5, wd, e)), one(a.min(1.0)), z, z]
        }
        "tiles" => {
            let sc = i[0][0];
            let (u, v) = flat(c, p3);
            let (x, y) = (u * sc, v * sc);
            let (ix, iy) = (x.floor(), y.floor());
            let (fx, fy) = (x - ix, y - iy);
            let edge = fx.min(1.0 - fx).min(fy.min(1.0 - fy));
            let j = i[1][0].max(0.0) * 0.5;
            let joint = 1.0 - smoothstep(j, j + 0.01, edge);
            let bevel = i[2][0].max(1e-4);
            let hgt = smoothstep(j, j + bevel, edge);
            let var = hash3(ix as i32, iy as i32, 11);
            [one(0.5 + (var - 0.5) * i[3][0]), one(joint), one(hgt), z]
        }
        "weave" => {
            let sc = i[0][0];
            let (u, v) = flat(c, p3);
            let (x, y) = (u * sc, v * sc);
            let even = ((x.floor() + y.floor()) as i64) & 1 == 0;
            let (a, b) = (((x - x.floor()) * std::f32::consts::PI).sin(), ((y - y.floor()) * std::f32::consts::PI).sin());
            let h = if even { b * (0.55 + 0.45 * a) } else { a * (0.55 + 0.45 * b) };
            [one(h), one(even as u8 as f32), z, z]
        }
        "shape" => {
            let (dx, dy) = ((c.uv[0] - i[0][0]) / (i[2][0].abs() * 0.5).max(1e-4), (c.uv[1] - i[1][0]) / (i[3][0].abs() * 0.5).max(1e-4));
            let d = match pstr(&s.params, "forme") {
                "rectangle" => dx.abs().max(dy.abs()),
                "losange" => dx.abs() + dy.abs(),
                _ => (dx * dx + dy * dy).sqrt(),
            };
            let soft = i[4][0].max(1e-4) * 2.0;
            [one(1.0 - smoothstep(1.0 - soft, 1.0, d)), z, z, z]
        }
        "edgewear" | "cavdirt" => {
            let sign = if s.op == "edgewear" { 1.0 } else { -1.0 };
            let k = (c.curv * sign * i[0][0] * 3.0).clamp(-1.0, 1.5);
            let sc = i[3][0];
            let n = fbm(p3[0] * sc, p3[1] * sc, p3[2] * sc, 4, 0.55);
            // La courbure décide où, le bruit casse le bord, la quantité déplace le seuil
            let v = k + (n - 0.5) * 0.8 + (i[1][0] - 0.5);
            [one(mask(v, i[2][0])), z, z, z]
        }
        "dust" => {
            let sc = i[2][0];
            let n = fbm(p3[0] * sc, p3[1] * sc, p3[2] * sc, 4, 0.5);
            let v = c.nrm[1] * 0.5 + (n - 0.5) * 0.6 + i[0][0] - 0.25;
            [one(mask(v, i[1][0])), z, z, z]
        }
        "drips" => {
            // Traînées verticales : plus fortes sur les parois, elles s'effacent vers le bas
            let sc = i[0][0];
            let col = (p3[0] * sc + p3[2] * sc * 0.73).floor();
            let fx = p3[0] * sc + p3[2] * sc * 0.73 - col;
            let top = hash3(col as i32, 3, 9) - 0.5;
            let len = i[1][0].max(0.01) * (0.4 + hash3(col as i32, 8, 1));
            let along = ((top - p3[1]) / len).clamp(0.0, 1.0);
            let width = (1.0 - (fx - 0.5).abs() * 2.0).max(0.0).powf(1.5);
            let on = (hash3(col as i32, 5, 2) < i[2][0]) as u8 as f32;
            let wall = 1.0 - c.nrm[1].abs();
            let v = if p3[1] < top { (1.0 - along) * width * on * wall } else { 0.0 };
            [one(v.clamp(0.0, 1.0)), z, z, z]
        }
        "scratches" => {
            let (u, v) = flat(c, p3);
            let (sc, amount, fine) = (i[0][0], i[1][0], i[2][0].clamp(0.0, 1.0));
            let mut m = 0.0f32;
            for l in 0..5 {
                let ang = hash3(l, 4, 7) * std::f32::consts::PI;
                let (ca, sa) = (ang.cos(), ang.sin());
                let (x, y) = ((u * ca - v * sa) * sc * 0.25 + l as f32 * 3.1, (u * sa + v * ca) * sc * 6.0);
                let (ix, iy) = (x.floor() as i32, y.floor() as i32);
                if hash3(ix, iy, l) < amount * 0.6 {
                    let fy = y - y.floor();
                    let center = 0.2 + 0.6 * hash3(ix, iy, l + 20);
                    let wd = 0.03 + 0.12 * (1.0 - fine);
                    let fxx = x - x.floor();
                    let ends = smoothstep(0.0, 0.15, fxx) * (1.0 - smoothstep(0.85, 1.0, fxx));
                    m = m.max((1.0 - ((fy - center).abs() / wd)).clamp(0.0, 1.0) * ends * (0.4 + 0.6 * hash3(ix, iy, l + 40)));
                }
            }
            [one(m), z, z, z]
        }
        "grunge" => {
            let sc = i[0][0];
            let q = [p3[0] * sc, p3[1] * sc, p3[2] * sc];
            let d = i[2][0];
            let w = [fbm(q[0], q[1], q[2], 4, 0.5) * d, fbm(q[0] + 5.2, q[1] + 1.3, q[2] + 8.1, 4, 0.5) * d, fbm(q[0] + 2.4, q[1] + 7.9, q[2], 4, 0.5) * d];
            let a = fbm(q[0] + w[0] * 2.0, q[1] + w[1] * 2.0, q[2] + w[2] * 2.0, 5, 0.6);
            let b = fbm(q[0] * 3.7 + 11.0, q[1] * 3.7, q[2] * 3.7, 3, 0.5);
            [one(mask(a * 0.75 + b * 0.25, i[1][0])), z, z, z]
        }
        "gamma" => {
            let g = 1.0 / i[1][0].max(0.01);
            [[i[0][0].max(0.0).powf(g), i[0][1].max(0.0).powf(g), i[0][2].max(0.0).powf(g), i[0][3]], z, z, z]
        }
        "tint" => {
            let l = lum(i[0]).clamp(0.0, 1.0);
            let k = i[3][0];
            let f = |ch: usize| i[0][ch] + ((i[1][ch] + (i[2][ch] - i[1][ch]) * l) - i[0][ch]) * k;
            [[f(0), f(1), f(2), i[0][3]], z, z, z]
        }
        "vibrance" => {
            let h = rgb2hsv(i[0]);
            let o = hsv2rgb(h[0], (h[1] * (1.0 + i[1][0] * (1.0 - h[1]) * 2.0)).clamp(0.0, 1.0), h[2]);
            [[o[0], o[1], o[2], i[0][3]], z, z, z]
        }
        "colorbalance" => {
            let f = |ch: usize| {
                let x = i[0][ch].max(0.0);
                let lifted = x + (i[1][ch] * 2.0 - 1.0) * 0.5 * (1.0 - x);
                (lifted * i[3][ch] * 2.0).max(0.0).powf(1.0 / (i[2][ch] * 2.0).max(0.05))
            };
            [[f(0), f(1), f(2), i[0][3]], z, z, z]
        }
        "splittone" => {
            let l = lum(i[0]).clamp(0.0, 1.0);
            let k = i[3][0];
            let f = |ch: usize| {
                let sh = i[0][ch] * (1.0 + (i[1][ch] - lum(i[1])) * 2.0);
                let hi = i[0][ch] * (1.0 + (i[2][ch] - lum(i[2])) * 2.0);
                i[0][ch] + ((sh + (hi - sh) * l) - i[0][ch]) * k
            };
            [[f(0), f(1), f(2), i[0][3]], z, z, z]
        }
        "tonemap" => {
            let e = 2f32.powf(i[1][0]);
            let curve = pstr(&s.params, "courbe");
            let f = |x: f32| {
                let x = (x * e).max(0.0);
                match curve {
                    "Reinhard" => x / (1.0 + x),
                    "filmique" => {
                        let h = |x: f32| ((x * (0.15 * x + 0.05) + 0.004) / (x * (0.15 * x + 0.5) + 0.06)) - 0.0667;
                        h(x * 2.0) / h(11.2)
                    }
                    _ => ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0),
                }
            };
            [[f(i[0][0]), f(i[0][1]), f(i[0][2]), i[0][3]], z, z, z]
        }
        "lumakey" => {
            let l = 0.2126 * i[0][0] + 0.7152 * i[0][1] + 0.0722 * i[0][2];
            let (lo, hi) = (i[1][0].min(i[2][0]), i[1][0].max(i[2][0]).max(i[1][0].min(i[2][0]) + 1e-4));
            let mut k = smoothstep(lo, hi, l);
            if pstr(&s.params, "garder") == "sombre" {
                k = 1.0 - k;
            }
            [[i[0][0], i[0][1], i[0][2], i[0][3] * k], one(k), z, z]
        }
        "diffkey" => {
            let d = ((i[0][0] - i[1][0]).powi(2) + (i[0][1] - i[1][1]).powi(2) + (i[0][2] - i[1][2]).powi(2)).sqrt();
            let k = smoothstep(i[2][0], i[2][0] + i[3][0].max(1e-4), d);
            [[i[0][0], i[0][1], i[0][2], i[0][3] * k], one(k), z, z]
        }
        "despill" => {
            // Le canal en trop ne dépasse plus la moyenne des deux autres
            let mut o = i[0];
            let (k, a, b) = if pstr(&s.params, "couleur") == "bleu" { (2, 0, 1) } else { (1, 0, 2) };
            let limit = (o[a] + o[b]) * 0.5;
            if o[k] > limit {
                o[k] -= (o[k] - limit) * i[1][0].clamp(0.0, 1.0);
            }
            [o, z, z, z]
        }
        "roto" => {
            let pts = parse_points(pstr(&s.params, "points"));
            let inside = point_in_poly(&pts, c.uv);
            let soft = i[0][0].max(0.0);
            let d = if soft > 0.0 { poly_distance(&pts, c.uv) } else { 0.0 };
            let mut k = if pts.len() < 3 {
                0.0
            } else if soft > 0.0 {
                // Bord adouci de part et d'autre du tracé
                let sd = if inside { d } else { -d };
                smoothstep(-soft, soft, sd)
            } else if inside {
                1.0
            } else {
                0.0
            };
            if pstr(&s.params, "mode") == "extérieur" {
                k = 1.0 - k;
            }
            [one(k), z, z, z]
        }
        "chromakey" => {
            // Distance dans le plan des couleurs (sans la luminosité), puis retrait du reflet vert
            let cbcr = |c: V| (-0.169 * c[0] - 0.331 * c[1] + 0.5 * c[2], 0.5 * c[0] - 0.419 * c[1] - 0.081 * c[2]);
            let ((a1, b1), (a2, b2)) = (cbcr(i[0]), cbcr(i[1]));
            let d = ((a1 - a2).powi(2) + (b1 - b2).powi(2)).sqrt();
            let keep = smoothstep(i[2][0], i[2][0] + i[3][0].max(1e-4), d);
            let mut o = i[0];
            let kmax = if i[1][1] >= i[1][0] && i[1][1] >= i[1][2] { 1 } else if i[1][2] >= i[1][0] { 2 } else { 0 };
            let others = (0..3).filter(|&k| k != kmax).map(|k| o[k]).fold(0.0f32, f32::max);
            o[kmax] = o[kmax].min(others + (o[kmax] - others) * keep);
            o[3] = i[0][3] * keep;
            [o, one(keep), z, z]
        }
        "alphaover" => {
            let a = (i[1][3] * i[2][0]).clamp(0.0, 1.0);
            [[i[0][0] + (i[1][0] - i[0][0]) * a, i[0][1] + (i[1][1] - i[0][1]) * a, i[0][2] + (i[1][2] - i[0][2]) * a, (i[0][3] + a * (1.0 - i[0][3])).min(1.0)], z, z, z]
        }
        "setalpha" => [[i[0][0], i[0][1], i[0][2], i[1][0].clamp(0.0, 1.0)], z, z, z],
        "mixf" => [one(i[1][0] + (i[2][0] - i[1][0]) * i[0][0]), z, z, z],
        "halftone" => {
            let size = i[1][0].max(2.0);
            let ang = i[2][0].to_radians();
            let (x, y) = (c.uv[0] * c.res[0], c.uv[1] * c.res[1]);
            let (rx, ry) = ((x * ang.cos() - y * ang.sin()) / size, (x * ang.sin() + y * ang.cos()) / size);
            let (fx, fy) = (rx - rx.floor() - 0.5, ry - ry.floor() - 0.5);
            let r = (1.0 - lum(i[0]).clamp(0.0, 1.0)).sqrt() * 0.71;
            let v = smoothstep(r - 0.06, r + 0.06, (fx * fx + fy * fy).sqrt());
            [[v, v, v, i[0][3]], z, z, z]
        }
        "scanlines" => {
            let k = 1.0 - i[1][0] * (0.5 + 0.5 * (c.uv[1] * i[2][0] * std::f32::consts::TAU).cos());
            [[i[0][0] * k, i[0][1] * k, i[0][2] * k, i[0][3]], z, z, z]
        }
        "letterbox" => {
            let aspect = c.res[0] / c.res[1].max(1.0);
            let band = ((1.0 - aspect / i[1][0].max(0.1)) * 0.5).max(0.0);
            let k = (c.uv[1] >= band && c.uv[1] <= 1.0 - band) as u8 as f32;
            [[i[0][0] * k, i[0][1] * k, i[0][2] * k, i[0][3]], z, z, z]
        }
        "color" | "value" => [i[0], z, z, z],
        "param" => [i[0], one(i[0][0]), z, z],
        "coords" => [one(c.uv[0]), one(c.uv[1]), [c.uv[0], c.uv[1], 0.0, 1.0], z],
        "geometry" => {
            let n = c.nrm;
            [[n[0] * 0.5 + 0.5, n[1] * 0.5 + 0.5, n[2] * 0.5 + 0.5, 1.0], [c.pos[0] + 0.5, c.pos[1] + 0.5, c.pos[2] + 0.5, 1.0], one(c.pos[1] + 0.5), one(1.0 - n[1].abs())]
        }
        "paintmask" => {
            let k = pstr(&s.params, "canal").parse::<usize>().unwrap_or(1).clamp(1, 4) - 1;
            [one(c.masks[k].clamp(0.0, 1.0)), z, z, z]
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
        "texture" | "video" => match &s.image {
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
            // Avec des coordonnées branchées, le dégradé les suit ; sinon il suit l'image (ou la hauteur du modèle)
            let (gu, gv, gh) = if linked(0) { (p3[0], p3[1], p3[1]) } else { (c.uv[0], c.uv[1], c.pos[1] + 0.5) };
            let v = match pstr(&s.params, "type") {
                "vertical" => gv,
                "radial" => 1.0 - ((gu - 0.5).powi(2) + (gv - 0.5).powi(2)).sqrt() * 2.0,
                "hauteur (3D)" => gh,
                _ => gu,
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
        "curves" => {
            let Some(t) = &s.curves else { return [i[0], z, z, z] };
            // Lecture dans la table, avec interpolation entre deux cases
            let look = |tab: &Vec<f32>, x: f32| {
                let f = if x.is_finite() { x.clamp(0.0, 1.0) * 255.0 } else { 0.0 };
                let k = (f as usize).min(254);
                tab[k] + (tab[k + 1] - tab[k]) * (f - k as f32)
            };
            let k = i[1][0].clamp(0.0, 1.0);
            let f = |ch: usize| i[0][ch] + (look(&t[0], look(&t[ch + 1], i[0][ch])) - i[0][ch]) * k;
            [[f(0), f(1), f(2), i[0][3]], z, z, z]
        }
        "colorramp" => {
            let mode = match pstr(&s.params, "interpolation") {
                "lisse" => 1,
                "constante" => 2,
                _ => 0,
            };
            let col = s.ramp.as_ref().map(|r| eval_ramp(r, i[0][0], mode)).unwrap_or([0.5, 0.5, 0.5, 1.0]);
            [col, one(col[3]), z, z]
        }
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
        "unmult" => {
            // Le noir devient transparent : l'alpha vient de la composante la plus forte
            let m = i[0][0].max(i[0][1]).max(i[0][2]).clamp(0.0, 1.0);
            if m <= 1e-5 {
                [[0.0, 0.0, 0.0, 0.0], z, z, z]
            } else {
                [[i[0][0] / m, i[0][1] / m, i[0][2] / m, i[0][3] * m], z, z, z]
            }
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
                "tangente" => a.tan(),
                "arc sinus" => a.clamp(-1.0, 1.0).asin(),
                "arc cosinus" => a.clamp(-1.0, 1.0).acos(),
                "arc tangente" => a.atan(),
                "arc tangente (A / B)" => a.atan2(b),
                "exponentielle" => a.exp(),
                "logarithme" => a.max(1e-12).ln(),
                "signe" => {
                    if a > 0.0 {
                        1.0
                    } else if a < 0.0 {
                        -1.0
                    } else {
                        0.0
                    }
                }
                "arrondi" => a.round(),
                "arrondi sup." => a.ceil(),
                "tronquer" => a.trunc(),
                "aimanter (pas B)" => {
                    if b == 0.0 {
                        a
                    } else {
                        (a / b).floor() * b
                    }
                }
                "boucler (0 à B)" => a.rem_euclid(if b == 0.0 { 1.0 } else { b }),
                "ping-pong (0 à B)" => {
                    if b == 0.0 {
                        0.0
                    } else {
                        (a.rem_euclid(2.0 * b) - b).abs() * -1.0 + b
                    }
                }
                "inverse (1 / A)" => {
                    if a == 0.0 {
                        0.0
                    } else {
                        1.0 / a
                    }
                }
                "opposé" => -a,
                "en radians" => a.to_radians(),
                "en degrés" => a.to_degrees(),
                // Minimum et maximum adoucis sur une largeur de 0,1 : pas d'arête là où les deux se croisent
                "minimum doux" => {
                    let h = (0.1 - (a - b).abs()).max(0.0) / 0.1;
                    a.min(b) - h * h * h * 0.1 / 6.0
                }
                "maximum doux" => {
                    let h = (0.1 - (a - b).abs()).max(0.0) / 0.1;
                    a.max(b) + h * h * h * 0.1 / 6.0
                }
                _ => a + b,
            };
            [one(v), z, z, z]
        }
        "vmath" => {
            let (a, b, k) = ([i[0][0], i[0][1], i[0][2]], [i[1][0], i[1][1], i[1][2]], i[2][0]);
            let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            let each = |f: &dyn Fn(f32, f32) -> f32| [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2])];
            let len = dot(a, a).sqrt();
            let (v, s): ([f32; 3], f32) = match pstr(&s.params, "opération") {
                "soustraire" => (each(&|x, y| x - y), 0.0),
                "multiplier" => (each(&|x, y| x * y), 0.0),
                "diviser" => (each(&|x, y| if y == 0.0 { 0.0 } else { x / y }), 0.0),
                "échelle (× facteur)" => ([a[0] * k, a[1] * k, a[2] * k], 0.0),
                "produit scalaire" => {
                    let d = dot(a, b);
                    ([d; 3], d)
                }
                "produit vectoriel" => ([a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]], 0.0),
                "longueur" => ([len; 3], len),
                "distance" => {
                    let d = each(&|x, y| x - y);
                    let l = dot(d, d).sqrt();
                    ([l; 3], l)
                }
                "normaliser" => (if len > 1e-12 { [a[0] / len, a[1] / len, a[2] / len] } else { [0.0; 3] }, len),
                "mélange (facteur)" => (each(&|x, y| x + (y - x) * k), 0.0),
                "réfléchir" => {
                    let bl = dot(b, b).sqrt().max(1e-12);
                    let n = [b[0] / bl, b[1] / bl, b[2] / bl];
                    let d = 2.0 * dot(a, n);
                    ([a[0] - d * n[0], a[1] - d * n[1], a[2] - d * n[2]], 0.0)
                }
                "projeter" => {
                    let d = dot(a, b) / dot(b, b).max(1e-12);
                    ([b[0] * d, b[1] * d, b[2] * d], d)
                }
                "minimum" => (each(&|x, y| x.min(y)), 0.0),
                "maximum" => (each(&|x, y| x.max(y)), 0.0),
                "absolu" => ([a[0].abs(), a[1].abs(), a[2].abs()], 0.0),
                "arrondi inf." => ([a[0].floor(), a[1].floor(), a[2].floor()], 0.0),
                "fraction" => ([a[0].rem_euclid(1.0), a[1].rem_euclid(1.0), a[2].rem_euclid(1.0)], 0.0),
                "sinus" => ([a[0].sin(), a[1].sin(), a[2].sin()], 0.0),
                "cosinus" => ([a[0].cos(), a[1].cos(), a[2].cos()], 0.0),
                _ => (each(&|x, y| x + y), 0.0),
            };
            // Sans valeur propre à l'opération, la seconde sortie donne la longueur du résultat
            let s = if s == 0.0 { dot(v, v).sqrt() } else { s };
            [[v[0], v[1], v[2], 1.0], one(s), z, z]
        }
        "sepxyz" => [one(i[0][0]), one(i[0][1]), one(i[0][2]), z],
        "combxyz" => [[i[0][0], i[1][0], i[2][0], 1.0], z, z, z],
        "compare" => {
            let (a, b, eps) = (i[0][0], i[1][0], i[2][0].abs());
            let r = match pstr(&s.params, "comparaison") {
                "différent" => (a - b).abs() > eps,
                "inférieur" => a < b,
                "inférieur ou égal" => a <= b,
                "supérieur" => a > b,
                "supérieur ou égal" => a >= b,
                _ => (a - b).abs() <= eps,
            };
            [one(r as u8 as f32), z, z, z]
        }
        "logic" => {
            let (a, b) = (i[0][0] > 0.5, i[1][0] > 0.5);
            let r = match pstr(&s.params, "opération") {
                "ou" => a || b,
                "non" => !a,
                "ou exclusif" => a != b,
                "non-et" => !(a && b),
                "non-ou" => !(a || b),
                _ => a && b,
            };
            [one(r as u8 as f32), z, z, z]
        }
        "switch" => [if i[0][0] > 0.5 { i[2] } else { i[1] }, z, z, z],
        "random" => {
            let q = if linked(3) { [i[3][0], i[3][1], i[3][2]] } else { p3 };
            let seed = i[2][0];
            let h = |k: f32| hash2(q[0] * 91.7 + q[2] * 13.1 + seed * 7.3 + k, q[1] * 57.3 - q[2] * 31.9 + seed * 3.1 + k * 1.7);
            let (lo, hi) = (i[0][0], i[1][0]);
            let m = |x: f32| lo + (hi - lo) * x;
            [one(m(h(0.0))), [m(h(0.0)), m(h(17.0)), m(h(41.0)), 1.0], z, z]
        }
        "time" => [one(pnum(&s.params, "_image", 0.0)), one(pnum(&s.params, "_secondes", 0.0)), z, z],
        "expr" => match &s.shader {
            Some(sh) => {
                let mut v = vec![0.0f32; sh.slots];
                let pre = [i[0][0], i[1][0], i[2][0], i[3][0], p3[0], p3[1], p3[2], c.uv[0], c.uv[1], std::f32::consts::PI, 0.0];
                v[..pre.len()].copy_from_slice(&pre);
                sh.eval(&mut v);
                [one(v[10]), z, z, z]
            }
            None => [z, z, z, z],
        },
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

thread_local! {
    /// Nodes copiés, partagés entre tous les graphes ouverts
    static NODE_CLIP: std::cell::RefCell<Option<(Vec<Node>, Vec<Link>)>> = const { std::cell::RefCell::new(None) };
    /// Les groupes dont les nodes copiés ont besoin : ils suivent la copie d'un graphe à l'autre
    static GROUP_CLIP: std::cell::RefCell<Vec<crate::nodeflow::Group>> = const { std::cell::RefCell::new(Vec::new()) };
}
const ROW: f32 = 22.0;
const HEAD: f32 = 24.0;
const THUMB: f32 = 96.0;
/// Couleur d'une catégorie de nodes (bande du haut, pastilles de la bibliothèque), d'après la maquette
pub fn cat_color(cat: &str) -> Color32 {
    match cat {
        "sortie" => theme::pal().accent,
        "géométrie" => Color32::from_rgb(0x58, 0xb8, 0xa4),
        "simulation" => Color32::from_rgb(0xa9, 0x8f, 0xd6),
        "répétition" => Color32::from_rgb(0xd9, 0x9a, 0x5b),
        "groupe" => Color32::from_rgb(0x8f, 0xbf, 0x7a),
        "entrée" | "motif" => Color32::from_rgb(0xc6, 0x95, 0x80),
        "usure" => Color32::from_rgb(0xa6, 0x7f, 0x6f),
        "couleur" => Color32::from_rgb(0x94, 0xb0, 0xd1),
        "filtre" => Color32::from_rgb(0x7f, 0xb8, 0xbd),
        "shader" => Color32::from_rgb(0x83, 0xb9, 0x9e),
        _ => Color32::from_rgb(0x94, 0x8e, 0x91),
    }
}

const CATS: [&str; 11] = ["entrée", "géométrie", "simulation", "motif", "usure", "couleur", "math", "filtre", "shader", "structure", "sortie"];

/// Node de sortie d'un graphe : il n'y en a qu'un, on ne le supprime ni ne le copie.
pub fn is_out_op(op: &str) -> bool {
    matches!(op, "mat_out" | "comp_out" | "geo_out")
}

/// Largeur d'un node : un renvoi n'est qu'une pastille.
fn node_w(n: &Node) -> f32 {
    if n.op == "reroute" { REROUTE_W } else { NW }
}
const REROUTE_W: f32 = 30.0;
const REROUTE_H: f32 = 16.0;

/// Ce qu'on choisit dans le menu d'ajout : un node, un node déjà réglé (« Math › multiplier »), un groupe.
#[derive(Clone, PartialEq, Debug)]
pub enum Pick {
    Op(&'static str),
    Preset(&'static str, &'static str, &'static str),
    Group(String),
}

/// Catégorie où ranger ce node dans les menus, ou rien s'il ne s'y montre pas.
fn menu_cat(d: &Def, kind: GraphKind, in_group: bool) -> Option<&'static str> {
    if matches!(d.op, "group_in" | "group_out") {
        return in_group.then_some("structure");
    }
    if d.op == "sim_in" {
        return (kind == GraphKind::Geometry).then_some("simulation");
    }
    (d.cat != "interne" && d.fits(kind)).then_some(d.cat)
}

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
    /// Filtre de la bibliothèque de nodes (panneau de gauche)
    lib_filter: String,
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
    /// Forme d'aperçu : 0 sphère, 1 cube arrondi (choisi d'office pour un matériau à calques)
    pv_shape: Option<u8>,
    boxy: Option<std::sync::Arc<crate::model3d::Prepared>>,
    pv_model: Option<(String, std::sync::Arc<crate::model3d::Prepared>)>,
    pv_key: u64,
    /// Node dont la sortie remplace le matériau dans l'aperçu (« prévisualiser la node »)
    solo: Option<u32>,
    /// Point du tracé roto qu'on déplace dans l'aperçu
    roto_drag: Option<usize>,
    /// Fichier à ouvrir dans un autre onglet (le maître d'un calque de l'atelier)
    pub open_request: Option<PathBuf>,
    /// Miniatures des préréglages de matériaux (bande du bas)
    preset_tex: HashMap<&'static str, egui::TextureHandle>,
    preset_rx: Option<std::sync::mpsc::Receiver<(&'static str, Img)>>,
    strip_open: bool,
    /// Atelier : calque choisi, et les matériaux du coffre (relus de temps en temps)
    layer_sel: Option<usize>,
    mat_list: Option<(std::time::Instant, Vec<String>)>,
    masters_seen: u64,
    /// Atelier : vignettes des matériaux (maîtres, intelligents, préréglages), calculées en arrière-plan
    mthumb: HashMap<String, egui::TextureHandle>,
    mthumb_rx: Option<std::sync::mpsc::Receiver<(String, Img)>>,
    /// Atelier : recherche et rayon de la bibliothèque (0 tout, 1 maîtres, 2 intelligents, 3 préréglages)
    lib_search: String,
    lib_shelf: u8,
    /// Groupe dont on édite l'intérieur (rien : la racine). Tant qu'on y est, `graph.nodes`, `links`
    /// et `frames` sont ceux du groupe, et ceux de la racine attendent à sa place dans `graph.groups`.
    level: Option<String>,
    /// Cadre dont on écrit la note
    note_edit: Option<usize>,
    /// Nom de groupe en cours de saisie dans l'inspecteur : (ancien nom, texte)
    group_rename: Option<(String, String)>,
    group_request: bool,
    leave_request: bool,
    /// Un champ de texte avait-il le clavier à l'image d'avant ? (pour la touche Tab)
    was_typing: bool,
    /// Géométrie et simulation : le moteur, l'aperçu 3D et la lecture
    geo: Option<Box<crate::geoview::GeoView>>,
    /// Image de l'animation montrée par l'aperçu d'un matériau ou d'un compositing (node « Temps »)
    frame: f32,
}

/// Échange le contenu de la racine et celui du groupe `name` : entrer dans un groupe, ou en sortir.
fn swap_level(g: &mut Graph, name: &str) -> bool {
    let Some(k) = g.groups.iter().position(|x| x.name == name) else { return false };
    let gr = &mut g.groups[k];
    std::mem::swap(&mut g.nodes, &mut gr.nodes);
    std::mem::swap(&mut g.links, &mut gr.links);
    std::mem::swap(&mut g.frames, &mut gr.frames);
    true
}

enum Act {
    Pan(egui::Vec2),
    Deselect(u32),
    Move(egui::Vec2),
    MoveEnd,
    Connect(u32, usize, u32, usize),
    Unlink(u32, usize),
    RemoveLink(usize),
    /// Pose un renvoi sur ce lien, à cet endroit
    Reroute(usize, Pos2),
    /// Entre dans le groupe posé par ce node
    Enter(u32),
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
    Group,
    Ungroup,
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
        let program = if graph.kind == GraphKind::Geometry { crate::nodeflow::lower(&graph, false).map(|_| Program::empty(GraphKind::Geometry)) } else { Program::compile(&graph.effective(), root, 1024) };
        let mut ed = Editor {
            lib_filter: String::new(),
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
            open_request: None,
            preset_tex: HashMap::new(),
            preset_rx: None,
            strip_open: true,
            layer_sel: None,
            mthumb: HashMap::new(),
            mthumb_rx: None,
            lib_search: String::new(),
            lib_shelf: 0,
            mat_list: None,
            masters_seen: 0,
            level: None,
            note_edit: None,
            group_rename: None,
            group_request: false,
            leave_request: false,
            was_typing: false,
            geo: None,
            frame: 0.0,
            frame_sel: false,
            frame_rename: None,
            frame_sel_request: false, pv_yaw: 0.4, pv_pitch: 0.25, pv_dist: 2.0, pv_env: 0, pv_shape: None, boxy: None, pv_model: None, pv_key: 0, solo: None, roto_drag: None,
        };
        if ed.graph.kind == GraphKind::Geometry {
            ed.geo = Some(Box::new(crate::geoview::GeoView::new(&ed.graph)));
        }
        // Un graphe dont des nodes se chevauchent (fichier écrit à la main, ancienne version) est rangé à l'ouverture
        if ed.overlaps() {
            ed.arrange();
        }
        Ok(ed)
    }

    /// Le document entier, racine à sa place, même quand on édite l'intérieur d'un groupe.
    fn doc(&self) -> Graph {
        let mut g = self.graph.clone();
        if let Some(l) = &self.level {
            swap_level(&mut g, l);
        }
        g
    }

    /// Agit sur le document entier (racine à sa place), puis revient au niveau en cours d'édition.
    fn with_doc<R>(&mut self, f: impl FnOnce(&mut Graph) -> R) -> R {
        if let Some(l) = self.level.clone() {
            swap_level(&mut self.graph, &l);
        }
        let r = f(&mut self.graph);
        if let Some(l) = self.level.clone() {
            if !swap_level(&mut self.graph, &l) {
                self.level = None;
            }
        }
        r
    }

    fn level_changed(&mut self) {
        self.selected.clear();
        self.thumbs.clear();
        self.thumb_version = 0;
        self.scene = Rect::ZERO;
        self.solo = None;
        self.rename = None;
        self.frame_rename = None;
        self.note_edit = None;
        self.drag_link = None;
    }

    /// Entre dans un groupe pour en modifier le contenu.
    fn enter(&mut self, name: &str) {
        self.leave();
        if swap_level(&mut self.graph, name) {
            self.level = Some(name.to_string());
            self.level_changed();
        }
    }

    /// Revient à la racine du graphe.
    fn leave(&mut self) {
        if let Some(l) = self.level.take() {
            swap_level(&mut self.graph, &l);
            self.level_changed();
        }
    }

    fn compile_doc(&self) -> Result<Program, (u32, String)> {
        if self.graph.kind == GraphKind::Geometry {
            // La géométrie a son propre moteur : ici, seule la mise à plat est vérifiée
            return crate::nodeflow::lower(&self.doc(), false).map(|_| Program::empty(GraphKind::Geometry));
        }
        Program::compile_at(&self.doc().effective(), &self.root, 1024, self.frame, 24.0)
    }

    /// Vrai si le graphe lit le temps (node « Temps », à la racine ou dans un groupe).
    fn uses_time(&self) -> bool {
        self.graph.nodes.iter().chain(self.graph.groups.iter().flat_map(|g| g.nodes.iter())).any(|n| n.op == "time")
    }

    fn changed(&mut self) {
        self.dirty = Some(std::time::Instant::now());
        self.with_doc(crate::nodeflow::sync);
        self.program = self.compile_doc();
        self.version += 1;
        self.commit();
    }

    /// Point d'annulation si le graphe a changé depuis le dernier.
    fn commit(&mut self) {
        let doc = self.doc();
        let a = serde_json::to_string(&doc).unwrap_or_default();
        let b = serde_json::to_string(&self.last).unwrap_or_default();
        if a != b {
            self.undo.push(std::mem::replace(&mut self.last, doc));
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }

    fn restore(&mut self, g: Graph) {
        self.graph = g;
        self.last = self.graph.clone();
        // On reste dans le groupe qu'on éditait, s'il existe encore
        if let Some(l) = self.level.clone() {
            if !swap_level(&mut self.graph, &l) {
                self.level = None;
                self.scene = Rect::ZERO;
            }
        }
        self.selected.retain(|id| self.graph.nodes.iter().any(|n| n.id == *id));
        self.dirty = Some(std::time::Instant::now());
        self.program = self.compile_doc();
        self.version += 1;
    }

    fn undo(&mut self) {
        if let Some(g) = self.undo.pop() {
            self.redo.push(self.doc());
            self.restore(g);
        }
    }

    fn redo(&mut self) {
        if let Some(g) = self.redo.pop() {
            self.undo.push(self.doc());
            self.restore(g);
        }
    }

    pub fn save_if_due(&mut self, force: bool) {
        if let Some(t) = self.dirty {
            if force || t.elapsed() > std::time::Duration::from_millis(500) {
                if let Err(e) = self.doc().save(&self.path) {
                    self.message = Some(e);
                }
                self.dirty = None;
            }
        }
    }

    /// Tout ce que les menus d'ajout proposent ici : (libellé, catégorie, choix, a une entrée).
    fn picks(&self) -> Vec<(String, &'static str, Pick, bool)> {
        let kind = self.graph.kind;
        let in_group = self.level.is_some();
        // Une seule sortie par graphe : elle n'est proposée que si elle manque
        let has_out = in_group || self.graph.nodes.iter().any(|n| is_out_op(&n.op));
        let mut v: Vec<(String, &'static str, Pick, bool)> = DEFS.iter().filter(|d| !(has_out && is_out_op(d.op))).filter_map(|d| menu_cat(d, kind, in_group).map(|c| (crate::i18n::t(d.label), c, Pick::Op(d.op), !d.inputs.is_empty() || crate::nodeflow::is_dynamic(d.op)))).collect();
        for gr in self.graph.groups.iter().filter(|gr| Some(&gr.name) != self.level.as_ref()) {
            v.push((gr.name.clone(), "structure", Pick::Group(gr.name.clone()), !gr.inputs.is_empty()));
        }
        v
    }

    /// Pose ce qu'on a choisi dans un menu d'ajout. Renvoie le node à relier (le début, pour une zone).
    fn add_pick(&mut self, pick: &Pick, at: [f32; 2]) -> Option<u32> {
        let id = match pick {
            Pick::Op("repeat_in") | Pick::Op("sim_in") => {
                let (a, b) = crate::nodeflow::add_zone(&mut self.graph, if *pick == Pick::Op("sim_in") { "sim" } else { "repeat" }, at);
                self.selected = [a, b].into_iter().collect();
                return Some(a);
            }
            Pick::Op(op) => self.graph.add(op, at),
            Pick::Preset(op, key, value) => {
                let id = self.graph.add(op, at);
                if let Some(n) = self.graph.nodes.iter_mut().find(|n| n.id == id) {
                    n.params.insert(key.to_string(), Param::S(value.to_string()));
                }
                id
            }
            Pick::Group(name) => crate::nodeflow::add_group_node(&mut self.graph, name, at)?,
        };
        self.selected = [id].into_iter().collect();
        Some(id)
    }

    /// Réunit la sélection en un groupe (Ctrl+G).
    fn group_selection(&mut self) {
        let base = crate::i18n::t("groupe");
        match crate::nodeflow::make_group(&mut self.graph, &self.selected, &base) {
            Ok(id) => {
                self.selected = [id].into_iter().collect();
                self.changed();
                self.message = Some(crate::i18n::t("Groupe créé. Tab pour y entrer, Ctrl+Alt+G pour le défaire."));
            }
            Err(e) => self.message = Some(crate::i18n::t(&e)),
        }
    }

    /// Remet à plat les nodes de groupe choisis (Ctrl+Alt+G).
    fn ungroup_selection(&mut self) {
        let ids: Vec<u32> = self.selected.iter().copied().filter(|id| self.graph.nodes.iter().any(|n| n.id == *id && n.op == "group")).collect();
        let mut new = vec![];
        for id in ids {
            match crate::nodeflow::ungroup(&mut self.graph, id) {
                Ok(v) => new.extend(v),
                Err(e) => self.message = Some(e),
            }
        }
        if !new.is_empty() {
            self.selected = new.into_iter().collect();
            self.changed();
        }
    }

    /// Supprime un node. Une zone part en entier ; un renvoi supprimé laisse ses liens se rejoindre.
    fn delete_node(&mut self, id: u32) {
        if self.is_output(id) {
            return;
        }
        let pair = crate::nodeflow::zone_pair(&self.graph, id);
        if self.graph.nodes.iter().any(|n| n.id == id && n.op == "reroute") {
            if let Some((from, out)) = self.graph.links.iter().find(|l| l.to == id).map(|l| (l.from, l.out)) {
                let users: Vec<(u32, usize)> = self.graph.links.iter().filter(|l| l.from == id).map(|l| (l.to, l.inp)).collect();
                self.graph.remove(id);
                for (to, inp) in users {
                    self.graph.links.push(Link { from, out, to, inp });
                }
            }
        }
        self.graph.remove(id);
        if let Some(o) = pair {
            self.graph.remove(o);
            self.selected.remove(&o);
        }
    }

    /// Le groupe posé par ce node, s'il en pose un.
    fn group_of(&self, id: u32) -> Option<String> {
        self.graph.nodes.iter().find(|n| n.id == id && n.op == "group").map(|n| pstr(&n.params, "groupe").to_string())
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
                    let h = n.def().map(|d| Self::node_size(n, d)).unwrap_or(120.0);
                    n.pos = [x0 + c as f32 * 240.0, y];
                    y += h + 28.0;
                }
            }
        }
        self.changed();
    }

    /// Deux nodes se recouvrent-ils ?
    fn overlaps(&self) -> bool {
        let rects: Vec<Rect> = self.graph.nodes.iter().filter_map(|n| n.def().map(|d| Rect::from_min_size(pos2(n.pos[0], n.pos[1]), vec2(node_w(n), Self::node_size(n, d))).shrink(4.0))).collect();
        rects.iter().enumerate().any(|(i, a)| rects[i + 1..].iter().any(|b| a.intersects(*b)))
    }

    fn is_output(&self, id: u32) -> bool {
        self.graph.nodes.iter().find(|n| n.id == id).is_some_and(|n| is_out_op(&n.op))
    }

    /// Copie les nodes choisis (avec les liens entre eux) décalés de `offset`.
    fn duplicate(&mut self, ids: &[u32], offset: [f32; 2]) -> Vec<u32> {
        // Une zone se copie en entier : l'autre bout suit
        let mut ids: Vec<u32> = ids.to_vec();
        for id in ids.clone() {
            if let Some(o) = crate::nodeflow::zone_pair(&self.graph, id) {
                if !ids.contains(&o) {
                    ids.push(o);
                }
            }
        }
        let nodes: Vec<Node> = self.graph.nodes.iter().filter(|n| ids.contains(&n.id) && !is_out_op(&n.op)).cloned().collect();
        let links: Vec<Link> = self.graph.links.iter().filter(|l| ids.contains(&l.from) && ids.contains(&l.to)).cloned().collect();
        self.paste(nodes, links, offset)
    }

    fn paste(&mut self, nodes: Vec<Node>, links: Vec<Link>, offset: [f32; 2]) -> Vec<u32> {
        let mut map = HashMap::new();
        // Les zones collées gardent leurs paires, sans se mêler à celles déjà là
        let zone_shift = self.graph.nodes.iter().filter_map(|n| match n.params.get("zone") { Some(Param::F(z)) => Some(*z), _ => None }).fold(0.0f32, f32::max);
        let zone_of = |n: &Node| match n.params.get("zone") { Some(Param::F(z)) => Some(*z), _ => None };
        for n in &nodes {
            if n.def().is_none() || is_out_op(&n.op) {
                continue;
            }
            // Une moitié de zone, une entrée de groupe hors d'un groupe, un groupe absent de ce document : on passe
            let whole_zone = !crate::nodeflow::is_zone(&n.op) || nodes.iter().any(|m| m.id != n.id && crate::nodeflow::is_zone(&m.op) && zone_of(m) == zone_of(n));
            let misplaced = matches!(n.op.as_str(), "group_in" | "group_out") && self.level.is_none();
            let lost_group = n.op == "group" && (!self.graph.groups.iter().any(|g| g.name == pstr(&n.params, "groupe")) || self.level.as_deref() == Some(pstr(&n.params, "groupe")));
            if !whole_zone || misplaced || lost_group {
                if lost_group {
                    self.message = Some(format!("{} : {}", crate::i18n::t("ce groupe n'existe pas dans ce graphe"), pstr(&n.params, "groupe")));
                }
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
                m.ins = n.ins.clone();
                m.outs = n.outs.clone();
                if let Some(z) = zone_of(n) {
                    m.params.insert("zone".into(), Param::F(z + zone_shift));
                }
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
        if n.op == "reroute" {
            return REROUTE_H;
        }
        if n.collapsed {
            return HEAD;
        }
        let rows = d.outputs.len() + d.inputs.len() + d.params.iter().map(|pp| match pp { ParamDef::Text(_) => 9, ParamDef::Ramp(_) => 3, ParamDef::Curve(_) => 7, _ => 1 }).sum::<usize>();
        let thumb = if n.preview && !d.outputs.is_empty() { THUMB + 8.0 } else { 0.0 };
        HEAD + rows as f32 * ROW + 8.0 + thumb
    }

    fn bbox(&self, only: Option<&std::collections::BTreeSet<u32>>) -> Rect {
        let mut bb = Rect::NOTHING;
        for n in &self.graph.nodes {
            if only.is_some_and(|s| !s.contains(&n.id)) {
                continue;
            }
            let h = n.def().map(|d| Self::node_size(n, d)).unwrap_or(100.0);
            bb = bb.union(Rect::from_min_size(pos2(n.pos[0], n.pos[1]), vec2(node_w(n), h)));
        }
        bb
    }

    /// La bande des préréglages : une sphère par matériau, calculée une fois en arrière-plan ; un clic remplace le graphe
    fn preset_strip(&mut self, ui: &mut egui::Ui, r: Rect, ctx: &egui::Context) -> Option<&'static str> {
        let p = theme::pal();
        if let Some(rx) = &self.preset_rx {
            while let Ok((name, img)) = rx.try_recv() {
                let ci = to_texture(ctx, "preset", &img);
                self.preset_tex.insert(name, ctx.load_texture(format!("preset-{name}"), ci, egui::TextureOptions::LINEAR));
            }
            if self.preset_tex.len() < MATERIAL_PRESETS.len() {
                ctx.request_repaint_after(std::time::Duration::from_millis(120));
            } else {
                self.preset_rx = None;
            }
        } else if self.strip_open && self.preset_tex.is_empty() {
            let root = self.root.clone();
            let sphere = self.sphere.get_or_insert_with(|| std::sync::Arc::new(crate::model3d::preview_sphere())).clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let c2 = ctx.clone();
            std::thread::spawn(move || {
                for name in MATERIAL_PRESETS {
                    let Some(g) = material_preset(name) else { continue };
                    let Ok(prog) = Program::compile(&g, &root, 256) else { continue };
                    let img = crate::model3d::render_material_thumb(&sphere, std::sync::Arc::new(prog), 80);
                    if tx.send((name, img)).is_err() {
                        return;
                    }
                    c2.request_repaint();
                }
            });
            self.preset_rx = Some(rx);
        }
        let painter = ui.painter_at(r);
        painter.rect_filled(r, 0.0, p.panel);
        painter.hline(r.x_range(), r.top() + 0.5, Stroke::new(1.0, p.line));
        let head = Rect::from_min_size(r.min, vec2(r.width(), 26.0));
        let hr = ui.interact(head, ui.scope_id().with("preset-strip-head"), Sense::click()).on_hover_text(crate::i18n::t("Afficher ou masquer"));
        painter.text(pos2(head.left() + 12.0, head.center().y), egui::Align2::LEFT_CENTER, format!("{}  {}", if self.strip_open { crate::lucide::CHEVRON_DOWN } else { crate::lucide::CHEVRON_RIGHT }, crate::i18n::t("Préréglages")), egui::FontId::proportional(12.0), p.text);
        painter.text(pos2(head.right() - 12.0, head.center().y), egui::Align2::RIGHT_CENTER, crate::i18n::t("un clic remplace le graphe actuel"), egui::FontId::proportional(11.0), p.muted);
        if hr.clicked() {
            self.strip_open ^= true;
        }
        if !self.strip_open {
            return None;
        }
        let body = Rect::from_min_max(pos2(r.left(), head.bottom()), r.max);
        let mut chosen = None;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body.shrink2(vec2(8.0, 2.0))).layout(egui::Layout::left_to_right(egui::Align::Min)));
        egui::ScrollArea::horizontal().id_salt("preset-strip").auto_shrink([false, false]).show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for name in MATERIAL_PRESETS {
                let (cell, resp) = ui.allocate_exact_size(vec2(80.0, 92.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(cell, 4.0, p.raised);
                }
                let img_r = Rect::from_center_size(pos2(cell.center().x, cell.top() + 32.0), vec2(60.0, 60.0));
                match self.preset_tex.get(name) {
                    Some(t) => {
                        ui.painter().image(t.id(), img_r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        ui.painter().circle_filled(img_r.center(), 24.0, p.raised);
                    }
                }
                // Nom sur deux lignes au plus, coupé proprement
                let mut job = egui::text::LayoutJob::simple(crate::i18n::t(name), egui::FontId::proportional(10.5), p.text, cell.width() - 4.0);
                job.halign = egui::Align::Center;
                job.wrap.max_rows = 2;
                job.wrap.break_anywhere = false;
                let gal = ui.painter().layout_job(job);
                ui.painter().galley(pos2(cell.center().x, cell.top() + 64.0), gal, p.text);
                if resp.on_hover_text(crate::i18n::t(name)).clicked() {
                    chosen = Some(name);
                }
            }
        });
        chosen
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
        if self.thumb_rx.is_some() || self.thumb_version == self.version || self.graph.kind == GraphKind::Geometry {
            return;
        }
        self.thumb_version = self.version;
        let ids: Vec<(u32, usize)> = self.graph.nodes.iter().filter(|n| n.preview && !n.collapsed && n.def().is_some_and(|d| !d.outputs.is_empty())).map(|n| (n.id, 0)).collect();
        if ids.is_empty() {
            return;
        }
        let g = self.graph.clone();
        let root = self.root.clone();
        let version = self.version;
        let frame = self.frame;
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
                let Ok(prog) = Program::compile_at(&pg, &root, 256, frame, 24.0) else { continue };
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
        if self.graph.stack.is_some() {
            self.atelier_ui(ui, &ctx);
            return;
        }
        let kind = self.graph.kind;
        // Disposition de la maquette : bibliothèque à gauche, aperçu et réglages à droite, graphe au centre
        let mut add_now: Option<Pick> = None;
        let picks = self.picks();
        let mut preset: Option<&'static str> = None;
        let avail = ui.available_rect_before_wrap();
        // Bibliothèque, inspecteur, préréglages : chacun peut sortir dans sa fenêtre ou sa case, et rend alors sa place au graphe
        let lib_away = crate::layout::offer("node-lib", "bibliothèque de nodes").is_some();
        let insp_away = crate::layout::offer("node-inspector", "aperçu et réglages (nodes)").is_some();
        let strip_away = crate::layout::offer("node-presets", "préréglages de matériaux").is_some();
        let lib_w = if lib_away { 0.0 } else { 240.0f32.min(avail.width() * 0.22) };
        // La géométrie se regarde en grand : son aperçu prend plus de place
        let insp_w = if insp_away {
            0.0
        } else if kind == GraphKind::Geometry {
            380.0f32.min(avail.width() * 0.34)
        } else {
            290.0f32.min(avail.width() * 0.28)
        };
        let lib_rect = Rect::from_min_size(avail.min, vec2(lib_w, avail.height()));
        let insp_rect = Rect::from_min_max(pos2(avail.right() - insp_w, avail.top()), avail.max);
        let graph_rect = Rect::from_min_max(pos2(lib_rect.right(), avail.top()), pos2(insp_rect.left(), avail.bottom()));
        // Bande des préréglages sous le graphe (matériaux seulement)
        let strip_h = if kind == GraphKind::Material && !strip_away { if self.strip_open { 126.0 } else { 28.0 } } else { 0.0 };
        let strip_rect = Rect::from_min_max(pos2(graph_rect.left(), graph_rect.bottom() - strip_h), graph_rect.max);
        let graph_rect = Rect::from_min_max(graph_rect.min, pos2(graph_rect.right(), graph_rect.bottom() - strip_h));
        if kind == GraphKind::Material {
            let picked = crate::dock::area(ui, "node-presets", "préréglages de matériaux", strip_rect, p.panel, 0.0, |ui| {
                let r = ui.max_rect();
                self.preset_strip(ui, r, &ctx)
            });
            if let Some(name) = picked {
                preset = Some(name);
            }
        }
        // ---- Bibliothèque de nodes : chercher, puis une rangée par node, rangés par catégorie
        crate::dock::area(ui, "node-lib", "bibliothèque de nodes", lib_rect, p.panel, 0.0, |lui| {
            let lib_rect = lui.max_rect();
            lui.painter().vline(lib_rect.right() - 0.5, lib_rect.y_range(), Stroke::new(1.0, p.line));
            let (hr, _) = lui.allocate_exact_size(vec2(lib_rect.width(), 34.0), Sense::hover());
            lui.painter().hline(hr.x_range(), hr.bottom() - 0.5, Stroke::new(1.0, p.line));
            lui.painter().text(hr.left_center() + vec2(12.0, 0.0), egui::Align2::LEFT_CENTER, crate::i18n::t("Nodes"), egui::FontId::new(13.0, theme::bold()), p.text);
            lui.add_space(10.0);
            lui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.add(egui::TextEdit::singleline(&mut self.lib_filter).hint_text(format!("{}  {}", crate::lucide::SEARCH, crate::i18n::t("Chercher un node"))).desired_width(lib_rect.width() - 24.0));
            });
            lui.add_space(4.0);
            let f = crate::chat::fold(&self.lib_filter);
            egui::ScrollArea::vertical().id_salt("node-lib").auto_shrink([false, false]).show(lui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for cat in CATS {
                    let items: Vec<&(String, &'static str, Pick, bool)> = picks.iter().filter(|x| x.1 == cat).filter(|x| f.is_empty() || crate::chat::fold(&x.0).contains(&f)).collect();
                    if items.is_empty() {
                        continue;
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(RichText::new(crate::i18n::sentence(&crate::i18n::t(cat))).size(11.0).color(p.muted));
                    });
                    ui.add_space(2.0);
                    for (label, _, pick, _) in items {
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click_and_drag());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, 0.0, p.line.linear_multiply(0.45));
                        }
                        let col = if matches!(pick, Pick::Group(_)) { cat_color("groupe") } else { cat_color(cat) };
                        ui.painter().rect_filled(Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(8.0, 8.0)), 2.0, col);
                        ui.painter().with_clip_rect(r).text(pos2(r.left() + 28.0, r.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(12.5), p.text);
                        if resp.on_hover_text(crate::i18n::t("cliquer pour l'ajouter au centre du graphe")).clicked() {
                            add_now = Some(pick.clone());
                        }
                    }
                }
            });
        });
        // ---- Inspecteur : aperçu carré, puis le node choisi
        crate::dock::area(ui, "node-inspector", "aperçu et réglages (nodes)", insp_rect, p.panel, 0.0, |ui| {
        let insp_rect = ui.max_rect();
        ui.painter().vline(insp_rect.left() + 0.5, insp_rect.y_range(), Stroke::new(1.0, p.line));
        // Dans une grande fenêtre, l'aperçu garde une taille raisonnable
        let side = (insp_rect.width() - 24.0).min((insp_rect.height() * 0.6).max(120.0));
        let prev_rect = Rect::from_min_size(insp_rect.min + vec2(12.0, 46.0), vec2(insp_rect.width() - 24.0, side));
        {
            let (hr, _) = (Rect::from_min_size(insp_rect.min, vec2(insp_rect.width(), 34.0)), ());
            ui.painter().hline(hr.x_range(), hr.bottom() - 0.5, Stroke::new(1.0, p.line));
            let chip = Rect::from_min_size(hr.left_center() + vec2(6.0, -12.0), vec2(70.0, 24.0));
            ui.painter().rect(chip, 2.0, p.raised, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
            ui.painter().text(chip.center(), egui::Align2::CENTER_CENTER, crate::i18n::t(match kind {
                GraphKind::Material => "Matériau",
                GraphKind::Compositor => "Image",
                GraphKind::Geometry => "Géométrie",
            }), egui::FontId::proportional(12.5), p.text);
        }
        self.preview_ui(ui, prev_rect, &ctx);
        {
            let below = Rect::from_min_max(pos2(insp_rect.left() + 12.0, prev_rect.bottom() + 40.0), insp_rect.max - vec2(12.0, 8.0));
            let mut iui = ui.new_child(egui::UiBuilder::new().max_rect(below).layout(egui::Layout::top_down(egui::Align::Min)));
            iui.set_clip_rect(below);
            iui.spacing_mut().item_spacing.y = 6.0;
            if let Some(n) = self.selected.iter().next().and_then(|id| self.graph.nodes.iter().find(|n| n.id == *id)) {
                if let Some(d) = n.def() {
                    iui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                        ui.painter().rect_filled(r, 2.0, cat_color(d.cat));
                        let title = if n.label.is_empty() { crate::i18n::t(d.label) } else { n.label.clone() };
                        ui.label(RichText::new(title).size(15.0).family(theme::bold()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(crate::i18n::t(d.cat)).size(11.0).color(p.muted));
                        });
                    });
                    iui.label(RichText::new(crate::i18n::t("Ses réglages sont sur le node. F2 : renommer · M : muet · P : miniature.")).size(11.5).color(p.muted));
                }
            } else {
                iui.label(RichText::new(crate::i18n::t("Choisis un node pour voir son nom et sa catégorie ici.")).size(11.5).color(p.muted));
            }
            if let Err((id, e)) = &self.program {
                let id = *id;
                let r = iui.add(egui::Label::new(RichText::new(format!("{}  {}", crate::lucide::TRIANGLE_ALERT, crate::i18n::t(e))).color(p.missing).size(11.5)).sense(Sense::click())).on_hover_text(crate::i18n::t("Cliquer pour montrer le node en cause"));
                if r.clicked() && self.graph.nodes.iter().any(|n| n.id == id) {
                    self.selected = [id].into_iter().collect();
                    self.frame_sel = true;
                }
            }
            if let Some(m) = &self.message {
                iui.label(RichText::new(m).color(p.muted).size(11.0));
            }
            iui.add_space(4.0);
            egui::ScrollArea::vertical().id_salt("node-structure").auto_shrink([false, false]).show(&mut iui, |ui| {
                self.structure_ui(ui);
            });
        }
        });
        if let Some(g) = preset.and_then(material_preset) {
            self.leave();
            self.graph = g;
            self.scene = Rect::ZERO;
            self.changed();
        }
        ui.painter().rect_filled(graph_rect, 0.0, p.bg);
        // ---- En-tête flottant du graphe (une zone au-dessus du graphe, qui garde ses clics)
        let hdr_w = (graph_rect.width() - 24.0).max(120.0);
        let mut frame_changed = false;
        crate::dock::overlay(ui, "node-header", "barre du graphe (nodes)", egui::Area::new(ui.scope_id().with("node-hdr")).order(egui::Order::Middle).fixed_pos(graph_rect.min + vec2(12.0, 10.0)), |ui| {
            {
                let v = &mut ui.style_mut().visuals.widgets;
                v.inactive.weak_bg_fill = Color32::TRANSPARENT;
                v.inactive.bg_stroke = Stroke::NONE;
                v.hovered.weak_bg_fill = p.raised;
                v.hovered.bg_stroke = Stroke::NONE;
                v.open.weak_bg_fill = p.raised;
            }
            ui.spacing_mut().button_padding = vec2(6.0, 2.0);
            egui::Frame::new().fill(p.panel).corner_radius(6).stroke(Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(6, 5)).show(ui, |ui| {
            ui.set_width(hdr_w - 14.0);
            ui.set_clip_rect(Rect::from_min_size(graph_rect.min + vec2(12.0, 10.0), vec2(hdr_w, 40.0)));
            ui.horizontal(|ui| {
            if let Some(l) = self.level.clone() {
                // Dans un groupe : d'où l'on vient, où l'on est, et la sortie
                if ui.button(format!("{}  {}", crate::lucide::ARROW_LEFT, crate::i18n::t("Graphe"))).on_hover_text(crate::i18n::t("Sortir du groupe (Tab)")).clicked() {
                    self.leave_request = true;
                }
                ui.label(RichText::new(crate::lucide::CHEVRON_RIGHT).size(11.0).color(p.muted));
                ui.label(RichText::new(format!("{}  {l}", crate::lucide::GROUP)).color(cat_color("groupe")).family(theme::bold()));
                ui.separator();
            }
            egui::containers::menu::MenuButton::new(crate::i18n::t("Ajouter")).ui(ui, |ui| {
                for cat in CATS {
                    let items: Vec<&(String, &'static str, Pick, bool)> = picks.iter().filter(|x| x.1 == cat).collect();
                    if items.is_empty() {
                        continue;
                    }
                    ui.menu_button(crate::i18n::t(cat), |ui| {
                        for (label, _, pick, _) in items {
                            if ui.button(label).clicked() {
                                add_now = Some(pick.clone());
                            }
                        }
                    });
                }
            });
            if kind == GraphKind::Material {
                egui::containers::menu::MenuButton::new(crate::i18n::t("Préréglages")).ui(ui, |ui| {
                    for name in MATERIAL_PRESETS {
                        if ui.button(name).on_hover_text(crate::i18n::t("remplace le graphe actuel")).clicked() {
                            preset = Some(name);
                        }
                    }
                });
            }
            egui::containers::menu::MenuButton::new(crate::i18n::t("Vue")).ui(ui, |ui| {
                if ui.button(format!("{}  {}", crate::lucide::SCAN, crate::i18n::t("Tout cadrer   Origine"))).clicked() {
                    self.scene = Rect::ZERO;
                }
                let all_on = self.graph.nodes.iter().filter(|n| !is_out_op(&n.op)).all(|n| n.preview);
                if ui.button(crate::i18n::t(if all_on { "Cacher les miniatures" } else { "Montrer les miniatures" })).clicked() {
                    for n in self.graph.nodes.iter_mut().filter(|n| !is_out_op(&n.op)) {
                        n.preview = !all_on;
                    }
                    self.changed();
                }
                ui.separator();
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new(format!("{}  {}", crate::lucide::UNDO_2, crate::i18n::t("Annuler   Ctrl+Z")))).clicked() {
                    self.undo();
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new(format!("{}  {}", crate::lucide::REDO_2, crate::i18n::t("Rétablir   Ctrl+Maj+Z")))).clicked() {
                    self.redo();
                }
            });
            egui::containers::menu::MenuButton::new(crate::i18n::t("Aide")).ui(ui, |ui| {
                ui.label(RichText::new(crate::i18n::t("Maj+A ou clic droit : ajouter (taper pour chercher, même une opération : « multiplier »)\nglisser dans le vide : sélection en rectangle (Maj : ajouter)\nclic molette ou clic droit glissé : se déplacer · molette : zoom\nglisser une sortie dans le vide : ajouter un node déjà relié\nCtrl+clic sur un lien : le couper · double-clic : y poser un renvoi\nA : tout · Alt+A : rien · X / Suppr : effacer\nMaj+D : dupliquer · Ctrl+C / Ctrl+V : copier, coller\nM : muet · H : réduire · P : miniature · F2 : renommer\nCtrl+G : grouper · Ctrl+Alt+G : défaire · Tab : entrer dans un groupe, en sortir\nCtrl+J : cadre autour de la sélection · clic droit dans le vide : note\nF : cadrer la sélection · Origine : tout cadrer · Q : ranger\nCtrl+Z / Ctrl+Maj+Z : annuler, rétablir\nGlisser des images depuis l'ordinateur : nodes texture")).size(11.0));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if kind == GraphKind::Compositor && ui.button(format!("{}  {}", crate::lucide::DOWNLOAD, crate::i18n::t("Exporter PNG"))).clicked() {
                    self.export();
                }
                // Le graphe lit le temps : l'image montrée par l'aperçu se choisit ici
                if kind != GraphKind::Geometry && self.uses_time() {
                    let r = ui.add(egui::DragValue::new(&mut self.frame).speed(0.25).range(0.0..=100000.0).max_decimals(0).prefix(format!("{} ", crate::i18n::t("image")))).on_hover_text(crate::i18n::t("Image de l'animation montrée par l'aperçu (lue par le node Temps)"));
                    if r.changed() {
                        self.frame = self.frame.round();
                        frame_changed = true;
                    }
                }
                let name = self.path.file_name().map(|n| crate::formats::strip(&n.to_string_lossy()).to_string()).unwrap_or_default();
                ui.label(RichText::new(format!("{name} · {} {}", self.graph.nodes.len(), crate::i18n::t_mid("nodes"))).size(11.0).color(p.muted));
            });
            });
            });
        });
        if frame_changed {
            self.program = self.compile_doc();
            self.version += 1;
        }
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(graph_rect));
        let over_graph = ctx.pointer_latest_pos().is_some_and(|pt| graph_rect.contains(pt));
        // Cadrage : tout (premier affichage, « recadrer », Origine) ou la sélection (F)
        let frame_all = self.scene == Rect::ZERO || !self.scene.is_finite();
        if frame_all || std::mem::take(&mut self.frame_sel) {
            let mut bb = if frame_all || self.selected.is_empty() { self.bbox(None) } else { self.bbox(Some(&self.selected)) };
            if !bb.is_finite() {
                bb = Rect::from_min_size(Pos2::ZERO, vec2(600.0, 400.0));
            }
            // Marge autour, et la place de l'en-tête flottant en haut
            let mut bb = bb.expand(40.0);
            bb.min.y -= bb.height() * 56.0 / (graph_rect.height() - 56.0).max(100.0);
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
        // Tab entre dans le groupe choisi, ou sort de celui où l'on est. egui s'en sert aussi pour passer
        // d'un champ à l'autre : dès l'appui, un menu prend le focus. On regarde donc si l'on écrivait
        // à l'image d'avant, et on rend le focus.
        if over_graph && self.add.is_none() && !self.was_typing && ctx.input(|i| i.key_pressed(egui::Key::Tab) && !i.modifiers.any()) {
            if let Some(f) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| m.surrender_focus(f));
            }
            match self.selected.iter().next().and_then(|id| self.group_of(*id)).filter(|_| self.selected.len() == 1) {
                Some(name) => self.enter(&name),
                None => self.leave(),
            }
            self.was_typing = false;
        } else {
            self.was_typing = typing;
        }
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
                    self.delete_node(id);
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
            // Ctrl+C / Ctrl+V : egui les transmet comme évènements Copier / Coller, pas comme touches.
            // Les nodes passent par le presse-papiers du système : on les colle aussi dans un autre
            // graphe, une autre fenêtre d'Aryadne.
            let (copy_ev, cut_ev, paste_ev) = ctx.input(|i| {
                let mut r = (false, false, None);
                for e in &i.events {
                    match e {
                        egui::Event::Copy => r.0 = true,
                        egui::Event::Cut => r.1 = true,
                        egui::Event::Paste(t) => r.2 = Some(t.clone()),
                        _ => {}
                    }
                }
                r
            });
            if (cmd && key(egui::Key::C)) || copy_ev || cut_ev {
                let ids: Vec<u32> = self.selected.iter().copied().collect();
                let nodes: Vec<Node> = self.graph.nodes.iter().filter(|n| ids.contains(&n.id) && !is_out_op(&n.op)).cloned().collect();
                let links: Vec<Link> = self.graph.links.iter().filter(|l| ids.contains(&l.from) && ids.contains(&l.to)).cloned().collect();
                if !nodes.is_empty() {
                    self.message = Some(format!("{} {}", nodes.len(), crate::i18n::t_mid("node(s) copié(s)")));
                    // Les groupes posés par ces nodes (et ceux qu'ils contiennent) partent avec la copie
                    let groups = crate::nodeflow::groups_used(&self.doc(), &nodes);
                    if let Ok(t) = serde_json::to_string(&serde_json::json!({ "aryadne_nodes": nodes, "links": links, "groups": groups })) {
                        ctx.copy_text(t);
                    }
                    GROUP_CLIP.with(|c| *c.borrow_mut() = groups);
                    NODE_CLIP.with(|c| *c.borrow_mut() = Some((nodes.clone(), links.clone())));
                    self.clipboard = Some((nodes, links));
                    if cut_ev {
                        for id in ids {
                            self.delete_node(id);
                        }
                        self.selected.clear();
                        self.changed();
                    }
                }
            }
            let pasted: Option<(Vec<Node>, Vec<Link>)> = paste_ev.as_deref().and_then(|t| {
                let v: serde_json::Value = serde_json::from_str(t).ok()?;
                let nodes: Vec<Node> = serde_json::from_value(v.get("aryadne_nodes")?.clone()).ok()?;
                let links: Vec<Link> = serde_json::from_value(v.get("links")?.clone()).unwrap_or_default();
                if let Some(groups) = v.get("groups").and_then(|g| serde_json::from_value::<Vec<crate::nodeflow::Group>>(g.clone()).ok()) {
                    GROUP_CLIP.with(|c| *c.borrow_mut() = groups);
                }
                Some((nodes, links))
            });
            let want_paste = pasted.is_some() || (cmd && key(egui::Key::V));
            if want_paste {
                let clip = pasted.or_else(|| NODE_CLIP.with(|c| c.borrow().clone())).or_else(|| self.clipboard.clone());
                if let Some((nodes, links)) = clip {
                    // Les groupes qui manquent à ce graphe arrivent avec les nodes collés
                    for gr in GROUP_CLIP.with(|c| c.borrow().clone()) {
                        if !self.graph.groups.iter().any(|x| x.name == gr.name) && nodes.iter().any(|n| n.op == "group") {
                            self.graph.groups.push(gr);
                        }
                    }
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
            // Groupes : Ctrl+G réunit, Ctrl+Alt+G défait, Tab entre et sort
            if cmd && alt && key(egui::Key::G) {
                self.ungroup_selection();
            } else if cmd && key(egui::Key::G) {
                self.group_selection();
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
                self.graph.frames.push(Frame { rect: [bb.min.x, bb.min.y - 26.0, bb.width(), bb.height() + 26.0], label: format!("{} {}", crate::i18n::t("cadre"), n + 1), color: cols[n % cols.len()], note: String::new() });
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
        // Image glissée depuis l'explorateur : sur un node qui prend une image, elle la remplace ;
        // dans le vide, elle devient un node texture (ou image, ou vidéo)
        if over_graph {
            if let (Some(rel), Some(pt)) = (egui::DragAndDrop::payload::<String>(&ctx), ctx.pointer_latest_pos()) {
                let g = to_graph(pt);
                let is_vid = is_video_source(&rel);
                let is_img = crate::images::has_preview(&rel) || ["exr", "hdr", "tga"].iter().any(|e| rel.to_lowercase().ends_with(&format!(".{e}")));
                // Le node sous le pointeur, s'il a un champ fichier du bon genre
                let target = self.graph.nodes.iter().rev().find(|n| {
                    let Some(d) = n.def() else { return false };
                    let h = Self::node_size(n, d);
                    g[0] >= n.pos[0] && g[0] <= n.pos[0] + NW && g[1] >= n.pos[1] && g[1] <= n.pos[1] + h && d.params.iter().any(|pp| matches!(pp, ParamDef::File(_))) && ((n.op == "video") == is_vid)
                }).map(|n| (n.id, n.pos));
                if is_vid || is_img {
                    // Pendant le glisser : le node visé s'allume
                    if let Some((_, pos)) = target {
                        let k = graph_rect.width() / self.scene.width().max(1e-3);
                        let tl = pos2(graph_rect.min.x + (pos[0] - self.scene.min.x) * k, graph_rect.min.y + (pos[1] - self.scene.min.y) * k);
                        ui.painter().rect_stroke(Rect::from_min_size(tl, vec2(NW * k, HEAD * k)).expand(3.0), 4.0, Stroke::new(2.0, p.accent), egui::StrokeKind::Outside);
                    }
                    if ctx.input(|i| i.pointer.any_released()) {
                        egui::DragAndDrop::clear_payload(&ctx);
                        let id = match target {
                            Some((id, _)) => id,
                            None => self.graph.add(if is_vid { "video" } else if kind == GraphKind::Material { "texture" } else { "image" }, g),
                        };
                        if let Some(n) = self.graph.nodes.iter_mut().find(|n| n.id == id) {
                            n.params.insert("fichier".into(), Param::S(rel.to_string()));
                        }
                        self.selected = [id].into_iter().collect();
                        self.changed();
                    }
                }
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
                let is_vid = is_video_source(&f.to_string_lossy());
                if !is_vid && !["png", "jpg", "jpeg", "webp", "tga", "tif", "tiff", "bmp", "exr", "hdr", "gif"].contains(&ext.as_str()) {
                    self.message = Some(format!("{} : seules les images et les vidéos vont dans un graphe", f.display()));
                    continue;
                }
                let op = if is_vid { "video" } else { op };
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
        if let Some(pick) = add_now {
            let at = to_graph(graph_rect.center());
            self.add_pick(&pick, [at[0] - NW * 0.5, at[1] - 60.0]);
            self.changed();
        }
        // Actions du graphe
        let mut changed = false;
        let mut enter: Option<String> = None;
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
                    self.connect(a, o, b, i);
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
                Act::Reroute(k, at) => {
                    if let Some(l) = self.graph.links.get(k).cloned() {
                        let id = self.graph.add("reroute", [at.x - REROUTE_W * 0.5, at.y - REROUTE_H * 0.5]);
                        self.graph.links.retain(|x| *x != l);
                        self.graph.links.push(Link { from: l.from, out: l.out, to: id, inp: 0 });
                        self.graph.links.push(Link { from: id, out: 0, to: l.to, inp: l.inp });
                        self.selected = [id].into_iter().collect();
                        changed = true;
                    }
                }
                Act::Enter(id) => enter = self.group_of(id),
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
                        let h = n.def().map(|d| Self::node_size(n, d)).unwrap_or(100.0);
                        if r.intersects(Rect::from_min_size(pos2(n.pos[0], n.pos[1]), vec2(node_w(n), h))) {
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
                            self.delete_node(id);
                            self.selected.remove(&id);
                        }
                        NodeOp::Disconnect => self.graph.links.retain(|l| l.from != id && l.to != id),
                        NodeOp::Group => {
                            self.group_selection();
                        }
                        NodeOp::Ungroup => {
                            self.selected = [id].into_iter().collect();
                            self.ungroup_selection();
                        }
                        NodeOp::Rename => {
                            let cur = self.graph.nodes.iter().find(|n| n.id == id).map(|n| n.label.clone()).unwrap_or_default();
                            self.rename = Some((id, cur));
                        }
                        _ => {
                            if let Some(n) = self.graph.nodes.iter_mut().find(|n| n.id == id) {
                                match op {
                                    NodeOp::Mute => {
                                        if !is_out_op(&n.op) {
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
        if let Some(name) = enter {
            self.enter(&name);
        }
        if std::mem::take(&mut self.group_request) {
            self.group_selection();
        }
        if std::mem::take(&mut self.leave_request) {
            self.leave();
        }
        self.add_menu_ui(&ctx);
        ui.allocate_rect(avail, Sense::hover());
    }

    fn open_add(&mut self, screen: Pos2, graph_at: [f32; 2], link: Option<(u32, usize)>) {
        self.add = Some(AddMenu { at: screen, graph_at, query: String::new(), cat: 0, sel: 0, link });
    }

    /// Menu d'ajout : on tape pour chercher (Entrée pose le premier résultat), ou on parcourt les
    /// catégories à gauche et leurs nodes à droite.
    fn add_menu_ui(&mut self, ctx: &egui::Context) {
        let p = theme::pal();
        if self.add.is_none() {
            return;
        }
        let picks = self.picks();
        let Some(m) = &mut self.add else { return };
        let mut chosen: Option<Pick> = None;
        let mut close = false;
        // Avec un lien en attente : seulement les nodes qui ont une entrée
        let need_input = m.link.is_some();
        let cats: Vec<&'static str> = CATS.iter().copied().filter(|c| picks.iter().any(|x| x.1 == *c && (!need_input || x.3))).collect();
        if m.cat >= cats.len() {
            m.cat = 0;
        }
        // Une rangée : pastille de la catégorie, nom, et la catégorie en discret à droite
        let row = |ui: &mut egui::Ui, label: &str, cat: &str, group: bool, hi: bool, w: f32, show_cat: bool| -> egui::Response {
            let (r, resp) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
            if hi {
                ui.painter().rect_filled(r, 5.0, p.raised);
            }
            ui.painter().rect_filled(Rect::from_center_size(pos2(r.left() + 12.0, r.center().y), vec2(8.0, 8.0)), 2.0, cat_color(if group { "groupe" } else { cat }));
            ui.painter().with_clip_rect(r).text(pos2(r.left() + 24.0, r.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(12.5), if hi { p.accent } else { p.text });
            if show_cat {
                ui.painter().text(pos2(r.right() - 8.0, r.center().y), egui::Align2::RIGHT_CENTER, crate::i18n::t(if group { "groupe" } else { cat }), egui::FontId::proportional(10.5), p.muted);
            }
            resp
        };
        let area = egui::Area::new(egui::Id::unique("node-add")).order(egui::Order::Foreground).fixed_pos(m.at).constrain(true).show(ctx, |ui| {
            egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).corner_radius(8).inner_margin(8).shadow(egui::epaint::Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(70) }).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                let r = ui.add(egui::TextEdit::singleline(&mut m.query).hint_text(format!("{}  {}", crate::lucide::SEARCH, crate::i18n::t("chercher un node…"))).desired_width(372.0));
                r.request_focus();
                ui.add_space(5.0);
                let q = crate::chat::fold(&m.query);
                let (down, up, enter) = ui.input(|i| (i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::Enter)));
                if q.is_empty() {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        // Catégories
                        ui.vertical(|ui| {
                            ui.set_width(124.0);
                            for (ci, cat) in cats.iter().enumerate() {
                                let active = m.cat == ci;
                                let (r, resp) = ui.allocate_exact_size(vec2(124.0, 24.0), Sense::click());
                                if active {
                                    ui.painter().rect_filled(r, 5.0, p.accent);
                                }
                                ui.painter().text(pos2(r.left() + 10.0, r.center().y), egui::Align2::LEFT_CENTER, crate::i18n::t(cat), egui::FontId::proportional(12.5), if active { p.on_accent } else { p.text });
                                ui.painter().text(pos2(r.right() - 8.0, r.center().y), egui::Align2::RIGHT_CENTER, crate::lucide::CHEVRON_RIGHT, egui::FontId::proportional(11.0), if active { p.on_accent } else { p.muted });
                                if (resp.hovered() || resp.clicked()) && m.cat != ci {
                                    m.cat = ci;
                                    m.sel = 0;
                                }
                            }
                        });
                        ui.vertical(|ui| {
                            ui.set_width(240.0);
                            let items: Vec<&(String, &'static str, Pick, bool)> = picks.iter().filter(|x| Some(&x.1) == cats.get(m.cat) && (!need_input || x.3)).collect();
                            egui::ScrollArea::vertical().id_salt("node-add-items").max_height(cats.len().max(8) as f32 * 25.0).show(ui, |ui| {
                                for (k, (label, cat, pick, _)) in items.iter().enumerate() {
                                    let resp = row(ui, label, cat, matches!(pick, Pick::Group(_)), k == m.sel, 240.0, false);
                                    if resp.hovered() {
                                        m.sel = k;
                                    }
                                    if resp.clicked() {
                                        chosen = Some(pick.clone());
                                    }
                                }
                            });
                            let n = items.len();
                            if down && n > 0 {
                                m.sel = (m.sel + 1) % n;
                            }
                            if up && n > 0 {
                                m.sel = (m.sel + n - 1) % n;
                            }
                            if enter {
                                chosen = items.get(m.sel).map(|x| x.2.clone());
                            }
                        });
                    });
                    let (right, left) = ui.input(|i| (i.key_pressed(egui::Key::ArrowRight), i.key_pressed(egui::Key::ArrowLeft)));
                    if right && !cats.is_empty() {
                        m.cat = (m.cat + 1) % cats.len();
                        m.sel = 0;
                    }
                    if left && !cats.is_empty() {
                        m.cat = (m.cat + cats.len() - 1) % cats.len();
                        m.sel = 0;
                    }
                } else {
                    // Chaque mot doit se retrouver dans le nom (français ou anglais), la catégorie ou le
                    // code du node ; les meilleurs d'abord : début de mot, puis milieu, puis lettres dans l'ordre.
                    let list: Vec<(String, &'static str, Pick)> = search_picks(&q, &picks, need_input).into_iter().take(14).map(|x| (x.1, x.2, x.3)).collect();
                    if m.sel >= list.len() {
                        m.sel = 0;
                    }
                    for (k, (label, cat, pick)) in list.iter().enumerate() {
                        let resp = row(ui, label, cat, matches!(pick, Pick::Group(_)), k == m.sel, 372.0, true);
                        if resp.hovered() {
                            m.sel = k;
                        }
                        if resp.clicked() {
                            chosen = Some(pick.clone());
                        }
                    }
                    if list.is_empty() {
                        ui.add_space(4.0);
                        ui.label(RichText::new(crate::i18n::t("aucun node")).color(p.muted));
                    }
                    let n = list.len();
                    if down && n > 0 {
                        m.sel = (m.sel + 1) % n;
                    }
                    if up && n > 0 {
                        m.sel = (m.sel + n - 1) % n;
                    }
                    if enter {
                        chosen = list.get(m.sel).map(|x| x.2.clone());
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close = true;
                }
            });
        });
        let (graph_at, link) = (m.graph_at, m.link);
        let outside = ctx.input(|i| i.pointer.any_pressed()) && !ctx.pointer_latest_pos().is_some_and(|pt| area.response.rect.contains(pt));
        if let Some(pick) = chosen {
            self.add = None;
            if let Some(id) = self.add_pick(&pick, graph_at) {
                if let Some((from, out)) = link {
                    // Le lien en attente va à la première entrée qui l'accepte
                    let from_sock = self.graph.nodes.iter().find(|n| n.id == from).and_then(|n| n.def()).and_then(|d| d.outputs.get(out)).map(|o| o.1).unwrap_or(Sock::Any);
                    let inp = self.graph.nodes.iter().find(|n| n.id == id).and_then(|n| n.def()).and_then(|d| d.inputs.iter().position(|i| socks_fit(from_sock, i.1) && i.0 != crate::nodeflow::VIRTUAL)).unwrap_or(0);
                    self.connect(from, out, id, inp);
                }
            }
            self.changed();
        } else if close || outside {
            self.add = None;
        }
    }

    /// Inspecteur : ce qui touche à la structure. Le groupe choisi (nom, entrer, défaire), l'interface
    /// du groupe où l'on se trouve, les prises de la zone choisie.
    fn structure_ui(&mut self, ui: &mut egui::Ui) {
        use crate::nodeflow::Port;
        let p = theme::pal();
        let first = self.selected.iter().next().copied();
        let mut dirty = false;
        let card = |ui: &mut egui::Ui, title: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
            egui::Frame::new().fill(p.field).corner_radius(8).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(theme::section(&crate::i18n::t(title)));
                ui.add_space(2.0);
                add(ui);
            });
            ui.add_space(6.0);
        };
        let icon = |ui: &mut egui::Ui, icon: &str, tip: &str| ui.add(egui::Button::new(RichText::new(icon).size(14.0)).frame(false).min_size(vec2(24.0, 22.0))).on_hover_text(crate::i18n::t(tip)).clicked();
        // Une liste de prises : pastille du type (clic : changer), nom, valeur par défaut, corbeille
        let allow_geo = self.graph.kind == GraphKind::Geometry;
        let ports_ui = |ui: &mut egui::Ui, ports: &mut Vec<Port>, salt: &str, defaults: bool| -> (bool, Option<usize>, Option<Port>) {
            let (mut changed, mut removed, mut added) = (false, None, None);
            for (k, port) in ports.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let (dot, dr) = ui.allocate_exact_size(vec2(16.0, 20.0), Sense::click());
                    ui.painter().circle_filled(dot.center(), 5.5, sock_color(port.sock));
                    let dr = dr.on_hover_text(crate::i18n::t("Type de la prise (clic pour changer)"));
                    egui::Popup::menu(&dr).show(|ui| {
                        for (s, name) in [(Sock::Float, "nombre à virgule"), (Sock::Int, "entier"), (Sock::Bool, "vrai / faux"), (Sock::Vector, "vecteur"), (Sock::Color, "couleur"), (Sock::Geo, "géométrie")] {
                            if s == Sock::Geo && !allow_geo {
                                continue;
                            }
                            if ui.selectable_label(port.sock == s, crate::i18n::t(name)).clicked() {
                                port.sock = s;
                                changed = true;
                                ui.close();
                            }
                        }
                    });
                    let w = ui.available_width() - if defaults && matches!(port.sock, Sock::Float | Sock::Int) { 86.0 } else { 30.0 };
                    if ui.add(egui::TextEdit::singleline(&mut port.name).desired_width(w.max(40.0)).id_salt((salt, k))).changed() {
                        changed = true;
                    }
                    if defaults && matches!(port.sock, Sock::Float | Sock::Int) {
                        let mut x = port.value[0];
                        if ui.add(egui::DragValue::new(&mut x).speed(0.01).max_decimals(3)).on_hover_text(crate::i18n::t("Valeur de départ sur un nouveau node de ce groupe")).changed() {
                            port.value = [x; 4];
                            changed = true;
                        }
                    }
                    if ui.add(egui::Button::new(RichText::new(crate::lucide::TRASH_2).size(12.5)).frame(false)).on_hover_text(crate::i18n::t("Retirer cette prise et ses liens")).clicked() {
                        removed = Some(k);
                    }
                });
            }
            if ui.add(egui::Button::new(RichText::new(format!("{}  {}", crate::lucide::PLUS, crate::i18n::t("ajouter une prise"))).size(11.5)).frame(false)).on_hover_text(crate::i18n::t("Ou relie un node à l'anneau vide « nouvelle prise »")).clicked() {
                added = Some(Port { name: crate::i18n::t_mid("valeur"), sock: Sock::Float, value: [0.0; 4] });
            }
            (changed, removed, added)
        };
        // 1. Le node de groupe choisi
        if let Some(name) = first.and_then(|id| self.group_of(id)) {
            let uses = self.with_doc(|g| crate::nodeflow::group_uses(g, &name));
            let (mut enter, mut ungroup) = (false, false);
            let mut text = match &self.group_rename {
                Some((old, t)) if *old == name => t.clone(),
                _ => name.clone(),
            };
            let mut commit = false;
            card(ui, "GROUPE", &mut |ui| {
                ui.horizontal(|ui| {
                    let te = ui.add(egui::TextEdit::singleline(&mut text).desired_width(ui.available_width() - 62.0));
                    commit |= te.lost_focus();
                    if icon(ui, crate::lucide::LOG_IN, "Entrer dans le groupe (Tab)") {
                        enter = true;
                    }
                    if icon(ui, crate::lucide::UNGROUP, "Défaire : remettre son contenu à plat ici (Ctrl+Alt+G)") {
                        ungroup = true;
                    }
                });
                ui.label(RichText::new(if uses > 1 { format!("{} {uses} {}", crate::i18n::t("Posé"), crate::i18n::t_mid("fois : le modifier change tous ses exemplaires.")) } else { crate::i18n::t("Ses entrées se règlent sur le node. Pose-le ailleurs depuis « Ajouter › structure ».") }).size(11.0).color(p.muted));
            });
            self.group_rename = (text != name).then(|| (name.clone(), text.clone()));
            if commit && text != name {
                let ok = self.with_doc(|g| crate::nodeflow::group_rename(g, &name, &text));
                self.group_rename = None;
                if ok {
                    dirty = true;
                } else {
                    self.message = Some(crate::i18n::t("Ce nom de groupe est déjà pris."));
                }
            }
            if enter {
                self.enter(&name);
                return;
            }
            if ungroup {
                self.ungroup_selection();
                return;
            }
        }
        // 2. L'interface du groupe où l'on se trouve
        if let Some(level) = self.level.clone() {
            let (mut ins, mut outs) = self.graph.groups.iter().find(|g| g.name == level).map(|g| (g.inputs.clone(), g.outputs.clone())).unwrap_or_default();
            let mut edits: Vec<(bool, (bool, Option<usize>, Option<Port>))> = vec![];
            card(ui, "ENTRÉES DU GROUPE", &mut |ui| edits.push((true, ports_ui(ui, &mut ins, "gin", true))));
            card(ui, "SORTIES DU GROUPE", &mut |ui| edits.push((false, ports_ui(ui, &mut outs, "gout", false))));
            for (input, (changed, removed, added)) in edits {
                if changed {
                    let list = if input { ins.clone() } else { outs.clone() };
                    self.with_doc(|g| {
                        if let Some(gr) = g.groups.iter_mut().find(|g| g.name == level) {
                            if input {
                                gr.inputs = list;
                            } else {
                                gr.outputs = list;
                            }
                        }
                    });
                    dirty = true;
                }
                if let Some(k) = removed {
                    self.with_doc(|g| crate::nodeflow::group_remove_port(g, &level, input, k));
                    dirty = true;
                }
                if let Some(port) = added {
                    self.with_doc(|g| crate::nodeflow::group_add_port(g, &level, input, port));
                    dirty = true;
                }
            }
        }
        // Géométrie : les attributs de ce qui sort du node choisi
        if let (Some(id), Some(mut geo)) = (first, first.and_then(|_| self.geo.take())) {
            let shows = self.graph.nodes.iter().find(|n| n.id == id).is_some_and(|n| n.op == "geo_out" || n.def().is_some_and(|d| d.outputs.first().map(|o| o.1) == Some(Sock::Geo)));
            if shows {
                geo.attributes(ui, &self.graph, id, self.version, &self.root);
            }
            self.geo = Some(geo);
        }
        // 3. Les prises de la zone choisie
        if let Some(id) = first.filter(|id| self.graph.nodes.iter().any(|n| n.id == *id && crate::nodeflow::is_zone(&n.op))) {
            let sim = self.graph.nodes.iter().any(|n| n.id == id && n.op.starts_with("sim"));
            // Simulation : figer le cache sur le disque, ou le libérer
            if sim && self.level.is_none() {
                let end = if self.graph.nodes.iter().any(|n| n.id == id && n.op == "sim_out") { Some(id) } else { crate::nodeflow::zone_pair(&self.graph, id) };
                if let Some(end) = end {
                    let baked = self.graph.nodes.iter().find(|n| n.id == end).is_some_and(|n| matches!(n.params.get("cache"), Some(Param::S(s)) if !s.is_empty()));
                    let mut toggle = false;
                    card(ui, "CACHE DE LA SIMULATION", &mut |ui| {
                        ui.horizontal(|ui| {
                            let (ic, label) = if baked { (crate::lucide::ROTATE_CCW, "libérer le cache") } else { (crate::lucide::HARD_DRIVE, "figer sur le disque") };
                            if ui.add(egui::Button::new(format!("{ic}  {}", crate::i18n::t(label))).corner_radius(10)).on_hover_text(crate::i18n::t(if baked { "La zone se recalcule de nouveau d'après ses réglages" } else { "Calcule toutes les images et les écrit dans le coffre : la zone se relit ensuite sans calcul, même si l'amont change" })).clicked() {
                                toggle = true;
                            }
                        });
                        ui.label(RichText::new(crate::i18n::t(if baked { "Figé : les images viennent du disque." } else { "En mémoire : chaque image calculée est gardée tant que rien ne change en amont." })).size(11.0).color(p.muted));
                    });
                    if toggle {
                        self.bake_zone(end, !baked);
                        return;
                    }
                }
            }
            let mut ports = self.graph.nodes.iter().find(|n| n.id == id).map(|n| n.ins.clone()).unwrap_or_default();
            let mut edit = (false, None, None);
            card(ui, if sim { "ÉTAT DE LA SIMULATION" } else { "ÉTAT DE LA RÉPÉTITION" }, &mut |ui| {
                edit = ports_ui(ui, &mut ports, "zone", false);
                ui.label(RichText::new(crate::i18n::t(if sim { "Ce qui passe d'une image à la suivante. Le contenu de la zone s'exécute une fois par image." } else { "Ce qui passe d'un tour au suivant. Le contenu de la zone s'exécute autant de fois qu'il y a d'itérations." })).size(11.0).color(p.muted));
            });
            let pair = crate::nodeflow::zone_pair(&self.graph, id);
            if edit.0 {
                for n in self.graph.nodes.iter_mut().filter(|n| n.id == id || Some(n.id) == pair) {
                    n.ins = ports.clone();
                }
                dirty = true;
            }
            if let Some(k) = edit.1 {
                crate::nodeflow::zone_remove_port(&mut self.graph, id, k);
                dirty = true;
            }
            if let Some(port) = edit.2 {
                crate::nodeflow::zone_add_port(&mut self.graph, id, port);
                dirty = true;
            }
        }
        if dirty {
            self.changed();
        }
    }

    /// Fond teinté derrière chaque zone : on voit d'un coup d'œil ce qui se répète ou se simule.
    fn draw_zones(&self, painter: &egui::Painter, layout: &[(u32, Rect, &'static Def)]) {
        for n in self.graph.nodes.iter().filter(|n| matches!(n.op.as_str(), "repeat_in" | "sim_in")) {
            let Some(end) = crate::nodeflow::zone_pair(&self.graph, n.id) else { continue };
            let body = crate::nodeflow::zone_body(&self.graph, n.id, end);
            let mut bb = Rect::NOTHING;
            for (id, r, _) in layout {
                if *id == n.id || *id == end || body.contains(id) {
                    bb = bb.union(*r);
                }
            }
            if !bb.is_finite() {
                continue;
            }
            let col = cat_color(if n.op == "sim_in" { "simulation" } else { "répétition" });
            painter.rect(bb.expand(18.0), 14.0, col.gamma_multiply(0.07), Stroke::new(1.0, col.gamma_multiply(0.5)), egui::StrokeKind::Outside);
        }
    }

    /// Relie une sortie à une entrée. Brancher la prise libre d'un groupe ou d'une zone crée
    /// d'abord la vraie prise, du type de ce qu'on y branche.
    fn connect(&mut self, from: u32, out: usize, to: u32, inp: usize) {
        use crate::nodeflow::{virtual_input, virtual_output, Port, VIRTUAL};
        let find = |g: &Graph, id: u32| g.nodes.iter().find(|n| n.id == id).cloned();
        let (Some(a), Some(b)) = (find(&self.graph, from), find(&self.graph, to)) else { return };
        let (Some(da), Some(db)) = (a.def(), b.def()) else { return };
        let (Some(o), Some(i)) = (da.outputs.get(out), db.inputs.get(inp)) else { return };
        if !socks_fit(o.1, i.1) {
            self.message = Some(crate::i18n::t("Ces deux prises ne vont pas ensemble : une géométrie ne se branche que sur une géométrie."));
            return;
        }
        let named = |name: &str, other: &str| if name.is_empty() || name == VIRTUAL { if other.is_empty() || other == VIRTUAL { "valeur".to_string() } else { other.to_string() } } else { name.to_string() };
        let typed = |s: Sock, other: Sock| if s == Sock::Any { if other == Sock::Any { Sock::Float } else { other } } else { s };
        let (free_out, free_in) = (virtual_output(&a) == Some(out), virtual_input(&b) == Some(inp));
        if free_out && free_in {
            return;
        }
        if free_out {
            // Entrée de groupe : elle prend le nom, le type et la valeur de l'entrée qu'on vise
            let Some(level) = self.level.clone() else { return };
            let value = b.values.get(inp).copied().unwrap_or(i.2);
            let port = Port { name: named(i.0, ""), sock: typed(i.1, Sock::Any), value };
            self.with_doc(|g| crate::nodeflow::group_add_port(g, &level, true, port));
        }
        if free_in {
            let port = Port { name: named(o.0, ""), sock: typed(o.1, Sock::Any), value: [0.0; 4] };
            if b.op == "group_out" {
                let Some(level) = self.level.clone() else { return };
                self.with_doc(|g| crate::nodeflow::group_add_port(g, &level, false, port));
            } else {
                crate::nodeflow::zone_add_port(&mut self.graph, to, port);
            }
        }
        // La nouvelle prise a pris le rang de la prise libre : le lien y va tel quel
        self.graph.connect(from, out, to, inp);
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
            if ui.button(format!("{}  {}", crate::lucide::PLUS, crate::i18n::t("ajouter un node…   Maj+A"))).clicked() {
                if let Some(pt) = pointer {
                    actions.push(Act::OpenAdd(pt, None));
                }
                ui.close();
            }
            if ui.add_enabled(self.clipboard.is_some(), egui::Button::new(format!("{}  {}", crate::lucide::CLIPBOARD_PASTE, crate::i18n::t("coller   Ctrl+V")))).clicked() {
                if let (Some((nodes, links)), Some(pt)) = (self.clipboard.clone(), pointer) {
                    let min = nodes.iter().fold([f32::MAX; 2], |m, n| [m[0].min(n.pos[0]), m[1].min(n.pos[1])]);
                    let new = self.paste(nodes, links, [pt.x - min[0], pt.y - min[1]]);
                    self.selected = new.into_iter().collect();
                    actions.push(Act::Changed);
                }
                ui.close();
            }
            if ui.button(format!("{}  {}", crate::lucide::CHECK_CHECK, crate::i18n::t("tout sélectionner   A"))).clicked() {
                self.selected = self.graph.nodes.iter().map(|n| n.id).collect();
                ui.close();
            }
            if !self.selected.is_empty() && ui.button(format!("{}  {}", crate::lucide::FRAME, crate::i18n::t("cadre autour de la sélection   Ctrl+J"))).clicked() {
                self.frame_sel_request = true;
                ui.close();
            }
            if ui.button(format!("{}  {}", crate::lucide::STICKY_NOTE, crate::i18n::t("poser une note ici"))).clicked() {
                if let Some(pt) = pointer {
                    self.graph.frames.push(Frame { rect: [pt.x, pt.y, 240.0, 150.0], label: crate::i18n::t("note"), color: [200, 140, 60], note: String::new() });
                    self.note_edit = Some(self.graph.frames.len() - 1);
                    actions.push(Act::Changed);
                }
                ui.close();
            }
            if !self.selected.is_empty() && ui.button(format!("{}  {}", crate::lucide::GROUP, crate::i18n::t("grouper la sélection   Ctrl+G"))).clicked() {
                self.group_request = true;
                ui.close();
            }
            if self.level.is_some() && ui.button(format!("{}  {}", crate::lucide::ARROW_LEFT, crate::i18n::t("sortir du groupe   Tab"))).clicked() {
                self.leave_request = true;
                ui.close();
            }
            if ui.button(format!("{}  {}", crate::lucide::SCAN, crate::i18n::t("tout cadrer   Origine"))).clicked() {
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
            // Groupe comme la maquette : fond à peine teinté, contour en pointillés, titre discret
            painter.rect_filled(r, 10.0, col.gamma_multiply(0.06));
            let pts = vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
            painter.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.0, col.gamma_multiply(0.7)), 5.0, 4.0));
            let title = Rect::from_min_size(r.min, vec2(r.width(), 26.0));
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
                    painter.text(title.left_center() + vec2(10.0, 0.0), egui::Align2::LEFT_CENTER, &fr.label, egui::FontId::proportional(12.0), p.muted);
                }
            }
            // Note : un texte libre dans le cadre (menu du titre, ou « poser une note ici »)
            let note_rect = Rect::from_min_max(pos2(r.left() + 10.0, title.bottom() + 2.0), pos2(r.right() - 10.0, r.bottom() - 14.0));
            if self.note_edit == Some(k) {
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(note_rect).layout(egui::Layout::top_down(egui::Align::Min)));
                let te = child.add_sized(note_rect.size(), egui::TextEdit::multiline(&mut self.graph.frames[k].note).font(egui::FontId::proportional(12.5)).hint_text(crate::i18n::t("Écris ta note ici…")));
                if te.lost_focus() || child.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.note_edit = None;
                    actions.push(Act::Changed);
                } else if !te.has_focus() {
                    te.request_focus();
                }
            } else if !fr.note.is_empty() {
                let gal = painter.layout(fr.note.clone(), egui::FontId::proportional(12.5), p.text, note_rect.width());
                painter.with_clip_rect(note_rect).galley(note_rect.min, gal, p.text);
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
                if ui.button(format!("{}  {}", crate::lucide::PENCIL, crate::i18n::t("renommer le cadre"))).clicked() {
                    self.frame_rename = Some((k, fr.label.clone()));
                    ui.close();
                }
                if ui.button(format!("{}  {}", crate::lucide::STICKY_NOTE, crate::i18n::t("écrire une note dans le cadre"))).clicked() {
                    self.note_edit = Some(k);
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
                if ui.button(format!("{}  {}", crate::lucide::TRASH_2, crate::i18n::t("supprimer le cadre"))).clicked() {
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
        let layout: Vec<(u32, Rect, &'static Def)> = self.graph.nodes.iter().filter_map(|n| n.def().map(|d| (n.id, Rect::from_min_size(pos2(n.pos[0], n.pos[1]), vec2(node_w(n), Self::node_size(n, d))), d))).collect();
        // Un renvoi se dessine comme un node réduit : ses deux prises à mi-hauteur
        let collapsed: std::collections::HashSet<u32> = self.graph.nodes.iter().filter(|n| n.collapsed || n.op == "reroute").map(|n| n.id).collect();
        self.draw_zones(&painter, &layout);
        for (id, r, d) in &layout {
            let c = collapsed.contains(id);
            for k in 0..d.outputs.len() {
                out_pos.insert((*id, k), if c { pos2(r.right(), r.center().y) } else { pos2(r.right(), r.top() + HEAD + (k as f32 + 0.5) * ROW) });
            }
            for k in 0..d.inputs.len() {
                in_pos.insert((*id, k), if c { pos2(r.left(), r.center().y) } else { pos2(r.left(), r.top() + HEAD + (d.outputs.len() + k) as f32 * ROW + ROW * 0.5) });
            }
        }
        // Type d'une sortie ; un renvoi prend celui de ce qui y entre
        let link_src: HashMap<u32, (u32, usize)> = self.graph.links.iter().filter(|l| l.inp == 0).map(|l| (l.to, (l.from, l.out))).collect();
        let sock_of = |id: u32, out: usize| -> Sock {
            let (mut id, mut out) = (id, out);
            for _ in 0..64 {
                match layout.iter().find(|x| x.0 == id) {
                    Some(x) if x.2.op == "reroute" => match link_src.get(&id) {
                        Some(&(f, o)) => (id, out) = (f, o),
                        None => return Sock::Any,
                    },
                    Some(x) => return x.2.outputs.get(out).map(|o| o.1).unwrap_or(Sock::Float),
                    None => break,
                }
            }
            Sock::Float
        };
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
            // Double-clic : un renvoi se pose là, pour faire passer le lien où l'on veut
            if let (true, Some(pt)) = (ui.input(|i| i.pointer.button_double_clicked(egui::PointerButton::Primary)), pointer) {
                actions.push(Act::Reroute(k, pt));
            }
        }
        // Nodes
        let mut geo_view = self.geo.take();
        let mut hovered_in: Option<(u32, usize)> = None;
        let ids: Vec<u32> = self.graph.nodes.iter().map(|n| n.id).collect();
        let mut rename_done = None;
        for (id, r, d) in &layout {
            let ni = ids.iter().position(|x| x == id).unwrap();
            let selected = self.selected.contains(id);
            let (muted, is_collapsed, preview_on) = (self.graph.nodes[ni].muted, self.graph.nodes[ni].collapsed, self.graph.nodes[ni].preview);
            let cat_col = cat_color(d.cat);
            let cat_col = if muted { Color32::from_gray(130) } else { cat_col };
            let hover_node = pointer.is_some_and(|pt| r.contains(pt));
            if d.op == "reroute" {
                // Renvoi : une pastille. On la déplace par le milieu, on tire un lien par ses bouts.
                let col = sock_color(sock_of(*id, 0));
                let body = ui.interact(r.shrink2(vec2(8.0, 0.0)), ui.scope_id().with(("node-body", *id)), Sense::click_and_drag());
                painter.rect(*r, REROUTE_H * 0.5, p.panel, if selected { Stroke::new(2.0, p.accent) } else if hover_node { Stroke::new(1.5, p.muted) } else { Stroke::new(1.0, p.line) }, egui::StrokeKind::Outside);
                painter.line_segment([r.left_center() + vec2(6.0, 0.0), r.right_center() - vec2(6.0, 0.0)], Stroke::new(2.0, col));
                if body.clicked() && ui.input(|i| i.modifiers.alt) {
                    actions.push(Act::Deselect(*id));
                } else if body.drag_started_by(egui::PointerButton::Primary) || body.clicked() || body.secondary_clicked() {
                    actions.push(Act::Click(*id, shift && !body.secondary_clicked()));
                }
                if body.dragged_by(egui::PointerButton::Primary) {
                    actions.push(Act::Move(body.drag_delta()));
                }
                if body.drag_stopped() {
                    actions.push(Act::MoveEnd);
                }
                body.context_menu(|ui| {
                    if ui.button(format!("{}  {}", crate::lucide::TRASH_2, crate::i18n::t("supprimer le renvoi (les liens se rejoignent)   X"))).clicked() {
                        actions.push(Act::Node(*id, NodeOp::Delete));
                        ui.close();
                    }
                });
                let (sp_in, sp_out) = (in_pos[&(*id, 0)], out_pos[&(*id, 0)]);
                let hov_in = pointer.is_some_and(|pt| sp_in.distance(pt) < 9.0);
                if hov_in && hovered_in.is_none() {
                    hovered_in = Some((*id, 0));
                }
                painter.circle(sp_in, if hov_in && self.drag_link.is_some() { 6.0 } else { 4.5 }, col, Stroke::new(1.0, p.bg));
                painter.circle(sp_out, if pointer.is_some_and(|pt| sp_out.distance(pt) < 9.0) { 6.0 } else { 4.5 }, col, Stroke::new(1.0, p.bg));
                let so = ui.interact(Rect::from_center_size(sp_out, vec2(14.0, 16.0)), ui.scope_id().with(("out", *id, 0usize)), Sense::drag());
                if so.drag_started() {
                    self.drag_link = Some((*id, 0));
                }
                let si = ui.interact(Rect::from_center_size(sp_in, vec2(14.0, 16.0)), ui.scope_id().with(("in", *id, 0usize)), Sense::drag());
                if si.drag_started() {
                    if let Some(l) = self.graph.links.iter().find(|l| l.to == *id && l.inp == 0) {
                        self.drag_link = Some((l.from, l.out));
                        actions.push(Act::Unlink(*id, 0));
                    }
                }
                continue;
            }
            // Comme la maquette : carte du panneau, coins de 6 px, bande de couleur de 3 px en haut
            painter.rect_filled(r.translate(vec2(0.0, 3.0)), 6.0, Color32::from_black_alpha(40));
            painter.rect_filled(*r, 6.0, if muted { p.bg } else { p.panel });
            // Tout le node est cliquable (déclaré avant ses réglages et ses prises, qui gardent la priorité)
            let body = ui.interact(*r, ui.scope_id().with(("node-body", *id)), Sense::click_and_drag());
            let head = Rect::from_min_size(r.min, vec2(r.width(), HEAD));
            painter.rect_filled(Rect::from_min_size(head.min, vec2(head.width(), 3.0)), egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, cat_col);
            painter.hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, p.line));
            // Flèche réduire, titre, miniature
            let arrow = Rect::from_min_size(head.min, vec2(20.0, HEAD));
            let ar = ui.interact(arrow, ui.scope_id().with(("node-arrow", *id)), Sense::click());
            painter.text(arrow.center() + vec2(1.0, 1.0), egui::Align2::CENTER_CENTER, if is_collapsed { crate::lucide::CHEVRON_RIGHT } else { crate::lucide::CHEVRON_DOWN }, egui::FontId::proportional(12.0), p.muted);
            if ar.clicked() {
                actions.push(Act::Node(*id, NodeOp::Collapse));
            }
            let has_out = !d.outputs.is_empty() && !crate::nodeflow::is_zone(d.op) && !matches!(d.op, "group_in" | "group_out");
            let eye = Rect::from_min_size(pos2(head.right() - 22.0, head.top()), vec2(22.0, HEAD));
            if has_out && d.cat != "sortie" && d.op != "group" {
                let er = ui.interact(eye, ui.scope_id().with(("node-eye", *id)), Sense::click()).on_hover_text(crate::i18n::t("miniature (P)"));
                painter.text(eye.center() + vec2(0.0, 1.0), egui::Align2::CENTER_CENTER, if preview_on { crate::lucide::EYE } else { crate::lucide::EYE_OFF }, egui::FontId::proportional(12.0), p.muted);
                if er.clicked() {
                    actions.push(Act::Node(*id, NodeOp::Preview));
                }
            }
            // Node de groupe : une icône pour entrer dedans
            if d.op == "group" {
                let er = ui.interact(eye, ui.scope_id().with(("node-enter", *id)), Sense::click()).on_hover_text(crate::i18n::t("Entrer dans le groupe (Tab)"));
                painter.text(eye.center() + vec2(0.0, 1.0), egui::Align2::CENTER_CENTER, crate::lucide::LOG_IN, egui::FontId::proportional(12.0), if er.hovered() { p.text } else { p.muted });
                if er.clicked() {
                    actions.push(Act::Enter(*id));
                }
            }
            let title_rect = Rect::from_min_max(pos2(arrow.right(), head.top()), pos2(eye.left(), head.bottom()));
            let label = &self.graph.nodes[ni].label;
            // Un node à choix se nomme d'après son opération : on lit le graphe sans ouvrir les menus
            let title = if !label.is_empty() {
                label.clone()
            } else if matches!(d.op, "math" | "vmath" | "compare" | "logic") {
                match d.params.first().map(|pd| if let ParamDef::Choice(k, _) = pd { pstr(&self.graph.nodes[ni].params, k) } else { "" }) {
                    Some(o) if !o.is_empty() => crate::i18n::t(o),
                    _ => crate::i18n::t(d.label),
                }
            } else {
                crate::i18n::t(d.label)
            };
            let title = if muted { format!("{title} ({})", crate::i18n::t_mid("muet")) } else { title };
            match &mut self.rename {
                Some((rid, text)) if rid == id => {
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(title_rect.shrink(2.0)));
                    let te = child.add(egui::TextEdit::singleline(text).desired_width(title_rect.width() - 6.0).font(egui::FontId::proportional(11.0)));
                    te.request_focus();
                    if te.lost_focus() || child.input(|i| i.key_pressed(egui::Key::Enter)) {
                        rename_done = Some((*id, text.clone()));
                    }
                }
                _ => {
                    painter.with_clip_rect(title_rect).text(title_rect.left_center() + vec2(0.0, 1.0), egui::Align2::LEFT_CENTER, title, egui::FontId::new(12.5, theme::bold()), p.text);
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
            painter.rect_stroke(*r, 6.0, border, egui::StrokeKind::Outside);
            if err_node == Some(*id) {
                // L'erreur s'écrit sous le node fautif, en clair
                if let Err((_, e)) = &self.program {
                    let gal = painter.layout(format!("{}  {}", crate::lucide::TRIANGLE_ALERT, crate::i18n::t(e)), egui::FontId::proportional(11.0), p.on_accent, NW + 40.0);
                    let br = Rect::from_min_size(pos2(r.left() - 2.0, r.bottom() + 8.0), gal.size() + vec2(16.0, 10.0));
                    painter.rect_filled(br, 6.0, p.missing);
                    painter.galley(br.min + vec2(8.0, 5.0), gal, p.on_accent);
                }
            }
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
                    if matches!(op, NodeOp::Preview | NodeOp::Solo) && !has_out {
                        continue;
                    }
                    if ui.button(crate::i18n::t(label)).clicked() {
                        actions.push(Act::Node(*id, op));
                        ui.close();
                    }
                }
                ui.separator();
                if d.op == "group" {
                    if ui.button(format!("{}  {}", crate::lucide::LOG_IN, crate::i18n::t("entrer dans le groupe   Tab"))).clicked() {
                        actions.push(Act::Enter(*id));
                        ui.close();
                    }
                    if ui.button(format!("{}  {}", crate::lucide::UNGROUP, crate::i18n::t("défaire le groupe   Ctrl+Alt+G"))).clicked() {
                        actions.push(Act::Node(*id, NodeOp::Ungroup));
                        ui.close();
                    }
                }
                if ui.button(format!("{}  {}", crate::lucide::GROUP, crate::i18n::t("grouper la sélection   Ctrl+G"))).clicked() {
                    actions.push(Act::Node(*id, NodeOp::Group));
                    ui.close();
                }
            };
            hr.context_menu(|ui| menu(ui, actions));
            body.context_menu(|ui| menu(ui, actions));
            // Prises (toujours, même réduit)
            for (k, (name, sock)) in d.outputs.iter().enumerate() {
                let sp = out_pos[&(*id, k)];
                let free = *sock == Sock::Any && *name == crate::nodeflow::VIRTUAL;
                if !is_collapsed {
                    painter.text(pos2(r.right() - 12.0, sp.y), egui::Align2::RIGHT_CENTER, crate::i18n::t(name), egui::FontId::proportional(11.0), if free { p.muted } else { p.text });
                }
                let hov = pointer.is_some_and(|pt| sp.distance(pt) < 9.0);
                if free {
                    // Prise libre : un anneau vide, à relier pour créer une vraie prise
                    painter.circle(sp, if hov { 6.5 } else { 5.0 }, p.panel, Stroke::new(1.5, p.muted));
                } else {
                    painter.circle(sp, if hov { 6.5 } else { 5.0 }, sock_color(*sock), Stroke::new(1.0, p.bg));
                }
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
                let free = *sock == Sock::Any && *name == crate::nodeflow::VIRTUAL;
                if free {
                    painter.circle(sp, if hov && self.drag_link.is_some() { 6.5 } else { 5.0 }, p.panel, Stroke::new(1.5, p.muted));
                } else {
                    painter.circle(sp, if hov && self.drag_link.is_some() { 6.5 } else { 5.0 }, sock_color(*sock), Stroke::new(1.0, p.bg));
                }
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
                    painter.text(row.left_center(), egui::Align2::LEFT_CENTER, crate::i18n::t(name), egui::FontId::proportional(11.0), p.text);
                } else if *name == "coordonnées" || matches!(sock, Sock::Geo | Sock::Any) {
                    // Rien à régler à la main : sans lien, le motif suit la surface ; une géométrie se branche
                    painter.text(row.left_center(), egui::Align2::LEFT_CENTER, crate::i18n::t(name), egui::FontId::proportional(11.0), p.muted);
                } else {
                    while node.values.len() <= k {
                        node.values.push(d.inputs[node.values.len()].2);
                    }
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)));
                    child.spacing_mut().interact_size.y = ROW - 4.0;
                    child.label(RichText::new(crate::i18n::t(name)).size(11.0));
                    let v = &mut node.values[k];
                    let resp = match sock {
                        Sock::Float | Sock::Geo | Sock::Any => child.add(egui::DragValue::new(&mut v[0]).speed(0.01).max_decimals(3)),
                        Sock::Int => {
                            let r = child.add(egui::DragValue::new(&mut v[0]).speed(0.1).max_decimals(0));
                            if r.changed() {
                                v[0] = v[0].round();
                            }
                            r
                        }
                        Sock::Bool => {
                            let mut on = v[0] > 0.5;
                            let r = child.checkbox(&mut on, "");
                            if r.changed() {
                                v[0] = on as u8 as f32;
                            }
                            r
                        }
                        Sock::Vector => {
                            // Trois nombres serrés sur la même rangée
                            child.spacing_mut().item_spacing.x = 1.0;
                            child.spacing_mut().button_padding.x = 2.0;
                            child.spacing_mut().interact_size.x = 30.0;
                            let mut any: Option<egui::Response> = None;
                            for c in 0..3 {
                                let r = child.add(egui::DragValue::new(&mut v[c]).speed(0.01).max_decimals(2));
                                any = Some(match any {
                                    Some(a) => a.union(r),
                                    None => r,
                                });
                            }
                            any.unwrap()
                        }
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
                        if matches!(sock, Sock::Float | Sock::Int | Sock::Bool) {
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
                    ParamDef::Ramp(n) => (3.0 * ROW, *n),
                    ParamDef::Curve(n) => (7.0 * ROW, *n),
                    ParamDef::Choice(n, _) | ParamDef::Float(n, ..) | ParamDef::File(n) | ParamDef::Line(n, _) => (ROW, *n),
                };
                let row = Rect::from_min_size(pos2(r.left() + 8.0, y + 1.0), vec2(NW - 16.0, h - 2.0));
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)));
                let entry = node.params.entry(name.to_string()).or_insert(Param::S(String::new()));
                match (pd, entry) {
                    (ParamDef::Choice(_, opts), Param::S(cur)) => {
                        egui::ComboBox::from_id_salt(("choice", *id, name)).selected_text(RichText::new(crate::i18n::t_mid(cur.as_str())).size(11.0)).width(NW - 24.0).height(320.0).show_ui(&mut child, |ui| {
                            for o in *opts {
                                if ui.selectable_label(cur == o, crate::i18n::t_mid(o)).clicked() {
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
                    (ParamDef::Line(..), Param::S(text)) => {
                        child.label(RichText::new(crate::i18n::t(name)).size(11.0));
                        let w = child.available_width() - 2.0;
                        if child.add(egui::TextEdit::singleline(text).desired_width(w).font(egui::FontId::monospace(11.0))).changed() {
                            actions.push(Act::Changed);
                        }
                    }
                    (ParamDef::Text(_), Param::S(code)) => {
                        // Les uns sous les autres : exemples, demande à l'IA, puis le code sur toute la largeur
                        let mut child = child.new_child(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::top_down(egui::Align::Min)));
                        child.set_clip_rect(child.clip_rect().intersect(row));
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
                            ui.add(egui::TextEdit::singleline(prompt).hint_text(crate::i18n::t("décris l'effet…")).desired_width(NW - 64.0).font(egui::FontId::proportional(10.5)));
                            if ui.add_enabled(!busy && !prompt.trim().is_empty(), egui::Button::new(RichText::new(if busy { "…" } else { "IA" }).size(10.5))).on_hover_text(crate::i18n::t("L'IA écrit le shader")).clicked() {
                                self.ai_request = Some((*id, prompt.trim().to_string()));
                            }
                        });
                        let te = egui::TextEdit::multiline(code).code_editor().desired_width(NW - 18.0).desired_rows(7).font(egui::FontId::proportional(10.5));
                        if child.add(te).changed() {
                            actions.push(Act::Changed);
                        }
                    }
                    (ParamDef::Curve(_), Param::S(text)) => {
                        let mut curves = parse_curves(text);
                        let ch_id = egui::Id::unique(("curve-ch", *id));
                        let mut ch: usize = child.data(|d| d.get_temp(ch_id)).unwrap_or(0).min(3);
                        let mut edited = false;
                        // Choix de la courbe, et remise à zéro
                        let tabs = Rect::from_min_size(row.min, vec2(row.width(), ROW));
                        let mut tu = child.new_child(egui::UiBuilder::new().max_rect(tabs).layout(egui::Layout::left_to_right(egui::Align::Center)));
                        tu.spacing_mut().item_spacing.x = 2.0;
                        let tints = [p.text, Color32::from_rgb(230, 80, 80), Color32::from_rgb(90, 190, 90), Color32::from_rgb(80, 140, 240)];
                        for (k, label) in ["C", "R", "V", "B"].iter().enumerate() {
                            if tu.selectable_label(ch == k, RichText::new(*label).size(11.0).color(tints[k])).on_hover_text(crate::i18n::t(["ensemble", "rouge", "vert", "bleu"][k])).clicked() {
                                ch = k;
                            }
                        }
                        if tu.small_button("↺").on_hover_text(crate::i18n::t("Remettre cette courbe droite")).clicked() {
                            curves[ch] = vec![[0.0, 0.0], [1.0, 1.0]];
                            edited = true;
                        }
                        // Le tracé : clic dans le vide pour ajouter un point, glisser pour le déplacer, clic droit pour le retirer
                        let plot = Rect::from_min_max(pos2(row.left(), row.top() + ROW + 2.0), pos2(row.right(), row.bottom() - 2.0));
                        let resp = child.interact(plot, egui::Id::unique(("curve-plot", *id)), Sense::click_and_drag());
                        let painter = child.painter();
                        painter.rect_filled(plot, 2.0, p.bg);
                        for g in 1..4 {
                            let t = g as f32 / 4.0;
                            painter.line_segment([pos2(plot.left() + plot.width() * t, plot.top()), pos2(plot.left() + plot.width() * t, plot.bottom())], Stroke::new(1.0, p.line));
                            painter.line_segment([pos2(plot.left(), plot.top() + plot.height() * t), pos2(plot.right(), plot.top() + plot.height() * t)], Stroke::new(1.0, p.line));
                        }
                        painter.rect_stroke(plot, 2.0, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
                        let to_screen = |q: [f32; 2]| pos2(plot.left() + q[0] * plot.width(), plot.bottom() - q[1] * plot.height());
                        let from_screen = |at: Pos2| [((at.x - plot.left()) / plot.width()).clamp(0.0, 1.0), ((plot.bottom() - at.y) / plot.height()).clamp(0.0, 1.0)];
                        let grab_id = egui::Id::unique(("curve-grab", *id));
                        let mut grab: Option<usize> = child.data(|d| d.get_temp(grab_id));
                        if let Some(at) = resp.interact_pointer_pos() {
                            let near = curves[ch].iter().enumerate().map(|(k, q)| (k, (to_screen(*q) - at).length())).min_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
                            if resp.secondary_clicked() {
                                if near.1 < 9.0 && curves[ch].len() > 2 {
                                    curves[ch].remove(near.0);
                                    edited = true;
                                }
                            } else if grab.is_none() && resp.is_pointer_button_down_on() && child.input(|i| i.pointer.primary_down()) {
                                if near.1 < 9.0 {
                                    grab = Some(near.0);
                                } else {
                                    let q = from_screen(at);
                                    curves[ch].push(q);
                                    curves[ch].sort_by(|a, b| a[0].total_cmp(&b[0]));
                                    grab = curves[ch].iter().position(|x| *x == q);
                                    edited = true;
                                }
                            }
                            if let (Some(g), true) = (grab, resp.dragged_by(egui::PointerButton::Primary)) {
                                if g < curves[ch].len() {
                                    let mut q = from_screen(at);
                                    // Un point reste entre ses deux voisins
                                    let lo = if g > 0 { curves[ch][g - 1][0] + 0.005 } else { 0.0 };
                                    let hi = if g + 1 < curves[ch].len() { curves[ch][g + 1][0] - 0.005 } else { 1.0 };
                                    q[0] = q[0].clamp(lo.min(hi), hi.max(lo));
                                    curves[ch][g] = q;
                                    edited = true;
                                }
                            }
                        }
                        if !resp.is_pointer_button_down_on() {
                            grab = None;
                        }
                        match grab {
                            Some(g) => {
                                child.data_mut(|d| d.insert_temp(grab_id, g));
                            }
                            None => {
                                child.data_mut(|d| d.remove::<usize>(grab_id));
                            }
                        }
                        let line: Vec<Pos2> = (0..=48).map(|k| k as f32 / 48.0).map(|x| to_screen([x, eval_curve(&curves[ch], x)])).collect();
                        painter.add(egui::Shape::line(line, Stroke::new(1.8, tints[ch])));
                        for (k, q) in curves[ch].iter().enumerate() {
                            painter.circle_filled(to_screen(*q), if grab == Some(k) { 4.5 } else { 3.5 }, tints[ch]);
                            painter.circle_stroke(to_screen(*q), if grab == Some(k) { 4.5 } else { 3.5 }, Stroke::new(1.0, p.bg));
                        }
                        child.data_mut(|d| d.insert_temp(ch_id, ch));
                        if edited {
                            *text = curves_string(&curves);
                            actions.push(Act::Changed);
                        }
                    }
                    (ParamDef::Ramp(_), Param::S(text)) => {
                        let mut stops = parse_ramp(text);
                        let sel_id = egui::Id::unique(("ramp-sel", *id));
                        let mut sel: usize = child.data(|d| d.get_temp(sel_id)).unwrap_or(0).min(stops.len() - 1);
                        let mut edited = false;
                        // La barre : damier sous la transparence, dégradé par tranches, poignées en dessous
                        let bar = Rect::from_min_size(row.min, vec2(row.width(), ROW * 1.25));
                        let resp = child.interact(bar.expand2(vec2(0.0, 6.0)), egui::Id::unique(("ramp-bar", *id)), Sense::click_and_drag());
                        let painter = child.painter();
                        let mode = 0;
                        let strips = 48;
                        for k in 0..strips {
                            let (a, b) = (k as f32 / strips as f32, (k + 1) as f32 / strips as f32);
                            let c = eval_ramp(&stops, (a + b) * 0.5, mode);
                            let seg = Rect::from_min_max(pos2(bar.left() + bar.width() * a, bar.top()), pos2(bar.left() + bar.width() * b + 0.5, bar.bottom() - 7.0));
                            if c[3] < 0.999 {
                                painter.rect_filled(seg, 0.0, if k % 2 == 0 { Color32::from_gray(150) } else { Color32::from_gray(100) });
                            }
                            let g = |x: f32| (x.clamp(0.0, 1.0) * 255.0) as u8;
                            painter.rect_filled(seg, 0.0, Color32::from_rgba_unmultiplied(g(c[0]), g(c[1]), g(c[2]), g(c[3])));
                        }
                        painter.rect_stroke(Rect::from_min_max(bar.min, pos2(bar.right(), bar.bottom() - 7.0)), 2.0, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
                        let x_of = |t: f32| bar.left() + bar.width() * t;
                        let t_of = |x: f32| ((x - bar.left()) / bar.width()).clamp(0.0, 1.0);
                        if let Some(at) = resp.interact_pointer_pos() {
                            if resp.drag_started() || resp.clicked() || resp.double_clicked() {
                                // La poignée la plus proche, si elle est sous le curseur
                                // On juge d'après l'endroit où le bouton a été enfoncé : le glissement ne démarre qu'après quelques pixels
                                let down = child.input(|i| i.pointer.press_origin()).unwrap_or(at);
                                let near = stops.iter().enumerate().map(|(k, s)| (k, (x_of(s.0) - down.x).abs())).min_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
                                if near.1 < 8.0 && !resp.double_clicked() {
                                    sel = near.0;
                                } else if resp.double_clicked() || child.input(|i| i.modifiers.ctrl) {
                                    // Double clic (ou Ctrl + clic) : nouvelle couleur, prise dans le dégradé à cet endroit
                                    let t = t_of(at.x);
                                    stops.push((t, eval_ramp(&stops, t, 0)));
                                    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
                                    sel = stops.iter().position(|s| s.0 == t).unwrap_or(0);
                                    edited = true;
                                }
                            }
                            // Dès que le bouton s'enfonce : a-t-on attrapé une poignée ? Si oui, elle devient la couleur choisie
                            let grabbed_id = egui::Id::unique(("ramp-grab", *id));
                            let mut grabbed: Option<bool> = child.data(|d| d.get_temp(grabbed_id));
                            if grabbed.is_none() && resp.is_pointer_button_down_on() {
                                let near = stops.iter().enumerate().map(|(k, s)| (k, (x_of(s.0) - at.x).abs())).min_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
                                grabbed = Some(near.1 < 8.0);
                                if near.1 < 8.0 {
                                    sel = near.0;
                                }
                                child.data_mut(|d| d.insert_temp(grabbed_id, grabbed.unwrap()));
                            }
                            let grabbed = grabbed.unwrap_or(false);
                            if resp.dragged() && grabbed && sel < stops.len() {
                                let c = stops[sel].1;
                                stops[sel].0 = t_of(at.x);
                                stops.sort_by(|a, b| a.0.total_cmp(&b.0));
                                sel = stops.iter().position(|s| s.1 == c && (x_of(s.0) - at.x).abs() < 1.5).unwrap_or(sel.min(stops.len() - 1));
                                edited = true;
                            }
                        }
                        if !resp.is_pointer_button_down_on() {
                            child.data_mut(|d| d.remove::<bool>(egui::Id::unique(("ramp-grab", *id))));
                        }
                        for (k, st) in stops.iter().enumerate() {
                            let x = x_of(st.0);
                            let g = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
                            let tri = vec![pos2(x, bar.bottom() - 9.0), pos2(x - 5.0, bar.bottom() - 1.0), pos2(x + 5.0, bar.bottom() - 1.0)];
                            painter.add(egui::Shape::convex_polygon(tri, Color32::from_rgb(g(st.1[0]), g(st.1[1]), g(st.1[2])), Stroke::new(if k == sel { 2.0 } else { 1.0 }, if k == sel { p.accent } else { p.text })));
                        }
                        // Sous la barre : couleur et position de la poignée choisie, ajouter, retirer, inverser
                        let tools = Rect::from_min_max(pos2(row.left(), bar.bottom() + 8.0), row.max);
                        let mut tu = child.new_child(egui::UiBuilder::new().max_rect(tools).layout(egui::Layout::left_to_right(egui::Align::Center)));
                        tu.spacing_mut().item_spacing.x = 3.0;
                        let mut rgba = egui::Rgba::from_rgba_unmultiplied(stops[sel].1[0], stops[sel].1[1], stops[sel].1[2], stops[sel].1[3]);
                        if egui::color_picker::color_edit_button_rgba(&mut tu, &mut rgba, egui::color_picker::Alpha::OnlyBlend).changed() {
                            let a = rgba.a();
                            let un = if a > 1e-4 { [rgba.r() / a, rgba.g() / a, rgba.b() / a, a] } else { [0.0, 0.0, 0.0, 0.0] };
                            stops[sel].1 = un;
                            edited = true;
                        }
                        let mut pos = stops[sel].0;
                        tu.spacing_mut().interact_size.x = 34.0;
                        if tu.add(egui::DragValue::new(&mut pos).range(0.0..=1.0).speed(0.005).max_decimals(2)).on_hover_text(crate::i18n::t("position de la couleur choisie")).changed() {
                            let c = stops[sel].1;
                            stops[sel].0 = pos;
                            stops.sort_by(|a, b| a.0.total_cmp(&b.0));
                            sel = stops.iter().position(|s| s.1 == c && s.0 == pos).unwrap_or(sel);
                            edited = true;
                        }
                        if tu.small_button("＋").on_hover_text(crate::i18n::t("Ajouter une couleur (ou double clic sur le dégradé)")).clicked() {
                            let (a, b) = if sel + 1 < stops.len() { (stops[sel].0, stops[sel + 1].0) } else if sel > 0 { (stops[sel - 1].0, stops[sel].0) } else { (0.0, 1.0) };
                            let t = (a + b) * 0.5;
                            stops.push((t, eval_ramp(&stops, t, 0)));
                            stops.sort_by(|a, b| a.0.total_cmp(&b.0));
                            sel = stops.iter().position(|s| s.0 == t).unwrap_or(0);
                            edited = true;
                        }
                        if tu.add_enabled(stops.len() > 1, egui::Button::new("−").small()).on_hover_text(crate::i18n::t("Retirer la couleur choisie")).clicked() {
                            stops.remove(sel);
                            sel = sel.min(stops.len() - 1);
                            edited = true;
                        }
                        if tu.small_button("⇄").on_hover_text(crate::i18n::t("Inverser le dégradé")).clicked() {
                            stops.iter_mut().for_each(|s| s.0 = 1.0 - s.0);
                            stops.reverse();
                            sel = stops.len() - 1 - sel;
                            edited = true;
                        }
                        child.data_mut(|d| d.insert_temp(sel_id, sel));
                        if edited {
                            *text = ramp_string(&stops);
                            actions.push(Act::Changed);
                        }
                    }
                    (ParamDef::File(_), Param::S(rel)) => {
                        let video = node.op == "video";
                        let label = if rel.is_empty() { crate::i18n::t(if video { "choisir une vidéo…" } else { "choisir une image…" }) } else { rel.rsplit('/').next().unwrap_or(rel).to_string() };
                        if child.button(RichText::new(label).size(11.0)).on_hover_text(crate::i18n::t(if video { "une vidéo ou un montage (.aryseq)" } else { "ou glisse une image dans le graphe" })).clicked() {
                            let dlg = if video { rfd::FileDialog::new().add_filter("vidéo", &["mp4", "mov", "mkv", "webm", "avi", "m4v", "aryseq", "aryav"]) } else { rfd::FileDialog::new().add_filter("images", &["png", "jpg", "jpeg", "webp", "tga", "tif", "tiff", "bmp", "exr", "hdr"]) };
                            if let Some(f) = dlg.pick_file() {
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
                            ParamDef::Ramp(_) => Param::S(DEFAULT_RAMP.to_string()),
                            ParamDef::Curve(_) => Param::S(DEFAULT_CURVES.to_string()),
                            ParamDef::Line(_, v) => Param::S(v.to_string()),
                            _ => Param::S(String::new()),
                        };
                    }
                }
                y += h;
            }
            // Miniature
            if preview_on && has_out {
                let tr = Rect::from_min_size(pos2(r.left() + 8.0, r.bottom() - THUMB - 6.0), vec2(NW - 16.0, THUMB));
                if let Some(geo) = geo_view.as_mut() {
                    if d.outputs.first().map(|o| o.1) == Some(Sock::Geo) {
                        geo.thumb(&painter, tr, &self.graph, *id, self.version, &self.root);
                    } else {
                        painter.text(tr.center(), egui::Align2::CENTER_CENTER, crate::i18n::t("champ : une valeur par point"), egui::FontId::proportional(10.5), p.muted);
                    }
                    continue;
                }
                painter.rect_filled(tr, 0.0, Color32::from_gray(40));
                match self.thumbs.get(id) {
                    Some((_, t)) => {
                        let s = t.size_vec2();
                        let k = (tr.width() / s.x).min(tr.height() / s.y);
                        let ir = Rect::from_center_size(tr.center(), s * k);
                        painter.image(t.id(), ir, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        painter.text(tr.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(11.0), p.muted);
                    }
                }
            }
        }
        self.geo = geo_view;
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
                            if !layout.iter().any(|(_, r, _)| r.expand(6.0).contains(pt)) {
                                actions.push(Act::OpenAdd(pt, Some((from, out))));
                            }
                        }
                    }
                }
                self.drag_link = None;
            }
        }
    }

    /// Les matériaux en nodes du coffre (chemins relatifs), pour choisir le maître d'un calque.
    fn vault_materials(&mut self) -> Vec<String> {
        if let Some((t, v)) = &self.mat_list {
            if t.elapsed().as_secs() < 5 {
                return v.clone();
            }
        }
        let mut out = vec![];
        fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<String>, depth: u8) {
            let Ok(rd) = std::fs::read_dir(dir) else { return };
            for e in rd.flatten() {
                let p = e.path();
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                if p.is_dir() && depth < 6 {
                    walk(&p, root, out, depth + 1);
                } else if crate::formats::has(&name, "mat") {
                    if let Ok(r) = p.strip_prefix(root) {
                        out.push(r.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }
        let root = std::path::PathBuf::from(&self.root);
        walk(&root, &root, &mut out, 0);
        out.sort();
        // Pas soi-même ni d'autres matériaux à calques comme maître
        let me = self.path.strip_prefix(&root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
        out.retain(|r| *r != me && crate::atelier::load_master(&self.root, r).is_some_and(|g| g.stack.is_none()));
        self.mat_list = Some((std::time::Instant::now(), out.clone()));
        out
    }

    /// Atelier (façon Substance Painter) : l'aperçu au centre, la pile de calques à droite.
    /// Double-clic sur un calque : son matériau maître s'ouvre dans l'éditeur de nodes (façon Designer).
    fn atelier_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let p = theme::pal();
        let mut changed = false;
        let mats = self.vault_materials();
        let me = self.path.strip_prefix(&self.root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
        let masters: Vec<String> = mats.into_iter().filter(|m| *m != me && crate::atelier::load_master(&self.root, m).is_some_and(|g| g.stack.is_none())).collect();
        let smart = crate::atelier::smart_materials(&self.root);
        let Some(stack) = self.graph.stack.clone() else { return };
        let mut stack = stack;
        // Les vignettes : un fil calcule celles qui manquent (une sphère par matériau)
        self.atelier_thumbs(ctx, &masters, &smart);

        let avail = ui.available_rect_before_wrap();
        // La bibliothèque et la pile de calques peuvent sortir dans leur fenêtre : l'aperçu prend alors leur place
        let lib_away = crate::layout::offer("atelier-lib", "matériaux (atelier)").is_some();
        let panel_away = crate::layout::offer("atelier-layers", "calques et propriétés (atelier)").is_some();
        let left_w = if lib_away { 0.0 } else { 236.0f32.min(avail.width() * 0.24) };
        let right_w = if panel_away { 0.0 } else { 330.0f32.min(avail.width() * 0.34) };
        let lib = Rect::from_min_max(avail.min, pos2(avail.min.x + left_w, avail.max.y));
        let panel = Rect::from_min_max(pos2(avail.max.x - right_w, avail.min.y), avail.max);
        let prev = Rect::from_min_max(pos2(lib.right() + 1.0, avail.min.y), pos2(panel.left() - 1.0, avail.max.y));
        self.preview_ui(ui, prev, ctx);
        if let Err((_, e)) = &self.program {
            ui.painter().text(prev.left_bottom() + vec2(12.0, -14.0), egui::Align2::LEFT_BOTTOM, format!("⚠ {e}"), egui::FontId::proportional(11.5), p.missing);
        }
        if let Some(m) = &self.message {
            ui.painter().text(prev.left_top() + vec2(12.0, 12.0), egui::Align2::LEFT_TOP, m, egui::FontId::proportional(11.5), p.muted);
        }
        let head = |ui: &mut egui::Ui, title: &str| {
            ui.label(RichText::new(crate::i18n::t(title)).size(11.0).strong().color(p.muted));
        };

        // ---------- Bibliothèque (à gauche) ----------
        ui.painter().vline(lib.right(), lib.y_range(), egui::Stroke::new(1.0, p.line));
        let mut add_master: Option<String> = None;
        let mut add_smart: Option<String> = None;
        let mut add_preset: Option<&'static str> = None;
        let mut new_master = false;
        crate::dock::area(ui, "atelier-lib", "matériaux (atelier)", lib, p.panel, 10.0, |lui| {
            head(lui, "MATÉRIAUX");
            lui.add_space(4.0);
            lui.add(egui::TextEdit::singleline(&mut self.lib_search).hint_text(format!("{}  {}", crate::lucide::SEARCH, crate::i18n::t("chercher"))).desired_width(f32::INFINITY));
            lui.add_space(4.0);
            lui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (k, (icon, tip)) in [(crate::lucide::LAYERS, "tout"), (crate::lucide::BLEND, "matériaux maîtres du coffre"), (crate::lucide::SPARKLES, "matériaux intelligents"), (crate::lucide::PALETTE, "préréglages")].into_iter().enumerate() {
                    let on = self.lib_shelf == k as u8;
                    let b = egui::Button::new(RichText::new(icon).size(13.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { Color32::TRANSPARENT }).corner_radius(10).min_size(vec2(30.0, 24.0));
                    if ui.add(b).on_hover_text(crate::i18n::t(tip)).clicked() {
                        self.lib_shelf = k as u8;
                    }
                }
            });
            lui.add_space(6.0);
            let q = self.lib_search.to_lowercase();
            let fits = |name: &str| q.is_empty() || name.to_lowercase().contains(&q);
            // (clé de vignette, nom, genre)
            let mut items: Vec<(String, String, u8)> = vec![];
            if self.lib_shelf == 0 || self.lib_shelf == 1 {
                for m in &masters {
                    let name = std::path::Path::new(m).file_name().map(|s| crate::formats::strip(&s.to_string_lossy()).to_string()).unwrap_or_default();
                    if fits(&name) {
                        items.push((format!("m:{m}"), name, 1));
                    }
                }
            }
            if self.lib_shelf == 0 || self.lib_shelf == 2 {
                for n in &smart {
                    if fits(n) {
                        items.push((format!("s:{n}"), n.clone(), 2));
                    }
                }
            }
            if self.lib_shelf == 0 || self.lib_shelf == 3 {
                for n in MATERIAL_PRESETS {
                    if fits(n) {
                        items.push((format!("p:{n}"), crate::i18n::t(n), 3));
                    }
                }
            }
            egui::ScrollArea::vertical().id_salt("atelier-lib").auto_shrink([false, false]).show(lui, |ui| {
                let cols = ((ui.available_width() + 6.0) / 70.0).floor().max(2.0) as usize;
                let cell = (ui.available_width() - 6.0 * (cols as f32 - 1.0)) / cols as f32;
                egui::Grid::new("atelier-lib-grid").spacing(vec2(6.0, 8.0)).show(ui, |ui| {
                    for (k, (key, name, kind)) in items.iter().enumerate() {
                        let (r, resp) = ui.allocate_exact_size(vec2(cell, cell + 16.0), Sense::click());
                        let disc = Rect::from_center_size(pos2(r.center().x, r.top() + cell / 2.0), vec2(cell - 6.0, cell - 6.0));
                        if resp.hovered() {
                            ui.painter().rect_filled(r.expand(2.0), 6.0, p.raised);
                        }
                        match self.mthumb.get(key) {
                            Some(t) => {
                                ui.painter().image(t.id(), disc, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                            }
                            None => {
                                ui.painter().circle_filled(disc.center(), disc.width() / 2.0 - 4.0, p.field);
                            }
                        }
                        // Le genre, en petit badge : intelligent ou préréglage
                        if *kind != 1 {
                            let icon = if *kind == 2 { crate::lucide::SPARKLES } else { crate::lucide::PALETTE };
                            ui.painter().text(disc.right_bottom() - vec2(4.0, 4.0), egui::Align2::RIGHT_BOTTOM, icon, egui::FontId::proportional(11.0), p.accent);
                        }
                        let short: String = if name.chars().count() > 11 { format!("{}…", name.chars().take(10).collect::<String>()) } else { name.clone() };
                        ui.painter().text(pos2(r.center().x, r.bottom() - 7.0), egui::Align2::CENTER_CENTER, short, egui::FontId::proportional(10.5), p.text);
                        let tip = match kind {
                            1 => crate::i18n::t("Ajouter en calque (double-clic sur le calque : ouvrir le maître)"),
                            2 => crate::i18n::t("Matériau intelligent : ajoute tous ses calques, avec leurs masques"),
                            _ => crate::i18n::t("Préréglage : devient un matériau maître du coffre, puis un calque"),
                        };
                        if resp.on_hover_text(format!("{name}\n{tip}")).clicked() {
                            match kind {
                                1 => add_master = Some(key[2..].to_string()),
                                2 => add_smart = Some(key[2..].to_string()),
                                _ => add_preset = MATERIAL_PRESETS.iter().copied().find(|n| *n == &key[2..]),
                            }
                        }
                        if (k + 1) % cols == 0 {
                            ui.end_row();
                        }
                    }
                });
                ui.add_space(8.0);
                if ui.add(egui::Button::new(format!("{}  {}", crate::lucide::PLUS, crate::i18n::t("nouveau matériau maître"))).corner_radius(10)).on_hover_text(crate::i18n::t("Crée un .arymat à côté, l'ajoute comme calque et l'ouvre en nodes")).clicked() {
                    new_master = true;
                }
            });
        });
        if let Some(m) = add_master {
            let name = std::path::Path::new(&m).file_name().map(|s| crate::formats::strip(&s.to_string_lossy()).to_string()).unwrap_or_default();
            stack.layers.push(crate::atelier::Layer { name, master: m, params: Default::default(), mask: Default::default(), opacity: 1.0, visible: true });
            self.layer_sel = Some(stack.layers.len() - 1);
            changed = true;
        }
        if let Some(n) = add_smart {
            if let Some(st) = crate::atelier::load_smart(&self.root, &n) {
                stack.layers.extend(st.layers);
                self.layer_sel = Some(stack.layers.len().saturating_sub(1));
                changed = true;
            }
        }
        if let Some(n) = add_preset {
            // Le préréglage devient un maître du coffre (« nodes/matériaux »), réutilisé s'il existe déjà
            let dir = std::path::Path::new(&self.root).join("nodes").join("matériaux");
            let _ = std::fs::create_dir_all(&dir);
            let file = dir.join(format!("{n}.{}", crate::formats::ext("mat")));
            let ok = file.exists() || material_preset(n).is_some_and(|g| g.save(&file).is_ok());
            if ok {
                let rel = file.strip_prefix(&self.root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
                stack.layers.push(crate::atelier::Layer { name: crate::i18n::t(n), master: rel, params: Default::default(), mask: Default::default(), opacity: 1.0, visible: true });
                self.layer_sel = Some(stack.layers.len() - 1);
                self.mat_list = None;
                changed = true;
            }
        }
        if new_master {
            // Un atelier pas encore enregistré : le maître va dans « nodes/ » du coffre (pas dans le dossier caché)
            let untitled = self.path.strip_prefix(&self.root).is_ok_and(|r| r.to_string_lossy().replace('\\', "/").starts_with(crate::untitled::DIR));
            let dir = if untitled { std::path::Path::new(&self.root).join("nodes") } else { self.path.parent().unwrap_or(std::path::Path::new("")).to_path_buf() };
            let _ = std::fs::create_dir_all(&dir);
            let stem = self.path.file_stem().map(|s| crate::formats::strip(&s.to_string_lossy()).to_string()).unwrap_or_else(|| "matériau".into());
            let mut n = stack.layers.len() + 1;
            let mut file = dir.join(format!("{stem} - calque {n}.{}", crate::formats::ext("mat")));
            while file.exists() {
                n += 1;
                file = dir.join(format!("{stem} - calque {n}.{}", crate::formats::ext("mat")));
            }
            if Graph::new_material().save(&file).is_ok() {
                let rel = file.strip_prefix(&self.root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| file.to_string_lossy().to_string());
                stack.layers.push(crate::atelier::Layer { name: format!("calque {n}"), master: rel, params: Default::default(), mask: Default::default(), opacity: 1.0, visible: true });
                self.layer_sel = Some(stack.layers.len() - 1);
                self.open_request = Some(file);
                self.mat_list = None;
                changed = true;
            }
        }

        // ---------- Calques et propriétés (à droite) ----------
        ui.painter().vline(panel.left(), panel.y_range(), egui::Stroke::new(1.0, p.line));
        crate::dock::area(ui, "atelier-layers", "calques et propriétés (atelier)", panel, p.panel, 10.0, |ui| {
        let n = stack.layers.len();
        let (mut del, mut up, mut down, mut dup) = (None, None, None, None);
        // En-tête des calques : le titre, et les actions en icônes (comme Painter)
        ui.horizontal(|ui| {
            head(ui, "CALQUES");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let ib = |ui: &mut egui::Ui, icon: &str, tip: &str, ok: bool| ui.add_enabled(ok, egui::Button::new(RichText::new(icon).size(14.0)).frame(false).min_size(vec2(24.0, 22.0))).on_hover_text(crate::i18n::t(tip)).clicked();
                let sel = self.layer_sel.filter(|&i| i < n);
                if ib(ui, crate::lucide::TRASH_2, "supprimer le calque", sel.is_some()) {
                    del = sel;
                }
                if ib(ui, crate::lucide::COPY, "dupliquer le calque", sel.is_some()) {
                    dup = sel;
                }
                if ib(ui, crate::lucide::ARROW_DOWN, "descendre", sel.is_some_and(|i| i > 0)) {
                    down = sel;
                }
                if ib(ui, crate::lucide::ARROW_UP, "monter", sel.is_some_and(|i| i + 1 < n)) {
                    up = sel;
                }
                if ib(ui, crate::lucide::SPARKLES, "Enregistrer la pile comme matériau intelligent (dans « matériaux intelligents » du coffre)", n > 0) {
                    let name = self.path.file_stem().map(|s| crate::formats::strip(&s.to_string_lossy()).to_string()).unwrap_or_else(|| "matériau".into());
                    self.message = Some(match crate::atelier::save_smart(&self.root, &name, &stack) {
                        Ok(_) => format!("{} « {name} »", crate::i18n::t("Matériau intelligent enregistré :")),
                        Err(e) => e,
                    });
                    self.mthumb.remove(&format!("s:{name}"));
                }
            });
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical().id_salt("atelier-layers").max_height((panel.height() * 0.38).max(140.0)).show(ui, |ui| {
            if n == 0 {
                ui.label(RichText::new(crate::i18n::t("Clique un matériau à gauche : il devient un calque, posé partout ou selon un masque (usure, saleté, poussière…).")).color(p.muted).size(11.0));
            }
            // Le calque du dessus en haut, comme dans Painter
            for i in (0..n).rev() {
                let l = &mut stack.layers[i];
                let on = self.layer_sel == Some(i);
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
                ui.painter().rect(r, 6.0, if on { p.accent.gamma_multiply(0.16) } else if resp.hovered() { p.raised } else { p.field }, egui::Stroke::new(1.0, if on { p.accent } else { p.line }), egui::StrokeKind::Inside);
                let eye = Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(18.0, 18.0));
                ui.painter().text(eye.center(), egui::Align2::CENTER_CENTER, if l.visible { crate::lucide::EYE } else { crate::lucide::EYE_OFF }, egui::FontId::proportional(13.0), if l.visible { p.text } else { p.muted });
                // Vignette du matériau, puis celle du masque (comme les deux pastilles de Painter)
                let th = Rect::from_center_size(pos2(r.left() + 46.0, r.center().y), vec2(30.0, 30.0));
                match self.mthumb.get(&format!("m:{}", l.master)) {
                    Some(t) => {
                        ui.painter().image(t.id(), th, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        ui.painter().circle_filled(th.center(), 13.0, p.raised);
                    }
                }
                let masked = l.mask.kind != "plein";
                let mk = Rect::from_center_size(pos2(r.left() + 78.0, r.center().y), vec2(22.0, 22.0));
                ui.painter().rect(mk, 3.0, if masked { Color32::from_gray(200) } else { p.raised }, egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
                if masked {
                    ui.painter().text(mk.center(), egui::Align2::CENTER_CENTER, crate::lucide::WAND_SPARKLES, egui::FontId::proportional(11.0), Color32::from_gray(40));
                }
                ui.painter().text(pos2(r.left() + 96.0, r.center().y - 7.0), egui::Align2::LEFT_CENTER, &l.name, egui::FontId::proportional(12.5), if l.visible { p.text } else { p.muted });
                let mask = crate::atelier::MASKS.iter().find(|m| m.0 == l.mask.kind).map(|m| m.1).unwrap_or("");
                ui.painter().text(pos2(r.left() + 96.0, r.center().y + 8.0), egui::Align2::LEFT_CENTER, crate::i18n::t(mask), egui::FontId::proportional(10.5), p.muted);
                ui.painter().text(pos2(r.right() - 10.0, r.center().y), egui::Align2::RIGHT_CENTER, format!("{:.0}", l.opacity * 100.0), egui::FontId::monospace(11.0), p.muted);
                if resp.clicked() {
                    if ui.input(|i| i.pointer.interact_pos()).is_some_and(|at| eye.expand(4.0).contains(at)) {
                        l.visible = !l.visible;
                        changed = true;
                    } else {
                        self.layer_sel = Some(i);
                    }
                }
                if resp.double_clicked() {
                    self.open_request = Some(std::path::Path::new(&self.root).join(&l.master));
                }
                resp.on_hover_text(crate::i18n::t("double-clic : ouvrir le matériau maître en nodes"));
                ui.add_space(3.0);
            }
        });
        if let Some(i) = up.filter(|&i| i + 1 < n) {
            stack.layers.swap(i, i + 1);
            self.layer_sel = Some(i + 1);
            changed = true;
        }
        if let Some(i) = down.filter(|&i| i > 0) {
            stack.layers.swap(i, i - 1);
            self.layer_sel = Some(i - 1);
            changed = true;
        }
        if let Some(i) = dup {
            let mut c = stack.layers[i].clone();
            c.name = format!("{} {}", c.name, crate::i18n::t("(copie)"));
            stack.layers.insert(i + 1, c);
            self.layer_sel = Some(i + 1);
            changed = true;
        }
        if let Some(i) = del {
            stack.layers.remove(i);
            self.layer_sel = None;
            changed = true;
        }
        // ---------- Propriétés du calque choisi : des cartes, libellés à gauche ----------
        if let Some(i) = self.layer_sel.filter(|&i| i < stack.layers.len()) {
            ui.add_space(8.0);
            ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), egui::Stroke::new(1.0, p.line));
            ui.add_space(8.0);
            head(ui, "PROPRIÉTÉS");
            ui.add_space(4.0);
            let l = &mut stack.layers[i];
            let card = |ui: &mut egui::Ui, title: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
                egui::Frame::new().fill(p.field).stroke(egui::Stroke::new(1.0, p.line)).corner_radius(8).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing = vec2(6.0, 8.0);
                    ui.label(RichText::new(crate::i18n::t(title)).strong().size(12.0));
                    body(ui);
                });
                ui.add_space(8.0);
            };
            egui::ScrollArea::vertical().id_salt("atelier-props").auto_shrink([false, false]).show(ui, |ui| {
                card(ui, "Calque", &mut |ui| {
                    crate::modifiers::row(ui, &crate::i18n::t("nom"), |ui| {
                        changed |= ui.add(egui::TextEdit::singleline(&mut l.name).desired_width(ui.available_width() - 30.0)).changed();
                        if ui.add(egui::Button::new(RichText::new(crate::lucide::WORKFLOW).size(13.0)).frame(false)).on_hover_text(format!("{}\n{}", crate::i18n::t("ouvrir le maître en nodes"), l.master)).clicked() {
                            self.open_request = Some(std::path::Path::new(&self.root).join(&l.master));
                        }
                    });
                    let mut o = l.opacity * 100.0;
                    if crate::modifiers::row_slider(ui, "opacité", egui::Slider::new(&mut o, 0.0..=100.0).suffix(" %").max_decimals(0)) {
                        l.opacity = o / 100.0;
                        changed = true;
                    }
                });
                let ex = crate::atelier::load_master(&self.root, &l.master).map(|g| crate::atelier::exposed(&g)).unwrap_or_default();
                card(ui, "Paramètres", &mut |ui| {
                    if ex.is_empty() {
                        ui.label(RichText::new(crate::i18n::t("Ce maître n'expose aucun paramètre. Dans ses nodes, ajoute un « Paramètre exposé » (et renomme-le) pour le régler d'ici.")).color(p.muted).size(10.5));
                    }
                    for e in &ex {
                        let mut v = l.params.get(&e.name).copied().unwrap_or(e.default);
                        let overridden = l.params.contains_key(&e.name);
                        let ch = crate::modifiers::row(ui, &e.name, |ui| {
                            let r = if e.color {
                                let mut c = [v[0], v[1], v[2]];
                                let r = ui.color_edit_button_rgb(&mut c).changed();
                                v = [c[0], c[1], c[2], 1.0];
                                r
                            } else {
                                ui.spacing_mut().slider_width = (ui.available_width() - 70.0).max(60.0);
                                ui.add(egui::Slider::new(&mut v[0], 0.0..=1.0)).changed()
                            };
                            if overridden && ui.add(egui::Button::new(RichText::new(crate::lucide::ROTATE_CCW).size(12.0)).frame(false)).on_hover_text(crate::i18n::t("revenir à la valeur du maître")).clicked() {
                                l.params.remove(&e.name);
                                changed = true;
                            }
                            r
                        });
                        if ch {
                            l.params.insert(e.name.clone(), v);
                            changed = true;
                        }
                    }
                });
                card(ui, "Masque", &mut |ui| {
                    // Les masques en pastilles, comme les générateurs de Painter
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                        for (id, label, _) in crate::atelier::MASKS {
                            let on = l.mask.kind == id;
                            let b = egui::Button::new(RichText::new(crate::i18n::t(label)).size(11.5).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { Color32::TRANSPARENT }).stroke(egui::Stroke::new(1.0, p.line)).corner_radius(10);
                            if ui.add(b).clicked() {
                                l.mask.kind = id.to_string();
                                changed = true;
                            }
                        }
                    });
                    egui::containers::menu::MenuButton::new(RichText::new(format!("{}  {}", crate::lucide::WAND_SPARKLES, crate::i18n::t("masques intelligents"))).size(12.0)).ui(ui, |ui| {
                        ui.set_min_width(240.0);
                        for (name, m) in crate::atelier::smart_masks() {
                            if ui.button(crate::i18n::t(name)).clicked() {
                                l.mask = m;
                                changed = true;
                                ui.close();
                            }
                        }
                    });
                    if l.mask.kind == "peint" {
                        // Peint à la main sur l'objet : on choisit le canal, la peinture se fait dans la scène
                        crate::modifiers::row(ui, &crate::i18n::t("canal"), |ui| {
                            for k in 0..4u8 {
                                let on = l.mask.paint == k;
                                let b = egui::Button::new(RichText::new(format!("{}", k + 1)).size(11.5).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { Color32::TRANSPARENT }).min_size(vec2(26.0, 20.0));
                                if ui.add(b).on_hover_text(format!("{} {}", crate::i18n::t("calque de peinture « masque »"), k + 1)).clicked() {
                                    l.mask.paint = k;
                                    changed = true;
                                }
                            }
                        });
                        ui.label(RichText::new(crate::i18n::t("Dans la scène, en mode peinture : ajoute à l'objet un calque « masque » de ce canal, et peins où ce calque doit apparaître.")).size(10.5).color(p.muted));
                    }
                    if l.mask.kind != "plein" {
                        if l.mask.kind != "peint" {
                            changed |= crate::modifiers::row_slider(ui, "quantité", egui::Slider::new(&mut l.mask.amount, 0.0..=1.0));
                            changed |= crate::modifiers::row_slider(ui, "contraste", egui::Slider::new(&mut l.mask.contrast, 0.0..=1.0));
                            changed |= crate::modifiers::row_slider(ui, "échelle", egui::Slider::new(&mut l.mask.scale, 0.5..=40.0).clamping(egui::SliderClamping::Never).logarithmic(true));
                        }
                        changed |= ui.checkbox(&mut l.mask.invert, crate::i18n::t("inverser")).changed();
                        // Cassure : un second motif découpe le masque, pour un résultat moins régulier
                        crate::modifiers::row(ui, &crate::i18n::t("cassure"), |ui| {
                            let cur = if l.mask.breakup.is_empty() { "aucune" } else { crate::atelier::MASKS.iter().find(|m| m.0 == l.mask.breakup).map(|m| m.1).unwrap_or("aucune") };
                            egui::ComboBox::from_id_salt("atelier-breakup").width(ui.available_width() - 4.0).selected_text(crate::i18n::t(cur)).show_ui(ui, |ui| {
                                if ui.selectable_label(l.mask.breakup.is_empty(), crate::i18n::t("aucune")).clicked() {
                                    l.mask.breakup.clear();
                                    changed = true;
                                }
                                for (id, label, op) in crate::atelier::MASKS {
                                    if !op.is_empty() && ui.selectable_label(l.mask.breakup == id, crate::i18n::t(label)).clicked() {
                                        l.mask.breakup = id.to_string();
                                        changed = true;
                                    }
                                }
                            });
                        });
                        if !l.mask.breakup.is_empty() {
                            changed |= crate::modifiers::row_slider(ui, "force", egui::Slider::new(&mut l.mask.breakup_amount, 0.0..=1.0));
                            changed |= crate::modifiers::row_slider(ui, "échelle cassure", egui::Slider::new(&mut l.mask.breakup_scale, 0.5..=40.0).logarithmic(true));
                        }
                    }
                });
            });
        }
        });
        if changed || self.graph.stack.as_ref() != Some(&stack) {
            self.graph.stack = Some(stack);
            self.changed();
        }
        // Un maître modifié dans un autre onglet : l'aperçu suit
        let key = self.graph.stack.as_ref().map(|s| s.layers.iter().map(|l| std::fs::metadata(std::path::Path::new(&self.root).join(&l.master)).and_then(|m| m.modified()).ok()).collect::<Vec<_>>());
        let h = {
            use std::hash::{Hash, Hasher};
            let mut hh = std::collections::hash_map::DefaultHasher::new();
            key.hash(&mut hh);
            hh.finish()
        };
        if self.masters_seen != h {
            self.masters_seen = h;
            self.program = Program::compile(&self.graph.effective(), &self.root, 1024);
            self.version += 1;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }

    /// Les vignettes de la bibliothèque de l'atelier : une sphère rendue par matériau, en arrière-plan
    fn atelier_thumbs(&mut self, ctx: &egui::Context, masters: &[String], smart: &[String]) {
        if let Some(rx) = &self.mthumb_rx {
            let mut done = false;
            loop {
                match rx.try_recv() {
                    Ok((key, img)) => {
                        let ci = to_texture(ctx, "mthumb", &img);
                        self.mthumb.insert(key.clone(), ctx.load_texture(format!("mthumb-{key}"), ci, egui::TextureOptions::LINEAR));
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(_) => {
                        done = true;
                        break;
                    }
                }
            }
            if done {
                self.mthumb_rx = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
                return;
            }
        }
        let mut todo: Vec<(String, Graph)> = vec![];
        for m in masters {
            let k = format!("m:{m}");
            if !self.mthumb.contains_key(&k) {
                if let Some(g) = crate::atelier::load_master(&self.root, m) {
                    todo.push((k, g));
                }
            }
        }
        for n in smart {
            let k = format!("s:{n}");
            if !self.mthumb.contains_key(&k) {
                if let Some(st) = crate::atelier::load_smart(&self.root, n) {
                    todo.push((k, crate::atelier::flatten(&st, &self.root)));
                }
            }
        }
        for n in MATERIAL_PRESETS {
            let k = format!("p:{n}");
            if !self.mthumb.contains_key(&k) {
                if let Some(g) = material_preset(n) {
                    todo.push((k, g));
                }
            }
        }
        if todo.is_empty() {
            return;
        }
        let root = self.root.clone();
        let sphere = self.sphere.get_or_insert_with(|| std::sync::Arc::new(crate::model3d::preview_sphere())).clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let c2 = ctx.clone();
        std::thread::spawn(move || {
            for (k, g) in todo {
                let Ok(prog) = Program::compile(&g, &root, 256) else { continue };
                let img = crate::model3d::render_material_thumb(&sphere, std::sync::Arc::new(prog), 96);
                if tx.send((k, img)).is_err() {
                    return;
                }
                c2.request_repaint();
            }
        });
        self.mthumb_rx = Some(rx);
    }

    /// Tracé roto du node choisi, sur l'aperçu : clic pour ajouter un point, glisser pour le déplacer,
    /// clic droit pour le retirer.
    fn roto_ui(&mut self, ui: &mut egui::Ui, r: Rect) {
        if self.graph.kind != GraphKind::Compositor || self.selected.len() != 1 {
            return;
        }
        let id = *self.selected.iter().next().unwrap();
        let Some(ni) = self.graph.nodes.iter().position(|n| n.id == id && n.op == "roto") else { return };
        let mut pts = parse_points(pstr(&self.graph.nodes[ni].params, "points"));
        let to_screen = |p: [f32; 2]| pos2(r.left() + p[0] * r.width(), r.top() + p[1] * r.height());
        let to_uv = |q: Pos2| [((q.x - r.left()) / r.width()).clamp(0.0, 1.0), ((q.y - r.top()) / r.height()).clamp(0.0, 1.0)];
        let resp = ui.interact(r, egui::Id::unique(("roto", id)), egui::Sense::click_and_drag());
        let near = |q: Pos2, pts: &[[f32; 2]]| pts.iter().position(|p| (to_screen(*p) - q).length() < 8.0);
        let mut moved = false;
        if resp.drag_started() {
            self.roto_drag = resp.interact_pointer_pos().and_then(|q| near(q, &pts));
        }
        if resp.dragged() {
            if let (Some(k), Some(q)) = (self.roto_drag, resp.interact_pointer_pos()) {
                if k < pts.len() {
                    pts[k] = to_uv(q);
                    moved = true;
                }
            }
        }
        if resp.drag_stopped() {
            self.roto_drag = None;
        }
        if resp.clicked() {
            if let Some(q) = resp.interact_pointer_pos() {
                if near(q, &pts).is_none() {
                    // Le nouveau point s'insère dans le côté le plus proche (à la fin s'il y en a moins de trois)
                    let at = if pts.len() < 3 {
                        pts.len()
                    } else {
                        let u = to_uv(q);
                        let n = pts.len();
                        (0..n).min_by(|&a, &b| {
                            let d = |k: usize| poly_distance(&[pts[k], pts[(k + 1) % n]], u);
                            d(a).total_cmp(&d(b))
                        }).map(|k| k + 1).unwrap_or(n)
                    };
                    pts.insert(at, to_uv(q));
                    moved = true;
                }
            }
        }
        if resp.secondary_clicked() {
            if let Some(k) = resp.interact_pointer_pos().and_then(|q| near(q, &pts)) {
                pts.remove(k);
                moved = true;
            }
        }
        // Le tracé, ses points
        let col = Color32::from_rgb(255, 200, 60);
        let sp: Vec<Pos2> = pts.iter().map(|p| to_screen(*p)).collect();
        if sp.len() >= 2 {
            let mut line = sp.clone();
            if sp.len() >= 3 {
                line.push(sp[0]);
            }
            ui.painter().add(egui::Shape::line(line, Stroke::new(1.5, col)));
        }
        for q in &sp {
            ui.painter().circle_filled(*q, 4.0, col);
            ui.painter().circle_stroke(*q, 4.0, Stroke::new(1.0, Color32::BLACK));
        }
        if pts.is_empty() {
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, crate::i18n::t("cliquer pour poser les points du tracé"), egui::FontId::proportional(12.0), col);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if moved {
            self.graph.nodes[ni].params.insert("points".into(), Param::S(format_points(&pts)));
            self.changed();
        }
    }

    fn preview_ui(&mut self, ui: &mut egui::Ui, rect: Rect, ctx: &egui::Context) {
        let p = theme::pal();
        if self.graph.kind == GraphKind::Geometry {
            self.geo_preview(ui, rect, ctx);
            return;
        }
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
                let mut dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
                // Ou glissé depuis l'explorateur
                if let Some(rel) = crate::drop_of::<String>(&resp) {
                    dropped.push(Path::new(&self.root).join(rel.as_str()));
                }
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
            let solo_prog = solo.and_then(|id| self.graph.probe(id, 0)).and_then(|g| Program::compile_at(&g, &self.root, 1024, self.frame, 24.0).ok());
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
                    GraphKind::Geometry => {}
                    GraphKind::Material => {
                        let model = match &self.pv_model {
                            Some((_, m)) => m.clone(),
                            None if self.pv_shape.unwrap_or(if self.graph.stack.is_some() { 1 } else { 0 }) == 1 => self.boxy.get_or_insert_with(|| std::sync::Arc::new(crate::model3d::preview_box())).clone(),
                            None => self.sphere.get_or_insert_with(|| std::sync::Arc::new(crate::model3d::preview_sphere())).clone(),
                        };
                        // Le cube a des coins plus loin du centre que la sphère : un peu plus de recul pour le voir en entier
                        let boxed = self.pv_model.is_none() && self.pv_shape.unwrap_or(if self.graph.stack.is_some() { 1 } else { 0 }) == 1;
                        let (yaw, pitch, dist, env) = (self.pv_yaw, self.pv_pitch, self.pv_dist * if boxed { 1.6 } else { 1.0 }, self.pv_env);
                        std::thread::spawn(move || {
                            let img = crate::model3d::render_material_view(&model, std::sync::Arc::new(prog), 480, yaw, pitch, dist, env);
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
            self.roto_ui(ui, r);
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
                let shape = self.pv_shape.unwrap_or(if self.graph.stack.is_some() { 1 } else { 0 });
                for (k, l, tip) in [(0u8, "sphère", "aperçu sur une sphère"), (1, "cube", "aperçu sur un cube aux arêtes adoucies : on y voit l'usure des arêtes et la saleté des creux")] {
                    if ui.selectable_label(self.pv_model.is_none() && shape == k, RichText::new(crate::i18n::t(l)).size(10.5)).on_hover_text(crate::i18n::t(tip)).clicked() {
                        self.pv_model = None;
                        self.pv_shape = Some(k);
                        self.pv_key = 0;
                    }
                }
                match &self.pv_model {
                    Some((name, _)) => {
                        ui.label(RichText::new(name).size(10.5));
                    }
                    None => {
                        ui.label(RichText::new(crate::i18n::t("ou dépose un modèle 3D ici")).color(p.muted).size(10.5));
                    }
                }
                if let Some(id) = self.solo {
                    let name = self.graph.nodes.iter().find(|n| n.id == id).map(|n| if n.label.is_empty() { n.def().map(|d| crate::i18n::t(d.label)).unwrap_or_default() } else { n.label.clone() }).unwrap_or_default();
                    if ui.small_button(format!("{} : {name} ×", crate::i18n::t("node"))).on_hover_text(crate::i18n::t("revenir au matériau complet")).clicked() {
                        self.solo = None;
                        self.pv_key = 0;
                    }
                }
            });
        }
    }

    /// Géométrie : l'aperçu 3D, et dessous la barre de lecture avec les images en cache.
    fn geo_preview(&mut self, ui: &mut egui::Ui, rect: Rect, ctx: &egui::Context) {
        let Some(mut geo) = self.geo.take() else { return };
        geo.solo = self.solo.filter(|id| self.level.is_none() && self.graph.nodes.iter().any(|n| n.id == *id));
        // Espace : lire ou mettre en pause (souris sur l'aperçu ou sur le graphe, hors saisie)
        if !ctx.egui_wants_keyboard_input() && self.add.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Space) && !i.modifiers.any()) {
            geo.playing = !geo.playing;
        }
        let doc = self.doc();
        geo.tick(&doc, self.version, &self.root, ctx);
        // Une erreur du moteur se montre sur le node en cause, comme les autres
        match (&geo.error, &self.program) {
            (Some(e), _) => self.program = Err(e.clone()),
            (None, Err(_)) => self.program = crate::nodeflow::lower(&doc, false).map(|_| Program::empty(GraphKind::Geometry)),
            _ => {}
        }
        geo.viewport(ui, rect);
        let mut time = doc.time.unwrap_or_default();
        let bar = Rect::from_min_size(rect.left_bottom() + vec2(0.0, 4.0), vec2(rect.width(), 36.0));
        if geo.transport(ui, bar, &mut time) {
            self.graph.time = Some(time);
            self.geo = Some(geo);
            self.changed();
            return;
        }
        self.geo = Some(geo);
    }

    /// Fige sur le disque la simulation de la zone choisie, ou la libère.
    fn bake_zone(&mut self, end: u32, on: bool) {
        let Some(mut geo) = self.geo.take() else { return };
        if on {
            let stem = self.path.file_stem().map(|s| crate::formats::strip(&s.to_string_lossy()).to_string()).unwrap_or_else(|| "géométrie".into());
            let rel = format!(".cache/{stem}/zone-{end}");
            let dir = Path::new(&self.root).join(&rel);
            let doc = self.doc();
            match geo.engine.bake(&doc, end, &dir, &self.root) {
                Ok(count) => {
                    if let Some(n) = self.graph.nodes.iter_mut().find(|n| n.id == end) {
                        n.params.insert("cache".into(), Param::S(rel));
                    }
                    self.message = Some(format!("{count} {}", crate::i18n::t_mid("images figées sur le disque. La zone ne se recalcule plus tant qu'elle n'est pas libérée.")));
                }
                Err(e) => self.message = Some(e),
            }
        } else if let Some(n) = self.graph.nodes.iter_mut().find(|n| n.id == end) {
            if let Some(Param::S(rel)) = n.params.remove("cache") {
                let dir = Path::new(&self.root).join(&rel);
                // On ne supprime que notre propre dossier de cache
                if rel.starts_with(".cache/") && dir.is_dir() {
                    let _ = std::fs::remove_dir_all(dir);
                }
            }
            geo.engine.baked.remove(&end);
        }
        geo.refresh();
        self.geo = Some(geo);
        self.changed();
    }

    fn export(&mut self) {
        let prog = match &self.program {
            Ok(_) => Program::compile(&self.doc().effective(), &self.root, 16384),
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
                let stem = self.path.file_name().map(|n| n.to_string_lossy().to_string()).map(|n| crate::formats::strip(&n).to_string()).unwrap_or_else(|| "composite".into());
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
/// Deux prises vont-elles ensemble ? Tout se convertit, sauf la géométrie, qui ne va qu'avec elle-même.
pub fn socks_fit(a: Sock, b: Sock) -> bool {
    a == Sock::Any || b == Sock::Any || (a == Sock::Geo) == (b == Sock::Geo)
}

/// Note d'un texte face aux mots cherchés : début de mot, milieu de mot, puis lettres dans l'ordre.
fn search_score(hay: &str, words: &[&str], q: &str) -> Option<i32> {
    let mut total = 0;
    for w in words {
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
    Some(total)
}

fn search_defs(q: &str, kind: GraphKind, need_input: bool) -> Vec<(i32, &'static Def)> {
    let q = crate::chat::fold(q);
    let words: Vec<&str> = q.split_whitespace().collect();
    let mut scored: Vec<(i32, &Def)> = DEFS
        .iter()
        .filter(|d| menu_cat(d, kind, false).is_some())
        .filter(|d| !need_input || !d.inputs.is_empty())
        .filter_map(|d| {
            let hay = crate::chat::fold(&format!("{} {} {} {} {}", d.label, crate::i18n::en(d.label), d.op, d.cat, crate::i18n::en(d.cat)));
            search_score(&hay, &words, &q).map(|s| (s, d))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored
}

/// Recherche du menu d'ajout. En plus des nodes et des groupes, chaque opération d'un node à choix
/// (« Math › multiplier », « Mélange › écran ») se trouve par son nom et arrive déjà réglée.
fn search_picks(q: &str, picks: &[(String, &'static str, Pick, bool)], need_input: bool) -> Vec<(i32, String, &'static str, Pick)> {
    let q = crate::chat::fold(q);
    let words: Vec<&str> = q.split_whitespace().collect();
    let mut out: Vec<(i32, String, &'static str, Pick)> = vec![];
    for (label, cat, pick, has_input) in picks {
        if need_input && !has_input {
            continue;
        }
        match pick {
            Pick::Op(op) => {
                let Some(d) = def(op) else { continue };
                let hay = crate::chat::fold(&format!("{} {} {} {} {}", d.label, crate::i18n::en(d.label), d.op, cat, crate::i18n::en(cat)));
                if let Some(sc) = search_score(&hay, &words, &q) {
                    out.push((sc + 4, label.clone(), cat, pick.clone()));
                }
                if matches!(d.op, "math" | "vmath" | "compare" | "logic" | "mix") {
                    if let Some(ParamDef::Choice(key, opts)) = d.params.first() {
                        for o in opts.iter() {
                            let hay = crate::chat::fold(&format!("{} {} {} {}", o, crate::i18n::en(o), d.label, crate::i18n::en(d.label)));
                            if let Some(sc) = search_score(&hay, &words, &q) {
                                out.push((sc, format!("{} › {}", crate::i18n::t(d.label), crate::i18n::t_mid(o)), cat, Pick::Preset(d.op, key, o)));
                            }
                        }
                    }
                }
            }
            _ => {
                if let Some(sc) = search_score(&crate::chat::fold(&format!("{label} groupe group")), &words, &q) {
                    out.push((sc + 6, label.clone(), cat, pick.clone()));
                }
            }
        }
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out
}

fn sock_color(s: Sock) -> Color32 {
    match s {
        Sock::Float => Color32::from_rgb(160, 160, 165),
        Sock::Color => Color32::from_rgb(230, 190, 60),
        Sock::Vector => Color32::from_rgb(120, 130, 225),
        Sock::Bool => Color32::from_rgb(215, 150, 190),
        Sock::Int => Color32::from_rgb(110, 170, 120),
        Sock::Geo => Color32::from_rgb(70, 200, 175),
        Sock::Any => Color32::from_rgb(120, 120, 124),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn exr_multicouche_et_roto() {
        use exr::prelude::*;
        let dir = std::env::temp_dir().join(format!("aryadne-exr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("passes.exr");
        let (w, h) = (8usize, 4usize);
        let ch = |name: &str, v: f32| AnyChannel::new(name, FlatSamples::F32(vec![v; w * h]));
        let chans = AnyChannels::sort(vec![ch("Diffuse.R", 0.8), ch("Diffuse.G", 0.2), ch("Diffuse.B", 0.1), ch("Depth.Z", 5.0), ch("R", 0.5), ch("G", 0.5), ch("B", 0.5), ch("A", 1.0)].into());
        let layer = Layer::new((w, h), LayerAttributes::named("ViewLayer"), Encoding::FAST_LOSSLESS, chans);
        Image::from_layer(layer).write().to_file(&f).unwrap();
        let passes = exr_passes(&f).unwrap();
        assert!(passes.contains(&"ViewLayer.Diffuse".to_string()), "{passes:?}");
        let d = load_exr_pass(&f, "diffuse", 1024).unwrap();
        assert_eq!((d.w, d.h), (8, 4));
        assert!((d.px[0][0] - 0.8).abs() < 1e-5 && (d.px[0][2] - 0.1).abs() < 1e-5);
        // Un canal seul (profondeur) remplit les trois couleurs
        let z = load_exr_pass(&f, "Depth", 1024).unwrap();
        assert!((z.px[3][1] - 5.0).abs() < 1e-5);
        assert!(load_exr_pass(&f, "rien", 1024).err().unwrap().contains("Diffuse"));
        // Tracé roto : un carré au milieu
        let pts = parse_points("0.25,0.25;0.75,0.25;0.75,0.75;0.25,0.75");
        assert_eq!(parse_points(&format_points(&pts)), pts);
        assert!(point_in_poly(&pts, [0.5, 0.5]) && !point_in_poly(&pts, [0.1, 0.5]));
        assert!((poly_distance(&pts, [0.5, 0.5]) - 0.25).abs() < 1e-5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;

    #[test]
    fn texture_video() {
        // Sans ffmpeg (certaines machines de test), rien à vérifier
        let Some(ff) = crate::ffmpeg::find("ffmpeg") else { return };
        let dir = std::env::temp_dir().join(format!("aryadne-vt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let vid = dir.join("clip.mp4");
        let ok = crate::ffmpeg::command(ff).args(["-v", "error", "-f", "lavfi", "-i", "testsrc=duration=2:size=64x48:rate=10", "-pix_fmt", "yuv420p", "-y"]).arg(&vid).status().map(|s| s.success()).unwrap_or(false);
        if !ok {
            return;
        }
        let root = dir.to_string_lossy().to_string();
        assert!(is_video_source("clip.mp4") && is_video_source("x.aryseq"));
        let a = video_frame(&root, "clip.mp4", &vid, 0.5).unwrap();
        let b = video_frame(&root, "clip.mp4", &vid, 1.5).unwrap();
        // Deux temps, deux images ; le même temps reprend le cache
        assert_ne!(a, b);
        assert_eq!(a, video_frame(&root, "clip.mp4", &vid, 0.5).unwrap());
        let img = load_img(&a, 1024).unwrap();
        assert_eq!((img.w, img.h), (64, 48));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn coordonnees_branchees_dans_un_motif() {
        // Un damier lu à travers un « placement » décalé d'une demi-case change de couleur au même point
        let build = |shift: f32| {
            let mut g = Graph::new_material();
            let out = g.nodes.iter().find(|n| n.op == "mat_out").unwrap().id;
            let ck = g.add("checker", [0.0, 0.0]);
            let mp = g.add("mapping", [-200.0, 0.0]);
            g.nodes.iter_mut().find(|n| n.id == mp).unwrap().params.insert("x".into(), Param::F(shift));
            let last = def("checker").unwrap().inputs.len() - 1;
            g.connect(mp, 0, ck, last);
            g.connect(ck, 0, out, 0);
            Program::compile(&g, "", 64).ok().unwrap()
        };
        let c = Ctx { pos: [0.3, 0.3, 0.3], nrm: [0.0, 1.0, 0.0], ..Default::default() };
        let (a, b) = (build(0.0).material(&c, &mut vec![]), build(0.125).material(&c, &mut vec![]));
        assert!((a.color[0] - b.color[0]).abs() > 0.5, "{:?} {:?}", a.color, b.color);
        // Chaque node du catalogue se compile et s'évalue sans valeur invalide
        for d in DEFS.iter().filter(|d| d.mat && d.op != "mat_out" && !d.outputs.is_empty()) {
            let mut g = Graph::new_material();
            let out = g.nodes.iter().find(|n| n.op == "mat_out").unwrap().id;
            let n = g.add(d.op, [0.0, 0.0]);
            g.connect(n, 0, out, 0);
            let s = Program::compile(&g, "", 64).ok().unwrap().material(&c, &mut vec![]);
            assert!(s.color.iter().all(|x| x.is_finite()), "{}", d.op);
        }
        // Même chose côté compositing, sur une petite image
        for d in DEFS.iter().filter(|d| d.comp && d.op != "comp_out" && !d.outputs.is_empty()) {
            let mut g = Graph::new_compositor();
            let out = g.nodes.iter().find(|n| n.op == "comp_out").unwrap().id;
            let n = g.add(d.op, [0.0, 0.0]);
            g.connect(n, 0, out, 0);
            let img = Program::compile(&g, "", 64).ok().unwrap().composite(Some([24, 16]));
            assert!(img.px.len() == 24 * 16 && img.px.iter().all(|p| p.iter().all(|x| x.is_finite())), "{}", d.op);
        }
    }

    /// RAMP_OUT=chemin.mat.json cargo test rampe_exemple : écrit un matériau d'essai avec une rampe à quatre couleurs
    #[test]
    fn rampe_exemple() {
        let Ok(out_path) = std::env::var("RAMP_OUT") else { return };
        let mut g = Graph::new_material();
        let out = g.nodes.iter().find(|n| n.op == "mat_out").unwrap().id;
        let no = g.add("noise", [60.0, 60.0]);
        let cr = g.add("colorramp", [300.0, 60.0]);
        g.nodes.iter_mut().find(|n| n.id == cr).unwrap().params.insert("dégradé".into(), Param::S("0.15,0.05,0.1,0.35,1;0.45,0.1,0.6,0.7,1;0.6,0.95,0.85,0.3,1;0.9,0.9,0.2,0.1,1".into()));
        g.connect(no, 0, cr, 0);
        let cv = g.add("curves", [540.0, 60.0]);
        g.nodes.iter_mut().find(|n| n.id == cv).unwrap().params.insert("courbes".into(), Param::S("0,0;0.3,0.1;0.7,0.9;1,1|0,0;1,1|0,0;1,1|0,0;0.5,0.7;1,1".into()));
        g.connect(cr, 0, cv, 0);
        g.connect(cv, 0, out, 0);
        g.nodes.iter_mut().find(|n| n.id == out).unwrap().pos = [800.0, 60.0];
        g.save(std::path::Path::new(&out_path)).unwrap();
    }

    #[test]
    fn unmult_profondeur_grain() {
        // Un graphe image → node → sortie, l'image venant de `over`
        let run = |op: &str, params: &[(&str, f32)], over: &Img, plate_too: bool| -> Img {
            let mut g = Graph::new_compositor();
            g.nodes.retain(|n| n.op == "image" || n.op == "comp_out");
            g.links.clear();
            let (img, out) = (g.nodes[0].id, g.nodes[1].id);
            let k = g.add(op, [100.0, 0.0]);
            for (name, v) in params {
                g.nodes.iter_mut().find(|n| n.id == k).unwrap().params.insert(name.to_string(), Param::F(*v));
            }
            g.connect(img, 0, k, 0);
            if plate_too {
                g.connect(img, 0, k, 1);
            }
            g.connect(k, 0, out, 0);
            Program::compile(&g, "", 64).ok().unwrap().composite_over(Some([over.w, over.h]), Some(over))
        };
        // Retirer le noir : le noir devient transparent, la couleur garde sa teinte pleine
        let one = Img { w: 1, h: 1, px: vec![[0.5, 0.25, 0.0, 1.0]] };
        let o = run("unmult", &[], &one, false);
        assert!((o.px[0][0] - 1.0).abs() < 1e-4 && (o.px[0][1] - 0.5).abs() < 1e-4 && (o.px[0][3] - 0.5).abs() < 1e-4, "{:?}", o.px[0]);
        // Damier fin : net à la mise au point, moyenné loin d'elle (profondeur 0 partout)
        let (w, h) = (24usize, 24usize);
        let checker = Img { w, h, px: (0..w * h).map(|i| if (i % w + i / w) % 2 == 0 { [1.0, 1.0, 1.0, 1.0] } else { [0.0, 0.0, 0.0, 1.0] }).collect() };
        let net = run("zdefocus", &[("mise au point", 0.0)], &checker, false);
        assert_eq!(net.px[12 * w + 12], checker.px[12 * w + 12]);
        let flou = run("zdefocus", &[("mise au point", 50.0), ("rayon max (px)", 4.0)], &checker, false);
        assert!((flou.px[12 * w + 12][0] - 0.5).abs() < 0.2, "{:?}", flou.px[12 * w + 12]);
        // Grain assorti : rien sur une plaque lisse, du grain sur une plaque bruitée
        let flat = Img { w, h, px: vec![[0.5, 0.5, 0.5, 1.0]; w * h] };
        let a = run("grainmatch", &[], &flat, true);
        assert!(a.px.iter().all(|c| (c[0] - 0.5).abs() < 1e-4));
        let noisy = Img { w, h, px: (0..w * h).map(|i| { let v = 0.5 + (hash3(i as i32, 0, 5) - 0.5) * 0.2; [v, v, v, 1.0] }).collect() };
        let b = run("grainmatch", &[], &flat_to(&noisy), true);
        let spread = b.px.iter().map(|c| c[0]).fold(0.0f32, f32::max) - b.px.iter().map(|c| c[0]).fold(1.0f32, f32::min);
        assert!(spread > 0.05, "{spread}");
        fn flat_to(i: &Img) -> Img {
            Img { w: i.w, h: i.h, px: i.px.clone() }
        }
    }

    #[test]
    fn table_de_couleurs() {
        // Graphe neutre : la table rend chaque couleur telle quelle
        let mut g = Graph::new_compositor();
        g.nodes.retain(|n| n.op == "image" || n.op == "comp_out");
        g.links.clear();
        let (img, out) = (g.nodes[0].id, g.nodes[1].id);
        g.connect(img, 0, out, 0);
        let cube = Program::compile(&g, "", 64).ok().unwrap().lut_cube(5);
        let rows: Vec<Vec<f32>> = cube.lines().skip(2).map(|l| l.split(' ').map(|x| x.parse().unwrap()).collect()).collect();
        assert_eq!(rows.len(), 125);
        // Le rouge varie le plus vite, puis le vert, puis le bleu
        assert_eq!(rows[1], vec![0.25, 0.0, 0.0]);
        assert_eq!(rows[5], vec![0.0, 0.25, 0.0]);
        assert_eq!(rows[124], vec![1.0, 1.0, 1.0]);
        // Avec une inversion entre les deux, la table inverse
        let inv = g.add("invert", [100.0, 0.0]);
        g.links.clear();
        g.connect(img, 0, inv, 0);
        g.connect(inv, 0, out, 0);
        let cube = Program::compile(&g, "", 64).ok().unwrap().lut_cube(5);
        let first: Vec<f32> = cube.lines().nth(2).unwrap().split(' ').map(|x| x.parse().unwrap()).collect();
        assert!(first.iter().all(|x| (*x - 1.0).abs() < 1e-4), "{first:?}");
    }

    #[test]
    fn courbes_rvb() {
        let c = parse_curves("0,0;0.5,0.8;1,1|0,0;1,1|0,1;1,0|");
        assert_eq!(c[0].len(), 3);
        assert_eq!(c[3], vec![[0.0, 0.0], [1.0, 1.0]]);
        // La courbe passe par ses points et reste entre 0 et 1
        assert!((eval_curve(&c[0], 0.5) - 0.8).abs() < 1e-5);
        assert!((0..=100).all(|k| (0.0..=1.0).contains(&eval_curve(&c[0], k as f32 / 100.0))));
        // Une droite reste une droite ; une courbe inversée inverse
        assert!((eval_curve(&c[1], 0.3) - 0.3).abs() < 1e-5);
        assert!((eval_curve(&c[2], 0.25) - 0.75).abs() < 1e-5);
        assert_eq!(parse_curves(&curves_string(&c)), c);
    }

    #[test]
    fn rampe_de_couleur() {
        let stops = parse_ramp("1,1,0,0,1; 0,0,0,1,1; 0.5,0,1,0,0.5");
        assert_eq!(stops.len(), 3);
        assert!(stops[0].0 == 0.0 && stops[1].0 == 0.5);
        assert_eq!(eval_ramp(&stops, 0.25, 0), [0.0, 0.5, 0.5, 0.75]);
        assert_eq!(eval_ramp(&stops, 0.25, 2), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(eval_ramp(&stops, 2.0, 0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(parse_ramp(&ramp_string(&stops)), stops);
        // Un texte abîmé redonne un dégradé noir vers blanc plutôt qu'une erreur
        assert_eq!(parse_ramp("n'importe quoi").len(), 2);
    }

    #[test]
    fn recherche_de_nodes() {
        let names = |q: &str| -> Vec<String> { search_defs(q, GraphKind::Material, false).iter().take(8).map(|x| crate::i18n::en(x.1.label).to_lowercase()).collect() };
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
        let mut g = Graph::empty(GraphKind::Compositor);
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

#[cfg(test)]
mod tests_ops {
    use super::*;

    /// Valeur de la sortie `out` d'un node seul, réglé par `setup`, à l'image `frame`.
    fn run(op: &str, out: usize, frame: f32, setup: impl FnOnce(&mut Node)) -> V {
        let mut g = Graph::empty(GraphKind::Material);
        let o = g.add("mat_out", [300.0, 0.0]);
        let id = g.add(op, [0.0, 0.0]);
        setup(g.nodes.iter_mut().find(|n| n.id == id).unwrap());
        g.connect(id, out, o, 0);
        let prog = Program::compile_at(&g, "", 64, frame, 24.0).map_err(|e| e.1).unwrap();
        prog.material(&Ctx { uv: [0.25, 0.75], pos: [1.0, 2.0, 3.0], nrm: [0.0, 1.0, 0.0], view: [0.0, 0.0, 1.0], vcol: [1.0; 4], res: [64.0, 64.0], ..Default::default() }, &mut vec![]).color
    }

    fn math(op: &str, a: f32, b: f32) -> f32 {
        run("math", 0, 0.0, |n| {
            n.values = vec![[a; 4], [b; 4]];
            n.params.insert("opération".into(), Param::S(op.into()));
        })[0]
    }

    #[test]
    fn math_complet() {
        let near = |x: f32, y: f32| (x - y).abs() < 1e-4;
        assert!(near(math("ajouter", 2.0, 3.0), 5.0));
        assert!(near(math("arc tangente (A / B)", 1.0, 1.0), std::f32::consts::FRAC_PI_4));
        assert!(near(math("signe", -3.0, 0.0), -1.0));
        assert!(near(math("arrondi", 2.6, 0.0), 3.0));
        assert!(near(math("arrondi sup.", 2.1, 0.0), 3.0));
        assert!(near(math("tronquer", -2.7, 0.0), -2.0));
        assert!(near(math("aimanter (pas B)", 0.74, 0.25), 0.5));
        assert!(near(math("boucler (0 à B)", 2.5, 1.0), 0.5));
        assert!(near(math("ping-pong (0 à B)", 1.25, 1.0), 0.75));
        assert!(near(math("ping-pong (0 à B)", 0.25, 1.0), 0.25));
        assert!(near(math("inverse (1 / A)", 4.0, 0.0), 0.25));
        assert!(near(math("inverse (1 / A)", 0.0, 0.0), 0.0));
        assert!(near(math("en degrés", std::f32::consts::PI, 0.0), 180.0));
        assert!(near(math("logarithme", std::f32::consts::E, 0.0), 1.0));
        // Loin du croisement, le minimum doux est le minimum ; au croisement, il passe juste dessous
        assert!(near(math("minimum doux", 0.2, 0.9), 0.2));
        assert!(math("minimum doux", 0.5, 0.5) < 0.5);
        // Chaque opération de la liste est traitée (aucune ne retombe en silence sur l'addition)
        for op in MATH.iter().filter(|o| **o != "ajouter") {
            assert!(!near(math(op, 0.37, 0.61), 0.98) || *op == "maximum doux", "{op}");
        }
    }

    #[test]
    fn vecteurs_logique_expression_temps() {
        let near = |x: f32, y: f32| (x - y).abs() < 1e-4;
        let vm = |op: &str, a: [f32; 3], b: [f32; 3], out: usize| {
            run("vmath", out, 0.0, |n| {
                n.values = vec![[a[0], a[1], a[2], 0.0], [b[0], b[1], b[2], 0.0], [2.0; 4]];
                n.params.insert("opération".into(), Param::S(op.into()));
            })
        };
        assert_eq!(vm("produit vectoriel", [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0)[..3], [0.0, 0.0, 1.0]);
        assert!(near(vm("produit scalaire", [1.0, 2.0, 3.0], [4.0, 5.0, 6.0], 1)[0], 32.0));
        assert!(near(vm("longueur", [3.0, 4.0, 0.0], [0.0; 3], 1)[0], 5.0));
        assert!(near(vm("normaliser", [0.0, 0.0, 9.0], [0.0; 3], 0)[2], 1.0));
        assert_eq!(vm("échelle (× facteur)", [1.0, 2.0, 3.0], [0.0; 3], 0)[..3], [2.0, 4.0, 6.0]);
        assert_eq!(vm("réfléchir", [1.0, -1.0, 0.0], [0.0, 1.0, 0.0], 0)[..3], [1.0, 1.0, 0.0]);
        for op in VMATH.iter().filter(|o| **o != "ajouter") {
            let v = vm(op, [0.3, 0.5, 0.7], [0.2, 0.9, 0.4], 0);
            assert!(v[..3] != [0.5, 1.4, 1.1], "{op}");
        }
        let cmp = |op: &str, a: f32, b: f32| {
            run("compare", 0, 0.0, |n| {
                n.values = vec![[a; 4], [b; 4], [0.001; 4]];
                n.params.insert("comparaison".into(), Param::S(op.into()));
            })[0]
        };
        assert_eq!(cmp("égal", 1.0, 1.0005), 1.0);
        assert_eq!(cmp("différent", 1.0, 1.0005), 0.0);
        assert_eq!(cmp("inférieur ou égal", 2.0, 2.0), 1.0);
        assert_eq!(cmp("supérieur", 2.0, 2.0), 0.0);
        let logic = |op: &str, a: f32, b: f32| {
            run("logic", 0, 0.0, |n| {
                n.values = vec![[a; 4], [b; 4]];
                n.params.insert("opération".into(), Param::S(op.into()));
            })[0]
        };
        assert_eq!([logic("et", 1.0, 0.0), logic("ou", 1.0, 0.0), logic("non", 0.0, 0.0), logic("ou exclusif", 1.0, 1.0), logic("non-et", 1.0, 1.0), logic("non-ou", 0.0, 0.0)], [0.0, 1.0, 1.0, 0.0, 0.0, 1.0]);
        // Aiguillage : la condition choisit l'une des deux entrées
        let sw = |c: f32| run("switch", 0, 0.0, |n| n.values = vec![[c; 4], [0.1, 0.2, 0.3, 1.0], [0.7, 0.8, 0.9, 1.0]]);
        assert_eq!(sw(0.0)[0], 0.1);
        assert_eq!(sw(1.0)[0], 0.7);
        // Expression : entrées a b c d, position x y z, coordonnées u v
        let ex = |f: &str| {
            run("expr", 0, 0.0, |n| {
                n.values = vec![[2.0; 4], [3.0; 4], [4.0; 4], [5.0; 4]];
                n.params.insert("formule".into(), Param::S(f.into()));
            })[0]
        };
        assert!(near(ex("a * b + c / d"), 6.8));
        assert!(near(ex("x + y + z"), 6.0));
        assert!(near(ex("mix(a, b, u)"), 2.25));
        assert!(near(ex("if(a > b, 1, 2)"), 2.0));
        // Une formule fausse donne une erreur lisible, sur le bon node
        let mut g = Graph::empty(GraphKind::Material);
        let o = g.add("mat_out", [0.0, 0.0]);
        let e = g.add("expr", [0.0, 0.0]);
        g.nodes.iter_mut().find(|n| n.id == e).unwrap().params.insert("formule".into(), Param::S("a +* zz".into()));
        g.connect(e, 0, o, 0);
        let err = Program::compile(&g, "", 64).err().unwrap();
        assert_eq!(err.0, e);
        assert!(!err.1.contains("ligne"), "{}", err.1);
        // Temps : l'image et les secondes
        assert_eq!(run("time", 0, 48.0, |_| {})[0], 48.0);
        assert!(near(run("time", 1, 48.0, |_| {})[0], 2.0));
        // Aléatoire : entre min et max, stable, et changé par la graine
        let rnd = |seed: f32| run("random", 0, 0.0, |n| n.values = vec![[2.0; 4], [3.0; 4], [seed; 4], [0.0; 4]])[0];
        assert!((2.0..=3.0).contains(&rnd(0.0)));
        assert_eq!(rnd(1.0), rnd(1.0));
        assert_ne!(rnd(1.0), rnd(2.0));
    }

    #[test]
    fn recherche_trouve_les_operations_et_les_groupes() {
        let picks: Vec<(String, &'static str, Pick, bool)> = DEFS.iter().filter_map(|d| menu_cat(d, GraphKind::Material, false).map(|c| (d.label.to_string(), c, Pick::Op(d.op), !d.inputs.is_empty()))).chain([("Rouille".to_string(), "structure", Pick::Group("Rouille".into()), true)]).collect();
        let first = |q: &str| search_picks(q, &picks, false).first().map(|x| x.3.clone());
        assert!(search_picks("multiplier", &picks, false).iter().any(|x| x.3 == Pick::Preset("math", "opération", "multiplier")));
        assert!(search_picks("produit vect", &picks, false).iter().any(|x| x.3 == Pick::Preset("vmath", "opération", "produit vectoriel")));
        assert_eq!(first("rouille"), Some(Pick::Group("Rouille".into())));
        assert_eq!(first("renvoi"), Some(Pick::Op("reroute")));
        // Les nodes internes ne se proposent pas, la zone de simulation reste à la géométrie
        assert!(!picks.iter().any(|x| matches!(x.2, Pick::Op("repeat_out" | "sim_out" | "group" | "sim_in" | "group_in"))));
        assert!(DEFS.iter().any(|d| d.op == "sim_in" && menu_cat(d, GraphKind::Geometry, false) == Some("simulation")));
        assert!(DEFS.iter().any(|d| d.op == "group_in" && menu_cat(d, GraphKind::Material, true) == Some("structure")));
        // Un nom de node l'emporte sur une opération qui contient le même mot
        assert_eq!(first("math"), Some(Pick::Op("math")));
    }
}

#[cfg(test)]
mod tests_comp_cache {
    use super::*;

    #[test]
    fn compositing_ne_refait_que_l_aval() {
        // bruit → flou → luminosité → sortie, à une taille que les autres essais n'utilisent pas
        let mut g = Graph::empty(GraphKind::Compositor);
        let out = g.add("comp_out", [900.0, 0.0]);
        let noise = g.add("noise", [0.0, 0.0]);
        let blur = g.add("blur", [300.0, 0.0]);
        let bright = g.add("brightcon", [600.0, 0.0]);
        g.nodes.iter_mut().find(|n| n.id == noise).unwrap().values[0] = [7.31; 4];
        g.connect(noise, 1, blur, 0);
        g.connect(blur, 0, bright, 0);
        g.connect(bright, 0, out, 0);
        let size = Some([61, 37]);
        let run = |g: &Graph| {
            let img = Program::compile(g, "", 64).map_err(|e| e.1).unwrap().composite(size);
            (img, COMP_COMPUTED.with(|c| c.get()))
        };
        let (a, n1) = run(&g);
        assert_eq!(n1, 3);
        // Même graphe : tout vient du cache, et l'image est la même
        let (b, n2) = run(&g);
        assert_eq!(n2, 0);
        assert_eq!(a.px, b.px);
        // Régler la luminosité ne refait ni le bruit ni le flou
        g.nodes.iter_mut().find(|n| n.id == bright).unwrap().values[1] = [0.2; 4];
        let (c, n3) = run(&g);
        assert_eq!(n3, 1);
        assert!(c.px != a.px);
        // Régler le flou refait le flou et ce qui le suit, pas le bruit
        g.nodes.iter_mut().find(|n| n.id == blur).unwrap().params.insert("rayon".into(), Param::F(30.0));
        assert_eq!(run(&g).1, 2);
        // Une autre taille est un autre calcul
        assert_eq!(Program::compile(&g, "", 64).map_err(|e| e.1).unwrap().composite(Some([62, 37])).px.len(), 62 * 37);
        assert_eq!(COMP_COMPUTED.with(|c| c.get()), 3);
        // Le résultat du cache est celui qu'on aurait sans cache
        let prog = Program::compile(&g, "", 64).map_err(|e| e.1).unwrap();
        let direct = prog.composite(size);
        let fresh = {
            *COMP_CACHE.lock().unwrap() = None;
            prog.composite(size)
        };
        assert_eq!(direct.px, fresh.px);
    }
}
