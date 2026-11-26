//! Panneaux ancrables : chaque panneau (explorateur, structure, assistant, terminal…)
//! peut être glissé par sa poignée vers un bord de la fenêtre (gauche, droite, bas)
//! ou lâché au milieu pour devenir une fenêtre flottante redimensionnable.
//! La disposition est gardée dans `~/.config/aryadne/dock.json`.

use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, Sense, Stroke, Ui};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum Side {
    Left,
    Right,
    Bottom,
    Float,
    /// Détaché dans sa propre fenêtre du système
    Window,
    /// Panneau d'un éditeur, à sa place dans l'éditeur (ou dans sa case de la disposition)
    Home,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub struct Place {
    pub side: Side,
    pub pos: [f32; 2],
    pub size: [f32; 2],
    /// Fenêtre détachée : son coin sur le bureau (tous écrans confondus). `pos` reste relatif à la fenêtre principale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abs: Option<[f32; 2]>,
}

impl Place {
    pub fn new(side: Side, pos: [f32; 2], size: [f32; 2]) -> Place {
        Place { side, pos, size, abs: None }
    }
}

static STATE: Mutex<Option<HashMap<String, Place>>> = Mutex::new(None);

thread_local! {
    /// Panneau dessiné en ce moment dans une case de la disposition (il remplit la case, sans en-tête)
    static INLINE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    /// Panneaux rangés dans la disposition en cases : ils ne se dessinent que dans leur case
    static MANAGED: std::cell::RefCell<std::collections::HashSet<String>> = std::cell::RefCell::new(Default::default());
}

/// Dessine `f` comme le contenu de la case du panneau `id`.
pub fn inline<R>(id: &str, f: impl FnOnce() -> R) -> R {
    INLINE.with(|c| *c.borrow_mut() = Some(id.to_string()));
    let r = f();
    INLINE.with(|c| *c.borrow_mut() = None);
    r
}

/// Les panneaux que la disposition en cases dessine elle-même.
pub fn set_managed(ids: std::collections::HashSet<String>) {
    MANAGED.with(|m| *m.borrow_mut() = ids);
}

/// Où se trouve un panneau (None : jamais placé).
pub fn side_of(id: &str) -> Option<Side> {
    with(|m| m.get(id).map(|p| p.side))
}

/// Change la place d'un panneau (par exemple : le sortir dans sa propre fenêtre).
pub fn set_side(id: &str, side: Side) {
    let mut p = place(id, side, 300.0);
    p.side = side;
    set(id, p);
}

fn path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("aryadne").join("dock.json"))
}

fn with<R>(f: impl FnOnce(&mut HashMap<String, Place>) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(|| path().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default());
    f(m)
}

fn save() {
    let data = with(|m| serde_json::to_vec_pretty(m).ok());
    if let (Some(p), Some(d)) = (path(), data) {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = crate::vault::atomic_write(p, d);
    }
}

/// La disposition en cours (pour l'enregistrer dans un espace de travail).
pub fn export() -> HashMap<String, Place> {
    with(|m| m.clone())
}

/// Remplace la disposition (en changeant d'espace de travail).
pub fn import(places: HashMap<String, Place>) {
    with(|m| *m = places);
    // Les fenêtres déjà ouvertes prennent la place que leur donne la nouvelle disposition
    LIVE.with(|l| l.borrow_mut().clear());
    save();
}

/// Place un panneau à un endroit précis (un onglet qu'on vient de détacher, par exemple).
pub fn put(id: &str, side: Side, pos: [f32; 2], size: [f32; 2]) {
    set(id, Place::new(side, pos, size));
    LIVE.with(|l| l.borrow_mut().remove(id));
}

/// Le point `at` est-il hors de la fenêtre de l'application ?
pub fn outside(ctx: &egui::Context, at: Pos2) -> bool {
    !ctx.content_rect().expand(4.0).contains(at)
}

/// Remet tous les panneaux à leur place d'origine.
pub fn reset() {
    with(|m| m.clear());
    LIVE.with(|l| l.borrow_mut().clear());
    save();
}

fn place(id: &str, default: Side, size: f32) -> Place {
    with(|m| *m.entry(id.to_string()).or_insert(Place::new(default, [200.0, 120.0], [size.max(260.0), 420.0])))
}

/// Taille d'un panneau flottant ramenée dans des bornes raisonnables pour l'écran.
fn float_size(screen: Rect, size: [f32; 2], min: f32) -> egui::Vec2 {
    vec2(size[0].clamp(min.max(180.0), (screen.width() * 0.8).max(min.max(180.0))), size[1].clamp(140.0, (screen.height() * 0.8).max(140.0)))
}

/// Largeur maximale d'un panneau ancré sur un côté : la zone centrale garde toujours de la place.
pub fn max_side(screen: Rect, min: f32) -> f32 {
    (screen.width() * 0.4).max(min + 40.0)
}

fn set(id: &str, p: Place) {
    with(|m| m.insert(id.to_string(), p));
    save();
}

/// Où un panneau atterrirait si on le lâchait à `at`.
fn drop_side(screen: Rect, at: Pos2) -> Side {
    let edge = 90.0;
    if !screen.expand(4.0).contains(at) {
        // Lâché hors de l'application : le panneau devient une vraie fenêtre
        Side::Window
    } else if at.x < screen.left() + edge {
        Side::Left
    } else if at.x > screen.right() - edge {
        Side::Right
    } else if at.y > screen.bottom() - edge {
        Side::Bottom
    } else {
        Side::Float
    }
}

