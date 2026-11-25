//! Scénario au format Fountain (.fountain) : lecture en mise en page de scénario, minutage estimé,
//! personnages et séquences, dépouillement en notes, export PDF (Courier 12, page US Letter).

use eframe::egui::{self, RichText};

pub fn is_fountain(rel: &str) -> bool {
    let l = rel.to_lowercase();
    l.ends_with(".fountain") || l.ends_with(".spmd")
}

#[derive(Clone, Debug, PartialEq)]
pub enum El {
    /// Page de titre : (clé, valeur)
    Title(String, String),
    Scene(String),
    Action(String),
    Character(String),
    Parenthetical(String),
    Dialogue(String),
    Transition(String),
    Centered(String),
    Section(usize, String),
    Synopsis(String),
    PageBreak,
}

fn is_scene(l: &str) -> bool {
    let u = l.trim_start().to_uppercase();
    (l.starts_with('.') && !l.starts_with("..")) || ["INT.", "EXT.", "INT/EXT", "INT./EXT", "EST.", "I/E", "INT ", "EXT "].iter().any(|p| u.starts_with(p))
}

fn is_upper(l: &str) -> bool {
    let letters: Vec<char> = l.chars().filter(|c| c.is_alphabetic()).collect();
    !letters.is_empty() && letters.iter().all(|c| !c.is_lowercase())
}

/// Retire les notes [[…]] et les commentaires /* … */.
fn strip_notes(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    loop {
        let a = rest.find("[[");
        let b = rest.find("/*");
        let (start, end_pat) = match (a, b) {
            (Some(x), Some(y)) if y < x => (y, "*/"),
            (Some(x), _) => (x, "]]"),
            (None, Some(y)) => (y, "*/"),
            (None, None) => {
                out.push_str(rest);
                return out;
            }
        };
        out.push_str(&rest[..start]);
        match rest[start..].find(end_pat) {
            Some(e) => rest = &rest[start + e + 2..],
            None => return out,
        }
    }
}

pub fn parse(src: &str) -> Vec<El> {
    let text = strip_notes(&src.replace("\r\n", "\n"));
    let lines: Vec<&str> = text.lines().collect();
    let mut out = vec![];
    let mut i = 0;
    // Page de titre : des « Clé: valeur » au tout début
    if lines.first().is_some_and(|l| l.contains(':') && !is_scene(l)) {
        while i < lines.len() && !lines[i].trim().is_empty() {
            let l = lines[i];
            match l.split_once(':') {
                Some((k, v)) if !l.starts_with([' ', '\t']) => out.push(El::Title(k.trim().to_string(), v.trim().to_string())),
                _ => {
                    if let Some(El::Title(_, v)) = out.last_mut() {
                        if !v.is_empty() {
                            v.push('\n');
                        }
                        v.push_str(l.trim());
                    }
                }
            }
            i += 1;
        }
    }
    while i < lines.len() {
        let raw = lines[i];
        let l = raw.trim();
        let blank_before = i == 0 || lines[i - 1].trim().is_empty();
        if l.is_empty() {
            i += 1;
            continue;
        }
        if l == "===" {
            out.push(El::PageBreak);
        } else if let Some(h) = l.strip_prefix('#') {
            let depth = 1 + h.chars().take_while(|c| *c == '#').count();
            out.push(El::Section(depth, h.trim_start_matches('#').trim().to_string()));
        } else if let Some(s) = l.strip_prefix('=') {
            out.push(El::Synopsis(s.trim().to_string()));
        } else if l.starts_with('>') && l.ends_with('<') {
            out.push(El::Centered(l[1..l.len() - 1].trim().to_string()));
        } else if let Some(t) = l.strip_prefix('>') {
            out.push(El::Transition(t.trim().to_string()));
        } else if blank_before && is_scene(l) {
            out.push(El::Scene(l.trim_start_matches('.').trim().to_string()));
        } else if blank_before && is_upper(l) && l.ends_with("TO:") {
            out.push(El::Transition(l.to_string()));
        } else if blank_before && (l.starts_with('@') || (is_upper(l) && i + 1 < lines.len() && !lines[i + 1].trim().is_empty())) && !l.starts_with('!') {
            // Personnage, puis ses répliques jusqu'à la ligne vide
            out.push(El::Character(l.trim_start_matches('@').trim().to_string()));
            i += 1;
            while i < lines.len() && !lines[i].trim().is_empty() {
                let d = lines[i].trim();
                if d.starts_with('(') && d.ends_with(')') {
                    out.push(El::Parenthetical(d.to_string()));
                } else {
                    match out.last_mut() {
                        Some(El::Dialogue(prev)) => {
                            prev.push('\n');
                            prev.push_str(d);
                        }
                        _ => out.push(El::Dialogue(d.to_string())),
                    }
                }
                i += 1;
            }
            continue;
        } else {
            let a = l.trim_start_matches('!').to_string();
            match (out.last_mut(), blank_before) {
                (Some(El::Action(prev)), false) => {
                    prev.push('\n');
                    prev.push_str(&a);
                }
                _ => out.push(El::Action(a)),
            }
        }
        i += 1;
    }
    out
}

