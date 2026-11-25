//! Rendus spéciaux de l'aperçu des notes : maths LaTeX (en caractères Unicode) et diagrammes Mermaid
//! (organigrammes « graph » / « flowchart », disposés en couches).

/// Commandes LaTeX courantes → symbole
const SYMBOLS: &[(&str, &str)] = &[
    ("alpha", "α"), ("beta", "β"), ("gamma", "γ"), ("delta", "δ"), ("epsilon", "ε"), ("varepsilon", "ε"), ("zeta", "ζ"), ("eta", "η"),
    ("theta", "θ"), ("iota", "ι"), ("kappa", "κ"), ("lambda", "λ"), ("mu", "μ"), ("nu", "ν"), ("xi", "ξ"), ("pi", "π"), ("rho", "ρ"),
    ("sigma", "σ"), ("tau", "τ"), ("upsilon", "υ"), ("phi", "φ"), ("varphi", "φ"), ("chi", "χ"), ("psi", "ψ"), ("omega", "ω"),
    ("Gamma", "Γ"), ("Delta", "Δ"), ("Theta", "Θ"), ("Lambda", "Λ"), ("Xi", "Ξ"), ("Pi", "Π"), ("Sigma", "Σ"), ("Phi", "Φ"), ("Psi", "Ψ"), ("Omega", "Ω"),
    ("sum", "∑"), ("prod", "∏"), ("int", "∫"), ("iint", "∬"), ("oint", "∮"), ("partial", "∂"), ("nabla", "∇"), ("infty", "∞"),
    ("cdot", "·"), ("times", "×"), ("div", "÷"), ("pm", "±"), ("mp", "∓"), ("leq", "≤"), ("le", "≤"), ("geq", "≥"), ("ge", "≥"),
    ("neq", "≠"), ("ne", "≠"), ("approx", "≈"), ("equiv", "≡"), ("sim", "∼"), ("propto", "∝"), ("to", "→"), ("rightarrow", "→"),
    ("leftarrow", "←"), ("Rightarrow", "⇒"), ("Leftarrow", "⇐"), ("Leftrightarrow", "⇔"), ("iff", "⇔"), ("mapsto", "↦"),
    ("in", "∈"), ("notin", "∉"), ("subset", "⊂"), ("subseteq", "⊆"), ("supset", "⊃"), ("cup", "∪"), ("cap", "∩"), ("emptyset", "∅"),
    ("forall", "∀"), ("exists", "∃"), ("neg", "¬"), ("land", "∧"), ("lor", "∨"), ("angle", "∠"), ("degree", "°"), ("circ", "∘"),
    ("ldots", "…"), ("cdots", "⋯"), ("dots", "…"), ("hbar", "ℏ"), ("ell", "ℓ"), ("Re", "ℜ"), ("Im", "ℑ"), ("star", "⋆"),
    ("mathbb{R}", "ℝ"), ("mathbb{N}", "ℕ"), ("mathbb{Z}", "ℤ"), ("mathbb{Q}", "ℚ"), ("mathbb{C}", "ℂ"),
    ("quad", "  "), ("qquad", "    "), (",", " "), (";", " "), ("!", ""), ("left", ""), ("right", ""), ("{", "{"), ("}", "}"),
    ("sin", "sin"), ("cos", "cos"), ("tan", "tan"), ("log", "log"), ("ln", "ln"), ("exp", "exp"), ("lim", "lim"), ("max", "max"), ("min", "min"),
];

fn sup(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰', '1' => '¹', '2' => '²', '3' => '³', '4' => '⁴', '5' => '⁵', '6' => '⁶', '7' => '⁷', '8' => '⁸', '9' => '⁹',
        '+' => '⁺', '-' => '⁻', '=' => '⁼', '(' => '⁽', ')' => '⁾', 'n' => 'ⁿ', 'i' => 'ⁱ', 'x' => 'ˣ', 'y' => 'ʸ', 'a' => 'ᵃ',
        'b' => 'ᵇ', 'c' => 'ᶜ', 'd' => 'ᵈ', 'e' => 'ᵉ', 'k' => 'ᵏ', 'm' => 'ᵐ', 't' => 'ᵗ', 'T' => 'ᵀ', '*' => '*', '′' => '′',
        _ => return None,
    })
}

