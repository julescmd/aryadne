// Panneaux de l'éditeur de montage (inclus dans montage_ui.rs).

/// Géométrie de la timeline : rangées V en haut (V1 juste au-dessus du séparateur), A en dessous.
#[derive(Clone, Copy)]
struct Geo {
    area: Rect,
    v_top: f32,
    a_top: f32,
    row_h: f32,
    nv: usize,
    na: usize,
    zoom: f32,
    scroll: f32,
}

impl Geo {
    fn y_of(&self, audio: bool, track: usize) -> f32 {
        if audio { self.a_top + track as f32 * self.row_h } else { self.v_top + (self.nv - 1 - track.min(self.nv - 1)) as f32 * self.row_h }
    }
    fn row_at(&self, y: f32) -> Option<(bool, usize)> {
        if y >= self.v_top && y < self.v_top + self.nv as f32 * self.row_h {
            let k = ((y - self.v_top) / self.row_h).floor() as usize;
            return Some((false, self.nv - 1 - k.min(self.nv - 1)));
        }
        if y >= self.a_top && y < self.a_top + self.na as f32 * self.row_h {
            return Some((true, (((y - self.a_top) / self.row_h).floor() as usize).min(self.na - 1)));
        }
        None
    }
    fn x_of(&self, t: f32) -> f32 {
        self.area.left() + (t - self.scroll) * self.zoom
    }
    fn t_of(&self, x: f32) -> f32 {
        (self.scroll + (x - self.area.left()) / self.zoom).max(0.0)
    }
    fn rect_of(&self, c: &Clip) -> Rect {
        let y = self.y_of(c.audio, c.track);
        Rect::from_min_max(pos2(self.x_of(c.start), y + 2.0), pos2(self.x_of(c.end()).max(self.x_of(c.start) + 2.0), y + self.row_h - 2.0))
    }
}

const HEAD_W: f32 = 118.0;
/// Règle : bandes des chapitres en haut (20 px), temps en dessous
const RULER_H: f32 = 46.0;
const CHAP_H: f32 = 22.0;
const SCROLL_H: f32 = 12.0;
const DIVIDER: f32 = 6.0;

