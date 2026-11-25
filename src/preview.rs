//! Mode lecture des notes : Markdown rendu (titres, gras, italique, barré, surligné, listes à puces ou
//! numérotées, cases à cocher cliquables, citations et encadrés « > [!note] », tableaux, lignes, images
//! du coffre, code coloré) avec les liens [[Note]] et [texte](https://…) cliquables.
use crate::theme;
use eframe::egui::{self, Color32, RichText};
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|([^\]]+))?\]\]").unwrap());
/// Morceaux d'une ligne : lien de note, lien web, gras, italique, barré, surligné, étiquette
static TOKENS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[\[[^\]]+\]\]|\[([^\]]+)\]\((https?://[^)\s]+)\)|\*\*([^*]+)\*\*|__([^_]+)__|\*([^*\s][^*]*)\*|~~([^~]+)~~|==([^=]+)==|(?:^|\s)(#[\p{L}\p{N}_/-]+)|(https?://[^\s)]+)|\[\^([^\]\s]+)\]").unwrap()
});
static TASK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)[-*+] \[([ xX])\] (.*)$").unwrap());
static BULLET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)[-*+] (.*)$").unwrap());
static NUMBERED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)(\d+)[.)] (.*)$").unwrap());
/// Définition d'une note de bas de page : « [^id]: texte »
static FOOTDEF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[\^([^\]\s]+)\]:\s?(.*)$").unwrap());

thread_local! {
    /// Notes de bas de page du document en cours de rendu : (id, texte), dans l'ordre du document
    static FOOTNOTES: std::cell::RefCell<Vec<(String, String)>> = const { std::cell::RefCell::new(Vec::new()) };
}

static IMAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^!\[\[([^\]|]+)(?:\|(\d+))?\]\]$|^!\[[^\]]*\]\(([^)]+)\)$").unwrap());
static CALLOUT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[!(\w+)\][+-]?\s*(.*)$").unwrap());

/// Ce que demande un clic dans la note
pub enum Out {
    /// Ouvrir une note (ou un fichier du coffre)
    Open(String),
    /// Cocher ou décocher la case de cette ligne (numéro de ligne, depuis 0)
    Toggle(usize),
}

/// Affiche la note et retourne le nom de la note cliquée, s'il y en a une (lecture seule)
pub fn show(ui: &mut egui::Ui, text: &str, existing: &HashSet<String>, paths: &HashSet<String>) -> Option<String> {
    match show_doc(ui, text, existing, paths) {
        Some(Out::Open(n)) => Some(n),
        _ => None,
    }
}

