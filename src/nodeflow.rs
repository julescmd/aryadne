//! Ce qui fait d'un graphe plus qu'une liste de nodes : prises sur mesure, groupes réutilisables,
//! zones de répétition et de simulation, renvois, nodes muets.
//!
//! Tout cela est une commodité d'écriture. Avant le calcul, `lower` ramène le graphe à sa forme
//! la plus simple : les groupes sont dépliés, les répétitions déroulées, les renvois et les nodes
//! muets retirés. Les moteurs (matériau, image, géométrie) ne voient donc que des nodes ordinaires,
//! et ce qui marchait avant marche pareil.

use crate::nodes::{def, Def, Frame, Graph, GraphKind, Link, Node, Param, Sock, V};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Une prise sur mesure : son nom, son type, sa valeur quand rien n'y est branché.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Port {
    pub name: String,
    pub sock: Sock,
    #[serde(default)]
    pub value: V,
}

/// Un groupe : un sous-graphe qu'on pose comme un seul node, autant de fois qu'on veut.
#[derive(Clone, Serialize, Deserialize)]
pub struct Group {
    pub name: String,
    #[serde(default)]
    pub inputs: Vec<Port>,
    #[serde(default)]
    pub outputs: Vec<Port>,
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    #[serde(default)]
    pub frames: Vec<Frame>,
}

/// Réglages de lecture d'un graphe animé.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub struct TimeRange {
    pub fps: f32,
    pub start: i32,
    pub end: i32,
}

impl Default for TimeRange {
    fn default() -> Self {
        TimeRange { fps: 24.0, start: 1, end: 120 }
    }
}

/// Nom affiché de la prise libre : y brancher un lien crée une vraie prise.
pub const VIRTUAL: &str = "nouvelle prise";

pub fn is_dynamic(op: &str) -> bool {
    matches!(op, "group" | "group_in" | "group_out" | "repeat_in" | "repeat_out" | "sim_in" | "sim_out")
}

/// Nodes qui vont par deux et délimitent une zone.
pub fn is_zone(op: &str) -> bool {
    matches!(op, "repeat_in" | "repeat_out" | "sim_in" | "sim_out")
}

fn pstr<'a>(n: &'a Node, k: &str) -> &'a str {
    match n.params.get(k) {
        Some(Param::S(s)) => s,
        _ => "",
    }
}

fn pnum(n: &Node, k: &str) -> Option<f32> {
    match n.params.get(k) {
        Some(Param::F(v)) => Some(*v),
        _ => None,
    }
}

// ---------- définitions sur mesure ----------

fn intern(s: &str) -> &'static str {
    static NAMES: std::sync::Mutex<Option<HashSet<&'static str>>> = std::sync::Mutex::new(None);
    let mut g = NAMES.lock().unwrap();
    let set = g.get_or_insert_with(HashSet::new);
    if let Some(x) = set.get(s) {
        return x;
    }
    let l: &'static str = Box::leak(s.to_string().into_boxed_str());
    set.insert(l);
    l
}

/// Définition d'un node : celle de son type, ou une définition construite d'après ses propres prises.
/// Les définitions sur mesure sont gardées une fois pour toutes (une par signature), d'où le `'static`.
pub fn node_def(n: &Node) -> Option<&'static Def> {
    if !is_dynamic(&n.op) {
        return def(&n.op);
    }
    static DEFS: std::sync::Mutex<Option<HashMap<String, &'static Def>>> = std::sync::Mutex::new(None);
    let mut sig = format!("{}|{}", n.op, if n.op == "group" { pstr(n, "groupe") } else { "" });
    for (tag, ports) in [("i", &n.ins), ("o", &n.outs)] {
        for p in ports {
            sig += &format!("|{tag}:{}:{:?}:{:?}", p.name, p.sock, p.value);
        }
    }
    let mut g = DEFS.lock().unwrap();
    let map = g.get_or_insert_with(HashMap::new);
    if let Some(d) = map.get(&sig) {
        return Some(d);
    }
    let ins = |ports: &[Port]| -> Vec<(&'static str, Sock, V)> { ports.iter().map(|p| (intern(&p.name), p.sock, p.value)).collect() };
    let outs = |ports: &[Port]| -> Vec<(&'static str, Sock)> { ports.iter().map(|p| (intern(&p.name), p.sock)).collect() };
    let free_in = (VIRTUAL, Sock::Any, [0.0; 4]);
    let free_out = (VIRTUAL, Sock::Any);
    let (label, cat, inputs, outputs): (&'static str, &'static str, Vec<(&'static str, Sock, V)>, Vec<(&'static str, Sock)>) = match n.op.as_str() {
        "group" => (intern(pstr(n, "groupe")), "groupe", ins(&n.ins), outs(&n.outs)),
        "group_in" => ("Entrées du groupe", "groupe", vec![], outs(&n.outs).into_iter().chain([free_out]).collect()),
        "group_out" => ("Sorties du groupe", "groupe", ins(&n.ins).into_iter().chain([free_in]).collect(), vec![]),
        "repeat_in" => ("Répéter", "répétition", [("itérations", Sock::Int, [4.0; 4])].into_iter().chain(ins(&n.ins)).chain([free_in]).collect(), [("itération", Sock::Int)].into_iter().chain(outs(&n.ins)).collect()),
        "repeat_out" => ("Fin de la répétition", "répétition", ins(&n.ins).into_iter().chain([free_in]).collect(), outs(&n.ins)),
        "sim_in" => ("Simulation", "simulation", ins(&n.ins).into_iter().chain([free_in]).collect(), [("pas de temps", Sock::Float)].into_iter().chain(outs(&n.ins)).collect()),
        _ => ("Fin de la simulation", "simulation", ins(&n.ins).into_iter().chain([free_in]).collect(), outs(&n.ins)),
    };
    let d: &'static Def = Box::leak(Box::new(Def { op: intern(&n.op), label, cat, inputs: Box::leak(inputs.into_boxed_slice()), outputs: Box::leak(outputs.into_boxed_slice()), params: &[], mat: true, comp: true }));
    map.insert(sig, d);
    Some(d)
}

/// Rang de la prise libre parmi les entrées de ce node, s'il en a une.
pub fn virtual_input(n: &Node) -> Option<usize> {
    match n.op.as_str() {
        "group_out" | "repeat_out" | "sim_in" | "sim_out" => Some(n.ins.len()),
        "repeat_in" => Some(n.ins.len() + 1),
        _ => None,
    }
}

/// Rang de la prise libre parmi les sorties de ce node, s'il en a une.
pub fn virtual_output(n: &Node) -> Option<usize> {
    (n.op == "group_in").then_some(n.outs.len())
}

/// Décalage entre le rang d'une prise d'état et son rang parmi les entrées / les sorties du node.
fn state_offset(op: &str) -> (usize, usize) {
    match op {
        "repeat_in" => (1, 1),
        "sim_in" => (0, 1),
        _ => (0, 0),
    }
}

/// L'autre bout d'une zone (début pour une fin, fin pour un début).
pub fn zone_pair(g: &Graph, id: u32) -> Option<u32> {
    let n = g.nodes.iter().find(|n| n.id == id)?;
    let other = match n.op.as_str() {
        "repeat_in" => "repeat_out",
        "repeat_out" => "repeat_in",
        "sim_in" => "sim_out",
        "sim_out" => "sim_in",
        _ => return None,
    };
    let z = pnum(n, "zone")?;
    g.nodes.iter().find(|m| m.op == other && pnum(m, "zone") == Some(z)).map(|m| m.id)
}

/// Pose une zone (ses deux nodes) avec une première prise d'état. `kind` : "repeat" ou "sim".
pub fn add_zone(g: &mut Graph, kind: &str, pos: [f32; 2]) -> (u32, u32) {
    let (a, b) = if kind == "sim" { ("sim_in", "sim_out") } else { ("repeat_in", "repeat_out") };
    let zone = g.nodes.iter().filter_map(|n| pnum(n, "zone")).fold(0.0f32, f32::max) + 1.0;
    let port = if g.kind == GraphKind::Geometry { Port { name: "géométrie".into(), sock: Sock::Geo, value: [0.0; 4] } } else { Port { name: "valeur".into(), sock: Sock::Color, value: [0.0, 0.0, 0.0, 1.0] } };
    let ia = g.add(a, pos);
    let ib = g.add(b, [pos[0] + 460.0, pos[1]]);
    for id in [ia, ib] {
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) {
            n.params.insert("zone".into(), Param::F(zone));
            n.ins = vec![port.clone()];
            n.preview = false;
        }
    }
    sync(g);
    (ia, ib)
}