impl Editor {
    fn tools_panel(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            for (tool, key, tip) in TOOLS {
                let tip = format!("{} ({key})", crate::i18n::t(tip));
                if icon_button(ui, Icon::Tool(tool), self.tool == tool, &tip).clicked() {
                    self.tool = tool;
                }
            }
        });
    }

    // ------------------------------------------------------------------ panneau Projet

    fn bin_items(&self) -> Vec<String> {
        let mut v = self.project.bin.clone();
        for c in &self.project.clips {
            if c.kind != "text" && !c.file.is_empty() && !v.contains(&c.file) {
                v.push(c.file.clone());
            }
        }
        v
    }

    /// Genre, durée et son d'un média du projet (d'après un clip qui l'utilise, ou lu une fois).
    fn media_info(&mut self, file: &str) -> (String, f32, bool) {
        if let Some(c) = self.project.clips.iter().find(|c| c.file == file) {
            return (c.kind.clone(), c.media, c.has_audio || c.audio);
        }
        let path = self.media_path(file);
        let kind = kind_of(&path).to_string();
        if kind == "image" {
            return (kind, 0.0, false);
        }
        let key = path.to_string_lossy().to_string();
        if let Some(v) = INFO.with(|m| m.borrow().get(&key).cloned()) {
            return v;
        }
        let v = import(&self.path, &path, 0, 0.0).map(|c| (c.kind, c.media, c.has_audio)).unwrap_or((kind, 0.0, false));
        INFO.with(|m| m.borrow_mut().insert(key, v.clone()));
        v
    }

    fn open_source(&mut self, file: &str) {
        let (kind, media, has_audio) = self.media_info(file);
        self.source = Some(Source { file: file.to_string(), kind, media, has_audio, t: 0.0, mark_in: None, mark_out: None, tex: None, rx: None, shown: None });
        self.left_tab = 1;
        self.top_tab = 1;
    }

    /// Pose la partie choisie d'un média : à la fin de l'histoire (V1), ou insérée à la tête de lecture
    fn add_from_card(&mut self, file: &str, at_end: bool) {
        let path = self.media_path(file);
        let Some(mut clips) = self.media_clips(&path, 0, self.atarget) else { return };
        if let Some(&(a, b)) = self.card_trim.get(file) {
            for c in clips.iter_mut() {
                if c.kind != "image" && b > a + 0.05 {
                    c.src_in = a;
                    c.src_out = b.min(c.media.max(b));
                }
            }
        }
        let at = if at_end { self.project.clips.iter().filter(|c| !c.audio && c.track == 0).map(|c| c.end()).fold(0.0, f32::max) } else { self.playhead };
        // À la fin : le son va sur la première piste son libre à cet endroit (rien n'est recouvert)
        if at_end {
            let len = clips.iter().map(|c| c.len()).fold(0.0, f32::max);
            let free = |k: usize| !self.project.clips.iter().any(|o| o.audio && o.track == k && o.start < at + len - 1e-3 && o.end() > at + 1e-3);
            let lane = (0..self.project.lanes(true)).find(|&k| free(k)).unwrap_or(self.project.lanes(true));
            for c in clips.iter_mut().filter(|c| c.audio) {
                c.track = lane;
            }
        }
        self.sel = self.place(clips, at, !at_end);
        let end = self.sel.iter().filter_map(|id| self.project.index(*id)).map(|i| self.project.clips[i].end()).fold(at, f32::max);
        self.changed();
        self.seek(if at_end { at } else { end });
    }

    /// Les médias en cartes, comme la maquette : survoler montre le plan, glisser sur la barre du bas
    /// choisit la partie, E l'ajoute à la fin, Maj+E à la tête de lecture ; glisser la carte la pose où on veut
    fn bin_panel(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let items = self.bin_items();
        let ctx = ui.ctx().clone();
        // Recherche et filtres
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.bin_search).hint_text(format!("{}  {}", crate::lucide::SEARCH, crate::i18n::t("brume, drone, plan large…"))).desired_width(ui.available_width()));
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for (k, label) in ["Tout", "Pas encore utilisés", "Vidéos", "Images", "Sons"].iter().enumerate() {
                let on = self.bin_filter == k as u8;
                let b = egui::Button::new(RichText::new(crate::i18n::t(label)).size(11.0).color(if on { p.text } else { p.muted })).fill(if on { p.selection } else { Color32::TRANSPARENT }).stroke(Stroke::new(1.0, p.line)).corner_radius(10).min_size(vec2(0.0, 20.0));
                if ui.add(b).clicked() {
                    self.bin_filter = k as u8;
                }
            }
        });
        ui.add_space(8.0);
        let q = crate::chat::fold(&self.bin_search);
        let used: HashSet<String> = self.project.clips.iter().map(|c| c.file.clone()).collect();
        let mut shown: Vec<String> = vec![];
        for f in items {
            if !q.is_empty() && !crate::chat::fold(&f).contains(&q) {
                continue;
            }
            let kind = self.media_info(&f).0;
            let keep = match self.bin_filter {
                1 => !used.contains(&f),
                2 => kind == "video",
                3 => kind == "image",
                4 => kind == "audio",
                _ => true,
            };
            if keep {
                shown.push(f);
            }
        }
        let foot = 104.0;
        let mut open: Option<String> = None;
        let mut remove: Option<String> = None;
        let mut add: Option<(String, bool)> = None;
        let bg = egui::ScrollArea::vertical().id_salt("bin").max_height((ui.available_height() - foot).max(60.0)).auto_shrink([false, false]).show(ui, |ui| {
            if shown.is_empty() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(crate::i18n::t("Importe des médias, ou glisse-les ici depuis l'explorateur.")).size(11.5).color(p.muted));
                });
            }
            let gap = 10.0;
            let w = ((ui.available_width() - gap) / 2.0).floor();
            let th_h = w * 9.0 / 16.0;
            let rows = shown.chunks(2).map(|c| c.to_vec()).collect::<Vec<_>>();
            for row in rows {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for file in &row {
                        let (kind, media, _) = self.media_info(file);
                        let name = crate::formats::strip(&Path::new(file).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| file.clone())).to_string();
                        let (r, resp) = ui.allocate_exact_size(vec2(w, th_h + 22.0), Sense::click_and_drag());
                        let th = Rect::from_min_size(r.min, vec2(w, th_h));
                        let bar = Rect::from_min_max(pos2(th.left(), th.bottom() - 12.0), th.max);
                        let hover = resp.hover_pos();
                        let on_bar = hover.is_some_and(|h| bar.contains(h)) || (resp.dragged() && self.card_trim.contains_key(file) && ui.input(|i| i.pointer.press_origin()).is_some_and(|o| bar.contains(o)));
                        // Survol : la carte montre l'image sous le pointeur (temps arrondi, pour garder peu de vignettes)
                        let t_hover = hover.filter(|h| th.contains(*h)).map(|h| ((h.x - th.left()) / th.width()).clamp(0.0, 1.0) * media);
                        let t = if kind == "image" { 0.0 } else { t_hover.map(|t| (t * 4.0).round() / 4.0).unwrap_or((media * 0.3).min(2.0)) };
                        let path = crate::bridge::media(&self.media_path(file));
                        ui.painter().rect_filled(th, 10.0, Color32::from_gray(28));
                        if kind == "audio" {
                            if let Some(wv) = self.strips.wave(&path, &ctx) {
                                let mid = th.center().y;
                                let mut x = th.left() + 6.0;
                                while x < th.right() - 6.0 {
                                    let f = (x - th.left()) / th.width();
                                    let v = wv.get((f * wv.len() as f32) as usize).copied().unwrap_or(0.0);
                                    ui.painter().line_segment([pos2(x, mid - v * th.height() * 0.38), pos2(x, mid + v * th.height() * 0.38)], Stroke::new(1.5, Color32::from_rgb(0x83, 0xb9, 0x9e)));
                                    x += 3.0;
                                }
                            }
                        } else if let Some(tex) = self.strips.thumb(&path, t, &ctx).or_else(|| self.strips.thumb(&path, (media * 0.3).min(2.0), &ctx)) {
                            ui.painter().add(egui::epaint::RectShape::filled(th, 10.0, Color32::WHITE).with_texture(tex, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))));
                        }
                        let focus = self.card_focus.as_deref() == Some(file.as_str());
                        if focus || resp.hovered() {
                            ui.painter().rect_stroke(th.expand(1.0), 11.0, Stroke::new(2.0, if focus { Color32::from_rgb(0xaf, 0x9c, 0xc9) } else { p.line }), egui::StrokeKind::Outside);
                        }
                        // Partie choisie : barre en bas, poignées, durée
                        if media > 0.0 && kind != "image" {
                            let x_of = |t: f32| th.left() + (t / media).clamp(0.0, 1.0) * th.width();
                            if resp.hovered() || self.card_trim.contains_key(file) {
                                ui.painter().rect_filled(Rect::from_min_max(pos2(th.left() + 6.0, th.bottom() - 6.0), pos2(th.right() - 6.0, th.bottom() - 3.0)), 1.5, Color32::from_black_alpha(150));
                            }
                            if let Some(&(a, b)) = self.card_trim.get(file) {
                                let (xa, xb) = (x_of(a), x_of(b));
                                ui.painter().rect_filled(Rect::from_min_max(pos2(xa, th.bottom() - 6.0), pos2(xb, th.bottom() - 3.0)), 1.5, Color32::from_rgb(0xaf, 0x9c, 0xc9));
                                for x in [xa, xb] {
                                    ui.painter().rect_filled(Rect::from_center_size(pos2(x, th.center().y), vec2(3.0, th.height() - 8.0)), 1.5, Color32::from_rgb(0xaf, 0x9c, 0xc9));
                                }
                                let g = ui.painter().layout_no_wrap(format!("{} → {}", short_time(a), short_time(b)), egui::FontId::monospace(10.0), Color32::WHITE);
                                let tr = Rect::from_min_size(th.min + vec2(6.0, 6.0), g.size() + vec2(8.0, 2.0));
                                ui.painter().rect_filled(tr, 2.0, Color32::from_black_alpha(170));
                                ui.painter().galley(tr.min + vec2(4.0, 1.0), g, Color32::WHITE);
                            }
                            if let Some(t) = t_hover {
                                let x = x_of(t);
                                ui.painter().vline(x, th.y_range(), Stroke::new(1.0, Color32::WHITE));
                            }
                            let g = ui.painter().layout_no_wrap(short_time(media), egui::FontId::monospace(10.0), Color32::WHITE);
                            let tr = Rect::from_min_size(pos2(th.right() - g.size().x - 14.0, th.bottom() - g.size().y - 12.0), g.size() + vec2(8.0, 2.0));
                            ui.painter().rect_filled(tr, 2.0, Color32::from_black_alpha(170));
                            ui.painter().galley(tr.min + vec2(4.0, 1.0), g, Color32::WHITE);
                        }
                        if used.contains(file) {
                            ui.painter().circle_filled(pos2(th.left() + 10.0, th.bottom() - 14.0), 3.0, Color32::from_rgb(0x88, 0xbf, 0x9a));
                        }
                        ui.painter().with_clip_rect(r).text(pos2(r.left() + 1.0, th.bottom() + 5.0), egui::Align2::LEFT_TOP, &name, egui::FontId::proportional(12.0), p.text);
                        if resp.hovered() {
                            self.card_focus = Some(file.clone());
                        }
                        // Glisser sur la barre du bas : choisir la partie ; ailleurs : poser la carte dans la timeline
                        let press_on_bar = ui.input(|i| i.pointer.press_origin()).is_some_and(|o| bar.expand2(vec2(0.0, 6.0)).contains(o));
                        if media > 0.0 && kind != "image" && press_on_bar && (resp.dragged() || resp.drag_started()) {
                            if let (Some(o), Some(at)) = (ui.input(|i| i.pointer.press_origin()), resp.interact_pointer_pos()) {
                                let ta = ((o.x.min(at.x) - th.left()) / th.width()).clamp(0.0, 1.0) * media;
                                let tb = ((o.x.max(at.x) - th.left()) / th.width()).clamp(0.0, 1.0) * media;
                                self.card_trim.insert(file.clone(), (ta, tb.max(ta + 0.1).min(media)));
                            }
                        } else {
                            resp.dnd_set_drag_payload(Payload::Media(file.clone()));
                        }
                        let _ = on_bar;

                        if resp.double_clicked() {
                            open = Some(file.clone());
                        }
                        resp.context_menu(|ui| {
                            if ui.button(crate::i18n::t("Ajouter à la fin   E")).clicked() {
                                add = Some((file.clone(), true));
                                ui.close();
                            }
                            if ui.button(crate::i18n::t("Insérer à la tête de lecture   Maj+E")).clicked() {
                                add = Some((file.clone(), false));
                                ui.close();
                            }
                            if self.card_trim.contains_key(file) && ui.button(crate::i18n::t("Tout le média")).clicked() {
                                self.card_trim.remove(file);
                                ui.close();
                            }
                            if ui.button(crate::i18n::t("Ouvrir dans la source")).clicked() {
                                open = Some(file.clone());
                                ui.close();
                            }
                            if self.project.bin.contains(file) && !self.project.clips.iter().any(|c| &c.file == file) && ui.button(crate::i18n::t("retirer du projet")).clicked() {
                                remove = Some(file.clone());
                                ui.close();
                            }
                        });
                    }
                });
                ui.add_space(8.0);
            }
        });
        // E et Maj+E : la carte survolée ou la dernière choisie
        if !ctx.egui_wants_keyboard_input() {
            if let Some(f) = self.card_focus.clone().filter(|f| shown.contains(f)) {
                let (e, shift) = ui.input(|i| (i.key_pressed(egui::Key::E), i.modifiers.shift));
                if e {
                    add = Some((f, !shift));
                }
            }
        }
        if let Some((f, end)) = add {
            self.add_from_card(&f, end);
        }
        // Fichiers glissés depuis l'explorateur du coffre : ajoutés au projet
        let drop_zone = ui.interact(bg.inner_rect, ui.scope_id().with("bin-drop"), Sense::hover());
        if let Some(rel) = crate::drop_of::<String>(&drop_zone) {
            let abs = Path::new(&self.root).join(rel.as_str());
            if let Ok(c) = import(&self.path, &abs, 0, 0.0) {
                if !self.project.bin.contains(&c.file) {
                    self.project.bin.push(c.file);
                    self.changed();
                }
            }
        }
        if drop_zone.dnd_hover_payload::<String>().is_some() {
            ui.painter().rect_stroke(bg.inner_rect, 6.0, Stroke::new(2.0, p.accent), egui::StrokeKind::Inside);
        }
        if let Some(f) = open {
            self.open_source(&f);
        }
        if let Some(f) = remove {
            self.project.bin.retain(|x| *x != f);
            self.changed();
        }
        // Aide et boutons, en bas
        egui::Frame::new().fill(p.raised).stroke(Stroke::new(1.0, p.line)).corner_radius(6).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(crate::i18n::t("Survole une carte pour voir le plan. Glisse sur sa barre du bas pour choisir la partie : E l'ajoute à la fin, Maj+E à la tête de lecture.")).size(11.5).color(p.text));
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::PLUS).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Vidéos, images, sons, images à calques (.aryimg)")).clicked() {
                if let Some(files) = rfd::FileDialog::new().pick_files() {
                    for f in files {
                        if let Ok(c) = import(&self.path, &f, 0, 0.0) {
                            if !self.project.bin.contains(&c.file) {
                                self.project.bin.push(c.file);
                            }
                        }
                    }
                    self.changed();
                }
            }
            if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::TYPE).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Ajoute un titre à la tête de lecture (outil Texte : T)")).clicked() {
                self.add_title(self.playhead, None);
            }
        });
    }

    fn add_title(&mut self, t: f32, track: Option<usize>) {
        let k = self.project.add_text(t);
        if let Some(tr) = track {
            self.project.clips[k].track = tr;
        }
        self.project.settle(k);
        self.sel = vec![self.project.clips[k].id];
        self.top_tab = 0;
        self.changed();
    }

    // ------------------------------------------------------------------ panneau Effets

    fn effects_panel(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        ui.add(egui::TextEdit::singleline(&mut self.fx_search).hint_text(crate::i18n::t("rechercher un effet")).desired_width(f32::INFINITY));
        ui.add_space(4.0);
        let q = self.fx_search.to_lowercase();
        let mut apply: Option<&'static str> = None;
        for cat in ["Transitions vidéo", "Effets vidéo", "Transitions audio"] {
            let list: Vec<&(&str, &str, &str)> = EFFECTS.iter().filter(|e| e.2 == cat && (q.is_empty() || crate::i18n::t(e.1).to_lowercase().contains(&q))).collect();
            if list.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(RichText::new(crate::i18n::t(cat)).size(12.0).family(crate::theme::bold())).default_open(true).show(ui, |ui| {
                for (id, name, _) in list {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click_and_drag());
                    resp.dnd_set_drag_payload(Payload::Effect(id));
                    if resp.hovered() {
                        ui.painter().rect_filled(r, 3.0, p.line.gamma_multiply(0.5));
                    }
                    let badge = Rect::from_min_size(pos2(r.left() + 4.0, r.center().y - 7.0), vec2(20.0, 14.0));
                    ui.painter().rect_stroke(badge, 2.0, Stroke::new(1.0, p.muted), egui::StrokeKind::Inside);
                    ui.painter().text(badge.center(), egui::Align2::CENTER_CENTER, if cat.starts_with("Transitions") { "⇄" } else { "fx" }, egui::FontId::proportional(9.5), p.muted);
                    ui.painter().text(pos2(badge.right() + 8.0, r.center().y), egui::Align2::LEFT_CENTER, crate::i18n::t(name), egui::FontId::proportional(12.0), p.text);
                    if resp.double_clicked() {
                        apply = Some(id);
                    }
                    resp.on_hover_text(crate::i18n::t("Glisse sur un clip de la timeline (ou double-clic : sur le clip choisi)"));
                }
            });
        }
        ui.add_space(8.0);
        ui.label(RichText::new(crate::i18n::t("Une transition se pose du côté du clip où on la lâche. Ctrl+D : fondu enchaîné, Ctrl+Maj+D : fondu audio.")).size(10.5).color(p.muted));
        if let (Some(fx), Some(i)) = (apply, self.first_selected()) {
            let at = self.project.clips[i].start;
            self.apply_effect(i, fx, at);
        }
    }

    // ------------------------------------------------------------------ options d'effet

    fn section(&mut self, ui: &mut egui::Ui, key: &'static str, title: &str, reset: Option<&mut bool>) -> bool {
        let p = crate::theme::pal();
        let open = !self.folded.contains(key);
        let mut toggle = false;
        ui.horizontal(|ui| {
            let r = ui.add(egui::Label::new(RichText::new(if open { "▾" } else { "▸" }).color(p.muted)).sense(Sense::click()));
            toggle |= r.clicked();
            ui.label(RichText::new("fx").size(9.5).color(p.accent).family(crate::theme::bold()));
            let r = ui.add(egui::Label::new(RichText::new(crate::i18n::t(title)).size(12.0).family(crate::theme::bold())).sense(Sense::click()));
            toggle |= r.clicked();
            if let Some(reset) = reset {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new("↺").size(12.0)).frame(false)).on_hover_text(crate::i18n::t("réinitialiser")).clicked() {
                        *reset = true;
                    }
                });
            }
        });
        if toggle {
            if open {
                self.folded.insert(key);
            } else {
                self.folded.remove(key);
            }
        }
        open
    }

    fn effect_controls(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let Some(s) = self.first_selected() else {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(crate::i18n::t("Choisis un clip dans la timeline pour régler ses effets.")).color(p.muted).size(12.0));
            });
            return;
        };
        let head = self.playhead;
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        {
            let c = &self.project.clips[s];
            ui.label(RichText::new(format!("{stem}  ›  {}", c.name)).size(11.0).color(p.muted));
        }
        // Mini-timeline du clip : ses clés et la tête de lecture
        {
            let c = self.project.clips[s].clone();
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click_and_drag());
            ui.painter().rect_filled(r, 2.0, p.bg);
            let x_of = |t: f32| r.left() + 6.0 + (t - c.start) / c.len().max(1e-3) * (r.width() - 12.0);
            ui.painter().rect_filled(Rect::from_min_max(pos2(x_of(c.start), r.top() + 3.0), pos2(x_of(c.end()), r.bottom() - 3.0)), 2.0, clip_color(&c).gamma_multiply(0.35));
            for k in &c.keys {
                let x = x_of(c.start + k.t);
                let here = (c.start + k.t - head).abs() < 0.021;
                crate::keys::draw(ui.painter(), pos2(x, r.center().y), 5.0, Some(k.ease), p.text, true, here);
            }
            if head >= c.start && head <= c.end() {
                let x = x_of(head);
                ui.painter().line_segment([pos2(x, r.top()), pos2(x, r.bottom())], Stroke::new(2.0, p.accent));
            }
            if let Some(at) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
                let t = c.start + ((at.x - r.left() - 6.0) / (r.width() - 12.0)).clamp(0.0, 1.0) * c.len();
                // Un clic sur une clé y va tout juste
                let on_key = c.keys.iter().find(|k| (x_of(c.start + k.t) - at.x).abs() < 7.0).map(|k| c.start + k.t);
                self.seek(on_key.unwrap_or(t));
            }
            // Clic droit sur une clé : son lissage (le même menu que la 3D et le Motion)
            if resp.secondary_clicked() {
                self.key_menu = resp.interact_pointer_pos().and_then(|at| c.keys.iter().position(|k| (x_of(c.start + k.t) - at.x).abs() < 7.0)).map(|k| (s, k));
            }
            if let Some((ci, ki)) = self.key_menu.filter(|m| m.0 == s) {
                let cur = c.keys.get(ki).map(|k| k.ease);
                let mut picked = None;
                if cur.is_some() {
                    resp.context_menu(|ui| {
                        if let Some(Some(e)) = crate::keys::menu(ui, cur, false) {
                            picked = Some(e);
                        }
                    });
                }
                if let Some(e) = picked {
                    if let Some(k) = self.project.clips.get_mut(ci).and_then(|c| c.keys.get_mut(ki)) {
                        k.ease = e;
                    }
                    self.key_menu = None;
                    self.changed();
                }
            }
        }
        ui.add_space(4.0);
        let mut ch = false;
        let mut ch_text = false;
        let mut new_grade = false;
        let mut pick_lut = false;
        let mut do_xfade = false;
        let mut key_toggle = false;
        let mut pose_before = None;
        egui::ScrollArea::vertical().id_salt("fx-controls").auto_shrink([false, false]).show(ui, |ui| {
            let visual = self.project.clips[s].visual();
            let audio = self.project.clips[s].sounds();
            let row = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui) -> bool| -> bool {
                ui.horizontal(|ui| {
                    ui.add_space(22.0);
                    ui.add_sized(vec2(110.0, 18.0), egui::Label::new(RichText::new(crate::i18n::t(label)).size(11.5).color(p.text)).truncate());
                    add(ui)
                })
                .inner
            };
            if visual {
                let mut reset = false;
                if self.section(ui, "motion", "Trajectoire", Some(&mut reset)) {
                    let c = &mut self.project.clips[s];
                    pose_before = Some((c.x, c.y, c.scale, c.opacity));
                    ui.horizontal(|ui| {
                        ui.add_space(22.0);
                        let animated = !c.keys.is_empty();
                        let rel = head - c.start;
                        let on_key = c.keys.iter().any(|k| (k.t - rel).abs() < 0.021);
                        if ui.selectable_label(animated, "⏱").on_hover_text(crate::i18n::t("Activer l'animation : pose une clé ici ; ensuite chaque réglage modifié pose sa clé")).clicked() {
                            key_toggle = true;
                        }
                        if animated {
                            if ui.small_button(if on_key { "◆" } else { "◇" }).on_hover_text(crate::i18n::t("ajouter / retirer une clé à la tête de lecture")).clicked() {
                                if on_key {
                                    c.keys.retain(|k| (k.t - rel).abs() >= 0.021);
                                } else if head >= c.start - 1e-3 && head <= c.end() + 1e-3 {
                                    c.set_key(head);
                                }
                                ch = true;
                            }
                            ui.label(RichText::new(format!("{} {}", c.keys.len(), crate::i18n::t_mid("clé(s)"))).size(10.5).color(p.muted));
                        }
                    });
                    ch |= row(ui, "Position", &mut |ui| scrub(ui, &mut c.x, 1.0, -8000.0..=8000.0, "", 1) | scrub(ui, &mut c.y, 1.0, -8000.0..=8000.0, "", 1));
                    let mut pct = c.scale * 100.0;
                    if row(ui, "Échelle", &mut |ui| scrub(ui, &mut pct, 0.5, 1.0..=1000.0, " %", 1)) {
                        c.scale = pct / 100.0;
                        ch = true;
                    }
                    if reset {
                        (c.x, c.y, c.scale) = (0.0, 0.0, 1.0);
                        ch = true;
                    }
                }
                let mut reset = false;
                if self.section(ui, "opacity", "Opacité", Some(&mut reset)) {
                    let c = &mut self.project.clips[s];
                    let mut pct = c.opacity * 100.0;
                    if row(ui, "Opacité", &mut |ui| scrub(ui, &mut pct, 0.5, 0.0..=100.0, " %", 1)) {
                        c.opacity = pct / 100.0;
                        ch = true;
                    }
                    ch |= row(ui, "Fondu d'entrée", &mut |ui| scrub(ui, &mut c.fade_in, 0.02, 0.0..=10.0, " s", 2));
                    ch |= row(ui, "Fondu de sortie", &mut |ui| scrub(ui, &mut c.fade_out, 0.02, 0.0..=10.0, " s", 2));
                    if reset {
                        (c.opacity, c.fade_in, c.fade_out) = (1.0, 0.0, 0.0);
                        ch = true;
                    }
                }
                if self.project.clips[s].kind == "video" {
                    let mut reset = false;
                    if self.section(ui, "time", "Remappage temporel", Some(&mut reset)) {
                        let c = &mut self.project.clips[s];
                        let mut pct = c.speed * 100.0;
                        if row(ui, "Vitesse", &mut |ui| scrub(ui, &mut pct, 0.5, 10.0..=1000.0, " %", 1)) {
                            c.speed = pct / 100.0;
                            ch = true;
                        }
                        if reset {
                            c.speed = 1.0;
                            ch = true;
                        }
                    }
                }
                if self.project.clips[s].kind == "text" && self.section(ui, "text", "Texte", None) {
                    let c = &mut self.project.clips[s];
                    ui.horizontal(|ui| {
                        ui.add_space(22.0);
                        ch_text |= ui.add(egui::TextEdit::multiline(&mut c.text).desired_rows(2).desired_width(f32::INFINITY)).changed();
                    });
                    ch_text |= row(ui, "Taille", &mut |ui| scrub(ui, &mut c.text_size, 0.5, 6.0..=1000.0, " px", 0));
                    ch_text |= row(ui, "Couleur", &mut |ui| ui.color_edit_button_srgb(&mut c.text_color).changed() | ui.checkbox(&mut c.outline, crate::i18n::t("contour")).changed());
                    ui.horizontal(|ui| {
                        ui.add_space(22.0);
                        for (id, label) in [("", "gras"), ("lato", "normal"), ("mono", "mono")] {
                            if ui.selectable_label(c.font == id || (id.is_empty() && c.font == "lato-bold"), RichText::new(crate::i18n::t(label)).size(11.0)).clicked() {
                                c.font = id.to_string();
                                ch_text = true;
                            }
                        }
                        if ui.small_button(crate::i18n::t("police…")).clicked() {
                            if let Some(f) = rfd::FileDialog::new().add_filter("police", &["ttf", "otf"]).pick_file() {
                                c.font = f.to_string_lossy().to_string();
                                ch_text = true;
                            }
                        }
                    });
                }
                let c = &self.project.clips[s];
                let lumetri = c.fx.iter().any(|f| f == "lumetri") || c.brightness != 0.0 || c.contrast != 1.0 || c.saturation != 1.0 || !c.grade.is_empty();
                if lumetri {
                    let mut reset = false;
                    if self.section(ui, "lumetri", "Couleur Lumetri", Some(&mut reset)) {
                        let c = &mut self.project.clips[s];
                        ch |= row(ui, "Exposition", &mut |ui| scrub(ui, &mut c.brightness, 0.005, -0.5..=0.5, "", 3));
                        ch |= row(ui, "Contraste", &mut |ui| scrub(ui, &mut c.contrast, 0.01, 0.2..=2.5, "", 2));
                        ch |= row(ui, "Saturation", &mut |ui| scrub(ui, &mut c.saturation, 0.01, 0.0..=3.0, "", 2));
                        ui.horizontal(|ui| {
                            ui.add_space(22.0);
                            if crate::montage::is_cube(&c.grade) {
                                let name = c.grade.rsplit('/').next().unwrap_or(&c.grade).to_string();
                                ui.label(egui::RichText::new(format!("{}  {name}", crate::lucide::PALETTE)).size(12.0)).on_hover_text(crate::i18n::t("LUT appliquée à ce clip"));
                            } else if ui.button(if c.grade.is_empty() { crate::i18n::t("＋ étalonnage en nodes") } else { crate::i18n::t("nodes ✎") }).on_hover_text(crate::i18n::t("Un graphe de nodes (courbes, rampes, balance…) appliqué aux couleurs de ce clip")).clicked() {
                                new_grade = true;
                            }
                            if c.grade.is_empty() && ui.add(egui::Button::new(egui::RichText::new(format!("{}  LUT", crate::lucide::PALETTE)).size(12.0))).on_hover_text(crate::i18n::t("Appliquer une LUT .cube (un look, le profil Log d'une caméra)")).clicked() {
                                pick_lut = true;
                            }
                            if !c.grade.is_empty() && ui.add(egui::Button::new(egui::RichText::new(crate::lucide::X).size(13.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("retirer")).clicked() {
                                c.grade.clear();
                                ch = true;
                            }
                        });
                        if reset {
                            (c.brightness, c.contrast, c.saturation) = (0.0, 1.0, 1.0);
                            c.fx.retain(|f| f != "lumetri");
                            ch = true;
                        }
                    }
                }
            }
            if audio {
                let mut reset = false;
                if self.section(ui, "volume", "Volume", Some(&mut reset)) {
                    let c = &mut self.project.clips[s];
                    let mut db = 20.0 * c.volume.max(1e-4).log10();
                    if row(ui, "Niveau", &mut |ui| scrub(ui, &mut db, 0.1, -60.0..=12.0, " dB", 1)) {
                        c.volume = if db <= -59.9 { 0.0 } else { 10f32.powf(db / 20.0) };
                        ch = true;
                    }
                    ch |= row(ui, "Fondu d'entrée", &mut |ui| scrub(ui, &mut c.fade_in, 0.02, 0.0..=10.0, " s", 2));
                    ch |= row(ui, "Fondu de sortie", &mut |ui| scrub(ui, &mut c.fade_out, 0.02, 0.0..=10.0, " s", 2));
                    if reset {
                        (c.volume, c.fade_in, c.fade_out) = (1.0, 0.0, 0.0);
                        ch = true;
                    }
                }
            }
            let mut reset = false;
            if self.section(ui, "xfade", "Fondu enchaîné", Some(&mut reset)) {
                let c = &mut self.project.clips[s];
                ch |= row(ui, "Durée", &mut |ui| scrub(ui, &mut c.xfade, 0.02, 0.0..=10.0, " s", 2));
                ui.horizontal(|ui| {
                    ui.add_space(22.0);
                    if ui.button(RichText::new(crate::i18n::t("enchaîner avec le clip précédent")).size(11.0)).on_hover_text(crate::i18n::t("Recule ce clip pour qu'il chevauche la fin du précédent sur la même piste, et fond l'un dans l'autre")).clicked() {
                        do_xfade = true;
                    }
                });
                if reset {
                    c.xfade = 0.0;
                    ch = true;
                }
            }
        });
        if key_toggle {
            let c = &mut self.project.clips[s];
            if c.keys.is_empty() {
                c.set_key(head.clamp(c.start, c.end()));
            } else {
                c.keys.clear();
            }
            ch = true;
        }
        if do_xfade {
            let d = if self.project.clips[s].xfade > 0.0 { self.project.clips[s].xfade } else { 1.0 };
            if self.project.crossfade(s, d) {
                ch = true;
            } else {
                self.message = Some(crate::i18n::t("Aucun clip avant celui-ci sur cette piste."));
            }
        }
        {
            let c = &mut self.project.clips[s];
            if !c.keys.is_empty() && pose_before.is_some_and(|b| b != (c.x, c.y, c.scale, c.opacity)) && head >= c.start - 1e-3 && head <= c.end() + 1e-3 {
                c.set_key(head);
            }
        }
        // Le son et l'image d'une même vidéo gardent les mêmes vitesse et fondus
        if ch {
            let c = self.project.clips[s].clone();
            for k in self.project.linked(s) {
                if k != s {
                    let o = &mut self.project.clips[k];
                    (o.speed, o.fade_in, o.fade_out, o.xfade) = (c.speed, c.fade_in, c.fade_out, c.xfade);
                }
            }
        }
        if new_grade {
            self.new_grade(s);
            ch = true;
        }
        if pick_lut {
            ch |= self.pick_lut(s);
        }
        if ch || ch_text {
            self.changed();
        }
    }

    // ------------------------------------------------------------------ moniteur Source

    fn source_panel(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let ctx = ui.ctx().clone();
        let fps = self.project.fps.max(1.0);
        let aspect = self.project.width as f32 / self.project.height.max(1) as f32;
        let path_of = self.source.as_ref().map(|s| crate::bridge::media(&self.media_path(&s.file)));
        let Some(s) = &mut self.source else {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(crate::i18n::t("Double-clique un média du panneau Projet pour le voir ici, choisir son entrée (I) et sa sortie (O), puis l'insérer (,) ou recouvrir (.) dans la timeline.")).color(p.muted).size(12.0));
            });
            return;
        };
        let path = path_of.unwrap_or_default();
        // Image demandée à ffmpeg (ou l'image elle-même)
        if let Some(rx) = &s.rx {
            if let Ok(img) = rx.try_recv() {
                if let Some(img) = img {
                    match &mut s.tex {
                        Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                        None => s.tex = Some(ctx.load_texture("source-monitor", img, egui::TextureOptions::LINEAR)),
                    }
                }
                s.rx = None;
            }
        }
        let want = (s.t * fps).round() as i64;
        if s.rx.is_none() && s.shown != Some(want) && s.kind != "audio" {
            let (tx, rx) = channel();
            let (kind, t, c2, pth) = (s.kind.clone(), s.t, ctx.clone(), path.clone());
            std::thread::spawn(move || {
                let img = if kind == "image" {
                    image::open(&pth).ok().map(|i| i.thumbnail(960, 960).to_rgba8())
                } else {
                    ffmpeg::find("ffmpeg").and_then(|ff| ffmpeg::command(ff).args(["-v", "error", "-ss", &format!("{t:.3}"), "-i"]).arg(&pth).args(["-frames:v", "1", "-vf", "scale=640:-2", "-f", "image2pipe", "-vcodec", "png", "-"]).output().ok()).and_then(|o| image::load_from_memory(&o.stdout).ok()).map(|i| i.to_rgba8())
                };
                let _ = tx.send(img.map(|i| egui::ColorImage::from_rgba_unmultiplied([i.width() as usize, i.height() as usize], i.as_raw())));
                c2.request_repaint();
            });
            s.rx = Some(rx);
            s.shown = Some(want);
        }
        let name = Path::new(&s.file).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        ui.label(RichText::new(format!("{} : {name}", crate::i18n::t("Source"))).size(11.5).family(crate::theme::bold()));
        let avail = ui.available_size() - vec2(0.0, 74.0);
        let size = if avail.x / avail.y.max(1.0) > aspect { vec2(avail.y * aspect, avail.y) } else { vec2(avail.x, avail.x / aspect) };
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), size.y.max(40.0)), Sense::click_and_drag());
        resp.dnd_set_drag_payload(Payload::Source);
        let resp = resp.on_hover_text(crate::i18n::t("Glisse l'image dans la timeline pour y poser la partie choisie"));
        let img_r = Rect::from_center_size(r.center(), size);
        ui.painter().rect_filled(img_r, 0.0, Color32::BLACK);
        if s.kind == "audio" {
            if let Some(w) = self.strips.wave(&path, &ctx) {
                let mid = img_r.center().y;
                let mut x = img_r.left();
                while x < img_r.right() {
                    let f = (x - img_r.left()) / img_r.width();
                    let v = w.get((f * w.len() as f32) as usize).copied().unwrap_or(0.0);
                    ui.painter().line_segment([pos2(x, mid - v * img_r.height() * 0.45), pos2(x, mid + v * img_r.height() * 0.45)], Stroke::new(1.0, Color32::from_rgb(96, 186, 146)));
                    x += 2.0;
                }
            }
        } else if let Some(t) = &s.tex {
            let ts = t.size_vec2();
            let fit = (img_r.width() / ts.x).min(img_r.height() / ts.y);
            ui.painter().image(t.id(), Rect::from_center_size(img_r.center(), ts * fit), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        let _ = resp;
        // Règle de la source : entrée, sortie, tête
        let len = if s.media > 0.0 { s.media } else { 5.0 };
        let (a, b) = s.range();
        ui.horizontal(|ui| {
            ui.label(RichText::new(timecode(s.t, fps)).monospace().color(p.accent).size(13.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(timecode(b - a, fps)).monospace().color(p.muted).size(11.0));
            });
        });
        let (bar, bresp) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::click_and_drag());
        ui.painter().rect_filled(bar, 2.0, p.bg);
        let bx = |t: f32| bar.left() + t / len * bar.width();
        ui.painter().rect_filled(Rect::from_min_max(pos2(bx(a), bar.top() + 3.0), pos2(bx(b), bar.bottom() - 3.0)), 2.0, p.accent.gamma_multiply(0.3));
        let hx = bx(s.t);
        ui.painter().line_segment([pos2(hx, bar.top()), pos2(hx, bar.bottom())], Stroke::new(2.0, p.accent));
        if let Some(at) = bresp.interact_pointer_pos().filter(|_| bresp.dragged() || bresp.clicked()) {
            s.t = seconds(((at.x - bar.left()) / bar.width()).clamp(0.0, 1.0) * len, fps);
        }
        let mut edit: Option<bool> = None;
        ui.horizontal(|ui| {
            if icon_button(ui, Icon::MarkIn, s.mark_in.is_some(), "marquer l'entrée (I)").clicked() {
                s.mark_in = Some(s.t);
            }
            if icon_button(ui, Icon::MarkOut, s.mark_out.is_some(), "marquer la sortie (O)").clicked() {
                s.mark_out = Some(s.t);
            }
            if icon_button(ui, Icon::GoIn, false, "aller à l'entrée").clicked() {
                s.t = a;
            }
            if icon_button(ui, Icon::StepBack, false, "image précédente").clicked() {
                s.t = (s.t - 1.0 / fps).max(0.0);
            }
            if icon_button(ui, Icon::StepFwd, false, "image suivante").clicked() {
                s.t = (s.t + 1.0 / fps).min(len);
            }
            if icon_button(ui, Icon::GoOut, false, "aller à la sortie").clicked() {
                s.t = b;
            }
            ui.separator();
            if icon_button(ui, Icon::Insert, false, "insérer à la tête de lecture (,)").clicked() {
                edit = Some(true);
            }
            if icon_button(ui, Icon::Overwrite, false, "recouvrir à la tête de lecture (.)").clicked() {
                edit = Some(false);
            }
            if s.has_audio && s.kind == "video" {
                ui.label(RichText::new(crate::i18n::t("image + son")).size(10.5).color(p.muted));
            }
        });
        // I et O au-dessus du moniteur Source : son entrée et sa sortie
        if ui.ui_contains_pointer() && !ctx.egui_wants_keyboard_input() {
            ui.input_mut(|i| {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::I) {
                    s.mark_in = Some(s.t);
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::O) {
                    s.mark_out = Some(s.t);
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
                    s.t = (s.t - 1.0 / fps).max(0.0);
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
                    s.t = (s.t + 1.0 / fps).min(len);
                }
            });
        }
        if let Some(insert) = edit {
            self.source_edit(insert);
        }
    }

    // ------------------------------------------------------------------ moniteur Programme

    /// Le moniteur unique, comme la maquette : format en haut, l'image au centre avec ses poignées,
    /// une barre de lecture flottante en dessous
    fn program_monitor(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let ctx = ui.ctx().clone();
        let fps = self.project.fps.max(1.0);
        let mut restart = false;
        {
            let v = &mut ui.style_mut().visuals.widgets;
            v.inactive.weak_bg_fill = Color32::TRANSPARENT;
            v.inactive.bg_stroke = Stroke::NONE;
            v.hovered.weak_bg_fill = p.raised;
            v.hovered.bg_stroke = Stroke::NONE;
            v.open.weak_bg_fill = p.raised;
        }
        // ---- Format (16:9, 9:16, 1:1, 4:5), puis son, réglages de séquence, export
        ui.horizontal(|ui| {
            let fr = egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).corner_radius(6).inner_margin(2);
            fr.show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let base = self.project.width.min(self.project.height).max(16);
                for (label, w, h) in [("16:9", 16u32, 9u32), ("9:16", 9, 16), ("1:1", 1, 1), ("4:5", 4, 5)] {
                    let on = self.project.width * h == self.project.height * w;
                    let b = egui::Button::new(RichText::new(label).size(12.0).color(if on { p.text } else { p.muted })).fill(if on { p.selection } else { Color32::TRANSPARENT }).min_size(vec2(0.0, 22.0));
                    if ui.add(b).on_hover_text(crate::i18n::t("Format de la vidéo")).clicked() && !on {
                        // Le petit côté reste le même (1080 reste 1080)
                        (self.project.width, self.project.height) = if w >= h { (base * w / h, base) } else { (base, base * h / w) };
                        self.changed();
                    }
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.export_rx.is_some() {
                    ui.spinner();
                    ui.label(RichText::new(crate::i18n::t("export en cours…")).color(p.muted));
                } else if ui.add(egui::Button::new(RichText::new(crate::i18n::t("Exporter")).color(p.on_accent)).fill(p.accent).wrap_mode(egui::TextWrapMode::Extend)).on_hover_text(crate::i18n::t("MP4 H.264 ; entre les points I et O s'ils sont posés (Ctrl+M)")).clicked() {
                    self.export_to(&ctx);
                }
                let mut ch = false;
                ui.menu_button(RichText::new(format!("{}×{} · {} i/s {}", self.project.width, self.project.height, self.project.fps, crate::lucide::CHEVRON_DOWN)).size(11.5).color(p.muted), |ui| {
                    ui.label(RichText::new(crate::i18n::t("Paramètres de séquence")).family(crate::theme::bold()));
                    ui.horizontal(|ui| {
                        ch |= ui.add(egui::DragValue::new(&mut self.project.width).range(16..=32768).suffix(" px")).changed();
                        ui.label("×");
                        ch |= ui.add(egui::DragValue::new(&mut self.project.height).range(16..=32768).suffix(" px")).changed();
                    });
                    ch |= ui.add(egui::DragValue::new(&mut self.project.fps).range(1.0..=1000.0).suffix(" i/s")).changed();
                    for (w, h, label) in [(1920, 1080, "HD 1080p"), (3840, 2160, "4K UHD"), (1080, 1920, "vertical 9:16"), (1080, 1080, "carré")] {
                        if ui.button(label).clicked() {
                            (self.project.width, self.project.height) = (w, h);
                            ch = true;
                        }
                    }
                });
                if ch {
                    self.changed();
                }
                if self.no_ffplay && !self.mute {
                    match &self.sound_install {
                        None => {
                            if ui.button(crate::i18n::t("installer le son (ffplay)")).on_hover_text(crate::i18n::t("La lecture est muette : ffplay manque. Aryadne le télécharge avec ffmpeg.")).clicked() {
                                self.sound_install = Some(ffmpeg::install(ui.ctx().clone()));
                            }
                        }
                        Some(j) => {
                            let st = j.lock().map(|s| s.clone()).unwrap_or_default();
                            if let Some(e) = &st.error {
                                ui.label(RichText::new(e).color(p.missing).size(11.0));
                            } else if st.done {
                                self.no_ffplay = false;
                                self.sound_install = None;
                                restart = self.play.is_some();
                            } else {
                                ui.label(RichText::new(&st.message).size(11.0).color(p.muted));
                                ui.spinner();
                            }
                        }
                    }
                }
            });
        });
        if restart {
            self.start(&ctx);
        }
        ui.add_space(8.0);
        // ---- L'image, centrée, à la plus grande taille qui laisse la place à la barre de lecture
        let aspect = self.project.width as f32 / self.project.height.max(1) as f32;
        let avail = ui.available_size() - vec2(0.0, 64.0);
        let size = if avail.x / avail.y.max(1.0) > aspect { vec2(avail.y * aspect, avail.y) } else { vec2(avail.x, avail.x / aspect) };
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), size.y.max(40.0)), Sense::click_and_drag());
        let img = Rect::from_center_size(r.center(), size);
        ui.painter().rect_filled(img, 0.0, Color32::BLACK);
        ui.painter().rect_stroke(img, 0.0, Stroke::new(1.0, p.line), egui::StrokeKind::Outside);
        match &self.tex {
            Some(t) if !self.project.clips.is_empty() => {
                ui.painter().image(t.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            _ => {
                ui.painter().text(img.center(), egui::Align2::CENTER_CENTER, crate::i18n::t("Glisse des vidéos, des images ou des sons dans la timeline"), egui::FontId::proportional(13.0), Color32::from_gray(150));
            }
        }
        // Le clip choisi : cadre et huit poignées ; glisser dedans le déplace, un coin change sa taille
        let k = img.width() / self.project.width.max(1) as f32;
        let sel = self.first_selected().filter(|&i| self.project.clips[i].visual());
        let mut corner_hit = false;
        if let Some(i) = sel {
            let c = &self.project.clips[i];
            if self.playhead >= c.start && self.playhead <= c.end() {
                let center = img.center() + vec2(c.x, c.y) * k;
                let half = vec2(img.width(), img.height()) * 0.5 * c.scale;
                let bx = Rect::from_center_size(center, half * 2.0);
                let pt = ui.painter().with_clip_rect(img.expand(6.0));
                pt.rect_stroke(bx, 0.0, Stroke::new(1.0, p.text), egui::StrokeKind::Middle);
                let pts = [bx.left_top(), bx.right_top(), bx.left_bottom(), bx.right_bottom(), bx.center_top(), bx.center_bottom(), bx.left_center(), bx.right_center()];
                for (n, h) in pts.iter().enumerate() {
                    let hr = Rect::from_center_size(*h, vec2(8.0, 8.0));
                    pt.rect_filled(hr, 0.0, p.text);
                    pt.rect_stroke(hr, 0.0, Stroke::new(1.0, Color32::BLACK), egui::StrokeKind::Inside);
                    if n < 4 && resp.hover_pos().is_some_and(|q| hr.expand(4.0).contains(q)) {
                        corner_hit = true;
                        ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
                    }
                }
                if resp.drag_started() && self.tool != Tool::Type {
                    if let Some(at) = resp.interact_pointer_pos() {
                        if pts[..4].iter().any(|h| (*h - at).length() < 10.0) {
                            self.monitor_scale = Some((c.id, (at - center).length().max(4.0), c.scale));
                            corner_hit = true;
                        }
                    }
                }
            }
        }
        if resp.drag_started() && self.tool != Tool::Type && self.monitor_scale.is_none() && !corner_hit {
            if let (Some(i), Some(at)) = (sel, resp.interact_pointer_pos()) {
                let c = &self.project.clips[i];
                self.monitor_drag = Some((c.id, at, c.x, c.y));
                self.stop();
            }
        }
        if let (Some((id, d0, s0)), Some(at)) = (self.monitor_scale, resp.interact_pointer_pos()) {
            if let Some(i) = self.project.index(id) {
                let c = &self.project.clips[i];
                let center = img.center() + vec2(c.x, c.y) * k;
                let s = (s0 * (at - center).length() / d0).clamp(0.02, 20.0);
                let c = &mut self.project.clips[i];
                c.scale = (s * 1000.0).round() / 1000.0;
                if !c.keys.is_empty() && self.playhead >= c.start && self.playhead <= c.end() {
                    c.set_key(self.playhead);
                }
                self.rev += 1;
            }
        }
        if let (Some((id, at0, x0, y0)), Some(at)) = (self.monitor_drag, resp.interact_pointer_pos()) {
            if let Some(i) = self.project.index(id) {
                let d = (at - at0) / k.max(1e-3);
                let c = &mut self.project.clips[i];
                (c.x, c.y) = ((x0 + d.x).round(), (y0 + d.y).round());
                if !c.keys.is_empty() && self.playhead >= c.start && self.playhead <= c.end() {
                    c.set_key(self.playhead);
                }
                self.rev += 1;
            }
        }
        if resp.drag_stopped() && (self.monitor_drag.take().is_some() | self.monitor_scale.take().is_some()) {
            self.changed();
        }
        if resp.clicked() && self.tool == Tool::Type {
            if let Some(at) = resp.interact_pointer_pos() {
                let t = self.playhead;
                self.add_title(t, None);
                if let Some(i) = self.first_selected() {
                    let d = (at - img.center()) / k.max(1e-3);
                    (self.project.clips[i].x, self.project.clips[i].y) = (d.x.round(), d.y.round());
                    self.changed();
                }
            }
        }
        // ---- Barre de lecture flottante : une pilule centrée sous l'image
        let dur = self.project.duration();
        let playing = self.play.is_some();
        ui.add_space(10.0);
        let pill_w = 400.0f32.min(ui.available_width());
        let (pr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
        let pill = Rect::from_center_size(pr.center(), vec2(pill_w, 40.0));
        ui.painter().rect(pill, 20.0, p.panel, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
        let mut pui = ui.new_child(egui::UiBuilder::new().max_rect(pill.shrink2(vec2(10.0, 4.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        pui.spacing_mut().item_spacing.x = 4.0;
        let ib = |ui: &mut egui::Ui, icon: &str, tip: &str, on: bool| ui.add(egui::Button::new(RichText::new(icon).size(14.0).color(if on { p.accent } else { p.muted })).frame(false).min_size(vec2(24.0, 24.0))).on_hover_text(crate::i18n::t(tip)).clicked();
        if ib(&mut pui, crate::lucide::CHEVRON_LEFT, "image précédente (←)", false) {
            let t = seconds(self.playhead - 1.0 / fps, fps);
            self.seek(t);
        }
        let (pb, presp) = pui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
        pui.painter().circle_filled(pb.center(), 16.0, p.text);
        pui.painter().text(pb.center() + vec2(if playing { 0.0 } else { 1.0 }, 0.0), egui::Align2::CENTER_CENTER, if playing { crate::lucide::PAUSE } else { crate::lucide::PLAY }, egui::FontId::proportional(14.0), Color32::from_rgb(0x10, 0x0a, 0x0d));
        if presp.on_hover_text(crate::i18n::t("lire / arrêter (Espace)")).clicked() {
            if playing {
                self.stop();
            } else {
                self.start(&ctx);
            }
        }
        if ib(&mut pui, crate::lucide::CHEVRON_RIGHT, "image suivante (→)", false) {
            let t = seconds(self.playhead + 1.0 / fps, fps);
            self.seek(t);
        }
        pui.label(RichText::new(timecode(self.playhead, fps)).monospace().size(13.0).color(p.text));
        pui.label(RichText::new(format!("/ {}", timecode(dur, fps))).size(11.0).color(p.muted));
        pui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ib(ui, crate::lucide::CAMERA, "exporter l'image (PNG à côté du montage)", false) {
                self.export_frame();
            }
            let mute = self.mute;
            if ib(ui, if mute { crate::lucide::VOLUME_X } else { crate::lucide::VOLUME_2 }, if mute { "rétablir le son de la lecture" } else { "couper le son de la lecture" }, false) {
                self.mute = !self.mute;
                if self.play.is_some() {
                    self.start(&ctx);
                }
            }
            if ib(ui, "]", "marquer la sortie (O)", self.project.mark_out.is_some()) {
                self.project.mark_out = Some(self.playhead);
                self.changed();
            }
            if ib(ui, "[", "marquer l'entrée (I)", self.project.mark_in.is_some()) {
                self.project.mark_in = Some(self.playhead);
                self.changed();
            }
        });
        if let Some(e) = &self.error {
            ui.colored_label(p.missing, e);
        }
        if let Some(m) = &self.message {
            ui.label(RichText::new(m).color(p.muted).size(10.5));
        }
    }

    /// Inspecteur en cartes, comme la maquette : le clip choisi, puis Cadre, Vitesse, Look, Son,
    /// et le passage vers le plan suivant ; les options d'effet détaillées restent dans l'onglet Avancé
    fn clip_cards(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let ctx = ui.ctx().clone();
        let fps = self.project.fps.max(1.0);
        let Some(i) = self.first_selected() else {
            ui.add_space(20.0);
            ui.label(RichText::new(crate::i18n::t("Choisis un plan dans la timeline : ses réglages s'affichent ici.")).color(p.muted).size(12.0));
            return;
        };
        let mut ch = false;
        let mut freeze = false;
        // ---- En-tête : vignette, nom, chapitre et durée
        {
            let c = self.project.clips[i].clone();
            ui.horizontal(|ui| {
                let (tr, _) = ui.allocate_exact_size(vec2(56.0, 32.0), Sense::hover());
                let path = self.media_path(&c.file);
                match (c.kind.as_str(), self.strips.thumb(&path, c.src_in + 0.1, &ctx)) {
                    ("video" | "image", Some(t)) => {
                        ui.painter().add(egui::epaint::RectShape::filled(tr, 6.0, Color32::WHITE).with_texture(t, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))));
                    }
                    _ => {
                        ui.painter().rect_filled(tr, 6.0, clip_color(&c).gamma_multiply(0.5));
                    }
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(RichText::new(&c.name).size(14.0).family(crate::theme::bold()));
                    let chap = self.project.chapters.iter().filter(|h| h.start <= c.start + 1e-3).last().map(|h| format!("{} · ", h.name)).unwrap_or_default();
                    ui.label(RichText::new(format!("{chap}{} → {} · {:.1} s", timecode(c.start, fps), timecode(c.end(), fps), c.len())).size(11.0).color(p.muted));
                });
            });
        }
        ui.add_space(8.0);
        let card = |ui: &mut egui::Ui, title: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
            egui::Frame::new().fill(p.raised).stroke(Stroke::new(1.0, p.line)).corner_radius(6).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.label(RichText::new(crate::i18n::t(title)).size(12.5).family(crate::theme::bold()));
                body(ui);
            });
            ui.add_space(8.0);
        };
        let chip = |ui: &mut egui::Ui, label: &str, on: bool| ui.add(egui::Button::new(RichText::new(label).size(12.0).color(if on { p.text } else { p.muted })).fill(if on { p.selection } else { Color32::TRANSPARENT }).stroke(Stroke::new(1.0, p.line)).corner_radius(12).min_size(vec2(0.0, 24.0))).clicked();
        let field = |ui: &mut egui::Ui, label: &str, v: &mut f32, speed: f64, range: std::ops::RangeInclusive<f32>, suffix: &str, k: f32| -> bool {
            let mut changed = false;
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.label(RichText::new(crate::i18n::t(label)).size(11.0).color(p.muted));
                let mut shown = *v * k;
                let (lo, hi) = (*range.start() * k, *range.end() * k);
                if ui.add_sized([ui.available_width(), 24.0], egui::DragValue::new(&mut shown).speed(speed).range(lo..=hi).suffix(suffix).max_decimals(1)).changed() {
                    *v = shown / k;
                    changed = true;
                }
            });
            changed
        };
        let is_visual = self.project.clips[i].visual();
        if is_visual {
            card(ui, "Cadre", &mut |ui| {
                let c = &mut self.project.clips[i];
                let w = (ui.available_width() - 12.0) / 3.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.allocate_ui(vec2(w, 44.0), |ui| ch |= field(ui, "Position X", &mut c.x, 1.0, -20000.0..=20000.0, "", 1.0));
                    ui.allocate_ui(vec2(w, 44.0), |ui| ch |= field(ui, "Position Y", &mut c.y, 1.0, -20000.0..=20000.0, "", 1.0));
                    ui.allocate_ui(vec2(w, 44.0), |ui| ch |= field(ui, "Échelle", &mut c.scale, 0.5, 0.01..=20.0, " %", 100.0));
                });
                ui.horizontal(|ui| {
                    ui.allocate_ui(vec2(w, 44.0), |ui| ch |= field(ui, "Opacité", &mut c.opacity, 0.5, 0.0..=1.0, " %", 100.0));
                    if chip(ui, &crate::i18n::t("Recentrer"), false) {
                        (c.x, c.y, c.scale) = (0.0, 0.0, 1.0);
                        ch = true;
                    }
                });
            });
        }
        card(ui, "Vitesse", &mut |ui| {
            let c = &mut self.project.clips[i];
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (label, v) in [("0,5×", 0.5f32), ("1×", 1.0), ("2×", 2.0), ("4×", 4.0)] {
                    if chip(ui, label, (c.speed - v).abs() < 1e-3) {
                        c.speed = v;
                        ch = true;
                    }
                }
            });
            let mut pct = c.speed * 100.0;
            if ui.add(egui::Slider::new(&mut pct, 10.0..=400.0).suffix(" %").logarithmic(true)).changed() {
                c.speed = pct / 100.0;
                ch = true;
            }
            if c.kind == "video" {
                let b = egui::Button::new(RichText::new(format!("{}  {}", crate::lucide::PAUSE, crate::i18n::t("Arrêt sur image"))).size(12.0));
                if ui.add(b).on_hover_text(crate::i18n::t("Fige l'image de la tête de lecture pendant 2 secondes ; la suite du montage recule d'autant")).clicked() {
                    freeze = true;
                }
            }
        });
        if is_visual {
            card(ui, "Look", &mut |ui| {
                let c = &mut self.project.clips[i];
                let looks: [(&str, f32, f32, f32); 4] = [("Naturel", 0.0, 1.0, 1.0), ("Doux", 0.03, 0.88, 0.9), ("Contrasté", 0.0, 1.2, 1.12), ("Noir et blanc", 0.0, 1.1, 0.0)];
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (name, b, k, sat) in looks {
                        let on = (c.brightness - b).abs() < 1e-3 && (c.contrast - k).abs() < 1e-3 && (c.saturation - sat).abs() < 1e-3;
                        if chip(ui, &crate::i18n::t(name), on) {
                            (c.brightness, c.contrast, c.saturation) = (b, k, sat);
                            ch = true;
                        }
                    }
                });
                for (label, v, r) in [("Lumière", &mut c.brightness, -1.0..=1.0), ("Contraste", &mut c.contrast, 0.0..=3.0), ("Saturation", &mut c.saturation, 0.0..=3.0)] {
                    ui.horizontal(|ui| {
                        let (lr, _) = ui.allocate_exact_size(vec2(72.0, 20.0), Sense::hover());
                        ui.painter().text(lr.left_center(), egui::Align2::LEFT_CENTER, crate::i18n::t(label), egui::FontId::proportional(12.0), p.muted);
                        ui.spacing_mut().slider_width = (ui.available_width() - 56.0).max(60.0);
                        ch |= ui.add(egui::Slider::new(v, r).max_decimals(2)).changed();
                    });
                }
            });
        }
        if self.project.clips[i].sounds() || self.project.linked(i).iter().any(|&k| self.project.clips[k].sounds()) {
            card(ui, "Son", &mut |ui| {
                let targets: Vec<usize> = std::iter::once(i).chain(self.project.linked(i)).filter(|&k| self.project.clips[k].sounds()).collect();
                if let Some(&k) = targets.first() {
                    let mut db = 20.0 * self.project.clips[k].volume.max(1e-4).log10();
                    ui.horizontal(|ui| {
                        let (lr, _) = ui.allocate_exact_size(vec2(72.0, 20.0), Sense::hover());
                        ui.painter().text(lr.left_center(), egui::Align2::LEFT_CENTER, crate::i18n::t("Volume"), egui::FontId::proportional(12.0), p.muted);
                        ui.spacing_mut().slider_width = (ui.available_width() - 64.0).max(60.0);
                        if ui.add(egui::Slider::new(&mut db, -40.0..=12.0).suffix(" dB").max_decimals(1)).changed() {
                            for &t in &targets {
                                self.project.clips[t].volume = 10f32.powf(db / 20.0);
                            }
                            ch = true;
                        }
                    });
                    let c = &mut self.project.clips[k];
                    let w = (ui.available_width() - 6.0) / 2.0;
                    ui.horizontal(|ui| {
                        ui.allocate_ui(vec2(w, 44.0), |ui| ch |= field(ui, "Fondu d'entrée", &mut c.fade_in, 0.02, 0.0..=30.0, " s", 1.0));
                        ui.allocate_ui(vec2(w, 44.0), |ui| ch |= field(ui, "Fondu de sortie", &mut c.fade_out, 0.02, 0.0..=30.0, " s", 1.0));
                    });
                }
            });
        }
        // ---- Vers le plan suivant (même piste) : coupe franche ou fondu enchaîné
        let next = {
            let c = &self.project.clips[i];
            self.project.clips.iter().enumerate().filter(|(k, a)| *k != i && a.track == c.track && a.audio == c.audio && a.start >= c.start + 1e-3).min_by(|a, b| a.1.start.total_cmp(&b.1.start)).map(|(k, _)| k)
        };
        if let Some(n) = next {
            card(ui, "Vers le plan suivant", &mut |ui| {
                let cur = self.project.clips[n].xfade;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if chip(ui, &crate::i18n::t("Coupe"), cur < 1e-3) && cur > 1e-3 {
                        for k in std::iter::once(n).chain(self.project.linked(n)) {
                            self.project.clips[k].xfade = 0.0;
                        }
                        ch = true;
                    }
                    for d in [0.4f32, 1.0] {
                        if chip(ui, &format!("{} {}", crate::i18n::t("Fondu"), format!("{d:.1} s").replace('.', ",")), (cur - d).abs() < 1e-3) {
                            ch |= self.project.crossfade(n, d);
                        }
                    }
                });
            });
        }
        // ---- Ailleurs : le plan dans d'autres outils
        ui.horizontal_wrapped(|ui| {
            if is_visual && ui.add(egui::Button::new(egui::RichText::new(crate::lucide::WORKFLOW).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Étalonner en nodes")).clicked() {
                self.new_grade(i);
            }
        });
        if ch {
            self.changed();
        }
        if freeze {
            self.freeze_frame(i);
        }
    }

    /// Arrêt sur image du plan `i` à la tête de lecture : l'image est extraite par ffmpeg dans « arrêts/ »
    /// à côté du montage, puis posée comme plan fixe de 2 secondes
    fn freeze_frame(&mut self, i: usize) {
        let c = self.project.clips[i].clone();
        let t = self.playhead.clamp(c.start, (c.end() - 1.0 / self.project.fps.max(1.0)).max(c.start));
        let src_t = c.src_in + (t - c.start) * c.speed;
        let Some(ff) = crate::ffmpeg::find("ffmpeg") else {
            self.message = Some(crate::i18n::t("ffmpeg n'est pas installé."));
            return;
        };
        let dir = self.path.parent().map(|d| d.join("arrêts")).unwrap_or_default();
        let _ = std::fs::create_dir_all(&dir);
        let name = format!("{} {:.2}.png", c.name, src_t);
        let out = dir.join(&name);
        let o = crate::ffmpeg::command(ff).args(["-v", "error", "-y", "-ss", &format!("{src_t:.3}"), "-i"]).arg(self.media_path(&c.file)).args(["-frames:v", "1"]).arg(&out).output();
        if !o.as_ref().is_ok_and(|o| o.status.success()) || !out.exists() {
            self.message = Some(crate::i18n::t("L'image n'a pas pu être extraite du plan."));
            return;
        }
        if let Some(k) = self.project.freeze_frame(i, t, 2.0, format!("arrêts/{name}")) {
            self.sel = vec![self.project.clips[k].id];
            self.changed();
        }
    }

    // ------------------------------------------------------------------ timeline

    fn timeline_panel(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        // Barre de 40 px comme la maquette : outils groupés, magnétique, repères, l'IA en pastilles, le zoom
        let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
        ui.painter().hline(hr.x_range(), hr.bottom() - 0.5, Stroke::new(1.0, p.line));
        let mut hui = ui.new_child(egui::UiBuilder::new().max_rect(hr.shrink2(vec2(10.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        hui.set_clip_rect(hr);
        {
            let v = &mut hui.style_mut().visuals.widgets;
            v.inactive.weak_bg_fill = Color32::TRANSPARENT;
            v.inactive.bg_stroke = Stroke::NONE;
            v.hovered.weak_bg_fill = p.raised;
            v.hovered.bg_stroke = Stroke::NONE;
        }
        hui.spacing_mut().item_spacing.x = 6.0;
        egui::Frame::new().fill(p.field).stroke(Stroke::new(1.0, p.line)).corner_radius(6).inner_margin(2).show(&mut hui, |ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            for (tool, key, tip) in TOOLS {
                let tip = format!("{} ({key})", crate::i18n::t(tip));
                if icon_button(ui, Icon::Tool(tool), self.tool == tool, &tip).clicked() {
                    self.tool = tool;
                }
            }
        });
        let mag = self.project.magnetic;
        let b = egui::Button::new(RichText::new(crate::lucide::MAGNET).size(14.0).color(if mag { p.text } else { p.muted })).fill(if mag { p.selection } else { Color32::TRANSPARENT }).corner_radius(12);
        if hui.add(b).on_hover_text(crate::i18n::t("Timeline magnétique : la piste V1 raconte l'histoire, sans trous ; titres, calques et sons restent accrochés à leur plan")).clicked() {
            self.project.magnetic = !mag;
            self.changed();
        }
        if icon_button(&mut hui, Icon::Magnet, self.snap, "aimanter aux coupes et à la tête de lecture (S)").clicked() {
            self.snap = !self.snap;
        }
        if icon_button(&mut hui, Icon::Link, self.linked_sel, "sélection liée : l'image et le son d'une vidéo bougent ensemble (Alt pour les séparer)").clicked() {
            self.linked_sel = !self.linked_sel;
        }
        if icon_button(&mut hui, Icon::Marker, false, "ajouter un repère (M)").clicked() {
            let t = self.playhead;
            if !self.project.markers.iter().any(|m| (m - t).abs() < 1e-3) {
                self.project.markers.push(t);
                self.changed();
            }
        }
        if hui.add(egui::Button::new(RichText::new(crate::lucide::BOOKMARK).size(14.0))).on_hover_text(crate::i18n::t("Commencer un chapitre à la tête de lecture (clic droit sur sa bande : renommer, supprimer)")).clicked() {
            let t = self.playhead;
            if !self.project.chapters.iter().any(|c| (c.start - t).abs() < 1e-3) {
                let n = self.project.chapters.len() + 1;
                self.project.chapters.push(crate::montage::Chapter { start: t, name: format!("{} {n}", crate::i18n::t("Chapitre")) });
                self.project.chapters.sort_by(|a, b| a.start.total_cmp(&b.start));
                self.changed();
            }
        }
        if hui.add_enabled(!self.history.is_empty(), egui::Button::new(RichText::new(crate::lucide::UNDO_2).size(13.0))).on_hover_text(crate::i18n::t("annuler (Ctrl+Z)")).clicked() {
            self.undo(false);
        }
        if hui.add_enabled(!self.redo.is_empty(), egui::Button::new(RichText::new(crate::lucide::REDO_2).size(13.0))).on_hover_text(crate::i18n::t("rétablir (Ctrl+Maj+Z)")).clicked() {
            self.undo(true);
        }
        hui.add_space(8.0);
        hui.toggle_value(&mut self.curves_open, RichText::new(crate::lucide::SPLINE).size(14.0)).on_hover_text(crate::i18n::t("Éditeur de courbes : les clés du clip choisi (position, taille, opacité), tangentes comprises"));
        if self.curves_open {
            let mut open = true;
            let pick = self.sel.first().and_then(|id| self.project.index(*id));
            let name = pick.map(|i| self.project.clips[i].name.clone()).unwrap_or_default();
            let fps = self.project.fps as f32;
            egui::Window::new(format!("{} · {name}", crate::i18n::t("Éditeur de courbes"))).id(egui::Id::unique("montage-curves")).open(&mut open).resizable(true).default_size(vec2(780.0, 360.0)).show(&hui.ctx().clone(), |ui| {
                let Some(i) = pick else {
                    ui.label(crate::i18n::t("Choisis un clip (avec des clés) pour voir ses courbes."));
                    return;
                };
                let c = &self.project.clips[i];
                self.curve_ed.show_for(&c.id.to_string());
                let (start, len) = (c.start, c.len());
                let mut cv = crate::montage::clip_curves(c);
                // Les clés sont en temps du clip : la tête de lecture aussi
                let out = self.curve_ed.ui(ui, &mut cv, self.playhead - start, (0.0, len), fps);
                if out.changed {
                    crate::montage::apply_clip_curves(&mut self.project.clips[i], &cv);
                    self.changed();
                    self.shown = None;
                }
                if let Some(t) = out.seek {
                    self.playhead = (start + t).max(0.0);
                    self.shown = None;
                }
            });
            if !open {
                self.curves_open = false;
            }
        }
        // Actions automatiques : les deux premières analysent le son, les deux autres attendent l'Assistant
        self.poll_smart();
        let busy = self.smart_rx.is_some();
        let tips = [
            "Retire les passages sans voix du clip choisi (et de son son lié) ; la suite de ces pistes se resserre",
            "Choisis la musique : les plans de V1 sont mis bout à bout et chaque coupe tombe sur un temps fort",
            "",
            "Crée une copie verticale du montage pour Reels, TikTok et Shorts : chaque plan remplit le cadre 9:16",
        ];
        let soon = crate::i18n::t("Pas encore branché : viendra avec l'Assistant qui agit dans l'application");
        let mut run: Option<usize> = None;
        if hui.available_width() > 640.0 {
            for (k, label) in ["Couper les silences", "Couper sur le rythme", "Sous-titres", "Version 9:16"].into_iter().enumerate() {
                let txt = if busy && k < 2 { format!("{} {}", crate::lucide::LOADER, crate::i18n::t(label)) } else { format!("{} {}", crate::lucide::SPARKLES, crate::i18n::t(label)) };
                let b = egui::Button::new(RichText::new(txt).size(12.0)).stroke(Stroke::new(1.0, p.line)).corner_radius(13).min_size(vec2(0.0, 26.0));
                if k < 2 || k == 3 {
                    if hui.add_enabled(!busy || k == 3, b).on_hover_text(crate::i18n::t(tips[k])).clicked() {
                        run = Some(k);
                    }
                } else {
                    hui.add_enabled(false, b).on_disabled_hover_text(&soon);
                }
            }
        } else {
            // Peu de place : dans un menu
            egui::containers::menu::MenuButton::new(RichText::new(format!("{} {} {}", crate::lucide::SPARKLES, crate::i18n::t("Auto"), crate::lucide::CHEVRON_DOWN)).size(12.0)).ui(&mut hui, |ui| {
                for (k, label) in ["Couper les silences", "Couper sur le rythme"].into_iter().enumerate() {
                    if ui.add_enabled(!busy, egui::Button::new(crate::i18n::t(label))).on_hover_text(crate::i18n::t(tips[k])).clicked() {
                        run = Some(k);
                        ui.close();
                    }
                }
                if ui.button(crate::i18n::t("Version 9:16")).on_hover_text(crate::i18n::t(tips[3])).clicked() {
                    run = Some(3);
                    ui.close();
                }
                ui.add_enabled(false, egui::Button::new(crate::i18n::t("Sous-titres"))).on_disabled_hover_text(&soon);
            });
        }
        match run {
            Some(3) => self.make_vertical(),
            Some(k) => self.start_smart(k == 1, &hui.ctx().clone()),
            None => {}
        }
        if hui.available_width() > 150.0 {
        hui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut z = self.zoom.ln();
            ui.spacing_mut().slider_width = 90.0;
            if ui.add(egui::Slider::new(&mut z, 2f32.ln()..=800f32.ln()).show_value(false)).on_hover_text(crate::i18n::t("Zoom de la timeline (Ctrl + molette)")).changed() {
                self.zoom = z.exp();
            }
            ui.label(RichText::new(crate::i18n::t("Zoom")).size(11.0).color(p.muted));
        });
        }
        self.timeline(ui);
    }

    fn timeline(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let ctx = ui.ctx().clone();
        self.strips.poll(&ctx);
        if crate::motion::generation() != self.motion_gen {
            self.motion_gen = crate::motion::generation();
            self.strips.refresh_motion();
            // L'image affichée est redemandée : elle peut venir d'une composition qui vient de changer
            self.shown = None;
        }
        let fps = self.project.fps.max(1.0);
        let (full, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let area = Rect::from_min_max(pos2(full.left() + HEAD_W, full.top()), pos2(full.right(), full.bottom() - SCROLL_H));
        let (nv, na) = (self.project.lanes(false), self.project.lanes(true));
        let row_h = ((area.height() - RULER_H - DIVIDER - 4.0) / (nv + na) as f32).clamp(22.0, 54.0);
        let v_top = area.top() + RULER_H;
        let a_top = v_top + nv as f32 * row_h + DIVIDER;
        let dur = self.project.duration();
        if self.fit_zoom && area.width() > 50.0 {
            self.fit_zoom = false;
            self.zoom = ((area.width() - 40.0) / dur.max(5.0)).clamp(2.0, 800.0);
            self.scroll = 0.0;
        }
        let resp = ui.interact(area, ui.scope_id().with("montage-timeline"), Sense::click_and_drag());
        // Molette : défiler ; Ctrl ou Alt + molette : zoomer autour du curseur
        if resp.hovered() {
            let (scroll, zoom, at, alt) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos(), i.modifiers.alt));
            let z = if alt && scroll.y != 0.0 { (scroll.y / 200.0).exp() } else { zoom };
            if z != 1.0 {
                let t = at.map(|a| self.scroll + (a.x - area.left()) / self.zoom).unwrap_or(self.scroll);
                self.zoom = (self.zoom * z).clamp(2.0, 800.0);
                self.scroll = (t - at.map(|a| (a.x - area.left()) / self.zoom).unwrap_or(0.0)).max(0.0);
            } else if scroll != egui::Vec2::ZERO {
                self.scroll = (self.scroll - (scroll.x + scroll.y) / self.zoom).max(0.0);
            }
        }
        let g = Geo { area, v_top, a_top, row_h, nv, na, zoom: self.zoom, scroll: self.scroll };
        // Lecture : la timeline suit la tête de lecture
        if self.play.is_some() {
            let x = g.x_of(self.playhead);
            if x > area.right() - 20.0 || x < area.left() {
                self.scroll = (self.playhead - 1.0).max(0.0);
            }
        }
        let painter = ui.painter_at(area);
        let hp = ui.painter_at(Rect::from_min_max(full.min, pos2(area.left(), area.bottom())));
        // ---- En-têtes de pistes : cible, verrou, œil (V) ; cible, verrou, muet, solo (A)
        hp.rect_filled(Rect::from_min_max(full.min, pos2(area.left(), full.bottom())), 0.0, p.panel);
        let mut header_click: Option<(bool, usize, u8)> = None;
        let mut header_menu: Option<(bool, usize)> = None;
        for audio in [false, true] {
            for k in 0..self.project.lanes(audio) {
                let y = g.y_of(audio, k);
                let r = Rect::from_min_size(pos2(full.left(), y), vec2(HEAD_W - 1.0, row_h));
                if r.bottom() < area.top() + RULER_H || r.top() > area.bottom() {
                    continue;
                }
                let st = self.project.state(audio, k);
                let target = if audio { self.atarget == k } else { self.vtarget == k };
                let tr = Rect::from_min_size(pos2(r.left() + 4.0, r.center().y - 9.0), vec2(28.0, 18.0));
                hp.rect_filled(tr, 3.0, if target { p.accent } else { p.bg });
                hp.text(tr.center(), egui::Align2::CENTER_CENTER, format!("{}{}", if audio { "A" } else { "V" }, k + 1), egui::FontId::monospace(10.5), if target { p.on_accent } else { p.muted });
                let mut bx = tr.right() + 6.0;
                let mut buttons: Vec<(Rect, u8)> = vec![];
                for (code, on) in if audio { vec![(1u8, st.lock), (2, st.off), (3, st.solo)] } else { vec![(1u8, st.lock), (2, !st.off)] } {
                    let br = Rect::from_min_size(pos2(bx, r.center().y - 9.0), vec2(20.0, 18.0));
                    let col = if on { if code == 2 && audio { Color32::from_rgb(90, 180, 120) } else if code == 3 { Color32::from_rgb(230, 190, 60) } else { p.accent } } else { p.muted };
                    if on && (code != 2 || audio) {
                        hp.rect_filled(br, 3.0, col.gamma_multiply(0.18));
                    }
                    match (code, audio) {
                        (1, _) => draw_icon(&hp, br.shrink(4.0), Icon::Lock, if on { p.accent } else { p.muted.gamma_multiply(0.7) }),
                        (2, false) => draw_icon(&hp, br.shrink(3.0), Icon::Eye, if on { p.text } else { p.muted.gamma_multiply(0.5) }),
                        (2, true) => {
                            hp.text(br.center(), egui::Align2::CENTER_CENTER, "M", egui::FontId::proportional(11.0), if on { col } else { p.muted });
                        }
                        _ => {
                            hp.text(br.center(), egui::Align2::CENTER_CENTER, "S", egui::FontId::proportional(11.0), if on { col } else { p.muted });
                        }
                    }
                    buttons.push((br, code));
                    bx += 22.0;
                }
                hp.line_segment([pos2(r.left(), r.bottom()), pos2(r.right(), r.bottom())], Stroke::new(1.0, p.line.gamma_multiply(0.6)));
                let hr = ui.interact(r, ui.scope_id().with(("track-head", audio, k)), Sense::click());
                if hr.clicked() {
                    if let Some(at) = hr.interact_pointer_pos() {
                        let code = buttons.iter().find(|(b, _)| b.contains(at)).map(|b| b.1).unwrap_or(if tr.expand(2.0).contains(at) { 0 } else { 9 });
                        header_click = Some((audio, k, code));
                    }
                }
                if hr.secondary_clicked() {
                    header_menu = Some((audio, k));
                }
                hr.context_menu(|ui| {
                    if ui.button(crate::i18n::t("ajouter une piste")).clicked() {
                        header_menu = Some((audio, usize::MAX));
                        ui.close();
                    }
                    let empty = !self.project.clips.iter().any(|c| c.audio == audio && c.track == k);
                    if ui.add_enabled(empty && self.project.lanes(audio) > 1, egui::Button::new(crate::i18n::t("supprimer la piste (vide)"))).clicked() {
                        header_menu = Some((audio, usize::MAX - 1 - k));
                        ui.close();
                    }
                });
            }
        }
        hp.line_segment([pos2(area.left() - 0.5, full.top()), pos2(area.left() - 0.5, full.bottom())], Stroke::new(1.0, p.line));
        if let Some((audio, k, code)) = header_click {
            match code {
                0 => {
                    if audio {
                        self.atarget = k;
                    } else {
                        self.vtarget = k;
                    }
                }
                1 => self.project.state_mut(audio, k).lock ^= true,
                2 => self.project.state_mut(audio, k).off ^= true,
                3 => self.project.state_mut(audio, k).solo ^= true,
                _ => {}
            }
            if (1..=3).contains(&code) {
                self.changed();
            }
        }
        if let Some((audio, k)) = header_menu {
            if k == usize::MAX {
                if audio {
                    self.project.atracks = self.project.lanes(true) + 1;
                } else {
                    self.project.tracks = self.project.lanes(false) + 1;
                }
                self.changed();
            } else if k < usize::MAX - 1 && k > usize::MAX / 2 {
                let del = usize::MAX - 1 - k;
                for c in self.project.clips.iter_mut().filter(|c| c.audio == audio && c.track > del) {
                    c.track -= 1;
                }
                let states = if audio { &mut self.project.astate } else { &mut self.project.vstate };
                if del < states.len() {
                    states.remove(del);
                }
                if audio {
                    self.project.atracks = self.project.lanes(true) - 1;
                } else {
                    self.project.tracks = self.project.lanes(false) - 1;
                }
                self.changed();
            }
        }

        // ---- Fond des pistes et règle
        for audio in [false, true] {
            for k in 0..self.project.lanes(audio) {
                let y = g.y_of(audio, k);
                let r = Rect::from_min_max(pos2(area.left(), y), pos2(area.right(), y + row_h));
                let st = self.project.state(audio, k);
                painter.rect_filled(r, 0.0, p.panel);
                if st.lock {
                    // Piste verrouillée : hachures
                    let mut x = r.left() - (r.left() % 12.0);
                    while x < r.right() {
                        painter.line_segment([pos2(x, r.bottom()), pos2(x + row_h, r.top())], Stroke::new(1.0, p.line.gamma_multiply(0.6)));
                        x += 12.0;
                    }
                }
            }
        }
        // Entre l'image et le son : un simple filet
        painter.hline(area.x_range(), a_top - DIVIDER / 2.0, Stroke::new(1.0, p.line));
        let ruler = Rect::from_min_size(area.min, vec2(area.width(), RULER_H));
        painter.rect_filled(ruler, 0.0, p.panel);
        // Chapitres : une bande nommée de chaque début au suivant
        {
            let colors = [Color32::from_rgb(0xb4, 0xc2, 0x6a), Color32::from_rgb(0xaf, 0x9c, 0xc9), Color32::from_rgb(0x94, 0xb0, 0xd1), Color32::from_rgb(0xe3, 0x9b, 0x6f), Color32::from_rgb(0x83, 0xb9, 0x9e)];
            let n = self.project.chapters.len();
            let mut menu: Option<(usize, u8)> = None;
            for k in 0..n {
                let a = self.project.chapters[k].start;
                let b = self.project.chapters.get(k + 1).map(|c| c.start).unwrap_or(dur.max(a + 1.0));
                let band = Rect::from_min_max(pos2(g.x_of(a) + 1.0, ruler.top() + 3.0), pos2(g.x_of(b) - 1.0, ruler.top() + CHAP_H));
                if band.right() < area.left() || band.left() > area.right() {
                    continue;
                }
                let col = colors[k % colors.len()];
                painter.rect(band, 4.0, col.gamma_multiply(0.2), Stroke::new(1.0, col.gamma_multiply(0.45)), egui::StrokeKind::Inside);
                painter.with_clip_rect(band.intersect(area)).text(pos2(band.left().max(area.left()) + 6.0, band.center().y), egui::Align2::LEFT_CENTER, &self.project.chapters[k].name, egui::FontId::proportional(11.0), p.text);
                let cr = ui.interact(band.intersect(area), ui.scope_id().with(("chapter", k)), Sense::click());
                if cr.double_clicked() {
                    let t = a;
                    self.seek(t);
                }
                cr.context_menu(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.project.chapters[k].name).desired_width(180.0));
                    if ui.button(crate::i18n::t("enregistrer le nom")).clicked() {
                        menu = Some((k, 0));
                        ui.close();
                    }
                    if ui.button(crate::i18n::t("supprimer le chapitre")).clicked() {
                        menu = Some((k, 1));
                        ui.close();
                    }
                });
            }
            match menu {
                Some((k, 1)) => {
                    self.project.chapters.remove(k);
                    self.changed();
                }
                Some(_) => self.changed(),
                None => {}
            }
        }
        // Entrée / sortie de la séquence
        if self.project.mark_in.is_some() || self.project.mark_out.is_some() {
            let (a, b) = (self.project.mark_in.unwrap_or(0.0), self.project.mark_out.unwrap_or(dur.max(self.project.mark_in.unwrap_or(0.0))));
            let r = Rect::from_min_max(pos2(g.x_of(a), ruler.top()), pos2(g.x_of(b), ruler.bottom()));
            painter.rect_filled(r, 0.0, p.accent.gamma_multiply(0.22));
            painter.rect_filled(Rect::from_min_max(pos2(r.left(), v_top), pos2(r.right(), area.bottom())), 0.0, p.accent.gamma_multiply(0.05));
        }
        let step = [1.0 / fps, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 300.0].into_iter().find(|s| s * self.zoom > 70.0).unwrap_or(600.0);
        let mut t = (self.scroll / step).floor() * step;
        while g.x_of(t) < area.right() {
            let x = g.x_of(t);
            painter.line_segment([pos2(x, ruler.bottom() - 7.0), pos2(x, ruler.bottom())], Stroke::new(1.0, p.muted));
            painter.text(pos2(x + 3.0, ruler.top() + CHAP_H + 4.0), egui::Align2::LEFT_TOP, timecode(t, fps), egui::FontId::monospace(10.0), p.muted);
            for q in 1..5 {
                let xq = g.x_of(t + step * q as f32 / 5.0);
                painter.line_segment([pos2(xq, ruler.bottom() - 3.0), pos2(xq, ruler.bottom())], Stroke::new(1.0, p.line));
            }
            t += step;
        }
        for &m in &self.project.markers {
            let x = g.x_of(m);
            draw_icon(&painter, Rect::from_center_size(pos2(x, ruler.bottom() - 7.0), vec2(10.0, 10.0)), Icon::Marker, Color32::from_rgb(90, 190, 110));
        }

        // ---- Souris
        let (alt, shift, ctrl) = ui.input(|i| (i.modifiers.alt, i.modifiers.shift, i.modifiers.command));
        let press = ui.input(|i| i.pointer.press_origin());
        let edge = 6.0;
        let hit_clip = |this: &Editor, at: Pos2| -> Option<usize> { (0..this.project.clips.len()).rev().find(|&i| g.rect_of(&this.project.clips[i]).contains(at)) };
        // Survol : le curseur dit ce que fera un glissement
        if let Some(h) = resp.hover_pos() {
            if h.y > ruler.bottom() {
                if let Some(i) = hit_clip(self, h) {
                    let r = g.rect_of(&self.project.clips[i]);
                    let on_edge = h.x < r.left() + edge || h.x > r.right() - edge;
                    let icon = match self.tool {
                        Tool::Select | Tool::Ripple | Tool::Roll | Tool::Stretch if on_edge => egui::CursorIcon::ResizeColumn,
                        Tool::Razor => egui::CursorIcon::Crosshair,
                        Tool::Slip => egui::CursorIcon::ResizeHorizontal,
                        Tool::Hand => egui::CursorIcon::Grab,
                        Tool::Zoom => egui::CursorIcon::ZoomIn,
                        Tool::Type => egui::CursorIcon::Text,
                        _ => egui::CursorIcon::Default,
                    };
                    ctx.set_cursor_icon(icon);
                } else if self.tool == Tool::Hand {
                    ctx.set_cursor_icon(egui::CursorIcon::Grab);
                }
                // Cutter : un trait montre où la coupe tombera
                if self.tool == Tool::Razor {
                    let t = self.snap_time(&g, g.t_of(h.x), &[]).0;
                    let x = g.x_of(t);
                    painter.line_segment([pos2(x, v_top), pos2(x, area.bottom())], Stroke::new(1.0, p.text));
                }
            }
        }
        if resp.drag_started() {
            if let Some(at) = press {
                self.begin_drag(&g, at, alt, shift);
            }
        }
        if resp.clicked() {
            if let Some(at) = resp.interact_pointer_pos() {
                self.click(&g, at, shift, alt);
            }
        }
        if let (Some(at), true) = (resp.interact_pointer_pos(), resp.dragged()) {
            self.update_drag(&g, at, press.unwrap_or(at), alt);
        }
        if resp.drag_stopped() {
            if let Some(at) = resp.interact_pointer_pos().or(ui.input(|i| i.pointer.latest_pos())) {
                self.end_drag(&g, at, press.unwrap_or(at), shift, ctrl);
            }
        }
        // ---- Glisser-déposer depuis les panneaux et l'explorateur
        self.drops(ui, &g, &resp, &painter, ctrl);

        // ---- Clips
        let sel: HashSet<u64> = self.sel.iter().copied().collect();
        for c in &self.project.clips {
            let r = g.rect_of(c);
            if r.right() < area.left() || r.left() > area.right() || r.bottom() < v_top || r.top() > area.bottom() {
                continue;
            }
            let on = sel.contains(&c.id);
            let base = clip_color(c);
            let hidden = !self.project.audible(c);
            // Comme la maquette : coins de 6 px ; l'image en pellicule, le son en ondes de sa couleur
            let r = r.shrink2(vec2(0.5, 2.0));
            let filmish = c.visual() && matches!(c.kind.as_str(), "video" | "image") && !c.file.is_empty();
            let fill = if c.audio || !filmish { base.gamma_multiply(if hidden { 0.08 } else { 0.22 }) } else { base.gamma_multiply(if hidden { 0.2 } else { 0.45 }) };
            painter.rect_filled(r, 6.0, fill);
            let file = crate::bridge::media(&resolve(&self.path, &c.file));
            let inner = r.shrink(1.0);
            let clip_p = painter.with_clip_rect(inner.intersect(area));
            let label_h = 0.0;
            if c.visual() && matches!(c.kind.as_str(), "video" | "image") && !c.file.is_empty() && row_h > 26.0 {
                let band = Rect::from_min_max(pos2(inner.left(), inner.top() + label_h), inner.max);
                let th = band.height();
                let tw = th * 16.0 / 9.0;
                let mut x = inner.left().max(area.left() - tw);
                x = inner.left() + ((x - inner.left()) / tw).floor() * tw;
                while x < inner.right().min(area.right()) {
                    let t_src = if c.kind == "image" { 0.0 } else { c.src_in + (x - inner.left()) / self.zoom * c.speed };
                    if let Some(tex) = self.strips.thumb(&file, t_src, &ctx) {
                        let cell = Rect::from_min_size(pos2(x, band.top()), vec2(tw, th));
                        // Coins arrondis aux deux bouts du clip seulement
                        let first = x <= inner.left() + 0.5;
                        let last = x + tw >= inner.right() - 0.5;
                        let rad = egui::CornerRadius { nw: if first { 5 } else { 0 }, sw: if first { 5 } else { 0 }, ne: if last { 5 } else { 0 }, se: if last { 5 } else { 0 } };
                        let dim = if hidden { 90 } else { 255 };
                        clip_p.add(egui::epaint::RectShape::filled(cell, rad, Color32::from_gray(dim)).with_texture(tex, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))));
                        clip_p.vline(cell.right(), cell.y_range(), Stroke::new(1.0, Color32::from_black_alpha(90)));
                    }
                    x += tw;
                }
            }
            if c.audio && !c.file.is_empty() {
                if let Some(w) = self.strips.wave(&file, &ctx) {
                    let band = Rect::from_min_max(pos2(inner.left(), inner.top() + label_h), inner.max);
                    let mid = band.center().y;
                    let col = base.gamma_multiply(0.9);
                    let mut x = band.left().max(area.left());
                    while x < band.right().min(area.right()) {
                        let t_src = c.src_in + (x - inner.left()) / self.zoom * c.speed;
                        let v = w.get((t_src * WAVE_RATE) as usize).copied().unwrap_or(0.0) * c.volume.min(2.0);
                        let hh = (v * band.height() * 0.5).min(band.height() * 0.5);
                        clip_p.line_segment([pos2(x, mid - hh), pos2(x, mid + hh)], Stroke::new(1.0, col));
                        x += 2.0;
                    }
                }
            }
            // Transitions : une boîte à l'entrée et à la sortie, comme dans Premiere
            let tr_col = Color32::from_rgba_unmultiplied(70, 60, 110, 200);
            let fin = c.fade_in.max(c.xfade);
            if fin > 0.0 {
                let b = Rect::from_min_size(pos2(r.left(), r.top()), vec2((fin * self.zoom).min(r.width()), (r.height() * 0.5).min(16.0)));
                clip_p.rect_filled(b, 2.0, tr_col);
                clip_p.line_segment([b.left_bottom(), b.right_top()], Stroke::new(1.0, Color32::from_white_alpha(120)));
            }
            if c.fade_out > 0.0 {
                let w = (c.fade_out * self.zoom).min(r.width());
                let b = Rect::from_min_size(pos2(r.right() - w, r.top()), vec2(w, (r.height() * 0.5).min(16.0)));
                clip_p.rect_filled(b, 2.0, tr_col);
                clip_p.line_segment([b.left_top(), b.right_bottom()], Stroke::new(1.0, Color32::from_white_alpha(120)));
            }
            // Clés d'animation : petits losanges en bas
            for k in &c.keys {
                let x = g.x_of(c.start + k.t);
                let y = r.bottom() - 5.0;
                crate::keys::draw(&clip_p, pos2(x, y), 3.5, Some(k.ease), Color32::WHITE, true, false);
            }
            painter.rect_stroke(r, 6.0, Stroke::new(if on { 2.0 } else { 1.0 }, if on { p.text } else { base.gamma_multiply(0.55) }), egui::StrokeKind::Inside);
            if on {
                // Poignées des bords : on y raccourcit le plan
                for x in [r.left() + 2.5, r.right() - 2.5] {
                    painter.rect_filled(Rect::from_center_size(pos2(x, r.center().y), vec2(5.0, r.height() - 4.0)), 2.0, p.text);
                }
            }
            // Nom (et « fx » si un effet couleur est posé), vitesse si changée
            let mut label = c.name.clone();
            if c.speed != 1.0 {
                label = format!("{label}  [{:.0} %]", c.speed * 100.0);
            }
            if c.link != 0 && c.audio {
                label = format!("{} {label}", crate::lucide::LINK);
            }
            let np = painter.with_clip_rect(r.shrink(2.0).intersect(area));
            let fx = !c.grade.is_empty() || c.fx.iter().any(|f| f == "lumetri") || c.brightness != 0.0 || c.contrast != 1.0 || c.saturation != 1.0;
            let mut lx = r.left().max(area.left()) + 4.0;
            if fx {
                let b = Rect::from_min_size(pos2(lx, r.top() + 2.0), vec2(14.0, 11.0));
                np.rect_filled(b, 2.0, Color32::from_rgb(230, 190, 60));
                np.text(b.center(), egui::Align2::CENTER_CENTER, "fx", egui::FontId::proportional(8.5), Color32::BLACK);
                lx += 17.0;
            }
            // Nom dans une pastille sombre, lisible sur l'image comme sur la couleur
            let gl = np.layout_no_wrap(label, egui::FontId::proportional(11.0), p.text);
            let chip = Rect::from_min_size(pos2(lx + 2.0, r.top() + 4.0), gl.size() + vec2(10.0, 2.0));
            if r.width() > 24.0 {
                np.rect_filled(chip, 2.0, Color32::from_rgba_unmultiplied(16, 10, 13, 170));
                np.galley(chip.min + vec2(5.0, 1.0), gl, p.text);
            }
        }

        // ---- Rectangle de sélection, ligne de magnétisme
        if let Some(d) = &self.drag {
            if let Act::Marquee { from } = d.act {
                if let Some(at) = resp.interact_pointer_pos() {
                    let r = Rect::from_two_pos(from, at);
                    painter.rect_filled(r, 0.0, p.accent.gamma_multiply(0.08));
                    painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
                }
            }
            if let Some(t) = d.snapped {
                let x = g.x_of(t);
                painter.line_segment([pos2(x, v_top), pos2(x, area.bottom())], Stroke::new(1.0, p.text));
                painter.add(egui::Shape::convex_polygon(vec![pos2(x - 5.0, v_top), pos2(x + 5.0, v_top), pos2(x, v_top + 6.0)], p.text, Stroke::NONE));
            }
        }

        // ---- Tête de lecture
        let x = g.x_of(self.playhead);
        if x >= area.left() - 1.0 {
            painter.line_segment([pos2(x, ruler.top()), pos2(x, area.bottom())], Stroke::new(1.5, p.accent));
            let tc = timecode(self.playhead, fps);
            let gl = painter.layout_no_wrap(tc, egui::FontId::monospace(10.0), Color32::from_rgb(0x10, 0x0a, 0x0d));
            let bub = Rect::from_center_size(pos2(x, ruler.top() + CHAP_H + 10.0), gl.size() + vec2(12.0, 4.0));
            painter.rect_filled(bub, 8.0, p.accent);
            painter.galley(bub.min + vec2(6.0, 2.0), gl, Color32::BLACK);
        }

        // ---- Barre de défilement (glisser pour se déplacer)
        let sb = Rect::from_min_max(pos2(area.left(), area.bottom()), pos2(area.right(), full.bottom()));
        ui.painter().rect_filled(sb, 0.0, p.panel);
        let span = (dur.max(self.scroll + area.width() / self.zoom) + 2.0).max(1.0);
        let view = Rect::from_min_max(pos2(sb.left() + self.scroll / span * sb.width(), sb.top() + 2.0), pos2(sb.left() + (self.scroll + area.width() / self.zoom) / span * sb.width(), sb.bottom() - 2.0));
        ui.painter().rect_filled(view, 4.0, p.line);
        let sresp = ui.interact(sb, ui.scope_id().with("tl-scroll"), Sense::click_and_drag());
        if sresp.dragged() {
            self.scroll = (self.scroll + sresp.drag_delta().x / sb.width() * span).max(0.0);
        }
        if sresp.double_clicked() {
            self.fit_zoom = true;
        }
    }

    /// Instants où un glissement s'aimante, sauf ceux des clips `except`.
    fn snap_time(&self, g: &Geo, t: f32, except: &[u64]) -> (f32, Option<f32>) {
        if !self.snap {
            return (t, None);
        }
        let mut pts: Vec<f32> = vec![0.0, self.playhead];
        pts.extend(self.project.markers.iter().copied());
        pts.extend(self.project.mark_in);
        pts.extend(self.project.mark_out);
        for c in &self.project.clips {
            if !except.contains(&c.id) {
                pts.extend([c.start, c.end()]);
            }
        }
        match pts.into_iter().filter(|s| (s - t).abs() * g.zoom < 8.0).min_by(|a, b| (a - t).abs().total_cmp(&(b - t).abs())) {
            Some(s) => (s, Some(s)),
            None => (t, None),
        }
    }

    fn click(&mut self, g: &Geo, at: Pos2, shift: bool, alt: bool) {
        let fps = self.project.fps.max(1.0);
        if at.y < g.v_top {
            let t = seconds(g.t_of(at.x), fps);
            self.seek(t);
            return;
        }
        let t = g.t_of(at.x);
        let hit = (0..self.project.clips.len()).rev().find(|&i| g.rect_of(&self.project.clips[i]).contains(at));
        match self.tool {
            Tool::Razor => {
                let t = self.snap_time(g, t, &[]).0;
                let idx: Vec<usize> = if shift {
                    (0..self.project.clips.len()).filter(|&i| !self.locked(&self.project.clips[i])).collect()
                } else {
                    hit.map(|i| self.with_links(&[i], alt)).unwrap_or_default()
                };
                let idx: Vec<usize> = idx.into_iter().filter(|&i| self.project.clips[i].start < t && self.project.clips[i].end() > t).collect();
                if self.split_at(t, &idx) {
                    self.changed();
                }
            }
            Tool::TrackFwd => {
                let row = g.row_at(at.y);
                self.sel = self.project.clips.iter().filter(|c| c.end() > t && !self.locked(c) && (!shift || row == Some((c.audio, c.track)))).map(|c| c.id).collect();
            }
            Tool::Zoom => {
                let f = if alt { 1.0 / 1.6 } else { 1.6 };
                self.zoom = (self.zoom * f).clamp(2.0, 800.0);
                self.scroll = (t - (at.x - g.area.left()) / self.zoom).max(0.0);
            }
            Tool::Type => {
                let track = g.row_at(at.y).filter(|r| !r.0).map(|r| r.1);
                self.add_title(self.snap_time(g, t, &[]).0, track);
            }
            Tool::Hand => {}
            _ => match hit {
                Some(i) if !self.locked(&self.project.clips[i]) => {
                    let id = self.project.clips[i].id;
                    let ids: Vec<u64> = self.with_links(&[i], alt).iter().map(|&k| self.project.clips[k].id).collect();
                    if shift {
                        if self.sel.contains(&id) {
                            self.sel.retain(|x| !ids.contains(x));
                        } else {
                            self.sel.extend(ids);
                        }
                    } else {
                        // Le clip cliqué d'abord : c'est lui que montrent les options d'effet
                        self.sel = std::iter::once(id).chain(ids.into_iter().filter(|x| *x != id)).collect();
                    }
                    self.top_tab = if self.top_tab == 2 { 2 } else { 0 };
                }
                Some(_) => {}
                None => {
                    if !shift {
                        self.sel.clear();
                    }
                }
            },
        }
    }

    fn begin_drag(&mut self, g: &Geo, at: Pos2, alt: bool, shift: bool) {
        if at.y < g.v_top {
            self.drag = Some(DragState { act: Act::Scrub, id: 0, orig: vec![], group: vec![], snapped: None });
            return;
        }
        let orig = self.project.clips.clone();
        let hit = (0..self.project.clips.len()).rev().find(|&i| g.rect_of(&self.project.clips[i]).contains(at)).filter(|&i| !self.locked(&self.project.clips[i]));
        if self.tool == Tool::Hand {
            self.drag = Some(DragState { act: Act::Pan { x0: at.x, s0: self.scroll }, id: 0, orig, group: vec![], snapped: None });
            return;
        }
        let Some(i) = hit else {
            if matches!(self.tool, Tool::Select | Tool::Ripple | Tool::Roll | Tool::Stretch | Tool::Slip) {
                if !shift {
                    self.sel.clear();
                }
                self.drag = Some(DragState { act: Act::Marquee { from: at }, id: 0, orig, group: vec![], snapped: None });
            }
            return;
        };
        let c = &self.project.clips[i];
        let id = c.id;
        let r = g.rect_of(c);
        let left = at.x < r.left() + 6.0;
        let right = at.x > r.right() - 6.0;
        let neighbor = |at_end: bool| -> Option<u64> {
            let edge = if at_end { c.end() } else { c.start };
            self.project.clips.iter().find(|o| o.id != id && o.audio == c.audio && o.track == c.track && if at_end { (o.start - edge).abs() < 0.02 } else { (o.end() - edge).abs() < 0.02 }).map(|o| o.id)
        };
        let act = match self.tool {
            Tool::Select | Tool::Ripple | Tool::Roll | Tool::Stretch if left || right => match self.tool {
                Tool::Ripple => if left { Act::RippleIn } else { Act::RippleOut },
                Tool::Roll => match neighbor(right) {
                    Some(other) => Act::Roll { other },
                    None => if left { Act::TrimIn } else { Act::TrimOut },
                },
                Tool::Stretch => Act::Stretch { left },
                _ => if left { Act::TrimIn } else { Act::TrimOut },
            },
            Tool::Slip => Act::Slip,
            Tool::Select | Tool::Ripple | Tool::Roll | Tool::Stretch | Tool::TrackFwd => Act::Move { grab: g.t_of(at.x) - c.start },
            _ => return,
        };
        // Le clip attrapé rejoint la sélection (avec ses clips liés)
        if !self.sel.contains(&id) {
            let ids: Vec<u64> = self.with_links(&[i], alt).iter().map(|&k| self.project.clips[k].id).collect();
            if shift {
                self.sel.extend(ids);
            } else {
                self.sel = std::iter::once(id).chain(ids.into_iter().filter(|x| *x != id)).collect();
            }
        }
        let group: Vec<u64> = match act {
            Act::Move { .. } => {
                let idx: Vec<usize> = self.selected();
                self.with_links(&idx, alt).iter().map(|&k| self.project.clips[k].id).collect()
            }
            _ => self.with_links(&[i], alt).iter().map(|&k| self.project.clips[k].id).collect(),
        };
        self.stop();
        self.drag = Some(DragState { act, id, orig, group, snapped: None });
    }

    fn update_drag(&mut self, g: &Geo, at: Pos2, from: Pos2, _alt: bool) {
        let Some(d) = self.drag.clone() else { return };
        let fps = self.project.fps.max(1.0);
        let frame = 1.0 / fps;
        let t = g.t_of(at.x);
        let orig_of = |id: u64| d.orig.iter().find(|c| c.id == id).cloned();
        let Some(o) = orig_of(d.id).or_else(|| d.orig.first().cloned()) else {
            match d.act {
                Act::Scrub => self.playhead = seconds(t, fps),
                Act::Pan { x0, s0 } => self.scroll = (s0 - (at.x - x0) / self.zoom).max(0.0),
                _ => {}
            }
            return;
        };
        let mut snapped = None;
        // Les autres clips du groupe suivent le même changement que le clip attrapé
        let partners: Vec<Clip> = d.group.iter().filter(|id| **id != d.id).filter_map(|id| orig_of(*id)).collect();
        match d.act {
            Act::Scrub => self.playhead = seconds(t, fps),
            Act::Pan { x0, s0 } => self.scroll = (s0 - (at.x - x0) / self.zoom).max(0.0),
            Act::Marquee { .. } => {}
            Act::Move { grab } => {
                let raw = (t - grab).max(0.0);
                let (a, sa) = self.snap_time(g, raw, &d.group);
                let (b, sb) = self.snap_time(g, raw + o.len(), &d.group);
                let start = if sa.is_some() && (sb.is_none() || (a - raw).abs() <= (b - o.len() - raw).abs()) {
                    snapped = sa;
                    a
                } else if sb.is_some() {
                    snapped = sb;
                    b - o.len()
                } else {
                    raw
                };
                let mut dt = start - o.start;
                let min_start = d.group.iter().filter_map(|id| orig_of(*id)).map(|c| c.start).fold(f32::MAX, f32::min);
                dt = dt.max(-min_start);
                // Changement de piste : seulement pour les clips du même genre que celui attrapé
                let dtr: i64 = match g.row_at(at.y) {
                    Some((audio, k)) if audio == o.audio => k as i64 - o.track as i64,
                    _ => 0,
                };
                let lanes = (self.project.lanes(false) as i64, self.project.lanes(true) as i64);
                let ok = d.group.iter().filter_map(|id| orig_of(*id)).filter(|c| c.audio == o.audio).all(|c| {
                    let n = c.track as i64 + dtr;
                    n >= 0 && n < if c.audio { lanes.1 } else { lanes.0 }
                });
                for id in &d.group {
                    if let (Some(oc), Some(i)) = (orig_of(*id), self.project.index(*id)) {
                        let c = &mut self.project.clips[i];
                        c.start = oc.start + dt;
                        if ok && oc.audio == o.audio {
                            c.track = (oc.track as i64 + dtr) as usize;
                        }
                    }
                }
            }
            Act::TrimIn | Act::RippleIn => {
                let ripple = d.act == Act::RippleIn;
                let (st, s) = self.snap_time(g, t, &d.group);
                snapped = s;
                let prev_end = if ripple { 0.0 } else { self.project.clips.iter().filter(|c| c.audio == o.audio && c.track == o.track && !d.group.contains(&c.id) && c.end() <= o.start + 1e-3).map(|c| c.end()).fold(0.0, f32::max) };
                let low = if o.kind == "image" || o.kind == "text" { prev_end } else { (o.start - o.src_in / o.speed).max(prev_end) };
                let ns = st.clamp(low, o.end() - frame);
                let delta = ns - o.start;
                for oc in std::iter::once(o.clone()).chain(partners.iter().cloned()) {
                    if let Some(i) = self.project.index(oc.id) {
                        let c = &mut self.project.clips[i];
                        if oc.kind == "image" || oc.kind == "text" {
                            c.src_in = 0.0;
                            c.src_out = (oc.len() - delta).max(frame);
                        } else {
                            c.src_in = (oc.src_in + delta * oc.speed).max(0.0);
                        }
                        c.start = if ripple { oc.start } else { oc.start + delta };
                    }
                }
                if ripple {
                    // Ce qui suit recule d'autant
                    let tracks: HashSet<(bool, usize)> = std::iter::once(&o).chain(partners.iter()).map(|c| (c.audio, c.track)).collect();
                    for oc in &d.orig {
                        if !d.group.contains(&oc.id) && tracks.contains(&(oc.audio, oc.track)) && oc.start >= o.end() - 1e-3 {
                            if let Some(i) = self.project.index(oc.id) {
                                self.project.clips[i].start = (oc.start - delta).max(0.0);
                            }
                        }
                    }
                }
            }
            Act::TrimOut | Act::RippleOut => {
                let ripple = d.act == Act::RippleOut;
                let (et, s) = self.snap_time(g, t, &d.group);
                snapped = s;
                let next_start = if ripple { f32::MAX } else { self.project.clips.iter().filter(|c| c.audio == o.audio && c.track == o.track && !d.group.contains(&c.id) && c.start >= o.end() - 1e-3).map(|c| c.start).fold(f32::MAX, f32::min) };
                let high = if o.media > 0.0 { (o.start + (o.media - o.src_in) / o.speed).min(next_start) } else { next_start };
                let ne = et.clamp(o.start + frame, high);
                let delta = ne - o.end();
                for oc in std::iter::once(o.clone()).chain(partners.iter().cloned()) {
                    if let Some(i) = self.project.index(oc.id) {
                        let c = &mut self.project.clips[i];
                        c.src_out = oc.src_out + delta * oc.speed;
                        if oc.media > 0.0 {
                            c.src_out = c.src_out.min(oc.media);
                        }
                    }
                }
                if ripple {
                    let tracks: HashSet<(bool, usize)> = std::iter::once(&o).chain(partners.iter()).map(|c| (c.audio, c.track)).collect();
                    for oc in &d.orig {
                        if !d.group.contains(&oc.id) && tracks.contains(&(oc.audio, oc.track)) && oc.start >= o.end() - 1e-3 {
                            if let Some(i) = self.project.index(oc.id) {
                                self.project.clips[i].start = (oc.start + delta).max(0.0);
                            }
                        }
                    }
                }
            }
            Act::Roll { other } => {
                let Some(n) = orig_of(other) else { return };
                let (left, right) = if n.start >= o.start { (o.clone(), n) } else { (n, o.clone()) };
                let (ct, s) = self.snap_time(g, t, &[left.id, right.id]);
                snapped = s;
                let mut lo = left.start + frame;
                let mut hi = right.end() - frame;
                if left.media > 0.0 {
                    hi = hi.min(left.start + (left.media - left.src_in) / left.speed);
                }
                if right.kind != "image" && right.kind != "text" {
                    lo = lo.max(right.start - right.src_in / right.speed);
                }
                let cut = ct.clamp(lo, hi.max(lo));
                let delta = cut - left.end();
                if let Some(i) = self.project.index(left.id) {
                    self.project.clips[i].src_out = left.src_out + delta * left.speed;
                }
                if let Some(i) = self.project.index(right.id) {
                    let c = &mut self.project.clips[i];
                    c.start = cut;
                    if right.kind == "image" || right.kind == "text" {
                        c.src_out = (right.len() - delta).max(frame);
                    } else {
                        c.src_in = right.src_in + delta * right.speed;
                    }
                }
            }
            Act::Stretch { left } => {
                let (st, s) = self.snap_time(g, t, &d.group);
                snapped = s;
                let src = o.src_out - o.src_in;
                let (ns, nl) = if left { let ns = st.clamp(0.0, o.end() - frame); (ns, o.end() - ns) } else { (o.start, (st - o.start).max(frame)) };
                let speed = (src / nl.max(frame)).clamp(0.1, 10.0);
                for oc in std::iter::once(o.clone()).chain(partners.iter().cloned()) {
                    if let Some(i) = self.project.index(oc.id) {
                        let c = &mut self.project.clips[i];
                        if oc.kind == "image" || oc.kind == "text" {
                            c.src_out = nl;
                        } else {
                            c.speed = speed;
                        }
                        c.start = ns;
                    }
                }
            }
            Act::Slip => {
                let shift = -(at.x - from.x) / self.zoom * o.speed;
                for oc in std::iter::once(o.clone()).chain(partners.iter().cloned()) {
                    if oc.kind == "image" || oc.kind == "text" {
                        continue;
                    }
                    let len = oc.src_out - oc.src_in;
                    let max_in = if oc.media > 0.0 { (oc.media - len).max(0.0) } else { f32::MAX };
                    let ni = (oc.src_in + shift).clamp(0.0, max_in);
                    if let Some(i) = self.project.index(oc.id) {
                        let c = &mut self.project.clips[i];
                        c.src_in = ni;
                        c.src_out = ni + len;
                    }
                }
                // Le moniteur montre la nouvelle première image
                self.playhead = o.start;
                self.rev += 1;
            }
        }
        if let Some(dd) = &mut self.drag {
            dd.snapped = snapped;
        }
    }

    fn end_drag(&mut self, g: &Geo, at: Pos2, from: Pos2, shift: bool, ctrl: bool) {
        let Some(d) = self.drag.take() else { return };
        match d.act {
            Act::Scrub | Act::Pan { .. } => {}
            Act::Marquee { from: f0 } => {
                let r = Rect::from_two_pos(f0, at);
                let hits: Vec<u64> = self.project.clips.iter().filter(|c| g.rect_of(c).intersects(r) && !self.locked(c)).map(|c| c.id).collect();
                if !shift {
                    self.sel.clear();
                }
                for id in hits {
                    if !self.sel.contains(&id) {
                        self.sel.push(id);
                    }
                }
                let _ = from;
            }
            Act::Move { .. } => {
                let keep: HashSet<u64> = d.group.iter().copied().collect();
                let moved: Vec<Clip> = d.group.iter().filter_map(|id| self.project.index(*id).map(|i| self.project.clips[i].clone())).collect();
                let unchanged = moved.iter().all(|c| d.orig.iter().any(|o| o.id == c.id && o.start == c.start && o.track == c.track));
                if !unchanged {
                    if ctrl {
                        // Ctrl en lâchant : insérer (ce qui suit recule)
                        let at = moved.iter().map(|c| c.start).fold(f32::MAX, f32::min);
                        let len = moved.iter().map(|c| c.end()).fold(0.0, f32::max) - at;
                        let crossing: Vec<usize> = (0..self.project.clips.len()).filter(|&i| {
                            let c = &self.project.clips[i];
                            !keep.contains(&c.id) && c.start < at - 1e-3 && c.end() > at + 1e-3 && !self.locked(c)
                        }).collect();
                        self.split_at(at, &crossing);
                        self.shift_after(at, len, None, &keep);
                    } else {
                        // Recouvrir : ce qui était dessous laisse la place
                        for c in &moved {
                            self.clear_range(c.audio, c.track, c.start, c.end(), &keep);
                        }
                    }
                    self.changed();
                }
            }
            _ => self.changed(),
        }
    }

    /// Médias, source et effets lâchés sur la timeline.
    fn drops(&mut self, ui: &egui::Ui, g: &Geo, resp: &egui::Response, painter: &egui::Painter, ctrl: bool) {
        let p = crate::theme::pal();
        let hover = ui.input(|i| i.pointer.hover_pos());
        let Some(at) = hover.filter(|h| g.area.contains(*h)) else { return };
        let t = g.t_of(at.x);
        let row = g.row_at(at.y);
        let vt = row.filter(|r| !r.0).map(|r| r.1).unwrap_or(self.vtarget);
        let at_track = row.filter(|r| r.0).map(|r| r.1).unwrap_or(self.atarget);
        // Ce que le glissement poserait
        let ghost = |this: &mut Editor, clips: &[Clip]| {
            let (t0, _) = this.snap_time(g, t, &[]);
            for c in clips {
                let mut c = c.clone();
                c.start = c.start + t0;
                let r = g.rect_of(&c);
                painter.rect_filled(r, 3.0, clip_color(&c).gamma_multiply(0.4));
                painter.rect_stroke(r, 3.0, Stroke::new(1.5, p.text), egui::StrokeKind::Inside);
            }
        };
        let place = |this: &mut Editor, clips: Vec<Clip>| {
            let (t0, _) = this.snap_time(g, t, &[]);
            this.sel = this.place(clips, t0, ctrl);
            this.changed();
        };
        let fix_tracks = |mut clips: Vec<Clip>| -> Vec<Clip> {
            for c in clips.iter_mut() {
                c.track = if c.audio { at_track } else { vt };
            }
            clips
        };
        // Depuis le panneau Projet
        if let Some(pl) = resp.dnd_hover_payload::<Payload>() {
            match &*pl {
                Payload::Media(f) => {
                    let path = self.media_path(f);
                    if let Some(clips) = self.media_clips_cached(&path, vt, at_track) {
                        ghost(self, &fix_tracks(clips));
                    }
                }
                Payload::Source => {
                    if let Some(clips) = self.source_clips() {
                        ghost(self, &fix_tracks(clips));
                    }
                }
                Payload::Effect(_) => {
                    if let Some(i) = (0..self.project.clips.len()).rev().find(|&i| g.rect_of(&self.project.clips[i]).contains(at)) {
                        painter.rect_stroke(g.rect_of(&self.project.clips[i]), 3.0, Stroke::new(2.0, p.accent), egui::StrokeKind::Inside);
                    }
                }
            }
        }
        if let Some(pl) = crate::drop_of::<Payload>(&resp) {
            match &*pl {
                Payload::Media(f) => {
                    let path = self.media_path(f);
                    if let Some(clips) = self.media_clips(&path, vt, at_track) {
                        place(self, fix_tracks(clips));
                    }
                }
                Payload::Source => {
                    if let Some(clips) = self.source_clips() {
                        place(self, fix_tracks(clips));
                    }
                }
                Payload::Effect(fx) => {
                    if let Some(i) = (0..self.project.clips.len()).rev().find(|&i| g.rect_of(&self.project.clips[i]).contains(at)) {
                        self.apply_effect(i, fx, t);
                    }
                }
            }
        }
        // Depuis l'explorateur du coffre (chemin relatif au coffre)
        if let Some(rel) = resp.dnd_hover_payload::<String>() {
            painter.rect_stroke(g.area.shrink(1.0), 0.0, Stroke::new(2.0, p.accent), egui::StrokeKind::Inside);
            let abs = Path::new(&self.root).join(rel.as_str());
            match self.media_clips_cached(&abs, vt, at_track) {
                Some(clips) => ghost(self, &fix_tracks(clips)),
                None => {
                    painter.line_segment([pos2(at.x, g.v_top), pos2(at.x, g.area.bottom())], Stroke::new(1.0, p.accent));
                }
            }
        }
        if let Some(rel) = crate::drop_of::<String>(&resp) {
            let abs = Path::new(&self.root).join(rel.as_str());
            if let Some(clips) = self.media_clips(&abs, vt, at_track) {
                place(self, fix_tracks(clips));
            }
        }
    }

    /// Comme `media_clips`, mais sans toucher au projet (pour l'aperçu du glissement), avec cache.
    fn media_clips_cached(&mut self, path: &Path, vtrack: usize, atrack: usize) -> Option<Vec<Clip>> {
        let key = path.to_string_lossy().to_string();
        let c = match CLIPS.with(|m| m.borrow().get(&key).cloned()) {
            Some(c) => c?,
            None => {
                let c = import(&self.path, path, vtrack, 0.0).ok();
                CLIPS.with(|m| m.borrow_mut().insert(key, c.clone()));
                c?
            }
        };
        let mut p = Project::default();
        p.add_media(c, atrack);
        Some(p.clips)
    }
}

