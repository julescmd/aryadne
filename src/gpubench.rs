//! Banc de mesure de la carte graphique : ce que la machine fait vraiment, avant d'écrire les
//! solveurs de simulation. Les rapports de recherche montrent que la simulation est bornée par la
//! mémoire : on mesure donc d'abord le débit réel (copie, puis itération de pression à 7 points),
//! puis un pas de fumée simple (advection semi-lagrangienne, flottabilité, pression par Jacobi).
//!
//! Puis le tissu en VBD (`gpucloth`) sur 250 000 et 1 million de sommets : temps, octets et
//! lancements par passe, une image au budget de 16 et de 32 passes sans et avec niveaux grossiers,
//! le surcoût de l'auto-collision, contre le processeur. Enfin la raideur obtenue et la chute sur
//! une boule à 16, 32 et 64 passes par image : XPBD, VBD, VBD avec niveaux grossiers.
//!
//! Lancement : menu Aide, « Mesurer la carte graphique », ou `aryadne --banc-gpu` dans un terminal.

use eframe::egui;
use std::sync::Mutex;
use std::time::Instant;

const SHADER: &str = r#"
struct P { n: u32, iters: u32, dt: f32, pad: u32 }
@group(0) @binding(0) var<uniform> p: P;
@group(0) @binding(1) var<storage, read> a: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> b: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> sa: array<f32>;
@group(0) @binding(4) var<storage, read_write> sb: array<f32>;
@group(0) @binding(5) var<storage, read> sc: array<f32>;

fn lin(gid: vec3<u32>, nwg: vec3<u32>) -> u32 { return gid.x + gid.y * nwg.x * 256u; }

// Copie : lit et écrit 16 octets par élément
@compute @workgroup_size(256)
fn copy(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i < arrayLength(&b)) { b[i] = a[i]; }
}

fn idx(x: u32, y: u32, z: u32) -> u32 { return x + p.n * (y + p.n * z); }

// Une itération de Jacobi pour la pression : sb = (somme des 6 voisins - sc) / 6, bords fermés
@compute @workgroup_size(8, 8, 4)
fn jacobi(@builtin(global_invocation_id) g: vec3<u32>) {
    let n = p.n;
    if (g.x >= n || g.y >= n || g.z >= n) { return; }
    let i = idx(g.x, g.y, g.z);
    let c = sa[i];
    let xm = select(c, sa[i - 1u], g.x > 0u);
    let xp = select(c, sa[i + 1u], g.x + 1u < n);
    let ym = select(c, sa[i - n], g.y > 0u);
    let yp = select(c, sa[i + n], g.y + 1u < n);
    let zm = select(c, sa[i - n * n], g.z > 0u);
    let zp = select(c, sa[i + n * n], g.z + 1u < n);
    sb[i] = (xm + xp + ym + yp + zm + zp - sc[i]) / 6.0;
}

fn fetch(x: i32, y: i32, z: i32) -> vec4<f32> {
    let m = i32(p.n) - 1;
    return a[idx(u32(clamp(x, 0, m)), u32(clamp(y, 0, m)), u32(clamp(z, 0, m)))];
}

// Advection semi-lagrangienne (vitesse xyz + densité w), puis flottabilité : lecture trilinéaire, 8 voisins
@compute @workgroup_size(8, 8, 4)
fn advect(@builtin(global_invocation_id) g: vec3<u32>) {
    let n = p.n;
    if (g.x >= n || g.y >= n || g.z >= n) { return; }
    let here = a[idx(g.x, g.y, g.z)];
    let q = vec3<f32>(g) - here.xyz * p.dt;
    let f = floor(q);
    let t = q - f;
    let i = vec3<i32>(f);
    let c00 = mix(fetch(i.x, i.y, i.z), fetch(i.x + 1, i.y, i.z), t.x);
    let c10 = mix(fetch(i.x, i.y + 1, i.z), fetch(i.x + 1, i.y + 1, i.z), t.x);
    let c01 = mix(fetch(i.x, i.y, i.z + 1), fetch(i.x + 1, i.y, i.z + 1), t.x);
    let c11 = mix(fetch(i.x, i.y + 1, i.z + 1), fetch(i.x + 1, i.y + 1, i.z + 1), t.x);
    var v = mix(mix(c00, c10, t.y), mix(c01, c11, t.y), t.z);
    v.y = v.y + v.w * 0.5 * p.dt;
    b[idx(g.x, g.y, g.z)] = v;
}

// Divergence de la vitesse (vers sb)
@compute @workgroup_size(8, 8, 4)
fn divergence(@builtin(global_invocation_id) g: vec3<u32>) {
    let n = p.n;
    if (g.x >= n || g.y >= n || g.z >= n) { return; }
    let x = i32(g.x); let y = i32(g.y); let z = i32(g.z);
    let d = (fetch(x + 1, y, z).x - fetch(x - 1, y, z).x + fetch(x, y + 1, z).y - fetch(x, y - 1, z).y + fetch(x, y, z + 1).z - fetch(x, y, z - 1).z) * 0.5;
    sb[idx(g.x, g.y, g.z)] = d;
}

fn pr(x: i32, y: i32, z: i32) -> f32 {
    let m = i32(p.n) - 1;
    return sa[idx(u32(clamp(x, 0, m)), u32(clamp(y, 0, m)), u32(clamp(z, 0, m)))];
}

// Projection : on retire le gradient de pression (a = vitesse, sa = pression, b = résultat)
@compute @workgroup_size(8, 8, 4)
fn project(@builtin(global_invocation_id) g: vec3<u32>) {
    let n = p.n;
    if (g.x >= n || g.y >= n || g.z >= n) { return; }
    let x = i32(g.x); let y = i32(g.y); let z = i32(g.z);
    let grad = vec3<f32>(pr(x + 1, y, z) - pr(x - 1, y, z), pr(x, y + 1, z) - pr(x, y - 1, z), pr(x, y, z + 1) - pr(x, y, z - 1)) * 0.5;
    let i = idx(g.x, g.y, g.z);
    b[i] = vec4<f32>(a[i].xyz - grad, a[i].w);
}
"#;

/// Une mesure : nom, temps par passe, octets déplacés par passe
#[derive(Clone, Debug)]
pub struct Mesure {
    /// « copie », « pression » ou « fumée »
    pub genre: &'static str,
    pub nom: String,
    pub ms: f64,
    pub octets: f64,
}

impl Mesure {
    pub fn debit(&self) -> f64 {
        self.octets / (self.ms / 1000.0) / 1e9
    }
}