fn zone(screen: Rect, s: Side) -> Rect {
    match s {
        Side::Left => Rect::from_min_size(screen.min, vec2(260.0, screen.height())),
        Side::Right => Rect::from_min_max(pos2(screen.right() - 300.0, screen.top()), screen.max),
        Side::Bottom => Rect::from_min_max(pos2(screen.left(), screen.bottom() - 220.0), screen.max),
        Side::Float | Side::Window | Side::Home => Rect::NOTHING,
    }
}

/// Panneaux fermés par leur bouton × depuis la dernière lecture (l'appelant décide quoi en faire).
static CLOSED: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Panneau qu'on tient par son en-tête : identifiant et décalage entre le pointeur et le coin du panneau.
static HELD: Mutex<Option<(String, egui::Vec2)>> = Mutex::new(None);

/// Le panneau `id` a-t-il été fermé par son bouton × ? (vrai une seule fois)
pub fn closed(id: &str) -> bool {
    let mut g = CLOSED.lock().unwrap_or_else(|e| e.into_inner());
    match g.iter().position(|x| x == id) {
        Some(i) => {
            g.remove(i);
            true
        }
        None => false,
    }
}

/// En-tête d'un panneau : son nom, de quoi le déplacer (on l'attrape n'importe où sur l'en-tête),
/// un menu ▾ pour choisir sa place, et × pour le fermer.
fn header(ui: &mut Ui, id: &str, title: &str, mut pl: Place, closable: bool, panel_min: Pos2) {
    let p = crate::theme::pal();
    let h = 22.0;
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click_and_drag());
    let held_me = HELD.lock().ok().and_then(|g| g.as_ref().map(|x| x.0 == id)).unwrap_or(false);
    let hot = resp.hovered() || held_me;
    let painter = ui.painter_at(r);
    painter.rect_filled(r, 0.0, if hot { p.accent.gamma_multiply(0.10) } else { p.bg.gamma_multiply(0.6) });
    painter.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, p.line));
    painter.text(pos2(r.left() + 8.0, r.center().y), egui::Align2::LEFT_CENTER, "⠿", egui::FontId::proportional(12.0), if hot { p.accent } else { p.muted });
    painter.text(pos2(r.left() + 22.0, r.center().y), egui::Align2::LEFT_CENTER, title, egui::FontId::new(12.0, crate::theme::bold()), if hot { p.text } else { p.muted });
    // Boutons à droite : × fermer, ▾ place
    let mut bx = r.right() - 4.0;
    let mut button = |glyph: &str, tip: &str| -> bool {
        let br = Rect::from_min_max(pos2(bx - 20.0, r.top() + 1.0), pos2(bx, r.bottom() - 1.0));
        bx -= 22.0;
        let b = ui.interact(br, ui.scope_id().with((id, glyph)), Sense::click());
        if b.hovered() {
            ui.painter().rect_filled(br, 3.0, p.line);
        }
        ui.painter().text(br.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(12.0), if b.hovered() { p.text } else { p.muted });
        b.on_hover_text(crate::i18n::t(tip)).clicked()
    };
    if closable && button("×", "fermer ce panneau") {
        CLOSED.lock().unwrap_or_else(|e| e.into_inner()).push(id.to_string());
    }
    // Le sortir d'un clic dans sa propre fenêtre
    if button(crate::lucide::EXTERNAL_LINK, "détacher dans sa propre fenêtre") {
        let c = ui.ctx().clone();
        let size = ui.max_rect().size();
        detach_at(&c, id, Some(desk(&c, panel_min)), [size.x.max(260.0), size.y.max(200.0)]);
        return;
    }
    // ▾ : la place du panneau, comme le clic droit
    let mr = Rect::from_min_max(pos2(bx - 20.0, r.top() + 1.0), pos2(bx, r.bottom() - 1.0));
    let screen = ui.ctx().content_rect();
    if resp.hovered() && !held_me {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    if resp.drag_started() {
        let at = ui.input(|i| i.pointer.press_origin()).unwrap_or(r.left_top());
        *HELD.lock().unwrap_or_else(|e| e.into_inner()) = Some((id.to_string(), at - panel_min));
    }
    if held_me && resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        if let Some(at) = ui.ctx().pointer_latest_pos() {
            let off = HELD.lock().ok().and_then(|g| g.as_ref().map(|x| x.1)).unwrap_or(vec2(20.0, 8.0));
            if pl.side == Side::Float {
                // Le panneau suit le pointeur là où on l'a attrapé
                pl.pos = [at.x - off.x, at.y - off.y];
                with(|m| m.insert(id.to_string(), pl));
            }
            // Les zones d'arrivée : faibles partout, vives sous le pointeur
            let target = drop_side(screen, at);
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("dock-preview")));
            for s in [Side::Left, Side::Right, Side::Bottom] {
                let z = zone(screen, s);
                if s != target {
                    painter.rect_stroke(z.shrink(6.0), 6.0, Stroke::new(1.0, p.accent.gamma_multiply(0.35)), egui::StrokeKind::Inside);
                }
            }
            let z = match target {
                Side::Float if pl.side != Side::Float => Rect::from_min_size(at - vec2(off.x.min(60.0), off.y.min(11.0)), float_size(screen, pl.size, 180.0)),
                Side::Float | Side::Window | Side::Home => Rect::NOTHING,
                s => zone(screen, s),
            };
            if z.is_positive() {
                painter.rect_filled(z, 6.0, p.accent.linear_multiply(0.15));
                painter.rect_stroke(z, 6.0, Stroke::new(2.0, p.accent), egui::StrokeKind::Inside);
                painter.text(z.center(), egui::Align2::CENTER_CENTER, title, egui::FontId::proportional(14.0), p.accent);
            }
            let hint = match target {
                Side::Left => "lâcher : ancrer à gauche",
                Side::Right => "lâcher : ancrer à droite",
                Side::Bottom => "lâcher : ancrer en bas",
                Side::Float => "lâcher : panneau flottant",
                Side::Window | Side::Home => "lâcher : fenêtre à part",
            };
            painter.text(at + vec2(16.0, 18.0), egui::Align2::LEFT_TOP, crate::i18n::t(hint), egui::FontId::proportional(11.0), p.accent);
        }
        ui.ctx().request_repaint();
    }
    if resp.drag_stopped() {
        let off = HELD.lock().ok().and_then(|g| g.as_ref().map(|x| x.1)).unwrap_or(vec2(20.0, 8.0));
        *HELD.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if let Some(at) = ui.ctx().pointer_latest_pos() {
            let target = drop_side(screen, at);
            if target == Side::Float {
                // Attrapé ancré (décalage énorme) : le panneau flottant arrive sous le pointeur
                let off = if pl.side == Side::Float { off } else { vec2(off.x.min(60.0), off.y.min(11.0)) };
                pl.pos = [at.x - off.x, at.y - off.y];
                let s = float_size(screen, pl.size, 180.0);
                pl.size = [s.x, s.y];
            }
            if target == Side::Window {
                detach_at(ui.ctx(), id, Some(desk(ui.ctx(), at - vec2(off.x.min(60.0), off.y.min(11.0)))), pl.size);
            } else {
                pl.side = target;
                set(id, pl);
            }
        }
    }
    // Menu de place : par le bouton ▾ ou le clic droit sur l'en-tête
    let place_menu = |ui: &mut Ui, pl: &mut Place| {
        for (s, l) in [(Side::Left, "ancrer à gauche"), (Side::Right, "ancrer à droite"), (Side::Bottom, "ancrer en bas"), (Side::Float, "panneau flottant"), (Side::Window, "fenêtre à part")] {
            if ui.radio(pl.side == s, crate::i18n::t(l)).clicked() {
                pl.side = s;
                set(id, *pl);
                ui.close();
            }
        }
        ui.separator();
        if closable && ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("fermer ce panneau"))).clicked() {
            CLOSED.lock().unwrap_or_else(|e| e.into_inner()).push(id.to_string());
            ui.close();
        }
        if ui.button(format!("{}  {}", crate::lucide::ROTATE_CCW, crate::i18n::t("réinitialiser la disposition"))).clicked() {
            reset();
            ui.close();
        }
    };
    let mut pl2 = pl;
    ui.scope_builder(egui::UiBuilder::new().max_rect(mr), |ui| {
        egui::containers::menu::MenuButton::from_button(egui::Button::new(egui::RichText::new("▾").size(12.0).color(p.muted)).frame(false)).ui(ui, |ui| {
            ui.set_min_width(190.0);
            place_menu(ui, &mut pl2);
        });
    });
    resp.context_menu(|ui| place_menu(ui, &mut pl));
    let _ = Color32::TRANSPARENT;
}

