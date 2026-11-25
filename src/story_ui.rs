//! Éditeur de storyboard : la case dessinée au centre, ses infos à droite, la bande des cases en bas,
//! et l'animatique qui se lit sur place.

use crate::story::{self, Story, Tool};
use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Key, Pos2, Rect, RichText, Stroke};
use resvg::tiny_skia::Pixmap;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

enum Undo {
    Pixels(u64, Vec<u8>),
    Story(Story, usize),
}

pub struct Editor {
    path: PathBuf,
    story: Story,
    cur: usize,
    tool: Tool,
    color: [u8; 4],
    size: f32,
    onion: bool,
    /// Dessins chargés (les plus récents), et leur ordre d'usage
    pix: HashMap<u64, Pixmap>,
    used: Vec<u64>,
    tex: Option<(u64, u64, egui::TextureHandle)>,
    onion_tex: Option<(u64, u64, egui::TextureHandle)>,
    thumbs: HashMap<u64, (u64, egui::TextureHandle)>,
    /// Version de chaque dessin, pour savoir quand refaire sa texture
    ver: HashMap<u64, u64>,
    /// Trait en cours : l'outil choisi à son départ, et ses points
    stroke: Option<(Tool, Vec<(f32, f32, f32)>)>,
    undo: Vec<Undo>,
    redo: Vec<Undo>,
    save_due: Option<Instant>,
    playing: Option<(Instant, f32)>,
    /// Caméra 2D : on règle ses cadres sur la case (le dessin est alors en pause) ; cadre saisi (0 départ, 1 arrivée)
    cam_edit: bool,
    cam_drag: Option<(usize, [f32; 2], Pos2)>,
    message: Option<String>,
    /// Fichier à ouvrir ensuite (le montage créé)
    pub open_request: Option<PathBuf>,
}

impl Editor {
    pub fn open(path: PathBuf) -> Result<Editor, String> {
        let story = Story::read(&path)?;
        Ok(Editor { path, story, cur: 0, tool: Tool::Pencil, color: [30, 30, 36, 255], size: 4.0, onion: false, pix: HashMap::new(), used: vec![], tex: None, onion_tex: None, thumbs: HashMap::new(), ver: HashMap::new(), stroke: None, undo: vec![], redo: vec![], save_due: None, playing: None, cam_edit: false, cam_drag: None, message: None, open_request: None })
    }

    pub fn save_if_due(&mut self, force: bool) {
        if let Some(t) = self.save_due {
            if force || t.elapsed().as_millis() > 600 {
                self.save_due = None;
                if let Err(e) = self.story.save(&self.path) {
                    self.message = Some(e);
                }
            }
        }
    }

    fn touch(&mut self) {
        self.save_due = Some(Instant::now());
    }

    fn story_checkpoint(&mut self) {
        self.undo.push(Undo::Story(self.story.clone(), self.cur));
        self.redo.clear();
        if self.undo.len() > 60 {
            self.undo.remove(0);
        }
    }

    /// Le dessin d'une case, chargé au besoin (seize gardés en mémoire).
    fn pixmap(&mut self, id: u64) -> &mut Pixmap {
        if !self.pix.contains_key(&id) {
            let pm = story::load(&self.path, id, self.story.width, self.story.height);
            self.pix.insert(id, pm);
        }
        self.used.retain(|x| *x != id);
        self.used.push(id);
        while self.used.len() > 16 {
            let old = self.used.remove(0);
            self.pix.remove(&old);
        }
        self.pix.get_mut(&id).unwrap()
    }

    fn bump(&mut self, id: u64) {
        *self.ver.entry(id).or_insert(0) += 1;
    }

    fn texture(ctx: &egui::Context, name: &str, pm: &Pixmap, max_w: u32) -> egui::TextureHandle {
        let (w, h) = (pm.width(), pm.height());
        let img = if w > max_w {
            let s = max_w as f32 / w as f32;
            let small = pm.clone();
            let (nw, nh) = (max_w, ((h as f32 * s) as u32).max(1));
            let mut out = Pixmap::new(nw, nh).unwrap();
            out.draw_pixmap(0, 0, small.as_ref(), &resvg::tiny_skia::PixmapPaint { quality: resvg::tiny_skia::FilterQuality::Bilinear, ..Default::default() }, resvg::tiny_skia::Transform::from_scale(s, s), None);
            out
        } else {
            pm.clone()
        };
        let ci = egui::ColorImage::from_rgba_premultiplied([img.width() as usize, img.height() as usize], img.data());
        ctx.load_texture(name, ci, egui::TextureOptions::LINEAR)
    }

