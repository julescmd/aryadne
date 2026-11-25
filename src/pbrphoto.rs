//! Photo vers matériau PBR (comme un Substance Sampler simple) : à partir d'une photo de surface,
//! une couleur sans éclairage (albedo), une hauteur, une normale et une rugosité, qui se répètent sans raccord.

use rayon::prelude::*;

pub struct Maps {
    pub w: usize,
    pub h: usize,
    pub albedo: Vec<[f32; 3]>,
    pub height: Vec<f32>,
    pub normal: Vec<[f32; 3]>,
    pub rough: Vec<f32>,
}

fn lum(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Flou en boîte (trois passes ≈ gaussien), qui boucle sur les bords (la texture se répète)
fn blur(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    blur_ex(src, w, h, r, true)
}

/// `wrap` : boucle sur les bords ; sinon le bord se répète (pour l'éclairage, qui ne boucle pas)
fn blur_ex(src: &[f32], w: usize, h: usize, r: usize, wrap: bool) -> Vec<f32> {
    if r == 0 {
        return src.to_vec();
    }
    if !wrap {
        // Bords répétés : on agrandit l'image d'une marge, on floute en boucle, on recoupe
        let m = r * 3;
        let (bw, bh) = (w + 2 * m, h + 2 * m);
        let big: Vec<f32> = (0..bw * bh).map(|i| {
            let (x, y) = ((i % bw) as isize - m as isize, (i / bw) as isize - m as isize);
            src[(y.clamp(0, h as isize - 1) as usize) * w + x.clamp(0, w as isize - 1) as usize]
        }).collect();
        let b = blur_ex(&big, bw, bh, r, true);
        return (0..w * h).map(|i| b[(i / w + m) * bw + i % w + m]).collect();
    }
    let mut cur = src.to_vec();
    let n = (2 * r + 1) as f32;
    for _ in 0..3 {
        let rows: Vec<f32> = (0..h).into_par_iter().flat_map_iter(|y| {
            let row = &cur[y * w..(y + 1) * w];
            let mut acc: f32 = (0..=2 * r).map(|k| row[(k + w * 4 - r) % w]).sum();
            let mut out = vec![0.0; w];
            for x in 0..w {
                out[x] = acc / n;
                acc += row[(x + r + 1) % w] - row[(x + w * 4 - r) % w];
            }
            out
        }).collect();
        let mut cols = vec![0.0f32; w * h];
        for x in 0..w {
            let mut acc: f32 = (0..=2 * r).map(|k| rows[((k + h * 4 - r) % h) * w + x]).sum();
            for y in 0..h {
                cols[y * w + x] = acc / n;
                acc += rows[((y + r + 1) % h) * w + x] - rows[((y + h * 4 - r) % h) * w + x];
            }
        }
        cur = cols;
    }
    cur
}

/// Les cartes d'une photo (RGB 0..1) ; `strength` : relief des normales
pub fn from_photo(px: &[[f32; 3]], w: usize, h: usize, strength: f32) -> Maps {
    from_photo_ex(px, w, h, strength, false)
}

/// `invert` : le clair est creux (joints clairs, mortier, rayures blanches)
pub fn from_photo_ex(px: &[[f32; 3]], w: usize, h: usize, strength: f32, invert: bool) -> Maps {
    let l: Vec<f32> = px.iter().map(|c| lum(*c)).collect();
    // 1. Déluminer : l'éclairage, c'est la luminosité très floue ; on la retire en gardant la moyenne
    let big = (w.max(h) / 8).max(2);
    let light = blur_ex(&l, w, h, big, false);
    let mean = l.iter().sum::<f32>() / l.len().max(1) as f32;
    let albedo: Vec<[f32; 3]> = px.iter().zip(&light).map(|(c, li)| {
        let k = mean / li.max(0.02);
        c.map(|v| (v * k).clamp(0.0, 1.0))
    }).collect();
    // 2. Hauteur : le détail de la luminosité déluminée (passe-haut), remis entre 0 et 1
    let la: Vec<f32> = albedo.iter().map(|c| lum(*c)).collect();
    let mid = blur(&la, w, h, (w.max(h) / 32).max(2));
    let hp: Vec<f32> = la.iter().zip(&mid).map(|(a, b)| a - b).collect();
    let (lo, hi) = hp.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    let height: Vec<f32> = blur(&hp, w, h, 1).iter().map(|v| {
        let t = ((v - lo) / (hi - lo).max(1e-5)).clamp(0.0, 1.0);
        if invert { 1.0 - t } else { t }
    }).collect();
    // 3. Normales depuis la pente de la hauteur (Sobel, en boucle)
    let at = |x: isize, y: isize| height[(y.rem_euclid(h as isize) as usize) * w + x.rem_euclid(w as isize) as usize];
    let normal: Vec<[f32; 3]> = (0..w * h).into_par_iter().map(|i| {
        let (x, y) = ((i % w) as isize, (i / w) as isize);
        let dx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1)) - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
        let dy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1)) - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
        let n = [-dx * strength, dy * strength, 1.0];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        n.map(|v| v / len)
    }).collect();
    // 4. Rugosité : les creux et le grain sont plus mats, les zones lisses plus brillantes
    let detail = blur(&hp.iter().map(|v| v.abs()).collect::<Vec<_>>(), w, h, 2);
    let dmax = detail.iter().cloned().fold(1e-5, f32::max);
    let rough: Vec<f32> = detail.iter().zip(&height).map(|(d, hv)| (0.45 + 0.4 * (d / dmax) + 0.15 * (1.0 - hv)).clamp(0.0, 1.0)).collect();
    Maps { w, h, albedo, height, normal, rough }
}

