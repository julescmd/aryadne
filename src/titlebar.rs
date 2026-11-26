//! Barre de titre dessinée par Aryadne : une seule rangée en haut, avec les menus, le nom du fichier
//! (qu'on tire pour déplacer la fenêtre, double-clic pour l'agrandir) et les boutons de la fenêtre.
//! Sans la barre du système, les bords de la fenêtre restent redimensionnables (voir `resize_edges`).
//! `ARY_BARRE_SYSTEME=1` rend la barre du système, si un gestionnaire de fenêtres la gère mal.

use eframe::egui;

/// Hauteur de la barre de titre
pub const HEIGHT: f32 = 32.0;

/// Aryadne dessine sa propre barre (sinon : celle du système)
pub fn own() -> bool {
    std::env::var_os("ARY_BARRE_SYSTEME").is_none()
}

fn maximized(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().maximized.unwrap_or(false))
}

fn fullscreen(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().fullscreen.unwrap_or(false))
}

/// Zone libre de la barre : le titre au centre ; la tirer déplace la fenêtre, un double-clic l'agrandit
pub fn drag_area(ui: &mut egui::Ui, title: &str) {
    let p = crate::theme::pal();
    // Les boutons de la fenêtre prennent la fin de la rangée
    let w = (ui.available_width() - if own() { 3.0 * 40.0 } else { 0.0 }).max(0.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, ui.available_height()), egui::Sense::click_and_drag());
    let galley = ui.painter().layout_no_wrap(title.to_string(), egui::FontId::proportional(12.5), p.muted);
    // Centré sur la fenêtre plutôt que sur la zone libre, comme les barres du système
    let cx = ui.ctx().content_rect().center().x.clamp(rect.left() + galley.size().x / 2.0 + 8.0, (rect.right() - galley.size().x / 2.0 - 8.0).max(rect.left()));
    if galley.size().x + 16.0 < rect.width() {
        ui.painter().galley(egui::pos2(cx - galley.size().x / 2.0, rect.center().y - galley.size().y / 2.0), galley, p.muted);
    }
    if !own() {
        return;
    }
    if resp.double_clicked() {
        let m = maximized(ui.ctx());
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!m));
    } else if resp.drag_started_by(egui::PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

/// Réduire, agrandir ou restaurer, fermer (à droite de la rangée)
pub fn controls(ui: &mut egui::Ui) {
    if !own() {
        return;
    }
    let p = crate::theme::pal();
    let ctx = ui.ctx().clone();
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let size = egui::vec2(40.0, ui.available_height());
        let mut button = |ui: &mut egui::Ui, icon: &str, tip: &str, danger: bool| -> bool {
            let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
            let hot = resp.hovered();
            if hot {
                ui.painter().rect_filled(rect, 0.0, if danger { egui::Color32::from_rgb(196, 43, 28) } else { p.field });
            }
            let col = if hot && danger { egui::Color32::WHITE } else if hot { p.text } else { p.muted };
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(14.0), col);
            resp.on_hover_text(crate::i18n::t(tip)).clicked()
        };
        if button(ui, crate::lucide::X, "fermer", true) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let m = maximized(&ctx);
        if button(ui, if m { crate::lucide::COPY } else { crate::lucide::SQUARE }, if m { "restaurer" } else { "agrandir" }, false) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!m));
        }
        if button(ui, crate::lucide::MINUS, "réduire", false) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    });
}

/// Une barre simple (logo, titre, boutons) pour les écrans qui n'ont pas de menus, comme l'accueil
pub fn strip(ui: &mut egui::Ui, title: &str) {
    if !own() {
        return;
    }
    let p = crate::theme::pal();
    egui::Panel::top("titlebar-strip").exact_size(HEIGHT).frame(egui::Frame::new().fill(p.bg).inner_margin(egui::Margin { left: 10, right: 0, top: 0, bottom: 0 })).show(ui, |ui| {
        ui.horizontal_centered(|ui| {
            crate::theme::mark(ui, 14.0);
            drag_area(ui, title);
            controls(ui);
        });
    });
}

/// Sans barre du système, les bords de la fenêtre servent à la redimensionner
pub fn resize_edges(ctx: &egui::Context) {
    if !own() || maximized(ctx) || fullscreen(ctx) {
        return;
    }
    use egui::ResizeDirection as D;
    let r = ctx.content_rect();
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else { return };
    let (edge, corner) = (5.0, 14.0);
    let (l, rgt, t, b) = (pos.x - r.left() < edge, r.right() - pos.x < edge, pos.y - r.top() < edge, r.bottom() - pos.y < edge);
    let (lc, rc, tc, bc) = (pos.x - r.left() < corner, r.right() - pos.x < corner, pos.y - r.top() < corner, r.bottom() - pos.y < corner);
    let dir = if (t && lc) || (l && tc) {
        Some((D::NorthWest, egui::CursorIcon::ResizeNorthWest))
    } else if (t && rc) || (rgt && tc) {
        Some((D::NorthEast, egui::CursorIcon::ResizeNorthEast))
    } else if (b && lc) || (l && bc) {
        Some((D::SouthWest, egui::CursorIcon::ResizeSouthWest))
    } else if (b && rc) || (rgt && bc) {
        Some((D::SouthEast, egui::CursorIcon::ResizeSouthEast))
    } else if t {
        Some((D::North, egui::CursorIcon::ResizeNorth))
    } else if b {
        Some((D::South, egui::CursorIcon::ResizeSouth))
    } else if l {
        Some((D::West, egui::CursorIcon::ResizeWest))
    } else if rgt {
        Some((D::East, egui::CursorIcon::ResizeEast))
    } else {
        None
    };
    // Pas pendant un glisser déjà commencé ailleurs (curseur, sélection)
    if ctx.dragged_id().is_some() {
        return;
    }
    if let Some((d, icon)) = dir {
        ctx.set_cursor_icon(icon);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(d));
        }
    }
}
