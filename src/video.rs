//! Vidéos : lecture image par image, cadence réglable, lecture « en 1s / 2s / 4s / 8s »
//! (comme en animation : chaque image tenue N fois), pelures d'oignon, dessin par-dessus,
//! et export en séquence d'images ou en vidéo. S'appuie sur ffmpeg (installé sans admin).
use crate::ffmpeg;
use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Key, Pos2, Rect, RichText, Sense, Shape, Stroke, TextureHandle};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub const EXT: [&str; 7] = ["mp4", "mov", "webm", "mkv", "avi", "m4v", "mpg"];
/// Au-delà, l'aperçu est échantillonné (les exports gardent toutes les images).
const MAX_PREVIEW_FRAMES: f32 = 4000.0;

pub fn is_video(rel: &str) -> bool {
    let e = rel.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    EXT.contains(&e.as_str())
}

#[derive(Clone, Default, Debug)]
pub struct Info {
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub duration: f32,
}

pub fn probe(path: &Path) -> Result<Info, String> {
    let ffprobe = ffmpeg::find("ffprobe").ok_or("ffprobe introuvable")?;
    let out = ffmpeg::command(ffprobe)
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,r_frame_rate,avg_frame_rate:format=duration", "-of", "json"])
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|_| "Vidéo illisible.".to_string())?;
    let s = &v["streams"][0];
    let rate = |k: &str| -> Option<f32> {
        let r = s[k].as_str()?;
        let (a, b) = r.split_once('/')?;
        let (a, b): (f32, f32) = (a.parse().ok()?, b.parse().ok()?);
        (b > 0.0 && a > 0.0).then_some(a / b)
    };
    Ok(Info {
        width: s["width"].as_u64().unwrap_or(0) as u32,
        height: s["height"].as_u64().unwrap_or(0) as u32,
        fps: rate("avg_frame_rate").or_else(|| rate("r_frame_rate")).unwrap_or(24.0).min(240.0),
        duration: v["format"]["duration"].as_str().and_then(|d| d.parse().ok()).unwrap_or(0.0),
    })
}

/// Dossier de cache des images d'aperçu (hors du coffre).
fn cache_dir(path: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok()).hash(&mut h);
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("aryadne").join("video").join(format!("{:016x}", h.finish()))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Line {
    pub pts: Vec<[f32; 2]>,
    pub color: [u8; 4],
    pub width: f32,
}

#[derive(Default)]
struct Extract {
    frames: usize,
    done: bool,
    error: Option<String>,
}

#[derive(Clone, Default)]
pub struct ExportJob {
    pub done: usize,
    pub total: usize,
    pub finished: bool,
    pub error: Option<String>,
    pub out: Option<PathBuf>,
}

pub struct Export {
    pub fps: f32,
    pub every: usize,
    pub format: &'static str,
    pub height: u32,
    pub with_drawings: bool,
    pub job: Option<Arc<Mutex<ExportJob>>>,
}

pub struct Viewer {
    path: PathBuf,
    pub info: Result<Info, String>,
    /// fps de l'aperçu (= fps source sauf vidéos très longues)
    preview_fps: f32,
    dir: PathBuf,
    extract: Arc<Mutex<Extract>>,
    tex: HashMap<usize, TextureHandle>,
    lru: VecDeque<usize>,
    pub cur: usize,
    playing: bool,
    clock: Option<(Instant, usize)>,
    pub play_fps: f32,
    pub on: usize,
    pub onion: bool,
    /// Pelures : nombre d'images avant / après, opacité, teinte rouge-vert ou naturelle.
    pub onion_before: usize,
    pub onion_after: usize,
    pub onion_opacity: f32,
    pub onion_tint: bool,
    pub onion_drawings: bool,
    /// Repères : tiers, centre, zones de sécurité, cache de format.
    pub guide_thirds: bool,
    pub guide_center: bool,
    pub guide_safe: bool,
    pub guide_ratio: f32,
    /// Plage de boucle (images) et lecture aller-retour
    pub loop_in: Option<usize>,
    pub loop_out: Option<usize>,
    pub pingpong: bool,
    pub draw: bool,
    pub pen: Color32,
    pub pen_width: f32,
    pub lines: BTreeMap<usize, Vec<Line>>,
    current: Vec<[f32; 2]>,
    notes_path: PathBuf,
    pub export: Option<Export>,
    /// Bande d'images sous la vidéo (façon Syncsketch)
    pub show_strip: bool,
    pub strip_h: f32,
    strip_tex: HashMap<usize, TextureHandle>,
    strip_lru: VecDeque<usize>,
    strip_follow: bool,
    pub zoom: crate::zoomview::Zoom,
}