#[derive(Clone, Debug, Default)]
pub struct Rapport {
    pub carte: String,
    pub logicielle: bool,
    pub mesures: Vec<Mesure>,
    /// Écart maximal entre la carte et le processeur sur une itération de Jacobi (contrôle)
    pub ecart: f32,
    pub notes: Vec<String>,
    /// Tissu en VBD, par taille
    pub tissu: Vec<Tissu>,
    /// Écart maximal entre la carte et le processeur sur un petit tissu, en mètres (contrôle)
    pub ecart_tissu: Option<f32>,
    /// VBD contre XPBD à budget égal
    pub comparaison: Option<crate::gpucloth::Comparison>,
    /// Fumée du solveur des nodes (pression par multigrille), par taille
    pub fumee: Vec<Fumee>,
}

/// Mesure du solveur de fumée des nodes sur une grille cubique : un pas complet (advection
/// MacCormack, sources, forces, pression MGPCG, projection).
#[derive(Clone, Debug)]
pub struct Fumee {
    pub cote: usize,
    /// Calculé sur la carte (sinon : elle n'y tient pas, mesuré sur le processeur)
    pub carte: bool,
    /// Par nombre d'itérations de pression : temps d'un pas (ms), plus grande divergence avant
    /// et après la pression (par seconde)
    pub par_iterations: Vec<(usize, f64, f32, f32)>,
    /// Un pas à 4 itérations avec le transfert de l'état vers le processeur, pour le cache
    pub avec_cache_ms: f64,
    /// Mémoire tenue sur la carte, en octets
    pub octets: u64,
}

/// Mesure la fumée des nodes sur une grille de côté `cote` : une bouffée chaude qui monte.
fn fumee(cote: usize, pas: usize) -> Fumee {
    use crate::smoke::{Emit, Op, Params, Shape, Sim};
    let res = [cote; 3];
    let h = 1.0 / cote as f32;
    let ops = vec![Op::Emit(Emit { shape: Shape::Ball([0.5, 0.2, 0.5], 0.1), density: 1.0, temperature: 1.0, fuel: 0.0, velocity: [0.0; 3], add: false, soft: 0.02, noise: 0.5, seed: 1, dt: 1.0 / 24.0 })];
    let mut sim = Sim::new(res, [0.0; 3], h, true);
    let mut out = Fumee { cote, carte: sim.on_gpu(), par_iterations: vec![], avec_cache_ms: 0.0, octets: sim.gpu_bytes() };
    // Quelques pas pour que la fumée soit en route, puis l'état de départ commun à toutes les mesures
    let warm = Params::default();
    for _ in 0..pas.max(2) {
        sim.step(1.0 / 24.0, &warm, &ops);
    }
    let start = sim.shot().grids();
    for iters in [2usize, 4, 8] {
        let par = Params { iterations: iters, ..Default::default() };
        sim.load(&start);
        sim.wait();
        let t0 = Instant::now();
        for _ in 0..pas {
            sim.step(1.0 / 24.0, &par, &ops);
        }
        let (st, _) = sim.stats_now();
        out.par_iterations.push((iters, t0.elapsed().as_secs_f64() * 1000.0 / pas as f64, st.before, st.after));
    }
    // Avec le transfert de chaque image vers le processeur (ce que demande le cache)
    let par = Params::default();
    sim.load(&start);
    sim.wait();
    let t0 = Instant::now();
    for _ in 0..pas {
        sim.step(1.0 / 24.0, &par, &ops);
        let _ = sim.shot().grids();
    }
    out.avec_cache_ms = t0.elapsed().as_secs_f64() * 1000.0 / pas as f64;
    out
}

/// Mesure d'un tissu carré : une « passe » met à jour tous les sommets une fois (toutes les couleurs).
#[derive(Clone, Debug)]
pub struct Tissu {
    pub points: usize,
    pub couleurs: usize,
    /// Lancements de noyaux par passe, dans la mesure d'une passe
    pub lancements: f64,
    /// Octets par passe du même tissu avant le rangement compact
    pub octets_avant: f64,
    /// Niveaux grossiers, et coût d'une correction en passes du niveau fin
    pub niveaux: usize,
    pub correction: f64,
    /// Des images entières, à budget donné
    pub images: Vec<Image>,
    /// Calculé sur la carte (sinon le tissu n'y tient pas, ou il n'y a pas de carte)
    pub carte: bool,
    /// Préparation au chargement : voisinages, couleurs, rangement
    pub preparation_ms: f64,
    pub passe_ms: f64,
    /// Relecture des positions et des vitesses
    pub relecture_ms: f64,
    pub passe_processeur_ms: f64,
    /// Octets lus et écrits par passe, et mémoire gardée sur la carte
    pub octets: f64,
    pub memoire: u64,
    /// Une image affichée, de trois façons (seulement sur la carte)
    pub affichage: Option<Affichage>,
}

/// Le temps d'une image **affichée** : un pas au budget de 16 passes avec niveaux grossiers, puis
/// de quoi la montrer. C'est ce que l'artiste attend entre deux images de l'aperçu.
#[derive(Clone, Debug, Default)]
pub struct Affichage {
    /// Pas, puis dessin direct depuis les tampons du solveur (1280 × 720). Rien ne quitte la carte.
    pub direct_ms: f64,
    /// Comme l'aperçu le fait : en plus, une relecture est demandée pour le cache des images, mais
    /// jamais attendue ni copiée
    pub cliche_ms: f64,
    /// Comme avant : pas, puis relecture attendue et copiée vers le processeur (le dessin
    /// logiciel qui suivait n'est pas compté)
    pub relu_ms: f64,
    /// Le dessin seul : normales et rasterisation
    pub dessin_ms: f64,
    /// Comme `direct_ms`, avec tout ce qu'a un objet de la scène : ombres de la lumière principale
    /// (reçues, avec une boule et un sol comme autres objets, et posées au sol), matériau en
    /// textures cuites de 1024², image des identifiants et contour de sélection
    pub complet_ms: f64,
    /// Le dessin complet seul
    pub dessin_complet_ms: f64,
}

/// Une image de tissu au budget de `budget` passes, découpée comme le solveur le conseille.
#[derive(Clone, Debug)]
pub struct Image {
    pub budget: usize,
    pub niveaux: bool,
    pub auto_collision: bool,
    pub sous_pas: usize,
    pub lancements: usize,
    pub ms: f64,
}

/// Passes par image de la mesure d'une passe : 4 sous-pas de 8 passes, sans rien d'autre
const PASSES: usize = 32;

