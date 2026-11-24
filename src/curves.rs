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
    /// Normaliser : chaque courbe affichée entre -1 et 1 (comme Maya), pour comparer des grandeurs différentes
    pub normalize: bool,
    /// Courbes tampon (comme Maya) : une copie pour comparer, qu'on peut échanger avec les courbes
    buffer: Option<(String, Vec<Curve>)>,
    /// Tweener sur les clés choisies (0 : clé d'avant, 1 : clé d'après)
    tween: f32,
}

impl Default for Editor {
    fn default() -> Self {
        Editor { view: None, sel: vec![], hidden: vec![], drag: None, who: String::new(), normalize: false, buffer: None, tween: 0.5 }
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
        if self.normalize {
            for c in curves.iter().filter(|c| !self.hidden.contains(&c.label)) {
                for k in &c.keys {
                    (t0, t1) = (t0.min(k.t), t1.max(k.t));
                }
            }
            return [t0 - 0.05, t1 + 0.05, -1.25, 1.25];
        }
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
        // ---- Barre d'icônes : tangentes, clés, outils façon Animbot, courbes tampon, vue
        let mut set_tan: Option<&str> = None;
        let mut set_ease: Option<Ease> = None;
        let mut act: Option<&str> = None;
        let any = !self.sel.is_empty();
        let mut tween_changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.spacing_mut().button_padding = vec2(4.0, 2.0);
            let icon = |ui: &mut egui::Ui, on: bool, i: &str, tip: &str| ui.add_enabled(on, egui::Button::new(egui::RichText::new(i).size(14.0)).frame(false).min_size(vec2(24.0, 22.0))).on_hover_text(crate::i18n::t(tip)).clicked();
            // Tangentes : un petit dessin de la courbe, comme les icônes de Maya
            for (id, tip) in [("auto", "Tangentes auto (plates aux sommets)"), ("spline", "Tangentes spline (lisses)"), ("linear", "Tangentes linéaires"), ("flat", "Tangentes plates"), ("step", "Tangentes en escalier")] {
                let (r, resp) = ui.allocate_exact_size(vec2(24.0, 22.0), if any { Sense::click() } else { Sense::hover() });
                let col = if !any { p.muted.gamma_multiply(0.5) } else if resp.hovered() { p.text } else { p.muted };
                if resp.hovered() && any {
                    ui.painter().rect_filled(r, 3.0, p.raised);
                }
                let g = r.shrink2(vec2(5.0, 5.0));
                let pt = |x: f32, y: f32| pos2(g.left() + g.width() * x, g.bottom() - g.height() * y);
                let pts: Vec<Pos2> = match id {
                    "linear" => vec![pt(0.0, 0.0), pt(1.0, 1.0)],
                    "step" => vec![pt(0.0, 0.0), pt(0.6, 0.0), pt(0.6, 1.0), pt(1.0, 1.0)],
                    "flat" => (0..=12).map(|k| { let x = k as f32 / 12.0; pt(x, x * x * (3.0 - 2.0 * x)) }).collect(),
                    "spline" => (0..=12).map(|k| { let x = k as f32 / 12.0; pt(x, 0.5 - 0.5 * (x * std::f32::consts::PI).cos()) }).collect(),
                    _ => (0..=12).map(|k| { let x = k as f32 / 12.0; pt(x, (x * 1.6).min(1.0).powf(0.8) * if x > 0.62 { 1.0 } else { 0.97 }) }).collect(),
                };
                ui.painter().add(Shape::line(pts, Stroke::new(1.5, col)));
                if resp.on_hover_text(crate::i18n::t(tip)).clicked() {
                    set_tan = Some(id);
                }
            }
            if icon(ui, any, crate::lucide::LINK, "Unifier les tangentes (les deux côtés alignés)") {
                act = Some("unify");
            }
            ui.add_enabled_ui(any, |ui| {
                ui.menu_button(egui::RichText::new(crate::lucide::CHART_SPLINE).size(14.0), |ui| {
                    if let Some(Some(e)) = crate::keys::menu(ui, None, false) {
                        set_ease = Some(e);
                    }
                })
                .response
                .on_hover_text(crate::i18n::t("Lissages de l'app (doux, rebond, ressort…)"));
            });
            ui.separator();
            // Clés : poser, caler sur les images, supprimer
            if icon(ui, true, crate::lucide::DIAMOND_PLUS, "Poser une clé à la tête de lecture sur les courbes affichées (I)") {
                act = Some("insert");
            }
            if icon(ui, any, crate::lucide::MAGNET, "Caler les clés choisies sur des images entières") {
                act = Some("snap");
            }
            if icon(ui, any, crate::lucide::ERASER, "Supprimer les clés choisies (Suppr)") {
                act = Some("delete");
            }
            ui.separator();
            // Façon Animbot : tweener, décaler, adoucir, alléger, filtre d'Euler
            ui.spacing_mut().slider_width = 70.0;
            let r = ui.add_enabled(any, egui::Slider::new(&mut self.tween, -0.5..=1.5).show_value(false)).on_hover_text(format!("{} · {:.0} %", crate::i18n::t("Tweener : les clés choisies entre leur voisine d'avant (0 %) et d'après (100 %)"), self.tween * 100.0));
            tween_changed = r.changed();
            if icon(ui, any, crate::lucide::CHEVRON_LEFT, "Avancer les clés choisies d'une image") {
                act = Some("nudge-");
            }
            if icon(ui, any, crate::lucide::CHEVRON_RIGHT, "Reculer les clés choisies d'une image") {
                act = Some("nudge+");
            }
            if icon(ui, any, crate::lucide::WAVES, "Adoucir les clés choisies (moyenne avec les voisines)") {
                act = Some("smooth");
            }
            if icon(ui, true, crate::lucide::MINIMIZE_2, "Alléger : retirer les clés que les voisines redonnent déjà") {
                act = Some("simplify");
            }
            if icon(ui, true, crate::lucide::ROTATE_CCW, "Filtre d'Euler : rotations sans sauts de 360°") {
                act = Some("euler");
            }
            ui.separator();
            // Courbes tampon (Maya) : garder une copie, l'échanger
            if icon(ui, true, crate::lucide::HISTORY, "Courbes tampon : garder une copie des courbes pour comparer") {
                act = Some("snapshot");
            }
            if icon(ui, self.buffer.as_ref().is_some_and(|b| b.0 == self.who), crate::lucide::REPEAT, "Échanger avec les courbes tampon") {
                act = Some("swap");
            }
            ui.separator();
            if icon(ui, true, crate::lucide::SCAN, "Cadrer (F)") {
                self.view = None;
            }
            let r = ui.add(egui::Button::new(egui::RichText::new(crate::lucide::SIGMA).size(14.0)).frame(false).selected(self.normalize).min_size(vec2(24.0, 22.0))).on_hover_text(crate::i18n::t("Normaliser : chaque courbe entre -1 et 1"));
            if r.clicked() {
                self.normalize = !self.normalize;
                self.view = None;
            }
            // Valeurs exactes de la clé choisie (comme les champs de Maya)
            if let [(ci, k)] = self.sel[..] {
                ui.separator();
                let key = curves[ci].keys[k];
                let mut f = key.t * fps.max(1.0);
                let mut v = key.v;
                let rf = ui.add(egui::DragValue::new(&mut f).speed(0.2).max_decimals(1).prefix(format!("{} ", crate::lucide::CLOCK))).on_hover_text(crate::i18n::t("Image de la clé"));
                let rv = ui.add(egui::DragValue::new(&mut v).speed(0.05).max_decimals(3)).on_hover_text(crate::i18n::t("Valeur de la clé"));
                if rv.changed() {
                    curves[ci].keys[k].v = v;
                    out.changed = true;
                }
                if rf.changed() {
                    let nt = (f / fps.max(1.0)).max(0.0);
                    let link = curves[ci].link;
                    let ok = curves.iter().filter(|c| c.link == link).all(|c| c.keys.iter().enumerate().all(|(j, x)| j == k || (x.t - nt).abs() > 1e-4) && (k == 0 || c.keys[k - 1].t < nt) && c.keys.get(k + 1).is_none_or(|n| n.t > nt));
                    if ok {
                        for c in curves.iter_mut().filter(|c| c.link == link) {
                            c.keys[k].t = nt;
                        }
                        out.changed = true;
                    }
                }
            }
        });
        // Les actions de la barre
        let step = 1.0 / if fps > 0.0 { fps } else { 24.0 };
        if tween_changed {
            for &(ci, k) in &self.sel {
                let ks = &curves[ci].keys;
                if k > 0 && k + 1 < ks.len() {
                    let (a, b) = (ks[k - 1].v, ks[k + 1].v);
                    curves[ci].keys[k].v = a + (b - a) * self.tween;
                    out.changed = true;
                }
            }
        }
        // Les clés choisies regroupées par lien (leur instant est commun)
        let mut linked: Vec<(u32, usize)> = self.sel.iter().map(|&(ci, k)| (curves[ci].link, k)).collect();
        linked.sort();
        linked.dedup();
        match act {
            Some("unify") => {
                for &(ci, k) in &self.sel {
                    let tg = tangents(&curves[ci].keys, k);
                    let s = (tg[1] / tg[0].min(-1e-4) + tg[3] / tg[2].max(1e-4)) / 2.0;
                    curves[ci].keys[k].tan = Some([tg[0], s * tg[0], tg[2], s * tg[2]]);
                }
                out.changed = true;
            }
            Some("insert") => {
                let links: Vec<u32> = curves.iter().filter(|c| !self.hidden.contains(&c.label)).map(|c| c.link).collect();
                for c in curves.iter_mut().filter(|c| links.contains(&c.link)) {
                    if c.keys.iter().any(|k| (k.t - now).abs() < 1e-4) {
                        continue;
                    }
                    let v = eval(&c.keys, now);
                    let at = c.keys.iter().position(|k| k.t > now).unwrap_or(c.keys.len());
                    c.keys.insert(at, CK { t: now, v, ease: Ease::Smooth, tan: None });
                }
                self.sel.clear();
                out.changed = true;
            }
            Some("snap") | Some("nudge-") | Some("nudge+") => {
                let before: Vec<Curve> = curves.to_vec();
                for &(link, k) in &linked {
                    for c in curves.iter_mut().filter(|c| c.link == link) {
                        if let Some(key) = c.keys.get_mut(k) {
                            key.t = match act {
                                Some("snap") => (key.t / step).round() * step,
                                Some("nudge-") => (key.t - step).max(0.0),
                                _ => key.t + step,
                            };
                        }
                    }
                }
                // Pas de clés qui se croisent ou se superposent : sinon on revient en arrière
                if curves.iter().all(|c| c.keys.windows(2).all(|w| w[0].t < w[1].t - 1e-4)) {
                    out.changed = true;
                } else {
                    curves.clone_from_slice(&before);
                }
            }
            Some("delete") => {
                for c in curves.iter_mut() {
                    let link = c.link;
                    let mut i = 0;
                    c.keys.retain(|_| {
                        let keep = !linked.contains(&(link, i));
                        i += 1;
                        keep
                    });
                }
                self.sel.clear();
                out.changed = true;
            }
            Some("smooth") => {
                let orig: Vec<Curve> = curves.to_vec();
                for &(ci, k) in &self.sel {
                    let ks = &orig[ci].keys;
                    if k > 0 && k + 1 < ks.len() {
                        curves[ci].keys[k].v = (ks[k - 1].v + 2.0 * ks[k].v + ks[k + 1].v) / 4.0;
                    }
                }
                out.changed = true;
            }
            Some("simplify") => {
                // Une clé part seulement si toutes les courbes de son lien peuvent s'en passer
                let mut links: Vec<u32> = curves.iter().map(|c| c.link).collect();
                links.sort();
                links.dedup();
                for link in links {
                    let mut i = 1;
                    loop {
                        let idx: Vec<usize> = (0..curves.len()).filter(|&c| curves[c].link == link).collect();
                        let n = idx.first().map(|&c| curves[c].keys.len()).unwrap_or(0);
                        if i + 1 >= n {
                            break;
                        }
                        let removable = idx.iter().all(|&c| {
                            let ks = &curves[c].keys;
                            let span = ks.iter().fold((f32::MAX, f32::MIN), |a, k| (a.0.min(k.v), a.1.max(k.v)));
                            let tol = ((span.1 - span.0) * 0.005).max(1e-4);
                            let mut without = ks.clone();
                            let k = without.remove(i);
                            (eval(&without, k.t) - k.v).abs() <= tol
                        });
                        if removable {
                            for &c in &idx {
                                curves[c].keys.remove(i);
                            }
                            out.changed = true;
                        } else {
                            i += 1;
                        }
                    }
                }
                self.sel.clear();
            }
            Some("euler") => {
                for c in curves.iter_mut().filter(|c| c.label.to_lowercase().contains("rotation")) {
                    for k in 1..c.keys.len() {
                        let prev = c.keys[k - 1].v;
                        while c.keys[k].v - prev > 180.0 {
                            c.keys[k].v -= 360.0;
                            out.changed = true;
                        }
                        while c.keys[k].v - prev < -180.0 {
                            c.keys[k].v += 360.0;
                            out.changed = true;
                        }
                    }
                }
            }
            Some("snapshot") => self.buffer = Some((self.who.clone(), curves.to_vec())),
            Some("swap") => {
                if let Some((who, b)) = self.buffer.take() {
                    if b.len() == curves.len() {
                        let now_curves = curves.to_vec();
                        curves.clone_from_slice(&b);
                        self.buffer = Some((who, now_curves));
                        self.sel.clear();
                        out.changed = true;
                    } else {
                        self.buffer = Some((who, b));
                    }
                }
            }
            _ => {}
        }
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
        let y_raw = |v: f32| plot.bottom() - (v - v0) / (v1 - v0) * plot.height();
        // Normaliser : chaque courbe ramenée entre -1 et 1 (centre et demi-étendue par courbe)
        let norm: Vec<(f32, f32)> = curves
            .iter()
            .map(|c| {
                if !self.normalize || c.keys.is_empty() {
                    return (0.0, 1.0);
                }
                let (lo, hi) = c.keys.iter().fold((f32::MAX, f32::MIN), |a, k| (a.0.min(k.v), a.1.max(k.v)));
                ((lo + hi) / 2.0, ((hi - lo) / 2.0).max(1e-3))
            })
            .collect();
        let y_of_c = |ci: usize, v: f32| y_raw((v - norm[ci].0) / norm[ci].1);
        let y_of = y_raw;
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
        // Courbes tampon : en gris, sous les courbes
        if let Some((who, buf)) = &self.buffer {
            if *who == self.who {
                for (ci, c) in buf.iter().enumerate().filter(|(_, c)| shown(c, &self.hidden)) {
                    if c.keys.is_empty() || ci >= norm.len() {
                        continue;
                    }
                    let pts: Vec<Pos2> = (0..=(plot.width() as usize / 3)).map(|i| {
                        let x = plot.left() + i as f32 * 3.0;
                        pos2(x, y_of_c(ci, eval(&c.keys, t_of(x))))
                    }).collect();
                    pp.extend(Shape::dashed_line(&pts, Stroke::new(1.2, p.muted.gamma_multiply(0.7)), 5.0, 4.0));
                }
            }
        }
        for (ci, c) in curves.iter().enumerate().filter(|(_, c)| shown(c, &self.hidden)) {
            if c.keys.is_empty() {
                continue;
            }
            let pts: Vec<Pos2> = (0..=(plot.width() as usize / 2)).map(|i| {
                let x = plot.left() + i as f32 * 2.0;
                pos2(x, y_of_c(ci, eval(&c.keys, t_of(x))))
            }).collect();
            pp.add(Shape::line(pts, Stroke::new(1.8, c.color)));
        }
        let key_pos = |ci: usize, c: &Curve, k: usize| pos2(x_of(c.keys[k].t), y_of_c(ci, c.keys[k].v));
        let tan_pts = |ci: usize, c: &Curve, k: usize| {
            let tg = tangents(&c.keys, k);
            let (kt, kv) = (c.keys[k].t, c.keys[k].v);
            (key_pos(ci, c, k), pos2(x_of(kt + tg[0]), y_of_c(ci, kv + tg[1])), pos2(x_of(kt + tg[2]), y_of_c(ci, kv + tg[3])))
        };
        for &(ci, k) in &self.sel {
            if !shown(&curves[ci], &self.hidden) {
                continue;
            }
            let (kp, l, r) = tan_pts(ci, &curves[ci], k);
            for h in [l, r] {
                pp.line_segment([kp, h], Stroke::new(1.0, p.muted));
                pp.circle_filled(h, 3.5, p.text);
            }
        }
        for (ci, c) in curves.iter().enumerate().filter(|(_, c)| shown(c, &self.hidden)) {
            for k in 0..c.keys.len() {
                let on = self.sel.contains(&(ci, k));
                let q = key_pos(ci, c, k);
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
                    let d = (key_pos(ci, c, k) - at).length();
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
                let (_, l, r) = tan_pts(ci, &curves[ci], k);
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
        let mut return_after_scale = false;
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
                // S enfoncé : mettre à l'échelle autour du centre de la sélection (temps et valeur), comme Maya
                if ui.input(|i| i.key_down(egui::Key::S)) {
                    let n = self.sel.len().max(1) as f32;
                    let pt = self.sel.iter().map(|&(ci, k)| orig[ci].keys[k].t).sum::<f32>() / n;
                    let kt = (1.0 + (at.x - from.x) / 200.0).max(0.05);
                    let kv = 1.0 - (at.y - from.y) / 200.0;
                    let mut moved: Vec<(u32, usize)> = self.sel.iter().map(|&(ci, k)| (orig[ci].link, k)).collect();
                    moved.sort();
                    moved.dedup();
                    for c in next.iter_mut() {
                        for &(link, k) in &moved {
                            if c.link == link {
                                if let Some(key) = c.keys.get_mut(k) {
                                    key.t = snap(pt + (key.t - pt) * kt).max(0.0);
                                }
                            }
                        }
                    }
                    for &(ci, k) in &self.sel {
                        let sel_in: Vec<f32> = self.sel.iter().filter(|s| s.0 == ci).map(|&(_, kk)| orig[ci].keys[kk].v).collect();
                        let pv = sel_in.iter().sum::<f32>() / sel_in.len().max(1) as f32;
                        next[ci].keys[k].v = pv + (orig[ci].keys[k].v - pv) * kv;
                    }
                    if next.iter().all(|c| c.keys.windows(2).all(|w| w[0].t < w[1].t - 1e-4)) {
                        curves.clone_from_slice(&next);
                        out.changed = true;
                    }
                    return_after_scale = true;
                }
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
                        next[ci].keys[k].v = orig[ci].keys[k].v + dv * norm[ci].1;
                    }
                }
                // Les clés restent dans l'ordre, à des instants différents
                if !return_after_scale && next.iter().all(|c| c.keys.windows(2).all(|w| w[0].t < w[1].t - 1e-4)) {
                    curves.clone_from_slice(&next);
                    out.changed = true;
                }
            }
            (Some(Drag::Tan { c, k, right }), Some(at)) if resp.dragged() => {
                let key = curves[c].keys[k];
                let mut tg = tangents(&curves[c].keys, k);
                let h = [t_of(at.x) - key.t, v_of(at.y) * norm[c].1 + norm[c].0 - key.v];
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
                        if b.contains(key_pos(ci, c, k)) && !self.sel.contains(&(ci, k)) {
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
            if ui.input(|i| !i.modifiers.any() && i.key_pressed(egui::Key::I)) {
                let links: Vec<u32> = curves.iter().filter(|c| !self.hidden.contains(&c.label)).map(|c| c.link).collect();
                for c in curves.iter_mut().filter(|c| links.contains(&c.link)) {
                    if c.keys.iter().any(|k| (k.t - now).abs() < 1e-4) {
                        continue;
                    }
                    let v = eval(&c.keys, now);
                    let at = c.keys.iter().position(|k| k.t > now).unwrap_or(c.keys.len());
                    c.keys.insert(at, CK { t: now, v, ease: Ease::Smooth, tan: None });
                }
                self.sel.clear();
                out.changed = true;
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
