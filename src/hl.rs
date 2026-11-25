//! Coloration du code aux couleurs de la charte (maquette) : mots-clés rose, chaînes or pâle,
//! fonctions bleu, nombres orangé, commentaires gris. Un découpage simple, rapide, sans état :
//! il couvre les langages courants du studio ; les autres passent par syntect.
use eframe::egui::{text::LayoutJob, Color32, FontId, TextFormat};

pub struct Colors {
    pub text: Color32,
    pub keyword: Color32,
    pub string: Color32,
    pub function: Color32,
    pub number: Color32,
    pub comment: Color32,
    pub kind: Color32,
}

pub fn colors(dark: bool) -> Colors {
    if dark {
        Colors {
            text: Color32::from_rgb(0xf4, 0xee, 0xf1),
            keyword: Color32::from_rgb(0xcb, 0x9a, 0xb2),
            string: Color32::from_rgb(0xc4, 0xb6, 0x8d),
            function: Color32::from_rgb(0x8b, 0xa8, 0xc6),
            number: Color32::from_rgb(0xe3, 0x9b, 0x6f),
            comment: Color32::from_rgb(0x94, 0x8e, 0x91),
            kind: Color32::from_rgb(0x83, 0xb9, 0x9e),
        }
    } else {
        Colors {
            text: Color32::from_rgb(0x2a, 0x22, 0x26),
            keyword: Color32::from_rgb(0x9a, 0x3f, 0x6e),
            string: Color32::from_rgb(0x7a, 0x62, 0x1e),
            function: Color32::from_rgb(0x2f, 0x5a, 0x8a),
            number: Color32::from_rgb(0x9a, 0x4a, 0x24),
            comment: Color32::from_rgb(0x7a, 0x72, 0x76),
            kind: Color32::from_rgb(0x2e, 0x6e, 0x4e),
        }
    }
}

struct Lang {
    keywords: &'static [&'static str],
    line_comment: &'static [&'static str],
    block: Option<(&'static str, &'static str)>,
}

fn lang(ext: &str) -> Option<Lang> {
    let c_like = Some(("/*", "*/"));
    Some(match ext {
        "py" | "pyw" => Lang { keywords: &["False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with", "yield", "self", "match", "case"], line_comment: &["#"], block: None },
        "rs" => Lang { keywords: &["as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while"], line_comment: &["//"], block: c_like },
        "js" | "ts" | "jsx" | "tsx" | "mjs" => Lang { keywords: &["async", "await", "break", "case", "catch", "class", "const", "continue", "default", "delete", "do", "else", "export", "extends", "false", "finally", "for", "from", "function", "if", "import", "in", "instanceof", "let", "new", "null", "of", "return", "static", "super", "switch", "this", "throw", "true", "try", "typeof", "undefined", "var", "void", "while", "yield", "interface", "type", "enum"], line_comment: &["//"], block: c_like },
        "c" | "h" | "cpp" | "hpp" | "cc" | "cs" | "java" | "glsl" | "hlsl" | "frag" | "vert" | "osl" | "go" => Lang { keywords: &["auto", "break", "case", "char", "class", "const", "continue", "default", "do", "double", "else", "enum", "extern", "false", "float", "for", "func", "if", "import", "in", "inout", "int", "long", "namespace", "new", "out", "package", "private", "public", "return", "short", "signed", "static", "struct", "switch", "this", "true", "typedef", "uniform", "unsigned", "using", "var", "void", "while", "vec2", "vec3", "vec4", "mat3", "mat4", "float2", "float3", "float4", "color", "point", "normal", "shader", "surface", "string", "bool"], line_comment: &["//"], block: c_like },
        "lua" => Lang { keywords: &["and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while"], line_comment: &["--"], block: None },
        "sh" | "bash" | "zsh" => Lang { keywords: &["if", "then", "else", "elif", "fi", "for", "while", "do", "done", "case", "esac", "function", "in", "return", "export", "local"], line_comment: &["#"], block: None },
        "gd" => Lang { keywords: &["extends", "class_name", "func", "var", "const", "signal", "if", "elif", "else", "for", "while", "match", "return", "pass", "break", "continue", "true", "false", "null", "self", "and", "or", "not", "in", "static", "enum", "await"], line_comment: &["#"], block: None },
        _ => return None,
    })
}

