//! Réglages de l'application : enregistrés dans le dossier de config de l'utilisateur.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct Prefs {
    /// Langue de l'interface (« fr », « en ») ; vide = langue du système
    pub lang: String,
    pub dark: bool,
    /// Couleur d'accent [r, g, b] ; None = orange par défaut.
    pub accent: Option<[u8; 3]>,
    pub ui_zoom: f32,
    pub editor_size: f32,
    pub wrap_code: bool,
    pub line_numbers: bool,
    pub minimap: bool,
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
    /// Polices : interface, chiffres et code, notes (nom intégré ou chemin d'un fichier ; vide = par défaut)
    pub ui_font: String,
    pub mono_font: String,
    pub notes_font: String,
    /// Unité de longueur affichée en 3D (« m », « cm », « mm », « km », « in », « ft ») ; la scène reste en mètres
    pub unit: String,
    /// Accessibilité : contraste élevé et animations réduites
    pub high_contrast: bool,
    pub reduce_motion: bool,
    /// Navigation 3D : « blender » (molette enfoncée) ou « maya » (Alt + boutons)
    pub nav3d: String,
    /// Thème de la charte (« rose », « sauge », « lagon », « argile », « graphite », « clair ») ;
    /// vide = d'après l'ancien réglage clair / sombre
    pub theme: String,
}

/// Unités de longueur proposées : nom et nombre d'unités dans un mètre.
pub const UNITS: [(&str, f32); 6] = [("m", 1.0), ("cm", 100.0), ("mm", 1000.0), ("km", 0.001), ("in", 39.370_08), ("ft", 3.280_84)];
static UNIT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// L'unité choisie : nom et facteur (valeur affichée = mètres × facteur).
pub fn unit() -> (&'static str, f32) {
    UNITS[UNIT.load(std::sync::atomic::Ordering::Relaxed).min(UNITS.len() - 1)]
}

