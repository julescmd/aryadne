//! Espaces de travail : chacun garde sa disposition de panneaux, ses onglets ouverts,
//! sa vue et sa couleur. On passe de l'un à l'autre par la barre sous le menu ;
//! tout est enregistré dans `~/.config/aryadne/workspaces.json`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Workspace {
    pub name: String,
    /// Couleur propre à l'espace ; None = celle des réglages
    #[serde(default)]
    pub accent: Option<[u8; 3]>,
    #[serde(default)]
    pub places: HashMap<String, crate::dock::Place>,
    #[serde(default)]
    pub tabs: Vec<String>,
    #[serde(default)]
    pub active: Option<String>,
    #[serde(default)]
    pub split: Option<String>,
    /// Onglets sortis dans leur propre panneau
    #[serde(default)]
    pub panes: Vec<String>,
    /// Vue affichée : files, brain, refs, boards, tasks, chat, search
    #[serde(default)]
    pub view: String,
    #[serde(default)]
    pub assistant: bool,
    /// Explorateur de fichiers ouvert (Ctrl+B)
    #[serde(default = "yes")]
    pub explorer: bool,
    /// Fenêtres de pages ajoutées par le menu « fenêtre » (cerveau#1, refs#2…)
    #[serde(default)]
    pub views: Vec<String>,
    /// Ce à quoi sert l'espace : notes, 3d, 2d, video, code, ai, refs, mat. Décide de l'écran d'accueil.
    #[serde(default)]
    pub kind: String,
    /// Disposition en cases de l'espace (None : celle de départ)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<crate::layout::Layout>,
}

fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
pub struct Set {
    pub current: usize,
    pub list: Vec<Workspace>,
    /// Espaces fournis d'origine déjà ajoutés (pour en proposer de nouveaux à une installation existante)
    #[serde(default)]
    pub seeded: u32,
}

fn path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("aryadne").join("workspaces.json"))
}

/// Les espaces fournis d'origine : (nom, couleur, vue, assistant ouvert, genre).
/// Les références ne sont plus un espace à part : chaque espace qui en a besoin a sa page « références ».
pub const PRESETS: [(&str, Option<[u8; 3]>, &str, bool, &str); 7] = [
    ("notes", None, "files", false, "notes"),
    ("3D", Some([0x3d, 0x9b, 0xff]), "files", false, "3d"),
    ("2D", Some([0xe8, 0x5d, 0xa8]), "files", false, "2d"),
    ("vidéo", Some([0x9b, 0x6d, 0xff]), "files", false, "video"),
    ("code", Some([0x3f, 0xc9, 0x7e]), "files", false, "code"),
    ("IA", Some([0xf2, 0xb5, 0x3a]), "chat", true, "ai"),
    ("matériaux", Some([0xe0, 0x7a, 0x3a]), "files", false, "mat"),
];

/// Les genres d'espaces connus d'Aryadne. Un espace d'un autre genre (fait à la main) accepte tout.
pub const KINDS: [&str; 7] = ["notes", "3d", "2d", "video", "code", "ai", "mat"];

/// Les pages de chaque espace, dans la deuxième rangée : (libellé, vue). Comme les jeux de menus de Maya,
/// elles changent avec l'espace ; la première est la page de travail (onglets de fichiers).
pub fn pages(kind: &str) -> &'static [(&'static str, &'static str)] {
    match kind {
        "3d" => &[("scène", "files"), ("références", "refs")],
        "2d" => &[("images", "files"), ("boards", "boards"), ("références", "refs")],
        "video" => &[("montage", "files"), ("médias", "refs")],
        "code" => &[("éditeur", "files"), ("recherche", "search")],
        "ai" => &[("chat", "chat"), ("recherche", "search")],
        "mat" => &[("nodes", "files"), ("références", "refs")],
        _ => &[("notes", "files"), ("cerveau", "brain"), ("tâches", "tasks"), ("recherche", "search")],
    }
}

/// Extensions de code et de texte brut : elles s'ouvrent dans l'espace « code ».
pub const CODE: [&str; 30] = ["rs", "py", "js", "ts", "tsx", "jsx", "html", "css", "json", "toml", "yaml", "yml", "c", "h", "cpp", "hpp", "java", "go", "lua", "sh", "glsl", "wgsl", "cs", "php", "rb", "swift", "kt", "sql", "xml", "ini"];

