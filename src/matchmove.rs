//! Suivi de caméra (matchmove) : des points suivis dans une vidéo, on retrouve le mouvement de la
//! caméra et un nuage de points de la scène filmée, puis on les pose dans une scène 3D.
//!
//! Trois étapes : `track_features` (coins détectés et suivis d'image en image), `solve` (caméra et
//! points à partir des pistes) et `to_scene` (caméra animée, repères, vidéo en fond).

use crate::track2d::Frames;

type V = [f64; 3];
type M3 = [[f64; 3]; 3];

/// Un point suivi : image où il apparaît, puis sa position (pixels) à chaque image suivante
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub start: usize,
    pub pts: Vec<[f32; 2]>,
}

impl Track {
    pub fn at(&self, f: usize) -> Option<[f32; 2]> {
        f.checked_sub(self.start).and_then(|k| self.pts.get(k)).copied()
    }
}

// ---------------------------------------------------------------- suivi 2D

/// Image en flottants et ses réductions de moitié (du plus fin au plus grossier)
struct Pyramid {
    levels: Vec<(usize, usize, Vec<f32>)>,
}

impl Pyramid {
    fn new(px: &[u8], w: usize, h: usize, n: usize) -> Pyramid {
        let mut levels = vec![(w, h, px.iter().map(|&v| v as f32).collect::<Vec<f32>>())];
        for _ in 1..n {
            let (w0, h0, a) = levels.last().unwrap();
            let (w1, h1) = (w0 / 2, h0 / 2);
            if w1 < 24 || h1 < 24 {
                break;
            }
            let mut b = Vec::with_capacity(w1 * h1);
            for y in 0..h1 {
                for x in 0..w1 {
                    let i = y * 2 * w0 + x * 2;
                    b.push((a[i] + a[i + 1] + a[i + w0] + a[i + w0 + 1]) * 0.25);
                }
            }
            levels.push((w1, h1, b));
        }
        Pyramid { levels }
    }
}

fn sample(img: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let (x, y) = (x.clamp(0.0, w as f32 - 1.001), y.clamp(0.0, h as f32 - 1.001));
    let (x0, y0) = (x as usize, y as usize);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let i = y0 * w + x0;
    let a = img[i] * (1.0 - fx) + img[i + 1] * fx;
    let b = img[i + w] * (1.0 - fx) + img[i + w + 1] * fx;
    a * (1.0 - fy) + b * fy
}

/// Demi-taille du motif suivi
const R: i32 = 7;

/// Un motif : ses valeurs (moyenne retirée) et ses pentes en x et en y
#[derive(Clone)]
struct Patch {
    v: Vec<f32>,
    gx: Vec<f32>,
    gy: Vec<f32>,
}

fn patch(img: &[f32], w: usize, h: usize, c: [f32; 2]) -> Patch {
    let n = ((2 * R + 1) * (2 * R + 1)) as usize;
    let (mut v, mut gx, mut gy) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
    for dy in -R..=R {
        for dx in -R..=R {
            let (x, y) = (c[0] + dx as f32, c[1] + dy as f32);
            v.push(sample(img, w, h, x, y));
            gx.push((sample(img, w, h, x + 1.0, y) - sample(img, w, h, x - 1.0, y)) * 0.5);
            gy.push((sample(img, w, h, x, y + 1.0) - sample(img, w, h, x, y - 1.0)) * 0.5);
        }
    }
    let m = v.iter().sum::<f32>() / n as f32;
    v.iter_mut().for_each(|x| *x -= m);
    Patch { v, gx, gy }
}

/// Cherche le motif `t` dans l'image autour de `q` (Lucas-Kanade, translation seule). Rend la
/// position trouvée et l'écart entre le motif et ce qu'on trouve là, rapporté au contraste du motif.
fn lk(t: &Patch, img: &[f32], w: usize, h: usize, mut q: [f32; 2]) -> Option<([f32; 2], f32)> {
    let (mut a, mut b, mut c) = (0.0f32, 0.0f32, 0.0f32);
    for i in 0..t.v.len() {
        a += t.gx[i] * t.gx[i];
        b += t.gx[i] * t.gy[i];
        c += t.gy[i] * t.gy[i];
    }
    let det = a * c - b * b;
    if !(det > 1e-3) {
        return None;
    }
    let n = t.v.len() as f32;
    let margin = R as f32 + 1.0;
    let mut err = f32::MAX;
    for _ in 0..20 {
        if !(q[0] >= margin && q[1] >= margin && q[0] < w as f32 - margin - 1.0 && q[1] < h as f32 - margin - 1.0) {
            return None;
        }
        let mut cur = Vec::with_capacity(t.v.len());
        for dy in -R..=R {
            for dx in -R..=R {
                cur.push(sample(img, w, h, q[0] + dx as f32, q[1] + dy as f32));
            }
        }
        let m = cur.iter().sum::<f32>() / n;
        let (mut ex, mut ey, mut e2) = (0.0f32, 0.0f32, 0.0f32);
        for i in 0..cur.len() {
            let d = t.v[i] - (cur[i] - m);
            ex += d * t.gx[i];
            ey += d * t.gy[i];
            e2 += d * d;
        }
        // Écart rapporté au relief du motif : 0 identique, 1 aussi différent que le motif a de contraste
        err = (e2 / n).sqrt() / (t.v.iter().map(|x| x * x).sum::<f32>() / n).sqrt().max(1.0);
        // Le déplacement qui annule au mieux l'écart, d'après les pentes du motif
        let (dx, dy) = ((c * ex - b * ey) / det, (a * ey - b * ex) / det);
        q = [q[0] + dx.clamp(-3.0, 3.0), q[1] + dy.clamp(-3.0, 3.0)];
        if dx * dx + dy * dy < 1e-4 {
            break;
        }
    }
    q.iter().all(|x| x.is_finite()).then_some((q, err))
}

/// Les coins les mieux marqués de l'image (plus petite valeur propre du tenseur de structure),
/// écartés d'au moins `min_dist` entre eux et des points de `avoid`.
pub fn detect(px: &[u8], w: usize, h: usize, max: usize, min_dist: f32, avoid: &[[f32; 2]]) -> Vec<[f32; 2]> {
    if w < 32 || h < 32 || px.len() < w * h || max == 0 {
        return vec![];
    }
    let at = |x: usize, y: usize| px[y * w + x] as f32;
    let (mut ixx, mut ixy, mut iyy) = (vec![0.0f32; w * h], vec![0.0f32; w * h], vec![0.0f32; w * h]);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let gx = (at(x + 1, y) - at(x - 1, y)) * 0.5;
            let gy = (at(x, y + 1) - at(x, y - 1)) * 0.5;
            let i = y * w + x;
            (ixx[i], ixy[i], iyy[i]) = (gx * gx, gx * gy, gy * gy);
        }
    }
    // Sommes sur une fenêtre de 7 × 7, en deux passes (lignes puis colonnes)
    let blur = |src: &[f32]| -> Vec<f32> {
        let mut tmp = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 3..w - 3 {
                let i = y * w + x;
                tmp[i] = src[i - 3..=i + 3].iter().sum();
            }
        }
        let mut out = vec![0.0f32; w * h];
        for y in 3..h - 3 {
            for x in 0..w {
                out[y * w + x] = (0..7).map(|k| tmp[(y + k - 3) * w + x]).sum();
            }
        }
        out
    };
    let (sxx, sxy, syy) = (blur(&ixx), blur(&ixy), blur(&iyy));
    let border = (R as usize + 3).max(8);
    let mut cands: Vec<(f32, usize, usize)> = vec![];
    for y in border..h - border {
        for x in border..w - border {
            let i = y * w + x;
            let (a, b, c) = (sxx[i], sxy[i], syy[i]);
            let s = (a + c) * 0.5 - (((a - c) * 0.5).powi(2) + b * b).sqrt();
            if s > 40.0 {
                cands.push((s, x, y));
            }
        }
    }
    cands.sort_by(|a, b| b.0.total_cmp(&a.0));
    // Une grille dont la case vaut la distance minimale : un seul point par voisinage
    let cell = min_dist.max(2.0);
    let (gw, gh) = ((w as f32 / cell) as usize + 1, (h as f32 / cell) as usize + 1);
    let mut grid: Vec<Vec<[f32; 2]>> = vec![vec![]; gw * gh];
    let free = |grid: &Vec<Vec<[f32; 2]>>, p: [f32; 2]| -> bool {
        let (cx, cy) = ((p[0] / cell) as i64, (p[1] / cell) as i64);
        for yy in cy - 1..=cy + 1 {
            for xx in cx - 1..=cx + 1 {
                if xx < 0 || yy < 0 || xx >= gw as i64 || yy >= gh as i64 {
                    continue;
                }
                if grid[yy as usize * gw + xx as usize].iter().any(|q| (q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) < cell * cell) {
                    return false;
                }
            }
        }
        true
    };
    let put = |grid: &mut Vec<Vec<[f32; 2]>>, p: [f32; 2]| {
        let (cx, cy) = (((p[0] / cell) as usize).min(gw - 1), ((p[1] / cell) as usize).min(gh - 1));
        grid[cy * gw + cx].push(p);
    };
    for p in avoid.iter().filter(|p| p[0] >= 0.0 && p[1] >= 0.0 && p[0].is_finite() && p[1].is_finite()) {
        put(&mut grid, *p);
    }
    let mut out = vec![];
    for (_, x, y) in cands {
        let p = [x as f32, y as f32];
        if free(&grid, p) {
            put(&mut grid, p);
            out.push(p);
            if out.len() >= max {
                break;
            }
        }
    }
    out
}

