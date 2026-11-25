//! Bases, comme dans Obsidian : un bloc ```base dans une note liste les notes du coffre filtrées par leurs
//! propriétés (l'en-tête YAML), en tableau, en cartes ou en liste, triées et groupées. Un bloc ```tasks
//! réunit les cases à cocher de tout le coffre.
//!
//! ```base
//! from: projets/          (dossier, ou #tag ; plusieurs séparés par des virgules)
//! where: statut = en cours (une condition par ligne : = != > < >= <= contient)
//! columns: file, statut, échéance
//! sort: échéance desc
//! group: statut
//! view: table            (table, cartes, liste)
//! limit: 50
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Une note lue : ses propriétés (clé en minuscules), ses tags, sa date de modification
#[derive(Clone, Debug, Default)]
pub struct Row {
    pub rel: String,
    pub props: Vec<(String, Vec<String>)>,
    pub tags: Vec<String>,
    pub modified: u64,
    pub tasks: Vec<(usize, bool, String)>,
}

impl Row {
    pub fn name(&self) -> &str {
        self.rel.rsplit('/').next().unwrap_or(&self.rel).trim_end_matches(".md")
    }

    /// Valeur d'une colonne : propriété, ou file, folder, tags, modifié
    pub fn get(&self, key: &str) -> String {
        match key.to_lowercase().as_str() {
            "file" | "fichier" | "nom" | "name" => self.name().to_string(),
            "folder" | "dossier" => self.rel.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default(),
            "tags" => self.tags.join(", "),
            "modified" | "modifié" | "mtime" => chrono::DateTime::from_timestamp(self.modified as i64, 0).map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default(),
            k => self.props.iter().find(|(p, _)| p.eq_ignore_ascii_case(k)).map(|(_, v)| v.join(", ")).unwrap_or_default(),
        }
    }
}

/// Cache des notes lues : chemin → (date de modification, ligne)
static CACHE: Mutex<Option<HashMap<PathBuf, (u64, Row)>>> = Mutex::new(None);

fn mtime(p: &std::path::Path) -> u64 {
    std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
}

/// Lit (ou reprend du cache) les notes `.md` données
pub fn rows(root: &str, rels: &[String]) -> Vec<Row> {
    let mut g = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let map = g.get_or_insert_with(HashMap::new);
    let mut out = Vec::with_capacity(rels.len());
    for rel in rels.iter().filter(|r| r.ends_with(".md")) {
        let path = std::path::Path::new(root).join(rel);
        let m = mtime(&path);
        if let Some((mm, r)) = map.get(&path) {
            if *mm == m {
                out.push(r.clone());
                continue;
            }
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let r = parse_row(rel, &text, m);
        map.insert(path, (m, r.clone()));
        out.push(r);
    }
    out
}

pub fn parse_row(rel: &str, text: &str, modified: u64) -> Row {
    let props = crate::vault::frontmatter(text).map(|(p, _)| p).unwrap_or_default();
    let mut tags = crate::vault::front_tags(text);
    for w in text.split_whitespace().filter(|w| w.starts_with('#') && w.len() > 1 && !w.starts_with("##")) {
        let t = w.trim_start_matches('#').trim_end_matches([',', '.', ';', ':', ')']);
        if !t.is_empty() && !tags.iter().any(|x| x == t) {
            tags.push(t.to_string());
        }
    }
    let tasks = text
        .lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let t = l.trim_start();
            let rest = t.strip_prefix("- [").or_else(|| t.strip_prefix("* ["))?;
            let (mark, label) = rest.split_once(']')?;
            Some((i, !mark.trim().is_empty(), label.trim().to_string()))
        })
        .collect();
    Row { rel: rel.to_string(), props, tags, modified, tasks }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Query {
    pub from: Vec<String>,
    pub wheres: Vec<(String, String, String)>,
    pub columns: Vec<String>,
    pub sort: Option<(String, bool)>,
    pub group: Option<String>,
    pub view: String,
    pub limit: usize,
}

pub fn parse_query(src: &str) -> Query {
    let mut q = Query { view: "table".into(), limit: 500, ..Default::default() };
    for line in src.lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        let v = v.trim();
        match k.trim().to_lowercase().as_str() {
            "from" | "de" | "depuis" => q.from.extend(v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty())),
            "where" | "si" | "filtre" => {
                for op in ["!=", ">=", "<=", " contient ", " contains ", "=", ">", "<"] {
                    if let Some((a, b)) = v.split_once(op) {
                        q.wheres.push((a.trim().to_string(), op.trim().to_string(), b.trim().trim_matches('"').to_string()));
                        break;
                    }
                }
            }
            "columns" | "colonnes" => q.columns = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            "sort" | "tri" | "trier" => {
                let mut it = v.split_whitespace();
                if let Some(key) = it.next() {
                    q.sort = Some((key.to_string(), it.next().is_some_and(|d| d.eq_ignore_ascii_case("desc") || d.eq_ignore_ascii_case("décroissant"))));
                }
            }
            "group" | "grouper" => q.group = Some(v.to_string()).filter(|s| !s.is_empty()),
            "view" | "vue" => q.view = v.to_lowercase(),
            "limit" | "limite" => q.limit = v.parse().unwrap_or(500),
            _ => {}
        }
    }
    if q.columns.is_empty() {
        q.columns = vec!["file".into()];
    }
    q
}

