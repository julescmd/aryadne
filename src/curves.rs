//! Éditeur de courbes commun, à la manière du Graph Editor de Maya : une courbe par propriété et par axe,
//! clés et tangentes à attraper, types de tangente (auto, spline, linéaire, plate, en escalier) et les
//! lissages communs de l'app. Sert au Motion et au montage ; chaque partie convertit ses clés en courbes
//! et retour. Les courbes d'un même « lien » partagent les instants de leurs clés (une clé de position
//! porte X et Y) : les déplacer dans le temps ou les supprimer agit sur tout le lien.

use crate::keys::Ease;
use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, Sense, Shape, Stroke};

/// Une clé de courbe : instant (s), valeur, passage vers la suivante, tangentes (gauche Δt Δv, droite Δt Δv)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CK {
    pub t: f32,
    pub v: f32,
    pub ease: Ease,
    pub tan: Option<[f32; 4]>,
}

#[derive(Clone, Debug)]
pub struct Curve {
    pub label: String,
    pub color: Color32,
    pub keys: Vec<CK>,
    pub link: u32,
}

pub const X: Color32 = Color32::from_rgb(237, 72, 88);
pub const Y: Color32 = Color32::from_rgb(120, 196, 52);
pub const Z: Color32 = Color32::from_rgb(64, 138, 245);
pub const W: Color32 = Color32::from_rgb(209, 175, 117);

/// Tangentes automatiques : à plat sur un sommet ou un creux, sinon la pente des voisines (comme « auto » de Maya)
pub fn auto_tan(keys: &[CK], i: usize) -> [f32; 4] {
    let k = keys[i];
    let prev = (i > 0).then(|| keys[i - 1]);
    let next = keys.get(i + 1).copied();
    let ll = prev.map(|p| (k.t - p.t) / 3.0).unwrap_or(0.3);
    let rl = next.map(|n| (n.t - k.t) / 3.0).unwrap_or(0.3);
    let slope = match (prev, next) {
        (Some(p), Some(n)) if !((k.v >= p.v && k.v >= n.v) || (k.v <= p.v && k.v <= n.v)) => (n.v - p.v) / (n.t - p.t).max(1e-6),
        _ => 0.0,
    };
    [-ll, -slope * ll, rl, slope * rl]
}

pub fn tangents(keys: &[CK], i: usize) -> [f32; 4] {
    keys[i].tan.unwrap_or_else(|| auto_tan(keys, i))
}

/// Valeur d'une courbe à l'instant `t`
pub fn eval(keys: &[CK], t: f32) -> f32 {
    let n = keys.len();
    if n == 0 {
        return 0.0;
    }
    if t <= keys[0].t {
        return keys[0].v;
    }
    if t >= keys[n - 1].t {
        return keys[n - 1].v;
    }
    let i = (0..n - 1).find(|&i| t < keys[i + 1].t).unwrap_or(0);
    segment(keys, i, t)
}

/// Le segment de `i` à `i + 1` : Bézier si l'une des deux clés a des tangentes posées, sinon le lissage
pub fn segment(keys: &[CK], i: usize, t: f32) -> f32 {
    let (a, b) = (keys[i], keys[i + 1]);
    let span = (b.t - a.t).max(1e-6);
    if a.tan.is_none() && b.tan.is_none() {
        return a.v + (b.v - a.v) * a.ease.apply((t - a.t) / span);
    }
    let (r, l) = (tangents(keys, i), tangents(keys, i + 1));
    bezier(a.t, a.v, [r[2], r[3]], b.t, b.v, [l[0], l[1]], t)
}

