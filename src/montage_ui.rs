//! Éditeur de montage façon Premiere Pro : panneau Projet, moniteurs Source et Programme,
//! options d'effet, panneau Effets, timeline avec pistes image (V) et son (A), outils et raccourcis.
use crate::ffmpeg;
use crate::montage::*;
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, RichText, Sense, Stroke};
use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::mpsc::{channel, sync_channel, Receiver};

/// Outils de la timeline, avec leur raccourci Premiere.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Select,
    TrackFwd,
    Ripple,
    Roll,
    Stretch,
    Razor,
    Slip,
    Hand,
    Zoom,
    Type,
}

pub const TOOLS: [(Tool, &str, &str); 10] = [
    (Tool::Select, "V", "Sélection : déplacer, raccourcir par les bords"),
    (Tool::TrackFwd, "A", "Sélection de piste en avant : tout ce qui suit (Maj : une seule piste)"),
    (Tool::Ripple, "B", "Modification par propagation : raccourcir sans laisser de trou"),
    (Tool::Roll, "N", "Modification par déroulement : déplacer une coupe entre deux clips"),
    (Tool::Stretch, "R", "Étirement temporel : la longueur change la vitesse"),
    (Tool::Razor, "C", "Cutter : couper à l'endroit cliqué (Maj : toutes les pistes)"),
    (Tool::Slip, "Y", "Déplacement : changer la partie du média utilisée"),
    (Tool::Hand, "H", "Main : faire défiler la timeline"),
    (Tool::Zoom, "Z", "Zoom : clic pour zoomer, Alt+clic pour dézoomer"),
    (Tool::Type, "T", "Texte : poser un titre"),
];

/// Les effets du panneau Effets : identifiant, nom, catégorie.
pub const EFFECTS: [(&str, &str, &str); 7] = [
    ("xfade", "Fondu enchaîné", "Transitions vidéo"),
    ("dip", "Fondu au noir", "Transitions vidéo"),
    ("lumetri", "Couleur Lumetri", "Effets vidéo"),
    ("nodes", "Étalonnage en nodes", "Effets vidéo"),
    ("stab", "Stabilisation", "Effets vidéo"),
    ("track", "Suivi de mouvement", "Effets vidéo"),
    ("afade", "Fondu audio", "Transitions audio"),
];

/// Ce qu'on glisse d'un panneau à la timeline.
#[derive(Clone)]
enum Payload {
    Media(String),
    Source,
    Effect(&'static str),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Act {
    Move { grab: f32 },
    TrimIn,
    TrimOut,
    RippleIn,
    RippleOut,
    Roll { other: u64 },
    Stretch { left: bool },
    Slip,
    Pan { x0: f32, s0: f32 },
    Marquee { from: Pos2 },
    Scrub,
}

#[derive(Clone)]
struct DragState {
    act: Act,
    id: u64,
    /// Les clips au début du geste : chaque image recalcule à partir d'eux
    orig: Vec<Clip>,
    /// Clips qui bougent ensemble (sélection et clips liés)
    group: Vec<u64>,
    snapped: Option<f32>,
}

struct Playback {
    child: Child,
    /// Le son : ffmpeg mixe les pistes, ffplay les joue
    sound: Vec<Child>,
    rx: Receiver<(u32, Vec<u8>)>,
    from: f32,
    size: [usize; 2],
}

/// Le moniteur Source : un média du projet, avant de le poser dans la timeline.
struct Source {
    file: String,
    kind: String,
    media: f32,
    has_audio: bool,
    t: f32,
    mark_in: Option<f32>,
    mark_out: Option<f32>,
    tex: Option<egui::TextureHandle>,
    rx: Option<Receiver<Option<egui::ColorImage>>>,
    shown: Option<i64>,
}

impl Source {
    fn range(&self) -> (f32, f32) {
        let len = if self.media > 0.0 { self.media } else { 5.0 };
        let a = self.mark_in.unwrap_or(0.0).clamp(0.0, len);
        let b = self.mark_out.unwrap_or(len).clamp(a + 0.04, len.max(a + 0.04));
        (a, b)
    }
}

pub struct Editor {
    pub path: PathBuf,
    pub project: Project,
    error: Option<String>,
    playhead: f32,
    pub(crate) sel: Vec<u64>,
    /// Étalonnage copié (luminosité, contraste, saturation, roues, étalonnage en nodes)
    pub(crate) grade_clip: Option<(f32, f32, f32, [[f32; 3]; 3], String)>,
    zoom: f32,
    scroll: f32,
    drag: Option<DragState>,
    tex: Option<egui::TextureHandle>,
    preview_rx: Option<(f32, Receiver<Result<image::RgbaImage, String>>)>,
    /// Scope affiché sur le moniteur (forme d'onde, parade, vectorscope, histogramme) et son image
    pub(crate) scope: Option<usize>,
    pub(crate) scope_tex: Option<egui::TextureHandle>,
    scope_n: u32,
    /// Dernière image du moniteur (pour recalculer un scope qu'on vient d'ouvrir)
    pub(crate) last_frame: Option<(usize, usize, Vec<u8>)>,
    shown: Option<(f32, u64)>,
    rev: u64,
    play: Option<Playback>,
    export_rx: Option<Receiver<Result<PathBuf, String>>>,
    /// File d'export : préréglage et fichier, faits l'un après l'autre ; le dernier préréglage choisi
    pub(crate) export_queue: Vec<(usize, PathBuf)>,
    pub(crate) export_done: usize,
    pub(crate) last_preset: usize,
    message: Option<String>,
    /// Analyse en cours pour « couper les silences » ou « caler sur le rythme »
    smart_rx: Option<Receiver<Result<Smart, String>>>,
    /// Proxies en fabrication : (faits, total) reçus au fil de l'eau
    proxy_rx: Option<(Receiver<(usize, usize, Option<String>)>, usize, usize)>,
    /// Éditeur de courbes (fenêtre) pour les clés du clip choisi
    curves_open: bool,
    curve_ed: crate::curves::Editor,
    /// Médias glissés sur la fenêtre, à ajouter
    pub add: Vec<PathBuf>,
    root: String,
    strips: Strips,
    /// Dernier rendu de composition vu (voir motion::generation)
    motion_gen: u64,
    /// Clé sous le clic droit : (clip, indice de la clé)
    pub(crate) key_menu: Option<(usize, usize)>,
    /// Graphe d'étalonnage à ouvrir dans l'éditeur de nodes
    pub open_request: Option<PathBuf>,
    /// Fichier à ouvrir dans un onglet à lui (la version 9:16)
    pub open_tab: Option<PathBuf>,
    saved: Project,
    history: Vec<Project>,
    redo: Vec<Project>,
    last_head: f32,
    mute: bool,
    no_ffplay: bool,
    sound_install: Option<std::sync::Arc<std::sync::Mutex<ffmpeg::InstallState>>>,
    tool: Tool,
    snap: bool,
    linked_sel: bool,
    /// Onglet du haut à gauche : 0 options d'effet, 1 source, 2 effets
    top_tab: u8,
    source: Option<Source>,
    /// Pistes ciblées pour insérer et recouvrir depuis la source
    vtarget: usize,
    atarget: usize,
    clipboard: Vec<Clip>,
    bin_search: String,
    bin_grid: bool,
    fx_search: String,
    /// Déplacement d'un clip dans le moniteur Programme : clip, souris et position de départ
    monitor_drag: Option<(u64, Pos2, f32, f32)>,
    /// Suivi de mouvement : le clip qui suivra le point cliqué dans le moniteur
    track_pick: Option<u64>,
    /// Poignée de coin tirée dans le moniteur : (clip, distance au centre au départ, taille au départ)
    monitor_scale: Option<(u64, f32, f32)>,
    /// Panneau de gauche : 0 médias, 1 source
    left_tab: u8,
    /// Partie choisie sur la carte d'un média (début, fin en secondes de la source)
    card_trim: std::collections::HashMap<String, (f32, f32)>,
    /// Dernière carte survolée ou cliquée : E l'ajoute à la fin, Maj+E à la tête de lecture
    card_focus: Option<String>,
    /// Filtre des médias : 0 tout, 1 pas encore utilisés, 2 vidéos, 3 images, 4 sons
    bin_filter: u8,
    folded: HashSet<&'static str>,
    shuttle: f32,
    /// Ajuster le zoom pour voir tout le montage (\\)
    fit_zoom: bool,
}

/// Résultat d'une analyse du son, faite en arrière-plan
enum Smart {
    /// Le son décodé du clip (son identifiant)
    Silences(u64, crate::audio::Buffer),
    /// Temps forts de la musique, en temps de la timeline
    Beats(Vec<f32>),
    /// Répliques transcrites, en temps de la timeline
    Subtitles(Vec<crate::transcribe::Cue>),
    /// Suivi de mouvement : les clés de placement du clip (son identifiant)
    Track(u64, Vec<crate::montage::PoseKey>),
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn seconds(t: f32, fps: f32) -> f32 {
    (t * fps).round() / fps.max(1.0)
}

impl Editor {
    pub fn open(path: PathBuf, root: &str) -> Editor {
        let (project, error) = match Project::read(&path) {
            Ok(p) => (p, None),
            Err(e) => (Project::default(), Some(e)),
        };
        let mut project = project;
        if error.is_none() && project.settle_all() {
            let _ = project.write(&path);
        }
        Editor {
            path,
            error,
            playhead: 0.0,
            sel: vec![],
            grade_clip: None,
            zoom: 60.0,
            scroll: 0.0,
            drag: None,
            tex: None,
            preview_rx: None,
            scope: None,
            scope_tex: None,
            scope_n: 0,
            last_frame: None,
            shown: None,
            rev: 0,
            play: None,
            export_rx: None,
            export_queue: vec![],
            export_done: 0,
            last_preset: 0,
            message: None,
            smart_rx: None,
            proxy_rx: None,
            curves_open: false,
            curve_ed: Default::default(),
            add: vec![],
            root: root.to_string(),
            strips: Strips::default(),
            motion_gen: crate::motion::generation(),
            key_menu: None,
            open_request: None,
            open_tab: None,
            saved: project.clone(),
            history: vec![],
            redo: vec![],
            last_head: 0.0,
            mute: false,
            no_ffplay: false,
            sound_install: None,
            tool: Tool::Select,
            snap: true,
            linked_sel: true,
            top_tab: 0,
            source: None,
            vtarget: 0,
            atarget: 0,
            clipboard: vec![],
            bin_search: String::new(),
            bin_grid: true,
            fx_search: String::new(),
            monitor_drag: None,
            track_pick: None,
            monitor_scale: None,
            left_tab: 0,
            card_trim: Default::default(),
            card_focus: None,
            bin_filter: 0,
            folded: HashSet::new(),
            shuttle: 0.0,
            fit_zoom: true,
            project,
        }
    }

