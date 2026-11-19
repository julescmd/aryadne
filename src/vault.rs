//! Le coffre : un dossier ordinaire. Tous les chemins manipulés ici sont relatifs
//! à la racine du coffre, avec des « / » (même sur Windows).
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use serde::{Deserialize, Serialize};
use std::sync::{LazyLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

const SUBDIRS: [&str; 4] = ["notes", "refs", "boards", ".app"];
/// Dossiers jamais affichés ni indexés.
const IGNORED: [&str; 4] = ["target", "node_modules", "__pycache__", "dist"];

pub static WIKILINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|([^\]]+))?\]\]").unwrap());

#[derive(Clone, Debug)]
pub struct Entry {
    pub rel: String,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
    /// Dernière modification (secondes Unix).
    pub modified: u64,
}

#[derive(Clone)]
pub struct Backlink {
    pub rel: String,
    pub name: String,
    pub excerpt: String,
}

pub fn stem(rel: &str) -> String {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.strip_suffix(".md").unwrap_or(name).to_string()
}

pub fn parent(rel: &str) -> String {
    rel.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default()
}

pub fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") }
}

pub fn is_md(rel: &str) -> bool {
    rel.to_lowercase().ends_with(".md")
}

// ---------- fichiers et dossiers liés ----------
//
// Le coffre peut afficher des fichiers qui vivent ailleurs sur le disque, sans les copier.
// Dans l'application ils ont un chemin virtuel :
//   « @Nom/sous/dossier/fichier »  pour un dossier lié,
//   « @fichiers liés/nom.png »     pour un fichier lié seul.
// La liste est enregistrée dans .app/links.json du coffre.

pub const LINKED_FILES: &str = "@fichiers liés";

#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
pub struct LinkedFolder {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
pub struct Links {
    #[serde(default)]
    pub folders: Vec<LinkedFolder>,
    #[serde(default)]
    pub files: Vec<String>,
}

static LINKS: LazyLock<RwLock<Links>> = LazyLock::new(|| RwLock::new(Links::default()));

pub fn link_list() -> Links {
    LINKS.read().map(|l| l.clone()).unwrap_or_default()
}

pub fn load_links(root: &str) -> Links {
    let l: Links = fs::read_to_string(Path::new(root).join(".app/links.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    if let Ok(mut g) = LINKS.write() {
        *g = l.clone();
    }
    l
}

pub fn save_links(root: &str, l: &Links) -> Result<(), String> {
    if let Ok(mut g) = LINKS.write() {
        *g = l.clone();
    }
    fs::create_dir_all(Path::new(root).join(".app")).map_err(|e| e.to_string())?;
    fs::write(Path::new(root).join(".app/links.json"), serde_json::to_string_pretty(l).unwrap()).map_err(|e| e.to_string())
}

/// Chemin hors du coffre (dossier ou fichier lié) ?
pub fn is_external(rel: &str) -> bool {
    rel.starts_with('@')
}

/// Racine d'un dossier lié (« @Nom ») ou fichier lié seul.
pub fn is_link_root(rel: &str) -> bool {
    is_external(rel) && (!rel.contains('/') || rel.starts_with(&format!("{LINKED_FILES}/")))
}

/// Noms affichés des fichiers liés (sans doublons) → chemin réel.
pub fn linked_files(l: &Links) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = vec![];
    for f in &l.files {
        let p = PathBuf::from(f);
        let base = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| f.clone());
        let mut name = base.clone();
        let mut i = 2;
        while out.iter().any(|(n, _)| *n == name) {
            name = match base.rsplit_once('.') {
                Some((b, e)) => format!("{b} ({i}).{e}"),
                None => format!("{base} ({i})"),
            };
            i += 1;
        }
        out.push((name, p));
    }
    out
}