impl Viewer {
    /// `notes_path` : fichier JSON où sont gardés les dessins (dans .app/ du coffre).
    pub fn open(path: PathBuf, notes_path: PathBuf, ctx: &egui::Context) -> Self {
        let info = if ffmpeg::find("ffprobe").is_some() { probe(&path) } else { Err("ffmpeg n'est pas installé.".into()) };
        let fps = info.as_ref().map(|i| i.fps).unwrap_or(24.0);
        let duration = info.as_ref().map(|i| i.duration).unwrap_or(0.0);
        let preview_fps = if duration * fps > MAX_PREVIEW_FRAMES { (MAX_PREVIEW_FRAMES / duration.max(1.0)).max(1.0) } else { fps };
        let lines = std::fs::read_to_string(&notes_path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let mut v = Self {
            dir: cache_dir(&path),
            path,
            info,
            preview_fps,
            extract: Arc::new(Mutex::new(Extract::default())),
            tex: HashMap::new(),
            lru: VecDeque::new(),
            cur: 0,
            playing: false,
            clock: None,
            play_fps: fps,
            on: 1,
            onion: false,
            onion_before: 2,
            onion_after: 1,
            onion_opacity: 0.45,
            onion_tint: true,
            onion_drawings: true,
            guide_thirds: false,
            guide_center: false,
            guide_safe: false,
            guide_ratio: 0.0,
            loop_in: None,
            loop_out: None,
            pingpong: false,
            draw: false,
            pen: Color32::from_rgb(0xff, 0x4f, 0x1f),
            pen_width: 3.0,
            lines,
            current: vec![],
            notes_path,
            export: None,
            show_strip: true,
            strip_h: 54.0,
            strip_tex: HashMap::new(),
            strip_lru: VecDeque::new(),
            strip_follow: true,
            zoom: Default::default(),
        };
        if v.info.is_ok() {
            v.start_extract(ctx.clone());
        }
        v
    }

    fn start_extract(&mut self, ctx: egui::Context) {
        let (src, dir, fps, ex) = (self.path.clone(), self.dir.clone(), self.preview_fps, self.extract.clone());
        std::thread::spawn(move || {
            let count = |d: &Path| std::fs::read_dir(d).map(|r| r.filter(|e| e.as_ref().is_ok_and(|e| e.path().extension().is_some_and(|x| x == "jpg"))).count()).unwrap_or(0);
            // Déjà extrait lors d'une ouverture précédente ?
            if dir.join("done").exists() {
                if let Ok(mut e) = ex.lock() {
                    e.frames = count(&dir);
                    e.done = true;
                }
                ctx.request_repaint();
                return;
            }
            let _ = std::fs::remove_dir_all(&dir);
            if let Err(err) = std::fs::create_dir_all(&dir) {
                ex.lock().unwrap().error = Some(err.to_string());
                return;
            }
            let Some(ff) = ffmpeg::find("ffmpeg") else { return };
            let child = ffmpeg::command(ff)
                .args(["-v", "error", "-i"])
                .arg(&src)
                .args(["-vf", &format!("fps={fps},scale=-2:'min(720,ih)'"), "-q:v", "3", "-progress", "pipe:1", "-nostats"])
                .arg(dir.join("%06d.jpg"))
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();
            let mut child = match child {
                Ok(c) => c,
                Err(e) => {
                    ex.lock().unwrap().error = Some(e.to_string());
                    return;
                }
            };
            if let Some(out) = child.stdout.take() {
                for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                    if let Some(n) = line.strip_prefix("frame=").and_then(|n| n.trim().parse::<usize>().ok()) {
                        ex.lock().unwrap().frames = n;
                        ctx.request_repaint();
                    }
                }
            }
            let ok = child.wait().map(|s| s.success()).unwrap_or(false);
            let mut e = ex.lock().unwrap();
            e.frames = count(&dir);
            e.done = true;
            if ok {
                let _ = std::fs::write(dir.join("done"), "");
            } else if e.frames == 0 {
                let mut msg = String::new();
                if let Some(mut err) = child.stderr.take() {
                    let _ = std::io::Read::read_to_string(&mut err, &mut msg);
                }
                e.error = Some(format!("Extraction impossible. {}", msg.lines().last().unwrap_or("")));
            }
            ctx.request_repaint();
        });
    }

