//! Export d'une note : une page HTML autonome (mise en page propre, images intégrées) ou un PDF
//! simple (titres, paragraphes, listes, citations), pour envoyer un document sans Aryadne.

use std::fmt::Write;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Mise en forme dans une ligne : **gras**, *italique*, `code`, [[liens]], [texte](url), ![image](chemin)
fn inline(s: &str, root: &std::path::Path) -> String {
    let mut out = String::new();
    let mut rest = s;
    while !rest.is_empty() {
        // Formule $x^2$ (sans espace juste à l'intérieur des $)
        if let Some(r) = rest.strip_prefix('$') {
            if let Some(end) = r.find('$').filter(|e| *e > 0 && !r.starts_with(' ') && !r[..*e].ends_with(' ')) {
                let _ = write!(out, "<em>{}</em>", esc(&crate::mdextra::tex(&r[..end])));
                rest = &r[end + 1..];
                continue;
            }
        }
        if let Some(r) = rest.strip_prefix("![") {
            if let Some((alt, after)) = r.split_once("](") {
                if let Some((src, tail)) = after.split_once(')') {
                    let path = root.join(src.trim());
                    let data = std::fs::read(&path).ok().map(|b| {
                        use base64::Engine;
                        let mime = if src.to_lowercase().ends_with(".png") { "png" } else if src.to_lowercase().ends_with(".webp") { "webp" } else { "jpeg" };
                        format!("data:image/{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(b))
                    });
                    let _ = write!(out, r#"<img alt="{}" src="{}">"#, esc(alt), data.unwrap_or_else(|| esc(src)));
                    rest = tail;
                    continue;
                }
            }
        }
        if let Some(r) = rest.strip_prefix("[[") {
            if let Some((inner, tail)) = r.split_once("]]") {
                let label = inner.split('|').next_back().unwrap_or(inner);
                let _ = write!(out, r#"<span class="link">{}</span>"#, esc(label));
                rest = tail;
                continue;
            }
        }
        if let Some(r) = rest.strip_prefix('[') {
            if let Some((label, after)) = r.split_once("](") {
                if let Some((url, tail)) = after.split_once(')') {
                    let _ = write!(out, r#"<a href="{}">{}</a>"#, esc(url), inline(label, root));
                    rest = tail;
                    continue;
                }
            }
        }
        let mut matched = false;
        for (mark, tag) in [("**", "strong"), ("__", "strong"), ("`", "code"), ("~~", "del"), ("*", "em"), ("_", "em")] {
            if let Some(r) = rest.strip_prefix(mark) {
                if let Some(end) = r.find(mark).filter(|e| *e > 0) {
                    let inner = &r[..end];
                    let body = if tag == "code" { esc(inner) } else { inline(inner, root) };
                    let _ = write!(out, "<{tag}>{body}</{tag}>");
                    rest = &r[end + mark.len()..];
                    matched = true;
                    break;
                }
            }
        }
        if matched {
            continue;
        }
        // Rien de reconnu à cet endroit : un caractère tel quel
        if let Some(c) = rest.chars().next() {
            out.push_str(&esc(&c.to_string()));
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// Le texte d'une note sans son en-tête YAML
fn body(text: &str) -> &str {
    if let Some(r) = text.strip_prefix("---\n") {
        if let Some(end) = r.find("\n---") {
            return r[end + 4..].trim_start_matches('\n');
        }
    }
    text
}

/// La note en page HTML autonome
pub fn to_html(text: &str, title: &str, root: &std::path::Path) -> String {
    let mut h = String::new();
    let mut in_code = false;
    let mut mermaid = false;
    let mut list: Option<&str> = None;
    let mut para: Vec<String> = vec![];
    let flush = |h: &mut String, para: &mut Vec<String>| {
        if !para.is_empty() {
            let _ = write!(h, "<p>{}</p>\n", para.join("<br>"));
            para.clear();
        }
    };
    let close_list = |h: &mut String, list: &mut Option<&str>| {
        if let Some(t) = list.take() {
            let _ = writeln!(h, "</{t}>");
        }
    };
    let lines: Vec<&str> = body(text).lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();
        if let Some(lang) = t.strip_prefix("```") {
            flush(&mut h, &mut para);
            close_list(&mut h, &mut list);
            let lang = lang.trim();
            // Formule et diagramme : rendus à part
            if !in_code && matches!(lang, "math" | "latex" | "tex" | "mermaid") {
                let mut code = String::new();
                i += 1;
                while i < lines.len() && !lines[i].trim().starts_with("```") {
                    code.push_str(lines[i]);
                    code.push('\n');
                    i += 1;
                }
                if lang == "mermaid" {
                    mermaid = true;
                    let _ = writeln!(h, r#"<pre class="mermaid">{}</pre>"#, esc(&code));
                } else {
                    let _ = writeln!(h, r#"<p class="math">{}</p>"#, esc(&crate::mdextra::tex(code.trim())));
                }
                i += 1;
                continue;
            }
            if in_code {
                h.push_str("</code></pre>\n");
            } else {
                h.push_str("<pre><code>");
            }
            in_code = !in_code;
            i += 1;
            continue;
        }
        if let Some(m) = t.strip_prefix("$$").and_then(|r| r.strip_suffix("$$")) {
            flush(&mut h, &mut para);
            let _ = writeln!(h, r#"<p class="math">{}</p>"#, esc(&crate::mdextra::tex(m.trim())));
            i += 1;
            continue;
        }
        if in_code {
            let _ = writeln!(h, "{}", esc(line));
            i += 1;
            continue;
        }
        if t.is_empty() {
            flush(&mut h, &mut para);
            close_list(&mut h, &mut list);
        } else if let Some(n) = (1..=6).find(|n| t.starts_with(&format!("{} ", "#".repeat(*n)))) {
            flush(&mut h, &mut para);
            close_list(&mut h, &mut list);
            let _ = writeln!(h, "<h{n}>{}</h{n}>", inline(t[n + 1..].trim(), root));
        } else if t == "---" || t == "***" {
            flush(&mut h, &mut para);
            h.push_str("<hr>\n");
        } else if let Some(q) = t.strip_prefix('>') {
            flush(&mut h, &mut para);
            close_list(&mut h, &mut list);
            let q = q.trim();
            // Encadré Obsidian : > [!note] Titre
            if let Some(rest) = q.strip_prefix("[!") {
                let (kind, title) = rest.split_once(']').unwrap_or((rest, ""));
                let _ = writeln!(h, r#"<div class="callout {}"><strong>{}</strong>"#, esc(kind), inline(if title.trim().is_empty() { kind } else { title.trim() }, root));
                while i + 1 < lines.len() && lines[i + 1].trim_start().starts_with('>') {
                    i += 1;
                    let _ = write!(h, "<p>{}</p>", inline(lines[i].trim_start()[1..].trim(), root));
                }
                h.push_str("</div>\n");
            } else {
                let _ = writeln!(h, "<blockquote>{}</blockquote>", inline(q, root));
            }
        } else if t.starts_with('|') && t.ends_with('|') {
            flush(&mut h, &mut para);
            close_list(&mut h, &mut list);
            h.push_str("<table>\n");
            let mut first = true;
            while i < lines.len() && lines[i].trim().starts_with('|') {
                let row = lines[i].trim().trim_matches('|');
                if row.chars().all(|c| matches!(c, '-' | ':' | '|' | ' ')) {
                    i += 1;
                    continue;
                }
                let tag = if first { "th" } else { "td" };
                h.push_str("<tr>");
                for cell in row.split('|') {
                    let _ = write!(h, "<{tag}>{}</{tag}>", inline(cell.trim(), root));
                }
                h.push_str("</tr>\n");
                first = false;
                i += 1;
            }
            h.push_str("</table>\n");
            continue;
        } else if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).or_else(|| t.strip_prefix("+ ")) {
            flush(&mut h, &mut para);
            if list != Some("ul") {
                close_list(&mut h, &mut list);
                h.push_str("<ul>\n");
                list = Some("ul");
            }
            let (check, item) = if let Some(r) = item.strip_prefix("[ ] ") { ("☐ ", r) } else if let Some(r) = item.strip_prefix("[x] ").or_else(|| item.strip_prefix("[X] ")) { ("☑ ", r) } else { ("", item) };
            let _ = writeln!(h, "<li>{check}{}</li>", inline(item, root));
        } else if let Some((num, item)) = t.split_once(". ").filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())) {
            let _ = num;
            flush(&mut h, &mut para);
            if list != Some("ol") {
                close_list(&mut h, &mut list);
                h.push_str("<ol>\n");
                list = Some("ol");
            }
            let _ = writeln!(h, "<li>{}</li>", inline(item, root));
        } else {
            close_list(&mut h, &mut list);
            para.push(inline(t, root));
        }
        i += 1;
    }
    flush(&mut h, &mut para);
    close_list(&mut h, &mut list);
    if in_code {
        h.push_str("</code></pre>\n");
    }
    format!(
        r#"<!DOCTYPE html>
<html lang="fr"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>{}</title>
<style>
body {{ font: 17px/1.6 Georgia, "Times New Roman", serif; color: #1d1b1f; max-width: 680px; margin: 48px auto; padding: 0 24px; }}
h1, h2, h3, h4 {{ font-family: Inter, "Helvetica Neue", Arial, sans-serif; line-height: 1.25; margin: 1.6em 0 .5em; }}
h1 {{ font-size: 2em; }} code, pre {{ font-family: "JetBrains Mono", Menlo, monospace; font-size: .88em; }}
pre {{ background: #f3f1ef; padding: 12px 14px; overflow-x: auto; border-radius: 6px; }}
blockquote {{ border-left: 3px solid #cbbfd0; margin: 1em 0; padding: .2em 1em; color: #5b5560; }}
.callout {{ background: #f6f1f5; border-left: 3px solid #b9789c; padding: .6em 1em; margin: 1em 0; border-radius: 4px; }}
.link {{ color: #8a4f72; }} img {{ max-width: 100%; border-radius: 4px; }}
table {{ border-collapse: collapse; margin: 1em 0; }} th, td {{ border: 1px solid #ddd5d9; padding: 6px 10px; text-align: left; }}
.math {{ text-align: center; font-style: italic; font-size: 1.2em; }} pre.mermaid {{ background: none; text-align: center; }}
</style></head><body>
{h}{}</body></html>
"#,
        esc(title),
        // Les diagrammes Mermaid se dessinent à l'ouverture (avec une connexion Internet) ; sinon le texte reste lisible
        if mermaid { "<script type=\"module\">import mermaid from 'https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs'; mermaid.initialize({ startOnLoad: true });</script>\n" } else { "" }
    )
}

/// Encodage PDF d'un texte (Windows-1252 ; ce qui n'y entre pas devient « ? »)
fn pdf_text(s: &str) -> Vec<u8> {
    let mut out = vec![];
    for c in s.chars() {
        let b: u8 = match c {
            '(' | ')' | '\\' => {
                out.push(b'\\');
                c as u8
            }
            c if (c as u32) < 128 => c as u8,
            c if (0xA0..=0xFF).contains(&(c as u32)) => c as u32 as u8,
            '’' | '‘' => 0x27,
            '“' | '”' => b'"',
            '–' => 0x96,
            '—' => 0x97,
            '…' => 0x85,
            '•' => 0x95,
            '€' => 0x80,
            'œ' => 0x9C,
            'Œ' => 0x8C,
            _ => b'?',
        };
        out.push(b);
    }
    out
}

/// Le texte d'une ligne sans la syntaxe Markdown
fn plain(s: &str) -> String {
    let mut t = s.replace("**", "").replace("__", "").replace('`', "").replace("~~", "");
    // [[lien|alias]] → alias ; [texte](url) → texte
    while let (Some(a), Some(b)) = (t.find("[["), t.find("]]")) {
        if b < a {
            break;
        }
        let inner = t[a + 2..b].to_string();
        let label = inner.split('|').next_back().unwrap_or(&inner).to_string();
        t.replace_range(a..b + 2, &label);
    }
    while let Some(a) = t.find("](") {
        let Some(open) = t[..a].rfind('[') else { break };
        let Some(close) = t[a..].find(')').map(|k| a + k) else { break };
        let label = t[open + 1..a].to_string();
        t.replace_range(open..=close, &label);
    }
    t.replace('*', "").replace('_', " ").trim().to_string()
}

/// La note en PDF A4 simple : titres, paragraphes, listes, citations (Helvetica)
pub fn to_pdf(text: &str, title: &str) -> Vec<u8> {
    let (pw, ph, margin) = (595.0f32, 842.0f32, 64.0f32);
    // Lignes à poser : (taille, gras, retrait, texte)
    // Le titre en tête, sauf si la note commence déjà par lui
    let first = body(text).lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    let mut blocks: Vec<(f32, bool, f32, String)> = if first.trim_start_matches('#').trim() == title.trim() && first.starts_with('#') { vec![] } else { vec![(22.0, true, 0.0, title.to_string()), (8.0, false, 0.0, String::new())] };
    let mut in_code = false;
    for line in body(text).lines() {
        let t = line.trim();
        if t.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            blocks.push((9.5, false, 12.0, line.to_string()));
            continue;
        }
        if t.is_empty() {
            blocks.push((6.0, false, 0.0, String::new()));
        } else if let Some(n) = (1..=6).find(|n| t.starts_with(&format!("{} ", "#".repeat(*n)))) {
            blocks.push((6.0, false, 0.0, String::new()));
            blocks.push(([22.0, 15.0, 13.0, 12.0, 11.5, 11.0][n - 1], true, 0.0, plain(&t[n + 1..])));
        } else if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            let item = item.strip_prefix("[ ] ").map(|r| format!("[ ] {r}")).or_else(|| item.strip_prefix("[x] ").map(|r| format!("[x] {r}"))).unwrap_or_else(|| item.to_string());
            blocks.push((11.0, false, 14.0, format!("\u{2022} {}", plain(&item))));
        } else if let Some(q) = t.strip_prefix('>') {
            blocks.push((11.0, false, 18.0, plain(q.trim().trim_start_matches(|c| c != ']' && q.trim().starts_with("[!")).trim_start_matches(']'))));
        } else {
            blocks.push((11.0, false, 0.0, plain(t)));
        }
    }
    // Coupe en lignes (largeur moyenne d'un caractère d'Helvetica : 0,5 em)
    let mut pages: Vec<Vec<u8>> = vec![];
    let mut cur: Vec<u8> = vec![];
    let mut y = ph - margin;
    for (size, bold, indent, txt) in blocks {
        let width = pw - 2.0 * margin - indent;
        let per = ((width / (size * 0.5)) as usize).max(10);
        let mut lines: Vec<String> = vec![];
        let mut l = String::new();
        for word in txt.split(' ') {
            if l.chars().count() + word.chars().count() + 1 > per && !l.is_empty() {
                lines.push(std::mem::take(&mut l));
            }
            if !l.is_empty() {
                l.push(' ');
            }
            l.push_str(word);
        }
        lines.push(l);
        for ln in lines {
            let lh = size * 1.4;
            if y - lh < margin {
                pages.push(std::mem::take(&mut cur));
                y = ph - margin;
            }
            y -= lh;
            if ln.trim().is_empty() {
                continue;
            }
            cur.extend(format!("BT /{} {size} Tf {} {y:.1} Td (", if bold { "F2" } else { "F1" }, margin + indent).as_bytes());
            cur.extend(pdf_text(&ln));
            cur.extend(b") Tj ET\n");
        }
    }
    pages.push(cur);
    // Le fichier : catalogue, pages, polices, contenus
    let mut objs: Vec<Vec<u8>> = vec![];
    let n = pages.len();
    let kids: Vec<String> = (0..n).map(|k| format!("{} 0 R", 5 + 2 * k)).collect();
    objs.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")).into_bytes());
    objs.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec());
    objs.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_vec());
    for (k, content) in pages.iter().enumerate() {
        objs.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw} {ph}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>", 6 + 2 * k).into_bytes());
        let mut s = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        s.extend(content);
        s.extend(b"\nendstream");
        objs.push(s);
    }
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offs = vec![];
    for (k, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend(format!("{} 0 obj\n", k + 1).as_bytes());
        out.extend(o);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn export_html_pdf() {
        let t = "---\ntags: [a]\n---\n# Titre\nDu **gras** et un [[Lien|alias]].\n\n- [x] fait\n- reste\n\n> [!note] Attention\n> détail\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
        let h = super::to_html(t, "Essai", std::path::Path::new("."));
        assert!(h.contains("<h1>Titre</h1>") && h.contains("<strong>gras</strong>") && h.contains(">alias</span>") && h.contains("☑ fait") && h.contains("class=\"callout note\"") && h.contains("<td>2</td>"), "{h}");
        assert!(!h.contains("tags:"));
        let m = super::to_html("Soit $x^2$, 5 $ ou 6 $.\n\n$$\\pi r^2$$\n\n```mermaid\ngraph TD\nA-->B\n```\n", "M", std::path::Path::new("."));
        assert!(m.contains("<em>x²</em>") && m.contains("5 $ ou 6 $") && m.contains("π r²") && m.contains("class=\"mermaid\"") && m.contains("mermaid.initialize"), "{m}");
        let p = super::to_pdf(t, "Essai");
        assert!(p.starts_with(b"%PDF-1.4") && p.ends_with(b"%%EOF\n"));
        assert_eq!(super::plain("le [[Lien|alias]] et [x](http://a)"), "le alias et x");
    }
}


