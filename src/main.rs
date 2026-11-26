#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cloud;
mod rig;
mod scopes;
mod story;
mod story_ui;
mod fountain;
mod sculpt;
mod multires;
mod matchmove;
mod nla;
mod autorig;
mod transcribe;
mod expr;
mod bases;
mod vector;
mod vgeom;
mod colorize;
mod track2d;
mod gltfscene;
mod fbxscene;
mod spell;
mod sendto;
mod aitasks;
mod related;
mod lucide_all;
mod mdexport;
mod canvas;
mod mdextra;
mod sources;
mod text3d;
mod timetrack;
mod timenav;
mod paint3d;
mod mocap;
mod pbrphoto;
mod vector_ui;
mod keymap;
mod lucide;
mod hl;
mod palette;
mod bridge;
mod layout;
mod screens;
mod icons;
mod atelier;
mod oidn;
mod dock;
mod formats;
mod boardlive;
mod gizmo;
mod paint;
mod untitled;
mod meshedit;
mod montage;
mod montage_ui;
mod motion;
mod lottie;
mod audio;
mod audio_ui;
mod spectral;
mod music;
mod music_ui;
mod midi;
mod animtools;
mod curves;
mod psd;
mod keys;
mod remesh;
mod motion_ui;
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
mod nodeflow;
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
        let _ = crate::vault::atomic_write(f, value);
    }
}

fn load_config(name: &str) -> Option<String> {
    config(name).and_then(|f| std::fs::read_to_string(f).ok()).map(|s| s.trim().to_string())
}