/// Les nodes à l'intérieur d'une zone : en aval du début et en amont de la fin.
pub fn zone_body(g: &Graph, begin: u32, end: u32) -> BTreeSet<u32> {
    let walk = |start: u32, down: bool| -> HashSet<u32> {
        let mut seen = HashSet::new();
        let mut stack = vec![start];
        while let Some(n) = stack.pop() {
            for l in &g.links {
                let (a, b) = if down { (l.from, l.to) } else { (l.to, l.from) };
                if a == n && seen.insert(b) {
                    stack.push(b);
                }
            }
        }
        seen
    };
    let down = walk(begin, true);
    let up = walk(end, false);
    down.intersection(&up).copied().filter(|id| *id != begin && *id != end).collect()
}

fn fill_values(n: &mut Node) {
    if let Some(d) = node_def(n) {
        while n.values.len() < d.inputs.len() {
            n.values.push(d.inputs[n.values.len()].2);
        }
    }
}

/// Remet d'accord les prises : chaque node de groupe suit l'interface de son groupe, chaque fin
/// de zone suit son début. À appeler après un chargement ou une modification d'interface.
pub fn sync(g: &mut Graph) {
    let faces: Vec<(String, Vec<Port>, Vec<Port>)> = g.groups.iter().map(|gr| (gr.name.clone(), gr.inputs.clone(), gr.outputs.clone())).collect();
    fn level(nodes: &mut [Node], faces: &[(String, Vec<Port>, Vec<Port>)], own: Option<usize>) {
        // Zones : la fin suit le début
        let begins: Vec<(String, f32, Vec<Port>)> = nodes.iter().filter(|n| matches!(n.op.as_str(), "repeat_in" | "sim_in")).filter_map(|n| pnum(n, "zone").map(|z| (n.op.clone(), z, n.ins.clone()))).collect();
        for n in nodes.iter_mut() {
            match n.op.as_str() {
                "group" => {
                    if let Some(f) = faces.iter().find(|f| f.0 == pstr(n, "groupe")) {
                        n.ins = f.1.clone();
                        n.outs = f.2.clone();
                    }
                }
                "group_in" => {
                    if let Some(k) = own {
                        n.outs = faces[k].1.clone();
                    }
                }
                "group_out" => {
                    if let Some(k) = own {
                        n.ins = faces[k].2.clone();
                    }
                }
                "repeat_out" | "sim_out" => {
                    let want = if n.op == "repeat_out" { "repeat_in" } else { "sim_in" };
                    if let Some(b) = begins.iter().find(|b| b.0 == want && Some(b.1) == pnum(n, "zone")) {
                        n.ins = b.2.clone();
                    }
                }
                _ => {}
            }
            if is_dynamic(&n.op) {
                fill_values(n);
            }
        }
    }
    level(&mut g.nodes, &faces, None);
    for k in 0..g.groups.len() {
        level(&mut g.groups[k].nodes, &faces, Some(k));
    }
}

/// Ajoute une prise d'état à une zone (aux deux bouts). Renvoie son rang.
pub fn zone_add_port(g: &mut Graph, id: u32, port: Port) -> Option<usize> {
    let other = zone_pair(g, id)?;
    let mut k = 0;
    for n in g.nodes.iter_mut().filter(|n| n.id == id || n.id == other) {
        // La valeur de la prise libre ne doit pas devenir celle de la nouvelle prise
        let (off, _) = state_offset(&n.op);
        n.values.truncate(off + n.ins.len());
        n.ins.push(port.clone());
        k = n.ins.len() - 1;
        fill_values(n);
    }
    Some(k)
}

/// Retire une prise d'état d'une zone, avec ses liens ; les prises suivantes gardent les leurs.
pub fn zone_remove_port(g: &mut Graph, id: u32, k: usize) {
    let Some(other) = zone_pair(g, id) else { return };
    for nid in [id, other] {
        let Some(n) = g.nodes.iter_mut().find(|n| n.id == nid) else { continue };
        if k >= n.ins.len() {
            continue;
        }
        let (oi, oo) = state_offset(&n.op);
        n.ins.remove(k);
        if oi + k < n.values.len() {
            n.values.remove(oi + k);
        }
        shift_links(&mut g.links, nid, Some(oi + k), Some(oo + k));
    }
}

/// Après la suppression d'une prise : retire ses liens et décale ceux des prises suivantes.
fn shift_links(links: &mut Vec<Link>, node: u32, inp: Option<usize>, out: Option<usize>) {
    if let Some(k) = inp {
        links.retain(|l| !(l.to == node && l.inp == k));
        for l in links.iter_mut().filter(|l| l.to == node && l.inp > k) {
            l.inp -= 1;
        }
    }
    if let Some(k) = out {
        links.retain(|l| !(l.from == node && l.out == k));
        for l in links.iter_mut().filter(|l| l.from == node && l.out > k) {
            l.out -= 1;
        }
    }
}

