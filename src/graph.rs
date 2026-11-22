//! Éditeur de courbes d'animation, à la manière du Graph Editor de Blender : une courbe par canal
//! (position, rotation, taille ; X Y Z), clés et poignées de Bézier à attraper, passage Bézier,
//! linéaire ou constant, poignées automatiques, alignées, libres ou vecteur.

use crate::scene::{channel_at, handles_of, Key, CHANNELS};
use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, Sense, Shape, Stroke};

const AXIS: [Color32; 3] = [Color32::from_rgb(237, 72, 88), Color32::from_rgb(120, 196, 52), Color32::from_rgb(64, 138, 245)];

#[derive(Clone)]
enum Drag {
    /// Clés choisies : point de départ et clés d'origine
    Keys { from: Pos2, orig: Vec<Key> },
    /// Poignée : clé, canal, côté droit ?
    Handle { key: usize, ch: usize, right: bool },
    Box { from: Pos2 },
    Pan { last: Pos2 },
    Scrub,
}

pub struct Graph {
    /// Canaux affichés
    pub on: [bool; 9],
    /// Vue : images de f0 à f1, valeurs de v0 à v1
    view: Option<[f32; 4]>,
    /// Clés choisies : (clé, canal)
    sel: Vec<(usize, usize)>,
    drag: Option<Drag>,
    /// Objet dont on montre les courbes : quand il change, la vue se recadre
    pub who: Option<usize>,
}

impl Default for Graph {
    fn default() -> Self {
        Graph { on: [true, true, true, false, false, false, false, false, false], view: None, sel: vec![], drag: None, who: None }
    }
}

pub struct Out {
    pub changed: bool,
    pub seek: Option<f32>,
}

impl Graph {
    pub fn reset_view(&mut self) {
        self.view = None;
        self.sel.clear();
    }

    fn fit(&self, keys: &[Key], range: (i32, i32)) -> [f32; 4] {
        let (mut v0, mut v1) = (f32::MAX, f32::MIN);
        for c in (0..9).filter(|&c| self.on[c]) {
            for k in keys {
                v0 = v0.min(k.get(c));
                v1 = v1.max(k.get(c));
            }
            // Les bosses des courbes entre les clés comptent aussi
            if keys.len() > 1 {
                let (a, b) = (keys[0].frame as f32, keys[keys.len() - 1].frame as f32);
                for i in 0..=64 {
                    let v = channel_at(keys, c, a + (b - a) * i as f32 / 64.0);
                    v0 = v0.min(v);
                    v1 = v1.max(v);
                }
            }
        }
        if v0 > v1 {
            (v0, v1) = (-1.0, 1.0);
        }
        let pad = ((v1 - v0) * 0.15).max(0.5);
        let (f0, f1) = match (keys.first(), keys.last()) {
            (Some(a), Some(b)) => (a.frame.min(range.0) as f32, b.frame.max(range.1) as f32),
            _ => (range.0 as f32, range.1 as f32),
        };
        let fp = ((f1 - f0) * 0.04).max(2.0);
        [f0 - fp, f1 + fp, v0 - pad, v1 + pad]
    }

