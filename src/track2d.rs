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

/// Suit une zone (centre `c`, demi-largeur `hw`, demi-hauteur `hh`, en pixels de la première image) : pour
/// chaque image, son centre et son échelle (1 au départ). Plusieurs motifs de la zone sont suivis, les mieux
/// texturés ; leur déplacement médian donne le centre, leur écartement l'échelle : un motif perdu ne fausse rien.
pub fn track_region(f: &Frames, c: [f32; 2], hw: f32, hh: f32) -> Vec<([f32; 2], f32)> {
    let Some(first) = f.px.first() else { return vec![] };
    let r = ((hw.min(hh) * 0.35) as i32).clamp(4, 10);
    // Neuf candidats sur une grille dans la zone ; on garde les cinq qui ont le plus de relief
    let mut cands: Vec<([f32; 2], f32)> = vec![];
    for gy in -1..=1 {
        for gx in -1..=1 {
            let q = [c[0] + gx as f32 * hw * 0.55, c[1] + gy as f32 * hh * 0.55];
            if q[0] < r as f32 || q[1] < r as f32 || q[0] > f.w as f32 - r as f32 - 1.0 || q[1] > f.h as f32 - r as f32 - 1.0 {
                continue;
            }
            let mut v = vec![];
            for dy in -r..=r {
                for dx in -r..=r {
                    v.push(at(first, f.w, f.h, q[0] + dx as f32, q[1] + dy as f32));
                }
            }
            let m = v.iter().sum::<f32>() / v.len() as f32;
            cands.push((q, v.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / v.len() as f32));
        }
    }
    cands.sort_by(|a, b| b.1.total_cmp(&a.1));
    cands.truncate(5);
    if cands.is_empty() {
        cands.push((c, 0.0));
    }
    let tracks: Vec<Vec<[f32; 2]>> = {
        use rayon::prelude::*;
        cands.par_iter().map(|(q, _)| track(f, *q, r, 12)).collect()
    };
    let median = |v: &mut Vec<f32>| -> f32 {
        v.sort_by(|a, b| a.total_cmp(b));
        if v.is_empty() { 0.0 } else { v[v.len() / 2] }
    };
    (0..f.px.len())
        .map(|k| {
            // Échelle : le rapport des distances entre motifs, maintenant et au départ
            let mut ratios = vec![];
            for a in 0..tracks.len() {
                for b in a + 1..tracks.len() {
                    let d0 = (tracks[a][0][0] - tracks[b][0][0]).hypot(tracks[a][0][1] - tracks[b][0][1]);
                    let d1 = (tracks[a][k][0] - tracks[b][k][0]).hypot(tracks[a][k][1] - tracks[b][k][1]);
                    if d0 > 4.0 {
                        ratios.push(d1 / d0);
                    }
                }
            }
            let s = if ratios.is_empty() { 1.0 } else { median(&mut ratios).clamp(0.25, 4.0) };
            // Centre : chaque motif dit où il le voit (son décalage au centre grandit avec l'échelle)
            let mut xs: Vec<f32> = tracks.iter().map(|t| t[k][0] - (t[0][0] - c[0]) * s).collect();
            let mut ys: Vec<f32> = tracks.iter().map(|t| t[k][1] - (t[0][1] - c[1]) * s).collect();
            ([median(&mut xs), median(&mut ys)], s)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suit_une_zone_qui_grandit() {
        // Un motif texturé qui glisse de (1,5 ; 1) px par image et grandit de 1 % par image, sur un fond uni
        let (w, h) = (200usize, 150usize);
        let tex = |u: f32, v: f32| -> f32 {
            // Taches lisses, sans répétition proche
            128.0 + 60.0 * (u * 0.31).sin() * (v * 0.27).cos() + 50.0 * ((u * 0.11 + v * 0.17).sin()) + 30.0 * ((u * 0.07 - v * 0.19).cos())
        };
        let mut px = vec![];
        for k in 0..16 {
            let (cx, cy, s) = (70.0 + 1.5 * k as f32, 60.0 + k as f32, 1.0 + 0.01 * k as f32);
            let mut img = vec![40u8; w * h];
            for y in 0..h {
                for x in 0..w {
                    let (u, v) = ((x as f32 - cx) / s, (y as f32 - cy) / s);
                    if u.abs() < 34.0 && v.abs() < 28.0 {
                        img[y * w + x] = tex(u, v).clamp(0.0, 255.0) as u8;
                    }
                }
            }
            px.push(img);
        }
        let f = Frames { w, h, px };
        let out = track_region(&f, [70.0, 60.0], 26.0, 22.0);
        assert_eq!(out.len(), 16);
        assert_eq!(out[0], ([70.0, 60.0], 1.0));
        let (c, s) = out[15];
        assert!((c[0] - 92.5).abs() < 1.5 && (c[1] - 75.0).abs() < 1.5, "{c:?}");
        assert!((s - 1.15).abs() < 0.04, "{s}");
        assert!(track_region(&Frames { w, h, px: vec![] }, [10.0, 10.0], 5.0, 5.0).is_empty());
    }

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
