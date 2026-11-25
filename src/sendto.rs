//! « Envoyer vers » : une image (un rendu, un calque, un plan de travail) passe d'un outil à
//! l'autre. L'image est posée dans « envois/ » du coffre, puis l'outil choisi la reçoit : dans
//! le fichier déjà ouvert de cet outil, sinon dans un nouveau fichier.

use eframe::egui;
use std::sync::Mutex;

/// Les destinations : identifiant, nom affiché, icône
pub const TARGETS: [(&str, &str, &str); 5] = [
    ("retouche", "Retouche (nouveau calque)", crate::lucide::BRUSH),
    ("vector", "Vectoriel (image)", crate::lucide::PEN_TOOL),
    ("motion", "Motion (calque image)", crate::lucide::FILM),
    ("montage", "Montage (plan à la fin)", crate::lucide::CLAPPERBOARD),
    ("board", "Moodboard", crate::lucide::LAYOUT_DASHBOARD),
];

static QUEUE: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Dossier des images envoyées
pub const DIR: &str = "envois";

/// Écrit l'image dans « envois/ » ; rend son chemin dans le coffre
pub fn stage(root: &str, name: &str, img: &image::RgbaImage) -> Result<String, String> {
    let dir = std::path::Path::new(root).join(DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let clean: String = name.chars().map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c }).collect();
    let clean = if clean.trim().is_empty() { "image".to_string() } else { clean.trim().to_string() };
    let mut n = 1;
    let mut file = dir.join(format!("{clean}.png"));
    while file.exists() {
        n += 1;
        file = dir.join(format!("{clean} {n}.png"));
    }
    img.save(&file).map_err(|e| e.to_string())?;
    Ok(file.strip_prefix(root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| file.to_string_lossy().to_string()))
}

pub fn push(target: &str, rel: String) {
    if let Ok(mut q) = QUEUE.lock() {
        q.push((target.to_string(), rel));
    }
}

/// Les envois en attente (vidés)
pub fn take() -> Vec<(String, String)> {
    QUEUE.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}

/// Bouton « Envoyer vers » (une icône) et son menu ; rend la destination choisie.
/// `except` : l'outil d'où l'on envoie (il n'est pas proposé).
pub fn button(ui: &mut egui::Ui, enabled: bool, except: &str) -> Option<&'static str> {
    let mut pick = None;
    ui.add_enabled_ui(enabled, |ui| {
        egui::containers::menu::MenuButton::new(egui::RichText::new(crate::lucide::SEND).size(14.0)).ui(ui, |ui| {
            ui.set_min_width(230.0);
            ui.label(egui::RichText::new(crate::i18n::t("envoyer vers")).weak().size(11.0));
            for (id, label, icon) in TARGETS {
                if id != except && ui.button(format!("{icon}  {}", crate::i18n::t(label))).clicked() {
                    pick = Some(id);
                    ui.close();
                }
            }
        })
        .0
        .on_hover_text(crate::i18n::t("Envoyer vers un autre outil (Retouche, Vectoriel, Motion, Montage, Moodboard)"));
    });
    pick
}

#[cfg(test)]
mod tests {
    #[test]
    fn envoi() {
        let dir = std::env::temp_dir().join(format!("ary-send-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.to_string_lossy().to_string();
        let img = image::RgbaImage::new(4, 4);
        let a = super::stage(&root, "rendu", &img).unwrap();
        let b = super::stage(&root, "rendu", &img).unwrap();
        assert_eq!((a.as_str(), b.as_str()), ("envois/rendu.png", "envois/rendu 2.png"));
        super::push("montage", a.clone());
        assert_eq!(super::take(), vec![("montage".to_string(), a)]);
        assert!(super::take().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
