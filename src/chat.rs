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

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Msg {
    pub role: Role,
    pub text: String,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub error: Option<String>,
    /// Noms des pièces jointes envoyées avec ce message.
    #[serde(default)]
    pub attached: Vec<String>,
    /// Aperçus des images jointes (JPEG en base64, 320 px), affichés dans la conversation
    #[serde(default)]
    pub thumbs: Vec<String>,
    /// Pièces jointes complètes, gardées en mémoire pour « modifier » et « régénérer »
    #[serde(skip)]
    pub sent: Vec<Attachment>,
}

/// Conversation enregistrée dans .app/chats/ du coffre.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Saved {
    pub title: String,
    pub msgs: Vec<Msg>,
}

pub fn chats_dir(root: &str) -> std::path::PathBuf {
    std::path::Path::new(root).join(".app/chats")
}

/// (id, titre, date de modification), les plus récentes d'abord.
pub fn list_saved(root: &str) -> Vec<(String, String, std::time::SystemTime)> {
    let mut v: Vec<(String, String, std::time::SystemTime)> = std::fs::read_dir(chats_dir(root))
        .map(|rd| {
            rd.flatten()
                .filter_map(|f| {
                    let name = f.file_name().to_string_lossy().to_string();
                    let id = name.strip_suffix(".json")?.to_string();
                    let t = f.metadata().ok()?.modified().ok()?;
                    let s: Saved = serde_json::from_str(&std::fs::read_to_string(f.path()).ok()?).ok()?;
                    Some((id, s.title, t))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| b.2.cmp(&a.2));
    v
}

/// Pièce jointe : image (envoyée au modèle s'il voit les images) ou texte.
#[derive(Clone)]
pub enum Attachment {
    Image { name: String, data_url: String, thumb: String },
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
        let small = img.thumbnail(320, 320);
        let mut tb = std::io::Cursor::new(Vec::new());
        let _ = small.to_rgb8().write_to(&mut tb, image::ImageFormat::Jpeg);
        let thumb = base64::engine::general_purpose::STANDARD.encode(tb.into_inner());
        Attachment::Image { name: name.to_string(), data_url: format!("data:image/jpeg;base64,{b64}"), thumb }
    }
}

fn is_text_path(p: &std::path::Path) -> bool {
    let e = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    TEXT_EXT.contains(&e.as_str()) || matches!(e.as_str(), "xml" | "svg" | "ini" | "cfg" | "log" | "gd" | "vue" | "svelte" | "php" | "rb" | "sql" | "bat" | "ps1" | "mjs" | "osl")
}

#[derive(Default)]
pub struct Live {
    /// Heure de la question (pour afficher le temps d'attente)
    pub asked: Option<std::time::Instant>,
    /// Réflexion du modèle (affichée à part, pas dans la réponse)
    pub thinking: String,
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
    /// Conversation enregistrée (id du fichier) et nombre de messages déjà enregistrés
    pub id: Option<String>,
    pub title: String,
    pub saved_len: usize,
}

impl Live {
    /// Texte d'attente : étape, secondes écoulées, et fin de la réflexion en cours.
    pub fn status_line(&self) -> Option<String> {
        if self.status.is_empty() {
            return None;
        }
        let secs = self.asked.map(|t| t.elapsed().as_secs()).unwrap_or(0);
        let mut s = format!("{} {secs} s", self.status);
        if !self.thinking.is_empty() {
            let tail: String = self.thinking.chars().rev().take(140).collect::<Vec<_>>().into_iter().rev().collect();
            s.push_str(&format!("\n… {}", tail.replace('\n', " ").trim()));
        }
        Some(s)
    }
}

impl Chat {
    /// Enregistre la conversation quand un nouveau message est arrivé.
    pub fn autosave(&mut self, root: &str) {
        if self.busy() || self.msgs.is_empty() || self.msgs.len() == self.saved_len {
            return;
        }
        let id = self.id.get_or_insert_with(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis().to_string()).unwrap_or_default()).clone();
        if self.title.is_empty() {
            self.title = self.msgs.iter().find(|m| m.role == Role::User).map(|m| m.text.chars().take(60).collect()).unwrap_or_default();
        }
        let dir = chats_dir(root);
        let _ = std::fs::create_dir_all(&dir);
        let s = Saved { title: self.title.clone(), msgs: self.msgs.clone() };
        if let Ok(t) = serde_json::to_string(&s) {
            let _ = std::fs::write(dir.join(format!("{id}.json")), t);
        }
        self.saved_len = self.msgs.len();
    }

    pub fn load_saved(&mut self, root: &str, id: &str) {
        if let Some(s) = std::fs::read_to_string(chats_dir(root).join(format!("{id}.json"))).ok().and_then(|t| serde_json::from_str::<Saved>(&t).ok()) {
            self.msgs = s.msgs;
            self.title = s.title;
            self.id = Some(id.to_string());
            self.saved_len = self.msgs.len();
        }
    }

    pub fn new_conversation(&mut self) {
        self.msgs.clear();
        self.id = None;
        self.title.clear();
        self.saved_len = 0;
    }

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
        let thumbs: Vec<String> = attachments.iter().filter_map(|a| if let Attachment::Image { thumb, .. } = a { Some(thumb.clone()) } else { None }).collect();
        self.msgs.push(Msg { role: Role::User, text: q.clone(), sources: vec![], error: None, attached, thumbs, sent: attachments.clone() });
        let history: Vec<Msg> = self.msgs.iter().rev().skip(1).take(8).rev().cloned().collect();
        let stop = Arc::new(AtomicBool::new(false));
        let live = Arc::new(Mutex::new(Live { status: "recherche dans le coffre…".into(), stop: stop.clone(), asked: Some(std::time::Instant::now()), ..Default::default() }));
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
            let local_model = crate::ollama::is_local(&settings.url);
            set(&move |l: &mut Live| {
                l.sources = src.clone();
                l.status = if local_model { "le modèle se charge et lit la demande…".into() } else { "réflexion…".into() };
            });
            let ready = if crate::ollama::is_local(&settings.url) { local.ensure_running(&settings.url) } else { Ok(()) };
            let docs = context_docs(&root, &entries, &q, &hits, open_file.as_ref());
            let result = ready.and_then(|_| {
                let mut messages = build_messages(&entries, &hits, open_file.as_ref(), &history, &q, &attachments, &docs);
                let full = std::sync::Arc::new(Mutex::new(String::new()));
                // Jusqu'à 3 tours : l'IA peut demander à lire des fichiers (« LIRE: chemin »)
                for round in 0..3 {
                    if let Ok(mut f) = full.lock() {
                        f.clear();
                    }
                    let on_think = |t: &str| {
                        let t = t.to_string();
                        set(&move |l: &mut Live| {
                            l.thinking.push_str(&t);
                            l.status = "réfléchit…".into();
                            l.started.get_or_insert_with(std::time::Instant::now);
                        });
                    };
                    let fl = full.clone();
                    let on_delta = |delta: &str| {
                        if let Ok(mut f) = fl.lock() {
                            f.push_str(delta);
                        }
                        let d = delta.to_string();
                        set(&move |l: &mut Live| {
                            l.text.push_str(&d);
                            l.status.clear();
                            l.started.get_or_insert_with(std::time::Instant::now);
                            l.chunks += 1;
                        });
                    };
                    if local_model {
                        stream_ollama(&settings, messages.clone(), &stop, &on_think, &on_delta)?;
                    } else {
                        stream(&settings, messages.clone(), &stop, &on_delta)?;
                    }
                    let answer = full.lock().map(|f| f.clone()).unwrap_or_default();
                    let wanted: Vec<String> = answer.lines().filter_map(|l| l.trim().strip_prefix("LIRE:").or_else(|| l.trim().strip_prefix("LIRE :")).map(|p| p.trim().trim_matches('`').to_string())).filter(|p| !p.is_empty()).take(4).collect();
                    if wanted.is_empty() || round == 2 || stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let mut provided = String::new();
                    for w in &wanted {
                        let rel = entries.iter().find(|e| !e.is_dir && (e.rel == *w || e.rel.ends_with(&format!("/{w}")) || crate::vault::stem(&e.rel) == *w)).map(|e| e.rel.clone());
                        match rel.and_then(|r| crate::vault::read(&root, &r).ok().map(|t| (r, t))) {
                            Some((r, t)) => provided.push_str(&format!("\n--- {r} ---\n{}\n", t.chars().take(12_000).collect::<String>())),
                            None => provided.push_str(&format!("\n--- {w} : introuvable ---\n")),
                        }
                    }
                    let names = wanted.join(", ");
                    set(&move |l: &mut Live| {
                        l.text.clear();
                        l.status = format!("lit {names}…");
                    });
                    messages.push(json!({ "role": "assistant", "content": answer }));
                    messages.push(json!({ "role": "user", "content": format!("Voici le contenu demandé :{provided}\nRéponds maintenant à ma question.") }));
                }
                Ok(())
            });
            set(&move |l: &mut Live| {
                if let Err(e) = &result {
                    l.error = Some(e.clone());
                } else if l.text.trim().is_empty() && !l.stop.load(Ordering::Relaxed) {
                    l.error = Some("Le modèle n'a rien répondu. Réessaie, ou choisis un modèle plus petit.".into());
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
            self.msgs.push(Msg { role: Role::Assistant, text, sources, error, attached: vec![], thumbs: vec![], sent: vec![] });
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
    TEXT_EXT.contains(&e.as_str()) && !crate::board::is_board(rel)
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

/// Contenu des fichiers utiles à la question : ceux qu'elle nomme, puis les notes (toutes si
/// le coffre est petit, sinon le début des plus récentes). Environ 24 000 caractères au plus.
fn context_docs(root: &str, entries: &[Entry], q: &str, hits: &[Hit], open: Option<&(String, String)>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    let mut budget: usize = 24_000;
    let qf = fold(q);
    let skip: HashSet<String> = hits.iter().map(|h| h.rel.clone()).chain(open.map(|o| o.0.clone())).collect();
    let push = |rel: &str, text: String, out: &mut Vec<(String, String)>, budget: &mut usize| {
        if *budget < 200 || out.iter().any(|(r, _)| r == rel) || skip.contains(rel) {
            return;
        }
        let t: String = text.chars().take((*budget).min(8000)).collect();
        *budget = budget.saturating_sub(t.len());
        out.push((rel.to_string(), t));
    };
    // 1. Fichiers nommés dans la question
    for e in entries.iter().filter(|e| !e.is_dir) {
        let stem = fold(&crate::vault::stem(&e.rel));
        if stem.chars().count() >= 3 && qf.contains(&stem) {
            if let Ok(t) = crate::vault::read(root, &e.rel) {
                push(&e.rel, t, &mut out, &mut budget);
            }
        }
    }
    // 2. Les notes : toutes si elles tiennent, sinon le début des plus récentes
    let mut notes: Vec<&Entry> = entries.iter().filter(|e| !e.is_dir && crate::vault::is_md(&e.rel) && !e.rel.starts_with("modèles/") && !e.rel.starts_with(".app")).collect();
    notes.sort_by(|a, b| b.modified.cmp(&a.modified));
    let texts: Vec<(String, String)> = notes.iter().take(80).filter_map(|e| crate::vault::read(root, &e.rel).ok().map(|t| (e.rel.clone(), t))).collect();
    let total: usize = texts.iter().map(|t| t.1.len()).sum();
    for (rel, t) in texts {
        let t = if total <= budget { t } else { t.chars().take(500).collect() };
        push(&rel, t, &mut out, &mut budget);
    }
    out
}

fn build_messages(entries: &[Entry], hits: &[Hit], open: Option<&(String, String)>, history: &[Msg], q: &str, attachments: &[Attachment], docs: &[(String, String)]) -> Vec<Value> {
    // Liste courte : un long contexte ralentit beaucoup les modèles locaux.
    let files: Vec<&str> = entries.iter().filter(|e| !e.is_dir && !e.rel.starts_with('@')).map(|e| e.rel.as_str()).take(120).collect();
    let mut sys = String::from(
        "Tu es l'assistant intégré à Aryadne, le logiciel de notes de l'utilisateur. Tu as accès à son coffre (un dossier de notes, de code et d'images).\n\
         Règles :\n\
         - Réponds en français, de façon concise et concrète, en Markdown simple.\n\
         - Appuie-toi sur les extraits fournis. Si l'information n'y est pas, dis-le et propose quel fichier ouvrir.\n\
         - Pour renvoyer vers une note .md, écris [[Nom de la note]] (sans .md). Pour un autre fichier, écris son chemin exact entre accents graves, par ex. `code/export.py`.\n\
         - Ne cite que des fichiers présents dans la liste ci-dessous.\n\
         - Quand tu proposes du code, donne-le complet dans un bloc ```langage, prêt à être inséré tel quel. Si une sélection est fournie, ta réponse porte d'abord sur elle.\n\
         - Si la demande est ambiguë ou qu'il te manque une information importante, pose d'abord UNE question courte au lieu de deviner. Tu peux proposer des réponses toutes faites sur une ligne qui commence par « ?> », séparées par « | », par ex. :\n?> Un résumé court | Un plan détaillé | Les deux\n\
         - Les pièces jointes (fichiers, dossiers, images) envoyées par l'utilisateur font partie de sa question.\n\
         - Si tu as besoin du contenu complet d'un fichier qui n'est pas fourni ci-dessous, réponds SEULEMENT par une ou plusieurs lignes « LIRE: chemin/du/fichier ». Le contenu te sera envoyé, puis tu répondras.\n\
         - Tu peux modifier un fichier (note, board .aryboard, code…) : donne son contenu COMPLET dans un bloc dont la première ligne est ```fichier:chemin/du/fichier (puis le contenu, puis ```). L'utilisateur pourra l'appliquer en un clic. Pour un board, garde le même format JSON et change seulement ce qui est demandé (par exemple les champs x et y des éléments).\n\n",
    );
    sys.push_str(&format!("Fichiers du coffre ({}) :\n{}\n", files.len(), files.join("\n")));
    if let Some((rel, text)) = open {
        let t: String = text.chars().take(6000).collect();
        sys.push_str(&format!("\nFichier actuellement ouvert : {rel}\n---\n{t}\n---\n"));
    }
    if !docs.is_empty() {
        sys.push_str("\nContenu de fichiers du coffre :\n");
        for (rel, t) in docs {
            sys.push_str(&format!("\n--- {rel} ---\n{t}\n"));
        }
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
            choices.extend(rest.split('|').map(|c| c.trim().trim_matches(|ch| ch == '`' || ch == '*' || ch == '"').trim().to_string()).filter(|c| !c.is_empty()));
        } else {
            kept.push(l);
        }
    }
    (kept.join("\n"), choices)
}

/// Ollama en local : on passe par son interface à lui (/api/chat). Elle permet de couper la
/// « réflexion » des modèles comme Qwen 3.x (beaucoup plus rapide), de voir cette réflexion
/// en direct si on la garde, et de garder le modèle chargé en mémoire 30 minutes.
fn stream_ollama(s: &crate::ai::Settings, messages: Vec<Value>, stop: &AtomicBool, on_think: &dyn Fn(&str), on_delta: &dyn Fn(&str)) -> Result<(), String> {
    let host = crate::ollama::host(&s.url);
    // Format OpenAI → format Ollama (images à part, en base64 sans préfixe)
    let msgs: Vec<Value> = messages
        .into_iter()
        .map(|m| {
            let role = m["role"].clone();
            match m["content"].as_array() {
                Some(parts) => {
                    let text: Vec<&str> = parts.iter().filter_map(|p| p["text"].as_str()).collect();
                    let images: Vec<String> = parts.iter().filter_map(|p| p["image_url"]["url"].as_str()).map(|u| u.split_once(',').map(|x| x.1).unwrap_or(u).to_string()).collect();
                    json!({ "role": role, "content": text.join("\n"), "images": images })
                }
                None => json!({ "role": role, "content": m["content"] }),
            }
        })
        .collect();
    let mut body = json!({ "model": s.model, "messages": msgs, "stream": true, "keep_alive": "30m", "options": { "temperature": 0.3 } });
    body["think"] = json!(s.think);
    let send = |body: &Value| crate::ollama::agent(1800).post(&format!("{host}/api/chat")).send_json(body);
    let mut resp = match send(&body) {
        Ok(r) => r,
        // Modèle sans « réflexion » : on réessaie sans le paramètre
        Err(ureq::Error::StatusCode(400)) => {
            body.as_object_mut().map(|o| o.remove("think"));
            send(&body).map_err(|e| e.to_string())?
        }
        Err(ureq::Error::StatusCode(404)) => return Err(format!("Modèle « {} » introuvable. Télécharge-le dans les réglages ia.", s.model)),
        Err(e) => return Err(format!("Ollama : {e}")),
    };
    let reader = std::io::BufReader::new(resp.body_mut().with_config().limit(u64::MAX).reader());
    for line in reader.lines() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let line = line.map_err(|e| e.to_string())?;
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if let Some(err) = v["error"].as_str() {
            return Err(err.to_string());
        }
        if let Some(t) = v["message"]["thinking"].as_str().filter(|t| !t.is_empty()) {
            on_think(t);
        }
        if let Some(d) = v["message"]["content"].as_str().filter(|d| !d.is_empty()) {
            on_delta(d);
        }
        if v["done"].as_bool() == Some(true) {
            break;
        }
    }
    Ok(())
}

/// Requête en streaming (Server-Sent Events) vers une API compatible OpenAI.
fn stream(s: &crate::ai::Settings, messages: Vec<Value>, stop: &AtomicBool, on_delta: &dyn Fn(&str)) -> Result<(), String> {
    // Claude : son API native (le format OpenAI de compatibilité suit mal les nouveaux modèles)
    if s.provider_id() == "anthropic" {
        let mut f = |d: &str| on_delta(d);
        return crate::ai::anthropic_stream(s, &messages, stop, &mut f);
    }
    let url = format!("{}/chat/completions", s.url.trim_end_matches('/'));
    let mut body = json!({ "model": s.model, "messages": messages, "stream": true });
    // Les modèles récents refusent une température personnalisée
    if crate::ai::sampling_ok(&s.model) {
        body["temperature"] = json!(0.3);
    }
    let agent = crate::ollama::agent(600);
    let mut resp = crate::ai::post_json(&agent, &url, s.provider_id(), &s.key, &body).map_err(|e| crate::ai::http_error(e, &url, &s.model))?;
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
    fn blocs_fichier() {
        let t = "Voici :\n```fichier:boards/a.board.json\n{\"x\": 1}\n```\nfin\n```rust\nfn a(){}\n```";
        let b = file_blocks(t);
        assert_eq!(b, vec![("boards/a.board.json".to_string(), "{\"x\": 1}\n".to_string())]);
        let (_, c) = split_choices("ok\n?> `notes/a.md` | **Les deux**");
        assert_eq!(c, vec!["notes/a.md".to_string(), "Les deux".to_string()]);
    }

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


/// Blocs ```fichier:chemin proposés par l'IA : (chemin, nouveau contenu).
pub fn file_blocks(text: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut cur: Option<(String, String)> = None;
    for line in text.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("```") {
            match cur.take() {
                Some((p, c)) => out.push((p, c)),
                None => {
                    if let Some(p) = rest.trim().strip_prefix("fichier:").or_else(|| rest.trim().strip_prefix("file:")) {
                        cur = Some((p.trim().to_string(), String::new()));
                    }
                }
            }
            continue;
        }
        if let Some((_, c)) = &mut cur {
            c.push_str(line);
            c.push('\n');
        }
    }
    out
}
