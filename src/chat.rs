//! Chat avec le coffre : cherche les passages pertinents dans tes fichiers, les envoie au
//! modèle avec la liste des fichiers, et affiche la réponse en direct avec ses sources.
use crate::vault::{self, Entry};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::BufRead;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const TEXT_EXT: [&str; 26] = [
    "md", "txt", "rs", "py", "js", "ts", "tsx", "jsx", "json", "toml", "yaml", "yml", "html", "css", "c", "h", "cpp", "hpp", "cs", "java", "go",
    "lua", "sh", "glsl", "hlsl", "csv",
];
const MAX_FILE: u64 = 400_000;
const CHUNK: usize = 1400;
const TOP: usize = 6;

#[derive(Clone, PartialEq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Clone)]
pub struct Msg {
    pub role: Role,
    pub text: String,
    pub sources: Vec<String>,
    pub error: Option<String>,
    /// Noms des pièces jointes envoyées avec ce message.
    pub attached: Vec<String>,
}

/// Pièce jointe : image (envoyée au modèle s'il voit les images) ou texte.
#[derive(Clone)]
pub enum Attachment {
    Image { name: String, data_url: String },
    Text { name: String, text: String },
}

impl Attachment {
    pub fn name(&self) -> &str {
        match self {
            Attachment::Image { name, .. } | Attachment::Text { name, .. } => name,
        }
    }

    /// Fichier ou dossier du disque → pièces jointes.
    pub fn from_path(path: &std::path::Path) -> Vec<Attachment> {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if path.is_dir() {
            // Dossier : arborescence + contenu des fichiers texte (jusqu'à ~60 000 caractères)
            let mut listing = String::new();
            let mut texts = String::new();
            let mut images = vec![];
            let mut stack = vec![(path.to_path_buf(), 0usize)];
            let mut count = 0;
            while let Some((d, depth)) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&d) else { continue };
                let mut items: Vec<_> = rd.flatten().collect();
                items.sort_by_key(|f| f.file_name());
                for f in items {
                    let fp = f.path();
                    let n = f.file_name().to_string_lossy().to_string();
                    if n.starts_with('.') || count > 400 {
                        continue;
                    }
                    count += 1;
                    let rel = fp.strip_prefix(path).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or(n.clone());
                    listing.push_str(&format!("{}{}{}\n", "  ".repeat(depth), n, if fp.is_dir() { "/" } else { "" }));
                    if fp.is_dir() {
                        if depth < 4 {
                            stack.push((fp, depth + 1));
                        }
                    } else if is_text_path(&fp) && texts.len() < 60_000 {
                        if let Ok(t) = std::fs::read_to_string(&fp) {
                            let t: String = t.chars().take(8000).collect();
                            texts.push_str(&format!("\n--- {rel} ---\n{t}\n"));
                        }
                    } else if crate::images::is_image(&n) && images.len() < 4 {
                        images.extend(Self::image(&fp, &format!("{name}/{rel}")));
                    }
                }
            }
            let mut out = vec![Attachment::Text { name: format!("{name}/"), text: format!("Dossier {name} :\n{listing}{texts}") }];
            out.extend(images);
            return out;
        }
        if crate::images::is_image(&name) {
            return Self::image(path, &name).into_iter().collect();
        }
        if is_text_path(path) {
            if let Ok(t) = std::fs::read_to_string(path) {
                return vec![Attachment::Text { name: name.clone(), text: t.chars().take(60_000).collect() }];
            }
        }
        if crate::office::is_office(&name) {
            if let Ok(t) = crate::office::to_text(path) {
                return vec![Attachment::Text { name: name.clone(), text: t.chars().take(60_000).collect() }];
            }
        }
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        vec![Attachment::Text { name: name.clone(), text: format!("(fichier binaire {name}, {} Ko : contenu non lisible)", size / 1024) }]
    }

    fn image(path: &std::path::Path, name: &str) -> Option<Attachment> {
        let img = image::open(path).ok()?;
        Some(Self::from_image(img, name))
    }

    pub fn from_image(img: image::DynamicImage, name: &str) -> Attachment {
        use base64::Engine;
        let img = if img.width() > 1280 || img.height() > 1280 { img.thumbnail(1280, 1280) } else { img };
        let mut buf = std::io::Cursor::new(Vec::new());
        let _ = img.to_rgb8().write_to(&mut buf, image::ImageFormat::Jpeg);
        let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
        Attachment::Image { name: name.to_string(), data_url: format!("data:image/jpeg;base64,{b64}") }
    }
}

