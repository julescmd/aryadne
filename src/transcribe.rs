//! Transcription de la parole en sous-titres : whisper.cpp sur l'ordinateur s'il est installé,
//! sinon l'API de transcription d'OpenAI si une clé est enregistrée dans les réglages de l'IA.
//! Lecture et écriture des fichiers SRT.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

fn srt_time(t: f32) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

fn parse_time(s: &str) -> Option<f32> {
    let s = s.trim().replace(',', ".");
    let mut parts = s.split(':').rev();
    let sec: f32 = parts.next()?.parse().ok()?;
    let min: f32 = parts.next().unwrap_or("0").parse().ok()?;
    let hour: f32 = parts.next().unwrap_or("0").parse().ok()?;
    Some(hour * 3600.0 + min * 60.0 + sec)
}

/// Lit un SRT (ou un WebVTT simple).
pub fn parse_srt(s: &str) -> Vec<Cue> {
    let mut out = vec![];
    let s = s.replace("\r\n", "\n");
    for block in s.split("\n\n") {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty() && l.trim() != "WEBVTT").collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let mut it = lines[ti].split("-->");
        let (Some(a), Some(b)) = (it.next().and_then(parse_time), it.next().and_then(|x| parse_time(x.split_whitespace().next().unwrap_or("")))) else { continue };
        let text = lines[ti + 1..].join("\n").trim().to_string();
        if !text.is_empty() && b > a {
            out.push(Cue { start: a, end: b, text });
        }
    }
    out
}

pub fn to_srt(cues: &[Cue]) -> String {
    cues.iter().enumerate().map(|(i, c)| format!("{}\n{} --> {}\n{}\n", i + 1, srt_time(c.start), srt_time(c.end), c.text.trim())).collect::<Vec<_>>().join("\n")
}

/// Coupe les répliques trop longues pour l'écran (≈ 42 caractères par ligne, deux lignes au plus),
/// en partageant leur temps selon la longueur de chaque morceau.
pub fn tidy(cues: Vec<Cue>) -> Vec<Cue> {
    let mut out = vec![];
    for c in cues {
        let words: Vec<&str> = c.text.split_whitespace().collect();
        let total: usize = words.iter().map(|w| w.len() + 1).sum::<usize>().max(1);
        let mut chunks: Vec<String> = vec![];
        let mut cur = String::new();
        for w in &words {
            if cur.len() + w.len() + 1 > 84 && !cur.is_empty() {
                chunks.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(w);
        }
        if !cur.is_empty() {
            chunks.push(cur);
        }
        let mut t = c.start;
        for ch in chunks {
            let d = (c.end - c.start) * (ch.len() + 1) as f32 / total as f32;
            // Deux lignes équilibrées
            let text = if ch.len() > 42 {
                let mid = ch.len() / 2;
                let cut = ch[..mid].rfind(' ').or_else(|| ch[mid..].find(' ').map(|i| i + mid)).unwrap_or(mid);
                format!("{}\n{}", ch[..cut].trim(), ch[cut..].trim())
            } else {
                ch
            };
            out.push(Cue { start: t, end: (t + d).min(c.end), text });
            t += d;
        }
    }
    out
}

/// whisper.cpp installé ? (son programme)
pub fn whisper_bin() -> Option<PathBuf> {
    ["whisper-cli", "whisper-cpp", "whisper.cpp", "main"].iter().find_map(|n| crate::lsp::which(n).filter(|p| *n != "main" || p.to_string_lossy().contains("whisper")))
}

/// Un modèle de whisper.cpp (ggml-*.bin) : ARYADNE_WHISPER_MODEL, sinon les dossiers habituels.
pub fn whisper_model() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("ARYADNE_WHISPER_MODEL").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    let mut dirs: Vec<PathBuf> = vec![];
    if let Some(h) = dirs::home_dir() {
        dirs.extend([h.join(".cache/whisper"), h.join(".local/share/whisper"), h.join("whisper.cpp/models"), h.join("models")]);
    }
    if let Some(d) = dirs::data_dir() {
        dirs.push(d.join("aryadne/whisper"));
    }
    let mut found: Vec<PathBuf> = dirs.iter().filter_map(|d| std::fs::read_dir(d).ok()).flatten().flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("ggml-") && n.to_string_lossy().ends_with(".bin"))).collect();
    // Le plus gros modèle trouvé : le plus juste
    found.sort_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0));
    found.pop()
}