/// Comparaison : en nombres si les deux en sont, en dates (AAAA-MM-JJ se compare comme du texte), sinon en texte
fn cmp_vals(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
        (Ok(x), Ok(y)) => x.total_cmp(&y),
        _ => a.to_lowercase().cmp(&b.to_lowercase()),
    }
}

fn keep(r: &Row, q: &Query) -> bool {
    let from_ok = q.from.is_empty()
        || q.from.iter().any(|f| match f.strip_prefix('#') {
            Some(tag) => r.tags.iter().any(|t| t.eq_ignore_ascii_case(tag) || t.to_lowercase().starts_with(&format!("{}/", tag.to_lowercase()))),
            None => r.rel.starts_with(f.trim_start_matches('/')),
        });
    from_ok
        && q.wheres.iter().all(|(k, op, v)| {
            let x = r.get(k);
            match op.as_str() {
                "=" => x.split(", ").any(|p| p.eq_ignore_ascii_case(v)) || x.eq_ignore_ascii_case(v),
                "!=" => !x.eq_ignore_ascii_case(v),
                ">" => !x.is_empty() && cmp_vals(&x, v).is_gt(),
                "<" => !x.is_empty() && cmp_vals(&x, v).is_lt(),
                ">=" => !x.is_empty() && cmp_vals(&x, v).is_ge(),
                "<=" => !x.is_empty() && cmp_vals(&x, v).is_le(),
                _ => x.to_lowercase().contains(&v.to_lowercase()),
            }
        })
}

/// Les notes qui répondent à la requête, triées, puis groupées (nom du groupe, notes)
pub fn run(q: &Query, all: &[Row]) -> Vec<(String, Vec<Row>)> {
    let mut v: Vec<Row> = all.iter().filter(|r| keep(r, q)).cloned().collect();
    match &q.sort {
        Some((k, desc)) => v.sort_by(|a, b| {
            let o = cmp_vals(&a.get(k), &b.get(k));
            if *desc {
                o.reverse()
            } else {
                o
            }
        }),
        None => v.sort_by(|a, b| a.name().to_lowercase().cmp(&b.name().to_lowercase())),
    }
    v.truncate(q.limit);
    match &q.group {
        None => vec![(String::new(), v)],
        Some(g) => {
            let mut groups: Vec<(String, Vec<Row>)> = vec![];
            for r in v {
                let k = r.get(g);
                match groups.iter_mut().find(|(n, _)| *n == k) {
                    Some((_, rows)) => rows.push(r),
                    None => groups.push((k, vec![r])),
                }
            }
            groups.sort_by(|a, b| cmp_vals(&a.0, &b.0));
            groups
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requetes_de_base() {
        let a = parse_row("projets/Aurore.md", "---\nstatut: en cours\npriorité: 2\ntags: [court]\n---\n# Aurore\n- [ ] animer le plan 3\n- [x] storyboard", 0);
        let b = parse_row("projets/Brume.md", "---\nstatut: fini\npriorité: 1\n---\n", 0);
        let c = parse_row("notes/idée.md", "Une idée #court", 0);
        assert_eq!(a.tasks.len(), 2);
        assert!(a.tasks[1].1 && !a.tasks[0].1);
        let all = vec![a, b, c];
        let q = parse_query("from: projets/\nwhere: statut = en cours\ncolumns: file, statut");
        let r = run(&q, &all);
        assert_eq!(r[0].1.len(), 1);
        assert_eq!(r[0].1[0].get("file"), "Aurore");
        // Par tag, tri décroissant sur un nombre, groupes
        let q = parse_query("from: #court\nsort: file desc");
        assert_eq!(run(&q, &all)[0].1.iter().map(|r| r.name().to_string()).collect::<Vec<_>>(), ["idée", "Aurore"]);
        let q = parse_query("from: projets/\ngroup: statut\nsort: priorité");
        let g = run(&q, &all);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].0, "en cours");
        let q = parse_query("where: priorité >= 2");
        assert_eq!(run(&q, &all)[0].1.len(), 1);
    }
}
