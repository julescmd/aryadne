//! Suivi d'un point dans une vidéo (comme le suivi 2D d'After Effects ou de Resolve) :
//! les images en niveaux de gris, un motif autour du point, cherché d'image en image.

use std::path::Path;

/// Les images d'un passage de la vidéo, en gris, `w` pixels de large, `fps` images par seconde
pub struct Frames {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Vec<u8>>,
}

/// Décode `len` secondes depuis `from` (secondes de la source)
pub fn decode(path: &Path, from: f32, len: f32, fps: f32, w: usize, src: (f32, f32)) -> Result<Frames, String> {
    let ff = crate::ffmpeg::find("ffmpeg").ok_or("ffmpeg n'est pas installé.")?;
    let h = ((src.1 / src.0.max(1.0)) * w as f32 / 2.0).round() as usize * 2;
    let o = crate::ffmpeg::command(ff)
        .args(["-v", "error", "-ss", &format!("{from:.3}"), "-t", &format!("{len:.3}"), "-i"])
        .arg(path)
        .args(["-vf", &format!("fps={fps},scale={w}:{h},format=gray"), "-f", "rawvideo", "-"])
        .output()
        .map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("ffmpeg").to_string());
    }
    let n = w * h;
    let px: Vec<Vec<u8>> = o.stdout.chunks_exact(n.max(1)).map(|c| c.to_vec()).collect();
    if px.is_empty() {
        return Err("aucune image décodée".into());
    }
    Ok(Frames { w, h, px })
}

/// Valeur bilinéaire
fn at(img: &[u8], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let (x, y) = (x.clamp(0.0, w as f32 - 1.001), y.clamp(0.0, h as f32 - 1.001));
    let (x0, y0) = (x as usize, y as usize);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let p = |xx: usize, yy: usize| img[yy * w + xx] as f32;
    let a = p(x0, y0) * (1.0 - fx) + p(x0 + 1, y0) * fx;
    let b = p(x0, y0 + 1) * (1.0 - fx) + p(x0 + 1, y0 + 1) * fx;
    a * (1.0 - fy) + b * fy
}

/// Suit le point `p0` (pixels de la première image) : sa position dans chaque image.
/// `r` : demi-taille du motif, `s` : demi-taille de la zone de recherche autour de la dernière position.
pub fn track(f: &Frames, p0: [f32; 2], r: i32, s: i32) -> Vec<[f32; 2]> {
    let patch = |img: &[u8], c: [f32; 2]| -> Vec<f32> {
        let mut v = Vec::with_capacity(((2 * r + 1) * (2 * r + 1)) as usize);
        for dy in -r..=r {
            for dx in -r..=r {
                v.push(at(img, f.w, f.h, c[0] + dx as f32, c[1] + dy as f32));
            }
        }
        // Sans la moyenne : un changement de lumière ne perd pas le point
        let m = v.iter().sum::<f32>() / v.len() as f32;
        v.iter_mut().for_each(|x| *x -= m);
        v
    };
    let orig = patch(&f.px[0], p0);
    let mut tmpl = orig.clone();
    let mut cur = p0;
    let mut out = vec![p0];
    let mut vel = [0.0f32, 0.0];
    for img in f.px.iter().skip(1) {
        // On cherche d'abord là où le mouvement mène
        let guess = [cur[0] + vel[0], cur[1] + vel[1]];
        let cost = |c: [f32; 2]| -> f32 {
            let p = patch(img, c);
            p.iter().zip(&tmpl).map(|(a, b)| (a - b) * (a - b)).sum()
        };
        let mut best = (f32::MAX, 0i32, 0i32);
        let mut costs = std::collections::HashMap::new();
        for dy in -s..=s {
            for dx in -s..=s {
                let c = cost([guess[0] + dx as f32, guess[1] + dy as f32]);
                costs.insert((dx, dy), c);
                if c < best.0 {
                    best = (c, dx, dy);
                }
            }
        }
        // Précision sous le pixel : une parabole sur chaque axe
        let sub = |m: Option<&f32>, z: f32, p: Option<&f32>| -> f32 {
            match (m, p) {
                (Some(a), Some(b)) => {
                    let d = a - 2.0 * z + b;
                    if d > 1e-6 {
                        ((a - b) / (2.0 * d)).clamp(-0.5, 0.5)
                    } else {
                        0.0
                    }
                }
                _ => 0.0,
            }
        };
        let (bx, by) = (best.1, best.2);
        let ox = sub(costs.get(&(bx - 1, by)), best.0, costs.get(&(bx + 1, by)));
        let oy = sub(costs.get(&(bx, by - 1)), best.0, costs.get(&(bx, by + 1)));
        let next = [guess[0] + bx as f32 + ox, guess[1] + by as f32 + oy];
        vel = [next[0] - cur[0], next[1] - cur[1]];
        cur = next;
        out.push(cur);
        // Le motif évolue un peu (rotation, lumière) mais reste ancré sur celui de départ : pas de dérive
        let now = patch(img, cur);
        for ((t, o), n) in tmpl.iter_mut().zip(&orig).zip(&now) {
            *t = 0.6 * o + 0.4 * n;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suit_un_carre() {
        // Un carré clair qui se déplace de (2, 1) px par image sur un fond texturé
        let (w, h) = (120usize, 80usize);
        let mut px = vec![];
        for k in 0..20 {
            let mut img = vec![0u8; w * h];
            for y in 0..h {
                for x in 0..w {
                    img[y * w + x] = ((x * 7 + y * 13) % 40) as u8;
                }
            }
            let (cx, cy) = (30 + 2 * k, 30 + k);
            for y in cy - 5..cy + 5 {
                for x in cx - 5..cx + 5 {
                    img[y * w + x] = 230;
                }
            }
            px.push(img);
        }
        let f = Frames { w, h, px };
        let pts = track(&f, [30.0, 30.0], 8, 6);
        assert_eq!(pts.len(), 20);
        let last = pts[19];
        assert!((last[0] - 68.0).abs() < 1.0 && (last[1] - 49.0).abs() < 1.0, "{last:?}");
    }
}
