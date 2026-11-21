//! Les formats de fichiers propres à Aryadne, réunis en un seul endroit.
//!
//! Règle de nommage : `.ary` suivi d'un mot court qui dit ce que contient le fichier
//! (`.aryscene`, `.arymat`…). Pour ajouter un format, il suffit d'une ligne dans `ALL` :
//! la fenêtre « nouveau fichier », les filtres et la conversion des anciens noms suivent.
//! Les anciennes extensions restent lisibles ; `upgrade` donne le nouveau nom d'un ancien fichier.

pub struct Format {
    /// Nom interne, stable
    pub id: &'static str,
    /// Extension actuelle, sans le point
    pub ext: &'static str,
    /// Anciennes extensions encore acceptées à l'ouverture
    pub legacy: &'static [&'static str],
    pub label: &'static str,
    pub help: &'static str,
    /// Proposé dans « nouveau fichier » (faux tant que l'éditeur n'existe pas)
    pub ready: bool,
}

pub const ALL: [Format; 6] = [
    Format { id: "scene", ext: "aryscene", legacy: &["arya3"], label: "scène 3D", help: "Modèles, lumières, caméras et animation ; arrive avec un cube, une lumière et une caméra", ready: true },
    Format { id: "seq", ext: "aryseq", legacy: &["aryav"], label: "montage vidéo", help: "Timeline de vidéos, d'images, de sons et de titres", ready: true },
    Format { id: "mat", ext: "arymat", legacy: &["mat.json"], label: "matériau en nodes", help: "Matériau pour les modèles et les objets d'une scène", ready: true },
    Format { id: "comp", ext: "arycomp", legacy: &["comp.json"], label: "compositing en nodes", help: "Retouche d'image ou étalonnage d'un clip", ready: true },
    Format { id: "board", ext: "aryboard", legacy: &["board.json"], label: "board", help: "Planche d'images, de notes et de liens", ready: true },
    Format { id: "image", ext: "aryimg", legacy: &["arya2"], label: "image 2D", help: "Image à calques (à venir)", ready: false },
];

pub fn get(id: &str) -> &'static Format {
    ALL.iter().find(|f| f.id == id).unwrap_or(&ALL[0])
}

/// Extension actuelle d'un format (sans le point).
pub fn ext(id: &str) -> &'static str {
    get(id).ext
}

fn suffix_len(rel: &str, f: &Format) -> Option<usize> {
    let l = rel.to_lowercase();
    std::iter::once(&f.ext).chain(f.legacy.iter()).find(|e| l.ends_with(&format!(".{e}"))).map(|e| e.len() + 1)
}

/// Le format de ce fichier, s'il est propre à Aryadne (extension actuelle ou ancienne).
pub fn of(rel: &str) -> Option<&'static Format> {
    ALL.iter().find(|f| suffix_len(rel, f).is_some())
}

pub fn has(rel: &str, id: &str) -> bool {
    of(rel).is_some_and(|f| f.id == id)
}

/// Le nom sans son extension Aryadne (« Forêt.aryboard » → « Forêt »).
pub fn strip(name: &str) -> &str {
    match ALL.iter().find_map(|f| suffix_len(name, f)) {
        Some(n) => &name[..name.len() - n],
        None => name,
    }
}

/// Nouveau nom d'un fichier qui porte encore une ancienne extension (None s'il est à jour ou d'un autre genre).
pub fn upgrade(rel: &str) -> Option<String> {
    let f = of(rel)?;
    if rel.to_lowercase().ends_with(&format!(".{}", f.ext)) {
        return None;
    }
    Some(format!("{}.{}", strip(rel), f.ext))
}

/// Après un renommage : corrige les fichiers qui citent les anciens noms (scènes, montages, réglages d'édition
/// des modèles). Renvoie le nombre de fichiers corrigés et le nombre de fichiers lus.
pub fn relink(root: &std::path::Path, renamed: &[(String, String)]) -> (usize, usize) {
    // On remplace le nom du fichier seul : les chemins sont tantôt relatifs au coffre, tantôt au dossier voisin
    let names: Vec<(String, String)> = renamed.iter().map(|(a, b)| (a.rsplit('/').next().unwrap_or(a).to_string(), b.rsplit('/').next().unwrap_or(b).to_string())).collect();
    let (mut fixed, mut read) = (0, 0);
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let name = p.to_string_lossy().to_string();
            // Seuls les fichiers qui peuvent citer d'autres fichiers : les nôtres, et les réglages rangés dans .app
            let ours = of(&name).is_some() || (name.replace('\\', "/").contains("/.app/") && name.ends_with(".json"));
            if !ours {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            read += 1;
            let mut out = text.clone();
            for (a, b) in &names {
                out = out.replace(a.as_str(), b.as_str());
            }
            if out != text && std::fs::write(&p, out).is_ok() {
                fixed += 1;
            }
        }
    }
    (fixed, read)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions() {
        assert!(has("3d/Ville.aryscene", "scene") && has("3d/Ville.ARYA3", "scene"));
        assert!(has("nodes/Or.arymat", "mat") && has("nodes/Or.mat.json", "mat") && !has("nodes/Or.json", "mat"));
        assert_eq!(strip("Forêt.board.json"), "Forêt");
        assert_eq!(strip("Forêt.aryboard"), "Forêt");
        assert_eq!(strip("notes.md"), "notes.md");
        assert_eq!(upgrade("nodes/Or.mat.json").as_deref(), Some("nodes/Or.arymat"));
        assert_eq!(upgrade("a/Film.aryav").as_deref(), Some("a/Film.aryseq"));
        assert_eq!(upgrade("a/Film.aryseq"), None);
        assert_eq!(upgrade("a/data.json"), None);
        // Un fichier qui cite un ancien nom suit le renommage
        let dir = std::env::temp_dir().join("aryadne-formats-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("3d")).unwrap();
        std::fs::write(dir.join("3d/Ville.aryscene"), r#"{"objects":[{"material":"nodes/Or.mat.json"}]}"#).unwrap();
        std::fs::write(dir.join("notes.md"), "voir Or.mat.json").unwrap();
        let (fixed, _) = relink(&dir, &[("nodes/Or.mat.json".into(), "nodes/Or.arymat".into())]);
        assert_eq!(fixed, 1);
        assert!(std::fs::read_to_string(dir.join("3d/Ville.aryscene")).unwrap().contains("nodes/Or.arymat"));
        assert!(std::fs::read_to_string(dir.join("notes.md")).unwrap().contains("Or.mat.json"));
        // Chaque extension est unique, commence par « ary » et ne contient pas de point
        for (i, f) in ALL.iter().enumerate() {
            assert!(f.ext.starts_with("ary") && !f.ext.contains('.'));
            assert!(ALL.iter().skip(i + 1).all(|g| g.ext != f.ext && g.id != f.id));
        }
    }
}