/// Bézier d'un segment, les poignées gardées dans le segment (la courbe reste une fonction du temps)
pub fn bezier(ta: f32, va: f32, r: [f32; 2], tb: f32, vb: f32, l: [f32; 2], t: f32) -> f32 {
    let span = (tb - ta).max(1e-6);
    let x1 = ta + r[0].clamp(0.0, span);
    let x2 = tb + l[0].clamp(-span, 0.0);
    let y1 = va + r[1] * if r[0] > span { span / r[0] } else { 1.0 };
    let y2 = vb + l[1] * if -l[0] > span { span / -l[0] } else { 1.0 };
    let bez = |p0: f32, p1: f32, p2: f32, p3: f32, s: f32| {
        let u = 1.0 - s;
        u * u * u * p0 + 3.0 * u * u * s * p1 + 3.0 * u * s * s * p2 + s * s * s * p3
    };
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..30 {
        let m = (lo + hi) / 2.0;
        if bez(ta, x1, x2, tb, m) < t {
            lo = m;
        } else {
            hi = m;
        }
    }
    bez(va, y1, y2, vb, (lo + hi) / 2.0)
}

#[derive(Clone)]
enum Drag {
    Keys { from: Pos2, orig: Vec<Curve> },
    Tan { c: usize, k: usize, right: bool },
    Box { from: Pos2 },
    Pan { last: Pos2 },
    Scrub,
}

pub struct Editor {
    view: Option<[f32; 4]>,
    pub sel: Vec<(usize, usize)>,
    hidden: Vec<String>,
    drag: Option<Drag>,
    /// Ce qu'on montre : quand ça change, la vue se recadre
    pub who: String,
}

impl Default for Editor {
    fn default() -> Self {
        Editor { view: None, sel: vec![], hidden: vec![], drag: None, who: String::new() }
    }
}

pub struct Out {
    pub changed: bool,
    pub seek: Option<f32>,
}

impl Editor {
    pub fn show_for(&mut self, who: &str) {
        if self.who != who {
            self.who = who.to_string();
            self.view = None;
            self.sel.clear();
        }
    }

    fn fit(&self, curves: &[Curve], range: (f32, f32)) -> [f32; 4] {
        let (mut v0, mut v1, mut t0, mut t1) = (f32::MAX, f32::MIN, range.0, range.1);
        for c in curves.iter().filter(|c| !self.hidden.contains(&c.label)) {
            for k in &c.keys {
                (v0, v1, t0, t1) = (v0.min(k.v), v1.max(k.v), t0.min(k.t), t1.max(k.t));
            }
            if c.keys.len() > 1 {
                let (a, b) = (c.keys[0].t, c.keys[c.keys.len() - 1].t);
                for i in 0..=64 {
                    let v = eval(&c.keys, a + (b - a) * i as f32 / 64.0);
                    (v0, v1) = (v0.min(v), v1.max(v));
                }
            }
        }
        if v0 > v1 {
            (v0, v1) = (-1.0, 1.0);
        }
        let pad = ((v1 - v0) * 0.12).max(0.05);
        let tp = ((t1 - t0) * 0.04).max(0.05);
        [t0 - tp, t1 + tp, v0 - pad, v1 + pad]
    }

