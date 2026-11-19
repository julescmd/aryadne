#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod preview;
mod theme;
mod vault;

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const APP: &str = "aryadne";

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Aryadne")
            .with_app_id(APP)
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(APP, options, Box::new(|cc| Ok(Box::new(App::new(&cc.egui_ctx)))))
}

/// Dernier coffre ouvert, gardé dans le dossier de config de l'utilisateur (aucun admin requis).
fn config_file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP).join("last_vault"))
}

fn theme_file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP).join("theme"))
}

fn save_config(file: Option<PathBuf>, value: &str) {
    if let Some(f) = file {
        let _ = std::fs::create_dir_all(f.parent().unwrap());
        let _ = std::fs::write(f, value);
    }
}

/// Petit libellé en majuscules, utilisé pour les titres de section.
fn caps(text: &str) -> RichText {
    RichText::new(text.to_uppercase()).size(11.0).color(theme::pal().muted).family(theme::bold())
}

#[derive(Default)]
struct App {
    vault: Option<String>,
    notes: Vec<vault::NoteInfo>,
    current: Option<String>,
    text: String,
    dirty_since: Option<Instant>,
    preview: bool,
    filter: String,
    backlinks: Vec<vault::Backlink>,
    error: Option<String>,
    /// Sélecteur rapide (Ctrl+P) : texte tapé et ligne choisie.
    switcher: Option<(String, usize)>,
}

impl App {
    fn new(ctx: &egui::Context) -> Self {
        let dark = theme_file().and_then(|f| std::fs::read_to_string(f).ok()).is_some_and(|t| t.trim() == "dark");
        theme::install_fonts(ctx);
        theme::apply(ctx, dark);
        let mut app = Self::default();
        if let Some(p) = config_file().and_then(|f| std::fs::read_to_string(f).ok()) {
            app.load_vault(p.trim().to_string());
        }
        app
    }

    fn report<T>(&mut self, r: Result<T, String>) -> Option<T> {
        r.map_err(|e| self.error = Some(e)).ok()
    }

    fn load_vault(&mut self, path: String) {
        self.flush();
        if self.report(vault::open_vault(path.clone())).is_none() {
            return;
        }
        save_config(config_file(), &path);
        self.vault = Some(path);
        self.refresh();
        self.current = None;
        if let Some(first) = self.notes.first().map(|n| n.name.clone()) {
            self.open_note(first);
        }
    }

    fn refresh(&mut self) {
        if let Some(v) = self.vault.clone() {
            if let Some(n) = self.report(vault::list_notes(v)) {
                self.notes = n;
            }
        }
    }

    fn flush(&mut self) {
        if self.dirty_since.take().is_none() {
            return;
        }
        if let (Some(v), Some(n)) = (self.vault.clone(), self.current.clone()) {
            let r = vault::write_note(v, n, self.text.clone());
            self.report(r);
        }
    }

    /// Ouvre une note, en la créant si elle n'existe pas (clic sur un lien [[ ]]).
    fn open_note(&mut self, name: String) {
        self.flush();
        let Some(v) = self.vault.clone() else { return };
        if self.report(vault::create_note(v.clone(), name.clone())).is_none() {
            return;
        }
        self.refresh();
        if let Some(t) = self.report(vault::read_note(v.clone(), name.clone())) {
            self.text = t;
            self.backlinks = vault::backlinks(v, name.clone()).unwrap_or_default();
            self.current = Some(name);
        }
    }

    fn new_note(&mut self) {
        let taken: HashSet<_> = self.notes.iter().map(|n| n.name.to_lowercase()).collect();
        let mut name = "Sans titre".to_string();
        let mut i = 2;
        while taken.contains(&name.to_lowercase()) {
            name = format!("Sans titre {i}");
            i += 1;
        }
        self.open_note(name);
        self.preview = false;
    }

