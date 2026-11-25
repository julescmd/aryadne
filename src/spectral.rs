//! Vue spectrale du son (comme Audition ou iZotope RX) : le spectrogramme d'un son, un pinceau pour
//! choisir une tache (un clic, une sonnerie, un sifflement) et l'atténuer ou la réparer à partir de ce
//! qui l'entoure. Le son retouché est écrit dans un nouveau fichier : l'original ne change pas.

use crate::audio::{RATE, fft_dir};
use eframe::egui::{self, Color32, Rect, Sense, Stroke, pos2, vec2};
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Taille de la fenêtre d'analyse, et pas entre deux colonnes (recouvrement de 75 %)
pub const N: usize = 2048;
pub const HOP: usize = 512;
pub const BINS: usize = N / 2 + 1;
/// Fréquence du haut de la vue
const FMAX: f32 = RATE as f32 / 2.0;

fn hann() -> Vec<f64> {
    (0..N).map(|k| 0.5 - 0.5 * (std::f64::consts::TAU * k as f64 / N as f64).cos()).collect()
}

/// La colonne `c` (centrée sur l'échantillon c × HOP) d'un signal donné par `sample`, fenêtrée et transformée
fn column(sample: &dyn Fn(i64) -> f64, c: usize, win: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let start = (c * HOP) as i64 - (N / 2) as i64;
    let mut re: Vec<f64> = (0..N).map(|k| sample(start + k as i64) * win[k]).collect();
    let mut im = vec![0.0f64; N];
    fft_dir(&mut re, &mut im, false);
    (re, im)
}

/// Un spectrogramme : `cols` colonnes de `BINS` niveaux en dB (0 dB : un sinus à pleine échelle)
pub struct Spec {
    pub cols: usize,
    pub db: Vec<f32>,
}

impl Spec {
    pub fn at(&self, c: usize, b: usize) -> f32 {
        self.db.get(c * BINS + b).copied().unwrap_or(-140.0)
    }
}

/// Le nombre de colonnes d'un son de `frames` échantillons
pub fn cols_of(frames: usize) -> usize {
    frames / HOP + 1
}

/// Analyse un son stéréo entrelacé (les deux canaux ensemble)
pub fn analyse(data: &[f32]) -> Spec {
    let frames = data.len() / 2;
    let cols = cols_of(frames);
    let win = hann();
    let sample = |i: i64| if i < 0 || i as usize >= frames { 0.0 } else { (data[i as usize * 2] + data[i as usize * 2 + 1]) as f64 * 0.5 };
    let db: Vec<f32> = (0..cols)
        .into_par_iter()
        .flat_map_iter(|c| {
            let (re, im) = column(&sample, c, &win);
            (0..BINS).map(move |b| (20.0 * ((re[b] * re[b] + im[b] * im[b]).sqrt() / (N as f64 / 4.0)).max(1e-7).log10()) as f32)
        })
        .collect();
    Spec { cols, db }
}

/// L'échelle de hauteur de la vue (mel) : la fréquence à la hauteur `v` (0 en bas, 1 en haut), et l'inverse
pub fn freq_of(v: f32) -> f32 {
    let top = 2595.0 * (1.0 + FMAX / 700.0).log10();
    700.0 * (10f32.powf(v.clamp(0.0, 1.0) * top / 2595.0) - 1.0)
}

pub fn v_of(f: f32) -> f32 {
    let top = 2595.0 * (1.0 + FMAX / 700.0).log10();
    (2595.0 * (1.0 + f.max(0.0) / 700.0).log10() / top).clamp(0.0, 1.0)
}

fn bin_of(f: f32) -> f32 {
    f * N as f32 / RATE as f32
}

/// Couleur d'un niveau (0..1) : du noir au jaune pâle en passant par le violet et l'orange
fn heat(x: f32) -> [u8; 3] {
    const STOPS: [[f32; 3]; 6] = [[6.0, 5.0, 14.0], [48.0, 18.0, 92.0], [130.0, 34.0, 120.0], [214.0, 72.0, 78.0], [250.0, 160.0, 50.0], [252.0, 250.0, 200.0]];
    let x = x.clamp(0.0, 1.0) * 5.0;
    let i = (x as usize).min(4);
    let f = x - i as f32;
    [0, 1, 2].map(|k| (STOPS[i][k] + (STOPS[i + 1][k] - STOPS[i][k]) * f) as u8)
}

