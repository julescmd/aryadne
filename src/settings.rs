//! Réglages de l'application : enregistrés dans le dossier de config de l'utilisateur.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct Prefs {
    pub dark: bool,
    /// Couleur d'accent [r, g, b] ; None = orange par défaut.
    pub accent: Option<[u8; 3]>,
    pub ui_zoom: f32,
    pub editor_size: f32,
    pub wrap_code: bool,
    pub line_numbers: bool,
    pub autosave_ms: u64,
    /// « link », « move », « copy » ou « ask »
    pub import_mode: String,
    pub notes_dir: String,
    pub confirm_delete: bool,
    pub auto_update: bool,
    /// Garder Aryadne dans le menu des applications (Linux) / menu Démarrer (Windows).
    pub menu_entry: bool,
    /// Linux : Wayland natif (net en mise à l'échelle fractionnaire, mais sans glisser-déposer).
    pub wayland_native: bool,
    /// Sauvegarde zip quotidienne du coffre.
    pub backup_daily: bool,
    /// Dossier des sauvegardes ("" = Documents/Aryadne-sauvegardes).
    pub backup_dir: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            dark: false,
            accent: None,
            ui_zoom: 1.0,
            editor_size: 13.0,
            wrap_code: false,
            line_numbers: true,
            autosave_ms: 500,
            import_mode: "link".into(),
            notes_dir: "notes".into(),
            confirm_delete: true,
            auto_update: true,
            menu_entry: true,
            wayland_native: false,
            backup_daily: true,
            backup_dir: String::new(),
        }
    }
}

impl Prefs {
    pub fn load() -> Self {
        if let Some(p) = load_config("settings.json").and_then(|t| serde_json::from_str(&t).ok()) {
            return p;
        }
        // Reprend les anciens réglages séparés.
        let mut p = Self::default();
        p.dark = load_config("theme").as_deref() == Some("dark");
        if let Some(m) = load_config("import_mode") {
            if m != "ask" {
                p.import_mode = m;
            }
        }
        p
    }

    pub fn save(&self) {
        save_config("settings.json", &serde_json::to_string_pretty(self).unwrap());
    }

    pub fn apply(&self, ctx: &egui::Context) {
        theme::set_accent(self.accent.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b)));
        theme::apply(ctx, self.dark);
        ctx.set_zoom_factor(self.ui_zoom.clamp(0.6, 2.0));
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Appearance,
    Editor,
    Files,
    Ai,
    Updates,
    Shortcuts,
}

pub struct SettingsState {
    pub tab: Tab,
    pub ai: ai::Settings,
}

const ACCENTS: [(&str, [u8; 3]); 8] = [
    ("orange", [0xff, 0x4f, 0x1f]),
    ("rouge", [0xdc, 0x26, 0x26]),
    ("rose", [0xdb, 0x27, 0x77]),
    ("violet", [0x7c, 0x3a, 0xed]),
    ("bleu", [0x25, 0x63, 0xeb]),
    ("cyan", [0x08, 0x91, 0xb2]),
    ("vert", [0x16, 0xa3, 0x4a]),
    ("encre", [0x18, 0x18, 0x1b]),
];

impl App {
    pub(super) fn open_settings_tab(&mut self, ctx: &egui::Context, tab: Tab) {
        let ai = self.ai.clone().unwrap_or_default();
        self.local.refresh(&ai.url, ctx);
        self.dialog = Some(Dialog::Settings(SettingsState { tab, ai }));
    }

