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

/// Poignée « ⠿ » : glisser pour déplacer le panneau, clic droit pour choisir sa place.
fn grip(ui: &mut Ui, id: &str, title: &str, mut pl: Place) {
    let p = crate::theme::pal();
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 14.0), Sense::click_and_drag());
    let hot = resp.hovered() || resp.dragged();
    ui.painter().text(pos2(r.left() + 4.0, r.center().y), egui::Align2::LEFT_CENTER, "⠿", egui::FontId::proportional(12.0), if hot { p.accent } else { p.muted });
    if hot {
        ui.painter().text(pos2(r.left() + 18.0, r.center().y), egui::Align2::LEFT_CENTER, crate::i18n::t("glisser pour déplacer"), egui::FontId::proportional(10.0), p.muted);
        ui.ctx().set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
    }
    let screen = ui.ctx().content_rect();
    if pl.side == Side::Window {
        // Dans sa propre fenêtre : un clic la ramène dans l'application
        if resp.on_hover_text(crate::i18n::t("cliquer pour rattacher à la fenêtre principale")).clicked() {
            pl.side = Side::Float;
            set(id, pl);
        }
        return;
    }
    if resp.dragged() {
        if let Some(at) = resp.interact_pointer_pos() {
            if pl.side == Side::Float {
                let d = resp.drag_delta();
                pl.pos = [pl.pos[0] + d.x, pl.pos[1] + d.y];
                with(|m| m.insert(id.to_string(), pl));
            }
            // Aperçu de la zone d'arrivée
            let target = drop_side(screen, at);
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("dock-preview")));
            let z = if target == Side::Float && pl.side != Side::Float { Rect::from_min_size(at - vec2(20.0, 8.0), vec2(pl.size[0], pl.size[1])) } else { zone(screen, target) };
            if z.is_positive() {
                painter.rect_filled(z, 6.0, p.accent.linear_multiply(0.15));
                painter.rect_stroke(z, 6.0, Stroke::new(2.0, p.accent), egui::StrokeKind::Inside);
                painter.text(z.center(), egui::Align2::CENTER_CENTER, title, egui::FontId::proportional(14.0), p.accent);
            }
        }
    }
    if resp.drag_stopped() {
        if let Some(at) = ui.ctx().pointer_latest_pos() {
            let target = drop_side(screen, at);
            if target == Side::Float && pl.side != Side::Float {
                pl.pos = [at.x - 20.0, at.y - 8.0];
            }
            pl.side = target;
            set(id, pl);
        }
    }
    resp.context_menu(|ui| {
        for (s, l) in [(Side::Left, "ancrer à gauche"), (Side::Right, "ancrer à droite"), (Side::Bottom, "ancrer en bas"), (Side::Float, "fenêtre flottante"), (Side::Window, "détacher hors de l'application")] {
            if ui.radio(pl.side == s, crate::i18n::t(l)).clicked() {
                pl.side = s;
                set(id, pl);
                ui.close();
            }
        }
        ui.separator();
        if ui.button(crate::i18n::t("réinitialiser la disposition")).clicked() {
            reset();
            ui.close();
        }
    });
    let _ = Color32::TRANSPARENT;
}

/// Affiche un panneau à sa place (ancré ou flottant). `default` sert la première fois.
pub fn show<R>(ui: &mut Ui, id: &str, title: &str, default: Side, size: f32, min: f32, frame: egui::Frame, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let pl = place(id, default, size);
    let body = |ui: &mut Ui| {
        grip(ui, id, title, pl);
        add(ui)
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
            let screen = ctx.content_rect();
            let pos = pos2(pl.pos[0].clamp(screen.left(), screen.right() - 80.0), pl.pos[1].clamp(screen.top(), screen.bottom() - 40.0));
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
