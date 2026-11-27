//! Dessin d'un maillage directement sur la carte graphique.
//!
//! Sert à montrer l'état d'une simulation **sans le relire** : le tampon de positions du solveur
//! est le tampon de sommets (`Source::Live`). Le même dessin sait aussi partir de positions venues
//! du processeur (`Source::Data`), pour qu'une image reprise du cache ait le même aspect.
//!
//! - **Normales sur la carte.** Un noyau de calcul fait, pour chaque sommet, la somme des normales
//!   de ses triangles (pondérées par leur aire). Il relit le tampon de positions, rien d'autre.
//! - **Ombrage.** La lumière principale, un peu de ciel, les deux faces éclairées. Un matériau
//!   donne sa couleur, sa rugosité et son métal, en nombres ou en textures cuites (`Baked`).
//! - **Ombres.** Deux cartes d'ombre vues de la lumière : celle des maillages dessinés ici, refaite
//!   à chaque image, et celle des autres objets de la scène (`Occluders`), refaite quand ils
//!   changent. Le maillage lit les deux. Une passe de plus pose l'ombre du maillage sur l'image
//!   calculée ailleurs (ses surfaces, retrouvées par leur profondeur, et le sol).
//! - **Identifiants.** Une image à part garde, par pixel, la profondeur et le numéro du morceau
//!   visible : pour le choisir au clic (`Canvas::pick`), dessiner son contour de sélection, et
//!   dire à la fumée ce qui est devant elle (`Canvas::front`).
//! - **Profondeur.** Le maillage a son tampon de profondeur. Pour se mêler à une image calculée
//!   ailleurs (la vue 3D de la scène est un rasteriseur logiciel), on lui donne la profondeur de
//!   cette image (`Soft`) : un pixel du maillage caché par elle n'est pas dessiné.
//! - **Sortie.** Une texture (`Canvas`) que l'interface affiche comme une image. Elle reste sur
//!   la carte de bout en bout. Sans interface (banc, tests), on peut la relire avec `pixels`.
//!
//! Tout passe par la carte de `gpusim`. Quand l'interface dessine avec wgpu, c'est la sienne.

use crate::gpucloth::Live;
use crate::gpusim::{self, Bind, Gpu, Kernels};
use eframe::egui;
use std::sync::{Mutex, OnceLock};

/// D'où viennent les positions d'un morceau
pub enum Source<'a> {
    /// Les tampons d'un tissu, tels que le solveur vient de les écrire
    Live(&'a Live),
    /// Des positions du processeur : leur version (elles ne sont renvoyées que si elle change),
    /// et de quoi les obtenir, dans l'ordre de la géométrie
    Data(u64, &'a dyn Fn() -> Vec<[f32; 4]>),
}

/// Un morceau de géométrie à dessiner
pub struct Part<'a> {
    /// Nombre de sommets
    pub n: usize,
    pub source: Source<'a>,
    /// Clé de la topologie, et ses triangles dans l'ordre de la géométrie (demandés une fois par clé)
    pub tris: (u64, &'a dyn Fn() -> Vec<[u32; 3]>),
    /// Couleur par sommet (linéaire, 8 bits), avec sa version ; sinon `base`
    pub colors: Option<(u64, &'a dyn Fn() -> Vec<[u8; 4]>)>,
    /// Couleur de base, linéaire
    pub base: [f32; 3],
    /// Place du morceau dans le monde (matrice 4×4, colonnes d'abord)
    pub model: [[f32; 4]; 4],
    /// UV par sommet (v vers le haut), avec leur version : pour les textures du matériau
    pub uv: Option<(u64, &'a dyn Fn() -> Vec<[f32; 2]>)>,
    pub surface: Surface<'a>,
    /// Numéro du morceau dans l'image des identifiants (0 : aucun), voir `Canvas::pick`
    pub id: u32,
    /// Contour de sélection : 0 aucun, 1 choisi, 2 actif
    pub select: u8,
}

/// L'aspect d'un morceau : rugosité et métal, et les textures cuites du matériau s'il en a
/// (elles demandent des UV ; la couleur de la texture remplace alors `base`).
#[derive(Clone, Copy)]
pub struct Surface<'a> {
    pub rough: f32,
    pub metal: f32,
    pub maps: Option<&'a Baked>,
}

impl Default for Surface<'_> {
    fn default() -> Self {
        Surface { rough: 0.62, metal: 0.0, maps: None }
    }
}

/// Un matériau cuit en textures carrées : de quoi le montrer sans l'évaluer pixel par pixel.
pub struct Baked {
    /// Change quand le matériau change : les textures ne sont envoyées à la carte qu'une fois par clé
    pub key: u64,
    /// Côté, en texels
    pub n: u32,
    /// Couleur (sRGB, 8 bits), rangée du haut d'abord
    pub color: Vec<[u8; 4]>,
    /// Rugosité (rouge) et métal (vert)
    pub orm: Vec<[u8; 4]>,
    /// Moyennes, pour un maillage sans UV : couleur (linéaire), rugosité, métal
    pub mean: ([f32; 3], f32, f32),
}

impl Baked {
    /// À partir de cartes en nombres : couleur sRGB, rugosité et métal par texel.
    pub fn new(key: u64, n: usize, color: &[[f32; 3]], rough: &[f32], metal: &[f32]) -> Option<Baked> {
        if n == 0 || color.len() != n * n || rough.len() != n * n || metal.len() != n * n {
            return None;
        }
        let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        let (mut c, mut r, mut m) = ([0.0f64; 3], 0.0f64, 0.0f64);
        for i in 0..n * n {
            for k in 0..3 {
                c[k] += (color[i][k].clamp(0.0, 1.0) as f64).powf(2.2);
            }
            r += rough[i].clamp(0.0, 1.0) as f64;
            m += metal[i].clamp(0.0, 1.0) as f64;
        }
        let t = (n * n) as f64;
        Some(Baked {
            key,
            n: n as u32,
            color: color.iter().map(|c| [q(c[0]), q(c[1]), q(c[2]), 255]).collect(),
            orm: rough.iter().zip(metal).map(|(r, m)| [q(*r), q(*m), 0, 255]).collect(),
            mean: ([(c[0] / t) as f32, (c[1] / t) as f32, (c[2] / t) as f32], (r / t) as f32, (m / t) as f32),
        })
    }
}

/// Niveau suivant d'une texture carrée (moyenne de quatre texels). `None` quand le côté est impair ou vaut 1.
pub fn half_size(px: &[[u8; 4]], n: usize) -> Option<Vec<[u8; 4]>> {
    if n < 2 || n % 2 != 0 || px.len() != n * n {
        return None;
    }
    let h = n / 2;
    let mut out = Vec::with_capacity(h * h);
    for y in 0..h {
        for x in 0..h {
            let mut s = [0u32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = px[(y * 2 + dy) * n + x * 2 + dx];
                for k in 0..4 {
                    s[k] += p[k] as u32;
                }
            }
            out.push([((s[0] + 2) / 4) as u8, ((s[1] + 2) / 4) as u8, ((s[2] + 2) / 4) as u8, ((s[3] + 2) / 4) as u8]);
        }
    }
    Some(out)
}

/// Les autres objets de la scène, vus seulement comme ce qui cache la lumière : envoyés à la
/// carte quand leur version change.
pub struct Occluders<'a> {
    pub version: u64,
    /// Sommets et triangles
    pub mesh: &'a dyn Fn() -> (Vec<[f32; 4]>, Vec<[u32; 3]>),
    /// Place de ces sommets dans le monde
    pub model: [[f32; 4]; 4],
}

/// Les ombres de la lumière principale (`Cam::light`).
pub struct Shadow<'a> {
    /// La boule du monde que la carte d'ombre couvre
    pub center: [f32; 3],
    pub radius: f32,
    pub occluders: Option<Occluders<'a>>,
    /// Hauteur du sol dans le monde, si l'image calculée ailleurs y montre des ombres
    pub floor: Option<f32>,
    /// Force de l'ombre posée sur les surfaces de l'image calculée ailleurs, et sur le sol (0 à 1)
    pub on_surfaces: f32,
    pub on_floor: f32,
}

/// Du monde vers la carte d'ombre : x et y de −1 à 1 dans le plan qui fait face à la lumière,
/// profondeur de 0 (côté lumière) à 1, pour la boule donnée. `light` : direction vers la lumière.
pub fn light_matrix(light: [f32; 3], center: [f32; 3], radius: f32) -> [[f32; 4]; 4] {
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let unit = |a: [f32; 3]| {
        let l = dot(a, a).sqrt().max(1e-12);
        [a[0] / l, a[1] / l, a[2] / l]
    };
    let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let l = unit(if dot(light, light) > 1e-12 { light } else { [0.0, 1.0, 0.0] });
    let x = unit(cross(if l[1].abs() < 0.95 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] }, l));
    let y = cross(l, x);
    let r = radius.max(1e-6);
    from_rows([
        [x[0] / r, x[1] / r, x[2] / r, -dot(x, center) / r],
        [y[0] / r, y[1] / r, y[2] / r, -dot(y, center) / r],
        [-l[0] / (2.0 * r), -l[1] / (2.0 * r), -l[2] / (2.0 * r), 0.5 + dot(l, center) / (2.0 * r)],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

/// Profondeur d'un pixel de l'image des identifiants où rien n'est dessiné
pub const NOTHING: f32 = 1e30;

/// Ce que garde un pixel de l'image des identifiants : le numéro du morceau et son contour.
pub fn id_code(id: u32, select: u8) -> u32 {
    id.min(0x3F_FFFF) * 4 + select.min(3) as u32
}

/// L'inverse de `id_code` : (numéro, contour).
pub fn id_decode(code: u32) -> (u32, u8) {
    (code / 4, (code % 4) as u8)
}

/// Profondeur d'une image calculée ailleurs, pour s'y mêler : une valeur par pixel, la même
/// grandeur que `Cam::depth_row` appliquée à un point du monde, plus petit = plus près.
pub struct Soft<'a> {
    pub w: u32,
    pub h: u32,
    pub version: u64,
    pub depth: &'a dyn Fn() -> Vec<f32>,
    /// Tolérance, dans l'unité de la profondeur
    pub bias: f32,
}

/// La caméra, en matrices (colonnes d'abord, profondeur de 0 à 1)
pub struct Cam<'a> {
    pub view_proj: [[f32; 4]; 4],
    /// Profondeur d'un point du monde `p` : `depth_row · (p, 1)`
    pub depth_row: [f32; 4],
    /// Direction vers la lumière, dans le monde
    pub light: [f32; 3],
    /// Position de l'œil, dans le monde
    pub eye: [f32; 3],
    pub soft: Option<Soft<'a>>,
    pub shadow: Option<Shadow<'a>>,
    /// Épaisseur du contour de sélection, en pixels (0 : pas de contour)
    pub outline: f32,
    /// Ce qu'un autre dessin de la carte a déjà posé devant (`Canvas::front`) : la fumée s'y arrête
    pub front: Option<wgpu::TextureView>,
    /// L'éclairage de la scène, pour ressembler à l'image calculée ailleurs. Sans lui, un éclairage
    /// fixe de studio
    pub scene: Option<Lighting>,
}

