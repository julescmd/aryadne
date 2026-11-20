//! Convertir et compresser des fichiers : images (directement), vidéos et sons (ffmpeg).
//! Qualité réglable, aperçu du résultat et de la taille avant de lancer, traitement par lots.
use crate::{ffmpeg, theme};
use eframe::egui::{self, RichText};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Image,
    Video,
    Audio,
}

const IMG: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "tga", "tif", "tiff", "gif", "jfif", "exr", "hdr"];
const VID: &[&str] = &["mp4", "mov", "mkv", "webm", "avi", "m4v", "wmv", "flv", "mpg", "mpeg", "gif"];
const AUD: &[&str] = &["mp3", "wav", "flac", "ogg", "m4a", "aac", "opus", "aiff", "wma"];

pub fn kind_of(p: &Path) -> Option<Kind> {
    let e = p.extension()?.to_string_lossy().to_lowercase();
    if IMG.contains(&e.as_str()) && e != "gif" {
        Some(Kind::Image)
    } else if VID.contains(&e.as_str()) {
        Some(Kind::Video)
    } else if AUD.contains(&e.as_str()) {
        Some(Kind::Audio)
    } else {
        None
    }
}

/// Formats de sortie : (extension, description)
pub fn formats(k: Kind) -> &'static [(&'static str, &'static str)] {
    match k {
        Kind::Image => &[("jpg", "JPEG : léger, avec perte"), ("png", "PNG : sans perte, transparence"), ("webp", "WebP : sans perte, plus léger que PNG"), ("tga", "TGA : moteurs de jeu"), ("tiff", "TIFF : impression"), ("bmp", "BMP")],
        Kind::Video => &[("mp4", "MP4 H.264 : partout"), ("webm", "WebM VP9 : web, très léger"), ("mov", "MOV ProRes : montage, lourd"), ("gif", "GIF animé"), ("mp3", "son seul en MP3"), ("wav", "son seul en WAV")],
        Kind::Audio => &[("mp3", "MP3"), ("ogg", "Ogg Vorbis"), ("m4a", "AAC"), ("flac", "FLAC : sans perte"), ("wav", "WAV : sans compression")],
    }
}

fn human(b: u64) -> String {
    crate::transfer::human(b)
}

#[derive(Clone, Default)]
pub struct Job {
    pub done: usize,
    pub total: usize,
    pub current: String,
    pub progress: f32,
    pub before: u64,
    pub after: u64,
    pub errors: Vec<String>,
    pub finished: bool,
    pub outputs: Vec<PathBuf>,
}

pub struct Dlg {
    pub files: Vec<PathBuf>,
    kind: Kind,
    fmt: &'static str,
    /// 1-100
    quality: u8,
    /// Plus grand côté (images) ou hauteur (vidéos), 0 = d'origine
    size: u32,
    fps: f32,
    keep_audio: bool,
    out_dir: Option<PathBuf>,
    replace: bool,
    preview_key: String,
    preview: Option<(egui::TextureHandle, u64, u64)>,
    preview_rx: Option<std::sync::mpsc::Receiver<Result<(egui::ColorImage, u64, u64), String>>>,
    preview_err: Option<String>,
    pub job: Option<Arc<Mutex<Job>>>,
    ffmpeg_install: Option<Arc<Mutex<ffmpeg::InstallState>>>,
}

