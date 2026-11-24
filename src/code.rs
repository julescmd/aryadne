//! Éditeur de code : tout ce qu'on attend d'un vrai éditeur, par-dessus le TextEdit d'egui.
//!
//! - fermeture auto des ( [ { " ' ` et des balises HTML (<div> → </div>)
//! - Entrée : garde l'indentation, ajoute un niveau après { [ ( : et entre deux balises,
//!   continue les listes Markdown
//! - Tab / Maj+Tab : indenter ou désindenter la sélection, abréviations Emmet en HTML
//! - Ctrl+/ commenter · Ctrl+D dupliquer · Ctrl+Maj+K supprimer la ligne · Alt+↑↓ déplacer
//! - Ctrl+F chercher · Ctrl+H remplacer · F3 suivant
//! - suggestions (mots du fichier, mots-clés du langage, [[notes]]) · Ctrl+Espace pour forcer

use eframe::egui::{self, text::CCursor, text::CCursorRange, Event, Id, Key, RichText};
use std::collections::HashMap;

use crate::theme;

#[derive(Default)]
pub struct Editor {
    popup: Option<Popup>,
    pub find: Option<Find>,
    /// Texte à insérer au curseur (envoyé par l'assistant IA).
    pub insert: Option<String>,
    click_accept: Option<usize>,
    scroll_to: Option<(usize, usize)>,
    force_popup: bool,
    /// Curseurs supplémentaires (sélections en octets), en plus du curseur principal.
    pub multi: Vec<(usize, usize)>,
    /// Barre « aller à la ligne » (Ctrl+G)
    pub goto: Option<String>,
    /// Ligne où sauter (depuis la structure, une erreur, la recherche…)
    pub jump: Option<usize>,
    /// Erreurs trouvées à l'enregistrement
    pub diags: Vec<Diag>,
    pub diag_rx: Option<std::sync::mpsc::Receiver<Vec<Diag>>>,
    /// Retour à la ligne forcé pour ce fichier (Alt+Z)
    pub wrap: Option<bool>,
    /// Mots des autres fichiers du même langage (suggestions)
    pub project: std::sync::Arc<Vec<String>>,
    /// Lignes de début des blocs repliés.
    pub folds: std::collections::BTreeSet<usize>,
    /// Client LSP (complétion, erreurs, définition) si un serveur est installé.
    pub lsp: Option<crate::lsp::Doc>,
    pub lsp_tried: bool,
    /// Version de référence pour les marques de modifications (dernier commit git, sinon le texte à l'ouverture)
    pub base: Option<std::sync::Arc<String>>,
    pub base_git: bool,
    pub base_rx: Option<std::sync::mpsc::Receiver<Option<String>>>,
    changes: Option<(u64, std::sync::Arc<String>, Vec<Hunk>)>,
    hunk_open: Option<usize>,
    /// Texte complet à remettre dans l'éditeur (bloc restauré)
    pub revert: Option<String>,
}

// ---------- marques de modifications ----------

/// Bloc de lignes changé : `old` dans la référence, `new` dans le texte actuel (lignes, fin exclue).
#[derive(Clone, Debug, PartialEq)]
pub struct Hunk {
    pub old: std::ops::Range<usize>,
    pub new: std::ops::Range<usize>,
}

impl Hunk {
    /// 0 ajouté, 1 modifié, 2 supprimé
    pub fn kind(&self) -> u8 {
        if self.old.is_empty() { 0 } else if self.new.is_empty() { 2 } else { 1 }
    }
}

/// Différence ligne à ligne (plus longue sous-suite commune, après les débuts et fins communs).
pub fn line_diff(old: &str, new: &str) -> Vec<Hunk> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let mut pre = 0;
    while pre < a.len() && pre < b.len() && a[pre] == b[pre] {
        pre += 1;
    }
    let mut suf = 0;
    while suf < a.len() - pre && suf < b.len() - pre && a[a.len() - 1 - suf] == b[b.len() - 1 - suf] {
        suf += 1;
    }
    let (am, bm) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
    if am.is_empty() && bm.is_empty() {
        return vec![];
    }
    // Trop gros pour la table : un seul bloc
    if am.len().saturating_mul(bm.len()) > 4_000_000 || am.is_empty() || bm.is_empty() {
        return vec![Hunk { old: pre..pre + am.len(), new: pre..pre + bm.len() }];
    }
    let (n, m) = (am.len(), bm.len());
    let mut t = vec![0u32; (n + 1) * (m + 1)];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[i * (m + 1) + j] = if am[i] == bm[j] { t[(i + 1) * (m + 1) + j + 1] + 1 } else { t[(i + 1) * (m + 1) + j].max(t[i * (m + 1) + j + 1]) };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    let (mut oi, mut oj) = (0, 0);
    let mut open = false;
    let flush = |out: &mut Vec<Hunk>, oi: usize, i: usize, oj: usize, j: usize| out.push(Hunk { old: pre + oi..pre + i, new: pre + oj..pre + j });
    while i < n || j < m {
        if i < n && j < m && am[i] == bm[j] {
            if open {
                flush(&mut out, oi, i, oj, j);
                open = false;
            }
            i += 1;
            j += 1;
            continue;
        }
        if !open {
            (oi, oj, open) = (i, j, true);
        }
        if j < m && (i == n || t[i * (m + 1) + j + 1] >= t[(i + 1) * (m + 1) + j]) {
            j += 1;
        } else {
            i += 1;
        }
    }
    if open {
        flush(&mut out, oi, i, oj, j);
    }
    out
}

/// Remet les lignes de référence d'un bloc dans le texte.
pub fn revert_hunk(old: &str, new: &str, h: &Hunk) -> String {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let mut out: Vec<&str> = Vec::with_capacity(b.len());
    out.extend_from_slice(&b[..h.new.start.min(b.len())]);
    out.extend_from_slice(&a[h.old.start.min(a.len())..h.old.end.min(a.len())]);
    out.extend_from_slice(&b[h.new.end.min(b.len())..]);
    let mut s = out.join("\n");
    if new.ends_with('\n') || (new.is_empty() && old.ends_with('\n')) {
        s.push('\n');
    }
    s
}

/// Version du fichier au dernier commit git, si le fichier est suivi.
pub fn git_base(path: &std::path::Path) -> Option<String> {
    let dir = path.parent()?;
    let name = path.file_name()?.to_string_lossy().to_string();
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C").arg(dir).arg("show").arg(format!("HEAD:./{name}"));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.stderr(std::process::Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"))
}

