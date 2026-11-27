//! Le séquenceur de l'étape Son (`.arymusic`) : des pistes d'instruments, des régions posées sur
//! la grille des mesures, et dans chaque région des notes (rouleau de piano). Les instruments sont
//! des synthés intégrés et une batterie synthétisée : rien à installer, le rendu se fait ici.
//! Des générateurs simples (accords, basse, rythmes, mélodie) donnent un point de départ.

use crate::audio::RATE;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub fn is_song(rel: &str) -> bool {
    crate::formats::has(rel, "music")
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Hash)]
pub enum Inst {
    Piano,
    Keys,
    Pluck,
    Pad,
    Bass,
    Lead,
    Drums,
}

pub const INSTS: [(Inst, &str, &str); 7] = [
    (Inst::Piano, "Piano", "attaque franche, notes qui s'éteignent"),
    (Inst::Keys, "Piano électrique", "doux, un peu cloche"),
    (Inst::Pluck, "Corde pincée", "guitare, harpe"),
    (Inst::Pad, "Nappe", "large et lente, pour le fond"),
    (Inst::Bass, "Basse", "ronde, filtrée"),
    (Inst::Lead, "Lead", "carré, avec vibrato"),
    (Inst::Drums, "Batterie", "kick, caisse claire, charleston…"),
];

impl Inst {
    pub fn name(self) -> &'static str {
        INSTS.iter().find(|i| i.0 == self).map(|i| i.1).unwrap_or("")
    }
}

/// Les pièces de la batterie : (note MIDI, nom), de haut en bas dans le rouleau
pub const KIT: [(u8, &str); 9] = [(49, "Cymbale"), (46, "Charleston ouvert"), (42, "Charleston"), (50, "Tom aigu"), (47, "Tom médium"), (45, "Tom grave"), (39, "Clap"), (38, "Caisse claire"), (36, "Kick")];

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Note {
    /// Début dans la région, en temps
    pub t: f64,
    pub len: f64,
    /// Hauteur MIDI (60 = do central)
    pub p: u8,
    #[serde(default = "full")]
    pub v: f32,
}