fn tissu(cote: usize, images: usize) -> Tissu {
    use crate::gpucloth::{grid, rest_lengths, split_budget, Cloth, Params, Topo};
    let n = cote * cote;
    let (p, e) = grid(cote, cote, 2.0, 1.6);
    let rest = rest_lengths(&e, &p);
    let pinned: Vec<bool> = (0..n).map(|i| i / cote == 0).collect();
    let t = Instant::now();
    let topo = Topo::build(n, &e, &rest, &p, &pinned, None);
    let preparation_ms = t.elapsed().as_secs_f64() * 1000.0;
    let (couleurs, octets, memoire) = (topo.colors(), topo.bytes_per_pass(true, false) as f64, topo.gpu_bytes());
    let octets_avant = topo.bytes_per_pass_before() as f64;
    let niveaux = topo.levels();
    let correction = topo.correction_cost(true, &Default::default(), niveaux);
    let scene = Params { floor: Some(0.0), ball: Some(([0.0, 0.7, 0.0], 0.55)), ..Default::default() };
    // Les images mesurées : budgets de 16 et de 32 passes, sans et avec niveaux, puis avec l'auto-collision en plus
    let mut plans = Vec::new();
    for (budget, with_levels, auto) in [(16, false, false), (16, true, false), (32, false, false), (32, true, false), (16, true, true)] {
        if with_levels && niveaux == 0 {
            continue;
        }
        let levels = if with_levels { niveaux } else { 0 };
        plans.push((budget, with_levels, auto, split_budget(&topo, budget, &Params { levels, thickness: auto.then_some(0.0), ..scene })));
    }
    // Une passe seule : ni accélération, ni amortissement interne, ni niveaux
    let par = Params { iterations: PASSES, substeps: 4, rho: 0.0, calm: 0.0, ..scene };
    let dt = 1.0 / 24.0;
    let chute = [vec![0.0; n], vec![-0.4; n], vec![0.0; n]];
    // Le processeur d'abord : un pas d'image, soit 32 passes
    let mut c = Cloth::new(topo, &p, false);
    c.set_velocities(&chute);
    let t = Instant::now();
    c.step(dt, &par);
    let passe_processeur_ms = t.elapsed().as_secs_f64() * 1000.0 / PASSES as f64;
    // Puis la carte, avec la même topologie
    let mut c = Cloth::new(c.into_topo(), &p, true);
    let carte = c.on_gpu();
    c.set_velocities(&chute);
    // Échauffement (compilation des noyaux, envoi des tampons), puis la mesure
    let mut mesure = |par: &Params| {
        c.step(dt, par);
        c.sync();
        let t = Instant::now();
        for _ in 0..images {
            c.step(dt, par);
        }
        if let Some(g) = crate::gpusim::gpu().filter(|_| carte) {
            g.finish();
        }
        (t.elapsed().as_secs_f64() * 1000.0 / images as f64, c.launches())
    };
    let count = images;
    let (ms, lances) = mesure(&par);
    let (passe_ms, lancements) = (ms / PASSES as f64, lances as f64 / PASSES as f64);
    let images = plans.iter().map(|(budget, niveaux, auto, par)| {
        let (ms, lancements) = mesure(par);
        Image { budget: *budget, niveaux: *niveaux, auto_collision: *auto, sous_pas: par.substeps, lancements, ms }
    }).collect();
    let t = Instant::now();
    c.sync();
    let relecture_ms = t.elapsed().as_secs_f64() * 1000.0;
    let affichage = plans.iter().find(|p| p.0 == 16 && p.1 && !p.2).or(plans.first()).and_then(|plan| affichage(&mut c, cote, &plan.3, dt, count.max(2)));
    Tissu { points: n, couleurs, lancements, octets_avant, niveaux, correction, images, carte: carte && c.on_gpu(), preparation_ms, passe_ms, relecture_ms, passe_processeur_ms, octets, memoire, affichage }
}

