#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod blend;
mod board;
mod brain;
mod chat;
mod code;
mod fbx;
mod ffmpeg;
mod images;
mod live;
mod model3d;
mod office;
mod ollama;
mod preview;
mod search;
mod settings;
mod slides;
mod theme;
mod transfer;
mod updater;
mod video;
mod vault;

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};
use vault::Entry;

const APP: &str = "aryadne";

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Aryadne")
            .with_app_id(APP)
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([760.0, 480.0])
            .with_drag_and_drop(true)
            .with_icon(std::sync::Arc::new(theme::window_icon().unwrap_or_default())),
        ..Default::default()
    };
    eframe::run_native(APP, options, Box::new(|cc| Ok(Box::new(App::new(&cc.egui_ctx)))))
}

// ---------- config utilisateur (aucun admin requis) ----------

fn config(name: &str) -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP).join(name))
}

fn save_config(name: &str, value: &str) {
    if let Some(f) = config(name) {
        let _ = std::fs::create_dir_all(f.parent().unwrap());
        let _ = std::fs::write(f, value);
    }
}

fn load_config(name: &str) -> Option<String> {
    config(name).and_then(|f| std::fs::read_to_string(f).ok()).map(|s| s.trim().to_string())
}

fn caps(text: &str) -> RichText {
    RichText::new(text.to_uppercase()).size(10.5).color(theme::pal().muted).family(theme::bold())
}

fn ext(rel: &str) -> String {
    rel.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default()
}

// ---------- état ----------

#[derive(PartialEq, Clone, Copy)]
enum View {
    Files,
    Brain,
    Refs,
    Boards,
    Chat,
    Search,
}

#[derive(PartialEq, Clone, Copy, Default)]
enum RefSort {
    #[default]
    Recent,
    Name,
    Color,
}

#[derive(Default)]
struct RefsState {
    filter: String,
    size: f32,
    selected: Option<String>,
    sort: RefSort,
}

enum Content {
    Text(String),
    Image,
    Model(model3d::Viewer),
    Office(office::Viewer),
    Video(Box<video::Viewer>),
    /// Vidéo ouverte alors que ffmpeg n'est pas encore installé.
    NeedFfmpeg(Option<std::sync::Arc<std::sync::Mutex<ffmpeg::InstallState>>>),
    Unreadable(String),
}

struct Doc {
    rel: String,
    content: Content,
    dirty_since: Option<Instant>,
    preview: bool,
    code: code::Editor,
}

enum Action {
    Open(String),
    OpenLink(String),
    NewFile(String),
    NewDir(String),
    Rename(String),
    Trash(String),
    Move(String, String),
    Close(usize),
    Summarize,
    Reload(String),
}

enum Dialog {
    NewFile { dir: String },
    NewDir { dir: String },
    Rename { from: String },
    ConfirmTrash { rel: String },
    Update,
    Import { files: Vec<PathBuf>, dir: String, remember: bool },
    MoveIn { rel: String, copy: bool },
    Transfer { send: bool },
    Settings(settings::SettingsState),
}

enum Summary {
    Loading,
    Done(String),
    Failed(String),
}

#[derive(Default)]
struct App {
    vault: Option<String>,
    entries: Vec<Entry>,
    md: HashMap<String, String>,
    docs: Vec<Doc>,
    active: Option<usize>,
    view: Option<View>,
    brain: brain::Brain,
    filter: String,
    collapsed: HashSet<String>,
    selected: Option<String>,
    backlinks: Vec<vault::Backlink>,
    actions: Vec<Action>,
    dialog: Option<Dialog>,
    dialog_text: String,
    switcher: Option<(String, usize)>,
    ai: Option<ai::Settings>,
    summary: Option<(String, Summary)>,
    summary_rx: Option<Receiver<Result<String, String>>>,
    local: ollama::Local,
    images: images::Images,
    board: board::Board,
    refs: RefsState,
    chat: chat::Chat,
    assistant: chat::Chat,
    assistant_open: bool,
    organize: Option<Organize>,
    auto_query: String,
    toast: Option<(String, Instant)>,
    updater: updater::Updater,
    prefs: settings::Prefs,
    links_dirty: bool,
    search: search::Search,
    pinned: bool,
    compact: bool,
    /// La fenêtre ouverte a déjà été affichée au moins une fois.
    dialog_shown: bool,
    receiver: Option<transfer::Receiver>,
    ctx: Option<egui::Context>,
    jobs: Vec<transfer::Shared>,
    peers: std::sync::Arc<std::sync::Mutex<Vec<transfer::Peer>>>,
    send_files: Vec<String>,
    send_addr: String,
    send_code: String,
    github_token: String,
    error: Option<String>,
}

impl App {
    fn new(ctx: &egui::Context) -> Self {
        egui_extras::install_image_loaders(ctx);
        theme::install_fonts(ctx);
        let prefs = settings::Prefs::load();
        prefs.apply(ctx);
        let mut app = Self { view: Some(View::Files), ai: Some(ai::Settings::load()), prefs, ..Default::default() };
        app.ctx = Some(ctx.clone());
        app.local.refresh(&app.ai.clone().unwrap_or_default().url, ctx);
        app.github_token = load_config("github_token").unwrap_or_default();
        // Vérifie les mises à jour au plus toutes les 6 h (pas en mode dev).
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let last: u64 = load_config("last_update_check").and_then(|v| v.parse().ok()).unwrap_or(0);
        if app.prefs.auto_update && !cfg!(debug_assertions) && now.saturating_sub(last) > 6 * 3600 {
            save_config("last_update_check", &now.to_string());
            app.updater.check(app.github_token.clone(), ctx);
        }
        if let Some(p) = load_config("last_vault") {
            app.load_vault(p);
        }
        app
    }

    fn view(&self) -> View {
        self.view.unwrap_or(View::Files)
    }

    fn report<T>(&mut self, r: Result<T, String>) -> Option<T> {
        r.map_err(|e| self.error = Some(e)).ok()
    }

    fn root(&self) -> String {
        self.vault.clone().unwrap_or_default()
    }

    fn load_vault(&mut self, path: String) {
        self.flush_all();
        if self.report(vault::init(&path)).is_none() {
            return;
        }
        save_config("last_vault", &path);
        self.vault = Some(path);
        self.docs.clear();
        self.active = None;
        self.summary = None;
        self.brain = brain::Brain::default();
        self.board = board::Board::default();
        self.refs = RefsState::default();
        self.chat = chat::Chat::default();
        let root = self.root();
        self.images.set_root(&root);
        vault::load_links(&root);
        self.refresh();
        if let Some(first) = self.entries.iter().find(|e| vault::is_md(&e.rel)).map(|e| e.rel.clone()) {
            self.open(first);
        }
    }

    fn refresh(&mut self) {
        self.entries = vault::scan(&self.root());
        self.md = vault::md_index(&self.entries);
        self.brain.dirty = true;
        self.update_backlinks();
    }

    fn update_backlinks(&mut self) {
        self.backlinks = match self.current() {
            Some(d) if vault::is_md(&d.rel) => vault::backlinks(&self.root(), &self.entries, &d.rel),
            _ => vec![],
        };
    }

    fn current(&self) -> Option<&Doc> {
        self.active.and_then(|i| self.docs.get(i))
    }

    fn flush(&mut self, i: usize) {
        let root = self.root();
        let Some(doc) = self.docs.get_mut(i) else { return };
        if doc.dirty_since.take().is_none() {
            return;
        }
        if let Content::Text(t) = &doc.content {
            let (rel, t) = (doc.rel.clone(), t.clone());
            let r = vault::write(&root, &rel, &t);
            live::bump();
            if self.report(r).is_some() && vault::is_md(&rel) {
                self.brain.dirty = true;
            }
        }
    }

    fn flush_all(&mut self) {
        for i in 0..self.docs.len() {
            self.flush(i);
        }
    }

    fn video_content(&self, rel: &str) -> Content {
        if ffmpeg::find("ffmpeg").is_none() {
            return Content::NeedFfmpeg(None);
        }
        let (Ok(path), Some(ctx)) = (vault::safe(&self.root(), rel), self.ctx.clone()) else {
            return Content::Unreadable("Vidéo introuvable.".into());
        };
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        rel.hash(&mut h);
        let notes = std::path::Path::new(&self.root()).join(".app/annotations").join(format!("{:016x}.json", h.finish()));
        Content::Video(Box::new(video::Viewer::open(path, notes, &ctx)))
    }

    fn open(&mut self, rel: String) {
        self.view = Some(View::Files);
        self.selected = Some(rel.clone());
        if let Some(i) = self.docs.iter().position(|d| d.rel == rel) {
            self.active = Some(i);
        } else {
            if board::is_board(&rel) {
                self.open_board(rel);
                return;
            }
            let content = if model3d::is_model(&rel) {
                match (vault::safe(&self.root(), &rel), self.ctx.clone()) {
                    (Ok(path), Some(ctx)) => Content::Model(model3d::Viewer::open(path, &ctx)),
                    (Err(e), _) => Content::Unreadable(e),
                    _ => Content::Unreadable("Contexte indisponible.".into()),
                }
            } else if video::is_video(&rel) {
                self.video_content(&rel)
            } else if office::is_office(&rel) {
                match vault::safe(&self.root(), &rel) {
                    Ok(path) => Content::Office(office::Viewer::open(&path)),
                    Err(e) => Content::Unreadable(e),
                }
            } else if images::is_image(&rel) {
                Content::Image
            } else {
                match vault::read(&self.root(), &rel) {
                    Ok(t) => Content::Text(t),
                    Err(e) => Content::Unreadable(e),
                }
            };
            self.docs.push(Doc { rel, content, dirty_since: None, preview: false, code: code::Editor::default() });
            self.active = Some(self.docs.len() - 1);
        }
        self.update_backlinks();
    }

    /// Dossier où créer ou importer : le dossier sélectionné, ou celui du fichier sélectionné.
    fn target_dir(&self, fallback: &str) -> String {
        match &self.selected {
            Some(s) if self.entries.iter().any(|e| &e.rel == s && e.is_dir) => s.clone(),
            Some(s) => vault::parent(s),
            None if self.entries.iter().any(|e| e.rel == fallback) => fallback.to_string(),
            None => String::new(),
        }
    }

    /// Dossier des nouvelles notes (réglage), s'il existe dans le coffre.
    fn notes_dir(&self) -> String {
        let d = self.prefs.notes_dir.trim().trim_matches('/').to_string();
        if !d.is_empty() && self.entries.iter().any(|e| e.rel == d && e.is_dir) { d } else { String::new() }
    }

    fn new_note(&mut self) {
        let nd = self.notes_dir();
        let dir = self.target_dir(&nd);
        let mut name = "Sans titre.md".to_string();
        let mut i = 2;
        while self.entries.iter().any(|e| e.rel == vault::join(&dir, &name)) {
            name = format!("Sans titre {i}.md");
            i += 1;
        }
        let rel = vault::join(&dir, &name);
        if self.report(vault::create_file(&self.root(), &rel)).is_some() {
            self.refresh();
            self.open(rel);
        }
    }

    fn pick_vault(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().set_title("Choisir le dossier du coffre").pick_folder() {
            self.load_vault(dir.to_string_lossy().to_string());
        }
    }

    fn set_dark(&mut self, ctx: &egui::Context, dark: bool) {
        self.prefs.dark = dark;
        self.prefs.save();
        self.prefs.apply(ctx);
    }

    // ---------- actions différées (évite les conflits d'emprunt pendant le dessin) ----------

    fn run_actions(&mut self, ctx: &egui::Context) {
        if std::mem::take(&mut self.links_dirty) {
            self.refresh();
        }
        let root = self.root();
        for a in std::mem::take(&mut self.actions) {
            match a {
                Action::Open(rel) => self.open(rel),
                Action::OpenLink(name) => {
                    if self.entries.iter().any(|e| e.rel == name && !e.is_dir) {
                        self.open(name);
                        continue;
                    }
                    let rel = match self.md.get(&name.to_lowercase()) {
                        Some(r) => r.clone(),
                        None => {
                            let nd = self.notes_dir();
                            let dir = nd.as_str();
                            let rel = vault::join(dir, &format!("{name}.md"));
                            if self.report(vault::create_file(&root, &rel)).is_none() {
                                continue;
                            }
                            self.refresh();
                            rel
                        }
                    };
                    self.open(rel);
                }
                Action::NewFile(dir) => {
                    self.dialog_text = String::new();
                    self.dialog = Some(Dialog::NewFile { dir });
                }
                Action::NewDir(dir) => {
                    self.dialog_text = String::new();
                    self.dialog = Some(Dialog::NewDir { dir });
                }
                Action::Rename(from) => {
                    self.dialog_text = from.clone();
                    self.dialog = Some(Dialog::Rename { from });
                }
                Action::Trash(rel) => {
                    if self.prefs.confirm_delete {
                        self.dialog = Some(Dialog::ConfirmTrash { rel });
                    } else {
                        self.do_trash(&rel);
                    }
                }
                Action::Move(from, to_dir) => {
                    let name = from.rsplit('/').next().unwrap_or(&from).to_string();
                    let to = vault::join(&to_dir, &name);
                    if to != from {
                        self.do_rename(&from, &to);
                    }
                }
                Action::Close(i) => {
                    self.flush(i);
                    if i < self.docs.len() {
                        self.docs.remove(i);
                        self.active = if self.docs.is_empty() { None } else { Some(self.active.unwrap_or(0).min(self.docs.len() - 1)) };
                        self.update_backlinks();
                    }
                }
                Action::Reload(rel) => {
                    if let Some(i) = self.docs.iter().position(|d| d.rel == rel) {
                        self.docs.remove(i);
                    }
                    self.images.forget(&rel);
                    self.open(rel);
                }
                Action::Summarize => {
                    let Some(doc) = self.current() else { continue };
                    let Content::Text(t) = &doc.content else { continue };
                    let (rel, text) = (doc.rel.clone(), t.clone());
                    let settings = self.ai.clone().unwrap_or_default();
                    self.summary_rx = Some(ai::summarize(settings, rel.clone(), text, self.local.clone(), ctx.clone()));
                    self.summary = Some((rel, Summary::Loading));
                }
            }
        }
    }

    fn open_settings(&mut self, ctx: &egui::Context) {
        self.open_settings_tab(ctx, settings::Tab::Ai);
    }

    fn do_rename(&mut self, from: &str, to: &str) {
        let r = vault::rename(&self.root(), from, to);
        if self.report(r).is_none() {
            return;
        }
        self.images.forget(from);
        for d in &mut self.docs {
            if d.rel == from {
                d.rel = to.to_string();
            } else if let Some(rest) = d.rel.strip_prefix(&format!("{from}/")) {
                d.rel = format!("{to}/{rest}");
            }
        }
        self.selected = Some(to.to_string());
        self.refresh();
    }

