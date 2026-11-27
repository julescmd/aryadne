//! Le moteur d'affichage de l'interface, et la carte qu'il partage avec les simulations.
//!
//! - **Deux moteurs.** `wgpu` (Vulkan, DirectX 12, Metal, ou OpenGL quand rien d'autre n'existe) est
//!   le moteur normal. `glow` (OpenGL pur) est le mode compatible : plus ancien, sans partage de
//!   carte, mais il démarre sur des machines où wgpu ne démarre pas.
//! - **Le choix** (`pick`) : la variable `ARY_RENDU` d'abord, puis le réglage, puis le souvenir d'un
//!   démarrage raté, sinon wgpu.
//! - **Le repli.** Si wgpu refuse de démarrer, `main` relance en mode compatible dans la foulée. Si
//!   le logiciel a été tué avant sa première image (pilote qui plante), un témoin resté sur le
//!   disque fait démarrer la fois suivante en mode compatible, et on le dit à l'artiste.
//! - **La carte partagée.** Avec wgpu, l'interface et les simulations (`gpusim`) utilisent le même
//!   appareil : une vue peut dessiner les tampons d'un solveur sans les relire vers le processeur.

use eframe::egui_wgpu;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Carte graphique moderne, partagée avec les simulations
    Wgpu,
    /// OpenGL : le mode compatible
    Glow,
}

/// Pourquoi ce moteur a été choisi (montré dans les réglages)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Why {
    Default,
    /// Variable d'environnement `ARY_RENDU`
    Env,
    /// Réglage de l'artiste
    Pref,
    /// Le dernier démarrage sur carte n'est pas allé jusqu'à sa première image
    Crashed,
    /// wgpu a refusé de démarrer cette fois-ci
    Failed,
}

/// Noms du réglage (`Prefs::renderer`) : vide = automatique
pub const AUTO: &str = "";
pub const WGPU: &str = "wgpu";
pub const GLOW: &str = "glow";

fn named(s: &str) -> Option<Kind> {
    match s.trim().to_lowercase().as_str() {
        "wgpu" | "carte" | "gpu" => Some(Kind::Wgpu),
        "glow" | "gl" | "opengl" | "compatible" => Some(Kind::Glow),
        _ => None,
    }
}

/// Le moteur à essayer. `env` : la variable `ARY_RENDU`. `pref` : le réglage. `mark` : contenu du
/// témoin de démarrage raté (la version qui a raté), s'il existe. `version` : la version qui démarre.
///
/// Un démarrage raté ne compte que pour la version qui l'a subi : une mise à jour réessaie la carte.
pub fn pick(env: Option<&str>, pref: &str, mark: Option<&str>, version: &str) -> (Kind, Why) {
    if let Some(k) = env.and_then(named) {
        return (k, Why::Env);
    }
    if let Some(k) = named(pref) {
        return (k, Why::Pref);
    }
    if mark.is_some_and(|m| m.trim() == version) {
        return (Kind::Glow, Why::Crashed);
    }
    (Kind::Wgpu, Why::Default)
}

/// Témoin écrit avant de démarrer sur carte, effacé après la première image
pub const STARTING: &str = "rendu-en-cours";
/// Témoin gardé quand un démarrage sur carte a raté : la version concernée
pub const FAILED: &str = "rendu-repli";

/// À appeler avant de choisir : un témoin `STARTING` encore là veut dire que le dernier démarrage
/// sur carte n'a jamais affiché d'image. Rend le contenu du témoin `FAILED` après mise à jour.
/// `dir` : le dossier de configuration.
pub fn carry_over(dir: &std::path::Path) -> Option<String> {
    let starting = dir.join(STARTING);
    if let Ok(v) = std::fs::read_to_string(&starting) {
        let _ = std::fs::write(dir.join(FAILED), v.trim());
        let _ = std::fs::remove_file(&starting);
    }
    std::fs::read_to_string(dir.join(FAILED)).ok().map(|s| s.trim().to_string())
}

/// Pose le témoin de démarrage (avant d'ouvrir la fenêtre sur carte)
pub fn mark_starting(dir: &std::path::Path, version: &str) {
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(dir.join(STARTING), version);
}

/// La première image est passée : le moteur marche, les témoins s'en vont
pub fn mark_started(dir: &std::path::Path, kind: Kind) {
    let _ = std::fs::remove_file(dir.join(STARTING));
    if kind == Kind::Wgpu {
        let _ = std::fs::remove_file(dir.join(FAILED));
    }
}

/// wgpu a refusé de démarrer : on s'en souvient pour ne pas réessayer à chaque lancement
pub fn mark_failed(dir: &std::path::Path, version: &str) {
    let _ = std::fs::remove_file(dir.join(STARTING));
    let _ = std::fs::write(dir.join(FAILED), version);
}

/// L'artiste a redemandé la carte : on oublie l'échec
pub fn forget_failure(dir: &std::path::Path) {
    let _ = std::fs::remove_file(dir.join(FAILED));
}

// ---------- ce qui tourne ----------

static ACTIVE: AtomicU8 = AtomicU8::new(0);
static WHY: AtomicU8 = AtomicU8::new(0);
static STATE: OnceLock<egui_wgpu::RenderState> = OnceLock::new();

const WHYS: [Why; 5] = [Why::Default, Why::Env, Why::Pref, Why::Crashed, Why::Failed];

