//! Boards : canevas infini façon PureRef. Un board = un fichier `.board.json` du coffre.
use crate::images::{Images, Size, Slot};
use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Kind {
    Image { rel: String },
    Text { text: String },
    Note { rel: String },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Item {
    #[serde(flatten)]
    pub kind: Kind,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
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

enum Drag {
    Move(usize),
    Resize(usize),
    Pan,
}

pub enum Event {
    Open(String),
}

#[derive(Default)]
pub struct Board {
    pub rel: Option<String>,
    pub data: BoardFile,
    pub selected: Option<usize>,
    editing: Option<usize>,
    drag: Option<Drag>,
    pub dirty_since: Option<Instant>,
    /// Centre de la vue en coordonnées du board (pour y ajouter des éléments).
    pub view_center: Vec2,
    fit_next: bool,
}

pub fn is_board(rel: &str) -> bool {
    rel.to_lowercase().ends_with(".board.json")
}

pub fn board_name(rel: &str) -> String {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.strip_suffix(".board.json").unwrap_or(name).to_string()
}

impl Board {
    pub fn load(root: &str, rel: &str) -> Result<Self, String> {
        let text = crate::vault::read(root, rel)?;
        let data: BoardFile = if text.trim().is_empty() {
            BoardFile::default()
        } else {
            serde_json::from_str(&text).map_err(|e| format!("Board illisible ({rel}) : {e}"))?
        };
        Ok(Self { rel: Some(rel.to_string()), data, fit_next: false, ..Default::default() })
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

    /// Ajoute un élément centré sur `at` (coordonnées du board).
    pub fn add(&mut self, kind: Kind, at: Vec2, size: Vec2) {
        self.data.items.push(Item { kind, x: at.x - size.x / 2.0, y: at.y - size.y / 2.0, w: size.x, h: size.y });
        self.selected = Some(self.data.items.len() - 1);
        self.touch();
    }

    /// Dispose des images en rangées, à droite de ce qui existe déjà.
    pub fn add_grid(&mut self, images: Vec<(String, f32)>) {
        if images.is_empty() {
            return;
        }
        let start_x = self.data.items.iter().map(|i| i.x + i.w).fold(f32::MIN, f32::max);
        let start_x = if start_x == f32::MIN { 0.0 } else { start_x + 120.0 };
        let top = self.data.items.iter().map(|i| i.y).fold(f32::MAX, f32::min);
        let top = if top == f32::MAX { 0.0 } else { top };
        let row_h = 240.0;
        let max_w = 1400.0;
        let (mut x, mut y) = (start_x, top);
        for (rel, aspect) in images {
            let w = row_h * aspect.clamp(0.3, 3.0);
            if x > start_x && x + w > start_x + max_w {
                x = start_x;
                y += row_h + 16.0;
            }
            self.data.items.push(Item { kind: Kind::Image { rel }, x, y, w, h: row_h });
            x += w + 16.0;
        }
        self.fit_next = true;
        self.touch();
    }

    pub fn edit_selected(&mut self) {
        self.editing = self.selected;
    }

    pub fn fit(&mut self) {
        self.fit_next = true;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, images: &mut Images, md_titles: &dyn Fn(&str) -> String) -> Option<Event> {
        let p = theme::pal();
        let mut event = None;
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, p.panel);

        if self.fit_next && rect.width() > 10.0 {
            self.fit_next = false;
            if let Some(bb) = self.bounds() {
                let z = ((rect.width() - 80.0) / bb.width().max(1.0)).min((rect.height() - 80.0) / bb.height().max(1.0)).clamp(0.05, 2.0);
                self.data.zoom = z;
                let c = bb.center();
                self.data.pan = [-c.x * z, -c.y * z];
            }
        }

        let zoom = self.data.zoom;
        let origin = rect.center() + Vec2::from(self.data.pan);
        let to_screen = |x: f32, y: f32| origin + vec2(x, y) * zoom;
        let to_world = |pt: Pos2| (pt - origin) / zoom;
        self.view_center = to_world(rect.center());

        // Grille
        let step = 40.0 * zoom;
        if step > 10.0 {
            let mut gx = rect.left() + (origin.x - rect.left()).rem_euclid(step);
            while gx < rect.right() {
                painter.line_segment([pos2(gx, rect.top()), pos2(gx, rect.bottom())], Stroke::new(1.0, p.line.linear_multiply(0.35)));
                gx += step;
            }
            let mut gy = rect.top() + (origin.y - rect.top()).rem_euclid(step);
            while gy < rect.bottom() {
                painter.line_segment([pos2(rect.left(), gy), pos2(rect.right(), gy)], Stroke::new(1.0, p.line.linear_multiply(0.35)));
                gy += step;
            }
        }

        let item_rect = |it: &Item| Rect::from_min_max(to_screen(it.x, it.y), to_screen(it.x + it.w, it.y + it.h));
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let hit = pointer.filter(|pt| rect.contains(*pt)).and_then(|pt| self.data.items.iter().rposition(|it| item_rect(it).contains(pt)));
        let handle = |r: Rect| Rect::from_center_size(r.right_bottom(), vec2(12.0, 12.0));
        let on_handle = self.selected.zip(pointer).is_some_and(|(i, pt)| self.data.items.get(i).is_some_and(|it| handle(item_rect(it)).contains(pt)));

        // Dessin des éléments
        for (i, it) in self.data.items.iter().enumerate() {
            let r = item_rect(it);
            if !r.intersects(rect) {
                continue;
            }
            match &it.kind {
                Kind::Image { rel } => {
                    let size = if r.width().max(r.height()) > 360.0 { Size::Full } else { Size::Thumb };
                    let slot = match images.get(rel, size) {
                        Slot::Loading if size == Size::Full => match images.peek(rel, Size::Thumb) {
                            Some(t) => Slot::Ready(t),
                            None => Slot::Loading,
                        },
                        s => s,
                    };
                    match slot {
                        Slot::Ready(l) => {
                            painter.image(l.tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                        }
                        Slot::Loading => {
                            painter.rect_filled(r, 0.0, p.line);
                        }
                        Slot::Failed(e) => {
                            painter.rect_filled(r, 0.0, p.bg);
                            painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.missing), egui::StrokeKind::Inside);
                            painter.text(r.center(), egui::Align2::CENTER_CENTER, format!("{rel}\n{e}"), egui::FontId::monospace(11.0), p.missing);
                        }
                    }
                }
                Kind::Text { text } => {
                    painter.rect_filled(r, 0.0, p.bg);
                    painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
                    painter.rect_filled(Rect::from_min_size(r.min, vec2(3.0 * zoom.max(0.5), r.height())), 0.0, p.accent);
                    if self.editing != Some(i) {
                        let font = egui::FontId::monospace((14.0 * zoom).max(4.0));
                        let galley = painter.layout(text.clone(), font, p.text, (r.width() - 20.0 * zoom).max(10.0));
                        painter.with_clip_rect(r).galley(r.min + vec2(12.0, 10.0) * zoom, galley, p.text);
                    }
                }
                Kind::Note { rel } => {
                    painter.rect_filled(r, 0.0, p.bg);
                    painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.link), egui::StrokeKind::Inside);
                    let title = egui::FontId::new((15.0 * zoom).max(4.0), theme::bold());
                    painter.with_clip_rect(r).text(r.min + vec2(12.0, 12.0) * zoom, egui::Align2::LEFT_TOP, format!("# {}", crate::vault::stem(rel)), title, p.link);
                    let body = painter.layout(md_titles(rel), egui::FontId::monospace((12.0 * zoom).max(4.0)), p.muted, (r.width() - 24.0 * zoom).max(10.0));
                    painter.with_clip_rect(r.shrink(2.0)).galley(r.min + vec2(12.0, 38.0) * zoom, body, p.muted);
                }
            }
            if self.selected == Some(i) {
                painter.rect_stroke(r, 0.0, Stroke::new(2.0, p.accent), egui::StrokeKind::Outside);
                painter.rect_filled(handle(r), 0.0, p.accent);
            } else if hit == Some(i) {
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.muted), egui::StrokeKind::Outside);
            }
        }

        // Édition de texte en place
        if let Some(i) = self.editing {
            if let Some(it) = self.data.items.get_mut(i) {
                let r = item_rect(it);
                if let Kind::Text { text } = &mut it.kind {
                    let edit = egui::TextEdit::multiline(text).hint_text("écris ici…").frame(egui::Frame::NONE).font(egui::FontId::monospace((14.0 * zoom).max(6.0))).desired_width(r.width() - 20.0 * zoom);
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink(8.0 * zoom)));
                    let out = child.add(edit);
                    out.request_focus();
                    if out.changed() {
                        self.dirty_since = Some(Instant::now());
                    }
                    if ui.input(|i| i.key_pressed(Key::Escape)) || (resp.clicked() && hit != Some(i)) {
                        self.editing = None;
                    }
                }
            }
            // Un texte laissé vide disparaît.
            if self.editing.is_none() {
                if let Some(Kind::Text { text }) = self.data.items.get(i).map(|it| &it.kind) {
                    if text.trim().is_empty() {
                        self.data.items.remove(i);
                        self.selected = None;
                        self.dirty_since = Some(Instant::now());
                    }
                }
            }
        }

        // Interactions
        if resp.drag_started() {
            self.editing = None;
            self.drag = Some(if on_handle {
                Drag::Resize(self.selected.unwrap())
            } else if let Some(i) = hit {
                self.selected = Some(i);
                Drag::Move(i)
            } else {
                Drag::Pan
            });
        }
        if resp.dragged() {
            let d = resp.drag_delta();
            match self.drag {
                Some(Drag::Move(i)) => {
                    if let Some(it) = self.data.items.get_mut(i) {
                        it.x += d.x / zoom;
                        it.y += d.y / zoom;
                        self.dirty_since = Some(Instant::now());
                    }
                }
                Some(Drag::Resize(i)) => {
                    if let Some(it) = self.data.items.get_mut(i) {
                        let keep_aspect = matches!(it.kind, Kind::Image { .. });
                        let aspect = it.w / it.h.max(1.0);
                        it.w = (it.w + d.x / zoom).max(40.0);
                        it.h = if keep_aspect { it.w / aspect } else { (it.h + d.y / zoom).max(30.0) };
                        self.dirty_since = Some(Instant::now());
                    }
                }
                Some(Drag::Pan) => {
                    self.data.pan[0] += d.x;
                    self.data.pan[1] += d.y;
                }
                None => {}
            }
        }
        if resp.drag_stopped() {
            if matches!(self.drag, Some(Drag::Pan)) {
                self.dirty_since = Some(Instant::now());
            }
            self.drag = None;
        }
        if resp.clicked() {
            self.selected = hit;
            if hit.is_none() {
                self.editing = None;
            }
        }
        if resp.double_clicked() {
            match hit.and_then(|i| self.data.items.get(i).map(|it| (i, it.kind.clone()))) {
                Some((i, Kind::Text { .. })) => self.editing = Some(i),
                Some((_, Kind::Note { rel })) | Some((_, Kind::Image { rel })) => event = Some(Event::Open(rel)),
                None => {
                    // Double-clic dans le vide : nouvelle note texte
                    if let Some(pt) = pointer {
                        let at = to_world(pt);
                        self.add(Kind::Text { text: String::new() }, at, vec2(260.0, 120.0));
                        self.editing = self.selected;
                    }
                }
            }
        }
        if resp.hovered() {
            let (scroll, zd) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = zd * (scroll * 0.0015).exp();
            if factor != 1.0 {
                let new = (zoom * factor).clamp(0.03, 6.0);
                if let Some(pt) = pointer {
                    let world = to_world(pt);
                    let new_origin = pt - world * new;
                    let base = rect.center();
                    self.data.pan = [(new_origin - base).x, (new_origin - base).y];
                }
                self.data.zoom = new;
            }
        }
        if self.editing.is_none() && resp.hovered() {
            let (del, dup) = ui.input_mut(|i| {
                (
                    i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
                    i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, Key::D)),
                )
            });
            if let Some(i) = self.selected {
                if del && i < self.data.items.len() {
                    self.data.items.remove(i);
                    self.selected = None;
                    self.dirty_since = Some(Instant::now());
                } else if dup {
                    if let Some(mut it) = self.data.items.get(i).cloned() {
                        it.x += 30.0;
                        it.y += 30.0;
                        self.data.items.push(it);
                        self.selected = Some(self.data.items.len() - 1);
                        self.dirty_since = Some(Instant::now());
                    }
                }
            }
        }
        if on_handle {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
        } else if hit.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }

        // Dépôt depuis l'explorateur (glisser une image ou une note sur le board)
        if let Some(rel) = resp.dnd_release_payload::<String>() {
            if let Some(pt) = pointer {
                self.drop_file(&rel, to_world(pt), images);
            }
        }
        if resp.dnd_hover_payload::<String>().is_some() {
            painter.rect_stroke(rect.shrink(6.0), 0.0, Stroke::new(2.0, p.accent), egui::StrokeKind::Inside);
        }

        if self.data.items.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "board vide\n\nglisse des images ou des notes depuis l'explorateur ou ton bureau\ndouble-clic : texte · molette : zoom · glisser le fond : déplacer",
                egui::FontId::monospace(13.0),
                p.muted,
            );
        }
        let info = format!("{} éléments · zoom {:.0} %   ·   suppr : retirer   ^D : dupliquer   double-clic : ouvrir / éditer", self.data.items.len(), zoom * 100.0);
        painter.text(rect.left_bottom() + vec2(12.0, -10.0), egui::Align2::LEFT_BOTTOM, info, egui::FontId::monospace(11.0), p.muted);
        event
    }

    pub fn drop_file(&mut self, rel: &str, at: Vec2, images: &mut Images) {
        if crate::images::is_image(rel) {
            let aspect = images.peek(rel, Size::Thumb).map(|l| l.dims[0] as f32 / l.dims[1].max(1) as f32).unwrap_or(1.5);
            let h = 260.0;
            self.add(Kind::Image { rel: rel.to_string() }, at, vec2(h * aspect, h));
        } else if crate::vault::is_md(rel) {
            self.add(Kind::Note { rel: rel.to_string() }, at, vec2(300.0, 180.0));
        } else {
            self.add(Kind::Text { text: format!("`{rel}`") }, at, vec2(260.0, 60.0));
        }
    }

    fn bounds(&self) -> Option<Rect> {
        let mut bb = Rect::NOTHING;
        for it in &self.data.items {
            bb = bb.union(Rect::from_min_size(pos2(it.x, it.y), vec2(it.w, it.h)));
        }
        (!self.data.items.is_empty()).then_some(bb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_json() {
        let mut b = Board::default();
        b.add(Kind::Text { text: "salut".into() }, vec2(0.0, 0.0), vec2(100.0, 50.0));
        b.add_grid(vec![("refs/a.png".into(), 1.5), ("refs/b.png".into(), 0.5)]);
        let json = serde_json::to_string(&b.data).unwrap();
        assert!(json.contains(r#""type":"image""#));
        let back: BoardFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.items.len(), 3);
        assert!(is_board("boards/Forêt.board.json"));
        assert_eq!(board_name("boards/Forêt.board.json"), "Forêt");
    }
}