/// Affiche un panneau à sa place (ancré ou flottant). `default` sert la première fois.
#[allow(dead_code)]
pub fn show<R>(ui: &mut Ui, id: &str, title: &str, default: Side, size: f32, min: f32, frame: egui::Frame, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    show_ex(ui, id, title, default, size, min, frame, false, add)
}

/// Comme `show`, avec un bouton × : `closed(id)` dit ensuite si on l'a fermé.
#[allow(clippy::too_many_arguments)]
pub fn show_ex<R>(ui: &mut Ui, id: &str, title: &str, default: Side, size: f32, min: f32, frame: egui::Frame, closable: bool, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    if INLINE.with(|c| c.borrow().as_deref() == Some(id)) {
        // Dans sa case : le titre et la fermeture sont dans l'onglet
        let r = frame.stroke(Stroke::NONE).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            add(ui)
        });
        return Some(r.inner);
    }
    if MANAGED.with(|m| m.borrow().contains(id)) {
        return None;
    }
    let pl = place(id, default, size);
    // L'en-tête prend toute la largeur, collé en haut : le cadre intérieur ne s'applique qu'au contenu
    let inner = frame.inner_margin;
    let frame = frame.inner_margin(0);
    let screen = ui.ctx().content_rect();
    // Flottant ou à part : une vraie fenêtre du système, qui peut sortir de l'application
    if matches!(pl.side, Side::Window | Side::Float) {
        let ctx = ui.ctx().clone();
        let shown = native(&ctx, id, title, vec2(pl.size[0], pl.size[1]), vec2(min.max(180.0), 140.0), true, |ui| {
            egui::CentralPanel::default()
                .frame(frame.stroke(Stroke::NONE))
                .show(ui, |ui| {
                    let back = window_header(ui, id, title);
                    (egui::Frame::new().inner_margin(inner).show(ui, add).inner, back)
                })
                .inner
        });
        let (out, back) = shown.out;
        if shown.attach || shown.close || back {
            // Rattaché : un panneau né flottant retrouve sa case ou ses onglets, les autres leur bord d'origine
            if matches!(default, Side::Float | Side::Window) {
                if pl.side == Side::Window && !id.starts_with("doc:") {
                    set_side(id, Side::Float);
                } else if closable {
                    CLOSED.lock().unwrap_or_else(|e| e.into_inner()).push(id.to_string());
                } else {
                    set_side(id, Side::Right);
                }
            } else {
                set_side(id, default);
            }
            ctx.request_repaint();
        }
        return Some(out);
    }
    let body = |ui: &mut Ui| {
        let top = ui.max_rect().min;
        header(ui, id, title, pl, closable, top);
        egui::Frame::new().inner_margin(inner).show(ui, add).inner
    };
    match pl.side {
        Side::Left => Some(egui::Panel::left(id.to_string()).resizable(true).default_size(size).min_size(min).max_size(max_side(screen, min)).frame(frame).show(ui, body).inner),
        Side::Bottom => Some(egui::Panel::bottom(id.to_string()).resizable(true).default_size(220.0).min_size(90.0).max_size((screen.height() * 0.6).max(140.0)).frame(frame).show(ui, body).inner),
        _ => Some(egui::Panel::right(id.to_string()).resizable(true).default_size(size).min_size(min).max_size(max_side(screen, min)).frame(frame).show(ui, body).inner),
    }
}