impl Dlg {
    /// `paths` : fichiers ou dossiers (les dossiers sont parcourus).
    pub fn new(paths: Vec<PathBuf>) -> Option<Dlg> {
        let mut files = vec![];
        for p in paths {
            if p.is_dir() {
                let mut stack = vec![p];
                while let Some(d) = stack.pop() {
                    for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                        let q = e.path();
                        if q.is_dir() {
                            if !e.file_name().to_string_lossy().starts_with('.') {
                                stack.push(q);
                            }
                        } else if kind_of(&q).is_some() {
                            files.push(q);
                        }
                    }
                }
            } else if kind_of(&p).is_some() {
                files.push(p);
            }
        }
        files.sort();
        // Le type le plus présent décide (les autres sont ignorés)
        let count = |k: Kind| files.iter().filter(|f| kind_of(f) == Some(k)).count();
        let kind = [Kind::Image, Kind::Video, Kind::Audio].into_iter().max_by_key(|k| count(*k))?;
        files.retain(|f| kind_of(f) == Some(kind));
        if files.is_empty() {
            return None;
        }
        let fmt = formats(kind)[0].0;
        Some(Dlg { files, kind, fmt, quality: 80, size: 0, fps: 0.0, keep_audio: true, out_dir: None, replace: false, preview_key: String::new(), preview: None, preview_rx: None, preview_err: None, job: None, ffmpeg_install: None })
    }

    fn output_for(&self, f: &Path) -> PathBuf {
        let dir = self.out_dir.clone().unwrap_or_else(|| f.parent().map(|p| p.to_path_buf()).unwrap_or_default());
        let stem = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let same = f.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref() == Some(self.fmt);
        let name = if self.replace && same { format!("{stem}.{}", self.fmt) } else { format!("{stem}-{}.{}", if same { "compressé" } else { "converti" }, self.fmt) };
        let mut out = dir.join(&name);
        if self.replace && same {
            // Remplacement : on écrit à côté puis on renomme
            out = dir.join(format!(".{name}.tmp"));
        }
        out
    }

    fn settings(&self) -> Settings {
        Settings { kind: self.kind, fmt: self.fmt, quality: self.quality, size: self.size, fps: self.fps, keep_audio: self.keep_audio }
    }

    /// Fenêtre. Retourne false quand elle est fermée.
    pub fn ui(&mut self, ctx: &egui::Context) -> bool {
        let p = theme::pal();
        let mut open = true;
        let frame = egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.text)).inner_margin(16);
        let busy = self.job.as_ref().is_some_and(|j| j.lock().map(|j| !j.finished).unwrap_or(false));
        let needs_ffmpeg = self.kind != Kind::Image && ffmpeg::find("ffmpeg").is_none();
        // Aperçu (première image, ou image tirée de la vidéo) recalculé quand les réglages changent
        let key = format!("{:?}{}{}{}", self.kind, self.fmt, self.quality, self.size);
        if self.kind == Kind::Image && key != self.preview_key && self.preview_rx.is_none() {
            self.preview_key = key;
            let (tx, rx) = std::sync::mpsc::channel();
            let (f, st, ctx2) = (self.files[0].clone(), self.settings(), ctx.clone());
            std::thread::spawn(move || {
                let r = preview_image(&f, &st);
                let _ = tx.send(r);
                ctx2.request_repaint();
            });
            self.preview_rx = Some(rx);
        }
        if let Some(rx) = &self.preview_rx {
            if let Ok(r) = rx.try_recv() {
                match r {
                    Ok((img, before, after)) => {
                        let t = ctx.load_texture("convert-preview", img, egui::TextureOptions::LINEAR);
                        self.preview = Some((t, before, after));
                        self.preview_err = None;
                    }
                    Err(e) => self.preview_err = Some(e),
                }
                self.preview_rx = None;
            }
        }
        egui::Modal::new(egui::Id::unique("convert")).frame(frame).show(ctx, |ui| {
            ui.set_width(620.0);
            ui.horizontal(|ui| {
                ui.label(crate::caps(&crate::i18n::t("convertir et compresser")));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(crate::i18n::t("fermer")).clicked() {
                        open = false;
                    }
                });
            });
            let total: u64 = self.files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum();
            let what = match self.kind {
                Kind::Image => "image(s)",
                Kind::Video => "vidéo(s)",
                Kind::Audio => "son(s)",
            };
            let names: Vec<String> = self.files.iter().take(3).filter_map(|f| f.file_name().map(|n| n.to_string_lossy().to_string())).collect();
            ui.label(RichText::new(format!("{} {what} · {} · {}{}", self.files.len(), human(total), names.join(", "), if self.files.len() > 3 { "…" } else { "" })).color(p.muted).size(11.5));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(crate::i18n::t("format"));
                for (f, help) in formats(self.kind) {
                    if ui.selectable_label(self.fmt == *f, f.to_uppercase()).on_hover_text(*help).clicked() {
                        self.fmt = f;
                    }
                }
            });
            let lossy = matches!(self.fmt, "jpg" | "mp4" | "webm" | "mp3" | "ogg" | "m4a" | "gif");
            ui.spacing_mut().slider_width = 260.0;
            if lossy {
                ui.add(egui::Slider::new(&mut self.quality, 1..=100).text(crate::i18n::t("qualité"))).on_hover_text(crate::i18n::t("100 : presque sans perte, fichier lourd. 60-80 : bon compromis."));
            }
            match self.kind {
                Kind::Image => {
                    ui.horizontal(|ui| {
                        ui.label(crate::i18n::t("plus grand côté"));
                        for (v, l) in [(0u32, "original"), (4096, "4096"), (2048, "2048"), (1024, "1024"), (512, "512")] {
                            if ui.selectable_label(self.size == v, l).clicked() {
                                self.size = v;
                            }
                        }
                        ui.add(egui::DragValue::new(&mut self.size).range(0..=16384).suffix(" px"));
                    });
                }
                Kind::Video => {
                    if self.fmt != "mp3" && self.fmt != "wav" {
                        ui.horizontal(|ui| {
                            ui.label(crate::i18n::t("hauteur"));
                            for (v, l) in [(0u32, "originale"), (2160, "2160"), (1080, "1080"), (720, "720"), (480, "480")] {
                                if ui.selectable_label(self.size == v, l).clicked() {
                                    self.size = v;
                                }
                            }
                            ui.add(egui::DragValue::new(&mut self.size).range(0..=8640).suffix(" px"));
                        });
                        ui.horizontal(|ui| {
                            ui.label(crate::i18n::t("cadence"));
                            for (v, l) in [(0.0f32, "originale"), (24.0, "24"), (25.0, "25"), (30.0, "30"), (12.0, "12")] {
                                if ui.selectable_label(self.fps == v, l).clicked() {
                                    self.fps = v;
                                }
                            }
                            ui.add(egui::DragValue::new(&mut self.fps).range(0.0..=240.0).speed(0.5).suffix(" i/s"));
                        });
                        if self.fmt != "gif" {
                            ui.checkbox(&mut self.keep_audio, crate::i18n::t("garder le son"));
                        }
                    }
                }
                Kind::Audio => {}
            }
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("enregistrer"));
                let label = match &self.out_dir {
                    Some(d) => d.display().to_string(),
                    None => "à côté de chaque fichier".into(),
                };
                if ui.button(label).clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        self.out_dir = Some(d);
                    }
                }
                if self.out_dir.is_some() && ui.small_button("×").clicked() {
                    self.out_dir = None;
                }
            });
            let same_fmt = self.files.iter().all(|f| f.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref() == Some(self.fmt));
            if same_fmt {
                ui.checkbox(&mut self.replace, crate::i18n::t("remplacer les originaux")).on_hover_text(crate::i18n::t("Sinon, le résultat est enregistré à côté avec « -compressé »"));
            }
            // Aperçu de qualité et taille estimée
            if self.kind == Kind::Image {
                ui.add_space(6.0);
                match &self.preview {
                    Some((t, before, after)) => {
                        let s = t.size_vec2();
                        let k = (600.0 / s.x).min(260.0 / s.y).min(1.0);
                        ui.add(egui::Image::new((t.id(), s * k)));
                        let pct = if *before > 0 { *after as f32 / *before as f32 * 100.0 } else { 100.0 };
                        let col = if *after <= *before { p.accent } else { p.missing };
                        ui.label(RichText::new(format!("1re image : {} → {} ({pct:.0} %)", human(*before), human(*after))).color(col).size(11.5));
                    }
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new(crate::i18n::t("aperçu…")).color(p.muted).size(11.0));
                        });
                    }
                }
                if let Some(e) = &self.preview_err {
                    ui.label(RichText::new(e).color(p.missing).size(11.0));
                }
            }
            if needs_ffmpeg {
                ui.add_space(6.0);
                match &self.ffmpeg_install {
                    None => {
                        ui.label(RichText::new(crate::i18n::t("Les vidéos et les sons passent par ffmpeg (gratuit, installé sans admin).")).color(p.muted).size(11.5));
                        if ui.button(crate::i18n::t("installer ffmpeg")).clicked() {
                            self.ffmpeg_install = Some(ffmpeg::install(ctx.clone()));
                        }
                    }
                    Some(st) => {
                        let s = st.lock().map(|s| s.clone()).unwrap_or_default();
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new(s.error.clone().unwrap_or(s.message.clone())).size(11.5));
                        });
                    }
                }
            }
            ui.add_space(10.0);
            // Travail en cours
            if let Some(j) = &self.job {
                let j = j.lock().map(|j| j.clone()).unwrap_or_default();
                let f = if j.total > 0 { (j.done as f32 + j.progress) / j.total as f32 } else { 0.0 };
                ui.add(egui::ProgressBar::new(f).corner_radius(0.0).fill(p.accent).text(format!("{} / {} · {}", j.done, j.total, j.current)));
                if j.finished {
                    let saved = j.before as i64 - j.after as i64;
                    let t = if saved >= 0 { format!("✓ terminé : {} → {} ({} gagnés)", human(j.before), human(j.after), human(saved as u64)) } else { format!("✓ terminé : {} → {}", human(j.before), human(j.after)) };
                    ui.label(RichText::new(t).color(p.accent).size(11.5));
                    if let Some(first) = j.outputs.first() {
                        if ui.button(crate::i18n::t("montrer le résultat")).clicked() {
                            crate::settings::reveal(first, true);
                        }
                    }
                }
                for e in j.errors.iter().take(5) {
                    ui.label(RichText::new(e).color(p.missing).size(11.0));
                }
            }
            let go = egui::Button::new(RichText::new(if busy { "en cours…" } else { "lancer" }).color(p.on_accent).family(theme::bold())).fill(p.accent).stroke(egui::Stroke::NONE);
            if ui.add_enabled(!busy && !(needs_ffmpeg), go).clicked() {
                self.start(ctx.clone());
            }
        });
        open || busy
    }

    fn start(&mut self, ctx: egui::Context) {
        let job = Arc::new(Mutex::new(Job { total: self.files.len(), ..Default::default() }));
        self.job = Some(job.clone());
        let st = self.settings();
        let work: Vec<(PathBuf, PathBuf)> = self.files.iter().map(|f| (f.clone(), self.output_for(f))).collect();
        let replace = self.replace;
        std::thread::spawn(move || {
            for (i, (src, out)) in work.into_iter().enumerate() {
                if let Ok(mut j) = job.lock() {
                    j.current = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    j.progress = 0.0;
                }
                ctx.request_repaint();
                let before = std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
                let r = match st.kind {
                    Kind::Image => convert_image(&src, &out, &st).map(|_| ()),
                    _ => convert_media(&src, &out, &st, &job, &ctx),
                };
                let mut final_out = out.clone();
                if r.is_ok() && replace {
                    // Fichier temporaire « .nom.tmp » → nom d'origine
                    let name = out.file_name().map(|n| n.to_string_lossy().trim_start_matches('.').trim_end_matches(".tmp").to_string()).unwrap_or_default();
                    final_out = out.with_file_name(name);
                    let _ = std::fs::rename(&out, &final_out);
                }
                let after = std::fs::metadata(&final_out).map(|m| m.len()).unwrap_or(0);
                if let Ok(mut j) = job.lock() {
                    j.done = i + 1;
                    match r {
                        Ok(()) => {
                            j.before += before;
                            j.after += after;
                            j.outputs.push(final_out);
                        }
                        Err(e) => {
                            let m = format!("{} : {e}", j.current);
                            j.errors.push(m);
                        }
                    }
                }
                ctx.request_repaint();
            }
            if let Ok(mut j) = job.lock() {
                j.finished = true;
                j.current.clear();
            }
            ctx.request_repaint();
        });
    }
}

