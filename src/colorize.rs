//! Mise en couleur pour l'animation 2D (Retouche et storyboard) : zones fermées d'un dessin au trait
//! (les petits trous du trait se referment), pot de peinture, palette de référence tirée d'un dessin
//! déjà colorié, et coloriage d'un dessin d'après un autre (chaque zone prend la couleur de la même zone).
//! Les images sont en RGBA, 4 octets par pixel.

use std::collections::HashMap;

/// Distance (en pas de côté) de chaque pixel au pixel fermé le plus proche, en deux passes
fn dist_closed(open: &[bool], w: usize, h: usize) -> Vec<u32> {
    const FAR: u32 = u32::MAX / 2;
    let mut d: Vec<u32> = open.iter().map(|o| if *o { FAR } else { 0 }).collect();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if d[i] == 0 {
                continue;
            }
            if x > 0 {
                d[i] = d[i].min(d[i - 1] + 1);
            }
            if y > 0 {
                d[i] = d[i].min(d[i - w] + 1);
            }
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            if d[i] == 0 {
                continue;
            }
            if x + 1 < w {
                d[i] = d[i].min(d[i + 1] + 1);
            }
            if y + 1 < h {
                d[i] = d[i].min(d[i + w] + 1);
            }
        }
    }
    d
}

/// Remplit par voisinage (4 côtés) depuis `seed` les pixels où `ok` est vrai ; pose `id` dans `lab`
fn flood(lab: &mut [u32], w: usize, h: usize, seed: usize, id: u32, ok: impl Fn(usize) -> bool, stack: &mut Vec<usize>) -> usize {
    stack.clear();
    stack.push(seed);
    lab[seed] = id;
    let mut n = 0;
    while let Some(i) = stack.pop() {
        n += 1;
        let (x, y) = (i % w, i / w);
        let mut go = |j: usize| {
            if lab[j] == 0 && ok(j) {
                lab[j] = id;
                stack.push(j);
            }
        };
        if x > 0 {
            go(i - 1);
        }
        if x + 1 < w {
            go(i + 1);
        }
        if y > 0 {
            go(i - w);
        }
        if y + 1 < h {
            go(i + w);
        }
    }
    n
}

/// Les zones étiquetées regagnent le terrain libre de `open`, toutes ensemble et de proche en proche :
/// chaque pixel va à la zone la plus proche, et deux zones se rejoignent au milieu d'un trou du trait
fn regrow(lab: &mut [u32], open: &[bool], w: usize, h: usize) {
    let mut front: Vec<usize> = (0..w * h).filter(|&i| lab[i] != 0).filter(|&i| {
        let (x, y) = (i % w, i / w);
        (x > 0 && lab[i - 1] == 0) || (x + 1 < w && lab[i + 1] == 0) || (y > 0 && lab[i - w] == 0) || (y + 1 < h && lab[i + w] == 0)
    }).collect();
    let mut next = vec![];
    while !front.is_empty() {
        for &i in &front {
            let (x, y, id) = (i % w, i / w, lab[i]);
            let mut go = |j: usize| {
                if lab[j] == 0 && open[j] {
                    lab[j] = id;
                    next.push(j);
                }
            };
            if x > 0 {
                go(i - 1);
            }
            if x + 1 < w {
                go(i + 1);
            }
            if y > 0 {
                go(i - w);
            }
            if y + 1 < h {
                go(i + w);
            }
        }
        front.clear();
        std::mem::swap(&mut front, &mut next);
    }
}

/// Remplissage qui ne passe pas par les trous plus petits que `gap` : la zone `open` (où l'on peut peindre)
/// est rétrécie de `gap`, remplie depuis la graine, puis regagne le terrain jusqu'au trait (coins compris),
/// en le partageant avec ce qui se trouve de l'autre côté des trous.
/// None si la graine tombe dans le trait épaissi (on remplit alors normalement).
pub fn gap_fill(open: &[bool], w: usize, h: usize, sx: usize, sy: usize, gap: usize) -> Option<Vec<bool>> {
    if open.len() != w * h || sx >= w || sy >= h {
        return None;
    }
    let d = dist_closed(open, w, h);
    let seed = sy * w + sx;
    if d[seed] as usize <= gap {
        return None;
    }
    let mut lab = vec![0u32; w * h];
    flood(&mut lab, w, h, seed, 1, |j| d[j] as usize > gap, &mut vec![]);
    if gap > 0 {
        // Le reste du terrain large (l'autre côté des trous) dispute la bande le long du trait
        for i in 0..w * h {
            if lab[i] == 0 && d[i] as usize > gap {
                lab[i] = 2;
            }
        }
        regrow(&mut lab, open, w, h);
    }
    Some(lab.into_iter().map(|l| l == 1).collect())
}