#[cfg(test)]
mod diff_tests {
    use super::*;
    #[test]
    fn blocs() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nB\nc\nx\ny\nd\n";
        let h = line_diff(old, new);
        assert_eq!(h, vec![Hunk { old: 1..2, new: 1..2 }, Hunk { old: 3..3, new: 3..5 }, Hunk { old: 4..5, new: 6..6 }]);
        assert_eq!(h.iter().map(|x| x.kind()).collect::<Vec<_>>(), vec![1, 0, 2]);
        let mut t = new.to_string();
        for x in line_diff(old, &t).iter().rev() {
            t = revert_hunk(old, &t, x);
        }
        assert_eq!(t, old);
        assert!(line_diff(old, old).is_empty());
    }

    #[test]
    fn reference_git() {
        let dir = std::env::temp_dir().join(format!("ary-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| std::process::Command::new("git").arg("-C").arg(&dir).args(args).output();
        if git(&["init", "-q"]).is_err() {
            return;
        }
        std::fs::write(dir.join("a.py"), "x = 1\n").unwrap();
        let _ = git(&["add", "."]);
        let _ = git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "i"]);
        std::fs::write(dir.join("a.py"), "x = 2\n").unwrap();
        assert_eq!(git_base(&dir.join("a.py")).as_deref(), Some("x = 1\n"));
        assert_eq!(git_base(&dir.join("b.py")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[derive(Clone, Debug)]
pub struct Diag {
    pub line: usize,
    pub msg: String,
}

struct Popup {
    items: Vec<(String, &'static str)>,
    sel: usize,
    /// Début du mot à remplacer (en caractères).
    start: usize,
    cursor: usize,
}

#[derive(Default)]
pub struct Find {
    pub query: String,
    pub replace: String,
    pub show_replace: bool,
    pub case: bool,
    focus: bool,
    current: usize,
}

// ---------- outils texte ----------

fn byte_of(s: &str, ci: usize) -> usize {
    s.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(s.len())
}
fn char_of(s: &str, b: usize) -> usize {
    s[..b.min(s.len())].chars().count()
}
fn line_start(s: &str, b: usize) -> usize {
    s[..b].rfind('\n').map(|i| i + 1).unwrap_or(0)
}
fn line_end(s: &str, b: usize) -> usize {
    s[b..].find('\n').map(|i| b + i).unwrap_or(s.len())
}
fn indent_of(line: &str) -> &str {
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn html_like(lang: &str) -> bool {
    matches!(lang, "html" | "htm" | "xml" | "svg" | "vue" | "svelte" | "jsx" | "tsx" | "php" | "xhtml")
}

/// Commentaire de ligne (ou paire début/fin) selon le langage.
fn comment(lang: &str) -> (&'static str, &'static str) {
    match lang {
        "py" | "sh" | "bash" | "zsh" | "toml" | "yaml" | "yml" | "rb" | "gd" | "r" | "pl" | "ps1" | "cmake" | "nix" | "conf" | "ini" | "mk" | "dockerfile" => ("# ", ""),
        "lua" | "sql" | "hs" => ("-- ", ""),
        "html" | "htm" | "xml" | "svg" | "md" | "vue" | "svelte" | "xhtml" => ("<!-- ", " -->"),
        "css" | "scss" | "less" => ("/* ", " */"),
        "bat" | "cmd" => ("REM ", ""),
        "vim" => ("\" ", ""),
        _ => ("// ", ""),
    }
}

const VOID: [&str; 14] = ["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr", "param"];

/// Unité d'indentation devinée à partir du fichier.
fn indent_unit(text: &str, lang: &str) -> String {
    let mut two = 0;
    let mut four = 0;
    for l in text.lines().take(2000) {
        if l.starts_with('\t') {
            return "\t".into();
        }
        let n = l.len() - l.trim_start_matches(' ').len();
        if n == 2 || n == 6 {
            two += 1;
        } else if n == 4 || n == 8 {
            four += 1;
        }
    }
    if two > four {
        return "  ".into();
    }
    if four > 0 {
        return "    ".into();
    }
    if matches!(lang, "html" | "htm" | "css" | "scss" | "js" | "ts" | "jsx" | "tsx" | "json" | "yaml" | "yml" | "vue" | "svelte" | "xml" | "svg" | "lua" | "rb") { "  ".into() } else { "    ".into() }
}

pub fn keywords(lang: &str) -> &'static [&'static str] {
    match lang {
        "rs" => &["fn", "let", "mut", "pub", "struct", "enum", "impl", "trait", "match", "if", "else", "for", "while", "loop", "return", "break", "continue", "use", "mod", "crate", "self", "Self", "super", "where", "async", "await", "move", "const", "static", "unsafe", "dyn", "ref", "type", "Some", "None", "Ok", "Err", "Option", "Result", "Vec", "String", "Box", "HashMap", "println!", "format!", "vec!", "derive", "Default", "Clone", "Debug", "impl", "usize", "u8", "u32", "u64", "i32", "i64", "f32", "f64", "bool", "true", "false"],
        "py" => &["def", "class", "import", "from", "return", "if", "elif", "else", "for", "while", "in", "not", "and", "or", "is", "None", "True", "False", "try", "except", "finally", "raise", "with", "as", "lambda", "yield", "pass", "break", "continue", "global", "nonlocal", "async", "await", "print", "range", "len", "self", "__init__", "__name__", "enumerate", "isinstance", "dict", "list", "tuple", "str", "int", "float", "bpy", "bpy.context", "bpy.data", "bpy.ops"],
        "js" | "ts" | "jsx" | "tsx" | "mjs" | "vue" | "svelte" => &["function", "const", "let", "var", "return", "if", "else", "for", "while", "switch", "case", "break", "continue", "class", "extends", "new", "this", "import", "export", "default", "from", "async", "await", "try", "catch", "finally", "throw", "typeof", "instanceof", "null", "undefined", "true", "false", "console.log", "document", "querySelector", "addEventListener", "window", "fetch", "Promise", "interface", "type", "enum", "readonly", "string", "number", "boolean", "Array", "Object", "JSON.stringify", "JSON.parse", "map", "filter", "reduce", "forEach", "length"],
        "html" | "htm" | "xhtml" | "php" => &["div", "span", "section", "article", "header", "footer", "main", "nav", "aside", "button", "input", "label", "form", "img", "video", "audio", "canvas", "script", "style", "link", "meta", "title", "head", "body", "html", "ul", "ol", "li", "table", "thead", "tbody", "tr", "td", "th", "h1", "h2", "h3", "h4", "p", "a", "strong", "em", "code", "pre", "class", "id", "href", "src", "alt", "type", "value", "placeholder", "onclick", "width", "height", "style", "textarea", "select", "option", "figure", "figcaption", "svg"],
        "css" | "scss" | "less" => &["display", "flex", "grid", "position", "absolute", "relative", "fixed", "width", "height", "margin", "padding", "border", "border-radius", "background", "background-color", "color", "font-family", "font-size", "font-weight", "line-height", "text-align", "justify-content", "align-items", "gap", "grid-template-columns", "transform", "transition", "animation", "opacity", "overflow", "z-index", "box-shadow", "cursor", "none", "block", "inline-block", "center", "space-between", "auto", "important", "hover", "focus", "media", "var", "calc", "rgba"],
        "c" | "h" | "cpp" | "hpp" | "cc" | "cs" | "java" | "glsl" | "hlsl" | "frag" | "vert" => &["int", "float", "double", "char", "void", "bool", "struct", "class", "public", "private", "protected", "static", "const", "return", "if", "else", "for", "while", "switch", "case", "break", "continue", "include", "define", "namespace", "using", "new", "delete", "true", "false", "nullptr", "vec2", "vec3", "vec4", "mat4", "uniform", "in", "out", "texture", "sampler2D", "gl_FragColor", "gl_Position", "normalize", "dot", "cross", "mix", "clamp", "smoothstep"],
        "lua" => &["local", "function", "end", "if", "then", "else", "elseif", "for", "do", "while", "return", "nil", "true", "false", "and", "or", "not", "pairs", "ipairs", "require", "print"],
        "gd" => &["func", "var", "const", "extends", "class_name", "signal", "export", "onready", "if", "elif", "else", "for", "while", "match", "return", "self", "null", "true", "false", "print", "Vector2", "Vector3", "_ready", "_process", "_physics_process", "get_node", "queue_free", "emit_signal"],
        "json" => &["true", "false", "null"],
        "sh" | "bash" | "zsh" => &["echo", "if", "then", "else", "fi", "for", "do", "done", "while", "case", "esac", "function", "export", "local", "return", "cd", "ls", "mkdir", "grep", "sed", "awk"],
        _ => &[],
    }
}

/// Abréviations Emmet simples : div.carte#id, ul>li, a, ! (squelette HTML).
fn emmet(abbr: &str, indent: &str, unit: &str) -> Option<(String, usize)> {
    if abbr == "!" {
        let s = format!("<!DOCTYPE html>\n{indent}<html lang=\"fr\">\n{indent}<head>\n{indent}{unit}<meta charset=\"UTF-8\">\n{indent}{unit}<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n{indent}{unit}<title>Document</title>\n{indent}</head>\n{indent}<body>\n{indent}{unit}");
        let cur = s.chars().count();
        return Some((format!("{s}\n{indent}</body>\n{indent}</html>"), cur));
    }
    let re = regex::Regex::new(r"^([a-zA-Z][a-zA-Z0-9]*)?((?:[.#][\w-]+)*)(?:>([a-zA-Z][a-zA-Z0-9]*))?(?:\*(\d{1,2}))?$").ok()?;
    let c = re.captures(abbr)?;
    let tag = c.get(1).map(|m| m.as_str()).unwrap_or("div");
    let mods = c.get(2).map(|m| m.as_str()).unwrap_or("");
    let child = c.get(3).map(|m| m.as_str());
    let times: usize = c.get(4).and_then(|m| m.as_str().parse().ok()).unwrap_or(1);
    // Un mot seul qui n'est pas une balise connue : on laisse la suggestion normale.
    if c.get(1).is_some() && mods.is_empty() && child.is_none() && !keywords("html").contains(&tag) {
        return None;
    }
    let mut classes = vec![];
    let mut id = None;
    for m in regex::Regex::new(r"[.#][\w-]+").ok()?.find_iter(mods) {
        let s = m.as_str();
        if let Some(c) = s.strip_prefix('.') {
            classes.push(c.to_string());
        } else {
            id = Some(s[1..].to_string());
        }
    }
    let mut attrs = String::new();
    if let Some(i) = id {
        attrs += &format!(" id=\"{i}\"");
    }
    if !classes.is_empty() {
        attrs += &format!(" class=\"{}\"", classes.join(" "));
    }
    attrs += match tag {
        "a" => " href=\"\"",
        "img" => " src=\"\" alt=\"\"",
        "link" => " rel=\"stylesheet\" href=\"\"",
        "script" => " src=\"\"",
        "input" => " type=\"text\"",
        _ => "",
    };
    let one = |t: &str, a: &str| -> String {
        if VOID.contains(&t) { format!("<{t}{a}>") } else { format!("<{t}{a}></{t}>") }
    };
    let out = match child {
        None => (0..times).map(|_| one(tag, &attrs)).collect::<Vec<_>>().join(&format!("\n{indent}")),
        Some(ch) => {
            let kids: Vec<String> = (0..times).map(|_| format!("{indent}{unit}{}", one(ch, ""))).collect();
            format!("<{tag}{attrs}>\n{}\n{indent}</{tag}>", kids.join("\n"))
        }
    };
    // Curseur : dans le premier attribut vide, sinon entre les balises.
    let cur = if let Some(i) = out.find("=\"\"") {
        char_of(&out, i + 2)
    } else if let Some(i) = out.find("></") {
        char_of(&out, i + 1)
    } else {
        out.chars().count()
    };
    Some((out, cur))
}

fn pair_of(c: char) -> Option<char> {
    Some(match c {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '"' => '"',
        '\'' => '\'',
        '`' => '`',
        _ => return None,
    })
}

/// Lignes couvertes par la sélection [a, b) en octets.
fn line_span(text: &str, a: usize, b: usize) -> (usize, usize) {
    let s = line_start(text, a);
    let mut e_from = b;
    if b > a && b > 0 && text.as_bytes()[b - 1] == b'\n' {
        e_from = b - 1;
    }
    (s, line_end(text, e_from.max(s)))
}

struct Ctx<'a> {
    text: &'a mut String,
    a: usize,
    b: usize,
    changed: bool,
}

impl Ctx<'_> {
    /// Remplace [x, y) (octets) et place le curseur/sélection (octets, après remplacement).
    fn set(&mut self, x: usize, y: usize, ins: &str, sel: (usize, usize)) {
        self.text.replace_range(x..y, ins);
        self.a = sel.0;
        self.b = sel.1;
        self.changed = true;
    }
}

// ---------- avant le TextEdit : on intercepte les touches ----------

/// À appeler juste avant d'afficher le TextEdit `id`. Renvoie vrai si le texte a changé.
pub fn before(ui: &egui::Ui, ed: &mut Editor, text: &mut String, lang: &str, id: Id, notes: &[String]) -> bool {
    let ctx = ui.ctx().clone();
    let mut state = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
    let focused = ui.memory(|m| m.has_focus(id));
    let range = state.cursor.char_range();
    let (ca, cb) = match range {
        Some(r) => {
            let (x, y) = (r.primary.index.0, r.secondary.index.0);
            (x.min(y), x.max(y))
        }
        None => {
            let n = text.chars().count();
            (n, n)
        }
    };
    let mut c = Ctx { a: byte_of(text, ca), b: byte_of(text, cb), text, changed: false };
    let unit = indent_unit(c.text, lang);
    let mut moved = false;

    // Insertion depuis l'assistant
    if let Some(ins) = ed.insert.take() {
        let (a, b) = (c.a, c.b);
        let end = a + ins.len();
        c.set(a, b, &ins, (end, end));
        moved = true;
    }
    if let Some(i) = ed.click_accept.take() {
        if let Some(p) = ed.popup.take() {
            accept(&mut c, &p, i, lang);
            moved = true;
        }
    }
    if let Some(rx) = &ed.diag_rx {
        if let Ok(d) = rx.try_recv() {
            ed.diags = d;
            ed.diag_rx = None;
        }
    }
    if let Some(line) = ed.jump.take() {
        let pos = c.text.split_inclusive('\n').take(line).map(|l| l.len()).sum::<usize>().min(c.text.len());
        let pos = pos + c.text[pos..].len() - c.text[pos..].trim_start_matches([' ', '\t']).len();
        c.a = pos;
        c.b = pos;
        moved = true;
        ed.scroll_to = Some((pos, pos));
        ed.multi.clear();
        ui.ctx().memory_mut(|m| m.request_focus(id));
    }
    // Plusieurs curseurs : chaque frappe s'applique partout
    if focused && !ed.multi.is_empty() {
        let events = ui.input(|i| i.events.clone());
        let mut eat = vec![false; events.len()];
        for (k, ev) in events.iter().enumerate() {
            let op = match ev {
                Event::Text(t) => Some(MultiOp::Insert(t.clone())),
                Event::Paste(t) => Some(MultiOp::Insert(t.clone())),
                Event::Key { key: Key::Backspace, pressed: true, .. } => Some(MultiOp::Back),
                Event::Key { key: Key::Delete, pressed: true, .. } => Some(MultiOp::Del),
                Event::Key { key: Key::Enter, pressed: true, .. } => Some(MultiOp::Insert("\n".into())),
                Event::Key { key: Key::ArrowLeft, pressed: true, modifiers: m, .. } if !m.shift && !m.command => Some(MultiOp::Left),
                Event::Key { key: Key::ArrowRight, pressed: true, modifiers: m, .. } if !m.shift && !m.command => Some(MultiOp::Right),
                Event::Key { key: Key::Escape, pressed: true, .. } => {
                    ed.multi.clear();
                    eat[k] = true;
                    None
                }
                Event::Key { key: Key::D, pressed: true, modifiers: m, .. } if m.command && !m.shift => None,
                Event::Key { pressed: true, modifiers: m, key, .. } if !(m.command && matches!(key, Key::C | Key::Z | Key::S)) => {
                    // Autre touche (flèches haut/bas, Origine…) : retour à un seul curseur
                    ed.multi.clear();
                    None
                }
                _ => None,
            };
            if let Some(op) = op {
                multi_apply(&mut c, &mut ed.multi, &op);
                eat[k] = true;
                moved = true;
            }
            if ed.multi.is_empty() {
                break;
            }
        }
        if eat.iter().any(|e| *e) {
            ui.ctx().input_mut(|i| {
                let mut k = 0;
                i.events.retain(|_| {
                    let keep = !eat.get(k).copied().unwrap_or(false);
                    k += 1;
                    keep
                });
            });
        }
    }

    if focused {
        let events = ui.input(|i| i.events.clone());
        let mut eat = vec![false; events.len()];
        for (k, ev) in events.iter().enumerate() {
            // Garder l'ordre des frappes : dès qu'un événement n'est pas pour nous,
            // on laisse tout le reste au TextEdit.
            let relevant = matches!(ev, Event::Key { pressed: true, .. } | Event::Text(_) | Event::Paste(_) | Event::Cut | Event::Copy);
            match ev {
                Event::Key { key, pressed: true, modifiers: m, .. } => {
                    let key = *key;
                    let cmd = m.command || m.ctrl;
                    // Fenêtre de suggestions
                    if let Some(p) = &mut ed.popup {
                        match key {
                            Key::ArrowDown if !cmd => {
                                p.sel = (p.sel + 1) % p.items.len();
                                eat[k] = true;
                                continue;
                            }
                            Key::ArrowUp if !cmd => {
                                p.sel = (p.sel + p.items.len() - 1) % p.items.len();
                                eat[k] = true;
                                continue;
                            }
                            Key::Enter | Key::Tab if !cmd && !m.shift => {
                                let p = ed.popup.take().unwrap();
                                let sel = p.sel;
                                accept(&mut c, &p, sel, lang);
                                moved = true;
                                eat[k] = true;
                                continue;
                            }
                            Key::Escape => {
                                ed.popup = None;
                                eat[k] = true;
                                continue;
                            }
                            _ => {}
                        }
                    }
                    match key {
                        Key::Space if cmd => {
                            ed.force_popup = true;
                            eat[k] = true;
                        }
                        Key::F if cmd => {
                            let f = ed.find.get_or_insert_with(Find::default);
                            if c.a != c.b && !c.text[c.a..c.b].contains('\n') {
                                f.query = c.text[c.a..c.b].to_string();
                            }
                            f.focus = true;
                            f.show_replace = m.shift && f.show_replace;
                            eat[k] = true;
                        }
                        Key::H if cmd => {
                            let f = ed.find.get_or_insert_with(Find::default);
                            if c.a != c.b && !c.text[c.a..c.b].contains('\n') {
                                f.query = c.text[c.a..c.b].to_string();
                            }
                            f.show_replace = true;
                            f.focus = true;
                            eat[k] = true;
                        }
                        Key::F3 => {
                            if let Some(f) = &ed.find {
                                if let Some((x, y)) = find_from(c.text, f, if m.shift { c.a } else { c.b }, m.shift) {
                                    c.a = x;
                                    c.b = y;
                                    moved = true;
                                    ed.scroll_to = Some((x, y));
                                }
                            }
                            eat[k] = true;
                        }
                        Key::Escape if ed.find.is_some() => {
                            ed.find = None;
                            eat[k] = true;
                        }
                        Key::Slash if cmd => {
                            toggle_comment(&mut c, lang);
                            eat[k] = true;
                        }
                        // Ctrl+D : sélectionne le mot, puis l'occurrence suivante (curseurs multiples)
                        Key::D if cmd && !m.shift => {
                            select_next(&mut c, &mut ed.multi);
                            ed.scroll_to = Some((c.a, c.b));
                            eat[k] = true;
                        }
                        // Ctrl+Maj+L : toutes les occurrences
                        Key::L if cmd && m.shift => {
                            select_all_occurrences(&mut c, &mut ed.multi);
                            eat[k] = true;
                        }
                        // Ctrl+L : sélectionner la ligne (et les suivantes)
                        Key::L if cmd => {
                            let s0 = line_start(c.text, c.a);
                            let e0 = line_end(c.text, c.b.max(c.a));
                            let e0 = if c.a != c.b && c.text[..c.b].ends_with('\n') { line_end(c.text, c.b) } else { e0 };
                            let e1 = (e0 + 1).min(c.text.len());
                            c.a = s0;
                            c.b = e1;
                            eat[k] = true;
                        }
                        // Dupliquer la ligne : Ctrl+Maj+D ou Alt+Maj+↓/↑
                        Key::D if cmd && m.shift => {
                            duplicate(&mut c, true);
                            eat[k] = true;
                        }
                        Key::ArrowUp | Key::ArrowDown if m.alt && m.shift => {
                            duplicate(&mut c, key == Key::ArrowDown);
                            eat[k] = true;
                        }
                        // Ctrl+Entrée : nouvelle ligne dessous ; Ctrl+Maj+Entrée : dessus
                        Key::Enter if cmd => {
                            let ls = line_start(c.text, c.a);
                            let ind = indent_of(&c.text[ls..line_end(c.text, ls)]).to_string();
                            if m.shift {
                                let ins = format!("{ind}\n");
                                let pos = ls + ind.len();
                                c.set(ls, ls, &ins, (pos, pos));
                            } else {
                                let le = line_end(c.text, c.b);
                                let ins = format!("\n{ind}");
                                let pos = le + ins.len();
                                c.set(le, le, &ins, (pos, pos));
                            }
                            eat[k] = true;
                        }
                        // F12 : aller à la définition (serveur LSP)
                        Key::F12 => {
                            if let Some(l) = &mut ed.lsp {
                                l.ask_definition(c.text, c.b);
                            }
                            eat[k] = true;
                        }
                        // Ctrl+Maj+[ / ] : replier, déplier le bloc
                        Key::OpenBracket if cmd && m.shift => {
                            let l = c.text[..c.b].matches('\n').count();
                            fold_at_line(ed, c.text, l, true);
                            eat[k] = true;
                        }
                        Key::CloseBracket if cmd && m.shift => {
                            let l = c.text[..c.b].matches('\n').count();
                            fold_at_line(ed, c.text, l, false);
                            eat[k] = true;
                        }
                        // Ctrl+] / Ctrl+[ : indenter, désindenter
                        Key::CloseBracket if cmd => {
                            let (a, b) = (c.a, c.b);
                            let s0 = line_start(c.text, a);
                            c.a = s0;
                            indent(&mut c, &unit);
                            let _ = b;
                            eat[k] = true;
                        }
                        Key::OpenBracket if cmd => {
                            outdent(&mut c, &unit);
                            eat[k] = true;
                        }
                        // Origine intelligente : début du texte, puis début de ligne
                        Key::Home if !cmd && !m.alt => {
                            let ls = line_start(c.text, c.b);
                            let first = ls + indent_of(&c.text[ls..line_end(c.text, ls)]).len();
                            let target = if c.b == first { ls } else { first };
                            if m.shift {
                                c.b = target;
                                let (x, y) = (c.a.min(c.b), c.a.max(c.b));
                                // garde l'ancre à l'autre bout
                                c.a = if target < c.a { y } else { x };
                                c.b = target;
                            } else {
                                c.a = target;
                                c.b = target;
                            }
                            eat[k] = true;
                        }
                        Key::Z if m.alt => {
                            ed.wrap = Some(!ed.wrap.unwrap_or(false));
                            eat[k] = true;
                        }
                        Key::K if cmd && m.shift => {
                            let (s, e) = line_span(c.text, c.a, c.b);
                            let e2 = (e + 1).min(c.text.len());
                            let s2 = if e2 == e && s > 0 { s - 1 } else { s };
                            c.set(s2, e2, "", (s2.min(c.text.len()), s2.min(c.text.len())));
                            let p = line_start(c.text, c.a.min(c.text.len()));
                            c.a = p;
                            c.b = p;
                            eat[k] = true;
                        }
                        Key::ArrowUp | Key::ArrowDown if m.alt && !m.shift => {
                            move_lines(&mut c, key == Key::ArrowUp);
                            eat[k] = true;
                        }
                        Key::Backspace if !cmd && !m.alt && c.a == c.b && c.a > 0 => {
                            let prev = c.text[..c.a].chars().next_back();
                            let next = c.text[c.a..].chars().next();
                            if let (Some(p), Some(n)) = (prev, next) {
                                if pair_of(p) == Some(n) {
                                    let x = c.a - p.len_utf8();
                                    c.set(x, c.a + n.len_utf8(), "", (x, x));
                                    eat[k] = true;
                                    continue;
                                }
                            }
                            // Effacer une indentation complète d'un coup
                            let ls = line_start(c.text, c.a);
                            let before = &c.text[ls..c.a];
                            if unit != "\t" && !before.is_empty() && before.chars().all(|ch| ch == ' ') {
                                let n = before.len() % unit.len();
                                let n = if n == 0 { unit.len() } else { n };
                                let x = c.a - n;
                                c.set(x, c.a, "", (x, x));
                                eat[k] = true;
                            }
                        }
                        Key::Enter if !cmd && !m.alt => {
                            enter(&mut c, lang, &unit);
                            ed.popup = None;
                            eat[k] = true;
                        }
                        Key::Tab if !cmd && !m.alt => {
                            if m.shift {
                                outdent(&mut c, &unit);
                            } else if c.a != c.b && c.text[c.a..c.b].contains('\n') {
                                indent(&mut c, &unit);
                            } else if !(html_like(lang) || lang == "md") || !try_emmet(&mut c, &unit) {
                                let (a, b) = (c.a, c.b);
                                let e = a + unit.len();
                                c.set(a, b, &unit, (e, e));
                            }
                            eat[k] = true;
                        }
                        _ => {}
                    }
                }
                Event::Text(s) if s.chars().count() == 1 && !ui.input(|i| i.modifiers.command) => {
                    let ch = s.chars().next().unwrap();
                    if typed(&mut c, ch, lang) {
                        eat[k] = true;
                    }
                }
                _ => {}
            }
            if eat[k] {
                moved = true;
            } else if relevant {
                break;
            }
        }
        if eat.iter().any(|e| *e) {
            ui.ctx().input_mut(|i| {
                let mut k = 0;
                i.events.retain(|_| {
                    let keep = !eat.get(k).copied().unwrap_or(false);
                    k += 1;
                    keep
                });
            });
        }
    }
    let _ = notes;
    if moved || c.changed {
        let (a, b) = (char_of(c.text, c.a), char_of(c.text, c.b));
        state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
        egui::TextEdit::store_state(&ctx, id, state);
    }
    c.changed
}

fn accept(c: &mut Ctx, p: &Popup, i: usize, lang: &str) {
    let Some((word, kind)) = p.items.get(i) else { return };
    let x = byte_of(c.text, p.start);
    let y = byte_of(c.text, p.cursor).max(x);
    let mut ins = word.clone();
    let mut back = 0;
    if kind == &"note" {
        // [[note]] : on ferme les crochets s'ils ne le sont pas déjà
        if !c.text[y..].starts_with("]]") {
            ins.push_str("]]");
        } else {
            back = 0;
        }
    } else if kind == &"extrait" {
        if let Some((_, _, body)) = snippets(lang).iter().find(|s| s.0 == word) {
            let ls = line_start(c.text, x);
            let ind = indent_of(&c.text[ls..x]).to_string();
            let body = body.replace('\n', &format!("\n{ind}"));
            let cur = body.find("$0").unwrap_or(body.len());
            let body = body.replace("$0", "");
            let pos = x + cur;
            c.set(x, y, &body, (pos, pos));
            return;
        }
    } else if kind == &"balise" && html_like(lang) && c.text[..x].ends_with('<') {
        ins = if VOID.contains(&word.as_str()) { format!("{word}>") } else { format!("{word}></{word}>") };
        back = if VOID.contains(&word.as_str()) { 0 } else { word.len() + 3 };
    }
    let end = x + ins.len() - back;
    c.set(x, y, &ins, (end, end));
}

fn typed(c: &mut Ctx, ch: char, lang: &str) -> bool {
    let next = c.text[c.b..].chars().next();
    let prev = c.text[..c.a].chars().next_back();
    // Envelopper la sélection
    if c.a != c.b {
        if let Some(close) = pair_of(ch) {
            let inner = c.text[c.a..c.b].to_string();
            let (a, b) = (c.a, c.b);
            c.set(a, b, &format!("{ch}{inner}{close}"), (a + 1, a + 1 + inner.len()));
            return true;
        }
        return false;
    }
    // Sauter par-dessus un fermant déjà présent
    if matches!(ch, ')' | ']' | '}' | '"' | '\'' | '`') && next == Some(ch) {
        c.a += 1;
        c.b = c.a;
        return true;
    }
    if let Some(close) = pair_of(ch) {
        let is_quote = matches!(ch, '"' | '\'' | '`');
        // Pas de paire pour l'apostrophe dans du texte (« l'image »), ni collé devant un mot.
        if is_quote && (prev.is_some_and(is_word) || (ch == '\'' && matches!(lang, "md" | "txt" | ""))) {
            return false;
        }
        if next.is_some_and(is_word) {
            return false;
        }
        if lang == "md" && ch == '`' {
            // `code` en markdown : OK
        }
        let a = c.a;
        c.set(a, a, &format!("{ch}{close}"), (a + 1, a + 1));
        return true;
    }
    // Fermeture automatique des balises
    if ch == '>' && (html_like(lang) || lang == "md") {
        let ls = line_start(c.text, c.a);
        let before = &c.text[ls..c.a];
        if let Some(lt) = before.rfind('<') {
            let tag_txt = &before[lt + 1..];
            if !tag_txt.contains('>') && !tag_txt.starts_with('/') && !tag_txt.starts_with('!') && !tag_txt.starts_with('?') && !tag_txt.ends_with('/') {
                let name: String = tag_txt.chars().take_while(|c| c.is_alphanumeric() || *c == '-' || *c == ':').collect();
                // Les attributs entre guillemets ne doivent pas être ouverts
                let quotes = tag_txt.matches('"').count() % 2 == 0;
                if !name.is_empty() && name.chars().next().unwrap().is_alphabetic() && quotes && !VOID.contains(&name.to_lowercase().as_str()) {
                    let a = c.a;
                    c.set(a, a, &format!("></{name}>"), (a + 1, a + 1));
                    return true;
                }
            }
        }
    }
    // « </ » : compléter la dernière balise ouverte
    if ch == '/' && prev == Some('<') && html_like(lang) {
        if let Some(open) = last_open_tag(&c.text[..c.a - 1]) {
            let a = c.a;
            let ins = format!("/{open}>");
            let e = a + ins.len();
            c.set(a, a, &ins, (e, e));
            return true;
        }
    }
    false
}

fn last_open_tag(s: &str) -> Option<String> {
    let re = regex::Regex::new(r"<(/?)([a-zA-Z][\w:-]*)[^>]*?(/?)>").ok()?;
    let mut stack: Vec<String> = vec![];
    for c in re.captures_iter(s) {
        let name = c[2].to_string();
        if &c[3] == "/" || VOID.contains(&name.to_lowercase().as_str()) {
            continue;
        }
        if &c[1] == "/" {
            if let Some(i) = stack.iter().rposition(|n| *n == name) {
                stack.truncate(i);
            }
        } else {
            stack.push(name);
        }
    }
    stack.pop()
}

fn enter(c: &mut Ctx, lang: &str, unit: &str) {
    let (a, b) = (c.a, c.b);
    let ls = line_start(c.text, a);
    let line = &c.text[ls..a];
    let ind = indent_of(line).to_string();
    let prev = c.text[..a].trim_end_matches([' ', '\t']).chars().next_back();
    let next_txt = &c.text[b..];
    let next = next_txt.chars().next();

    // Listes Markdown
    if lang == "md" {
        let t = line.trim_start();
        let re = regex::Regex::new(r"^(- \[[ xX]\] |[-*+] |(\d+)\. |> )").unwrap();
        if let Some(m) = re.captures(t) {
            let marker = m.get(1).unwrap().as_str();
            if t.len() == marker.len() {
                // Ligne vide de liste : on sort de la liste
                c.set(ls, b, &ind, (ls + ind.len(), ls + ind.len()));
                return;
            }
            let new = match m.get(2) {
                Some(n) => format!("{}. ", n.as_str().parse::<u64>().unwrap_or(0) + 1),
                None if marker.starts_with("- [") => "- [ ] ".to_string(),
                None => marker.to_string(),
            };
            let ins = format!("\n{ind}{new}");
            let e = a + ins.len();
            c.set(a, b, &ins, (e, e));
            return;
        }
    }

    let opens = matches!(prev, Some('{' | '[' | '('));
    let colon = prev == Some(':') && matches!(lang, "py" | "gd" | "yaml" | "yml" | "nim");
    let between_tags = html_like(lang) && prev == Some('>') && next_txt.starts_with("</") && {
        let before = &c.text[ls..a];
        before.rfind('<').is_some_and(|i| !before[i..].starts_with("</"))
    };
    let closes_next = matches!((prev, next), (Some('{'), Some('}')) | (Some('['), Some(']')) | (Some('('), Some(')')));
    if closes_next || between_tags {
        let first = format!("\n{ind}{unit}");
        let ins = format!("{first}\n{ind}");
        let e = a + first.len();
        c.set(a, b, &ins, (e, e));
    } else if opens || colon || (html_like(lang) && prev == Some('>') && opening_tag_before(&c.text[ls..a])) {
        let ins = format!("\n{ind}{unit}");
        let e = a + ins.len();
        c.set(a, b, &ins, (e, e));
    } else {
        let ins = format!("\n{ind}");
        let e = a + ins.len();
        // On ne laisse pas d'espaces en fin de ligne
        let trim_from = ls + c.text[ls..a].trim_end_matches([' ', '\t']).len();
        let trim_from = if c.text[ls..a].trim().is_empty() { a } else { trim_from };
        let e = e - (a - trim_from);
        c.set(trim_from, b, &ins, (e, e));
    }
}

fn opening_tag_before(line: &str) -> bool {
    let Some(i) = line.rfind('<') else { return false };
    let t = &line[i + 1..];
    !t.starts_with('/') && !t.starts_with('!') && !t.ends_with("/>") && {
        let name: String = t.chars().take_while(|c| c.is_alphanumeric()).collect();
        !name.is_empty() && !VOID.contains(&name.to_lowercase().as_str())
    }
}

fn indent(c: &mut Ctx, unit: &str) {
    let (s, e) = line_span(c.text, c.a, c.b);
    let block = &c.text[s..e];
    let n = block.lines().count().max(1);
    let new: String = block.split('\n').map(|l| if l.is_empty() { l.to_string() } else { format!("{unit}{l}") }).collect::<Vec<_>>().join("\n");
    let grow = new.len() - block.len();
    let (a, b) = (c.a + unit.len(), c.b + grow);
    let _ = n;
    c.set(s, e, &new, (a, b));
}

fn outdent(c: &mut Ctx, unit: &str) {
    let (s, e) = line_span(c.text, c.a, c.b);
    let block = c.text[s..e].to_string();
    let mut first = 0;
    let new: Vec<String> = block
        .split('\n')
        .enumerate()
        .map(|(k, l)| {
            let cut = if l.starts_with('\t') {
                1
            } else {
                (l.len() - l.trim_start_matches(' ').len()).min(unit.len().max(1))
            };
            if k == 0 {
                first = cut;
            }
            l[cut..].to_string()
        })
        .collect();
    let new = new.join("\n");
    let shrink = block.len() - new.len();
    let a = c.a.saturating_sub(first).max(s);
    let b = c.b.saturating_sub(shrink).max(a);
    c.set(s, e, &new, (a, b));
}

fn toggle_comment(c: &mut Ctx, lang: &str) {
    let (open, close) = comment(lang);
    let (s, e) = line_span(c.text, c.a, c.b);
    let block = c.text[s..e].to_string();
    let lines: Vec<&str> = block.split('\n').collect();
    let o = open.trim_end();
    let cl = close.trim_start();
    let all = lines.iter().filter(|l| !l.trim().is_empty()).all(|l| l.trim_start().starts_with(o));
    let min_ind = lines.iter().filter(|l| !l.trim().is_empty()).map(|l| indent_of(l).len()).min().unwrap_or(0);
    let new: Vec<String> = lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                return l.to_string();
            }
            if all {
                let ind = indent_of(l);
                let mut rest = l.trim_start().strip_prefix(o).unwrap_or(l.trim_start());
                rest = rest.strip_prefix(' ').unwrap_or(rest);
                if !cl.is_empty() {
                    rest = rest.trim_end().strip_suffix(cl).unwrap_or(rest).trim_end();
                }
                format!("{ind}{rest}")
            } else {
                format!("{}{open}{}{close}", &l[..min_ind], &l[min_ind..])
            }
        })
        .collect();
    let new = new.join("\n");
    let end = s + new.len();
    if c.a == c.b {
        let delta = new.len() as isize - block.len() as isize;
        let a = (c.a as isize + delta).clamp(s as isize, end as isize) as usize;
        let a = if c.text.is_char_boundary(a) { a } else { end };
        c.set(s, e, &new, (a.min(end), a.min(end)));
        // Corrige une éventuelle position hors caractère
        while !c.text.is_char_boundary(c.a) {
            c.a -= 1;
        }
        c.b = c.a;
    } else {
        c.set(s, e, &new, (s, end));
    }
}

