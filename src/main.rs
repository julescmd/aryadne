#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cloud;
mod bridge;
mod dock;
mod formats;
mod boardlive;
mod gizmo;
mod paint;
mod graph;
mod untitled;
mod meshedit;
mod montage;
mod montage_ui;
mod scene;
mod workspace;
mod retopo;
mod bvh;
mod convert;
mod uvedit;
mod zoomview;
mod i18n;
mod fbxwrite;
mod cloud_ui;
mod lsp;
mod ai;
mod blend;
mod board;
mod brain;
mod chat;
mod code;
mod desktop;
mod extras;
mod files;
mod fbx;
mod ffmpeg;
mod images;
mod live;
mod modifiers;
mod nodes;
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
    let prefs = settings::Prefs::load();
    if prefs.menu_entry && !desktop::installed() {
        let _ = desktop::install();
    }
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Aryadne")
            .with_app_id(APP)
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([760.0, 480.0])
            .with_drag_and_drop(true)
            .with_icon(std::sync::Arc::new(theme::window_icon().unwrap_or_default())),
        ..Default::default()
    };
    // Sous Wayland, winit ne gère pas le glisser-déposer de fichiers : on passe par
    // XWayland (X11) sauf si l'utilisateur préfère Wayland natif.
    #[cfg(target_os = "linux")]
    if !prefs.wayland_native && std::env::var_os("DISPLAY").is_some() {
        options.event_loop_builder = Some(Box::new(|b| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            b.with_x11();
        }));
    }
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
    Tasks,
    Chat,
    Search,
}

fn view_of(id: &str) -> View {
    match id {
        "brain" => View::Brain,
        "refs" => View::Refs,
        "boards" => View::Boards,
        "tasks" => View::Tasks,
        "chat" => View::Chat,
        "search" => View::Search,
        _ => View::Files,
    }
}

/// Options de la fenêtre « nouveau fichier », gardées d'une fois à l'autre.
#[derive(Clone)]
struct NewOpts {
    img_size: usize,
    img_white: bool,
    scene_starter: bool,
    res: usize,
    fps: usize,
    preset: usize,
    code: usize,
}

impl Default for NewOpts {
    fn default() -> Self {
        NewOpts { img_size: 0, img_white: true, scene_starter: true, res: 0, fps: 0, preset: 0, code: 0 }
    }
}

const NF_RES: [(&str, u32, u32); 6] = [("1920 × 1080 (HD)", 1920, 1080), ("3840 × 2160 (4K)", 3840, 2160), ("1280 × 720", 1280, 720), ("1080 × 1920 (vertical)", 1080, 1920), ("1080 × 1080 (carré)", 1080, 1080), ("2048 × 858 (cinéma 2.39)", 2048, 858)];
const NF_IMG: [(&str, usize, usize); 6] = [("1920 × 1080", 1920, 1080), ("2048 × 2048 (texture)", 2048, 2048), ("1080 × 1350 (portrait)", 1080, 1350), ("3840 × 2160 (4K)", 3840, 2160), ("A4 à 150 ppp", 1240, 1754), ("1024 × 1024", 1024, 1024)];
const NF_FPS: [f32; 5] = [24.0, 25.0, 30.0, 50.0, 60.0];
const NF_CODE: [(&str, &str); 10] = [("py", "Python"), ("rs", "Rust"), ("js", "JavaScript"), ("ts", "TypeScript"), ("html", "HTML"), ("css", "CSS"), ("json", "JSON"), ("glsl", "GLSL"), ("sh", "Shell"), ("txt", "texte brut")];

/// Pages qu'on peut ouvrir dans une fenêtre à part (menu « fenêtre »), autant de fois qu'on veut.
const VIEW_WINDOWS: [(&str, &str); 6] = [("cerveau", "brain"), ("références", "refs"), ("boards", "boards"), ("tâches", "tasks"), ("chat", "chat"), ("recherche", "search")];

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
    Montage(Box<montage::Editor>),
    Nodes(Box<nodes::Editor>),
    /// Image à calques (.aryimg)
    Paint(Box<paint::Editor>),
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