    fn frame_tex(&mut self, ctx: &egui::Context, i: usize) -> Option<TextureHandle> {
        if let Some(t) = self.tex.get(&i) {
            return Some(t.clone());
        }
        let img = image::open(self.dir.join(format!("{:06}.jpg", i + 1))).ok()?.to_rgba8();
        let t = ctx.load_texture(format!("vid{i}"), egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()), egui::TextureOptions::LINEAR);
        self.tex.insert(i, t.clone());
        self.lru.push_back(i);
        while self.lru.len() > 160 {
            if let Some(old) = self.lru.pop_front() {
                self.tex.remove(&old);
            }
        }
        Some(t)
    }

    /// Petite image pour la bande (chargée à la demande, quelques-unes par image affichée).
    fn strip_thumb(&mut self, ctx: &egui::Context, i: usize, h: u32, budget: &mut usize) -> Option<TextureHandle> {
        if let Some(t) = self.strip_tex.get(&i) {
            return Some(t.clone());
        }
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        let img = image::open(self.dir.join(format!("{:06}.jpg", i + 1))).ok()?;
        let w = ((img.width() as f32 / img.height().max(1) as f32) * h as f32).max(1.0) as u32;
        let img = img.resize_exact(w, h, image::imageops::FilterType::Triangle).to_rgba8();
        let t = ctx.load_texture(format!("strip{i}"), egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()), egui::TextureOptions::LINEAR);
        self.strip_tex.insert(i, t.clone());
        self.strip_lru.push_back(i);
        while self.strip_lru.len() > 600 {
            if let Some(old) = self.strip_lru.pop_front() {
                self.strip_tex.remove(&old);
            }
        }
        Some(t)
    }

    /// Bande d'images : toutes les images côte à côte, clic ou glisser pour aller.
    fn filmstrip(&mut self, ui: &mut egui::Ui, frames: usize, held: usize, aspect: f32) {
        let p = theme::pal();
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            ui.toggle_value(&mut self.show_strip, RichText::new("bande d'images").size(11.0));
            if self.show_strip {
                ui.spacing_mut().slider_width = 90.0;
                ui.add(egui::Slider::new(&mut self.strip_h, 28.0..=140.0).show_value(false).text(RichText::new("taille").size(10.5)));
                ui.checkbox(&mut self.strip_follow, RichText::new("suivre la lecture").size(11.0));
            }
        });
        if !self.show_strip || frames == 0 {
            return;
        }
        let h = self.strip_h;
        let w = (h * aspect).clamp(16.0, 400.0);
        let total_w = frames as f32 * (w + 2.0);
        let mut budget = 8usize;
        let id = ui.make_persistent_id(("filmstrip", &self.path));
        let mut sa = egui::ScrollArea::horizontal().id_salt(id).max_height(h + 24.0).auto_shrink([false, true]);
        if self.strip_follow {
            // Garde l'image courante visible (centrée)
            let off: f32 = ui.data(|d| d.get_temp(id.with("off")).unwrap_or(0.0));
            let view_w = ui.available_width();
            let x = held as f32 * (w + 2.0);
            if x < off || x + w > off + view_w {
                sa = sa.horizontal_scroll_offset((x - view_w / 2.0).max(0.0));
            }
        }
        let out = sa.show_viewport(ui, |ui, vp| {
            let (rect, resp) = ui.allocate_exact_size(vec2(total_w, h + 18.0), Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            let first = (vp.left() / (w + 2.0)).floor().max(0.0) as usize;
            let last = ((vp.right() / (w + 2.0)).ceil() as usize + 1).min(frames);
            let loop_r = (self.loop_in.unwrap_or(0), self.loop_out.unwrap_or(frames.saturating_sub(1)));
            let has_loop = self.loop_in.is_some() || self.loop_out.is_some();
            for i in first..last {
                let r = Rect::from_min_size(pos2(rect.left() + i as f32 * (w + 2.0), rect.top()), vec2(w, h));
                match self.strip_thumb(&ctx, i, (h * 1.5) as u32, &mut budget) {
                    Some(t) => {
                        painter.image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        painter.rect_filled(r, 0.0, Color32::from_gray(45));
                    }
                }
                if has_loop && (i < loop_r.0 || i > loop_r.1) {
                    painter.rect_filled(r, 0.0, Color32::from_black_alpha(140));
                }
                if self.lines.contains_key(&i) {
                    painter.rect_filled(Rect::from_min_size(r.left_bottom() - vec2(0.0, 3.0), vec2(w, 3.0)), 0.0, p.accent);
                }
                if i == held {
                    painter.rect_stroke(r, 0.0, Stroke::new(2.5, p.accent), egui::StrokeKind::Inside);
                }
                if i % 5 == 0 || i == held || w > 60.0 {
                    let col = if i == held { p.accent } else { p.muted };
                    painter.text(pos2(r.left() + 2.0, r.bottom() + 2.0), egui::Align2::LEFT_TOP, (i + 1).to_string(), egui::FontId::monospace(10.0), col);
                }
            }
            if resp.clicked() || resp.dragged_by(egui::PointerButton::Primary) {
                if let Some(pp) = resp.interact_pointer_pos() {
                    self.cur = (((pp.x - rect.left()) / (w + 2.0)).floor().max(0.0) as usize).min(frames - 1);
                    self.playing = false;
                    self.strip_follow = false;
                }
            }
            if budget == 0 {
                ctx.request_repaint();
            }
        });
        ui.data_mut(|d| d.insert_temp(id.with("off"), out.state.clamped_offset().x));
        if self.playing {
            self.strip_follow = true;
        }
    }

    fn save_lines(&self) {
        if let Some(d) = self.notes_path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(&self.notes_path, serde_json::to_string(&self.lines).unwrap_or_default());
    }

    /// Image réellement affichée : en « 2s », chaque image est tenue deux fois.
    fn held(&self, i: usize) -> usize {
        i - i % self.on.max(1)
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let ctx = ui.ctx().clone();
        let info = match &self.info {
            Ok(i) => i.clone(),
            Err(e) => {
                ui.label(RichText::new(e).color(p.missing));
                return;
            }
        };
        let (frames, done, err) = {
            let e = self.extract.lock().unwrap();
            (e.frames, e.done, e.error.clone())
        };
        if let Some(e) = err {
            ui.label(RichText::new(e).color(p.missing));
            return;
        }
        if !done {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        // Lecture
        if self.playing && frames > 0 {
            let (start, from) = *self.clock.get_or_insert((Instant::now(), self.cur));
            let step = (start.elapsed().as_secs_f32() * self.play_fps * (self.preview_fps / info.fps)) as usize;
            // Plage de boucle (ou toute la vidéo), aller-retour si demandé
            let a = self.loop_in.unwrap_or(0).min(frames - 1);
            let b = self.loop_out.unwrap_or(frames - 1).clamp(a, frames - 1);
            let len = b - a + 1;
            let from_off = from.clamp(a, b) - a;
            let t = from_off + step;
            self.cur = if self.pingpong && len > 1 {
                let period = 2 * (len - 1);
                let k = t % period;
                a + if k < len { k } else { period - k }
            } else {
                a + t % len
            };
            ctx.request_repaint();
        }

        // Barre d'outils
        ui.horizontal_wrapped(|ui| {
            if ui.button(if self.playing { "⏸ pause" } else { "▶ lecture" }).clicked() {
                self.playing = !self.playing;
                self.clock = None;
            }
            if ui.button("◀").on_hover_text("image précédente (←)").clicked() {
                self.cur = self.cur.saturating_sub(self.on);
                self.playing = false;
            }
            if ui.button("▶").on_hover_text("image suivante (→)").clicked() {
                self.cur = (self.cur + self.on).min(frames.saturating_sub(1));
                self.playing = false;
            }
            ui.separator();
            ui.label("cadence");
            let mut fps = self.play_fps;
            if ui.add(egui::DragValue::new(&mut fps).range(1.0..=120.0).speed(0.2).suffix(" i/s")).changed() {
                self.play_fps = fps;
                self.clock = None;
            }
            for f in [12.0, 24.0, 30.0] {
                if ui.small_button(format!("{f}")).clicked() {
                    self.play_fps = f;
                    self.clock = None;
                }
            }
            ui.separator();
            ui.label("en");
            for n in [1usize, 2, 3, 4] {
                if ui.selectable_label(self.on == n, format!("{n}s")).on_hover_text(format!("chaque image tenue {n} fois (animation « en {n} »)")).clicked() {
                    self.on = n;
                }
            }
            ui.add(egui::DragValue::new(&mut self.on).range(1..=240).speed(0.1).suffix("s")).on_hover_text("n'importe quelle valeur : glisse ou double-clique pour taper");
            ui.separator();
            if ui.small_button("[").on_hover_text("début de la boucle ici (I)").clicked() {
                self.loop_in = Some(self.cur);
                self.clock = None;
            }
            if ui.small_button("]").on_hover_text("fin de la boucle ici (O)").clicked() {
                self.loop_out = Some(self.cur);
                self.clock = None;
            }
            if (self.loop_in.is_some() || self.loop_out.is_some()) && ui.small_button("×").on_hover_text("retirer la boucle").clicked() {
                self.loop_in = None;
                self.loop_out = None;
            }
            if ui.toggle_value(&mut self.pingpong, "⇄").on_hover_text("aller-retour (ping-pong)").changed() {
                self.clock = None;
            }
            ui.separator();
            ui.checkbox(&mut self.onion, "pelures");
            let stay = egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
            egui::containers::menu::MenuButton::new("▾").config(stay.clone()).ui(ui, |ui| {
                ui.set_min_width(230.0);
                ui.label(RichText::new("pelures d'oignon").color(p.muted).size(11.0));
                ui.add(egui::Slider::new(&mut self.onion_before, 0..=6).text("images avant"));
                ui.add(egui::Slider::new(&mut self.onion_after, 0..=6).text("images après"));
                ui.add(egui::Slider::new(&mut self.onion_opacity, 0.05..=1.0).text("opacité"));
                ui.checkbox(&mut self.onion_tint, "teinter (avant rouge, après vert)");
                ui.checkbox(&mut self.onion_drawings, "inclure les dessins");
                ui.label(RichText::new("L'écart suit le réglage « en 1s / 2s / 4s ».").color(p.muted).size(10.5));
            });
            egui::containers::menu::MenuButton::new("repères ▾").config(stay).ui(ui, |ui| {
                ui.set_min_width(200.0);
                ui.checkbox(&mut self.guide_thirds, "règle des tiers");
                ui.checkbox(&mut self.guide_center, "centre");
                ui.checkbox(&mut self.guide_safe, "zones de sécurité (action 93 %, titres 90 %)");
                ui.label(RichText::new("cache de format").color(p.muted).size(11.0));
                for (r, l) in [(0.0, "aucun"), (1.0, "1:1"), (1.85, "1.85:1"), (2.0, "2:1"), (2.39, "2.39:1 (scope)"), (16.0 / 9.0, "16:9"), (4.0 / 3.0, "4:3"), (9.0 / 16.0, "9:16 (vertical)")] {
                    ui.radio_value(&mut self.guide_ratio, r, l);
                }
            });
            ui.checkbox(&mut self.draw, "dessiner");
            if self.draw {
                ui.color_edit_button_srgba(&mut self.pen);
                ui.add(egui::Slider::new(&mut self.pen_width, 1.0..=20.0).show_value(false));
                if ui.small_button("effacer l'image").clicked() {
                    self.lines.remove(&self.held(self.cur));
                    self.save_lines();
                }
            }
            ui.separator();
            if ui.button("exporter…").clicked() && self.export.is_none() {
                self.export = Some(Export { fps: info.fps, every: self.on, format: "png", height: 0, with_drawings: true, job: None });
            }
        });

        // Infos
        let held = self.held(self.cur);
        let t = held as f32 / self.preview_fps;
        ui.label(
            RichText::new(format!(
                "image {}/{}{} · {:.2} s · source {}×{} à {:.2} i/s, {:.1} s{}",
                held + 1,
                frames.max(1),
                if done { "" } else { " (extraction…)" },
                t,
                info.width,
                info.height,
                info.fps,
                info.duration,
                if (self.preview_fps - info.fps).abs() > 0.01 { format!(" · aperçu à {:.1} i/s", self.preview_fps) } else { String::new() }
            ))
            .color(p.muted)
            .size(11.0),
        );

        // Export (panneau)
        self.export_panel(ui, &info);

        // Image
        let strip = if self.show_strip { self.strip_h + 58.0 } else { 26.0 };
        let avail = ui.available_size() - vec2(0.0, 46.0 + strip);
        let (rect, resp) = ui.allocate_exact_size(avail.max(vec2(100.0, 100.0)), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(18));
        let aspect = info.width.max(1) as f32 / info.height.max(1) as f32;
        let vsize = vec2(info.width.max(1) as f32, info.height.max(1) as f32);
        self.zoom.interact(ui, &resp, rect, vsize, !self.draw);
        let img_rect = self.zoom.image_rect(rect, vsize);
        let _ = aspect;
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        // Pelures : les plus éloignées sont les plus pâles.
        let mut ghosts: Vec<(usize, Color32)> = vec![];
        if self.onion && frames > 0 {
            let fade = |k: usize, n: usize| self.onion_opacity * (1.0 - (k as f32 - 1.0) / n.max(1) as f32 * 0.7);
            for k in (1..=self.onion_before).rev() {
                if let Some(i) = held.checked_sub(k * self.on) {
                    let a = (fade(k, self.onion_before) * 255.0) as u8;
                    ghosts.push((i, if self.onion_tint { Color32::from_rgba_unmultiplied(255, 80, 80, a) } else { Color32::from_white_alpha(a) }));
                }
            }
            for k in (1..=self.onion_after).rev() {
                let i = held + k * self.on;
                if i < frames {
                    let a = (fade(k, self.onion_after) * 255.0) as u8;
                    ghosts.push((i, if self.onion_tint { Color32::from_rgba_unmultiplied(80, 255, 140, a) } else { Color32::from_white_alpha(a) }));
                }
            }
        }
        match self.frame_tex(&ctx, held) {
            Some(t) => {
                painter.image(t.id(), img_rect, uv, Color32::WHITE);
            }
            None => {
                painter.text(rect.center(), egui::Align2::CENTER_CENTER, "extraction des images…", egui::FontId::monospace(13.0), p.muted);
            }
        }
        // Les pelures passent par-dessus l'image courante, en transparence.
        for (i, tint) in &ghosts {
            if let Some(t) = self.frame_tex(&ctx, *i) {
                painter.image(t.id(), img_rect, uv, *tint);
            }
        }
        // Repères
        let guide = Stroke::new(1.0, Color32::from_white_alpha(110));
        if self.guide_ratio > 0.0 {
            let r = self.guide_ratio;
            let (w, h) = (img_rect.width(), img_rect.height());
            let mask = Color32::from_black_alpha(170);
            if w / h > r {
                let bw = (w - h * r) / 2.0;
                painter.rect_filled(Rect::from_min_size(img_rect.min, vec2(bw, h)), 0.0, mask);
                painter.rect_filled(Rect::from_min_size(pos2(img_rect.right() - bw, img_rect.top()), vec2(bw, h)), 0.0, mask);
            } else {
                let bh = (h - w / r) / 2.0;
                painter.rect_filled(Rect::from_min_size(img_rect.min, vec2(w, bh)), 0.0, mask);
                painter.rect_filled(Rect::from_min_size(pos2(img_rect.left(), img_rect.bottom() - bh), vec2(w, bh)), 0.0, mask);
            }
        }
        if self.guide_thirds {
            for k in 1..3 {
                let x = img_rect.left() + img_rect.width() * k as f32 / 3.0;
                let y = img_rect.top() + img_rect.height() * k as f32 / 3.0;
                painter.line_segment([pos2(x, img_rect.top()), pos2(x, img_rect.bottom())], guide);
                painter.line_segment([pos2(img_rect.left(), y), pos2(img_rect.right(), y)], guide);
            }
        }
        if self.guide_center {
            let c = img_rect.center();
            painter.line_segment([c - vec2(12.0, 0.0), c + vec2(12.0, 0.0)], guide);
            painter.line_segment([c - vec2(0.0, 12.0), c + vec2(0.0, 12.0)], guide);
        }
        if self.guide_safe {
            for k in [0.93, 0.90] {
                painter.rect_stroke(Rect::from_center_size(img_rect.center(), img_rect.size() * k), 0.0, guide, egui::StrokeKind::Inside);
            }
        }

        // Dessins
        let to_screen = |q: [f32; 2]| pos2(img_rect.left() + q[0] * img_rect.width(), img_rect.top() + q[1] * img_rect.height());
        let draw_lines = |lines: &[Line], alpha: f32| {
            for l in lines {
                let c = Color32::from_rgba_unmultiplied(l.color[0], l.color[1], l.color[2], (l.color[3] as f32 * alpha) as u8);
                let pts: Vec<Pos2> = l.pts.iter().map(|q| to_screen(*q)).collect();
                let w = l.width * img_rect.height() / 720.0;
                if pts.len() == 1 {
                    painter.circle_filled(pts[0], w / 2.0, c);
                } else {
                    painter.add(Shape::line(pts, Stroke::new(w, c)));
                }
            }
        };
        if self.onion && self.onion_drawings {
            for (i, tint) in &ghosts {
                if let Some(ls) = self.lines.get(i) {
                    draw_lines(ls, tint.a() as f32 / 255.0 * 0.8);
                }
            }
        }
        if let Some(ls) = self.lines.get(&held) {
            draw_lines(ls, 1.0);
        }
        if self.draw {
            let to_norm = |s: Pos2| [((s.x - img_rect.left()) / img_rect.width()).clamp(0.0, 1.0), ((s.y - img_rect.top()) / img_rect.height()).clamp(0.0, 1.0)];
            if resp.drag_started_by(egui::PointerButton::Primary) {
                self.playing = false;
                self.current.clear();
            }
            if resp.dragged_by(egui::PointerButton::Primary) {
                if let Some(pt) = resp.interact_pointer_pos() {
                    self.current.push(to_norm(pt));
                }
            }
            if !self.current.is_empty() {
                let pts: Vec<Pos2> = self.current.iter().map(|q| to_screen(*q)).collect();
                painter.add(Shape::line(pts, Stroke::new(self.pen_width * img_rect.height() / 720.0, self.pen)));
            }
            if resp.drag_stopped() && !self.current.is_empty() {
                let c = self.pen;
                let line = Line { pts: std::mem::take(&mut self.current), color: [c.r(), c.g(), c.b(), c.a()], width: self.pen_width };
                self.lines.entry(held).or_default().push(line);
                self.save_lines();
            }
            if ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, Key::Z))) {
                if let Some(ls) = self.lines.get_mut(&held) {
                    ls.pop();
                }
                self.save_lines();
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        } else if resp.clicked() {
            self.playing = !self.playing;
            self.clock = None;
        }

        self.zoom.badge(ui, rect, vsize);
        // Clavier
        if resp.hovered() || resp.has_focus() {
            // Maj + ← / → : image dessinée précédente / suivante
            let (sl, sr) = ui.input(|i| (i.modifiers.shift && i.key_pressed(Key::ArrowLeft), i.modifiers.shift && i.key_pressed(Key::ArrowRight)));
            if sl || sr {
                let next = if sr { self.lines.keys().find(|k| **k > held).copied() } else { self.lines.keys().rev().find(|k| **k < held).copied() };
                if let Some(k) = next {
                    self.cur = k;
                    self.playing = false;
                }
            }
            let (left, right, space) = ui.input(|i| (i.key_pressed(Key::ArrowLeft) && !i.modifiers.shift, i.key_pressed(Key::ArrowRight) && !i.modifiers.shift, i.key_pressed(Key::Space)));
            if ui.input(|i| i.key_pressed(Key::I)) {
                self.loop_in = Some(self.cur);
            }
            if ui.input(|i| i.key_pressed(Key::O)) {
                self.loop_out = Some(self.cur);
            }
            if left {
                self.cur = self.cur.saturating_sub(self.on);
                self.playing = false;
            }
            if right {
                self.cur = (self.cur + self.on).min(frames.saturating_sub(1));
                self.playing = false;
            }
            if space && !self.draw {
                self.playing = !self.playing;
                self.clock = None;
            }
        }

        // Ligne de temps : repères des images dessinées (aimantées), crochets de boucle à glisser
        let (tl, tresp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click_and_drag());
        let pt = ui.painter_at(tl.expand(4.0));
        pt.rect_filled(tl, 0.0, p.panel);
        let n = frames.max(1) as f32;
        let x_of = |k: f32| tl.left() + (k + 0.5) / n * tl.width();
        let frame_at = |x: f32| (((x - tl.left()) / tl.width()) * n).clamp(0.0, n - 1.0) as usize;
        // Graduations : une par seconde
        let fps = self.preview_fps.max(1.0);
        let mut sec = 0.0;
        while sec * fps < n {
            let x = x_of(sec * fps);
            pt.line_segment([pos2(x, tl.bottom() - 6.0), pos2(x, tl.bottom())], Stroke::new(1.0, p.line));
            sec += 1.0;
        }
        for &k in self.lines.keys() {
            let x = x_of(k as f32);
            pt.line_segment([pos2(x, tl.top()), pos2(x, tl.top() + 10.0)], Stroke::new(2.0, p.accent));
        }
        let last = frames.saturating_sub(1);
        let (a, b) = (self.loop_in.unwrap_or(0), self.loop_out.unwrap_or(last));
        let has_loop = self.loop_in.is_some() || self.loop_out.is_some();
        if has_loop {
            let r = Rect::from_x_y_ranges(x_of(a as f32) - 0.5 / n * tl.width()..=x_of(b as f32) + 0.5 / n * tl.width(), tl.y_range());
            pt.rect_filled(r, 0.0, p.accent.gamma_multiply(0.15));
        }
        // Crochets : toujours visibles, glisser pour régler, clic droit pour enlever la boucle
        let mut bracket_drag = false;
        for (is_in, k) in [(true, a), (false, b)] {
            let x = if is_in { x_of(k as f32) - 0.5 / n * tl.width() } else { x_of(k as f32) + 0.5 / n * tl.width() };
            let hit = Rect::from_center_size(pos2(x, tl.center().y), vec2(12.0, tl.height()));
            let id = ui.make_persistent_id(("bracket", is_in, &self.path));
            let r = ui.interact(hit, id, Sense::click_and_drag()).on_hover_text(if is_in { "début de la boucle ( [ ) · glisser · clic droit : enlever" } else { "fin de la boucle ( ] ) · glisser · clic droit : enlever" });
            let col = if has_loop || r.hovered() || r.dragged() { p.accent } else { p.muted.gamma_multiply(0.6) };
            let dir = if is_in { 1.0 } else { -1.0 };
            pt.line_segment([pos2(x, tl.top()), pos2(x, tl.bottom())], Stroke::new(2.0, col));
            pt.line_segment([pos2(x, tl.top()), pos2(x + 6.0 * dir, tl.top())], Stroke::new(2.0, col));
            pt.line_segment([pos2(x, tl.bottom()), pos2(x + 6.0 * dir, tl.bottom())], Stroke::new(2.0, col));
            if r.hovered() || r.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if r.dragged() {
                bracket_drag = true;
                if let Some(pp) = r.interact_pointer_pos() {
                    let mut f = frame_at(pp.x);
                    // Aimant : images dessinées (Alt pour désactiver)
                    if !ui.input(|i| i.modifiers.alt) {
                        if let Some(&k) = self.lines.keys().min_by_key(|k| ((x_of(**k as f32) - pp.x).abs() * 100.0) as i64) {
                            if (x_of(k as f32) - pp.x).abs() < 7.0 {
                                f = k;
                            }
                        }
                    }
                    if is_in {
                        self.loop_in = Some(f.min(self.loop_out.unwrap_or(last)));
                    } else {
                        self.loop_out = Some(f.max(self.loop_in.unwrap_or(0)));
                    }
                }
            }
            if r.secondary_clicked() {
                self.loop_in = None;
                self.loop_out = None;
            }
        }
        let x = x_of(held as f32);
        pt.rect_filled(Rect::from_center_size(pos2(x, tl.center().y), vec2(3.0, tl.height())), 0.0, p.accent);
        if !bracket_drag && (tresp.dragged() || tresp.clicked()) {
            if let Some(pp) = tresp.interact_pointer_pos() {
                let mut f = frame_at(pp.x);
                if !ui.input(|i| i.modifiers.alt) {
                    if let Some(&k) = self.lines.keys().min_by_key(|k| ((x_of(**k as f32) - pp.x).abs() * 100.0) as i64) {
                        if (x_of(k as f32) - pp.x).abs() < 7.0 {
                            f = k;
                        }
                    }
                }
                self.cur = f;
                self.playing = false;
            }
        }
        if tresp.hovered() {
            if let Some(hp) = tresp.hover_pos() {
                let f = frame_at(hp.x);
                pt.text(pos2(hp.x, tl.top() - 2.0), egui::Align2::CENTER_BOTTOM, format!("{}", f + 1), egui::FontId::monospace(10.0), p.muted);
            }
        }
        self.filmstrip(ui, frames, held, info.width as f32 / info.height.max(1) as f32);
    }

    fn export_panel(&mut self, ui: &mut egui::Ui, info: &Info) {
        let p = theme::pal();
        let Some(ex) = &mut self.export else { return };
        let mut close = false;
        let mut start = false;
        egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).inner_margin(10).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("exporter").family(theme::bold()));
                ui.label("format");
                for f in ["png", "jpg", "mp4"] {
                    if ui.selectable_label(ex.format == f, f).clicked() {
                        ex.format = f;
                    }
                }
                ui.label("cadence");
                ui.add(egui::DragValue::new(&mut ex.fps).range(1.0..=120.0).speed(0.2).suffix(" i/s"));
                ui.label("en");
                for n in [1usize, 2, 3, 4] {
                    if ui.selectable_label(ex.every == n, format!("{n}s")).clicked() {
                        ex.every = n;
                    }
                }
                ui.add(egui::DragValue::new(&mut ex.every).range(1..=240).speed(0.1).suffix("s"));
                ui.label("hauteur");
                for (h, l) in [(0u32, "originale"), (1080, "1080"), (720, "720")] {
                    if ui.selectable_label(ex.height == h, l).clicked() {
                        ex.height = h;
                    }
                }
                if ex.format != "mp4" {
                    ui.checkbox(&mut ex.with_drawings, "avec les dessins");
                }
            });
            let total = (info.duration * ex.fps / ex.every as f32).round() as usize;
            let what = if ex.format == "mp4" {
                format!("vidéo à {:.2} i/s, chaque image tenue {} fois", ex.fps, ex.every)
            } else {
                format!("≈ {total} images ({:.2} images par seconde de vidéo)", ex.fps / ex.every as f32)
            };
            ui.label(RichText::new(what).color(p.muted).size(11.0));
            ui.horizontal(|ui| {
                let running = ex.job.as_ref().is_some_and(|j| !j.lock().unwrap().finished);
                if ui.add_enabled(!running, egui::Button::new("exporter")).clicked() {
                    start = true;
                }
                if ui.button("fermer").clicked() {
                    close = true;
                }
                if let Some(j) = &ex.job {
                    let j = j.lock().unwrap().clone();
                    if let Some(e) = &j.error {
                        ui.label(RichText::new(e).color(p.missing).size(11.5));
                    } else if j.finished {
                        ui.label(RichText::new(format!("terminé : {}", j.out.as_ref().map(|o| o.display().to_string()).unwrap_or_default())).color(p.accent).size(11.5));
                        if let Some(o) = &j.out {
                            if ui.small_button("ouvrir le dossier").clicked() {
                                crate::settings::reveal(o, o.is_file());
                            }
                        }
                    } else {
                        ui.add(egui::ProgressBar::new(if j.total > 0 { j.done as f32 / j.total as f32 } else { 0.0 }).desired_width(220.0).corner_radius(0.0).fill(p.accent).text(format!("{} / {}", j.done, j.total)));
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                    }
                }
            });
        });
        if start {
            self.start_export(ui.ctx().clone(), info.clone());
        }
        if close {
            self.export = None;
        }
    }

    fn start_export(&mut self, ctx: egui::Context, info: Info) {
        let Some(ex) = &mut self.export else { return };
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "video".into());
        let parent = self.path.parent().map(Path::to_path_buf).unwrap_or_default();
        let (fps, every, format, height, with_drawings) = (ex.fps, ex.every.max(1), ex.format, ex.height, ex.with_drawings);
        let out = if format == "mp4" {
            parent.join(format!("{stem}_{}ips_en{every}s.mp4", fps.round()))
        } else {
            parent.join(format!("{stem}_{}ips_en{every}s", fps.round()))
        };
        let total = if format == "mp4" { (info.duration * fps) as usize } else { (info.duration * fps / every as f32).round() as usize };
        let job = Arc::new(Mutex::new(ExportJob { total, out: Some(out.clone()), ..Default::default() }));
        ex.job = Some(job.clone());
        let src = self.path.clone();
        // Dessins : image source (aperçu) → image exportée
        let drawings: Vec<(usize, Vec<Line>)> = if with_drawings && format != "mp4" { self.lines.iter().map(|(k, v)| (*k, v.clone())).collect() } else { vec![] };
        let preview_fps = self.preview_fps;
        std::thread::spawn(move || {
            let set = |f: &dyn Fn(&mut ExportJob)| {
                if let Ok(mut j) = job.lock() {
                    f(&mut j);
                }
                ctx.request_repaint();
            };
            let r = (|| -> Result<(), String> {
                let ff = ffmpeg::find("ffmpeg").ok_or("ffmpeg n'est pas installé.")?;
                let scale = if height > 0 { format!(",scale=-2:{height}") } else { String::new() };
                let mut cmd = ffmpeg::command(ff);
                cmd.args(["-v", "error", "-y", "-i"]).arg(&src);
                if format == "mp4" {
                    // « en 2s » : on garde 1 image sur 2 puis on la tient 2 fois.
                    cmd.args(["-vf", &format!("fps={},fps={fps}{scale}", fps / every as f32), "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18", "-an"]).arg(&out);
                } else {
                    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
                    cmd.args(["-vf", &format!("fps={}{scale}", fps / every as f32)]);
                    if format == "jpg" {
                        cmd.args(["-q:v", "2"]);
                    }
                    cmd.arg(out.join(format!("{stem}_%04d.{format}")));
                }
                let mut child = cmd.args(["-progress", "pipe:1", "-nostats"]).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
                if let Some(o) = child.stdout.take() {
                    for line in std::io::BufReader::new(o).lines().map_while(Result::ok) {
                        if let Some(n) = line.strip_prefix("frame=").and_then(|n| n.trim().parse::<usize>().ok()) {
                            set(&move |j: &mut ExportJob| j.done = n);
                        }
                    }
                }
                if !child.wait().map_err(|e| e.to_string())?.success() {
                    let mut msg = String::new();
                    if let Some(mut e) = child.stderr.take() {
                        let _ = std::io::Read::read_to_string(&mut e, &mut msg);
                    }
                    return Err(format!("ffmpeg : {}", msg.lines().last().unwrap_or("erreur")));
                }
                // Incruste les dessins dans les images exportées correspondantes.
                let out_fps = fps / every as f32;
                for (k, lines) in drawings {
                    let t = k as f32 / preview_fps;
                    let idx = (t * out_fps).floor() as usize + 1;
                    let file = out.join(format!("{stem}_{idx:04}.{format}"));
                    let Ok(img) = image::open(&file) else { continue };
                    let mut img = img.to_rgba8();
                    burn(&mut img, &lines);
                    let _ = image::DynamicImage::ImageRgba8(img).to_rgb8().save(&file);
                }
                Ok(())
            })();
            set(&move |j: &mut ExportJob| {
                j.finished = true;
                if let Err(e) = &r {
                    j.error = Some(e.clone());
                }
                if j.error.is_none() {
                    j.done = j.total;
                }
            });
        });
    }
}