/// Mesure une image affichée, sans relecture et avec (voir `Affichage`). `None` hors de la carte.
fn affichage(c: &mut crate::gpucloth::Cloth, cote: usize, par: &crate::gpucloth::Params, dt: f32, images: usize) -> Option<Affichage> {
    use crate::gpudraw::{from_rows, paint, Cam, Canvas, Part, Source, IDENTITY};
    let g = crate::gpusim::gpu().filter(|_| c.on_gpu())?;
    let n = cote * cote;
    let tris = || {
        let mut t = Vec::with_capacity((cote - 1) * (cote - 1) * 2);
        for z in 0..cote - 1 {
            for x in 0..cote - 1 {
                let i = (z * cote + x) as u32;
                t.push([i, i + 1, i + cote as u32 + 1]);
                t.push([i, i + cote as u32 + 1, i + cote as u32]);
            }
        }
        t
    };
    // Une vue de trois quarts sur le tissu pendu (2 m de côté, accroché à 1,6 m)
    let (w, h) = (1280.0f32, 720.0f32);
    let (yaw, pitch, dist, target) = (0.6f32, 0.35f32, 4.5f32, [0.0f32, 0.8, 0.0]);
    let ((sy, cy), (sp, cp)) = (yaw.sin_cos(), pitch.sin_cos());
    let (x, y, z) = ([cy, 0.0, -sy], [-sy * sp, cp, -cy * sp], [sy * cp, sp, cy * cp]);
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (kx, ky, near, far) = (2.0 * h * 0.9 / w, 2.0 * 0.9, 0.05f32, 100.0f32);
    let f = far / (far - near);
    let depth = [-z[0], -z[1], -z[2], dist + dot(z, target)];
    let vp = from_rows([[x[0] * kx, x[1] * kx, x[2] * kx, -dot(x, target) * kx], [y[0] * ky, y[1] * ky, y[2] * ky, -dot(y, target) * ky], [depth[0] * f, depth[1] * f, depth[2] * f, (depth[3] - near) * f], depth]);
    let cam = Cam { view_proj: vp, depth_row: depth, light: [0.3, 0.8, 0.5], eye: [target[0] + z[0] * dist, target[1] + z[1] * dist, target[2] + z[2] * dist], soft: None, shadow: None, outline: 0.0, front: None, scene: None };
    let mut canvas = Canvas::default();
    let mut full = false;
    // Le dessin complet : une boule et un sol pour l'ombre reçue, un matériau en damier de 1024²
    let (ball, floor) = (([0.0f32, 0.7, 0.0], 0.55f32), 0.0f32);
    let occluders = move || {
        let (mut pos, mut tris) = (vec![], vec![]);
        let (seg, rings) = (64u32, 32u32);
        for r in 0..=rings {
            let th = std::f32::consts::PI * r as f32 / rings as f32;
            for s in 0..=seg {
                let ph = std::f32::consts::TAU * s as f32 / seg as f32;
                pos.push([ball.0[0] + ball.1 * th.sin() * ph.cos(), ball.0[1] + ball.1 * th.cos(), ball.0[2] + ball.1 * th.sin() * ph.sin(), 0.0]);
            }
        }
        for r in 0..rings {
            for s in 0..seg {
                let a = r * (seg + 1) + s;
                tris.push([a, a + seg + 1, a + 1]);
                tris.push([a + 1, a + seg + 1, a + seg + 2]);
            }
        }
        let b = pos.len() as u32;
        pos.extend([[-4.0, floor, -4.0, 0.0], [4.0, floor, -4.0, 0.0], [4.0, floor, 4.0, 0.0], [-4.0, floor, 4.0, 0.0]]);
        tris.extend([[b, b + 1, b + 2], [b, b + 2, b + 3]]);
        (pos, tris)
    };
    let side = 1024usize;
    let checker: Vec<[f32; 3]> = (0..side * side).map(|i| if ((i % side) / 64 + (i / side) / 64) % 2 == 0 { [0.8, 0.3, 0.25] } else { [0.9, 0.85, 0.8] }).collect();
    let baked = crate::gpudraw::Baked::new(0xBA2C_7E47, side, &checker, &vec![0.6; side * side], &vec![0.0; side * side])?;
    let uvs = || (0..n).map(|i| [(i % cote) as f32 / (cote - 1) as f32, (i / cote) as f32 / (cote - 1) as f32]).collect::<Vec<[f32; 2]>>();
    let mut draw = |c: &mut crate::gpucloth::Cloth, full: bool| {
        let Some(live) = c.live() else { return false };
        let shadow = full.then(|| crate::gpudraw::Shadow { center: target, radius: 2.5, occluders: Some(crate::gpudraw::Occluders { version: 1, mesh: &occluders, model: IDENTITY }), floor: Some(floor), on_surfaces: 0.5, on_floor: 0.42 });
        let cam = Cam { view_proj: cam.view_proj, depth_row: cam.depth_row, light: cam.light, eye: cam.eye, shadow, outline: if full { 2.0 } else { 0.0 }, front: None, scene: None, soft: None };
        let part = Part {
            n,
            source: Source::Live(&live),
            tris: (0xBA2C_0000 + cote as u64, &tris),
            colors: None,
            base: [0.79, 0.71, 0.64],
            model: IDENTITY,
            uv: full.then_some((1, &uvs as &dyn Fn() -> Vec<[f32; 2]>)),
            surface: if full { crate::gpudraw::Surface { rough: 0.6, metal: 0.0, maps: Some(&baked) } } else { Default::default() },
            id: full as u32,
            select: if full { 2 } else { 0 },
        };
        paint(&mut canvas, [w as u32, h as u32], &cam, &[part])
    };
    let gravity = [0.0, -9.81 * dt, 0.0];
    // Échauffement : pipelines, tampons du maillage
    c.push_uniform(gravity, 1.0);
    c.step(dt, par);
    if !draw(c, false) || !draw(c, true) {
        eprintln!("banc : le dessin direct n'a rien donné (carte : {}, erreur : {})", c.on_gpu(), g.broken());
        return None;
    }
    c.sync();
    let per = |t: Instant| t.elapsed().as_secs_f64() * 1000.0 / images as f64;
    // 1. Rien ne quitte la carte
    let t = Instant::now();
    for _ in 0..images {
        c.push_uniform(gravity, 1.0);
        c.step(dt, par);
        draw(c, full);
    }
    g.finish();
    let direct_ms = per(t);
    c.sync();
    // 2. Comme l'aperçu : un cliché par image, pour le cache, jamais attendu
    let t = Instant::now();
    let mut kept = Vec::with_capacity(images);
    for _ in 0..images {
        c.push_uniform(gravity, 1.0);
        c.step(dt, par);
        kept.push(c.shot());
        draw(c, full);
    }
    g.finish();
    let cliche_ms = per(t);
    drop(kept);
    c.sync();
    // 3. Comme avant : chaque image est relue et copiée avant de continuer
    let t = Instant::now();
    for _ in 0..images {
        c.push_uniform(gravity, 1.0);
        c.step(dt, par);
        let _ = c.read();
    }
    let relu_ms = per(t);
    // 4. Le dessin seul
    let t = Instant::now();
    for _ in 0..images {
        draw(c, full);
    }
    g.finish();
    let dessin_ms = per(t);
    // 5. Comme 1 et 4, avec ombres, matériau et sélection
    full = true;
    let t = Instant::now();
    for _ in 0..images {
        c.push_uniform(gravity, 1.0);
        c.step(dt, par);
        draw(c, full);
    }
    g.finish();
    let complet_ms = per(t);
    let t = Instant::now();
    for _ in 0..images {
        draw(c, full);
    }
    g.finish();
    let dessin_complet_ms = per(t);
    (c.on_gpu() && !g.broken()).then_some(Affichage { direct_ms, cliche_ms, relu_ms, dessin_ms, complet_ms, dessin_complet_ms })
}

/// Le même petit tissu sur la carte et sur le processeur : écart maximal des positions après trois images.
fn ecart_tissu() -> Option<f32> {
    use crate::gpucloth::{grid, rest_lengths, Cloth, Params, Topo};
    let cote = 48;
    let (p, e) = grid(cote, cote, 2.0, 1.6);
    let rest = rest_lengths(&e, &p);
    let pinned: Vec<bool> = (0..cote * cote).map(|i| i / cote == 0).collect();
    let par = Params { iterations: 32, substeps: 4, floor: Some(0.0), ball: Some(([0.0, 0.7, 0.0], 0.55)), ..Default::default() };
    let run = |carte: bool| {
        let mut c = Cloth::new(Topo::build(cote * cote, &e, &rest, &p, &pinned, None), &p, carte);
        if c.on_gpu() != carte {
            return None;
        }
        for _ in 0..3 {
            let (_, mut v) = c.read();
            v[1].iter_mut().for_each(|y| *y -= 9.8 / 24.0);
            c.push_velocities(&v);
            c.step(1.0 / 24.0, &par);
        }
        Some(c.read().0)
    };
    let (a, b) = (run(false)?, run(true)?);
    Some((0..3).flat_map(|k| a[k].iter().zip(&b[k]).map(|(x, y)| (x - y).abs())).fold(0.0, f32::max))
}

struct Ctx {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    module: wgpu::ShaderModule,
    pl: wgpu::PipelineLayout,
}

