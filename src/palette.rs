//! Générateur de palettes : depuis des images (couleurs dominantes regroupées), ou par harmonie
//! autour d'une couleur (analogue, complément, triade, monochrome, aléatoire), puis réglage de la
//! saturation et du contraste. Sert l'inspecteur « Couleurs » du board.
use eframe::egui::{ecolor::Hsva, Color32};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Harmony {
    #[default]
    Image,
    Analogous,
    Complement,
    Triad,
    Mono,
    Random,
}

pub const HARMONIES: [(Harmony, &str); 6] = [
    (Harmony::Image, "Image"),
    (Harmony::Analogous, "Analogue"),
    (Harmony::Complement, "Complément"),
    (Harmony::Triad, "Triade"),
    (Harmony::Mono, "Monochrome"),
    (Harmony::Random, "Aléatoire"),
];

/// Réglages de l'inspecteur
#[derive(Clone, Debug)]
pub struct Settings {
    pub count: usize,
    /// -1..1 : moins ou plus saturé
    pub saturation: f32,
    /// -1..1 : écarts de clarté resserrés ou élargis
    pub contrast: f32,
    pub harmony: Harmony,
    /// Montrer le texte lisible sur chaque couleur (contraste 4,5:1)
    pub readable: bool,
    /// Graine du mode aléatoire (change à chaque clic)
    pub seed: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { count: 5, saturation: 0.0, contrast: 0.0, harmony: Harmony::Image, readable: true, seed: 1 }
    }
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

fn rgb(c: Color32) -> [f32; 3] {
    [c.r() as f32, c.g() as f32, c.b() as f32]
}

/// Regroupe des couleurs en `n` couleurs (k-moyennes, départ par points les plus éloignés, déterministe),
/// triées de la plus sombre à la plus claire.
pub fn cluster(cols: &[Color32], n: usize) -> Vec<Color32> {
    let pts: Vec<[f32; 3]> = cols.iter().map(|&c| rgb(c)).collect();
    if pts.is_empty() || n == 0 {
        return vec![];
    }
    let n = n.min(pts.len());
    let mut centers = vec![pts[0]];
    while centers.len() < n {
        let far = pts.iter().copied().max_by(|a, b| {
            let da = centers.iter().map(|c| dist2(*a, *c)).fold(f32::MAX, f32::min);
            let db = centers.iter().map(|c| dist2(*b, *c)).fold(f32::MAX, f32::min);
            da.total_cmp(&db)
        });
        centers.push(far.unwrap_or(pts[0]));
    }
    for _ in 0..12 {
        let mut sum = vec![[0.0f32; 4]; n];
        for p in &pts {
            let k = (0..n).min_by(|&a, &b| dist2(*p, centers[a]).total_cmp(&dist2(*p, centers[b]))).unwrap_or(0);
            sum[k][0] += p[0];
            sum[k][1] += p[1];
            sum[k][2] += p[2];
            sum[k][3] += 1.0;
        }
        for (c, s) in centers.iter_mut().zip(&sum) {
            if s[3] > 0.0 {
                *c = [s[0] / s[3], s[1] / s[3], s[2] / s[3]];
            }
        }
    }
    let mut out: Vec<Color32> = centers.iter().map(|c| Color32::from_rgb(c[0].round() as u8, c[1].round() as u8, c[2].round() as u8)).collect();
    out.sort_by(|a, b| luminance(*a).total_cmp(&luminance(*b)));
    out.dedup();
    out
}

fn from_hsv(h: f32, s: f32, v: f32) -> Color32 {
    Color32::from(Hsva::new(h.rem_euclid(1.0), s.clamp(0.0, 1.0), v.clamp(0.0, 1.0), 1.0))
}

/// Petit générateur pseudo-aléatoire (xorshift), pour un mode aléatoire reproductible
fn rand(seed: &mut u32) -> f32 {
    let mut x = *seed | 1;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *seed = x;
    (x % 10_000) as f32 / 10_000.0
}

