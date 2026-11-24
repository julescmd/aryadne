//! Les clés d'animation, pareilles partout (scène 3D, Motion, montage) : les mêmes lissages, la même
//! forme de losange selon le lissage, le même menu au clic droit avec l'aperçu de chaque courbe.
//!
//! Formes : losange = doux, carré = linéaire, sablier = arrivée ou départ en douceur, rond = rebond ou
//! ressort, carré coupé = constant (la valeur tient jusqu'à la clé suivante).

use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, Stroke};
use serde::{Deserialize, Serialize};

/// Passage d'une clé vers la suivante
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Debug, Hash)]
pub enum Ease {
    Linear,
    #[default]
    Smooth,
    /// Part lentement
    In,
    /// Arrive lentement
    Out,
    Bounce,
    Spring,
    /// La valeur reste jusqu'à la clé suivante
    Hold,
}

/// Tous les lissages, dans l'ordre des menus : (lissage, nom, raccourci dans le menu)
pub const EASES: [(Ease, &str); 7] = [(Ease::Smooth, "Doux"), (Ease::Linear, "Linéaire"), (Ease::In, "Départ doux"), (Ease::Out, "Arrivée douce"), (Ease::Bounce, "Rebond"), (Ease::Spring, "Ressort"), (Ease::Hold, "Constant")];

impl Ease {
    pub fn name(self) -> &'static str {
        EASES.iter().find(|e| e.0 == self).map(|e| e.1).unwrap_or("")
    }

    /// De 0 à 1 ; peut dépasser 1 en chemin (rebond, ressort) mais arrive toujours à 1
    pub fn apply(self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        match self {
            Ease::Linear => x,
            Ease::Smooth => {
                if x < 0.5 {
                    4.0 * x * x * x
                } else {
                    1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
                }
            }
            Ease::In => x * x * x,
            Ease::Out => 1.0 - (1.0 - x).powi(3),
            Ease::Bounce => {
                let (c1, c3) = (1.70158, 2.70158);
                1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
            }
            Ease::Spring => 1.0 - (-6.0 * x).exp() * (4.5 * std::f32::consts::PI * x).cos(),
            Ease::Hold => {
                if x >= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }

    /// Pour la scène 3D : le mot gardé dans le fichier (« bezier » reste à part, avec ses poignées)
    pub fn word(self) -> &'static str {
        match self {
            Ease::Linear => "linear",
            Ease::Smooth => "smooth",
            Ease::In => "in",
            Ease::Out => "out",
            Ease::Bounce => "bounce",
            Ease::Spring => "spring",
            Ease::Hold => "constant",
        }
    }

    pub fn from_word(w: &str) -> Option<Ease> {
        EASES.iter().map(|e| e.0).find(|e| e.word() == w)
    }
}

/// Dessine une clé : sa forme dit son lissage ; pleine si choisie ou sous la tête de lecture
pub fn draw(painter: &egui::Painter, c: Pos2, r: f32, ease: Option<Ease>, color: Color32, filled: bool, selected: bool) {
    let fill = if filled { color } else { Color32::TRANSPARENT };
    let stroke = Stroke::new(if selected { 2.0 } else { 1.2 }, if selected { Color32::WHITE } else { color });
    match ease {
        // Bézier (poignées libres) : losange, comme doux
        None | Some(Ease::Smooth) => {
            painter.add(egui::Shape::convex_polygon(vec![pos2(c.x, c.y - r), pos2(c.x + r, c.y), pos2(c.x, c.y + r), pos2(c.x - r, c.y)], fill, stroke));
        }
        Some(Ease::Linear) => {
            painter.rect(Rect::from_center_size(c, vec2(r * 1.5, r * 1.5)), 0.0, fill, stroke, egui::StrokeKind::Middle);
        }
        Some(Ease::In) | Some(Ease::Out) => {
            // Sablier : le côté plat est celui qui s'adoucit
            let flip = if ease == Some(Ease::In) { -1.0 } else { 1.0 };
            let pts = vec![pos2(c.x - r * flip, c.y - r), pos2(c.x + r * 0.2 * flip, c.y), pos2(c.x - r * flip, c.y + r), pos2(c.x + r * flip, c.y + r * 0.6), pos2(c.x + r * flip, c.y - r * 0.6)];
            painter.add(egui::Shape::closed_line(pts.clone(), stroke));
            if filled {
                painter.add(egui::Shape::convex_polygon(vec![pts[0], pts[4], pts[3], pts[2]], fill, Stroke::NONE));
            }
        }
        Some(Ease::Bounce) | Some(Ease::Spring) => {
            painter.circle(c, r * 0.85, fill, stroke);
        }
        Some(Ease::Hold) => {
            let q = Rect::from_center_size(c, vec2(r * 1.5, r * 1.5));
            painter.rect(q, 0.0, fill, stroke, egui::StrokeKind::Middle);
            painter.line_segment([pos2(q.left(), q.center().y), pos2(q.right(), q.center().y)], Stroke::new(1.0, if filled { Color32::from_black_alpha(160) } else { color }));
        }
    }
}