#[derive(Clone, Copy)]
pub struct Settings {
    pub kind: Kind,
    pub fmt: &'static str,
    pub quality: u8,
    pub size: u32,
    pub fps: f32,
    pub keep_audio: bool,
}

fn encode_image(img: &image::DynamicImage, st: &Settings) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    let mut img = img.clone();
    if st.size > 0 && img.width().max(img.height()) > st.size {
        img = img.resize(st.size, st.size, image::imageops::FilterType::Lanczos3);
    }
    let mut buf = vec![];
    let e = |e: image::ImageError| e.to_string();
    match st.fmt {
        "jpg" => {
            let rgb = img.to_rgb8();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, st.quality.clamp(1, 100)).encode_image(&rgb).map_err(e)?;
        }
        "png" => {
            let enc = image::codecs::png::PngEncoder::new_with_quality(&mut buf, image::codecs::png::CompressionType::Best, image::codecs::png::FilterType::Adaptive);
            let rgba = if img.color().has_alpha() { img.to_rgba8() } else { image::DynamicImage::ImageRgb8(img.to_rgb8()).to_rgba8() };
            if img.color().has_alpha() {
                enc.write_image(rgba.as_raw(), rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8).map_err(e)?;
            } else {
                let rgb = img.to_rgb8();
                enc.write_image(rgb.as_raw(), rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8).map_err(e)?;
            }
        }
        "webp" => {
            let rgba = img.to_rgba8();
            image::codecs::webp::WebPEncoder::new_lossless(&mut buf).write_image(rgba.as_raw(), rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8).map_err(e)?;
        }
        "tga" => img.to_rgba8().write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Tga).map_err(e)?,
        "tiff" => img.to_rgba8().write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Tiff).map_err(e)?,
        "bmp" => img.to_rgb8().write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Bmp).map_err(e)?,
        f => return Err(format!("format {f} non pris en charge")),
    }
    Ok(buf)
}

