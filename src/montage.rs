//! Montage vidéo (`.aryseq`) : des pistes, des clips qu'on déplace, raccourcit et coupe,
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

pub fn is_project(rel: &str) -> bool {
    crate::formats::has(rel, "seq")
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
    /// Étalonnage en nodes : fichier de compositing appliqué à ce clip (vide = aucun)
    #[serde(default)]
    pub grade: String,
    /// Clés d'animation du placement, de la taille et de l'opacité, triées par instant
    #[serde(default)]
    pub keys: Vec<PoseKey>,
    /// Titre (clip de genre « text ») : le texte, sa taille en pixels du projet, sa couleur, sa police, son contour
    #[serde(default)]
    pub text: String,
    #[serde(default = "text_size")]
    pub text_size: f32,
    #[serde(default = "white")]
    pub text_color: [u8; 3],
    /// « lato », « lato-bold », « mono », ou le chemin d'un fichier de police
    #[serde(default)]
    pub font: String,
    #[serde(default)]
    pub outline: bool,
    /// Fondu enchaîné avec le clip qui précède sur la même piste, en secondes (0 : aucun)
    #[serde(default)]
    pub xfade: f32,
}

fn text_size() -> f32 {
    72.0
}
fn white() -> [u8; 3] {
    [255; 3]
}

/// Dessine un titre sur une image transparente de la taille du projet, centré.
pub fn render_text(c: &Clip, w: u32, h: u32) -> image::RgbaImage {
    use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
    let mut img = image::RgbaImage::new(w.max(1), h.max(1));
    let font = match c.font.as_str() {
        "" | "lato-bold" => FontArc::try_from_slice(include_bytes!("../assets/fonts/Lato-Bold.ttf")).ok(),
        "lato" => FontArc::try_from_slice(include_bytes!("../assets/fonts/Lato-Regular.ttf")).ok(),
        "mono" => FontArc::try_from_slice(include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf")).ok(),
        path => std::fs::read(path).ok().and_then(|b| FontArc::try_from_vec(b).ok()).or_else(|| FontArc::try_from_slice(include_bytes!("../assets/fonts/Lato-Bold.ttf")).ok()),
    };
    let Some(font) = font else { return img };
    let size = c.text_size.clamp(4.0, 2000.0);
    let sf = font.as_scaled(PxScale::from(size));
    let line_h = sf.height() + sf.line_gap();
    let lines: Vec<&str> = c.text.lines().collect();
    let width = |l: &str| {
        let mut x = 0.0;
        let mut prev = None;
        for ch in l.chars() {
            let g = sf.glyph_id(ch);
            if let Some(p) = prev {
                x += sf.kern(p, g);
            }
            x += sf.h_advance(g);
            prev = Some(g);
        }
        x
    };
    let top = (h as f32 - line_h * lines.len() as f32) / 2.0;
    // Contour : le même texte en noir, décalé tout autour, sous le texte en couleur
    let reach = if c.outline { (size / 18.0).max(1.5) } else { 0.0 };
    let mut passes: Vec<(f32, f32, [u8; 3])> = vec![];
    if c.outline {
        for k in 0..16 {
            let a = k as f32 / 16.0 * std::f32::consts::TAU;
            passes.push((a.cos() * reach, a.sin() * reach, [0, 0, 0]));
        }
    }
    passes.push((0.0, 0.0, c.text_color));
    for (dx, dy, col) in passes {
        for (li, l) in lines.iter().enumerate() {
            let mut x = (w as f32 - width(l)) / 2.0 + dx;
            let y = top + li as f32 * line_h + sf.ascent() + dy;
            let mut prev = None;
            for ch in l.chars() {
                let id = sf.glyph_id(ch);
                if let Some(p) = prev {
                    x += sf.kern(p, id);
                }
                let glyph = id.with_scale_and_position(PxScale::from(size), ab_glyph::point(x, y));
                x += sf.h_advance(id);
                prev = Some(id);
                if let Some(o) = font.outline_glyph(glyph) {
                    let b = o.px_bounds();
                    o.draw(|gx, gy, cov| {
                        let (px, py) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                        if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 || cov <= 0.0 {
                            return;
                        }
                        let d = img.get_pixel_mut(px as u32, py as u32);
                        let a = cov.min(1.0);
                        let da = d.0[3] as f32 / 255.0;
                        let oa = a + da * (1.0 - a);
                        for k in 0..3 {
                            d.0[k] = ((col[k] as f32 * a + d.0[k] as f32 * da * (1.0 - a)) / oa.max(1e-6)) as u8;
                        }
                        d.0[3] = (oa * 255.0) as u8;
                    });
                }
            }
        }
    }
    img
}

/// Image d'un titre, gardée en cache : son nom change dès que le texte ou son allure change.
pub fn text_file(c: &Clip, p: &Project) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (&c.text, c.text_size.to_bits(), c.text_color, &c.font, c.outline, p.width, p.height).hash(&mut h);
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("aryadne").join("titres").join(format!("{:016x}.png", h.finish()))
}

/// Dessine les titres qui n'ont pas encore leur image. Renvoie vrai si au moins une a été écrite.
pub fn bake_texts(p: &Project) -> bool {
    let mut any = false;
    for c in p.clips.iter().filter(|c| c.kind == "text") {
        let out = text_file(c, p);
        if out.exists() {
            continue;
        }
        if let Some(d) = out.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        any |= render_text(c, p.width, p.height).save(&out).is_ok();
    }
    any
}

/// Une clé : placement, taille et opacité du clip à un instant (en secondes depuis son début).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct PoseKey {
    pub t: f32,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub opacity: f32,
}

