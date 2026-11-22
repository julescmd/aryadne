//! Poignées de transformation dans la vue 3D (déplacer, tourner, taille), à la manière de Blender :
//! flèches et plans pour déplacer, anneaux pour tourner, cubes pour la taille, centre libre.
//! Le module ne connaît que l'écran et un espace 3D quelconque : l'appelant donne la projection
//! et applique ce que renvoie `drag` à son objet ou à ses sommets.

use eframe::egui;
use egui::{vec2, Color32, Painter, Pos2, Shape, Stroke, Vec2};

pub type V3 = [f32; 3];

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub const AXIS: [Color32; 3] = [Color32::from_rgb(237, 72, 88), Color32::from_rgb(132, 204, 52), Color32::from_rgb(64, 138, 245)];
const HOT: Color32 = Color32::from_rgb(255, 214, 92);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Move,
    Rotate,
    Scale,
}

impl Mode {
    pub fn from_u8(k: u8) -> Mode {
        match k {
            1 => Mode::Rotate,
            2 => Mode::Scale,
            _ => Mode::Move,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Handle {
    /// Le long d'un axe (déplacer, taille)
    Axis(usize),
    /// Dans le plan perpendiculaire à cet axe (déplacer, taille)
    Plane(usize),
    /// Centre : déplacement libre dans le plan de la vue, ou taille uniforme
    Center,
    /// Anneau autour d'un axe (tourner)
    Ring(usize),
    /// Grand anneau : rotation autour de l'axe de la vue
    View,
}

pub enum Delta {
    Move(V3),
    /// Axe (unitaire) et angle en radians
    Rotate(V3, f32),
    /// Facteur le long de chaque axe du rig
    Scale([f32; 3]),
}

/// Ce qu'il faut savoir pour placer la poignée à l'écran.
pub struct Rig<'a> {
    pub origin: V3,
    /// Axes unitaires
    pub axes: [V3; 3],
    /// Longueur des axes dans l'espace de la scène
    pub len: f32,
    /// Vers la caméra depuis l'origine (unitaire)
    pub toward: V3,
    /// Droite et haut de la vue (unitaires)
    pub right: V3,
    pub up: V3,
    /// Unités de la scène par pixel, à la profondeur de l'origine
    pub per_px: f32,
    pub project: &'a dyn Fn(V3) -> Option<Pos2>,
}

/// La poignée telle qu'elle tombe à l'écran.
pub struct Layout {
    pub center: Pos2,
    tips: [Option<Pos2>; 3],
    /// Les quatre coins de chaque poignée de plan
    planes: [Option<[Pos2; 4]>; 3],
    /// Anneaux : points et côté (vers la caméra ou non)
    rings: [Vec<(Pos2, bool)>; 3],
    view_r: f32,
    /// À quel point chaque axe est vu de bout (0 : de face, 1 : pointé vers nous)
    endon: [f32; 3],
}

impl Rig<'_> {
    pub fn layout(&self, mode: Mode) -> Option<Layout> {
        let center = (self.project)(self.origin)?;
        let at = |v: V3| (self.project)(add(self.origin, v));
        let tips = [0, 1, 2].map(|k| at(mul(self.axes[k], self.len)));
        let endon = [0, 1, 2].map(|k| dot(self.axes[k], self.toward).abs());
        let planes = [0, 1, 2].map(|k| {
            if mode == Mode::Rotate {
                return None;
            }
            let (a, b) = (self.axes[(k + 1) % 3], self.axes[(k + 2) % 3]);
            let (s0, s1) = (self.len * 0.22, self.len * 0.42);
            Some([at(add(mul(a, s0), mul(b, s0)))?, at(add(mul(a, s1), mul(b, s0)))?, at(add(mul(a, s1), mul(b, s1)))?, at(add(mul(a, s0), mul(b, s1)))?])
        });
        let rings = [0, 1, 2].map(|k| {
            if mode != Mode::Rotate {
                return vec![];
            }
            let (a, b) = (self.axes[(k + 1) % 3], self.axes[(k + 2) % 3]);
            (0..=72)
                .filter_map(|i| {
                    let t = i as f32 / 72.0 * std::f32::consts::TAU;
                    let v = add(mul(a, t.cos() * self.len), mul(b, t.sin() * self.len));
                    at(v).map(|p| (p, dot(v, self.toward) >= -1e-4))
                })
                .collect()
        });
        let view_r = self.len / self.per_px.max(1e-9) * 1.2;
        Some(Layout { center, tips, planes, rings, view_r, endon })
    }
}

fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t - p).length()
}

