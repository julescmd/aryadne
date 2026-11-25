//! Les fichiers d'Aryadne utilisables partout : une image à calques (.aryimg) devient une image
//! ordinaire pour le montage, le board, la 3D et les nodes. Le résultat est gardé en cache et
//! refait seulement quand le fichier change.
use std::path::{Path, PathBuf};

/// Fichier d'image à calques.
pub fn is_layered(path: &str) -> bool {
    let l = path.to_lowercase();
    l.ends_with(".aryimg") || l.ends_with(".aryvec")
}

fn cache_dir() -> PathBuf {
    std::env::temp_dir().join("aryadne-bridge")
}

/// Clé du cache : chemin, taille et date de modification.
fn key(path: &Path) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let m = std::fs::metadata(path).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    m.len().hash(&mut h);
    m.modified().ok().hash(&mut h);
    Some(h.finish())
}

/// L'image aplatie d'un .aryimg (PNG en cache) ; tout autre fichier est rendu tel quel.
pub fn media(path: &Path) -> PathBuf {
    if !is_layered(&path.to_string_lossy()) {
        return path.to_path_buf();
    }
    let Some(k) = key(path) else { return path.to_path_buf() };
    let out = cache_dir().join(format!("{k:016x}.png"));
    if out.is_file() {
        return out;
    }
    // Un dessin vectoriel : son premier plan de travail, rendu à 2048 px au plus
    if path.to_string_lossy().to_lowercase().ends_with(".aryvec") {
        let _ = std::fs::create_dir_all(cache_dir());
        let tmp = out.with_extension("tmp.png");
        return match crate::vector::thumbnail(path, 2048) {
            Ok(img) if img.save(&tmp).is_ok() && std::fs::rename(&tmp, &out).is_ok() => out,
            _ => path.to_path_buf(),
        };
    }
    match crate::paint::Doc::load(path) {
        Ok(doc) => {
            let _ = std::fs::create_dir_all(cache_dir());
            let tmp = out.with_extension("tmp.png");
            if doc.flatten().save(&tmp).is_ok() && std::fs::rename(&tmp, &out).is_ok() {
                out
            } else {
                path.to_path_buf()
            }
        }
        Err(_) => path.to_path_buf(),
    }
}

/// L'image aplatie, directement.
pub fn image(path: &Path) -> Result<image::DynamicImage, String> {
    let m = media(path);
    if m == path && is_layered(&path.to_string_lossy()) {
        return crate::paint::Doc::load(path).map(|d| image::DynamicImage::ImageRgba8(d.flatten()));
    }
    image::open(&m).map_err(|e| format!("Image illisible : {e}"))
}

/// Dimensions d'un .aryimg (lues dans l'en-tête, sans décoder les calques).
pub fn dims(path: &Path) -> Option<[u32; 2]> {
    if path.to_string_lossy().to_lowercase().ends_with(".aryvec") {
        let d = crate::vector::Doc::load(path).ok()?;
        let b = d.boards.first()?;
        return Some([b.w.round() as u32, b.h.round() as u32]);
    }
    let b = std::fs::read(path).ok()?;
    if !b.starts_with(b"ARYIMG1\n") {
        return image::load_from_memory(&b).ok().map(|i| [i.width(), i.height()]);
    }
    let n = u32::from_le_bytes(b.get(8..12)?.try_into().ok()?) as usize;
    let v: serde_json::Value = serde_json::from_slice(b.get(12..12 + n)?).ok()?;
    Some([v["w"].as_u64()? as u32, v["h"].as_u64()? as u32])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_a_calques_partout() {
        let dir = std::env::temp_dir().join("aryadne-bridge-test");
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("essai.aryimg");
        let mut doc = crate::paint::Doc::new(64, 32, false);
        doc.layers[0].px.chunks_mut(4).for_each(|p| p.copy_from_slice(&[200, 30, 30, 255]));
        doc.save(&f).unwrap();
        assert_eq!(dims(&f), Some([64, 32]));
        let m = media(&f);
        assert!(m.extension().is_some_and(|e| e == "png"));
        let img = image::open(&m).unwrap().to_rgba8();
        assert_eq!((img.width(), img.height()), (64, 32));
        assert_eq!(img.get_pixel(5, 5).0, [200, 30, 30, 255]);
        // Le même fichier sans changement : le même cache
        assert_eq!(media(&f), m);
    }
}