/// Vrai si ce langage a une coloration à la charte
pub fn supports(ext: &str) -> bool {
    lang(ext).is_some()
}

pub fn highlight(text: &str, ext: &str, font: FontId, c: &Colors) -> LayoutJob {
    let mut job = LayoutJob::default();
    let Some(l) = lang(ext) else {
        job.append(text, 0.0, TextFormat::simple(font, c.text));
        return job;
    };
    let push = |job: &mut LayoutJob, s: &str, col: Color32| {
        if !s.is_empty() {
            job.append(s, 0.0, TextFormat::simple(font.clone(), col));
        }
    };
    let b = text.as_bytes();
    let mut i = 0;
    let mut plain = 0;
    while i < b.len() {
        let rest = &text[i..];
        // Commentaire de ligne
        if let Some(lc) = l.line_comment.iter().find(|lc| rest.starts_with(**lc)) {
            let _ = lc;
            push(&mut job, &text[plain..i], c.text);
            let end = rest.find('\n').map(|k| i + k).unwrap_or(b.len());
            push(&mut job, &text[i..end], c.comment);
            i = end;
            plain = i;
            continue;
        }
        if let Some((open, close)) = l.block {
            if rest.starts_with(open) {
                push(&mut job, &text[plain..i], c.text);
                let end = rest[open.len()..].find(close).map(|k| i + open.len() + k + close.len()).unwrap_or(b.len());
                push(&mut job, &text[i..end], c.comment);
                i = end;
                plain = i;
                continue;
            }
        }
        let ch = b[i];
        // Chaînes : guillemets simples ou doubles, triples en Python, échappements
        if ch == b'"' || ch == b'\'' || ch == b'`' {
            if ch == b'\'' && ext == "rs" && !rest[1..].chars().nth(1).is_some_and(|x| x == '\'') && !rest[1..].starts_with('\\') {
                // Durée de vie Rust ('a), pas une chaîne
                i += 1;
                continue;
            }
            push(&mut job, &text[plain..i], c.text);
            let triple = ext.starts_with("py") && (rest.starts_with("\"\"\"") || rest.starts_with("'''"));
            let end = if triple {
                let q = &rest[..3];
                rest[3..].find(q).map(|k| i + 3 + k + 3).unwrap_or(b.len())
            } else {
                let mut k = i + 1;
                while k < b.len() && b[k] != ch && b[k] != b'\n' {
                    k += if b[k] == b'\\' { 2 } else { 1 };
                }
                (k + 1).min(b.len())
            };
            let end = (end..=b.len()).find(|&e| text.is_char_boundary(e)).unwrap_or(b.len());
            push(&mut job, &text[i..end], c.string);
            i = end;
            plain = i;
            continue;
        }
        // Mots : mot-clé, appel de fonction, type (Majuscule) ou nombre
        if ch.is_ascii_alphanumeric() || ch == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || (b[start].is_ascii_digit() && b[i] == b'.')) {
                i += 1;
            }
            let w = &text[start..i];
            let col = if b[start].is_ascii_digit() {
                Some(c.number)
            } else if l.keywords.contains(&w) {
                Some(c.keyword)
            } else if text[i..].trim_start_matches(' ').starts_with('(') || text[i..].starts_with("!(") {
                Some(c.function)
            } else if b[start].is_ascii_uppercase() && w.chars().any(|x| x.is_lowercase()) {
                Some(c.kind)
            } else {
                None
            };
            if let Some(col) = col {
                push(&mut job, &text[plain..start], c.text);
                push(&mut job, w, col);
                plain = i;
            }
            continue;
        }
        i += text[i..].chars().next().map(|x| x.len_utf8()).unwrap_or(1);
    }
    push(&mut job, &text[plain..], c.text);
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colore_sans_perdre_de_texte() {
        let c = colors(true);
        for (src, ext) in [("def f(x):\n    return \"é\" + 'a' # note\n", "py"), ("fn main() { let s = \"x\"; /* bloc */ 'a: loop {} } // fin", "rs"), ("const a = `t`; f(1.5)", "js"), ("rien", "zzz")] {
            let job = highlight(src, ext, FontId::monospace(13.0), &c);
            assert_eq!(job.text, src);
        }
        let job = highlight("def f(): pass", "py", FontId::monospace(13.0), &c);
        assert_eq!(job.sections[0].format.color, c.keyword);
        assert!(supports("py") && !supports("md"));
    }
}