    pub(super) fn settings_window(&mut self, ctx: &egui::Context, mut st: SettingsState) {
        let p = theme::pal();
        let before = self.prefs.clone();
        let mut close = false;
        let mut goto_update = false;
        let mut links_changed = false;
        let frame = egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.text)).inner_margin(0);
        let r = egui::Modal::new(egui::Id::unique("settings")).frame(frame).show(ctx, |ui| {
            let size = egui::vec2(780.0, 540.0);
            let (outer, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let nav_rect = egui::Rect::from_min_size(outer.min, egui::vec2(170.0, size.y));
            let body_rect = egui::Rect::from_min_max(egui::pos2(nav_rect.right(), outer.top()), outer.max);
            ui.painter().rect_filled(nav_rect, 0.0, p.panel);
            ui.painter().line_segment([nav_rect.right_top(), nav_rect.right_bottom()], egui::Stroke::new(1.0, p.line));

            // Navigation
            let mut nav = ui.new_child(egui::UiBuilder::new().max_rect(nav_rect.shrink(14.0)).layout(egui::Layout::top_down(egui::Align::Min)));
            nav.label(caps("réglages"));
            nav.add_space(10.0);
            for (t, label) in [
                (Tab::Appearance, "apparence"),
                (Tab::Editor, "éditeur"),
                (Tab::Files, "fichiers"),
                (Tab::Ai, "ia"),
                (Tab::Updates, "mises à jour"),
                (Tab::Shortcuts, "raccourcis"),
            ] {
                let active = st.tab == t;
                let text = if active { RichText::new(format!("▌ {label}")).color(p.accent).family(theme::bold()) } else { RichText::new(format!("  {label}")).color(p.muted) };
                if nav.add(egui::Button::new(text).frame(false)).clicked() {
                    st.tab = t;
                }
            }
            let mut foot = ui.new_child(egui::UiBuilder::new().max_rect(nav_rect.shrink(14.0)).layout(egui::Layout::bottom_up(egui::Align::Min)));
            if foot.button("fermer").clicked() {
                close = true;
            }
            foot.label(RichText::new(format!("aryadne v{}", updater::CURRENT)).color(p.muted).size(10.5));

            // Contenu
            let mut body = ui.new_child(egui::UiBuilder::new().max_rect(body_rect.shrink(18.0)).layout(egui::Layout::top_down(egui::Align::Min)));
            egui::ScrollArea::vertical().id_salt("settings-body").auto_shrink(false).show(&mut body, |ui| {
                ui.set_width(body_rect.width() - 50.0);
                match st.tab {
                    Tab::Appearance => self.tab_appearance(ui),
                    Tab::Editor => self.tab_editor(ui),
                    Tab::Files => links_changed = self.tab_files(ui),
                    Tab::Ai => self.tab_ai(ui, &mut st.ai, ctx),
                    Tab::Updates => goto_update = self.tab_updates(ui),
                    Tab::Shortcuts => tab_shortcuts(ui),
                }
            });
        });
        // Le zoom n'est appliqué qu'au relâchement de la souris (sinon le curseur saute).
        let zoom_pending = ctx.input(|i| i.pointer.any_down()) && self.prefs.ui_zoom != ctx.zoom_factor();
        if self.prefs != before || (!zoom_pending && self.prefs.ui_zoom != ctx.zoom_factor()) {
            self.prefs.save();
            if zoom_pending {
                let z = self.prefs.ui_zoom;
                self.prefs.ui_zoom = ctx.zoom_factor();
                self.prefs.apply(ctx);
                self.prefs.ui_zoom = z;
            } else {
                self.prefs.apply(ctx);
            }
        }
        if links_changed {
            self.refresh();
        }
        if goto_update {
            self.dialog = Some(Dialog::Update);
            return;
        }
        let esc = ctx.input(|i| i.key_pressed(Key::Escape));
        if close || esc || r.should_close() {
            return;
        }
        self.dialog = Some(Dialog::Settings(st));
    }

    fn tab_appearance(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        ui.label(caps("thème"));
        ui.horizontal(|ui| {
            if ui.selectable_label(!self.prefs.dark, "clair").clicked() {
                self.prefs.dark = false;
            }
            if ui.selectable_label(self.prefs.dark, "sombre").clicked() {
                self.prefs.dark = true;
            }
        });
        ui.add_space(14.0);
        ui.label(caps("couleur d'accent"));
        ui.horizontal_wrapped(|ui| {
            for (name, c) in ACCENTS {
                let col = egui::Color32::from_rgb(c[0], c[1], c[2]);
                let (r, resp) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::click());
                ui.painter().rect_filled(r, 0.0, col);
                let selected = self.prefs.accent.unwrap_or([0xff, 0x4f, 0x1f]) == c;
                if selected {
                    ui.painter().rect_stroke(r.expand(3.0), 0.0, egui::Stroke::new(2.0, p.text), egui::StrokeKind::Inside);
                }
                if resp.on_hover_text(name).clicked() {
                    self.prefs.accent = if name == "orange" { None } else { Some(c) };
                }
            }
            let mut custom = self.prefs.accent.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b)).unwrap_or(p.accent);
            if ui.color_edit_button_srgba(&mut custom).on_hover_text("couleur personnalisée").changed() {
                self.prefs.accent = Some([custom.r(), custom.g(), custom.b()]);
            }
        });
        ui.add_space(14.0);
        ui.label(caps("taille de l'interface"));
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut self.prefs.ui_zoom, 0.75..=1.6).step_by(0.05).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
            if ui.button("100 %").clicked() {
                self.prefs.ui_zoom = 1.0;
            }
        });
        ui.label(RichText::new("Aussi avec Ctrl + / Ctrl − / Ctrl 0.").color(p.muted).size(11.0));
    }

    fn tab_editor(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        ui.label(caps("éditeur"));
        ui.add(egui::Slider::new(&mut self.prefs.editor_size, 10.0..=22.0).step_by(1.0).text("taille du texte"));
        ui.checkbox(&mut self.prefs.line_numbers, "numéros de ligne (fichiers de code)");
        ui.checkbox(&mut self.prefs.wrap_code, "retour à la ligne dans le code");
        ui.add_space(10.0);
        ui.label(caps("sauvegarde automatique"));
        ui.add(egui::Slider::new(&mut self.prefs.autosave_ms, 200..=5000).step_by(100.0).custom_formatter(|v, _| format!("{:.1} s après la dernière frappe", v / 1000.0)));
        ui.add_space(10.0);
        ui.label(caps("notes"));
        ui.horizontal(|ui| {
            ui.label("dossier des nouvelles notes");
            ui.add(egui::TextEdit::singleline(&mut self.prefs.notes_dir).desired_width(160.0));
        });
        ui.label(RichText::new("Utilisé pour Ctrl+N quand rien n'est sélectionné, et pour les liens [[ ]] vers des notes qui n'existent pas encore.").color(p.muted).size(11.0));
    }

    /// Retourne true si les liens ont changé (il faut rescanner).
    fn tab_files(&mut self, ui: &mut egui::Ui) -> bool {
        let p = theme::pal();
        let mut changed = false;
        ui.label(caps("fichiers glissés dans la fenêtre"));
        for (v, label, help) in [
            ("link", "lier (recommandé)", "Le fichier reste où il est. Aryadne l'affiche et le modifie à sa place."),
            ("move", "déplacer dans le coffre", "Le fichier quitte son dossier d'origine. Aucun doublon."),
            ("copy", "copier dans le coffre", "L'original reste en place : le fichier prend deux fois la place."),
            ("ask", "demander à chaque fois", ""),
        ] {
            ui.radio_value(&mut self.prefs.import_mode, v.to_string(), label);
            if !help.is_empty() {
                ui.label(RichText::new(format!("    {help}")).color(p.muted).size(11.0));
            }
        }
        ui.add_space(14.0);
        ui.label(caps("sauvegarde"));
        changed |= ui.checkbox(&mut self.prefs.backup_daily, "sauvegarder le coffre en zip une fois par jour (7 gardées)").changed();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("dans : {}", extras::backup_dir(&self.prefs).display())).color(p.muted).size(11.0));
            if ui.small_button("changer…").clicked() {
                if let Some(d) = rfd::FileDialog::new().set_title("Dossier des sauvegardes").pick_folder() {
                    self.prefs.backup_dir = d.display().to_string();
                    changed = true;
                }
            }
            if ui.small_button("sauvegarder maintenant").clicked() {
                self.start_backup(true);
            }
        });
        ui.label(RichText::new("Les fichiers de plus de 50 Mo et la corbeille ne sont pas inclus. Chaque note garde aussi ses 40 dernières versions (historique).").color(p.muted).size(11.0));
        ui.add_space(14.0);
        ui.label(caps("dossiers liés"));
        ui.label(RichText::new("Des dossiers de ton disque affichés dans Aryadne sans rien copier (refs, projets Blender, cours…).").color(p.muted).size(11.0));
        let root = self.root();
        let links = vault::link_list();
        for f in &links.folders {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("@{}", f.name)).color(p.accent).family(theme::bold()));
                ui.label(RichText::new(&f.path).color(p.muted).size(11.0));
                if ui.small_button("ouvrir").clicked() {
                    reveal(std::path::Path::new(&f.path), false);
                }
                if ui.small_button("retirer").on_hover_text("Le dossier n'est pas supprimé").clicked() {
                    let r = vault::unlink(&root, &format!("@{}", f.name));
                    self.report(r);
                    changed = true;
                }
            });
        }
        if ui.button("+ lier un dossier…").clicked() {
            if let Some(dir) = rfd::FileDialog::new().set_title("Dossier à lier").pick_folder() {
                let r = vault::link_external(&root, &dir);
                self.report(r);
                changed = true;
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(format!("fichiers liés : {}", links.files.len()));
            if !links.files.is_empty() && ui.small_button("tout retirer").on_hover_text("Les fichiers ne sont pas supprimés").clicked() {
                let r = vault::unlink(&root, vault::LINKED_FILES);
                self.report(r);
                changed = true;
            }
        });
        ui.add_space(14.0);
        ui.label(caps("coffre"));
        ui.label(RichText::new(&root).color(p.muted).size(11.0));
        ui.horizontal(|ui| {
            if ui.button("ouvrir le dossier").clicked() {
                reveal(std::path::Path::new(&root), false);
            }
            if ui.button("changer de coffre…").clicked() {
                self.pick_vault();
                changed = true;
            }
        });
        ui.checkbox(&mut self.prefs.confirm_delete, "confirmer avant de supprimer");
        changed
    }

    fn tab_ai(&mut self, ui: &mut egui::Ui, s: &mut ai::Settings, ctx: &egui::Context) {
        let p = theme::pal();
        let before = (s.url.clone(), s.model.clone(), s.key.clone());
        ui.label(caps("résumé et chat ia"));
        ui.label(RichText::new("Toute API compatible OpenAI. Par défaut : Ollama en local, gratuit et privé.").color(p.muted).size(11.5));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("ollama").clicked() {
                s.url = "http://localhost:11434/v1".into();
            }
            if ui.button("lm studio").clicked() {
                s.url = "http://localhost:1234/v1".into();
            }
            if ui.button("openai").clicked() {
                s.url = "https://api.openai.com/v1".into();
            }
        });
        ui.add_space(6.0);
        egui::Grid::new("ai-grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("adresse");
            ui.add(egui::TextEdit::singleline(&mut s.url).desired_width(380.0));
            ui.end_row();
            ui.label("modèle");
            ui.add(egui::TextEdit::singleline(&mut s.model).hint_text("llama3.2, qwen2.5, gpt-4o-mini…").desired_width(380.0));
            ui.end_row();
            ui.label("clé api");
            ui.add(egui::TextEdit::singleline(&mut s.key).password(true).hint_text("vide pour Ollama / LM Studio").desired_width(380.0));
            ui.end_row();
        });
        if (s.url.clone(), s.model.clone(), s.key.clone()) != before {
            s.save();
            self.ai = Some(s.clone());
        }
        if ollama::is_local(&s.url) {
            ui.add_space(14.0);
            ui.separator();
            ui.add_space(4.0);
            ollama_panel(ui, &self.local, s, ctx);
        }
    }

    /// Retourne true pour ouvrir la fenêtre de mise à jour.
    fn tab_updates(&mut self, ui: &mut egui::Ui) -> bool {
        let p = theme::pal();
        ui.label(caps("mises à jour"));
        ui.label(format!("version installée : v{}", updater::CURRENT));
        ui.checkbox(&mut self.prefs.auto_update, "vérifier au démarrage (toutes les 6 h)");
        ui.label(RichText::new(format!("Source : releases de github.com/{}", updater::REPO)).color(p.muted).size(11.0));
        ui.add_space(8.0);
        let check = ui.button("vérifier maintenant").clicked();
        ui.add_space(16.0);
        ui.label(caps("système"));
        let label = if cfg!(windows) { "Aryadne dans le menu Démarrer" } else { "Aryadne dans le menu des applications (avec son icône)" };
        if ui.checkbox(&mut self.prefs.menu_entry, label).changed() {
            if self.prefs.menu_entry {
                if let Err(e) = desktop::install() {
                    self.error = Some(e);
                }
            } else {
                desktop::uninstall();
            }
        }
        if cfg!(target_os = "linux") {
            ui.checkbox(&mut self.prefs.wayland_native, "Wayland natif (plus net si l'écran est mis à l'échelle, mais le glisser-déposer de fichiers ne marche pas)");
            ui.label(RichText::new("Prend effet au prochain démarrage.").color(p.muted).size(11.0));
        }
        check
    }
}

