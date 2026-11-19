//! Le coffre : un dossier ordinaire. Les notes sont des fichiers .md dans notes/.
use std::sync::LazyLock;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

const SUBDIRS: [&str; 4] = ["notes", "refs", "boards", ".app"];

pub static WIKILINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[([^\]|#]+)(?:[#|][^\]]*)?\]\]").unwrap());

#[derive(Clone)]
pub struct NoteInfo {
    pub name: String,
    pub modified: u64,
}

#[derive(Clone)]
pub struct Backlink {
    pub name: String,
    pub excerpt: String,
}

fn notes_dir(vault: &str) -> PathBuf {
    Path::new(vault).join("notes")
}

/// Refuse les noms qui sortiraient du dossier notes/.
fn note_path(vault: &str, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty()
        || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        || name.starts_with('.')
    {
        return Err(format!("Nom de note invalide : « {name} »"));
    }
    Ok(notes_dir(vault).join(format!("{name}.md")))
}

/// Ouvre un dossier comme coffre et crée la structure si elle manque.
pub fn open_vault(path: String) -> Result<(), String> {
    let root = Path::new(&path);
    if !root.is_dir() {
        return Err("Ce dossier n'existe pas.".into());
    }
    for d in SUBDIRS {
        fs::create_dir_all(root.join(d)).map_err(|e| e.to_string())?;
    }
    let welcome = notes_dir(&path).join("Bienvenue.md");
    if fs::read_dir(notes_dir(&path)).map_err(|e| e.to_string())?.next().is_none() {
        fs::write(
            &welcome,
            "# Bienvenue\n\nCeci est ton coffre. Chaque note est un fichier .md dans le dossier notes/.\n\nCrée un lien vers une autre note avec des doubles crochets : [[Idées]].\nSi la note n'existe pas, un clic la crée.\n",
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn list_notes(vault: String) -> Result<Vec<NoteInfo>, String> {
    let mut notes = Vec::new();
    for entry in fs::read_dir(notes_dir(&vault)).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
            notes.push(NoteInfo { name: stem.to_string(), modified });
        }
    }
    notes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(notes)
}

pub fn read_note(vault: String, name: String) -> Result<String, String> {
    fs::read_to_string(note_path(&vault, &name)?).map_err(|e| e.to_string())
}

pub fn write_note(vault: String, name: String, content: String) -> Result<(), String> {
    fs::write(note_path(&vault, &name)?, content).map_err(|e| e.to_string())
}

/// Crée une note vide si elle n'existe pas encore (ne touche jamais une note existante).
pub fn create_note(vault: String, name: String) -> Result<(), String> {
    let p = note_path(&vault, &name)?;
    if !p.exists() {
        fs::write(&p, format!("# {}\n\n", name.trim())).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn delete_note(vault: String, name: String) -> Result<(), String> {
    fs::remove_file(note_path(&vault, &name)?).map_err(|e| e.to_string())
}

/// Toutes les notes qui contiennent un lien [[name]].
pub fn backlinks(vault: String, name: String) -> Result<Vec<Backlink>, String> {
    let target = name.trim().to_lowercase();
    let mut out = Vec::new();
    for note in list_notes(vault.clone())? {
        if note.name.to_lowercase() == target {
            continue;
        }
        let text = read_note(vault.clone(), note.name.clone()).unwrap_or_default();
        if let Some(line) = text.lines().find(|l| {
            WIKILINK
                .captures_iter(l)
                .any(|c| c[1].trim().to_lowercase() == target)
        }) {
            let excerpt: String = line.trim().chars().take(140).collect();
            out.push(Backlink { name: note.name, excerpt });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coffre_notes_et_backlinks() {
        let dir = std::env::temp_dir().join("aryadne-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let v = dir.to_string_lossy().to_string();
        open_vault(v.clone()).unwrap();
        assert!(dir.join("refs").is_dir() && dir.join(".app").is_dir());
        create_note(v.clone(), "Idées".into()).unwrap();
        write_note(v.clone(), "Bienvenue".into(), "Voir [[idées|mes idées]]".into()).unwrap();
        let bl = backlinks(v.clone(), "Idées".into()).unwrap();
        assert_eq!(bl.len(), 1);
        assert_eq!(bl[0].name, "Bienvenue");
        assert!(note_path(&v, "../evil").is_err());
        assert_eq!(list_notes(v).unwrap().len(), 2);
    }
}