/// L'espace où un fichier a sa place.
pub fn owner(rel: &str) -> &'static str {
    let low = rel.to_lowercase();
    let ext = low.rsplit('.').next().unwrap_or("");
    if crate::nodes::is_graph(rel) {
        "mat"
    } else if crate::model3d::is_model(rel) || matches!(ext, "hdr" | "exr") {
        "3d"
    } else if crate::montage::is_project(rel) || crate::video::is_video(rel) || crate::formats::has(rel, "seq") || matches!(ext, "wav" | "mp3" | "ogg" | "flac" | "m4a" | "aac") {
        "video"
    } else if crate::board::is_board(rel) || crate::images::is_image(rel) || crate::formats::has(rel, "image") {
        "2d"
    } else if CODE.contains(&ext) && crate::formats::of(rel).is_none() {
        "code"
    } else {
        "notes"
    }
}

/// Un fichier peut-il s'ouvrir dans un espace de ce genre ?
pub fn accepts(kind: &str, rel: &str) -> bool {
    !KINDS.contains(&kind) || owner(rel) == kind
}

pub fn preset(i: usize) -> Workspace {
    let (name, accent, view, assistant, kind) = PRESETS[i];
    // La 3D garde toute la place pour la scène : l'explorateur s'ouvre au besoin (Ctrl+B)
    Workspace { name: name.to_string(), accent, view: view.to_string(), assistant, kind: kind.to_string(), explorer: kind != "3d", ..Default::default() }
}

impl Default for Set {
    fn default() -> Set {
        Set::load()
    }
}

impl Set {
    pub fn load() -> Set {
        let mut s: Set = path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_else(|| {
                // Première fois : l'espace « notes » reprend la disposition déjà en place
                let mut list: Vec<Workspace> = (0..PRESETS.len()).map(preset).collect();
                list[0].places = crate::dock::export();
                Set { current: 0, list, seeded: 2 }
            });
        // L'ancien espace « références » devient une page des autres espaces
        if s.seeded < 2 {
            let cur = s.list.get(s.current).map(|w| w.name.clone());
            s.list.retain(|w| w.kind != "refs" && !(w.kind.is_empty() && w.name == "références"));
            s.current = cur.and_then(|n| s.list.iter().position(|w| w.name == n)).unwrap_or(0);
        }
        // Installation d'avant les genres : on les retrouve par le nom, et l'espace « matériaux » s'ajoute
        for w in &mut s.list {
            if w.kind.is_empty() {
                w.kind = PRESETS.iter().find(|p| p.0 == w.name).map(|p| p.4).unwrap_or("notes").to_string();
            }
        }
        if s.seeded < 1 {
            if !s.list.iter().any(|w| w.kind == "mat") {
                s.list.push(preset(6));
            }
        }
        s.seeded = s.seeded.max(2);
        if s.list.is_empty() {
            s.list.push(preset(0));
        }
        s.current = s.current.min(s.list.len() - 1);
        s
    }

    pub fn save(&self) {
        if let (Some(p), Ok(d)) = (path(), serde_json::to_vec_pretty(self)) {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(p, d);
        }
    }

    pub fn cur(&self) -> &Workspace {
        &self.list[self.current]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn espaces_aller_retour() {
        let mut s = Set { current: 1, list: (0..PRESETS.len()).map(preset).collect(), seeded: 1 };
        s.list[1].tabs = vec!["modele.glb".into()];
        s.list[1].places.insert("side".into(), crate::dock::Place { side: crate::dock::Side::Window, pos: [1.0, 2.0], size: [300.0, 400.0] });
        let back: Set = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(back.current, 1);
        assert_eq!(back.list[1].tabs, vec!["modele.glb".to_string()]);
        assert!(back.list[1].places["side"].side == crate::dock::Side::Window);
        // Un ancien fichier sans les champs récents reste lisible
        let old: Set = serde_json::from_str(r#"{"current":5,"list":[{"name":"a"}]}"#).unwrap();
        assert_eq!(old.list[0].view, "");
        assert_eq!(preset(1).kind, "3d");
    }
}
