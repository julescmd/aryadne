//! Montage vidéo (`.aryseq`) : des pistes, des clips qu'on déplace, raccourcit et coupe,
//! un aperçu à la tête de lecture et un export. Le fichier est du JSON lisible ; les médias
//! restent à leur place. Tout le calcul d'image passe par ffmpeg, avec un seul graphe de
//! filtres pour l'aperçu, la lecture et l'export : ce qu'on voit est ce qu'on exporte.

use crate::ffmpeg;
use std::collections::HashMap;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

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
    /// Roues de couleur (ombres, tons moyens, hautes lumières) : position du point (x, y entre -1 et 1) et luminosité
    #[serde(default, skip_serializing_if = "wheels_neutral")]
    pub wheels: [[f32; 3]; 3],
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
    /// Sur une piste son (A1, A2…) plutôt qu'une piste image (V1, V2…)
    #[serde(default)]
    pub audio: bool,
    /// Groupe de clips liés (l'image et le son d'une même vidéo) ; 0 : aucun
    #[serde(default)]
    pub link: u32,
    /// Effets ajoutés depuis le panneau Effets (sections affichées dans les options d'effet)
    #[serde(default)]
    pub fx: Vec<String>,
    /// Stabilisation (0 : aucune) : agrandissement en % qui cache les bords qui bougent
    #[serde(default, skip_serializing_if = "is_zero_f")]
    pub stab: f32,
    /// Identifiant stable dans le montage (la sélection survit aux suppressions et aux coupes)
    #[serde(default)]
    pub id: u64,
}

/// État d'une piste : verrouillée, éteinte (œil ou muet), en solo.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub struct TrackState {
    #[serde(default)]
    pub lock: bool,
    #[serde(default)]
    pub off: bool,
    #[serde(default)]
    pub solo: bool,
}

fn text_size() -> f32 {
    72.0
}
fn white() -> [u8; 3] {
    [255; 3]
}

/// Dessine un titre sur une image transparente de la taille du projet, centré.
pub fn wheels_neutral(w: &[[f32; 3]; 3]) -> bool {
    w.iter().all(|v| v.iter().all(|x| x.abs() < 1e-4))
}

/// Décalage RVB d'une roue : le point (x, y) donne une teinte dans le plan Cb/Cr, plus la luminosité
pub fn wheel_rgb(v: [f32; 3]) -> [f32; 3] {
    let (cb, cr) = (v[0] * 0.25, -v[1] * 0.25);
    [v[2] + 1.402 * cr, v[2] - 0.344 * cb - 0.714 * cr, v[2] + 1.772 * cb].map(|x| x.clamp(-1.0, 1.0))
}

/// Filtre ffmpeg des roues : ombres (lift), tons moyens (gamma), hautes lumières (gain)
pub fn colorbalance(w: &[[f32; 3]; 3]) -> String {
    let [s, m, h] = [wheel_rgb(w[0]), wheel_rgb(w[1]), wheel_rgb(w[2])];
    format!("colorbalance=rs={:.3}:gs={:.3}:bs={:.3}:rm={:.3}:gm={:.3}:bm={:.3}:rh={:.3}:gh={:.3}:bh={:.3}", s[0], s[1], s[2], m[0], m[1], m[2], h[0], h[1], h[2])
}

/// Couleur du mot en cours dans les sous-titres mot par mot
pub const HIGHLIGHT: [u8; 3] = [255, 214, 64];

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
    // « **mot** » : le mot en surbrillance (sous-titres mot par mot) ; les marques ne s'affichent pas
    let plain = c.text.replace("**", "");
    let lit: Vec<Vec<bool>> = c.text.lines().map(|l| {
        let mut on = false;
        let mut v = vec![];
        let mut rest = l;
        while !rest.is_empty() {
            if let Some(r) = rest.strip_prefix("**") {
                on = !on;
                rest = r;
                continue;
            }
            let ch = rest.chars().next().unwrap();
            v.push(on);
            rest = &rest[ch.len_utf8()..];
        }
        v
    }).collect();
    let lines: Vec<&str> = plain.lines().collect();
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
            for (ci, ch) in l.chars().enumerate() {
                let id = sf.glyph_id(ch);
                if let Some(p) = prev {
                    x += sf.kern(p, id);
                }
                // Le mot en surbrillance prend la couleur vive (le contour reste noir)
                let col = if col != [0, 0, 0] && lit.get(li).and_then(|v| v.get(ci)).copied().unwrap_or(false) { HIGHLIGHT } else { col };
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
    /// Passage vers la clé suivante (linéaire pour les anciens montages)
    #[serde(default = "linear")]
    pub ease: crate::keys::Ease,
    /// Tangentes posées dans l'éditeur de courbes, par réglage (x, y, taille, opacité)
    #[serde(default, skip_serializing_if = "no_tan")]
    pub tan: [Option<[f32; 4]>; 4],
}

fn is_zero_f(v: &f32) -> bool {
    *v == 0.0
}

fn no_tan(t: &[Option<[f32; 4]>; 4]) -> bool {
    t.iter().all(|x| x.is_none())
}

impl PoseKey {
    pub fn get(&self, c: usize) -> f32 {
        [self.x, self.y, self.scale, self.opacity][c.min(3)]
    }

    pub fn set(&mut self, c: usize, v: f32) {
        match c {
            0 => self.x = v,
            1 => self.y = v,
            2 => self.scale = v,
            _ => self.opacity = v,
        }
    }
}

/// La pose à l'instant `r` (relatif au clip) d'après les clés : lissages, ou Bézier si des tangentes sont posées
pub fn pose_in(k: &[PoseKey], r: f32) -> Option<PoseKey> {
    let (first, last) = (k.first()?, k.last()?);
    if r <= first.t {
        return Some(*first);
    }
    if r >= last.t {
        return Some(*last);
    }
    let i = (0..k.len() - 1).find(|&i| r < k[i + 1].t)?;
    let (a, b) = (k[i], k[i + 1]);
    let mut out = PoseKey { t: r, ease: a.ease, tan: [None; 4], ..a };
    if a.tan.iter().chain(b.tan.iter()).any(|x| x.is_some()) {
        for c in 0..4 {
            let curve: Vec<crate::curves::CK> = k.iter().map(|q| crate::curves::CK { t: q.t, v: q.get(c), ease: q.ease, tan: q.tan[c] }).collect();
            out.set(c, crate::curves::segment(&curve, i, r));
        }
    } else {
        let f = a.ease.apply((r - a.t) / (b.t - a.t).max(1e-4));
        for c in 0..4 {
            out.set(c, a.get(c) + (b.get(c) - a.get(c)) * f);
        }
    }
    Some(out)
}

/// Les courbes d'un clip pour l'éditeur de courbes (liées : une clé porte les quatre réglages)
pub fn clip_curves(c: &Clip) -> Vec<crate::curves::Curve> {
    use crate::curves::{Curve, CK, W, X, Y, Z};
    [("Position X", X), ("Position Y", Y), ("Taille", Z), ("Opacité", W)]
        .iter()
        .enumerate()
        .map(|(i, (name, col))| Curve { label: crate::i18n::t(name), color: *col, keys: c.keys.iter().map(|k| CK { t: k.t, v: k.get(i), ease: k.ease, tan: k.tan[i] }).collect(), link: 0 })
        .collect()
}

pub fn apply_clip_curves(c: &mut Clip, curves: &[crate::curves::Curve]) {
    let Some(first) = curves.first() else { return };
    c.keys = first
        .keys
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let mut pk = PoseKey { t: k.t, x: 0.0, y: 0.0, scale: 1.0, opacity: 1.0, ease: k.ease, tan: [None; 4] };
            for (ch, cv) in curves.iter().enumerate().take(4) {
                if let Some(x) = cv.keys.get(i) {
                    pk.set(ch, x.v);
                    pk.tan[ch] = x.tan;
                }
            }
            pk
        })
        .collect();
}

fn linear() -> crate::keys::Ease {
    crate::keys::Ease::Linear
}