    fn pick_vault(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().set_title("Choisir le dossier du coffre").pick_folder() {
            self.load_vault(dir.to_string_lossy().to_string());
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let pressed = |k| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, k)));
        if self.vault.is_none() {
            return;
        }
        if pressed(Key::P) {
            self.switcher = Some((String::new(), 0));
        }
        if pressed(Key::N) {
            self.new_note();
        }
        if pressed(Key::E) {
            self.preview = !self.preview;
        }
        if pressed(Key::S) {
            self.flush();
        }
    }

    fn topbar(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        egui::Panel::top("top")
            .exact_size(44.0)
            .frame(egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(14, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 0.0, p.accent);
                    ui.label(RichText::new("ARYADNE").family(theme::bold()).size(14.0));
                    ui.add_space(6.0);
                    let path = self.vault.clone().unwrap_or_default();
                    let short = path.split(['/', '\\']).filter(|s| !s.is_empty()).last().unwrap_or("").to_string();
                    if ui.add(egui::Button::new(RichText::new(short).color(p.muted)).frame(false)).on_hover_text(format!("{path}\nChanger de coffre")).clicked() {
                        self.pick_vault();
                    }
                    ui.add_space(24.0);
                    for (label, active) in [("Notes", true), ("Refs", false), ("Boards", false)] {
                        let color = if active { p.text } else { p.muted };
                        let r = ui.add(egui::Label::new(RichText::new(label).color(color).family(theme::bold())).sense(egui::Sense::hover()));
                        if active {
                            let rect = egui::Rect::from_min_max(egui::pos2(r.rect.left(), ui.max_rect().bottom() - 2.0), egui::pos2(r.rect.right(), ui.max_rect().bottom()));
                            ui.painter().rect_filled(rect, 0.0, p.accent);
                        } else {
                            r.on_hover_text("Bientôt");
                        }
                        ui.add_space(10.0);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if theme::is_dark() { "Mode clair" } else { "Mode sombre" };
                        if ui.button(label).clicked() {
                            let dark = !theme::is_dark();
                            theme::apply(ui.ctx(), dark);
                            save_config(theme_file(), if dark { "dark" } else { "light" });
                        }
                    });
                });
            });
    }

    fn statusbar(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        egui::Panel::bottom("status")
            .exact_size(26.0)
            .frame(egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(0, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    let (mode, color) = if self.preview { ("LECTURE", p.link) } else { ("ÉDITION", p.accent) };
                    egui::Frame::new().fill(color).inner_margin(egui::Margin::symmetric(10, 5)).show(ui, |ui| {
                        ui.label(RichText::new(mode).color(p.on_accent).family(theme::bold()).size(11.0));
                    });
                    if let Some(n) = &self.current {
                        ui.label(RichText::new(format!("{n}.md")).size(12.0));
                        let words = self.text.split_whitespace().count();
                        ui.label(RichText::new(format!("{words} mots")).color(p.muted).size(12.0));
                        let saved = if self.dirty_since.is_some() { "Non sauvé" } else { "Sauvé" };
                        ui.label(RichText::new(saved).color(p.muted).size(12.0));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(10.0);
                        ui.label(RichText::new("Ctrl+P ouvrir   Ctrl+N nouvelle   Ctrl+E lecture").color(p.muted).size(11.0));
                    });
                });
            });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        egui::Panel::left("notes")
            .resizable(true)
            .default_size(240.0)
            .min_size(170.0)
            .frame(egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(12))
            .show(ui, |ui| {
                let full = ui.available_width();
                let new_btn = egui::Button::new(RichText::new("+  Nouvelle note").color(p.on_accent).family(theme::bold()))
                    .fill(p.accent)
                    .stroke(egui::Stroke::NONE)
                    .min_size(egui::vec2(full, 32.0));
                if ui.add(new_btn).on_hover_text("Ctrl+N").clicked() {
                    self.new_note();
                }
                ui.add_space(8.0);
                egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Rechercher").frame(egui::Frame::NONE).margin(egui::Margin::ZERO).vertical_align(egui::Align::Center).min_size(egui::vec2(0.0, 18.0)).desired_width(f32::INFINITY));
                });
                ui.add_space(14.0);
                ui.label(caps(&format!("Notes · {}", self.notes.len())));
                ui.add_space(2.0);
                let f = self.filter.to_lowercase();
                let mut open = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for n in self.notes.iter().filter(|n| n.name.to_lowercase().contains(&f)) {
                        let active = self.current.as_deref() == Some(&n.name);
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::click());
                        let painter = ui.painter();
                        if active {
                            painter.rect_filled(rect, 0.0, p.bg);
                            painter.rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())), 0.0, p.accent);
                        } else if resp.hovered() {
                            painter.rect_filled(rect, 0.0, p.line.linear_multiply(0.5));
                        }
                        let galley = painter.layout(
                            n.name.clone(),
                            egui::FontId::proportional(13.5),
                            if active { p.text } else { p.muted },
                            rect.width() - 20.0,
                        );
                        painter.galley(egui::pos2(rect.left() + 12.0, rect.center().y - galley.size().y / 2.0), galley, p.text);
                        if resp.clicked() {
                            open = Some(n.name.clone());
                        }
                    }
                });
                if let Some(n) = open {
                    self.open_note(n);
                }
            });
    }

    fn backlinks_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        egui::Panel::right("backlinks")
            .resizable(true)
            .default_size(260.0)
            .min_size(170.0)
            .frame(egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(12))
            .show(ui, |ui| {
                ui.label(caps(&format!("Liens entrants · {}", self.backlinks.len())));
                ui.add_space(6.0);
                if self.backlinks.is_empty() {
                    ui.label(RichText::new("Aucune note ne pointe ici.").color(p.muted).size(12.5));
                }
                let mut open = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for b in &self.backlinks {
                        let r = egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(10).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(&b.name).family(theme::bold()));
                            ui.label(RichText::new(&b.excerpt).color(p.muted).size(12.0));
                        });
                        let resp = r.response.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                        if resp.hovered() {
                            ui.painter().rect_stroke(resp.rect, 0.0, egui::Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
                        }
                        if resp.clicked() {
                            open = Some(b.name.clone());
                        }
                    }
                });
                if let Some(n) = open {
                    self.open_note(n);
                }
            });
    }

    fn central(&mut self, ui: &mut egui::Ui) {
        let frame = egui::Frame::new().fill(theme::pal().bg).inner_margin(egui::Margin::symmetric(40, 28));
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            if self.current.is_none() {
                ui.centered_and_justified(|ui| ui.label(RichText::new("Aucune note. Ctrl+N pour commencer.").color(theme::pal().muted)));
                return;
            }
            let mut goto = None;
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                ui.set_max_width(780.0_f32.min(ui.available_width()));
                if self.preview {
                    let existing: HashSet<_> = self.notes.iter().map(|n| n.name.to_lowercase()).collect();
                    goto = preview::show(ui, &self.text, &existing);
                } else {
                    let r = ui.add(
                        egui::TextEdit::multiline(&mut self.text)
                            .frame(egui::Frame::NONE)
                            .code_editor()
                            .desired_width(f32::INFINITY)
                            .desired_rows(30)
                            .lock_focus(true),
                    );
                    if r.changed() {
                        self.dirty_since = Some(Instant::now());
                    }
                }
            });
            if let Some(n) = goto {
                self.open_note(n);
            }
        });
    }

    fn switcher(&mut self, ctx: &egui::Context) {
        let Some((mut query, mut sel)) = self.switcher.take() else { return };
        let q = query.to_lowercase();
        let matches: Vec<String> =
            self.notes.iter().filter(|n| n.name.to_lowercase().contains(&q)).map(|n| n.name.clone()).take(12).collect();
        let (up, down, enter, esc) = ctx.input(|i| {
            (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::Enter), i.key_pressed(Key::Escape))
        });
        if down && sel + 1 < matches.len() { sel += 1; }
        if up && sel > 0 { sel -= 1; }
        sel = sel.min(matches.len().saturating_sub(1));

        let mut chosen = None;
        let modal = egui::Modal::new(egui::Id::unique("switcher")).show(ctx, |ui| {
            ui.set_width(480.0);
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut query).hint_text("Ouvrir ou créer une note").frame(egui::Frame::NONE).desired_width(f32::INFINITY));
                r.request_focus();
            });
            ui.separator();
            for (i, name) in matches.iter().enumerate() {
                let t = if i == sel { RichText::new(format!("▌ {name}")).color(theme::pal().accent).family(theme::bold()) } else { RichText::new(format!("   {name}")) };
                if ui.add(egui::Button::new(t).frame(false)).clicked() {
                    chosen = Some(name.clone());
                }
            }
            if !query.trim().is_empty() && matches.is_empty() {
                ui.label(RichText::new(format!("Entrée pour créer « {} »", query.trim())).color(theme::pal().muted).small());
            }
        });
        if enter {
            chosen = matches.get(sel).cloned().or_else(|| Some(query.trim().to_string()).filter(|s| !s.is_empty()));
        }
        if let Some(n) = chosen {
            self.open_note(n);
        } else if !(esc || modal.should_close()) {
            self.switcher = Some((query, sel));
        }
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.32);
                let (r, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 0.0, p.accent);
                ui.add_space(10.0);
                ui.label(RichText::new("ARYADNE").size(32.0).family(theme::bold()));
                ui.label(RichText::new("Le fil entre tes idées.").color(p.muted));
                ui.add_space(22.0);
                let btn = egui::Button::new(RichText::new("Ouvrir ou créer un coffre").color(p.on_accent).family(theme::bold()).size(14.0))
                    .fill(p.accent)
                    .stroke(egui::Stroke::NONE)
                    .min_size(egui::vec2(240.0, 40.0));
                if ui.add(btn).clicked() {
                    self.pick_vault();
                }
            });
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);

        // Sauvegarde automatique 500 ms après la dernière frappe, et à la fermeture.
        if self.dirty_since.is_some_and(|t| t.elapsed() > Duration::from_millis(500))
            || ctx.input(|i| i.viewport().close_requested())
        {
            self.flush();
        }
        if self.dirty_since.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        if self.vault.is_none() {
            self.welcome(ui);
        } else {
            self.topbar(ui);
            self.statusbar(ui);
            self.sidebar(ui);
            self.backlinks_panel(ui);
            self.central(ui);
            self.switcher(&ctx);
        }

        if let Some(e) = self.error.clone() {
            egui::Window::new("Erreur").collapsible(false).resizable(false).show(&ctx, |ui| {
                ui.label(RichText::new(&e).color(theme::pal().missing));
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn on_exit(&mut self) {
        self.flush();
    }
}
