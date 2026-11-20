//! Langue de l'interface. Le français est la langue source ; les autres langues sont des
//! tables (assets/i18n/<langue>.json) qui associent le texte français à sa traduction.
//! Un texte absent de la table reste en français.
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::LazyLock;

pub const LANGS: &[(&str, &str)] = &[("fr", "Français"), ("en", "English")];

static LANG: AtomicU8 = AtomicU8::new(0);

static EN: LazyLock<HashMap<String, String>> = LazyLock::new(|| serde_json::from_str(include_str!("../assets/i18n/en.json")).unwrap_or_default());

pub fn set(code: &str) {
    let i = LANGS.iter().position(|(c, _)| *c == code).unwrap_or(0);
    LANG.store(i as u8, Ordering::Relaxed);
}

pub fn current() -> &'static str {
    LANGS[LANG.load(Ordering::Relaxed) as usize].0
}

/// Traduit un texte de l'interface.
pub fn t(s: &str) -> String {
    match LANG.load(Ordering::Relaxed) {
        1 => EN.get(s).cloned().unwrap_or_else(|| s.to_string()),
        _ => s.to_string(),
    }
}

/// Langue du système (pour le premier démarrage).
#[allow(dead_code)]
/// Traduction anglaise d'un texte, quelle que soit la langue affichée (pour la recherche).
pub fn en(s: &str) -> String {
    EN.get(s).cloned().unwrap_or_else(|| s.to_string())
}

#[allow(dead_code)]
pub fn system_default() -> &'static str {
    let l = std::env::var("LANG").or_else(|_| std::env::var("LC_ALL")).unwrap_or_default().to_lowercase();
    if l.starts_with("en") { "en" } else { "fr" }
}

#[cfg(test)]
mod tests {
    #[test]
    fn table_anglaise() {
        super::set("en");
        assert_eq!(super::t("fermer"), "close");
        assert_eq!(super::t("texte absent"), "texte absent");
        super::set("fr");
        assert_eq!(super::t("fermer"), "fermer");
        assert!(super::EN.len() > 400);
    }
}