/// Ajoute un fichier ou un dossier externe sans le copier. Retourne son chemin virtuel.
pub fn link_external(root: &str, src: &Path) -> Result<String, String> {
    let abs = fs::canonicalize(src).map_err(|e| e.to_string())?;
    let mut l = link_list();
    if abs.is_dir() {
        if let Some(f) = l.folders.iter().find(|f| Path::new(&f.path) == abs) {
            return Ok(format!("@{}", f.name));
        }
        let base = abs.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "dossier".into());
        let base: String = base.chars().filter(|c| !"\\/:*?\"<>|".contains(*c)).collect();
        let mut name = base.clone();
        let mut i = 2;
        while l.folders.iter().any(|f| f.name == name) || format!("@{name}") == LINKED_FILES {
            name = format!("{base} {i}");
            i += 1;
        }
        l.folders.push(LinkedFolder { name: name.clone(), path: abs.to_string_lossy().to_string() });
        save_links(root, &l)?;
        Ok(format!("@{name}"))
    } else {
        let s = abs.to_string_lossy().to_string();
        if !l.files.contains(&s) {
            l.files.push(s.clone());
            save_links(root, &l)?;
        }
        let name = linked_files(&l).into_iter().find(|(_, p)| p.to_string_lossy() == s).map(|(n, _)| n).unwrap_or_default();
        Ok(format!("{LINKED_FILES}/{name}"))
    }
}

/// Retire un lien (le fichier ou dossier d'origine n'est jamais touché).
pub fn unlink(root: &str, rel: &str) -> Result<(), String> {
    let mut l = link_list();
    if let Some(name) = rel.strip_prefix(&format!("{LINKED_FILES}/")) {
        let target = linked_files(&l).into_iter().find(|(n, _)| n == name).map(|(_, p)| p);
        l.files.retain(|f| Some(PathBuf::from(f)) != target);
    } else if rel == LINKED_FILES {
        l.files.clear();
    } else if let Some(name) = rel.strip_prefix('@') {
        l.folders.retain(|f| f.name != name);
    }
    save_links(root, &l)
}

fn check_parts(rel: &str) -> Result<(), String> {
    let p = Path::new(rel);
    if p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("Chemin invalide : « {rel} »"));
    }
    if rel.split('/').any(|part| part.contains(['\\', ':', '*', '?', '"', '<', '>', '|'])) {
        return Err(format!("Caractère interdit dans « {rel} »"));
    }
    Ok(())
}

/// Chemin réel d'un chemin du coffre. Refuse « .. » et les chemins absolus.
/// Gère les dossiers liés (« @Nom/... ») et les fichiers liés.
pub fn safe(root: &str, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim().trim_matches('/');
    if rel.is_empty() {
        return Err("Chemin vide.".into());
    }
    if let Some(rest) = rel.strip_prefix('@') {
        let (name, sub) = rest.split_once('/').unwrap_or((rest, ""));
        let l = link_list();
        if format!("@{name}") == LINKED_FILES {
            if sub.is_empty() {
                return Err("« fichiers liés » est un dossier virtuel : crée tes fichiers ailleurs.".into());
            }
            return linked_files(&l).into_iter().find(|(n, _)| n == sub).map(|(_, p)| p).ok_or_else(|| format!("Fichier lié introuvable : {sub}"));
        }
        let folder = l.folders.iter().find(|f| f.name == name).ok_or_else(|| format!("Dossier lié introuvable : {name}"))?;
        if sub.is_empty() {
            return Ok(PathBuf::from(&folder.path));
        }
        check_parts(sub)?;
        return Ok(Path::new(&folder.path).join(sub));
    }
    check_parts(rel)?;
    Ok(Path::new(root).join(rel))
}

