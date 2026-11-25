//! Notes proches : les notes du coffre qui parlent des mêmes choses que celle qu'on écrit
//! (mots rares en commun, TF-IDF), pas encore liées. Tout est calculé sur place, sans modèle.

use std::collections::HashMap;

const STOP: &[&str] = &[
    "avec", "dans", "pour", "mais", "plus", "comme", "tout", "tous", "toute", "toutes", "être", "avoir", "fait", "faire", "cette", "ces", "sont", "était", "aussi", "alors", "donc", "elle", "elles", "nous", "vous", "leur", "leurs", "sans", "sous", "entre", "encore", "très", "peut", "même", "autre", "autres", "quand", "quoi", "dont", "chez", "vers", "après", "avant", "bien", "rien", "deux", "trois", "note", "notes", "this", "that", "with", "from", "have", "will", "your", "about", "there", "their", "which", "would", "could", "should", "what", "when", "where", "into", "than", "then", "them", "they", "were", "been", "just", "like", "some", "more", "only", "also",
];

/// Les mots d'un texte (minuscules, quatre lettres et plus, hors mots vides et syntaxe Markdown)
pub fn terms(text: &str) -> HashMap<String, f32> {
    let mut out: HashMap<String, f32> = HashMap::new();
    // L'en-tête YAML compte aussi (tags, type), le code non
    let mut in_code = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        for w in line.split(|c: char| !c.is_alphanumeric()) {
            let w = w.to_lowercase();
            if w.chars().count() >= 4 && !w.chars().all(|c| c.is_numeric()) && !STOP.contains(&w.as_str()) {
                *out.entry(w).or_default() += 1.0;
            }
        }
    }
    out
}

thread_local! {
    static CORPUS: std::cell::RefCell<HashMap<String, (u64, HashMap<String, f32>)>> = Default::default();
    static LAST: std::cell::RefCell<Option<(u64, Vec<(String, String, f32)>)>> = const { std::cell::RefCell::new(None) };
}

/// Les notes proches de `text` (la note `rel`) : chemin, nom, ressemblance (0 à 1). `notes` : (chemin, nom, date)
pub fn suggest(root: &str, rel: &str, text: &str, notes: &[(String, String, u64)]) -> Vec<(String, String, f32)> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    rel.hash(&mut h);
    text.hash(&mut h);
    notes.len().hash(&mut h);
    let key = h.finish();
    if let Some(v) = LAST.with(|l| l.borrow().as_ref().filter(|x| x.0 == key).map(|x| x.1.clone())) {
        return v;
    }
    // Les mots de chaque note, relus seulement si elle a changé
    let docs: Vec<(String, String, HashMap<String, f32>)> = CORPUS.with(|c| {
        let mut c = c.borrow_mut();
        notes
            .iter()
            .filter(|(r, _, _)| r != rel)
            .map(|(r, name, m)| {
                let stale = c.get(r).is_none_or(|x| x.0 != *m);
                if stale {
                    let t = std::fs::read_to_string(std::path::Path::new(root).join(r)).unwrap_or_default();
                    c.insert(r.clone(), (*m, terms(&t)));
                }
                (r.clone(), name.clone(), c.get(r).map(|x| x.1.clone()).unwrap_or_default())
            })
            .collect()
    });
    let n = docs.len().max(1) as f32 + 1.0;
    let mut df: HashMap<&str, f32> = HashMap::new();
    for (_, _, t) in &docs {
        for w in t.keys() {
            *df.entry(w.as_str()).or_default() += 1.0;
        }
    }
    let idf = |w: &str| (n / (1.0 + df.get(w).copied().unwrap_or(0.0))).ln().max(0.0);
    let vec_of = |t: &HashMap<String, f32>| -> HashMap<String, f32> { t.iter().map(|(w, c)| (w.clone(), (1.0 + c.ln()) * idf(w))).collect() };
    let me = vec_of(&terms(text));
    let norm = |v: &HashMap<String, f32>| v.values().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
    let nm = norm(&me);
    let lower = text.to_lowercase();
    let mut out: Vec<(String, String, f32)> = docs
        .iter()
        // Déjà liée : on ne la propose pas
        .filter(|(_, name, _)| !lower.contains(&format!("[[{}", name.to_lowercase())))
        .map(|(r, name, t)| {
            let v = vec_of(t);
            let dot: f32 = me.iter().map(|(w, x)| x * v.get(w).copied().unwrap_or(0.0)).sum();
            (r.clone(), name.clone(), dot / (nm * norm(&v)))
        })
        .filter(|x| x.2 > 0.06)
        .collect();
    out.sort_by(|a, b| b.2.total_cmp(&a.2));
    out.truncate(6);
    LAST.with(|l| *l.borrow_mut() = Some((key, out.clone())));
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn notes_proches() {
        let dir = std::env::temp_dir().join(format!("ary-rel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("dragon.md"), "Le dragon vit dans la montagne. Ses écailles brillent, le dragon crache du feu.").unwrap();
        std::fs::write(dir.join("recette.md"), "Recette de crêpes : farine, lait, oeufs, beurre.").unwrap();
        std::fs::write(dir.join("chevalier.md"), "Le chevalier affronte le [[dragon]] dans la montagne.").unwrap();
        let notes = vec![("dragon.md".to_string(), "dragon".to_string(), 1), ("recette.md".to_string(), "recette".to_string(), 1)];
        let root = dir.to_string_lossy().to_string();
        let s = super::suggest(&root, "plan.md", "Plan 12 : le dragon survole la montagne, ses écailles au soleil.", &notes);
        assert_eq!(s.first().map(|x| x.1.as_str()), Some("dragon"));
        assert!(!s.iter().any(|x| x.1 == "recette"));
        // Déjà liée : plus proposée
        let s = super::suggest(&root, "plan.md", "Le [[dragon]] survole la montagne, ses écailles au soleil.", &notes);
        assert!(s.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
