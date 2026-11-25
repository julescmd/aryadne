//! Texte 3D : le texte tracé avec Inter Bold, puis extrudé en volume (voxels), enregistré en .obj.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};

/// La couverture du texte (0..1) : largeur, hauteur, pixels ; `size` : hauteur d'une ligne en pixels
pub fn rasterize(text: &str, size: f32) -> (usize, usize, Vec<f32>) {
    let Ok(font) = FontRef::try_from_slice(include_bytes!("../assets/fonts/Inter-Bold.ttf")) else { return (0, 0, vec![]) };
    let scaled = font.as_scaled(PxScale::from(size));
    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    let margin = (size * 0.15).ceil() as usize;
    let line_h = scaled.height() + scaled.line_gap();
    let widths: Vec<f32> = lines.iter().map(|l| {
        let mut x = 0.0;
        let mut prev = None;
        for c in l.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = prev {
                x += scaled.kern(p, id);
            }
            x += scaled.h_advance(id);
            prev = Some(id);
        }
        x
    }).collect();
    let w = widths.iter().cloned().fold(0.0, f32::max).ceil() as usize + margin * 2;
    let h = (line_h * lines.len() as f32).ceil() as usize + margin * 2;
    if lines.is_empty() || w > 8000 || h > 8000 {
        return (0, 0, vec![]);
    }
    let mut cov = vec![0.0f32; w * h];
    for (li, l) in lines.iter().enumerate() {
        // Lignes centrées
        let mut x = margin as f32 + (widths.iter().cloned().fold(0.0, f32::max) - widths[li]) / 2.0;
        let y = margin as f32 + li as f32 * line_h + scaled.ascent();
        let mut prev = None;
        for c in l.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = prev {
                x += scaled.kern(p, id);
            }
            let g = id.with_scale_and_position(PxScale::from(size), ab_glyph::point(x, y));
            if let Some(og) = font.outline_glyph(g) {
                let b = og.px_bounds();
                og.draw(|gx, gy, v| {
                    let (px, py) = (b.min.x as i64 + gx as i64, b.min.y as i64 + gy as i64);
                    if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                        let c = &mut cov[py as usize * w + px as usize];
                        *c = (*c + v).min(1.0);
                    }
                });
            }
            x += scaled.h_advance(id);
            prev = Some(id);
        }
    }
    (w, h, cov)
}

/// Le texte en maillage : hauteur d'une ligne `height` (unités de la scène), épaisseur `depth`
pub fn mesh(text: &str, height: f32, depth: f32, bevel: u32) -> Option<crate::remesh::Out> {
    let px_line = 48.0;
    let (w, h, cov) = rasterize(text, px_line);
    let unit = height / px_line;
    let mut o = crate::remesh::extrude(&cov, w, h, unit, depth, bevel)?;
    // Centré sur l'origine, posé au sol
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &o.pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let c = [(lo[0] + hi[0]) / 2.0, lo[1], (lo[2] + hi[2]) / 2.0];
    for p in &mut o.pos {
        for k in 0..3 {
            p[k] -= c[k];
        }
    }
    Some(o)
}

/// Écrit un .obj simple (positions et triangles)
pub fn write_obj(o: &crate::remesh::Out, path: &std::path::Path) -> Result<(), String> {
    use std::fmt::Write;
    let mut s = String::from("# Texte 3D (Aryadne)\n");
    for p in &o.pos {
        let _ = writeln!(s, "v {:.5} {:.5} {:.5}", p[0], p[1], p[2]);
    }
    for t in &o.tris {
        let _ = writeln!(s, "f {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1);
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    crate::vault::atomic_write(path, s).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn texte_en_volume() {
        let (w, h, cov) = super::rasterize("Ab", 40.0);
        assert!(w > 40 && h > 40 && cov.iter().any(|v| *v > 0.9));
        let o = super::mesh("Ab", 1.0, 0.2, 1).unwrap();
        assert!(o.tris.len() > 200);
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &o.pos {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        // Épaisseur proche de 0,2, posé au sol, plus large que haut
        assert!((hi[2] - lo[2] - 0.2).abs() < 0.05, "{:?} {:?}", lo, hi);
        assert!(lo[1].abs() < 1e-4 && hi[0] - lo[0] > hi[1] - lo[1]);
    }
}
