//! Atelier de matériaux (façon Substance Painter + Designer) : un matériau est une pile de calques.
//! Chaque calque est une *instance* d'un matériau maître (un graphe de nodes .arymat qui expose des
//! paramètres), posée selon un *masque* (plein, usure des arêtes, saleté des creux, poussière…).
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

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Layer {
    pub name: String,
    /// Matériau maître : chemin d'un .arymat dans le coffre
    pub master: String,
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

fn one() -> f32 {
    1.0
}
fn yes() -> bool {
    true
}

/// Où le calque se pose.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Mask {
    /// « plein », « arêtes », « creux », « dessus », « coulures », « rayures », « grunge », « bruit »
    pub kind: String,
    pub amount: f32,
    pub contrast: f32,
    pub scale: f32,
    #[serde(default)]
    pub invert: bool,
}

impl Default for Mask {
    fn default() -> Self {
        Mask { kind: "plein".into(), amount: 0.5, contrast: 0.5, scale: 8.0, invert: false }
    }
}

/// Les masques proposés : identifiant, nom affiché, node générateur.
pub const MASKS: [(&str, &str, &str); 8] = [
    ("plein", "partout", ""),
    ("arêtes", "usure des arêtes", "edgewear"),
    ("creux", "saleté des creux", "cavdirt"),
    ("dessus", "poussière sur le dessus", "dust"),
    ("coulures", "coulures", "drips"),
    ("rayures", "rayures d'usure", "scratches"),
    ("grunge", "saleté (grunge)", "grunge"),
    ("bruit", "bruit", "noise"),
];

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

/// Les paramètres qu'un maître expose.
pub fn exposed(g: &Graph) -> Vec<Exposed> {
    g.nodes
        .iter()
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

#[derive(Clone, Copy)]
enum Src {
    Const([f32; 4]),
    Link(u32, usize),
}

/// La pile ramenée à un seul graphe de matériau.
pub fn flatten(stack: &Stack, root: &str) -> Graph {
    let mut g = Graph { kind: GraphKind::Material, nodes: vec![], links: vec![], next_id: 0, frames: vec![], stack: None };
    let out = g.add("mat_out", [1400.0, 0.0]);
    let socks: Vec<bool> = crate::nodes::def("mat_out").map(|d| d.inputs.iter().map(|i| matches!(i.1, crate::nodes::Sock::Color)).collect()).unwrap_or_default();
    let defaults: Vec<[f32; 4]> = g.nodes[0].values.clone();
    let mut cur: Vec<Src> = defaults.iter().map(|v| Src::Const(*v)).collect();
    for (li, layer) in stack.layers.iter().enumerate() {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
        }
        let Some(master) = load_master(root, &layer.master) else { continue };
        let master = master.effective().into_owned();
        let Some(mo) = master.nodes.iter().find(|n| n.op == "mat_out").cloned() else { continue };
        let off = g.nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1;
        let dy = li as f32 * 600.0;
        for n in &master.nodes {
            if n.id == mo.id {
                continue;
            }
            let mut n2 = n.clone();
            n2.id += off;
            n2.pos = [n.pos[0], n.pos[1] + dy];
            n2.preview = false;
            if n.op == "param" {
                if let Some(v) = layer.params.get(&param_name(n)) {
                    if let Some(x) = n2.values.first_mut() {
                        *x = *v;
                    }
                }
            }
            g.nodes.push(n2);
        }
        let mut src: Vec<Src> = mo.values.iter().map(|v| Src::Const(*v)).collect();
        src.resize(cur.len(), Src::Const([0.0; 4]));
        for l in &master.links {
            if l.to == mo.id {
                if l.inp < src.len() {
                    src[l.inp] = Src::Link(l.from + off, l.out);
                }
            } else if l.from != mo.id {
                g.links.push(Link { from: l.from + off, out: l.out, to: l.to + off, inp: l.inp });
            }
        }
        g.next_id = g.nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1;
        // Facteur du calque : masque × opacité
        let x0 = -400.0;
        let mut factor = Src::Const([layer.opacity; 4]);
        if let Some((_, _, op)) = MASKS.iter().find(|m| m.0 == layer.mask.kind).filter(|m| !m.2.is_empty()) {
            let mk = g.add(op, [x0, dy]);
            set_named(&mut g, mk, "quantité", layer.mask.amount);
            set_named(&mut g, mk, "contraste", layer.mask.contrast);
            set_named(&mut g, mk, "échelle du bruit", layer.mask.scale);
            set_named(&mut g, mk, "échelle", layer.mask.scale);
            let mut m = Src::Link(mk, 0);
            if layer.mask.invert {
                let inv = g.add("math", [x0 + 150.0, dy]);
                set_choice(&mut g, inv, "opération", "soustraire");
                set_value(&mut g, inv, 0, [1.0; 4]);
                wire(&mut g, m, inv, 1);
                m = Src::Link(inv, 0);
            }
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
                Layer { name: "métal".into(), master: "metal.arymat".into(), params, mask: Mask::default(), opacity: 1.0, visible: true },
                Layer { name: "peinture".into(), master: "paint.arymat".into(), params: BTreeMap::new(), mask: Mask { kind: "arêtes".into(), invert: true, ..Mask::default() }, opacity: 0.8, visible: true },
            ],
        };
        let g = flatten(&stack, &root);
        // Le graphe se compile, et la couleur du calque du dessous vient du paramètre redéfini
        let prog = crate::nodes::Program::compile(&g, &root, 64).map_err(|e| e.1).unwrap();
        let c = crate::nodes::Ctx { curv: 0.0, uv: [0.5, 0.5], pos: [0.0; 3], nrm: [0.0, 1.0, 0.0], view: [0.0, 0.0, 1.0], vcol: [1.0; 4], res: [64.0, 64.0] };
        let s = prog.material(&c, &mut vec![]);
        assert!(s.color[0] > 0.05 || s.color[1] > 0.05, "{:?}", s.color);
        assert!(g.nodes.iter().any(|n| n.op == "edgewear") && g.nodes.iter().any(|n| n.op == "mix"));
        // Sans la peinture : le rouge du paramètre
        let only = Stack { layers: vec![stack.layers[0].clone()] };
        let prog = crate::nodes::Program::compile(&flatten(&only, &root), &root, 64).map_err(|e| e.1).unwrap();
        let s = prog.material(&c, &mut vec![]);
        assert!(s.color[0] > 0.8 && s.color[1] < 0.2, "{:?}", s.color);
    }
}