/// Détecte environ `want` coins et les suit dans tout le passage. Un point qu'on perd (sorti de
/// l'image, caché, ou qui ne ressemble plus à son motif de départ) s'arrête ; d'autres sont
/// détectés à mesure pour garder le compte. Les pistes trop courtes sont écartées.
pub fn track_features(f: &Frames, want: usize, progress: &dyn Fn(f32)) -> Vec<Track> {
    struct Live {
        id: usize,
        pos: [f32; 2],
        reference: Patch,
    }
    let (w, h) = (f.w, f.h);
    if f.px.is_empty() || f.px.iter().any(|p| p.len() < w * h) || w < 32 || h < 32 {
        return vec![];
    }
    let min_dist = (((w * h) as f32 / want.max(1) as f32).sqrt() * 0.6).clamp(8.0, 60.0);
    let mut tracks: Vec<Track> = vec![];
    let mut live: Vec<Live> = vec![];
    let mut prev = Pyramid::new(&f.px[0], w, h, 3);
    let spawn = |k: usize, pyr: &Pyramid, live: &mut Vec<Live>, tracks: &mut Vec<Track>| {
        let avoid: Vec<[f32; 2]> = live.iter().map(|l| l.pos).collect();
        let (_, _, img) = &pyr.levels[0];
        for p in detect(&f.px[k], w, h, want.saturating_sub(live.len()), min_dist, &avoid) {
            live.push(Live { id: tracks.len(), pos: p, reference: patch(img, w, h, p) });
            tracks.push(Track { start: k, pts: vec![p] });
        }
    };
    spawn(0, &prev, &mut live, &mut tracks);
    for k in 1..f.px.len() {
        let cur = Pyramid::new(&f.px[k], w, h, 3);
        // Du plus grossier au plus fin, depuis l'image précédente ; puis l'aller-retour pour vérifier
        let step = |from: &Pyramid, to: &Pyramid, p: [f32; 2]| -> Option<[f32; 2]> {
            let top = from.levels.len().min(to.levels.len()) - 1;
            let mut d = [0.0f32; 2];
            for l in (0..=top).rev() {
                let s = (1 << l) as f32;
                let (lw, lh, a) = &from.levels[l];
                let (_, _, b) = &to.levels[l];
                let c = [p[0] / s, p[1] / s];
                let t = patch(a, *lw, *lh, c);
                match lk(&t, b, *lw, *lh, [c[0] + d[0] / s, c[1] + d[1] / s]) {
                    Some((q, _)) => d = [(q[0] - c[0]) * s, (q[1] - c[1]) * s],
                    // Un niveau grossier sans relief ne dit rien : on garde l'estimation ; le plus fin doit réussir
                    None if l > 0 => {}
                    None => return None,
                }
            }
            Some([p[0] + d[0], p[1] + d[1]])
        };
        let (_, _, img) = &cur.levels[0];
        live.retain_mut(|l| {
            let Some(q) = step(&prev, &cur, l.pos) else { return false };
            let Some(back) = step(&cur, &prev, q) else { return false };
            if (back[0] - l.pos[0]).powi(2) + (back[1] - l.pos[1]).powi(2) > 1.0 {
                return false;
            }
            // Recalage sur le motif de départ : sans lui, les petites erreurs s'ajoutent et le point glisse
            let q = match lk(&l.reference, img, w, h, q) {
                Some((r, err)) if (r[0] - q[0]).powi(2) + (r[1] - q[1]).powi(2) < 4.0 && err < 0.5 => r,
                // Le motif a changé (perspective, lumière) sans être caché : on repart de ce qu'il est devenu
                Some((_, err)) if err < 0.9 => {
                    l.reference = patch(img, w, h, q);
                    q
                }
                _ => return false,
            };
            l.pos = q;
            tracks[l.id].pts.push(q);
            true
        });
        if live.len() * 10 < want * 8 {
            spawn(k, &cur, &mut live, &mut tracks);
        }
        prev = cur;
        progress(k as f32 / f.px.len() as f32);
    }
    tracks.retain(|t| t.pts.len() >= 5);
    tracks
}

// ---------------------------------------------------------------- algèbre

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V, k: f64) -> V {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn len(a: V) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: V) -> V {
    mul(a, 1.0 / len(a).max(1e-300))
}
fn mv(m: &M3, v: V) -> V {
    [dot(m[0], v), dot(m[1], v), dot(m[2], v)]
}
fn tr(m: &M3) -> M3 {
    [[m[0][0], m[1][0], m[2][0]], [m[0][1], m[1][1], m[2][1]], [m[0][2], m[1][2], m[2][2]]]
}
fn mm(a: &M3, b: &M3) -> M3 {
    let bt = tr(b);
    [[dot(a[0], bt[0]), dot(a[0], bt[1]), dot(a[0], bt[2])], [dot(a[1], bt[0]), dot(a[1], bt[1]), dot(a[1], bt[2])], [dot(a[2], bt[0]), dot(a[2], bt[1]), dot(a[2], bt[2])]]
}
const ID: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Rotation d'un vecteur de rotation (axe × angle)
fn exp_rot(w: V) -> M3 {
    let th = len(w);
    if th < 1e-12 {
        return ID;
    }
    let k = mul(w, 1.0 / th);
    let (s, c) = th.sin_cos();
    let kx = [[0.0, -k[2], k[1]], [k[2], 0.0, -k[0]], [-k[1], k[0], 0.0]];
    let k2 = mm(&kx, &kx);
    let mut r = ID;
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] += s * kx[i][j] + (1.0 - c) * k2[i][j];
        }
    }
    r
}

/// Valeurs et vecteurs propres d'une matrice symétrique (Jacobi) ; vecteurs en colonnes de `v`
fn jacobi(mut a: Vec<Vec<f64>>) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = a.len();
    let mut v = vec![vec![0.0; n]; n];
    for i in 0..n {
        v[i][i] = 1.0;
    }
    for _ in 0..80 {
        let off: f64 = (0..n).flat_map(|p| (p + 1..n).map(move |q| (p, q))).map(|(p, q)| a[p][q] * a[p][q]).sum();
        let scale: f64 = (0..n).map(|i| a[i][i] * a[i][i]).sum::<f64>().max(1e-300);
        if off < 1e-26 * scale {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if a[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let (akp, akq) = (a[k][p], a[k][q]);
                    a[k][p] = c * akp - s * akq;
                    a[k][q] = s * akp + c * akq;
                }
                for k in 0..n {
                    let (apk, aqk) = (a[p][k], a[q][k]);
                    a[p][k] = c * apk - s * aqk;
                    a[q][k] = s * apk + c * aqk;
                }
                for k in 0..n {
                    let (vkp, vkq) = (v[k][p], v[k][q]);
                    v[k][p] = c * vkp - s * vkq;
                    v[k][q] = s * vkp + c * vkq;
                }
            }
        }
    }
    ((0..n).map(|i| a[i][i]).collect(), v)
}

/// Résout un petit système linéaire (pivot de Gauss) ; None s'il est singulier
fn solve_lin(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for i in 0..n {
        let p = (i..n).max_by(|&x, &y| a[x][i].abs().total_cmp(&a[y][i].abs()))?;
        if !(a[p][i].abs() > 1e-14) {
            return None;
        }
        a.swap(i, p);
        b.swap(i, p);
        for r in i + 1..n {
            let k = a[r][i] / a[i][i];
            if k != 0.0 {
                for c in i..n {
                    a[r][c] -= k * a[i][c];
                }
                b[r] -= k * b[i];
            }
        }
    }
    for i in (0..n).rev() {
        let s: f64 = (i + 1..n).map(|c| a[i][c] * b[c]).sum();
        b[i] = (b[i] - s) / a[i][i];
    }
    b.iter().all(|x| x.is_finite()).then_some(b)
}

// ---------------------------------------------------------------- géométrie

/// Pose d'une caméra : un point du monde X se voit en R·X + t (z vers l'avant, y vers le bas)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub r: M3,
    pub t: V,
}

impl Pose {
    fn cam(&self, x: V) -> V {
        add(mv(&self.r, x), self.t)
    }
    /// Position de la caméra dans le monde
    pub fn centre(&self) -> V {
        mul(mv(&tr(&self.r), self.t), -1.0)
    }
}