fn is_text_path(p: &std::path::Path) -> bool {
    let e = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    TEXT_EXT.contains(&e.as_str()) || matches!(e.as_str(), "xml" | "svg" | "ini" | "cfg" | "log" | "gd" | "vue" | "svelte" | "php" | "rb" | "sql" | "bat" | "ps1" | "mjs" | "osl")
}

#[derive(Default)]
pub struct Live {
    /// Mis à vrai par « arrêter » : la génération s'interrompt.
    pub stop: Arc<AtomicBool>,
    pub text: String,
    pub sources: Vec<String>,
    pub status: String,
    pub done: bool,
    pub error: Option<String>,
    /// Début de la réponse et nombre de morceaux reçus (pour la vitesse)
    pub started: Option<std::time::Instant>,
    pub chunks: usize,
}

#[derive(Default)]
pub struct Chat {
    pub msgs: Vec<Msg>,
    pub input: String,
    pub live: Option<Arc<Mutex<Live>>>,
    pub include_open: bool,
    pub attachments: Vec<Attachment>,
    /// Vitesse de la dernière réponse (jetons par seconde, approximatif)
    pub last_speed: Option<f32>,
}

impl Chat {
    pub fn busy(&self) -> bool {
        self.live.is_some()
    }

    /// Interrompt la réponse en cours (le texte déjà reçu est gardé).
    pub fn stop(&self) {
        if let Some(l) = &self.live {
            if let Ok(l) = l.lock() {
                l.stop.store(true, Ordering::Relaxed);
            }
        }
    }

    /// Envoie la question. `open_file` = (chemin, contenu) du fichier ouvert, si on l'inclut.
    pub fn send(
        &mut self,
        root: String,
        entries: Vec<Entry>,
        open_file: Option<(String, String)>,
        settings: crate::ai::Settings,
        local: crate::ollama::Local,
        ctx: eframe::egui::Context,
    ) {
        let q = self.input.trim().to_string();
        if q.is_empty() || self.busy() {
            return;
        }
        self.input.clear();
        let attachments = std::mem::take(&mut self.attachments);
        let attached: Vec<String> = attachments.iter().map(|a| a.name().to_string()).collect();
        self.msgs.push(Msg { role: Role::User, text: q.clone(), sources: vec![], error: None, attached });
        let history: Vec<Msg> = self.msgs.iter().rev().skip(1).take(8).rev().cloned().collect();
        let stop = Arc::new(AtomicBool::new(false));
        let live = Arc::new(Mutex::new(Live { status: "recherche dans le coffre…".into(), stop: stop.clone(), ..Default::default() }));
        self.live = Some(live.clone());
        std::thread::spawn(move || {
            let set = |f: &dyn Fn(&mut Live)| {
                if let Ok(mut l) = live.lock() {
                    f(&mut l);
                }
                ctx.request_repaint();
            };
            let hits = search(&root, &entries, &q);
            let sources: Vec<String> = {
                let mut seen = HashSet::new();
                hits.iter().filter(|h| seen.insert(h.rel.clone())).map(|h| h.rel.clone()).collect()
            };
            let src = sources.clone();
            set(&move |l: &mut Live| {
                l.sources = src.clone();
                l.status = "réflexion…".into();
            });
            let ready = if crate::ollama::is_local(&settings.url) { local.ensure_running(&settings.url) } else { Ok(()) };
            let result = ready.and_then(|_| {
                let messages = build_messages(&entries, &hits, open_file.as_ref(), &history, &q, &attachments);
                stream(&settings, messages, &stop, &|delta: &str| {
                    let d = delta.to_string();
                    set(&move |l: &mut Live| {
                        l.text.push_str(&d);
                        l.status.clear();
                        l.started.get_or_insert_with(std::time::Instant::now);
                        l.chunks += 1;
                    });
                })
            });
            set(&move |l: &mut Live| {
                if let Err(e) = &result {
                    l.error = Some(e.clone());
                }
                l.done = true;
            });
        });
    }