    fn changed(&mut self) {
        self.project.ensure_ids();
        // Timeline magnétique : l'histoire se resserre, ce qui y est accroché suit
        if self.project.magnetic {
            let before = self.saved.clone();
            self.project.magnetize(&before);
        }
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

    fn undo(&mut self, forward: bool) {
        let (from, to) = if forward { (&mut self.redo, &mut self.history) } else { (&mut self.history, &mut self.redo) };
        if let Some(p) = from.pop() {
            to.push(std::mem::replace(&mut self.project, p));
            self.saved = self.project.clone();
            self.sel.retain(|id| self.project.index(*id).is_some());
            self.rev += 1;
            let _ = self.project.write(&self.path);
        }
    }

    fn stop(&mut self) {
        if let Some(mut p) = self.play.take() {
            let _ = p.child.kill();
            let _ = p.child.wait();
            for mut c in p.sound {
                let _ = c.kill();
                let _ = c.wait();
            }
        }
        self.shuttle = 0.0;
    }

    /// Lecture : un seul ffmpeg calcule le montage à partir de la tête de lecture et envoie ses images au fil de l'eau.
    fn start(&mut self, ctx: &egui::Context) {
        self.stop();
        let Some(ff) = ffmpeg::find("ffmpeg") else { return };
        let end = self.project.mark_out.filter(|o| *o > self.playhead + 0.05).unwrap_or(self.project.duration());
        if self.playhead >= end - 0.05 {
            self.playhead = self.project.mark_in.unwrap_or(0.0);
        }
        let w = 640u32.min(self.project.width.max(2)) / 2 * 2;
        let h = ((w as f32 * self.project.height as f32 / self.project.width.max(1) as f32).round() as u32).max(2) / 2 * 2;
        let (inputs, filter, _, _) = graph(&self.project, &self.path, self.playhead, w, h, true, false);
        let child = ffmpeg::command(ff.clone()).args(["-v", "error"]).args(inputs).args(["-filter_complex", &filter, "-map", "[vout]", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
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
        let sound = self.start_sound(&ff);
        self.play = Some(Playback { child, sound, rx, from: self.playhead, size: [w as usize, h as usize] });
        self.shuttle = 1.0;
    }

    /// Le son du montage à partir de la tête de lecture, mixé par ffmpeg et joué par ffplay.
    fn start_sound(&mut self, ff: &Path) -> Vec<Child> {
        if self.mute {
            return vec![];
        }
        let (inputs, filter, has, _) = graph(&self.project, &self.path, self.playhead, 0, 0, false, true);
        if !has {
            return vec![];
        }
        let Some(fp) = ffmpeg::find("ffplay") else {
            self.no_ffplay = true;
            return vec![];
        };
        let Ok(mut mix) = ffmpeg::command(ff.to_path_buf()).args(["-v", "error"]).args(inputs).args(["-filter_complex", &filter, "-map", "[aout]", "-ar", "48000", "-ac", "2", "-f", "wav", "-"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else { return vec![] };
        let Some(out) = mix.stdout.take() else { return vec![mix] };
        match ffmpeg::command(fp).args(["-nodisp", "-autoexit", "-loglevel", "quiet", "-fflags", "nobuffer", "-i", "-"]).stdin(Stdio::from(out)).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
            Ok(play) => vec![mix, play],
            Err(_) => vec![mix],
        }
    }

    // ------------------------------------------------------------------ opérations de montage

    fn selected(&self) -> Vec<usize> {
        self.sel.iter().filter_map(|id| self.project.index(*id)).collect()
    }

    fn first_selected(&self) -> Option<usize> {
        self.sel.first().and_then(|id| self.project.index(*id))
    }

    fn locked(&self, c: &Clip) -> bool {
        self.project.state(c.audio, c.track).lock
    }

    /// La sélection plus les clips liés (sauf avec Alt ou sélection liée coupée).
    fn with_links(&self, idx: &[usize], alt: bool) -> Vec<usize> {
        let mut out: Vec<usize> = idx.to_vec();
        if self.linked_sel && !alt {
            for &i in idx {
                out.extend(self.project.linked(i));
            }
        }
        out.sort();
        out.dedup();
        out.retain(|&i| !self.locked(&self.project.clips[i]));
        out
    }

    /// Coupe les clips `idx` à l'instant `t` ; les moitiés droites de clips liés restent liées entre elles.
    fn split_at(&mut self, t: f32, idx: &[usize]) -> bool {
        let ids: Vec<u64> = idx.iter().map(|&i| self.project.clips[i].id).collect();
        let mut new_links: std::collections::HashMap<u32, u32> = Default::default();
        let mut any = false;
        for id in ids {
            let Some(i) = self.project.index(id) else { continue };
            if let Some(k) = self.project.split(i, t) {
                any = true;
                let l = self.project.clips[k].link;
                if l != 0 {
                    let next = self.project.next_link();
                    let nl = *new_links.entry(l).or_insert(next);
                    self.project.clips[k].link = nl;
                }
            }
        }
        any
    }

    /// Libère l'intervalle [a, b] d'une piste : ce qui s'y trouve est raccourci, coupé ou retiré (recouvrir).
    fn clear_range(&mut self, audio: bool, track: usize, a: f32, b: f32, keep: &HashSet<u64>) {
        let eps = 1e-3;
        // Un clip qui déborde des deux côtés : coupé d'abord en b
        let wide: Vec<usize> = (0..self.project.clips.len()).filter(|&i| {
            let c = &self.project.clips[i];
            c.audio == audio && c.track == track && !keep.contains(&c.id) && c.start < a - eps && c.end() > b + eps
        }).collect();
        for i in wide {
            self.project.split(i, b);
        }
        let mut remove = HashSet::new();
        for c in self.project.clips.iter_mut() {
            if c.audio != audio || c.track != track || keep.contains(&c.id) || c.end() <= a + eps || c.start >= b - eps {
                continue;
            }
            if c.start >= a - eps && c.end() <= b + eps {
                remove.insert(c.id);
            } else if c.start < a {
                c.src_out = c.src_in + (a - c.start) * c.speed;
                c.fade_out = c.fade_out.min(c.len());
            } else {
                let d = b - c.start;
                c.src_in += d * c.speed;
                c.start = b;
                c.xfade = 0.0;
            }
        }
        self.project.clips.retain(|c| !remove.contains(&c.id));
    }

    /// Décale de `d` secondes tout ce qui commence après `from` (sur les pistes données, ou toutes les pistes non verrouillées).
    fn shift_after(&mut self, from: f32, d: f32, tracks: Option<&HashSet<(bool, usize)>>, except: &HashSet<u64>) {
        let states = (self.project.vstate.clone(), self.project.astate.clone());
        for c in self.project.clips.iter_mut() {
            let lock = (if c.audio { &states.1 } else { &states.0 }).get(c.track).is_some_and(|s| s.lock);
            if lock || except.contains(&c.id) || c.start < from - 1e-3 {
                continue;
            }
            if tracks.is_some_and(|t| !t.contains(&(c.audio, c.track))) {
                continue;
            }
            c.start = (c.start + d).max(0.0);
        }
        self.project.markers.iter_mut().filter(|m| **m >= from && tracks.is_none()).for_each(|m| *m = (*m + d).max(0.0));
    }

    /// Supprimer (laisser un trou) ou supprimer avec propagation (refermer le trou).
    fn delete(&mut self, ripple: bool, alt: bool) {
        let idx = self.with_links(&self.selected(), alt);
        if idx.is_empty() {
            return;
        }
        let gone: Vec<Clip> = idx.iter().map(|&i| self.project.clips[i].clone()).collect();
        let ids: HashSet<u64> = gone.iter().map(|c| c.id).collect();
        self.project.clips.retain(|c| !ids.contains(&c.id));
        if ripple {
            // Le trou se referme : du plus tard au plus tôt, piste par piste
            let mut g = gone.clone();
            g.sort_by(|a, b| b.start.total_cmp(&a.start));
            let tracks: HashSet<(bool, usize)> = g.iter().map(|c| (c.audio, c.track)).collect();
            let (a, b) = (g.iter().map(|c| c.start).fold(f32::MAX, f32::min), g.iter().map(|c| c.end()).fold(0.0, f32::max));
            // Fermer seulement si rien d'autre n'occupe le trou sur ces pistes
            let busy = self.project.clips.iter().any(|c| tracks.contains(&(c.audio, c.track)) && c.end() > a + 1e-3 && c.start < b - 1e-3);
            if !busy {
                self.shift_after(b, a - b, Some(&tracks), &HashSet::new());
            }
        }
        self.sel.clear();
        self.changed();
    }

    /// Ctrl+K : coupe à la tête de lecture la sélection, ou tout ce qui passe sous elle.
    fn add_edit(&mut self) {
        let t = self.playhead;
        let sel = self.selected();
        let idx: Vec<usize> = if sel.is_empty() { (0..self.project.clips.len()).filter(|&i| !self.locked(&self.project.clips[i])).collect() } else { self.with_links(&sel, false) };
        let idx: Vec<usize> = idx.into_iter().filter(|&i| self.project.clips[i].start < t && self.project.clips[i].end() > t).collect();
        if self.split_at(t, &idx) {
            self.changed();
        }
    }

    /// Q et W : retire ce qui va du début du clip (Q) ou de sa fin (W) à la tête de lecture, et referme.
    fn ripple_trim(&mut self, previous: bool) {
        let t = self.playhead;
        let crossing: Vec<Clip> = self.project.clips.iter().filter(|c| c.start < t - 1e-3 && c.end() > t + 1e-3 && !self.project.state(c.audio, c.track).lock).cloned().collect();
        if crossing.is_empty() {
            return;
        }
        let tracks: HashSet<(bool, usize)> = crossing.iter().map(|c| (c.audio, c.track)).collect();
        if previous {
            let d = crossing.iter().map(|c| t - c.start).fold(0.0, f32::max);
            for c in &crossing {
                if let Some(i) = self.project.index(c.id) {
                    let cut = t - c.start;
                    self.project.clips[i].src_in += cut * c.speed;
                }
            }
            let ids: HashSet<u64> = HashSet::new();
            let from = crossing.iter().map(|c| c.start).fold(f32::MAX, f32::min);
            for c in self.project.clips.iter_mut() {
                if tracks.contains(&(c.audio, c.track)) && c.start >= from - 1e-3 && !ids.contains(&c.id) {
                    c.start = if crossing.iter().any(|x| x.id == c.id) { c.start } else { c.start - d };
                }
            }
            self.playhead = from;
        } else {
            let d = crossing.iter().map(|c| c.end() - t).fold(0.0, f32::max);
            let ends: Vec<(u64, f32)> = crossing.iter().map(|c| (c.id, c.end())).collect();
            for c in &crossing {
                if let Some(i) = self.project.index(c.id) {
                    self.project.clips[i].src_out = c.src_in + (t - c.start) * c.speed;
                }
            }
            let from = ends.iter().map(|e| e.1).fold(f32::MAX, f32::min);
            self.shift_after(from, -d, Some(&tracks), &crossing.iter().map(|c| c.id).collect());
        }
        self.changed();
    }

    /// Pose des clips à l'instant `at` (le premier clip donne l'origine) : en recouvrant ou en insérant.
    fn place(&mut self, mut clips: Vec<Clip>, at: f32, insert: bool) -> Vec<u64> {
        if clips.is_empty() {
            return vec![];
        }
        let t0 = clips.iter().map(|c| c.start).fold(f32::MAX, f32::min);
        let len = clips.iter().map(|c| c.end()).fold(0.0, f32::max) - t0;
        // Liens recréés : les copies d'une même paire restent liées entre elles, pas aux originaux
        let mut relink: std::collections::HashMap<u32, u32> = Default::default();
        let mut next = self.project.next_link();
        for c in clips.iter_mut() {
            c.start = c.start - t0 + at;
            c.id = 0;
            if c.link != 0 {
                c.link = *relink.entry(c.link).or_insert_with(|| {
                    next += 1;
                    next - 1
                });
            }
            if c.audio {
                self.project.atracks = self.project.atracks.max(c.track + 1);
            } else {
                self.project.tracks = self.project.tracks.max(c.track + 1);
            }
        }
        if insert {
            // Insérer : tout ce qui est après la tête de lecture recule de la longueur posée, sur toutes les pistes
            let crossing: Vec<usize> = (0..self.project.clips.len()).filter(|&i| {
                let c = &self.project.clips[i];
                c.start < at - 1e-3 && c.end() > at + 1e-3 && !self.locked(c)
            }).collect();
            self.split_at(at, &crossing);
            self.shift_after(at, len, None, &HashSet::new());
        } else {
            for c in &clips {
                self.clear_range(c.audio, c.track, c.start, c.end(), &HashSet::new());
            }
        }
        let n0 = self.project.clips.len();
        self.project.clips.extend(clips);
        self.project.ensure_ids();
        self.project.clips[n0..].iter().map(|c| c.id).collect()
    }

    /// Le clip du moniteur Source, prêt à poser sur les pistes ciblées.
    fn source_clips(&self) -> Option<Vec<Clip>> {
        let s = self.source.as_ref()?;
        let media = resolve(&self.path, &s.file);
        let mut c = import(&self.path, &media, self.vtarget, 0.0).ok()?;
        let (a, b) = s.range();
        if c.kind == "image" {
            (c.src_in, c.src_out) = (0.0, b - a);
        } else {
            (c.src_in, c.src_out) = (a, b);
        }
        c.file = s.file.clone();
        let mut p = Project::default();
        p.add_media(c, self.atarget);
        Some(p.clips)
    }

    fn source_edit(&mut self, insert: bool) {
        let Some(clips) = self.source_clips() else {
            self.message = Some(crate::i18n::t("Ouvre d'abord un média dans le moniteur Source (double-clic dans le panneau Projet)."));
            return;
        };
        let len = clips.iter().map(|c| c.end()).fold(0.0, f32::max);
        let at = self.playhead;
        self.sel = self.place(clips, at, insert);
        self.playhead = at + len;
        self.changed();
    }

    /// Importe un média (chemin du disque) en clips, sans les poser.
    fn media_clips(&mut self, path: &Path, vtrack: usize, atrack: usize) -> Option<Vec<Clip>> {
        let c = match import(&self.path, path, vtrack, 0.0) {
            Ok(c) => c,
            Err(e) => {
                self.message = Some(format!("{} : {e}", path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
                return None;
            }
        };
        if !self.project.bin.contains(&c.file) {
            self.project.bin.push(c.file.clone());
        }
        let mut p = Project::default();
        p.add_media(c, atrack);
        Some(p.clips)
    }

    /// Un chemin de média tel qu'enregistré dans le projet (relatif au montage quand c'est possible) → chemin du disque.
    fn media_path(&self, file: &str) -> PathBuf {
        resolve(&self.path, file)
    }

    /// Applique un effet du panneau Effets au clip `i` ; `at` dit de quel côté (entrée ou sortie) pour une transition.
    fn apply_effect(&mut self, i: usize, fx: &str, at: f32) {
        let c = self.project.clips[i].clone();
        let near_start = (at - c.start).abs() <= (c.end() - at).abs();
        match fx {
            "xfade" | "afade" | "dip" => {
                let d = if fx == "dip" { 0.5 } else { 1.0 };
                if near_start {
                    let prev = self.project.clips.iter().any(|o| o.id != c.id && o.audio == c.audio && o.track == c.track && (o.end() - c.start).abs() < 0.1);
                    if fx != "dip" && prev && self.project.crossfade(i, d) {
                    } else {
                        self.project.clips[i].fade_in = d.min(c.len() * 0.5);
                    }
                } else {
                    let next = self.project.clips.iter().position(|o| o.id != c.id && o.audio == c.audio && o.track == c.track && (o.start - c.end()).abs() < 0.1);
                    match next {
                        Some(k) if fx != "dip" => {
                            self.project.crossfade(k, d);
                        }
                        _ => self.project.clips[i].fade_out = d.min(c.len() * 0.5),
                    }
                }
                // L'image et son son se fondent ensemble
                for k in self.project.linked(i) {
                    if k != i {
                        let (fi, fo) = (self.project.clips[i].fade_in, self.project.clips[i].fade_out);
                        self.project.clips[k].fade_in = fi;
                        self.project.clips[k].fade_out = fo;
                    }
                }
            }
            "lumetri" => {
                if !c.fx.iter().any(|f| f == "lumetri") {
                    self.project.clips[i].fx.push("lumetri".into());
                }
            }
            "nodes" => {
                self.new_grade(i);
            }
            "stab" => {
                if self.project.clips[i].kind == "video" && self.project.clips[i].stab == 0.0 {
                    self.project.clips[i].stab = 6.0;
                }
            }
            "track" => {
                self.track_pick = Some(self.project.clips[i].id);
                self.message = Some(crate::i18n::t("Clique dans le moniteur le point de la vidéo dessous que ce clip doit suivre."));
            }
            _ => {}
        }
        self.sel = vec![self.project.clips[i].id];
        self.top_tab = 0;
        self.changed();
    }

    /// Choisit une LUT .cube pour le clip `s` ; prise hors du dossier du montage, elle est copiée dans « luts/ »
    pub(crate) fn pick_lut(&mut self, s: usize) -> bool {
        let Some(f) = rfd::FileDialog::new().add_filter("LUT", &["cube"]).pick_file() else { return false };
        let dir = self.path.parent().unwrap_or(Path::new("")).to_path_buf();
        let rel = match f.strip_prefix(&dir) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => {
                let name = f.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "look.cube".into());
                let _ = std::fs::create_dir_all(dir.join("luts"));
                if let Err(e) = std::fs::copy(&f, dir.join("luts").join(&name)) {
                    self.message = Some(e.to_string());
                    return false;
                }
                format!("luts/{name}")
            }
        };
        self.project.clips[s].grade = rel;
        true
    }

    fn new_grade(&mut self, s: usize) {
        if !self.project.clips[s].grade.is_empty() {
            self.open_request = Some(resolve(&self.path, &self.project.clips[s].grade));
            return;
        }
        let stem = self.path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "montage".into());
        let dir = self.path.parent().unwrap_or(Path::new("")).to_path_buf();
        let mut file = dir.join(format!("{stem} - {}.{}", self.project.clips[s].name, crate::formats::ext("comp")));
        let mut n = 2;
        while file.exists() {
            file = dir.join(format!("{stem} - {} {n}.{}", self.project.clips[s].name, crate::formats::ext("comp")));
            n += 1;
        }
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
        }
    }

    /// Les instants où une coupe existe (début et fin des clips), triés.
    fn edits(&self) -> Vec<f32> {
        let mut v: Vec<f32> = self.project.clips.iter().flat_map(|c| [c.start, c.end()]).collect();
        v.push(0.0);
        v.sort_by(|a, b| a.total_cmp(b));
        v.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
        v
    }

    fn seek(&mut self, t: f32) {
        if self.play.is_some() {
            self.stop();
        }
        self.playhead = t.max(0.0);
    }

    fn export_to(&mut self, ctx: &egui::Context) {
        let preset = self.last_preset;
        self.queue_export(ctx, preset);
    }

    /// Demande le fichier, puis ajoute l'export à la file (il part tout de suite si rien ne tourne)
    pub(crate) fn queue_export(&mut self, ctx: &egui::Context, preset: usize) {
        let (_, ext, _) = crate::montage::PRESETS[preset.min(crate::montage::PRESETS.len() - 1)];
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "montage".into());
        let mut d = rfd::FileDialog::new().add_filter(ext.to_uppercase(), &[ext]).set_file_name(format!("{stem}.{ext}"));
        if let Some(dir) = self.path.parent() {
            d = d.set_directory(dir);
        }
        if let Some(mut out) = d.save_file() {
            if out.extension().is_none() {
                out.set_extension(ext);
            }
            self.last_preset = preset;
            self.export_queue.push((preset, out));
            self.next_export(ctx);
        }
    }

    fn next_export(&mut self, ctx: &egui::Context) {
        if self.export_rx.is_some() || self.export_queue.is_empty() {
            return;
        }
        let (preset, out) = self.export_queue.remove(0);
        let (tx, rx) = channel();
        let (proj, path, c2) = (self.project.clone(), self.path.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(crate::montage::export_as(&proj, &path, &out, preset).map(|_| out));
            c2.request_repaint();
        });
        self.export_rx = Some(rx);
    }