/// Image de couverture (vers 1 s) pour les refs et les boards.
pub fn poster(path: &Path) -> Result<image::DynamicImage, String> {
    let ff = ffmpeg::find("ffmpeg").ok_or("ffmpeg n'est pas installé")?;
    let out = ffmpeg::command(ff)
        .args(["-v", "error", "-ss", "1", "-i"])
        .arg(path)
        .args(["-frames:v", "1", "-vf", "scale=-2:'min(480,ih)'", "-f", "image2pipe", "-vcodec", "png", "-"])
        .output()
        .map_err(|e| e.to_string())?;
    let bytes = if out.stdout.is_empty() {
        // Vidéo de moins d'une seconde : première image.
        ffmpeg::command(ffmpeg::find("ffmpeg").unwrap()).args(["-v", "error", "-i"]).arg(path).args(["-frames:v", "1", "-f", "image2pipe", "-vcodec", "png", "-"]).output().map_err(|e| e.to_string())?.stdout
    } else {
        out.stdout
    };
    image::load_from_memory(&bytes).map_err(|_| "aperçu vidéo indisponible".to_string())
}

/// Dessine des traits (coordonnées 0..1) dans une image.
pub fn burn(img: &mut image::RgbaImage, lines: &[Line]) {
    let (w, h) = (img.width() as f32, img.height() as f32);
    for l in lines {
        let r = (l.width * h / 720.0 / 2.0).max(0.5);
        let pts: Vec<(f32, f32)> = l.pts.iter().map(|q| (q[0] * w, q[1] * h)).collect();
        let segs: Vec<((f32, f32), (f32, f32))> = if pts.len() == 1 { vec![(pts[0], pts[0])] } else { pts.windows(2).map(|s| (s[0], s[1])).collect() };
        for (a, b) in segs {
            let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            let steps = (len / (r * 0.5).max(0.5)).ceil().max(1.0) as usize;
            for s in 0..=steps {
                let t = s as f32 / steps as f32;
                let (cx, cy) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                for y in ((cy - r).floor() as i32)..=((cy + r).ceil() as i32) {
                    for x in ((cx - r).floor() as i32)..=((cx + r).ceil() as i32) {
                        if x < 0 || y < 0 || x as f32 >= w || y as f32 >= h {
                            continue;
                        }
                        if ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt() <= r {
                            img.put_pixel(x as u32, y as u32, image::Rgba(l.color));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_ffmpeg() {
        assert!(is_video("rendu/Plan_01.MP4") && !is_video("a.png"));
        let mut img = image::RgbaImage::from_pixel(100, 100, image::Rgba([0, 0, 0, 255]));
        burn(&mut img, &[Line { pts: vec![[0.1, 0.5], [0.9, 0.5]], color: [255, 0, 0, 255], width: 20.0 }]);
        assert_eq!(img.get_pixel(50, 50).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(50, 10).0, [0, 0, 0, 255]);

        // Avec ffmpeg disponible : vidéo de test de 2 s à 24 i/s, export en 2s à 24 i/s = 24 images.
        let Some(ff) = ffmpeg::find("ffmpeg") else { return };
        let dir = std::env::temp_dir().join("aryadne-video-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("test.mp4");
        let st = ffmpeg::command(ff).args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=duration=2:size=320x240:rate=24", "-pix_fmt", "yuv420p"]).arg(&src).status().unwrap();
        assert!(st.success());
        let info = probe(&src).unwrap();
        assert_eq!((info.width, info.height), (320, 240));
        assert!((info.fps - 24.0).abs() < 0.01 && (info.duration - 2.0).abs() < 0.1);

        let ctx = egui::Context::default();
        let mut v = Viewer::open(src, dir.join("notes.json"), &ctx);
        for _ in 0..200 {
            if v.extract.lock().unwrap().done {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(v.extract.lock().unwrap().frames, 48);
        v.lines.insert(0, vec![Line { pts: vec![[0.2, 0.2], [0.8, 0.8]], color: [255, 0, 0, 255], width: 30.0 }]);
        v.export = Some(Export { fps: 24.0, every: 2, format: "png", height: 0, with_drawings: true, job: None });
        v.start_export(ctx, info);
        let job = v.export.as_ref().unwrap().job.clone().unwrap();
        for _ in 0..400 {
            if job.lock().unwrap().finished {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let j = job.lock().unwrap().clone();
        assert!(j.error.is_none(), "{:?}", j.error);
        let out = j.out.unwrap();
        let n = std::fs::read_dir(&out).unwrap().count();
        assert_eq!(n, 24, "2 s à 24 i/s en 2s = 24 images");
        let first = image::open(out.join("test_0001.png")).unwrap().to_rgba8();
        assert_eq!(first.get_pixel(160, 120).0[..3], [255, 0, 0], "le dessin est incrusté dans la première image");
    }
}
