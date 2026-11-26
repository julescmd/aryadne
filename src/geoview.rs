//! Géométrie et simulation en nodes : la vue. Un aperçu 3D des points et des arêtes, la lecture
//! image par image avec sa barre de cache, les miniatures des nodes et le tableau des attributs.

use crate::geo::{Engine, Geo, Stats};
use crate::nodeflow::TimeRange;
use crate::nodes::Graph;
use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, RichText, Sense, Stroke};
use std::collections::HashMap;

/// Miniature d'un node : quelques centaines de points déjà projetés, entre 0 et 1.
struct Thumb {
    key: (u64, i32),
    pts: Vec<([f32; 2], Color32)>,
    lines: Vec<[[f32; 2]; 2]>,
    count: usize,
    error: bool,
}

pub struct GeoView {
    pub engine: Engine,
    pub frame: i32,
    pub playing: bool,
    last_tick: Option<std::time::Instant>,
    acc: f32,
    yaw: f32,
    pitch: f32,
    dist: f32,
    target: [f32; 3],
    framed: bool,
    /// Tant que l'artiste n'a pas touché à la caméra, le cadre s'élargit pour suivre ce qui grandit
    auto: bool,
    /// Boîte englobante de tout ce qui a été vu depuis le dernier recadrage
    seen: Option<([f32; 3], [f32; 3])>,
    result: Option<Geo>,
    key: Option<(u64, i32, Option<u32>)>,
    pub error: Option<(u32, String)>,
    stats: Stats,
    ms: f32,
    thumbs: HashMap<u32, Thumb>,
    /// Node montré à la place de la sortie (« prévisualiser la node »)
    pub solo: Option<u32>,
    /// Zones de simulation du graphe (node de fin), pour la barre de cache
    zones: Vec<u32>,
    table: Option<((u64, i32, u32), Result<Geo, String>)>,
}

/// Couleur d'un point d'après son attribut `Cd` (ou la teinte par défaut).
fn point_color(g: &Geo, i: usize, base: Color32) -> Color32 {
    match g.col("Cd") {
        Some(c) => {
            let v = c.get(i);
            let q = |x: f32| (x.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0 + 0.5) as u8;
            Color32::from_rgb(q(v[0]), q(v[1]), q(v[2]))
        }
        None => base,
    }
}

impl GeoView {
    pub fn new(g: &Graph) -> GeoView {
        let start = g.time.unwrap_or_default().start;
        GeoView { engine: Engine::default(), frame: start, playing: false, last_tick: None, acc: 0.0, yaw: 0.6, pitch: 0.35, dist: 5.0, target: [0.0, 0.8, 0.0], framed: false, auto: true, seen: None, result: None, key: None, error: None, stats: Stats::default(), ms: 0.0, thumbs: HashMap::new(), solo: None, zones: vec![], table: None }
    }

    /// Les caches figés sur le disque, d'après le réglage « cache » des fins de zone.
    fn sync_baked(&mut self, doc: &Graph, root: &str) {
        self.engine.baked.clear();
        for n in doc.nodes.iter().filter(|n| n.op == "sim_out") {
            if let Some(crate::nodes::Param::S(rel)) = n.params.get("cache") {
                let dir = std::path::Path::new(root).join(rel);
                if !rel.is_empty() && dir.is_dir() {
                    self.engine.baked.insert(n.id, dir);
                }
            }
        }
    }