/// Une vraie fenêtre du système (comme les fenêtres de Maya) : on peut la sortir de l'application, sur un
/// autre écran. `key` la reconnaît d'une image à l'autre ; sa place et sa taille sont gardées pendant la
/// session. Rend false quand on la ferme.
pub fn os_window(ctx: &egui::Context, key: &str, title: &str, size: egui::Vec2, body: impl FnOnce(&mut Ui)) -> bool {
    let p = crate::theme::pal();
    let shown = native(ctx, &format!("win:{key}"), title, size, vec2(320.0, 180.0), false, |ui| {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.panel).inner_margin(8)).show(ui, body);
    });
    !shown.close
}

// ---------------------------------------------------------------------- fenêtres détachées
//
// Tout panneau peut sortir dans une vraie fenêtre du système, qu'on pose sur un autre écran.
// Ce sont des fenêtres « immédiates » d'egui : le contenu du panneau est dessiné une seule fois par
// image, dans sa fenêtre au lieu de sa place d'origine, avec le même accès direct à l'état de
// l'application (pas d'état partagé entre fils, pas de double calcul d'une vue 3D).
// Leur place vit dans la même table que les panneaux ancrés : elle suit donc `dock.json`,
// les espaces de travail et les dispositions enregistrées.

/// Une fenêtre détachée ouverte pendant la session.
struct Live {
    /// Dernière image où elle a été dessinée
    frame: u64,
    /// Place et taille données à sa création (constantes : le système décide ensuite)
    pos0: Pos2,
    size0: egui::Vec2,
    born: f64,
    last: Pos2,
    /// Dernier déplacement par l'utilisateur
    moved_at: f64,
    dragged: bool,
    /// Prochaine vérification des écrans
    check: f64,
}

/// Fenêtre détachée qu'on déplace en ce moment, pour montrer la zone de rattachement dans la fenêtre principale.
struct Moving {
    title: String,
    over: bool,
    frame: u64,
}

thread_local! {
    static LIVE: std::cell::RefCell<HashMap<String, Live>> = std::cell::RefCell::new(HashMap::new());
    static MOVING: std::cell::RefCell<Option<Moving>> = const { std::cell::RefCell::new(None) };
    /// Panneaux d'éditeur dessinés à cette image (le même panneau de deux éditeurs côte à côte n'a qu'une fenêtre)
    static SEEN: std::cell::RefCell<HashMap<String, u64>> = std::cell::RefCell::new(HashMap::new());
}

/// Places changées par un déplacement de fenêtre, à écrire sur le disque un peu plus tard.
static DIRTY: Mutex<Option<std::time::Instant>> = Mutex::new(None);

fn touch() {
    let mut g = DIRTY.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        *g = Some(std::time::Instant::now());
    }
}

/// À appeler une fois par image depuis la fenêtre principale : écrit les places des fenêtres déplacées
/// (pas à chaque pixel) et montre où lâcher une fenêtre pour la rattacher.
pub fn tick(ctx: &egui::Context) {
    let closing = ctx.input(|i| i.viewport().close_requested());
    let due = DIRTY.lock().ok().and_then(|g| *g).is_some_and(|t| closing || t.elapsed().as_secs_f32() > 0.8);
    if due {
        *DIRTY.lock().unwrap_or_else(|e| e.into_inner()) = None;
        save();
    } else if DIRTY.lock().ok().is_some_and(|g| g.is_some()) {
        ctx.request_repaint_after(std::time::Duration::from_millis(400));
    }
    let frame = ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT);
    let moving = MOVING.with(|m| m.borrow().as_ref().filter(|m| m.frame + 2 >= frame).map(|m| (m.title.clone(), m.over)));
    if let Some((title, over)) = moving {
        let p = crate::theme::pal();
        let z = attach_zone(ctx.content_rect());
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("dock-attach")));
        painter.rect_filled(z, 6.0, if over { p.accent.linear_multiply(0.30) } else { p.panel });
        painter.rect_stroke(z, 6.0, Stroke::new(if over { 2.0 } else { 1.0 }, p.accent), egui::StrokeKind::Inside);
        let label = format!("{}  {} · {title}", crate::lucide::MINIMIZE_2, crate::i18n::t(if over { "attendre ici : le panneau se rattache" } else { "amener la fenêtre ici pour la rattacher" }));
        painter.text(z.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(12.0), if over { p.text } else { p.accent });
    }
}

/// Où amener une fenêtre détachée pour la rattacher : une pastille en haut de la fenêtre principale.
fn attach_zone(content: Rect) -> Rect {
    Rect::from_center_size(pos2(content.center().x, content.top() + 92.0), vec2(360.0f32.min(content.width() - 20.0).max(120.0), 44.0))
}

