//! Recherche plein texte dans tout le coffre (dossiers liés compris), sans tenir
//! compte des majuscules ni des accents. Tourne en arrière-plan.
use crate::chat::fold;
use crate::vault::{self, Entry};
use std::sync::{Arc, Mutex};

const MAX_HITS: usize = 600;
const MAX_FILE: u64 = 2_000_000;

#[derive(Clone)]
pub struct Line {
    pub number: usize,
    pub text: String,
}

#[derive(Clone)]
pub struct FileHits {
    pub rel: String,
    pub name_match: bool,
    pub lines: Vec<Line>,
}

#[derive(Default)]
pub struct Results {
    pub query: String,
    pub files: Vec<FileHits>,
    pub done: bool,
    pub scanned: usize,
}

#[derive(Default)]
pub struct Search {
    pub input: String,
    pub results: Option<Arc<Mutex<Results>>>,
    pub last_query: String,
    /// Remplacer : le texte de remplacement (None : le mode est fermé)
    pub replace: Option<String>,
    /// Fichiers décochés : laissés tels quels au remplacement
    pub skip: std::collections::HashSet<String>,
}

fn searchable(rel: &str) -> bool {
    let e = rel.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    !crate::images::is_image(rel) && !crate::model3d::is_model(rel) && !["blend", "zip", "exe", "pdf", "fbx", "mp4", "mov", "psd", "kra"].contains(&e.as_str())
}

impl Search {
    /// Lance (ou relance) la recherche si le texte a changé.
    pub fn run(&mut self, root: String, entries: Vec<Entry>, ctx: eframe::egui::Context) {
        let q = self.input.trim().to_string();
        if q == self.last_query {
            return;
        }
        self.last_query = q.clone();
        if q.chars().count() < 2 {
            self.results = None;
            return;
        }
        let res = Arc::new(Mutex::new(Results { query: q.clone(), ..Default::default() }));
        self.results = Some(res.clone());
        std::thread::spawn(move || {
            let needle = fold(&q);
            let mut total = 0;
            for e in entries.iter().filter(|e| !e.is_dir) {
                // Une recherche plus récente a remplacé celle-ci : on arrête.
                if Arc::strong_count(&res) == 1 {
                    return;
                }
                let name_match = fold(&e.name).contains(&needle);
                let mut lines = vec![];
                if searchable(&e.rel) && total < MAX_HITS {
                    let big = vault::safe(&root, &e.rel).ok().and_then(|p| std::fs::metadata(p).ok()).is_none_or(|m| m.len() > MAX_FILE);
                    if !big {
                        if let Ok(text) = vault::read(&root, &e.rel) {
                            // « #tag » trouve aussi les tags de l'en-tête (« tags: [tag] »)
                            if let Some(tag) = q.strip_prefix('#') {
                                let t = fold(tag);
                                if vault::front_tags(&text).iter().any(|x| fold(x) == t) && !fold(&text).contains(&needle) {
                                    lines.push(Line { number: 1, text: format!("tags: {tag}") });
                                    total += 1;
                                }
                            }
                            for (i, line) in text.lines().enumerate() {
                                if fold(line).contains(&needle) {
                                    lines.push(Line { number: i + 1, text: line.trim().chars().take(220).collect() });
                                    total += 1;
                                    if lines.len() >= 40 || total >= MAX_HITS {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                if let Ok(mut r) = res.lock() {
                    r.scanned += 1;
                    if name_match || !lines.is_empty() {
                        r.files.push(FileHits { rel: e.rel.clone(), name_match, lines });
                    }
                }
            }
            if let Ok(mut r) = res.lock() {
                // Les fichiers dont le nom correspond passent en premier.
                r.files.sort_by(|a, b| b.name_match.cmp(&a.name_match).then(b.lines.len().cmp(&a.lines.len())));
                r.done = true;
            }
            ctx.request_repaint();
        });
    }
}

/// Découpe une ligne pour surligner les passages trouvés (insensible aux accents).
pub fn highlight(line: &str, query: &str) -> Vec<(String, bool)> {
    let needle = fold(query);
    let chars: Vec<char> = line.chars().collect();
    let folded: Vec<char> = fold(line).chars().collect();
    let n: Vec<char> = needle.chars().collect();
    if n.is_empty() || folded.len() != chars.len() {
        return vec![(line.to_string(), false)];
    }
    let mut out = vec![];
    let mut i = 0;
    let mut start = 0;
    while i + n.len() <= folded.len() {
        if folded[i..i + n.len()] == n[..] {
            if i > start {
                out.push((chars[start..i].iter().collect(), false));
            }
            out.push((chars[i..i + n.len()].iter().collect(), true));
            i += n.len();
            start = i;
        } else {
            i += 1;
        }
    }
    if start < chars.len() {
        out.push((chars[start..].iter().collect(), false));
    }
    out
}

/// Remplace chaque passage trouvé (même règle que la recherche : majuscules et accents ignorés).
/// Retourne le texte et le nombre de remplacements.
pub fn replace_in(text: &str, query: &str, with: &str) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut n = 0;
    // Ligne par ligne : la recherche aussi trouve ligne par ligne
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let parts = highlight(line, query);
        for (part, hit) in parts {
            if hit {
                out.push_str(with);
                n += 1;
            } else {
                out.push_str(&part);
            }
        }
    }
    (out, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remplacer_sans_accents() {
        let (t, n) = replace_in("La Forêt\nune foret\r\nrien", "foret", "jungle");
        assert_eq!(t, "La jungle\nune jungle\r\nrien");
        assert_eq!(n, 2);
        assert_eq!(replace_in("abc", "zz", "y"), ("abc".to_string(), 0));
    }

    #[test]
    fn surlignage_sans_accents() {
        let parts = highlight("La Forêt brumeuse", "foret");
        assert_eq!(parts, vec![("La ".to_string(), false), ("Forêt".to_string(), true), (" brumeuse".to_string(), false)]);
    }

    #[test]
    fn recherche_complete() {
        let dir = std::env::temp_dir().join("aryadne-search-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.md"), "rien\nla forêt est là\n").unwrap();
        std::fs::write(dir.join("foret.txt"), "titre seulement").unwrap();
        let root = dir.to_string_lossy().to_string();
        let entries: Vec<Entry> = vault::scan(&root).into_iter().filter(|e| !e.rel.starts_with('@')).collect();
        let mut s = Search { input: "FORET".into(), ..Default::default() };
        s.run(root, entries, eframe::egui::Context::default());
        let res = s.results.clone().unwrap();
        for _ in 0..100 {
            if res.lock().unwrap().done {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let r = res.lock().unwrap();
        assert_eq!(r.files.len(), 2);
        assert_eq!(r.files[0].rel, "foret.txt", "le nom de fichier passe en premier");
        assert_eq!(r.files[1].lines[0].number, 2);
    }
}
