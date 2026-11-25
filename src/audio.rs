//! L'éditeur audio de l'étape Son (`.aryaudio`) : des pistes, et sur chaque piste des clips qui
//! renvoient à des sons du coffre. Rien n'est détruit : un clip garde son début, sa partie du son,
//! son gain, ses fondus et ses effets (rendus par ffmpeg). Le mixage se fait ici, en stéréo à 48 kHz.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const RATE: u32 = 48_000;
/// Échantillons par point de la forme d'onde
pub const PEAK_STEP: usize = 256;

pub const EXT: [&str; 9] = ["wav", "mp3", "flac", "ogg", "m4a", "aac", "opus", "aiff", "wma"];

pub fn is_audio(rel: &str) -> bool {
    let e = rel.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    EXT.contains(&e.as_str())
}

pub fn is_session(rel: &str) -> bool {
    crate::formats::has(rel, "audio")
}

pub fn db(v: f32) -> f32 {
    10f32.powf(v / 20.0)
}

pub fn to_db(v: f32) -> f32 {
    20.0 * v.max(1e-6).log10()
}

/// Effet d'un clip, appliqué par ffmpeg au son d'origine
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Fx {
    /// Réduction de bruit, de 0 à 1
    Denoise { amount: f32 },
    /// Égaliseur trois bandes, en dB
    Eq { low: f32, mid: f32, high: f32 },
    /// Compresseur pour la voix parlée
    Compress,
    /// Coupe-bas
    Highpass { hz: f32 },
    /// Voix claire : coupe-bas, réduction de bruit douce, présence, compression
    Voice,
    /// Hauteur en demi-tons, sans changer la durée
    Pitch { semitones: f32 },
    /// Réverbération : de 0 (sèche) à 1 (grande salle)
    Reverb { amount: f32 },
    /// Adoucit les « s » qui sifflent
    Deess,
    /// Limiteur : rien ne dépasse le plafond (dBFS)
    Limiter { ceiling: f32 },
    /// Gate : coupe ce qui reste sous le seuil (dBFS), le souffle entre les phrases
    Gate { threshold: f32 },
}

impl Fx {
    pub fn label(&self) -> String {
        match self {
            Fx::Denoise { amount } => format!("{} {:.0} %", crate::i18n::t("Réduction de bruit"), amount * 100.0),
            Fx::Eq { low, mid, high } => format!("{} {low:+.0} / {mid:+.0} / {high:+.0} dB", crate::i18n::t("Égaliseur")),
            Fx::Compress => crate::i18n::t("Compresseur"),
            Fx::Highpass { hz } => format!("{} {hz:.0} Hz", crate::i18n::t("Coupe-bas")),
            Fx::Voice => crate::i18n::t("Voix claire"),
            Fx::Pitch { semitones } => format!("{} {semitones:+.1} {}", crate::i18n::t("Hauteur"), crate::i18n::t_mid("demi-tons")),
            Fx::Reverb { amount } => format!("{} {:.0} %", crate::i18n::t("Réverbération"), amount * 100.0),
            Fx::Deess => crate::i18n::t("De-esser"),
            Fx::Limiter { ceiling } => format!("{} {ceiling:+.1} dB", crate::i18n::t("Limiteur")),
            Fx::Gate { threshold } => format!("{} {threshold:.0} dB", crate::i18n::t("Gate")),
        }
    }

