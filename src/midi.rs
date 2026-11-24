//! Fichiers MIDI standard (.mid) : lecture en pistes de notes pour le séquenceur, et écriture d'un morceau.
//! Formats 0 et 1, notes et tempo ; le reste (contrôleurs, paroles…) est ignoré.

use crate::music::{Inst, MTrack, Note, Region, Song};

pub fn is_midi(rel: &str) -> bool {
    let l = rel.to_lowercase();
    l.ends_with(".mid") || l.ends_with(".midi")
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn u8(&mut self) -> Result<u8, String> {
        let v = *self.b.get(self.i).ok_or("MIDI tronqué")?;
        self.i += 1;
        Ok(v)
    }
    fn n(&mut self, k: usize) -> Result<u32, String> {
        let mut v = 0u32;
        for _ in 0..k {
            v = (v << 8) | self.u8()? as u32;
        }
        Ok(v)
    }
    fn var(&mut self) -> Result<u32, String> {
        let mut v = 0u32;
        for _ in 0..4 {
            let c = self.u8()?;
            v = (v << 7) | (c & 0x7f) as u32;
            if c & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err("MIDI : longueur variable invalide".into())
    }
}

/// Notes d'une piste MIDI : (canal, début en temps, longueur, hauteur, force)
type Raw = Vec<(u8, f64, f64, u8, f32)>;

/// Lit un fichier MIDI : un morceau avec une piste par piste MIDI qui a des notes (le canal 10 donne la batterie)
pub fn read(bytes: &[u8]) -> Result<Song, String> {
    let mut r = Reader { b: bytes, i: 0 };
    if bytes.get(0..4) != Some(b"MThd") {
        return Err("ce n'est pas un fichier MIDI".into());
    }
    r.i = 4;
    let hl = r.n(4)? as usize;
    let start = r.i;
    let _format = r.n(2)?;
    let ntrk = r.n(2)?;
    let div = r.n(2)?;
    if div & 0x8000 != 0 {
        return Err("MIDI en temps SMPTE : pas pris en charge".into());
    }
    let tpq = div.max(1) as f64;
    r.i = start + hl;
    let mut bpm = 120.0f32;
    let mut tracks: Vec<(String, Raw)> = vec![];
    for k in 0..ntrk {
        if r.b.get(r.i..r.i + 4) != Some(b"MTrk") {
            break;
        }
        r.i += 4;
        let len = r.n(4)? as usize;
        let end = (r.i + len).min(bytes.len());
        let mut tick = 0u64;
        let mut status = 0u8;
        let mut name = format!("Piste {}", k + 1);
        let mut open: std::collections::HashMap<(u8, u8), (u64, u8)> = Default::default();
        let mut raw: Raw = vec![];
        while r.i < end {
            tick += r.var()? as u64;
            let mut b = r.u8()?;
            if b < 0x80 {
                // Statut courant : le premier octet est déjà une donnée
                r.i -= 1;
                b = status;
            } else if b < 0xf0 {
                status = b;
            }
            match b {
                0xff => {
                    let ty = r.u8()?;
                    let l = r.var()? as usize;
                    let data = bytes.get(r.i..r.i + l).ok_or("MIDI tronqué")?;
                    match ty {
                        0x51 if l == 3 => {
                            let us = ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | data[2] as u32;
                            if us > 0 {
                                bpm = (60_000_000.0 / us as f64) as f32;
                            }
                        }
                        0x03 => name = String::from_utf8_lossy(data).trim().to_string(),
                        _ => {}
                    }
                    r.i += l;
                }
                0xf0 | 0xf7 => {
                    let l = r.var()? as usize;
                    r.i += l;
                }
                _ => {
                    let ch = b & 0x0f;
                    match b & 0xf0 {
                        0x90 | 0x80 => {
                            let (p, v) = (r.u8()?, r.u8()?);
                            if b & 0xf0 == 0x90 && v > 0 {
                                open.insert((ch, p), (tick, v));
                            } else if let Some((t0, v0)) = open.remove(&(ch, p)) {
                                raw.push((ch, t0 as f64 / tpq, (tick - t0) as f64 / tpq, p, v0 as f32 / 127.0));
                            }
                        }
                        0xa0 | 0xb0 | 0xe0 => r.i += 2,
                        0xc0 | 0xd0 => r.i += 1,
                        _ => return Err("MIDI : événement inconnu".into()),
                    }
                }
            }
        }
        r.i = end;
        // Une piste MIDI qui mélange des canaux donne une piste par canal (mélodie et basse séparées)
        let mut chans: Vec<u8> = raw.iter().map(|n| n.0).collect();
        chans.sort_unstable();
        chans.dedup();
        for ch in &chans {
            let part: Raw = raw.iter().filter(|n| n.0 == *ch).cloned().collect();
            let label = if chans.len() > 1 { format!("{name} · {}", ch + 1) } else { name.clone() };
            tracks.push((label, part));
        }
    }
    let mut song = Song { bpm: bpm.clamp(40.0, 240.0), beats_per_bar: 4, bars: 1, key: 0, minor: false, tracks: vec![] };
    let mut last = 0.0f64;
    for (name, raw) in tracks {
        let drums = raw.iter().filter(|n| n.0 == 9).count() * 2 > raw.len();
        let lowest = raw.iter().map(|n| n.3).min().unwrap_or(60);
        let inst = if drums {
            Inst::Drums
        } else if lowest < 48 {
            Inst::Bass
        } else {
            Inst::Piano
        };
        let mut t = song.new_track(inst);
        t.name = if name.is_empty() { t.name } else { name };
        let notes: Vec<Note> = raw.iter().map(|n| Note { t: n.1, len: n.2.max(0.05), p: if drums { drum_map(n.3) } else { n.3 }, v: n.4.max(0.05) }).collect();
        let end = notes.iter().map(|n| n.t + n.len).fold(0.0, f64::max);
        last = last.max(end);
        let len = (end / 4.0).ceil().max(1.0) * 4.0;
        t.regions.push(Region { start: 0.0, len, name: String::new(), notes });
        song.tracks.push(t);
    }
    if song.tracks.is_empty() {
        return Err("aucune note dans ce fichier MIDI".into());
    }
    song.bars = ((last / 4.0).ceil() as u32).max(1);
    Ok(song)
}

/// Ramène une note de batterie General MIDI sur les pièces du kit intégré
fn drum_map(p: u8) -> u8 {
    match p {
        35 | 36 => 36,
        37 | 38 | 40 => 38,
        39 => 39,
        42 | 44 => 42,
        46 => 46,
        41 | 43 | 45 => 45,
        47 | 48 => 47,
        50 => 50,
        49 | 51 | 52 | 53 | 55 | 57 | 59 => 49,
        _ => 42,
    }
}

const TPQ: u32 = 480;

fn var(out: &mut Vec<u8>, mut v: u32) {
    let mut buf = vec![(v & 0x7f) as u8];
    v >>= 7;
    while v > 0 {
        buf.push((v & 0x7f) as u8 | 0x80);
        v >>= 7;
    }
    buf.reverse();
    out.extend(buf);
}

/// Écrit le morceau en MIDI (format 1) : une piste par piste audible, la batterie sur le canal 10
pub fn write(s: &Song) -> Vec<u8> {
    let mut out = b"MThd".to_vec();
    let tracks: Vec<&MTrack> = s.tracks.iter().filter(|t| !t.regions.is_empty()).collect();
    out.extend(6u32.to_be_bytes());
    out.extend(1u16.to_be_bytes());
    out.extend(((tracks.len() + 1) as u16).to_be_bytes());
    out.extend((TPQ as u16).to_be_bytes());
    // Piste de tempo
    let us = (60_000_000.0 / s.bpm as f64) as u32;
    let mut t0 = vec![0u8, 0xff, 0x51, 3, (us >> 16) as u8, (us >> 8) as u8, us as u8];
    t0.extend([0, 0xff, 0x2f, 0]);
    out.extend(b"MTrk");
    out.extend((t0.len() as u32).to_be_bytes());
    out.extend(t0);
    let mut melodic = 0u8;
    for tr in tracks {
        let ch = if tr.inst == Inst::Drums {
            9
        } else {
            let c = melodic % 15;
            melodic += 1;
            if c >= 9 {
                c + 1
            } else {
                c
            }
        };
        // Événements (tick, ordre, octets) : les fins de note passent avant les débuts au même tick
        let mut ev: Vec<(u32, u8, [u8; 3])> = vec![];
        for g in &tr.regions {
            for n in g.notes.iter().filter(|n| n.t >= 0.0 && n.t < g.len) {
                let a = ((g.start + n.t) * TPQ as f64).round() as u32;
                let b = ((g.start + (n.t + n.len).min(g.len)) * TPQ as f64).round().max(a as f64 + 1.0) as u32;
                let v = (n.v * 127.0).round().clamp(1.0, 127.0) as u8;
                ev.push((a, 1, [0x90 | ch, n.p, v]));
                ev.push((b, 0, [0x80 | ch, n.p, 0]));
            }
        }
        ev.sort_by_key(|e| (e.0, e.1));
        let mut body = vec![0u8, 0xff, 0x03];
        var(&mut body, tr.name.len() as u32);
        body.extend(tr.name.as_bytes());
        let mut last = 0u32;
        for (t, _, b) in ev {
            var(&mut body, t - last);
            body.extend(b);
            last = t;
        }
        body.extend([0, 0xff, 0x2f, 0]);
        out.extend(b"MTrk");
        out.extend((body.len() as u32).to_be_bytes());
        out.extend(body);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canaux_separes() {
        // Une seule piste MIDI, une note sur le canal 1 et une basse sur le canal 2 : deux pistes
        let body: Vec<u8> = vec![0, 0x90, 72, 100, 0, 0x91, 36, 100, 0x83, 0x60, 0x80, 72, 0, 0, 0x81, 36, 0, 0, 0xff, 0x2f, 0];
        let mut f = b"MThd".to_vec();
        f.extend(6u32.to_be_bytes());
        f.extend([0, 0, 0, 1, 0x01, 0xe0]);
        f.extend(b"MTrk");
        f.extend((body.len() as u32).to_be_bytes());
        f.extend(body);
        let s = read(&f).unwrap();
        assert_eq!(s.tracks.len(), 2);
        assert_eq!(s.tracks[1].inst, Inst::Bass);
        assert_eq!(s.tracks[0].regions[0].notes[0].len, 1.0);
        assert!(read(&f[..10]).is_err());
    }

    #[test]
    fn aller_retour_midi() {
        let s = Song::starter();
        let bytes = write(&s);
        let back = read(&bytes).unwrap();
        assert!((back.bpm - s.bpm).abs() < 0.1);
        // Les pistes vides (le lead du départ) ne sont pas écrites
        assert_eq!(back.tracks.len(), 3);
        assert_eq!(back.tracks[0].inst, Inst::Drums);
        assert_eq!(back.tracks[2].inst, Inst::Bass);
        let a: Vec<(i64, u8)> = s.tracks[1].regions[0].notes.iter().map(|n| ((n.t * 480.0).round() as i64, n.p)).collect();
        let mut b: Vec<(i64, u8)> = back.tracks[1].regions[0].notes.iter().map(|n| ((n.t * 480.0).round() as i64, n.p)).collect();
        let mut a = a;
        a.sort();
        b.sort();
        assert_eq!(a, b);
        assert_eq!(back.bars, s.bars);
        assert!(read(b"pas du midi").is_err());
    }
}
