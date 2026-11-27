//! Atelier de matériaux (façon Substance Painter + Designer) : un matériau est une pile de calques.
//! Chaque calque a son graphe de nodes : le sien (calque « graphe », enregistré dans le matériau), ou
//! celui d'un matériau maître du coffre (un .arymat qui expose des paramètres, dont le calque est une
//! *instance*). Il se pose selon un *masque* (plein, usure des arêtes, saleté des creux, poussière…, ou
//! son propre graphe). Un ancien matériau en nodes est une pile d'un seul calque « graphe ».
//! La pile entière se ramène à un seul graphe de nodes (`flatten`) : c'est lui que voient la 3D,
//! le rendu et l'éditeur de nodes (« pile = graphe »).
use crate::nodes::{Graph, GraphKind, Link, Param};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize, PartialEq, Default, Debug)]
pub struct Stack {
    /// Du bas vers le haut
    pub layers: Vec<Layer>,
}

/// Un graphe rangé dans un calque (son remplissage ou son masque).
#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Inner(pub Box<Graph>);

impl PartialEq for Inner {
    fn eq(&self, o: &Inner) -> bool {
        serde_json::to_string(&self.0).ok() == serde_json::to_string(&o.0).ok()
    }
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "graphe de {} nodes", self.0.nodes.len())
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Layer {
    pub name: String,
    /// Matériau maître : chemin d'un .arymat dans le coffre (vide pour un calque « graphe »)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub master: String,
    /// Calque « graphe » : son propre graphe de nodes, enregistré dans le matériau
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<Inner>,
    /// Valeurs des paramètres exposés par le maître (nom → valeur), à la place de leurs valeurs par défaut
    #[serde(default)]
    pub params: BTreeMap<String, [f32; 4]>,
    #[serde(default)]
    pub mask: Mask,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default = "yes")]
    pub visible: bool,
}

impl Layer {
    /// Instance d'un matériau maître du coffre.
    pub fn linked(name: impl Into<String>, master: impl Into<String>) -> Layer {
        Layer { name: name.into(), master: master.into(), graph: None, params: Default::default(), mask: Default::default(), opacity: 1.0, visible: true }
    }

    /// Calque « graphe » : il porte son propre graphe.
    pub fn own(name: impl Into<String>, graph: Graph) -> Layer {
        Layer { graph: Some(Inner(Box::new(graph))), ..Layer::linked(name, "") }
    }

    /// Le graphe du remplissage : celui du calque, sinon celui de son maître.
    pub fn fill(&self, root: &str) -> Option<Graph> {
        match &self.graph {
            Some(g) => Some((*g.0).clone()),
            None => load_master(root, &self.master),
        }
    }

    /// Le remplissage avec les valeurs que ce calque donne aux paramètres exposés : ce que montre sa vignette.
    pub fn fill_with_params(&self, root: &str) -> Option<Graph> {
        let mut g = self.fill(root)?;
        with_params(&mut g, &self.params);
        Some(g)
    }
}

/// Donne aux nodes « Paramètre exposé » d'un graphe (racine et groupes) les valeurs choisies sur le calque.
pub fn with_params(g: &mut Graph, params: &BTreeMap<String, [f32; 4]>) {
    if params.is_empty() {
        return;
    }
    for n in g.nodes.iter_mut().chain(g.groups.iter_mut().flat_map(|gr| gr.nodes.iter_mut())) {
        if n.op == "param" {
            if let Some(v) = params.get(&param_name(n)) {
                if let Some(x) = n.values.first_mut() {
                    *x = *v;
                }
            }
        }
    }
}

/// Nom du calque unique d'un ancien matériau en nodes.
pub const GRAPH_LAYER: &str = "graphe";

fn default_name(n: &str) -> bool {
    let n = n.trim().to_lowercase();
    n == GRAPH_LAYER || n == "graph"
}

/// Nom montré d'un calque : celui d'un ancien matériau en nodes se dit dans la langue de l'interface.
pub fn shown_name(n: &str) -> String {
    if default_name(n) { crate::i18n::t(GRAPH_LAYER) } else { n.to_string() }
}

