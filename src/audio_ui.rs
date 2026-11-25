//! L'éditeur audio multipiste : les sons du coffre à gauche, les pistes et leurs clips au centre,
//! les effets à droite.

use crate::audio::{self, Clip, Fx, Session, Slot, RATE};
use crate::lucide as l;
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, Sense, Stroke};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Razor,
}

enum Drag {
    /// Déplace un clip : piste et clip de départ, décalage de la souris dans le clip
    Move { track: usize, clip: usize, grab: f64 },
    Trim { track: usize, clip: usize, left: bool, start: f64, offset: f64, len: f64 },
    Fade { track: usize, clip: usize, left: bool },
    Range { a: f64, track: usize },
    Scrub,
}

pub struct Editor {
    pub path: PathBuf,
    pub session: Session,
    /// Sons du coffre, donnés par la fenêtre à chaque image
    pub files: Vec<String>,
    root: String,
    sounds: audio::Sounds,
    error: Option<String>,
    message: Option<(String, Instant)>,
    /// Cible de « Normaliser la sonie » (LUFS)
    loud_target: f32,
    time: f64,
    /// Lecture : départ, instant de départ, et le mixage joué (pour les vumètres)
    playing: Option<(Instant, f64, Arc<Vec<f32>>)>,
    player: Option<std::process::Child>,
    looping: bool,
    tool: Tool,
    sel_clip: Option<(usize, usize)>,
    sel_track: usize,
    /// Plage choisie : début, fin, pistes (de, à)
    range: Option<(f64, f64, usize, usize)>,
    zoom: f64,
    scroll: f64,
    fit: bool,
    drag: Option<Drag>,
    saved: Session,
    history: Vec<Session>,
    redo: Vec<Session>,
    insp_tab: u8,
    export_rx: Option<std::sync::mpsc::Receiver<Result<PathBuf, String>>>,
    /// Dernier spectre calculé (ce qu'il décrit, ses bandes)
    spec: Option<(String, Vec<f32>)>,
    /// Courbes de ducking affichées (signature de ce dont elles dépendent, gains à 50 par seconde)
    duck_view: std::collections::HashMap<usize, (String, Arc<Vec<f32>>)>,
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.stop();
    }
}

const HEAD_W: f32 = 180.0;
const TRACK_H: f32 = 96.0;
const CLIP_BAR: f32 = 16.0;

fn col(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// 1:23,456
fn clock(t: f64) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    format!("{}:{:02},{:03}", ms / 60000, (ms / 1000) % 60, ms % 1000)
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(egui::RichText::new(title).size(11.5).color(crate::theme::pal().muted));
    ui.add_space(2.0);
}

fn flat(ui: &mut egui::Ui) {
    let p = crate::theme::pal();
    let v = &mut ui.style_mut().visuals.widgets;
    v.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.inactive.bg_stroke = Stroke::NONE;
    v.hovered.weak_bg_fill = p.raised;
    v.hovered.bg_stroke = Stroke::NONE;
}

impl Editor {
    pub fn open(path: PathBuf, root: &str) -> Editor {
        let (session, error) = match Session::read(&path) {
            Ok(s) => (s, None),
            Err(e) => (Session::starter(), Some(e)),
        };
        Editor {
            path,
            saved: session.clone(),
            session,
            files: vec![],
            root: root.to_string(),
            sounds: audio::Sounds::new(root),
            error,
            message: None,
            loud_target: -14.0,
            time: 0.0,
            playing: None,
            player: None,
            looping: false,
            tool: Tool::Select,
            sel_clip: None,
            sel_track: 0,
            range: None,
            zoom: 60.0,
            scroll: 0.0,
            fit: true,
            drag: None,
            history: vec![],
            redo: vec![],
            insp_tab: 0,
            export_rx: None,
            spec: None,
            duck_view: Default::default(),
        }
    }