/// Palette par harmonie autour de `base` (la couleur la plus saturée des images, en général)
pub fn harmony(base: Color32, kind: Harmony, n: usize, seed: u32) -> Vec<Color32> {
    let b = Hsva::from(base);
    let (h, s, v) = (b.h, b.s.max(0.25), b.v.clamp(0.3, 0.9));
    let n = n.max(1);
    let t = |k: usize| if n == 1 { 0.5 } else { k as f32 / (n - 1) as f32 };
    let mut out: Vec<Color32> = match kind {
        Harmony::Image => vec![base],
        Harmony::Analogous => (0..n).map(|k| from_hsv(h + (t(k) - 0.5) * 0.18, s, 0.25 + 0.65 * t(k))).collect(),
        Harmony::Complement => (0..n).map(|k| {
            let other = k % 2 == 1;
            from_hsv(if other { h + 0.5 } else { h }, s * if other { 0.8 } else { 1.0 }, 0.25 + 0.65 * t(k))
        }).collect(),
        Harmony::Triad => (0..n).map(|k| from_hsv(h + (k % 3) as f32 / 3.0, s, 0.3 + 0.6 * t(k))).collect(),
        Harmony::Mono => (0..n).map(|k| from_hsv(h, s * (1.0 - 0.5 * t(k)), 0.15 + 0.8 * t(k))).collect(),
        Harmony::Random => {
            let mut sd = seed.max(1);
            (0..n).map(|k| from_hsv(h + (rand(&mut sd) - 0.5) * 0.6, 0.2 + 0.6 * rand(&mut sd), 0.2 + 0.7 * t(k))).collect()
        }
    };
    let _ = v;
    out.sort_by(|a, b| luminance(*a).total_cmp(&luminance(*b)));
    out
}

/// Applique la saturation et le contraste (-1..1) à toute la palette
pub fn adjust(cols: &[Color32], saturation: f32, contrast: f32) -> Vec<Color32> {
    if cols.is_empty() {
        return vec![];
    }
    let mean = cols.iter().map(|&c| Hsva::from(c).v).sum::<f32>() / cols.len() as f32;
    cols.iter()
        .map(|&c| {
            let x = Hsva::from(c);
            let s = if saturation >= 0.0 { x.s + (1.0 - x.s) * saturation } else { x.s * (1.0 + saturation) };
            let v = mean + (x.v - mean) * (1.0 + contrast);
            from_hsv(x.h, s, v)
        })
        .collect()
}

/// Palette finale pour l'inspecteur : couleurs des images regroupées, harmonie, réglages
pub fn generate(source: &[Color32], st: &Settings) -> Vec<Color32> {
    let base = cluster(source, st.count);
    let out = match st.harmony {
        Harmony::Image => base,
        h => {
            // La couleur la plus présente parmi les plus vives des images (pas un pixel isolé criard)
            let sv = |c: &Color32| {
                let x = Hsva::from(*c);
                x.s * x.v
            };
            let seed_col = base.iter().copied().max_by(|a, b| sv(a).total_cmp(&sv(b))).unwrap_or(Color32::from_rgb(0x9a, 0xa7, 0x7c));
            harmony(seed_col, h, st.count, st.seed)
        }
    };
    adjust(&out, st.saturation, st.contrast)
}

/// Luminance relative (WCAG)
pub fn luminance(c: Color32) -> f32 {
    let f = |x: u8| {
        let x = x as f32 / 255.0;
        if x <= 0.03928 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
}

pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Le texte le plus lisible sur cette couleur (noir ou blanc), et son contraste
pub fn text_on(c: Color32) -> (Color32, f32) {
    let (w, k) = (contrast(c, Color32::WHITE), contrast(c, Color32::from_rgb(0x10, 0x0a, 0x0d)));
    if w >= k { (Color32::WHITE, w) } else { (Color32::from_rgb(0x10, 0x0a, 0x0d), k) }
}

pub fn hex(c: Color32) -> String {
    format!("{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes() {
        let src = [Color32::from_rgb(10, 20, 10), Color32::from_rgb(12, 22, 12), Color32::from_rgb(200, 180, 140), Color32::from_rgb(90, 110, 70), Color32::from_rgb(205, 182, 138)];
        let c = cluster(&src, 3);
        assert_eq!(c.len(), 3);
        assert!(luminance(c[0]) <= luminance(c[2]));
        for h in [Harmony::Analogous, Harmony::Complement, Harmony::Triad, Harmony::Mono, Harmony::Random] {
            assert_eq!(harmony(src[3], h, 5, 7).len(), 5);
        }
        let st = Settings { contrast: 0.5, ..Default::default() };
        assert_eq!(generate(&src, &st).len(), 5.min(src.len()));
        assert!(contrast(Color32::BLACK, Color32::WHITE) > 20.0);
        assert!(text_on(Color32::WHITE).1 >= 4.5);
        assert!(adjust(&[], 0.0, 0.0).is_empty());
    }
}

/// Les `n` couleurs dominantes d'un ensemble de pixels (regroupées, de la plus présente à la moins présente)
pub fn dominant(px: &[Color32], n: usize) -> Vec<Color32> {
    cluster(px, n)
}