/// Les points (instant, valeur) d'un réglage animé, avec des points en plus là où le passage n'est pas
/// linéaire : l'expression ffmpeg (en lignes droites) suit alors la courbe
pub fn eased_points(keys: &[PoseKey], offset: f32, f: &dyn Fn(&PoseKey) -> f32) -> Vec<(f32, f32)> {
    use crate::keys::Ease;
    let mut out = vec![];
    for (i, k) in keys.iter().enumerate() {
        out.push((offset + k.t, f(k)));
        let Some(n) = keys.get(i + 1) else { break };
        let (va, vb) = (f(k), f(n));
        if k.tan.iter().chain(n.tan.iter()).any(|x| x.is_some()) {
            // Tangentes : on suit la courbe de près
            for s in 1..16 {
                let r = k.t + (n.t - k.t) * s as f32 / 16.0;
                if let Some(q) = pose_in(keys, r) {
                    out.push((offset + r, f(&q)));
                }
            }
            continue;
        }
        match k.ease {
            Ease::Linear => {}
            Ease::Hold => out.push((offset + n.t - 0.001, va)),
            e => {
                let steps = 12;
                for s in 1..steps {
                    let x = s as f32 / steps as f32;
                    out.push((offset + k.t + (n.t - k.t) * x, va + (vb - va) * e.apply(x)));
                }
            }
        }
    }
    out
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

/// Une LUT .cube donnée telle quelle (un look, le profil d'une caméra), au lieu d'un graphe de nodes
pub fn is_cube(grade: &str) -> bool {
    grade.to_lowercase().ends_with(".cube")
}

/// La table de couleurs à appliquer à un clip : son fichier .cube, ou celle calculée de son graphe
pub fn grade_lut(project: &Path, grade: &str) -> PathBuf {
    if is_cube(grade) {
        resolve(project, grade)
    } else {
        lut_file(&resolve(project, grade))
    }
}

/// Calcule les tables de couleurs manquantes. Renvoie vrai si au moins une a été écrite.
pub fn bake_luts(p: &Project, project: &Path, root: &str) -> bool {
    let mut any = false;
    for c in p.clips.iter().filter(|c| !c.grade.is_empty() && !is_cube(&c.grade)) {
        let g = resolve(project, &c.grade);
        let out = lut_file(&g);
        if out.exists() || !g.exists() {
            continue;
        }
        if let Ok(prog) = crate::nodes::Graph::load(&g).and_then(|gr| crate::nodes::Program::compile(&gr, root, 64).map_err(|e| e.1)) {
            if let Some(d) = out.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            any |= crate::vault::atomic_write(&out, prog.lut_cube(33)).is_ok();
        }
    }
    any
}

impl Clip {
    /// Clip vide pour les tests
    #[cfg(test)]
    pub fn test() -> Clip {
        Clip { name: "c".into(), file: "c.mp4".into(), kind: "video".into(), track: 0, start: 0.0, src_in: 0.0, src_out: 1.0, media: 10.0, has_audio: false, speed: 1.0, opacity: 1.0, volume: 1.0, fade_in: 0.0, fade_out: 0.0, brightness: 0.0, contrast: 1.0, saturation: 1.0, wheels: [[0.0; 3]; 3], x: 0.0, y: 0.0, scale: 1.0, grade: String::new(), keys: vec![], text: String::new(), text_size: 72.0, text_color: [255; 3], font: String::new(), outline: false, xfade: 0.0, audio: false, link: 0, fx: vec![], stab: 0.0, id: 0 }
    }

    /// Placement, taille et opacité à l'instant `t` de la timeline, d'après les clés (None sans clé).
    pub fn pose_at(&self, t: f32) -> Option<PoseKey> {
        pose_in(&self.keys, t - self.start)
    }

    /// Pose une clé à l'instant `t` de la timeline avec les réglages actuels.
    pub fn set_key(&mut self, t: f32) {
        let r = ((t - self.start).clamp(0.0, self.len()) * 100.0).round() / 100.0;
        // Une clé neuve part en douceur, comme partout dans l'app
        let key = PoseKey { t: r, x: self.x, y: self.y, scale: self.scale, opacity: self.opacity, ease: crate::keys::Ease::Smooth, tan: [None; 4] };
        match self.keys.iter().position(|k| (k.t - r).abs() < 0.021) {
            Some(i) => self.keys[i] = PoseKey { t: self.keys[i].t, ease: self.keys[i].ease, tan: self.keys[i].tan, ..key },
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
    pub fn visual(&self) -> bool {
        self.kind != "audio" && !self.audio
    }
    /// Ce clip fait entendre du son (il est sur une piste son).
    pub fn sounds(&self) -> bool {
        self.audio && self.has_audio
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Project {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    /// Pistes image (V1 en bas, la plus haute passe devant)
    pub tracks: usize,
    pub clips: Vec<Clip>,
    /// Pistes son (A1, A2…)
    #[serde(default)]
    pub atracks: usize,
    #[serde(default)]
    pub vstate: Vec<TrackState>,
    #[serde(default)]
    pub astate: Vec<TrackState>,
    /// Repères (M), en secondes
    #[serde(default)]
    pub markers: Vec<f32>,
    /// Point d'entrée et de sortie de la séquence (I et O)
    #[serde(default)]
    pub mark_in: Option<f32>,
    #[serde(default)]
    pub mark_out: Option<f32>,
    /// Médias du panneau Projet (chemins comme ceux des clips)
    #[serde(default)]
    pub bin: Vec<String>,
    /// Timeline magnétique : la piste V1 est l'histoire, sans trous ; le reste suit le plan auquel il est accroché
    /// (absent des anciens montages : ils restent libres, on l'allume dans la barre de la timeline)
    #[serde(default)]
    pub magnetic: bool,
    /// Chapitres (bandes nommées au-dessus de la règle), triés par début
    #[serde(default)]
    pub chapters: Vec<Chapter>,
    /// Lecture avec des copies légères des vidéos (proxies), quand elles sont prêtes
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub proxies: bool,
}

/// Le proxy d'une vidéo : une copie en 540p dans le cache, propre à son contenu (chemin, taille, date).
pub fn proxy_path(media: &Path) -> Option<PathBuf> {
    let k = crate::bridge::key(media)?;
    Some(dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("aryadne").join("proxies").join(format!("{k:016x}.mp4")))
}

/// Le proxy s'il est prêt.
pub fn proxy_ready(media: &Path) -> Option<PathBuf> {
    proxy_path(media).filter(|p| p.is_file())
}

/// Fabrique le proxy d'une vidéo : 540 px de haut, images clés serrées pour sauter vite dans la timeline.
pub fn make_proxy(media: &Path) -> Result<PathBuf, String> {
    let out = proxy_path(media).ok_or("média introuvable")?;
    if out.is_file() {
        return Ok(out);
    }
    let ff = ffmpeg::find("ffmpeg").ok_or("ffmpeg n'est pas installé.")?;
    std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
    let tmp = out.with_extension("part.mp4");
    let o = ffmpeg::command(ff).args(["-v", "error", "-y", "-i"]).arg(media).args(["-vf", "scale=-2:'min(540,ih)'", "-c:v", "libx264", "-preset", "veryfast", "-crf", "26", "-g", "12", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "96k"]).arg(&tmp).output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("échec du proxy").to_string());
    }
    std::fs::rename(&tmp, &out).map_err(|e| e.to_string())?;
    Ok(out)
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Chapter {
    pub start: f32,
    pub name: String,
}



/// Nom des titres qui sont des sous-titres
pub const SUBTITLE: &str = "sous-titre";

impl Default for Project {
    fn default() -> Self {
        Project { version: 2, width: 1920, height: 1080, fps: 24.0, tracks: 3, clips: vec![], atracks: 3, vstate: vec![], astate: vec![], markers: vec![], mark_in: None, mark_out: None, bin: vec![], magnetic: true, chapters: vec![], proxies: false }
    }
}

impl Project {
    pub fn read(path: &Path) -> Result<Project, String> {
        let t = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        if t.trim().is_empty() {
            return Ok(Project::default());
        }
        let mut p: Project = serde_json::from_str(&t).map_err(|e| format!("Montage illisible : {e}"))?;
        p.upgrade();
        p.ensure_ids();
        Ok(p)
    }

    /// Donne un identifiant unique à chaque clip qui n'en a pas (ou qui partage celui d'un autre).
    pub fn ensure_ids(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let mut next = self.clips.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        for c in &mut self.clips {
            if c.id == 0 || !seen.insert(c.id) {
                c.id = next;
                seen.insert(next);
                next += 1;
            }
        }
    }
    pub fn index(&self, id: u64) -> Option<usize> {
        self.clips.iter().position(|c| c.id == id)
    }

    /// État d'une piste (image ou son).
    pub fn state(&self, audio: bool, track: usize) -> TrackState {
        (if audio { &self.astate } else { &self.vstate }).get(track).copied().unwrap_or_default()
    }
    pub fn state_mut(&mut self, audio: bool, track: usize) -> &mut TrackState {
        let v = if audio { &mut self.astate } else { &mut self.vstate };
        if v.len() <= track {
            v.resize(track + 1, TrackState::default());
        }
        &mut v[track]
    }
    pub fn lanes(&self, audio: bool) -> usize {
        if audio { self.atracks.max(1) } else { self.tracks.max(1) }
    }
    /// Ce clip est-il rendu (piste visible, ou non muette en tenant compte du solo) ?
    pub fn audible(&self, c: &Clip) -> bool {
        let s = self.state(c.audio, c.track);
        if s.off {
            return false;
        }
        let states = if c.audio { &self.astate } else { &self.vstate };
        !states.iter().any(|x| x.solo) || s.solo
    }
    pub fn next_link(&self) -> u32 {
        self.clips.iter().map(|c| c.link).max().unwrap_or(0) + 1
    }
    /// Indices des clips liés à `i` (lui compris).
    pub fn linked(&self, i: usize) -> Vec<usize> {
        let Some(c) = self.clips.get(i) else { return vec![] };
        if c.link == 0 {
            return vec![i];
        }
        (0..self.clips.len()).filter(|&k| k == i || self.clips[k].link == c.link).collect()
    }

    /// Montages d'avant les pistes son : le son d'une vidéo devient un clip lié sur une piste son.
    pub fn upgrade(&mut self) {
        if self.version >= 2 {
            return;
        }
        self.version = 2;
        self.atracks = self.atracks.max(2);
        let n = self.clips.len();
        for i in 0..n {
            if self.clips[i].kind == "audio" {
                self.clips[i].audio = true;
                self.clips[i].track = 0;
            } else if self.clips[i].has_audio && self.clips[i].kind == "video" && self.clips[i].link == 0 {
                let id = self.next_link();
                let mut a = self.clips[i].clone();
                (a.audio, a.track, a.link, a.keys) = (true, 0, id, vec![]);
                self.clips[i].link = id;
                self.clips.push(a);
            }
        }
        // Clips son posés à la suite les uns des autres sans se recouvrir
        for i in 0..self.clips.len() {
            if self.clips[i].audio {
                self.settle(i);
            }
        }
    }

    /// Ajoute un clip importé ; une vidéo avec du son arrive avec son clip son lié sur `atrack`.
    pub fn add_media(&mut self, mut c: Clip, atrack: usize) -> Vec<usize> {
        if c.kind == "audio" {
            (c.audio, c.track) = (true, atrack);
            self.clips.push(c);
            self.ensure_ids();
            return vec![self.clips.len() - 1];
        }
        if c.kind == "video" && c.has_audio {
            let id = self.next_link();
            let mut a = c.clone();
            (a.audio, a.track, a.link) = (true, atrack, id);
            c.link = id;
            self.clips.push(c);
            self.clips.push(a);
            self.ensure_ids();
            return vec![self.clips.len() - 2, self.clips.len() - 1];
        }
        self.clips.push(c);
        self.ensure_ids();
        vec![self.clips.len() - 1]
    }

    /// Timeline magnétique : resserre l'histoire (V1) sans trous, depuis 0, puis déplace chaque clip
    /// accroché (titres, calques, sons) du même décalage que le plan de V1 sous lequel il commençait
    /// dans `before`. Les sons liés à un plan de V1 le suivent par leur lien. Vrai si quelque chose a bougé.
    pub fn magnetize(&mut self, before: &Project) -> bool {
        let main = |c: &Clip| !c.audio && c.track == 0;
        let mut order: Vec<usize> = (0..self.clips.len()).filter(|&i| main(&self.clips[i])).collect();
        order.sort_by(|&a, &b| self.clips[a].start.total_cmp(&self.clips[b].start));
        // Décalage de chaque plan de V1 (par identifiant)
        let mut delta: Vec<(u64, f32)> = vec![];
        let mut t = 0.0f32;
        for (n, &i) in order.iter().enumerate() {
            let c = &mut self.clips[i];
            // Un fondu enchaîné fait se chevaucher le plan et le précédent
            let want = if n == 0 { 0.0 } else { (t - c.xfade).max(0.0) };
            let d = want - c.start;
            if d.abs() > 1e-4 {
                c.start = want;
            }
            delta.push((c.id, d));
            t = c.end();
        }
        let mut moved = delta.iter().any(|(_, d)| d.abs() > 1e-4);
        let of_id = |id: u64| delta.iter().find(|x| x.0 == id).map(|x| x.1);
        // Plan de V1 sous un instant, dans l'état d'avant
        let host = |at: f32| before.clips.iter().filter(|c| main(c) && c.start - 1e-3 <= at && at < c.end() - 1e-3).map(|c| c.id).next();
        let link_of: Vec<(u32, u64)> = self.clips.iter().filter(|c| main(c) && c.link != 0).map(|c| (c.link, c.id)).collect();
        let mut shifted: Vec<usize> = vec![];
        for (ci, c) in self.clips.iter_mut().enumerate().filter(|(_, c)| !main(c)) {
            // Le son lié à un plan de V1 suit ce plan ; le reste suit le plan sous son début (avant le changement)
            let by_link = (c.link != 0).then(|| link_of.iter().find(|(l, _)| *l == c.link).map(|x| x.1)).flatten();
            let old_start = before.clips.iter().find(|b| b.id == c.id).map(|b| b.start).unwrap_or(c.start);
            let anchor = by_link.or_else(|| host(old_start));
            if let Some(d) = anchor.and_then(of_id) {
                let want = (old_start + d).max(0.0);
                if (c.start - want).abs() > 1e-4 && (c.start - old_start).abs() < 1e-4 {
                    c.start = want;
                    moved = true;
                    shifted.push(ci);
                }
            }
        }
        // Un clip déplacé qui tombe sur un autre passe sur la piste libre suivante, tout de suite
        // (sinon c'est la réouverture du montage qui le ferait, par surprise)
        for i in shifted {
            self.settle(i);
        }
        moved
    }

    pub fn write(&self, path: &Path) -> Result<(), String> {
        crate::vault::atomic_write(path, serde_json::to_string_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
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
        // Le son lié suit l'image
        for k in self.linked(i) {
            self.clips[k].start = (prev - d).max(0.0);
            self.clips[k].xfade = d;
        }
        true
    }

    /// Le clip `i` en chevauche-t-il un autre sur sa piste (au-delà d'un fondu enchaîné voulu) ?
    fn collides(&self, i: usize, track: usize) -> bool {
        let c = &self.clips[i];
        self.clips.iter().enumerate().any(|(k, o)| {
            if k == i || o.track != track || o.audio != c.audio {
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
        let (from, audio) = (self.clips[i].track, self.clips[i].audio);
        let mut t = from + 1;
        while t < self.lanes(audio) && self.collides(i, t) {
            t += 1;
        }
        if audio {
            self.atracks = self.atracks.max(t + 1);
        } else {
            self.tracks = self.tracks.max(t + 1);
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
        wheels: [[0.0; 3]; 3],
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
            audio: false,
            link: 0,
            fx: vec![],
            stab: 0.0,
            id: 0,
        });
        self.ensure_ids();
        self.clips.len() - 1
    }

    /// Sous-titres : un titre par réplique sur la piste du haut, en bas de l'image ; les anciens sous-titres partent.
    pub fn set_subtitles(&mut self, cues: &[crate::transcribe::Cue]) -> usize {
        self.clips.retain(|c| !(c.kind == "text" && c.name == SUBTITLE));
        let top = self.tracks.saturating_sub(1);
        for cue in cues {
            let i = self.add_text(cue.start);
            let c = &mut self.clips[i];
            c.name = SUBTITLE.into();
            c.track = top;
            c.src_out = (cue.end - cue.start).max(0.2);
            c.text = cue.text.clone();
            c.text_size = (self.height as f32 / 22.0).round().max(14.0);
            c.y = self.height as f32 * 0.38;
            c.fade_in = 0.0;
            c.fade_out = 0.0;
        }
        cues.len()
    }

    /// Sous-titres mot par mot (formats verticaux) : des groupes de `group` mots, gros et centrés, le mot dit
    /// à cet instant en couleur vive. Le temps de chaque mot suit sa longueur dans la réplique.
    pub fn set_word_captions(&mut self, cues: &[crate::transcribe::Cue], group: usize) -> usize {
        self.clips.retain(|c| !(c.kind == "text" && c.name == SUBTITLE));
        let top = self.tracks.saturating_sub(1);
        let group = group.max(1);
        let mut n = 0;
        for cue in cues {
            let words: Vec<&str> = cue.text.split_whitespace().collect();
            if words.is_empty() {
                continue;
            }
            let total: usize = words.iter().map(|w| w.chars().count() + 1).sum();
            let mut t = cue.start;
            let times: Vec<(f32, f32)> = words.iter().map(|w| {
                let d = (cue.end - cue.start) * (w.chars().count() + 1) as f32 / total as f32;
                let r = (t, t + d);
                t += d;
                r
            }).collect();
            for (g, chunk) in words.chunks(group).enumerate() {
                for (k, _) in chunk.iter().enumerate() {
                    let wi = g * group + k;
                    let text = chunk.iter().enumerate().map(|(j, w)| if j == k { format!("**{w}**") } else { w.to_string() }).collect::<Vec<_>>().join(" ");
                    let (a, b) = times[wi];
                    let i = self.add_text(a);
                    let c = &mut self.clips[i];
                    c.name = SUBTITLE.into();
                    c.track = top;
                    c.src_out = (b - a).max(0.05);
                    c.text = text;
                    c.text_size = (self.height.min(self.width) as f32 / 11.0).round().max(16.0);
                    c.y = self.height as f32 * 0.22;
                    c.outline = true;
                    c.fade_in = 0.0;
                    c.fade_out = 0.0;
                    n += 1;
                }
            }
        }
        n
    }

    /// Les sous-titres du montage, dans l'ordre.
    pub fn subtitles(&self) -> Vec<crate::transcribe::Cue> {
        let mut v: Vec<crate::transcribe::Cue> = self.clips.iter().filter(|c| c.kind == "text" && c.name == SUBTITLE).map(|c| crate::transcribe::Cue { start: c.start, end: c.end(), text: c.text.replace("**", "") }).collect();
        v.sort_by(|a, b| a.start.total_cmp(&b.start));
        // Mot par mot : un même groupe répété se fond en une réplique (pour l'export SRT)
        let mut out: Vec<crate::transcribe::Cue> = vec![];
        for c in v {
            match out.last_mut() {
                Some(l) if l.text == c.text && (c.start - l.end).abs() < 0.05 => l.end = c.end,
                _ => out.push(c),
            }
        }
        out
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
        b.xfade = 0.0;
        b.id = 0;
        // Les clés passent à la moitié qui les contient
        let at = t - c.start;
        if let Some(mid) = c.pose_at(t) {
            self.clips[i].keys.retain(|k| k.t < at - 1e-3);
            self.clips[i].keys.push(PoseKey { t: at, ..mid });
            b.keys = std::iter::once(PoseKey { t: 0.0, ..mid }).chain(c.keys.iter().filter(|k| k.t > at + 1e-3).map(|k| PoseKey { t: k.t - at, ..*k })).collect();
        }
        self.clips.push(b);
        self.ensure_ids();
        Some(self.clips.len() - 1)
    }

    /// Les changements de plan d'un média entre `src_in` et `src_out` (secondes de la source), détectés par
    /// ffmpeg sur une image réduite ; `threshold` entre 0 (tout) et 1 (rien), 0,3 convient d'habitude
    pub fn scene_cuts(ff: &std::path::Path, media: &std::path::Path, src_in: f32, src_out: f32, threshold: f32) -> Vec<f32> {
        let filter = format!("scale=160:-2,select='gt(scene,{threshold:.3})',showinfo");
        let o = crate::ffmpeg::command(ff.to_path_buf()).args(["-hide_banner", "-nostats", "-ss", &format!("{src_in:.3}"), "-to", &format!("{src_out:.3}"), "-i"]).arg(media).args(["-an", "-vf", &filter, "-f", "null", "-"]).output();
        let Ok(o) = o else { return vec![] };
        let err = String::from_utf8_lossy(&o.stderr);
        let mut cuts: Vec<f32> = err.lines().filter(|l| l.contains("showinfo") && l.contains("pts_time:")).filter_map(|l| l.split("pts_time:").nth(1)?.split_whitespace().next()?.parse::<f32>().ok()).map(|t| src_in + t).filter(|t| *t > src_in + 0.2 && *t < src_out - 0.2).collect();
        cuts.sort_by(f32::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 0.2);
        cuts
    }

    /// Imbrication : les clips `ids` partent dans un nouveau montage (mêmes réglages, ramenés au début) ;
    /// rend ce montage et la place (piste, début) où poser son clip
    pub fn nest(&mut self, ids: &[u64]) -> Option<(Project, usize, f32)> {
        let picked: Vec<Clip> = self.clips.iter().filter(|c| ids.contains(&c.id)).cloned().collect();
        if picked.is_empty() {
            return None;
        }
        let t0 = picked.iter().map(|c| c.start).fold(f32::MAX, f32::min);
        let vtracks: Vec<usize> = picked.iter().filter(|c| c.kind != "audio").map(|c| c.track).collect();
        let track = vtracks.iter().copied().min().unwrap_or(0);
        let mut sub = Project { clips: vec![], ..self.clone() };
        sub.markers.clear();
        sub.chapters.clear();
        sub.mark_in = None;
        sub.mark_out = None;
        for mut c in picked {
            c.start -= t0;
            if c.kind != "audio" {
                c.track -= track;
            }
            sub.clips.push(c);
        }
        self.clips.retain(|c| !ids.contains(&c.id));
        Some((sub, track, t0))
    }

    /// Coupe à `t` les clips d'identifiants `ids` ; les moitiés droites de clips liés restent liées entre elles
    pub fn split_linked(&mut self, ids: &[u64], t: f32) {
        let mut new_links: HashMap<u32, u32> = HashMap::new();
        for &id in ids {
            let Some(i) = self.index(id) else { continue };
            if let Some(k) = self.split(i, t) {
                let l = self.clips[k].link;
                if l != 0 {
                    let next = self.next_link();
                    let nl = *new_links.entry(l).or_insert(next);
                    self.clips[k].link = nl;
                }
            }
        }
    }

    /// Arrêt sur image : à `t`, le plan `i` est coupé et une image fixe (`still`, déjà extraite) tient pendant
    /// `dur` secondes ; tout ce qui suit, sur toutes les pistes, recule d'autant (le son reste synchro).
    /// Rend l'indice du plan fixe.
    pub fn freeze_frame(&mut self, i: usize, t: f32, dur: f32, still: String) -> Option<usize> {
        let c = self.clips.get(i)?.clone();
        if t < c.start - 1e-3 || t > c.end() + 1e-3 || dur <= 0.0 {
            return None;
        }
        // Ce qui passe à cet instant : vidéo et son sont coupés (ils restent synchro), titres et images fixes s'allongent
        let fixed = |x: &Clip| x.kind == "text" || x.kind == "image";
        let spans = |x: &Clip| x.start < t - 1e-3 && x.end() > t + 1e-3;
        let ids: Vec<u64> = self.clips.iter().enumerate().filter(|(k, x)| spans(x) && (*k == i || !fixed(x))).map(|(_, x)| x.id).collect();
        for x in self.clips.iter_mut().enumerate().filter(|(k, x)| *k != i && spans(x) && fixed(x)).map(|(_, x)| x) {
            x.src_out += dur * x.speed;
        }
        self.split_linked(&ids, t);
        for x in self.clips.iter_mut().filter(|x| x.start >= t - 1e-3) {
            x.start += dur;
        }
        let pose = c.pose_at(t);
        let mut f = Clip { kind: "image".into(), file: still, name: format!("{} (arrêt)", c.name), start: t, src_in: 0.0, src_out: dur, media: 0.0, has_audio: false, speed: 1.0, fade_in: 0.0, fade_out: 0.0, xfade: 0.0, keys: vec![], link: 0, id: 0, ..c };
        if let Some(k) = pose {
            (f.x, f.y, f.scale, f.opacity) = (k.x, k.y, k.scale, k.opacity);
        }
        self.clips.push(f);
        self.ensure_ids();
        Some(self.clips.len() - 1)
    }

    /// Retire [a, b) de la timeline sur les pistes `lanes` (piste son ?, numéro) : ce qui est dedans part,
    /// ce qui suit sur ces pistes recule d'autant. Les autres pistes ne bougent pas.
    pub fn cut_range(&mut self, lanes: &[(bool, usize)], a: f32, b: f32) {
        if b <= a + 1e-3 {
            return;
        }
        let on = |c: &Clip| lanes.contains(&(c.audio, c.track));
        let ids: Vec<u64> = self.clips.iter().filter(|c| on(c)).map(|c| c.id).collect();
        self.split_linked(&ids, b);
        let ids: Vec<u64> = self.clips.iter().filter(|c| on(c)).map(|c| c.id).collect();
        self.split_linked(&ids, a);
        let eps = 1e-3;
        self.clips.retain(|c| !(on(c) && c.start >= a - eps && c.end() <= b + eps));
        for c in self.clips.iter_mut().filter(|c| lanes.contains(&(c.audio, c.track))) {
            if c.start >= b - eps {
                c.start -= b - a;
            }
        }
    }

    /// Retire les silences du clip `i` (et de ses clips liés), en gardant un peu d'air ; rend le temps gagné
    pub fn cut_silences(&mut self, i: usize, buf: &crate::audio::Buffer) -> f32 {
        let Some(c) = self.clips.get(i).cloned() else { return 0.0 };
        let mut lanes: Vec<(bool, usize)> = self.linked(i).iter().map(|&k| (self.clips[k].audio, self.clips[k].track)).collect();
        lanes.dedup();
        let pad = 0.12f32;
        let speed = c.speed.max(1e-3);
        let gaps: Vec<(f32, f32)> = crate::audio::silences(buf, c.src_in as f64, c.src_out as f64, -42.0, 0.45)
            .into_iter()
            .map(|(x, y)| (c.start + (x as f32 - c.src_in) / speed + pad, c.start + (y as f32 - c.src_in) / speed - pad))
            .filter(|(x, y)| y - x > 0.05)
            .collect();
        let mut saved = 0.0;
        for (x, y) in gaps.into_iter().rev() {
            self.cut_range(&lanes, x, y);
            saved += y - x;
        }
        saved
    }

    /// Cale les clips visuels de la piste `track` sur les temps forts : mis bout à bout depuis le premier,
    /// chacun finit sur le temps fort le plus proche de sa fin (au moins `min` s par plan). Les clips liés
    /// (le son des vidéos) suivent. Rend le nombre de plans calés.
    pub fn fit_to_beats(&mut self, track: usize, beats: &[f32], min: f32) -> usize {
        let mut order: Vec<usize> = (0..self.clips.len()).filter(|&i| !self.clips[i].audio && self.clips[i].track == track).collect();
        order.sort_by(|&x, &y| self.clips[x].start.total_cmp(&self.clips[y].start));
        let Some(&first) = order.first() else { return 0 };
        let mut cur = self.clips[first].start;
        let mut n = 0;
        let ids: Vec<u64> = order.iter().map(|&i| self.clips[i].id).collect();
        for id in ids {
            let Some(i) = self.index(id) else { continue };
            let c = self.clips[i].clone();
            let speed = c.speed.max(1e-3);
            // Ce que la source permet au plus (une image n'a pas de limite)
            let max_len = if c.media > 0.0 { (c.media - c.src_in) / speed } else { f32::INFINITY };
            let want = cur + c.len();
            let end = beats.iter().cloned().filter(|b| *b >= cur + min && *b - cur <= max_len + 1e-3).min_by(|a, b| (a - want).abs().total_cmp(&(b - want).abs()));
            let len = match end {
                Some(e) => {
                    n += 1;
                    e - cur
                }
                None => c.len().min(max_len),
            };
            for k in self.linked(i) {
                let lk = &mut self.clips[k];
                lk.start = cur;
                lk.src_out = lk.src_in + len * lk.speed.max(1e-3);
                lk.xfade = 0.0;
            }
            cur += len;
        }
        n
    }
}

/// Taille d'un média (largeur, hauteur en pixels), pour le recadrage
pub fn media_dims(project: &Path, c: &Clip) -> Option<(f32, f32)> {
    let path = source_file(project, &c.file);
    if crate::motion::is_compo(&c.file) {
        let comp = crate::motion::Compo::read(&path).ok()?;
        return Some((comp.w as f32, comp.h as f32));
    }
    if c.kind == "image" {
        if let Ok((w, h)) = image::image_dimensions(&path) {
            return Some((w as f32, h as f32));
        }
    }
    let ffprobe = ffmpeg::find("ffprobe")?;
    let o = ffmpeg::command(ffprobe).args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=p=0:s=x"]).arg(&path).output().ok()?;
    let t = String::from_utf8_lossy(&o.stdout);
    let (w, h) = t.trim().split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// La version verticale (9:16) d'un montage : même histoire, chaque plan agrandi pour remplir le cadre
/// (au lieu des bandes noires), centré ; les titres gardent leur taille. `dims` donne la taille d'un média.
pub fn vertical_version(p: &Project, dims: impl Fn(&Clip) -> Option<(f32, f32)>) -> Project {
    let (ow, oh) = (p.width.max(2) as f32, p.height.max(2) as f32);
    let short = p.width.min(p.height).max(2);
    let mut v = p.clone();
    (v.width, v.height) = (short, short * 16 / 9 / 2 * 2);
    let (nw, nh) = (v.width as f32, v.height as f32);
    for c in v.clips.iter_mut() {
        if c.kind == "audio" || c.kind == "text" {
            continue;
        }
        // « Contenir » → « couvrir » dans le nouveau cadre ; les décalages suivent la taille du cadre
        let k = match dims(c) {
            Some((mw, mh)) if mw > 0.0 && mh > 0.0 => {
                let (a, b) = (nw / mw, nh / mh);
                a.max(b) / a.min(b)
            }
            _ => 1.0,
        };
        let (fx, fy) = (nw / ow, nh / oh);
        c.scale *= k;
        (c.x, c.y) = (c.x * fx, c.y * fy);
        for key in c.keys.iter_mut() {
            key.scale *= k;
            (key.x, key.y) = (key.x * fx, key.y * fy);
        }
    }
    v
}

pub fn resolve(project: &Path, file: &str) -> PathBuf {
    let p = Path::new(file);
    let p = if p.is_absolute() { p.to_path_buf() } else { project.parent().unwrap_or(Path::new("")).join(p) };
    // Une composition Motion se lit par sa vidéo rendue (remise à jour en arrière-plan quand elle change)
    if crate::motion::is_compo(file) {
        return crate::motion::render_cached(&p, false);
    }
    // Une sous-séquence (montage imbriqué) aussi
    if is_project(file) && p != project {
        return nested_cached(&p, false);
    }
    p
}

/// La vidéo d'un montage imbriqué, rendue en cache (refaite quand le montage change)
pub fn nested_cached(seq: &Path, wait: bool) -> PathBuf {
    let out = crate::motion::cache_path(seq).with_extension("seq.mp4");
    if !crate::motion::stale(seq, &out) {
        return out;
    }
    let busy = !crate::motion::running().lock().unwrap().insert(out.clone());
    if busy {
        if wait {
            while crate::motion::running().lock().unwrap().contains(&out) {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        return out;
    }
    let (s2, o2) = (seq.to_path_buf(), out.clone());
    let job = move || {
        let r = Project::read(&s2).and_then(|p| {
            if let Some(d) = o2.parent() {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            export_as(&p, &s2, &o2, 0)
        });
        if let Err(e) = r {
            eprintln!("rendu de {} : {e}", s2.display());
        }
        crate::motion::running().lock().unwrap().remove(&o2);
        crate::motion::bump_generation();
    };
    if wait {
        job();
    } else {
        std::thread::spawn(job);
    }
    out
}

/// Le fichier d'un clip tel qu'il est sur le disque (la composition elle-même, pas sa vidéo)
fn source_file(project: &Path, file: &str) -> PathBuf {
    let p = Path::new(file);
    if p.is_absolute() { p.to_path_buf() } else { project.parent().unwrap_or(Path::new("")).join(p) }
}

pub(crate) fn kind_of(path: &Path) -> &'static str {
    match path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default().as_str() {
        "aryimg" | "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tga" | "tif" | "tiff" => "image",
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "opus" => "audio",
        // Une composition Motion se monte comme une vidéo
        _ => "video",
    }
}

/// Lit ce qu'il faut savoir d'un média pour en faire un clip.
pub fn import(project: &Path, media: &Path, track: usize, start: f32) -> Result<Clip, String> {
    let kind = kind_of(media);
    let (mut dur, mut has_audio) = (5.0f32, false);
    if is_project(&media.to_string_lossy()) {
        let p = Project::read(media)?;
        dur = p.duration().max(0.1);
        has_audio = p.clips.iter().any(|c| c.sounds());
        nested_cached(media, false);
    } else if crate::motion::is_compo(&media.to_string_lossy()) {
        // Sa vidéo est rendue maintenant, pour que l'aperçu la trouve
        let c = crate::motion::Compo::read(media)?;
        dur = c.duration;
        has_audio = c.music.is_some();
        // Sa vidéo se prépare en arrière-plan ; le montage la montre dès qu'elle est prête
        crate::motion::render_cached(media, false);
    } else if kind != "image" {
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
        wheels: [[0.0; 3]; 3],
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
        audio: false,
        link: 0,
        fx: vec![],
        stab: 0.0,
        id: 0,
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
    args.extend(["-f".into(), "lavfi".into(), "-i".into(), format!("color=c=black:s={}x{}:r={}:d={total:.3}", w.max(2), h.max(2), p.fps)]);
    let mut filter = String::new();
    let (mut last, mut n, mut audio) = ("0:v".to_string(), 1usize, vec![]);
    // Seuls les clips qui se voient ou s'entendent ouvrent leur média
    let mut order: Vec<&Clip> = p.clips.iter().filter(|c| c.end() > from + 1e-3 && c.len() > 1e-3 && p.audible(c) && if c.audio { with_audio && c.has_audio } else { w > 0 }).collect();
    order.sort_by(|a, b| a.track.cmp(&b.track).then(a.start.total_cmp(&b.start)));
    for c in order {
        // Ce qui reste du clip après `from`
        let skipped = (from - c.start).max(0.0);
        let (st, en) = ((c.start - from).max(0.0), c.end() - from);
        let (src_in, src_len) = (c.src_in + skipped * c.speed, (c.src_out - c.src_in) - skipped * c.speed);
        let path = if c.kind == "text" {
            text_file(c, p)
        } else {
            let m = resolve(project, &c.file);
            // Aperçu de l'image seule : le proxy s'il est prêt (l'export et le son gardent l'original)
            match (realtime && !with_audio && p.proxies && c.kind == "video").then(|| proxy_ready(&m)).flatten() {
                Some(px) => px,
                None => crate::bridge::media(&m),
            }
        }
        .to_string_lossy()
        .to_string();
        if c.kind == "image" || c.kind == "text" {
            args.extend(["-loop".into(), "1".into(), "-t".into(), format!("{:.3}", en - st), "-i".into(), path]);
        } else {
            args.extend(["-ss".into(), format!("{src_in:.3}"), "-t".into(), format!("{src_len:.3}"), "-i".into(), path]);
        }
        if w > 0 && c.visual() && p.audible(c) && (c.opacity > 0.0 || !c.keys.is_empty()) {
            // Clés : chaque réglage animé devient une expression du temps de la timeline
            let pts = |f: &dyn Fn(&PoseKey) -> f32| -> Vec<(f32, f32)> { eased_points(&c.keys, c.start - from, f) };
            let animated = c.keys.len() > 1;
            filter += &format!("[{n}:v]fps={},setpts=(PTS-STARTPTS)/{:.4}+{st:.3}/TB", p.fps, c.speed.max(0.01));
            // Stabilisation : l'image tremble moins, un léger agrandissement cache les bords
            if c.stab > 0.0 && c.kind == "video" {
                let z = 1.0 + c.stab.clamp(0.0, 40.0) / 100.0;
                filter += &format!(",deshake=rx=48:ry=48:edge=mirror,scale=w='trunc(iw*{z:.3}/2)*2':h='trunc(ih*{z:.3}/2)*2',crop=w='trunc(iw/{z:.3}/2)*2':h='trunc(ih/{z:.3}/2)*2'");
            }
            if !c.grade.is_empty() {
                let lut = grade_lut(project, &c.grade);
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
            if !wheels_neutral(&c.wheels) {
                filter += &format!(",{}", colorbalance(&c.wheels));
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
        if with_audio && c.sounds() && p.audible(c) && c.volume > 0.0 && c.kind != "image" && c.kind != "text" {
            filter += &format!("[{n}:a]asetpts=PTS-STARTPTS,atempo={:.4},volume={:.3}", c.speed.clamp(0.5, 2.0), c.volume);
            // Le son suit les fondus de l'image, et s'efface sous un clip qui arrive en fondu enchaîné
            let fin = c.fade_in.max(c.xfade);
            if fin > 0.0 && skipped == 0.0 {
                filter += &format!(",afade=t=in:st=0:d={fin:.3}");
            }
            let next = p.clips.iter().filter(|b| b.track == c.track && b.audio == c.audio && b.xfade > 0.0 && b.start > c.start && b.start < c.end()).map(|b| (b.start, b.xfade)).min_by(|a, b| a.0.total_cmp(&b.0));
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
    if w == 0 {
        // Son seul : la vidéo de fond est jetée
        filter += "[0:v]nullsink";
    } else {
        filter += &format!("[{last}]null[vout]");
    }
    (args, filter, !audio.is_empty(), total)
}

/// Préréglages d'export : nom, extension, et ce qu'ils deviennent pour ffmpeg
pub const PRESETS: [(&str, &str, &str); 7] = [
    ("MP4 H.264 (YouTube, réseaux)", "mp4", "Bonne qualité, lisible partout"),
    ("MP4 léger (envoi rapide)", "mp4", "Fichier plus petit, pour une revue ou un message"),
    ("ProRes 422 HQ (.mov)", "mov", "Pour l'étalonnage ou un autre logiciel de montage"),
    ("WebM VP9", "webm", "Pour le web"),
    ("GIF animé", "gif", "15 images par seconde, 480 px de large"),
    ("Images PNG", "png", "Une image par trame : nom_00001.png, nom_00002.png…"),
    ("Son seul (WAV)", "wav", "Le mixage, sans l'image"),
];

/// Exporte le montage en MP4 (H.264 + AAC).
#[cfg(test)]
pub fn export(p: &Project, project: &Path, out: &Path) -> Result<(), String> {
    export_as(p, project, out, 0)
}

/// Exporte le montage avec le préréglage `preset` (voir PRESETS).
pub fn export_as(p: &Project, project: &Path, out: &Path, preset: usize) -> Result<(), String> {
    let ff = ffmpeg::find("ffmpeg").ok_or("ffmpeg n'est pas installé.")?;
    if p.clips.is_empty() {
        return Err("Le montage est vide.".into());
    }
    let (w, h) = (p.width.max(2) / 2 * 2, p.height.max(2) / 2 * 2);
    // Les compositions Motion du montage sont rendues à jour avant l'export
    for c in p.clips.iter().filter(|c| crate::motion::is_compo(&c.file)) {
        crate::motion::render_cached(&source_file(project, &c.file), true);
    }
    // Les sous-séquences aussi
    for c in p.clips.iter().filter(|c| is_project(&c.file)) {
        let f = source_file(project, &c.file);
        if f != project {
            nested_cached(&f, true);
        }
    }
    bake_texts(p);
    let (inputs, mut filter, has_audio, total) = graph(p, project, 0.0, w, h, false, true);
    let mut cmd = ffmpeg::command(ff);
    if preset == 4 {
        // GIF : palette calculée sur la vidéo elle-même
        filter += ";[vout]fps=15,scale=480:-2:flags=lanczos,split[ga][gb];[ga]palettegen=stats_mode=diff[gp];[gb][gp]paletteuse=dither=bayer:bayer_scale=4[gout]";
    }
    cmd.args(["-v", "error", "-y"]).args(inputs).args(["-filter_complex", &filter]);
    match preset {
        6 => {
            if !has_audio {
                return Err("Le montage n'a pas de son.".into());
            }
            cmd.args(["-map", "[aout]", "-c:a", "pcm_s24le"]);
        }
        4 => {
            cmd.args(["-map", "[gout]"]);
        }
        5 => {
            cmd.args(["-map", "[vout]", "-c:v", "png"]);
        }
        _ => {
            cmd.args(["-map", "[vout]"]);
            if has_audio {
                match preset {
                    2 => cmd.args(["-map", "[aout]", "-c:a", "pcm_s16le"]),
                    3 => cmd.args(["-map", "[aout]", "-c:a", "libopus", "-b:a", "160k"]),
                    _ => cmd.args(["-map", "[aout]", "-c:a", "aac", "-b:a", "192k"]),
                };
            }
            match preset {
                1 => cmd.args(["-c:v", "libx264", "-preset", "fast", "-crf", "26", "-pix_fmt", "yuv420p", "-movflags", "+faststart"]),
                2 => cmd.args(["-c:v", "prores_ks", "-profile:v", "3", "-pix_fmt", "yuv422p10le"]),
                3 => cmd.args(["-c:v", "libvpx-vp9", "-crf", "32", "-b:v", "0", "-pix_fmt", "yuv420p"]),
                _ => cmd.args(["-c:v", "libx264", "-preset", "medium", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart"]),
            };
        }
    }
    cmd.args(["-t", &format!("{total:.3}")]);
    if preset == 5 {
        // Séquence : nom_00001.png à côté du nom choisi
        let stem = out.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "image".into());
        cmd.arg(out.with_file_name(format!("{stem}_%05d.png")));
    } else {
        cmd.arg(out);
    }
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
pub(crate) struct Strips {
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

pub(crate) const WAVE_RATE: f32 = 50.0;

impl Strips {
    fn channel(&mut self) -> std::sync::mpsc::Sender<StripMsg> {
        if self.tx.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            self.tx = Some(tx);
            self.rx = Some(rx);
        }
        self.tx.clone().unwrap()
    }

    /// Une composition vient d'être rendue : ses vignettes et sa forme d'onde sont à refaire, et les échecs à réessayer
    pub(crate) fn refresh_motion(&mut self) {
        self.thumbs.retain(|k, v| v.is_some() && !crate::motion::is_cache_file(&k.0));
        self.waves.retain(|k, v| v.is_some() && !crate::motion::is_cache_file(k));
    }

    pub(crate) fn poll(&mut self, ctx: &egui::Context) {
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
    pub(crate) fn thumb(&mut self, file: &Path, t: f32, ctx: &egui::Context) -> Option<egui::TextureId> {
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
    pub(crate) fn wave(&mut self, file: &Path, ctx: &egui::Context) -> Option<std::sync::Arc<Vec<f32>>> {
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

pub use crate::montage_ui::Editor;

pub(crate) fn timecode(t: f32, fps: f32) -> String {
    let t = t.max(0.0);
    let f = ((t - t.floor()) * fps.max(1.0)).floor() as u32;
    format!("{:02}:{:02}:{:02}", (t / 60.0) as u32, t as u32 % 60, f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arret_sur_image() {
        let mut p = Project::default();
        let mut v = Clip::test();
        (v.src_in, v.src_out, v.has_audio) = (0.0, 4.0, true);
        p.clips.push(v);
        let mut a = Clip::test();
        (a.kind, a.audio, a.src_out) = ("audio".into(), true, 4.0);
        p.clips.push(a);
        let mut after = Clip::test();
        (after.start, after.src_out) = (4.0, 2.0);
        p.clips.push(after);
        p.ensure_ids();
        let mut titre = Clip::test();
        (titre.kind, titre.track, titre.start, titre.src_out) = ("text".into(), 2, 1.0, 2.0);
        p.clips.push(titre);
        p.ensure_ids();
        let k = p.freeze_frame(0, 1.5, 2.0, "arrets/c 1.5.png".into()).unwrap();
        // Le titre par-dessus n'est pas coupé : il dure 2 s de plus
        let ts: Vec<&Clip> = p.clips.iter().filter(|c| c.kind == "text").collect();
        assert_eq!(ts.len(), 1);
        assert_eq!((ts[0].start, ts[0].len()), (1.0, 4.0));
        let f = &p.clips[k];
        assert_eq!((f.kind.as_str(), f.start, f.len()), ("image", 1.5, 2.0));
        // Le plan est coupé, sa suite et le son reculent de 2 s, le plan d'après aussi
        assert!(p.clips.iter().any(|c| c.kind == "video" && c.start == 3.5 && (c.src_in - 1.5).abs() < 1e-4));
        assert!(p.clips.iter().any(|c| c.audio && c.start == 3.5));
        assert!(p.clips.iter().any(|c| c.kind == "video" && c.start == 6.0));
        assert_eq!(p.duration(), 8.0);
    }

    #[test]
    fn version_verticale() {
        let mut p = Project { width: 1920, height: 1080, ..Default::default() };
        let mut c = Clip::test();
        c.x = 192.0;
        p.clips.push(c);
        let mut t = Clip::test();
        t.kind = "text".into();
        p.clips.push(t);
        let v = vertical_version(&p, |_| Some((1920.0, 1080.0)));
        assert_eq!((v.width, v.height), (1080, 1920));
        // Un plan 16:9 qui remplit un cadre 9:16 : (16/9)² ≈ 3,16 fois plus grand
        assert!((v.clips[0].scale - (16.0f32 / 9.0).powi(2)).abs() < 0.01, "{}", v.clips[0].scale);
        assert!((v.clips[0].x - 108.0).abs() < 0.01);
        assert_eq!(v.clips[1].scale, 1.0);
    }

    #[test]
    fn timeline_magnetique() {
        let clip = |id: u64, track: usize, audio: bool, start: f32, len: f32, link: u32| Clip { id, track, audio, start, src_in: 0.0, src_out: len, link, ..Clip::test() };
        let mut p = Project { clips: vec![clip(1, 0, false, 0.0, 4.0, 1), clip(2, 0, false, 4.0, 3.0, 0), clip(3, 0, false, 7.0, 5.0, 0), clip(4, 1, false, 8.0, 2.0, 0), clip(5, 0, true, 0.0, 4.0, 1)], ..Default::default() };
        let before = p.clone();
        // On retire le plan 2 : le plan 3 recule de 3 s, et le titre accroché au plan 3 aussi
        p.clips.retain(|c| c.id != 2);
        assert!(p.magnetize(&before));
        let at = |p: &Project, id: u64| p.clips.iter().find(|c| c.id == id).unwrap().start;
        assert_eq!(at(&p, 3), 4.0);
        assert_eq!(at(&p, 4), 5.0);
        assert_eq!(at(&p, 5), 0.0);
        // Un plan rallongé pousse la suite, et le son lié reste avec son plan
        let before = p.clone();
        let i = p.clips.iter().position(|c| c.id == 1).unwrap();
        p.clips[i].start = 2.0;
        p.magnetize(&before);
        assert_eq!(at(&p, 1), 0.0);
        assert!(!p.clone().magnetize(&p));
    }

    fn clip(name: &str, start: f32, len: f32, media: f32) -> Clip {
        let mut c = Clip::test();
        c.name = name.into();
        c.start = start;
        c.src_in = 0.0;
        c.src_out = len;
        c.media = media;
        c
    }

    #[test]
    fn retirer_une_plage_et_caler_sur_le_rythme() {
        let mut p = Project::default();
        // V1 : une vidéo de 10 s et son son lié sur A1 ; V2 : un titre qui ne doit pas bouger
        let mut v = clip("v", 0.0, 10.0, 10.0);
        v.link = 1;
        let mut a = v.clone();
        a.audio = true;
        a.kind = "audio".into();
        let mut titre = clip("t", 5.0, 2.0, 0.0);
        titre.track = 1;
        p.clips = vec![v, a, titre];
        p.ensure_ids();
        p.cut_range(&[(false, 0), (true, 0)], 3.0, 5.0);
        let mut v1: Vec<(f32, f32, f32)> = p.clips.iter().filter(|c| !c.audio && c.track == 0).map(|c| (c.start, c.src_in, c.src_out)).collect();
        v1.sort_by(|x, y| x.0.total_cmp(&y.0));
        assert_eq!(v1, vec![(0.0, 0.0, 3.0), (3.0, 5.0, 10.0)]);
        assert_eq!(p.clips.iter().filter(|c| c.audio).count(), 2);
        assert_eq!(p.clips.iter().find(|c| c.name == "t").unwrap().start, 5.0, "les autres pistes restent");
        // Les moitiés droites de la vidéo et du son restent liées entre elles
        let right = p.clips.iter().position(|c| !c.audio && c.start == 3.0).unwrap();
        assert_eq!(p.linked(right).len(), 2);
        // Rythme : temps forts à 0,5 s d'écart ; deux plans calés sur ceux près de leur fin
        let mut q = Project::default();
        q.clips = vec![clip("a", 0.0, 2.2, 10.0), clip("b", 2.2, 1.9, 10.0)];
        q.ensure_ids();
        let beats: Vec<f32> = (1..20).map(|k| k as f32 * 0.5).collect();
        assert_eq!(q.fit_to_beats(0, &beats, 0.5), 2);
        assert_eq!((q.clips[0].start, q.clips[0].len()), (0.0, 2.0));
        assert_eq!((q.clips[1].start, q.clips[1].len()), (2.0, 2.0));
    }

    #[test]
    fn coupe_et_durees() {
        let mut p = Project::default();
        let c = Clip { name: "a".into(), file: "a.mp4".into(), kind: "video".into(), track: 1, start: 2.0, src_in: 1.0, src_out: 9.0, media: 10.0, has_audio: true, speed: 2.0, opacity: 1.0, volume: 1.0, fade_in: 0.5, fade_out: 0.5, brightness: 0.0, contrast: 1.0, saturation: 1.0, wheels: [[0.0; 3]; 3], x: 0.0, y: 0.0, scale: 1.0, grade: String::new(), keys: vec![], text: String::new(), text_size: 72.0, text_color: [255; 3], font: String::new(), outline: false, xfade: 0.0, audio: false, link: 0, fx: vec![], stab: 0.0, id: 0 };
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
        // Montage d'avant les pistes son : chaque moitié reçoit son clip son lié sur A1
        p.version = 1;
        p.upgrade();
        assert_eq!(p.clips.len(), 4);
        assert!(p.clips.iter().filter(|c| c.audio).all(|c| c.track == 0 && c.link != 0));
        assert_eq!(p.linked(0).len(), 2);
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
        let base = Clip { start: 0.0, src_in: 0.0, src_out: 4.0, speed: 1.0, keys: vec![], track: 1, opacity: 1.0, fade_in: 0.0, fade_out: 0.0, link: 0, audio: false, id: 0, ..p.clips[0].clone() };
        q.clips = vec![base.clone(), Clip { start: 4.0, ..base }];
        q.version = 1;
        q.upgrade();
        assert!(q.crossfade(1, 1.0) && !q.crossfade(0, 1.0));
        assert_eq!((q.clips[1].start, q.clips[1].xfade, q.duration()), (3.0, 1.0, 7.0));
        let (_, f, _, _) = graph(&q, Path::new("/x/m.aryav"), 0.0, 640, 360, false, true);
        assert!(f.contains("fade=t=in:st=3.000:d=1.000:alpha=1") && f.contains("afade=t=out:st=3.000:d=1.000") && f.contains("afade=t=in:st=0:d=1.000"), "{f}");
        assert_eq!(expr(&[(1.0, 0.0), (3.0, 10.0)], "t"), "if(lt(t,1.0000),0.0000,if(lt(t,3.0000),0.0000+(10.0000)*(t-(1.0000))/2.0000,10.0000))");
    }

    #[test]
    fn lut_cube() {
        // Une LUT .cube qui inverse les couleurs, appliquée telle quelle à un plan blanc : il devient noir
        let Some(ff) = ffmpeg::find("ffmpeg") else { return };
        let dir = std::env::temp_dir().join(format!("aryadne-lut-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("luts")).unwrap();
        let mut cube = String::from("LUT_3D_SIZE 2\n");
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    cube += &format!("{} {} {}\n", 1 - r, 1 - g, 1 - b);
                }
            }
        }
        std::fs::write(dir.join("luts/inverse.cube"), cube).unwrap();
        let st = ffmpeg::command(ff).args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=white:size=64x48:duration=1:rate=24", "-pix_fmt", "yuv420p"]).arg(dir.join("blanc.mp4")).status().unwrap();
        assert!(st.success());
        let file = dir.join("essai.aryav");
        let mut p = Project { width: 64, height: 48, ..Default::default() };
        let mut c = import(&file, &dir.join("blanc.mp4"), 1, 0.0).unwrap();
        c.grade = "luts/inverse.cube".into();
        assert!(is_cube(&c.grade) && grade_lut(&file, &c.grade).exists());
        p.clips.push(c);
        let img = frame(&p, &file, 0.5, 64).unwrap();
        let px = img.get_pixel(32, 24);
        assert!(px[0] < 30 && px[1] < 30 && px[2] < 30, "{px:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn proxies() {
        let Some(ff) = ffmpeg::find("ffmpeg") else { return };
        let dir = std::env::temp_dir().join(format!("aryadne-proxy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("grand.mp4");
        let ok = ffmpeg::command(ff).args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=s=1920x1080:d=1:r=24"]).arg(&src).status().map(|s| s.success()).unwrap_or(false);
        if !ok {
            return;
        }
        assert!(proxy_ready(&src).is_none());
        let px = make_proxy(&src).unwrap();
        assert_eq!(proxy_ready(&src), Some(px.clone()));
        let img = std::process::Command::new(ffmpeg::find("ffprobe").unwrap()).args(["-v", "error", "-show_entries", "stream=height", "-of", "csv=p=0"]).arg(&px).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&img.stdout).trim(), "540");
        // L'aperçu s'en sert, l'export non
        let mut p = Project::default();
        let mut c = import(&dir.join("m.aryseq"), &src, 0, 0.0).unwrap();
        c.file = "grand.mp4".into();
        p.clips.push(c);
        p.proxies = true;
        let (live, _, _, _) = graph(&p, &dir.join("m.aryseq"), 0.0, 640, 360, true, false);
        assert!(live.iter().any(|a| a == &px.to_string_lossy()));
        let (out, _, _, _) = graph(&p, &dir.join("m.aryseq"), 0.0, 640, 360, false, false);
        assert!(!out.iter().any(|a| a == &px.to_string_lossy()));
        let _ = std::fs::remove_file(&px);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sous_titres_du_montage() {
        let mut p = Project::default();
        let cues = vec![crate::transcribe::Cue { start: 1.0, end: 2.5, text: "Bonjour".into() }, crate::transcribe::Cue { start: 3.0, end: 4.0, text: "Salut".into() }];
        assert_eq!(p.set_subtitles(&cues), 2);
        assert_eq!(p.subtitles(), cues);
        assert!(p.clips.iter().all(|c| c.track == p.tracks - 1 && c.y > 0.0));
        // Refaire remplace, sans doubler
        p.set_subtitles(&cues[..1]);
        assert_eq!(p.subtitles().len(), 1);
    }

    #[test]
    fn prereglages_d_export() {
        // Chaque préréglage produit un fichier ; les roues de couleur passent dans le filtre
        let Some(ff) = ffmpeg::find("ffmpeg") else { return };
        let dir = std::env::temp_dir().join(format!("aryadne-presets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let st = ffmpeg::command(ff).args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=duration=1:size=64x48:rate=12", "-f", "lavfi", "-i", "sine=duration=1", "-shortest", "-pix_fmt", "yuv420p"]).arg(dir.join("m.mp4")).status().unwrap();
        assert!(st.success());
        let file = dir.join("essai.aryav");
        let mut p = Project { width: 64, height: 48, fps: 12.0, ..Default::default() };
        let mut c = import(&file, &dir.join("m.mp4"), 1, 0.0).unwrap();
        c.wheels[0] = [0.0, 0.5, 0.0];
        assert!(colorbalance(&c.wheels).starts_with("colorbalance=rs="));
        p.clips.push(c);
        for (k, (name, ext, _)) in PRESETS.iter().enumerate() {
            if k == 3 {
                continue; // VP9 : souvent absent des ffmpeg minimaux
            }
            if k == 6 {
                // Pas de clip son dans ce montage : un message clair
                assert!(export_as(&p, &file, &dir.join("s.wav"), k).is_err());
                continue;
            }
            let out = dir.join(format!("sortie{k}.{ext}"));
            export_as(&p, &file, &out, k).unwrap_or_else(|e| panic!("{name} : {e}"));
            let made = if k == 5 { dir.join(format!("sortie{k}_00001.png")) } else { out };
            assert!(made.exists() && std::fs::metadata(&made).unwrap().len() > 0, "{name}");
        }
        let _ = std::fs::remove_dir_all(&dir);
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
        p.version = 1;
        p.upgrade();
        p.ensure_ids();
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
        // Le son seul de la lecture, à partir d'une seconde : un WAV d'environ quatre secondes
        let (inputs, filter, has, _) = graph(&p, &file, 1.0, 0, 0, false, true);
        assert!(has && !filter.contains("[vout]"));
        let o = ffmpeg::command(ff).args(["-v", "error"]).args(inputs).args(["-filter_complex", &filter, "-map", "[aout]", "-ar", "48000", "-ac", "2", "-f", "wav", "-"]).output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let secs = (o.stdout.len() as f32 - 44.0) / (48000.0 * 4.0);
        assert!((secs - 4.0).abs() < 0.4, "{secs}");
    }
}

#[cfg(test)]
mod scene_tests {
    #[test]
    fn changements_de_plan() {
        let Some(ff) = crate::ffmpeg::find("ffmpeg") else { return };
        let path = std::env::temp_dir().join(format!("ary-cuts-{}.mp4", std::process::id()));
        let ok = crate::ffmpeg::command(ff.clone())
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=red:s=160x90:d=1.5:r=25", "-f", "lavfi", "-i", "color=c=blue:s=160x90:d=1.5:r=25", "-f", "lavfi", "-i", "color=c=green:s=160x90:d=1:r=25", "-filter_complex", "[0][1][2]concat=n=3:v=1:a=0", "-pix_fmt", "yuv420p"])
            .arg(&path)
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            return;
        }
        let cuts = super::Project::scene_cuts(&ff, &path, 0.0, 4.0, 0.3);
        let _ = std::fs::remove_file(&path);
        assert_eq!(cuts.len(), 2, "{cuts:?}");
        assert!((cuts[0] - 1.5).abs() < 0.1 && (cuts[1] - 3.0).abs() < 0.1, "{cuts:?}");
    }
}

#[cfg(test)]
mod nest_tests {
    use super::*;

    #[test]
    fn imbrication() {
        if crate::ffmpeg::find("ffmpeg").is_none() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("ary-nest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        image::RgbaImage::from_pixel(64, 36, image::Rgba([200, 40, 40, 255])).save(dir.join("rouge.png")).unwrap();
        let main = dir.join("principal.aryav");
        let mut p = Project::default();
        let mut a = import(&main, &dir.join("rouge.png"), 0, 1.0).unwrap();
        a.src_out = 2.0;
        p.clips.push(a);
        p.ensure_ids();
        let id = p.clips[0].id;
        let (sub, track, t0) = p.nest(&[id]).unwrap();
        assert!(p.clips.is_empty() && track == 0 && (t0 - 1.0).abs() < 1e-4);
        assert!((sub.clips[0].start).abs() < 1e-4);
        let subf = dir.join("principal - imbrication 1.aryav");
        sub.write(&subf).unwrap();
        let c = import(&main, &subf, track, t0).unwrap();
        assert!((c.media - sub.duration()).abs() < 0.05 || (c.src_out - sub.duration()).abs() < 0.05);
        p.clips.push(c);
        p.ensure_ids();
        p.write(&main).unwrap();
        let out = dir.join("sortie.mp4");
        export_as(&p, &main, &out, 0).unwrap();
        assert!(out.exists() && std::fs::metadata(&out).unwrap().len() > 1000);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod words_tests {
    #[test]
    fn sous_titres_mot_par_mot() {
        let mut p = super::Project::default();
        let cues = vec![crate::transcribe::Cue { start: 1.0, end: 3.0, text: "un deux trois quatre".into() }];
        let n = p.set_word_captions(&cues, 3);
        assert_eq!(n, 4);
        let mut subs: Vec<&super::Clip> = p.clips.iter().filter(|c| c.name == super::SUBTITLE).collect();
        subs.sort_by(|a, b| a.start.total_cmp(&b.start));
        assert_eq!(subs[1].text, "un **deux** trois");
        assert_eq!(subs[3].text, "**quatre**");
        assert!((subs[0].start - 1.0).abs() < 1e-4 && (subs[3].end() - 3.0).abs() < 1e-3);
        // Pour le SRT : deux répliques, sans les marques
        let srt = p.subtitles();
        assert_eq!(srt.len(), 2);
        assert_eq!(srt[0].text, "un deux trois");
        // Le rendu ne montre pas les étoiles, et colore le mot
        let img = super::render_text(subs[1], 600, 120);
        assert!(img.pixels().any(|px| px.0[0] > 240 && px.0[1] > 190 && px.0[2] < 120 && px.0[3] > 200));
    }
}