    /// Une session neuve autour d'un son du coffre (ouvert seul) : à côté de lui, ou celle qui le contient déjà
    pub fn session_for(root: &str, rel: &str) -> Result<String, String> {
        let stem = std::path::Path::new(rel).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "son".into());
        let dir = crate::vault::parent(rel);
        let out = crate::vault::join(&dir, &format!("{stem}.{}", crate::formats::ext("audio")));
        let path = crate::vault::safe(root, &out)?;
        if !path.exists() {
            let mut s = Session::starter();
            s.tracks[0].name = stem.clone();
            // Longueur 0 : tout le son, connue une fois décodé
            s.tracks[0].clips.push(Clip { src: rel.to_string(), start: 0.0, offset: 0.0, len: 0.0, gain: 0.0, fade_in: 0.0, fade_out: 0.0, fx: vec![] });
            s.write(&path)?;
        }
        Ok(out)
    }

    fn say(&mut self, m: impl Into<String>) {
        self.message = Some((m.into(), Instant::now()));
    }

    fn changed(&mut self) {
        if self.saved == self.session {
            return;
        }
        for t in &mut self.session.tracks {
            t.clips.sort_by(|a, b| a.start.total_cmp(&b.start));
        }
        self.history.push(std::mem::replace(&mut self.saved, self.session.clone()));
        if self.history.len() > 200 {
            self.history.remove(0);
        }
        self.redo.clear();
        if self.error.is_none() {
            if let Err(e) = self.session.write(&self.path) {
                self.say(e);
            }
        }
    }

    fn undo(&mut self, forward: bool) {
        let (from, to) = if forward { (&mut self.redo, &mut self.history) } else { (&mut self.history, &mut self.redo) };
        if let Some(s) = from.pop() {
            to.push(std::mem::replace(&mut self.session, s));
            self.saved = self.session.clone();
            self.sel_clip = None;
            let _ = self.session.write(&self.path);
        }
    }

    fn stop(&mut self) {
        if let Some(mut c) = self.player.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.playing = None;
    }

    /// Lecture à partir de la tête (ou de la plage en boucle) : le mixage est calculé ici et joué par ffplay
    fn play(&mut self) {
        self.stop();
        let (a, b) = match (self.looping, self.range) {
            (true, Some((a, b, ..))) => (a, b),
            _ => {
                let end = self.session.duration();
                if self.time >= end - 0.01 {
                    self.time = 0.0;
                }
                (self.time, end.min(self.time + 600.0))
            }
        };
        if b <= a {
            return;
        }
        self.time = a;
        let data = Arc::new(audio::mix(&self.session, &self.sounds, a, b, false));
        self.playing = Some((Instant::now(), a, data.clone()));
        let Some(fp) = crate::ffmpeg::find("ffplay") else {
            self.say(crate::i18n::t("ffplay manque pour écouter (il vient avec ffmpeg)."));
            return;
        };
        let child = crate::ffmpeg::command(fp)
            .args(["-nodisp", "-autoexit", "-loglevel", "quiet", "-fflags", "nobuffer", "-i", "-"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if let Ok(mut c) = child {
            if let Some(mut stdin) = c.stdin.take() {
                std::thread::spawn(move || {
                    use std::io::Write;
                    let _ = stdin.write_all(&audio::wav_bytes(&data));
                });
            }
            self.player = Some(c);
        }
    }

    fn toggle_play(&mut self) {
        if self.playing.is_some() {
            self.stop();
        } else {
            self.play();
        }
    }

    /// Les pistes visées : celles de la plage, sinon la piste du clip choisi, sinon la piste active
    fn target_tracks(&self) -> Vec<usize> {
        match self.range {
            Some((_, _, a, b)) => (a.min(b)..=a.max(b)).collect(),
            None => vec![self.sel_clip.map(|c| c.0).unwrap_or(self.sel_track)],
        }
    }

    /// Les clips sur lesquels agit un effet : ceux de la plage (coupés à ses bords), ou le clip choisi
    fn fx_targets(&mut self) -> Vec<(usize, usize)> {
        if let Some((a, b, ..)) = self.range {
            let tracks = self.target_tracks();
            self.session.split(&tracks, a);
            self.session.split(&tracks, b);
            let mut out = vec![];
            for ti in tracks {
                for (ci, c) in self.session.tracks[ti].clips.iter().enumerate() {
                    if c.start >= a - 1e-6 && c.end() <= b + 1e-6 {
                        out.push((ti, ci));
                    }
                }
            }
            return out;
        }
        self.sel_clip.into_iter().collect()
    }

    fn add_fx(&mut self, fx: Fx) {
        let targets = self.fx_targets();
        if targets.is_empty() {
            self.say(crate::i18n::t("Choisis un clip ou une plage."));
            return;
        }
        for (ti, ci) in &targets {
            let c = &mut self.session.tracks[*ti].clips[*ci];
            // Un effet du même genre est remplacé
            c.fx.retain(|f| std::mem::discriminant(f) != std::mem::discriminant(&fx));
            c.fx.push(fx.clone());
        }
        self.say(format!("{} · {} clip(s)", fx.label(), targets.len()));
        self.changed();
    }

    fn add_file(&mut self, rel: &str) {
        let ti = self.sel_track.min(self.session.tracks.len().saturating_sub(1));
        if self.session.tracks.is_empty() {
            self.session.tracks.push(Session::new_track(0));
        }
        // À la tête de lecture, ou à la suite si un clip y est déjà
        let tr = &self.session.tracks[ti];
        let busy = tr.clips.iter().any(|c| self.time >= c.start && self.time < c.end());
        let start = if busy { tr.clips.iter().map(|c| c.end()).fold(0.0, f64::max) } else { self.time };
        self.session.tracks[ti].clips.push(Clip { src: rel.to_string(), start, offset: 0.0, len: 0.0, gain: 0.0, fade_in: 0.0, fade_out: 0.0, fx: vec![] });
        let ci = self.session.tracks[ti].clips.len() - 1;
        self.sel_clip = Some((ti, ci));
        self.changed();
        self.sel_clip = self.session.tracks[ti].clips.iter().position(|c| c.src == rel && (c.start - start).abs() < 1e-9).map(|c| (ti, c));
    }

    fn import(&mut self) {
        let Some(files) = rfd::FileDialog::new().add_filter("son", &audio::EXT).pick_files() else { return };
        let here = crate::vault::inside(&self.root, &self.path).map(|r| crate::vault::parent(&r)).unwrap_or_default();
        for f in files {
            let rel = match crate::vault::inside(&self.root, &f) {
                Some(r) => Ok(r),
                None => crate::vault::import(&self.root, &here, &f, false),
            };
            match rel {
                Ok(r) => self.add_file(&r),
                Err(e) => self.say(e),
            }
        }
    }

    /// Clips de longueur encore inconnue (son entier) : elle arrive avec le décodage
    fn settle_lengths(&mut self, ctx: &egui::Context) {
        let mut any = false;
        for t in &mut self.session.tracks {
            for c in &mut t.clips {
                if c.len <= 0.0 {
                    if let Slot::Ready(b) = self.sounds.get(&c.src, &c.fx_key(), Some(ctx)) {
                        c.len = (b.secs() - c.offset).max(0.01);
                        any = true;
                    }
                }
            }
        }
        if any {
            self.saved = self.session.clone();
            let _ = self.session.write(&self.path);
            self.fit = true;
        }
    }

    fn keyboard(&mut self, ui: &egui::Ui) {
        if ui.ctx().memory(|m| m.focused().is_some()) {
            return;
        }
        let i = ui.input(|i| i.clone());
        let cmd = i.modifiers.command;
        if i.key_pressed(egui::Key::Space) {
            self.toggle_play();
        }
        if i.key_pressed(egui::Key::Home) {
            self.stop();
            self.time = 0.0;
            self.scroll = 0.0;
        }
        if cmd && i.key_pressed(egui::Key::Z) {
            self.undo(i.modifiers.shift);
        }
        if cmd && i.key_pressed(egui::Key::Y) {
            self.undo(true);
        }
        if !cmd && i.key_pressed(egui::Key::L) {
            self.looping ^= true;
        }
        if !cmd && i.key_pressed(egui::Key::V) {
            self.tool = Tool::Select;
        }
        if !cmd && i.key_pressed(egui::Key::B) {
            self.tool = Tool::Razor;
        }
        if !cmd && i.key_pressed(egui::Key::M) {
            let n = self.session.markers.len() + 1;
            self.session.markers.push(audio::Marker { t: self.time, name: format!("{} {n}", crate::i18n::t("Repère")) });
            self.session.markers.sort_by(|a, b| a.t.total_cmp(&b.t));
            self.changed();
        }
        // S : coupe à la tête de lecture
        if !cmd && i.key_pressed(egui::Key::S) {
            let tracks = if self.range.is_some() || self.sel_clip.is_some() { self.target_tracks() } else { (0..self.session.tracks.len()).collect() };
            if self.session.split(&tracks, self.time) > 0 {
                self.sel_clip = None;
                self.changed();
            }
        }
        if cmd && i.key_pressed(egui::Key::D) {
            if let Some((ti, ci)) = self.sel_clip {
                let mut c = self.session.tracks[ti].clips[ci].clone();
                c.start = self.session.tracks[ti].clips.iter().map(|c| c.end()).fold(0.0, f64::max);
                self.session.tracks[ti].clips.push(c);
                self.changed();
            }
        }
        if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) {
            self.delete_selection();
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let ctx = ui.ctx().clone();
        self.settle_lengths(&ctx);
        if let Some((t0, from, data)) = &self.playing {
            let t = from + t0.elapsed().as_secs_f64();
            let end = from + data.len() as f64 / 2.0 / RATE as f64;
            if t >= end {
                if self.looping && self.range.is_some() {
                    self.play();
                } else {
                    self.stop();
                    self.time = end;
                }
            } else {
                self.time = t;
            }
            ctx.request_repaint();
        }
        if let Some(rx) = &self.export_rx {
            if let Ok(r) = rx.try_recv() {
                self.export_rx = None;
                match r {
                    Ok(f) => self.say(format!("{} : {}", crate::i18n::t("exporté"), f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())),
                    Err(e) => self.say(e),
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }
        }
        self.keyboard(ui);
        let frame = egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line));
        egui::Panel::left("audio-sounds").resizable(true).default_size(230.0).min_size(180.0).max_size(420.0).frame(frame).show(ui, |ui| self.sounds_panel(ui));
        egui::Panel::right("audio-inspector").resizable(true).default_size(290.0).min_size(240.0).max_size(460.0).frame(frame).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| self.inspector(ui));
        });
        egui::Panel::bottom("audio-status").exact_size(34.0).frame(frame).show(ui, |ui| self.status(ui));
        egui::Panel::top("audio-transport").exact_size(44.0).frame(frame).show(ui, |ui| self.transport(ui));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.code_bg)).show(ui, |ui| self.tracks(ui));
    }

    // ---------- Sons ----------

    fn sounds_panel(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        ui.set_min_height(ui.available_height());
        let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
        ui.painter().hline(hr.x_range(), hr.bottom() - 0.5, Stroke::new(1.0, p.line));
        ui.painter().text(pos2(hr.left() + 12.0, hr.center().y), egui::Align2::LEFT_CENTER, crate::i18n::t("Sons"), egui::FontId::new(13.0, crate::theme::bold()), p.text);
        let br = Rect::from_center_size(pos2(hr.right() - 18.0, hr.center().y), vec2(24.0, 24.0));
        let b = ui.interact(br, ui.scope_id().with("audio-import"), Sense::click()).on_hover_text(crate::i18n::t("Importer des sons…"));
        if b.hovered() {
            ui.painter().rect_filled(br, 4.0, p.raised);
        }
        ui.painter().text(br.center(), egui::Align2::CENTER_CENTER, l::PLUS, egui::FontId::proportional(14.0), p.text);
        if b.clicked() {
            self.import();
        }
        let used: std::collections::HashSet<String> = self.session.tracks.iter().flat_map(|t| t.clips.iter().map(|c| c.src.clone())).collect();
        egui::ScrollArea::vertical().max_height(ui.available_height() * 0.6).auto_shrink([false, true]).show(ui, |ui| {
            ui.add_space(4.0);
            if self.files.is_empty() {
                ui.label(egui::RichText::new(crate::i18n::t("Pas encore de son dans le coffre : + pour en importer.")).size(11.5).color(p.muted));
            }
            for rel in self.files.clone() {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
                let on = used.contains(&rel);
                if resp.hovered() {
                    ui.painter().rect_filled(r, 2.0, p.raised);
                }
                ui.painter().text(pos2(r.left() + 12.0, r.center().y), egui::Align2::LEFT_CENTER, l::MUSIC, egui::FontId::proportional(13.0), if on { p.accent } else { p.muted });
                let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
                // Deux sons du même nom : on montre aussi leur dossier
                let twin = self.files.iter().filter(|f| f.rsplit('/').next() == Some(name.as_str())).count() > 1;
                let name = if twin { format!("{name}  ·  {}", crate::vault::parent(&rel)) } else { name };
                ui.painter().with_clip_rect(r.shrink2(vec2(30.0, 0.0))).text(pos2(r.left() + 32.0, r.center().y), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(12.5), p.text);
                if let Slot::Ready(b) = self.sounds.get(&rel, "", None) {
                    let s = b.secs();
                    ui.painter().text(pos2(r.right() - 8.0, r.center().y), egui::Align2::RIGHT_CENTER, format!("{}:{:02}", (s / 60.0) as u32, (s % 60.0) as u32), egui::FontId::proportional(11.0), p.muted);
                }
                let resp = resp.on_hover_text(crate::i18n::t("Clic : l'ajouter sur la piste active, à la tête de lecture"));
                if resp.clicked() {
                    self.add_file(&rel);
                }
            }
        });
        ui.add_space(6.0);
        ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, p.line));
        egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 4)).show(ui, |ui| {
            section(ui, &crate::i18n::t("Repères"));
            if self.session.markers.is_empty() {
                ui.label(egui::RichText::new(crate::i18n::t("M pose un repère à la tête de lecture.")).size(11.5).color(p.muted));
            }
        });
        let mut goto = None;
        let mut remove = None;
        for (k, m) in self.session.markers.iter().enumerate() {
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, 2.0, p.raised);
            }
            ui.painter().text(pos2(r.left() + 12.0, r.center().y), egui::Align2::LEFT_CENTER, l::FLAG, egui::FontId::proportional(12.0), p.warning);
            ui.painter().text(pos2(r.left() + 32.0, r.center().y), egui::Align2::LEFT_CENTER, &m.name, egui::FontId::proportional(12.5), p.text);
            ui.painter().text(pos2(r.right() - 8.0, r.center().y), egui::Align2::RIGHT_CENTER, clock(m.t), egui::FontId::monospace(11.0), p.muted);
            if resp.clicked() {
                goto = Some(m.t);
            }
            resp.context_menu(|ui| {
                if ui.button(format!("{}  {}", crate::lucide::TRASH_2, crate::i18n::t("retirer le repère"))).clicked() {
                    remove = Some(k);
                }
            });
        }
        if let Some(t) = goto {
            self.stop();
            self.time = t;
        }
        if let Some(k) = remove {
            self.session.markers.remove(k);
            self.changed();
        }
    }

    // ---------- Transport ----------

    fn transport(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let r = ui.max_rect().shrink2(vec2(10.0, 0.0));
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::left_to_right(egui::Align::Center)));
        flat(&mut ui);
        let ib = |ui: &mut egui::Ui, icon: &str, tip: &str, on: bool, color: Option<Color32>| {
            let b = egui::Button::new(egui::RichText::new(icon).size(15.0).color(color.unwrap_or(if on { p.text } else { p.text }))).min_size(vec2(30.0, 30.0)).selected(on);
            ui.add(b).on_hover_text(crate::i18n::t(tip)).clicked()
        };
        if ib(&mut ui, l::SKIP_BACK, "Revenir au début (Début)", false, None) {
            self.stop();
            self.time = 0.0;
            self.scroll = 0.0;
        }
        let playing = self.playing.is_some();
        if ib(&mut ui, if playing { l::PAUSE } else { l::PLAY }, "Lire (Espace)", false, None) {
            self.toggle_play();
        }
        if ib(&mut ui, l::SQUARE, "Arrêter", false, None) {
            self.stop();
        }
        if ib(&mut ui, l::REPEAT, "Boucle sur la plage (L)", self.looping, None) {
            self.looping ^= true;
        }
        ui.add_space(6.0);
        ui.separator();
        if ib(&mut ui, l::MOUSE_POINTER_2, "Sélection (V)", self.tool == Tool::Select, None) {
            self.tool = Tool::Select;
        }
        if ib(&mut ui, l::SCISSORS, "Rasoir : couper un clip (B) ; S coupe à la tête de lecture", self.tool == Tool::Razor, None) {
            self.tool = Tool::Razor;
        }
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);
        let (cr, _) = ui.allocate_exact_size(vec2(118.0, 30.0), Sense::hover());
        ui.painter().rect(cr, 2.0, p.code_bg, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
        ui.painter().text(cr.center(), egui::Align2::CENTER_CENTER, clock(self.time), egui::FontId::monospace(17.0), p.text);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let busy = self.export_rx.is_some();
            ui.menu_button(format!("{}  {}", l::DOWNLOAD, crate::i18n::t(if busy { "Export…" } else { "Exporter" })), |ui| {
                for (ext, label) in [("wav", "WAV 24 bits"), ("mp3", "MP3 256 kb/s"), ("flac", "FLAC")] {
                    if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                        let out = self.export_path(ext);
                        let s = self.session.clone();
                        let sounds = self.sounds.clone();
                        let (tx, rx) = std::sync::mpsc::channel();
                        self.export_rx = Some(rx);
                        std::thread::spawn(move || {
                            let _ = tx.send(audio::export(&s, &sounds, &out).map(|_| out));
                        });
                        ui.close();
                    }
                }
                ui.separator();
                let b = egui::Button::new(format!("{}  {}", l::LAYERS, crate::i18n::t("Pistes séparées (stems)")));
                if ui.add_enabled(!busy, b).on_hover_text(crate::i18n::t("Un WAV par piste, tous de la durée de la session : ils se recalent tels quels dans un autre logiciel")).clicked() {
                    let out = self.export_path("wav");
                    let dir = out.with_file_name(out.file_stem().map(|s| s.to_string_lossy().replace(" - mix", " - stems")).unwrap_or_else(|| "stems".into()));
                    let s = self.session.clone();
                    let sounds = self.sounds.clone();
                    let (tx, rx) = std::sync::mpsc::channel();
                    self.export_rx = Some(rx);
                    std::thread::spawn(move || {
                        let _ = tx.send(audio::export_stems(&s, &sounds, &dir).map(|_| dir));
                    });
                    ui.close();
                }
            });
            ui.add_space(8.0);
            if ui.available_width() < 300.0 {
                return;
            }
            // Vumètres : la crête des 50 dernières millisecondes jouées
            let (l_pk, r_pk) = self.levels();
            let db = audio::to_db(l_pk.max(r_pk));
            ui.label(egui::RichText::new(if l_pk.max(r_pk) > 1e-4 { format!("{db:.0} dB") } else { "−∞ dB".into() }).monospace().size(11.5).color(p.muted));
            let (mr, _) = ui.allocate_exact_size(vec2(200.0, 16.0), Sense::hover());
            for (k, v) in [l_pk, r_pk].into_iter().enumerate() {
                let row = Rect::from_min_size(pos2(mr.left(), mr.top() + 1.0 + k as f32 * 8.0), vec2(mr.width(), 6.0));
                ui.painter().rect_filled(row, 1.0, p.raised);
                // De -60 à 0 dB
                let f = ((audio::to_db(v) + 60.0) / 60.0).clamp(0.0, 1.0);
                let w = row.width() * f;
                let green = row.width() * 0.75;
                ui.painter().rect_filled(Rect::from_min_size(row.min, vec2(w.min(green), 6.0)), 1.0, p.success);
                if w > green {
                    ui.painter().rect_filled(Rect::from_min_max(pos2(row.left() + green, row.top()), pos2(row.left() + w.min(row.width() * 0.92), row.bottom())), 0.0, p.warning);
                }
                if w > row.width() * 0.92 {
                    ui.painter().rect_filled(Rect::from_min_max(pos2(row.left() + row.width() * 0.92, row.top()), pos2(row.left() + w, row.bottom())), 0.0, p.missing);
                }
            }
        });
    }

    /// « Voix - mix.wav » à côté de la session ; jamais le nom d'un son qu'elle utilise
    fn export_path(&self, ext: &str) -> PathBuf {
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "mix".into());
        let dir = crate::untitled::export_dir(&self.path);
        let used: Vec<PathBuf> = self.session.tracks.iter().flat_map(|t| t.clips.iter()).filter_map(|c| crate::vault::safe(&self.root, &c.src).ok()).collect();
        let mut k = 1;
        loop {
            let name = if k == 1 { format!("{stem} - mix.{ext}") } else { format!("{stem} - mix {k}.{ext}") };
            let p = dir.join(name);
            if !used.contains(&p) {
                return p;
            }
            k += 1;
        }
    }

    fn levels(&self) -> (f32, f32) {
        let Some((t0, _, data)) = &self.playing else { return (0.0, 0.0) };
        let f = (t0.elapsed().as_secs_f64() * RATE as f64) as usize;
        let w = (RATE as f64 * 0.05) as usize;
        let (a, b) = (f.saturating_sub(w) * 2, (f * 2).min(data.len()));
        let (mut l, mut r) = (0.0f32, 0.0f32);
        for s in data[a.min(b)..b].chunks_exact(2) {
            l = l.max(s[0].abs());
            r = r.max(s[1].abs());
        }
        (l, r)
    }

    // ---------- Pistes ----------

    fn tracks(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let full = ui.max_rect();
        ui.allocate_rect(full, Sense::hover());
        let ruler = Rect::from_min_max(pos2(full.left() + HEAD_W, full.top()), pos2(full.right(), full.top() + 24.0));
        let lanes_x = ruler.x_range();
        let dur = self.session.duration();
        if self.fit && dur > 0.0 {
            self.zoom = ((lanes_x.span() as f64 - 40.0) / dur).clamp(2.0, 2000.0);
            self.scroll = 0.0;
            self.fit = false;
        }
        let (zoom, scroll) = (self.zoom, self.scroll);
        let x_of = move |t: f64| ruler.left() + ((t - scroll) * zoom) as f32 + 8.0;
        let t_of = move |x: f32| ((x - ruler.left() - 8.0) as f64 / zoom + scroll).max(0.0);
        let painter = ui.painter_at(full);
        painter.rect_filled(Rect::from_min_max(full.min, pos2(full.right(), ruler.bottom())), 0.0, p.panel);
        painter.hline(full.x_range(), ruler.bottom(), Stroke::new(1.0, p.line));
        // Règle
        let step = [0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0].into_iter().find(|s| s * zoom > 70.0).unwrap_or(600.0);
        let mut s = (scroll / step).floor() * step;
        while x_of(s) < full.right() {
            let x = x_of(s);
            if x >= ruler.left() {
                painter.vline(x, ruler.top() + 6.0..=ruler.bottom(), Stroke::new(1.0, p.line));
                let label = if step < 1.0 { format!("{}:{:04.1}", (s / 60.0) as u32, s % 60.0) } else { format!("{}:{:02}", (s / 60.0) as u32, (s % 60.0).round() as u32) };
                painter.text(pos2(x + 4.0, ruler.center().y + 1.0), egui::Align2::LEFT_CENTER, label, egui::FontId::monospace(10.5), p.muted);
            }
            s += step;
        }
        for m in &self.session.markers {
            let x = x_of(m.t);
            if x >= ruler.left() {
                painter.add(egui::Shape::convex_polygon(vec![pos2(x, ruler.bottom()), pos2(x - 5.0, ruler.top() + 6.0), pos2(x + 5.0, ruler.top() + 6.0)], p.warning, Stroke::NONE));
            }
        }
        let rr = ui.interact(ruler, ui.scope_id().with("audio-ruler"), Sense::click_and_drag());
        if rr.drag_started() || rr.clicked() {
            self.drag = Some(Drag::Scrub);
        }

        // Pistes
        let n = self.session.tracks.len();
        let mut actions: Vec<Box<dyn FnOnce(&mut Editor)>> = vec![];
        let lane_clip = Rect::from_min_max(pos2(ruler.left(), ruler.bottom()), full.max);
        let lp = ui.painter_at(lane_clip);
        let mut y = ruler.bottom();
        let range = self.range;
        let solo = self.session.tracks.iter().any(|t| t.solo);
        for ti in 0..n {
            let tr = self.session.tracks[ti].clone();
            let row = Rect::from_min_max(pos2(full.left(), y), pos2(full.right(), y + TRACK_H));
            let head = Rect::from_min_max(row.min, pos2(ruler.left(), row.bottom()));
            let lane = Rect::from_min_max(pos2(ruler.left(), row.top()), row.max);
            let active = self.sel_track == ti;
            painter.rect_filled(head, 0.0, if active { p.raised } else { p.panel });
            painter.vline(head.right(), head.y_range(), Stroke::new(1.0, p.line));
            painter.hline(full.x_range(), row.bottom() - 0.5, Stroke::new(1.0, p.line));
            let dim = tr.mute || (solo && !tr.solo);
            // En-tête : couleur, nom, M S, volume
            let mut hu = ui.new_child(egui::UiBuilder::new().max_rect(head.shrink2(vec2(10.0, 8.0))).layout(egui::Layout::top_down(egui::Align::Min)));
            hu.horizontal(|ui| {
                let (d, _) = ui.allocate_exact_size(vec2(8.0, 16.0), Sense::hover());
                ui.painter().circle_filled(d.center(), 4.0, col(tr.color));
                let mut name = tr.name.clone();
                if ui.add(egui::TextEdit::singleline(&mut name).frame(egui::Frame::NONE).font(egui::FontId::new(12.5, crate::theme::bold())).desired_width(ui.available_width())).changed() {
                    actions.push(Box::new(move |e: &mut Editor| {
                        e.session.tracks[ti].name = name;
                        e.changed();
                    }));
                }
            });
            let secs: f64 = tr.clips.iter().map(|c| c.len).sum();
            hu.label(egui::RichText::new(if tr.clips.is_empty() { crate::i18n::t("vide") } else { format!("{} clip(s) · {:.1} s", tr.clips.len(), secs) }).size(11.0).color(p.muted));
            hu.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if ui.add(egui::Button::new(egui::RichText::new("M").size(11.5)).selected(tr.mute).min_size(vec2(22.0, 20.0))).on_hover_text(crate::i18n::t("Muet")).clicked() {
                    actions.push(Box::new(move |e: &mut Editor| {
                        e.session.tracks[ti].mute ^= true;
                        e.changed();
                    }));
                }
                if ui.add(egui::Button::new(egui::RichText::new("S").size(11.5)).selected(tr.solo).min_size(vec2(22.0, 20.0))).on_hover_text(crate::i18n::t("Solo")).clicked() {
                    actions.push(Box::new(move |e: &mut Editor| {
                        e.session.tracks[ti].solo ^= true;
                        e.changed();
                    }));
                }
                let mut v = tr.volume;
                let r = ui.add(egui::Slider::new(&mut v, -40.0..=12.0).show_value(false)).on_hover_text(format!("{v:+.1} dB"));
                if r.changed() {
                    actions.push(Box::new(move |e: &mut Editor| e.session.tracks[ti].volume = v));
                }
                if r.drag_stopped() {
                    actions.push(Box::new(|e: &mut Editor| e.changed()));
                }
            });
            let hr = ui.interact(head, ui.scope_id().with(("audio-head", ti)), Sense::click());
            if hr.clicked() {
                self.sel_track = ti;
            }
            hr.context_menu(|ui| {
                if ui.add(egui::Button::new(format!("{}  {}", crate::lucide::TRASH_2, crate::i18n::t("supprimer la piste")))).clicked() {
                    actions.push(Box::new(move |e: &mut Editor| {
                        e.session.remove_track(ti);
                        e.sel_clip = None;
                        e.sel_track = 0;
                        e.changed();
                    }));
                }
            });
            // Plage choisie
            if let Some((a, b, t0, t1)) = range {
                if ti >= t0.min(t1) && ti <= t0.max(t1) {
                    let r = Rect::from_min_max(pos2(x_of(a), lane.top()), pos2(x_of(b), lane.bottom()));
                    lp.rect_filled(r, 0.0, p.text.gamma_multiply(0.1));
                    lp.vline(r.left(), r.y_range(), Stroke::new(1.0, p.text.gamma_multiply(0.6)));
                    lp.vline(r.right(), r.y_range(), Stroke::new(1.0, p.text.gamma_multiply(0.6)));
                }
            }
            // Clips
            for (ci, c) in tr.clips.iter().enumerate() {
                let len = if c.len > 0.0 { c.len } else { 1.0 };
                let cr = Rect::from_min_max(pos2(x_of(c.start), lane.top() + 4.0), pos2(x_of(c.start + len), lane.bottom() - 4.0));
                if cr.right() < lane.left() || cr.left() > lane.right() {
                    continue;
                }
                let chosen = self.sel_clip == Some((ti, ci));
                let base = col(tr.color);
                lp.rect(cr, 4.0, base.gamma_multiply(if dim { 0.08 } else { 0.16 }), Stroke::new(if chosen { 1.5 } else { 1.0 }, if chosen { p.text } else { base.gamma_multiply(0.5) }), egui::StrokeKind::Inside);
                let bar = Rect::from_min_max(cr.min, pos2(cr.right(), cr.top() + CLIP_BAR));
                lp.rect_filled(bar, egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, base.gamma_multiply(if dim { 0.25 } else { 0.55 }));
                let mut label = c.src.rsplit('/').next().unwrap_or(&c.src).to_string();
                if !c.fx.is_empty() {
                    label = format!("fx  {label}");
                }
                if c.gain.abs() > 0.05 {
                    label = format!("{label}  {:+.1} dB", c.gain);
                }
                lp.with_clip_rect(bar.intersect(lane_clip)).text(pos2(bar.left() + 6.0, bar.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(11.0), p.text);
                // Forme d'onde
                let wave = Rect::from_min_max(pos2(cr.left(), bar.bottom()), cr.max);
                match self.sounds.get(&c.src, &c.fx_key(), Some(ui.ctx())) {
                    Slot::Ready(buf) => {
                        let mid = wave.center().y;
                        let h = wave.height() / 2.0 - 3.0;
                        let x0 = cr.left().max(lane.left());
                        let x1 = cr.right().min(lane.right());
                        let mut x = x0;
                        let color = if dim { p.muted.gamma_multiply(0.5) } else { base };
                        while x < x1 {
                            let ta = t_of(x) - c.start + c.offset;
                            let tb = t_of(x + 1.0) - c.start + c.offset;
                            let (ia, ib) = ((ta * RATE as f64) as usize / audio::PEAK_STEP, ((tb * RATE as f64) as usize / audio::PEAK_STEP).max((ta * RATE as f64) as usize / audio::PEAK_STEP + 1));
                            let (mut lo, mut hi) = (0.0f32, 0.0f32);
                            for (a, b) in buf.peaks.get(ia.min(buf.peaks.len())..ib.min(buf.peaks.len())).unwrap_or(&[]) {
                                lo = lo.min(*a);
                                hi = hi.max(*b);
                            }
                            let g = c.env(t_of(x)).min(4.0);
                            lp.vline(x, (mid - hi * g * h)..=(mid - lo * g * h).max(mid - hi * g * h + 1.0), Stroke::new(1.0, color));
                            x += 1.0;
                        }
                        lp.hline(wave.x_range(), mid, Stroke::new(1.0, p.line.gamma_multiply(0.5)));
                    }
                    Slot::Loading => {
                        lp.text(wave.center(), egui::Align2::CENTER_CENTER, crate::i18n::t("lecture du son…"), egui::FontId::proportional(11.0), p.muted);
                    }
                    Slot::Failed(e) => {
                        lp.text(wave.center(), egui::Align2::CENTER_CENTER, e, egui::FontId::proportional(11.0), p.missing);
                    }
                }
                // Fondus : courbe et poignée en haut des coins
                for (left, f) in [(true, c.fade_in), (false, c.fade_out)] {
                    let fx = if left { x_of(c.start + f as f64) } else { x_of(c.start + len - f as f64) };
                    let corner = if left { pos2(cr.left(), wave.bottom()) } else { pos2(cr.right(), wave.bottom()) };
                    if f > 0.0 {
                        let pts: Vec<Pos2> = (0..=16).map(|k| {
                            let u = k as f32 / 16.0;
                            let x = corner.x + (fx - corner.x) * u;
                            pos2(x, wave.bottom() - (wave.height()) * u.powf(1.5))
                        }).collect();
                        lp.add(egui::Shape::line(pts, Stroke::new(1.2, p.text.gamma_multiply(0.7))));
                    }
                    let hp = pos2(fx, bar.bottom() + 5.0);
                    if chosen || f > 0.0 {
                        lp.rect(Rect::from_center_size(hp, vec2(8.0, 8.0)), 1.0, p.code_bg, Stroke::new(1.0, p.text), egui::StrokeKind::Inside);
                    }
                }
            }
            // Souris sur la piste
            let resp = ui.interact(lane.intersect(lane_clip), ui.scope_id().with(("audio-lane", ti)), Sense::click_and_drag());
            if let Some(h) = resp.hover_pos() {
                let hit = self.hit(ti, h, &x_of, lane);
                match (self.tool, hit) {
                    (Tool::Razor, _) => ui.ctx().set_cursor_icon(egui::CursorIcon::VerticalText),
                    (_, Some((_, Part::Left | Part::Right))) => ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal),
                    (_, Some((_, Part::Bar))) => ui.ctx().set_cursor_icon(egui::CursorIcon::Grab),
                    (_, Some((_, Part::FadeIn | Part::FadeOut))) => ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn),
                    _ => {}
                }
                if self.tool == Tool::Razor {
                    lp.vline(h.x, lane.y_range(), Stroke::new(1.0, p.missing));
                }
            }
            if resp.clicked() {
                if let Some(h) = resp.interact_pointer_pos() {
                    self.sel_track = ti;
                    if self.tool == Tool::Razor {
                        let all = ui.input(|i| i.modifiers.shift);
                        let tracks: Vec<usize> = if all { (0..n).collect() } else { vec![ti] };
                        let t = t_of(h.x);
                        actions.push(Box::new(move |e: &mut Editor| {
                            if e.session.split(&tracks, t) > 0 {
                                e.sel_clip = None;
                                e.changed();
                            }
                        }));
                    } else {
                        self.range = None;
                        self.sel_clip = self.hit(ti, h, &x_of, lane).map(|(ci, _)| (ti, ci));
                        self.stop();
                        self.time = t_of(h.x);
                    }
                }
            }
            if resp.drag_started() && self.tool == Tool::Select {
                if let Some(h) = resp.interact_pointer_pos() {
                    self.sel_track = ti;
                    let t = t_of(h.x);
                    self.drag = Some(match self.hit(ti, h, &x_of, lane) {
                        Some((ci, Part::Bar)) => {
                            self.sel_clip = Some((ti, ci));
                            self.range = None;
                            Drag::Move { track: ti, clip: ci, grab: t - self.session.tracks[ti].clips[ci].start }
                        }
                        Some((ci, Part::Left | Part::Right)) => {
                            let c = &self.session.tracks[ti].clips[ci];
                            let left = matches!(self.hit(ti, h, &x_of, lane), Some((_, Part::Left)));
                            self.sel_clip = Some((ti, ci));
                            Drag::Trim { track: ti, clip: ci, left, start: c.start, offset: c.offset, len: c.len }
                        }
                        Some((ci, Part::FadeIn)) => Drag::Fade { track: ti, clip: ci, left: true },
                        Some((ci, Part::FadeOut)) => Drag::Fade { track: ti, clip: ci, left: false },
                        _ => {
                            self.sel_clip = None;
                            Drag::Range { a: t, track: ti }
                        }
                    });
                }
            }
            // Ducking : la courbe du volume sur la piste, et ce qui la commande
            if let Some(d) = &tr.duck {
                if let Some(curve) = self.duck_view_for(ti) {
                    let (t0, t1) = (t_of(lane.left()), t_of(lane.right()));
                    let top = lane.top() + 6.0;
                    let h = lane.height() - 12.0;
                    let low = audio::db(-d.amount.max(0.1));
                    let mut pts = vec![];
                    let mut tt = t0;
                    let dt = (1.0 / zoom * 3.0).max(0.02);
                    while tt <= t1 + dt {
                        let g = curve.get((tt * 50.0) as usize).copied().unwrap_or(1.0);
                        let k = ((1.0 - g) / (1.0 - low)).clamp(0.0, 1.0);
                        pts.push(pos2(x_of(tt), top + k * h * 0.6));
                        tt += dt;
                    }
                    lp.add(egui::Shape::line(pts, Stroke::new(1.5, p.warning.gamma_multiply(0.9))));
                }
                let under = self.session.tracks.get(d.under).map(|t| t.name.clone()).unwrap_or_default();
                lp.text(pos2(lane.right() - 8.0, lane.bottom() - 6.0), egui::Align2::RIGHT_BOTTOM, format!("↓ {} dB · {under}", d.amount.round()), egui::FontId::proportional(10.5), p.warning);
            }
            y += TRACK_H;
        }
        // + piste
        let add = Rect::from_min_size(pos2(full.left() + 10.0, y + 8.0), vec2(HEAD_W - 20.0, 26.0));
        let ar = ui.interact(add, ui.scope_id().with("audio-add-track"), Sense::click());
        painter.rect(add, 4.0, if ar.hovered() { p.raised } else { Color32::TRANSPARENT }, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
        painter.text(add.center(), egui::Align2::CENTER_CENTER, format!("{}  {}", l::PLUS, crate::i18n::t("Piste")), egui::FontId::proportional(12.0), p.text);
        if ar.clicked() {
            let k = self.session.tracks.len();
            self.session.tracks.push(Session::new_track(k));
            self.sel_track = k;
            self.changed();
        }

        // Glisser
        if let (Some(d), Some(m)) = (self.drag.take(), ui.input(|i| i.pointer.interact_pos())) {
            let t = t_of(m.x);
            let track_at = |y: f32| (((y - ruler.bottom()) / TRACK_H).floor().max(0.0) as usize).min(n.saturating_sub(1));
            // Aimant : bords des autres clips, tête de lecture, repères (8 px)
            let edges: Vec<f64> = self.session.tracks.iter().flat_map(|t| t.clips.iter().flat_map(|c| [c.start, c.end()])).chain([self.time, 0.0]).chain(self.session.markers.iter().map(|m| m.t)).collect();
            let snap = |v: f64, skip: (f64, f64)| edges.iter().cloned().filter(|e| (*e - skip.0).abs() > 1e-9 && (*e - skip.1).abs() > 1e-9).find(|e| ((e - v) * zoom).abs() < 8.0);
            let next = match &d {
                Drag::Scrub => {
                    self.stop();
                    self.time = t;
                    Some(d)
                }
                Drag::Range { a, track } => {
                    let b = snap(t, (f64::NAN, f64::NAN)).unwrap_or(t);
                    let (lo, hi) = if b < *a { (b, *a) } else { (*a, b) };
                    if (hi - lo) * zoom > 2.0 {
                        self.range = Some((lo, hi, *track, track_at(m.y)));
                    }
                    Some(d)
                }
                Drag::Move { track, clip, grab } => {
                    let (ti, ci) = (*track, *clip);
                    let len = self.session.tracks[ti].clips[ci].len;
                    let old = (self.session.tracks[ti].clips[ci].start, self.session.tracks[ti].clips[ci].end());
                    let mut s = (t - grab).max(0.0);
                    if let Some(e) = snap(s, old) {
                        s = e;
                    } else if let Some(e) = snap(s + len, old) {
                        s = e - len;
                    }
                    let to = track_at(m.y);
                    self.session.tracks[ti].clips[ci].start = s.max(0.0);
                    if to != ti {
                        let c = self.session.tracks[ti].clips.remove(ci);
                        self.session.tracks[to].clips.push(c);
                        let nc = self.session.tracks[to].clips.len() - 1;
                        self.sel_clip = Some((to, nc));
                        Some(Drag::Move { track: to, clip: nc, grab: *grab })
                    } else {
                        Some(d)
                    }
                }
                Drag::Trim { track, clip, left, start, offset, len } => {
                    let max = self.sounds.ready(&self.session.tracks[*track].clips[*clip]).map(|b| b.secs()).unwrap_or(f64::MAX);
                    let c = &mut self.session.tracks[*track].clips[*clip];
                    let t = snap(t, (*start, start + len)).unwrap_or(t);
                    if *left {
                        let d = (t - start).clamp(-offset, len - 0.02);
                        c.start = start + d;
                        c.offset = offset + d;
                        c.len = len - d;
                    } else {
                        c.len = (t - start).clamp(0.02, max - offset);
                    }
                    Some(d)
                }
                Drag::Fade { track, clip, left } => {
                    let c = &mut self.session.tracks[*track].clips[*clip];
                    let v = if *left { t - c.start } else { c.end() - t };
                    let v = v.clamp(0.0, c.len * 0.9) as f32;
                    if *left {
                        c.fade_in = v;
                    } else {
                        c.fade_out = v;
                    }
                    Some(d)
                }
            };
            if ui.input(|i| i.pointer.any_down()) {
                self.drag = next;
            } else if !matches!(next, Some(Drag::Scrub) | Some(Drag::Range { .. })) {
                self.changed();
            }
        }
        for a in actions {
            a(self);
        }
        // Molette : défilement ; Ctrl : zoom autour de la souris
        if ui.rect_contains_pointer(full) {
            let (dy, dx, cmd, hover) = ui.input(|i| (i.smooth_scroll_delta.y, i.smooth_scroll_delta.x, i.modifiers.command, i.pointer.hover_pos()));
            if cmd && dy != 0.0 {
                let at = hover.map(|h| t_of(h.x)).unwrap_or(self.time);
                self.zoom = (self.zoom * (1.0 + dy as f64 * 0.003)).clamp(2.0, 4000.0);
                self.scroll = (at - (hover.map(|h| h.x).unwrap_or(ruler.center().x) - ruler.left() - 8.0) as f64 / self.zoom).max(0.0);
            } else if dy != 0.0 || dx != 0.0 {
                self.scroll = (self.scroll - (dy + dx) as f64 / self.zoom).max(0.0);
            }
        }
        // Tête de lecture
        let x = x_of(self.time);
        if x >= ruler.left() {
            painter.vline(x, ruler.top()..=full.bottom(), Stroke::new(1.0, p.accent));
            painter.add(egui::Shape::convex_polygon(vec![pos2(x - 5.0, ruler.top()), pos2(x + 5.0, ruler.top()), pos2(x, ruler.top() + 7.0)], p.accent, Stroke::NONE));
        }
        if n == 0 || dur == 0.0 {
            painter.text(lane_clip.center(), egui::Align2::CENTER_CENTER, crate::i18n::t("Clique un son à gauche pour le poser sur la piste active."), egui::FontId::proportional(12.5), p.muted);
        }
        if let Some((m, at)) = &self.message {
            if at.elapsed().as_secs_f32() < 5.0 {
                let r = Rect::from_min_size(pos2(lane_clip.left() + 12.0, full.bottom() - 36.0), vec2(lane_clip.width() - 24.0, 26.0));
                painter.rect_filled(r, 4.0, p.raised);
                painter.text(r.left_center() + vec2(10.0, 0.0), egui::Align2::LEFT_CENTER, m, egui::FontId::proportional(12.0), p.text);
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
            }
        }
        if let Some(e) = &self.error {
            painter.text(pos2(lane_clip.left() + 12.0, lane_clip.top() + 14.0), egui::Align2::LEFT_CENTER, e, egui::FontId::proportional(12.0), p.missing);
        }
    }

    fn hit(&self, ti: usize, h: Pos2, x_of: &dyn Fn(f64) -> f32, lane: Rect) -> Option<(usize, Part)> {
        let tr = self.session.tracks.get(ti)?;
        for (ci, c) in tr.clips.iter().enumerate().rev() {
            let len = if c.len > 0.0 { c.len } else { 1.0 };
            let (l, r) = (x_of(c.start), x_of(c.start + len));
            if h.x < l - 4.0 || h.x > r + 4.0 {
                continue;
            }
            let top = lane.top() + 4.0;
            let fi = x_of(c.start + c.fade_in as f64);
            let fo = x_of(c.start + len - c.fade_out as f64);
            let fy = top + CLIP_BAR + 5.0;
            if (h.y - fy).abs() < 6.0 {
                if (h.x - fi).abs() < 6.0 {
                    return Some((ci, Part::FadeIn));
                }
                if (h.x - fo).abs() < 6.0 {
                    return Some((ci, Part::FadeOut));
                }
            }
            if (h.x - l).abs() < 5.0 {
                return Some((ci, Part::Left));
            }
            if (h.x - r).abs() < 5.0 {
                return Some((ci, Part::Right));
            }
            if h.y < top + CLIP_BAR {
                return Some((ci, Part::Bar));
            }
            if h.x >= l && h.x <= r {
                return Some((ci, Part::Body));
            }
        }
        None
    }

    // ---------- Inspecteur ----------

    fn inspector(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        if let Some(k) = crate::montage_ui::tab_chips(ui, self.insp_tab, &[&crate::i18n::t("Effets"), &crate::i18n::t("Clip"), &crate::i18n::t("Spectre")]) {
            self.insp_tab = k;
        }
        egui::Frame::new().inner_margin(12).show(ui, |ui| {
            let what = match (self.range, self.sel_clip) {
                (Some((a, b, t0, t1)), _) => {
                    let names: Vec<String> = (t0.min(t1)..=t0.max(t1)).filter_map(|i| self.session.tracks.get(i).map(|t| t.name.clone())).collect();
                    format!("{} · {:.1} s · {}", crate::i18n::t("Sur la plage"), b - a, names.join(", "))
                }
                (None, Some((ti, ci))) => format!("{} · {}", crate::i18n::t("Sur le clip"), self.session.tracks.get(ti).and_then(|t| t.clips.get(ci)).map(|c| c.src.rsplit('/').next().unwrap_or("").to_string()).unwrap_or_default()),
                _ => crate::i18n::t("Choisis un clip, ou glisse sur une piste pour prendre une plage."),
            };
            ui.label(egui::RichText::new(what).size(11.5).color(p.muted));
            if self.insp_tab == 1 {
                self.clip_props(ui);
                return;
            }
            if self.insp_tab == 2 {
                self.spectrum_view(ui);
                return;
            }
            let has = self.range.is_some() || self.sel_clip.is_some();
            let fx_btn = |ui: &mut egui::Ui, title: &str, sub: &str| -> bool {
                let b = egui::Button::new(egui::RichText::new(format!("{}\n{}", crate::i18n::t(title), crate::i18n::t(sub))).size(12.0)).min_size(vec2(ui.available_width(), 38.0));
                ui.add_enabled(has, b).clicked()
            };
            section(ui, &crate::i18n::t("Volume"));
            if fx_btn(ui, "Normaliser", "crête à −1 dB") {
                let targets = self.fx_targets();
                for (ti, ci) in targets {
                    let c = self.session.tracks[ti].clips[ci].clone();
                    if let Some(b) = self.sounds.ready(&c) {
                        self.session.tracks[ti].clips[ci].gain = audio::normalize_gain(&c, &b, -1.0);
                    }
                }
                self.changed();
            }
            // Sonie (LUFS) : la cible des plateformes, la crête gardée sous -1 dB
            ui.horizontal(|ui| {
                let cur = audio::LOUDNESS.iter().find(|l| l.0 == self.loud_target).map(|l| format!("{:.0} LUFS · {}", l.0, crate::i18n::t(l.1))).unwrap_or_else(|| format!("{:.0} LUFS", self.loud_target));
                egui::ComboBox::from_id_salt("loud-target").width(ui.available_width() - 40.0).selected_text(cur).show_ui(ui, |ui| {
                    for (v, name) in audio::LOUDNESS {
                        ui.selectable_value(&mut self.loud_target, v, format!("{v:.0} LUFS · {}", crate::i18n::t(name)));
                    }
                });
                let go = ui.add_enabled(has, egui::Button::new(egui::RichText::new(crate::lucide::AUDIO_LINES).size(15.0))).on_hover_text(crate::i18n::t("Normaliser la sonie : chaque clip choisi atteint la cible, crête sous -1 dB"));
                if go.clicked() {
                    let mut notes = vec![];
                    for (ti, ci) in self.fx_targets() {
                        let c = self.session.tracks[ti].clips[ci].clone();
                        if let Some(b) = self.sounds.ready(&c) {
                            if let Some((g, measured, limited)) = audio::loudness_gain(&c, &b, self.loud_target) {
                                self.session.tracks[ti].clips[ci].gain = g;
                                notes.push(format!("{:.1} → {:.1} LUFS{}", measured, measured + g, if limited { format!(" ({})", crate::i18n::t("limité par la crête")) } else { String::new() }));
                            }
                        }
                    }
                    if !notes.is_empty() {
                        self.message = Some((notes.join(" · "), Instant::now()));
                        self.changed();
                    }
                }
            });
            if fx_btn(ui, "Fondu entrant et sortant", "une demi-seconde de chaque côté") {
                for (ti, ci) in self.fx_targets() {
                    let c = &mut self.session.tracks[ti].clips[ci];
                    c.fade_in = 0.5f32.min(c.len as f32 / 3.0);
                    c.fade_out = 0.5f32.min(c.len as f32 / 3.0);
                }
                self.changed();
            }
            section(ui, &crate::i18n::t("Nettoyer"));
            if fx_btn(ui, "Voix claire", "coupe-bas, bruit, présence, compression") {
                self.add_fx(Fx::Voice);
            }
            if fx_btn(ui, "Réduction de bruit", "souffle et bourdonnement") {
                self.add_fx(Fx::Denoise { amount: 0.5 });
            }
            if fx_btn(ui, "Coupe-bas", "retire les graves sous 100 Hz") {
                self.add_fx(Fx::Highpass { hz: 100.0 });
            }
            if fx_btn(ui, "Couper le silence", "retire les blancs et resserre la piste") {
                let mut saved = 0.0;
                let targets = self.fx_targets();
                // De la fin vers le début : les indices restent justes
                for (ti, ci) in targets.into_iter().rev() {
                    let c = self.session.tracks[ti].clips[ci].clone();
                    if let Some(b) = self.sounds.ready(&c) {
                        saved += audio::cut_silences(&mut self.session, ti, ci, &b);
                    }
                }
                self.range = None;
                self.sel_clip = None;
                self.say(format!("{:.1} s {}", saved, crate::i18n::t_mid("de silence retirées")));
                self.changed();
            }
            section(ui, &crate::i18n::t("Couleur du son"));
            if fx_btn(ui, "Égaliseur", "plus de clarté : +3 dB d'aigus, −2 dB de graves") {
                self.add_fx(Fx::Eq { low: -2.0, mid: 0.0, high: 3.0 });
            }
            if fx_btn(ui, "Égaliseur chaud", "+3 dB de graves, −1 dB d'aigus") {
                self.add_fx(Fx::Eq { low: 3.0, mid: 0.0, high: -1.0 });
            }
            if fx_btn(ui, "Compresseur", "voix parlée, niveau plus égal") {
                self.add_fx(Fx::Compress);
            }
            if fx_btn(ui, "De-esser", "adoucit les « s » qui sifflent") {
                self.add_fx(Fx::Deess);
            }
            section(ui, &crate::i18n::t("Espace et voix"));
            if fx_btn(ui, "Réverbération", "une pièce : réglable dans l'onglet Clip") {
                self.add_fx(Fx::Reverb { amount: 0.35 });
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let w = (ui.available_width() - 4.0) / 2.0;
                for (label, st) in [("Voix plus grave", -3.0f32), ("Voix plus aiguë", 3.0)] {
                    let b = egui::Button::new(egui::RichText::new(format!("{}\n{st:+.0} {}", crate::i18n::t(label), crate::i18n::t_mid("demi-tons"))).size(12.0)).min_size(vec2(w, 38.0));
                    if ui.add_enabled(has, b).on_hover_text(crate::i18n::t("Change la hauteur sans changer la durée")).clicked() {
                        self.add_fx(Fx::Pitch { semitones: st });
                    }
                }
            });
            section(ui, &crate::i18n::t("Montage"));
            if fx_btn(ui, "Supprimer", "et resserrer la piste (Suppr)") {
                self.delete_selection();
            }
        });
    }

    /// Le spectre : en direct pendant la lecture, sinon celui de la plage ou du clip choisi
    fn spectrum_view(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let bands = 96;
        let live = self.playing.as_ref().map(|(t0, _, data)| {
            let f = (t0.elapsed().as_secs_f64() * RATE as f64) as usize;
            let a = f.saturating_sub(4096).min(data.len() / 2);
            let b = (a + 4096).min(data.len() / 2);
            audio::Buffer::from_samples(data[a * 2..b * 2].to_vec())
        });
        let (what, sp) = if let Some(buf) = live {
            ui.ctx().request_repaint();
            (crate::i18n::t("En direct"), audio::spectrum(&buf, 0.0, buf.secs(), bands))
        } else {
            // La source : le clip choisi, ou le premier clip sous la plage
            let src = match (self.range, self.sel_clip) {
                (Some((a, b, t0, _)), _) => self.session.tracks.get(t0).and_then(|t| t.clips.iter().find(|c| c.end() > a && c.start < b)).map(|c| (c.clone(), c.offset + (a - c.start).max(0.0), c.offset + (b.min(c.end()) - c.start))),
                (None, Some((ti, ci))) => self.session.tracks.get(ti).and_then(|t| t.clips.get(ci)).map(|c| (c.clone(), c.offset, c.offset + c.len)),
                _ => None,
            };
            let Some((c, a, b)) = src else {
                ui.label(egui::RichText::new(crate::i18n::t("Lance la lecture, ou choisis un clip ou une plage.")).size(11.5).color(p.muted));
                return;
            };
            let key = format!("{}|{}|{a:.3}|{b:.3}", c.src, c.fx_key());
            if self.spec.as_ref().is_none_or(|(k, _)| *k != key) {
                match self.sounds.ready(&c) {
                    Some(buf) => self.spec = Some((key.clone(), audio::spectrum(&buf, a, b, bands))),
                    None => {
                        ui.label(egui::RichText::new(crate::i18n::t("lecture du son…")).color(p.muted));
                        return;
                    }
                }
            }
            (format!("{} · {:.1} s", c.src.rsplit('/').next().unwrap_or(""), b - a), self.spec.as_ref().map(|s| s.1.clone()).unwrap_or_default())
        };
        ui.label(egui::RichText::new(what).size(11.5).color(p.muted));
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 170.0), Sense::hover());
        let pt = ui.painter_at(r);
        pt.rect(r, 3.0, p.code_bg, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
        let g = r.shrink2(vec2(6.0, 14.0));
        let (lo, hi) = (-90.0f32, 0.0f32);
        let x_of = |f: f64| g.left() + g.width() * ((f / 20.0).log10() / 3.0) as f32;
        for (f, label) in [(100.0, "100"), (1000.0, "1k"), (10000.0, "10k")] {
            let x = x_of(f);
            pt.vline(x, g.y_range(), Stroke::new(1.0, p.line.gamma_multiply(0.6)));
            pt.text(pos2(x + 3.0, r.bottom() - 7.0), egui::Align2::LEFT_CENTER, label, egui::FontId::monospace(9.5), p.muted);
        }
        for db in [-60.0f32, -30.0] {
            let y = g.bottom() - (db - lo) / (hi - lo) * g.height();
            pt.hline(g.x_range(), y, Stroke::new(1.0, p.line.gamma_multiply(0.4)));
            pt.text(pos2(g.left() + 2.0, y - 6.0), egui::Align2::LEFT_CENTER, format!("{db:.0}"), egui::FontId::monospace(9.5), p.muted);
        }
        let n = sp.len().max(1);
        let pts: Vec<Pos2> = sp.iter().enumerate().map(|(j, v)| pos2(g.left() + g.width() * (j as f32 + 0.5) / n as f32, g.bottom() - ((v.clamp(lo, hi) - lo) / (hi - lo)) * g.height())).collect();
        // Remplissage en bandes verticales (le polygone n'est pas convexe)
        for w2 in pts.windows(2) {
            pt.add(egui::Shape::convex_polygon(vec![w2[0], w2[1], pos2(w2[1].x, g.bottom()), pos2(w2[0].x, g.bottom())], p.accent.gamma_multiply(0.18), Stroke::NONE));
        }
        pt.add(egui::Shape::line(pts, Stroke::new(1.5, p.accent)));
        // Ce qui domine
        if let Some((j, v)) = sp.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)) {
            let f = 20.0 * 1000f64.powf((j as f64 + 0.5) / n as f64);
            ui.label(egui::RichText::new(format!("{} ≈ {} Hz · {:.0} dB", crate::i18n::t("Pic"), f.round(), v)).size(11.5));
        }
    }

    fn delete_selection(&mut self) {
        if let Some((a, b, t0, t1)) = self.range.take() {
            let tracks: Vec<usize> = (t0.min(t1)..=t0.max(t1)).collect();
            self.session.delete_range(&tracks, a, b, true);
            self.time = a;
            self.changed();
        } else if let Some((ti, ci)) = self.sel_clip.take() {
            if ti < self.session.tracks.len() && ci < self.session.tracks[ti].clips.len() {
                self.session.tracks[ti].clips.remove(ci);
                self.changed();
            }
        }
    }

    fn clip_props(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let Some((ti, ci)) = self.sel_clip.filter(|(t, c)| self.session.tracks.get(*t).is_some_and(|tr| *c < tr.clips.len())) else {
            // Sans clip : les réglages de la piste choisie
            if self.sel_track < self.session.tracks.len() && self.track_props(ui, self.sel_track) {
                self.changed();
            }
            return;
        };
        let mut changed = false;
        let mut fx_edit = false;
        let mut drop_fx = None;
        {
            let c = &mut self.session.tracks[ti].clips[ci];
            section(ui, &crate::i18n::t("Clip"));
            egui::Grid::new("audio-clip").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
                ui.label(egui::RichText::new(crate::i18n::t("Gain")).color(p.muted));
                changed |= ui.add(egui::DragValue::new(&mut c.gain).speed(0.1).range(-60.0..=24.0).max_decimals(1).suffix(" dB")).changed();
                ui.end_row();
                ui.label(egui::RichText::new(crate::i18n::t("Fondu entrant")).color(p.muted));
                changed |= ui.add(egui::DragValue::new(&mut c.fade_in).speed(0.01).range(0.0..=(c.len as f32)).max_decimals(2).suffix(" s")).changed();
                ui.end_row();
                ui.label(egui::RichText::new(crate::i18n::t("Fondu sortant")).color(p.muted));
                changed |= ui.add(egui::DragValue::new(&mut c.fade_out).speed(0.01).range(0.0..=(c.len as f32)).max_decimals(2).suffix(" s")).changed();
                ui.end_row();
                ui.label(egui::RichText::new(crate::i18n::t("Début")).color(p.muted));
                changed |= ui.add(egui::DragValue::new(&mut c.start).speed(0.01).range(0.0..=36000.0).max_decimals(3).suffix(" s")).changed();
                ui.end_row();
                ui.label(egui::RichText::new(crate::i18n::t("Durée")).color(p.muted));
                ui.label(format!("{:.3} s", c.len));
                ui.end_row();
            });
            section(ui, &crate::i18n::t("Effets du clip"));
            if c.fx.is_empty() {
                ui.label(egui::RichText::new(crate::i18n::t("Aucun effet.")).size(11.5).color(p.muted));
            }
            for (k, f) in self.session.tracks[ti].clips[ci].fx.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    // Les effets à un réglage se règlent ici
                    match f {
                        Fx::Pitch { semitones } => {
                            ui.label(crate::i18n::t("Hauteur"));
                            fx_edit |= ui.add(egui::DragValue::new(semitones).range(-12.0..=12.0).speed(0.05).max_decimals(1).suffix(" st")).changed();
                        }
                        Fx::Reverb { amount } => {
                            ui.label(crate::i18n::t("Réverbération"));
                            let mut pc = *amount * 100.0;
                            if ui.add(egui::DragValue::new(&mut pc).range(0.0..=100.0).speed(0.5).max_decimals(0).suffix(" %")).changed() {
                                *amount = pc / 100.0;
                                fx_edit = true;
                            }
                        }
                        Fx::Denoise { amount } => {
                            ui.label(crate::i18n::t("Réduction de bruit"));
                            let mut pc = *amount * 100.0;
                            if ui.add(egui::DragValue::new(&mut pc).range(0.0..=100.0).speed(0.5).max_decimals(0).suffix(" %")).changed() {
                                *amount = pc / 100.0;
                                fx_edit = true;
                            }
                        }
                        _ => {
                            ui.label(f.label());
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(l::X).on_hover_text(crate::i18n::t("retirer")).clicked() {
                            drop_fx = Some(k);
                        }
                    });
                });
            }
        }
        if let Some(k) = drop_fx {
            self.session.tracks[ti].clips[ci].fx.remove(k);
            changed = true;
        }
        changed |= fx_edit;
        changed |= self.track_props(ui, ti);
        if changed {
            self.changed();
        }
    }

    /// Réglages de la piste : couleur, volume, ducking
    fn track_props(&mut self, ui: &mut egui::Ui, ti: usize) -> bool {
        let mut changed = false;
        let tr = &mut self.session.tracks[ti];
        section(ui, &crate::i18n::t("Piste"));
        ui.horizontal(|ui| {
            changed |= ui.color_edit_button_srgb(&mut tr.color).changed();
            changed |= ui.add(egui::DragValue::new(&mut tr.volume).speed(0.1).range(-60.0..=12.0).max_decimals(1).prefix(crate::i18n::t("volume ")).suffix(" dB")).changed();
        });
        // Ducking : la piste baisse quand une autre joue (la musique sous la voix)
        let names: Vec<(usize, String)> = self.session.tracks.iter().enumerate().filter(|(i, _)| *i != ti).map(|(i, t)| (i, t.name.clone())).collect();
        let tr = &mut self.session.tracks[ti];
        ui.horizontal(|ui| {
            ui.label(crate::i18n::t("Baisser sous"));
            let cur = tr.duck.as_ref().and_then(|d| names.iter().find(|n| n.0 == d.under)).map(|n| n.1.clone()).unwrap_or_else(|| crate::i18n::t("aucune piste"));
            egui::ComboBox::from_id_salt(("duck", ti)).selected_text(cur).show_ui(ui, |ui| {
                if ui.selectable_label(tr.duck.is_none(), crate::i18n::t("aucune piste")).clicked() {
                    tr.duck = None;
                    changed = true;
                }
                for (i, n) in &names {
                    if ui.selectable_label(tr.duck.as_ref().is_some_and(|d| d.under == *i), n).clicked() {
                        let keep = tr.duck.take();
                        tr.duck = Some(audio::Duck { under: *i, ..keep.unwrap_or_else(|| audio::Duck::new(*i)) });
                        changed = true;
                    }
                }
            }).response.on_hover_text(crate::i18n::t("La piste baisse toute seule quand celle-ci joue (la musique sous la voix)"));
        });
        if let Some(d) = &mut tr.duck {
            ui.horizontal(|ui| {
                changed |= ui.add(egui::DragValue::new(&mut d.amount).speed(0.2).range(1.0..=40.0).max_decimals(0).prefix(crate::i18n::t("de ")).suffix(" dB")).changed();
                changed |= ui.add(egui::DragValue::new(&mut d.release).speed(0.01).range(0.05..=3.0).max_decimals(2).prefix(crate::i18n::t("remontée ")).suffix(" s")).changed();
            });
        }
        changed
    }

    /// Gains du ducking de la piste `ti`, toutes les 20 ms depuis 0 ; recalculés quand la piste de
    /// référence, ses sons ou les réglages changent
    fn duck_view_for(&mut self, ti: usize) -> Option<Arc<Vec<f32>>> {
        let tr = self.session.tracks.get(ti)?;
        let d = tr.duck.as_ref()?;
        let key = self.session.tracks.get(d.under)?;
        let ready = key.clips.iter().filter(|c| self.sounds.ready(c).is_some()).count();
        let sig = format!("{}|{}|{}|{}", serde_json::to_string(&key.clips).unwrap_or_default(), key.mute, serde_json::to_string(d).unwrap_or_default(), ready);
        if let Some((s, c)) = self.duck_view.get(&ti) {
            if *s == sig {
                return Some(c.clone());
            }
        }
        let dur = self.session.duration();
        let steps = (dur * 50.0).ceil() as usize + 1;
        let curve = Arc::new(audio::duck_curve(&self.session, &self.sounds, ti, 0.0, steps as f64 / 50.0, steps).unwrap_or_default());
        self.duck_view.insert(ti, (sig, curve.clone()));
        Some(curve)
    }

    // ---------- État ----------

    fn status(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let r = ui.max_rect().shrink2(vec2(12.0, 0.0));
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::left_to_right(egui::Align::Center)));
        let k = |ui: &mut egui::Ui, s: &str| {
            ui.label(egui::RichText::new(crate::i18n::t(s)).size(11.5).color(p.muted));
        };
        let v = |ui: &mut egui::Ui, s: String| {
            ui.label(egui::RichText::new(s).monospace().size(12.0));
        };
        match self.range {
            Some((a, b, ..)) => {
                k(&mut ui, "Sélection");
                v(&mut ui, format!("{} → {}", clock(a), clock(b)));
                k(&mut ui, "durée");
                v(&mut ui, format!("{:.3} s", b - a).replace('.', ","));
            }
            None => {
                k(&mut ui, "Session");
                v(&mut ui, clock(self.session.duration()));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            v(ui, "48 000 Hz".into());
            k(ui, "Fréquence du projet");
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Part {
    Bar,
    Body,
    Left,
    Right,
    FadeIn,
    FadeOut,
}
