//! Sculpture : pinceaux qui déforment un maillage du mode édition (dessiner, gonfler, lisser,
//! saisir, aplatir, pincer), avec symétrie sur X.

use crate::meshedit::EMesh;

type V3 = [f32; 3];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Brush {
    Draw,
    Clay,
    Inflate,
    Smooth,
    Grab,
    Flatten,
    Pinch,
}

/// Pinceaux : (pinceau, icône, nom, touche)
pub const BRUSHES: [(Brush, &str, &str, &str); 7] = [
    (Brush::Draw, crate::lucide::BRUSH, "dessiner", "X"),
    (Brush::Clay, crate::lucide::LAYERS, "argile", "C"),
    (Brush::Inflate, crate::lucide::CIRCLE_DOT, "gonfler", "I"),
    (Brush::Smooth, crate::lucide::WAVES, "lisser (Maj)", "S"),
    (Brush::Grab, crate::lucide::HAND, "saisir", "G"),
    (Brush::Flatten, crate::lucide::MINUS, "aplatir", "T"),
    (Brush::Pinch, crate::lucide::SHRINK, "pincer", "P"),
];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt().max(1e-12);
    [a[0] / l, a[1] / l, a[2] / l]
}

/// Normales par sommet (moyenne des faces voisines, pondérée par leur aire).
pub fn normals(m: &EMesh) -> Vec<V3> {
    let mut n = vec![[0.0f32; 3]; m.pos.len()];
    for p in &m.polys {
        if p.len() < 3 {
            continue;
        }
        // Newell : robuste pour les polygones
        let mut f = [0.0f32; 3];
        for i in 0..p.len() {
            let (a, b) = (m.pos[p[i] as usize], m.pos[p[(i + 1) % p.len()] as usize]);
            f[0] += (a[1] - b[1]) * (a[2] + b[2]);
            f[1] += (a[2] - b[2]) * (a[0] + b[0]);
            f[2] += (a[0] - b[0]) * (a[1] + b[1]);
        }
        for &v in p {
            let v = v as usize;
            for k in 0..3 {
                n[v][k] += f[k];
            }
        }
    }
    n.into_iter().map(norm).collect()
}

/// Atténuation douce au bord du pinceau : 1 au centre, 0 au bord (`t` = distance / rayon).
pub fn falloff(t: f32) -> f32 {
    let u = (1.0 - t * t).clamp(0.0, 1.0);
    u * u
}

/// Voisinage du maillage, calculé une fois par geste : voisins de chaque sommet (par les arêtes) et
/// faces qui le touchent. Les pinceaux ne recalculent ainsi que ce qui est sous eux.
pub struct Topo {
    pub nb: Vec<Vec<u32>>,
    pub vp: Vec<Vec<u32>>,
}

impl Topo {
    pub fn new(m: &EMesh) -> Topo {
        let mut vp: Vec<Vec<u32>> = vec![vec![]; m.pos.len()];
        for (f, p) in m.polys.iter().enumerate() {
            for &v in p {
                if let Some(l) = vp.get_mut(v as usize) {
                    l.push(f as u32);
                }
            }
        }
        Topo { nb: m.neighbors(), vp }
    }

    /// Normale d'un sommet d'après ses seules faces voisines (même résultat que `normals`)
    pub fn normal(&self, m: &EMesh, i: usize) -> V3 {
        let mut n = [0.0f32; 3];
        for &f in self.vp.get(i).map(|l| l.as_slice()).unwrap_or(&[]) {
            let Some(p) = m.polys.get(f as usize).filter(|p| p.len() >= 3) else { continue };
            for k in 0..p.len() {
                let (a, b) = (m.pos[p[k] as usize], m.pos[p[(k + 1) % p.len()] as usize]);
                n[0] += (a[1] - b[1]) * (a[2] + b[2]);
                n[1] += (a[2] - b[2]) * (a[0] + b[0]);
                n[2] += (a[0] - b[0]) * (a[1] + b[1]);
            }
        }
        norm(n)
    }
}

