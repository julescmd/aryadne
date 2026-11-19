//! Miniature intégrée des fichiers Blender (.blend), sans lancer Blender.
//!
//! Blender enregistre un aperçu de la vue dans le bloc « TEST » du fichier
//! (option « Save Preview Images », activée par défaut). On gère l'ancien format
//! (BLENDER-v / BLENDER_v), le nouveau (Blender 5, « BLENDER17-01v… ») et la
//! compression zstd (« Compress » à l'enregistrement).
use image::{DynamicImage, RgbaImage};
use std::io::Read;
use std::path::Path;

/// Lit au plus 64 Mo : la miniature est toujours au tout début du fichier.
const MAX_READ: u64 = 64 * 1024 * 1024;

pub fn thumbnail(path: &Path) -> Result<DynamicImage, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic).map_err(|_| "Fichier .blend vide.".to_string())?;
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut data = vec![];
    if magic == [0x28, 0xB5, 0x2F, 0xFD] {
        let dec = ruzstd::decoding::StreamingDecoder::new(std::io::BufReader::new(f)).map_err(|e| format!(".blend compressé illisible : {e}"))?;
        dec.take(MAX_READ).read_to_end(&mut data).map_err(|e| e.to_string())?;
    } else {
        f.take(MAX_READ).read_to_end(&mut data).map_err(|e| e.to_string())?;
    }
    parse(&data)
}

pub fn parse(d: &[u8]) -> Result<DynamicImage, String> {
    if d.len() < 12 || &d[..7] != b"BLENDER" {
        return Err("Ce n'est pas un fichier Blender (ou il est compressé en gzip, ancien format).".into());
    }
    // Ancien en-tête : « BLENDER » + taille de pointeur (_ = 4, - = 8) + boutisme (v/V) + version.
    // Nouveau (Blender 5) : « BLENDER » + taille d'en-tête sur 2 chiffres + « -01 » + boutisme + version.
    let (mut pos, ptr8, little, large) = if d[7] == b'_' || d[7] == b'-' {
        (12usize, d[7] == b'-', d[8] == b'v', false)
    } else {
        let hsize: usize = std::str::from_utf8(&d[7..9]).ok().and_then(|s| s.parse().ok()).ok_or("En-tête .blend inconnu.")?;
        let little = d.get(12) == Some(&b'v');
        (hsize, true, little, true)
    };
    let u32_at = |o: usize| -> Option<u32> {
        let b: [u8; 4] = d.get(o..o + 4)?.try_into().ok()?;
        Some(if little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    };
    let i64_at = |o: usize| -> Option<i64> {
        let b: [u8; 8] = d.get(o..o + 8)?.try_into().ok()?;
        Some(if little { i64::from_le_bytes(b) } else { i64::from_be_bytes(b) })
    };
    loop {
        let code = d.get(pos..pos + 4).ok_or("Pas de miniature dans ce .blend (« Save Preview Images » désactivé ?).")?;
        // Taille du bloc et de son en-tête selon le format.
        let (len, head) = if large {
            // code(4) sdna(4) old(8) len(8) nr(8)
            (i64_at(pos + 16).ok_or("Bloc tronqué.")? as usize, 32)
        } else {
            (u32_at(pos + 4).ok_or("Bloc tronqué.")? as usize, if ptr8 { 24 } else { 20 })
        };
        let body = pos + head;
        if code == b"TEST" {
            let w = u32_at(body).ok_or("Miniature tronquée.")? as usize;
            let h = u32_at(body + 4).ok_or("Miniature tronquée.")? as usize;
            let px = d.get(body + 8..body + 8 + w * h * 4).ok_or("Miniature tronquée.")?;
            let mut img = RgbaImage::from_raw(w as u32, h as u32, px.to_vec()).ok_or("Miniature invalide.")?;
            image::imageops::flip_vertical_in_place(&mut img);
            return Ok(DynamicImage::ImageRgba8(img));
        }
        if code == b"ENDB" || code == b"DNA1" {
            return Err("Pas de miniature dans ce .blend (« Save Preview Images » désactivé ?).".into());
        }
        pos = body + len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(code: &[u8; 4], body: &[u8], large: bool) -> Vec<u8> {
        let mut b = code.to_vec();
        if large {
            b.extend(0u32.to_le_bytes()); // sdna
            b.extend(0u64.to_le_bytes()); // old
            b.extend((body.len() as i64).to_le_bytes());
            b.extend(1i64.to_le_bytes());
        } else {
            b.extend((body.len() as u32).to_le_bytes());
            b.extend(0u64.to_le_bytes());
            b.extend(0u32.to_le_bytes());
            b.extend(1u32.to_le_bytes());
        }
        b.extend(body);
        b
    }

    fn thumb_body() -> Vec<u8> {
        let mut t = vec![];
        t.extend(2u32.to_le_bytes());
        t.extend(2u32.to_le_bytes());
        // rangée du bas (rouge) puis du haut (bleu) : l'image doit être retournée
        t.extend([255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255]);
        t
    }

    #[test]
    fn ancien_et_nouveau_format() {
        for large in [false, true] {
            let mut d = if large { b"BLENDER17-01v0500".to_vec() } else { b"BLENDER-v405".to_vec() };
            d.extend(block(b"GLOB", &[1, 2, 3], large));
            d.extend(block(b"TEST", &thumb_body(), large));
            d.extend(block(b"ENDB", &[], large));
            let img = parse(&d).unwrap().to_rgba8();
            assert_eq!(img.dimensions(), (2, 2));
            assert_eq!(img.get_pixel(0, 0).0, [0, 0, 255, 255], "le haut de l'image doit être bleu");
        }
        let mut no = b"BLENDER-v405".to_vec();
        no.extend(block(b"ENDB", &[], false));
        assert!(parse(&no).is_err());
    }
}