/// Affiche la note ; un clic peut ouvrir un lien ou cocher une case
pub fn show_doc(ui: &mut egui::Ui, text: &str, existing: &HashSet<String>, paths: &HashSet<String>) -> Option<Out> {
    let mut out = None;
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    let ctx = Ctx { existing, paths };
    // Notes de bas de page : leurs définitions sont gardées pour les numéros et la liste du bas
    let notes: Vec<(String, String)> = lines.iter().filter_map(|l| FOOTDEF.captures(l.trim_end())).map(|c| (c[1].to_string(), c[2].to_string())).collect();
    let saved = FOOTNOTES.with(|f| std::mem::replace(&mut *f.borrow_mut(), notes.clone()));
    // En-tête YAML : un tableau de propriétés (comme Obsidian), pas du texte brut
    if let Some((props, n)) = crate::vault::frontmatter(text) {
        properties(ui, &props, &ctx, &mut out);
        i = n;
    }
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        // Définition de note de bas de page : affichée en bas
        if FOOTDEF.is_match(line.trim_end()) {
            i += 1;
            continue;
        }
        // Bloc de code
        if let Some(lang) = trimmed.strip_prefix("```") {
            let mut code = String::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code.push_str(lines[i]);
                code.push('\n');
                i += 1;
            }
            code_block(ui, &code, lang.trim());
            i += 1;
            continue;
        }
        // Tableau : une ligne d'en-tête, une ligne de tirets, puis les rangées
        if trimmed.starts_with('|') && lines.get(i + 1).is_some_and(|l| is_table_rule(l)) {
            let mut rows = vec![cells(trimmed)];
            i += 2;
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                rows.push(cells(lines[i].trim_start()));
                i += 1;
            }
            table(ui, &rows, &ctx, &mut out);
            continue;
        }
        // Citation ou encadré : les lignes « > » qui se suivent
        if trimmed.starts_with('>') {
            let mut body = vec![];
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                body.push(lines[i].trim_start().trim_start_matches('>').trim_start());
                i += 1;
            }
            quote(ui, &body, &ctx, &mut out);
            continue;
        }
        if trimmed.is_empty() {
            ui.add_space(8.0);
        } else if trimmed.chars().all(|c| c == '-' || c == '*' || c == '_' || c == ' ') && trimmed.chars().filter(|c| !c.is_whitespace()).count() >= 3 {
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);
        } else if let Some((level, rest)) = heading(trimmed) {
            ui.add_space(if level == 1 { 12.0 } else { 8.0 });
            let size = match level {
                1 => 26.0,
                2 => 20.0,
                3 => 17.0,
                _ => 15.0,
            };
            ui.horizontal_wrapped(|ui| inline(ui, rest, &ctx, &mut out, Some(size)));
            if level <= 2 {
                let w = ui.available_width();
                let (r, _) = ui.allocate_exact_size(egui::vec2(w, 1.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 0.0, theme::pal().line);
            }
            ui.add_space(2.0);
        } else if let Some(c) = IMAGE.captures(trimmed) {
            let target = c.get(1).or(c.get(3)).map(|m| m.as_str().trim()).unwrap_or("");
            let width = c.get(2).and_then(|w| w.as_str().parse::<f32>().ok());
            match note_target(target, ctx.paths) {
                Some((rel, sec)) => embed(ui, &rel, sec.as_deref(), &ctx, &mut out),
                None => image(ui, target, width, &ctx),
            }
        } else if let Some(c) = TASK.captures(line) {
            let indent = c[1].len() as f32 * 6.0;
            let mut done = &c[2] != " ";
            ui.horizontal_wrapped(|ui| {
                ui.add_space(indent);
                if ui.checkbox(&mut done, "").on_hover_text(crate::i18n::t("cocher dans la note")).changed() {
                    out = Some(Out::Toggle(i));
                }
                if done {
                    ui.scope(|ui| {
                        ui.visuals_mut().override_text_color = Some(theme::pal().muted);
                        inline(ui, &format!("~~{}~~", &c[3]), &ctx, &mut out, None);
                    });
                } else {
                    inline(ui, &c[3], &ctx, &mut out, None);
                }
            });
        } else if let Some(c) = BULLET.captures(line) {
            let depth = c[1].len() / 2;
            ui.horizontal_wrapped(|ui| {
                ui.add_space(depth as f32 * 18.0 + 4.0);
                ui.label(RichText::new(if depth % 2 == 0 { "•  " } else { "◦  " }).color(theme::pal().accent));
                inline(ui, &c[2], &ctx, &mut out, None);
            });
        } else if let Some(c) = NUMBERED.captures(line) {
            let depth = c[1].len() / 2;
            ui.horizontal_wrapped(|ui| {
                ui.add_space(depth as f32 * 18.0 + 2.0);
                ui.label(RichText::new(format!("{}.  ", &c[2])).color(theme::pal().accent));
                inline(ui, &c[3], &ctx, &mut out, None);
            });
        } else {
            ui.horizontal_wrapped(|ui| inline(ui, trimmed, &ctx, &mut out, None));
        }
        i += 1;
    }
    if !notes.is_empty() {
        let p = theme::pal();
        ui.add_space(10.0);
        ui.separator();
        for (k, (_, t)) in notes.iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{}.  ", k + 1)).color(p.muted));
                inline(ui, t, &ctx, &mut out, None);
            });
        }
    }
    FOOTNOTES.with(|f| *f.borrow_mut() = saved);
    out
}

/// Coche ou décoche la case de la ligne `n` ; rend le texte changé
pub fn toggle_task(text: &str, n: usize) -> Option<String> {
    let mut lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
    let l = lines.get_mut(n)?;
    let c = TASK.captures(l)?;
    let mark = c.get(2)?;
    let new = if mark.as_str() == " " { "x" } else { " " };
    l.replace_range(mark.range(), new);
    Some(lines.join("\n"))
}

struct Ctx<'a> {
    existing: &'a HashSet<String>,
    paths: &'a HashSet<String>,
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&level) {
        line[level..].strip_prefix(' ').map(|rest| (level, rest))
    } else {
        None
    }
}

fn is_table_rule(l: &str) -> bool {
    let t = l.trim();
    t.starts_with('|') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) && t.contains('-')
}

fn cells(l: &str) -> Vec<String> {
    let t = l.trim().trim_start_matches('|').trim_end_matches('|');
    t.split('|').map(|c| c.trim().to_string()).collect()
}