pub fn convert_image(src: &Path, out: &Path, st: &Settings) -> Result<u64, String> {
    let img = image::open(src).map_err(|e| e.to_string())?;
    let buf = encode_image(&img, st)?;
    std::fs::write(out, &buf).map_err(|e| e.to_string())?;
    Ok(buf.len() as u64)
}

/// Encode la première image pour montrer la qualité et la taille obtenues.
fn preview_image(src: &Path, st: &Settings) -> Result<(egui::ColorImage, u64, u64), String> {
    let img = image::open(src).map_err(|e| e.to_string())?;
    let before = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    let buf = encode_image(&img, st)?;
    let back = image::load_from_memory(&buf).map_err(|e| e.to_string())?;
    let back = if back.width().max(back.height()) > 1200 { back.resize(1200, 1200, image::imageops::FilterType::Triangle) } else { back };
    let rgba = back.to_rgba8();
    Ok((egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()), before, buf.len() as u64))
}

/// Arguments ffmpeg selon le format et la qualité (1-100).
pub fn ffmpeg_args(st: &Settings) -> Vec<String> {
    let q = st.quality.clamp(1, 100) as f32 / 100.0;
    let mut a: Vec<String> = vec![];
    let mut vf: Vec<String> = vec![];
    if st.size > 0 && !matches!(st.fmt, "mp3" | "wav" | "ogg" | "m4a" | "flac") {
        vf.push(format!("scale=-2:{}", st.size));
    }
    if st.fps > 0.0 {
        vf.push(format!("fps={}", st.fps));
    }
    let s = |x: &str| x.to_string();
    match st.fmt {
        "mp4" => {
            let crf = (35.0 - q * 18.0).round() as i32; // 100 % → 17, 50 % → 26
            a.extend([s("-c:v"), s("libx264"), s("-preset"), s("veryfast"), s("-crf"), crf.to_string(), s("-pix_fmt"), s("yuv420p"), s("-movflags"), s("+faststart")]);
            if st.keep_audio && st.kind == Kind::Video {
                a.extend([s("-c:a"), s("aac"), s("-b:a"), format!("{}k", (96.0 + q * 160.0) as i32)]);
            } else {
                a.push(s("-an"));
            }
        }
        "webm" => {
            let crf = (50.0 - q * 30.0).round() as i32;
            a.extend([s("-c:v"), s("libvpx-vp9"), s("-crf"), crf.to_string(), s("-b:v"), s("0"), s("-row-mt"), s("1"), s("-deadline"), s("good"), s("-cpu-used"), s("4")]);
            if st.keep_audio {
                a.extend([s("-c:a"), s("libopus"), s("-b:a"), format!("{}k", (64.0 + q * 96.0) as i32)]);
            } else {
                a.push(s("-an"));
            }
        }
        "mov" => {
            a.extend([s("-c:v"), s("prores_ks"), s("-profile:v"), s("2")]);
            if st.keep_audio {
                a.extend([s("-c:a"), s("pcm_s16le")]);
            } else {
                a.push(s("-an"));
            }
        }
        "gif" => {
            let colors = (32.0 + q * 224.0) as i32;
            let pre = if vf.is_empty() { String::new() } else { format!("{},", vf.join(",")) };
            vf.clear();
            a.extend([s("-filter_complex"), format!("[0:v]{pre}split[a][b];[a]palettegen=max_colors={colors}[p];[b][p]paletteuse=dither=sierra2_4a"), s("-an")]);
        }
        "mp3" => a.extend([s("-vn"), s("-c:a"), s("libmp3lame"), s("-q:a"), ((1.0 - q) * 9.0).round().to_string()]),
        "ogg" => a.extend([s("-vn"), s("-c:a"), s("libvorbis"), s("-q:a"), (q * 10.0).round().to_string()]),
        "m4a" => a.extend([s("-vn"), s("-c:a"), s("aac"), s("-b:a"), format!("{}k", (64.0 + q * 256.0) as i32)]),
        "flac" => a.extend([s("-vn"), s("-c:a"), s("flac")]),
        "wav" => a.extend([s("-vn"), s("-c:a"), s("pcm_s16le")]),
        _ => {}
    }
    if !vf.is_empty() {
        a.extend([s("-vf"), vf.join(",")]);
    }
    a
}