type Obs = (usize, [f64; 2]);

/// Le point du monde vu aux positions données (coordonnées normalisées) par ces caméras
fn triangulate(views: &[(Pose, [f64; 2])]) -> Option<V> {
    let mut a = vec![vec![0.0; 3]; 3];
    let mut b = vec![0.0; 3];
    for (p, x) in views {
        for (row, rhs) in [(sub(p.r[0], mul(p.r[2], x[0])), x[0] * p.t[2] - p.t[0]), (sub(p.r[1], mul(p.r[2], x[1])), x[1] * p.t[2] - p.t[1])] {
            for i in 0..3 {
                for j in 0..3 {
                    a[i][j] += row[i] * row[j];
                }
                b[i] += row[i] * rhs;
            }
        }
    }
    let x = solve_lin(a, b)?;
    let x = [x[0], x[1], x[2]];
    views.iter().all(|(p, _)| p.cam(x)[2] > 1e-6).then_some(x)
}

/// Écart (coordonnées normalisées) entre la projection de `x` et son observation
fn residual(p: &Pose, x: V, o: [f64; 2]) -> [f64; 2] {
    let c = p.cam(x);
    let z = if c[2].abs() < 1e-9 { 1e-9 } else { c[2] };
    [c[0] / z - o[0], c[1] / z - o[1]]
}

/// Poids robuste : au-delà de `d`, une observation fausse ne tire plus aussi fort
fn huber(r: [f64; 2], d: f64) -> f64 {
    let n = (r[0] * r[0] + r[1] * r[1]).sqrt();
    if n <= d { 1.0 } else { d / n }
}

/// Affine la pose d'une caméra pour que les points connus se projettent sur leurs observations
fn refine_pose(pose: &mut Pose, pts: &[(V, [f64; 2])], delta: f64, iters: usize) {
    let cost = |p: &Pose| -> f64 { pts.iter().map(|(x, o)| { let r = residual(p, *x, *o); let w = huber(r, delta); w * (r[0] * r[0] + r[1] * r[1]) }).sum() };
    let mut lambda = 1e-4;
    let mut c0 = cost(pose);
    for _ in 0..iters {
        let mut h = vec![vec![0.0; 6]; 6];
        let mut g = vec![0.0; 6];
        for (x, o) in pts {
            let rx = mv(&pose.r, *x);
            let c = add(rx, pose.t);
            if c[2] < 1e-9 {
                continue;
            }
            let (iz, r) = (1.0 / c[2], residual(pose, *x, *o));
            let w = huber(r, delta);
            // d(projection)/d(point caméra), puis d(point caméra)/d(rotation, translation)
            let dp = [[iz, 0.0, -c[0] * iz * iz], [0.0, iz, -c[1] * iz * iz]];
            let dc = [[0.0, rx[2], -rx[1], 1.0, 0.0, 0.0], [-rx[2], 0.0, rx[0], 0.0, 1.0, 0.0], [rx[1], -rx[0], 0.0, 0.0, 0.0, 1.0]];
            for k in 0..2 {
                let j: Vec<f64> = (0..6).map(|c6| dp[k][0] * dc[0][c6] + dp[k][1] * dc[1][c6] + dp[k][2] * dc[2][c6]).collect();
                for a in 0..6 {
                    for b in 0..6 {
                        h[a][b] += w * j[a] * j[b];
                    }
                    g[a] += w * j[a] * r[k];
                }
            }
        }
        let mut done = false;
        for _ in 0..6 {
            let mut hl = h.clone();
            for i in 0..6 {
                hl[i][i] += lambda * h[i][i].max(1e-12);
            }
            let Some(d) = solve_lin(hl, g.iter().map(|x| -x).collect()) else {
                lambda *= 10.0;
                continue;
            };
            let cand = Pose { r: mm(&exp_rot([d[0], d[1], d[2]]), &pose.r), t: add(pose.t, [d[3], d[4], d[5]]) };
            let c1 = cost(&cand);
            if c1 < c0 {
                done = (c0 - c1) < 1e-12 * c0.max(1e-30);
                *pose = cand;
                c0 = c1;
                lambda = (lambda * 0.3).max(1e-9);
                break;
            }
            lambda *= 10.0;
            if lambda > 1e8 {
                done = true;
                break;
            }
        }
        if done {
            break;
        }
    }
}

/// Affine un point du monde vu par plusieurs caméras
fn refine_point(x: &mut V, views: &[(Pose, [f64; 2])], delta: f64, iters: usize) {
    let cost = |x: V| -> f64 { views.iter().map(|(p, o)| { let r = residual(p, x, *o); huber(r, delta) * (r[0] * r[0] + r[1] * r[1]) }).sum() };
    let mut c0 = cost(*x);
    let mut lambda = 1e-4;
    for _ in 0..iters {
        let mut h = vec![vec![0.0; 3]; 3];
        let mut g = vec![0.0; 3];
        for (p, o) in views {
            let c = p.cam(*x);
            if c[2] < 1e-9 {
                continue;
            }
            let (iz, r) = (1.0 / c[2], residual(p, *x, *o));
            let w = huber(r, delta);
            let dp = [[iz, 0.0, -c[0] * iz * iz], [0.0, iz, -c[1] * iz * iz]];
            for k in 0..2 {
                let j = [0, 1, 2].map(|c3| dp[k][0] * p.r[0][c3] + dp[k][1] * p.r[1][c3] + dp[k][2] * p.r[2][c3]);
                for a in 0..3 {
                    for b in 0..3 {
                        h[a][b] += w * j[a] * j[b];
                    }
                    g[a] += w * j[a] * r[k];
                }
            }
        }
        let mut hl = h.clone();
        for i in 0..3 {
            hl[i][i] += lambda * h[i][i].max(1e-12);
        }
        let Some(d) = solve_lin(hl, g.iter().map(|x| -x).collect()) else { break };
        let cand = [x[0] + d[0], x[1] + d[1], x[2] + d[2]];
        let c1 = cost(cand);
        if c1 < c0 {
            let small = (c0 - c1) < 1e-12 * c0.max(1e-30);
            *x = cand;
            c0 = c1;
            lambda = (lambda * 0.3).max(1e-9);
            if small {
                break;
            }
        } else {
            lambda *= 10.0;
            if lambda > 1e8 {
                break;
            }
        }
    }
}

/// Matrice essentielle par les huit points : x2ᵀ·E·x1 = 0 pour chaque paire
fn essential(pairs: &[([f64; 2], [f64; 2])]) -> Option<M3> {
    if pairs.len() < 8 {
        return None;
    }
    let mut ata = vec![vec![0.0; 9]; 9];
    for (a, b) in pairs {
        let row = [b[0] * a[0], b[0] * a[1], b[0], b[1] * a[0], b[1] * a[1], b[1], a[0], a[1], 1.0];
        for i in 0..9 {
            for j in 0..9 {
                ata[i][j] += row[i] * row[j];
            }
        }
    }
    let (val, vec) = jacobi(ata);
    let k = (0..9).min_by(|&a, &b| val[a].total_cmp(&val[b]))?;
    let e: Vec<f64> = (0..9).map(|i| vec[i][k]).collect();
    let e = [[e[0], e[1], e[2]], [e[3], e[4], e[5]], [e[6], e[7], e[8]]];
    e.iter().flatten().all(|x| x.is_finite()).then_some(e)
}

fn sampson(e: &M3, a: [f64; 2], b: [f64; 2]) -> f64 {
    let (x1, x2) = ([a[0], a[1], 1.0], [b[0], b[1], 1.0]);
    let ex1 = mv(e, x1);
    let etx2 = mv(&tr(e), x2);
    let n = dot(x2, ex1);
    n * n / (ex1[0] * ex1[0] + ex1[1] * ex1[1] + etx2[0] * etx2[0] + etx2[1] * etx2[1]).max(1e-300)
}

/// Les quatre poses possibles d'une matrice essentielle
fn decompose(e: &M3) -> Vec<Pose> {
    let et = tr(e);
    let ete = mm(&et, e);
    let (val, vec) = jacobi(ete.iter().map(|r| r.to_vec()).collect());
    let mut order = [0usize, 1, 2];
    order.sort_by(|&a, &b| val[b].total_cmp(&val[a]));
    let col = |k: usize| -> V { [vec[0][k], vec[1][k], vec[2][k]] };
    let (v0, v1) = (unit(col(order[0])), unit(col(order[1])));
    let v2 = cross(v0, v1);
    let (u0, u1) = (unit(mv(e, v0)), mv(e, v1));
    // u1 est rendu perpendiculaire à u0 (les données bruitées ne le garantissent pas)
    let u1 = unit(sub(u1, mul(u0, dot(u0, u1))));
    let u2 = cross(u0, u1);
    let u = tr(&[u0, u1, u2]);
    let vt = [v0, v1, v2];
    let w = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    let (r1, r2) = (mm(&mm(&u, &w), &vt), mm(&mm(&u, &tr(&w)), &vt));
    vec![Pose { r: r1, t: u2 }, Pose { r: r1, t: mul(u2, -1.0) }, Pose { r: r2, t: u2 }, Pose { r: r2, t: mul(u2, -1.0) }]
}