fn move_lines(c: &mut Ctx, up: bool) {
    let (s, e) = line_span(c.text, c.a, c.b);
    let block = c.text[s..e].to_string();
    if up {
        if s == 0 {
            return;
        }
        let ps = line_start(c.text, s - 1);
        let prev = c.text[ps..s - 1].to_string();
        let new = format!("{block}\n{prev}");
        let d = s - ps;
        let (a, b) = (c.a - d, c.b - d);
        c.set(ps, e, &new, (a, b));
    } else {
        if e >= c.text.len() {
            return;
        }
        let ne = line_end(c.text, e + 1);
        let next = c.text[e + 1..ne].to_string();
        let new = format!("{next}\n{block}");
        let d = next.len() + 1;
        let (a, b) = (c.a + d, c.b + d);
        c.set(s, ne, &new, (a, b));
    }
}

fn try_emmet(c: &mut Ctx, unit: &str) -> bool {
    if c.a != c.b {
        return false;
    }
    let ls = line_start(c.text, c.a);
    let before = &c.text[ls..c.a];
    let ind = indent_of(before).to_string();
    let ws = before.rfind(|ch: char| ch.is_whitespace() || ch == '"').map(|i| i + 1).unwrap_or(0);
    let gt = before.rfind('>').map(|i| i + 1).unwrap_or(0).max(ws);
    let mut found = None;
    for start in [ws, gt] {
        let abbr = &before[start..];
        if !abbr.is_empty() {
            if let Some(r) = emmet(abbr, &ind, unit) {
                found = Some((start, r));
                break;
            }
        }
    }
    let Some((abbr_start, (out, cur))) = found else { return false };
    let x = ls + abbr_start;
    let a = c.a;
    let cur_b = x + byte_of(&out, cur);
    c.set(x, a, &out, (cur_b, cur_b));
    true
}


