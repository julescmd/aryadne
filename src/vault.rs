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
    crate::vault::atomic_write(Path::new(root).join(".app/links.json"), serde_json::to_string_pretty(l).unwrap()).map_err(|e| e.to_string())
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
        return Ok(join_parts(Path::new(&folder.path), sub));
    }
    check_parts(rel)?;
    Ok(join_parts(Path::new(root), rel))
}

/// Ajoute un chemin « a/b/c » segment par segment : séparateurs natifs (\ sous Windows).
fn join_parts(base: &Path, rel: &str) -> PathBuf {
    let mut p = base.to_path_buf();
    for part in rel.split('/').filter(|s| !s.is_empty()) {
        p.push(part);
    }
    p
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
        crate::vault::atomic_write(
            r.join("notes/Bienvenue.md"),
            "# Bienvenue\n\nCeci est ton coffre : un dossier ordinaire sur ton disque.\n\n- Relie tes notes avec des doubles crochets : [[Idées]].\n- Ouvre la vue Cerveau pour voir tout le réseau.\n- Clic droit dans l'explorateur pour créer, renommer ou supprimer.\n- Glisse des fichiers dans la fenêtre pour les importer.\n",
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn walk(base: &Path, dir: &Path, prefix: &str, depth: usize, out: &mut Vec<Entry>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    // Un seul appel système par élément : le type et la date viennent des infos du dossier
    // (gratuites sous Windows), au lieu de trois lectures séparées
    let mut items: Vec<(String, bool, u64, std::path::PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || IGNORED.contains(&name.as_str()) {
                return None;
            }
            let path = e.path();
            // Lien symbolique : on suit la cible (comme avant avec is_dir et metadata)
            let md = match e.file_type() {
                Ok(t) if t.is_symlink() => fs::metadata(&path).ok(),
                _ => e.metadata().ok(),
            };
            let is_dir = md.as_ref().is_some_and(|m| m.is_dir());
            let modified = md.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            Some((name, is_dir, modified, path))
        })
        .collect();
    items.sort_by_cached_key(|(name, is_dir, _, _)| (!*is_dir, name.to_lowercase()));
    for (name, is_dir, modified, path) in items {
        let sub = path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
        let rel = join(prefix, &sub);
        out.push(Entry { rel, name, is_dir, depth, modified });
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
    let mut own = Vec::new();
    walk(Path::new(root), Path::new(root), "", 0, &mut own);
    // On regroupe par élément de premier niveau pour mêler les dossiers et fichiers liés
    // aux autres, comme s'ils étaient dans le coffre.
    let mut groups: Vec<Vec<Entry>> = vec![];
    for e in own {
        if e.depth == 0 {
            groups.push(vec![e]);
        } else if let Some(g) = groups.last_mut() {
            g.push(e);
        }
    }
    let l = link_list();
    for f in &l.folders {
        let rel = format!("@{}", f.name);
        let mut g = vec![Entry { rel: rel.clone(), name: f.name.clone(), is_dir: true, depth: 0, modified: mtime(Path::new(&f.path)) }];
        walk(Path::new(&f.path), Path::new(&f.path), &rel, 1, &mut g);
        groups.push(g);
    }
    for (name, p) in linked_files(&l) {
        groups.push(vec![Entry { rel: format!("{LINKED_FILES}/{name}"), name, is_dir: false, depth: 0, modified: mtime(&p) }]);
    }
    groups.sort_by_key(|g| (!g[0].is_dir, g[0].name.to_lowercase()));
    groups.into_iter().flatten().collect()
}

/// Chemin tel qu'on l'affiche : sans le « @ » des dossiers liés ni le dossier virtuel
/// des fichiers liés.
pub fn display_rel(rel: &str) -> String {
    if let Some(rest) = rel.strip_prefix(".app/untitled/") {
        return format!("{} · {}", rest.rsplit('/').next().unwrap_or(rest), crate::i18n::t("non enregistré (Ctrl+S)"));
    }
    if let Some(rest) = rel.strip_prefix(&format!("{LINKED_FILES}/")) {
        return rest.to_string();
    }
    rel.strip_prefix('@').unwrap_or(rel).to_string()
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

/// Écriture sûre : le contenu va d'abord dans un fichier temporaire du même dossier, puis
/// le remplace d'un coup. Une coupure ou un plantage en plein milieu laisse l'ancienne
/// version intacte au lieu d'un fichier tronqué. Un lien symbolique est écrit directement.
pub fn atomic_write(path: impl AsRef<std::path::Path>, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    use std::io::Write;
    let path = path.as_ref();
    if fs::symlink_metadata(path).map(|m| m.file_type().is_symlink()).unwrap_or(false) {
        return fs::write(path, contents);
    }
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."));
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let res = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(contents.as_ref())?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
        // Dossier en lecture seule pour les nouveaux fichiers, partage réseau capricieux… : écriture directe
        return fs::write(path, contents);
    }
    res
}

pub fn write(root: &str, rel: &str, text: &str) -> Result<(), String> {
    crate::vault::atomic_write(safe(root, rel)?, text).map_err(|e| e.to_string())
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
    crate::vault::atomic_write(p, content).map_err(|e| e.to_string())
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
    trash_to(root, rel).map(|_| ())
}

/// Remet à sa place un élément mis à la corbeille (pour annuler une suppression).
pub fn restore(root: &str, bin: &Path, rel: &str) -> Result<(), String> {
    let dst = safe(root, rel)?;
    if dst.exists() {
        return Err(format!("« {rel} » existe déjà."));
    }
    if let Some(dir) = dst.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    move_path(bin, &dst)
}

/// Comme `trash`, en renvoyant où l'élément a été rangé dans la corbeille (None pour un lien simplement retiré).
pub fn trash_to(root: &str, rel: &str) -> Result<Option<PathBuf>, String> {
    if is_link_root(rel) || rel == LINKED_FILES {
        return unlink(root, rel).map(|_| None);
    }
    let src = safe(root, rel)?;
    let bin = Path::new(root).join(".app/trash");
    fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let name = rel.rsplit('/').next().unwrap_or(rel);
    // Deux éléments du même nom supprimés dans la même seconde ne doivent pas s'écraser
    let mut dst = bin.join(format!("{ts}_{name}"));
    let mut n = 2;
    while dst.exists() {
        dst = bin.join(format!("{ts}-{n}_{name}"));
        n += 1;
    }
    move_path(&src, &dst).map(|_| Some(dst))
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


/// Note qui écrit le nom d'une autre note sans en faire un lien [[ ]].
#[derive(Clone)]
pub struct Mention {
    pub rel: String,
    pub name: String,
    pub count: usize,
    pub excerpt: String,
}

/// Positions (en octets) des mentions non liées de `name` dans `text` : mot entier, sans tenir
/// compte de la casse, hors liens [[ ]], hors blocs de code et hors `code`.
pub fn unlinked_spans(text: &str, name: &str) -> Vec<(usize, usize)> {
    let want: Vec<char> = name.chars().flat_map(|c| c.to_lowercase()).collect();
    if want.len() < 3 {
        return vec![];
    }
    let mut masked = vec![false; text.len()];
    for m in WIKILINK.find_iter(text) {
        masked[m.start()..m.end()].iter_mut().for_each(|b| *b = true);
    }
    // Blocs ``` et code en ligne
    let mut at = 0;
    let mut fence = false;
    for line in text.split_inclusive('\n') {
        let t = line.trim_start();
        if t.starts_with("```") {
            fence = !fence;
            masked[at..at + line.len()].iter_mut().for_each(|b| *b = true);
        } else if fence {
            masked[at..at + line.len()].iter_mut().for_each(|b| *b = true);
        } else {
            let mut open = None;
            for (i, c) in line.char_indices() {
                if c == '`' {
                    match open.take() {
                        None => open = Some(i),
                        Some(o) => masked[at + o..at + i + 1].iter_mut().for_each(|b| *b = true),
                    }
                }
            }
        }
        at += line.len();
    }
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    let mut prev: Option<char> = None;
    let mut skip_until = 0;
    for (i, c) in text.char_indices() {
        if i >= skip_until && !masked[i] && !prev.is_some_and(word) {
            let mut it = text[i..].char_indices();
            let mut ok = true;
            let mut end = i;
            let mut k = 0;
            while k < want.len() {
                match it.next() {
                    Some((j, ch)) => {
                        for lc in ch.to_lowercase() {
                            if want.get(k) != Some(&lc) {
                                ok = false;
                            }
                            k += 1;
                        }
                        end = i + j + ch.len_utf8();
                        if !ok {
                            break;
                        }
                    }
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok && k == want.len() && !text[end..].chars().next().is_some_and(word) && !masked[i..end].iter().any(|b| *b) {
                out.push((i, end));
                skip_until = end;
            }
        }
        prev = Some(c);
    }
    out
}

/// Transforme la mention `n` (dans l'ordre du texte) en lien : [[Nom]] si la casse est la même, sinon [[Nom|texte]].
pub fn link_mention(text: &str, name: &str, n: usize) -> Option<String> {
    let (a, b) = *unlinked_spans(text, name).get(n)?;
    let seen = &text[a..b];
    let link = if seen == name { format!("[[{name}]]") } else { format!("[[{name}|{seen}]]") };
    Some(format!("{}{}{}", &text[..a], link, &text[b..]))
}

pub fn mentions(root: &str, entries: &[Entry], target_rel: &str, open: &dyn Fn(&str) -> Option<String>) -> Vec<Mention> {
    let name = stem(target_rel);
    let mut out = Vec::new();
    for e in entries.iter().filter(|e| !e.is_dir && is_md(&e.rel) && e.rel != target_rel) {
        let text = open(&e.rel).unwrap_or_else(|| read(root, &e.rel).unwrap_or_default());
        let spans = unlinked_spans(&text, &name);
        if let Some(&(a, b)) = spans.first() {
            let ls = text[..a].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let le = text[b..].find('\n').map(|i| b + i).unwrap_or(text.len());
            out.push(Mention { rel: e.rel.clone(), name: stem(&e.rel), count: spans.len(), excerpt: text[ls..le].trim().chars().take(140).collect() });
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

/// Le graphe local d'une note : elle et ses voisines jusqu'à `depth` liens (sans les dossiers)
pub fn local_graph(g: Graph, center: &str, depth: usize) -> Graph {
    let Some(c) = g.nodes.iter().position(|n| n.rel == center) else { return g };
    let mut dist = vec![usize::MAX; g.nodes.len()];
    dist[c] = 0;
    let mut frontier = vec![c];
    for d in 1..=depth {
        let mut next = vec![];
        for &(a, b, folder) in &g.edges {
            if folder {
                continue;
            }
            for (x, y) in [(a, b), (b, a)] {
                if frontier.contains(&x) && dist[y] == usize::MAX {
                    dist[y] = d;
                    next.push(y);
                }
            }
        }
        frontier = next;
    }
    let keep: Vec<usize> = (0..g.nodes.len()).filter(|&i| dist[i] != usize::MAX && !g.nodes[i].is_dir).collect();
    let map: HashMap<usize, usize> = keep.iter().enumerate().map(|(n, &o)| (o, n)).collect();
    let edges = g.edges.iter().filter_map(|&(a, b, f)| Some((*map.get(&a)?, *map.get(&b)?, f))).collect();
    let mut nodes = g.nodes;
    let mut out = vec![];
    for (i, n) in nodes.drain(..).enumerate() {
        if map.contains_key(&i) {
            out.push(n);
        }
    }
    Graph { nodes: out, edges }
}

pub fn graph(root: &str, entries: &[Entry], with_folders: bool, with_files: bool, scope: &str) -> Graph {
    let mut nodes = Vec::new();
    let mut idx: HashMap<String, usize> = HashMap::new();
    if with_folders {
        // Nœud racine : le dossier affiché (tout le coffre par défaut).
        let label = if scope.is_empty() {
            let name = Path::new(root).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "~".into());
            format!("~/{name}")
        } else {
            scope.rsplit('/').next().unwrap_or(scope).to_string()
        };
        idx.insert(scope.to_string(), 0);
        nodes.push(GraphNode { rel: scope.to_string(), label, is_dir: true });
    }
    let prefix = format!("{scope}/");
    for e in entries.iter().filter(|e| scope.is_empty() || e.rel.starts_with(&prefix)) {
        let keep = if e.is_dir { with_folders } else { is_md(&e.rel) || with_files };
        if keep {
            idx.insert(e.rel.clone(), nodes.len());
            let label = if is_md(&e.rel) { stem(&e.rel) } else { e.name.clone() };
            nodes.push(GraphNode { rel: e.rel.clone(), label, is_dir: e.is_dir });
        }
    }
    let notes = md_index(entries);
    let mut edges = Vec::new();
    // Paires déjà reliées (chercher dans la liste des liens coûtait le carré du nombre de liens)
    let mut seen: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    for (i, n) in nodes.iter().enumerate() {
        if with_folders {
            let par = parent(&n.rel);
            let par = if par == LINKED_FILES { String::new() } else { par };
            if let Some(&p) = idx.get(&par).filter(|&&p| p != i) {
                edges.push((p, i, true));
            }
        }
        if !n.is_dir && is_md(&n.rel) {
            let text = read(root, &n.rel).unwrap_or_default();
            for t in links(&text) {
                if let Some(&j) = notes.get(&t.to_lowercase()).and_then(|r| idx.get(r)) {
                    // Une paire de notes n'est reliée qu'une fois, dans un sens ou dans l'autre
                    if i != j && seen.insert((i.min(j), i.max(j))) {
                        edges.push((i, j, false));
                    }
                }
            }
        }
    }
    Graph { nodes, edges }
}

#[cfg(test)]
mod mention_tests {
    use super::*;
    #[test]
    fn mentions_non_liees() {
        let t = "Voir Aurore et [[Aurore]], pas aurores.\n```\nAurore\n```\nl'aurore `Aurore` ok";
        let s = unlinked_spans(t, "Aurore");
        assert_eq!(s.len(), 2);
        assert_eq!(&t[s[0].0..s[0].1], "Aurore");
        assert_eq!(&t[s[1].0..s[1].1], "aurore");
        let l = link_mention(t, "Aurore", 1).unwrap();
        assert!(l.contains("l'[[Aurore|aurore]] `Aurore`"));
        assert!(unlinked_spans("Él", "Él").is_empty());
        assert_eq!(unlinked_spans("Une Étoile.", "étoile").len(), 1);
    }
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
        let g = graph(&v, &entries, true, false, "");
        assert!(g.edges.iter().any(|e| !e.2));
        // Deux notes qui se citent l'une l'autre, plusieurs fois : un seul lien entre elles
        write(&v, "notes/Idées.md", "Retour vers [[Bienvenue]], encore [[Bienvenue]].").unwrap();
        let g2 = graph(&v, &entries, false, false, "");
        assert_eq!(g2.edges.iter().filter(|e| !e.2).count(), 1);
        write(&v, "notes/Idées.md", "").unwrap();
        assert!(safe(&v, "../evil").is_err());
        assert!(safe(&v, "/etc/passwd").unwrap().starts_with(&dir));
        assert!(safe(&v, "C:\\Windows").is_err());
        rename(&v, "code/main.rs", "notes/main.rs").unwrap();
        assert!(rename(&v, "notes", "notes/sous").is_err());
        trash(&v, "notes/main.rs").unwrap();
        assert!(!dir.join("notes/main.rs").exists());
        let src = std::env::temp_dir().join("aryadne-import.txt");
        crate::vault::atomic_write(&src, "x").unwrap();
        assert_eq!(import(&v, "refs", &src, false).unwrap(), "refs/aryadne-import.txt");
        assert_eq!(import(&v, "refs", &src, true).unwrap(), "refs/aryadne-import 2.txt");
        assert!(!src.exists(), "déplacer ne doit pas laisser de doublon");
        assert_eq!(inside(&v, &dir.join("refs/aryadne-import.txt")).as_deref(), Some("refs/aryadne-import.txt"));
        assert!(inside(&v, &std::env::temp_dir()).is_none());

        // Liens : rien n'est copié, le fichier reste à sa place.
        let ext = std::env::temp_dir().join("aryadne-ext");
        let _ = fs::remove_dir_all(&ext);
        fs::create_dir_all(ext.join("sous")).unwrap();
        crate::vault::atomic_write(ext.join("sous/photo.png"), "x").unwrap();
        crate::vault::atomic_write(ext.join("seul.txt"), "salut").unwrap();
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

#[cfg(test)]
mod atomic_tests {
    #[test]
    fn ecriture_sure() {
        let d = std::env::temp_dir().join(format!("aryadne-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("note.md");
        super::atomic_write(&f, "un").unwrap();
        super::atomic_write(&f, "deux").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "deux");
        // Aucun fichier temporaire ne reste
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1);
        std::fs::remove_dir_all(&d).unwrap();
    }
}

#[cfg(test)]
mod scan_bench {
    /// Mesure : `cargo test --release scan_10k -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn scan_10k() {
        let d = std::env::temp_dir().join("aryadne-scan10k");
        if !d.exists() {
            for i in 0..100 {
                let sub = d.join(format!("dossier {i}"));
                std::fs::create_dir_all(&sub).unwrap();
                for j in 0..100 {
                    std::fs::write(sub.join(format!("note {j}.md")), "x").unwrap();
                }
            }
        }
        let t = std::time::Instant::now();
        let n = super::scan(d.to_str().unwrap()).len();
        eprintln!("{n} éléments en {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
    }
}

/// Propriétés de l'en-tête YAML d'une note (comme Obsidian) : (clé, valeurs) dans l'ordre, et le nombre de
/// lignes de l'en-tête (traits compris). Lit les formes simples : « clé: valeur », « clé: [a, b] » et les listes
/// « - a » sous la clé.
pub fn frontmatter(text: &str) -> Option<(Vec<(String, Vec<String>)>, usize)> {
    let mut lines = text.lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    let mut props: Vec<(String, Vec<String>)> = vec![];
    let unquote = |v: &str| v.trim().trim_matches('"').trim_matches('\'').to_string();
    for (n, l) in lines.enumerate() {
        if l.trim_end() == "---" || l.trim_end() == "..." {
            return Some((props, n + 2));
        }
        if let Some(item) = l.trim_start().strip_prefix("- ").filter(|_| l.starts_with(' ') || l.starts_with('-')) {
            if let Some(last) = props.last_mut() {
                last.1.push(unquote(item));
            }
            continue;
        }
        if let Some((k, v)) = l.split_once(':').filter(|(k, _)| !k.trim().is_empty() && !k.starts_with(' ')) {
            let v = v.trim();
            let vals = if let Some(inner) = v.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
                inner.split(',').map(unquote).filter(|x| !x.is_empty()).collect()
            } else if v.is_empty() {
                vec![]
            } else {
                vec![unquote(v)]
            };
            props.push((k.trim().to_string(), vals));
        }
    }
    None
}

/// Change une propriété de l'en-tête YAML (la crée au besoin, avec l'en-tête) ; `None` la retire.
/// Une liste s'écrit « clé: [a, b] », une valeur seule « clé: valeur » (entre guillemets si elle en a besoin).
pub fn set_front(text: &str, key: &str, vals: Option<&[String]>, list: bool) -> String {
    let key = key.trim();
    let quote = |v: &str| {
        if v.is_empty() || v.contains(": ") || v.contains('#') || v.starts_with(['[', '{', '"', '\'', '-', '*', '&', '!', '|', '>', '%', '@']) {
            format!("\"{}\"", v.replace('"', "\\\""))
        } else {
            v.to_string()
        }
    };
    let line = vals.map(|v| {
        if list || v.len() > 1 {
            format!("{key}: [{}]", v.iter().map(|x| quote(x)).collect::<Vec<_>>().join(", "))
        } else {
            format!("{key}: {}", v.first().map(|x| quote(x)).unwrap_or_default())
        }
    });
    let Some((_, n)) = frontmatter(text) else {
        return match line {
            Some(l) => format!("---\n{l}\n---\n{text}"),
            None => text.to_string(),
        };
    };
    let lines: Vec<&str> = text.lines().collect();
    // Les lignes de la clé : la sienne, puis ses « - élément » en dessous
    let mut out: Vec<String> = vec![];
    let mut i = 0;
    let mut done = false;
    while i < lines.len() {
        let l = lines[i];
        let is_key = i > 0 && i < n - 1 && l.split_once(':').is_some_and(|(k, _)| k.trim() == key && !k.starts_with(' '));
        if is_key {
            i += 1;
            while i < n - 1 && (lines[i].starts_with(' ') || lines[i].starts_with("- ")) {
                i += 1;
            }
            if let Some(l) = &line {
                out.push(l.clone());
            }
            done = true;
            continue;
        }
        if i == n - 1 && !done {
            if let Some(l) = &line {
                out.push(l.clone());
            }
        }
        out.push(l.to_string());
        i += 1;
    }
    let mut s = out.join("\n");
    if text.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// Les tags de l'en-tête (« tags: » ou « tag: »), sans le « # »
pub fn front_tags(text: &str) -> Vec<String> {
    frontmatter(text)
        .map(|(p, _)| p.into_iter().filter(|(k, _)| k.eq_ignore_ascii_case("tags") || k.eq_ignore_ascii_case("tag")).flat_map(|(_, v)| v).flat_map(|v| v.split_whitespace().map(|t| t.trim_start_matches('#').to_string()).collect::<Vec<_>>()).filter(|t| !t.is_empty()).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod front_tests {
    use super::*;

    #[test]
    fn proprietes_yaml() {
        let t = "---\ntitre: Le court\ntags: [court, \"pelico\"]\nauteurs:\n  - Jules\n  - Annabelle\nvide:\n---\n# Corps";
        let (p, n) = frontmatter(t).unwrap();
        assert_eq!(n, 8);
        assert_eq!(p[0], ("titre".into(), vec!["Le court".into()]));
        assert_eq!(p[1].1, vec!["court".to_string(), "pelico".into()]);
        assert_eq!(p[2].1, vec!["Jules".to_string(), "Annabelle".into()]);
        assert!(p[3].1.is_empty());
        assert_eq!(front_tags(t), vec!["court".to_string(), "pelico".into()]);
        assert_eq!(front_tags("---\ntags: a b\n---"), vec!["a".to_string(), "b".into()]);
        assert!(frontmatter("# pas d'en-tête").is_none());
        assert!(frontmatter("---\njamais fermé").is_none());
        // Modifier, ajouter, retirer
        let t2 = set_front(t, "titre", Some(&["Le long".to_string()]), false);
        assert!(t2.contains("titre: Le long\n"));
        let t3 = set_front(&t2, "auteurs", Some(&["Jules".to_string()]), true);
        assert!(t3.contains("auteurs: [Jules]\n") && !t3.contains("  - Annabelle"));
        let t4 = set_front(&t3, "statut", Some(&["à faire: vite".to_string()]), false);
        assert_eq!(frontmatter(&t4).unwrap().0.iter().find(|p| p.0 == "statut").unwrap().1, vec!["à faire: vite".to_string()]);
        let t5 = set_front(&t4, "vide", None, false);
        assert!(!t5.contains("vide:") && t5.ends_with("# Corps"));
        assert_eq!(set_front("# Note", "tags", Some(&["a".to_string()]), true), "---\ntags: [a]\n---\n# Note");
    }
}