/// Une ligne de la page mise en forme : (marge gauche en caractères, texte, gras)
type Line = (usize, String, bool);

fn wrap(s: &str, width: usize) -> Vec<String> {
    let mut out = vec![];
    for para in s.lines() {
        let mut cur = String::new();
        for w in para.split_whitespace() {
            if !cur.is_empty() && cur.chars().count() + 1 + w.chars().count() > width {
                out.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(w);
        }
        out.push(cur);
    }
    out
}

/// Mise en page standard : 55 lignes par page, marges du format américain (en colonnes de Courier 12).
pub fn layout(els: &[El]) -> Vec<Vec<Line>> {
    let mut pages: Vec<Vec<Line>> = vec![vec![]];
    let mut push = |block: Vec<Line>, gap: bool| {
        let page = pages.last_mut().unwrap();
        let need = block.len() + usize::from(gap && !page.is_empty());
        if page.len() + need > 55 && !page.is_empty() {
            pages.push(vec![]);
        }
        let page = pages.last_mut().unwrap();
        if gap && !page.is_empty() {
            page.push((0, String::new(), false));
        }
        page.extend(block);
    };
    let mut scene_no = 0;
    for e in els {
        match e {
            El::Title(..) | El::Section(..) | El::Synopsis(..) => {}
            El::PageBreak => push(vec![(usize::MAX, String::new(), false)], false),
            El::Scene(s) => {
                scene_no += 1;
                push(vec![(0, format!("{scene_no}  {}", s.to_uppercase()), true)], true);
            }
            El::Action(s) => push(wrap(s, 60).into_iter().map(|l| (0, l, false)).collect(), true),
            El::Character(s) => push(vec![(22, s.to_uppercase(), false)], true),
            El::Parenthetical(s) => push(wrap(s, 25).into_iter().map(|l| (16, l, false)).collect(), false),
            El::Dialogue(s) => push(wrap(s, 35).into_iter().map(|l| (10, l, false)).collect(), false),
            El::Transition(s) => push(vec![(45, s.to_uppercase(), false)], true),
            El::Centered(s) => push(wrap(s, 60).into_iter().map(|l| (30usize.saturating_sub(l.chars().count() / 2), l, false)).collect(), true),
        }
    }
    // Saut de page forcé : le marqueur commence une nouvelle page
    let mut out: Vec<Vec<Line>> = vec![];
    for p in pages {
        let mut cur: Vec<Line> = vec![];
        for l in p {
            if l.0 == usize::MAX {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            } else {
                cur.push(l);
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
    }
    out
}

pub struct Stats {
    pub pages: usize,
    /// Minutes estimées (une page ≈ une minute)
    pub minutes: f32,
    pub scenes: Vec<String>,
    /// Personnages et leur nombre de répliques, du plus présent au moins présent
    pub characters: Vec<(String, usize)>,
}

pub fn stats(els: &[El]) -> Stats {
    let pages = layout(els);
    let lines: usize = pages.iter().map(|p| p.len()).sum();
    let minutes = if pages.is_empty() { 0.0 } else { (pages.len() - 1) as f32 + pages.last().unwrap().len() as f32 / 55.0 };
    let _ = lines;
    let scenes = els.iter().filter_map(|e| if let El::Scene(s) = e { Some(s.clone()) } else { None }).collect();
    let mut chars: Vec<(String, usize)> = vec![];
    for e in els {
        if let El::Character(c) = e {
            // « LÉA (V.O.) » et « LÉA » sont la même personne
            let name = c.split('(').next().unwrap_or(c).trim().trim_end_matches('^').trim().to_uppercase();
            match chars.iter_mut().find(|x| x.0 == name) {
                Some(x) => x.1 += 1,
                None => chars.push((name, 1)),
            }
        }
    }
    chars.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Stats { pages: pages.len(), minutes, scenes, characters: chars }
}

/// Le scénario mis en page, page par page, dans l'éditeur (mode lecture).
pub fn show(ui: &mut egui::Ui, src: &str) -> Option<u8> {
    let p = crate::theme::pal();
    let els = parse(src);
    let st = stats(&els);
    let mut action = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{} {} · ≈ {:.0} min · {} {} · {} {}", st.pages, crate::i18n::t_mid(if st.pages == 1 { "page" } else { "pages" }), st.minutes.max(if st.pages > 0 { 1.0 } else { 0.0 }), st.scenes.len(), crate::i18n::t_mid("séquences"), st.characters.len(), crate::i18n::t_mid("personnages"))).size(11.5).color(p.muted));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(format!("{}  PDF", crate::lucide::DOWNLOAD)).on_hover_text(crate::i18n::t("exporter le scénario en PDF (Courier 12, mise en page standard)")).clicked() {
                action = Some(1);
            }
            if ui.button(format!("{}  {}", crate::lucide::USERS, crate::i18n::t("dépouillement"))).on_hover_text(crate::i18n::t("Crée une note par personnage et par décor (s'ils n'existent pas), liées au scénario")).clicked() {
                action = Some(2);
            }
        });
    });
    if !st.characters.is_empty() {
        ui.label(RichText::new(st.characters.iter().map(|(n, k)| format!("{n} ({k})")).collect::<Vec<_>>().join(" · ")).size(11.0).color(p.muted));
    }
    ui.add_space(8.0);
    let titles: Vec<(String, String)> = els.iter().filter_map(|e| if let El::Title(k, v) = e { Some((k.clone(), v.clone())) } else { None }).collect();
    let fs = 13.0;
    let cw = ui.fonts_mut(|f| f.glyph_width(&egui::FontId::monospace(fs), 'M'));
    let page_w = cw * 62.0 + 80.0;
    let page = |ui: &mut egui::Ui, add: &mut dyn FnMut(&mut egui::Ui)| {
        egui::Frame::new().fill(egui::Color32::from_rgb(252, 251, 247)).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(40, 36)).show(ui, |ui| {
            ui.set_width(page_w - 80.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| add(ui));
        });
        ui.add_space(14.0);
    };
    let ink = egui::Color32::from_rgb(24, 24, 28);
    if !titles.is_empty() {
        page(ui, &mut |ui| {
            ui.add_space(120.0);
            ui.vertical_centered(|ui| {
                for (k, v) in &titles {
                    let big = k.eq_ignore_ascii_case("title") || k.eq_ignore_ascii_case("titre");
                    ui.label(RichText::new(v).monospace().size(if big { fs + 5.0 } else { fs }).color(ink).strong());
                    ui.add_space(10.0);
                }
            });
            ui.add_space(120.0);
        });
    }
    for (n, lines) in layout(&els).iter().enumerate() {
        page(ui, &mut |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.label(RichText::new(format!("{}.", n + 1)).monospace().size(fs).color(ink));
            });
            for (indent, text, bold) in lines {
                let t = format!("{}{}", " ".repeat(*indent), text);
                let mut rt = RichText::new(if t.is_empty() { " ".to_string() } else { t }).monospace().size(fs).color(ink);
                if *bold {
                    rt = rt.strong();
                }
                ui.add(egui::Label::new(rt).wrap_mode(egui::TextWrapMode::Extend));
            }
        });
    }
    action
}