/// L'éclairage d'une scène, en couleurs linéaires : ce que le ciel donne à une surface tournée
/// vers le haut et vers le bas, la couleur du soleil (`Cam::light`) multipliée par sa force, et
/// l'exposition (un facteur)
#[derive(Clone, Copy, Debug)]
pub struct Lighting {
    pub sky_up: [f32; 3],
    pub sky_down: [f32; 3],
    pub sun: [f32; 3],
    pub exposure: f32,
}

pub const IDENTITY: [[f32; 4]; 4] = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

/// Matrice à partir de ses quatre lignes
pub fn from_rows(r: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut m = [[0.0; 4]; 4];
    for (i, row) in r.iter().enumerate() {
        for (j, x) in row.iter().enumerate() {
            m[j][i] = *x;
        }
    }
    m
}

const NORMALS: &str = r#"
struct NU { count: u32, p0: u32, p1: u32, p2: u32 }
@group(0) @binding(0) var<uniform> s: NU;
@group(0) @binding(1) var<storage, read> pos: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> off: array<u32>;
@group(0) @binding(3) var<storage, read> adj: array<vec2<u32>>;
@group(0) @binding(4) var<storage, read_write> nrm: array<vec4<f32>>;

@compute @workgroup_size(WG)
fn normals(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= s.count) { return; }
    let p = pos[i].xyz;
    var n = vec3<f32>(0.0);
    let end = off[i + 1u];
    for (var e = off[i]; e < end; e++) {
        let ab = adj[e];
        n += cross(pos[ab.x].xyz - p, pos[ab.y].xyz - p);
    }
    let l = length(n);
    var o = vec3<f32>(0.0, 1.0, 0.0);
    if (l > 1e-30) { o = n / l; }
    nrm[i] = vec4<f32>(o, 0.0);
}
"#;

const SHADER: &str = r#"
struct U {
    vp: mat4x4<f32>,
    model: mat4x4<f32>,
    light_vp: mat4x4<f32>,
    origin: vec4<f32>,
    depth_row: vec4<f32>,
    light: vec4<f32>,
    eye: vec4<f32>,
    base: vec4<f32>,
    soft: vec4<f32>,
    size: vec4<f32>,
    // Ombres : actives, taille d'un texel dans le monde, tolérance en profondeur, carte des autres objets
    sh: vec4<f32>,
    // Matériau : rugosité, métal, textures, UV
    mat: vec4<f32>,
    // Code du morceau dans l'image des identifiants
    ids: vec4<f32>,
    // Éclairage de la scène (`Cam::scene`) : ciel vu d'en haut (w : présent), d'en bas (w : exposition), soleil
    sky_up: vec4<f32>,
    sky_down: vec4<f32>,
    sun: vec4<f32>,
}
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var soft: texture_2d<f32>;
@group(0) @binding(2) var sh_own: texture_depth_2d;
@group(0) @binding(3) var sh_others: texture_depth_2d;
@group(0) @binding(4) var sh_cmp: sampler_comparison;
@group(0) @binding(5) var t_color: texture_2d<f32>;
@group(0) @binding(6) var t_orm: texture_2d<f32>;
@group(0) @binding(7) var t_samp: sampler;

struct VO {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) vd: f32,
    @location(4) uv: vec2<f32>,
}

@vertex
fn vs(@location(0) p: vec3<f32>, @location(1) n: vec3<f32>, @location(2) c: vec4<f32>, @location(3) uv: vec2<f32>) -> VO {
    var o: VO;
    let w = u.model * vec4<f32>(p + u.origin.xyz, 1.0);
    o.clip = u.vp * w;
    o.world = w.xyz;
    o.nrm = (u.model * vec4<f32>(n, 0.0)).xyz;
    o.color = mix(u.base.rgb, c.rgb, u.base.a);
    o.vd = dot(u.depth_row, w);
    // Les textures cuites ont leur rangée du haut d'abord
    o.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    return o;
}

// Part de lumière qui arrive en un point du monde (1 : tout, 0 : à l'ombre). `n` : la normale, du côté de la lumière
fn lit(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if (u.sh.x < 0.5) { return 1.0; }
    let q = u.light_vp * vec4<f32>(world + n * u.sh.y * 1.5, 1.0);
    let c = vec2<f32>(q.x * 0.5 + 0.5, 0.5 - q.y * 0.5);
    if (any(c < vec2<f32>(0.0)) || any(c > vec2<f32>(1.0)) || q.z > 1.0) { return 1.0; }
    let dims = vec2<f32>(textureDimensions(sh_own));
    let r = q.z - u.sh.z;
    var s = 0.0;
    for (var k = 0; k < 4; k++) {
        let o = (vec2<f32>(f32(k & 1), f32(k >> 1u)) - 0.5) * 1.5 / dims;
        var a = textureSampleCompareLevel(sh_own, sh_cmp, c + o, r);
        if (u.sh.w > 0.5) { a = min(a, textureSampleCompareLevel(sh_others, sh_cmp, c + o, r)); }
        s += a;
    }
    return s * 0.25;
}

@fragment
fn fs(i: VO, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    // Les textures se lisent avant tout branchement qui dépend du pixel
    let tc = textureSample(t_color, t_samp, i.uv).rgb;
    let to = textureSample(t_orm, t_samp, i.uv).rg;
    // Caché par l'image calculée ailleurs : rien à dessiner ici
    if (u.soft.z > 0.5) {
        let q = vec2<i32>(i.clip.xy / u.size.xy * u.soft.xy);
        let d = textureLoad(soft, clamp(q, vec2<i32>(0), vec2<i32>(u.soft.xy) - vec2<i32>(1)), 0).r;
        if (i.vd > d + u.soft.w) { discard; }
    }
    var albedo = i.color;
    var rough = u.mat.x;
    var metal = u.mat.y;
    if (u.mat.z > 0.5) {
        albedo = tc;
        rough = to.x;
        metal = to.y;
    }
    rough = clamp(rough, 0.06, 1.0);
    var n = normalize(i.nrm);
    let v = normalize(u.eye.xyz - i.world);
    // Les deux faces sont éclairées : la normale regarde toujours l'œil
    if (dot(n, v) < 0.0) { n = -n; }
    let l = normalize(u.light.xyz);
    let ndl = dot(n, l);
    var shade = 1.0;
    if (ndl > 0.0) { shade = lit(i.world, n); }
    let diffuse = max(ndl, 0.0) * shade;
    let sky = 0.5 + 0.5 * n.y;
    // Reflet de la lumière (GGX), plus large et plus faible quand la surface est rugueuse
    let h = normalize(l + v);
    let ndh = max(dot(n, h), 0.0);
    let ndv = max(dot(n, v), 1e-3);
    let a2 = rough * rough * rough * rough;
    let dd = ndh * ndh * (a2 - 1.0) + 1.0;
    let ggx = a2 / (3.14159 * dd * dd);
    let kk = (rough + 1.0) * (rough + 1.0) / 8.0;
    let vis = 1.0 / ((max(ndl, 1e-3) * (1.0 - kk) + kk) * (ndv * (1.0 - kk) + kk) * 4.0);
    let f0 = mix(vec3<f32>(0.04), albedo, metal);
    let fres = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - max(dot(h, v), 0.0), 5.0);
    let spec = fres * min(ggx * vis, 8.0) * diffuse;
    // Un métal reflète le ciel : sans cela il serait noir
    let rv = reflect(-v, n);
    let fenv = f0 + (max(vec3<f32>(1.0 - rough), f0) - f0) * pow(1.0 - ndv, 5.0);
    // Le dos d'un tissu est un peu plus sombre : on lit les plis
    var back = 1.0;
    if (!front) { back = 0.82; }
    if (u.sky_up.w > 0.5) {
        // L'éclairage de la scène, comme le rasteriseur : le ciel en lumière diffuse, le soleil,
        // puis la même courbe de tons
        let amb = mix(u.sky_down.rgb, u.sky_up.rgb, clamp(0.5 + 0.5 * n.y, 0.0, 1.0));
        let env = mix(u.sky_down.rgb, u.sky_up.rgb, clamp(0.5 + 0.5 * rv.y, 0.0, 1.0));
        let sun = u.sun.rgb;
        let lin = (albedo * (1.0 - metal) * (amb + sun * diffuse) + spec * sun * 0.5) * back + env * fenv * mix(1.0, 0.25, rough) * mix(0.35, 1.0, metal) * back;
        return vec4<f32>(tone(lin * u.sky_down.w), 1.0);
    }
    let env = mix(vec3<f32>(0.20, 0.19, 0.18), vec3<f32>(0.62, 0.68, 0.78), clamp(0.5 + 0.5 * rv.y, 0.0, 1.0));
    let lin = albedo * (1.0 - metal) * (0.22 + 0.16 * sky + 0.72 * diffuse) * back + spec * 0.72 + env * fenv * mix(1.0, 0.25, rough) * mix(0.35, 1.0, metal) * back;
    // L'interface attend des couleurs déjà en gamma
    let g = pow(clamp(lin, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / 2.2));
    return vec4<f32>(g, 1.0);
}

// Courbe « filmique » (ACES approchée) puis gamma : celle du rasteriseur de la vue
fn tone(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    let y = (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14);
    return pow(clamp(y, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / 2.2));
}

struct PO {
    @builtin(position) clip: vec4<f32>,
    @location(0) vd: f32,
}

// Identifiants : la position seule
@vertex
fn vs_ids(@location(0) p: vec3<f32>) -> PO {
    var o: PO;
    let w = u.model * vec4<f32>(p + u.origin.xyz, 1.0);
    o.clip = u.vp * w;
    o.vd = dot(u.depth_row, w);
    return o;
}

@fragment
fn fs_ids(i: PO) -> @location(0) vec4<u32> {
    if (u.soft.z > 0.5) {
        let q = vec2<i32>(i.clip.xy / u.size.xy * u.soft.xy);
        let d = textureLoad(soft, clamp(q, vec2<i32>(0), vec2<i32>(u.soft.xy) - vec2<i32>(1)), 0).r;
        if (i.vd > d + u.soft.w) { discard; }
    }
    // Des entiers : la profondeur telle quelle (ses bits) et le code, sans arrondi ni mélange
    return vec4<u32>(bitcast<u32>(i.vd), u32(u.ids.x + 0.5), 0u, 0u);
}

// Carte d'ombre : le maillage vu de la lumière
@vertex
fn vs_light(@location(0) p: vec3<f32>) -> @builtin(position) vec4<f32> {
    return u.light_vp * (u.model * vec4<f32>(p + u.origin.xyz, 1.0));
}
"#;

/// Ce qui se pose sur l'image après le maillage : son ombre sur ce qui a été calculé ailleurs
/// (dessous), et le contour de sélection (dessus).
const OVERLAY: &str = r#"
struct O {
    inv_vp: mat4x4<f32>,
    light_vp: mat4x4<f32>,
    depth_row: vec4<f32>,
    light: vec4<f32>,
    soft: vec4<f32>,
    // Taille de l'image, épaisseur du contour
    size: vec4<f32>,
    // Sol : hauteur, présent, force de l'ombre au sol, force sur les surfaces
    floor: vec4<f32>,
    sh: vec4<f32>,
}
@group(0) @binding(0) var<uniform> u: O;
@group(0) @binding(1) var soft: texture_2d<f32>;
@group(0) @binding(2) var ids: texture_2d<u32>;
@group(0) @binding(3) var sh_own: texture_depth_2d;
@group(0) @binding(4) var sh_others: texture_depth_2d;
@group(0) @binding(5) var sh_cmp: sampler_comparison;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    return vec4<f32>(p, 0.0, 1.0);
}