// ---------- curseurs multiples ----------

enum MultiOp {
    Insert(String),
    Back,
    Del,
    Left,
    Right,
}

fn prev_char(s: &str, b: usize) -> usize {
    s[..b].chars().next_back().map(|c| b - c.len_utf8()).unwrap_or(0)
}
fn next_char(s: &str, b: usize) -> usize {
    s[b..].chars().next().map(|c| b + c.len_utf8()).unwrap_or(b)
}

/// Applique la même modification au curseur principal et à tous les autres.
fn multi_apply(c: &mut Ctx, multi: &mut Vec<(usize, usize)>, op: &MultiOp) {
    let mut ranges: Vec<(usize, usize, bool)> = multi.iter().map(|&(a, b)| (a.min(b), a.max(b), false)).collect();
    ranges.push((c.a.min(c.b), c.a.max(c.b), true));
    ranges.sort_by_key(|r| r.0);
    ranges.dedup_by_key(|r| r.0);
    let text = c.text.clone();
    let mut out = String::with_capacity(text.len() + 64);
    let mut last = 0;
    let mut news: Vec<(usize, bool)> = vec![];
    for (a, b, main) in ranges {
        if a < last {
            continue;
        }
        match op {
            MultiOp::Left | MultiOp::Right => {
                out.push_str(&text[last..a.max(last)]);
                last = a.max(last);
                let pos = if a != b {
                    if matches!(op, MultiOp::Left) { a } else { b }
                } else if matches!(op, MultiOp::Left) {
                    prev_char(&text, a)
                } else {
                    next_char(&text, a)
                };
                news.push((pos, main));
                continue;
            }
            _ => {}
        }
        let (mut x, mut y) = (a, b);
        if a == b {
            match op {
                MultiOp::Back => x = prev_char(&text, a).max(last),
                MultiOp::Del => y = next_char(&text, b),
                _ => {}
            }
        }
        out.push_str(&text[last..x]);
        if let MultiOp::Insert(t) = op {
            if t == "\n" {
                let ls = text[..x].rfind('\n').map(|i| i + 1).unwrap_or(0);
                out.push('\n');
                out.push_str(indent_of(&text[ls..]));
            } else {
                out.push_str(t);
            }
        }
        news.push((out.len(), main));
        last = y;
    }
    if !matches!(op, MultiOp::Left | MultiOp::Right) {
        out.push_str(&text[last..]);
        *c.text = out;
        c.changed = true;
    }
    multi.clear();
    for (pos, main) in news {
        if main {
            c.a = pos;
            c.b = pos;
        } else {
            multi.push((pos, pos));
        }
    }
}

fn word_at(text: &str, b: usize) -> (usize, usize) {
    let s = text[..b].char_indices().rev().take_while(|(_, c)| is_word(*c)).last().map(|(i, _)| i).unwrap_or(b);
    let e = text[b..].char_indices().take_while(|(_, c)| is_word(*c)).last().map(|(i, c)| b + i + c.len_utf8()).unwrap_or(b);
    (s, e)
}

/// Ctrl+D : la première fois, sélectionne le mot ; ensuite ajoute l'occurrence suivante.
fn select_next(c: &mut Ctx, multi: &mut Vec<(usize, usize)>) {
    if c.a == c.b {
        let (s, e) = word_at(c.text, c.a);
        if s < e {
            c.a = s;
            c.b = e;
        }
        return;
    }
    let needle = c.text[c.a..c.b].to_string();
    let after = multi.iter().map(|r| r.1).chain([c.b]).max().unwrap_or(c.b);
    let found = c.text[after..].find(&needle).map(|i| after + i).or_else(|| c.text.find(&needle));
    if let Some(i) = found {
        let r = (i, i + needle.len());
        if r != (c.a, c.b) && !multi.contains(&r) {
            multi.push((c.a, c.b));
            c.a = r.0;
            c.b = r.1;
        }
    }
}

fn select_all_occurrences(c: &mut Ctx, multi: &mut Vec<(usize, usize)>) {
    if c.a == c.b {
        let (s, e) = word_at(c.text, c.a);
        if s >= e {
            return;
        }
        c.a = s;
        c.b = e;
    }
    let needle = c.text[c.a..c.b].to_string();
    multi.clear();
    let main = (c.a, c.b);
    for (i, _) in c.text.match_indices(&needle).take(2000) {
        if (i, i + needle.len()) != main {
            multi.push((i, i + needle.len()));
        }
    }
}

fn duplicate(c: &mut Ctx, down: bool) {
    let (s, e) = line_span(c.text, c.a, c.b);
    let block = c.text[s..e].to_string();
    let off = block.len() + 1;
    if down {
        let (a, b) = (c.a + off, c.b + off);
        c.set(e, e, &format!("\n{block}"), (a, b));
    } else {
        let (a, b) = (c.a, c.b);
        c.set(s, s, &format!("{block}\n"), (a, b));
    }
}

// ---------- structure du fichier ----------

#[derive(Clone)]
pub struct Symbol {
    pub line: usize,
    pub depth: usize,
    pub name: String,
    pub kind: &'static str,
}

