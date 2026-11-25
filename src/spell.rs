//! Correcteur orthographique des notes : français et anglais (listes de mots libres, MIT),
//! plus le dictionnaire personnel du coffre. Les mots inconnus sont soulignés ; un clic droit
//! propose des corrections.

use eframe::egui;
use std::collections::HashSet;
use std::sync::{LazyLock, RwLock};

fn load(gz: &[u8]) -> HashSet<String> {
    use std::io::Read;
    let mut s = String::new();
    let _ = flate2::read::GzDecoder::new(gz).read_to_string(&mut s);
    s.lines().map(|l| l.to_string()).collect()
}

static FR: LazyLock<HashSet<String>> = LazyLock::new(|| {
    let mut d = load(include_bytes!("../assets/dict/fr.txt.gz"));
    // Mots composés avec apostrophe, absents de la liste
    for w in ["aujourd'hui", "quelqu'un", "quelqu'une", "presqu'île", "presqu'îles", "prud'homme", "prud'hommes", "entr'acte", "c'est-à-dire", "perso", "persos", "appli", "applis", "ok", "info", "infos", "resto", "restos", "ado", "ados", "storyboard", "storyboards", "rig", "rigs", "rendu", "shader", "shaders"] {
        d.insert(w.into());
    }
    d
});
static EN: LazyLock<HashSet<String>> = LazyLock::new(|| load(include_bytes!("../assets/dict/en.txt.gz")));
/// Dictionnaire personnel (le coffre ouvert) : mots ajoutés par l'utilisateur
static USER: LazyLock<RwLock<(String, HashSet<String>)>> = LazyLock::new(|| RwLock::new((String::new(), HashSet::new())));

fn user_file(root: &str) -> std::path::PathBuf {
    std::path::Path::new(root).join(".app").join("dictionnaire.txt")
}

/// Le dictionnaire personnel du coffre `root` (relu au changement de coffre)
fn user_words(root: &str) -> HashSet<String> {
    if let Ok(u) = USER.read() {
        if u.0 == root {
            return u.1.clone();
        }
    }
    let set: HashSet<String> = std::fs::read_to_string(user_file(root)).unwrap_or_default().lines().map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty()).collect();
    if let Ok(mut u) = USER.write() {
        *u = (root.to_string(), set.clone());
    }
    set
}

pub fn add_word(root: &str, word: &str) {
    let w = word.trim().to_lowercase();
    if w.is_empty() {
        return;
    }
    let mut set = user_words(root);
    set.insert(w);
    let mut list: Vec<&String> = set.iter().collect();
    list.sort();
    let path = user_file(root);
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(&path, list.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n"));
    if let Ok(mut u) = USER.write() {
        *u = (root.to_string(), set);
    }
    CACHE.with(|c| c.borrow_mut().clear());
}

fn known_plain(w: &str, user: &HashSet<String>) -> bool {
    FR.contains(w) || EN.contains(w) || user.contains(w)
}

/// Le mot est-il connu ? (élisions, traits d'union, ligatures et majuscules pris en compte)
pub fn known(word: &str, user: &HashSet<String>) -> bool {
    let w = word.to_lowercase().replace('’', "'").replace('œ', "oe").replace('æ', "ae");
    if known_plain(&w, user) {
        return true;
    }
    // Élision : l'arbre, qu'il, jusqu'à…
    if let Some((head, tail)) = w.split_once('\'') {
        let elided = ["l", "d", "j", "m", "n", "s", "t", "c", "qu", "jusqu", "lorsqu", "puisqu", "quoiqu"];
        if elided.contains(&head) && (tail.is_empty() || known(tail, user)) {
            return true;
        }
        // Anglais : don't, it's…
        if ["s", "t", "re", "ve", "ll", "d", "m"].contains(&tail) && known_plain(head, user) {
            return true;
        }
    }
    // Mots composés : chaque partie connue
    if w.contains('-') {
        return w.split('-').all(|p| p.is_empty() || known(p, user));
    }
    false
}

/// Les mots d'un texte (début et fin en octets), hors code, liens, adresses et balises
pub fn words(text: &str) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut in_block = false;
    let mut in_front = false;
    let mut off = 0;
    for (li, line) in text.split_inclusive('\n').enumerate() {
        let t = line.trim_start();
        if li == 0 && t.trim_end() == "---" {
            in_front = true;
        } else if in_front && t.trim_end() == "---" {
            in_front = false;
            off += line.len();
            continue;
        }
        if t.starts_with("```") {
            in_block = !in_block;
        }
        if in_block || in_front || t.starts_with("```") || line.starts_with("    ") || line.starts_with('\t') {
            off += line.len();
            continue;
        }
        let bytes = line.as_bytes();
        let mut i = 0;
        let mut code = false;
        while i < line.len() {
            let c = line[i..].chars().next().unwrap();
            let cl = c.len_utf8();
            if c == '`' {
                code = !code;
                i += cl;
                continue;
            }
            // Liens wiki [[…]], cibles de liens (…), adresses
            if line[i..].starts_with("[[") {
                i = line[i..].find("]]").map(|k| i + k + 2).unwrap_or(line.len());
                continue;
            }
            if line[i..].starts_with("](") {
                i = line[i..].find(')').map(|k| i + k + 1).unwrap_or(line.len());
                continue;
            }
            if c.is_alphabetic() {
                let start = i;
                let mut j = i;
                while j < line.len() {
                    let ch = line[j..].chars().next().unwrap();
                    let ok = ch.is_alphanumeric() || ((ch == '\'' || ch == '’' || ch == '-') && line[j + ch.len_utf8()..].chars().next().is_some_and(|n| n.is_alphabetic()));
                    if !ok {
                        break;
                    }
                    j += ch.len_utf8();
                }
                let w = &line[start..j];
                // Collé à une adresse, un chemin, une balise ou un nombre : on laisse
                let before = start.checked_sub(1).map(|k| bytes[k]);
                let after = bytes.get(j).copied();
                let skip = code
                    || w.chars().any(|ch| ch.is_numeric())
                    || matches!(before, Some(b'#' | b'@' | b'/' | b'\\' | b'.' | b'_'))
                    || matches!(after, Some(b'/' | b'_' | b'@') )
                    || (after == Some(b'.') && bytes.get(j + 1).is_some_and(|b| b.is_ascii_alphanumeric()))
                    || line[j..].starts_with("://")
                    || w.chars().count() < 2
                    // SIGLES et mots en Majuscule au milieu d'une phrase (des noms propres)
                    || w.chars().skip(1).any(|ch| ch.is_uppercase());
                if !skip {
                    out.push((off + start, off + j));
                }
                i = j;
                continue;
            }
            i += cl;
        }
        off += line.len();
    }
    out
}

