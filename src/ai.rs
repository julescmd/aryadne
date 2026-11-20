//! Résumé de fichiers par IA, via n'importe quelle API compatible OpenAI :
//! Ollama (par défaut, local et gratuit), LM Studio, OpenAI, OpenRouter...
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{LazyLock, Mutex};

/// Un fournisseur d'IA. Tous parlent le format OpenAI (/chat/completions, /models).
pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    /// Page où créer une clé API (vide pour les serveurs locaux).
    pub key_page: &'static str,
    /// Modèles proposés tant que la liste n'a pas été lue chez le fournisseur.
    pub suggested: &'static [&'static str],
}

pub const PROVIDERS: &[Provider] = &[
    Provider { id: "ollama", name: "Ollama (local)", url: "http://localhost:11434/v1", key_page: "", suggested: &[] },
    Provider { id: "anthropic", name: "Claude (Anthropic)", url: "https://api.anthropic.com/v1", key_page: "https://console.anthropic.com/settings/keys", suggested: &["claude-sonnet-4-5", "claude-opus-4-1", "claude-haiku-4-5"] },
    Provider { id: "openai", name: "ChatGPT (OpenAI)", url: "https://api.openai.com/v1", key_page: "https://platform.openai.com/api-keys", suggested: &["gpt-5", "gpt-5-mini", "gpt-4.1", "gpt-4o-mini"] },
    Provider { id: "deepseek", name: "DeepSeek", url: "https://api.deepseek.com/v1", key_page: "https://platform.deepseek.com/api_keys", suggested: &["deepseek-chat", "deepseek-reasoner"] },
    Provider { id: "gemini", name: "Gemini (Google)", url: "https://generativelanguage.googleapis.com/v1beta/openai", key_page: "https://aistudio.google.com/apikey", suggested: &["gemini-2.5-flash", "gemini-2.5-pro"] },
    Provider { id: "mistral", name: "Mistral", url: "https://api.mistral.ai/v1", key_page: "https://console.mistral.ai/api-keys", suggested: &["mistral-large-latest", "mistral-small-latest", "codestral-latest"] },
    Provider { id: "xai", name: "Grok (xAI)", url: "https://api.x.ai/v1", key_page: "https://console.x.ai", suggested: &["grok-4", "grok-3-mini"] },
    Provider { id: "groq", name: "Groq (très rapide)", url: "https://api.groq.com/openai/v1", key_page: "https://console.groq.com/keys", suggested: &["llama-3.3-70b-versatile", "qwen/qwen3-32b"] },
    Provider { id: "openrouter", name: "OpenRouter (tous les modèles)", url: "https://openrouter.ai/api/v1", key_page: "https://openrouter.ai/keys", suggested: &["anthropic/claude-sonnet-4.5", "openai/gpt-5", "deepseek/deepseek-chat"] },
    Provider { id: "lmstudio", name: "LM Studio (local)", url: "http://localhost:1234/v1", key_page: "", suggested: &[] },
];

pub fn provider(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// Fournisseur correspondant à une adresse.
pub fn provider_for_url(url: &str) -> Option<&'static Provider> {
    let u = url.trim_end_matches('/');
    PROVIDERS.iter().find(|p| p.url == u)
}

/// Listes de modèles lues chez chaque fournisseur (mises en cache sur le disque).
static MODELS: LazyLock<Mutex<BTreeMap<String, Result<Vec<String>, String>>>> = LazyLock::new(|| {
    let cached: BTreeMap<String, Vec<String>> = models_file().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    Mutex::new(cached.into_iter().map(|(k, v)| (k, Ok(v))).collect())
});

fn models_file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("aryadne").join("ai-models.json"))
}

/// Modèles connus pour un fournisseur : la liste lue, sinon les suggestions.
pub fn models_of(id: &str) -> Vec<String> {
    if let Some(Ok(v)) = MODELS.lock().ok().and_then(|m| m.get(id).cloned()) {
        if !v.is_empty() {
            return v;
        }
    }
    provider(id).map(|p| p.suggested.iter().map(|s| s.to_string()).collect()).unwrap_or_default()
}

/// Dernière erreur en lisant la liste d'un fournisseur (clé refusée…).
pub fn models_error(id: &str) -> Option<String> {
    MODELS.lock().ok().and_then(|m| m.get(id).and_then(|r| r.clone().err()))
}