/// Fonctions, classes, titres… pour le panneau « structure » et Ctrl+Maj+O.
pub fn symbols(text: &str, lang: &str) -> Vec<Symbol> {
    use std::sync::OnceLock;
    static RES: OnceLock<HashMap<&'static str, Vec<(regex::Regex, &'static str)>>> = OnceLock::new();
    let res = RES.get_or_init(|| {
        let r = |p: &str| regex::Regex::new(p).unwrap();
        let mut m: HashMap<&'static str, Vec<(regex::Regex, &'static str)>> = HashMap::new();
        m.insert("rs", vec![(r(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+(\w+)"), "fn"), (r(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|trait|union)\s+(\w+)"), "type"), (r(r"^\s*impl(?:<[^>]*>)?\s+([^{]+?)\s*\{"), "impl"), (r(r"^\s*(?:pub\s+)?mod\s+(\w+)"), "mod")]);
        m.insert("py", vec![(r(r"^\s*(?:async\s+)?def\s+(\w+)"), "fn"), (r(r"^\s*class\s+(\w+)"), "type")]);
        let js = vec![
            (r(r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\*?\s+(\w+)"), "fn"),
            (r(r"^\s*(?:export\s+)?(?:default\s+)?class\s+(\w+)"), "type"),
            (r(r"^\s*(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s*)?(?:\([^)]*\)|\w+)\s*=>"), "fn"),
            (r(r"^\s*(?:export\s+)?(?:interface|type|enum)\s+(\w+)"), "type"),
            (r(r"^\s+(?:async\s+|static\s+|get\s+|set\s+)*(\w+)\s*\([^)]*\)\s*\{\s*$"), "méthode"),
        ];
        for l in ["js", "ts", "jsx", "tsx", "mjs", "vue", "svelte"] {
            m.insert(l, js.clone());
        }
        let c = vec![(r(r"^\s*(?:[\w:<>,\*&\[\]]+\s+)+\*?(\w+)\s*\([^;]*\)\s*(?:const\s*)?\{?\s*$"), "fn"), (r(r"^\s*(?:struct|class|enum|union)\s+(\w+)"), "type")];
        for l in ["c", "h", "cpp", "hpp", "cc", "cs", "java", "glsl", "hlsl", "frag", "vert", "osl"] {
            m.insert(l, c.clone());
        }
        m.insert("lua", vec![(r(r"^\s*(?:local\s+)?function\s+([\w.:]+)"), "fn")]);
        m.insert("gd", vec![(r(r"^\s*(?:static\s+)?func\s+(\w+)"), "fn"), (r(r"^\s*class(?:_name)?\s+(\w+)"), "type"), (r(r"^\s*signal\s+(\w+)"), "signal")]);
        m.insert("sh", vec![(r(r"^\s*(?:function\s+)?(\w+)\s*\(\)\s*\{"), "fn")]);
        m.insert("go", vec![(r(r"^func\s+(?:\([^)]*\)\s*)?(\w+)"), "fn"), (r(r"^type\s+(\w+)"), "type")]);
        m.insert("css", vec![(r(r"^\s*([^{}@/][^{}]*?)\s*\{"), "règle"), (r(r"^\s*(@media[^{]*)\{"), "media")]);
        m
    });
    let mut out = vec![];
    if lang == "md" {
        let mut code = false;
        for (i, l) in text.lines().enumerate() {
            if l.trim_start().starts_with("```") {
                code = !code;
            }
            if code {
                continue;
            }
            let n = l.chars().take_while(|c| *c == '#').count();
            if n > 0 && n <= 6 && l[n..].starts_with(' ') {
                out.push(Symbol { line: i, depth: n - 1, name: l[n..].trim().to_string(), kind: "titre" });
            }
        }
        return out;
    }
    if matches!(lang, "html" | "htm" | "xml" | "svg") {
        let re = regex::Regex::new(r#"<(h[1-6]|section|header|footer|nav|main|article|script|style)\b[^>]*?(?:id="([^"]+)")?[^>]*>"#).unwrap();
        for (i, l) in text.lines().enumerate() {
            if let Some(c) = re.captures(l) {
                let name = c.get(2).map(|m| format!("{} #{}", &c[1], m.as_str())).unwrap_or_else(|| c[1].to_string());
                out.push(Symbol { line: i, depth: (l.len() - l.trim_start().len()) / 2, name, kind: "balise" });
            }
        }
        return out;
    }
    if matches!(lang, "json" | "yaml" | "yml" | "toml" | "ini" | "cfg") {
        for (i, l) in text.lines().enumerate() {
            let ind = l.len() - l.trim_start().len();
            let t = l.trim();
            if ind <= 2 {
                if let Some(k) = t.strip_prefix('"').and_then(|r| r.split('"').next()).filter(|_| lang == "json" && t.contains(':')) {
                    out.push(Symbol { line: i, depth: ind / 2, name: k.to_string(), kind: "clé" });
                } else if (lang == "yaml" || lang == "yml") && t.ends_with(':') && !t.starts_with('#') && !t.starts_with('-') {
                    out.push(Symbol { line: i, depth: ind / 2, name: t.trim_end_matches(':').to_string(), kind: "clé" });
                } else if t.starts_with('[') && t.ends_with(']') {
                    out.push(Symbol { line: i, depth: 0, name: t.to_string(), kind: "section" });
                }
            }
        }
        return out;
    }
    let Some(list) = res.get(lang) else { return out };
    const KW: [&str; 9] = ["if", "for", "while", "switch", "catch", "return", "else", "do", "sizeof"];
    for (i, l) in text.lines().enumerate().take(50_000) {
        for (re, kind) in list {
            if let Some(c) = re.captures(l) {
                let name = c[1].trim().to_string();
                if KW.contains(&name.as_str()) {
                    continue;
                }
                let ind = l.len() - l.trim_start().len();
                out.push(Symbol { line: i, depth: ind / 4, name, kind });
                break;
            }
        }
    }
    out
}

// ---------- vérification à l'enregistrement ----------

/// Vérifie la syntaxe avec l'outil du langage s'il est installé (Python, Node, Bash, Lua) ou en interne (JSON).
pub fn check(path: &std::path::Path, lang: &str, text: &str) -> Vec<Diag> {
    if lang == "json" {
        return match serde_json::from_str::<serde_json::Value>(text) {
            Ok(_) => vec![],
            Err(e) => vec![Diag { line: e.line().saturating_sub(1), msg: e.to_string() }],
        };
    }
    let py = if cfg!(windows) { "python" } else { "python3" };
    let (prog, args): (&str, Vec<String>) = match lang {
        "py" => (py, vec!["-c".into(), "import ast,sys\nsrc=open(sys.argv[1],encoding='utf-8').read()\ntry:\n ast.parse(src)\nexcept SyntaxError as e:\n print(f'{e.lineno}: {e.msg}')".into(), path.display().to_string()]),
        "js" | "mjs" | "cjs" => ("node", vec!["--check".into(), path.display().to_string()]),
        "sh" | "bash" => ("bash", vec!["-n".into(), path.display().to_string()]),
        "lua" => ("luac", vec!["-p".into(), path.display().to_string()]),
        _ => return vec![],
    };
    let mut cmd = std::process::Command::new(prog);
    cmd.args(&args).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let Ok(out) = cmd.output() else { return vec![] };
    let all = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let mut diags = vec![];
    let re_py = regex::Regex::new(r"^(\d+): (.+)$").unwrap();
    let re_line = regex::Regex::new(r"(?:line |:)(\d+)(?::\d+)?[:)]?\s*(.*)").unwrap();
    let lines: Vec<&str> = all.lines().collect();
    for (k, l) in lines.iter().enumerate() {
        if lang == "py" {
            if let Some(c) = re_py.captures(l) {
                diags.push(Diag { line: c[1].parse::<usize>().unwrap_or(1).saturating_sub(1), msg: c[2].to_string() });
            }
        } else if l.contains(&*path.file_name().unwrap_or_default().to_string_lossy()) || lang == "sh" {
            if let Some(c) = re_line.captures(l) {
                let mut msg = c[2].trim().to_string();
                // node : le message est quelques lignes plus bas
                if msg.is_empty() || lang.starts_with('j') || lang == "mjs" {
                    if let Some(m) = lines[k..].iter().find(|x| x.contains("Error")) {
                        msg = m.trim().to_string();
                    }
                }
                diags.push(Diag { line: c[1].parse::<usize>().unwrap_or(1).saturating_sub(1), msg });
            }
        }
        if diags.len() > 20 {
            break;
        }
    }
    diags.dedup_by_key(|d| d.line);
    diags
}

// ---------- formatage ----------

/// Formate tout le fichier avec l'outil habituel du langage (s'il est installé).
pub fn format(text: &str, lang: &str, file_name: &str) -> Result<String, String> {
    if lang == "json" {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("JSON invalide : {e}"))?;
        return serde_json::to_string_pretty(&v).map(|s| s + "\n").map_err(|e| e.to_string());
    }
    let tries: Vec<(&str, Vec<String>)> = match lang {
        "rs" => vec![("rustfmt", vec!["--edition".into(), "2021".into(), "--emit".into(), "stdout".into()])],
        "py" => vec![("ruff", vec!["format".into(), "-".into()]), ("black", vec!["-q".into(), "-".into()])],
        "js" | "ts" | "jsx" | "tsx" | "mjs" | "css" | "scss" | "less" | "html" | "htm" | "vue" | "svelte" | "md" | "yaml" | "yml" | "graphql" => vec![("prettier", vec!["--stdin-filepath".into(), file_name.into()])],
        "c" | "h" | "cpp" | "hpp" | "cc" | "cs" | "java" | "glsl" | "hlsl" | "frag" | "vert" => vec![("clang-format", vec![format!("--assume-filename={file_name}")])],
        "go" => vec![("gofmt", vec![])],
        "lua" => vec![("stylua", vec!["-".into()])],
        "gd" => vec![("gdformat", vec!["-".into()])],
        "sh" | "bash" => vec![("shfmt", vec![])],
        _ => vec![],
    };
    if tries.is_empty() {
        return Err("Pas de formateur connu pour ce langage.".into());
    }
    let mut missing = vec![];
    for (prog, args) in &tries {
        let exe = if cfg!(windows) && *prog == "prettier" { "prettier.cmd" } else { prog };
        let mut cmd = std::process::Command::new(exe);
        cmd.args(args).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let Ok(mut child) = cmd.spawn() else {
            missing.push(*prog);
            continue;
        };
        if let Some(mut si) = child.stdin.take() {
            use std::io::Write;
            let t = text.to_string();
            std::thread::spawn(move || {
                let _ = si.write_all(t.as_bytes());
            });
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        if out.status.success() && !out.stdout.is_empty() {
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
        return Err(format!("{prog} : {}", String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("erreur")));
    }
    Err(format!("Installe {} pour formater ce langage.", missing.join(" ou ")))
}

// ---------- extraits (snippets) ----------

/// (déclencheur, description, texte) ; « $0 » marque la place du curseur.
fn snippets(lang: &str) -> &'static [(&'static str, &'static str, &'static str)] {
    match lang {
        "py" => &[
            ("def", "fonction", "def $0():\n    pass"),
            ("class", "classe", "class $0:\n    def __init__(self):\n        pass"),
            ("for", "boucle", "for i in range($0):\n    "),
            ("ifmain", "point d'entrée", "if __name__ == \"__main__\":\n    $0"),
            ("with", "fichier", "with open($0) as f:\n    "),
            ("try", "try / except", "try:\n    $0\nexcept Exception as e:\n    print(e)"),
            ("bpyop", "opérateur Blender", "import bpy\n\nclass OBJECT_OT_$0(bpy.types.Operator):\n    bl_idname = \"object.mon_operateur\"\n    bl_label = \"Mon opérateur\"\n\n    def execute(self, context):\n        return {'FINISHED'}\n\ndef register():\n    bpy.utils.register_class(OBJECT_OT_)\n"),
        ],
        "rs" => &[
            ("fn", "fonction", "fn $0() {\n    \n}"),
            ("pfn", "fonction publique", "pub fn $0() {\n    \n}"),
            ("struct", "structure", "#[derive(Debug, Clone)]\nstruct $0 {\n    \n}"),
            ("impl", "implémentation", "impl $0 {\n    \n}"),
            ("for", "boucle", "for $0 in  {\n    \n}"),
            ("match", "match", "match $0 {\n    _ => {}\n}"),
            ("test", "test", "#[test]\nfn $0() {\n    \n}"),
        ],
        "js" | "ts" | "jsx" | "tsx" | "mjs" | "vue" | "svelte" => &[
            ("fn", "fonction", "function $0() {\n  \n}"),
            ("afn", "fonction fléchée", "const $0 = () => {\n  \n};"),
            ("for", "boucle", "for (let i = 0; i < $0; i++) {\n  \n}"),
            ("forof", "for…of", "for (const item of $0) {\n  \n}"),
            ("if", "si", "if ($0) {\n  \n}"),
            ("class", "classe", "class $0 {\n  constructor() {\n    \n  }\n}"),
            ("log", "console.log", "console.log($0);"),
            ("fetch", "requête", "const res = await fetch($0);\nconst data = await res.json();"),
        ],
        "c" | "cpp" | "h" | "hpp" | "cs" | "java" => &[("for", "boucle", "for (int i = 0; i < $0; i++) {\n    \n}"), ("if", "si", "if ($0) {\n    \n}"), ("main", "main", "int main() {\n    $0\n    return 0;\n}")],
        "glsl" | "frag" | "vert" | "hlsl" => &[("main", "main", "void main() {\n    $0\n}"), ("frag", "fragment shader", "#version 330 core\nin vec2 uv;\nout vec4 color;\nuniform float time;\n\nvoid main() {\n    color = vec4(uv, 0.5 + 0.5 * sin(time), 1.0);$0\n}")],
        "lua" => &[("fn", "fonction", "local function $0()\n  \nend"), ("for", "boucle", "for i = 1, $0 do\n  \nend")],
        "gd" => &[("func", "fonction", "func $0() -> void:\n\tpass"), ("ready", "_ready", "func _ready() -> void:\n\t$0")],
        "md" => &[("table", "tableau", "| $0 |  |\n|---|---|\n|  |  |"), ("todo", "case à cocher", "- [ ] $0"), ("code", "bloc de code", "```\n$0\n```")],
        "css" | "scss" => &[("flex", "flex centré", "display: flex;\nalign-items: center;\njustify-content: center;$0"), ("grid", "grille", "display: grid;\ngrid-template-columns: repeat(3, 1fr);\ngap: 16px;$0"), ("media", "media query", "@media (max-width: 768px) {\n  $0\n}")],
        _ => &[],
    }
}

// ---------- recherche ----------

fn find_all(text: &str, f: &Find) -> Vec<(usize, usize)> {
    if f.query.is_empty() {
        return vec![];
    }
    if f.case {
        text.match_indices(&f.query).map(|(i, m)| (i, i + m.len())).take(5000).collect()
    } else {
        let lower = text.to_lowercase();
        let q = f.query.to_lowercase();
        if lower.len() != text.len() {
            // Cas rare (caractères qui changent de taille) : on retombe sur la casse exacte.
            return text.match_indices(&f.query).map(|(i, m)| (i, i + m.len())).take(5000).collect();
        }
        lower.match_indices(&q).map(|(i, m)| (i, i + m.len())).take(5000).collect()
    }
}

fn find_from(text: &str, f: &Find, from: usize, back: bool) -> Option<(usize, usize)> {
    let all = find_all(text, f);
    if back {
        all.iter().rev().find(|m| m.1 <= from).or(all.last()).copied()
    } else {
        all.iter().find(|m| m.0 >= from).or(all.first()).copied()
    }
}