/// Titre de section : en casse normale, demi-gras (pas de petites capitales grises).
fn caps(text: &str) -> RichText {
    let mut t = text.to_string();
    if let Some(f) = t.get(0..1) {
        let up = f.to_uppercase();
        t.replace_range(0..1, &up);
    }
    RichText::new(t).size(12.0).color(theme::pal().text).family(theme::bold())
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

fn view_name(v: View) -> &'static str {
    match v {
        View::Files => "files",
        View::Brain => "brain",
        View::Refs => "refs",
        View::Boards => "boards",
        View::Tasks => "tasks",
        View::Chat => "chat",
        View::Search => "search",
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
    /// Composition animée (.arymotion)
    Motion(Box<motion_ui::Editor>),
    /// Session audio multipiste (.aryaudio)
    Audio(Box<audio_ui::Editor>),
    /// Morceau du séquenceur (.arymusic)
    Music(Box<music_ui::Editor>),
    Nodes(Box<nodes::Editor>),
    /// Image à calques (.aryimg)
    Paint(Box<paint::Editor>),
    Vector(Box<vector_ui::Editor>),
    Story(Box<story_ui::Editor>),
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
    /// Ouvrir une note et aller à un de ses titres
    OpenHeading(String, String),
    /// Renommer ou retirer une propriété dans toutes les notes
    RenameProp(String, Option<String>),
    /// Changer une propriété d'une note (sa valeur) : glisser une carte de kanban
    SetPropOf(String, String, String),
    NewFile(String),
    NewDir(String),
    Rename(String),
    Trash(String),
    Move(String, String),
    Close(usize),
    Summarize,
    /// Tâche d'IA sur le fichier actif : « edit », « proof », « props », « explain »
    AiTask(&'static str),
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
    mentions: Vec<vault::Mention>,
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
    /// Dernières commandes lancées depuis la palette (les plus récentes d'abord)
    cmd_recent: Vec<String>,
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
    /// Accueil : ouvert par-dessus les fichiers (menu ◆ Aryadne), filtre des récents, question à l'Assistant
    home_open: bool,
    home_filter: u8,
    home_ask: String,
    /// Nom tapé dans le menu Disposition
    disp_new: String,
    /// Raccourcis à jouer au début de l'image suivante (les entrées de la barre de menus passent par là)
    inject: Vec<(egui::Modifiers, egui::Key)>,
    /// Le prochain affichage de l'Assistant envoie le message déjà écrit (venu de l'accueil)
    chat_autosend: bool,
    /// Inspecteur du board : réglages du générateur de palettes, onglet (0 couleurs, 1 propriétés)
    pal_gen: palette::Settings,
    board_tab: u8,
    /// Diaporama du board (croquis de pose) : images, index, secondes par image, début de l'image, en pause depuis
    slideshow: Option<Slideshow>,
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
    /// Onglet de l'inspecteur : 0 plan, 1 liens, 2 Assistant
    insp_tab: u8,
    /// Cerveau : les notes colorées par tag
    brain_tags: bool,
    /// Temps passé par fichier (et sa fenêtre de bilan : 0 aujourd'hui, 1 7 jours, 2 30 jours, 3 tout)
    time: timetrack::Tracker,
    time_report: Option<u8>,
    view_panels: Vec<String>,
    /// Zones de l'interface (image précédente) : la barre du bas montre les raccourcis de celle sous la souris
    zones_last: Vec<(&'static str, egui::Rect)>,
    zones_next: Vec<(&'static str, egui::Rect)>,
    /// Fenêtre « nouveau fichier » : genre choisi et nom tapé
    new_file: Option<(usize, String)>,
    /// Explorateur : éléments choisis en plus de `selected` (Ctrl + clic, Maj + clic)
    multi: Vec<String>,
    /// L'explorateur a reçu le dernier clic : F2, Suppr et Ctrl+Z s'adressent à lui
    explorer_focus: bool,
    explorer_rect: Option<egui::Rect>,
    /// Disposition en cases de l'espace en cours
    tree: Option<layout::Layout>,
    /// Panneaux d'éditeur proposés à l'image précédente (case → titre)
    offered_now: HashMap<String, String>,
    /// Panneaux d'éditeur à ranger dans la disposition (vrai) ou à rendre à l'éditeur (faux)
    dock_editor_panes: Vec<(String, bool)>,
    /// Demandé depuis le menu d'un onglet
    reset_layout: bool,
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
        let prefs = settings::Prefs::load();
        theme::install_fonts(ctx, &prefs.fonts());
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
        let (vault_arg, ws_arg) = workspace::launch_args();
        if let Some(i) = ws_arg {
            // Fenêtre secondaire : cet espace-là, sans toucher aux réglages de la principale
            workspace::SECONDARY.store(true, std::sync::atomic::Ordering::Relaxed);
            if i < app.ws.list.len() {
                app.ws.current = i;
            }
        }
        if let Some(p) = vault_arg.or_else(|| load_config("last_vault")) {
            app.load_vault(p);
        }
        app
    }

    fn view(&self) -> View {
        self.view.unwrap_or(View::Files)
    }

    /// L'outil ouvert dans l'étape en cours (le premier si rien n'est choisi)
    fn cur_tool(&self) -> &'static str {
        let w = self.ws.cur();
        let tools = workspace::tools(&w.kind);
        // Une page d'outil ouverte (moodboard, tâches, cerveau) : c'est cet outil
        let v = view_name(self.view());
        if v != "files" {
            if let Some(t) = tools.iter().find(|t| t.view == v) {
                return t.id;
            }
        }
        // Sinon l'outil à fichiers choisi, ou le premier
        tools.iter().find(|t| t.id == w.tool && t.view == "files").or(tools.iter().find(|t| t.view == "files")).map(|t| t.id).unwrap_or("")
    }

    /// Choisir un outil de l'étape : sa page, et son dernier fichier ouvert
    fn set_tool(&mut self, id: &'static str) {
        let kind = self.ws.cur().kind.clone();
        let Some(t) = workspace::tools(&kind).iter().find(|t| t.id == id) else { return };
        let cur = self.ws.current;
        self.ws.list[cur].tool = id.to_string();
        self.home_open = false;
        self.view = Some(view_of(t.view));
        if t.view == "files" {
            if let Some(j) = (0..self.docs.len()).rev().find(|&j| workspace::tool_of(&self.docs[j].rel) == id) {
                self.active = Some(j);
            }
        }
        self.ws.save();
    }

    /// Ce fichier se montre-t-il dans l'outil en cours ? (une étape à un seul outil montre tout)
    fn in_tool(&self, rel: &str) -> bool {
        let kind = &self.ws.cur().kind;
        workspace::tools(kind).iter().filter(|t| t.view == "files").count() < 2 || workspace::tool_of(rel) == self.cur_tool()
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
        w.layout = self.tree.clone();
    }

    /// Met en place l'espace en cours : panneaux, couleur, onglets, vue.
    fn ws_apply(&mut self, ctx: &egui::Context, tabs: bool) {
        let w = self.ws.cur().clone();
        dock::import(w.places);
        self.explorer_open = w.explorer;
        self.view_panels = w.views.clone();
        self.tree = w.layout.clone();
        theme::set_workspace_accent(workspace::accent_of(self.ws.cur()));
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
        self.view = Some(view_of(if workspace::has_view(&w.kind, &w.view) { w.view.as_str() } else { pages[0].1 }));
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

    // ---------- dispositions de l'étape ----------

    /// Nom de la disposition en cours (« Principale » tant qu'on n'en a pas nommé)
    fn disp_name(&self) -> String {
        let w = self.ws.cur();
        if w.disposition.is_empty() { crate::i18n::t("Principale") } else { w.disposition.clone() }
    }

    /// Range l'arrangement actuel des panneaux dans la disposition en cours
    fn disp_store(&mut self) {
        self.ws_capture();
        let name = self.disp_name();
        let w = &mut self.ws.list[self.ws.current];
        let d = workspace::Disposition { name: name.clone(), places: w.places.clone(), layout: w.layout.clone(), explorer: w.explorer, views: w.views.clone() };
        match w.dispositions.iter_mut().find(|x| x.name == name) {
            Some(x) => *x = d,
            None => w.dispositions.push(d),
        }
        w.disposition = name;
    }

    /// Passer à une autre disposition de l'étape (les fichiers ouverts restent)
    fn disp_switch(&mut self, name: &str, ctx: &egui::Context) {
        self.disp_store();
        let w = &mut self.ws.list[self.ws.current];
        let Some(d) = w.dispositions.iter().find(|x| x.name == name).cloned() else { return };
        w.places = d.places;
        w.layout = d.layout;
        w.explorer = d.explorer;
        w.views = d.views;
        w.disposition = d.name;
        dock::reset();
        self.ws_apply(ctx, false);
        self.ws.save();
    }

    /// Menu « Disposition » à droite des onglets : changer, enregistrer sous un nom, renommer, réinitialiser
    fn disp_menu(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let ctx = ui.ctx().clone();
        let cur = self.disp_name();
        let names: Vec<String> = {
            let w = self.ws.cur();
            let mut v: Vec<String> = w.dispositions.iter().map(|d| d.name.clone()).collect();
            if !v.contains(&cur) {
                v.insert(0, cur.clone());
            }
            v
        };
        let mut go: Option<String> = None;
        let mut act: u8 = 0;
        // Le menu reste ouvert quand on clique dans le champ du nom (il se ferme par ses boutons, ou à côté)
        let stay = egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
        egui::containers::menu::MenuButton::new(RichText::new(format!("{cur} {}", lucide::CHEVRON_DOWN)).size(12.5).color(p.text)).config(stay).ui(ui, |ui| {
            self.disp_items(ui, &names, &cur, &mut go, &mut act);
        });
        self.disp_apply(go, act, &cur, &ctx);
    }

    /// Le contenu du menu des dispositions (aussi dans la barre de menus)
    fn disp_items(&mut self, ui: &mut egui::Ui, names: &[String], cur: &str, go: &mut Option<String>, act: &mut u8) {
        let p = theme::pal();
            ui.set_min_width(240.0);
            ui.label(RichText::new(crate::i18n::t("Dispositions de l'étape")).size(11.0).color(p.muted));
            for n in names {
                if ui.selectable_label(*n == *cur, n).clicked() && *n != *cur {
                    *go = Some(n.clone());
                    ui.close();
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.disp_new).hint_text(crate::i18n::t("Modelage, Animation…")).desired_width(150.0));
                if ui.add_enabled(!self.disp_new.trim().is_empty(), egui::Button::new(format!("{}  {}", crate::lucide::SAVE, crate::i18n::t("Enregistrer")))).on_hover_text(crate::i18n::t("Garde l'arrangement actuel des panneaux sous ce nom")).clicked() {
                    *act = 1;
                    ui.close();
                }
            });
            if ui.button(format!("{}  {}", crate::lucide::PENCIL, crate::i18n::t("Renommer en ce nom"))).on_hover_text(crate::i18n::t("Donne le nom tapé ci-dessus à la disposition en cours")).clicked() && !self.disp_new.trim().is_empty() {
                *act = 2;
                ui.close();
            }
            if ui.button(format!("{}  {}", crate::lucide::ROTATE_CCW, crate::i18n::t("Réinitialiser cette disposition"))).clicked() {
                *act = 3;
                ui.close();
            }
            if names.len() > 1 && ui.button(RichText::new(crate::i18n::t("Supprimer cette disposition")).color(p.missing)).clicked() {
                *act = 4;
                ui.close();
            }
            ui.separator();
            if ui.button(format!("{}  {}", crate::lucide::FOLDER_OPEN, crate::i18n::t("Ouvrir l'étape dans une nouvelle fenêtre"))).on_hover_text(crate::i18n::t("Par exemple les matériaux sur un écran et la scène sur l'autre")).clicked() {
                if let Err(e) = workspace::open_in_window(&self.root(), self.ws.current) {
                    self.error = Some(e);
                }
                ui.close();
            }
    }

    fn disp_apply(&mut self, go: Option<String>, act: u8, cur: &str, ctx: &egui::Context) {
        let ctx = ctx.clone();
        let cur = cur.to_string();
        if let Some(n) = go {
            self.disp_switch(&n, &ctx);
        }
        match act {
            1 => {
                // La disposition en cours est gardée telle quelle, la nouvelle part de l'arrangement actuel
                self.disp_store();
                let name = std::mem::take(&mut self.disp_new).trim().to_string();
                let w = &mut self.ws.list[self.ws.current];
                let mut d = w.dispositions.iter().find(|x| x.name == w.disposition).cloned().unwrap_or_default();
                d.name = name.clone();
                w.dispositions.retain(|x| x.name != name);
                w.dispositions.push(d);
                w.disposition = name;
                self.ws.save();
            }
            2 => {
                let name = std::mem::take(&mut self.disp_new).trim().to_string();
                self.disp_store();
                let w = &mut self.ws.list[self.ws.current];
                if !w.dispositions.iter().any(|x| x.name == name) {
                    if let Some(d) = w.dispositions.iter_mut().find(|x| x.name == cur) {
                        d.name = name.clone();
                    }
                    w.disposition = name;
                }
                self.ws.save();
            }
            3 => {
                let i = self.ws.current;
                self.ws.list[i].places.clear();
                self.ws.list[i].layout = None;
                dock::reset();
                self.tree = None;
                self.disp_store();
                self.ws.save();
            }
            4 => {
                let i = self.ws.current;
                self.ws.list[i].dispositions.retain(|x| x.name != cur);
                if let Some(n) = self.ws.list[i].dispositions.first().map(|d| d.name.clone()) {
                    self.ws.list[i].disposition = String::new();
                    // La première qui reste prend la place
                    let w = &mut self.ws.list[i];
                    if let Some(d) = w.dispositions.iter().find(|x| x.name == n).cloned() {
                        w.places = d.places;
                        w.layout = d.layout;
                        w.explorer = d.explorer;
                        w.views = d.views;
                        w.disposition = d.name;
                    }
                    dock::reset();
                    self.ws_apply(&ctx, false);
                }
                self.ws.save();
            }
            _ => {}
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
                        if ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("annuler"))).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
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
                "aryvec" => self.new_untitled(ext, |p| vector::Doc::starter().save(p)),
                "arystory" => self.new_untitled(ext, |p| story::Story::starter().save(p)),
                "aryimg" => {
                    let (_, w, h) = NF_IMG[o.img_size];
                    let white = o.img_white;
                    self.new_untitled(ext, move |p| paint::Doc::new(w, h, white).save(p));
                }
                "code" => self.new_untitled(NF_CODE[o.code].0, |p| crate::vault::atomic_write(p, "").map_err(|e| e.to_string())),
                _ => self.new_untitled(ext, |p| crate::vault::atomic_write(p, "").map_err(|e| e.to_string())),
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
            "arymotion" => "motion".to_string(),
            "aryaudio" => "son".to_string(),
            "arymusic" => "son".to_string(),
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

    /// Ctrl+Maj+S : une copie du fichier ouvert, à l'endroit et sous le nom choisis ; la copie s'ouvre, l'original ne change pas.
    fn save_copy_as(&mut self, i: usize) {
        let Some(rel) = self.docs.get(i).map(|d| d.rel.clone()) else { return };
        if untitled::is_untitled(&rel) {
            self.save_as(i);
            return;
        }
        self.flush_all();
        let root = self.root();
        let src = std::path::Path::new(&root).join(&rel);
        let e = ext(&rel);
        let model = matches!(self.docs.get(i).map(|d| &d.content), Some(Content::Model(_))) && !crate::scene::is_scene(&src);
        let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let mut d = rfd::FileDialog::new().set_title(crate::i18n::t("enregistrer sous")).set_file_name(format!("{stem} copie.{e}"));
        if let Some(dir) = src.parent() {
            d = d.set_directory(dir);
        }
        if model {
            // Un modèle peut changer de format en route
            for f in crate::modifiers::EXPORT_FORMATS {
                d = d.add_filter(f.to_uppercase(), &[*f]);
            }
        } else if !e.is_empty() {
            d = d.add_filter(&e, &[e.as_str()]);
        }
        let Some(mut dest) = d.save_file() else { return };
        if dest.extension().is_none() && !e.is_empty() {
            dest.set_extension(&e);
        }
        if dest == src {
            return;
        }
        if model {
            if let Some(Content::Model(v)) = self.docs.get_mut(i).map(|d| &mut d.content) {
                v.save_request = Some(Some(dest));
            }
            return;
        }
        if let Err(err) = std::fs::copy(&src, &dest) {
            self.error = Some(err.to_string());
            return;
        }
        self.refresh();
        match vault::inside(&root, &dest) {
            Some(new_rel) => {
                self.open(new_rel.clone());
                self.toast(format!("{} {new_rel}", crate::i18n::t("enregistré sous")));
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
                ui.label(format!("« {} » {}", untitled::name(&rel), crate::i18n::t_mid("n'a pas encore de nom ni de place dans le coffre.")));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new(crate::i18n::t("enregistrer…")).color(p.on_accent)).fill(p.accent)).clicked() {
                        choice = 1;
                    }
                    if ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("ne pas enregistrer"))).clicked() {
                        choice = 2;
                    }
                    if ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("annuler"))).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
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
                    if ui.button(format!("{}  {}", crate::lucide::TRASH_2, crate::i18n::t("les supprimer"))).clicked() {
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
    /// Accueil, comme la maquette : reprendre, commencer (modèles), récents en vignettes, et à droite
    /// l'Assistant, les raccourcis utiles et les nouveautés. S'affiche quand aucun fichier n'est ouvert,
    /// ou depuis le menu ◆ Aryadne.
    /// Colonne de l'accueil : Assistant, raccourcis, nouveautés
    /// Un outil de l'étape sans fichier ouvert : ce qu'il fait, de quoi commencer, ses fichiers récents
    fn tool_landing(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let ctx = ui.ctx().clone();
        let kind = self.ws.cur().kind.clone();
        let tool = self.cur_tool();
        let Some(t) = workspace::tools(&kind).iter().find(|t| t.id == tool) else { return };
        let accent = workspace::accent_of(self.ws.cur()).map(|c| egui::Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(p.accent);
        let (about, actions): (&str, Vec<(&str, extras::Cmd)>) = match tool {
            "image" => ("Peinture et retouche à calques, comme Photoshop. Ouvre une image, ou crée une image à calques.", vec![("Nouvelle image à calques", extras::Cmd::NewFile), ("Importer des images…", extras::Cmd::Import)]),
            "vector" => ("Logos, illustrations et maquettes d'écrans en vecteurs, comme Illustrator et Figma : plume, formes, textes, booléens, plans de travail exportés en SVG ou PNG.", vec![("Nouveau dessin vectoriel", extras::Cmd::NewVector)]),
            "comp" => ("Retoucher une image en nodes : couleurs, flous, masques, et l'étalonnage des clips du montage.", vec![("Nouveau compositing", extras::Cmd::NewCompositor)]),
            "scene" => ("Modèles, lumières, caméras et animation. Un modèle ouvert seul arrive dans une scène toute prête.", vec![("Nouvelle scène 3D", extras::Cmd::NewScene), ("Importer des modèles…", extras::Cmd::Import)]),
            "mat" => ("Les matériaux des modèles : en nodes, ou à calques comme Substance.", vec![("Nouveau matériau (nodes)", extras::Cmd::NewMaterial), ("Nouveau matériau à calques", extras::Cmd::NewLayered)]),
            "music" => ("Composer avec des synthés et une batterie intégrés : régions, rouleau de piano, idées d'accords et de rythmes.", vec![("Nouveau morceau", extras::Cmd::NewMusic)]),
            "audio" => ("Voix, musiques et ambiances sur plusieurs pistes : couper, nettoyer, mixer, exporter.", vec![("Nouvelle session audio", extras::Cmd::NewAudio), ("Importer des sons…", extras::Cmd::Import)]),
            "motion" => ("Titres, formes et images animés par clés, avec des préréglages, puis exportés en MP4.", vec![("Nouvelle composition", extras::Cmd::NewMotion)]),
            "story" => ("Dessiner le film case par case : plan, caméra, action et dialogue, puis lire l'animatique et l'envoyer au montage.", vec![("Nouveau storyboard", extras::Cmd::NewStory)]),
            "seq" => ("Assembler des plans, des titres et des sons sur une timeline magnétique.", vec![("Nouveau montage", extras::Cmd::NewMontage), ("Importer des fichiers…", extras::Cmd::Import)]),
            "review" => ("Regarder un plan image par image, l'annoter et le commenter, comme SyncSketch.", vec![("Importer une vidéo…", extras::Cmd::Import)]),
            "code" => ("Scripts, shaders et pages, avec l'exécution et l'aide d'un serveur de langage.", vec![("Nouveau fichier", extras::Cmd::NewFile)]),
            _ => ("Écris une note, ou rouvre une note récente.", vec![("Nouvelle note", extras::Cmd::NewNote), ("Note du jour", extras::Cmd::Daily)]),
        };
        let mut rec: Vec<(String, u64)> = self.entries.iter().filter(|e| !e.is_dir && workspace::owner(&e.rel) == kind && workspace::tool_of(&e.rel) == tool).map(|e| (e.rel.clone(), e.modified)).collect();
        rec.sort_by(|a, b| b.1.cmp(&a.1));
        rec.truncate(8);
        let mut run = None;
        let mut open = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.12).max(30.0));
                ui.set_max_width(640.0);
                ui.label(RichText::new(t.icon).size(34.0).color(accent));
                ui.add_space(6.0);
                ui.label(RichText::new(crate::i18n::t(t.label)).size(24.0).family(theme::bold()));
                ui.add_space(6.0);
                ui.label(RichText::new(crate::i18n::t(about)).color(p.muted));
                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    let total = actions.len() as f32 * 220.0;
                    ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
                    for (k, (label, cmd)) in actions.iter().enumerate() {
                        let text = RichText::new(crate::i18n::t(label)).color(if k == 0 { p.on_accent } else { p.text });
                        let b = if k == 0 { egui::Button::new(text).fill(accent) } else { egui::Button::new(text) };
                        if ui.add(b.min_size(egui::vec2(200.0, 32.0))).clicked() {
                            run = Some(*cmd);
                        }
                    }
                });
                if !rec.is_empty() {
                    ui.add_space(28.0);
                    ui.label(RichText::new(crate::i18n::t("Récents")).size(13.0).family(theme::bold()));
                    ui.add_space(8.0);
                    for (rel, _) in &rec {
                        let (icon, col, _) = files::kind(rel);
                        let mut job = egui::text::LayoutJob::default();
                        job.append(&format!("{icon}  "), 0.0, egui::TextFormat { font_id: egui::FontId::proportional(13.0), color: col, ..Default::default() });
                        job.append(rel.rsplit('/').next().unwrap_or(rel), 0.0, egui::TextFormat { font_id: egui::FontId::proportional(13.0), color: p.text, ..Default::default() });
                        let dir = vault::parent(rel);
                        if !dir.is_empty() {
                            job.append(&format!("   {dir}"), 0.0, egui::TextFormat { font_id: egui::FontId::proportional(11.0), color: p.muted, ..Default::default() });
                        }
                        if ui.add(egui::Button::new(job).frame(false)).clicked() {
                            open = Some(rel.clone());
                        }
                    }
                }
            });
        });
        if let Some(c) = run {
            self.run_cmd(c, &ctx);
        }
        if let Some(r) = open {
            self.open(r);
        }
    }

    fn home_side(&mut self, ui: &mut egui::Ui, ask: &mut Option<String>) {
        let p = theme::pal();
        let card = |ui: &mut egui::Ui, body: &mut dyn FnMut(&mut egui::Ui)| {
            egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(16).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 8.0;
                body(ui);
            });
            ui.add_space(20.0);
        };
        card(ui, &mut |ui| {
            ui.label(RichText::new(format!("{}  {}", lucide::SPARKLES, crate::i18n::t("Demander à l'Assistant"))).family(theme::bold()));
            ui.label(RichText::new(crate::i18n::t("Il voit ton coffre et peut t'aider : ranger, résumer, proposer une palette. Tu confirmes avant chaque changement.")).size(12.5).color(p.muted));
            let r = ui.add(egui::TextEdit::singleline(&mut self.home_ask).hint_text(crate::i18n::t("Range mes téléchargements par projet…")).desired_width(f32::INFINITY).min_size(egui::vec2(0.0, 30.0)));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.home_ask.trim().is_empty() {
                *ask = Some(std::mem::take(&mut self.home_ask));
            }
        });
        card(ui, &mut |ui| {
            ui.label(RichText::new(crate::i18n::t("Raccourcis utiles")).family(theme::bold()));
            for (label, key) in [("Tout chercher, tout lancer", "palette"), ("Ouvrir un fichier", "switcher"), ("Côte à côte", "split"), ("Assistant", "assistant"), ("Réglages", "settings")] {
                let k = keymap::text(key);
                if k.is_empty() {
                    continue;
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(crate::i18n::t(label)).size(12.5));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(k.split(" / ").next().unwrap_or("")).monospace().size(11.0).background_color(p.field));
                    });
                });
            }
        });
        card(ui, &mut |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} {}", crate::i18n::t("Nouveau dans"), env!("CARGO_PKG_VERSION"))).family(theme::bold()));
            });
            for line in ["Montage repensé : un moniteur, timeline magnétique, médias qu'on découpe sur la carte", "Générateur de palettes dans le moodboard", "Nouvelle interface : 3D, notes, matériaux et code comme la maquette", "Biseau, couteau et coupe en boucle à la souris, curseur 3D"] {
                ui.label(RichText::new(format!("•  {}", crate::i18n::t(line))).size(12.5).color(p.muted));
            }
        });
    }

    fn home(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let ctx = ui.ctx().clone();
        let mut run: Option<extras::Cmd> = None;
        let mut open: Option<String> = None;
        let mut reopen = false;
        let mut ask: Option<String> = None;
        let files = self.entries.iter().filter(|e| !e.is_dir).count();
        let tabs_n = self.ws.cur().tabs.len();
        let vault = std::path::Path::new(&self.root()).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let ago = |t: u64| -> String {
            let s = now.saturating_sub(t);
            if s < 3600 {
                crate::i18n::t_mid("il y a {}").replace("{}", &format!("{} min", (s / 60).max(1)))
            } else if s < 86_400 {
                crate::i18n::t_mid("il y a {}").replace("{}", &format!("{} h", s / 3600))
            } else if s < 2 * 86_400 {
                crate::i18n::t("hier")
            } else {
                crate::i18n::t_mid("il y a {}").replace("{}", &format!("{} {}", s / 86_400, crate::i18n::t_mid("j")))
            }
        };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let full = ui.available_width();
            let content_w = full.min(1180.0);
            ui.horizontal(|ui| {
                ui.add_space(((full - content_w) / 2.0).max(0.0) + 40.0);
                let side_w = if content_w > 900.0 { 300.0 } else { 0.0 };
                let main_w = content_w - 80.0 - if side_w > 0.0 { side_w + 40.0 } else { 0.0 };
                ui.vertical(|ui| {
                    ui.set_width(main_w);
                    ui.add_space(40.0);
                    // ---- Reprendre
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(crate::i18n::t("Reprendre où tu étais")).size(22.0).family(theme::bold()));
                            ui.label(RichText::new(format!("{} {vault} · {files} {}", crate::i18n::t("Coffre"), crate::i18n::t_mid("fichiers"))).color(p.muted));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if tabs_n > 0 && ui.add(egui::Button::new(RichText::new(format!("{} {tabs_n} {}", crate::i18n::t("Rouvrir les"), crate::i18n::t_mid("onglets"))).color(p.on_accent).family(theme::bold())).fill(p.accent).min_size(egui::vec2(0.0, 32.0))).clicked() {
                                reopen = true;
                            }
                            if ui.add(egui::Button::new(format!("{}  {}", crate::lucide::FOLDER_OPEN, crate::i18n::t("Ouvrir un autre coffre"))).min_size(egui::vec2(0.0, 32.0))).clicked() {
                                run = Some(extras::Cmd::ChangeVault);
                            }
                        });
                    });
                    ui.add_space(32.0);
                    // ---- Commencer : un modèle par genre de fichier, dessiné
                    ui.label(RichText::new(crate::i18n::t("Commencer")).size(15.0).family(theme::bold()));
                    ui.add_space(12.0);
                    let tpls: [(&str, &str, &str, extras::Cmd, u8); 7] = [
                        ("Note", "page vide", "notes", extras::Cmd::NewNote, 0),
                        ("Note du jour", "journal", "notes", extras::Cmd::Daily, 0),
                        ("Moodboard", "toile libre", "2d", extras::Cmd::NewBoard, 1),
                        ("Scène 3D", "cube, lumière, caméra", "3d", extras::Cmd::NewScene, 2),
                        ("Montage", "vidéo et son", "video", extras::Cmd::NewMontage, 3),
                        ("Matériau", "nodes", "mat", extras::Cmd::NewMaterial, 4),
                        ("Fichier de code", "script, shader, page", "code", extras::Cmd::NewFile, 5),
                    ];
                    let gap = 14.0;
                    let n = tpls.len() as f32;
                    let cw = ((main_w - gap * (n - 1.0)) / n).floor();
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for (name, sub, ws, cmd, shape) in tpls {
                            let col = theme::ws_color(ws).unwrap_or(p.accent);
                            let (r, resp) = ui.allocate_exact_size(egui::vec2(cw, cw * 0.75 + 40.0), egui::Sense::click());
                            let art = egui::Rect::from_min_size(r.min, egui::vec2(cw, cw * 0.75));
                            ui.painter().rect(art, 10.0, if resp.hovered() { p.raised } else { p.panel }, egui::Stroke::new(1.0, if resp.hovered() { col } else { p.line }), egui::StrokeKind::Inside);
                            let c = art.center();
                            let pt = ui.painter();
                            match shape {
                                0 => {
                                    for (k, w) in [0.9f32, 0.7, 0.85, 0.4].iter().enumerate() {
                                        pt.rect_filled(egui::Rect::from_min_size(egui::pos2(c.x - 30.0, c.y - 14.0 + k as f32 * 8.0), egui::vec2(60.0 * w, 3.0)), 1.0, p.muted);
                                    }
                                }
                                1 => {
                                    for (k, cc) in [theme::ws_color("2d"), theme::ws_color("3d"), theme::ws_color("mat"), theme::ws_color("notes")].iter().enumerate() {
                                        let o = egui::vec2((k % 2) as f32 * 22.0 - 11.0, (k / 2) as f32 * 22.0 - 11.0);
                                        pt.rect_filled(egui::Rect::from_center_size(c + o, egui::vec2(18.0, 18.0)), 3.0, cc.unwrap_or(p.accent).gamma_multiply(0.6));
                                    }
                                }
                                2 => {
                                    pt.text(c, egui::Align2::CENTER_CENTER, lucide::BOX, egui::FontId::proportional(34.0), col);
                                }
                                3 => {
                                    pt.rect_filled(egui::Rect::from_min_size(c + egui::vec2(-35.0, -12.0), egui::vec2(56.0, 8.0)), 2.0, col.gamma_multiply(0.6));
                                    pt.rect_filled(egui::Rect::from_min_size(c + egui::vec2(-14.0, -1.0), egui::vec2(38.0, 8.0)), 2.0, col.gamma_multiply(0.4));
                                    pt.rect_filled(egui::Rect::from_min_size(c + egui::vec2(-35.0, 10.0), egui::vec2(70.0, 6.0)), 2.0, theme::ws_color("code").unwrap_or(p.accent).gamma_multiply(0.5));
                                }
                                4 => {
                                    pt.circle_filled(c, 19.0, col.gamma_multiply(0.55));
                                    pt.circle_filled(c + egui::vec2(-5.0, -5.0), 12.0, col);
                                    pt.circle_filled(c + egui::vec2(-8.0, -8.0), 4.0, egui::Color32::from_white_alpha(140));
                                }
                                _ => {
                                    pt.text(c, egui::Align2::CENTER_CENTER, lucide::CODE, egui::FontId::proportional(32.0), col);
                                }
                            }
                            pt.circle_filled(egui::pos2(art.left() + 4.0, art.bottom() + 14.0), 3.5, col);
                            pt.text(egui::pos2(art.left() + 13.0, art.bottom() + 14.0), egui::Align2::LEFT_CENTER, crate::i18n::t(name), egui::FontId::proportional(12.5), p.text);
                            pt.text(egui::pos2(art.left(), art.bottom() + 30.0), egui::Align2::LEFT_CENTER, crate::i18n::t(sub), egui::FontId::proportional(11.0), p.muted);
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                run = Some(cmd);
                            }
                        }
                    });
                    ui.add_space(28.0);
                    // ---- Récents, avec filtres par genre
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(crate::i18n::t("Récents")).size(15.0).family(theme::bold()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            for (k, label) in [(6u8, "Code"), (5, "Notes"), (4, "Vidéo"), (3, "Matériaux"), (2, "2D"), (1, "3D"), (0, "Tout")] {
                                let on = self.home_filter == k;
                                let b = egui::Button::new(RichText::new(crate::i18n::t(label)).size(12.5).color(if on { p.text } else { p.muted })).fill(if on { p.raised } else { egui::Color32::TRANSPARENT }).stroke(egui::Stroke::new(1.0, if on { p.line } else { egui::Color32::TRANSPARENT }));
                                if ui.add(b).clicked() {
                                    self.home_filter = k;
                                }
                            }
                        });
                    });
                    ui.add_space(12.0);
                    let want = self.home_filter;
                    let mut rec: Vec<(String, u64)> = self.entries.iter().filter(|e| !e.is_dir).filter(|e| {
                        let (_, _, f) = files::kind(&e.rel);
                        match want {
                            1 => f == "3d",
                            2 => f == "images" || f == "boards",
                            3 => f == "nodes",
                            4 => f == "vidéos" || montage::is_project(&e.rel),
                            5 => f == "notes",
                            6 => f == "code",
                            _ => true,
                        }
                    }).map(|e| (e.rel.clone(), e.modified)).collect();
                    rec.sort_by(|a, b| b.1.cmp(&a.1));
                    let per_row = 6usize;
                    rec.truncate(per_row * 2);
                    let rw = ((main_w - gap * (per_row as f32 - 1.0)) / per_row as f32).floor();
                    for row in rec.chunks(per_row) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            for (rel, modified) in row {
                                let (icon, col, _) = files::kind(rel);
                                let (r, resp) = ui.allocate_exact_size(egui::vec2(rw, rw * 0.625 + 42.0), egui::Sense::click());
                                let art = egui::Rect::from_min_size(r.min, egui::vec2(rw, rw * 0.625));
                                ui.painter().rect_filled(art, 10.0, col.gamma_multiply(0.16));
                                if images::is_image(rel) {
                                    if let images::Slot::Ready(l) = self.images.get(rel, images::Size::Thumb) {
                                        ui.painter().add(egui::epaint::RectShape::filled(art, 10.0, egui::Color32::WHITE).with_texture(l.tex.id(), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))));
                                    }
                                } else {
                                    ui.painter().text(art.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(30.0), col.gamma_multiply(0.8));
                                }
                                ui.painter().rect_stroke(art, 10.0, egui::Stroke::new(1.0, if resp.hovered() { col } else { p.line }), egui::StrokeKind::Inside);
                                let name = rel.rsplit('/').next().unwrap_or(rel);
                                let clip = ui.painter().with_clip_rect(r);
                                clip.circle_filled(egui::pos2(art.left() + 4.0, art.bottom() + 14.0), 3.5, col);
                                clip.text(egui::pos2(art.left() + 13.0, art.bottom() + 14.0), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(12.5), p.text);
                                let dir = vault::parent(rel);
                                clip.text(egui::pos2(art.left(), art.bottom() + 31.0), egui::Align2::LEFT_CENTER, format!("{} · {}", if dir.is_empty() { "/".to_string() } else { dir }, ago(*modified)), egui::FontId::proportional(11.0), p.muted);
                                if resp.on_hover_text(rel).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    open = Some(rel.clone());
                                }
                            }
                        });
                        ui.add_space(14.0);
                    }
                    if rec.is_empty() {
                        ui.label(RichText::new(crate::i18n::t("Rien ici pour l'instant.")).color(p.muted));
                    }
                    ui.add_space(30.0);
                });
                // ---- Colonne de droite (sous le reste quand la place manque)
                if side_w > 0.0 {
                    ui.add_space(40.0);
                    ui.vertical(|ui| {
                        ui.set_width(side_w);
                        ui.add_space(40.0);
                        self.home_side(ui, &mut ask);
                    });
                }
            });
            if content_w <= 900.0 {
                ui.horizontal(|ui| {
                    ui.add_space(((full - content_w) / 2.0).max(0.0) + 40.0);
                    ui.vertical(|ui| {
                        ui.set_width((content_w - 80.0).min(520.0));
                        self.home_side(ui, &mut ask);
                    });
                });
            }
        });
        if reopen {
            self.home_open = false;
            self.ws_apply(&ctx, true);
        }
        if let Some(c) = run {
            self.home_open = false;
            self.run_cmd(c, &ctx);
        }
        if let Some(rel) = open {
            self.home_open = false;
            self.open(rel);
        }
        if let Some(q) = ask {
            self.chat.input = q;
            self.chat_autosend = true;
            if !self.assistant_open && !(self.side_wanted() && self.insp_tab == 2) {
                self.toggle_assistant();
            }
        }
    }

    fn ws_switch(&mut self, i: usize, ctx: &egui::Context) {
        if i >= self.ws.list.len() || i == self.ws.current {
            return;
        }
        self.ws_capture();
        self.ws.current = i;
        self.ws_apply(ctx, true);
        // Première visite d'une étape : son premier outil (la 2D s'ouvre sur le moodboard)
        if self.ws.cur().tool.is_empty() && !self.ws_routing {
            if let Some(t) = workspace::tools(&self.ws.cur().kind).first() {
                self.set_tool(t.id);
            }
        }
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
                self.toast = Some((format!("{old} {}", crate::i18n::t_mid("fichier(s) avec une ancienne extension : palette de commandes → « passer aux nouvelles extensions »")), Instant::now()));
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
        let mut del: Option<usize> = None;
        ui.spacing_mut().item_spacing.x = 2.0;
        let n = self.ws.list.len();
        for i in 0..n {
            let on = i == self.ws.current;
            let color = workspace::accent_of(&self.ws.list[i]).map(|c| egui::Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(if on { theme::theme().p.accent } else { p.muted });
            let label = crate::i18n::t(&self.ws.list[i].name);
            // Comme la maquette : pastille + nom ; l'onglet actif a un fond relevé, un contour et sa couleur en bas
            let galley = ui.painter().layout_no_wrap(label, egui::FontId::proportional(12.5), if on { p.text } else { p.muted });
            let size = egui::vec2(galley.size().x + 10.0 + 8.0 + 7.0 + 10.0, 28.0);
            let bg = ui.painter().add(egui::Shape::Noop);
            let (rect, r) = ui.allocate_exact_size(size, egui::Sense::click());
            let hov = r.hovered();
            // L'étape active montre ses outils dans son onglet (2D : Moodboard · Retouche · Compositing)
            let mut whole = rect;
            if on {
                let tools = workspace::tools(&self.ws.list[i].kind);
                if tools.len() > 1 {
                    let cur = self.cur_tool();
                    let mut pick: Option<&'static str> = None;
                    ui.add_space(0.0);
                    let (sep, _) = ui.allocate_exact_size(egui::vec2(9.0, 28.0), egui::Sense::hover());
                    ui.painter().vline(sep.center().x - 3.0, sep.shrink2(egui::vec2(0.0, 7.0)).y_range(), egui::Stroke::new(1.0, p.line));
                    for t in tools {
                        let tg = ui.painter().layout_no_wrap(crate::i18n::t(t.label), egui::FontId::proportional(12.0), p.text);
                        let (cr, cresp) = ui.allocate_exact_size(egui::vec2(tg.size().x + 16.0, 28.0), egui::Sense::click());
                        let chip = cr.shrink2(egui::vec2(0.0, 4.0));
                        let active_tool = t.id == cur;
                        if active_tool {
                            ui.painter().rect_filled(chip, 2.0, p.selection);
                        } else if cresp.hovered() {
                            ui.painter().rect_filled(chip, 2.0, p.line.linear_multiply(0.5));
                        }
                        ui.painter().galley_with_override_text_color(egui::pos2(chip.left() + 8.0, chip.center().y - tg.size().y / 2.0), tg, if active_tool || cresp.hovered() { p.text } else { p.muted });
                        if cresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            pick = Some(t.id);
                        }
                        whole = whole.union(cr);
                    }
                    ui.add_space(4.0);
                    whole.max.x += 4.0;
                    if let Some(t) = pick {
                        self.set_tool(t);
                    }
                }
            }
            if on || hov {
                ui.painter().set(bg, egui::Shape::Rect(egui::epaint::RectShape::new(whole, 2.0, p.raised, egui::Stroke::new(1.0, if on { p.line } else { egui::Color32::TRANSPARENT }), egui::StrokeKind::Inside)));
            }
            let dot = egui::pos2(rect.left() + 10.0 + 4.0, rect.center().y);
            // Le point dit l'état de l'étape : plein si des fichiers y sont ouverts, un anneau si l'un
            // n'est pas encore enregistré, creux si rien n'est ouvert
            let open_n = if on { self.docs.len() } else { self.ws.list[i].tabs.len() };
            let dirty = on && self.docs.iter().any(|d| d.dirty_since.is_some());
            if open_n > 0 {
                ui.painter().circle_filled(dot, 4.0, color);
            } else {
                ui.painter().circle_stroke(dot, 3.4, egui::Stroke::new(1.3, color));
            }
            if dirty {
                ui.painter().circle_stroke(dot, 6.5, egui::Stroke::new(1.2, color));
            }
            let text_col = if on || hov { p.text } else { p.muted };
            ui.painter().galley_with_override_text_color(egui::pos2(dot.x + 4.0 + 7.0, rect.center().y - galley.size().y / 2.0), galley, text_col);
            if on {
                // La barre de couleur touche le bas de la barre du haut
                let bottom = ui.max_rect().bottom() + 6.0;
                ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(whole.left() + 8.0, bottom - 2.0), egui::pos2(whole.right() - 8.0, bottom)), 0.0, color);
            }
            let r = r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("{}  Ctrl+{}", crate::i18n::t("Étape du pipeline"), i + 1));
            if r.clicked() {
                go = Some(i);
            }
            // Les étapes sont fixes ; un ancien espace fait à la main peut seulement être retiré
            let custom = !workspace::PRESETS.iter().any(|p| p.4 == self.ws.list[i].kind && p.0 == self.ws.list[i].name);
            if custom {
                r.context_menu(|ui| {
                    if n > 1 && ui.button(RichText::new(crate::i18n::t("retirer cet espace")).color(p.missing)).clicked() {
                        del = Some(i);
                        ui.close();
                    }
                });
            }
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
            let w = workspace::accent_of(self.ws.cur());
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
        if !workspace::has_view(&kind, want) {
            let ctx = ui.ctx().clone();
            match self.ws.list.iter().position(|w| workspace::KINDS.contains(&w.kind.as_str()) && workspace::has_view(&w.kind, want)) {
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
        let accent = workspace::accent_of(self.ws.cur()).map(|c| egui::Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(p.accent);
        // Rail d'icônes à gauche : les pages de l'espace (fichiers, cerveau, références…), toujours à la même place
        egui::Panel::left("rail").exact_size(44.0).resizable(false).frame(egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin { left: 6, right: 6, top: 8, bottom: 8 })).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for (label, v) in pages {
                    let view = view_of(v);
                    let on = view == self.view();
                    let v: &str = v;
                    let icon = match v {
                        "files" => lucide::FOLDER,
                        "brain" => lucide::NETWORK,
                        "refs" => lucide::IMAGES,
                        "boards" => lucide::LAYOUT_DASHBOARD,
                        "tasks" => lucide::LIST_CHECKS,
                        "chat" => lucide::MESSAGES_SQUARE,
                        _ => lucide::SEARCH,
                    };
                    let (r, resp) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
                    if on || resp.hovered() {
                        ui.painter().rect_filled(r, 2.0, p.raised);
                    }
                    if on {
                        ui.painter().rect_filled(egui::Rect::from_min_size(egui::pos2(r.left() - 6.0, r.top() + 6.0), egui::vec2(2.0, r.height() - 12.0)), 0.0, accent);
                    }
                    let col = if on { p.text } else if resp.hovered() { p.text } else { p.muted };
                    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(17.0), col);
                    let tip = if v == "files" { format!("{}  ({})", crate::i18n::t(label), keymap::text("explorer")) } else { crate::i18n::t(label) };
                    if resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        if on && v == "files" {
                            // Fichiers déjà là : le même bouton ouvre et ferme l'explorateur
                            self.explorer_open = !self.explorer_open;
                        } else {
                            self.view = Some(view);
                            if v == "files" {
                                self.explorer_open = true;
                            }
                        }
                    }
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
    /// Le contenu du centre : le document ouvert ou la page de l'espace.
    fn main_view(&mut self, ui: &mut egui::Ui) {
        match self.view() {
            View::Files => self.editor(ui),
            View::Search => self.search_view(ui),
            View::Brain => self.brain_view(ui),
            View::Refs => self.refs_view(ui),
            View::Boards => self.boards_view(ui),
            View::Chat => self.chat_view(ui),
            View::Tasks => self.tasks_view(ui),
        }
    }

    /// L'Assistant : dans l'onglet de l'inspecteur quand il est affiché, sinon dans son propre panneau.
    pub(crate) fn toggle_assistant(&mut self) {
        if self.side_wanted() && !self.assistant_open {
            self.insp_tab = if self.insp_tab == 2 { 0 } else { 2 };
        } else {
            self.assistant_open = !self.assistant_open;
        }
    }

    /// Le panneau structure n'a de sens que pour un texte.
    fn side_wanted(&self) -> bool {
        self.view() == View::Files && self.current().is_none_or(|d| matches!(d.content, Content::Text(_))) && self.split.is_none() && !self.side_hidden
    }

    /// Un panneau de la disposition est-il ouvert ?
    fn pane_open(&self, id: &str) -> bool {
        match id {
            layout::MAIN => true,
            "explorer" => self.explorer_open,
            "terminal" => self.terminal.is_some(),
            "assistant" => self.assistant_open,
            "side" => self.side_wanted(),
            v if v.starts_with("ed:") => self.offered_now.contains_key(v),
            v => v.strip_prefix("view:").is_some_and(|k| self.view_panels.iter().any(|x| x == k)),
        }
    }

    fn pane_title(id: &str) -> String {
        match id {
            layout::MAIN => crate::i18n::t("document"),
            "explorer" => crate::i18n::t("fichiers"),
            "terminal" => "terminal".into(),
            "assistant" => crate::i18n::t("assistant"),
            "side" => crate::i18n::t("inspecteur"),
            v if v.starts_with("ed:") => layout::title(v).map(|t| crate::i18n::t(&t)).unwrap_or_else(|| v.trim_start_matches("ed:").to_string()),
            v => {
                let id = v.strip_prefix("view:").unwrap_or(v);
                let kind = id.split('#').next().unwrap_or("");
                let n = id.split('#').nth(1).unwrap_or("1");
                let label = VIEW_WINDOWS.iter().find(|(_, k)| *k == kind).map(|(l, _)| crate::i18n::t(l)).unwrap_or_else(|| kind.to_string());
                if n == "1" { label } else { format!("{label} {n}") }
            }
        }
    }

    /// Dessine un panneau (dans sa case, ou dans sa propre fenêtre s'il en est sorti).
    fn pane_body(&mut self, ui: &mut egui::Ui, id: &str) {
        match id {
            layout::MAIN => {
                if dock::is_out(layout::MAIN) {
                    // La zone du document dans sa propre fenêtre (sur un deuxième écran, par exemple)
                    let p = theme::pal();
                    dock::show_ex(ui, layout::MAIN, &crate::i18n::t("document"), dock::Side::Home, 900.0, 320.0, egui::Frame::new().fill(p.bg), false, |ui| self.main_view(ui));
                } else {
                    self.main_view(ui);
                }
            }
            "explorer" => {
                self.explorer(ui);
                if crate::dock::closed("explorer") {
                    self.explorer_open = false;
                }
            }
            "terminal" => self.terminal_panel(ui),
            "assistant" => self.assistant_panel(ui),
            "side" => self.side_panel(ui),
            v => {
                let id = v.strip_prefix("view:").unwrap_or(v).to_string();
                let kind = id.split('#').next().unwrap_or("").to_string();
                let p = theme::pal();
                let title = Self::pane_title(v);
                dock::show_ex(ui, v, &title, dock::Side::Float, 720.0, 260.0, egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)), true, |ui| {
                    ui.push_id(v, |ui| match kind.as_str() {
                        "brain" => self.brain_view(ui),
                        "refs" => self.refs_view(ui),
                        "boards" => self.boards_view(ui),
                        "tasks" => self.tasks_view(ui),
                        "chat" => self.chat_view(ui),
                        _ => self.search_view(ui),
                    });
                });
                if dock::closed(v) {
                    self.view_panels.retain(|x| *x != id);
                }
            }
        }
    }

    /// La disposition en cases : chaque panneau dans sa case, les panneaux sortis dans leur fenêtre.
    fn tiles(&mut self, ui: &mut egui::Ui) {
        let mut tree = self.tree.take().unwrap_or_else(|| layout::default_layout(true));
        self.offered_now = layout::offered_take();
        for (id, on) in std::mem::take(&mut self.dock_editor_panes) {
            if on {
                layout::add(&mut tree, &id, "");
            } else {
                layout::remove(&mut tree, &id);
            }
        }
        // Les pages ajoutées par le menu « fenêtre » ont leur case ; celles qu'on a fermées la perdent
        for v in self.view_panels.clone() {
            layout::add(&mut tree, &format!("view:{v}"), "side");
        }
        for p in layout::panes(&tree) {
            if let Some(k) = p.strip_prefix("view:") {
                if !self.view_panels.iter().any(|x| x == k) {
                    layout::remove(&mut tree, &p);
                }
            }
        }
        for p in ["explorer", layout::MAIN, "terminal", "side", "assistant"] {
            if layout::find(&tree, p).is_none() {
                layout::add(&mut tree, p, layout::MAIN);
            }
        }
        // Sortis dans leur propre fenêtre du système : dessinés à part
        let panes = layout::panes(&tree);
        let outside: Vec<String> = panes.iter().filter(|p| dock::is_out(p)).cloned().collect();
        dock::set_managed(panes.iter().filter(|p| !outside.contains(p)).cloned().collect());
        layout::set_visibility(&mut tree, &|p| !outside.iter().any(|o| o == p) && self.pane_open(p));
        layout::set_docked(panes.iter().filter(|p| p.starts_with("ed:") && self.offered_now.contains_key(*p) && !outside.contains(p)).cloned().collect());
        for o in &outside {
            // Les panneaux d'éditeur sortis sont dessinés par leur éditeur, dans leur fenêtre
            if self.pane_open(o) && !o.starts_with("ed:") {
                self.pane_body(ui, o);
            }
        }
        let ctx = ui.ctx().clone();
        let dragged = tree.dragged_id(&ctx).and_then(|t| tree.tiles.get_pane(&t).cloned()).filter(|p| p != layout::MAIN);
        let area = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::pal().line).inner_margin(0))
            .show(ui, |ui| {
                let main_tile = layout::find(&tree, layout::MAIN);
                // Les panneaux seuls dans leur case : pas de barre d'onglets, ils dessinent leur propre en-tête
                let alone: HashSet<egui_tiles::TileId> = tree
                    .tiles
                    .iter()
                    .filter_map(|(_, t)| match t {
                        egui_tiles::Tile::Container(egui_tiles::Container::Tabs(tb)) => {
                            let shown: Vec<_> = tb.children.iter().copied().filter(|c| tree.tiles.is_visible(*c)).collect();
                            (shown.len() == 1).then(|| shown[0])
                        }
                        _ => None,
                    })
                    .collect();
                let mut b = Panes { app: self, main_tile, alone };
                tree.ui(&mut b, ui);
            })
            .response
            .rect;
        // Un onglet tiré hors de la disposition (barres du haut, ou hors de l'application) devient une vraie fenêtre
        if let Some(pane) = dragged {
            let at = ctx.input(|i| i.pointer.latest_pos());
            let out = at.is_none_or(|a| !area.shrink(2.0).contains(a));
            if out {
                if let Some(a) = at {
                    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("tile-out")));
                    painter.text(a + egui::vec2(16.0, 18.0), egui::Align2::LEFT_TOP, crate::i18n::t("lâcher : fenêtre à part"), egui::FontId::proportional(11.0), theme::pal().accent);
                }
                if ctx.input(|i| i.pointer.any_released()) {
                    let a = at.unwrap_or(area.center());
                    dock::detach_at(&ctx, &pane, Some(dock::desk(&ctx, a - egui::vec2(40.0, 12.0))), [560.0, 440.0]);
                    ctx.request_repaint();
                }
            }
        }
        for (zone, id) in [("explorer", "explorer"), ("side", "side"), ("assistant", "assistant")] {
            if let Some(r) = layout::find(&tree, id).filter(|t| tree.tiles.is_visible(*t)).and_then(|t| tree.tiles.rect(t)) {
                self.zones_next.push((zone, r));
            }
        }
        self.tree = Some(tree);
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
        nodes::set_vault_root(&path);
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
        // Les alias de l'en-tête (aliases:) mènent aussi à la note
        let rels: Vec<String> = self.entries.iter().filter(|e| !e.is_dir && e.rel.ends_with(".md")).map(|e| e.rel.clone()).collect();
        for r in crate::bases::rows(&self.root(), &rels) {
            for (k, vals) in &r.props {
                if k.eq_ignore_ascii_case("aliases") || k.eq_ignore_ascii_case("alias") {
                    for a in vals.iter().map(|a| a.trim().trim_matches(['[', ']', '"'])).filter(|a| !a.is_empty()) {
                        self.md.entry(a.to_lowercase()).or_insert_with(|| r.rel.clone());
                    }
                }
            }
        }
        self.brain.dirty = true;
        self.update_backlinks();
    }

    fn update_backlinks(&mut self) {
        self.backlinks = match self.current() {
            Some(d) if vault::is_md(&d.rel) => vault::backlinks(&self.root(), &self.entries, &d.rel),
            _ => vec![],
        };
        // Les notes ouvertes comptent avec leur texte en cours, pas celui du disque
        let open: HashMap<String, String> = self.docs.iter().filter_map(|d| if let Content::Text(t) = &d.content { Some((d.rel.clone(), t.clone())) } else { None }).collect();
        self.mentions = match self.current() {
            Some(d) if vault::is_md(&d.rel) => vault::mentions(&self.root(), &self.entries, &d.rel, &|r| open.get(r).cloned()),
            _ => vec![],
        };
    }

    /// Change en lien [[ ]] la première mention (ou toutes) de la note courante dans `rel`
    fn link_mention(&mut self, rel: &str, all: bool) {
        let Some(name) = self.current().map(|d| vault::stem(&d.rel)) else { return };
        let link_all = |mut t: String| {
            let mut changed = false;
            while let Some(nt) = vault::link_mention(&t, &name, 0) {
                t = nt;
                changed = true;
                if !all {
                    break;
                }
            }
            changed.then_some(t)
        };
        if let Some(doc) = self.docs.iter_mut().find(|d| d.rel == rel) {
            if let Content::Text(t) = &mut doc.content {
                if let Some(nt) = link_all(t.clone()) {
                    *t = nt;
                    doc.dirty_since = Some(Instant::now());
                }
            }
        } else {
            let root = self.root();
            if let Some(nt) = vault::read(&root, rel).ok().and_then(link_all) {
                self.report(vault::write(&root, rel, &nt));
            }
        }
        self.update_backlinks();
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
        if let Content::Vector(ed) = &mut doc.content {
            ed.save_if_due(true);
        }
        if let Content::Story(ed) = &mut doc.content {
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
        self.home_open = false;
        // Un son ouvert seul arrive dans une session audio toute prête (à côté de lui)
        let rel = if audio::is_audio(&rel) {
            match self.report(audio_ui::Editor::session_for(&self.root(), &rel)) {
                Some(s) => s,
                None => return,
            }
        } else if midi::is_midi(&rel) {
            // Un MIDI ouvert seul devient un morceau du séquenceur
            match self.report(music_ui::Editor::song_for(&self.root(), &rel)) {
                Some(s) => s,
                None => return,
            }
        } else {
            rel
        };
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
        // L'outil de l'étape suit le fichier ouvert (un matériau ouvre l'outil Matériaux de la 3D)
        let tool = workspace::tool_of(&rel);
        let cur = self.ws.current;
        if workspace::tools(&self.ws.list[cur].kind).iter().any(|t| t.id == tool) {
            self.ws.list[cur].tool = tool.to_string();
        }
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
            // Un .canvas d'Obsidian s'ouvre en board (copie à côté ; l'original ne change pas)
            if ext(&rel) == "canvas" {
                self.import_canvas(&rel);
                return;
            }
            let content = if model3d::is_model(&rel) {
                match (vault::safe(&self.root(), &rel), self.ctx.clone()) {
                    (Ok(path), Some(ctx)) => {
                        // Un modèle seul s'ouvre dans une scène toute prête ; une scène s'ouvre telle quelle
                        let mut v = if formats::has(&rel, "scene") { model3d::Viewer::open(path, &ctx) } else { model3d::Viewer::open_in_new_scene(path, &ctx) };
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
            } else if story::is_story(&rel) {
                match vault::safe(&self.root(), &rel).and_then(story_ui::Editor::open) {
                    Ok(e) => Content::Story(Box::new(e)),
                    Err(e) => Content::Unreadable(e),
                }
            } else if vector::is_vector(&rel) {
                match vault::safe(&self.root(), &rel).and_then(|p| vector_ui::Editor::open(p, &self.root())) {
                    Ok(e) => Content::Vector(Box::new(e)),
                    Err(e) => Content::Unreadable(e),
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
            } else if music::is_song(&rel) {
                match vault::safe(&self.root(), &rel) {
                    Ok(p) => Content::Music(Box::new(music_ui::Editor::open(p))),
                    Err(e) => Content::Unreadable(e),
                }
            } else if audio::is_session(&rel) {
                if ffmpeg::find("ffmpeg").is_none() {
                    Content::NeedFfmpeg(None)
                } else {
                    match vault::safe(&self.root(), &rel) {
                        Ok(p) => Content::Audio(Box::new(audio_ui::Editor::open(p, &self.root()))),
                        Err(e) => Content::Unreadable(e),
                    }
                }
            } else if motion::is_compo(&rel) {
                match vault::safe(&self.root(), &rel) {
                    Ok(p) => Content::Motion(Box::new(motion_ui::Editor::open(p, &self.root()))),
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
            let mut code = code::Editor::default();
            // Référence des marques de modifications : le texte à l'ouverture, puis le dernier commit git s'il existe
            if let Content::Text(t) = &content {
                code.base = Some(std::sync::Arc::new(t.clone()));
                if let Ok(path) = vault::safe(&self.root(), &rel) {
                    let (tx, rx) = std::sync::mpsc::channel();
                    let ctx = self.ctx.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(code::git_base(&path));
                        if let Some(c) = ctx {
                            c.request_repaint();
                        }
                    });
                    code.base_rx = Some(rx);
                }
            }
            self.docs.push(Doc { rel, content, dirty_since: None, preview: false, code });
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
        self.new_untitled("md", |p| crate::vault::atomic_write(p, "").map_err(|e| e.to_string()));
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
        // Bascule clair / sombre : le thème clair, ou le dernier thème sombre choisi
        if dark {
            if self.prefs.theme == "clair" || self.prefs.theme.is_empty() {
                self.prefs.theme = "rose".into();
            }
        } else {
            self.prefs.theme = "clair".into();
        }
        self.prefs.save();
        self.prefs.apply(ctx);
    }

    // ---------- actions différées (évite les conflits d'emprunt pendant le dessin) ----------

    /// Livre une image envoyée d'un outil à un autre : au fichier ouvert de cet outil (le plus récent),
    /// sinon à un nouveau fichier
    fn deliver(&mut self, target: &str, rel: &str) {
        let root = self.root();
        let full = std::path::Path::new(&root).join(rel);
        let matches = |c: &Content| match target {
            "retouche" => matches!(c, Content::Paint(_)),
            "vector" => matches!(c, Content::Vector(_)),
            "motion" => matches!(c, Content::Motion(_)),
            "montage" => matches!(c, Content::Montage(_)),
            _ => false,
        };
        if target == "board" {
            self.board.drop_file(rel, egui::vec2(0.0, 0.0), &mut self.images);
            self.view = Some(View::Boards);
            self.toast = Some((crate::i18n::t("Image ajoutée au moodboard."), Instant::now()));
            return;
        }
        let found = self.active.filter(|&a| matches(&self.docs[a].content)).or_else(|| (0..self.docs.len()).rev().find(|&k| matches(&self.docs[k].content)));
        let k = match found {
            Some(k) => k,
            None => {
                // Pas de fichier ouvert pour cet outil : un nouveau
                match target {
                    // Retouche : l'image devient le document (un calque)
                    "retouche" => {
                        let bytes = std::fs::read(&full).unwrap_or_default();
                        self.new_untitled(crate::formats::ext("image"), |p| std::fs::write(p, &bytes).map_err(|e| e.to_string()));
                        return;
                    }
                    "vector" => self.new_untitled(crate::formats::ext("vector"), |p| crate::vector::Doc::starter().save(p)),
                    "motion" => self.new_untitled(crate::formats::ext("motion"), |p| crate::motion::Compo::default().write(p)),
                    "montage" => self.new_untitled(crate::formats::ext("seq"), |p| crate::montage::Project::default().write(p)),
                    _ => return,
                }
                match self.active.filter(|&a| matches(&self.docs[a].content)) {
                    Some(k) => k,
                    None => return,
                }
            }
        };
        match &mut self.docs[k].content {
            Content::Paint(ed) => ed.receive(&full),
            Content::Vector(ed) => ed.receive(rel),
            Content::Motion(ed) => ed.receive(rel),
            Content::Montage(ed) => ed.receive(&full),
            _ => {}
        }
        // On passe à l'outil qui reçoit (son espace de travail compris)
        let r = self.docs[k].rel.clone();
        self.open(r);
    }

    fn run_actions(&mut self, ctx: &egui::Context) {
        if std::mem::take(&mut self.links_dirty) {
            self.refresh();
        }
        // Images envoyées d'un outil à un autre
        for (target, rel) in crate::sendto::take() {
            self.refresh();
            self.deliver(&target, &rel);
        }
        let root = self.root();
        for a in std::mem::take(&mut self.actions) {
            match a {
                Action::Open(rel) => self.open(rel),
                Action::OpenHeading(rel, heading) => {
                    self.open(rel.clone());
                    self.goto_heading(&rel, &heading);
                }
                Action::OpenLink(name) => {
                    // [[Note#Titre]] et [[Note^bloc]] : on ouvre la note (et on va au titre)
                    let heading = name.split_once('#').map(|(_, h)| h.trim().to_string()).filter(|h| !h.is_empty() && !h.starts_with('^'));
                    let name = name.split(['#', '^']).next().unwrap_or(&name).trim().to_string();
                    if name.is_empty() {
                        continue;
                    }
                    if self.entries.iter().any(|e| e.rel == name && !e.is_dir) {
                        self.open(name.clone());
                        if let Some(h) = heading {
                            self.goto_heading(&name, &h);
                        }
                        continue;
                    }
                    if let (Some(h), Some(rel)) = (&heading, self.md.get(&name.to_lowercase()).cloned()) {
                        self.open(rel.clone());
                        self.goto_heading(&rel, h);
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
                Action::RenameProp(from, to) => self.rename_prop_everywhere(&from, to.as_deref()),
                Action::SetPropOf(rel, key, value) => self.set_prop_of(&rel, &key, &value),
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
                Action::AiTask(kind) => {
                    let Some(a) = self.active else { continue };
                    let ai_ctx = aitasks::Ctx { settings: self.ai.clone().unwrap_or_default(), local: self.local.clone(), lang: ext(&self.docs[a].rel) };
                    let rel = self.docs[a].rel.clone();
                    let d = &mut self.docs[a];
                    let Content::Text(t) = &d.content else { continue };
                    let task = match kind {
                        "edit" => {
                            // La sélection du champ (sinon la ligne du curseur)
                            let id = egui::Id::unique(("code", &rel));
                            let range = egui::TextEdit::load_state(ctx, id).and_then(|s| s.cursor.char_range()).map(|r| {
                                let (a, b): (usize, usize) = (r.primary.index.into(), r.secondary.index.into());
                                (a.min(b), a.max(b))
                            });
                            let byte = |c: usize| t.char_indices().nth(c).map(|x| x.0).unwrap_or(t.len());
                            let (mut s, mut e) = range.map(|(a, b)| (byte(a), byte(b))).unwrap_or((0, 0));
                            if s == e {
                                s = t[..s].rfind('\n').map(|k| k + 1).unwrap_or(0);
                                e = t[e..].find('\n').map(|k| e + k).unwrap_or(t.len());
                            }
                            aitasks::Task::new(aitasks::Kind::Edit { start: s, end: e, original: t[s..e].to_string() })
                        }
                        "proof" => aitasks::Task::new(aitasks::Kind::Proofread),
                        "props" => aitasks::Task::new(aitasks::Kind::Props),
                        _ => aitasks::Task::new(aitasks::Kind::Explain { error: d.code.diags.first().map(|g| format!("ligne {} : {}", g.line + 1, g.msg)).unwrap_or_default() }),
                    };
                    let mut task = task;
                    if kind != "edit" {
                        aitasks::start(&mut task, t, &ai_ctx, ctx);
                    }
                    d.code.ai = Some(task);
                }
                Action::Summarize => {
                    let Some(doc) = self.current() else { continue };
                    let Content::Text(t) = &doc.content else { continue };
                    let (rel, text) = (doc.rel.clone(), t.clone());
                    let settings = self.ai.clone().unwrap_or_default();
                    self.summary_rx = Some(ai::summarize(settings, rel.clone(), text, self.local.clone(), ctx.clone()));
                    self.summary = Some((rel, Summary::Loading));
                    self.insp_tab = 2;
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
            Some("top") => return t("clic : changer d'espace · clic droit sur un espace : nom, couleur, dupliquer · ＋ nouvel espace · Ctrl+PgPréc / PgSuiv : espace voisin · Ctrl+K commandes"),
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
            Some(Content::Model(v)) if v.is_scene() && v.painting() => t("glisser : peindre · Alt+glisser : tourner · molette enfoncée : tourner (Maj : déplacer) · [ ] : taille du pinceau · E : gomme · Ctrl+Z annuler"),
            Some(Content::Model(v)) if v.is_scene() => t("molette enfoncée : tourner (Maj : déplacer) · glisser : rectangle · G R S : déplacer, tourner, taille (X Y Z : axe) · Maj+clic, A, B : sélection · Maj+A ajouter · X supprimer · Maj+D dupliquer · P parent · H cacher · I clé · Tab édition · Espace lecture · Ctrl+Z annuler"),
            Some(Content::Model(_)) => t("glisser : tourner · Maj+glisser : déplacer · molette : zoom · 1 3 7 : vues · 5 : perspective · L + glisser : lumière · H + glisser : HDRI · F2 renommer le fichier"),
            Some(Content::Music(_)) => t("Espace lire · L reprendre · double-clic : nouvelle région · cadre : choisir des notes · Ctrl+A/C/X/V · ↑↓ transposer (Maj : octave) · ←→ décaler · Ctrl+molette : zoom"),
            Some(Content::Audio(_)) => t("Espace lire · S couper à la tête · B rasoir · V sélection · L boucle · M repère · Suppr retirer et resserrer · Ctrl+D dupliquer · Ctrl+molette : zoom"),
            Some(Content::Motion(_)) => t("Espace lecture · ← → image par image · K clé de position · V Q E T outils · Maj : contraindre · Ctrl+molette : zoom · Suppr calque ou clé"),
            Some(Content::Montage(_)) => t("Espace / J K L lecture · V C B N R Y H Z T outils · I O entrée sortie · , insérer · . recouvrir · Ctrl+K couper · Maj+Suppr supprimer et refermer · Q W propager · M repère · S magnétisme · Alt+molette : zoom"),
            Some(Content::Nodes(_)) => t("clic droit : ajouter · Q aligner · Suppr supprimer · F2 renommer la node · molette : zoom · Ctrl+Z annuler"),
            Some(Content::Paint(_)) => t("B pinceau · E gomme · G pot · I pipette · M sélection · V déplacer · [ ] taille · Ctrl+A / Ctrl+D sélection · Ctrl+J dupliquer · Ctrl+E fusionner · Ctrl+Z annuler · Espace+glisser : déplacer la vue"),
            Some(Content::Video(_)) => t("Espace lire · ← → image par image · Maj+← → seconde · I O : entrée, sortie · B pinceau · E gomme"),
            Some(Content::Image) => t("molette : zoom · glisser : déplacer · double-clic : 100 % / ajuster · F2 renommer le fichier"),
            Some(Content::Office(_)) => t("molette : défiler · Ctrl+molette : zoom · F2 renommer le fichier"),
            Some(Content::Text(_)) if self.current().is_some_and(|d| vault::is_md(&d.rel)) => t("Ctrl+E lire / éditer · [[ lien vers une note · Ctrl+F chercher · Ctrl+Z annuler · Ctrl+S enregistrer · F2 renommer"),
            Some(Content::Text(_)) => t("Ctrl+F chercher · Ctrl+G aller à la ligne · Ctrl+Maj+O symboles · F12 définition · Maj+F12 références · F2 renommer le symbole · Alt+Maj+F formater · F5 exécuter · Ctrl+` terminal"),
            Some(_) => t("F2 renommer le fichier · Ctrl+P ouvrir · Ctrl+K commandes"),
            None => t("Ctrl+N nouvelle note · Ctrl+P ouvrir un fichier · glisse des fichiers pour les importer · Ctrl+K commandes"),
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.vault.is_none() || self.dialog.is_some() || keymap::capturing(ctx) {
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
        // Dans un code suivi par un serveur LSP, F2 renomme le symbole sous le curseur (Maj+F2 : le fichier)
        let in_lsp = !self.explorer_focus && self.current().is_some_and(|d| d.code.lsp.is_some() && matches!(d.content, Content::Text(_)));
        if in_lsp && pressed(Modifiers::SHIFT, Key::F2) {
            if let Some(rel) = self.current().map(|d| d.rel.clone()) {
                self.actions.push(Action::Rename(rel));
            }
        }
        if !in_nodes && !in_lsp && pressed(Modifiers::NONE, Key::F2) {
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
        if keymap::pressed(ctx, "palette") {
            self.palette = Some((String::new(), 0));
        }
        if keymap::pressed(ctx, "settings") {
            self.run_cmd(extras::Cmd::Settings(settings::Tab::Appearance), ctx);
        }
        if keymap::pressed(ctx, "switcher") {
            self.switcher = Some((String::new(), 0));
        }
        if !crate::model3d::mesh_keys() && keymap::pressed(ctx, "daily") {
            self.daily_note();
        }
        if keymap::pressed(ctx, "split") {
            self.toggle_split();
        }
        if keymap::pressed(ctx, "new_note") {
            self.new_note();
        }
        if keymap::pressed(ctx, "search") {
            self.view = Some(View::Search);
            self.search.last_query.clear();
        }
        if keymap::pressed(ctx, "replace_all") {
            self.view = Some(View::Search);
            self.search.last_query.clear();
            self.search.replace.get_or_insert_with(String::new);
        }
        if keymap::pressed(ctx, "assistant") {
            self.toggle_assistant();
        }
        if !crate::model3d::mesh_keys() && keymap::pressed(ctx, "explorer") {
            self.explorer_open = !self.explorer_open;
        }
        if keymap::pressed(ctx, "compact") {
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
        if keymap::pressed(ctx, "terminal") {
            self.toggle_terminal();
        }
        if text_open && pressed(Modifiers::COMMAND, Key::G) {
            if let Some(i) = self.active {
                self.docs[i].code.goto = Some(String::new());
            }
        } else if !matches!(self.current().map(|d| &d.content), Some(Content::Paint(_) | Content::Vector(_) | Content::Nodes(_))) && pressed(Modifiers::COMMAND, Key::G) {
            // (Dans la Retouche, le vectoriel et les nodes, Ctrl+G groupe, comme dans Photoshop, Illustrator et Blender)
            self.view = Some(if self.view() == View::Brain { View::Files } else { View::Brain });
        }
        // Ctrl+1, 2, 3… : les étapes du pipeline ; Alt+1, 2, 3… : les outils de l'étape en cours
        let tools = workspace::tools(&self.ws.cur().kind);
        for (n, k) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7].into_iter().enumerate() {
            if pressed(Modifiers::COMMAND, k) && n < self.ws.list.len() {
                self.ws_switch(n, ctx);
            }
            if pressed(Modifiers::ALT, k) {
                if let Some(t) = tools.get(n) {
                    self.set_tool(t.id);
                }
            }
        }
        // Ctrl+Page précédente / suivante : espace voisin (comme dans Blender)
        let n_ws = self.ws.list.len();
        if n_ws > 1 {
            if keymap::pressed(ctx, "next_ws") {
                let i = (self.ws.current + 1) % n_ws;
                self.ws_switch(i, ctx);
            } else if keymap::pressed(ctx, "prev_ws") {
                let i = (self.ws.current + n_ws - 1) % n_ws;
                self.ws_switch(i, ctx);
            }
        }
        // Ctrl+Tab / Ctrl+Maj+Tab : onglet suivant / précédent
        if !self.docs.is_empty() {
            let n = self.docs.len();
            if keymap::pressed(ctx, "prev_tab") {
                self.active = Some((self.active.unwrap_or(0) + n - 1) % n);
            } else if keymap::pressed(ctx, "next_tab") {
                self.active = Some((self.active.unwrap_or(0) + 1) % n);
            }
        }
        // Ctrl+E : lecture / édition d'une note (ailleurs, l'éditeur ouvert garde la touche : menu des arêtes en 3D)
        let on_note = self.active.and_then(|i| self.docs.get(i)).is_some_and(|d| vault::is_md(&d.rel) || fountain::is_fountain(&d.rel));
        if on_note && pressed(Modifiers::COMMAND, Key::E) {
            if let Some(d) = self.active.and_then(|i| self.docs.get_mut(i)) {
                if vault::is_md(&d.rel) || fountain::is_fountain(&d.rel) {
                    d.preview = !d.preview;
                }
            }
        }
        // Ctrl+Maj+S : enregistrer une copie ailleurs (l'original reste), et l'ouvrir
        if keymap::pressed(ctx, "save_copy") {
            if let Some(i) = self.active {
                self.save_copy_as(i);
            }
        }
        // Ctrl+S sur un modèle 3D modifié : les changements (modificateurs, retopo) vont dans le fichier
        if ctx.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::S)) {
            if let Some(Content::Model(v)) = self.active.and_then(|i| self.docs.get_mut(i)).map(|d| &mut d.content) {
                if v.has_pending() {
                    v.save_request = Some(None);
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
        if keymap::pressed(ctx, "close_tab") {
            if let Some(i) = self.active {
                self.actions.push(Action::Close(i));
            }
        }
    }

    // ---------- barres ----------

    /// Barre de menus, tout en haut (comme Maya ou Blender) : Fichier, Édition, Affichage, Disposition, Fenêtre, Aide
    fn menubar(&mut self, ui: &mut egui::Ui) {
        use egui::{Key, Modifiers};
        let p = theme::pal();
        let ctx = ui.ctx().clone();
        let mut cmd: Option<extras::Cmd> = None;
        let mut keys: Option<(Modifiers, Key)> = None;
        let (mut go, mut act) = (None, 0u8);
        let cur = self.disp_name();
        let names: Vec<String> = {
            let w = self.ws.cur();
            let mut v: Vec<String> = w.dispositions.iter().map(|d| d.name.clone()).collect();
            if !v.contains(&cur) {
                v.insert(0, cur.clone());
            }
            v
        };
        let cs = Modifiers::COMMAND | Modifiers::SHIFT;
        egui::Panel::top("menubar")
            .exact_size(24.0)
            .frame(egui::Frame::new().fill(p.bg).inner_margin(egui::Margin::symmetric(8, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    theme::mark(ui, 14.0);
                    ui.add_space(6.0);
                    let item = |ui: &mut egui::Ui, label: &str, short: &str| ui.add(egui::Button::new(crate::i18n::t(label)).shortcut_text(short)).clicked();
                    let top = |t: &str| RichText::new(crate::i18n::t(t)).size(12.5);
                    ui.menu_button(top("Fichier"), |ui| {
                        ui.set_min_width(240.0);
                        ui.menu_button(crate::i18n::t("Nouveau"), |ui| {
                            for (label, c) in [
                                ("note", extras::Cmd::NewNote),
                                ("note du jour", extras::Cmd::Daily),
                                ("depuis un modèle…", extras::Cmd::FromTemplate),
                                ("board", extras::Cmd::NewBoard),
                                ("scène 3D", extras::Cmd::NewScene),
                                ("matériau en nodes", extras::Cmd::NewMaterial),
                                ("matériau à calques (atelier)", extras::Cmd::NewLayered),
                                ("dessin vectoriel", extras::Cmd::NewVector),
                                ("compositing en nodes", extras::Cmd::NewCompositor),
                                ("composition animée (motion)", extras::Cmd::NewMotion),
                                ("storyboard", extras::Cmd::NewStory),
                                ("montage vidéo", extras::Cmd::NewMontage),
                                ("session audio", extras::Cmd::NewAudio),
                                ("morceau (séquenceur)", extras::Cmd::NewMusic),
                                ("scénario (Fountain)", extras::Cmd::NewScript),
                                ("suivi de production", extras::Cmd::NewProduction),
                            ] {
                                if ui.button(crate::i18n::t(label)).clicked() {
                                    cmd = Some(c);
                                }
                            }
                            ui.separator();
                            if ui.button(crate::i18n::t("autre… (choisir le genre)")).clicked() {
                                cmd = Some(extras::Cmd::NewFile);
                            }
                        });
                        if item(ui, "Ouvrir un fichier…", &keymap::text("switcher")) {
                            self.switcher = Some((String::new(), 0));
                        }
                        if item(ui, "Importer des fichiers…", "") {
                            cmd = Some(extras::Cmd::Import);
                        }
                        ui.separator();
                        if item(ui, "Enregistrer une copie sous…", &keymap::text("save_copy")) {
                            keys = Some((cs, Key::S));
                        }
                        if item(ui, "Historique des versions", "") {
                            cmd = Some(extras::Cmd::History);
                        }
                        if item(ui, "Convertir, compresser…", "") {
                            cmd = Some(extras::Cmd::Convert);
                        }
                        if item(ui, "Fermer l'onglet", &keymap::text("close_tab")) {
                            cmd = Some(extras::Cmd::CloseTab);
                        }
                        ui.separator();
                        ui.menu_button(crate::i18n::t("Coffre"), |ui| {
                            let path = self.root();
                            ui.label(RichText::new(&path).color(p.muted).size(10.5));
                            for (label, c) in [("statistiques", extras::Cmd::Stats), ("sauvegarder maintenant (zip)", extras::Cmd::Backup), ("synchroniser avec le cloud", extras::Cmd::Sync), ("ouvrir le dossier", extras::Cmd::OpenVaultFolder), ("recharger les fichiers", extras::Cmd::Reload), ("changer de coffre…", extras::Cmd::ChangeVault)] {
                                if ui.button(crate::i18n::t(label)).clicked() {
                                    cmd = Some(c);
                                }
                            }
                        });
                        if item(ui, "Réglages", &keymap::text("settings")) {
                            cmd = Some(extras::Cmd::Settings(settings::Tab::Appearance));
                        }
                        ui.separator();
                        if item(ui, "Quitter", "") {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.menu_button(top("Édition"), |ui| {
                        ui.set_min_width(220.0);
                        for (label, short, k) in [("Annuler", "Ctrl+Z", (Modifiers::COMMAND, Key::Z)), ("Rétablir", "Ctrl+Maj+Z", (cs, Key::Z))] {
                            if item(ui, label, short) {
                                keys = Some(k);
                            }
                        }
                        ui.separator();
                        for (label, short, k) in [("Couper", "Ctrl+X", Key::X), ("Copier", "Ctrl+C", Key::C), ("Coller", "Ctrl+V", Key::V), ("Dupliquer", "Ctrl+D", Key::D), ("Tout choisir", "Ctrl+A", Key::A)] {
                            if item(ui, label, short) {
                                keys = Some((Modifiers::COMMAND, k));
                            }
                        }
                        ui.separator();
                        if item(ui, "Rechercher dans le coffre", &keymap::text("search")) {
                            self.view = Some(View::Search);
                        }
                        if item(ui, "Palette de commandes", &keymap::text("palette")) {
                            self.palette = Some((String::new(), 0));
                        }
                        if item(ui, "Raccourcis clavier…", "") {
                            cmd = Some(extras::Cmd::Settings(settings::Tab::Shortcuts));
                        }
                    });
                    ui.menu_button(top("Affichage"), |ui| {
                        ui.set_min_width(240.0);
                        ui.checkbox(&mut self.explorer_open, format!("{}   {}", crate::i18n::t("Fichiers"), keymap::text("explorer")));
                        ui.checkbox(&mut self.assistant_open, format!("{}   {}", crate::i18n::t("Assistant"), keymap::text("assistant")));
                        if item(ui, "Terminal", &keymap::text("terminal")) {
                            keys = Some((Modifiers::COMMAND, Key::Backtick));
                        }
                        ui.separator();
                        if item(ui, "Vue côte à côte", &keymap::text("split")) {
                            cmd = Some(extras::Cmd::Split);
                        }
                        if item(ui, "Mode compact", &keymap::text("compact")) {
                            cmd = Some(extras::Cmd::Compact);
                        }
                        if item(ui, "Plein écran", "F11") {
                            let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!full));
                        }
                        ui.separator();
                        if item(ui, if theme::is_dark() { "Thème clair" } else { "Thème sombre" }, "") {
                            cmd = Some(extras::Cmd::Theme);
                        }
                        if ui.add(egui::Button::new(crate::i18n::t("Épingler au-dessus")).selected(self.pinned)).clicked() {
                            cmd = Some(extras::Cmd::Pin);
                        }
                    });
                    let stay = egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
                    egui::containers::menu::MenuButton::new(top("Disposition")).config(stay).ui(ui, |ui| {
                        self.disp_items(ui, &names, &cur, &mut go, &mut act);
                    });
                    ui.menu_button(top("Fenêtre"), |ui| self.window_menu(ui, &mut cmd, &ctx));
                    ui.menu_button(top("Aide"), |ui| {
                        ui.set_min_width(220.0);
                        if item(ui, "Accueil", "") {
                            self.home_open = true;
                        }
                        if item(ui, "Raccourcis clavier", "") {
                            cmd = Some(extras::Cmd::Settings(settings::Tab::Shortcuts));
                        }
                        if item(ui, "Temps passé par projet", "") {
                            cmd = Some(extras::Cmd::TimeReport);
                        }
                        if item(ui, "Vérifier les mises à jour", "") {
                            cmd = Some(extras::Cmd::Updates);
                        }
                        // API de scripts : un exemple prêt à lancer (F5) dans le dossier scripts
                        if item(ui, "Script Python d'exemple (API)", "") {
                            let rel = "scripts/exemple aryadne.py".to_string();
                            let full = std::path::Path::new(&self.root()).join(&rel);
                            if !full.exists() {
                                let _ = std::fs::create_dir_all(full.parent().unwrap_or(std::path::Path::new(".")));
                                let _ = std::fs::write(&full, extras::SCRIPT_EXAMPLE);
                            }
                            extras::install_script_api(&self.root());
                            self.open(rel);
                        }
                        ui.separator();
                        ui.label(RichText::new(format!("Aryadne {}", env!("CARGO_PKG_VERSION"))).size(11.0).color(p.muted));
                    });
                });
            });
        self.disp_apply(go, act, &cur, &ctx);
        if let Some(k) = keys {
            self.inject.push(k);
        }
        if let Some(c) = cmd {
            self.run_cmd(c, &ctx);
        }
    }

    fn topbar(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let ctx = ui.ctx().clone();
        let mut cmd: Option<extras::Cmd> = None;
        let mut open_vault: Option<String> = None;
        egui::Panel::top("top")
            .exact_size(40.0)
            .frame(egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(10, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    // Menu ◆ Aryadne : le coffre (nouveau, importer, sauvegarder…), la fenêtre et les réglages
                    let path = self.root();
                    let short = path.split(['/', '\\']).filter(|s| !s.is_empty()).last().unwrap_or("").to_string();
                    // Le logo et le nom sont passés dans la barre de menus : ici, le coffre seul
                    let mut job = egui::text::LayoutJob::default();
                    job.append(&format!("{}  {short}  {}", lucide::FOLDER, lucide::CHEVRON_DOWN), 0.0, egui::TextFormat { font_id: egui::FontId::proportional(12.5), color: p.text, ..Default::default() });
                    let (r, _) = egui::containers::menu::MenuButton::from_button(egui::Button::new(job).frame(false)).ui(ui, |ui| {
                        ui.set_min_width(240.0);
                        ui.label(RichText::new(&path).color(p.muted).size(10.5));
                        ui.separator();
                        for (label, key, c) in [("nouvelle note", "new_note", extras::Cmd::NewNote), ("note du jour", "daily", extras::Cmd::Daily)] {
                            if ui.add(egui::Button::new(crate::i18n::t(label)).shortcut_text(keymap::text(key))).clicked() {
                                cmd = Some(c);
                            }
                        }
                        for (label, c) in [
                            ("depuis un modèle…", extras::Cmd::FromTemplate),
                            ("scène 3D", extras::Cmd::NewScene),
                            ("montage vidéo", extras::Cmd::NewMontage),
                            ("composition animée (motion)", extras::Cmd::NewMotion),
                            ("session audio", extras::Cmd::NewAudio),
                            ("morceau (séquenceur)", extras::Cmd::NewMusic),
                            ("matériau en nodes", extras::Cmd::NewMaterial),
                            ("matériau à calques (atelier)", extras::Cmd::NewLayered),
                            ("compositing en nodes", extras::Cmd::NewCompositor),
                            ("dessin vectoriel", extras::Cmd::NewVector),
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
                        ui.separator();
                        ui.menu_button(crate::i18n::t("fenêtre"), |ui| self.window_menu(ui, &mut cmd, &ctx));
                        if ui.add(egui::Button::new(format!("{}  {}", lucide::HOUSE, crate::i18n::t("Accueil")))).clicked() {
                            self.home_open = true;
                            ui.close();
                        }
                        if ui.add(egui::Button::new(format!("{}  {}", crate::lucide::SETTINGS, crate::i18n::t("réglages"))).shortcut_text(keymap::text("settings"))).clicked() {
                            cmd = Some(extras::Cmd::Settings(settings::Tab::Appearance));
                        }
                    });
                    r.on_hover_text(format!("{path}"));
                    ui.add_space(18.0);
                    self.ws_items(ui);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ai_btn = egui::Button::new(if self.assistant_open { RichText::new(format!("{} {}", lucide::SPARKLES, crate::i18n::t("assistant"))).color(p.on_accent) } else { RichText::new(format!("{} {}", lucide::SPARKLES, crate::i18n::t("assistant"))) }).fill(if self.assistant_open { p.accent } else { p.panel }).min_size(egui::vec2(0.0, 28.0));
                        if ui.add(ai_btn).on_hover_text(format!("{} ({})", crate::i18n::t("Assistant IA à côté de tout"), keymap::text("assistant"))).clicked() {
                            cmd = Some(extras::Cmd::Assistant);
                        }
                        // Le champ de recherche toujours visible : ouvre la palette (fichiers, commandes, Assistant)
                        let w = ui.available_width().clamp(160.0, 320.0);
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 28.0), egui::Sense::click());
                        let hov = resp.hovered();
                        ui.painter().rect(rect, 2.0, p.field, egui::Stroke::new(1.0, if hov { p.accent } else { p.line }), egui::StrokeKind::Inside);
                        ui.painter().text(rect.left_center() + egui::vec2(10.0, 0.0), egui::Align2::LEFT_CENTER, format!("{}  {}", lucide::SEARCH, crate::i18n::t("rechercher ou lancer une commande")), egui::FontId::proportional(12.5), p.muted);
                        let k = keymap::text("palette").split(" / ").next().unwrap_or("").to_string();
                        let kg = ui.painter().layout_no_wrap(k.clone(), egui::FontId::monospace(11.0), p.muted);
                        let kr = egui::Rect::from_min_size(rect.right_center() - egui::vec2(kg.size().x + 18.0, kg.size().y / 2.0 + 2.0), kg.size() + egui::vec2(10.0, 4.0));
                        if rect.width() > 260.0 {
                            ui.painter().rect_stroke(kr, 2.0, egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
                            ui.painter().galley(kr.min + egui::vec2(5.0, 2.0), kg, p.muted);
                        }
                        if resp.on_hover_text(crate::i18n::t("Fichiers, commandes et Assistant au même endroit")).on_hover_cursor(egui::CursorIcon::Text).clicked() {
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

    /// Menu Fenêtre (dans le menu ◆ Aryadne) : panneaux, nouvelles fenêtres, disposition, thème.
    fn window_menu(&mut self, ui: &mut egui::Ui, cmd: &mut Option<extras::Cmd>, ctx: &egui::Context) {
        let p = theme::pal();
        let _ = &p;
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
        // Panneaux de l'éditeur affiché : dans l'éditeur, ou dans leur propre case de la disposition
        let offered = layout::offered();
        if !offered.is_empty() {
            ui.separator();
            ui.label(caps(&crate::i18n::t("panneaux de l'éditeur : fenêtre à part, ou case")));
            let in_tree: Vec<String> = self.tree.as_ref().map(layout::panes).unwrap_or_default();
            for (id, title) in offered {
                let mut on = in_tree.contains(&id);
                ui.horizontal(|ui| {
                    // À gauche : le sortir dans une vraie fenêtre ; la case : lui donner une case de la disposition
                    let out = dock::is_out(&id);
                    let b = egui::Button::new(egui::RichText::new(if out { crate::lucide::MINIMIZE_2 } else { crate::lucide::EXTERNAL_LINK }).size(12.0).color(if out { theme::pal().accent } else { theme::pal().muted })).frame(false).min_size(egui::vec2(18.0, 18.0));
                    if ui.add(b).on_hover_text(crate::i18n::t(if out { "rattacher à la fenêtre principale" } else { "détacher dans sa propre fenêtre" })).clicked() {
                        if out {
                            dock::attach(ctx, &id);
                        } else {
                            dock::detach_at(ctx, &id, None, [320.0, 460.0]);
                        }
                    }
                    if ui.checkbox(&mut on, crate::i18n::t(&title)).on_hover_text(crate::i18n::t("Coché : le panneau a sa propre case, qu'on glisse où on veut. Décoché : il revient dans l'éditeur.")).changed() {
                        self.dock_editor_panes.push((id, on));
                    }
                });
            }
        }
        ui.separator();
        // La zone du document elle-même, dans sa propre fenêtre (pour un deuxième écran)
        let mut main_out = dock::is_out(layout::MAIN);
        if ui.checkbox(&mut main_out, format!("{}  {}", crate::lucide::EXTERNAL_LINK, crate::i18n::t("document dans sa propre fenêtre"))).on_hover_text(crate::i18n::t("La zone du document (vue 3D, toile, montage, texte…) sort dans une vraie fenêtre, qu'on pose sur un autre écran. Décoché, ou la fenêtre fermée : elle revient.")).changed() {
            if main_out {
                dock::detach_at(ctx, layout::MAIN, None, [900.0, 640.0]);
            } else {
                dock::set_side(layout::MAIN, dock::Side::Home);
            }
            ui.close();
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
        if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::ROTATE_CCW).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("réinitialiser la disposition")).clicked() {
            dock::reset();
            self.reset_layout = true;
            self.view_panels.clear();
            self.explorer_open = true;
            self.side_hidden = false;
            ui.close();
        }
        ui.separator();
        if ui.button(crate::i18n::t(if theme::is_dark() { "thème clair" } else { "thème sombre" })).clicked() {
            *cmd = Some(extras::Cmd::Theme);
        }
        let mut pinned = self.pinned;
        if ui.checkbox(&mut pinned, crate::i18n::t("épingler au-dessus")).on_hover_text(crate::i18n::t("Garder Aryadne au-dessus de Blender et des autres fenêtres")).changed() {
            *cmd = Some(extras::Cmd::Pin);
        }
        if ui.button(crate::i18n::t("mode compact   Ctrl+Maj+M")).clicked() {
            *cmd = Some(extras::Cmd::Compact);
        }
        ui.separator();
        if ui.button(crate::i18n::t(if cfg!(windows) { "ajouter au menu Démarrer" } else { "ajouter au menu des applications" })).clicked() {
            *cmd = Some(extras::Cmd::MenuEntry);
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
                        View::Brain => ("cerveau", p.text),
                        View::Refs => ("références", p.text),
                        View::Boards => ("board", p.text),
                        View::Chat => ("chat", p.link),
                        View::Tasks => ("tâches", p.text),
                        View::Search => ("recherche", p.text),
                        View::Files if preview => ("lecture", p.link),
                        View::Files => ("édition", p.accent),
                    };
                    ui.add_space(10.0);
                    let _ = color;
                    if let Some(d) = self.current() {
                        ui.label(RichText::new(vault::display_rel(&d.rel)).size(11.0));
                        if let Content::Text(t) = &d.content {
                            let lang = if vault::is_md(&d.rel) { "markdown".to_string() } else if ext(&d.rel).is_empty() { "texte".into() } else { ext(&d.rel) };
                            ui.label(RichText::new(lang).color(p.muted).size(11.5));
                            ui.label(RichText::new(format!("{} {} · {} {}", t.lines().count(), crate::i18n::t_mid("lignes"), t.split_whitespace().count(), crate::i18n::t_mid("mots"))).color(p.muted).size(11.5));
                            ui.label(RichText::new(crate::i18n::t(if d.dirty_since.is_some() { "● modifié" } else { "enregistré" })).color(if d.dirty_since.is_some() { p.warning } else { p.muted }).size(11.0));
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
                        // Le mode, en texte discret (plus de badge)
                        ui.label(RichText::new(crate::i18n::t(mode)).color(p.muted).size(11.0));
                        // Au centre : l'aide du moment, courte (trois touches) ; survol : toutes les touches utiles ici
                        let hints = self.key_hints();
                        let short: Vec<&str> = hints.split(" · ").take(3).collect();
                        let more = hints.split(" · ").count() > 3;
                        let text = format!("{}{}", short.join("  ·  "), if more { "  ·  …" } else { "" });
                        let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 20.0), egui::Sense::hover());
                        let galley = ui.painter().layout_no_wrap(text, egui::FontId::proportional(11.0), p.muted);
                        let x = (ui.ctx().content_rect().center().x - galley.size().x / 2.0).clamp(r.left() + 8.0, (r.right() - galley.size().x - 8.0).max(r.left() + 8.0));
                        ui.painter().with_clip_rect(r).galley(egui::pos2(x, r.center().y - galley.size().y / 2.0), galley, p.muted);
                        resp.on_hover_text(hints.replace(" · ", "\n"));
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
                        if !self.in_tool(&d.rel) {
                            continue;
                        }
                        let active = Some(i) == self.active;
                        let name = d.rel.rsplit('/').next().unwrap_or(&d.rel);
                        let mark = if d.dirty_since.is_some() { " ●" } else { "" };
                        // Comme la maquette : 32 px, pastille de la couleur du genre de fichier, barre de cette
                        // couleur en haut de l'onglet actif, séparateur à droite
                        let col = files::kind(&d.rel).1;
                        let fill = if active { p.bg } else { p.panel };
                        let r = egui::Frame::new().fill(fill).inner_margin(egui::Margin { left: 12, right: 4, top: 0, bottom: 0 }).show(ui, |ui| {
                            ui.set_height(32.0);
                            ui.horizontal_centered(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                let (dr, _) = ui.allocate_exact_size(egui::vec2(6.0, 6.0), egui::Sense::hover());
                                ui.painter().circle_filled(dr.center(), 3.0, col);
                                let out = if self.panes.contains(&d.rel) { format!("{} ", lucide::PANEL_RIGHT) } else { String::new() };
                                ui.label(RichText::new(format!("{out}{name}{mark}")).color(if active { p.text } else { p.muted }).size(12.5));
                                let x = ui.add(egui::Button::new(RichText::new(lucide::X).size(11.0).color(p.muted)).frame(false).min_size(egui::vec2(20.0, 20.0)));
                                if x.on_hover_text(format!("{} ({})", crate::i18n::t("fermer"), keymap::text("close_tab"))).clicked() {
                                    self.actions.push(Action::Close(i));
                                }
                            });
                        });
                        let top = r.response.rect;
                        ui.painter().vline(top.right(), top.y_range(), egui::Stroke::new(1.0, p.line));
                        if active {
                            ui.painter().rect_filled(egui::Rect::from_min_size(top.min, egui::vec2(top.width(), 2.0)), 0.0, col);
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
                                if ui.button(format!("{}  {}", crate::lucide::ROTATE_CCW, crate::i18n::t("remettre parmi les onglets"))).clicked() {
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
                            if ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("fermer"))).clicked() {
                                self.actions.push(Action::Close(i));
                                ui.close();
                            }
                        });
                        if resp.middle_clicked() {
                            self.actions.push(Action::Close(i));
                        }
                    }
                    ui.add_space(4.0);
                    let ctx = ui.ctx().clone();
                    if ui.add(egui::Button::new(RichText::new(lucide::PLUS).size(14.0).color(p.muted)).frame(false).min_size(egui::vec2(26.0, 26.0))).on_hover_text(crate::i18n::t("nouveau fichier")).clicked() {
                        self.run_cmd(extras::Cmd::NewFile, &ctx);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(6.0);
                        let on = self.split.is_some();
                        let split_on = on;
                        let _ = split_on;
                        let b = egui::Button::new(RichText::new(lucide::COLUMNS_2).size(14.0).color(if on { p.text } else { p.muted })).fill(if on { p.raised } else { egui::Color32::TRANSPARENT }).frame(on).min_size(egui::vec2(26.0, 26.0));
                        if ui.add(b).on_hover_text(format!("{} ({})", crate::i18n::t("côte à côte"), keymap::text("split"))).clicked() {
                            self.toggle_split();
                        }
                        // Comme la maquette : « Disposition  Principale ▾ »
                        {
                            let v = &mut ui.style_mut().visuals.widgets;
                            v.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                            v.inactive.bg_stroke = egui::Stroke::NONE;
                            v.hovered.weak_bg_fill = p.raised;
                            v.open.weak_bg_fill = p.raised;
                        }
                        self.disp_menu(ui);
                        ui.label(RichText::new(crate::i18n::t("Disposition")).size(11.0).color(p.muted));
                    });
                });
            });
            let r = ui.min_rect();
            ui.painter().hline(r.x_range(), r.bottom(), egui::Stroke::new(1.0, p.line));
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
        crate::dock::panel(ui, "split-pane", "vue côte à côte", egui::Panel::right("split-pane").resizable(true).default_size(w).min_size(240.0).max_size((total - 260.0).max(260.0)), egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)), |ui| {
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
        let mut lsp_note: Option<String> = None;
        let mut open_material: Option<PathBuf> = None;
        let mut open_file: Option<PathBuf> = None;
        let mut open_tab: Option<PathBuf> = None;
        self.feed_project_words();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| {
            if self.home_open || (self.docs.is_empty() && self.ws.cur().tool.is_empty()) {
                self.home(ui);
                return;
            }
            if self.docs.is_empty() {
                // Un outil choisi, rien d'ouvert : l'écran de l'outil
                self.tool_landing(ui);
                return;
            }
            let y0 = ui.cursor().min.y;
            self.tabs(ui);
            let tab_rect = egui::Rect::from_min_max(egui::pos2(ui.max_rect().left(), y0), egui::pos2(ui.max_rect().right(), ui.cursor().min.y));
            self.zones_next.push(("tabs", tab_rect));
            // Le fichier actif n'est pas de l'outil choisi : son dernier fichier, sinon l'écran de l'outil
            if self.active.is_none_or(|a| a >= self.docs.len() || !self.in_tool(&self.docs[a].rel)) {
                match (0..self.docs.len()).rev().find(|&j| self.in_tool(&self.docs[j].rel)) {
                    Some(j) => self.active = Some(j),
                    None => {
                        self.tool_landing(ui);
                        return;
                    }
                }
            }
            let Some(i) = self.active else { return };
            let root = self.root();
            let dark = theme::is_dark();
            let existing: HashSet<String> = self.md.keys().cloned().collect();
            let paths: HashSet<String> = self.entries.iter().map(|e| e.rel.clone()).collect();
            let rel = self.docs[i].rel.clone();
            let is_md = vault::is_md(&rel);
            let can_summarize = matches!(self.docs[i].content, Content::Text(_));

            // Barre du document, comme la maquette : 34 px, chemin à gauche, actions sans cadre à droite
            // La 3D n'a pas de barre de document dans la maquette : la vue prend tout
            let show_bar = !matches!(self.docs[i].content, Content::Model(_) | Content::Montage(_) | Content::Motion(_) | Content::Audio(_) | Content::Music(_) | Content::Nodes(_));
            if show_bar {
            let bar = egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_height(34.0);
                {
                    let v = &mut ui.style_mut().visuals.widgets;
                    v.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                    v.inactive.bg_stroke = egui::Stroke::NONE;
                    v.hovered.weak_bg_fill = p.raised;
                    v.hovered.bg_stroke = egui::Stroke::NONE;
                    v.open.weak_bg_fill = p.raised;
                }
                ui.horizontal_centered(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Fichier sans titre : le bouton pour lui donner un nom et une place
                        if untitled::is_untitled(&rel) && ui.add(egui::Button::new(RichText::new(crate::i18n::t("enregistrer…  Ctrl+S")).color(p.on_accent)).fill(p.accent)).clicked() {
                            self.want_save_as = true;
                        }
                        // « ouvrir ▾ » : application par défaut, au choix, ou dans le dossier
                        let ext_label = if rel.to_lowercase().ends_with(".blend") { "dans Blender" } else { "avec l'app par défaut" };
                        let mut which = 0;
                        egui::containers::menu::MenuButton::new(format!("{} {}", crate::i18n::t("ouvrir"), lucide::CHEVRON_DOWN)).ui(ui, |ui| {
                            if ui.button(ext_label).clicked() {
                                which = 1;
                            }
                            if ui.button(format!("{}  {}", crate::lucide::EXTERNAL_LINK, crate::i18n::t("avec…"))).on_hover_text(crate::i18n::t("Choisir l'application")).clicked() {
                                which = 2;
                            }
                            if ui.add(egui::Button::new(format!("{}  {}", crate::lucide::FOLDER_OPEN, crate::i18n::t("afficher dans le dossier")))).clicked() {
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
                        if rel.to_lowercase().ends_with(".py") && ui.add(egui::Button::new(egui::RichText::new(lucide::CUBOID).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Lance le script dans Blender en arrière-plan (blender --background --python)")).clicked() {
                            self.want_blender = true;
                        }
                        if extras::runner_for(&rel).is_some() && ui.add(egui::Button::new(egui::RichText::new(crate::lucide::PLAY).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Lance le script et affiche sa sortie (F5)")).clicked() {
                            *run_now = true;
                        }
                        // Versions publiées (v001, v002…) : pour tout fichier
                        egui::containers::menu::MenuButton::new(egui::RichText::new(lucide::GIT_COMMIT_VERTICAL).size(14.0)).ui(ui, |ui| {
                            ui.set_min_width(240.0);
                            let list = extras::published(&root, &rel);
                            if ui.button(format!("{}  {} v{:03}", lucide::UPLOAD, crate::i18n::t("publier la version"), list.last().map(|v| v.0 + 1).unwrap_or(1))).on_hover_text(crate::i18n::t("Une copie figée dans le dossier « versions » à côté du fichier")).clicked() {
                                self.flush_all();
                                match extras::publish(&root, &rel) {
                                    Ok(n) => self.toast = Some((format!("{} v{n:03}", crate::i18n::t("publié :")), Instant::now())),
                                    Err(e) => self.error = Some(e),
                                }
                                self.refresh();
                                ui.close();
                            }
                            if !list.is_empty() {
                                ui.separator();
                            }
                            for (n, vrel) in list.iter().rev() {
                                ui.horizontal(|ui| {
                                    ui.label(format!("v{n:03}"));
                                    if ui.small_button(lucide::EXTERNAL_LINK).on_hover_text(crate::i18n::t("ouvrir cette version")).clicked() {
                                        self.actions.push(Action::Open(vrel.clone()));
                                        ui.close();
                                    }
                                    if ui.small_button(lucide::UNDO_2).on_hover_text(crate::i18n::t("revenir à cette version (l'état actuel est publié avant)")).clicked() {
                                        self.flush_all();
                                        match extras::restore_version(&root, &rel, vrel) {
                                            Ok(saved) => {
                                                self.toast = Some((format!("{} v{n:03} · {} v{saved:03}", crate::i18n::t("revenu à"), crate::i18n::t_mid("état précédent gardé en")), Instant::now()));
                                                self.actions.push(Action::Reload(rel.clone()));
                                            }
                                            Err(e) => self.error = Some(e),
                                        }
                                        self.refresh();
                                        ui.close();
                                    }
                                });
                            }
                        })
                        .0
                        .on_hover_text(crate::i18n::t("Versions publiées (v001, v002…)"));
                        if matches!(self.docs[i].content, Content::Text(_) | Content::Vector(_)) && ui.add(egui::Button::new(egui::RichText::new(lucide::HISTORY).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Versions précédentes de ce fichier : les nommer, les comparer, y revenir")).clicked() {
                            self.flush_all();
                            self.history_dlg = Some(extras::HistoryDlg::new(rel.clone(), extras::history_list(&root, &rel), false));
                        }
                        // Exporter la note en page HTML ou en PDF
                        if is_md {
                            let mut fmt = None;
                            egui::containers::menu::MenuButton::new(egui::RichText::new(lucide::DOWNLOAD).size(14.0)).ui(ui, |ui| {
                                if ui.button(format!("{}  {}", lucide::GLOBE, crate::i18n::t("page HTML (images comprises)"))).clicked() {
                                    fmt = Some("html");
                                    ui.close();
                                }
                                if ui.button(format!("{}  PDF", lucide::FILE_TEXT)).clicked() {
                                    fmt = Some("pdf");
                                    ui.close();
                                }
                            }).0.on_hover_text(crate::i18n::t("Exporter la note"));
                            if let (Some(f), Content::Text(t)) = (fmt, &self.docs[i].content) {
                                let title = std::path::Path::new(&rel).file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
                                let dir = std::path::Path::new(&root).join(&rel).parent().map(|p| p.to_path_buf()).unwrap_or_default();
                                let bytes = if f == "html" { crate::mdexport::to_html(t, &title, &dir).into_bytes() } else { crate::mdexport::to_pdf(t, &title) };
                                if let Some(out) = rfd::FileDialog::new().add_filter(f.to_uppercase(), &[f]).set_file_name(format!("{title}.{f}")).save_file() {
                                    match std::fs::write(&out, bytes) {
                                        Ok(()) => self.toast = Some((format!("{} {}", crate::i18n::t("exporté :"), out.to_string_lossy()), Instant::now())),
                                        Err(e) => self.error = Some(e.to_string()),
                                    }
                                }
                            }
                        }
                        if live::is_previewable(&rel) && ui.add(egui::Button::new(egui::RichText::new(lucide::GLOBE).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Ouvre la page dans ton navigateur ; elle se recharge à chaque enregistrement")).clicked() {
                            self.flush_all();
                            match live::open(&root, &rel) {
                                Ok(url) => self.toast = Some((format!("aperçu : {url}"), Instant::now())),
                                Err(e) => self.error = Some(e),
                            }
                        }
                        if can_summarize {
                            // L'IA sur ce fichier : résumer, modifier la sélection, relire, remplir les propriétés, expliquer une erreur
                            let has_diag = !self.docs[i].code.diags.is_empty();
                            let mut act = None;
                            egui::containers::menu::MenuButton::new(egui::RichText::new(lucide::SPARKLES).size(14.0)).ui(ui, |ui| {
                                ui.set_min_width(250.0);
                                if ui.button(format!("{}  {}", lucide::SCROLL_TEXT, crate::i18n::t("résumer ce fichier"))).clicked() {
                                    act = Some(Action::Summarize);
                                    ui.close();
                                }
                                if ui.button(format!("{}  {}", lucide::WAND_SPARKLES, crate::i18n::t("modifier la sélection…   Ctrl+Maj+I"))).on_hover_text(crate::i18n::t("Une consigne (« plus court », « en anglais »…) ; tu vois la différence avant d'accepter")).clicked() {
                                    act = Some(Action::AiTask("edit"));
                                    ui.close();
                                }
                                if is_md {
                                    if ui.button(format!("{}  {}", lucide::SPELL_CHECK, crate::i18n::t("relire (fautes, répétitions)"))).on_hover_text(crate::i18n::t("Des corrections une par une, sans réécrire ton texte")).clicked() {
                                        act = Some(Action::AiTask("proof"));
                                        ui.close();
                                    }
                                    if ui.button(format!("{}  {}", lucide::TAGS, crate::i18n::t("remplir les propriétés"))).on_hover_text(crate::i18n::t("Tags, type et résumé déduits du texte, ajoutés à l'en-tête YAML")).clicked() {
                                        act = Some(Action::AiTask("props"));
                                        ui.close();
                                    }
                                }
                                if has_diag && ui.button(format!("{}  {}", lucide::BUG, crate::i18n::t("expliquer l'erreur"))).clicked() {
                                    act = Some(Action::AiTask("explain"));
                                    ui.close();
                                }
                            }).0.on_hover_text(crate::i18n::t("IA sur ce fichier"));
                            if let Some(a) = act {
                                self.actions.push(a);
                            }
                        }
                        if is_md || fountain::is_fountain(&self.docs[i].rel) {
                            // Texte (édition) ou Lecture (rendu), comme des onglets
                            let d = &mut self.docs[i];
                            ui.add_space(4.0);
                            for (label, icon, preview) in [("lecture", lucide::BOOK_OPEN, true), ("texte", lucide::PENCIL, false)] {
                                let on = d.preview == preview;
                                let b = egui::Button::new(RichText::new(icon).size(14.0).color(if on { p.text } else { p.muted })).fill(if on { p.raised } else { egui::Color32::TRANSPARENT }).stroke(egui::Stroke::new(1.0, if on { p.line } else { egui::Color32::TRANSPARENT }));
                                if ui.add(b).on_hover_text(format!("{} (Ctrl+E)", crate::i18n::t(label))).clicked() {
                                    d.preview = preview;
                                }
                            }
                        }
                        // Le chemin prend la place qui reste, à gauche, et se coupe au besoin : il ne passe plus sous les boutons
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let shown = vault::display_rel(&rel);
                            let mut job = egui::text::LayoutJob::default();
                            let parts: Vec<&str> = shown.split('/').collect();
                            for (k, part) in parts.iter().enumerate() {
                                let last = k == parts.len() - 1;
                                job.append(part, 0.0, egui::TextFormat { font_id: egui::FontId::proportional(12.5), color: if last { p.text } else { p.muted }, ..Default::default() });
                                if !last {
                                    job.append(" / ", 0.0, egui::TextFormat { font_id: egui::FontId::proportional(12.5), color: p.muted, ..Default::default() });
                                }
                            }
                            // Code : le symbole où se trouve le curseur, comme la maquette (fichier › fonction)
                            if !is_md {
                                if let Content::Text(t) = &self.docs[i].content {
                                    let st = egui::text_edit::TextEditState::load(ui.ctx(), egui::Id::unique(("code", &rel)));
                                    if let Some(cr) = st.and_then(|s| s.cursor.char_range()) {
                                        let at: usize = cr.primary.index.into();
                                        let line = t.chars().take(at).filter(|c| *c == '\n').count();
                                        let syms = code::symbols(t, &ext(&rel));
                                        if let Some(sym) = syms.iter().filter(|s| s.line <= line).max_by_key(|s| (s.line, s.depth)) {
                                            job.append(&format!("  {}  ", lucide::CHEVRON_RIGHT), 0.0, egui::TextFormat { font_id: egui::FontId::proportional(11.0), color: p.muted, ..Default::default() });
                                            job.append(&sym.name, 0.0, egui::TextFormat { font_id: egui::FontId::proportional(12.5), color: p.text, ..Default::default() });
                                        }
                                    }
                                }
                            }
                            ui.add(egui::Label::new(job).truncate()).on_hover_text(&shown);
                        });
                    });
                });
            });
            let br = bar.response.rect;
            ui.painter().hline(br.x_range(), br.bottom(), egui::Stroke::new(1.0, p.line));
            }

            let vault_root = self.root();
            let spell_on = self.prefs.spell;
            let ai_ctx = aitasks::Ctx { settings: self.ai.clone().unwrap_or_default(), local: self.local.clone(), lang: String::new() };
            let doc = &mut self.docs[i];
            match &mut doc.content {
                Content::Unreadable(msg) => {
                    ui.add_space(30.0);
                    let mut open = false;
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(msg.as_str()).color(p.muted));
                        ui.add_space(10.0);
                        open = ui.button(format!("{}  {}", crate::lucide::FOLDER_OPEN, crate::i18n::t("ouvrir avec l'app par défaut"))).clicked();
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
                    m.show(ui);
                    if let Some(path) = m.open_request.take() {
                        open_material = Some(path);
                    }
                    if let Some(path) = m.open_tab.take() {
                        open_tab = Some(path);
                    }
                }
                Content::Music(m) => m.show(ui),
                Content::Audio(m) => {
                    m.files = self.entries.iter().filter(|e| !e.is_dir && audio::is_audio(&e.rel)).map(|e| e.rel.clone()).collect();
                    m.show(ui);
                }
                Content::Motion(m) => {
                    m.comps = self.entries.iter().filter(|e| !e.is_dir && motion::is_compo(&e.rel)).map(|e| e.rel.clone()).collect();
                    m.show(ui);
                    if let Some(r) = m.open_request.take() {
                        self.actions.push(Action::Open(r));
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
                Content::Vector(ed) => {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| ed.show(ui));
                    if let Some(p) = ed.open_request.take() {
                        open_file = Some(p);
                    }
                }
                Content::Story(ed) => {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| ed.show(ui));
                    ed.save_if_due(false);
                    if let Some(p) = ed.open_request.take() {
                        open_file = Some(p);
                    }
                }
                Content::Nodes(ed) => {
                    ed.show(ui);
                    if let Some(path) = ed.open_request.take() {
                        open_material = Some(path);
                    }
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
                    viewer.show(ui);
                    if let Some(path) = viewer.open_request.take() {
                        open_material = Some(path);
                    }
                    if std::mem::take(&mut viewer.reload_request) {
                        self.actions.push(Action::Reload(rel.clone()));
                    }
                    if let Some(f) = viewer.scene_request.take() {
                        let root = std::path::PathBuf::from(&viewer.root);
                        if let Ok(r) = f.strip_prefix(&root) {
                            self.actions.push(Action::Open(r.to_string_lossy().replace('\\', "/")));
                        }
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
                Content::Text(text) if doc.preview && fountain::is_fountain(&doc.rel) => {
                    let mut act = None;
                    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            ui.set_max_width(760.0_f32.min(ui.available_width()));
                            egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 20, bottom: 60 }).show(ui, |ui| {
                                act = fountain::show(ui, text);
                            });
                        });
                    });
                    match act {
                        Some(1) => {
                            let stem = std::path::Path::new(&rel).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                            if let Some(f) = rfd::FileDialog::new().set_file_name(format!("{stem}.pdf")).add_filter("PDF", &["pdf"]).save_file() {
                                let r = std::fs::write(&f, fountain::pdf(text)).map_err(|e| e.to_string());
                                self.toast = Some((match r {
                                    Ok(_) => format!("{} {}", crate::i18n::t("exporté :"), f.display()),
                                    Err(e) => e,
                                }, Instant::now()));
                            }
                        }
                        Some(2) => {
                            self.toast = Some((match fountain::breakdown(&vault_root, &rel, text) {
                                Ok(n) => format!("{n} {}", crate::i18n::t_mid("notes créées (personnages et décors)")),
                                Err(e) => e,
                            }, Instant::now()));
                            live::bump();
                        }
                        _ => {}
                    }
                }
                Content::Text(text) if doc.preview => {
                    let mut goto = None;
                    let mut toggle = None;
                    let mut set_prop: Option<(String, Option<Vec<String>>, bool)> = None;
                    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            ui.set_max_width((680.0 + 64.0_f32).min(ui.available_width()));
                            egui::Frame::new().inner_margin(egui::Margin { left: 32, right: 32, top: 40, bottom: 80 }).show(ui, |ui| {
                                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                    match preview::show_doc(ui, text, &existing, &paths) {
                                        Some(preview::Out::Open(n)) => goto = Some(n),
                                        Some(preview::Out::Toggle(l)) => toggle = Some(l),
                                        Some(preview::Out::SetProp { key, vals, list }) => set_prop = Some((key, vals, list)),
                                        Some(preview::Out::RenameProp { from, to }) => self.actions.push(Action::RenameProp(from, to)),
                                        Some(preview::Out::SetPropOf { rel, key, value }) => self.actions.push(Action::SetPropOf(rel, key, value)),
                                        None => {}
                                    }
                                });
                            });
                        });
                    });
                    if let Some(n) = goto {
                        self.actions.push(Action::OpenLink(n));
                    }
                    // Une propriété modifiée en lecture change l'en-tête de la note
                    if let Some((key, vals, list)) = set_prop {
                        *text = vault::set_front(text, &key, vals.as_deref(), list);
                        doc.dirty_since = Some(Instant::now());
                    }
                    // Une case cochée en lecture est cochée dans la note
                    if let Some(nt) = toggle.and_then(|l| preview::toggle_task(text, l)) {
                        *text = nt;
                        doc.dirty_since = Some(Instant::now());
                    }
                }
                Content::Text(text) => {
                    let lang = if is_md { "md".to_string() } else { ext(&rel) };
                    // Les notes : deux points de plus que le code (15 px par défaut, comme la maquette)
                    let fs = if is_md { self.prefs.editor_size + 2.0 } else { self.prefs.editor_size };
                    // Les notes ont leur police (réglages), le code garde la police à chasse fixe
                    let font = if is_md { egui::FontId::new(fs, theme::notes()) } else { egui::FontId::monospace(fs) };
                    let theme = if dark { egui_extras::syntax_highlighting::CodeTheme::dark(fs) } else { egui_extras::syntax_highlighting::CodeTheme::light(fs) }.with_font_id(font);
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
                        if let Some(n) = l.nothing.take() {
                            lsp_note = Some(crate::i18n::t(n));
                        }
                        // Renommage reçu : ce fichier dans l'éditeur, les autres sur le disque
                        if let Some(list) = l.renamed.take() {
                            let here = vault::safe(&root, &rel).ok();
                            let (mut files, mut count) = (0, 0);
                            for (path, edits) in list {
                                if edits.is_empty() {
                                    continue;
                                }
                                count += edits.len();
                                files += 1;
                                if here.as_ref().is_some_and(|h| h == &path || h.canonicalize().ok() == path.canonicalize().ok()) {
                                    ed.revert = Some(lsp::apply_edits(text, &edits));
                                } else if let Ok(old) = std::fs::read_to_string(&path) {
                                    let _ = vault::atomic_write(&path, lsp::apply_edits(&old, &edits));
                                }
                            }
                            lsp_note = Some(format!("{count} {} · {files} {}", crate::i18n::t_mid(if count == 1 { "occurrence renommée" } else { "occurrences renommées" }), crate::i18n::t_mid(if files == 1 { "fichier" } else { "fichiers" })));
                        }
                        // Références reçues : une liste avec un aperçu de chaque ligne
                        if let Some(list) = l.refs.take() {
                            let here = vault::safe(&root, &rel).ok();
                            ed.refs = Some(
                                list.into_iter()
                                    .map(|(path, line)| {
                                        let same = here.as_ref().is_some_and(|h| h == &path);
                                        let src = if same { Some(text.clone()) } else { std::fs::read_to_string(&path).ok() };
                                        let preview = src.and_then(|t| t.lines().nth(line).map(|x| x.trim().chars().take(90).collect())).unwrap_or_default();
                                        (path, line, preview)
                                    })
                                    .collect(),
                            );
                        }
                        if !l.waits.is_empty() {
                            ui.ctx().request_repaint_after(Duration::from_millis(60));
                        }
                    }
                    // Panneau Problèmes : toutes les erreurs connues du projet, en bas à droite de l'éditeur
                    let probs = crate::lsp::all_diags();
                    if !probs.is_empty() {
                        let pid = egui::Id::unique("problems-open");
                        let mut open: bool = ui.ctx().data(|d| d.get_temp(pid).unwrap_or(false));
                        let br = ui.max_rect().right_bottom() + egui::vec2(-12.0, -12.0);
                        egui::Area::new(egui::Id::unique(("problems", &rel))).order(egui::Order::Foreground).pivot(egui::Align2::RIGHT_BOTTOM).fixed_pos(br).show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                if open {
                                    ui.set_width(420.0);
                                    egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                                        for (path, line, msg) in &probs {
                                            let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                                            let r = ui.add(egui::Button::new(RichText::new(format!("{file}:{}   {}", line + 1, msg.lines().next().unwrap_or(""))).monospace().size(11.0)).frame(false).truncate());
                                            if r.on_hover_text(msg).clicked() {
                                                lsp_goto = Some((path.clone(), *line));
                                            }
                                        }
                                    });
                                    ui.separator();
                                }
                                let label = format!("{}  {} {}", crate::lucide::TRIANGLE_ALERT, probs.len(), crate::i18n::t_mid(if probs.len() == 1 { "problème" } else { "problèmes" }));
                                if ui.add(egui::Button::new(RichText::new(label).size(12.0).color(p.warning)).frame(false)).on_hover_text(crate::i18n::t("Toutes les erreurs et alertes du projet (clic pour la liste)")).clicked() {
                                    open = !open;
                                }
                            });
                        });
                        ui.ctx().data_mut(|d| d.insert_temp(pid, open));
                    }
                    // F2 : le nouveau nom, dans une petite barre en haut de l'éditeur
                    let corner = ui.max_rect().right_top() + egui::vec2(-380.0, 8.0);
                    if let Some((at, mut name)) = ed.rename.take() {
                        let mut keep = true;
                        egui::Area::new(egui::Id::unique(("lsp-rename", &rel))).order(egui::Order::Foreground).fixed_pos(corner).show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(crate::lucide::PENCIL).color(p.muted)).on_hover_text(crate::i18n::t("renommer partout (F2)"));
                                    let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(220.0));
                                    if !r.has_focus() && !r.lost_focus() {
                                        r.request_focus();
                                    }
                                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                    if enter && !name.trim().is_empty() {
                                        if let Some(l) = &mut ed.lsp {
                                            l.ask_rename(text, at, name.trim());
                                        }
                                        keep = false;
                                    }
                                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) || ui.add(egui::Button::new(crate::lucide::X).frame(false)).on_hover_text(crate::i18n::t("annuler (Échap)")).clicked() {
                                        keep = false;
                                    }
                                });
                            });
                        });
                        if keep {
                            ed.rename = Some((at, name));
                        }
                    }
                    // Maj+F12 : la liste des références
                    if let Some(list) = ed.refs.take() {
                        let mut keep = true;
                        let here = vault::safe(&root, &rel).ok();
                        egui::Area::new(egui::Id::unique(("lsp-refs", &rel))).order(egui::Order::Foreground).fixed_pos(corner).show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                ui.set_width(360.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("{} {}", list.len(), crate::i18n::t_mid(if list.len() == 1 { "référence" } else { "références" }))).strong());
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.add(egui::Button::new(crate::lucide::X).frame(false)).on_hover_text(crate::i18n::t("fermer (Échap)")).clicked() {
                                            keep = false;
                                        }
                                    });
                                });
                                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                                    for (path, line, preview) in &list {
                                        let same = here.as_ref().is_some_and(|h| h == path);
                                        let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                                        let r = ui.add(egui::Button::new(RichText::new(format!("{file}:{}   {preview}", line + 1)).monospace().size(11.0)).frame(false).truncate());
                                        if r.clicked() {
                                            if same {
                                                ed.jump = Some(*line);
                                            } else {
                                                lsp_goto = Some((path.clone(), *line));
                                            }
                                        }
                                    }
                                });
                            });
                        });
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            keep = false;
                        }
                        if keep {
                            ed.refs = Some(list);
                        }
                    }
                    let folds = ed.folds.clone();
                    let spell_root = (spell_on && lang == "md").then(|| vault_root.clone());
                    let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, width: f32| {
                        let mut job = if lang == "md" {
                            let mut j = md_layout(buf.as_str(), fs);
                            // Fautes d'orthographe soulignées en rouge
                            if let Some(root) = &spell_root {
                                crate::spell::underline(&mut j, &crate::spell::misspelled(buf.as_str(), root), egui::Color32::from_rgb(232, 72, 72));
                            }
                            j
                        } else if hl::supports(&lang) {
                            // Couleurs de la charte (maquette) pour les langages courants
                            let cols = hl::colors(dark);
                            let mut j = hl::highlight(buf.as_str(), &lang, egui::FontId::monospace(fs), &cols);
                            hl::color_brackets(&mut j, cols.text, dark);
                            j
                        } else {
                            egui_extras::syntax_highlighting::highlight(ui.ctx(), ui.style(), &theme, buf.as_str(), &lang)
                        };
                        job.wrap.max_width = if wrap { width } else { f32::INFINITY };
                        code::apply_folds(&mut job, &code::hidden_bytes(buf.as_str(), &folds));
                        ui.painter().layout_job(job)
                    };
                    let mut changed = code::find_bar(ui, ed, text, id);
                    code::goto_bar(ui, ed, text, &lang);
                    // IA : la tâche en cours (consigne, différence à valider, relecture…)
                    if ui.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::I)) {
                        self.actions.push(Action::AiTask("edit"));
                    }
                    changed |= aitasks::bar(ui, &mut ed.ai, text, &ai_ctx);
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
                        // Une note se lit dans une colonne centrée de 680 px (la maquette) ; le code prend toute la largeur
                        let side = if is_md { 32i8 } else { 0 };
                        // Une note : colonne de 680 px centrée (la maquette) ; le code prend toute la largeur
                        let col_w = if is_md { (680.0 + 64.0_f32).min(ui.available_width()) } else { ui.available_width() };
                        ui.with_layout(egui::Layout::top_down(if is_md { egui::Align::Center } else { egui::Align::Min }), |ui| {
                        ui.set_max_width(col_w);
                        egui::Frame::new().inner_margin(egui::Margin { left: if is_md { side } else { gutter }, right: if is_md { side } else if self.prefs.minimap && text.lines().count() > 40 { 84 } else { 24 }, top: if is_md { 40 } else { 16 }, bottom: 120 }).show(ui, |ui| {
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
                            // Clic droit sur un mot souligné : corrections, ou l'ajouter au dictionnaire
                            if let Some(root) = &spell_root {
                                changed |= crate::spell::context_menu(ui, &out, text, root);
                            }
                            code::after(ui, ed, text, &lang, &out, &notes, typed_now && out.response.changed(), code::View { minimap: !is_md && self.prefs.minimap, changes: !is_md, gutter: numbers, font_size: fs, bg });
                        });
                        });
                    });
                    if let Some(nt) = doc.code.revert.take() {
                        *text = nt;
                        changed = true;
                    }
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
        if let Some(rel) = open_tab.and_then(|p| vault::inside(&self.root(), &p)) {
            self.refresh();
            self.open(rel);
        }
        if let Some(path) = open_file {
            if let Some(rel) = vault::inside(&self.root(), &path) {
                self.refresh();
                self.open(rel);
            }
        }
        if let Some(path) = open_material {
            if let Some(rel) = vault::inside(&self.root(), &path) {
                let model = self.active;
                self.refresh();
                self.open(rel.clone());
                self.split = Some(rel);
                self.active = model;
            }
        }
        if let Some(n) = lsp_note {
            self.toast = Some((n, Instant::now()));
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
                // Onglet Assistant : le résumé (s'il y en a un) en haut, la discussion en dessous
                let assistant_tab = self.insp_tab == 2;
                let max_h = if assistant_tab { if self.summary.is_some() { ui.available_height() * 0.4 } else { 0.0 } } else { f32::INFINITY };
                if max_h > 0.0 {
                egui::ScrollArea::vertical().auto_shrink(!assistant_tab).max_height(max_h).show(ui, |ui| {
                    let is_md = self.current().is_some_and(|d| vault::is_md(&d.rel));
                    let tab = self.insp_tab;
                    // Structure : fonctions, classes, titres… (clic = aller à la ligne)
                    let syms = if tab != 0 { vec![] } else { self.current().and_then(|d| if let Content::Text(t) = &d.content { Some(code::symbols(t, &if vault::is_md(&d.rel) { "md".to_string() } else { ext(&d.rel) })) } else { None }).unwrap_or_default() };
                    if tab == 0 && syms.is_empty() {
                        ui.label(RichText::new(crate::i18n::t("Pas de titres ni de symboles dans ce fichier.")).color(p.muted).size(12.5));
                    }
                    if !syms.is_empty() {
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
                                let r = ui.add(egui::Button::new(RichText::new(&sy.name).size(11.5)).frame(false)).on_hover_text(format!("ligne {}", sy.line + 1));
                                if r.clicked() {
                                    jump = Some(sy.line);
                                }
                                if sy.kind == "titre" && is_md {
                                    r.context_menu(|ui| {
                                        let key = format!("{}#{}", self.current().map(|d| d.rel.clone()).unwrap_or_default(), sy.name.trim_start_matches('#').trim());
                                        let on = self.files.pinned.contains(&key);
                                        if ui.button(format!("{}  {}", lucide::STAR, crate::i18n::t(if on { "retirer des signets" } else { "ajouter ce titre aux signets" }))).clicked() {
                                            if on {
                                                self.files.pinned.retain(|k| *k != key);
                                            } else {
                                                self.files.pinned.push(key);
                                            }
                                            self.files.save();
                                            ui.close();
                                        }
                                    });
                                }
                            });
                        }
                        if let (Some(l), Some(i)) = (jump, self.active) {
                            self.docs[i].code.jump = Some(l);
                        }
                        ui.add_space(16.0);
                    }
                    if tab == 1 && !is_md {
                        ui.label(RichText::new(crate::i18n::t("Les liens entrants concernent les notes.")).color(p.muted).size(12.5));
                    }
                    if tab == 1 && is_md {
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
                        ui.add_space(14.0);
                        // Mentions non liées : le nom de la note écrit ailleurs, sans [[ ]]
                        ui.label(caps(&format!("{} · {}", crate::i18n::t("mentions non liées"), self.mentions.len())));
                        ui.add_space(4.0);
                        if self.mentions.is_empty() {
                            ui.label(RichText::new(crate::i18n::t("aucune autre note n'écrit ce nom")).color(p.muted).size(11.5));
                        }
                        let mut link = None;
                        for m in self.mentions.clone() {
                            egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(8).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    if ui.add(egui::Button::new(RichText::new(&m.name).family(theme::bold()).size(12.5)).frame(false)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                        self.actions.push(Action::Open(m.rel.clone()));
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if m.count > 1 && ui.small_button(crate::i18n::t("tout lier")).on_hover_text(format!("{} {}", m.count, crate::i18n::t_mid("mentions"))).clicked() {
                                            link = Some((m.rel.clone(), true));
                                        }
                                        if ui.small_button(crate::i18n::t("lier")).on_hover_text(crate::i18n::t("Changer la première mention en lien [[ ]]")).clicked() {
                                            link = Some((m.rel.clone(), false));
                                        }
                                    });
                                });
                                ui.label(RichText::new(&m.excerpt).color(p.muted).size(11.0));
                            });
                            ui.add_space(4.0);
                        }
                        if let Some((rel, all)) = link {
                            self.link_mention(&rel, all);
                        }
                        ui.add_space(14.0);
                        // Notes proches : elles parlent des mêmes choses, sans lien pour l'instant
                        let cur = self.current().and_then(|d| if let Content::Text(t) = &d.content { Some((d.rel.clone(), t.clone())) } else { None });
                        if let Some((rel, text)) = cur {
                            let notes: Vec<(String, String, u64)> = self.entries.iter().filter(|e| !e.is_dir && vault::is_md(&e.rel)).map(|e| (e.rel.clone(), e.name.trim_end_matches(".md").to_string(), e.modified)).collect();
                            let sug = crate::related::suggest(&self.root(), &rel, &text, &notes);
                            ui.label(caps(&format!("{} · {}", crate::i18n::t("notes proches"), sug.len()))).on_hover_text(crate::i18n::t("Elles parlent des mêmes choses (mots rares en commun) et ne sont pas encore liées"));
                            ui.add_space(4.0);
                            if sug.is_empty() {
                                ui.label(RichText::new(crate::i18n::t("rien de proche pour l'instant")).color(p.muted).size(11.5));
                            }
                            let mut insert = None;
                            for (r, name, score) in sug {
                                egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(8).show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        if ui.add(egui::Button::new(RichText::new(&name).family(theme::bold()).size(12.5)).frame(false)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                            self.actions.push(Action::Open(r.clone()));
                                        }
                                        ui.label(RichText::new(format!("{:.0} %", score * 100.0)).color(p.muted).size(10.5));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.small_button(crate::lucide::LINK).on_hover_text(crate::i18n::t("Insérer le lien [[ ]] au curseur")).clicked() {
                                                insert = Some(format!("[[{name}]]"));
                                            }
                                        });
                                    });
                                });
                                ui.add_space(4.0);
                            }
                            if let (Some(t), Some(a)) = (insert, self.active) {
                                self.docs[a].code.insert = Some(t);
                            }
                        }
                        ui.add_space(16.0);
                    }

                    if tab != 2 {
                        return;
                    }
                    let mut save = None;
                    if self.summary.is_some() {
                        ui.label(caps(&crate::i18n::t("résumé ia")));
                        ui.add_space(4.0);
                    }
                    match &self.summary {
                        None => {}
                        Some((rel, Summary::Loading)) => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new(format!("lecture de {rel}…")).color(p.muted).size(11.5));
                            });
                        }
                        Some((rel, Summary::Failed(e))) => {
                            ui.label(RichText::new(rel).color(p.muted).size(11.0));
                            ui.label(RichText::new(e).color(p.missing).size(12.0));
                            if ui.button(format!("{}  {}", crate::lucide::SETTINGS, crate::i18n::t("réglages ia"))).clicked() {
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
                                if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::COPY).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("copier")).clicked() {
                                    ui.ctx().copy_text(text.clone());
                                }
                                if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::FILE_TEXT).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("enregistrer en note")).clicked() {
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
                }
                if assistant_tab {
                    self.assistant_ui(ui, true);
                }
            });
    }

    // ---------- cerveau ----------

    fn brain_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        if self.brain.dirty {
            self.flush_all();
            let g = match self.brain.local.clone() {
                // Graphe local : la note et ses voisines à deux liens, sans les dossiers
                Some(center) => vault::local_graph(vault::graph(&self.root(), &self.entries, false, self.brain.with_files, ""), &center, 2),
                None => vault::graph(&self.root(), &self.entries, self.brain.with_folders, self.brain.with_files, &self.brain.scope.clone()),
            };
            // Groupes de couleur : les tags les plus portés du coffre
            self.brain.group_of.clear();
            if self.brain_tags {
                let root = self.root();
                let tags = extras::collect_tags(&root, &self.entries);
                self.brain.groups = tags.into_iter().take(6).map(|t| t.0).collect();
                for n in g.nodes.iter().filter(|n| vault::is_md(&n.rel)) {
                    if let Ok(t) = vault::read(&root, &n.rel) {
                        let mine = extras::tags_in(&t);
                        if let Some(k) = self.brain.groups.iter().position(|g| mine.contains(g)) {
                            self.brain.group_of.insert(n.rel.clone(), k);
                        }
                    }
                }
            } else {
                self.brain.groups.clear();
            }
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
                    if ui.checkbox(&mut self.brain_tags, crate::i18n::t("couleurs par tag")).on_hover_text(crate::i18n::t("Les notes prennent la couleur de leur tag le plus courant (les six tags les plus portés)")).changed() {
                        self.brain.dirty = true;
                    }
                    // Graphe local de la dernière note ouverte
                    let note = self.selected.clone().filter(|r| vault::is_md(r));
                    let mut local = self.brain.local.is_some();
                    if ui.add_enabled(note.is_some() || local, egui::Checkbox::new(&mut local, crate::i18n::t("graphe local"))).on_hover_text(crate::i18n::t("La dernière note ouverte et ses voisines à deux liens")).changed() {
                        self.brain.local = if local { note } else { None };
                        self.brain.dirty = true;
                        self.brain.recenter();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(format!("{}  {}", crate::lucide::SCAN, crate::i18n::t("recentrer"))).clicked() {
                            self.brain.recenter();
                        }
                        let busy = self.organize.as_ref().is_some_and(|o| o.rx.is_some());
                        if busy {
                            ui.spinner();
                        } else if ui.button(format!("{}  {}", crate::lucide::SPARKLES, crate::i18n::t("ranger avec l'IA"))).on_hover_text(crate::i18n::t("L'IA propose un rangement de ce dossier ; tu choisis quoi appliquer")).clicked() {
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
                close = ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("fermer"))).clicked();
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

    /// Place le curseur de la note ouverte sur un titre (par son texte)
    fn goto_heading(&mut self, rel: &str, heading: &str) {
        let Some(i) = self.docs.iter().position(|d| d.rel == rel) else { return };
        let line = if let Content::Text(t) = &self.docs[i].content {
            t.lines().position(|l| {
                let l = l.trim_start();
                l.starts_with('#') && l.trim_start_matches('#').trim().eq_ignore_ascii_case(heading.trim())
            })
        } else {
            None
        };
        if let Some(l) = line {
            self.docs[i].code.jump = Some(l);
        }
    }

    /// Bilan du temps passé : par projet (dossier) puis par fichier, sur une période ; copie en CSV
    fn time_report_ui(&mut self, ctx: &egui::Context) {
        let Some(mut period) = self.time_report else { return };
        let p = theme::pal();
        let mut open = true;
        open = crate::dock::os_window(ctx, "time-report", &crate::i18n::t("Temps passé"), egui::vec2(460.0, 520.0), |ui| {
            ui.horizontal(|ui| {
                for (k, l) in [(0u8, "aujourd'hui"), (1, "7 jours"), (2, "30 jours"), (3, "tout")] {
                    ui.selectable_value(&mut period, k, crate::i18n::t(l));
                }
            });
            let from = match period {
                0 => Some(timetrack::days_ago(0)),
                1 => Some(timetrack::days_ago(6)),
                2 => Some(timetrack::days_ago(29)),
                _ => None,
            };
            let rows = self.time.totals(from.as_deref());
            let total: f64 = rows.iter().map(|r| r.1).sum();
            ui.label(RichText::new(format!("{} {}", crate::i18n::t("Total :"), timetrack::fmt(total))).size(15.0).family(theme::bold()));
            ui.label(RichText::new(crate::i18n::t("Compté quand la fenêtre a le focus et qu'il y a eu une action dans les deux dernières minutes.")).size(11.0).color(p.muted));
            ui.add_space(6.0);
            egui::ScrollArea::vertical().auto_shrink(false).max_height(380.0).show(ui, |ui| {
                for (proj, secs, files) in &rows {
                    egui::CollapsingHeader::new(RichText::new(format!("{proj}   {}", timetrack::fmt(*secs))).strong()).id_salt(("time-proj", proj)).show(ui, |ui| {
                        for (rel, s) in files.iter().take(40) {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(vault::display_rel(rel)).size(12.0));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(timetrack::fmt(*s)).monospace().size(11.5).color(p.muted));
                                });
                            });
                        }
                    });
                }
            });
            if ui.button(format!("{}  {}", lucide::COPY, crate::i18n::t("copier en CSV"))).on_hover_text(crate::i18n::t("Projet, fichier, heures : à coller dans un tableur ou une facture")).clicked() {
                let mut csv = String::from("projet,fichier,heures\n");
                for (proj, _, files) in &rows {
                    for (rel, s) in files {
                        csv.push_str(&format!("\"{proj}\",\"{rel}\",{:.2}\n", s / 3600.0));
                    }
                }
                ui.ctx().copy_text(csv);
                self.toast("bilan copié");
            }
        });
        self.time_report = open.then_some(period);
    }

    /// Convertit un .canvas du coffre en board (à côté, même nom) et l'ouvre
    fn import_canvas(&mut self, rel: &str) {
        let root = self.root();
        let data = vault::read(&root, rel).and_then(|t| canvas::import(&t));
        let Some(data) = self.report(data) else { return };
        let stem = rel.strip_suffix(".canvas").unwrap_or(rel);
        let mut out = format!("{stem}.{}", formats::ext("board"));
        let mut i = 2;
        while self.entries.iter().any(|e| e.rel == out) {
            out = format!("{stem} {i}.{}", formats::ext("board"));
            i += 1;
        }
        let json = serde_json::to_string_pretty(&data).unwrap_or_default();
        if self.report(vault::write(&root, &out, &json)).is_some() {
            self.refresh();
            self.open_board(out);
            self.board.fit();
            self.toast = Some((crate::i18n::t("canvas importé en board (le .canvas reste tel quel)").to_string(), Instant::now()));
        }
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
        let frame = egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line));
        // Comme la maquette : les calques à gauche, les couleurs à droite, le board au centre
        // Pendant le diaporama, l'image prend toute la place
        if self.slideshow.is_none() {
            crate::dock::panel(ui, "board-layers", "calques (moodboard)", egui::Panel::left("board-layers").resizable(true).default_size(240.0).min_size(180.0).max_size(360.0), frame, |ui| self.board_layers(ui));
            crate::dock::panel(ui, "board-inspector", "inspecteur (moodboard)", egui::Panel::right("board-inspector").resizable(true).default_size(300.0).min_size(260.0).max_size(420.0), frame, |ui| self.board_inspector(ui));
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| {
            let canvas = ui.available_rect_before_wrap();
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
            self.board_header(ui, canvas);
            self.board_tools(ui, canvas);
            self.slideshow_ui(ui, canvas);
            // Invitation à déposer : board vide, ou des fichiers survolent la fenêtre
            let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
            if hovering || self.board.data.items.is_empty() {
                let text = format!("{}  {}", lucide::IMAGES, crate::i18n::t("Dépose ici des images, des fichiers ou un lien de page web"));
                let g = ui.painter().layout_no_wrap(text, egui::FontId::proportional(12.0), p.text);
                let sub = ui.painter().layout_no_wrap(crate::i18n::t("depuis le bureau ou le navigateur"), egui::FontId::proportional(11.0), p.muted);
                let w = g.size().x + sub.size().x + 34.0;
                let r = egui::Rect::from_center_size(egui::pos2(canvas.center().x, canvas.bottom() - 32.0), egui::vec2(w, 30.0));
                ui.painter().rect_filled(r, 6.0, p.raised);
                dashed_rect(ui.painter(), r, p.accent);
                let gy = r.center().y - g.size().y / 2.0;
                let gw = g.size().x;
                ui.painter().galley(egui::pos2(r.left() + 12.0, gy), g, p.text);
                ui.painter().galley(egui::pos2(r.left() + 22.0 + gw, r.center().y - sub.size().y / 2.0), sub, p.muted);
            }
        });
    }

    /// Le diaporama par-dessus le board : l'image, le temps qui reste, les commandes ; Échap ferme
    fn slideshow_ui(&mut self, ui: &mut egui::Ui, canvas: egui::Rect) {
        let Some(sh) = self.slideshow.as_mut() else { return };
        let p = theme::pal();
        let now = Instant::now();
        let elapsed = sh.paused.unwrap_or(now).duration_since(sh.since).as_secs_f32();
        if sh.secs > 0.0 && elapsed >= sh.secs {
            sh.at = (sh.at + 1) % sh.images.len();
            sh.since = now;
        }
        let rel = sh.images[sh.at].clone();
        let (n, at, secs) = (sh.images.len(), sh.at, sh.secs);
        let left = (secs - elapsed).max(0.0);
        let paused = sh.paused.is_some();
        let painter = ui.painter_at(canvas);
        painter.rect_filled(canvas, 0.0, egui::Color32::from_gray(12));
        // Le board dessous ne reçoit pas les clics
        let _ = ui.interact(canvas, ui.scope_id().with("slideshow-block"), egui::Sense::click_and_drag());
        let area = canvas.shrink2(egui::vec2(24.0, 56.0));
        match self.images.get(&rel, images::Size::Full) {
            images::Slot::Ready(l) => {
                let (w, h) = (l.dims[0] as f32, l.dims[1] as f32);
                let k = (area.width() / w).min(area.height() / h);
                let r = egui::Rect::from_center_size(area.center(), egui::vec2(w * k, h * k));
                painter.image(l.tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
            }
            images::Slot::Loading => {
                ui.ctx().request_repaint_after(Duration::from_millis(50));
            }
            images::Slot::Failed(e) => {
                painter.text(area.center(), egui::Align2::CENTER_CENTER, e, egui::FontId::proportional(13.0), p.missing);
            }
        }
        // Le temps qui reste, en barre sous l'image et en chiffres
        if secs > 0.0 {
            let bar = egui::Rect::from_min_size(egui::pos2(canvas.left(), canvas.bottom() - 4.0), egui::vec2(canvas.width() * (left / secs), 4.0));
            painter.rect_filled(bar, 0.0, p.accent);
            ui.ctx().request_repaint_after(Duration::from_millis(200));
        }
        let mut cmd = 0i8;
        let bar = egui::Rect::from_center_size(egui::pos2(canvas.center().x, canvas.bottom() - 30.0), egui::vec2(300.0, 34.0));
        painter.rect(bar, 6.0, p.panel, egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
        let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink2(egui::vec2(8.0, 4.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        let b = |ui: &mut egui::Ui, icon: &str, tip: &str| ui.add(egui::Button::new(RichText::new(icon).size(15.0)).frame(false).min_size(egui::vec2(28.0, 26.0))).on_hover_text(crate::i18n::t(tip)).clicked();
        if b(&mut cui, crate::lucide::SKIP_BACK, "image précédente (←)") {
            cmd = -1;
        }
        if b(&mut cui, if paused { crate::lucide::PLAY } else { crate::lucide::PAUSE }, "pause (Espace)") {
            cmd = 2;
        }
        if b(&mut cui, crate::lucide::SKIP_FORWARD, "image suivante (→)") {
            cmd = 1;
        }
        let clock = if secs > 0.0 { format!("{}:{:02}", left as u32 / 60, left.ceil() as u32 % 60) } else { "—".into() };
        cui.label(RichText::new(format!("{clock}   {}/{n}", at + 1)).monospace().size(13.0));
        cui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if b(ui, crate::lucide::X, "fermer (Échap)") {
                cmd = 3;
            }
        });
        let keys = ui.input(|i| (i.key_pressed(Key::ArrowLeft), i.key_pressed(Key::ArrowRight), i.key_pressed(Key::Space), i.key_pressed(Key::Escape)));
        if keys.0 {
            cmd = -1;
        } else if keys.1 {
            cmd = 1;
        } else if keys.2 {
            cmd = 2;
        } else if keys.3 {
            cmd = 3;
        }
        let Some(sh) = self.slideshow.as_mut() else { return };
        match cmd {
            -1 => {
                sh.at = (sh.at + n - 1) % n;
                sh.since = Instant::now();
                sh.paused = sh.paused.map(|_| sh.since);
            }
            1 => {
                sh.at = (sh.at + 1) % n;
                sh.since = Instant::now();
                sh.paused = sh.paused.map(|_| sh.since);
            }
            2 => match sh.paused.take() {
                Some(t) => sh.since += Instant::now().duration_since(t),
                None => sh.paused = Some(Instant::now()),
            },
            3 => self.slideshow = None,
            _ => {}
        }
    }

    /// En-tête flottant du board : choix du board, menus Vue, Sélection, Ajouter, Disposer ; à droite, le compte et le zoom
    fn board_header(&mut self, ui: &mut egui::Ui, canvas: egui::Rect) {
        let p = theme::pal();
        let hr = egui::Rect::from_min_max(canvas.min + egui::vec2(12.0, 10.0), egui::pos2(canvas.right() - 12.0, canvas.top() + 44.0));
        ui.painter().rect(hr, 6.0, p.panel, egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
        let mut hui = ui.new_child(egui::UiBuilder::new().max_rect(hr.shrink2(egui::vec2(6.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        flat_buttons(&mut hui);
        hui.set_clip_rect(hr.shrink(1.0));
        hui.spacing_mut().item_spacing.x = 4.0;
        let boards = self.boards_list();
        let current = self.board.rel.clone().unwrap_or_default();
        let mut pick = None;
        let mut new_board = false;
        let mut canvas_io: Option<bool> = None;
        egui::containers::menu::MenuButton::new(RichText::new(format!("{}  {} {}", lucide::LAYOUT_DASHBOARD, board::board_name(&current), lucide::CHEVRON_DOWN)).strong()).ui(&mut hui, |ui| {
            for b in &boards {
                if ui.selectable_label(*b == current, board::board_name(b)).clicked() {
                    pick = Some(b.clone());
                }
            }
            ui.separator();
            if ui.button(format!("{} {}", lucide::PLUS, crate::i18n::t("Nouveau board"))).clicked() {
                new_board = true;
            }
            ui.separator();
            if ui.button(format!("{} {}", lucide::DOWNLOAD, crate::i18n::t("Importer un .canvas (Obsidian)…"))).clicked() {
                canvas_io = Some(true);
            }
            if ui.button(format!("{} {}", lucide::UPLOAD, crate::i18n::t("Exporter en .canvas…"))).on_hover_text(crate::i18n::t("Images, notes, textes et palettes ; les traits à main levée restent dans le board")).clicked() {
                canvas_io = Some(false);
            }
        });
        match canvas_io {
            Some(true) => {
                if let Some(f) = rfd::FileDialog::new().add_filter("JSON Canvas", &["canvas"]).pick_file() {
                    let root = std::path::PathBuf::from(self.root());
                    if let Ok(rel) = f.strip_prefix(&root) {
                        // Dans le coffre : les chemins des fichiers restent valides
                        self.import_canvas(&rel.to_string_lossy().replace('\\', "/"));
                    } else if let Some(text) = self.report(std::fs::read_to_string(&f).map_err(|e| e.to_string())) {
                        let name = f.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or("Canvas".into());
                        let data = canvas::import(&text);
                        if let (Some(data), Some(_)) = (self.report(data), self.new_board(&name)) {
                            self.board.data = data;
                            self.board.checkpoint();
                            let root = self.root();
                            let _ = self.board.save(&root);
                            self.board.fit();
                        }
                    }
                }
            }
            Some(false) => {
                let name = board::board_name(&current);
                if let Some(out) = rfd::FileDialog::new().add_filter("JSON Canvas", &["canvas"]).set_file_name(format!("{name}.canvas")).save_file() {
                    match std::fs::write(&out, canvas::export(&self.board.data)) {
                        Ok(()) => self.toast = Some((format!("{} {}", crate::i18n::t("exporté :"), out.to_string_lossy()), Instant::now())),
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
            }
            None => {}
        }
        if let Some(b) = pick {
            self.open_board(b);
        }
        if new_board {
            self.new_board("Moodboard");
        }
        let sep = |ui: &mut egui::Ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(9.0, 16.0), egui::Sense::hover());
            ui.painter().vline(r.center().x, r.y_range(), egui::Stroke::new(1.0, p.line));
        };
        sep(&mut hui);
        egui::containers::menu::MenuButton::new(crate::i18n::t("Vue")).ui(&mut hui, |ui| {
            if ui.add(egui::Button::new(format!("{}  {}", crate::lucide::SCAN, crate::i18n::t("Tout cadrer")))).clicked() {
                self.board.fit();
            }
            if ui.button(format!("{}  {}", crate::lucide::SCAN_SEARCH, crate::i18n::t("Taille réelle (100 %)"))).clicked() {
                self.board.data.zoom = 1.0;
            }
            ui.separator();
            // Tout en gris (valeurs), et la fenêtre gardée au-dessus des autres logiciels
            if ui.add(egui::Button::new(format!("{}  {}", crate::lucide::CONTRAST, crate::i18n::t("Tout en niveaux de gris"))).selected(self.board.gray_all)).on_hover_text(crate::i18n::t("Pour juger les valeurs")).clicked() {
                self.board.gray_all = !self.board.gray_all;
            }
            if ui.add(egui::Button::new(format!("{}  {}", crate::lucide::PIN, crate::i18n::t("Toujours au-dessus"))).selected(self.board.on_top)).on_hover_text(crate::i18n::t("Garder la fenêtre au-dessus des autres logiciels (Blender, Krita…), comme PureRef")).clicked() {
                self.board.on_top = !self.board.on_top;
                let level = if self.board.on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal };
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
            }
            ui.menu_button(format!("{}  {}", crate::lucide::PRESENTATION, crate::i18n::t("Diaporama (croquis de pose)")), |ui| {
                ui.label(RichText::new(crate::i18n::t("Les images choisies (ou toutes), une à la fois")).size(11.0).color(theme::pal().muted));
                for (label, secs) in [("30 secondes", 30.0), ("1 minute", 60.0), ("2 minutes", 120.0), ("5 minutes", 300.0), ("sans minuteur", 0.0)] {
                    if ui.button(crate::i18n::t(label)).clicked() {
                        let sel: Vec<usize> = if self.board.selection.len() > 1 { self.board.selection.clone() } else { (0..self.board.data.items.len()).collect() };
                        let images: Vec<String> = sel.iter().filter_map(|&k| match &self.board.data.items.get(k)?.kind {
                            board::Kind::Image { rel, .. } if !crate::boardlive::is_model(rel) && !crate::montage::is_project(rel) => Some(rel.clone()),
                            _ => None,
                        }).collect();
                        if images.is_empty() {
                            self.toast("aucune image sur le board");
                        } else {
                            self.slideshow = Some(Slideshow { images, at: 0, secs, since: Instant::now(), paused: None });
                        }
                        ui.close();
                    }
                }
            });
            ui.separator();
            if ui.add_enabled(self.board.can_undo(), egui::Button::new(format!("{}  {}", crate::lucide::UNDO_2, crate::i18n::t("Annuler   Ctrl+Z")))).clicked() {
                self.board.undo();
            }
            if ui.add_enabled(self.board.can_redo(), egui::Button::new(format!("{}  {}", crate::lucide::REDO_2, crate::i18n::t("Rétablir   Ctrl+Y")))).clicked() {
                self.board.redo();
            }
        });
        egui::containers::menu::MenuButton::new(crate::i18n::t("Sélection")).ui(&mut hui, |ui| {
            if ui.button(format!("{}  {}", crate::lucide::CHECK_CHECK, crate::i18n::t("Tout choisir"))).clicked() {
                self.board.selection = (0..self.board.data.items.len()).collect();
            }
            if ui.button(format!("{}  {}", crate::lucide::IMAGES, crate::i18n::t("Toutes les images"))).clicked() {
                self.board.selection = (0..self.board.data.items.len()).filter(|&i| matches!(self.board.data.items[i].kind, board::Kind::Image { .. })).collect();
            }
            if ui.button(format!("{}  {}", crate::lucide::SQUARE_DASHED, crate::i18n::t("Rien"))).clicked() {
                self.board.selection.clear();
            }
            ui.separator();
            if ui.add_enabled(!self.board.selection.is_empty(), egui::Button::new(format!("{}  {}", crate::lucide::TRASH_2, crate::i18n::t("Supprimer   Suppr")))).clicked() {
                self.board.delete_selection();
            }
        });
        egui::containers::menu::MenuButton::new(crate::i18n::t("Ajouter")).ui(&mut hui, |ui| {
            ui.set_min_width(260.0);
            if ui.button(format!("{}  {}", crate::lucide::TYPE, crate::i18n::t("Texte   T"))).clicked() {
                self.board.tool = board::Tool::Text;
            }
            if ui.button(format!("{}  {}", crate::lucide::PALETTE, crate::i18n::t("Palette depuis la sélection"))).clicked() {
                self.board_tab = 0;
                self.palette_to_board();
            }
            ui.separator();
            ui.label(RichText::new(crate::i18n::t("Moodboard automatique")).size(11.0).color(p.muted));
            ui.add(egui::TextEdit::singleline(&mut self.auto_query).hint_text(crate::i18n::t("mots-clés : forêt brume…")).desired_width(240.0));
            if ui.button(format!("{}  {}", crate::lucide::SEARCH, crate::i18n::t("Images par mots-clés"))).on_hover_text(crate::i18n::t("Ajoute les images dont le nom, le dossier ou les notes qui les citent correspondent")).clicked() {
                let q = self.auto_query.clone();
                let n = self.auto_board_keywords(&q);
                self.toast(if n == 0 { "aucune image ne correspond".to_string() } else { format!("{n} images ajoutées") });
            }
            let seed = (self.board.selection.len() == 1)
                .then(|| self.board.selection[0])
                .and_then(|i| self.board.data.items.get(i))
                .and_then(|it| if let board::Kind::Image { rel, .. } = &it.kind { Some(rel.clone()) } else { None });
            if ui.add_enabled(seed.is_some(), egui::Button::new(format!("{}  {}", crate::lucide::PIPETTE, crate::i18n::t("Images aux couleurs proches")))).on_disabled_hover_text(crate::i18n::t("sélectionne d'abord une seule image du board")).clicked() {
                if let Some(seed) = seed {
                    match self.auto_board_colors(&seed) {
                        Ok(n) => self.toast(format!("{} images aux couleurs proches", n.saturating_sub(1))),
                        Err(e) => self.toast(e),
                    }
                }
            }
        });
        egui::containers::menu::MenuButton::new(crate::i18n::t("Disposer")).ui(&mut hui, |ui| {
            if ui.button(format!("{}  {}", crate::lucide::LAYOUT_GRID, crate::i18n::t("En rangées serrées"))).on_hover_text(crate::i18n::t("La sélection, ou tout le board")).clicked() {
                self.board.arrange_grid();
            }
            if ui.add_enabled(self.board.selection.len() > 1, egui::Button::new(format!("{}  {}", crate::lucide::RULER, crate::i18n::t("Même hauteur")))).clicked() {
                self.board.same_height();
            }
            let many = self.board.selection.len() > 1;
            if ui.add_enabled(many, egui::Button::new(format!("{}  {}", crate::lucide::COLUMNS_3, crate::i18n::t("En une ligne")))).clicked() {
                self.board.line_up(false);
            }
            if ui.add_enabled(many, egui::Button::new(format!("{}  {}", crate::lucide::ROWS_3, crate::i18n::t("En une colonne")))).clicked() {
                self.board.line_up(true);
            }
            if ui.add_enabled(many, egui::Button::new(format!("{}  {}", crate::lucide::LAYERS, crate::i18n::t("Empiler")))).clicked() {
                self.board.stack();
            }
            ui.separator();
            ui.label(RichText::new(crate::i18n::t("Aligner la sélection")).size(11.0).color(p.muted));
            ui.horizontal(|ui| {
                for (k, icon, tip) in [(0u8, crate::lucide::ALIGN_START_VERTICAL, "à gauche"), (1, crate::lucide::ALIGN_CENTER_VERTICAL, "au centre"), (2, crate::lucide::ALIGN_END_VERTICAL, "à droite"), (3, crate::lucide::ALIGN_START_HORIZONTAL, "en haut"), (4, crate::lucide::ALIGN_CENTER_HORIZONTAL, "au milieu"), (5, crate::lucide::ALIGN_END_HORIZONTAL, "en bas")] {
                    if ui.add_enabled(many, egui::Button::new(RichText::new(icon).size(14.0))).on_hover_text(crate::i18n::t(tip)).clicked() {
                        self.board.align(k);
                    }
                }
            });
        });

        // Le compte et le zoom à droite, seulement s'il reste la place
        if hui.available_width() > 150.0 {
        hui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let z = (self.board.data.zoom * 100.0).round();
            egui::containers::menu::MenuButton::new(RichText::new(format!("{z} %")).monospace()).ui(ui, |ui| {
                for (label, v) in [("50 %", 0.5), ("100 %", 1.0), ("200 %", 2.0)] {
                    if ui.button(label).clicked() {
                        self.board.data.zoom = v;
                    }
                }
                if ui.add(egui::Button::new(format!("{}  {}", crate::lucide::SCAN, crate::i18n::t("Tout cadrer")))).clicked() {
                    self.board.fit();
                }
            });
            let n = self.board.data.items.len();
            let s = self.board.selection.len();
            let info = if s > 0 { format!("{n} {} · {s} {}", crate::i18n::t_mid("éléments"), crate::i18n::t_mid("choisis")) } else { format!("{n} {}", crate::i18n::t_mid("éléments")) };
            if ui.available_width() > 90.0 {
                ui.label(RichText::new(info).size(11.0).color(p.muted));
            }
        });
        }
    }

    /// Colonne d'outils flottante, à gauche du board ; les réglages du trait à côté quand on dessine
    fn board_tools(&mut self, ui: &mut egui::Ui, canvas: egui::Rect) {
        let p = theme::pal();
        let tools: [(board::Tool, &str, &str, &str); 8] = [
            (board::Tool::Select, lucide::MOUSE_POINTER_2, "Sélectionner", "V"),
            (board::Tool::Text, lucide::TYPE, "Texte", "T"),
            (board::Tool::Pen, lucide::PENCIL, "Pinceau", "B"),
            (board::Tool::Eraser, lucide::ERASER, "Gomme", "E"),
            (board::Tool::Rect, lucide::SQUARE, "Cadre", "R"),
            (board::Tool::Ellipse, lucide::CIRCLE, "Ellipse", "O"),
            (board::Tool::Line, lucide::SPLINE, "Ligne", "L"),
            (board::Tool::Arrow, lucide::ARROW_RIGHT, "Flèche", "Maj+A"),
        ];
        let col = egui::Rect::from_min_size(canvas.min + egui::vec2(12.0, 56.0), egui::vec2(40.0, 8.0 + tools.len() as f32 * 32.0));
        ui.painter().rect(col, 6.0, p.panel, egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
        for (k, (tool, icon, label, key)) in tools.iter().enumerate() {
            let r = egui::Rect::from_min_size(col.min + egui::vec2(5.0, 4.0 + k as f32 * 32.0), egui::vec2(30.0, 30.0));
            let resp = ui.interact(r, egui::Id::unique(("board-tool", k)), egui::Sense::click());
            let on = self.board.tool == *tool;
            if on {
                ui.painter().rect_filled(r, 4.0, p.accent);
            } else if resp.hovered() {
                ui.painter().rect_filled(r, 4.0, p.raised);
            }
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, *icon, egui::FontId::proportional(16.0), if on { p.on_accent } else { p.text });
            if resp.on_hover_text(format!("{} ({key})", crate::i18n::t(label))).clicked() {
                self.board.tool = *tool;
            }
        }
        let drawing = !matches!(self.board.tool, board::Tool::Select | board::Tool::Text | board::Tool::Eraser);
        if drawing {
            let pr = egui::Rect::from_min_size(egui::pos2(col.right() + 8.0, col.top()), egui::vec2(176.0, 92.0));
            ui.painter().rect(pr, 6.0, p.panel, egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
            let mut pui = ui.new_child(egui::UiBuilder::new().max_rect(pr.shrink(8.0)).layout(egui::Layout::top_down(egui::Align::Min)));
            pui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for c in [p.accent, egui::Color32::from_rgb(0x18, 0x18, 0x1b), egui::Color32::WHITE, egui::Color32::from_rgb(0x25, 0x63, 0xeb), egui::Color32::from_rgb(0x16, 0xa3, 0x4a), egui::Color32::from_rgb(0xea, 0xb3, 0x08)] {
                    let (r, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
                    ui.painter().circle_filled(r.center(), 8.0, c);
                    if self.board.pen_color == c {
                        ui.painter().circle_stroke(r.center(), 9.5, egui::Stroke::new(1.5, p.text));
                    }
                    if resp.clicked() {
                        self.board.pen_color = c;
                    }
                }
                ui.color_edit_button_srgba(&mut self.board.pen_color);
            });
            pui.spacing_mut().slider_width = 100.0;
            pui.add(egui::Slider::new(&mut self.board.pen_width, 0.5..=60.0).logarithmic(true).text(crate::i18n::t("épaisseur")));
            pui.add(egui::Slider::new(&mut self.board.opacity, 0.05..=1.0).text(crate::i18n::t("opacité")));
        }
    }

    /// Panneau des calques : un élément par rangée, le plus haut en premier
    fn board_layers(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let (hr, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), egui::Sense::hover());
        ui.painter().hline(hr.x_range(), hr.bottom() - 0.5, egui::Stroke::new(1.0, p.line));
        ui.painter().text(hr.left_center() + egui::vec2(12.0, 0.0), egui::Align2::LEFT_CENTER, crate::i18n::t("Calques"), egui::FontId::new(13.0, theme::bold()), p.text);
        ui.add_space(6.0);
        let shift = ui.input(|i| i.modifiers.shift || i.modifiers.command);
        let mut click: Option<usize> = None;
        egui::ScrollArea::vertical().id_salt("board-layers").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for i in (0..self.board.data.items.len()).rev() {
                let (icon, name) = match &self.board.data.items[i].kind {
                    board::Kind::Image { rel, .. } => (if video::is_video(rel) { lucide::FILM } else if crate::boardlive::is_model(rel) { lucide::BOX } else { lucide::IMAGE }, rel.rsplit('/').next().unwrap_or(rel).to_string()),
                    board::Kind::Text { text, .. } => (lucide::TYPE, text.lines().next().unwrap_or("").replace(['*', '~', '_'], "").chars().take(40).collect::<String>()),
                    board::Kind::Note { rel } => (lucide::FILE_TEXT, vault::stem(rel).to_string()),
                    board::Kind::Palette { .. } => (lucide::PALETTE, crate::i18n::t("Palette")),
                    board::Kind::Stroke { .. } => (lucide::PENCIL, crate::i18n::t("Trait")),
                };
                let name = if name.trim().is_empty() { crate::i18n::t("Texte") } else { name };
                let sel = self.board.selection.contains(&i);
                let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::click());
                if sel {
                    ui.painter().rect_filled(r, 0.0, p.selection);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, 0.0, p.line.linear_multiply(0.45));
                }
                ui.painter().text(r.left_center() + egui::vec2(19.0, 0.0), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(14.0), p.muted);
                let clip = ui.painter().with_clip_rect(r.shrink2(egui::vec2(8.0, 0.0)));
                clip.text(r.left_center() + egui::vec2(32.0, 0.0), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(12.5), p.text);
                if resp.clicked() {
                    click = Some(i);
                }
            }
        });
        if let Some(i) = click {
            if shift {
                if let Some(at) = self.board.selection.iter().position(|&k| k == i) {
                    self.board.selection.remove(at);
                } else {
                    self.board.selection.push(i);
                }
            } else {
                self.board.selection = vec![i];
            }
        }
    }

    /// Couleurs des images choisies (ou de tout le board), d'après leurs vignettes
    fn board_source_colors(&mut self) -> (Vec<egui::Color32>, usize) {
        let sel: Vec<usize> = if self.board.selection.is_empty() { (0..self.board.data.items.len()).collect() } else { self.board.selection.clone() };
        let mut cols = vec![];
        let mut n = 0;
        for i in sel {
            if let Some(board::Kind::Image { rel, .. }) = self.board.data.items.get(i).map(|x| x.kind.clone()) {
                if let images::Slot::Ready(l) = self.images.get(&rel, images::Size::Thumb) {
                    cols.extend(l.palette.iter().copied());
                    n += 1;
                }
            }
        }
        (cols, n)
    }

    fn palette_to_board(&mut self) {
        let (src, _) = self.board_source_colors();
        let cols = palette::generate(&src, &self.pal_gen);
        if cols.is_empty() {
            self.toast(crate::i18n::t("Choisis des images pour en tirer une palette"));
            return;
        }
        let n = cols.len() as f32;
        let at = self.board.view_center;
        self.board.add(board::Kind::Palette { colors: cols.iter().map(|c| [c.r(), c.g(), c.b()]).collect() }, at, egui::vec2(60.0 * n, 100.0));
    }

    /// Inspecteur du board : onglets Couleurs (générateur de palettes) et Propriétés
    fn board_inspector(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let (hr, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), egui::Sense::hover());
        ui.painter().hline(hr.x_range(), hr.bottom() - 0.5, egui::Stroke::new(1.0, p.line));
        let mut x = hr.left() + 6.0;
        for (k, label) in [(1u8, "Propriétés"), (0, "Couleurs")] {
            let text = crate::i18n::t(label);
            let g = ui.painter().layout_no_wrap(text, egui::FontId::proportional(12.5), p.text);
            let r = egui::Rect::from_min_size(egui::pos2(x, hr.center().y - 12.0), egui::vec2(g.size().x + 16.0, 24.0));
            let resp = ui.interact(r, egui::Id::unique(("board-insp", k)), egui::Sense::click());
            let on = self.board_tab == k;
            if on {
                ui.painter().rect(r, 2.0, p.raised, egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
            }
            ui.painter().galley(egui::pos2(r.left() + 8.0, r.center().y - g.size().y / 2.0), g, if on { p.text } else { p.muted });
            if resp.clicked() {
                self.board_tab = k;
            }
            x = r.right() + 2.0;
        }
        egui::ScrollArea::vertical().id_salt("board-insp").auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new().inner_margin(12).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                if self.board_tab == 1 {
                    self.board_props(ui);
                } else {
                    self.board_colors(ui);
                }
            });
        });
    }

    fn board_props(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let selection = self.board.selection.clone();
        match selection.as_slice() {
            [] => {
                ui.label(RichText::new(crate::i18n::t("Rien de choisi. Clique un élément du board ou d'un calque.")).size(12.0).color(p.muted));
            }
            [i] => {
                let Some(it) = self.board.data.items.get(*i).cloned() else { return };
                let (what, path) = match &it.kind {
                    board::Kind::Image { rel, .. } => ("Image", Some(rel.clone())),
                    board::Kind::Text { .. } => ("Texte", None),
                    board::Kind::Note { rel } => ("Note", Some(rel.clone())),
                    board::Kind::Palette { .. } => ("Palette", None),
                    board::Kind::Stroke { .. } => ("Trait", None),
                };
                ui.label(RichText::new(crate::i18n::t(what)).size(15.0).family(theme::bold()));
                if let Some(rel) = &path {
                    ui.label(RichText::new(rel).size(11.0).color(p.muted));
                    let root = self.root();
                    // Image manquante : la re-lier à un fichier
                    if !self.entries.iter().any(|e| e.rel == *rel) && !vault::safe(&root, rel).map(|p| p.exists()).unwrap_or(false) {
                        ui.label(RichText::new(format!("{}  {}", crate::lucide::TRIANGLE_ALERT, crate::i18n::t("fichier introuvable"))).size(12.0).color(p.missing));
                        if ui.button(format!("{}  {}", crate::lucide::LINK, crate::i18n::t("re-lier à un fichier…"))).on_hover_text(crate::i18n::t("Choisir le fichier : il est copié à sa place dans le coffre")).clicked() {
                            if let Some(f) = rfd::FileDialog::new().pick_file() {
                                let r = vault::safe(&root, rel).and_then(|dst| {
                                    if let Some(d) = dst.parent() {
                                        let _ = std::fs::create_dir_all(d);
                                    }
                                    std::fs::copy(&f, &dst).map(|_| ()).map_err(|e| e.to_string())
                                });
                                if self.report(r).is_some() {
                                    sources::record(&root, rel, &f.to_string_lossy());
                                    self.images.forget(rel);
                                    self.refresh();
                                }
                            }
                        }
                    }
                    // D'où vient l'image
                    if let Some(src) = sources::get(&root, rel) {
                        ui.label(RichText::new(crate::i18n::t("Source")).size(11.0).color(p.muted));
                        ui.horizontal(|ui| {
                            let short: String = if src.chars().count() > 38 { format!("…{}", src.chars().rev().take(36).collect::<Vec<_>>().into_iter().rev().collect::<String>()) } else { src.clone() };
                            ui.label(RichText::new(short).size(11.5)).on_hover_text(&src);
                            if ui.add(egui::Button::new(RichText::new(crate::lucide::EXTERNAL_LINK).size(13.0)).frame(false)).on_hover_text(crate::i18n::t("ouvrir la source")).clicked() {
                                if src.starts_with("http") {
                                    ui.ctx().open_url(egui::OpenUrl::new_tab(&src));
                                } else {
                                    let _ = open::that(std::path::Path::new(&src).parent().unwrap_or(std::path::Path::new(&src)));
                                }
                            }
                            if ui.add(egui::Button::new(RichText::new(crate::lucide::COPY).size(13.0)).frame(false)).on_hover_text(crate::i18n::t("copier")).clicked() {
                                ui.ctx().copy_text(src.clone());
                            }
                        });
                    }
                }
                ui.label(RichText::new(crate::i18n::t("Taille")).size(11.0).color(p.muted));
                ui.label(RichText::new(format!("{:.0} × {:.0}", it.w, it.h)).monospace().size(12.0));
                // Grille de composition sur l'image
                if let board::Kind::Image { guide, .. } = &it.kind {
                    let mut g = *guide;
                    ui.label(RichText::new(crate::i18n::t("Grille de composition")).size(11.0).color(p.muted));
                    ui.horizontal_wrapped(|ui| {
                        for (k, name) in board::GUIDES.iter().enumerate() {
                            if ui.selectable_label(g == k as u8, crate::i18n::t(name)).clicked() {
                                g = k as u8;
                            }
                        }
                    });
                    if g != *guide {
                        self.board.checkpoint();
                        if let Some(board::Item { kind: board::Kind::Image { guide, .. }, .. }) = self.board.data.items.get_mut(*i) {
                            *guide = g;
                        }
                        self.board.mark_dirty();
                    }
                }
                if let board::Kind::Palette { colors } = &it.kind {
                    ui.horizontal_wrapped(|ui| {
                        for c in colors {
                            let col = egui::Color32::from_rgb(c[0], c[1], c[2]);
                            if ui.add(egui::Button::new(RichText::new(palette::hex(col)).monospace().size(11.0))).on_hover_text(crate::i18n::t("copier")).clicked() {
                                ui.ctx().copy_text(format!("#{}", palette::hex(col)));
                            }
                        }
                    });
                }
                if let Some(rel) = path {
                    if ui.button(format!("{}  {}", crate::lucide::FOLDER_OPEN, crate::i18n::t("Ouvrir"))).clicked() {
                        self.actions.push(Action::Open(rel));
                    }
                }
            }
            many => {
                ui.label(RichText::new(format!("{} {}", many.len(), crate::i18n::t_mid("éléments choisis"))).size(13.0));
            }
        }
    }

    fn board_colors(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let (src, n_img) = self.board_source_colors();
        let cols = palette::generate(&src, &self.pal_gen);
        ui.horizontal(|ui| {
            ui.label(RichText::new(crate::i18n::t(if self.board.selection.is_empty() { "Palette du board" } else { "Palette de la sélection" })).size(11.0).color(p.muted));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("{n_img} {}", crate::i18n::t_mid(if n_img == 1 { "image" } else { "images" }))).size(11.0).color(p.muted));
            });
        });
        if cols.is_empty() {
            ui.label(RichText::new(crate::i18n::t("Choisis des images sur le board : leurs couleurs donnent la palette.")).size(12.0).color(p.muted));
        } else {
            let gap = 6.0;
            let w = ((ui.available_width() - gap * (cols.len() as f32 - 1.0)) / cols.len() as f32).max(16.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for c in &cols {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 56.0), egui::Sense::click());
                        ui.painter().rect_filled(r, 2.0, *c);
                        if self.pal_gen.readable {
                            let (tc, ratio) = palette::text_on(*c);
                            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "Aa", egui::FontId::new(13.0, theme::bold()), tc);
                            let _ = ratio;
                        }
                        if resp.on_hover_text(format!("#{} · {}", palette::hex(*c), crate::i18n::t("cliquer pour copier"))).clicked() {
                            ui.ctx().copy_text(format!("#{}", palette::hex(*c)));
                            self.toast = Some((format!("#{} copié", palette::hex(*c)), Instant::now()));
                        }
                        ui.label(RichText::new(palette::hex(*c)).monospace().size(10.5).color(p.muted));
                    });
                }
            });
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 8.0) / 3.0;
                if ui.add_sized([w, 24.0], egui::Button::new(format!("{}  {}", crate::lucide::COPY, crate::i18n::t("Copier")))).on_hover_text(crate::i18n::t("Les codes hexadécimaux, une ligne chacun")).clicked() {
                    ui.ctx().copy_text(cols.iter().map(|c| format!("#{}", palette::hex(*c))).collect::<Vec<_>>().join("\n"));
                    self.toast("palette copiée");
                }
                if ui.add_sized([w, 24.0], egui::Button::new(format!("{}  {}", crate::lucide::DOWNLOAD, crate::i18n::t("Exporter")))).on_hover_text(crate::i18n::t("Fichier .gpl (GIMP, Krita, Inkscape, Blender) dans le dossier palettes du coffre")).clicked() {
                    let name = self.board.rel.as_deref().map(board::board_name).unwrap_or_else(|| "palette".into());
                    let mut text = format!("GIMP Palette\nName: {name}\nColumns: {}\n#\n", cols.len());
                    for c in &cols {
                        text.push_str(&format!("{:3} {:3} {:3}\t#{}\n", c.r(), c.g(), c.b(), palette::hex(*c)));
                    }
                    let rel = format!("palettes/{name}.gpl");
                    match vault::write(&self.root(), &rel, &text) {
                        Ok(()) => {
                            self.toast(format!("exportée : {rel}"));
                            self.refresh();
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                if ui.add_sized([w, 24.0], egui::Button::new(RichText::new(crate::i18n::t("Sur le board")).color(p.on_accent)).fill(p.accent)).clicked() {
                    self.palette_to_board();
                }
            });
        }
        ui.add_space(6.0);
        ui.label(RichText::new(crate::i18n::t("Harmonie")).size(11.0).color(p.muted));
        let w = (ui.available_width() - 8.0) / 3.0;
        egui::Grid::new("pal-harmony").num_columns(3).spacing(egui::vec2(4.0, 4.0)).show(ui, |ui| {
            for (k, (h, label)) in palette::HARMONIES.iter().enumerate() {
                let on = self.pal_gen.harmony == *h;
                let b = egui::Button::new(RichText::new(crate::i18n::t(label)).size(12.0)).fill(if on { p.selection } else { p.field });
                if ui.add_sized([w.max(60.0), 24.0], b).clicked() {
                    if *h == palette::Harmony::Random && on {
                        self.pal_gen.seed = self.pal_gen.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                    }
                    self.pal_gen.harmony = *h;
                }
                if k % 3 == 2 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new(crate::i18n::t("Réglages")).size(11.0).color(p.muted));
        let slider_row = |ui: &mut egui::Ui, label: &str, w: egui::Slider<'_>| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(80.0, 20.0), egui::Sense::hover());
                ui.painter().text(r.left_center(), egui::Align2::LEFT_CENTER, crate::i18n::t(label), egui::FontId::proportional(12.5), p.text);
                ui.spacing_mut().slider_width = (ui.available_width() - 50.0).max(60.0);
                ui.add(w);
            });
        };
        slider_row(ui, "Couleurs", egui::Slider::new(&mut self.pal_gen.count, 3..=9));
        let mut sat = (self.pal_gen.saturation * 100.0).round() as i32;
        slider_row(ui, "Saturation", egui::Slider::new(&mut sat, -100..=100));
        self.pal_gen.saturation = sat as f32 / 100.0;
        let mut con = (self.pal_gen.contrast * 100.0).round() as i32;
        slider_row(ui, "Contraste", egui::Slider::new(&mut con, -100..=100));
        self.pal_gen.contrast = con as f32 / 100.0;
        ui.checkbox(&mut self.pal_gen.readable, crate::i18n::t("Texte lisible sur chaque couleur (4,5:1)"));
        if !src.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(crate::i18n::t("Variantes")).size(11.0).color(p.muted));
            for h in [palette::Harmony::Analogous, palette::Harmony::Complement, palette::Harmony::Mono] {
                let st = palette::Settings { harmony: h, ..self.pal_gen.clone() };
                let alt = palette::generate(&src, &st);
                let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), egui::Sense::click());
                let w = r.width() / alt.len().max(1) as f32;
                for (k, c) in alt.iter().enumerate() {
                    ui.painter().rect_filled(egui::Rect::from_min_size(egui::pos2(r.left() + w * k as f32, r.top()), egui::vec2(w + 0.5, r.height())), 0.0, *c);
                }
                if resp.hovered() {
                    ui.painter().rect_stroke(r, 2.0, egui::Stroke::new(1.0, p.text), egui::StrokeKind::Outside);
                }
                let label = palette::HARMONIES.iter().find(|x| x.0 == h).map(|x| x.1).unwrap_or("");
                if resp.on_hover_text(crate::i18n::t(label)).clicked() {
                    self.pal_gen.harmony = h;
                }
            }
        }
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
            crate::dock::panel(ui, "ref-detail", "détail de la référence", egui::Panel::right("ref-detail").resizable(true).default_size(320.0).min_size(240.0).max_size((ui.available_width() * 0.5).max(260.0)), egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(12), |ui| {
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
                        if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::FOLDER_OPEN).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("ouvrir")).clicked() {
                            self.actions.push(Action::Open(sel.clone()));
                        }
                        if ui.button(format!("{}  {}", crate::lucide::PLUS, crate::i18n::t("ajouter au board"))).clicked() {
                            self.ensure_board();
                            let c = self.board.view_center;
                            self.board.drop_file(&sel, c, &mut self.images);
                            self.toast(format!("ajouté à « {} »", board::board_name(self.board.rel.as_deref().unwrap_or(""))));
                        }
                        if ui.button(format!("{}  {}", crate::lucide::PALETTE, crate::i18n::t("board auto (couleurs)"))).on_hover_text(crate::i18n::t("Ajoute cette image et les refs aux couleurs proches au board")).clicked() {
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
                    if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::UPLOAD).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("importer des fichiers…")).clicked() {
                        self.pick_import(false);
                    }
                    if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::FOLDER_PLUS).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("lier un dossier…")).clicked() {
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
        self.assistant_ui(ui, false);
    }

    /// L'Assistant : dans son panneau, ou dans l'onglet Assistant de l'inspecteur (`embedded`).
    fn assistant_ui(&mut self, ui: &mut egui::Ui, embedded: bool) {
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
        let mut body = |ui: &mut egui::Ui| {
                ui.horizontal(|ui| {
                    if !embedded {
                        ui.label(caps(&crate::i18n::t("assistant")));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !embedded && ui.small_button("×").on_hover_text(crate::i18n::t("Ctrl+L")).clicked() {
                            self.assistant_open = false;
                        }
                        if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::ERASER).size(13.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("vider")).clicked() {
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
                                    if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::COPY).size(13.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("copier")).clicked() {
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
            };
        if embedded {
            body(ui);
        } else {
            crate::dock::show_ex(ui, "assistant", &crate::i18n::t("assistant"), crate::dock::Side::Right, 340.0, 240.0, egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(12), true, body);
        };
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
                        if ui.add_enabled(!self.chat.msgs.is_empty(), egui::Button::new(format!("{}  {}", crate::lucide::SAVE, crate::i18n::t("enregistrer en note")))).clicked() {
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
                    // Ctrl+V avec une image dans le presse-papiers : elle est jointe au message
                    // (du texte collé reste du texte : Word ou un navigateur mettent aussi une image à côté)
                    let pasted = ui.input(|i| {
                        let text = i.events.iter().any(|e| matches!(e, egui::Event::Paste(t) if !t.trim().is_empty()));
                        !text && i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)) || matches!(e, egui::Event::Key { key: Key::V, pressed: true, modifiers, .. } if modifiers.command))
                    });
                    if pasted && (r.has_focus() || !ui.ctx().egui_wants_keyboard_input()) {
                        extras::paste_image(&mut self.chat);
                    }
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
                    let mut edit_at: Option<usize> = None;
                    let mut regen = false;
                    for (k, m) in msgs.iter().enumerate() {
                        let (text, choices) = chat::split_choices(&m.text);
                        let others: Vec<&String> = m.attached.iter().filter(|n| !(m.thumbs.len() > 0 && crate::images::is_image(n))).collect();
                        let shown = if others.is_empty() { text } else { format!("{text}\n\n📎 {}", others.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")) };
                        msg_block(ui, m.role == chat::Role::User, &shown, &m.sources, m.error.as_deref(), None, &existing, &paths, &mut goto);
                        // Images jointes : aperçus (plus grands au survol)
                        if !m.thumbs.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                for t in &m.thumbs {
                                    if let Some(tex) = extras::thumb_texture(ui.ctx(), t) {
                                        let sz = tex.size_vec2();
                                        let h = 96.0;
                                        ui.add(egui::Image::new((tex.id(), egui::vec2(sz.x / sz.y * h, h))).corner_radius(4).sense(egui::Sense::hover())).on_hover_ui(|ui| {
                                            ui.add(egui::Image::new((tex.id(), sz)));
                                        });
                                    }
                                }
                            });
                        }
                        // Actions sur le message : modifier (question), copier, régénérer (dernière réponse)
                        if !self.chat.busy() {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 10.0;
                                let small = |s: &str| egui::Button::new(RichText::new(crate::i18n::t(s)).size(10.5).color(p.muted)).frame(false);
                                if m.role == chat::Role::User && ui.add(small("modifier")).on_hover_text(crate::i18n::t("Reprendre cette question pour la changer : la suite de la conversation est remplacée")).clicked() {
                                    edit_at = Some(k);
                                }
                                if ui.add(small("copier")).clicked() {
                                    ui.ctx().copy_text(m.text.clone());
                                }
                                if k == last && m.role == chat::Role::Assistant && ui.add(small("régénérer")).on_hover_text(crate::i18n::t("Redemander la même chose")).clicked() {
                                    regen = true;
                                }
                            });
                        }
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
                    // Modifier une question : elle revient dans le champ (avec ses pièces jointes) et la suite est retirée
                    let redo_at = if regen { (0..msgs.len()).rev().find(|&i| msgs[i].role == chat::Role::User) } else { None };
                    if let Some(k) = edit_at.or(redo_at) {
                        let m = self.chat.msgs[k].clone();
                        self.chat.msgs.truncate(k);
                        self.chat.input = m.text;
                        self.chat.attachments = m.sent;
                        if regen {
                            send = true;
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
        let send = send || std::mem::take(&mut self.chat_autosend);
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
                    if ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("fermer"))).clicked() {
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
                            if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::SQUARE).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("arrêter")).clicked() {
                                self.receiver = None;
                            }
                            if ui.button(format!("{}  {}", crate::lucide::FOLDER_OPEN, crate::i18n::t("ouvrir reçus/"))).clicked() {
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
                            if ui.small_button("×").on_hover_text(crate::i18n::t("retirer")).clicked() {
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

    /// Remplace partout où la recherche a trouvé, sauf dans les fichiers décochés ; un fichier ouvert est
    /// changé dans son onglet (et s'enregistre comme une saisie)
    /// Renomme (ou retire) une propriété de l'en-tête dans toutes les notes du coffre, notes ouvertes comprises
    /// Change une propriété d'une note : dans son onglet si elle est ouverte, sinon sur le disque.
    fn set_prop_of(&mut self, rel: &str, key: &str, value: &str) {
        let vals: Option<Vec<String>> = (!value.is_empty()).then(|| vec![value.to_string()]);
        if let Some(d) = self.docs.iter_mut().find(|d| d.rel == rel) {
            if let Content::Text(t) = &mut d.content {
                *t = vault::set_front(t, key, vals.as_deref(), false);
                d.dirty_since = Some(Instant::now());
                return;
            }
        }
        let root = self.root();
        match vault::read(&root, rel) {
            Ok(t) => {
                let r = vault::write(&root, rel, &vault::set_front(&t, key, vals.as_deref(), false));
                live::bump();
                self.report(r);
            }
            Err(e) => self.toast = Some((e, Instant::now())),
        }
    }

    fn rename_prop_everywhere(&mut self, from: &str, to: Option<&str>) {
        let rewrite = |text: &str| -> Option<String> {
            let (props, _) = vault::frontmatter(text)?;
            let (key, vals) = props.into_iter().find(|(k, _)| k.eq_ignore_ascii_case(from))?;
            let list = vals.len() > 1 || matches!(key.to_lowercase().as_str(), "tags" | "aliases");
            match to {
                // Renommer : la ligne de la clé change sur place (l'ordre des propriétés reste)
                Some(n) => {
                    let (_, end) = vault::frontmatter(text)?;
                    let mut out = String::with_capacity(text.len());
                    for (i, line) in text.split_inclusive('\n').enumerate() {
                        match line.split_once(':') {
                            Some((k, rest)) if i > 0 && i < end && k.trim() == key && !line.starts_with([' ', '\t', '-']) => out.push_str(&format!("{n}:{rest}")),
                            _ => out.push_str(line),
                        }
                    }
                    let _ = (&vals, list);
                    Some(out)
                }
                None => Some(vault::set_front(text, &key, None, list)),
            }
        };
        let root = self.root();
        let rels: Vec<String> = self.entries.iter().filter(|e| e.rel.ends_with(".md")).map(|e| e.rel.clone()).collect();
        let mut n = 0;
        for rel in rels {
            if let Some(d) = self.docs.iter_mut().find(|d| d.rel == rel) {
                if let Content::Text(t) = &mut d.content {
                    if let Some(new) = rewrite(t) {
                        *t = new;
                        d.dirty_since = Some(Instant::now());
                        n += 1;
                    }
                    continue;
                }
            }
            if let Ok(text) = vault::read(&root, &rel) {
                if let Some(new) = rewrite(&text) {
                    if vault::write(&root, &rel, &new).is_ok() {
                        n += 1;
                    }
                }
            }
        }
        self.toast(match to {
            Some(t) => format!("« {from} » → « {t} » · {n} {}", crate::i18n::t_mid("notes")),
            None => format!("« {from} » {} · {n} {}", crate::i18n::t_mid("retirée"), crate::i18n::t_mid("notes")),
        });
    }

    fn replace_everywhere(&mut self) {
        let (Some(res), Some(with)) = (self.search.results.clone(), self.search.replace.clone()) else { return };
        let (files, query) = {
            let r = res.lock().unwrap();
            (r.files.iter().filter(|f| !f.lines.is_empty()).map(|f| f.rel.clone()).collect::<Vec<_>>(), r.query.clone())
        };
        let root = self.root();
        let (mut n_files, mut n) = (0, 0);
        for rel in files.into_iter().filter(|r| !self.search.skip.contains(r)) {
            if let Some(d) = self.docs.iter_mut().find(|d| d.rel == rel) {
                if let Content::Text(t) = &mut d.content {
                    let (new, k) = search::replace_in(t, &query, &with);
                    if k > 0 {
                        *t = new;
                        d.dirty_since = Some(Instant::now());
                        n += k;
                        n_files += 1;
                    }
                    continue;
                }
            }
            if let Ok(text) = vault::read(&root, &rel) {
                let (new, k) = search::replace_in(&text, &query, &with);
                if k > 0 && vault::write(&root, &rel, &new).is_ok() {
                    n += k;
                    n_files += 1;
                }
            }
        }
        self.toast(format!("{n} {} · {n_files} {}", crate::i18n::t("remplacements"), crate::i18n::t("fichiers")));
        // La recherche se relance sur le nouveau texte
        self.search.last_query.clear();
        self.search.skip.clear();
    }

    fn search_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| {
            egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("/").color(p.accent).size(18.0).family(theme::bold()));
                    let open = self.search.replace.is_some();
                    let w = ui.available_width() - 34.0;
                    let r = ui.add(egui::TextEdit::singleline(&mut self.search.input).hint_text(crate::i18n::t("chercher dans tout le coffre (notes, code, texte, noms de fichiers)…")).font(egui::FontId::monospace(15.0)).frame(egui::Frame::NONE).desired_width(w));
                    if self.search.results.is_none() && self.search.input.is_empty() {
                        r.request_focus();
                    }
                    if ui.add(egui::Button::new(RichText::new(lucide::REPLACE).size(15.0)).selected(open)).on_hover_text(crate::i18n::t("Remplacer dans tous les fichiers (Ctrl+Maj+H)")).clicked() {
                        self.search.replace = if open { None } else { Some(String::new()) };
                    }
                });
                if self.search.replace.is_some() {
                    let mut go = false;
                    ui.horizontal(|ui| {
                        let Some(with) = self.search.replace.as_mut() else { return };
                        ui.label(RichText::new(lucide::CORNER_DOWN_RIGHT).color(p.accent).size(15.0));
                        ui.add(egui::TextEdit::singleline(with).hint_text(crate::i18n::t("remplacer par…")).font(egui::FontId::monospace(15.0)).frame(egui::Frame::NONE).desired_width(ui.available_width() - 34.0));
                        go = ui.add(egui::Button::new(RichText::new(lucide::REPLACE_ALL).size(15.0))).on_hover_text(crate::i18n::t("Tout remplacer dans les fichiers cochés")).clicked();
                    });
                    if go {
                        self.replace_everywhere();
                    }
                }
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
                        let replacing = self.search.replace.clone();
                        ui.horizontal(|ui| {
                            if replacing.is_some() && !f.lines.is_empty() {
                                let mut on = !self.search.skip.contains(&f.rel);
                                if ui.checkbox(&mut on, "").on_hover_text(crate::i18n::t("inclure ce fichier au remplacement")).changed() {
                                    if on {
                                        self.search.skip.remove(&f.rel);
                                    } else {
                                        self.search.skip.insert(f.rel.clone());
                                    }
                                }
                            }
                            let head = ui.add(egui::Button::new(RichText::new(&f.rel).family(theme::bold()).color(if f.name_match { p.accent } else { p.text })).frame(false));
                            if head.clicked() {
                                open = Some(f.rel.clone());
                            }
                        });
                        for l in &f.lines {
                            let r = ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                ui.label(RichText::new(format!("{:>5}  ", l.number)).color(p.muted).size(11.5));
                                let with = self.search.replace.as_ref().filter(|_| !self.search.skip.contains(&f.rel));
                                for (part, hit) in search::highlight(&l.text, &query) {
                                    let t = RichText::new(part).size(12.5);
                                    match (hit, with) {
                                        // Aperçu du remplacement : l'ancien barré, le nouveau à côté
                                        (true, Some(w)) => {
                                            ui.label(t.strikethrough().background_color(p.missing.linear_multiply(0.25)).color(p.muted));
                                            ui.label(RichText::new(w).size(12.5).background_color(p.accent.linear_multiply(0.3)).color(p.text));
                                        }
                                        (true, None) => {
                                            ui.label(t.background_color(p.accent.linear_multiply(0.3)).color(p.text));
                                        }
                                        _ => {
                                            ui.label(t.color(p.muted));
                                        }
                                    }
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
                    if ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("annuler"))).clicked() {
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
                            ui.label(format!("{} {n} {}", crate::i18n::t("Mettre"), crate::i18n::t_mid("éléments à la corbeille du coffre (.app/trash) ? Ctrl+Z dans l'explorateur les remet.")));
                        } else {
                            ui.label(format!("Mettre « {rel} » à la corbeille du coffre (.app/trash) ?"));
                        }
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new(crate::i18n::t("supprimer")).color(p.missing)).clicked() {
                            ok = true;
                        }
                        if ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("annuler"))).clicked() {
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
                    if ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("annuler"))).clicked() {
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
                    if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::FOLDER_INPUT).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("déplacer dans le coffre")).clicked() {
                        choice = Some("move");
                    }
                    ui.label(RichText::new(crate::i18n::t("Le fichier quitte son dossier d'origine. Aucun doublon.")).color(p.muted).size(11.0));
                    ui.add_space(6.0);
                    if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::COPY_PLUS).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("copier dans le coffre")).clicked() {
                        choice = Some("copy");
                    }
                    ui.label(RichText::new(crate::i18n::t("L'original reste en place : le fichier prend deux fois la place.")).color(p.muted).size(11.0));
                    ui.add_space(8.0);
                    ui.checkbox(&mut remember, crate::i18n::t("retenir ce choix (modifiable dans les réglages)"));
                    if ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("annuler"))).clicked() {
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
                        if !st.checking && st.progress.is_none() && ui.add(egui::Button::new(egui::RichText::new(crate::lucide::CHECK).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("vérifier")).clicked() {
                            up.check(self.github_token.clone(), ctx);
                        }
                        if ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("fermer"))).clicked() {
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

    /// Un fichier de l'explorateur lâché ailleurs que sur une zone qui le prend déjà
    /// (board, timeline, dossier) : la scène ou le montage ouverts le reçoivent, sinon il s'ouvre.
    /// Pendant le glissement, son nom suit le pointeur.
    fn internal_drop(&mut self, ctx: &egui::Context) {
        let Some(rel) = egui::DragAndDrop::payload::<String>(ctx) else { return };
        let Some(at) = ctx.input(|i| i.pointer.latest_pos()) else { return };
        let p = theme::pal();
        let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("dnd-tag")));
        let g = painter.layout_no_wrap(name, egui::FontId::proportional(12.0), p.text);
        let r = egui::Rect::from_min_size(at + egui::vec2(14.0, 10.0), g.size() + egui::vec2(12.0, 6.0));
        painter.rect_filled(r, 3.0, p.panel);
        painter.rect_stroke(r, 3.0, egui::Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
        painter.galley(r.min + egui::vec2(6.0, 3.0), g, p.text);
        if !ctx.input(|i| i.pointer.any_released()) || self.explorer_rect.is_some_and(|r| r.contains(at)) {
            return;
        }
        let Some(rel) = egui::DragAndDrop::take_payload::<String>(ctx) else { return };
        let rel = (*rel).clone();
        let abs = std::path::Path::new(&self.root()).join(&rel);
        if self.view() == View::Files {
            match self.active.and_then(|i| self.docs.get_mut(i)).map(|d| &mut d.content) {
                Some(Content::Montage(m)) if video::is_video(&rel) || motion::is_compo(&rel) || images::has_preview(&rel) || [".mp3", ".wav", ".flac", ".ogg", ".m4a", ".aac", ".opus"].iter().any(|e| rel.to_lowercase().ends_with(e)) => {
                    m.add.push(abs);
                    return;
                }
                Some(Content::Model(v)) if v.is_scene() => {
                    let n = rel.to_lowercase();
                    if model3d::is_model(&n) || [".png", ".jpg", ".jpeg", ".webp", ".aryimg"].iter().any(|e| n.ends_with(e)) {
                        v.scene_add.push(abs);
                        return;
                    }
                }
                _ => {}
            }
        }
        self.actions.push(Action::Open(rel));
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
                if !f.is_dir() && mode != "link" {
                    sources::record(&root, &rel, &f.to_string_lossy());
                }
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

/// Ce qui est lâché sur `r`, s'il est du bon type. `dnd_release_payload` d'egui vide le
/// glissement même quand le type ne correspond pas : la première zone de dépôt qui attend un
/// autre type avalait alors le fichier venu de l'explorateur.
/// Message bref pour l'utilisateur, depuis n'importe quel éditeur (affiché en bas de l'écran).
static NOTICES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
pub fn notify(msg: impl Into<String>) {
    NOTICES.lock().unwrap_or_else(|e| e.into_inner()).push(msg.into());
}

/// Fichier de l'explorateur lâché sur `r`, seulement s'il convient (`fits` reçoit son chemin dans le coffre).
pub fn drop_rel(r: &egui::Response, fits: impl Fn(&str) -> bool) -> Option<String> {
    let rel = egui::DragAndDrop::payload::<String>(&r.ctx)?;
    if !fits(&rel) {
        return None;
    }
    drop_of::<String>(r).map(|x| (*x).clone())
}

pub fn drop_of<T: std::any::Any + Send + Sync>(r: &egui::Response) -> Option<std::sync::Arc<T>> {
    if egui::DragAndDrop::has_payload_of_type::<T>(&r.ctx) {
        r.dnd_release_payload::<T>()
    } else {
        None
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Les entrées de menu qui reviennent à un raccourci : on le rejoue, chaque éditeur réagit comme au clavier
        for (m, k) in std::mem::take(&mut self.inject) {
            ctx.input_mut(|i| {
                i.modifiers = m;
                i.events.push(egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m });
                match (m.command, k) {
                    (true, egui::Key::C) => i.events.push(egui::Event::Copy),
                    (true, egui::Key::X) => i.events.push(egui::Event::Cut),
                    _ => {}
                }
            });
        }
        settings::set_unit(&self.prefs.unit);
        // Couleur de l'espace courant d'après le thème (il peut changer dans les réglages)
        theme::set_workspace_accent(workspace::accent_of(self.ws.cur()));
        self.shortcuts(&ctx);
        self.cloud_tick();
        // Temps passé : le fichier ouvert compte tant qu'on travaille
        {
            let (input, focused) = ctx.input(|i| (!i.events.is_empty() || i.pointer.is_moving(), i.viewport().focused.unwrap_or(true)));
            let rel = self.current().map(|d| d.rel.clone()).or_else(|| self.board.rel.clone().filter(|_| self.view() == View::Boards));
            let root = self.root();
            self.time.tick(&root, rel.as_deref(), input, focused);
            if ctx.input(|i| i.viewport().close_requested()) {
                self.time.save();
            }
        }

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

        // Polices changées dans les réglages : rechargées
        if theme::fonts() != self.prefs.fonts() {
            theme::install_fonts(&ctx, &self.prefs.fonts());
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
                self.menubar(ui);
                let a = ui.available_rect_before_wrap();
                self.topbar(ui);
                let b = ui.available_rect_before_wrap();
                self.statusbar(ui);
                let c = ui.available_rect_before_wrap();
                self.ws_bar(ui);
                let d = ui.available_rect_before_wrap();
                self.zones_next.push(("top", egui::Rect::from_min_max(a.min, egui::pos2(a.max.x, b.min.y))));
                self.zones_next.push(("status", egui::Rect::from_min_max(egui::pos2(b.min.x, c.max.y), b.max)));
                self.zones_next.push(("pages", egui::Rect::from_min_max(c.min, egui::pos2(d.min.x, c.max.y))));
                self.explorer_rect = None;
                if std::mem::take(&mut self.reset_layout) {
                    self.tree = None;
                    dock::reset();
                }
                self.tiles(ui);
            }
            self.internal_drop(&ctx);
            // Fenêtres détachées : places retenues sur le disque, zone de rattachement
            dock::tick(&ctx);
            if let Some(m) = NOTICES.lock().unwrap_or_else(|e| e.into_inner()).drain(..).last() {
                self.toast(m);
            }
            if self.compact {
                // Mode compact : pas de disposition, juste le document
                if self.terminal.is_some() {
                    self.terminal_panel(ui);
                }
                if self.assistant_open {
                    self.assistant_panel(ui);
                }
                self.main_view(ui);
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
            self.time_report_ui(&ctx);
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
        (p.success, format!("en marche · {} modèle(s)", st.models.len()))
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
                if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::TRASH_2).size(13.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Libère l'espace disque")).clicked() {
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
            if ui.button(format!("{}  {}", crate::lucide::DOWNLOAD, crate::i18n::t("installer ollama (sans admin)"))).clicked() {
                local.install(&s.url, ctx);
            }
        }
        if st.binary.is_some() && !st.running && ui.button(format!("{}  {}", crate::lucide::PLAY, crate::i18n::t("démarrer"))).clicked() {
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
    let (who, color) = if user { (crate::i18n::t("vous"), p.muted) } else { ("Aryadne".to_string(), p.accent) };
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


/// Ce que la disposition en cases demande à l'application : le contenu et le titre de chaque case.
struct Slideshow {
    images: Vec<String>,
    at: usize,
    secs: f32,
    since: Instant,
    paused: Option<Instant>,
}

struct Panes<'a> {
    app: &'a mut App,
    /// La case de l'éditeur principal : seule dans ses onglets, elle n'a pas de barre « Document »
    main_tile: Option<egui_tiles::TileId>,
    alone: HashSet<egui_tiles::TileId>,
}

impl egui_tiles::Behavior<String> for Panes<'_> {
    fn pane_ui(&mut self, ui: &mut egui::Ui, _tile: egui_tiles::TileId, pane: &mut String) -> egui_tiles::UiResponse {
        let id = pane.clone();
        if id.starts_with("ed:") {
            // L'éditeur y dessine lui-même son panneau : on retient juste la place
            layout::set_rect(&id, ui.max_rect());
            if !self.app.offered_now.contains_key(&id) {
                ui.label(egui::RichText::new(crate::i18n::t("Ce panneau appartient à un autre éditeur.")).color(theme::pal().muted).size(11.0));
            }
            return egui_tiles::UiResponse::None;
        }
        let header = id != layout::MAIN && self.alone.contains(&_tile);
        ui.push_id(("pane", &id), |ui| {
            let mut rect = ui.max_rect();
            let p = theme::pal();
            ui.painter().rect_filled(rect, 0.0, p.panel);
            let full = rect;
            if header {
                rect.min.y += 34.0;
            }
            {
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)));
                child.set_clip_rect(ui.clip_rect().intersect(rect));
                dock::inline(&id, || self.app.pane_body(&mut child, &id));
            }
            // La poignée discrète en haut de la case : un clic (ou la tirer dehors) sort le panneau dans sa fenêtre
            let ask = dock::grip(ui, full, &id, &App::pane_title(&id), false);
            if let dock::Grip::Detach(at) = ask {
                let c = ui.ctx().clone();
                dock::detach_at(&c, &id, Some(at.unwrap_or_else(|| dock::desk(&c, full.min))), [full.width().max(260.0), full.height().max(200.0)]);
            }
            // L'en-tête après le contenu : ses boutons passent au-dessus
            let rect = full;
            if header {
                // En-tête du panneau, comme la maquette : titre, fermer ; clic droit : le sortir
                let h = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 34.0));
                ui.painter().hline(h.x_range(), h.bottom() - 0.5, egui::Stroke::new(1.0, p.line));
                // Le fond de l'en-tête d'abord : les onglets et la croix, enregistrés après, passent au-dessus
                let hr = ui.interact(h, ui.scope_id().with(("pane-head", &id)), egui::Sense::click());
                hr.context_menu(|ui| {
                    if ui.button(format!("{}  {}", crate::lucide::EXTERNAL_LINK, crate::i18n::t("sortir dans sa propre fenêtre"))).clicked() {
                        dock::set_side(&id, dock::Side::Window);
                        ui.close();
                    }
                    if ui.button(format!("{}  {}", crate::lucide::ROTATE_CCW, crate::i18n::t("réinitialiser la disposition"))).clicked() {
                        self.app.reset_layout = true;
                        ui.close();
                    }
                });
                if id == "side" {
                    // L'inspecteur : des onglets fixes à la place du titre (Plan, Liens, Assistant)
                    let mut x = h.left() + 6.0;
                    for (k, label) in [(0u8, crate::i18n::t("structure")), (1, crate::i18n::t("liens")), (2, format!("{} {}", lucide::SPARKLES, crate::i18n::t("assistant")))] {
                        let on = self.app.insp_tab == k;
                        let g = ui.painter().layout_no_wrap(label, egui::FontId::proportional(12.5), if on { p.text } else { p.muted });
                        let r = egui::Rect::from_min_size(egui::pos2(x, h.center().y - 12.0), egui::vec2(g.size().x + 16.0, 24.0));
                        let resp = ui.interact(r, ui.scope_id().with(("insp-tab", k)), egui::Sense::click());
                        if on || resp.hovered() {
                            ui.painter().rect(r, 2.0, p.raised, egui::Stroke::new(1.0, if on { p.line } else { egui::Color32::TRANSPARENT }), egui::StrokeKind::Inside);
                        }
                        let col = if on || resp.hovered() { p.text } else { p.muted };
                        ui.painter().galley_with_override_text_color(egui::pos2(r.left() + 8.0, r.center().y - g.size().y / 2.0), g, col);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            self.app.insp_tab = k;
                        }
                        x = r.right() + 2.0;
                    }
                } else {
                    ui.painter().text(egui::pos2(h.left() + 12.0, h.center().y), egui::Align2::LEFT_CENTER, App::pane_title(&id), egui::FontId::new(13.0, theme::bold()), p.text);
                }
                {
                    // Sortir le panneau dans sa propre fenêtre, d'un clic
                    let dr = egui::Rect::from_center_size(egui::pos2(h.right() - 44.0, h.center().y), egui::vec2(24.0, 24.0));
                    let dresp = ui.interact(dr, ui.scope_id().with(("pane-out", &id)), egui::Sense::click());
                    if dresp.hovered() {
                        ui.painter().rect_filled(dr, 2.0, p.raised);
                    }
                    ui.painter().text(dr.center(), egui::Align2::CENTER_CENTER, lucide::EXTERNAL_LINK, egui::FontId::proportional(12.5), if dresp.hovered() { p.text } else { p.muted });
                    if dresp.on_hover_text(crate::i18n::t("détacher dans sa propre fenêtre")).clicked() {
                        let c = ui.ctx().clone();
                        dock::detach_at(&c, &id, Some(dock::desk(&c, rect.min)), [rect.width().max(260.0), rect.height().max(200.0)]);
                    }
                }
                if !id.starts_with("ed:") {
                    let xr = egui::Rect::from_center_size(egui::pos2(h.right() - 18.0, h.center().y), egui::vec2(24.0, 24.0));
                    let xresp = ui.interact(xr, ui.scope_id().with(("pane-close", &id)), egui::Sense::click());
                    if xresp.hovered() {
                        ui.painter().rect_filled(xr, 2.0, p.raised);
                    }
                    ui.painter().text(xr.center(), egui::Align2::CENTER_CENTER, lucide::X, egui::FontId::proportional(13.0), if xresp.hovered() { p.text } else { p.muted });
                    if xresp.on_hover_text(crate::i18n::t("fermer")).clicked() {
                        match id.as_str() {
                            "explorer" => self.app.explorer_open = false,
                            "terminal" => self.app.terminal = None,
                            "assistant" => self.app.assistant_open = false,
                            "side" => self.app.side_hidden = true,
                            v => {
                                if let Some(k) = v.strip_prefix("view:") {
                                    self.app.view_panels.retain(|x| x != k);
                                }
                            }
                        }
                    }
                }
            }
        });
        egui_tiles::UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &String) -> egui::WidgetText {
        App::pane_title(pane).into()
    }

    fn is_tab_closable(&self, tiles: &egui_tiles::Tiles<String>, tile: egui_tiles::TileId) -> bool {
        tiles.get_pane(&tile).is_some_and(|p| p != layout::MAIN)
    }

    fn on_tab_close(&mut self, tiles: &mut egui_tiles::Tiles<String>, tile: egui_tiles::TileId) -> bool {
        let Some(p) = tiles.get_pane(&tile).cloned() else { return false };
        match p.as_str() {
            "explorer" => self.app.explorer_open = false,
            "terminal" => self.app.terminal = None,
            "assistant" => self.app.assistant_open = false,
            "side" => self.app.side_hidden = true,
            v => {
                if let Some(k) = v.strip_prefix("view:") {
                    self.app.view_panels.retain(|x| x != k);
                    return true;
                }
                if v.starts_with("ed:") {
                    // Le panneau retourne dans son éditeur
                    return true;
                }
            }
        }
        // Les panneaux de base gardent leur case (cachée) : ils reviennent au même endroit
        false
    }

    fn on_tab_button(&mut self, tiles: &mut egui_tiles::Tiles<String>, tile: egui_tiles::TileId, button: egui::Response) -> egui::Response {
        if let Some(p) = tiles.get_pane(&tile).cloned() {
            button.context_menu(|ui| {
                if p != layout::MAIN && ui.button(format!("{}  {}", crate::lucide::EXTERNAL_LINK, crate::i18n::t("sortir dans sa propre fenêtre"))).clicked() {
                    dock::set_side(&p, dock::Side::Window);
                    ui.close();
                }
                if ui.button(format!("{}  {}", crate::lucide::ROTATE_CCW, crate::i18n::t("réinitialiser la disposition"))).clicked() {
                    self.app.reset_layout = true;
                    ui.close();
                }
            });
        }
        button
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        26.0
    }

    /// À droite de la rangée d'onglets : sortir l'onglet affiché dans sa propre fenêtre
    fn top_bar_right_ui(&mut self, tiles: &egui_tiles::Tiles<String>, ui: &mut egui::Ui, _tile_id: egui_tiles::TileId, tabs: &egui_tiles::Tabs, _scroll_offset: &mut f32) {
        let Some(pane) = tabs.active.and_then(|a| tiles.get_pane(&a).cloned()) else { return };
        let p = theme::pal();
        if ui.add(egui::Button::new(egui::RichText::new(lucide::EXTERNAL_LINK).size(12.0).color(p.muted)).frame(false)).on_hover_text(format!("{} · {}", crate::i18n::t("détacher dans sa propre fenêtre"), App::pane_title(&pane))).clicked() {
            let c = ui.ctx().clone();
            let at = ui.max_rect().left_bottom();
            dock::detach_at(&c, &pane, Some(dock::desk(&c, at)), [560.0, 440.0]);
        }
    }

    /// L'éditeur principal seul dans sa case : pas de barre d'onglets au-dessus (les documents ont déjà la leur)
    fn is_tab_bar_visible(&self, _tile_id: egui_tiles::TileId, tabs: &egui_tiles::Tabs) -> bool {
        !tabs.children.iter().any(|c| self.alone.contains(c))
    }

    /// Panneaux nets, séparés par une ligne de 1 px (la charte : pas de coins arrondis entre les cases)
    fn gap_width(&self, _style: &egui::Style) -> f32 {
        1.0
    }

    /// La ligne de séparation s'allume au survol pour montrer qu'on peut la tirer.
    fn resize_stroke(&self, _style: &egui::Style, state: egui_tiles::ResizeState) -> egui::Stroke {
        match state {
            egui_tiles::ResizeState::Idle => egui::Stroke::new(1.0, theme::pal().line),
            egui_tiles::ResizeState::Hovering => egui::Stroke::new(2.0, theme::pal().muted),
            egui_tiles::ResizeState::Dragging => egui::Stroke::new(2.0, theme::pal().accent),
        }
    }

    fn min_size(&self) -> f32 {
        140.0
    }

    fn tab_bar_color(&self, _visuals: &egui::Visuals) -> egui::Color32 {
        theme::pal().panel
    }

    fn tab_bg_color(&self, _visuals: &egui::Visuals, _tiles: &egui_tiles::Tiles<String>, _tile: egui_tiles::TileId, state: &egui_tiles::TabState) -> egui::Color32 {
        if state.active { theme::pal().bg } else { theme::pal().panel }
    }

    fn tab_text_color(&self, _visuals: &egui::Visuals, _tiles: &egui_tiles::Tiles<String>, _tile: egui_tiles::TileId, state: &egui_tiles::TabState) -> egui::Color32 {
        if state.active { theme::pal().text } else { theme::pal().muted }
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        egui_tiles::SimplificationOptions { all_panes_must_have_tabs: true, ..Default::default() }
    }
}