/// Lit la liste des modèles chez un fournisseur, en arrière-plan.
pub fn fetch_models(id: &'static str, key: String, ctx: eframe::egui::Context) {
    let Some(p) = provider(id) else { return };
    std::thread::spawn(move || {
        let url = format!("{}/models", p.url);
        let req = auth(crate::ollama::agent(20).get(&url), id, &key);
        let res = req.call().map_err(|e| match e {
            ureq::Error::StatusCode(401) | ureq::Error::StatusCode(403) => "clé refusée".to_string(),
            other => other.to_string(),
        });
        let res = res.and_then(|mut r| r.body_mut().read_json::<Value>().map_err(|e| e.to_string())).map(|v| {
            let mut ids: Vec<String> = v["data"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str()).map(|s| s.trim_start_matches("models/").to_string()).collect()).unwrap_or_default();
            // Garde les modèles de discussion
            ids.retain(|m| {
                let m = m.to_lowercase();
                !["embed", "whisper", "tts", "dall-e", "image", "audio", "moderation", "transcribe", "realtime", "search", "davinci", "babbage", "aqa", "imagen", "veo"].iter().any(|x| m.contains(x))
            });
            ids.sort();
            ids.reverse();
            ids
        });
        if let Ok(mut m) = MODELS.lock() {
            m.insert(id.to_string(), res);
            let ok: BTreeMap<&String, &Vec<String>> = m.iter().filter_map(|(k, v)| v.as_ref().ok().map(|v| (k, v))).collect();
            if let Some(f) = models_file() {
                let _ = std::fs::create_dir_all(f.parent().unwrap());
                let _ = std::fs::write(f, serde_json::to_string(&ok).unwrap_or_default());
            }
        }
        ctx.request_repaint();
    });
}

/// Ajoute les en-têtes d'authentification propres au fournisseur.
pub fn auth<B>(req: ureq::RequestBuilder<B>, provider_id: &str, key: &str) -> ureq::RequestBuilder<B> {
    let key = key.trim();
    if key.is_empty() {
        return req;
    }
    let req = req.header("Authorization", format!("Bearer {key}"));
    if provider_id == "anthropic" {
        req.header("x-api-key", key).header("anthropic-version", "2023-06-01")
    } else {
        req
    }
}

const MAX_CHARS: usize = 24_000;

#[derive(Clone)]
pub struct Settings {
    pub url: String,
    pub model: String,
    pub key: String,
    /// Laisser les modèles qui « réfléchissent » (Qwen 3.x, DeepSeek R1…) réfléchir avant de répondre.
    /// Plus juste, mais beaucoup plus lent.
    pub think: bool,
    /// Clés API par fournisseur (id → clé).
    pub keys: BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { url: "http://localhost:11434/v1".into(), model: "llama3.2".into(), key: String::new(), think: false, keys: BTreeMap::new() }
    }
}

fn file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("aryadne").join("ai.json"))
}

impl Settings {
    /// Id du fournisseur actif (« custom » si l'adresse est personnalisée).
    pub fn provider_id(&self) -> &'static str {
        provider_for_url(&self.url).map(|p| p.id).unwrap_or("custom")
    }

    /// Fournisseurs utilisables : locaux, ou avec une clé.
    pub fn connected(&self) -> Vec<&'static Provider> {
        PROVIDERS.iter().filter(|p| p.key_page.is_empty() || self.keys.get(p.id).is_some_and(|k| !k.trim().is_empty())).collect()
    }

    /// Passe à un modèle d'un fournisseur.
    pub fn use_model(&mut self, provider_id: &str, model: &str) {
        if let Some(p) = provider(provider_id) {
            self.url = p.url.to_string();
            self.key = self.keys.get(p.id).cloned().unwrap_or_default();
        }
        self.model = model.trim_end_matches(":latest").to_string();
    }

    pub fn load() -> Self {
        let d = Self::default();
        let Some(v) = file().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|s| serde_json::from_str::<Value>(&s).ok())
        else {
            return d;
        };
        let get = |k: &str, def: String| v[k].as_str().map(str::to_string).unwrap_or(def);
        let mut keys: BTreeMap<String, String> = serde_json::from_value(v["keys"].clone()).unwrap_or_default();
        let url = get("url", d.url);
        let key = get("key", d.key);
        // Ancienne config : une seule clé, rangée sous son fournisseur
        if let Some(p) = provider_for_url(&url) {
            if !key.is_empty() {
                keys.entry(p.id.to_string()).or_insert(key.clone());
            }
        }
        Self { url, model: get("model", d.model), key, think: v["think"].as_bool().unwrap_or(false), keys }
    }

    pub fn save(&self) {
        if let Some(f) = file() {
            let _ = std::fs::create_dir_all(f.parent().unwrap());
            let v = json!({ "url": self.url, "model": self.model, "key": self.key, "think": self.think, "keys": self.keys });
            let _ = std::fs::write(f, serde_json::to_string_pretty(&v).unwrap());
        }
    }
}