// Le point du monde montré par un texel de l'image calculée ailleurs (w = 0 : le fond)
fn surface(q: vec2<i32>) -> vec4<f32> {
    let n = vec2<i32>(u.soft.xy);
    if (any(q < vec2<i32>(0)) || any(q >= n)) { return vec4<f32>(0.0); }
    let d = textureLoad(soft, q, 0).r;
    if (d > 1e29) { return vec4<f32>(0.0); }
    let c = (vec2<f32>(q) + 0.5) / u.soft.xy;
    let ndc = vec2<f32>(c.x * 2.0 - 1.0, 1.0 - c.y * 2.0);
    let a4 = u.inv_vp * vec4<f32>(ndc, 0.0, 1.0);
    let b4 = u.inv_vp * vec4<f32>(ndc, 1.0, 1.0);
    let o = a4.xyz / a4.w;
    let dir = normalize(b4.xyz / b4.w - o);
    let dd = dot(u.depth_row.xyz, dir);
    if (abs(dd) < 1e-9) { return vec4<f32>(0.0); }
    return vec4<f32>(o + dir * ((d - dot(u.depth_row, vec4<f32>(o, 1.0))) / dd), 1.0);
}

// Part de ce point cachée de la lumière par les maillages dessinés ici, là où les autres objets
// ne la cachent pas déjà (leur ombre est dans l'image calculée ailleurs)
fn hidden(world: vec3<f32>, n: vec3<f32>) -> f32 {
    let q = u.light_vp * vec4<f32>(world + n * u.sh.y * 2.0, 1.0);
    let c = vec2<f32>(q.x * 0.5 + 0.5, 0.5 - q.y * 0.5);
    if (any(c < vec2<f32>(0.0)) || any(c > vec2<f32>(1.0)) || q.z > 1.0) { return 0.0; }
    let dims = vec2<f32>(textureDimensions(sh_own));
    let r = q.z - u.sh.z;
    var s = 0.0;
    for (var k = 0; k < 9; k++) {
        let o = (vec2<f32>(f32(k % 3), f32(k / 3)) - 1.0) * 1.4 / dims;
        var a = 1.0 - textureSampleCompareLevel(sh_own, sh_cmp, c + o, r);
        if (u.sh.w > 0.5) { a *= textureSampleCompareLevel(sh_others, sh_cmp, c + o, r); }
        s += a;
    }
    return s / 9.0;
}

@fragment
fn fs_under(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    // Sur le maillage lui-même : son ombre propre est déjà dans son éclairage
    if (textureLoad(ids, vec2<i32>(pos.xy), 0).g > 0u) { return vec4<f32>(0.0); }
    let l = normalize(u.light.xyz);
    var k = 0.0;
    var world = vec3<f32>(0.0);
    var n = l;
    var found = false;
    if (u.soft.z > 0.5) {
        let q = vec2<i32>(pos.xy / u.size.xy * u.soft.xy);
        let p = surface(q);
        if (p.w > 0.5) {
            found = true;
            world = p.xyz;
            k = u.floor.w;
            // La normale de la surface, d'après ses voisins dans l'image
            let px = surface(q + vec2<i32>(1, 0));
            let py = surface(q + vec2<i32>(0, 1));
            if (px.w > 0.5 && py.w > 0.5) {
                let c = cross(px.xyz - p.xyz, py.xyz - p.xyz);
                if (dot(c, c) > 1e-24) {
                    n = normalize(c);
                    // Une surface qui tourne le dos à la lumière est déjà sombre
                    let ndl = abs(dot(n, l));
                    if (dot(n, l) < 0.0) { n = -n; }
                    k *= smoothstep(0.02, 0.3, ndl);
                }
            }
        }
    }
    if (!found && u.floor.y > 0.5) {
        let ndc = vec2<f32>(pos.x / u.size.x * 2.0 - 1.0, 1.0 - pos.y / u.size.y * 2.0);
        let a4 = u.inv_vp * vec4<f32>(ndc, 0.0, 1.0);
        let b4 = u.inv_vp * vec4<f32>(ndc, 1.0, 1.0);
        let o = a4.xyz / a4.w;
        let dir = normalize(b4.xyz / b4.w - o);
        if (abs(dir.y) > 1e-5 && l.y > 0.0) {
            let t = (u.floor.x - o.y) / dir.y;
            if (t > 0.0) {
                found = true;
                world = o + dir * t;
                n = vec3<f32>(0.0, 1.0, 0.0);
                k = u.floor.z;
            }
        }
    }
    if (!found || k <= 0.0) { return vec4<f32>(0.0); }
    return vec4<f32>(0.0, 0.0, 0.0, k * hidden(world, n));
}

fn chosen(q: vec2<i32>) -> u32 {
    if (any(q < vec2<i32>(0)) || any(q >= vec2<i32>(u.size.xy))) { return 0u; }
    return textureLoad(ids, q, 0).g & 3u;
}

@fragment
fn fs_over(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let q = vec2<i32>(pos.xy);
    let own = chosen(q);
    var best = 0u;
    for (var k = 0; k < 16; k++) {
        let a = f32(k & 7) * 0.7853982;
        let r = u.size.z * select(1.0, 0.5, k > 7);
        best = max(best, chosen(q + vec2<i32>(round(vec2<f32>(cos(a), sin(a)) * r))));
    }
    if (best <= own) { return vec4<f32>(0.0); }
    // Orange clair pour l'objet actif, plus foncé pour les autres : comme le reste de la vue
    if (best >= 2u) { return vec4<f32>(1.0, 0.667, 0.251, 1.0); }
    return vec4<f32>(0.922, 0.431, 0.078, 1.0);
}
"#;

/// Octets des réglages du dessin (`U`) et de la passe d'après (`O`)
const U_SIZE: u64 = 64 * 3 + 16 * 13;
const O_SIZE: u64 = 64 * 2 + 16 * 6;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Image des identifiants : les bits de la profondeur (rouge) et le code du morceau (vert). Des
/// entiers : toutes les cartes savent y dessiner, ce qui n'est pas vrai des nombres à virgule.
const IDS: wgpu::TextureFormat = wgpu::TextureFormat::Rg32Uint;

struct Programs {
    normals: Kernels,
    layout: wgpu::BindGroupLayout,
    /// Branchement réduit aux réglages : pour les cartes d'ombre, qui ne peuvent pas se lire elles-mêmes
    layout_light: wgpu::BindGroupLayout,
    layout_over: wgpu::BindGroupLayout,
    /// Un pipeline par nombre d'échantillons (1 ou 4)
    pipelines: [wgpu::RenderPipeline; 2],
    ids: wgpu::RenderPipeline,
    light: wgpu::RenderPipeline,
    under: wgpu::RenderPipeline,
    over: wgpu::RenderPipeline,
    /// Textures d'un pixel, branchées quand il n'y a rien d'autre
    blank: wgpu::TextureView,
    blank_depth: wgpu::TextureView,
    blank_color: wgpu::TextureView,
    compare: wgpu::Sampler,
    sampler: wgpu::Sampler,
    /// Lissage ×4 permis par la carte ?
    msaa: bool,
}