fn convert_media(src: &Path, out: &Path, st: &Settings, job: &Arc<Mutex<Job>>, ctx: &egui::Context) -> Result<(), String> {
    let ff = ffmpeg::find("ffmpeg").ok_or("ffmpeg introuvable")?;
    let duration = crate::video::probe(src).map(|i| i.duration).unwrap_or(0.0);
    let mut cmd = ffmpeg::command(ff);
    cmd.args(["-y", "-hide_banner", "-loglevel", "error", "-nostats", "-progress", "pipe:1", "-i"]).arg(src).args(ffmpeg_args(st)).arg(out);
    let mut child = cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    if let Some(o) = child.stdout.take() {
        for line in BufReader::new(o).lines().map_while(Result::ok) {
            if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.trim().parse::<f64>().ok()) {
                if duration > 0.0 {
                    if let Ok(mut j) = job.lock() {
                        j.progress = ((us / 1e6) as f32 / duration).clamp(0.0, 1.0);
                    }
                    ctx.request_repaint();
                }
            }
        }
    }
    let outp = child.wait_with_output().map_err(|e| e.to_string())?;
    if !outp.status.success() {
        let err = String::from_utf8_lossy(&outp.stderr);
        return Err(err.lines().last().unwrap_or("échec de ffmpeg").to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_et_qualite() {
        let dir = std::env::temp_dir().join(format!("aryadne-conv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("a.png");
        let img = image::RgbaImage::from_fn(640, 480, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, ((x * y) % 256) as u8, 255]));
        img.save(&src).unwrap();
        let st = |fmt, quality, size| Settings { kind: Kind::Image, fmt, quality, size, fps: 0.0, keep_audio: true };
        let hi = convert_image(&src, &dir.join("hi.jpg"), &st("jpg", 95, 0)).unwrap();
        let lo = convert_image(&src, &dir.join("lo.jpg"), &st("jpg", 20, 0)).unwrap();
        assert!(lo < hi, "qualité basse = plus léger");
        convert_image(&src, &dir.join("s.png"), &st("png", 80, 320)).unwrap();
        assert_eq!(image::open(dir.join("s.png")).unwrap().width(), 320);
        for f in ["webp", "tga", "tiff", "bmp"] {
            convert_image(&src, &dir.join(format!("x.{f}")), &st(f, 80, 0)).unwrap();
        }
        let d = Dlg::new(vec![dir.clone()]).unwrap();
        assert_eq!(d.kind, Kind::Image);
        assert!(d.files.len() >= 6);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn video_reelle() {
        if ffmpeg::find("ffmpeg").is_none() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("aryadne-convv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("in.mp4");
        let ok = ffmpeg::command(ffmpeg::find("ffmpeg").unwrap()).args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=24:duration=1", "-pix_fmt", "yuv420p"]).arg(&src).status().unwrap();
        assert!(ok.success());
        let job = Arc::new(Mutex::new(Job::default()));
        let ctx = egui::Context::default();
        for fmt in ["mp4", "webm", "gif"] {
            let st = Settings { kind: Kind::Video, fmt, quality: 60, size: 120, fps: 12.0, keep_audio: false };
            let out = dir.join(format!("o.{fmt}"));
            convert_media(&src, &out, &st, &job, &ctx).unwrap_or_else(|e| panic!("{fmt} : {e}"));
            assert!(std::fs::metadata(&out).unwrap().len() > 100, "{fmt}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn arguments_ffmpeg() {
        let st = Settings { kind: Kind::Video, fmt: "mp4", quality: 100, size: 720, fps: 24.0, keep_audio: false };
        let a = ffmpeg_args(&st).join(" ");
        assert!(a.contains("-crf 17") && a.contains("scale=-2:720,fps=24") && a.contains("-an"), "{a}");
    }
}