    /// Dessine l'éditeur et gère la souris et le clavier. `keys` est modifié sur place.
    pub fn ui(&mut self, ui: &mut egui::Ui, keys: &mut Vec<Key>, frame: f32, range: (i32, i32)) -> Out {
        let p = theme::pal();
        let mut out = Out { changed: false, seek: None };
        self.sel.retain(|(k, _)| *k < keys.len());
        // ---- Barre : passage, poignées, cadrer
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.label(egui::RichText::new(crate::i18n::t("passage")).size(10.5).color(p.muted));
            for (m, l, tip) in [("bezier", "∿", "Bézier : courbe douce, réglable par les poignées"), ("linear", "╱", "linéaire : vitesse constante"), ("constant", "⌐", "constant : saute d'une clé à l'autre")] {
                if ui.add_enabled(!self.sel.is_empty(), egui::Button::new(egui::RichText::new(l).size(13.0))).on_hover_text(crate::i18n::t(tip)).clicked() {
                    for &(k, _) in &self.sel {
                        keys[k].interp = m.to_string();
                    }
                    out.changed = true;
                }
            }
            ui.separator();
            ui.label(egui::RichText::new(crate::i18n::t("poignées")).size(10.5).color(p.muted));
            for (h, l) in [(0u8, "auto"), (1, "alignées"), (2, "libres"), (3, "vecteur")] {
                if ui.add_enabled(!self.sel.is_empty(), egui::Button::new(egui::RichText::new(crate::i18n::t(l)).size(11.0))).clicked() {
                    for &(k, _) in &self.sel {
                        set_mode(keys, k, h);
                    }
                    out.changed = true;
                }
            }
            ui.separator();
            if ui.button(crate::i18n::t("cadrer")).on_hover_text("Origine").clicked() {
                self.view = None;
            }
        });
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        // ---- Colonne des canaux
        let list = Rect::from_min_max(rect.min, pos2(rect.left() + 118.0, rect.bottom()));
        let plot = Rect::from_min_max(pos2(list.right() + 4.0, rect.top()), rect.max);
        let painter = ui.painter_at(rect);
        painter.rect_filled(list, 3.0, p.bg);
        let mut y = list.top() + 6.0;
        let mut toggled = None;
        for (c, (group, axis)) in CHANNELS.iter().enumerate() {
            if c % 3 == 0 {
                painter.text(pos2(list.left() + 8.0, y), egui::Align2::LEFT_TOP, crate::i18n::t(group), egui::FontId::monospace(10.0), p.muted);
                y += 14.0;
            }
            let r = Rect::from_min_size(pos2(list.left() + 14.0, y), vec2(list.width() - 20.0, 15.0));
            let col = AXIS[c % 3];
            painter.rect_filled(Rect::from_min_size(r.min + vec2(0.0, 3.0), vec2(9.0, 9.0)), 2.0, if self.on[c] { col } else { p.line });
            painter.text(r.min + vec2(16.0, 0.0), egui::Align2::LEFT_TOP, *axis, egui::FontId::monospace(11.0), if self.on[c] { p.text } else { p.muted });
            if resp.clicked() && resp.interact_pointer_pos().is_some_and(|q| r.contains(q)) {
                toggled = Some(c);
            }
            y += 16.0;
        }
        if let Some(c) = toggled {
            // Maj : ajouter ; sinon : seulement ce groupe de trois si on clique sur un canal caché
            if ui.input(|i| i.modifiers.shift) || self.on[c] {
                self.on[c] = !self.on[c];
            } else {
                self.on = [false; 9];
                self.on[c] = true;
            }
            self.view = None;
        }
        // ---- Vue
        let view = match self.view {
            Some(v) => v,
            None => {
                let v = self.fit(keys, range);
                self.view = Some(v);
                v
            }
        };
        let [f0, f1, v0, v1] = view;
        let x_of = |f: f32| plot.left() + (f - f0) / (f1 - f0) * plot.width();
        let y_of = |v: f32| plot.bottom() - (v - v0) / (v1 - v0) * plot.height();
        let f_of = |x: f32| f0 + (x - plot.left()) / plot.width() * (f1 - f0);
        let v_of = |y: f32| v0 + (plot.bottom() - y) / plot.height() * (v1 - v0);
        let pp = ui.painter_at(plot);
        pp.rect_filled(plot, 3.0, p.bg);
        // Grille : images en colonnes, valeurs en lignes
        let nice = |span: f32, px: f32, min_px: f32| {
            let raw = span / (px / min_px).max(1.0);
            let m = 10f32.powf(raw.log10().floor());
            [1.0, 2.0, 5.0, 10.0].into_iter().map(|k| k * m).find(|s| *s >= raw).unwrap_or(10.0 * m)
        };
        let fs = nice(f1 - f0, plot.width(), 60.0).max(1.0);
        let mut f = (f0 / fs).ceil() * fs;
        while f <= f1 {
            let x = x_of(f);
            pp.line_segment([pos2(x, plot.top()), pos2(x, plot.bottom())], Stroke::new(1.0, p.line.gamma_multiply(0.6)));
            pp.text(pos2(x + 3.0, plot.top() + 2.0), egui::Align2::LEFT_TOP, format!("{f}"), egui::FontId::monospace(9.0), p.muted);
            f += fs;
        }
        let vs = nice(v1 - v0, plot.height(), 36.0);
        let mut v = (v0 / vs).ceil() * vs;
        while v <= v1 {
            let yy = y_of(v);
            pp.line_segment([pos2(plot.left(), yy), pos2(plot.right(), yy)], Stroke::new(if v.abs() < vs * 0.01 { 1.4 } else { 1.0 }, p.line.gamma_multiply(if v.abs() < vs * 0.01 { 1.0 } else { 0.5 })));
            pp.text(pos2(plot.left() + 3.0, yy - 2.0), egui::Align2::LEFT_BOTTOM, format!("{}", (v * 1000.0).round() / 1000.0), egui::FontId::monospace(9.0), p.muted);
            v += vs;
        }
        // Plage de la scène : l'extérieur est grisé
        for (a, b) in [(f0, range.0 as f32), (range.1 as f32, f1)] {
            if b > a {
                pp.rect_filled(Rect::from_min_max(pos2(x_of(a), plot.top()), pos2(x_of(b), plot.bottom())), 0.0, Color32::from_black_alpha(18));
            }
        }
        // Courbes
        for c in (0..9).filter(|&c| self.on[c]) {
            if keys.is_empty() {
                continue;
            }
            let col = AXIS[c % 3];
            let pts: Vec<Pos2> = (0..=(plot.width() as usize / 2)).map(|i| {
                let x = plot.left() + i as f32 * 2.0;
                pos2(x, y_of(channel_at(keys, c, f_of(x))))
            }).collect();
            let stroke = Stroke::new(1.8, col);
            match c / 3 {
                0 => {
                    pp.add(Shape::line(pts, stroke));
                }
                1 => pp.extend(Shape::dashed_line(&pts, stroke, 8.0, 4.0)),
                _ => pp.extend(Shape::dashed_line(&pts, stroke, 2.5, 3.0)),
            }
        }
        // Points des clés et poignées des clés choisies
        let key_pos = |k: &Key, c: usize| pos2(x_of(k.frame as f32), y_of(k.get(c)));
        let handle_pts = |keys: &[Key], k: usize, c: usize| {
            let (l, r) = handles_of(keys, k, c);
            let kp = key_pos(&keys[k], c);
            let at = |h: [f32; 2]| pos2(x_of(keys[k].frame as f32 + h[0]), y_of(keys[k].get(c) + h[1]));
            (kp, at(l), at(r))
        };
        for &(k, c) in &self.sel {
            if !self.on[c] {
                continue;
            }
            let (kp, l, r) = handle_pts(keys, k, c);
            for h in [l, r] {
                pp.line_segment([kp, h], Stroke::new(1.0, p.muted));
                pp.circle_filled(h, 3.5, p.text);
            }
        }
        for c in (0..9).filter(|&c| self.on[c]) {
            for (k, key) in keys.iter().enumerate() {
                let q = key_pos(key, c);
                let on = self.sel.contains(&(k, c));
                pp.circle_filled(q, if on { 4.5 } else { 3.5 }, if on { theme::pal().accent } else { Color32::from_gray(30) });
                pp.circle_stroke(q, if on { 4.5 } else { 3.5 }, Stroke::new(1.0, AXIS[c % 3]));
            }
        }
        // Tête de lecture
        let px = x_of(frame);
        pp.line_segment([pos2(px, plot.top()), pos2(px, plot.bottom())], Stroke::new(2.0, p.accent));
        pp.text(pos2(px + 4.0, plot.bottom() - 4.0), egui::Align2::LEFT_BOTTOM, format!("{}", frame.round()), egui::FontId::monospace(10.0), p.accent);

        // ---- Souris
        let hover = ui.input(|i| i.pointer.hover_pos());
        let on = self.on;
        let sel_now = self.sel.clone();
        let near_key = |keys: &[Key], at: Pos2| -> Option<(usize, usize)> {
            let mut best: Option<((usize, usize), f32)> = None;
            for c in (0..9).filter(|&c| on[c]) {
                for (k, key) in keys.iter().enumerate() {
                    let d = (key_pos(key, c) - at).length();
                    if d < 8.0 && best.is_none_or(|b| d < b.1) {
                        best = Some(((k, c), d));
                    }
                }
            }
            best.map(|b| b.0)
        };
        let near_handle = |keys: &[Key], at: Pos2| -> Option<(usize, usize, bool)> {
            for &(k, c) in &sel_now {
                if !on[c] {
                    continue;
                }
                let (_, l, r) = handle_pts(keys, k, c);
                if (r - at).length() < 7.0 {
                    return Some((k, c, true));
                }
                if (l - at).length() < 7.0 {
                    return Some((k, c, false));
                }
            }
            None
        };
        let shift = ui.input(|i| i.modifiers.shift);
        let in_plot = |q: Pos2| plot.contains(q);
        if resp.drag_started() {
            let at = ui.input(|i| i.pointer.press_origin()).unwrap_or(plot.center());
            if !in_plot(at) {
                self.drag = None;
            } else if resp.dragged_by(egui::PointerButton::Middle) || (shift && ui.input(|i| i.modifiers.ctrl)) {
                self.drag = Some(Drag::Pan { last: at });
            } else if at.y < plot.top() + 16.0 {
                self.drag = Some(Drag::Scrub);
            } else if let Some((k, c, right)) = near_handle(keys, at) {
                self.drag = Some(Drag::Handle { key: k, ch: c, right });
            } else if let Some(hit) = near_key(keys, at) {
                if !self.sel.contains(&hit) {
                    if !shift {
                        self.sel.clear();
                    }
                    self.sel.push(hit);
                }
                self.drag = Some(Drag::Keys { from: at, orig: keys.clone() });
            } else {
                if !shift {
                    self.sel.clear();
                }
                self.drag = Some(Drag::Box { from: at });
            }
        }
        let cur = resp.interact_pointer_pos().or(hover);
        match (self.drag.clone(), cur) {
            (Some(Drag::Pan { last }), Some(at)) if resp.dragged() => {
                let d = at - last;
                let (df, dv) = (-d.x / plot.width() * (f1 - f0), d.y / plot.height() * (v1 - v0));
                self.view = Some([f0 + df, f1 + df, v0 + dv, v1 + dv]);
                self.drag = Some(Drag::Pan { last: at });
            }
            (Some(Drag::Scrub), Some(at)) if resp.dragged() => out.seek = Some(f_of(at.x).round()),
            (Some(Drag::Keys { from, orig }), Some(at)) if resp.dragged() => {
                let df = ((at.x - from.x) / plot.width() * (f1 - f0)).round() as i32;
                let dv = -(at.y - from.y) / plot.height() * (v1 - v0);
                let ctrl = ui.input(|i| i.modifiers.command);
                let mut next = orig.clone();
                let moved: std::collections::BTreeSet<usize> = self.sel.iter().map(|s| s.0).collect();
                for &k in &moved {
                    next[k].frame = orig[k].frame + df;
                }
                for &(k, c) in &self.sel {
                    // Ctrl : seulement dans le temps
                    if !ctrl {
                        next[k].set(c, orig[k].get(c) + dv);
                    }
                }
                // Les clés restent dans l'ordre et sur des images différentes
                if next.windows(2).all(|w| w[0].frame < w[1].frame) {
                    *keys = next;
                    out.changed = true;
                }
            }
            (Some(Drag::Handle { key, ch, right }), Some(at)) if resp.dragged() && key < keys.len() => {
                let kf = keys[key].frame as f32;
                let kv = keys[key].get(ch);
                let mut h = [f_of(at.x) - kf, v_of(at.y) - kv];
                h[0] = if right { h[0].max(0.05) } else { h[0].min(-0.05) };
                if keys[key].handle == 0 || keys[key].handle == 3 {
                    set_mode(keys, key, 1);
                }
                let (l, r) = handles_of(keys, key, ch);
                let aligned = keys[key].handle == 1;
                let (nl, nr) = if right {
                    let other = if aligned { scale_opposite(h, l) } else { l };
                    (other, h)
                } else {
                    let other = if aligned { scale_opposite(h, r) } else { r };
                    (h, other)
                };
                keys[key].handles[ch] = [nl[0], nl[1], nr[0], nr[1]];
                out.changed = true;
            }
            (Some(Drag::Box { from }), Some(at)) if resp.dragged() => {
                let b = Rect::from_two_pos(from, at);
                pp.rect_filled(b, 0.0, p.accent.gamma_multiply(0.1));
                pp.rect_stroke(b, 0.0, Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
                if ui.input(|i| i.pointer.primary_released()) {
                    for c in (0..9).filter(|&c| self.on[c]) {
                        for (k, key) in keys.iter().enumerate() {
                            if b.contains(key_pos(key, c)) && !self.sel.contains(&(k, c)) {
                                self.sel.push((k, c));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        if resp.drag_stopped() {
            if let (Some(Drag::Box { from }), Some(at)) = (self.drag.take(), cur) {
                let b = Rect::from_two_pos(from, at);
                for c in (0..9).filter(|&c| self.on[c]) {
                    for (k, key) in keys.iter().enumerate() {
                        if b.contains(key_pos(key, c)) && !self.sel.contains(&(k, c)) {
                            self.sel.push((k, c));
                        }
                    }
                }
            }
            self.drag = None;
        }
        // Clic : choisir une clé, ou déplacer la tête de lecture dans la règle du haut
        if resp.clicked() {
            if let Some(at) = resp.interact_pointer_pos().filter(|q| in_plot(*q)) {
                if at.y < plot.top() + 16.0 {
                    out.seek = Some(f_of(at.x).round());
                } else {
                    match near_key(keys, at) {
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
            if scroll != 0.0 {
                if let Some(at) = at.filter(|q| in_plot(*q)) {
                    let k = (-scroll * 0.002).exp();
                    let [mut a, mut b, mut c, mut d] = self.view.unwrap_or(view);
                    if ctrl {
                        let v = v_of(at.y);
                        (c, d) = (v + (c - v) * k, v + (d - v) * k);
                    } else {
                        let f = f_of(at.x);
                        (a, b) = (f + (a - f) * k, f + (b - f) * k);
                    }
                    self.view = Some([a, b, c, d]);
                }
            }
        }
        // Clavier, quand la souris est sur l'éditeur
        if resp.hovered() && !ui.ctx().egui_wants_keyboard_input() {
            let (all, del, home) = ui.input(|i| (!i.modifiers.any() && i.key_pressed(egui::Key::A), i.key_pressed(egui::Key::X) || i.key_pressed(egui::Key::Delete), i.key_pressed(egui::Key::Home)));
            if all {
                self.sel = (0..keys.len()).flat_map(|k| (0..9).filter(|&c| self.on[c]).map(move |c| (k, c))).collect();
            }
            if del && !self.sel.is_empty() {
                let gone: std::collections::BTreeSet<usize> = self.sel.iter().map(|s| s.0).collect();
                let mut i = 0;
                keys.retain(|_| {
                    let keep = !gone.contains(&i);
                    i += 1;
                    keep
                });
                self.sel.clear();
                out.changed = true;
            }
            if home {
                self.view = None;
            }
        }
        if keys.is_empty() {
            pp.text(plot.center(), egui::Align2::CENTER_CENTER, crate::i18n::t("pas encore de clé : I ou ◇ pour en poser une"), egui::FontId::monospace(11.0), p.muted);
        }
        if self.drag.is_some() {
            ui.ctx().request_repaint();
        }
        out
    }
}

/// Change le type des poignées d'une clé. En quittant le mode automatique, les poignées
/// gardent leur forme actuelle : on part de là pour les régler.
fn set_mode(keys: &mut [Key], k: usize, mode: u8) {
    if keys[k].handles.len() != 9 {
        let hs: Vec<[f32; 4]> = (0..9).map(|c| {
            let (l, r) = handles_of(keys, k, c);
            [l[0], l[1], r[0], r[1]]
        }).collect();
        keys[k].handles = hs;
    }
    keys[k].handle = mode;
    if mode == 0 || mode == 3 {
        keys[k].handles.clear();
    }
}

/// Poignée opposée alignée : même direction que `h`, retournée, en gardant la longueur de `other`.
fn scale_opposite(h: [f32; 2], other: [f32; 2]) -> [f32; 2] {
    let lh = (h[0] * h[0] + h[1] * h[1]).sqrt().max(1e-6);
    let lo = (other[0] * other[0] + other[1] * other[1]).sqrt();
    [-h[0] / lh * lo, -h[1] / lh * lo]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poignees_alignees() {
        let o = scale_opposite([2.0, 2.0], [-1.0, 0.0]);
        assert!((o[0] + 0.7071).abs() < 1e-3 && (o[1] + 0.7071).abs() < 1e-3);
        let k = |frame: i32, x: f32| Key { frame, pos: [x, 0.0, 0.0], rot: [0.0; 3], scale: [1.0; 3], ease: true, interp: String::new(), handle: 0, handles: vec![] };
        let mut keys = vec![k(0, 0.0), k(10, 5.0), k(20, 10.0)];
        let before = handles_of(&keys, 1, 0);
        set_mode(&mut keys, 1, 1);
        // Passer en « alignées » garde la forme ; revenir en auto efface les poignées
        assert_eq!(handles_of(&keys, 1, 0), before);
        set_mode(&mut keys, 1, 0);
        assert!(keys[1].handles.is_empty());
    }
}
