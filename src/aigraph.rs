//! L'Assistant et les graphes de nodes : ce qu'il en voit (résumé du graphe, catalogue des nodes,
//! notice d'usage) et ce qu'il peut y changer.
//!
//! Le modèle écrit ses changements dans un bloc ```graphe : du JSON, une liste d'opérations
//! (ajouter, relier, régler, supprimer, grouper, zone…). `blocks` les lit, `apply` les vérifie
//! contre le catalogue et les applique à une copie du graphe : soit tout passe, soit rien n'est
//! touché et la liste des erreurs est rendue. Ce format texte marche avec tous les fournisseurs
//! (Ollama, Claude, les API compatibles OpenAI), y compris ceux qui ne gèrent pas les outils.

use crate::atelier::{self, Layer, Mask};
use crate::nodeflow::{self, Port};
use crate::nodes::{self, Def, Frame, Graph, GraphKind, Node, Param, ParamDef, Sock, V, DEFS};
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashMap};

// ---------- lecture des blocs ----------

/// Un bloc ```graphe écrit par le modèle.
#[derive(Clone, Debug)]
pub struct Edit {
    /// Fichier visé (pour refuser d'appliquer au mauvais document)
    pub file: Option<String>,
    /// Repartir d'un graphe vide (il ne garde que sa sortie)
    pub replace: bool,
    /// Matériau à calques : le calque dont on modifie le graphe, et s'il s'agit de son masque
    pub layer: Option<Value>,
    pub mask: bool,
    pub ops: Vec<Value>,
}

/// Clé ou nom ramené à sa forme simple : sans accents, sans majuscules, « œ » en « oe ».
fn key(s: &str) -> String {
    crate::chat::fold(&s.replace('œ', "oe").replace('Œ', "oe")).trim().replace(['-', ' ', '\''], "_")
}

/// Nom comparé sans accents ni majuscules (les espaces sont gardés).
fn plain(s: &str) -> String {
    crate::chat::fold(&s.replace('œ', "oe").replace('Œ', "oe")).trim().to_string()
}

/// Valeur d'un objet par l'un de ses noms possibles (accents et majuscules indifférents).
fn get<'a>(o: &'a Map<String, Value>, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|n| o.iter().find(|(k, _)| key(k) == *n).map(|(_, v)| v))
}

