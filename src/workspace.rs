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
    /// Dispositions enregistrées de l'étape (« Modelage », « Animation »…) et celle en cours
    #[serde(default)]
    pub dispositions: Vec<Disposition>,
    #[serde(default)]
    pub disposition: String,
    /// Outil ouvert dans l'espace (voir `tools`) ; vide : le premier
    #[serde(default)]
    pub tool: String,
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
/// Une disposition : l'arrangement des panneaux d'une étape, sous un nom
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Disposition {
    pub name: String,
    #[serde(default)]
    pub places: HashMap<String, crate::dock::Place>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<crate::layout::Layout>,
    #[serde(default = "yes")]
    pub explorer: bool,
    #[serde(default)]
    pub views: Vec<String>,
}

/// Dans l'ordre du pipeline : un espace par étape, ses outils en onglets (voir `tools`).
/// Les matériaux sont un outil de la 3D, le compositing un outil de la 2D ; l'IA est l'Assistant, partout.
pub const PRESETS: [(&str, Option<[u8; 3]>, &str, bool, &str); 7] = [
    ("notes", None, "files", false, "notes"),
    ("2D", Some([0xe8, 0x5d, 0xa8]), "files", false, "2d"),
    ("3D", Some([0x3d, 0x9b, 0xff]), "files", false, "3d"),
    ("motion", Some([0xd1, 0xaf, 0x75]), "files", false, "motion"),
    ("vidéo", Some([0x9b, 0x6d, 0xff]), "files", false, "video"),
    ("son", Some([0x7f, 0xb8, 0xbd]), "files", false, "son"),
    ("code", Some([0x3f, 0xc9, 0x7e]), "files", false, "code"),
];

/// Un outil d'un espace : identifiant, nom, icône, et la page qu'il ouvre (« files » : des fichiers en onglets)
pub struct Tool {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub view: &'static str,
}

/// Les outils de chaque espace, montrés dans l'onglet de l'espace actif
pub fn tools(kind: &str) -> &'static [Tool] {
    use crate::lucide as l;
    const NOTES: [Tool; 3] = [Tool { id: "notes", label: "Notes", icon: l::FILE_TEXT, view: "files" }, Tool { id: "tasks", label: "Tâches", icon: l::LIST_CHECKS, view: "tasks" }, Tool { id: "brain", label: "Cerveau", icon: l::NETWORK, view: "brain" }];
    const D2: [Tool; 4] = [Tool { id: "board", label: "Moodboard", icon: l::LAYOUT_DASHBOARD, view: "boards" }, Tool { id: "image", label: "Retouche", icon: l::IMAGE, view: "files" }, Tool { id: "vector", label: "Vectoriel", icon: l::PEN_TOOL, view: "files" }, Tool { id: "comp", label: "Compositing", icon: l::WORKFLOW, view: "files" }];
    const D3: [Tool; 2] = [Tool { id: "scene", label: "Scène", icon: l::BOX, view: "files" }, Tool { id: "mat", label: "Matériaux", icon: l::BLEND, view: "files" }];
    const MOTION: [Tool; 1] = [Tool { id: "motion", label: "Compos", icon: l::SPLINE, view: "files" }];
    const VIDEO: [Tool; 2] = [Tool { id: "seq", label: "Montage", icon: l::CLAPPERBOARD, view: "files" }, Tool { id: "review", label: "Revue", icon: l::MESSAGE_SQUARE, view: "files" }];
    const SON: [Tool; 2] = [Tool { id: "audio", label: "Audio", icon: l::AUDIO_LINES, view: "files" }, Tool { id: "music", label: "Musique", icon: l::MUSIC, view: "files" }];
    const CODE_T: [Tool; 1] = [Tool { id: "code", label: "Code", icon: l::CODE, view: "files" }];
    match kind {
        "2d" => &D2,
        "3d" => &D3,
        "motion" => &MOTION,
        "video" => &VIDEO,
        "son" => &SON,
        "code" => &CODE_T,
        "notes" => &NOTES,
        _ => &[],
    }
}

/// L'outil d'un fichier (dans l'espace qui le possède)
pub fn tool_of(rel: &str) -> &'static str {
    let low = rel.to_lowercase();
    let ext = low.rsplit('.').next().unwrap_or("");
    match owner(rel) {
        "2d" if crate::board::is_board(rel) => "board",
        "2d" if crate::formats::has(rel, "comp") => "comp",
        "2d" if crate::formats::has(rel, "vector") => "vector",
        "2d" => "image",
        "3d" if crate::formats::has(rel, "mat") => "mat",
        "3d" => "scene",
        "motion" => "motion",
        "video" if crate::formats::has(rel, "seq") => "seq",
        "video" => "review",
        "son" if crate::formats::has(rel, "music") || crate::midi::is_midi(rel) => "music",
        "son" => "audio",
        "code" => "code",
        _ => {
            let _ = ext;
            "notes"
        }
    }
}

