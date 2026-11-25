//! Navigation commune de toutes les timelines (montage, audio, musique, motion, 3D) et de l'éditeur de
//! courbes : mêmes gestes de souris, mêmes raccourcis partout ; chaque éditeur garde ses réglages propres.
//!
//! Souris : molette = défiler dans le temps ; Ctrl+molette (ou pincer) = zoomer dans le temps autour du
//! pointeur ; Maj+molette = défiler en hauteur (pistes, notes, valeurs) ; Alt+molette = zoomer en hauteur ;
//! clic molette glissé = se déplacer dans les deux sens.
//! Clavier : Espace lecture ; Origine / Fin ; ← → une image ; Maj+← → clé ou coupe précédente / suivante ;
//! F tout voir ; = et − zoomer.
use eframe::egui::{self, Pos2, Rect, Response, Ui};

/// Le rappel affiché dans les bulles d'aide des timelines
pub const HELP: &str = "molette : temps · Ctrl+molette : zoom · Maj+molette : hauteur · Alt+molette : zoom en hauteur · clic molette : se déplacer · F : tout voir · Origine / Fin · ← → : une image · Maj+← → : clé suivante · = − : zoom";

/// Ce que la souris demande à une vue de temps, cette image
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wheel {
    /// Défilement dans le temps, en pixels (positif : vers la droite du temps, c'est-à-dire plus tard)
    pub time_px: f32,
    /// Zoom dans le temps (facteur, 1 : rien)
    pub zoom: f32,
    /// Défilement vertical, en pixels (positif : vers le bas)
    pub y_px: f32,
    /// Zoom vertical (facteur, 1 : rien)
    pub yzoom: f32,
    /// Où est le pointeur (pour zoomer autour)
    pub at: Option<Pos2>,
}

impl Wheel {
    pub fn is_none(&self) -> bool {
        self.time_px == 0.0 && self.zoom == 1.0 && self.y_px == 0.0 && self.yzoom == 1.0
    }
}

/// Molette, pincement et glisser du bouton du milieu, quand le pointeur est sur `rect`
pub fn wheel(ui: &Ui, rect: Rect, resp: Option<&Response>) -> Option<Wheel> {
    let middle = resp.filter(|r| r.dragged_by(egui::PointerButton::Middle)).map(|r| r.drag_delta());
    if !ui.rect_contains_pointer(rect) && middle.is_none() {
        return None;
    }
    let (d, pinch, m, at) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers, i.pointer.hover_pos()));
    let mut w = Wheel { time_px: 0.0, zoom: 1.0, y_px: 0.0, yzoom: 1.0, at };
    // (egui change Ctrl+molette en zoom, et Maj+molette en défilement horizontal selon le système)
    let amount = d.x + d.y;
    if m.command {
        w.zoom = pinch * (amount / 200.0).exp();
    } else if m.alt {
        w.yzoom = (amount / 200.0).exp();
    } else if m.shift {
        w.y_px = -amount;
    } else {
        w.zoom = pinch;
        w.time_px = -amount;
    }
    if let Some(dd) = middle {
        w.time_px -= dd.x;
        w.y_px -= dd.y;
    }
    (!w.is_none()).then_some(w)
}

/// Zoom dans le temps autour d'un point : `left` (temps au bord gauche), `pps` (pixels par seconde),
/// `at_px` (position du point, en pixels depuis le bord gauche)
pub fn zoom_time(left: &mut f64, pps: &mut f64, factor: f64, at_px: f64, min: f64, max: f64) {
    let t = *left + at_px / *pps;
    *pps = (*pps * factor).clamp(min, max);
    *left = (t - at_px / *pps).max(0.0);
}

/// Les raccourcis communs des vues de temps
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Keys {
    pub play: bool,
    pub home: bool,
    pub end: bool,
    /// ← → : une image (-1 ou +1)
    pub step: i32,
    /// Maj+← → : clé ou coupe précédente / suivante (-1 ou +1)
    pub jump: i32,
    pub fit: bool,
    /// = ou + : zoom avant (>1), − : arrière (<1)
    pub zoom: f32,
}

/// Les raccourcis, si aucun champ de texte n'a le clavier
pub fn keys(ui: &Ui) -> Keys {
    if ui.ctx().egui_wants_keyboard_input() {
        return Keys::default();
    }
    use egui::Key;
    ui.input(|i| {
        let plain = !i.modifiers.command && !i.modifiers.alt;
        let shift = i.modifiers.shift;
        let lr = |k: Key| i.key_pressed(k) && plain;
        Keys {
            play: i.key_pressed(Key::Space) && plain && !shift,
            home: i.key_pressed(Key::Home) && plain,
            end: i.key_pressed(Key::End) && plain,
            step: if !shift { lr(Key::ArrowRight) as i32 - lr(Key::ArrowLeft) as i32 } else { 0 },
            jump: if shift { lr(Key::ArrowRight) as i32 - lr(Key::ArrowLeft) as i32 } else { 0 },
            fit: i.key_pressed(Key::F) && plain && !shift,
            zoom: if (i.key_pressed(Key::Equals) || i.key_pressed(Key::Plus)) && plain {
                1.25
            } else if i.key_pressed(Key::Minus) && plain {
                0.8
            } else {
                1.0
            },
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zoom_autour_du_pointeur() {
        // 100 px/s, bord gauche à 10 s ; le point à 200 px (12 s) reste à sa place en zoomant
        let (mut left, mut pps) = (10.0, 100.0);
        zoom_time(&mut left, &mut pps, 2.0, 200.0, 1.0, 1000.0);
        assert_eq!(pps, 200.0);
        assert!((left + 200.0 / pps - 12.0).abs() < 1e-9);
        // Jamais avant zéro, et borné
        zoom_time(&mut left, &mut pps, 0.001, 0.0, 1.0, 1000.0);
        assert_eq!(pps, 1.0);
        assert!(left >= 0.0);
    }
}