/// Barre « chercher / remplacer » au-dessus de l'éditeur. Renvoie vrai si le texte a changé.
pub fn find_bar(ui: &mut egui::Ui, ed: &mut Editor, text: &mut String, id: Id) -> bool {
    let p = theme::pal();
    let Some(f) = &mut ed.find else { return false };
    let mut changed = false;
    let mut close = false;
    let mut go: Option<bool> = None;
    let mut replace_one = false;
    let mut replace_all = false;
    let matches = find_all(text, f);
    egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut f.query).hint_text(crate::i18n::t("chercher")).desired_width(220.0));
            if f.focus {
                r.request_focus();
                f.focus = false;
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                go = Some(ui.input(|i| i.modifiers.shift));
                r.request_focus();
            }
            if r.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                close = true;
            }
            ui.toggle_value(&mut f.case, crate::i18n::t("Aa")).on_hover_text(crate::i18n::t("respecter la casse"));
            let count = if f.query.is_empty() { String::new() } else if matches.is_empty() { "aucun".into() } else { format!("{}/{}", (f.current + 1).min(matches.len()), matches.len()) };
            ui.label(RichText::new(count).color(if matches.is_empty() && !f.query.is_empty() { p.missing } else { p.muted }).size(11.5));
            if ui.small_button("↑").on_hover_text(crate::i18n::t("précédent (Maj+F3)")).clicked() {
                go = Some(true);
            }
            if ui.small_button("↓").on_hover_text(crate::i18n::t("suivant (F3, Entrée)")).clicked() {
                go = Some(false);
            }
            ui.toggle_value(&mut f.show_replace, crate::i18n::t("remplacer")).on_hover_text(crate::i18n::t("Ctrl+H"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("×").on_hover_text(crate::i18n::t("fermer (Échap)")).clicked() {
                    close = true;
                }
            });
        });
        if f.show_replace {
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut f.replace).hint_text(crate::i18n::t("remplacer par")).desired_width(220.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    replace_one = true;
                    r.request_focus();
                }
                replace_one |= ui.button(crate::i18n::t("remplacer")).clicked();
                replace_all |= ui.button(crate::i18n::t("tout remplacer")).clicked();
            });
        }
    });
    if close {
        ed.find = None;
        ui.memory_mut(|m| m.request_focus(id));
        return false;
    }
    let state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
    let cur = state.cursor.char_range().map(|r| {
        let (x, y) = (r.primary.index.0, r.secondary.index.0);
        (byte_of(text, x.min(y)), byte_of(text, x.max(y)))
    });
    if replace_all && !matches.is_empty() {
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for (a, b) in &matches {
            out.push_str(&text[last..*a]);
            out.push_str(&f.replace);
            last = *b;
        }
        out.push_str(&text[last..]);
        *text = out;
        changed = true;
    }
    if replace_one {
        // Remplace la sélection si c'est une occurrence, puis passe à la suivante.
        if let Some((a, b)) = cur {
            if matches.contains(&(a, b)) {
                text.replace_range(a..b, &f.replace);
                changed = true;
                let from = a + f.replace.len();
                if let Some(m) = find_from(text, f, from, false) {
                    set_sel(ui.ctx(), id, text, m);
                    ed.scroll_to = Some(m);
                }
                return changed;
            }
        }
        go = Some(false);
    }
    if let Some(back) = go {
        let from = cur.map(|(a, b)| if back { a } else { b }).unwrap_or(0);
        if let Some(m) = find_from(text, f, from, back) {
            f.current = matches.iter().position(|x| *x == m).unwrap_or(0);
            set_sel(ui.ctx(), id, text, m);
            ed.scroll_to = Some(m);
        }
    }
    changed
}

fn set_sel(ctx: &egui::Context, id: Id, text: &str, (a, b): (usize, usize)) {
    let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(char_of(text, a)), CCursor::new(char_of(text, b)))));
    egui::TextEdit::store_state(ctx, id, st);
}

// ---------- après le TextEdit : suggestions, surlignage ----------

pub struct View {
    pub minimap: bool,
    pub changes: bool,
    pub gutter: bool,
    pub font_size: f32,
    pub bg: egui::layers::ShapeIdx,
}