    /// L'éditeur : `curves` est modifié sur place. `fps` arrondit les déplacements à l'image.
    pub fn ui(&mut self, ui: &mut egui::Ui, curves: &mut [Curve], now: f32, range: (f32, f32), fps: f32) -> Out {
        let p = theme::pal();
        let mut out = Out { changed: false, seek: None };
        self.sel.retain(|&(c, k)| curves.get(c).is_some_and(|cv| k < cv.keys.len()));
        let snap = |t: f32| if fps > 0.0 { (t * fps).round() / fps } else { t };
        // ---- Barre : types de tangente (comme Maya), lissages, cadrer
        let mut set_tan: Option<&str> = None;
        let mut set_ease: Option<Ease> = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let any = !self.sel.is_empty();
            ui.label(egui::RichText::new(crate::i18n::t("tangentes")).size(10.5).color(p.muted));
            for (id, label, tip) in [("auto", "Auto", "Plates aux sommets, sinon suivent les voisines"), ("spline", "Spline", "Lisses : suivent la pente des voisines"), ("linear", "Linéaire", "Droites vers les clés voisines"), ("flat", "Plate", "Horizontales : départ et arrivée en douceur"), ("step", "Escalier", "La valeur tient jusqu'à la clé suivante")] {
                if ui.add_enabled(any, egui::Button::new(egui::RichText::new(crate::i18n::t(label)).size(11.0))).on_hover_text(crate::i18n::t(tip)).clicked() {
                    set_tan = Some(id);
                }
            }
            ui.separator();
            ui.add_enabled_ui(any, |ui| {
                ui.menu_button(egui::RichText::new(format!("{} ▾", crate::i18n::t("Lissage"))).size(11.0), |ui| {
                    if let Some(Some(e)) = crate::keys::menu(ui, None, false) {
                        set_ease = Some(e);
                    }
                });
            });
            ui.separator();
            if ui.button(egui::RichText::new(crate::i18n::t("cadrer")).size(11.0)).on_hover_text("F").clicked() {
                self.view = None;
            }
        });
        let (rect, resp) = ui.allocate_exact_size(ui.available_size().max(vec2(200.0, 120.0)), Sense::click_and_drag());
        // ---- Canaux, à gauche
        let list = Rect::from_min_max(rect.min, pos2(rect.left() + 128.0, rect.bottom()));
        let plot = Rect::from_min_max(pos2(list.right() + 4.0, rect.top()), rect.max);
        let painter = ui.painter_at(rect);
        painter.rect_filled(list, 3.0, p.bg);
        let mut y = list.top() + 6.0;
        let mut toggle = None;
        for c in curves.iter() {
            let r = Rect::from_min_size(pos2(list.left() + 8.0, y), vec2(list.width() - 12.0, 16.0));
            let on = !self.hidden.contains(&c.label);
            painter.rect_filled(Rect::from_min_size(r.min + vec2(0.0, 3.5), vec2(9.0, 9.0)), 2.0, if on { c.color } else { p.line });
            painter.text(r.min + vec2(15.0, 0.0), egui::Align2::LEFT_TOP, &c.label, egui::FontId::proportional(11.5), if on { p.text } else { p.muted });
            if resp.clicked() && resp.interact_pointer_pos().is_some_and(|q| r.contains(q)) {
                toggle = Some(c.label.clone());
            }
            y += 17.0;
        }
        if let Some(l) = toggle {
            // Maj : ajouter ou retirer ; sinon : n'afficher que celle-là (re-clic : tout)
            let all: Vec<String> = curves.iter().map(|c| c.label.clone()).collect();
            if ui.input(|i| i.modifiers.shift) {
                if let Some(i) = self.hidden.iter().position(|h| *h == l) {
                    self.hidden.remove(i);
                } else {
                    self.hidden.push(l);
                }
            } else if self.hidden.len() == all.len() - 1 && !self.hidden.contains(&l) {
                self.hidden.clear();
            } else {
                self.hidden = all.into_iter().filter(|x| *x != l).collect();
            }
            self.view = None;
        }
        let shown = |c: &Curve, hidden: &Vec<String>| !hidden.contains(&c.label);
        // ---- Vue
        let view = match self.view {
            Some(v) => v,
            None => {
                let v = self.fit(curves, range);
                self.view = Some(v);
                v
            }
        };
        let [t0, t1, v0, v1] = view;
        let x_of = |t: f32| plot.left() + (t - t0) / (t1 - t0) * plot.width();
        let y_of = |v: f32| plot.bottom() - (v - v0) / (v1 - v0) * plot.height();
        let t_of = |x: f32| t0 + (x - plot.left()) / plot.width() * (t1 - t0);
        let v_of = |y: f32| v0 + (plot.bottom() - y) / plot.height() * (v1 - v0);
        let pp = ui.painter_at(plot);
        pp.rect_filled(plot, 3.0, p.bg);
        let nice = |span: f32, px: f32, min_px: f32| {
            let raw = span / (px / min_px).max(1.0);
            let m = 10f32.powf(raw.log10().floor());
            [1.0, 2.0, 5.0, 10.0].into_iter().map(|k| k * m).find(|s| *s >= raw).unwrap_or(10.0 * m)
        };
        // Grille : en images si on connaît la cadence, sinon en secondes
        let unit = if fps > 0.0 { fps } else { 1.0 };
        let ts = nice((t1 - t0) * unit, plot.width(), 60.0).max(1.0) / unit;
        let mut t = (t0 / ts).ceil() * ts;
        while t <= t1 {
            let x = x_of(t);
            pp.line_segment([pos2(x, plot.top()), pos2(x, plot.bottom())], Stroke::new(1.0, p.line.gamma_multiply(0.6)));
            pp.text(pos2(x + 3.0, plot.top() + 2.0), egui::Align2::LEFT_TOP, format!("{}", (t * unit).round()), egui::FontId::monospace(9.0), p.muted);
            t += ts;
        }
        let vs = nice(v1 - v0, plot.height(), 36.0);
        let mut v = (v0 / vs).ceil() * vs;
        while v <= v1 {
            let yy = y_of(v);
            pp.line_segment([pos2(plot.left(), yy), pos2(plot.right(), yy)], Stroke::new(1.0, p.line.gamma_multiply(if v.abs() < vs * 0.01 { 1.0 } else { 0.5 })));
            pp.text(pos2(plot.left() + 3.0, yy - 2.0), egui::Align2::LEFT_BOTTOM, format!("{}", (v * 1000.0).round() / 1000.0), egui::FontId::monospace(9.0), p.muted);
            v += vs;
        }
        for (a, b) in [(t0, range.0), (range.1, t1)] {
            if b > a {
                pp.rect_filled(Rect::from_min_max(pos2(x_of(a), plot.top()), pos2(x_of(b), plot.bottom())), 0.0, Color32::from_black_alpha(18));
            }
        }
        // Courbes, clés, tangentes des clés choisies
        for c in curves.iter().filter(|c| shown(c, &self.hidden)) {
            if c.keys.is_empty() {
                continue;
            }
            let pts: Vec<Pos2> = (0..=(plot.width() as usize / 2)).map(|i| {
                let x = plot.left() + i as f32 * 2.0;
                pos2(x, y_of(eval(&c.keys, t_of(x))))
            }).collect();
            pp.add(Shape::line(pts, Stroke::new(1.8, c.color)));
        }
        let key_pos = |c: &Curve, k: usize| pos2(x_of(c.keys[k].t), y_of(c.keys[k].v));
        let tan_pts = |c: &Curve, k: usize| {
            let tg = tangents(&c.keys, k);
            let (kt, kv) = (c.keys[k].t, c.keys[k].v);
            (key_pos(c, k), pos2(x_of(kt + tg[0]), y_of(kv + tg[1])), pos2(x_of(kt + tg[2]), y_of(kv + tg[3])))
        };
        for &(ci, k) in &self.sel {
            if !shown(&curves[ci], &self.hidden) {
                continue;
            }
            let (kp, l, r) = tan_pts(&curves[ci], k);
            for h in [l, r] {
                pp.line_segment([kp, h], Stroke::new(1.0, p.muted));
                pp.circle_filled(h, 3.5, p.text);
            }
        }
        for (ci, c) in curves.iter().enumerate().filter(|(_, c)| shown(c, &self.hidden)) {
            for k in 0..c.keys.len() {
                let on = self.sel.contains(&(ci, k));
                let q = key_pos(c, k);
                pp.circle_filled(q, if on { 4.5 } else { 3.5 }, if on { p.accent } else { Color32::from_gray(30) });
                pp.circle_stroke(q, if on { 4.5 } else { 3.5 }, Stroke::new(1.0, c.color));
            }
        }
        let px = x_of(now);
        pp.line_segment([pos2(px, plot.top()), pos2(px, plot.bottom())], Stroke::new(2.0, p.accent));

        // ---- Types de tangente et lissages sur les clés choisies
        if set_tan.is_some() || set_ease.is_some() {
            let sel = self.sel.clone();
            for &(ci, k) in &sel {
                let auto = auto_tan(&curves[ci].keys, k);
                let key = &mut curves[ci].keys[k];
                if let Some(e) = set_ease {
                    key.ease = e;
                    key.tan = None;
                }
                match set_tan {
                    Some("auto") => {
                        key.tan = None;
                        key.ease = Ease::Smooth;
                    }
                    Some("spline") => {
                        // Comme auto, mais sans s'aplatir aux sommets : la pente des voisines
                        let ks = curves[ci].keys.clone();
                        let (prev, next) = ((k > 0).then(|| ks[k - 1]), ks.get(k + 1).copied());
                        let slope = match (prev, next) {
                            (Some(a), Some(b)) => (b.v - a.v) / (b.t - a.t).max(1e-6),
                            _ => 0.0,
                        };
                        let key = &mut curves[ci].keys[k];
                        key.tan = Some([auto[0], slope * auto[0], auto[2], slope * auto[2]]);
                    }
                    Some("linear") => {
                        key.tan = None;
                        key.ease = Ease::Linear;
                    }
                    Some("flat") => key.tan = Some([auto[0], 0.0, auto[2], 0.0]),
                    Some("step") => {
                        key.tan = None;
                        key.ease = Ease::Hold;
                    }
                    _ => {}
                }
            }
            // Le lissage est commun à tout le lien (une clé du Motion porte X et Y)
            let first: Vec<(u32, usize, Ease)> = sel.iter().map(|&(ci, k)| (curves[ci].link, k, curves[ci].keys[k].ease)).collect();
            for (link, k, e) in first {
                for c in curves.iter_mut().filter(|c| c.link == link) {
                    if let Some(key) = c.keys.get_mut(k) {
                        key.ease = e;
                    }
                }
            }
            out.changed = true;
        }

        // ---- Souris
        let hidden = self.hidden.clone();
        let near_key = |curves: &[Curve], at: Pos2| -> Option<(usize, usize)> {
            let mut best: Option<((usize, usize), f32)> = None;
            for (ci, c) in curves.iter().enumerate().filter(|(_, c)| shown(c, &hidden)) {
                for k in 0..c.keys.len() {
                    let d = (key_pos(c, k) - at).length();
                    if d < 8.0 && best.is_none_or(|b| d < b.1) {
                        best = Some(((ci, k), d));
                    }
                }
            }
            best.map(|b| b.0)
        };
        let sel_now = self.sel.clone();
        let near_tan = |curves: &[Curve], at: Pos2| -> Option<(usize, usize, bool)> {
            for &(ci, k) in &sel_now {
                let (_, l, r) = tan_pts(&curves[ci], k);
                if (r - at).length() < 7.0 {
                    return Some((ci, k, true));
                }
                if (l - at).length() < 7.0 {
                    return Some((ci, k, false));
                }
            }
            None
        };
        let shift = ui.input(|i| i.modifiers.shift);
        if resp.drag_started() {
            let at = ui.input(|i| i.pointer.press_origin()).unwrap_or(plot.center());
            if !plot.contains(at) {
                self.drag = None;
            } else if resp.dragged_by(egui::PointerButton::Middle) || ui.input(|i| i.modifiers.alt) {
                self.drag = Some(Drag::Pan { last: at });
            } else if at.y < plot.top() + 16.0 {
                self.drag = Some(Drag::Scrub);
            } else if let Some((c, k, right)) = near_tan(curves, at) {
                self.drag = Some(Drag::Tan { c, k, right });
            } else if let Some(hit) = near_key(curves, at) {
                if !self.sel.contains(&hit) {
                    if !shift {
                        self.sel.clear();
                    }
                    self.sel.push(hit);
                }
                self.drag = Some(Drag::Keys { from: at, orig: curves.to_vec() });
            } else {
                if !shift {
                    self.sel.clear();
                }
                self.drag = Some(Drag::Box { from: at });
            }
        }
        let cur = resp.interact_pointer_pos().or(ui.input(|i| i.pointer.hover_pos()));
        match (self.drag.clone(), cur) {
            (Some(Drag::Pan { last }), Some(at)) if resp.dragged() => {
                let d = at - last;
                let (dt, dv) = (-d.x / plot.width() * (t1 - t0), d.y / plot.height() * (v1 - v0));
                self.view = Some([t0 + dt, t1 + dt, v0 + dv, v1 + dv]);
                self.drag = Some(Drag::Pan { last: at });
            }
            (Some(Drag::Scrub), Some(at)) if resp.dragged() => out.seek = Some(snap(t_of(at.x))),
            (Some(Drag::Keys { from, orig }), Some(at)) if resp.dragged() => {
                let dt = snap((at.x - from.x) / plot.width() * (t1 - t0));
                let dv = -(at.y - from.y) / plot.height() * (v1 - v0);
                let time_only = ui.input(|i| i.modifiers.command);
                let mut next = orig.clone();
                // Le temps bouge pour tout le lien ; la valeur seulement pour la courbe choisie
                let mut moved: Vec<(u32, usize)> = self.sel.iter().map(|&(ci, k)| (orig[ci].link, k)).collect();
                moved.sort();
                moved.dedup();
                for c in next.iter_mut() {
                    for &(link, k) in &moved {
                        if c.link == link {
                            if let Some(key) = c.keys.get_mut(k) {
                                key.t = (key.t + dt).max(0.0);
                            }
                        }
                    }
                }
                if !time_only {
                    for &(ci, k) in &self.sel {
                        next[ci].keys[k].v = orig[ci].keys[k].v + dv;
                    }
                }
                // Les clés restent dans l'ordre, à des instants différents
                if next.iter().all(|c| c.keys.windows(2).all(|w| w[0].t < w[1].t - 1e-4)) {
                    curves.clone_from_slice(&next);
                    out.changed = true;
                }
            }
            (Some(Drag::Tan { c, k, right }), Some(at)) if resp.dragged() => {
                let key = curves[c].keys[k];
                let mut tg = tangents(&curves[c].keys, k);
                let h = [t_of(at.x) - key.t, v_of(at.y) - key.v];
                // Les deux côtés restent alignés (une tangente lisse), sauf avec Maj (cassée)
                let broken = ui.input(|i| i.modifiers.shift);
                if right {
                    tg[2] = h[0].max(0.005);
                    tg[3] = h[1];
                    if !broken {
                        let s = tg[3] / tg[2];
                        tg[1] = s * tg[0];
                    }
                } else {
                    tg[0] = h[0].min(-0.005);
                    tg[1] = h[1];
                    if !broken {
                        let s = tg[1] / tg[0];
                        tg[3] = s * tg[2];
                    }
                }
                curves[c].keys[k].tan = Some(tg);
                out.changed = true;
            }
            (Some(Drag::Box { from }), Some(at)) if resp.dragged() => {
                let b = Rect::from_two_pos(from, at);
                pp.rect_filled(b, 0.0, p.accent.gamma_multiply(0.1));
                pp.rect_stroke(b, 0.0, Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
            }
            _ => {}
        }
        if resp.drag_stopped() {
            if let (Some(Drag::Box { from }), Some(at)) = (self.drag.take(), cur) {
                let b = Rect::from_two_pos(from, at);
                for (ci, c) in curves.iter().enumerate().filter(|(_, c)| shown(c, &self.hidden)) {
                    for k in 0..c.keys.len() {
                        if b.contains(key_pos(c, k)) && !self.sel.contains(&(ci, k)) {
                            self.sel.push((ci, k));
                        }
                    }
                }
            }
            self.drag = None;
        }
        if resp.clicked() {
            if let Some(at) = resp.interact_pointer_pos().filter(|q| plot.contains(*q)) {
                if at.y < plot.top() + 16.0 {
                    out.seek = Some(snap(t_of(at.x)));
                } else {
                    match near_key(curves, at) {
                        Some(hit) if shift => {
                            if let Some(i) = self.sel.iter().position(|s| *s == hit) {
                                self.sel.remove(i);
                            } else {
                                self.sel.push(hit);
                            }
                        }
                        Some(hit) => self.sel = vec![hit],
                        None if !shift => self.sel.clear(),
                        None => {}
                    }
                }
            }
        }
        // Molette : zoom dans le temps ; Ctrl : dans les valeurs
        if resp.hovered() {
            let (scroll, ctrl, at) = ui.input(|i| (i.smooth_scroll_delta.y + i.smooth_scroll_delta.x, i.modifiers.command, i.pointer.hover_pos()));
            if let (true, Some(at)) = (scroll != 0.0, at.filter(|q| plot.contains(*q))) {
                let k = (-scroll * 0.002).exp();
                let [mut a, mut b, mut c, mut d] = self.view.unwrap_or(view);
                if ctrl {
                    let v = v_of(at.y);
                    (c, d) = (v + (c - v) * k, v + (d - v) * k);
                } else {
                    let t = t_of(at.x);
                    (a, b) = (t + (a - t) * k, t + (b - t) * k);
                }
                self.view = Some([a, b, c, d]);
            }
        }
        // Clavier : A tout choisir, F cadrer, Suppr retirer (sur tout le lien)
        if resp.hovered() && !ui.ctx().egui_wants_keyboard_input() {
            let (all, fit, del) = ui.input(|i| (!i.modifiers.any() && i.key_pressed(egui::Key::A), i.key_pressed(egui::Key::F) || i.key_pressed(egui::Key::Home), i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)));
            if all {
                self.sel = curves.iter().enumerate().filter(|(_, c)| shown(c, &self.hidden)).flat_map(|(ci, c)| (0..c.keys.len()).map(move |k| (ci, k))).collect();
            }
            if fit {
                self.view = None;
            }
            if del && !self.sel.is_empty() {
                let mut gone: Vec<(u32, usize)> = self.sel.iter().map(|&(ci, k)| (curves[ci].link, k)).collect();
                gone.sort();
                gone.dedup();
                for c in curves.iter_mut() {
                    let mut i = 0;
                    let link = c.link;
                    c.keys.retain(|_| {
                        let keep = !gone.contains(&(link, i));
                        i += 1;
                        keep
                    });
                }
                self.sel.clear();
                out.changed = true;
            }
        }
        if curves.iter().all(|c| c.keys.is_empty()) {
            pp.text(plot.center(), egui::Align2::CENTER_CENTER, crate::i18n::t("pas encore de clé"), egui::FontId::proportional(12.0), p.muted);
        }
        if self.drag.is_some() {
            ui.ctx().request_repaint();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn courbes() {
        let k = |t: f32, v: f32| CK { t, v, ease: Ease::Linear, tan: None };
        let keys = vec![k(0.0, 0.0), k(1.0, 10.0)];
        assert!((eval(&keys, 0.5) - 5.0).abs() < 1e-4);
        // Tangentes plates : départ et arrivée doux, milieu au milieu
        let mut flat = keys.clone();
        flat[0].tan = Some([-0.3, 0.0, 0.3, 0.0]);
        flat[1].tan = Some([-0.3, 0.0, 0.3, 0.0]);
        assert!(eval(&flat, 0.1) < 1.0);
        assert!((eval(&flat, 0.5) - 5.0).abs() < 0.1);
        // Sommet : la tangente auto est plate
        let peak = vec![k(0.0, 0.0), k(1.0, 10.0), k(2.0, 0.0)];
        assert_eq!(auto_tan(&peak, 1)[1], 0.0);
    }
}