fn inside(p: Pos2, quad: &[Pos2; 4]) -> bool {
    let mut sign = 0.0f32;
    for i in 0..4 {
        let (a, b) = (quad[i], quad[(i + 1) % 4]);
        let c = (b - a).x * (p - a).y - (b - a).y * (p - a).x;
        if c.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = c.signum();
        } else if c.signum() != sign {
            return false;
        }
    }
    true
}

/// Les axes trop vus de bout disparaissent, comme dans Blender : impossible de les tirer proprement.
fn usable(l: &Layout, k: usize) -> bool {
    l.endon[k] < 0.96
}
fn plane_usable(l: &Layout, k: usize) -> bool {
    l.endon[k] > 0.2
}

impl Layout {
    /// La poignée sous le pointeur, la plus précise d'abord.
    pub fn hit(&self, mode: Mode, p: Pos2) -> Option<Handle> {
        let to_center = (p - self.center).length();
        match mode {
            Mode::Rotate => {
                // L'anneau le plus proche, la moitié tournée vers nous passe devant
                let mut best: Option<(Handle, f32)> = None;
                for k in 0..3 {
                    for w in self.rings[k].windows(2) {
                        let d = seg_dist(p, w[0].0, w[1].0) + if w[0].1 { 0.0 } else { 3.0 };
                        if d < 8.0 && best.is_none_or(|b| d < b.1) {
                            best = Some((Handle::Ring(k), d));
                        }
                    }
                }
                if let Some(b) = best {
                    return Some(b.0);
                }
                ((to_center - self.view_r).abs() < 7.0).then_some(Handle::View)
            }
            _ => {
                if to_center < 11.0 {
                    return Some(Handle::Center);
                }
                let mut best: Option<(usize, f32)> = None;
                for k in 0..3 {
                    if let (Some(t), true) = (self.tips[k], usable(self, k)) {
                        // La pointe dépasse un peu le bout de l'axe
                        let tip = t + (t - self.center).normalized() * 10.0;
                        let d = seg_dist(p, self.center, tip);
                        if d < 9.0 && (p - self.center).length() > 14.0 && best.is_none_or(|b| d < b.1) {
                            best = Some((k, d));
                        }
                    }
                }
                if let Some((k, _)) = best {
                    return Some(Handle::Axis(k));
                }
                (0..3).find(|&k| plane_usable(self, k) && self.planes[k].as_ref().is_some_and(|q| inside(p, q))).map(Handle::Plane)
            }
        }
    }
}

