//! Montage vidéo (`.aryav`) : des pistes, des clips qu'on déplace, raccourcit et coupe,
//! un aperçu à la tête de lecture et un export. Le fichier est du JSON lisible ; les médias
//! restent à leur place. Tout le calcul d'image passe par ffmpeg, avec un seul graphe de
//! filtres pour l'aperçu, la lecture et l'export : ce qu'on voit est ce qu'on exporte.

use crate::ffmpeg;
use eframe::egui::{self, pos2, vec2, Color32, Rect, RichText, Sense, Stroke};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::mpsc::{channel, sync_channel, Receiver};

pub const EXT: &str = "aryav";

pub fn is_project(rel: &str) -> bool {
    rel.to_lowercase().ends_with(&format!(".{EXT}"))
}

fn one() -> f32 {
    1.0
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Clip {
    pub name: String,
    /// Chemin du média, relatif au dossier du montage quand c'est possible
    pub file: String,
    /// « video », « audio » ou « image »
    pub kind: String,
    pub track: usize,
    /// Début sur la timeline, en secondes
    pub start: f32,
    /// Partie du média utilisée, en secondes de la source
    pub src_in: f32,
    pub src_out: f32,
    /// Durée du média (0 : sans limite, pour une image)
    #[serde(default)]
    pub media: f32,
    #[serde(default)]
    pub has_audio: bool,
    #[serde(default = "one")]
    pub speed: f32,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default = "one")]
    pub volume: f32,
    #[serde(default)]
    pub fade_in: f32,
    #[serde(default)]
    pub fade_out: f32,
    /// Étalonnage : 0 = neutre pour la luminosité, 1 pour le contraste et la saturation
    #[serde(default)]
    pub brightness: f32,
    #[serde(default = "one")]
    pub contrast: f32,
    #[serde(default = "one")]
    pub saturation: f32,
    /// Placement dans l'image : décalage en pixels du projet, et taille
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default = "one")]
    pub scale: f32,
}

impl Clip {
    pub fn len(&self) -> f32 {
        ((self.src_out - self.src_in) / self.speed.max(0.01)).max(0.0)
    }
    pub fn end(&self) -> f32 {
        self.start + self.len()
    }
    fn visual(&self) -> bool {
        self.kind != "audio"
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Project {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub tracks: usize,
    pub clips: Vec<Clip>,
}

impl Default for Project {
    fn default() -> Self {
        Project { version: 1, width: 1920, height: 1080, fps: 24.0, tracks: 3, clips: vec![] }
    }
}

impl Project {
    pub fn read(path: &Path) -> Result<Project, String> {
        let t = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        if t.trim().is_empty() {
            return Ok(Project::default());
        }
        serde_json::from_str(&t).map_err(|e| format!("Montage illisible : {e}"))
    }

    pub fn write(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, serde_json::to_string_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }

    pub fn duration(&self) -> f32 {
        self.clips.iter().map(|c| c.end()).fold(0.0, f32::max)
    }

    /// Coupe le clip `i` en deux à l'instant `t` de la timeline. Renvoie l'indice de la seconde moitié.
    pub fn split(&mut self, i: usize, t: f32) -> Option<usize> {
        let c = self.clips.get(i)?.clone();
        if t <= c.start + 0.02 || t >= c.end() - 0.02 {
            return None;
        }
        let cut = c.src_in + (t - c.start) * c.speed;
        let mut b = c.clone();
        self.clips[i].src_out = cut;
        self.clips[i].fade_out = 0.0;
        b.src_in = cut;
        b.start = t;
        b.fade_in = 0.0;
        self.clips.push(b);
        Some(self.clips.len() - 1)
    }
}

pub fn resolve(project: &Path, file: &str) -> PathBuf {
    let p = Path::new(file);
    if p.is_absolute() { p.to_path_buf() } else { project.parent().unwrap_or(Path::new("")).join(p) }
}

fn kind_of(path: &Path) -> &'static str {
    match path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default().as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tga" | "tif" | "tiff" => "image",
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "opus" => "audio",
        _ => "video",
    }
}