/// Les zones fermées d'un dessin : pour chaque pixel, le numéro de sa zone (0 : trait). Les trous du trait
/// plus petits que `gap` ne relient pas deux zones ; les recoins trop minces forment leurs propres zones.
pub fn regions(open: &[bool], w: usize, h: usize, gap: usize) -> (Vec<u32>, u32) {
    let mut lab = vec![0u32; w * h];
    if open.len() != w * h {
        return (lab, 0);
    }
    let d = dist_closed(open, w, h);
    let mut stack = vec![];
    let mut n = 0;
    for i in 0..w * h {
        if lab[i] == 0 && d[i] as usize > gap {
            n += 1;
            flood(&mut lab, w, h, i, n, |j| d[j] as usize > gap, &mut stack);
        }
    }
    if gap > 0 {
        regrow(&mut lab, open, w, h);
    }
    for i in 0..w * h {
        if lab[i] == 0 && open[i] {
            n += 1;
            flood(&mut lab, w, h, i, n, |j| open[j], &mut stack);
        }
    }
    (lab, n)
}

fn dist2(a: [u8; 4], b: [u8; 4]) -> i32 {
    (0..3).map(|k| (a[k] as i32 - b[k] as i32).pow(2)).sum()
}

/// La couleur de la palette la plus proche
pub fn nearest(palette: &[[u8; 4]], c: [u8; 4]) -> Option<[u8; 4]> {
    palette.iter().copied().min_by_key(|p| dist2(*p, c))
}

/// La palette d'un dessin colorié : ses aplats, du plus étendu au moins étendu (au plus `max` couleurs).
/// Les teintes de passage (bords adoucis) et les couleurs presque pareilles à une couleur déjà prise sont écartées.
pub fn palette_of(px: &[u8], max: usize) -> Vec<[u8; 4]> {
    let mut count: HashMap<[u8; 3], usize> = HashMap::new();
    let mut total = 0usize;
    for c in px.chunks_exact(4).filter(|c| c[3] >= 250) {
        *count.entry([c[0], c[1], c[2]]).or_default() += 1;
        total += 1;
    }
    let mut all: Vec<([u8; 3], usize)> = count.into_iter().collect();
    all.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let floor = (total / 500).max(4);
    let mut out: Vec<[u8; 4]> = vec![];
    for (c, n) in all {
        if n < floor || out.len() >= max {
            break;
        }
        let c = [c[0], c[1], c[2], 255];
        if out.iter().all(|o| dist2(*o, c) > 24 * 24) {
            out.push(c);
        }
    }
    out
}

fn centroid(px: &[u8], w: usize) -> Option<[f32; 2]> {
    let (mut sx, mut sy, mut n) = (0f64, 0f64, 0f64);
    for (i, c) in px.chunks_exact(4).enumerate() {
        if c[3] >= 128 {
            sx += (i % w) as f64;
            sy += (i / w) as f64;
            n += 1.0;
        }
    }
    (n > 0.0).then(|| [(sx / n) as f32, (sy / n) as f32])
}

