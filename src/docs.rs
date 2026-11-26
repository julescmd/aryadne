//! La documentation d'Aryadne, embarquée dans le binaire (les pages Markdown de `docs/`, voir `build.rs`).
//! Menu Aide › Documentation : elle est copiée dans `<coffre>/.app/docs/` en gardant son arborescence,
//! puis son sommaire s'ouvre en lecture comme une note. Elle n'est réécrite que si la version d'Aryadne
//! (ou le contenu embarqué) a changé.

include!(concat!(env!("OUT_DIR"), "/docs_embed.rs"));

/// Dossier de la documentation dans le coffre, et sa page d'entrée.
pub const DIR: &str = ".app/docs";
pub const HOME: &str = ".app/docs/README.md";

/// Vrai pour une page de la documentation copiée dans le coffre.
pub fn is_doc(rel: &str) -> bool {
    rel.starts_with(".app/docs/")
}

/// Ce qui identifie la copie : la version d'Aryadne et une empreinte des pages embarquées.
fn stamp() -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for (rel, text) in FILES {
        rel.hash(&mut h);
        text.hash(&mut h);
    }
    format!("{} {:x}", env!("CARGO_PKG_VERSION"), h.finish())
}

/// Copie la documentation dans le coffre si elle n'y est pas déjà à cette version. Rend la page d'entrée.
pub fn install(root: &str) -> Result<String, String> {
    let dir = std::path::Path::new(root).join(DIR);
    let mark = dir.join(".version");
    let stamp = stamp();
    if std::fs::read_to_string(&mark).ok().as_deref() != Some(stamp.as_str()) || !dir.join("README.md").exists() {
        // Une page retirée de la documentation ne doit pas rester : on repart d'un dossier vide
        let _ = std::fs::remove_dir_all(&dir);
        for (rel, text) in FILES {
            let file = dir.join(rel);
            if let Some(d) = file.parent() {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            std::fs::write(&file, text).map_err(|e| e.to_string())?;
        }
        std::fs::write(&mark, &stamp).map_err(|e| e.to_string())?;
    }
    Ok(HOME.to_string())
}

/// Résout un lien relatif `[texte](../page.md#titre)` écrit dans la page `from` : le chemin dans le
/// coffre, et l'ancre s'il y en a une. Rien pour un lien qui sort du coffre.
pub fn resolve(from: &str, target: &str) -> Option<(String, Option<String>)> {
    let (path, anchor) = match target.split_once('#') {
        Some((p, a)) => (p, Some(a.to_string()).filter(|a| !a.is_empty())),
        None => (target, None),
    };
    if path.is_empty() {
        return Some((from.to_string(), anchor));
    }
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some((parts.join("/"), anchor))
}

/// L'ancre d'un titre, comme les écrivent les liens Markdown : minuscules, espaces en tirets, ponctuation retirée.
pub fn slug(heading: &str) -> String {
    heading.trim().to_lowercase().chars().filter_map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { Some(c) } else if c == ' ' { Some('-') } else { None }).collect()
}

/// Le titre d'un texte Markdown qui porte cette ancre.
pub fn heading_of(text: &str, anchor: &str) -> Option<String> {
    let mut fence = false;
    text.lines().find_map(|l| {
        let t = l.trim_start();
        if t.starts_with("```") {
            fence = !fence;
        }
        (!fence && t.starts_with('#')).then(|| t.trim_start_matches('#').trim().to_string()).filter(|h| slug(h) == anchor.to_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documentation_embarquee_et_copiee() {
        assert!(FILES.len() > 30 && FILES.iter().any(|f| f.0 == "README.md") && FILES.iter().any(|f| f.0 == "utilisateur/3d-atelier.md"));
        let dir = std::env::temp_dir().join(format!("aryadne-docs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.to_string_lossy().to_string();
        assert_eq!(install(&root).unwrap(), HOME);
        for (rel, text) in FILES {
            assert_eq!(&std::fs::read_to_string(dir.join(DIR).join(rel)).unwrap(), text, "{rel}");
        }
        // Même version : rien n'est réécrit (une page annotée à la main reste telle quelle)
        let page = dir.join(HOME);
        std::fs::write(&page, "annotée").unwrap();
        install(&root).unwrap();
        assert_eq!(std::fs::read_to_string(&page).unwrap(), "annotée");
        // Autre version : tout est recopié, et une page disparue de la documentation s'en va
        std::fs::write(dir.join(DIR).join(".version"), "0.0.0 0").unwrap();
        std::fs::write(dir.join(DIR).join("ancienne.md"), "x").unwrap();
        install(&root).unwrap();
        assert!(std::fs::read_to_string(&page).unwrap() != "annotée" && !dir.join(DIR).join("ancienne.md").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn liens_relatifs() {
        assert_eq!(resolve(".app/docs/README.md", "utilisateur/notes.md"), Some((".app/docs/utilisateur/notes.md".into(), None)));
        assert_eq!(resolve(".app/docs/utilisateur/3d-atelier.md", "../technique/noeuds.md#latelier--pile--graphe"), Some((".app/docs/technique/noeuds.md".into(), Some("latelier--pile--graphe".into()))));
        assert_eq!(resolve("a/b.md", "#titre"), Some(("a/b.md".into(), Some("titre".into()))));
        assert_eq!(resolve("b.md", "../../x.md"), None);
        assert_eq!(slug("L'Atelier : pile = graphe"), "latelier--pile--graphe");
        assert_eq!(heading_of("# A\n\n## L'Atelier : pile = graphe\n", "latelier--pile--graphe").as_deref(), Some("L'Atelier : pile = graphe"));
        // Tous les liens relatifs de la documentation mènent à une page embarquée, et leurs ancres à un titre
        let link = regex::Regex::new(r"\]\(([^)\s]+)\)").unwrap();
        let mut broken = vec![];
        for (rel, text) in FILES {
            let mut fence = false;
            for line in text.lines() {
                if line.trim_start().starts_with("```") {
                    fence = !fence;
                }
                if fence {
                    continue;
                }
                for c in link.captures_iter(line) {
                    let t = &c[1];
                    // Un lien vers un dossier reste du texte dans la visionneuse
                    if t.starts_with("http") || t.starts_with("mailto:") || t.ends_with('/') {
                        continue;
                    }
                    let ok = resolve(rel, t).is_some_and(|(page, anchor)| match FILES.iter().find(|f| f.0 == page) {
                        Some(f) => anchor.is_none_or(|a| heading_of(f.1, &a).is_some()),
                        None => false,
                    });
                    if !ok {
                        broken.push(format!("{rel} → {t}"));
                    }
                }
            }
        }
        assert!(broken.is_empty(), "liens cassés : {broken:#?}");
    }
}