    pub fn filter(&self) -> String {
        match self {
            Fx::Denoise { amount } => format!("afftdn=nr={:.1}:nf=-40", 6.0 + amount.clamp(0.0, 1.0) * 34.0),
            Fx::Eq { low, mid, high } => format!("bass=g={low:.1},equalizer=f=1200:t=q:w=1:g={mid:.1},treble=g={high:.1}"),
            Fx::Compress => "acompressor=threshold=-20dB:ratio=3:attack=8:release=160:makeup=3".into(),
            Fx::Highpass { hz } => format!("highpass=f={hz:.0}"),
            Fx::Voice => "highpass=f=80,afftdn=nr=10:nf=-40,equalizer=f=3500:t=q:w=1:g=3,acompressor=threshold=-20dB:ratio=3:attack=8:release=160:makeup=2".into(),
            // Plus vite (ou plus lent) pour la hauteur, puis le tempo remis : la durée ne change pas
            Fx::Pitch { semitones } => {
                let k = 2f32.powf(semitones.clamp(-12.0, 12.0) / 12.0);
                format!("aresample={RATE},asetrate={:.0},aresample={RATE},atempo={:.6}", RATE as f32 * k, 1.0 / k)
            }
            // Trois échos qui s'éteignent : une pièce, une salle
            Fx::Reverb { amount } => {
                let a = amount.clamp(0.0, 1.0);
                format!("aecho=0.85:0.9:{:.0}|{:.0}|{:.0}:{:.3}|{:.3}|{:.3}", 30.0 + 40.0 * a, 55.0 + 80.0 * a, 85.0 + 130.0 * a, 0.12 + 0.3 * a, 0.08 + 0.22 * a, 0.05 + 0.16 * a)
            }
            Fx::Deess => "deesser=i=0.6:m=0.5:f=0.5".into(),
            Fx::Limiter { ceiling } => format!("alimiter=limit={:.4}:attack=5:release=50:level=disabled", db(ceiling.clamp(-24.0, 0.0))),
            Fx::Gate { threshold } => format!("agate=threshold={:.5}:ratio=8:attack=5:release=120", db(threshold.clamp(-80.0, -10.0))),
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Clip {
    /// Son dans le coffre
    pub src: String,
    /// Début sur la piste, en secondes
    pub start: f64,
    /// Partie du son : à partir de `offset`, pendant `len` secondes
    pub offset: f64,
    pub len: f64,
    #[serde(default)]
    pub gain: f32,
    #[serde(default)]
    pub fade_in: f32,
    #[serde(default)]
    pub fade_out: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fx: Vec<Fx>,
}

impl Clip {
    pub fn end(&self) -> f64 {
        self.start + self.len
    }

    /// Les réglages du rendu par ffmpeg (un clip sans effet partage le son brut)
    pub fn fx_key(&self) -> String {
        self.fx.iter().map(|f| f.filter()).collect::<Vec<_>>().join(",")
    }

    /// Le gain à l'instant `t` de la piste (fondus compris)
    pub fn env(&self, t: f64) -> f32 {
        let a = (t - self.start) as f32;
        let b = (self.end() - t) as f32;
        let mut g = db(self.gain);
        if self.fade_in > 0.0 && a < self.fade_in {
            g *= (a / self.fade_in).clamp(0.0, 1.0).powf(1.5);
        }
        if self.fade_out > 0.0 && b < self.fade_out {
            g *= (b / self.fade_out).clamp(0.0, 1.0).powf(1.5);
        }
        g
    }
}

fn yes() -> f32 {
    0.0
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Track {
    pub name: String,
    pub color: [u8; 3],
    /// Volume de la piste en dB
    #[serde(default = "yes")]
    pub volume: f32,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub solo: bool,
    pub clips: Vec<Clip>,
    /// Baisse automatique sous une autre piste (la musique sous la voix)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duck: Option<Duck>,
    /// Automation du volume : points (instant en s, gain en dB), reliés en ligne droite
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auto: Vec<[f64; 2]>,
}

impl Track {
    /// Gain de l'automation à l'instant `t`, en dB (0 sans points)
    pub fn auto_db(&self, t: f64) -> f64 {
        let a = &self.auto;
        match a.len() {
            0 => 0.0,
            _ if t <= a[0][0] => a[0][1],
            _ if t >= a[a.len() - 1][0] => a[a.len() - 1][1],
            _ => {
                let k = a.partition_point(|p| p[0] <= t);
                let (p, q) = (a[k - 1], a[k]);
                let u = (t - p[0]) / (q[0] - p[0]).max(1e-9);
                p[1] + (q[1] - p[1]) * u
            }
        }
    }
}

/// La piste baisse de `amount` dB quand la piste `under` joue
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Duck {
    pub under: usize,
    pub amount: f32,
    /// Remontée, en secondes
    #[serde(default = "release")]
    pub release: f32,
}

fn release() -> f32 {
    0.4
}

impl Duck {
    pub fn new(under: usize) -> Duck {
        Duck { under, amount: 12.0, release: release() }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Marker {
    pub t: f64,
    pub name: String,
}

pub const COLORS: [[u8; 3]; 6] = [[0xe3, 0x9b, 0x6f], [0x83, 0xb9, 0x9e], [0x8b, 0xa8, 0xc6], [0xcb, 0x9a, 0xb2], [0xd1, 0xaf, 0x75], [0xaf, 0x9c, 0xc9]];

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct Session {
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub markers: Vec<Marker>,
    /// Tempo de la session (grille en mesures, métronome, aimantation sur les temps)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tempo: Option<Tempo>,
}

/// Carte de tempo : un tempo de départ et des changements, chacun commence une mesure
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Tempo {
    pub bpm: f32,
    pub beats_per_bar: u32,
    #[serde(default)]
    pub changes: Vec<TempoChange>,
    /// Le clic du métronome à l'écoute (jamais dans l'export)
    #[serde(default)]
    pub click: bool,
    /// Les clips s'aimantent aux temps
    #[serde(default)]
    pub snap: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct TempoChange {
    pub t: f64,
    pub bpm: f32,
}

impl Default for Tempo {
    fn default() -> Tempo {
        Tempo { bpm: 120.0, beats_per_bar: 4, changes: vec![], click: true, snap: true }
    }
}

impl Tempo {
    /// Les segments de tempo : (début, bpm), triés
    fn segments(&self) -> Vec<(f64, f32)> {
        let mut v = vec![(0.0, self.bpm)];
        let mut ch: Vec<&TempoChange> = self.changes.iter().filter(|c| c.t > 0.0).collect();
        ch.sort_by(|a, b| a.t.total_cmp(&b.t));
        v.extend(ch.into_iter().map(|c| (c.t, c.bpm)));
        v
    }

    /// Les temps entre `a` et `b` : (instant, numéro de mesure si c'est le premier temps)
    pub fn beats(&self, a: f64, b: f64) -> Vec<(f64, Option<u32>)> {
        let seg = self.segments();
        let bpb = self.beats_per_bar.max(1) as u64;
        let mut out = vec![];
        let mut bar0 = 0u64;
        for (k, &(t0, bpm)) in seg.iter().enumerate() {
            let end = seg.get(k + 1).map(|s| s.0).unwrap_or(f64::INFINITY);
            let spb = 60.0 / bpm.clamp(20.0, 400.0) as f64;
            // Le premier temps visible du segment
            let first = if a > t0 { ((a - t0) / spb).ceil() as u64 } else { 0 };
            let mut i = first;
            loop {
                let t = t0 + i as f64 * spb;
                if t >= end - 1e-9 || t > b {
                    break;
                }
                out.push((t, (i % bpb == 0).then_some((bar0 + i / bpb) as u32 + 1)));
                i += 1;
            }
            if end.is_finite() {
                bar0 += ((end - t0) / spb / bpb as f64).ceil() as u64;
            }
        }
        out
    }

    /// Le tempo à l'instant `t`
    pub fn bpm_at(&self, t: f64) -> f32 {
        self.segments().into_iter().rev().find(|s| s.0 <= t).map(|s| s.1).unwrap_or(self.bpm)
    }
}

/// Ajoute le clic du métronome à un mixage stéréo qui commence à `a`
pub fn add_clicks(out: &mut [f32], tempo: &Tempo, a: f64) {
    let n = out.len() / 2;
    let b = a + n as f64 / RATE as f64;
    let len = (RATE as f64 * 0.035) as usize;
    for (t, bar) in tempo.beats(a, b) {
        let f = if bar.is_some() { 1760.0 } else { 1175.0 };
        let g = if bar.is_some() { 0.5 } else { 0.32 };
        let off = ((t - a) * RATE as f64).round() as usize;
        for k in 0..len {
            let i = off + k;
            if i >= n {
                break;
            }
            let x = k as f32 / RATE as f32;
            let v = (x * f * std::f32::consts::TAU).sin() * g * (-x * 140.0).exp();
            out[i * 2] += v;
            out[i * 2 + 1] += v;
        }
    }
}

impl Session {
    pub fn starter() -> Session {
        Session { tracks: vec![Session::new_track(0), Session::new_track(1)], markers: vec![], tempo: None }
    }

    pub fn new_track(i: usize) -> Track {
        Track { name: format!("{} {}", crate::i18n::t("Piste"), i + 1), color: COLORS[i % COLORS.len()], volume: 0.0, mute: false, solo: false, clips: vec![], duck: None, auto: vec![] }
    }

    pub fn read(path: &Path) -> Result<Session, String> {
        let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| format!("{} : {e}", crate::i18n::t("session audio illisible")))
    }

    pub fn write(&self, path: &Path) -> Result<(), String> {
        crate::vault::atomic_write(path, serde_json::to_string_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }

    pub fn duration(&self) -> f64 {
        self.tracks.iter().flat_map(|t| t.clips.iter().map(|c| c.end())).fold(0.0, f64::max)
    }

    /// Coupe les clips des pistes `tracks` à l'instant `t`
    pub fn split(&mut self, tracks: &[usize], t: f64) -> usize {
        let mut n = 0;
        for &ti in tracks {
            let Some(tr) = self.tracks.get_mut(ti) else { continue };
            let mut out = vec![];
            for c in tr.clips.drain(..) {
                if t > c.start + 0.005 && t < c.end() - 0.005 {
                    let mut a = c.clone();
                    let mut b = c;
                    a.len = t - a.start;
                    a.fade_out = 0.0;
                    b.offset += t - b.start;
                    b.len -= t - b.start;
                    b.start = t;
                    b.fade_in = 0.0;
                    out.push(a);
                    out.push(b);
                    n += 1;
                } else {
                    out.push(c);
                }
            }
            tr.clips = out;
        }
        n
    }

    /// Retire la plage [a, b) des pistes choisies ; avec `ripple`, ce qui suit se rapproche
    pub fn delete_range(&mut self, tracks: &[usize], a: f64, b: f64, ripple: bool) {
        if b <= a {
            return;
        }
        self.split(tracks, a);
        self.split(tracks, b);
        for &ti in tracks {
            let Some(tr) = self.tracks.get_mut(ti) else { continue };
            tr.clips.retain(|c| !(c.start >= a - 1e-6 && c.end() <= b + 1e-6));
            if ripple {
                for c in &mut tr.clips {
                    if c.start >= b - 1e-6 {
                        c.start -= b - a;
                    }
                }
            }
        }
    }

    /// Les pistes qu'on entend (solo, muet)
    /// Retire une piste ; les baisses qui la suivaient sont recalées
    pub fn remove_track(&mut self, ti: usize) {
        if ti >= self.tracks.len() {
            return;
        }
        self.tracks.remove(ti);
        for t in &mut self.tracks {
            match &mut t.duck {
                Some(d) if d.under == ti => t.duck = None,
                Some(d) if d.under > ti => d.under -= 1,
                _ => {}
            }
        }
    }

    pub fn audible(&self) -> Vec<usize> {
        let solo = self.tracks.iter().any(|t| t.solo);
        (0..self.tracks.len()).filter(|&i| if solo { self.tracks[i].solo } else { !self.tracks[i].mute }).collect()
    }
}

// ---------- Sons décodés ----------

/// Un son en stéréo entrelacée à 48 kHz, et sa forme d'onde (min, max par `PEAK_STEP` échantillons)
pub struct Buffer {
    pub data: Vec<f32>,
    pub peaks: Vec<(f32, f32)>,
}

impl Buffer {
    pub fn frames(&self) -> usize {
        self.data.len() / 2
    }

    pub fn secs(&self) -> f64 {
        self.frames() as f64 / RATE as f64
    }

    pub fn from_samples(data: Vec<f32>) -> Buffer {
        let peaks = data
            .chunks(PEAK_STEP * 2)
            .map(|c| {
                let (mut lo, mut hi) = (0.0f32, 0.0f32);
                for s in c.chunks_exact(2) {
                    let m = (s[0] + s[1]) * 0.5;
                    lo = lo.min(m);
                    hi = hi.max(m);
                }
                (lo, hi)
            })
            .collect();
        Buffer { data, peaks }
    }

    /// Crête (valeur absolue) entre deux instants du son
    pub fn peak(&self, a: f64, b: f64) -> f32 {
        let (i, j) = (((a * RATE as f64) as usize * 2).min(self.data.len()), ((b * RATE as f64) as usize * 2).min(self.data.len()));
        self.data[i..j.max(i)].iter().fold(0.0f32, |m, v| m.max(v.abs()))
    }
}

pub fn decode(path: &Path, filter: &str) -> Result<Buffer, String> {
    let ff = crate::ffmpeg::find("ffmpeg").ok_or(crate::i18n::t("ffmpeg n'est pas installé."))?;
    let mut cmd = crate::ffmpeg::command(ff);
    cmd.args(["-v", "error", "-i"]).arg(path);
    if !filter.is_empty() {
        cmd.args(["-af", filter]);
    }
    cmd.args(["-ac", "2", "-ar", &RATE.to_string(), "-f", "f32le", "-"]).stdin(std::process::Stdio::null());
    let o = cmd.output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("ffmpeg").to_string());
    }
    Ok(Buffer::from_samples(o.stdout.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()))
}

#[derive(Clone)]
pub enum Slot {
    Loading,
    Ready(Arc<Buffer>),
    Failed(String),
}

/// Les sons décodés, par (son, effets) ; le décodage se fait en arrière-plan
#[derive(Clone, Default)]
pub struct Sounds {
    map: Arc<Mutex<HashMap<(String, String), Slot>>>,
    root: String,
}

impl Sounds {
    pub fn new(root: &str) -> Sounds {
        Sounds { map: Default::default(), root: root.to_string() }
    }

    /// Le son demandé ; lance son décodage s'il n'est pas encore là
    pub fn get(&self, src: &str, fx: &str, ctx: Option<&eframe::egui::Context>) -> Slot {
        let key = (src.to_string(), fx.to_string());
        let mut m = self.map.lock().unwrap();
        if let Some(s) = m.get(&key) {
            return s.clone();
        }
        m.insert(key.clone(), Slot::Loading);
        let map = self.map.clone();
        let path = crate::vault::safe(&self.root, src);
        let ctx = ctx.cloned();
        std::thread::spawn(move || {
            let r = path.and_then(|p| decode(&p, &key.1));
            map.lock().unwrap().insert(key, match r {
                Ok(b) => Slot::Ready(Arc::new(b)),
                Err(e) => Slot::Failed(e),
            });
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
        Slot::Loading
    }

    pub fn ready(&self, c: &Clip) -> Option<Arc<Buffer>> {
        match self.get(&c.src, &c.fx_key(), None) {
            Slot::Ready(b) => Some(b),
            _ => None,
        }
    }

    /// Attend qu'un son soit décodé (export, tests)
    pub fn wait(&self, c: &Clip) -> Option<Arc<Buffer>> {
        loop {
            match self.get(&c.src, &c.fx_key(), None) {
                Slot::Ready(b) => return Some(b),
                Slot::Failed(_) => return None,
                Slot::Loading => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
    }

    #[cfg(test)]
    pub fn put(&self, src: &str, b: Buffer) {
        self.map.lock().unwrap().insert((src.to_string(), String::new()), Slot::Ready(Arc::new(b)));
    }
}

/// Le mixage stéréo des pistes audibles entre `a` et `b` secondes. `wait` attend les sons pas encore décodés.
/// Entrée micro pour ffmpeg selon le système : (format, périphérique)
pub fn mic_input() -> Option<(String, String)> {
    if cfg!(target_os = "windows") {
        // Le premier périphérique audio que DirectShow annonce
        let ff = crate::ffmpeg::find("ffmpeg")?;
        let out = crate::ffmpeg::command(ff).args(["-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"]).output().ok()?;
        let text = String::from_utf8_lossy(&out.stderr).to_string();
        let name = text.lines().find(|l| l.contains("(audio)")).and_then(|l| l.split('"').nth(1))?.to_string();
        Some(("dshow".into(), format!("audio={name}")))
    } else if cfg!(target_os = "macos") {
        Some(("avfoundation".into(), ":0".into()))
    } else {
        Some(("pulse".into(), "default".into()))
    }
}

/// Une prise en cours : ffmpeg écrit le WAV ; « q » sur son entrée l'arrête proprement
pub struct Recording {
    pub child: std::process::Child,
    pub out: PathBuf,
    pub started: std::time::Instant,
    /// Instant de la session où la prise commence, et sa piste
    pub at: f64,
    pub track: usize,
}

pub fn start_recording(out: &Path, at: f64, track: usize) -> Result<Recording, String> {
    let ff = crate::ffmpeg::find("ffmpeg").ok_or("ffmpeg n'est pas installé.")?;
    let (fmt, dev) = mic_input().ok_or("Aucun micro trouvé.")?;
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let child = crate::ffmpeg::command(ff)
        .args(["-v", "error", "-y", "-f", &fmt, "-i", &dev, "-ac", "2", "-ar", &RATE.to_string(), "-c:a", "pcm_s16le"])
        .arg(out)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(Recording { child, out: out.to_path_buf(), started: std::time::Instant::now(), at, track })
}

impl Recording {
    /// Arrête la prise ; Err si ffmpeg n'a rien pu enregistrer
    pub fn stop(mut self) -> Result<PathBuf, String> {
        use std::io::Write;
        if let Some(mut i) = self.child.stdin.take() {
            let _ = i.write_all(b"q");
        }
        let start = std::time::Instant::now();
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                _ if start.elapsed().as_secs() > 4 => {
                    let _ = self.child.kill();
                    break;
                }
                _ => std::thread::sleep(std::time::Duration::from_millis(30)),
            }
        }
        if std::fs::metadata(&self.out).map(|m| m.len() > 100).unwrap_or(false) {
            Ok(self.out)
        } else {
            let mut err = String::new();
            if let Some(mut e) = self.child.stderr.take() {
                use std::io::Read;
                let _ = e.read_to_string(&mut err);
            }
            Err(format!("{} {}", crate::i18n::t("Enregistrement impossible :"), err.lines().last().unwrap_or("micro introuvable")))
        }
    }

    /// Ffmpeg s'est arrêté tout seul (micro absent, refusé)
    pub fn failed(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

pub fn mix(s: &Session, sounds: &Sounds, a: f64, b: f64, wait: bool) -> Vec<f32> {
    let n = (((b - a) * RATE as f64).max(0.0)) as usize;
    let mut out = vec![0.0f32; n * 2];
    for ti in s.audible() {
        let gains = duck_gains(s, sounds, ti, a, n, wait);
        add_track(&mut out, s, sounds, ti, a, n, wait, gains.as_deref());
    }
    out
}

/// Exporte une piste par fichier (stems) : WAV 24 bits, chacun sur toute la durée de la session (ils se
/// recalent tels quels), avec le volume, les enveloppes et le ducking du mixage. Rend les fichiers écrits.
pub fn export_stems(s: &Session, sounds: &Sounds, dir: &Path) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let n = (s.duration() * RATE as f64) as usize;
    let mut out = vec![];
    for (k, ti) in s.audible().into_iter().enumerate() {
        if s.tracks[ti].clips.is_empty() {
            continue;
        }
        let mut data = vec![0.0f32; n * 2];
        let gains = duck_gains(s, sounds, ti, 0.0, n, true);
        add_track(&mut data, s, sounds, ti, 0.0, n, true, gains.as_deref());
        let name: String = s.tracks[ti].name.chars().map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c }).collect();
        let path = dir.join(format!("{:02} {}.wav", k + 1, name.trim()));
        std::fs::write(&path, wav_bytes(&data)).map_err(|e| e.to_string())?;
        out.push(path);
    }
    if out.is_empty() {
        return Err(crate::i18n::t("Aucune piste à exporter."));
    }
    Ok(out)
}

/// Ajoute une piste au mélange, avec un gain par échantillon en plus (ducking)
#[allow(clippy::too_many_arguments)]
fn add_track(out: &mut [f32], s: &Session, sounds: &Sounds, ti: usize, a: f64, n: usize, wait: bool, gains: Option<&[f32]>) {
    let tr = &s.tracks[ti];
    let tv = db(tr.volume);
    let b = a + n as f64 / RATE as f64;
    for c in &tr.clips {
        if c.end() <= a || c.start >= b {
            continue;
        }
        let Some(buf) = (if wait { sounds.wait(c) } else { sounds.ready(c) }) else { continue };
        let from = ((c.start.max(a) - a) * RATE as f64) as usize;
        let to = (((c.end().min(b)) - a) * RATE as f64) as usize;
        for f in from..to.min(n) {
            let t = a + f as f64 / RATE as f64;
            let si = ((t - c.start + c.offset) * RATE as f64) as usize;
            if si >= buf.frames() {
                break;
            }
            let auto = if tr.auto.is_empty() { 1.0 } else { db(tr.auto_db(t) as f32) };
            let g = c.env(t) * tv * auto * gains.map_or(1.0, |g| g[f]);
            out[f * 2] += buf.data[si * 2] * g;
            out[f * 2 + 1] += buf.data[si * 2 + 1] * g;
        }
    }
}

/// Seuil au-dessus duquel la piste de référence « parle » (environ -36 dBFS)
const DUCK_THRESHOLD: f32 = 0.016;

/// Gains du ducking de la piste `ti` sur [a, a + n) : suit le niveau de la piste de référence,
/// descend en 30 ms et remonte en `release`. On commence une seconde plus tôt pour que l'état soit juste.
pub fn duck_gains(s: &Session, sounds: &Sounds, ti: usize, a: f64, n: usize, wait: bool) -> Option<Vec<f32>> {
    let d = s.tracks.get(ti)?.duck.as_ref()?;
    let key = s.tracks.get(d.under)?;
    if d.under == ti || key.mute || d.amount <= 0.0 {
        return None;
    }
    let pre = (RATE as usize).min((a.max(0.0) * RATE as f64) as usize);
    let total = pre + n;
    let a0 = a - pre as f64 / RATE as f64;
    let mut sig = vec![0.0f32; total * 2];
    add_track(&mut sig, s, sounds, d.under, a0, total, wait, None);
    let low = db(-d.amount);
    let (att, rel) = (1.0 - (-1.0 / (0.03 * RATE as f32)).exp(), 1.0 - (-1.0 / (d.release.max(0.02) * RATE as f32)).exp());
    let det_rel = 1.0 - (-1.0 / (0.12 * RATE as f32)).exp();
    let (mut level, mut g) = (0.0f32, 1.0f32);
    let mut out = Vec::with_capacity(n);
    for f in 0..total {
        let x = sig[f * 2].abs().max(sig[f * 2 + 1].abs());
        // Détecteur de crête : monte tout de suite, retombe en 120 ms
        level = if x > level { x } else { level + (x - level) * det_rel };
        let target = if level > DUCK_THRESHOLD { low } else { 1.0 };
        g += (target - g) * if target < g { att } else { rel };
        if f >= pre {
            out.push(g);
        }
    }
    Some(out)
}

/// Courbe de ducking pour l'affichage : `steps` gains de a à b
pub fn duck_curve(s: &Session, sounds: &Sounds, ti: usize, a: f64, b: f64, steps: usize) -> Option<Vec<f32>> {
    let n = (((b - a) * RATE as f64).max(0.0)) as usize;
    let g = duck_gains(s, sounds, ti, a, n, false)?;
    Some((0..steps).map(|k| g.get(k * n / steps.max(1)).copied().unwrap_or(1.0)).collect())
}

/// Spectre moyen d'un passage (en secondes du son) : `bands` valeurs en dB sur une échelle de fréquences
/// logarithmique de 20 Hz à 20 kHz, calculées par FFT de 4096 points avec fenêtre de Hann
pub fn spectrum(buf: &Buffer, a: f64, b: f64, bands: usize) -> Vec<f32> {
    const N: usize = 4096;
    let (i0, i1) = (((a * RATE as f64) as usize).min(buf.frames()), ((b * RATE as f64) as usize).min(buf.frames()));
    let mut power = vec![0.0f64; N / 2];
    let mut count = 0;
    let hann: Vec<f64> = (0..N).map(|k| 0.5 - 0.5 * (std::f64::consts::TAU * k as f64 / N as f64).cos()).collect();
    // Au plus 64 fenêtres réparties sur le passage
    let windows = ((i1.saturating_sub(i0)) / N).clamp(1, 64);
    let step = ((i1.saturating_sub(i0 + N)) / windows.max(1)).max(N / 2);
    let mut start = i0;
    while start + N <= i1.max(i0 + N).min(buf.frames()) && count < 64 {
        let mut re: Vec<f64> = (0..N).map(|k| ((buf.data[(start + k) * 2] + buf.data[(start + k) * 2 + 1]) * 0.5) as f64 * hann[k]).collect();
        let mut im = vec![0.0f64; N];
        fft(&mut re, &mut im);
        for k in 0..N / 2 {
            power[k] += re[k] * re[k] + im[k] * im[k];
        }
        count += 1;
        start += step;
    }
    if count == 0 {
        return vec![-120.0; bands];
    }
    let bin_hz = RATE as f64 / N as f64;
    (0..bands)
        .map(|j| {
            let f0 = 20.0 * 1000f64.powf(j as f64 / bands as f64);
            let f1 = 20.0 * 1000f64.powf((j + 1) as f64 / bands as f64);
            let (k0, k1) = ((f0 / bin_hz) as usize, ((f1 / bin_hz).ceil() as usize).max(f0 as usize / bin_hz as usize + 1).min(N / 2));
            let k0 = k0.min(k1.saturating_sub(1));
            let p = power[k0..k1].iter().cloned().fold(0.0, f64::max) / count as f64;
            // Normalisé : un sinus plein donne 0 dB
            (10.0 * (p.max(1e-20) / (N as f64 * N as f64 / 16.0)).log10()) as f32
        })
        .collect()
}

/// FFT en place (Cooley-Tukey, taille puissance de deux)
fn fft(re: &mut [f64], im: &mut [f64]) {
    fft_dir(re, im, false);
}

/// FFT en place, directe ou inverse (l'inverse est normalisée par 1/n). Les cosinus et sinus sont calculés
/// une seule fois pour toute la transformée, pas à chaque papillon.
pub(crate) fn fft_dir(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    if n < 2 || !n.is_power_of_two() || im.len() != n {
        return;
    }
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let sign = if inverse { 1.0 } else { -1.0 };
    let (wr, wi): (Vec<f64>, Vec<f64>) = (0..n / 2).map(|k| { let a = sign * std::f64::consts::TAU * k as f64 / n as f64; (a.cos(), a.sin()) }).unzip();
    let mut len = 2;
    while len <= n {
        let (half, stride) = (len / 2, n / len);
        for s in (0..n).step_by(len) {
            for k in 0..half {
                let (c, d) = (wr[k * stride], wi[k * stride]);
                let (a, b) = (s + k, s + k + half);
                let (xr, xi) = (re[b] * c - im[b] * d, re[b] * d + im[b] * c);
                re[b] = re[a] - xr;
                im[b] = im[a] - xi;
                re[a] += xr;
                im[a] += xi;
            }
        }
        len <<= 1;
    }
    if inverse {
        let k = 1.0 / n as f64;
        re.iter_mut().chain(im.iter_mut()).for_each(|v| *v *= k);
    }
}

/// Les passages de silence d'un son entre deux instants (sous `floor_db` pendant au moins `min_len` s), en secondes du son
pub fn silences(buf: &Buffer, a: f64, b: f64, floor_db: f32, min_len: f64) -> Vec<(f64, f64)> {
    let win = (RATE as f64 * 0.02) as usize;
    let (i0, i1) = ((a * RATE as f64) as usize, ((b * RATE as f64) as usize).min(buf.frames()));
    let floor = db(floor_db);
    let mut out = vec![];
    let mut start: Option<usize> = None;
    let mut i = i0;
    while i < i1 {
        let j = (i + win).min(i1);
        let rms = (buf.data[i * 2..j * 2].iter().map(|v| v * v).sum::<f32>() / ((j - i) * 2).max(1) as f32).sqrt();
        if rms < floor {
            start.get_or_insert(i);
        } else if let Some(s) = start.take() {
            out.push((s, i));
        }
        i = j;
    }
    if let Some(s) = start {
        out.push((s, i1));
    }
    out.into_iter().map(|(s, e)| (s as f64 / RATE as f64, e as f64 / RATE as f64)).filter(|(s, e)| e - s >= min_len).collect()
}

/// Retire les silences d'un clip (on garde un peu d'air autour) et resserre la piste ; rend le temps gagné
pub fn cut_silences(s: &mut Session, ti: usize, ci: usize, buf: &Buffer) -> f64 {
    let Some(c) = s.tracks.get(ti).and_then(|t| t.clips.get(ci)).cloned() else { return 0.0 };
    let pad = 0.12;
    let gaps: Vec<(f64, f64)> = silences(buf, c.offset, c.offset + c.len, -42.0, 0.45)
        .into_iter()
        .map(|(a, b)| (c.start + (a - c.offset) + pad, c.start + (b - c.offset) - pad))
        // Le silence du tout début ou de la toute fin garde aussi son air
        .filter(|(a, b)| b - a > 0.05)
        .collect();
    let mut saved = 0.0;
    // De la fin vers le début : les instants restent justes
    for (a, b) in gaps.into_iter().rev() {
        s.delete_range(&[ti], a, b, true);
        saved += b - a;
    }
    saved
}

/// Sonie intégrée (LUFS, UIT-R BS.1770 / EBU R128) d'un son stéréo entrelacé à 48 kHz : filtre de pondération K,
/// blocs de 400 ms qui se chevauchent de 75 %, portes absolue (-70 LUFS) et relative (-10 LU). None : silence.
pub fn lufs(data: &[f32]) -> Option<f32> {
    // Pondération K à 48 kHz : plateau des aigus puis coupe-bas (coefficients de la norme)
    const S: ([f64; 3], [f64; 2]) = ([1.53512485958697, -2.69169618940638, 1.19839281085285], [-1.69065929318241, 0.73248077421585]);
    const H: ([f64; 3], [f64; 2]) = ([1.0, -2.0, 1.0], [-1.99004745483398, 0.99007225036621]);
    let frames = data.len() / 2;
    let mut sq = vec![0.0f64; frames];
    for ch in 0..2 {
        let mut st = [[0.0f64; 4]; 2];
        for i in 0..frames {
            let mut x = data[i * 2 + ch] as f64;
            for (k, (b, a)) in [S, H].iter().enumerate() {
                let z = &mut st[k];
                let y = b[0] * x + b[1] * z[0] + b[2] * z[1] - a[0] * z[2] - a[1] * z[3];
                z[1] = z[0];
                z[0] = x;
                z[3] = z[2];
                z[2] = y;
                x = y;
            }
            sq[i] += x * x;
        }
    }
    let (block, step) = ((RATE as usize * 4) / 10, RATE as usize / 10);
    if frames < block {
        return None;
    }
    // Somme glissante : puissance moyenne de chaque bloc
    let mut pre = vec![0.0f64; frames + 1];
    for i in 0..frames {
        pre[i + 1] = pre[i] + sq[i];
    }
    let z: Vec<f64> = (0..=(frames - block) / step).map(|b| (pre[b * step + block] - pre[b * step]) / block as f64).collect();
    let loud = |m: f64| -0.691 + 10.0 * m.max(1e-20).log10();
    let abs: Vec<f64> = z.iter().copied().filter(|&m| loud(m) > -70.0).collect();
    if abs.is_empty() {
        return None;
    }
    let rel = loud(abs.iter().sum::<f64>() / abs.len() as f64) - 10.0;
    let gated: Vec<f64> = abs.into_iter().filter(|&m| loud(m) > rel).collect();
    Some(loud(gated.iter().sum::<f64>() / gated.len().max(1) as f64) as f32)
}

/// Cibles de sonie courantes
pub const LOUDNESS: [(f32, &str); 4] = [(-14.0, "YouTube, Spotify"), (-16.0, "Apple Music, podcasts"), (-23.0, "Télé (EBU R128)"), (-19.0, "Podcast en mono")];

/// Le gain qui amène la sonie du clip à `target` LUFS, sans que sa crête dépasse -1 dB.
/// Retourne aussi la sonie mesurée, et si la crête a limité le gain.
/// Crête vraie (dBTP) d'un son stéréo entrelacé : les échantillons suréchantillonnés 4× (interpolation
/// cubique), pour attraper les crêtes qui tombent entre deux échantillons
pub fn true_peak_db(data: &[f32]) -> f32 {
    let mut pk = 0.0f32;
    for ch in 0..2 {
        let x: Vec<f32> = data.iter().skip(ch).step_by(2).copied().collect();
        for i in 1..x.len().saturating_sub(2) {
            let (p0, p1, p2, p3) = (x[i - 1], x[i], x[i + 1], x[i + 2]);
            pk = pk.max(p1.abs());
            for k in 1..4 {
                let t = k as f32 / 4.0;
                // Catmull-Rom
                let v = 0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t * t * t);
                pk = pk.max(v.abs());
            }
        }
    }
    to_db(pk)
}

/// Corrélation stéréo de -1 (opposition de phase) à 1 (mono) ; 0 : très large
pub fn correlation(data: &[f32]) -> f32 {
    let (mut lr, mut ll, mut rr) = (0.0f64, 0.0f64, 0.0f64);
    for f in data.chunks_exact(2) {
        let (l, r) = (f[0] as f64, f[1] as f64);
        lr += l * r;
        ll += l * l;
        rr += r * r;
    }
    if ll * rr <= 1e-12 {
        return 1.0;
    }
    (lr / (ll * rr).sqrt()) as f32
}

pub fn loudness_gain(c: &Clip, buf: &Buffer, target: f32) -> Option<(f32, f32, bool)> {
    let (i, j) = (((c.offset * RATE as f64) as usize * 2).min(buf.data.len()), (((c.offset + c.len) * RATE as f64) as usize * 2).min(buf.data.len()));
    let l = lufs(&buf.data[i..j.max(i)])?;
    let want = target - l;
    let pk = buf.peak(c.offset, c.offset + c.len);
    let ceiling = if pk > 1e-5 { -1.0 - to_db(pk) } else { want };
    Some((want.min(ceiling), l, want > ceiling + 0.05))
}

/// Le gain qui amène la crête du clip à `target` dB
pub fn normalize_gain(c: &Clip, buf: &Buffer, target: f32) -> f32 {
    let pk = buf.peak(c.offset, c.offset + c.len);
    if pk <= 1e-5 {
        return c.gain;
    }
    target - to_db(pk)
}

/// Exporte le mixage en WAV 24 bits, ou dans un autre format par ffmpeg (selon l'extension)
/// Les régions de la session : d'un repère au suivant (le dernier va jusqu'à la fin), nommées d'après le repère
pub fn regions(s: &Session) -> Vec<(String, f64, f64)> {
    let end = s.duration();
    let mut m: Vec<&Marker> = s.markers.iter().filter(|m| m.t < end).collect();
    m.sort_by(|a, b| a.t.total_cmp(&b.t));
    (0..m.len()).map(|k| (m[k].name.clone(), m[k].t, m.get(k + 1).map(|n| n.t).unwrap_or(end))).filter(|r| r.2 - r.1 > 0.01).collect()
}

/// Un WAV par région, dans `dir`
pub fn export_regions(s: &Session, sounds: &Sounds, dir: &Path) -> Result<Vec<PathBuf>, String> {
    let regs = regions(s);
    if regs.is_empty() {
        return Err(crate::i18n::t("Pose des repères : chaque région va d'un repère au suivant."));
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut out = vec![];
    for (k, (name, a, b)) in regs.iter().enumerate() {
        let data = mix(s, sounds, *a, *b, true);
        let clean: String = name.chars().map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c }).collect();
        let path = dir.join(format!("{:02} {}.wav", k + 1, clean.trim()));
        std::fs::write(&path, wav_bytes(&data)).map_err(|e| e.to_string())?;
        out.push(path);
    }
    Ok(out)
}

pub fn export(s: &Session, sounds: &Sounds, out: &Path) -> Result<(), String> {
    let data = mix(s, sounds, 0.0, s.duration(), true);
    let wav = wav_bytes(&data);
    if out.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")) {
        return std::fs::write(out, wav).map_err(|e| e.to_string());
    }
    use std::io::Write;
    let ff = crate::ffmpeg::find("ffmpeg").ok_or(crate::i18n::t("ffmpeg n'est pas installé."))?;
    let mut child = crate::ffmpeg::command(ff)
        .args(["-v", "error", "-y", "-f", "wav", "-i", "-", "-b:a", "256k"])
        .arg(out)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    child.stdin.take().ok_or("ffmpeg")?.write_all(&wav).map_err(|e| e.to_string())?;
    let o = child.wait_with_output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("ffmpeg").to_string());
    }
    Ok(())
}

/// Un WAV 24 bits stéréo à 48 kHz
pub fn wav_bytes(data: &[f32]) -> Vec<u8> {
    let n = data.len() as u32 * 3;
    let mut v = Vec::with_capacity(44 + n as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + n).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&RATE.to_le_bytes());
    v.extend_from_slice(&(RATE * 6).to_le_bytes());
    v.extend_from_slice(&6u16.to_le_bytes());
    v.extend_from_slice(&24u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&n.to_le_bytes());
    for s in data {
        let i = (s.clamp(-1.0, 1.0) * 8_388_607.0) as i32;
        v.extend_from_slice(&i.to_le_bytes()[..3]);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(secs: f64, amp: f32, silent: &[(f64, f64)]) -> Buffer {
        let n = (secs * RATE as f64) as usize;
        let mut d = Vec::with_capacity(n * 2);
        for i in 0..n {
            let t = i as f64 / RATE as f64;
            let v = if silent.iter().any(|(a, b)| t >= *a && t < *b) { 0.0 } else { amp * (t as f32 * 440.0 * std::f32::consts::TAU).sin() };
            d.push(v);
            d.push(v);
        }
        Buffer::from_samples(d)
    }

    #[test]
    fn fft_aller_retour() {
        let src: Vec<f64> = (0..1024).map(|i| ((i * 37 % 101) as f64 / 50.0 - 1.0)).collect();
        let (mut re, mut im) = (src.clone(), vec![0.0; 1024]);
        fft_dir(&mut re, &mut im, false);
        fft_dir(&mut re, &mut im, true);
        assert!(re.iter().zip(&src).all(|(a, b)| (a - b).abs() < 1e-9));
        assert!(im.iter().all(|v| v.abs() < 1e-9));
        // Taille qui n'est pas une puissance de deux : rien ne bouge, pas de panique
        let (mut r3, mut i3) = (vec![1.0, 2.0, 3.0], vec![0.0; 3]);
        fft_dir(&mut r3, &mut i3, false);
        assert_eq!(r3, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn spectre_d_un_la() {
        // Un la 440 Hz : le pic du spectre tombe dans la bande qui contient 440 Hz
        let b = tone(1.0, 0.5, &[]);
        let sp = spectrum(&b, 0.0, 1.0, 120);
        let (best, _) = sp.iter().enumerate().fold((0, f32::MIN), |m, (i, v)| if *v > m.1 { (i, *v) } else { m });
        let f = |j: usize| 20.0 * 1000f64.powf(j as f64 / 120.0);
        assert!(f(best) <= 440.0 && f(best + 1) >= 430.0, "pic à {} Hz", f(best));
        assert!(sp[best] - sp[110] > 40.0, "les aigus sont loin dessous");
    }

    #[test]
    fn la_musique_baisse_sous_la_voix() {
        let sounds = Sounds::new("/nulle-part");
        // Voix de 2 à 4 s, musique tout le long
        sounds.put("voix.wav", tone(2.0, 0.5, &[]));
        sounds.put("musique.wav", tone(6.0, 0.5, &[]));
        let mut s = Session::starter();
        s.tracks[0].clips.push(Clip { src: "voix.wav".into(), start: 2.0, offset: 0.0, len: 2.0, gain: 0.0, fade_in: 0.0, fade_out: 0.0, fx: vec![] });
        s.tracks[1].clips.push(Clip { src: "musique.wav".into(), start: 0.0, offset: 0.0, len: 6.0, gain: 0.0, fade_in: 0.0, fade_out: 0.0, fx: vec![] });
        s.tracks[1].duck = Some(Duck::new(0));
        let g = duck_curve(&s, &sounds, 1, 0.0, 6.0, 60).unwrap();
        assert!(g[10] > 0.99, "avant la voix : plein volume");
        assert!((g[30] - db(-12.0)).abs() < 0.02, "pendant : -12 dB ({})", g[30]);
        assert!(g[55] > 0.95, "après : remonté ({})", g[55]);
        // Le calcul par morceaux (lecture) donne le même gain qu'en une fois
        let n = RATE as usize / 2;
        let part = duck_gains(&s, &sounds, 1, 3.0, n, false).unwrap();
        let whole = duck_gains(&s, &sounds, 1, 0.0, 6 * RATE as usize, false).unwrap();
        assert!((part[100] - whole[3 * RATE as usize + 100]).abs() < 0.01);
        // Supprimer la voix retire la baisse
        s.remove_track(0);
        assert!(s.tracks[0].duck.is_none());
    }

    #[test]
    fn couper_mixer_et_silences() {
        let sounds = Sounds::new("/nulle-part");
        sounds.put("voix.wav", tone(6.0, 0.5, &[(2.0, 3.0)]));
        let mut s = Session::starter();
        s.tracks[0].clips.push(Clip { src: "voix.wav".into(), start: 1.0, offset: 0.0, len: 6.0, gain: 0.0, fade_in: 0.0, fade_out: 0.0, fx: vec![] });
        assert!((s.duration() - 7.0).abs() < 1e-9);
        // Le mixage commence au début du clip
        let m = mix(&s, &sounds, 0.0, 2.0, false);
        assert!(m[..RATE as usize * 2 - 200].iter().all(|v| *v == 0.0));
        assert!(m[RATE as usize * 2 + 200..].iter().any(|v| v.abs() > 0.4));
        // La hauteur garde la durée : asetrate × k puis atempo ÷ k
        let f = Fx::Pitch { semitones: 12.0 }.filter();
        assert!(f.contains("asetrate=96000") && f.contains("atempo=0.5"), "{f}");
        assert!(Fx::Reverb { amount: 0.5 }.filter().starts_with("aecho="));
        // Sonie : un sinus de 997 Hz à pleine échelle sur les deux canaux vaut 0 LUFS, à -20 dB il vaut -20
        let sine = |amp: f32| -> Vec<f32> { (0..RATE as usize * 3).flat_map(|i| { let v = amp * (i as f32 * 997.0 * std::f32::consts::TAU / RATE as f32).sin(); [v, v] }).collect() };
        assert!(lufs(&sine(1.0)).unwrap().abs() < 0.1, "{:?}", lufs(&sine(1.0)));
        assert!((lufs(&sine(0.1)).unwrap() + 20.0).abs() < 0.1);
        assert!(lufs(&vec![0.0; 96_000]).is_none());
        // Normaliser à -1 dB : 0,5 de crête → +5 dB environ
        let buf = sounds.ready(&s.tracks[0].clips[0]).unwrap();
        let g = normalize_gain(&s.tracks[0].clips[0], &buf, -1.0);
        assert!((g - 5.02).abs() < 0.1, "{g}");
        // Le silence d'une seconde (2 à 3 s du son) part ; la piste se resserre
        let saved = cut_silences(&mut s, 0, 0, &buf);
        assert!((saved - 0.76).abs() < 0.05, "{saved}");
        assert_eq!(s.tracks[0].clips.len(), 2);
        assert!((s.duration() - (7.0 - saved)).abs() < 1e-6);
        // Coupe et suppression d'une plage avec resserrement
        let mut s2 = Session::starter();
        s2.tracks[0].clips.push(Clip { src: "voix.wav".into(), start: 0.0, offset: 0.0, len: 6.0, gain: 0.0, fade_in: 0.5, fade_out: 0.5, fx: vec![] });
        assert_eq!(s2.split(&[0], 2.0), 1);
        assert_eq!(s2.tracks[0].clips[1].offset, 2.0);
        s2.delete_range(&[0], 1.0, 3.0, true);
        assert!((s2.duration() - 4.0).abs() < 1e-9);
        let back: Session = serde_json::from_str(&serde_json::to_string(&s2).unwrap()).unwrap();
        assert_eq!(back, s2);
        assert_eq!(&wav_bytes(&[0.0, 0.0])[..4], b"RIFF");
    }
}

/// Chaînes d'effets réutilisables : un fichier .arychain par chaîne, dans « sons/chaînes/ » du coffre
pub mod chains {
    use super::Fx;

    pub fn dir(root: &str) -> std::path::PathBuf {
        std::path::Path::new(root).join("sons").join("chaînes")
    }

    pub fn list(root: &str) -> Vec<(String, std::path::PathBuf)> {
        let mut v: Vec<(String, std::path::PathBuf)> = std::fs::read_dir(dir(root))
            .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "arychain")).map(|p| (p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), p)).collect())
            .unwrap_or_default();
        v.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        v
    }

    pub fn save(root: &str, name: &str, fx: &[Fx]) -> Result<(), String> {
        let clean: String = name.chars().filter(|c| !"/\\:*?\"<>|".contains(*c)).collect();
        let clean = if clean.trim().is_empty() { "chaîne".to_string() } else { clean.trim().to_string() };
        std::fs::create_dir_all(dir(root)).map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(fx).map_err(|e| e.to_string())?;
        crate::vault::atomic_write(&dir(root).join(format!("{clean}.arychain")), json).map_err(|e| e.to_string())
    }

    pub fn load(path: &std::path::Path) -> Option<Vec<Fx>> {
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn chaines() {
            let root = std::env::temp_dir().join(format!("ary-chains-{}", std::process::id()));
            let r = root.to_string_lossy().to_string();
            let fx = vec![super::Fx::Highpass { hz: 90.0 }, super::Fx::Compress, super::Fx::Limiter { ceiling: -1.0 }];
            super::save(&r, "Voix podcast", &fx).unwrap();
            let l = super::list(&r);
            assert_eq!(l[0].0, "Voix podcast");
            assert_eq!(super::load(&l[0].1).unwrap(), fx);
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

#[cfg(test)]
mod meter_tests {
    #[test]
    fn crete_vraie_et_correlation() {
        // Un sinus à fs/4 décalé de 45° : les échantillons valent ±0,707, la vraie crête est 1
        let n = 4800;
        let data: Vec<f32> = (0..n).flat_map(|i| {
            let v = (std::f32::consts::FRAC_PI_2 * i as f32 + std::f32::consts::FRAC_PI_4).sin();
            [v, v]
        }).collect();
        let sample_peak = data.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(sample_peak < 0.75);
        assert!(super::true_peak_db(&data) > -1.5, "{}", super::true_peak_db(&data));
        assert!((super::correlation(&data) - 1.0).abs() < 1e-4);
        let anti: Vec<f32> = data.chunks(2).flat_map(|f| [f[0], -f[1]]).collect();
        assert!((super::correlation(&anti) + 1.0).abs() < 1e-4);
    }
}

#[cfg(test)]
mod region_tests {
    #[test]
    fn regions_entre_reperes() {
        let mut s = super::Session::starter();
        s.tracks[0].clips.push(super::Clip { src: "x.wav".into(), start: 0.0, offset: 0.0, len: 10.0, gain: 0.0, fade_in: 0.0, fade_out: 0.0, fx: vec![] });
        s.markers = vec![super::Marker { t: 6.0, name: "Pas".into() }, super::Marker { t: 1.0, name: "Porte".into() }];
        let r = super::regions(&s);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0], ("Porte".to_string(), 1.0, 6.0));
        assert!((r[1].2 - s.duration()).abs() < 1e-6);
    }
}