    fn save_panel(&mut self, id: u64) {
        if let Some(pm) = self.pix.get(&id) {
            if let Err(e) = story::save_panel(&self.path, id, pm) {
                self.message = Some(e);
            }
        }
    }

    fn undo_step(&mut self, redo: bool) {
        let item = if redo { self.redo.pop() } else { self.undo.pop() };
        let Some(item) = item else { return };
        let back = match item {
            Undo::Pixels(id, data) => {
                let pm = self.pixmap(id);
                let old = pm.data().to_vec();
                if data.len() == pm.data().len() {
                    pm.data_mut().copy_from_slice(&data);
                }
                self.bump(id);
                self.save_panel(id);
                if let Some(i) = self.story.panels.iter().position(|p| p.id == id) {
                    self.cur = i;
                }
                Undo::Pixels(id, old)
            }
            Undo::Story(s, cur) => {
                let old = Undo::Story(std::mem::replace(&mut self.story, s), self.cur);
                self.cur = cur.min(self.story.panels.len().saturating_sub(1));
                self.touch();
                old
            }
        };
        if redo {
            self.undo.push(back);
        } else {
            self.redo.push(back);
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        if self.story.panels.is_empty() {
            self.story.add(None);
        }
        self.cur = self.cur.min(self.story.panels.len() - 1);
        let typing = ui.ctx().egui_wants_keyboard_input();
        // Écrire dans une case arrête la lecture : le texte va à la case qu'on voit
        if typing {
            self.playing = None;
        }
        // ---- Lecture de l'animatique
        if let Some((t0, from)) = self.playing {
            let t = from + t0.elapsed().as_secs_f32();
            if t >= self.story.total() {
                self.playing = None;
            } else {
                self.cur = self.story.at(t);
                ui.ctx().request_repaint();
            }
        }
        // ---- Raccourcis
        if !typing {
            let (cmd, shift, alt) = ui.input(|i| (i.modifiers.command, i.modifiers.shift, i.modifiers.alt));
            let key = |k: Key| ui.input(|i| i.key_pressed(k));
            for (t, k) in [(Tool::Pencil, Key::P), (Tool::Ink, Key::I), (Tool::Tone, Key::T), (Tool::Fill, Key::F), (Tool::Eraser, Key::E)] {
                if !cmd && key(k) {
                    self.tool = t;
                }
            }
            if !cmd && key(Key::O) {
                self.onion = !self.onion;
            }
            if key(Key::Space) {
                self.toggle_play();
            }
            if cmd && key(Key::Z) {
                self.undo_step(shift);
            }
            if cmd && key(Key::Y) {
                self.undo_step(true);
            }
            if key(Key::OpenBracket) {
                self.size = (self.size / 1.25).max(1.0);
            }
            if key(Key::CloseBracket) {
                self.size = (self.size * 1.25).min(200.0);
            }
            if alt && key(Key::ArrowLeft) {
                self.move_panel(-1);
            } else if alt && key(Key::ArrowRight) {
                self.move_panel(1);
            } else if key(Key::ArrowLeft) && self.cur > 0 {
                self.cur -= 1;
                self.playing = None;
            } else if key(Key::ArrowRight) && self.cur + 1 < self.story.panels.len() {
                self.cur += 1;
                self.playing = None;
            }
            if !cmd && key(Key::N) {
                self.story_checkpoint();
                self.cur = self.story.add(Some(self.cur));
                self.touch();
            }
            if cmd && key(Key::D) {
                self.duplicate();
            }
        }
        // ---- Barre d'outils
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let tools = [(Tool::Pencil, crate::lucide::PENCIL, "crayon (P)"), (Tool::Ink, crate::lucide::PEN_TOOL, "encre (I)"), (Tool::Tone, crate::lucide::BRUSH, "ton, gris léger (T)"), (Tool::Fill, crate::lucide::LASSO, "aplat : entourer une zone pour la remplir (F)"), (Tool::Eraser, crate::lucide::ERASER, "gomme (E)")];
            for (t, icon, tip) in tools {
                let on = self.tool == t;
                if ui.add(egui::Button::new(RichText::new(icon).size(15.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { Color32::TRANSPARENT }).min_size(vec2(28.0, 26.0))).on_hover_text(crate::i18n::t(tip)).clicked() {
                    self.tool = t;
                }
            }
            ui.separator();
            ui.add(egui::DragValue::new(&mut self.size).range(1.0..=200.0).speed(0.3).suffix(" px")).on_hover_text(crate::i18n::t("taille ([ et ])"));
            for c in [[30, 30, 36, 255], [120, 120, 128, 255], [90, 150, 230, 255], [220, 70, 60, 255], [255, 255, 255, 255]] {
                let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::click());
                ui.painter().circle_filled(r.center(), 7.0, Color32::from_rgb(c[0], c[1], c[2]));
                if self.color == c {
                    ui.painter().circle_stroke(r.center(), 8.5, Stroke::new(1.5, p.accent));
                } else {
                    ui.painter().circle_stroke(r.center(), 7.0, Stroke::new(1.0, p.line));
                }
                if resp.clicked() {
                    self.color = c;
                }
            }
            let mut rgb = [self.color[0], self.color[1], self.color[2]];
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                self.color = [rgb[0], rgb[1], rgb[2], 255];
            }
            ui.separator();
            let on = self.onion;
            if ui.add(egui::Button::new(RichText::new(crate::lucide::LAYERS).size(15.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { Color32::TRANSPARENT }).min_size(vec2(28.0, 26.0))).on_hover_text(crate::i18n::t("pelure d'oignon : la case précédente en transparence (O)")).clicked() {
                self.onion = !self.onion;
            }
            if ui.add(egui::Button::new(RichText::new(crate::lucide::UNDO_2).size(15.0)).frame(false)).on_hover_text(crate::i18n::t("annuler (Ctrl+Z)")).clicked() {
                self.undo_step(false);
            }
            if ui.add(egui::Button::new(RichText::new(crate::lucide::REDO_2).size(15.0)).frame(false)).on_hover_text(crate::i18n::t("rétablir (Ctrl+Maj+Z)")).clicked() {
                self.undo_step(true);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::containers::menu::MenuButton::new(RichText::new(crate::lucide::DOWNLOAD).size(15.0)).ui(ui, |ui| {
                    ui.set_min_width(240.0);
                    if ui.button(format!("{}  {}", crate::lucide::FILM, crate::i18n::t("animatique en MP4…"))).clicked() {
                        self.export(0);
                        ui.close();
                    }
                    if ui.button(format!("{}  {}", crate::lucide::IMAGE, crate::i18n::t("planche en PNG…"))).clicked() {
                        self.export(1);
                        ui.close();
                    }
                    if ui.button(format!("{}  {}", crate::lucide::CLAPPERBOARD, crate::i18n::t("envoyer au montage"))).on_hover_text(crate::i18n::t("Crée un montage avec une image par case, à leur durée, et un repère à chaque scène")).clicked() {
                        self.export(2);
                        ui.close();
                    }
                })
                .0
                .on_hover_text(crate::i18n::t("exporter"));
                let aspect = story::ASPECTS.iter().find(|a| a.1 == self.story.width && a.2 == self.story.height).map(|a| a.0).unwrap_or("?");
                egui::ComboBox::from_id_salt("story-aspect").selected_text(aspect).width(90.0).show_ui(ui, |ui| {
                    for (name, w, h) in story::ASPECTS {
                        if ui.selectable_label(self.story.width == w && self.story.height == h, name).clicked() && (self.story.width, self.story.height) != (w, h) {
                            self.story_checkpoint();
                            self.story.width = w;
                            self.story.height = h;
                            // Les dessins se rechargent à la nouvelle taille
                            self.pix.clear();
                            self.used.clear();
                            self.tex = None;
                            self.onion_tex = None;
                            self.thumbs.clear();
                            self.touch();
                        }
                    }
                })
                .response
                .on_hover_text(crate::i18n::t("format de l'image"));
                if let Some(m) = &self.message {
                    ui.label(RichText::new(m).size(11.0).color(p.muted));
                }
            });
        });
        ui.add_space(4.0);
        // ---- Zones : bande des cases en bas, infos à droite, la case au centre
        let full = ui.available_rect_before_wrap();
        let strip_h = 132.0;
        let side_w = 270.0;
        let strip = Rect::from_min_max(pos2(full.left(), full.bottom() - strip_h), full.max);
        let side = Rect::from_min_max(pos2(full.right() - side_w, full.top()), pos2(full.right(), strip.top() - 6.0));
        let stage = Rect::from_min_max(full.min, pos2(side.left() - 8.0, strip.top() - 6.0));
        ui.allocate_rect(full, egui::Sense::hover());
        self.canvas(ui, stage);
        self.side(ui, side);
        self.strip(ui, strip);
    }

    fn toggle_play(&mut self) {
        if self.playing.is_some() {
            self.playing = None;
        } else {
            let from = self.story.starts().get(self.cur).copied().unwrap_or(0.0);
            self.playing = Some((Instant::now(), from));
        }
    }

    fn move_panel(&mut self, d: i32) {
        let to = self.cur as i32 + d;
        if to < 0 || to as usize >= self.story.panels.len() {
            return;
        }
        self.story_checkpoint();
        self.story.panels.swap(self.cur, to as usize);
        self.cur = to as usize;
        self.touch();
    }

    fn duplicate(&mut self) {
        self.story_checkpoint();
        let src = self.story.panels[self.cur].clone();
        let at = self.story.add(Some(self.cur));
        let id = self.story.panels[at].id;
        self.story.panels[at] = story::Panel { id, ..src.clone() };
        let pm = self.pixmap(src.id).clone();
        self.pix.insert(id, pm);
        self.save_panel(id);
        self.cur = at;
        self.touch();
    }

    fn export(&mut self, what: u8) {
        self.save_if_due(true);
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let dir = self.path.parent().map(|d| d.to_path_buf()).unwrap_or_default();
        let r = match what {
            0 => rfd::FileDialog::new().set_directory(&dir).set_file_name(format!("{stem} animatique.mp4")).add_filter("MP4", &["mp4"]).save_file().map(|f| story::export_mp4(&self.path, &self.story, &f).map(|_| f)),
            1 => rfd::FileDialog::new().set_directory(&dir).set_file_name(format!("{stem} planche.png")).add_filter("PNG", &["png"]).save_file().map(|f| story::sheet(&self.path, &self.story, &f).map(|_| f)),
            _ => {
                let mut f = dir.join(format!("{stem} animatique.{}", crate::formats::ext("seq")));
                let mut n = 2;
                while f.exists() {
                    f = dir.join(format!("{stem} animatique {n}.{}", crate::formats::ext("seq")));
                    n += 1;
                }
                Some(story::to_montage(&self.path, &self.story, &f).map(|_| f))
            }
        };
        if let (2, Some(Ok(f))) = (what, &r) {
            self.open_request = Some(f.clone());
        }
        self.message = match r {
            Some(Ok(f)) => Some(format!("{} {}", crate::i18n::t("exporté :"), f.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default())),
            Some(Err(e)) => Some(e),
            None => None,
        };
    }

    /// La case au centre : dessin, pelure d'oignon, pinceau.
    fn canvas(&mut self, ui: &mut egui::Ui, area: Rect) {
        let p = theme::pal();
        let (w, h) = (self.story.width as f32, self.story.height as f32);
        let s = ((area.width() - 16.0) / w).min((area.height() - 16.0) / h).max(0.05);
        let rect = Rect::from_center_size(area.center(), vec2(w * s, h * s));
        let painter = ui.painter_at(area);
        painter.rect_filled(area, 4.0, p.raised);
        painter.rect_filled(rect.translate(vec2(0.0, 2.0)), 0.0, Color32::from_black_alpha(60));
        painter.rect_filled(rect, 0.0, Color32::WHITE);
        let id = self.story.panels[self.cur].id;
        // Pelure d'oignon : la case précédente
        if self.onion && self.cur > 0 && self.playing.is_none() {
            let pid = self.story.panels[self.cur - 1].id;
            let v = self.ver.get(&pid).copied().unwrap_or(0);
            if self.onion_tex.as_ref().is_none_or(|t| t.0 != pid || t.1 != v) {
                let pm = self.pixmap(pid).clone();
                self.onion_tex = Some((pid, v, Self::texture(ui.ctx(), "story-onion", &pm, 1600)));
            }
            if let Some((_, _, t)) = &self.onion_tex {
                painter.image(t.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::from_rgba_unmultiplied(255, 120, 120, 70));
            }
        }
        let v = self.ver.get(&id).copied().unwrap_or(0);
        if self.tex.as_ref().is_none_or(|t| t.0 != id || t.1 != v) {
            let pm = self.pixmap(id).clone();
            self.tex = Some((id, v, Self::texture(ui.ctx(), "story-canvas", &pm, 4096)));
        }
        // En lecture, la caméra 2D cadre une partie du dessin
        let uv = match (self.playing, &self.story.panels[self.cur].cam) {
            (Some((t0, from)), Some(cam)) => {
                let t = from + t0.elapsed().as_secs_f32();
                let start = self.story.starts()[self.cur];
                let r = story::cam_rect(cam, (t - start) / self.story.panels[self.cur].dur.max(0.05));
                Rect::from_min_max(pos2(r[0], r[1]), pos2(r[2], r[3]))
            }
            _ => Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        };
        if let Some((_, _, t)) = &self.tex {
            painter.image(t.id(), rect, uv, Color32::WHITE);
        }
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, p.line), egui::StrokeKind::Outside);
        // Réglage des cadres de la caméra 2D : glisser un cadre pour le déplacer, molette dessus pour zoomer
        if self.cam_edit && self.playing.is_none() {
            if let Some(mut cam) = self.story.panels[self.cur].cam {
                let resp = ui.interact(rect, egui::Id::unique(("story-cam", &self.path)), egui::Sense::drag());
                let frame_of = |c: [f32; 3]| {
                    let r = story::cam_rect(&[c, c], 0.0);
                    Rect::from_min_max(pos2(rect.left() + r[0] * rect.width(), rect.top() + r[1] * rect.height()), pos2(rect.left() + r[2] * rect.width(), rect.top() + r[3] * rect.height()))
                };
                let cols = [Color32::from_rgb(70, 170, 120), Color32::from_rgb(230, 110, 90)];
                // Le cadre le plus petit d'abord : il reste saisissable quand l'autre l'entoure
                let order = if cam[1][2] >= cam[0][2] { [1usize, 0] } else { [0, 1] };
                if resp.drag_started() {
                    let at = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()).unwrap_or_default();
                    let k = order.into_iter().find(|&k| frame_of(cam[k]).contains(at)).unwrap_or(order[0]);
                    self.cam_drag = Some((k, [cam[k][0], cam[k][1]], at));
                }
                let mut changed = false;
                // Depuis le point de départ du glisser (pas d'à-coup si la souris saute)
                if let (Some((k, c0, at0)), true, Some(now)) = (self.cam_drag, resp.dragged(), resp.interact_pointer_pos()) {
                    cam[k][0] = (c0[0] + (now.x - at0.x) / rect.width()).clamp(0.0, 1.0);
                    cam[k][1] = (c0[1] + (now.y - at0.y) / rect.height()).clamp(0.0, 1.0);
                    changed = true;
                }
                if resp.drag_stopped() {
                    self.cam_drag = None;
                }
                if let Some(h) = resp.hover_pos() {
                    let sc = ui.input(|i| i.smooth_scroll_delta.y);
                    if sc != 0.0 {
                        if let Some(k) = order.into_iter().find(|&k| frame_of(cam[k]).contains(h)) {
                            cam[k][2] = (cam[k][2] * (sc * 0.004).exp()).clamp(1.0, 8.0);
                            changed = true;
                        }
                    }
                }
                for k in [0usize, 1] {
                    let fr = frame_of(cam[k]);
                    painter.rect_stroke(fr, 2.0, Stroke::new(2.0, cols[k]), egui::StrokeKind::Inside);
                    painter.text(fr.left_top() + vec2(6.0, 4.0), egui::Align2::LEFT_TOP, ["A", "B"][k], egui::FontId::proportional(14.0), cols[k]);
                }
                // Le chemin du centre, de A vers B
                painter.arrow(frame_of(cam[0]).center(), frame_of(cam[1]).center() - frame_of(cam[0]).center(), Stroke::new(1.5, cols[1].gamma_multiply(0.7)));
                if changed {
                    self.story.panels[self.cur].cam = Some(cam);
                    self.touch();
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(700));
                }
                return;
            }
        }
        // Numéro de la case, en haut à gauche, hors du dessin
        painter.text(rect.left_top() + vec2(0.0, -2.0), egui::Align2::LEFT_BOTTOM, self.story.label(self.cur), egui::FontId::proportional(11.0), p.muted);
        if self.playing.is_some() {
            return;
        }
        // ---- Dessin : les événements bruts du pointeur, pour garder chaque point même si l'image est lente
        let resp = ui.interact(rect.expand(30.0), egui::Id::unique(("story-canvas", &self.path)), egui::Sense::drag());
        let to_px = |q: Pos2| ((q.x - rect.left()) / s, (q.y - rect.top()) / s);
        // Pression du stylet, si la tablette en donne
        let force = ui.input(|i| i.events.iter().rev().find_map(|e| if let egui::Event::Touch { force: Some(f), .. } = e { Some(*f) } else { None })).unwrap_or(1.0).clamp(0.1, 1.0);
        let mine = |q: Pos2| rect.expand(30.0).contains(q) && ui.ctx().layer_id_at(q) == Some(ui.layer_id());
        let events = ui.input(|i| i.events.clone());
        let mut ended = false;
        for ev in events {
            match ev {
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. } if mine(pos) && self.stroke.is_none() => {
                    let before = self.pixmap(id).data().to_vec();
                    self.undo.push(Undo::Pixels(id, before));
                    self.redo.clear();
                    if self.undo.len() > 60 {
                        self.undo.remove(0);
                    }
                    let (x, y) = to_px(pos);
                    self.stroke = Some((self.tool, vec![(x, y, force)]));
                }
                egui::Event::PointerMoved(pos) | egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, .. } if self.stroke.is_some() => {
                    let (x, y) = to_px(pos);
                    let (tool, st) = self.stroke.as_mut().unwrap();
                    let tool = *tool;
                    let last = *st.last().unwrap();
                    if (x - last.0).hypot(y - last.1) > 0.8 {
                        st.push((x, y, force));
                        if tool != Tool::Fill {
                            // Seulement le nouveau bout du trait : le reste est déjà dans l'image
                            let seg = [last, (x, y, force)];
                            let (color, size) = (self.color, self.size);
                            story::stroke(self.pixmap(id), tool, color, size, &seg);
                            self.bump(id);
                        }
                    }
                    if matches!(ev, egui::Event::PointerButton { .. }) {
                        ended = true;
                    }
                }
                egui::Event::PointerGone if self.stroke.is_some() => ended = true,
                _ => {}
            }
        }
        if ended {
            if let Some((tool, st)) = self.stroke.take() {
                let (color, size) = (self.color, self.size);
                if tool == Tool::Fill {
                    story::fill(self.pixmap(id), color, &st);
                } else if st.len() == 1 {
                    story::stroke(self.pixmap(id), tool, color, size, &st);
                }
                self.bump(id);
                self.save_panel(id);
            }
        }
        if self.stroke.is_some() {
            ui.ctx().request_repaint();
        }
        // Le lasso de l'aplat en cours, et le cercle du pinceau
        if let Some((tool, st)) = &self.stroke {
            if *tool == Tool::Fill && st.len() > 1 {
                let pts: Vec<Pos2> = st.iter().map(|(x, y, _)| pos2(rect.left() + x * s, rect.top() + y * s)).collect();
                painter.add(egui::Shape::line(pts, Stroke::new(1.5, Color32::from_rgb(self.color[0], self.color[1], self.color[2]))));
            }
        }
        if let Some(h) = resp.hover_pos() {
            if self.tool != Tool::Fill {
                painter.circle_stroke(h, (self.size * s * 0.5).max(1.5), Stroke::new(1.0, Color32::from_gray(120)));
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
    }

    /// Infos de la case : scène, plan, caméra, durée, action, dialogue, notes.
    fn side(&mut self, ui: &mut egui::Ui, area: Rect) {
        let p = theme::pal();
        let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(egui::Layout::top_down(egui::Align::Min)));
        let ui = &mut cui;
        egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).corner_radius(4.0).inner_margin(10).show(ui, |ui| {
            ui.set_min_size(area.size() - vec2(20.0, 20.0));
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                let n = self.story.panels.len();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{} {}", crate::i18n::t("Case"), self.story.label(self.cur))).strong().size(14.0));
                    ui.label(RichText::new(format!("/ {n}")).color(p.muted));
                });
                let before = self.story.panels[self.cur].clone();
                let total = self.story.total();
                let start = self.story.starts()[self.cur];
                let pan = &mut self.story.panels[self.cur];
                let field = |ui: &mut egui::Ui, label: &str| {
                    ui.add_space(6.0);
                    ui.label(RichText::new(crate::i18n::t(label)).size(11.0).color(p.muted));
                };
                field(ui, "scène");
                ui.add(egui::TextEdit::singleline(&mut pan.scene).hint_text(crate::i18n::t("1, INT. CUISINE – JOUR…")).desired_width(f32::INFINITY));
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        field(ui, "plan");
                        egui::ComboBox::from_id_salt("story-shot").selected_text(if pan.shot.is_empty() { "—".to_string() } else { crate::i18n::t(&pan.shot) }).width(116.0).show_ui(ui, |ui| {
                            if ui.selectable_label(pan.shot.is_empty(), "—").clicked() {
                                pan.shot.clear();
                            }
                            for s in story::SHOTS {
                                if ui.selectable_label(pan.shot == s, crate::i18n::t(s)).clicked() {
                                    pan.shot = s.to_string();
                                }
                            }
                        });
                    });
                    ui.vertical(|ui| {
                        field(ui, "caméra");
                        egui::ComboBox::from_id_salt("story-move").selected_text(if pan.camera.is_empty() { "—".to_string() } else { crate::i18n::t(&pan.camera) }).width(116.0).show_ui(ui, |ui| {
                            if ui.selectable_label(pan.camera.is_empty(), "—").clicked() {
                                pan.camera.clear();
                            }
                            for s in story::MOVES {
                                if ui.selectable_label(pan.camera == s, crate::i18n::t(s)).clicked() {
                                    pan.camera = s.to_string();
                                }
                            }
                        });
                    });
                });
                // Caméra 2D : deux cadres (A au début, B à la fin) parcourus pendant la case
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let mut on = pan.cam.is_some();
                    if ui.checkbox(&mut on, crate::i18n::t("caméra 2D")).on_hover_text(crate::i18n::t("Pano, zoom ou travelling sur le dessin : le cadre passe de A à B pendant la case (lecture, MP4, montage)")).changed() {
                        pan.cam = on.then_some([[0.5, 0.5, 1.0], [0.5, 0.5, 1.6]]);
                        self.cam_edit = on;
                    }
                    if pan.cam.is_some() {
                        let b = egui::Button::new(RichText::new(crate::lucide::SCAN).color(if self.cam_edit { p.on_accent } else { p.text })).fill(if self.cam_edit { p.accent } else { Color32::TRANSPARENT });
                        if ui.add(b).on_hover_text(crate::i18n::t("Régler les cadres sur la case : glisser pour déplacer, molette pour zoomer")).clicked() {
                            self.cam_edit = !self.cam_edit;
                        }
                    }
                });
                field(ui, "durée");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut pan.dur).range(0.1..=600.0).speed(0.05).max_decimals(2).suffix(" s"));
                    ui.label(RichText::new(format!("{} {} · {} {}", crate::i18n::t_mid("à"), clock(start), crate::i18n::t_mid("total"), clock(total))).size(11.0).color(p.muted));
                });
                field(ui, "action");
                ui.add(egui::TextEdit::multiline(&mut pan.action).hint_text(crate::i18n::t("Ce qui se passe à l'image")).desired_rows(3).desired_width(f32::INFINITY));
                field(ui, "dialogue");
                ui.add(egui::TextEdit::multiline(&mut pan.dialogue).hint_text(crate::i18n::t("Répliques, voix off")).desired_rows(3).desired_width(f32::INFINITY));
                field(ui, "notes");
                ui.add(egui::TextEdit::multiline(&mut pan.notes).hint_text(crate::i18n::t("Son, effets, transitions…")).desired_rows(2).desired_width(f32::INFINITY));
                if *pan != before {
                    self.touch();
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button(crate::lucide::CHEVRON_LEFT).on_hover_text(crate::i18n::t("déplacer avant (Alt+←)")).clicked() {
                        self.move_panel(-1);
                    }
                    if ui.button(crate::lucide::CHEVRON_RIGHT).on_hover_text(crate::i18n::t("déplacer après (Alt+→)")).clicked() {
                        self.move_panel(1);
                    }
                    if ui.button(crate::lucide::COPY).on_hover_text(crate::i18n::t("dupliquer la case (Ctrl+D)")).clicked() {
                        self.duplicate();
                    }
                    if ui.button(crate::lucide::TRASH_2).on_hover_text(crate::i18n::t("supprimer la case (son dessin reste sur le disque)")).clicked() && self.story.panels.len() > 1 {
                        self.story_checkpoint();
                        self.story.panels.remove(self.cur);
                        self.cur = self.cur.min(self.story.panels.len() - 1);
                        self.touch();
                    }
                });
            });
        });
    }

    /// La bande des cases : vignettes, durée, ajout ; le temps de l'animatique au-dessus.
    fn strip(&mut self, ui: &mut egui::Ui, area: Rect) {
        let p = theme::pal();
        let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(egui::Layout::top_down(egui::Align::Min)));
        let ui = &mut cui;
        ui.horizontal(|ui| {
            let playing = self.playing.is_some();
            if ui.add(egui::Button::new(RichText::new(if playing { crate::lucide::PAUSE } else { crate::lucide::PLAY }).size(15.0)).min_size(vec2(28.0, 24.0))).on_hover_text(crate::i18n::t("lire l'animatique (Espace)")).clicked() {
                self.toggle_play();
            }
            let now = match self.playing {
                Some((t0, from)) => from + t0.elapsed().as_secs_f32(),
                None => self.story.starts()[self.cur],
            };
            ui.label(RichText::new(format!("{} / {}", clock(now), clock(self.story.total()))).monospace().size(11.5).color(p.muted));
            // Barre du temps : chaque case à sa largeur, la tête de lecture par-dessus
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width() - 4.0, 14.0), egui::Sense::click_and_drag());
            let total = self.story.total().max(0.001);
            let starts = self.story.starts();
            for (i, pnl) in self.story.panels.iter().enumerate() {
                let x0 = r.left() + starts[i] / total * r.width();
                let x1 = x0 + pnl.dur.max(0.05) / total * r.width();
                let col = if i == self.cur { p.accent } else if i % 2 == 0 { p.raised } else { p.line };
                ui.painter().rect_filled(Rect::from_min_max(pos2(x0, r.top() + 3.0), pos2(x1 - 1.0, r.bottom() - 3.0)), 1.0, col);
            }
            let hx = r.left() + now / total * r.width();
            ui.painter().vline(hx, r.y_range(), Stroke::new(2.0, p.text));
            if let Some(m) = resp.interact_pointer_pos().filter(|_| resp.clicked() || resp.dragged()) {
                let t = ((m.x - r.left()) / r.width()).clamp(0.0, 1.0) * total;
                self.cur = self.story.at(t);
                if self.playing.is_some() {
                    self.playing = Some((Instant::now(), t));
                }
            }
        });
        let thumb_h = 72.0;
        let thumb_w = thumb_h * self.story.width as f32 / self.story.height.max(1) as f32;
        egui::ScrollArea::horizontal().id_salt("story-strip").auto_shrink(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let mut pick: Option<usize> = None;
                let mut insert: Option<usize> = None;
                let mut prev_scene: Option<String> = None;
                for i in 0..self.story.panels.len() {
                    let pnl = self.story.panels[i].clone();
                    // Un trait sépare les scènes
                    if prev_scene.as_ref().is_some_and(|s| *s != pnl.scene) {
                        let (r, _) = ui.allocate_exact_size(vec2(2.0, thumb_h + 22.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, 1.0, p.accent.gamma_multiply(0.6));
                    }
                    prev_scene = Some(pnl.scene.clone());
                    let v = self.ver.get(&pnl.id).copied().unwrap_or(0);
                    if self.thumbs.get(&pnl.id).is_none_or(|t| t.0 != v) {
                        let pm = self.pixmap(pnl.id).clone();
                        let t = Self::texture(ui.ctx(), "story-thumb", &pm, 240);
                        self.thumbs.insert(pnl.id, (v, t));
                    }
                    let (r, resp) = ui.allocate_exact_size(vec2(thumb_w, thumb_h + 22.0), egui::Sense::click());
                    let img = Rect::from_min_size(r.min, vec2(thumb_w, thumb_h));
                    ui.painter().rect_filled(img, 2.0, Color32::WHITE);
                    if let Some((_, t)) = self.thumbs.get(&pnl.id) {
                        ui.painter().image(t.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    let on = i == self.cur;
                    ui.painter().rect_stroke(img, 2.0, Stroke::new(if on { 2.5 } else { 1.0 }, if on { p.accent } else { p.line }), egui::StrokeKind::Outside);
                    ui.painter().text(pos2(img.left(), img.bottom() + 4.0), egui::Align2::LEFT_TOP, self.story.label(i), egui::FontId::proportional(11.0), if on { p.text } else { p.muted });
                    ui.painter().text(pos2(img.right(), img.bottom() + 4.0), egui::Align2::RIGHT_TOP, format!("{:.1} s", pnl.dur), egui::FontId::proportional(10.5), p.muted);
                    let resp = resp.on_hover_text(if pnl.dialogue.trim().is_empty() { pnl.action.clone() } else { format!("« {} »", pnl.dialogue.trim()) });
                    if resp.clicked() {
                        pick = Some(i);
                    }
                    resp.context_menu(|ui| {
                        if ui.button(crate::i18n::t("nouvelle case après (N)")).clicked() {
                            insert = Some(i);
                            ui.close();
                        }
                    });
                }
                let (r, resp) = ui.allocate_exact_size(vec2(thumb_h * 0.8, thumb_h), egui::Sense::click());
                ui.painter().rect_stroke(r, 4.0, Stroke::new(1.0, if resp.hovered() { p.accent } else { p.line }), egui::StrokeKind::Inside);
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, crate::lucide::PLUS, egui::FontId::proportional(20.0), p.muted);
                if resp.on_hover_text(crate::i18n::t("nouvelle case (N)")).clicked() {
                    insert = Some(self.cur);
                }
                if let Some(i) = pick {
                    self.cur = i;
                    self.playing = None;
                }
                if let Some(i) = insert {
                    self.story_checkpoint();
                    self.cur = self.story.add(Some(i));
                    self.touch();
                }
            });
        });
    }
}

fn clock(t: f32) -> String {
    let t = t.max(0.0);
    format!("{}:{:04.1}", (t / 60.0) as u32, t % 60.0)
}