/// Ce que devient le glisser d'une poignée, de `prev` à `cur` (en pixels).
pub fn drag(rig: &Rig, l: &Layout, mode: Mode, h: Handle, prev: Pos2, cur: Pos2) -> Option<Delta> {
    let d = cur - prev;
    if d == Vec2::ZERO {
        return None;
    }
    let screen = |k: usize| l.tips[k].map(|t| t - l.center).filter(|v| v.length_sq() > 4.0);
    match (mode, h) {
        (Mode::Move, Handle::Axis(k)) => {
            let e = screen(k)?;
            let t = d.dot(e) / e.length_sq();
            Some(Delta::Move(mul(rig.axes[k], t * rig.len)))
        }
        (Mode::Move, Handle::Plane(k)) => {
            let (a, b) = ((k + 1) % 3, (k + 2) % 3);
            let (ea, eb) = (screen(a)?, screen(b)?);
            let det = ea.x * eb.y - ea.y * eb.x;
            if det.abs() < 1e-3 {
                return None;
            }
            let s = (d.x * eb.y - d.y * eb.x) / det;
            let t = (ea.x * d.y - ea.y * d.x) / det;
            Some(Delta::Move(add(mul(rig.axes[a], s * rig.len), mul(rig.axes[b], t * rig.len))))
        }
        (Mode::Move, Handle::Center) => Some(Delta::Move(add(mul(rig.right, d.x * rig.per_px), mul(rig.up, -d.y * rig.per_px)))),
        (Mode::Scale, Handle::Axis(k)) => {
            let e = screen(k)?.normalized();
            let (a, b) = ((prev - l.center).dot(e), (cur - l.center).dot(e));
            let f = if a.abs() > 6.0 { (b / a).clamp(0.2, 5.0) } else { 1.0 + d.dot(e) * 0.01 };
            let mut s = [1.0; 3];
            s[k] = f;
            Some(Delta::Scale(s))
        }
        (Mode::Scale, Handle::Plane(_)) | (Mode::Scale, Handle::Center) => {
            let (a, b) = ((prev - l.center).length(), (cur - l.center).length());
            let f = if a > 6.0 { (b / a).clamp(0.2, 5.0) } else { 1.0 };
            let mut s = [f; 3];
            if let Handle::Plane(k) = h {
                s[k] = 1.0;
            }
            Some(Delta::Scale(s))
        }
        (Mode::Rotate, Handle::Ring(_)) | (Mode::Rotate, Handle::View) => {
            let axis = match h {
                Handle::Ring(k2) => rig.axes[k2],
                _ => rig.toward,
            };
            let (a, b) = (prev - l.center, cur - l.center);
            if a.length() < 3.0 || b.length() < 3.0 {
                return None;
            }
            let mut turn = b.angle() - a.angle();
            if turn > std::f32::consts::PI {
                turn -= std::f32::consts::TAU;
            } else if turn < -std::f32::consts::PI {
                turn += std::f32::consts::TAU;
            }
            // L'écran a son Y vers le bas : un tour dans le sens horaire est négatif vu de face
            let facing = if dot(axis, rig.toward) >= 0.0 { 1.0 } else { -1.0 };
            Some(Delta::Rotate(axis, -turn * facing))
        }
        _ => None,
    }
}

fn arrow_head(sh: &mut Vec<Shape>, base: Pos2, dir: Vec2, len: f32, w: f32, col: Color32) {
    let n = vec2(-dir.y, dir.x);
    let tip = base + dir * len;
    sh.push(Shape::convex_polygon(vec![tip, base + n * w, base - n * w], col, Stroke::NONE));
    // Un petit rebord clair donne du volume à la pointe
    sh.push(Shape::line_segment([base + n * w, tip], Stroke::new(1.0, col.gamma_multiply(1.4))));
}