    /// Avance la lecture et recalcule ce qui doit l'être. `version` change à chaque modification du graphe.
    pub fn tick(&mut self, doc: &Graph, version: u64, root: &str, ctx: &egui::Context) {
        let time = doc.time.unwrap_or_default();
        if self.playing {
            let now = std::time::Instant::now();
            let dt = self.last_tick.map(|t| (now - t).as_secs_f32()).unwrap_or(0.0).min(0.25);
            self.last_tick = Some(now);
            // Tant que la simulation n'a pas rattrapé l'image demandée, la lecture l'attend
            if !self.engine.incomplete {
                self.acc += dt * time.fps;
                let step = self.acc.floor();
                self.acc -= step;
                self.frame += step as i32;
                if self.frame > time.end {
                    self.frame = time.start;
                }
            }
            ctx.request_repaint();
        } else {
            self.last_tick = None;
        }
        self.frame = self.frame.clamp(time.start.min(time.end), time.end.max(time.start));
        let key = (version, self.frame, self.solo);
        if self.key == Some(key) && !self.engine.incomplete {
            return;
        }
        if self.key.map(|k| k.0) != Some(version) {
            self.sync_baked(doc, root);
            self.zones = doc.nodes.iter().filter(|n| n.op == "sim_out").map(|n| n.id).collect();
        }
        self.key = Some(key);
        let t0 = std::time::Instant::now();
        // Une simulation longue à rattraper se calcule par tranches : l'interface reste vivante
        self.engine.deadline = Some(t0 + std::time::Duration::from_millis(30));
        let r = match self.solo {
            Some(id) => self.engine.eval_node(doc, id, 0, self.frame, root),
            None => self.engine.eval(doc, self.frame, root),
        };
        self.stats = self.engine.stats;
        self.ms = t0.elapsed().as_secs_f32() * 1000.0;
        let incomplete = self.engine.incomplete;
        match r {
            Ok(g) => {
                if !self.framed {
                    self.seen = None;
                    self.auto = true;
                }
                if (!self.framed || self.auto) && g.n > 0 {
                    self.frame_on(&g);
                }
                self.result = Some(g);
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e);
                self.playing = false;
            }
        }
        self.engine.incomplete = incomplete;
        if incomplete {
            ctx.request_repaint();
        }
    }

    /// Cadre la vue sur tout ce qui a été vu depuis le dernier recadrage : une fontaine qui grandit
    /// reste entière à l'écran, sans que la caméra saute à chaque image.
    fn frame_on(&mut self, g: &Geo) {
        let Some((lo, hi)) = g.bounds() else { return };
        let (lo, hi) = match self.seen {
            Some((a, b)) => ([a[0].min(lo[0]), a[1].min(lo[1]), a[2].min(lo[2])], [b[0].max(hi[0]), b[1].max(hi[1]), b[2].max(hi[2])]),
            None => (lo, hi),
        };
        if self.seen == Some((lo, hi)) && self.framed {
            return;
        }
        self.seen = Some((lo, hi));
        self.target = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
        let d = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt();
        self.dist = (d * 1.25).clamp(0.5, 500.0);
        self.framed = true;
    }

    /// Projette un point du monde dans le rectangle. Rend aussi sa profondeur (négative : derrière la caméra).
    fn project(&self, rect: Rect, p: [f32; 3]) -> (Pos2, f32) {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let d = [p[0] - self.target[0], p[1] - self.target[1], p[2] - self.target[2]];
        let x = d[0] * cy - d[2] * sy;
        let z = d[0] * sy + d[2] * cy;
        let y = d[1] * cp - z * sp;
        let z = d[1] * sp + z * cp;
        let depth = self.dist - z;
        let f = rect.height().min(rect.width()) * 0.9 / depth.max(0.05);
        (pos2(rect.center().x + x * f, rect.center().y - y * f), depth)
    }

    /// L'aperçu 3D : glisser pour tourner, molette pour s'approcher, double-clic pour recadrer.
    pub fn viewport(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let p = theme::pal();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6.0, Color32::from_gray(26));
        let resp = ui.interact(rect, ui.scope_id().with("geo-view"), Sense::click_and_drag());
        if resp.dragged() || (resp.hovered() && ui.input(|i| i.smooth_scroll_delta.y != 0.0)) {
            self.auto = false;
        }
        if resp.dragged_by(egui::PointerButton::Primary) {
            let d = resp.drag_delta();
            self.yaw += d.x * 0.01;
            self.pitch = (self.pitch + d.y * 0.01).clamp(-1.5, 1.5);
        }
        if resp.dragged_by(egui::PointerButton::Middle) || resp.dragged_by(egui::PointerButton::Secondary) {
            let d = resp.drag_delta();
            let k = self.dist / rect.height().max(1.0);
            let (sy, cy) = self.yaw.sin_cos();
            self.target[0] -= d.x * k * cy;
            self.target[2] += d.x * k * sy;
            self.target[1] += d.y * k;
        }
        if resp.hovered() {
            let sc = ui.input(|i| i.smooth_scroll_delta.y);
            if sc != 0.0 {
                self.dist = (self.dist * (-sc * 0.002).exp()).clamp(0.2, 2000.0);
            }
        }
        if resp.double_clicked() {
            if let Some(g) = self.result.clone() {
                self.seen = None;
                self.framed = false;
                self.frame_on(&g);
            }
        }
        // Sol : une grille discrète à la hauteur zéro
        let grid = Color32::from_gray(48);
        let step = 10f32.powf((self.dist * 0.4).max(0.01).log10().round()).max(0.01);
        for k in -5..=5 {
            let t = k as f32 * step;
            let cx = (self.target[0] / step).round() * step;
            let cz = (self.target[2] / step).round() * step;
            for (a, b) in [([cx + t, 0.0, cz - 5.0 * step], [cx + t, 0.0, cz + 5.0 * step]), ([cx - 5.0 * step, 0.0, cz + t], [cx + 5.0 * step, 0.0, cz + t])] {
                let ((pa, da), (pb, db)) = (self.project(rect, a), self.project(rect, b));
                if da > 0.05 && db > 0.05 {
                    painter.line_segment([pa, pb], Stroke::new(1.0, grid));
                }
            }
        }
        if let Some(g) = &self.result {
            let base = Color32::from_rgb(0xe6, 0xdc, 0xd2);
            let mut mesh = egui::Mesh::default();
            // Arêtes : de fins traits, en un seul maillage
            let stride_e = (g.edges.len() / 60_000).max(1);
            let line = Color32::from_rgba_unmultiplied(150, 160, 175, 150);
            for e in g.edges.iter().step_by(stride_e) {
                let (a, b) = (e[0] as usize, e[1] as usize);
                if a >= g.n || b >= g.n {
                    continue;
                }
                let ((pa, da), (pb, db)) = (self.project(rect, g.pos(a)), self.project(rect, g.pos(b)));
                if da <= 0.05 || db <= 0.05 {
                    continue;
                }
                let dir = pb - pa;
                let len = dir.length();
                if len < 0.01 {
                    continue;
                }
                let nrm = vec2(-dir.y, dir.x) / len * 0.5;
                let i = mesh.vertices.len() as u32;
                for q in [pa + nrm, pb + nrm, pb - nrm, pa - nrm] {
                    mesh.colored_vertex(q, line);
                }
                mesh.indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
            }
            // Points : de petits carrés, plus gros quand ils sont peu nombreux
            let stride = (g.n / 150_000).max(1);
            let shown = g.n / stride;
            let size = if shown > 40_000 { 1.0 } else if shown > 4_000 { 1.4 } else if g.edges.is_empty() { 2.4 } else { 1.8 };
            for i in (0..g.n).step_by(stride) {
                let (q, depth) = self.project(rect, g.pos(i));
                if depth <= 0.05 || !rect.expand(4.0).contains(q) {
                    continue;
                }
                let c = point_color(g, i, base);
                let k = mesh.vertices.len() as u32;
                for o in [vec2(-size, -size), vec2(size, -size), vec2(size, size), vec2(-size, size)] {
                    mesh.colored_vertex(q + o, c);
                }
                mesh.indices.extend_from_slice(&[k, k + 1, k + 2, k, k + 2, k + 3]);
            }
            painter.add(egui::Shape::mesh(mesh));
        }
        // Ce qu'on regarde, et ce que le calcul a coûté
        if let Some((_, e)) = &self.error {
            let gal = painter.layout(format!("{}  {}", crate::lucide::TRIANGLE_ALERT, crate::i18n::t(e)), egui::FontId::proportional(11.5), p.missing, rect.width() - 20.0);
            painter.galley(rect.left_top() + vec2(10.0, 8.0), gal, p.missing);
        } else if let Some(g) = &self.result {
            let mut line = format!("{} {}", g.n, crate::i18n::t_mid("points"));
            if !g.edges.is_empty() {
                line += &format!(" · {} {}", g.edges.len(), crate::i18n::t_mid("arêtes"));
            }
            painter.text(rect.left_top() + vec2(10.0, 8.0), egui::Align2::LEFT_TOP, line, egui::FontId::proportional(11.5), Color32::from_gray(190));
            let s = self.stats;
            let mut cost = format!("{} {} · {} {}", s.computed, crate::i18n::t_mid("refaits"), s.reused, crate::i18n::t_mid("repris du cache"));
            if s.steps > 0 {
                cost += &format!(" · {} {}", s.steps, crate::i18n::t_mid("pas simulés"));
            }
            cost += &format!(" · {:.1} ms", self.ms);
            painter.text(rect.left_bottom() + vec2(10.0, -8.0), egui::Align2::LEFT_BOTTOM, cost, egui::FontId::proportional(10.5), Color32::from_gray(130));
        }
        if self.engine.incomplete {
            painter.text(rect.right_top() + vec2(-10.0, 8.0), egui::Align2::RIGHT_TOP, crate::i18n::t("simulation en cours…"), egui::FontId::proportional(11.0), p.accent);
        }
        resp.on_hover_text(crate::i18n::t("Glisser : tourner · clic droit glissé : déplacer · molette : zoom · double-clic : recadrer"));
    }

    /// La barre de lecture : première image, lecture, image courante, réglages ; dessous, les images en cache.
    /// Rend vrai si les réglages de temps du graphe ont changé.
    pub fn transport(&mut self, ui: &mut egui::Ui, rect: Rect, time: &mut TimeRange) -> bool {
        let p = theme::pal();
        let mut changed = false;
        let bar = Rect::from_min_size(rect.min, vec2(rect.width(), 24.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)));
        child.spacing_mut().item_spacing.x = 2.0;
        let icon = |ui: &mut egui::Ui, icon: &str, tip: &str| ui.add(egui::Button::new(RichText::new(icon).size(14.0)).frame(false).min_size(vec2(24.0, 22.0))).on_hover_text(crate::i18n::t(tip)).clicked();
        if icon(&mut child, crate::lucide::SKIP_BACK, "Revenir à la première image") {
            self.frame = time.start;
            self.playing = false;
        }
        if icon(&mut child, if self.playing { crate::lucide::PAUSE } else { crate::lucide::PLAY }, "Lire ou mettre en pause (Espace, souris sur l'aperçu)") {
            self.playing = !self.playing;
        }
        child.add_space(4.0);
        child.add(egui::DragValue::new(&mut self.frame).range(time.start..=time.end).speed(0.2)).on_hover_text(crate::i18n::t("Image courante (glisser pour parcourir)"));
        child.label(RichText::new(format!("/ {}", time.end)).size(11.0).color(p.muted));
        child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::containers::menu::MenuButton::new(RichText::new(crate::lucide::SETTINGS).size(13.0)).ui(ui, |ui| {
                ui.set_min_width(210.0);
                changed |= crate::modifiers::row(ui, "première image", |ui| ui.add(egui::DragValue::new(&mut time.start).speed(0.2)).changed());
                changed |= crate::modifiers::row(ui, "dernière image", |ui| ui.add(egui::DragValue::new(&mut time.end).speed(0.2)).changed());
                changed |= crate::modifiers::row(ui, "images par seconde", |ui| ui.add(egui::DragValue::new(&mut time.fps).range(1.0..=240.0).speed(0.1)).changed());
                ui.separator();
                ui.label(RichText::new(format!("{} : {:.1} Mo", crate::i18n::t("Cache en mémoire"), self.engine.cached_bytes() as f32 / 1e6)).size(11.0).color(p.muted));
                if ui.button(format!("{}  {}", crate::lucide::TRASH_2, crate::i18n::t("vider le cache"))).clicked() {
                    self.engine.clear();
                    self.key = None;
                    ui.close();
                }
            }).0.on_hover_text(crate::i18n::t("Durée, cadence et cache"));
            if icon(ui, crate::lucide::SCAN, "Recadrer l'aperçu sur la géométrie") {
                self.framed = false;
                self.key = None;
            }
        });
        if time.end < time.start {
            time.end = time.start;
        }
        // Barre de cache : chaque image de simulation déjà calculée est teintée ; le trait marque l'image courante.
        // Cliquer ou glisser dessus déplace la lecture.
        let strip = Rect::from_min_size(pos2(rect.left(), bar.bottom() + 3.0), vec2(rect.width(), 8.0));
        let resp = ui.interact(strip.expand2(vec2(0.0, 3.0)), ui.scope_id().with("geo-cache-bar"), Sense::click_and_drag());
        let span = (time.end - time.start).max(1) as f32;
        ui.painter().rect_filled(strip, 3.0, p.field);
        let x_of = |f: i32| strip.left() + (f - time.start) as f32 / span * strip.width();
        for end in &self.zones {
            for f in self.engine.cached_frames(*end) {
                let x0 = x_of(f - 1).max(strip.left());
                let x1 = x_of(f).min(strip.right());
                if x1 > x0 {
                    ui.painter().rect_filled(Rect::from_min_max(pos2(x0, strip.top() + 2.0), pos2(x1 + 0.5, strip.bottom() - 2.0)), 0.0, if self.engine.baked.contains_key(end) { p.success } else { p.accent.gamma_multiply(0.75) });
                }
            }
            if self.engine.baked.contains_key(end) {
                ui.painter().rect_filled(Rect::from_min_max(pos2(strip.left(), strip.top() + 2.0), pos2(strip.right(), strip.bottom() - 2.0)), 0.0, p.success.gamma_multiply(0.55));
            }
        }
        let x = x_of(self.frame);
        ui.painter().line_segment([pos2(x, strip.top() - 2.0), pos2(x, strip.bottom() + 2.0)], Stroke::new(2.0, p.text));
        if let Some(at) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
            self.frame = (time.start as f32 + ((at.x - strip.left()) / strip.width()).clamp(0.0, 1.0) * span).round() as i32;
            self.playing = false;
        }
        resp.on_hover_text(crate::i18n::t("Images de simulation en cache. Cliquer ou glisser pour se déplacer dans le temps."));
        changed
    }

    /// Miniature d'un node : ce qui en sort, vu de face. Calculée quand le graphe ou l'image changent.
    pub fn thumb(&mut self, painter: &egui::Painter, rect: Rect, doc: &Graph, id: u32, version: u64, root: &str) {
        let p = theme::pal();
        let key = (version, self.frame);
        if self.thumbs.get(&id).map(|t| t.key) != Some(key) {
            let saved = (self.engine.stats, self.engine.incomplete, self.engine.deadline.take());
            // Une miniature ne lance pas une simulation : elle prend ce qui est déjà en cache
            self.engine.deadline = Some(std::time::Instant::now() + std::time::Duration::from_millis(4));
            let r = self.engine.eval_node(doc, id, 0, self.frame, root);
            (self.engine.stats, self.engine.incomplete, self.engine.deadline) = saved;
            let mut t = Thumb { key, pts: vec![], lines: vec![], count: 0, error: r.is_err() };
            if let Ok(g) = r {
                t.count = g.n;
                if let Some((lo, hi)) = g.bounds() {
                    // Vue de trois quarts, cadrée sur la boîte englobante
                    let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
                    let r = (((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt() * 0.5).max(1e-3);
                    let (sy, cy) = 0.6f32.sin_cos();
                    let (sp, cp) = 0.4f32.sin_cos();
                    let flat = |q: [f32; 3]| -> [f32; 2] {
                        let d = [(q[0] - c[0]) / r, (q[1] - c[1]) / r, (q[2] - c[2]) / r];
                        let x = d[0] * cy - d[2] * sy;
                        let z = d[0] * sy + d[2] * cy;
                        [0.5 + x * 0.48, 0.5 - (d[1] * cp - z * sp) * 0.48]
                    };
                    let base = Color32::from_rgb(0xe6, 0xdc, 0xd2);
                    let stride = (g.n / 500).max(1);
                    t.pts = (0..g.n).step_by(stride).map(|i| (flat(g.pos(i)), point_color(&g, i, base))).collect();
                    let stride_e = (g.edges.len() / 500).max(1);
                    t.lines = g.edges.iter().step_by(stride_e).filter(|e| (e[0] as usize) < g.n && (e[1] as usize) < g.n).map(|e| [flat(g.pos(e[0] as usize)), flat(g.pos(e[1] as usize))]).collect();
                }
            }
            self.thumbs.insert(id, t);
        }
        let Some(t) = self.thumbs.get(&id) else { return };
        painter.rect_filled(rect, 4.0, Color32::from_gray(30));
        let sq = Rect::from_center_size(rect.center(), vec2(rect.height(), rect.height()));
        let at = |q: [f32; 2]| pos2(sq.left() + q[0] * sq.width(), sq.top() + q[1] * sq.height());
        for l in &t.lines {
            painter.line_segment([at(l[0]), at(l[1])], Stroke::new(0.6, Color32::from_rgba_unmultiplied(150, 160, 175, 110)));
        }
        for (q, c) in &t.pts {
            painter.rect_filled(Rect::from_center_size(at(*q), vec2(1.6, 1.6)), 0.0, *c);
        }
        let note = if t.error { crate::i18n::t("rien à montrer") } else { format!("{} {}", t.count, crate::i18n::t_mid("points")) };
        painter.text(rect.left_bottom() + vec2(5.0, -3.0), egui::Align2::LEFT_BOTTOM, note, egui::FontId::proportional(9.5), p.muted);
    }

    /// Tableau des attributs de ce qui sort d'un node : nom, genre, étendue des valeurs.
    pub fn attributes(&mut self, ui: &mut egui::Ui, doc: &Graph, id: u32, version: u64, root: &str) {
        let p = theme::pal();
        let key = (version, self.frame, id);
        if self.table.as_ref().map(|t| t.0) != Some(key) {
            let saved = (self.engine.stats, self.engine.incomplete, self.engine.deadline.take());
            self.engine.deadline = Some(std::time::Instant::now() + std::time::Duration::from_millis(4));
            let is_out = doc.nodes.iter().any(|n| n.id == id && n.op == "geo_out");
            let r = if is_out { self.engine.eval(doc, self.frame, root) } else { self.engine.eval_node(doc, id, 0, self.frame, root) };
            (self.engine.stats, self.engine.incomplete, self.engine.deadline) = saved;
            self.table = Some((key, r.map_err(|e| e.1)));
        }
        let Some((_, Ok(g))) = &self.table else { return };
        egui::Frame::new().fill(p.field).corner_radius(8).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(theme::section(&crate::i18n::t("ATTRIBUTS")));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{} {}", g.n, crate::i18n::t_mid("points"))).size(11.0).color(p.muted));
                });
            });
            ui.add_space(2.0);
            let row = |ui: &mut egui::Ui, name: &str, kind: &str, range: String| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(RichText::new(name).monospace().size(11.5));
                    ui.label(RichText::new(crate::i18n::t_mid(kind)).size(10.5).color(p.muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(range).size(10.5).color(p.muted));
                    });
                });
            };
            for (name, c) in &g.cols {
                let kind = match c.width() {
                    1 => "nombre à virgule",
                    3 => "vecteur",
                    4 => "couleur",
                    _ => "valeurs",
                };
                let (mut lo, mut hi) = (f32::MAX, f32::MIN);
                for comp in c.data.iter().take(3) {
                    for x in comp {
                        lo = lo.min(*x);
                        hi = hi.max(*x);
                    }
                }
                let range = if lo <= hi { format!("{lo:.2} … {hi:.2}") } else { String::new() };
                row(ui, name, kind, range);
            }
            if !g.edges.is_empty() {
                row(ui, crate::i18n::t_mid("arêtes").as_str(), "", format!("{}", g.edges.len()));
            }
            if g.cols.is_empty() {
                ui.label(RichText::new(crate::i18n::t("Aucun point pour l'instant.")).size(11.0).color(p.muted));
            }
        });
        ui.add_space(6.0);
    }

    /// Oublie ce qui est affiché : au prochain passage, tout est redemandé au moteur.
    pub fn refresh(&mut self) {
        self.key = None;
        self.table = None;
        self.thumbs.clear();
    }
}