/// Expression ffmpeg qui suit des points (instant, valeur) en ligne droite de l'un à l'autre.
pub fn expr(points: &[(f32, f32)], var: &str) -> String {
    match points {
        [] => "0".into(),
        [(_, v)] => format!("{v:.4}"),
        _ => {
            let mut e = format!("{:.4}", points[points.len() - 1].1);
            for w in points.windows(2).rev() {
                let ((t0, v0), (t1, v1)) = (w[0], w[1]);
                e = format!("if(lt({var},{t1:.4}),{v0:.4}+({:.4})*({var}-({t0:.4}))/{:.4},{e})", v1 - v0, (t1 - t0).max(1e-4));
            }
            format!("if(lt({var},{:.4}),{:.4},{e})", points[0].0, points[0].1)
        }
    }
}

/// Fichier de la table de couleurs d'un étalonnage : change de nom dès que le graphe est modifié.
pub fn lut_file(grade: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    grade.hash(&mut h);
    std::fs::metadata(grade).ok().and_then(|m| m.modified().ok()).hash(&mut h);
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("aryadne").join("lut").join(format!("{:016x}.cube", h.finish()))
}

/// Calcule les tables de couleurs manquantes. Renvoie vrai si au moins une a été écrite.
pub fn bake_luts(p: &Project, project: &Path, root: &str) -> bool {
    let mut any = false;
    for c in p.clips.iter().filter(|c| !c.grade.is_empty()) {
        let g = resolve(project, &c.grade);
        let out = lut_file(&g);
        if out.exists() || !g.exists() {
            continue;
        }
        if let Ok(prog) = crate::nodes::Graph::load(&g).and_then(|gr| crate::nodes::Program::compile(&gr, root, 64).map_err(|e| e.1)) {
            if let Some(d) = out.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            any |= std::fs::write(&out, prog.lut_cube(33)).is_ok();
        }
    }
    any
}

impl Clip {
    /// Placement, taille et opacité à l'instant `t` de la timeline, d'après les clés (None sans clé).
    pub fn pose_at(&self, t: f32) -> Option<PoseKey> {
        let (k, r) = (&self.keys, t - self.start);
        let (first, last) = (k.first()?, k.last()?);
        if r <= first.t {
            return Some(*first);
        }
        if r >= last.t {
            return Some(*last);
        }
        let i = (0..k.len() - 1).find(|&i| r < k[i + 1].t)?;
        let (a, b) = (k[i], k[i + 1]);
        let f = (r - a.t) / (b.t - a.t).max(1e-4);
        let m = |x: f32, y: f32| x + (y - x) * f;
        Some(PoseKey { t: r, x: m(a.x, b.x), y: m(a.y, b.y), scale: m(a.scale, b.scale), opacity: m(a.opacity, b.opacity) })
    }

    /// Pose une clé à l'instant `t` de la timeline avec les réglages actuels.
    pub fn set_key(&mut self, t: f32) {
        let r = ((t - self.start).clamp(0.0, self.len()) * 100.0).round() / 100.0;
        let key = PoseKey { t: r, x: self.x, y: self.y, scale: self.scale, opacity: self.opacity };
        match self.keys.iter().position(|k| (k.t - r).abs() < 0.021) {
            Some(i) => self.keys[i] = PoseKey { t: self.keys[i].t, ..key },
            None => {
                self.keys.push(key);
                self.keys.sort_by(|a, b| a.t.total_cmp(&b.t));
            }
        }
    }

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

    /// Fondu enchaîné : le clip `i` recule pour chevaucher de `d` secondes la fin du clip qui le précède sur sa piste.
    /// Renvoie faux s'il n'y a pas de clip avant lui.
    pub fn crossfade(&mut self, i: usize, d: f32) -> bool {
        let Some(c) = self.clips.get(i).cloned() else { return false };
        let prev = self.clips.iter().enumerate().filter(|(k, a)| *k != i && a.track == c.track && a.start < c.start).map(|(_, a)| a.end()).fold(f32::MIN, f32::max);
        if prev == f32::MIN {
            return false;
        }
        let d = d.clamp(0.05, c.len() * 0.9);
        self.clips[i].start = (prev - d).max(0.0);
        self.clips[i].xfade = d;
        true
    }

    /// Le clip `i` en chevauche-t-il un autre sur sa piste (au-delà d'un fondu enchaîné voulu) ?
    fn collides(&self, i: usize, track: usize) -> bool {
        let c = &self.clips[i];
        self.clips.iter().enumerate().any(|(k, o)| {
            if k == i || o.track != track {
                return false;
            }
            let over = c.end().min(o.end()) - c.start.max(o.start);
            // Le clip qui commence le second porte le fondu
            let allowed = if c.start >= o.start { c.xfade } else { o.xfade };
            over > allowed + 1e-3
        })
    }

    /// Si le clip `i` en recouvre un autre, il monte sur la première piste libre (au besoin une nouvelle) :
    /// deux clips ne se cachent jamais l'un l'autre sur une même piste.
    pub fn settle(&mut self, i: usize) -> bool {
        if i >= self.clips.len() || !self.collides(i, self.clips[i].track) {
            return false;
        }
        let from = self.clips[i].track;
        let mut t = from + 1;
        while t < self.tracks && self.collides(i, t) {
            t += 1;
        }
        if t >= self.tracks {
            self.tracks = t + 1;
        }
        self.clips[i].track = t;
        true
    }

    /// Range tous les clips qui se recouvrent (projets plus anciens).
    pub fn settle_all(&mut self) -> bool {
        let mut moved = false;
        for i in 0..self.clips.len() {
            moved |= self.settle(i);
        }
        moved
    }