pub fn after(ui: &mut egui::Ui, ed: &mut Editor, text: &str, lang: &str, out: &egui::text_edit::TextEditOutput, notes: &[String], typed_now: bool, view: View) {
    let p = theme::pal();
    let origin = out.galley_pos;
    // ---------- gouttière, ligne courante, guides d'indentation, erreurs ----------
    {
        let clip = ui.clip_rect();
        let focused = out.response.has_focus();
        let cur_b = out.cursor_range.map(|r| byte_of(text, r.primary.index.0)).unwrap_or(0);
        let cur_line = text[..cur_b.min(text.len())].matches('\n').count();
        let cw = ui.fonts_mut(|f| f.glyph_width(&egui::FontId::monospace(view.font_size), ' ')).max(1.0);
        let unit = indent_unit(text, lang);
        let unit_cols = if unit == "\t" { 4 } else { unit.len() };
        let lines: Vec<&str> = text.split('\n').collect();
        let mut bg = vec![];
        let mut fg = vec![];
        let mut line = 0usize;
        let mut first_row = true;
        let font = egui::FontId::monospace((view.font_size - 1.0).max(8.0));
        let diag_lines: HashMap<usize, &str> = ed.diags.iter().map(|d| (d.line, d.msg.as_str())).collect();
        let mut hover_msg = None;
        let ranges = fold_ranges(text);
        let fold_at: HashMap<usize, usize> = ranges.iter().map(|&(a, b)| (a, b)).collect();
        // Le curseur ne doit pas rester dans un bloc replié
        ed.folds.retain(|f| fold_at.get(f).is_none_or(|&e| !(cur_line > *f && cur_line <= e)) && fold_at.contains_key(f));
        let gutter_hover = ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| p.x < origin.x && p.x > clip.left() - 80.0);
        // Marques de modifications : différence avec la référence, recalculée quand le texte change
        if let Some(rx) = &ed.base_rx {
            if let Ok(r) = rx.try_recv() {
                ed.base_rx = None;
                if let Some(b) = r {
                    ed.base = Some(std::sync::Arc::new(b));
                    ed.base_git = true;
                    ed.changes = None;
                }
            }
        }
        let hunks: Vec<Hunk> = match (&ed.base, view.changes) {
            (Some(base), true) => {
                use std::hash::{Hash, Hasher};
                let mut hs = std::collections::hash_map::DefaultHasher::new();
                text.hash(&mut hs);
                let key = hs.finish();
                if !ed.changes.as_ref().is_some_and(|c| c.0 == key && std::sync::Arc::ptr_eq(&c.1, base)) {
                    ed.changes = Some((key, base.clone(), line_diff(base, text)));
                }
                ed.changes.as_ref().map(|c| c.2.clone()).unwrap_or_default()
            }
            _ => vec![],
        };
        let mut marks: HashMap<usize, (u8, usize)> = HashMap::new();
        for (k, h) in hunks.iter().enumerate() {
            if h.new.is_empty() {
                marks.entry(h.new.start).or_insert((2, k));
            } else {
                for l in h.new.clone() {
                    marks.insert(l, (h.kind(), k));
                }
            }
        }
        let last_line = lines.len().saturating_sub(1);
        let mut hunk_click = None;
        let mut hunk_anchor = None;
        let mut toggle = None;
        let mut fold_fg: Vec<(egui::Pos2, String, egui::Color32)> = vec![];
        for row in &out.galley.rows {
            let r = row.rect().translate(origin.to_vec2());
            let hidden_row = r.height() < 2.0;
            if !hidden_row && r.bottom() >= clip.top() - 40.0 && r.top() <= clip.bottom() + 40.0 {
                if first_row {
                    if let Some(&end) = fold_at.get(&line) {
                        let folded = ed.folds.contains(&line);
                        if folded || gutter_hover {
                            let c = egui::pos2(origin.x - 8.0, r.center().y);
                            let hit = egui::Rect::from_center_size(c, egui::vec2(12.0, r.height().max(10.0)));
                            let resp = ui.interact(hit, Id::unique(("fold", line)), egui::Sense::click());
                            let col = if resp.hovered() { p.accent } else { p.muted };
                            fold_fg.push((c, if folded { "▸".into() } else { "▾".into() }, col));
                            if resp.clicked() {
                                toggle = Some(line);
                            }
                        }
                        if folded {
                            // « ⋯ » après la ligne repliée
                            let x = r.right() + 8.0;
                            let n = end - line;
                            let label = if n == 1 { "⋯ 1 ligne".to_string() } else { format!("⋯ {n} lignes") };
                            let pill = egui::Rect::from_min_size(egui::pos2(x, r.top() + 1.0), egui::vec2(cw * 0.95 * label.chars().count() as f32 + 12.0, r.height() - 2.0));
                            fold_fg.push((pill.center(), label, p.muted));
                            bg.push(egui::Shape::rect_filled(pill, 2.0, p.line.gamma_multiply(0.6)));
                            if ui.interact(pill, Id::unique(("fold-pill", line)), egui::Sense::click()).clicked() {
                                toggle = Some(line);
                            }
                        }
                    }
                }
                if line == cur_line && (focused || ed.multi.is_empty()) {
                    bg.push(egui::Shape::rect_filled(egui::Rect::from_x_y_ranges(clip.x_range(), r.y_range()), 0.0, p.line.gamma_multiply(if focused { 0.45 } else { 0.2 })));
                }
                if first_row {
                    // Guides d'indentation (lignes vides : on garde l'indentation de la ligne d'avant)
                    let l = lines.get(line).copied().unwrap_or("");
                    let cols = if l.trim().is_empty() {
                        lines[..line.min(lines.len())].iter().rev().find(|x| !x.trim().is_empty()).map(|x| leading_cols(x)).unwrap_or(0)
                    } else {
                        leading_cols(l)
                    };
                    let mut k = unit_cols;
                    while k < cols && unit_cols > 0 {
                        let x = origin.x + (k - unit_cols) as f32 * cw + 1.0;
                        bg.push(egui::Shape::line_segment([egui::pos2(x, r.top()), egui::pos2(x, r.bottom())], egui::Stroke::new(1.0, p.line.gamma_multiply(0.8))));
                        k += unit_cols;
                    }
                    // Ligne ajoutée (vert), modifiée (bleu) ; lignes supprimées : petit triangle rouge
                    let mark = marks.get(&line).copied().or_else(|| (line == last_line).then(|| marks.get(&(line + 1)).copied()).flatten().map(|(_, k)| (3, k)));
                    if let Some((kind, k)) = mark {
                        let x = origin.x - 15.5;
                        let hit = egui::Rect::from_x_y_ranges(x - 2.0..=x + 5.0, r.y_range());
                        let resp = ui.interact(hit, Id::unique(("hunk", line)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                        let hot = resp.hovered() || ed.hunk_open == Some(k);
                        let col = [p.success, p.info, p.missing, p.missing][kind as usize];
                        match kind {
                            0 | 1 => bg.push(egui::Shape::rect_filled(egui::Rect::from_x_y_ranges(x..=x + if hot { 4.0 } else { 3.0 }, r.top()..=r.bottom()), 0.0, col)),
                            _ => {
                                let y = if kind == 2 { r.top() } else { r.bottom() };
                                bg.push(egui::Shape::convex_polygon(vec![egui::pos2(x, y - 4.0), egui::pos2(x + 5.0, y), egui::pos2(x, y + 4.0)], col, egui::Stroke::NONE));
                            }
                        }
                        if resp.clicked() {
                            hunk_click = Some(k);
                        }
                        if ed.hunk_open == Some(k) && hunk_anchor.is_none() {
                            hunk_anchor = Some(egui::pos2(origin.x, r.top()));
                        }
                    }
                    if view.gutter {
                        let col = if line == cur_line { p.text } else { p.muted.gamma_multiply(0.75) };
                        fg.push((egui::pos2(origin.x - 18.0, r.center().y), (line + 1).to_string(), col));
                    }
                    if let Some(msg) = diag_lines.get(&line) {
                        // Erreur : point rouge dans la gouttière et soulignement
                        fg.push((egui::pos2(origin.x - 5.0, r.center().y), "●".into(), p.missing));
                        bg.push(egui::Shape::rect_filled(egui::Rect::from_x_y_ranges(clip.x_range(), r.y_range()), 0.0, p.missing.gamma_multiply(0.10)));
                        let y = r.bottom() - 1.0;
                        let mut pts = vec![];
                        let mut x = r.left();
                        let mut up = true;
                        while x < r.right().max(r.left() + 40.0) {
                            pts.push(egui::pos2(x, if up { y - 1.5 } else { y + 0.5 }));
                            x += 3.0;
                            up = !up;
                        }
                        ui.painter().add(egui::Shape::line(pts, egui::Stroke::new(1.0, p.missing)));
                        if ui.rect_contains_pointer(egui::Rect::from_x_y_ranges(clip.left()..=clip.right(), r.y_range())) {
                            hover_msg = Some(msg.to_string());
                        }
                    }
                }
            }
            if row.ends_with_newline {
                line += 1;
                first_row = true;
            } else {
                first_row = false;
            }
        }
        ui.painter().set(view.bg, egui::Shape::Vec(bg));
        if let Some(k) = hunk_click {
            ed.hunk_open = if ed.hunk_open == Some(k) { None } else { Some(k) };
        }
        if ed.hunk_open.is_some_and(|k| k >= hunks.len()) {
            ed.hunk_open = None;
        }
        // Bulle du bloc cliqué : les lignes d'avant, et de quoi les remettre
        if let (Some(k), Some(at), Some(base)) = (ed.hunk_open, hunk_anchor, ed.base.clone()) {
            let h = hunks[k].clone();
            let old_lines: Vec<&str> = base.lines().skip(h.old.start).take(h.old.len()).collect();
            let mut close = false;
            egui::Area::new(Id::unique("hunk-pop")).order(egui::Order::Foreground).fixed_pos(at + egui::vec2(4.0, -6.0)).pivot(egui::Align2::LEFT_BOTTOM).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(560.0);
                    ui.horizontal(|ui| {
                        let what = [crate::i18n::t("lignes ajoutées"), crate::i18n::t("lignes modifiées"), crate::i18n::t("lignes supprimées")][h.kind() as usize].clone();
                        let since = crate::i18n::t(if ed.base_git { "depuis le dernier commit" } else { "depuis l'ouverture" });
                        ui.label(RichText::new(format!("{what} · {since}")).color(p.muted).size(11.5));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").clicked() {
                                close = true;
                            }
                            if ui.button(crate::i18n::t("restaurer")).on_hover_text(crate::i18n::t("Remettre ces lignes comme avant")).clicked() {
                                ed.revert = Some(revert_hunk(&base, text, &h));
                                close = true;
                            }
                        });
                    });
                    if !old_lines.is_empty() {
                        egui::Frame::new().fill(p.missing.gamma_multiply(0.12)).inner_margin(6).show(ui, |ui| {
                            for l in old_lines.iter().take(30) {
                                ui.label(RichText::new(format!("- {l}")).monospace().size(view.font_size - 2.0).color(p.text));
                            }
                            if old_lines.len() > 30 {
                                ui.label(RichText::new(format!("… +{}", old_lines.len() - 30)).color(p.muted).size(11.0));
                            }
                        });
                    }
                });
            });
            if close || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                ed.hunk_open = None;
            }
        }
        // Mini-carte à droite : tout le fichier en miniature, clic pour y aller
        let total = lines.len();
        if view.minimap && total > 40 {
            let w = 64.0;
            let mm = egui::Rect::from_min_max(egui::pos2(clip.right() - w - 6.0, clip.top() + 4.0), egui::pos2(clip.right() - 6.0, clip.bottom() - 4.0));
            let lh = (mm.height() / total as f32).min(3.0);
            let painter = ui.painter();
            painter.rect_filled(mm, 0.0, p.bg.gamma_multiply(0.92));
            painter.vline(mm.left(), mm.y_range(), egui::Stroke::new(1.0, p.line));
            let gh = out.galley.size().y.max(1.0);
            let (v0, v1) = (((clip.top() - origin.y) / gh).clamp(0.0, 1.0), ((clip.bottom() - origin.y) / gh).clamp(0.0, 1.0));
            let used = lh * total as f32;
            painter.rect_filled(egui::Rect::from_x_y_ranges(mm.x_range(), mm.top() + v0 * used..=mm.top() + v1 * used), 0.0, p.line.gamma_multiply(0.7));
            let mut shapes = vec![];
            for (k, l) in lines.iter().enumerate().take(30_000) {
                let t = l.trim_start();
                if t.is_empty() {
                    continue;
                }
                let ind = leading_cols(l) as f32 * 0.5;
                let len = (t.chars().count() as f32 * 0.5).min(w - 8.0 - ind);
                let y = mm.top() + k as f32 * lh;
                let col = if diag_lines.contains_key(&k) { p.missing } else if k == cur_line { p.accent } else { p.muted.gamma_multiply(0.5) };
                shapes.push(egui::Shape::rect_filled(egui::Rect::from_min_size(egui::pos2(mm.left() + 4.0 + ind, y), egui::vec2(len.max(1.0), (lh * 0.7).max(1.0))), 0.0, col));
            }
            painter.extend(shapes);
            let r = ui.interact(mm, Id::unique(("minimap", text.len() / 1_000_000)), egui::Sense::click_and_drag());
            if r.clicked() || r.dragged() {
                if let Some(pt) = r.interact_pointer_pos() {
                    let line = (((pt.y - mm.top()) / lh) as usize).min(total.saturating_sub(1));
                    let b = text.split_inclusive('\n').take(line).map(|l| l.len()).sum::<usize>().min(text.len());
                    let rr = out.galley.pos_from_cursor(CCursor::new(char_of(text, b))).translate(origin.to_vec2());
                    ui.scroll_to_rect(rr, Some(egui::Align::Center));
                }
            }
        }
        let painter = ui.painter();
        for (pos, t, col) in fg {
            painter.text(pos, egui::Align2::RIGHT_CENTER, t, font.clone(), col);
        }
        for (pos, t, col) in fold_fg {
            painter.text(pos, egui::Align2::CENTER_CENTER, t, font.clone(), col);
        }
        if let Some(l) = toggle {
            if !ed.folds.remove(&l) {
                ed.folds.insert(l);
            }
            ui.ctx().request_repaint();
        }
        if let Some(m) = hover_msg {
            egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), Id::unique("diag-tip"), egui::PopupAnchor::Pointer).show(|ui| {
                ui.label(RichText::new(m).color(p.missing));
            });
        }
    }
    let rect_of = |a: usize, b: usize| -> egui::Rect {
        let r1 = out.galley.pos_from_cursor(CCursor::new(char_of(text, a)));
        let r2 = out.galley.pos_from_cursor(CCursor::new(char_of(text, b)));
        egui::Rect::from_min_max(r1.min, egui::pos2(r2.max.x.max(r1.min.x + 2.0), r1.max.y.max(r2.max.y))).translate(origin.to_vec2())
    };

    // Curseurs supplémentaires
    if !ed.multi.is_empty() {
        let painter = ui.painter_at(out.text_clip_rect);
        for &(a, b) in &ed.multi {
            let (a, b) = (a.min(text.len()), b.min(text.len()));
            if a != b {
                painter.rect_filled(rect_of(a, b), 0.0, p.accent.gamma_multiply(0.3));
            }
            let r = rect_of(b, b);
            painter.line_segment([r.left_top(), r.left_bottom()], egui::Stroke::new(2.0, p.accent));
        }
        if out.response.clicked() {
            ed.multi.clear();
        }
    }
    // Occurrences de la recherche
    if let Some(f) = &ed.find {
        let painter = ui.painter_at(out.text_clip_rect);
        let clip = ui.clip_rect();
        for (a, b) in find_all(text, f).into_iter().take(2000) {
            let r = rect_of(a, b);
            if r.intersects(clip) {
                painter.rect_filled(r, 0.0, p.accent.gamma_multiply(0.28));
            }
        }
    }
    if let Some((a, b)) = ed.scroll_to.take() {
        ui.scroll_to_rect(rect_of(a, b).expand(40.0), Some(egui::Align::Center));
    }

    // Parenthèse correspondante
    let cursor = out.cursor_range.map(|r| r.primary.index.0);
    if let (Some(ci), true) = (cursor, out.response.has_focus()) {
        let b = byte_of(text, ci);
        if let Some(m) = matching_bracket(text, b) {
            let painter = ui.painter_at(out.text_clip_rect);
            for x in [m.0, m.1] {
                let ch_len = text[x..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                painter.rect_stroke(rect_of(x, x + ch_len), 0.0, egui::Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
            }
        }
    }

    // Survol : documentation du serveur LSP sous la souris
    if let Some(l) = &mut ed.lsp {
        let hp = ui.input(|i| i.pointer.hover_pos()).filter(|p| out.text_clip_rect.contains(*p) && ui.input(|i| i.pointer.velocity().length() < 50.0));
        match hp {
            Some(pt) => {
                let cc = out.galley.cursor_from_pos(pt - origin);
                let b = byte_of(text, cc.index.0.min(text.chars().count()));
                let on_word = text[b..].chars().next().is_some_and(is_word);
                if !on_word {
                    l.hover_at = None;
                } else if l.hover_at.is_none_or(|(x, _)| x != b) {
                    l.hover_at = Some((b, std::time::Instant::now()));
                    l.hover = None;
                    l.hover_asked = None;
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(550));
                } else if let Some((x, t)) = l.hover_at {
                    if t.elapsed() > std::time::Duration::from_millis(500) && l.hover_asked != Some(x) {
                        l.hover_asked = Some(x);
                        l.ask_hover(text, x);
                    }
                }
                if let (Some((x, _)), Some((y, msg))) = (l.hover_at, &l.hover) {
                    if x == *y {
                        let msg = msg.clone();
                        egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), Id::unique("lsp-hover"), egui::PopupAnchor::Pointer).show(|ui| {
                            ui.set_max_width(520.0);
                            ui.label(RichText::new(msg).monospace().size(11.5));
                        });
                    }
                }
            }
            None => l.hover_at = None,
        }
    }

    // Suggestions
    let Some(ci) = cursor else {
        ed.popup = None;
        return;
    };
    if !out.response.has_focus() {
        ed.popup = None;
        return;
    }
    if let Some(pp) = &ed.popup {
        if pp.cursor != ci && !typed_now {
            ed.popup = None;
        }
    }
    let cb = byte_of(text, ci);
    if typed_now || ed.force_popup {
        let force = ed.force_popup;
        ed.force_popup = false;
        ed.popup = suggest(text, ci, lang, notes, &ed.project, force);
        // Le serveur LSP propose aussi ses suggestions (méthodes, champs, imports…)
        if let Some(l) = &mut ed.lsp {
            let prev = text[..cb].chars().last();
            if force || prev.is_some_and(|c| is_word(c) || matches!(c, '.' | ':' | '>')) {
                l.ask_completion(text, cb);
            }
        }
    }
    if let Some(l) = &mut ed.lsp {
        if let Some((at, list)) = l.completion.take() {
            if at == cb {
                let ls = line_start(text, cb);
                let start = text[ls..cb].char_indices().rev().take_while(|(_, c)| is_word(*c)).last().map(|(i, _)| ls + i).unwrap_or(cb);
                let pre = text[start..cb].to_lowercase();
                let mut items: Vec<(String, &'static str)> = list.into_iter().filter(|w| pre.is_empty() || w.to_lowercase().starts_with(&pre)).filter(|w| w.to_lowercase() != pre).take(12).map(|w| (w, "lsp")).collect();
                if let Some(old) = &ed.popup {
                    for it in &old.items {
                        if items.len() < 14 && !items.iter().any(|x| x.0 == it.0) {
                            items.push(it.clone());
                        }
                    }
                }
                if !items.is_empty() {
                    ed.popup = Some(Popup { items, sel: 0, start: char_of(text, start), cursor: ci });
                }
            }
        }
    }
    let Some(pp) = &ed.popup else { return };
    let pos = rect_of(byte_of(text, pp.cursor), byte_of(text, pp.cursor)).left_bottom() + egui::vec2(0.0, 2.0);
    let mut clicked = None;
    egui::Area::new(Id::unique("code-ac")).order(egui::Order::Foreground).fixed_pos(pos).show(ui.ctx(), |ui| {
        egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(2).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let w = 240.0;
            for (k, (word, kind)) in pp.items.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 20.0), egui::Sense::click());
                if k == pp.sel {
                    ui.painter().rect_filled(r, 0.0, p.accent.gamma_multiply(0.18));
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, 0.0, p.line.gamma_multiply(0.5));
                }
                ui.painter().text(r.left_center() + egui::vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, word, egui::FontId::monospace(12.5), p.text);
                ui.painter().text(r.right_center() - egui::vec2(6.0, 0.0), egui::Align2::RIGHT_CENTER, *kind, egui::FontId::monospace(10.5), p.muted);
                if resp.clicked() {
                    clicked = Some(k);
                }
            }
            ui.label(RichText::new(crate::i18n::t("Tab / Entrée : insérer · Échap")).size(10.0).color(p.muted));
        });
    });
    if clicked.is_some() {
        ed.click_accept = clicked;
        ui.memory_mut(|m| m.request_focus(out.response.id));
    }
}

fn matching_bracket(text: &str, b: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let check = |i: usize| -> Option<(usize, usize)> {
        let c = *bytes.get(i)?;
        let (open, close, fwd) = match c {
            b'(' => (b'(', b')', true),
            b'[' => (b'[', b']', true),
            b'{' => (b'{', b'}', true),
            b')' => (b'(', b')', false),
            b']' => (b'[', b']', false),
            b'}' => (b'{', b'}', false),
            _ => return None,
        };
        let mut depth = 0i32;
        if fwd {
            for (k, &x) in bytes[i..].iter().enumerate().take(200_000) {
                if x == open {
                    depth += 1;
                } else if x == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some((i, i + k));
                    }
                }
            }
        } else {
            for k in (i.saturating_sub(200_000)..=i).rev() {
                let x = bytes[k];
                if x == close {
                    depth += 1;
                } else if x == open {
                    depth -= 1;
                    if depth == 0 {
                        return Some((k, i));
                    }
                }
            }
        }
        None
    };
    check(b).or_else(|| b.checked_sub(1).and_then(check))
}