/// Mise en forme d'une note pendant qu'on l'écrit (proche du rendu final, comme la maquette) :
/// titres plus grands en semi-gras, `#`, `**`, `- [ ]` discrets, [[liens]] colorés, `code` à chasse fixe.
/// Le texte reste le markdown tel quel : chaque caractère garde sa place, seule l'apparence change.
fn md_layout(text: &str, fs: f32) -> egui::text::LayoutJob {
    let p = theme::pal();
    let mut job = egui::text::LayoutJob::default();
    let fmt = |size: f32, bold: bool, color: egui::Color32| egui::TextFormat {
        font_id: egui::FontId::new(size, if bold { theme::bold() } else { theme::notes() }),
        color,
        line_height: Some(size * 1.6),
        ..Default::default()
    };
    let mut in_code = false;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches('\n');
        let nl = &line[body.len()..];
        let trimmed = body.trim_start();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            job.append(line, 0.0, egui::TextFormat { font_id: egui::FontId::monospace(fs - 1.0), color: p.muted, ..Default::default() });
            continue;
        }
        if in_code {
            job.append(line, 0.0, egui::TextFormat { font_id: egui::FontId::monospace(fs - 1.0), color: p.text, background: p.panel, ..Default::default() });
            continue;
        }
        // Titres : les # en gris, le texte en grand
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            let size = match hashes {
                1 => fs + 7.0,
                2 => fs + 3.0,
                _ => fs,
            };
            let lead = body.len() - trimmed.len() + hashes + 1;
            job.append(&body[..lead], 0.0, fmt(size, false, p.muted));
            inline(&mut job, &body[lead..], size, true, &fmt);
            job.append(nl, 0.0, fmt(size, false, p.text));
            continue;
        }
        // Listes et cases à cocher : la marque en gris (case cochée : texte estompé)
        let mut rest = body;
        for mark in ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "- ", "* ", "> "] {
            if let Some(r) = trimmed.strip_prefix(mark) {
                let lead = body.len() - r.len();
                job.append(&body[..lead], 0.0, fmt(fs, false, if mark.contains('x') || mark.contains('X') { p.success } else { p.muted }));
                rest = &body[lead..];
                if mark.contains("[x]") || mark.contains("[X]") {
                    job.append(rest, 0.0, egui::TextFormat { strikethrough: egui::Stroke::new(1.0, p.muted), ..fmt(fs, false, p.muted) });
                    rest = "";
                }
                break;
            }
        }
        inline(&mut job, rest, fs, false, &fmt);
        job.append(nl, 0.0, fmt(fs, false, p.text));
    }
    return job;

    /// Dans une ligne : [[liens]], `code`, **gras** ; le reste en texte normal.
    fn inline(job: &mut egui::text::LayoutJob, s: &str, size: f32, bold: bool, fmt: &dyn Fn(f32, bool, egui::Color32) -> egui::TextFormat) {
        let p = theme::pal();
        let mut i = 0;
        let b = s.as_bytes();
        let mut plain = 0;
        let flush = |job: &mut egui::text::LayoutJob, from: usize, to: usize| {
            if to > from {
                job.append(&s[from..to], 0.0, fmt(size, bold, p.text));
            }
        };
        while i < b.len() {
            let (open, close, kind) = if s[i..].starts_with("[[") {
                ("[[", "]]", 0)
            } else if s[i..].starts_with("**") {
                ("**", "**", 1)
            } else if b[i] == b'`' {
                ("`", "`", 2)
            } else {
                i += s[i..].chars().next().map_or(1, |c| c.len_utf8());
                continue;
            };
            let Some(end) = s[i + open.len()..].find(close).map(|e| i + open.len() + e) else {
                i += open.len();
                continue;
            };
            flush(job, plain, i);
            let inner = &s[i + open.len()..end];
            job.append(open, 0.0, fmt(size, false, p.muted));
            match kind {
                0 => job.append(inner, 0.0, fmt(size, bold, p.link)),
                1 => job.append(inner, 0.0, fmt(size, true, p.text)),
                _ => job.append(inner, 0.0, egui::TextFormat { font_id: egui::FontId::monospace(size - 1.5), color: p.text, background: p.panel, ..Default::default() }),
            }
            job.append(close, 0.0, fmt(size, false, p.muted));
            i = end + close.len();
            plain = i;
        }
        flush(job, plain, b.len());
    }
}