/// Petit dessin de la courbe d'un lissage (pour les menus)
pub fn mini_curve(painter: &egui::Painter, r: Rect, ease: Ease, color: Color32) {
    painter.rect_stroke(r, 2.0, Stroke::new(1.0, color.gamma_multiply(0.3)), egui::StrokeKind::Inside);
    let inner = r.shrink(3.0);
    let pts: Vec<Pos2> = (0..=24)
        .map(|k| {
            let x = k as f32 / 24.0;
            let y = ease.apply(x);
            pos2(inner.left() + inner.width() * x, inner.bottom() - inner.height() * 0.8 * y - inner.height() * 0.1)
        })
        .collect();
    painter.add(egui::Shape::line(pts, Stroke::new(1.5, color)));
}

/// Le menu des lissages (clic droit sur une clé). `bezier` : propose aussi Bézier (scène 3D).
/// Rend le choix : Some(None) pour Bézier, Some(Some(lissage)) sinon.
pub fn menu(ui: &mut egui::Ui, current: Option<Ease>, bezier: bool) -> Option<Option<Ease>> {
    let p = crate::theme::pal();
    let mut out = None;
    ui.set_max_width(210.0);
    ui.label(egui::RichText::new(crate::i18n::t("Vers la clé suivante")).size(11.0).color(p.muted));
    let row = |ui: &mut egui::Ui, label: String, ease: Option<Ease>, on: bool| -> bool {
        let (r, resp) = ui.allocate_exact_size(vec2(200.0, 26.0), egui::Sense::click());
        if resp.hovered() || on {
            ui.painter().rect_filled(r, 3.0, if on { p.selection } else { p.raised });
        }
        let icon = Rect::from_min_size(pos2(r.left() + 6.0, r.top() + 3.0), vec2(28.0, 20.0));
        match ease {
            Some(e) => mini_curve(ui.painter(), icon, e, p.accent),
            None => {
                // Bézier : la courbe douce, avec ses poignées
                mini_curve(ui.painter(), icon, Ease::Smooth, p.accent);
                ui.painter().circle_filled(pos2(icon.left() + 9.0, icon.bottom() - 5.0), 2.0, p.text);
                ui.painter().circle_filled(pos2(icon.right() - 9.0, icon.top() + 5.0), 2.0, p.text);
            }
        }
        draw(ui.painter(), pos2(r.left() + 46.0, r.center().y), 5.0, ease, p.text, true, false);
        ui.painter().text(pos2(r.left() + 60.0, r.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(12.5), p.text);
        resp.clicked()
    };
    if bezier && row(ui, crate::i18n::t("Bézier (poignées)"), None, current.is_none()) {
        out = Some(None);
    }
    for (e, name) in EASES {
        if row(ui, crate::i18n::t(name), Some(e), current == Some(e)) {
            out = Some(Some(e));
        }
    }
    if out.is_some() {
        ui.close();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lissages() {
        for (e, _) in EASES {
            assert!((e.apply(1.0) - 1.0).abs() < 1e-3, "{e:?} arrive à 1");
            assert!(e.apply(0.0).abs() < 1e-3, "{e:?} part de 0");
            if e != Ease::Hold {
                assert_eq!(Ease::from_word(e.word()), Some(e));
            }
        }
        assert!(Ease::In.apply(0.3) < 0.1 && Ease::Out.apply(0.3) > 0.6);
        assert_eq!(Ease::from_word("constant"), Some(Ease::Hold));
        assert_eq!(Ease::from_word("bezier"), None);
    }

    #[test]
    fn memes_lissages_partout() {
        // Montage : une clé « arrivée douce » donne des points intermédiaires qui suivent la courbe
        let k = |t: f32, x: f32, ease: Ease| crate::montage::PoseKey { t, x, y: 0.0, scale: 1.0, opacity: 1.0, ease, tan: [None; 4] };
        let keys = [k(0.0, 0.0, Ease::Out), k(1.0, 100.0, Ease::Linear)];
        let pts = crate::montage::eased_points(&keys, 2.0, &|q| q.x);
        assert_eq!(pts.first(), Some(&(2.0, 0.0)));
        assert_eq!(pts.last(), Some(&(3.0, 100.0)));
        let mid = pts.iter().find(|p| (p.0 - 2.5).abs() < 1e-4).unwrap();
        assert!((mid.1 - Ease::Out.apply(0.5) * 100.0).abs() < 1e-3);
        // Scène 3D : le même lissage à mi-chemin
        let mut a = crate::scene::Key { frame: 0, pos: [0.0; 3], rot: [0.0; 3], scale: [1.0; 3], ease: true, interp: Ease::Out.word().into(), handle: 0, handles: vec![] };
        let b = crate::scene::Key { frame: 10, pos: [10.0, 0.0, 0.0], ..a.clone() };
        let v = crate::scene::channel_at(&[a.clone(), b.clone()], 0, 5.0);
        assert!((v - Ease::Out.apply(0.5) * 10.0).abs() < 1e-3, "{v}");
        a.interp = "constant".into();
        assert_eq!(crate::scene::channel_at(&[a, b], 0, 9.0), 0.0);
    }
}