fn programs(g: &Gpu) -> &'static Programs {
    static P: OnceLock<Programs> = OnceLock::new();
    P.get_or_init(|| {
        use Bind::{Read, Uniform, Write};
        let d = &g.device;
        let normals = g.kernels("dessin-normales", NORMALS, &[Uniform, Read, Read, Read, Write], &["normals"]);
        let module = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("dessin-maillage"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let overlay = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("dessin-apres"), source: wgpu::ShaderSource::Wgsl(OVERLAY.into()) });
        let uniform = |binding: u32, visibility| wgpu::BindGroupLayoutEntry { binding, visibility, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None };
        let texture = |binding: u32, sample_type| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None };
        let sampler = |binding: u32, ty| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(ty), count: None };
        let raw = wgpu::TextureSampleType::Float { filterable: false };
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dessin-maillage"),
            entries: &[
                uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                texture(1, raw),
                texture(2, wgpu::TextureSampleType::Depth),
                texture(3, wgpu::TextureSampleType::Depth),
                sampler(4, wgpu::SamplerBindingType::Comparison),
                texture(5, wgpu::TextureSampleType::Float { filterable: true }),
                texture(6, wgpu::TextureSampleType::Float { filterable: true }),
                sampler(7, wgpu::SamplerBindingType::Filtering),
            ],
        });
        let layout_light = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("dessin-ombre"), entries: &[uniform(0, wgpu::ShaderStages::VERTEX)] });
        let layout_over = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dessin-apres"),
            entries: &[uniform(0, wgpu::ShaderStages::FRAGMENT), texture(1, raw), texture(2, wgpu::TextureSampleType::Uint), texture(3, wgpu::TextureSampleType::Depth), texture(4, wgpu::TextureSampleType::Depth), sampler(5, wgpu::SamplerBindingType::Comparison)],
        });
        let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("dessin-maillage"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pl_light = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("dessin-ombre"), bind_group_layouts: &[Some(&layout_light)], immediate_size: 0 });
        let pl_over = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("dessin-apres"), bind_group_layouts: &[Some(&layout_over)], immediate_size: 0 });
        // Positions et normales : des `vec4`, dont on lit les trois premiers nombres
        let positions = wgpu::VertexBufferLayout { array_stride: 16, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x3] };
        let depth_state = |bias: wgpu::DepthBiasState| Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(true), depth_compare: Some(wgpu::CompareFunction::Less), stencil: Default::default(), bias });
        let pipeline = |count: u32| {
            d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("dessin-maillage"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    buffers: &[
                        Some(positions.clone()),
                        Some(wgpu::VertexBufferLayout { array_stride: 16, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![1 => Float32x3] }),
                        Some(wgpu::VertexBufferLayout { array_stride: 4, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![2 => Unorm8x4] }),
                        Some(wgpu::VertexBufferLayout { array_stride: 8, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![3 => Float32x2] }),
                    ],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
                depth_stencil: depth_state(Default::default()),
                multisample: wgpu::MultisampleState { count, mask: !0, alpha_to_coverage_enabled: false },
                fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some("fs"), targets: &[Some(wgpu::ColorTargetState { format: FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
                multiview_mask: None,
                cache: None,
            })
        };
        let ids = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("dessin-identifiants"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_ids"), buffers: &[Some(positions.clone())], compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: depth_state(Default::default()),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some("fs_ids"), targets: &[Some(wgpu::ColorTargetState { format: IDS, blend: None, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            multiview_mask: None,
            cache: None,
        });
        // La carte d'ombre : la profondeur seule, un peu repoussée selon la pente
        let light = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("dessin-ombre"),
            layout: Some(&pl_light),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_light"), buffers: &[Some(positions.clone())], compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: depth_state(wgpu::DepthBiasState { constant: 2, slope_scale: 1.5, clamp: 0.0 }),
            multisample: Default::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        let after = |entry: &str, blend: wgpu::BlendState| {
            d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("dessin-apres"),
                layout: Some(&pl_over),
                vertex: wgpu::VertexState { module: &overlay, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState { module: &overlay, entry_point: Some(entry), targets: &[Some(wgpu::ColorTargetState { format: FORMAT, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
                multiview_mask: None,
                cache: None,
            })
        };
        // Dessous : l'ombre ne se voit que là où le maillage ne couvre pas le pixel
        let below = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::OneMinusDstAlpha, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
        let under = after("fs_under", wgpu::BlendState { color: below, alpha: below });
        let over = after("fs_over", wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let one = |label: &str, format, usage| d.create_texture(&wgpu::TextureDescriptor { label: Some(label), size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[] }).create_view(&Default::default());
        let blank = one("dessin-sans-profondeur", wgpu::TextureFormat::R32Float, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        let blank_depth = one("dessin-sans-ombre", DEPTH, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT);
        let blank_color = one("dessin-sans-texture", wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        let compare = d.create_sampler(&wgpu::SamplerDescriptor { label: Some("dessin-ombre"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, compare: Some(wgpu::CompareFunction::LessEqual), ..Default::default() });
        let sampler = d.create_sampler(&wgpu::SamplerDescriptor { label: Some("dessin-texture"), address_mode_u: wgpu::AddressMode::Repeat, address_mode_v: wgpu::AddressMode::Repeat, mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, mipmap_filter: wgpu::MipmapFilterMode::Linear, ..Default::default() });
        // Le lissage ×4 est garanti par WebGPU, pas par tous les pilotes OpenGL : on l'a demandé à la carte
        let msaa = g.msaa4;
        Programs { normals, layout, layout_light, layout_over, pipelines: [pipeline(1), pipeline(4)], ids, light, under, over, blank, blank_depth, blank_color, compare, sampler, msaa }
    })
}

/// Ce qu'un morceau garde sur la carte d'une image à l'autre
struct Mesh {
    /// Clé de la topologie, et numéro du tissu (0 : positions venues du processeur)
    key: (u64, u64),
    n: usize,
    index: wgpu::Buffer,
    index_count: u32,
    normals: wgpu::Buffer,
    colors: wgpu::Buffer,
    colors_for: Option<u64>,
    uv: wgpu::Buffer,
    uv_for: Option<u64>,
    /// Positions envoyées par le processeur (`Source::Data`), et leur version
    own_pos: Option<(wgpu::Buffer, u64)>,
    off: wgpu::Buffer,
    adj: wgpu::Buffer,
    count: wgpu::Buffer,
    uniform: wgpu::Buffer,
    /// Les réglages seuls, pour la carte d'ombre
    light: wgpu::BindGroup,
    /// Branchement du noyau des normales, et le tampon de positions pour lequel il est fait
    group: Option<(wgpu::BindGroup, wgpu::Buffer)>,
    used: u64,
}

static MESHES: Mutex<Vec<Mesh>> = Mutex::new(Vec::new());
static CLOCK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Les textures d'un matériau cuit, sur la carte : clé, couleur, rugosité et métal, dernière image qui s'en est servie
static TEXTURES: Mutex<Vec<(u64, wgpu::TextureView, wgpu::TextureView, u64)>> = Mutex::new(Vec::new());

/// Envoie une texture carrée et ses niveaux réduits.
fn upload(g: &Gpu, label: &str, format: wgpu::TextureFormat, n: u32, px: &[[u8; 4]]) -> wgpu::TextureView {
    let mut levels = vec![px.to_vec()];
    let mut side = n as usize;
    while let Some(next) = half_size(levels.last().map(|l| &l[..]).unwrap_or(&[]), side) {
        side /= 2;
        levels.push(next);
    }
    let t = g.device.create_texture(&wgpu::TextureDescriptor { label: Some(label), size: wgpu::Extent3d { width: n, height: n, depth_or_array_layers: 1 }, mip_level_count: levels.len() as u32, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[] });
    let mut side = n;
    for (level, data) in levels.iter().enumerate() {
        g.queue.write_texture(wgpu::TexelCopyTextureInfo { texture: &t, mip_level: level as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All }, gpusim::bytes_of(data), wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(side * 4), rows_per_image: Some(side) }, wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 1 });
        side = (side / 2).max(1);
    }
    t.create_view(&Default::default())
}

/// Les textures d'un matériau cuit, envoyées à la carte la première fois qu'on le voit.
fn textures(g: &Gpu, b: &Baked, clock: u64) -> Option<(wgpu::TextureView, wgpu::TextureView)> {
    if b.n == 0 || b.n > 8192 || b.color.len() != (b.n * b.n) as usize || b.orm.len() != b.color.len() {
        return None;
    }
    let mut all = TEXTURES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(t) = all.iter_mut().find(|t| t.0 == b.key) {
        t.3 = clock;
        return Some((t.1.clone(), t.2.clone()));
    }
    // Quelques matériaux restent prêts ; le plus ancien cède sa place
    if all.len() >= 8 {
        if let Some(old) = (0..all.len()).min_by_key(|k| all[*k].3) {
            all.swap_remove(old);
        }
    }
    let color = upload(g, "dessin-texture-couleur", wgpu::TextureFormat::Rgba8UnormSrgb, b.n, &b.color);
    let orm = upload(g, "dessin-texture-rugosite", wgpu::TextureFormat::Rgba8Unorm, b.n, &b.orm);
    all.push((b.key, color.clone(), orm.clone(), clock));
    Some((color, orm))
}

impl Mesh {
    /// Tampons d'une topologie. `tris` : dans l'ordre de la géométrie ; `inv` : rang de chaque
    /// sommet dans le tampon de positions (un tissu range ses sommets autrement).
    fn new(g: &Gpu, key: (u64, u64), n: usize, tris: &[[u32; 3]], inv: Option<&[u32]>) -> Option<Mesh> {
        let rank = |v: u32| inv.map_or(v, |inv| inv.get(v as usize).copied().unwrap_or(0));
        let tris: Vec<[u32; 3]> = tris.iter().filter(|t| t.iter().all(|v| (*v as usize) < n)).map(|t| [rank(t[0]), rank(t[1]), rank(t[2])]).collect();
        if n == 0 || tris.is_empty() {
            return None;
        }
        // Voisinage : pour chaque sommet, les deux autres sommets de chacun de ses triangles
        let mut off = vec![0u32; n + 1];
        for t in &tris {
            for v in t {
                off[*v as usize + 1] += 1;
            }
        }
        for i in 0..n {
            off[i + 1] += off[i];
        }
        let mut at = off.clone();
        let mut adj = vec![[0u32; 2]; tris.len() * 3];
        for t in &tris {
            for k in 0..3 {
                let v = t[k] as usize;
                adj[at[v] as usize] = [t[(k + 1) % 3], t[(k + 2) % 3]];
                at[v] += 1;
            }
        }
        let limit = g.max_binding.min(g.max_buffer);
        if (adj.len() * 8) as u64 > limit || (n * 16) as u64 > limit {
            return None;
        }
        let d = &g.device;
        let make = |label: &str, bytes: &[u8], usage: wgpu::BufferUsages| {
            let b = d.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: (bytes.len() as u64).next_multiple_of(4).max(16), usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            if !bytes.is_empty() {
                g.queue.write_buffer(&b, 0, bytes);
            }
            b
        };
        let index = make("dessin-triangles", gpusim::bytes_of(&tris), wgpu::BufferUsages::INDEX);
        let off_b = make("dessin-voisinage-debuts", gpusim::bytes_of(&off), wgpu::BufferUsages::STORAGE);
        let adj_b = make("dessin-voisinage", gpusim::bytes_of(&adj), wgpu::BufferUsages::STORAGE);
        let normals = d.create_buffer(&wgpu::BufferDescriptor { label: Some("dessin-normales"), size: (n * 16) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX, mapped_at_creation: false });
        let colors = make("dessin-couleurs", &vec![255u8; n * 4], wgpu::BufferUsages::VERTEX);
        // Les UV : à zéro tant que le morceau n'en donne pas
        let uv = d.create_buffer(&wgpu::BufferDescriptor { label: Some("dessin-uv"), size: ((n * 8) as u64).max(16), usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let count = g.uniform("dessin-nombre", 16);
        g.queue.write_buffer(&count, 0, gpusim::bytes_of(&[n as u32, 0, 0, 0]));
        let uniform = g.uniform("dessin-reglages", U_SIZE);
        let light = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &programs(g).layout_light, entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }] });
        Some(Mesh { key, n, index, index_count: (tris.len() * 3) as u32, normals, colors, colors_for: None, uv, uv_for: None, own_pos: None, off: off_b, adj: adj_b, count, uniform, light, group: None, used: 0 })
    }
}

/// Les autres objets de la scène sur la carte, pour leur ombre
struct Occ {
    version: u64,
    pos: wgpu::Buffer,
    index: wgpu::Buffer,
    index_count: u32,
    uniform: wgpu::Buffer,
    group: wgpu::BindGroup,
}

/// La cible du dessin d'une vue : sa texture de couleur, sa profondeur, et l'image que l'interface affiche.
pub struct Canvas {
    size: [u32; 2],
    samples: u32,
    color: Option<wgpu::Texture>,
    view: Option<wgpu::TextureView>,
    msaa: Option<wgpu::TextureView>,
    depth: Option<wgpu::TextureView>,
    id: Option<egui::TextureId>,
    /// Profondeur extérieure : texture, taille, version
    soft: Option<(wgpu::Texture, wgpu::TextureView, [u32; 2], u64)>,
    /// Image des identifiants, sa profondeur, et si le dernier dessin l'a remplie
    ids: Option<(wgpu::Texture, wgpu::TextureView, wgpu::TextureView)>,
    ids_drawn: bool,
    /// Carte d'ombre des maillages dessinés ici
    shadow_own: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// Carte d'ombre des autres objets, et ce pour quoi elle a été faite (objets, lumière)
    shadow_others: Option<(wgpu::Texture, wgpu::TextureView, u64)>,
    occ: Option<Occ>,
    after: Option<wgpu::Buffer>,
}

impl Default for Canvas {
    fn default() -> Self {
        Canvas { size: [0, 0], samples: 1, color: None, view: None, msaa: None, depth: None, id: None, soft: None, ids: None, ids_drawn: false, shadow_own: None, shadow_others: None, occ: None, after: None }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        if let (Some(id), Some(ui)) = (self.id.take(), crate::gfx::ui()) {
            ui.renderer.write().free_texture(&id);
        }
    }
}

/// Attend qu'un tampon soit lisible et le donne à `f`. Rien si la carte rend une erreur.
fn mapped<T>(g: &Gpu, out: &wgpu::Buffer, f: impl FnOnce(&[u8]) -> T) -> Option<T> {
    let done = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
    let d2 = done.clone();
    out.slice(..).map_async(wgpu::MapMode::Read, move |r| d2.store(if r.is_ok() { 1 } else { 2 }, std::sync::atomic::Ordering::Release));
    while done.load(std::sync::atomic::Ordering::Acquire) == 0 {
        if g.broken() || g.device.poll(wgpu::PollType::wait_indefinitely()).is_err() {
            return None;
        }
    }
    if done.load(std::sync::atomic::Ordering::Acquire) != 1 {
        return None;
    }
    let data = out.slice(..).get_mapped_range().ok()?;
    Some(f(&data))
}

impl Canvas {
    fn ensure(&mut self, g: &Gpu, size: [u32; 2], samples: u32) {
        if self.size == size && self.samples == samples && self.color.is_some() {
            return;
        }
        let d = &g.device;
        let extent = wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 };
        let tex = |label: &str, format, count: u32, usage| d.create_texture(&wgpu::TextureDescriptor { label: Some(label), size: extent, mip_level_count: 1, sample_count: count, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[] });
        let color = tex("dessin-couleur", FORMAT, 1, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC);
        let view = color.create_view(&Default::default());
        self.msaa = (samples > 1).then(|| tex("dessin-couleur-lissee", FORMAT, samples, wgpu::TextureUsages::RENDER_ATTACHMENT).create_view(&Default::default()));
        self.depth = Some(tex("dessin-profondeur", DEPTH, samples, wgpu::TextureUsages::RENDER_ATTACHMENT).create_view(&Default::default()));
        // L'image des identifiants n'est créée que si un dessin la demande
        if let Some(old) = self.ids.take() {
            old.0.destroy();
        }
        self.ids_drawn = false;
        // L'image montrée par l'interface suit la nouvelle texture
        if let Some(ui) = crate::gfx::ui().filter(|_| g.shared) {
            let mut r = ui.renderer.write();
            match self.id {
                Some(id) => r.update_egui_texture_from_wgpu_texture(d, &view, wgpu::FilterMode::Linear, id),
                None => self.id = Some(r.register_native_texture(d, &view, wgpu::FilterMode::Linear)),
            }
        }
        if let Some(old) = self.color.replace(color) {
            old.destroy();
        }
        self.view = Some(view);
        self.size = size;
        self.samples = samples;
    }

    /// L'image à montrer dans l'interface (rien tant que rien n'a été dessiné, ou sans interface sur cette carte)
    pub fn texture(&self) -> Option<egui::TextureId> {
        self.id
    }

    pub fn size(&self) -> [u32; 2] {
        self.size
    }

    /// Ce que le dernier dessin a posé devant le reste : par pixel, sa profondeur (1e30 : rien)
    /// et son code. Pour qu'un autre dessin de la carte (la fumée) s'y arrête.
    pub fn front(&self) -> Option<wgpu::TextureView> {
        self.ids.as_ref().filter(|_| self.ids_drawn).map(|i| i.1.clone())
    }

    /// Le numéro du morceau visible sous un pixel du dernier dessin (`Part::id`), et sa
    /// profondeur. Relit un seul pixel : à appeler au clic, pas à chaque image.
    pub fn pick(&self, x: u32, y: u32) -> Option<(u32, f32)> {
        let (g, ids) = (gpusim::gpu()?, self.ids.as_ref().filter(|_| self.ids_drawn)?);
        if x >= self.size[0] || y >= self.size[1] {
            return None;
        }
        let out = g.device.create_buffer(&wgpu::BufferDescriptor { label: Some("dessin-pointage"), size: 256, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mut enc = g.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &ids.0, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &out, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256), rows_per_image: Some(1) } },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        g.queue.submit([enc.finish()]);
        let (depth, code) = mapped(g, &out, |d| (f32::from_le_bytes([d[0], d[1], d[2], d[3]]), u32::from_le_bytes([d[4], d[5], d[6], d[7]])))?;
        let (id, _) = id_decode(code);
        (id > 0).then_some((id, depth))
    }

    /// Relit l'image dessinée (RGBA, 8 bits, déjà en gamma). Pour les tests et le banc : l'interface
    /// n'en a jamais besoin.
    pub fn pixels(&self) -> Option<Vec<u8>> {
        let (g, color) = (gpusim::gpu()?, self.color.as_ref()?);
        let [w, h] = self.size;
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let out = g.device.create_buffer(&wgpu::BufferDescriptor { label: Some("dessin-relecture"), size: (row * h) as u64, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mut enc = g.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &out, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        g.queue.submit([enc.finish()]);
        mapped(g, &out, |data| {
            let mut px = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h {
                px.extend_from_slice(&data[(y * row) as usize..][..(w * 4) as usize]);
            }
            px
        })
    }
}

/// Peut-on dessiner sur la carte ? (Une carte de calcul est ouverte, et elle n'a pas rendu d'erreur.)
pub fn available() -> bool {
    gpusim::gpu().is_some()
}

/// Peut-on montrer ce dessin dans l'interface sans relecture ? Il faut que l'interface et les
/// simulations soient sur la même carte (affichage wgpu, voir `gfx`).
pub fn shared() -> bool {
    gpusim::gpu().is_some_and(|g| g.shared)
}

/// Côté des cartes d'ombre, en texels
fn shadow_side(g: &Gpu) -> u32 {
    if g.soft { 1024 } else { 2048 }
}

/// Dessine les morceaux dans la cible, à la taille donnée (en pixels). Rend faux si rien n'a pu
/// être dessiné (pas de carte, maillage trop gros pour elle) : l'appelant dessine alors autrement.
pub fn paint(canvas: &mut Canvas, size: [u32; 2], cam: &Cam, parts: &[Part]) -> bool {
    let Some(g) = gpusim::gpu() else { return false };
    let size = [size[0].clamp(1, 8192), size[1].clamp(1, 8192)];
    let progs = programs(g);
    let samples = if progs.msaa { 4 } else { 1 };
    canvas.ensure(g, size, samples);
    let (Some(view), Some(depth)) = (canvas.view.clone(), canvas.depth.clone()) else { return false };
    let d = &g.device;
    // Profondeur extérieure : renvoyée seulement quand elle change
    let mut soft_u = [0.0f32; 4];
    if let Some(s) = cam.soft.as_ref().filter(|s| s.w > 0 && s.h > 0) {
        if canvas.soft.as_ref().is_none_or(|c| c.2 != [s.w, s.h]) {
            let t = d.create_texture(&wgpu::TextureDescriptor { label: Some("dessin-profondeur-exterieure"), size: wgpu::Extent3d { width: s.w, height: s.h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::R32Float, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[] });
            let v = t.create_view(&Default::default());
            if let Some(old) = canvas.soft.replace((t, v, [s.w, s.h], u64::MAX)) {
                old.0.destroy();
            }
        }
        if let Some(c) = canvas.soft.as_mut() {
            if c.3 != s.version {
                let data = (s.depth)();
                if data.len() == (s.w * s.h) as usize {
                    g.queue.write_texture(wgpu::TexelCopyTextureInfo { texture: &c.0, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All }, gpusim::bytes_of(&data), wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(s.w * 4), rows_per_image: Some(s.h) }, wgpu::Extent3d { width: s.w, height: s.h, depth_or_array_layers: 1 });
                    c.3 = s.version;
                }
            }
            if c.3 == s.version {
                soft_u = [s.w as f32, s.h as f32, 1.0, s.bias];
            }
        }
    }
    let soft_view = match (&canvas.soft, soft_u[2] > 0.5) {
        (Some(c), true) => c.1.clone(),
        _ => progs.blank.clone(),
    };
    // Image des identifiants : pour le clic, le contour, les ombres posées ailleurs, la fumée
    let want_ids = cam.shadow.is_some() || parts.iter().any(|p| p.id != 0 || p.select != 0);
    if want_ids && canvas.ids.is_none() {
        let extent = wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 };
        let t = d.create_texture(&wgpu::TextureDescriptor { label: Some("dessin-identifiants"), size: extent, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: IDS, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
        let z = d.create_texture(&wgpu::TextureDescriptor { label: Some("dessin-identifiants-profondeur"), size: extent, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: DEPTH, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[] });
        canvas.ids = Some((t.clone(), t.create_view(&Default::default()), z.create_view(&Default::default())));
    }
    // Cartes d'ombre
    let side = shadow_side(g);
    let mut sh_u = [0.0f32; 4];
    let mut light_vp = IDENTITY;
    let mut others_due = false;
    if let Some(s) = &cam.shadow {
        let radius = s.radius.max(1e-6);
        light_vp = light_matrix(cam.light, s.center, radius);
        let texel = 2.0 * radius / side as f32;
        sh_u = [1.0, texel, 0.6 * texel / (2.0 * radius), 0.0];
        let map = |label: &str| {
            let t = d.create_texture(&wgpu::TextureDescriptor { label: Some(label), size: wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: DEPTH, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] });
            let v = t.create_view(&Default::default());
            (t, v)
        };
        if canvas.shadow_own.is_none() {
            canvas.shadow_own = Some(map("dessin-ombre-maillages"));
        }
        if let Some(o) = &s.occluders {
            if canvas.occ.as_ref().is_none_or(|c| c.version != o.version) {
                let (pos, tris) = (o.mesh)();
                let n = pos.len() as u32;
                let tris: Vec<[u32; 3]> = tris.into_iter().filter(|t| t.iter().all(|v| *v < n)).collect();
                let limit = g.max_buffer;
                canvas.occ = None;
                if !tris.is_empty() && (pos.len() * 16) as u64 <= limit && (tris.len() * 12) as u64 <= limit {
                    let make = |label: &str, bytes: &[u8], usage: wgpu::BufferUsages| {
                        let b = d.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: (bytes.len() as u64).next_multiple_of(4).max(16), usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
                        g.queue.write_buffer(&b, 0, bytes);
                        b
                    };
                    let uniform = g.uniform("dessin-ombre-reglages", U_SIZE);
                    let group = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &progs.layout_light, entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }] });
                    canvas.occ = Some(Occ { version: o.version, pos: make("dessin-ombre-sommets", gpusim::bytes_of(&pos), wgpu::BufferUsages::VERTEX), index: make("dessin-ombre-triangles", gpusim::bytes_of(&tris), wgpu::BufferUsages::INDEX), index_count: (tris.len() * 3) as u32, uniform, group });
                }
            }
            if let Some(occ) = &canvas.occ {
                // Refaite quand les objets, leur place ou la lumière changent
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                (occ.version, light_vp.as_flattened().iter().map(|x| x.to_bits()).collect::<Vec<_>>(), o.model.as_flattened().iter().map(|x| x.to_bits()).collect::<Vec<_>>()).hash(&mut h);
                let key = h.finish();
                if canvas.shadow_others.is_none() {
                    let (t, v) = map("dessin-ombre-objets");
                    canvas.shadow_others = Some((t, v, !key));
                }
                if let Some(m) = canvas.shadow_others.as_mut() {
                    if m.2 != key {
                        m.2 = key;
                        others_due = true;
                        let mut u = [0.0f32; (U_SIZE / 4) as usize];
                        u[16..32].copy_from_slice(o.model.as_flattened());
                        u[32..48].copy_from_slice(light_vp.as_flattened());
                        g.queue.write_buffer(&occ.uniform, 0, gpusim::bytes_of(&u));
                    }
                }
                sh_u[3] = 1.0;
            }
        }
    }
    let sh_own_view = canvas.shadow_own.as_ref().filter(|_| sh_u[0] > 0.5).map(|s| s.1.clone()).unwrap_or_else(|| progs.blank_depth.clone());
    let sh_others_view = canvas.shadow_others.as_ref().filter(|_| sh_u[3] > 0.5).map(|s| s.1.clone()).unwrap_or_else(|| progs.blank_depth.clone());
    let clock = CLOCK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut meshes = MESHES.lock().unwrap_or_else(|e| e.into_inner());
    let mut enc = d.create_command_encoder(&Default::default());
    // Ce qu'on va dessiner : (rang du maillage, tampon de positions, branchement des réglages)
    let mut draws: Vec<(usize, wgpu::Buffer, wgpu::BindGroup)> = vec![];
    let mut outlined = false;
    for p in parts {
        let key = (p.tris.0, match &p.source { Source::Live(l) => l.cloth, Source::Data(..) => 0 });
        let at = match meshes.iter().position(|m| m.key == key && m.n == p.n) {
            Some(k) => k,
            None => {
                let tris = (p.tris.1)();
                let inv = match &p.source { Source::Live(l) => Some(&l.inv[..]), Source::Data(..) => None };
                let Some(m) = Mesh::new(g, key, p.n, &tris, inv) else { continue };
                // Quelques maillages restent prêts ; le plus ancien cède sa place et sa mémoire
                if meshes.len() >= 6 {
                    if let Some(old) = (0..meshes.len()).filter(|k| !draws.iter().any(|x| x.0 == *k)).min_by_key(|k| meshes[*k].used) {
                        meshes.swap_remove(old);
                        // Le dernier maillage a pris la place libérée
                        let moved = meshes.len();
                        for x in draws.iter_mut().filter(|x| x.0 == moved) {
                            x.0 = old;
                        }
                    }
                }
                meshes.push(m);
                meshes.len() - 1
            }
        };
        let m = &mut meshes[at];
        m.used = clock;
        // Positions : le tampon du solveur tel quel, ou celles du processeur quand elles ont changé
        let (pos, origin) = match &p.source {
            Source::Live(l) => (l.pos.clone(), l.origin),
            Source::Data(version, get) => {
                if m.own_pos.as_ref().is_none_or(|o| o.1 != *version) {
                    let data = get();
                    if data.len() != p.n {
                        continue;
                    }
                    let buf = match m.own_pos.take() {
                        Some((b, _)) => b,
                        None => d.create_buffer(&wgpu::BufferDescriptor { label: Some("dessin-positions"), size: (p.n * 16) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }),
                    };
                    g.queue.write_buffer(&buf, 0, gpusim::bytes_of(&data));
                    m.own_pos = Some((buf, *version));
                }
                let Some((b, _)) = &m.own_pos else { continue };
                (b.clone(), [0.0; 3])
            }
        };
        if pos.size() < (p.n * 16) as u64 {
            continue;
        }
        // Couleurs par sommet, dans l'ordre du tampon de positions
        if let Some((version, get)) = &p.colors {
            if m.colors_for != Some(*version) {
                let c = get();
                if c.len() == p.n {
                    let sorted: Vec<[u8; 4]> = match &p.source {
                        Source::Live(l) => l.perm.iter().map(|i| c.get(*i as usize).copied().unwrap_or([255; 4])).collect(),
                        Source::Data(..) => c,
                    };
                    g.queue.write_buffer(&m.colors, 0, gpusim::bytes_of(&sorted));
                }
                m.colors_for = Some(*version);
            }
        }
        // UV, dans le même ordre
        let mut has_uv = false;
        if let Some((version, get)) = &p.uv {
            if m.uv_for != Some(*version) {
                let c = get();
                if c.len() == p.n {
                    let sorted: Vec<[f32; 2]> = match &p.source {
                        Source::Live(l) => l.perm.iter().map(|i| c.get(*i as usize).copied().unwrap_or([0.0; 2])).collect(),
                        Source::Data(..) => c,
                    };
                    g.queue.write_buffer(&m.uv, 0, gpusim::bytes_of(&sorted));
                    m.uv_for = Some(*version);
                }
            }
            has_uv = m.uv_for == Some(*version);
        }
        // Textures du matériau : seulement avec des UV
        let maps = p.surface.maps.filter(|_| has_uv).and_then(|b| textures(g, b, clock));
        // Normales : un lancement du noyau, sur le tampon de positions
        if m.group.as_ref().is_none_or(|x| x.1 != pos) {
            m.group = Some((g.group(&progs.normals, &[&m.count, &pos, &m.off, &m.adj, &m.normals]), pos.clone()));
        }
        if let Some((group, _)) = &m.group {
            let mut pass = enc.begin_compute_pass(&Default::default());
            g.launch(&mut pass, &progs.normals.pipelines[0], group, &[], p.n as u64);
        }
        let mut u = [0.0f32; (U_SIZE / 4) as usize];
        let mut put = |at: usize, v: &[f32]| u[at..at + v.len()].copy_from_slice(v);
        put(0, cam.view_proj.as_flattened());
        put(16, p.model.as_flattened());
        put(32, light_vp.as_flattened());
        put(48, &[origin[0], origin[1], origin[2], 0.0]);
        put(52, &cam.depth_row);
        put(56, &[cam.light[0], cam.light[1], cam.light[2], 0.0]);
        put(60, &[cam.eye[0], cam.eye[1], cam.eye[2], 0.0]);
        put(64, &[p.base[0], p.base[1], p.base[2], if p.colors.is_some() { 1.0 } else { 0.0 }]);
        put(68, &soft_u);
        put(72, &[size[0] as f32, size[1] as f32, 0.0, 0.0]);
        put(76, &sh_u);
        put(80, &[p.surface.rough, p.surface.metal, if maps.is_some() { 1.0 } else { 0.0 }, 0.0]);
        put(84, &[id_code(p.id, p.select) as f32, 0.0, 0.0, 0.0]);
        if let Some(l) = &cam.scene {
            put(88, &[l.sky_up[0], l.sky_up[1], l.sky_up[2], 1.0]);
            put(92, &[l.sky_down[0], l.sky_down[1], l.sky_down[2], l.exposure]);
            put(96, &[l.sun[0], l.sun[1], l.sun[2], 0.0]);
        }
        g.queue.write_buffer(&m.uniform, 0, gpusim::bytes_of(&u));
        outlined |= p.select != 0;
        let (t_color, t_orm) = maps.unwrap_or_else(|| (progs.blank_color.clone(), progs.blank_color.clone()));
        let group = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &progs.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: m.uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&soft_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&sh_own_view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&sh_others_view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&progs.compare) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&t_color) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&t_orm) },
                wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::Sampler(&progs.sampler) },
            ],
        });
        draws.push((at, pos, group));
    }
    // Les cartes d'ombre : la profondeur vue de la lumière
    let light_pass = |enc: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, f: &dyn Fn(&mut wgpu::RenderPass)| {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("dessin-ombre"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: target, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&progs.light);
        f(&mut pass);
    };
    if sh_u[0] > 0.5 {
        if let Some(own) = &canvas.shadow_own {
            light_pass(&mut enc, &own.1, &|pass| {
                for (at, pos, _) in &draws {
                    let m = &meshes[*at];
                    pass.set_bind_group(0, &m.light, &[]);
                    pass.set_vertex_buffer(0, pos.slice(..));
                    pass.set_index_buffer(m.index.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..m.index_count, 0, 0..1);
                }
            });
        }
        if let (true, Some(others), Some(occ)) = (others_due, &canvas.shadow_others, &canvas.occ) {
            light_pass(&mut enc, &others.1, &|pass| {
                pass.set_bind_group(0, &occ.group, &[]);
                pass.set_vertex_buffer(0, occ.pos.slice(..));
                pass.set_index_buffer(occ.index.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..occ.index_count, 0, 0..1);
            });
        }
    }
    // L'image des identifiants : sans lissage, chaque pixel dit exactement ce qu'il montre
    canvas.ids_drawn = false;
    if let (true, Some(ids)) = (want_ids, &canvas.ids) {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("dessin-identifiants"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &ids.1, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: NOTHING.to_bits() as f64, g: 0.0, b: 0.0, a: 0.0 }), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &ids.2, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }), stencil_ops: None }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&progs.ids);
        for (at, pos, group) in &draws {
            let m = &meshes[*at];
            pass.set_bind_group(0, group, &[]);
            pass.set_vertex_buffer(0, pos.slice(..));
            pass.set_index_buffer(m.index.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..m.index_count, 0, 0..1);
        }
        canvas.ids_drawn = true;
    }
    {
        let (target, resolve) = match &canvas.msaa {
            Some(m) => (m, Some(&view)),
            None => (&view, None),
        };
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("dessin-maillage"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target, resolve_target: resolve, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }), stencil_ops: None }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&progs.pipelines[if samples > 1 { 1 } else { 0 }]);
        for (at, pos, group) in &draws {
            let m = &meshes[*at];
            pass.set_bind_group(0, group, &[]);
            pass.set_vertex_buffer(0, pos.slice(..));
            pass.set_vertex_buffer(1, m.normals.slice(..));
            pass.set_vertex_buffer(2, m.colors.slice(..));
            pass.set_vertex_buffer(3, m.uv.slice(..));
            pass.set_index_buffer(m.index.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..m.index_count, 0, 0..1);
        }
    }
    // Après le maillage : son ombre sur l'image calculée ailleurs, puis le contour de sélection
    let under = cam.shadow.as_ref().filter(|s| sh_u[0] > 0.5 && !draws.is_empty() && ((soft_u[2] > 0.5 && s.on_surfaces > 0.0) || (s.floor.is_some() && s.on_floor > 0.0)));
    let over = outlined && cam.outline > 0.0 && !draws.is_empty();
    if let (true, Some(ids), Some(inv)) = (under.is_some() || over, canvas.ids.as_ref().filter(|_| canvas.ids_drawn), crate::volume::mat_inv(&cam.view_proj)) {
        let uniform = canvas.after.get_or_insert_with(|| g.uniform("dessin-apres", O_SIZE)).clone();
        let mut u = [0.0f32; (O_SIZE / 4) as usize];
        let mut put = |at: usize, v: &[f32]| u[at..at + v.len()].copy_from_slice(v);
        put(0, inv.as_flattened());
        put(16, light_vp.as_flattened());
        put(32, &cam.depth_row);
        put(36, &[cam.light[0], cam.light[1], cam.light[2], 0.0]);
        put(40, &soft_u);
        put(44, &[size[0] as f32, size[1] as f32, cam.outline.clamp(1.0, 12.0), 0.0]);
        if let Some(s) = under {
            put(48, &[s.floor.unwrap_or(0.0), if s.floor.is_some() { 1.0 } else { 0.0 }, s.on_floor.clamp(0.0, 1.0), s.on_surfaces.clamp(0.0, 1.0)]);
        }
        put(52, &sh_u);
        g.queue.write_buffer(&uniform, 0, gpusim::bytes_of(&u));
        let group = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &progs.layout_over,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&soft_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&ids.1) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&sh_own_view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&sh_others_view) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::Sampler(&progs.compare) },
            ],
        });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("dessin-apres"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &group, &[]);
        if under.is_some() {
            pass.set_pipeline(&progs.under);
            pass.draw(0..3, 0..1);
        }
        if over {
            pass.set_pipeline(&progs.over);
            pass.draw(0..3, 0..1);
        }
    }
    g.queue.submit([enc.finish()]);
    !draws.is_empty() && !g.broken()
}