/// Le matériau tel que l'atelier l'édite : toujours une pile. Un ancien matériau en nodes (sans pile)
/// devient une pile d'un seul calque « graphe » qui porte tout son graphe, groupes et cadres compris.
pub fn to_stack(g: Graph) -> Graph {
    if g.kind != GraphKind::Material || g.stack.is_some() {
        return g;
    }
    Graph { stack: Some(Stack { layers: vec![Layer::own(GRAPH_LAYER, g)] }), ..Graph::empty(GraphKind::Material) }
}

/// Le matériau tel qu'on l'écrit : une pile d'un seul calque « graphe » nu (posé partout, opaque, sans
/// réglage) redevient un simple graphe. Un ancien fichier se relit et se réécrit donc sans changer de forme.
pub fn to_file(g: &Graph) -> std::borrow::Cow<'_, Graph> {
    if let (GraphKind::Material, Some(st)) = (g.kind, &g.stack) {
        if let [l] = st.layers.as_slice() {
            if let Some(inner) = &l.graph {
                if g.nodes.is_empty() && l.master.is_empty() && l.params.is_empty() && l.mask == Mask::default() && l.opacity >= 1.0 && l.visible && default_name(&l.name) && inner.0.stack.is_none() {
                    return std::borrow::Cow::Owned((*inner.0).clone());
                }
            }
        }
    }
    std::borrow::Cow::Borrowed(g)
}

fn one() -> f32 {
    1.0
}
fn yes() -> bool {
    true
}

/// Où le calque se pose.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Mask {
    /// « plein », « arêtes », « creux », « dessus », « coulures », « rayures », « grunge », « bruit »…
    /// ou « graphe » : le masque est alors la couleur de sortie de `graph`
    pub kind: String,
    pub amount: f32,
    pub contrast: f32,
    pub scale: f32,
    #[serde(default)]
    pub invert: bool,
    /// Masque intelligent : un second générateur qui casse le premier (comme un « breakup » de Substance)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub breakup: String,
    #[serde(default = "half")]
    pub breakup_amount: f32,
    #[serde(default = "eight")]
    pub breakup_scale: f32,
    /// Masque « peint » : le canal de masque peint à la main sur l'objet (0 à 3, « masque 1 » à « masque 4 »)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub paint: u8,
    /// Masque en nodes : un graphe dont la couleur de sortie dit où le calque se pose (blanc : partout)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<Inner>,
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

fn half() -> f32 {
    0.5
}
fn eight() -> f32 {
    8.0
}

impl Default for Mask {
    fn default() -> Self {
        Mask { kind: "plein".into(), amount: 0.5, contrast: 0.5, scale: 8.0, invert: false, breakup: String::new(), breakup_amount: 0.5, breakup_scale: 8.0, paint: 0, graph: None }
    }
}

/// Masques intelligents tout prêts : nom, puis le masque (générateur, réglages, cassure)
pub fn smart_masks() -> Vec<(&'static str, Mask)> {
    let m = |kind: &str, amount: f32, contrast: f32, scale: f32, invert: bool, breakup: &str, ba: f32, bs: f32| Mask { kind: kind.into(), amount, contrast, scale, invert, breakup: breakup.into(), breakup_amount: ba, breakup_scale: bs, paint: 0, graph: None };
    vec![
        ("peinture écaillée sur les arêtes", m("arêtes", 0.55, 0.75, 14.0, false, "grunge", 0.7, 6.0)),
        ("rouille dans les creux", m("creux", 0.6, 0.6, 8.0, false, "taches", 0.6, 10.0)),
        ("poussière et saleté", m("dessus", 0.5, 0.4, 8.0, false, "bruit", 0.5, 12.0)),
        ("bords légèrement usés", m("arêtes", 0.3, 0.9, 20.0, false, "", 0.5, 8.0)),
        ("coulures de pluie", m("coulures", 0.5, 0.6, 6.0, false, "grunge", 0.4, 4.0)),
        ("métal rayé", m("rayures", 0.5, 0.7, 10.0, false, "bruit", 0.5, 3.0)),
        ("sol craquelé", m("fissures", 0.5, 0.5, 6.0, false, "grunge", 0.3, 5.0)),
    ]
}