thread_local! {
    static CACHE: std::cell::RefCell<std::collections::HashMap<u64, Vec<(usize, usize)>>> = Default::default();
}

/// Les mots inconnus du texte (gardés en mémoire tant que le texte ne change pas)
pub fn misspelled(text: &str, root: &str) -> Vec<(usize, usize)> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    root.hash(&mut h);
    let key = h.finish();
    if let Some(v) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return v;
    }
    let user = user_words(root);
    let mut v: Vec<(usize, usize)> = words(text).into_iter().filter(|(a, b)| {
        let w = &text[*a..*b];
        // Un mot à majuscule inconnu est sans doute un nom (personnage, lieu, marque) : on le laisse
        let cap = w.chars().next().is_some_and(|c| c.is_uppercase());
        !cap && !known(w, &user)
    }).collect();
    v.sort();
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 64 {
            c.clear();
        }
        c.insert(key, v.clone());
    });
    v
}

/// Corrections proposées : à une ou deux fautes de frappe, les plus proches d'abord
pub fn suggest(word: &str, root: &str) -> Vec<String> {
    let user = user_words(root);
    let lower = word.to_lowercase();
    let cap = word.chars().next().is_some_and(|c| c.is_uppercase());
    let alpha: Vec<char> = "abcdefghijklmnopqrstuvwxyzéèêëàâäîïôöùûüçœ'-".chars().collect();
    let edits = |w: &str| -> Vec<String> {
        let cs: Vec<char> = w.chars().collect();
        let mut out = vec![];
        for i in 0..=cs.len() {
            if i < cs.len() {
                // Suppression, remplacement
                let mut d = cs.clone();
                d.remove(i);
                out.push(d.iter().collect());
                for &a in &alpha {
                    let mut r = cs.clone();
                    r[i] = a;
                    out.push(r.iter().collect());
                }
            }
            if i + 1 < cs.len() {
                let mut t = cs.clone();
                t.swap(i, i + 1);
                out.push(t.iter().collect());
            }
            for &a in &alpha {
                let mut ins = cs.clone();
                ins.insert(i, a);
                out.push(ins.iter().collect());
            }
        }
        out
    };
    let strip = |s: &str| -> String { s.chars().map(|c| match c { 'é' | 'è' | 'ê' | 'ë' => 'e', 'à' | 'â' | 'ä' => 'a', 'î' | 'ï' => 'i', 'ô' | 'ö' => 'o', 'ù' | 'û' | 'ü' => 'u', 'ç' => 'c', c => c }).collect() };
    let mut found: Vec<(u32, String)> = vec![];
    let first = edits(&lower);
    for c in &first {
        if known_plain(c, &user) {
            // Les accents oubliés passent devant, puis les mots français
            let score = if strip(c) == strip(&lower) { 0 } else if FR.contains(c.as_str()) { 1 } else { 2 };
            found.push((score, c.clone()));
        }
    }
    if found.len() < 3 && lower.chars().count() > 3 {
        for c in first.iter().take(4000) {
            for c2 in edits(c) {
                if known_plain(&c2, &user) && !found.iter().any(|f| f.1 == c2) {
                    found.push((if FR.contains(c2.as_str()) { 3 } else { 4 }, c2));
                }
            }
            if found.len() >= 8 {
                break;
            }
        }
    }
    found.sort();
    found.dedup_by(|a, b| a.1 == b.1);
    found
        .into_iter()
        .take(6)
        .map(|(_, w)| {
            if cap {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or(w)
            } else {
                w
            }
        })
        .collect()
}

