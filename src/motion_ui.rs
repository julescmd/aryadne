//! L'éditeur de l'étape Motion : projet à gauche, la composition au centre avec ses poignées,
//! l'inspecteur à droite, et la timeline des calques et de leurs clés en bas.

use crate::lucide as l;
use crate::motion::{self, Compo, Kind, Layer, Prop, PROPS};
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, Sense, Stroke};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Rect,
    Ellipse,
    Text,
    /// Tracé à main levée
    Pen,
}

/// Ce qu'on tire dans la composition
enum ViewDrag {
    Move { start: Pos2, from: [f32; 2] },
    Scale { from: [f32; 2], d0: f32 },
    Rotate { from: f32, a0: f32 },
    Draw { a: [f32; 2], b: [f32; 2] },
    /// Tracé à main levée : les points du geste (composition)
    Pen(Vec<[f32; 2]>),
    /// Un point d'un tracé, qu'on déplace
    PathPt(usize),
}

/// Simplifie un geste (Ramer-Douglas-Peucker) : garde les points qui s'écartent de plus de `eps`
fn simplify(pts: &[[f32; 2]], eps: f32) -> Vec<[f32; 2]> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy).max(1e-6);
    let (mut far, mut at) = (0.0, 0);
    for (i, p) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
        let d = ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / len;
        if d > far {
            (far, at) = (d, i);
        }
    }
    if far <= eps {
        return vec![a, b];
    }
    let mut left = simplify(&pts[..=at], eps);
    left.pop();
    left.extend(simplify(&pts[at..], eps));
    left
}

/// Ce qu'on tire dans la timeline
enum TlDrag {
    Scrub,
    /// Toutes les clés d'un calque à cet instant
    Keys { layer: usize, t0: f32, prop: Option<Prop> },
    Bar { layer: usize, grab: f32, part: u8, start: f32, end: f32 },
}

pub struct Editor {
    pub path: PathBuf,
    pub compo: Compo,
    /// Compositions du coffre, données par la fenêtre à chaque image (panneau Projet)
    pub comps: Vec<String>,
    /// Composition à ouvrir (clic dans le panneau Projet)
    pub open_request: Option<String>,
    root: String,
    error: Option<String>,
    message: Option<String>,
    time: f32,
    playing: Option<(Instant, f32)>,
    sel: Option<usize>,
    /// Barre d'outils d'animation (tweener, décaler, poses…)
    anim_tools: bool,
    /// Pelure d'oignon : le calque choisi aux images d'avant (rouge) et d'après (vert)
    onion: bool,
    /// Éditeur de courbes (fenêtre, comme le Graph Editor de Maya)
    curves_open: bool,
    curve_ed: crate::curves::Editor,
    tween_w: f32,
    pose_clip: Option<crate::animtools::motion::Pose>,
    /// Clé choisie : calque, propriété, instant
    sel_key: Option<(usize, Prop, f32)>,
    tool: Tool,
    cache: motion::Cache,
    tex: Option<egui::TextureHandle>,
    shown: Option<(f32, u64, u32)>,
    rev: u64,
    saved: Compo,
    history: Vec<Compo>,
    redo: Vec<Compo>,
    last_change: Option<Instant>,
    view_drag: Option<ViewDrag>,
    tl_drag: Option<TlDrag>,
    insp_tab: u8,
    /// Zoom de la vue : None = ajusté à la place
    zoom: Option<f32>,
    export_rx: Option<Receiver<Result<PathBuf, String>>>,
    export_prog: Arc<AtomicU32>,
    /// Propriété montrée dans la courbe, à droite de la timeline
    curve: Prop,
    tl_scroll: f32,
    /// Clé sous le clic droit (calque, propriété, instant)
    ctx_key: Option<(usize, Prop, f32)>,
    /// Message affiché et depuis quand (il s'efface après quelques secondes)
    msg_seen: Option<(String, Instant)>,
    /// Son de la composition analysé (chemin, résultat attendu d'un fil)
    wave_job: Option<(String, Arc<std::sync::Mutex<Option<Result<Arc<motion::Wave>, String>>>>)>,
    /// ffplay qui joue la musique pendant la lecture
    sound: Option<std::process::Child>,
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.stop_sound();
    }
}

fn tc(t: f32, fps: f32) -> String {
    let f = (t * fps).round() as i64;
    let fps_i = fps.round().max(1.0) as i64;
    format!("{}:{:02}:{:02}", f / fps_i / 60, (f / fps_i) % 60, f % fps_i)
}

/// Couleur d'un calque dans la timeline, selon son genre
fn layer_color(k: &Kind) -> Color32 {
    let p = crate::theme::pal();
    match k {
        Kind::Text { .. } => crate::theme::ws_color("2d").unwrap_or(p.accent),
        Kind::Rect { .. } | Kind::Ellipse { .. } => p.accent,
        Kind::Image { .. } => crate::theme::ws_color("3d").unwrap_or(p.info),
        Kind::Compo { .. } => crate::theme::ws_color("motion").unwrap_or(p.accent),
        Kind::Path { .. } => p.accent,
    }
}

fn layer_icon(k: &Kind) -> &'static str {
    match k {
        Kind::Text { .. } => l::TYPE,
        Kind::Rect { .. } => l::SQUARE,
        Kind::Ellipse { .. } => l::CIRCLE,
        Kind::Image { .. } => l::IMAGE,
        Kind::Compo { .. } => l::FILM,
        Kind::Path { .. } => l::PEN_TOOL,
    }
}

fn diamond(painter: &egui::Painter, c: Pos2, r: f32, fill: Option<Color32>, stroke: Color32) {
    let pts = vec![pos2(c.x, c.y - r), pos2(c.x + r, c.y), pos2(c.x, c.y + r), pos2(c.x - r, c.y)];
    painter.add(egui::Shape::convex_polygon(pts, fill.unwrap_or(Color32::TRANSPARENT), Stroke::new(1.0, stroke)));
}

fn flat(ui: &mut egui::Ui) {
    let p = crate::theme::pal();
    let v = &mut ui.style_mut().visuals.widgets;
    v.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.inactive.bg_stroke = Stroke::NONE;
    v.hovered.weak_bg_fill = p.raised;
    v.hovered.bg_stroke = Stroke::NONE;
    v.open.weak_bg_fill = p.raised;
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(crate::i18n::t(title)).size(11.5).color(crate::theme::pal().muted));
    ui.add_space(2.0);
}

impl Editor {
    pub fn open(path: PathBuf, root: &str) -> Editor {
        let (compo, error) = match Compo::read(&path) {
            Ok(c) => (c, None),
            Err(e) => (Compo::default(), Some(e)),
        };
        Editor {
            path,
            saved: compo.clone(),
            sel: if compo.layers.is_empty() { None } else { Some(0) },
            anim_tools: false,
            onion: false,
            curves_open: false,
            curve_ed: Default::default(),
            tween_w: 0.5,
            pose_clip: None,
            compo,
            comps: vec![],
            open_request: None,
            root: root.to_string(),
            error,
            message: None,
            time: 2.0,
            playing: None,
            sel_key: None,
            tool: Tool::Select,
            cache: motion::Cache::new(std::path::Path::new(root)),
            tex: None,
            shown: None,
            rev: 0,
            history: vec![],
            redo: vec![],
            last_change: None,
            view_drag: None,
            tl_drag: None,
            insp_tab: 0,
            zoom: None,
            export_rx: None,
            export_prog: Arc::new(AtomicU32::new(0)),
            curve: Prop::Pos,
            tl_scroll: 0.0,
            ctx_key: None,
            msg_seen: None,
            wave_job: None,
            sound: None,
        }
    }

    /// Après une modification : historique (les changements rapprochés d'un même geste n'en font qu'un) et enregistrement
    fn changed(&mut self) {
        if self.saved == self.compo {
            return;
        }
        for l in &mut self.compo.layers {
            for (p, _) in PROPS {
                l.track_mut(p).sort();
            }
        }
        let recent = self.last_change.is_some_and(|t| t.elapsed().as_secs_f32() < 0.6);
        let prev = std::mem::replace(&mut self.saved, self.compo.clone());
        if !recent || self.history.is_empty() {
            self.history.push(prev);
            if self.history.len() > 200 {
                self.history.remove(0);
            }
        }
        self.redo.clear();
        self.last_change = Some(Instant::now());
        self.rev += 1;
        if self.error.is_none() {
            if let Err(e) = self.compo.write(&self.path) {
                self.message = Some(e);
            }
        }
    }

    fn undo(&mut self, forward: bool) {
        let (from, to) = if forward { (&mut self.redo, &mut self.history) } else { (&mut self.history, &mut self.redo) };
        if let Some(c) = from.pop() {
            to.push(std::mem::replace(&mut self.compo, c));
            self.saved = self.compo.clone();
            self.last_change = None;
            self.sel = self.sel.filter(|&i| i < self.compo.layers.len());
            self.sel_key = None;
            self.rev += 1;
            let _ = self.compo.write(&self.path);
        }
    }

    fn add_layer(&mut self, kind: Kind, at: Option<[f32; 2]>) {
        let base = match &kind {
            Kind::Text { .. } => "Texte",
            Kind::Rect { .. } => "Rectangle",
            Kind::Ellipse { .. } => "Ellipse",
            Kind::Image { .. } => "Image",
            Kind::Compo { path, .. } => path.rsplit('/').next().unwrap_or("Composition").trim_end_matches(".arymotion"),
            Kind::Path { .. } => "Tracé",
        };
        let name = self.compo.fresh_name(&crate::i18n::t(base));
        let mut layer = Layer::new(&name, kind, &self.compo);
        if let Some(p) = at {
            layer.pos.v = p;
        }
        self.compo.layers.insert(0, layer);
        self.sel = Some(0);
        self.sel_key = None;
        self.changed();
    }