fn table(ui: &mut egui::Ui, rows: &[Vec<String>], ctx: &Ctx, out: &mut Option<Out>) {
    let p = theme::pal();
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(1);
    ui.add_space(4.0);
    egui::Frame::new().stroke(egui::Stroke::new(1.0, p.line)).corner_radius(4).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
        egui::Grid::new(ui.next_auto_id()).striped(true).spacing(egui::vec2(18.0, 6.0)).show(ui, |ui| {
            for (k, r) in rows.iter().enumerate() {
                for c in 0..cols {
                    let cell = r.get(c).map(|s| s.as_str()).unwrap_or("");
                    ui.horizontal(|ui| {
                        if k == 0 {
                            inline(ui, &format!("**{cell}**"), ctx, out, None);
                        } else {
                            inline(ui, cell, ctx, out, None);
                        }
                    });
                }
                ui.end_row();
            }
        });
    });
    ui.add_space(4.0);
}

/// Citation, ou encadré Obsidian (« > [!astuce] Titre ») avec sa couleur et son icône
fn quote(ui: &mut egui::Ui, body: &[&str], ctx: &Ctx, out: &mut Option<Out>) {
    let p = theme::pal();
    let callout = body.first().and_then(|l| CALLOUT.captures(l));
    let (color, icon, title, rest) = match &callout {
        Some(c) => {
            let kind = c[1].to_lowercase();
            let (color, icon) = match kind.as_str() {
                "tip" | "astuce" | "hint" => (p.success, crate::lucide::SPARKLES),
                "warning" | "attention" | "caution" => (p.warning, crate::lucide::FLAG),
                "danger" | "error" | "bug" | "erreur" => (p.missing, crate::lucide::X),
                "question" | "faq" => (p.info, crate::lucide::SEARCH),
                "todo" | "afaire" => (p.accent, crate::lucide::CHECK),
                _ => (p.info, crate::lucide::BOOKMARK),
            };
            let title = if c[2].trim().is_empty() {
                let mut k = kind.clone();
                if let Some(f) = k.get_mut(0..1) {
                    f.make_ascii_uppercase();
                }
                k
            } else {
                c[2].trim().to_string()
            };
            (color, Some(icon), Some(title), &body[1..])
        }
        None => (p.accent, None, None, body),
    };
    ui.add_space(4.0);
    let fill = if callout.is_some() { color.gamma_multiply(0.1) } else { Color32::TRANSPARENT };
    let resp = egui::Frame::new().fill(fill).corner_radius(4).inner_margin(egui::Margin { left: 14, right: 10, top: 6, bottom: 6 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if let (Some(icon), Some(title)) = (icon, &title) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon).color(color));
                ui.label(RichText::new(title).strong().color(color));
            });
        }
        for l in rest {
            if l.is_empty() {
                ui.add_space(4.0);
            } else {
                ui.horizontal_wrapped(|ui| {
                    if callout.is_none() {
                        ui.visuals_mut().override_text_color = Some(p.muted);
                    }
                    inline(ui, l, ctx, out, None)
                });
            }
        }
    });
    let r = resp.response.rect;
    ui.painter().rect_filled(egui::Rect::from_min_size(r.min, egui::vec2(3.0, r.height())), 1.0, color);
    ui.add_space(4.0);
}

thread_local! {
    /// Profondeur des notes intégrées : une note qui s'intègre elle-même ne boucle pas
    static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// « ![[Note]] » ou « ![[Note#Titre]] » : la note du coffre visée et la section, si ce n'est pas une image
fn note_target(target: &str, paths: &HashSet<String>) -> Option<(String, Option<String>)> {
    let (name, sec) = match target.split_once('#') {
        Some((n, s)) => (n.trim(), Some(s.trim().to_string()).filter(|s| !s.is_empty())),
        None => (target.trim(), None),
    };
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    if ext.as_deref().is_some_and(|e| e != "md") {
        return None;
    }
    let file = if name.ends_with(".md") { name.to_string() } else { format!("{name}.md") };
    let file = file.trim_start_matches('/');
    let rel = if paths.contains(file) {
        file.to_string()
    } else {
        let low = file.to_lowercase();
        paths.iter().find(|q| q.rsplit('/').next().is_some_and(|n| n.to_lowercase() == low))?.clone()
    };
    Some((rel, sec))
}

/// La partie d'une note sous un titre, jusqu'au titre suivant de même niveau ou plus haut
pub fn section(text: &str, title: &str) -> Option<String> {
    let want = title.trim().to_lowercase();
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|l| heading(l.trim_start()).is_some_and(|(_, t)| t.trim().to_lowercase() == want))?;
    let level = heading(lines[start].trim_start())?.0;
    let end = (start + 1..lines.len()).find(|&i| heading(lines[i].trim_start()).is_some_and(|(l, _)| l <= level)).unwrap_or(lines.len());
    Some(lines[start..end].join("\n"))
}