/// Une opération sur les fichiers qu'on peut annuler depuis l'explorateur.
enum FileOp {
    /// (chemin d'origine, emplacement dans la corbeille)
    Trashed(String, PathBuf),
    /// (ancien chemin, nouveau chemin)
    Renamed(String, String),
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
    files: files::FilesState,
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
    palette: Option<(String, usize)>,
    template_dlg: Option<extras::TemplateDlg>,
    history_dlg: Option<extras::HistoryDlg>,
    url_dlg: Option<String>,
    url_rx: Option<Receiver<Result<String, String>>>,
    backup_rx: Option<Receiver<Result<PathBuf, String>>>,
    cloud: cloud::Sync,
    cloud_cfg: Option<cloud::Config>,
    cloud_next: Option<Instant>,
    cloud_seen: Option<Instant>,
    relay: cloud::Sync,
    convert: Option<convert::Dlg>,
    /// Fichier affiché à droite en vue côte à côte.
    split: Option<String>,
    in_split: bool,
    split_swap: bool,
    relay_asked: bool,
    relay_done_seen: Option<Instant>,
    relay_list: std::sync::Arc<std::sync::Mutex<Option<Result<Vec<cloud::Package>, String>>>>,
    timer: Option<extras::Timer>,
    tasks: extras::TasksState,
    tags: Option<(Instant, Vec<(String, usize)>)>,
    stats: Option<extras::Stats>,
    want_format: bool,
    want_blender: bool,
    want_term: bool,
    terminal: Option<extras::Terminal>,
    proj_words: HashMap<String, (Instant, std::sync::Arc<Vec<String>>)>,
    run: Option<extras::Run>,
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
    ws: workspace::Set,
    /// Onglets affichés dans leur propre panneau (déplaçable, détachable), en plus de la zone centrale
    panes: Vec<String>,
    /// Panneau en cours d'affichage (pour son en-tête)
    pane_now: Option<usize>,
    ws_started: bool,
    /// On change d'espace pour ouvrir un fichier : pas de scène ouverte d'office
    ws_routing: bool,
    nf_opt: NewOpts,
    /// Fichiers sans titre : empreinte à la création (pour savoir s'ils ont changé)
    untitled_hash: HashMap<String, u64>,
    /// Fermeture d'un fichier sans titre modifié : on demande quoi faire
    close_ask: Option<String>,
    recover: Vec<String>,
    recover_checked: bool,
    want_save_as: bool,
    nf_age: u32,
    /// Explorateur ouvert (Ctrl+B) ; panneau « structure » masqué ; fenêtres de pages ajoutées
    explorer_open: bool,
    side_hidden: bool,
    view_panels: Vec<String>,
    /// Zones de l'interface (image précédente) : la barre du bas montre les raccourcis de celle sous la souris
    zones_last: Vec<(&'static str, egui::Rect)>,
    zones_next: Vec<(&'static str, egui::Rect)>,
    ws_name: String,
    /// Nom tapé dans l'écran d'accueil (nouveau fichier de code)
    landing_name: String,
    /// Fenêtre « nouveau fichier » : genre choisi et nom tapé
    new_file: Option<(usize, String)>,
    /// Explorateur : éléments choisis en plus de `selected` (Ctrl + clic, Maj + clic)
    multi: Vec<String>,
    /// L'explorateur a reçu le dernier clic : F2, Suppr et Ctrl+Z s'adressent à lui
    explorer_focus: bool,
    explorer_rect: Option<egui::Rect>,
    /// Éléments affichés dans l'explorateur, dans l'ordre (image en cours, image précédente)
    explorer_order: Vec<String>,
    explorer_prev: Vec<String>,
    /// Opérations sur les fichiers à annuler, par lots
    file_undo: Vec<Vec<FileOp>>,
    undoing: bool,
}

impl App {
    fn new(ctx: &egui::Context) -> Self {
        egui_extras::install_image_loaders(ctx);
        theme::install_fonts(ctx);
        let prefs = settings::Prefs::load();
        i18n::set(if prefs.lang.is_empty() { "fr" } else { &prefs.lang });
        prefs.apply(ctx);
        let mut app = Self { view: Some(View::Files), ai: Some(ai::Settings::load()), prefs, ..Default::default() };
        app.ctx = Some(ctx.clone());
        { let u = app.ai.clone().unwrap_or_default().url; app.local.refresh(if ollama::is_local(&u) { &u } else { ai::PROVIDERS[0].url }, ctx); }
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

    // ---------- espaces de travail ----------

    /// Range l'état actuel (onglets, vue, panneaux) dans l'espace en cours.
    fn ws_capture(&mut self) {
        let view = match self.view() {
            View::Files => "files",
            View::Brain => "brain",
            View::Refs => "refs",
            View::Boards => "boards",
            View::Tasks => "tasks",
            View::Chat => "chat",
            View::Search => "search",
        };
        let w = &mut self.ws.list[self.ws.current];
        w.places = dock::export();
        w.tabs = self.docs.iter().map(|d| d.rel.clone()).collect();
        w.active = self.active.and_then(|i| self.docs.get(i)).map(|d| d.rel.clone());
        w.split = self.split.clone();
        w.panes = self.panes.clone();
        w.view = view.to_string();
        w.assistant = self.assistant_open;
        w.explorer = self.explorer_open;
        w.views = self.view_panels.clone();
    }

    /// Met en place l'espace en cours : panneaux, couleur, onglets, vue.
    fn ws_apply(&mut self, ctx: &egui::Context, tabs: bool) {
        let w = self.ws.cur().clone();
        dock::import(w.places);
        self.explorer_open = w.explorer;
        self.view_panels = w.views.clone();
        theme::set_workspace_accent(w.accent);
        theme::apply(ctx, self.prefs.dark);
        if !tabs {
            return;
        }
        self.assistant_open = w.assistant;
        self.flush_all();
        self.docs.clear();
        self.active = None;
        self.split = None;
        self.panes.clear();
        let known: std::collections::HashSet<&str> = self.entries.iter().map(|e| e.rel.as_str()).collect();
        let root = self.root();
        let tabs: Vec<String> = w.tabs.iter().filter(|t| (known.contains(t.as_str()) || (untitled::is_untitled(t) && std::path::Path::new(&root).join(t.as_str()).exists())) && workspace::accepts(&w.kind, t)).cloned().collect();
        for t in &tabs {
            self.open_here(t.clone());
        }
        if let Some(a) = w.active.filter(|a| tabs.contains(a)) {
            self.open_here(a);
        }
        self.split = w.split.filter(|s| tabs.contains(s));
        self.panes = w.panes.into_iter().filter(|s| tabs.contains(s)).collect();
        let pages = workspace::pages(&w.kind);
        self.view = Some(view_of(if pages.iter().any(|(_, v)| *v == w.view) { w.view.as_str() } else { pages[0].1 }));
        self.update_backlinks();
        // Espace 3D sans rien d'ouvert : on arrive dans une scène, la dernière utilisée ou une neuve (cube, lumière, caméra)
        if w.kind == "3d" && self.docs.is_empty() && !self.entries.is_empty() && !std::mem::take(&mut self.ws_routing) {
            let last = self.entries.iter().filter(|e| !e.is_dir && formats::has(&e.rel, "scene")).max_by_key(|e| e.modified).map(|e| e.rel.clone());
            match last {
                Some(rel) => self.open_here(rel),
                None => self.run_cmd(extras::Cmd::NewScene, ctx),
            }
        }
    }

    /// Fenêtre « nouveau fichier » : le genre, puis ses options. Le fichier s'ouvre sous le nom `untitled.<ext>` ;
    /// il ne prend un nom et une place dans le coffre qu'au premier Ctrl+S (seul le dossier demande un nom tout de suite).
    fn new_file_window(&mut self, ctx: &egui::Context) {
        let Some((mut kind, mut name)) = self.new_file.take() else {
            self.nf_age = 0;
            return;
        };
        // La touche Entrée qui a ouvert la fenêtre (palette) ne doit pas aussi la valider
        self.nf_age += 1;
        let fresh = self.nf_age < 3;
        let p = theme::pal();
        // (nom, extension, à quoi ça sert)
        let mut kinds: Vec<(&str, &str, &str)> = vec![("note", "md", "Texte en Markdown : notes, listes, liens entre notes")];
        kinds.extend(formats::ALL.iter().filter(|f| f.ready).map(|f| (f.label, f.ext, f.help)));
        kinds.push(("code ou texte", "code", "Script, page web, réglages, texte brut"));
        kinds.push(("dossier", "/", "Pour ranger"));
        kind = kind.min(kinds.len() - 1);
        let (mut keep, mut go) = (true, false);
        let mut o = self.nf_opt.clone();
        egui::Window::new(crate::i18n::t("nouveau fichier")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.set_min_width(560.0);
            ui.horizontal_top(|ui| {
                // À gauche : les genres
                ui.vertical(|ui| {
                    ui.set_width(220.0);
                    for (k, (label, ext, _)) in kinds.iter().enumerate() {
                        let on = kind == k;
                        let mut job = egui::text::LayoutJob::default();
                        job.append(&crate::i18n::t(label), 0.0, egui::TextFormat { font_id: egui::FontId::new(13.0, if on { theme::bold() } else { egui::FontFamily::Proportional }), color: if on { p.text } else { p.muted }, ..Default::default() });
                        if !matches!(*ext, "/" | "code") {
                            job.append(&format!("  .{ext}"), 0.0, egui::TextFormat { font_id: egui::FontId::monospace(10.5), color: p.accent, ..Default::default() });
                        }
                        if ui.add(egui::Button::selectable(on, job).min_size(egui::vec2(214.0, 26.0))).clicked() {
                            kind = k;
                        }
                    }
                });
                ui.separator();
                // À droite : ce que fait ce genre, et ses options
                ui.vertical(|ui| {
                    let (label, ext, help) = kinds[kind];
                    ui.label(RichText::new(crate::i18n::t(label)).size(16.0).family(theme::bold()));
                    ui.label(RichText::new(crate::i18n::t(help)).size(11.0).color(p.muted));
                    ui.add_space(10.0);
                    ui.label(caps(&crate::i18n::t("options")));
                    match ext {
                        "aryscene" => {
                            ui.radio_value(&mut o.scene_starter, true, crate::i18n::t("cube, lumière et caméra"));
                            ui.radio_value(&mut o.scene_starter, false, crate::i18n::t("scène vide"));
                        }
                        "aryimg" => {
                            egui::ComboBox::from_id_salt("nf-img").selected_text(NF_IMG[o.img_size].0).show_ui(ui, |ui| {
                                for (k, (l, _, _)) in NF_IMG.iter().enumerate() {
                                    ui.selectable_value(&mut o.img_size, k, crate::i18n::t(l));
                                }
                            });
                            ui.radio_value(&mut o.img_white, true, crate::i18n::t("fond blanc"));
                            ui.radio_value(&mut o.img_white, false, crate::i18n::t("fond transparent"));
                        }
                        "aryseq" => {
                            egui::ComboBox::from_id_salt("nf-res").selected_text(NF_RES[o.res].0).show_ui(ui, |ui| {
                                for (k, (l, _, _)) in NF_RES.iter().enumerate() {
                                    ui.selectable_value(&mut o.res, k, *l);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(crate::i18n::t("images/s"));
                                for (k, f) in NF_FPS.iter().enumerate() {
                                    ui.selectable_value(&mut o.fps, k, format!("{f}"));
                                }
                            });
                        }
                        "arymat" => {
                            let cur = if o.preset == 0 { crate::i18n::t("vide (sortie seule)") } else { crate::i18n::t(nodes::MATERIAL_PRESETS[o.preset - 1]) };
                            egui::ComboBox::from_id_salt("nf-preset").selected_text(cur).height(320.0).show_ui(ui, |ui| {
                                ui.selectable_value(&mut o.preset, 0, crate::i18n::t("vide (sortie seule)"));
                                for (k, n) in nodes::MATERIAL_PRESETS.iter().enumerate() {
                                    ui.selectable_value(&mut o.preset, k + 1, crate::i18n::t(n));
                                }
                            });
                        }
                        "code" => {
                            ui.horizontal_wrapped(|ui| {
                                for (k, (e, l)) in NF_CODE.iter().enumerate() {
                                    ui.selectable_value(&mut o.code, k, *l).on_hover_text(format!(".{e}"));
                                }
                            });
                        }
                        "/" | "aryboard" => {
                            let r = ui.add(egui::TextEdit::singleline(&mut name).hint_text(crate::i18n::t("nom")).desired_width(260.0));
                            if !r.has_focus() && name.is_empty() {
                                r.request_focus();
                            }
                            go |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        }
                        _ => {
                            ui.label(RichText::new(crate::i18n::t("rien à régler")).size(11.0).color(p.muted));
                        }
                    }
                    ui.add_space(12.0);
                    if !matches!(ext, "/" | "aryboard") {
                        let shown = if ext == "code" { format!("untitled.{}", NF_CODE[o.code].0) } else { format!("untitled.{ext}") };
                        ui.label(RichText::new(format!("{} {shown} · {}", crate::i18n::t("s'ouvre sous le nom"), crate::i18n::t("Ctrl+S pour choisir son nom et son dossier"))).size(10.5).color(p.muted));
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        go |= ui.add(egui::Button::new(RichText::new(crate::i18n::t("créer")).color(p.on_accent)).fill(p.accent)).clicked();
                        if ui.button(crate::i18n::t("annuler")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            keep = false;
                        }
                    });
                });
            });
            if !fresh && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !matches!(kinds[kind].1, "/" | "aryboard") {
                go = true;
            }
        });
        self.nf_opt = o.clone();
        if go {
            let ext = kinds[kind].1;
            let clean = name.trim().trim_matches('/').replace('\\', "/");
            match ext {
                "/" => {
                    if clean.is_empty() {
                        self.new_file = Some((kind, name));
                        return;
                    }
                    let rel = vault::join(&self.target_dir(""), &clean);
                    let r = vault::safe(&self.root(), &rel).and_then(|path| std::fs::create_dir_all(path).map_err(|e| e.to_string()));
                    if self.report(r).is_some() {
                        self.refresh();
                    }
                }
                "aryboard" => {
                    if let Some(rel) = self.new_board(if clean.is_empty() { "Moodboard" } else { &clean }) {
                        self.open(rel);
                    }
                }
                "aryscene" => {
                    let starter = o.scene_starter;
                    self.new_untitled(ext, move |p| if starter { scene::Scene::starter() } else { scene::Scene::default() }.write(p));
                }
                "aryseq" => {
                    let (_, w, h) = NF_RES[o.res];
                    let fps = NF_FPS[o.fps];
                    self.new_untitled(ext, move |p| montage::Project { width: w, height: h, fps, ..Default::default() }.write(p));
                }
                "arymat" => {
                    let g = if o.preset == 0 { nodes::Graph::new_material() } else { nodes::material_preset(nodes::MATERIAL_PRESETS[o.preset - 1]).unwrap_or_else(nodes::Graph::new_material) };
                    self.new_untitled(ext, move |p| g.save(p));
                }
                "arycomp" => self.new_untitled(ext, |p| nodes::Graph::new_compositor().save(p)),
                "aryimg" => {
                    let (_, w, h) = NF_IMG[o.img_size];
                    let white = o.img_white;
                    self.new_untitled(ext, move |p| paint::Doc::new(w, h, white).save(p));
                }
                "code" => self.new_untitled(NF_CODE[o.code].0, |p| std::fs::write(p, "").map_err(|e| e.to_string())),
                _ => self.new_untitled(ext, |p| std::fs::write(p, "").map_err(|e| e.to_string())),
            }
            return;
        }
        if keep {
            self.new_file = Some((kind, name));
        }
    }

    /// Crée un fichier sans titre (`untitled.<ext>`) dans le dossier caché, et l'ouvre.
    fn new_untitled(&mut self, ext: &str, write: impl FnOnce(&std::path::Path) -> Result<(), String>) {
        let root = self.root();
        let rel = untitled::new_rel(&root, ext);
        let path = std::path::Path::new(&root).join(&rel);
        let r = path.parent().map(|d| std::fs::create_dir_all(d).map_err(|e| e.to_string())).unwrap_or(Ok(())).and_then(|_| write(&path));
        if self.report(r).is_some() {
            self.untitled_hash.insert(rel.clone(), untitled::hash(&root, &rel));
            self.open(rel);
        }
    }

    /// Un fichier sans titre a-t-il changé depuis sa création ?
    fn untitled_changed(&self, rel: &str) -> bool {
        self.untitled_hash.get(rel).is_none_or(|h| *h != untitled::hash(&self.root(), rel))
    }

    /// Ctrl+S sur un fichier sans titre : on choisit son nom et son dossier, puis il se rouvre à sa vraie place.
    fn save_as(&mut self, i: usize) {
        self.flush_all();
        let Some(rel) = self.docs.get(i).map(|d| d.rel.clone()) else { return };
        let root = self.root();
        let e = ext(&rel);
        let fallback = match e.as_str() {
            "md" => self.notes_dir(),
            "arymat" | "arycomp" => "nodes".to_string(),
            "aryscene" => "3d".to_string(),
            "aryseq" => "montage".to_string(),
            _ => String::new(),
        };
        let start = std::path::Path::new(&root).join(self.target_dir(&fallback));
        let mut d = rfd::FileDialog::new().set_title(crate::i18n::t("enregistrer sous")).set_directory(start).set_file_name(format!("untitled.{e}"));
        if !e.is_empty() {
            d = d.add_filter(&e, &[e.as_str()]);
        }
        let Some(mut dest) = d.save_file() else { return };
        if !e.is_empty() && dest.extension().is_none() {
            dest.set_extension(&e);
        }
        if let Err(err) = untitled::save_to(&root, &rel, &dest) {
            self.error = Some(err);
            return;
        }
        self.untitled_hash.remove(&rel);
        if i < self.docs.len() {
            self.docs.remove(i);
            self.active = if self.docs.is_empty() { None } else { Some(self.active.unwrap_or(0).min(self.docs.len() - 1)) };
        }
        self.refresh();
        match vault::inside(&root, &dest) {
            Some(new_rel) => {
                self.open(new_rel.clone());
                self.toast(format!("{} {new_rel}", crate::i18n::t("enregistré :")));
            }
            None => self.toast(format!("{} {}", crate::i18n::t("enregistré hors du coffre :"), dest.to_string_lossy())),
        }
    }

    /// Fenêtres liées aux fichiers sans titre : fermer sans enregistrer, et récupération après un plantage.
    fn untitled_dialogs(&mut self, ctx: &egui::Context) {
        let p = theme::pal();
        if let Some(rel) = self.close_ask.clone() {
            let mut choice = 0;
            egui::Window::new(crate::i18n::t("fichier non enregistré")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label(format!("« {} » {}", untitled::name(&rel), crate::i18n::t("n'a pas encore de nom ni de place dans le coffre.")));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new(crate::i18n::t("enregistrer…")).color(p.on_accent)).fill(p.accent)).clicked() {
                        choice = 1;
                    }
                    if ui.button(crate::i18n::t("ne pas enregistrer")).clicked() {
                        choice = 2;
                    }
                    if ui.button(crate::i18n::t("annuler")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        choice = 3;
                    }
                });
            });
            let idx = self.docs.iter().position(|d| d.rel == rel);
            match (choice, idx) {
                (1, Some(i)) => {
                    self.close_ask = None;
                    self.save_as(i);
                }
                (2, Some(i)) => {
                    self.close_ask = None;
                    untitled::discard(&self.root(), &rel);
                    self.untitled_hash.remove(&rel);
                    self.docs.remove(i);
                    self.active = if self.docs.is_empty() { None } else { Some(self.active.unwrap_or(0).min(self.docs.len() - 1)) };
                }
                (3, _) | (_, None) if choice != 0 || idx.is_none() => self.close_ask = None,
                _ => {}
            }
        }
        // Au lancement : des fichiers sans titre restés là (plantage, ou fermés sans enregistrer)
        if !self.recover_checked && self.ws_started && self.vault.is_some() {
            self.recover_checked = true;
            let open: HashSet<String> = self.docs.iter().map(|d| d.rel.clone()).collect();
            self.recover = untitled::leftovers(&self.root()).into_iter().filter(|r| !open.contains(r)).collect();
        }
        if !self.recover.is_empty() {
            let mut choice = 0;
            egui::Window::new(crate::i18n::t("fichiers à récupérer")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label(crate::i18n::t("Ces fichiers n'avaient pas été enregistrés quand Aryadne s'est fermé :"));
                ui.add_space(4.0);
                for r in &self.recover {
                    ui.label(RichText::new(format!("• {}", untitled::name(r))).monospace().size(12.0));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new(crate::i18n::t("les rouvrir")).color(p.on_accent)).fill(p.accent)).clicked() {
                        choice = 1;
                    }
                    if ui.button(crate::i18n::t("les supprimer")).clicked() {
                        choice = 2;
                    }
                });
            });
            let list = std::mem::take(&mut self.recover);
            match choice {
                1 => {
                    for r in list {
                        // Empreinte inconnue : ils comptent comme modifiés, la fermeture demandera quoi faire
                        self.untitled_hash.insert(r.clone(), 0);
                        self.open(r);
                    }
                }
                2 => {
                    let root = self.root();
                    for r in list {
                        untitled::discard(&root, &r);
                    }
                }
                _ => self.recover = list,
            }
        }
    }

    /// Écran d'accueil de l'espace de travail, quand rien n'est ouvert : de quoi créer, et les fichiers récents du bon genre.
    fn landing(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let ctx = ui.ctx().clone();
        let w = self.ws.cur().clone();
        const CODE: [&str; 28] = ["rs", "py", "js", "ts", "tsx", "jsx", "html", "css", "json", "toml", "yaml", "yml", "c", "h", "cpp", "hpp", "java", "go", "lua", "sh", "glsl", "wgsl", "cs", "php", "rb", "swift", "kt", "sql"];
        let ext = |rel: &str| rel.rsplit('.').next().unwrap_or("").to_lowercase();
        type Pick = fn(&str) -> bool;
        let (subtitle, actions, pick): (&str, Vec<(&str, extras::Cmd)>, Pick) = match w.kind.as_str() {
            "3d" => ("Une scène réunit des modèles, des lumières et des caméras. Une scène neuve arrive avec un cube, une lumière et une caméra.", vec![("nouvelle scène 3D", extras::Cmd::NewScene), ("importer des fichiers…", extras::Cmd::Import)], |r| model3d::is_model(r)),
            "video" => ("Un montage assemble des vidéos, des images, des sons et des titres sur une timeline.", vec![("nouveau montage vidéo", extras::Cmd::NewMontage), ("importer des fichiers…", extras::Cmd::Import)], |r| montage::is_project(r) || video::is_video(r)),
            "mat" => ("Un matériau en nodes s'applique aux modèles et aux objets d'une scène ; un compositing retouche une image ou étalonne un clip.", vec![("nouveau matériau (nodes)", extras::Cmd::NewMaterial), ("nouveau compositing (nodes)", extras::Cmd::NewCompositor)], |r| nodes::is_graph(r)),
            "2d" => ("Images, croquis et planches.", vec![("nouveau board", extras::Cmd::NewBoard), ("importer des fichiers…", extras::Cmd::Import)], |r| images::is_image(r)),
            "code" => ("Crée un fichier en lui donnant son nom, ou rouvre un fichier récent.", vec![], |_| false),
            _ => ("Écris une note, ou rouvre un fichier récent.", vec![("nouvelle note", extras::Cmd::NewNote), ("note du jour", extras::Cmd::Daily)], |r| r.to_lowercase().ends_with(".md")),
        };
        let mut recent: Vec<(String, u64)> = self.entries.iter().filter(|e| !e.is_dir && if w.kind == "code" { CODE.contains(&ext(&e.rel).as_str()) && !nodes::is_graph(&e.rel) && formats::of(&e.rel).is_none() } else { pick(&e.rel) }).map(|e| (e.rel.clone(), e.modified)).collect();
        recent.sort_by(|a, b| b.1.cmp(&a.1));
        recent.truncate(10);
        let mut run: Option<extras::Cmd> = None;
        let mut open: Option<String> = None;
        let mut create: Option<String> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.16).max(24.0));
                ui.set_max_width(520.0);
                let accent = w.accent.map(|c| egui::Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(p.accent);
                ui.label(RichText::new(crate::i18n::t(&w.name)).size(30.0).family(theme::bold()).color(accent));
                ui.add_space(6.0);
                ui.label(RichText::new(crate::i18n::t(subtitle)).color(p.muted));
                ui.add_space(18.0);
                if w.kind == "code" {
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - 380.0).max(0.0) / 2.0);
                        let r = ui.add(egui::TextEdit::singleline(&mut self.landing_name).hint_text(crate::i18n::t("nom du fichier, par exemple main.py")).desired_width(260.0));
                        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if (ui.add(egui::Button::new(RichText::new(crate::i18n::t("créer")).color(p.on_accent)).fill(accent)).clicked() || enter) && !self.landing_name.trim().is_empty() {
                            create = Some(self.landing_name.trim().to_string());
                        }
                    });
                }
                ui.horizontal(|ui| {
                    let total: f32 = actions.len() as f32 * 190.0;
                    ui.add_space((ui.available_width() - total).max(0.0) / 2.0);
                    for (k, (label, cmd)) in actions.iter().enumerate() {
                        let text = RichText::new(crate::i18n::t(label)).color(if k == 0 { p.on_accent } else { p.text });
                        let b = if k == 0 { egui::Button::new(text).fill(accent) } else { egui::Button::new(text) };
                        if ui.add(b.min_size(egui::vec2(170.0, 30.0))).clicked() {
                            run = Some(*cmd);
                        }
                    }
                });
                ui.add_space(22.0);
                if !recent.is_empty() {
                    ui.label(caps(&crate::i18n::t("récents")));
                    ui.add_space(4.0);
                    for (rel, _) in &recent {
                        let name = rel.rsplit('/').next().unwrap_or(rel);
                        let dir = rel.rsplit_once('/').map(|x| x.0).unwrap_or("");
                        let mut job = egui::text::LayoutJob::default();
                        job.append(name, 0.0, egui::TextFormat { font_id: egui::FontId::proportional(13.0), color: p.text, ..Default::default() });
                        if !dir.is_empty() {
                            job.append(&format!("   {dir}"), 0.0, egui::TextFormat { font_id: egui::FontId::proportional(11.0), color: p.muted, ..Default::default() });
                        }
                        if ui.add(egui::Button::new(job).frame(false)).clicked() {
                            open = Some(rel.clone());
                        }
                    }
                }
                ui.add_space(18.0);
                ui.label(RichText::new(crate::i18n::t("^P ouvrir un fichier · glisse des fichiers ici pour les importer")).size(11.0).color(p.muted));
            });
        });
        if let Some(c) = run {
            self.run_cmd(c, &ctx);
        }
        if let Some(rel) = open {
            self.open(rel);
        }
        if let Some(name) = create {
            // Un nom sans extension devient un fichier texte ; un nom déjà pris est simplement rouvert
            let rel = if name.contains('.') { name } else { format!("{name}.txt") };
            if !self.entries.iter().any(|e| e.rel == rel) {
                let r = vault::safe(&self.root(), &rel).and_then(|path| {
                    if let Some(d) = path.parent() {
                        let _ = std::fs::create_dir_all(d);
                    }
                    std::fs::write(&path, "").map_err(|e| e.to_string())
                });
                if self.report(r).is_none() {
                    return;
                }
                self.refresh();
            }
            self.landing_name.clear();
            self.open(rel);
        }
    }

    fn ws_switch(&mut self, i: usize, ctx: &egui::Context) {
        if i >= self.ws.list.len() || i == self.ws.current {
            return;
        }
        self.ws_capture();
        self.ws.current = i;
        self.ws_apply(ctx, true);
        self.ws.save();
    }

    /// Au lancement : reprendre la disposition, la couleur et les onglets de l'espace en cours.
    fn ws_start(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !self.ws_started {
            // Au lancement : reprendre la disposition et la couleur, et les onglets si rien n'est ouvert
            self.ws_started = true;
            let tabs = self.docs.is_empty() && !self.ws.cur().tabs.is_empty();
            self.ws_apply(&ctx, tabs);
            // Fichiers aux anciennes extensions : on le signale une fois, la conversion reste un choix
            let old = self.entries.iter().filter(|e| !e.is_dir && formats::upgrade(&e.rel).is_some()).count();
            if old > 0 {
                self.toast = Some((format!("{old} {}", crate::i18n::t("fichier(s) avec une ancienne extension : palette de commandes → « passer aux nouvelles extensions »")), Instant::now()));
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            self.flush_all();
            self.ws_capture();
            self.ws.save();
            // Fichiers sans titre jamais modifiés : rien à garder. Les autres restent pour être récupérés.
            let root = self.root();
            for d in &self.docs {
                if untitled::is_untitled(&d.rel) && !self.untitled_changed(&d.rel) {
                    untitled::discard(&root, &d.rel);
                }
            }
        }
    }

    /// Les espaces de travail, dans la barre du haut : chacun a sa couleur, ses pages et ses genres de fichiers.
    fn ws_items(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let ctx = ui.ctx().clone();
        let mut go: Option<usize> = None;
        let mut changed = false;
        let mut add = false;
        let mut dup: Option<usize> = None;
        let mut del: Option<usize> = None;
        let mut reset: Option<usize> = None;
        ui.spacing_mut().item_spacing.x = 2.0;
        let n = self.ws.list.len();
        for i in 0..n {
            let on = i == self.ws.current;
            let color = self.ws.list[i].accent.map(|c| egui::Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(if on { p.accent } else { p.muted });
            let label = crate::i18n::t(&self.ws.list[i].name);
            let mut text = egui::text::LayoutJob::default();
            let font = egui::FontId::new(13.0, if on { theme::bold() } else { egui::FontFamily::Proportional });
            text.append("● ", 0.0, egui::TextFormat { font_id: font.clone(), color, ..Default::default() });
            text.append(&label, 0.0, egui::TextFormat { font_id: font, color: if on { color } else { p.muted }, ..Default::default() });
            let r = ui.add(egui::Button::new(text).frame(false)).on_hover_text(crate::i18n::t("clic droit : renommer, couleur, dupliquer"));
            if r.clicked() {
                go = Some(i);
            }
            r.context_menu(|ui| {
                ui.set_min_width(210.0);
                if self.ws_name.is_empty() || ui.memory(|m| m.data.get_temp::<usize>(egui::Id::unique("ws-edit"))) != Some(i) {
                    self.ws_name = self.ws.list[i].name.clone();
                    ui.memory_mut(|m| m.data.insert_temp(egui::Id::unique("ws-edit"), i));
                }
                if ui.add(egui::TextEdit::singleline(&mut self.ws_name).hint_text(crate::i18n::t("nom de l'espace"))).changed() && !self.ws_name.trim().is_empty() {
                    self.ws.list[i].name = self.ws_name.trim().to_string();
                    changed = true;
                }
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::t("couleur"));
                    let mut c = self.ws.list[i].accent.unwrap_or([p.accent.r(), p.accent.g(), p.accent.b()]);
                    if ui.color_edit_button_srgb(&mut c).changed() {
                        self.ws.list[i].accent = Some(c);
                        changed = true;
                    }
                    if self.ws.list[i].accent.is_some() && ui.small_button(crate::i18n::t("celle des réglages")).clicked() {
                        self.ws.list[i].accent = None;
                        changed = true;
                    }
                });
                ui.separator();
                if ui.button(crate::i18n::t("dupliquer")).clicked() {
                    dup = Some(i);
                    ui.close();
                }
                if ui.button(crate::i18n::t("réinitialiser la disposition")).clicked() {
                    reset = Some(i);
                    ui.close();
                }
                if n > 1 && ui.button(RichText::new(crate::i18n::t("supprimer")).color(p.missing)).clicked() {
                    del = Some(i);
                    ui.close();
                }
            });
            ui.add_space(10.0);
        }
        if ui.add(egui::Button::new(RichText::new("＋").color(p.muted)).frame(false)).on_hover_text(crate::i18n::t("Enregistrer la disposition actuelle comme nouvel espace de travail")).clicked() {
            add = true;
        }
        if add || dup.is_some() {
            self.ws_capture();
            let mut w = self.ws.list[dup.unwrap_or(self.ws.current)].clone();
            w.name = if add { format!("{} {}", crate::i18n::t("espace"), self.ws.list.len() + 1) } else { format!("{} 2", w.name) };
            self.ws.list.push(w);
            go = Some(self.ws.list.len() - 1);
        }
        if let Some(i) = reset {
            self.ws.list[i].places.clear();
            if i == self.ws.current {
                dock::reset();
            }
            changed = true;
        }
        if let Some(i) = del {
            if i == self.ws.current {
                // On quitte l'espace avant de le retirer
                let to = if i == 0 { 1 } else { i - 1 };
                self.ws.current = to;
                self.ws_apply(&ctx, true);
            }
            self.ws.list.remove(i);
            if self.ws.current > i {
                self.ws.current -= 1;
            }
            go = None;
            changed = true;
        }
        if changed {
            let w = self.ws.cur().accent;
            theme::set_workspace_accent(w);
            theme::apply(&ctx, self.prefs.dark);
            self.ws.save();
        }
        if let Some(i) = go {
            self.ws_switch(i, &ctx);
        }
    }

    /// Deuxième rangée : les pages de l'espace en cours (comme les jeux de menus de Maya, elles changent avec lui).
    fn ws_bar(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let kind = self.ws.cur().kind.clone();
        let pages = workspace::pages(&kind);
        let view_id = |v: View| match v {
            View::Files => "files",
            View::Brain => "brain",
            View::Refs => "refs",
            View::Boards => "boards",
            View::Tasks => "tasks",
            View::Chat => "chat",
            View::Search => "search",
        };
        // Une vue qui n'appartient pas à l'espace (raccourci comme Ctrl+G) : on passe à l'espace qui l'a,
        // sinon on revient à la première page
        let want = view_id(self.view());
        if !pages.iter().any(|(_, v)| *v == want) {
            let ctx = ui.ctx().clone();
            match self.ws.list.iter().position(|w| workspace::KINDS.contains(&w.kind.as_str()) && workspace::pages(&w.kind).iter().any(|(_, v)| *v == want)) {
                Some(i) => {
                    let v = self.view();
                    self.ws_switch(i, &ctx);
                    self.view = Some(v);
                }
                None => self.view = Some(view_of(pages[0].1)),
            }
        }
        let kind = self.ws.cur().kind.clone();
        let pages = workspace::pages(&kind);
        let accent = self.ws.cur().accent.map(|c| egui::Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(p.accent);
        egui::Panel::top("workspaces").exact_size(28.0).frame(egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(12, 0))).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                // Ouvrir / fermer l'explorateur, d'où qu'on soit
                let on = self.explorer_open;
                if ui.add(egui::Button::new(RichText::new("☰").size(13.0).color(if on { accent } else { p.muted })).frame(false)).on_hover_text(crate::i18n::t("fichiers (Ctrl+B)")).clicked() {
                    self.explorer_open = !on;
                }
                ui.add_space(8.0);
                for (label, v) in pages {
                    let view = view_of(v);
                    let on = view == self.view();
                    let text = RichText::new(crate::i18n::t(label)).size(12.0).color(if on { p.text } else { p.muted });
                    let r = ui.add(egui::Label::new(if on { text.family(theme::bold()) } else { text }).sense(egui::Sense::click()));
                    if on {
                        let y = ui.max_rect().bottom();
                        ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(r.rect.left(), y - 2.0), egui::pos2(r.rect.right(), y)), 0.0, accent);
                    }
                    if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.view = Some(view);
                    }
                    ui.add_space(12.0);
                }
            });
        });
    }

    /// Ouvre une page dans une nouvelle fenêtre flottante (cerveau, références, boards…).
    fn add_view_panel(&mut self, kind: &str) {
        let n = (1..).find(|n| !self.view_panels.contains(&format!("{kind}#{n}"))).unwrap_or(1);
        let id = format!("{kind}#{n}");
        if let Some(ctx) = &self.ctx {
            let c = ctx.content_rect().center();
            let off = 24.0 * (self.view_panels.len() % 6) as f32;
            dock::put(&format!("view:{id}"), dock::Side::Float, [c.x - 360.0 + off, c.y - 240.0 + off], [720.0, 480.0]);
        }
        self.view_panels.push(id);
    }

    /// Les fenêtres de pages ajoutées par le menu « fenêtre » : chacune est un panneau qu'on place où on veut.
    fn view_windows(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        for id in self.view_panels.clone() {
            let kind = id.split('#').next().unwrap_or("").to_string();
            let Some((label, _)) = VIEW_WINDOWS.iter().find(|(_, k)| *k == kind) else { continue };
            let n = id.split('#').nth(1).unwrap_or("1");
            let title = if n == "1" { crate::i18n::t(label) } else { format!("{} {n}", crate::i18n::t(label)) };
            let pid = format!("view:{id}");
            dock::show_ex(ui, &pid, &title, dock::Side::Float, 720.0, 260.0, egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)), true, |ui| {
                ui.push_id(&pid, |ui| match kind.as_str() {
                    "brain" => self.brain_view(ui),
                    "refs" => self.refs_view(ui),
                    "boards" => self.boards_view(ui),
                    "tasks" => self.tasks_view(ui),
                    "chat" => self.chat_view(ui),
                    _ => self.search_view(ui),
                });
            });
            if dock::closed(&pid) {
                self.view_panels.retain(|x| *x != id);
            }
        }
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
        let mut recent: Vec<String> = load_config("recent_vaults").map(|t| t.lines().map(String::from).collect()).unwrap_or_default();
        recent.retain(|r| *r != path && !r.trim().is_empty());
        recent.insert(0, path.clone());
        recent.truncate(8);
        save_config("recent_vaults", &recent.join("\n"));
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
        // Première note du coffre, si l'espace en cours est fait pour les notes
        if let Some(first) = self.entries.iter().find(|e| vault::is_md(&e.rel)).map(|e| e.rel.clone()) {
            if workspace::accepts(&self.ws.cur().kind, &first) {
                self.open_here(first);
            }
        }
        self.start_backup(false);
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
        if let Content::Nodes(ed) = &mut doc.content {
            ed.save_if_due(true);
        }
        if let Content::Paint(ed) = &mut doc.content {
            ed.save_if_due(true);
        }
        if doc.dirty_since.take().is_none() {
            return;
        }
        if let Content::Text(t) = &doc.content {
            let (rel, t) = (doc.rel.clone(), t.clone());
            let r = vault::write(&root, &rel, &t);
            live::bump();
            if r.is_ok() {
                extras::snapshot(&root, &rel, &t);
                // Avec un serveur LSP, c'est lui qui donne les erreurs
                let has_lsp = match self.docs.get_mut(i).and_then(|d| d.code.lsp.as_mut()) {
                    Some(l) => {
                        l.sync(&t);
                        l.saved();
                        true
                    }
                    None => false,
                };
                // Vérification de la syntaxe en arrière-plan
                let lang = ext(&rel);
                if !has_lsp && matches!(lang.as_str(), "py" | "js" | "mjs" | "cjs" | "json" | "sh" | "bash" | "lua") {
                    if let (Ok(path), Some(doc)) = (vault::safe(&root, &rel), self.docs.get_mut(i)) {
                        let (tx, rx) = std::sync::mpsc::channel();
                        let ctx = self.ctx.clone();
                        let t2 = t.clone();
                        std::thread::spawn(move || {
                            let _ = tx.send(code::check(&path, &lang, &t2));
                            if let Some(c) = ctx {
                                c.request_repaint();
                            }
                        });
                        doc.code.diag_rx = Some(rx);
                    }
                }
            }
            if self.report(r).is_some() && vault::is_md(&rel) {
                self.brain.dirty = true;
            }
        }
    }

    fn flush_all(&mut self) {
        for d in &mut self.docs {
            if let Content::Nodes(ed) = &mut d.content {
                ed.save_if_due(true);
            }
        }
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

    /// Ouvre un fichier dans l'espace où il a sa place : un montage dans l'espace vidéo, une scène dans l'espace 3D…
    /// On y passe tout seul, comme Maya change de menus avec le mode.
    fn open(&mut self, rel: String) {
        let here = self.ws.cur().kind.clone();
        let want = workspace::owner(&rel);
        if !workspace::accepts(&here, &rel) {
            if let (Some(i), Some(ctx)) = (self.ws.list.iter().position(|w| w.kind == want), self.ctx.clone()) {
                self.ws_routing = true;
                self.ws_switch(i, &ctx);
                self.ws_routing = false;
            }
        }
        self.open_here(rel);
    }

    fn open_here(&mut self, rel: String) {
        self.view = Some(View::Files);
        self.selected = Some(rel.clone());
        self.files.reveal(&rel);
        self.files.touch(&rel);
        if let Some(i) = self.docs.iter().position(|d| d.rel == rel) {
            self.active = Some(i);
        } else {
            if board::is_board(&rel) {
                self.open_board(rel);
                return;
            }
            let content = if model3d::is_model(&rel) {
                match (vault::safe(&self.root(), &rel), self.ctx.clone()) {
                    (Ok(path), Some(ctx)) => {
                        let mut v = model3d::Viewer::open(path, &ctx);
                        let root = self.root();
                        v.hdr_files = self
                            .entries
                            .iter()
                            .filter(|e| !e.is_dir && matches!(ext(&e.rel).as_str(), "hdr" | "exr"))
                            .filter_map(|e| vault::safe(&root, &e.rel).ok().map(|p| (e.name.clone(), p)))
                            .take(30)
                            .collect();
                        v.mat_files = self
                            .entries
                            .iter()
                            .filter(|e| !e.is_dir && formats::has(&e.rel, "mat"))
                            .filter_map(|e| vault::safe(&root, &e.rel).ok().map(|p| (formats::strip(&e.name).to_string(), p)))
                            .take(40)
                            .collect();
                        v.model_files = self
                            .entries
                            .iter()
                            .filter(|e| !e.is_dir && model3d::is_model(&e.rel) && !formats::has(&e.rel, "scene"))
                            .filter_map(|e| vault::safe(&root, &e.rel).ok().map(|p| (e.rel.clone(), p)))
                            .take(200)
                            .collect();
                        v.root = root.clone();
                        Content::Model(v)
                    }
                    (Err(e), _) => Content::Unreadable(e),
                    _ => Content::Unreadable("Contexte indisponible.".into()),
                }
            } else if paint::is_image_doc(&rel) {
                match vault::safe(&self.root(), &rel).and_then(paint::Editor::open) {
                    Ok(e) => Content::Paint(Box::new(e)),
                    Err(e) => Content::Unreadable(e),
                }
            } else if nodes::is_graph(&rel) {
                match vault::safe(&self.root(), &rel).and_then(|p| nodes::Editor::open(p, &self.root())) {
                    Ok(e) => Content::Nodes(Box::new(e)),
                    Err(e) => Content::Unreadable(e),
                }
            } else if montage::is_project(&rel) {
                if ffmpeg::find("ffmpeg").is_none() {
                    Content::NeedFfmpeg(None)
                } else {
                    match vault::safe(&self.root(), &rel) {
                        Ok(p) => Content::Montage(Box::new(montage::Editor::open(p, &self.root()))),
                        Err(e) => Content::Unreadable(e),
                    }
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
        // Ctrl+N : une note sans titre, qui prend son nom au premier Ctrl+S
        self.new_untitled("md", |p| std::fs::write(p, "").map_err(|e| e.to_string()));
    }

    #[allow(dead_code)]
    fn new_note_named(&mut self) {
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
                    // [[Note#Titre]] et [[Note^bloc]] : on ouvre la note
                    let name = name.split(['#', '^']).next().unwrap_or(&name).trim().to_string();
                    if name.is_empty() {
                        continue;
                    }
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
                    // Un fichier sans titre : rien n'est perdu sans qu'on l'ait choisi
                    if let Some(rel) = self.docs.get(i).map(|d| d.rel.clone()).filter(|r| untitled::is_untitled(r)) {
                        self.flush(i);
                        if self.untitled_changed(&rel) {
                            self.close_ask = Some(rel);
                            continue;
                        }
                        untitled::discard(&self.root(), &rel);
                        self.untitled_hash.remove(&rel);
                    }
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
        if !self.undoing {
            self.file_undo.push(vec![FileOp::Renamed(from.to_string(), to.to_string())]);
        }
        self.refresh();
    }

    /// Annule la dernière opération de l'explorateur : suppression, renommage ou déplacement.
    fn undo_file_op(&mut self) {
        let Some(batch) = self.file_undo.pop() else {
            self.toast = Some((crate::i18n::t("rien à annuler dans les fichiers"), Instant::now()));
            return;
        };
        let root = self.root();
        let (mut done, mut last) = (0, None);
        for op in batch.into_iter().rev() {
            match op {
                FileOp::Trashed(rel, bin) => {
                    if self.report(vault::restore(&root, &bin, &rel)).is_some() {
                        done += 1;
                        last = Some(rel);
                    }
                }
                FileOp::Renamed(from, to) => {
                    self.undoing = true;
                    self.do_rename(&to, &from);
                    self.undoing = false;
                    done += 1;
                    last = Some(from);
                }
            }
        }
        self.refresh();
        if let Some(rel) = last {
            self.selected = Some(rel.clone());
            self.files.reveal(&rel);
            self.toast = Some((format!("{} : {done}", crate::i18n::t("annulé")), Instant::now()));
        }
    }

    /// Ce que l'explorateur considère comme choisi : la sélection multiple, sinon l'élément courant.
    fn explorer_selection(&self) -> Vec<String> {
        if self.multi.len() > 1 { self.multi.clone() } else { self.selected.iter().cloned().collect() }
    }

    /// Met à la corbeille un élément, ou plusieurs séparés par des retours à la ligne. Annulable.
    fn do_trash(&mut self, rels: &str) {
        let mut batch = vec![];
        let mut removed: Vec<String> = vec![];
        // Un dossier choisi emporte déjà ce qu'il contient
        let all: Vec<&str> = rels.lines().filter(|r| !r.is_empty()).collect();
        for rel in all.iter().filter(|r| !all.iter().any(|d| *d != **r && r.starts_with(&format!("{d}/")))) {
            match vault::trash_to(&self.root(), rel) {
                Ok(bin) => {
                    removed.push(rel.to_string());
                    if let Some(b) = bin {
                        batch.push(FileOp::Trashed(rel.to_string(), b));
                    }
                }
                Err(e) => {
                    self.report::<()>(Err(e));
                }
            }
        }
        if !batch.is_empty() {
            self.file_undo.push(batch);
        }
        if removed.is_empty() {
            return;
        }
        self.multi.clear();
        let gone = |d: &Doc| removed.iter().any(|rel| &d.rel == rel || d.rel.starts_with(&format!("{rel}/")));
        self.docs.retain(|d| !gone(d));
        self.active = if self.docs.is_empty() { None } else { Some(self.active.unwrap_or(0).min(self.docs.len() - 1)) };
        self.selected = None;
        self.refresh();
    }

    // ---------- clavier ----------

    /// Les raccourcis disponibles là où se trouve la souris, ou là où l'on travaille, pour la barre du bas
    /// (comme la barre d'état de Blender) : dialogues, zones de l'interface, puis la page et le fichier ouverts.
    fn key_hints(&self) -> String {
        let t = crate::i18n::t;
        if self.palette.is_some() {
            return t("↑ ↓ choisir · Entrée lancer · Échap fermer");
        }
        if self.switcher.is_some() {
            return t("tape un nom · ↑ ↓ choisir · Entrée ouvrir · Échap fermer");
        }
        if self.new_file.is_some() {
            return t("clic : genre de fichier · tape le nom · Entrée créer · Échap annuler");
        }
        if self.dialog.is_some() || self.history_dlg.is_some() {
            return t("Échap fermer");
        }
        let at = self.ctx.as_ref().and_then(|c| c.input(|i| i.pointer.hover_pos()));
        let zone = at.and_then(|p| self.zones_last.iter().rev().find(|(_, r)| r.contains(p)).map(|z| z.0));
        let pages = workspace::pages(&self.ws.cur().kind).len();
        match zone {
            Some("top") => return t("clic : changer d'espace · clic droit sur un espace : nom, couleur, dupliquer · ＋ nouvel espace · Ctrl+PgPréc / PgSuiv : espace voisin · Ctrl+Maj+P commandes"),
            Some("pages") => return format!("{} · Ctrl+1…{pages}", t("clic : changer de page")),
            Some("tabs") => return t("clic : afficher · clic du milieu : fermer · glisser hors de la barre : panneau à part · Ctrl+Tab onglet suivant · Ctrl+W fermer"),
            Some("explorer") => {}
            Some("assistant") => return t("Entrée envoyer · Maj+Entrée nouvelle ligne · Ctrl+L fermer l'assistant"),
            Some("side") => return t("clic : aller au titre, ou ouvrir la note liée"),
            Some("status") => return t("clic sur la version : mises à jour · minuteur : clic pour lancer"),
            _ => {}
        }
        if self.explorer_focus || zone == Some("explorer") {
            return t("double-clic ou Entrée : ouvrir · F2 renommer · Suppr supprimer · Ctrl+clic, Maj+clic : plusieurs · Ctrl+A tout · Ctrl+Z annuler · glisser : déplacer · clic droit : menu");
        }
        match self.view() {
            View::Brain => return t("molette : zoom · glisser : déplacer · clic : ouvrir · Ctrl+G revenir aux fichiers"),
            View::Refs => return t("Ctrl+V coller une image ou un lien · double-clic : ouvrir · glisser vers un board · molette : défiler"),
            View::Boards => return t("V sélection · T texte · B pinceau · E gomme · L ligne · R rectangle · O ellipse · Maj+A flèche · Ctrl+C / X / V · Ctrl+D dupliquer · Suppr · Ctrl+Z · Espace+glisser : déplacer la vue · Alt+glisser : tourner un modèle"),
            View::Tasks => return t("Entrée ajouter une tâche · clic sur la case : faite · clic sur le texte : ouvrir la note"),
            View::Chat => return t("Entrée envoyer · Maj+Entrée nouvelle ligne · @ citer un fichier · Ctrl+L assistant à côté"),
            View::Search => return t("tape au moins 2 caractères · Entrée ouvrir le premier · clic : ouvrir à la ligne"),
            View::Files => {}
        }
        match self.current().map(|d| &d.content) {
            Some(Content::Model(v)) if v.is_mesh_edit() => t("MODE ÉDITION · 1 2 3 : sommets, arêtes, faces · glisser : rectangle (Maj ajoute, Ctrl retire) · Ctrl+clic : chemin · Alt+clic : boucle · Ctrl+plus / moins · Ctrl+I inverser · G R S · E extruder · I incruster · M fusionner · L pièce liée · X supprimer · Ctrl+Z"),
            Some(Content::Model(v)) if v.is_scene() => t("molette enfoncée : tourner (Maj : déplacer) · glisser : rectangle · G R S : déplacer, tourner, taille (X Y Z : axe) · Maj+clic, A, B : sélection · Maj+A ajouter · X supprimer · Maj+D dupliquer · P parent · H cacher · I clé · Tab édition · Espace lecture · Ctrl+Z annuler"),
            Some(Content::Model(_)) => t("glisser : tourner · Maj+glisser : déplacer · molette : zoom · 1 3 7 : vues · 5 : perspective · L + glisser : lumière · H + glisser : HDRI · F2 renommer le fichier"),
            Some(Content::Montage(_)) => t("Espace / J K L lecture · V C B N R Y H Z T outils · I O entrée sortie · , insérer · . recouvrir · Ctrl+K couper · Maj+Suppr supprimer et refermer · Q W propager · M repère · S magnétisme · Alt+molette : zoom"),
            Some(Content::Nodes(_)) => t("clic droit : ajouter · Q aligner · Suppr supprimer · F2 renommer la node · molette : zoom · Ctrl+Z annuler"),
            Some(Content::Paint(_)) => t("B pinceau · E gomme · G pot · I pipette · M sélection · V déplacer · [ ] taille · Ctrl+A / Ctrl+D sélection · Ctrl+J dupliquer · Ctrl+E fusionner · Ctrl+Z annuler · Espace+glisser : déplacer la vue"),
            Some(Content::Video(_)) => t("Espace lire · ← → image par image · Maj+← → seconde · I O : entrée, sortie · B pinceau · E gomme"),
            Some(Content::Image) => t("molette : zoom · glisser : déplacer · double-clic : 100 % / ajuster · F2 renommer le fichier"),
            Some(Content::Office(_)) => t("molette : défiler · Ctrl+molette : zoom · F2 renommer le fichier"),
            Some(Content::Text(_)) if self.current().is_some_and(|d| vault::is_md(&d.rel)) => t("Ctrl+E lire / éditer · [[ lien vers une note · Ctrl+F chercher · Ctrl+Z annuler · Ctrl+S enregistrer · F2 renommer"),
            Some(Content::Text(_)) => t("Ctrl+F chercher · Ctrl+G aller à la ligne · Ctrl+Maj+O symboles · F12 définition · Alt+Maj+F formater · F5 exécuter · Ctrl+` terminal"),
            Some(_) => t("F2 renommer le fichier · Ctrl+P ouvrir · Ctrl+Maj+P commandes"),
            None => t("Ctrl+N nouvelle note · Ctrl+P ouvrir un fichier · glisse des fichiers pour les importer · Ctrl+Maj+P commandes"),
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.vault.is_none() || self.dialog.is_some() {
            return;
        }
        let pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        // Qui a reçu le dernier clic : l'explorateur ou le reste ?
        if let (true, Some(at)) = ctx.input(|i| (i.pointer.any_pressed(), i.pointer.interact_pos())) {
            self.explorer_focus = self.explorer_rect.is_some_and(|r| r.contains(at));
        }
        // F2 renomme partout : l'élément choisi dans l'explorateur, sinon le fichier ouvert
        // (l'éditeur de nodes garde F2 pour renommer une node)
        let typing = ctx.egui_wants_keyboard_input();
        let in_nodes = !self.explorer_focus && matches!(self.current().map(|d| &d.content), Some(Content::Nodes(_)));
        if !in_nodes && pressed(Modifiers::NONE, Key::F2) {
            let target = if self.explorer_focus { self.selected.clone() } else { self.current().map(|d| d.rel.clone()).or(self.selected.clone()) };
            if let Some(rel) = target {
                self.actions.push(Action::Rename(rel));
            }
        }
        if self.explorer_focus && !typing {
            let sel = self.explorer_selection();
            if !sel.is_empty() && (pressed(Modifiers::NONE, Key::Delete) || pressed(Modifiers::NONE, Key::Backspace)) {
                self.actions.push(Action::Trash(sel.join("\n")));
            }
            if pressed(Modifiers::COMMAND, Key::Z) {
                self.undo_file_op();
            }
            if pressed(Modifiers::COMMAND, Key::A) {
                self.multi = self.explorer_prev.clone();
            }
            if pressed(Modifiers::NONE, Key::Escape) {
                self.multi.clear();
            }
        }
        if pressed(Modifiers::COMMAND | Modifiers::SHIFT, Key::P) {
            self.palette = Some((String::new(), 0));
        }
        if pressed(Modifiers::COMMAND, Key::P) {
            self.switcher = Some((String::new(), 0));
        }
        if pressed(Modifiers::COMMAND, Key::J) {
            self.daily_note();
        }
        if pressed(Modifiers::COMMAND, Key::Backslash) {
            self.toggle_split();
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
        if pressed(Modifiers::COMMAND, Key::B) {
            self.explorer_open = !self.explorer_open;
        }
        if pressed(Modifiers::COMMAND | Modifiers::SHIFT, Key::M) {
            self.compact = !self.compact;
        }
        let text_open = self.view() == View::Files && self.current().is_some_and(|d| matches!(d.content, Content::Text(_)));
        if text_open && pressed(Modifiers::COMMAND | Modifiers::SHIFT, Key::O) {
            if let Some(i) = self.active {
                self.docs[i].code.goto = Some("@".into());
            }
        }
        if text_open && pressed(Modifiers::ALT | Modifiers::SHIFT, Key::F) {
            self.format_current();
        }
        if pressed(Modifiers::COMMAND, Key::Backtick) {
            self.toggle_terminal();
        }
        if text_open && pressed(Modifiers::COMMAND, Key::G) {
            if let Some(i) = self.active {
                self.docs[i].code.goto = Some(String::new());
            }
        } else if pressed(Modifiers::COMMAND, Key::G) {
            self.view = Some(if self.view() == View::Brain { View::Files } else { View::Brain });
        }
        // Ctrl+1, 2, 3… : les pages de l'espace en cours, dans l'ordre de la deuxième rangée
        let pages = workspace::pages(&self.ws.cur().kind);
        for (n, k) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6].into_iter().enumerate() {
            if pressed(Modifiers::COMMAND, k) {
                if let Some((_, v)) = pages.get(n) {
                    self.view = Some(view_of(v));
                }
            }
        }
        // Ctrl+Page précédente / suivante : espace voisin (comme dans Blender)
        let n_ws = self.ws.list.len();
        if n_ws > 1 {
            if pressed(Modifiers::COMMAND, Key::PageDown) {
                let i = (self.ws.current + 1) % n_ws;
                self.ws_switch(i, ctx);
            } else if pressed(Modifiers::COMMAND, Key::PageUp) {
                let i = (self.ws.current + n_ws - 1) % n_ws;
                self.ws_switch(i, ctx);
            }
        }
        // Ctrl+Tab / Ctrl+Maj+Tab : onglet suivant / précédent
        if !self.docs.is_empty() {
            let n = self.docs.len();
            if pressed(Modifiers::COMMAND | Modifiers::SHIFT, Key::Tab) {
                self.active = Some((self.active.unwrap_or(0) + n - 1) % n);
            } else if pressed(Modifiers::COMMAND, Key::Tab) {
                self.active = Some((self.active.unwrap_or(0) + 1) % n);
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
            match self.active.filter(|&i| self.docs.get(i).is_some_and(|d| untitled::is_untitled(&d.rel))) {
                Some(i) => self.save_as(i),
                None => {
                    self.flush_all();
                    self.update_backlinks();
                }
            }
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
        let ctx = ui.ctx().clone();
        let mut cmd: Option<extras::Cmd> = None;
        let mut open_vault: Option<String> = None;
        egui::Panel::top("top")
            .exact_size(38.0)
            .frame(egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(12, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    theme::mark(ui, 16.0);
                    ui.label(RichText::new(crate::i18n::t("aryadne")).family(theme::bold()));
                    // Menu du coffre : tout ce qui concerne les fichiers et le dossier
                    let path = self.root();
                    let short = path.split(['/', '\\']).filter(|s| !s.is_empty()).last().unwrap_or("").to_string();
                    let (r, _) = egui::containers::menu::MenuButton::from_button(egui::Button::new(RichText::new(format!("~/{short} ▾")).color(p.muted)).frame(false)).ui(ui, |ui| {
                        ui.set_min_width(240.0);
                        ui.label(RichText::new(&path).color(p.muted).size(10.5));
                        ui.separator();
                        for (label, c) in [
                            ("nouvelle note   Ctrl+N", extras::Cmd::NewNote),
                            ("note du jour   Ctrl+J", extras::Cmd::Daily),
                            ("depuis un modèle…", extras::Cmd::FromTemplate),
                            ("scène 3D", extras::Cmd::NewScene),
                            ("montage vidéo", extras::Cmd::NewMontage),
                            ("matériau en nodes", extras::Cmd::NewMaterial),
                            ("compositing en nodes", extras::Cmd::NewCompositor),
                        ] {
                            if ui.button(crate::i18n::t(label)).clicked() {
                                cmd = Some(c);
                            }
                        }
                        ui.separator();
                        for (label, c) in [
                            ("importer des fichiers…", extras::Cmd::Import),
                            ("lier un dossier…", extras::Cmd::LinkFolder),
                            ("image depuis une URL…", extras::Cmd::ImportUrl),
                            ("transfert entre ordinateurs…", extras::Cmd::Transfer),
                        ] {
                            if ui.button(crate::i18n::t(label)).clicked() {
                                cmd = Some(c);
                            }
                        }
                        ui.separator();
                        let recent: Vec<String> = load_config("recent_vaults").map(|t| t.lines().map(String::from).filter(|r| *r != path).collect()).unwrap_or_default();
                        if !recent.is_empty() {
                            ui.menu_button(crate::i18n::t("coffres récents"), |ui| {
                                for r in recent {
                                    if ui.button(&r).clicked() {
                                        open_vault = Some(r);
                                    }
                                }
                            });
                        }
                        for (label, c) in [
                            ("statistiques du coffre", extras::Cmd::Stats),
                            ("sauvegarder maintenant (zip)", extras::Cmd::Backup),
                            ("synchroniser avec le cloud", extras::Cmd::Sync),
                            ("ouvrir le dossier", extras::Cmd::OpenVaultFolder),
                            ("changer de coffre…", extras::Cmd::ChangeVault),
                        ] {
                            if ui.button(crate::i18n::t(label)).clicked() {
                                cmd = Some(c);
                            }
                        }
                    });
                    r.on_hover_text(format!("{path}"));
                    ui.add_space(18.0);
                    self.ws_items(ui);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(crate::i18n::t("réglages")).clicked() {
                            cmd = Some(extras::Cmd::Settings(settings::Tab::Appearance));
                        }
                        // Fenêtre : thème, épingler, compact
                        let (r, _) = egui::containers::menu::MenuButton::new(crate::i18n::t("fenêtre ▾")).ui(ui, |ui| {
                            ui.set_min_width(260.0);
                            // Panneaux : on les ouvre et ferme d'ici, puis on les place par leur en-tête
                            ui.label(caps(&crate::i18n::t("panneaux")));
                            ui.checkbox(&mut self.explorer_open, format!("{}   Ctrl+B", crate::i18n::t("fichiers")));
                            let mut side = !self.side_hidden;
                            if ui.checkbox(&mut side, crate::i18n::t("structure et liens")).changed() {
                                self.side_hidden = !side;
                            }
                            ui.checkbox(&mut self.assistant_open, format!("{}   Ctrl+L", crate::i18n::t("assistant")));
                            let mut term = self.terminal.is_some();
                            if ui.checkbox(&mut term, format!("{}   Ctrl+`", crate::i18n::t("terminal"))).changed() {
                                self.toggle_terminal();
                            }
                            ui.separator();
                            // Nouvelles fenêtres : autant qu'on veut, chacune se place par son en-tête
                            ui.label(caps(&crate::i18n::t("ajouter une fenêtre")));
                            for (label, kind) in VIEW_WINDOWS {
                                if ui.button(format!("＋ {}", crate::i18n::t(label))).clicked() {
                                    self.add_view_panel(kind);
                                    ui.close();
                                }
                            }
                            if let Some(rel) = self.current().map(|d| d.rel.clone()) {
                                if ui.button(format!("＋ {}", crate::i18n::t("le fichier ouvert, à part"))).clicked() {
                                    let at = ctx.content_rect().center() - egui::vec2(320.0, 230.0);
                                    self.tear_tab(rel, dock::Side::Float, at);
                                    ui.close();
                                }
                            }
                            if ui.button(crate::i18n::t("réinitialiser la disposition")).clicked() {
                                dock::reset();
                                self.view_panels.clear();
                                self.explorer_open = true;
                                self.side_hidden = false;
                                ui.close();
                            }
                            ui.separator();
                            if ui.button(if theme::is_dark() { "thème clair" } else { "thème sombre" }).clicked() {
                                cmd = Some(extras::Cmd::Theme);
                            }
                            let mut pinned = self.pinned;
                            if ui.checkbox(&mut pinned, crate::i18n::t("épingler au-dessus")).on_hover_text(crate::i18n::t("Garder Aryadne au-dessus de Blender et des autres fenêtres")).changed() {
                                cmd = Some(extras::Cmd::Pin);
                            }
                            if ui.button(crate::i18n::t("mode compact   Ctrl+Maj+M")).clicked() {
                                cmd = Some(extras::Cmd::Compact);
                            }
                            ui.separator();
                            if ui.button(if cfg!(windows) { "ajouter au menu Démarrer" } else { "ajouter au menu des applications" }).clicked() {
                                cmd = Some(extras::Cmd::MenuEntry);
                            }
                        });
                        if self.pinned {
                            r.on_hover_text(crate::i18n::t("épinglé"));
                        }
                        let ai_btn = egui::Button::new(if self.assistant_open { RichText::new(crate::i18n::t("assistant")).color(p.on_accent) } else { RichText::new(crate::i18n::t("assistant")) }).fill(if self.assistant_open { p.accent } else { p.bg });
                        if ui.add(ai_btn).on_hover_text(crate::i18n::t("Assistant IA à côté de tout (Ctrl+L)")).clicked() {
                            cmd = Some(extras::Cmd::Assistant);
                        }
                        let pal = egui::Button::new(RichText::new(crate::i18n::t("⌘ commandes")).color(p.muted)).fill(p.panel);
                        if ui.add(pal).on_hover_text(crate::i18n::t("Toutes les actions (Ctrl+Maj+P)")).clicked() {
                            self.palette = Some((String::new(), 0));
                        }
                        if let Some(r) = self.updater.update_available() {
                            let b = egui::Button::new(RichText::new(format!("↑ {}", r.version)).color(p.on_accent).family(theme::bold())).fill(p.accent).stroke(egui::Stroke::NONE);
                            if ui.add(b).on_hover_text(crate::i18n::t("Nouvelle version disponible")).clicked() {
                                self.dialog = Some(Dialog::Update);
                            }
                        }
                    });
                });
            });
        if let Some(c) = cmd {
            self.run_cmd(c, &ctx);
        }
        if let Some(v) = open_vault {
            self.load_vault(v);
        }
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
                        View::Tasks => ("TÂCHES", p.text),
                        View::Search => ("RECHERCHE", p.text),
                        View::Files if preview => ("LECTURE", p.link),
                        View::Files => ("ÉDITION", p.accent),
                    };
                    egui::Frame::new().fill(color).inner_margin(egui::Margin::symmetric(10, 4)).show(ui, |ui| {
                        ui.label(RichText::new(crate::i18n::t(mode)).color(p.on_accent).family(theme::bold()).size(11.0));
                    });
                    if let Some(d) = self.current() {
                        ui.label(RichText::new(vault::display_rel(&d.rel)).size(11.5));
                        if let Content::Text(t) = &d.content {
                            let lang = if vault::is_md(&d.rel) { "markdown".to_string() } else if ext(&d.rel).is_empty() { "texte".into() } else { ext(&d.rel) };
                            ui.label(RichText::new(lang).color(p.muted).size(11.5));
                            ui.label(RichText::new(format!("{} {} · {} {}", t.lines().count(), crate::i18n::t("lignes"), t.split_whitespace().count(), crate::i18n::t("mots"))).color(p.muted).size(11.5));
                            ui.label(RichText::new(crate::i18n::t(if d.dirty_since.is_some() { "● modifié" } else { "sauvé" })).color(p.muted).size(11.5));
                            match &d.code.lsp {
                                Some(l) => {
                                    let dead = l.server.dead.lock().ok().and_then(|x| x.clone());
                                    let (t, c) = if dead.is_some() { (format!("lsp : {} arrêté", l.server.name), p.missing) } else { (format!("lsp : {}", l.server.name), p.muted) };
                                    ui.label(RichText::new(t).color(c).size(11.5)).on_hover_text(crate::i18n::t("Complétion intelligente, erreurs en direct, survol, F12 : définition"));
                                }
                                None => {
                                    if let Some(sv) = lsp::suggested(&ext(&d.rel)) {
                                        ui.label(RichText::new(crate::i18n::t("sans lsp")).color(p.muted.gamma_multiply(0.7)).size(11.5)).on_hover_text(format!("Installe « {sv} » pour la complétion intelligente, les erreurs en direct et F12 (définition). Aryadne le détecte tout seul."));
                                    }
                                }
                            }
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        if ui.add(egui::Button::new(RichText::new(format!("v{}", updater::CURRENT)).color(p.muted).size(11.0)).frame(false)).on_hover_text(crate::i18n::t("Mises à jour")).clicked() {
                            self.dialog = Some(Dialog::Update);
                        }
                        self.timer_widget(ui);
                        self.cloud_widget(ui);
                        if self.backup_rx.is_some() {
                            ui.add(egui::Spinner::new().size(10.0));
                            ui.label(RichText::new(crate::i18n::t("sauvegarde…")).color(p.muted).size(11.0));
                        }
                        // Les touches utiles là où on se trouve, comme la barre d'état de Blender :
                        // elles prennent la place qui reste et se coupent proprement au besoin (survol : tout le texte)
                        let hints = self.key_hints();
                        ui.add(egui::Label::new(RichText::new(&hints).color(p.muted).size(11.0)).truncate()).on_hover_text(hints.replace(" · ", "\n"));
                    });
                });
            });
    }

    // ---------- explorateur ----------

    // ---------- éditeur ----------

    fn tabs(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        if self.in_split {
            // Volet de droite : juste le nom, échanger, fermer
            let name = self.current().map(|d| vault::display_rel(&d.rel)).unwrap_or_default();
            egui::Frame::new().fill(p.panel).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("◧ {name}")).family(theme::bold()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(n) = self.pane_now {
                            // Panneau à part : le fermer ramène simplement le fichier parmi les onglets
                            if ui.small_button("×").on_hover_text(crate::i18n::t("remettre parmi les onglets")).clicked() && n < self.panes.len() {
                                self.panes.remove(n);
                            }
                            return;
                        }
                        if ui.small_button("×").on_hover_text(crate::i18n::t("fermer la vue côte à côte")).clicked() {
                            self.split = None;
                        }
                        if ui.small_button("⇄").on_hover_text(crate::i18n::t("échanger les deux côtés")).clicked() {
                            self.split_swap = true;
                        }
                    });
                });
            });
            return;
        }
        let mut tear: Option<(String, dock::Side, egui::Pos2)> = None;
        let mut untear: Option<String> = None;
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
                                let out = if self.panes.contains(&d.rel) { "◧ " } else { "" };
                                ui.label(RichText::new(format!("{out}{name}{mark}")).color(if active { p.text } else { p.muted }).size(12.0));
                                let x = ui.add(egui::Label::new(RichText::new("×").color(p.muted)).sense(egui::Sense::click()));
                                if x.on_hover_text(crate::i18n::t("Fermer (Ctrl+W)")).clicked() {
                                    self.actions.push(Action::Close(i));
                                }
                            });
                        });
                        if active {
                            let top = r.response.rect;
                            ui.painter().rect_filled(egui::Rect::from_min_size(top.min, egui::vec2(top.width(), 2.0)), 0.0, p.accent);
                        }
                        let resp = r.response.interact(egui::Sense::click_and_drag());
                        if resp.clicked() {
                            self.active = Some(i);
                            self.selected = Some(d.rel.clone());
                            self.actions.push(Action::Open(d.rel.clone()));
                        }
                        // Tirer un onglet hors de la barre : il devient un panneau à part (hors de l'app : une vraie fenêtre)
                        if resp.dragged() {
                            if let Some(at) = ui.ctx().pointer_latest_pos() {
                                if (at.y - resp.rect.center().y).abs() > 36.0 || dock::outside(ui.ctx(), at) {
                                    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("tab-drag")));
                                    let ghost = egui::Rect::from_min_size(at + egui::vec2(8.0, 8.0), egui::vec2(220.0, 30.0));
                                    painter.rect_filled(ghost, 6.0, p.accent.linear_multiply(0.2));
                                    painter.rect_stroke(ghost, 6.0, egui::Stroke::new(2.0, p.accent), egui::StrokeKind::Inside);
                                    painter.text(ghost.center(), egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(12.0), p.accent);
                                }
                            }
                        }
                        if resp.drag_stopped() {
                            if let Some(at) = ui.ctx().pointer_latest_pos() {
                                let away = dock::outside(ui.ctx(), at);
                                if away || (at.y - resp.rect.center().y).abs() > 36.0 {
                                    tear = Some((d.rel.clone(), if away { dock::Side::Window } else { dock::Side::Float }, at));
                                }
                            }
                        }
                        resp.context_menu(|ui| {
                            if self.panes.contains(&d.rel) {
                                if ui.button(crate::i18n::t("remettre parmi les onglets")).clicked() {
                                    untear = Some(d.rel.clone());
                                    ui.close();
                                }
                            } else {
                                for (label, side) in [("ouvrir dans un panneau flottant", dock::Side::Float), ("ouvrir dans un panneau à droite", dock::Side::Right), ("ouvrir dans un panneau en bas", dock::Side::Bottom), ("ouvrir dans sa propre fenêtre", dock::Side::Window)] {
                                    if ui.button(crate::i18n::t(label)).clicked() {
                                        tear = Some((d.rel.clone(), side, egui::pos2(260.0, 160.0)));
                                        ui.close();
                                    }
                                }
                            }
                            ui.separator();
                            if ui.button(crate::i18n::t("fermer")).clicked() {
                                self.actions.push(Action::Close(i));
                                ui.close();
                            }
                        });
                        if resp.middle_clicked() {
                            self.actions.push(Action::Close(i));
                        }
                    }
                });
            });
        });
        if let Some(rel) = untear {
            self.panes.retain(|r| *r != rel);
        }
        if let Some((rel, side, at)) = tear {
            self.tear_tab(rel, side, at);
        }
    }

    /// Sort un onglet dans son propre panneau.
    fn tear_tab(&mut self, rel: String, side: dock::Side, at: egui::Pos2) {
        if self.docs.len() < 2 {
            self.toast = Some((crate::i18n::t("ouvre un deuxième fichier pour sortir celui-ci dans un panneau"), Instant::now()));
            return;
        }
        let Some(j) = self.docs.iter().position(|d| d.rel == rel) else { return };
        if self.split.as_deref() == Some(rel.as_str()) {
            self.split = None;
        }
        if !self.panes.contains(&rel) {
            self.panes.push(rel.clone());
        }
        dock::put(&format!("doc:{rel}"), side, [at.x - 20.0, at.y - 8.0], [640.0, 460.0]);
        // La zone centrale montre un autre onglet, pour ne pas afficher deux fois le même fichier
        if self.active == Some(j) {
            self.active = (0..self.docs.len()).rev().find(|&k| k != j && !self.panes.contains(&self.docs[k].rel)).or(self.active);
        }
    }

    /// Les onglets sortis : chacun dans un panneau qu'on déplace, ancre ou détache comme les autres.
    fn doc_panes(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        self.panes.retain(|r| self.docs.iter().any(|d| &d.rel == r));
        for n in 0..self.panes.len() {
            let Some(rel) = self.panes.get(n).cloned() else { break };
            let Some(j) = self.docs.iter().position(|d| d.rel == rel) else { continue };
            // Le même fichier ne s'affiche qu'une fois par image
            if Some(j) == self.active || self.split.as_deref() == Some(rel.as_str()) {
                continue;
            }
            let title = rel.rsplit('/').next().unwrap_or(&rel).to_string();
            let pid = format!("doc:{rel}");
            if dock::closed(&pid) {
                // Fermer le panneau ramène le fichier parmi les onglets
                self.panes.retain(|r| *r != rel);
                continue;
            }
            dock::show_ex(ui, &pid, &title, dock::Side::Float, 520.0, 260.0, egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)), true, |ui| {
                let keep = self.active;
                self.active = Some(j);
                self.in_split = true;
                self.pane_now = Some(n);
                let mut rn = false;
                self.editor_inner(ui, &mut rn);
                self.pane_now = None;
                self.in_split = false;
                self.active = keep;
            });
        }
    }

    /// Ouvre ou ferme la vue côte à côte : le fichier actif passe à droite,
    /// et l'onglet précédent revient à gauche.
    pub(crate) fn toggle_split(&mut self) {
        if self.split.take().is_some() {
            return;
        }
        let Some(i) = self.active else { return };
        if self.docs.len() < 2 {
            self.toast = Some(("ouvre un deuxième fichier pour la vue côte à côte".into(), Instant::now()));
            return;
        }
        self.split = Some(self.docs[i].rel.clone());
        self.active = Some(if i == 0 { 1 } else { i - 1 });
    }

    fn split_pane(&mut self, ui: &mut egui::Ui) {
        let Some(sp) = self.split.clone() else { return };
        let Some(j) = self.docs.iter().position(|d| d.rel == sp) else {
            self.split = None;
            return;
        };
        if Some(j) == self.active {
            // Même fichier des deux côtés : on montre l'onglet voisin à gauche
            if self.docs.len() < 2 {
                self.split = None;
                return;
            }
            self.active = Some(if j == 0 { 1 } else { j - 1 });
        }
        let p = theme::pal();
        let total = ui.available_width();
        let w = total / 2.0;
        egui::Panel::right("split-pane").resizable(true).default_size(w).min_size(240.0).max_size((total - 260.0).max(260.0)).frame(egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line))).show(ui, |ui| {
            let keep = self.active;
            self.active = Some(j);
            self.in_split = true;
            let mut rn = false;
            self.editor_inner(ui, &mut rn);
            self.in_split = false;
            self.active = keep;
        });
        if std::mem::take(&mut self.split_swap) {
            if let Some(a) = self.active {
                self.split = Some(self.docs[a].rel.clone());
                self.active = Some(j);
            }
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        self.doc_panes(ui);
        self.split_pane(ui);
        let mut run_now = ui.input(|i| i.key_pressed(Key::F5)) && self.current().is_some_and(|d| extras::runner_for(&d.rel).is_some());
        self.run_panel(ui);
        self.editor_inner(ui, &mut run_now);
        if run_now {
            self.run_current();
        }
        if std::mem::take(&mut self.want_blender) {
            match extras::find_blender() {
                Some(b) => self.run_with(b.display().to_string(), vec!["--background".into(), "--python".into()]),
                None => self.error = Some("Blender introuvable. Ajoute-le au PATH, ou installe-le à l'emplacement habituel.".into()),
            }
        }
        if std::mem::take(&mut self.want_format) {
            self.format_current();
        }
        if std::mem::take(&mut self.want_term) {
            self.toggle_terminal();
        }
    }

    fn editor_inner(&mut self, ui: &mut egui::Ui, run_now: &mut bool) {
        let p = theme::pal();
        let mut lsp_goto: Option<(PathBuf, usize)> = None;
        let mut open_material: Option<PathBuf> = None;
        self.feed_project_words();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| {
            if self.docs.is_empty() {
                self.landing(ui);
                return;
            }
            let y0 = ui.cursor().min.y;
            self.tabs(ui);
            let tab_rect = egui::Rect::from_min_max(egui::pos2(ui.max_rect().left(), y0), egui::pos2(ui.max_rect().right(), ui.cursor().min.y));
            self.zones_next.push(("tabs", tab_rect));
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
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Fichier sans titre : le bouton pour lui donner un nom et une place
                        if untitled::is_untitled(&rel) && ui.add(egui::Button::new(RichText::new(crate::i18n::t("enregistrer…  Ctrl+S")).color(p.on_accent)).fill(p.accent)).clicked() {
                            self.want_save_as = true;
                        }
                        // « ouvrir ▾ » : application par défaut, au choix, ou dans le dossier
                        let ext_label = if rel.to_lowercase().ends_with(".blend") { "dans Blender" } else { "avec l'app par défaut" };
                        let mut which = 0;
                        egui::containers::menu::MenuButton::new(crate::i18n::t("ouvrir ▾")).ui(ui, |ui| {
                            if ui.button(ext_label).clicked() {
                                which = 1;
                            }
                            if ui.button(crate::i18n::t("avec…")).on_hover_text(crate::i18n::t("Choisir l'application")).clicked() {
                                which = 2;
                            }
                            if ui.button(crate::i18n::t("afficher dans le dossier")).clicked() {
                                which = 3;
                            }
                        });
                        let path = vault::safe(&root, &rel);
                        let r = match (which, &path) {
                            (1, Ok(p)) => settings::open_external(p),
                            (2, Ok(p)) => settings::open_with(p),
                            (3, Ok(p)) => {
                                settings::reveal(p, true);
                                Ok(())
                            }
                            (0, _) => Ok(()),
                            (_, Err(e)) => Err(e.clone()),
                            _ => Ok(()),
                        };
                        if let Err(e) = r {
                            self.error = Some(e);
                        }
                        if matches!(self.docs[i].content, Content::Text(_)) && !vault::is_md(&rel) {
                            let mut act = 0;
                            egui::containers::menu::MenuButton::new(crate::i18n::t("code ▾")).ui(ui, |ui| {
                                ui.set_min_width(260.0);
                                for (k, l) in [(1, "formater le fichier   Alt+Maj+F"), (2, "aller à la ligne   Ctrl+G"), (3, "aller au symbole   Ctrl+Maj+O"), (4, "retour à la ligne   Alt+Z"), (5, "terminal   Ctrl+`"), (6, "vérifier maintenant"), (7, "tout replier"), (8, "tout déplier"), (9, "vue côte à côte")] {
                                    if ui.button(crate::i18n::t(l)).clicked() {
                                        act = k;
                                    }
                                }
                                ui.separator();
                                ui.label(RichText::new(crate::i18n::t("Ctrl+D : occurrence suivante (plusieurs curseurs)\nCtrl+Maj+L : toutes les occurrences\nCtrl+L : sélectionner la ligne\nAlt+↑↓ : déplacer · Alt+Maj+↑↓ : dupliquer\nCtrl+/ : commenter · Ctrl+] [ : indenter\nCtrl+Entrée : ligne en dessous\nCtrl+Espace : suggestions · Tab : extrait\nCtrl+Maj+[ ] : replier, déplier\nF12 : aller à la définition (LSP)")).color(p.muted).size(10.5));
                            });
                            match act {
                                1 => self.want_format = true,
                                2 => self.docs[i].code.goto = Some(String::new()),
                                3 => self.docs[i].code.goto = Some("@".into()),
                                4 => {
                                    let w = self.docs[i].code.wrap.unwrap_or(self.prefs.wrap_code);
                                    self.docs[i].code.wrap = Some(!w);
                                }
                                5 => self.want_term = true,
                                7 | 8 => {
                                    if let Content::Text(t) = &self.docs[i].content {
                                        let t = t.clone();
                                        code::fold_all(&mut self.docs[i].code, &t, act == 7);
                                    }
                                }
                                9 => self.toggle_split(),
                                6 => {
                                    if let Content::Text(t) = &self.docs[i].content {
                                        let lang = ext(&rel);
                                        let t = t.clone();
                                        if let Ok(path) = vault::safe(&root, &rel) {
                                            self.docs[i].code.diags = code::check(&path, &lang, &t);
                                            if self.docs[i].code.diags.is_empty() {
                                                self.toast = Some(("aucune erreur trouvée".into(), Instant::now()));
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        if rel.to_lowercase().ends_with(".py") && ui.button(crate::i18n::t("▶ Blender")).on_hover_text(crate::i18n::t("Lance le script dans Blender en arrière-plan (blender --background --python)")).clicked() {
                            self.want_blender = true;
                        }
                        if extras::runner_for(&rel).is_some() && ui.button(crate::i18n::t("▶ exécuter")).on_hover_text(crate::i18n::t("Lance le script et affiche sa sortie (F5)")).clicked() {
                            *run_now = true;
                        }
                        if matches!(self.docs[i].content, Content::Text(_)) && ui.button(crate::i18n::t("historique")).on_hover_text(crate::i18n::t("Versions précédentes de ce fichier")).clicked() {
                            self.flush_all();
                            self.history_dlg = Some(extras::HistoryDlg { versions: extras::history_list(&root, &rel), rel: rel.clone(), sel: 0, diff: true });
                        }
                        if live::is_previewable(&rel) && ui.button(crate::i18n::t("aperçu live")).on_hover_text(crate::i18n::t("Ouvre la page dans ton navigateur ; elle se recharge à chaque enregistrement")).clicked() {
                            self.flush_all();
                            match live::open(&root, &rel) {
                                Ok(url) => self.toast = Some((format!("aperçu : {url}"), Instant::now())),
                                Err(e) => self.error = Some(e),
                            }
                        }
                        if can_summarize && ui.button(crate::i18n::t("résumer")).on_hover_text(crate::i18n::t("Résumé IA de ce fichier")).clicked() {
                            self.actions.push(Action::Summarize);
                        }
                        if is_md {
                            let d = &mut self.docs[i];
                            let label = if d.preview { "éditer" } else { "lire" };
                            if ui.button(crate::i18n::t(label)).on_hover_text(crate::i18n::t("Ctrl+E")).clicked() {
                                d.preview = !d.preview;
                            }
                        }
                        // Le chemin prend la place qui reste, à gauche, et se coupe au besoin : il ne passe plus sous les boutons
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let shown = vault::display_rel(&rel);
                            let mut job = egui::text::LayoutJob::default();
                            let parts: Vec<&str> = shown.split('/').collect();
                            for (k, part) in parts.iter().enumerate() {
                                let last = k == parts.len() - 1;
                                job.append(part, 0.0, egui::TextFormat { font_id: egui::FontId::monospace(12.0), color: if last { p.text } else { p.muted }, ..Default::default() });
                                if !last {
                                    job.append(" / ", 0.0, egui::TextFormat { font_id: egui::FontId::monospace(12.0), color: p.line, ..Default::default() });
                                }
                            }
                            ui.add(egui::Label::new(job).truncate()).on_hover_text(&shown);
                        });
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
                        open = ui.button(crate::i18n::t("ouvrir avec l'app par défaut")).clicked();
                    });
                    if open {
                        let r = vault::safe(&root, &rel).and_then(|p| settings::open_external(&p));
                        self.report(r);
                    }
                }
                Content::Video(viewer) => {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| viewer.show(ui));
                }
                Content::Montage(m) => {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| m.show(ui));
                    if let Some(path) = m.open_request.take() {
                        open_material = Some(path);
                    }
                }
                Content::NeedFfmpeg(job) => {
                    let mut reopen = false;
                    egui::Frame::new().inner_margin(30).show(ui, |ui| {
                        ui.label(RichText::new(crate::i18n::t("Les vidéos ont besoin de ffmpeg.")).family(theme::bold()));
                        ui.label(RichText::new(format!("Aryadne peut l'installer pour toi, sans droits admin (~100 Mo, dans {}).", ffmpeg::install_dir().display())).color(p.muted).size(12.0));
                        ui.add_space(10.0);
                        match job {
                            None => {
                                let b = egui::Button::new(RichText::new(crate::i18n::t("installer ffmpeg")).color(p.on_accent).family(theme::bold())).fill(p.accent).stroke(egui::Stroke::NONE);
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
                Content::Paint(ed) => {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| ed.show(ui));
                }
                Content::Nodes(ed) => {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| ed.show(ui));
                    if let Some((id, desc)) = ed.ai_request.take() {
                        let settings = self.ai.clone().unwrap_or_else(ai::Settings::load);
                        let local = self.local.clone();
                        let ctx = ui.ctx().clone();
                        let (tx, rx) = std::sync::mpsc::channel();
                        let mat = ed.graph.kind == nodes::GraphKind::Material;
                        std::thread::spawn(move || {
                            let ready = if ollama::is_local(&settings.url) { local.ensure_running(&settings.url) } else { Ok(()) };
                            let user = format!("Effet voulu ({}) : {desc}", if mat { "matériau 3D" } else { "image" });
                            let r = ready.and_then(|_| ai::complete(&settings, nodes::SHADER_AI_PROMPT, &user)).map(|t| nodes::clean_ai_code(&t)).and_then(|code| nodes::Shader::compile(&code).map(|_| code).map_err(|e| format!("Le code de l'IA ne marche pas : {e}")));
                            let _ = tx.send((id, r));
                            ctx.request_repaint();
                        });
                        ed.ai_rx = Some(rx);
                    }
                }
                Content::Model(viewer) => {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| viewer.show(ui));
                    if let Some(path) = viewer.open_request.take() {
                        open_material = Some(path);
                    }
                }
                Content::Image => {
                    let slot = self.images.get(&rel, images::Size::Full);
                    match slot {
                        images::Slot::Ready(l) => {
                            let size = l.tex.size_vec2();
                            let zid = egui::Id::unique(("img-zoom", &rel));
                            let mut z: zoomview::Zoom = ui.data(|d| d.get_temp(zid).unwrap_or_default());
                            let avail = ui.available_size() - egui::vec2(0.0, 30.0);
                            let (area, resp) = ui.allocate_exact_size(avail.max(egui::vec2(60.0, 60.0)), egui::Sense::click_and_drag());
                            z.interact(ui, &resp, area.shrink(12.0), size, true);
                            let painter = ui.painter_at(area);
                            painter.rect_filled(area, 0.0, p.panel);
                            let ir = z.image_rect(area.shrink(12.0), size);
                            // Très agrandie : on voit les pixels nets
                            let opts = if z.scale.is_some_and(|s| s >= 3.0) { egui::TextureOptions::NEAREST } else { egui::TextureOptions::LINEAR };
                            let _ = opts;
                            painter.image(l.tex.id(), ir, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
                            }
                            z.badge(ui, area, size);
                            ui.data_mut(|d| d.insert_temp(zid, z));
                            ui.horizontal(|ui| {
                                ui.add_space(12.0);
                                ui.label(RichText::new(format!("{} × {} px · molette : zoom · glisser : déplacer · double-clic : 100 % / ajuster", l.dims[0], l.dims[1])).color(p.muted).size(11.0));
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
                                ui.label(RichText::new(crate::i18n::t("chargement…")).color(p.muted));
                            });
                        }
                        images::Slot::Failed(e) => {
                            ui.label(RichText::new(e).color(p.missing));
                        }
                    }
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
                    let id = egui::Id::unique(("code", &rel));
                    let notes: Vec<String> = existing.iter().cloned().collect();
                    let ed = &mut doc.code;
                    let wrap = ed.wrap.unwrap_or(is_md || lang.is_empty() || lang == "txt" || self.prefs.wrap_code);
                    let numbers = !is_md && self.prefs.line_numbers;
                    // Serveur LSP (si installé) : erreurs en direct, définitions, complétion
                    if !ed.lsp_tried && !is_md {
                        ed.lsp_tried = true;
                        if let Ok(path) = vault::safe(&root, &rel) {
                            ed.lsp = lsp::Doc::open(&root, &path, text, ui.ctx());
                        }
                    }
                    if let Some(l) = &mut ed.lsp {
                        if let Some(line) = l.poll() {
                            ed.jump = Some(line);
                        }
                        if let Some(d) = l.new_diags() {
                            ed.diags = d.into_iter().map(|(line, msg)| code::Diag { line, msg }).collect();
                        }
                        if let Some(g) = l.goto.take() {
                            lsp_goto = Some(g);
                        }
                        if !l.waits.is_empty() {
                            ui.ctx().request_repaint_after(Duration::from_millis(60));
                        }
                    }
                    let folds = ed.folds.clone();
                    let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, width: f32| {
                        let mut job = egui_extras::syntax_highlighting::highlight(ui.ctx(), ui.style(), &theme, buf.as_str(), &lang);
                        job.wrap.max_width = if wrap { width } else { f32::INFINITY };
                        code::apply_folds(&mut job, &code::hidden_bytes(buf.as_str(), &folds));
                        ui.painter().layout_job(job)
                    };
                    let mut changed = code::find_bar(ui, ed, text, id);
                    code::goto_bar(ui, ed, text, &lang);
                    changed |= code::before(ui, ed, text, &lang, id, &notes);
                    let typed_now = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_))));
                    // Erreurs trouvées à l'enregistrement
                    if !ed.diags.is_empty() {
                        let d = ed.diags[0].clone();
                        let n = ed.diags.len();
                        egui::Frame::new().fill(p.missing.gamma_multiply(0.12)).inner_margin(egui::Margin::symmetric(12, 4)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let t = format!("⚠ ligne {} : {}{}", d.line + 1, d.msg, if n > 1 { format!("  (+{} autre(s))", n - 1) } else { String::new() });
                            if ui.add(egui::Button::new(RichText::new(t).color(p.missing).size(11.5)).frame(false)).on_hover_text(crate::i18n::t("aller à l'erreur")).clicked() {
                                ed.jump = Some(d.line);
                            }
                        });
                    }
                    let digits = text.lines().count().max(1).to_string().len().max(3) as f32;
                    let gutter = if numbers { (digits * fs * 0.62 + 26.0) as i8 } else if wrap { 40 } else { 12 };
                    let scroll = if wrap { egui::ScrollArea::vertical() } else { egui::ScrollArea::both() };
                    scroll.id_salt(("scroll", &rel)).auto_shrink(false).show(ui, |ui| {
                        egui::Frame::new().inner_margin(egui::Margin { left: gutter, right: if !is_md && self.prefs.minimap && text.lines().count() > 40 { 84 } else { 24 }, top: 16, bottom: 120 }).show(ui, |ui| {
                            let bg = ui.painter().add(egui::Shape::Noop);
                            let edit = egui::TextEdit::multiline(text)
                                .id(id)
                                .frame(egui::Frame::NONE)
                                .code_editor()
                                .lock_focus(true)
                                .desired_rows(40)
                                .desired_width(if wrap { (ui.available_width()).min(if is_md { 820.0 } else { 2000.0 }) } else { f32::INFINITY })
                                .layouter(&mut layouter);
                            let out = edit.show(ui);
                            changed |= out.response.changed();
                            code::after(ui, ed, text, &lang, &out, &notes, typed_now && out.response.changed(), code::View { minimap: !is_md && self.prefs.minimap, gutter: numbers, font_size: fs, bg });
                        });
                    });
                    if changed {
                        doc.dirty_since = Some(Instant::now());
                        if let Some(l) = &mut doc.code.lsp {
                            l.sync(text);
                        }
                    }
                }
            }
        });
        // Matériau édité à côté du modèle (vue côte à côte)
        if let Some(path) = open_material {
            if let Some(rel) = vault::inside(&self.root(), &path) {
                let model = self.active;
                self.refresh();
                self.open(rel.clone());
                self.split = Some(rel);
                self.active = model;
            }
        }
        if let Some((path, line)) = lsp_goto {
            match vault::inside(&self.root(), &path) {
                Some(rel) => {
                    self.open(rel);
                    if let Some(a) = self.active {
                        self.docs[a].code.jump = Some(line);
                    }
                }
                None => self.toast = Some((format!("définition hors du coffre : {}:{}", path.display(), line + 1), Instant::now())),
            }
        }
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        if crate::dock::closed("side") {
            self.side_hidden = true;
            return;
        }
        crate::dock::show_ex(ui, "side", &crate::i18n::t("structure"), crate::dock::Side::Right, 290.0, 200.0, egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(12), true, |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    let is_md = self.current().is_some_and(|d| vault::is_md(&d.rel));
                    // Structure : fonctions, classes, titres… (clic = aller à la ligne)
                    let syms = self.current().and_then(|d| if let Content::Text(t) = &d.content { Some(code::symbols(t, &if vault::is_md(&d.rel) { "md".to_string() } else { ext(&d.rel) })) } else { None }).unwrap_or_default();
                    if !syms.is_empty() {
                        ui.label(caps(&format!("{} · {}", crate::i18n::t("structure"), syms.len())));
                        ui.add_space(4.0);
                        let mut jump = None;
                        for sy in syms.iter().take(300) {
                            ui.horizontal(|ui| {
                                ui.add_space(sy.depth.min(6) as f32 * 12.0);
                                let col = match sy.kind {
                                    "fn" | "méthode" => egui::Color32::from_rgb(0x4f, 0x8a, 0xb8),
                                    "type" | "impl" => egui::Color32::from_rgb(0x8a, 0x5c, 0xd6),
                                    "titre" => p.accent,
                                    _ => p.muted,
                                };
                                ui.label(RichText::new(match sy.kind { "fn" | "méthode" => "ƒ", "type" | "impl" => "◇", "titre" => "#", _ => "·" }).color(col).size(11.0));
                                if ui.add(egui::Button::new(RichText::new(&sy.name).size(11.5)).frame(false)).on_hover_text(format!("ligne {}", sy.line + 1)).clicked() {
                                    jump = Some(sy.line);
                                }
                            });
                        }
                        if let (Some(l), Some(i)) = (jump, self.active) {
                            self.docs[i].code.jump = Some(l);
                        }
                        ui.add_space(16.0);
                    }
                    if is_md {
                        ui.label(caps(&format!("{} · {}", crate::i18n::t("liens entrants"), self.backlinks.len())));
                        ui.add_space(4.0);
                        if self.backlinks.is_empty() {
                            ui.label(RichText::new(crate::i18n::t("aucune note ne pointe ici")).color(p.muted).size(11.5));
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

                    ui.label(caps(&crate::i18n::t("résumé ia")));
                    ui.add_space(4.0);
                    let mut save = None;
                    match &self.summary {
                        None => {
                            ui.label(RichText::new(crate::i18n::t("ouvre un fichier et clique sur « résumer »")).color(p.muted).size(11.5));
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
                            if ui.button(crate::i18n::t("réglages ia")).clicked() {
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
                                if ui.button(crate::i18n::t("copier")).clicked() {
                                    ui.ctx().copy_text(text.clone());
                                }
                                if ui.button(crate::i18n::t("enregistrer en note")).clicked() {
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
                    ui.label(caps(&crate::i18n::t("cerveau")));
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
                    if ui.checkbox(&mut self.brain.with_folders, crate::i18n::t("dossiers")).changed() {
                        self.brain.dirty = true;
                    }
                    if ui.checkbox(&mut self.brain.with_files, crate::i18n::t("tous les fichiers")).changed() {
                        self.brain.dirty = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(crate::i18n::t("recentrer")).clicked() {
                            self.brain.recenter();
                        }
                        let busy = self.organize.as_ref().is_some_and(|o| o.rx.is_some());
                        if busy {
                            ui.spinner();
                        } else if ui.button(crate::i18n::t("ranger avec l'IA")).on_hover_text(crate::i18n::t("L'IA propose un rangement de ce dossier ; tu choisis quoi appliquer")).clicked() {
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
                        self.files.expanded.insert(target.clone());
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
            ui.label(caps(&crate::i18n::t("rangement proposé par l'ia")));
            ui.add_space(6.0);
            if let Some(e) = &o.error {
                ui.label(RichText::new(e).color(p.missing));
            } else if o.moves.is_empty() {
                ui.label(RichText::new(crate::i18n::t("Rien à ranger : tout semble déjà à sa place.")).color(p.muted));
            } else {
                ui.label(RichText::new(crate::i18n::t("Décoche ce que tu ne veux pas. Rien n'est déplacé avant « appliquer ».")).color(p.muted).size(11.5));
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
                close = ui.button(crate::i18n::t("fermer")).clicked();
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
        let mut rel = vault::join(dir, &format!("{name}.{}", formats::ext("board")));
        let mut i = 2;
        while self.entries.iter().any(|e| e.rel == rel) {
            rel = vault::join(dir, &format!("{name} {i}.{}", formats::ext("board")));
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
            let t = text.trim();
            if t.starts_with("http://") || t.starts_with("https://") {
                let ctx = self.ctx.clone();
                if let Some(ctx) = ctx {
                    self.import_url(t.to_string(), &ctx);
                }
                return;
            }
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
        // Du texte simple, sur un board : il devient une étiquette
        if self.view() == View::Boards {
            if let Ok(text) = cb.get_text() {
                if !text.trim().is_empty() {
                    self.board.paste_text(&text, at.to_pos2());
                    return;
                }
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

    /// Taille d'une image posée sur un board : sa taille d'origine en pixels, sinon une vignette aux bonnes proportions.
    fn board_size(&self, rel: &str) -> egui::Vec2 {
        match self.images.natural(rel) {
            Some([w, h]) => egui::vec2(w as f32, h as f32),
            None => {
                let a = self.images.peek(rel, images::Size::Thumb).map(|l| l.dims[0] as f32 / l.dims[1].max(1) as f32).unwrap_or(1.5);
                egui::vec2(240.0 * a, 240.0)
            }
        }
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
        let picks: Vec<(String, egui::Vec2)> = found.into_iter().take(40).map(|(r, _)| (r.clone(), self.board_size(&r))).collect();
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
        let mut picks = vec![(seed.to_string(), self.board_size(seed))];
        picks.extend(scored.into_iter().take(15).filter(|(_, d)| *d < 90.0).map(|(r, _)| (r.clone(), self.board_size(&r))));
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
                ui.horizontal_wrapped(|ui| {
                    ui.label(caps(&crate::i18n::t("board")));
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
                    if ui.button(crate::i18n::t("+ board")).clicked() {
                        self.new_board("Moodboard");
                    }
                    ui.separator();
                    // Outils
                    // Outils : sélection et texte, puis les outils de dessin de l'annotation vidéo
                    for (tool, label, key) in [(board::Tool::Select, "sélection", "V"), (board::Tool::Text, "texte", "T")] {
                        if ui.selectable_label(self.board.tool == tool, crate::i18n::t(label)).on_hover_text(key).clicked() {
                            self.board.tool = tool;
                        }
                    }
                    ui.separator();
                    for (tool, icon, label, key) in board::DRAW_TOOLS {
                        let on = self.board.tool == tool;
                        let b = egui::Button::new(RichText::new(icon).size(14.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { p.panel }).min_size(egui::vec2(26.0, 22.0));
                        let key = if tool == board::Tool::Arrow { "Maj+A".to_string() } else { key.to_string() };
                        if ui.add(b).on_hover_text(format!("{} ({key})", crate::i18n::t(label))).clicked() {
                            self.board.tool = tool;
                        }
                    }
                    let drawing = !matches!(self.board.tool, board::Tool::Select | board::Tool::Text | board::Tool::Eraser);
                    if drawing {
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
                        egui::containers::menu::MenuButton::new(crate::i18n::t("trait ▾")).ui(ui, |ui| {
                            ui.set_min_width(240.0);
                            ui.add(egui::Slider::new(&mut self.board.pen_width, 0.5..=60.0).logarithmic(true).text(crate::i18n::t("épaisseur")));
                            ui.add(egui::Slider::new(&mut self.board.opacity, 0.05..=1.0).text(crate::i18n::t("opacité")));
                            ui.add(egui::Slider::new(&mut self.board.stabilizer, 0.0..=0.95).text(crate::i18n::t("stabilisateur")));
                            ui.add(egui::Slider::new(&mut self.board.taper, 0.0..=1.0).text(crate::i18n::t("effilé")));
                            ui.label(RichText::new(crate::i18n::t("Maj : ligne droite, carré, cercle, angles de 15°")).size(10.5).color(p.muted));
                        });
                    }
                    ui.separator();
                    if ui.add_enabled(self.board.can_undo(), egui::Button::new("↶")).on_hover_text(crate::i18n::t("annuler (Ctrl+Z)")).clicked() {
                        self.board.undo();
                    }
                    if ui.add_enabled(self.board.can_redo(), egui::Button::new("↷")).on_hover_text(crate::i18n::t("rétablir (Ctrl+Y)")).clicked() {
                        self.board.redo();
                    }
                    ui.scope(|ui| {
                        if ui.button(crate::i18n::t("cadrer")).clicked() {
                            self.board.fit();
                        }
                    });
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(crate::i18n::t("auto")).color(p.accent).family(theme::bold()));
                    ui.add(egui::TextEdit::singleline(&mut self.auto_query).hint_text(crate::i18n::t("mots-clés : forêt brume…")).desired_width(200.0));
                    if ui.button(crate::i18n::t("par mots-clés")).on_hover_text(crate::i18n::t("Ajoute les images dont le nom, le dossier ou les notes qui les citent correspondent")).clicked() {
                        let q = self.auto_query.clone();
                        let n = self.auto_board_keywords(&q);
                        self.toast(if n == 0 { "aucune image ne correspond".to_string() } else { format!("{n} images ajoutées") });
                    }
                    let seed = (self.board.selection.len() == 1)
                        .then(|| self.board.selection[0])
                        .and_then(|i| self.board.data.items.get(i))
                        .and_then(|it| if let board::Kind::Image { rel, .. } = &it.kind { Some(rel.clone()) } else { None });
                    let btn = ui.add_enabled(seed.is_some(), egui::Button::new(crate::i18n::t("par couleurs"))).on_hover_text(crate::i18n::t("Ajoute les refs aux couleurs proches de l'image sélectionnée")).on_disabled_hover_text("sélectionne d'abord une seule image du board");
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
        // Ctrl+V dans les refs : image copiée ou lien vers une image
        let typing = ui.ctx().egui_wants_keyboard_input();
        let pasted = ui.input(|i| i.events.iter().find_map(|e| if let egui::Event::Paste(t) = e { Some(t.clone()) } else { None }));
        if !typing {
            if let Some(t) = pasted {
                let t = t.trim().to_string();
                if t.starts_with("http://") || t.starts_with("https://") {
                    let ctx = ui.ctx().clone();
                    self.import_url(t, &ctx);
                } else {
                    self.paste_image(egui::Vec2::ZERO);
                }
            }
        }
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
            egui::Panel::right("ref-detail").resizable(true).default_size(320.0).min_size(240.0).max_size((ui.available_width() * 0.5).max(260.0))
                .frame(egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(12))
                .show(ui, |ui| {
                    ui.label(caps(&crate::i18n::t("référence")));
                    ui.label(RichText::new(&sel).size(11.5));
                    ui.add_space(6.0);
                    match self.images.get(&sel, images::Size::Full) {
                        images::Slot::Ready(l) => {
                            let s = l.tex.size_vec2();
                            let scale = ui.available_width() / s.x;
                            ui.add(egui::Image::new(&l.tex).fit_to_exact_size(s * scale));
                            ui.label(RichText::new(format!("{} × {} px", l.dims[0], l.dims[1])).color(p.muted).size(11.0));
                            ui.add_space(6.0);
                            ui.label(caps(&crate::i18n::t("palette · clic pour copier")));
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
                        if ui.button(crate::i18n::t("ouvrir")).clicked() {
                            self.actions.push(Action::Open(sel.clone()));
                        }
                        if ui.button(crate::i18n::t("ajouter au board")).clicked() {
                            self.ensure_board();
                            let c = self.board.view_center;
                            self.board.drop_file(&sel, c, &mut self.images);
                            self.toast(format!("ajouté à « {} »", board::board_name(self.board.rel.as_deref().unwrap_or(""))));
                        }
                        if ui.button(crate::i18n::t("board auto (couleurs)")).on_hover_text(crate::i18n::t("Ajoute cette image et les refs aux couleurs proches au board")).clicked() {
                            self.ensure_board();
                            match self.auto_board_colors(&sel) {
                                Ok(n) => {
                                    self.toast(format!("{n} images ajoutées au board"));
                                    self.view = Some(View::Boards);
                                }
                                Err(e) => self.toast(e),
                            }
                        }
                        if ui.button(crate::i18n::t("explorateur")).clicked() {
                            self.selected = Some(sel.clone());
                            self.files.reveal(&sel);
                        }
                    });
                });
        }

        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| {
            egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(12, 5)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(caps(&format!("refs · {}", imgs.len())));
                    ui.add(egui::TextEdit::singleline(&mut self.refs.filter).hint_text(crate::i18n::t("filtrer par nom ou dossier")).desired_width(200.0));
                    ui.separator();
                    for (s, label) in [(RefSort::Recent, "récentes"), (RefSort::Name, "nom"), (RefSort::Color, "couleur")] {
                        if ui.selectable_label(self.refs.sort == s, label).clicked() {
                            self.refs.sort = s;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(egui::Slider::new(&mut self.refs.size, 90.0..=340.0).show_value(false));
                        ui.label(RichText::new(crate::i18n::t("taille")).color(p.muted).size(11.0));
                    });
                });
            });
            if imgs.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new(crate::i18n::t("aucune image\n\nglisse des images dans la fenêtre, ou :")).color(p.muted));
                    if ui.button(crate::i18n::t("importer des fichiers…")).clicked() {
                        self.pick_import(false);
                    }
                    if ui.button(crate::i18n::t("lier un dossier…")).clicked() {
                        self.pick_import(true);
                    }
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
        let mut apply_file: Option<(String, String)> = None;
        let mut insert: Option<String> = None;
        let mut bar_out: Option<extras::ChatBarOut> = None;
        if crate::dock::closed("assistant") {
            self.assistant_open = false;
            return;
        }
        crate::dock::show_ex(ui, "assistant", &crate::i18n::t("assistant"), crate::dock::Side::Right, 340.0, 240.0, egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(12), true, |ui| {
                ui.horizontal(|ui| {
                    ui.label(caps(&crate::i18n::t("assistant")));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").on_hover_text(crate::i18n::t("Ctrl+L")).clicked() {
                            self.assistant_open = false;
                        }
                        if ui.small_button(crate::i18n::t("vider")).clicked() {
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
                    let settings = self.ai.clone().unwrap_or_else(ai::Settings::load);
                    let of = self.open_file_path();
                    bar_out = Some(extras::chat_bar(ui, &mut self.assistant, &self.local, &settings, of));
                    let edit = egui::TextEdit::multiline(&mut self.assistant.input).hint_text(crate::i18n::t("demande quelque chose… (Entrée pour envoyer)")).desired_rows(3).desired_width(f32::INFINITY);
                    let r = ui.add(edit);
                    if r.has_focus() && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift) {
                        self.assistant.input = self.assistant.input.trim_end_matches('\n').to_string();
                        send = true;
                    }
                });
                egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink(false).show(ui, |ui| {
                    if self.assistant.msgs.is_empty() && !self.assistant.busy() {
                        ui.label(RichText::new(crate::i18n::t("Pose une question sur ce que tu as à l'écran. Les blocs de code proposés peuvent être insérés directement dans le fichier ouvert.")).color(p.muted).size(11.5));
                    }
                    let n = self.assistant.msgs.len();
                    let mut reply = None;
                    for (k, m) in self.assistant.msgs.iter().enumerate() {
                        let user = m.role == chat::Role::User;
                        let (text, choices) = chat::split_choices(&m.text);
                        let shown = if m.attached.is_empty() { text } else { format!("{text}\n\n📎 {}", m.attached.join(", ")) };
                        msg_block(ui, user, &shown, &m.sources, m.error.as_deref(), None, &existing, &paths, &mut goto);
                        if k + 1 == n && !user && self.assistant.live.is_none() {
                            reply = extras::choice_buttons(ui, &choices);
                        }
                        if !user {
                            if let Some(f) = extras::file_buttons(ui, &m.text, &|r| paths.contains(r)) {
                                apply_file = Some(f);
                            }
                        }
                        if !user {
                            for (k, block) in code_blocks(&m.text).into_iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("bloc {}", k + 1)).color(p.muted).size(11.0));
                                    if ui.small_button(crate::i18n::t("insérer au curseur")).clicked() {
                                        insert = Some(block.clone());
                                    }
                                    if ui.small_button(crate::i18n::t("copier")).clicked() {
                                        ui.ctx().copy_text(block.clone());
                                    }
                                });
                            }
                        }
                    }
                    if let Some(live) = &self.assistant.live {
                        if let Ok(l) = live.lock() {
                            let status_s = l.status_line();
                            let status = status_s.as_deref();
                            msg_block(ui, false, &l.text, &[], l.error.as_deref(), status, &existing, &paths, &mut goto);
                        }
                    }
                    if let Some(r) = reply {
                        self.assistant.input = r;
                        send = true;
                    }
                });
            });
        if let Some(b) = bar_out {
            self.apply_chat_bar(b, &ctxt);
        }
        if send && !self.assistant.busy() {
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
        if let Some((rel, content)) = apply_file {
            self.apply_ai_file(rel, content);
        }
    }

    fn chat_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        self.chat.poll();
        let root = self.root();
        self.chat.autosave(&root);
        let mut bar_out: Option<extras::ChatBarOut> = None;
        let existing: HashSet<String> = self.md.keys().cloned().collect();
        let paths: HashSet<String> = self.entries.iter().map(|e| e.rel.clone()).collect();
        let mut send = false;
        let mut save = false;
        let mut apply_file: Option<(String, String)> = None;

        egui::Panel::bottom("chat-input")
            .frame(egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(12))
            .show(ui, |ui| {
                let open_name = self.current().map(|d| d.rel.clone());
                ui.horizontal_wrapped(|ui| {
                    let label = match &open_name {
                        Some(n) => format!("inclure le fichier ouvert ({n})"),
                        None => "inclure le fichier ouvert".into(),
                    };
                    ui.add_enabled(open_name.is_some(), egui::Checkbox::new(&mut self.chat.include_open, RichText::new(label).size(11.5)));
                    // Les boutons passent à la ligne plutôt que de recouvrir la case à cocher
                    ui.scope(|ui| {
                        if ui.button(crate::i18n::t("nouvelle")).clicked() && !self.chat.busy() {
                            self.chat.new_conversation();
                        }
                        // Conversations enregistrées
                        let root = self.root();
                        egui::containers::menu::MenuButton::new(crate::i18n::t("conversations ▾")).ui(ui, |ui| {
                            ui.set_min_width(320.0);
                            let list = chat::list_saved(&root);
                            if list.is_empty() {
                                ui.label(RichText::new(crate::i18n::t("aucune pour l'instant : elles s'enregistrent toutes seules")).color(p.muted).size(11.0));
                            }
                            for (id, title, _) in list.iter().take(40) {
                                ui.horizontal(|ui| {
                                    let cur = self.chat.id.as_deref() == Some(id.as_str());
                                    if ui.selectable_label(cur, if title.is_empty() { "(sans titre)" } else { title.as_str() }).clicked() && !self.chat.busy() {
                                        self.chat.load_saved(&root, id);
                                    }
                                    if ui.small_button("×").on_hover_text(crate::i18n::t("supprimer")).clicked() {
                                        let _ = std::fs::remove_file(chat::chats_dir(&root).join(format!("{id}.json")));
                                        if cur {
                                            self.chat.new_conversation();
                                        }
                                    }
                                });
                            }
                        });
                        if !self.chat.msgs.is_empty() {
                            if ui.add(egui::TextEdit::singleline(&mut self.chat.title).hint_text(crate::i18n::t("titre")).desired_width(160.0)).changed() {
                                self.chat.saved_len = 0;
                            }
                        }
                        if ui.add_enabled(!self.chat.msgs.is_empty(), egui::Button::new(crate::i18n::t("enregistrer en note"))).clicked() {
                            save = true;
                        }
                        let model = self.ai.clone().unwrap_or_default().model;
                        ui.label(RichText::new(format!("modèle : {model}")).color(p.muted).size(11.0));
                    });
                });
                ui.add_space(4.0);
                let settings = self.ai.clone().unwrap_or_else(ai::Settings::load);
                let of = self.open_file_path();
                let bar = extras::chat_bar(ui, &mut self.chat, &self.local, &settings, of);
                bar_out = Some(bar);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let enter = ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
                    let edit = egui::TextEdit::multiline(&mut self.chat.input)
                        .hint_text(crate::i18n::t("demande n'importe quoi sur ton coffre…  (entrée : envoyer · maj+entrée : nouvelle ligne)"))
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
                    let btn = egui::Button::new(RichText::new(crate::i18n::t("envoyer")).color(p.on_accent).family(theme::bold())).fill(p.accent).stroke(egui::Stroke::NONE).min_size(egui::vec2(80.0, 40.0));
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
                        ui.label(RichText::new(crate::i18n::t("chat")).size(22.0).family(theme::bold()));
                        ui.label(RichText::new(crate::i18n::t("// pose une question : je cherche dans tes notes, ton code et tes fichiers, et je te renvoie vers les bons.")).color(p.muted));
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
                    let last = msgs.len().saturating_sub(1);
                    for (k, m) in msgs.iter().enumerate() {
                        let (text, choices) = chat::split_choices(&m.text);
                        let shown = if m.attached.is_empty() { text } else { format!("{text}\n\n📎 {}", m.attached.join(", ")) };
                        msg_block(ui, m.role == chat::Role::User, &shown, &m.sources, m.error.as_deref(), None, &existing, &paths, &mut goto);
                        if k == last && m.role == chat::Role::Assistant && !self.chat.busy() {
                            if let Some(c) = extras::choice_buttons(ui, &choices) {
                                self.chat.input = c;
                                send = true;
                            }
                        }
                        if m.role == chat::Role::Assistant {
                            if let Some(f) = extras::file_buttons(ui, &m.text, &|r| paths.contains(r)) {
                                apply_file = Some(f);
                            }
                        }
                    }
                    if let Some(live) = &self.chat.live {
                        if let Ok(l) = live.lock() {
                            let status_s = l.status_line();
                            let status = status_s.as_deref();
                            msg_block(ui, false, &l.text, &l.sources, l.error.as_deref(), status, &existing, &paths, &mut goto);
                        }
                    }
                });
            });
            if let Some(g) = goto {
                self.actions.push(Action::OpenLink(g));
            }
        });

        if let Some(b) = bar_out {
            let ctx = ui.ctx().clone();
            self.apply_chat_bar(b, &ctx);
        }
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
        if let Some((rel, content)) = apply_file {
            self.apply_ai_file(rel, content);
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
                ui.label(caps(&crate::i18n::t("transfert entre ordinateurs")));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(crate::i18n::t("fermer")).clicked() {
                        keep = false;
                    }
                });
            });
            ui.label(RichText::new(crate::i18n::t("Sur le même réseau (wifi ou câble), sans cloud et sans limite de taille. Idéal pour les fichiers 3D.")).color(p.muted).size(11.5));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.selectable_label(!send, crate::i18n::t("recevoir")).clicked() {
                    send = false;
                }
                if ui.selectable_label(send, crate::i18n::t("envoyer")).clicked() {
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
                        ui.label(crate::i18n::t("Mets cet ordinateur en réception, puis envoie depuis l'autre."));
                        ui.add_space(6.0);
                        let b = egui::Button::new(RichText::new(crate::i18n::t("commencer à recevoir")).color(p.on_accent).family(theme::bold())).fill(p.accent).stroke(egui::Stroke::NONE);
                        if ui.add(b).clicked() {
                            match transfer::Receiver::start(self.root(), ctx.clone()) {
                                Ok(r) => self.receiver = Some(r),
                                Err(e) => self.error = Some(e),
                            }
                        }
                        ui.label(RichText::new(crate::i18n::t("Windows peut demander d'autoriser Aryadne sur le réseau : accepte pour les réseaux privés.")).color(p.muted).size(11.0));
                    }
                    Some(rx) => {
                        let ip = transfer::local_ip().unwrap_or_else(|| "?".into());
                        egui::Grid::new("rx").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                            ui.label(crate::i18n::t("ordinateur"));
                            ui.label(RichText::new(transfer::hostname()).family(theme::bold()));
                            ui.end_row();
                            ui.label(crate::i18n::t("adresse"));
                            ui.label(RichText::new(format!("{ip}:{}", rx.port)).family(theme::bold()));
                            ui.end_row();
                            ui.label(crate::i18n::t("code"));
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
                            if ui.button(crate::i18n::t("arrêter")).clicked() {
                                self.receiver = None;
                            }
                            if ui.button(crate::i18n::t("ouvrir reçus/")).clicked() {
                                if let Ok(path) = vault::safe(&self.root(), "reçus") {
                                    let _ = std::fs::create_dir_all(&path);
                                    settings::reveal(&path, false);
                                }
                            }
                        });
                    }
                }
                self.relay_receive_ui(ui, ctx);
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
                    if ui.button(crate::i18n::t("+ fichiers de l'ordi…")).clicked() {
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
                        if ui.button(crate::i18n::t("+ fichier ouvert")).clicked() && !self.send_files.contains(&d) {
                            self.send_files.push(d);
                        }
                    }
                    ui.label(RichText::new(crate::i18n::t("ou clic droit dans l'explorateur")).color(p.muted).size(11.0));
                });
                ui.add_space(8.0);
                ui.label(caps(&crate::i18n::t("vers")));
                let peers = self.peers.lock().map(|p| p.clone()).unwrap_or_default();
                if peers.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(crate::i18n::t("aucun ordinateur trouvé (l'autre doit être en mode « recevoir »)")).color(p.muted).size(11.5));
                        if ui.small_button(crate::i18n::t("chercher")).clicked() {
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
                    ui.label(crate::i18n::t("adresse"));
                    ui.add(egui::TextEdit::singleline(&mut self.send_addr).hint_text("192.168.1.42:47123").desired_width(200.0));
                    ui.label(crate::i18n::t("code"));
                    ui.add(egui::TextEdit::singleline(&mut self.send_code).hint_text("1234").desired_width(60.0));
                });
                ui.add_space(8.0);
                let ready = !self.send_files.is_empty() && !self.send_addr.trim().is_empty() && self.send_code.trim().len() == 4;
                let b = egui::Button::new(RichText::new(crate::i18n::t("envoyer")).color(p.on_accent).family(theme::bold())).fill(p.accent).stroke(egui::Stroke::NONE);
                if ui.add_enabled(ready, b).clicked() {
                    self.flush_all();
                    let job = transfer::send(self.root(), self.send_files.clone(), self.send_addr.trim().to_string(), self.send_code.clone(), ctx.clone());
                    self.jobs.push(job);
                }
                self.relay_send_ui(ui, ctx);
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
                    if ui.add(egui::Button::new(RichText::new(crate::i18n::t("quitter le mode compact")).size(11.0)).frame(false)).clicked() {
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
                    let r = ui.add(egui::TextEdit::singleline(&mut self.search.input).hint_text(crate::i18n::t("chercher dans tout le coffre (notes, code, texte, noms de fichiers)…")).font(egui::FontId::monospace(15.0)).frame(egui::Frame::NONE).desired_width(f32::INFINITY));
                    if self.search.results.is_none() && self.search.input.is_empty() {
                        r.request_focus();
                    }
                });
            });
            self.search.run(self.root(), self.entries.clone(), ctx.clone());
            let Some(res) = self.search.results.clone() else {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| ui.label(RichText::new(crate::i18n::t("tape au moins 2 caractères · majuscules et accents ignorés")).color(p.muted)));
                // Nuage de tags #mot trouvés dans les notes
                if self.tags.as_ref().is_none_or(|(t, _)| t.elapsed() > Duration::from_secs(10)) {
                    self.tags = Some((Instant::now(), extras::collect_tags(&self.root(), &self.entries)));
                }
                let tags = self.tags.as_ref().map(|t| t.1.clone()).unwrap_or_default();
                if !tags.is_empty() {
                    ui.add_space(24.0);
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.label(caps(&crate::i18n::t("tags")));
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(16.0);
                        for (t, n) in tags.iter().take(80) {
                            if ui.add(egui::Button::new(RichText::new(format!("#{t}  {n}")).size(12.0))).clicked() {
                                self.search.input = format!("#{t}");
                            }
                        }
                    });
                }
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
                ui.add(egui::TextEdit::singleline(&mut query).hint_text(crate::i18n::t("ouvrir un fichier, ou taper un nom pour créer une note")).frame(egui::Frame::NONE).desired_width(f32::INFINITY)).request_focus();
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
        self.new_file_window(ctx);
        if std::mem::take(&mut self.want_save_as) {
            if let Some(i) = self.active {
                self.save_as(i);
            }
        }
        self.untitled_dialogs(ctx);
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
                    if ui.button(crate::i18n::t("ok")).clicked() {
                        ok = true;
                    }
                    if ui.button(crate::i18n::t("annuler")).clicked() {
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
                    ui.label(caps(&crate::i18n::t("supprimer")));
                    ui.add_space(6.0);
                    if vault::is_external(rel) {
                        ui.label(format!("« {rel} » est dans un dossier lié : il sera retiré de son dossier d'origine et mis dans la corbeille du coffre (.app/trash)."));
                    } else {
                        let n = rel.lines().count();
                        if n > 1 {
                            ui.label(format!("{} {n} {}", crate::i18n::t("Mettre"), crate::i18n::t("éléments à la corbeille du coffre (.app/trash) ? Ctrl+Z dans l'explorateur les remet.")));
                        } else {
                            ui.label(format!("Mettre « {rel} » à la corbeille du coffre (.app/trash) ?"));
                        }
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new(crate::i18n::t("supprimer")).color(p.missing)).clicked() {
                            ok = true;
                        }
                        if ui.button(crate::i18n::t("annuler")).clicked() {
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
                        ui.label(RichText::new(crate::i18n::t("Le fichier quittera son dossier d'origine.")).color(p.muted).size(11.0));
                    }
                    ui.add_space(8.0);
                    ui.label(crate::i18n::t("dans quel dossier du coffre ?"));
                    let dirs: Vec<String> = std::iter::once(String::new()).chain(self.entries.iter().filter(|e| e.is_dir && !vault::is_external(&e.rel)).map(|e| e.rel.clone())).collect();
                    for d in dirs {
                        let label = if d.is_empty() { "/ (racine)".to_string() } else { format!("/{d}") };
                        let t = if d == default_dir { RichText::new(label).color(p.accent).family(theme::bold()) } else { RichText::new(label) };
                        if ui.add(egui::Button::new(t).frame(false)).clicked() {
                            go = Some(d);
                        }
                    }
                    ui.add_space(8.0);
                    if ui.button(crate::i18n::t("annuler")).clicked() {
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
                    ui.label(caps(&crate::i18n::t("importer dans le coffre")));
                    ui.add_space(4.0);
                    let names: Vec<String> = files.iter().take(4).map(|f| f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).collect();
                    let more = if files.len() > 4 { format!(" et {} autres", files.len() - 4) } else { String::new() };
                    ui.label(format!("{}{} → /{}", names.join(", "), more, dir));
                    ui.add_space(8.0);
                    let link = egui::Button::new(RichText::new(crate::i18n::t("lier (garder à sa place)")).color(p.on_accent).family(theme::bold())).fill(p.accent).stroke(egui::Stroke::NONE);
                    if ui.add(link).clicked() {
                        choice = Some("link");
                    }
                    ui.label(RichText::new(crate::i18n::t("Recommandé. Le fichier reste dans son dossier ; Aryadne l'affiche sans le copier.")).color(p.muted).size(11.0));
                    ui.add_space(6.0);
                    if ui.button(crate::i18n::t("déplacer dans le coffre")).clicked() {
                        choice = Some("move");
                    }
                    ui.label(RichText::new(crate::i18n::t("Le fichier quitte son dossier d'origine. Aucun doublon.")).color(p.muted).size(11.0));
                    ui.add_space(6.0);
                    if ui.button(crate::i18n::t("copier dans le coffre")).clicked() {
                        choice = Some("copy");
                    }
                    ui.label(RichText::new(crate::i18n::t("L'original reste en place : le fichier prend deux fois la place.")).color(p.muted).size(11.0));
                    ui.add_space(8.0);
                    ui.checkbox(&mut remember, crate::i18n::t("retenir ce choix (modifiable dans les réglages)"));
                    if ui.button(crate::i18n::t("annuler")).clicked() {
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
                    ui.label(caps(&crate::i18n::t("mises à jour")));
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
                            ui.label(RichText::new(crate::i18n::t("Mise à jour installée. Redémarre pour l'utiliser.")).color(p.accent));
                        }
                        None if st.checked && st.error.is_none() => {
                            ui.label(RichText::new(crate::i18n::t("Tu as la dernière version.")).color(p.muted));
                        }
                        None => {}
                    }
                    if let Some(f) = st.progress {
                        ui.add(egui::ProgressBar::new(f).corner_radius(0.0).fill(p.accent).text(format!("téléchargement… {:.0} %", f * 100.0)));
                    }
                    if st.checking {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(crate::i18n::t("vérification…"));
                        });
                    }
                    if let Some(e) = &st.error {
                        ui.label(RichText::new(e).color(p.missing).size(12.0));
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if st.installed {
                            if ui.button(RichText::new(crate::i18n::t("redémarrer")).family(theme::bold())).clicked() {
                                self.flush_all();
                                self.save_board();
                                updater::restart(ctx);
                            }
                        } else if up.update_available().is_some() && st.progress.is_none() {
                            if ui.button(RichText::new(crate::i18n::t("installer")).family(theme::bold())).clicked() {
                                up.install(self.github_token.clone(), ctx);
                            }
                        }
                        if !st.checking && st.progress.is_none() && ui.button(crate::i18n::t("vérifier")).clicked() {
                            up.check(self.github_token.clone(), ctx);
                        }
                        if ui.button(crate::i18n::t("fermer")).clicked() {
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
            painter.text(screen.center(), egui::Align2::CENTER_CENTER, if self.view() == View::Chat { "déposer pour joindre au message".to_string() } else { format!("déposer pour importer dans /{dir}") }, egui::FontId::monospace(16.0), p.text);
        }
        if !dropped.is_empty() {
            let paths: Vec<PathBuf> = dropped.iter().map(|f| f.path().to_path_buf()).collect();
            // Un graphe de nodes ouvert garde les images glissées (nodes texture)
            if self.view() == View::Files && self.current().is_some_and(|d| matches!(d.content, Content::Nodes(_))) {
                return;
            }
            // Une scène 3D ouverte reçoit les modèles glissés : ils y sont ajoutés, pas importés
            // Un montage ouvert reçoit les médias glissés : ils deviennent des clips
            if self.view() == View::Files {
                if let Some(Content::Montage(m)) = self.active.and_then(|i| self.docs.get_mut(i)).map(|d| &mut d.content) {
                    m.add.extend(paths);
                    return;
                }
            }
            let for_scene = |f: &PathBuf| {
                let n = f.to_string_lossy().to_lowercase();
                model3d::is_model(&n) || [".png", ".jpg", ".jpeg", ".webp"].iter().any(|e| n.ends_with(e))
            };
            if self.view() == View::Files && paths.iter().any(for_scene) {
                if let Some(Content::Model(v)) = self.active.and_then(|i| self.docs.get_mut(i)).map(|d| &mut d.content) {
                    if v.is_scene() {
                        v.scene_add.extend(paths);
                        return;
                    }
                }
            }
            // Dans le chat (ou sur le panneau de l'assistant) : les fichiers deviennent des pièces jointes.
            let over_assistant = self.assistant_open && ctx.input(|i| i.pointer.hover_pos()).is_some_and(|pos| pos.x > ctx.content_rect().right() - 360.0);
            if self.view() == View::Chat || over_assistant {
                let chat = if over_assistant { &mut self.assistant } else { &mut self.chat };
                for f in &paths {
                    chat.attachments.extend(chat::Attachment::from_path(f));
                }
                return;
            }
            self.import_paths(paths);
        }
    }

    /// Importe des fichiers ou dossiers (glissés, choisis, collés) selon le réglage d'import.
    fn import_paths(&mut self, paths: Vec<PathBuf>) {
        let dir = self.target_dir(if self.entries.iter().any(|e| e.rel == "refs") { "refs" } else { "" });
        let root = self.root();
        let mut already = vec![];
        let mut outside = vec![];
        for f in paths {
            // Déjà dans le coffre : on l'utilise tel quel, sans copie.
            match vault::inside(&root, &f) {
                Some(rel) => already.push(rel),
                None => outside.push(f),
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

    /// « importer… » : sélecteur de fichiers (ou de dossier) du système.
    fn pick_import(&mut self, folder: bool) {
        let picked = if folder {
            rfd::FileDialog::new().set_title("Lier un dossier").pick_folder().map(|f| vec![f])
        } else {
            rfd::FileDialog::new().set_title("Importer des fichiers").pick_files()
        };
        if let Some(files) = picked {
            self.import_paths(files);
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
                ui.label(RichText::new(crate::i18n::t("aryadne")).size(30.0).family(theme::bold()));
                ui.label(RichText::new(crate::i18n::t("// le fil entre tes idées")).color(p.muted));
                ui.add_space(22.0);
                let btn = egui::Button::new(RichText::new(crate::i18n::t("ouvrir ou créer un coffre")).color(p.on_accent).family(theme::bold()))
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
        self.cloud_tick();

        // Sauvegarde auto 500 ms après la dernière frappe, et à la fermeture.
        let closing = ctx.input(|i| i.viewport().close_requested());
        if closing {
            lsp::shutdown_all();
        }
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
        self.poll_backup();
        self.poll_url();
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
                self.zones_last = std::mem::take(&mut self.zones_next);
                self.ws_start(ui);
                let a = ui.available_rect_before_wrap();
                self.topbar(ui);
                let b = ui.available_rect_before_wrap();
                self.ws_bar(ui);
                let c = ui.available_rect_before_wrap();
                self.statusbar(ui);
                let d = ui.available_rect_before_wrap();
                self.zones_next.push(("top", egui::Rect::from_min_max(a.min, egui::pos2(a.max.x, b.min.y))));
                self.zones_next.push(("pages", egui::Rect::from_min_max(b.min, egui::pos2(b.max.x, c.min.y))));
                self.zones_next.push(("status", egui::Rect::from_min_max(egui::pos2(c.min.x, d.max.y), c.max)));
                self.explorer_rect = None;
                if self.explorer_open {
                    self.explorer(ui);
                    if crate::dock::closed("explorer") {
                        self.explorer_open = false;
                    }
                }
                if let Some(r) = self.explorer_rect {
                    self.zones_next.push(("explorer", r));
                }
                self.view_windows(ui);
            }
            if self.terminal.is_some() {
                self.terminal_panel(ui);
            }
            if self.assistant_open {
                let a = ui.available_rect_before_wrap();
                self.assistant_panel(ui);
                let b = ui.available_rect_before_wrap();
                self.zones_next.push(("assistant", egui::Rect::from_min_max(egui::pos2(b.max.x, a.min.y), a.max)));
            }
            match self.view() {
                View::Files => {
                    // Le panneau de droite (liens, résumé) sert aux textes ; la 3D, les vidéos
                    // et les documents gardent toute la largeur.
                    let text = self.current().is_none_or(|d| matches!(d.content, Content::Text(_)));
                    if !self.compact && !self.assistant_open && text && self.split.is_none() && !self.side_hidden {
                        let a = ui.available_rect_before_wrap();
                        self.side_panel(ui);
                        let b = ui.available_rect_before_wrap();
                        self.zones_next.push(("side", egui::Rect::from_min_max(egui::pos2(b.max.x, a.min.y), a.max)));
                    }
                    self.editor(ui);
                }
                View::Search => self.search_view(ui),
                View::Brain => self.brain_view(ui),
                View::Refs => self.refs_view(ui),
                View::Boards => self.boards_view(ui),
                View::Chat => self.chat_view(ui),
                View::Tasks => self.tasks_view(ui),
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
            self.palette_ui(&ctx);
            self.template_ui(&ctx);
            self.history_ui(&ctx);
            if let Some(d) = &mut self.convert {
                if !d.ui(&ctx) {
                    self.convert = None;
                    self.refresh();
                }
            }
            self.url_ui(&ctx);
            self.stats_ui(&ctx);
            self.dialogs(&ctx);
            self.run_actions(&ctx);
        }

        if let Some(e) = self.error.clone() {
            let p = theme::pal();
            let r = egui::Modal::new(egui::Id::unique("error")).frame(egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.missing)).inner_margin(16)).show(&ctx, |ui| {
                ui.set_max_width(440.0);
                ui.label(caps(&crate::i18n::t("erreur")));
                ui.label(RichText::new(&e).color(p.missing));
                ui.add_space(6.0);
                ui.button(crate::i18n::t("ok")).clicked()
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
/// Renvoie vrai si le modèle choisi a changé.
fn ollama_panel(ui: &mut egui::Ui, local: &ollama::Local, s: &mut ai::Settings, ctx: &egui::Context) -> bool {
    let p = theme::pal();
    let st = local.snapshot();
    let mut changed = false;
    ui.label(caps(&crate::i18n::t("ia locale · ollama")));
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
    // Modèles installés
    let ram = ollama::total_ram_gb();
    if !st.models.is_empty() {
        ui.add_space(6.0);
        ui.label(caps(&crate::i18n::t("modèles installés")));
        let mut delete = None;
        for m in &st.models {
            let gb = st.sizes.get(m).copied().unwrap_or(0) as f32 / 1e9;
            let active = *m == s.model || m.trim_end_matches(":latest") == s.model;
            ui.horizontal(|ui| {
                let name = RichText::new(m).family(if active { theme::bold() } else { egui::FontFamily::Monospace });
                ui.label(if active { name.color(p.accent) } else { name });
                ui.label(RichText::new(format!("{gb:.1} Go")).color(p.muted).size(11.0));
                if active {
                    ui.label(RichText::new(crate::i18n::t("utilisé")).color(p.accent).size(11.0));
                } else if ui.small_button(crate::i18n::t("utiliser")).clicked() {
                    s.model = m.trim_end_matches(":latest").to_string();
                    changed = true;
                }
                if ui.small_button(crate::i18n::t("supprimer")).on_hover_text(crate::i18n::t("Libère l'espace disque")).clicked() {
                    delete = Some(m.clone());
                }
            });
        }
        if let Some(m) = delete {
            local.delete(&s.url, &m, ctx);
        }
    }
    // Télécharger n'importe quel modèle
    ui.add_space(8.0);
    ui.label(caps(&crate::i18n::t("télécharger un modèle")));
    let id = egui::Id::unique("pull-name");
    let mut name: String = ui.data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
    let busy = st.task.is_some();
    ui.horizontal(|ui| {
        let r = ui.add(egui::TextEdit::singleline(&mut name).hint_text(crate::i18n::t("qwen3.8:27b  ou  hf.co/utilisateur/modèle-GGUF")).desired_width(320.0));
        let go = (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) || ui.add_enabled(!busy && !name.trim().is_empty(), egui::Button::new(crate::i18n::t("télécharger"))).clicked();
        if go && !busy && !name.trim().is_empty() {
            local.pull(&s.url, name.trim(), ctx);
            s.model = name.trim().to_string();
            changed = true;
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(crate::i18n::t("suggestions :")).color(p.muted).size(11.0));
        for (m, gb) in [("qwen3.8:27b", 18.0), ("qwen3.8", 18.0), ("llama3.2", 2.0), ("gemma3", 3.3)] {
            if ui.small_button(format!("{m} (~{gb} Go)")).clicked() {
                name = m.to_string();
            }
        }
    });
    ui.label(RichText::new(crate::i18n::t("N'importe quel nom de ollama.com/library, ou un GGUF de Hugging Face avec hf.co/…  Il faut à peu près la taille du modèle en mémoire (RAM ou carte graphique), plus 2 à 4 Go.")).color(p.muted).size(10.5));
    if let Some(r) = ram {
        ui.label(RichText::new(format!("mémoire de cet ordinateur : {r:.0} Go")).color(p.muted).size(10.5));
    }
    ui.data_mut(|d| d.insert_temp(id, name));
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
        return changed;
    }
    ui.horizontal(|ui| {
        if st.checked && st.binary.is_none() && !st.running {
            if ui.button(crate::i18n::t("installer ollama (sans admin)")).clicked() {
                local.install(&s.url, ctx);
            }
        }
        if st.binary.is_some() && !st.running && ui.button(crate::i18n::t("démarrer")).clicked() {
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
        if st.binary.is_some() && ui.button(crate::i18n::t("mettre à jour")).on_hover_text(crate::i18n::t("Retélécharge la dernière version d'Ollama")).clicked() {
            local.install(&s.url, ctx);
        }
        if ui.small_button("↻").on_hover_text(crate::i18n::t("Revérifier")).clicked() {
            local.refresh(&s.url, ctx);
        }
    });
    if st.checked && st.binary.is_none() && !st.running {
        ui.label(RichText::new(format!("~2 Go, installé dans {}", ollama::install_dir().to_string_lossy())).color(p.muted).size(10.5));
    }
    if let Some(e) = &st.error {
        ui.label(RichText::new(e).color(p.missing).size(11.5));
    }
    changed
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
            ui.label(RichText::new(crate::i18n::t("sources :")).color(p.muted).size(11.0));
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