/// L'image (RGBA, w × h) des colonnes `c0..c1` du spectrogramme, niveaux entre `floor` et `ceil` dB
pub fn image(spec: &Spec, c0: f64, c1: f64, w: usize, h: usize, floor: f32, ceil: f32) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    if w == 0 || h == 0 || spec.cols == 0 {
        return out;
    }
    // Les cases de fréquence de chaque ligne (du haut vers le bas)
    let rows: Vec<(usize, usize)> = (0..h)
        .map(|y| {
            let (lo, hi) = (bin_of(freq_of(1.0 - (y + 1) as f32 / h as f32)), bin_of(freq_of(1.0 - y as f32 / h as f32)));
            let a = (lo as usize).min(BINS - 1);
            (a, (hi.ceil() as usize).clamp(a + 1, BINS))
        })
        .collect();
    let span = (c1 - c0).max(1e-6) / w as f64;
    out.par_chunks_mut(4).enumerate().for_each(|(i, px)| {
        let (x, y) = (i % w, i / w);
        let ca = c0 + x as f64 * span;
        if ca < 0.0 || ca >= spec.cols as f64 {
            px.copy_from_slice(&[6, 5, 14, 255]);
            return;
        }
        let (a, b) = (ca as usize, ((ca + span).ceil() as usize).clamp(ca as usize + 1, spec.cols));
        // Au plus 6 colonnes par pixel : le plus fort l'emporte (un clic reste visible de loin)
        let step = (b - a).div_ceil(6).max(1);
        let (r0, r1) = rows[y];
        let rstep = (r1 - r0).div_ceil(6).max(1);
        let mut m = -140.0f32;
        for c in (a..b).step_by(step) {
            for r in (r0..r1).step_by(rstep) {
                m = m.max(spec.db[c * BINS + r]);
            }
        }
        let k = heat((m - floor) / (ceil - floor).max(1.0));
        px.copy_from_slice(&[k[0], k[1], k[2], 255]);
    });
    out
}

/// Ce que le pinceau a choisi : un poids (0..1) par case, colonne par colonne (seules les colonnes touchées existent)
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Mask {
    pub cols: BTreeMap<usize, Vec<f32>>,
}

impl Mask {
    pub fn is_empty(&self) -> bool {
        self.cols.is_empty()
    }

    pub fn w(&self, c: usize, b: usize) -> f32 {
        self.cols.get(&c).map(|v| v[b]).unwrap_or(0.0)
    }

    /// Un coup de pinceau : une ellipse de demi-largeur `rc` colonnes et de demi-hauteur `rv` (en hauteur de vue),
    /// centrée sur la colonne `col` et la hauteur `v` ; le bord est doux
    pub fn stamp(&mut self, col: f32, v: f32, rc: f32, rv: f32, max_cols: usize) {
        let (rc, rv) = (rc.max(0.5), rv.max(1e-4));
        let (b0, b1) = (bin_of(freq_of(v - rv)).floor().max(0.0) as usize, (bin_of(freq_of(v + rv)).ceil() as usize).min(BINS - 1));
        let (c0, c1) = ((col - rc).floor().max(0.0) as usize, ((col + rc).ceil().max(0.0) as usize).min(max_cols.saturating_sub(1)));
        // La hauteur de chaque case, une fois pour toutes les colonnes
        let vs: Vec<f32> = (b0..=b1).map(|b| v_of(b as f32 * RATE as f32 / N as f32)).collect();
        for c in c0..=c1 {
            let dx = (c as f32 - col) / rc;
            if dx.abs() > 1.0 {
                continue;
            }
            let colw = self.cols.entry(c).or_insert_with(|| vec![0.0; BINS]);
            for (k, b) in (b0..=b1).enumerate() {
                let dy = (vs[k] - v) / rv;
                let d = (dx * dx + dy * dy).sqrt();
                if d <= 1.0 {
                    // Plein jusqu'à 70 % du rayon, puis fondu
                    let wv = ((1.0 - d) / 0.3).clamp(0.0, 1.0);
                    colw[b] = colw[b].max(wv);
                }
            }
        }
    }

    /// Un trait de pinceau entre deux points
    pub fn stroke(&mut self, a: (f32, f32), b: (f32, f32), rc: f32, rv: f32, max_cols: usize) {
        let n = (((b.0 - a.0).abs() / (rc * 0.5).max(0.25)).max((b.1 - a.1).abs() / (rv * 0.5).max(1e-4)).ceil() as usize).clamp(1, 4000);
        for k in 0..=n {
            let f = k as f32 / n as f32;
            self.stamp(a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f, rc, rv, max_cols);
        }
    }