/// Paires de parenthèses colorées selon leur profondeur (comme VS Code) : seules celles de la couleur du
/// texte ordinaire changent, donc jamais dans une chaîne ou un commentaire
pub fn color_brackets(job: &mut eframe::egui::text::LayoutJob, plain: Color32, dark: bool) {
    use eframe::egui::text::ByteIndex as BI;
    let cols = if dark {
        [Color32::from_rgb(0xe6, 0xc0, 0x7b), Color32::from_rgb(0xd0, 0x8f, 0xd6), Color32::from_rgb(0x7f, 0xb4, 0xe6)]
    } else {
        [Color32::from_rgb(0x9a, 0x6a, 0x00), Color32::from_rgb(0x9a, 0x3f, 0xa6), Color32::from_rgb(0x1f, 0x66, 0xb3)]
    };
    let text = job.text.clone();
    let mut depth = 0usize;
    let mut out = Vec::with_capacity(job.sections.len());
    for sec in std::mem::take(&mut job.sections) {
        let (s0, s1) = (sec.byte_range.start.0, sec.byte_range.end.0);
        if sec.format.color != plain || !text[s0..s1].contains(['(', ')', '[', ']', '{', '}']) {
            out.push(sec);
            continue;
        }
        let mut start = s0;
        for (i, c) in text[s0..s1].char_indices() {
            let b = s0 + i;
            let open = matches!(c, '(' | '[' | '{');
            if !open && !matches!(c, ')' | ']' | '}') {
                continue;
            }
            if !open {
                depth = depth.saturating_sub(1);
            }
            let color = cols[depth % 3];
            if open {
                depth += 1;
            }
            if b > start {
                let lead = if start == s0 { sec.leading_space } else { 0.0 };
                out.push(eframe::egui::text::LayoutSection { leading_space: lead, byte_range: BI(start)..BI(b), format: sec.format.clone() });
            }
            let mut f = sec.format.clone();
            f.color = color;
            let lead = if b == s0 { sec.leading_space } else { 0.0 };
            out.push(eframe::egui::text::LayoutSection { leading_space: lead, byte_range: BI(b)..BI(b + 1), format: f });
            start = b + 1;
        }
        if start < s1 {
            out.push(eframe::egui::text::LayoutSection { leading_space: 0.0, byte_range: BI(start)..BI(s1), format: sec.format.clone() });
        }
    }
    job.sections = out;
}

#[cfg(test)]
mod bracket_tests {
    use super::*;

    #[test]
    fn paires_colorees() {
        let c = colors(true);
        let mut job = highlight("f(a[b], \"(x)\") // (c)", "rs", eframe::egui::FontId::monospace(12.0), &c);
        color_brackets(&mut job, c.text, true);
        let color_at = |b: usize| job.sections.iter().find(|s| s.byte_range.start.0 <= b && b < s.byte_range.end.0).unwrap().format.color;
        let t = job.text.clone();
        let (o1, o2) = (t.find('(').unwrap(), t.find('[').unwrap());
        assert_ne!(color_at(o1), c.text);
        assert_ne!(color_at(o1), color_at(o2));
        // La fermante a la couleur de son ouvrante
        assert_eq!(color_at(t.find(']').unwrap()), color_at(o2));
        let close = t.find("\")").map(|i| i + 1).unwrap();
        assert_eq!(color_at(close), color_at(o1));
        // Dans la chaîne et le commentaire : inchangées
        let in_str = t.find("\"(").unwrap() + 1;
        assert_eq!(color_at(in_str), c.string);
        let sections_ok = job.sections.windows(2).all(|w| w[0].byte_range.end == w[1].byte_range.start);
        assert!(sections_ok);
    }
}