    fn add_image(&mut self) {
        let Some(f) = rfd::FileDialog::new().add_filter("image", &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tga"]).pick_file() else { return };
        match self.vault_path(&f) {
            Ok(rel) => self.add_layer(Kind::Image { path: rel }, None),
            Err(e) => self.message = Some(e),
        }
    }

    /// Le chemin d'un fichier dans le coffre ; un fichier d'ailleurs est copié à côté de la composition
    fn vault_path(&self, f: &std::path::Path) -> Result<String, String> {
        if let Some(rel) = crate::vault::inside(&self.root, f) {
            return Ok(rel);
        }
        let here = crate::vault::inside(&self.root, &self.path).map(|r| crate::vault::parent(&r)).unwrap_or_default();
        crate::vault::import(&self.root, &here, f, false)
    }

    fn keyboard(&mut self, ui: &egui::Ui) {
        if ui.ctx().memory(|m| m.focused().is_some()) {
            return;
        }
        let (mut del, mut play, mut undo, mut redo, mut step, mut home, mut key) = (false, false, false, false, 0i32, false, false);
        let mut tool = None;
        ui.input(|i| {
            del = i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace);
            play = i.key_pressed(egui::Key::Space);
            undo = i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::Z);
            redo = i.modifiers.command && (i.key_pressed(egui::Key::Y) || (i.modifiers.shift && i.key_pressed(egui::Key::Z)));
            home = i.key_pressed(egui::Key::Home);
            if i.key_pressed(egui::Key::ArrowRight) {
                step = if i.modifiers.shift { 10 } else { 1 };
            }
            if i.key_pressed(egui::Key::ArrowLeft) {
                step = if i.modifiers.shift { -10 } else { -1 };
            }
            if !i.modifiers.command {
                key = i.key_pressed(egui::Key::K);
                for (k, t) in [(egui::Key::V, Tool::Select), (egui::Key::Q, Tool::Rect), (egui::Key::E, Tool::Ellipse), (egui::Key::T, Tool::Text), (egui::Key::P, Tool::Pen)] {
                    if i.key_pressed(k) {
                        tool = Some(t);
                    }
                }
            }
        });
        if let Some(t) = tool {
            self.tool = t;
        }
        if undo || redo {
            self.undo(redo);
        }
        if play {
            self.toggle_play();
        }
        if home {
            self.time = 0.0;
            self.playing = None;
        }
        if step != 0 {
            self.playing = None;
            self.time = (self.time + step as f32 / self.compo.fps).clamp(0.0, self.compo.duration);
        }
        // K : une clé de position à la tête de lecture
        if key {
            if let Some(i) = self.sel {
                let t = self.time;
                self.compo.layers[i].pos.toggle_key(t);
                self.changed();
            }
        }
        if del {
            if let Some((li, p, t)) = self.sel_key.take() {
                if let Some(tr) = self.compo.layers.get_mut(li).map(|l| l.track_mut(p)) {
                    if let Some(k) = tr.key_at(t) {
                        tr.toggle_key(tr.keys[k].t);
                    }
                }
                self.changed();
            } else if let Some(i) = self.sel.take() {
                self.compo.layers.remove(i);
                self.changed();
            }
        }
    }

    fn toggle_play(&mut self) {
        if self.playing.is_some() {
            self.playing = None;
            self.stop_sound();
        } else {
            if self.time >= self.compo.duration - 1e-3 {
                self.time = 0.0;
            }
            self.playing = Some((Instant::now(), self.time));
            self.start_sound();
        }
    }

    fn stop_sound(&mut self) {
        if let Some(mut c) = self.sound.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    /// La musique à partir de la tête de lecture (ffplay, sans fenêtre)
    fn start_sound(&mut self) {
        self.stop_sound();
        let Some(m) = self.compo.music.as_ref().and_then(|m| crate::vault::safe(&self.root, m).ok()) else { return };
        let Some(fp) = crate::ffmpeg::find("ffplay") else { return };
        let mut cmd = crate::ffmpeg::command(fp);
        cmd.args(["-nodisp", "-autoexit", "-loglevel", "quiet", "-ss", &format!("{:.3}", self.time), "-t", &format!("{:.3}", (self.compo.duration - self.time).max(0.0))]).arg(m);
        cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        self.sound = cmd.spawn().ok();
    }

    /// L'enveloppe et les temps forts de la musique, analysés en arrière-plan la première fois
    fn wave(&mut self) -> Option<Arc<motion::Wave>> {
        let m = self.compo.music.clone()?;
        if self.wave_job.as_ref().is_none_or(|(p, _)| *p != m) {
            let slot = Arc::new(std::sync::Mutex::new(None));
            let s2 = slot.clone();
            let path = crate::vault::safe(&self.root, &m).ok();
            std::thread::spawn(move || {
                let r = path.ok_or_else(|| "?".to_string()).and_then(|p| motion::analyse(&p)).map(Arc::new);
                *s2.lock().unwrap() = Some(r);
            });
            self.wave_job = Some((m, slot));
        }
        let (_, slot) = self.wave_job.as_ref()?;
        let g = slot.lock().ok()?;
        g.as_ref().and_then(|r| r.as_ref().ok()).cloned()
    }

    fn export(&mut self) {
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "composition".into());
        let out = crate::untitled::export_dir(&self.path).join(format!("{stem}.mp4"));
        let c = self.compo.clone();
        let root = PathBuf::from(&self.root);
        let prog = self.export_prog.clone();
        prog.store(0, Ordering::Relaxed);
        let (tx, rx) = std::sync::mpsc::channel();
        self.export_rx = Some(rx);
        std::thread::spawn(move || {
            let r = motion::export(&c, &out, &root, |f| prog.store((f * 1000.0) as u32, Ordering::Relaxed));
            let _ = tx.send(r);
        });
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        if let Some((t0, from)) = self.playing {
            let t = from + t0.elapsed().as_secs_f32();
            self.time = if t >= self.compo.duration { 0.0 } else { t };
            if t >= self.compo.duration {
                self.playing = Some((Instant::now(), 0.0));
                self.start_sound();
            }
            ui.ctx().request_repaint();
        }
        if let Some(rx) = &self.export_rx {
            match rx.try_recv() {
                Ok(r) => {
                    self.message = Some(match r {
                        Ok(f) => format!("{} : {}", crate::i18n::t("exporté"), f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
                        Err(e) => e,
                    });
                    self.export_rx = None;
                }
                Err(_) => ui.ctx().request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }
        self.keyboard(ui);
        if self.playing.is_none() && self.sound.is_some() {
            self.stop_sound();
        }
        let frame = egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line));
        let full_h = ui.available_height();
        egui::Panel::left("motion-project").resizable(true).default_size(240.0).min_size(190.0).max_size(420.0).frame(frame).show(ui, |ui| self.project_panel(ui));
        egui::Panel::right("motion-inspector").resizable(true).default_size(290.0).min_size(240.0).max_size(460.0).frame(frame).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| self.inspector(ui));
        });
        egui::Panel::bottom("motion-timeline").resizable(true).default_size((full_h * 0.36).clamp(230.0, 320.0)).min_size(160.0).max_size((full_h * 0.7).max(200.0)).frame(frame).show(ui, |ui| self.timeline(ui));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.code_bg)).show(ui, |ui| self.viewer(ui));
    }

    // ---------- Projet ----------

    fn project_panel(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        ui.set_min_height(ui.available_height());
        crate::montage_ui::pane_head(ui, &crate::i18n::t("Projet"), None);
        egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
            section(ui, "Compositions");
            let me = crate::vault::inside(&self.root, &self.path).unwrap_or_default();
            let mut comps = self.comps.clone();
            if !comps.contains(&me) && !me.is_empty() {
                comps.insert(0, me.clone());
            }
            for rel in comps {
                let on = rel == me;
                let name = crate::formats::strip(rel.rsplit('/').next().unwrap_or(&rel)).to_string();
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
                if on {
                    ui.painter().rect_filled(r, 2.0, p.selection);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, 2.0, p.raised);
                }
                ui.painter().text(pos2(r.left() + 8.0, r.center().y), egui::Align2::LEFT_CENTER, l::SPLINE, egui::FontId::proportional(13.0), p.muted);
                ui.painter().text(pos2(r.left() + 28.0, r.center().y), egui::Align2::LEFT_CENTER, &name, egui::FontId::proportional(12.5), p.text);
                if on {
                    ui.painter().text(pos2(r.right() - 6.0, r.center().y), egui::Align2::RIGHT_CENTER, format!("{} s", self.compo.duration), egui::FontId::proportional(11.0), p.muted);
                }
                if resp.clicked() && !on {
                    self.open_request = Some(rel.clone());
                }
            }
            section(ui, "Ajouter un calque");
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                let c = (self.compo.w as f32 / 2.0, self.compo.h as f32 / 2.0);
                if ui.add(egui::Button::new(egui::RichText::new(l::TYPE).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Texte")).clicked() {
                    self.add_layer(Kind::Text { text: crate::i18n::t("Texte"), size: 120.0, color: [0xf4, 0xee, 0xf1], font: "inter-bold".into(), tracking: 0.0, align: 1 }, Some([c.0, c.1]));
                }
                if ui.add(egui::Button::new(egui::RichText::new(l::SQUARE).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Forme")).clicked() {
                    self.add_layer(Kind::Rect { w: 400.0, h: 240.0, radius: 12.0, color: [0xcc, 0xac, 0xb9] }, Some([c.0, c.1]));
                }
                if ui.add(egui::Button::new(egui::RichText::new(l::CIRCLE).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Ellipse")).clicked() {
                    self.add_layer(Kind::Ellipse { w: 260.0, h: 260.0, color: [0x8b, 0xa8, 0xc6] }, Some([c.0, c.1]));
                }
                if ui.add(egui::Button::new(egui::RichText::new(l::IMAGE_PLUS).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("Image…")).clicked() {
                    self.add_image();
                }
                let me = self.path.clone();
                let others: Vec<String> = self.comps.iter().filter(|r| !me.ends_with(r.as_str())).cloned().collect();
                ui.add_enabled_ui(!others.is_empty(), |ui| {
                    ui.menu_button(egui::RichText::new(l::FILM).size(14.0), |ui| {
                        for r in &others {
                            if ui.button(r).clicked() {
                                self.add_layer(Kind::Compo { path: r.clone(), speed: 1.0, offset: 0.0, looped: false }, Some([c.0, c.1]));
                                ui.close();
                            }
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("Précomposition : une autre composition du coffre, jouée comme un calque"));
                });
            });
            section(ui, "Préréglages d'animation");
            let has_sel = self.sel.is_some();
            for name in motion::PRESETS {
                let is_text = self.sel.and_then(|i| self.compo.layers.get(i)).is_some_and(|l| matches!(l.kind, Kind::Text { .. }));
                let ok = has_sel && (name != "Entrée lettre par lettre" || is_text);
                let b = ui.add_enabled(ok, egui::Button::new(crate::i18n::t(name)).min_size(vec2(ui.available_width(), 26.0)));
                if b.clicked() {
                    if let Some(i) = self.sel {
                        let t = self.time;
                        motion::apply_preset(&mut self.compo.layers[i], name, t);
                        self.changed();
                    }
                }
            }
            if !has_sel {
                ui.label(egui::RichText::new(crate::i18n::t("Choisis un calque pour l'animer.")).size(11.5).color(p.muted));
            }
        });
    }

    // ---------- Vue ----------

    fn viewer(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let full = ui.max_rect();
        let area = full.shrink2(vec2(56.0, 0.0)).with_min_y(full.top() + 52.0).with_max_y(full.bottom() - 16.0);
        let (cw, ch) = (self.compo.w as f32, self.compo.h as f32);
        let fit = (area.width() / cw).min(area.height() / ch).max(0.01);
        let s = self.zoom.unwrap_or(fit);
        let comp_r = Rect::from_center_size(area.center(), vec2(cw * s, ch * s));
        let resp = ui.interact(full, ui.scope_id().with("motion-view"), Sense::click_and_drag());
        let painter = ui.painter_at(full);
        // L'image : recalculée quand l'instant, la composition ou la taille changent
        let ppp = ui.ctx().pixels_per_point();
        let rs = (s * ppp).min(1600.0 / cw).max(0.05);
        let key = (self.time, self.rev, (rs * 1000.0) as u32);
        if self.shown != Some(key) || self.tex.is_none() {
            let img = motion::render(&self.compo, self.time, rs, &mut self.cache);
            let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
            match &mut self.tex {
                Some(t) => t.set(ci, egui::TextureOptions::LINEAR),
                None => self.tex = Some(ui.ctx().load_texture("motion-frame", ci, egui::TextureOptions::LINEAR)),
            }
            self.shown = Some(key);
        }
        painter.rect_filled(comp_r.expand(1.0), 0.0, p.line);
        if let Some(t) = &self.tex {
            painter.image(t.id(), comp_r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        let to_screen = |q: [f32; 2]| pos2(comp_r.left() + q[0] * s, comp_r.top() + q[1] * s);
        let to_comp = |q: Pos2| [(q.x - comp_r.left()) / s, (q.y - comp_r.top()) / s];

        // Sélection : cadre, trajet de la position, poignées
        let mut handles: Vec<(Pos2, u8)> = vec![];
        let mut path_pts: Vec<(Pos2, usize)> = vec![];
        if let Some(i) = self.sel.filter(|&i| i < self.compo.layers.len()) {
            let lay = &self.compo.layers[i];
            // Le trajet est dans le repère du parent : on le ramène dans la composition, instant par instant
            if lay.pos.animated() {
                let (a, b) = (lay.pos.keys[0].t, lay.pos.keys[lay.pos.keys.len() - 1].t);
                let n = ((b - a) * self.compo.fps).ceil().max(1.0) as usize;
                let at = |t: f32, v: [f32; 2]| to_screen(self.compo.parent_world(i, t).apply(v));
                let pts: Vec<Pos2> = (0..=n).map(|k| {
                    let t = a + (b - a) * k as f32 / n as f32;
                    at(t, lay.pos.at(t))
                }).collect();
                painter.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.2, p.warning), 2.0, 4.0));
                for k in &lay.pos.keys {
                    painter.circle_filled(at(k.t, k.v), 3.0, p.warning);
                }
            }
            // Le parent, relié d'un trait
            if let Some(pj) = lay.parent.and_then(|id| self.compo.index_of(id)) {
                let a = to_screen(self.compo.world(i, self.time).apply([0.0, 0.0]));
                let b = to_screen(self.compo.world(pj, self.time).apply([0.0, 0.0]));
                painter.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(1.0, p.info), 4.0, 4.0));
                painter.circle_stroke(b, 4.0, Stroke::new(1.0, p.info));
            }
            // Pelure d'oignon : trois fantômes de chaque côté, toutes les deux images, de plus en plus pâles
            if self.onion {
                let step = 2.0 / self.compo.fps.max(1.0);
                let now = motion::corners(&self.compo, i, self.time, &mut self.cache);
                for k in (1..=3).rev() {
                    for (sign, col) in [(-1.0f32, Color32::from_rgb(230, 90, 90)), (1.0, Color32::from_rgb(110, 200, 130))] {
                        let t = self.time + sign * k as f32 * step;
                        if t < 0.0 || t > self.compo.duration || !lay.live(t) {
                            continue;
                        }
                        let fade = 0.55 / k as f32;
                        let cc = motion::corners(&self.compo, i, t, &mut self.cache);
                        // Immobile à cet instant : pas de fantôme (il se poserait sur le calque)
                        if cc.iter().zip(now.iter()).all(|(a, b)| (a[0] - b[0]).abs() + (a[1] - b[1]).abs() < 0.5) {
                            continue;
                        }
                        let c = cc.map(to_screen);
                        painter.add(egui::Shape::convex_polygon(c.to_vec(), col.gamma_multiply(fade * 0.25), Stroke::new(1.2, col.gamma_multiply(fade))));
                    }
                }
            }
            if lay.live(self.time) || lay.visible {
                let c = motion::corners(&self.compo, i, self.time, &mut self.cache).map(to_screen);
                let mut pts = c.to_vec();
                pts.push(c[0]);
                painter.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.0, p.accent), 3.0, 3.0));
                for q in c {
                    handles.push((q, 0));
                }
                let top = pos2((c[0].x + c[1].x) / 2.0, (c[0].y + c[1].y) / 2.0);
                let center = to_screen(self.compo.world(i, self.time).apply([0.0, 0.0]));
                let dir = (top - center).normalized();
                let rot = top + dir * 22.0;
                painter.line_segment([top, rot], Stroke::new(1.0, p.accent));
                handles.push((rot, 1));
                // Tracé : ses points se déplacent à la souris
                if let Kind::Path { points, .. } = &lay.kind {
                    let x = self.compo.world(i, self.time);
                    for (k, q) in points.iter().enumerate() {
                        let sp = to_screen(x.apply(*q));
                        path_pts.push((sp, k));
                        painter.circle(sp, 4.0, Color32::WHITE, Stroke::new(1.5, p.accent));
                    }
                }
                for (q, kind) in &handles {
                    if *kind == 0 {
                        painter.rect(Rect::from_center_size(*q, vec2(7.0, 7.0)), 0.0, p.accent, Stroke::new(1.0, p.code_bg), egui::StrokeKind::Outside);
                    } else {
                        painter.circle(*q, 4.0, p.code_bg, Stroke::new(1.5, p.accent));
                    }
                }
            }
        }

        // Souris
        let hover = resp.hover_pos();
        let near = |q: Pos2| hover.is_some_and(|h| h.distance(q) < 9.0);
        if self.tool == Tool::Select && path_pts.iter().any(|(q, _)| near(*q)) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        } else if self.tool == Tool::Select {
            if let Some((_, k)) = handles.iter().find(|(q, _)| near(*q)) {
                ui.ctx().set_cursor_icon(if *k == 0 { egui::CursorIcon::ResizeNwSe } else { egui::CursorIcon::Alias });
            }
        } else if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if resp.drag_started() {
            // Là où le bouton a été enfoncé : le glissé démarre après quelques pixels, souvent déjà sur une autre poignée
            if let Some(m) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()) {
                let cp = to_comp(m);
                match self.tool {
                    Tool::Rect | Tool::Ellipse => self.view_drag = Some(ViewDrag::Draw { a: cp, b: cp }),
                    Tool::Pen => self.view_drag = Some(ViewDrag::Pen(vec![cp])),
                    Tool::Text => {}
                    Tool::Select if path_pts.iter().any(|(q, _)| q.distance(m) < 7.0) => {
                        let k = path_pts.iter().filter(|(q, _)| q.distance(m) < 7.0).min_by(|a, b| a.0.distance(m).total_cmp(&b.0.distance(m))).map(|x| x.1).unwrap_or(0);
                        self.view_drag = Some(ViewDrag::PathPt(k));
                    }
                    Tool::Select => {
                        let handle = handles.iter().find(|(q, _)| q.distance(m) < 9.0).map(|(_, k)| *k);
                        if let (Some(k), Some(i)) = (handle, self.sel) {
                            let lay = &self.compo.layers[i];
                            let c = self.compo.world(i, self.time).apply([0.0, 0.0]);
                            let d = ((cp[0] - c[0]).hypot(cp[1] - c[1])).max(1.0);
                            self.view_drag = Some(if k == 0 {
                                ViewDrag::Scale { from: lay.scale.at(self.time), d0: d }
                            } else {
                                ViewDrag::Rotate { from: lay.rot.at(self.time)[0], a0: (cp[1] - c[1]).atan2(cp[0] - c[0]) }
                            });
                        } else {
                            self.sel = motion::pick(&self.compo, self.time, cp, &mut self.cache);
                            self.sel_key = None;
                            if let Some(i) = self.sel {
                                self.view_drag = Some(ViewDrag::Move { start: m, from: self.compo.layers[i].pos.at(self.time) });
                            }
                        }
                    }
                }
            }
        }
        if resp.dragged() {
            if let (Some(m), Some(d)) = (resp.interact_pointer_pos(), &mut self.view_drag) {
                let cp = to_comp(m);
                let t = self.time;
                match d {
                    ViewDrag::Draw { b, .. } => *b = cp,
                    ViewDrag::PathPt(k) => {
                        if let Some(i) = self.sel {
                            let x = self.compo.world(i, t);
                            if let (Some(lp), Kind::Path { points, .. }) = (x.inverse(cp), &mut self.compo.layers[i].kind) {
                                if let Some(q) = points.get_mut(*k) {
                                    *q = lp;
                                    self.rev += 1;
                                }
                            }
                        }
                    }
                    ViewDrag::Pen(pts) => {
                        if pts.last().is_none_or(|q| (q[0] - cp[0]).hypot(q[1] - cp[1]) > 1.5 / s) {
                            pts.push(cp);
                        }
                    }
                    ViewDrag::Move { start, from } => {
                        if let Some(i) = self.sel {
                            let mut dv = [(m.x - start.x) / s, (m.y - start.y) / s];
                            if ui.input(|i| i.modifiers.shift) {
                                if dv[0].abs() > dv[1].abs() {
                                    dv[1] = 0.0;
                                } else {
                                    dv[0] = 0.0;
                                }
                            }
                            // Le déplacement à l'écran, ramené dans le repère du parent
                            let dv = self.compo.parent_world(i, t).inverse_vec(dv).unwrap_or(dv);
                            let v = [from[0] + dv[0], from[1] + dv[1]];
                            self.compo.layers[i].pos.set(t, v);
                            self.rev += 1;
                        }
                    }
                    ViewDrag::Scale { from, d0 } => {
                        if let Some(i) = self.sel {
                            let c = self.compo.world(i, t).apply([0.0, 0.0]);
                            let f = (cp[0] - c[0]).hypot(cp[1] - c[1]) / *d0;
                            self.compo.layers[i].scale.set(t, [from[0] * f, from[1] * f]);
                            self.rev += 1;
                        }
                    }
                    ViewDrag::Rotate { from, a0 } => {
                        if let Some(i) = self.sel {
                            let c = self.compo.world(i, t).apply([0.0, 0.0]);
                            let mut a = *from + ((cp[1] - c[1]).atan2(cp[0] - c[0]) - *a0).to_degrees();
                            if ui.input(|i| i.modifiers.shift) {
                                a = (a / 15.0).round() * 15.0;
                            }
                            self.compo.layers[i].rot.set(t, [a, 0.0]);
                            self.rev += 1;
                        }
                    }
                }
            }
        }
        if let Some(ViewDrag::Pen(pts)) = &self.view_drag {
            let line: Vec<Pos2> = pts.iter().map(|q| to_screen(*q)).collect();
            painter.add(egui::Shape::line(line, Stroke::new(2.0, p.accent)));
        }
        if let Some(ViewDrag::Draw { a, b }) = &self.view_drag {
            let r = Rect::from_two_pos(to_screen(*a), to_screen(*b));
            if self.tool == Tool::Ellipse {
                painter.add(egui::Shape::ellipse_stroke(r.center(), r.size() / 2.0, Stroke::new(1.0, p.accent)));
            } else {
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
            }
        }
        if resp.drag_stopped() && matches!(self.view_drag, Some(ViewDrag::Pen(_))) {
            if let Some(ViewDrag::Pen(pts)) = self.view_drag.take() {
                // Le geste simplifié, centré sur son cadre : le calque est posé au centre
                let pts = simplify(&pts, 2.5 / s);
                if pts.len() >= 2 {
                    let (lo, hi) = pts.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), q| ([lo[0].min(q[0]), lo[1].min(q[1])], [hi[0].max(q[0]), hi[1].max(q[1])]));
                    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
                    let points = pts.iter().map(|q| [q[0] - c[0], q[1] - c[1]]).collect();
                    self.add_layer(Kind::Path { points, closed: false, width: 12.0, color: [0xf4, 0xee, 0xf1], arrow: false }, Some(c));
                }
            }
        }
        if resp.drag_stopped() {
            if let Some(ViewDrag::Draw { a, b }) = self.view_drag.take() {
                let (w, h) = ((b[0] - a[0]).abs().max(8.0), (b[1] - a[1]).abs().max(8.0));
                let c = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
                let kind = if self.tool == Tool::Ellipse { Kind::Ellipse { w, h, color: [0x8b, 0xa8, 0xc6] } } else { Kind::Rect { w, h, radius: 0.0, color: [0xcc, 0xac, 0xb9] } };
                self.add_layer(kind, Some(c));
                self.tool = Tool::Select;
            } else {
                self.view_drag = None;
                self.changed();
            }
        }
        if resp.clicked() {
            if let Some(m) = resp.interact_pointer_pos() {
                let cp = to_comp(m);
                match self.tool {
                    Tool::Text => {
                        self.add_layer(Kind::Text { text: crate::i18n::t("Texte"), size: 120.0, color: [0xf4, 0xee, 0xf1], font: "inter-bold".into(), tracking: 0.0, align: 1 }, Some(cp));
                        self.tool = Tool::Select;
                    }
                    _ => {
                        self.sel = motion::pick(&self.compo, self.time, cp, &mut self.cache);
                        self.sel_key = None;
                    }
                }
            }
        }
        if resp.hovered() {
            let (scroll, ctrl) = ui.input(|i| (i.smooth_scroll_delta.y, i.modifiers.command));
            if ctrl && scroll != 0.0 {
                self.zoom = Some((s * (1.0 + scroll * 0.002)).clamp(0.05, 4.0));
            }
        }

        // Outils, à gauche
        let tools = [(Tool::Select, l::MOUSE_POINTER_2, "Sélectionner (V)"), (Tool::Rect, l::SQUARE, "Rectangle (Q)"), (Tool::Ellipse, l::CIRCLE, "Ellipse (E)"), (Tool::Text, l::TYPE, "Texte (T)"), (Tool::Pen, l::PEN_TOOL, "Tracé à main levée (P) : dessinez dans la composition")];
        let strip = Rect::from_min_size(pos2(full.left() + 12.0, full.top() + 56.0), vec2(38.0, 8.0 + 32.0 * tools.len() as f32));
        painter.rect(strip, 6.0, p.panel, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
        for (k, (t, icon, tip)) in tools.iter().enumerate() {
            let r = Rect::from_min_size(pos2(strip.left() + 4.0, strip.top() + 4.0 + 32.0 * k as f32), vec2(30.0, 30.0));
            let b = ui.interact(r, ui.scope_id().with(("motion-tool", k)), Sense::click()).on_hover_text(crate::i18n::t(tip));
            let on = self.tool == *t;
            if on {
                painter.rect_filled(r, 4.0, p.accent);
            } else if b.hovered() {
                painter.rect_filled(r, 4.0, p.raised);
            }
            painter.text(r.center(), egui::Align2::CENTER_CENTER, *icon, egui::FontId::proportional(15.0), if on { p.on_accent } else { p.text });
            if b.clicked() {
                self.tool = *t;
            }
        }

        // En-tête flottant
        let head = Rect::from_min_max(pos2(full.left() + 12.0, full.top() + 10.0), pos2(full.right() - 12.0, full.top() + 44.0));
        painter.rect(head, 6.0, p.panel, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(head.shrink2(vec2(6.0, 3.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        flat(&mut child);
        let ui = &mut child;
        ui.menu_button(crate::i18n::t("Calque"), |ui| {
            let c = [self.compo.w as f32 / 2.0, self.compo.h as f32 / 2.0];
            if ui.button(crate::i18n::t("nouveau texte")).clicked() {
                self.add_layer(Kind::Text { text: crate::i18n::t("Texte"), size: 120.0, color: [0xf4, 0xee, 0xf1], font: "inter-bold".into(), tracking: 0.0, align: 1 }, Some(c));
            }
            if ui.button(crate::i18n::t("nouveau rectangle")).clicked() {
                self.add_layer(Kind::Rect { w: 400.0, h: 240.0, radius: 12.0, color: [0xcc, 0xac, 0xb9] }, Some(c));
            }
            if ui.button(format!("{}  {}", crate::lucide::CIRCLE, crate::i18n::t("nouvelle ellipse"))).clicked() {
                self.add_layer(Kind::Ellipse { w: 260.0, h: 260.0, color: [0x8b, 0xa8, 0xc6] }, Some(c));
            }
            if ui.button(format!("{}  {}", crate::lucide::IMAGE, crate::i18n::t("image…"))).clicked() {
                self.add_image();
            }
            ui.separator();
            let sel = self.sel;
            if ui.add_enabled(sel.is_some(), egui::Button::new(format!("{}  {}", crate::lucide::COPY, crate::i18n::t("dupliquer")))).clicked() {
                if let Some(i) = sel {
                    let mut d = self.compo.layers[i].clone();
                    d.name = self.compo.fresh_name(&d.name);
                    d.id = motion::new_id();
                    self.compo.layers.insert(i, d);
                    self.changed();
                }
            }
            if ui.add_enabled(sel.is_some_and(|i| i > 0), egui::Button::new(format!("{}  {}", crate::lucide::ARROW_UP, crate::i18n::t("monter")))).clicked() {
                if let Some(i) = sel {
                    self.compo.layers.swap(i, i - 1);
                    self.sel = Some(i - 1);
                    self.changed();
                }
            }
            if ui.add_enabled(sel.is_some_and(|i| i + 1 < self.compo.layers.len()), egui::Button::new(format!("{}  {}", crate::lucide::ARROW_DOWN, crate::i18n::t("descendre")))).clicked() {
                if let Some(i) = sel {
                    self.compo.layers.swap(i, i + 1);
                    self.sel = Some(i + 1);
                    self.changed();
                }
            }
            if ui.add_enabled(sel.is_some(), egui::Button::new(format!("{}  {}", crate::lucide::TRASH_2, crate::i18n::t("supprimer")))).clicked() {
                if let Some(i) = sel {
                    self.compo.layers.remove(i);
                    self.sel = None;
                    self.changed();
                }
            }
        });
        ui.menu_button(crate::i18n::t("Animation"), |ui| {
            for name in motion::PRESETS {
                if ui.add_enabled(self.sel.is_some(), egui::Button::new(crate::i18n::t(name))).clicked() {
                    if let Some(i) = self.sel {
                        let t = self.time;
                        motion::apply_preset(&mut self.compo.layers[i], name, t);
                        self.changed();
                    }
                }
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let busy = self.export_rx.is_some();
            let label = if busy { format!("{} {} %", crate::i18n::t("Export"), self.export_prog.load(Ordering::Relaxed) / 10) } else { crate::i18n::t("Exporter MP4") };
            if ui.add_enabled(!busy, egui::Button::new(format!("{}  {}", l::FILM, label))).clicked() {
                self.export();
            }
            ui.add_space(4.0);
            let zl = match self.zoom {
                None => crate::i18n::t("Ajusté"),
                Some(z) => format!("{} %", (z * 100.0).round()),
            };
            ui.menu_button(zl, |ui| {
                if ui.add(egui::Button::new(format!("{}  {}", crate::lucide::SCAN, crate::i18n::t("ajuster")))).clicked() {
                    self.zoom = None;
                }
                for z in [0.25, 0.5, 1.0] {
                    if ui.button(format!("{} %", z * 100.0)).clicked() {
                        self.zoom = Some(z);
                    }
                }
            });
            ui.label(egui::RichText::new(format!("{} × {}", self.compo.w, self.compo.h)).size(11.5).color(p.muted));
        });
        if let Some(m) = self.message.clone() {
            if self.msg_seen.as_ref().is_none_or(|(s, _)| *s != m) {
                self.msg_seen = Some((m, Instant::now()));
            } else if self.msg_seen.as_ref().is_some_and(|(_, t)| t.elapsed().as_secs_f32() > 5.0) {
                self.message = None;
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        }
        if let Some(m) = self.message.clone().or(self.error.clone()) {
            let r = Rect::from_min_size(pos2(full.left() + 64.0, full.bottom() - 40.0), vec2(full.width() - 128.0, 26.0));
            painter.rect_filled(r, 4.0, p.raised);
            painter.text(r.left_center() + vec2(10.0, 0.0), egui::Align2::LEFT_CENTER, &m, egui::FontId::proportional(12.0), p.text);
        }
    }

    // ---------- Inspecteur ----------

    fn inspector(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        if let Some(k) = crate::montage_ui::tab_chips(ui, self.insp_tab, &[&crate::i18n::t("Propriétés"), &crate::i18n::t("Composition")]) {
            self.insp_tab = k;
        }
        let mut changed = false;
        let mut new_parent: Option<Option<u64>> = None;
        egui::Frame::new().inner_margin(12).show(ui, |ui| {
            if self.insp_tab == 1 {
                changed |= self.compo_settings(ui);
                return;
            }
            let Some(i) = self.sel.filter(|&i| i < self.compo.layers.len()) else {
                ui.label(egui::RichText::new(crate::i18n::t("Clique un calque dans la vue ou la timeline.")).color(p.muted));
                return;
            };
            let t = self.time;
            // Parents possibles : ni lui-même, ni ses descendants
            let banned = self.compo.descendants(i);
            let others: Vec<(u64, String)> = self.compo.layers.iter().enumerate().filter(|(k, _)| *k != i && !banned.contains(k)).map(|(_, l)| (l.id, l.name.clone())).collect();
            let lay = &mut self.compo.layers[i];
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(layer_icon(&lay.kind)).size(15.0).color(layer_color(&lay.kind)));
                changed |= ui.add(egui::TextEdit::singleline(&mut lay.name).frame(egui::Frame::NONE).font(egui::FontId::new(15.0, crate::theme::bold())).desired_width(ui.available_width() - 70.0)).changed();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(egui::RichText::new(crate::i18n::t(lay.kind.label())).size(11.5).color(p.muted)));
            });
            section(ui, "Transformer");
            for (prop, label) in PROPS {
                let tr = lay.track(prop);
                let at = tr.key_at(t).is_some();
                let anim = tr.animated();
                let mut v = tr.at(t);
                let row = ui.horizontal(|ui| {
                    let (r, b) = ui.allocate_exact_size(vec2(16.0, 22.0), Sense::click());
                    diamond(ui.painter(), r.center(), 4.5, if at { Some(p.accent) } else if anim { Some(p.accent.gamma_multiply(0.35)) } else { None }, if anim { p.accent } else { p.muted });
                    let b = b.on_hover_text(crate::i18n::t(if at { "Retirer la clé à cet instant" } else { "Poser une clé à cet instant (anime la propriété)" }));
                    ui.add_sized(vec2(64.0, 22.0), egui::Label::new(egui::RichText::new(crate::i18n::t(label)).color(p.muted)));
                    let mut ch = false;
                    match prop {
                        Prop::Pos => {
                            ch |= ui.add(egui::DragValue::new(&mut v[0]).speed(1.0).max_decimals(1).prefix("x ")).changed();
                            ch |= ui.add(egui::DragValue::new(&mut v[1]).speed(1.0).max_decimals(1).prefix("y ")).changed();
                        }
                        Prop::Scale => {
                            let mut u = v[0];
                            if ui.add(egui::DragValue::new(&mut u).speed(0.5).max_decimals(1).suffix(" %")).changed() {
                                let r = if v[0].abs() > 1e-3 { v[1] / v[0] } else { 1.0 };
                                v = [u, u * r];
                                ch = true;
                            }
                            ch |= ui.add(egui::DragValue::new(&mut v[1]).speed(0.5).max_decimals(1).prefix("y ").suffix(" %")).changed();
                        }
                        Prop::Rot => ch |= ui.add(egui::DragValue::new(&mut v[0]).speed(0.5).max_decimals(1).suffix("°")).changed(),
                        Prop::Opacity => ch |= ui.add(egui::DragValue::new(&mut v[0]).speed(0.5).range(0.0..=100.0).max_decimals(0).suffix(" %")).changed(),
                    }
                    (b.clicked(), ch)
                });
                let (toggle, ch) = row.inner;
                if toggle {
                    lay.track_mut(prop).toggle_key(t);
                    changed = true;
                }
                if ch {
                    lay.track_mut(prop).set(t, v);
                    changed = true;
                }
            }
            match &mut lay.kind {
                Kind::Text { text, size, color, font, tracking, align } => {
                    section(ui, "Texte");
                    changed |= ui.add(egui::TextEdit::multiline(text).desired_rows(2).desired_width(f32::INFINITY)).changed();
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("motion-font").selected_text(motion::FONTS.iter().find(|f| f.0 == font).map(|f| f.1).unwrap_or("Inter gras")).show_ui(ui, |ui| {
                            for (id, name) in motion::FONTS {
                                if ui.selectable_label(font == id, name).clicked() {
                                    *font = id.to_string();
                                    changed = true;
                                }
                            }
                        });
                        changed |= ui.color_edit_button_srgb(color).changed();
                    });
                    ui.horizontal(|ui| {
                        changed |= ui.add(egui::DragValue::new(size).speed(1.0).range(4.0..=1000.0).prefix(crate::i18n::t("taille ")).suffix(" px")).changed();
                        changed |= ui.add(egui::DragValue::new(tracking).speed(0.2).range(-50.0..=200.0).prefix(crate::i18n::t("espace "))).changed();
                    });
                    ui.horizontal(|ui| {
                        for (k, (icon, tip)) in [(l::ALIGN_LEFT, "à gauche"), (l::ALIGN_CENTER, "centré"), (l::ALIGN_RIGHT, "à droite")].iter().enumerate() {
                            if ui.selectable_label(*align == k as u8, egui::RichText::new(*icon)).on_hover_text(crate::i18n::t(tip)).clicked() {
                                *align = k as u8;
                                changed = true;
                            }
                        }
                    });
                }
                Kind::Rect { w, h, radius, color } => {
                    section(ui, "Forme");
                    ui.horizontal(|ui| {
                        changed |= ui.add(egui::DragValue::new(w).speed(1.0).range(1.0..=10000.0).prefix("l ")).changed();
                        changed |= ui.add(egui::DragValue::new(h).speed(1.0).range(1.0..=10000.0).prefix("h ")).changed();
                        changed |= ui.color_edit_button_srgb(color).changed();
                    });
                    changed |= ui.add(egui::DragValue::new(radius).speed(0.5).range(0.0..=5000.0).prefix(crate::i18n::t("coins "))).changed();
                }
                Kind::Ellipse { w, h, color } => {
                    section(ui, "Forme");
                    ui.horizontal(|ui| {
                        changed |= ui.add(egui::DragValue::new(w).speed(1.0).range(1.0..=10000.0).prefix("l ")).changed();
                        changed |= ui.add(egui::DragValue::new(h).speed(1.0).range(1.0..=10000.0).prefix("h ")).changed();
                        changed |= ui.color_edit_button_srgb(color).changed();
                    });
                }
                Kind::Image { path } => {
                    section(ui, "Image");
                    ui.label(egui::RichText::new(path.as_str()).size(11.5).color(p.muted));
                }
                Kind::Path { closed, width, color, arrow, .. } => {
                    section(ui, "Tracé");
                    ui.horizontal(|ui| {
                        changed |= ui.add(egui::DragValue::new(width).speed(0.3).range(0.5..=400.0).max_decimals(1).prefix(format!("{} ", crate::i18n::t("épaisseur")))).changed();
                        changed |= ui.color_edit_button_srgb(color).changed();
                        changed |= ui.checkbox(closed, crate::i18n::t("fermé")).changed();
                        changed |= ui.toggle_value(arrow, egui::RichText::new(l::MOVE_RIGHT).size(14.0)).on_hover_text(crate::i18n::t("Pointe de flèche au bout du tracé")).changed();
                    });
                }
                Kind::Compo { path, speed, offset, looped } => {
                    section(ui, "Précomposition");
                    ui.label(egui::RichText::new(path.as_str()).size(11.5).color(p.muted));
                    // Remappage du temps
                    ui.horizontal(|ui| {
                        let mut pc = *speed * 100.0;
                        if ui.add(egui::DragValue::new(&mut pc).speed(1.0).range(-800.0..=800.0).max_decimals(0).prefix(format!("{} ", l::GAUGE)).suffix(" %")).on_hover_text(crate::i18n::t("Vitesse : 200 % deux fois plus vite, -100 % à l'envers")).changed() {
                            *speed = pc / 100.0;
                            changed = true;
                        }
                        changed |= ui.add(egui::DragValue::new(offset).speed(0.02).max_decimals(2).prefix(format!("{} ", l::SKIP_FORWARD)).suffix(" s")).on_hover_text(crate::i18n::t("Départ : l'instant de la précomposition joué au début du calque")).changed();
                        changed |= ui.toggle_value(looped, egui::RichText::new(l::REPEAT).size(14.0)).on_hover_text(crate::i18n::t("En boucle au lieu de garder la dernière image")).changed();
                    });
                    if ui.button(format!("{}  {}", l::EXTERNAL_LINK, crate::i18n::t("ouvrir la précomposition"))).clicked() {
                        self.open_request = Some(path.clone());
                    }
                    if ui.button(format!("{}  {}", l::REFRESH_CW, crate::i18n::t("recharger"))).on_hover_text(crate::i18n::t("Relit la précomposition après l'avoir modifiée")).clicked() {
                        self.cache.forget_compos();
                        changed = true;
                    }
                }
            }
            if matches!(lay.kind, Kind::Path { .. }) {
                section(ui, "Tracé progressif");
                // Fin et début de la partie visible ; une clé à cet instant si la propriété est animée
                let tr = &mut lay.trim;
                let mut v = tr.at(t);
                let at = tr.key_at(t).is_some();
                let anim = tr.animated();
                ui.horizontal(|ui| {
                    let (r, b) = ui.allocate_exact_size(vec2(16.0, 22.0), Sense::click());
                    diamond(ui.painter(), r.center(), 4.5, if at { Some(p.accent) } else if anim { Some(p.accent.gamma_multiply(0.35)) } else { None }, if anim { p.accent } else { p.muted });
                    if b.on_hover_text(crate::i18n::t(if at { "Retirer la clé à cet instant" } else { "Poser une clé à cet instant (anime la propriété)" })).clicked() {
                        match tr.key_at(t) {
                            Some(k) => {
                                tr.keys.remove(k);
                                if tr.keys.is_empty() {
                                    tr.v = v;
                                }
                            }
                            None => tr.insert(motion::Key { t, v, ease: motion::Ease::Smooth, tan: vec![] }),
                        }
                        changed = true;
                    }
                    let mut ch = ui.add(egui::DragValue::new(&mut v[0]).speed(0.5).range(0.0..=100.0).max_decimals(0).prefix(format!("{} ", crate::i18n::t("fin"))).suffix(" %")).changed();
                    ch |= ui.add(egui::DragValue::new(&mut v[1]).speed(0.5).range(0.0..=100.0).max_decimals(0).prefix(format!("{} ", crate::i18n::t("début"))).suffix(" %")).changed();
                    if ch {
                        tr.set(t, v);
                        changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    // Le tracé se dessine en une seconde depuis la tête de lecture, ou s'efface jusqu'à la fin du calque
                    let tr = &mut lay.trim;
                    if ui.button(format!("{}  {}", l::PEN_TOOL, crate::i18n::t("Animer le dessin"))).on_hover_text(crate::i18n::t("Le tracé se dessine en une seconde à partir de la tête de lecture")).clicked() {
                        tr.keys.retain(|k| k.t < t - 1e-3 || k.t > t + 1.0 + 1e-3);
                        tr.insert(motion::Key { t, v: [0.0, 0.0], ease: motion::Ease::Smooth, tan: vec![] });
                        tr.insert(motion::Key { t: t + 1.0, v: [100.0, 0.0], ease: motion::Ease::Smooth, tan: vec![] });
                        changed = true;
                    }
                    if ui.button(format!("{}  {}", l::ERASER, crate::i18n::t("Effacer à la fin"))).on_hover_text(crate::i18n::t("Le tracé s'efface pendant la dernière seconde du calque")).clicked() {
                        let e = lay.end;
                        let tr = &mut lay.trim;
                        tr.keys.retain(|k| k.t < e - 1.0 - 1e-3);
                        tr.insert(motion::Key { t: e - 1.0, v: [100.0, 0.0], ease: motion::Ease::Smooth, tan: vec![] });
                        tr.insert(motion::Key { t: e, v: [100.0, 100.0], ease: motion::Ease::Smooth, tan: vec![] });
                        changed = true;
                    }
                });
            }
            if matches!(lay.kind, Kind::Text { .. }) {
                section(ui, "Animateur de texte");
                let mut on = lay.reveal.is_some();
                if ui.checkbox(&mut on, crate::i18n::t("Entrée morceau par morceau")).changed() {
                    lay.reveal = on.then(motion::Reveal::default);
                    changed = true;
                }
                if let Some(r) = &mut lay.reveal {
                    ui.horizontal(|ui| {
                        for (u, name) in [(motion::Unit::Letter, "Lettre"), (motion::Unit::Word, "Mot"), (motion::Unit::Line, "Ligne")] {
                            if ui.selectable_label(r.by == u, crate::i18n::t(name)).clicked() {
                                r.by = u;
                                changed = true;
                            }
                        }
                    });
                    let mut ms = r.delay * 1000.0;
                    ui.horizontal(|ui| {
                        if ui.add(egui::DragValue::new(&mut ms).speed(1.0).range(0.0..=2000.0).prefix(crate::i18n::t("décalage ")).suffix(" ms")).changed() {
                            r.delay = ms / 1000.0;
                            changed = true;
                        }
                        changed |= ui.add(egui::DragValue::new(&mut r.rise).speed(1.0).range(-1000.0..=1000.0).prefix(crate::i18n::t("départ y ")).suffix(" px")).changed();
                    });
                    ui.horizontal(|ui| {
                        changed |= ui.add(egui::DragValue::new(&mut r.dur).speed(0.01).range(0.05..=5.0).prefix(crate::i18n::t("durée ")).suffix(" s")).changed();
                        egui::ComboBox::from_id_salt("motion-reveal-ease").selected_text(crate::i18n::t(motion::EASES.iter().find(|e| e.0 == r.ease).map(|e| e.1).unwrap_or("Doux"))).show_ui(ui, |ui| {
                            for (e, name) in motion::EASES {
                                if ui.selectable_label(r.ease == e, crate::i18n::t(name)).clicked() {
                                    r.ease = e;
                                    changed = true;
                                }
                            }
                        });
                    });
                }
            }
            section(ui, "Lisser les clés");
            let target = self.sel_key.filter(|k| k.0 == i);
            ui.label(egui::RichText::new(crate::i18n::t(if target.is_some() { "La clé choisie, vers la suivante" } else { "Toutes les clés du calque" })).size(11.0).color(p.muted));
            let cur = target.and_then(|(_, pr, kt)| lay.track(pr).key_at(kt).map(|k| lay.track(pr).keys[k].ease));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                for (e, name) in motion::EASES {
                    if ui.selectable_label(cur == Some(e), crate::i18n::t(name)).clicked() {
                        match target {
                            Some((_, pr, kt)) => {
                                let tr = lay.track_mut(pr);
                                if let Some(k) = tr.key_at(kt) {
                                    tr.keys[k].ease = e;
                                }
                            }
                            None => {
                                for (pr, _) in PROPS {
                                    for k in &mut lay.track_mut(pr).keys {
                                        k.ease = e;
                                    }
                                }
                            }
                        }
                        changed = true;
                    }
                }
            });
            section(ui, "Calque");
            ui.horizontal(|ui| {
                changed |= ui.add(egui::DragValue::new(&mut lay.start).speed(0.02).range(0.0..=lay.end).max_decimals(2).prefix(crate::i18n::t("début ")).suffix(" s")).changed();
                let d = self.compo.duration;
                changed |= ui.add(egui::DragValue::new(&mut lay.end).speed(0.02).range(lay.start..=d.max(lay.start)).max_decimals(2).prefix(crate::i18n::t("fin ")).suffix(" s")).changed();
            });
            changed |= ui.checkbox(&mut lay.visible, crate::i18n::t("Visible")).changed();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(crate::i18n::t("Parent")).color(p.muted));
                let cur = lay.parent.and_then(|id| others.iter().find(|o| o.0 == id)).map(|o| o.1.clone()).unwrap_or_else(|| crate::i18n::t("aucun"));
                egui::ComboBox::from_id_salt("motion-parent").selected_text(cur).show_ui(ui, |ui| {
                    if ui.selectable_label(lay.parent.is_none(), crate::i18n::t("aucun")).clicked() {
                        new_parent = Some(None);
                    }
                    for (id, name) in &others {
                        if ui.selectable_label(lay.parent == Some(*id), name).clicked() {
                            new_parent = Some(Some(*id));
                        }
                    }
                });
            }).response.on_hover_text(crate::i18n::t("Le calque suit son parent : déplacer, tourner ou agrandir le parent l'emporte avec lui"));
            // Cache de piste : le calque du dessus découpe celui-ci (et ne se dessine plus lui-même)
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(crate::i18n::t("Cache")).color(p.muted));
                let cur = lay.matte.map(|m| crate::i18n::t(m.name())).unwrap_or_else(|| crate::i18n::t("aucun"));
                ui.add_enabled_ui(i > 0, |ui| {
                    egui::ComboBox::from_id_salt("motion-matte").selected_text(cur).show_ui(ui, |ui| {
                        if ui.selectable_label(lay.matte.is_none(), crate::i18n::t("aucun")).clicked() {
                            lay.matte = None;
                            changed = true;
                        }
                        for m in motion::Matte::ALL {
                            if ui.selectable_label(lay.matte == Some(m), crate::i18n::t(m.name())).clicked() {
                                lay.matte = Some(m);
                                changed = true;
                            }
                        }
                    });
                });
            }).response.on_hover_text(crate::i18n::t("Cache de piste (comme After Effects) : ce calque n'apparaît qu'à travers le calque juste au-dessus, qui ne se dessine plus"));
            section(ui, "Style");
            let st = &mut lay.style;
            let mut on = st.stroke.is_some();
            if ui.checkbox(&mut on, crate::i18n::t("Contour")).changed() {
                st.stroke = on.then(|| motion::StrokeStyle { width: 6.0, color: [0x14, 0x10, 0x1a] });
                changed = true;
            }
            if let Some(sk) = &mut st.stroke {
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::DragValue::new(&mut sk.width).speed(0.2).range(0.5..=100.0).prefix(crate::i18n::t("épaisseur ")).suffix(" px")).changed();
                    changed |= ui.color_edit_button_srgb(&mut sk.color).changed();
                });
            }
            let mut on = st.shadow.is_some();
            if ui.checkbox(&mut on, crate::i18n::t("Ombre portée")).changed() {
                st.shadow = on.then(|| motion::Shadow { dx: 8.0, dy: 12.0, blur: 18.0, color: [0, 0, 0], opacity: 0.6 });
                changed = true;
            }
            if let Some(sh) = &mut st.shadow {
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::DragValue::new(&mut sh.dx).speed(0.5).range(-500.0..=500.0).prefix("x ")).changed();
                    changed |= ui.add(egui::DragValue::new(&mut sh.dy).speed(0.5).range(-500.0..=500.0).prefix("y ")).changed();
                    changed |= ui.color_edit_button_srgb(&mut sh.color).changed();
                });
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::DragValue::new(&mut sh.blur).speed(0.5).range(0.0..=200.0).prefix(crate::i18n::t("flou ")).suffix(" px")).changed();
                    let mut o = sh.opacity * 100.0;
                    if ui.add(egui::DragValue::new(&mut o).speed(0.5).range(0.0..=100.0).max_decimals(0).suffix(" %")).changed() {
                        sh.opacity = o / 100.0;
                        changed = true;
                    }
                });
            }
            let mut on = st.glow.is_some();
            if ui.checkbox(&mut on, crate::i18n::t("Lueur")).changed() {
                st.glow = on.then(|| motion::Glow { radius: 30.0, color: [0xcc, 0xac, 0xb9], strength: 1.2 });
                changed = true;
            }
            if let Some(g) = &mut st.glow {
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::DragValue::new(&mut g.radius).speed(0.5).range(1.0..=300.0).prefix(crate::i18n::t("rayon ")).suffix(" px")).changed();
                    changed |= ui.add(egui::DragValue::new(&mut g.strength).speed(0.02).range(0.1..=4.0).prefix(crate::i18n::t("force "))).changed();
                    changed |= ui.color_edit_button_srgb(&mut g.color).changed();
                });
            }
            section(ui, "Tremblement");
            let mut on = lay.wiggle.is_some();
            if ui.checkbox(&mut on, crate::i18n::t("Bouger tout seul")).on_hover_text(crate::i18n::t("Petits mouvements au hasard, doux, en plus de l'animation (caméra à l'épaule, objet qui flotte)")).changed() {
                lay.wiggle = on.then(motion::Wiggle::default);
                changed = true;
            }
            if let Some(w) = &mut lay.wiggle {
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::DragValue::new(&mut w.freq).speed(0.05).range(0.1..=30.0).max_decimals(2).suffix(crate::i18n::t_mid(" /s"))).on_hover_text(crate::i18n::t("Mouvements par seconde")).changed();
                    changed |= ui.add(egui::DragValue::new(&mut w.pos).speed(0.5).range(0.0..=2000.0).max_decimals(0).prefix(crate::i18n::t("position ")).suffix(" px")).changed();
                });
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::DragValue::new(&mut w.rot).speed(0.1).range(0.0..=180.0).max_decimals(1).prefix(crate::i18n::t("rotation ")).suffix("°")).changed();
                    changed |= ui.add(egui::DragValue::new(&mut w.scale).speed(0.1).range(0.0..=100.0).max_decimals(1).prefix(crate::i18n::t("échelle ")).suffix(" %")).changed();
                    if ui.small_button(crate::lucide::DICES).on_hover_text(crate::i18n::t("Autre tirage")).clicked() {
                        w.seed = w.seed.wrapping_add(1);
                        changed = true;
                    }
                });
            }
        });
        if let (Some(np), Some(i)) = (new_parent, self.sel) {
            let t = self.time;
            self.compo.set_parent(i, np, t);
            changed = true;
        }
        if changed {
            self.changed();
        }
    }

    fn compo_settings(&mut self, ui: &mut egui::Ui) -> bool {
        let p = crate::theme::pal();
        let c = &mut self.compo;
        let mut changed = false;
        section(ui, "Format");
        ui.horizontal_wrapped(|ui| {
            for (w, h, name) in [(1920, 1080, "16:9"), (1080, 1920, "9:16"), (1080, 1080, "1:1"), (3840, 2160, "4K")] {
                if ui.selectable_label(c.w == w && c.h == h, name).clicked() {
                    // Les calques gardent leur place relative au centre
                    let (dx, dy) = ((w as f32 - c.w as f32) / 2.0, (h as f32 - c.h as f32) / 2.0);
                    for l in &mut c.layers {
                        l.pos.v = [l.pos.v[0] + dx, l.pos.v[1] + dy];
                        for k in &mut l.pos.keys {
                            k.v = [k.v[0] + dx, k.v[1] + dy];
                        }
                    }
                    (c.w, c.h) = (w, h);
                    changed = true;
                }
            }
        });
        ui.horizontal(|ui| {
            changed |= ui.add(egui::DragValue::new(&mut c.w).speed(2.0).range(16..=8192).prefix("l ")).changed();
            changed |= ui.add(egui::DragValue::new(&mut c.h).speed(2.0).range(16..=8192).prefix("h ")).changed();
        });
        section(ui, "Temps");
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("motion-fps").selected_text(format!("{} i/s", c.fps)).show_ui(ui, |ui| {
                for f in [24.0, 25.0, 30.0, 50.0, 60.0] {
                    if ui.selectable_label(c.fps == f, format!("{f} i/s")).clicked() {
                        c.fps = f;
                        changed = true;
                    }
                }
            });
            let before = c.duration;
            if ui.add(egui::DragValue::new(&mut c.duration).speed(0.05).range(0.5..=600.0).max_decimals(2).prefix(crate::i18n::t("durée ")).suffix(" s")).changed() {
                // Les calques qui allaient jusqu'au bout suivent
                for l in &mut c.layers {
                    if (l.end - before).abs() < 1e-3 || l.end > c.duration {
                        l.end = c.duration;
                    }
                }
                changed = true;
            }
        });
        section(ui, "Fond");
        changed |= ui.color_edit_button_srgb(&mut c.bg).changed();
        section(ui, "Musique");
        match c.music.clone() {
            Some(m) => {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{}  {}", l::MUSIC, m.rsplit('/').next().unwrap_or(&m))).size(12.0));
                    if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::X).size(13.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("retirer")).clicked() {
                        c.music = None;
                        changed = true;
                    }
                });
            }
            None => {
                if ui.button(crate::i18n::t("Choisir un son…")).clicked() {
                    if let Some(f) = rfd::FileDialog::new().add_filter("son", &["wav", "mp3", "ogg", "flac", "m4a", "aac"]).pick_file() {
                        match self.vault_path(&f) {
                            Ok(rel) => {
                                self.compo.music = Some(rel);
                                changed = true;
                            }
                            Err(e) => self.message = Some(e),
                        }
                    }
                }
                ui.label(egui::RichText::new(crate::i18n::t("Jouée à l'export MP4.")).size(11.0).color(p.muted));
            }
        }
        changed
    }

    // ---------- Timeline ----------

    fn timeline(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let fps = self.compo.fps;
        let dur = self.compo.duration;
        // Barre : lecture, temps, image
        let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
        ui.painter().hline(bar.x_range(), bar.bottom() - 0.5, Stroke::new(1.0, p.line));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink2(vec2(8.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        flat(&mut child);
        let icon = if self.playing.is_some() { l::PAUSE } else { l::PLAY };
        if child.button(egui::RichText::new(icon).size(14.0)).on_hover_text(crate::i18n::t("Lire (Espace)")).clicked() {
            self.toggle_play();
        }
        child.label(egui::RichText::new(tc(self.time, fps)).monospace().size(12.5));
        child.label(egui::RichText::new(format!("{} {} / {} · {} i/s", crate::i18n::t("image"), (self.time * fps).round() as u32, self.compo.frames(), fps)).size(11.5).color(p.muted));
        if self.compo.music.is_some() {
            let beats = self.wave().map(|w| w.beats.clone()).unwrap_or_default();
            let ok = self.sel.is_some() && !beats.is_empty();
            if child.add_enabled(ok, egui::Button::new(egui::RichText::new(l::MUSIC).size(14.0))).on_hover_text(crate::i18n::t("Les clés du calque choisi vont sur les temps forts les plus proches")).clicked() {
                if let Some(i) = self.sel {
                    let n = motion::snap_to_beats(&mut self.compo.layers[i], &beats);
                    self.message = Some(format!("{n} {}", crate::i18n::t_mid("clé(s) calée(s) sur le rythme")));
                    self.changed();
                }
            }
        }
        child.toggle_value(&mut self.onion, egui::RichText::new(l::LAYERS).size(14.0)).on_hover_text(crate::i18n::t("Pelure d'oignon : le calque choisi aux images d'avant (rouge) et d'après (vert)"));
        // Flou de mouvement : un clic l'allume, un clic droit règle l'obturateur
        let mut on = self.compo.blur.is_some();
        let r = child.toggle_value(&mut on, egui::RichText::new(l::WIND).size(14.0)).on_hover_text(crate::i18n::t("Flou de mouvement (comme After Effects) · clic droit : angle d'obturateur et qualité"));
        if r.changed() {
            self.compo.blur = on.then(motion::MotionBlur::default);
            self.changed();
        }
        r.context_menu(|ui| {
            let mut b = self.compo.blur.unwrap_or_default();
            let mut ch = false;
            ch |= ui.add(egui::Slider::new(&mut b.angle, 1.0..=720.0).suffix("°").text(crate::i18n::t("angle d'obturateur"))).changed();
            ch |= ui.add(egui::Slider::new(&mut b.samples, 2..=32).text(crate::i18n::t("échantillons"))).changed();
            ui.label(egui::RichText::new(crate::i18n::t("180° : le flou du cinéma ; plus d'échantillons : plus lisse, plus lent")).small());
            if ch {
                self.compo.blur = Some(b);
                self.changed();
            }
        });
        child.toggle_value(&mut self.curves_open, egui::RichText::new(l::SPLINE).size(14.0)).on_hover_text(crate::i18n::t("Éditeur de courbes : toutes les clés du calque, tangentes comprises (comme le Graph Editor de Maya)"));
        child.toggle_value(&mut self.anim_tools, egui::RichText::new(l::WAND_SPARKLES).size(14.0)).on_hover_text(crate::i18n::t("Outils d'animation : tweener, décaler les clés, poses, miroir…"));
        let wide = bar.width() > 900.0;
        child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !wide {
                return;
            }
            let cur = self.curve;
            for (pr, name) in PROPS.iter().rev() {
                if ui.selectable_label(cur == *pr, crate::i18n::t(name)).clicked() {
                    self.curve = *pr;
                }
            }
            ui.label(egui::RichText::new(crate::i18n::t("Courbe")).size(11.5).color(p.muted));
        });

        // Outils d'animation, comme dans la 3D (même logique, en secondes)
        if self.anim_tools {
            let (r2, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
            ui.painter().rect_filled(r2, 0.0, p.bg.gamma_multiply(0.6));
            ui.painter().hline(r2.x_range(), r2.bottom() - 0.5, Stroke::new(1.0, p.line));
            let mut tu = ui.new_child(egui::UiBuilder::new().max_rect(r2.shrink2(vec2(8.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
            flat(&mut tu);
            tu.spacing_mut().item_spacing.x = 4.0;
            use crate::animtools::motion as at;
            let t = self.time;
            let step = 1.0 / fps.max(1.0);
            let mut changed = false;
            let mut seek: Option<f32> = None;
            let sel = self.sel.filter(|&i| i < self.compo.layers.len());
            tu.add_enabled_ui(sel.is_some(), |ui| {
              egui::ScrollArea::horizontal().id_salt("motion-anim-tools").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
              ui.horizontal_centered(|ui| {
                let Some(i) = sel else { return };
                ui.spacing_mut().slider_width = 90.0;
                let mut apply = ui.add(egui::Slider::new(&mut self.tween_w, -0.5..=1.5).show_value(false)).on_hover_text(format!("{} · {:.0} %", crate::i18n::t("Entre la clé d'avant (0 %) et celle d'après (100 %) ; au-delà : dépassement"), self.tween_w * 100.0)).changed();
                for (lab, w) in [("0", 0.0), ("¼", 0.25), ("½", 0.5), ("¾", 0.75), ("1", 1.0)] {
                    if ui.small_button(lab).clicked() {
                        self.tween_w = w;
                        apply = true;
                    }
                }
                if apply {
                    if at::tween(&mut self.compo.layers[i], t, self.tween_w) {
                        changed = true;
                    } else {
                        self.message = Some(crate::i18n::t("Le tweener a besoin d'une clé avant et d'une clé après l'image courante"));
                    }
                }
                ui.separator();
                let typing = ui.ctx().egui_wants_keyboard_input();
                for (icon, d, tip) in [(l::CHEVRON_LEFT, -1.0, "Avancer d'une image les clés de l'image courante (Alt+←)"), (l::CHEVRON_RIGHT, 1.0, "Reculer d'une image les clés de l'image courante (Alt+→)")] {
                    let key = !typing && ui.input(|x| x.modifiers.alt && !x.modifiers.shift && x.key_pressed(if d < 0.0 { egui::Key::ArrowLeft } else { egui::Key::ArrowRight }));
                    if (ui.small_button(icon).on_hover_text(crate::i18n::t(tip)).clicked() || key) && at::nudge(&mut self.compo.layers[i], t, d * step) {
                        seek = Some(t + d * step);
                        changed = true;
                    }
                }
                ui.separator();
                for (lab, d, tip) in [("−1", -1.0, "Retirer une image après l'image courante : la suite se rapproche (Alt+Maj+←)"), ("+1", 1.0, "Ajouter une image après l'image courante : la suite s'éloigne (Alt+Maj+→)")] {
                    let key = !typing && ui.input(|x| x.modifiers.alt && x.modifiers.shift && x.key_pressed(if d < 0.0 { egui::Key::ArrowLeft } else { egui::Key::ArrowRight }));
                    if ui.small_button(lab).on_hover_text(crate::i18n::t(tip)).clicked() || key {
                        at::shift_after(&mut self.compo.layers[i], t, d * step);
                        changed = true;
                    }
                }
                ui.separator();
                if ui.small_button(l::COPY).on_hover_text(crate::i18n::t("Copier la pose")).clicked() {
                    self.pose_clip = Some(at::pose(&self.compo.layers[i], t));
                    self.message = Some(crate::i18n::t("Pose copiée"));
                }
                if ui.add_enabled(self.pose_clip.is_some(), egui::Button::new(l::CLIPBOARD_PASTE).small()).on_hover_text(crate::i18n::t("Coller la pose (et poser une clé)")).clicked() {
                    if let Some(pz) = self.pose_clip.clone() {
                        at::paste(&mut self.compo.layers[i], t, &pz);
                        changed = true;
                    }
                }
                ui.separator();
                if ui.small_button(l::MINIMIZE_2).on_hover_text(crate::i18n::t("Retire les clés que les voisines redonnent déjà (l'animation ne change pas)")).clicked() {
                    let n = at::reduce(&mut self.compo.layers[i]);
                    self.message = Some(format!("{} · {n}", crate::i18n::t("Clés retirées")));
                    changed |= n > 0;
                }
                let mut off = 0i32;
                if ui.add(egui::DragValue::new(&mut off).speed(0.15).prefix(format!("{} ", l::MOVE_HORIZONTAL)).suffix(crate::i18n::t_mid(" im."))).on_hover_text(crate::i18n::t("Glisser : décale toute l'animation des objets choisis")).changed() && off != 0 {
                    at::offset_all(&mut self.compo.layers[i], off as f32 * step);
                    changed = true;
                }
              });
              });
            });
            if let Some(s2) = seek {
                self.time = s2.clamp(0.0, dur);
            }
            if changed {
                self.changed();
            }
        }
        // Éditeur de courbes, dans sa fenêtre
        if self.curves_open {
            let mut open = true;
            let name = self.sel.and_then(|i| self.compo.layers.get(i)).map(|l| l.name.clone()).unwrap_or_default();
            egui::Window::new(format!("{} · {name}", crate::i18n::t("Éditeur de courbes"))).id(egui::Id::unique("motion-curves")).open(&mut open).resizable(true).default_size(vec2(780.0, 380.0)).show(ui.ctx(), |ui| {
                let Some(i) = self.sel.filter(|&i| i < self.compo.layers.len()) else {
                    ui.label(crate::i18n::t("Choisis un calque pour voir ses courbes."));
                    return;
                };
                let id = self.compo.layers[i].id.to_string();
                self.curve_ed.show_for(&id);
                let mut cv = motion::layer_curves(&self.compo.layers[i]);
                let lp = self.compo.layers[i].cycle;
                self.curve_ed.cycle = Some(lp);
                let out = self.curve_ed.ui(ui, &mut cv, self.time, (0.0, dur), fps);
                if out.changed {
                    motion::apply_curves(&mut self.compo.layers[i], &cv);
                    self.changed();
                }
                if let Some(new) = self.curve_ed.cycle.filter(|n| *n != lp) {
                    self.compo.layers[i].cycle = new;
                    self.changed();
                }
                if let Some(t) = out.seek {
                    self.time = t.clamp(0.0, dur);
                }
            });
            if !open {
                self.curves_open = false;
            }
        }
        let body = ui.available_rect_before_wrap();
        // Le panneau garde toute sa hauteur (sinon il se resserre sur son contenu)
        ui.allocate_rect(body, Sense::hover());
        let names_w = 250.0f32.min(body.width() * 0.4);
        let curve_w = if body.width() > 900.0 { 260.0 } else { 0.0 };
        let tracks = Rect::from_min_max(pos2(body.left() + names_w, body.top()), pos2(body.right() - curve_w, body.bottom()));
        let ruler = Rect::from_min_max(tracks.left_top(), pos2(tracks.right(), tracks.top() + 26.0));
        let x_of = |t: f32| tracks.left() + 10.0 + (t / dur.max(0.01)) * (tracks.width() - 20.0);
        let t_of = |x: f32| ((x - tracks.left() - 10.0) / (tracks.width() - 20.0) * dur).clamp(0.0, dur);
        let snap = |t: f32| (t * fps).round() / fps;
        let painter = ui.painter_at(body);
        painter.vline(tracks.left(), body.y_range(), Stroke::new(1.0, p.line));
        if curve_w > 0.0 {
            painter.vline(tracks.right(), body.y_range(), Stroke::new(1.0, p.line));
        }
        painter.hline(body.x_range(), ruler.bottom(), Stroke::new(1.0, p.line));
        painter.text(pos2(body.left() + 10.0, ruler.center().y), egui::Align2::LEFT_CENTER, crate::i18n::t("Calques"), egui::FontId::proportional(11.5), p.muted);
        // Règle : une graduation par seconde (ou plus espacée)
        let step = [0.25f32, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0].into_iter().find(|s| (x_of(*s) - x_of(0.0)) > 56.0).unwrap_or(60.0);
        let mut s = 0.0;
        while s <= dur + 1e-3 {
            let x = x_of(s);
            painter.vline(x, ruler.bottom() - 8.0..=ruler.bottom(), Stroke::new(1.0, p.line));
            painter.text(pos2(x + 3.0, ruler.top() + 8.0), egui::Align2::LEFT_CENTER, if step < 1.0 { format!("{s:.2}s") } else { format!("{s:.0}s") }, egui::FontId::monospace(10.5), p.muted);
            s += step;
        }
        let rr = ui.interact(ruler, ui.scope_id().with("motion-ruler"), Sense::click_and_drag());
        if rr.drag_started() || rr.clicked() {
            self.tl_drag = Some(TlDrag::Scrub);
        }

        // Rangées : un calque, puis ses propriétés quand il est choisi
        let row_h = 28.0;
        let sub_h = 22.0;
        // Les rangées défilent sous la règle (molette)
        let rows_clip = Rect::from_min_max(pos2(body.left(), ruler.bottom() + 0.5), pos2(tracks.right(), body.bottom()));
        let head_painter = painter.clone();
        let painter = ui.painter_at(rows_clip);
        let top0 = ruler.bottom() - self.tl_scroll;
        let mut y = top0;
        let t_now = self.time;
        let mut clicked_layer = None;
        let mut pending_ease: Option<(usize, Prop, f32, motion::Ease)> = None;
        let mut toggles: Vec<(usize, Prop)> = vec![];
        let mut eye = None;
        let n = self.compo.layers.len();
        for i in 0..n {
            let lay = &self.compo.layers[i];
            let sel = self.sel == Some(i);
            let r = Rect::from_min_max(pos2(body.left(), y), pos2(tracks.right(), y + row_h));
            if r.top() > body.bottom() {
                y += row_h + if sel { sub_h * PROPS.len() as f32 } else { 0.0 };
                continue;
            }
            if sel {
                painter.rect_filled(Rect::from_min_max(r.min, pos2(tracks.left(), r.bottom())), 0.0, p.selection);
                painter.rect_filled(Rect::from_min_max(pos2(tracks.left(), r.top()), r.max), 0.0, p.raised);
            }
            painter.hline(body.x_range(), r.bottom() - 0.5, Stroke::new(1.0, p.line.gamma_multiply(0.6)));
            let c = layer_color(&lay.kind);
            painter.rect_filled(Rect::from_min_size(pos2(r.left() + 8.0, r.center().y - 8.0), vec2(3.0, 16.0)), 0.0, c);
            // Calque qui sert de cache au calque d'en dessous : son icône devient celle du cache ; le calque caché a une flèche
            let is_matte = self.compo.layers.get(i + 1).is_some_and(|n| n.matte.is_some());
            let icon = if is_matte { l::SQUARE_DASHED } else if lay.matte.is_some() { l::CORNER_LEFT_UP } else { layer_icon(&lay.kind) };
            painter.text(pos2(r.left() + 20.0, r.center().y), egui::Align2::LEFT_CENTER, icon, egui::FontId::proportional(13.0), if is_matte || lay.matte.is_some() { p.accent } else { p.muted });
            let name_r = Rect::from_min_max(pos2(r.left() + 40.0, r.top()), pos2(tracks.left() - 30.0, r.bottom()));
            painter.with_clip_rect(name_r).text(name_r.left_center(), egui::Align2::LEFT_CENTER, &lay.name, egui::FontId::proportional(12.5), if lay.visible { p.text } else { p.muted });
            let eye_r = Rect::from_center_size(pos2(tracks.left() - 16.0, r.center().y), vec2(20.0, 20.0));
            let er = ui.interact(eye_r.intersect(rows_clip), ui.scope_id().with(("motion-eye", i)), Sense::click());
            painter.text(eye_r.center(), egui::Align2::CENTER_CENTER, if lay.visible { l::EYE } else { l::EYE_OFF }, egui::FontId::proportional(13.0), if er.hovered() { p.text } else { p.muted });
            if er.clicked() {
                eye = Some(i);
            }
            let nr = ui.interact(Rect::from_min_max(r.min, pos2(tracks.left() - 30.0, r.bottom())).intersect(rows_clip), ui.scope_id().with(("motion-name", i)), Sense::click());
            if nr.clicked() {
                clicked_layer = Some(i);
            }
            // Barre du calque et ses clés
            let bar_r = Rect::from_min_max(pos2(x_of(lay.start), r.top() + 6.0), pos2(x_of(lay.end), r.bottom() - 6.0));
            painter.rect_filled(bar_r, 3.0, c.gamma_multiply(if sel { 0.45 } else { 0.3 }));
            for kt in lay.key_times() {
                let ease = PROPS.iter().find_map(|(pr, _)| lay.track(*pr).key_at(kt).map(|k| lay.track(*pr).keys[k].ease));
                let here = (kt - t_now).abs() < motion::KEY_EPS;
                crate::keys::draw(&painter, pos2(x_of(kt), r.center().y), 4.5, ease, if sel { p.text } else { c }, true, here);
            }
            let br = ui.interact(bar_r.expand2(vec2(3.0, 0.0)).intersect(rows_clip), ui.scope_id().with(("motion-bar", i)), Sense::click_and_drag());
            if let Some(h) = br.hover_pos() {
                let edge = (h.x - bar_r.left()).abs() < 6.0 || (h.x - bar_r.right()).abs() < 6.0;
                if edge {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
            }
            if br.clicked() {
                clicked_layer = Some(i);
                // Clic sur un losange : on va à cette clé
                if let Some(h) = br.interact_pointer_pos() {
                    if let Some(kt) = lay.key_times().into_iter().find(|kt| (x_of(*kt) - h.x).abs() < 6.0) {
                        self.time = kt;
                        self.playing = None;
                    }
                }
            }
            if br.drag_started() {
                if let Some(h) = br.interact_pointer_pos() {
                    clicked_layer = Some(i);
                    if let Some(kt) = lay.key_times().into_iter().find(|kt| (x_of(*kt) - h.x).abs() < 6.0) {
                        self.tl_drag = Some(TlDrag::Keys { layer: i, t0: kt, prop: None });
                    } else {
                        let part = if (h.x - bar_r.left()).abs() < 6.0 {
                            1
                        } else if (h.x - bar_r.right()).abs() < 6.0 {
                            2
                        } else {
                            0
                        };
                        self.tl_drag = Some(TlDrag::Bar { layer: i, grab: t_of(h.x), part, start: lay.start, end: lay.end });
                    }
                }
            }
            y += row_h;
            if sel {
                for (pr, label) in PROPS {
                    let r = Rect::from_min_max(pos2(body.left(), y), pos2(tracks.right(), y + sub_h));
                    if r.top() > body.bottom() {
                        y += sub_h;
                        continue;
                    }
                    let lay = &self.compo.layers[i];
                    let tr = lay.track(pr);
                    let at = tr.key_at(t_now).is_some();
                    let dr = Rect::from_center_size(pos2(r.left() + 30.0, r.center().y), vec2(16.0, 16.0));
                    let dresp = ui.interact(dr.intersect(rows_clip), ui.scope_id().with(("motion-dia", i, label)), Sense::click()).on_hover_text(crate::i18n::t("Poser ou retirer une clé à cet instant"));
                    diamond(&painter, dr.center(), 4.0, if at { Some(p.accent) } else { None }, if tr.animated() { p.accent } else { p.muted });
                    if dresp.clicked() {
                        toggles.push((i, pr));
                    }
                    painter.text(pos2(r.left() + 44.0, r.center().y), egui::Align2::LEFT_CENTER, crate::i18n::t(label), egui::FontId::proportional(12.0), if tr.animated() { p.text } else { p.muted });
                    let v = tr.at(t_now);
                    let val = match pr {
                        Prop::Pos => format!("{:.0}, {:.0}", v[0], v[1]),
                        Prop::Scale => format!("{:.0} %", v[0]),
                        Prop::Rot => format!("{:.1}°", v[0]),
                        Prop::Opacity => format!("{:.0} %", v[0]),
                    };
                    if names_w >= 200.0 {
                        painter.text(pos2(tracks.left() - 12.0, r.center().y), egui::Align2::RIGHT_CENTER, val, egui::FontId::monospace(11.0), p.muted);
                    }
                    painter.hline(body.x_range(), r.bottom() - 0.5, Stroke::new(1.0, p.line.gamma_multiply(0.35)));
                    for k in &tr.keys {
                        let chosen = self.sel_key.is_some_and(|s| s.0 == i && s.1 == pr && (s.2 - k.t).abs() < motion::KEY_EPS);
                        crate::keys::draw(&painter, pos2(x_of(k.t), r.center().y), 4.0, Some(k.ease), p.accent, true, chosen);
                    }
                    let kr = ui.interact(Rect::from_min_max(pos2(tracks.left(), r.top()), r.max).intersect(rows_clip), ui.scope_id().with(("motion-keys", i, label)), Sense::click_and_drag());
                    let hit = |h: Pos2| tr.keys.iter().map(|k| k.t).find(|kt| (x_of(*kt) - h.x).abs() < 6.0);
                    if kr.clicked() {
                        if let Some(h) = kr.interact_pointer_pos() {
                            match hit(h) {
                                Some(kt) => {
                                    self.sel_key = Some((i, pr, kt));
                                    self.curve = pr;
                                    self.time = kt;
                                    self.playing = None;
                                }
                                None => self.sel_key = None,
                            }
                        }
                    }
                    // Clic droit sur une clé : son lissage (le même menu partout dans l'app)
                    if kr.secondary_clicked() {
                        if let Some(kt) = kr.interact_pointer_pos().and_then(hit) {
                            self.ctx_key = Some((i, pr, kt));
                            self.sel_key = Some((i, pr, kt));
                        }
                    }
                    let cur = self.ctx_key.filter(|c| c.0 == i && c.1 == pr).and_then(|c| tr.key_at(c.2).map(|k| tr.keys[k].ease));
                    let mut picked = None;
                    if cur.is_some() {
                        kr.context_menu(|ui| {
                            if let Some(Some(e)) = crate::keys::menu(ui, cur, false) {
                                picked = Some(e);
                            }
                        });
                    }
                    if let (Some(e), Some((li, lp, kt))) = (picked, self.ctx_key) {
                        pending_ease = Some((li, lp, kt, e));
                    }
                    if kr.drag_started() {
                        if let Some(kt) = kr.interact_pointer_pos().and_then(hit) {
                            self.sel_key = Some((i, pr, kt));
                            self.tl_drag = Some(TlDrag::Keys { layer: i, t0: kt, prop: Some(pr) });
                        } else {
                            self.tl_drag = Some(TlDrag::Scrub);
                        }
                    }
                    y += sub_h;
                }
            }
        }
        // La musique, sous les calques : forme d'onde et temps forts
        if let Some(m) = self.compo.music.clone() {
            let r = Rect::from_min_max(pos2(body.left(), y), pos2(tracks.right(), y + 34.0));
            let c = crate::theme::ws_color("video").unwrap_or(p.success);
            painter.rect_filled(Rect::from_min_size(pos2(r.left() + 8.0, r.center().y - 8.0), vec2(3.0, 16.0)), 0.0, c);
            painter.text(pos2(r.left() + 20.0, r.center().y), egui::Align2::LEFT_CENTER, l::MUSIC, egui::FontId::proportional(13.0), p.muted);
            let name_r = Rect::from_min_max(pos2(r.left() + 40.0, r.top()), pos2(tracks.left() - 8.0, r.bottom()));
            painter.with_clip_rect(name_r.intersect(rows_clip)).text(name_r.left_center(), egui::Align2::LEFT_CENTER, m.rsplit('/').next().unwrap_or(&m), egui::FontId::proportional(12.5), p.text);
            if let Some(w) = self.wave() {
                let mid = r.center().y;
                let x0 = x_of(0.0).max(tracks.left());
                let mut x = x0;
                while x < x_of(dur) {
                    let k = (t_of(x) * motion::WAVE_RATE) as usize;
                    let v = w.env.get(k..(k + 3).min(w.env.len())).map(|s| s.iter().cloned().fold(0.0, f32::max)).unwrap_or(0.0);
                    painter.vline(x, (mid - v * 12.0)..=(mid + v * 12.0).max(mid + 0.5), Stroke::new(1.0, c.gamma_multiply(0.7)));
                    x += 2.0;
                }
                for b in w.beats.iter().filter(|b| **b <= dur) {
                    let bx = x_of(*b);
                    painter.add(egui::Shape::convex_polygon(vec![pos2(bx - 3.5, r.top() + 2.0), pos2(bx + 3.5, r.top() + 2.0), pos2(bx, r.top() + 7.0)], p.warning, Stroke::NONE));
                }
            } else {
                painter.text(pos2(tracks.left() + 12.0, r.center().y), egui::Align2::LEFT_CENTER, crate::i18n::t("lecture du son…"), egui::FontId::proportional(11.5), p.muted);
            }
            painter.hline(body.x_range(), r.bottom() - 0.5, Stroke::new(1.0, p.line.gamma_multiply(0.6)));
            y += 34.0;
        }
        // Défilement : la molette sur les rangées
        let content = y - top0;
        let room = rows_clip.height();
        if ui.rect_contains_pointer(rows_clip) {
            let d = ui.input(|i| if i.modifiers.command { 0.0 } else { i.smooth_scroll_delta.y });
            self.tl_scroll -= d;
        }
        self.tl_scroll = self.tl_scroll.clamp(0.0, (content - room + 12.0).max(0.0));
        if content > room {
            let bar_h = (room * room / content).max(24.0);
            let by = rows_clip.top() + (room - bar_h) * self.tl_scroll / (content - room + 12.0).max(1.0);
            painter.rect_filled(Rect::from_min_size(pos2(tracks.right() - 5.0, by), vec2(3.0, bar_h)), 2.0, p.line);
        }
        if n == 0 {
            painter.text(pos2(tracks.center().x, ruler.bottom() + 30.0), egui::Align2::CENTER_CENTER, crate::i18n::t("Pas encore de calque : ajoute un texte ou une forme."), egui::FontId::proportional(12.0), p.muted);
        }
        // Clic dans le vide des pistes : la tête de lecture y va
        let empty = Rect::from_min_max(pos2(tracks.left(), y.max(rows_clip.top())), tracks.max);
        if empty.height() > 4.0 {
            let er = ui.interact(empty, ui.scope_id().with("motion-empty"), Sense::click_and_drag());
            if er.drag_started() || er.clicked() {
                self.tl_drag = Some(TlDrag::Scrub);
                if er.clicked() {
                    self.sel = None;
                    self.sel_key = None;
                }
            }
        }
        // Glisser
        let mut commit = false;
        let beats: Vec<f32> = self.wave().map(|w| w.beats.clone()).unwrap_or_default();
        if let (Some(d), Some(m)) = (self.tl_drag.take(), ui.input(|i| i.pointer.interact_pos())) {
            // Les clés et la tête de lecture s'aimantent aux temps forts de la musique
            let t = match beats.iter().find(|b| (x_of(**b) - m.x).abs() < 6.0) {
                Some(b) if !ui.input(|i| i.modifiers.alt) => *b,
                _ => snap(t_of(m.x)),
            };
            let mut next = None;
            match &d {
                TlDrag::Scrub => {
                    self.time = t;
                    self.playing = None;
                }
                TlDrag::Keys { layer, t0, prop } => {
                    let (li, t0, prop) = (*layer, *t0, *prop);
                    if (t - t0).abs() > 1e-4 {
                        if let Some(lay) = self.compo.layers.get_mut(li) {
                            for (pr, _) in PROPS {
                                if prop.is_some_and(|q| q != pr) {
                                    continue;
                                }
                                let tr = lay.track_mut(pr);
                                if let Some(k) = tr.key_at(t0) {
                                    if tr.key_at(t).is_none() {
                                        tr.keys[k].t = t;
                                        tr.sort();
                                    }
                                }
                            }
                        }
                        if let Some(sk) = &mut self.sel_key {
                            if sk.0 == li && (sk.2 - t0).abs() < motion::KEY_EPS {
                                sk.2 = t;
                            }
                        }
                        next = Some(TlDrag::Keys { layer: li, t0: t, prop });
                        self.time = t;
                        self.rev += 1;
                    }
                }
                TlDrag::Bar { layer, grab, part, start, end } => {
                    let (li, dt, part, start, end) = (*layer, t - snap(*grab), *part, *start, *end);
                    if let Some(lay) = self.compo.layers.get_mut(li) {
                        match part {
                            1 => lay.start = (start + dt).clamp(0.0, lay.end - 1.0 / fps),
                            2 => lay.end = (end + dt).clamp(lay.start + 1.0 / fps, dur),
                            _ => {
                                // Le calque glisse avec toutes ses clés
                                let dt = dt.clamp(-start, dur - end);
                                let shift = start + dt - lay.start;
                                lay.start = start + dt;
                                lay.end = end + dt;
                                for (pr, _) in PROPS {
                                    for k in &mut lay.track_mut(pr).keys {
                                        k.t += shift;
                                    }
                                }
                            }
                        }
                        self.rev += 1;
                    }
                }
            }
            if !ui.input(|i| i.pointer.any_down()) {
                commit = !matches!(d, TlDrag::Scrub);
            } else {
                self.tl_drag = Some(next.unwrap_or(d));
            }
        }
        if commit {
            self.changed();
        }
        if let Some(i) = clicked_layer {
            if self.sel != Some(i) {
                self.sel_key = None;
            }
            self.sel = Some(i);
        }
        if let Some(i) = eye {
            self.compo.layers[i].visible ^= true;
            self.changed();
        }
        if let Some((li, pr, kt, e)) = pending_ease {
            if let Some(tr) = self.compo.layers.get_mut(li).map(|l| l.track_mut(pr)) {
                if let Some(k) = tr.key_at(kt) {
                    tr.keys[k].ease = e;
                }
            }
            self.ctx_key = None;
            self.changed();
        }
        if !toggles.is_empty() {
            for (i, pr) in toggles {
                let t = self.time;
                self.compo.layers[i].track_mut(pr).toggle_key(t);
            }
            self.changed();
        }
        // Tête de lecture
        let painter = head_painter;
        let x = x_of(self.time);
        painter.vline(x, ruler.top()..=body.bottom(), Stroke::new(1.0, p.accent));
        painter.add(egui::Shape::convex_polygon(vec![pos2(x - 5.0, ruler.top() + 14.0), pos2(x + 5.0, ruler.top() + 14.0), pos2(x, ruler.top() + 20.0)], p.accent, Stroke::NONE));

        // Courbe de la propriété choisie
        if curve_w > 0.0 {
            let cr = Rect::from_min_max(pos2(tracks.right() + 10.0, body.top() + 8.0), pos2(body.right() - 10.0, body.bottom() - 10.0));
            let pr = self.curve;
            let label = PROPS.iter().find(|x| x.0 == pr).map(|x| x.1).unwrap_or("");
            let lay = self.sel.and_then(|i| self.compo.layers.get(i));
            let ease = lay.and_then(|l| self.sel_key.filter(|k| k.1 == pr).and_then(|k| l.track(pr).key_at(k.2).map(|j| l.track(pr).keys[j].ease)));
            painter.text(cr.left_top(), egui::Align2::LEFT_TOP, format!("{} · {}", crate::i18n::t("Courbe"), crate::i18n::t(label).to_lowercase()), egui::FontId::proportional(11.5), p.muted);
            if let Some(e) = ease {
                painter.text(cr.right_top(), egui::Align2::RIGHT_TOP, crate::i18n::t(motion::EASES.iter().find(|x| x.0 == e).map(|x| x.1).unwrap_or("")).to_lowercase(), egui::FontId::proportional(11.5), p.muted);
            }
            let g = Rect::from_min_max(pos2(cr.left(), cr.top() + 20.0), cr.max);
            painter.rect(g, 2.0, p.code_bg, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
            if let Some(tr) = lay.map(|l| l.track(pr)).filter(|t| t.animated()) {
                // Valeur (x ou seule valeur) au fil de la composition, mise à l'échelle de la case
                let n = 120;
                let vals: Vec<f32> = (0..=n).map(|k| tr.at(dur * k as f32 / n as f32)[0]).collect();
                let (lo, hi) = vals.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
                let span = (hi - lo).max(1e-3);
                let inner = g.shrink(8.0);
                let pt = |k: usize, v: f32| pos2(inner.left() + inner.width() * k as f32 / n as f32, inner.bottom() - (v - lo) / span * inner.height());
                let pts: Vec<Pos2> = vals.iter().enumerate().map(|(k, v)| pt(k, *v)).collect();
                painter.add(egui::Shape::line(pts, Stroke::new(1.5, p.accent)));
                for k in &tr.keys {
                    let kx = inner.left() + inner.width() * k.t / dur.max(0.01);
                    let ky = inner.bottom() - (k.v[0] - lo) / span * inner.height();
                    diamond(&painter, pos2(kx, ky), 3.5, Some(p.warning), p.code_bg);
                }
                let px = inner.left() + inner.width() * self.time / dur.max(0.01);
                painter.vline(px, inner.y_range(), Stroke::new(1.0, p.accent.gamma_multiply(0.6)));
            } else {
                painter.text(g.center(), egui::Align2::CENTER_CENTER, crate::i18n::t("Pas de clé sur cette propriété"), egui::FontId::proportional(11.5), p.muted);
            }
        }
    }
}
