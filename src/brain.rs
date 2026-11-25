//! Vue « Cerveau » : graphe de tout le coffre, disposé par simulation de forces.
use crate::theme;
use crate::vault::Graph;
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use std::collections::HashMap;

pub struct Brain {
    pub graph: Graph,
    pos: Vec<Vec2>,
    vel: Vec<Vec2>,
    heat: f32,
    pan: Vec2,
    zoom: f32,
    dragging: Option<usize>,
    pub with_folders: bool,
    pub with_files: bool,
    pub dirty: bool,
    /// Dossier dans lequel on est entré ("" = tout le coffre).
    pub scope: String,
    /// Graphe local : la note au centre (ses voisines à deux liens) ; None : tout le coffre
    pub local: Option<String>,
    /// Cadre automatiquement le graphe tant que l'utilisateur n'a pas bougé la vue.
    auto_fit: bool,
}

impl Default for Brain {
    fn default() -> Self {
        Self {
            graph: Graph { nodes: vec![], edges: vec![] },
            pos: vec![],
            vel: vec![],
            heat: 1.0,
            pan: Vec2::ZERO,
            zoom: 1.0,
            dragging: None,
            with_folders: true,
            with_files: false,
            dirty: true,
            scope: String::new(),
            local: None,
            auto_fit: true,
        }
    }
}

impl Brain {
    /// Remplace le graphe en gardant la position des nœuds déjà connus.
    pub fn set_graph(&mut self, g: Graph) {
        let old: HashMap<String, Vec2> = self.graph.nodes.iter().map(|n| n.rel.clone()).zip(self.pos.iter().copied()).collect();
        let n = g.nodes.len();
        self.pos = g
            .nodes
            .iter()
            .enumerate()
            .map(|(i, node)| {
                old.get(&node.rel).copied().unwrap_or_else(|| {
                    // Disposition initiale en spirale, stable d'une ouverture à l'autre.
                    let a = i as f32 * 2.399;
                    let r = 30.0 * (i as f32 + 1.0).sqrt();
                    vec2(a.cos() * r, a.sin() * r)
                })
            })
            .collect();
        self.vel = vec![Vec2::ZERO; n];
        self.graph = g;
        self.heat = 1.0;
        self.dirty = false;
    }

    fn degree(&self) -> Vec<usize> {
        let mut d = vec![0; self.graph.nodes.len()];
        for &(a, b, _) in &self.graph.edges {
            d[a] += 1;
            d[b] += 1;
        }
        d
    }

    fn step(&mut self) {
        let n = self.pos.len();
        if n == 0 || self.heat < 0.002 {
            return;
        }
        let k = 55.0_f32;
        let mut force = vec![Vec2::ZERO; n];
        for i in 0..n {
            for j in (i + 1)..n {
                let d = self.pos[i] - self.pos[j];
                let dist2 = d.length_sq().max(25.0);
                let f = d * (k * k / dist2);
                force[i] += f;
                force[j] -= f;
            }
            force[i] -= self.pos[i] * 0.06; // gravité vers le centre
        }
        for &(a, b, folder) in &self.graph.edges {
            let d = self.pos[b] - self.pos[a];
            let len = d.length().max(1.0);
            let rest = if folder { k * 0.8 } else { k };
            let f = d / len * (len - rest) * 0.08;
            force[a] += f;
            force[b] -= f;
        }
        for i in 0..n {
            if Some(i) == self.dragging {
                continue;
            }
            self.vel[i] = (self.vel[i] + force[i] * 0.05 * self.heat) * 0.82;
            let v = self.vel[i];
            let max = 20.0;
            self.pos[i] += if v.length() > max { v.normalized() * max } else { v };
        }
        self.heat *= 0.992;
    }