/// Lance le résumé dans un fil séparé pour ne pas figer l'interface.
pub fn summarize(settings: Settings, name: String, text: String, local: crate::ollama::Local, ctx: eframe::egui::Context) -> Receiver<Result<String, String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        // Ollama local installé mais arrêté : on le démarre d'abord.
        let ready = if crate::ollama::is_local(&settings.url) { local.ensure_running(&settings.url).map_err(|e| format!("{e} Ouvre les réglages ia pour l'installer.")) } else { Ok(()) };
        let _ = tx.send(ready.and_then(|_| request(&settings, &name, &text)));
        ctx.request_repaint();
    });
    rx
}

fn request(s: &Settings, name: &str, text: &str) -> Result<String, String> {
    let mut content: String = text.chars().take(MAX_CHARS).collect();
    if content.len() < text.len() {
        content.push_str("\n\n[...fichier tronqué...]");
    }
    complete(s, "Tu résumes des fichiers pour une base de notes personnelle. Réponds en français, en Markdown : une phrase d'ensemble, puis 3 à 7 points clés. Pour du code : ce que fait le fichier, ses parties principales et ses dépendances. Sois concis.", &format!("Fichier : {name}\n\n{content}"))
}

/// Une requête simple (non diffusée) au modèle configuré.
pub fn complete(s: &Settings, system: &str, user: &str) -> Result<String, String> {
    // Ollama local : interface native, sans « réflexion » (bien plus rapide), modèle gardé en mémoire
    if crate::ollama::is_local(&s.url) {
        let host = crate::ollama::host(&s.url);
        let mut body = json!({
            "model": s.model, "stream": false, "think": false, "keep_alive": "30m",
            "options": { "temperature": 0.2 },
            "messages": [ { "role": "system", "content": system }, { "role": "user", "content": user } ]
        });
        let send = |b: &Value| crate::ollama::agent(1800).post(&format!("{host}/api/chat")).send_json(b);
        let mut resp = match send(&body) {
            Ok(r) => r,
            Err(ureq::Error::StatusCode(400)) => {
                body.as_object_mut().map(|o| o.remove("think"));
                send(&body).map_err(|e| e.to_string())?
            }
            Err(ureq::Error::StatusCode(404)) => return Err(format!("Modèle « {} » introuvable. Télécharge-le dans les réglages ia.", s.model)),
            Err(e) => return Err(format!("Ollama : {e}")),
        };
        let v: Value = resp.body_mut().read_json().map_err(|e| e.to_string())?;
        if let Some(e) = v["error"].as_str() {
            return Err(e.to_string());
        }
        return v["message"]["content"].as_str().map(|t| t.trim().to_string()).ok_or_else(|| "Réponse vide du modèle.".into());
    }
    let body = json!({
        "model": s.model,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ]
    });
    let url = format!("{}/chat/completions", s.url.trim_end_matches('/'));
    let agent = crate::ollama::agent(180);
    let req = auth(agent.post(&url), s.provider_id(), &s.key);
    let mut resp = req.send_json(&body).map_err(|e| match e {
        ureq::Error::Io(_) | ureq::Error::ConnectionFailed | ureq::Error::HostNotFound => {
            format!("Impossible de joindre {url}. Le serveur IA est-il lancé ?")
        }
        ureq::Error::StatusCode(404) => format!("Modèle « {} » introuvable. Télécharge-le dans les réglages ia.", s.model),
        ureq::Error::StatusCode(401) => "Clé API refusée.".into(),
        other => other.to_string(),
    })?;
    let v: Value = resp.body_mut().read_json().map_err(|e| e.to_string())?;
    v["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.trim().to_string())
        .ok_or_else(|| format!("Réponse inattendue : {}", v.to_string().chars().take(200).collect::<String>()))
}