/// Colorie le dessin `cur` (traits sur fond transparent) d'après `reference` (le même personnage déjà colorié,
/// par exemple l'image d'avant) : chaque zone fermée prend la couleur la plus présente au même endroit du modèle.
/// Le décalage d'ensemble entre les deux dessins est rattrapé. Avec une `palette`, seules ses couleurs sont posées.
/// Les zones sans couleur dans le modèle (le fond) restent vides ; un ton léger déjà posé reste par-dessus la couleur.
/// `premul` : pixels à alpha prémultiplié (tiny-skia). Rend le nombre de zones coloriées.
pub fn colorize(cur: &mut [u8], reference: &[u8], w: usize, h: usize, gap: usize, palette: &[[u8; 4]], premul: bool) -> usize {
    if cur.len() != w * h * 4 || reference.len() != cur.len() || w == 0 || h == 0 {
        return 0;
    }
    let open: Vec<bool> = cur.chunks_exact(4).map(|c| c[3] < 128).collect();
    let (lab, n) = regions(&open, w, h, gap);
    if n == 0 {
        return 0;
    }
    // La couleur du trait : la plus fréquente parmi les pixels dessinés (elle compte moins dans le vote)
    let mut ink: HashMap<[u8; 3], usize> = HashMap::new();
    for c in cur.chunks_exact(4).filter(|c| c[3] >= 250) {
        *ink.entry([c[0], c[1], c[2]]).or_default() += 1;
    }
    let ink = ink.into_iter().max_by_key(|(c, k)| (*k, *c)).map(|(c, _)| [c[0], c[1], c[2], 255]);
    // Décalage d'ensemble : d'un centre du dessin à l'autre, borné au quart de l'image
    let shift = match (centroid(cur, w), centroid(reference, w)) {
        (Some(a), Some(b)) => [((b[0] - a[0]).round() as i32).clamp(-(w as i32) / 4, w as i32 / 4), ((b[1] - a[1]).round() as i32).clamp(-(h as i32) / 4, h as i32 / 4)],
        _ => [0, 0],
    };
    let mut votes: Vec<HashMap<[u8; 3], u32>> = vec![HashMap::new(); n as usize + 1];
    let mut none = vec![0u32; n as usize + 1];
    let mut snap: HashMap<[u8; 3], [u8; 3]> = HashMap::new();
    for i in 0..w * h {
        let id = lab[i] as usize;
        if id == 0 {
            continue;
        }
        let (x, y) = ((i % w) as i32 + shift[0], (i / w) as i32 + shift[1]);
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            none[id] += 4;
            continue;
        }
        let r = &reference[(y as usize * w + x as usize) * 4..][..4];
        if r[3] < 128 {
            none[id] += 4;
            continue;
        }
        let mut c = [r[0], r[1], r[2]];
        let is_ink = ink.is_some_and(|k| dist2(k, [c[0], c[1], c[2], 255]) < 40 * 40);
        if !palette.is_empty() {
            c = *snap.entry(c).or_insert_with(|| nearest(palette, [c[0], c[1], c[2], 255]).map(|p| [p[0], p[1], p[2]]).unwrap_or(c));
        }
        *votes[id].entry(c).or_default() += if is_ink { 1 } else { 4 };
    }
    let chosen: Vec<Option<[u8; 3]>> = (0..=n as usize).map(|id| {
        let (c, k) = votes[id].iter().max_by_key(|(c, k)| (**k, **c))?;
        (*k >= none[id]).then_some(*c)
    }).collect();
    for i in 0..w * h {
        if let Some(c) = chosen[lab[i] as usize].filter(|_| lab[i] != 0) {
            under(&mut cur[i * 4..i * 4 + 4], [c[0], c[1], c[2], 255], premul);
        }
    }
    under_edges(cur, &lab, w, h, |id| chosen[id as usize].map(|c| [c[0], c[1], c[2], 255]), premul);
    chosen.iter().flatten().count()
}

/// Les bords adoucis du trait laissent voir le fond : la couleur de la zone voisine passe dessous
fn under_edges(px: &mut [u8], lab: &[u32], w: usize, h: usize, color: impl Fn(u32) -> Option<[u8; 4]>, premul: bool) {
    for i in 0..w * h {
        let a = px[i * 4 + 3];
        if lab[i] != 0 || a == 255 {
            continue;
        }
        let (x, y) = (i % w, i / w);
        let mut found = None;
        for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            let (xx, yy) = (x as i32 + dx, y as i32 + dy);
            if xx < 0 || yy < 0 || xx >= w as i32 || yy >= h as i32 {
                continue;
            }
            let l = lab[yy as usize * w + xx as usize];
            if l != 0 {
                if let Some(c) = color(l) {
                    found = Some(c);
                    break;
                }
            }
        }
        let Some(c) = found else { continue };
        under(&mut px[i * 4..i * 4 + 4], c, premul);
    }
}