#[cfg(test)]
mod tempo_tests {
    use super::*;
    #[test]
    fn carte_de_tempo() {
        let mut t = Tempo { bpm: 120.0, beats_per_bar: 4, ..Tempo::default() };
        let b = t.beats(0.0, 4.0);
        // 120 bpm : un temps par demi-seconde, mesures 1 et 2 sur 0 s et 2 s
        assert_eq!(b.len(), 9);
        assert_eq!(b[0], (0.0, Some(1)));
        assert_eq!(b[4], (2.0, Some(2)));
        assert_eq!(b[1].1, None);
        // Passage à 60 bpm à 4 s : la mesure 3 y commence, puis un temps par seconde
        t.changes.push(TempoChange { t: 4.0, bpm: 60.0 });
        let b = t.beats(3.9, 9.0);
        assert_eq!(b[0], (4.0, Some(3)));
        assert_eq!(b[1].0, 5.0);
        assert_eq!(b[4], (8.0, Some(4)));
        assert_eq!(t.bpm_at(5.0), 60.0);
        let mut out = vec![0.0f32; RATE as usize * 2];
        add_clicks(&mut out, &t, 0.0);
        assert!(out[10].abs() > 0.01 && out[RATE as usize / 2 * 2 + 10].abs() > 0.01);
    }
}