    /// Un rectangle plein : colonnes `c0..=c1`, entre deux hauteurs de vue
    pub fn rect(&mut self, c0: usize, c1: usize, v0: f32, v1: f32) {
        let (b0, b1) = (bin_of(freq_of(v0.min(v1))).floor().max(0.0) as usize, (bin_of(freq_of(v0.max(v1))).ceil() as usize).min(BINS - 1));
        for c in c0..=c1 {
            let colw = self.cols.entry(c).or_insert_with(|| vec![0.0; BINS]);
            colw[b0..=b1].iter_mut().for_each(|x| *x = 1.0);
        }
    }
}

/// Comment traiter ce qui est choisi
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    /// Baisser de tant de dB (120 : effacer)
    Attenuate(f32),
    /// Réparer : ramener au niveau de ce qu'il y a juste avant et juste après, à la même fréquence
    Heal,
}

/// Applique le masque au son stéréo entrelacé `data`. Seules les colonnes peintes sont recalculées, et le son
/// ne change que là où elles portent. Rend le nombre de colonnes traitées.
pub fn repair(data: &mut [f32], mask: &Mask, mode: Mode) -> usize {
    let frames = data.len() / 2;
    let (Some((&c0, _)), Some((&c1, _))) = (mask.cols.first_key_value(), mask.cols.last_key_value()) else { return 0 };
    let win = hann();
    // Jusqu'où chercher du son intact de chaque côté (environ 1 s)
    const REACH: usize = 96;
    let base = (c0 * HOP) as i64 - (N / 2) as i64;
    let span = (c1 - c0) * HOP + N;
    for ch in 0..2 {
        let sample = |i: i64| if i < 0 || i as usize >= frames { 0.0 } else { data[i as usize * 2 + ch] as f64 };
        // Les niveaux des colonnes voisines (pour réparer), calculés une fois
        let mags: BTreeMap<usize, Vec<f64>> = if mode == Mode::Heal {
            let (a, b) = (c0.saturating_sub(REACH), (c1 + REACH).min(cols_of(frames).saturating_sub(1)));
            (a..=b).into_par_iter().map(|c| { let (re, im) = column(&sample, c, &win); (c, (0..BINS).map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt()).collect()) }).collect()
        } else {
            BTreeMap::new()
        };
        // La différence à ajouter au son, colonne par colonne
        let parts: Vec<(usize, Vec<f64>)> = mask
            .cols
            .par_iter()
            .map(|(&c, w)| {
                let (mut re, mut im) = column(&sample, c, &win);
                for b in 0..BINS {
                    if w[b] <= 0.0 {
                        re[b] = 0.0;
                        im[b] = 0.0;
                        if b > 0 && b < N / 2 {
                            re[N - b] = 0.0;
                            im[N - b] = 0.0;
                        }
                        continue;
                    }
                    let g = match mode {
                        Mode::Attenuate(db) => 10f64.powf(-(db.max(0.0) as f64) / 20.0),
                        Mode::Heal => {
                            let m = (re[b] * re[b] + im[b] * im[b]).sqrt().max(1e-12);
                            let side = |dir: i64| -> Option<(usize, f64)> {
                                (1..=REACH as i64).map(|d| c as i64 + dir * d).take_while(|x| *x >= 0).map(|x| x as usize).find(|x| mask.w(*x, b) < 0.05).and_then(|x| mags.get(&x).map(|v| (x, v[b])))
                            };
                            let target = match (side(-1), side(1)) {
                                (Some((l, ml)), Some((r, mr))) => {
                                    let f = (c - l) as f64 / (r - l).max(1) as f64;
                                    ml.max(1e-12).powf(1.0 - f) * mr.max(1e-12).powf(f)
                                }
                                (Some((_, v)), None) | (None, Some((_, v))) => v,
                                (None, None) => m * 0.01,
                            };
                            (target / m).min(1.0)
                        }
                    };
                    let k = w[b] as f64 * (g - 1.0);
                    re[b] *= k;
                    im[b] *= k;
                    if b > 0 && b < N / 2 {
                        re[N - b] *= k;
                        im[N - b] *= k;
                    }
                }
                fft_dir(&mut re, &mut im, true);
                (c, re)
            })
            .collect();
        let mut delta = vec![0.0f64; span];
        for (c, re) in parts {
            let at = (c - c0) * HOP;
            for k in 0..N {
                // Fenêtre de synthèse ; la somme des fenêtres au carré vaut 1,5 à ce recouvrement
                delta[at + k] += re[k] * win[k] / 1.5;
            }
        }
        for (k, d) in delta.iter().enumerate() {
            let i = base + k as i64;
            if i >= 0 && (i as usize) < frames {
                data[i as usize * 2 + ch] += *d as f32;
            }
        }
    }
    mask.cols.len()
}