    fn do_trash(&mut self, rel: &str) {
        let r = vault::trash(&self.root(), rel);
        if self.report(r).is_none() {
            return;
        }
        let gone = |d: &Doc| d.rel == rel || d.rel.starts_with(&format!("{rel}/"));
        self.docs.retain(|d| !gone(d));
        self.active = if self.docs.is_empty() { None } else { Some(self.active.unwrap_or(0).min(self.docs.len() - 1)) };
        self.selected = None;
        self.refresh();
    }

    // ---------- clavier ----------

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.vault.is_none() || self.dialog.is_some() {
            return;
        }
        let pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        if pressed(Modifiers::COMMAND, Key::P) {
            self.switcher = Some((String::new(), 0));
        }
        if pressed(Modifiers::COMMAND, Key::N) {
            self.new_note();
        }
        if pressed(Modifiers::COMMAND | Modifiers::SHIFT, Key::F) {
            self.view = Some(View::Search);
            self.search.last_query.clear();
        }
        if pressed(Modifiers::COMMAND, Key::L) {
            self.assistant_open = !self.assistant_open;
        }
        if pressed(Modifiers::COMMAND | Modifiers::SHIFT, Key::M) {
            self.compact = !self.compact;
        }
        if pressed(Modifiers::COMMAND, Key::G) {
            self.view = Some(if self.view() == View::Brain { View::Files } else { View::Brain });
        }
        for (k, v) in [(Key::Num1, View::Files), (Key::Num2, View::Brain), (Key::Num3, View::Refs), (Key::Num4, View::Boards), (Key::Num5, View::Chat)] {
            if pressed(Modifiers::COMMAND, k) {
                self.view = Some(v);
            }
        }
        if pressed(Modifiers::COMMAND, Key::E) {
            if let Some(d) = self.active.and_then(|i| self.docs.get_mut(i)) {
                if vault::is_md(&d.rel) {
                    d.preview = !d.preview;
                }
            }
        }
        if pressed(Modifiers::COMMAND, Key::S) {
            self.flush_all();
            self.update_backlinks();
        }
        if pressed(Modifiers::COMMAND, Key::W) {
            if let Some(i) = self.active {
                self.actions.push(Action::Close(i));
            }
        }
    }

    // ---------- barres ----------

    fn topbar(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        egui::Panel::top("top")
            .exact_size(38.0)
            .frame(egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(12, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    theme::mark(ui, 16.0);
                    ui.label(RichText::new("aryadne").family(theme::bold()));
                    let path = self.root();
                    let short = path.split(['/', '\\']).filter(|s| !s.is_empty()).last().unwrap_or("").to_string();
                    if ui.add(egui::Button::new(RichText::new(format!("~/{short}")).color(p.muted)).frame(false)).on_hover_text(format!("{path}\nChanger de coffre")).clicked() {
                        self.pick_vault();
                    }
                    ui.add_space(18.0);
                    let tabs = [("fichiers", Some(View::Files)), ("cerveau", Some(View::Brain)), ("refs", Some(View::Refs)), ("boards", Some(View::Boards)), ("chat", Some(View::Chat)), ("recherche", Some(View::Search))];
                    for (label, v) in tabs {
                        let active = v == Some(self.view());
                        let color = if active { p.text } else { p.muted };
                        let text = RichText::new(label).color(color);
                        let r = ui.add(egui::Label::new(if active { text.family(theme::bold()) } else { text }).sense(egui::Sense::click()));
                        if active {
                            let y = ui.max_rect().bottom();
                            ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(r.rect.left(), y - 2.0), egui::pos2(r.rect.right(), y)), 0.0, p.accent);
                        }
                        match v {
                            Some(v) => {
                                if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    self.view = Some(v);
                                }
                            }
                            None => {
                                r.on_hover_text("bientôt");
                            }
                        }
                        ui.add_space(8.0);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(r) = self.updater.update_available() {
                            let b = egui::Button::new(RichText::new(format!("↑ {}", r.version)).color(p.on_accent).family(theme::bold())).fill(p.accent).stroke(egui::Stroke::NONE);
                            if ui.add(b).on_hover_text("Nouvelle version disponible").clicked() {
                                self.dialog = Some(Dialog::Update);
                            }
                        } else if ui.add(egui::Button::new(RichText::new(format!("v{}", updater::CURRENT)).color(p.muted).size(11.0)).frame(false)).on_hover_text("Mises à jour").clicked() {
                            self.dialog = Some(Dialog::Update);
                        }
                        let label = if theme::is_dark() { "clair" } else { "sombre" };
                        if ui.button(label).clicked() {
                            self.set_dark(ui.ctx(), !theme::is_dark());
                        }
                        let pin = if self.pinned { RichText::new("épinglé").color(p.on_accent) } else { RichText::new("épingler") };
                        let pin_btn = egui::Button::new(pin).fill(if self.pinned { p.accent } else { p.bg });
                        if ui.add(pin_btn).on_hover_text("Garder Aryadne au-dessus des autres fenêtres (Blender, etc.)").clicked() {
                            self.set_pinned(ui.ctx(), !self.pinned);
                        }
                        if ui.button("compact").on_hover_text("Masquer les panneaux (Ctrl+Maj+M)").clicked() {
                            self.compact = true;
                        }
                        let ai_btn = egui::Button::new(if self.assistant_open { RichText::new("assistant").color(p.on_accent) } else { RichText::new("assistant") }).fill(if self.assistant_open { p.accent } else { p.bg });
                        if ui.add(ai_btn).on_hover_text("Assistant IA à côté de tout (Ctrl+L)").clicked() {
                            self.assistant_open = !self.assistant_open;
                        }
                        if ui.button("transfert").on_hover_text("Envoyer ou recevoir des fichiers entre deux ordinateurs").clicked() {
                            self.dialog = Some(Dialog::Transfer { send: !self.send_files.is_empty() });
                        }
                        if ui.button("réglages").clicked() {
                            self.open_settings_tab(ui.ctx(), settings::Tab::Appearance);
                        }
                    });
                });
            });
    }

    fn statusbar(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        egui::Panel::bottom("status")
            .exact_size(24.0)
            .frame(egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    let preview = self.current().is_some_and(|d| d.preview);
                    let (mode, color) = match self.view() {
                        View::Brain => ("CERVEAU", p.text),
                        View::Refs => ("REFS", p.text),
                        View::Boards => ("BOARD", p.text),
                        View::Chat => ("CHAT", p.link),
                        View::Search => ("RECHERCHE", p.text),
                        View::Files if preview => ("LECTURE", p.link),
                        View::Files => ("ÉDITION", p.accent),
                    };
                    egui::Frame::new().fill(color).inner_margin(egui::Margin::symmetric(10, 4)).show(ui, |ui| {
                        ui.label(RichText::new(mode).color(p.on_accent).family(theme::bold()).size(11.0));
                    });
                    if let Some(d) = self.current() {
                        ui.label(RichText::new(&d.rel).size(11.5));
                        if let Content::Text(t) = &d.content {
                            let lang = if vault::is_md(&d.rel) { "markdown".to_string() } else if ext(&d.rel).is_empty() { "texte".into() } else { ext(&d.rel) };
                            ui.label(RichText::new(lang).color(p.muted).size(11.5));
                            ui.label(RichText::new(format!("{} lignes · {} mots", t.lines().count(), t.split_whitespace().count())).color(p.muted).size(11.5));
                            ui.label(RichText::new(if d.dirty_since.is_some() { "● modifié" } else { "sauvé" }).color(p.muted).size(11.5));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        ui.label(RichText::new("^P ouvrir  ^N note  ^E lecture  ^1-5 vues  ^W fermer").color(p.muted).size(11.0));
                    });
                });
            });
    }

    // ---------- explorateur ----------

    fn explorer(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        egui::Panel::left("explorer")
            .resizable(true)
            .default_size(250.0)
            .min_size(170.0)
            .frame(egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(0, 10)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(caps("explorateur"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        if ui.small_button("+dir").on_hover_text("Nouveau dossier").clicked() {
                            self.actions.push(Action::NewDir(self.target_dir("")));
                        }
                        if ui.small_button("+fichier").on_hover_text("Nouveau fichier").clicked() {
                            self.actions.push(Action::NewFile(self.target_dir(&self.notes_dir())));
                        }
                    });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
                        ui.label(RichText::new("/").color(p.accent));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.filter)
                                .hint_text("filtrer")
                                .frame(egui::Frame::NONE)
                                .margin(egui::Margin::ZERO),
                        );
                    });
                });
                ui.add_space(8.0);

                let resp = egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    self.tree(ui);
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
                        if ui.button("nouveau fichier").clicked() {
                            self.actions.push(Action::NewFile(String::new()));
                        }
                        if ui.button("nouveau dossier").clicked() {
                            self.actions.push(Action::NewDir(String::new()));
                        }
                    });
                });
                let _ = resp;
            });
    }

    fn tree(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let filter = self.filter.to_lowercase();
        let active_rel = self.current().map(|d| d.rel.clone());
        let dirty: HashSet<String> = self.docs.iter().filter(|d| d.dirty_since.is_some()).map(|d| d.rel.clone()).collect();
        let mut hidden_under: Option<String> = None;

        let entries = self.entries.clone();
        for e in &entries {
            if let Some(h) = &hidden_under {
                if e.rel.starts_with(&format!("{h}/")) {
                    continue;
                }
                hidden_under = None;
            }
            if !filter.is_empty() {
                if e.is_dir || !e.rel.to_lowercase().contains(&filter) {
                    continue;
                }
            } else if e.is_dir && self.collapsed.contains(&e.rel) {
                hidden_under = Some(e.rel.clone());
            }

            let depth = if filter.is_empty() { e.depth } else { 0 };
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::click_and_drag());
            let selected = self.selected.as_deref() == Some(&e.rel);
            let active = active_rel.as_deref() == Some(&e.rel);
            let painter = ui.painter();
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
            let x = rect.left() + 14.0 + depth as f32 * 14.0;
            // Guides d'indentation
            for d in 0..depth {
                let gx = rect.left() + 18.0 + d as f32 * 14.0;
                painter.line_segment([egui::pos2(gx, rect.top()), egui::pos2(gx, rect.bottom())], egui::Stroke::new(1.0, p.line));
            }
            let font = egui::FontId::monospace(12.5);
            let (icon, icon_color) = if e.is_dir {
                (if self.collapsed.contains(&e.rel) { "▸" } else { "▾" }, p.muted)
            } else if vault::is_md(&e.rel) {
                ("#", p.accent)
            } else if e.rel.to_lowercase().ends_with(".blend") || model3d::is_model(&e.rel) {
                ("◆", p.accent)
            } else if office::is_office(&e.rel) {
                ("▤", p.link)
            } else if video::is_video(&e.rel) {
                ("▶", p.accent)
            } else if images::is_image(&e.rel) {
                ("▣", p.link)
            } else if board::is_board(&e.rel) {
                ("▦", p.accent)
            } else {
                ("·", p.muted)
            };
            painter.text(egui::pos2(x, rect.center().y), egui::Align2::LEFT_CENTER, icon, font.clone(), icon_color);
            let label = if e.is_dir { format!("{}/", e.name) } else if filter.is_empty() { e.name.clone() } else { vault::display_rel(&e.rel) };
            let color = if active || e.is_dir { p.text } else { p.muted };
            let galley = painter.layout_no_wrap(label, font.clone(), color);
            painter.galley(egui::pos2(x + 14.0, rect.center().y - galley.size().y / 2.0), galley, color);
            if dirty.contains(&e.rel) {
                painter.text(egui::pos2(rect.right() - 12.0, rect.center().y), egui::Align2::RIGHT_CENTER, "●", font.clone(), p.accent);
            }

            let resp = if e.depth == 0 && vault::is_external(&e.rel) {
                let real = vault::safe(&self.root(), &e.rel).map(|p| p.display().to_string()).unwrap_or_default();
                resp.on_hover_text(format!("à son emplacement d'origine : {real}"))
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
                    if !self.collapsed.remove(&e.rel) {
                        self.collapsed.insert(e.rel.clone());
                    }
                } else {
                    self.actions.push(Action::Open(e.rel.clone()));
                }
            }
            let dir = if e.is_dir { e.rel.clone() } else { vault::parent(&e.rel) };
            resp.context_menu(|ui| {
                ui.label(RichText::new(&e.rel).color(p.muted).size(11.0));
                ui.separator();
                if !e.is_dir && ui.button("ouvrir").clicked() {
                    self.actions.push(Action::Open(e.rel.clone()));
                }
                if ui.button("nouveau fichier ici").clicked() {
                    self.actions.push(Action::NewFile(dir.clone()));
                }
                if ui.button("nouveau dossier ici").clicked() {
                    self.actions.push(Action::NewDir(dir.clone()));
                }
                ui.separator();
                let link_root = vault::is_link_root(&e.rel) || e.rel == vault::LINKED_FILES;
                if !link_root && ui.button("renommer / déplacer").clicked() {
                    self.actions.push(Action::Rename(e.rel.clone()));
                }
                if vault::is_external(&e.rel) && !e.is_dir {
                    if ui.button("déplacer dans le coffre").on_hover_text("Le fichier quitte son dossier d'origine").clicked() {
                        self.dialog = Some(Dialog::MoveIn { rel: e.rel.clone(), copy: false });
                    }
                    if ui.button("copier dans le coffre").clicked() {
                        self.dialog = Some(Dialog::MoveIn { rel: e.rel.clone(), copy: true });
                    }
                }
                if !e.is_dir && ui.button(if e.rel.to_lowercase().ends_with(".blend") { "ouvrir dans Blender" } else { "ouvrir avec l'app par défaut" }).clicked() {
                    let r = vault::safe(&self.root(), &e.rel).and_then(|p| settings::open_external(&p));
                    if let Err(err) = r {
                        self.error = Some(err);
                    }
                }
                if !e.is_dir && ui.button("ouvrir avec…").clicked() {
                    let r = vault::safe(&self.root(), &e.rel).and_then(|p| settings::open_with(&p));
                    if let Err(err) = r {
                        self.error = Some(err);
                    }
                }
                ui.separator();
                if ui.button("exporter en zip…").clicked() {
                    self.export_zip(vec![e.rel.clone()], false);
                }
                if (vault::is_md(&e.rel) || board::is_board(&e.rel)) && ui.button("exporter avec ses liens en zip…").on_hover_text("Ajoute les notes liées, les images citées et le contenu du board").clicked() {
                    self.export_zip(vec![e.rel.clone()], true);
                }
                if ui.button("envoyer à un autre ordi…").clicked() {
                    let root = self.root();
                    self.send_files = transfer::collect(&root, &self.entries, &[e.rel.clone()], false);
                    self.dialog = Some(Dialog::Transfer { send: true });
                }
                ui.separator();
                if ui.button("afficher dans le dossier").clicked() {
                    if let Ok(path) = vault::safe(&self.root(), &e.rel) {
                        settings::reveal(&path, !e.is_dir);
                    }
                }
                if ui.button("copier le chemin").clicked() {
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
            });
        }
        if self.entries.is_empty() {
            ui.label(RichText::new("   coffre vide").color(p.muted));
        }
    }

    // ---------- éditeur ----------

    fn tabs(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        egui::Frame::new().fill(p.panel).show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::horizontal().id_salt("tabs").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (i, d) in self.docs.iter().enumerate() {
                        let active = Some(i) == self.active;
                        let name = d.rel.rsplit('/').next().unwrap_or(&d.rel);
                        let mark = if d.dirty_since.is_some() { " ●" } else { "" };
                        let fill = if active { p.bg } else { p.panel };
                        let r = egui::Frame::new().fill(fill).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin { left: 12, right: 6, top: 6, bottom: 6 }).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                ui.label(RichText::new(format!("{name}{mark}")).color(if active { p.text } else { p.muted }).size(12.0));
                                let x = ui.add(egui::Label::new(RichText::new("×").color(p.muted)).sense(egui::Sense::click()));
                                if x.on_hover_text("Fermer (Ctrl+W)").clicked() {
                                    self.actions.push(Action::Close(i));
                                }
                            });
                        });
                        if active {
                            let top = r.response.rect;
                            ui.painter().rect_filled(egui::Rect::from_min_size(top.min, egui::vec2(top.width(), 2.0)), 0.0, p.accent);
                        }
                        let resp = r.response.interact(egui::Sense::click());
                        if resp.clicked() {
                            self.active = Some(i);
                            self.selected = Some(d.rel.clone());
                            self.actions.push(Action::Open(d.rel.clone()));
                        }
                        if resp.middle_clicked() {
                            self.actions.push(Action::Close(i));
                        }
                    }
                });
            });
        });
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| {
            if self.docs.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("aucun fichier ouvert\n\n^P ouvrir · ^N nouvelle note · ^G cerveau").color(p.muted));
                });
                return;
            }
            self.tabs(ui);
            let Some(i) = self.active else { return };
            let root = self.root();
            let dark = theme::is_dark();
            let existing: HashSet<String> = self.md.keys().cloned().collect();
            let paths: HashSet<String> = self.entries.iter().map(|e| e.rel.clone()).collect();
            let rel = self.docs[i].rel.clone();
            let is_md = vault::is_md(&rel);
            let can_summarize = matches!(self.docs[i].content, Content::Text(_));

            // Barre d'outils : chemin + actions
            egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 6)).stroke(egui::Stroke::new(1.0, p.line)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let shown = vault::display_rel(&rel);
                    let parts: Vec<&str> = shown.split('/').collect();
                    for (k, part) in parts.iter().enumerate() {
                        let last = k == parts.len() - 1;
                        ui.label(RichText::new(*part).color(if last { p.text } else { p.muted }).size(12.0));
                        if !last {
                            ui.label(RichText::new("/").color(p.line).size(12.0));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("ouvrir avec…").on_hover_text("Choisir l'application").clicked() {
                            let r = vault::safe(&root, &rel).and_then(|p| settings::open_with(&p));
                            if let Err(e) = r {
                                self.error = Some(e);
                            }
                        }
                        let ext_label = if rel.to_lowercase().ends_with(".blend") { "ouvrir dans Blender" } else { "ouvrir" };
                        if ui.button(ext_label).on_hover_text("Ouvre le fichier avec l'application par défaut du système").clicked() {
                            let r = vault::safe(&root, &rel).and_then(|p| settings::open_external(&p));
                            if let Err(e) = r {
                                self.error = Some(e);
                            }
                        }
                        if live::is_previewable(&rel) && ui.button("aperçu live").on_hover_text("Ouvre la page dans ton navigateur ; elle se recharge à chaque enregistrement").clicked() {
                            self.flush_all();
                            match live::open(&root, &rel) {
                                Ok(url) => self.toast = Some((format!("aperçu : {url}"), Instant::now())),
                                Err(e) => self.error = Some(e),
                            }
                        }
                        if can_summarize && ui.button("résumer").on_hover_text("Résumé IA de ce fichier").clicked() {
                            self.actions.push(Action::Summarize);
                        }
                        if is_md {
                            let d = &mut self.docs[i];
                            let label = if d.preview { "éditer" } else { "lire" };
                            if ui.button(label).on_hover_text("Ctrl+E").clicked() {
                                d.preview = !d.preview;
                            }
                        }
                    });
                });
            });

            let doc = &mut self.docs[i];
            match &mut doc.content {
                Content::Unreadable(msg) => {
                    ui.add_space(30.0);
                    let mut open = false;
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(msg.as_str()).color(p.muted));
                        ui.add_space(10.0);
                        open = ui.button("ouvrir avec l'app par défaut").clicked();
                    });
                    if open {
                        let r = vault::safe(&root, &rel).and_then(|p| settings::open_external(&p));
                        self.report(r);
                    }
                }
                Content::Video(viewer) => {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| viewer.show(ui));
                }
                Content::NeedFfmpeg(job) => {
                    let mut reopen = false;
                    egui::Frame::new().inner_margin(30).show(ui, |ui| {
                        ui.label(RichText::new("Les vidéos ont besoin de ffmpeg.").family(theme::bold()));
                        ui.label(RichText::new(format!("Aryadne peut l'installer pour toi, sans droits admin (~100 Mo, dans {}).", ffmpeg::install_dir().display())).color(p.muted).size(12.0));
                        ui.add_space(10.0);
                        match job {
                            None => {
                                let b = egui::Button::new(RichText::new("installer ffmpeg").color(p.on_accent).family(theme::bold())).fill(p.accent).stroke(egui::Stroke::NONE);
                                if ui.add(b).clicked() {
                                    *job = Some(ffmpeg::install(ui.ctx().clone()));
                                }
                            }
                            Some(j) => {
                                let st = j.lock().map(|s| s.clone()).unwrap_or_default();
                                if let Some(e) = &st.error {
                                    ui.label(RichText::new(e).color(p.missing));
                                } else if st.done {
                                    reopen = true;
                                } else {
                                    match st.progress {
                                        Some(f) => {
                                            ui.add(egui::ProgressBar::new(f).desired_width(360.0).corner_radius(0.0).fill(p.accent).text(&st.message));
                                        }
                                        None => {
                                            ui.horizontal(|ui| {
                                                ui.spinner();
                                                ui.label(&st.message);
                                            });
                                        }
                                    }
                                    ui.ctx().request_repaint_after(Duration::from_millis(200));
                                }
                            }
                        }
                    });
                    if reopen {
                        doc.content = self_video_placeholder();
                        self.actions.push(Action::Reload(rel.clone()));
                    }
                }
                Content::Office(viewer) => {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| viewer.show(ui));
                }
                Content::Model(viewer) => {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| viewer.show(ui));
                }
                Content::Image => {
                    let slot = self.images.get(&rel, images::Size::Full);
                    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                        egui::Frame::new().inner_margin(24).show(ui, |ui| match slot {
                            images::Slot::Ready(l) => {
                                let size = l.tex.size_vec2();
                                // Les miniatures .blend sont petites : on les agrandit.
                                let max_up = if rel.to_lowercase().ends_with(".blend") { 512.0 / size.x.max(1.0) } else { 1.0 };
                                let scale = (ui.available_width() / size.x).min(max_up.max(1.0)).max(0.05);
                                ui.add(egui::Image::new(&l.tex).fit_to_exact_size(size * scale));
                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("{} × {} px", l.dims[0], l.dims[1])).color(p.muted).size(11.5));
                                    for c in &l.palette {
                                        let (r, resp) = ui.allocate_exact_size(egui::vec2(22.0, 16.0), egui::Sense::click());
                                        ui.painter().rect_filled(r, 0.0, *c);
                                        if resp.on_hover_text(format!("{} · clic pour copier", images::hex(*c))).clicked() {
                                            ui.ctx().copy_text(images::hex(*c));
                                        }
                                    }
                                });
                            }
                            images::Slot::Loading => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(RichText::new("chargement…").color(p.muted));
                                });
                            }
                            images::Slot::Failed(e) => {
                                ui.label(RichText::new(e).color(p.missing));
                            }
                        });
                    });
                }
                Content::Text(text) if doc.preview => {
                    let mut goto = None;
                    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                        egui::Frame::new().inner_margin(egui::Margin::symmetric(40, 24)).show(ui, |ui| {
                            ui.set_max_width(820.0_f32.min(ui.available_width()));
                            goto = preview::show(ui, text, &existing, &paths);
                        });
                    });
                    if let Some(n) = goto {
                        self.actions.push(Action::OpenLink(n));
                    }
                }
                Content::Text(text) => {
                    let lang = if is_md { "md".to_string() } else { ext(&rel) };
                    let fs = self.prefs.editor_size;
                    let theme = if dark { egui_extras::syntax_highlighting::CodeTheme::dark(fs) } else { egui_extras::syntax_highlighting::CodeTheme::light(fs) };
                    let wrap = is_md || lang.is_empty() || lang == "txt" || self.prefs.wrap_code;
                    let numbers = !is_md && self.prefs.line_numbers;
                    let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, width: f32| {
                        let mut job = egui_extras::syntax_highlighting::highlight(ui.ctx(), ui.style(), &theme, buf.as_str(), &lang);
                        job.wrap.max_width = if wrap { width } else { f32::INFINITY };
                        ui.painter().layout_job(job)
                    };
                    let id = egui::Id::unique(("code", &rel));
                    let notes: Vec<String> = existing.iter().cloned().collect();
                    let ed = &mut doc.code;
                    let mut changed = code::find_bar(ui, ed, text, id);
                    changed |= code::before(ui, ed, text, &lang, id, &notes);
                    let typed_now = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_))));
                    let scroll = if wrap { egui::ScrollArea::vertical() } else { egui::ScrollArea::both() };
                    scroll.id_salt(("scroll", &rel)).auto_shrink(false).show(ui, |ui| {
                        egui::Frame::new().inner_margin(egui::Margin { left: if wrap { 40 } else { 8 }, right: 24, top: 16, bottom: 120 }).show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                if numbers && !wrap {
                                    let n = text.lines().count().max(1) + usize::from(text.ends_with('\n'));
                                    let nums: String = (1..=n).map(|k| format!("{k:>4}\n")).collect();
                                    ui.add(egui::Label::new(RichText::new(nums.trim_end()).monospace().size(fs).color(p.muted.linear_multiply(0.7))).selectable(false));
                                    ui.add_space(10.0);
                                }
                                let edit = egui::TextEdit::multiline(text)
                                    .id(id)
                                    .frame(egui::Frame::NONE)
                                    .code_editor()
                                    .lock_focus(true)
                                    .desired_rows(40)
                                    .desired_width(if wrap { (ui.available_width()).min(820.0) } else { f32::INFINITY })
                                    .layouter(&mut layouter);
                                let out = edit.show(ui);
                                changed |= out.response.changed();
                                code::after(ui, ed, text, &lang, &out, &notes, typed_now && out.response.changed());
                            });
                        });
                    });
                    if changed {
                        doc.dirty_since = Some(Instant::now());
                    }
                }
            }
        });
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        egui::Panel::right("side")
            .resizable(true)
            .default_size(290.0)
            .min_size(200.0)
            .frame(egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(12))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    let is_md = self.current().is_some_and(|d| vault::is_md(&d.rel));
                    if is_md {
                        ui.label(caps(&format!("liens entrants · {}", self.backlinks.len())));
                        ui.add_space(4.0);
                        if self.backlinks.is_empty() {
                            ui.label(RichText::new("aucune note ne pointe ici").color(p.muted).size(11.5));
                        }
                        for b in self.backlinks.clone() {
                            let r = egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(8).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(RichText::new(&b.name).family(theme::bold()).size(12.5));
                                ui.label(RichText::new(&b.excerpt).color(p.muted).size(11.0));
                            });
                            let resp = r.response.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                            if resp.hovered() {
                                ui.painter().rect_stroke(resp.rect, 0.0, egui::Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
                            }
                            if resp.clicked() {
                                self.actions.push(Action::Open(b.rel.clone()));
                            }
                            ui.add_space(4.0);
                        }
                        ui.add_space(16.0);
                    }

                    ui.label(caps("résumé ia"));
                    ui.add_space(4.0);
                    let mut save = None;
                    match &self.summary {
                        None => {
                            ui.label(RichText::new("ouvre un fichier et clique sur « résumer »").color(p.muted).size(11.5));
                        }
                        Some((rel, Summary::Loading)) => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new(format!("lecture de {rel}…")).color(p.muted).size(11.5));
                            });
                        }
                        Some((rel, Summary::Failed(e))) => {
                            ui.label(RichText::new(rel).color(p.muted).size(11.0));
                            ui.label(RichText::new(e).color(p.missing).size(12.0));
                            if ui.button("réglages ia").clicked() {
                                self.open_settings(ui.ctx());
                            }
                        }
                        Some((rel, Summary::Done(text))) => {
                            ui.label(RichText::new(rel).color(p.muted).size(11.0));
                            egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(10).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                let existing: HashSet<String> = self.md.keys().cloned().collect();
                                let paths: HashSet<String> = self.entries.iter().map(|e| e.rel.clone()).collect();
                                if let Some(n) = preview::show(ui, text, &existing, &paths) {
                                    self.actions.push(Action::OpenLink(n));
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui.button("copier").clicked() {
                                    ui.ctx().copy_text(text.clone());
                                }
                                if ui.button("enregistrer en note").clicked() {
                                    save = Some((rel.clone(), text.clone()));
                                }
                            });
                        }
                    }
                    if let Some((rel, text)) = save {
                        let name = vault::stem(&rel);
                        let nd = self.notes_dir();
                            let dir = nd.as_str();
                        let note = vault::join(dir, &format!("Résumé - {name}.md"));
                        let source = if vault::is_md(&rel) { format!("[[{name}]]") } else { format!("`{rel}`") };
                        let body = format!("# Résumé - {name}\n\nSource : {source}\n\n{text}\n");
                        if self.report(vault::write(&self.root(), &note, &body)).is_some() {
                            self.refresh();
                            self.actions.push(Action::Open(note));
                        }
                    }
                });
            });
    }

    // ---------- cerveau ----------

    fn brain_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        if self.brain.dirty {
            self.flush_all();
            let g = vault::graph(&self.root(), &self.entries, self.brain.with_folders, self.brain.with_files, &self.brain.scope.clone());
            self.brain.set_graph(g);
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| {
            egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(12, 5)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(caps("cerveau"));
                    ui.add_space(8.0);
                    // Fil d'Ariane : on remonte d'un clic
                    let mut go: Option<String> = None;
                    if ui.add(egui::Button::new(RichText::new("~").color(if self.brain.scope.is_empty() { p.text } else { p.link })).frame(false)).clicked() {
                        go = Some(String::new());
                    }
                    let parts: Vec<String> = self.brain.scope.split('/').filter(|s| !s.is_empty()).map(String::from).collect();
                    for (k, part) in parts.iter().enumerate() {
                        ui.label(RichText::new("/").color(p.line));
                        let last = k == parts.len() - 1;
                        if ui.add(egui::Button::new(RichText::new(part).color(if last { p.text } else { p.link })).frame(false)).clicked() {
                            go = Some(parts[..=k].join("/"));
                        }
                    }
                    if let Some(g) = go {
                        self.brain.scope = g;
                        self.brain.dirty = true;
                        self.brain.recenter();
                    }
                    ui.add_space(12.0);
                    if ui.checkbox(&mut self.brain.with_folders, "dossiers").changed() {
                        self.brain.dirty = true;
                    }
                    if ui.checkbox(&mut self.brain.with_files, "tous les fichiers").changed() {
                        self.brain.dirty = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("recentrer").clicked() {
                            self.brain.recenter();
                        }
                        let busy = self.organize.as_ref().is_some_and(|o| o.rx.is_some());
                        if busy {
                            ui.spinner();
                        } else if ui.button("ranger avec l'IA").on_hover_text("L'IA propose un rangement de ce dossier ; tu choisis quoi appliquer").clicked() {
                            self.start_organize(ui.ctx());
                        }
                    });
                });
            });
            if let Some(rel) = self.brain.show(ui) {
                let is_dir = rel.is_empty() || self.entries.iter().any(|e| e.rel == rel && e.is_dir);
                if is_dir {
                    // Clic sur un dossier : on entre dedans ; sur le dossier courant : on remonte.
                    let target = if rel == self.brain.scope { vault::parent(&rel) } else { rel.clone() };
                    if target != self.brain.scope || !rel.is_empty() {
                        self.brain.scope = target.clone();
                        self.brain.dirty = true;
                        self.brain.recenter();
                        self.selected = Some(target.clone());
                        self.collapsed.remove(&target);
                    }
                } else {
                    self.actions.push(Action::Open(rel));
                }
            }
        });
        self.organize_dialog(ui.ctx());
    }

    // ---------- rangement par l'IA ----------

    fn start_organize(&mut self, ctx: &egui::Context) {
        let scope = self.brain.scope.clone();
        let prefix = format!("{scope}/");
        let root = self.root();
        let files: Vec<String> = self
            .entries
            .iter()
            .filter(|e| scope.is_empty() || e.rel.starts_with(&prefix))
            .filter(|e| !e.rel.starts_with('@') || !scope.is_empty())
            .take(400)
            .map(|e| {
                let mut line = if e.is_dir { format!("{}/", e.rel) } else { e.rel.clone() };
                if vault::is_md(&e.rel) {
                    let head: String = vault::read(&root, &e.rel).unwrap_or_default().chars().take(140).collect::<String>().replace('\n', " ");
                    line.push_str(&format!("  — {head}"));
                }
                line
            })
            .collect();
        let settings = self.ai.clone().unwrap_or_else(ai::Settings::load);
        let local = self.local.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        let where_ = if scope.is_empty() { "tout le coffre".to_string() } else { format!("le dossier {scope}") };
        std::thread::spawn(move || {
            let sys = "Tu ranges les fichiers d'un artiste 3D (notes, références, modèles, vidéos, code). Tu proposes des déplacements de fichiers vers des dossiers clairs et peu nombreux, en français, en minuscules. Ne renomme pas les fichiers, ne touche pas aux dossiers .app ni aux fichiers déjà bien rangés. Réponds UNIQUEMENT avec du JSON : {\"moves\":[{\"from\":\"chemin/actuel.ext\",\"to_dir\":\"dossier/cible\",\"why\":\"raison courte\"}]}. Maximum 40 déplacements.";
            let user = format!("Range {where_}. Fichiers (chemin, et début du contenu pour les notes) :\n{}", files.join("\n"));
            let ready = if ollama::is_local(&settings.url) { local.ensure_running(&settings.url) } else { Ok(()) };
            let r = ready.and_then(|_| ai::complete(&settings, sys, &user)).and_then(|t| parse_moves(&t));
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.organize = Some(Organize { rx: Some(rx), moves: vec![], error: None });
    }

    fn organize_dialog(&mut self, ctx: &egui::Context) {
        let Some(o) = &mut self.organize else { return };
        if let Some(rx) = &o.rx {
            match rx.try_recv() {
                Ok(Ok(m)) => {
                    let known: HashSet<String> = self.entries.iter().map(|e| e.rel.clone()).collect();
                    o.moves = m.into_iter().filter(|m| known.contains(&m.from) && vault::parent(&m.from) != m.to_dir).map(|m| (true, m)).collect();
                    o.rx = None;
                }
                Ok(Err(e)) => {
                    o.error = Some(e);
                    o.rx = None;
                }
                Err(_) => return,
            }
        }
        let p = theme::pal();
        let mut close = false;
        let mut apply = false;
        egui::Modal::new(egui::Id::unique("organize")).frame(egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(16)).show(ctx, |ui| {
            ui.set_width(620.0);
            ui.label(caps("rangement proposé par l'ia"));
            ui.add_space(6.0);
            if let Some(e) = &o.error {
                ui.label(RichText::new(e).color(p.missing));
            } else if o.moves.is_empty() {
                ui.label(RichText::new("Rien à ranger : tout semble déjà à sa place.").color(p.muted));
            } else {
                ui.label(RichText::new("Décoche ce que tu ne veux pas. Rien n'est déplacé avant « appliquer ».").color(p.muted).size(11.5));
                ui.add_space(6.0);
                egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                    for (on, m) in &mut o.moves {
                        ui.horizontal(|ui| {
                            ui.checkbox(on, "");
                            ui.vertical(|ui| {
                                ui.label(RichText::new(format!("{}  →  {}/", m.from, m.to_dir)).size(12.0));
                                if !m.why.is_empty() {
                                    ui.label(RichText::new(&m.why).color(p.muted).size(11.0));
                                }
                            });
                        });
                    }
                });
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let n = o.moves.iter().filter(|m| m.0).count();
                if n > 0 {
                    let b = egui::Button::new(RichText::new(format!("appliquer ({n})")).color(p.on_accent).family(theme::bold())).fill(p.accent).stroke(egui::Stroke::NONE);
                    apply = ui.add(b).clicked();
                }
                close = ui.button("fermer").clicked();
            });
        });
        let moves: Vec<OrganizeMove> = o.moves.iter().filter(|m| m.0).map(|m| m.1.clone()).collect();
        if apply {
            let root = self.root();
            for m in &moves {
                if !m.to_dir.is_empty() {
                    let r = vault::create_dir(&root, &m.to_dir);
                    self.report(r);
                }
                self.actions.push(Action::Move(m.from.clone(), m.to_dir.clone()));
            }
            self.toast = Some((format!("{} fichier(s) rangé(s)", moves.len()), Instant::now()));
            self.brain.dirty = true;
            close = true;
        }
        if close {
            self.organize = None;
        }
    }

    // ---------- boards ----------

    fn boards_list(&self) -> Vec<String> {
        self.entries.iter().filter(|e| !e.is_dir && board::is_board(&e.rel)).map(|e| e.rel.clone()).collect()
    }

    fn save_board(&mut self) {
        let root = self.root();
        let r = self.board.save(&root);
        self.report(r);
    }

    fn open_board(&mut self, rel: String) {
        if self.board.rel.as_deref() != Some(rel.as_str()) {
            self.save_board();
            let root = self.root();
            if let Some(b) = self.report(board::Board::load(&root, &rel)) {
                self.board = b;
                save_config("last_board", &rel);
            }
        }
        self.selected = Some(rel);
        self.view = Some(View::Boards);
    }

    fn new_board(&mut self, name: &str) -> Option<String> {
        let dir = if self.entries.iter().any(|e| e.rel == "boards") { "boards" } else { "" };
        let mut rel = vault::join(dir, &format!("{name}.board.json"));
        let mut i = 2;
        while self.entries.iter().any(|e| e.rel == rel) {
            rel = vault::join(dir, &format!("{name} {i}.board.json"));
            i += 1;
        }
        let r = vault::write(&self.root(), &rel, "{\n  \"items\": []\n}\n");
        self.report(r)?;
        self.refresh();
        self.open_board(rel.clone());
        Some(rel)
    }

    /// Board courant, ou le dernier ouvert, ou un nouveau.
    fn ensure_board(&mut self) {
        if self.board.rel.is_some() {
            return;
        }
        let last = load_config("last_board").filter(|r| self.entries.iter().any(|e| &e.rel == r));
        match last.or_else(|| self.boards_list().first().cloned()) {
            Some(rel) => {
                let root = self.root();
                if let Ok(b) = board::Board::load(&root, &rel) {
                    self.board = b;
                }
            }
            None => {
                self.new_board("Moodboard");
            }
        }
    }

    /// Colle l'image du presse-papier sur le board (enregistrée dans refs/),
    /// ou un chemin de fichier copié (lié, pas copié).
    fn paste_image(&mut self, at: egui::Vec2) {
        let mut cb = match arboard::Clipboard::new() {
            Ok(c) => c,
            Err(e) => return self.toast(format!("presse-papier indisponible : {e}")),
        };
        if let Ok(img) = cb.get_image() {
            let Some(buf) = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned()) else {
                return self.toast("image du presse-papier illisible");
            };
            let dir = if self.entries.iter().any(|e| e.rel == "refs") { "refs" } else { "" };
            let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let rel = vault::join(dir, &format!("collé-{ts}.png"));
            let saved = vault::safe(&self.root(), &rel).and_then(|path| buf.save(&path).map_err(|e| e.to_string()));
            if self.report(saved).is_some() {
                self.refresh();
                self.board.drop_file(&rel, at, &mut self.images);
                self.toast(format!("image collée → {rel}"));
            }
            return;
        }
        if let Ok(text) = cb.get_text() {
            let path = std::path::PathBuf::from(text.trim().trim_matches('"'));
            if path.exists() {
                let root = self.root();
                let rel = vault::inside(&root, &path).map(Ok).unwrap_or_else(|| vault::link_external(&root, &path));
                if let Some(rel) = self.report(rel) {
                    self.refresh();
                    self.board.drop_file(&rel, at, &mut self.images);
                }
                return;
            }
        }
        self.toast("rien à coller : copie une image d'abord");
    }

    fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn all_images(&self) -> Vec<Entry> {
        self.entries.iter().filter(|e| !e.is_dir && images::has_preview(&e.rel)).cloned().collect()
    }

    fn aspect(&self, rel: &str) -> f32 {
        self.images.peek(rel, images::Size::Thumb).map(|l| l.dims[0] as f32 / l.dims[1].max(1) as f32).unwrap_or(1.5)
    }

    /// Moodboard automatique par mots-clés (nom, dossier, notes qui parlent de l'image).
    fn auto_board_keywords(&mut self, query: &str) -> usize {
        let words: Vec<String> = chat::fold(query).split_whitespace().filter(|w| w.len() > 1).map(str::to_string).collect();
        if words.is_empty() {
            return 0;
        }
        let root = self.root();
        // Texte associé à chaque image : son chemin + les lignes de notes qui la mentionnent.
        let mut ctx_text: HashMap<String, String> = self.all_images().into_iter().map(|e| (e.rel.clone(), chat::fold(&e.rel))).collect();
        for e in self.entries.iter().filter(|e| vault::is_md(&e.rel)) {
            let Ok(t) = vault::read(&root, &e.rel) else { continue };
            for line in t.lines() {
                for (rel, acc) in ctx_text.iter_mut() {
                    let name = rel.rsplit('/').next().unwrap_or(rel);
                    if line.contains(name) {
                        acc.push(' ');
                        acc.push_str(&chat::fold(line));
                        acc.push(' ');
                        acc.push_str(&chat::fold(&e.rel));
                    }
                }
            }
        }
        let on_board: HashSet<String> = self.board.data.items.iter().filter_map(|i| if let board::Kind::Image { rel, .. } = &i.kind { Some(rel.clone()) } else { None }).collect();
        let mut found: Vec<(String, usize)> = ctx_text
            .into_iter()
            .filter(|(rel, _)| !on_board.contains(rel))
            .map(|(rel, t)| {
                let score = words.iter().filter(|w| t.contains(w.as_str())).count();
                (rel, score)
            })
            .filter(|(_, s)| *s > 0)
            .collect();
        found.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let picks: Vec<(String, f32)> = found.into_iter().take(40).map(|(r, _)| (r.clone(), self.aspect(&r))).collect();
        let n = picks.len();
        self.board.add_grid(picks);
        n
    }

    /// Moodboard automatique par couleurs proches de l'image `seed`.
    fn auto_board_colors(&mut self, seed: &str) -> Result<usize, String> {
        let Some(seed_pal) = self.images.peek(seed, images::Size::Thumb).map(|l| l.palette.clone()) else {
            return Err("Palette pas encore prête, réessaie dans une seconde.".into());
        };
        let all = self.all_images();
        let mut pending = 0;
        let mut scored = vec![];
        for e in &all {
            if e.rel == seed {
                continue;
            }
            match self.images.get(&e.rel, images::Size::Thumb) {
                images::Slot::Ready(l) => scored.push((e.rel.clone(), images::palette_distance(&seed_pal, &l.palette))),
                images::Slot::Loading => pending += 1,
                images::Slot::Failed(_) => {}
            }
        }
        if pending > 0 && scored.len() < 3 {
            return Err(format!("Analyse des couleurs en cours ({pending} images restantes), réessaie dans un instant."));
        }
        scored.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut picks = vec![(seed.to_string(), self.aspect(seed))];
        picks.extend(scored.into_iter().take(15).filter(|(_, d)| *d < 90.0).map(|(r, _)| (r.clone(), self.aspect(&r))));
        let n = picks.len();
        self.board.add_grid(picks);
        Ok(n)
    }

    fn boards_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        self.ensure_board();
        let boards = self.boards_list();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| {
            egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(12, 5)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(caps("board"));
                    let current = self.board.rel.clone().unwrap_or_default();
                    let mut pick = None;
                    egui::ComboBox::from_id_salt("board-pick").selected_text(board::board_name(&current)).width(180.0).show_ui(ui, |ui| {
                        for b in &boards {
                            if ui.selectable_label(*b == current, board::board_name(b)).clicked() {
                                pick = Some(b.clone());
                            }
                        }
                    });
                    if let Some(b) = pick {
                        self.open_board(b);
                    }
                    if ui.button("+ board").clicked() {
                        self.new_board("Moodboard");
                    }
                    ui.separator();
                    // Outils
                    for (tool, label, key) in [(board::Tool::Select, "sélection", "V"), (board::Tool::Pen, "dessin", "B"), (board::Tool::Eraser, "gomme", "E"), (board::Tool::Text, "texte", "T")] {
                        if ui.selectable_label(self.board.tool == tool, label).on_hover_text(key).clicked() {
                            self.board.tool = tool;
                        }
                    }
                    if matches!(self.board.tool, board::Tool::Pen) {
                        ui.separator();
                        for c in [p.accent, egui::Color32::from_rgb(0x18, 0x18, 0x1b), egui::Color32::WHITE, egui::Color32::from_rgb(0x25, 0x63, 0xeb), egui::Color32::from_rgb(0x16, 0xa3, 0x4a), egui::Color32::from_rgb(0xea, 0xb3, 0x08)] {
                            let (r, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
                            ui.painter().rect_filled(r, 0.0, c);
                            ui.painter().rect_stroke(r, 0.0, egui::Stroke::new(if self.board.pen_color == c { 2.0 } else { 1.0 }, if self.board.pen_color == c { p.text } else { p.line }), egui::StrokeKind::Inside);
                            if resp.clicked() {
                                self.board.pen_color = c;
                            }
                        }
                        ui.color_edit_button_srgba(&mut self.board.pen_color);
                        ui.add(egui::Slider::new(&mut self.board.pen_width, 1.0..=40.0).show_value(false)).on_hover_text("épaisseur");
                    }
                    ui.separator();
                    if ui.add_enabled(self.board.can_undo(), egui::Button::new("↶")).on_hover_text("annuler (Ctrl+Z)").clicked() {
                        self.board.undo();
                    }
                    if ui.add_enabled(self.board.can_redo(), egui::Button::new("↷")).on_hover_text("rétablir (Ctrl+Y)").clicked() {
                        self.board.redo();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("cadrer").clicked() {
                            self.board.fit();
                        }
                    });
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("auto").color(p.accent).family(theme::bold()));
                    ui.add(egui::TextEdit::singleline(&mut self.auto_query).hint_text("mots-clés : forêt brume…").desired_width(200.0));
                    if ui.button("par mots-clés").on_hover_text("Ajoute les images dont le nom, le dossier ou les notes qui les citent correspondent").clicked() {
                        let q = self.auto_query.clone();
                        let n = self.auto_board_keywords(&q);
                        self.toast(if n == 0 { "aucune image ne correspond".to_string() } else { format!("{n} images ajoutées") });
                    }
                    let seed = (self.board.selection.len() == 1)
                        .then(|| self.board.selection[0])
                        .and_then(|i| self.board.data.items.get(i))
                        .and_then(|it| if let board::Kind::Image { rel, .. } = &it.kind { Some(rel.clone()) } else { None });
                    let btn = ui.add_enabled(seed.is_some(), egui::Button::new("par couleurs")).on_hover_text("Ajoute les refs aux couleurs proches de l'image sélectionnée").on_disabled_hover_text("sélectionne d'abord une seule image du board");
                    if btn.clicked() {
                        if let Some(seed) = seed {
                            match self.auto_board_colors(&seed) {
                                Ok(n) => self.toast(format!("{} images aux couleurs proches", n.saturating_sub(1))),
                                Err(e) => self.toast(e),
                            }
                        }
                    }
                });
            });
            let entries = self.entries.clone();
            let root = self.root();
            let summary = move |rel: &str| -> String {
                let _ = &entries;
                vault::read(&root, rel).map(|t| t.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()).take(6).collect::<Vec<_>>().join("\n")).unwrap_or_default()
            };
            match self.board.show(ui, &mut self.images, &summary) {
                Some(board::Event::Open(rel)) => self.actions.push(Action::Open(rel)),
                Some(board::Event::PasteImage(at)) => self.paste_image(at.to_vec2()),
                None => {}
            }
        });
    }

    // ---------- refs ----------

    fn refs_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        if self.refs.size == 0.0 {
            self.refs.size = 170.0;
        }
        let mut imgs = self.all_images();
        let f = chat::fold(&self.refs.filter);
        imgs.retain(|e| f.is_empty() || chat::fold(&e.rel).contains(&f));
        match self.refs.sort {
            RefSort::Recent => imgs.sort_by(|a, b| b.modified.cmp(&a.modified)),
            RefSort::Name => imgs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            RefSort::Color => {
                let hue = |rel: &str, me: &Self| {
                    me.images.peek(rel, images::Size::Thumb).and_then(|l| l.palette.first().copied()).map(|c| {
                        let hsva = egui::ecolor::Hsva::from(c);
                        if hsva.s < 0.15 { 2.0 + hsva.v } else { hsva.h }
                    }).unwrap_or(9.0)
                };
                let mut keyed: Vec<(f32, Entry)> = imgs.into_iter().map(|e| (hue(&e.rel, self), e)).collect();
                keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
                imgs = keyed.into_iter().map(|(_, e)| e).collect();
            }
        }

        // Panneau de détails
        if let Some(sel) = self.refs.selected.clone() {
            egui::Panel::right("ref-detail").resizable(true).default_size(320.0).min_size(240.0)
                .frame(egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(12))
                .show(ui, |ui| {
                    ui.label(caps("référence"));
                    ui.label(RichText::new(&sel).size(11.5));
                    ui.add_space(6.0);
                    match self.images.get(&sel, images::Size::Full) {
                        images::Slot::Ready(l) => {
                            let s = l.tex.size_vec2();
                            let scale = ui.available_width() / s.x;
                            ui.add(egui::Image::new(&l.tex).fit_to_exact_size(s * scale));
                            ui.label(RichText::new(format!("{} × {} px", l.dims[0], l.dims[1])).color(p.muted).size(11.0));
                            ui.add_space(6.0);
                            ui.label(caps("palette · clic pour copier"));
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                let w = ((ui.available_width() - 4.0 * (l.palette.len() as f32 - 1.0)) / l.palette.len().max(1) as f32).max(12.0);
                                for c in &l.palette {
                                    let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 32.0), egui::Sense::click());
                                    ui.painter().rect_filled(r, 0.0, *c);
                                    if resp.on_hover_text(images::hex(*c)).clicked() {
                                        ui.ctx().copy_text(images::hex(*c));
                                        self.toast = Some((format!("{} copié", images::hex(*c)), Instant::now()));
                                    }
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                for c in &l.palette {
                                    ui.label(RichText::new(images::hex(*c)).size(10.5).color(p.muted));
                                }
                            });
                        }
                        images::Slot::Loading => {
                            ui.spinner();
                        }
                        images::Slot::Failed(e) => {
                            ui.label(RichText::new(e).color(p.missing));
                        }
                    }
                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("ouvrir").clicked() {
                            self.actions.push(Action::Open(sel.clone()));
                        }
                        if ui.button("ajouter au board").clicked() {
                            self.ensure_board();
                            let c = self.board.view_center;
                            self.board.drop_file(&sel, c, &mut self.images);
                            self.toast(format!("ajouté à « {} »", board::board_name(self.board.rel.as_deref().unwrap_or(""))));
                        }
                        if ui.button("board auto (couleurs)").on_hover_text("Ajoute cette image et les refs aux couleurs proches au board").clicked() {
                            self.ensure_board();
                            match self.auto_board_colors(&sel) {
                                Ok(n) => {
                                    self.toast(format!("{n} images ajoutées au board"));
                                    self.view = Some(View::Boards);
                                }
                                Err(e) => self.toast(e),
                            }
                        }
                        if ui.button("explorateur").clicked() {
                            self.selected = Some(sel.clone());
                            self.collapsed.remove(&vault::parent(&sel));
                        }
                    });
                });
        }

        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| {
            egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(12, 5)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(caps(&format!("refs · {}", imgs.len())));
                    ui.add(egui::TextEdit::singleline(&mut self.refs.filter).hint_text("filtrer par nom ou dossier").desired_width(200.0));
                    ui.separator();
                    for (s, label) in [(RefSort::Recent, "récentes"), (RefSort::Name, "nom"), (RefSort::Color, "couleur")] {
                        if ui.selectable_label(self.refs.sort == s, label).clicked() {
                            self.refs.sort = s;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(egui::Slider::new(&mut self.refs.size, 90.0..=340.0).show_value(false));
                        ui.label(RichText::new("taille").color(p.muted).size(11.0));
                    });
                });
            });
            if imgs.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("aucune image\n\nglisse des images dans la fenêtre pour les importer dans refs/").color(p.muted));
                });
                return;
            }
            let cell = self.refs.size;
            let gap = 8.0;
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                egui::Frame::new().inner_margin(12).show(ui, |ui| {
                    let cols = ((ui.available_width() + gap) / (cell + gap)).floor().max(1.0) as usize;
                    let rows = imgs.len().div_ceil(cols);
                    for row in 0..rows {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            for e in imgs.iter().skip(row * cols).take(cols) {
                                let (rect, resp) = ui.allocate_exact_size(egui::vec2(cell, cell), egui::Sense::click_and_drag());
                                let selected = self.refs.selected.as_deref() == Some(&e.rel);
                                // Ne charge que ce qui est visible
                                if ui.is_rect_visible(rect) {
                                    let painter = ui.painter();
                                    painter.rect_filled(rect, 0.0, p.panel);
                                    match self.images.get(&e.rel, images::Size::Thumb) {
                                        images::Slot::Ready(l) => {
                                            let s = l.tex.size_vec2();
                                            let k = (cell / s.x).min(cell / s.y);
                                            let r = egui::Rect::from_center_size(rect.center(), s * k);
                                            painter.image(l.tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                                        }
                                        images::Slot::Loading => {
                                            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::monospace(14.0), p.muted);
                                        }
                                        images::Slot::Failed(_) => {
                                            let ext = e.rel.rsplit_once('.').map(|(_, x)| x.to_uppercase()).unwrap_or_default();
                                            painter.text(rect.center() - egui::vec2(0.0, 10.0), egui::Align2::CENTER_CENTER, ext, egui::FontId::new(22.0, theme::bold()), p.accent);
                                            painter.with_clip_rect(rect.shrink(6.0)).text(rect.center() + egui::vec2(0.0, 16.0), egui::Align2::CENTER_CENTER, &e.name, egui::FontId::monospace(10.5), p.muted);
                                        }
                                    }
                                    if resp.hovered() || selected {
                                        let band = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 20.0), rect.right_bottom());
                                        painter.rect_filled(band, 0.0, p.bg.linear_multiply(0.85));
                                        painter.with_clip_rect(band).text(band.left_center() + egui::vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, &e.name, egui::FontId::monospace(10.5), p.text);
                                    }
                                    if selected {
                                        painter.rect_stroke(rect, 0.0, egui::Stroke::new(2.0, p.accent), egui::StrokeKind::Inside);
                                    }
                                }
                                if resp.drag_started() {
                                    egui::DragAndDrop::set_payload(ui.ctx(), e.rel.clone());
                                }
                                if resp.clicked() {
                                    self.refs.selected = Some(e.rel.clone());
                                }
                                if resp.double_clicked() {
                                    self.actions.push(Action::Open(e.rel.clone()));
                                }
                            }
                        });
                        ui.add_space(gap - ui.spacing().item_spacing.y);
                    }
                });
            });
        });
    }

    // ---------- chat ----------

    /// Ce que l'assistant voit : le fichier ouvert (et la sélection), ou le board courant.
    fn assistant_context(&self, ctx: &egui::Context) -> Option<(String, String)> {
        match self.view() {
            View::Files => {
                let d = self.current()?;
                let Content::Text(t) = &d.content else {
                    return Some((d.rel.clone(), format!("(fichier non textuel : {})", d.rel)));
                };
                let id = egui::Id::unique(("code", &d.rel));
                let sel = egui::TextEdit::load_state(ctx, id).and_then(|s| s.cursor.char_range()).and_then(|r| {
                    let (a, b) = (r.primary.index.0.min(r.secondary.index.0), r.primary.index.0.max(r.secondary.index.0));
                    (b > a).then(|| t.chars().skip(a).take(b - a).collect::<String>())
                });
                let mut body = t.clone();
                if let Some(s) = sel {
                    body = format!("SÉLECTION ACTUELLE :\n{s}\n\nFICHIER COMPLET :\n{t}");
                }
                Some((d.rel.clone(), body))
            }
            View::Boards => {
                let rel = self.board.rel.clone()?;
                let root = self.root();
                let raw = vault::safe(&root, &rel).ok().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
                Some((rel, raw.chars().take(6000).collect()))
            }
            _ => None,
        }
    }

    fn assistant_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        self.assistant.poll();
        let existing: HashSet<String> = self.md.keys().cloned().collect();
        let paths: HashSet<String> = self.entries.iter().map(|e| e.rel.clone()).collect();
        let ctxt = ui.ctx().clone();
        let context = self.assistant_context(&ctxt);
        let mut send = false;
        let mut goto = None;
        let mut insert: Option<String> = None;
        egui::Panel::right("assistant")
            .resizable(true)
            .default_size(340.0)
            .min_size(240.0)
            .frame(egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(caps("assistant"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").on_hover_text("Ctrl+L").clicked() {
                            self.assistant_open = false;
                        }
                        if ui.small_button("vider").clicked() {
                            self.assistant.msgs.clear();
                        }
                    });
                });
                let ctx_label = match &context {
                    Some((rel, body)) if body.starts_with("SÉLECTION") => format!("voit : la sélection dans {rel}"),
                    Some((rel, _)) => format!("voit : {rel}"),
                    None => "voit : tout le coffre".into(),
                };
                ui.label(RichText::new(ctx_label).color(p.muted).size(11.0));
                ui.add_space(6.0);
                // Saisie en bas
                egui::Panel::bottom("assistant-input").frame(egui::Frame::NONE).show(ui, |ui| {
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        let quick: &[(&str, &str)] = match self.view() {
                            View::Files => &[("expliquer", "Explique ce code ou ce texte simplement."), ("corriger", "Trouve les bugs ou les erreurs et donne la version corrigée dans un bloc de code."), ("améliorer", "Propose une version améliorée (plus claire, plus propre) dans un bloc de code."), ("commenter", "Ajoute des commentaires utiles et renvoie le code complet dans un bloc."), ("résumer", "Résume ce fichier en quelques points.")],
                            View::Boards => &[("idées", "Propose des idées et des références à ajouter à ce moodboard."), ("palette", "Décris l'ambiance et propose une palette de couleurs (codes hex).")],
                            _ => &[("organiser", "Comment mieux organiser mes fichiers et mes dossiers ?"), ("à faire", "Liste les tâches non terminées (cases - [ ]) trouvées dans mes notes.")],
                        };
                        for (label, prompt) in quick {
                            if ui.small_button(*label).clicked() && !self.assistant.busy() {
                                self.assistant.input = prompt.to_string();
                                send = true;
                            }
                        }
                    });
                    let edit = egui::TextEdit::multiline(&mut self.assistant.input).hint_text("demande quelque chose… (Entrée pour envoyer)").desired_rows(3).desired_width(f32::INFINITY);
                    let r = ui.add(edit);
                    if r.has_focus() && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift) {
                        self.assistant.input = self.assistant.input.trim_end_matches('\n').to_string();
                        send = true;
                    }
                });
                egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink(false).show(ui, |ui| {
                    if self.assistant.msgs.is_empty() && !self.assistant.busy() {
                        ui.label(RichText::new("Pose une question sur ce que tu as à l'écran. Les blocs de code proposés peuvent être insérés directement dans le fichier ouvert.").color(p.muted).size(11.5));
                    }
                    for m in &self.assistant.msgs {
                        let user = m.role == chat::Role::User;
                        msg_block(ui, user, &m.text, &m.sources, m.error.as_deref(), None, &existing, &paths, &mut goto);
                        if !user {
                            for (k, block) in code_blocks(&m.text).into_iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("bloc {}", k + 1)).color(p.muted).size(11.0));
                                    if ui.small_button("insérer au curseur").clicked() {
                                        insert = Some(block.clone());
                                    }
                                    if ui.small_button("copier").clicked() {
                                        ui.ctx().copy_text(block.clone());
                                    }
                                });
                            }
                        }
                    }
                    if let Some(live) = &self.assistant.live {
                        if let Ok(l) = live.lock() {
                            let status = (!l.status.is_empty()).then_some(l.status.as_str());
                            msg_block(ui, false, &l.text, &[], l.error.as_deref(), status, &existing, &paths, &mut goto);
                        }
                    }
                });
            });
        if send {
            let root = self.root();
            let entries = self.entries.clone();
            let settings = self.ai.clone().unwrap_or_else(ai::Settings::load);
            self.assistant.send(root, entries, context, settings, self.local.clone(), ctxt.clone());
        }
        if let Some(code) = insert {
            match self.active.filter(|_| self.view() == View::Files).and_then(|i| self.docs.get_mut(i)) {
                Some(d) if matches!(d.content, Content::Text(_)) => {
                    d.code.insert = Some(code);
                    d.dirty_since = Some(Instant::now());
                    self.toast = Some(("inséré dans le fichier".into(), Instant::now()));
                }
                _ => {
                    ctxt.copy_text(code);
                    self.toast = Some(("aucun fichier texte ouvert : copié dans le presse-papiers".into(), Instant::now()));
                }
            }
        }
        if let Some(n) = goto {
            self.actions.push(Action::OpenLink(n));
        }
    }

    fn chat_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        self.chat.poll();
        let existing: HashSet<String> = self.md.keys().cloned().collect();
        let paths: HashSet<String> = self.entries.iter().map(|e| e.rel.clone()).collect();
        let mut send = false;
        let mut save = false;

        egui::Panel::bottom("chat-input")
            .frame(egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(12))
            .show(ui, |ui| {
                let open_name = self.current().map(|d| d.rel.clone());
                ui.horizontal(|ui| {
                    let label = match &open_name {
                        Some(n) => format!("inclure le fichier ouvert ({n})"),
                        None => "inclure le fichier ouvert".into(),
                    };
                    ui.add_enabled(open_name.is_some(), egui::Checkbox::new(&mut self.chat.include_open, RichText::new(label).size(11.5)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("nouvelle conversation").clicked() && !self.chat.busy() {
                            self.chat.msgs.clear();
                        }
                        if ui.add_enabled(!self.chat.msgs.is_empty(), egui::Button::new("enregistrer en note")).clicked() {
                            save = true;
                        }
                        let model = self.ai.clone().unwrap_or_default().model;
                        ui.label(RichText::new(format!("modèle : {model}")).color(p.muted).size(11.0));
                    });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let enter = ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
                    let edit = egui::TextEdit::multiline(&mut self.chat.input)
                        .hint_text("demande n'importe quoi sur ton coffre…  (entrée : envoyer · maj+entrée : nouvelle ligne)")
                        .desired_rows(2)
                        .desired_width(ui.available_width() - 90.0);
                    let r = ui.add(edit);
                    if r.has_focus() && enter {
                        // Retire le saut de ligne que l'Entrée vient d'ajouter
                        while self.chat.input.ends_with('\n') {
                            self.chat.input.pop();
                        }
                        send = true;
                    }
                    let btn = egui::Button::new(RichText::new("envoyer").color(p.on_accent).family(theme::bold())).fill(p.accent).stroke(egui::Stroke::NONE).min_size(egui::vec2(80.0, 40.0));
                    if ui.add_enabled(!self.chat.busy(), btn).clicked() {
                        send = true;
                    }
                    if self.chat.msgs.is_empty() && !self.chat.busy() {
                        r.request_focus();
                    }
                });
            });

        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| {
            let mut goto = None;
            egui::ScrollArea::vertical().auto_shrink(false).stick_to_bottom(true).show(ui, |ui| {
                egui::Frame::new().inner_margin(egui::Margin::symmetric(32, 20)).show(ui, |ui| {
                    ui.set_max_width(860.0_f32.min(ui.available_width()));
                    if self.chat.msgs.is_empty() && !self.chat.busy() {
                        ui.add_space(30.0);
                        ui.label(RichText::new("chat").size(22.0).family(theme::bold()));
                        ui.label(RichText::new("// pose une question : je cherche dans tes notes, ton code et tes fichiers, et je te renvoie vers les bons.").color(p.muted));
                        ui.add_space(14.0);
                        for s in [
                            "Résume mes projets en cours",
                            "Quelles notes parlent du court métrage ?",
                            "Qu'est-ce que fait le code dans le dossier code ?",
                            "Quelles idées je n'ai pas encore développées ?",
                        ] {
                            if ui.button(format!("› {s}")).clicked() {
                                self.chat.input = s.to_string();
                                send = true;
                            }
                        }
                    }
                    let msgs = self.chat.msgs.clone();
                    for m in &msgs {
                        msg_block(ui, m.role == chat::Role::User, &m.text, &m.sources, m.error.as_deref(), None, &existing, &paths, &mut goto);
                    }
                    if let Some(live) = &self.chat.live {
                        if let Ok(l) = live.lock() {
                            let status = (!l.status.is_empty()).then_some(l.status.as_str());
                            msg_block(ui, false, &l.text, &l.sources, l.error.as_deref(), status, &existing, &paths, &mut goto);
                        }
                    }
                });
            });
            if let Some(g) = goto {
                self.actions.push(Action::OpenLink(g));
            }
        });

        if send && !self.chat.busy() && !self.chat.input.trim().is_empty() {
            self.flush_all();
            let open = if self.chat.include_open {
                self.current().and_then(|d| if let Content::Text(t) = &d.content { Some((d.rel.clone(), t.clone())) } else { None })
            } else {
                None
            };
            let settings = self.ai.clone().unwrap_or_default();
            self.chat.send(self.root(), self.entries.clone(), open, settings, self.local.clone(), ui.ctx().clone());
        }
        if save {
            let nd = self.notes_dir();
                            let dir = nd.as_str();
            let mut rel = vault::join(dir, "Conversation.md");
            let mut i = 2;
            while self.entries.iter().any(|e| e.rel == rel) {
                rel = vault::join(dir, &format!("Conversation {i}.md"));
                i += 1;
            }
            let md = self.chat.to_markdown();
            if self.report(vault::write(&self.root(), &rel, &md)).is_some() {
                self.refresh();
                self.actions.push(Action::Open(rel));
            }
        }
    }


    // ---------- export et transfert ----------

    fn export_zip(&mut self, rels: Vec<String>, with_links: bool) {
        let root = self.root();
        let files = transfer::collect(&root, &self.entries, &rels, with_links);
        if files.is_empty() {
            return self.toast("rien à exporter");
        }
        let base = rels.first().map(|r| vault::stem(r)).unwrap_or_else(|| "export".into()).trim_start_matches('@').to_string();
        let Some(dest) = rfd::FileDialog::new().set_title("Exporter en zip").set_file_name(format!("{base}.zip")).add_filter("zip", &["zip"]).save_file() else { return };
        let job = transfer::export_zip(root, files, dest, self.ctx.clone().unwrap());
        self.jobs.push(job);
    }

    fn poll_jobs(&mut self) {
        let mut msgs = vec![];
        self.jobs.retain(|j| {
            let p = j.lock().map(|p| p.clone()).unwrap_or_default();
            if p.finished {
                msgs.push(p.error.map(|e| format!("erreur : {e}")).or(p.result).unwrap_or_default());
                false
            } else {
                true
            }
        });
        let mut got_files = false;
        if let Some(rx) = &self.receiver {
            if let Ok(mut p) = rx.progress.lock() {
                if p.finished {
                    p.finished = false;
                    if let Some(r) = p.result.take() {
                        msgs.push(r);
                    }
                    got_files = true;
                }
                if let Some(e) = p.error.take() {
                    msgs.push(format!("erreur : {e}"));
                }
            }
        }
        if got_files {
            self.refresh();
        }
        if let Some(m) = msgs.into_iter().filter(|m| !m.is_empty()).last() {
            self.toast(m);
        }
    }

    fn transfer_dialog(&mut self, ctx: &egui::Context, mut send: bool) -> bool {
        let p = theme::pal();
        let mut keep = true;
        let frame = egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.text)).inner_margin(16);
        let r = egui::Modal::new(egui::Id::unique("transfer")).frame(frame).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.horizontal(|ui| {
                ui.label(caps("transfert entre ordinateurs"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("fermer").clicked() {
                        keep = false;
                    }
                });
            });
            ui.label(RichText::new("Sur le même réseau (wifi ou câble), sans cloud et sans limite de taille. Idéal pour les fichiers 3D.").color(p.muted).size(11.5));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.selectable_label(!send, "recevoir").clicked() {
                    send = false;
                }
                if ui.selectable_label(send, "envoyer").clicked() {
                    send = true;
                    let (found, c) = (self.peers.clone(), ctx.clone());
                    if let Ok(mut f) = found.lock() {
                        f.clear();
                    }
                    transfer::discover(found, c);
                }
            });
            ui.separator();
            if !send {
                match &self.receiver {
                    None => {
                        ui.label("Mets cet ordinateur en réception, puis envoie depuis l'autre.");
                        ui.add_space(6.0);
                        let b = egui::Button::new(RichText::new("commencer à recevoir").color(p.on_accent).family(theme::bold())).fill(p.accent).stroke(egui::Stroke::NONE);
                        if ui.add(b).clicked() {
                            match transfer::Receiver::start(self.root(), ctx.clone()) {
                                Ok(r) => self.receiver = Some(r),
                                Err(e) => self.error = Some(e),
                            }
                        }
                        ui.label(RichText::new("Windows peut demander d'autoriser Aryadne sur le réseau : accepte pour les réseaux privés.").color(p.muted).size(11.0));
                    }
                    Some(rx) => {
                        let ip = transfer::local_ip().unwrap_or_else(|| "?".into());
                        egui::Grid::new("rx").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                            ui.label("ordinateur");
                            ui.label(RichText::new(transfer::hostname()).family(theme::bold()));
                            ui.end_row();
                            ui.label("adresse");
                            ui.label(RichText::new(format!("{ip}:{}", rx.port)).family(theme::bold()));
                            ui.end_row();
                            ui.label("code");
                            ui.label(RichText::new(&rx.code).size(28.0).color(p.accent).family(theme::bold()));
                            ui.end_row();
                        });
                        let pr = rx.progress.lock().map(|p| p.clone()).unwrap_or_default();
                        ui.add_space(6.0);
                        if pr.total > 0 && pr.done < pr.total {
                            ui.add(egui::ProgressBar::new(pr.done as f32 / pr.total as f32).corner_radius(0.0).fill(p.accent).text(format!("{} / {}", transfer::human(pr.done), transfer::human(pr.total))));
                        }
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new(&pr.label).color(p.muted).size(11.5));
                        });
                        if !pr.received.is_empty() {
                            ui.label(RichText::new(format!("derniers fichiers reçus : {}", pr.received.len())).size(11.5));
                        }
                        ui.horizontal(|ui| {
                            if ui.button("arrêter").clicked() {
                                self.receiver = None;
                            }
                            if ui.button("ouvrir reçus/").clicked() {
                                if let Ok(path) = vault::safe(&self.root(), "reçus") {
                                    let _ = std::fs::create_dir_all(&path);
                                    settings::reveal(&path, false);
                                }
                            }
                        });
                    }
                }
            } else {
                ui.label(caps(&format!("fichiers · {} · {}", self.send_files.len(), transfer::human(transfer::total_size(&self.root(), &self.send_files)))));
                egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                    let mut remove = None;
                    for (k, f) in self.send_files.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.small_button("×").clicked() {
                                remove = Some(k);
                            }
                            ui.label(RichText::new(f).size(11.5));
                        });
                    }
                    if let Some(k) = remove {
                        self.send_files.remove(k);
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("+ fichiers de l'ordi…").clicked() {
                        if let Some(paths) = rfd::FileDialog::new().pick_files() {
                            let root = self.root();
                            for pth in paths {
                                let rel = vault::inside(&root, &pth).map(Ok).unwrap_or_else(|| vault::link_external(&root, &pth));
                                if let Some(rel) = self.report(rel) {
                                    self.send_files.push(rel);
                                }
                            }
                            self.links_dirty = true;
                        }
                    }
                    if let Some(d) = self.current().map(|d| d.rel.clone()) {
                        if ui.button("+ fichier ouvert").clicked() && !self.send_files.contains(&d) {
                            self.send_files.push(d);
                        }
                    }
                    ui.label(RichText::new("ou clic droit dans l'explorateur").color(p.muted).size(11.0));
                });
                ui.add_space(8.0);
                ui.label(caps("vers"));
                let peers = self.peers.lock().map(|p| p.clone()).unwrap_or_default();
                if peers.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("aucun ordinateur trouvé (l'autre doit être en mode « recevoir »)").color(p.muted).size(11.5));
                        if ui.small_button("chercher").clicked() {
                            transfer::discover(self.peers.clone(), ctx.clone());
                        }
                    });
                }
                for peer in peers {
                    let t = if self.send_addr == peer.addr { RichText::new(format!("▌ {} · {}", peer.name, peer.addr)).color(p.accent) } else { RichText::new(format!("  {} · {}", peer.name, peer.addr)) };
                    if ui.add(egui::Button::new(t).frame(false)).clicked() {
                        self.send_addr = peer.addr.clone();
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("adresse");
                    ui.add(egui::TextEdit::singleline(&mut self.send_addr).hint_text("192.168.1.42:47123").desired_width(200.0));
                    ui.label("code");
                    ui.add(egui::TextEdit::singleline(&mut self.send_code).hint_text("1234").desired_width(60.0));
                });
                ui.add_space(8.0);
                let ready = !self.send_files.is_empty() && !self.send_addr.trim().is_empty() && self.send_code.trim().len() == 4;
                let b = egui::Button::new(RichText::new("envoyer").color(p.on_accent).family(theme::bold())).fill(p.accent).stroke(egui::Stroke::NONE);
                if ui.add_enabled(ready, b).clicked() {
                    self.flush_all();
                    let job = transfer::send(self.root(), self.send_files.clone(), self.send_addr.trim().to_string(), self.send_code.clone(), ctx.clone());
                    self.jobs.push(job);
                }
            }
            // Progression des envois et exports en cours
            for j in &self.jobs {
                let pr = j.lock().map(|p| p.clone()).unwrap_or_default();
                if pr.total > 0 {
                    ui.add(egui::ProgressBar::new(pr.done as f32 / pr.total as f32).corner_radius(0.0).fill(p.accent).text(RichText::new(format!("{} · {} / {}", pr.label, transfer::human(pr.done), transfer::human(pr.total))).size(11.0)));
                }
            }
        });
        if r.should_close() || ctx.input(|i| i.key_pressed(Key::Escape)) {
            keep = false;
        }
        if keep {
            self.dialog = Some(Dialog::Transfer { send });
        }
        keep
    }


    // ---------- épinglage, mode compact, recherche ----------

    fn set_pinned(&mut self, ctx: &egui::Context, on: bool) {
        self.pinned = on;
        let level = if on { egui::viewport::WindowLevel::AlwaysOnTop } else { egui::viewport::WindowLevel::Normal };
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
    }

    /// Mode compact : juste le contenu, avec une barre minuscule pour en sortir.
    fn compact_bar(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        egui::Panel::top("compact").exact_size(26.0).frame(egui::Frame::new().fill(p.panel).inner_margin(egui::Margin::symmetric(8, 0))).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                theme::mark(ui, 11.0);
                for (label, v) in [("fichiers", View::Files), ("refs", View::Refs), ("boards", View::Boards)] {
                    let t = if self.view() == v { RichText::new(label).size(11.0).color(p.text) } else { RichText::new(label).size(11.0).color(p.muted) };
                    if ui.add(egui::Button::new(t).frame(false)).clicked() {
                        self.view = Some(v);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new("quitter le mode compact").size(11.0)).frame(false)).clicked() {
                        self.compact = false;
                    }
                    let pin = if self.pinned { "épinglé ✓" } else { "épingler" };
                    if ui.add(egui::Button::new(RichText::new(pin).size(11.0).color(if self.pinned { p.accent } else { p.muted })).frame(false)).clicked() {
                        self.set_pinned(ui.ctx(), !self.pinned);
                    }
                });
            });
        });
    }

    fn search_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| {
            egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("/").color(p.accent).size(18.0).family(theme::bold()));
                    let r = ui.add(egui::TextEdit::singleline(&mut self.search.input).hint_text("chercher dans tout le coffre (notes, code, texte, noms de fichiers)…").font(egui::FontId::monospace(15.0)).frame(egui::Frame::NONE).desired_width(f32::INFINITY));
                    if self.search.results.is_none() && self.search.input.is_empty() {
                        r.request_focus();
                    }
                });
            });
            self.search.run(self.root(), self.entries.clone(), ctx.clone());
            let Some(res) = self.search.results.clone() else {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| ui.label(RichText::new("tape au moins 2 caractères · majuscules et accents ignorés").color(p.muted)));
                return;
            };
            let (files, done, scanned, query) = {
                let r = res.lock().unwrap();
                (r.files.clone(), r.done, r.scanned, r.query.clone())
            };
            if !done {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            let total: usize = files.iter().map(|f| f.lines.len()).sum();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                if !done {
                    ui.spinner();
                }
                ui.label(RichText::new(format!("{total} résultats dans {} fichiers · {scanned} fichiers parcourus", files.len())).color(p.muted).size(11.5));
            });
            let mut open = None;
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 6)).show(ui, |ui| {
                    for f in &files {
                        ui.add_space(8.0);
                        let head = ui.add(egui::Button::new(RichText::new(&f.rel).family(theme::bold()).color(if f.name_match { p.accent } else { p.text })).frame(false));
                        if head.clicked() {
                            open = Some(f.rel.clone());
                        }
                        for l in &f.lines {
                            let r = ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                ui.label(RichText::new(format!("{:>5}  ", l.number)).color(p.muted).size(11.5));
                                for (part, hit) in search::highlight(&l.text, &query) {
                                    let t = RichText::new(part).size(12.5);
                                    ui.label(if hit { t.background_color(p.accent.linear_multiply(0.3)).color(p.text) } else { t.color(p.muted) });
                                }
                            });
                            if r.response.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                open = Some(f.rel.clone());
                            }
                        }
                    }
                });
            });
            if let Some(rel) = open {
                self.actions.push(Action::Open(rel));
            }
        });
    }

    // ---------- fenêtres ----------

    fn switcher(&mut self, ctx: &egui::Context) {
        let p = theme::pal();
        let Some((mut query, mut sel)) = self.switcher.take() else { return };
        let q = query.to_lowercase();
        let matches: Vec<String> = self
            .entries
            .iter()
            .filter(|e| !e.is_dir && e.rel.to_lowercase().contains(&q))
            .map(|e| e.rel.clone())
            .take(14)
            .collect();
        let (up, down, enter, esc) = ctx.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        if down && sel + 1 < matches.len() {
            sel += 1;
        }
        if up && sel > 0 {
            sel -= 1;
        }
        sel = sel.min(matches.len().saturating_sub(1));
        let mut chosen = None;
        let modal = egui::Modal::new(egui::Id::unique("switcher")).frame(egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.text)).inner_margin(12)).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(">").color(p.accent).family(theme::bold()));
                ui.add(egui::TextEdit::singleline(&mut query).hint_text("ouvrir un fichier, ou taper un nom pour créer une note").frame(egui::Frame::NONE).desired_width(f32::INFINITY)).request_focus();
            });
            ui.separator();
            for (i, rel) in matches.iter().enumerate() {
                let t = if i == sel { RichText::new(format!("▌ {rel}")).color(p.accent) } else { RichText::new(format!("  {rel}")).color(p.muted) };
                if ui.add(egui::Button::new(t).frame(false)).clicked() {
                    chosen = Some(rel.clone());
                }
            }
            if !query.trim().is_empty() && matches.is_empty() {
                ui.label(RichText::new(format!("  entrée : créer la note « {} »", query.trim())).color(p.muted).size(11.5));
            }
        });
        if enter {
            chosen = matches.get(sel).cloned();
            if chosen.is_none() && !query.trim().is_empty() {
                self.actions.push(Action::OpenLink(query.trim().to_string()));
                return;
            }
        }
        if let Some(rel) = chosen {
            self.actions.push(Action::Open(rel));
        } else if !(esc || modal.should_close()) {
            self.switcher = Some((query, sel));
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        // Une fenêtre s'affiche à l'image suivante : sinon le clic qui l'ouvre compte
        // comme un clic à l'extérieur et la referme aussitôt.
        if self.dialog.is_none() {
            self.dialog_shown = false;
            return;
        }
        if !self.dialog_shown {
            self.dialog_shown = true;
            ctx.request_repaint();
            return;
        }
        let p = theme::pal();
        let Some(dialog) = self.dialog.take() else { return };
        let (enter, esc) = ctx.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        let frame = egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.text)).inner_margin(16);
        let mut keep = true;
        let mut ok = false;

        let mut text_dialog = |title: &str, hint: &str, this: &mut Self| {
            let r = egui::Modal::new(egui::Id::unique("dialog")).frame(frame).show(ctx, |ui| {
                ui.set_width(460.0);
                ui.label(caps(title));
                ui.add_space(6.0);
                ui.add(egui::TextEdit::singleline(&mut this.dialog_text).hint_text(hint).desired_width(f32::INFINITY)).request_focus();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("ok").clicked() {
                        ok = true;
                    }
                    if ui.button("annuler").clicked() {
                        keep = false;
                    }
                });
            });
            if r.should_close() {
                keep = false;
            }
        };

        match &dialog {
            Dialog::NewFile { dir } => text_dialog(&format!("nouveau fichier dans /{dir}"), "nom.md, script.py, sous/dossier/notes.md…", self),
            Dialog::NewDir { dir } => text_dialog(&format!("nouveau dossier dans /{dir}"), "nom du dossier", self),
            Dialog::Rename { from } => text_dialog(&format!("renommer ou déplacer {from}"), "nouveau/chemin.md", self),
            Dialog::ConfirmTrash { rel } => {
                let r = egui::Modal::new(egui::Id::unique("trash")).frame(frame).show(ctx, |ui| {
                    ui.set_width(420.0);
                    ui.label(caps("supprimer"));
                    ui.add_space(6.0);
                    if vault::is_external(rel) {
                        ui.label(format!("« {rel} » est dans un dossier lié : il sera retiré de son dossier d'origine et mis dans la corbeille du coffre (.app/trash)."));
                    } else {
                        ui.label(format!("Mettre « {rel} » à la corbeille du coffre (.app/trash) ?"));
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("supprimer").color(p.missing)).clicked() {
                            ok = true;
                        }
                        if ui.button("annuler").clicked() {
                            keep = false;
                        }
                    });
                });
                if r.should_close() {
                    keep = false;
                }
            }
            Dialog::Settings(_) | Dialog::Transfer { .. } => {}
            Dialog::MoveIn { rel, copy } => {
                let (rel, copy) = (rel.clone(), *copy);
                let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
                let default_dir = if images::is_image(&rel) && self.entries.iter().any(|e| e.rel == "refs") { "refs".to_string() } else { self.notes_dir() };
                let mut go = None;
                let r = egui::Modal::new(egui::Id::unique("movein")).frame(frame).show(ctx, |ui| {
                    ui.set_width(460.0);
                    ui.label(caps(if copy { "copier dans le coffre" } else { "déplacer dans le coffre" }));
                    ui.add_space(4.0);
                    ui.label(&name);
                    if !copy {
                        ui.label(RichText::new("Le fichier quittera son dossier d'origine.").color(p.muted).size(11.0));
                    }
                    ui.add_space(8.0);
                    ui.label("dans quel dossier du coffre ?");
                    let dirs: Vec<String> = std::iter::once(String::new()).chain(self.entries.iter().filter(|e| e.is_dir && !vault::is_external(&e.rel)).map(|e| e.rel.clone())).collect();
                    for d in dirs {
                        let label = if d.is_empty() { "/ (racine)".to_string() } else { format!("/{d}") };
                        let t = if d == default_dir { RichText::new(label).color(p.accent).family(theme::bold()) } else { RichText::new(label) };
                        if ui.add(egui::Button::new(t).frame(false)).clicked() {
                            go = Some(d);
                        }
                    }
                    ui.add_space(8.0);
                    if ui.button("annuler").clicked() {
                        keep = false;
                    }
                });
                if let Some(dir) = go {
                    let root = self.root();
                    let target = vault::join(&dir, &name);
                    let res = if copy {
                        vault::safe(&root, &rel).and_then(|src| vault::import(&root, &dir, &src, false))
                    } else {
                        vault::rename(&root, &rel, &target).map(|_| target.clone())
                    };
                    if let Some(new_rel) = self.report(res) {
                        if !copy {
                            for d in &mut self.docs {
                                if d.rel == rel {
                                    d.rel = new_rel.clone();
                                }
                            }
                        }
                        self.refresh();
                        self.actions.push(Action::Open(new_rel));
                    }
                    return;
                }
                if r.should_close() || esc {
                    keep = false;
                }
                if keep {
                    self.dialog = Some(Dialog::MoveIn { rel, copy });
                }
                return;
            }
            Dialog::Import { files, dir, remember } => {
                let (files, dir, mut remember) = (files.clone(), dir.clone(), *remember);
                let mut choice = None;
                let r = egui::Modal::new(egui::Id::unique("import")).frame(frame).show(ctx, |ui| {
                    ui.set_width(480.0);
                    ui.label(caps("importer dans le coffre"));
                    ui.add_space(4.0);
                    let names: Vec<String> = files.iter().take(4).map(|f| f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).collect();
                    let more = if files.len() > 4 { format!(" et {} autres", files.len() - 4) } else { String::new() };
                    ui.label(format!("{}{} → /{}", names.join(", "), more, dir));
                    ui.add_space(8.0);
                    let link = egui::Button::new(RichText::new("lier (garder à sa place)").color(p.on_accent).family(theme::bold())).fill(p.accent).stroke(egui::Stroke::NONE);
                    if ui.add(link).clicked() {
                        choice = Some("link");
                    }
                    ui.label(RichText::new("Recommandé. Le fichier reste dans son dossier ; Aryadne l'affiche sans le copier.").color(p.muted).size(11.0));
                    ui.add_space(6.0);
                    if ui.button("déplacer dans le coffre").clicked() {
                        choice = Some("move");
                    }
                    ui.label(RichText::new("Le fichier quitte son dossier d'origine. Aucun doublon.").color(p.muted).size(11.0));
                    ui.add_space(6.0);
                    if ui.button("copier dans le coffre").clicked() {
                        choice = Some("copy");
                    }
                    ui.label(RichText::new("L'original reste en place : le fichier prend deux fois la place.").color(p.muted).size(11.0));
                    ui.add_space(8.0);
                    ui.checkbox(&mut remember, "retenir ce choix (modifiable dans les réglages)");
                    if ui.button("annuler").clicked() {
                        keep = false;
                    }
                });
                if let Some(mode) = choice {
                    if remember {
                        self.prefs.import_mode = mode.to_string();
                        self.prefs.save();
                    }
                    self.import_files(files, &dir, mode);
                    return;
                }
                if r.should_close() || esc {
                    keep = false;
                }
                if keep {
                    self.dialog = Some(Dialog::Import { files, dir, remember });
                }
                return;
            }
            Dialog::Update => {
                let up = self.updater.clone();
                let st = up.snapshot();
                let mut token_changed = false;
                let r = egui::Modal::new(egui::Id::unique("update")).frame(frame).show(ctx, |ui| {
                    ui.set_width(500.0);
                    ui.label(caps("mises à jour"));
                    ui.add_space(4.0);
                    ui.label(format!("Version installée : v{}", updater::CURRENT));
                    match up.update_available() {
                        Some(rel) => {
                            ui.label(RichText::new(format!("Nouvelle version : {}", rel.version)).color(p.accent).family(theme::bold()));
                            if !rel.notes.trim().is_empty() {
                                egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                                    ui.label(RichText::new(rel.notes.trim()).size(11.5).color(p.muted));
                                });
                            }
                        }
                        None if st.installed => {
                            ui.label(RichText::new("Mise à jour installée. Redémarre pour l'utiliser.").color(p.accent));
                        }
                        None if st.checked && st.error.is_none() => {
                            ui.label(RichText::new("Tu as la dernière version.").color(p.muted));
                        }
                        None => {}
                    }
                    if let Some(f) = st.progress {
                        ui.add(egui::ProgressBar::new(f).corner_radius(0.0).fill(p.accent).text(format!("téléchargement… {:.0} %", f * 100.0)));
                    }
                    if st.checking {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("vérification…");
                        });
                    }
                    if let Some(e) = &st.error {
                        ui.label(RichText::new(e).color(p.missing).size(12.0));
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if st.installed {
                            if ui.button(RichText::new("redémarrer").family(theme::bold())).clicked() {
                                self.flush_all();
                                self.save_board();
                                updater::restart(ctx);
                            }
                        } else if up.update_available().is_some() && st.progress.is_none() {
                            if ui.button(RichText::new("installer").family(theme::bold())).clicked() {
                                up.install(self.github_token.clone(), ctx);
                            }
                        }
                        if !st.checking && st.progress.is_none() && ui.button("vérifier").clicked() {
                            up.check(self.github_token.clone(), ctx);
                        }
                        if ui.button("fermer").clicked() {
                            keep = false;
                        }
                    });
                    ui.add_space(10.0);
                    ui.separator();
                    ui.label(RichText::new(format!("Repo privé ? Ajoute un jeton GitHub en lecture seule (Contents: read) pour {}.", updater::REPO)).color(p.muted).size(11.0));
                    token_changed = ui.add(egui::TextEdit::singleline(&mut self.github_token).password(true).hint_text("github_pat_…").desired_width(f32::INFINITY)).changed();
                });
                if token_changed {
                    save_config("github_token", self.github_token.trim());
                }
                if r.should_close() || esc {
                    keep = false;
                }
                if keep {
                    self.dialog = Some(Dialog::Update);
                }
                return;
            }
        }

        if let Dialog::Settings(st) = dialog {
            self.settings_window(ctx, st);
            return;
        }
        if let Dialog::Transfer { send } = dialog {
            self.transfer_dialog(ctx, send);
            return;
        }

        if esc {
            keep = false;
        }
        if enter || ok {
            let root = self.root();
            let name = self.dialog_text.trim().trim_matches('/').to_string();
            match &dialog {
                Dialog::NewFile { dir } if !name.is_empty() => {
                    let name = if name.contains('.') { name } else { format!("{name}.md") };
                    let rel = vault::join(dir, &name);
                    if self.report(vault::create_file(&root, &rel)).is_some() {
                        self.refresh();
                        self.open(rel);
                    }
                }
                Dialog::NewDir { dir } if !name.is_empty() => {
                    let rel = vault::join(dir, &name);
                    if self.report(vault::create_dir(&root, &rel)).is_some() {
                        self.selected = Some(rel);
                        self.refresh();
                    }
                }
                Dialog::Rename { from } if !name.is_empty() && &name != from => {
                    let from = from.clone();
                    self.do_rename(&from, &name);
                }
                Dialog::ConfirmTrash { rel } => {
                    let rel = rel.clone();
                    self.do_trash(&rel);
                }
                _ => {}
            }
            return;
        }
        if keep {
            self.dialog = Some(dialog);
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let p = theme::pal();
        let (hovering, dropped) = ctx.input(|i| (!i.raw.hovered_files.is_empty(), i.raw.dropped_files.clone()));
        let dir = self.target_dir(if self.entries.iter().any(|e| e.rel == "refs") { "refs" } else { "" });
        if hovering {
            let screen = ctx.content_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("drop")));
            painter.rect_filled(screen, 0.0, p.bg.linear_multiply(0.85));
            painter.rect_stroke(screen.shrink(16.0), 0.0, egui::Stroke::new(2.0, p.accent), egui::StrokeKind::Inside);
            painter.text(screen.center(), egui::Align2::CENTER_CENTER, format!("déposer pour importer dans /{dir}"), egui::FontId::monospace(16.0), p.text);
        }
        if !dropped.is_empty() {
            let root = self.root();
            let mut already = vec![];
            let mut outside = vec![];
            for f in dropped {
                // Déjà dans le coffre : on l'utilise tel quel, sans copie.
                match vault::inside(&root, f.path()) {
                    Some(rel) => already.push(rel),
                    None => outside.push(f.path().to_path_buf()),
                }
            }
            if !already.is_empty() {
                self.place_imported(already);
            }
            if !outside.is_empty() {
                match self.prefs.import_mode.as_str() {
                    "ask" => self.dialog = Some(Dialog::Import { files: outside, dir, remember: false }),
                    mode => {
                        let mode = mode.to_string();
                        self.import_files(outside, &dir, &mode);
                    }
                }
            }
        }
    }

    /// `mode` : « link » (garder en place), « move » ou « copy ». Les dossiers sont toujours liés.
    fn import_files(&mut self, files: Vec<PathBuf>, dir: &str, mode: &str) {
        let root = self.root();
        let mut imported = vec![];
        for f in files {
            let r = if mode == "link" || f.is_dir() { vault::link_external(&root, &f) } else { vault::import(&root, dir, &f, mode == "move") };
            if let Some(rel) = self.report(r) {
                imported.push(rel);
            }
        }
        self.refresh();
        self.place_imported(imported);
    }

    /// Après un import : sur le board, dans les refs, ou ouvert dans un onglet.
    fn place_imported(&mut self, rels: Vec<String>) {
        let last = rels.last().cloned();
        if self.view() == View::Boards {
            let center = self.board.view_center;
            for (k, rel) in rels.iter().enumerate() {
                let at = center + egui::vec2(k as f32 * 40.0, k as f32 * 40.0);
                self.board.drop_file(rel, at, &mut self.images);
            }
        } else if self.view() == View::Refs {
            self.refs.selected = last;
        } else if let Some(rel) = last {
            self.actions.push(Action::Open(rel));
        }
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.3);
                theme::logo(ui, 64.0);
                ui.add_space(12.0);
                ui.label(RichText::new("aryadne").size(30.0).family(theme::bold()));
                ui.label(RichText::new("// le fil entre tes idées").color(p.muted));
                ui.add_space(22.0);
                let btn = egui::Button::new(RichText::new("ouvrir ou créer un coffre").color(p.on_accent).family(theme::bold()))
                    .fill(p.accent)
                    .stroke(egui::Stroke::NONE)
                    .min_size(egui::vec2(260.0, 38.0));
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

        // Sauvegarde auto 500 ms après la dernière frappe, et à la fermeture.
        let closing = ctx.input(|i| i.viewport().close_requested());
        let mut waiting = false;
        for i in 0..self.docs.len() {
            if let Some(t) = self.docs[i].dirty_since {
                if closing || t.elapsed() > Duration::from_millis(self.prefs.autosave_ms) {
                    self.flush(i);
                    if Some(i) == self.active {
                        self.update_backlinks();
                    }
                } else {
                    waiting = true;
                }
            }
        }
        if waiting {
            ctx.request_repaint_after(Duration::from_millis(200));
        }

        self.images.poll(&ctx);
        self.poll_jobs();
        // Ctrl + / Ctrl − / Ctrl 0 (gérés par egui) : on retient la taille choisie.
        if self.dialog.is_none() && (ctx.zoom_factor() - self.prefs.ui_zoom).abs() > 0.001 && !ctx.input(|i| i.pointer.any_down()) {
            self.prefs.ui_zoom = ctx.zoom_factor();
            self.prefs.save();
        }
        if !self.jobs.is_empty() || self.receiver.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if self.board.dirty_since.is_some_and(|t| t.elapsed() > Duration::from_millis(600)) || (closing && self.board.dirty_since.is_some()) {
            self.save_board();
        }
        if self.board.dirty_since.is_some() || self.chat.busy() || self.assistant.busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        // Résultat du résumé IA
        if let Some(rx) = &self.summary_rx {
            if let Ok(r) = rx.try_recv() {
                if let Some((_, s)) = &mut self.summary {
                    *s = match r {
                        Ok(t) => Summary::Done(t),
                        Err(e) => Summary::Failed(e),
                    };
                }
                self.summary_rx = None;
            }
        }

        if self.vault.is_none() {
            self.welcome(ui);
        } else {
            self.handle_dropped_files(&ctx);
            if self.compact {
                self.compact_bar(ui);
            } else {
                self.topbar(ui);
                self.statusbar(ui);
                self.explorer(ui);
            }
            if self.assistant_open {
                self.assistant_panel(ui);
            }
            match self.view() {
                View::Files => {
                    if !self.compact && !self.assistant_open {
                        self.side_panel(ui);
                    }
                    self.editor(ui);
                }
                View::Search => self.search_view(ui),
                View::Brain => self.brain_view(ui),
                View::Refs => self.refs_view(ui),
                View::Boards => self.boards_view(ui),
                View::Chat => self.chat_view(ui),
            }
            if let Some((msg, t)) = self.toast.clone() {
                if t.elapsed() < Duration::from_secs(3) {
                    let p = theme::pal();
                    let screen = ctx.content_rect();
                    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("toast")));
                    let galley = painter.layout_no_wrap(msg, egui::FontId::monospace(12.5), p.on_accent);
                    let r = egui::Rect::from_center_size(egui::pos2(screen.center().x, screen.bottom() - 60.0), galley.size() + egui::vec2(28.0, 16.0));
                    painter.rect_filled(r, 0.0, p.text);
                    painter.galley(r.min + egui::vec2(14.0, 8.0), galley, p.bg);
                    ctx.request_repaint_after(Duration::from_millis(200));
                } else {
                    self.toast = None;
                }
            }
            self.switcher(&ctx);
            self.dialogs(&ctx);
            self.run_actions(&ctx);
        }

        if let Some(e) = self.error.clone() {
            let p = theme::pal();
            let r = egui::Modal::new(egui::Id::unique("error")).frame(egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.missing)).inner_margin(16)).show(&ctx, |ui| {
                ui.set_max_width(440.0);
                ui.label(caps("erreur"));
                ui.label(RichText::new(&e).color(p.missing));
                ui.add_space(6.0);
                ui.button("ok").clicked()
            });
            if r.inner || r.should_close() {
                self.error = None;
            }
        }
    }

    fn on_exit(&mut self) {
        self.flush_all();
        self.save_board();
        self.local.stop();
    }
}

