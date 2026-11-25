//! Temps passé par fichier et par projet (pour facturer) : le temps actif devant un fichier ouvert est compté
//! par jour dans .app/temps.json. « Actif » : la fenêtre a le focus et il y a eu une action dans les 2 dernières minutes.

use std::collections::BTreeMap;
use std::time::Instant;

#[derive(Default)]
pub struct Tracker {
    root: String,
    /// jour (AAAA-MM-JJ) → fichier → secondes
    pub days: BTreeMap<String, BTreeMap<String, f64>>,
    last_input: Option<Instant>,
    last_tick: Option<Instant>,
    last_save: Option<Instant>,
    dirty: bool,
}

/// Secondes sans action après lesquelles on ne compte plus
const IDLE: f64 = 120.0;

impl Tracker {
    fn path(root: &str) -> std::path::PathBuf {
        std::path::Path::new(root).join(".app/temps.json")
    }

    pub fn load(root: &str) -> Tracker {
        let days = std::fs::read_to_string(Self::path(root)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Tracker { root: root.to_string(), days, ..Default::default() }
    }

    pub fn save(&mut self) {
        if !self.dirty || self.root.is_empty() {
            return;
        }
        if let Ok(t) = serde_json::to_string(&self.days) {
            let p = Self::path(&self.root);
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(p, t);
        }
        self.dirty = false;
        self.last_save = Some(Instant::now());
    }

    /// Une image de l'interface : `input` s'il y a eu une action, `focused` si la fenêtre a le focus
    pub fn tick(&mut self, root: &str, rel: Option<&str>, input: bool, focused: bool) {
        if root != self.root {
            self.save();
            *self = Tracker::load(root);
        }
        let now = Instant::now();
        if input {
            self.last_input = Some(now);
        }
        let dt = self.last_tick.map(|t| now.duration_since(t).as_secs_f64()).unwrap_or(0.0).min(2.0);
        self.last_tick = Some(now);
        let active = focused && self.last_input.is_some_and(|t| now.duration_since(t).as_secs_f64() < IDLE);
        if let (true, Some(rel)) = (active, rel) {
            self.add(&crate::extras::today(), rel, dt);
        }
        if self.last_save.is_none_or(|t| now.duration_since(t).as_secs() > 30) {
            self.save();
            self.last_save = Some(now);
        }
    }

    pub fn add(&mut self, day: &str, rel: &str, secs: f64) {
        if secs <= 0.0 {
            return;
        }
        *self.days.entry(day.to_string()).or_default().entry(rel.to_string()).or_default() += secs;
        self.dirty = true;
    }

    /// Le total par projet (dossier du premier niveau, « racine » pour les fichiers à la racine) et par fichier,
    /// depuis le jour `from` inclus (None : tout)
    pub fn totals(&self, from: Option<&str>) -> Vec<(String, f64, Vec<(String, f64)>)> {
        let mut by: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
        for (day, files) in &self.days {
            if from.is_some_and(|f| day.as_str() < f) {
                continue;
            }
            for (rel, s) in files {
                let proj = rel.split_once('/').map(|x| x.0.to_string()).unwrap_or_else(|| crate::i18n::t("racine"));
                *by.entry(proj).or_default().entry(rel.clone()).or_default() += s;
            }
        }
        let mut v: Vec<(String, f64, Vec<(String, f64)>)> = by
            .into_iter()
            .map(|(p, files)| {
                let mut f: Vec<(String, f64)> = files.into_iter().collect();
                f.sort_by(|a, b| b.1.total_cmp(&a.1));
                (p, f.iter().map(|x| x.1).sum(), f)
            })
            .collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v
    }
}

/// 3 h 05, 12 min
pub fn fmt(secs: f64) -> String {
    if secs < 59.5 {
        return format!("{} s", secs.round() as u64);
    }
    let m = (secs / 60.0).round() as u64;
    if m >= 60 {
        format!("{} h {:02}", m / 60, m % 60)
    } else {
        format!("{m} min")
    }
}

/// Le jour il y a `n` jours (AAAA-MM-JJ)
pub fn days_ago(n: i64) -> String {
    (chrono::Local::now() - chrono::Duration::days(n)).format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn totaux_par_projet() {
        let mut t = super::Tracker::default();
        t.add("2026-11-01", "court/plan1.aryscene", 3600.0);
        t.add("2026-11-20", "court/montage.aryav", 1800.0);
        t.add("2026-11-20", "clip/titre.arymotion", 600.0);
        let all = t.totals(None);
        assert_eq!(all[0].0, "court");
        assert!((all[0].1 - 5400.0).abs() < 1e-6);
        let recent = t.totals(Some("2026-11-10"));
        assert!((recent[0].1 - 1800.0).abs() < 1e-6);
        assert_eq!(super::fmt(5400.0), "1 h 30");
        assert_eq!(super::fmt(600.0), "10 min");
    }
}