/// Les masques proposés : identifiant, nom affiché, node générateur.
pub const MASKS: [(&str, &str, &str); 12] = [
    ("plein", "partout", ""),
    ("arêtes", "usure des arêtes", "edgewear"),
    ("creux", "saleté des creux", "cavdirt"),
    ("dessus", "poussière sur le dessus", "dust"),
    ("coulures", "coulures", "drips"),
    ("rayures", "rayures d'usure", "scratches"),
    ("grunge", "saleté (grunge)", "grunge"),
    ("bruit", "bruit", "noise"),
    ("fissures", "fissures", "cracks"),
    ("taches", "taches", "voronoi"),
    // Peint à la main sur l'objet de la scène, dans un calque de peinture « masque 1 » à « masque 4 »
    ("peint", "peint à la main", "paintmask"),
    // Le masque a son propre graphe de nodes
    ("graphe", "graphe de nodes", ""),
];

/// Un matériau intelligent : une pile de calques enregistrée, à reposer sur un autre objet
pub const SMART_DIR: &str = "matériaux intelligents";

pub fn smart_materials(root: &str) -> Vec<String> {
    let dir = std::path::Path::new(root).join(SMART_DIR);
    let mut out: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str().filter(|n| n.ends_with(".arysmart")).map(|n| n.trim_end_matches(".arysmart").to_string()))
        .collect();
    out.sort();
    out
}

pub fn save_smart(root: &str, name: &str, stack: &Stack) -> Result<std::path::PathBuf, String> {
    let dir = std::path::Path::new(root).join(SMART_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let clean: String = name.chars().map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c }).collect();
    let path = dir.join(format!("{}.arysmart", clean.trim()));
    crate::vault::atomic_write(&path, serde_json::to_string_pretty(stack).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn load_smart(root: &str, name: &str) -> Option<Stack> {
    let t = std::fs::read_to_string(std::path::Path::new(root).join(SMART_DIR).join(format!("{name}.arysmart"))).ok()?;
    serde_json::from_str(&t).ok()
}

/// Ajoute un générateur de masque au graphe ; rend sa sortie
fn generator(g: &mut Graph, kind: &str, amount: f32, contrast: f32, scale: f32, canal: u8, at: [f32; 2]) -> Option<Src> {
    let (_, _, op) = MASKS.iter().find(|m| m.0 == kind).filter(|m| !m.2.is_empty())?;
    let mk = g.add(op, at);
    set_named(g, mk, "quantité", amount);
    set_named(g, mk, "contraste", contrast);
    set_named(g, mk, "échelle du bruit", scale);
    set_named(g, mk, "échelle", scale);
    if *op == "paintmask" {
        set_choice(g, mk, "canal", &(canal.min(3) + 1).to_string());
    }
    let mut m = Src::Link(mk, 0);
    // Les motifs (bruit, taches, fissures) passent par un seuil doux réglé par la quantité
    if matches!(*op, "noise" | "voronoi" | "cracks") {
        let th = g.add("levels", [at[0] + 120.0, at[1]]);
        let lo = (1.0 - amount).clamp(0.0, 0.98) * 0.8;
        let hi = (lo + 0.05 + (1.0 - contrast) * 0.5).min(1.0);
        set_named(g, th, "noir", lo);
        set_named(g, th, "blanc", hi);
        wire(g, m, th, 0);
        m = Src::Link(th, 0);
    }
    Some(m)
}

/// Les noms des paramètres d'un calque de remplissage (dans l'ordre du graphe) : ceux-là se traduisent à l'écran.
pub const FILL_PARAMS: [&str; 3] = ["couleur", "rugosité", "métal"];

/// Le graphe d'un nouveau calque de remplissage : une couleur, une rugosité et un métal, tous trois
/// exposés (ils se règlent depuis la pile, sans ouvrir les nodes).
pub fn fill_graph(color: [f32; 3]) -> Graph {
    let mut g = Graph::empty(GraphKind::Material);
    let out = g.add("mat_out", [320.0, 0.0]);
    for (k, (name, kind, v)) in [("couleur", "couleur", [color[0], color[1], color[2], 1.0]), ("rugosité", "valeur", [0.5; 4]), ("métal", "valeur", [0.0; 4])].into_iter().enumerate() {
        let id = g.add("param", [0.0, k as f32 * 170.0]);
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) {
            n.label = name.to_string();
            n.preview = false;
            n.values[0] = v;
            n.params.insert("type".into(), Param::S(kind.into()));
        }
        g.links.push(Link { from: id, out: if kind == "couleur" { 0 } else { 1 }, to: out, inp: k });
    }
    g
}

