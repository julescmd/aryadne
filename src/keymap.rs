//! Raccourcis globaux modifiables : une seule carte, enregistrée dans keymap.json (réglages),
//! avec détection des conflits. Les éditeurs (3D, montage, nodes…) gardent leurs touches à eux.
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use std::collections::BTreeMap;
use std::sync::{LazyLock, RwLock};

/// Une action globale : identifiant stable, libellé, raccourcis par défaut.
pub struct Action {
    pub id: &'static str,
    pub label: &'static str,
    pub default: &'static [(Modifiers, Key)],
}

const C: Modifiers = Modifiers::COMMAND;
const CS: Modifiers = Modifiers { shift: true, ..Modifiers::COMMAND };

pub const ACTIONS: &[Action] = &[
    Action { id: "palette", label: "palette de commandes", default: &[(C, Key::K), (CS, Key::P)] },
    Action { id: "switcher", label: "ouvrir un fichier, ou créer une note", default: &[(C, Key::P)] },
    Action { id: "new_note", label: "nouvelle note", default: &[(C, Key::N)] },
    Action { id: "daily", label: "note du jour", default: &[(C, Key::J)] },
    Action { id: "search", label: "recherche dans tout le coffre", default: &[(CS, Key::F)] },
    Action { id: "replace_all", label: "remplacer dans tous les fichiers", default: &[(CS, Key::H)] },
    Action { id: "assistant", label: "assistant IA", default: &[(C, Key::L)] },
    Action { id: "explorer", label: "afficher / masquer l'explorateur", default: &[(C, Key::B)] },
    Action { id: "split", label: "vue côte à côte", default: &[(C, Key::Backslash)] },
    Action { id: "compact", label: "mode compact", default: &[(CS, Key::M)] },
    Action { id: "terminal", label: "terminal", default: &[(C, Key::Backtick)] },
    Action { id: "save_copy", label: "enregistrer une copie sous…", default: &[(CS, Key::S)] },
    Action { id: "settings", label: "réglages", default: &[(C, Key::Comma)] },
    Action { id: "close_tab", label: "fermer l'onglet", default: &[(C, Key::W)] },
    Action { id: "next_tab", label: "onglet suivant", default: &[(C, Key::Tab)] },
    Action { id: "prev_tab", label: "onglet précédent", default: &[(CS, Key::Tab)] },
    Action { id: "next_ws", label: "espace de travail suivant", default: &[(C, Key::PageDown)] },
    Action { id: "prev_ws", label: "espace de travail précédent", default: &[(C, Key::PageUp)] },
];

/// Raccourcis choisis par l'utilisateur, par action (absent = ceux par défaut)
static USER: LazyLock<RwLock<BTreeMap<String, Vec<String>>>> = LazyLock::new(|| {
    let m = crate::load_config("keymap.json").and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    RwLock::new(m)
});

fn action(id: &str) -> Option<&'static Action> {
    ACTIONS.iter().find(|a| a.id == id)
}

/// Les raccourcis en vigueur pour une action.
pub fn bindings(id: &str) -> Vec<KeyboardShortcut> {
    if let Some(v) = USER.read().ok().and_then(|u| u.get(id).cloned()) {
        return v.iter().filter_map(|s| parse(s)).collect();
    }
    action(id).map(|a| a.default.iter().map(|&(m, k)| KeyboardShortcut::new(m, k)).collect()).unwrap_or_default()
}

/// Vrai (et la touche est consommée) si un raccourci de l'action vient d'être pressé.
/// Raccourcis que l'éditeur affiché garde pour lui cette image (Ctrl+J dupliquer un calque, Ctrl+K couper
/// au montage…) : ceux de l'app ne les prennent pas.
static CLAIMED: std::sync::Mutex<Vec<KeyboardShortcut>> = std::sync::Mutex::new(Vec::new());

/// L'éditeur affiché réserve ces raccourcis (à appeler avant les raccourcis de l'app, à chaque image)
pub fn claim(v: Vec<KeyboardShortcut>) {
    *CLAIMED.lock().unwrap_or_else(|e| e.into_inner()) = v;
}

/// Une touche pressée cette image correspond exactement à un raccourci réservé par l'éditeur
fn editor_has(ctx: &egui::Context) -> bool {
    let claimed = CLAIMED.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if claimed.is_empty() {
        return false;
    }
    ctx.input(|i| {
        i.events.iter().any(|e| match e {
            egui::Event::Key { key, pressed: true, modifiers, .. } => {
                claimed.iter().any(|c| c.logical_key == *key && c.modifiers.command == modifiers.command && c.modifiers.shift == modifiers.shift && c.modifiers.alt == modifiers.alt)
            }
            _ => false,
        })
    })
}