/// Un point de la fenêtre courante, en coordonnées du bureau.
pub fn desk(ctx: &egui::Context, at: Pos2) -> Pos2 {
    let origin = ctx.input(|i| i.viewport().inner_rect).map(|r| r.min).unwrap_or(Pos2::ZERO);
    origin + at.to_vec2()
}

/// Sort le panneau `id` dans sa propre fenêtre. `at` : où la poser sur le bureau (None : là où elle était la dernière fois).
pub fn detach_at(ctx: &egui::Context, id: &str, at: Option<Pos2>, size: [f32; 2]) {
    let main = ctx.input_for(egui::ViewportId::ROOT, |i| i.viewport().outer_rect).map(|r| r.min).unwrap_or(Pos2::ZERO);
    with(|m| {
        let pl = m.entry(id.to_string()).or_insert(Place::new(Side::Window, [120.0, 160.0], size));
        pl.side = Side::Window;
        if let Some(at) = at {
            pl.abs = Some([at.x, at.y]);
            pl.pos = [at.x - main.x, at.y - main.y];
            pl.size = size;
        }
    });
    LIVE.with(|l| l.borrow_mut().remove(id));
    save();
    ctx.request_repaint();
}

/// Le panneau `id` est-il sorti dans sa propre fenêtre ?
pub fn is_out(id: &str) -> bool {
    side_of(id) == Some(Side::Window)
}

/// Ce que rend une fenêtre détachée.
struct Shown<R> {
    out: R,
    /// On l'a fermée par le système
    close: bool,
    /// On l'a ramenée sur la fenêtre principale
    attach: bool,
}

/// Dessine `body` dans une vraie fenêtre du système. Sa place sur le bureau est retenue sous `key`
/// et retrouvée au lancement ; si son écran a disparu, elle revient sur celui de la fenêtre principale.
fn native<R>(ctx: &egui::Context, key: &str, title: &str, size: egui::Vec2, min: egui::Vec2, can_attach: bool, body: impl FnOnce(&mut Ui) -> R) -> Shown<R> {
    let root = egui::ViewportId::ROOT;
    let frame = ctx.cumulative_frame_nr_for(root);
    let now = ctx.input(|i| i.time);
    let (main_outer, main_inner) = ctx.input_for(root, |i| (i.viewport().outer_rect, i.viewport().inner_rect));
    let main_outer = main_outer.unwrap_or(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0)));
    let main_inner = main_inner.unwrap_or(main_outer);
    let ppp = ctx.input_for(root, |i| i.pixels_per_point);
    let screens = crate::screens::list(ppp);
    let (pos0, size0) = LIVE.with(|l| {
        let mut l = l.borrow_mut();
        let n = l.len();
        let fresh = l.get(key).is_none_or(|v| v.frame + 1 < frame);
        if fresh {
            let pl = with(|m| m.get(key).copied());
            let size = pl.map(|p| vec2(p.size[0], p.size[1])).filter(|s| s.x >= 40.0 && s.y >= 40.0).unwrap_or(size);
            let size = vec2(size.x.max(min.x), size.y.max(min.y));
            let mut pos = match pl {
                Some(Place { abs: Some(a), .. }) => pos2(a[0], a[1]),
                Some(p) => main_outer.min + vec2(p.pos[0], p.pos[1]),
                None => main_outer.min + vec2(120.0 + 24.0 * (n % 6) as f32, 160.0 + 24.0 * (n % 6) as f32),
            };
            // Rangée sur un écran débranché depuis : elle revient sur l'écran principal
            if let Some(p) = screens.as_deref().and_then(|s| crate::screens::rescue(pos, size, s, main_outer, n)) {
                pos = p;
            }
            l.insert(key.to_string(), Live { frame, pos0: pos, size0: size, born: now, last: pos, moved_at: f64::NEG_INFINITY, dragged: false, check: now + 1.0 });
        }
        let v = l.get_mut(key).expect("fenêtre inscrite");
        v.frame = frame;
        (v.pos0, v.size0)
    });
    let vp = egui::ViewportBuilder::default().with_title(format!("{title} · Aryadne")).with_inner_size(size0).with_min_inner_size(min).with_position(pos0);
    let mut close = false;
    let mut attach = false;
    let mut body = Some(body);
    // Le contenu ne se dessine qu'une fois par image : pas de deuxième passe de mise en page dans cette fenêtre
    // (egui en demande parfois une ; le corps d'un panneau ne peut pas être rejoué, et une vue 3D ne doit pas l'être)
    let passes = ctx.options(|o| o.max_passes);
    ctx.options_mut(|o| o.max_passes = std::num::NonZeroUsize::MIN);
    let out = ctx.show_viewport_immediate(egui::ViewportId::from_hash_of(("dock", key)), vp, |ui, _class| {
        let (asked, outer, inner) = ui.ctx().input(|i| (i.viewport().close_requested(), i.viewport().outer_rect, i.viewport().inner_rect));
        close = asked;
        if let (Some(o), Some(r)) = (outer, inner) {
            // Déplacée ou redimensionnée par le système : on retient sa place
            let changed = with(|m| {
                let pl = m.entry(key.to_string()).or_insert(Place::new(Side::Window, [0.0, 0.0], [r.width(), r.height()]));
                let abs = [o.min.x, o.min.y];
                let size = [r.width(), r.height()];
                let far = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() > 0.5 || (a[1] - b[1]).abs() > 0.5;
                let changed = pl.abs.is_none_or(|a| far(a, abs)) || far(pl.size, size);
                pl.abs = Some(abs);
                pl.pos = [o.min.x - main_outer.min.x, o.min.y - main_outer.min.y];
                pl.size = size;
                changed
            });
            if changed {
                touch();
            }
            LIVE.with(|l| {
                let mut l = l.borrow_mut();
                let Some(v) = l.get_mut(key) else { return };
                let settled = now - v.born > 0.6;
                if (o.min - v.last).length() > 0.5 {
                    v.last = o.min;
                    if settled {
                        v.moved_at = now;
                        v.dragged = true;
                    }
                }
                let since = now - v.moved_at;
                if can_attach && v.dragged && since < 1.2 {
                    // On la promène : la fenêtre principale montre où la lâcher pour la rattacher
                    let zone = attach_zone(Rect::from_min_size(Pos2::ZERO, main_inner.size())).translate(main_inner.min.to_vec2());
                    let over = zone.contains(o.center_top() + vec2(0.0, 8.0));
                    MOVING.with(|m| *m.borrow_mut() = Some(Moving { title: title.to_string(), over, frame }));
                    if over && since > 0.45 {
                        attach = true;
                        v.dragged = false;
                    }
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
                    ui.ctx().request_repaint_of(root);
                } else if since >= 1.2 {
                    v.dragged = false;
                }
                // Un écran qu'on débranche pendant la session : la fenêtre revient aussi
                if settled && now >= v.check {
                    v.check = now + 1.5;
                    if let Some(p) = screens.as_deref().and_then(|s| crate::screens::rescue(o.min, o.size(), s, main_outer, 0)) {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::OuterPosition(p));
                        v.last = p;
                    }
                }
            });
        }
        // Tant qu'une fenêtre est dehors, on repasse voir si son écran est toujours là
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(2));
        (body.take().expect("corps dessiné une seule fois"))(ui)
    });
    ctx.options_mut(|o| o.max_passes = passes);
    if attach || close {
        LIVE.with(|l| l.borrow_mut().remove(key));
        MOVING.with(|m| *m.borrow_mut() = None);
    }
    Shown { out, close, attach }
}

