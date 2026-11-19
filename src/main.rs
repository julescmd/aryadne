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
        theme::apply(ctx);
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
        if let Some(f) = config_file() {
            let _ = std::fs::create_dir_all(f.parent().unwrap());
            let _ = std::fs::write(f, &path);
        }
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
        egui::Panel::top("top").exact_size(30.0).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new("aryadne").color(theme::ACCENT).strong());
                let path = self.vault.clone().unwrap_or_default();
                if ui.button(RichText::new(format!("~ {path}")).color(theme::MUTED)).on_hover_text("Changer de coffre").clicked() {
                    self.pick_vault();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (label, active) in [("boards", false), ("refs", false), ("notes", true)] {
                        let t = if active {
                            RichText::new(format!("[{label}]")).color(theme::ACCENT)
                        } else {
                            RichText::new(format!(" {label} ")).color(theme::MUTED)
                        };
                        ui.label(t).on_hover_text(if active { "" } else { "Bientôt" });
                    }
                });
            });
        });
    }

    fn statusbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status").exact_size(22.0).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let (mode, color) = if self.preview { (" LECTURE ", theme::LINK) } else { (" ÉDITION ", theme::ACCENT) };
                ui.label(RichText::new(mode).background_color(color).color(theme::BG).strong());
                if let Some(n) = &self.current {
                    ui.label(format!("{n}.md"));
                    let words = self.text.split_whitespace().count();
                    ui.label(RichText::new(format!("{words} mots")).color(theme::MUTED));
                    let saved = if self.dirty_since.is_some() { "● non sauvé" } else { "✓ sauvé" };
                    ui.label(RichText::new(saved).color(theme::MUTED));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("ctrl+p ouvrir · ctrl+n nouvelle · ctrl+e lecture").color(theme::MUTED).small());
                });
            });
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("notes").resizable(true).default_size(230.0).min_size(160.0).show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(">").color(theme::ACCENT));
                ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("filtrer").frame(egui::Frame::NONE).desired_width(ui.available_width() - 28.0));
                if ui.button("+").on_hover_text("Nouvelle note (Ctrl+N)").clicked() {
                    self.new_note();
                }
            });
            ui.separator();
            let f = self.filter.to_lowercase();
            let mut open = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                for n in self.notes.iter().filter(|n| n.name.to_lowercase().contains(&f)) {
                    let active = self.current.as_deref() == Some(&n.name);
                    let label = if active {
                        RichText::new(format!("▸ {}", n.name)).color(theme::ACCENT)
                    } else {
                        RichText::new(format!("  {}", n.name)).color(theme::MUTED)
                    };
                    if ui.add(egui::Button::new(label).frame(false).truncate()).clicked() {
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
        egui::Panel::right("backlinks").resizable(true).default_size(250.0).min_size(160.0).show(ui, |ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("-- liens entrants --").color(theme::MUTED));
            ui.add_space(4.0);
            if self.backlinks.is_empty() {
                ui.label(RichText::new("aucune note ne pointe ici").color(theme::MUTED).small());
            }
            let mut open = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                for b in &self.backlinks {
                    let r = egui::Frame::new().stroke(egui::Stroke::new(1.0, theme::LINE)).inner_margin(6.0).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(&b.name).color(theme::LINK));
                        ui.label(RichText::new(&b.excerpt).color(theme::MUTED).small());
                    });
                    if r.response.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
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
        let frame = egui::Frame::new().fill(theme::BG).inner_margin(egui::Margin::symmetric(24, 16));
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            if self.current.is_none() {
                ui.centered_and_justified(|ui| ui.label(RichText::new("aucune note · ctrl+n pour commencer").color(theme::MUTED)));
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
                ui.label(RichText::new(">").color(theme::ACCENT));
                let r = ui.add(egui::TextEdit::singleline(&mut query).hint_text("ouvrir ou créer une note").frame(egui::Frame::NONE).desired_width(f32::INFINITY));
                r.request_focus();
            });
            ui.separator();
            for (i, name) in matches.iter().enumerate() {
                let t = if i == sel { RichText::new(format!("▸ {name}")).color(theme::ACCENT) } else { RichText::new(format!("  {name}")) };
                if ui.add(egui::Button::new(t).frame(false)).clicked() {
                    chosen = Some(name.clone());
                }
            }
            if !query.trim().is_empty() && matches.is_empty() {
                ui.label(RichText::new(format!("  ↵ crée « {} »", query.trim())).color(theme::MUTED).small());
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
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG)).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.33);
                ui.label(RichText::new("aryadne").size(34.0).color(theme::ACCENT).strong());
                ui.label(RichText::new("le fil entre tes idées").color(theme::MUTED));
                ui.add_space(18.0);
                if ui.button(RichText::new("[ ouvrir ou créer un coffre ]").size(15.0)).clicked() {
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
            egui::Window::new("erreur").collapsible(false).resizable(false).show(&ctx, |ui| {
                ui.label(RichText::new(&e).color(theme::MISSING));
                if ui.button("[ ok ]").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn on_exit(&mut self) {
        self.flush();
    }
}