pub fn pressed(ctx: &egui::Context, id: &str) -> bool {
    // Souris sur la vue 3D d'une scène : ses raccourcis (Ctrl+B biseau, Ctrl+J joindre, Ctrl+L
    // pièces liées, Ctrl+N…) passent avant ceux de l'app ; ouvrir, chercher et les onglets restent
    if crate::model3d::mesh_keys() && matches!(id, "explorer" | "daily" | "assistant" | "new_note" | "split" | "compact" | "terminal") {
        return false;
    }
    // Même chose pour les raccourcis réservés par l'éditeur affiché
    if editor_has(ctx) {
        return false;
    }
    let b = bindings(id);
    // Alt en trop ne compte pas comme le même raccourci (Ctrl+Alt+K n'est pas Ctrl+K)
    ctx.input_mut(|i| {
        b.iter().any(|s| {
            let alt_extra = i.events.iter().any(|e| matches!(e, egui::Event::Key { key, pressed: true, modifiers, .. } if *key == s.logical_key && modifiers.alt != s.modifiers.alt));
            !alt_extra && i.consume_shortcut(s)
        })
    })
}

/// Remplace les raccourcis d'une action (vide = aucun) et enregistre.
pub fn set(id: &str, v: Vec<KeyboardShortcut>) {
    if let Ok(mut u) = USER.write() {
        u.insert(id.to_string(), v.iter().map(format).collect());
        save(&u);
    }
}

/// Revient aux raccourcis par défaut (une action, ou toutes avec None).
pub fn reset(id: Option<&str>) {
    if let Ok(mut u) = USER.write() {
        match id {
            Some(id) => {
                u.remove(id);
            }
            None => u.clear(),
        }
        save(&u);
    }
}

pub fn is_default(id: &str) -> bool {
    USER.read().map(|u| !u.contains_key(id)).unwrap_or(true)
}

fn save(u: &BTreeMap<String, Vec<String>>) {
    crate::save_config("keymap.json", &serde_json::to_string_pretty(u).unwrap_or_default());
}

/// Raccourcis affichés (« Ctrl+Maj+P »), plusieurs séparés par « ou ».
pub fn text(id: &str) -> String {
    bindings(id).iter().map(|b| shown(&format(b))).collect::<Vec<_>>().join(" / ")
}

/// Comme on le lit sur le clavier (« Ctrl+, » plutôt que « Ctrl+Comma ») ; l'enregistrement garde les noms.
fn shown(s: &str) -> String {
    let mut out = s.to_string();
    for (name, sym) in [("Comma", ","), ("Period", "."), ("Backtick", "`"), ("Backslash", "\\"), ("Slash", "/"), ("Semicolon", ";"), ("Minus", "−"), ("Equals", "=")] {
        if out.ends_with(name) {
            out.truncate(out.len() - name.len());
            out.push_str(sym);
        }
    }
    out
}

/// Texte d'un raccourci, aussi utilisé comme format d'enregistrement.
pub fn format(s: &KeyboardShortcut) -> String {
    let m = s.modifiers;
    let mut out = String::new();
    if m.command || m.ctrl || m.mac_cmd {
        out.push_str(if cfg!(target_os = "macos") { "Cmd+" } else { "Ctrl+" });
    }
    if m.alt {
        out.push_str("Alt+");
    }
    if m.shift {
        out.push_str("Maj+");
    }
    out.push_str(s.logical_key.name());
    out
}

pub fn parse(s: &str) -> Option<KeyboardShortcut> {
    let mut m = Modifiers::NONE;
    let parts: Vec<&str> = s.split('+').collect();
    let (key, mods) = if s.ends_with("++") { ("+", &parts[..parts.len().saturating_sub(2)]) } else { (*parts.last()?, &parts[..parts.len() - 1]) };
    for p in mods {
        match *p {
            "Ctrl" | "Cmd" => m = m | Modifiers::COMMAND,
            "Alt" => m = m | Modifiers::ALT,
            "Maj" | "Shift" => m = m | Modifiers::SHIFT,
            _ => return None,
        }
    }
    Some(KeyboardShortcut::new(m, Key::from_name(key)?))
}