    /// Enregistre l'image affichée dans le moniteur Programme, en PNG à côté du montage.
    fn export_frame(&mut self) {
        let (proj, path, t) = (self.project.clone(), self.path.clone(), self.playhead);
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "montage".into());
        let out = path.with_file_name(format!("{stem} {}.png", timecode(t, proj.fps).replace(':', "-")));
        match frame(&proj, &path, t, proj.width.max(16)) {
            Ok(img) => {
                let _ = img.save(&out);
                self.message = Some(format!("{} : {}", crate::i18n::t("image exportée"), out.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
            }
            Err(e) => self.message = Some(e),
        }
    }

    // ------------------------------------------------------------------ interface

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let ctx = ui.ctx().clone();
        // Médias déposés sur la fenêtre : dans le panneau Projet, et posés à la suite sur les pistes ciblées
        for f in std::mem::take(&mut self.add) {
            let (vt, at) = (self.vtarget, self.atarget);
            if let Some(clips) = self.media_clips(&f, vt, at) {
                let end = self.project.clips.iter().filter(|c| (!c.audio && c.track == vt) || (c.audio && c.track == at)).map(|c| c.end()).fold(0.0, f32::max);
                self.sel = self.place(clips, end, false);
                self.changed();
            }
        }
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
        self.poll_playback(&ctx);
        self.poll_preview(&ctx);
        if let Some(rx) = &self.export_rx {
            if let Ok(r) = rx.try_recv() {
                self.message = Some(match r {
                    Ok(f) => format!("{} : {}", crate::i18n::t("exporté"), f.display()),
                    Err(e) => e,
                });
                self.export_rx = None;
                self.export_done += 1;
                self.next_export(&ctx);
            }
        }
        if self.export_rx.is_none() && self.export_queue.is_empty() {
            self.export_done = 0;
        }
        self.keyboard(ui);

        let panel_frame = egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line));
        let full_h = ui.available_height();
        // Chaque panneau peut avoir sa propre case dans la disposition (menu « fenêtre »)
        let bin_r = crate::layout::offer("montage-project", "projet (montage)");
        let tl_r = crate::layout::offer("montage-timeline", "timeline (montage)");
        let fx_r = crate::layout::offer("montage-fx", "options d'effet, source, effets");
        let prog_r = crate::layout::offer("montage-program", "moniteur programme");
        let timeline_block = |this: &mut Editor, ui: &mut egui::Ui| {
            // Les outils sont dans la barre de la timeline
            egui::CentralPanel::default().frame(egui::Frame::new().fill(p.panel)).show(ui, |ui| this.timeline_panel(ui));
        };
        let top_left = |this: &mut Editor, ui: &mut egui::Ui| {
            let tabs = ["options d'effet", "source", "effets"];
            tab_row(ui, &mut this.top_tab, &tabs);
            ui.add_space(4.0);
            match this.top_tab {
                0 => this.effect_controls(ui),
                1 => this.source_panel(ui),
                _ => this.effects_panel(ui),
            }
        };
        // Disposition de la maquette (quand aucun panneau n'a sa propre case) : médias à gauche,
        // inspecteur à droite, moniteurs source et programme côte à côte, timeline dessous
        if bin_r.is_none() && tl_r.is_none() && fx_r.is_none() && prog_r.is_none() {
            // Médias à gauche, avec la source en second onglet (on y choisit l'entrée et la sortie d'un média)
            egui::Panel::left("montage-media").resizable(true).default_size(280.0).min_size(200.0).max_size(480.0).frame(panel_frame).show(ui, |ui| {
                let left = if self.source.is_some() && self.left_tab == 1 { 1 } else { 0 };
                let k = tab_chips(ui, left, &[&crate::i18n::t("Médias"), &crate::i18n::t("Source")]);
                if let Some(k) = k {
                    self.left_tab = k;
                }
                egui::Frame::new().inner_margin(8).show(ui, |ui| if left == 1 { self.source_panel(ui) } else { self.bin_panel(ui) });
            });
            egui::Panel::right("montage-inspector").resizable(true).default_size(290.0).min_size(240.0).max_size(460.0).frame(panel_frame).show(ui, |ui| {
                // Clip (cartes), Effets (bibliothèque), Avancé (toutes les options d'effet)
                let cur = match self.top_tab { 2 => 1, 3 => 2, _ => 0 };
                if let Some(k) = tab_chips(ui, cur, &[&crate::i18n::t("Clip"), &crate::i18n::t("Effets"), &crate::i18n::t("Avancé")]) {
                    self.top_tab = [0, 2, 3][k as usize];
                }
                egui::ScrollArea::vertical().id_salt("montage-insp").auto_shrink([false, false]).show(ui, |ui| {
                    egui::Frame::new().inner_margin(12).show(ui, |ui| match cur {
                        1 => self.effects_panel(ui),
                        2 => self.effect_controls(ui),
                        _ => self.clip_cards(ui),
                    });
                });
            });
            egui::Panel::bottom("montage-lower").resizable(true).default_size((full_h * 0.4).max(240.0)).min_size(180.0).max_size((full_h * 0.75).max(200.0)).frame(egui::Frame::new()).show(ui, |ui| timeline_block(self, ui));
            // Un seul moniteur : le montage
            egui::CentralPanel::default().frame(egui::Frame::new().fill(p.code_bg).inner_margin(egui::Margin::symmetric(16, 10))).show(ui, |ui| self.program_monitor(ui));
            return;
            #[allow(unreachable_code)]
            egui::CentralPanel::default().frame(egui::Frame::new().fill(p.code_bg).inner_margin(egui::Margin::symmetric(16, 12))).show(ui, |ui| {
                // Deux moniteurs de même largeur, chacun dans sa case (rien ne déborde sur le voisin)
                let full = ui.available_rect_before_wrap();
                let w = (full.width() - 16.0) / 2.0;
                for (k, left) in [(0, full.left()), (1, full.left() + w + 16.0)] {
                    let r = Rect::from_min_size(pos2(left, full.top()), egui::vec2(w, full.height()));
                    let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::top_down(egui::Align::Min)));
                    cui.set_clip_rect(r.expand(1.0));
                    if k == 0 {
                        self.source_panel(&mut cui);
                    } else {
                        self.program_monitor(&mut cui);
                    }
                }
                ui.allocate_rect(full, Sense::hover());
            });
            return;
        }
        if bin_r.is_none() || tl_r.is_none() {
            egui::Panel::bottom("montage-lower").resizable(true).default_size((full_h * 0.46).max(220.0)).min_size(180.0).max_size((full_h * 0.75).max(200.0)).frame(egui::Frame::new()).show(ui, |ui| {
                if bin_r.is_none() {
                    let pf = panel_frame.inner_margin(6);
                    if tl_r.is_none() {
                        egui::Panel::left("montage-bin").resizable(true).default_size(260.0).min_size(180.0).max_size(520.0).frame(pf).show(ui, |ui| self.bin_panel(ui));
                    } else {
                        egui::CentralPanel::default().frame(pf).show(ui, |ui| self.bin_panel(ui));
                    }
                }
                if tl_r.is_none() {
                    timeline_block(self, ui);
                }
            });
        }
        if let Some(r) = bin_r {
            crate::layout::draw_in(ui, r, p.panel, 6.0, |ui| self.bin_panel(ui));
        }
        if let Some(r) = tl_r {
            crate::layout::draw_in(ui, r, p.bg, 0.0, |ui| timeline_block(self, ui));
        }
        match fx_r {
            Some(r) => crate::layout::draw_in(ui, r, p.panel, 8.0, |ui| top_left(self, ui)),
            None if prog_r.is_some() => {
                egui::CentralPanel::default().frame(panel_frame.inner_margin(8)).show(ui, |ui| top_left(self, ui));
            }
            None => {
                egui::Panel::left("montage-top-left").resizable(true).default_size((ui.available_width() * 0.4).max(300.0)).min_size(260.0).max_size((ui.available_width() * 0.7).max(300.0)).frame(panel_frame.inner_margin(8)).show(ui, |ui| top_left(self, ui));
            }
        }
        match prog_r {
            Some(r) => crate::layout::draw_in(ui, r, p.panel, 8.0, |ui| self.program_monitor(ui)),
            None => {
                egui::CentralPanel::default().frame(egui::Frame::new().fill(p.panel).inner_margin(8)).show(ui, |ui| self.program_monitor(ui));
            }
        }
        if fx_r.is_some() && prog_r.is_some() {
            egui::CentralPanel::default().frame(egui::Frame::new().fill(p.panel)).show(ui, |ui| {
                ui.centered_and_justified(|ui| ui.label(RichText::new(crate::i18n::t("Les panneaux du montage sont dans leurs cases.")).color(p.muted)));
            });
        }
    }

    fn poll_playback(&mut self, ctx: &egui::Context) {
        let mut ended = false;
        if let Some(pb) = &mut self.play {
            let mut latest = None;
            while let Ok(f) = pb.rx.try_recv() {
                latest = Some(f);
            }
            let got = latest.is_some();
            if let Some((n, buf)) = latest {
                self.playhead = pb.from + n as f32 / self.project.fps.max(1.0);
                // Le scope suit la lecture, une image sur quatre
                self.scope_n = self.scope_n.wrapping_add(1);
                if self.scope.is_some() && self.scope_n % 4 == 0 {
                    let rgba: Vec<u8> = buf.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect();
                    let (w, h) = (pb.size[0], pb.size[1]);
                    if let Some(kind) = self.scope {
                        let (sw, sh, out) = crate::scopes::render(kind, w, h, &rgba);
                        let img = egui::ColorImage::from_rgba_unmultiplied([sw, sh], &out);
                        match &mut self.scope_tex {
                            Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                            None => self.scope_tex = Some(ctx.load_texture("montage-scope", img, egui::TextureOptions::LINEAR)),
                        }
                    }
                }
                let img = egui::ColorImage::from_rgb(pb.size, &buf);
                match &mut self.tex {
                    Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                    None => self.tex = Some(ctx.load_texture("montage", img, egui::TextureOptions::LINEAR)),
                }
                self.shown = None;
            }
            ended = matches!(pb.child.try_wait(), Ok(Some(_))) && !got;
            // Point de sortie : la lecture s'arrête là
            if self.project.mark_out.is_some_and(|o| o > pb.from + 0.05 && self.playhead >= o) {
                ended = true;
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
        if ended {
            self.stop();
        }
    }

    /// Recalcule l'image du scope choisi à partir d'une image RVBA
    pub(crate) fn update_scope(&mut self, ctx: &egui::Context, w: usize, h: usize, px: &[u8]) {
        let Some(kind) = self.scope else { return };
        if w == 0 || h == 0 || px.len() < w * h * 4 {
            return;
        }
        let (sw, sh, out) = crate::scopes::render(kind, w, h, px);
        let img = egui::ColorImage::from_rgba_unmultiplied([sw, sh], &out);
        match &mut self.scope_tex {
            Some(t) => t.set(img, egui::TextureOptions::LINEAR),
            None => self.scope_tex = Some(ctx.load_texture("montage-scope", img, egui::TextureOptions::LINEAR)),
        }
    }

    fn poll_preview(&mut self, ctx: &egui::Context) {
        if self.play.is_some() {
            return;
        }
        if let Some((_, rx)) = &self.preview_rx {
            if let Ok(r) = rx.try_recv() {
                match r {
                    Ok(img) => {
                        self.last_frame = Some((img.width() as usize, img.height() as usize, img.as_raw().clone()));
                        if self.scope.is_some() {
                            self.update_scope(ctx, img.width() as usize, img.height() as usize, img.as_raw());
                        }
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
        if self.preview_rx.is_none() && self.shown != Some(want) && self.drag.is_none() && ffmpeg::find("ffmpeg").is_some() {
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

    /// Les raccourcis de Premiere Pro.
    fn keyboard(&mut self, ui: &egui::Ui) {
        let ctx = ui.ctx().clone();
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        use egui::Key;
        let (cmd, shift, alt) = ui.input(|i| (i.modifiers.command, i.modifiers.shift, i.modifiers.alt));
        let k = |key: Key| ui.input(|i| i.key_pressed(key));
        let fps = self.project.fps.max(1.0);
        if cmd {
            if k(Key::Z) {
                self.undo(shift);
            }
            if k(Key::Y) {
                self.undo(true);
            }
            if k(Key::K) {
                self.add_edit();
            }
            if k(Key::A) {
                self.sel = self.project.clips.iter().map(|c| c.id).collect();
            }
            if k(Key::L) {
                // Lier / délier la sélection
                let idx = self.selected();
                if idx.iter().any(|&i| self.project.clips[i].link != 0) {
                    for &i in &idx {
                        self.project.clips[i].link = 0;
                    }
                } else if idx.len() > 1 {
                    let id = self.project.next_link();
                    for &i in &idx {
                        self.project.clips[i].link = id;
                    }
                }
                self.changed();
            }
            if k(Key::D) {
                if let Some(i) = self.first_selected() {
                    let at = self.project.clips[i].start;
                    self.apply_effect(i, if shift { "afade" } else { "xfade" }, at);
                }
            }
            if k(Key::M) {
                self.export_to(&ctx);
            }
        }
        // Copier / coller arrivent comme événements du presse-papiers (Ctrl+Maj+V : coller en insérant)
        let (copy, paste) = ui.input(|i| (i.events.iter().any(|e| matches!(e, egui::Event::Copy)), i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)))));
        if copy {
            self.copy();
        }
        if paste {
            self.paste(shift);
        }
        if cmd {
            return;
        }
        if k(Key::Space) {
            if self.play.is_some() {
                self.stop();
            } else {
                self.start(&ctx);
            }
        }
        // J K L : arrière, arrêt, lecture
        if k(Key::L) && self.play.is_none() {
            self.start(&ctx);
        }
        if k(Key::K) {
            self.stop();
        }
        if k(Key::J) {
            let t = self.playhead - 1.0;
            self.seek(t);
        }
        let step = if shift { 5.0 } else { 1.0 } / fps;
        if k(Key::ArrowLeft) {
            let t = seconds(self.playhead - step, fps);
            self.seek(t);
        }
        if k(Key::ArrowRight) {
            let t = seconds(self.playhead + step, fps);
            self.seek(t);
        }
        if k(Key::ArrowUp) {
            let t = self.edits().into_iter().rev().find(|&e| e < self.playhead - 1e-3).unwrap_or(0.0);
            self.seek(t);
        }
        if k(Key::ArrowDown) {
            if let Some(t) = self.edits().into_iter().find(|&e| e > self.playhead + 1e-3) {
                self.seek(t);
            }
        }
        if k(Key::Home) {
            self.seek(0.0);
        }
        if k(Key::End) {
            let t = self.project.duration();
            self.seek(t);
        }
        if k(Key::I) {
            self.project.mark_in = Some(self.playhead);
            self.changed();
        }
        if k(Key::O) {
            self.project.mark_out = Some(self.playhead);
            self.changed();
        }
        if alt && k(Key::X) {
            (self.project.mark_in, self.project.mark_out) = (None, None);
            self.changed();
        }
        if k(Key::M) {
            let t = self.playhead;
            if !self.project.markers.iter().any(|m| (m - t).abs() < 1e-3) {
                self.project.markers.push(t);
                self.project.markers.sort_by(|a, b| a.total_cmp(b));
                self.changed();
            }
        }
        if k(Key::S) {
            self.snap = !self.snap;
        }
        if k(Key::Q) {
            self.ripple_trim(true);
        }
        if k(Key::W) {
            self.ripple_trim(false);
        }
        if k(Key::Delete) || k(Key::Backspace) {
            self.delete(shift, alt);
        }
        if k(Key::Comma) {
            self.source_edit(true);
        }
        if k(Key::Period) {
            self.source_edit(false);
        }
        if k(Key::Semicolon) {
            self.delete(false, false);
        }
        if k(Key::Quote) {
            self.delete(true, false);
        }
        if k(Key::Equals) || k(Key::Plus) {
            self.zoom = (self.zoom * 1.5).min(800.0);
        }
        if k(Key::Minus) {
            self.zoom = (self.zoom / 1.5).max(2.0);
        }
        if k(Key::Backslash) {
            self.fit_zoom = true;
        }
        for (tool, key, _) in TOOLS {
            let key = match key {
                "V" => Key::V,
                "A" => Key::A,
                "B" => Key::B,
                "N" => Key::N,
                "R" => Key::R,
                "C" => Key::C,
                "Y" => Key::Y,
                "H" => Key::H,
                "Z" => Key::Z,
                _ => Key::T,
            };
            if k(key) && !alt {
                self.tool = tool;
            }
        }
    }

    fn copy(&mut self) {
        let idx = self.with_links(&self.selected(), false);
        self.clipboard = idx.iter().map(|&i| self.project.clips[i].clone()).collect();
        if !self.clipboard.is_empty() {
            self.message = Some(format!("{} {}", self.clipboard.len(), crate::i18n::t_mid("clip(s) copié(s)")));
        }
    }

    fn paste(&mut self, insert: bool) {
        if self.clipboard.is_empty() {
            return;
        }
        let clips = self.clipboard.clone();
        let len = clips.iter().map(|c| c.end()).fold(0.0, f32::max) - clips.iter().map(|c| c.start).fold(f32::MAX, f32::min);
        let at = self.playhead;
        self.sel = self.place(clips, at, insert);
        self.playhead = at + len;
        self.changed();
    }
}

// ---------------------------------------------------------------------- petits éléments d'interface

/// Onglets d'un panneau, soulignés à l'accent comme le reste de l'application.
fn tab_row(ui: &mut egui::Ui, current: &mut u8, tabs: &[&str]) {
    let p = crate::theme::pal();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        for (k, t) in tabs.iter().enumerate() {
            let on = *current == k as u8;
            let r = ui.add(egui::Label::new(RichText::new(crate::i18n::t(t)).size(12.0).color(if on { p.text } else { p.muted }).family(if on { crate::theme::bold() } else { egui::FontFamily::Proportional })).sense(Sense::click()));
            if on {
                ui.painter().line_segment([pos2(r.rect.left(), r.rect.bottom() + 2.0), pos2(r.rect.right(), r.rect.bottom() + 2.0)], Stroke::new(2.0, p.accent));
            }
            if r.clicked() {
                *current = k as u8;
            }
        }
    });
    let r = ui.available_rect_before_wrap();
    ui.painter().line_segment([pos2(r.left(), r.top() + 1.0), pos2(r.right(), r.top() + 1.0)], Stroke::new(1.0, p.line));
}

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Tool(Tool),
    MarkIn,
    MarkOut,
    GoIn,
    GoOut,
    StepBack,
    StepFwd,
    Play,
    Pause,
    Lift,
    Extract,
    Camera,
    Insert,
    Overwrite,
    Lock,
    Eye,
    Magnet,
    Link,
    Marker,
    Speaker,
}

/// Dessine une icône simple (traits) dans `r`.
pub fn draw_icon(pt: &egui::Painter, r: Rect, icon: Icon, col: Color32) {
    let c = r.center();
    let s = r.width().min(r.height()) * 0.5;
    let st = Stroke::new(1.4, col);
    let poly = |pts: Vec<Pos2>| egui::Shape::convex_polygon(pts, col, Stroke::NONE);
    let l = |a: (f32, f32), b: (f32, f32)| egui::Shape::line_segment([pos2(c.x + a.0 * s, c.y + a.1 * s), pos2(c.x + b.0 * s, c.y + b.1 * s)], st);
    let pp = |x: f32, y: f32| pos2(c.x + x * s, c.y + y * s);
    match icon {
        Icon::Tool(Tool::Select) => {
            pt.add(egui::Shape::convex_polygon(vec![pp(-0.45, -0.75), pp(0.45, 0.15), pp(0.0, 0.15), pp(-0.45, 0.6)], col, Stroke::NONE));
            pt.add(l((-0.05, 0.1), (0.25, 0.7)));
        }
        Icon::Tool(Tool::TrackFwd) => {
            for dx in [-0.45, 0.15] {
                pt.add(poly(vec![pp(dx, -0.45), pp(dx + 0.4, 0.0), pp(dx, 0.45)]));
            }
            pt.add(l((-0.75, -0.6), (-0.75, 0.6)));
        }
        Icon::Tool(Tool::Ripple) => {
            pt.add(l((-0.5, -0.7), (-0.5, 0.7)));
            pt.add(l((-0.5, -0.7), (-0.2, -0.7)));
            pt.add(l((-0.5, 0.7), (-0.2, 0.7)));
            pt.add(l((-0.2, 0.0), (0.6, 0.0)));
            pt.add(poly(vec![pp(0.75, 0.0), pp(0.45, -0.25), pp(0.45, 0.25)]));
        }
        Icon::Tool(Tool::Roll) => {
            pt.add(l((0.0, -0.7), (0.0, 0.7)));
            pt.add(poly(vec![pp(-0.75, 0.0), pp(-0.35, -0.3), pp(-0.35, 0.3)]));
            pt.add(poly(vec![pp(0.75, 0.0), pp(0.35, -0.3), pp(0.35, 0.3)]));
        }
        Icon::Tool(Tool::Stretch) => {
            pt.add(l((-0.7, -0.6), (-0.7, 0.6)));
            pt.add(l((0.7, -0.6), (0.7, 0.6)));
            pt.add(l((-0.5, 0.0), (0.5, 0.0)));
            pt.add(poly(vec![pp(-0.55, 0.0), pp(-0.25, -0.25), pp(-0.25, 0.25)]));
            pt.add(poly(vec![pp(0.55, 0.0), pp(0.25, -0.25), pp(0.25, 0.25)]));
        }
        Icon::Tool(Tool::Razor) => {
            pt.add(poly(vec![pp(-0.2, -0.75), pp(0.2, -0.75), pp(0.2, 0.25), pp(0.0, 0.45), pp(-0.2, 0.25)]));
            pt.add(l((0.0, 0.45), (0.0, 0.8)));
        }
        Icon::Tool(Tool::Slip) => {
            pt.add(l((-0.75, -0.6), (-0.75, 0.6)));
            pt.add(l((0.75, -0.6), (0.75, 0.6)));
            pt.add(l((-0.45, 0.0), (0.45, 0.0)));
            pt.add(poly(vec![pp(-0.5, 0.0), pp(-0.2, -0.25), pp(-0.2, 0.25)]));
            pt.add(poly(vec![pp(0.5, 0.0), pp(0.2, -0.25), pp(0.2, 0.25)]));
            pt.add(l((-0.4, -0.5), (0.4, -0.5)));
            pt.add(l((-0.4, 0.5), (0.4, 0.5)));
        }
        Icon::Tool(Tool::Hand) => {
            pt.rect_stroke(Rect::from_center_size(pp(0.0, 0.2), vec2(s * 1.0, s * 0.9)), 3.0, st, egui::StrokeKind::Middle);
            for dx in [-0.35, -0.12, 0.12, 0.35] {
                pt.add(l((dx, -0.25), (dx, -0.75)));
            }
        }
        Icon::Tool(Tool::Zoom) => {
            pt.circle_stroke(pp(-0.15, -0.15), s * 0.45, st);
            pt.add(l((0.18, 0.18), (0.7, 0.7)));
        }
        Icon::Tool(Tool::Type) => {
            pt.add(l((-0.55, -0.65), (0.55, -0.65)));
            pt.add(l((0.0, -0.65), (0.0, 0.7)));
        }
        Icon::MarkIn => {
            pt.add(l((-0.1, -0.7), (-0.1, 0.7)));
            pt.add(l((-0.1, -0.7), (0.35, -0.7)));
            pt.add(l((-0.1, 0.7), (0.35, 0.7)));
        }
        Icon::MarkOut => {
            pt.add(l((0.1, -0.7), (0.1, 0.7)));
            pt.add(l((0.1, -0.7), (-0.35, -0.7)));
            pt.add(l((0.1, 0.7), (-0.35, 0.7)));
        }
        Icon::GoIn => {
            pt.add(l((-0.6, -0.6), (-0.6, 0.6)));
            pt.add(poly(vec![pp(-0.5, 0.0), pp(0.4, -0.55), pp(0.4, 0.55)]));
        }
        Icon::GoOut => {
            pt.add(l((0.6, -0.6), (0.6, 0.6)));
            pt.add(poly(vec![pp(0.5, 0.0), pp(-0.4, -0.55), pp(-0.4, 0.55)]));
        }
        Icon::StepBack => {
            pt.add(poly(vec![pp(-0.5, 0.0), pp(0.25, -0.55), pp(0.25, 0.55)]));
            pt.add(l((0.5, -0.55), (0.5, 0.55)));
        }
        Icon::StepFwd => {
            pt.add(poly(vec![pp(0.5, 0.0), pp(-0.25, -0.55), pp(-0.25, 0.55)]));
            pt.add(l((-0.5, -0.55), (-0.5, 0.55)));
        }
        Icon::Play => {
            pt.add(poly(vec![pp(-0.45, -0.7), pp(0.65, 0.0), pp(-0.45, 0.7)]));
        }
        Icon::Pause => {
            pt.rect_filled(Rect::from_min_max(pp(-0.5, -0.6), pp(-0.15, 0.6)), 0.0, col);
            pt.rect_filled(Rect::from_min_max(pp(0.15, -0.6), pp(0.5, 0.6)), 0.0, col);
        }
        Icon::Lift => {
            pt.rect_stroke(Rect::from_min_max(pp(-0.7, 0.1), pp(0.7, 0.6)), 1.0, st, egui::StrokeKind::Middle);
            pt.add(l((0.0, 0.0), (0.0, -0.7)));
            pt.add(poly(vec![pp(0.0, -0.8), pp(-0.3, -0.4), pp(0.3, -0.4)]));
        }
        Icon::Extract => {
            pt.add(l((-0.7, 0.4), (-0.2, 0.4)));
            pt.add(l((0.2, 0.4), (0.7, 0.4)));
            pt.add(poly(vec![pp(0.0, -0.8), pp(-0.3, -0.4), pp(0.3, -0.4)]));
            pt.add(l((0.0, -0.4), (0.0, 0.2)));
        }
        Icon::Camera => {
            pt.rect_stroke(Rect::from_min_max(pp(-0.75, -0.4), pp(0.75, 0.6)), 2.0, st, egui::StrokeKind::Middle);
            pt.circle_stroke(pp(0.0, 0.1), s * 0.3, st);
            pt.add(l((-0.3, -0.4), (-0.15, -0.65)));
            pt.add(l((-0.15, -0.65), (0.15, -0.65)));
            pt.add(l((0.15, -0.65), (0.3, -0.4)));
        }
        Icon::Insert => {
            pt.rect_stroke(Rect::from_min_max(pp(-0.75, -0.2), pp(0.75, 0.6)), 1.0, st, egui::StrokeKind::Middle);
            pt.add(poly(vec![pp(0.0, 0.1), pp(-0.3, -0.5), pp(0.3, -0.5)]));
        }
        Icon::Overwrite => {
            pt.rect_filled(Rect::from_min_max(pp(-0.75, -0.2), pp(0.75, 0.6)), 1.0, col.gamma_multiply(0.4));
            pt.add(poly(vec![pp(0.0, 0.1), pp(-0.3, -0.5), pp(0.3, -0.5)]));
        }
        Icon::Lock => {
            pt.rect_filled(Rect::from_min_max(pp(-0.5, -0.05), pp(0.5, 0.7)), 1.0, col);
            pt.add(egui::Shape::line(vec![pp(-0.3, -0.05), pp(-0.3, -0.45), pp(-0.15, -0.65), pp(0.15, -0.65), pp(0.3, -0.45), pp(0.3, -0.05)], st));
        }
        Icon::Eye => {
            let pts: Vec<Pos2> = (0..=16).map(|k| {
                let a = k as f32 / 16.0 * std::f32::consts::TAU;
                pp(a.cos() * 0.75, a.sin() * 0.38)
            }).collect();
            pt.add(egui::Shape::closed_line(pts, st));
            pt.circle_filled(c, s * 0.22, col);
        }
        Icon::Magnet => {
            pt.add(egui::Shape::line(vec![pp(-0.5, -0.6), pp(-0.5, 0.1), pp(-0.3, 0.5), pp(0.0, 0.62), pp(0.3, 0.5), pp(0.5, 0.1), pp(0.5, -0.6)], Stroke::new(2.4, col)));
        }
        Icon::Link => {
            pt.rect_stroke(Rect::from_min_max(pp(-0.8, -0.3), pp(0.1, 0.3)), s * 0.3, st, egui::StrokeKind::Middle);
            pt.rect_stroke(Rect::from_min_max(pp(-0.1, -0.3), pp(0.8, 0.3)), s * 0.3, st, egui::StrokeKind::Middle);
        }
        Icon::Marker => {
            pt.add(poly(vec![pp(-0.45, -0.65), pp(0.45, -0.65), pp(0.45, 0.25), pp(0.0, 0.7), pp(-0.45, 0.25)]));
        }
        Icon::Speaker => {
            pt.add(poly(vec![pp(-0.7, -0.25), pp(-0.35, -0.25), pp(0.1, -0.65), pp(0.1, 0.65), pp(-0.35, 0.25), pp(-0.7, 0.25)]));
            pt.add(l((0.35, -0.35), (0.35, 0.35)));
            pt.add(l((0.6, -0.55), (0.6, 0.55)));
        }
    }
}

/// Bouton carré à icône, allumé ou non.
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, on: bool, tip: &str) -> egui::Response {
    let p = crate::theme::pal();
    let (r, resp) = ui.allocate_exact_size(vec2(24.0, 22.0), Sense::click());
    let resp = resp.on_hover_text(crate::i18n::t(tip));
    if on {
        ui.painter().rect_filled(r, 3.0, p.accent.gamma_multiply(0.18));
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 3.0, p.line.gamma_multiply(0.6));
    }
    draw_icon(ui.painter(), r.shrink(6.0), icon, if on { p.accent } else { p.text });
    resp
}