impl Ctx {
    fn pipeline(&self, entry: &str) -> wgpu::ComputePipeline {
        self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(entry), layout: Some(&self.pl), module: &self.module, entry_point: Some(entry), compilation_options: Default::default(), cache: None })
    }
    fn buffer(&self, bytes: u64) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: bytes.max(16), usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
    }
    fn params(&self, n: u32, dt: f32) -> wgpu::Buffer {
        let data: [u32; 4] = [n, 0, dt.to_bits(), 0];
        let b = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&b, 0, &bytes(&data));
        b
    }
    fn group(&self, p: &wgpu::Buffer, a: &wgpu::Buffer, b: &wgpu::Buffer, sa: &wgpu::Buffer, sb: &wgpu::Buffer, sc: &wgpu::Buffer) -> wgpu::BindGroup {
        fn e(i: u32, buf: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
            wgpu::BindGroupEntry { binding: i, resource: buf.as_entire_binding() }
        }
        self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.layout, entries: &[e(0, p), e(1, a), e(2, b), e(3, sa), e(4, sb), e(5, sc)] })
    }
    fn wait(&self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }
    /// Lit un tampon de f32 (petits tampons de contrôle seulement)
    fn read(&self, buf: &wgpu::Buffer, len: usize) -> Vec<f32> {
        let out = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (len * 4) as u64, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(buf, 0, &out, 0, (len * 4) as u64);
        self.queue.submit([enc.finish()]);
        let slice = out.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.wait();
        let v = match slice.get_mapped_range() {
            Ok(d) => d.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(),
            Err(_) => vec![f32::NAN; len],
        };
        out.unmap();
        v
    }
}

fn bytes<T: Copy>(v: &[T]) -> Vec<u8> {
    // Types simples (f32, u32) : pas d'octets de remplissage
    let n = std::mem::size_of_val(v);
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, n) }.to_vec()
}

fn open() -> Result<(Ctx, String, bool), String> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })).map_err(|e| format!("pas de carte graphique utilisable : {e}"))?;
    let info = adapter.get_info();
    let limits = adapter.limits();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { label: Some("aryadne-banc"), required_limits: limits, ..Default::default() })).map_err(|e| e.to_string())?;
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("banc"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
    let st = |b: u32, ro: bool| wgpu::BindGroupLayoutEntry { binding: b, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: ro }, has_dynamic_offset: false, min_binding_size: None }, count: None };
    let uni = wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[uni, st(1, true), st(2, false), st(3, true), st(4, false), st(5, true)] });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
    let soft = info.device_type == wgpu::DeviceType::Cpu;
    Ok((Ctx { device, queue, layout, module, pl }, format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type), soft))
}

/// Temps moyen d'une passe (en ms) : une passe d'échauffement, puis `k` passes chronométrées
fn time(cx: &Ctx, k: usize, mut pass: impl FnMut(&mut wgpu::CommandEncoder)) -> f64 {
    let mut run = |k: usize| {
        let mut enc = cx.device.create_command_encoder(&Default::default());
        for _ in 0..k {
            pass(&mut enc);
        }
        cx.queue.submit([enc.finish()]);
        cx.wait();
    };
    run(1);
    let t = Instant::now();
    run(k);
    t.elapsed().as_secs_f64() * 1000.0 / k as f64
}

fn dispatch(enc: &mut wgpu::CommandEncoder, pipe: &wgpu::ComputePipeline, group: &wgpu::BindGroup, x: u32, y: u32, z: u32) {
    let mut p = enc.begin_compute_pass(&Default::default());
    p.set_pipeline(pipe);
    p.set_bind_group(0, group, &[]);
    p.dispatch_workgroups(x, y, z);
}

/// Jacobi sur le processeur, pour vérifier la carte
fn jacobi_cpu(n: usize, s: &[f32], rhs: &[f32]) -> Vec<f32> {
    let id = |x: usize, y: usize, z: usize| x + n * (y + n * z);
    let mut out = vec![0.0; n * n * n];
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                let i = id(x, y, z);
                let c = s[i];
                let g = |ok: bool, j: usize| if ok { s[j] } else { c };
                let sum = g(x > 0, i.wrapping_sub(1)) + g(x + 1 < n, i + 1) + g(y > 0, i.wrapping_sub(n)) + g(y + 1 < n, i + n) + g(z > 0, i.wrapping_sub(n * n)) + g(z + 1 < n, i + n * n);
                out[i] = (sum - rhs[i]) / 6.0;
            }
        }
    }
    out
}