/// Un coup de pinceau (hors « saisir ») sur les sommets touchés, chacun avec son atténuation.
/// `step` : déplacement de base dans le repère du maillage ; `invert` : creuser au lieu d'ajouter.
/// Ne lit et n'écrit que la zone touchée : le coût suit la taille du pinceau, pas celle du maillage.
pub fn dab(m: &mut EMesh, topo: &Topo, brush: Brush, hits: &[(usize, f32)], step: f32, invert: bool) {
    let hits: Vec<(usize, f32)> = hits.iter().copied().filter(|&(i, w)| i < m.pos.len() && w.is_finite()).collect();
    if hits.is_empty() {
        return;
    }
    let ns: Vec<V3> = hits.iter().map(|&(i, _)| topo.normal(m, i)).collect();
    let sign = if invert { -1.0 } else { 1.0 };
    // Normale et centre moyens de la zone
    let mut an = [0.0f32; 3];
    let mut ac = [0.0f32; 3];
    let mut wsum = 0.0;
    for (h, &(i, w)) in hits.iter().enumerate() {
        for k in 0..3 {
            an[k] += ns[h][k] * w;
            ac[k] += m.pos[i][k] * w;
        }
        wsum += w;
    }
    let an = norm(an);
    let ac = ac.map(|x| x / wsum.max(1e-12));
    let old = &m.pos;
    let avg = |i: usize| -> V3 {
        let list = topo.nb.get(i).map(|l| l.as_slice()).unwrap_or(&[]);
        let mut s = [0.0f32; 3];
        let mut n = 0.0f32;
        for &j in list {
            if let Some(q) = old.get(j as usize) {
                for k in 0..3 {
                    s[k] += q[k];
                }
                n += 1.0;
            }
        }
        if n == 0.0 { old[i] } else { s.map(|x| x / n) }
    };
    // Plan de la zone, au-dessus des points les plus hauts pour l'argile
    let top = hits.iter().map(|&(i, _)| dot(sub(old[i], ac), an)).fold(f32::MIN, f32::max);
    // Tous les déplacements d'abord (lus sur le maillage intact), appliqués ensuite
    let moved: Vec<V3> = hits
        .iter()
        .enumerate()
        .map(|(h, &(i, w))| {
            let p = old[i];
            let d: V3 = match brush {
                Brush::Draw => an.map(|x| x * step * w * sign),
                Brush::Inflate => ns[h].map(|x| x * step * w * sign),
                Brush::Smooth => {
                    let a = avg(i);
                    sub(a, p).map(|x| x * (w * 0.5).min(1.0))
                }
                Brush::Clay => {
                    // Monte vers un plan un peu au-dessus de la zone, sans dépasser : la matière s'empile à plat
                    let plane = top.min(step * 4.0) * 0.5 + step;
                    let h = dot(sub(p, ac), an);
                    let push = (plane * sign - h * sign).max(0.0) * sign;
                    let a = avg(i);
                    let s = sub(a, p).map(|x| x * 0.25 * w);
                    [an[0] * push * w * 0.5 + s[0], an[1] * push * w * 0.5 + s[1], an[2] * push * w * 0.5 + s[2]]
                }
                Brush::Flatten => {
                    let h = dot(sub(p, ac), an);
                    an.map(|x| -x * h * (w * 0.3).min(1.0))
                }
                Brush::Pinch => {
                    let to = sub(ac, p);
                    let along = dot(to, an);
                    let flat = [to[0] - an[0] * along, to[1] - an[1] * along, to[2] - an[2] * along];
                    flat.map(|x| x * 0.15 * w * sign)
                }
                Brush::Grab => [0.0; 3],
            };
            [p[0] + d[0], p[1] + d[1], p[2] + d[2]]
        })
        .collect();
    for (&(i, _), q) in hits.iter().zip(moved) {
        // Un sommet touché deux fois (pinceau et reflet au milieu) garde le dernier déplacement, comme avant
        if q.iter().all(|x| x.is_finite()) {
            m.pos[i] = q;
        }
    }
}

/// « Saisir » : les sommets pris au début du geste suivent la souris, atténués au bord.
/// Ajoute au lieu de remplacer : un sommet pris par le pinceau et par son reflet reçoit les deux.
pub fn grab(m: &mut EMesh, hits: &[(usize, f32)], delta: V3) {
    for &(i, w) in hits {
        if let Some(p) = m.pos.get_mut(i) {
            *p = [p[0] + delta[0] * w, p[1] + delta[1] * w, p[2] + delta[2] * w];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> EMesh {
        // Une grille plate de 11 × 11 sommets dans le plan XZ, la normale vers +Y
        let n = 11;
        let mut pos = vec![];
        for j in 0..n {
            for i in 0..n {
                pos.push([i as f32 / 10.0 - 0.5, 0.0, j as f32 / 10.0 - 0.5]);
            }
        }
        let mut polys = vec![];
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let a = (j * n + i) as u32;
                polys.push(vec![a, a + n as u32, a + n as u32 + 1, a + 1]);
            }
        }
        EMesh { pos, polys }
    }

    #[test]
    fn pinceaux() {
        let mut m = grid();
        let nb = Topo::new(&m);
        let n = normals(&m);
        assert!(n[60][1] > 0.99, "{:?}", n[60]);
        // La normale locale est celle du calcul complet
        for i in [0, 5, 60, 120] {
            let l = nb.normal(&m, i);
            assert!((0..3).all(|k| (l[k] - n[i][k]).abs() < 1e-5));
        }
        // Entrées douteuses : indice hors du maillage, poids NaN, rien ne panique ni ne casse les positions
        let mut bad = grid();
        dab(&mut bad, &nb, Brush::Draw, &[(9999, 1.0), (3, f32::NAN)], 0.1, false);
        assert!(bad.pos.iter().flatten().all(|x| x.is_finite()));
        // Dessiner : le centre monte, plus que les bords du pinceau
        let hits: Vec<(usize, f32)> = vec![(60, 1.0), (61, 0.5), (59, 0.5)];
        dab(&mut m, &nb, Brush::Draw, &hits, 0.1, false);
        assert!(m.pos[60][1] > m.pos[61][1] && m.pos[61][1] > 0.0);
        // Creuser (Ctrl)
        let mut c = grid();
        dab(&mut c, &nb, Brush::Draw, &hits, 0.1, true);
        assert!(c.pos[60][1] < 0.0);
        // Lisser : la bosse redescend
        let before = m.pos[60][1];
        let all: Vec<(usize, f32)> = (0..m.pos.len()).map(|i| (i, 1.0)).collect();
        dab(&mut m, &nb, Brush::Smooth, &all, 0.1, false);
        assert!(m.pos[60][1] < before);
        // Saisir
        let start = m.pos.clone();
        grab(&mut m, &[(0, 1.0)], [0.0, 1.0, 0.0]);
        assert!((m.pos[0][1] - start[0][1] - 1.0).abs() < 1e-5);
        assert!(falloff(0.0) == 1.0 && falloff(1.0) == 0.0);
    }
}