/// Valeur réglable façon Premiere : un nombre à l'accent qu'on fait glisser.
fn scrub(ui: &mut egui::Ui, v: &mut f32, speed: f64, range: std::ops::RangeInclusive<f32>, suffix: &str, decimals: usize) -> bool {
    let p = crate::theme::pal();
    ui.scope(|ui| {
        ui.visuals_mut().override_text_color = Some(p.accent);
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.weak_bg_fill = Color32::TRANSPARENT;
        w.inactive.bg_stroke = Stroke::NONE;
        ui.add(egui::DragValue::new(v).speed(speed).range(range).suffix(suffix).fixed_decimals(decimals)).changed()
    })
    .inner
}

/// Couleur d'un clip selon son genre (étiquettes de Premiere, adoucies).
fn clip_color(c: &Clip) -> Color32 {
    if c.audio {
        return Color32::from_rgb(96, 186, 146);
    }
    match c.kind.as_str() {
        "image" => Color32::from_rgb(196, 140, 200),
        "text" => Color32::from_rgb(226, 132, 160),
        _ => Color32::from_rgb(132, 140, 226),
    }
}

include!("montage_panels.rs");

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(start: f32, len: f32, track: usize, audio: bool) -> Clip {
        let mut p = Project::default();
        let k = p.add_text(0.0);
        let mut c = p.clips[k].clone();
        (c.kind, c.start, c.src_in, c.src_out, c.track, c.audio, c.fade_in, c.fade_out, c.id) = ("image".into(), start, 0.0, len, track, audio, 0.0, 0.0, 0);
        c
    }

    #[test]
    fn montage_premiere() {
        let dir = std::env::temp_dir().join("aryadne-premiere-test");
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("m.aryav");
        let mut p = Project::default();
        p.clips = vec![clip(0.0, 4.0, 0, false), clip(4.0, 4.0, 0, false), clip(0.0, 8.0, 0, true)];
        // Mode libre (comme Premiere) : la timeline magnétique est testée dans montage.rs
        p.magnetic = false;
        p.ensure_ids();
        p.write(&f).unwrap();
        let mut e = Editor::open(f.clone(), dir.to_str().unwrap());
        // Recouvrir 2 s au milieu du premier clip : il est coupé en deux
        let keep = HashSet::new();
        e.clear_range(false, 0, 1.0, 3.0, &keep);
        let mut v: Vec<(f32, f32)> = e.project.clips.iter().filter(|c| !c.audio).map(|c| (c.start, c.end())).collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(v, vec![(0.0, 1.0), (3.0, 4.0), (4.0, 8.0)]);
        // Ctrl+K à 6 s : tout ce qui passe sous la tête est coupé (V1 et A1)
        e.playhead = 6.0;
        e.add_edit();
        assert_eq!(e.project.clips.len(), 6);
        // Supprimer avec propagation le clip qui commence à 6 s sur V1 : rien ne suit, la durée de V1 recule
        let id = e.project.clips.iter().find(|c| !c.audio && c.start == 6.0).unwrap().id;
        e.sel = vec![id];
        e.delete(true, false);
        assert!(e.project.clips.iter().filter(|c| !c.audio).all(|c| c.end() <= 6.0 + 1e-3));
        // Supprimer avec propagation un clip au milieu : la suite recule
        let id = e.project.clips.iter().find(|c| !c.audio && c.start == 0.0).unwrap().id;
        e.sel = vec![id];
        e.delete(true, false);
        let mut v: Vec<f32> = e.project.clips.iter().filter(|c| !c.audio).map(|c| c.start).collect();
        v.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(v, vec![2.0, 3.0]);
        // Insérer une copie à 0 : tout recule de sa longueur, sur toutes les pistes
        let copy = vec![clip(0.0, 1.5, 0, false)];
        e.place(copy, 0.0, true);
        assert!(e.project.clips.iter().filter(|c| c.audio).all(|c| c.start >= 1.5 - 1e-3));
        // Q : la partie avant la tête de lecture disparaît et tout se referme
        let before = e.project.duration();
        e.playhead = 2.0;
        e.ripple_trim(true);
        assert!(e.project.duration() < before);
        // Annuler revient en arrière
        let n = e.history.len();
        e.undo(false);
        assert_eq!(e.history.len(), n - 1);
    }
}