fn full() -> f32 {
    0.8
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Region {
    /// Début et longueur sur la piste, en temps
    pub start: f64,
    pub len: f64,
    #[serde(default)]
    pub name: String,
    pub notes: Vec<Note>,
}

impl Region {
    pub fn end(&self) -> f64 {
        self.start + self.len
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct MTrack {
    pub name: String,
    pub inst: Inst,
    pub color: [u8; 3],
    #[serde(default)]
    pub volume: f32,
    #[serde(default)]
    pub pan: f32,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub solo: bool,
    pub regions: Vec<Region>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Song {
    pub bpm: f32,
    #[serde(default = "four")]
    pub beats_per_bar: u32,
    /// Longueur du morceau, en mesures
    pub bars: u32,
    /// Tonalité : 0 = do, 1 = do dièse…
    #[serde(default)]
    pub key: u8,
    #[serde(default)]
    pub minor: bool,
    pub tracks: Vec<MTrack>,
}

fn four() -> u32 {
    4
}

pub const NOTE_NAMES: [&str; 12] = ["do", "do♯", "ré", "ré♯", "mi", "fa", "fa♯", "sol", "sol♯", "la", "la♯", "si"];

/// Les mêmes notes en lettres, pour l'interface anglaise (C, D, E…)
pub const NOTE_LETTERS: [&str; 12] = ["C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B"];

/// Nom d'une classe de hauteur (0 = do) dans la langue de l'interface : « la » en français, « A » en anglais.
pub fn pitch_class_name(k: usize) -> &'static str {
    if crate::i18n::current() == "en" { NOTE_LETTERS[k % 12] } else { NOTE_NAMES[k % 12] }
}

pub fn note_name(p: u8) -> String {
    format!("{}{}", pitch_class_name(p as usize), p as i32 / 12 - 1)
}

const MAJOR: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
const MINOR: [u8; 7] = [0, 2, 3, 5, 7, 8, 10];

impl Song {
    pub fn read(path: &Path) -> Result<Song, String> {
        let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| format!("{} : {e}", crate::i18n::t("morceau illisible")))
    }

    pub fn write(&self, path: &Path) -> Result<(), String> {
        crate::vault::atomic_write(path, serde_json::to_string_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }

    pub fn spb(&self) -> f64 {
        60.0 / self.bpm.max(20.0) as f64
    }

    pub fn beats(&self) -> f64 {
        (self.bars * self.beats_per_bar) as f64
    }

    pub fn scale(&self) -> [u8; 7] {
        let s = if self.minor { MINOR } else { MAJOR };
        s.map(|d| (d + self.key) % 12)
    }

    pub fn in_scale(&self, p: u8) -> bool {
        self.scale().contains(&(p % 12))
    }

    pub fn audible(&self) -> Vec<usize> {
        let solo = self.tracks.iter().any(|t| t.solo);
        (0..self.tracks.len()).filter(|&i| if solo { self.tracks[i].solo } else { !self.tracks[i].mute }).collect()
    }

    pub fn new_track(&self, inst: Inst) -> MTrack {
        let colors = crate::audio::COLORS;
        MTrack { name: crate::i18n::t(inst.name()), inst, color: colors[self.tracks.len() % colors.len()], volume: 0.0, pan: 0.0, mute: false, solo: false, regions: vec![] }
    }

    /// Un morceau neuf : un rythme, des accords et une basse sur huit mesures, et une piste de mélodie vide
    pub fn starter() -> Song {
        let mut s = Song { bpm: 104.0, beats_per_bar: 4, bars: 8, key: 9, minor: true, tracks: vec![] };
        let bar = 4.0;
        let mut drums = s.new_track(Inst::Drums);
        drums.regions.push(Region { start: 0.0, len: 8.0 * bar, name: crate::i18n::t("Rythme"), notes: drum_pattern(0, 8) });
        s.tracks.push(drums);
        let mut keys = s.new_track(Inst::Keys);
        keys.regions.push(Region { start: 0.0, len: 8.0 * bar, name: crate::i18n::t("Accords"), notes: chords(&s, 8, 0) });
        s.tracks.push(keys);
        let mut bass = s.new_track(Inst::Bass);
        bass.regions.push(Region { start: 0.0, len: 8.0 * bar, name: crate::i18n::t("Basse"), notes: bassline(&s, 8, 0) });
        s.tracks.push(bass);
        s.tracks.push(s.new_track(Inst::Lead));
        s
    }
}

// ---------- Générateurs ----------

/// Les suites d'accords proposées (degrés de la gamme, à partir de 0)
pub const PROGRESSIONS: [(&str, [u8; 4]); 4] = [("I V vi IV", [0, 4, 5, 3]), ("vi IV I V", [5, 3, 0, 4]), ("I vi IV V", [0, 5, 3, 4]), ("ii V I I", [1, 4, 0, 0])];

/// Accord de trois sons sur un degré de la gamme, autour du do central
fn triad(s: &Song, degree: u8) -> [u8; 3] {
    let sc = if s.minor { MINOR } else { MAJOR };
    let n = |d: u8| {
        let d = d as usize;
        let oct = (d / 7) as u8 * 12;
        48 + s.key + sc[d % 7] + oct
    };
    let mut c = [n(degree), n(degree + 2), n(degree + 4)];
    // Resserre l'accord autour de 60
    for v in &mut c {
        while *v < 55 {
            *v += 12;
        }
        while *v > 67 {
            *v -= 12;
        }
    }
    c.sort();
    c
}

pub fn chords(s: &Song, bars: u32, prog: usize) -> Vec<Note> {
    let p = PROGRESSIONS[prog % PROGRESSIONS.len()].1;
    let bpb = s.beats_per_bar as f64;
    (0..bars)
        .flat_map(|b| {
            let deg = p[b as usize % 4] % 7;
            triad(s, deg).map(|q| Note { t: b as f64 * bpb, len: bpb - 0.1, p: q, v: 0.6 })
        })
        .collect()
}

pub fn bassline(s: &Song, bars: u32, prog: usize) -> Vec<Note> {
    let p = PROGRESSIONS[prog % PROGRESSIONS.len()].1;
    let sc = if s.minor { MINOR } else { MAJOR };
    let bpb = s.beats_per_bar as f64;
    let mut out = vec![];
    for b in 0..bars {
        let root = 36 + s.key + sc[(p[b as usize % 4] % 7) as usize];
        // Fondamentale sur le premier temps, puis des croches avec l'octave
        out.push(Note { t: b as f64 * bpb, len: 1.4, p: root, v: 0.9 });
        out.push(Note { t: b as f64 * bpb + 1.5, len: 0.4, p: root, v: 0.7 });
        out.push(Note { t: b as f64 * bpb + 2.0, len: 0.9, p: root + 12, v: 0.75 });
        out.push(Note { t: b as f64 * bpb + 3.0, len: 0.9, p: root, v: 0.8 });
    }
    out
}

pub const DRUM_STYLES: [&str; 3] = ["Pop", "Four on the floor", "Trap"];

pub fn drum_pattern(style: usize, bars: u32) -> Vec<Note> {
    let mut out = vec![];
    let hit = |out: &mut Vec<Note>, t: f64, p: u8, v: f32| out.push(Note { t, len: 0.25, p, v });
    for b in 0..bars {
        let o = b as f64 * 4.0;
        match style {
            1 => {
                for k in 0..4 {
                    hit(&mut out, o + k as f64, 36, 0.95);
                    hit(&mut out, o + k as f64 + 0.5, 46, 0.55);
                }
                hit(&mut out, o + 1.0, 39, 0.8);
                hit(&mut out, o + 3.0, 39, 0.8);
            }
            2 => {
                for k in 0..16 {
                    hit(&mut out, o + k as f64 * 0.25, 42, if k % 4 == 0 { 0.7 } else { 0.45 });
                }
                for t in [0.0, 1.75, 2.5] {
                    hit(&mut out, o + t, 36, 0.95);
                }
                hit(&mut out, o + 2.0, 38, 0.9);
            }
            _ => {
                for k in 0..8 {
                    hit(&mut out, o + k as f64 * 0.5, 42, if k % 2 == 0 { 0.65 } else { 0.45 });
                }
                hit(&mut out, o, 36, 0.95);
                hit(&mut out, o + 2.5, 36, 0.8);
                hit(&mut out, o + 1.0, 38, 0.9);
                hit(&mut out, o + 3.0, 38, 0.9);
            }
        }
    }
    out
}

/// Une mélodie sur la gamme du morceau : un pas à la fois, surtout par degrés voisins, sur les notes des accords aux temps forts
pub fn melody(s: &Song, bars: u32, seed: u64) -> Vec<Note> {
    let mut rng = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
    let sc: Vec<u8> = (0..3).flat_map(|o| s.scale().into_iter().map(move |d| 60 + o * 12 + ((d + 12 - s.key) % 12) + s.key)).filter(|p| (60..=84).contains(p)).collect();
    let mut sc = sc;
    sc.sort();
    sc.dedup();
    let mut i = sc.len() / 3;
    let mut out = vec![];
    let mut t = 0.0;
    let end = (bars * s.beats_per_bar) as f64;
    while t < end {
        let len: f64 = [0.5, 0.5, 1.0, 1.0, 1.5, 0.5, 2.0][rng.next() as usize % 7];
        let step: i32 = [-2, -1, -1, 0, 1, 1, 2, 3][rng.next() as usize % 8];
        i = (i as i32 + step).clamp(0, sc.len() as i32 - 1) as usize;
        // Un silence de temps en temps
        if rng.next() % 7 != 0 {
            out.push(Note { t, len: (len - 0.05).min(end - t), p: sc[i], v: 0.6 + (rng.next() % 30) as f32 / 100.0 });
        }
        t += len;
    }
    out
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn noise(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 23) as f32 - 1.0
    }
}

// ---------- Synthèse ----------

fn freq(p: f32) -> f32 {
    440.0 * 2f32.powf((p - 69.0) / 12.0)
}

/// Le son d'une note (mono) : `dur` secondes tenues, plus la queue de l'instrument
pub fn voice(inst: Inst, p: u8, v: f32, dur: f32) -> Vec<f32> {
    let r = RATE as f32;
    let f = freq(p as f32);
    let tau = std::f32::consts::TAU;
    let mut rng = Rng(0x2545F4914F6CDD1D ^ (p as u64 * 7919));
    // Enveloppe : attaque, maintien jusqu'à `dur`, puis relâche
    let env = |t: f32, a: f32, rel: f32| -> f32 {
        let up = (t / a.max(1e-4)).min(1.0);
        if t < dur {
            up
        } else {
            up * (1.0 - (t - dur) / rel).max(0.0)
        }
    };
    let tail = match inst {
        Inst::Pad => 0.9,
        Inst::Piano | Inst::Keys | Inst::Pluck => 0.35,
        Inst::Drums => 0.0,
        _ => 0.12,
    };
    let n = (((dur + tail) * r) as usize).max(1);
    let mut out = vec![0.0f32; n];
    match inst {
        Inst::Piano => {
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / r;
                let mut s = 0.0;
                for k in 1..=7 {
                    let kf = k as f32;
                    s += (tau * f * kf * (1.0 + 0.0004 * kf * kf) * t).sin() * (-(1.2 + kf * 0.9) * t).exp() / kf.powf(1.4);
                }
                *o = s * env(t, 0.003, 0.25) * 0.55;
            }
        }
        Inst::Keys => {
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / r;
                let bell = (tau * f * 2.0 * t + 1.2 * (tau * f * t).sin()).sin() * (-5.0 * t).exp() * 0.35;
                let body = (tau * f * t).sin() * (-1.1 * t).exp();
                *o = (body + bell) * env(t, 0.004, 0.3) * 0.5;
            }
        }
        Inst::Pluck => {
            // Karplus-Strong : un bruit qui tourne dans une boucle de la longueur d'une période, adoucie à chaque tour
            let period = ((r / f) as usize).max(2);
            let mut ring: Vec<f32> = (0..period).map(|_| rng.noise()).collect();
            let mut k = 0;
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / r;
                let a = ring[k];
                let b = ring[(k + 1) % period];
                ring[k] = (a + b) * 0.5 * 0.996;
                k = (k + 1) % period;
                *o = a * env(t, 0.001, 0.2) * 0.6;
            }
        }
        Inst::Pad => {
            let mut lp = 0.0f32;
            let mut ph = [0.0f32; 3];
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / r;
                let mut s = 0.0;
                for (j, d) in [0.997f32, 1.0, 1.004].iter().enumerate() {
                    ph[j] = (ph[j] + f * d / r).fract();
                    s += ph[j] * 2.0 - 1.0;
                }
                let c = 1.0 - (-tau * 1400.0 / r).exp();
                lp += (s / 3.0 - lp) * c;
                *o = lp * env(t, 0.45, 0.9) * 0.45;
            }
        }
        Inst::Bass => {
            let mut lp = 0.0f32;
            let mut ph = 0.0f32;
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / r;
                ph = (ph + f / r).fract();
                let saw = ph * 2.0 - 1.0;
                let sub = (tau * f * 0.5 * t).sin();
                let cut = 220.0 + 1600.0 * (-7.0 * t).exp();
                let c = 1.0 - (-tau * cut / r).exp();
                lp += (saw * 0.7 + sub * 0.5 - lp) * c;
                *o = lp * env(t, 0.004, 0.08) * 0.8;
            }
        }
        Inst::Lead => {
            let mut lp = 0.0f32;
            let mut ph = 0.0f32;
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / r;
                let vib = 1.0 + 0.006 * (tau * 5.5 * t).sin() * ((t - 0.2) * 4.0).clamp(0.0, 1.0);
                ph = (ph + f * vib / r).fract();
                let sq = if ph < 0.5 { 1.0 } else { -1.0 };
                let c = 1.0 - (-tau * 2800.0 / r).exp();
                lp += (sq - lp) * c;
                *o = lp * env(t, 0.01, 0.12) * 0.3;
            }
        }
        Inst::Drums => return drum(p, v),
    }
    for o in &mut out {
        *o *= v;
    }
    out
}

