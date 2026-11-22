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
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub struct Place {
    pub side: Side,
    pub pos: [f32; 2],
    pub size: [f32; 2],
}

static STATE: Mutex<Option<HashMap<String, Place>>> = Mutex::new(None);

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
        let _ = std::fs::write(p, d);
    }
}

/// La disposition en cours (pour l'enregistrer dans un espace de travail).
pub fn export() -> HashMap<String, Place> {
    with(|m| m.clone())
}

/// Remplace la disposition (en changeant d'espace de travail).
pub fn import(places: HashMap<String, Place>) {
    with(|m| *m = places);
    save();
}

/// Place un panneau à un endroit précis (un onglet qu'on vient de détacher, par exemple).
pub fn put(id: &str, side: Side, pos: [f32; 2], size: [f32; 2]) {
    set(id, Place { side, pos, size });
}

/// Le point `at` est-il hors de la fenêtre de l'application ?
pub fn outside(ctx: &egui::Context, at: Pos2) -> bool {
    !ctx.content_rect().expand(4.0).contains(at)
}

/// Remet tous les panneaux à leur place d'origine.
pub fn reset() {
    with(|m| m.clear());
    save();
}

fn place(id: &str, default: Side, size: f32) -> Place {
    with(|m| *m.entry(id.to_string()).or_insert(Place { side: default, pos: [200.0, 120.0], size: [size.max(260.0), 420.0] }))
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
        Side::Float | Side::Window => Rect::NOTHING,
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
    painter.text(pos2(r.left() + 22.0, r.center().y), egui::Align2::LEFT_CENTER, title.to_uppercase(), egui::FontId::new(10.5, crate::theme::bold()), if hot { p.text } else { p.muted });
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
    // ▾ : la place du panneau, comme le clic droit
    let mr = Rect::from_min_max(pos2(bx - 20.0, r.top() + 1.0), pos2(bx, r.bottom() - 1.0));
    let screen = ui.ctx().content_rect();
    if pl.side == Side::Window {
        // Dans sa propre fenêtre : un clic sur l'en-tête la ramène dans l'application
        if resp.on_hover_text(crate::i18n::t("cliquer pour rattacher à la fenêtre principale")).clicked() {
            pl.side = Side::Float;
            set(id, pl);
        }
        return;
    }
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
                Side::Float if pl.side != Side::Float => Rect::from_min_size(at - off, vec2(pl.size[0], pl.size[1])),
                Side::Float | Side::Window => Rect::NOTHING,
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
                Side::Window => "lâcher : fenêtre à part",
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
                pl.pos = [at.x - off.x, at.y - off.y];
            }
            pl.side = target;
            set(id, pl);
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
        if closable && ui.button(crate::i18n::t("fermer ce panneau")).clicked() {
            CLOSED.lock().unwrap_or_else(|e| e.into_inner()).push(id.to_string());
            ui.close();
        }
        if ui.button(crate::i18n::t("réinitialiser la disposition")).clicked() {
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
    let pl = place(id, default, size);
    // L'en-tête prend toute la largeur, collé en haut : le cadre intérieur ne s'applique qu'au contenu
    let inner = frame.inner_margin;
    let frame = frame.inner_margin(0);
    // Flottant : le contenu reste dans le cadre du panneau (un board ou un graphe y prend toute la place sans déborder)
    let screen = ui.ctx().content_rect();
    let float_pos = pos2(pl.pos[0].clamp(screen.left(), screen.right() - 80.0), pl.pos[1].clamp(screen.top(), screen.bottom() - 40.0));
    let float_rect = (pl.side == Side::Float).then(|| Rect::from_min_size(float_pos, vec2(pl.size[0], pl.size[1])));
    let body = |ui: &mut Ui| {
        let top = ui.max_rect().min;
        if let Some(r) = float_rect {
            ui.set_clip_rect(ui.clip_rect().intersect(r));
        }
        header(ui, id, title, pl, closable, top);
        egui::Frame::new().inner_margin(inner).show(ui, |ui| {
            if let Some(r) = float_rect {
                ui.set_max_height((r.bottom() - ui.cursor().top() - 4.0).max(40.0));
            }
            add(ui)
        }).inner
    };
    match pl.side {
        Side::Left => Some(egui::Panel::left(id.to_string()).resizable(true).default_size(size).min_size(min).frame(frame).show(ui, body).inner),
        Side::Right => Some(egui::Panel::right(id.to_string()).resizable(true).default_size(size).min_size(min).frame(frame).show(ui, body).inner),
        Side::Bottom => Some(egui::Panel::bottom(id.to_string()).resizable(true).default_size(220.0).min_size(90.0).frame(frame).show(ui, body).inner),
        Side::Window => {
            let ctx = ui.ctx().clone();
            let mut body = Some(body);
            let mut back = false;
            let vp = egui::ViewportBuilder::default().with_title(format!("{title} · Aryadne")).with_inner_size(vec2(pl.size[0].max(min), pl.size[1].max(240.0)));
            let out = ctx.show_viewport_immediate(egui::ViewportId::from_hash_of(("dock", id)), vp, |ui, _class| {
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    back = true;
                }
                let body = body.take()?;
                Some(egui::CentralPanel::default().frame(frame.stroke(Stroke::NONE)).show(ui, body).inner)
            });
            if back {
                // Fermer la fenêtre détachée ramène le panneau à sa place d'origine
                set(id, Place { side: default, ..pl });
                ctx.request_repaint();
            }
            out
        }
        Side::Float => {
            let ctx = ui.ctx().clone();
            let pos = float_pos;
            let r = egui::Window::new(title)
                .id(egui::Id::unique(("dock", id)))
                .title_bar(false)
                .movable(false)
                .resizable(true)
                .current_pos(pos)
                .default_size(vec2(pl.size[0], pl.size[1]))
                .min_size(vec2(min, 120.0))
                .frame(frame.shadow(ctx.global_style().visuals.window_shadow).corner_radius(8))
                .show(&ctx, body)?;
            // Garder la taille choisie pour la prochaine fois
            let sz = r.response.rect.size();
            if (sz.x - pl.size[0]).abs() > 2.0 || (sz.y - pl.size[1]).abs() > 2.0 {
                with(|m| {
                    if let Some(p) = m.get_mut(id) {
                        p.size = [sz.x, sz.y];
                    }
                });
                if !ctx.input(|i| i.pointer.any_down()) {
                    save();
                }
            }
            r.inner
        }
    }
}