// ---------- Interface ----------

/// Ce que la vue demande à l'éditeur de son
pub enum Action {
    None,
    Close,
    /// Le son retouché (stéréo entrelacé), à écrire dans un nouveau fichier
    Apply(Vec<f32>),
}

pub struct View {
    /// Le son montré (son chemin dans le coffre)
    pub src: String,
    buf: Arc<crate::audio::Buffer>,
    /// Le son en cours de retouche (les réparations déjà faites), et ses états d'avant
    work: Vec<f32>,
    undo: Vec<Vec<f32>>,
    spec: Option<Arc<Spec>>,
    job: Option<std::sync::mpsc::Receiver<Spec>>,
    mask: Mask,
    tex: Option<(egui::TextureHandle, (i64, i64, usize, usize, u32))>,
    mask_tex: Option<egui::TextureHandle>,
    mask_dirty: bool,
    /// Temps au bord gauche (s) et pixels par seconde ; 0 : tout voir
    left: f64,
    pps: f64,
    brush: f32,
    heal: bool,
    db: f32,
    /// Rectangle au lieu du pinceau
    rect_tool: bool,
    drag_from: Option<egui::Pos2>,
    last: Option<(f32, f32)>,
    /// Niveau le plus bas affiché (dB)
    floor: f32,
    version: u32,
}

impl View {
    pub fn new(src: &str, buf: Arc<crate::audio::Buffer>) -> View {
        let mut v = View { src: src.to_string(), work: buf.data.clone(), buf, undo: vec![], spec: None, job: None, mask: Mask::default(), tex: None, mask_tex: None, mask_dirty: false, left: 0.0, pps: 0.0, brush: 14.0, heal: true, db: 120.0, rect_tool: false, drag_from: None, last: None, floor: -100.0, version: 0 };
        v.reanalyse();
        v
    }