/// Rend le JSON d'un modèle un peu distrait lisible : sans commentaires « // », sans virgule
/// avant une fermeture. Ce qui est entre guillemets n'est pas touché.
fn tidy_json(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    let (mut in_str, mut esc) = (false, false);
    while let Some(c) = chars.next() {
        if in_str {
            out.push(c);
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for d in chars.by_ref() {
                    if d == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '}' | ']' => {
                let kept = out.trim_end().len();
                if out[..kept].ends_with(',') {
                    out.truncate(kept - 1);
                }
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn parse_edit(body: &str) -> Result<Edit, String> {
    let v: Value = serde_json::from_str(&tidy_json(body)).map_err(|e| format!("{} ({e})", crate::i18n::t("bloc illisible : ce n'est pas du JSON valide")))?;
    let (o, ops) = match v {
        Value::Array(a) => (Map::new(), a),
        Value::Object(o) => {
            let ops = match get(&o, &["ops", "operations", "changements"]) {
                Some(Value::Array(a)) => a.clone(),
                // Une seule opération écrite sans liste
                None if get(&o, &["op"]).is_some() => vec![Value::Object(o.clone())],
                _ => return Err(crate::i18n::t("bloc sans liste « ops »")),
            };
            (o, ops)
        }
        _ => return Err(crate::i18n::t("bloc sans liste « ops »")),
    };
    Ok(Edit {
        file: get(&o, &["fichier", "file", "document"]).and_then(|v| v.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        replace: get(&o, &["remplacer", "replace", "nouveau"]).and_then(|v| v.as_bool()).unwrap_or(false),
        layer: get(&o, &["calque", "layer"]).cloned().filter(|v| !v.is_null()),
        mask: get(&o, &["partie", "part"]).and_then(|v| v.as_str()).is_some_and(|s| plain(s).starts_with("mas")),
        ops,
    })
}

/// Un bloc de code de la réponse : sa langue, son corps, et s'il est refermé.
struct Fence {
    lang: String,
    body: String,
    closed: bool,
    /// Rangs de ses lignes dans le texte (barrières comprises)
    lines: (usize, usize),
}

fn fences(text: &str) -> Vec<Fence> {
    let mut out: Vec<Fence> = vec![];
    let mut open = false;
    for (k, line) in text.lines().enumerate() {
        if let Some(rest) = line.trim_start().strip_prefix("```") {
            if open {
                if let Some(f) = out.last_mut() {
                    f.closed = true;
                    f.lines.1 = k;
                }
                open = false;
            } else {
                out.push(Fence { lang: plain(rest), body: String::new(), closed: false, lines: (k, usize::MAX) });
                open = true;
            }
            continue;
        }
        if open {
            if let Some(f) = out.last_mut() {
                f.body.push_str(line);
                f.body.push('\n');
            }
        }
    }
    out
}

/// Ce bloc est-il un changement de graphe ? Un bloc ```graphe, ou un bloc ```json qui en a la forme
/// (certains modèles reviennent à « json » par habitude).
fn is_edit(f: &Fence) -> bool {
    if matches!(f.lang.as_str(), "graphe" | "graph" | "aryadne-graphe" | "noeuds" | "nodes") {
        return true;
    }
    matches!(f.lang.as_str(), "json" | "") && f.closed && serde_json::from_str::<Value>(&tidy_json(&f.body)).ok().is_some_and(|v| {
        let ops = match &v {
            Value::Object(o) => get(o, &["ops", "operations"]).and_then(|x| x.as_array()).cloned(),
            _ => None,
        };
        ops.is_some_and(|a| !a.is_empty() && a.iter().all(|x| x.get("op").is_some_and(|k| k.is_string())))
    })
}

/// Les blocs ```graphe d'une réponse, dans l'ordre. Un bloc pas encore refermé (réponse en cours)
/// est ignoré.
pub fn blocks(text: &str) -> Vec<Result<Edit, String>> {
    fences(text).iter().filter(|f| f.closed && is_edit(f)).map(|f| parse_edit(&f.body)).collect()
}

/// Le texte d'une réponse sans ses blocs ```graphe (la carte « Appliquer » les résume). Un bloc
/// en cours d'écriture est caché lui aussi.
pub fn strip_blocks(text: &str) -> String {
    let hide: Vec<(usize, usize)> = fences(text).iter().filter(|f| is_edit(f)).map(|f| f.lines).collect();
    let kept: Vec<&str> = text.lines().enumerate().filter(|(k, _)| !hide.iter().any(|(a, b)| k >= a && k <= b)).map(|(_, l)| l).collect();
    kept.join("\n").trim_end().to_string()
}

// ---------- ce que le modèle voit ----------

fn kind_name(k: GraphKind) -> &'static str {
    match k {
        GraphKind::Material => "matériau",
        GraphKind::Compositor => "compositing",
        GraphKind::Geometry => "géométrie et simulation",
    }
}

fn sock_letter(s: Sock) -> &'static str {
    match s {
        Sock::Float => "n",
        Sock::Int => "i",
        Sock::Bool => "b",
        Sock::Vector => "v",
        Sock::Color => "c",
        Sock::Geo => "G",
        Sock::Any => "*",
    }
}

fn sock_name(s: Sock) -> &'static str {
    match s {
        Sock::Float => "nombre",
        Sock::Int => "entier",
        Sock::Bool => "booléen",
        Sock::Vector => "vecteur",
        Sock::Color => "couleur",
        Sock::Geo => "géométrie",
        Sock::Any => "libre",
    }
}

/// Nombre écrit court : « 2 », « 0.5 », « 0.042 ».
fn num(x: f32) -> String {
    if x == x.round() && x.abs() < 1e7 {
        format!("{}", x as i64)
    } else {
        let s = format!("{x:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Valeur d'une entrée, écrite selon son type.
fn show_value(s: Sock, v: V) -> String {
    match s {
        Sock::Vector => format!("[{},{},{}]", num(v[0]), num(v[1]), num(v[2])),
        Sock::Color | Sock::Any => {
            if v[0] == v[1] && v[1] == v[2] {
                num(v[0])
            } else {
                format!("[{},{},{}]", num(v[0]), num(v[1]), num(v[2]))
            }
        }
        Sock::Bool => (if v[0] > 0.5 { "vrai" } else { "faux" }).to_string(),
        _ => num(v[0]),
    }
}

/// Les nodes qu'on peut poser dans ce genre de graphe (sans les nodes internes ni les bouts de zone).
fn usable(d: &Def, kind: GraphKind) -> bool {
    d.fits(kind) && d.cat != "interne" && !nodeflow::is_dynamic(d.op)
}

/// Catalogue compact des nodes d'un genre de graphe, rangé par catégorie : une ligne par node.
pub fn catalog(kind: GraphKind) -> String {
    let mut out = String::from("Types : n nombre, i entier, b booléen, v vecteur [x,y,z], c couleur [r,v,b] de 0 à 1, G géométrie (se relie, ne se règle pas). Après « = » : la valeur par défaut. Ligne : type « libellé » | entrées | -> sorties | réglages{choix possibles}.\n");
    let mut cats: Vec<&str> = vec![];
    for d in DEFS.iter().filter(|d| usable(d, kind)) {
        if !cats.contains(&d.cat) {
            cats.push(d.cat);
        }
    }
    for cat in cats {
        out.push_str(&format!("[{cat}]\n"));
        for d in DEFS.iter().filter(|d| usable(d, kind) && d.cat == cat) {
            let mut line = format!("{} « {} »", d.op, d.label);
            if !d.inputs.is_empty() {
                let ins: Vec<String> = d
                    .inputs
                    .iter()
                    .map(|(n, s, v)| match s {
                        Sock::Geo | Sock::Any | Sock::Color => format!("{n}:{}", sock_letter(*s)),
                        _ => format!("{n}:{}={}", sock_letter(*s), show_value(*s, *v)),
                    })
                    .collect();
                line.push_str(&format!(" | {}", ins.join(", ")));
            }
            if !d.outputs.is_empty() {
                let outs: Vec<String> = d.outputs.iter().map(|(n, s)| format!("{n}:{}", sock_letter(*s))).collect();
                line.push_str(&format!(" | -> {}", outs.join(", ")));
            }
            if !d.params.is_empty() {
                let ps: Vec<String> = d
                    .params
                    .iter()
                    .map(|p| match p {
                        ParamDef::Choice(n, c) => format!("{n}{{{}}}", c.join("|")),
                        ParamDef::Float(n, lo, hi, v) => format!("{n}:n={} ({} à {})", num(*v), num(*lo), num(*hi)),
                        ParamDef::Text(n) => format!("{n}:code"),
                        ParamDef::File(n) => format!("{n}:fichier du coffre"),
                        ParamDef::Ramp(n) => format!("{n}:dégradé \"pos,r,v,b,a;…\""),
                        ParamDef::Curve(n) => format!("{n}:courbes"),
                        ParamDef::Line(n, v) => format!("{n}:texte=\"{v}\""),
                    })
                    .collect();
                line.push_str(&format!(" | réglages : {}", ps.join(", ")));
            }
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

fn pstr<'a>(n: &'a Node, k: &str) -> &'a str {
    match n.params.get(k) {
        Some(Param::S(s)) => s,
        _ => "",
    }
}

/// Nom d'un node pour l'artiste : celui qu'il a donné, sinon celui de son type.
fn node_name(n: &Node) -> String {
    if !n.label.trim().is_empty() {
        return n.label.trim().to_string();
    }
    n.def().map(|d| crate::i18n::t(d.label)).unwrap_or_else(|| n.op.clone())
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

/// Un niveau de graphe (racine, intérieur d'un groupe, graphe d'un calque) en texte : un node par
/// ligne avec ce qui s'écarte des valeurs par défaut, puis les liens.
fn describe_level(g: &Graph, out: &mut String, pad: &str) {
    for n in &g.nodes {
        let Some(d) = n.def() else { continue };
        let mut line = format!("{pad}n{} {}", n.id, n.op);
        if n.op == "group" {
            line.push_str(&format!(" « {} »", pstr(n, "groupe")));
        }
        if !n.label.trim().is_empty() {
            line.push_str(&format!(" nom « {} »", n.label.trim()));
        }
        line.push_str(&format!(" @{},{}", n.pos[0].round(), n.pos[1].round()));
        if n.muted {
            line.push_str(" (muet)");
        }
        let mut sets: Vec<String> = vec![];
        for (k, (name, sock, dv)) in d.inputs.iter().enumerate() {
            if *name == nodeflow::VIRTUAL || *sock == Sock::Geo || g.links.iter().any(|l| l.to == n.id && l.inp == k) {
                continue;
            }
            let v = n.values.get(k).copied().unwrap_or(*dv);
            if v != *dv {
                sets.push(format!("{name}={}", show_value(*sock, v)));
            }
        }
        for (k, p) in &n.params {
            if k == "zone" || k == "groupe" {
                continue;
            }
            let dflt = d.params.iter().find_map(|pd| match pd {
                ParamDef::Choice(n, c) if n == k => Some(c[0].to_string()),
                ParamDef::Float(n, _, _, v) if n == k => Some(num(*v)),
                ParamDef::Line(n, v) if n == k => Some(v.to_string()),
                ParamDef::Text(n) if n == k => Some(nodes::DEFAULT_SHADER.to_string()),
                ParamDef::Ramp(n) if n == k => Some(nodes::DEFAULT_RAMP.to_string()),
                ParamDef::Curve(n) if n == k => Some(nodes::DEFAULT_CURVES.to_string()),
                ParamDef::File(n) if n == k => Some(String::new()),
                _ => None,
            });
            let cur = match p {
                Param::F(v) => num(*v),
                Param::S(s) => s.clone(),
            };
            if dflt.as_deref() != Some(cur.as_str()) {
                sets.push(format!("{k}=\"{}\"", clip(&cur.replace('\n', " ⏎ "), 300)));
            }
        }
        if !sets.is_empty() {
            line.push_str(&format!(" : {}", sets.join(", ")));
        }
        out.push_str(&line);
        out.push('\n');
    }
    if !g.links.is_empty() {
        let port = |id: u32, k: usize, input: bool| -> String {
            let name = g.nodes.iter().find(|n| n.id == id).and_then(|n| n.def()).and_then(|d| if input { d.inputs.get(k).map(|i| i.0) } else { d.outputs.get(k).map(|o| o.0) });
            match name {
                Some(n) if !n.is_empty() => format!("n{id}.{n}"),
                _ => format!("n{id}.{k}"),
            }
        };
        let links: Vec<String> = g.links.iter().map(|l| format!("{} -> {}", port(l.from, l.out, false), port(l.to, l.inp, true))).collect();
        out.push_str(&format!("{pad}Liens : {}\n", links.join(" ; ")));
    }
    for f in &g.frames {
        out.push_str(&format!("{pad}Cadre « {} »{}\n", f.label, if f.note.is_empty() { String::new() } else { format!(" (note : {})", clip(&f.note, 120)) }));
    }
}

fn describe_mask(m: &Mask) -> String {
    let mut s = format!("masque {}", m.kind);
    if m.kind != "plein" {
        s.push_str(&format!(" (quantité {}, contraste {}, échelle {}{})", num(m.amount), num(m.contrast), num(m.scale), if m.invert { ", inversé" } else { "" }));
    }
    if !m.breakup.is_empty() {
        s.push_str(&format!(", cassure {}", m.breakup));
    }
    s
}

/// Résumé lisible du graphe tel que l'artiste l'a sous les yeux. `g` est le niveau en cours
/// d'édition ; `level` nomme le groupe ou le calque où l'on se trouve.
pub fn describe(g: &Graph, rel: &str, level: Option<&str>) -> String {
    let mut out = format!("Document : {rel} (graphe de {})\n", kind_name(g.kind));
    if let Some(l) = level {
        out.push_str(&format!("Niveau en cours d'édition : {l}. Les opérations s'appliquent à ce niveau.\n"));
    }
    if let Some(t) = &g.time {
        out.push_str(&format!("Lecture : {} images par seconde, images {} à {}.\n", num(t.fps), t.start, t.end));
    }
    if let Some(st) = &g.stack {
        out.push_str("Matériau à calques (pile de l'Atelier), du bas vers le haut :\n");
        let root = nodes::vault_root();
        for (k, l) in st.layers.iter().enumerate() {
            let what = match &l.graph {
                Some(_) => "graphe propre".to_string(),
                None => format!("lié au maître {}", l.master),
            };
            out.push_str(&format!("c{} « {} » : {what}, {}, opacité {}{}\n", k + 1, l.name, describe_mask(&l.mask), num(l.opacity), if l.visible { "" } else { ", caché" }));
            let exposed = l.fill(&root).map(|f| atelier::exposed(&f)).unwrap_or_default();
            if !exposed.is_empty() {
                let ps: Vec<String> = exposed
                    .iter()
                    .map(|e| {
                        let v = l.params.get(&e.name).copied().unwrap_or(e.default);
                        format!("{}={}", e.name, show_value(if e.color { Sock::Color } else { Sock::Float }, v))
                    })
                    .collect();
                out.push_str(&format!("   paramètres exposés : {}\n", ps.join(", ")));
            }
            if let Some(inner) = &l.graph {
                if out.len() < 14_000 {
                    describe_level(&inner.0, &mut out, "   ");
                }
            }
            if let Some(mg) = &l.mask.graph {
                if l.mask.kind == "graphe" && out.len() < 14_000 {
                    out.push_str("   graphe du masque :\n");
                    describe_level(&mg.0, &mut out, "      ");
                }
            }
        }
        return out;
    }
    out.push_str(&format!("Nodes ({}) : « n<id> type @x,y : ce qui s'écarte des valeurs par défaut ».\n", g.nodes.len()));
    describe_level(g, &mut out, "");
    for gr in &g.groups {
        // Pendant l'édition d'un groupe, sa place dans la liste porte la racine : on ne la montre pas comme un groupe
        if level.is_some_and(|l| l.contains(&format!("« {} »", gr.name))) {
            continue;
        }
        let ports = |p: &[Port]| p.iter().map(|x| format!("{}:{}", x.name, sock_letter(x.sock))).collect::<Vec<_>>().join(", ");
        out.push_str(&format!("Groupe « {} » ({} nodes) | {} | -> {}\n", gr.name, gr.nodes.len(), ports(&gr.inputs), ports(&gr.outputs)));
    }
    if out.len() > 16_000 {
        out = format!("{}\n[…résumé tronqué : graphe très grand…]\n", out.chars().take(16_000).collect::<String>());
    }
    out
}

/// Comment écrire un bloc ```graphe : le format, puis ce qu'il faut savoir du genre de graphe.
/// `rel` : le document ouvert, écrit tel quel dans l'exemple pour que le modèle le reprenne.
pub fn notice(kind: GraphKind, stack: bool, rel: &str) -> String {
    let mut s = String::from(
        "MODIFIER LE GRAPHE. Tu peux agir toi-même sur le graphe ouvert : n'explique pas une méthode générique, fais le changement. Écris un bloc dont la première ligne est ```graphe, contenant du JSON :\n\
         {\"fichier\": \"<chemin du document>\", \"ops\": [ … ]}\n\
         L'utilisateur l'applique d'un clic (annulable d'un Ctrl+Z). Avant ou après le bloc, dis en une ou deux phrases ce que tu fais. Un seul bloc par réponse. N'utilise que les types, prises et réglages du catalogue ; tout est vérifié et rien n'est appliqué s'il y a une erreur.\n\
         Opérations :\n\
         - {\"op\":\"ajouter\",\"id\":\"grille\",\"type\":\"geo_grid\",\"valeurs\":{\"largeur\":3},\"reglages\":{\"plan\":\"mur (XY)\"},\"nom\":\"Drapeau\",\"pos\":[0,0]} : pose un node. « id » est un nom que tu choisis pour t'y référer ensuite. « valeurs » règle les entrées non reliées, « reglages » les réglages. « pos » et « nom » sont facultatifs (sans « pos », le node est rangé tout seul).\n\
         - {\"op\":\"relier\",\"de\":\"grille.géométrie\",\"vers\":\"n4.géométrie\"} : relie une sortie à une entrée (noeud.prise). Une entrée ne reçoit qu'un lien : le nouveau remplace l'ancien.\n\
         - {\"op\":\"delier\",\"vers\":\"n4.géométrie\"} : retire le lien qui arrive sur cette entrée.\n\
         - {\"op\":\"regler\",\"noeud\":\"n4\",\"valeurs\":{…},\"reglages\":{…},\"nom\":\"…\",\"muet\":false} : change un node existant.\n\
         - {\"op\":\"supprimer\",\"noeuds\":[\"n3\",\"n5\"]} : supprime des nodes (pas la sortie).\n\
         - {\"op\":\"zone\",\"genre\":\"simulation\",\"debut\":\"sim\",\"fin\":\"fin\"} : pose une zone (ses deux nodes, nommés par « debut » et « fin »). Genre « simulation » ou « repetition » (avec \"iterations\": 4).\n\
         - {\"op\":\"grouper\",\"noeuds\":[\"a\",\"b\"],\"nom\":\"Usure\"} : réunit des nodes en un groupe réutilisable. {\"op\":\"degrouper\",\"noeud\":\"n7\"} le défait. Un groupe existant se pose avec {\"op\":\"ajouter\",\"type\":\"groupe\",\"groupe\":\"Usure\"}.\n\
         - {\"op\":\"cadre\",\"noeuds\":[\"a\",\"b\"],\"titre\":\"épinglage\"} : entoure des nodes d'un cadre titré.\n\
         Références : les nodes existants s'appellent n<id> comme dans le résumé (n4) ; les tiens par leur « id » ; « sortie » désigne le node de sortie. Les valeurs suivent le type de la prise : nombre, vecteur [x,y,z], couleur [r,v,b] entre 0 et 1, booléen true/false.\n\
         Pour repartir de zéro, ajoute \"remplacer\": true à côté de \"ops\" : le graphe ne garde que sa sortie (« sortie »).\n",
    );
    match kind {
        GraphKind::Geometry => s.push_str(
            "GÉOMÉTRIE ET SIMULATION. Une géométrie, ce sont des points, leurs attributs (P position, v vitesse, pin épinglé, Cd couleur) et des arêtes. Les nodes sans entrée G (position, index, attribute, math, compare…) sont des champs : calculés pour chaque point quand on les branche sur une entrée « sélection » ou « valeur ».\n\
             - {\"op\":\"lecture\",\"ips\":24,\"debut\":1,\"fin\":120} règle la durée de l'animation.\n\
             - Simulation : une zone de simulation recalcule son contenu à chaque image à partir de l'image d'avant. Début de zone : entrée « géométrie » (état de départ), sorties « pas de temps » et « géométrie » (état de l'image précédente). Fin de zone : entrée « géométrie » (nouvel état), sortie « géométrie » (résultat, à relier vers la sortie). Dans la zone, branche debut.« pas de temps » sur l'entrée « pas de temps » de chaque node de simulation.\n\
             - Tissu : geo_grid avec arêtes « avec diagonales » → épingler → zone { sim_forces (gravité, vent) → sim_solver } → sortie. Épingler = geo_attr avec nom « pin », type « nombre », et sur « sélection » un champ booléen (par exemple position → sepxyz → compare). Les points épinglés ne bougent pas.\n\
             - sim_solver, réglage « solveur » : « tissu (carte graphique) » est le simulateur de tissu sur carte graphique (rapide, pour les maillages fins ; entrées raideur, flexion, sous-pas, cisaillement, amortissement des plis, épaisseur ; réglage « auto-collision ») ; « tissu et fils, XPBD (processeur) » est le solveur de référence (entrée souplesse). Obstacles du solveur : le sol (réglage « sol », entrée « hauteur du sol »), une boule (rayon 0 : aucune), une boîte (taille 0 : aucune).\n\
             - Particules : zone { sim_emit → sim_forces → sim_move → sim_collide }.\n\
             - Une grille « mur (XY) » de largeur L va de x = -L/2 à x = L/2, debout. Une grille « sol (XZ) » est à plat.\n\
             - L'éditeur a des exemples tout faits (menu « Exemples ») : EXEMPLES. Tu peux y renvoyer l'utilisateur, mais fais d'abord ce qu'il demande.\n\
             Exemple, « un drapeau qui flotte au vent » dans un graphe vide :\n\
             ```graphe\n\
             {\"fichier\":\"drapeau.arygeo\",\"remplacer\":true,\"ops\":[\n\
             {\"op\":\"ajouter\",\"id\":\"grille\",\"type\":\"geo_grid\",\"nom\":\"Drapeau\",\"valeurs\":{\"largeur\":3,\"profondeur\":2,\"points en largeur\":48,\"points en profondeur\":32},\"reglages\":{\"plan\":\"mur (XY)\",\"arêtes\":\"avec diagonales\"}},\n\
             {\"op\":\"ajouter\",\"id\":\"pos\",\"type\":\"position\"},\n\
             {\"op\":\"ajouter\",\"id\":\"xyz\",\"type\":\"sepxyz\"},\n\
             {\"op\":\"ajouter\",\"id\":\"bord\",\"type\":\"compare\",\"nom\":\"Bord du mât\",\"valeurs\":{\"B\":-1.49},\"reglages\":{\"comparaison\":\"inférieur\"}},\n\
             {\"op\":\"ajouter\",\"id\":\"epingle\",\"type\":\"geo_attr\",\"nom\":\"Épingler au mât\",\"valeurs\":{\"valeur\":1},\"reglages\":{\"nom\":\"pin\",\"type\":\"nombre\"}},\n\
             {\"op\":\"zone\",\"genre\":\"simulation\",\"debut\":\"sim\",\"fin\":\"fin\"},\n\
             {\"op\":\"ajouter\",\"id\":\"forces\",\"type\":\"sim_forces\",\"valeurs\":{\"vent\":[14,0,3],\"frottement de l'air\":0.3}},\n\
             {\"op\":\"ajouter\",\"id\":\"solveur\",\"type\":\"sim_solver\",\"reglages\":{\"solveur\":\"tissu (carte graphique)\",\"sol\":\"non\"}},\n\
             {\"op\":\"relier\",\"de\":\"pos.position\",\"vers\":\"xyz.vecteur\"},\n\
             {\"op\":\"relier\",\"de\":\"xyz.X\",\"vers\":\"bord.A\"},\n\
             {\"op\":\"relier\",\"de\":\"grille.géométrie\",\"vers\":\"epingle.géométrie\"},\n\
             {\"op\":\"relier\",\"de\":\"bord.résultat\",\"vers\":\"epingle.sélection\"},\n\
             {\"op\":\"relier\",\"de\":\"epingle.géométrie\",\"vers\":\"sim.géométrie\"},\n\
             {\"op\":\"relier\",\"de\":\"sim.géométrie\",\"vers\":\"forces.géométrie\"},\n\
             {\"op\":\"relier\",\"de\":\"sim.pas de temps\",\"vers\":\"forces.pas de temps\"},\n\
             {\"op\":\"relier\",\"de\":\"forces.géométrie\",\"vers\":\"solveur.géométrie\"},\n\
             {\"op\":\"relier\",\"de\":\"sim.pas de temps\",\"vers\":\"solveur.pas de temps\"},\n\
             {\"op\":\"relier\",\"de\":\"solveur.géométrie\",\"vers\":\"fin.géométrie\"},\n\
             {\"op\":\"relier\",\"de\":\"fin.géométrie\",\"vers\":\"sortie.géométrie\"},\n\
             {\"op\":\"lecture\",\"ips\":24,\"debut\":1,\"fin\":240}\n\
             ]}\n\
             ```\n",
        ),
        GraphKind::Material => {
            s.push_str("MATÉRIAU. La sortie mat_out reçoit couleur, rugosité, métal, émission… Les motifs (noise, voronoi, bricks…) donnent des valeurs ou des couleurs ; colorramp et mix les transforment en couleurs ; les nodes d'usure (edgewear, cavdirt, dust…) suivent la forme de l'objet. Un node « param » nommé expose un réglage dans la pile de calques.\n");
            if stack {
                s.push_str(
                    "CALQUES DE L'ATELIER. Ce matériau est une pile de calques (c1 en bas). Opérations sur la pile :\n\
                     - {\"op\":\"ajouter_calque\",\"nom\":\"Rouille\",\"couleur\":[0.45,0.2,0.1],\"rugosite\":0.9,\"metal\":0,\"masque\":{\"genre\":\"creux\",\"quantite\":0.6,\"contraste\":0.6,\"echelle\":8,\"inverser\":false,\"cassure\":\"grunge\"},\"opacite\":1} : calque de remplissage posé au-dessus. À la place de « couleur » : \"prereglage\":\"<nom>\" (matériau tout fait) ou \"maitre\":\"<chemin .arymat du coffre>\".\n\
                     - {\"op\":\"regler_calque\",\"calque\":\"c2\",\"nom\":\"…\",\"opacite\":0.8,\"visible\":true,\"masque\":{…},\"parametres\":{\"couleur\":[1,0,0],\"rugosité\":0.4}} : « parametres » règle les paramètres exposés du calque.\n\
                     - {\"op\":\"supprimer_calque\",\"calque\":\"c2\"} ; {\"op\":\"deplacer_calque\",\"calque\":\"c1\",\"position\":3} (1 = tout en bas).\n\
                     Un calque se désigne par c<rang> ou par son nom. Pour modifier les nodes d'un calque à graphe propre, ajoute \"calque\":\"c1\" à côté de \"ops\" (et \"partie\":\"masque\" pour le graphe de son masque) puis utilise les opérations de nodes.\n",
                );
                s.push_str(&format!("Genres de masque : {}. Cassures : les mêmes générateurs (grunge, bruit, taches…) ou rien.\n", atelier::MASKS.iter().filter(|m| m.0 != "graphe").map(|m| m.0).collect::<Vec<_>>().join(", ")));
                s.push_str(&format!("Préréglages : {}.\n", nodes::MATERIAL_PRESETS.join(", ")));
            }
        }
        GraphKind::Compositor => s.push_str("COMPOSITING. L'image part d'un node image, exr ou video (réglage « fichier » : chemin dans le coffre), traverse les corrections et les filtres, et arrive sur comp_out. Insérer un effet dans une chaîne = relier la sortie d'avant vers son entrée « image » ou « couleur », puis sa sortie vers l'entrée d'après.\n"),
    }
    s.replace("drapeau.arygeo", rel).replace("<chemin du document>", rel).replace("EXEMPLES", &crate::geo::EXAMPLES.join(", "))
}

/// Tout ce que le modèle reçoit quand un graphe est ouvert : la notice, le résumé, le catalogue.
pub fn context(g: &Graph, rel: &str, level: Option<&str>) -> String {
    // Dans la pile d'un matériau, les nodes vivent dans les calques
    let stack = g.stack.is_some();
    format!("{}\nGRAPHE OUVERT\n{}\nCATALOGUE DES NODES ({})\n{}", notice(g.kind, stack, rel), describe(g, rel, level), kind_name(g.kind), catalog(g.kind))
}

// ---------- application ----------

/// Hauteur et largeur d'un node à l'écran (pour ranger sans chevauchement).
fn node_rect(n: &Node) -> [f32; 4] {
    let h = n.def().map(|d| nodes::Editor::node_size(n, d)).unwrap_or(120.0);
    [n.pos[0], n.pos[1], nodes::node_w(n), h]
}

fn rects_touch(a: [f32; 4], b: [f32; 4], gap: f32) -> bool {
    a[0] < b[0] + b[2] + gap && b[0] < a[0] + a[2] + gap && a[1] < b[1] + b[3] + gap && b[1] < a[1] + a[3] + gap
}

struct Work<'a> {
    g: &'a mut Graph,
    alias: HashMap<String, u32>,
    /// Nodes posés par ce bloc, et ceux d'entre eux qui n'ont pas reçu de position
    new: Vec<u32>,
    unplaced: Vec<u32>,
    /// Cadres à tracer une fois les nodes rangés : (nodes, titre, note)
    frames: Vec<(Vec<u32>, String, String)>,
    done: Vec<String>,
}

fn as_f32(v: &Value) -> Option<f32> {
    match v {
        Value::Number(n) => n.as_f64().map(|x| x as f32).filter(|x| x.is_finite()),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => match plain(s).as_str() {
            "vrai" | "true" | "oui" => Some(1.0),
            "faux" | "false" | "non" => Some(0.0),
            t => t.replace(',', ".").parse::<f32>().ok().filter(|x| x.is_finite()),
        },
        _ => None,
    }
}

fn hex_color(s: &str) -> Option<V> {
    let h = s.trim().strip_prefix('#')?;
    if h.len() != 6 && h.len() != 8 {
        return None;
    }
    let b = |k: usize| u8::from_str_radix(h.get(k..k + 2)?, 16).ok().map(|x| x as f32 / 255.0);
    // Les mêmes nombres que ceux du sélecteur de couleur des nodes
    Some([b(0)?, b(2)?, b(4)?, if h.len() == 8 { b(6)? } else { 1.0 }])
}

/// Valeur JSON → valeur d'une prise de ce type.
fn parse_value(sock: Sock, v: &Value) -> Result<V, String> {
    let list = |a: &Vec<Value>| -> Option<Vec<f32>> { a.iter().map(as_f32).collect() };
    match sock {
        Sock::Geo => Err("une géométrie se relie, elle ne se règle pas".into()),
        Sock::Float | Sock::Int | Sock::Bool => match v {
            Value::Array(a) if a.len() == 1 => as_f32(&a[0]).map(|x| [x; 4]).ok_or_else(|| "nombre attendu".to_string()),
            Value::Array(_) | Value::Object(_) | Value::Null => Err(format!("{} attendu", sock_name(sock))),
            _ => as_f32(v).map(|x| [if sock == Sock::Int { x.round() } else { x }; 4]).ok_or_else(|| format!("{} attendu", sock_name(sock))),
        },
        Sock::Vector => match v {
            Value::Array(a) if a.len() == 3 || a.len() == 4 => list(a).map(|l| [l[0], l[1], l[2], 0.0]).ok_or_else(|| "vecteur [x,y,z] attendu".to_string()),
            Value::Number(_) => as_f32(v).map(|x| [x, x, x, 0.0]).ok_or_else(|| "vecteur [x,y,z] attendu".to_string()),
            _ => Err("vecteur [x,y,z] attendu".into()),
        },
        Sock::Color | Sock::Any => match v {
            Value::Array(a) if a.len() == 3 => list(a).map(|l| [l[0], l[1], l[2], 1.0]).ok_or_else(|| "couleur [r,v,b] attendue".to_string()),
            Value::Array(a) if a.len() == 4 => list(a).map(|l| [l[0], l[1], l[2], l[3]]).ok_or_else(|| "couleur [r,v,b] attendue".to_string()),
            Value::String(s) if s.trim().starts_with('#') => hex_color(s).ok_or_else(|| format!("couleur « {s} » illisible")),
            Value::Number(_) | Value::Bool(_) => as_f32(v).map(|x| [x; 4]).ok_or_else(|| "couleur [r,v,b] attendue".to_string()),
            _ => Err("couleur [r,v,b] ou nombre attendu".into()),
        },
    }
}

fn param_name(p: &ParamDef) -> &'static str {
    match p {
        ParamDef::Choice(n, _) | ParamDef::Float(n, ..) | ParamDef::Text(n) | ParamDef::File(n) | ParamDef::Ramp(n) | ParamDef::Curve(n) | ParamDef::Line(n, _) => n,
    }
}

/// Cherche un nom dans une liste, sans accents ni majuscules : exact, puis début unique, puis morceau unique.
fn find_name(names: &[&str], want: &str) -> Option<usize> {
    let w = plain(want);
    if w.is_empty() {
        return None;
    }
    if let Some(k) = names.iter().position(|n| plain(n) == w) {
        return Some(k);
    }
    let tests: [fn(&str, &str) -> bool; 2] = [|n, w| n.starts_with(w), |n, w| n.contains(w)];
    for test in tests {
        let hits: Vec<usize> = names.iter().enumerate().filter(|(_, n)| test(&plain(n), &w)).map(|(k, _)| k).collect();
        if hits.len() == 1 {
            return Some(hits[0]);
        }
    }
    None
}

impl Work<'_> {
    fn node(&self, id: u32) -> Option<&Node> {
        self.g.nodes.iter().find(|n| n.id == id)
    }

    fn name(&self, id: u32) -> String {
        self.node(id).map(node_name).unwrap_or_else(|| format!("n{id}"))
    }

    /// Référence d'un node : un « id » donné par ce bloc, n<id>, « sortie », ou un type présent une seule fois.
    fn node_ref(&self, v: &Value) -> Result<u32, String> {
        let s = match v {
            Value::String(s) => s.trim().to_string(),
            Value::Number(n) => format!("n{n}"),
            _ => return Err("référence de node attendue (par exemple \"n4\")".into()),
        };
        if let Some(id) = self.alias.get(&key(&s)) {
            return Ok(*id);
        }
        let digits = s.strip_prefix('n').or_else(|| s.strip_prefix('N')).unwrap_or(&s);
        if let Ok(id) = digits.parse::<u32>() {
            return if self.node(id).is_some() { Ok(id) } else { Err(format!("le node n{id} n'existe pas")) };
        }
        if matches!(key(&s).as_str(), "sortie" | "out" | "output") {
            if let Some(n) = self.g.nodes.iter().find(|n| nodes::is_out_op(&n.op)) {
                return Ok(n.id);
            }
        }
        // Un type ou un nom qui ne désigne qu'un node
        let hits: Vec<u32> = self.g.nodes.iter().filter(|n| n.op == s || plain(&n.label) == plain(&s)).map(|n| n.id).collect();
        match hits.as_slice() {
            [id] => Ok(*id),
            [] => Err(format!("node « {s} » inconnu (ni un « id » de ce bloc, ni un n<id> du graphe)")),
            _ => Err(format!("« {s} » désigne plusieurs nodes : précise avec n<id>")),
        }
    }

    /// « noeud.prise », ["noeud","prise"] ou {"noeud":…,"prise":…} → (node, prise demandée).
    fn end_ref(&self, v: &Value) -> Result<(u32, Option<String>), String> {
        match v {
            Value::String(s) => {
                // Le node d'abord en entier (un « id » peut contenir un point), sinon coupé au premier point
                if let Ok(id) = self.node_ref(v) {
                    return Ok((id, None));
                }
                match s.split_once('.') {
                    Some((a, b)) => Ok((self.node_ref(&Value::String(a.to_string()))?, Some(b.trim().to_string()).filter(|b| !b.is_empty()))),
                    None => Err(self.node_ref(v).unwrap_err()),
                }
            }
            Value::Array(a) if !a.is_empty() => Ok((self.node_ref(&a[0])?, a.get(1).map(|p| p.as_str().map(str::to_string).unwrap_or_else(|| p.to_string())))),
            Value::Object(o) => {
                let n = get(o, &["noeud", "node", "id"]).ok_or("il manque « noeud »")?;
                Ok((self.node_ref(n)?, get(o, &["prise", "port", "entree", "sortie"]).map(|p| p.as_str().map(str::to_string).unwrap_or_else(|| p.to_string()))))
            }
            _ => Err("prise attendue, sous la forme \"noeud.prise\"".into()),
        }
    }

    /// Rang d'une prise d'un node, par son nom ou son numéro.
    fn port(&self, id: u32, want: Option<&str>, input: bool) -> Result<usize, String> {
        let n = self.node(id).ok_or("node disparu")?;
        let d = n.def().ok_or("node sans définition")?;
        let all: Vec<&str> = if input { d.inputs.iter().map(|i| i.0).collect() } else { d.outputs.iter().map(|o| o.0).collect() };
        let names: Vec<&str> = all.iter().copied().filter(|n| *n != nodeflow::VIRTUAL).collect();
        let (what, of) = if input { ("entrée", "d'entrée") } else { ("sortie", "de sortie") };
        let listed = if names.is_empty() { "aucune".to_string() } else { names.join(", ") };
        match want {
            None => match names.len() {
                1 => Ok(all.iter().position(|x| *x == names[0]).unwrap_or(0)),
                0 => Err(format!("« {} » n'a pas {of}", self.name(id))),
                _ => Err(format!("précise la {what} de « {} » ({listed})", self.name(id))),
            },
            Some(w) => {
                if let Ok(k) = w.trim().parse::<usize>() {
                    if k < all.len() && all[k] != nodeflow::VIRTUAL {
                        return Ok(k);
                    }
                }
                find_name(&names, w).and_then(|k| all.iter().position(|x| *x == names[k])).ok_or_else(|| format!("« {} » n'a pas {of} « {w} » ({what}s : {listed})", self.name(id)))
            }
        }
    }

    fn has_param(&self, id: u32, pname: &str) -> bool {
        let names: Vec<&str> = self.node(id).and_then(|n| nodes::def(&n.op)).map(|d| d.params.iter().map(param_name).collect()).unwrap_or_default();
        find_name(&names, pname).is_some()
    }

    fn set_param(&mut self, id: u32, pname: &str, v: &Value) -> Result<String, String> {
        let n = self.node(id).ok_or("node disparu")?;
        let d = nodes::def(&n.op).ok_or("node sans définition")?;
        let names: Vec<&str> = d.params.iter().map(param_name).collect();
        let Some(k) = find_name(&names, pname) else {
            return Err(format!("« {} » n'a pas de réglage « {pname} » (réglages : {})", self.name(id), if names.is_empty() { "aucun".to_string() } else { names.join(", ") }));
        };
        let text = || match v {
            Value::String(s) => Ok(s.clone()),
            Value::Number(n) => Ok(n.to_string()),
            Value::Bool(b) => Ok((if *b { "oui" } else { "non" }).to_string()),
            _ => Err(format!("texte attendu pour « {} »", names[k])),
        };
        let (val, shown) = match d.params[k] {
            ParamDef::Choice(_, choices) => {
                let t = text()?;
                let c = choices.iter().find(|c| plain(c) == plain(&t)).or_else(|| find_name(choices, &t).map(|k| &choices[k]));
                match c {
                    Some(c) => (Param::S(c.to_string()), c.to_string()),
                    None => return Err(format!("« {t} » n'est pas un choix de « {} » (choix : {})", names[k], choices.join(" | "))),
                }
            }
            ParamDef::Float(_, lo, hi, _) => {
                let x = as_f32(v).ok_or_else(|| format!("nombre attendu pour « {} »", names[k]))?.clamp(lo, hi);
                (Param::F(x), num(x))
            }
            ParamDef::Text(_) => {
                let t = text()?;
                if n.op == "shader" {
                    nodes::Shader::compile(&t).map_err(|e| format!("code du shader refusé : {e}"))?;
                }
                (Param::S(t.clone()), clip(&t.replace('\n', " ⏎ "), 60))
            }
            ParamDef::Ramp(_) => {
                let t = text()?;
                if nodes::parse_ramp(&t).len() < 2 {
                    return Err(format!("dégradé illisible pour « {} » : écris \"pos,r,v,b,a;pos,r,v,b,a\"", names[k]));
                }
                (Param::S(t.clone()), clip(&t, 60))
            }
            _ => {
                let t = text()?;
                (Param::S(t.clone()), clip(&t, 60))
            }
        };
        let pk = names[k].to_string();
        if let Some(n) = self.g.nodes.iter_mut().find(|n| n.id == id) {
            n.params.insert(pk.clone(), val);
        }
        Ok(format!("{pk} : {shown}"))
    }

    fn set_value(&mut self, id: u32, pname: &str, v: &Value) -> Result<String, String> {
        let k = self.port(id, Some(pname), true)?;
        let d = self.node(id).and_then(|n| n.def()).ok_or("node sans définition")?;
        let (name, sock, _) = d.inputs[k];
        let val = parse_value(sock, v).map_err(|e| format!("{e} pour « {name} »"))?;
        if let Some(n) = self.g.nodes.iter_mut().find(|n| n.id == id) {
            while n.values.len() < d.inputs.len() {
                let dv = d.inputs[n.values.len()].2;
                n.values.push(dv);
            }
            n.values[k] = val;
        }
        Ok(format!("{name} : {}", show_value(sock, val)))
    }

    /// Valeurs, réglages, nom, position, muet : ce qu'« ajouter » et « regler » ont en commun.
    /// Un réglage rangé par erreur dans « valeurs » (ou l'inverse) est accepté.
    fn tune(&mut self, id: u32, o: &Map<String, Value>) -> Result<Vec<String>, String> {
        let mut said = vec![];
        for (group, first_value) in [(&["valeurs", "values", "entrees", "inputs"][..], true), (&["reglages", "parametres", "params", "settings"][..], false)] {
            let Some(v) = get(o, group) else { continue };
            let Value::Object(map) = v else { return Err("« valeurs » et « reglages » sont des objets {nom: valeur}".into()) };
            for (k, v) in map {
                let has_value = self.port(id, Some(k), true).is_ok();
                let has_param = self.has_param(id, k);
                // Du bon côté d'abord ; sinon de l'autre s'il porte ce nom
                let as_value = match (first_value, has_value, has_param) {
                    (true, true, _) | (true, false, false) | (false, true, false) => true,
                    _ => false,
                };
                said.push(if as_value { self.set_value(id, k, v)? } else { self.set_param(id, k, v)? });
            }
        }
        if let Some(v) = get(o, &["nom", "name", "label", "libelle"]) {
            let t = v.as_str().ok_or("« nom » est un texte")?.trim().to_string();
            if let Some(n) = self.g.nodes.iter_mut().find(|n| n.id == id) {
                n.label = t;
            }
        }
        if let Some(v) = get(o, &["pos", "position"]) {
            let p = v.as_array().filter(|a| a.len() == 2).and_then(|a| Some([as_f32(&a[0])?, as_f32(&a[1])?])).ok_or("« pos » s'écrit [x, y]")?;
            if let Some(n) = self.g.nodes.iter_mut().find(|n| n.id == id) {
                n.pos = p;
            }
            self.unplaced.retain(|x| *x != id);
        }
        for (names, what) in [(&["muet", "muted", "mute"][..], 0), (&["reduit", "collapsed"][..], 1), (&["apercu", "preview"][..], 2)] {
            if let Some(b) = get(o, names).and_then(|v| v.as_bool()) {
                if let Some(n) = self.g.nodes.iter_mut().find(|n| n.id == id) {
                    match what {
                        0 => n.muted = b,
                        1 => n.collapsed = b,
                        _ => n.preview = b,
                    }
                }
                if what == 0 {
                    said.push((if b { "muet" } else { "actif" }).to_string());
                }
            }
        }
        Ok(said)
    }

    fn bind(&mut self, o: &Map<String, Value>, names: &[&str], id: u32) -> Result<(), String> {
        if let Some(v) = get(o, names) {
            let a = match v {
                Value::String(s) => key(s),
                Value::Number(n) => n.to_string(),
                _ => return Err("« id » est un nom court".into()),
            };
            if a.is_empty() {
                return Ok(());
            }
            if self.alias.contains_key(&a) {
                return Err(format!("l'id « {a} » est déjà pris dans ce bloc"));
            }
            self.alias.insert(a, id);
        }
        Ok(())
    }

    fn ids(&self, o: &Map<String, Value>) -> Result<Vec<u32>, String> {
        match get(o, &["noeuds", "nodes", "noeud", "node", "id"]) {
            Some(Value::Array(a)) => a.iter().map(|v| self.node_ref(v)).collect(),
            Some(v) => Ok(vec![self.node_ref(v)?]),
            None => Err("il manque « noeuds »".into()),
        }
    }

    fn op(&mut self, o: &Map<String, Value>) -> Result<(), String> {
        let kind = get(o, &["op", "operation", "action"]).and_then(|v| v.as_str()).map(key).ok_or("il manque « op »")?;
        match kind.as_str() {
            "ajouter" | "add" | "ajoute" | "creer" => {
                let ty = get(o, &["type", "node", "noeud"]).and_then(|v| v.as_str()).ok_or("il manque « type »")?.trim().to_string();
                let id = if matches!(key(&ty).as_str(), "groupe" | "group") {
                    let name = get(o, &["groupe", "group"]).and_then(|v| v.as_str()).ok_or("il manque « groupe » : le nom du groupe à poser")?;
                    let known: Vec<&str> = self.g.groups.iter().map(|g| g.name.as_str()).collect();
                    let k = find_name(&known, name).ok_or_else(|| format!("le groupe « {name} » n'existe pas (groupes : {})", if known.is_empty() { "aucun".to_string() } else { known.join(", ") }))?;
                    let gname = known[k].to_string();
                    nodeflow::add_group_node(self.g, &gname, [0.0, 0.0]).ok_or("groupe introuvable")?
                } else {
                    let d = find_def(&ty, self.g.kind)?;
                    if matches!(d.op, "sim_in" | "sim_out" | "repeat_in" | "repeat_out") {
                        return Err("une zone se pose avec l'opération « zone »".into());
                    }
                    if nodes::is_out_op(d.op) && self.g.nodes.iter().any(|n| nodes::is_out_op(&n.op)) {
                        return Err("le graphe a déjà son node de sortie : relie-le sous le nom « sortie »".into());
                    }
                    self.g.add(d.op, [0.0, 0.0])
                };
                self.new.push(id);
                self.unplaced.push(id);
                self.bind(o, &["id", "alias", "ref"], id)?;
                let said = self.tune(id, o)?;
                self.done.push(format!("{} « {} »{}", crate::i18n::t("ajoute"), self.name(id), if said.is_empty() { String::new() } else { format!(" ({})", said.join(", ")) }));
            }
            "regler" | "set" | "modifier" | "regle" => {
                let id = self.node_ref(get(o, &["noeud", "node", "id"]).ok_or("il manque « noeud »")?)?;
                let before = self.name(id);
                let said = self.tune(id, o)?;
                let after = self.name(id);
                let mut line = format!("{} « {before} »", crate::i18n::t("règle"));
                if after != before {
                    line = format!("{} « {before} » → « {after} »", crate::i18n::t("renomme"));
                }
                if !said.is_empty() {
                    line.push_str(&format!(" ({})", said.join(", ")));
                }
                self.done.push(line);
            }
            "relier" | "connect" | "lier" | "relie" | "link" => {
                let (from, fp) = self.end_ref(get(o, &["de", "from", "source"]).ok_or("il manque « de »")?)?;
                let (to, tp) = self.end_ref(get(o, &["vers", "to", "a", "cible"]).ok_or("il manque « vers »")?)?;
                let out = self.port(from, fp.as_deref(), false)?;
                let inp = self.port(to, tp.as_deref(), true)?;
                let so = self.node(from).and_then(|n| n.def()).and_then(|d| d.outputs.get(out)).map(|x| x.1).unwrap_or(Sock::Any);
                let (iname, si) = self.node(to).and_then(|n| n.def()).and_then(|d| d.inputs.get(inp)).map(|x| (x.0, x.1)).unwrap_or(("", Sock::Any));
                let oname = self.node(from).and_then(|n| n.def()).and_then(|d| d.outputs.get(out)).map(|x| x.0).unwrap_or("");
                if !nodes::socks_fit(so, si) {
                    return Err(format!("types incompatibles : « {} › {oname} » ({}) ne va pas sur « {} › {iname} » ({})", self.name(from), sock_name(so), self.name(to), sock_name(si)));
                }
                if from == to {
                    return Err("un node ne se relie pas à lui-même".into());
                }
                self.g.connect(from, out, to, inp);
                if !self.g.links.iter().any(|l| l.from == from && l.out == out && l.to == to && l.inp == inp) {
                    return Err(format!("relier « {} » à « {} » ferait une boucle", self.name(from), self.name(to)));
                }
                self.done.push(format!("{} {} › {oname} → {} › {iname}", crate::i18n::t("relie"), self.name(from), self.name(to)));
            }
            "delier" | "disconnect" | "unlink" | "debrancher" => {
                let before = self.g.links.len();
                let what = if let Some(v) = get(o, &["vers", "to", "entree", "cible"]) {
                    let (to, tp) = self.end_ref(v)?;
                    let inp = self.port(to, tp.as_deref(), true)?;
                    self.g.links.retain(|l| !(l.to == to && l.inp == inp));
                    self.name(to)
                } else if let Some(v) = get(o, &["de", "from", "sortie"]) {
                    let (from, fp) = self.end_ref(v)?;
                    let out = self.port(from, fp.as_deref(), false)?;
                    self.g.links.retain(|l| !(l.from == from && l.out == out));
                    self.name(from)
                } else {
                    return Err("il manque « vers » (l'entrée à libérer)".into());
                };
                if self.g.links.len() == before {
                    return Err(format!("aucun lien à retirer sur « {what} »"));
                }
                self.done.push(format!("{} « {what} »", crate::i18n::t("retire un lien de")));
            }
            "supprimer" | "delete" | "remove" | "retirer" => {
                for id in self.ids(o)? {
                    let Some(n) = self.node(id) else { continue };
                    if nodes::is_out_op(&n.op) {
                        return Err("le node de sortie ne se supprime pas".into());
                    }
                    let name = self.name(id);
                    let pair = nodeflow::zone_pair(self.g, id);
                    self.g.remove(id);
                    if let Some(p) = pair {
                        self.g.remove(p);
                    }
                    self.alias.retain(|_, v| *v != id && Some(*v) != pair);
                    self.new.retain(|x| *x != id && Some(*x) != pair);
                    self.unplaced.retain(|x| *x != id && Some(*x) != pair);
                    self.done.push(format!("{} « {name} »", crate::i18n::t("supprime")));
                }
            }
            "zone" => {
                let genre = get(o, &["genre", "kind", "type"]).and_then(|v| v.as_str()).map(plain).unwrap_or_else(|| "simulation".into());
                let sim = genre.starts_with("sim");
                if !sim && !genre.starts_with("rep") && !genre.starts_with("bou") {
                    return Err(format!("genre de zone « {genre} » inconnu (simulation ou repetition)"));
                }
                if sim && self.g.kind != GraphKind::Geometry {
                    return Err("une zone de simulation ne se pose que dans un graphe de géométrie".into());
                }
                let (a, b) = nodeflow::add_zone(self.g, if sim { "sim" } else { "repeat" }, [0.0, 0.0]);
                self.new.extend([a, b]);
                self.unplaced.extend([a, b]);
                self.bind(o, &["debut", "begin", "start", "id"], a)?;
                self.bind(o, &["fin", "end"], b)?;
                if let Some(Value::Array(ports)) = get(o, &["prises", "etats", "ports"]) {
                    for p in ports {
                        let (name, ty) = match p {
                            Value::String(s) => (s.clone(), String::new()),
                            Value::Object(po) => (get(po, &["nom", "name"]).and_then(|v| v.as_str()).unwrap_or("valeur").to_string(), get(po, &["type"]).and_then(|v| v.as_str()).map(plain).unwrap_or_default()),
                            _ => return Err("« prises » est une liste de {nom, type}".into()),
                        };
                        let sock = match ty.chars().next() {
                            Some('g') => Sock::Geo,
                            Some('v') => Sock::Vector,
                            Some('c') => Sock::Color,
                            Some('b') => Sock::Bool,
                            Some('e') | Some('i') => Sock::Int,
                            _ => Sock::Float,
                        };
                        nodeflow::zone_add_port(self.g, a, Port { name, sock, value: [0.0; 4] });
                    }
                    nodeflow::sync(self.g);
                }
                if let Some(v) = get(o, &["iterations"]) {
                    if !sim {
                        let x = as_f32(v).ok_or("« iterations » est un nombre")?.round().clamp(1.0, 4096.0);
                        if let Some(n) = self.g.nodes.iter_mut().find(|n| n.id == a) {
                            if let Some(slot) = n.values.first_mut() {
                                *slot = [x; 4];
                            }
                        }
                    }
                }
                if let Some(v) = get(o, &["pos", "position"]) {
                    let p = v.as_array().filter(|a| a.len() == 2).and_then(|a| Some([as_f32(&a[0])?, as_f32(&a[1])?])).ok_or("« pos » s'écrit [x, y]")?;
                    for (id, dx) in [(a, 0.0), (b, 700.0)] {
                        if let Some(n) = self.g.nodes.iter_mut().find(|n| n.id == id) {
                            n.pos = [p[0] + dx, p[1]];
                        }
                    }
                    self.unplaced.retain(|x| *x != a && *x != b);
                }
                self.done.push(crate::i18n::t(if sim { "pose une zone de simulation" } else { "pose une zone de répétition" }));
            }
            "grouper" | "group" => {
                let ids: BTreeSet<u32> = self.ids(o)?.into_iter().collect();
                let name = get(o, &["nom", "name"]).and_then(|v| v.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| crate::i18n::t("groupe"));
                let n = ids.len();
                let floating = ids.iter().any(|i| self.unplaced.contains(i));
                let id = nodeflow::make_group(self.g, &ids, &name).map_err(|e| crate::i18n::t(&e))?;
                self.alias.retain(|_, v| !ids.contains(v));
                self.new.retain(|x| !ids.contains(x));
                self.unplaced.retain(|x| !ids.contains(x));
                self.new.push(id);
                // Le node de groupe prend la place de ses nodes : s'ils n'en avaient pas, lui non plus
                if floating {
                    self.unplaced.push(id);
                }
                self.bind(o, &["id", "alias"], id)?;
                self.done.push(format!("{} {n} nodes : « {} »", crate::i18n::t("groupe"), pstr(self.node(id).ok_or("groupe introuvable")?, "groupe")));
            }
            "degrouper" | "ungroup" => {
                let id = self.node_ref(get(o, &["noeud", "node", "id"]).ok_or("il manque « noeud »")?)?;
                if self.node(id).is_none_or(|n| n.op != "group") {
                    return Err(format!("« {} » n'est pas un node de groupe", self.name(id)));
                }
                let name = self.name(id);
                nodeflow::ungroup(self.g, id)?;
                self.done.push(format!("{} « {name} »", crate::i18n::t("défait le groupe")));
            }
            "cadre" | "frame" => {
                let ids = self.ids(o)?;
                let title = get(o, &["titre", "title", "nom", "label"]).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
                let note = get(o, &["note", "texte"]).and_then(|v| v.as_str()).unwrap_or("").to_string();
                self.done.push(format!("{} « {title} »", crate::i18n::t("trace le cadre")));
                self.frames.push((ids, title, note));
            }
            "lecture" | "temps" | "time" | "duree" => {
                if self.g.kind != GraphKind::Geometry {
                    return Err("« lecture » ne vaut que pour un graphe de géométrie".into());
                }
                let mut t = self.g.time.unwrap_or_default();
                if let Some(x) = get(o, &["ips", "fps"]).and_then(as_f32) {
                    t.fps = x.clamp(1.0, 240.0);
                }
                if let Some(x) = get(o, &["debut", "start"]).and_then(as_f32) {
                    t.start = x.round() as i32;
                }
                if let Some(x) = get(o, &["fin", "end"]).and_then(as_f32) {
                    t.end = x.round() as i32;
                }
                if t.end <= t.start {
                    return Err("la dernière image doit venir après la première".into());
                }
                self.g.time = Some(t);
                self.done.push(format!("{} : {} i/s, {} → {}", crate::i18n::t("lecture de l'animation"), num(t.fps), t.start, t.end));
            }
            other => return Err(format!("opération « {other} » inconnue (ajouter, relier, delier, regler, supprimer, zone, grouper, degrouper, cadre, lecture)")),
        }
        Ok(())
    }

    /// Range les nodes posés sans position : chacun à droite de ce qui l'alimente, sans chevaucher
    /// personne. Les nodes placés par le modèle qui en recouvrent un autre sont glissés vers le bas.
    fn layout(&mut self) {
        let new: BTreeSet<u32> = self.new.iter().copied().filter(|id| self.g.nodes.iter().any(|n| n.id == *id)).collect();
        let unplaced: BTreeSet<u32> = self.unplaced.iter().copied().filter(|id| new.contains(id)).collect();
        let mut fixed: Vec<[f32; 4]> = self.g.nodes.iter().filter(|n| !new.contains(&n.id)).map(node_rect).collect();
        // Ordre : d'amont en aval (colonne = plus long chemin parmi les nouveaux)
        let mut col: HashMap<u32, usize> = new.iter().map(|i| (*i, 0)).collect();
        for _ in 0..new.len() {
            let mut moved = false;
            for l in &self.g.links {
                if new.contains(&l.from) && new.contains(&l.to) && col[&l.to] < col[&l.from] + 1 {
                    col.insert(l.to, col[&l.from] + 1);
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        let mut order: Vec<u32> = new.iter().copied().collect();
        order.sort_by_key(|id| (col[id], self.new.iter().position(|x| x == id)));
        let base_x = fixed.iter().map(|r| r[0]).fold(f32::MAX, f32::min);
        let below = fixed.iter().map(|r| r[1] + r[3]).fold(f32::MIN, f32::max);
        let (x0, y0) = if fixed.is_empty() || unplaced.len() == self.g.nodes.len() { (0.0, 0.0) } else { (base_x, below + 60.0) };
        for id in order {
            let mut pos = self.node(id).map(|n| n.pos).unwrap_or([0.0, 0.0]);
            if unplaced.contains(&id) {
                // À droite de ses sources déjà rangées ; sinon à gauche de ce qu'il alimente ; sinon en début de rangée
                let preds: Vec<[f32; 2]> = self.g.links.iter().filter(|l| l.to == id).filter_map(|l| self.node(l.from)).filter(|n| !unplaced.contains(&n.id) || col[&n.id] < col[&id]).map(|n| n.pos).collect();
                let succs: Vec<[f32; 2]> = self.g.links.iter().filter(|l| l.from == id).filter_map(|l| self.node(l.to)).filter(|n| !new.contains(&n.id)).map(|n| n.pos).collect();
                pos = if !preds.is_empty() {
                    [preds.iter().map(|p| p[0]).fold(f32::MIN, f32::max) + 250.0, preds.iter().map(|p| p[1]).fold(f32::MAX, f32::min)]
                } else if !succs.is_empty() {
                    [succs.iter().map(|p| p[0]).fold(f32::MAX, f32::min) - 250.0, succs.iter().map(|p| p[1]).fold(f32::MAX, f32::min)]
                } else {
                    [x0, y0]
                };
            }
            let size = self.node(id).map(node_rect).unwrap_or([0.0, 0.0, 190.0, 120.0]);
            let mut r = [pos[0], pos[1], size[2], size[3]];
            let mut guard = 0;
            while let Some(hit) = fixed.iter().find(|f| rects_touch(r, **f, 24.0)) {
                r[1] = hit[1] + hit[3] + 28.0;
                guard += 1;
                if guard > 500 {
                    break;
                }
            }
            if let Some(n) = self.g.nodes.iter_mut().find(|n| n.id == id) {
                n.pos = [r[0].round(), r[1].round()];
            }
            fixed.push(r);
        }
        // La sortie reste à droite de ce qui y arrive
        if let Some(out) = self.g.nodes.iter().find(|n| nodes::is_out_op(&n.op)).map(|n| (n.id, n.pos)) {
            let feeders: Vec<[f32; 2]> = self.g.links.iter().filter(|l| l.to == out.0 && new.contains(&l.from)).filter_map(|l| self.node(l.from)).map(|n| n.pos).collect();
            if let Some(far) = feeders.iter().map(|p| p[0]).reduce(f32::max) {
                if out.1[0] < far + 240.0 {
                    let y = feeders.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
                    if let Some(n) = self.g.nodes.iter_mut().find(|n| n.id == out.0) {
                        n.pos = [far + 250.0, y];
                    }
                }
            }
        }
        for (ids, label, note) in std::mem::take(&mut self.frames) {
            let rects: Vec<[f32; 4]> = ids.iter().filter_map(|id| self.node(*id)).map(node_rect).collect();
            if rects.is_empty() {
                continue;
            }
            let x0 = rects.iter().map(|r| r[0]).fold(f32::MAX, f32::min) - 20.0;
            let y0 = rects.iter().map(|r| r[1]).fold(f32::MAX, f32::min) - 40.0;
            let x1 = rects.iter().map(|r| r[0] + r[2]).fold(f32::MIN, f32::max) + 20.0;
            let y1 = rects.iter().map(|r| r[1] + r[3]).fold(f32::MIN, f32::max) + 20.0;
            self.g.frames.push(Frame { rect: [x0, y0, x1 - x0, y1 - y0], label, color: [90, 120, 200], note });
        }
    }
}

/// Définition d'un node par son type (« geo_grid ») ou son libellé (« Grille »), dans ce genre de graphe.
fn find_def(ty: &str, kind: GraphKind) -> Result<&'static Def, String> {
    let t = plain(ty);
    let hit = DEFS.iter().find(|d| d.op == ty.trim()).or_else(|| DEFS.iter().find(|d| d.fits(kind) && (plain(d.label) == t || plain(&crate::i18n::en(d.label)) == t)));
    match hit {
        Some(d) if d.fits(kind) && d.cat != "interne" => Ok(d),
        Some(d) => Err(format!("le node « {} » ({}) n'existe pas dans un graphe de {}", d.label, d.op, kind_name(kind))),
        None => {
            let near: Vec<String> = DEFS.iter().filter(|d| usable(d, kind) && (d.op.contains(t.as_str()) || t.contains(d.op) || plain(d.label).contains(&t))).take(6).map(|d| format!("{} « {} »", d.op, d.label)).collect();
            Err(format!("type de node « {ty} » inconnu{}", if near.is_empty() { " : n'utilise que les types du catalogue".to_string() } else { format!(" (proches : {})", near.join(", ")) }))
        }
    }
}

/// Applique des opérations de nodes à un graphe (un niveau). Rend le résumé, ou les erreurs.
fn apply_nodes(g: &mut Graph, ops: &[&Value], replace: bool, base: usize) -> (Vec<String>, Vec<String>) {
    let mut done = vec![];
    if replace {
        let kind = g.kind;
        let (time, stack) = (g.time, g.stack.take());
        *g = Graph::empty(kind);
        g.time = time;
        g.stack = stack;
        let out = g.out_op();
        g.add(out, [0.0, 0.0]);
        done.push(crate::i18n::t("repart d'un graphe vide"));
    }
    let mut w = Work { g, alias: HashMap::new(), new: vec![], unplaced: vec![], frames: vec![], done, };
    if replace {
        // La sortie fait partie de ce qu'on range
        if let Some(id) = w.g.nodes.first().map(|n| n.id) {
            w.new.push(id);
            w.unplaced.push(id);
        }
    }
    let mut errs = vec![];
    for (k, v) in ops.iter().enumerate() {
        let r = match v {
            Value::Object(o) => w.op(o),
            _ => Err("une opération est un objet {\"op\": …}".into()),
        };
        if let Err(e) = r {
            let what = v.get("op").and_then(|x| x.as_str()).unwrap_or("?");
            errs.push(format!("{} {} ({what}) : {e}", crate::i18n::t("opération"), base + k + 1));
        }
    }
    if errs.is_empty() {
        w.layout();
        nodeflow::sync(w.g);
    }
    (w.done, errs)
}

fn layer_ref(st: &atelier::Stack, v: &Value) -> Result<usize, String> {
    let s = match v {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => format!("c{n}"),
        _ => return Err("calque attendu : c<rang> ou son nom".into()),
    };
    let names: Vec<&str> = st.layers.iter().map(|l| l.name.as_str()).collect();
    if let Some(k) = s.strip_prefix('c').or_else(|| s.strip_prefix('C')).and_then(|d| d.parse::<usize>().ok()) {
        return if k >= 1 && k <= st.layers.len() { Ok(k - 1) } else { Err(format!("le calque c{k} n'existe pas (la pile en a {})", st.layers.len())) };
    }
    find_name(&names, &s).ok_or_else(|| format!("calque « {s} » introuvable (calques : {})", names.join(", ")))
}

fn mask_from(base: &Mask, v: &Value) -> Result<Mask, String> {
    let mut m = base.clone();
    let kinds: Vec<&str> = atelier::MASKS.iter().map(|x| x.0).filter(|k| *k != "graphe").collect();
    let o = match v {
        Value::String(s) => {
            let k = find_name(&kinds, s).ok_or_else(|| format!("genre de masque « {s} » inconnu ({})", kinds.join(", ")))?;
            m.kind = kinds[k].to_string();
            return Ok(m);
        }
        Value::Object(o) => o,
        _ => return Err("« masque » est un objet {genre, quantite, …}".into()),
    };
    if let Some(s) = get(o, &["genre", "kind", "type"]).and_then(|v| v.as_str()) {
        let k = find_name(&kinds, s).ok_or_else(|| format!("genre de masque « {s} » inconnu ({})", kinds.join(", ")))?;
        m.kind = kinds[k].to_string();
    }
    for (names, slot, lo, hi) in [(&["quantite", "amount"][..], 0, 0.0, 1.0), (&["contraste", "contrast"][..], 1, 0.0, 1.0), (&["echelle", "scale"][..], 2, 0.1, 200.0), (&["quantite_cassure", "breakup_amount"][..], 3, 0.0, 1.0), (&["echelle_cassure", "breakup_scale"][..], 4, 0.1, 200.0), (&["canal", "paint"][..], 5, 1.0, 4.0)] {
        if let Some(v) = get(o, names) {
            let x = as_f32(v).ok_or_else(|| format!("nombre attendu pour « {} »", names[0]))?.clamp(lo, hi);
            match slot {
                0 => m.amount = x,
                1 => m.contrast = x,
                2 => m.scale = x,
                3 => m.breakup_amount = x,
                4 => m.breakup_scale = x,
                _ => m.paint = x.round() as u8 - 1,
            }
        }
    }
    if let Some(b) = get(o, &["inverser", "invert", "inverse"]).and_then(|v| v.as_bool()) {
        m.invert = b;
    }
    if let Some(v) = get(o, &["cassure", "breakup"]) {
        let s = v.as_str().unwrap_or("");
        if s.trim().is_empty() || matches!(plain(s).as_str(), "rien" | "aucune" | "non") {
            m.breakup = String::new();
        } else {
            let gens: Vec<&str> = atelier::MASKS.iter().filter(|x| !x.2.is_empty() && x.0 != "peint").map(|x| x.0).collect();
            let k = find_name(&gens, s).ok_or_else(|| format!("cassure « {s} » inconnue ({})", gens.join(", ")))?;
            m.breakup = gens[k].to_string();
        }
    }
    Ok(m)
}

/// Opacité, visibilité, masque, nom, paramètres exposés : ce qu'« ajouter_calque » et « regler_calque » partagent.
fn tune_layer(l: &mut Layer, o: &Map<String, Value>, root: &str) -> Result<Vec<String>, String> {
    let mut said = vec![];
    if let Some(v) = get(o, &["opacite", "opacity"]) {
        l.opacity = as_f32(v).ok_or("« opacite » est un nombre de 0 à 1")?.clamp(0.0, 1.0);
        said.push(format!("opacité {}", num(l.opacity)));
    }
    if let Some(b) = get(o, &["visible"]).and_then(|v| v.as_bool()) {
        l.visible = b;
        said.push((if b { "visible" } else { "caché" }).to_string());
    }
    if let Some(v) = get(o, &["masque", "mask"]) {
        l.mask = mask_from(&l.mask, v)?;
        said.push(describe_mask(&l.mask));
    }
    // Paramètres exposés : « parametres », et les raccourcis rugosite / metal / couleur d'un remplissage
    let mut params: Vec<(String, Value)> = vec![];
    if let Some(Value::Object(p)) = get(o, &["parametres", "params", "reglages"]) {
        params.extend(p.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    for (names, pname) in [(&["rugosite", "roughness"][..], "rugosité"), (&["metal", "metallic"][..], "métal")] {
        if let Some(v) = get(o, names) {
            params.push((pname.to_string(), v.clone()));
        }
    }
    if !params.is_empty() {
        let exposed = l.fill(root).map(|f| atelier::exposed(&f)).unwrap_or_default();
        let names: Vec<&str> = exposed.iter().map(|e| e.name.as_str()).collect();
        for (k, v) in params {
            let i = find_name(&names, &k).ok_or_else(|| format!("le calque « {} » n'expose pas de paramètre « {k} » (paramètres : {})", l.name, if names.is_empty() { "aucun".to_string() } else { names.join(", ") }))?;
            let sock = if exposed[i].color { Sock::Color } else { Sock::Float };
            let val = parse_value(sock, &v).map_err(|e| format!("{e} pour « {} »", names[i]))?;
            said.push(format!("{} : {}", names[i], show_value(sock, val)));
            match &mut l.graph {
                // Graphe propre : la valeur vit dans son node « param »
                Some(inner) => {
                    for n in inner.0.nodes.iter_mut().chain(inner.0.groups.iter_mut().flat_map(|g| g.nodes.iter_mut())).filter(|n| n.op == "param" && atelier::param_name(n) == names[i]) {
                        if let Some(slot) = n.values.first_mut() {
                            *slot = val;
                        }
                    }
                }
                None => {
                    l.params.insert(names[i].to_string(), val);
                }
            }
        }
    }
    Ok(said)
}

fn layer_op(g: &mut Graph, kind: &str, o: &Map<String, Value>, root: &str) -> Result<String, String> {
    let st = g.stack.as_mut().ok_or("ce matériau n'a pas de pile de calques ici : reviens à la pile pour agir sur les calques")?;
    match kind {
        "ajouter_calque" | "add_layer" => {
            let name = get(o, &["nom", "name"]).and_then(|v| v.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            let mut l = if let Some(m) = get(o, &["maitre", "master"]).and_then(|v| v.as_str()) {
                let rel = m.trim().trim_start_matches("./").to_string();
                if !std::path::Path::new(root).join(&rel).is_file() {
                    return Err(format!("le matériau maître « {rel} » n'est pas dans le coffre"));
                }
                Layer::linked(name.clone().unwrap_or_else(|| crate::vault::stem(&rel)), rel)
            } else if let Some(p) = get(o, &["prereglage", "preset"]).and_then(|v| v.as_str()) {
                let k = find_name(&nodes::MATERIAL_PRESETS, p).ok_or_else(|| format!("préréglage « {p} » inconnu ({})", nodes::MATERIAL_PRESETS.join(", ")))?;
                let pn = nodes::MATERIAL_PRESETS[k];
                Layer::own(name.clone().unwrap_or_else(|| pn.to_string()), nodes::material_preset(pn).ok_or("préréglage introuvable")?)
            } else {
                let c = match get(o, &["couleur", "color"]) {
                    Some(v) => parse_value(Sock::Color, v).map_err(|e| format!("{e} pour « couleur »"))?,
                    None => [0.5, 0.5, 0.5, 1.0],
                };
                Layer::own(name.clone().unwrap_or_else(|| crate::i18n::t("calque")), atelier::fill_graph([c[0], c[1], c[2]]))
            };
            let said = tune_layer(&mut l, o, root)?;
            let line = format!("{} « {} »{}", crate::i18n::t("ajoute le calque"), l.name, if said.is_empty() { String::new() } else { format!(" ({})", said.join(", ")) });
            let at = get(o, &["position", "rang"]).and_then(as_f32).map(|x| (x.round().max(1.0) as usize - 1).min(st.layers.len())).unwrap_or(st.layers.len());
            st.layers.insert(at, l);
            Ok(line)
        }
        "regler_calque" | "set_layer" | "modifier_calque" => {
            let k = layer_ref(st, get(o, &["calque", "layer"]).ok_or("il manque « calque »")?)?;
            let before = st.layers[k].name.clone();
            if let Some(n) = get(o, &["nom", "name"]).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) {
                st.layers[k].name = n.to_string();
            }
            let said = tune_layer(&mut st.layers[k], o, root)?;
            Ok(format!("{} « {before} »{}", crate::i18n::t("règle le calque"), if said.is_empty() { String::new() } else { format!(" ({})", said.join(", ")) }))
        }
        "supprimer_calque" | "delete_layer" | "remove_layer" => {
            let k = layer_ref(st, get(o, &["calque", "layer"]).ok_or("il manque « calque »")?)?;
            if st.layers.len() <= 1 {
                return Err("un matériau garde au moins un calque".into());
            }
            let l = st.layers.remove(k);
            Ok(format!("{} « {} »", crate::i18n::t("supprime le calque"), l.name))
        }
        "deplacer_calque" | "move_layer" => {
            let k = layer_ref(st, get(o, &["calque", "layer"]).ok_or("il manque « calque »")?)?;
            let to = get(o, &["position", "rang", "vers"]).and_then(as_f32).ok_or("il manque « position » (1 = tout en bas)")?;
            let to = (to.round().max(1.0) as usize - 1).min(st.layers.len() - 1);
            let l = st.layers.remove(k);
            let line = format!("{} « {} » → c{}", crate::i18n::t("déplace le calque"), l.name, to + 1);
            st.layers.insert(to, l);
            Ok(line)
        }
        _ => Err("opération de calque inconnue".into()),
    }
}

fn is_layer_op(v: &Value) -> Option<String> {
    let k = key(v.get("op").or_else(|| v.get("operation"))?.as_str()?);
    (k.ends_with("_calque") || k.ends_with("_layer")).then_some(k)
}

/// Applique un bloc à `g`, le niveau de graphe que l'artiste a sous les yeux (racine, intérieur d'un
/// groupe, graphe d'un calque, ou pile de calques d'un matériau). `g` n'est modifié que si tout passe.
/// Rend le résumé lisible des changements, ou la liste des erreurs.
pub fn apply(g: &mut Graph, edit: &Edit, root: &str) -> Result<Vec<String>, Vec<String>> {
    if edit.ops.is_empty() && !edit.replace {
        return Err(vec![crate::i18n::t("le bloc ne contient aucune opération")]);
    }
    let mut work = g.clone();
    let mut done = vec![];
    let mut errs = vec![];
    // 1. Les opérations de calque, dans l'ordre
    let mut node_ops: Vec<&Value> = vec![];
    let mut first_node_op = 0;
    for (k, v) in edit.ops.iter().enumerate() {
        match (is_layer_op(v), v) {
            (Some(kind), Value::Object(o)) => match layer_op(&mut work, &kind, o, root) {
                Ok(line) => done.push(line),
                Err(e) => errs.push(format!("{} {} ({kind}) : {e}", crate::i18n::t("opération"), k + 1)),
            },
            _ => {
                if node_ops.is_empty() {
                    first_node_op = k;
                }
                node_ops.push(v);
            }
        }
    }
    // 2. Les opérations de nodes : sur ce niveau, ou dans le graphe d'un calque de la pile
    if !node_ops.is_empty() || (edit.replace && done.is_empty()) {
        if work.stack.is_some() {
            match layer_graph(&mut work, edit) {
                Ok((k, mut inner)) => {
                    let lname = work.stack.as_ref().map(|s| s.layers[k].name.clone()).unwrap_or_default();
                    let (d, e) = apply_nodes(&mut inner, &node_ops, edit.replace, first_node_op);
                    done.push(format!("{} « {lname} »{} :", crate::i18n::t("dans le calque"), if edit.mask { format!(" ({})", crate::i18n::t_mid("masque")) } else { String::new() }));
                    done.extend(d);
                    errs.extend(e);
                    if let Some(l) = work.stack.as_mut().and_then(|s| s.layers.get_mut(k)) {
                        let inner = Some(atelier::Inner(Box::new(inner)));
                        if edit.mask {
                            l.mask.kind = "graphe".into();
                            l.mask.graph = inner;
                        } else {
                            l.graph = inner;
                        }
                    }
                }
                Err(e) => errs.push(e),
            }
        } else {
            let (d, e) = apply_nodes(&mut work, &node_ops, edit.replace, first_node_op);
            done.extend(d);
            errs.extend(e);
        }
    }
    if !errs.is_empty() {
        return Err(errs);
    }
    *g = work;
    Ok(done)
}

/// Le graphe du calque visé par le bloc (son remplissage ou son masque).
fn layer_graph(work: &mut Graph, edit: &Edit) -> Result<(usize, Graph), String> {
    let st = work.stack.as_ref().ok_or("pas de pile de calques")?;
    let k = match &edit.layer {
        Some(v) => layer_ref(st, v)?,
        None => {
            let own: Vec<usize> = st.layers.iter().enumerate().filter(|(_, l)| l.graph.is_some()).map(|(k, _)| k).collect();
            match own.as_slice() {
                [k] => *k,
                _ => return Err("ce matériau est une pile de calques : précise le calque visé avec \"calque\":\"c1\" à côté de \"ops\", ou utilise les opérations de calque".into()),
            }
        }
    };
    let l = &st.layers[k];
    if edit.mask {
        return Ok((k, match (&l.mask.graph, l.mask.kind.as_str()) {
            (Some(g), "graphe") => (*g.0).clone(),
            _ => atelier::mask_graph(&l.mask),
        }));
    }
    match &l.graph {
        Some(g) => Ok((k, (*g.0).clone())),
        None => Err(format!("le calque « {} » est lié au matériau maître {} : ouvre ce fichier pour modifier ses nodes", l.name, l.master)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La réponse type à « fais-moi un drapeau en simulation de tissu » : l'exemple de la notice.
    fn flag_answer() -> String {
        let n = notice(GraphKind::Geometry, false, "sim/drapeau.arygeo");
        let a = n.rfind("```graphe\n").unwrap();
        let b = n[a + 3..].find("```").unwrap() + a + 6;
        format!("Voici un drapeau accroché par son bord gauche, simulé sur la carte graphique.\n\n{}\nLance la lecture pour le voir flotter.", &n[a..b])
    }

    fn one(text: &str) -> Edit {
        let mut b = blocks(text);
        assert_eq!(b.len(), 1, "un seul bloc attendu");
        b.remove(0).expect("bloc lisible")
    }

    fn find<'a>(g: &'a Graph, op: &str) -> &'a Node {
        g.nodes.iter().find(|n| n.op == op).unwrap_or_else(|| panic!("pas de node {op}"))
    }

    fn linked(g: &Graph, from: &str, out: usize, to: &str, inp: usize) -> bool {
        g.links.iter().any(|l| l.out == out && l.inp == inp && g.nodes.iter().any(|n| n.id == l.from && n.op == from) && g.nodes.iter().any(|n| n.id == l.to && n.op == to))
    }

    #[test]
    fn lecture_des_blocs() {
        let t = "Texte\n```graphe\n{\"ops\": [ {\"op\":\"supprimer\",\"noeuds\":[\"n1\"],}, // commentaire\n ]}\n```\nfin\n```rust\nfn a(){}\n```\n```graphe\npas du json\n```\n```graphe\n{\"ops\":[";
        let b = blocks(t);
        assert_eq!(b.len(), 2, "le bloc non refermé est ignoré");
        assert_eq!(b[0].as_ref().unwrap().ops.len(), 1, "virgule en trop et commentaire tolérés");
        assert!(b[1].is_err());
        let s = strip_blocks(t);
        assert!(s.contains("fn a(){}") && !s.contains("supprimer") && s.contains("fin"));
        // Un bloc ```json qui a la forme d'un changement est pris aussi ; un autre JSON ne l'est pas
        let j = "```json\n{\"ops\":[{\"op\":\"zone\"}]}\n```\n```json\n{\"a\":1}\n```";
        assert_eq!(blocks(j).len(), 1);
        assert!(strip_blocks(j).contains("\"a\":1") && !strip_blocks(j).contains("zone"));
        // Une liste nue, ou une opération seule
        assert_eq!(one("```graphe\n[{\"op\":\"zone\"}]\n```").ops.len(), 1);
        assert_eq!(one("```graphe\n{\"op\":\"zone\"}\n```").ops.len(), 1);
        let e = one("```graphe\n{\"Fichier\":\"a.arygeo\",\"remplacer\":true,\"calque\":\"c2\",\"partie\":\"masque\",\"opérations\":[]}\n```");
        assert!(e.replace && e.mask && e.file.as_deref() == Some("a.arygeo") && e.layer.is_some());
    }

    #[test]
    fn drapeau_en_tissu_sur_carte_graphique() {
        let edit = one(&flag_answer());
        assert!(edit.replace);
        assert_eq!(edit.file.as_deref(), Some("sim/drapeau.arygeo"), "le chemin du document est repris dans l'exemple");
        // Appliqué au graphe de départ (le tissu sur une boule) : il est remplacé
        let mut g = Graph::new_geometry();
        let done = apply(&mut g, &edit, "").unwrap_or_else(|e| panic!("{}", e.join("\n")));
        assert!(done.len() >= 20, "{done:?}");
        // Grille debout → épinglage du bord → zone { forces (vent) → solveur tissu carte graphique } → sortie
        let grid = find(&g, "geo_grid");
        assert_eq!(pstr(grid, "plan"), "mur (XY)");
        assert_eq!(pstr(grid, "arêtes"), "avec diagonales");
        assert_eq!((grid.values[0][0], grid.values[2][0]), (3.0, 48.0));
        assert_eq!(grid.label, "Drapeau");
        let pin = find(&g, "geo_attr");
        assert_eq!((pstr(pin, "nom"), pstr(pin, "type")), ("pin", "nombre"));
        assert!(linked(&g, "position", 0, "sepxyz", 0) && linked(&g, "sepxyz", 0, "compare", 0) && linked(&g, "compare", 0, "geo_attr", 2));
        assert!(linked(&g, "geo_grid", 0, "geo_attr", 0) && linked(&g, "geo_attr", 0, "sim_in", 0));
        // Début de zone : sortie 0 « pas de temps », sortie 1 « géométrie »
        assert!(linked(&g, "sim_in", 1, "sim_forces", 0) && linked(&g, "sim_in", 0, "sim_forces", 4));
        assert!(linked(&g, "sim_forces", 0, "sim_solver", 0) && linked(&g, "sim_in", 0, "sim_solver", 4));
        assert!(linked(&g, "sim_solver", 0, "sim_out", 0) && linked(&g, "sim_out", 0, "geo_out", 0));
        let solver = find(&g, "sim_solver");
        assert_eq!(pstr(solver, "solveur"), crate::geo::SOLVER_NAMES[1]);
        assert_eq!(pstr(solver, "sol"), "non");
        assert_eq!(find(&g, "sim_forces").values[2], [14.0, 0.0, 3.0, 0.0]);
        assert_eq!(g.time.map(|t| t.end), Some(240));
        assert_eq!(nodeflow::zone_pair(&g, find(&g, "sim_in").id), Some(find(&g, "sim_out").id));
        assert_eq!(g.nodes.len(), 10);
        // Rangé sans chevauchement, la sortie tout à droite
        let rects: Vec<[f32; 4]> = g.nodes.iter().map(node_rect).collect();
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                assert!(!rects_touch(*a, *b, 0.0), "deux nodes se chevauchent : {a:?} {b:?}");
            }
        }
        let out_x = find(&g, "geo_out").pos[0];
        assert!(g.nodes.iter().all(|n| n.pos[0] <= out_x));
        // Le graphe se met à plat sans erreur et se relit tel quel
        nodeflow::lower(&g, false).map_err(|e| e.1).unwrap();
        let back: Graph = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert_eq!(back.nodes.len(), g.nodes.len());
    }

    /// Le drapeau bouge vraiment : calculé avec le solveur de référence (le test ne dépend pas d'une
    /// carte graphique), le bord épinglé reste en place et le reste part dans le vent.
    #[test]
    fn le_drapeau_se_simule() {
        let mut g = Graph::new_geometry();
        apply(&mut g, &one(&flag_answer()), "").unwrap();
        let tweak = one("```graphe\n{\"ops\":[{\"op\":\"regler\",\"noeud\":\"sim_solver\",\"reglages\":{\"solveur\":\"xpbd\"}},{\"op\":\"regler\",\"noeud\":\"geo_grid\",\"valeurs\":{\"points en largeur\":12,\"points en profondeur\":8}}]}\n```");
        let done = apply(&mut g, &tweak, "").unwrap();
        assert!(done[0].contains("XPBD"), "{done:?}");
        let mut eng = crate::geo::Engine::default();
        let first = eng.eval(&g, 1, "").map_err(|e| e.1).unwrap();
        let mut last = first.clone();
        for f in 2..=12 {
            last = eng.eval(&g, f, "").map_err(|e| e.1).unwrap();
        }
        assert_eq!(first.n, 96);
        let pin = last.col("pin").expect("attribut pin").data()[0].clone();
        let (mut pinned, mut moved) = (0, 0.0f32);
        for i in 0..first.n {
            let (a, b) = (first.pos(i), last.pos(i));
            let d = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
            if pin[i] > 0.5 {
                pinned += 1;
                assert!(d < 1e-4, "un point épinglé a bougé de {d}");
                assert!(a[0] < -1.49);
            } else {
                moved = moved.max(d);
            }
        }
        assert_eq!(pinned, 8, "la colonne du mât");
        assert!(moved > 0.05, "le tissu libre doit bouger ({moved})");
    }

    #[test]
    fn modifier_un_graphe_existant() {
        let mut g = Graph::new_geometry();
        let solver = find(&g, "sim_solver").id;
        let ball = find(&g, "geo_sphere").id;
        let n = g.nodes.len();
        let t = format!(
            "```graphe\n{{\"ops\":[\
             {{\"op\":\"regler\",\"noeud\":\"n{solver}\",\"réglages\":{{\"Solveur\":\"tissu (carte graphique)\"}},\"valeurs\":{{\"itérations\":24,\"centre de la boule\":[0,1,0]}}}},\
             {{\"op\":\"supprimer\",\"noeud\":\"n{ball}\"}},\
             {{\"op\":\"ajouter\",\"id\":\"vent\",\"type\":\"Bruit\",\"valeurs\":{{\"échelle\":2}}}},\
             {{\"op\":\"relier\",\"de\":\"vent.couleur\",\"vers\":\"sim_forces.vent\"}},\
             {{\"op\":\"delier\",\"vers\":\"geo_out.géométrie\"}},\
             {{\"op\":\"cadre\",\"noeuds\":[\"vent\",\"sim_forces\"],\"titre\":\"vent turbulent\"}}\
             ]}}\n```"
        );
        let done = apply(&mut g, &one(&t), "").unwrap_or_else(|e| panic!("{}", e.join("\n")));
        assert_eq!(done.len(), 6, "{done:?}");
        assert_eq!(g.nodes.len(), n, "un node en moins, un en plus");
        let s = find(&g, "sim_solver");
        assert_eq!(pstr(s, "solveur"), "tissu (carte graphique)");
        assert_eq!((s.values[1][0], s.values[6]), (24.0, [0.0, 1.0, 0.0, 0.0]));
        assert!(linked(&g, "noise", 1, "sim_forces", 2));
        assert!(!g.links.iter().any(|l| l.to == find(&g, "geo_out").id));
        assert_eq!(g.frames.last().unwrap().label, "vent turbulent");
        // Le nouveau node ne recouvre personne
        let new = node_rect(find(&g, "noise"));
        assert!(g.nodes.iter().filter(|n| n.op != "noise").all(|n| !rects_touch(new, node_rect(n), 0.0)));
    }

    #[test]
    fn erreurs_claires_et_rien_a_moitie() {
        let mut g = Graph::new_geometry();
        let before = serde_json::to_string(&g).unwrap();
        let t = "```graphe\n{\"ops\":[\
                 {\"op\":\"ajouter\",\"id\":\"a\",\"type\":\"geo_grid\",\"valeurs\":{\"largeur\":5}},\
                 {\"op\":\"ajouter\",\"id\":\"b\",\"type\":\"cloth_sim\"},\
                 {\"op\":\"ajouter\",\"id\":\"c\",\"type\":\"blur\"},\
                 {\"op\":\"regler\",\"noeud\":\"sim_solver\",\"reglages\":{\"solveur\":\"fluide\"}},\
                 {\"op\":\"regler\",\"noeud\":\"sim_solver\",\"valeurs\":{\"viscosité\":2}},\
                 {\"op\":\"relier\",\"de\":\"a.géométrie\",\"vers\":\"sim_solver.itérations\"},\
                 {\"op\":\"relier\",\"de\":\"n999.x\",\"vers\":\"a\"},\
                 {\"op\":\"supprimer\",\"noeud\":\"sortie\"},\
                 {\"op\":\"regler\",\"noeud\":\"a\",\"valeurs\":{\"largeur\":\"large\"}},\
                 {\"op\":\"teleporter\"}\
                 ]}\n```";
        let errs = apply(&mut g, &one(t), "").unwrap_err();
        assert_eq!(errs.len(), 9, "{errs:#?}");
        assert!(errs[0].contains("pération 2") && errs[0].contains("cloth_sim") && errs[0].contains("inconnu"));
        assert!(errs[1].contains("n'existe pas dans un graphe de géométrie"));
        assert!(errs[2].contains("n'est pas un choix") && errs[2].contains("tissu (carte graphique)"));
        assert!(errs[3].contains("viscosité") && errs[3].contains("raideur"));
        assert!(errs[4].contains("types incompatibles"));
        assert!(errs[5].contains("n999"));
        assert!(errs[6].contains("sortie ne se supprime pas"));
        assert!(errs[7].contains("nombre attendu"));
        assert!(errs[8].contains("teleporter"));
        assert_eq!(serde_json::to_string(&g).unwrap(), before, "rien n'est appliqué à moitié");
        // Une boucle est refusée
        let t = "```graphe\n{\"ops\":[{\"op\":\"relier\",\"de\":\"sim_solver.géométrie\",\"vers\":\"sim_forces.géométrie\"}]}\n```";
        assert!(apply(&mut g, &one(t), "").unwrap_err()[0].contains("boucle"));
        assert!(apply(&mut g, &one("```graphe\n{\"ops\":[]}\n```"), "").is_err());
    }

    #[test]
    fn groupes_et_zone_de_repetition() {
        let mut g = Graph::new_compositor();
        let t = "```graphe\n{\"ops\":[\
                 {\"op\":\"ajouter\",\"id\":\"grain\",\"type\":\"grain\"},\
                 {\"op\":\"relier\",\"de\":\"vignette\",\"vers\":\"grain.image\"},\
                 {\"op\":\"relier\",\"de\":\"grain\",\"vers\":\"sortie\"},\
                 {\"op\":\"grouper\",\"id\":\"look\",\"noeuds\":[\"vignette\",\"grain\"],\"nom\":\"Look pellicule\"},\
                 {\"op\":\"zone\",\"genre\":\"repetition\",\"debut\":\"rep\",\"fin\":\"fin\",\"iterations\":3},\
                 {\"op\":\"ajouter\",\"id\":\"flou\",\"type\":\"blur\"},\
                 {\"op\":\"relier\",\"de\":\"look\",\"vers\":\"rep.valeur\"},\
                 {\"op\":\"relier\",\"de\":\"rep.valeur\",\"vers\":\"flou.image\"},\
                 {\"op\":\"relier\",\"de\":\"flou\",\"vers\":\"fin.valeur\"},\
                 {\"op\":\"relier\",\"de\":\"fin.valeur\",\"vers\":\"sortie.image\"}\
                 ]}\n```";
        let done = apply(&mut g, &one(t), "").unwrap_or_else(|e| panic!("{}", e.join("\n")));
        assert!(done.iter().any(|d| d.contains("Look pellicule")));
        assert_eq!(g.groups.len(), 1);
        assert_eq!(g.groups[0].name, "Look pellicule");
        assert_eq!(find(&g, "repeat_in").values[0][0], 3.0);
        assert!(linked(&g, "group", 0, "repeat_in", 1) && linked(&g, "repeat_out", 0, "comp_out", 0));
        nodes::Program::compile(&g, "", 16).map_err(|e| e.1).unwrap();
        // Le groupe se repose par son nom, puis se défait
        let t = "```graphe\n[{\"op\":\"ajouter\",\"id\":\"x\",\"type\":\"groupe\",\"groupe\":\"look pellicule\"},{\"op\":\"degrouper\",\"noeud\":\"x\"}]\n```";
        apply(&mut g, &one(t), "").unwrap_or_else(|e| panic!("{}", e.join("\n")));
        assert!(g.nodes.iter().filter(|n| n.op == "grain").count() == 1);
    }

    #[test]
    fn calques_de_l_atelier() {
        let mut g = atelier::to_stack(Graph::new_material());
        let t = "```graphe\n{\"ops\":[\
                 {\"op\":\"ajouter_calque\",\"nom\":\"Rouille\",\"couleur\":[0.45,0.2,0.1],\"rugosite\":0.9,\"masque\":{\"genre\":\"creux\",\"quantite\":0.6,\"cassure\":\"grunge\"},\"opacite\":0.8},\
                 {\"op\":\"ajouter_calque\",\"prereglage\":\"velours\"},\
                 {\"op\":\"regler_calque\",\"calque\":\"Rouille\",\"parametres\":{\"couleur\":\"#ff0000\"},\"masque\":{\"inverser\":true}},\
                 {\"op\":\"deplacer_calque\",\"calque\":\"c3\",\"position\":1}\
                 ]}\n```";
        let done = apply(&mut g, &one(t), "").unwrap_or_else(|e| panic!("{}", e.join("\n")));
        assert_eq!(done.len(), 4);
        let st = g.stack.clone().unwrap();
        assert_eq!(st.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), vec!["velours", atelier::GRAPH_LAYER, "Rouille"]);
        let r = &st.layers[2];
        assert_eq!((r.mask.kind.as_str(), r.mask.amount, r.mask.breakup.as_str(), r.mask.invert, r.opacity), ("creux", 0.6, "grunge", true, 0.8));
        let ex = atelier::exposed(&r.graph.as_ref().unwrap().0);
        assert_eq!(ex[0].default, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(ex[1].default[0], 0.9);
        // Les nodes d'un calque : il faut dire lequel dès qu'il y en a plusieurs
        let t = "```graphe\n{\"ops\":[{\"op\":\"ajouter\",\"id\":\"b\",\"type\":\"bricks\"},{\"op\":\"relier\",\"de\":\"b.couleur\",\"vers\":\"sortie.couleur\"}]}\n```";
        assert!(apply(&mut g, &one(t), "").unwrap_err()[0].contains("précise le calque"));
        let t = t.replace("{\"ops\"", "{\"calque\":\"c2\",\"ops\"");
        apply(&mut g, &one(&t), "").unwrap_or_else(|e| panic!("{}", e.join("\n")));
        let inner = g.stack.as_ref().unwrap().layers[1].graph.as_ref().unwrap();
        assert!(linked(&inner.0, "bricks", 0, "mat_out", 0));
        nodes::Program::compile(&g.effective(), "", 16).map_err(|e| e.1).unwrap();
        // Erreurs de calque
        let t = "```graphe\n[{\"op\":\"regler_calque\",\"calque\":\"c9\"},{\"op\":\"ajouter_calque\",\"masque\":\"nuages\"}]\n```";
        assert_eq!(apply(&mut g, &one(t), "").unwrap_err().len(), 2);
    }

    #[test]
    fn contexte_compact_et_complet() {
        for (kind, max) in [(GraphKind::Geometry, 16_000), (GraphKind::Material, 22_000), (GraphKind::Compositor, 26_000)] {
            let c = catalog(kind);
            assert!(c.len() < max, "catalogue {kind:?} trop long : {}", c.len());
            // Tout node du genre est au catalogue, avec ses choix
            for d in DEFS.iter().filter(|d| usable(d, kind)) {
                assert!(c.contains(&format!("\n{} « ", d.op)), "{} manque", d.op);
            }
        }
        let c = catalog(GraphKind::Geometry);
        assert!(c.contains("solveur{tissu et fils, XPBD (processeur)|tissu (carte graphique)}"));
        assert!(c.contains("sim_forces") && !c.contains("\nblur «") && !c.contains("sim_in"));
        let g = Graph::new_geometry();
        let ctx = context(&g, "sim/drapeau.arygeo", None);
        assert!(ctx.contains("Document : sim/drapeau.arygeo") && ctx.contains("```graphe") && ctx.contains("tissu (carte graphique)"));
        let solver = find(&g, "sim_solver");
        assert!(ctx.contains(&format!("n{} sim_solver", solver.id)));
        assert!(ctx.contains("rayon de la boule=0.55") && ctx.contains("arêtes=\"avec diagonales\""));
        assert!(ctx.contains(".géométrie -> n"));
        // Une pile de calques est décrite calque par calque
        let m = atelier::to_stack(Graph::new_material());
        let ctx = context(&m, "m.arymat", None);
        assert!(ctx.contains("c1 «") && ctx.contains("ajouter_calque") && ctx.contains("mat_out"));
        assert!(!context(&Graph::new_compositor(), "c.arycomp", None).contains("ajouter_calque"));
    }
}