/// En-tête d'un panneau dans sa propre fenêtre : son nom, et de quoi le rattacher. Rend vrai si on le rattache.
fn window_header(ui: &mut Ui, id: &str, title: &str) -> bool {
    let p = crate::theme::pal();
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    let painter = ui.painter_at(r);
    painter.rect_filled(r, 0.0, p.bg.gamma_multiply(0.6));
    painter.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, p.line));
    painter.text(pos2(r.left() + 10.0, r.center().y), egui::Align2::LEFT_CENTER, title, egui::FontId::new(12.0, crate::theme::bold()), p.muted);
    let br = Rect::from_min_max(pos2(r.right() - 26.0, r.top() + 1.0), pos2(r.right() - 4.0, r.bottom() - 1.0));
    let b = ui.interact(br, ui.scope_id().with(("attach", id)), Sense::click());
    if b.hovered() {
        ui.painter().rect_filled(br, 3.0, p.line);
    }
    ui.painter().text(br.center(), egui::Align2::CENTER_CENTER, crate::lucide::MINIMIZE_2, egui::FontId::proportional(13.0), if b.hovered() { p.text } else { p.accent });
    b.on_hover_text(crate::i18n::t("rattacher à la fenêtre principale")).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Ce que demande la poignée d'un panneau.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Grip {
    None,
    /// Le sortir dans sa fenêtre, posée à cet endroit du bureau (None : sur place)
    Detach(Option<Pos2>),
    Attach,
}

/// La poignée discrète d'un panneau : trois points au milieu de son bord haut (ou de son bord gauche pour une
/// barre basse). Un clic le détache, le tirer le détache là où on le lâche. Dans une fenêtre détachée, un clic le rattache.
/// Dessinée par-dessus le contenu, dans la marge : la mise en page du panneau ne change pas.
pub fn grip(ui: &Ui, rect: Rect, id: &str, title: &str, out: bool) -> Grip {
    let p = crate::theme::pal();
    // Barre basse et large (transport, statut) : la poignée se met debout, à gauche
    let upright = rect.height() < 64.0 && rect.width() > rect.height() * 2.0;
    let pill = if upright { Rect::from_center_size(pos2(rect.left() + 4.0, rect.center().y), vec2(8.0, 26.0f32.min(rect.height() - 4.0))) } else { Rect::from_center_size(pos2(rect.center().x, rect.top() + 4.0), vec2(34.0f32.min(rect.width() - 4.0), 8.0)) };
    let resp = ui.interact(pill, egui::Id::unique(("dock-grip", id)), Sense::click_and_drag());
    let hot = resp.hovered() || resp.dragged();
    let painter = ui.ctx().layer_painter(ui.layer_id()).with_clip_rect(rect.expand(1.0));
    if hot {
        painter.rect_filled(pill, 4.0, p.accent);
    } else if out {
        painter.rect_filled(pill, 4.0, p.accent.gamma_multiply(0.35));
    }
    let dot = if hot { p.on_accent } else if out { p.accent } else { p.muted.gamma_multiply(0.75) };
    for k in [-1.0f32, 0.0, 1.0] {
        let c = if upright { pill.center() + vec2(0.0, k * 5.0) } else { pill.center() + vec2(k * 5.0, 0.0) };
        painter.circle_filled(c, 1.1, dot);
    }
    let mut ask = Grip::None;
    if out {
        if resp.clicked() {
            ask = Grip::Attach;
        }
        resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("{}  {} · {title}", crate::lucide::MINIMIZE_2, crate::i18n::t("rattacher à la fenêtre principale")));
        return ask;
    }
    if resp.clicked() {
        ask = Grip::Detach(None);
    }
    if resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        if let Some(at) = ui.ctx().pointer_latest_pos() {
            // Le fantôme de la fenêtre suit le pointeur tant qu'il reste dans l'application
            let ghost = Rect::from_min_size(at - vec2(rect.width().min(360.0) / 2.0, 6.0), vec2(rect.width().min(360.0), rect.height().min(220.0)));
            let top = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("dock-ghost")));
            top.rect_filled(ghost, 4.0, p.accent.linear_multiply(0.12));
            top.rect_stroke(ghost, 4.0, Stroke::new(1.5, p.accent), egui::StrokeKind::Inside);
            top.text(ghost.center(), egui::Align2::CENTER_CENTER, format!("{}  {title}", crate::lucide::EXTERNAL_LINK), egui::FontId::proportional(12.0), p.accent);
        }
        ui.ctx().request_repaint();
    }
    if resp.drag_stopped() {
        if let Some(at) = ui.ctx().pointer_latest_pos() {
            if outside(ui.ctx(), at) || (at - pill.center()).length() > 24.0 {
                ask = Grip::Detach(Some(desk(ui.ctx(), at - vec2(rect.width().min(360.0) / 2.0, 6.0))));
            }
        }
    }
    if !resp.dragged() {
        resp.on_hover_cursor(egui::CursorIcon::Grab).on_hover_text(format!("{}  {} · {title}\n{}", crate::lucide::EXTERNAL_LINK, crate::i18n::t("détacher dans sa propre fenêtre"), crate::i18n::t("un clic, ou tirer la poignée hors de la fenêtre (vers un autre écran)")));
    }
    ask
}