/// Lit ce qu'il faut savoir d'un média pour en faire un clip.
pub fn import(project: &Path, media: &Path, track: usize, start: f32) -> Result<Clip, String> {
    let kind = kind_of(media);
    let (mut dur, mut has_audio) = (5.0f32, false);
    if kind != "image" {
        let ffprobe = ffmpeg::find("ffprobe").ok_or("ffprobe introuvable")?;
        let out = ffmpeg::command(ffprobe).args(["-v", "error", "-show_entries", "format=duration:stream=codec_type", "-of", "json"]).arg(media).output().map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|_| "Média illisible.".to_string())?;
        dur = v["format"]["duration"].as_str().and_then(|d| d.parse().ok()).ok_or("Média illisible (durée inconnue).")?;
        has_audio = v["streams"].as_array().is_some_and(|s| s.iter().any(|x| x["codec_type"] == "audio"));
    }
    let dir = project.parent().unwrap_or(Path::new(""));
    let file = media.strip_prefix(dir).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| media.to_string_lossy().to_string());
    Ok(Clip {
        name: media.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "clip".into()),
        file,
        kind: kind.into(),
        track,
        start,
        src_in: 0.0,
        src_out: dur,
        media: if kind == "image" { 0.0 } else { dur },
        has_audio: has_audio || kind == "audio",
        speed: 1.0,
        opacity: 1.0,
        volume: 1.0,
        fade_in: 0.0,
        fade_out: 0.0,
        brightness: 0.0,
        contrast: 1.0,
        saturation: 1.0,
        x: 0.0,
        y: 0.0,
        scale: 1.0,
    })
}

/// Le graphe ffmpeg du montage à partir de l'instant `from`, à la taille `w` × `h`.
/// Renvoie les arguments d'entrée, le filtre, si une piste son existe, et la durée restante.
pub fn graph(p: &Project, project: &Path, from: f32, w: u32, h: u32, realtime: bool, with_audio: bool) -> (Vec<String>, String, bool, f32) {
    let total = (p.duration() - from).max(1.0 / p.fps.max(1.0));
    let k = w as f32 / p.width.max(1) as f32;
    let mut args: Vec<String> = vec![];
    if realtime {
        args.push("-re".into());
    }
    args.extend(["-f".into(), "lavfi".into(), "-i".into(), format!("color=c=black:s={w}x{h}:r={}:d={total:.3}", p.fps)]);
    let mut filter = String::new();
    let (mut last, mut n, mut audio) = ("0:v".to_string(), 1usize, vec![]);
    let mut order: Vec<&Clip> = p.clips.iter().filter(|c| c.end() > from + 1e-3 && c.len() > 1e-3).collect();
    order.sort_by(|a, b| a.track.cmp(&b.track).then(a.start.total_cmp(&b.start)));
    for c in order {
        // Ce qui reste du clip après `from`
        let skipped = (from - c.start).max(0.0);
        let (st, en) = ((c.start - from).max(0.0), c.end() - from);
        let (src_in, src_len) = (c.src_in + skipped * c.speed, (c.src_out - c.src_in) - skipped * c.speed);
        let path = resolve(project, &c.file).to_string_lossy().to_string();
        if c.kind == "image" {
            args.extend(["-loop".into(), "1".into(), "-t".into(), format!("{:.3}", en - st), "-i".into(), path]);
        } else {
            args.extend(["-ss".into(), format!("{src_in:.3}"), "-t".into(), format!("{src_len:.3}"), "-i".into(), path]);
        }
        if c.visual() && c.opacity > 0.0 {
            let (cw, ch) = (((w as f32 * c.scale).round() as u32).max(2), ((h as f32 * c.scale).round() as u32).max(2));
            filter += &format!("[{n}:v]fps={},setpts=(PTS-STARTPTS)/{:.4}+{st:.3}/TB,scale=w={cw}:h={ch}:force_original_aspect_ratio=decrease", p.fps, c.speed.max(0.01));
            if c.brightness != 0.0 || c.contrast != 1.0 || c.saturation != 1.0 {
                filter += &format!(",eq=brightness={:.3}:contrast={:.3}:saturation={:.3}", c.brightness.clamp(-1.0, 1.0), c.contrast.clamp(0.0, 4.0), c.saturation.clamp(0.0, 3.0));
            }
            filter += ",format=rgba";
            if c.opacity < 1.0 {
                filter += &format!(",colorchannelmixer=aa={:.3}", c.opacity.clamp(0.0, 1.0));
            }
            if c.fade_in > 0.0 && skipped == 0.0 {
                filter += &format!(",fade=t=in:st={st:.3}:d={:.3}:alpha=1", c.fade_in);
            }
            if c.fade_out > 0.0 {
                filter += &format!(",fade=t=out:st={:.3}:d={:.3}:alpha=1", (en - c.fade_out).max(0.0), c.fade_out);
            }
            filter += &format!("[c{n}];[{last}][c{n}]overlay=x=(W-w)/2+{:.1}:y=(H-h)/2+{:.1}:enable='between(t,{st:.3},{en:.3})':eof_action=pass[o{n}];", c.x * k, c.y * k);
            last = format!("o{n}");
        }
        if with_audio && c.has_audio && c.volume > 0.0 && c.kind != "image" {
            filter += &format!("[{n}:a]asetpts=PTS-STARTPTS,atempo={:.4},volume={:.3},adelay={}:all=1[a{n}];", c.speed.clamp(0.5, 2.0), c.volume, (st * 1000.0) as u64);
            audio.push(format!("[a{n}]"));
        }
        n += 1;
    }
    if !audio.is_empty() {
        filter += &format!("{}amix=inputs={}:normalize=0:duration=longest[aout];", audio.concat(), audio.len());
    }
    filter += &format!("[{last}]null[vout]");
    (args, filter, !audio.is_empty(), total)
}