/// Section « IA locale » des réglages : installer, démarrer, télécharger un modèle. Aucun admin.
fn ollama_panel(ui: &mut egui::Ui, local: &ollama::Local, s: &ai::Settings, ctx: &egui::Context) {
    let p = theme::pal();
    let st = local.snapshot();
    ui.label(caps("ia locale · ollama"));
    ui.add_space(4.0);
    let has_model = st.models.iter().any(|m| m == &s.model || m.starts_with(&format!("{}:", s.model)));
    let (dot, status) = if !st.checked {
        (p.muted, "vérification…".to_string())
    } else if st.running {
        (egui::Color32::from_rgb(0x22, 0xa0, 0x55), format!("en marche · {} modèle(s)", st.models.len()))
    } else if st.binary.is_some() {
        (p.accent, "installé, arrêté".to_string())
    } else {
        (p.missing, "pas installé".to_string())
    };
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        ui.painter().rect_filled(r, 0.0, dot);
        ui.label(status);
        if let Some(b) = &st.binary {
            ui.label(RichText::new(b.to_string_lossy()).color(p.muted).size(10.5));
        }
    });
    if !st.models.is_empty() {
        ui.label(RichText::new(st.models.join("  ·  ")).color(p.muted).size(11.0));
    }
    ui.add_space(6.0);
    if let Some((msg, progress)) = &st.task {
        match progress {
            Some(f) => {
                ui.add(egui::ProgressBar::new(*f).text(RichText::new(msg).size(11.0)).corner_radius(0.0).fill(p.accent));
            }
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(msg).size(11.5));
                });
            }
        }
        return;
    }
    ui.horizontal(|ui| {
        if st.checked && st.binary.is_none() && !st.running {
            if ui.button("installer ollama (sans admin)").clicked() {
                local.install(&s.url, ctx);
            }
        }
        if st.binary.is_some() && !st.running && ui.button("démarrer").clicked() {
            let l = local.clone();
            let (url, c) = (s.url.clone(), ctx.clone());
            std::thread::spawn(move || {
                let r = l.ensure_running(&url);
                if let (Err(e), Ok(mut st)) = (r, l.state.lock()) {
                    st.error = Some(e);
                }
                l.refresh(&url, &c);
            });
        }
        if (st.running || st.binary.is_some()) && !has_model && !s.model.trim().is_empty() {
            if ui.button(format!("télécharger {}", s.model.trim())).clicked() {
                local.pull(&s.url, &s.model, ctx);
            }
        }
        if st.binary.is_some() && ui.button("mettre à jour").on_hover_text("Retélécharge la dernière version d'Ollama").clicked() {
            local.install(&s.url, ctx);
        }
        if ui.small_button("↻").on_hover_text("Revérifier").clicked() {
            local.refresh(&s.url, ctx);
        }
    });
    if st.checked && st.binary.is_none() && !st.running {
        ui.label(RichText::new(format!("~2 Go, installé dans {}", ollama::install_dir().to_string_lossy())).color(p.muted).size(10.5));
    }
    if let Some(e) = &st.error {
        ui.label(RichText::new(e).color(p.missing).size(11.5));
    }
}