/// Applique ce qu'a demandé la poignée du panneau `key`, qui occupe `rect` dans la fenêtre courante.
pub fn apply_grip(ctx: &egui::Context, key: &str, rect: Rect, ask: Grip) {
    match ask {
        Grip::None => {}
        Grip::Detach(at) => {
            let size = [rect.width().max(200.0), rect.height().max(160.0)];
            detach_at(ctx, key, Some(at.unwrap_or_else(|| desk(ctx, rect.min))), size);
        }
        Grip::Attach => attach(ctx, key),
    }
}

/// Ramène le panneau d'éditeur `key` à sa place.
pub fn attach(ctx: &egui::Context, key: &str) {
    with(|m| {
        if let Some(pl) = m.get_mut(key) {
            pl.side = Side::Home;
        }
    });
    LIVE.with(|l| l.borrow_mut().remove(key));
    save();
    ctx.request_repaint();
}

/// Premier dessin de ce panneau à cette image ? (deux éditeurs du même genre côte à côte se partagent l'identifiant)
fn first_this_frame(ctx: &egui::Context, key: &str) -> bool {
    // Par passe, pas par image : si la fenêtre principale refait sa mise en page, le panneau se redessine avec elle
    let pass = ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT);
    SEEN.with(|s| s.borrow_mut().insert(key.to_string(), pass) != Some(pass))
}

/// Dessine un panneau d'éditeur sorti de son éditeur : dans sa case de la disposition, ou dans sa propre
/// fenêtre. Dans les deux cas, sa poignée est là (pour le détacher, ou pour le rattacher).
pub fn slot<R>(ui: &mut Ui, slot: crate::layout::Slot, fill: Color32, margin: f32, body: impl FnOnce(&mut Ui) -> R) -> R {
    let ctx = ui.ctx().clone();
    let key = slot.key;
    let title = crate::i18n::t(&slot.title);
    match slot.rect {
        Some(r) => {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink(margin)).layout(egui::Layout::top_down(egui::Align::Min)));
            child.set_clip_rect(r);
            child.painter().rect_filled(r, 0.0, fill);
            let out = body(&mut child);
            let ask = grip(ui, r, &key, &title, false);
            apply_grip(&ctx, &key, r, ask);
            out
        }
        None if first_this_frame(&ctx, &key) => {
            let shown = native(&ctx, &key, &title, vec2(320.0, 420.0), vec2(120.0, 80.0), true, |ui| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(fill).inner_margin(margin))
                    .show(ui, |ui| {
                        let rect = ui.max_rect().expand(margin);
                        let out = body(ui);
                        (out, grip(ui, rect, &key, &title, true))
                    })
                    .inner
            });
            let (out, ask) = shown.out;
            if shown.close || shown.attach || ask == Grip::Attach {
                attach(&ctx, &key);
            }
            out
        }
        None => {
            // Deux éditeurs du même genre côte à côte : la fenêtre montre le premier, l'autre attend son tour
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(ui.max_rect().min, vec2(1.0, 1.0))));
            child.set_invisible();
            body(&mut child)
        }
    }
}

/// Un panneau d'éditeur détachable. À sa place, c'est `panel` (un panneau egui ordinaire) avec une poignée
/// discrète ; rangé dans la disposition en cases, il remplit sa case ; détaché, il a sa propre fenêtre.
/// `title` est la clé française du titre (traduite ici).
pub fn panel<R>(ui: &mut Ui, id: &str, title: &str, panel: egui::Panel, frame: egui::Frame, body: impl FnOnce(&mut Ui) -> R) -> R {
    if let Some(s) = crate::layout::offer(id, title) {
        return slot(ui, s, frame.fill, frame.inner_margin.leftf(), body);
    }
    let shown = panel.frame(frame).show(ui, body);
    let rect = shown.response.rect;
    let key = crate::layout::editor_pane(id);
    let ask = grip(ui, rect, &key, &crate::i18n::t(title), false);
    apply_grip(ui.ctx(), &key, rect, ask);
    shown.inner
}