    /// À appeler à chaque image : range la réponse terminée dans l'historique.
    pub fn poll(&mut self) {
        let finished = self.live.as_ref().and_then(|l| l.lock().ok().filter(|l| l.done).map(|l| (l.text.clone(), l.sources.clone(), l.error.clone(), l.stop.load(Ordering::Relaxed), l.started.map(|t| t.elapsed().as_secs_f32()), l.chunks)));
        if let Some((mut text, sources, error, stopped, secs, chunks)) = finished {
            if stopped {
                text.push_str("\n\n*(arrêté)*");
            }
            self.last_speed = secs.filter(|s| *s > 0.2 && chunks > 3).map(|s| chunks as f32 / s);
            self.msgs.push(Msg { role: Role::Assistant, text, sources, error, attached: vec![] });
            self.live = None;
        }
    }

    /// La conversation en Markdown, pour l'enregistrer comme note.
    pub fn to_markdown(&self) -> String {
        let mut out = String::from("# Conversation\n\n");
        for m in &self.msgs {
            match m.role {
                Role::User => out.push_str(&format!("**Moi :** {}\n\n", m.text)),
                Role::Assistant => {
                    out.push_str(&format!("**Aryadne :** {}\n\n", m.text));
                    if !m.sources.is_empty() {
                        let s: Vec<String> = m.sources.iter().map(|r| if vault::is_md(r) { format!("[[{}]]", vault::stem(r)) } else { format!("`{r}`") }).collect();
                        out.push_str(&format!("Sources : {}\n\n", s.join(", ")));
                    }
                }
            }
        }
        out
    }
}

// ---------- recherche ----------

pub struct Hit {
    pub rel: String,
    pub text: String,
    pub score: f32,
}

/// Minuscules sans accents, pour que « équipe » trouve « equipe ».
pub fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' | 'À' | 'Â' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'î' | 'ï' | 'í' | 'Î' | 'Ï' => 'i',
            'ô' | 'ö' | 'ó' | 'Ô' | 'Ö' => 'o',
            'ù' | 'û' | 'ü' | 'ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

const STOP: [&str; 48] = [
    "les", "des", "une", "est", "que", "qui", "dans", "pour", "par", "sur", "avec", "pas", "plus", "mes", "mon", "son", "ses", "aux", "cette", "ces",
    "the", "and", "for", "with", "this", "that", "are", "what", "how", "quoi", "quel", "quels", "quelle", "quelles", "comment", "fait", "faire",
    "moi", "toi", "tout", "tous", "elle", "ils", "nous", "vous", "ont", "sont", "etre",
];

fn tokens(s: &str) -> Vec<String> {
    fold(s).split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 2 && !STOP.contains(w)).map(str::to_string).collect()
}

fn is_text(rel: &str) -> bool {
    let e = rel.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    TEXT_EXT.contains(&e.as_str()) && !rel.ends_with(".board.json")
}