fn drum(p: u8, v: f32) -> Vec<f32> {
    let r = RATE as f32;
    let tau = std::f32::consts::TAU;
    let mut rng = Rng(0x9E3779B97F4A7C15 ^ p as u64);
    let len = match p {
        49 => 1.6,
        46 => 0.5,
        36 | 45 | 47 | 50 => 0.45,
        _ => 0.25,
    };
    let n = (len * r) as usize;
    let mut out = vec![0.0f32; n];
    let mut ph = 0.0f32;
    let mut prev = 0.0f32;
    for (i, o) in out.iter_mut().enumerate() {
        let t = i as f32 / r;
        let noise = rng.noise();
        // Bruit « aigu » : la différence de deux échantillons (un passe-haut grossier)
        let hi = noise - prev;
        prev = noise;
        *o = match p {
            36 => {
                let f = 45.0 + 110.0 * (-35.0 * t).exp();
                ph += f / r;
                (tau * ph).sin() * (-8.0 * t).exp() + noise * 0.15 * (-300.0 * t).exp()
            }
            38 => noise * 0.55 * (-16.0 * t).exp() + (tau * 190.0 * t).sin() * 0.5 * (-22.0 * t).exp(),
            39 => {
                let burst = [0.0, 0.011, 0.022].iter().map(|s| if t >= *s { (-60.0 * (t - s)).exp() } else { 0.0 }).sum::<f32>() + if t > 0.022 { (-14.0 * (t - 0.022)).exp() * 0.6 } else { 0.0 };
                hi * 0.45 * burst
            }
            42 => hi * 0.35 * (-55.0 * t).exp(),
            46 => hi * 0.3 * (-8.0 * t).exp(),
            49 => hi * 0.28 * (-2.6 * t).exp(),
            45 | 47 | 50 => {
                let base = match p {
                    45 => 95.0,
                    47 => 125.0,
                    _ => 165.0,
                };
                let f = base * (1.0 + 0.6 * (-20.0 * t).exp());
                ph += f / r;
                (tau * ph).sin() * (-7.0 * t).exp() * 0.8
            }
            _ => 0.0,
        } * v;
    }
    out
}