// ---------- PDF ----------

/// Encode du texte pour une chaîne PDF en WinAnsi (le Latin-1 couvre les accents français).
fn pdf_str(s: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    for c in s.chars() {
        let b = match c {
            '’' | '‘' => b'\'',
            '“' | '”' | '«' | '»' => b'"',
            '—' | '–' => b'-',
            '…' => b'.',
            c if (c as u32) < 256 => c as u32 as u8,
            _ => b'?',
        };
        if matches!(b, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(b);
    }
    out.push(b')');
    out
}

/// Le scénario en PDF : Courier 12 (police de base du PDF, rien à embarquer), page Letter, 55 lignes par page.
pub fn pdf(src: &str) -> Vec<u8> {
    let els = parse(src);
    let titles: Vec<(String, String)> = els.iter().filter_map(|e| if let El::Title(k, v) = e { Some((k.clone(), v.clone())) } else { None }).collect();
    let mut pages: Vec<Vec<u8>> = vec![];
    // 72 points par pouce ; marge gauche 1,5 po, haut 1 po ; 12 pt par ligne ; 7,2 pt par caractère
    let line = |c: &mut Vec<u8>, x: f32, y: f32, s: &str, bold: bool| {
        c.extend(format!("BT /{} 12 Tf {x:.1} {y:.1} Td ", if bold { "F2" } else { "F1" }).as_bytes());
        c.extend(pdf_str(s));
        c.extend(b" Tj ET\n");
    };
    if !titles.is_empty() {
        let mut c = vec![];
        let mut y = 500.0;
        for (_, v) in &titles {
            for l in v.lines() {
                let x = 306.0 - l.chars().count() as f32 * 3.6;
                line(&mut c, x, y, l, false);
                y -= 14.0;
            }
            y -= 14.0;
        }
        pages.push(c);
    }
    for (n, lines) in layout(&els).iter().enumerate() {
        let mut c = vec![];
        line(&mut c, 522.0, 756.0, &format!("{}.", n + 1), false);
        for (k, (indent, text, bold)) in lines.iter().enumerate() {
            if !text.is_empty() {
                line(&mut c, 108.0 + *indent as f32 * 7.2, 720.0 - k as f32 * 12.0, text, *bold);
            }
        }
        pages.push(c);
    }
    // Objets : 1 catalogue, 2 pages, 3 police, 4 police grasse, puis (page, contenu) par page
    let mut objs: Vec<Vec<u8>> = vec![];
    let n = pages.len();
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 5 + i * 2)).collect();
    objs.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")).into_bytes());
    objs.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>".to_vec());
    objs.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier-Bold /Encoding /WinAnsiEncoding >>".to_vec());
    for (i, c) in pages.iter().enumerate() {
        objs.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>", 6 + i * 2).into_bytes());
        let mut s = format!("<< /Length {} >>\nstream\n", c.len()).into_bytes();
        s.extend(c);
        s.extend(b"\nendstream");
        objs.push(s);
    }
    let mut out = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offs = vec![];
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).as_bytes());
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