fn suggest(text: &str, ci: usize, lang: &str, notes: &[String], project: &[String], force: bool) -> Option<Popup> {
    let b = byte_of(text, ci);
    let ls = line_start(text, b);
    let line = &text[ls..b];
    // [[note : suggestions de notes
    if let Some(i) = line.rfind("[[") {
        let q = &line[i + 2..];
        if !q.contains("]]") {
            let ql = q.to_lowercase();
            let mut items: Vec<(String, &'static str)> = notes.iter().filter(|n| n.to_lowercase().contains(&ql)).take(8).map(|n| (n.clone(), "note")).collect();
            items.sort_by_key(|(n, _)| !n.to_lowercase().starts_with(&ql));
            if items.is_empty() {
                return None;
            }
            return Some(Popup { items, sel: 0, start: char_of(text, ls + i + 2), cursor: ci });
        }
    }
    let word_chars = |c: char| is_word(c) || c == '-' && matches!(lang, "css" | "scss" | "less" | "html" | "htm") || c == '.' && lang == "py" || c == '!' && lang == "rs";
    let start = line.char_indices().rev().take_while(|(_, c)| word_chars(*c)).last().map(|(i, _)| ls + i).unwrap_or(b);
    let prefix = &text[start..b];
    if prefix.chars().count() < if force { 0 } else { 2 } || prefix.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    // Pas de suggestions au milieu d'un mot
    if text[b..].chars().next().is_some_and(word_chars) {
        return None;
    }
    let pl = prefix.to_lowercase();
    let mut seen = std::collections::HashSet::new();
    let mut items: Vec<(String, &'static str, i32)> = vec![];
    let in_tag = html_like(lang) && text[..start].ends_with('<');
    let kw_kind = if in_tag || matches!(lang, "html" | "htm") { "balise" } else { "mot-clé" };
    for (trig, _, _) in snippets(lang) {
        if trig.starts_with(&pl) && seen.insert(format!("snip:{trig}")) {
            items.push((trig.to_string(), "extrait", 100));
        }
    }
    for k in keywords(lang) {
        if k.to_lowercase().starts_with(&pl) && *k != prefix && seen.insert(k.to_string()) {
            items.push((k.to_string(), kw_kind, 0));
        }
    }
    let scan = &text[..text.len().min(400_000)];
    let mut count: std::collections::HashMap<&str, i32> = std::collections::HashMap::new();
    let mut i = 0;
    let bytes = scan.as_bytes();
    while i < scan.len() {
        if !scan.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let ch = scan[i..].chars().next().unwrap();
        if word_chars(ch) && !ch.is_ascii_digit() {
            let s = i;
            let mut e = i;
            for (k, c) in scan[i..].char_indices() {
                if !word_chars(c) {
                    break;
                }
                e = i + k + c.len_utf8();
            }
            if e - s >= 3 && s != start {
                *count.entry(&scan[s..e]).or_default() += 1;
            }
            i = e.max(i + 1);
        } else {
            i += ch.len_utf8();
            let _ = bytes;
        }
    }
    let mut words: Vec<(&str, i32)> = count.into_iter().filter(|(w, _)| w.to_lowercase().starts_with(&pl) && *w != prefix).collect();
    words.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.len().cmp(&b.0.len())));
    for (w, n) in words.into_iter().take(12) {
        if seen.insert(w.to_string()) {
            items.push((w.to_string(), "fichier", n));
        }
    }
    for w in project.iter().filter(|w| w.to_lowercase().starts_with(&pl) && w.as_str() != prefix).take(20) {
        if seen.insert(w.clone()) {
            items.push((w.clone(), "projet", 0));
        }
    }
    items.sort_by(|a, b| (a.1 != "extrait").cmp(&(b.1 != "extrait")).then(a.0.len().cmp(&b.0.len())).then(b.2.cmp(&a.2)));
    items.sort_by_key(|i| !i.0.starts_with(prefix) && i.1 != "extrait");
    items.truncate(10);
    if items.is_empty() {
        return None;
    }
    Some(Popup { items: items.into_iter().map(|(w, k, _)| (w, k)).collect(), sel: 0, start: char_of(text, start), cursor: ci })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, at: usize, f: impl FnOnce(&mut Ctx)) -> (String, usize) {
        let mut s = text.to_string();
        let mut c = Ctx { text: &mut s, a: at, b: at, changed: false };
        f(&mut c);
        let a = c.a;
        (s, a)
    }

    #[test]
    fn closes_tags_and_pairs() {
        let (s, a) = run("<div class=\"x\"", 14, |c| {
            typed(c, '>', "html");
        });
        assert_eq!(s, "<div class=\"x\"></div>");
        assert_eq!(a, 15);
        let (s, _) = run("<br", 3, |c| {
            typed(c, '>', "html");
        });
        assert_eq!(s, "<br");
        let (s, a) = run("f", 1, |c| {
            typed(c, '(', "rs");
        });
        assert_eq!((s.as_str(), a), ("f()", 2));
        let (s, a) = run("<div>", 5, |c| enter(c, "html", "  "));
        assert_eq!((s.as_str(), a), ("<div>\n  ", 8));
        let (s, a) = run("<div></div>", 5, |c| enter(c, "html", "  "));
        assert_eq!((s.as_str(), a), ("<div>\n  \n</div>", 8));
        let (s, a) = run("fn x() {}", 8, |c| enter(c, "rs", "    "));
        assert_eq!((s.as_str(), a), ("fn x() {\n    \n}", 13));
        let (s, _) = run("- un", 4, |c| enter(c, "md", "  "));
        assert_eq!(s, "- un\n- ");
        let (s, _) = run("let a = 1;", 0, |c| toggle_comment(c, "rs"));
        assert_eq!(s, "// let a = 1;");
        let (s, _) = run("// let a = 1;", 0, |c| toggle_comment(c, "rs"));
        assert_eq!(s, "let a = 1;");
        let (s, _) = run("ul>li*2", 7, |c| {
            try_emmet(c, "  ");
        });
        assert_eq!(s, "<ul>\n  <li></li>\n  <li></li>\n</ul>");
        assert_eq!(last_open_tag("<div><p>a</p><span>").as_deref(), Some("span"));

        // Curseurs multiples : Ctrl+D deux fois puis frappe
        let mut s = "let a = 1;\nlet b = a + a;".to_string();
        let mut multi = vec![];
        let mut c = Ctx { text: &mut s, a: 4, b: 4, changed: false };
        select_next(&mut c, &mut multi); // sélectionne « a »
        assert_eq!((c.a, c.b), (4, 5));
        select_next(&mut c, &mut multi);
        select_next(&mut c, &mut multi);
        assert_eq!(multi.len(), 2);
        multi_apply(&mut c, &mut multi, &MultiOp::Insert("x".into()));
        assert_eq!(c.text.as_str(), "let x = 1;\nlet b = x + x;");
        multi_apply(&mut c, &mut multi, &MultiOp::Back);
        assert_eq!(c.text.as_str(), "let  = 1;\nlet b =  + ;");
        // Structure
        let syms = symbols("fn main() {}\npub struct A;\nimpl A {\n    pub fn new() -> Self { A }\n}", "rs");
        assert_eq!(syms.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["main", "A", "A", "new"]);
        let syms = symbols("class Foo:\n    def bar(self):\n        pass", "py");
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[1].depth, 1);
        // Formatage JSON interne
        assert_eq!(format("{\"a\":1}", "json", "x.json").unwrap(), "{\n  \"a\": 1\n}\n");
    }
}


fn leading_cols(l: &str) -> usize {
    let mut n = 0;
    for c in l.chars() {
        match c {
            ' ' => n += 1,
            '\t' => n += 4,
            _ => break,
        }
    }
    n
}

/// Barre « aller à la ligne » (Ctrl+G) ou « aller au symbole » (@nom, Ctrl+Maj+O).
pub fn goto_bar(ui: &mut egui::Ui, ed: &mut Editor, text: &str, lang: &str) {
    let p = theme::pal();
    let Some(q) = &mut ed.goto else { return };
    let mut close = false;
    let mut jump = None;
    egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let syms = q.starts_with('@');
        ui.horizontal(|ui| {
            ui.label(RichText::new(if syms { "symbole" } else { "ligne" }).color(p.muted).size(11.5));
            let r = ui.add(egui::TextEdit::singleline(q).hint_text(crate::i18n::t("numéro de ligne, ou @ pour un symbole")).desired_width(260.0));
            r.request_focus();
            let total = text.lines().count();
            ui.label(RichText::new(format!("{total} lignes")).color(p.muted).size(11.0));
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                close = true;
            }
            let enter = ui.input(|i| i.key_pressed(Key::Enter));
            if !syms && enter {
                if let Ok(n) = q.trim().parse::<usize>() {
                    jump = Some(n.saturating_sub(1));
                }
            }
        });
        if syms {
            let f = crate::chat::fold(q.trim_start_matches('@'));
            let list: Vec<Symbol> = symbols(text, lang).into_iter().filter(|s| crate::chat::fold(&s.name).contains(&f)).take(12).collect();
            for (k, s) in list.iter().enumerate() {
                let t = RichText::new(format!("{}{}  {}", "  ".repeat(s.depth), s.name, s.kind)).color(if k == 0 { p.accent } else { p.text }).size(12.0);
                if ui.add(egui::Button::new(t).frame(false)).clicked() {
                    jump = Some(s.line);
                }
            }
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                jump = list.first().map(|s| s.line).or(jump);
            }
            if list.is_empty() {
                ui.label(RichText::new(crate::i18n::t("aucun symbole")).color(p.muted).size(11.0));
            }
        }
    });
    if let Some(l) = jump {
        ed.jump = Some(l);
        close = true;
    }
    if close {
        ed.goto = None;
    }
}


// ---------- repli de code ----------

/// Replie (ou déplie) le bloc qui contient la ligne `line`.
pub fn fold_at_line(ed: &mut Editor, text: &str, line: usize, fold: bool) {
    let r = fold_ranges(text);
    let best = r.iter().filter(|(a, b)| *a <= line && line <= *b).max_by_key(|(a, _)| *a).map(|(a, _)| *a);
    if let Some(a) = best {
        if fold {
            ed.folds.insert(a);
        } else {
            ed.folds.remove(&a);
        }
    }
}

/// Replie tous les blocs de premier niveau (ou déplie tout).
pub fn fold_all(ed: &mut Editor, text: &str, fold: bool) {
    ed.folds.clear();
    if fold {
        let mut last_end = None;
        for (a, b) in fold_ranges(text) {
            if last_end.is_some_and(|e| a <= e) {
                continue;
            }
            ed.folds.insert(a);
            last_end = Some(b);
        }
    }
}

/// Blocs repliables : (ligne de début, dernière ligne cachée), selon l'indentation.
/// Une ligne ouvre un bloc si la suivante (non vide) est plus indentée.
pub fn fold_ranges(text: &str) -> Vec<(usize, usize)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let ind: Vec<Option<usize>> = lines.iter().map(|l| if l.trim().is_empty() { None } else { Some(leading_cols(l)) }).collect();
    let mut out = vec![];
    for i in 0..lines.len() {
        let Some(base) = ind[i] else { continue };
        let Some(j) = (i + 1..lines.len()).find(|&j| ind[j].is_some()) else { continue };
        if ind[j].unwrap() <= base {
            continue;
        }
        let mut end = j;
        for k in j..lines.len() {
            match ind[k] {
                Some(c) if c <= base => break,
                Some(_) => end = k,
                None => {}
            }
        }
        if end > i {
            out.push((i, end));
        }
    }
    out
}

/// Plages d'octets cachées par les replis actifs.
pub fn hidden_bytes(text: &str, folds: &std::collections::BTreeSet<usize>) -> Vec<std::ops::Range<usize>> {
    if folds.is_empty() {
        return vec![];
    }
    let starts: Vec<usize> = std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).collect();
    let line_end = |l: usize| starts.get(l + 1).copied().unwrap_or(text.len());
    let mut out: Vec<std::ops::Range<usize>> = vec![];
    for (s, e) in fold_ranges(text) {
        if !folds.contains(&s) {
            continue;
        }
        let r = starts[s + 1]..line_end(e);
        // Replis imbriqués : on garde le plus grand
        if out.last().is_some_and(|l| l.end >= r.end && l.start <= r.start) {
            continue;
        }
        out.push(r);
    }
    out
}

/// Rend minuscules et invisibles les lignes repliées (le texte reste intact).
pub fn apply_folds(job: &mut egui::text::LayoutJob, hidden: &[std::ops::Range<usize>]) {
    if hidden.is_empty() {
        return;
    }
    let mut sections = Vec::with_capacity(job.sections.len() + hidden.len() * 2);
    for sec in job.sections.drain(..) {
        let (s0, s1) = (sec.byte_range.start.0, sec.byte_range.end.0);
        let mut cuts = vec![s0, s1];
        for h in hidden {
            for b in [h.start, h.end] {
                if b > s0 && b < s1 {
                    cuts.push(b);
                }
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        for w in cuts.windows(2) {
            let mut part = sec.clone();
            part.byte_range = egui::text::ByteIndex(w[0])..egui::text::ByteIndex(w[1]);
            part.leading_space = if w[0] == s0 { sec.leading_space } else { 0.0 };
            if hidden.iter().any(|h| w[0] >= h.start && w[1] <= h.end) {
                part.format.font_id.size = 0.01;
                part.format.color = egui::Color32::TRANSPARENT;
                part.format.background = egui::Color32::TRANSPARENT;
            }
            sections.push(part);
        }
    }
    job.sections = sections;
}