/// La zone centrale d'un éditeur (vue, moniteur, pistes…), détachable elle aussi. Sortie dans sa fenêtre,
/// elle laisse dans l'éditeur un rappel avec de quoi la ramener.
pub fn central<R>(ui: &mut Ui, id: &str, title: &str, frame: egui::Frame, body: impl FnOnce(&mut Ui) -> R) -> R {
    let key = crate::layout::editor_pane(id);
    if let Some(s) = crate::layout::offer(id, title) {
        let out = s.rect.is_none();
        let r = slot(ui, s, frame.fill, frame.inner_margin.leftf(), body);
        let p = crate::theme::pal();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(frame.fill)).show(ui, |ui| {
            ui.centered_and_justified(|ui| {
                let what = if out { "dans sa propre fenêtre" } else { "dans sa case de la disposition" };
                let label = format!("{}  {} : {}", crate::lucide::EXTERNAL_LINK, crate::i18n::t(title), crate::i18n::t_mid(what));
                let b = ui.add(egui::Label::new(egui::RichText::new(label).size(12.0).color(p.muted)).sense(if out { Sense::click() } else { Sense::hover() }));
                if out && b.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(crate::i18n::t("rattacher à la fenêtre principale")).clicked() {
                    attach(ui.ctx(), &key);
                }
            });
        });
        return r;
    }
    let shown = egui::CentralPanel::default().frame(frame).show(ui, body);
    let rect = shown.response.rect;
    let ask = grip(ui, rect, &key, &crate::i18n::t(title), false);
    apply_grip(ui.ctx(), &key, rect, ask);
    shown.inner
}

/// Une fenêtre flottante détachable : dans l'application, c'est `win` (une fenêtre egui) avec la poignée ;
/// détachée, une vraie fenêtre du système. Fermer la fenêtre du système la ramène dans l'application.
pub fn window<R>(ctx: &egui::Context, id: &str, title: &str, win: egui::Window<'_>, body: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let key = format!("float:{id}");
    let p = crate::theme::pal();
    if is_out(&key) && first_this_frame(ctx, &key) {
        let shown = native(ctx, &key, title, vec2(360.0, 320.0), vec2(200.0, 120.0), true, |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(p.panel).inner_margin(10))
                .show(ui, |ui| {
                    let rect = ui.max_rect().expand(10.0);
                    let out = egui::ScrollArea::both().auto_shrink([false, false]).show(ui, body).inner;
                    (out, grip(ui, rect, &key, title, true))
                })
                .inner
        });
        let (out, ask) = shown.out;
        if shown.close || shown.attach || ask == Grip::Attach {
            attach(ctx, &key);
        }
        return Some(out);
    }
    let mut ask = Grip::None;
    let mut rect = Rect::NOTHING;
    let shown = win.show(ctx, |ui| {
        let out = body(ui);
        rect = ui.min_rect().expand(6.0);
        ask = grip(ui, rect, &key, title, false);
        out
    });
    if let Grip::Detach(at) = ask {
        // La fenêtre du système prend la place et la taille de la fenêtre flottante
        let full = shown.as_ref().map(|s| s.response.rect).unwrap_or(rect);
        detach_at(ctx, &key, Some(at.unwrap_or_else(|| desk(ctx, full.min))), [rect.width().max(240.0), rect.height().max(140.0)]);
    }
    shown.and_then(|s| s.inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_relue_des_anciennes_versions() {
        // Avant les fenêtres détachées : pas de coin sur le bureau
        let old: Place = serde_json::from_str(r#"{"side":"Window","pos":[10.0,20.0],"size":[300.0,400.0]}"#).unwrap();
        assert!(old.side == Side::Window && old.abs.is_none());
        // Une fenêtre sur un deuxième écran, à gauche du principal : coordonnées négatives gardées
        let mut pl = Place::new(Side::Window, [-1500.0, 40.0], [640.0, 480.0]);
        pl.abs = Some([-1400.0, 120.0]);
        let back: Place = serde_json::from_str(&serde_json::to_string(&pl).unwrap()).unwrap();
        assert_eq!(back.abs, Some([-1400.0, 120.0]));
        assert_eq!(back.size, [640.0, 480.0]);
        // Un panneau d'éditeur rattaché garde la place de sa fenêtre pour la prochaine fois
        let home: Place = serde_json::from_str(r#"{"side":"Home","pos":[0.0,0.0],"size":[300.0,400.0],"abs":[2000.0,90.0]}"#).unwrap();
        assert!(home.side == Side::Home && home.abs == Some([2000.0, 90.0]));
        // Une place sans coin ne l'écrit pas (les fichiers restent lisibles par l'ancienne version)
        assert!(!serde_json::to_string(&Place::new(Side::Left, [0.0, 0.0], [1.0, 1.0])).unwrap().contains("abs"));
    }

    #[test]
    fn zone_de_rattachement_dans_la_fenetre() {
        let content = Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 850.0));
        let z = attach_zone(content);
        assert!(content.contains_rect(z));
        // Fenêtre principale étroite : la zone rétrécit sans en sortir
        let small = Rect::from_min_size(Pos2::ZERO, vec2(300.0, 400.0));
        assert!(small.contains_rect(attach_zone(small)));
    }
}