/// Dessine la poignée. `hot` : poignée survolée ; `active` : poignée tenue ;
/// `swept` : angle parcouru depuis le début d'une rotation (radians) et point de départ.
pub fn draw(painter: &Painter, rig: &Rig, l: &Layout, mode: Mode, hot: Option<Handle>, active: Option<Handle>, swept: Option<(f32, Pos2)>) {
    let mut sh: Vec<Shape> = vec![];
    let shadow = Color32::from_black_alpha(90);
    let lit = |h: Handle, col: Color32| if active == Some(h) || (active.is_none() && hot == Some(h)) { HOT } else { col };
    let busy = active.is_some();
    match mode {
        Mode::Rotate => {
            // Grand anneau de la vue
            let vh = Handle::View;
            if !busy || active == Some(vh) {
                sh.push(Shape::circle_stroke(l.center, l.view_r, Stroke::new(2.0, lit(vh, Color32::from_white_alpha(190)))));
            }
            // Fond discret dans la boule, comme une sphère qu'on attrape
            if !busy {
                sh.push(Shape::circle_filled(l.center, l.view_r * 0.83, Color32::from_white_alpha(10)));
            }
            // Les moitiés éloignées d'abord, pâles, puis les moitiés proches par-dessus
            for front in [false, true] {
                for k in 0..3 {
                    let h = Handle::Ring(k);
                    if busy && active != Some(h) {
                        continue;
                    }
                    let col = lit(h, AXIS[k]);
                    let pts = &l.rings[k];
                    let mut run: Vec<Pos2> = vec![];
                    let flush = |run: &mut Vec<Pos2>, sh: &mut Vec<Shape>| {
                        if run.len() >= 2 {
                            if front {
                                sh.push(Shape::line(run.clone(), Stroke::new(4.5, shadow)));
                                sh.push(Shape::line(std::mem::take(run), Stroke::new(if Some(h) == active || Some(h) == hot { 3.6 } else { 2.6 }, col)));
                            } else {
                                sh.push(Shape::line(std::mem::take(run), Stroke::new(1.4, col.gamma_multiply(0.35))));
                            }
                        }
                        run.clear();
                    };
                    for w in pts.windows(2) {
                        if w[0].1 == front || w[1].1 == front {
                            if run.is_empty() {
                                run.push(w[0].0);
                            }
                            run.push(w[1].0);
                        } else {
                            flush(&mut run, &mut sh);
                        }
                    }
                    flush(&mut run, &mut sh);
                }
            }
            // Pendant une rotation : la part de tarte parcourue et l'angle
            if let (Some((angle, from)), Some(h)) = (swept, active) {
                let col = match h {
                    Handle::Ring(k) => AXIS[k],
                    _ => Color32::WHITE,
                };
                let r = (from - l.center).length().clamp(20.0, l.view_r);
                let a0 = (from - l.center).angle();
                let facing = match h {
                    Handle::Ring(k) => if dot(rig.axes[k], rig.toward) >= 0.0 { 1.0 } else { -1.0 },
                    _ => 1.0,
                };
                let span = -angle * facing;
                let steps = ((span.abs() / 0.05) as usize).clamp(2, 400);
                let mut fan = vec![l.center];
                for i in 0..=steps {
                    let t = a0 + span * i as f32 / steps as f32;
                    fan.push(l.center + Vec2::angled(t) * r);
                }
                // Un polygone en éventail n'est pas convexe au-delà d'un demi-tour : on le découpe
                for w in fan[1..].windows(2) {
                    sh.push(Shape::convex_polygon(vec![l.center, w[0], w[1]], col.gamma_multiply(0.16), Stroke::NONE));
                }
                sh.push(Shape::line_segment([l.center, l.center + Vec2::angled(a0) * r], Stroke::new(1.2, col)));
                sh.push(Shape::line_segment([l.center, l.center + Vec2::angled(a0 + span) * r], Stroke::new(1.2, col)));
                let deg = angle.to_degrees();
                text_tag(&mut sh, painter, l.center + Vec2::angled(a0 + span) * (r + 18.0), &format!("{deg:+.1}°"), col);
            }
            sh.push(Shape::circle_filled(l.center, 3.0, Color32::WHITE));
        }
        Mode::Move | Mode::Scale => {
            // Poignées de plan
            for k in 0..3 {
                let h = Handle::Plane(k);
                if !plane_usable(l, k) || (busy && active != Some(h)) {
                    continue;
                }
                if let Some(q) = l.planes[k] {
                    let col = lit(h, AXIS[k]);
                    let fade = ((l.endon[k] - 0.2) / 0.3).clamp(0.0, 1.0);
                    sh.push(Shape::convex_polygon(q.to_vec(), col.gamma_multiply(0.35 * fade), Stroke::new(1.2, col.gamma_multiply(0.9 * fade))));
                }
            }
            // Axes, du plus loin au plus proche
            let mut order: Vec<usize> = (0..3).collect();
            order.sort_by(|a, b| dot(rig.axes[*a], rig.toward).total_cmp(&dot(rig.axes[*b], rig.toward)));
            for k in order {
                let h = Handle::Axis(k);
                let Some(t) = l.tips[k] else { continue };
                if !usable(l, k) || (busy && active != Some(h)) {
                    continue;
                }
                let fade = ((0.96 - l.endon[k]) / 0.15).clamp(0.0, 1.0);
                let col = lit(h, AXIS[k]).gamma_multiply(fade);
                let dir = (t - l.center).normalized();
                let start = l.center + dir * 14.0;
                let w = if Some(h) == active || Some(h) == hot { 3.4 } else { 2.4 };
                sh.push(Shape::line_segment([start, t], Stroke::new(w + 2.0, shadow.gamma_multiply(fade))));
                sh.push(Shape::line_segment([start, t], Stroke::new(w, col)));
                if mode == Mode::Move {
                    arrow_head(&mut sh, t - dir * 2.0, dir, 16.0, 6.0, col);
                } else {
                    let s = 6.5;
                    let n = vec2(-dir.y, dir.x);
                    let c = t + dir * s;
                    sh.push(Shape::convex_polygon(vec![c - dir * s - n * s, c - dir * s + n * s, c + dir * s + n * s, c + dir * s - n * s], col, Stroke::new(1.0, shadow)));
                }
            }
            // Centre : déplacement libre (anneau) ou taille uniforme (anneau plus large)
            let h = Handle::Center;
            if !busy || active == Some(h) {
                let col = lit(h, Color32::WHITE);
                let r = if mode == Mode::Scale { 12.0 } else { 9.0 };
                sh.push(Shape::circle_stroke(l.center, r, Stroke::new(3.5, shadow)));
                sh.push(Shape::circle_stroke(l.center, r, Stroke::new(if hot == Some(h) || active == Some(h) { 2.4 } else { 1.6 }, col)));
            }
            sh.push(Shape::circle_filled(l.center, 2.5, Color32::WHITE));
            // Pendant un déplacement sur un axe : l'axe se prolonge dans toute la vue
            if let Some(Handle::Axis(k)) = active {
                if let Some(t) = l.tips[k] {
                    let dir = (t - l.center).normalized();
                    let far = 4000.0;
                    sh.insert(0, Shape::line_segment([l.center - dir * far, l.center + dir * far], Stroke::new(1.0, AXIS[k].gamma_multiply(0.7))));
                }
            }
        }
    }
    painter.extend(sh);
}