/// Écrit les quatre cartes à côté de la photo : nom_albedo.png, nom_normal.png (OpenGL), nom_hauteur.png, nom_rugosite.png
pub fn write(photo: &std::path::Path, strength: f32, invert: bool) -> Result<Vec<std::path::PathBuf>, String> {
    let img = image::open(photo).map_err(|e| e.to_string())?.to_rgb8();
    // Au plus 2048 px : assez pour une texture, rapide à calculer
    let img = if img.width().max(img.height()) > 2048 { image::DynamicImage::ImageRgb8(img).thumbnail(2048, 2048).to_rgb8() } else { img };
    let (w, h) = (img.width() as usize, img.height() as usize);
    let px: Vec<[f32; 3]> = img.pixels().map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]).collect();
    let m = from_photo_ex(&px, w, h, strength, invert);
    let stem = photo.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "photo".into());
    let dir = photo.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let save_rgb = |name: &str, f: &dyn Fn(usize) -> [u8; 3]| -> Result<std::path::PathBuf, String> {
        let raw: Vec<u8> = (0..w * h).flat_map(f).collect();
        let p = dir.join(format!("{stem}_{name}.png"));
        image::RgbImage::from_raw(w as u32, h as u32, raw).ok_or("image")?.save(&p).map_err(|e| e.to_string())?;
        Ok(p)
    };
    Ok(vec![
        save_rgb("albedo", &|i| m.albedo[i].map(q))?,
        save_rgb("normal", &|i| m.normal[i].map(|v| q(v * 0.5 + 0.5)))?,
        save_rgb("hauteur", &|i| [q(m.height[i]); 3])?,
        save_rgb("rugosite", &|i| [q(m.rough[i]); 3])?,
    ])
}

#[cfg(test)]
mod tests {
    #[test]
    fn photo_en_materiau() {
        // Une photo de briques éclairée par la gauche : l'albedo doit être plus égal que la photo
        let (w, h) = (128usize, 64usize);
        let px: Vec<[f32; 3]> = (0..w * h).map(|i| {
            let (x, y) = (i % w, i / w);
            let mortar = x % 32 < 3 || y % 16 < 3;
            let base = if mortar { [0.7, 0.7, 0.68] } else { [0.6, 0.3, 0.2] };
            let light = 0.4 + 0.6 * x as f32 / w as f32;
            base.map(|v| v * light)
        }).collect();
        let m = super::from_photo(&px, w, h, 4.0);
        let brick_l = |img: &Vec<[f32; 3]>, x: usize| super::lum(img[20 * w + x]);
        let photo_ratio = brick_l(&px, 120) / brick_l(&px, 10);
        let albedo_ratio = brick_l(&m.albedo, 120) / brick_l(&m.albedo, 10);
        assert!(albedo_ratio < photo_ratio * 0.75, "{photo_ratio} {albedo_ratio}");
        // Le joint (creux) est plus bas que la brique, et les normales penchent aux bords des briques
        assert!((m.height[1 * w + 10] - m.height[8 * w + 10]).abs() > 0.2);
        assert!(m.normal.iter().any(|n| n[0].abs() > 0.2));
        assert!(m.rough.iter().all(|r| (0.0..=1.0).contains(r)));
    }
}