// ---------- groupes ----------

/// Chaque niveau du document : la racine, puis le contenu de chaque groupe.
fn levels(g: &mut Graph) -> Vec<(&mut Vec<Node>, &mut Vec<Link>)> {
    let mut v = vec![(&mut g.nodes, &mut g.links)];
    for gr in g.groups.iter_mut() {
        v.push((&mut gr.nodes, &mut gr.links));
    }
    v
}

/// Ajoute une prise à l'interface d'un groupe. Renvoie son rang.
pub fn group_add_port(g: &mut Graph, name: &str, input: bool, port: Port) -> Option<usize> {
    let gr = g.groups.iter_mut().find(|x| x.name == name)?;
    let list = if input { &mut gr.inputs } else { &mut gr.outputs };
    list.push(port);
    let k = list.len() - 1;
    // Les valeurs en trop (celle de la prise libre) ne doivent pas devenir celle de la nouvelle prise
    for (nodes, _) in levels(g) {
        for n in nodes.iter_mut() {
            if (input && n.op == "group" && pstr(n, "groupe") == name) || (!input && n.op == "group_out") {
                n.values.truncate(k);
            }
        }
    }
    sync(g);
    Some(k)
}

/// Retire une prise de l'interface d'un groupe, dedans comme sur chaque node qui pose ce groupe.
pub fn group_remove_port(g: &mut Graph, name: &str, input: bool, k: usize) {
    let Some(gi) = g.groups.iter().position(|x| x.name == name) else { return };
    {
        let gr = &mut g.groups[gi];
        let list = if input { &mut gr.inputs } else { &mut gr.outputs };
        if k >= list.len() {
            return;
        }
        list.remove(k);
        // Dedans : les nodes d'entrée ou de sortie du groupe
        let ids: Vec<u32> = gr.nodes.iter().filter(|n| n.op == if input { "group_in" } else { "group_out" }).map(|n| n.id).collect();
        for id in ids {
            if input {
                shift_links(&mut gr.links, id, None, Some(k));
            } else {
                shift_links(&mut gr.links, id, Some(k), None);
                if let Some(n) = gr.nodes.iter_mut().find(|n| n.id == id) {
                    if k < n.values.len() {
                        n.values.remove(k);
                    }
                }
            }
        }
    }
    // Dehors : chaque node qui pose ce groupe, à tous les niveaux
    for (nodes, links) in levels(g) {
        let ids: Vec<u32> = nodes.iter().filter(|n| n.op == "group" && pstr(n, "groupe") == name).map(|n| n.id).collect();
        for id in ids {
            if input {
                shift_links(links, id, Some(k), None);
                if let Some(n) = nodes.iter_mut().find(|n| n.id == id) {
                    if k < n.values.len() {
                        n.values.remove(k);
                    }
                }
            } else {
                shift_links(links, id, None, Some(k));
            }
        }
    }
    sync(g);
}

/// Renomme un groupe partout où il est posé. Refuse un nom déjà pris.
pub fn group_rename(g: &mut Graph, old: &str, new: &str) -> bool {
    let new = new.trim();
    if new.is_empty() || new == old || g.groups.iter().any(|x| x.name == new) {
        return false;
    }
    for gr in g.groups.iter_mut().filter(|x| x.name == old) {
        gr.name = new.to_string();
    }
    for (nodes, _) in levels(g) {
        for n in nodes.iter_mut().filter(|n| n.op == "group" && pstr(n, "groupe") == old) {
            n.params.insert("groupe".into(), Param::S(new.to_string()));
        }
    }
    true
}

/// Nombre de fois qu'un groupe est posé dans le document.
pub fn group_uses(g: &Graph, name: &str) -> usize {
    let count = |nodes: &[Node]| nodes.iter().filter(|n| n.op == "group" && pstr(n, "groupe") == name).count();
    count(&g.nodes) + g.groups.iter().map(|gr| count(&gr.nodes)).sum::<usize>()
}

/// Les groupes dont ces nodes ont besoin : ceux qu'ils posent, et ceux que ces groupes posent à leur tour.
pub fn groups_used(g: &Graph, nodes: &[Node]) -> Vec<Group> {
    let mut names: Vec<String> = vec![];
    let mut todo: Vec<String> = nodes.iter().filter(|n| n.op == "group").map(|n| pstr(n, "groupe").to_string()).collect();
    while let Some(name) = todo.pop() {
        if names.contains(&name) {
            continue;
        }
        if let Some(gr) = g.groups.iter().find(|x| x.name == name) {
            todo.extend(gr.nodes.iter().filter(|n| n.op == "group").map(|n| pstr(n, "groupe").to_string()));
            names.push(name);
        }
    }
    g.groups.iter().filter(|x| names.contains(&x.name)).cloned().collect()
}

/// Pose un node du groupe `name` dans le niveau courant.
pub fn add_group_node(g: &mut Graph, name: &str, pos: [f32; 2]) -> Option<u32> {
    let gr = g.groups.iter().find(|x| x.name == name)?;
    let (ins, outs) = (gr.inputs.clone(), gr.outputs.clone());
    let id = g.add("group", pos);
    let n = g.nodes.iter_mut().find(|n| n.id == id)?;
    n.params.insert("groupe".into(), Param::S(name.to_string()));
    n.values = ins.iter().map(|p| p.value).collect();
    n.ins = ins;
    n.outs = outs;
    Some(id)
}

fn free_name(g: &Graph, base: &str) -> String {
    let mut name = base.to_string();
    let mut k = 2;
    while g.groups.iter().any(|x| x.name == name) {
        name = format!("{base} {k}");
        k += 1;
    }
    name
}

