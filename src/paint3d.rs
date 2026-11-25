//! Peinture sur le modèle 3D (comme Substance Painter) : chaque objet d'une scène a une texture de peinture,
//! posée sur ses UV. Le pinceau dépose des touches dans la texture ; la vue la mélange à la couleur du
//! matériau. La texture s'enregistre en PNG dans « textures/ » du coffre.
use std::sync::{Arc, RwLock};

/// Texture de peinture partagée entre l'éditeur et le rendu
pub type Shared = Arc<RwLock<image::RgbaImage>>;

/// Côté d'une nouvelle texture de peinture
pub const SIZE: u32 = 2048;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Brush {
    /// Couleur (sRGB, 0..1)
    pub color: [f32; 3],
    /// Rayon à l'écran (pixels)
    pub radius: f32,
    /// Opacité d'une touche (0..1)
    pub strength: f32,
    /// Dureté du bord (0 : très doux, 1 : net)
    pub hardness: f32,
    /// Gomme : retire la peinture au lieu d'en poser
    pub erase: bool,
}

impl Default for Brush {
    fn default() -> Brush {
        Brush { color: [0.85, 0.35, 0.28], radius: 30.0, strength: 0.6, hardness: 0.5, erase: false }
    }
}

pub fn blank() -> image::RgbaImage {
    image::RgbaImage::new(SIZE, SIZE)
}

/// Une touche de pinceau en `uv` (0..1, v vers le haut), de rayon `r_uv` (en unités d'UV)
pub fn dab(img: &mut image::RgbaImage, uv: [f32; 2], r_uv: f32, b: &Brush) {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let (cx, cy) = (uv[0] * w, (1.0 - uv[1]) * h);
    let r = (r_uv * w.max(h)).max(0.75);
    let inner = r * b.hardness.clamp(0.0, 0.98);
    let (x0, x1) = (((cx - r).floor().max(0.0)) as u32, ((cx + r).ceil().min(w - 1.0)).max(0.0) as u32);
    let (y0, y1) = (((cy - r).floor().max(0.0)) as u32, ((cy + r).ceil().min(h - 1.0)).max(0.0) as u32);
    if cx + r < 0.0 || cy + r < 0.0 || cx - r > w || cy - r > h {
        return;
    }
    let col = b.color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round());
    for y in y0..=y1 {
        for x in x0..=x1 {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            if d > r {
                continue;
            }
            let f = if d <= inner { 1.0 } else { let t = (r - d) / (r - inner).max(1e-3); t * t * (3.0 - 2.0 * t) };
            let a = (b.strength.clamp(0.0, 1.0) * f).clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            put(img.get_pixel_mut(x, y), a, col, b.erase);
        }
    }
}

/// Dépose `a` (0..1) de la couleur `col` (0..255) sur un pixel, ou en retire avec la gomme
fn put(px: &mut image::Rgba<u8>, a: f32, col: [f32; 3], erase: bool) {
    let da = px[3] as f32 / 255.0;
    if erase {
        px[3] = ((da * (1.0 - a)) * 255.0).round() as u8;
        return;
    }
    // « Par-dessus » en alpha droit
    let oa = a + da * (1.0 - a);
    for k in 0..3 {
        let v = (col[k] * a + px[k] as f32 * da * (1.0 - a)) / oa.max(1e-6);
        px[k] = v.round().clamp(0.0, 255.0) as u8;
    }
    px[3] = (oa * 255.0).round().clamp(0.0, 255.0) as u8;
}

fn falloff(d: f32, r: f32, hardness: f32) -> f32 {
    if d > r {
        return 0.0;
    }
    let inner = r * hardness.clamp(0.0, 0.98);
    if d <= inner {
        1.0
    } else {
        let t = (r - d) / (r - inner).max(1e-6);
        t * t * (3.0 - 2.0 * t)
    }
}