fn sub(c: char) -> Option<char> {
    Some(match c {
        '0' => '₀', '1' => '₁', '2' => '₂', '3' => '₃', '4' => '₄', '5' => '₅', '6' => '₆', '7' => '₇', '8' => '₈', '9' => '₉',
        '+' => '₊', '-' => '₋', '=' => '₌', '(' => '₍', ')' => '₎', 'a' => 'ₐ', 'e' => 'ₑ', 'i' => 'ᵢ', 'j' => 'ⱼ', 'k' => 'ₖ',
        'n' => 'ₙ', 'o' => 'ₒ', 'x' => 'ₓ', 't' => 'ₜ', 'm' => 'ₘ', 'p' => 'ₚ', 's' => 'ₛ', 'r' => 'ᵣ', 'u' => 'ᵤ', 'v' => 'ᵥ',
        _ => return None,
    })
}

/// Le groupe qui suit : {…} ou un seul caractère (ou une commande \xxx)
fn group(chars: &[char], i: &mut usize) -> String {
    while *i < chars.len() && chars[*i] == ' ' {
        *i += 1;
    }
    if *i >= chars.len() {
        return String::new();
    }
    if chars[*i] == '{' {
        let mut depth = 0;
        let start = *i + 1;
        while *i < chars.len() {
            match chars[*i] {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let s: String = chars[start..*i].iter().collect();
                        *i += 1;
                        return tex(&s);
                    }
                }
                _ => {}
            }
            *i += 1;
        }
        return tex(&chars[start..].iter().collect::<String>());
    }
    if chars[*i] == '\\' {
        let start = *i;
        *i += 1;
        while *i < chars.len() && chars[*i].is_ascii_alphabetic() {
            *i += 1;
        }
        return tex(&chars[start..*i].iter().collect::<String>());
    }
    *i += 1;
    chars[*i - 1].to_string()
}

/// Un exposant ou un indice : en petits caractères si tous existent, sinon ^(…) / _(…)
fn script(s: &str, up: bool) -> String {
    let conv: Option<String> = s.chars().map(|c| if up { sup(c) } else { sub(c) }).collect();
    match conv {
        Some(t) => t,
        None if s.chars().count() == 1 => format!("{}{s}", if up { "^" } else { "_" }),
        None => format!("{}({s})", if up { "^" } else { "_" }),
    }
}

/// Une formule LaTeX en texte Unicode lisible (x^2 → x², \frac{a}{b} → a⁄b, \sqrt{x} → √x…)
pub fn tex(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                i += 1;
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    i += 1;
                }
                if i == start && i < chars.len() {
                    i += 1;
                }
                let mut name: String = chars[start..i].iter().collect();
                if name == "mathbb" || name == "text" || name == "mathrm" || name == "operatorname" || name == "mathbf" || name == "mathit" {
                    let g = group(&chars, &mut i);
                    if name == "mathbb" {
                        name = format!("mathbb{{{g}}}");
                    } else {
                        out.push_str(&g);
                        continue;
                    }
                }
                match name.as_str() {
                    "frac" | "dfrac" | "tfrac" => {
                        let a = group(&chars, &mut i);
                        let b = group(&chars, &mut i);
                        let wrap = |s: String| if s.chars().count() > 1 { format!("({s})") } else { s };
                        out.push_str(&format!("{}⁄{}", wrap(a), wrap(b)));
                    }
                    "sqrt" => {
                        let g = group(&chars, &mut i);
                        out.push_str(&if g.chars().count() > 1 { format!("√({g})") } else { format!("√{g}") });
                    }
                    "vec" => out.push_str(&format!("{}\u{20d7}", group(&chars, &mut i))),
                    "hat" => out.push_str(&format!("{}\u{0302}", group(&chars, &mut i))),
                    "bar" | "overline" => out.push_str(&format!("{}\u{0305}", group(&chars, &mut i))),
                    "dot" => out.push_str(&format!("{}\u{0307}", group(&chars, &mut i))),
                    _ => match SYMBOLS.iter().find(|(k, _)| *k == name) {
                        Some((_, v)) => out.push_str(v),
                        None => out.push_str(&name),
                    },
                }
            }
            '^' | '_' => {
                i += 1;
                let g = group(&chars, &mut i);
                out.push_str(&script(&g, c == '^'));
            }
            '{' | '}' => i += 1,
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