/// Réunit les nodes choisis en un groupe, et les remplace par un seul node relié pareil.
/// Les liens qui entraient deviennent les entrées du groupe, ceux qui sortaient ses sorties.
pub fn make_group(g: &mut Graph, sel: &BTreeSet<u32>, base: &str) -> Result<u32, String> {
    let ids: BTreeSet<u32> = sel.iter().copied().filter(|id| g.nodes.iter().any(|n| n.id == *id && !matches!(n.op.as_str(), "mat_out" | "comp_out" | "geo_out" | "group_in" | "group_out"))).collect();
    if ids.is_empty() {
        return Err("choisis d'abord les nodes à grouper".into());
    }
    for id in &ids {
        if let Some(o) = zone_pair(g, *id) {
            if !ids.contains(&o) {
                return Err("une zone se groupe en entier : choisis aussi son autre bout".into());
            }
        }
    }
    let name = free_name(g, base);
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for n in g.nodes.iter().filter(|n| ids.contains(&n.id)) {
        for k in 0..2 {
            lo[k] = lo[k].min(n.pos[k]);
            hi[k] = hi[k].max(n.pos[k]);
        }
    }
    let port_of = |g: &Graph, id: u32, k: usize, input: bool| -> (String, Sock, V) {
        let n = g.nodes.iter().find(|n| n.id == id);
        let d = n.and_then(node_def);
        if input {
            d.and_then(|d| d.inputs.get(k)).map(|i| (i.0.to_string(), i.1, n.and_then(|n| n.values.get(k).copied()).unwrap_or(i.2))).unwrap_or(("valeur".into(), Sock::Float, [0.0; 4]))
        } else {
            d.and_then(|d| d.outputs.get(k)).map(|o| (o.0.to_string(), o.1, [0.0; 4])).unwrap_or(("valeur".into(), Sock::Float, [0.0; 4]))
        }
    };
    let mut body = Group { name: name.clone(), inputs: vec![], outputs: vec![], nodes: g.nodes.iter().filter(|n| ids.contains(&n.id)).cloned().collect(), links: vec![], frames: vec![] };
    let gin = body.nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1;
    let gout = gin + 1;
    // Une entrée par source extérieure, une sortie par prise qui sert dehors
    let mut in_src: Vec<(u32, usize)> = vec![];
    let mut out_src: Vec<(u32, usize)> = vec![];
    let mut outer: Vec<Link> = vec![];
    let inst = g.next_id.max(g.nodes.iter().map(|n| n.id + 1).max().unwrap_or(0));
    for l in g.links.clone() {
        match (ids.contains(&l.from), ids.contains(&l.to)) {
            (true, true) => body.links.push(l),
            (false, true) => {
                let k = in_src.iter().position(|s| *s == (l.from, l.out)).unwrap_or_else(|| {
                    in_src.push((l.from, l.out));
                    let (pname, sock, value) = port_of(g, l.to, l.inp, true);
                    // Un renvoi n'a pas de type : on prend celui de la source
                    let sock = if sock == Sock::Any { port_of(g, l.from, l.out, false).1 } else { sock };
                    let pname = if pname.is_empty() { port_of(g, l.from, l.out, false).0 } else { pname };
                    body.inputs.push(Port { name: pname, sock, value });
                    outer.push(Link { from: l.from, out: l.out, to: inst, inp: in_src.len() - 1 });
                    in_src.len() - 1
                });
                body.links.push(Link { from: gin, out: k, to: l.to, inp: l.inp });
            }
            (true, false) => {
                let k = out_src.iter().position(|s| *s == (l.from, l.out)).unwrap_or_else(|| {
                    out_src.push((l.from, l.out));
                    let (pname, sock, _) = port_of(g, l.from, l.out, false);
                    let (tname, tsock, _) = port_of(g, l.to, l.inp, true);
                    body.outputs.push(Port { name: if pname.is_empty() { tname } else { pname }, sock: if sock == Sock::Any { tsock } else { sock }, value: [0.0; 4] });
                    body.links.push(Link { from: l.from, out: l.out, to: gout, inp: out_src.len() - 1 });
                    out_src.len() - 1
                });
                outer.push(Link { from: inst, out: k, to: l.to, inp: l.inp });
            }
            (false, false) => {}
        }
    }
    // Un groupe sans sortie ne servirait à rien : on expose la première sortie du dernier node
    if body.outputs.is_empty() {
        if let Some((id, name, sock)) = body.nodes.iter().rev().find_map(|n| node_def(n).and_then(|d| d.outputs.first()).map(|o| (n.id, o.0.to_string(), o.1))) {
            body.outputs.push(Port { name, sock, value: [0.0; 4] });
            body.links.push(Link { from: id, out: 0, to: gout, inp: 0 });
        }
    }
    let blank = |id: u32, op: &str, pos: [f32; 2]| Node { id, op: op.into(), pos, values: vec![], params: Default::default(), muted: false, collapsed: false, preview: false, label: String::new(), ins: vec![], outs: vec![] };
    body.nodes.push(blank(gin, "group_in", [lo[0] - 260.0, lo[1]]));
    body.nodes.push(blank(gout, "group_out", [hi[0] + 260.0, lo[1]]));
    // Dehors : les nodes partent, un node de groupe prend leur place
    g.nodes.retain(|n| !ids.contains(&n.id));
    g.links.retain(|l| !ids.contains(&l.from) && !ids.contains(&l.to));
    g.groups.push(body);
    let id = add_group_node(g, &name, [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5]).ok_or("groupe introuvable")?;
    debug_assert_eq!(id, inst);
    g.links.extend(outer);
    sync(g);
    Ok(id)
}

/// Remet à plat un node de groupe : son contenu revient dans le niveau courant, relié pareil.
/// Renvoie les nodes ainsi posés.
pub fn ungroup(g: &mut Graph, inst: u32) -> Result<Vec<u32>, String> {
    let before: HashSet<u32> = g.nodes.iter().map(|n| n.id).collect();
    let groups = g.groups.clone();
    let mut origin = HashMap::new();
    let mut chain = HashMap::new();
    let mut next = g.next_id.max(g.nodes.iter().map(|n| n.id + 1).max().unwrap_or(0));
    expand_one(g, inst, &groups, &mut next, &mut origin, &mut chain, true).map_err(|e| e.1)?;
    g.next_id = next;
    Ok(g.nodes.iter().map(|n| n.id).filter(|id| !before.contains(id)).collect())
}

// ---------- mise à plat avant le calcul ----------

/// D'où vient une valeur : d'une sortie, d'une constante, ou de nulle part (le node garde son réglage).
#[derive(Clone, Copy, Debug)]
enum Source {
    Link(u32, usize),
    Const(V),
    Nothing,
}

fn source_of(g: &Graph, node: &Node, inp: usize) -> Source {
    match g.links.iter().find(|l| l.to == node.id && l.inp == inp) {
        Some(l) => Source::Link(l.from, l.out),
        None => Source::Const(node.values.get(inp).copied().or_else(|| node_def(node).and_then(|d| d.inputs.get(inp)).map(|i| i.2)).unwrap_or([0.0; 4])),
    }
}

fn set_const(g: &mut Graph, to: u32, inp: usize, v: V) {
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == to) {
        fill_values(n);
        while n.values.len() <= inp {
            n.values.push([0.0; 4]);
        }
        n.values[inp] = v;
    }
}

fn feed(g: &mut Graph, to: u32, inp: usize, src: Source) {
    g.links.retain(|l| !(l.to == to && l.inp == inp));
    match src {
        Source::Link(from, out) => g.links.push(Link { from, out, to, inp }),
        Source::Const(v) => set_const(g, to, inp, v),
        Source::Nothing => {}
    }
}