/// Couleur d'un espace : celle choisie à la main, sinon celle de son genre dans le thème de la
/// charte (les couleurs vives d'origine des espaces fournis suivent aussi le thème). Les notes
/// gardent la couleur principale du thème (None).
pub fn accent_of(w: &Workspace) -> Option<[u8; 3]> {
    let preset = PRESETS.iter().find(|p| p.4 == w.kind).and_then(|p| p.1);
    if w.accent.is_some() && w.accent != preset {
        return w.accent;
    }
    if w.kind == "notes" || w.kind.is_empty() {
        return None;
    }
    crate::theme::ws_color(&w.kind).map(|c| [c.r(), c.g(), c.b()])
}

/// Les genres d'espaces connus d'Aryadne. Un espace d'un autre genre (fait à la main) accepte tout.
pub const KINDS: [&str; 7] = ["notes", "2d", "3d", "motion", "video", "son", "code"];

/// Les pages de chaque espace, dans la deuxième rangée : (libellé, vue). Comme les jeux de menus de Maya,
/// elles changent avec l'espace ; la première est la page de travail (onglets de fichiers).
pub fn pages(kind: &str) -> &'static [(&'static str, &'static str)] {
    match kind {
        "3d" => &[("fichiers", "files"), ("références", "refs")],
        "2d" => &[("fichiers", "files"), ("références", "refs")],
        "video" => &[("fichiers", "files"), ("médias", "refs")],
        "motion" => &[("fichiers", "files"), ("références", "refs")],
        "son" => &[("fichiers", "files"), ("recherche", "search")],
        "code" => &[("fichiers", "files"), ("recherche", "search")],
        _ => &[("fichiers", "files"), ("recherche", "search")],
    }
}

/// Toutes les pages d'un espace : celles du rail et celles de ses outils
pub fn has_view(kind: &str, view: &str) -> bool {
    pages(kind).iter().any(|(_, v)| *v == view) || tools(kind).iter().any(|t| t.view == view)
}

/// Extensions de code et de texte brut : elles s'ouvrent dans l'espace « code ».
pub const CODE: [&str; 30] = ["rs", "py", "js", "ts", "tsx", "jsx", "html", "css", "json", "toml", "yaml", "yml", "c", "h", "cpp", "hpp", "java", "go", "lua", "sh", "glsl", "wgsl", "cs", "php", "rb", "swift", "kt", "sql", "xml", "ini"];