/// Dépouillement : une note par personnage et par décor, dans des dossiers à côté du scénario.
/// Renvoie le nombre de notes créées.
pub fn breakdown(root: &str, rel: &str, src: &str) -> Result<usize, String> {
    let els = parse(src);
    let st = stats(&els);
    let dir = std::path::Path::new(rel).parent().map(|d| d.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    let join = |sub: &str, name: &str| if dir.is_empty() { format!("{sub}/{name}.md") } else { format!("{dir}/{sub}/{name}.md") };
    let script = std::path::Path::new(rel).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let clean = |s: &str| s.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c }).collect::<String>().trim().to_string();
    let mut made = 0;
    let mut write = |r: String, text: String| -> Result<(), String> {
        let p = std::path::Path::new(root).join(&r);
        if p.exists() {
            return Ok(());
        }
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        crate::vault::write(root, &r, &text)?;
        made += 1;
        Ok(())
    };
    for (name, n) in &st.characters {
        let title = clean(&capitalize_words(name));
        write(join("Personnages", &title), format!("---\ntype: personnage\nrépliques: {n}\n---\n# {title}\n\nDans [[{script}]].\n\n## Personnalité\n- \n\n## Références visuelles\n- \n"))?;
    }
    let mut places: Vec<String> = vec![];
    for s in &st.scenes {
        // « INT. CUISINE - JOUR » → « CUISINE »
        let body = s.split_once(['.', ' ']).map(|(_, b)| b).unwrap_or(s).trim_start_matches(['.', ' ']);
        let place = body.split(" - ").next().unwrap_or(body).split(" – ").next().unwrap_or(body).trim().to_string();
        if !place.is_empty() && !places.contains(&place) {
            places.push(place);
        }
    }
    for place in places {
        let title = clean(&capitalize_words(&place));
        write(join("Décors", &title), format!("---\ntype: décor\n---\n# {title}\n\nDans [[{script}]].\n\n## Références\n- \n"))?;
    }
    Ok(made)
}