/// Exporte le montage en MP4 (H.264 + AAC).
pub fn export(p: &Project, project: &Path, out: &Path) -> Result<(), String> {
    let ff = ffmpeg::find("ffmpeg").ok_or("ffmpeg n'est pas installé.")?;
    if p.clips.is_empty() {
        return Err("Le montage est vide.".into());
    }
    let (w, h) = (p.width.max(2) / 2 * 2, p.height.max(2) / 2 * 2);
    let (inputs, filter, has_audio, total) = graph(p, project, 0.0, w, h, false, true);
    let mut cmd = ffmpeg::command(ff);
    cmd.args(["-v", "error", "-y"]).args(inputs).args(["-filter_complex", &filter, "-map", "[vout]"]);
    if has_audio {
        cmd.args(["-map", "[aout]", "-c:a", "aac", "-b:a", "192k"]);
    }
    cmd.args(["-t", &format!("{total:.3}"), "-c:v", "libx264", "-preset", "medium", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart"]).arg(out);
    let o = cmd.output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!("ffmpeg : {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("erreur")));
    }
    Ok(())
}

/// L'image du montage à l'instant `t`, à la largeur `w`.
pub fn frame(p: &Project, project: &Path, t: f32, w: u32) -> Result<image::RgbaImage, String> {
    let ff = ffmpeg::find("ffmpeg").ok_or("ffmpeg n'est pas installé.")?;
    let h = ((w as f32 * p.height as f32 / p.width.max(1) as f32).round() as u32).max(2);
    let (inputs, filter, _, _) = graph(p, project, t, w, h, false, false);
    let o = ffmpeg::command(ff).args(["-v", "error"]).args(inputs).args(["-filter_complex", &filter, "-map", "[vout]", "-frames:v", "1", "-f", "image2pipe", "-vcodec", "png", "-"]).output().map_err(|e| e.to_string())?;
    image::load_from_memory(&o.stdout).map(|i| i.to_rgba8()).map_err(|_| format!("aperçu impossible : {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("?")))
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    Move,
    TrimIn,
    TrimOut,
}

struct Playback {
    child: Child,
    rx: Receiver<(u32, Vec<u8>)>,
    from: f32,
    size: [usize; 2],
}

pub struct Editor {
    pub path: PathBuf,
    pub project: Project,
    error: Option<String>,
    playhead: f32,
    selected: Option<usize>,
    /// Pixels par seconde, et instant affiché au bord gauche
    zoom: f32,
    scroll: f32,
    drag: Option<(usize, Drag, f32, Clip)>,
    tex: Option<egui::TextureHandle>,
    /// Aperçu en cours de calcul (instant demandé), et dernier instant affiché
    preview_rx: Option<(f32, Receiver<Result<image::RgbaImage, String>>)>,
    shown: Option<(f32, u64)>,
    rev: u64,
    play: Option<Playback>,
    export_rx: Option<Receiver<Result<PathBuf, String>>>,
    message: Option<String>,
    /// Médias glissés sur la fenêtre, à ajouter
    pub add: Vec<PathBuf>,
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Editor {
    pub fn open(path: PathBuf) -> Editor {
        let (project, error) = match Project::read(&path) {
            Ok(p) => (p, None),
            Err(e) => (Project::default(), Some(e)),
        };
        Editor { path, project, error, playhead: 0.0, selected: None, zoom: 60.0, scroll: 0.0, drag: None, tex: None, preview_rx: None, shown: None, rev: 0, play: None, export_rx: None, message: None, add: vec![] }
    }

    fn changed(&mut self) {
        self.rev += 1;
        if let Err(e) = self.project.write(&self.path) {
            self.message = Some(e);
        }
    }

    fn stop(&mut self) {
        if let Some(mut p) = self.play.take() {
            let _ = p.child.kill();
            let _ = p.child.wait();
        }
    }

    /// Lecture : un seul ffmpeg calcule le montage à partir de la tête de lecture et envoie ses images au fil de l'eau.
    fn start(&mut self, ctx: &egui::Context) {
        self.stop();
        let Some(ff) = ffmpeg::find("ffmpeg") else { return };
        if self.playhead >= self.project.duration() - 0.05 {
            self.playhead = 0.0;
        }
        let w = 640u32.min(self.project.width.max(2)) / 2 * 2;
        let h = ((w as f32 * self.project.height as f32 / self.project.width.max(1) as f32).round() as u32).max(2) / 2 * 2;
        let (inputs, filter, _, _) = graph(&self.project, &self.path, self.playhead, w, h, true, false);
        let child = ffmpeg::command(ff).args(["-v", "error"]).args(inputs).args(["-filter_complex", &filter, "-map", "[vout]", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
        let Ok(mut child) = child else { return };
        let Some(mut out) = child.stdout.take() else { return };
        let (tx, rx) = sync_channel(2);
        let ctx = ctx.clone();
        let len = (w * h * 3) as usize;
        std::thread::spawn(move || {
            let mut n = 0u32;
            loop {
                let mut buf = vec![0u8; len];
                if out.read_exact(&mut buf).is_err() || tx.send((n, buf)).is_err() {
                    break;
                }
                n += 1;
                ctx.request_repaint();
            }
            ctx.request_repaint();
        });
        self.play = Some(Playback { child, rx, from: self.playhead, size: [w as usize, h as usize] });
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let ctx = ui.ctx().clone();
        // Médias déposés : posés à la suite sur la première piste libre de leur genre
        for f in std::mem::take(&mut self.add) {
            let track = if kind_of(&f) == "audio" { 0 } else { 1.min(self.project.tracks.saturating_sub(1)) };
            let start = self.project.clips.iter().filter(|c| c.track == track).map(|c| c.end()).fold(0.0, f32::max);
            match import(&self.path, &f, track, start) {
                Ok(c) => {
                    self.project.clips.push(c);
                    self.selected = Some(self.project.clips.len() - 1);
                    self.changed();
                }
                Err(e) => self.message = Some(format!("{} : {e}", f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())),
            }
        }
        // Images de la lecture en cours
        let mut ended = false;
        if let Some(pb) = &mut self.play {
            let mut latest = None;
            while let Ok(f) = pb.rx.try_recv() {
                latest = Some(f);
            }
            let got = latest.is_some();
            if let Some((n, buf)) = latest {
                self.playhead = pb.from + n as f32 / self.project.fps.max(1.0);
                let img = egui::ColorImage::from_rgb(pb.size, &buf);
                match &mut self.tex {
                    Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                    None => self.tex = Some(ctx.load_texture("montage", img, egui::TextureOptions::LINEAR)),
                }
                self.shown = None;
            }
            ended = matches!(pb.child.try_wait(), Ok(Some(_))) && !got;
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
        if ended {
            self.stop();
        }
        // Aperçu à l'arrêt : recalculé quand la tête de lecture ou le montage change
        if self.play.is_none() {
            if let Some((_, rx)) = &self.preview_rx {
                if let Ok(r) = rx.try_recv() {
                    match r {
                        Ok(img) => {
                            let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], &img);
                            match &mut self.tex {
                                Some(tx) => tx.set(ci, egui::TextureOptions::LINEAR),
                                None => self.tex = Some(ctx.load_texture("montage", ci, egui::TextureOptions::LINEAR)),
                            }
                        }
                        Err(e) => self.message = Some(e),
                    }
                    self.preview_rx = None;
                }
            }
            let want = (self.playhead, self.rev);
            if self.preview_rx.is_none() && self.shown != Some(want) && ffmpeg::find("ffmpeg").is_some() {
                let (tx, rx) = channel();
                let (proj, path, t, c2) = (self.project.clone(), self.path.clone(), self.playhead, ctx.clone());
                std::thread::spawn(move || {
                    let _ = tx.send(frame(&proj, &path, t, 640));
                    c2.request_repaint();
                });
                self.preview_rx = Some((self.playhead, rx));
                self.shown = Some(want);
            }
        }
        if let Some(rx) = &self.export_rx {
            if let Ok(r) = rx.try_recv() {
                self.message = Some(match r {
                    Ok(f) => format!("{} : {}", crate::i18n::t("exporté"), f.display()),
                    Err(e) => e,
                });
                self.export_rx = None;
            }
        }

        // ---- Barre du haut
        let mut toggle_play = false;
        let mut split = false;
        let mut delete = false;
        ui.horizontal(|ui| {
            if ui.button(if self.play.is_some() { "⏸" } else { "▶" }).on_hover_text(crate::i18n::t("lire / arrêter (Espace)")).clicked() {
                toggle_play = true;
            }
            if ui.button("⏮").clicked() {
                self.stop();
                self.playhead = 0.0;
            }
            ui.label(RichText::new(format!("{} / {}", timecode(self.playhead, self.project.fps), timecode(self.project.duration(), self.project.fps))).monospace().color(p.muted));
            ui.separator();
            if ui.button(crate::i18n::t("＋ média")).on_hover_text(crate::i18n::t("Vidéos, images ou sons. On peut aussi les glisser sur la fenêtre.")).clicked() {
                if let Some(files) = rfd::FileDialog::new().pick_files() {
                    self.add.extend(files);
                }
            }
            if ui.button(crate::i18n::t("couper")).on_hover_text(crate::i18n::t("Coupe le clip choisi à la tête de lecture (S)")).clicked() {
                split = true;
            }
            if ui.add_enabled(self.selected.is_some(), egui::Button::new(crate::i18n::t("supprimer"))).clicked() {
                delete = true;
            }
            if ui.button(crate::i18n::t("＋ piste")).clicked() {
                self.project.tracks += 1;
                self.changed();
            }
            ui.separator();
            let mut ch = false;
            ch |= ui.add(egui::DragValue::new(&mut self.project.width).range(16..=7680).suffix(" px")).changed();
            ui.label("×");
            ch |= ui.add(egui::DragValue::new(&mut self.project.height).range(16..=4320).suffix(" px")).changed();
            ch |= ui.add(egui::DragValue::new(&mut self.project.fps).range(1.0..=120.0).suffix(" i/s")).changed();
            if ch {
                self.changed();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.export_rx.is_some() {
                    ui.spinner();
                    ui.label(RichText::new(crate::i18n::t("export en cours…")).color(p.muted));
                } else if ui.add(egui::Button::new(RichText::new(crate::i18n::t("exporter…")).color(p.on_accent)).fill(p.accent)).clicked() {
                    let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "montage".into());
                    let mut d = rfd::FileDialog::new().add_filter("MP4", &["mp4"]).set_file_name(format!("{stem}.mp4"));
                    if let Some(dir) = self.path.parent() {
                        d = d.set_directory(dir);
                    }
                    if let Some(mut out) = d.save_file() {
                        if out.extension().is_none() {
                            out.set_extension("mp4");
                        }
                        let (tx, rx) = channel();
                        let (proj, path, c2) = (self.project.clone(), self.path.clone(), ctx.clone());
                        std::thread::spawn(move || {
                            let _ = tx.send(export(&proj, &path, &out).map(|_| out));
                            c2.request_repaint();
                        });
                        self.export_rx = Some(rx);
                    }
                }
            });
        });
        if let Some(e) = &self.error {
            ui.colored_label(p.missing, e);
        }
        if let Some(m) = &self.message {
            ui.label(RichText::new(m).color(p.muted).size(11.0));
        }
        if !ctx.egui_wants_keyboard_input() {
            ui.input(|i| {
                toggle_play |= i.key_pressed(egui::Key::Space);
                split |= i.key_pressed(egui::Key::S) && !i.modifiers.any();
                delete |= i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace);
            });
        }
        if toggle_play {
            if self.play.is_some() { self.stop() } else { self.start(&ctx) }
        }
        if split {
            let t = self.playhead;
            let targets: Vec<usize> = match self.selected {
                Some(s) => vec![s],
                None => (0..self.project.clips.len()).filter(|&i| self.project.clips[i].start < t && self.project.clips[i].end() > t).collect(),
            };
            let mut any = false;
            for i in targets {
                any |= self.project.split(i, t).is_some();
            }
            if any {
                self.changed();
            }
        }
        if delete {
            if let Some(s) = self.selected.take().filter(|&s| s < self.project.clips.len()) {
                self.project.clips.remove(s);
                self.changed();
            }
        }

        // ---- Timeline en bas, réglages du clip à droite, aperçu au centre
        let tl_h = 30.0 + self.project.tracks as f32 * 36.0 + 8.0;
        egui::Panel::bottom("montage-timeline").exact_size(tl_h).frame(egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line))).show(ui, |ui| self.timeline(ui));
        if let Some(s) = self.selected.filter(|&s| s < self.project.clips.len()) {
            let mut ch = false;
            egui::Panel::right("montage-clip").exact_size(250.0).frame(egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).inner_margin(10)).show(ui, |ui| {
                let c = &mut self.project.clips[s];
                ui.label(RichText::new(crate::i18n::t("CLIP")).size(10.5).color(p.muted).family(crate::theme::bold()));
                ch |= ui.add(egui::TextEdit::singleline(&mut c.name).desired_width(f32::INFINITY)).changed();
                ui.label(RichText::new(&c.file).size(10.0).color(p.muted));
                ui.add_space(6.0);
                let mut row = |ui: &mut egui::Ui, label: &str, v: &mut f32, r: std::ops::RangeInclusive<f32>, reset: f32| {
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new(crate::i18n::t(label)).size(11.0).color(p.muted)).frame(false).min_size(vec2(84.0, 0.0))).on_hover_text(crate::i18n::t("cliquer pour remettre à zéro")).clicked() {
                            *v = reset;
                            ch = true;
                        }
                        ch |= ui.add(egui::Slider::new(v, r)).changed();
                    });
                };
                if c.kind != "audio" {
                    row(ui, "opacité", &mut c.opacity, 0.0..=1.0, 1.0);
                    row(ui, "taille", &mut c.scale, 0.05..=3.0, 1.0);
                    row(ui, "décalage x", &mut c.x, -2000.0..=2000.0, 0.0);
                    row(ui, "décalage y", &mut c.y, -2000.0..=2000.0, 0.0);
                    ui.separator();
                    ui.label(RichText::new(crate::i18n::t("ÉTALONNAGE")).size(10.5).color(p.muted).family(crate::theme::bold()));
                    row(ui, "luminosité", &mut c.brightness, -0.5..=0.5, 0.0);
                    row(ui, "contraste", &mut c.contrast, 0.2..=2.5, 1.0);
                    row(ui, "saturation", &mut c.saturation, 0.0..=3.0, 1.0);
                    ui.separator();
                }
                if c.kind != "image" {
                    row(ui, "vitesse", &mut c.speed, 0.25..=4.0, 1.0);
                }
                if c.has_audio {
                    row(ui, "volume", &mut c.volume, 0.0..=2.0, 1.0);
                }
                row(ui, "fondu d'entrée", &mut c.fade_in, 0.0..=5.0, 0.0);
                row(ui, "fondu de sortie", &mut c.fade_out, 0.0..=5.0, 0.0);
            });
            if ch {
                self.changed();
            }
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg).inner_margin(10)).show(ui, |ui| {
            let avail = ui.available_size();
            let aspect = self.project.width as f32 / self.project.height.max(1) as f32;
            let size = if avail.x / avail.y > aspect { vec2(avail.y * aspect, avail.y) } else { vec2(avail.x, avail.x / aspect) };
            ui.centered_and_justified(|ui| {
                let (r, _) = ui.allocate_exact_size(size, Sense::hover());
                ui.painter().rect_filled(r, 0.0, Color32::BLACK);
                match &self.tex {
                    Some(t) if !self.project.clips.is_empty() => {
                        ui.painter().image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    _ => {
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, crate::i18n::t("Glisse des vidéos, des images ou des sons ici"), egui::FontId::proportional(14.0), Color32::from_gray(150));
                    }
                }
            });
        });
    }

    fn timeline(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let (area, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(area);
        let (ruler_h, row_h) = (24.0, 36.0);
        let tracks = self.project.tracks.max(1);
        // Molette : défiler ; Ctrl + molette : zoomer autour du curseur
        if resp.hovered() {
            let (scroll, zoom, at) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
            if zoom != 1.0 {
                let t = at.map(|a| self.scroll + (a.x - area.left()) / self.zoom).unwrap_or(self.scroll);
                self.zoom = (self.zoom * zoom).clamp(2.0, 800.0);
                self.scroll = (t - at.map(|a| (a.x - area.left()) / self.zoom).unwrap_or(0.0)).max(0.0);
            } else if scroll != egui::Vec2::ZERO {
                self.scroll = (self.scroll - (scroll.x + scroll.y) / self.zoom).max(0.0);
            }
        }
        let (zoom, scroll) = (self.zoom, self.scroll);
        let x_of = |t: f32| area.left() + (t - scroll) * zoom;
        let t_of = |x: f32| (scroll + (x - area.left()) / zoom).max(0.0);
        // La piste la plus haute est celle qui s'affiche par-dessus les autres
        let y_of = |track: usize| area.top() + ruler_h + (tracks - 1 - track.min(tracks - 1)) as f32 * row_h;
        let track_at = |y: f32| (tracks as i32 - 1 - ((y - area.top() - ruler_h) / row_h).floor() as i32).clamp(0, tracks as i32 - 1) as usize;
        // Règle
        painter.rect_filled(Rect::from_min_size(area.min, vec2(area.width(), ruler_h)), 0.0, p.bg);
        let step = [0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 300.0].into_iter().find(|s| s * zoom > 60.0).unwrap_or(600.0);
        let mut t = (scroll / step).floor() * step;
        while x_of(t) < area.right() {
            let x = x_of(t);
            painter.line_segment([pos2(x, area.top() + ruler_h - 6.0), pos2(x, area.top() + ruler_h)], Stroke::new(1.0, p.muted));
            painter.text(pos2(x + 3.0, area.top() + 3.0), egui::Align2::LEFT_TOP, timecode(t, self.project.fps), egui::FontId::monospace(9.5), p.muted);
            t += step;
        }
        for k in 0..tracks {
            let y = y_of(k);
            painter.line_segment([pos2(area.left(), y), pos2(area.right(), y)], Stroke::new(1.0, p.line));
        }
        let rect_of = |c: &Clip| Rect::from_min_max(pos2(x_of(c.start), y_of(c.track) + 3.0), pos2(x_of(c.end()), y_of(c.track) + row_h - 3.0));
        // Souris : attraper un clip (milieu : déplacer ; bords : raccourcir), sinon déplacer la tête de lecture
        let press = ui.input(|i| i.pointer.press_origin());
        let mut on_clip = false;
        if resp.drag_started() || resp.clicked() {
            if let Some(at) = press.or(resp.interact_pointer_pos()) {
                let hit = (0..self.project.clips.len()).rev().find(|&i| rect_of(&self.project.clips[i]).contains(at));
                if at.y < area.top() + ruler_h {
                    self.selected = self.selected.filter(|_| resp.drag_started());
                    self.drag = None;
                } else if let Some(i) = hit {
                    on_clip = true;
                    self.selected = Some(i);
                    if resp.drag_started() {
                        let r = rect_of(&self.project.clips[i]);
                        let mode = if at.x < r.left() + 7.0 { Drag::TrimIn } else if at.x > r.right() - 7.0 { Drag::TrimOut } else { Drag::Move };
                        self.drag = Some((i, mode, t_of(at.x) - self.project.clips[i].start, self.project.clips[i].clone()));
                    }
                } else {
                    self.selected = None;
                }
            }
        }
        let mut dirty = false;
        if let (Some((i, mode, grab, orig)), Some(at)) = (self.drag.clone(), resp.interact_pointer_pos().filter(|_| resp.dragged())) {
            // Aimantation : bords des autres clips et tête de lecture
            let mut snaps: Vec<f32> = vec![0.0, self.playhead];
            for (k, c) in self.project.clips.iter().enumerate() {
                if k != i {
                    snaps.extend([c.start, c.end()]);
                }
            }
            let snap = |t: f32| snaps.iter().copied().filter(|s| (s - t).abs() * zoom < 8.0).min_by(|a, b| (a - t).abs().total_cmp(&(b - t).abs())).unwrap_or(t);
            let t = t_of(at.x);
            let c = &mut self.project.clips[i];
            match mode {
                Drag::Move => {
                    let len = orig.len();
                    let mut s = (t - grab).max(0.0);
                    let (a, b) = (snap(s), snap(s + len) - len);
                    s = if (a - s).abs() <= (b - s).abs() { a } else { b }.max(0.0);
                    c.start = s;
                    c.track = track_at(at.y);
                }
                Drag::TrimIn => {
                    let s = snap(t).clamp((orig.start - orig.src_in / orig.speed).max(0.0), orig.end() - 0.05);
                    let s = if orig.kind == "image" { snap(t).clamp(0.0, orig.end() - 0.05) } else { s };
                    c.src_in = if orig.kind == "image" { 0.0 } else { orig.src_in + (s - orig.start) * orig.speed };
                    if orig.kind == "image" {
                        c.src_out = orig.end() - s;
                    }
                    c.start = s;
                }
                Drag::TrimOut => {
                    let mut out = orig.src_in + (snap(t) - orig.start).max(0.05) * orig.speed;
                    if orig.media > 0.0 {
                        out = out.min(orig.media);
                    }
                    c.src_out = out;
                }
            }
            dirty = true;
        } else if resp.interact_pointer_pos().is_some() && (resp.dragged() || resp.clicked()) && self.drag.is_none() && !on_clip {
            if let Some(at) = resp.interact_pointer_pos() {
                if self.play.is_some() {
                    self.stop();
                }
                self.playhead = t_of(at.x);
            }
        }
        if resp.drag_stopped() {
            if self.drag.take().is_some() {
                self.changed();
            }
        } else if dirty {
            // Pendant le glissement : l'aperçu attend la fin, le fichier aussi
        }
        // Clips
        for (i, c) in self.project.clips.iter().enumerate() {
            let r = rect_of(c);
            if r.right() < area.left() || r.left() > area.right() {
                continue;
            }
            let base = match c.kind.as_str() {
                "audio" => Color32::from_rgb(70, 150, 110),
                "image" => Color32::from_rgb(170, 120, 60),
                _ => Color32::from_rgb(70, 110, 180),
            };
            let on = self.selected == Some(i);
            painter.rect_filled(r, 4.0, if on { base } else { base.linear_multiply(0.7) });
            painter.rect_stroke(r, 4.0, Stroke::new(if on { 2.0 } else { 1.0 }, if on { p.accent } else { p.line }), egui::StrokeKind::Inside);
            // Fondus : un coin plus sombre à l'entrée et à la sortie
            for (len, left) in [(c.fade_in, true), (c.fade_out, false)] {
                if len > 0.0 {
                    let w = (len * zoom).min(r.width());
                    let pts = if left { vec![r.left_top(), pos2(r.left() + w, r.top()), r.left_bottom()] } else { vec![r.right_top(), pos2(r.right() - w, r.top()), r.right_bottom()] };
                    painter.add(egui::Shape::convex_polygon(pts, Color32::from_black_alpha(90), Stroke::NONE));
                }
            }
            painter.with_clip_rect(r.shrink(3.0)).text(r.left_center() + vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, &c.name, egui::FontId::proportional(11.5), Color32::WHITE);
        }
        // Tête de lecture
        let x = x_of(self.playhead);
        painter.line_segment([pos2(x, area.top()), pos2(x, area.bottom())], Stroke::new(2.0, p.accent));
        painter.add(egui::Shape::convex_polygon(vec![pos2(x - 5.0, area.top()), pos2(x + 5.0, area.top()), pos2(x, area.top() + 8.0)], p.accent, Stroke::NONE));
    }
}

