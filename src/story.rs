//! Storyboard (.arystory) : des cases dessinées, avec dialogue, action, valeur de plan et durée.
//! Chaque case est une image PNG dans le dossier « <nom> - cases » à côté du fichier.
//! L'animatique se lit dans l'éditeur, s'exporte en MP4 ou part au montage ; la planche s'exporte en PNG.

use resvg::tiny_skia::{self, Pixmap};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn is_story(rel: &str) -> bool {
    rel.to_lowercase().ends_with(".arystory")
}

/// Formats d'image : (nom, largeur, hauteur)
pub const ASPECTS: [(&str, u32, u32); 5] = [("16:9", 1280, 720), ("2,39:1 (scope)", 1280, 536), ("4:3", 960, 720), ("1:1", 900, 900), ("9:16 (vertical)", 720, 1280)];

/// Valeurs de plan courantes
pub const SHOTS: [&str; 9] = ["plan d'ensemble", "plan large", "plan moyen", "plan américain", "plan rapproché", "gros plan", "très gros plan", "plongée", "contre-plongée"];

/// Mouvements de caméra courants
pub const MOVES: [&str; 8] = ["fixe", "panoramique", "travelling avant", "travelling arrière", "travelling latéral", "zoom", "caméra à l'épaule", "grue"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Panel {
    pub id: u64,
    /// Séquence ou scène (« 1 », « INT. CUISINE »…) : les cases d'une même scène se suivent
    #[serde(default)]
    pub scene: String,
    #[serde(default)]
    pub shot: String,
    #[serde(default)]
    pub camera: String,
    /// Durée dans l'animatique, en secondes
    #[serde(default = "two")]
    pub dur: f32,
    #[serde(default)]
    pub dialogue: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub notes: String,
    /// Caméra 2D : cadre de départ et d'arrivée dans la case (centre X, centre Y en 0..1, zoom ≥ 1),
    /// parcourus pendant la durée de la case (pano, zoom, travelling sur le dessin)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cam: Option<[[f32; 3]; 2]>,
}

/// Le cadre de la caméra 2D à l'instant `t` (0..1) de la case : zone visible (u0, v0, u1, v1) en 0..1
pub fn cam_rect(cam: &[[f32; 3]; 2], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    let e = t * t * (3.0 - 2.0 * t);
    let z = |k: usize| cam[0][k] + (cam[1][k] - cam[0][k]) * e;
    // Le zoom s'interpole en logarithme : une vitesse régulière à l'œil
    let zoom = (cam[0][2].max(1.0).ln() + (cam[1][2].max(1.0).ln() - cam[0][2].max(1.0).ln()) * e).exp();
    let half = 0.5 / zoom;
    let (cx, cy) = (z(0).clamp(half, 1.0 - half), z(1).clamp(half, 1.0 - half));
    [cx - half, cy - half, cx + half, cy + half]
}

/// Une image de la case vue par la caméra 2D à l'instant `t`, à la taille de la case
pub fn cam_frame(img: &image::RgbImage, cam: &[[f32; 3]; 2], t: f32) -> image::RgbImage {
    let r = cam_rect(cam, t);
    let (w, h) = (img.width(), img.height());
    let (x0, y0) = ((r[0] * w as f32) as u32, (r[1] * h as f32) as u32);
    let (cw, ch) = ((((r[2] - r[0]) * w as f32) as u32).max(2).min(w - x0), (((r[3] - r[1]) * h as f32) as u32).max(2).min(h - y0));
    let crop = image::imageops::crop_imm(img, x0, y0, cw, ch).to_image();
    image::imageops::resize(&crop, w, h, image::imageops::FilterType::Triangle)
}

fn two() -> f32 {
    2.0
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Story {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub panels: Vec<Panel>,
    #[serde(default)]
    pub next_id: u64,
}

impl Story {
    pub fn starter() -> Story {
        let mut s = Story { version: 1, width: 1280, height: 720, panels: vec![], next_id: 1 };
        for _ in 0..3 {
            s.add(None);
        }
        s
    }

    pub fn read(path: &Path) -> Result<Story, String> {
        let t = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        if t.trim().is_empty() {
            return Ok(Story::starter());
        }
        let mut s: Story = serde_json::from_str(&t).map_err(|e| format!("Storyboard illisible : {e}"))?;
        s.next_id = s.next_id.max(s.panels.iter().map(|p| p.id + 1).max().unwrap_or(1));
        Ok(s)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        crate::vault::atomic_write(path, serde_json::to_string_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }

    /// Ajoute une case après `after` (ou à la fin) et renvoie son indice. Elle reprend la scène de la précédente.
    pub fn add(&mut self, after: Option<usize>) -> usize {
        let at = after.map(|a| (a + 1).min(self.panels.len())).unwrap_or(self.panels.len());
        let scene = at.checked_sub(1).and_then(|i| self.panels.get(i)).map(|p| p.scene.clone()).unwrap_or_default();
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        self.panels.insert(at, Panel { id, scene, shot: String::new(), camera: String::new(), dur: 2.0, dialogue: String::new(), action: String::new(), notes: String::new(), cam: None });
        at
    }

    pub fn total(&self) -> f32 {
        self.panels.iter().map(|p| p.dur.max(0.05)).sum()
    }

    /// Début de chaque case dans l'animatique.
    pub fn starts(&self) -> Vec<f32> {
        let mut t = 0.0;
        self.panels
            .iter()
            .map(|p| {
                let s = t;
                t += p.dur.max(0.05);
                s
            })
            .collect()
    }

    /// La case montrée au temps `t`.
    pub fn at(&self, t: f32) -> usize {
        let st = self.starts();
        st.iter().rposition(|&s| s <= t).unwrap_or(0).min(self.panels.len().saturating_sub(1))
    }

    /// Numéro affiché d'une case : « scène.numéro dans la scène » quand il y a des scènes.
    pub fn label(&self, i: usize) -> String {
        let p = &self.panels[i];
        if p.scene.trim().is_empty() {
            return format!("{}", i + 1);
        }
        let n = self.panels[..=i].iter().rev().take_while(|q| q.scene == p.scene).count();
        format!("{} · {n}", p.scene.trim())
    }
}

/// Dossier des dessins d'un storyboard.
pub fn panels_dir(story: &Path) -> PathBuf {
    let stem = story.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "storyboard".into());
    story.parent().unwrap_or(Path::new("")).join(format!("{stem} - cases"))
}

pub fn panel_file(story: &Path, id: u64) -> PathBuf {
    panels_dir(story).join(format!("{id:04}.png"))
}

/// Charge le dessin d'une case (transparent s'il n'existe pas encore), à la taille du storyboard.
pub fn load(story: &Path, id: u64, w: u32, h: u32) -> Pixmap {
    let mut pm = Pixmap::new(w.max(1), h.max(1)).expect("taille");
    if let Ok(img) = image::open(panel_file(story, id)) {
        let img = img.to_rgba8();
        let img = if img.width() != w || img.height() != h { image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle) } else { img };
        for (px, s) in pm.pixels_mut().iter_mut().zip(img.pixels()) {
            *px = tiny_skia::ColorU8::from_rgba(s[0], s[1], s[2], s[3]).premultiply();
        }
    }
    pm
}

pub fn save_panel(story: &Path, id: u64, pm: &Pixmap) -> Result<(), String> {
    std::fs::create_dir_all(panels_dir(story)).map_err(|e| e.to_string())?;
    let mut img = image::RgbaImage::new(pm.width(), pm.height());
    for (d, s) in img.pixels_mut().zip(pm.pixels()) {
        let c = s.demultiply();
        *d = image::Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    img.save(panel_file(story, id)).map_err(|e| e.to_string())
}

/// Le dessin posé sur du blanc (pour l'animatique, la planche, le montage).
pub fn flat(pm: &Pixmap) -> image::RgbImage {
    let mut img = image::RgbImage::new(pm.width(), pm.height());
    for (d, s) in img.pixels_mut().zip(pm.pixels()) {
        let a = s.alpha() as u32;
        let f = |c: u8| (c as u32 + (255 - a)).min(255) as u8;
        *d = image::Rgb([f(s.red()), f(s.green()), f(s.blue())]);
    }
    img
}

/// Outils de dessin : (nom, icône, touche)
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
    Pencil,
    Ink,
    Tone,
    Fill,
    Eraser,
    /// Pot de peinture : un clic remplit la zone fermée par le trait
    Bucket,
}

/// Un trait de pinceau entre deux points, avec bouts arrondis.
pub fn stroke(pm: &mut Pixmap, tool: Tool, color: [u8; 4], size: f32, pts: &[(f32, f32, f32)]) {
    if pts.is_empty() {
        return;
    }
    let mut paint = tiny_skia::Paint { anti_alias: true, ..Default::default() };
    let (alpha, width) = match tool {
        Tool::Pencil => (0.85, size),
        Tool::Ink => (1.0, size),
        Tool::Tone => (0.22, size),
        Tool::Fill | Tool::Bucket => (1.0, size),
        Tool::Eraser => (1.0, size),
    };
    paint.set_color_rgba8(color[0], color[1], color[2], (color[3] as f32 * alpha) as u8);
    if tool == Tool::Eraser {
        paint.blend_mode = tiny_skia::BlendMode::DestinationOut;
        paint.set_color_rgba8(0, 0, 0, 255);
    }
    // Segment par segment : la pression change la largeur le long du trait
    for w in pts.windows(2).chain(std::iter::once(&pts[pts.len() - 1..])) {
        let (a, b) = (w[0], *w.last().unwrap());
        let mut pb = tiny_skia::PathBuilder::new();
        pb.move_to(a.0, a.1);
        pb.line_to(b.0 + 0.01, b.1);
        let Some(path) = pb.finish() else { continue };
        let st = tiny_skia::Stroke { width: (width * (a.2 + b.2) * 0.5).max(0.5), line_cap: tiny_skia::LineCap::Round, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
        pm.stroke_path(&path, &paint, &st, tiny_skia::Transform::identity(), None);
        if pts.len() == 1 {
            break;
        }
    }
}

/// Remplit une forme fermée (le lasso de l'outil « aplat »).
pub fn fill(pm: &mut Pixmap, color: [u8; 4], pts: &[(f32, f32, f32)]) {
    if pts.len() < 3 {
        return;
    }
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(pts[0].0, pts[0].1);
    for p in &pts[1..] {
        pb.line_to(p.0, p.1);
    }
    pb.close();
    let Some(path) = pb.finish() else { return };
    let mut paint = tiny_skia::Paint { anti_alias: true, ..Default::default() };
    paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
    pm.fill_path(&path, &paint, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
}

/// Les trous du trait refermés par le pot et le coloriage, selon la taille de la case
fn gap_of(pm: &Pixmap) -> usize {
    (pm.width() as usize / 320).clamp(2, 8)
}

/// Pot de peinture : remplit la zone fermée par le trait autour de (x, y) ; les petits trous du trait sont refermés.
pub fn bucket(pm: &mut Pixmap, color: [u8; 4], x: f32, y: f32) -> bool {
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    if x < 0.0 || y < 0.0 {
        return false;
    }
    let gap = gap_of(pm);
    crate::colorize::bucket(pm.data_mut(), w, h, x as usize, y as usize, color, 0.12, gap, true)
}

/// Colorie la case d'après une autre (la précédente, déjà coloriée) : chaque zone fermée du trait reprend la
/// couleur de la même zone. Rend le nombre de zones coloriées.
pub fn colorize_from(pm: &mut Pixmap, model: &Pixmap) -> usize {
    if pm.width() != model.width() || pm.height() != model.height() {
        return 0;
    }
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let gap = gap_of(pm);
    crate::colorize::colorize(pm.data_mut(), model.data(), w, h, gap, &[], true)
}

// ---------- texte (planche) ----------

/// Écrit du texte dans une image, avec retour à la ligne à `max_w`. Renvoie la hauteur utilisée.
fn draw_text(img: &mut image::RgbImage, x: f32, y: f32, size: f32, max_w: f32, text: &str, color: [u8; 3], bold: bool) -> f32 {
    use ab_glyph::{Font, ScaleFont};
    let font = crate::motion::font(if bold { "inter-bold" } else { "inter" });
    let sf = font.as_scaled(size);
    let line_h = (sf.ascent() - sf.descent() + sf.line_gap()).ceil();
    // Coupe en lignes selon la largeur
    let mut lines: Vec<String> = vec![];
    for para in text.lines() {
        let mut cur = String::new();
        for word in para.split(' ') {
            let trial = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            let w: f32 = trial.chars().map(|c| sf.h_advance(font.glyph_id(c))).sum();
            if w > max_w && !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
                cur = word.to_string();
            } else {
                cur = trial;
            }
        }
        lines.push(cur);
    }
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    for (li, line) in lines.iter().enumerate() {
        let mut pen = x;
        let base = y + sf.ascent() + li as f32 * line_h;
        for c in line.chars() {
            let id = font.glyph_id(c);
            let g = id.with_scale_and_position(size, ab_glyph::point(pen, base));
            pen += sf.h_advance(id);
            if let Some(o) = font.outline_glyph(g) {
                let bb = o.px_bounds();
                o.draw(|gx, gy, cov| {
                    let (px, py) = (bb.min.x as i32 + gx as i32, bb.min.y as i32 + gy as i32);
                    if px >= 0 && py >= 0 && px < iw && py < ih {
                        let d = img.get_pixel_mut(px as u32, py as u32);
                        for k in 0..3 {
                            d.0[k] = (d.0[k] as f32 * (1.0 - cov) + color[k] as f32 * cov) as u8;
                        }
                    }
                });
            }
        }
    }
    lines.len() as f32 * line_h
}

/// La planche : les cases en grille (3 par rangée), avec leur numéro, plan, action et dialogue.
pub fn sheet(story_path: &Path, s: &Story, out: &Path) -> Result<(), String> {
    let cols = 3usize;
    let cell_w = 520u32;
    let cell_h = (cell_w as f32 * s.height as f32 / s.width.max(1) as f32) as u32;
    let text_h = 150u32;
    let (margin, gap) = (48u32, 28u32);
    let rows = s.panels.len().div_ceil(cols).max(1) as u32;
    let w = margin * 2 + cols as u32 * cell_w + (cols as u32 - 1) * gap;
    let h = margin * 2 + 60 + rows * (cell_h + text_h + gap);
    let mut img = image::RgbImage::from_pixel(w, h, image::Rgb([255, 255, 255]));
    let title = story_path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
    draw_text(&mut img, margin as f32, margin as f32 - 8.0, 30.0, w as f32, &title, [20, 20, 24], true);
    for (i, p) in s.panels.iter().enumerate() {
        let (c, r) = ((i % cols) as u32, (i / cols) as u32);
        let x = margin + c * (cell_w + gap);
        let y = margin + 60 + r * (cell_h + text_h + gap);
        let pm = load(story_path, p.id, s.width, s.height);
        let pic = image::imageops::resize(&flat(&pm), cell_w, cell_h, image::imageops::FilterType::Triangle);
        image::imageops::overlay(&mut img, &pic, x as i64, y as i64);
        // Cadre
        for k in 0..cell_w {
            img.put_pixel(x + k, y, image::Rgb([40, 40, 40]));
            img.put_pixel(x + k, y + cell_h - 1, image::Rgb([40, 40, 40]));
        }
        for k in 0..cell_h {
            img.put_pixel(x, y + k, image::Rgb([40, 40, 40]));
            img.put_pixel(x + cell_w - 1, y + k, image::Rgb([40, 40, 40]));
        }
        let mut ty = (y + cell_h + 8) as f32;
        let head = [Some(s.label(i)), (!p.shot.is_empty()).then(|| crate::i18n::t(&p.shot)), (!p.camera.is_empty()).then(|| crate::i18n::t(&p.camera)), Some(format!("{:.1} s", p.dur))].into_iter().flatten().collect::<Vec<_>>().join("   ");
        ty += draw_text(&mut img, x as f32, ty, 17.0, cell_w as f32, &head, [20, 20, 24], true) + 4.0;
        if !p.action.trim().is_empty() {
            ty += draw_text(&mut img, x as f32, ty, 15.0, cell_w as f32, p.action.trim(), [60, 60, 66], false) + 3.0;
        }
        if !p.dialogue.trim().is_empty() {
            draw_text(&mut img, x as f32 + 12.0, ty, 15.0, cell_w as f32 - 12.0, &format!("« {} »", p.dialogue.trim()), [30, 60, 130], false);
        }
    }
    img.save(out).map_err(|e| e.to_string())
}

/// Les images aplaties de chaque case, écrites dans un dossier : (chemin, durée).
fn flats(story_path: &Path, s: &Story, dir: &Path) -> Result<Vec<(PathBuf, f32)>, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut out = vec![];
    for p in &s.panels {
        let f = dir.join(format!("{:04}.png", p.id));
        // Dimensions paires pour l'encodeur vidéo
        let img = flat(&load(story_path, p.id, s.width, s.height));
        let (w, h) = (img.width() & !1, img.height() & !1);
        let img = image::imageops::crop_imm(&img, 0, 0, w, h).to_image();
        match &p.cam {
            // Caméra 2D : la case devient un petit film (24 i/s), le cadre glissant sur le dessin
            Some(cam) => {
                let v = dir.join(format!("{:04}.mp4", p.id));
                cam_video(&img, cam, p.dur.max(0.05), &v)?;
                out.push((v, p.dur.max(0.05)));
            }
            None => {
                img.save(&f).map_err(|e| e.to_string())?;
                out.push((f, p.dur.max(0.05)));
            }
        }
    }
    Ok(out)
}

/// Le mouvement de caméra d'une case en vidéo
fn cam_video(img: &image::RgbImage, cam: &[[f32; 3]; 2], dur: f32, out: &Path) -> Result<(), String> {
    use std::io::Write;
    let ff = crate::ffmpeg::find("ffmpeg").ok_or("ffmpeg introuvable")?;
    let n = (dur * 24.0).round().max(1.0) as usize;
    let mut child = crate::ffmpeg::command(ff)
        .args(["-y", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{}x{}", img.width(), img.height()), "-r", "24", "-i", "-", "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-pix_fmt", "yuv420p"])
        .arg(out)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    {
        let mut si = child.stdin.take().ok_or("ffmpeg")?;
        for k in 0..n {
            let fr = cam_frame(img, cam, if n > 1 { k as f32 / (n - 1) as f32 } else { 0.0 });
            si.write_all(fr.as_raw()).map_err(|e| e.to_string())?;
        }
    }
    let o = child.wait_with_output().map_err(|e| e.to_string())?;
    if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("échec de ffmpeg").to_string()) }
}

/// L'animatique en MP4 (une image par case, pendant sa durée).
pub fn export_mp4(story_path: &Path, s: &Story, out: &Path) -> Result<(), String> {
    if s.panels.is_empty() {
        return Err("aucune case".into());
    }
    let tmp = std::env::temp_dir().join(format!("aryadne-animatique-{}", std::process::id()));
    let list = flats(story_path, s, &tmp)?;
    let mut txt = String::new();
    for (f, d) in &list {
        txt.push_str(&format!("file '{}'\nduration {d}\n", f.to_string_lossy().replace('\'', "'\\''")));
    }
    // Le dernier fichier se répète : sinon sa durée est ignorée
    if let Some((f, _)) = list.last() {
        txt.push_str(&format!("file '{}'\n", f.to_string_lossy().replace('\'', "'\\''")));
    }
    // Avec des mouvements de caméra : chaque case devient une courte vidéo, puis tout se met bout à bout
    if list.iter().any(|(f, _)| f.extension().is_some_and(|e| e == "mp4")) {
        let ff = crate::ffmpeg::find("ffmpeg").ok_or("ffmpeg introuvable")?;
        txt.clear();
        for (k, (f, d)) in list.iter().enumerate() {
            let seg = if f.extension().is_some_and(|e| e == "mp4") {
                f.clone()
            } else {
                let seg = tmp.join(format!("seg{k:04}.mp4"));
                let o = crate::ffmpeg::command(ff.clone()).args(["-y", "-v", "error", "-loop", "1", "-i"]).arg(f).args(["-t", &format!("{d}"), "-r", "24", "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-pix_fmt", "yuv420p"]).arg(&seg).output().map_err(|e| e.to_string())?;
                if !o.status.success() {
                    return Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("échec de ffmpeg").to_string());
                }
                seg
            };
            txt.push_str(&format!("file '{}'\n", seg.to_string_lossy().replace('\'', "'\\''")));
        }
        let lf = tmp.join("liste.txt");
        std::fs::write(&lf, txt).map_err(|e| e.to_string())?;
        let o = crate::ffmpeg::command(ff).args(["-y", "-v", "error", "-f", "concat", "-safe", "0", "-i"]).arg(&lf).args(["-c", "copy"]).arg(out).output().map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(&tmp);
        return if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("échec de ffmpeg").to_string()) };
    }
    let lf = tmp.join("liste.txt");
    std::fs::write(&lf, txt).map_err(|e| e.to_string())?;
    let ff = crate::ffmpeg::find("ffmpeg").ok_or("ffmpeg introuvable")?;
    let o = crate::ffmpeg::command(ff).args(["-y", "-v", "error", "-f", "concat", "-safe", "0", "-i"]).arg(&lf).args(["-vf", "fps=24,format=yuv420p", "-c:v", "libx264", "-preset", "veryfast", "-crf", "20"]).arg(out).output().map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&tmp);
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("échec de ffmpeg").to_string());
    }
    Ok(())
}