static NAV_MAYA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Navigation 3D au profil Maya (Alt + gauche / milieu / droit) au lieu de Blender
pub fn nav_maya() -> bool {
    NAV_MAYA.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_unit(name: &str) {
    UNIT.store(UNITS.iter().position(|u| u.0 == name).unwrap_or(0), std::sync::atomic::Ordering::Relaxed);
}

impl Prefs {
    pub fn fonts(&self) -> theme::Fonts {
        theme::Fonts { ui: self.ui_font.clone(), mono: self.mono_font.clone(), notes: self.notes_font.clone() }
    }
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            lang: String::new(),
            dark: true,
            accent: None,
            ui_zoom: 1.0,
            editor_size: 13.0,
            wrap_code: false,
            line_numbers: true,
            minimap: true,
            autosave_ms: 500,
            import_mode: "link".into(),
            notes_dir: "notes".into(),
            confirm_delete: true,
            auto_update: true,
            menu_entry: true,
            wayland_native: false,
            backup_daily: true,
            backup_dir: String::new(),
            ui_font: String::new(),
            mono_font: String::new(),
            notes_font: String::new(),
            unit: "m".into(),
            high_contrast: false,
            reduce_motion: false,
            nav3d: "blender".into(),
            theme: String::new(),
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
        theme::set_access(self.high_contrast, self.reduce_motion);
        theme::set_theme(if self.theme.is_empty() { if self.dark { "rose" } else { "clair" } } else { &self.theme });
        NAV_MAYA.store(self.nav3d == "maya", std::sync::atomic::Ordering::Relaxed);
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
    Cloud,
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
        self.local.refresh(if ollama::is_local(&ai.url) { &ai.url } else { ai::PROVIDERS[0].url }, ctx);
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
            nav.label(caps(&crate::i18n::t("réglages")));
            nav.add_space(10.0);
            for (t, label) in [
                (Tab::Appearance, "apparence"),
                (Tab::Editor, "éditeur"),
                (Tab::Files, "fichiers"),
                (Tab::Ai, "ia"),
                (Tab::Cloud, "coffre en ligne"),
                (Tab::Updates, "mises à jour"),
                (Tab::Shortcuts, "raccourcis"),
            ] {
                let active = st.tab == t;
                let text = if active { RichText::new(format!("▌ {}", crate::i18n::t(label))).color(p.accent).family(theme::bold()) } else { RichText::new(format!("  {}", crate::i18n::t(label))).color(p.muted) };
                if nav.add(egui::Button::new(text).frame(false)).clicked() {
                    st.tab = t;
                }
            }
            let mut foot = ui.new_child(egui::UiBuilder::new().max_rect(nav_rect.shrink(14.0)).layout(egui::Layout::bottom_up(egui::Align::Min)));
            if foot.button(crate::i18n::t("fermer")).clicked() {
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
                    Tab::Cloud => self.tab_cloud(ui, ctx),
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
        ui.label(caps("langue / language"));
        ui.horizontal(|ui| {
            for (code, name) in crate::i18n::LANGS {
                if ui.selectable_label(crate::i18n::current() == *code, *name).clicked() {
                    self.prefs.lang = code.to_string();
                    crate::i18n::set(code);
                }
            }
        });
        ui.add_space(12.0);
        ui.label(caps(&crate::i18n::t("thème")));
        // Une vignette par thème : fond, panneau, texte, couleur principale et les couleurs d'espace
        ui.horizontal_wrapped(|ui| {
            for t in &theme::THEMES {
                let on = theme::theme().id == t.id;
                let (r, resp) = ui.allocate_exact_size(egui::vec2(96.0, 64.0), egui::Sense::click());
                let pp = &t.p;
                ui.painter().rect_filled(r, 4.0, pp.bg);
                ui.painter().rect_filled(egui::Rect::from_min_size(r.min, egui::vec2(24.0, r.height())), egui::CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 }, pp.panel);
                ui.painter().rect_filled(egui::Rect::from_min_size(r.min + egui::vec2(32.0, 10.0), egui::vec2(36.0, 12.0)), 2.0, pp.accent);
                for (k, c) in t.ws.iter().enumerate() {
                    ui.painter().circle_filled(r.min + egui::vec2(34.0 + k as f32 * 8.0, 34.0), 2.6, *c);
                }
                ui.painter().text(r.left_bottom() + egui::vec2(30.0, -6.0), egui::Align2::LEFT_BOTTOM, crate::i18n::t(t.name), egui::FontId::proportional(11.0), pp.text);
                if on {
                    ui.painter().rect_stroke(r.expand(2.0), 5.0, egui::Stroke::new(2.0, p.accent), egui::StrokeKind::Outside);
                }
                if resp.on_hover_text(crate::i18n::t(t.name)).clicked() {
                    self.prefs.theme = t.id.to_string();
                    self.prefs.dark = t.dark;
                }
            }
        });
        ui.add_space(14.0);
        ui.label(caps(&crate::i18n::t("couleur d'accent")));
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
            if ui.color_edit_button_srgba(&mut custom).on_hover_text(crate::i18n::t("couleur personnalisée")).changed() {
                self.prefs.accent = Some([custom.r(), custom.g(), custom.b()]);
            }
        });
        ui.add_space(14.0);
        ui.label(caps(&crate::i18n::t("polices")));
        let (mut a, mut b, mut c) = (self.prefs.ui_font.clone(), self.prefs.mono_font.clone(), self.prefs.notes_font.clone());
        egui::Grid::new("fonts").num_columns(3).spacing([12.0, 8.0]).show(ui, |ui| {
            font_picker(ui, "interface", &mut a, "inter");
            ui.end_row();
            font_picker(ui, "chiffres et code", &mut b, "jetbrains");
            ui.end_row();
            font_picker(ui, "notes", &mut c, "jetbrains");
            ui.end_row();
        });
        (self.prefs.ui_font, self.prefs.mono_font, self.prefs.notes_font) = (a, b, c);
        ui.add_space(14.0);
        ui.label(caps(&crate::i18n::t("unités (3D)")));
        ui.horizontal_wrapped(|ui| {
            for (name, _) in UNITS {
                if ui.selectable_label(self.prefs.unit == name, name).clicked() {
                    self.prefs.unit = name.to_string();
                }
            }
        });
        ui.label(egui::RichText::new(crate::i18n::t("Les positions, distances et tailles s'affichent dans cette unité ; les fichiers restent en mètres.")).size(11.0).color(p.muted));
        ui.add_space(10.0);
        ui.label(caps(&crate::i18n::t("navigation 3D")));
        ui.horizontal(|ui| {
            for (id, name) in [("blender", "Blender"), ("maya", "Maya")] {
                if ui.selectable_label(self.prefs.nav3d == id, name).clicked() {
                    self.prefs.nav3d = id.to_string();
                }
            }
        });
        let help = if self.prefs.nav3d == "maya" {
            "Alt + clic gauche : tourner · Alt + clic milieu (ou milieu seul) : déplacer · Alt + clic droit : zoom · molette : zoom"
        } else {
            "Molette enfoncée : tourner · Maj : déplacer · Ctrl : zoom · Alt + clic gauche : tourner · molette : zoom"
        };
        ui.label(egui::RichText::new(crate::i18n::t(help)).size(11.0).color(p.muted));
        ui.add_space(14.0);
        ui.label(caps(&crate::i18n::t("taille de l'interface")));
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut self.prefs.ui_zoom, 0.75..=2.0).step_by(0.05).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
            if ui.button("100 %").clicked() {
                self.prefs.ui_zoom = 1.0;
            }
        });
        ui.label(RichText::new(crate::i18n::t("Aussi avec Ctrl + / Ctrl − / Ctrl 0.")).color(p.muted).size(11.0));
        ui.add_space(14.0);
        ui.label(caps(&crate::i18n::t("accessibilité")));
        ui.checkbox(&mut self.prefs.high_contrast, crate::i18n::t("contraste élevé")).on_hover_text(crate::i18n::t("Textes secondaires, bordures et champs plus marqués, contour sur chaque commande"));
        ui.checkbox(&mut self.prefs.reduce_motion, crate::i18n::t("réduire les animations")).on_hover_text(crate::i18n::t("Pas de fondus ni de défilement animé"));
    }

    fn tab_editor(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        ui.label(caps(&crate::i18n::t("éditeur")));
        ui.add(egui::Slider::new(&mut self.prefs.editor_size, 10.0..=22.0).step_by(1.0).text(crate::i18n::t("taille du texte")));
        ui.checkbox(&mut self.prefs.line_numbers, crate::i18n::t("numéros de ligne (fichiers de code)"));
        ui.checkbox(&mut self.prefs.minimap, crate::i18n::t("mini-carte à droite (fichiers de code)"));
        ui.checkbox(&mut self.prefs.wrap_code, crate::i18n::t("retour à la ligne dans le code"));
        ui.add_space(10.0);
        ui.label(caps(&crate::i18n::t("sauvegarde automatique")));
        ui.add(egui::Slider::new(&mut self.prefs.autosave_ms, 200..=5000).step_by(100.0).custom_formatter(|v, _| format!("{:.1} s après la dernière frappe", v / 1000.0)));
        ui.add_space(10.0);
        ui.label(caps(&crate::i18n::t("notes")));
        ui.horizontal(|ui| {
            ui.label(crate::i18n::t("dossier des nouvelles notes"));
            ui.add(egui::TextEdit::singleline(&mut self.prefs.notes_dir).desired_width(160.0));
        });
        ui.label(RichText::new(crate::i18n::t("Utilisé pour Ctrl+N quand rien n'est sélectionné, et pour les liens [[ ]] vers des notes qui n'existent pas encore.")).color(p.muted).size(11.0));
    }

    /// Retourne true si les liens ont changé (il faut rescanner).
    fn tab_files(&mut self, ui: &mut egui::Ui) -> bool {
        let p = theme::pal();
        let mut changed = false;
        ui.label(caps(&crate::i18n::t("fichiers glissés dans la fenêtre")));
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
        ui.label(caps(&crate::i18n::t("sauvegarde")));
        changed |= ui.checkbox(&mut self.prefs.backup_daily, crate::i18n::t("sauvegarder le coffre en zip une fois par jour (7 gardées)")).changed();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("dans : {}", extras::backup_dir(&self.prefs).display())).color(p.muted).size(11.0));
            if ui.small_button(crate::i18n::t("changer…")).clicked() {
                if let Some(d) = rfd::FileDialog::new().set_title("Dossier des sauvegardes").pick_folder() {
                    self.prefs.backup_dir = d.display().to_string();
                    changed = true;
                }
            }
            if ui.small_button(crate::i18n::t("sauvegarder maintenant")).clicked() {
                self.start_backup(true);
            }
        });
        ui.label(RichText::new(crate::i18n::t("Les fichiers de plus de 50 Mo et la corbeille ne sont pas inclus. Chaque note garde aussi ses 40 dernières versions (historique).")).color(p.muted).size(11.0));
        ui.add_space(14.0);
        ui.label(caps(&crate::i18n::t("dossiers liés")));
        ui.label(RichText::new(crate::i18n::t("Des dossiers de ton disque affichés dans Aryadne sans rien copier (refs, projets Blender, cours…).")).color(p.muted).size(11.0));
        let root = self.root();
        let links = vault::link_list();
        for f in &links.folders {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("@{}", f.name)).color(p.accent).family(theme::bold()));
                ui.label(RichText::new(&f.path).color(p.muted).size(11.0));
                if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::FOLDER_OPEN).size(13.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("ouvrir")).clicked() {
                    reveal(std::path::Path::new(&f.path), false);
                }
                if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::X).size(13.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Le dossier n'est pas supprimé")).clicked() {
                    let r = vault::unlink(&root, &format!("@{}", f.name));
                    self.report(r);
                    changed = true;
                }
            });
        }
        if ui.button(crate::i18n::t("+ lier un dossier…")).clicked() {
            if let Some(dir) = rfd::FileDialog::new().set_title("Dossier à lier").pick_folder() {
                let r = vault::link_external(&root, &dir);
                self.report(r);
                changed = true;
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(format!("fichiers liés : {}", links.files.len()));
            if !links.files.is_empty() && ui.add(egui::Button::new(egui::RichText::new(crate::lucide::X).size(13.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Les fichiers ne sont pas supprimés")).clicked() {
                let r = vault::unlink(&root, vault::LINKED_FILES);
                self.report(r);
                changed = true;
            }
        });
        ui.add_space(14.0);
        ui.label(caps(&crate::i18n::t("coffre")));
        ui.label(RichText::new(&root).color(p.muted).size(11.0));
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::FOLDER_OPEN).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("ouvrir le dossier")).clicked() {
                reveal(std::path::Path::new(&root), false);
            }
            if ui.button(crate::i18n::t("changer de coffre…")).clicked() {
                self.pick_vault();
                changed = true;
            }
        });
        ui.checkbox(&mut self.prefs.confirm_delete, crate::i18n::t("confirmer avant de supprimer"));
        changed
    }

    fn tab_ai(&mut self, ui: &mut egui::Ui, s: &mut ai::Settings, ctx: &egui::Context) {
        let p = theme::pal();
        let before = (s.url.clone(), s.model.clone(), s.key.clone(), s.keys.clone());
        ui.label(caps(&crate::i18n::t("fournisseurs d'ia")));
        ui.label(RichText::new(crate::i18n::t("Colle une clé API pour brancher un service. Ses modèles apparaissent ensuite dans le menu du chat. Les clés restent sur cet ordinateur (dossier de config d'Aryadne).")).color(p.muted).size(11.5));
        ui.add_space(8.0);
        let active = s.provider_id();
        egui::Grid::new("ai-providers").num_columns(4).spacing([10.0, 8.0]).show(ui, |ui| {
            for pr in ai::PROVIDERS {
                let on = active == pr.id;
                ui.label(RichText::new(pr.name).strong().color(if on { p.accent } else { p.text }));
                if pr.key_page.is_empty() {
                    ui.label(RichText::new(crate::i18n::t("local, gratuit, sans clé")).color(p.muted).size(11.0));
                    ui.label("");
                } else {
                    let key = s.keys.entry(pr.id.to_string()).or_default();
                    let r = ui.add(egui::TextEdit::singleline(key).password(true).hint_text(crate::i18n::t("clé api")).desired_width(180.0));
                    if r.lost_focus() && !key.trim().is_empty() {
                        ai::fetch_models(pr.id, key.clone(), ctx.clone());
                    }
                    ui.hyperlink_to(RichText::new(crate::i18n::t("clé ↗")).size(11.0), pr.key_page).on_hover_text(pr.key_page);
                }
                let has_key = pr.key_page.is_empty() || s.keys.get(pr.id).is_some_and(|k| !k.trim().is_empty());
                ui.horizontal(|ui| {
                    if has_key {
                        if let Some(e) = ai::models_error(pr.id).filter(|_| !pr.key_page.is_empty()) {
                            ui.label(RichText::new(format!("✗ {e}")).color(p.missing).size(11.0));
                        } else if !pr.key_page.is_empty() {
                            ui.label(RichText::new(format!("✓ {} modèles", ai::models_of(pr.id).len())).color(p.muted).size(11.0));
                            if ui.small_button("↻").on_hover_text(crate::i18n::t("relire la liste des modèles")).clicked() {
                                ai::fetch_models(pr.id, s.keys.get(pr.id).cloned().unwrap_or_default(), ctx.clone());
                            }
                        }
                        if !on && ui.small_button(crate::i18n::t("utiliser")).clicked() {
                            let m = if pr.id == "ollama" { self.local.snapshot().models.first().cloned().unwrap_or_else(|| "llama3.2".into()) } else { ai::models_of(pr.id).first().cloned().unwrap_or_default() };
                            s.use_model(pr.id, &m);
                        }
                    }
                });
                ui.end_row();
            }
        });
        ui.add_space(12.0);
        ui.label(caps(&crate::i18n::t("modèle actif")));
        egui::Grid::new("ai-grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label(crate::i18n::t("modèle"));
            ui.add(egui::TextEdit::singleline(&mut s.model).desired_width(380.0));
            ui.end_row();
            ui.label(crate::i18n::t("adresse"));
            ui.add(egui::TextEdit::singleline(&mut s.url).desired_width(380.0)).on_hover_text(crate::i18n::t("Toute API compatible OpenAI. Change-la pour un serveur personnalisé."));
            ui.end_row();
            if active == "custom" {
                ui.label(crate::i18n::t("clé api"));
                ui.add(egui::TextEdit::singleline(&mut s.key).password(true).desired_width(380.0));
                ui.end_row();
            }
        });
        // La clé active suit celle du fournisseur
        if let Some(pr) = ai::provider_for_url(&s.url) {
            s.key = s.keys.get(pr.id).cloned().unwrap_or_default();
        }
        if (s.url.clone(), s.model.clone(), s.key.clone(), s.keys.clone()) != before {
            s.save();
            self.ai = Some(s.clone());
        }
        ui.add_space(14.0);
        ui.separator();
        ui.add_space(4.0);
        // Gestion des modèles locaux : toujours sur l'adresse d'Ollama
        let mut local = s.clone();
        if active != "ollama" {
            local.url = ai::PROVIDERS[0].url.to_string();
            local.model.clear();
        }
        if ollama_panel(ui, &self.local, &mut local, ctx) {
            if active == "ollama" || !local.model.is_empty() {
                s.use_model("ollama", &local.model);
                s.url = local.url.clone();
            }
            s.save();
            self.ai = Some(s.clone());
        }
    }

    /// Retourne true pour ouvrir la fenêtre de mise à jour.
    fn tab_updates(&mut self, ui: &mut egui::Ui) -> bool {
        let p = theme::pal();
        ui.label(caps(&crate::i18n::t("mises à jour")));
        ui.label(format!("version installée : v{}", updater::CURRENT));
        ui.checkbox(&mut self.prefs.auto_update, crate::i18n::t("vérifier au démarrage (toutes les 6 h)"));
        ui.label(RichText::new(format!("Source : releases de github.com/{}", updater::REPO)).color(p.muted).size(11.0));
        ui.add_space(8.0);
        let check = ui.add(egui::Button::new(egui::RichText::new(crate::lucide::REFRESH_CW).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("vérifier maintenant")).clicked();
        ui.add_space(16.0);
        ui.label(caps(&crate::i18n::t("système")));
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
            ui.checkbox(&mut self.prefs.wayland_native, crate::i18n::t("Wayland natif (plus net si l'écran est mis à l'échelle, mais le glisser-déposer de fichiers ne marche pas)"));
            ui.label(RichText::new(crate::i18n::t("Prend effet au prochain démarrage.")).color(p.muted).size(11.0));
        }
        check
    }
}