#[allow(clippy::too_many_arguments)]
fn msg_block(
    ui: &mut egui::Ui,
    user: bool,
    text: &str,
    sources: &[String],
    error: Option<&str>,
    status: Option<&str>,
    existing: &HashSet<String>,
    paths: &HashSet<String>,
    goto: &mut Option<String>,
) {
    let p = theme::pal();
    ui.add_space(10.0);
    let (who, color) = if user { ("vous ›", p.muted) } else { ("aryadne ›", p.accent) };
    ui.label(RichText::new(who).color(color).family(theme::bold()).size(12.0));
    let fill = if user { p.panel } else { p.bg };
    egui::Frame::new().fill(fill).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(12).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if let Some(s) = status {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(s).color(p.muted).size(12.0));
            });
        }
        if user {
            ui.label(text);
        } else if !text.is_empty() {
            if let Some(n) = preview::show(ui, text, existing, paths) {
                *goto = Some(n);
            }
        }
        if let Some(e) = error {
            ui.label(RichText::new(e).color(p.missing).size(12.0));
        }
    });
    if !sources.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("sources :").color(p.muted).size(11.0));
            for s in sources {
                let name = if vault::is_md(s) { vault::stem(s) } else { s.clone() };
                if ui.add(egui::Button::new(RichText::new(name).size(11.0).color(p.link))).on_hover_text(s).clicked() {
                    *goto = Some(s.clone());
                }
            }
        });
    }
}