/// Une touche en 3D, comme dans Substance : une boule de rayon `r` autour de `c` sur la surface. Chaque triangle
/// qu'elle touche est rempli dans ses UV, la force suivant la distance en 3D. Pas de débordement d'un îlot d'UV
/// à l'autre, pas de couture visible ; les faces tournées à l'opposé de `n` (l'autre côté d'une paroi) sont épargnées.
pub fn dab3d(img: &mut image::RgbaImage, pos: &[[f32; 3]], tris: &[[u32; 3]], uv: &[[[f32; 2]; 3]], c: [f32; 3], n: [f32; 3], r: f32, b: &Brush) {
    dab3d_with(img, pos, tris, uv, c, n, r, b, None);
}

/// Comme `dab3d`, avec un décalque : pour chaque point de la surface, sa couleur et son alpha (sRGB, 0..1),
/// ou rien hors de l'image. Le décalque remplace la couleur du pinceau et son bord doux.
#[allow(clippy::too_many_arguments)]
pub fn dab3d_with(img: &mut image::RgbaImage, pos: &[[f32; 3]], tris: &[[u32; 3]], uv: &[[[f32; 2]; 3]], c: [f32; 3], n: [f32; 3], r: f32, b: &Brush, stamp: Option<&dyn Fn([f32; 3]) -> Option<[f32; 4]>>) {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let col = b.color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round());
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let d2 = |a: [f32; 3]| a[0] * a[0] + a[1] * a[1] + a[2] * a[2];
    // Un pixel partagé par deux triangles voisins (leurs marges) ne reçoit la touche qu'une fois
    let mut got: std::collections::HashMap<(u32, u32), (f32, [f32; 3])> = Default::default();
    for (t, u) in tris.iter().zip(uv) {
        let p = t.map(|i| pos[i as usize]);
        // Le triangle est-il assez près de la boule ? (sa sphère englobante)
        let m = [(p[0][0] + p[1][0] + p[2][0]) / 3.0, (p[0][1] + p[1][1] + p[2][1]) / 3.0, (p[0][2] + p[1][2] + p[2][2]) / 3.0];
        let tr = p.iter().map(|q| d2(sub(*q, m))).fold(0.0f32, f32::max).sqrt();
        if d2(sub(m, c)).sqrt() > r + tr {
            continue;
        }
        let e1 = sub(p[1], p[0]);
        let e2 = sub(p[2], p[0]);
        let fnrm = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
        if fnrm[0] * n[0] + fnrm[1] * n[1] + fnrm[2] * n[2] < 0.0 {
            continue;
        }
        // Le triangle en pixels de la texture
        let q = u.map(|v| [v[0] * w, (1.0 - v[1]) * h]);
        let area = (q[1][0] - q[0][0]) * (q[2][1] - q[0][1]) - (q[2][0] - q[0][0]) * (q[1][1] - q[0][1]);
        if area.abs() < 1e-6 {
            continue;
        }
        // Un pixel et demi de marge autour du triangle, pour que les coutures ne laissent pas de fil vide
        let longest = (0..3).map(|k| ((q[(k + 1) % 3][0] - q[k][0]).powi(2) + (q[(k + 1) % 3][1] - q[k][1]).powi(2)).sqrt()).fold(0.0f32, f32::max);
        let eps = 1.5 / (area.abs() / longest.max(1e-6));
        let x0 = (q.iter().map(|v| v[0]).fold(f32::MAX, f32::min) - 2.0).floor().max(0.0) as u32;
        let x1 = (q.iter().map(|v| v[0]).fold(f32::MIN, f32::max) + 2.0).ceil().min(w - 1.0).max(0.0) as u32;
        let y0 = (q.iter().map(|v| v[1]).fold(f32::MAX, f32::min) - 2.0).floor().max(0.0) as u32;
        let y1 = (q.iter().map(|v| v[1]).fold(f32::MIN, f32::max) + 2.0).ceil().min(h - 1.0).max(0.0) as u32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w1 = ((px - q[0][0]) * (q[2][1] - q[0][1]) - (q[2][0] - q[0][0]) * (py - q[0][1])) / area;
                let w2 = ((q[1][0] - q[0][0]) * (py - q[0][1]) - (px - q[0][0]) * (q[1][1] - q[0][1])) / area;
                let w0 = 1.0 - w1 - w2;
                if w0 < -eps || w1 < -eps || w2 < -eps {
                    continue;
                }
                let (a0, a1, a2) = (w0.max(0.0), w1.max(0.0), w2.max(0.0));
                let s = (a0 + a1 + a2).max(1e-6);
                let pt = [0, 1, 2].map(|k| (p[0][k] * a0 + p[1][k] * a1 + p[2][k] * a2) / s);
                let (a, cc) = match stamp {
                    Some(st) => match st(pt) {
                        Some(sc) if d2(sub(pt, c)).sqrt() <= r * 1.5 => ((b.strength.clamp(0.0, 1.0) * sc[3]).clamp(0.0, 1.0), [sc[0] * 255.0, sc[1] * 255.0, sc[2] * 255.0]),
                        _ => (0.0, col),
                    },
                    None => ((b.strength.clamp(0.0, 1.0) * falloff(d2(sub(pt, c)).sqrt(), r, b.hardness)).clamp(0.0, 1.0), col),
                };
                if a > 0.0 {
                    let e = got.entry((x, y)).or_insert((0.0, cc));
                    if a > e.0 {
                        *e = (a, cc);
                    }
                }
            }
        }
    }
    for ((x, y), (a, cc)) in got {
        put(img.get_pixel_mut(x, y), a, cc, b.erase);
    }
}