/// Le masque d'un calque dans `g` : générateur, cassure, inversion. Rien : le calque se pose partout.
fn mask_src(g: &mut Graph, lm: &Mask, x0: f32, dy: f32) -> Option<Src> {
    let mut m = if lm.kind == "graphe" {
        // Masque en nodes : ce qui entre dans la couleur de sa sortie
        let sub = lm.graph.as_ref()?;
        let src = splice(g, &sub.0, [x0 - 900.0, dy], None)?;
        match src.first().copied()? {
            Src::Const(v) => Src::Const([v[0]; 4]),
            s => s,
        }
    } else {
        let mut m = generator(g, &lm.kind, lm.amount, lm.contrast, lm.scale, lm.paint, [x0 - 400.0, dy])?;
        // Masque intelligent : le second générateur casse le premier (m × mélange(1, cassure, force))
        if let Some(b) = generator(g, &lm.breakup, 0.5, 0.5, lm.breakup_scale, lm.paint, [x0 - 400.0, dy + 160.0]) {
            let mx = g.add("mixf", [x0 - 150.0, dy + 120.0]);
            set_value(g, mx, 0, [lm.breakup_amount; 4]);
            set_value(g, mx, 1, [1.0; 4]);
            wire(g, b, mx, 2);
            let mul = g.add("math", [x0, dy + 60.0]);
            set_choice(g, mul, "opération", "multiplier");
            wire(g, m, mul, 0);
            wire(g, Src::Link(mx, 0), mul, 1);
            m = Src::Link(mul, 0);
        }
        m
    };
    if lm.invert {
        let inv = g.add("math", [x0 + 150.0, dy]);
        set_choice(g, inv, "opération", "soustraire");
        set_value(g, inv, 0, [1.0; 4]);
        wire(g, m, inv, 1);
        m = Src::Link(inv, 0);
    }
    Some(m)
}

/// Le masque d'un calque, écrit en graphe de nodes : le point de départ quand on l'ouvre en nodes.
/// Le générateur choisi, sa cassure et son inversion y sont déjà, branchés sur la couleur de la sortie.
pub fn mask_graph(mask: &Mask) -> Graph {
    let mut g = Graph::empty(GraphKind::Material);
    let out = g.add("mat_out", [520.0, 0.0]);
    let m = if mask.kind == "graphe" { None } else { mask_src(&mut g, mask, 200.0, 0.0) };
    match m {
        Some(m) => wire(&mut g, m, out, 0),
        // Posé partout : du blanc
        None => set_value(&mut g, out, 0, [1.0, 1.0, 1.0, 1.0]),
    }
    // Un masque se lit en gris mat : ni métal, ni brillance
    set_value(&mut g, out, 1, [1.0; 4]);
    // La sortie se pose juste après le dernier node : un graphe serré se lit en entier à l'ouverture
    let right = g.nodes.iter().filter(|n| n.id != out).map(|n| n.pos[0]).fold(f32::NEG_INFINITY, f32::max);
    if right.is_finite() {
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == out) {
            n.pos[0] = right + 220.0;
        }
    }
    g
}