/// Où la transcription peut se faire, en mots, ou pourquoi elle ne peut pas.
pub fn engine() -> Result<&'static str, String> {
    if whisper_bin().is_some() && whisper_model().is_some() {
        return Ok("whisper.cpp");
    }
    if openai_key().is_some() {
        return Ok("OpenAI");
    }
    Err(crate::i18n::t("Pour transcrire : installe whisper.cpp et un modèle (ggml-*.bin dans ~/.cache/whisper), ou enregistre une clé OpenAI dans les réglages de l'IA."))
}

fn openai_key() -> Option<String> {
    let s = crate::ai::Settings::load();
    s.keys.get("openai").cloned().filter(|k| !k.trim().is_empty()).or_else(|| (s.url.contains("api.openai.com") && !s.key.trim().is_empty()).then(|| s.key.clone()))
}

/// Transcrit le son d'un fichier (n'importe quel média que ffmpeg lit). `lang` : « fr », « en »… ou vide pour deviner.
pub fn transcribe(media: &Path, lang: &str) -> Result<Vec<Cue>, String> {
    let ff = crate::ffmpeg::find("ffmpeg").ok_or("ffmpeg introuvable")?;
    let tmp = std::env::temp_dir().join(format!("aryadne-transcription-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let r = (|| {
        if let (Some(bin), Some(model)) = (whisper_bin(), whisper_model()) {
            let wav = tmp.join("son.wav");
            run(crate::ffmpeg::command(ff.clone()).args(["-y", "-v", "error", "-i"]).arg(media).args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le"]).arg(&wav))?;
            let out = tmp.join("texte");
            let mut cmd = std::process::Command::new(bin);
            cmd.arg("-m").arg(&model).arg("-f").arg(&wav).arg("-osrt").arg("-of").arg(&out).arg("-l").arg(if lang.is_empty() { "auto" } else { lang });
            run(&mut cmd)?;
            let srt = std::fs::read_to_string(out.with_extension("srt")).map_err(|e| e.to_string())?;
            return Ok(parse_srt(&srt));
        }
        let key = openai_key().ok_or_else(|| engine().err().unwrap_or_default())?;
        // Son compressé : l'API refuse les fichiers de plus de 25 Mo
        let m4a = tmp.join("son.m4a");
        run(crate::ffmpeg::command(ff.clone()).args(["-y", "-v", "error", "-i"]).arg(media).args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "aac", "-b:a", "48k"]).arg(&m4a))?;
        let bytes = std::fs::read(&m4a).map_err(|e| e.to_string())?;
        if bytes.len() > 24_000_000 {
            return Err(crate::i18n::t("Le son est trop long pour l'API (plus de 25 Mo) : coupe-le en parties, ou installe whisper.cpp."));
        }
        let boundary = format!("aryadne{}", std::process::id());
        let mut body: Vec<u8> = vec![];
        let mut field = |name: &str, value: &str| {
            body.extend(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
        };
        field("model", "whisper-1");
        field("response_format", "srt");
        if !lang.is_empty() {
            field("language", lang);
        }
        body.extend(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"son.m4a\"\r\nContent-Type: audio/mp4\r\n\r\n").as_bytes());
        body.extend(&bytes);
        body.extend(format!("\r\n--{boundary}--\r\n").as_bytes());
        let mut resp = ureq::post("https://api.openai.com/v1/audio/transcriptions")
            .header("Authorization", &format!("Bearer {}", key.trim()))
            .header("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
            .send(&body[..])
            .map_err(|e| format!("{} {e}", crate::i18n::t("Transcription refusée :")))?;
        let srt = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
        Ok(parse_srt(&srt))
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    r.map(tidy)
}

fn run(cmd: &mut std::process::Command) -> Result<(), String> {
    let o = cmd.output().map_err(|e| e.to_string())?;
    if o.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("échec").to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sous_titres() {
        let s = "1\n00:00:01,000 --> 00:00:02,500\nBonjour.\n\n2\n00:00:03,000 --> 00:00:04,000\nÇa va ?\nOui.\n";
        let c = parse_srt(s);
        assert_eq!(c.len(), 2);
        assert_eq!(c[1].text, "Ça va ?\nOui.");
        assert!((c[0].end - 2.5).abs() < 1e-4);
        assert_eq!(parse_srt(&to_srt(&c)), c);
        // WebVTT
        let v = parse_srt("WEBVTT\n\n00:01.000 --> 00:02.000\nSalut\n");
        assert_eq!(v.len(), 1);
        // Une longue réplique se coupe en morceaux de deux lignes au plus
        let long = Cue { start: 0.0, end: 10.0, text: "un ".repeat(60) };
        let t = tidy(vec![long]);
        assert!(t.len() >= 2 && t.iter().all(|c| c.text.lines().count() <= 2 && c.text.lines().all(|l| l.len() <= 50)));
        assert!((t.last().unwrap().end - 10.0).abs() < 0.01);
    }
}