/// Tout ce qui lisait la sortie `out` de `node` lit désormais `src`.
fn rewire(g: &mut Graph, node: u32, out: usize, src: Source) {
    let users: Vec<(u32, usize)> = g.links.iter().filter(|l| l.from == node && l.out == out).map(|l| (l.to, l.inp)).collect();
    for (to, inp) in users {
        feed(g, to, inp, src);
    }
}

/// Graphe mis à plat, et de quel node d'origine vient chaque node créé (pour situer les erreurs).
pub struct Lowered {
    pub graph: Graph,
    origin: HashMap<u32, u32>,
}

impl Lowered {
    /// Le node du graphe de départ dont vient ce node.
    pub fn origin(&self, id: u32) -> u32 {
        self.origin.get(&id).copied().unwrap_or(id)
    }
}

type Fail = (u32, String);

fn expand_one(w: &mut Graph, inst_id: u32, groups: &[Group], next: &mut u32, origin: &mut HashMap<u32, u32>, chain: &mut HashMap<u32, Vec<String>>, keep_layout: bool) -> Result<(), Fail> {
    let Some(inst) = w.nodes.iter().find(|n| n.id == inst_id).cloned() else { return Ok(()) };
    let top = origin.get(&inst.id).copied().unwrap_or(inst.id);
    let name = pstr(&inst, "groupe").to_string();
    let Some(grp) = groups.iter().find(|x| x.name == name) else { return Err((top, format!("groupe introuvable : {name}"))) };
    let mut ch = chain.get(&inst.id).cloned().unwrap_or_default();
    if ch.contains(&name) {
        return Err((top, format!("le groupe « {name} » se contient lui-même")));
    }
    ch.push(name);
    let is_in = |id: u32| grp.nodes.iter().any(|n| n.id == id && n.op == "group_in");
    let out_node = grp.nodes.iter().find(|n| n.op == "group_out");
    let is_out = |id: u32| out_node.is_some_and(|n| n.id == id);
    let in_src: Vec<Source> = (0..grp.inputs.len())
        .map(|k| match w.links.iter().find(|l| l.to == inst.id && l.inp == k) {
            Some(l) => Source::Link(l.from, l.out),
            None => Source::Const(inst.values.get(k).copied().unwrap_or(grp.inputs[k].value)),
        })
        .collect();
    // Centre du contenu, pour le poser autour du node de groupe quand on le remet à plat
    let inner: Vec<&Node> = grp.nodes.iter().filter(|n| !matches!(n.op.as_str(), "group_in" | "group_out")).collect();
    let c = if inner.is_empty() { [0.0; 2] } else { [inner.iter().map(|n| n.pos[0]).sum::<f32>() / inner.len() as f32, inner.iter().map(|n| n.pos[1]).sum::<f32>() / inner.len() as f32] };
    let mut map: HashMap<u32, u32> = HashMap::new();
    // Les zones du groupe gardent leurs paires, sans se mêler à celles du dehors
    let zone_shift = w.nodes.iter().filter_map(|n| pnum(n, "zone")).fold(0.0f32, f32::max);
    for bn in inner {
        let id = *next;
        *next += 1;
        map.insert(bn.id, id);
        let mut n = bn.clone();
        n.id = id;
        if keep_layout {
            n.pos = [inst.pos[0] + bn.pos[0] - c[0], inst.pos[1] + bn.pos[1] - c[1]];
        }
        if let Some(z) = pnum(&n, "zone") {
            n.params.insert("zone".into(), Param::F(z + zone_shift));
        }
        w.nodes.push(n);
        origin.insert(id, top);
        chain.insert(id, ch.clone());
    }
    let mut out_src: Vec<Source> = grp.outputs.iter().enumerate().map(|(k, p)| Source::Const(out_node.and_then(|n| n.values.get(k).copied()).unwrap_or(p.value))).collect();
    for l in &grp.links {
        match (is_in(l.from), is_out(l.to)) {
            (false, false) => {
                if let (Some(&a), Some(&b)) = (map.get(&l.from), map.get(&l.to)) {
                    w.links.push(Link { from: a, out: l.out, to: b, inp: l.inp });
                }
            }
            (true, false) => {
                if let (Some(src), Some(&b)) = (in_src.get(l.out), map.get(&l.to)) {
                    feed(w, b, l.inp, *src);
                }
            }
            (false, true) => {
                if let (Some(&a), Some(slot)) = (map.get(&l.from), out_src.get_mut(l.inp)) {
                    *slot = Source::Link(a, l.out);
                }
            }
            (true, true) => {
                if let (Some(src), Some(slot)) = (in_src.get(l.out).copied(), out_src.get_mut(l.inp)) {
                    *slot = src;
                }
            }
        }
    }
    for (k, src) in out_src.iter().enumerate() {
        rewire(w, inst.id, k, *src);
    }
    w.remove(inst.id);
    Ok(())
}