/// Note le moteur qui a démarré, et garde la carte de l'interface si c'est wgpu.
pub fn started(kind: Kind, why: Why, state: Option<&egui_wgpu::RenderState>) {
    ACTIVE.store(if kind == Kind::Wgpu { 1 } else { 2 }, Ordering::Relaxed);
    WHY.store(WHYS.iter().position(|w| *w == why).unwrap_or(0) as u8, Ordering::Relaxed);
    if let (Kind::Wgpu, Some(s)) = (kind, state) {
        let _ = STATE.set(s.clone());
    }
}

static FRAMES: AtomicU8 = AtomicU8::new(0);
static START: OnceLock<std::time::Instant> = OnceLock::new();

/// À appeler tout au début de `main` : l'heure de départ, pour mesurer le délai jusqu'à la première image
pub fn clock() {
    let _ = START.set(std::time::Instant::now());
}

/// À appeler à chaque image de l'interface. À la deuxième, la première a été dessinée et montrée
/// en entier : le moteur marche, le témoin de démarrage s'en va. `dir` : le dossier de configuration.
pub fn frame_shown(dir: &std::path::Path) {
    if FRAMES.load(Ordering::Relaxed) >= 2 {
        return;
    }
    if FRAMES.fetch_add(1, Ordering::Relaxed) == 1 {
        if let Some((kind, why)) = active() {
            mark_started(dir, kind);
            if std::env::var_os("ARY_DBG").is_some() {
                let since = START.get().map(|t| t.elapsed().as_millis()).unwrap_or(0);
                eprintln!("affichage : {kind:?} ({why:?}), {}, première image après {since} ms", card().unwrap_or_else(|| "OpenGL".into()));
            }
        }
    }
}

/// Le moteur en service (`None` : pas d'interface, par exemple dans les tests ou `--banc-gpu`)
pub fn active() -> Option<(Kind, Why)> {
    let k = match ACTIVE.load(Ordering::Relaxed) {
        1 => Kind::Wgpu,
        2 => Kind::Glow,
        _ => return None,
    };
    Some((k, WHYS[(WHY.load(Ordering::Relaxed) as usize).min(WHYS.len() - 1)]))
}

/// La carte de l'interface, quand elle dessine avec wgpu
pub fn ui() -> Option<&'static egui_wgpu::RenderState> {
    STATE.get()
}

/// Nom lisible de la carte de l'interface et de son moteur (« NVIDIA RTX 4070 (Vulkan) »)
pub fn card() -> Option<String> {
    ui().map(|s| {
        let i = s.adapter.get_info();
        format!("{} ({:?})", i.name, i.backend)
    })
}

/// Réglages wgpu de l'interface : on demande à la carte tout ce qu'elle permet, parce que les
/// simulations travaillent sur le même appareil (les limites par défaut de WebGPU plafonneraient
/// un tissu vers 600 000 sommets, et le profil OpenGL par défaut n'a pas de calcul du tout).
pub fn wgpu_setup() -> egui_wgpu::WgpuSetup {
    let mut new = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    new.device_descriptor = std::sync::Arc::new(|adapter| wgpu::DeviceDescriptor { label: Some("aryadne"), required_limits: adapter.limits(), ..Default::default() });
    egui_wgpu::WgpuSetup::CreateNew(new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choix_du_moteur() {
        // Rien de particulier : la carte
        assert_eq!(pick(None, "", None, "1.0"), (Kind::Wgpu, Why::Default));
        // Le réglage décide, la variable d'environnement passe devant lui
        assert_eq!(pick(None, GLOW, None, "1.0"), (Kind::Glow, Why::Pref));
        assert_eq!(pick(Some("wgpu"), GLOW, None, "1.0"), (Kind::Wgpu, Why::Env));
        assert_eq!(pick(Some("OpenGL"), WGPU, None, "1.0"), (Kind::Glow, Why::Env));
        // Une valeur inconnue ne casse rien
        assert_eq!(pick(Some("vite"), "bidule", None, "1.0"), (Kind::Wgpu, Why::Default));
        // Démarrage raté avec cette version : mode compatible. Avec une autre version : on réessaie
        assert_eq!(pick(None, "", Some("1.0"), "1.0"), (Kind::Glow, Why::Crashed));
        assert_eq!(pick(None, "", Some("0.9"), "1.0"), (Kind::Wgpu, Why::Default));
        // L'artiste qui redemande la carte l'obtient malgré le témoin
        assert_eq!(pick(None, WGPU, Some("1.0"), "1.0"), (Kind::Wgpu, Why::Pref));
    }

    #[test]
    fn temoins_de_demarrage() {
        let dir = std::env::temp_dir().join(format!("aryadne-gfx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Démarrage normal : le témoin est posé puis retiré, rien ne reste
        assert_eq!(carry_over(&dir), None);
        mark_starting(&dir, "1.0");
        mark_started(&dir, Kind::Wgpu);
        assert_eq!(carry_over(&dir), None);
        // Tué avant la première image : la fois suivante le sait, et choisit le mode compatible
        mark_starting(&dir, "1.0");
        let mark = carry_over(&dir);
        assert_eq!(mark.as_deref(), Some("1.0"));
        assert_eq!(pick(None, "", mark.as_deref(), "1.0").0, Kind::Glow);
        // Le mode compatible qui démarre bien ne fait pas oublier l'échec de la carte
        mark_started(&dir, Kind::Glow);
        assert_eq!(carry_over(&dir).as_deref(), Some("1.0"));
        // L'artiste redemande la carte, elle démarre : tout est oublié
        forget_failure(&dir);
        assert_eq!(carry_over(&dir), None);
        mark_failed(&dir, "1.0");
        assert_eq!(carry_over(&dir).as_deref(), Some("1.0"));
        mark_starting(&dir, "1.0");
        mark_started(&dir, Kind::Wgpu);
        assert_eq!(carry_over(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
