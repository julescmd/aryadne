//! Chat avec le coffre : cherche les passages pertinents dans tes fichiers, les envoie au
//! modèle avec la liste des fichiers, et affiche la réponse en direct avec ses sources.
use crate::vault::{self, Entry};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::BufRead;
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
}

#[derive(Default)]
pub struct Live {
    pub text: String,
    pub sources: Vec<String>,
    pub status: String,
    pub done: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Chat {
    pub msgs: Vec<Msg>,
    pub input: String,
    pub live: Option<Arc<Mutex<Live>>>,
    pub include_open: bool,
}

impl Chat {
    pub fn busy(&self) -> bool {
        self.live.is_some()
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
        self.msgs.push(Msg { role: Role::User, text: q.clone(), sources: vec![], error: None });
        let history: Vec<Msg> = self.msgs.iter().rev().skip(1).take(8).rev().cloned().collect();
        let live = Arc::new(Mutex::new(Live { status: "recherche dans le coffre…".into(), ..Default::default() }));
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
                let messages = build_messages(&entries, &hits, open_file.as_ref(), &history, &q);
                stream(&settings, messages, &|delta: &str| {
                    let d = delta.to_string();
                    set(&move |l: &mut Live| {
                        l.text.push_str(&d);
                        l.status.clear();
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
        let finished = self.live.as_ref().and_then(|l| l.lock().ok().filter(|l| l.done).map(|l| (l.text.clone(), l.sources.clone(), l.error.clone())));
        if let Some((text, sources, error)) = finished {
            self.msgs.push(Msg { role: Role::Assistant, text, sources, error });
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

fn build_messages(entries: &[Entry], hits: &[Hit], open: Option<&(String, String)>, history: &[Msg], q: &str) -> Vec<Value> {
    let files: Vec<&str> = entries.iter().filter(|e| !e.is_dir).map(|e| e.rel.as_str()).take(400).collect();
    let mut sys = String::from(
        "Tu es l'assistant intégré à Aryadne, le logiciel de notes de l'utilisateur. Tu as accès à son coffre (un dossier de notes, de code et d'images).\n\
         Règles :\n\
         - Réponds en français, de façon concise et concrète, en Markdown simple.\n\
         - Appuie-toi sur les extraits fournis. Si l'information n'y est pas, dis-le et propose quel fichier ouvrir.\n\
         - Pour renvoyer vers une note .md, écris [[Nom de la note]] (sans .md). Pour un autre fichier, écris son chemin exact entre accents graves, par ex. `code/export.py`.\n\
         - Ne cite que des fichiers présents dans la liste ci-dessous.\n\
         - Quand tu proposes du code, donne-le complet dans un bloc ```langage, prêt à être inséré tel quel. Si une sélection est fournie, ta réponse porte d'abord sur elle.\n\n",
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
    msgs.push(json!({ "role": "user", "content": q }));
    msgs
}

/// Requête en streaming (Server-Sent Events) vers une API compatible OpenAI.
fn stream(s: &crate::ai::Settings, messages: Vec<Value>, on_delta: &dyn Fn(&str)) -> Result<(), String> {
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