    /// Ajoute un titre de quatre secondes sur la piste du haut, à l'instant `t`.
    pub fn add_text(&mut self, t: f32) -> usize {
        self.clips.push(Clip {
            name: "titre".into(),
            file: String::new(),
            kind: "text".into(),
            track: self.tracks.saturating_sub(1),
            start: t,
            src_in: 0.0,
            src_out: 4.0,
            media: 0.0,
            has_audio: false,
            speed: 1.0,
            opacity: 1.0,
            volume: 1.0,
            fade_in: 0.3,
            fade_out: 0.3,
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            grade: String::new(),
            keys: vec![],
            text: "Titre".into(),
            text_size: (self.height as f32 / 9.0).round(),
            text_color: [255; 3],
            font: String::new(),
            outline: true,
            xfade: 0.0,
        });
        self.clips.len() - 1
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
        grade: String::new(),
        keys: vec![],
        text: String::new(),
        text_size: 72.0,
        text_color: [255; 3],
        font: String::new(),
        outline: false,
        xfade: 0.0,
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
        let path = if c.kind == "text" { text_file(c, p) } else { resolve(project, &c.file) }.to_string_lossy().to_string();
        if c.kind == "image" || c.kind == "text" {
            args.extend(["-loop".into(), "1".into(), "-t".into(), format!("{:.3}", en - st), "-i".into(), path]);
        } else {
            args.extend(["-ss".into(), format!("{src_in:.3}"), "-t".into(), format!("{src_len:.3}"), "-i".into(), path]);
        }
        if c.visual() && (c.opacity > 0.0 || !c.keys.is_empty()) {
            // Clés : chaque réglage animé devient une expression du temps de la timeline
            let pts = |f: &dyn Fn(&PoseKey) -> f32| -> Vec<(f32, f32)> { c.keys.iter().map(|q| (c.start + q.t - from, f(q))).collect() };
            let animated = c.keys.len() > 1;
            filter += &format!("[{n}:v]fps={},setpts=(PTS-STARTPTS)/{:.4}+{st:.3}/TB", p.fps, c.speed.max(0.01));
            if !c.grade.is_empty() {
                let lut = lut_file(&resolve(project, &c.grade));
                if lut.exists() {
                    filter += &format!(",lut3d=file='{}'", lut.to_string_lossy().replace('\\', "/").replace(':', "\\:"));
                }
            }
            if animated {
                let e = expr(&pts(&|q| q.scale), "t");
                filter += &format!(",scale=w='max(2,{w}*({e}))':h='max(2,{h}*({e}))':eval=frame:force_original_aspect_ratio=decrease");
            } else {
                let sc = c.keys.first().map(|q| q.scale).unwrap_or(c.scale);
                let (cw, ch) = (((w as f32 * sc).round() as u32).max(2), ((h as f32 * sc).round() as u32).max(2));
                filter += &format!(",scale=w={cw}:h={ch}:force_original_aspect_ratio=decrease");
            }
            if c.brightness != 0.0 || c.contrast != 1.0 || c.saturation != 1.0 {
                filter += &format!(",eq=brightness={:.3}:contrast={:.3}:saturation={:.3}", c.brightness.clamp(-1.0, 1.0), c.contrast.clamp(0.0, 4.0), c.saturation.clamp(0.0, 3.0));
            }
            filter += ",format=rgba";
            if animated {
                if c.keys.iter().any(|q| q.opacity < 1.0) {
                    filter += &format!(",geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*clip({},0,1)'", expr(&pts(&|q| q.opacity), "T"));
                }
            } else {
                let op = c.keys.first().map(|q| q.opacity).unwrap_or(c.opacity);
                if op < 1.0 {
                    filter += &format!(",colorchannelmixer=aa={:.3}", op.clamp(0.0, 1.0));
                }
            }
            // Fondu enchaîné : ce clip apparaît par-dessus la fin du précédent
            let fin = c.fade_in.max(c.xfade);
            if fin > 0.0 && skipped == 0.0 {
                filter += &format!(",fade=t=in:st={st:.3}:d={fin:.3}:alpha=1");
            }
            if c.fade_out > 0.0 {
                filter += &format!(",fade=t=out:st={:.3}:d={:.3}:alpha=1", (en - c.fade_out).max(0.0), c.fade_out);
            }
            let (ex, ey) = if animated { (expr(&pts(&|q| q.x * k), "t"), expr(&pts(&|q| q.y * k), "t")) } else { (format!("{:.1}", c.keys.first().map(|q| q.x).unwrap_or(c.x) * k), format!("{:.1}", c.keys.first().map(|q| q.y).unwrap_or(c.y) * k)) };
            filter += &format!("[c{n}];[{last}][c{n}]overlay=x='(W-w)/2+({ex})':y='(H-h)/2+({ey})':enable='between(t,{st:.3},{en:.3})':eof_action=pass[o{n}];");
            last = format!("o{n}");
        }
        if with_audio && c.has_audio && c.volume > 0.0 && c.kind != "image" && c.kind != "text" {
            filter += &format!("[{n}:a]asetpts=PTS-STARTPTS,atempo={:.4},volume={:.3}", c.speed.clamp(0.5, 2.0), c.volume);
            // Le son suit les fondus de l'image, et s'efface sous un clip qui arrive en fondu enchaîné
            let fin = c.fade_in.max(c.xfade);
            if fin > 0.0 && skipped == 0.0 {
                filter += &format!(",afade=t=in:st=0:d={fin:.3}");
            }
            let next = p.clips.iter().filter(|b| b.track == c.track && b.xfade > 0.0 && b.start > c.start && b.start < c.end()).map(|b| (b.start, b.xfade)).min_by(|a, b| a.0.total_cmp(&b.0));
            let out = match next {
                Some((bs, d)) => Some((bs - c.start - skipped, d)),
                None if c.fade_out > 0.0 => Some((en - st - c.fade_out, c.fade_out)),
                None => None,
            };
            if let Some((at, d)) = out {
                filter += &format!(",afade=t=out:st={:.3}:d={d:.3}", at.max(0.0));
            }
            filter += &format!(",adelay={}:all=1[a{n}];", (st * 1000.0) as u64);
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
    bake_texts(p);
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
    bake_texts(p);
    let (inputs, filter, _, _) = graph(p, project, t, w, h, false, false);
    let o = ffmpeg::command(ff).args(["-v", "error"]).args(inputs).args(["-filter_complex", &filter, "-map", "[vout]", "-frames:v", "1", "-f", "image2pipe", "-vcodec", "png", "-"]).output().map_err(|e| e.to_string())?;
    image::load_from_memory(&o.stdout).map(|i| i.to_rgba8()).map_err(|_| format!("aperçu impossible : {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("?")))
}

/// Vignettes et formes d'onde des clips, calculées en arrière-plan par ffmpeg et gardées en mémoire.
#[derive(Default)]
struct Strips {
    /// (fichier, temps en dixièmes de seconde) → vignette
    thumbs: std::collections::HashMap<(PathBuf, i64), Option<egui::TextureHandle>>,
    /// fichier → pics (0..1), 50 par seconde
    waves: std::collections::HashMap<PathBuf, Option<std::sync::Arc<Vec<f32>>>>,
    tx: Option<std::sync::mpsc::Sender<StripMsg>>,
    rx: Option<Receiver<StripMsg>>,
    busy: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

enum StripMsg {
    Thumb((PathBuf, i64), Option<egui::ColorImage>),
    Wave(PathBuf, Option<Vec<f32>>),
}

const WAVE_RATE: f32 = 50.0;

impl Strips {
    fn channel(&mut self) -> std::sync::mpsc::Sender<StripMsg> {
        if self.tx.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            self.tx = Some(tx);
            self.rx = Some(rx);
        }
        self.tx.clone().unwrap()
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.rx else { return };
        while let Ok(m) = rx.try_recv() {
            match m {
                StripMsg::Thumb(k, img) => {
                    let tex = img.map(|i| ctx.load_texture(format!("strip-{}-{}", k.0.to_string_lossy(), k.1), i, egui::TextureOptions::LINEAR));
                    self.thumbs.insert(k, tex);
                }
                StripMsg::Wave(f, w) => {
                    self.waves.insert(f, w.map(std::sync::Arc::new));
                }
            }
        }
    }

    /// Vignette d'un média à un instant (arrondi au dixième de seconde) ; lancée si besoin
    fn thumb(&mut self, file: &Path, t: f32, ctx: &egui::Context) -> Option<egui::TextureId> {
        let key = (file.to_path_buf(), (t * 10.0).round() as i64);
        if let Some(v) = self.thumbs.get(&key) {
            return v.as_ref().map(|t| t.id());
        }
        // Pas plus de trois ffmpeg à la fois
        if self.busy.load(std::sync::atomic::Ordering::Relaxed) >= 3 {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
            return None;
        }
        let Some(ff) = ffmpeg::find("ffmpeg") else { return None };
        self.thumbs.insert(key.clone(), None);
        let (tx, busy, c2) = (self.channel(), self.busy.clone(), ctx.clone());
        busy.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::thread::spawn(move || {
            let o = ffmpeg::command(ff).args(["-v", "error", "-ss", &format!("{:.2}", key.1 as f32 / 10.0), "-i"]).arg(&key.0).args(["-frames:v", "1", "-vf", "scale=-2:64", "-f", "image2pipe", "-vcodec", "png", "-"]).output();
            let img = o.ok().and_then(|o| image::load_from_memory(&o.stdout).ok()).map(|i| {
                let i = i.to_rgba8();
                egui::ColorImage::from_rgba_unmultiplied([i.width() as usize, i.height() as usize], i.as_raw())
            });
            let _ = tx.send(StripMsg::Thumb(key, img));
            busy.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            c2.request_repaint();
        });
        None
    }

    /// Forme d'onde d'un média (pics par 1/50 s) ; lancée si besoin
    fn wave(&mut self, file: &Path, ctx: &egui::Context) -> Option<std::sync::Arc<Vec<f32>>> {
        if let Some(v) = self.waves.get(file) {
            return v.clone();
        }
        let ff = ffmpeg::find("ffmpeg")?;
        self.waves.insert(file.to_path_buf(), None);
        let (tx, c2, f) = (self.channel(), ctx.clone(), file.to_path_buf());
        std::thread::spawn(move || {
            // Son mono à 4 kHz, puis le pic de chaque tranche de 1/50 s
            let o = ffmpeg::command(ff).args(["-v", "error", "-i"]).arg(&f).args(["-vn", "-ac", "1", "-ar", "4000", "-f", "s16le", "-"]).output();
            let w = o.ok().filter(|o| !o.stdout.is_empty()).map(|o| {
                let per = (4000.0 / WAVE_RATE) as usize;
                let samples: Vec<f32> = o.stdout.chunks_exact(2).map(|b| (i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).abs()).collect();
let peaks: Vec<f32> = samples.chunks(per).map(|c| c.iter().copied().fold(0.0, f32::max)).collect();
                // Ramenée au pic du fichier : un son discret reste lisible
                let top = peaks.iter().copied().fold(0.0f32, f32::max).max(1e-4);
                peaks.into_iter().map(|v| (v / top).sqrt()).collect::<Vec<f32>>()
            });
            let _ = tx.send(StripMsg::Wave(f, w));
            c2.request_repaint();
        });
        None
    }
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
    /// Dossier du coffre (pour les images des graphes d'étalonnage)
    root: String,
    strips: Strips,
    /// Graphe d'étalonnage à ouvrir dans l'éditeur de nodes
    pub open_request: Option<PathBuf>,
    /// Annuler / rétablir : états précédents du montage
    saved: Project,
    history: Vec<Project>,
    redo: Vec<Project>,
    last_head: f32,
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Editor {
    pub fn open(path: PathBuf, root: &str) -> Editor {
        let (project, error) = match Project::read(&path) {
            Ok(p) => (p, None),
            Err(e) => (Project::default(), Some(e)),
        };
        let mut project = project;
        // Des clips qui se recouvrent sur une piste : on les sépare dès l'ouverture
        if error.is_none() && project.settle_all() {
            let _ = project.write(&path);
        }
        Editor { path, error, playhead: 0.0, selected: None, zoom: 60.0, scroll: 0.0, drag: None, tex: None, preview_rx: None, shown: None, rev: 0, play: None, export_rx: None, message: None, add: vec![], root: root.to_string(), strips: Strips::default(), open_request: None, saved: project.clone(), history: vec![], redo: vec![], last_head: 0.0, project }
    }

    fn changed(&mut self) {
        if self.saved != self.project {
            self.history.push(std::mem::replace(&mut self.saved, self.project.clone()));
            if self.history.len() > 200 {
                self.history.remove(0);
            }
            self.redo.clear();
        }
        self.rev += 1;
        if let Err(e) = self.project.write(&self.path) {
            self.message = Some(e);
        }
    }

    /// Revient un pas en arrière (ou en avant avec `forward`).
    fn undo(&mut self, forward: bool) {
        let (from, to) = if forward { (&mut self.redo, &mut self.history) } else { (&mut self.history, &mut self.redo) };
        if let Some(p) = from.pop() {
            to.push(std::mem::replace(&mut self.project, p));
            self.saved = self.project.clone();
            self.selected = self.selected.filter(|&s| s < self.project.clips.len());
            self.rev += 1;
            let _ = self.project.write(&self.path);
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
        // Étalonnages en nodes : la table de couleurs se refait dès que son graphe change
        if bake_luts(&self.project, &self.path, &self.root) | bake_texts(&self.project) {
            self.rev += 1;
        }
        // Clips animés : leurs réglages suivent la tête de lecture
        if self.playhead != self.last_head {
            self.last_head = self.playhead;
            for c in &mut self.project.clips {
                if let Some(k) = c.pose_at(self.playhead) {
                    (c.x, c.y, c.scale, c.opacity) = (k.x, k.y, k.scale, k.opacity);
                }
            }
            self.saved = self.project.clone();
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
        let mut undo: Option<bool> = None;
        ui.horizontal_wrapped(|ui| {
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
            if ui.button(crate::i18n::t("＋ texte")).on_hover_text(crate::i18n::t("Ajoute un titre à la tête de lecture")).clicked() {
                let k = self.project.add_text(self.playhead);
                self.project.settle(k);
                self.selected = Some(k);
                self.changed();
            }
            if ui.button(crate::i18n::t("couper")).on_hover_text(crate::i18n::t("Coupe le clip choisi à la tête de lecture (S)")).clicked() {
                split = true;
            }
            if ui.add_enabled(self.selected.is_some(), egui::Button::new(crate::i18n::t("supprimer"))).clicked() {
                delete = true;
            }
            if ui.add_enabled(!self.history.is_empty(), egui::Button::new("↶")).on_hover_text(crate::i18n::t("annuler (Ctrl+Z)")).clicked() {
                undo = Some(false);
            }
            if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("↷")).on_hover_text(crate::i18n::t("rétablir (Ctrl+Maj+Z)")).clicked() {
                undo = Some(true);
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
            ui.separator();
            ui.scope(|ui| {
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
                if i.modifiers.command && i.key_pressed(egui::Key::Z) {
                    undo = Some(i.modifiers.shift);
                }
                if i.modifiers.command && i.key_pressed(egui::Key::Y) {
                    undo = Some(true);
                }
                split |= i.key_pressed(egui::Key::S) && !i.modifiers.any();
                delete |= i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace);
            });
        }
        if let Some(forward) = undo {
            self.undo(forward);
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
            let head = self.playhead;
            let (mut new_grade, mut open_grade) = (false, false);
            let mut pose_before: Option<(f32, f32, f32, f32)> = None;
            let mut ch2 = false;
            let mut ch_text = false;
            let mut do_xfade = false;
            egui::Panel::right("montage-clip").exact_size(280.0).frame(egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).inner_margin(10)).show(ui, |ui| {
                let c = &mut self.project.clips[s];
                ui.spacing_mut().slider_width = 84.0;
                ui.label(RichText::new(crate::i18n::t("CLIP")).size(10.5).color(p.muted).family(crate::theme::bold()));
                ch |= ui.add(egui::TextEdit::singleline(&mut c.name).desired_width(f32::INFINITY)).changed();
                ui.label(RichText::new(&c.file).size(10.0).color(p.muted));
                ui.add_space(6.0);
                if c.kind == "text" {
                    ch_text |= ui.add(egui::TextEdit::multiline(&mut c.text).desired_rows(3).desired_width(f32::INFINITY)).changed();
                    ui.horizontal(|ui| {
                        ch_text |= ui.add(egui::DragValue::new(&mut c.text_size).range(6.0..=1000.0).suffix(" px")).changed();
                        ch_text |= ui.color_edit_button_srgb(&mut c.text_color).changed();
                        ch_text |= ui.checkbox(&mut c.outline, crate::i18n::t("contour")).changed();
                    });
                    ui.horizontal(|ui| {
                        for (id, label) in [("", "gras"), ("lato", "normal"), ("mono", "mono")] {
                            if ui.selectable_label(c.font == id || (id.is_empty() && c.font == "lato-bold"), RichText::new(crate::i18n::t(label)).size(11.0)).clicked() {
                                c.font = id.to_string();
                                ch_text = true;
                            }
                        }
                        if ui.small_button(crate::i18n::t("police…")).on_hover_text(crate::i18n::t("Choisir un fichier de police (.ttf, .otf)")).clicked() {
                            if let Some(f) = rfd::FileDialog::new().add_filter("police", &["ttf", "otf"]).pick_file() {
                                c.font = f.to_string_lossy().to_string();
                                ch_text = true;
                            }
                        }
                    });
                    ui.separator();
                }
                let mut row = |ui: &mut egui::Ui, label: &str, v: &mut f32, r: std::ops::RangeInclusive<f32>, reset: f32| {
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new(crate::i18n::t(label)).size(11.0).color(p.muted)).frame(false).min_size(vec2(78.0, 0.0))).on_hover_text(crate::i18n::t("cliquer pour remettre à zéro")).clicked() {
                            *v = reset;
                            ch = true;
                        }
                        ch |= ui.add(egui::Slider::new(v, r)).changed();
                    });
                };
                if c.kind != "audio" {
                    // Clés d'animation du placement, de la taille et de l'opacité
                    ui.horizontal(|ui| {
                        let inside = head >= c.start - 1e-3 && head <= c.end() + 1e-3;
                        let rel = head - c.start;
                        let on_key = c.keys.iter().any(|k| (k.t - rel).abs() < 0.021);
                        if ui.add_enabled(inside, egui::Button::new(RichText::new(if on_key { "◆" } else { "◇" }).color(if on_key { p.accent } else { p.text }))).on_hover_text(crate::i18n::t("Poser une clé ici : placement, taille et opacité. Ensuite, chaque réglage modifié pose sa clé tout seul.")).clicked() {
                            c.set_key(head);
                            ch2 = true;
                        }
                        if ui.add_enabled(on_key, egui::Button::new("✕")).on_hover_text(crate::i18n::t("Retirer la clé de cet instant")).clicked() {
                            c.keys.retain(|k| (k.t - rel).abs() >= 0.021);
                            ch2 = true;
                        }
                        ui.label(RichText::new(format!("{} {}", c.keys.len(), crate::i18n::t("clé(s)"))).size(11.0).color(p.muted));
                        if !c.keys.is_empty() && ui.small_button(crate::i18n::t("tout retirer")).clicked() {
                            c.keys.clear();
                            ch2 = true;
                        }
                    });
                    pose_before = Some((c.x, c.y, c.scale, c.opacity));
                    row(ui, "opacité", &mut c.opacity, 0.0..=1.0, 1.0);
                    row(ui, "taille", &mut c.scale, 0.05..=3.0, 1.0);
                    row(ui, "décalage x", &mut c.x, -2000.0..=2000.0, 0.0);
                    row(ui, "décalage y", &mut c.y, -2000.0..=2000.0, 0.0);
                    ui.separator();
                    ui.label(RichText::new(crate::i18n::t("ÉTALONNAGE")).size(10.5).color(p.muted).family(crate::theme::bold()));
                    row(ui, "luminosité", &mut c.brightness, -0.5..=0.5, 0.0);
                    row(ui, "contraste", &mut c.contrast, 0.2..=2.5, 1.0);
                    row(ui, "saturation", &mut c.saturation, 0.0..=3.0, 1.0);
                    ui.horizontal(|ui| {
                        if c.grade.is_empty() {
                            if ui.button(crate::i18n::t("＋ étalonnage en nodes")).on_hover_text(crate::i18n::t("Un graphe de nodes (courbes, rampes, balance…) appliqué aux couleurs de ce clip")).clicked() {
                                new_grade = true;
                            }
                        } else {
                            if ui.button(crate::i18n::t("nodes ✎")).on_hover_text(&c.grade).clicked() {
                                open_grade = true;
                            }
                            if ui.small_button(crate::i18n::t("retirer")).clicked() {
                                c.grade.clear();
                                ch2 = true;
                            }
                        }
                    });
                    ui.separator();
                }
                if c.kind != "image" && c.kind != "text" {
                    row(ui, "vitesse", &mut c.speed, 0.25..=4.0, 1.0);
                }
                if c.has_audio {
                    row(ui, "volume", &mut c.volume, 0.0..=2.0, 1.0);
                }
                row(ui, "fondu enchaîné", &mut c.xfade, 0.0..=5.0, 0.0);
                if ui.button(RichText::new(crate::i18n::t("enchaîner avec le clip précédent")).size(11.0)).on_hover_text(crate::i18n::t("Recule ce clip pour qu'il chevauche la fin du précédent sur la même piste, et fond l'un dans l'autre (image et son)")).clicked() {
                    do_xfade = true;
                }
                row(ui, "fondu d'entrée", &mut c.fade_in, 0.0..=5.0, 0.0);
                row(ui, "fondu de sortie", &mut c.fade_out, 0.0..=5.0, 0.0);
            });
            ch |= ch2 | ch_text;
            if do_xfade {
                let d = if self.project.clips[s].xfade > 0.0 { self.project.clips[s].xfade } else { 1.0 };
                if self.project.crossfade(s, d) {
                    ch = true;
                } else {
                    self.message = Some(crate::i18n::t("Aucun clip avant celui-ci sur cette piste."));
                }
            }
            // Un clip déjà animé : chaque réglage modifié pose sa clé à la tête de lecture
            {
                let c = &mut self.project.clips[s];
                if !c.keys.is_empty() && pose_before.is_some_and(|b| b != (c.x, c.y, c.scale, c.opacity)) && head >= c.start - 1e-3 && head <= c.end() + 1e-3 {
                    c.set_key(head);
                }
            }
            if new_grade {
                let stem = self.path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "montage".into());
                let dir = self.path.parent().unwrap_or(Path::new("")).to_path_buf();
                let mut file = dir.join(format!("{stem} - {}.{}", self.project.clips[s].name, crate::formats::ext("comp")));
                let mut n = 2;
                while file.exists() {
                    file = dir.join(format!("{stem} - {} {n}.{}", self.project.clips[s].name, crate::formats::ext("comp")));
                    n += 1;
                }
                // Graphe de départ : l'image du clip, des courbes, la sortie
                let mut g = crate::nodes::Graph::new_compositor();
                g.nodes.retain(|x| x.op == "image" || x.op == "comp_out");
                g.links.clear();
                let (img, out) = (g.nodes[0].id, g.nodes[1].id);
                let cv = g.add("curves", [240.0, 0.0]);
                g.connect(img, 0, cv, 0);
                g.connect(cv, 0, out, 0);
                if g.save(&file).is_ok() {
                    self.project.clips[s].grade = file.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
                    self.open_request = Some(file);
                    ch = true;
                }
            }
            if open_grade {
                self.open_request = Some(resolve(&self.path, &self.project.clips[s].grade));
            }
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
        self.strips.poll(ui.ctx());
        let (full, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        // Colonne des pistes à gauche, comme dans Premiere : V1, V2… (la plus haute passe devant)
        let head_w = 46.0;
        let area = Rect::from_min_max(pos2(full.left() + head_w, full.top()), full.max);
        let resp = ui.interact(area, ui.scope_id().with("timeline-area"), Sense::click_and_drag());
        let painter = ui.painter_at(area);
        let (ruler_h, row_h) = (24.0, 36.0);
        {
            let hp = ui.painter_at(Rect::from_min_max(full.min, pos2(area.left(), full.bottom())));
            hp.rect_filled(Rect::from_min_max(full.min, pos2(area.left(), full.bottom())), 0.0, p.bg);
            let tracks = self.project.tracks.max(1);
            for k in 0..tracks {
                let y = full.top() + ruler_h + (tracks - 1 - k) as f32 * row_h;
                let r = Rect::from_min_size(pos2(full.left(), y), vec2(head_w - 1.0, row_h));
                hp.rect_filled(r.shrink(2.0), 3.0, p.panel);
                hp.text(r.center(), egui::Align2::CENTER_CENTER, format!("V{}", k + 1), egui::FontId::monospace(11.0), p.muted);
            }
            hp.line_segment([pos2(area.left() - 0.5, full.top()), pos2(area.left() - 0.5, full.bottom())], Stroke::new(1.0, p.line));
        }
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
            if let Some((i, mode, _, _)) = self.drag.take() {
                if matches!(mode, Drag::Move) {
                    self.project.settle(i);
                }
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
                "text" => Color32::from_rgb(150, 90, 170),
                _ => Color32::from_rgb(70, 110, 180),
            };
            let on = self.selected == Some(i);
            painter.rect_filled(r, 4.0, if on { base } else { base.linear_multiply(0.7) });
            // Vignettes (vidéo, image) le long du clip, et forme d'onde du son en bas
            let file = resolve(&self.path, &c.file);
            let inner = r.shrink(1.0);
            let clip_p = painter.with_clip_rect(inner.intersect(area));
            if matches!(c.kind.as_str(), "video" | "image") && !c.file.is_empty() {
                let th = inner.height();
                let tw = th * 16.0 / 9.0;
                let mut x = inner.left().max(area.left() - tw);
                // On cale les vignettes sur la timeline : elles ne glissent pas quand on fait défiler
                x = inner.left() + ((x - inner.left()) / tw).floor() * tw;
                let ctx = ui.ctx().clone();
                while x < inner.right().min(area.right()) {
                    let t_src = if c.kind == "image" { 0.0 } else { c.src_in + (x - inner.left()) / zoom * c.speed };
                    if let Some(tex) = self.strips.thumb(&file, t_src, &ctx) {
                        clip_p.image(tex, Rect::from_min_size(pos2(x, inner.top()), vec2(tw, th)), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::from_white_alpha(if on { 235 } else { 190 }));
                    }
                    x += tw;
                }
            }
            if (c.kind == "audio" || c.has_audio) && !c.file.is_empty() {
                let ctx = ui.ctx().clone();
                if let Some(w) = self.strips.wave(&file, &ctx) {
                    let band = if c.kind == "audio" { inner } else { Rect::from_min_max(pos2(inner.left(), inner.center().y + 2.0), inner.max) };
                    let mid = band.center().y;
                    let col = Color32::from_white_alpha(if c.kind == "audio" { 170 } else { 120 });
                    let mut x = band.left().max(area.left());
                    while x < band.right().min(area.right()) {
                        let t_src = c.src_in + (x - inner.left()) / zoom * c.speed;
                        let v = w.get((t_src * WAVE_RATE) as usize).copied().unwrap_or(0.0) * c.volume.min(2.0);
                        let hh = (v * band.height() * 0.5).min(band.height() * 0.5);
                        clip_p.line_segment([pos2(x, mid - hh), pos2(x, mid + hh)], Stroke::new(1.0, col));
                        x += 2.0;
                    }
                }
            }
            painter.rect_stroke(r, 4.0, Stroke::new(if on { 2.0 } else { 1.0 }, if on { p.accent } else { p.line }), egui::StrokeKind::Inside);
            // Fondus : un coin plus sombre à l'entrée et à la sortie
            for (len, left) in [(c.fade_in, true), (c.fade_out, false)] {
                if len > 0.0 {
                    let w = (len * zoom).min(r.width());
                    let pts = if left { vec![r.left_top(), pos2(r.left() + w, r.top()), r.left_bottom()] } else { vec![r.right_top(), pos2(r.right() - w, r.top()), r.right_bottom()] };
                    painter.add(egui::Shape::convex_polygon(pts, Color32::from_black_alpha(90), Stroke::NONE));
                }
            }
            // Le nom, sur une pastille sombre pour rester lisible par-dessus les vignettes
            let np = painter.with_clip_rect(r.shrink(3.0).intersect(area));
            let g = np.layout_no_wrap(c.name.clone(), egui::FontId::proportional(11.0), Color32::WHITE);
            let nr = Rect::from_min_size(pos2(r.left() + 4.0, r.top() + 3.0), g.size() + vec2(8.0, 2.0));
            np.rect_filled(nr, 3.0, Color32::from_black_alpha(120));
            np.galley(nr.min + vec2(4.0, 1.0), g, Color32::WHITE);
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
        let c = Clip { name: "a".into(), file: "a.mp4".into(), kind: "video".into(), track: 1, start: 2.0, src_in: 1.0, src_out: 9.0, media: 10.0, has_audio: true, speed: 2.0, opacity: 1.0, volume: 1.0, fade_in: 0.5, fade_out: 0.5, brightness: 0.0, contrast: 1.0, saturation: 1.0, x: 0.0, y: 0.0, scale: 1.0, grade: String::new(), keys: vec![], text: String::new(), text_size: 72.0, text_color: [255; 3], font: String::new(), outline: false, xfade: 0.0 };
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
        // Un titre posé sur un clip de la même piste monte sur une piste libre
        let mut q = Project::default();
        q.clips.push(p.clips[0].clone());
        let t = q.add_text(q.clips[0].start + 0.5);
        q.clips[t].track = q.clips[0].track;
        assert!(q.settle(t));
        assert_ne!(q.clips[t].track, q.clips[0].track);
        assert!(q.tracks > q.clips[t].track);
        assert!(!q.settle_all());
        // Clés : à mi-chemin entre deux clés, la pose est entre les deux ; l'expression ffmpeg suit les mêmes points
        let c = &mut p.clips[0];
        c.set_key(2.0);
        c.x = 100.0;
        c.opacity = 0.0;
        c.set_key(3.0);
        let mid = c.pose_at(2.5).unwrap();
        assert!((mid.x - 50.0).abs() < 1e-3 && (mid.opacity - 0.5).abs() < 1e-3);
        c.x = 80.0;
        c.set_key(3.0);
        assert!(c.keys.len() == 2 && c.keys[1].x == 80.0);
        assert_eq!(expr(&[(0.0, 1.0)], "t"), "1.0000");
        // Titre : des pixels opaques au centre, rien dans les coins ; avec contour, du noir autour des lettres
        let k = p.add_text(1.0);
        p.clips[k].text = "Aryadne".into();
        let img = render_text(&p.clips[k], 640, 360);
        assert_eq!(img.get_pixel(3, 3).0[3], 0);
        assert!(img.pixels().filter(|px| px.0[3] > 200 && px.0[0] > 200).count() > 500);
        assert!(img.pixels().any(|px| px.0[3] > 200 && px.0[0] < 30));
        assert_ne!(text_file(&p.clips[k], &p), text_file(&Clip { text: "autre".into(), ..p.clips[k].clone() }, &p));
        // Fondu enchaîné : le second clip recule pour chevaucher la fin du premier
        let mut q = Project::default();
        let base = Clip { start: 0.0, src_in: 0.0, src_out: 4.0, speed: 1.0, keys: vec![], track: 1, opacity: 1.0, fade_in: 0.0, fade_out: 0.0, ..p.clips[0].clone() };
        q.clips = vec![base.clone(), Clip { start: 4.0, ..base }];
        assert!(q.crossfade(1, 1.0) && !q.crossfade(0, 1.0));
        assert_eq!((q.clips[1].start, q.clips[1].xfade, q.duration()), (3.0, 1.0, 7.0));
        let (_, f, _, _) = graph(&q, Path::new("/x/m.aryav"), 0.0, 640, 360, false, true);
        assert!(f.contains("fade=t=in:st=3.000:d=1.000:alpha=1") && f.contains("afade=t=out:st=3.000:d=1.000") && f.contains("afade=t=in:st=0:d=1.000"), "{f}");
        assert_eq!(expr(&[(1.0, 0.0), (3.0, 10.0)], "t"), "if(lt(t,1.0000),0.0000,if(lt(t,3.0000),0.0000+(10.0000)*(t-(1.0000))/2.0000,10.0000))");
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
        // Le premier clip glisse, rétrécit et s'efface ; le second passe par un étalonnage en nodes qui inverse les couleurs
        a.set_key(0.0);
        (a.x, a.scale, a.opacity) = (60.0, 0.5, 0.2);
        a.set_key(2.0);
        let mut g = crate::nodes::Graph::new_compositor();
        g.nodes.retain(|x| x.op == "image" || x.op == "comp_out");
        g.links.clear();
        let (img, out) = (g.nodes[0].id, g.nodes[1].id);
        let inv = g.add("invert", [100.0, 0.0]);
        g.connect(img, 0, inv, 0);
        g.connect(inv, 0, out, 0);
        g.save(&dir.join("inverse.comp.json")).unwrap();
        b.grade = "inverse.comp.json".into();
        // Un titre par-dessus le tout
        let mut with_title = Project { width: 320, height: 240, ..Default::default() };
        let t = with_title.add_text(0.5);
        let title = with_title.clips[t].clone();
        p.clips = vec![a, b, title];
        p.write(&file).unwrap();
        // Aperçu au milieu du second clip : sans la table, puis avec (les barres claires deviennent sombres)
        let _ = std::fs::remove_file(lut_file(&dir.join("inverse.comp.json")));
        let plain = frame(&p, &file, 3.0, 160).unwrap();
        assert!(bake_luts(&p, &file, ""));
        let img = frame(&p, &file, 3.0, 160).unwrap();
        assert!(plain.get_pixel(10, 10).0[0] > 150 && img.get_pixel(10, 10).0[0] < 100, "{:?} {:?}", plain.get_pixel(10, 10), img.get_pixel(10, 10));
        // Au milieu de l'animation du premier clip, l'image est plus petite que le cadre : un coin reste noir
        let anim = frame(&p, &file, 1.0, 160).unwrap();
        assert!(anim.get_pixel(2, 2).0[0] < 20 && anim.pixels().any(|px| px.0[0] > 60));
        assert_eq!((img.width(), img.height()), (160, 120));
        assert!(img.pixels().any(|px| px.0[0] > 60));
        let out = dir.join("essai.mp4");
        export(&p, &file, &out).unwrap();
        let info = crate::video::probe(&out).unwrap();
        assert!((info.duration - 5.0).abs() < 0.3, "{}", info.duration);
        assert_eq!((info.width, info.height), (320, 240));
    }
}