/// Lance tout le banc. `quick` : petites tailles (tests, carte logicielle)
pub fn run(quick: bool, progress: &dyn Fn(String)) -> Result<Rapport, String> {
    let (cx, carte, soft) = open()?;
    let mut r = Rapport { carte: carte.clone(), logicielle: soft, ..Default::default() };
    progress(format!("{} : {carte}", crate::i18n::t("carte graphique utilisée")));
    let quick = quick || soft;
    if soft {
        r.notes.push(crate::i18n::t("Rendu logiciel (pas de vraie carte) : les chiffres ne disent rien de la carte, seulement que le code marche."));
    }
    let max_bind = cx.device.limits().max_storage_buffer_binding_size as u64;
    let (pc, pa, pb, ps) = (cx.pipeline("copy"), cx.pipeline("jacobi"), cx.pipeline("advect"), cx.pipeline("project"));
    let pdiv = cx.pipeline("divergence");
    // Tampons vides pour les places inutilisées : un pour les lectures, un par place écrite (pas de double usage)
    let (dr, dwb, dws) = (cx.buffer(16), cx.buffer(16), cx.buffer(16));

    // 1. Copie : le plafond de débit
    {
        let bytes = (if quick { 16u64 << 20 } else { 256u64 << 20 }).min(max_bind / 16 * 16);
        let (a, b) = (cx.buffer(bytes), cx.buffer(bytes));
        let p = cx.params(0, 0.0);
        let g = cx.group(&p, &a, &b, &dr, &dws, &dr);
        let groups = (bytes / 16).div_ceil(256) as u32;
        let (gx, gy) = (groups.min(256), groups.div_ceil(256));
        let ms = time(&cx, if quick { 4 } else { 20 }, |enc| dispatch(enc, &pc, &g, gx, gy, 1));
        let m = Mesure { genre: "copie", nom: crate::i18n::t("copie de {} Mo").replace("{}", &(bytes >> 20).to_string()), ms, octets: (bytes * 2) as f64 };
        progress(format!("{} : {:.2} ms, {:.0} Go/s", m.nom, m.ms, m.debit()));
        r.mesures.push(m);
    }

    // 2. Pression : itération de Jacobi à 7 points, et contrôle contre le processeur
    for n in if quick { vec![32u32, 64] } else { vec![128u32, 256] } {
        let cells = (n as u64).pow(3);
        if cells * 4 > max_bind {
            r.notes.push(format!("{n}³ : {} ({} Mo)", crate::i18n::t_mid("dépasse la taille de tampon permise"), max_bind >> 20));
            continue;
        }
        let init: Vec<f32> = (0..cells).map(|i| ((i * 2654435761) % 1000) as f32 / 1000.0).collect();
        let rhs: Vec<f32> = (0..cells).map(|i| ((i * 40503) % 997) as f32 / 997.0 - 0.5).collect();
        let (s0, s1, d) = (cx.buffer(cells * 4), cx.buffer(cells * 4), cx.buffer(cells * 4));
        cx.queue.write_buffer(&s0, 0, &bytes(&init));
        cx.queue.write_buffer(&d, 0, &bytes(&rhs));
        let p = cx.params(n, 0.0);
        let g01 = cx.group(&p, &dr, &dwb, &s0, &s1, &d);
        let g10 = cx.group(&p, &dr, &dwb, &s1, &s0, &d);
        let wg = (n.div_ceil(8), n.div_ceil(8), n.div_ceil(4));
        if n == 32 || n == 128 {
            let mut enc = cx.device.create_command_encoder(&Default::default());
            dispatch(&mut enc, &pa, &g01, wg.0, wg.1, wg.2);
            cx.queue.submit([enc.finish()]);
            let gpu = cx.read(&s1, cells as usize);
            let cpu = jacobi_cpu(n as usize, &init, &rhs);
            r.ecart = r.ecart.max(gpu.iter().zip(&cpu).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max));
        }
        let mut flip = false;
        let ms = time(&cx, if quick { 6 } else { 40 }, |enc| {
            dispatch(enc, &pa, if flip { &g10 } else { &g01 }, wg.0, wg.1, wg.2);
            flip = !flip;
        });
        // Octets utiles : lire la pression et le second membre, écrire la pression
        let m = Mesure { genre: "pression", nom: crate::i18n::t("itération de pression {}³").replace("{}", &n.to_string()), ms, octets: (cells * 12) as f64 };
        progress(format!("{} : {:.3} ms, {:.0} Go/s utiles", m.nom, m.ms, m.debit()));
        r.mesures.push(m);
    }

    // 3. Fumée : advection + divergence + 40 itérations de Jacobi + projection, par pas
    for n in if quick { vec![32u32] } else { vec![128u32, 256] } {
        let cells = (n as u64).pow(3);
        if cells * 16 > max_bind {
            r.notes.push(format!("{n}³ : {} ({} Mo)", crate::i18n::t_mid("dépasse la taille de tampon permise"), max_bind >> 20));
            continue;
        }
        let iters = 40;
        let (v0, v1) = (cx.buffer(cells * 16), cx.buffer(cells * 16));
        let (p0, p1, dv) = (cx.buffer(cells * 4), cx.buffer(cells * 4), cx.buffer(cells * 4));
        // Une boule de fumée qui monte
        let c = n as f32 / 2.0;
        let init: Vec<f32> = (0..cells)
            .flat_map(|i| {
                let (x, y, z) = ((i % n as u64) as f32, ((i / n as u64) % n as u64) as f32, (i / (n as u64 * n as u64)) as f32);
                let d = ((x - c).powi(2) + (y - n as f32 * 0.25).powi(2) + (z - c).powi(2)).sqrt();
                let w = if d < n as f32 * 0.12 { 1.0 } else { 0.0 };
                [0.0, w, 0.0, w]
            })
            .collect();
        cx.queue.write_buffer(&v0, 0, &bytes(&init));
        let p = cx.params(n, 1.0);
        let g_adv = cx.group(&p, &v0, &v1, &dr, &dws, &dr);
        let g_div = cx.group(&p, &v1, &dwb, &dr, &dv, &dr);
        let g_j01 = cx.group(&p, &dr, &dwb, &p0, &p1, &dv);
        let g_j10 = cx.group(&p, &dr, &dwb, &p1, &p0, &dv);
        let g_prj = cx.group(&p, &v1, &v0, &p0, &dws, &dr);
        let wg = (n.div_ceil(8), n.div_ceil(8), n.div_ceil(4));
        let ms = time(&cx, if quick { 2 } else { 8 }, |enc| {
            dispatch(enc, &pb, &g_adv, wg.0, wg.1, wg.2);
            dispatch(enc, &pdiv, &g_div, wg.0, wg.1, wg.2);
            for k in 0..iters {
                dispatch(enc, &pa, if k % 2 == 0 { &g_j01 } else { &g_j10 }, wg.0, wg.1, wg.2);
            }
            dispatch(enc, &ps, &g_prj, wg.0, wg.1, wg.2);
        });
        let octets = cells as f64 * (16.0 * 2.0 + 16.0 + 4.0 + iters as f64 * 12.0 + 16.0 * 2.0 + 4.0);
        let m = Mesure { genre: "fumée", nom: crate::i18n::t("pas de fumée {}³ (40 itérations de pression)").replace("{}", &n.to_string()), ms, octets };
        progress(format!("{} : {:.1} ms", m.nom, m.ms));
        r.mesures.push(m);
        // Contrôle grossier : la fumée ne doit pas exploser
        if n <= 64 {
            let v = cx.read(&v0, (cells * 4) as usize);
            if v.iter().any(|x| !x.is_finite()) {
                r.notes.push(crate::i18n::t("la fumée donne des valeurs non finies"));
            }
        }
    }
    if r.ecart > 1e-4 {
        r.notes.push(format!("{} : {}", crate::i18n::t("écart avec le processeur trop grand"), r.ecart));
    }
    // 3 bis. La fumée des nodes : pression par multigrille (MGPCG), à comparer au Jacobi ci-dessus
    for cote in if quick { vec![32usize] } else { vec![128, 256] } {
        progress(crate::i18n::t("fumée {}³, pression par multigrille").replace("{}", &cote.to_string()));
        let m = fumee(cote, if quick { 2 } else { 6 });
        if !m.carte {
            r.notes.push(crate::i18n::t("fumée {}³ : elle ne tient pas sur cette carte, mesurée sur le processeur").replace("{}", &cote.to_string()));
        }
        for (it, ms, _, after) in &m.par_iterations {
            progress(format!("{}³, {} {} : {:.1} ms, {} {:.1e}", cote, it, crate::i18n::t_mid("itérations"), ms, crate::i18n::t_mid("divergence"), after));
        }
        r.fumee.push(m);
    }
    // Tissu en VBD : d'abord le contrôle, puis la vitesse sur de grands tissus
    r.ecart_tissu = ecart_tissu();
    if r.ecart_tissu.is_some_and(|e| e > 1e-3) {
        r.notes.push(format!("{} : {:?}", crate::i18n::t("tissu : écart avec le processeur trop grand"), r.ecart_tissu));
    }
    for cote in if quick { vec![64usize] } else { vec![500, 1000] } {
        let m = tissu(cote, if quick { 1 } else { 3 });
        progress(format!("{} : {:.3} ms", crate::i18n::t("tissu, {} sommets, une passe").replace("{}", &m.points.to_string()), m.passe_ms));
        if !m.carte {
            r.notes.push(format!("{} ({} Mo)", crate::i18n::t("tissu de {} sommets : il ne tient pas sur cette carte, mesuré sur le processeur").replace("{}", &m.points.to_string()), max_bind >> 20));
        }
        r.tissu.push(m);
    }
    if !quick {
        progress(crate::i18n::t("comparaison du tissu avec le XPBD, moins d'une minute"));
        r.comparaison = Some(crate::geo::cloth_comparison(40, &[16, 32, 64]));
    }
    Ok(r)
}