/// Un montage (.aryseq) avec une image par case, à la suite, et un repère au début de chaque scène.
pub fn to_montage(story_path: &Path, s: &Story, out: &Path) -> Result<(), String> {
    let dir = panels_dir(story_path).join("animatique");
    let list = flats(story_path, s, &dir)?;
    let mut p = crate::montage::Project { width: s.width & !1, height: s.height & !1, ..Default::default() };
    let mut t = 0.0;
    let mut last_scene: Option<&str> = None;
    for ((f, d), panel) in list.iter().zip(&s.panels) {
        let mut c = crate::montage::import(out, f, 0, t)?;
        c.src_out = *d;
        c.name = panel.dialogue.lines().next().filter(|l| !l.trim().is_empty()).map(|l| l.chars().take(40).collect()).unwrap_or(c.name);
        p.clips.push(c);
        if last_scene != Some(panel.scene.as_str()) {
            p.markers.push(t);
            last_scene = Some(panel.scene.as_str());
        }
        t += d;
    }
    p.ensure_ids();
    p.write(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pot_et_coloriage_d_une_case() {
        // Une case : un rond à l'encre avec une petite ouverture ; le pot le remplit sans déborder
        let ring = |cx: f32, open: bool| {
            let mut pm = Pixmap::new(320, 180).unwrap();
            let n = if open { 252 } else { 257 };
            let pts: Vec<(f32, f32, f32)> = (0..n).map(|k| {
                let a = k as f32 / 256.0 * std::f32::consts::TAU;
                (cx + 50.0 * a.cos(), 90.0 + 50.0 * a.sin(), 1.0)
            }).collect();
            stroke(&mut pm, Tool::Ink, [20, 20, 24, 255], 4.0, &pts);
            pm
        };
        let px = |pm: &Pixmap, x: u32, y: u32| { let c = pm.pixel(x, y).unwrap(); [c.red(), c.green(), c.blue(), c.alpha()] };
        let mut a = ring(120.0, false);
        assert!(bucket(&mut a, [230, 120, 40, 255], 120.0, 90.0));
        assert_eq!(px(&a, 120, 90), [230, 120, 40, 255]);
        assert_eq!(px(&a, 10, 10), [0, 0, 0, 0]);
        assert!(!bucket(&mut a, [0, 0, 0, 255], -4.0, 90.0));
        // La case suivante : le même rond déplacé, trait ouvert de quelques pixels ; il reprend la couleur
        let mut b = ring(150.0, true);
        assert_eq!(colorize_from(&mut b, &a), 1);
        assert_eq!(px(&b, 150, 90), [230, 120, 40, 255]);
        assert_eq!(px(&b, 10, 10), [0, 0, 0, 0]);
        assert_eq!(px(&b, 150, 40)[3], 255);
        assert!(px(&b, 150, 40)[0] < 80, "le trait reste : {:?}", px(&b, 150, 40));
        assert_eq!(colorize_from(&mut b, &Pixmap::new(10, 10).unwrap()), 0);
    }

    #[test]
    fn storyboard() {
        let dir = std::env::temp_dir().join(format!("aryadne-story-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("essai.arystory");
        let mut s = Story::starter();
        assert_eq!(s.panels.len(), 3);
        s.panels[0].scene = "1".into();
        let k = s.add(Some(0));
        assert_eq!(k, 1);
        assert_eq!(s.panels[1].scene, "1");
        assert_eq!(s.label(1), "1 · 2");
        s.panels[1].dur = 1.0;
        assert!((s.total() - 7.0).abs() < 1e-5);
        assert_eq!(s.at(2.5), 1);
        // Dessin : un trait d'encre, enregistré puis relu
        let mut pm = Pixmap::new(s.width, s.height).unwrap();
        stroke(&mut pm, Tool::Ink, [0, 0, 0, 255], 8.0, &[(100.0, 100.0, 1.0), (300.0, 120.0, 1.0)]);
        save_panel(&f, s.panels[0].id, &pm).unwrap();
        let back = load(&f, s.panels[0].id, s.width, s.height);
        assert!(back.pixel(200, 110).unwrap().alpha() > 200);
        stroke(&mut pm, Tool::Eraser, [0, 0, 0, 255], 30.0, &[(200.0, 110.0, 1.0)]);
        assert!(pm.pixel(200, 110).unwrap().alpha() < 10);
        s.save(&f).unwrap();
        assert_eq!(Story::read(&f).unwrap(), s);
        // Planche
        sheet(&f, &s, &dir.join("planche.png")).unwrap();
        // Caméra 2D : du plan large au gros plan sur le coin haut gauche
        let cam = [[0.5, 0.5, 1.0], [0.25, 0.25, 2.0]];
        assert_eq!(cam_rect(&cam, 0.0), [0.0, 0.0, 1.0, 1.0]);
        let r = cam_rect(&cam, 1.0);
        assert!((r[0] - 0.0).abs() < 1e-5 && (r[2] - 0.5).abs() < 1e-5, "{r:?}");
        if crate::ffmpeg::find("ffmpeg").is_some() {
            // Petit format pour aller vite
            (s.width, s.height) = (320, 180);
            s.panels[0].dur = 0.5;
            s.panels[0].cam = Some(cam);
            export_mp4(&f, &s, &dir.join("anim.mp4")).unwrap();
            assert!(std::fs::metadata(dir.join("anim.mp4")).unwrap().len() > 1000);
        }
        assert!(image::open(dir.join("planche.png")).unwrap().width() > 1000);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
