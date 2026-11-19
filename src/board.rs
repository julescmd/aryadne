//! Boards : canevas infini façon PureRef. Un board = un fichier `.board.json` du coffre.
//!
//! Outils : sélection (V), dessin (B), gomme (E), texte (T).
//! Sélection multiple au lasso, 4 poignées de redimensionnement, annuler/rétablir,
//! menu clic droit, coller une image (Ctrl+V).
use crate::images::{Images, Size, Slot};
use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Key, Modifiers, PointerButton, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Kind {
    Image {
        rel: String,
        #[serde(default)]
        flip: bool,
    },
    Text {
        text: String,
        #[serde(default = "text_size")]
        size: f32,
    },
    Note {
        rel: String,
    },
    /// Trait à main levée. Points normalisés (0..1) dans le rectangle de l'élément.
    Stroke {
        points: Vec<[f32; 2]>,
        color: [u8; 4],
        width: f32,
    },
}

fn text_size() -> f32 {
    18.0
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Item {
    #[serde(flatten)]
    pub kind: Kind,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Item {
    fn rect(&self) -> Rect {
        Rect::from_min_size(pos2(self.x, self.y), vec2(self.w, self.h))
    }
    fn set_rect(&mut self, r: Rect) {
        self.x = r.min.x;
        self.y = r.min.y;
        self.w = r.width();
        self.h = r.height();
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BoardFile {
    pub items: Vec<Item>,
    #[serde(default)]
    pub pan: [f32; 2],
    #[serde(default = "one")]
    pub zoom: f32,
}

fn one() -> f32 {
    1.0
}

impl Default for BoardFile {
    fn default() -> Self {
        Self { items: vec![], pan: [0.0, 0.0], zoom: 1.0 }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
    Select,
    Pen,
    Eraser,
    Text,
}

enum Drag {
    /// Déplacement de la sélection : positions de départ.
    Move(Vec<(usize, Pos2)>),
    /// Redimensionnement : élément, coin fixe (monde), rectangle de départ.
    Resize { i: usize, anchor: Pos2, start: Rect },
    Pan,
    Marquee(Pos2),
    Draw,
    Erase,
}

pub enum Event {
    Open(String),
    PasteImage(Pos2),
}

pub struct Board {
    pub rel: Option<String>,
    pub data: BoardFile,
    pub selection: Vec<usize>,
    editing: Option<usize>,
    drag: Option<Drag>,
    pub dirty_since: Option<Instant>,
    /// Centre de la vue en coordonnées du board (pour y ajouter des éléments).
    pub view_center: Vec2,
    fit_next: bool,
    pub tool: Tool,
    pub pen_color: Color32,
    pub pen_width: f32,
    stroke_pts: Vec<Pos2>,
    undo: Vec<Vec<Item>>,
    redo: Vec<Vec<Item>>,
    /// Pointeur au dernier clic droit (pour coller au bon endroit).
    menu_at: Pos2,
}

impl Default for Board {
    fn default() -> Self {
        Self {
            rel: None,
            data: BoardFile::default(),
            selection: vec![],
            editing: None,
            drag: None,
            dirty_since: None,
            view_center: Vec2::ZERO,
            fit_next: false,
            tool: Tool::Select,
            pen_color: Color32::from_rgb(0xff, 0x4f, 0x1f),
            pen_width: 4.0,
            stroke_pts: vec![],
            undo: vec![],
            redo: vec![],
            menu_at: Pos2::ZERO,
        }
    }
}

pub fn is_board(rel: &str) -> bool {
    rel.to_lowercase().ends_with(".board.json")
}

pub fn board_name(rel: &str) -> String {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.strip_suffix(".board.json").unwrap_or(name).to_string()
}

/// Distance d'un point à un segment.
fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() < 1e-6 { 0.0 } else { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
    (p - (a + ab * t)).length()
}

impl Board {
    pub fn load(root: &str, rel: &str) -> Result<Self, String> {
        let text = crate::vault::read(root, rel)?;
        let data: BoardFile = if text.trim().is_empty() {
            BoardFile::default()
        } else {
            serde_json::from_str(&text).map_err(|e| format!("Board illisible ({rel}) : {e}"))?
        };
        Ok(Self { rel: Some(rel.to_string()), data, ..Default::default() })
    }

    pub fn save(&mut self, root: &str) -> Result<(), String> {
        self.dirty_since = None;
        let Some(rel) = &self.rel else { return Ok(()) };
        let json = serde_json::to_string_pretty(&self.data).map_err(|e| e.to_string())?;
        crate::vault::write(root, rel, &json)
    }

    fn touch(&mut self) {
        self.dirty_since = Some(Instant::now());
    }

    /// Mémorise l'état actuel avant une modification (pour Ctrl+Z).
    pub fn checkpoint(&mut self) {
        if self.undo.last() != Some(&self.data.items) {
            self.undo.push(self.data.items.clone());
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.data.items, prev));
            self.after_history();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.data.items, next));
            self.after_history();
        }
    }

    fn after_history(&mut self) {
        self.selection.retain(|&i| i < self.data.items.len());
        self.editing = None;
        self.touch();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Ajoute un élément centré sur `at` (coordonnées du board).
    pub fn add(&mut self, kind: Kind, at: Vec2, size: Vec2) {
        self.checkpoint();
        self.data.items.push(Item { kind, x: at.x - size.x / 2.0, y: at.y - size.y / 2.0, w: size.x, h: size.y });
        self.selection = vec![self.data.items.len() - 1];
        self.touch();
    }

    /// Dispose des images en rangées, à droite de ce qui existe déjà.
    pub fn add_grid(&mut self, images: Vec<(String, f32)>) {
        if images.is_empty() {
            return;
        }
        self.checkpoint();
        let right = self.data.items.iter().map(|i| i.x + i.w).fold(f32::MIN, f32::max);
        let start_x = if right == f32::MIN { 0.0 } else { right + 120.0 };
        let top = self.data.items.iter().map(|i| i.y).fold(f32::MAX, f32::min);
        let top = if top == f32::MAX { 0.0 } else { top };
        let (row_h, max_w, gap) = (240.0, 1400.0, 16.0);
        let (mut x, mut y) = (start_x, top);
        let first = self.data.items.len();
        for (rel, aspect) in images {
            let w = row_h * aspect.clamp(0.2, 5.0);
            if x > start_x && x + w > start_x + max_w {
                x = start_x;
                y += row_h + gap;
            }
            self.data.items.push(Item { kind: Kind::Image { rel, flip: false }, x, y, w, h: row_h });
            x += w + gap;
        }
        self.selection = (first..self.data.items.len()).collect();
        self.fit_next = true;
        self.touch();
    }

    pub fn fit(&mut self) {
        self.fit_next = true;
    }

    pub fn delete_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        let mut sel = std::mem::take(&mut self.selection);
        sel.sort_unstable();
        sel.dedup();
        for i in sel.into_iter().rev() {
            if i < self.data.items.len() {
                self.data.items.remove(i);
            }
        }
        self.editing = None;
        self.touch();
    }

    fn duplicate_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        let mut new_sel = vec![];
        for &i in &self.selection.clone() {
            if let Some(mut it) = self.data.items.get(i).cloned() {
                it.x += 30.0;
                it.y += 30.0;
                self.data.items.push(it);
                new_sel.push(self.data.items.len() - 1);
            }
        }
        self.selection = new_sel;
        self.touch();
    }

    fn reorder(&mut self, to_front: bool) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        let mut sel = self.selection.clone();
        sel.sort_unstable();
        sel.dedup();
        let mut picked: Vec<Item> = sel.iter().rev().map(|&i| self.data.items.remove(i)).collect();
        picked.reverse();
        let n = picked.len();
        if to_front {
            let start = self.data.items.len();
            self.data.items.extend(picked);
            self.selection = (start..start + n).collect();
        } else {
            for (k, it) in picked.into_iter().enumerate() {
                self.data.items.insert(k, it);
            }
            self.selection = (0..n).collect();
        }
        self.touch();
    }

    fn bounds(&self, which: Option<&[usize]>) -> Option<Rect> {
        let mut bb = Rect::NOTHING;
        let mut any = false;
        for (i, it) in self.data.items.iter().enumerate() {
            if which.is_none_or(|w| w.contains(&i)) {
                bb = bb.union(it.rect());
                any = true;
            }
        }
        any.then_some(bb)
    }

    pub fn drop_file(&mut self, rel: &str, at: Vec2, images: &mut Images) {
        if crate::images::is_image(rel) {
            let aspect = images.peek(rel, Size::Thumb).map(|l| l.dims[0] as f32 / l.dims[1].max(1) as f32).unwrap_or(1.5);
            let h = 260.0;
            self.add(Kind::Image { rel: rel.to_string(), flip: false }, at, vec2(h * aspect, h));
        } else if crate::vault::is_md(rel) {
            self.add(Kind::Note { rel: rel.to_string() }, at, vec2(300.0, 180.0));
        } else {
            self.add(Kind::Text { text: format!("`{rel}`"), size: 16.0 }, at, vec2(300.0, 60.0));
        }
    }

    /// Hit test d'un élément (les traits se testent sur leur tracé, pas leur cadre).
    fn hit(&self, it: &Item, world: Pos2, tol: f32) -> bool {
        match &it.kind {
            Kind::Stroke { points, width, .. } => {
                if !it.rect().expand(tol + width).contains(world) {
                    return false;
                }
                let pts: Vec<Pos2> = points.iter().map(|p| pos2(it.x + p[0] * it.w, it.y + p[1] * it.h)).collect();
                if pts.len() == 1 {
                    return (pts[0] - world).length() < tol + width / 2.0;
                }
                pts.windows(2).any(|w| seg_dist(world, w[0], w[1]) < tol + width / 2.0)
            }
            _ => it.rect().contains(world),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, images: &mut Images, note_preview: &dyn Fn(&str) -> String) -> Option<Event> {
        let p = theme::pal();
        let mut event = None;
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, p.panel);

        // Les images gardent leurs vraies proportions dès qu'on les connaît.
        for it in &mut self.data.items {
            if let Kind::Image { rel, .. } = &it.kind {
                if let Some(l) = images.peek(rel, Size::Thumb) {
                    let aspect = l.dims[0] as f32 / l.dims[1].max(1) as f32;
                    if ((it.w / it.h.max(1.0)) - aspect).abs() > 0.01 {
                        it.h = it.w / aspect;
                        self.dirty_since.get_or_insert_with(Instant::now);
                    }
                }
            }
        }

        if self.fit_next && rect.width() > 10.0 {
            self.fit_next = false;
            if let Some(bb) = self.bounds(None) {
                let z = ((rect.width() - 100.0) / bb.width().max(1.0)).min((rect.height() - 100.0) / bb.height().max(1.0)).clamp(0.03, 2.0);
                self.data.zoom = z;
                let c = bb.center();
                self.data.pan = [-c.x * z, -c.y * z];
            }
        }

        let zoom = self.data.zoom;
        let origin = rect.center() + Vec2::from(self.data.pan);
        let to_screen = move |w: Pos2| origin + w.to_vec2() * zoom;
        let to_world = move |s: Pos2| pos2((s.x - origin.x) / zoom, (s.y - origin.y) / zoom);
        self.view_center = to_world(rect.center()).to_vec2();
        let screen_rect = |r: Rect| Rect::from_min_max(to_screen(r.min), to_screen(r.max));

        // Grille de points
        let step = 32.0 * zoom;
        if step > 9.0 {
            let mut gy = rect.top() + (origin.y - rect.top()).rem_euclid(step);
            while gy < rect.bottom() {
                let mut gx = rect.left() + (origin.x - rect.left()).rem_euclid(step);
                while gx < rect.right() {
                    painter.rect_filled(Rect::from_center_size(pos2(gx, gy), vec2(1.5, 1.5)), 0.0, p.line);
                    gx += step;
                }
                gy += step;
            }
        }

        let (pointer, mods, space) = ui.input(|i| (i.pointer.interact_pos().or(i.pointer.hover_pos()), i.modifiers, i.key_down(Key::Space)));
        let pointer_in = pointer.filter(|pt| rect.contains(*pt));
        let world_ptr = pointer_in.map(to_world);
        let tol = 6.0 / zoom;
        let hit = world_ptr.and_then(|w| (0..self.data.items.len()).rev().find(|&i| self.hit(&self.data.items[i], w, tol)));

        // Poignée sous le pointeur (sélection unique, outil sélection)
        let single = (self.selection.len() == 1 && self.tool == Tool::Select).then(|| self.selection[0]).filter(|&i| i < self.data.items.len());
        let corners = |r: Rect| [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
        let handle_hit = single.and_then(|i| {
            let r = screen_rect(self.data.items[i].rect());
            let pt = pointer_in?;
            corners(r).iter().position(|c| Rect::from_center_size(*c, vec2(16.0, 16.0)).contains(pt))
        });

        // ---------- dessin des éléments ----------
        for (i, it) in self.data.items.iter().enumerate() {
            let r = screen_rect(it.rect());
            if !r.intersects(rect) {
                continue;
            }
            match &it.kind {
                Kind::Image { rel, flip } => {
                    let size = if r.width().max(r.height()) > 340.0 { Size::Full } else { Size::Thumb };
                    let slot = match images.get(rel, size) {
                        Slot::Loading if size == Size::Full => images.peek(rel, Size::Thumb).map(Slot::Ready).unwrap_or(Slot::Loading),
                        s => s,
                    };
                    let uv = if *flip { Rect::from_min_max(pos2(1.0, 0.0), pos2(0.0, 1.0)) } else { Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)) };
                    match slot {
                        Slot::Ready(l) => {
                            painter.image(l.tex.id(), r, uv, Color32::WHITE);
                        }
                        Slot::Loading => {
                            painter.rect_filled(r, 0.0, p.line);
                        }
                        Slot::Failed(e) => {
                            painter.rect_filled(r, 0.0, p.bg);
                            painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.missing), egui::StrokeKind::Inside);
                            painter.with_clip_rect(r).text(r.center(), egui::Align2::CENTER_CENTER, format!("{rel}\n{e}"), egui::FontId::monospace(11.0), p.missing);
                        }
                    }
                }
                Kind::Text { text, size } => {
                    if self.editing != Some(i) {
                        let font = egui::FontId::proportional((size * zoom).max(3.0));
                        let galley = painter.layout(text.clone(), font, p.text, r.width().max(10.0));
                        painter.with_clip_rect(r.expand(2.0)).galley(r.min, galley, p.text);
                    }
                }
                Kind::Note { rel } => {
                    painter.rect_filled(r, 0.0, p.bg);
                    painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.link), egui::StrokeKind::Inside);
                    let clip = painter.with_clip_rect(r.shrink(1.0).intersect(rect));
                    clip.text(r.min + vec2(12.0, 12.0) * zoom, egui::Align2::LEFT_TOP, format!("# {}", crate::vault::stem(rel)), egui::FontId::new((15.0 * zoom).max(3.0), theme::bold()), p.link);
                    let body = painter.layout(note_preview(rel), egui::FontId::monospace((12.0 * zoom).max(3.0)), p.muted, (r.width() - 24.0 * zoom).max(10.0));
                    clip.galley(r.min + vec2(12.0, 38.0) * zoom, body, p.muted);
                }
                Kind::Stroke { points, color, width } => {
                    let col = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
                    let pts: Vec<Pos2> = points.iter().map(|q| pos2(r.min.x + q[0] * r.width(), r.min.y + q[1] * r.height())).collect();
                    let w = (width * zoom).max(0.5);
                    if pts.len() == 1 {
                        painter.circle_filled(pts[0], w / 2.0, col);
                    } else {
                        painter.add(Shape::line(pts, Stroke::new(w, col)));
                    }
                }
            }
            if self.selection.contains(&i) {
                painter.rect_stroke(r, 0.0, Stroke::new(1.5, p.accent), egui::StrokeKind::Outside);
            } else if hit == Some(i) && self.tool == Tool::Select && self.drag.is_none() {
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.muted), egui::StrokeKind::Outside);
            } else if hit == Some(i) && self.tool == Tool::Eraser && matches!(it.kind, Kind::Stroke { .. }) {
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.missing), egui::StrokeKind::Outside);
            }
        }
        if let Some(i) = single {
            let r = screen_rect(self.data.items[i].rect());
            for c in corners(r) {
                let h = Rect::from_center_size(c, vec2(9.0, 9.0));
                painter.rect_filled(h, 0.0, p.bg);
                painter.rect_stroke(h, 0.0, Stroke::new(1.5, p.accent), egui::StrokeKind::Inside);
            }
        } else if self.selection.len() > 1 {
            if let Some(bb) = self.bounds(Some(&self.selection.clone())) {
                painter.rect_stroke(screen_rect(bb).expand(4.0), 0.0, Stroke::new(1.0, p.accent.linear_multiply(0.6)), egui::StrokeKind::Outside);
            }
        }
        // Trait en cours
        if !self.stroke_pts.is_empty() {
            let pts: Vec<Pos2> = self.stroke_pts.iter().map(|w| to_screen(*w)).collect();
            painter.add(Shape::line(pts, Stroke::new((self.pen_width * zoom).max(0.5), self.pen_color)));
        }
        // Lasso
        if let (Some(Drag::Marquee(start)), Some(pt)) = (&self.drag, pointer_in) {
            let r = Rect::from_two_pos(*start, pt);
            painter.rect_filled(r, 0.0, p.accent.linear_multiply(0.08));
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
        }

        // ---------- édition de texte en place ----------
        if let Some(i) = self.editing {
            let r = self.data.items.get(i).map(|it| screen_rect(it.rect()));
            if let (Some(r), Some(Kind::Text { text, size })) = (r, self.data.items.get_mut(i).map(|it| &mut it.kind)) {
                painter.rect_stroke(r.expand(4.0), 0.0, Stroke::new(1.0, p.accent), egui::StrokeKind::Outside);
                let edit = egui::TextEdit::multiline(text)
                    .hint_text("écris ici…")
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .font(egui::FontId::proportional((*size * zoom).max(6.0)))
                    .desired_width(r.width().max(40.0));
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r.intersect(rect)));
                let out = child.add(edit);
                out.request_focus();
                if out.changed() {
                    self.dirty_since = Some(Instant::now());
                }
                // La boîte grandit avec le texte.
                let needed = out.rect.height() / zoom;
                if let Some(it) = self.data.items.get_mut(i) {
                    if needed > it.h {
                        it.h = needed + 4.0;
                    }
                }
                let leave = ui.input(|i| i.key_pressed(Key::Escape)) || (resp.clicked() && hit != Some(i)) || resp.drag_started();
                if leave {
                    self.editing = None;
                }
            }
            if self.editing.is_none() {
                // Un texte laissé vide disparaît.
                if let Some(Kind::Text { text, .. }) = self.data.items.get(i).map(|it| &it.kind) {
                    if text.trim().is_empty() {
                        self.data.items.remove(i);
                        self.selection.clear();
                        self.touch();
                    }
                }
            }
        }

        // ---------- interactions souris ----------
        let pan_drag = resp.dragged_by(PointerButton::Middle) || resp.dragged_by(PointerButton::Secondary) || (space && resp.dragged_by(PointerButton::Primary));
        if pan_drag {
            let d = resp.drag_delta();
            self.data.pan[0] += d.x;
            self.data.pan[1] += d.y;
            self.drag = Some(Drag::Pan);
        } else if resp.drag_started_by(PointerButton::Primary) && self.editing.is_none() {
            let start_screen = ui.input(|i| i.pointer.press_origin()).or(pointer_in).unwrap_or(rect.center());
            let start_world = to_world(start_screen);
            let start_hit = (0..self.data.items.len()).rev().find(|&i| self.hit(&self.data.items[i], start_world, tol));
            let start_handle = single.and_then(|i| {
                let r = screen_rect(self.data.items[i].rect());
                corners(r).iter().position(|c| Rect::from_center_size(*c, vec2(16.0, 16.0)).contains(start_screen))
            });
            self.drag = match self.tool {
                Tool::Select => {
                    if let (Some(i), Some(k)) = (single, start_handle) {
                        self.checkpoint();
                        let r = self.data.items[i].rect();
                        let anchor = corners(r)[(k + 2) % 4];
                        Some(Drag::Resize { i, anchor, start: r })
                    } else if let Some(i) = start_hit {
                        if !self.selection.contains(&i) {
                            if mods.shift {
                                self.selection.push(i);
                            } else {
                                self.selection = vec![i];
                            }
                        }
                        self.checkpoint();
                        // Comme PureRef : l'élément qu'on déplace passe au premier plan.
                        if self.selection.len() == 1 && self.selection[0] + 1 < self.data.items.len() {
                            let it = self.data.items.remove(self.selection[0]);
                            self.data.items.push(it);
                            self.selection = vec![self.data.items.len() - 1];
                        }
                        Some(Drag::Move(self.selection.iter().map(|&j| (j, pos2(self.data.items[j].x, self.data.items[j].y))).collect()))
                    } else {
                        if !mods.shift {
                            self.selection.clear();
                        }
                        Some(Drag::Marquee(start_screen))
                    }
                }
                Tool::Pen => {
                    self.stroke_pts = vec![start_world];
                    Some(Drag::Draw)
                }
                Tool::Eraser => {
                    self.checkpoint();
                    Some(Drag::Erase)
                }
                Tool::Text => None,
            };
        }
        if resp.dragged_by(PointerButton::Primary) && !pan_drag {
            if let (Some(drag), Some(pt)) = (&self.drag, pointer) {
                let w = to_world(pt);
                match drag {
                    Drag::Move(starts) => {
                        let origin_w = to_world(ui.input(|i| i.pointer.press_origin()).unwrap_or(pt));
                        let d = w - origin_w;
                        for (j, s) in starts.clone() {
                            if let Some(it) = self.data.items.get_mut(j) {
                                it.x = s.x + d.x;
                                it.y = s.y + d.y;
                            }
                        }
                        self.dirty_since = Some(Instant::now());
                    }
                    Drag::Resize { i, anchor, start } => {
                        let (i, anchor, start) = (*i, *anchor, *start);
                        if let Some(it) = self.data.items.get_mut(i) {
                            let keep = matches!(it.kind, Kind::Image { .. }) && !mods.shift;
                            let mut dx = w.x - anchor.x;
                            let mut dy = w.y - anchor.y;
                            let min = 20.0;
                            if keep {
                                // Garde les proportions : on suit l'axe qui a le plus bougé.
                                let aspect = start.width() / start.height().max(1.0);
                                let sx = if dx < 0.0 { -1.0 } else { 1.0 };
                                let sy = if dy < 0.0 { -1.0 } else { 1.0 };
                                let width = dx.abs().max(dy.abs() * aspect).max(min);
                                dx = width * sx;
                                dy = width / aspect * sy;
                            } else {
                                if dx.abs() < min {
                                    dx = min * if dx < 0.0 { -1.0 } else { 1.0 };
                                }
                                if dy.abs() < min {
                                    dy = min * if dy < 0.0 { -1.0 } else { 1.0 };
                                }
                            }
                            it.set_rect(Rect::from_two_pos(anchor, anchor + vec2(dx, dy)));
                            self.dirty_since = Some(Instant::now());
                        }
                    }
                    Drag::Draw => {
                        if self.stroke_pts.last().is_none_or(|l| (*l - w).length() * zoom > 1.5) {
                            if mods.shift && !self.stroke_pts.is_empty() {
                                // Maj : ligne droite
                                let a = self.stroke_pts[0];
                                self.stroke_pts = vec![a, w];
                            } else {
                                self.stroke_pts.push(w);
                            }
                        }
                    }
                    Drag::Erase => {
                        if let Some(i) = (0..self.data.items.len()).rev().find(|&i| matches!(self.data.items[i].kind, Kind::Stroke { .. }) && self.hit(&self.data.items[i], w, tol)) {
                            self.data.items.remove(i);
                            self.selection.clear();
                            self.dirty_since = Some(Instant::now());
                        }
                    }
                    Drag::Pan | Drag::Marquee(_) => {}
                }
            }
        }
        if resp.drag_stopped() {
            match self.drag.take() {
                Some(Drag::Marquee(start)) => {
                    if let Some(end) = pointer {
                        let sel = Rect::from_two_pos(to_world(start), to_world(end));
                        for (i, it) in self.data.items.iter().enumerate() {
                            if sel.intersects(it.rect()) && !self.selection.contains(&i) {
                                self.selection.push(i);
                            }
                        }
                    }
                }
                Some(Drag::Draw) => self.finish_stroke(),
                Some(Drag::Pan) => self.touch(),
                Some(Drag::Move(_)) | Some(Drag::Resize { .. }) | Some(Drag::Erase) => {
                    // Rien n'a bougé : on retire le point d'annulation inutile.
                    if self.undo.last() == Some(&self.data.items) {
                        self.undo.pop();
                    }
                }
                None => {}
            }
        }

        // Clics
        if resp.clicked_by(PointerButton::Primary) && self.editing.is_none() {
            match self.tool {
                Tool::Select => match hit {
                    Some(i) if mods.shift => {
                        if let Some(k) = self.selection.iter().position(|&j| j == i) {
                            self.selection.remove(k);
                        } else {
                            self.selection.push(i);
                        }
                    }
                    Some(i) => self.selection = vec![i],
                    None => self.selection.clear(),
                },
                Tool::Text => {
                    if let Some(w) = world_ptr {
                        self.add(Kind::Text { text: String::new(), size: text_size() }, w.to_vec2() + vec2(140.0, 20.0), vec2(280.0, 40.0));
                        self.editing = self.selection.last().copied();
                        self.tool = Tool::Select;
                    }
                }
                Tool::Pen => {
                    // Un simple clic dépose un point.
                    if let Some(w) = world_ptr {
                        self.stroke_pts = vec![w];
                        self.finish_stroke();
                    }
                }
                Tool::Eraser => {
                    if let Some(i) = hit.filter(|&i| matches!(self.data.items[i].kind, Kind::Stroke { .. })) {
                        self.checkpoint();
                        self.data.items.remove(i);
                        self.selection.clear();
                        self.touch();
                    }
                }
            }
        }
        if resp.double_clicked() && self.tool == Tool::Select {
            match hit.and_then(|i| self.data.items.get(i).map(|it| (i, it.kind.clone()))) {
                Some((i, Kind::Text { .. })) => {
                    self.checkpoint();
                    self.editing = Some(i);
                }
                Some((_, Kind::Note { rel })) | Some((_, Kind::Image { rel, .. })) => event = Some(Event::Open(rel)),
                Some((_, Kind::Stroke { .. })) => {}
                None => {
                    if let Some(w) = world_ptr {
                        self.add(Kind::Text { text: String::new(), size: text_size() }, w.to_vec2() + vec2(140.0, 20.0), vec2(280.0, 40.0));
                        self.editing = self.selection.last().copied();
                    }
                }
            }
        }

        // Zoom molette (vers le pointeur)
        if resp.hovered() {
            let (scroll, zd) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = zd * (scroll * 0.0015).exp();
            if factor != 1.0 {
                let new = (zoom * factor).clamp(0.03, 8.0);
                if let Some(pt) = pointer_in {
                    let w = to_world(pt);
                    let new_origin = pt - w.to_vec2() * new;
                    let d = new_origin - rect.center();
                    self.data.pan = [d.x, d.y];
                }
                self.data.zoom = new;
            }
        }

        // ---------- clavier ----------
        let typing = self.editing.is_some() || ui.ctx().egui_wants_keyboard_input();
        if !typing && (resp.hovered() || self.drag.is_some()) {
            let cmd = |k: Key| ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(Modifiers::COMMAND, k)));
            let cmd_shift = |k: Key| ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, k)));
            if cmd_shift(Key::Z) || cmd(Key::Y) {
                self.redo();
            } else if cmd(Key::Z) {
                self.undo();
            }
            if cmd(Key::A) {
                self.selection = (0..self.data.items.len()).collect();
            }
            if cmd(Key::D) {
                self.duplicate_selection();
            }
            let pasted = ui.input(|i| {
                i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)) || matches!(e, egui::Event::Key { key: Key::V, pressed: true, modifiers, .. } if modifiers.command))
            });
            if pasted {
                event = Some(Event::PasteImage(world_ptr.unwrap_or(self.view_center.to_pos2())));
            }
            let (del, esc, v, b, e, t) = ui.input(|i| {
                let plain = !i.modifiers.command && !i.modifiers.alt;
                (
                    i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
                    i.key_pressed(Key::Escape),
                    plain && i.key_pressed(Key::V),
                    plain && i.key_pressed(Key::B),
                    plain && i.key_pressed(Key::E),
                    plain && i.key_pressed(Key::T),
                )
            });
            if del {
                self.delete_selection();
            }
            if esc {
                self.selection.clear();
                self.tool = Tool::Select;
            }
            if v {
                self.tool = Tool::Select;
            }
            if b {
                self.tool = Tool::Pen;
            }
            if e {
                self.tool = Tool::Eraser;
            }
            if t {
                self.tool = Tool::Text;
            }
        }

        // ---------- menu clic droit ----------
        if resp.secondary_clicked() {
            self.menu_at = world_ptr.unwrap_or(self.view_center.to_pos2());
            if let Some(i) = hit {
                if !self.selection.contains(&i) {
                    self.selection = vec![i];
                }
            }
        }
        let menu_at = self.menu_at;
        resp.context_menu(|ui| {
            let n = self.selection.len();
            if n > 0 {
                ui.label(egui::RichText::new(format!("{n} élément(s)")).color(p.muted).size(11.0));
                let first = self.data.items.get(self.selection[0]).cloned();
                if let Some(Item { kind: Kind::Image { rel, .. } | Kind::Note { rel }, .. }) = &first {
                    if ui.button("ouvrir").clicked() {
                        event = Some(Event::Open(rel.clone()));
                        ui.close();
                    }
                }
                if ui.button("premier plan").clicked() {
                    self.reorder(true);
                    ui.close();
                }
                if ui.button("arrière-plan").clicked() {
                    self.reorder(false);
                    ui.close();
                }
                if self.selection.iter().any(|&i| matches!(self.data.items.get(i).map(|x| &x.kind), Some(Kind::Image { .. }))) {
                    if ui.button("retourner horizontalement").clicked() {
                        self.checkpoint();
                        for &i in &self.selection {
                            if let Some(Item { kind: Kind::Image { flip, .. }, .. }) = self.data.items.get_mut(i) {
                                *flip = !*flip;
                            }
                        }
                        self.touch();
                        ui.close();
                    }
                    if ui.button("taille d'origine").clicked() {
                        self.checkpoint();
                        for &i in &self.selection.clone() {
                            if let Some(it) = self.data.items.get_mut(i) {
                                if let Kind::Image { rel, .. } = &it.kind {
                                    if let Some(l) = images.peek(rel, Size::Thumb) {
                                        let c = it.rect().center();
                                        it.set_rect(Rect::from_center_size(c, vec2(l.dims[0] as f32, l.dims[1] as f32)));
                                    }
                                }
                            }
                        }
                        self.touch();
                        ui.close();
                    }
                }
                if self.selection.iter().any(|&i| matches!(self.data.items.get(i).map(|x| &x.kind), Some(Kind::Text { .. }))) {
                    ui.horizontal(|ui| {
                        ui.label("texte");
                        for (label, f) in [("−", 0.8), ("+", 1.25)] {
                            if ui.button(label).clicked() {
                                self.checkpoint();
                                for &i in &self.selection {
                                    if let Some(Item { kind: Kind::Text { size, .. }, .. }) = self.data.items.get_mut(i) {
                                        *size = (*size * f).clamp(8.0, 200.0);
                                    }
                                }
                                self.touch();
                            }
                        }
                    });
                }
                if ui.button("dupliquer  ^D").clicked() {
                    self.duplicate_selection();
                    ui.close();
                }
                if ui.button(egui::RichText::new("supprimer  suppr").color(p.missing)).clicked() {
                    self.delete_selection();
                    ui.close();
                }
                ui.separator();
            }
            if ui.button("coller une image  ^V").clicked() {
                event = Some(Event::PasteImage(menu_at));
                ui.close();
            }
            if ui.button("ajouter du texte").clicked() {
                self.add(Kind::Text { text: String::new(), size: text_size() }, menu_at.to_vec2() + vec2(140.0, 20.0), vec2(280.0, 40.0));
                self.editing = self.selection.last().copied();
                ui.close();
            }
            if ui.button("tout sélectionner  ^A").clicked() {
                self.selection = (0..self.data.items.len()).collect();
                ui.close();
            }
            if ui.button("cadrer").clicked() {
                self.fit();
                ui.close();
            }
        });

        // ---------- glisser-déposer depuis l'explorateur ou les refs ----------
        if let Some(rel) = resp.dnd_release_payload::<String>() {
            if let Some(w) = world_ptr {
                self.drop_file(&rel, w.to_vec2(), images);
            }
        }
        if resp.dnd_hover_payload::<String>().is_some() {
            painter.rect_stroke(rect.shrink(6.0), 0.0, Stroke::new(2.0, p.accent), egui::StrokeKind::Inside);
        }

        // Curseurs
        if let Some(k) = handle_hit {
            ui.ctx().set_cursor_icon(if k % 2 == 0 { egui::CursorIcon::ResizeNwSe } else { egui::CursorIcon::ResizeNeSw });
        } else if pan_drag || (space && resp.hovered()) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if resp.hovered() {
            match self.tool {
                Tool::Pen => ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair),
                Tool::Text => ui.ctx().set_cursor_icon(egui::CursorIcon::Text),
                Tool::Eraser => ui.ctx().set_cursor_icon(egui::CursorIcon::NotAllowed),
                Tool::Select if hit.is_some() => ui.ctx().set_cursor_icon(egui::CursorIcon::Grab),
                Tool::Select => {}
            }
        }

        if self.data.items.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "board vide\n\nglisse des images ou des notes ici, ou colle une image (Ctrl+V)\ndouble-clic : texte · B : dessin · molette : zoom · clic droit / milieu : déplacer la vue",
                egui::FontId::monospace(13.0),
                p.muted,
            );
        }
        let info = format!(
            "{} éléments{} · zoom {:.0} %",
            self.data.items.len(),
            if self.selection.is_empty() { String::new() } else { format!(" · {} sélectionné(s)", self.selection.len()) },
            zoom * 100.0
        );
        painter.text(rect.left_bottom() + vec2(12.0, -10.0), egui::Align2::LEFT_BOTTOM, info, egui::FontId::monospace(11.0), p.muted);
        if self.drag.is_some() || self.editing.is_some() {
            ui.ctx().request_repaint();
        }
        event
    }

    fn finish_stroke(&mut self) {
        let pts = std::mem::take(&mut self.stroke_pts);
        if pts.is_empty() {
            return;
        }
        let mut bb = Rect::NOTHING;
        for p in &pts {
            bb.extend_with(*p);
        }
        // Évite un cadre de taille nulle (trait horizontal, vertical ou point).
        let bb = Rect::from_center_size(bb.center(), vec2(bb.width().max(1.0), bb.height().max(1.0)));
        let points = pts.iter().map(|p| [(p.x - bb.min.x) / bb.width(), (p.y - bb.min.y) / bb.height()]).collect();
        let c = self.pen_color;
        self.checkpoint();
        self.data.items.push(Item {
            kind: Kind::Stroke { points, color: [c.r(), c.g(), c.b(), c.a()], width: self.pen_width },
            x: bb.min.x,
            y: bb.min.y,
            w: bb.width(),
            h: bb.height(),
        });
        self.touch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_json_et_historique() {
        let mut b = Board::default();
        b.add(Kind::Text { text: "salut".into(), size: 18.0 }, vec2(0.0, 0.0), vec2(100.0, 50.0));
        b.add_grid(vec![("refs/a.png".into(), 1.5), ("refs/b.png".into(), 0.5)]);
        b.stroke_pts = vec![pos2(0.0, 0.0), pos2(10.0, 0.0)];
        b.finish_stroke();
        assert_eq!(b.data.items.len(), 4);
        let json = serde_json::to_string(&b.data).unwrap();
        assert!(json.contains(r#""type":"image""#) && json.contains(r#""type":"stroke""#));
        let back: BoardFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.items.len(), 4);
        // Anciens boards (v0.5) sans « flip » ni « size »
        let old: BoardFile = serde_json::from_str(r#"{"items":[{"type":"image","rel":"a.png","x":0,"y":0,"w":1,"h":1},{"type":"text","text":"t","x":0,"y":0,"w":1,"h":1}]}"#).unwrap();
        assert_eq!(old.items.len(), 2);
        // Annuler / rétablir
        b.selection = vec![0];
        b.delete_selection();
        assert_eq!(b.data.items.len(), 3);
        b.undo();
        assert_eq!(b.data.items.len(), 4);
        b.redo();
        assert_eq!(b.data.items.len(), 3);
        // Premier plan
        b.selection = vec![0];
        b.reorder(true);
        assert_eq!(b.selection, vec![2]);
        // Le trait horizontal se teste sur son tracé
        let s = b.data.items.iter().find(|i| matches!(i.kind, Kind::Stroke { .. })).unwrap().clone();
        assert!(b.hit(&s, pos2(5.0, 0.5), 2.0));
        assert!(!b.hit(&s, pos2(5.0, 30.0), 2.0));
        assert!(is_board("boards/Forêt.board.json"));
        assert_eq!(board_name("boards/Forêt.board.json"), "Forêt");
    }
}