thread_local! {
    static INFO: std::cell::RefCell<std::collections::HashMap<String, (String, f32, bool)>> = Default::default();
    static CLIPS: std::cell::RefCell<std::collections::HashMap<String, Option<Clip>>> = Default::default();
}

/// Durée courte pour les cartes : « 0:42 », « 1:10 », « 8 s »
fn short_time(t: f32) -> String {
    if t < 10.0 {
        format!("{:.0} s", t.max(0.0))
    } else {
        format!("{}:{:02}", (t / 60.0) as u32, t as u32 % 60)
    }
}


impl Editor {
    /// Lance l'analyse du son : du clip choisi (silences) ou de la musique (temps forts)
    /// « Version 9:16 » : une copie verticale du montage, enregistrée à côté et ouverte
    fn make_vertical(&mut self) {
        let mut sizes: std::collections::HashMap<String, Option<(f32, f32)>> = Default::default();
        for c in &self.project.clips {
            if c.kind != "audio" && c.kind != "text" && !sizes.contains_key(&c.file) {
                sizes.insert(c.file.clone(), crate::montage::media_dims(&self.path, c));
            }
        }
        let v = crate::montage::vertical_version(&self.project, |c| sizes.get(&c.file).copied().flatten());
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "montage".into());
        let ext = self.path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_else(|| "aryav".into());
        let dir = self.path.parent().map(|d| d.to_path_buf()).unwrap_or_default();
        let mut out = dir.join(format!("{stem} 9x16.{ext}"));
        let mut n = 2;
        while out.exists() {
            out = dir.join(format!("{stem} 9x16 ({n}).{ext}"));
            n += 1;
        }
        match v.write(&out) {
            Ok(()) => self.open_tab = Some(out),
            Err(e) => self.message = Some(e),
        }
    }

    fn start_smart(&mut self, beats: bool, ctx: &egui::Context) {
        let pick = self.sel.iter().filter_map(|id| self.project.index(*id)).find(|&i| self.project.clips[i].sounds());
        let i = match (pick, beats) {
            (Some(i), _) => Some(i),
            // Sans choix, pour le rythme : la plus longue musique du montage
            (None, true) => (0..self.project.clips.len()).filter(|&i| self.project.clips[i].kind == "audio").max_by(|&a, &b| self.project.clips[a].len().total_cmp(&self.project.clips[b].len())),
            (None, false) => None,
        };
        let Some(i) = i else {
            self.message = Some(crate::i18n::t(if beats { "Choisis la musique (un clip son) dans la timeline." } else { "Choisis le clip qui parle dans la timeline." }));
            return;
        };
        let c = self.project.clips[i].clone();
        let path = resolve(&self.path, &c.file);
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = if beats {
                crate::motion::analyse(&path).map(|w| {
                    let sp = c.speed.max(1e-3);
                    Smart::Beats(w.beats.into_iter().filter(|b| *b >= c.src_in && *b <= c.src_out).map(|b| c.start + (b - c.src_in) / sp).collect())
                })
            } else {
                crate::audio::decode(&path, "").map(|b| Smart::Silences(c.id, b))
            };
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.smart_rx = Some(rx);
        self.message = Some(crate::i18n::t("Analyse du son…"));
    }

    fn poll_smart(&mut self) {
        let Some(rx) = &self.smart_rx else { return };
        let Ok(r) = rx.try_recv() else { return };
        self.smart_rx = None;
        match r {
            Err(e) => self.message = Some(e),
            Ok(Smart::Silences(id, buf)) => {
                let Some(i) = self.project.index(id) else { return };
                let saved = self.project.cut_silences(i, &buf);
                self.message = Some(if saved > 0.05 { format!("{} {saved:.1} s", crate::i18n::t("Silences retirés :")) } else { crate::i18n::t("Aucun silence assez long dans ce clip.") });
                self.changed();
            }
            Ok(Smart::Beats(beats)) => {
                if beats.len() < 2 {
                    self.message = Some(crate::i18n::t("Pas assez de temps forts dans cette musique."));
                    return;
                }
                let n = self.project.fit_to_beats(0, &beats, 0.6);
                self.message = Some(format!("{} {n} · {} {}", crate::i18n::t("Plans calés sur le rythme :"), beats.len(), crate::i18n::t_mid("temps forts")));
                self.changed();
            }
        }
    }
}