fn timecode(t: f32, fps: f32) -> String {
    let t = t.max(0.0);
    let f = ((t - t.floor()) * fps.max(1.0)).floor() as u32;
    format!("{:02}:{:02}:{:02}", (t / 60.0) as u32, t as u32 % 60, f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coupe_et_durees() {
        let mut p = Project::default();
        let c = Clip { name: "a".into(), file: "a.mp4".into(), kind: "video".into(), track: 1, start: 2.0, src_in: 1.0, src_out: 9.0, media: 10.0, has_audio: true, speed: 2.0, opacity: 1.0, volume: 1.0, fade_in: 0.5, fade_out: 0.5, brightness: 0.0, contrast: 1.0, saturation: 1.0, x: 0.0, y: 0.0, scale: 1.0 };
        p.clips.push(c);
        // 8 s de source à vitesse 2 : 4 s sur la timeline
        assert_eq!(p.clips[0].len(), 4.0);
        assert_eq!(p.duration(), 6.0);
        let b = p.split(0, 3.0).unwrap();
        assert_eq!((p.clips[0].src_out, p.clips[b].src_in, p.clips[b].start), (3.0, 3.0, 3.0));
        assert_eq!(p.duration(), 6.0);
        assert!(p.split(0, 2.0).is_none());
        // Le fichier se relit à l'identique
        let back: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back, p);
        // Le graphe reprend au milieu d'un clip : la source est avancée d'autant
        let (args, filter, audio, total) = graph(&p, Path::new("/x/m.aryav"), 2.5, 640, 360, false, true);
        assert!(audio && (total - 3.5).abs() < 1e-3);
        assert!(args.windows(2).any(|w| w[0] == "-ss" && w[1] == "2.000"), "{args:?}");
        assert!(filter.contains("overlay") && filter.ends_with("[vout]"));
    }

    #[test]
    fn export_reel() {
        // Avec ffmpeg disponible : deux clips qui se suivent, un fondu, un son ; on exporte et on relit la durée
        let Some(ff) = ffmpeg::find("ffmpeg") else { return };
        if ffmpeg::find("ffprobe").is_none() {
            return;
        }
        let dir = std::env::temp_dir().join("aryadne-montage-test");
        let _ = std::fs::create_dir_all(&dir);
        for (name, src) in [("a.mp4", "testsrc=duration=3:size=320x240:rate=24"), ("b.mp4", "smptebars=duration=3:size=320x240:rate=24")] {
            let st = ffmpeg::command(ff.clone()).args(["-v", "error", "-y", "-f", "lavfi", "-i", src, "-f", "lavfi", "-i", "sine=frequency=440:duration=3", "-shortest", "-pix_fmt", "yuv420p"]).arg(dir.join(name)).status().unwrap();
            assert!(st.success());
        }
        let file = dir.join("essai.aryav");
        let mut p = Project { width: 320, height: 240, ..Default::default() };
        let mut a = import(&file, &dir.join("a.mp4"), 1, 0.0).unwrap();
        assert!(a.has_audio && (a.media - 3.0).abs() < 0.2 && a.file == "a.mp4");
        a.src_out = 2.0;
        let mut b = import(&file, &dir.join("b.mp4"), 1, 2.0).unwrap();
        b.fade_out = 0.5;
        b.saturation = 0.3;
        p.clips = vec![a, b];
        p.write(&file).unwrap();
        // Aperçu au milieu du second clip
        let img = frame(&p, &file, 3.0, 160).unwrap();
        assert_eq!((img.width(), img.height()), (160, 120));
        assert!(img.pixels().any(|px| px.0[0] > 60));
        let out = dir.join("essai.mp4");
        export(&p, &file, &out).unwrap();
        let info = crate::video::probe(&out).unwrap();
        assert!((info.duration - 5.0).abs() < 0.3, "{}", info.duration);
        assert_eq!((info.width, info.height), (320, 240));
    }
}
