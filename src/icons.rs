//! Icônes dessinées (traits fins, une seule couleur), toutes sur la même grille : elles remplacent
//! les caractères Unicode, de tailles inégales selon la police.
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, Shape, Stroke};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum I {
    Object,
    Modifier,
    Material,
    Data,
    Scene,
    World,
    BoxSelect,
    Move,
    Rotate,
    Scale,
    Add,
    Frame,
    Image,
    Empty,
    Light,
    Camera,
    ShadeSolid,
    ShadeFlat,
    ShadeWhite,
    ShadeChrome,
    ShadeNormals,
    ShadeColor,
    Eye,
    EyeOff,
    #[allow(dead_code)]
    KeyOn,
    #[allow(dead_code)]
    KeyOff,
}

/// L'icône qui correspond à un ancien glyphe, s'il y en a une.
pub fn from_glyph(g: &str) -> Option<I> {
    Some(match g {
        "▣" => I::Object,
        "⚙" => I::Modifier,
        "●" => I::Material,
        "✦" => I::Data,
        "▤" => I::Scene,
        "◐" => I::World,
        "⬚" => I::BoxSelect,
        "✥" => I::Move,
        "⟳" => I::Rotate,
        "⤡" => I::Scale,
        "＋" | "+" => I::Add,
        "⌂" => I::Frame,
        "▨" => I::Image,
        "◇" => I::Empty,
        "☀" | "☼" | "✳" => I::Light,
        "◎" => I::Camera,
        _ => return None,
    })
}

/// Dessine un glyphe : en icône vectorielle si on en a une, sinon le caractère.
pub fn glyph(p: &egui::Painter, r: Rect, g: &str, col: Color32) {
    match from_glyph(g) {
        Some(i) => draw(p, r, i, col),
        None => {
            p.text(r.center(), egui::Align2::CENTER_CENTER, g, egui::FontId::proportional(r.height() * 0.8), col);
        }
    }
}