pub fn init(root: &str) -> Result<(), String> {
    let r = Path::new(root);
    if !r.is_dir() {
        return Err("Ce dossier n'existe pas.".into());
    }
    let fresh = fs::read_dir(r).map_err(|e| e.to_string())?.next().is_none();
    for d in SUBDIRS {
        fs::create_dir_all(r.join(d)).map_err(|e| e.to_string())?;
    }
    if fresh {
        fs::write(
            r.join("notes/Bienvenue.md"),
            "# Bienvenue\n\nCeci est ton coffre : un dossier ordinaire sur ton disque.\n\n- Relie tes notes avec des doubles crochets : [[Idées]].\n- Ouvre la vue Cerveau pour voir tout le réseau.\n- Clic droit dans l'explorateur pour créer, renommer ou supprimer.\n- Glisse des fichiers dans la fenêtre pour les importer.\n",
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn walk(base: &Path, dir: &Path, prefix: &str, depth: usize, out: &mut Vec<Entry>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut items: Vec<_> = rd.flatten().collect();
    items.sort_by_key(|e| {
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        (!is_dir, e.file_name().to_string_lossy().to_lowercase())
    });
    for e in items {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || IGNORED.contains(&name.as_str()) {
            continue;
        }
        let path = e.path();
        let sub = path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
        let rel = join(prefix, &sub);
        let is_dir = path.is_dir();
        out.push(Entry { rel, name, is_dir, depth, modified: mtime(&path) });
        if is_dir && depth < 24 {
            walk(base, &path, prefix, depth + 1, out);
        }
    }
}

fn mtime(p: &Path) -> u64 {
    fs::metadata(p).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
}

/// Arborescence complète (dossiers d'abord, ordre alphabétique), sans fichiers cachés,
/// suivie des dossiers et fichiers liés.
pub fn scan(root: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    walk(Path::new(root), Path::new(root), "", 0, &mut out);
    let l = link_list();
    for f in &l.folders {
        let rel = format!("@{}", f.name);
        out.push(Entry { rel: rel.clone(), name: format!("@{}", f.name), is_dir: true, depth: 0, modified: mtime(Path::new(&f.path)) });
        walk(Path::new(&f.path), Path::new(&f.path), &rel, 1, &mut out);
    }
    let files = linked_files(&l);
    if !files.is_empty() {
        out.push(Entry { rel: LINKED_FILES.into(), name: LINKED_FILES.into(), is_dir: true, depth: 0, modified: 0 });
        for (name, p) in files {
            out.push(Entry { rel: format!("{LINKED_FILES}/{name}"), name, is_dir: false, depth: 1, modified: mtime(&p) });
        }
    }
    out
}

/// Déplace un fichier, même d'un disque à l'autre (copie puis suppression).
fn move_path(a: &Path, b: &Path) -> Result<(), String> {
    if fs::rename(a, b).is_ok() {
        return Ok(());
    }
    if a.is_dir() {
        return Err("Impossible de déplacer un dossier vers un autre disque.".into());
    }
    fs::copy(a, b).map_err(|e| e.to_string())?;
    fs::remove_file(a).map_err(|e| format!("Copié, mais l'original n'a pas pu être supprimé : {e}"))
}

pub fn read(root: &str, rel: &str) -> Result<String, String> {
    let bytes = fs::read(safe(root, rel)?).map_err(|e| e.to_string())?;
    if bytes.iter().take(8000).any(|b| *b == 0) {
        return Err(format!("« {rel} » est un fichier binaire."));
    }
    String::from_utf8(bytes).map_err(|_| format!("« {rel} » n'est pas du texte UTF-8."))
}

pub fn write(root: &str, rel: &str, text: &str) -> Result<(), String> {
    fs::write(safe(root, rel)?, text).map_err(|e| e.to_string())
}

/// Crée un fichier vide (ou un titre pour un .md). Ne touche jamais un fichier existant.
pub fn create_file(root: &str, rel: &str) -> Result<(), String> {
    let p = safe(root, rel)?;
    if p.exists() {
        return Ok(());
    }
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let content = if is_md(rel) { format!("# {}\n\n", stem(rel)) } else { String::new() };
    fs::write(p, content).map_err(|e| e.to_string())
}

pub fn create_dir(root: &str, rel: &str) -> Result<(), String> {
    fs::create_dir_all(safe(root, rel)?).map_err(|e| e.to_string())
}

/// Renomme ou déplace (le nouveau chemin peut être dans un autre dossier).
pub fn rename(root: &str, from: &str, to: &str) -> Result<(), String> {
    let (a, b) = (safe(root, from)?, safe(root, to)?);
    if b.exists() {
        return Err(format!("« {to} » existe déjà."));
    }
    if b.starts_with(&a) {
        return Err("Impossible de déplacer un dossier dans lui-même.".into());
    }
    if is_link_root(from) && is_external(to) {
        return Err("Un élément lié se renomme depuis son dossier d'origine.".into());
    }
    if let Some(dir) = b.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    move_path(&a, &b)?;
    // Un fichier lié déplacé dans le coffre n'est plus un lien.
    if from.starts_with(&format!("{LINKED_FILES}/")) {
        let mut l = link_list();
        l.files.retain(|f| Path::new(f) != a);
        save_links(root, &l)?;
    }
    Ok(())
}

/// « Supprimer » déplace dans .app/trash : rien n'est jamais perdu par accident.
pub fn trash(root: &str, rel: &str) -> Result<(), String> {
    if is_link_root(rel) || rel == LINKED_FILES {
        return unlink(root, rel);
    }
    let src = safe(root, rel)?;
    let bin = Path::new(root).join(".app/trash");
    fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let name = rel.rsplit('/').next().unwrap_or(rel);
    move_path(&src, &bin.join(format!("{ts}_{name}")))
}

/// Si `src` est déjà dans le coffre, son chemin relatif.
pub fn inside(root: &str, src: &Path) -> Option<String> {
    let src = fs::canonicalize(src).ok()?;
    let rootp = fs::canonicalize(root).ok()?;
    if let Ok(r) = src.strip_prefix(&rootp) {
        let rel = r.to_string_lossy().replace('\\', "/");
        return (!rel.is_empty()).then_some(rel);
    }
    // Dans un dossier déjà lié ?
    let l = link_list();
    for f in &l.folders {
        if let Ok(r) = src.strip_prefix(fs::canonicalize(&f.path).ok()?) {
            return Some(join(&format!("@{}", f.name), &r.to_string_lossy().replace('\\', "/")));
        }
    }
    let s = src.to_string_lossy();
    linked_files(&l).into_iter().find(|(_, p)| p.to_string_lossy() == s).map(|(n, _)| format!("{LINKED_FILES}/{n}"))
}

/// Importe un fichier externe dans le dossier `dir` du coffre, sans écraser.
/// `move_file` : déplace au lieu de copier (pas de doublon sur le disque).
pub fn import(root: &str, dir: &str, src: &Path, move_file: bool) -> Result<String, String> {
    let name = src.file_name().ok_or("Fichier sans nom.")?.to_string_lossy().to_string();
    let (base, ext) = match name.rsplit_once('.') {
        Some((b, e)) if !b.is_empty() => (b.to_string(), format!(".{e}")),
        _ => (name.clone(), String::new()),
    };
    let mut rel = join(dir, &name);
    let mut i = 2;
    while safe(root, &rel)?.exists() {
        rel = join(dir, &format!("{base} {i}{ext}"));
        i += 1;
    }
    if src.is_dir() {
        return Err(format!("« {name} » est un dossier : glisse plutôt les fichiers qu'il contient."));
    }
    let dest = safe(root, &rel)?;
    if move_file {
        move_path(src, &dest)?;
    } else {
        fs::copy(src, &dest).map_err(|e| e.to_string())?;
    }
    Ok(rel)
}

/// Nom de note (en minuscules) → chemin du .md.
pub fn md_index(entries: &[Entry]) -> HashMap<String, String> {
    entries
        .iter()
        .filter(|e| !e.is_dir && is_md(&e.rel))
        .map(|e| (stem(&e.rel).to_lowercase(), e.rel.clone()))
        .collect()
}

pub fn links(text: &str) -> Vec<String> {
    WIKILINK.captures_iter(text).map(|c| c[1].trim().to_string()).collect()
}

pub fn backlinks(root: &str, entries: &[Entry], target_rel: &str) -> Vec<Backlink> {
    let target = stem(target_rel).to_lowercase();
    let mut out = Vec::new();
    for e in entries.iter().filter(|e| !e.is_dir && is_md(&e.rel) && e.rel != target_rel) {
        let text = read(root, &e.rel).unwrap_or_default();
        if let Some(line) = text.lines().find(|l| links(l).iter().any(|t| t.to_lowercase() == target)) {
            out.push(Backlink {
                rel: e.rel.clone(),
                name: stem(&e.rel),
                excerpt: line.trim().chars().take(140).collect(),
            });
        }
    }
    out
}

/// Graphe du coffre. Nœuds : notes (et dossiers si demandé). Arêtes : liens [[ ]] (+ appartenance).
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<(usize, usize, bool)>, // (a, b, est_un_lien_de_dossier)
}

pub struct GraphNode {
    pub rel: String,
    pub label: String,
    pub is_dir: bool,
}

pub fn graph(root: &str, entries: &[Entry], with_folders: bool, with_files: bool) -> Graph {
    let mut nodes = Vec::new();
    let mut idx: HashMap<String, usize> = HashMap::new();
    if with_folders {
        // Nœud racine : tout le coffre se rattache à lui.
        let name = Path::new(root).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "~".into());
        idx.insert(String::new(), 0);
        nodes.push(GraphNode { rel: String::new(), label: format!("~/{name}"), is_dir: true });
    }
    for e in entries {
        let keep = if e.is_dir { with_folders } else { is_md(&e.rel) || with_files };
        if keep {
            idx.insert(e.rel.clone(), nodes.len());
            let label = if is_md(&e.rel) { stem(&e.rel) } else { e.name.clone() };
            nodes.push(GraphNode { rel: e.rel.clone(), label, is_dir: e.is_dir });
        }
    }
    let notes = md_index(entries);
    let mut edges = Vec::new();
    for (i, n) in nodes.iter().enumerate() {
        if with_folders {
            if let Some(&p) = idx.get(&parent(&n.rel)).filter(|&&p| p != i) {
                edges.push((p, i, true));
            }
        }
        if !n.is_dir && is_md(&n.rel) {
            let text = read(root, &n.rel).unwrap_or_default();
            for t in links(&text) {
                if let Some(&j) = notes.get(&t.to_lowercase()).and_then(|r| idx.get(r)) {
                    if i != j && !edges.iter().any(|&(a, b, _)| (a, b) == (i, j) || (a, b) == (j, i)) {
                        edges.push((i, j, false));
                    }
                }
            }
        }
    }
    Graph { nodes, edges }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coffre_complet() {
        let dir = std::env::temp_dir().join("aryadne-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let v = dir.to_string_lossy().to_string();
        init(&v).unwrap();
        create_file(&v, "notes/Idées.md").unwrap();
        create_file(&v, "code/main.rs").unwrap();
        write(&v, "notes/Bienvenue.md", "Voir [[idées|mes idées]]").unwrap();
        let entries = scan(&v);
        assert!(entries.iter().any(|e| e.rel == "code/main.rs" && e.depth == 1));
        assert!(!entries.iter().any(|e| e.rel.starts_with(".app")));
        let bl = backlinks(&v, &entries, "notes/Idées.md");
        assert_eq!(bl.len(), 1);
        assert_eq!(bl[0].name, "Bienvenue");
        let g = graph(&v, &entries, true, false);
        assert!(g.edges.iter().any(|e| !e.2));
        assert!(safe(&v, "../evil").is_err());
        assert!(safe(&v, "/etc/passwd").unwrap().starts_with(&dir));
        assert!(safe(&v, "C:\\Windows").is_err());
        rename(&v, "code/main.rs", "notes/main.rs").unwrap();
        assert!(rename(&v, "notes", "notes/sous").is_err());
        trash(&v, "notes/main.rs").unwrap();
        assert!(!dir.join("notes/main.rs").exists());
        let src = std::env::temp_dir().join("aryadne-import.txt");
        fs::write(&src, "x").unwrap();
        assert_eq!(import(&v, "refs", &src, false).unwrap(), "refs/aryadne-import.txt");
        assert_eq!(import(&v, "refs", &src, true).unwrap(), "refs/aryadne-import 2.txt");
        assert!(!src.exists(), "déplacer ne doit pas laisser de doublon");
        assert_eq!(inside(&v, &dir.join("refs/aryadne-import.txt")).as_deref(), Some("refs/aryadne-import.txt"));
        assert!(inside(&v, &std::env::temp_dir()).is_none());

        // Liens : rien n'est copié, le fichier reste à sa place.
        let ext = std::env::temp_dir().join("aryadne-ext");
        let _ = fs::remove_dir_all(&ext);
        fs::create_dir_all(ext.join("sous")).unwrap();
        fs::write(ext.join("sous/photo.png"), "x").unwrap();
        fs::write(ext.join("seul.txt"), "salut").unwrap();
        let d = link_external(&v, &ext).unwrap();
        assert_eq!(d, "@aryadne-ext");
        let f = link_external(&v, &ext.join("seul.txt")).unwrap();
        assert_eq!(f, "@fichiers liés/seul.txt");
        let all = scan(&v);
        assert!(all.iter().any(|e| e.rel == "@aryadne-ext/sous/photo.png" && e.depth == 2));
        assert_eq!(read(&v, "@fichiers liés/seul.txt").unwrap(), "salut");
        assert!(!dir.join("seul.txt").exists());
        assert!(safe(&v, "@aryadne-ext/../x").is_err());
        assert_eq!(inside(&v, &ext.join("sous/photo.png")).as_deref(), Some("@aryadne-ext/sous/photo.png"));
        // Supprimer un lien ne supprime jamais l'original.
        trash(&v, "@fichiers liés/seul.txt").unwrap();
        assert!(ext.join("seul.txt").exists());
        trash(&v, "@aryadne-ext").unwrap();
        assert!(ext.join("sous/photo.png").exists());
        assert!(link_list().folders.is_empty() && link_list().files.is_empty());
    }
}