/// Ce que rend la résolution
#[derive(Clone, Debug)]
pub struct Solution {
    /// Focale en pixels de l'image suivie
    pub focal: f64,
    /// Pose de la caméra à chaque image (None : image non résolue)
    pub poses: Vec<Option<Pose>>,
    /// Le point de la scène de chaque piste (None : non reconstruit ou écarté)
    pub points: Vec<Option<V>>,
    /// Erreur de reprojection moyenne (pixels) et nombre d'observations gardées
    pub error: f64,
    pub used: usize,
}

struct Problem {
    /// Observations par piste : (image, position normalisée)
    by_track: Vec<Vec<Obs>>,
    by_frame: Vec<Vec<Obs>>,
    poses: Vec<Option<Pose>>,
    points: Vec<Option<V>>,
    /// Seuil d'erreur en coordonnées normalisées (un pixel divisé par la focale)
    px: f64,
}

impl Problem {
    fn views(&self, t: usize) -> Vec<(Pose, [f64; 2])> {
        self.by_track[t].iter().filter_map(|(f, o)| self.poses[*f].map(|p| (p, *o))).collect()
    }

    /// Reconstruit les pistes vues par au moins deux caméras résolues, si l'angle entre les rayons suffit
    fn triangulate_new(&mut self) -> usize {
        let mut n = 0;
        for t in 0..self.by_track.len() {
            if self.points[t].is_some() {
                continue;
            }
            let v = self.views(t);
            if v.len() < 2 {
                continue;
            }
            let ray = |(p, o): &(Pose, [f64; 2])| unit(mv(&tr(&p.r), [o[0], o[1], 1.0]));
            let r0 = ray(&v[0]);
            let par = v.iter().map(|x| dot(r0, ray(x))).fold(1.0f64, f64::min);
            // Moins de 1,5° entre les rayons : la profondeur ne serait pas fiable
            if par > 0.99966 {
                continue;
            }
            let Some(mut x) = triangulate(&v) else { continue };
            refine_point(&mut x, &v, 2.0 * self.px, 5);
            let worst = v.iter().map(|(p, o)| { let r = residual(p, x, *o); r[0].hypot(r[1]) }).fold(0.0f64, f64::max);
            if worst < 4.0 * self.px && v.iter().all(|(p, _)| p.cam(x)[2] > 0.0) {
                self.points[t] = Some(x);
                n += 1;
            }
        }
        n
    }

    fn known(&self, f: usize) -> Vec<(V, [f64; 2])> {
        self.by_frame[f].iter().filter_map(|(t, o)| self.points[*t].map(|x| (x, *o))).collect()
    }

    /// Ajustement d'ensemble, en alternant : chaque point d'après ses caméras, chaque caméra d'après ses points
    fn bundle(&mut self, rounds: usize) {
        for _ in 0..rounds {
            for t in 0..self.points.len() {
                if let Some(mut x) = self.points[t] {
                    let v = self.views(t);
                    if v.len() >= 2 {
                        refine_point(&mut x, &v, 2.0 * self.px, 3);
                        self.points[t] = Some(x);
                    }
                }
            }
            for f in 0..self.poses.len() {
                if let Some(mut p) = self.poses[f] {
                    let k = self.known(f);
                    if k.len() >= 6 {
                        refine_pose(&mut p, &k, 2.0 * self.px, 3);
                        self.poses[f] = Some(p);
                    }
                }
            }
        }
    }

    /// Erreur de reprojection moyenne (en pixels) et nombre d'observations comptées
    fn error(&self) -> (f64, usize) {
        let (mut s, mut n) = (0.0, 0usize);
        for t in 0..self.points.len() {
            let Some(x) = self.points[t] else { continue };
            for (p, o) in self.views(t) {
                let r = residual(&p, x, o);
                s += r[0] * r[0] + r[1] * r[1];
                n += 1;
            }
        }
        if n == 0 { (f64::MAX, 0) } else { ((s / n as f64).sqrt() / self.px, n) }
    }

    /// Écarte les observations dont l'erreur dépasse `limit` pixels, et les points qui n'en gardent pas deux
    fn drop_outliers(&mut self, limit: f64) {
        for t in 0..self.points.len() {
            let Some(x) = self.points[t] else { continue };
            let poses = &self.poses;
            let px = self.px;
            self.by_track[t].retain(|(f, o)| match poses[*f] {
                Some(p) => {
                    let r = residual(&p, x, *o);
                    r[0].hypot(r[1]) < limit * px && p.cam(x)[2] > 0.0
                }
                None => true,
            });
            if self.views(t).len() < 2 {
                self.points[t] = None;
            }
        }
        self.index();
    }

    fn index(&mut self) {
        for l in self.by_frame.iter_mut() {
            l.clear();
        }
        for (t, obs) in self.by_track.iter().enumerate() {
            for (f, o) in obs {
                self.by_frame[*f].push((t, *o));
            }
        }
    }
}

