//! Zoom et déplacement pour les images et les vidéos : molette (vers le curseur),
//! glisser pour déplacer, double-clic pour basculer entre « ajuster » et 100 %.
use eframe::egui::{self, pos2, vec2, Rect, Response, RichText, Vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zoom {
    /// Pixels écran par pixel image ; None = ajuster à la zone
    pub scale: Option<f32>,
    /// Point de l'image (0-1) au centre de la zone
    pub center: [f32; 2],
}

impl Default for Zoom {
    fn default() -> Self {
        Zoom { scale: None, center: [0.5, 0.5] }
    }
}

impl Zoom {
    pub fn fit_scale(area: Rect, size: Vec2) -> f32 {
        (area.width() / size.x.max(1.0)).min(area.height() / size.y.max(1.0))
    }

    /// Rectangle écran de l'image.
    pub fn image_rect(&self, area: Rect, size: Vec2) -> Rect {
        match self.scale {
            None => Rect::from_center_size(area.center(), size * Self::fit_scale(area, size)),
            Some(s) => {
                let d = size * s;
                let min = area.center() - vec2(self.center[0] * d.x, self.center[1] * d.y);
                Rect::from_min_size(min, d)
            }
        }
    }

    /// Gère molette, glisser et double-clic sur `resp` (zone `area`). `pan_primary` :
    /// le bouton gauche déplace l'image (sinon seulement le bouton du milieu, ou Espace + gauche).
    /// Retourne vrai si la vue a changé.
    pub fn interact(&mut self, ui: &egui::Ui, resp: &Response, area: Rect, size: Vec2, pan_primary: bool) -> bool {
        let before = *self;
        let fit = Self::fit_scale(area, size);
        let cur = self.scale.unwrap_or(fit);
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.zoom_delta().ln() * 200.0);
            if scroll.abs() > 0.1 {
                if let Some(pt) = resp.hover_pos() {
                    let img = self.image_rect(area, size);
                    // Point de l'image sous la souris : il reste sous la souris
                    let under = [(pt.x - img.left()) / img.width().max(1e-6), (pt.y - img.top()) / img.height().max(1e-6)];
                    let ns = (cur * (scroll * 0.0025).exp()).clamp(fit.min(0.02) * 0.5, 64.0);
                    let d = size * ns;
                    let c = [under[0] - (pt.x - area.center().x) / d.x, under[1] - (pt.y - area.center().y) / d.y];
                    self.scale = Some(ns);
                    self.center = c;
                }
            }
        }
        let space = ui.input(|i| i.key_down(egui::Key::Space));
        let panning = resp.dragged_by(egui::PointerButton::Middle) || ((pan_primary || space) && resp.dragged_by(egui::PointerButton::Primary));
        if panning {
            let d = resp.drag_delta();
            let s = self.scale.unwrap_or(fit);
            self.scale = Some(s);
            self.center = [self.center[0] - d.x / (size.x * s), self.center[1] - d.y / (size.y * s)];
        }
        if resp.double_clicked() {
            *self = if self.scale.is_none() { Zoom { scale: Some(1.0), center: self.hover_center(resp, area, size) } } else { Zoom::default() };
        }
        if resp.hovered() && !ui.ctx().egui_wants_keyboard_input() {
            if ui.input(|i| i.key_pressed(egui::Key::F) && !i.modifiers.any()) {
                *self = Zoom::default();
            }
        }
        *self != before
    }

    fn hover_center(&self, resp: &Response, area: Rect, size: Vec2) -> [f32; 2] {
        let img = self.image_rect(area, size);
        resp.hover_pos().map(|p| [((p.x - img.left()) / img.width()).clamp(0.0, 1.0), ((p.y - img.top()) / img.height()).clamp(0.0, 1.0)]).unwrap_or([0.5, 0.5])
    }

    /// Petite barre en bas à droite : pourcentage, ajuster, 100 %.
    pub fn badge(&mut self, ui: &mut egui::Ui, area: Rect, size: Vec2) {
        let p = crate::theme::pal();
        let s = self.scale.unwrap_or(Self::fit_scale(area, size));
        let r = Rect::from_min_size(pos2(area.right() - 190.0, area.bottom() - 28.0), vec2(184.0, 24.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::right_to_left(egui::Align::Center)));
        egui::Frame::new().fill(p.bg.gamma_multiply(0.85)).inner_margin(egui::Margin::symmetric(4, 1)).show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if ui.small_button("100 %").clicked() {
                self.scale = Some(1.0);
            }
            if ui.small_button("ajuster").clicked() {
                *self = Zoom::default();
            }
            ui.label(RichText::new(format!("{:.0} %", s * 100.0)).size(11.0).color(p.muted));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Pos2;

    #[test]
    fn zoom_garde_le_point() {
        let area = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        let size = vec2(1600.0, 1200.0);
        let z = Zoom::default();
        let r = z.image_rect(area, size);
        assert!((r.width() - 800.0).abs() < 1e-3);
        let z2 = Zoom { scale: Some(1.0), center: [0.25, 0.25] };
        let r2 = z2.image_rect(area, size);
        // Le quart de l'image est au centre de la zone
        assert!((r2.left() + 0.25 * 1600.0 - 400.0).abs() < 1e-3);
    }
}