/// Le mixage stéréo du morceau entre deux instants (en temps)
pub fn render(s: &Song, a: f64, b: f64) -> Vec<f32> {
    let spb = s.spb();
    let n = ((b - a).max(0.0) * spb * RATE as f64) as usize;
    let mut out = vec![0.0f32; n * 2];
    for ti in s.audible() {
        let tr = &s.tracks[ti];
        let g = crate::audio::db(tr.volume);
        let pan = tr.pan.clamp(-1.0, 1.0);
        let (gl, gr) = (g * (1.0 - pan.max(0.0)), g * (1.0 + pan.min(0.0)));
        for rg in &tr.regions {
            for nt in &rg.notes {
                if nt.t >= rg.len || nt.t < 0.0 {
                    continue;
                }
                let start = rg.start + nt.t;
                let len = nt.len.min(rg.len - nt.t);
                // La queue d'un son peut déborder : on prend large avant `a`
                if start >= b || start + len + 2.0 / spb.max(0.01) < a {
                    continue;
                }
                let w = voice(tr.inst, nt.p, nt.v, (len * spb) as f32);
                let off = ((start - a) * spb * RATE as f64).round() as i64;
                for (k, x) in w.iter().enumerate() {
                    let i = off + k as i64;
                    if i < 0 {
                        continue;
                    }
                    let i = i as usize;
                    if i >= n {
                        break;
                    }
                    out[i * 2] += x * gl;
                    out[i * 2 + 1] += x * gr;
                }
            }
        }
    }
    // Limiteur doux : pas de saturation dure
    for x in &mut out {
        *x = (*x * 0.8).tanh();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn morceau_de_depart() {
        let s = Song::starter();
        assert_eq!(s.tracks.len(), 4);
        // La mineur : les accords et la basse restent dans la gamme
        for tr in &s.tracks[1..3] {
            for n in &tr.regions[0].notes {
                assert!(s.in_scale(n.p), "{} hors gamme", note_name(n.p));
            }
        }
        assert_eq!(note_name(57), "la3");
        let mel = melody(&s, 4, 7);
        assert!(!mel.is_empty() && mel.iter().all(|n| s.in_scale(n.p) && n.t + n.len <= 16.0 + 1e-9));
        // Deux mesures à 104 bpm : environ 4,6 s de son, et du son dedans
        let a = render(&s, 0.0, 8.0);
        assert_eq!(a.len(), (8.0 * 60.0 / 104.0 * RATE as f64) as usize * 2);
        let peak = a.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.2 && peak < 1.0, "{peak}");
        for (inst, ..) in INSTS {
            let w = voice(inst, if inst == Inst::Drums { 38 } else { 60 }, 0.8, 0.5);
            assert!(w.iter().any(|v| v.abs() > 0.05), "{inst:?} muet");
            assert!(w.iter().all(|v| v.is_finite()));
        }
        let back: Song = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
