//! Fichiers « sans titre » : un fichier neuf s'ouvre sous le nom `untitled.<ext>` et vit dans un dossier
//! caché du coffre (`.app/untitled/<n>/`) tant qu'on ne l'a pas enregistré avec un nom et un dossier.
//! Ce dossier sert aussi de sauvegarde en cas de plantage : ce qui y reste est proposé au lancement suivant.

use std::path::Path;

pub const DIR: &str = ".app/untitled";

pub fn is_untitled(rel: &str) -> bool {
    rel.starts_with(".app/untitled/")
}

/// Nom affiché : `untitled.arymat`, sans le dossier caché.
pub fn name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

/// Un emplacement libre pour un nouveau fichier sans titre de cette extension.
pub fn new_rel(root: &str, ext: &str) -> String {
    // Le nom ne compte que les fichiers sans titre du même genre : untitled.arymat, untitled-2.arymat…
    let taken: Vec<String> = leftovers(root).iter().map(|r| name(r).to_string()).collect();
    let mut k = 1;
    let file = loop {
        let f = if k == 1 { format!("untitled.{ext}") } else { format!("untitled-{k}.{ext}") };
        if !taken.contains(&f) {
            break f;
        }
        k += 1;
    };
    let mut n = 1;
    loop {
        let dir = format!("{DIR}/{n}");
        if !Path::new(root).join(&dir).exists() {
            return format!("{dir}/{file}");
        }
        n += 1;
    }
}

/// Empreinte du contenu, pour savoir si un fichier sans titre a été modifié depuis sa création.
pub fn hash(root: &str, rel: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::fs::read(Path::new(root).join(rel)).unwrap_or_default().hash(&mut h);
    h.finish()
}

/// Fichiers sans titre restés là (plantage, ou fermeture sans enregistrer).
pub fn leftovers(root: &str) -> Vec<String> {
    let mut out = vec![];
    let Ok(dirs) = std::fs::read_dir(Path::new(root).join(DIR)) else { return out };
    for d in dirs.flatten() {
        if let Ok(files) = std::fs::read_dir(d.path()) {
            for f in files.flatten() {
                if f.path().is_file() {
                    if let (Some(dn), Some(fname)) = (d.file_name().to_str(), f.file_name().to_str()) {
                        out.push(format!("{DIR}/{dn}/{fname}"));
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// Abandonne un fichier sans titre (et son dossier).
pub fn discard(root: &str, rel: &str) {
    if let Some(dir) = Path::new(root).join(rel).parent() {
        if dir.starts_with(Path::new(root).join(DIR)) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// Enregistre un fichier sans titre à l'endroit choisi, puis retire son dossier caché.
pub fn save_to(root: &str, rel: &str, dest: &Path) -> Result<(), String> {
    let src = Path::new(root).join(rel);
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    if std::fs::rename(&src, dest).is_err() {
        std::fs::copy(&src, dest).map_err(|e| e.to_string())?;
    }
    discard(root, rel);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sans_titre() {
        let root = std::env::temp_dir().join(format!("aryadne-untitled-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let r = root.to_string_lossy().to_string();
        let a = new_rel(&r, "arymat");
        assert!(is_untitled(&a) && name(&a) == "untitled.arymat");
        std::fs::create_dir_all(root.join(&a).parent().unwrap()).unwrap();
        std::fs::write(root.join(&a), "{}").unwrap();
        assert_eq!(name(&new_rel(&r, "aryscene")), "untitled.aryscene");
        assert_eq!(name(&new_rel(&r, "arymat")), "untitled-2.arymat");
        let h = hash(&r, &a);
        std::fs::write(root.join(&a), "{ }").unwrap();
        assert_ne!(h, hash(&r, &a));
        assert_eq!(leftovers(&r), vec![a.clone()]);
        let dest = root.join("nodes/Bois.arymat");
        save_to(&r, &a, &dest).unwrap();
        assert!(dest.exists() && leftovers(&r).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
