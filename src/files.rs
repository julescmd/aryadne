//! Panneau des fichiers (à gauche) : sections « épinglés » et « récents », arbre du coffre
//! replié par défaut, filtres par type, tri, icônes et couleurs par type de fichier.
use super::*;
use eframe::egui::Color32;
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FilesState {
    pub expanded: HashSet<String>,
    pub pinned: Vec<String>,
    pub recent: Vec<String>,
    /// 0 = nom, 1 = modifié récemment, 2 = type
    pub sort: u8,
    pub show_ext: bool,
    pub hide_recent: bool,
    /// Vue en grille avec miniatures (façon Unreal)
    #[serde(default)]
    pub grid: bool,
    #[serde(default = "tile_default")]
    pub tile: f32,
    /// Dossier affiché en grille
    #[serde(default)]
    pub dir: String,
    #[serde(skip)]
    pub kind: Option<&'static str>,
    #[serde(skip)]
    loaded_for: String,
}

fn tile_default() -> f32 {
    96.0
}

impl FilesState {
    fn path(root: &str) -> PathBuf {
        std::path::Path::new(root).join(".app/explorer.json")
    }
    pub fn load(root: &str) -> FilesState {
        let mut s: FilesState = std::fs::read_to_string(Self::path(root)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        s.loaded_for = root.to_string();
        s
    }
    pub fn save(&self) {
        if self.loaded_for.is_empty() {
            return;
        }
        if let Ok(t) = serde_json::to_string(self) {
            let _ = std::fs::write(Self::path(&self.loaded_for), t);
        }
    }
    /// Déplie tous les dossiers parents de `rel` (pour le montrer dans l'arbre).
    pub fn reveal(&mut self, rel: &str) {
        let mut cur = vault::parent(rel);
        while !cur.is_empty() {
            self.expanded.insert(cur.clone());
            cur = vault::parent(&cur);
        }
    }
    pub fn touch(&mut self, rel: &str) {
        self.recent.retain(|r| r != rel);
        self.recent.insert(0, rel.to_string());
        self.recent.truncate(12);
        self.save();
    }
}

/// Type de fichier : (icône, couleur, nom du filtre)
pub fn kind(rel: &str) -> (&'static str, Color32, &'static str) {
    let l = rel.to_lowercase();
    let e = l.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if vault::is_md(&l) {
        ("#", Color32::from_rgb(0xe8, 0x6a, 0x2c), "notes")
    } else if board::is_board(&l) {
        ("▦", Color32::from_rgb(0xd9, 0xa4, 0x1c), "boards")
    } else if nodes::is_graph(&l) {
        ("⬡", Color32::from_rgb(0xd0, 0x4a, 0x84), "nodes")
    } else if l.ends_with(".blend") || model3d::is_model(&l) {
        ("◆", Color32::from_rgb(0x8a, 0x5c, 0xd6), "3d")
    } else if video::is_video(&l) {
        ("▶", Color32::from_rgb(0xd9, 0x48, 0x48), "vidéos")
    } else if images::is_image(&l) || matches!(e, "psd" | "kra" | "exr" | "hdr") {
        ("▣", Color32::from_rgb(0x2a, 0x9d, 0x8f), "images")
    } else if office::is_office(&l) || e == "pdf" {
        ("▤", Color32::from_rgb(0x3b, 0x7d, 0xd8), "docs")
    } else if matches!(e, "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "html" | "htm" | "css" | "scss" | "c" | "cpp" | "h" | "hpp" | "cs" | "java" | "go" | "lua" | "sh" | "glsl" | "hlsl" | "json" | "toml" | "yaml" | "yml" | "gd" | "rb" | "php" | "sql" | "vue" | "svelte" | "xml" | "svg" | "osl" | "ps1" | "bat") {
        ("‹›", Color32::from_rgb(0x4f, 0x8a, 0xb8), "code")
    } else {
        ("·", Color32::from_gray(140), "autres")
    }
}

const FILTERS: [(&str, &str); 9] = [("notes", "#"), ("images", "▣"), ("3d", "◆"), ("code", "‹›"), ("docs", "▤"), ("vidéos", "▶"), ("boards", "▦"), ("nodes", "⬡"), ("autres", "·")];

impl App {
    pub(super) fn explorer(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let root = self.root();
        if self.files.loaded_for != root {
            self.files = FilesState::load(&root);
            if let Some(d) = self.current().map(|d| d.rel.clone()) {
                self.files.reveal(&d);
            }
        }
        egui::Panel::left("explorer")
            .resizable(true)
            .default_size(260.0)
            .min_size(180.0)
            .frame(egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                // En-tête : titre, « + » (créer / importer) et « ⋯ » (affichage)
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(caps(&crate::i18n::t("fichiers")));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        let g = self.files.grid;
                        egui::containers::menu::MenuButton::new("⋯").ui(ui, |ui| {
                            ui.set_min_width(200.0);
                            ui.label(RichText::new(crate::i18n::t("trier")).color(p.muted).size(11.0));
                            for (k, l) in [(0u8, "par nom"), (1, "modifiés récemment"), (2, "par type")] {
                                if ui.radio(self.files.sort == k, crate::i18n::t(l)).clicked() {
                                    self.files.sort = k;
                                    self.files.save();
                                }
                            }
                            ui.separator();
                            if ui.checkbox(&mut self.files.show_ext, crate::i18n::t("afficher les extensions")).changed() {
                                self.files.save();
                            }
                            let mut show_recent = !self.files.hide_recent;
                            if ui.checkbox(&mut show_recent, crate::i18n::t("section « récents »")).changed() {
                                self.files.hide_recent = !show_recent;
                                self.files.save();
                            }
                            ui.separator();
                            if ui.button(crate::i18n::t("tout replier")).clicked() {
                                self.files.expanded.clear();
                                self.files.save();
                            }
                            if ui.button(crate::i18n::t("tout déplier")).clicked() {
                                self.files.expanded = self.entries.iter().filter(|e| e.is_dir).map(|e| e.rel.clone()).collect();
                                self.files.save();
                            }
                            if ui.button(crate::i18n::t("montrer le fichier ouvert")).clicked() {
                                if let Some(d) = self.current().map(|d| d.rel.clone()) {
                                    self.files.reveal(&d);
                                    self.selected = Some(d);
                                }
                            }
                        });
                        if ui.add(egui::Button::new(if g { "▦" } else { "≣" }).frame(false)).on_hover_text(if g { "vue en grille (clic : liste)" } else { "vue en liste (clic : grille avec miniatures)" }).clicked() {
                            self.files.grid = !g;
                            self.files.save();
                        }
                        egui::containers::menu::MenuButton::new("+").ui(ui, |ui| {
                            ui.set_min_width(210.0);
                            let dir = self.target_dir(&self.notes_dir());
                            if ui.button(crate::i18n::t("note")).clicked() {
                                self.new_note();
                            }
                            if ui.button(crate::i18n::t("fichier…")).clicked() {
                                self.actions.push(Action::NewFile(dir.clone()));
                            }
                            if ui.button(crate::i18n::t("dossier…")).clicked() {
                                self.actions.push(Action::NewDir(self.target_dir("")));
                            }
                            ui.separator();
                            let ctx = ui.ctx().clone();
                            for (l, c) in [("note du jour", extras::Cmd::Daily), ("depuis un modèle…", extras::Cmd::FromTemplate), ("board…", extras::Cmd::NewBoard), ("matériau en nodes", extras::Cmd::NewMaterial), ("compositing en nodes", extras::Cmd::NewCompositor)] {
                                if ui.button(crate::i18n::t(l)).clicked() {
                                    self.run_cmd(c, &ctx);
                                }
                            }
                            ui.separator();
                            if ui.button(crate::i18n::t("importer des fichiers…")).clicked() {
                                self.pick_import(false);
                            }
                            if ui.button(crate::i18n::t("lier un dossier…")).clicked() {
                                self.pick_import(true);
                            }
                        });
                    });
                });
                ui.add_space(6.0);
                // Recherche
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
                        ui.set_width(ui.available_width() - 10.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("⌕").color(p.muted));
                            ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text(crate::i18n::t("chercher un fichier")).frame(egui::Frame::NONE).margin(egui::Margin::ZERO).desired_width(ui.available_width() - 16.0));
                            if !self.filter.is_empty() && ui.add(egui::Button::new(RichText::new("×").color(p.muted)).frame(false)).clicked() {
                                self.filter.clear();
                            }
                        });
                    });
                });
                // Filtres par type (icônes)
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(10.0);
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let all = self.files.kind.is_none();
                    if ui.add(egui::Button::new(RichText::new(crate::i18n::t("tout")).size(11.0).color(if all { p.on_accent } else { p.muted })).fill(if all { p.accent } else { p.panel }).stroke(egui::Stroke::NONE)).clicked() {
                        self.files.kind = None;
                    }
                    let present: HashSet<&str> = self.entries.iter().filter(|e| !e.is_dir).map(|e| kind(&e.rel).2).collect();
                    for (k, icon) in FILTERS {
                        if !present.contains(k) {
                            continue;
                        }
                        let on = self.files.kind == Some(k);
                        let col = kind(match k {
                            "notes" => "a.md",
                            "images" => "a.png",
                            "3d" => "a.obj",
                            "code" => "a.py",
                            "docs" => "a.pptx",
                            "vidéos" => "a.mp4",
                            "boards" => "a.board.json",
                            "nodes" => "a.mat.json",
                            _ => "a",
                        }).1;
                        let b = egui::Button::new(RichText::new(icon).size(12.0).color(if on { p.on_accent } else { col })).fill(if on { col } else { p.panel }).stroke(egui::Stroke::NONE).min_size(egui::vec2(24.0, 18.0));
                        if ui.add(b).on_hover_text(k).clicked() {
                            self.files.kind = if on { None } else { Some(k) };
                        }
                    }
                });
                ui.add_space(6.0);
                ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), egui::Stroke::new(1.0, p.line));
                ui.add_space(4.0);

                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let searching = !self.filter.trim().is_empty() || self.files.kind.is_some();
                    if self.files.grid {
                        self.grid_view(ui, searching);
                    } else if searching {
                        self.flat_list(ui);
                    } else {
                        // Épinglés et récents
                        let pinned: Vec<Entry> = self.files.pinned.iter().filter_map(|r| self.entries.iter().find(|e| e.rel == *r).cloned()).collect();
                        if !pinned.is_empty() {
                            section(ui, &crate::i18n::t("épinglés"));
                            for e in &pinned {
                                self.row(ui, e, 0, true);
                            }
                            ui.add_space(6.0);
                        }
                        if !self.files.hide_recent {
                            let recent: Vec<Entry> = self.files.recent.iter().filter(|r| !self.files.pinned.contains(r)).filter_map(|r| self.entries.iter().find(|e| e.rel == *r && !e.is_dir).cloned()).take(5).collect();
                            if !recent.is_empty() {
                                section(ui, &crate::i18n::t("récents"));
                                for e in &recent {
                                    self.row(ui, e, 0, true);
                                }
                                ui.add_space(6.0);
                            }
                        }
                        section(ui, &format!("coffre · {}", self.root().split(['/', '\\']).filter(|s| !s.is_empty()).last().unwrap_or("")));
                        self.tree_level(ui, "", 0);
                    }
                    // Zone vide en bas : déposer ici = racine du coffre ; clic droit = menu racine.
                    let (rect, bg) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ui.available_height().max(40.0)), egui::Sense::click());
                    if let Some(from) = bg.dnd_release_payload::<String>() {
                        self.actions.push(Action::Move((*from).clone(), String::new()));
                    }
                    if bg.dnd_hover_payload::<String>().is_some() {
                        ui.painter().rect_stroke(rect.shrink(4.0), 0.0, egui::Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
                    }
                    if bg.clicked() {
                        self.selected = None;
                    }
                    bg.context_menu(|ui| {
                        if ui.button(crate::i18n::t("nouvelle note")).clicked() {
                            self.new_note();
                        }
                        if ui.button(crate::i18n::t("nouveau fichier")).clicked() {
                            self.actions.push(Action::NewFile(String::new()));
                        }
                        if ui.button(crate::i18n::t("nouveau dossier")).clicked() {
                            self.actions.push(Action::NewDir(String::new()));
                        }
                    });
                    if self.entries.is_empty() {
                        ui.label(RichText::new(crate::i18n::t("   coffre vide : + pour créer, ou glisse des fichiers ici")).color(p.muted));
                    }
                });
            });
    }

    /// Vue en grille : fil d'Ariane, taille des tuiles, miniatures.
    fn grid_view(&mut self, ui: &mut egui::Ui, searching: bool) {
        let p = theme::pal();
        if !self.files.dir.is_empty() && !self.entries.iter().any(|e| e.is_dir && e.rel == self.files.dir) {
            self.files.dir.clear();
        }
        ui.horizontal_wrapped(|ui| {
            ui.add_space(10.0);
            ui.spacing_mut().item_spacing.x = 2.0;
            if !self.files.dir.is_empty() && ui.small_button("↑").on_hover_text(crate::i18n::t("dossier parent")).clicked() {
                self.files.dir = vault::parent(&self.files.dir);
                self.files.save();
            }
            if ui.add(egui::Button::new(RichText::new(crate::i18n::t("coffre")).size(11.5)).frame(false)).clicked() {
                self.files.dir.clear();
                self.files.save();
            }
            let parts: Vec<String> = self.files.dir.split('/').filter(|s| !s.is_empty()).map(String::from).collect();
            for k in 0..parts.len() {
                ui.label(RichText::new("/").color(p.muted).size(11.5));
                let last = k + 1 == parts.len();
                let t = RichText::new(vault::display_rel(&parts[k])).size(11.5);
                if ui.add(egui::Button::new(if last { t.family(theme::bold()) } else { t }).frame(false)).clicked() {
                    self.files.dir = parts[..=k].join("/");
                    self.files.save();
                }
            }
        });
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.spacing_mut().slider_width = 110.0;
            if ui.add(egui::Slider::new(&mut self.files.tile, 56.0..=220.0).show_value(false).text(RichText::new(crate::i18n::t("taille")).size(10.5))).drag_stopped() {
                self.files.save();
            }
        });
        ui.add_space(6.0);
        let q = chat::fold(self.filter.trim());
        let mut items: Vec<Entry> = if searching {
            self.entries.iter().filter(|e| !e.is_dir && self.files.kind.is_none_or(|k| kind(&e.rel).2 == k) && (q.is_empty() || chat::fold(&e.rel).contains(&q))).take(600).cloned().collect()
        } else {
            let dir = self.files.dir.clone();
            self.entries
                .iter()
                .filter(|e| {
                    let par = vault::parent(&e.rel);
                    let par = if par == vault::LINKED_FILES { String::new() } else { par };
                    par == dir && e.rel != vault::LINKED_FILES
                })
                .cloned()
                .collect()
        };
        match self.files.sort {
            1 => items.sort_by(|a, b| (!a.is_dir).cmp(&!b.is_dir).then(b.modified.cmp(&a.modified))),
            2 => items.sort_by(|a, b| (!a.is_dir).cmp(&!b.is_dir).then(kind(&a.rel).2.cmp(kind(&b.rel).2)).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))),
            _ => items.sort_by(|a, b| (!a.is_dir).cmp(&!b.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))),
        }
        let t = self.files.tile;
        egui::Frame::new().inner_margin(egui::Margin { left: 10, right: 6, top: 0, bottom: 0 }).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                for e in &items {
                    self.tile(ui, e, t, searching);
                }
            });
        });
        if items.is_empty() {
            ui.label(RichText::new(crate::i18n::t("   dossier vide")).color(p.muted));
        }
    }

    fn tile(&mut self, ui: &mut egui::Ui, e: &Entry, t: f32, with_path: bool) {
        let p = theme::pal();
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(t, t + 30.0), egui::Sense::click_and_drag());
        if !ui.is_rect_visible(rect) {
            return;
        }
        let painter = ui.painter().clone();
        let active = self.current().is_some_and(|d| d.rel == e.rel);
        let selected = self.selected.as_deref() == Some(&e.rel);
        let drop_target = e.is_dir && resp.dnd_hover_payload::<String>().is_some_and(|f| *f != e.rel);
        let (icon, col, _) = if e.is_dir { ("▰", p.muted, "") } else { kind(&e.rel) };
        let thumb = egui::Rect::from_min_size(rect.min, egui::vec2(t, t));
        painter.rect_filled(thumb, 0.0, if drop_target { p.accent.linear_multiply(0.2) } else { p.bg });
        let mut drawn = false;
        if !e.is_dir && images::has_preview(&e.rel) {
            if let images::Slot::Ready(l) = self.images.get(&e.rel, images::Size::Thumb) {
                let s = l.tex.size_vec2();
                let k = (thumb.width() / s.x).min(thumb.height() / s.y);
                let r = egui::Rect::from_center_size(thumb.center(), s * k);
                painter.image(l.tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                drawn = true;
            }
        }
        if e.is_dir {
            // Dossier dessiné : onglet + corps
            let w = t * 0.5;
            let body = egui::Rect::from_center_size(thumb.center() + egui::vec2(0.0, t * 0.04), egui::vec2(w, w * 0.68));
            let tab = egui::Rect::from_min_size(body.min - egui::vec2(0.0, w * 0.12), egui::vec2(w * 0.42, w * 0.14));
            let fc = egui::Color32::from_rgb(214, 170, 72);
            painter.rect_filled(tab, 1.0, fc.gamma_multiply(0.8));
            painter.rect_filled(body, 1.0, fc);
            let n = self.entries.iter().filter(|c| !c.is_dir && c.rel.starts_with(&format!("{}/", e.rel))).count();
            painter.text(body.center(), egui::Align2::CENTER_CENTER, n.to_string(), egui::FontId::monospace((t * 0.13).max(9.0)), egui::Color32::WHITE);
        } else if !drawn {
            painter.text(thumb.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::monospace(t * 0.32), col);
        }
        // Bande de couleur du type, comme dans Unreal
        if !e.is_dir {
            painter.rect_filled(egui::Rect::from_min_max(egui::pos2(thumb.left(), thumb.bottom() - 3.0), thumb.right_bottom()), 0.0, col);
        }
        let border = if active || selected { egui::Stroke::new(2.0, p.accent) } else if resp.hovered() { egui::Stroke::new(1.0, p.muted) } else { egui::Stroke::new(1.0, p.line) };
        painter.rect_stroke(thumb, 0.0, border, egui::StrokeKind::Inside);
        let name = e.name.trim_end_matches(".md").trim_end_matches(".board.json").trim_end_matches(".mat.json").trim_end_matches(".comp.json").to_string();
        let font = egui::FontId::monospace(10.5);
        let g = painter.layout(name, font.clone(), p.text, t);
        let clip = egui::Rect::from_min_size(egui::pos2(rect.left(), thumb.bottom() + 3.0), egui::vec2(t, 26.0));
        painter.with_clip_rect(clip).galley(clip.min, g, p.text);
        let resp = if with_path { resp.on_hover_text(vault::display_rel(&e.rel)) } else { resp.on_hover_text(&e.name) };
        if resp.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), e.rel.clone());
        }
        if e.is_dir {
            if let Some(from) = resp.dnd_release_payload::<String>() {
                if *from != e.rel {
                    self.actions.push(Action::Move((*from).clone(), e.rel.clone()));
                }
            }
        }
        if resp.clicked() {
            self.selected = Some(e.rel.clone());
            if e.is_dir {
                self.files.dir = e.rel.clone();
                self.files.save();
            } else {
                self.actions.push(Action::Open(e.rel.clone()));
            }
        }
        let e = e.clone();
        resp.context_menu(|ui| {
            self.entry_menu(ui, &e);
        });
    }

    /// Enfants d'un dossier, triés selon le réglage, puis récursion sur les dossiers ouverts.
    fn tree_level(&mut self, ui: &mut egui::Ui, dir: &str, depth: usize) {
        let mut kids: Vec<Entry> = self
            .entries
            .iter()
            .filter(|e| {
                let par = vault::parent(&e.rel);
                let par = if par == vault::LINKED_FILES { String::new() } else { par };
                par == dir && e.rel != vault::LINKED_FILES
            })
            .cloned()
            .collect();
        match self.files.sort {
            1 => kids.sort_by(|a, b| (!a.is_dir).cmp(&!b.is_dir).then(b.modified.cmp(&a.modified))),
            2 => kids.sort_by(|a, b| (!a.is_dir).cmp(&!b.is_dir).then(kind(&a.rel).2.cmp(kind(&b.rel).2)).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))),
            _ => kids.sort_by(|a, b| (!a.is_dir).cmp(&!b.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))),
        }
        for e in kids {
            self.row(ui, &e, depth, false);
            if e.is_dir && self.files.expanded.contains(&e.rel) {
                self.tree_level(ui, &e.rel, depth + 1);
            }
        }
    }

    /// Recherche ou filtre par type : liste à plat avec le dossier en gris.
    fn flat_list(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let q = chat::fold(self.filter.trim());
        let mut hits: Vec<Entry> = self
            .entries
            .iter()
            .filter(|e| !e.is_dir)
            .filter(|e| self.files.kind.is_none_or(|k| kind(&e.rel).2 == k))
            .filter(|e| q.is_empty() || chat::fold(&e.rel).contains(&q))
            .cloned()
            .collect();
        // Les noms qui commencent par la recherche d'abord
        hits.sort_by_key(|e| (!chat::fold(&e.name).starts_with(&q), e.name.to_lowercase()));
        section(ui, &format!("{} résultat(s)", hits.len()));
        for e in hits.iter().take(400) {
            self.row(ui, e, 0, true);
        }
        if hits.is_empty() {
            ui.label(RichText::new(crate::i18n::t("   rien trouvé")).color(p.muted));
        }
    }

    /// Une ligne de l'arbre (fichier ou dossier).
    fn row(&mut self, ui: &mut egui::Ui, e: &Entry, depth: usize, with_path: bool) {
        let p = theme::pal();
        let active = self.current().is_some_and(|d| d.rel == e.rel);
        let dirty = self.docs.iter().any(|d| d.rel == e.rel && d.dirty_since.is_some());
        let h = if with_path { 34.0 } else { 24.0 };
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::click_and_drag());
        let selected = self.selected.as_deref() == Some(&e.rel);
        let painter = ui.painter().clone();
        let drop_target = e.is_dir && resp.dnd_hover_payload::<String>().is_some_and(|f| *f != e.rel);
        if drop_target {
            painter.rect_filled(rect, 0.0, p.accent.linear_multiply(0.15));
        } else if active {
            painter.rect_filled(rect, 0.0, p.bg);
        } else if selected || resp.hovered() {
            painter.rect_filled(rect, 0.0, p.line.linear_multiply(0.45));
        }
        if active {
            painter.rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(2.0, rect.height())), 0.0, p.accent);
        }
        let x = rect.left() + 12.0 + depth as f32 * 14.0;
        let cy = if with_path { rect.top() + 12.0 } else { rect.center().y };
        for d in 0..depth {
            let gx = rect.left() + 17.0 + d as f32 * 14.0;
            painter.line_segment([egui::pos2(gx, rect.top()), egui::pos2(gx, rect.bottom())], egui::Stroke::new(1.0, p.line.linear_multiply(0.7)));
        }
        let font = egui::FontId::monospace(12.5);
        let open = e.is_dir && self.files.expanded.contains(&e.rel);
        if e.is_dir {
            painter.text(egui::pos2(x, cy), egui::Align2::LEFT_CENTER, if open { "▾" } else { "▸" }, font.clone(), p.muted);
            let name = painter.layout_no_wrap(e.name.clone(), egui::FontId::new(12.5, theme::bold()), p.text);
            let w = name.size().x;
            painter.galley(egui::pos2(x + 14.0, cy - name.size().y / 2.0), name, p.text);
            let prefix = format!("{}/", e.rel);
            let n = self.entries.iter().filter(|c| !c.is_dir && c.rel.starts_with(&prefix)).count();
            painter.text(egui::pos2(x + 22.0 + w, cy), egui::Align2::LEFT_CENTER, n.to_string(), egui::FontId::monospace(10.5), p.muted.linear_multiply(0.8));
        } else {
            let (icon, col, _) = kind(&e.rel);
            painter.text(egui::pos2(x + 6.0, cy), egui::Align2::CENTER_CENTER, icon, egui::FontId::monospace(if icon.len() > 1 { 10.0 } else { 12.5 }), col);
            let (stem, ext) = match e.name.rsplit_once('.') {
                Some((s, x)) if !s.is_empty() => (s.to_string(), format!(".{x}")),
                _ => (e.name.clone(), String::new()),
            };
            let stem = stem.trim_end_matches(".board").trim_end_matches(".mat").trim_end_matches(".comp").to_string();
            let color = if active { p.text } else { p.text.linear_multiply(0.85) };
            let g = painter.layout_no_wrap(stem, font.clone(), color);
            let w = g.size().x;
            painter.galley(egui::pos2(x + 16.0, cy - g.size().y / 2.0), g, color);
            let special = vault::is_md(&e.rel) || board::is_board(&e.rel) || nodes::is_graph(&e.rel);
            if (self.files.show_ext || !special) && !ext.is_empty() {
                painter.text(egui::pos2(x + 16.0 + w, cy), egui::Align2::LEFT_CENTER, ext, font.clone(), p.muted.linear_multiply(0.7));
            }
        }
        if with_path {
            let par = vault::display_rel(&vault::parent(&e.rel));
            painter.text(egui::pos2(x + 16.0, rect.top() + 26.0), egui::Align2::LEFT_CENTER, if par.is_empty() { "/".to_string() } else { par }, egui::FontId::monospace(10.0), p.muted);
        }
        // Marqueurs à droite : modifié, épinglé, lié
        let mut rx = rect.right() - 10.0;
        if dirty {
            painter.text(egui::pos2(rx, cy), egui::Align2::RIGHT_CENTER, "●", font.clone(), p.accent);
            rx -= 14.0;
        }
        if self.files.pinned.contains(&e.rel) && !with_path {
            painter.text(egui::pos2(rx, cy), egui::Align2::RIGHT_CENTER, "★", egui::FontId::monospace(10.0), p.muted);
            rx -= 14.0;
        }
        if vault::is_external(&e.rel) && e.depth == 0 {
            painter.text(egui::pos2(rx, cy), egui::Align2::RIGHT_CENTER, "↗", egui::FontId::monospace(10.5), p.muted);
        }
        let resp = if vault::is_external(&e.rel) && e.depth == 0 {
            let real = vault::safe(&self.root(), &e.rel).map(|p| p.display().to_string()).unwrap_or_default();
            resp.on_hover_text(format!("lié, reste à son emplacement : {real}"))
        } else {
            resp
        };
        // Glisser-déposer pour déplacer
        if resp.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), e.rel.clone());
        }
        if resp.dragged() {
            if let Some(pos) = ui.ctx().pointer_interact_pos() {
                let layer = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("drag-ghost")));
                layer.text(pos + egui::vec2(14.0, 0.0), egui::Align2::LEFT_CENTER, format!("→ {}", e.name), font.clone(), p.accent);
            }
        }
        if e.is_dir {
            if let Some(from) = resp.dnd_release_payload::<String>() {
                if *from != e.rel {
                    self.actions.push(Action::Move((*from).clone(), e.rel.clone()));
                }
            }
        }
        if resp.clicked() {
            self.selected = Some(e.rel.clone());
            if e.is_dir {
                if !self.files.expanded.remove(&e.rel) {
                    self.files.expanded.insert(e.rel.clone());
                }
                self.files.save();
            } else {
                self.actions.push(Action::Open(e.rel.clone()));
            }
        }
        let e = e.clone();
        resp.context_menu(|ui| {
            let pinned = self.files.pinned.contains(&e.rel);
            if ui.button(if pinned { "désépingler" } else { "épingler en haut" }).clicked() {
                if pinned {
                    self.files.pinned.retain(|r| *r != e.rel);
                } else {
                    self.files.pinned.push(e.rel.clone());
                }
                self.files.save();
            }
            self.entry_menu(ui, &e);
        });
    }

    fn entry_menu(&mut self, ui: &mut egui::Ui, e: &Entry) {
        let p = theme::pal();
        let dir = if e.is_dir { e.rel.clone() } else { vault::parent(&e.rel) };

            ui.label(RichText::new(&e.rel).color(p.muted).size(11.0));
            ui.separator();
            if !e.is_dir && ui.button(crate::i18n::t("ouvrir")).clicked() {
                self.actions.push(Action::Open(e.rel.clone()));
            }
            if ui.button(crate::i18n::t("nouveau fichier ici")).clicked() {
                self.actions.push(Action::NewFile(dir.clone()));
            }
            if ui.button(crate::i18n::t("nouveau dossier ici")).clicked() {
                self.actions.push(Action::NewDir(dir.clone()));
            }
            if let Ok(path) = vault::safe(&self.root(), &e.rel) {
                let ok = if e.is_dir { true } else { convert::kind_of(&path).is_some() };
                if ok && ui.button(if e.is_dir { "convertir / compresser le dossier…" } else { "convertir / compresser…" }).clicked() {
                    self.convert = convert::Dlg::new(vec![path]);
                    if self.convert.is_none() {
                        self.toast = Some(("rien à convertir ici (images, vidéos, sons)".into(), Instant::now()));
                    }
                }
            }
            ui.separator();
            let link_root = vault::is_link_root(&e.rel) || e.rel == vault::LINKED_FILES;
            if !link_root && ui.button(crate::i18n::t("renommer / déplacer")).clicked() {
                self.actions.push(Action::Rename(e.rel.clone()));
            }
            if vault::is_external(&e.rel) && !e.is_dir {
                if ui.button(crate::i18n::t("déplacer dans le coffre")).on_hover_text(crate::i18n::t("Le fichier quitte son dossier d'origine")).clicked() {
                    self.dialog = Some(Dialog::MoveIn { rel: e.rel.clone(), copy: false });
                }
                if ui.button(crate::i18n::t("copier dans le coffre")).clicked() {
                    self.dialog = Some(Dialog::MoveIn { rel: e.rel.clone(), copy: true });
                }
            }
            if !e.is_dir && ui.button(if e.rel.to_lowercase().ends_with(".blend") { "ouvrir dans Blender" } else { "ouvrir avec l'app par défaut" }).clicked() {
                let r = vault::safe(&self.root(), &e.rel).and_then(|p| settings::open_external(&p));
                if let Err(err) = r {
                    self.error = Some(err);
                }
            }
            if !e.is_dir && ui.button(crate::i18n::t("ouvrir avec…")).clicked() {
                let r = vault::safe(&self.root(), &e.rel).and_then(|p| settings::open_with(&p));
                if let Err(err) = r {
                    self.error = Some(err);
                }
            }
            ui.separator();
            if ui.button(crate::i18n::t("exporter en zip…")).clicked() {
                self.export_zip(vec![e.rel.clone()], false);
            }
            if (vault::is_md(&e.rel) || board::is_board(&e.rel)) && ui.button(crate::i18n::t("exporter avec ses liens en zip…")).on_hover_text(crate::i18n::t("Ajoute les notes liées, les images citées et le contenu du board")).clicked() {
                self.export_zip(vec![e.rel.clone()], true);
            }
            if ui.button(crate::i18n::t("envoyer à un autre ordi…")).clicked() {
                let root = self.root();
                self.send_files = transfer::collect(&root, &self.entries, &[e.rel.clone()], false);
                self.dialog = Some(Dialog::Transfer { send: true });
            }
            ui.separator();
            if ui.button(crate::i18n::t("afficher dans le dossier")).clicked() {
                if let Ok(path) = vault::safe(&self.root(), &e.rel) {
                    settings::reveal(&path, !e.is_dir);
                }
            }
            if ui.button(crate::i18n::t("copier le chemin")).clicked() {
                let full = vault::safe(&self.root(), &e.rel).map(|p| p.to_string_lossy().to_string()).unwrap_or(e.rel.clone());
                ui.ctx().copy_text(full);
            }
            let label = if link_root { "retirer le lien (le fichier reste en place)" } else { "supprimer" };
            if ui.button(RichText::new(label).color(p.missing)).clicked() {
                if link_root {
                    let r = vault::trash(&self.root(), &e.rel);
                    if self.report(r).is_some() {
                        self.docs.retain(|d| !(d.rel == e.rel || d.rel.starts_with(&format!("{}/", e.rel))));
                        self.active = if self.docs.is_empty() { None } else { Some(0) };
                        self.links_dirty = true;
                    }
                } else {
                    self.actions.push(Action::Trash(e.rel.clone()));
                }
            }
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(RichText::new(title.to_uppercase()).size(10.0).color(theme::pal().muted).family(theme::bold()));
    });
    ui.add_space(4.0);
}