/// Oublie les maillages et les textures gardés sur la carte (leur mémoire est rendue).
pub fn clear() {
    MESHES.lock().unwrap_or_else(|e| e.into_inner()).clear();
    TEXTURES.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un tissu simulé sur la carte, dessiné depuis les tampons du solveur : l'image montre un
    /// tissu éclairé, et aucune donnée n'est revenue au processeur.
    #[test]
    fn tissu_dessine_sans_relecture() {
        if !available() {
            return;
        }
        let n = 24;
        let (pos, edges) = crate::gpucloth::grid(n, n, 1.0, 1.0);
        let rest = crate::gpucloth::rest_lengths(&edges, &pos);
        let pinned: Vec<bool> = (0..n * n).map(|i| i < n).collect();
        let topo = crate::gpucloth::Topo::build(n * n, &edges, &rest, &pos, &pinned, None);
        let mut cloth = crate::gpucloth::Cloth::new(topo, &pos, true);
        if !cloth.on_gpu() {
            return;
        }
        let par = crate::gpucloth::Params::default();
        for _ in 0..3 {
            cloth.push_uniform([0.0, -9.81 / 24.0, 0.0], 1.0);
            cloth.step(1.0 / 24.0, &par);
        }
        let shot = cloth.shot();
        let live = shot.live.as_ref().expect("tissu sur la carte");
        assert!(live.current());
        // Triangles de la grille, dans l'ordre de la géométrie
        let tris = || {
            let mut t = vec![];
            for z in 0..n - 1 {
                for x in 0..n - 1 {
                    let i = (z * n + x) as u32;
                    t.push([i, i + 1, i + n as u32 + 1]);
                    t.push([i, i + n as u32 + 1, i + n as u32]);
                }
            }
            t
        };
        // Vue de dessus : l'œil à 3 m au-dessus du centre, regard vers le bas
        let (near, far) = (0.1f32, 20.0f32);
        let f = far / (far - near);
        let depth = [0.0, -1.0, 0.0, 3.0];
        let vp = from_rows([[1.2, 0.0, 0.0, 0.0], [0.0, 0.0, -1.2, 0.0], [0.0, -f, 0.0, (3.0 - near) * f], depth]);
        let cam = Cam { view_proj: vp, depth_row: depth, light: [0.2, 1.0, 0.3], eye: [0.0, 3.0, 0.0], soft: None, shadow: None, outline: 0.0, front: None, scene: None };
        let part = Part { n: n * n, source: Source::Live(live), tris: (0xBEEF, &tris), colors: None, base: [0.2, 0.6, 0.9], model: IDENTITY, uv: None, surface: Default::default(), id: 0, select: 0 };
        let mut canvas = Canvas::default();
        assert!(paint(&mut canvas, [96, 96], &cam, &[part]));
        let px = canvas.pixels().expect("image relue");
        let covered = px.chunks(4).filter(|p| p[3] == 255).count();
        let lit = px.chunks(4).filter(|p| p[3] == 255 && p[2] > 60 && p[2] > p[0]).count();
        // Le tissu (1 m de côté vu à 3 m, focale 1,2) couvre quelques centaines de pixels au milieu
        assert!(covered > 300 && covered < 1500, "{covered} pixels couverts");
        assert_eq!(px[(48 * 96 + 48) * 4 + 3], 255, "le milieu de l'image est sur le tissu");
        assert_eq!(px[3], 0, "le coin de l'image est vide");
        assert!(lit * 10 > covered * 9, "{lit} pixels éclairés sur {covered}");
        // Rien n'a été relu pour dessiner
        assert!(!shot.resolved());
        // Le tissu avance : ce cliché ne décrit plus les tampons de la carte
        cloth.step(1.0 / 24.0, &par);
        assert!(!live.current());
    }

    /// Un carré face à la caméra : il couvre le milieu de l'image, pas les coins ; et une
    /// profondeur extérieure plus proche que lui le cache.
    #[test]
    fn un_carre_dessine_et_cache() {
        if !available() {
            return;
        }
        let quad = || vec![[-0.5f32, -0.5, 0.0, 0.0], [0.5, -0.5, 0.0, 0.0], [0.5, 0.5, 0.0, 0.0], [-0.5, 0.5, 0.0, 0.0]];
        let tris = || vec![[0u32, 1, 2], [0, 2, 3]];
        // Caméra en z = 2, qui regarde vers −z : profondeur = 2 − z
        let (near, far) = (0.1f32, 10.0f32);
        let vp = from_rows([[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, -far / (far - near), far * (2.0 - near) / (far - near)], [0.0, 0.0, -1.0, 2.0]]);
        let part = |version: u64| Part { n: 4, source: Source::Data(version, &quad), tris: (0xC0FFEE, &tris), colors: None, base: [0.8, 0.2, 0.2], model: IDENTITY, uv: None, surface: Default::default(), id: 0, select: 0 };
        let cam = |soft| Cam { view_proj: vp, depth_row: [0.0, 0.0, -1.0, 2.0], light: [0.3, 0.5, 1.0], eye: [0.0, 0.0, 2.0], soft, shadow: None, outline: 0.0, front: None, scene: None };
        let mut canvas = Canvas::default();
        assert!(paint(&mut canvas, [64, 64], &cam(None), &[part(1)]));
        let px = canvas.pixels().expect("image relue");
        let at = |x: usize, y: usize| [px[(y * 64 + x) * 4], px[(y * 64 + x) * 4 + 1], px[(y * 64 + x) * 4 + 2], px[(y * 64 + x) * 4 + 3]];
        // Le carré (demi-côté 0,5 à la distance 2) couvre le quart central : rouge au milieu, vide au coin
        assert!(at(32, 32)[3] == 255 && at(32, 32)[0] > at(32, 32)[1] + 40, "milieu {:?}", at(32, 32));
        assert_eq!(at(2, 2)[3], 0, "coin {:?}", at(2, 2));
        // Une image extérieure à la profondeur 1 partout (le carré est à 2) : il est caché
        let near_all = || vec![1.0f32; 16 * 16];
        let hidden = Soft { w: 16, h: 16, version: 1, depth: &near_all, bias: 0.01 };
        assert!(paint(&mut canvas, [64, 64], &cam(Some(hidden)), &[part(1)]));
        assert_eq!(canvas.pixels().unwrap()[(32 * 64 + 32) * 4 + 3], 0);
        // À la profondeur 3 partout : il passe devant
        let far_all = || vec![3.0f32; 16 * 16];
        let shown = Soft { w: 16, h: 16, version: 2, depth: &far_all, bias: 0.01 };
        assert!(paint(&mut canvas, [64, 64], &cam(Some(shown)), &[part(1)]));
        assert_eq!(canvas.pixels().unwrap()[(32 * 64 + 32) * 4 + 3], 255);
    }
    /// Applique une matrice (colonnes d'abord) à un point.
    fn apply(m: &[[f32; 4]; 4], p: [f32; 3]) -> [f32; 4] {
        let v = [p[0], p[1], p[2], 1.0];
        let mut o = [0.0f32; 4];
        for r in 0..4 {
            for c in 0..4 {
                o[r] += m[c][r] * v[c];
            }
        }
        o
    }

    /// La carte d'ombre couvre la boule demandée : son centre au milieu, le côté de la lumière à
    /// la profondeur 0, l'autre à 1.
    #[test]
    fn matrice_de_la_lumiere() {
        for light in [[0.0, 1.0, 0.0], [0.3, 0.8, -0.5], [0.0, -1.0, 0.0], [0.0, 0.0, 0.0]] {
            let (c, r) = ([1.0, 2.0, 3.0], 2.0);
            let m = light_matrix(light, c, r);
            let o = apply(&m, c);
            assert!(o[0].abs() < 1e-5 && o[1].abs() < 1e-5 && (o[2] - 0.5).abs() < 1e-5, "{o:?}");
            let len = (light[0] * light[0] + light[1] * light[1] + light[2] * light[2]).sqrt();
            let l = if len > 1e-6 { [light[0] / len, light[1] / len, light[2] / len] } else { [0.0, 1.0, 0.0] };
            let near = apply(&m, [c[0] + l[0] * r, c[1] + l[1] * r, c[2] + l[2] * r]);
            let far = apply(&m, [c[0] - l[0] * r, c[1] - l[1] * r, c[2] - l[2] * r]);
            assert!(near[2].abs() < 1e-5 && (far[2] - 1.0).abs() < 1e-5, "{near:?} {far:?}");
            // Un point au bord de la boule, de côté : au bord de la carte
            let side = [l[1] - l[2], l[2] - l[0], l[0] - l[1]];
            let sl = (side[0] * side[0] + side[1] * side[1] + side[2] * side[2]).sqrt();
            let e = apply(&m, [c[0] + side[0] / sl * r, c[1] + side[1] / sl * r, c[2] + side[2] / sl * r]);
            assert!(((e[0] * e[0] + e[1] * e[1]).sqrt() - 1.0).abs() < 1e-4, "{e:?}");
        }
    }

    #[test]
    fn codes_et_textures() {
        for (id, sel) in [(0, 0), (1, 2), (7, 1), (0x3F_FFFF, 3)] {
            assert_eq!(id_decode(id_code(id, sel)), (id, sel));
        }
        // Un code tient dans un nombre à virgule sans arrondi (il passe par les réglages)
        assert_eq!(id_code(0x3F_FFFF, 3) as f32 as u32, id_code(0x3F_FFFF, 3));
        let px = vec![[0, 0, 0, 255], [255, 255, 255, 255], [100, 0, 0, 255], [0, 100, 0, 255]];
        assert_eq!(half_size(&px, 2), Some(vec![[89, 89, 64, 255]]));
        assert_eq!(half_size(&px[..1], 1), None);
        assert_eq!(half_size(&px, 3), None);
        // Moyennes : la couleur en linéaire, rugosité et métal tels quels
        let b = Baked::new(9, 2, &[[1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]], &[0.2, 0.4, 0.6, 0.8], &[0.0, 0.0, 1.0, 1.0]).unwrap();
        assert_eq!((b.n, b.color[0], b.orm[3]), (2, [255, 0, 0, 255], [204, 255, 0, 255]));
        assert!((b.mean.0[0] - 0.5).abs() < 1e-6 && (b.mean.1 - 0.5).abs() < 1e-6 && (b.mean.2 - 0.5).abs() < 1e-6);
        assert!(Baked::new(1, 2, &[[0.0; 3]; 3], &[0.0; 4], &[0.0; 4]).is_none());
    }

    /// Caméra au-dessus de l'origine, à 3 m, qui regarde vers le bas : x vers la droite de l'image,
    /// z vers le haut de l'image. Profondeur : 3 − y.
    fn from_above() -> ([[f32; 4]; 4], [f32; 4]) {
        let (near, far) = (0.1f32, 20.0f32);
        let f = far / (far - near);
        let depth = [0.0, -1.0, 0.0, 3.0];
        (from_rows([[1.2, 0.0, 0.0, 0.0], [0.0, 0.0, -1.2, 0.0], [0.0, -f, 0.0, (3.0 - near) * f], depth]), depth)
    }

    /// Un carré horizontal de demi-côté `r` à la hauteur `y`, avec ses UV
    fn flat(r: f32, y: f32) -> (Vec<[f32; 4]>, Vec<[f32; 2]>) {
        (vec![[-r, y, -r, 0.0], [r, y, -r, 0.0], [r, y, r, 0.0], [-r, y, r, 0.0]], vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
    }

    fn pixel(px: &[u8], w: usize, x: usize, y: usize) -> [u8; 4] {
        let o = (y * w + x) * 4;
        [px[o], px[o + 1], px[o + 2], px[o + 3]]
    }

    fn light(p: [u8; 4]) -> u32 {
        p[0] as u32 + p[1] as u32 + p[2] as u32
    }

    /// Au clic, l'image des identifiants dit quel morceau est sous le pointeur ; le morceau
    /// choisi a son contour orange, juste autour de lui.
    #[test]
    fn choisi_au_clic_et_entoure() {
        if !available() {
            return;
        }
        let (vp, depth) = from_above();
        let quad = || flat(0.5, 0.0).0;
        let tris = || vec![[0u32, 1, 2], [0, 2, 3]];
        let cam = Cam { view_proj: vp, depth_row: depth, light: [0.2, 1.0, 0.3], eye: [0.0, 3.0, 0.0], soft: None, shadow: None, outline: 2.0, front: None, scene: None };
        let part = |select| Part { n: 4, source: Source::Data(1, &quad), tris: (0xC11C, &tris), colors: None, base: [0.5, 0.5, 0.5], model: IDENTITY, uv: None, surface: Default::default(), id: 7, select };
        let mut canvas = Canvas::default();
        assert!(paint(&mut canvas, [96, 96], &cam, &[part(2)]));
        // Le carré (1 m vu à 3 m, focale 1,2) va des pixels 38,4 à 57,6
        let (id, d) = canvas.pick(48, 48).expect("le carré est sous le milieu");
        assert_eq!(id, 7);
        assert!((d - 3.0).abs() < 1e-3, "profondeur {d}");
        assert_eq!(canvas.pick(5, 5), None);
        assert_eq!(canvas.pick(500, 5), None);
        assert!(canvas.front().is_some());
        let px = canvas.pixels().unwrap();
        let edge = pixel(&px, 96, 37, 48);
        assert!(edge[3] == 255 && edge[0] > 200 && edge[1] > 120 && edge[1] < 200 && edge[2] < 100, "contour {edge:?}");
        let inside = pixel(&px, 96, 48, 48);
        assert!(inside[0].abs_diff(inside[2]) < 40, "le milieu reste gris {inside:?}");
        assert_eq!(pixel(&px, 96, 30, 48)[3], 0, "loin du bord, rien");
        // Pas choisi : pas de contour, mais toujours pointable
        assert!(paint(&mut canvas, [96, 96], &cam, &[part(0)]));
        assert_eq!(pixel(&canvas.pixels().unwrap(), 96, 37, 48)[3], 0);
        assert_eq!(canvas.pick(48, 48).map(|p| p.0), Some(7));
    }

    /// Lumière venue d'en haut à droite : un petit carré à 0,5 m au-dessus d'un grand pose son
    /// ombre à 0,5 m sur la gauche. Qu'il soit dessiné ici ou seulement donné comme ce qui cache
    /// la lumière (`Occluders`), le grand carré est plus sombre là.
    #[test]
    fn ombre_recue_sur_la_carte() {
        if !available() {
            return;
        }
        let (vp, depth) = from_above();
        let floor = || flat(1.0, 0.0).0;
        let small = || flat(0.2, 0.5).0;
        let tris = || vec![[0u32, 1, 2], [0, 2, 3]];
        let occ_mesh = || (flat(0.2, 0.5).0, vec![[0u32, 1, 2], [0, 2, 3]]);
        let nothing = || (vec![], vec![]);
        for (label, through_occluders) in [("morceau", false), ("objets", true)] {
            let shadow = Shadow {
                center: [0.0, 0.25, 0.0],
                radius: 1.6,
                occluders: Some(Occluders { version: 1 + through_occluders as u64, mesh: if through_occluders { &occ_mesh } else { &nothing }, model: IDENTITY }),
                floor: None,
                on_surfaces: 0.0,
                on_floor: 0.0,
            };
            let cam = Cam { view_proj: vp, depth_row: depth, light: [1.0, 1.0, 0.0], eye: [0.0, 3.0, 0.0], soft: None, shadow: Some(shadow), outline: 0.0, front: None, scene: None };
            let mut parts = vec![Part { n: 4, source: Source::Data(1, &floor), tris: (0x5A01, &tris), colors: None, base: [0.7, 0.7, 0.7], model: IDENTITY, uv: None, surface: Default::default(), id: 1, select: 0 }];
            if !through_occluders {
                parts.push(Part { n: 4, source: Source::Data(1, &small), tris: (0x5A02, &tris), colors: None, base: [0.7, 0.7, 0.7], model: IDENTITY, uv: None, surface: Default::default(), id: 2, select: 0 });
            }
            let mut canvas = Canvas::default();
            assert!(paint(&mut canvas, [96, 96], &cam, &parts));
            let px = canvas.pixels().unwrap();
            // x = −0,5 m au sol : pixel 38 ; x = +0,5 m : pixel 58
            let (dark, lit) = (pixel(&px, 96, 38, 48), pixel(&px, 96, 58, 48));
            assert!(dark[3] == 255 && lit[3] == 255);
            assert!(light(dark) + 90 < light(lit), "{label} : à l'ombre {dark:?}, au soleil {lit:?}");
            // Sans ombres : les deux côtés se valent
            let cam = Cam { shadow: None, ..cam };
            assert!(paint(&mut canvas, [96, 96], &cam, &parts));
            let px = canvas.pixels().unwrap();
            assert!(light(pixel(&px, 96, 38, 48)).abs_diff(light(pixel(&px, 96, 58, 48))) < 30, "{label}");
        }
    }

    /// Le maillage pose son ombre sur l'image calculée ailleurs : sur le sol (rien d'autre
    /// dessiné), puis sur une surface retrouvée par sa profondeur.
    #[test]
    fn ombre_portee_sur_l_image_d_ailleurs() {
        if !available() {
            return;
        }
        let (vp, depth) = from_above();
        let small = || flat(0.2, 0.5).0;
        let tris = || vec![[0u32, 1, 2], [0, 2, 3]];
        let shadow = |floor: Option<f32>, on_surfaces: f32| Shadow { center: [0.0, 0.25, 0.0], radius: 1.6, occluders: None, floor, on_surfaces, on_floor: 0.5 };
        let part = Part { n: 4, source: Source::Data(1, &small), tris: (0x5A03, &tris), colors: None, base: [0.7, 0.7, 0.7], model: IDENTITY, uv: None, surface: Default::default(), id: 1, select: 0 };
        let mut canvas = Canvas::default();
        let cam = Cam { view_proj: vp, depth_row: depth, light: [1.0, 1.0, 0.0], eye: [0.0, 3.0, 0.0], soft: None, shadow: Some(shadow(Some(0.0), 0.0)), outline: 0.0, front: None, scene: None };
        assert!(paint(&mut canvas, [96, 96], &cam, &[part]));
        let px = canvas.pixels().unwrap();
        let (dark, lit) = (pixel(&px, 96, 38, 48), pixel(&px, 96, 58, 48));
        assert!(dark[3] > 100 && dark[3] < 150 && dark[0] == 0, "ombre au sol {dark:?}");
        assert_eq!(lit[3], 0, "hors de l'ombre {lit:?}");
        // Une surface à la profondeur 3 partout (un sol en y = 0, vu par l'autre image)
        let plane = || vec![3.0f32; 48 * 48];
        let part = Part { n: 4, source: Source::Data(1, &small), tris: (0x5A03, &tris), colors: None, base: [0.7, 0.7, 0.7], model: IDENTITY, uv: None, surface: Default::default(), id: 1, select: 0 };
        let cam = Cam { soft: Some(Soft { w: 48, h: 48, version: 1, depth: &plane, bias: 0.01 }), shadow: Some(shadow(None, 0.4)), ..cam };
        assert!(paint(&mut canvas, [96, 96], &cam, &[part]));
        let px = canvas.pixels().unwrap();
        let (dark, lit) = (pixel(&px, 96, 38, 48), pixel(&px, 96, 58, 48));
        assert!(dark[3] > 80 && dark[3] < 120, "ombre sur la surface {dark:?}");
        assert_eq!(lit[3], 0, "hors de l'ombre {lit:?}");
        // Le carré lui-même n'est pas assombri par sa propre ombre posée
        assert_eq!(pixel(&px, 96, 48, 48)[3], 255);
    }

    /// Les textures cuites d'un matériau se lisent par les UV : rouge à gauche, vert à droite.
    /// Un métal poli ne ressemble pas à une surface mate de la même couleur.
    #[test]
    fn materiau_en_textures() {
        if !available() {
            return;
        }
        let (vp, depth) = from_above();
        let quad = || flat(0.5, 0.0).0;
        let uvs = || flat(0.5, 0.0).1;
        let tris = || vec![[0u32, 1, 2], [0, 2, 3]];
        let n = 8;
        let color: Vec<[f32; 3]> = (0..n * n).map(|i| if i % n < n / 2 { [0.9, 0.1, 0.1] } else { [0.1, 0.8, 0.1] }).collect();
        let baked = Baked::new(0xBA4ED, n, &color, &vec![0.7; n * n], &vec![0.0; n * n]).unwrap();
        let cam = Cam { view_proj: vp, depth_row: depth, light: [0.2, 1.0, 0.3], eye: [0.0, 3.0, 0.0], soft: None, shadow: None, outline: 0.0, front: None, scene: None };
        let part = |uv: bool, surface: Surface<'_>| -> Vec<u8> {
            let p = Part { n: 4, source: Source::Data(1, &quad), tris: (0x3A7, &tris), colors: None, base: [0.5, 0.5, 0.5], model: IDENTITY, uv: uv.then_some((1, &uvs as &dyn Fn() -> Vec<[f32; 2]>)), surface, id: 0, select: 0 };
            let mut canvas = Canvas::default();
            assert!(paint(&mut canvas, [96, 96], &cam, &[p]));
            canvas.pixels().unwrap()
        };
        let px = part(true, Surface { rough: 0.7, metal: 0.0, maps: Some(&baked) });
        let (l, r) = (pixel(&px, 96, 42, 48), pixel(&px, 96, 54, 48));
        assert!(l[0] > l[1] + 60 && r[1] > r[0] + 60, "gauche {l:?}, droite {r:?}");
        // Sans UV, les textures ne peuvent pas se lire : la couleur de base
        let px = part(false, Surface { rough: 0.7, metal: 0.0, maps: Some(&baked) });
        let c = pixel(&px, 96, 42, 48);
        assert!(c[0].abs_diff(c[1]) < 20, "{c:?}");
        // Métal poli contre mat, même couleur de base
        let matte = pixel(&part(false, Surface { rough: 0.9, metal: 0.0, maps: None }), 96, 48, 48);
        let metal = pixel(&part(false, Surface { rough: 0.15, metal: 1.0, maps: None }), 96, 48, 48);
        assert!(light(matte).abs_diff(light(metal)) > 30, "mat {matte:?}, métal {metal:?}");
    }
}