/// Copie le graphe `sub` dans `g` (groupes dépliés, sans sa sortie). Rend ce qui entrait dans chaque
/// prise de sa sortie. `params` : valeurs données aux paramètres exposés.
fn splice(g: &mut Graph, sub: &Graph, at: [f32; 2], params: Option<&BTreeMap<String, [f32; 4]>>) -> Option<Vec<Src>> {
    let flat = sub.effective();
    let flat = crate::nodeflow::lower(&flat, true).ok()?.graph;
    let mo = flat.nodes.iter().find(|n| n.op == "mat_out").cloned()?;
    let off = g.nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1;
    for n in &flat.nodes {
        if n.id == mo.id {
            continue;
        }
        let mut n2 = n.clone();
        n2.id += off;
        n2.pos = [n.pos[0] + at[0], n.pos[1] + at[1]];
        n2.preview = false;
        if n.op == "param" {
            if let Some(v) = params.and_then(|p| p.get(&param_name(n))) {
                if let Some(x) = n2.values.first_mut() {
                    *x = *v;
                }
            }
        }
        g.nodes.push(n2);
    }
    let mut src: Vec<Src> = mo.values.iter().map(|v| Src::Const(*v)).collect();
    for l in &flat.links {
        if l.to == mo.id {
            if l.inp < src.len() {
                src[l.inp] = Src::Link(l.from + off, l.out);
            }
        } else if l.from != mo.id {
            g.links.push(Link { from: l.from + off, out: l.out, to: l.to + off, inp: l.inp });
        }
    }
    g.next_id = g.nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1;
    Some(src)
}

/// Un paramètre exposé par un matériau maître (node « Paramètre exposé ») : nom, valeur par défaut, couleur ou nombre.
#[derive(Clone, Debug, PartialEq)]
pub struct Exposed {
    pub name: String,
    pub default: [f32; 4],
    pub color: bool,
}

pub fn param_name(n: &crate::nodes::Node) -> String {
    if n.label.trim().is_empty() { format!("paramètre {}", n.id) } else { n.label.trim().to_string() }
}

/// Les paramètres qu'un graphe expose (à sa racine ou dans ses groupes).
pub fn exposed(g: &Graph) -> Vec<Exposed> {
    g.nodes
        .iter()
        .chain(g.groups.iter().flat_map(|gr| gr.nodes.iter()))
        .filter(|n| n.op == "param")
        .map(|n| Exposed { name: param_name(n), default: n.values.first().copied().unwrap_or([0.5; 4]), color: matches!(n.params.get("type"), Some(Param::S(s)) if s == "couleur") })
        .collect()
}

thread_local! {
    static CACHE: std::cell::RefCell<std::collections::HashMap<String, (Option<std::time::SystemTime>, Graph)>> = Default::default();
}

/// Charge un maître (gardé en mémoire tant que le fichier ne change pas).
pub fn load_master(root: &str, rel: &str) -> Option<Graph> {
    let path = std::path::Path::new(root).join(rel);
    let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    if let Some(g) = CACHE.with(|c| c.borrow().get(rel).filter(|(t, _)| *t == mtime).map(|(_, g)| g.clone())) {
        return Some(g);
    }
    let g = Graph::load(&path).ok()?;
    CACHE.with(|c| c.borrow_mut().insert(rel.to_string(), (mtime, g.clone())));
    Some(g)
}

/// Oublie un maître gardé en mémoire (on vient de le réécrire).
pub fn forget(rel: &str) {
    CACHE.with(|c| c.borrow_mut().remove(rel));
}

#[derive(Clone, Copy)]
enum Src {
    Const([f32; 4]),
    Link(u32, usize),
}