    /// Dessine le graphe. Retourne le chemin du nœud cliqué.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<String> {
        let p = theme::pal();
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, p.bg);

        // Grille de points, façon papier millimétré.
        let grid = 24.0 * self.zoom;
        if grid > 8.0 {
            let origin = rect.center() + self.pan;
            let start = pos2(
                rect.left() + (origin.x - rect.left()).rem_euclid(grid),
                rect.top() + (origin.y - rect.top()).rem_euclid(grid),
            );
            let mut y = start.y;
            while y < rect.bottom() {
                let mut x = start.x;
                while x < rect.right() {
                    painter.rect_filled(Rect::from_center_size(pos2(x, y), vec2(1.5, 1.5)), 0.0, p.line);
                    x += grid;
                }
                y += grid;
            }
        }

        for _ in 0..3 {
            self.step();
        }
        if self.heat > 0.002 || self.dragging.is_some() {
            ui.ctx().request_repaint();
        }

        let center = rect.center() + self.pan;
        let to_screen = |v: Vec2, zoom: f32| center + v * zoom;
        let deg = self.degree();
        let size = |i: usize| 6.0 + (deg[i] as f32).sqrt() * 3.0;

        // Nœud survolé
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let hovered = pointer.filter(|pp| rect.contains(*pp)).and_then(|pp| {
            (0..self.pos.len())
                .filter(|&i| (to_screen(self.pos[i], self.zoom) - pp).length() < size(i) * self.zoom.max(0.6) + 4.0)
                .min_by(|&a, &b| {
                    let da = (to_screen(self.pos[a], self.zoom) - pp).length();
                    let db = (to_screen(self.pos[b], self.zoom) - pp).length();
                    da.total_cmp(&db)
                })
        });
        let focus = self.dragging.or(hovered);
        let neighbor = |i: usize| -> bool {
            focus.is_some_and(|f| f == i || self.graph.edges.iter().any(|&(a, b, _)| (a == f && b == i) || (b == f && a == i)))
        };

        // Interactions : glisser un nœud, déplacer la vue, zoomer.
        if resp.drag_started() {
            self.dragging = hovered;
        }
        if resp.dragged() {
            let d = resp.drag_delta();
            match self.dragging {
                Some(i) => {
                    self.pos[i] += d / self.zoom;
                    self.heat = self.heat.max(0.3);
                }
                None => self.pan += d,
            }
        }
        if resp.drag_stopped() {
            self.dragging = None;
        }
        if resp.hovered() {
            let (scroll, zoom_delta) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = zoom_delta * (scroll * 0.0015).exp();
            if factor != 1.0 {
                let new = (self.zoom * factor).clamp(0.15, 4.0);
                if let Some(pp) = pointer {
                    let world = (pp - center) / self.zoom;
                    self.pan += world * (self.zoom - new);
                }
                self.zoom = new;
            }
        }

        if resp.dragged() || (resp.hovered() && ui.input(|i| i.smooth_scroll_delta.y != 0.0 || i.zoom_delta() != 1.0)) {
            self.auto_fit = false;
        }
        if self.auto_fit && !self.pos.is_empty() {
            let mut bb = Rect::NOTHING;
            for v in &self.pos {
                bb.extend_with(pos2(v.x, v.y));
            }
            let room = rect.shrink2(vec2(140.0, 60.0)).size();
            let target = (room.x / bb.width().max(1.0)).min(room.y / bb.height().max(1.0)).clamp(0.2, 2.0);
            self.zoom += (target - self.zoom) * 0.2;
            self.pan += (-bb.center().to_vec2() * self.zoom - self.pan) * 0.2;
            ui.ctx().request_repaint();
        }

        let center = rect.center() + self.pan;
        let to_screen = |v: Vec2| center + v * self.zoom;

        for &(a, b, folder) in &self.graph.edges {
            let lit = focus.is_some_and(|f| f == a || f == b);
            let color = if lit { p.accent } else if folder { p.line } else { p.muted.linear_multiply(0.55) };
            let width = if lit { 1.6 } else { 1.0 };
            painter.line_segment([to_screen(self.pos[a]), to_screen(self.pos[b])], Stroke::new(width, color));
        }

        let font = egui::FontId::monospace(11.0);
        for (i, node) in self.graph.nodes.iter().enumerate() {
            let c = to_screen(self.pos[i]);
            let s = size(i) * self.zoom.max(0.5);
            let r = Rect::from_center_size(c, vec2(s, s));
            let dim = focus.is_some() && !neighbor(i);
            let fade = |col: Color32| if dim { col.linear_multiply(0.3) } else { col };
            if node.is_dir {
                painter.rect_filled(r, 0.0, p.bg);
                painter.rect_stroke(r, 0.0, Stroke::new(1.5, fade(p.accent)), egui::StrokeKind::Inside);
            } else if crate::vault::is_md(&node.rel) {
                painter.rect_filled(r, 0.0, fade(if Some(i) == focus { p.accent } else { p.text }));
            } else {
                painter.rect_filled(r, 0.0, fade(p.muted));
            }
            let show_label = self.zoom > 0.45 || neighbor(i) || deg[i] >= 4;
            if show_label {
                let label = if node.is_dir { format!("{}/", node.label) } else { node.label.clone() };
                painter.text(c + vec2(s / 2.0 + 5.0, 0.0), egui::Align2::LEFT_CENTER, label, font.clone(), fade(if node.is_dir { p.accent } else { p.text }));
            }
        }

        // Légende et aide
        let info = format!(
            "{} nœuds · {} liens   ·   molette : zoom   glisser : déplacer   clic : ouvrir",
            self.graph.nodes.len(),
            self.graph.edges.iter().filter(|e| !e.2).count()
        );
        painter.text(rect.left_bottom() + vec2(12.0, -12.0), egui::Align2::LEFT_BOTTOM, info, egui::FontId::monospace(11.0), p.muted);

        if resp.clicked() {
            if let Some(i) = hovered {
                return Some(self.graph.nodes[i].rel.clone());
            }
        }
        if hovered.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let _ = Pos2::ZERO;
        None
    }

    pub fn recenter(&mut self) {
        self.auto_fit = true;
    }
}