/// En-tête de panneau de 34 px, comme la maquette : titre en gras, compte discret
pub(crate) fn pane_head(ui: &mut egui::Ui, title: &str, count: Option<&str>) {
    let p = crate::theme::pal();
    let (hr, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), Sense::hover());
    ui.painter().hline(hr.x_range(), hr.bottom() - 0.5, Stroke::new(1.0, p.line));
    let g = ui.painter().layout_no_wrap(title.to_string(), egui::FontId::new(13.0, crate::theme::bold()), p.text);
    let w = g.size().x;
    ui.painter().galley(pos2(hr.left() + 12.0, hr.center().y - g.size().y / 2.0), g, p.text);
    if let Some(c) = count {
        ui.painter().text(pos2(hr.left() + 20.0 + w, hr.center().y), egui::Align2::LEFT_CENTER, c, egui::FontId::proportional(11.0), p.muted);
    }
}

/// Rangée d'onglets de 34 px (pastille en relief pour l'onglet ouvert) ; rend l'onglet cliqué
pub(crate) fn tab_chips(ui: &mut egui::Ui, cur: u8, tabs: &[&str]) -> Option<u8> {
    let p = crate::theme::pal();
    let (hr, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), Sense::hover());
    ui.painter().hline(hr.x_range(), hr.bottom() - 0.5, Stroke::new(1.0, p.line));
    let mut x = hr.left() + 6.0;
    let mut out = None;
    for (k, label) in tabs.iter().enumerate() {
        let g = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(12.5), p.text);
        let r = Rect::from_min_size(pos2(x, hr.center().y - 12.0), egui::vec2(g.size().x + 16.0, 24.0));
        let resp = ui.interact(r, ui.scope_id().with(("tab-chip", k)), Sense::click());
        let on = cur == k as u8;
        if on {
            ui.painter().rect(r, 2.0, p.raised, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 2.0, p.raised);
        }
        ui.painter().galley(pos2(r.left() + 8.0, r.center().y - g.size().y / 2.0), g, if on { p.text } else { p.muted });
        if resp.clicked() {
            out = Some(k as u8);
        }
        x = r.right() + 2.0;
    }
    out
}