/// « LÉA DUPONT » → « Léa Dupont »
fn capitalize_words(s: &str) -> String {
    s.split(' ').map(|w| {
        let mut c = w.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
            None => String::new(),
        }
    }).collect::<Vec<_>>().join(" ")
}

pub const STARTER: &str = "Title: Mon court métrage\nCredit: écrit par\nAuthor: \nDraft date: \n\nFADE IN:\n\nEXT. VILLAGE - AUBE\n\nLa brume se lève sur les toits. Une lanterne brille encore à une fenêtre.\n\nLÉA\n(à voix basse)\nIl est déjà parti ?\n\nMARC (V.O.)\nDepuis longtemps.\n\nCUT TO:\n\nINT. CUISINE - JOUR\n\nLéa pose la lanterne sur la table.\n\n> FIN <\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fountain() {
        let els = parse(STARTER);
        assert!(matches!(&els[0], El::Title(k, _) if k == "Title"));
        assert!(els.contains(&El::Scene("EXT. VILLAGE - AUBE".into())));
        assert!(els.contains(&El::Character("LÉA".into())));
        assert!(els.contains(&El::Parenthetical("(à voix basse)".into())));
        assert!(els.contains(&El::Dialogue("Il est déjà parti ?".into())));
        assert!(els.contains(&El::Transition("CUT TO:".into())));
        assert!(els.contains(&El::Centered("FIN".into())));
        let st = stats(&els);
        assert_eq!(st.scenes.len(), 2);
        assert_eq!(st.characters.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), vec!["LÉA", "MARC"]);
        assert_eq!(st.pages, 1);
        let p = pdf(STARTER);
        assert!(p.starts_with(b"%PDF") && p.ends_with(b"%%EOF\n"));
        // Les notes [[…]] disparaissent
        assert_eq!(parse("Il marche. [[à couper]] Vite."), vec![El::Action("Il marche.  Vite.".into())]);
        let dir = std::env::temp_dir().join(format!("aryadne-fountain-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.to_string_lossy().to_string();
        assert_eq!(breakdown(&root, "Scénario.fountain", STARTER).unwrap(), 4);
        assert!(dir.join("Décors/Village.md").exists() && dir.join("Personnages/Léa.md").exists());
        assert_eq!(breakdown(&root, "Scénario.fountain", STARTER).unwrap(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