/// Les propriétés de l'en-tête : une ligne par clé, les listes et les tags en pastilles
fn properties(ui: &mut egui::Ui, props: &[(String, Vec<String>)], ctx: &Ctx, out: &mut Option<Out>) {
    if props.is_empty() {
        return;
    }
    let p = theme::pal();
    egui::Frame::new().stroke(egui::Stroke::new(1.0, p.line)).corner_radius(6).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new(ui.next_auto_id()).num_columns(2).spacing(egui::vec2(18.0, 6.0)).show(ui, |ui| {
            for (k, vals) in props {
                let low = k.to_lowercase();
                let icon = match low.as_str() {
                    "tags" | "tag" => crate::lucide::TAGS,
                    "date" | "created" | "updated" | "créé" | "modifié" | "échéance" | "due" => crate::lucide::CALENDAR,
                    "aliases" | "alias" => crate::lucide::FORWARD,
                    _ if vals.len() > 1 => crate::lucide::LIST,
                    _ if vals.first().is_some_and(|v| v.starts_with("http")) => crate::lucide::LINK,
                    _ => crate::lucide::TEXT_ICON,
                };
                ui.label(RichText::new(format!("{icon}  {k}")).color(p.muted).size(12.5));
                ui.horizontal_wrapped(|ui| {
                    if vals.is_empty() {
                        ui.label(RichText::new("—").color(p.muted));
                    }
                    let tags = low == "tags" || low == "tag";
                    for v in vals {
                        if tags {
                            for t in v.split_whitespace() {
                                let t = t.trim_start_matches('#');
                                ui.label(RichText::new(format!("#{t}")).size(12.0).color(p.link).background_color(p.link.gamma_multiply(0.15)));
                            }
                        } else if vals.len() > 1 {
                            egui::Frame::new().fill(p.code_bg).corner_radius(10).inner_margin(egui::Margin::symmetric(8, 1)).show(ui, |ui| inline(ui, v, ctx, out, None));
                        } else {
                            inline(ui, v, ctx, out, None);
                        }
                    }
                });
                ui.end_row();
            }
        });
    });
    ui.add_space(6.0);
}

/// Sans l'en-tête YAML du début
fn body(text: &str) -> &str {
    if let Some(rest) = text.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            return rest[end + 4..].trim_start_matches('\n');
        }
    }
    text
}

/// Une note intégrée : son contenu dans un cadre, son nom cliquable pour l'ouvrir
fn embed(ui: &mut egui::Ui, rel: &str, sec: Option<&str>, ctx: &Ctx, out: &mut Option<Out>) {
    let p = theme::pal();
    let depth = DEPTH.with(|d| d.get());
    let name = rel.rsplit('/').next().unwrap_or(rel).trim_end_matches(".md");
    let title = match sec {
        Some(s) => format!("{name} › {s}"),
        None => name.to_string(),
    };
    let text = std::fs::read_to_string(std::path::Path::new(&crate::nodes::vault_root()).join(rel)).unwrap_or_default();
    let content = match sec {
        Some(s) => section(body(&text), s),
        None => Some(body(&text).to_string()),
    };
    let frame = egui::Frame::new().fill(p.code_bg.gamma_multiply(0.6)).stroke(egui::Stroke::new(1.0, p.line)).corner_radius(6).inner_margin(egui::Margin { left: 14, right: 10, top: 8, bottom: 10 });
    let resp = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        let r = ui.add(egui::Label::new(RichText::new(format!("{}  {title}", crate::lucide::FILE_TEXT)).small().color(p.link)).sense(egui::Sense::click()));
        if r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(crate::i18n::t("ouvrir la note intégrée")).clicked() {
            *out = Some(Out::Open(name.to_string()));
        }
        ui.add_space(2.0);
        match content {
            _ if depth >= 3 => {
                ui.label(RichText::new(crate::i18n::t("note intégrée trop profondément")).color(p.muted));
            }
            Some(c) => {
                DEPTH.with(|d| d.set(depth + 1));
                // Une case cochée ici viserait une ligne de l'autre note : seuls les liens remontent
                if let Some(Out::Open(n)) = show_doc(ui, &c, ctx.existing, ctx.paths) {
                    *out = Some(Out::Open(n));
                }
                DEPTH.with(|d| d.set(depth));
            }
            None => {
                ui.label(RichText::new(crate::i18n::t("section introuvable")).color(p.missing));
            }
        }
    });
    let r = resp.response.rect;
    ui.painter().rect_filled(egui::Rect::from_min_size(r.min, egui::vec2(3.0, r.height())), egui::CornerRadius { nw: 6, sw: 6, ne: 0, se: 0 }, p.accent);
    ui.add_space(4.0);
}

