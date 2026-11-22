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
                // Seul le fichier principal compte (pas ses fichiers voisins : maillages, graphes…)
                if f.path().is_file() && f.file_name().to_string_lossy().starts_with("untitled") {
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
    // Les fichiers posés à côté (maillage d'un objet modifié, graphe d'étalonnage…) suivent le fichier :
    // le fichier les cite par leur nom, renommé si la place est prise.
    let mut text = std::fs::read(&src).map_err(|e| e.to_string())?;
    if let (Some(from), Some(to)) = (src.parent(), dest.parent()) {
        if let Ok(rd) = std::fs::read_dir(from) {
            for f in rd.flatten() {
                let p = f.path();
                if p == src {
                    continue;
                }
                let name = f.file_name().to_string_lossy().to_string();
                let mut target = to.join(&name);
                if p.is_dir() {
                    let _ = copy_dir(&p, &target);
                    continue;
                }
                let (stem, ext) = match name.rsplit_once('.') {
                    Some((s, e)) => (s.to_string(), format!(".{e}")),
                    None => (name.clone(), String::new()),
                };
                let mut n = 2;
                while target.exists() {
                    target = to.join(format!("{stem} {n}{ext}"));
                    n += 1;
                }
                std::fs::copy(&p, &target).map_err(|e| e.to_string())?;
                let new_name = target.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
                if new_name != name {
                    let s = String::from_utf8_lossy(&text).replace(&format!("\"{name}\""), &format!("\"{new_name}\""));
                    text = s.into_bytes();
                }
            }
        }
    }
    std::fs::write(dest, &text).map_err(|e| e.to_string())?;
    discard(root, rel);
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for f in std::fs::read_dir(from)?.flatten() {
        let t = to.join(f.file_name());
        if f.path().is_dir() {
            copy_dir(&f.path(), &t)?;
        } else if !t.exists() {
            std::fs::copy(f.path(), t)?;
        }
    }
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
        // Une scène qui cite un maillage posé à côté : le maillage suit, renommé si un autre porte son nom
        let s = new_rel(&r, "aryscene");
        let dir = root.join(&s).parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(root.join(&s), r#"{"objects":[{"file":"cube.obj"}]}"#).unwrap();
        std::fs::write(dir.join("cube.obj"), "v 0 0 0").unwrap();
        assert_eq!(leftovers(&r), vec![s.clone()]);
        std::fs::create_dir_all(root.join("3d")).unwrap();
        std::fs::write(root.join("3d/cube.obj"), "autre").unwrap();
        let dest = root.join("3d/Scène.aryscene");
        save_to(&r, &s, &dest).unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), r#"{"objects":[{"file":"cube 2.obj"}]}"#);
        assert_eq!(std::fs::read_to_string(root.join("3d/cube 2.obj")).unwrap(), "v 0 0 0");
        let _ = std::fs::remove_dir_all(&root);
    }
}