/// La pile ramenée à un seul graphe de matériau.
pub fn flatten(stack: &Stack, root: &str) -> Graph {
    let mut g = Graph::empty(GraphKind::Material);
    let out = g.add("mat_out", [1400.0, 0.0]);
    let socks: Vec<bool> = crate::nodes::def("mat_out").map(|d| d.inputs.iter().map(|i| matches!(i.1, crate::nodes::Sock::Color)).collect()).unwrap_or_default();
    let defaults: Vec<[f32; 4]> = g.nodes[0].values.clone();
    let mut cur: Vec<Src> = defaults.iter().map(|v| Src::Const(*v)).collect();
    for (li, layer) in stack.layers.iter().enumerate() {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
        }
        let Some(fill) = layer.fill(root) else { continue };
        let dy = li as f32 * 600.0;
        let Some(mut src) = splice(&mut g, &fill, [0.0, dy], Some(&layer.params)) else { continue };
        src.resize(cur.len(), Src::Const([0.0; 4]));
        // Facteur du calque : masque × opacité
        let x0 = -400.0;
        let mut factor = Src::Const([layer.opacity; 4]);
        if let Some(mut m) = mask_src(&mut g, &layer.mask, x0, dy) {
            if layer.opacity < 1.0 {
                let mul = g.add("math", [x0 + 300.0, dy]);
                set_choice(&mut g, mul, "opération", "multiplier");
                wire(&mut g, m, mul, 0);
                set_value(&mut g, mul, 1, [layer.opacity; 4]);
                m = Src::Link(mul, 0);
            }
            factor = m;
        }
        // Mélange canal par canal : la couche du dessous (A) et ce calque (B)
        for ch in 0..cur.len() {
            let full = matches!(factor, Src::Const(v) if v[0] >= 1.0);
            if full {
                cur[ch] = src[ch];
                continue;
            }
            let color = socks.get(ch).copied().unwrap_or(false);
            let mx = g.add(if color { "mix" } else { "mixf" }, [1100.0, dy + ch as f32 * 30.0]);
            wire(&mut g, factor, mx, 0);
            wire(&mut g, cur[ch], mx, 1);
            wire(&mut g, src[ch], mx, 2);
            cur[ch] = Src::Link(mx, 0);
        }
    }
    for (ch, s) in cur.iter().enumerate() {
        wire(&mut g, *s, out, ch);
    }
    g
}

fn wire(g: &mut Graph, s: Src, to: u32, inp: usize) {
    match s {
        Src::Const(v) => set_value(g, to, inp, v),
        Src::Link(n, o) => g.links.push(Link { from: n, out: o, to, inp }),
    }
}

fn set_value(g: &mut Graph, id: u32, inp: usize, v: [f32; 4]) {
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) {
        if let Some(x) = n.values.get_mut(inp) {
            *x = v;
        }
    }
}

fn set_named(g: &mut Graph, id: u32, name: &str, v: f32) {
    let Some(n) = g.nodes.iter().position(|n| n.id == id) else { return };
    let Some(d) = crate::nodes::def(&g.nodes[n].op) else { return };
    if let Some(k) = d.inputs.iter().position(|i| i.0 == name) {
        g.nodes[n].values[k] = [v; 4];
    }
}