fn image(ui: &mut egui::Ui, target: &str, width: Option<f32>, ctx: &Ctx) {
    let p = theme::pal();
    let src = if target.starts_with("http://") || target.starts_with("https://") {
        Some(target.to_string())
    } else {
        // Un nom seul (« ![[croquis.png]] ») : on le cherche dans le coffre
        let t = target.trim_start_matches('/');
        let rel = if ctx.paths.contains(t) { Some(t.to_string()) } else { ctx.paths.iter().find(|q| q.rsplit('/').next() == Some(t)).cloned() };
        rel.map(|r| std::path::Path::new(&crate::nodes::vault_root()).join(r).to_string_lossy().to_string())
    };
    match src {
        Some(path) => {
            let max = ui.available_width();
            // La taille vient de l'en-tête du fichier : l'image a sa place avant même d'être chargée
            let (w, h) = image::image_dimensions(&path).map(|(w, h)| (w as f32, h as f32)).unwrap_or((max, max * 0.6));
            let tw = width.unwrap_or(w).min(max);
            let uri = if path.starts_with("http") { path } else { format!("file://{path}") };
            ui.add(egui::Image::new(uri).fit_to_exact_size(egui::vec2(tw, tw * h / w.max(1.0))).corner_radius(4));
        }
        None => {
            ui.label(RichText::new(format!("{}  {target}", crate::lucide::IMAGE)).color(p.missing));
        }
    }
}

fn inline(ui: &mut egui::Ui, text: &str, ctx: &Ctx, out: &mut Option<Out>, size: Option<f32>) {
    ui.spacing_mut().item_spacing.x = 0.0;
    let p = theme::pal();
    let base = |s: &str| {
        let t = RichText::new(s.to_owned());
        match size {
            Some(px) => t.size(px).family(theme::bold()),
            None => t,
        }
    };
    // Les segments impairs entre `accents graves` sont du code
    for (k, seg) in text.split('`').enumerate() {
        if k % 2 == 1 {
            let t = RichText::new(seg).monospace().background_color(p.code_bg);
            if ctx.paths.contains(seg.trim_matches('/')) {
                let r = ui.add(egui::Label::new(t.color(p.link).underline()).sense(egui::Sense::click()));
                if r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(crate::i18n::t("ouvrir")).clicked() {
                    *out = Some(Out::Open(seg.trim_matches('/').to_string()));
                }
            } else {
                ui.label(t.color(p.accent));
            }
            continue;
        }
        let mut last = 0;
        for cap in TOKENS.captures_iter(seg) {
            let m = cap.get(0).unwrap();
            // Une étiquette garde l'espace qui la précède
            let start = if cap.get(8).is_some() { cap.get(8).unwrap().start() } else { m.start() };
            if start > last {
                ui.label(base(&seg[last..start]));
            }
            let whole = m.as_str();
            if whole.starts_with("[[") {
                if let Some(l) = LINK.captures(whole) {
                    let target = l[1].trim().to_string();
                    let shown = l.get(2).map(|a| a.as_str().trim()).unwrap_or(&target).to_string();
                    let color = if ctx.existing.contains(&target.to_lowercase()) { p.link } else { p.missing };
                    let resp = ui.add(egui::Label::new(base(&shown).color(color).underline()).sense(egui::Sense::click()));
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        *out = Some(Out::Open(target));
                    }
                } else {
                    ui.label(base(whole));
                }
            } else if let (Some(label), Some(url)) = (cap.get(1), cap.get(2)) {
                ui.hyperlink_to(base(label.as_str()).color(p.link), url.as_str());
            } else if let Some(t) = cap.get(3).or(cap.get(4)) {
                ui.label(base(t.as_str()).family(theme::bold()).color(p.text));
            } else if let Some(t) = cap.get(5) {
                ui.label(base(t.as_str()).italics());
            } else if let Some(t) = cap.get(6) {
                ui.label(base(t.as_str()).strikethrough());
            } else if let Some(t) = cap.get(7) {
                ui.label(base(t.as_str()).background_color(p.warning.gamma_multiply(0.35)));
            } else if let Some(t) = cap.get(8) {
                ui.label(base(t.as_str()).color(p.accent).background_color(p.accent.gamma_multiply(0.12)));
            } else if let Some(u) = cap.get(9) {
                ui.hyperlink_to(base(u.as_str()).color(p.link), u.as_str());
            } else if let Some(id) = cap.get(10) {
                // Renvoi à une note de bas de page : son numéro en exposant, la note au survol
                let found = FOOTNOTES.with(|f| f.borrow().iter().position(|(n, _)| n == id.as_str()).map(|k| (k + 1, f.borrow()[k].1.clone())));
                match found {
                    Some((n, t)) => {
                        ui.add(egui::Label::new(RichText::new(n.to_string()).small().raised().color(p.link)).sense(egui::Sense::hover())).on_hover_text(t);
                    }
                    None => {
                        ui.label(base(whole).color(p.missing));
                    }
                }
            }
            last = m.end();
        }
        if last < seg.len() {
            ui.label(base(&seg[last..]));
        }
    }
}