/// Recherche plein texte (score de type TF-IDF) sur des morceaux de fichiers.
pub fn search(root: &str, entries: &[Entry], query: &str) -> Vec<Hit> {
    let q = tokens(query);
    if q.is_empty() {
        return vec![];
    }
    let mut chunks: Vec<(String, String, HashMap<String, usize>)> = vec![];
    for e in entries.iter().filter(|e| !e.is_dir && is_text(&e.rel)) {
        let path = std::path::Path::new(root).join(&e.rel);
        if std::fs::metadata(&path).map(|m| m.len() > MAX_FILE).unwrap_or(true) {
            continue;
        }
        let Ok(text) = vault::read(root, &e.rel) else { continue };
        let mut start = 0;
        let chars: Vec<char> = text.chars().collect();
        while start < chars.len() || start == 0 {
            let end = (start + CHUNK).min(chars.len());
            let piece: String = chars[start..end].iter().collect();
            let mut tf = HashMap::new();
            for t in tokens(&format!("{} {}", e.rel, piece)) {
                *tf.entry(t).or_insert(0) += 1;
            }
            chunks.push((e.rel.clone(), piece, tf));
            if end >= chars.len() {
                break;
            }
            start = end.saturating_sub(200);
        }
    }
    let n = chunks.len() as f32;
    let df = |t: &str| chunks.iter().filter(|c| c.2.contains_key(t)).count() as f32;
    let idf: HashMap<&String, f32> = q.iter().map(|t| (t, ((n + 1.0) / (df(t) + 0.5)).ln())).collect();
    let mut hits: Vec<Hit> = chunks
        .into_iter()
        .filter_map(|(rel, text, tf)| {
            let name = fold(&rel);
            let mut score = 0.0;
            for t in &q {
                if let Some(&c) = tf.get(t) {
                    score += (1.0 + (c as f32).ln()) * idf[t];
                }
                if name.contains(t.as_str()) {
                    score += 2.0 * idf[t];
                }
            }
            (score > 0.0).then_some(Hit { rel, text, score })
        })
        .collect();
    hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    hits.truncate(TOP);
    hits
}

fn build_messages(entries: &[Entry], hits: &[Hit], open: Option<&(String, String)>, history: &[Msg], q: &str, attachments: &[Attachment]) -> Vec<Value> {
    let files: Vec<&str> = entries.iter().filter(|e| !e.is_dir).map(|e| e.rel.as_str()).take(400).collect();
    let mut sys = String::from(
        "Tu es l'assistant intégré à Aryadne, le logiciel de notes de l'utilisateur. Tu as accès à son coffre (un dossier de notes, de code et d'images).\n\
         Règles :\n\
         - Réponds en français, de façon concise et concrète, en Markdown simple.\n\
         - Appuie-toi sur les extraits fournis. Si l'information n'y est pas, dis-le et propose quel fichier ouvrir.\n\
         - Pour renvoyer vers une note .md, écris [[Nom de la note]] (sans .md). Pour un autre fichier, écris son chemin exact entre accents graves, par ex. `code/export.py`.\n\
         - Ne cite que des fichiers présents dans la liste ci-dessous.\n\
         - Quand tu proposes du code, donne-le complet dans un bloc ```langage, prêt à être inséré tel quel. Si une sélection est fournie, ta réponse porte d'abord sur elle.\n\
         - Si la demande est ambiguë ou qu'il te manque une information importante, pose d'abord UNE question courte au lieu de deviner. Tu peux proposer des réponses toutes faites sur une ligne qui commence par « ?> », séparées par « | », par ex. :\n?> Un résumé court | Un plan détaillé | Les deux\n\
         - Les pièces jointes (fichiers, dossiers, images) envoyées par l'utilisateur font partie de sa question.\n\n",
    );
    sys.push_str(&format!("Fichiers du coffre ({}) :\n{}\n", files.len(), files.join("\n")));
    if let Some((rel, text)) = open {
        let t: String = text.chars().take(6000).collect();
        sys.push_str(&format!("\nFichier actuellement ouvert : {rel}\n---\n{t}\n---\n"));
    }
    if !hits.is_empty() {
        sys.push_str("\nExtraits pertinents :\n");
        for h in hits {
            sys.push_str(&format!("\n[{}]\n{}\n", h.rel, h.text));
        }
    }
    let mut msgs = vec![json!({ "role": "system", "content": sys })];
    for m in history {
        let role = if m.role == Role::User { "user" } else { "assistant" };
        if m.error.is_none() {
            msgs.push(json!({ "role": role, "content": m.text }));
        }
    }
    // Pièces jointes : le texte est ajouté à la question, les images envoyées à part.
    let mut text = q.to_string();
    for a in attachments {
        if let Attachment::Text { name, text: t } = a {
            text.push_str(&format!("\n\n[Pièce jointe : {name}]\n{t}"));
        }
    }
    let images: Vec<&str> = attachments.iter().filter_map(|a| if let Attachment::Image { data_url, .. } = a { Some(data_url.as_str()) } else { None }).collect();
    if images.is_empty() {
        msgs.push(json!({ "role": "user", "content": text }));
    } else {
        let mut parts = vec![json!({ "type": "text", "text": text })];
        for u in images {
            parts.push(json!({ "type": "image_url", "image_url": { "url": u } }));
        }
        msgs.push(json!({ "role": "user", "content": parts }));
    }
    msgs
}