/// Dessine l'icône `i` centrée dans `r` (grille de 16 × 16 ramenée à la taille de `r`).
pub fn draw(p: &egui::Painter, r: Rect, i: I, col: Color32) {
    let s = r.width().min(r.height()) / 16.0;
    let o = r.center() - vec2(8.0, 8.0) * s;
    let at = |x: f32, y: f32| -> Pos2 { pos2(o.x + x * s, o.y + y * s) };
    let st = Stroke::new((1.3 * s).max(1.0), col);
    let line = |pts: &[(f32, f32)]| Shape::line(pts.iter().map(|(x, y)| at(*x, *y)).collect(), st);
    let closed = |pts: &[(f32, f32)]| Shape::closed_line(pts.iter().map(|(x, y)| at(*x, *y)).collect(), st);
    let fill = |pts: &[(f32, f32)]| Shape::convex_polygon(pts.iter().map(|(x, y)| at(*x, *y)).collect(), col, Stroke::NONE);
    let circle = |x: f32, y: f32, rr: f32| Shape::circle_stroke(at(x, y), rr * s, st);
    let dot = |x: f32, y: f32, rr: f32| Shape::circle_filled(at(x, y), rr * s, col);
    let arrow = |p: &egui::Painter, (x0, y0): (f32, f32), (x1, y1): (f32, f32)| {
        p.add(line(&[(x0, y0), (x1, y1)]));
        let (dx, dy) = (x1 - x0, y1 - y0);
        let l = (dx * dx + dy * dy).sqrt().max(1e-3);
        let (ux, uy) = (dx / l, dy / l);
        p.add(fill(&[(x1 + ux * 1.2, y1 + uy * 1.2), (x1 - ux * 2.2 - uy * 2.0, y1 - uy * 2.2 + ux * 2.0), (x1 - ux * 2.2 + uy * 2.0, y1 - uy * 2.2 - ux * 2.0)]));
    };
    match i {
        I::Object => {
            // Cube en perspective
            p.add(closed(&[(8.0, 2.0), (14.0, 5.0), (14.0, 11.5), (8.0, 14.5), (2.0, 11.5), (2.0, 5.0)]));
            p.add(line(&[(2.0, 5.0), (8.0, 8.0), (14.0, 5.0)]));
            p.add(line(&[(8.0, 8.0), (8.0, 14.5)]));
        }
        I::Modifier => {
            // Clé plate
            p.add(line(&[(3.0, 13.0), (9.0, 7.0)]));
            p.add(circle(10.8, 5.2, 3.2));
            p.add(fill(&[(11.0, 1.6), (14.4, 5.0), (12.0, 5.5), (10.5, 4.0)]));
        }
        I::Material => {
            p.add(circle(8.0, 8.0, 5.8));
            p.add(Shape::circle_filled(at(6.0, 6.0), 1.6 * s, col));
            p.add(line(&[(3.6, 10.0), (5.5, 12.0), (8.5, 13.3)]));
        }
        I::Data => {
            // Triangle de maillage : trois sommets reliés
            p.add(closed(&[(8.0, 2.5), (13.5, 12.5), (2.5, 12.5)]));
            for (x, y) in [(8.0, 2.5), (13.5, 12.5), (2.5, 12.5)] {
                p.add(dot(x, y, 1.6));
            }
        }
        I::Scene => {
            // Cône et sphère posés
            p.add(closed(&[(5.0, 3.0), (9.0, 12.5), (1.0, 12.5)]));
            p.add(circle(11.5, 9.5, 3.0));
            p.add(line(&[(0.5, 14.5), (15.5, 14.5)]));
        }
        I::World => {
            p.add(circle(8.0, 8.0, 6.0));
            p.add(line(&[(2.0, 8.0), (14.0, 8.0)]));
            let pts: Vec<(f32, f32)> = (0..=12).map(|k| {
                let a = -std::f32::consts::FRAC_PI_2 + k as f32 / 12.0 * std::f32::consts::PI;
                (8.0 + a.cos() * 2.8, 8.0 + a.sin() * 6.0)
            }).collect();
            p.add(line(&pts));
        }
        I::BoxSelect => {
            for (a, b) in [((2.0, 2.0), (6.0, 2.0)), ((9.0, 2.0), (13.0, 2.0)), ((2.0, 2.0), (2.0, 6.0)), ((2.0, 9.0), (2.0, 13.0)), ((13.0, 2.0), (13.0, 6.0)), ((2.0, 13.0), (6.0, 13.0))] {
                p.add(line(&[a, b]));
            }
            p.add(fill(&[(9.0, 9.0), (15.0, 11.5), (12.3, 12.3), (11.5, 15.0)]));
        }
        I::Move => {
            for (x, y) in [(8.0, 1.0), (8.0, 15.0), (1.0, 8.0), (15.0, 8.0)] {
                arrow(p, (8.0, 8.0), (8.0 + (x - 8.0) * 0.82, 8.0 + (y - 8.0) * 0.82));
            }
        }
        I::Rotate => {
            let pts: Vec<(f32, f32)> = (0..=20).map(|k| {
                let a = 0.6 + k as f32 / 20.0 * 4.9;
                (8.0 + a.cos() * 5.5, 8.0 + a.sin() * 5.5)
            }).collect();
            p.add(line(&pts));
            let a = 0.6f32;
            let (x, y) = (8.0 + a.cos() * 5.5, 8.0 + a.sin() * 5.5);
            p.add(fill(&[(x + 2.6, y - 0.4), (x - 1.2, y - 2.6), (x - 0.6, y + 1.8)]));
        }
        I::Scale => {
            p.add(closed(&[(2.0, 7.0), (9.0, 7.0), (9.0, 14.0), (2.0, 14.0)]));
            arrow(p, (6.0, 10.0), (13.0, 3.0));
        }
        I::Add => {
            p.add(line(&[(8.0, 2.5), (8.0, 13.5)]));
            p.add(line(&[(2.5, 8.0), (13.5, 8.0)]));
        }
        I::Frame => {
            for (x, y, dx, dy) in [(2.0, 2.0, 1.0, 1.0), (14.0, 2.0, -1.0, 1.0), (2.0, 14.0, 1.0, -1.0), (14.0, 14.0, -1.0, -1.0)] {
                p.add(line(&[(x + dx * 4.0, y), (x, y), (x, y + dy * 4.0)]));
            }
            p.add(dot(8.0, 8.0, 1.6));
        }
        I::Image => {
            p.add(closed(&[(2.0, 3.0), (14.0, 3.0), (14.0, 13.0), (2.0, 13.0)]));
            p.add(line(&[(2.5, 12.0), (6.5, 7.5), (9.5, 10.5), (11.0, 9.0), (13.5, 12.0)]));
            p.add(dot(11.0, 6.0, 1.3));
        }
        I::Empty => {
            p.add(line(&[(8.0, 2.0), (8.0, 14.0)]));
            p.add(line(&[(2.0, 8.0), (14.0, 8.0)]));
            p.add(line(&[(4.0, 12.0), (12.0, 4.0)]));
        }
        I::Light => {
            p.add(circle(8.0, 8.0, 3.0));
            for k in 0..8 {
                let a = k as f32 / 8.0 * std::f32::consts::TAU;
                p.add(line(&[(8.0 + a.cos() * 5.0, 8.0 + a.sin() * 5.0), (8.0 + a.cos() * 7.0, 8.0 + a.sin() * 7.0)]));
            }
        }
        I::Camera => {
            p.add(closed(&[(1.5, 5.0), (10.5, 5.0), (10.5, 12.0), (1.5, 12.0)]));
            p.add(closed(&[(10.5, 7.0), (14.5, 5.0), (14.5, 12.0), (10.5, 10.0)]));
        }
        I::ShadeSolid => {
            p.add(circle(8.0, 8.0, 6.0));
            let pts: Vec<Pos2> = (0..=16).map(|k| {
                let a = std::f32::consts::FRAC_PI_4 + k as f32 / 16.0 * std::f32::consts::PI;
                at(8.0 + a.cos() * 6.0, 8.0 + a.sin() * 6.0)
            }).collect();
            p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
        }
        I::ShadeFlat => {
            p.add(Shape::circle_filled(at(8.0, 8.0), 6.0 * s, col));
        }
        I::ShadeWhite => {
            p.add(circle(8.0, 8.0, 6.0));
        }
        I::ShadeChrome => {
            p.add(circle(8.0, 8.0, 6.0));
            p.add(line(&[(4.5, 6.0), (6.5, 4.5)]));
            p.add(line(&[(4.0, 9.0), (8.5, 5.0)]));
            p.add(line(&[(9.0, 12.5), (12.5, 9.5)]));
        }
        I::ShadeNormals => {
            p.add(circle(8.0, 8.0, 6.0));
            arrow(p, (8.0, 8.0), (8.0, 3.5));
            arrow(p, (8.0, 8.0), (12.0, 10.0));
        }
        I::ShadeColor => {
            p.add(circle(8.0, 8.0, 6.0));
            p.add(line(&[(2.5, 6.0), (13.5, 6.0)]));
            p.add(line(&[(2.5, 10.0), (13.5, 10.0)]));
            p.add(Shape::circle_filled(at(8.0, 8.0), 1.4 * s, col));
        }
        I::Eye | I::EyeOff => {
            let pts: Vec<(f32, f32)> = (0..=24).map(|k| {
                let a = k as f32 / 24.0 * std::f32::consts::TAU;
                (8.0 + a.cos() * 6.5, 8.0 + a.sin() * 3.5)
            }).collect();
            p.add(closed(&pts));
            p.add(Shape::circle_filled(at(8.0, 8.0), 1.8 * s, col));
            if i == I::EyeOff {
                p.add(line(&[(2.0, 14.0), (14.0, 2.0)]));
            }
        }
        I::KeyOn => {
            p.add(fill(&[(8.0, 2.5), (13.5, 8.0), (8.0, 13.5), (2.5, 8.0)]));
        }
        I::KeyOff => {
            p.add(closed(&[(8.0, 2.5), (13.5, 8.0), (8.0, 13.5), (2.5, 8.0)]));
        }
    }
}

/// Bouton carré à icône (allumé à la couleur de l'espace).
pub fn button(ui: &mut egui::Ui, i: I, on: bool, size: f32, tip: &str) -> egui::Response {
    let p = crate::theme::pal();
    let (r, resp) = ui.allocate_exact_size(vec2(size, size), egui::Sense::click());
    let fill = if on { p.accent } else if resp.hovered() { p.line } else { Color32::TRANSPARENT };
    ui.painter().rect_filled(r, 3.0, fill);
    draw(ui.painter(), r.shrink(size * 0.2), i, if on { p.on_accent } else { p.text });
    resp.on_hover_text(tip)
}
