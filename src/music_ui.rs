//! Le séquenceur : l'arrangement (pistes et régions) en haut, le rouleau de piano de la région
//! choisie en bas, l'instrument et les idées à droite.

use crate::lucide as l;
use crate::music::{self, Inst, Note, Region, Song, INSTS, KIT};
use eframe::egui::{self, pos2, vec2, Color32, Rect, Sense, Stroke};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

const HEAD_W: f32 = 180.0;
const TRACK_H: f32 = 62.0;
const KEYS_W: f32 = 70.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Arrange,
    Roll,
}

enum Drag {
    Scrub,
    Region { track: usize, region: usize, grab: f64, mode: u8, start: f64, len: f64 },
    /// Les notes choisies glissent ensemble : (indice, début, hauteur, longueur) au départ
    Notes { grab: f64, row0: i64, resize: bool, orig: Vec<(usize, f64, u8, f64)> },
    /// Cadre de sélection dans le rouleau : coin de départ (temps, rangée)
    Box { a: (f64, i64), additive: bool },
}

pub struct Editor {
    pub path: PathBuf,
    pub song: Song,
    error: Option<String>,
    message: Option<(String, Instant)>,
    /// Tête de lecture, en temps
    beat: f64,
    playing: Option<(Instant, f64, f64)>,
    pending: Option<Receiver<(f64, f64, Vec<f32>)>>,
    player: Option<std::process::Child>,
    looping: bool,
    sel_track: usize,
    sel_region: Option<(usize, usize)>,
    /// Notes choisies dans la région du rouleau
    sel_notes: Vec<usize>,
    clipboard: Vec<Note>,
    /// Pas de la grille du rouleau, en temps (0,25 = double croche)
    grid: f64,
    last_len: f64,
    zoom: f32,
    /// Défilement de l'arrangement (en temps)
    ascroll: f64,
    roll_zoom: f32,
    roll_scroll: f64,
    roll_top: f32,
    focus: Focus,
    drag: Option<Drag>,
    saved: Song,
    history: Vec<Song>,
    redo: Vec<Song>,
    insp_tab: u8,
    seed: u64,
    /// Région que montre le rouleau (pour le recadrer quand elle change)
    roll_for: Option<(usize, usize)>,
    export_rx: Option<Receiver<Result<PathBuf, String>>>,
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn col(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(egui::RichText::new(crate::i18n::t(title)).size(11.5).color(crate::theme::pal().muted));
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

/// Joue un son court (aperçu d'une note) sans rien attendre
fn preview(data: Vec<f32>) {
    let Some(fp) = crate::ffmpeg::find("ffplay") else { return };
    std::thread::spawn(move || {
        use std::io::Write;
        let stereo: Vec<f32> = data.iter().flat_map(|v| [*v * 0.8, *v * 0.8]).collect();
        if let Ok(mut c) = crate::ffmpeg::command(fp).args(["-nodisp", "-autoexit", "-loglevel", "quiet", "-i", "-"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
            if let Some(mut s) = c.stdin.take() {
                let _ = s.write_all(&crate::audio::wav_bytes(&stereo));
            }
            let _ = c.wait();
        }
    });
}

impl Editor {
    pub fn open(path: PathBuf) -> Editor {
        let (song, error) = match Song::read(&path) {
            Ok(s) => (s, None),
            Err(e) => (Song::starter(), Some(e)),
        };
        let sel_region = song.tracks.iter().enumerate().find(|(_, t)| !t.regions.is_empty()).map(|(i, _)| (i, 0));
        Editor {
            path,
            saved: song.clone(),
            song,
            error,
            message: None,
            beat: 0.0,
            playing: None,
            pending: None,
            player: None,
            looping: true,
            sel_track: sel_region.map(|r| r.0).unwrap_or(0),
            sel_region,
            sel_notes: vec![],
            clipboard: vec![],
            grid: 0.25,
            last_len: 0.5,
            zoom: 0.0,
            ascroll: 0.0,
            roll_zoom: 0.0,
            roll_scroll: 0.0,
            roll_top: 79.0,
            focus: Focus::Arrange,
            drag: None,
            history: vec![],
            redo: vec![],
            insp_tab: 0,
            seed: 1,
            roll_for: None,
            export_rx: None,
        }
    }

    fn say(&mut self, m: impl Into<String>) {
        self.message = Some((m.into(), Instant::now()));
    }

    fn changed(&mut self) {
        if self.saved == self.song {
            return;
        }
        for t in &mut self.song.tracks {
            t.regions.sort_by(|a, b| a.start.total_cmp(&b.start));
        }
        self.history.push(std::mem::replace(&mut self.saved, self.song.clone()));
        if self.history.len() > 200 {
            self.history.remove(0);
        }
        self.redo.clear();
        if self.error.is_none() {
            if let Err(e) = self.song.write(&self.path) {
                self.say(e);
            }
        }
    }

    fn undo(&mut self, forward: bool) {
        let (from, to) = if forward { (&mut self.redo, &mut self.history) } else { (&mut self.history, &mut self.redo) };
        if let Some(s) = from.pop() {
            to.push(std::mem::replace(&mut self.song, s));
            self.saved = self.song.clone();
            self.sel_notes.clear();
            self.sel_region = self.sel_region.filter(|(t, r)| self.song.tracks.get(*t).is_some_and(|tr| *r < tr.regions.len()));
            let _ = self.song.write(&self.path);
        }
    }

    fn region(&self) -> Option<&Region> {
        self.sel_region.and_then(|(t, r)| self.song.tracks.get(t)?.regions.get(r))
    }

    fn region_mut(&mut self) -> Option<&mut Region> {
        let (t, r) = self.sel_region?;
        self.song.tracks.get_mut(t)?.regions.get_mut(r)
    }

    fn stop(&mut self) {
        if let Some(mut c) = self.player.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.playing = None;
        self.pending = None;
    }

    /// Le rendu se fait dans un fil ; la lecture part quand il est prêt
    fn play(&mut self) {
        self.stop();
        let end = self.song.beats();
        if self.beat >= end - 0.01 {
            self.beat = 0.0;
        }
        let (a, b) = (self.beat, end);
        let song = self.song.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send((a, b, music::render(&song, a, b)));
        });
        self.pending = Some(rx);
    }

    fn start_player(&mut self, a: f64, b: f64, data: Vec<f32>) {
        self.playing = Some((Instant::now(), a, b));
        let Some(fp) = crate::ffmpeg::find("ffplay") else {
            self.say(crate::i18n::t("ffplay manque pour écouter (il vient avec ffmpeg)."));
            return;
        };
        if let Ok(mut c) = crate::ffmpeg::command(fp).args(["-nodisp", "-autoexit", "-loglevel", "quiet", "-i", "-"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
            if let Some(mut s) = c.stdin.take() {
                let data = Arc::new(data);
                std::thread::spawn(move || {
                    use std::io::Write;
                    let _ = s.write_all(&crate::audio::wav_bytes(&data));
                });
            }
            self.player = Some(c);
        }
    }

    fn toggle_play(&mut self) {
        if self.playing.is_some() || self.pending.is_some() {
            self.stop();
        } else {
            self.play();
        }
    }

    fn preview_note(&self, track: usize, p: u8) {
        if let Some(tr) = self.song.tracks.get(track) {
            preview(music::voice(tr.inst, p, 0.8, 0.35));
        }
    }

    /// La région où écrire une idée : la région choisie de la piste, sinon une nouvelle de 4 mesures à la tête de lecture
    fn idea_region(&mut self, track: usize) -> (usize, usize) {
        if let Some((t, r)) = self.sel_region.filter(|(t, _)| *t == track) {
            return (t, r);
        }
        let bar = self.song.beats_per_bar as f64;
        let start = (self.beat / bar).floor() * bar;
        let tr = &mut self.song.tracks[track];
        tr.regions.push(Region { start, len: bar * 4.0, name: String::new(), notes: vec![] });
        tr.regions.sort_by(|a, b| a.start.total_cmp(&b.start));
        let r = tr.regions.iter().position(|g| g.start == start && g.notes.is_empty()).unwrap_or(0);
        self.sel_region = Some((track, r));
        (track, r)
    }

    fn import_midi(&mut self) {
        let Some(f) = rfd::FileDialog::new().add_filter("MIDI", &["mid", "midi"]).pick_file() else { return };
        let r = std::fs::read(&f).map_err(|e| e.to_string()).and_then(|b| crate::midi::read(&b));
        match r {
            Ok(m) => {
                let fresh = self.song.tracks.iter().all(|t| t.regions.is_empty());
                if fresh {
                    self.song.bpm = m.bpm;
                }
                self.song.bars = self.song.bars.max(m.bars);
                let n = m.tracks.len();
                for mut t in m.tracks {
                    t.color = crate::audio::COLORS[self.song.tracks.len() % crate::audio::COLORS.len()];
                    self.song.tracks.push(t);
                }
                self.zoom = 0.0;
                self.say(format!("{n} {}", crate::i18n::t_mid("piste(s) importée(s)")));
                self.changed();
            }
            Err(e) => self.say(e),
        }
    }

    /// Un fichier MIDI du coffre ouvert seul : un morceau à côté de lui (ou celui qui existe déjà)
    pub fn song_for(root: &str, rel: &str) -> Result<String, String> {
        let stem = std::path::Path::new(rel).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "morceau".into());
        let out = crate::vault::join(&crate::vault::parent(rel), &format!("{stem}.{}", crate::formats::ext("music")));
        let path = crate::vault::safe(root, &out)?;
        if !path.exists() {
            let bytes = std::fs::read(crate::vault::safe(root, rel)?).map_err(|e| e.to_string())?;
            crate::midi::read(&bytes)?.write(&path)?;
        }
        Ok(out)
    }

    fn delete_notes(&mut self) {
        let mut ks = std::mem::take(&mut self.sel_notes);
        ks.sort_unstable();
        ks.dedup();
        if ks.is_empty() {
            return;
        }
        if let Some(g) = self.region_mut() {
            for k in ks.into_iter().rev() {
                if k < g.notes.len() {
                    g.notes.remove(k);
                }
            }
        }
        self.changed();
    }

    /// Raccourcis du rouleau : tout choisir, copier, coller à la tête de lecture, transposer, décaler
    fn roll_keys(&mut self, i: &egui::InputState) {
        let cmd = i.modifiers.command;
        let Some(g) = self.region().cloned() else { return };
        if cmd && i.key_pressed(egui::Key::A) {
            self.sel_notes = (0..g.notes.len()).collect();
        }
        let copy = i.events.iter().any(|e| matches!(e, egui::Event::Copy)) || (cmd && i.key_pressed(egui::Key::C));
        let cut = i.events.iter().any(|e| matches!(e, egui::Event::Cut)) || (cmd && i.key_pressed(egui::Key::X));
        if (copy || cut) && !self.sel_notes.is_empty() {
            let mut c: Vec<Note> = self.sel_notes.iter().filter_map(|k| g.notes.get(*k).cloned()).collect();
            let t0 = c.iter().map(|n| n.t).fold(f64::MAX, f64::min);
            c.iter_mut().for_each(|n| n.t -= t0);
            self.clipboard = c;
            if cut {
                self.delete_notes();
            }
        }
        let paste = i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))) || (cmd && i.key_pressed(egui::Key::V));
        if paste && !self.clipboard.is_empty() {
            let at = (self.beat - g.start).clamp(0.0, g.len);
            let at = (at / self.grid).floor() * self.grid;
            let clip = self.clipboard.clone();
            if let Some(rg) = self.region_mut() {
                let n0 = rg.notes.len();
                rg.notes.extend(clip.into_iter().map(|mut n| {
                    n.t += at;
                    n
                }).filter(|n| n.t < g.len));
                let n1 = rg.notes.len();
                self.sel_notes = (n0..n1).collect();
            }
            // La tête de lecture avance d'autant : coller plusieurs fois enchaîne
            let span = self.clipboard.iter().map(|n| n.t + n.len).fold(0.0, f64::max);
            self.beat = g.start + at + (span / self.song.beats_per_bar as f64).ceil().max(1.0) * self.song.beats_per_bar as f64;
            self.changed();
        }
        if self.sel_notes.is_empty() {
            return;
        }
        let dp: i32 = if i.key_pressed(egui::Key::ArrowUp) { if i.modifiers.shift { 12 } else { 1 } } else if i.key_pressed(egui::Key::ArrowDown) { if i.modifiers.shift { -12 } else { -1 } } else { 0 };
        let dt = if i.key_pressed(egui::Key::ArrowRight) { self.grid } else if i.key_pressed(egui::Key::ArrowLeft) { -self.grid } else { 0.0 };
        if dp != 0 || dt != 0.0 {
            let sel = self.sel_notes.clone();
            let glen = g.len;
            if let Some(rg) = self.region_mut() {
                for k in sel {
                    if let Some(n) = rg.notes.get_mut(k) {
                        n.p = (n.p as i32 + dp).clamp(0, 127) as u8;
                        n.t = (n.t + dt).clamp(0.0, (glen - n.len).max(0.0));
                    }
                }
            }
            self.changed();
        }
    }

    fn keyboard(&mut self, ui: &egui::Ui) {
        if ui.ctx().memory(|m| m.focused().is_some()) {
            return;
        }
        let i = ui.input(|i| i.clone());
        let cmd = i.modifiers.command;
        if self.focus == Focus::Roll {
            self.roll_keys(&i);
        }
        if i.key_pressed(egui::Key::Space) {
            self.toggle_play();
        }
        if i.key_pressed(egui::Key::Home) {
            self.stop();
            self.beat = 0.0;
        }
        // Les raccourcis communs à toutes les timelines
        let k = crate::timenav::keys(ui);
        if k.end {
            self.stop();
            self.beat = self.song.beats();
        }
        if k.jump != 0 && self.focus != Focus::Roll {
            // Mesure précédente / suivante
            let bpb = self.song.beats_per_bar as f64;
            self.beat = if k.jump > 0 { ((self.beat / bpb).floor() + 1.0) * bpb } else { (((self.beat - 1e-6) / bpb).ceil() - 1.0).max(0.0) * bpb };
        }
        if k.step != 0 && self.focus != Focus::Roll {
            self.beat = (self.beat + k.step as f64 * 0.25).max(0.0);
        }
        if k.fit {
            self.zoom = 0.0;
            self.ascroll = 0.0;
            self.roll_zoom = 0.0;
        }
        if k.zoom != 1.0 {
            self.zoom = (self.zoom * k.zoom).clamp(2.0, 200.0);
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
        if cmd && i.key_pressed(egui::Key::D) {
            if let Some((t, r)) = self.sel_region {
                let mut g = self.song.tracks[t].regions[r].clone();
                g.start = g.end();
                self.song.tracks[t].regions.push(g);
                self.changed();
                self.sel_region = self.song.tracks[t].regions.iter().position(|x| (x.start - self.song.tracks[t].regions[r].end()).abs() < 1e-9).map(|k| (t, k));
            }
        }
        if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) {
            match self.focus {
                Focus::Roll => self.delete_notes(),
                Focus::Arrange => {
                    if let Some((t, r)) = self.sel_region.take() {
                        self.song.tracks[t].regions.remove(r);
                        self.sel_notes.clear();
                        self.changed();
                    }
                }
            }
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let ctx = ui.ctx().clone();
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok((a, b, data)) => {
                    self.pending = None;
                    self.start_player(a, b, data);
                }
                Err(_) => ctx.request_repaint_after(std::time::Duration::from_millis(30)),
            }
        }
        if let Some((t0, a, b)) = self.playing {
            let beat = a + t0.elapsed().as_secs_f64() / self.song.spb();
            if beat >= b {
                self.stop();
                self.beat = if self.looping { 0.0 } else { b };
                if self.looping {
                    self.play();
                }
            } else {
                self.beat = beat;
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
        let full_h = ui.available_height();
        egui::Panel::top("music-transport").exact_size(44.0).frame(frame).show(ui, |ui| self.transport(ui));
        egui::Panel::right("music-inspector").resizable(true).default_size(290.0).min_size(240.0).max_size(460.0).frame(frame).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| self.inspector(ui));
        });
        egui::Panel::bottom("music-roll").resizable(true).default_size((full_h * 0.5).clamp(220.0, 480.0)).min_size(160.0).max_size((full_h * 0.8).max(200.0)).frame(egui::Frame::new().fill(p.code_bg).stroke(Stroke::new(1.0, p.line))).show(ui, |ui| self.roll(ui));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.code_bg)).show(ui, |ui| self.arrange(ui));
    }

    // ---------- Transport ----------

    fn transport(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let r = ui.max_rect().shrink2(vec2(10.0, 0.0));
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::left_to_right(egui::Align::Center)));
        flat(&mut ui);
        let ib = |ui: &mut egui::Ui, icon: &str, tip: &str, on: bool| ui.add(egui::Button::new(egui::RichText::new(icon).size(15.0)).min_size(vec2(30.0, 30.0)).selected(on)).on_hover_text(crate::i18n::t(tip)).clicked();
        if ib(&mut ui, l::SKIP_BACK, "Revenir au début (Début)", false) {
            self.stop();
            self.beat = 0.0;
        }
        let busy = self.playing.is_some() || self.pending.is_some();
        if ib(&mut ui, if busy { l::PAUSE } else { l::PLAY }, "Lire (Espace)", false) {
            self.toggle_play();
        }
        if ib(&mut ui, l::REPEAT, "Reprendre au début à la fin (L)", self.looping) {
            self.looping ^= true;
        }
        ui.add_space(6.0);
        let bpb = self.song.beats_per_bar as f64;
        let (cr, _) = ui.allocate_exact_size(vec2(96.0, 30.0), Sense::hover());
        ui.painter().rect(cr, 2.0, p.code_bg, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
        let pos = format!("{}.{}.{}", (self.beat / bpb) as u32 + 1, (self.beat % bpb) as u32 + 1, ((self.beat.fract()) * 4.0) as u32 + 1);
        ui.painter().text(cr.center(), egui::Align2::CENTER_CENTER, pos, egui::FontId::monospace(16.0), p.text);
        ui.add_space(6.0);
        let mut changed = false;
        ui.style_mut().visuals.widgets.inactive.weak_bg_fill = p.field;
        changed |= ui.add(egui::DragValue::new(&mut self.song.bpm).range(40.0..=240.0).speed(0.5).max_decimals(0).suffix(" bpm")).changed();
        egui::ComboBox::from_id_salt("music-key").selected_text(format!("{} {}", music::NOTE_NAMES[self.song.key as usize % 12], crate::i18n::t_mid(if self.song.minor { "mineur" } else { "majeur" }))).width(110.0).show_ui(&mut ui, |ui| {
            for k in 0..12u8 {
                for minor in [false, true] {
                    let on = self.song.key == k && self.song.minor == minor;
                    if ui.selectable_label(on, format!("{} {}", music::NOTE_NAMES[k as usize], crate::i18n::t_mid(if minor { "mineur" } else { "majeur" }))).clicked() {
                        self.song.key = k;
                        self.song.minor = minor;
                        changed = true;
                    }
                }
            }
        });
        egui::ComboBox::from_id_salt("music-grid").selected_text(format!("{} 1/{}", crate::i18n::t("grille"), (4.0 / self.grid).round())).width(96.0).show_ui(&mut ui, |ui| {
            for (g, label) in [(1.0, "1/4"), (0.5, "1/8"), (0.25, "1/16"), (1.0 / 3.0, "1/12 (triolets)")] {
                if ui.selectable_label((self.grid - g).abs() < 1e-6, label).clicked() {
                    self.grid = g;
                }
            }
        });
        changed |= ui.add(egui::DragValue::new(&mut self.song.bars).range(1..=512).speed(0.2).suffix(crate::i18n::t_mid(" mesures"))).changed();
        if changed {
            self.changed();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let busy = self.export_rx.is_some();
            ui.menu_button(format!("{}  {}", l::DOWNLOAD, crate::i18n::t(if busy { "Export…" } else { "Exporter" })), |ui| {
                if ui.add_enabled(!busy, egui::Button::new(crate::i18n::t("MIDI (.mid)"))).clicked() {
                    let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "morceau".into());
                    let out = crate::untitled::export_dir(&self.path).join(format!("{stem}.mid"));
                    match std::fs::write(&out, crate::midi::write(&self.song)) {
                        Ok(()) => self.say(format!("{} : {stem}.mid", crate::i18n::t("exporté"))),
                        Err(e) => self.say(e.to_string()),
                    }
                    ui.close();
                }
                ui.separator();
                for (ext, label) in [("wav", "WAV 24 bits"), ("mp3", "MP3 256 kb/s")] {
                    if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "morceau".into());
                        let out = crate::untitled::export_dir(&self.path).join(format!("{stem} - mix.{ext}"));
                        let s = self.song.clone();
                        let (tx, rx) = std::sync::mpsc::channel();
                        self.export_rx = Some(rx);
                        std::thread::spawn(move || {
                            let data = music::render(&s, 0.0, s.beats() + s.beats_per_bar as f64);
                            let _ = tx.send(write_audio(&out, &data).map(|_| out));
                        });
                        ui.close();
                    }
                }
            });
            if ui.add(egui::Button::new(egui::RichText::new(l::UPLOAD).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Ses pistes s'ajoutent au morceau")).clicked() {
                self.import_midi();
            }
            if self.pending.is_some() {
                ui.label(egui::RichText::new(crate::i18n::t("préparation…")).size(11.5).color(p.muted));
            }
        });
    }

    // ---------- Arrangement ----------

    fn arrange(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let full = ui.max_rect();
        ui.allocate_rect(full, Sense::hover());
        let ruler = Rect::from_min_max(pos2(full.left() + HEAD_W, full.top()), pos2(full.right(), full.top() + 24.0));
        let beats = self.song.beats();
        if self.zoom <= 0.0 {
            self.zoom = ((ruler.width() - 20.0) / beats as f32).max(4.0);
        }
        let zoom = self.zoom;
        let ascroll = self.ascroll;
        let x_of = move |b: f64| ruler.left() + 8.0 + (b - ascroll) as f32 * zoom;
        let b_of = move |x: f32| ((x - ruler.left() - 8.0) / zoom).max(0.0) as f64 + ascroll;
        let bpb = self.song.beats_per_bar as f64;
        let painter = ui.painter_at(full);
        painter.rect_filled(Rect::from_min_max(full.min, pos2(full.right(), ruler.bottom())), 0.0, p.panel);
        painter.hline(full.x_range(), ruler.bottom(), Stroke::new(1.0, p.line));
        let every = [1u32, 2, 4, 8, 16].into_iter().find(|k| *k as f32 * bpb as f32 * zoom > 36.0).unwrap_or(32);
        for bar in 0..=self.song.bars {
            let x = x_of(bar as f64 * bpb);
            if x > full.right() {
                break;
            }
            let strong = bar % every == 0;
            painter.vline(x, ruler.bottom()..=full.bottom(), Stroke::new(1.0, p.line.gamma_multiply(if strong { 0.6 } else { 0.25 })));
            if strong {
                painter.text(pos2(x + 4.0, ruler.center().y), egui::Align2::LEFT_CENTER, format!("{}", bar + 1), egui::FontId::monospace(10.5), p.muted);
            }
        }
        let end = x_of(beats);
        painter.rect_filled(Rect::from_min_max(pos2(end, ruler.bottom()), full.max), 0.0, Color32::from_black_alpha(40));
        let rr = ui.interact(ruler, ui.scope_id().with("music-ruler"), Sense::click_and_drag());
        if rr.drag_started() || rr.clicked() {
            self.drag = Some(Drag::Scrub);
        }
        let n = self.song.tracks.len();
        let solo = self.song.tracks.iter().any(|t| t.solo);
        let mut y = ruler.bottom();
        let mut todo: Vec<Box<dyn FnOnce(&mut Editor)>> = vec![];
        let lane_clip = Rect::from_min_max(pos2(ruler.left(), ruler.bottom()), full.max);
        let lp = ui.painter_at(lane_clip);
        for ti in 0..n {
            let tr = self.song.tracks[ti].clone();
            let row = Rect::from_min_max(pos2(full.left(), y), pos2(full.right(), y + TRACK_H));
            let head = Rect::from_min_max(row.min, pos2(ruler.left(), row.bottom()));
            let lane = Rect::from_min_max(pos2(ruler.left(), row.top()), row.max);
            painter.rect_filled(head, 0.0, if self.sel_track == ti { p.raised } else { p.panel });
            painter.vline(head.right(), head.y_range(), Stroke::new(1.0, p.line));
            painter.hline(full.x_range(), row.bottom() - 0.5, Stroke::new(1.0, p.line));
            let dim = tr.mute || (solo && !tr.solo);
            let mut hu = ui.new_child(egui::UiBuilder::new().max_rect(head.shrink2(vec2(10.0, 6.0))).layout(egui::Layout::top_down(egui::Align::Min)));
            hu.horizontal(|ui| {
                let (d, _) = ui.allocate_exact_size(vec2(8.0, 16.0), Sense::hover());
                ui.painter().circle_filled(d.center(), 4.0, col(tr.color));
                ui.label(egui::RichText::new(&tr.name).strong().size(12.5));
            });
            hu.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if ui.add(egui::Button::new(egui::RichText::new("M").size(11.0)).selected(tr.mute).min_size(vec2(20.0, 18.0))).on_hover_text(crate::i18n::t("Muet")).clicked() {
                    todo.push(Box::new(move |e: &mut Editor| {
                        e.song.tracks[ti].mute ^= true;
                        e.changed();
                    }));
                }
                if ui.add(egui::Button::new(egui::RichText::new("S").size(11.0)).selected(tr.solo).min_size(vec2(20.0, 18.0))).on_hover_text(crate::i18n::t("Solo")).clicked() {
                    todo.push(Box::new(move |e: &mut Editor| {
                        e.song.tracks[ti].solo ^= true;
                        e.changed();
                    }));
                }
                ui.label(egui::RichText::new(crate::i18n::t(tr.inst.name())).size(11.0).color(p.muted));
            });
            let hr = ui.interact(head, ui.scope_id().with(("music-head", ti)), Sense::click());
            if hr.clicked() {
                self.sel_track = ti;
            }
            hr.context_menu(|ui| {
                if ui.add(egui::Button::new(format!("{}  {}", crate::lucide::TRASH_2, crate::i18n::t("supprimer la piste")))).clicked() {
                    todo.push(Box::new(move |e: &mut Editor| {
                        e.song.tracks.remove(ti);
                        e.sel_region = None;
                        e.sel_track = 0;
                        e.changed();
                    }));
                }
            });
            for (ri, g) in tr.regions.iter().enumerate() {
                let r = Rect::from_min_max(pos2(x_of(g.start), lane.top() + 4.0), pos2(x_of(g.end()), lane.bottom() - 4.0));
                let chosen = self.sel_region == Some((ti, ri));
                let base = col(tr.color);
                lp.rect(r, 4.0, base.gamma_multiply(if dim { 0.1 } else { 0.22 }), Stroke::new(if chosen { 1.5 } else { 1.0 }, if chosen { p.text } else { base.gamma_multiply(0.6) }), egui::StrokeKind::Inside);
                let name = if g.name.is_empty() { tr.name.clone() } else { g.name.clone() };
                lp.with_clip_rect(r.intersect(lane_clip)).text(pos2(r.left() + 6.0, r.top() + 9.0), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(11.0), p.text);
                // Notes en miniature
                let inner = Rect::from_min_max(pos2(r.left(), r.top() + 18.0), pos2(r.right(), r.bottom() - 3.0));
                let (lo, hi) = g.notes.iter().fold((127u8, 0u8), |(a, b), n| (a.min(n.p), b.max(n.p)));
                let span = (hi.saturating_sub(lo)).max(6) as f32;
                for nt in g.notes.iter().filter(|n| n.t < g.len) {
                    let x0 = x_of(g.start + nt.t);
                    let x1 = x_of(g.start + (nt.t + nt.len).min(g.len)).max(x0 + 1.5);
                    let yy = inner.bottom() - (nt.p - lo) as f32 / span * inner.height();
                    lp.hline(x0..=x1, yy, Stroke::new(2.0, base.gamma_multiply(if dim { 0.4 } else { 0.95 })));
                }
            }
            let resp = ui.interact(lane.intersect(lane_clip), ui.scope_id().with(("music-lane", ti)), Sense::click_and_drag());
            let hit = |h: egui::Pos2| -> Option<(usize, u8)> {
                tr.regions.iter().enumerate().rev().find_map(|(ri, g)| {
                    let (a, b) = (x_of(g.start), x_of(g.end()));
                    if h.x < a - 3.0 || h.x > b + 3.0 {
                        return None;
                    }
                    Some((ri, if (h.x - b).abs() < 6.0 { 2 } else if (h.x - a).abs() < 6.0 { 1 } else { 0 }))
                })
            };
            if let Some(h) = resp.hover_pos() {
                if let Some((_, m)) = hit(h) {
                    ui.ctx().set_cursor_icon(if m == 0 { egui::CursorIcon::Grab } else { egui::CursorIcon::ResizeHorizontal });
                }
            }
            if resp.clicked() {
                if let Some(h) = resp.interact_pointer_pos() {
                    self.focus = Focus::Arrange;
                    self.sel_track = ti;
                    match hit(h) {
                        Some((ri, _)) => {
                            if self.sel_region != Some((ti, ri)) {
                                self.sel_notes.clear();
                                self.roll_scroll = 0.0;
                            }
                            self.sel_region = Some((ti, ri));
                        }
                        None => {
                            self.stop();
                            self.beat = b_of(h.x);
                        }
                    }
                }
            }
            if resp.double_clicked() {
                if let Some(h) = resp.interact_pointer_pos() {
                    if hit(h).is_none() {
                        let start = (b_of(h.x) / bpb).floor() * bpb;
                        todo.push(Box::new(move |e: &mut Editor| {
                            let tr = &mut e.song.tracks[ti];
                            // Une mesure, sans chevaucher la région suivante
                            let next = tr.regions.iter().map(|g| g.start).filter(|s| *s > start).fold(f64::MAX, f64::min);
                            tr.regions.push(Region { start, len: bpb.min(next - start).max(0.25), name: String::new(), notes: vec![] });
                            e.changed();
                            e.sel_region = e.song.tracks[ti].regions.iter().position(|g| g.start == start).map(|k| (ti, k));
                            e.sel_notes.clear();
                        }));
                    }
                }
            }
            if resp.drag_started() {
                if let Some(h) = resp.interact_pointer_pos() {
                    if let Some((ri, mode)) = hit(h) {
                        let g = &tr.regions[ri];
                        self.sel_region = Some((ti, ri));
                        self.sel_track = ti;
                        self.focus = Focus::Arrange;
                        self.drag = Some(Drag::Region { track: ti, region: ri, grab: b_of(h.x) - g.start, mode, start: g.start, len: g.len });
                    }
                }
            }
            y += TRACK_H;
        }
        // + piste
        let add = Rect::from_min_size(pos2(full.left() + 10.0, y + 8.0), vec2(HEAD_W - 20.0, 26.0));
        let ar = ui.interact(add, ui.scope_id().with("music-add"), Sense::click());
        painter.rect(add, 4.0, if ar.hovered() { p.raised } else { Color32::TRANSPARENT }, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
        painter.text(add.center(), egui::Align2::CENTER_CENTER, format!("{}  {}", l::PLUS, crate::i18n::t("Piste")), egui::FontId::proportional(12.0), p.text);
        let popup = egui::Popup::menu(&ar);
        popup.show(|ui| {
            for (inst, name, _) in INSTS {
                if ui.button(crate::i18n::t(name)).clicked() {
                    let t = self.song.new_track(inst);
                    self.song.tracks.push(t);
                    self.sel_track = self.song.tracks.len() - 1;
                    self.changed();
                }
            }
        });
        // Glisser une région : aimant à la mesure (Maj : au temps)
        if let (Some(d), Some(m)) = (self.drag.take(), ui.input(|i| i.pointer.interact_pos())) {
            let fine = ui.input(|i| i.modifiers.shift);
            let q = if fine { 1.0 } else { bpb };
            let snap = |b: f64| (b / q).round() * q;
            let keep = match &d {
                Drag::Scrub => {
                    self.stop();
                    self.beat = (b_of(m.x) * 4.0).round() / 4.0;
                    true
                }
                Drag::Region { track, region, grab, mode, start, len } => {
                    let b = b_of(m.x);
                    if let Some(g) = self.song.tracks.get_mut(*track).and_then(|t| t.regions.get_mut(*region)) {
                        match mode {
                            0 => g.start = snap(b - grab).max(0.0),
                            2 => g.len = (snap(b) - start).max(q.min(1.0)),
                            _ => {
                                let s = snap(b).clamp(0.0, start + len - 1.0);
                                g.len = start + len - s;
                                // Les notes restent à leur place dans le temps
                                let shift = s - start;
                                for n in &mut g.notes {
                                    n.t -= shift - (g.start - start);
                                }
                                g.start = s;
                            }
                        }
                    }
                    true
                }
                Drag::Notes { .. } | Drag::Box { .. } => true,
            };
            if keep && ui.input(|i| i.pointer.any_down()) {
                self.drag = Some(d);
            } else if !matches!(d, Drag::Scrub) {
                self.changed();
            }
        }
        for t in todo {
            t(self);
        }
        // Navigation commune à toutes les timelines (voir timenav)
        if let Some(w) = crate::timenav::wheel(ui, full, None) {
            let (mut l, mut z) = (self.ascroll, self.zoom as f64);
            if w.zoom != 1.0 {
                crate::timenav::zoom_time(&mut l, &mut z, w.zoom as f64, w.at.map(|a| (a.x - ruler.left() - 8.0) as f64).unwrap_or(0.0), 2.0, 200.0);
            }
            self.ascroll = (l + w.time_px as f64 / z).clamp(0.0, beats);
            self.zoom = z as f32;
        }
        let x = x_of(self.beat);
        painter.vline(x, ruler.top()..=full.bottom(), Stroke::new(1.0, p.accent));
        painter.add(egui::Shape::convex_polygon(vec![pos2(x - 5.0, ruler.top()), pos2(x + 5.0, ruler.top()), pos2(x, ruler.top() + 7.0)], p.accent, Stroke::NONE));
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

    // ---------- Rouleau de piano ----------

    fn roll(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let full = ui.max_rect();
        ui.allocate_rect(full, Sense::hover());
        let painter = ui.painter_at(full);
        let Some((ti, ri)) = self.sel_region else {
            painter.text(full.center(), egui::Align2::CENTER_CENTER, crate::i18n::t("Choisis une région dans l'arrangement (double-clic dans une piste pour en créer une)."), egui::FontId::proportional(12.5), p.muted);
            return;
        };
        let Some(tr) = self.song.tracks.get(ti).cloned() else { return };
        let Some(g) = tr.regions.get(ri).cloned() else { return };
        let drums = tr.inst == Inst::Drums;
        // Une autre région : le rouleau se recadre sur ses notes
        if self.roll_for != Some((ti, ri)) {
            self.roll_for = Some((ti, ri));
            self.roll_zoom = 0.0;
            self.roll_scroll = 0.0;
            self.roll_top = g.notes.iter().map(|n| n.p as f32 + 4.0).fold(f32::MIN, f32::max).max(if g.notes.is_empty() { 76.0 } else { 0.0 }).clamp(36.0, 108.0);
        }
        let head = Rect::from_min_max(full.min, pos2(full.right(), full.top() + 26.0));
        painter.rect_filled(head, 0.0, p.panel);
        painter.hline(full.x_range(), head.bottom(), Stroke::new(1.0, p.line));
        painter.circle_filled(pos2(head.left() + 14.0, head.center().y), 4.0, col(tr.color));
        let title = format!("{} · {}", tr.name, if g.name.is_empty() { format!("{} {}", crate::i18n::t("mesure"), (g.start / self.song.beats_per_bar as f64) as u32 + 1) } else { g.name.clone() });
        painter.text(pos2(head.left() + 26.0, head.center().y), egui::Align2::LEFT_CENTER, title, egui::FontId::new(12.5, crate::theme::bold()), p.text);
        painter.text(pos2(head.right() - 12.0, head.center().y), egui::Align2::RIGHT_CENTER, crate::i18n::t("clic : ajouter · glisser : déplacer · bord : allonger · clic droit : retirer"), egui::FontId::proportional(11.0), p.muted);
        let grid_r = Rect::from_min_max(pos2(full.left() + KEYS_W, head.bottom()), full.max);
        if self.roll_zoom <= 0.0 {
            self.roll_zoom = ((grid_r.width() - 16.0) / g.len.max(1.0) as f32).clamp(12.0, 200.0);
        }
        let (zoom, scroll) = (self.roll_zoom, self.roll_scroll);
        let x_of = move |b: f64| grid_r.left() + 8.0 + ((b - scroll) as f32) * zoom;
        let b_of = move |x: f32| (x - grid_r.left() - 8.0) as f64 / zoom as f64 + scroll;
        // Rangées : les pièces de la batterie, ou les hauteurs (aiguës en haut)
        let rows: Vec<u8> = if drums { KIT.iter().map(|k| k.0).collect() } else { (24..=108u8).rev().collect() };
        let row_h = if drums { ((grid_r.height()) / rows.len() as f32).clamp(16.0, 30.0) } else { 14.0 };
        let first = if drums { 0 } else { rows.iter().position(|r| *r as f32 <= self.roll_top).unwrap_or(0) };
        let row_of = |pitch: u8| rows.iter().position(|r| *r == pitch).map(|i| i as i64 - first as i64);
        let y_of = move |i: i64| grid_r.top() + i as f32 * row_h;
        let gp = ui.painter_at(grid_r);
        let kp = ui.painter_at(Rect::from_min_max(pos2(full.left(), grid_r.top()), pos2(grid_r.left(), full.bottom())));
        for (i, pitch) in rows.iter().enumerate().skip(first) {
            let y = y_of(i as i64 - first as i64);
            if y > full.bottom() {
                break;
            }
            let rr = Rect::from_min_max(pos2(grid_r.left(), y), pos2(grid_r.right(), y + row_h));
            let black = !drums && matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
            let in_scale = drums || self.song.in_scale(*pitch);
            gp.rect_filled(rr, 0.0, if !in_scale { Color32::from_black_alpha(50) } else if black { Color32::from_black_alpha(22) } else { Color32::TRANSPARENT });
            gp.hline(rr.x_range(), rr.bottom(), Stroke::new(1.0, p.line.gamma_multiply(if pitch % 12 == 0 { 0.7 } else { 0.25 })));
            // Clavier
            let kr = Rect::from_min_max(pos2(full.left(), y), pos2(grid_r.left() - 1.0, y + row_h));
            if drums {
                kp.rect_filled(kr, 0.0, p.panel);
                let name = KIT.iter().find(|k| k.0 == *pitch).map(|k| k.1).unwrap_or("");
                kp.text(pos2(kr.left() + 6.0, kr.center().y), egui::Align2::LEFT_CENTER, crate::i18n::t(name), egui::FontId::proportional(11.0), p.text);
            } else {
                // Touches noires plus courtes, comme un vrai clavier
                kp.rect_filled(kr, 0.0, Color32::from_gray(212));
                if black {
                    kp.rect_filled(Rect::from_min_max(kr.min, pos2(kr.left() + kr.width() * 0.62, kr.bottom())), 1.0, Color32::from_gray(38));
                }
                if pitch % 12 == 0 {
                    kp.text(pos2(kr.right() - 4.0, kr.center().y), egui::Align2::RIGHT_CENTER, music::note_name(*pitch), egui::FontId::proportional(10.0), Color32::from_gray(60));
                }
                kp.hline(kr.x_range(), kr.bottom(), Stroke::new(1.0, Color32::from_gray(120)));
            }
            let kresp = ui.interact(kr.intersect(full), ui.scope_id().with(("music-key", *pitch)), Sense::click());
            if kresp.clicked() {
                self.preview_note(ti, *pitch);
            }
        }
        // Grille de temps
        let bpb = self.song.beats_per_bar as f64;
        let mut b = (scroll / self.grid).floor() * self.grid;
        while x_of(b) < grid_r.right() {
            let x = x_of(b);
            let strength = if (b / bpb).fract().abs() < 1e-6 { 0.8 } else if b.fract().abs() < 1e-6 { 0.4 } else { 0.15 };
            gp.vline(x, grid_r.y_range(), Stroke::new(1.0, p.line.gamma_multiply(strength)));
            b += self.grid;
        }
        // Fin de la région
        let xe = x_of(g.len);
        gp.rect_filled(Rect::from_min_max(pos2(xe, grid_r.top()), grid_r.max), 0.0, Color32::from_black_alpha(70));
        // Notes
        let base = col(tr.color);
        for (k, n) in g.notes.iter().enumerate() {
            let Some(i) = row_of(n.p) else { continue };
            let y = y_of(i);
            let r = Rect::from_min_max(pos2(x_of(n.t), y + 1.0), pos2(x_of(n.t + n.len).max(x_of(n.t) + 4.0), y + row_h - 1.0));
            let chosen = self.sel_notes.contains(&k);
            gp.rect(r, 3.0, base.gamma_multiply(0.45 + 0.55 * n.v), Stroke::new(1.0, if chosen { p.text } else { base.gamma_multiply(0.4) }), egui::StrokeKind::Inside);
        }
        // Tête de lecture
        let pb = self.beat - g.start;
        if pb >= 0.0 && pb <= g.len {
            gp.vline(x_of(pb), grid_r.y_range(), Stroke::new(1.0, p.accent));
        }
        // Souris
        let resp = ui.interact(grid_r, ui.scope_id().with("music-grid"), Sense::click_and_drag());
        let row_at = move |y: f32| ((y - grid_r.top()) / row_h).floor() as i64 + first as i64;
        let pitch_at = |y: f32| -> Option<u8> { rows.get(row_at(y).max(0) as usize).copied() };
        let note_at = |pos: egui::Pos2| -> Option<(usize, bool)> {
            let pt = pitch_at(pos.y)?;
            g.notes.iter().enumerate().rev().find_map(|(k, n)| {
                if n.p != pt {
                    return None;
                }
                let (a, b) = (x_of(n.t), x_of(n.t + n.len).max(x_of(n.t) + 4.0));
                (pos.x >= a - 2.0 && pos.x <= b + 2.0).then_some((k, (pos.x - b).abs() < 6.0))
            })
        };
        if let Some(h) = resp.hover_pos() {
            if let Some((_, edge)) = note_at(h) {
                ui.ctx().set_cursor_icon(if edge { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::Grab });
            }
        }
        let gstep = self.grid;
        let snap = move |b: f64| (b / gstep).floor() * gstep;
        let shift = ui.input(|i| i.modifiers.shift);
        if resp.secondary_clicked() {
            if let Some(h) = resp.interact_pointer_pos() {
                if let Some((k, _)) = note_at(h) {
                    if !self.sel_notes.contains(&k) {
                        self.sel_notes = vec![k];
                    }
                    self.delete_notes();
                }
            }
        }
        if resp.clicked() {
            self.focus = Focus::Roll;
            if let Some(h) = resp.interact_pointer_pos() {
                match note_at(h) {
                    Some((k, _)) => {
                        if shift {
                            if let Some(i) = self.sel_notes.iter().position(|x| *x == k) {
                                self.sel_notes.remove(i);
                            } else {
                                self.sel_notes.push(k);
                            }
                        } else {
                            self.sel_notes = vec![k];
                        }
                        self.last_len = g.notes[k].len;
                        self.preview_note(ti, g.notes[k].p);
                    }
                    None if shift => self.sel_notes.clear(),
                    None => {
                        if let Some(pt) = pitch_at(h.y) {
                            let t = snap(b_of(h.x));
                            if t >= 0.0 && t < g.len {
                                let len = if drums { self.grid } else { self.last_len.min(g.len - t) };
                                if let Some(rg) = self.region_mut() {
                                    rg.notes.push(Note { t, len, p: pt, v: 0.8 });
                                    let k = rg.notes.len() - 1;
                                    self.sel_notes = vec![k];
                                }
                                self.preview_note(ti, pt);
                                self.changed();
                            }
                        }
                    }
                }
            }
        }
        if resp.drag_started() {
            self.focus = Focus::Roll;
            if let Some(h) = resp.interact_pointer_pos() {
                match note_at(h) {
                    Some((k, edge)) => {
                        if !self.sel_notes.contains(&k) {
                            self.sel_notes = vec![k];
                        }
                        let orig = self.sel_notes.iter().filter_map(|i| g.notes.get(*i).map(|n| (*i, n.t, n.p, n.len))).collect();
                        self.drag = Some(Drag::Notes { grab: b_of(h.x), row0: row_at(h.y), resize: edge, orig });
                    }
                    None => self.drag = Some(Drag::Box { a: (b_of(h.x), row_at(h.y)), additive: shift }),
                }
            }
        }
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let released = !ui.input(|i| i.pointer.any_down());
        match self.drag.take() {
            Some(Drag::Notes { grab, row0, resize, orig }) => {
                if let Some(m) = pointer {
                    let db = ((b_of(m.x) - grab) / gstep).round() * gstep;
                    let dr = row_at(m.y) - row0;
                    let glen = g.len;
                    let mut heard = None;
                    if let Some(rg) = self.region_mut() {
                        // Le groupe reste entier dans la région
                        let lo = orig.iter().map(|o| o.1).fold(f64::MAX, f64::min);
                        let hi = orig.iter().map(|o| o.1 + o.3).fold(f64::MIN, f64::max);
                        let db = db.clamp(-lo, (glen - hi).max(-lo));
                        for (i, t0, p0, len0) in &orig {
                            let Some(n) = rg.notes.get_mut(*i) else { continue };
                            if resize {
                                n.len = (len0 + db).max(gstep);
                            } else {
                                n.t = t0 + db;
                                let r0 = rows.iter().position(|r| r == p0).unwrap_or(0) as i64;
                                let np = rows[(r0 + dr).clamp(0, rows.len() as i64 - 1) as usize];
                                if np != n.p && orig.len() == 1 {
                                    heard = Some(np);
                                }
                                n.p = np;
                            }
                        }
                    }
                    if let Some(p) = heard {
                        self.preview_note(ti, p);
                    }
                }
                if released {
                    if let Some(n) = orig.first().and_then(|o| self.region().and_then(|g| g.notes.get(o.0))) {
                        self.last_len = n.len;
                    }
                    self.changed();
                } else {
                    self.drag = Some(Drag::Notes { grab, row0, resize, orig });
                }
            }
            Some(Drag::Box { a, additive }) => {
                if let Some(m) = pointer {
                    let (b0, b1) = (a.0.min(b_of(m.x)), a.0.max(b_of(m.x)));
                    let (r0, r1) = (a.1.min(row_at(m.y)), a.1.max(row_at(m.y)));
                    let y0 = grid_r.top() + (r0 - first as i64) as f32 * row_h;
                    let y1 = grid_r.top() + (r1 + 1 - first as i64) as f32 * row_h;
                    let br = Rect::from_min_max(pos2(x_of(b0), y0), pos2(x_of(b1), y1));
                    gp.rect(br, 2.0, p.accent.gamma_multiply(0.12), Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
                    if released {
                        let hit: Vec<usize> = g
                            .notes
                            .iter()
                            .enumerate()
                            .filter(|(_, n)| {
                                let r = rows.iter().position(|x| *x == n.p).map(|i| i as i64).unwrap_or(-1);
                                r >= r0 && r <= r1 && n.t < b1 && n.t + n.len > b0
                            })
                            .map(|(k, _)| k)
                            .collect();
                        if additive {
                            for k in hit {
                                if !self.sel_notes.contains(&k) {
                                    self.sel_notes.push(k);
                                }
                            }
                        } else {
                            self.sel_notes = hit;
                        }
                    }
                }
                if !released {
                    self.drag = Some(Drag::Box { a, additive });
                }
            }
            other => self.drag = other,
        }
        // Navigation commune à toutes les timelines : molette temps, Maj+molette hauteurs (notes), Ctrl zoom
        if let Some(w) = crate::timenav::wheel(ui, full, None) {
            let (mut l, mut z) = (self.roll_scroll, self.roll_zoom as f64);
            if w.zoom != 1.0 {
                crate::timenav::zoom_time(&mut l, &mut z, w.zoom as f64, w.at.map(|a| (a.x - grid_r.left() - 8.0) as f64).unwrap_or(0.0), 6.0, 400.0);
            }
            self.roll_scroll = (l + w.time_px as f64 / z).clamp(0.0, g.len);
            self.roll_zoom = z as f32;
            if !drums && w.y_px != 0.0 {
                self.roll_top = (self.roll_top - w.y_px / row_h).clamp(36.0, 108.0);
            }
        }
    }

    // ---------- Inspecteur ----------

    fn inspector(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        if let Some(k) = crate::montage_ui::tab_chips(ui, self.insp_tab, &[&crate::i18n::t("Piste"), &crate::i18n::t("Idées")]) {
            self.insp_tab = k;
        }
        let ti = self.sel_track.min(self.song.tracks.len().saturating_sub(1));
        if self.song.tracks.is_empty() {
            return;
        }
        egui::Frame::new().inner_margin(12).show(ui, |ui| {
            if self.insp_tab == 1 {
                self.ideas(ui, ti);
                return;
            }
            let mut changed = false;
            {
                let tr = &mut self.song.tracks[ti];
                changed |= ui.add(egui::TextEdit::singleline(&mut tr.name).font(egui::FontId::new(15.0, crate::theme::bold())).frame(egui::Frame::NONE)).changed();
                section(ui, "Instrument");
                for (inst, name, sub) in INSTS {
                    let on = tr.inst == inst;
                    let b = egui::Button::new(egui::RichText::new(format!("{}\n{}", crate::i18n::t(name), crate::i18n::t(sub))).size(12.0)).min_size(vec2(ui.available_width(), 36.0)).selected(on);
                    if ui.add(b).clicked() && !on {
                        tr.inst = inst;
                        changed = true;
                    }
                }
                section(ui, "Mixage");
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::DragValue::new(&mut tr.volume).speed(0.1).range(-40.0..=12.0).max_decimals(1).prefix(crate::i18n::t("volume ")).suffix(" dB")).changed();
                    changed |= ui.add(egui::DragValue::new(&mut tr.pan).speed(0.01).range(-1.0..=1.0).max_decimals(2).prefix(crate::i18n::t("pan "))).changed();
                });
                changed |= ui.color_edit_button_srgb(&mut tr.color).changed();
            }
            if let Some((rt, rr)) = self.sel_region.filter(|(t, _)| *t == ti) {
                section(ui, "Région");
                let g = &mut self.song.tracks[rt].regions[rr];
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(crate::i18n::t("Nom")).color(p.muted));
                    changed |= ui.text_edit_singleline(&mut g.name).changed();
                });
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::t("−1 octave")).clicked() {
                        g.notes.iter_mut().for_each(|n| n.p = n.p.saturating_sub(12));
                        changed = true;
                    }
                    if ui.button(crate::i18n::t("+1 octave")).clicked() {
                        g.notes.iter_mut().for_each(|n| n.p = (n.p + 12).min(127));
                        changed = true;
                    }
                    if ui.button(crate::i18n::t("Quantifier")).on_hover_text(crate::i18n::t("Cale les notes sur la grille")).clicked() {
                        let q = self.grid;
                        g.notes.iter_mut().for_each(|n| {
                            n.t = (n.t / q).round() * q;
                            n.len = (n.len / q).round().max(1.0) * q;
                        });
                        changed = true;
                    }
                });
                if ui.button(crate::i18n::t("Vider la région")).clicked() {
                    g.notes.clear();
                    self.sel_notes.clear();
                    changed = true;
                }
                let sel: Vec<usize> = self.sel_notes.iter().copied().filter(|k| *k < self.song.tracks[rt].regions[rr].notes.len()).collect();
                if let Some(&k0) = sel.first() {
                    let notes = &mut self.song.tracks[rt].regions[rr].notes;
                    section(ui, if sel.len() == 1 { "Note" } else { "Notes" });
                    ui.label(if sel.len() == 1 { music::note_name(notes[k0].p) } else { format!("{} {}", sel.len(), crate::i18n::t_mid("notes choisies")) });
                    let mut v = notes[k0].v;
                    if ui.add(egui::Slider::new(&mut v, 0.05..=1.0).text(crate::i18n::t("force"))).changed() {
                        for k in &sel {
                            notes[*k].v = v;
                        }
                        changed = true;
                    }
                }
            }
            if changed {
                self.changed();
            }
        });
    }

    fn ideas(&mut self, ui: &mut egui::Ui, ti: usize) {
        let p = crate::theme::pal();
        let inst = self.song.tracks[ti].inst;
        ui.label(egui::RichText::new(crate::i18n::t("Des points de départ calculés sur la tonalité du morceau. Ils remplissent la région choisie, ou une nouvelle de 4 mesures à la tête de lecture.")).size(11.5).color(p.muted));
        let bars = |e: &Editor, t: usize, r: usize| ((e.song.tracks[t].regions[r].len / e.song.beats_per_bar as f64).ceil() as u32).max(1);
        if inst == Inst::Drums {
            section(ui, "Rythmes");
            for (k, name) in music::DRUM_STYLES.iter().enumerate() {
                if ui.add(egui::Button::new(crate::i18n::t(name)).min_size(vec2(ui.available_width(), 28.0))).clicked() {
                    let (t, r) = self.idea_region(ti);
                    let n = bars(self, t, r);
                    self.song.tracks[t].regions[r].notes = music::drum_pattern(k, n);
                    self.changed();
                }
            }
            return;
        }
        section(ui, "Accords");
        for (k, (name, _)) in music::PROGRESSIONS.iter().enumerate() {
            if ui.add(egui::Button::new(*name).min_size(vec2(ui.available_width(), 28.0))).clicked() {
                let (t, r) = self.idea_region(ti);
                let n = bars(self, t, r);
                let notes = if inst == Inst::Bass { music::bassline(&self.song, n, k) } else { music::chords(&self.song, n, k) };
                self.song.tracks[t].regions[r].notes = notes;
                self.roll_for = None;
                self.changed();
            }
        }
        section(ui, "Mélodie");
        if ui.add(egui::Button::new(format!("{}  {}", l::SPARKLES, crate::i18n::t("Une idée de mélodie"))).min_size(vec2(ui.available_width(), 28.0))).on_hover_text(crate::i18n::t("Chaque clic en propose une autre")).clicked() {
            self.seed = self.seed.wrapping_add(1);
            let (t, r) = self.idea_region(ti);
            let n = bars(self, t, r);
            let mut notes = music::melody(&self.song, n, self.seed);
            if inst == Inst::Bass {
                notes.iter_mut().for_each(|x| x.p -= 24);
            }
            self.song.tracks[t].regions[r].notes = notes;
            self.roll_for = None;
            self.changed();
        }
    }
}

fn write_audio(out: &std::path::Path, data: &[f32]) -> Result<(), String> {
    let wav = crate::audio::wav_bytes(data);
    if out.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")) {
        return std::fs::write(out, wav).map_err(|e| e.to_string());
    }
    use std::io::Write;
    let ff = crate::ffmpeg::find("ffmpeg").ok_or(crate::i18n::t("ffmpeg n'est pas installé."))?;
    let mut child = crate::ffmpeg::command(ff).args(["-v", "error", "-y", "-f", "wav", "-i", "-", "-b:a", "256k"]).arg(out).stdin(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    child.stdin.take().ok_or("ffmpeg")?.write_all(&wav).map_err(|e| e.to_string())?;
    let o = child.wait_with_output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("ffmpeg").to_string());
    }
    Ok(())
}