/// Résout la caméra pour une focale donnée (pixels)
fn solve_focal(tracks: &[Track], frames: usize, w: usize, h: usize, focal: f64) -> Result<Solution, String> {
    let (cx, cy) = (w as f64 * 0.5, h as f64 * 0.5);
    let by_track: Vec<Vec<Obs>> = tracks
        .iter()
        .map(|t| t.pts.iter().enumerate().filter(|(k, p)| t.start + k < frames && p[0].is_finite() && p[1].is_finite()).map(|(k, p)| (t.start + k, [(p[0] as f64 - cx) / focal, (p[1] as f64 - cy) / focal])).collect())
        .collect();
    let mut pb = Problem { by_track, by_frame: vec![vec![]; frames], poses: vec![None; frames], points: vec![None; tracks.len()], px: 1.0 / focal };
    pb.index();
    let not_enough = || crate::i18n::t("Pas assez de points suivis ou pas assez de déplacement de la caméra pour retrouver son mouvement.");
    // 1. La paire de départ : beaucoup de points communs, et qui ont bien bougé entre les deux images
    let shared = |a: usize, b: usize| -> Vec<(usize, [f64; 2], [f64; 2])> {
        let mut at: std::collections::HashMap<usize, [f64; 2]> = pb.by_frame[a].iter().copied().collect();
        pb.by_frame[b].iter().filter_map(|(t, o)| at.remove(t).map(|p| (*t, p, *o))).collect()
    };
    let stepf = (frames / 24).max(1);
    let mut best: Option<(f64, usize, usize)> = None;
    for a in (0..frames).step_by(stepf) {
        for b in (a + stepf..frames).step_by(stepf) {
            let s = shared(a, b);
            if s.len() < 12 {
                continue;
            }
            let mut d: Vec<f64> = s.iter().map(|(_, p, q)| (p[0] - q[0]).hypot(p[1] - q[1])).collect();
            d.sort_by(f64::total_cmp);
            let med = d[d.len() / 2];
            let score = (s.len().min(80) as f64) * med.min(0.15);
            if med > 0.01 && best.is_none_or(|x| score > x.0) {
                best = Some((score, a, b));
            }
        }
    }
    let (_, a, b) = best.ok_or_else(not_enough)?;
    let pairs = shared(a, b);
    // 2. Matrice essentielle, par tirages de huit points (les mauvaises pistes sont laissées de côté)
    let thr = (2.0 * pb.px).powi(2);
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut rnd = |n: usize| -> usize {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    let mut best_e: Option<(usize, M3)> = None;
    for _ in 0..400 {
        let mut pick = Vec::with_capacity(8);
        while pick.len() < 8 {
            let i = rnd(pairs.len());
            if !pick.contains(&i) {
                pick.push(i);
            }
        }
        let sample: Vec<_> = pick.iter().map(|&i| (pairs[i].1, pairs[i].2)).collect();
        let Some(e) = essential(&sample) else { continue };
        let n = pairs.iter().filter(|(_, p, q)| sampson(&e, *p, *q) < thr).count();
        if best_e.as_ref().is_none_or(|x| n > x.0) {
            best_e = Some((n, e));
        }
    }
    let (_, e) = best_e.ok_or_else(not_enough)?;
    let inl: Vec<_> = pairs.iter().filter(|(_, p, q)| sampson(&e, *p, *q) < thr).copied().collect();
    let e = essential(&inl.iter().map(|x| (x.1, x.2)).collect::<Vec<_>>()).unwrap_or(e);
    let inl: Vec<_> = pairs.iter().filter(|(_, p, q)| sampson(&e, *p, *q) < thr).copied().collect();
    if inl.len() < 10 {
        return Err(not_enough());
    }
    // 3. Des quatre poses possibles, celle qui met les points devant les deux caméras
    let first = Pose { r: ID, t: [0.0; 3] };
    let second = decompose(&e)
        .into_iter()
        .map(|p| (inl.iter().filter(|(_, x1, x2)| triangulate(&[(first, *x1), (p, *x2)]).is_some()).count(), p))
        .max_by_key(|x| x.0)
        .filter(|x| x.0 * 2 > inl.len())
        .ok_or_else(not_enough)?
        .1;
    pb.poses[a] = Some(first);
    pb.poses[b] = Some(second);
    let mut p2 = second;
    for (t, x1, x2) in &inl {
        if let Some(x) = triangulate(&[(first, *x1), (second, *x2)]) {
            pb.points[*t] = Some(x);
        }
    }
    // La seconde pose est affinée sur ces points, puis les points sur les deux poses
    refine_pose(&mut p2, &pb.known(b), 2.0 * pb.px, 10);
    pb.poses[b] = Some(p2);
    pb.bundle(4);
    if pb.points.iter().flatten().count() < 8 {
        return Err(not_enough());
    }
    // 4. Les autres images, de proche en proche : d'abord entre les deux, puis vers les bouts
    let mut order: Vec<usize> = (a + 1..b).collect();
    let (mut lo, mut hi) = (a as i64 - 1, b + 1);
    while lo >= 0 || hi < frames {
        if hi < frames {
            order.push(hi);
            hi += 1;
        }
        if lo >= 0 {
            order.push(lo as usize);
            lo -= 1;
        }
    }
    let mut pending = order;
    let mut since = 0;
    loop {
        let mut left = vec![];
        let mut progress = false;
        for &f in &pending {
            // La pose de l'image résolue la plus proche sert de départ
            let near = (1..frames).find_map(|d| [f.checked_sub(d), Some(f + d).filter(|x| *x < frames)].into_iter().flatten().find_map(|g| pb.poses[g]));
            let known = pb.known(f);
            let Some(mut p) = near.filter(|_| known.len() >= 8) else {
                left.push(f);
                continue;
            };
            refine_pose(&mut p, &known, 2.0 * pb.px, 15);
            // La pose est-elle crédible ? assez de points devant la caméra et près de leur observation
            let good = known.iter().filter(|(x, o)| { let r = residual(&p, *x, *o); p.cam(*x)[2] > 0.0 && r[0].hypot(r[1]) < 4.0 * pb.px }).count();
            if good < 8 || good * 2 < known.len() {
                left.push(f);
                continue;
            }
            pb.poses[f] = Some(p);
            progress = true;
            pb.triangulate_new();
            since += 1;
            if since >= 12 {
                pb.bundle(2);
                since = 0;
            }
        }
        if left.is_empty() || !progress {
            break;
        }
        pending = left;
    }
    // 5. Ajustement d'ensemble, observations fausses écartées, puis de nouveau
    pb.bundle(8);
    pb.drop_outliers(4.0);
    pb.triangulate_new();
    pb.bundle(8);
    pb.drop_outliers(3.0);
    pb.bundle(4);
    let (error, used) = pb.error();
    let solved = pb.poses.iter().flatten().count();
    if solved < 2 || used < 16 || !error.is_finite() {
        return Err(not_enough());
    }
    Ok(Solution { focal, poses: pb.poses, points: pb.points, error, used })
}

/// Retrouve la caméra et les points de la scène à partir des pistes. `fov` : champ horizontal en
/// degrés s'il est connu ; sinon plusieurs focales sont essayées et la plus cohérente est gardée.
/// La première image résolue devient le repère du monde, et la scène est mise à l'échelle pour que
/// les points soient à cinq unités de la caméra en médiane.
pub fn solve(tracks: &[Track], frames: usize, w: usize, h: usize, fov: Option<f32>) -> Result<Solution, String> {
    if frames < 2 || w < 2 || h < 2 {
        return Err(crate::i18n::t("Pas assez de points suivis ou pas assez de déplacement de la caméra pour retrouver son mouvement."));
    }
    let wf = w as f64;
    let mut sol = match fov.filter(|f| f.is_finite() && *f > 1.0 && *f < 170.0) {
        Some(fov) => solve_focal(tracks, frames, w, h, wf * 0.5 / (fov as f64).to_radians().mul_add(0.5, 0.0).tan())?,
        None => {
            // Le score : l'erreur, alourdie quand peu d'images sont résolues. L'erreur seule distingue mal les
            // focales (une longue focale colle toujours un peu mieux) : les objectifs courants sont un peu favorisés
            let score = |s: &Solution| s.error * (frames as f64 / s.poses.iter().flatten().count().max(1) as f64).powi(2) * (1.0 + 0.35 * (s.focal / wf / 0.9).ln().abs());
            let mut best: Option<Solution> = None;
            let mut last_err = None;
            let mut try_k = |k: f64, best: &mut Option<Solution>| match solve_focal(tracks, frames, w, h, wf * k) {
                Ok(s) => {
                    if best.as_ref().is_none_or(|b| score(&s) < score(b)) {
                        *best = Some(s);
                    }
                }
                Err(e) => last_err = Some(e),
            };
            for k in [0.55, 0.75, 1.0, 1.35, 1.8, 2.4] {
                try_k(k, &mut best);
            }
            // Puis plus fin autour de la meilleure
            if let Some(k0) = best.as_ref().map(|b| b.focal / wf) {
                for k in [k0 * 0.87, k0 * 1.15, k0 * 0.93, k0 * 1.07] {
                    try_k(k, &mut best);
                }
            }
            match best {
                Some(b) => b,
                None => return Err(last_err.unwrap_or_default()),
            }
        }
    };
    // Repère : la première image résolue à l'origine, sans rotation
    let Some(p0) = sol.poses.iter().flatten().next().copied() else { return Err(String::new()) };
    let (r0t, c0) = (tr(&p0.r), p0.centre());
    // Monde' = R0·(X − C0) : la caméra de départ regarde droit devant, à l'origine
    let mut depths: Vec<f64> = sol.points.iter().flatten().map(|x| len(sub(*x, c0))).collect();
    depths.sort_by(f64::total_cmp);
    let k = if depths.is_empty() { 1.0 } else { 5.0 / depths[depths.len() / 2].max(1e-9) };
    for p in sol.poses.iter_mut().flatten() {
        let c = mul(mv(&p0.r, sub(p.centre(), c0)), k);
        let r = mm(&p.r, &r0t);
        *p = Pose { r, t: mul(mv(&r, c), -1.0) };
    }
    for x in sol.points.iter_mut().flatten() {
        *x = mul(mv(&p0.r, sub(*x, c0)), k);
    }
    Ok(sol)
}

// ---------------------------------------------------------------- vers la scène

/// Orientation d'objet (matrice objet → scène) et position d'une pose, dans le repère de la scène
/// (y vers le haut, la caméra regarde vers −z)
pub fn pose_to_scene(p: &Pose) -> ([[f32; 3]; 3], [f32; 3]) {
    // Changement de repère : y et z sont retournés, pour le monde comme pour la caméra
    let d = [1.0, -1.0, -1.0];
    let c = p.centre();
    let mut m = [[0.0f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            // Rotation de vue retournée : d·R·d ; l'objet est son inverse (sa transposée)
            m[j][i] = (d[i] * p.r[i][j] * d[j]) as f32;
        }
    }
    (m, [c[0] as f32, (-c[1]) as f32, (-c[2]) as f32])
}

/// Pose le résultat dans la scène : une caméra animée (une clé par image résolue), jusqu'à
/// `max_points` repères sous un groupe, la vidéo en fond et la durée de la scène. Rend l'indice de la caméra.
pub fn to_scene(scene: &mut crate::scene::Scene, sol: &Solution, tracks: &[Track], size: (usize, usize), video: &str, fps: f32, from: f32, max_points: usize) -> usize {
    use crate::scene::{Backdrop, Key};
    let cam = scene.add_role("caméra suivie", "camera");
    // Champ sur le petit côté de l'image, comme l'attend la vue
    let small = size.0.min(size.1).max(1) as f32;
    scene.objects[cam].fov = crate::model3d::fov_of_focal(sol.focal as f32 / small);
    let mut keys: Vec<Key> = vec![];
    let mut last: Option<[f32; 3]> = None;
    for (f, p) in sol.poses.iter().enumerate() {
        let Some(p) = p else { continue };
        let (m, pos) = pose_to_scene(p);
        let mut rot = crate::scene::rot_euler(m);
        // Les angles restent continus d'une image à l'autre (pas de saut de 360°)
        if let Some(l) = last {
            for i in 0..3 {
                rot[i] += ((l[i] - rot[i]) / 360.0).round() * 360.0;
            }
        }
        last = Some(rot);
        keys.push(Key { frame: f as i32, pos, rot, scale: [1.0; 3], ease: false, interp: "linear".into(), handle: 0, handles: vec![] });
    }
    if let Some(k) = keys.first() {
        (scene.objects[cam].pos, scene.objects[cam].rot) = (k.pos, k.rot);
    }
    scene.objects[cam].keys = keys;
    // Les repères : les pistes les plus longues d'abord, ce sont les plus sûres
    let mut order: Vec<usize> = (0..sol.points.len().min(tracks.len())).filter(|&t| sol.points[t].is_some()).collect();
    order.sort_by_key(|&t| std::cmp::Reverse(tracks[t].pts.len()));
    order.truncate(max_points);
    if !order.is_empty() {
        let group = scene.add_role("repères", "");
        let gname = scene.objects[group].name.clone();
        for t in order {
            let Some(x) = sol.points[t] else { continue };
            let k = scene.add_role("repère", "");
            scene.objects[k].pos = [x[0] as f32, (-x[1]) as f32, (-x[2]) as f32];
            scene.objects[k].parent = gname.clone();
        }
    }
    let frames = sol.poses.len();
    scene.backdrop = Some(Backdrop { file: video.to_string(), fps, from, first: 0, frames: frames as u32, size: [size.0 as u32, size.1 as u32] });
    scene.fps = fps;
    scene.start = 0;
    scene.end = frames.saturating_sub(1) as i32;
    cam
}

// ---------------------------------------------------------------- travail en arrière-plan

/// Résultat d'un suivi de caméra complet sur une vidéo
pub struct Solved {
    pub sol: Solution,
    pub tracks: Vec<Track>,
    /// Taille des images suivies
    pub size: (usize, usize),
    pub video: std::path::PathBuf,
    pub fps: f32,
    pub from: f32,
}

/// Suivi en cours : avancement (0 à 1, étape) et résultat quand il arrive
pub struct Job {
    pub progress: std::sync::Arc<std::sync::Mutex<(f32, &'static str)>>,
    pub result: std::sync::Arc<std::sync::Mutex<Option<Result<Solved, String>>>>,
}

/// Au plus tant d'images suivies : au-delà, le début de la vidéo seulement
pub const MAX_FRAMES: usize = 360;

/// Lance le suivi de caméra d'une vidéo dans un fil : décodage, suivi des points, résolution.
/// `fov` : champ horizontal connu (degrés), sinon il est estimé. `wake` réveille l'interface.
pub fn start(video: std::path::PathBuf, fov: Option<f32>, wake: impl Fn() + Send + Sync + 'static) -> Job {
    let job = Job { progress: std::sync::Arc::new(std::sync::Mutex::new((0.0, "lecture de la vidéo…"))), result: Default::default() };
    let (progress, result) = (job.progress.clone(), job.result.clone());
    std::thread::spawn(move || {
        let set = |v: f32, what: &'static str| {
            if let Ok(mut g) = progress.lock() {
                *g = (v, what);
            }
            wake();
        };
        let run = || -> Result<Solved, String> {
            let info = crate::video::probe(&video)?;
            if info.width == 0 || info.height == 0 {
                return Err(crate::i18n::t("Vidéo illisible."));
            }
            // Au-delà de 30 images par seconde, une image sur deux suffit au suivi
            let fps = if info.fps > 31.0 { info.fps / (info.fps / 30.0).round().max(1.0) } else { info.fps.max(1.0) };
            let len = if info.duration > 0.0 { info.duration.min(MAX_FRAMES as f32 / fps) } else { MAX_FRAMES as f32 / fps };
            let w = 640usize.min(info.width as usize / 2 * 2).max(64);
            let mut fr = crate::track2d::decode(&video, 0.0, len, fps, w, (info.width as f32, info.height as f32))?;
            fr.px.truncate(MAX_FRAMES);
            if fr.px.len() < 8 {
                return Err(crate::i18n::t("La vidéo est trop courte pour un suivi de caméra."));
            }
            set(0.05, "suivi des points…");
            let tracks = track_features(&fr, 220, &|v| set(0.05 + v * 0.6, "suivi des points…"));
            set(0.7, "calcul de la caméra…");
            let sol = solve(&tracks, fr.px.len(), fr.w, fr.h, fov)?;
            Ok(Solved { sol, tracks, size: (fr.w, fr.h), video: video.clone(), fps, from: 0.0 })
        };
        let r = run();
        if let Ok(mut g) = result.lock() {
            *g = Some(r);
        }
        wake();
    });
    job
}

/// Les images de la vidéo de fond, en petit (RVB), pour l'aperçu dans la vue
pub struct BackdropFrames {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Vec<u8>>,
}

pub type BackdropSlot = std::sync::Arc<std::sync::Mutex<Option<Result<std::sync::Arc<BackdropFrames>, String>>>>;

/// Taille d'aperçu : 480 pixels de large au plus, moins si le passage est long (environ 150 Mo au total)
pub fn backdrop_size(src: (u32, u32), frames: usize) -> (usize, usize) {
    let (sw, sh) = (src.0.max(2) as f32, src.1.max(2) as f32);
    let budget = 150.0e6 / (frames.max(1) as f32 * 3.0);
    let w = (budget * sw / sh).sqrt().min(480.0).min(sw).max(32.0);
    let w = (w as usize / 2 * 2).max(2);
    let h = (((w as f32 * sh / sw) / 2.0).round() as usize * 2).max(2);
    (w, h)
}

/// Décode les images de fond dans un fil
pub fn load_backdrop(video: std::path::PathBuf, b: crate::scene::Backdrop, wake: impl Fn() + Send + 'static) -> BackdropSlot {
    let slot: BackdropSlot = Default::default();
    let out = slot.clone();
    std::thread::spawn(move || {
        let run = || -> Result<std::sync::Arc<BackdropFrames>, String> {
            let ff = crate::ffmpeg::find("ffmpeg").ok_or_else(|| crate::i18n::t("ffmpeg n'est pas installé."))?;
            let src = if b.size[0] > 0 && b.size[1] > 0 { (b.size[0], b.size[1]) } else { crate::video::probe(&video).map(|i| (i.width, i.height))? };
            let n = (b.frames.max(1) as usize).min(MAX_FRAMES * 2);
            let (w, h) = backdrop_size(src, n);
            let o = crate::ffmpeg::command(ff)
                .args(["-v", "error", "-ss", &format!("{:.3}", b.from.max(0.0)), "-i"])
                .arg(&video)
                .args(["-vf", &format!("fps={},scale={w}:{h}", b.fps.max(1.0)), "-frames:v", &n.to_string(), "-pix_fmt", "rgb24", "-f", "rawvideo", "-"])
                .output()
                .map_err(|e| e.to_string())?;
            let px: Vec<Vec<u8>> = o.stdout.chunks_exact(w * h * 3).map(|c| c.to_vec()).collect();
            if px.is_empty() {
                return Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("ffmpeg").to_string());
            }
            Ok(std::sync::Arc::new(BackdropFrames { w, h, px }))
        };
        let r = run();
        if let Ok(mut g) = out.lock() {
            *g = Some(r);
        }
        wake();
    });
    slot
}