#[cfg(test)]
mod tests_md_layout {
    #[test]
    fn mise_en_forme_garde_le_texte() {
        for t in [
            "# Titre\n\n## À faire\n- [ ] tâche [[Lien]]\n- [x] fait\n\nDu **gras**, du `code` et un [[lien\n",
            "```rust\nfn main() {}\n```\npas fini **gras\n> citation\n* puce\n### é è ü\n",
            "",
            "#pas un titre\n[[",
        ] {
            assert_eq!(super::md_layout(t, 15.0).text, t);
        }
    }
}

/// Boutons sans cadre (barres flottantes, en-têtes) : fond seulement au survol
fn flat_buttons(ui: &mut egui::Ui) {
    let p = theme::pal();
    let v = &mut ui.style_mut().visuals.widgets;
    v.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    v.inactive.bg_stroke = egui::Stroke::NONE;
    v.hovered.weak_bg_fill = p.raised;
    v.hovered.bg_stroke = egui::Stroke::NONE;
    v.open.weak_bg_fill = p.raised;
}

/// Contour en pointillés (zone où déposer)
fn dashed_rect(painter: &egui::Painter, r: egui::Rect, color: egui::Color32) {
    let pts = vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    painter.extend(egui::Shape::dashed_line(&pts, egui::Stroke::new(1.0, color), 4.0, 3.0));
}
