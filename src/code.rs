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
    if ed.find.as_ref().is_some_and(|f| f.focus) {
        // la barre de recherche a le focus : rien à intercepter ici
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
                        Key::D if cmd => {
                            let (s, e) = line_span(c.text, c.a, c.b);
                            let block = c.text[s..e].to_string();
                            let off = block.len() + 1;
                            let (a, b) = (c.a + off, c.b + off);
                            c.set(e, e, &format!("\n{block}"), (a, b));
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
                        Key::ArrowUp | Key::ArrowDown if m.alt => {
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
            let r = ui.add(egui::TextEdit::singleline(&mut f.query).hint_text("chercher").desired_width(220.0));
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
            ui.toggle_value(&mut f.case, "Aa").on_hover_text("respecter la casse");
            let count = if f.query.is_empty() { String::new() } else if matches.is_empty() { "aucun".into() } else { format!("{}/{}", (f.current + 1).min(matches.len()), matches.len()) };
            ui.label(RichText::new(count).color(if matches.is_empty() && !f.query.is_empty() { p.missing } else { p.muted }).size(11.5));
            if ui.small_button("↑").on_hover_text("précédent (Maj+F3)").clicked() {
                go = Some(true);
            }
            if ui.small_button("↓").on_hover_text("suivant (F3, Entrée)").clicked() {
                go = Some(false);
            }
            ui.toggle_value(&mut f.show_replace, "remplacer").on_hover_text("Ctrl+H");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("×").clicked() {
                    close = true;
                }
            });
        });
        if f.show_replace {
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut f.replace).hint_text("remplacer par").desired_width(220.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    replace_one = true;
                    r.request_focus();
                }
                replace_one |= ui.button("remplacer").clicked();
                replace_all |= ui.button("tout remplacer").clicked();
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

pub fn after(ui: &mut egui::Ui, ed: &mut Editor, text: &str, lang: &str, out: &egui::text_edit::TextEditOutput, notes: &[String], typed_now: bool) {
    let p = theme::pal();
    let origin = out.galley_pos;
    let rect_of = |a: usize, b: usize| -> egui::Rect {
        let r1 = out.galley.pos_from_cursor(CCursor::new(char_of(text, a)));
        let r2 = out.galley.pos_from_cursor(CCursor::new(char_of(text, b)));
        egui::Rect::from_min_max(r1.min, egui::pos2(r2.max.x.max(r1.min.x + 2.0), r1.max.y.max(r2.max.y))).translate(origin.to_vec2())
    };

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
    if typed_now || ed.force_popup {
        let force = std::mem::take(&mut ed.force_popup);
        ed.popup = suggest(text, ci, lang, notes, force);
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
            ui.label(RichText::new("Tab / Entrée : insérer · Échap").size(10.0).color(p.muted));
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

fn suggest(text: &str, ci: usize, lang: &str, notes: &[String], force: bool) -> Option<Popup> {
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
    items.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then(b.2.cmp(&a.2)));
    items.sort_by_key(|i| !i.0.starts_with(prefix));
    items.truncate(8);
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
    }
}