fn set_choice(g: &mut Graph, id: u32, name: &str, v: &str) {
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) {
        n.params.insert(name.to_string(), Param::S(v.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masque_en_nodes_serre() {
        // Un générateur seul : la sortie suit de près, sans grand vide entre les deux
        let g = mask_graph(&Mask { kind: "arêtes".into(), ..Mask::default() });
        let out = g.nodes.iter().find(|n| n.op == "mat_out").unwrap().pos[0];
        let generateur = g.nodes.iter().filter(|n| n.op != "mat_out").map(|n| n.pos[0]).fold(f32::NEG_INFINITY, f32::max);
        assert!(out > generateur && out - generateur <= 260.0, "{generateur} {out}");
    }

    #[test]
    fn vignette_avec_les_parametres_du_calque() {
        // La vignette d'un calque doit montrer la couleur choisie, pas la valeur par défaut du paramètre
        let mut g = Graph::new_material();
        let p = g.add("param", [0.0, 0.0]);
        g.nodes.iter_mut().find(|n| n.id == p).unwrap().label = "couleur".into();
        let mut params = BTreeMap::new();
        params.insert("couleur".to_string(), [0.9, 0.0, 0.0, 1.0]);
        let l = Layer { params, ..Layer::own("plastique", g) };
        let f = l.fill_with_params("/nulle/part").unwrap();
        assert_eq!(f.nodes.iter().find(|n| n.id == p).unwrap().values[0], [0.9, 0.0, 0.0, 1.0]);
        // Le graphe du calque lui-même n'est pas touché
        assert_ne!(l.fill("/nulle/part").unwrap().nodes.iter().find(|n| n.id == p).unwrap().values[0], [0.9, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn pile_en_graphe() {
        let dir = std::env::temp_dir().join("aryadne-atelier-test");
        let _ = std::fs::create_dir_all(&dir);
        // Deux maîtres : un métal rouge (couleur exposée) et une peinture
        let mut metal = Graph::new_material();
        metal.links.clear();
        metal.nodes.retain(|n| n.op == "mat_out");
        let p = metal.add("param", [0.0, 0.0]);
        metal.nodes.iter_mut().find(|n| n.id == p).unwrap().label = "teinte".into();
        metal.nodes.iter_mut().find(|n| n.id == p).unwrap().params.insert("type".into(), Param::S("couleur".into()));
        let out = metal.nodes.iter().find(|n| n.op == "mat_out").unwrap().id;
        metal.connect(p, 0, out, 0);
        metal.save(&dir.join("metal.arymat")).unwrap();
        assert_eq!(exposed(&metal).len(), 1);
        let mut paint = Graph::new_material();
        paint.links.clear();
        paint.nodes.retain(|n| n.op == "mat_out");
        paint.nodes[0].values[0] = [0.1, 0.8, 0.2, 1.0];
        paint.save(&dir.join("paint.arymat")).unwrap();
        let root = dir.to_string_lossy().to_string();
        let mut params = BTreeMap::new();
        params.insert("teinte".to_string(), [0.9, 0.1, 0.1, 1.0]);
        let stack = Stack {
            layers: vec![
                Layer { params, ..Layer::linked("métal", "metal.arymat") },
                Layer { mask: Mask { kind: "arêtes".into(), invert: true, breakup: "taches".into(), ..Mask::default() }, opacity: 0.8, ..Layer::linked("peinture", "paint.arymat") },
            ],
        };
        let g = flatten(&stack, &root);
        // Le graphe se compile, et la couleur du calque du dessous vient du paramètre redéfini
        let prog = crate::nodes::Program::compile(&g, &root, 64).map_err(|e| e.1).unwrap();
        let c = crate::nodes::Ctx { masks: [0.0; 4], curv: 0.0, uv: [0.5, 0.5], pos: [0.0; 3], nrm: [0.0, 1.0, 0.0], view: [0.0, 0.0, 1.0], vcol: [1.0; 4], res: [64.0, 64.0] };
        let s = prog.material(&c, &mut vec![]);
        assert!(s.color[0] > 0.05 || s.color[1] > 0.05, "{:?}", s.color);
        assert!(g.nodes.iter().any(|n| n.op == "edgewear") && g.nodes.iter().any(|n| n.op == "mix"));
        // Sans la peinture : le rouge du paramètre
        let only = Stack { layers: vec![stack.layers[0].clone()] };
        let prog = crate::nodes::Program::compile(&flatten(&only, &root), &root, 64).map_err(|e| e.1).unwrap();
        let s = prog.material(&c, &mut vec![]);
        assert!(s.color[0] > 0.8 && s.color[1] < 0.2, "{:?}", s.color);
        // Masque peint à la main : la peinture verte n'apparaît que là où le canal 3 de l'objet est peint
        let mut hand = stack.clone();
        hand.layers[1].mask = Mask { kind: "peint".into(), paint: 2, ..Mask::default() };
        hand.layers[1].opacity = 1.0;
        let g = flatten(&hand, &root);
        assert!(g.nodes.iter().any(|n| n.op == "paintmask" && matches!(n.params.get("canal"), Some(Param::S(s)) if s == "3")));
        let prog = crate::nodes::Program::compile(&g, &root, 64).map_err(|e| e.1).unwrap();
        let at = |masks: [f32; 4]| prog.material(&crate::nodes::Ctx { masks, ..c }, &mut vec![]).color;
        let (bare, painted, half, other) = (at([0.0; 4]), at([0.0, 0.0, 1.0, 0.0]), at([0.0, 0.0, 0.5, 0.0]), at([1.0, 1.0, 0.0, 1.0]));
        assert!(bare[0] > 0.8 && bare[1] < 0.2, "{bare:?}");
        assert!(painted[0] < 0.2 && painted[1] > 0.7, "{painted:?}");
        assert!(half[0] > 0.3 && half[0] < 0.7 && half[1] > 0.3, "{half:?}");
        assert_eq!(other, bare, "les autres canaux ne comptent pas");
        // Le réglage s'enregistre avec la pile, et une pile d'avant se relit (canal 1 par défaut)
        let json = serde_json::to_string(&hand).unwrap();
        assert_eq!(serde_json::from_str::<Stack>(&json).unwrap(), hand);
        assert_eq!(serde_json::from_str::<Mask>(r#"{"kind":"peint","amount":0.5,"contrast":0.5,"scale":8.0}"#).unwrap().paint, 0);
        // Une pile d'avant les calques « graphe » (maître obligatoire, pas de graphe) se relit telle quelle
        let old: Stack = serde_json::from_str(r#"{"layers":[{"name":"métal","master":"metal.arymat","mask":{"kind":"plein","amount":0.5,"contrast":0.5,"scale":8.0}}]}"#).unwrap();
        assert!(old.layers[0].graph.is_none() && old.layers[0].master == "metal.arymat" && old.layers[0].visible);
        assert!(!serde_json::to_string(&old).unwrap().contains("graph"));
    }

    #[test]
    fn ancien_materiau_en_un_calque_graphe() {
        // Un matériau en nodes d'avant, avec un groupe et un cadre : rien ne se perd à l'aller-retour
        let mut old = Graph::new_material();
        let sel: std::collections::BTreeSet<u32> = old.nodes.iter().filter(|n| n.op == "ramp").map(|n| n.id).collect();
        crate::nodeflow::make_group(&mut old, &sel, "groupe").unwrap();
        old.frames.push(crate::nodes::Frame { rect: [0.0, 0.0, 300.0, 200.0], label: "cadre".into(), color: [90, 120, 200], note: "note".into() });
        crate::nodeflow::sync(&mut old);
        let json = serde_json::to_string(&old).unwrap();
        let st = to_stack(old.clone());
        let layers = &st.stack.as_ref().unwrap().layers;
        assert!(layers.len() == 1 && layers[0].graph.is_some() && layers[0].master.is_empty() && st.nodes.is_empty());
        // Ce qu'on écrit : exactement l'ancien fichier
        assert_eq!(serde_json::to_string(&*to_file(&st)).unwrap(), json);
        // Et le rendu est le même, groupe compris
        let c = crate::nodes::Ctx { masks: [0.0; 4], curv: 0.0, uv: [0.3, 0.6], pos: [0.1, 0.2, 0.3], nrm: [0.0, 1.0, 0.0], view: [0.0, 0.0, 1.0], vcol: [1.0; 4], res: [64.0, 64.0] };
        let look = |g: &Graph| crate::nodes::Program::compile(g, "", 64).map_err(|e| e.1).unwrap().material(&c, &mut vec![]).color;
        assert_eq!(look(&st), look(&old));
        // Dès que la pile dit autre chose (un nom, une opacité, un deuxième calque), elle s'écrit en pile
        let mut named = st.clone();
        named.stack.as_mut().unwrap().layers[0].name = "base".into();
        assert!(to_file(&named).stack.is_some());
        let mut faded = st.clone();
        faded.stack.as_mut().unwrap().layers[0].opacity = 0.5;
        assert!(to_file(&faded).stack.is_some());
        let mut two = st.clone();
        two.stack.as_mut().unwrap().layers.push(Layer::own("dessus", fill_graph([1.0, 0.0, 0.0])));
        let back: Graph = serde_json::from_str(&serde_json::to_string(&*to_file(&two)).unwrap()).unwrap();
        assert_eq!(back.stack, two.stack);
        // Le calque du dessus, opaque et posé partout, cache celui du dessous : son rouge exposé se règle
        assert!(look(&two)[0] > 0.95 && look(&two)[1] < 0.05);
        assert_eq!(exposed(&fill_graph([1.0, 0.0, 0.0])).iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), vec!["couleur", "rugosité", "métal"]);
        two.stack.as_mut().unwrap().layers[1].params.insert("couleur".into(), [0.0, 0.0, 1.0, 1.0]);
        assert!(look(&two)[2] > 0.95 && look(&two)[0] < 0.05);
        // Un matériau à calques d'avant (pile vide) et les autres genres de graphe ne changent pas
        let empty = Graph { stack: Some(Stack::default()), ..Graph::empty(GraphKind::Material) };
        assert!(to_stack(empty.clone()).stack == Some(Stack::default()) && to_file(&empty).stack.is_some());
        assert!(to_stack(Graph::new_compositor()).stack.is_none());
    }
}