/// Met l'image de fond derrière le rendu : partout où rien n'a été dessiné (`ids` vaut u32::MAX),
/// dans le cadre (x, y, largeur, hauteur) en pixels du rendu.
pub fn composite(rgba: &mut [u8], ids: &[u32], w: usize, h: usize, frame: (f32, f32, f32, f32), img: &[u8], iw: usize, ih: usize) {
    let (fx, fy, fw, fh) = frame;
    if !(fw > 0.5 && fh > 0.5) || iw == 0 || ih == 0 || img.len() < iw * ih * 3 || rgba.len() < w * h * 4 || ids.len() < w * h {
        return;
    }
    let (x0, y0) = (fx.max(0.0).ceil() as usize, fy.max(0.0).ceil() as usize);
    let (x1, y1) = (((fx + fw).floor().max(0.0) as usize).min(w), ((fy + fh).floor().max(0.0) as usize).min(h));
    for y in y0..y1 {
        let v = (((y as f32 + 0.5 - fy) / fh) * ih as f32) as usize;
        let row = v.min(ih - 1) * iw;
        for x in x0..x1 {
            let i = y * w + x;
            if ids[i] != u32::MAX {
                continue;
            }
            let u = ((((x as f32 + 0.5 - fx) / fw) * iw as f32) as usize).min(iw - 1);
            let s = (row + u) * 3;
            rgba[i * 4..i * 4 + 3].copy_from_slice(&img[s..s + 3]);
            rgba[i * 4 + 3] = 255;
        }
    }
}