// ---------- Mermaid ----------

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Rect,
    Round,
    Diamond,
    Circle,
}

#[derive(Debug, Clone)]
pub struct FlowNode {
    pub id: String,
    pub label: String,
    pub shape: Shape,
    /// Couche (rang dans le sens du diagramme) et place dans la couche
    pub layer: usize,
    pub slot: usize,
}

#[derive(Debug, Clone)]
pub struct Flow {
    /// Gauche → droite (LR) ; sinon de haut en bas (TD)
    pub horizontal: bool,
    pub nodes: Vec<FlowNode>,
    /// (de, vers, étiquette, pointillé)
    pub edges: Vec<(usize, usize, String, bool)>,
}

/// Un nœud écrit « A », « A[Texte] », « A(Texte) », « A{Texte} », « A((Texte)) »
fn parse_node(s: &str) -> Option<(String, Option<(String, Shape)>)> {
    let s = s.trim().trim_end_matches(';');
    if s.is_empty() {
        return None;
    }
    let id_end = s.find(['[', '(', '{']).unwrap_or(s.len());
    let id = s[..id_end].trim().to_string();
    if id.is_empty() || id.contains(' ') {
        return None;
    }
    let rest = &s[id_end..];
    let unq = |t: &str| t.trim().trim_matches('"').to_string();
    let shape = if let Some(t) = rest.strip_prefix("((").and_then(|r| r.strip_suffix("))")) {
        Some((unq(t), Shape::Circle))
    } else if let Some(t) = rest.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        Some((unq(t.trim_start_matches('(').trim_end_matches(')')), Shape::Rect))
    } else if let Some(t) = rest.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        Some((unq(t.trim_start_matches('[').trim_end_matches(']')), Shape::Round))
    } else if let Some(t) = rest.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        Some((unq(t), Shape::Diamond))
    } else {
        None
    };
    Some((id, shape))
}