    fn reanalyse(&mut self) {
        let data = self.work.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(analyse(&data));
        });
        self.job = Some(rx);
    }

    pub fn dirty(&self) -> bool {
        !self.undo.is_empty()
    }

    fn apply_mask(&mut self) {
        if self.mask.is_empty() {
            return;
        }
        self.undo.push(self.work.clone());
        if self.undo.len() > 8 {
            self.undo.remove(0);
        }
        repair(&mut self.work, &self.mask, if self.heal { Mode::Heal } else { Mode::Attenuate(self.db) });
        self.mask = Mask::default();
        self.mask_dirty = true;
        self.reanalyse();
    }

    /// La vue : barre d'outils, spectrogramme, pinceau. `a..b` : la partie du son que le clip utilise (s).
    pub fn show(&mut self, ui: &mut egui::Ui, a: f64, b: f64) -> Action {
        let p = crate::theme::pal();
        let t = crate::i18n::t;
        let mut action = Action::None;
        if let Some(rx) = &self.job {
            match rx.try_recv() {
                Ok(s) => {
                    self.spec = Some(Arc::new(s));
                    self.job = None;
                    self.version += 1;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.job = None,
                Err(_) => ui.ctx().request_repaint_after(std::time::Duration::from_millis(80)),
            }
        }
        let secs = self.buf.secs().max(0.01);
        // Barre d'outils
        egui::Frame::new().fill(p.panel).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let icon = |ui: &mut egui::Ui, ic: &str, on: bool, tip: &str| ui.add(egui::Button::selectable(on, egui::RichText::new(ic).size(15.0))).on_hover_text(crate::i18n::t(tip));
                if icon(ui, crate::lucide::X, false, "Fermer la vue spectrale (Échap)").clicked() {
                    action = Action::Close;
                }
                ui.label(egui::RichText::new(self.src.rsplit('/').next().unwrap_or("")).strong());
                ui.separator();
                if icon(ui, crate::lucide::BRUSH, !self.rect_tool, "Pinceau : peins la tache à traiter (B)").clicked() {
                    self.rect_tool = false;
                }
                if icon(ui, crate::lucide::SQUARE_DASHED, self.rect_tool, "Rectangle : glisse pour prendre une zone de temps et de fréquences (M)").clicked() {
                    self.rect_tool = true;
                }
                if !self.rect_tool {
                    ui.add(egui::Slider::new(&mut self.brush, 3.0..=80.0).show_value(false)).on_hover_text(t("Taille du pinceau ([ et ])"));
                }
                ui.separator();
                if icon(ui, crate::lucide::BANDAGE, self.heal, "Réparer : la tache est ramenée au niveau du son juste avant et juste après (clic, bruit bref)").clicked() {
                    self.heal = true;
                }
                if icon(ui, crate::lucide::ERASER, !self.heal, "Atténuer : la tache est baissée du nombre de dB choisi (sonnerie, sifflement)").clicked() {
                    self.heal = false;
                }
                if !self.heal {
                    ui.add(egui::DragValue::new(&mut self.db).range(3.0..=120.0).speed(0.5).suffix(" dB")).on_hover_text(t("Baisse appliquée (120 dB : effacé)"));
                }
                ui.separator();
                let has = !self.mask.is_empty();
                if ui.add_enabled(has, egui::Button::new(egui::RichText::new(crate::lucide::WAND_SPARKLES).size(15.0))).on_hover_text(t("Traiter ce qui est peint (Entrée)")).clicked() {
                    self.apply_mask();
                }
                if ui.add_enabled(has, egui::Button::new(egui::RichText::new(crate::lucide::TRASH_2).size(15.0))).on_hover_text(t("Oublier ce qui est peint (Suppr)")).clicked() {
                    self.mask = Mask::default();
                    self.mask_dirty = true;
                }
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new(egui::RichText::new(crate::lucide::UNDO_2).size(15.0))).on_hover_text(t("Annuler le dernier traitement (Ctrl+Z)")).clicked() {
                    self.undo_one();
                }
                ui.separator();
                ui.add(egui::Slider::new(&mut self.floor, -130.0..=-50.0).show_value(false)).on_hover_text(t("Sensibilité : le niveau le plus bas affiché"));
                if self.job.is_some() {
                    ui.spinner();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(self.dirty(), egui::Button::new(format!("{}  {}", crate::lucide::CHECK, t("Garder")))).on_hover_text(t("Écrit le son retouché dans un nouveau fichier et le met dans le clip ; l'original reste intact")).clicked() {
                        action = Action::Apply(self.work.clone());
                    }
                });
            });
        });
        // Raccourcis
        if !ui.ctx().egui_wants_keyboard_input() {
            let (esc, enter, del, b, m, lb, rb, z) = ui.input(|i| (i.key_pressed(egui::Key::Escape), i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace), i.key_pressed(egui::Key::B), i.key_pressed(egui::Key::M), i.key_pressed(egui::Key::OpenBracket), i.key_pressed(egui::Key::CloseBracket), i.modifiers.command && i.key_pressed(egui::Key::Z)));
            if esc {
                action = Action::Close;
            }
            if enter {
                self.apply_mask();
            }
            if del && !self.mask.is_empty() {
                self.mask = Mask::default();
                self.mask_dirty = true;
            }
            if b {
                self.rect_tool = false;
            }
            if m {
                self.rect_tool = true;
            }
            if lb {
                self.brush = (self.brush / 1.25).max(3.0);
            }
            if rb {
                self.brush = (self.brush * 1.25).min(80.0);
            }
            if z {
                self.undo_one();
            }
        }
        // Le spectrogramme
        let avail = ui.available_rect_before_wrap();
        let (rect, resp) = ui.allocate_exact_size(avail.size(), Sense::click_and_drag());
        let pt = ui.painter_at(rect);
        pt.rect_filled(rect, 0.0, Color32::from_rgb(6, 5, 14));
        let axis_w = 44.0;
        let g = Rect::from_min_max(pos2(rect.left() + axis_w, rect.top()), pos2(rect.right(), rect.bottom() - 16.0));
        if g.width() < 20.0 || g.height() < 20.0 {
            return action;
        }
        let fit = g.width() as f64 / secs;
        if self.pps <= 0.0 {
            // Première image : la partie utilisée par le clip, avec un peu de marge
            let (va, vb) = ((a - (b - a) * 0.05).max(0.0), (b + (b - a) * 0.05).min(secs));
            self.pps = (g.width() as f64 / (vb - va).max(0.05)).max(fit);
            self.left = va;
        }
        // Navigation commune des timelines
        if let Some(w) = crate::timenav::wheel(ui, g, Some(&resp)) {
            if w.zoom != 1.0 {
                let at = w.at.map(|q| (q.x - g.left()) as f64).unwrap_or(g.width() as f64 / 2.0);
                crate::timenav::zoom_time(&mut self.left, &mut self.pps, w.zoom as f64, at, fit, 20_000.0);
            }
            self.left += w.time_px as f64 / self.pps;
        }
        let keys = crate::timenav::keys(ui);
        if keys.fit {
            self.pps = fit;
            self.left = 0.0;
        }
        if keys.zoom != 1.0 {
            crate::timenav::zoom_time(&mut self.left, &mut self.pps, keys.zoom as f64, g.width() as f64 / 2.0, fit, 20_000.0);
        }
        self.pps = self.pps.max(fit);
        self.left = self.left.clamp(0.0, (secs - g.width() as f64 / self.pps).max(0.0));
        let col_per_px = RATE as f64 / HOP as f64 / self.pps;
        let (c0, c1) = (self.left * RATE as f64 / HOP as f64, self.left * RATE as f64 / HOP as f64 + g.width() as f64 * col_per_px);
        let Some(spec) = self.spec.clone() else {
            pt.text(g.center(), egui::Align2::CENTER_CENTER, t("analyse du son…"), egui::FontId::proportional(13.0), p.muted);
            return action;
        };
        // L'image n'est refaite que si la vue, la sensibilité ou le son changent
        let (tw, th) = ((g.width() as usize).clamp(16, 2048), (g.height() as usize).clamp(16, 1024));
        let sig = ((c0 * 16.0) as i64, (c1 * 16.0) as i64, tw, th, self.version.wrapping_mul(1000).wrapping_add((self.floor * -1.0) as u32));
        if self.tex.as_ref().is_none_or(|(_, s)| *s != sig) {
            let img = egui::ColorImage::from_rgba_unmultiplied([tw, th], &image(&spec, c0, c1, tw, th, self.floor, -6.0));
            let h = ui.ctx().load_texture("spectrogramme", img, egui::TextureOptions::LINEAR);
            self.tex = Some((h, sig));
            self.mask_dirty = true;
        }
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        if let Some((h, _)) = &self.tex {
            pt.image(h.id(), g, uv, Color32::WHITE);
        }
        // Ce qui est peint, par-dessus
        if self.mask_dirty {
            self.mask_dirty = false;
            self.mask_tex = (!self.mask.is_empty()).then(|| {
                let (mw, mh) = (tw.min(1024), th.min(512));
                let mut px = vec![Color32::TRANSPARENT; mw * mh];
                let rows: Vec<usize> = (0..mh).map(|y| (bin_of(freq_of(1.0 - (y as f32 + 0.5) / mh as f32)) as usize).min(BINS - 1)).collect();
                for x in 0..mw {
                    let ca = c0 + (c1 - c0) * x as f64 / mw as f64;
                    let cb = (c0 + (c1 - c0) * (x + 1) as f64 / mw as f64).max(ca + 1.0);
                    if ca < 0.0 {
                        continue;
                    }
                    for (&c, w) in self.mask.cols.range(ca as usize..cb.ceil() as usize) {
                        let _ = c;
                        for y in 0..mh {
                            let wv = w[rows[y]];
                            if wv > 0.0 {
                                let aa = (wv * 150.0) as u8;
                                let cur = px[y * mw + x];
                                if cur.a() < aa {
                                    px[y * mw + x] = Color32::from_rgba_unmultiplied(120, 235, 255, aa);
                                }
                            }
                        }
                    }
                }
                ui.ctx().load_texture("spectro-masque", egui::ColorImage { size: [mw, mh], source_size: vec2(mw as f32, mh as f32), pixels: px }, egui::TextureOptions::LINEAR)
            });
        }
        if let Some(h) = &self.mask_tex {
            pt.image(h.id(), g, uv, Color32::WHITE);
        }
        // La partie du son que le clip utilise : le reste est assombri
        let x_of = |s: f64| g.left() + ((s - self.left) * self.pps) as f32;
        for (s0, s1) in [(0.0, a), (b, secs)] {
            let (x0, x1) = (x_of(s0).max(g.left()), x_of(s1).min(g.right()));
            if x1 > x0 {
                pt.rect_filled(Rect::from_min_max(pos2(x0, g.top()), pos2(x1, g.bottom())), 0.0, Color32::from_black_alpha(120));
            }
        }
        // Axes : fréquences à gauche, temps en bas
        let apt = ui.painter_at(rect);
        for (f, label) in [(100.0, "100"), (500.0, "500"), (1000.0, "1k"), (2000.0, "2k"), (5000.0, "5k"), (10000.0, "10k"), (20000.0, "20k")] {
            let y = g.bottom() - v_of(f) * g.height();
            apt.text(pos2(g.left() - 6.0, y), egui::Align2::RIGHT_CENTER, label, egui::FontId::monospace(9.5), p.muted);
            apt.hline(g.left() - 3.0..=g.left(), y, Stroke::new(1.0, p.muted));
        }
        let step = [0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0].into_iter().find(|s| s * self.pps >= 70.0).unwrap_or(120.0);
        let mut s = (self.left / step).ceil() * step;
        while x_of(s) < g.right() {
            apt.text(pos2(x_of(s), g.bottom() + 8.0), egui::Align2::CENTER_CENTER, if step < 1.0 { format!("{s:.2}") } else { format!("{s:.0} s") }, egui::FontId::monospace(9.5), p.muted);
            s += step;
        }
        // Pinceau et rectangle
        let to_cv = |q: egui::Pos2| ((c0 + (q.x - g.left()) as f64 * col_per_px) as f32, 1.0 - (q.y - g.top()) / g.height());
        let primary = resp.dragged_by(egui::PointerButton::Primary) || resp.clicked_by(egui::PointerButton::Primary) || resp.drag_started_by(egui::PointerButton::Primary);
        if let Some(q) = resp.interact_pointer_pos().filter(|_| primary) {
            if self.rect_tool {
                if resp.drag_started() {
                    self.drag_from = Some(q);
                }
            } else {
                let cur = to_cv(q);
                let (rc, rv) = ((self.brush as f64 * col_per_px) as f32, self.brush / g.height());
                let from = self.last.filter(|_| !resp.drag_started()).unwrap_or(cur);
                self.mask.stroke(from, cur, rc, rv, spec.cols);
                self.last = Some(cur);
                self.mask_dirty = true;
            }
        }
        if resp.drag_stopped() || !primary {
            self.last = None;
        }
        if let Some(from) = self.drag_from {
            if let Some(q) = ui.input(|i| i.pointer.latest_pos()) {
                let r = Rect::from_two_pos(from, q).intersect(g);
                pt.rect(r, 0.0, Color32::from_rgba_unmultiplied(120, 235, 255, 40), Stroke::new(1.0, Color32::from_rgb(120, 235, 255)), egui::StrokeKind::Inside);
                if resp.drag_stopped() {
                    let (ca, va) = to_cv(r.left_top());
                    let (cb, vb) = to_cv(r.right_bottom());
                    if r.width() > 2.0 && r.height() > 2.0 {
                        self.mask.rect(ca.max(0.0) as usize, (cb.max(0.0) as usize).min(spec.cols.saturating_sub(1)), vb, va);
                        self.mask_dirty = true;
                    }
                    self.drag_from = None;
                }
            } else {
                self.drag_from = None;
            }
        }
        // Le pinceau sous le pointeur, et ce qu'il y a dessous
        if let Some(q) = resp.hover_pos().filter(|q| g.contains(*q)) {
            if !self.rect_tool {
                pt.circle_stroke(q, self.brush, Stroke::new(1.0, Color32::from_white_alpha(200)));
            }
            let (c, v) = to_cv(q);
            let f = freq_of(v);
            let db = spec.at(c.max(0.0) as usize, (bin_of(f) as usize).min(BINS - 1));
            pt.text(pos2(g.right() - 8.0, g.top() + 10.0), egui::Align2::RIGHT_CENTER, format!("{:.2} s · {:.0} Hz · {:.0} dB", c as f64 * HOP as f64 / RATE as f64, f, db), egui::FontId::monospace(10.5), Color32::from_white_alpha(220));
        }
        action
    }

    fn undo_one(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.work = prev;
            self.mask = Mask::default();
            self.mask_dirty = true;
            self.reanalyse();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(f: f32, amp: f32, n: usize) -> Vec<f32> {
        (0..n).flat_map(|i| { let v = amp * (i as f32 * f * std::f32::consts::TAU / RATE as f32).sin(); [v, v] }).collect()
    }

    /// Niveau (dB) autour de la fréquence `f` à la colonne `c`
    fn level(data: &[f32], c: usize, f: f32) -> f32 {
        let s = analyse(data);
        let b = bin_of(f).round() as usize;
        (b.saturating_sub(2)..=b + 2).map(|k| s.at(c, k)).fold(-140.0, f32::max)
    }

    #[test]
    fn spectrogramme_d_un_sinus() {
        let d = sine(1000.0, 0.5, RATE as usize);
        let s = analyse(&d);
        assert_eq!(s.cols, RATE as usize / HOP + 1);
        // Le pic est à 1 kHz, à -6 dB (demi-échelle)
        let c = 40;
        let best = (0..BINS).max_by(|a, b| s.at(c, *a).total_cmp(&s.at(c, *b))).unwrap();
        assert!((best as f32 - bin_of(1000.0)).abs() <= 1.0, "{best}");
        assert!((s.at(c, best) + 6.0).abs() < 1.0, "{}", s.at(c, best));
        assert!(s.at(c, 400) < -90.0);
        // L'échelle de hauteur se relit dans les deux sens
        for f in [50.0, 440.0, 5000.0, 20000.0] {
            assert!((freq_of(v_of(f)) - f).abs() / f < 1e-3);
        }
        let img = image(&s, 0.0, s.cols as f64, 64, 32, -100.0, -6.0);
        assert_eq!(img.len(), 64 * 32 * 4);
        // La ligne de 1 kHz est claire, le haut de l'image est sombre
        let y = ((1.0 - v_of(1000.0)) * 32.0) as usize;
        assert!(img[(y * 64 + 30) * 4] > 200 && img[(2 * 64 + 30) * 4] < 20);
        assert!(image(&Spec { cols: 0, db: vec![] }, 0.0, 1.0, 4, 4, -100.0, 0.0).iter().all(|v| *v == 0));
    }

    #[test]
    fn reparer_un_clic() {
        // Un la, et un clic (bruit bref et fort) à 0,5 s
        let clean = sine(440.0, 0.3, RATE as usize);
        let mut d = clean.clone();
        let at = RATE as usize / 2;
        let mut seed = 7u32;
        for i in at..at + 96 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let v = ((seed >> 8) as f32 / (1 << 24) as f32 - 0.5) * 1.6;
            d[i * 2] += v;
            d[i * 2 + 1] += v;
        }
        let err = |x: &[f32]| x.iter().zip(&clean).map(|(a, b)| ((a - b) * (a - b)) as f64).sum::<f64>();
        let before = err(&d);
        // On peint la tache sur toute la hauteur, autour du clic
        let mut m = Mask::default();
        let c = at as f32 / HOP as f32;
        m.stroke((c, 0.02), (c, 0.98), 4.0, 0.06, cols_of(RATE as usize));
        assert!(!m.is_empty() && m.cols.len() <= 10);
        let n = repair(&mut d, &m, Mode::Heal);
        assert_eq!(n, m.cols.len());
        let after = err(&d);
        assert!(after < before / 30.0, "le clic baisse d'au moins 15 dB : {before} → {after}");
        // Loin du clic, pas un échantillon n'a bougé
        assert!(d[..(at - 4096) * 2] == clean[..(at - 4096) * 2] && d[(at + 4096) * 2..] == clean[(at + 4096) * 2..]);
        // Le la est toujours là, au même niveau, sous la réparation
        assert!((level(&d, c as usize, 440.0) - level(&clean, c as usize, 440.0)).abs() < 1.0);
    }

    #[test]
    fn attenuer_une_sonnerie() {
        // Une voix (220 Hz) et une sonnerie (3 kHz) ensemble : on efface la sonnerie entre 0,3 et 0,7 s
        let n = RATE as usize;
        let mut d: Vec<f32> = sine(220.0, 0.3, n).iter().zip(sine(3000.0, 0.2, n)).map(|(a, b)| a + b).collect();
        let col = |s: f32| (s * RATE as f32 / HOP as f32) as usize;
        let mut m = Mask::default();
        m.rect(col(0.3), col(0.7), v_of(2600.0), v_of(3400.0));
        repair(&mut d, &m, Mode::Attenuate(120.0));
        let mid = col(0.5);
        assert!(level(&d, mid, 3000.0) < -60.0, "{}", level(&d, mid, 3000.0));
        assert!((level(&d, mid, 220.0) + 10.5).abs() < 1.5, "{}", level(&d, mid, 220.0));
        // Hors de la zone, la sonnerie est intacte
        assert!((level(&d, col(0.1), 3000.0) + 14.0).abs() < 1.5);
        // Une baisse de 20 dB seulement
        let mut d2: Vec<f32> = sine(3000.0, 0.2, n);
        repair(&mut d2, &m, Mode::Attenuate(20.0));
        assert!((level(&d2, mid, 3000.0) + 34.0).abs() < 1.5, "{}", level(&d2, mid, 3000.0));
        // Rien de peint : rien ne change
        assert_eq!(repair(&mut d2, &Mask::default(), Mode::Heal), 0);
    }
}