/// Déroule une zone de répétition : son contenu est recopié autant de fois qu'il y a d'itérations,
/// chaque copie lisant l'état laissé par la précédente.
fn unroll(w: &mut Graph, begin: u32, end: u32, next: &mut u32, origin: &mut HashMap<u32, u32>) -> Result<(), Fail> {
    let top = origin.get(&end).copied().unwrap_or(end);
    let b = w.nodes.iter().find(|n| n.id == begin).cloned().unwrap();
    let e = w.nodes.iter().find(|n| n.id == end).cloned().unwrap();
    // Le nombre d'itérations doit être connu avant le calcul : une valeur réglée à la main, ou un node « Valeur »
    let count = match source_of(w, &b, 0) {
        Source::Const(v) => v[0],
        Source::Link(from, _) => match w.nodes.iter().find(|n| n.id == from) {
            Some(n) if n.op == "value" && !w.links.iter().any(|l| l.to == n.id) => n.values.first().map(|v| v[0]).unwrap_or(0.0),
            _ => return Err((origin.get(&begin).copied().unwrap_or(begin), "le nombre d'itérations doit être un nombre fixe ici : règle-le sur le node, sans lien".into())),
        },
        Source::Nothing => 0.0,
    };
    let count = count.round().clamp(0.0, 256.0) as usize;
    let body = zone_body(w, begin, end);
    if body.len() * count + w.nodes.len() > 20000 {
        return Err((top, "cette répétition donnerait un graphe trop gros : baisse le nombre d'itérations".into()));
    }
    let ports = b.ins.len();
    // État avant la première itération : ce qui entre dans le début de la zone
    let mut state: Vec<Source> = (0..ports).map(|k| source_of(w, &b, k + 1)).collect();
    let body_nodes: Vec<Node> = w.nodes.iter().filter(|n| body.contains(&n.id)).cloned().collect();
    let body_links: Vec<Link> = w.links.iter().filter(|l| body.contains(&l.to) && (body.contains(&l.from) || l.from == begin)).cloned().collect();
    let end_links: Vec<Link> = w.links.iter().filter(|l| l.to == end).cloned().collect();
    let end_consts: Vec<V> = (0..ports).map(|k| e.values.get(k).copied().unwrap_or(e.ins[k].value)).collect();
    let mut last: HashMap<u32, u32> = HashMap::new();
    for it in 0..count {
        let mut map: HashMap<u32, u32> = HashMap::new();
        for bn in &body_nodes {
            let id = *next;
            *next += 1;
            map.insert(bn.id, id);
            let mut n = bn.clone();
            n.id = id;
            w.nodes.push(n);
            origin.insert(id, origin.get(&bn.id).copied().unwrap_or(bn.id));
        }
        let from_begin = |out: usize, state: &[Source]| -> Source {
            if out == 0 {
                Source::Const([it as f32; 4])
            } else {
                state.get(out - 1).copied().unwrap_or(Source::Nothing)
            }
        };
        for l in &body_links {
            let to = map[&l.to];
            if l.from == begin {
                feed(w, to, l.inp, from_begin(l.out, &state));
            } else {
                w.links.push(Link { from: map[&l.from], out: l.out, to, inp: l.inp });
            }
        }
        // Les liens venus du dehors de la zone servent à chaque copie
        let outside: Vec<Link> = w.links.iter().filter(|l| body.contains(&l.to) && !body.contains(&l.from) && l.from != begin).cloned().collect();
        for l in outside {
            w.links.push(Link { from: l.from, out: l.out, to: map[&l.to], inp: l.inp });
        }
        let new_state: Vec<Source> = (0..ports)
            .map(|k| match end_links.iter().find(|l| l.inp == k) {
                Some(l) if l.from == begin => from_begin(l.out, &state),
                Some(l) if body.contains(&l.from) => Source::Link(map[&l.from], l.out),
                Some(l) => Source::Link(l.from, l.out),
                None => Source::Const(end_consts[k]),
            })
            .collect();
        state = new_state;
        last = map;
    }
    // Ce qui lisait la fin de la zone lit l'état final ; ce qui lisait le contenu depuis le dehors lit la dernière copie
    for (k, src) in state.iter().enumerate() {
        rewire(w, end, k, *src);
    }
    let escaping: Vec<Link> = w.links.iter().filter(|l| (body.contains(&l.from) || l.from == begin) && !body.contains(&l.to) && l.to != end).cloned().collect();
    for l in escaping {
        let src = if l.from == begin {
            if l.out == 0 {
                Source::Const([count.saturating_sub(1) as f32; 4])
            } else {
                source_of(w, &b, l.out)
            }
        } else {
            match last.get(&l.from) {
                Some(&id) => Source::Link(id, l.out),
                None => Source::Nothing,
            }
        };
        feed(w, l.to, l.inp, src);
    }
    for id in body.iter().copied().chain([begin, end]) {
        w.remove(id);
    }
    Ok(())
}