/// Souligne (en rouge) les plages d'octets données dans une mise en page de texte
pub fn underline(job: &mut egui::text::LayoutJob, ranges: &[(usize, usize)], color: egui::Color32) {
    if ranges.is_empty() {
        return;
    }
    let mut out: Vec<egui::text::LayoutSection> = Vec::with_capacity(job.sections.len() + ranges.len() * 2);
    for s in job.sections.drain(..) {
        let (a, b) = (s.byte_range.start.0, s.byte_range.end.0);
        let bi = egui::text::ByteIndex;
        let mut cur = a;
        for &(r0, r1) in ranges.iter().filter(|(r0, r1)| *r1 > a && *r0 < b) {
            let (r0, r1) = (r0.max(a), r1.min(b));
            if r0 > cur {
                out.push(egui::text::LayoutSection { byte_range: bi(cur)..bi(r0), ..s.clone() });
            }
            let mut f = s.format.clone();
            f.underline = egui::Stroke::new(1.5, color);
            out.push(egui::text::LayoutSection { byte_range: bi(r0)..bi(r1), format: f, leading_space: if r0 == a { s.leading_space } else { 0.0 } });
            cur = r1;
        }
        if cur < b {
            out.push(egui::text::LayoutSection { byte_range: bi(cur)..bi(b), leading_space: if cur == a { s.leading_space } else { 0.0 }, ..s });
        }
    }
    job.sections = out;
}


/// Le mot inconnu sous le pointeur au moment du clic droit (plage d'octets), gardé le temps du menu
static MENU_WORD: LazyLock<RwLock<Option<(usize, usize, String)>>> = LazyLock::new(|| RwLock::new(None));

/// Menu du clic droit d'une note : corrections du mot souligné et ajout au dictionnaire ; vrai si le texte change
pub fn context_menu(ui: &egui::Ui, out: &egui::text_edit::TextEditOutput, text: &mut String, root: &str) -> bool {
    if out.response.secondary_clicked() {
        let word = out.response.interact_pointer_pos().and_then(|p| {
            let c = out.galley.cursor_from_pos(p - out.galley_pos);
            let at = text.char_indices().nth(c.index.into()).map(|(b, _)| b).unwrap_or(text.len());
            misspelled(text, root).into_iter().find(|(a, b)| at >= *a && at <= *b).map(|(a, b)| (a, b, text[a..b].to_string()))
        });
        if let Ok(mut m) = MENU_WORD.write() {
            *m = word;
        }
    }
    let Some((a, b, word)) = MENU_WORD.read().ok().and_then(|m| m.clone()) else { return false };
    let mut changed = false;
    let mut close = false;
    out.response.context_menu(|ui| {
        ui.set_min_width(200.0);
        // Le texte a pu changer entre-temps
        if text.get(a..b) != Some(word.as_str()) {
            ui.close();
            return;
        }
        let sugg = suggest(&word, root);
        if sugg.is_empty() {
            ui.label(egui::RichText::new(crate::i18n::t("aucune suggestion")).weak());
        }
        for sgt in sugg {
            if ui.button(egui::RichText::new(&sgt).strong()).clicked() {
                text.replace_range(a..b, &sgt);
                changed = true;
                close = true;
                ui.close();
            }
        }
        ui.separator();
        if ui.button(format!("{}  {}", crate::lucide::BOOK_PLUS, crate::i18n::t("ajouter au dictionnaire"))).on_hover_text(crate::i18n::t("Ce mot ne sera plus souligné dans ce coffre")).clicked() {
            add_word(root, &word);
            close = true;
            ui.close();
        }
    });
    let _ = ui;
    if close {
        if let Ok(mut m) = MENU_WORD.write() {
            *m = None;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mots_et_fautes() {
        let t = "# Titre\nLe chatt mange l'arbre aujourd'hui, c'est-à-dire bien. Voir [[Notes]] et `codee` https://exemple.com\n```\nfautee\n```\nAlex est là. Lle chat.";
        let bad: Vec<&str> = misspelled(t, "/nulle-part").iter().map(|(a, b)| &t[*a..*b]).collect();
        assert_eq!(bad, vec!["chatt"]);
        assert!(known("Œuvre", &HashSet::new()));
        assert!(known("don't", &HashSet::new()));
        let s = suggest("chatt", "/nulle-part");
        assert!(s.contains(&"chat".to_string()), "{s:?}");
        let s = suggest("etait", "/nulle-part");
        assert_eq!(s.first().map(|x| x.as_str()), Some("était"), "{s:?}");
    }

    #[test]
    fn soulignement() {
        let mut job = egui::text::LayoutJob::simple("un motd ici".into(), egui::FontId::proportional(12.0), egui::Color32::WHITE, 100.0);
        underline(&mut job, &[(3, 7)], egui::Color32::RED);
        assert_eq!(job.sections.len(), 3);
        assert_eq!(job.sections[1].byte_range, egui::text::ByteIndex(3)..egui::text::ByteIndex(7));
        assert!(job.sections[1].format.underline.width > 0.0);
    }
}