fn code_block(ui: &mut egui::Ui, code: &str, lang: &str) {
    let p = theme::pal();
    egui::Frame::new().fill(p.code_bg).stroke(egui::Stroke::new(1.0, p.line)).corner_radius(4).inner_margin(10.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if !lang.is_empty() {
            ui.label(RichText::new(lang).size(10.5).color(p.muted));
        }
        let ext = match lang {
            "python" => "py",
            "rust" => "rs",
            "javascript" | "js" => "js",
            "typescript" | "ts" => "ts",
            "bash" | "shell" | "sh" => "sh",
            "json" => "json",
            "html" => "html",
            "css" => "css",
            other => other,
        };
        let font = egui::FontId::monospace(12.5);
        if !ext.is_empty() && crate::hl::supports(ext) {
            let job = crate::hl::highlight(code.trim_end(), ext, font, &crate::hl::colors(ui.visuals().dark_mode));
            ui.label(job);
        } else {
            ui.label(RichText::new(code.trim_end()).font(font));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cocher_une_case() {
        let t = "# Liste\n- [ ] modéliser\n  - [x] UV\ntexte";
        let a = toggle_task(t, 1).unwrap();
        assert_eq!(a, "# Liste\n- [x] modéliser\n  - [x] UV\ntexte");
        assert_eq!(toggle_task(&a, 2).unwrap(), "# Liste\n- [x] modéliser\n  - [ ] UV\ntexte");
        assert!(toggle_task(t, 0).is_none());
        assert!(is_table_rule("|---|:--:|"));
        assert_eq!(cells("| a | **b** |"), vec!["a", "**b**"]);
        assert!(CALLOUT.captures("[!astuce] Garder le nombre de polygones bas").is_some());
        assert!(IMAGE.captures("![[croquis.png|300]]").is_some());
    }

    #[test]
    fn notes_de_bas_de_page() {
        assert!(FOOTDEF.is_match("[^1]: Une source"));
        assert!(!FOOTDEF.is_match("texte [^1] suite"));
        let c = TOKENS.captures("voir[^src] ici").unwrap();
        assert_eq!(c.get(10).map(|m| m.as_str()), Some("src"));
        // Un lien Markdown n'est pas une note
        assert!(TOKENS.captures("[a](https://x.org)").unwrap().get(10).is_none());
    }

    #[test]
    fn notes_integrees() {
        let paths: HashSet<String> = ["notes/Idées.md".to_string(), "refs/croquis.png".to_string()].into_iter().collect();
        assert_eq!(note_target("Idées", &paths), Some(("notes/Idées.md".into(), None)));
        assert_eq!(note_target("idées#Couleurs", &paths), Some(("notes/Idées.md".into(), Some("Couleurs".into()))));
        assert_eq!(note_target("croquis.png", &paths), None);
        let t = "# Idées\nintro\n## Couleurs\nrouge\n### Nuances\nvermillon\n## Formes\nrond";
        assert_eq!(section(t, "couleurs").unwrap(), "## Couleurs\nrouge\n### Nuances\nvermillon");
        assert!(section(t, "absent").is_none());
        assert_eq!(body("---\ntags: x\n---\nTexte"), "Texte");
    }
}