/// La peinture en `uv` (sRGB et alpha, 0..1), au plus proche
pub fn sample(img: &image::RgbaImage, uv: [f32; 2]) -> [f32; 4] {
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return [0.0; 4];
    }
    let x = ((uv[0].rem_euclid(1.0)) * w as f32).min(w as f32 - 1.0) as u32;
    let y = (((1.0 - uv[1]).rem_euclid(1.0)) * h as f32).min(h as f32 - 1.0) as u32;
    let p = img.get_pixel(x, y);
    [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0, p[3] as f32 / 255.0]
}

/// Les points d'un trait entre deux positions de la souris, espacés d'une fraction du rayon
pub fn spaced(a: [f32; 2], b: [f32; 2], radius: f32) -> Vec<[f32; 2]> {
    let step = (radius * 0.25).max(1.0);
    let d = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    let n = (d / step).ceil().max(1.0) as usize;
    (1..=n).map(|k| {
        let t = k as f32 / n as f32;
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decalque_projete() {
        let pos = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
        let tris = [[0, 1, 2], [0, 2, 3]];
        let uv = [[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], [[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]]];
        let mut img = image::RgbaImage::new(32, 32);
        let b = Brush { strength: 1.0, ..Default::default() };
        // Moitié gauche bleue, moitié droite transparente
        let st = |p: [f32; 3]| (p[0] < 0.5).then_some([0.0, 0.0, 1.0, 1.0]);
        dab3d_with(&mut img, &pos, &tris, &uv, [0.5, 0.5, 0.0], [0.0, 0.0, 1.0], 1.0, &b, Some(&st));
        assert_eq!(img.get_pixel(4, 16).0, [0, 0, 255, 255]);
        assert_eq!(img.get_pixel(28, 16)[3], 0);
    }


    #[test]
    fn touche_en_3d_sans_deborder() {
        // Deux triangles d'un carré, dépliés dans deux îlots éloignés
        let pos = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
        let tris = [[0, 1, 2], [0, 2, 3]];
        let uv = [[[0.0, 0.0], [0.4, 0.0], [0.4, 0.4]], [[0.6, 0.6], [1.0, 1.0], [0.6, 1.0]]];
        let mut img = image::RgbaImage::new(64, 64);
        let b = Brush { strength: 1.0, hardness: 1.0, ..Default::default() };
        dab3d(&mut img, &pos, &tris, &uv, [0.5, 0.5, 0.0], [0.0, 0.0, 1.0], 0.3, &b);
        // Peint des deux côtés de la diagonale, rien entre les îlots
        assert!(img.get_pixel(19, 51)[3] > 0);
        assert!(img.get_pixel(32, 32)[3] == 0);
        assert!(img.pixels().filter(|p| p[3] > 0).count() > 40);
        // Une face tournée à l'opposé n'est pas touchée
        let mut img2 = image::RgbaImage::new(64, 64);
        dab3d(&mut img2, &pos, &tris, &uv, [0.5, 0.5, 0.0], [0.0, 0.0, -1.0], 0.3, &b);
        assert!(img2.pixels().all(|p| p[3] == 0));
    }

    #[test]
    fn touches_de_pinceau() {
        let mut img = image::RgbaImage::new(64, 64);
        let b = Brush { color: [1.0, 0.0, 0.0], radius: 10.0, strength: 1.0, hardness: 0.8, erase: false };
        dab(&mut img, [0.5, 0.5], 0.1, &b);
        let c = sample(&img, [0.5, 0.5]);
        assert!(c[0] > 0.99 && c[3] > 0.99);
        assert_eq!(sample(&img, [0.05, 0.05])[3], 0.0);
        // v vers le haut : (0.5, 0.9) est en haut de l'image
        dab(&mut img, [0.5, 0.9], 0.03, &b);
        assert!(img.get_pixel(32, 6)[3] > 200);
        // La gomme retire
        dab(&mut img, [0.5, 0.5], 0.1, &Brush { erase: true, ..b });
        assert!(sample(&img, [0.5, 0.5])[3] < 0.01);
        // Un trait : des touches serrées
        assert_eq!(spaced([0.0, 0.0], [10.0, 0.0], 8.0).len(), 5);
    }
}

/// Ce que le rendu reçoit pour un objet peint : la texture, les UV de chaque triangle de l'objet
/// et l'indice de son premier triangle dans la scène
#[derive(Clone)]
pub struct Layer {
    pub tex: Shared,
    pub uv: std::sync::Arc<Vec<[[f32; 2]; 3]>>,
    pub first: usize,
}

/// UV automatiques pour peindre un objet qui n'en a pas : coutures et dépliage du module UV,
/// déterministes (le même maillage donne les mêmes UV, donc la peinture retombe au même endroit)
pub fn auto_uv(pos: &[[f32; 3]], tris: &[[u32; 3]], edges: &[u8]) -> Vec<[[f32; 2]; 3]> {
    use crate::uvedit::{auto_unwrap, AutoOptions, EditData, PackOptions, Topo};
    // Sommets propres à l'objet, soudés comme le fait le module UV
    let mut weld: std::collections::HashMap<[u32; 3], u32> = Default::default();
    let mut lpos: Vec<[f32; 3]> = vec![];
    let ltris: Vec<[u32; 3]> = tris
        .iter()
        .map(|t| {
            t.map(|i| {
                let p = pos[i as usize];
                *weld.entry([p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]).or_insert_with(|| {
                    lpos.push(p);
                    (lpos.len() - 1) as u32
                })
            })
        })
        .collect();
    let topo = Topo::new(&lpos, &ltris, edges);
    let mut d = EditData::fresh(&topo);
    auto_unwrap(&topo, &mut d, &AutoOptions::default(), &PackOptions::default());
    // Topo soude encore (positions identiques) : ses sommets suivent l'ordre de lpos, donc les indices restent
    let mut by_vert: std::collections::HashMap<u32, Vec<usize>> = Default::default();
    for (f, p) in topo.polys.iter().enumerate() {
        for &v in p {
            by_vert.entry(v).or_default().push(f);
        }
    }
    ltris
        .iter()
        .map(|t| {
            let f = by_vert.get(&t[0]).and_then(|fs| fs.iter().copied().find(|&f| t[1..].iter().all(|v| topo.polys[f].contains(v))));
            match f.and_then(|f| d.uv.get(f).map(|u| (f, u))) {
                Some((f, u)) if u.len() == topo.polys[f].len() => t.map(|v| {
                    let c = topo.polys[f].iter().position(|&x| x == v).unwrap_or(0);
                    u[c]
                }),
                _ => [[0.0; 2]; 3],
            }
        })
        .collect()
}
