//! Mode lecture : rendu Markdown simple, avec liens [[Note]] cliquables.
use crate::theme;
use eframe::egui::{self, RichText};
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

static LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|([^\]]+))?\]\]").unwrap());

/// Affiche la note et retourne le nom de la note cliquée, s'il y en a une.
pub fn show(ui: &mut egui::Ui, text: &str, existing: &HashSet<String>) -> Option<String> {
    let mut clicked = None;
    let mut in_code = false;
    let mut code = String::new();

    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            if in_code {
                code_block(ui, &code);
                code.clear();
            }
            in_code = !in_code;
            continue;
        }
        if in_code {
            code.push_str(line);
            code.push('\n');
            continue;
        }

        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            ui.add_space(8.0);
        } else if let Some((level, rest)) = heading(trimmed) {
            ui.add_space(if level == 1 { 10.0 } else { 6.0 });
            let size = match level { 1 => 26.0, 2 => 19.0, _ => 16.0 };
            ui.horizontal_wrapped(|ui| inline(ui, rest, existing, &mut clicked, Some(size)));
            if level == 1 {
                let w = ui.available_width();
                let (r, _) = ui.allocate_exact_size(egui::vec2(w, 1.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 0.0, theme::pal().line);
            }
        } else if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("  ▪  ").color(theme::pal().accent));
                inline(ui, rest, existing, &mut clicked, None);
            });
        } else if let Some(rest) = trimmed.strip_prefix("> ") {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(" ▌ ").color(theme::pal().accent));
                inline(ui, rest, existing, &mut clicked, None);
            });
        } else {
            ui.horizontal_wrapped(|ui| inline(ui, trimmed, existing, &mut clicked, None));
        }
    }
    if in_code && !code.is_empty() {
        code_block(ui, &code);
    }
    clicked
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&level) {
        line[level..].strip_prefix(' ').map(|rest| (level, rest))
    } else {
        None
    }
}

fn inline(
    ui: &mut egui::Ui,
    text: &str,
    existing: &HashSet<String>,
    clicked: &mut Option<String>,
    size: Option<f32>,
) {
    ui.spacing_mut().item_spacing.x = 0.0;
    let styled = |s: &str| {
        let t = RichText::new(s.to_owned());
        match size { Some(px) => t.size(px).family(theme::bold()), None => t }
    };
    let mut last = 0;
    for cap in LINK.captures_iter(text) {
        let m = cap.get(0).unwrap();
        if m.start() > last {
            ui.label(styled(&text[last..m.start()]));
        }
        let target = cap[1].trim().to_string();
        let shown = cap.get(2).map(|a| a.as_str().trim()).unwrap_or(&target).to_string();
        let color = if existing.contains(&target.to_lowercase()) { theme::pal().link } else { theme::pal().missing };
        let resp = ui.add(egui::Label::new(styled(&shown).color(color).underline()).sense(egui::Sense::click()));
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            *clicked = Some(target);
        }
        last = m.end();
    }
    if last < text.len() {
        ui.label(styled(&text[last..]));
    }
}

fn code_block(ui: &mut egui::Ui, code: &str) {
    egui::Frame::new()
        .fill(theme::pal().code_bg)
        .stroke(egui::Stroke::new(1.0, theme::pal().line))
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(code.trim_end()).monospace());
        });
}