fn tab_shortcuts(ui: &mut egui::Ui) {
    let p = theme::pal();
    ui.label(caps(&crate::i18n::t("raccourcis globaux")));
    crate::keymap::settings_ui(ui);
    ui.add_space(16.0);
    ui.label(caps(&crate::i18n::t("autres raccourcis")));
    let rows = [
        ("Ctrl+P", "ouvrir un fichier, ou créer une note"),
        ("Ctrl+N", "nouvelle note"),
        ("Ctrl+S", "sauvegarder"),
        ("Ctrl+W", "fermer l'onglet"),
        ("Ctrl+E", "édition / lecture (Markdown)"),
        ("Ctrl+1 … 5", "étapes : notes, 2D, 3D, vidéo, code"),
        ("Alt+1 … 3", "outils de l'étape (Moodboard, Retouche, Compositing…)"),
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

/// Choix d'une police : intégrées, installées sur l'ordinateur, ou un fichier .ttf / .otf.
fn font_picker(ui: &mut egui::Ui, label: &str, value: &mut String, default: &str) {
    let p = theme::pal();
    ui.label(RichText::new(crate::i18n::t(label)).color(p.muted));
    let current = if value.is_empty() { default.to_string() } else { value.clone() };
    egui::ComboBox::from_id_salt(("font", label)).width(220.0).selected_text(theme::font_label(&current)).height(360.0).show_ui(ui, |ui| {
        for (id, name) in theme::builtin_fonts() {
            if ui.selectable_label(current == id, name).clicked() {
                *value = if id == default { String::new() } else { id };
            }
        }
        ui.separator();
        ui.label(RichText::new(crate::i18n::t("installées sur l'ordinateur")).color(p.muted).size(11.0));
        for (name, path) in theme::system_fonts() {
            if ui.selectable_label(current == path, name).clicked() {
                *value = path;
            }
        }
    });
    ui.horizontal(|ui| {
        if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::FILE).size(13.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Choisir un fichier .ttf ou .otf")).clicked() {
            if let Some(f) = rfd::FileDialog::new().add_filter("police", &["ttf", "otf"]).pick_file() {
                *value = f.to_string_lossy().to_string();
            }
        }
        // Aperçu dans la police choisie (une fois chargée)
        if theme::has_font(ui, &current) {
            ui.label(RichText::new("Aa Bb 0123").family(theme::font_family(&current, false, false)).size(14.0));
        }
    });
}