/// Un organigramme Mermaid (graph / flowchart TD, TB, LR, RL, BT). None : autre genre de diagramme.
pub fn parse_flow(src: &str) -> Option<Flow> {
    let mut lines = src.lines().map(|l| l.trim()).filter(|l| !l.is_empty() && !l.starts_with("%%"));
    let head = lines.next()?;
    let mut words = head.split_whitespace();
    if !matches!(words.next()?, "graph" | "flowchart") {
        return None;
    }
    let horizontal = matches!(words.next().unwrap_or("TD"), "LR" | "RL");
    let mut nodes: Vec<FlowNode> = vec![];
    let mut edges = vec![];
    let mut node = |nodes: &mut Vec<FlowNode>, s: &str| -> Option<usize> {
        let (id, shape) = parse_node(s)?;
        if let Some(k) = nodes.iter().position(|n| n.id == id) {
            if let Some((label, sh)) = shape {
                nodes[k].label = label;
                nodes[k].shape = sh;
            }
            return Some(k);
        }
        let (label, sh) = shape.unwrap_or((id.clone(), Shape::Rect));
        nodes.push(FlowNode { id, label, shape: sh, layer: 0, slot: 0 });
        Some(nodes.len() - 1)
    };
    // Les flèches reconnues, de la plus longue à la plus courte
    const ARROWS: &[(&str, bool)] = &[("-.->", true), ("==>", false), ("-->", false), ("---", false), ("-.-", true)];
    for line in lines {
        if line.starts_with("subgraph") || line == "end" || line.starts_with("style") || line.starts_with("classDef") || line.starts_with("class ") || line.starts_with("click") || line.starts_with("linkStyle") {
            continue;
        }
        // Découpe « A --> B -->|oui| C » en morceaux
        let mut rest = line.to_string();
        let mut prev: Option<usize> = None;
        let mut pending: Option<(String, bool)> = None;
        loop {
            let found = ARROWS.iter().filter_map(|(a, d)| rest.find(a).map(|p| (p, *a, *d))).min_by_key(|x| x.0);
            let Some((pos, arrow, dashed)) = found else {
                if let Some(k) = node(&mut nodes, &rest) {
                    if let (Some(p), Some((label, d))) = (prev, pending.take()) {
                        edges.push((p, k, label, d));
                    }
                }
                break;
            };
            let left = rest[..pos].to_string();
            let mut after = rest[pos + arrow.len()..].to_string();
            // Étiquette « -->|texte| »
            let mut label = String::new();
            if let Some(r) = after.trim_start().strip_prefix('|') {
                if let Some((l, tail)) = r.split_once('|') {
                    label = l.trim().to_string();
                    after = tail.to_string();
                }
            }
            if let Some(k) = node(&mut nodes, &left) {
                if let (Some(p), Some((l, d))) = (prev, pending.take()) {
                    edges.push((p, k, l, d));
                }
                prev = Some(k);
            }
            pending = Some((label, dashed));
            rest = after;
        }
    }
    if nodes.is_empty() {
        return None;
    }
    // Couches : le plus long chemin depuis une racine, sans les liens qui remontent (boucles)
    let n = nodes.len();
    let mut back = vec![false; edges.len()];
    {
        // Parcours en profondeur : un lien vers un nœud encore « en cours » remonte
        let mut state = vec![0u8; n];
        fn dfs(v: usize, edges: &[(usize, usize, String, bool)], state: &mut [u8], back: &mut [bool]) {
            state[v] = 1;
            for (k, e) in edges.iter().enumerate() {
                if e.0 == v {
                    match state[e.1] {
                        0 => dfs(e.1, edges, state, back),
                        1 => back[k] = true,
                        _ => {}
                    }
                }
            }
            state[v] = 2;
        }
        for v in 0..n {
            if state[v] == 0 {
                dfs(v, &edges, &mut state, &mut back);
            }
        }
    }
    let mut layer = vec![0usize; n];
    for _ in 0..n {
        let mut changed = false;
        for (k, &(a, b, _, _)) in edges.iter().enumerate() {
            if !back[k] && a != b && layer[b] < layer[a] + 1 && layer[a] + 1 < n {
                layer[b] = layer[a] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut count = std::collections::HashMap::<usize, usize>::new();
    for (k, nd) in nodes.iter_mut().enumerate() {
        nd.layer = layer[k];
        let c = count.entry(layer[k]).or_default();
        nd.slot = *c;
        *c += 1;
    }
    Some(Flow { horizontal, nodes, edges })
}

#[cfg(test)]
mod tests {
    #[test]
    fn maths_unicode() {
        assert_eq!(super::tex("x^2 + y_1"), "x² + y₁");
        assert_eq!(super::tex(r"\frac{a+b}{2}"), "(a+b)⁄2");
        assert_eq!(super::tex(r"\sqrt{x} \leq \pi"), "√x ≤ π");
        assert_eq!(super::tex(r"\sum_{i=0}^{n} i"), "∑ᵢ₌₀ⁿ i");
        assert_eq!(super::tex(r"e^{i\pi} = -1"), "e^(iπ) = -1");
        assert_eq!(super::tex(r"\mathbb{R}"), "ℝ");
    }

    #[test]
    fn mermaid_organigramme() {
        let f = super::parse_flow("graph LR\n  A[Modélisation] --> B(Texturing)\n  B -->|validé| C{Rendu ?}\n  C -.-> A\n  C --> D((Fin))").unwrap();
        assert!(f.horizontal);
        assert_eq!(f.nodes.len(), 4);
        assert_eq!(f.nodes[2].label, "Rendu ?");
        assert_eq!(f.nodes[2].shape, super::Shape::Diamond);
        assert_eq!(f.edges.len(), 4);
        assert_eq!(f.edges[1].2, "validé");
        assert!(f.edges[2].3);
        assert_eq!(f.nodes[3].layer, 3);
        // Chaîne sur une seule ligne
        let g = super::parse_flow("flowchart TD\nA --> B --> C").unwrap();
        assert_eq!(g.edges.len(), 2);
        assert!(super::parse_flow("sequenceDiagram\nA->>B: x").is_none());
    }
}