impl Rapport {
    /// Le rapport lisible, avec ce que ces chiffres veulent dire pour les simulations prévues
    pub fn texte(&self) -> String {
        let t = crate::i18n::t;
        let tm = crate::i18n::t_mid;
        let mut s = format!("{} : {}\n\n", t("carte graphique utilisée"), self.carte);
        for m in &self.mesures {
            s += &format!("{} : {:.3} ms ({:.0} {})\n", m.nom, m.ms, m.debit(), crate::i18n::t_mid("Go/s"));
        }
        s += &format!("\n{} : {:.1e}\n", t("écart avec le processeur (contrôle)"), self.ecart);
        let copy = self.mesures.iter().find(|m| m.genre == "copie").map(|m| m.debit());
        let jac = self.mesures.iter().filter(|m| m.genre == "pression").last().map(|m| m.debit());
        if let (Some(c), Some(j)) = (copy, jac) {
            // Estimations du rapport de recherche, recalées sur la mesure : fumée 512³, 8 itérations de multigrille
            let cells = 512f64.powi(3);
            let mg = cells * 8.0 * 12.0 * 6.0 / (j * 1e9) * 1000.0;
            let adv = cells * 70.0 / (c * 1e9) * 1000.0;
            s += &format!(
                "\n{} :\n- {} : ~{:.0} ms\n- {} : ~{:.0} ms\n- {} : {:.0} %\n",
                t("ce que ça donne pour la suite (estimation)"),
                t("fumée 512³ dense, pression par multigrille (8 itérations), par pas"),
                mg + adv,
                t("advection et forces 512³, par pas"),
                adv,
                t("efficacité de la pression contre la copie"),
                j / c * 100.0
            );
        }
        if !self.fumee.is_empty() {
            s += &format!("\n{}\n", t("Fumée des nodes : un pas complet, pression par multigrille (MGPCG). À comparer au pas à 40 itérations de Jacobi ci-dessus."));
            for f in &self.fumee {
                s += &format!("- {}³{} · {:.0} {}\n", f.cote, if f.carte { String::new() } else { format!(" ({})", tm("sur le processeur")) }, f.octets as f64 / 1048576.0, tm("Mo sur la carte"));
                for (it, ms, before, after) in &f.par_iterations {
                    s += &format!("  {} {} : {:.1} ms {} · {} {:.1e} → {:.1e}\n", it, tm("itérations"), ms, tm("par pas"), tm("divergence"), before, after);
                }
                s += &format!("  {} : {:.1} ms {}\n", tm("4 itérations, avec le transfert de chaque image pour le cache"), f.avec_cache_ms, tm("par pas"));
            }
        }
        if !self.tissu.is_empty() {
            s += &format!("\n{} :\n", t("tissu en VBD (une passe = tous les sommets mis à jour une fois)"));
        }
        for m in &self.tissu {
            let ou = tm(if m.carte { "carte graphique" } else { "processeur" });
            s += &format!("- {} {} ({} {}, {}) : {:.3} ms {}, {:.1} {}, {:.2} ns {}, ~{:.0} {}\n", m.points, tm("sommets"), m.couleurs, tm("couleurs"), ou, m.passe_ms, tm("par passe"), m.lancements, tm("lancements par passe"), m.passe_ms * 1e6 / m.points as f64, tm("par sommet et par passe"), m.octets / (m.passe_ms / 1000.0) / 1e9, tm("Go/s"));
            s += &format!("  {} : {:.0} ({} : {:.0}, ÷{:.1})\n", t("octets lus et écrits par sommet et par passe"), m.octets / m.points as f64, tm("avant le rangement compact"), m.octets_avant / m.points as f64, m.octets_avant / m.octets);
            for im in &m.images {
                let quoi = match (im.niveaux, im.auto_collision) {
                    (false, _) => tm("sans niveaux"),
                    (true, false) => tm("avec niveaux grossiers"),
                    (true, true) => tm("avec niveaux et auto-collision"),
                };
                s += &format!("  {} {} {}, {} : {:.1} ms ({} {}, {} {})", t("une image au budget de"), im.budget, tm("passes"), quoi, im.ms, im.sous_pas, tm("sous-pas"), im.lancements, tm("lancements"));
                // Le surcoût de l'auto-collision : contre la même image sans elle, par sous-pas
                if let Some(base) = m.images.iter().find(|b| im.auto_collision && b.budget == im.budget && b.niveaux && !b.auto_collision) {
                    s += &format!(" · {} : {:.2} ms {} ({} {:.2})", tm("l'auto-collision coûte"), im.ms / im.sous_pas.max(1) as f64 - base.ms / base.sous_pas.max(1) as f64, tm("par sous-pas"), tm("sans elle"), base.ms / base.sous_pas.max(1) as f64);
                }
                s += "\n";
            }
            if m.niveaux > 0 {
                s += &format!("  {} : {} · {} {:.1} {}\n", t("niveaux grossiers"), m.niveaux, tm("une correction coûte"), m.correction, tm("passes"));
            }
            s += &format!("  {} : {:.0} {} · {} : {:.1} ms\n", t("à 24 images par seconde"), 1000.0 / 24.0 / m.passe_ms, tm("passes par image"), tm("relecture"), m.relecture_ms);
            if let Some(a) = &m.affichage {
                s += &format!("  {} : {:.1} ms ({:.0} {}) · {} : {:.1} ms · {} : {:.1} ms ({:.0} {}) · {} : {:.1} ms\n", t("image affichée sans relecture (pas à 16 passes + dessin direct)"), a.direct_ms, 1000.0 / a.direct_ms.max(1e-6), tm("images/s"), tm("avec le transfert différé pour le cache"), a.cliche_ms, tm("avec relecture comme avant, sans son dessin"), a.relu_ms, 1000.0 / a.relu_ms.max(1e-6), tm("images/s"), tm("dessin seul"), a.dessin_ms);
                s += &format!("  {} : {:.1} ms ({:.0} {}) · {} : {:.1} ms\n", t("avec ombres, matériau en textures et contour de sélection"), a.complet_ms, 1000.0 / a.complet_ms.max(1e-6), tm("images/s"), tm("dessin seul"), a.dessin_complet_ms);
            }
            s += &format!("  {} : {:.2} ms {} (×{:.1}) · {} : {:.0} ms · {} : {:.0} Mo\n", t("processeur"), m.passe_processeur_ms, tm("par passe"), m.passe_processeur_ms / m.passe_ms, tm("préparation"), m.preparation_ms, tm("mémoire sur la carte"), m.memoire as f64 / 1e6);
        }
        if let Some(e) = self.ecart_tissu {
            s += &format!("{} : {:.1e} m\n", t("écart du tissu entre la carte et le processeur (contrôle)"), e);
        }
        if let Some(c) = &self.comparaison {
            s += &format!("\n{} ({} {}, {} N/m, {} {}) :\n", t("VBD, sans et avec niveaux grossiers, contre XPBD, à nombre de passes égal"), c.points, tm("sommets"), c.stiffness, c.levels, tm("niveaux grossiers"));
            s += &format!("{} : {:.3} %, {:.4} J\n", t("tissu pendu, calculé jusqu'au bout : allongement, énergie élastique"), c.reference.0 * 100.0, c.reference.1);
            let mut budget = 0;
            for row in &c.rows {
                if row.budget != budget {
                    budget = row.budget;
                    s += &format!("{} {} :\n", budget, tm("passes par image"));
                }
                s += &format!("- {:13} ({:2} {}, {} {:4.1}) : {} {:5.1} %, {} {:.3} % · {} : {} {:.1} %, {} {:.2} %\n", row.solver, row.substeps, tm("sous-pas"), tm("coût réel"), row.cost, tm("raideur obtenue"), row.stiffness * 100.0, tm("allongement"), row.elongation * 100.0, tm("chute sur une boule"), tm("pire arête"), row.worst * 100.0, tm("moyenne"), row.mean * 100.0);
            }
        }
        for n in &self.notes {
            s += &format!("\n{} : {n}", t("note"));
        }
        s
    }
}