/// Glisse la couleur `c` sous un pixel (ce qu'il porte déjà reste par-dessus)
fn under(p: &mut [u8], c: [u8; 4], premul: bool) {
    let k = p[3] as f32 / 255.0;
    let ca = c[3] as f32 / 255.0;
    let oa = k + ca * (1.0 - k);
    for j in 0..3 {
        let top = p[j] as f32;
        let v = if premul {
            top + c[j] as f32 * ca * (1.0 - k)
        } else if oa > 0.0 {
            (top * k + c[j] as f32 * ca * (1.0 - k)) / oa
        } else {
            0.0
        };
        p[j] = v.round().clamp(0.0, 255.0) as u8;
    }
    p[3] = (oa * 255.0).round() as u8;
}

/// Pot de peinture : remplit de `col` la zone de même couleur autour de (x, y), à `tol` près (0..1), sans passer
/// par les trous du trait plus petits que `gap`. `premul` : pixels à alpha prémultiplié (tiny-skia).
/// Rend false si rien n'a été rempli.
pub fn bucket(px: &mut [u8], w: usize, h: usize, x: usize, y: usize, col: [u8; 4], tol: f32, gap: usize, premul: bool) -> bool {
    if px.len() != w * h * 4 || x >= w || y >= h {
        return false;
    }
    let i0 = (y * w + x) * 4;
    let target = [px[i0], px[i0 + 1], px[i0 + 2], px[i0 + 3]];
    let lim = (tol.clamp(0.0, 1.0) * 255.0).round() as i32;
    let open: Vec<bool> = px.chunks_exact(4).map(|p| (0..4).all(|k| (p[k] as i32 - target[k] as i32).abs() <= lim)).collect();
    let region = gap_fill(&open, w, h, x, y, gap).or_else(|| gap_fill(&open, w, h, x, y, 0));
    let Some(region) = region else { return false };
    let put = if premul { [0, 1, 2].map(|k| (col[k] as u32 * col[3] as u32 / 255) as u8) } else { [col[0], col[1], col[2]] };
    for (i, on) in region.iter().enumerate() {
        if *on {
            px[i * 4..i * 4 + 4].copy_from_slice(&[put[0], put[1], put[2], col[3]]);
        }
    }
    // Sur un fond vide, la couleur passe sous les bords adoucis du trait
    if target[3] == 0 {
        let lab: Vec<u32> = region.iter().map(|r| *r as u32).collect();
        under_edges(px, &lab, w, h, |_| Some(col), premul);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un cadre au trait noir (2 px), coin haut-gauche (x, y), côté s, avec une ouverture de `hole` px à droite
    fn frame(px: &mut [u8], w: usize, x: usize, y: usize, s: usize, hole: usize) {
        for k in 0..s {
            for t in 0..2 {
                for (cx, cy) in [(x + k, y + t), (x + k, y + s - 1 - t), (x + t, y + k), (x + s - 1 - t, y + k)] {
                    if cx >= x + s - 2 && cy >= y + s / 2 && cy < y + s / 2 + hole {
                        continue;
                    }
                    px[(cy * w + cx) * 4..][..4].copy_from_slice(&[10, 10, 12, 255]);
                }
            }
        }
    }

    #[test]
    fn zones_et_trous() {
        let (w, h) = (60usize, 40usize);
        let mut px = vec![0u8; w * h * 4];
        frame(&mut px, w, 5, 5, 20, 3);
        frame(&mut px, w, 32, 8, 20, 0);
        let open: Vec<bool> = px.chunks_exact(4).map(|c| c[3] < 128).collect();
        // Sans fermeture : le premier cadre fuit, il ne reste que deux zones (dehors + premier cadre, second cadre)
        assert_eq!(regions(&open, w, h, 0).1, 2);
        // Avec 3 px : dehors, et chaque cadre ; tous les pixels libres ont une zone
        let (lab, n) = regions(&open, w, h, 3);
        assert_eq!(n, 3);
        assert!(lab.iter().zip(&open).all(|(l, o)| (*l != 0) == *o));
        assert_ne!(lab[15 * w + 15], lab[0]);
        assert_ne!(lab[15 * w + 15], lab[18 * w + 42]);
        // Même réponse que le pot : la zone du premier cadre, jusqu'au trait
        let r = gap_fill(&open, w, h, 15, 15, 3).unwrap();
        assert!(r[15 * w + 15] && r[15 * w + 7] && !r[2 * w + 2] && !r[18 * w + 42]);
        assert!(gap_fill(&open, w, h, 5, 5, 3).is_none(), "graine sur le trait");
        assert!(gap_fill(&open, w, h, 99, 0, 3).is_none(), "graine hors de l'image");
    }

    #[test]
    fn coloriage_d_apres_un_modele() {
        let (w, h) = (60usize, 40usize);
        // Le modèle : deux cadres, coloriés en rouge et en bleu
        let mut model = vec![0u8; w * h * 4];
        frame(&mut model, w, 5, 5, 20, 0);
        frame(&mut model, w, 32, 8, 20, 0);
        assert!(bucket(&mut model, w, h, 15, 15, [220, 60, 50, 255], 0.1, 0, false));
        assert!(bucket(&mut model, w, h, 42, 18, [50, 90, 220, 255], 0.1, 0, false));
        assert_eq!(&model[(15 * w + 8) * 4..][..4], &[220, 60, 50, 255]);
        assert_eq!(&model[0..4], &[0, 0, 0, 0], "le dehors reste vide");
        let pal = palette_of(&model, 8);
        assert_eq!(pal.len(), 3, "rouge, bleu et le trait : {pal:?}");
        assert!(pal.contains(&[220, 60, 50, 255]) && pal.contains(&[50, 90, 220, 255]));
        // Le dessin suivant : les mêmes cadres 3 px plus loin, le premier avec un trou dans le trait
        let mut cur = vec![0u8; w * h * 4];
        frame(&mut cur, w, 8, 6, 20, 3);
        frame(&mut cur, w, 35, 9, 20, 0);
        // Un ton léger déjà posé dans le premier cadre : il reste par-dessus la couleur
        cur[(20 * w + 15) * 4..][..4].copy_from_slice(&[0, 0, 0, 51]);
        let n = colorize(&mut cur, &model, w, h, 3, &[], false);
        assert_eq!(&cur[(20 * w + 15) * 4..][..4], &[176, 48, 40, 255]);
        assert_eq!(n, 2);
        assert_eq!(&cur[(16 * w + 18) * 4..][..4], &[220, 60, 50, 255]);
        assert_eq!(&cur[(19 * w + 45) * 4..][..4], &[50, 90, 220, 255]);
        assert_eq!(&cur[(2 * w + 2) * 4..][..4], &[0, 0, 0, 0], "le fond n'est pas colorié");
        assert_eq!(&cur[(6 * w + 12) * 4..][..4], &[10, 10, 12, 255], "le trait est intact");
        // Avec une palette de référence : seules ses couleurs sont posées
        let mut cur2 = vec![0u8; w * h * 4];
        frame(&mut cur2, w, 8, 6, 20, 0);
        frame(&mut cur2, w, 35, 9, 20, 0);
        colorize(&mut cur2, &model, w, h, 2, &[[230, 70, 60, 255], [40, 80, 200, 255], [10, 10, 12, 255]], false);
        assert_eq!(&cur2[(16 * w + 18) * 4..][..4], &[230, 70, 60, 255]);
        assert_eq!(&cur2[(19 * w + 45) * 4..][..4], &[40, 80, 200, 255]);
        assert_eq!(nearest(&[[0, 0, 0, 255], [255, 255, 255, 255]], [200, 190, 180, 255]), Some([255, 255, 255, 255]));
        // Tailles incohérentes : rien, sans panique
        assert_eq!(colorize(&mut cur2, &model[..40], w, h, 2, &[], false), 0);
    }

    #[test]
    fn pot_et_bords_adoucis() {
        // Alpha prémultiplié (storyboard) : un bord de trait à demi transparent reçoit la couleur dessous
        let (w, h) = (20usize, 20usize);
        let mut px = vec![0u8; w * h * 4];
        frame(&mut px, w, 2, 2, 16, 0);
        px[(10 * w + 4) * 4..][..4].copy_from_slice(&[5, 5, 6, 128]);
        assert!(bucket(&mut px, w, h, 10, 10, [200, 100, 0, 255], 0.05, 2, true));
        assert_eq!(&px[(10 * w + 10) * 4..][..4], &[200, 100, 0, 255]);
        let e = &px[(10 * w + 4) * 4..][..4];
        assert_eq!(e[3], 255);
        assert!(e[0] > 90 && e[0] < 115, "{e:?}");
        assert_eq!(&px[0..4], &[0, 0, 0, 0]);
        assert!(!bucket(&mut px, w, h, 40, 2, [0, 0, 0, 255], 0.1, 0, true));
    }
}
