//! Résumé de fichiers par IA, via n'importe quelle API compatible OpenAI :
//! Ollama (par défaut, local et gratuit), LM Studio, OpenAI, OpenRouter...
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

const MAX_CHARS: usize = 24_000;

#[derive(Clone)]
pub struct Settings {
    pub url: String,
    pub model: String,
    pub key: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self { url: "http://localhost:11434/v1".into(), model: "llama3.2".into(), key: String::new() }
    }
}

fn file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("aryadne").join("ai.json"))
}

impl Settings {
    pub fn load() -> Self {
        let d = Self::default();
        let Some(v) = file().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|s| serde_json::from_str::<Value>(&s).ok())
        else {
            return d;
        };
        let get = |k: &str, def: String| v[k].as_str().map(str::to_string).unwrap_or(def);
        Self { url: get("url", d.url), model: get("model", d.model), key: get("key", d.key) }
    }

    pub fn save(&self) {
        if let Some(f) = file() {
            let _ = std::fs::create_dir_all(f.parent().unwrap());
            let v = json!({ "url": self.url, "model": self.model, "key": self.key });
            let _ = std::fs::write(f, serde_json::to_string_pretty(&v).unwrap());
        }
    }
}

/// Lance le résumé dans un fil séparé pour ne pas figer l'interface.
pub fn summarize(settings: Settings, name: String, text: String, ctx: eframe::egui::Context) -> Receiver<Result<String, String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(request(&settings, &name, &text));
        ctx.request_repaint();
    });
    rx
}

fn request(s: &Settings, name: &str, text: &str) -> Result<String, String> {
    let mut content: String = text.chars().take(MAX_CHARS).collect();
    if content.len() < text.len() {
        content.push_str("\n\n[...fichier tronqué...]");
    }
    let body = json!({
        "model": s.model,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": "Tu résumes des fichiers pour une base de notes personnelle. Réponds en français, en Markdown : une phrase d'ensemble, puis 3 à 7 points clés. Pour du code : ce que fait le fichier, ses parties principales et ses dépendances. Sois concis." },
            { "role": "user", "content": format!("Fichier : {name}\n\n{content}") }
        ]
    });
    let url = format!("{}/chat/completions", s.url.trim_end_matches('/'));
    let agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(180))).build().new_agent();
    let mut req = agent.post(&url);
    if !s.key.trim().is_empty() {
        req = req.header("Authorization", format!("Bearer {}", s.key.trim()));
    }
    let mut resp = req.send_json(&body).map_err(|e| match e {
        ureq::Error::Io(_) | ureq::Error::ConnectionFailed | ureq::Error::HostNotFound => {
            format!("Impossible de joindre {url}. Ollama ou LM Studio est-il lancé ?")
        }
        ureq::Error::StatusCode(404) => format!("Modèle « {} » introuvable (ou mauvaise adresse).", s.model),
        ureq::Error::StatusCode(401) => "Clé API refusée.".into(),
        other => other.to_string(),
    })?;
    let v: Value = resp.body_mut().read_json().map_err(|e| e.to_string())?;
    v["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.trim().to_string())
        .ok_or_else(|| format!("Réponse inattendue : {}", v.to_string().chars().take(200).collect::<String>()))
}