fn self_video_placeholder() -> Content {
    Content::Unreadable("rechargement…".into())
}

/// Blocs de code ``` d'une réponse de l'assistant.
fn code_blocks(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur: Option<String> = None;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            match cur.take() {
                Some(b) => out.push(b.trim_end_matches('\n').to_string()),
                None => cur = Some(String::new()),
            }
        } else if let Some(b) = &mut cur {
            b.push_str(line);
            b.push('\n');
        }
    }
    out
}

#[derive(Clone, serde::Deserialize)]
struct OrganizeMove {
    from: String,
    to_dir: String,
    #[serde(default)]
    why: String,
}

struct Organize {
    rx: Option<std::sync::mpsc::Receiver<Result<Vec<OrganizeMove>, String>>>,
    moves: Vec<(bool, OrganizeMove)>,
    error: Option<String>,
}

/// Extrait le JSON de la réponse (les modèles ajoutent parfois du texte ou des ```).
fn parse_moves(t: &str) -> Result<Vec<OrganizeMove>, String> {
    #[derive(serde::Deserialize)]
    struct Plan {
        moves: Vec<OrganizeMove>,
    }
    let start = t.find('{').ok_or("Réponse de l'IA illisible (pas de JSON).")?;
    let end = t.rfind('}').ok_or("Réponse de l'IA illisible.")?;
    let plan: Plan = serde_json::from_str(&t[start..=end]).map_err(|e| format!("Réponse de l'IA illisible : {e}"))?;
    Ok(plan.moves.into_iter().map(|mut m| {
        m.to_dir = m.to_dir.trim_matches('/').to_string();
        m.from = m.from.trim_start_matches('/').to_string();
        m
    }).collect())
}
