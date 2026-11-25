//! Scopes d'étalonnage, comme dans DaVinci Resolve : forme d'onde, parade RVB, vectorscope et histogramme.
//! Chacun se calcule à partir d'une image RVBA et rend une petite image à afficher par-dessus le moniteur.

/// Les scopes proposés, dans l'ordre du menu
pub const KINDS: [(&str, &str); 4] = [
    ("Forme d'onde", "La luminosité de chaque colonne de l'image, de bas (noir) en haut (blanc)"),
    ("Parade RVB", "La forme d'onde du rouge, du vert et du bleu, côte à côte"),
    ("Vectorscope", "Teinte (angle) et saturation (distance au centre) de tous les pixels"),
    ("Histogramme", "Combien de pixels à chaque niveau, pour R, V et B"),
];

/// Rend le scope `kind` (0 à 3) d'une image `w`×`h` en RVBA ; renvoie (largeur, hauteur, RVBA)
pub fn render(kind: usize, w: usize, h: usize, px: &[u8]) -> (usize, usize, Vec<u8>) {
    match kind {
        0 => waveform(w, h, px, false),
        1 => waveform(w, h, px, true),
        2 => vectorscope(w, h, px),
        _ => histogram(w, h, px),
    }
}

/// Un pixel sur `step` dans chaque sens : assez pour un scope, et rapide sur une grande image
fn step(w: usize, h: usize) -> usize {
    ((w * h) as f32 / 90_000.0).sqrt().ceil().max(1.0) as usize
}

fn luma(r: u8, g: u8, b: u8) -> f32 {
    0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32
}

/// Accumule des points, puis les éclaire : plus il y a de pixels à un endroit, plus c'est lumineux
fn glow(acc: &[[f32; 3]], scale: f32) -> Vec<u8> {
    let mut out = Vec::with_capacity(acc.len() * 4);
    for a in acc {
        let m = a[0].max(a[1]).max(a[2]);
        let k = |v: f32| ((1.0 - (-v * scale).exp()) * 255.0) as u8;
        out.extend([k(a[0]), k(a[1]), k(a[2]), if m > 0.0 { 255 } else { 0 }]);
    }
    out
}

fn waveform(w: usize, h: usize, px: &[u8], parade: bool) -> (usize, usize, Vec<u8>) {
    let (sw, sh) = (256usize, 128usize);
    let mut acc = vec![[0.0f32; 3]; sw * sh];
    let st = step(w, h);
    let third = sw / 3;
    for y in (0..h).step_by(st) {
        for x in (0..w).step_by(st) {
            let i = (y * w + x) * 4;
            let (r, g, b) = (px[i], px[i + 1], px[i + 2]);
            if parade {
                for (c, v) in [r, g, b].into_iter().enumerate() {
                    let col = c * third + x * (third - 2) / w.max(1);
                    let row = sh - 1 - (v as usize * (sh - 1) / 255);
                    acc[row * sw + col][c] += 1.0;
                }
            } else {
                let col = x * (sw - 1) / w.max(1);
                let row = sh - 1 - (luma(r, g, b) as usize * (sh - 1) / 255);
                let a = &mut acc[row * sw + col];
                for c in a.iter_mut() {
                    *c += 1.0;
                }
            }
        }
    }
    let per_col = (h / st).max(1) as f32;
    let mut out = glow(&acc, 18.0 / per_col);
    // Lignes de repère à 0, 25, 50, 75 et 100 %
    for k in 0..=4 {
        let row = (sh - 1) * k / 4;
        for x in 0..sw {
            let i = (row * sw + x) * 4;
            if out[i + 3] == 0 {
                out[i..i + 4].copy_from_slice(&[70, 70, 70, 255]);
            }
        }
    }
    (sw, sh, out)
}

