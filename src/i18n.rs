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

/// Traduit un texte de l'interface, en casse de phrase : première lettre en majuscule, comme
/// dans les logiciels de création (« Nouvelle note », « Mode édition »). Les symboles du début
/// (« ✚ », « // », « ＋ ») sont sautés.
pub fn t(s: &str) -> String {
    sentence(&t_mid(s))
}

/// Traduit un morceau placé au milieu d'une phrase (« 3 fichier(s) renommé(s) ») : sans majuscule.
pub fn t_mid(s: &str) -> String {
    match LANG.load(Ordering::Relaxed) {
        1 => EN.get(s).cloned().unwrap_or_else(|| s.to_string()),
        _ => s.to_string(),
    }
}

/// Met en majuscule la première lettre (après d'éventuels symboles).
pub fn sentence(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut done = false;
    for c in s.chars() {
        if !done && c.is_alphanumeric() {
            done = true;
            if c.is_lowercase() {
                out.extend(c.to_uppercase());
                continue;
            }
        }
        out.push(c);
    }
    out
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
        assert_eq!(super::t("fermer"), "Close");
        assert_eq!(super::t_mid("fermer"), "close");
        assert_eq!(super::t("texte absent"), "Texte absent");
        super::set("fr");
        assert_eq!(super::t("fermer"), "Fermer");
        assert_eq!(super::t("✚ ouvrir"), "✚ Ouvrir");
        assert_eq!(super::t("élément"), "Élément");
        assert!(super::EN.len() > 400);
    }
}