/// Petite étiquette de valeur, sur fond sombre.
pub fn text_tag(sh: &mut Vec<Shape>, painter: &Painter, at: Pos2, text: &str, col: Color32) {
    let galley = painter.layout_no_wrap(text.to_string(), egui::FontId::monospace(11.5), col);
    let r = egui::Rect::from_center_size(at, galley.size() + vec2(10.0, 4.0));
    sh.push(Shape::rect_filled(r, 3.0, Color32::from_black_alpha(170)));
    sh.push(Shape::galley(r.min + vec2(5.0, 2.0), galley, col));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn ortho(v: V3) -> Option<Pos2> {
        Some(pos2(400.0 + v[0] * 100.0, 300.0 - v[1] * 100.0))
    }

    fn rig(proj: &dyn Fn(V3) -> Option<Pos2>) -> Rig<'_> {
        // Vue de trois quarts : Z ne pointe pas pile vers la caméra
        Rig { origin: [0.0; 3], axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], len: 1.0, toward: [0.0, 0.0, 1.0], right: [1.0, 0.0, 0.0], up: [0.0, 1.0, 0.0], per_px: 0.01, project: proj }
    }

    #[test]
    fn poignees() {
        let r = rig(&ortho);
        let l = r.layout(Mode::Move).unwrap();
        assert_eq!(l.hit(Mode::Move, pos2(400.0, 300.0)), Some(Handle::Center));
        assert_eq!(l.hit(Mode::Move, pos2(470.0, 301.0)), Some(Handle::Axis(0)));
        assert_eq!(l.hit(Mode::Move, pos2(401.0, 230.0)), Some(Handle::Axis(1)));
        // Plan XY (perpendiculaire à Z) : entre 0.22 et 0.42 sur X et Y
        assert_eq!(l.hit(Mode::Move, pos2(432.0, 268.0)), Some(Handle::Plane(2)));
        match drag(&r, &l, Mode::Move, Handle::Axis(0), pos2(470.0, 300.0), pos2(520.0, 300.0)) {
            Some(Delta::Move(m)) => assert!((m[0] - 0.5).abs() < 1e-4 && m[1].abs() < 1e-6),
            _ => panic!(),
        }
        match drag(&r, &l, Mode::Move, Handle::Plane(2), pos2(430.0, 270.0), pos2(450.0, 250.0)) {
            Some(Delta::Move(m)) => assert!((m[0] - 0.2).abs() < 1e-4 && (m[1] - 0.2).abs() < 1e-4),
            _ => panic!(),
        }
        let l = r.layout(Mode::Rotate).unwrap();
        // L'anneau de Z (vu de face) est un cercle de 100 px
        assert_eq!(l.hit(Mode::Rotate, pos2(400.0 + 70.7, 300.0 - 70.7)), Some(Handle::Ring(2)));
        match drag(&r, &l, Mode::Rotate, Handle::Ring(2), pos2(500.0, 300.0), pos2(400.0, 200.0)) {
            // Du bas-droite vers le haut, vu de face : sens trigonométrique, angle positif
            Some(Delta::Rotate(a, t)) => assert!(a == [0.0, 0.0, 1.0] && (t - std::f32::consts::FRAC_PI_2).abs() < 1e-3),
            _ => panic!(),
        }
    }
}
