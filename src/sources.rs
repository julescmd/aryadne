//! D'où vient chaque fichier importé (adresse web ou chemin d'origine), gardé dans .app/sources.json.

use std::collections::HashMap;

fn path(root: &str) -> std::path::PathBuf {
    std::path::Path::new(root).join(".app/sources.json")
}

pub fn load(root: &str) -> HashMap<String, String> {
    std::fs::read_to_string(path(root)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Note la source d'un fichier du coffre
pub fn record(root: &str, rel: &str, src: &str) {
    let mut m = load(root);
    m.insert(rel.to_string(), src.to_string());
    if let Ok(t) = serde_json::to_string_pretty(&m) {
        let p = path(root);
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, t);
    }
}

pub fn get(root: &str, rel: &str) -> Option<String> {
    load(root).remove(rel)
}

#[cfg(test)]
mod tests {
    #[test]
    fn sources() {
        let dir = std::env::temp_dir().join(format!("ary-src-{}", std::process::id()));
        let root = dir.to_string_lossy().to_string();
        super::record(&root, "refs/a.png", "https://exemple.org/a.png");
        super::record(&root, "refs/b.jpg", "/home/x/b.jpg");
        assert_eq!(super::get(&root, "refs/a.png").as_deref(), Some("https://exemple.org/a.png"));
        assert_eq!(super::load(&root).len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