/// Réponses proposées par l'IA (ligne « ?> a | b | c ») et texte sans cette ligne.
pub fn split_choices(text: &str) -> (String, Vec<String>) {
    let mut choices = vec![];
    let mut kept = vec![];
    for l in text.lines() {
        if let Some(rest) = l.trim_start().strip_prefix("?>") {
            choices.extend(rest.split('|').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()));
        } else {
            kept.push(l);
        }
    }
    (kept.join("\n"), choices)
}

/// Requête en streaming (Server-Sent Events) vers une API compatible OpenAI.
fn stream(s: &crate::ai::Settings, messages: Vec<Value>, stop: &AtomicBool, on_delta: &dyn Fn(&str)) -> Result<(), String> {
    let url = format!("{}/chat/completions", s.url.trim_end_matches('/'));
    let mut req = crate::ollama::agent(600).post(&url);
    if !s.key.trim().is_empty() {
        req = req.header("Authorization", format!("Bearer {}", s.key.trim()));
    }
    let body = json!({ "model": s.model, "messages": messages, "stream": true, "temperature": 0.3 });
    let mut resp = req.send_json(&body).map_err(|e| match e {
        ureq::Error::StatusCode(404) => format!("Modèle « {} » introuvable. Télécharge-le dans les réglages ia.", s.model),
        ureq::Error::StatusCode(401) => "Clé API refusée.".into(),
        ureq::Error::Io(_) | ureq::Error::ConnectionFailed | ureq::Error::HostNotFound => format!("Impossible de joindre {url}."),
        other => other.to_string(),
    })?;
    let reader = std::io::BufReader::new(resp.body_mut().with_config().limit(u64::MAX).reader());
    for line in reader.lines() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let line = line.map_err(|e| e.to_string())?;
        let Some(data) = line.strip_prefix("data:").map(str::trim) else { continue };
        if data == "[DONE]" {
            break;
        }
        if let Ok(v) = serde_json::from_str::<Value>(data) {
            if let Some(d) = v["choices"][0]["delta"]["content"].as_str() {
                on_delta(d);
            }
            if let Some(err) = v["error"]["message"].as_str() {
                return Err(err.to_string());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recherche() {
        let dir = std::env::temp_dir().join("aryadne-chat-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("notes")).unwrap();
        std::fs::write(dir.join("notes/Forêt.md"), "La forêt brumeuse du court métrage, lumière froide.").unwrap();
        std::fs::write(dir.join("notes/Recette.md"), "Pâtes au pesto, basilic.").unwrap();
        let root = dir.to_string_lossy().to_string();
        let entries = vault::scan(&root);
        let hits = search(&root, &entries, "Quelle lumière pour la foret ?");
        assert_eq!(hits[0].rel, "notes/Forêt.md");
        assert!(hits.iter().all(|h| h.rel != "notes/Recette.md"));
        assert_eq!(fold("Équipe Forêt"), "equipe foret");
    }
}