/// Le cadre de la caméra dans une vue de `w` × `h` : centré, au format `aspect` (largeur / hauteur)
pub fn camera_frame(w: f32, h: f32, aspect: f32) -> (f32, f32, f32, f32) {
    let a = if aspect.is_finite() && aspect > 0.05 { aspect } else { 16.0 / 9.0 };
    let fh = h.min(w / a);
    let fw = fh * a;
    ((w - fw) * 0.5, (h - fh) * 0.5, fw, fh)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une caméra qui avance en travelling devant un nuage de points ; rend (pistes, centres vrais)
    fn synthetic(frames: usize, focal: f64, w: usize, h: usize, noise: f64) -> (Vec<Track>, Vec<V>, Vec<Pose>) {
        let mut seed = 12345u64;
        let mut rnd = move || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) as f64 / (1u64 << 31) as f64) - 1.0
        };
        let pts: Vec<V> = (0..320).map(|_| [2.0 + rnd() * 11.0, rnd() * 4.0, 9.0 + rnd() * 4.0]).collect();
        let poses: Vec<Pose> = (0..frames)
            .map(|f| {
                let u = f as f64;
                let c = [u * 0.05, 0.3 * (u * 0.1).sin(), u * 0.02];
                let r = exp_rot([0.002 * u, -0.004 * u, 0.003 * u]);
                Pose { r, t: mul(mv(&r, c), -1.0) }
            })
            .collect();
        let mut tracks = vec![];
        for x in &pts {
            let mut cur: Option<Track> = None;
            for (f, p) in poses.iter().enumerate() {
                let c = p.cam(*x);
                let (u, v) = (focal * c[0] / c[2] + w as f64 / 2.0 + rnd() * noise, focal * c[1] / c[2] + h as f64 / 2.0 + rnd() * noise);
                let inside = c[2] > 0.1 && u > 4.0 && v > 4.0 && u < w as f64 - 4.0 && v < h as f64 - 4.0;
                match (&mut cur, inside) {
                    (Some(t), true) => t.pts.push([u as f32, v as f32]),
                    (None, true) => cur = Some(Track { start: f, pts: vec![[u as f32, v as f32]] }),
                    (Some(_), false) => tracks.extend(cur.take()),
                    _ => {}
                }
            }
            tracks.extend(cur);
        }
        (tracks, pts, poses)
    }

    #[test]
    fn algebre() {
        // Vecteurs propres : A·v = λ·v
        let a = vec![vec![4.0, 1.0, 0.5], vec![1.0, 3.0, 0.2], vec![0.5, 0.2, 1.0]];
        let (val, vec) = jacobi(a.clone());
        for k in 0..3 {
            for i in 0..3 {
                let av: f64 = (0..3).map(|j| a[i][j] * vec[j][k]).sum();
                assert!((av - val[k] * vec[i][k]).abs() < 1e-9);
            }
        }
        let x = solve_lin(vec![vec![2.0, 1.0], vec![1.0, 3.0]], vec![5.0, 10.0]).unwrap();
        assert!((x[0] - 1.0).abs() < 1e-12 && (x[1] - 3.0).abs() < 1e-12);
        assert!(solve_lin(vec![vec![1.0, 2.0], vec![2.0, 4.0]], vec![1.0, 2.0]).is_none());
        // Une rotation reste une rotation
        let r = exp_rot([0.3, -0.2, 0.5]);
        let rrt = mm(&r, &tr(&r));
        assert!((0..3).all(|i| (0..3).all(|j| (rrt[i][j] - ID[i][j]).abs() < 1e-12)));
        // Triangulation exacte de deux vues
        let p1 = Pose { r: ID, t: [0.0; 3] };
        let p2 = Pose { r: exp_rot([0.0, -0.1, 0.0]), t: [-1.0, 0.0, 0.1] };
        let x = [0.4, -0.3, 6.0];
        let o = |p: &Pose| { let c = p.cam(x); [c[0] / c[2], c[1] / c[2]] };
        let t = triangulate(&[(p1, o(&p1)), (p2, o(&p2))]).unwrap();
        assert!(len(sub(t, x)) < 1e-9);
        // Derrière la caméra : refusé
        assert!(triangulate(&[(p1, [0.0, 0.0]), (Pose { r: ID, t: [1.0, 0.0, 0.0] }, [-0.1, 0.0])]).is_none());
    }

    #[test]
    fn camera_retrouvee() {
        let (w, h, focal) = (960usize, 540usize, 800.0);
        let (tracks, _, truth) = synthetic(48, focal, w, h, 0.3);
        let fov = (2.0 * (w as f64 * 0.5 / focal).atan()).to_degrees() as f32;
        let sol = solve(&tracks, 48, w, h, Some(fov)).unwrap();
        assert!(sol.poses.iter().all(|p| p.is_some()), "toutes les images sont résolues");
        assert!(sol.error < 0.6, "erreur {}", sol.error);
        let npts = sol.points.iter().flatten().count();
        assert!(npts * 10 > tracks.len() * 7, "{npts} points pour {} pistes, erreur {}", tracks.len(), sol.error);
        // Même trajectoire que la vraie, à l'échelle près (les deux partent de l'origine)
        let est: Vec<V> = sol.poses.iter().map(|p| p.unwrap().centre()).collect();
        let tru: Vec<V> = truth.iter().map(|p| p.centre()).collect();
        let k = len(tru[47]) / len(est[47]);
        let worst = (0..48).map(|f| len(sub(mul(est[f], k), tru[f]))).fold(0.0f64, f64::max);
        assert!(worst < 0.05 * len(tru[47]), "écart {worst} sur {}", len(tru[47]));
        // Les rotations aussi
        let dr = mm(&sol.poses[47].unwrap().r, &tr(&truth[47].r));
        assert!(dr[0][0] > 0.9995 && dr[1][1] > 0.9995, "{dr:?}");
        // Échelle : les points sont à cinq unités en médiane
        let mut d: Vec<f64> = sol.points.iter().flatten().map(|x| len(*x)).collect();
        d.sort_by(f64::total_cmp);
        assert!((d[d.len() / 2] - 5.0).abs() < 1e-6);
        // Pas assez de données : une erreur, pas de panique
        assert!(solve(&tracks[..5], 48, w, h, Some(fov)).is_err());
        assert!(solve(&[], 48, w, h, None).is_err());
        assert!(solve(&tracks, 1, w, h, None).is_err());
        // Caméra immobile : rien à trianguler
        let still: Vec<Track> = (0..60).map(|i| Track { start: 0, pts: vec![[100.0 + (i % 10) as f32 * 70.0, 80.0 + (i / 10) as f32 * 60.0]; 20] }).collect();
        assert!(solve(&still, 20, w, h, Some(fov)).is_err());
    }

    #[test]
    fn focale_estimee_et_pistes_fausses() {
        let (w, h, focal) = (960usize, 540usize, 800.0);
        let (mut tracks, _, _) = synthetic(40, focal, w, h, 0.2);
        // Quelques pistes fausses (un reflet qui glisse) : elles sont écartées
        for k in 0..8 {
            tracks.push(Track { start: 0, pts: (0..40).map(|f| [200.0 + k as f32 * 60.0 + f as f32 * 3.0, 300.0 - f as f32 * 2.0]).collect() });
        }
        let sol = solve(&tracks, 40, w, h, None).unwrap();
        assert!(sol.error < 1.0, "erreur {}", sol.error);
        assert!((sol.focal / focal - 1.0).abs() < 0.25, "focale {}", sol.focal);
        let n = tracks.len();
        assert!(sol.points[n - 8..].iter().filter(|p| p.is_some()).count() <= 2, "les pistes fausses ne donnent pas de points");
    }

    #[test]
    fn vers_la_scene() {
        // Caméra à l'origine sans rotation : regarde vers −z dans la scène, y en haut
        let (m, pos) = pose_to_scene(&Pose { r: ID, t: [0.0; 3] });
        assert_eq!(pos, [0.0; 3]);
        assert!((0..3).all(|i| (m[i][i] - 1.0).abs() < 1e-6));
        // Caméra déplacée vers la droite, vers le haut (y négatif en image) et vers l'avant
        let p = Pose { r: ID, t: [-1.0, 2.0, -3.0] };
        assert_eq!(pose_to_scene(&p).1, [1.0, 2.0, -3.0]);
        let (w, h) = (960usize, 540usize);
        let (tracks, _, _) = synthetic(30, 800.0, w, h, 0.2);
        let sol = solve(&tracks, 30, w, h, Some(61.9)).unwrap();
        let mut sc = crate::scene::Scene::default();
        let cam = to_scene(&mut sc, &sol, &tracks, (w, h), "plan.mp4", 24.0, 1.5, 20);
        let o = &sc.objects[cam];
        assert_eq!(crate::scene::kind(o), crate::scene::Kind::Camera);
        assert_eq!(o.keys.len(), 30);
        assert!((o.fov - 37.3).abs() < 1.0, "champ vertical {}", o.fov);
        assert_eq!(sc.objects.iter().filter(|o| o.name.starts_with("repère") && !o.parent.is_empty()).count(), 20);
        assert_eq!((sc.start, sc.end), (0, 29));
        let b = sc.backdrop.as_ref().unwrap();
        assert_eq!((b.frame_at(0.0), b.frame_at(29.0), b.frame_at(30.0), b.frame_at(-1.0)), (Some(0), Some(29), None, None));
        // La caméra avance vers +x (travelling à droite) et les repères sont devant elle (z négatif)
        assert!(o.keys[29].pos[0] > o.keys[0].pos[0] + 0.1);
        assert!(sc.objects.iter().filter(|o| o.name.starts_with("repère ") || o.name.starts_with("repère") && !o.parent.is_empty()).all(|o| o.pos[2] < 0.0));
        assert!((sc.frame_aspect() - 16.0 / 9.0).abs() < 1e-3);
        // Les angles ne sautent pas d'une image à l'autre
        assert!(o.keys.windows(2).all(|k| (0..3).all(|i| (k[1].rot[i] - k[0].rot[i]).abs() < 5.0)));
    }

    #[test]
    fn coins_et_suivi() {
        // Une image de taches nettes qui glisse de (1,5 ; −0,75) pixel par image
        let (w, h) = (160usize, 120usize);
        let tex = |x: f32, y: f32| -> f32 {
            let mut v = 0.0;
            for k in 0..40 {
                let (cx, cy) = ((k * 37 % 200) as f32 - 20.0, (k * 53 % 150) as f32 - 15.0);
                let d2 = (x - cx).powi(2) + (y - cy).powi(2);
                v += (if k % 2 == 0 { 90.0 } else { -70.0 }) * (-d2 / 18.0).exp();
            }
            (128.0 + v).clamp(0.0, 255.0)
        };
        let px: Vec<Vec<u8>> = (0..12).map(|f| (0..w * h).map(|i| tex((i % w) as f32 - 1.5 * f as f32, (i / w) as f32 + 0.75 * f as f32) as u8).collect()).collect();
        let fr = Frames { w, h, px };
        let corners = detect(&fr.px[0], w, h, 30, 10.0, &[]);
        assert!(corners.len() >= 8, "{}", corners.len());
        // Écartés les uns des autres, et loin des points à éviter
        assert!(corners.iter().enumerate().all(|(i, a)| corners[..i].iter().all(|b| (a[0] - b[0]).hypot(a[1] - b[1]) >= 10.0)));
        let again = detect(&fr.px[0], w, h, 30, 10.0, &corners);
        assert!(again.iter().all(|a| corners.iter().all(|b| (a[0] - b[0]).hypot(a[1] - b[1]) >= 10.0)));
        let tracks = track_features(&fr, 30, &|_| {});
        let full: Vec<&Track> = tracks.iter().filter(|t| t.start == 0 && t.pts.len() == 12).collect();
        assert!(full.len() >= 5, "{} pistes complètes sur {}", full.len(), tracks.len());
        for t in &full {
            let (dx, dy) = (t.pts[11][0] - t.pts[0][0], t.pts[11][1] - t.pts[0][1]);
            assert!((dx - 16.5).abs() < 0.5 && (dy + 8.25).abs() < 0.5, "{dx} {dy}");
        }
        // Image vide ou unie : aucune piste, pas de panique
        assert!(track_features(&Frames { w, h, px: vec![vec![128; w * h]; 4] }, 30, &|_| {}).is_empty());
        assert!(track_features(&Frames { w: 0, h: 0, px: vec![] }, 30, &|_| {}).is_empty());
        assert!(detect(&[], 0, 0, 10, 5.0, &[]).is_empty());
    }

    #[test]
    fn fond_derriere_le_rendu() {
        // Vue 8 × 6, cadre 16:9 centré : 8 × 4,5 → lignes 1 à 4 entières
        let (x, y, fw, fh) = camera_frame(8.0, 6.0, 16.0 / 9.0);
        assert!((x, fw) == (0.0, 8.0) && (y - 0.75).abs() < 1e-6 && (fh - 4.5).abs() < 1e-6);
        assert_eq!(camera_frame(4.0, 8.0, f32::NAN).2, 4.0);
        let mut rgba = vec![9u8; 8 * 6 * 4];
        let mut ids = vec![u32::MAX; 8 * 6];
        ids[2 * 8 + 3] = 7; // un triangle dessiné là : le fond ne le recouvre pas
        // Fond 2 × 2 : rouge, vert / bleu, blanc
        let img = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
        composite(&mut rgba, &ids, 8, 6, (x, y, fw, fh), &img, 2, 2);
        let px = |x: usize, y: usize| &rgba[(y * 8 + x) * 4..(y * 8 + x) * 4 + 3];
        assert_eq!(px(0, 1), [255, 0, 0]);
        assert_eq!(px(7, 1), [0, 255, 0]);
        assert_eq!(px(0, 4), [0, 0, 255]);
        assert_eq!(px(7, 4), [255, 255, 255]);
        assert_eq!(px(3, 2), [9, 9, 9], "l'objet reste devant");
        assert_eq!(px(0, 0), [9, 9, 9], "hors du cadre : rien");
        // Entrées incohérentes : rien ne panique
        composite(&mut rgba, &ids, 8, 6, (0.0, 0.0, 8.0, 6.0), &img[..5], 2, 2);
        composite(&mut rgba, &ids[..3], 8, 6, (0.0, 0.0, 8.0, 6.0), &img, 2, 2);
        composite(&mut rgba, &ids, 8, 6, (-50.0, -50.0, 1000.0, 1000.0), &img, 2, 2);
        // Aperçu : jamais plus de 480 de large, pair, et plus petit pour un long passage
        assert_eq!(backdrop_size((1920, 1080), 100), (480, 270));
        let (w, h) = backdrop_size((1920, 1080), 700);
        assert!(w < 480 && w % 2 == 0 && h % 2 == 0 && w * h * 3 * 700 < 160_000_000);
        assert_eq!(backdrop_size((0, 0), 10).0 % 2, 0);
    }

    /// Des points lumineux dans l'espace, filmés par une caméra en travelling : (images, poses vraies, focale)
    fn blob_film() -> (Frames, Vec<Pose>, f64) {
        let (w, h, focal, n) = (320usize, 240usize, 300.0f64, 20usize);
        let mut seed = 99u64;
        let mut rnd = move || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) as f64 / (1u64 << 31) as f64) - 1.0
        };
        let pts: Vec<(V, f64)> = (0..350).map(|_| ([1.0 + rnd() * 7.0, rnd() * 4.5, 8.0 + rnd() * 4.0], 60.0 + rnd().abs() * 120.0)).collect();
        let poses: Vec<Pose> = (0..n).map(|f| { let r = exp_rot([0.0, -0.003 * f as f64, 0.002 * f as f64]); Pose { r, t: mul(mv(&r, [0.09 * f as f64, 0.01 * f as f64, 0.0]), -1.0) } }).collect();
        let px: Vec<Vec<u8>> = poses
            .iter()
            .map(|p| {
                let mut img = vec![30.0f32; w * h];
                for (x, a) in &pts {
                    let c = p.cam(*x);
                    let (u, v) = (focal * c[0] / c[2] + w as f64 / 2.0, focal * c[1] / c[2] + h as f64 / 2.0);
                    for yy in (v as i64 - 6).max(0)..(v as i64 + 7).min(h as i64) {
                        for xx in (u as i64 - 6).max(0)..(u as i64 + 7).min(w as i64) {
                            let d2 = (xx as f64 - u).powi(2) + (yy as f64 - v).powi(2);
                            img[yy as usize * w + xx as usize] += (*a * (-d2 / 5.0).exp()) as f32;
                        }
                    }
                }
                img.iter().map(|v| v.min(255.0) as u8).collect()
            })
            .collect();
        (Frames { w, h, px }, poses, focal)
    }

    #[test]
    fn de_la_video_a_la_camera() {
        // Images, suivi, résolution
        let (fr, poses, focal) = blob_film();
        let (w, h, n) = (fr.w, fr.h, fr.px.len());
        let tracks = track_features(&fr, 120, &|_| {});
        assert!(tracks.iter().filter(|t| t.pts.len() >= 10).count() > 25, "{} pistes", tracks.len());
        let fov = (2.0 * (w as f64 * 0.5 / focal).atan()).to_degrees() as f32;
        let sol = solve(&tracks, n, w, h, Some(fov)).unwrap();
        assert!(sol.poses.iter().all(|p| p.is_some()) && sol.error < 1.0, "erreur {}", sol.error);
        // La trajectoire retrouvée est la vraie, à l'échelle près
        let est: Vec<V> = sol.poses.iter().map(|p| p.unwrap().centre()).collect();
        let tru: Vec<V> = poses.iter().map(|p| p.centre()).collect();
        let k = len(tru[n - 1]) / len(est[n - 1]);
        let worst = (0..n).map(|f| len(sub(mul(est[f], k), tru[f]))).fold(0.0f64, f64::max);
        assert!(worst < 0.1 * len(tru[n - 1]), "écart {worst} sur {}", len(tru[n - 1]));
    }
}