fn tab_shortcuts(ui: &mut egui::Ui) {
    let p = theme::pal();
    ui.label(caps("raccourcis"));
    let rows = [
        ("Ctrl+P", "ouvrir un fichier, ou créer une note"),
        ("Ctrl+N", "nouvelle note"),
        ("Ctrl+S", "sauvegarder"),
        ("Ctrl+W", "fermer l'onglet"),
        ("Ctrl+E", "édition / lecture (Markdown)"),
        ("Ctrl+1 … 5", "fichiers, cerveau, refs, boards, chat"),
        ("Ctrl+G", "cerveau"),
        ("Ctrl + / − / 0", "taille de l'interface"),
        ("Ctrl+Maj+F", "recherche dans tout le coffre"),
        ("Ctrl+Maj+M", "mode compact"),
        ("", ""),
        ("board · V", "outil sélection"),
        ("board · B", "outil dessin"),
        ("board · E", "gomme"),
        ("board · T", "texte"),
        ("board · Ctrl+Z / Ctrl+Y", "annuler / rétablir"),
        ("board · Ctrl+A", "tout sélectionner"),
        ("board · Ctrl+D", "dupliquer"),
        ("board · Ctrl+V", "coller une image"),
        ("board · Suppr", "supprimer la sélection"),
        ("board · Maj", "redimensionner sans garder les proportions"),
        ("board · clic milieu / droit, ou Espace", "déplacer la vue"),
    ];
    egui::Grid::new("keys").num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
        for (k, v) in rows {
            ui.label(RichText::new(k).color(p.accent));
            ui.label(v);
            ui.end_row();
        }
    });
}

/// Ouvre le gestionnaire de fichiers du système sur ce chemin.
pub fn reveal(path: &std::path::Path, select: bool) {
    #[cfg(windows)]
    {
        let _ = if select {
            std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn()
        } else {
            std::process::Command::new("explorer").arg(path).spawn()
        };
    }
    #[cfg(not(windows))]
    {
        let target = if select { path.parent().unwrap_or(path) } else { path };
        let _ = std::process::Command::new("xdg-open").arg(target).spawn();
    }
}

/// Ouvre un fichier avec l'application par défaut du système (Blender pour un .blend, etc.).
pub fn open_external(path: &std::path::Path) -> Result<(), String> {
    open::that_detached(path).map_err(|e| format!("Impossible d'ouvrir {} : {e}", path.display()))
}

/// Laisse choisir l'application (fenêtre « Ouvrir avec » de Windows).
pub fn open_with(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("rundll32.exe")
            .arg("shell32.dll,OpenAs_RunDLL")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Impossible d'ouvrir {} : {e}", path.display()))
    }
    #[cfg(not(windows))]
    {
        open_external(path)
    }
}
