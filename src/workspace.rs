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
}

#[derive(Serialize, Deserialize)]
pub struct Set {
    pub current: usize,
    pub list: Vec<Workspace>,
}

fn path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("aryadne").join("workspaces.json"))
}

/// Les espaces fournis d'origine : (nom, couleur, vue, assistant ouvert).
pub const PRESETS: [(&str, Option<[u8; 3]>, &str, bool); 7] = [
    ("notes", None, "files", false),
    ("3D", Some([0x3d, 0x9b, 0xff]), "files", false),
    ("2D", Some([0xe8, 0x5d, 0xa8]), "files", false),
    ("vidéo", Some([0x9b, 0x6d, 0xff]), "files", false),
    ("code", Some([0x3f, 0xc9, 0x7e]), "files", false),
    ("IA", Some([0xf2, 0xb5, 0x3a]), "chat", true),
    ("références", Some([0x2c, 0xc4, 0xc4]), "refs", false),
];

pub fn preset(i: usize) -> Workspace {
    let (name, accent, view, assistant) = PRESETS[i];
    Workspace { name: name.to_string(), accent, view: view.to_string(), assistant, ..Default::default() }
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
                Set { current: 0, list }
            });
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
        let mut s = Set { current: 1, list: (0..PRESETS.len()).map(preset).collect() };
        s.list[1].tabs = vec!["modele.glb".into()];
        s.list[1].places.insert("side".into(), crate::dock::Place { side: crate::dock::Side::Window, pos: [1.0, 2.0], size: [300.0, 400.0] });
        let back: Set = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(back.current, 1);
        assert_eq!(back.list[1].tabs, vec!["modele.glb".to_string()]);
        assert!(back.list[1].places["side"].side == crate::dock::Side::Window);
        // Un ancien fichier sans les champs récents reste lisible
        let old: Set = serde_json::from_str(r#"{"current":5,"list":[{"name":"a"}]}"#).unwrap();
        assert_eq!(old.list[0].view, "");
    }
}