fn vectorscope(w: usize, h: usize, px: &[u8]) -> (usize, usize, Vec<u8>) {
    let s = 160usize;
    let mut acc = vec![[0.0f32; 3]; s * s];
    let st = step(w, h);
    let c = s as f32 / 2.0;
    for y in (0..h).step_by(st) {
        for x in (0..w).step_by(st) {
            let i = (y * w + x) * 4;
            let (r, g, b) = (px[i] as f32 / 255.0, px[i + 1] as f32 / 255.0, px[i + 2] as f32 / 255.0);
            // Cb et Cr (Rec. 709), entre -0,5 et 0,5
            let cb = -0.1146 * r - 0.3854 * g + 0.5 * b;
            let cr = 0.5 * r - 0.4542 * g - 0.0458 * b;
            let (sx, sy) = ((c + cb * (s as f32 - 4.0)) as i64, (c - cr * (s as f32 - 4.0)) as i64);
            // Un petit point de 2 × 2 pour que les couleurs franches restent visibles
            for (ox, oy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (x, y) = (sx + ox, sy + oy);
                if x >= 0 && y >= 0 && (x as usize) < s && (y as usize) < s {
                    let a = &mut acc[y as usize * s + x as usize];
                    a[0] += 0.4 + r;
                    a[1] += 0.4 + g;
                    a[2] += 0.4 + b;
                }
            }
        }
    }
    let n = ((w / st) * (h / st)).max(1) as f32;
    let mut out = glow(&acc, 3000.0 / n);
    // Cercle extérieur, croix, et la ligne des tons chair (environ 123°)
    let mut dot = |x: f32, y: f32| {
        let (x, y) = (x as usize, y as usize);
        if x < s && y < s {
            let i = (y * s + x) * 4;
            if out[i + 3] == 0 {
                out[i..i + 4].copy_from_slice(&[70, 70, 70, 255]);
            }
        }
    };
    for k in 0..360 {
        let a = (k as f32).to_radians();
        dot(c + a.cos() * (c - 2.0), c + a.sin() * (c - 2.0));
    }
    for k in 0..s {
        dot(k as f32, c);
        dot(c, k as f32);
    }
    let skin = 123f32.to_radians();
    for k in 0..(c as usize) {
        dot(c + skin.cos() * k as f32, c - skin.sin() * k as f32);
    }
    (s, s, out)
}

fn histogram(w: usize, h: usize, px: &[u8]) -> (usize, usize, Vec<u8>) {
    let (sw, sh) = (256usize, 100usize);
    let mut bins = [[0u32; 256]; 3];
    let st = step(w, h);
    for y in (0..h).step_by(st) {
        for x in (0..w).step_by(st) {
            let i = (y * w + x) * 4;
            for c in 0..3 {
                bins[c][px[i + c] as usize] += 1;
            }
        }
    }
    let max = bins.iter().flat_map(|b| b.iter()).copied().max().unwrap_or(1).max(1) as f32;
    let mut out = vec![0u8; sw * sh * 4];
    for x in 0..sw {
        for c in 0..3 {
            let top = ((bins[c][x] as f32 / max).sqrt() * (sh - 1) as f32) as usize;
            for k in 0..top {
                let i = ((sh - 1 - k) * sw + x) * 4;
                out[i + c] = 220;
                out[i + 3] = 255;
            }
        }
    }
    (sw, sh, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: usize, h: usize, c: [u8; 4]) -> Vec<u8> {
        c.repeat(w * h)
    }

    #[test]
    fn scopes_lisent_l_image() {
        // Blanc : la forme d'onde éclaire la ligne du haut ; noir : celle du bas
        let (sw, sh, wf) = render(0, 40, 20, &flat(40, 20, [255, 255, 255, 255]));
        let lit = |row: usize| (0..sw).filter(|x| wf[(row * sw + x) * 4] > 100).count();
        assert!(lit(0) > sw / 2 && lit(sh / 2) == 0);
        // Rouge pur : dans la parade, le tiers rouge en haut, le vert et le bleu en bas
        let (sw, _, pr) = render(1, 30, 30, &flat(30, 30, [255, 0, 0, 255]));
        assert!(pr[(sw / 6) * 4] > 100, "rouge en haut");
        // Gris : le vectorscope ne montre qu'un point au centre
        let (s, _, vs) = render(2, 20, 20, &flat(20, 20, [128, 128, 128, 255]));
        let c = s / 2;
        assert!(vs[(c * s + c) * 4] > 100);
        // Histogramme d'un gris à 128 : une seule colonne pleine
        let (sw, sh, hi) = render(3, 10, 10, &flat(10, 10, [128, 128, 128, 255]));
        assert_eq!(hi[((sh - 1) * sw + 128) * 4 + 3], 255);
        assert_eq!(hi[((sh - 1) * sw + 10) * 4 + 3], 0);
    }
}