/// L'espace où un fichier a sa place.
pub fn owner(rel: &str) -> &'static str {
    let low = rel.to_lowercase();
    let ext = low.rsplit('.').next().unwrap_or("");
    if crate::formats::has(rel, "comp") {
        "2d"
    } else if crate::formats::has(rel, "motion") {
        "motion"
    } else if crate::formats::has(rel, "audio") || crate::formats::has(rel, "music") || crate::audio::is_audio(rel) || crate::midi::is_midi(rel) {
        "son"
    } else if crate::formats::has(rel, "mat") {
        "3d"
    } else if crate::model3d::is_model(rel) || matches!(ext, "hdr" | "exr") {
        "3d"
    } else if crate::montage::is_project(rel) || crate::video::is_video(rel) || crate::formats::has(rel, "seq") || matches!(ext, "wav" | "mp3" | "ogg" | "flac" | "m4a" | "aac") {
        "video"
    } else if crate::board::is_board(rel) || crate::images::is_image(rel) || crate::formats::has(rel, "image") || crate::formats::has(rel, "vector") {
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
    // La 3D et le motion gardent toute la place pour la vue : l'explorateur s'ouvre au besoin (Ctrl+B)
    Workspace { name: name.to_string(), accent, view: view.to_string(), assistant, kind: kind.to_string(), explorer: !matches!(kind, "3d" | "motion" | "son"), ..Default::default() }
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
                Set { current: 0, list, seeded: 5 }
            });
        s.migrate();
        s
    }

    /// Remet un jeu d'espaces d'une ancienne version à jour
    pub fn migrate(&mut self) {
        let s = self;
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
            // (ancien espace « matériaux » : devenu un outil de la 3D)
        }
        // Espaces d'une étape du pipeline : l'IA et les matériaux ne sont plus des espaces. Les onglets
        // des matériaux passent dans la 3D (compositing dans la 2D), et les espaces fournis prennent l'ordre
        // du pipeline (notes, 2D, 3D, vidéo, code) ; les espaces faits à la main suivent
        if s.seeded < 3 {
            let cur = s.list.get(s.current).map(|w| w.name.clone());
            let mut moved: Vec<String> = vec![];
            for w in s.list.iter().filter(|w| w.kind == "mat" || w.kind == "ai") {
                moved.extend(w.tabs.iter().cloned());
            }
            s.list.retain(|w| w.kind != "mat" && w.kind != "ai");
            for t in moved {
                if let Some(w) = s.list.iter_mut().find(|w| w.kind == owner(&t)) {
                    if !w.tabs.contains(&t) {
                        w.tabs.push(t);
                    }
                }
            }
            let rank = |w: &Workspace| KINDS.iter().position(|k| *k == w.kind && PRESETS.iter().any(|p| p.0 == w.name)).unwrap_or(KINDS.len());
            s.list.sort_by_key(rank);
            s.current = cur.and_then(|n| s.list.iter().position(|w| w.name == n)).unwrap_or(0);
        }
        s.seeded = s.seeded.max(3);
        // Nouvelle étape Motion, entre la 3D et la vidéo
        if s.seeded < 4 {
            if !s.list.iter().any(|w| w.kind == "motion") {
                let at = s.list.iter().position(|w| w.kind == "video" || w.kind == "code").unwrap_or(s.list.len());
                let cur = s.list.get(s.current).map(|w| w.name.clone());
                s.list.insert(at, preset(PRESETS.iter().position(|p| p.4 == "motion").unwrap_or(0)));
                s.current = cur.and_then(|n| s.list.iter().position(|w| w.name == n)).unwrap_or(0);
            }
            s.seeded = 4;
        }
        // Nouvelle étape Son, entre la vidéo et le code
        if s.seeded < 5 {
            if !s.list.iter().any(|w| w.kind == "son") {
                let at = s.list.iter().position(|w| w.kind == "code").unwrap_or(s.list.len());
                let cur = s.list.get(s.current).map(|w| w.name.clone());
                s.list.insert(at, preset(PRESETS.iter().position(|p| p.4 == "son").unwrap_or(0)));
                s.current = cur.and_then(|n| s.list.iter().position(|w| w.name == n)).unwrap_or(0);
            }
            s.seeded = 5;
        }
        if s.list.is_empty() {
            s.list.push(preset(0));
        }
        s.current = s.current.min(s.list.len() - 1);
    }

    pub fn save(&self) {
        // Une fenêtre secondaire (un espace ouvert à part) ne réécrit pas les espaces de la principale
        if SECONDARY.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        if let (Some(p), Ok(d)) = (path(), serde_json::to_vec_pretty(self)) {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = crate::vault::atomic_write(p, d);
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
        assert_eq!(preset(2).kind, "3d");
        // Chaque fichier a son espace et son outil
        assert_eq!((owner("a/Or.arymat"), tool_of("a/Or.arymat")), ("3d", "mat"));
        assert_eq!((owner("a/G.arycomp"), tool_of("a/G.arycomp")), ("2d", "comp"));
        assert_eq!(tool_of("plan.mp4"), "review");
        assert_eq!(tool_of("Teaser.aryseq"), "seq");
        assert_eq!((owner("Intro.arymotion"), tool_of("Intro.arymotion")), ("motion", "motion"));
        assert_eq!((owner("voix.wav"), tool_of("Voix.aryaudio")), ("son", "audio"));
        assert_eq!((owner("Thème.arymusic"), tool_of("Thème.arymusic")), ("son", "music"));
        assert_eq!(tool_of("robot.glb"), "scene");
        assert_eq!(tool_of("croquis.png"), "image");
        assert!(tools("2d").iter().any(|t| t.view == "boards"));
        assert!(has_view("notes", "brain") && !has_view("3d", "brain"));
        // Un ancien jeu d'espaces : l'IA et les matériaux disparaissent, leurs onglets passent ailleurs
        let mut old = Set { current: 0, list: vec![preset(0)], seeded: 2 };
        let mut mat = Workspace { name: "matériaux".into(), kind: "mat".into(), ..Default::default() };
        mat.tabs = vec!["nodes/Or.arymat".into()];
        old.list.push(mat);
        old.list.push(Workspace { name: "IA".into(), kind: "ai".into(), ..Default::default() });
        old.list.push(preset(2));
        old.migrate();
        let kinds: Vec<&str> = old.list.iter().map(|w| w.kind.as_str()).collect();
        // … et l'étape Motion arrive après la 3D
        assert_eq!(kinds, vec!["notes", "3d", "motion", "son"]);
        assert_eq!(old.list[1].tabs, vec!["nodes/Or.arymat".to_string()]);
    }
}

/// Vrai dans une fenêtre lancée par « ouvrir dans une nouvelle fenêtre ».
pub static SECONDARY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Ouvre l'espace `index` du coffre `vault` dans une nouvelle fenêtre de l'application
/// (par exemple les matériaux sur un écran et la scène sur l'autre).
pub fn open_in_window(vault: &str, index: usize) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe).arg("--vault").arg(vault).arg("--workspace").arg(index.to_string()).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Arguments de lancement : coffre et espace de travail à ouvrir (fenêtre secondaire).
pub fn launch_args() -> (Option<String>, Option<usize>) {
    let args: Vec<String> = std::env::args().collect();
    let get = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    (get("--vault"), get("--workspace").and_then(|w| w.parse().ok()))
}