/// Paires d'actions qui partagent un même raccourci : (raccourci, action, action).
pub fn conflicts() -> Vec<(String, &'static str, &'static str)> {
    let mut out = vec![];
    let all: Vec<(&'static str, Vec<String>)> = ACTIONS.iter().map(|a| (a.id, bindings(a.id).iter().map(format).collect())).collect();
    for i in 0..all.len() {
        for j in i + 1..all.len() {
            for s in &all[i].1 {
                if all[j].1.contains(s) {
                    out.push((s.clone(), all[i].0, all[j].0));
                }
            }
        }
    }
    out
}

fn cap_id() -> egui::Id {
    egui::Id::unique("keymap-capture")
}

/// Vrai pendant qu'on attend une nouvelle combinaison dans les réglages (les raccourcis globaux
/// se taisent pour la laisser passer).
pub fn capturing(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<String>(cap_id()).is_some())
}

/// Page « raccourcis » des réglages : clic sur un raccourci puis nouvelle combinaison.
pub fn settings_ui(ui: &mut egui::Ui) {
    let p = crate::theme::pal();
    let cap_id = cap_id();
    let mut capture: Option<String> = ui.data(|d| d.get_temp(cap_id));
    if let Some(id) = capture.clone() {
        // En attente d'une combinaison : la première touche pressée (avec ses modificateurs) l'emporte
        let got = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, pressed: true, modifiers, .. } if *key != Key::Escape => Some(KeyboardShortcut::new(*modifiers, *key)),
                _ => None,
            })
        });
        if let Some(s) = got {
            set(&id, vec![s]);
            capture = None;
        } else if ui.input(|i| i.key_pressed(Key::Escape)) {
            capture = None;
        }
    }
    let conf = conflicts();
    ui.label(egui::RichText::new(crate::i18n::t("Clique un raccourci, puis appuie sur la nouvelle combinaison (Échap : annuler).")).color(p.muted).size(11.0));
    ui.add_space(6.0);
    egui::Grid::new("keymap").num_columns(3).spacing([16.0, 6.0]).show(ui, |ui| {
        for a in ACTIONS {
            ui.label(crate::i18n::t(a.label));
            let waiting = capture.as_deref() == Some(a.id);
            let shown = if waiting { crate::i18n::t("appuie sur une combinaison…") } else { text(a.id) };
            let clash = conf.iter().any(|c| c.1 == a.id || c.2 == a.id);
            let col = if clash { p.missing } else if waiting { p.text } else { p.accent };
            let r = ui.add(egui::Button::new(egui::RichText::new(if shown.is_empty() { "—".to_string() } else { shown }).color(col).family(crate::theme::bold())).min_size(egui::vec2(140.0, 0.0)));
            let r = if clash { r.on_hover_text(crate::i18n::t("Ce raccourci sert déjà à une autre action")) } else { r };
            if r.clicked() {
                capture = Some(a.id.to_string());
            }
            ui.horizontal(|ui| {
                if !is_default(a.id) && ui.small_button(crate::i18n::t("par défaut")).clicked() {
                    reset(Some(a.id));
                }
                if !bindings(a.id).is_empty() && ui.small_button("×").on_hover_text(crate::i18n::t("aucun raccourci")).clicked() {
                    set(a.id, vec![]);
                }
            });
            ui.end_row();
        }
    });
    for (s, a, b) in &conf {
        let l = |id: &str| action(id).map(|x| crate::i18n::t(x.label)).unwrap_or_default();
        ui.label(egui::RichText::new(format!("⚠ {s} : {} / {}", l(a), l(b))).color(p.missing).size(11.5));
    }
    ui.add_space(6.0);
    if ui.button(crate::i18n::t("tout remettre par défaut")).clicked() {
        reset(None);
    }
    ui.data_mut(|d| match capture {
        Some(c) => {
            d.insert_temp(cap_id, c);
        }
        None => d.remove::<String>(cap_id),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_retour_et_conflits() {
        for a in ACTIONS {
            for &(m, k) in a.default {
                let s = KeyboardShortcut::new(m, k);
                assert_eq!(parse(&format(&s)), Some(s), "{}", a.id);
            }
        }
        // Les raccourcis par défaut ne se marchent pas dessus
        let mut seen = std::collections::HashSet::new();
        for a in ACTIONS {
            for &(m, k) in a.default {
                assert!(seen.insert(format(&KeyboardShortcut::new(m, k))), "{}", a.id);
            }
        }
    }
}