/// Met le graphe à plat. `pixel` : pour un matériau ou une image, où tout se calcule point par point
/// et où une zone de simulation n'a pas de sens. Le moteur de géométrie, lui, garde les zones de
/// simulation et les exécute image après image.
pub fn lower(g: &Graph, pixel: bool) -> Result<Lowered, Fail> {
    let mut origin: HashMap<u32, u32> = HashMap::new();
    let plain = !g.nodes.iter().any(|n| n.muted || is_dynamic(&n.op) || n.op == "reroute");
    if plain {
        return Ok(Lowered { graph: g.clone(), origin });
    }
    let mut w = g.clone();
    let groups = std::mem::take(&mut w.groups);
    w.frames.clear();
    let mut next = w.next_id.max(w.nodes.iter().map(|n| n.id + 1).max().unwrap_or(0));
    // 1. Groupes : dépliés jusqu'à ce qu'il n'en reste plus
    let mut chain: HashMap<u32, Vec<String>> = HashMap::new();
    let mut guard = 0;
    while let Some(id) = w.nodes.iter().find(|n| n.op == "group" && !n.muted).map(|n| n.id) {
        expand_one(&mut w, id, &groups, &mut next, &mut origin, &mut chain, false)?;
        guard += 1;
        if guard > 4000 || w.nodes.len() > 20000 {
            return Err((origin.get(&id).copied().unwrap_or(id), "trop de groupes imbriqués".into()));
        }
    }
    // Entrées et sorties de groupe restées à ce niveau (aperçu de l'intérieur d'un groupe) :
    // les entrées donnent leur valeur par défaut
    for n in w.nodes.clone().iter().filter(|n| matches!(n.op.as_str(), "group_in" | "group_out")) {
        for (k, p) in n.outs.iter().enumerate() {
            rewire(&mut w, n.id, k, Source::Const(p.value));
        }
        w.remove(n.id);
    }
    // 2. Nodes muets et renvois : ils laissent passer leur première entrée
    loop {
        let Some(n) = w.nodes.iter().find(|n| n.op == "reroute" || (n.muted && !is_zone(&n.op) && !matches!(n.op.as_str(), "mat_out" | "comp_out" | "geo_out"))).cloned() else { break };
        let src = match w.links.iter().find(|l| l.to == n.id && l.inp == 0) {
            Some(l) => Source::Link(l.from, l.out),
            None => Source::Nothing,
        };
        let outs = node_def(&n).map(|d| d.outputs.len()).unwrap_or(1).max(1);
        for k in 0..outs {
            let users: Vec<(u32, usize)> = w.links.iter().filter(|l| l.from == n.id && l.out == k).map(|l| (l.to, l.inp)).collect();
            for (to, inp) in users {
                w.links.retain(|l| !(l.to == to && l.inp == inp));
                if let Source::Link(from, out) = src {
                    w.links.push(Link { from, out, to, inp });
                }
            }
        }
        w.remove(n.id);
    }
    // 3. Zones
    if pixel {
        if let Some(n) = w.nodes.iter().find(|n| matches!(n.op.as_str(), "sim_in" | "sim_out")) {
            return Err((origin.get(&n.id).copied().unwrap_or(n.id), "une zone de simulation garde un état d'une image à l'autre : elle va dans un graphe de géométrie".into()));
        }
    }
    {
        loop {
            let ends: Vec<u32> = w.nodes.iter().filter(|n| n.op == "repeat_out").map(|n| n.id).collect();
            if ends.is_empty() {
                // Un début sans fin ne fait rien : ses sorties rendent ses entrées
                for n in w.nodes.clone().iter().filter(|n| n.op == "repeat_in") {
                    for k in 0..n.ins.len() {
                        let src = source_of(&w, n, k + 1);
                        rewire(&mut w, n.id, k + 1, src);
                    }
                    rewire(&mut w, n.id, 0, Source::Const([0.0; 4]));
                    w.remove(n.id);
                }
                break;
            }
            // La zone la plus intérieure d'abord : celle dont le contenu ne renferme aucune autre zone
            let mut done = false;
            for end in &ends {
                let Some(begin) = zone_pair(&w, *end) else {
                    return Err((origin.get(end).copied().unwrap_or(*end), "cette fin de répétition a perdu son début".into()));
                };
                let body = zone_body(&w, begin, *end);
                if w.nodes.iter().any(|n| body.contains(&n.id) && n.op.starts_with("sim_")) {
                    return Err((origin.get(end).copied().unwrap_or(*end), "une zone de simulation ne peut pas se trouver dans une répétition : mets la répétition dans la simulation".into()));
                }
                if w.nodes.iter().any(|n| body.contains(&n.id) && is_zone(&n.op)) {
                    continue;
                }
                unroll(&mut w, begin, *end, &mut next, &mut origin)?;
                done = true;
                break;
            }
            if !done {
                let id = ends[0];
                return Err((origin.get(&id).copied().unwrap_or(id), "des zones de répétition se croisent : chacune doit être entière, ou tout entière dans une autre".into()));
            }
        }
    }
    w.next_id = next;
    Ok(Lowered { graph: w, origin })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::{Ctx, Program};

    fn color_of(g: &Graph) -> V {
        let prog = Program::compile(g, "", 64).map_err(|e| e.1).unwrap();
        prog.material(&Ctx { uv: [0.3, 0.6], pos: [0.1, 0.2, 0.3], nrm: [0.0, 1.0, 0.0], view: [0.0, 0.0, 1.0], vcol: [1.0; 4], res: [64.0, 64.0], ..Default::default() }, &mut vec![]).color
    }

    fn out_of(g: &Graph) -> u32 {
        g.nodes.iter().find(|n| n.op == "mat_out").unwrap().id
    }

    fn set(g: &mut Graph, id: u32, k: usize, v: f32) {
        let n = g.nodes.iter_mut().find(|n| n.id == id).unwrap();
        while n.values.len() <= k {
            n.values.push([0.0; 4]);
        }
        n.values[k] = [v; 4];
    }

    fn choose(g: &mut Graph, id: u32, key: &str, v: &str) {
        g.nodes.iter_mut().find(|n| n.id == id).unwrap().params.insert(key.into(), Param::S(v.into()));
    }

    /// valeur(0.2) → math(+0.3) → math(×2) → couleur de sortie
    fn chain() -> (Graph, u32, u32, u32, u32) {
        let mut g = Graph::new_material();
        let out = out_of(&g);
        let v = g.add("value", [0.0, 0.0]);
        set(&mut g, v, 0, 0.2);
        let a = g.add("math", [200.0, 0.0]);
        set(&mut g, a, 1, 0.3);
        let m = g.add("math", [400.0, 0.0]);
        choose(&mut g, m, "opération", "multiplier");
        set(&mut g, m, 1, 2.0);
        g.connect(v, 0, a, 0);
        g.connect(a, 0, m, 0);
        g.connect(m, 0, out, 0);
        (g, v, a, m, out)
    }

    #[test]
    fn renvoi_et_muet_laissent_passer() {
        let (mut g, _, a, m, _) = chain();
        let base = color_of(&g)[0];
        assert!((base - 1.0).abs() < 1e-5);
        // Un renvoi au milieu d'un lien ne change rien
        let r = g.add("reroute", [300.0, 0.0]);
        g.connect(a, 0, r, 0);
        g.connect(r, 0, m, 0);
        assert!((color_of(&g)[0] - base).abs() < 1e-6);
        // Le node muet laisse passer sa première entrée : (0.2) × 2
        g.nodes.iter_mut().find(|n| n.id == a).unwrap().muted = true;
        assert!((color_of(&g)[0] - 0.4).abs() < 1e-5);
    }

    #[test]
    fn grouper_ne_change_pas_le_resultat_et_se_defait() {
        let (mut g, v, a, m, out) = chain();
        let base = color_of(&g);
        let count = g.nodes.len();
        let id = make_group(&mut g, &[a, m].into_iter().collect(), "Groupe").unwrap();
        assert_eq!(g.groups.len(), 1);
        assert_eq!(g.groups[0].inputs.len(), 1);
        assert_eq!(g.groups[0].outputs.len(), 1);
        assert_eq!(g.nodes.len(), count - 1, "deux nodes remplacés par un node de groupe");
        assert!(g.links.iter().any(|l| l.from == v && l.to == id) && g.links.iter().any(|l| l.from == id && l.to == out));
        assert_eq!(color_of(&g), base);
        // Valeur exposée : sans lien, l'entrée du groupe se règle sur le node de groupe
        g.links.retain(|l| l.to != id);
        set(&mut g, id, 0, 0.7);
        assert!((color_of(&g)[0] - 2.0).abs() < 1e-5);
        // Enregistré puis relu : pareil
        let back: Graph = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert_eq!(color_of(&back), color_of(&g));
        // Deux fois le même groupe à la suite : ((0.7 + 0.3) × 2 + 0.3) × 2
        let second = add_group_node(&mut g, "Groupe", [600.0, 0.0]).unwrap();
        g.connect(id, 0, second, 0);
        g.connect(second, 0, out, 0);
        assert!((color_of(&g)[0] - 4.6).abs() < 1e-4);
        // Remis à plat : même résultat, plus de node de groupe
        let before = color_of(&g);
        ungroup(&mut g, id).unwrap();
        ungroup(&mut g, second).unwrap();
        assert!(!g.nodes.iter().any(|n| n.op == "group"));
        assert_eq!(color_of(&g), before);
    }

    #[test]
    fn groupe_dans_un_groupe_et_boucle_refusee() {
        let (mut g, _, a, m, _) = chain();
        let base = color_of(&g);
        let inner = make_group(&mut g, &[a].into_iter().collect(), "Dedans").unwrap();
        make_group(&mut g, &[inner, m].into_iter().collect(), "Dehors").unwrap();
        assert_eq!(g.groups.len(), 2);
        assert_eq!(color_of(&g), base);
        // Un groupe qui se contient lui-même : erreur claire, pas de boucle sans fin
        let gi = g.groups.iter().position(|x| x.name == "Dedans").unwrap();
        let mut me = g.groups[gi].nodes[0].clone();
        me.id = 900;
        me.op = "group".into();
        me.params.insert("groupe".into(), Param::S("Dedans".into()));
        g.groups[gi].nodes.push(me);
        let gin = g.groups[gi].nodes.iter().find(|n| n.op == "group_in").unwrap().id;
        let gout = g.groups[gi].nodes.iter().find(|n| n.op == "group_out").unwrap().id;
        g.groups[gi].links.push(Link { from: gin, out: 0, to: 900, inp: 0 });
        g.groups[gi].links.retain(|l| l.to != gout);
        g.groups[gi].links.push(Link { from: 900, out: 0, to: gout, inp: 0 });
        sync(&mut g);
        let err = Program::compile(&g, "", 64).err().unwrap();
        assert!(err.1.contains("se contient"), "{}", err.1);
    }

    #[test]
    fn copier_emporte_les_groupes_utiles() {
        let (mut g, _, a, m, _) = chain();
        let inner = make_group(&mut g, &[a].into_iter().collect(), "Dedans").unwrap();
        let outer = make_group(&mut g, &[inner, m].into_iter().collect(), "Dehors").unwrap();
        let other = g.add("math", [0.0, 300.0]);
        make_group(&mut g, &[other].into_iter().collect(), "Ailleurs").unwrap();
        let copied: Vec<Node> = g.nodes.iter().filter(|n| n.id == outer).cloned().collect();
        let names: Vec<String> = groups_used(&g, &copied).into_iter().map(|x| x.name).collect();
        assert_eq!(names, ["Dedans", "Dehors"], "le groupe copié et celui qu'il contient, pas les autres");
    }

    #[test]
    fn interface_de_groupe_ajout_et_retrait() {
        let (mut g, _, a, m, _) = chain();
        let id = make_group(&mut g, &[a, m].into_iter().collect(), "G").unwrap();
        let k = group_add_port(&mut g, "G", true, Port { name: "force".into(), sock: Sock::Float, value: [3.0; 4] }).unwrap();
        assert_eq!(k, 1);
        let n = g.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(n.ins.len(), 2);
        assert_eq!(n.values[1], [3.0; 4]);
        assert_eq!(node_def(n).unwrap().inputs.len(), 2);
        // Retirer la première entrée : le lien qui y arrivait part, la seconde prend le rang 0
        group_remove_port(&mut g, "G", true, 0);
        let n = g.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(n.ins.len(), 1);
        assert_eq!(n.ins[0].name, "force");
        assert!(!g.links.iter().any(|l| l.to == id));
        assert!(group_rename(&mut g, "G", "Usure"));
        assert_eq!(group_uses(&g, "Usure"), 1);
        assert!(Program::compile(&g, "", 64).is_ok());
    }

    #[test]
    fn repetition_deroulee() {
        // état = état × 2, cinq fois, en partant de 0.1 → 3.2
        let mut g = Graph::new_material();
        let out = out_of(&g);
        let (b, e) = add_zone(&mut g, "repeat", [0.0, 0.0]);
        set(&mut g, b, 0, 5.0);
        set(&mut g, b, 1, 0.1);
        let m = g.add("math", [200.0, 0.0]);
        choose(&mut g, m, "opération", "multiplier");
        set(&mut g, m, 1, 2.0);
        g.connect(b, 1, m, 0);
        g.connect(m, 0, e, 0);
        g.connect(e, 0, out, 0);
        assert!((color_of(&g)[0] - 3.2).abs() < 1e-4);
        // Zéro itération : l'état de départ ressort tel quel
        set(&mut g, b, 0, 0.0);
        assert!((color_of(&g)[0] - 0.1).abs() < 1e-6);
        // Le numéro d'itération : somme de 0 + 1 + 2 + 3 = 6
        set(&mut g, b, 0, 4.0);
        set(&mut g, b, 1, 0.0);
        choose(&mut g, m, "opération", "ajouter");
        g.connect(b, 0, m, 1);
        assert!((color_of(&g)[0] - 6.0).abs() < 1e-4);
    }

    #[test]
    fn repetitions_imbriquees() {
        // dehors (3 fois) : état = dedans(état) ; dedans (2 fois) : état + 1 → 0 + 3 × 2 = 6
        let mut g = Graph::new_material();
        let out = out_of(&g);
        let (b1, e1) = add_zone(&mut g, "repeat", [0.0, 0.0]);
        let (b2, e2) = add_zone(&mut g, "repeat", [100.0, 0.0]);
        set(&mut g, b1, 0, 3.0);
        set(&mut g, b1, 1, 0.0);
        set(&mut g, b2, 0, 2.0);
        let m = g.add("math", [200.0, 0.0]);
        set(&mut g, m, 1, 1.0);
        g.connect(b1, 1, b2, 1);
        g.connect(b2, 1, m, 0);
        g.connect(m, 0, e2, 0);
        g.connect(e2, 0, e1, 0);
        g.connect(e1, 0, out, 0);
        assert!((color_of(&g)[0] - 6.0).abs() < 1e-4);
    }

    #[test]
    fn zone_prises_et_erreurs_lisibles() {
        let mut g = Graph::new_material();
        let out = out_of(&g);
        let (b, e) = add_zone(&mut g, "repeat", [0.0, 0.0]);
        assert_eq!(zone_pair(&g, b), Some(e));
        let k = zone_add_port(&mut g, b, Port { name: "masque".into(), sock: Sock::Float, value: [0.5; 4] }).unwrap();
        assert_eq!(k, 1);
        assert_eq!(g.nodes.iter().find(|n| n.id == e).unwrap().ins.len(), 2);
        g.connect(b, 2, e, 1);
        g.connect(e, 1, out, 1);
        zone_remove_port(&mut g, b, 0);
        // La prise restante a pris le rang 0, ses liens ont suivi
        assert!(g.links.iter().any(|l| l.from == b && l.out == 1 && l.to == e && l.inp == 0));
        assert!(g.links.iter().any(|l| l.from == e && l.out == 0 && l.to == out));
        // Itérations branchées sur autre chose qu'une valeur fixe : message clair, sur le bon node
        let n = g.add("noise", [-200.0, 0.0]);
        g.connect(n, 0, b, 0);
        let err = Program::compile(&g, "", 64).err().unwrap();
        assert_eq!(err.0, b);
        assert!(err.1.contains("itérations"));
        // Une zone de simulation n'a pas sa place dans un matériau
        let mut g = Graph::new_material();
        add_zone(&mut g, "sim", [0.0, 0.0]);
        let o = out_of(&g);
        let s = g.nodes.iter().find(|n| n.op == "sim_out").unwrap().id;
        g.connect(s, 0, o, 0);
        assert!(Program::compile(&g, "", 64).err().unwrap().1.contains("simulation"));
    }
}