// ---------- fenêtre du menu Aide ----------

struct Etat {
    open: bool,
    lines: Vec<String>,
    rx: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    done: Option<Result<String, String>>,
}

static ETAT: Mutex<Option<Etat>> = Mutex::new(None);

/// Ouvre la fenêtre et lance le banc dans un fil à part
pub fn start() {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut g = ETAT.lock().unwrap_or_else(|e| e.into_inner());
    if g.as_ref().is_some_and(|e| e.rx.is_some()) {
        if let Some(e) = g.as_mut() {
            e.open = true;
        }
        return;
    }
    *g = Some(Etat { open: true, lines: vec![], rx: Some(rx), done: None });
    std::thread::spawn(move || {
        let tx2 = tx.clone();
        // Une erreur de la carte (pilote, validation) ne doit pas laisser la fenêtre attendre pour rien
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(false, &move |l| {
                let _ = tx2.send(Ok(format!("…{l}")));
            })
        }));
        let res = match res {
            Ok(r) => r.map(|r| r.texte()),
            Err(e) => Err(e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| crate::i18n::t("la carte graphique a renvoyé une erreur"))),
        };
        let _ = tx.send(res);
    });
}

pub fn ui(ctx: &egui::Context) {
    let mut g = ETAT.lock().unwrap_or_else(|e| e.into_inner());
    let Some(st) = g.as_mut() else { return };
    if let Some(rx) = &st.rx {
        while let Ok(m) = rx.try_recv() {
            match m {
                Ok(l) if l.starts_with('…') => st.lines.push(l.trim_start_matches('…').to_string()),
                other => {
                    st.done = Some(other);
                    st.rx = None;
                    break;
                }
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    if !st.open {
        return;
    }
    let p = crate::theme::pal();
    let (lines, done, running) = (st.lines.clone(), st.done.clone(), st.rx.is_some());
    drop(g);
    let still = crate::dock::os_window(ctx, "banc-gpu", &crate::i18n::t("mesure de la carte graphique"), egui::vec2(560.0, 420.0), |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            match &done {
                Some(Ok(t)) => {
                    ui.label(egui::RichText::new(t).monospace().size(12.0));
                    if ui.button(crate::i18n::t("copier le rapport")).clicked() {
                        ui.ctx().copy_text(t.clone());
                    }
                }
                Some(Err(e)) => {
                    ui.colored_label(p.warning, e);
                }
                None => {
                    for l in &lines {
                        ui.label(egui::RichText::new(l).monospace().size(12.0));
                    }
                }
            }
            if running {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(egui::RichText::new(crate::i18n::t("mesure en cours, quelques secondes")).color(p.muted));
                });
            }
        });
    });
    if !still {
        if let Some(st) = ETAT.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            st.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn banc_court() {
        // Sans carte du tout (machine de test), rien à vérifier
        let Ok(r) = super::run(true, &|_| {}) else { return };
        assert!(r.ecart < 1e-4, "écart {}", r.ecart);
        assert!(r.mesures.iter().all(|m| m.ms.is_finite() && m.ms > 0.0));
        assert!(!r.notes.iter().any(|n| n.contains("non finies")));
        assert!(r.tissu.iter().all(|m| m.passe_ms > 0.0 && m.passe_processeur_ms > 0.0));
        // Le dessin complet (ombres, matériau, sélection) est mesuré quand le dessin direct l'est
        assert!(r.tissu.iter().filter_map(|m| m.affichage.as_ref()).all(|a| a.complet_ms > 0.0 && a.dessin_complet_ms > 0.0));
        // La fumée des nodes : chaque doublement des itérations réduit la divergence qui reste
        let f = &r.fumee[0];
        assert!(f.par_iterations.len() == 3 && f.par_iterations.iter().all(|m| m.1 > 0.0 && m.3.is_finite()));
        assert!(f.par_iterations[2].3 < f.par_iterations[0].3 && f.par_iterations[1].3 < f.par_iterations[1].2 * 0.1, "{:?}", f.par_iterations);
        assert!(r.texte().contains("MGPCG"));
        assert!(r.ecart_tissu.is_none_or(|e| e < 1e-3), "écart du tissu {:?}", r.ecart_tissu);
        eprintln!("{}", r.texte());
    }

    /// La comparaison à budget égal, à la demande (une minute en mode release) :
    /// `cargo test --release comparaison_a_la_demande -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn comparaison_a_la_demande() {
        let r = super::Rapport { comparaison: Some(crate::geo::cloth_comparison(40, &[16, 32, 64])), ..Default::default() };
        eprintln!("{}", r.texte());
    }

    /// Mesure à la demande d'un tissu de côté `ARY_COTE` (250 par défaut) : `cargo test --release mesure_tissu -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mesure_tissu() {
        let cote = std::env::var("ARY_COTE").ok().and_then(|s| s.parse().ok()).unwrap_or(250);
        let r = super::Rapport { tissu: vec![super::tissu(cote, 2)], ..Default::default() };
        eprintln!("{}", r.texte());
    }
}
