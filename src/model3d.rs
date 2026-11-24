//! Visionneuse 3D : OBJ, FBX (binaire), glTF / GLB et STL. Rendu logiciel avec egui.
//!
//! Les modèles très lourds sont simplifiés par regroupement de sommets (grille 3D) :
//! la forme reste complète, avec moins de triangles, pour rester fluide.
//! `thumbnail` rend une image du modèle (z-buffer) pour les boards et les refs.
use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Key, Pos2, Sense, Stroke};
use std::collections::HashMap;
use std::io::BufRead;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver};

pub const EXT: [&str; 12] = ["obj", "fbx", "glb", "gltf", "stl", "3ds", "ply", "ma", "mb", "max", "aryscene", "arya3"];
/// Nombre de triangles visé pour l'affichage interactif.
const MAX_TRIS: usize = 150_000;

pub fn is_model(rel: &str) -> bool {
    let e = rel.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    EXT.contains(&e.as_str())
}

type V3 = [f32; 3];

#[path = "formats3d.rs"]
pub mod formats3d;
#[path = "pathtrace.rs"]
pub mod pathtrace;
#[path = "gputrace.rs"]
pub mod gputrace;

pub struct Model {
    pub positions: Vec<V3>,
    pub tris: Vec<[u32; 3]>,
    pub colors: Vec<Color32>,
    /// Arêtes réelles (pas les diagonales des quads) : bit 0 v0-v1, bit 1 v1-v2, bit 2 v2-v0.
    pub edges: Vec<u8>,
    pub min: V3,
    pub max: V3,
    /// Avant simplification
    pub total_tris: usize,
    pub total_vertices: usize,
    /// Y en haut par défaut ; STL et la plupart des exports « Z up ».
    pub z_up: bool,
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt().max(1e-12);
    [a[0] / l, a[1] / l, a[2] / l]
}

impl Model {
    fn new(positions: Vec<V3>, tris: Vec<[u32; 3]>, colors: Option<Vec<Color32>>, z_up: bool) -> Result<Self, String> {
        Self::with_edges(positions, tris, colors, None, z_up)
    }

    pub(crate) fn with_edges(positions: Vec<V3>, tris: Vec<[u32; 3]>, colors: Option<Vec<Color32>>, edges: Option<Vec<u8>>, z_up: bool) -> Result<Self, String> {
        Self::assemble(positions, tris, colors, edges, z_up, true)
    }

    /// Sans simplification : une scène doit garder ses triangles dans l'ordre pour savoir à quel objet chacun appartient
    /// (chaque modèle qu'elle réunit a déjà été allégé à son chargement).
    pub(crate) fn exact(positions: Vec<V3>, tris: Vec<[u32; 3]>, colors: Option<Vec<Color32>>, edges: Option<Vec<u8>>) -> Result<Self, String> {
        Self::assemble(positions, tris, colors, edges, false, false)
    }

    fn assemble(positions: Vec<V3>, tris: Vec<[u32; 3]>, colors: Option<Vec<Color32>>, edges: Option<Vec<u8>>, z_up: bool, simplify: bool) -> Result<Self, String> {
        let n0 = tris.len();
        let edges = edges.filter(|e| e.len() == n0).unwrap_or_else(|| vec![7; n0]);
        let (tris, edges): (Vec<[u32; 3]>, Vec<u8>) = tris.into_iter().zip(edges).filter(|(t, _)| t.iter().all(|&i| (i as usize) < positions.len())).unzip();
        if tris.is_empty() {
            return Err("Aucun triangle dans ce fichier.".into());
        }
        let colors = colors.filter(|c| c.len() == tris.len()).unwrap_or_else(|| vec![Color32::from_gray(205); tris.len()]);
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for v in &positions {
            for k in 0..3 {
                min[k] = min[k].min(v[k]);
                max[k] = max[k].max(v[k]);
            }
        }
        let (total_tris, total_vertices) = (tris.len(), positions.len());
        let mut m = Self { positions, tris, colors, edges, min, max, total_tris, total_vertices, z_up };
        if simplify {
            m.simplify(MAX_TRIS);
        }
        Ok(m)
    }

    pub fn size(&self) -> V3 {
        sub(self.max, self.min)
    }

    /// Simplification par grille (voir `simplify_arrays`).
    fn simplify(&mut self, target: usize) {
        if self.tris.len() <= target {
            return;
        }
        let (p, t, c) = simplify_arrays(&self.positions, &self.tris, &self.colors, target);
        self.edges = vec![7; t.len()];
        self.positions = p;
        self.tris = t;
        self.colors = c;
    }

    /// Sommets centrés, mis à l'échelle (taille 1) et remis en Y vers le haut.
    pub fn normalized(&self, z_up: bool) -> Vec<V3> {
        let s = self.size();
        let scale = 1.0 / s[0].max(s[1]).max(s[2]).max(1e-9);
        let c = [(self.min[0] + self.max[0]) / 2.0, (self.min[1] + self.max[1]) / 2.0, (self.min[2] + self.max[2]) / 2.0];
        self.positions
            .iter()
            .map(|v| {
                let v = [(v[0] - c[0]) * scale, (v[1] - c[1]) * scale, (v[2] - c[2]) * scale];
                if z_up { [v[0], v[2], -v[1]] } else { v }
            })
            .collect()
    }
}

/// Simplification par grille : tous les sommets d'une même case fusionnent.
/// On resserre la grille jusqu'à passer sous `target` triangles.
pub fn simplify_arrays(positions: &[V3], tris_in: &[[u32; 3]], colors_in: &[Color32], target: usize) -> (Vec<V3>, Vec<[u32; 3]>, Vec<Color32>) {
    if tris_in.len() <= target {
        return (positions.to_vec(), tris_in.to_vec(), colors_in.to_vec());
    }
    // D'abord des contractions d'arêtes propres ; la grille ne finit que ce qu'elles n'ont pas pu
    let (p, t, ids) = crate::retopo::simplify_to(positions, tris_in, target);
    let c: Vec<Color32> = ids.iter().map(|&i| colors_in.get(i as usize).copied().unwrap_or(Color32::WHITE)).collect();
    if t.len() <= target + target / 10 {
        return (p, t, c);
    }
    grid_simplify(&p, &t, &c, target)
}

/// Simplification par grille, en dernier recours (rapide mais laisse des échardes).
fn grid_simplify(positions: &[V3], tris_in: &[[u32; 3]], colors_in: &[Color32], target: usize) -> (Vec<V3>, Vec<[u32; 3]>, Vec<Color32>) {
    if tris_in.len() <= target {
        return (positions.to_vec(), tris_in.to_vec(), colors_in.to_vec());
    }
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in positions {
        for k in 0..3 {
            min[k] = min[k].min(v[k]);
            max[k] = max[k].max(v[k]);
        }
    }
    let longest = (max[0] - min[0]).max(max[1] - min[1]).max(max[2] - min[2]).max(1e-9);
    let mut cells = 600.0_f32;
    loop {
        let cell = longest / cells;
        let key = |v: &V3| -> (i32, i32, i32) { (((v[0] - min[0]) / cell) as i32, ((v[1] - min[1]) / cell) as i32, ((v[2] - min[2]) / cell) as i32) };
        let mut ids: HashMap<(i32, i32, i32), u32> = HashMap::new();
        let mut sums: Vec<([f64; 3], u32)> = vec![];
        let remap: Vec<u32> = positions
            .iter()
            .map(|v| {
                let id = *ids.entry(key(v)).or_insert_with(|| {
                    sums.push(([0.0; 3], 0));
                    (sums.len() - 1) as u32
                });
                let s = &mut sums[id as usize];
                s.0[0] += v[0] as f64;
                s.0[1] += v[1] as f64;
                s.0[2] += v[2] as f64;
                s.1 += 1;
                id
            })
            .collect();
        let mut seen = std::collections::HashSet::new();
        let mut tris = vec![];
        let mut colors = vec![];
        for (t, c) in tris_in.iter().zip(colors_in) {
            let n = [remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize]];
            if n[0] == n[1] || n[1] == n[2] || n[0] == n[2] {
                continue;
            }
            let mut sorted = n;
            sorted.sort_unstable();
            if seen.insert(sorted) {
                tris.push(n);
                colors.push(*c);
            }
        }
        if tris.len() <= target || cells < 6.0 {
            let pos = sums.iter().map(|(s, n)| [(s[0] / *n as f64) as f32, (s[1] / *n as f64) as f32, (s[2] / *n as f64) as f32]).collect();
            return (pos, tris, colors);
        }
        cells *= (target as f32 / tris.len() as f32).sqrt().clamp(0.4, 0.92);
    }
}

// ---------- lecture des formats ----------

pub fn load(path: &Path) -> Result<Model, String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "obj" => load_obj(std::io::BufReader::new(std::fs::File::open(path).map_err(|e| e.to_string())?)),
        "stl" => load_stl(&std::fs::read(path).map_err(|e| e.to_string())?),
        "3ds" => formats3d::load_3ds(&std::fs::read(path).map_err(|e| e.to_string())?),
        "ply" => formats3d::load_ply(&std::fs::read(path).map_err(|e| e.to_string())?),
        "ma" => formats3d::load_ma(&std::fs::read_to_string(path).map_err(|e| e.to_string())?),
        "mb" => Err("Les .mb (Maya binaire) ne se lisent pas sans Maya : dans Maya, enregistre en .ma (Maya ASCII), ou exporte en FBX ou OBJ.".into()),
        "max" => Err("Les .max se lisent seulement dans 3ds Max : exporte en FBX, OBJ ou 3DS.".into()),
        "fbx" => {
            let m = crate::fbx::load(&std::fs::read(path).map_err(|e| e.to_string())?)?;
            Model::with_edges(m.positions, m.tris, None, Some(m.edges), false)
        }
        "glb" | "gltf" => load_gltf(path),
        "arya3" | "aryscene" => crate::scene::build(&crate::scene::Scene::read(path)?, path).0,
        _ => Err("Format 3D non pris en charge.".into()),
    }
}

/// OBJ lu ligne par ligne (les fichiers de plusieurs Go ne sont pas chargés d'un bloc).
pub fn load_obj(reader: impl BufRead) -> Result<Model, String> {
    let mut verts: Vec<V3> = vec![];
    let mut tris = vec![];
    let mut edges = vec![];
    let mut poly: Vec<u32> = vec![];
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix("v ") {
            let mut it = rest.split_whitespace().filter_map(|x| x.parse::<f32>().ok());
            if let (Some(x), Some(y), Some(z)) = (it.next(), it.next(), it.next()) {
                verts.push([x, y, z]);
            }
        } else if let Some(rest) = line.strip_prefix("f ") {
            poly.clear();
            for tok in rest.split_whitespace() {
                let Some(i) = tok.split('/').next().and_then(|s| s.parse::<i64>().ok()) else { continue };
                let idx = if i > 0 { i - 1 } else { verts.len() as i64 + i };
                if idx >= 0 {
                    poly.push(idx as u32);
                }
            }
            let n = poly.len();
            for k in 1..n.saturating_sub(1) {
                tris.push([poly[0], poly[k], poly[k + 1]]);
                edges.push(fan_mask(k, n));
            }
        }
    }
    Model::with_edges(verts, tris, None, Some(edges), false)
}

/// Arêtes réelles du triangle k d'un polygone de n côtés découpé en éventail depuis le sommet 0.
pub fn fan_mask(k: usize, n: usize) -> u8 {
    let mut m = 0b010; // v1-v2 : toujours un côté du polygone
    if k == 1 {
        m |= 0b001;
    }
    if k + 2 == n {
        m |= 0b100;
    }
    m
}

pub fn load_stl(bytes: &[u8]) -> Result<Model, String> {
    let mut soup: Vec<V3> = vec![];
    let is_ascii = bytes.starts_with(b"solid") && std::str::from_utf8(&bytes[..bytes.len().min(512)]).map(|s| s.contains("facet")).unwrap_or(false);
    if is_ascii {
        for line in String::from_utf8_lossy(bytes).lines() {
            let mut it = line.split_whitespace();
            if it.next() == Some("vertex") {
                let c: Vec<f32> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                if c.len() == 3 {
                    soup.push([c[0], c[1], c[2]]);
                }
            }
        }
    } else {
        if bytes.len() < 84 {
            return Err("STL trop court.".into());
        }
        let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        let f = |o: usize| f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        for i in 0..n {
            let o = 84 + i * 50 + 12;
            if o + 36 > bytes.len() {
                break;
            }
            for k in 0..3 {
                soup.push([f(o + k * 12), f(o + k * 12 + 4), f(o + k * 12 + 8)]);
            }
        }
    }
    // Les STL répètent les sommets : on les fusionne.
    let mut ids: HashMap<[u32; 3], u32> = HashMap::new();
    let mut positions = vec![];
    let idx: Vec<u32> = soup
        .iter()
        .map(|v| {
            *ids.entry([v[0].to_bits(), v[1].to_bits(), v[2].to_bits()]).or_insert_with(|| {
                positions.push(*v);
                (positions.len() - 1) as u32
            })
        })
        .collect();
    let tris = idx.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
    Model::new(positions, tris, None, true)
}

fn load_gltf(path: &Path) -> Result<Model, String> {
    let (doc, buffers, _images) = gltf::import(path).map_err(|e| format!("glTF illisible : {e}"))?;
    let mut positions = vec![];
    let mut tris = vec![];
    let mut colors = vec![];
    fn mul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
        let mut r = [[0.0; 4]; 4];
        for c in 0..4 {
            for row in 0..4 {
                r[c][row] = (0..4).map(|k| a[k][row] * b[c][k]).sum();
            }
        }
        r
    }
    fn apply(m: &[[f32; 4]; 4], v: [f32; 3]) -> V3 {
        [
            m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2] + m[3][0],
            m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2] + m[3][1],
            m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2] + m[3][2],
        ]
    }
    let identity = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
    let scene = doc.default_scene().or_else(|| doc.scenes().next()).ok_or("Aucune scène.")?;
    let mut stack: Vec<(gltf::Node, [[f32; 4]; 4])> = scene.nodes().map(|n| (n, identity)).collect();
    while let Some((node, parent)) = stack.pop() {
        let world = mul(&parent, &node.transform().matrix());
        if let Some(mesh) = node.mesh() {
            for prim in mesh.primitives().filter(|p| p.mode() == gltf::mesh::Mode::Triangles) {
                let reader = prim.reader(|b| Some(&buffers[b.index()]));
                let Some(pos) = reader.read_positions() else { continue };
                let base = positions.len() as u32;
                positions.extend(pos.map(|p| apply(&world, p)));
                let count = positions.len() as u32 - base;
                let c = prim.material().pbr_metallic_roughness().base_color_factor();
                let to8 = |x: f32| (x.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8;
                let col = Color32::from_rgb(to8(c[0]), to8(c[1]), to8(c[2]));
                let idx: Vec<u32> = match reader.read_indices() {
                    Some(i) => i.into_u32().collect(),
                    None => (0..count).collect(),
                };
                for t in idx.chunks_exact(3) {
                    tris.push([base + t[0], base + t[1], base + t[2]]);
                    colors.push(col);
                }
            }
        }
        for child in node.children() {
            stack.push((child, world));
        }
    }
    Model::new(positions, tris, Some(colors), false)
}

// ---------- rendu « pixols » ----------
//
// Comme ZBrush : chaque pixel est calculé avec sa profondeur (z-buffer) dans une image,
// puis affiché comme une texture. Rendu en deux passes : on trouve d'abord le triangle
// visible de chaque pixel, puis on l'éclaire (HDRI + lumière), avec le fil de fer
// éventuel par-dessus. On ne recalcule l'image que quand quelque chose change.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Material {
    Clay,
    Gray,
    White,
    Chrome,
    Normals,
    Color,
}

impl Material {
    pub const ALL: [(Material, &'static str); 6] = [
        (Material::Clay, "argile"),
        (Material::Gray, "gris"),
        (Material::White, "blanc"),
        (Material::Chrome, "chrome"),
        (Material::Normals, "normales"),
        (Material::Color, "couleurs du fichier"),
    ];
    /// Couleur de base (linéaire) et rugosité.
    fn params(self) -> ([f32; 3], f32, f32) {
        match self {
            Material::Clay => ([0.56, 0.21, 0.13], 0.75, 0.04),
            Material::Gray => ([0.40, 0.40, 0.42], 0.45, 0.04),
            Material::White => ([0.80, 0.79, 0.76], 0.55, 0.04),
            Material::Chrome => ([0.0, 0.0, 0.0], 0.08, 0.9),
            Material::Normals | Material::Color => ([0.6, 0.6, 0.6], 0.5, 0.04),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub pan: [f32; 2],
    pub ortho: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Self { yaw: 0.6, pitch: 0.35, dist: 2.2, pan: [0.0, 0.0], ortho: false }
    }
}

impl Camera {
    fn rotate(&self, v: V3) -> V3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let x = v[0] * cy + v[2] * sy;
        let z = -v[0] * sy + v[2] * cy;
        let y = v[1] * cp - z * sp;
        let z = v[1] * sp + z * cp;
        [x, y, z]
    }
    /// Inverse de `rotate` : de l'espace caméra vers le monde.
    fn unrotate(&self, v: V3) -> V3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let y = v[1] * cp + v[2] * sp;
        let z1 = -v[1] * sp + v[2] * cp;
        [v[0] * cy - z1 * sy, y, v[0] * sy + z1 * cy]
    }
    /// Espace caméra : la caméra regarde vers -Z.
    fn view(&self, v: V3) -> V3 {
        let r = self.rotate(v);
        [r[0] + self.pan[0], r[1] + self.pan[1], r[2] - self.dist]
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Light {
    /// Angle horizontal et hauteur, en radians.
    pub azimuth: f32,
    pub elevation: f32,
    pub intensity: f32,
    pub color: [f32; 3],
}

impl Default for Light {
    fn default() -> Self {
        Self { azimuth: 0.6, elevation: 0.7, intensity: 1.2, color: [1.0, 0.97, 0.92] }
    }
}

impl Light {
    fn dir(&self) -> V3 {
        let (sa, ca) = self.azimuth.sin_cos();
        let (se, ce) = self.elevation.sin_cos();
        norm([sa * ce, se, ca * ce])
    }
}

// ---------- environnements (HDRI) ----------

/// Image d'environnement en latitude-longitude, en valeurs HDR linéaires, avec ses
/// versions floues (reflets plus ou moins nets) et l'éclairage diffus (harmoniques sphériques).
pub struct Env {
    pub name: String,
    bg: Map,
    bg_blur: Map,
    sharp: Map,
    rough: Map,
    sh: [[f32; 3]; 9],
}

#[derive(Clone)]
struct Map {
    w: usize,
    h: usize,
    px: Vec<[f32; 3]>,
}

impl Map {
    fn dir_of(&self, x: f32, y: f32) -> V3 {
        let phi = (x / self.w as f32 - 0.5) * std::f32::consts::TAU;
        let theta = y / self.h as f32 * std::f32::consts::PI;
        let (st, ct) = theta.sin_cos();
        [st * phi.sin(), ct, -st * phi.cos()]
    }
    fn sample(&self, d: V3) -> [f32; 3] {
        let u = d[0].atan2(-d[2]) / std::f32::consts::TAU + 0.5;
        let v = d[1].clamp(-1.0, 1.0).acos() / std::f32::consts::PI;
        let x = u * self.w as f32 - 0.5;
        let y = (v * self.h as f32 - 0.5).clamp(0.0, self.h as f32 - 1.0);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let xi = |k: f32| ((k as i64).rem_euclid(self.w as i64)) as usize;
        let yi = |k: f32| (k as usize).min(self.h - 1);
        let p = |xx: usize, yy: usize| self.px[yy * self.w + xx];
        let (a, b, c, e) = (p(xi(x0), yi(y0)), p(xi(x0 + 1.0), yi(y0)), p(xi(x0), yi(y0 + 1.0)), p(xi(x0 + 1.0), yi(y0 + 1.0)));
        let mut out = [0.0; 3];
        for k in 0..3 {
            out[k] = (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + e[k] * fx) * fy;
        }
        out
    }
    fn resized(&self, w: usize, h: usize) -> Map {
        // Moyenne par blocs (réduction) ou échantillonnage (agrandissement).
        let mut px = vec![[0.0f32; 3]; w * h];
        let (sx, sy) = (self.w as f32 / w as f32, self.h as f32 / h as f32);
        for y in 0..h {
            for x in 0..w {
                let (x0, x1) = ((x as f32 * sx) as usize, (((x + 1) as f32 * sx) as usize).max((x as f32 * sx) as usize + 1).min(self.w));
                let (y0, y1) = ((y as f32 * sy) as usize, (((y + 1) as f32 * sy) as usize).max((y as f32 * sy) as usize + 1).min(self.h));
                let mut s = [0.0f32; 3];
                let mut n = 0.0;
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        let c = self.px[yy * self.w + xx];
                        s[0] += c[0];
                        s[1] += c[1];
                        s[2] += c[2];
                        n += 1.0;
                    }
                }
                px[y * w + x] = [s[0] / n, s[1] / n, s[2] / n];
            }
        }
        Map { w, h, px }
    }
    /// Flou séparable (3 passes de boîte, le bord gauche/droit se rejoint).
    fn blurred(&self, r: usize) -> Map {
        let mut m = self.clone();
        for _ in 0..3 {
            let mut tmp = m.px.clone();
            for y in 0..m.h {
                // Plus on monte vers les pôles, plus les pixels sont serrés : flou horizontal élargi.
                let lat = ((y as f32 + 0.5) / m.h as f32 * std::f32::consts::PI).sin().max(0.15);
                let rx = ((r as f32 / lat) as usize).min(m.w / 2);
                for x in 0..m.w {
                    let mut s = [0.0; 3];
                    for k in 0..=2 * rx {
                        let xx = (x + m.w * 2 + k - rx) % m.w;
                        let c = m.px[y * m.w + xx];
                        s[0] += c[0];
                        s[1] += c[1];
                        s[2] += c[2];
                    }
                    let n = (2 * rx + 1) as f32;
                    tmp[y * m.w + x] = [s[0] / n, s[1] / n, s[2] / n];
                }
            }
            for y in 0..m.h {
                for x in 0..m.w {
                    let mut s = [0.0; 3];
                    let mut n = 0.0;
                    for yy in y.saturating_sub(r)..(y + r + 1).min(m.h) {
                        let c = tmp[yy * m.w + x];
                        s[0] += c[0];
                        s[1] += c[1];
                        s[2] += c[2];
                        n += 1.0;
                    }
                    m.px[y * m.w + x] = [s[0] / n, s[1] / n, s[2] / n];
                }
            }
        }
        m
    }
}

fn sh_basis(d: V3) -> [f32; 9] {
    let (x, y, z) = (d[0], d[1], d[2]);
    [0.282095, 0.488603 * y, 0.488603 * z, 0.488603 * x, 1.092548 * x * y, 1.092548 * y * z, 0.315392 * (3.0 * z * z - 1.0), 1.092548 * x * z, 0.546274 * (x * x - y * y)]
}

impl Env {
    fn from_map(name: &str, full: Map) -> Env {
        let bg = if full.w > 1024 { full.resized(1024, 512) } else { full };
        let small = bg.resized(128, 64);
        let sharp = small.blurred(1);
        let rough = small.resized(64, 32).blurred(4);
        let bg_blur = small.blurred(2);
        // Éclairage diffus : projection sur 9 harmoniques sphériques.
        let sh_src = small.resized(64, 32);
        let mut sh = [[0.0f32; 3]; 9];
        let dphi = std::f32::consts::TAU / sh_src.w as f32;
        let dtheta = std::f32::consts::PI / sh_src.h as f32;
        for y in 0..sh_src.h {
            let theta = (y as f32 + 0.5) * dtheta;
            let dw = dphi * dtheta * theta.sin();
            for x in 0..sh_src.w {
                let d = sh_src.dir_of(x as f32 + 0.5, y as f32 + 0.5);
                let b = sh_basis(d);
                let c = sh_src.px[y * sh_src.w + x];
                for k in 0..9 {
                    for ch in 0..3 {
                        sh[k][ch] += c[ch] * b[k] * dw;
                    }
                }
            }
        }
        Env { name: name.to_string(), bg, bg_blur, sharp, rough, sh }
    }

    /// Lumière diffuse reçue par une surface de normale `n` (déjà divisée par π).
    fn irradiance(&self, n: V3) -> [f32; 3] {
        let b = sh_basis(n);
        let a = [1.0, 2.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0, 0.25, 0.25, 0.25, 0.25, 0.25];
        let mut e = [0.0f32; 3];
        for k in 0..9 {
            for ch in 0..3 {
                e[ch] += a[k] * self.sh[k][ch] * b[k];
            }
        }
        [e[0].max(0.0), e[1].max(0.0), e[2].max(0.0)]
    }

    fn reflect(&self, d: V3, rough: f32) -> [f32; 3] {
        let a = self.sharp.sample(d);
        let b = self.rough.sample(d);
        let t = (rough * 1.6).clamp(0.0, 1.0);
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
    }

    /// Ouvre un fichier .hdr ou .exr.
    pub fn load_file(path: &Path) -> Result<Env, String> {
        let img = image::open(path).map_err(|e| format!("HDRI illisible : {e}"))?;
        let img = img.to_rgb32f();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let px: Vec<[f32; 3]> = img.pixels().map(|p| [p[0].max(0.0), p[1].max(0.0), p[2].max(0.0)]).collect();
        let full = Map { w, h, px };
        // On réduit tout de suite les très grandes images (8K et plus).
        let full = if w > 2048 { full.resized(2048, 1024) } else { full };
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "hdri".into());
        Ok(Env::from_map(&name, full))
    }

    pub fn preset(i: usize) -> std::sync::Arc<Env> {
        static CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<usize, std::sync::Arc<Env>>>> = std::sync::OnceLock::new();
        let cache = CACHE.get_or_init(Default::default);
        if let Some(e) = cache.lock().ok().and_then(|c| c.get(&i).cloned()) {
            return e;
        }
        let i = i.min(PRESETS.len() - 1);
        let (w, h) = (512, 256);
        let mut m = Map { w, h, px: vec![[0.0; 3]; w * h] };
        for y in 0..h {
            for x in 0..w {
                let d = m.dir_of(x as f32 + 0.5, y as f32 + 0.5);
                m.px[y * w + x] = preset_radiance(i, d);
            }
        }
        let e = std::sync::Arc::new(Env::from_map(PRESETS[i], m));
        if let Ok(mut c) = cache.lock() {
            c.insert(i, e.clone());
        }
        e
    }
}

pub const PRESETS: [&str; 6] = ["studio", "studio doux", "extérieur", "coucher de soleil", "nuit", "neutre"];

fn spot(d: V3, az: f32, el: f32) -> f32 {
    let (sa, ca) = az.to_radians().sin_cos();
    let (se, ce) = el.to_radians().sin_cos();
    dot(d, [sa * ce, se, -ca * ce])
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn preset_radiance(i: usize, d: V3) -> [f32; 3] {
    let up = d[1];
    let soft = |c: f32, edge: f32| ((c - edge) / (1.0 - edge)).clamp(0.0, 1.0).powf(0.6);
    match i {
        // Studio : boîtes à lumière (principale, d'appoint, contre-jour) sur fond gris.
        0 | 1 => {
            let base = lerp3([0.05, 0.05, 0.055], [0.30, 0.30, 0.32], (up * 0.5 + 0.5).powf(1.5));
            let k = if i == 0 { 1.0 } else { 0.35 };
            let key = soft(spot(d, -40.0, 35.0), 0.93) * 9.0 * k;
            let fill = soft(spot(d, 65.0, 15.0), 0.9) * 2.0 * k;
            let rim = soft(spot(d, 170.0, 40.0), 0.95) * 7.0 * k;
            let top = soft(up, 0.9) * 2.5 * k;
            let add = key + fill + rim + top + if i == 1 { 0.4 } else { 0.0 };
            [base[0] + add, base[1] + add * 0.99, base[2] + add * 0.97]
        }
        // Extérieur : ciel bleu, soleil, sol.
        2 => {
            let sun = spot(d, 30.0, 40.0);
            let mut c = if up > 0.0 { lerp3([0.85, 0.9, 1.0], [0.25, 0.45, 0.95], up.powf(0.5)) } else { lerp3([0.35, 0.32, 0.28], [0.12, 0.11, 0.09], (-up).powf(0.4)) };
            if sun > 0.9985 {
                c = [500.0, 470.0, 420.0];
            } else {
                let g = soft(sun, 0.9) * 2.0;
                c = [c[0] + g, c[1] + g * 0.95, c[2] + g * 0.85];
            }
            c
        }
        // Coucher de soleil : horizon orange, ciel violet, soleil rasant.
        3 => {
            let sun = spot(d, -60.0, 4.0);
            let mut c = if up > 0.0 { lerp3([1.6, 0.6, 0.25], [0.12, 0.10, 0.30], up.powf(0.45)) } else { lerp3([0.25, 0.12, 0.08], [0.03, 0.03, 0.04], (-up).powf(0.4)) };
            if sun > 0.998 {
                c = [200.0, 110.0, 40.0];
            } else {
                let g = soft(sun, 0.8) * 3.0;
                c = [c[0] + g, c[1] + g * 0.5, c[2] + g * 0.2];
            }
            c
        }
        // Nuit : ciel sombre et lune.
        4 => {
            let moon = spot(d, 120.0, 35.0);
            let mut c = if up > 0.0 { lerp3([0.05, 0.06, 0.12], [0.01, 0.015, 0.04], up.powf(0.5)) } else { [0.01, 0.01, 0.015] };
            if moon > 0.9993 {
                c = [30.0, 32.0, 40.0];
            } else {
                let g = soft(moon, 0.95) * 0.3;
                c = [c[0] + g, c[1] + g, c[2] + g * 1.2];
            }
            c
        }
        _ => [0.6, 0.6, 0.6],
    }
}

// ---------- maillage préparé ----------

/// Normales lissées par sommet (moyenne des faces), pondérées par l'aire.
fn vertex_normals(pos: &[V3], tris: &[[u32; 3]]) -> Vec<V3> {
    let mut n = vec![[0.0f32; 3]; pos.len()];
    for t in tris {
        let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        let f = cross(sub(b, a), sub(c, a));
        for &i in t {
            let v = &mut n[i as usize];
            v[0] += f[0];
            v[1] += f[1];
            v[2] += f[2];
        }
    }
    n.into_iter().map(norm).collect()
}

/// Courbure par sommet, comme une carte de courbure de Substance : positive sur les arêtes
/// saillantes, négative dans les creux, à peu près nulle sur les surfaces planes ou doucement
/// bombées. On compare chaque sommet à la surface lissée sur un rayon fixe (2,5 % de la taille
/// du modèle) : le résultat ne dépend ni de la densité du maillage ni de sa taille. Rien à cuire.
pub(crate) fn vertex_curvature(pos: &[V3], nrm: &[V3], tris: &[[u32; 3]]) -> Vec<f32> {
    let n = pos.len();
    if n == 0 {
        return vec![];
    }
    let mut edges: Vec<(u32, u32)> = Vec::with_capacity(tris.len() * 3);
    for t in tris {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            if a != b {
                edges.push((a.min(b), a.max(b)));
            }
        }
    }
    edges.sort_unstable();
    edges.dedup();
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let size = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
    let avg = edges.iter().map(|&(a, b)| dot(sub(pos[a as usize], pos[b as usize]), sub(pos[a as usize], pos[b as usize])).sqrt()).sum::<f32>() / edges.len().max(1) as f32;
    let radius = size * 0.025;
    // Une diffusion de k pas s'étale sur environ arête × √k
    let iters = ((radius / avg.max(1e-9)).powi(2) as usize).clamp(2, 160);
    let mut sm = pos.to_vec();
    let mut sum = vec![[0.0f32; 3]; n];
    let mut cnt = vec![0.0f32; n];
    for _ in 0..iters {
        sum.iter_mut().for_each(|s| *s = [0.0; 3]);
        cnt.iter_mut().for_each(|c| *c = 0.0);
        for &(a, b) in &edges {
            let (a, b) = (a as usize, b as usize);
            for k in 0..3 {
                sum[a][k] += sm[b][k];
                sum[b][k] += sm[a][k];
            }
            cnt[a] += 1.0;
            cnt[b] += 1.0;
        }
        for i in 0..n {
            if cnt[i] > 0.0 {
                let c = cnt[i];
                // Pas à moitié : plus stable
                sm[i] = [(sm[i][0] + sum[i][0] / c) * 0.5, (sm[i][1] + sum[i][1] / c) * 0.5, (sm[i][2] + sum[i][2] / c) * 0.5];
            }
        }
    }
    (0..n).map(|i| dot(sub(pos[i], sm[i]), nrm[i]) / radius).collect()
}

/// Données prêtes pour le rendu (modèle centré, mis à l'échelle, normales).
pub struct Prepared {
    pos: Vec<V3>,
    nrm: Vec<V3>,
    tris: Vec<[u32; 3]>,
    colors: Vec<[f32; 3]>,
    edges: Vec<u8>,
    pub floor: f32,
    /// UV par coin de triangle (vide si le maillage n'en a pas)
    pub uv: Vec<[[f32; 2]; 3]>,
    /// ID de matériau par triangle (vide : un seul matériau)
    pub mat: Vec<u16>,
    /// Courbure par sommet, calculée à la demande (node « courbure »)
    curv: std::sync::OnceLock<Vec<f32>>,
    /// Arbre de rayons pour les ombres portées, construit au premier besoin
    bvh: std::sync::OnceLock<crate::bvh::Bvh>,
}

impl Prepared {
    pub fn new(model: &Model, z_up: bool, levels: u8) -> Self {
        let mut pos = model.normalized(z_up);
        let mut tris = model.tris.clone();
        let mut colors = model.colors.clone();
        let mut edges = model.edges.clone();
        if levels > 0 {
            (pos, tris, colors, edges) = catmull_clark(&pos, &tris, &colors, &edges, levels, false);
        }
        let nrm = vertex_normals(&pos, &tris);
        let floor = pos.iter().map(|v| v[1]).fold(f32::MAX, f32::min);
        let lin = |c: u8| (c as f32 / 255.0).powf(2.2);
        let colors = colors.iter().map(|c| [lin(c.r()), lin(c.g()), lin(c.b())]).collect();
        Self { pos, nrm, tris, colors, edges, floor, uv: vec![], mat: vec![], curv: std::sync::OnceLock::new(), bvh: std::sync::OnceLock::new() }
    }
    pub fn tri_count(&self) -> usize {
        self.tris.len()
    }

    /// Depuis un maillage modifié : recentré et remis à l'échelle pour tenir dans la vue.
    /// Pour une scène : positions gardées telles quelles (pas de recentrage), sol fixé par l'appelant.
    pub fn from_mesh_fixed(mut m: crate::modifiers::Mesh, floor: f32) -> Self {
        split_sharp(&mut m.pos, &mut m.tris, 38.0);
        let nrm = vertex_normals(&m.pos, &m.tris);
        let lin = |c: u8| (c as f32 / 255.0).powf(2.2);
        let colors = m.colors.iter().map(|c| [lin(c.r()), lin(c.g()), lin(c.b())]).collect();
        Self { pos: m.pos, nrm, tris: m.tris, colors, edges: m.edges, floor, uv: vec![], mat: vec![], curv: std::sync::OnceLock::new(), bvh: std::sync::OnceLock::new() }
    }

    pub fn from_mesh(m: crate::modifiers::Mesh) -> Self {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for p in &m.pos {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let longest = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
        let k = if longest > 1.0 { 1.0 / longest } else { 1.0 };
        let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let pos: Vec<V3> = m.pos.iter().map(|p| [(p[0] - c[0]) * k, (p[1] - c[1]) * k, (p[2] - c[2]) * k]).collect();
        let nrm = vertex_normals(&pos, &m.tris);
        let floor = pos.iter().map(|v| v[1]).fold(f32::MAX, f32::min);
        let lin = |c: u8| (c as f32 / 255.0).powf(2.2);
        let colors = m.colors.iter().map(|c| [lin(c.r()), lin(c.g()), lin(c.b())]).collect();
        Self { pos, nrm, tris: m.tris, colors, edges: m.edges, floor, uv: vec![], mat: vec![], curv: std::sync::OnceLock::new(), bvh: std::sync::OnceLock::new() }
    }
}

pub const MAX_SUBDIV_TRIS: usize = 2_500_000;

/// Transformation d'un objet de la scène, parents compris, sous forme affine (colonnes, origine).
fn object_affine(sc: &crate::scene::Scene, k: usize) -> ([V3; 3], V3) {
    let o = sc.world(k, [0.0; 3]);
    let col = |e: V3| {
        let w = sc.world(k, e);
        [w[0] - o[0], w[1] - o[1], w[2] - o[2]]
    };
    ([col([1.0, 0.0, 0.0]), col([0.0, 1.0, 0.0]), col([0.0, 0.0, 1.0])], o)
}

fn inv3(m: [V3; 3]) -> Option<[V3; 3]> {
    // m est donné par colonnes ; on inverse la matrice dont ce sont les colonnes
    let [a, b, c] = m;
    let det = a[0] * (b[1] * c[2] - c[1] * b[2]) - b[0] * (a[1] * c[2] - c[1] * a[2]) + c[0] * (a[1] * b[2] - b[1] * a[2]);
    if det.abs() < 1e-12 {
        return None;
    }
    let r0 = cross(b, c);
    let r1 = cross(c, a);
    let r2 = cross(a, b);
    // Lignes de l'inverse = r_i / det ; renvoyées en colonnes
    Some([[r0[0] / det, r1[0] / det, r2[0] / det], [r0[1] / det, r1[1] / det, r2[1] / det], [r0[2] / det, r1[2] / det, r2[2] / det]])
}

fn mulm(m: &[V3; 3], v: V3) -> V3 {
    [m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2], m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2], m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2]]
}

/// Déplacement rapide : si seules les poses des objets ont changé depuis `built`, renvoie le
/// maillage affiché avec les sommets des objets déplacés transformés sur place.
fn fast_move(built: &crate::scene::Scene, scene: &crate::scene::Scene, ranges: &[(usize, usize, usize)], prep: &Prepared, m: &Model) -> Option<Prepared> {
    if built.objects.len() != scene.objects.len() || prep.tris.len() != m.tris.len() {
        return None;
    }
    // Rien d'autre que la pose ne doit avoir changé (fichier, modificateurs, couleur, visibilité…)
    for (a, b) in built.objects.iter().zip(&scene.objects) {
        let mut b2 = b.clone();
        (b2.pos, b2.rot, b2.scale) = (a.pos, a.rot, a.scale);
        b2.keys = a.keys.clone();
        if &b2 != a {
            return None;
        }
    }
    let s = m.size();
    let scale = 1.0 / s[0].max(s[1]).max(s[2]).max(1e-9);
    let c = [(m.min[0] + m.max[0]) / 2.0, (m.min[1] + m.max[1]) / 2.0, (m.min[2] + m.max[2]) / 2.0];
    let mut pos = prep.pos.clone();
    let mut nrm = prep.nrm.clone();
    let mut any = false;
    for &(k, t0, t1) in ranges {
        let (la, oa) = object_affine(built, k);
        let (lb, ob) = object_affine(scene, k);
        if la == lb && oa == ob {
            continue;
        }
        let ia = inv3(la)?;
        // D = B ∘ A⁻¹ en coordonnées du monde ; L = partie linéaire
        let l = [mulm(&lb, ia[0]), mulm(&lb, ia[1]), mulm(&lb, ia[2])];
        let lt_inv = inv3([[l[0][0], l[1][0], l[2][0]], [l[0][1], l[1][1], l[2][1]], [l[0][2], l[1][2], l[2][2]]])?;
        let mut seen = std::collections::HashSet::new();
        for t in prep.tris.get(t0..t1)? {
            for &v in t {
                if !seen.insert(v) {
                    continue;
                }
                let n = pos[v as usize];
                let w = [n[0] / scale + c[0], n[1] / scale + c[1], n[2] / scale + c[2]];
                let local = mulm(&ia, [w[0] - oa[0], w[1] - oa[1], w[2] - oa[2]]);
                let w2 = mulm(&lb, local);
                let w2 = [w2[0] + ob[0], w2[1] + ob[1], w2[2] + ob[2]];
                pos[v as usize] = [(w2[0] - c[0]) * scale, (w2[1] - c[1]) * scale, (w2[2] - c[2]) * scale];
                nrm[v as usize] = norm(mulm(&lt_inv, nrm[v as usize]));
            }
        }
        any = true;
    }
    if !any {
        return None;
    }
    Some(Prepared { pos, nrm, tris: prep.tris.clone(), colors: prep.colors.clone(), edges: prep.edges.clone(), floor: prep.floor, uv: prep.uv.clone(), mat: prep.mat.clone(), curv: std::sync::OnceLock::new(), bvh: std::sync::OnceLock::new() })
}

/// Réglages d'affichage (comme le panneau d'affichage de Substance).
#[derive(Clone)]
pub struct Look {
    pub material: Material,
    pub smooth: bool,
    pub env: std::sync::Arc<Env>,
    pub env_rot: f32,
    pub env_strength: f32,
    pub exposure: f32,
    pub light: Light,
    pub show_bg: bool,
    pub blur_bg: bool,
    pub wire: f32,
    pub wire_color: [f32; 3],
    pub wire_only: bool,
    /// Matériau en nodes (remplace le matériau prédéfini).
    pub nodes: Option<std::sync::Arc<crate::nodes::Program>>,
    /// Damier sur les UV (vérifier l'étirement et la densité)
    pub uv_checker: bool,
    /// Matériaux en nodes par ID de matériau (mode édition) ; `slot_rev` change quand l'un d'eux change
    pub slot_nodes: Vec<Option<std::sync::Arc<crate::nodes::Program>>>,
    pub slot_rev: u64,
    /// Lumières de la scène ; vide : la lumière unique de la vue (`light`)
    pub lamps: Vec<Lamp>,
    /// Ombres portées (un rayon par lumière et par pixel)
    pub shadows: bool,
    /// Douceur des ombres (0 : bord net) et nombre de rayons par lumière
    pub shadow_soft: f32,
    pub shadow_samples: u8,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            material: Material::Clay,
            smooth: true,
            env: Env::preset(0),
            env_rot: 0.0,
            env_strength: 1.0,
            exposure: 0.0,
            light: Light::default(),
            show_bg: false,
            blur_bg: true,
            wire: 0.0,
            wire_color: [0.02, 0.02, 0.02],
            wire_only: false,
            nodes: None,
            uv_checker: false,
            slot_nodes: vec![],
            slot_rev: 0,
            lamps: vec![],
            shadows: false,
            shadow_soft: 0.35,
            shadow_samples: 8,
        }
    }
}

impl Look {
    /// Clé de comparaison : l'image n'est recalculée que si elle change.
    fn key(&self) -> (Material, bool, usize, [u32; 4], Light, bool, bool, [u32; 4], bool, usize, bool, u64) {
        (
            self.material,
            self.smooth,
            std::sync::Arc::as_ptr(&self.env) as usize,
            [self.env_rot.to_bits(), self.env_strength.to_bits(), self.exposure.to_bits(), {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                (self.shadows, self.shadow_soft.to_bits(), self.shadow_samples).hash(&mut h);
                for l in &self.lamps {
                    (l.point, l.at.map(f32::to_bits), l.color.map(f32::to_bits), l.intensity.to_bits(), l.range.to_bits()).hash(&mut h);
                }
                h.finish() as u32
            }],
            self.light,
            self.show_bg,
            self.blur_bg,
            [self.wire.to_bits(), self.wire_color[0].to_bits(), self.wire_color[1].to_bits(), self.wire_color[2].to_bits()],
            self.wire_only,
            self.nodes.as_ref().map(|n| std::sync::Arc::as_ptr(n) as usize).unwrap_or(0),
            self.uv_checker,
            self.slot_rev,
        )
    }
}

/// Une lumière du rendu, dans l'espace du modèle préparé.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Lamp {
    pub point: bool,
    /// Soleil : direction vers la lumière. Ponctuelle : sa position.
    pub at: V3,
    pub color: [f32; 3],
    pub intensity: f32,
    /// Portée d'une ponctuelle : au-delà, elle n'éclaire plus
    pub range: f32,
    /// Forme : 0 soleil, 1 ponctuelle, 2 spot, 3 rectangle
    pub shape: u8,
    /// Spot et rectangle : direction dans laquelle ils éclairent
    pub axis: V3,
    /// Spot : cosinus du bord du cône et du début du fondu
    pub cone: [f32; 2],
    /// Rectangle : demi-côtés (vecteurs dans la scène)
    pub half: [V3; 2],
}

fn sc3(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

/// Une lumière de la scène, dans l'espace normalisé de la vue (centrée sur `c`, taille 1).
pub fn lamp_of(l: &crate::scene::SceneLight, c: V3, longest: f32) -> Lamp {
    let k = 1.0 / longest.max(1e-9);
    Lamp {
        point: l.point,
        at: if l.point { [(l.at[0] - c[0]) * k, (l.at[1] - c[1]) * k, (l.at[2] - c[2]) * k] } else { l.at },
        color: l.color,
        intensity: l.energy,
        range: l.range * k,
        shape: l.shape,
        axis: l.axis,
        cone: l.cone,
        half: [sc3(l.half[0], k), sc3(l.half[1], k)],
    }
}

impl Lamp {
    pub fn simple(point: bool, at: V3, color: [f32; 3], intensity: f32, range: f32) -> Lamp {
        Lamp { point, at, color, intensity, range, shape: if point { 1 } else { 0 }, axis: [0.0, -1.0, 0.0], cone: [0.7, 0.8], half: [[0.0; 3]; 2] }
    }

    /// Direction vers la lumière depuis `p`, distance, et part de lumière reçue (portée, cône,
    /// inclinaison du rectangle). `u` choisit le point tiré sur la source (soleil élargi, rectangle).
    pub fn sample(&self, p: V3, u: [f32; 2]) -> (V3, f32, f32) {
        if self.shape == 0 && !self.point {
            let n = self.at;
            let a = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
            let t1 = norm(cross(n, a));
            let t2 = cross(n, t1);
            let (ang, rr) = (u[0] * std::f32::consts::TAU, u[1].sqrt() * 0.03);
            return (norm(add3(n, add3(sc3(t1, ang.cos() * rr), sc3(t2, ang.sin() * rr)))), 1e6, 1.0);
        }
        let src = if self.shape == 3 { add3(self.at, add3(sc3(self.half[0], 2.0 * u[0] - 1.0), sc3(self.half[1], 2.0 * u[1] - 1.0))) } else { self.at };
        let to = sub(src, p);
        let d = dot(to, to).sqrt().max(1e-6);
        let dir = sc3(to, 1.0 / d);
        let mut k = (1.0 - (d / self.range.max(1e-6)).min(1.0).powi(2)).powi(2);
        let facing = -dot(dir, self.axis);
        match self.shape {
            2 => {
                let t = ((facing - self.cone[0]) / (self.cone[1] - self.cone[0]).max(1e-4)).clamp(0.0, 1.0);
                k *= t * t * (3.0 - 2.0 * t);
            }
            3 => k *= facing.max(0.0),
            _ => {}
        }
        (dir, d, k)
    }
}

pub struct Frame {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
    /// Triangle visible à chaque pixel (u32::MAX : fond)
    pub ids: Vec<u32>,
    /// Profondeur à chaque pixel, dans l'espace caméra (plus grand = plus près ; f32::MIN : fond)
    pub depth: Vec<f32>,
}

fn rot_y(d: V3, a: f32) -> V3 {
    let (s, c) = a.sin_cos();
    [d[0] * c + d[2] * s, d[1], -d[0] * s + d[2] * c]
}

/// Courbe de tons « filmique » (ACES approchée) puis sRGB.
fn tone(c: [f32; 3], exposure: f32) -> [u8; 3] {
    let k = exposure;
    let f = |x: f32| {
        let x = (x * k).max(0.0);
        let y = (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14);
        (y.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0 + 0.5) as u8
    };
    [f(c[0]), f(c[1]), f(c[2])]
}

/// Rendu en parallèle (une bande d'image par cœur).
/// Damier UV (8 × 8 cases, lettres de couleur par quart) pour voir l'étirement.
fn checker(uv: [f32; 2], tint: Option<[f32; 3]>) -> [f32; 3] {
    let (u, v) = (uv[0] * 8.0, uv[1] * 8.0);
    let cell = (u.floor() as i64 + v.floor() as i64).rem_euclid(2) == 0;
    // Teinte selon le quart de la texture : on voit les îlots retournés ou mal placés
    let q = [(uv[0].rem_euclid(1.0) * 2.0) as usize % 2, (uv[1].rem_euclid(1.0) * 2.0) as usize % 2];
    let base: [f32; 3] = [[0.85, 0.35, 0.3], [0.3, 0.6, 0.85], [0.35, 0.75, 0.4], [0.9, 0.75, 0.3]][q[0] + q[1] * 2];
    let fine = ((u * 4.0).fract() < 0.06) || ((v * 4.0).fract() < 0.06);
    let k = if cell { 0.9 } else { 0.35 };
    let mut c = [base[0] * k, base[1] * k, base[2] * k];
    if fine {
        c = [c[0] * 0.8, c[1] * 0.8, c[2] * 0.8];
    }
    if let Some(t) = tint {
        c = [c[0] * 0.75 + t[0] * 0.25, c[1] * 0.75 + t[1] * 0.25, c[2] * 0.75 + t[2] * 0.25];
    }
    let l = |x: f32| x.powf(2.2);
    [l(c[0]), l(c[1]), l(c[2])]
}

/// Triangle sous un point de l'écran (même projection que le rendu). Retourne
/// (triangle, coordonnées barycentriques).
pub fn pick(m: &Prepared, cam: &Camera, w: f32, h: f32, px: f32, py: f32) -> Option<(usize, [f32; 3])> {
    let focal = w.min(h) * 1.2;
    let ortho_scale = focal / cam.dist;
    let (cx, cy) = (w / 2.0, h / 2.0);
    let proj = |v: V3| -> Option<[f32; 3]> {
        let c = cam.view(v);
        if cam.ortho {
            Some([cx + c[0] * ortho_scale, cy - c[1] * ortho_scale, c[2]])
        } else if c[2] < -0.02 {
            Some([cx + c[0] / -c[2] * focal, cy - c[1] / -c[2] * focal, c[2]])
        } else {
            None
        }
    };
    let screen: Vec<Option<[f32; 3]>> = m.pos.iter().map(|v| proj(*v)).collect();
    let mut best: Option<(usize, [f32; 3], f32)> = None;
    for (ti, t) in m.tris.iter().enumerate() {
        let (Some(a), Some(b), Some(c)) = (screen[t[0] as usize], screen[t[1] as usize], screen[t[2] as usize]) else { continue };
        let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
        if d.abs() < 1e-9 {
            continue;
        }
        let w0 = ((b[1] - c[1]) * (px - c[0]) + (c[0] - b[0]) * (py - c[1])) / d;
        let w1 = ((c[1] - a[1]) * (px - c[0]) + (a[0] - c[0]) * (py - c[1])) / d;
        let w2 = 1.0 - w0 - w1;
        if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
            continue;
        }
        let z = a[2] * w0 + b[2] * w1 + c[2] * w2;
        if best.as_ref().is_none_or(|bb| z > bb.2) {
            best = Some((ti, [w0, w1, w2], z));
        }
    }
    best.map(|(t, b, _)| (t, b))
}

/// Position d'un sommet dans l'espace d'affichage.
impl Prepared {
    pub fn vertex(&self, i: usize) -> V3 {
        self.pos[i]
    }
    pub fn triangle(&self, t: usize) -> [u32; 3] {
        self.tris[t]
    }
}

pub fn render(m: &Prepared, cam: &Camera, look: &Look, w: usize, h: usize) -> Frame {
    use rayon::prelude::*;
    let focal = w.min(h) as f32 * 1.2;
    let ortho_scale = focal / cam.dist;
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let screen: Vec<[f32; 3]> = m
        .pos
        .par_iter()
        .map(|v| {
            let c = cam.view(*v);
            if cam.ortho {
                [cx + c[0] * ortho_scale, cy - c[1] * ortho_scale, c[2]]
            } else if c[2] < -0.02 {
                [cx + c[0] / -c[2] * focal, cy - c[1] / -c[2] * focal, c[2]]
            } else {
                [f32::NAN, f32::NAN, c[2]]
            }
        })
        .collect();
    let exposure = 2f32.powf(look.exposure);
    let env = &look.env;
    let ld = look.light.dir();
    // Les lumières à calculer : celles de la scène, sinon la lumière de la vue
    let lamps: Vec<Lamp> = if look.lamps.is_empty() { vec![Lamp::simple(false, ld, look.light.color, look.light.intensity, 0.0)] } else { look.lamps.clone() };
    let bvh = look.shadows.then(|| m.bvh.get_or_init(|| crate::bvh::Bvh::new(&m.pos, &m.tris)));
    // Part de lumière qui arrive en `o` : plusieurs rayons légèrement écartés donnent un bord d'ombre doux.
    // Le motif d'écart tourne d'un pixel à l'autre, ce qui change les marches d'escalier en grain fin.
    let (soft, samples) = (look.shadow_soft.clamp(0.0, 1.0), if look.shadow_soft > 0.0 { look.shadow_samples.clamp(1, 32) as usize } else { 1 });
    let shadow = |b: &crate::bvh::Bvh, o: V3, dir: V3, reach: f32, point: bool, x: usize, y: usize| -> f32 {
        if samples <= 1 {
            return if b.occluded(o, dir, 1e-3, reach) { 0.0 } else { 1.0 };
        }
        let t1 = norm(cross(dir, if dir[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] }));
        let t2 = cross(dir, t1);
        let mut h = (x as u32).wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2C1B_3C6D);
        let turn = (h >> 8) as f32 / 16_777_216.0 * std::f32::consts::TAU;
        // Écart : un angle pour un soleil ; pour une ponctuelle, la taille de la source vue d'ici
        let spread = if point { soft * 0.06 / reach.max(0.05) } else { soft * 0.09 };
        let mut open = 0.0;
        for k in 0..samples {
            let r = ((k as f32 + 0.5) / samples as f32).sqrt() * spread;
            let a = turn + k as f32 * 2.399_963;
            let d = norm([dir[0] + (t1[0] * a.cos() + t2[0] * a.sin()) * r, dir[1] + (t1[1] * a.cos() + t2[1] * a.sin()) * r, dir[2] + (t1[2] * a.cos() + t2[2] * a.sin()) * r]);
            if !b.occluded(o, d, 1e-3, reach) {
                open += 1.0;
            }
        }
        open / samples as f32
    };
    let (albedo0, rough, metal) = look.material.params();
    let wire = look.wire;

    let band = 16usize;
    let mut rgba = vec![0u8; w * h * 4];
    let bands: Vec<(Vec<u32>, Vec<f32>)> = rgba.par_chunks_mut(w * 4 * band).enumerate().map(|(bi, chunk)| {
        let y0 = bi * band;
        let rows = chunk.len() / (w * 4);
        let n = w * rows;
        let mut depth = vec![f32::MIN; n];
        let mut tri = vec![u32::MAX; n];
        let mut bary = vec![[0.0f32; 2]; n];
        // Passe 1 : triangle visible de chaque pixel
        for (ti, t) in m.tris.iter().enumerate() {
            let (a, b, c) = (screen[t[0] as usize], screen[t[1] as usize], screen[t[2] as usize]);
            if a[0].is_nan() || b[0].is_nan() || c[0].is_nan() {
                continue;
            }
            let miny = a[1].min(b[1]).min(c[1]).floor().max(y0 as f32) as i32;
            let maxy = a[1].max(b[1]).max(c[1]).ceil().min((y0 + rows) as f32 - 1.0) as i32;
            if miny > maxy {
                continue;
            }
            let minx = a[0].min(b[0]).min(c[0]).floor().max(0.0) as i32;
            let maxx = a[0].max(b[0]).max(c[0]).ceil().min(w as f32 - 1.0) as i32;
            if minx > maxx {
                continue;
            }
            let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
            if area.abs() < 1e-9 {
                continue;
            }
            let inv = 1.0 / area;
            for y in miny..=maxy {
                let py = y as f32 + 0.5;
                for x in minx..=maxx {
                    let pxf = x as f32 + 0.5;
                    let w0 = ((b[0] - pxf) * (c[1] - py) - (c[0] - pxf) * (b[1] - py)) * inv;
                    let w1 = ((c[0] - pxf) * (a[1] - py) - (a[0] - pxf) * (c[1] - py)) * inv;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let z = w0 * a[2] + w1 * b[2] + w2 * c[2];
                    let k = (y as usize - y0) * w + x as usize;
                    if z > depth[k] {
                        depth[k] = z;
                        tri[k] = ti as u32;
                        bary[k] = [w0, w1];
                    }
                }
            }
        }
        // Passe 2 : éclairage
        for k in 0..n {
            let x = k % w;
            let y = y0 + k / w;
            let o = k * 4;
            // Direction du regard (du pixel vers la scène) en espace monde
            let ray_view = if cam.ortho { [0.0, 0.0, -1.0] } else { norm([(x as f32 + 0.5 - cx) / focal, -(y as f32 + 0.5 - cy) / focal, -1.0]) };
            let ray = cam.unrotate(ray_view);
            let ti = tri[k];
            if ti == u32::MAX {
                if look.show_bg {
                    let d = rot_y(ray, -look.env_rot);
                    let c = if look.blur_bg { env.bg_blur.sample(d) } else { env.bg.sample(d) };
                    let s = look.env_strength;
                    let t = tone([c[0] * s, c[1] * s, c[2] * s], exposure);
                    chunk[o..o + 3].copy_from_slice(&t);
                    chunk[o + 3] = 255;
                } else if let Some(b) = bvh {
                    // Sol : là où un objet cache une lumière, une ombre se pose sur le plan du sol
                    let eye = if cam.ortho { cam.unrotate([(x as f32 + 0.5 - cx) / ortho_scale - cam.pan[0], -(y as f32 + 0.5 - cy) / ortho_scale - cam.pan[1], cam.dist]) } else { cam.unrotate([-cam.pan[0], -cam.pan[1], cam.dist]) };
                    if ray[1].abs() > 1e-5 {
                        let tt = (m.floor - eye[1]) / ray[1];
                        if tt > 0.0 {
                            let hit = [eye[0] + ray[0] * tt, m.floor + 1e-3, eye[2] + ray[2] * tt];
                            let (mut dark, mut total) = (0.0f32, 0.0f32);
                            for lamp in &lamps {
                                let (dir, reach, fall) = if lamp.point { lamp.sample(hit, [0.5, 0.5]) } else { (lamp.at, 1e6, 1.0) };
                                let e = lamp.intensity * fall * dir[1].max(0.0);
                                if e <= 0.0 {
                                    continue;
                                }
                                total += e;
                                dark += e * (1.0 - shadow(b, hit, dir, reach, lamp.point, x, y));
                            }
                            if dark > 0.0 {
                                chunk[o..o + 3].copy_from_slice(&[0, 0, 0]);
                                chunk[o + 3] = (dark / total.max(1e-6) * 0.42 * 255.0) as u8;
                            }
                        }
                    }
                }
                continue;
            }
            let t = m.tris[ti as usize];
            let [w0, w1] = bary[k];
            let w2 = 1.0 - w0 - w1;
            let (pa, pb, pc) = (m.pos[t[0] as usize], m.pos[t[1] as usize], m.pos[t[2] as usize]);
            let face = norm(cross(sub(pb, pa), sub(pc, pa)));
            let mut nrm = if look.smooth {
                let (na, nb, nc) = (m.nrm[t[0] as usize], m.nrm[t[1] as usize], m.nrm[t[2] as usize]);
                norm([na[0] * w0 + nb[0] * w1 + nc[0] * w2, na[1] * w0 + nb[1] * w1 + nc[1] * w2, na[2] * w0 + nb[2] * w1 + nc[2] * w2])
            } else {
                face
            };
            let v = [-ray[0], -ray[1], -ray[2]];
            // Faces arrière : on retourne la normale pour éviter les trous noirs.
            if dot(face, v) < 0.0 {
                nrm = [-nrm[0], -nrm[1], -nrm[2]];
            }
            let mut col = if look.wire_only {
                let bg = if look.show_bg { None } else { Some([0.0f32; 3]) };
                bg.unwrap_or([0.0; 3])
            } else if look.material == Material::Normals {
                let nv = cam.rotate(nrm);
                let lin = |x: f32| (x * 0.5 + 0.5).powf(2.2);
                [lin(nv[0]) / exposure, lin(nv[1]) / exposure, lin(nv[2]) / exposure]
            } else {
                let (mut albedo, mut rough, mut metal, mut emit) = (if look.material == Material::Color { m.colors[ti as usize] } else { albedo0 }, rough, metal, [0.0f32; 3]);
                // (transmission, indice, vernis, bandes toon, reflets)
                let mut surf = (0.0f32, 1.45f32, 0.0f32, 0.0f32, 1.0f32);
                // (occlusion, velours, sous-surface, teinte sous-surface)
                let mut extra = (1.0f32, 0.0f32, 0.0f32, [0.0f32; 3]);
                let mut nrm = nrm;
                let tuv = m.uv.get(ti as usize).map(|u| [u[0][0] * w0 + u[1][0] * w1 + u[2][0] * w2, u[0][1] * w0 + u[1][1] * w1 + u[2][1] * w2]);
                if look.uv_checker {
                    if let Some(uv) = tuv {
                        albedo = checker(uv, if look.material == Material::Color { Some(m.colors[ti as usize]) } else { None });
                    }
                }
                let slot_prog = m.mat.get(ti as usize).and_then(|&k| look.slot_nodes.get(k as usize)).and_then(|p| p.as_ref());
                if let Some(prog) = slot_prog.or(look.nodes.as_ref()).filter(|_| !look.uv_checker) {
                    let pos = [pa[0] * w0 + pb[0] * w1 + pc[0] * w2, pa[1] * w0 + pb[1] * w1 + pc[1] * w2, pa[2] * w0 + pb[2] * w1 + pc[2] * w2];
                    let vc = m.colors[ti as usize];
                    let g = |x: f32| x.max(0.0).powf(1.0 / 2.2);
                    // Courbure : calculée une fois par maillage, seulement si un matériau la lit
                    let curv = if prog.uses_curvature {
                        let cv = m.curv.get_or_init(|| vertex_curvature(&m.pos, &m.nrm, &m.tris));
                        cv[t[0] as usize] * w0 + cv[t[1] as usize] * w1 + cv[t[2] as usize] * w2
                    } else {
                        0.0
                    };
                    let c = crate::nodes::Ctx { curv, uv: tuv.map(|u| [u[0], 1.0 - u[1]]).unwrap_or([pos[0] + 0.5, 0.5 - pos[1]]), pos, nrm, view: v, vcol: [g(vc[0]), g(vc[1]), g(vc[2]), 1.0], res: [w as f32, h as f32] };
                    let sf = NODE_REGS.with(|regs| prog.material(&c, &mut regs.borrow_mut()));
                    let (col, r, mt, em) = (sf.color, sf.rough, sf.metal, sf.emit);
                    surf = (sf.transmission, sf.ior, sf.coat, sf.toon, sf.spec);
                    let l = |x: f32| x.clamp(0.0, 1.0).powf(2.2);
                    extra = (sf.ao, sf.sheen, sf.sss, [l(sf.sss_color[0]), l(sf.sss_color[1]), l(sf.sss_color[2])]);
                    if prog.uses_bump && sf.bump_k != 0.0 {
                        // Relief : la pente de la hauteur, mesurée sur deux petits pas le long de la surface, penche la normale
                        let t1 = norm(cross(nrm, if nrm[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] }));
                        let t2 = cross(nrm, t1);
                        let eps = 0.003;
                        let height = |t: V3| {
                            let c2 = crate::nodes::Ctx { pos: [pos[0] + t[0] * eps, pos[1] + t[1] * eps, pos[2] + t[2] * eps], ..c };
                            NODE_REGS.with(|regs| prog.material(&c2, &mut regs.borrow_mut())).bump
                        };
                        let (d1, d2) = (height(t1) - sf.bump, height(t2) - sf.bump);
                        let k = sf.bump_k * 0.02 / eps;
                        nrm = norm([nrm[0] - (t1[0] * d1 + t2[0] * d2) * k, nrm[1] - (t1[1] * d1 + t2[1] * d2) * k, nrm[2] - (t1[2] * d1 + t2[2] * d2) * k]);
                    }
                    albedo = [l(col[0]), l(col[1]), l(col[2])];
                    rough = r;
                    metal = mt;
                    emit = [em[0].max(0.0).powf(2.2), em[1].max(0.0).powf(2.2), em[2].max(0.0).powf(2.2)];
                }
                let albedo = if metal > 0.5 && look.nodes.is_some() { albedo } else { albedo };
                let ndv = dot(nrm, v).clamp(0.0, 1.0);
                let ne = rot_y(nrm, -look.env_rot);
                let s = look.env_strength;
                let (ao, sheen, sss, sss_col) = extra;
                let irr = env.irradiance(ne);
                let irr = [irr[0] * ao, irr[1] * ao, irr[2] * ao];
                let r = sub([2.0 * dot(nrm, v) * nrm[0], 2.0 * dot(nrm, v) * nrm[1], 2.0 * dot(nrm, v) * nrm[2]], v);
                let refl = env.reflect(rot_y(r, -look.env_rot), rough);
                let f0 = if look.nodes.is_some() || slot_prog.is_some() {
                    [0.04 + (albedo[0] - 0.04) * metal, 0.04 + (albedo[1] - 0.04) * metal, 0.04 + (albedo[2] - 0.04) * metal]
                } else if metal > 0.5 {
                    [0.95, 0.93, 0.88]
                } else {
                    [0.04; 3]
                };
                let fres = (1.0 - ndv).powf(5.0);
                let shin = 2.0 / (rough * rough).max(0.002) - 2.0;
                let (transmission, ior, coat, toon, spec_k) = surf;
                // Toon : l'ambiance devient une teinte plate
                let irr = if toon >= 1.0 {
                    let amb = (irr[0] + irr[1] + irr[2]) / 3.0;
                    [amb.min(0.35); 3]
                } else {
                    irr
                };
                // Chaque lumière ajoute sa part : éclairage direct, reflet, et lumière qui déborde (sous-surface)
                let (mut l_diff, mut l_spec, mut l_wrap) = ([0.0f32; 3], [0.0f32; 3], [0.0f32; 3]);
                let here = [pa[0] * w0 + pb[0] * w1 + pc[0] * w2, pa[1] * w0 + pb[1] * w1 + pc[1] * w2, pa[2] * w0 + pb[2] * w1 + pc[2] * w2];
                for lamp in &lamps {
                    let (dir, reach, fall) = if lamp.point { lamp.sample(here, [0.5, 0.5]) } else { (lamp.at, f32::MAX, 1.0) };
                    if fall <= 0.0 || lamp.intensity <= 0.0 {
                        continue;
                    }
                    let raw = dot(nrm, dir);
                    let mut lit = fall * lamp.intensity;
                    // Ombre portée : quelque chose entre ce point et la lumière ?
                    if let Some(b) = bvh {
                        if raw > 0.0 || sss > 0.0 {
                            let side = if dot(face, dir) >= 0.0 { 1.0 } else { -1.0 };
                            let o = [here[0] + face[0] * side * 2e-3, here[1] + face[1] * side * 2e-3, here[2] + face[2] * side * 2e-3];
                            lit *= shadow(b, o, dir, if lamp.point { reach - 4e-3 } else { 1e6 }, lamp.point, x, y);
                        }
                    }
                    if lit <= 0.0 {
                        continue;
                    }
                    let ndl = raw.max(0.0);
                    let half = norm([dir[0] + v[0], dir[1] + v[1], dir[2] + v[2]]);
                    let spec_l = dot(nrm, half).max(0.0).powf(shin) * (shin + 8.0) / 25.0;
                    let wrap = ((raw + 0.6) / 1.6).max(0.0);
                    // Toon : la lumière tombe par paliers
                    let (ndl, spec_l) = if toon >= 1.0 {
                        let bands = toon.round();
                        (((ndl * bands).ceil() / bands).min(1.0), if spec_l > 0.6 { 1.0 } else { 0.0 })
                    } else {
                        (ndl, spec_l)
                    };
                    for ch in 0..3 {
                        let e = lamp.color[ch] * lit;
                        l_diff[ch] += e * ndl;
                        l_spec[ch] += e * spec_l;
                        l_wrap[ch] += e * (wrap - ndl).max(0.0);
                    }
                }
                // Ce qu'on voit à travers : l'environnement dans la direction réfractée
                let through = if transmission > 0.0 {
                    let eta = 1.0 / ior;
                    let cosi = dot(nrm, v).clamp(-1.0, 1.0);
                    let k = 1.0 - eta * eta * (1.0 - cosi * cosi);
                    let d = if k < 0.0 {
                        r
                    } else {
                        let a = eta * cosi - k.sqrt();
                        [-eta * v[0] + a * nrm[0], -eta * v[1] + a * nrm[1], -eta * v[2] + a * nrm[2]]
                    };
                    env.reflect(rot_y(d, -look.env_rot), rough)
                } else {
                    [0.0; 3]
                };
                let coat_refl = if coat > 0.0 { env.reflect(rot_y(r, -look.env_rot), 0.04) } else { [0.0; 3] };
                let mut out = [0.0f32; 3];
                for ch in 0..3 {
                    let f = f0[ch] + (1.0 - f0[ch]) * fres * (1.0 - rough).powf(2.0);
                    let diff = albedo[ch] * (irr[ch] * s + l_diff[ch]) * (1.0 - metal);
                    let diff = diff * (1.0 - transmission) + through[ch] * s * albedo[ch] * transmission * (1.0 - metal);
                    let diff = diff + sss_col[ch] * albedo[ch] * l_wrap[ch] * sss * (1.0 - metal);
                    let spec = (refl[ch] * s * (0.5 + 0.5 * ao) + l_spec[ch] * 0.25) * f * spec_k;
                    let diff = diff + sheen * fres.sqrt() * (irr[ch] * s + l_diff[ch]) * (0.5 + 0.5 * albedo[ch]) * 0.6;
                    let clear = coat * (coat_refl[ch] * s + l_spec[ch] * 0.25) * (0.04 + 0.96 * fres);
                    out[ch] = diff * (1.0 - f) + spec + clear + emit[ch];
                }
                out
            };
            if look.wire_only && !look.show_bg {
                chunk[o + 3] = 0;
            } else {
                chunk[o + 3] = 255;
            }
            let mut rgb = tone(col, exposure);
            // Fil de fer : distance du pixel à chaque arête réelle du triangle
            if wire > 0.0 {
                let e = m.edges[ti as usize];
                let (a, b, c) = (screen[t[0] as usize], screen[t[1] as usize], screen[t[2] as usize]);
                let area2 = ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs();
                let len = |p: [f32; 3], q: [f32; 3]| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt().max(1e-6);
                let mut dmin = f32::MAX;
                if e & 0b001 != 0 {
                    dmin = dmin.min(w2 * area2 / len(a, b));
                }
                if e & 0b010 != 0 {
                    dmin = dmin.min(w0 * area2 / len(b, c));
                }
                if e & 0b100 != 0 {
                    dmin = dmin.min(w1 * area2 / len(c, a));
                }
                let cover = (1.4 - dmin).clamp(0.0, 1.0) * wire;
                if cover > 0.0 {
                    let wc = tone([look.wire_color[0] / exposure * 1.0, look.wire_color[1] / exposure, look.wire_color[2] / exposure], exposure);
                    if look.wire_only && !look.show_bg {
                        rgb = wc;
                        chunk[o + 3] = (cover * 255.0) as u8;
                    } else {
                        for ch in 0..3 {
                            rgb[ch] = (rgb[ch] as f32 * (1.0 - cover) + wc[ch] as f32 * cover) as u8;
                        }
                    }
                } else if look.wire_only && !look.show_bg {
                    chunk[o + 3] = 0;
                }
                col = [0.0; 3];
            }
            let _ = col;
            chunk[o..o + 3].copy_from_slice(&rgb);
        }
        (tri, depth)
    }).collect();
    let (mut ids, mut depth) = (Vec::with_capacity(w * h), Vec::with_capacity(w * h));
    for (i, d) in bands {
        ids.extend(i);
        depth.extend(d);
    }
    Frame { w, h, rgba, ids, depth }
}

fn add3(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// Onglets de l'éditeur de propriétés de la scène, comme ceux de Blender.
#[derive(Clone, Copy, PartialEq, Default)]
enum PropTab {
    #[default]
    Object,
    Modifiers,
    Material,
    Data,
    Scene,
    World,
}

const PROP_TABS: [(PropTab, &str, &str); 6] = [
    (PropTab::Object, "▣", "objet"),
    (PropTab::Modifiers, "⚙", "modificateurs"),
    (PropTab::Material, "●", "matériau"),
    (PropTab::Data, "✦", "données (lumière, caméra, fichier)"),
    (PropTab::Scene, "▤", "scène"),
    (PropTab::World, "◐", "monde"),
];

/// Éléments de l'en-tête de la vue 3D, dans l'ordre. Ajouter un menu : une variante ici, une ligne dans HEADER,
/// un bras dans le match de `show`.
#[derive(Clone, Copy, PartialEq)]
enum Hdr {
    Mode,
    View,
    Select,
    Add,
    Object,
    /// Outils de poignée dans l'en-tête (remplacé par la barre d'outils à gauche de la vue ; gardé pour un en-tête sans barre)
    #[allow(dead_code)]
    Tools,
    Fill,
    Shading,
    Overlays,
    Light,
    Modifiers,
    Busy,
}

const HEADER: [Hdr; 11] = [Hdr::Mode, Hdr::View, Hdr::Select, Hdr::Add, Hdr::Object, Hdr::Fill, Hdr::Shading, Hdr::Overlays, Hdr::Light, Hdr::Modifiers, Hdr::Busy];

/// Superpositions de la vue (ombrage lisse, fil de fer) : menu de l'en-tête, ou onglet Scène des propriétés.
fn overlays_ui(look: &mut Look, ui: &mut egui::Ui) {
    let p = crate::theme::pal();


                    ui.checkbox(&mut look.smooth, crate::i18n::t("ombrage lisse"));
                    ui.label(egui::RichText::new(crate::i18n::t("subdivision, miroir, torsion… : bouton « modificateurs »")).color(p.muted).size(10.5));
                    ui.separator();
                    ui.label(egui::RichText::new(crate::i18n::t("fil de fer")).color(p.muted).size(11.0));
                    ui.add(egui::Slider::new(&mut look.wire, 0.0..=1.0).text(crate::i18n::t("opacité")));
                    ui.horizontal(|ui| {
                        for (c, l) in [([0.02, 0.02, 0.02], "noir"), ([1.0, 1.0, 1.0], "blanc"), (lin_accent(p.accent), "accent")] {
                            if ui.selectable_label(look.wire_color == c, l).clicked() {
                                look.wire_color = c;
                                if look.wire == 0.0 {
                                    look.wire = 0.6;
                                }
                            }
                        }
                    });
                    ui.checkbox(&mut look.wire_only, crate::i18n::t("fil de fer seul"));
                    
}

/// Éclairage de la vue (ombres, HDRI, lumière) : menu de l'en-tête, ou onglet Monde des propriétés.
fn lighting_ui(look: &mut Look, hdr_files: &[(String, std::path::PathBuf)], loading: bool, env_error: &Option<String>, pending: &mut Option<std::path::PathBuf>, ui: &mut egui::Ui) {
    let p = crate::theme::pal();

                    ui.checkbox(&mut look.shadows, crate::i18n::t("ombres portées")).on_hover_text(crate::i18n::t("Chaque lumière projette l'ombre des objets. Plus lent sur les gros modèles."));
                    if look.shadows {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(crate::i18n::t("douceur")).color(p.muted).size(11.0));
                            ui.add(egui::Slider::new(&mut look.shadow_soft, 0.0..=1.0).show_value(false)).on_hover_text(crate::i18n::t("0 : bord net. Plus haut : pénombre large, calculée avec plusieurs rayons."));
                        });
                    }
                    ui.separator();
                    ui.label(egui::RichText::new(crate::i18n::t("environnement (HDRI)")).color(p.muted).size(11.0));
                    for (i, name) in PRESETS.iter().enumerate() {
                        if ui.selectable_label(look.env.name == *name, *name).clicked() {
                            look.env = Env::preset(i);
                        }
                    }
                    for (name, path) in hdr_files.to_vec() {
                        if ui.selectable_label(look.env.name == std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), format!("{name}")).clicked() {
                            *pending = Some(path);
                        }
                    }
                    if ui.button(crate::i18n::t("charger un .hdr / .exr…")).clicked() {
                        *pending = rfd::FileDialog::new().add_filter("HDRI", &["hdr", "exr"]).pick_file();
                    }
                    if loading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(crate::i18n::t("chargement du HDRI…"));
                        });
                    }
                    if let Some(e) = env_error {
                        ui.label(egui::RichText::new(e).color(p.missing).size(11.0));
                    }
                    ui.add(egui::Slider::new(&mut look.env_rot, -std::f32::consts::PI..=std::f32::consts::PI).text(crate::i18n::t("rotation (H + glisser)")).show_value(false));
                    ui.add(egui::Slider::new(&mut look.env_strength, 0.0..=3.0).clamping(egui::SliderClamping::Never).text(crate::i18n::t("intensité")));
                    ui.add(egui::Slider::new(&mut look.exposure, -4.0..=4.0).clamping(egui::SliderClamping::Never).text(crate::i18n::t("exposition (EV)")));
                    ui.checkbox(&mut look.show_bg, crate::i18n::t("afficher le HDRI en fond"));
                    ui.add_enabled(look.show_bg, egui::Checkbox::new(&mut look.blur_bg, "fond flou"));
                    ui.separator();
                    ui.label(egui::RichText::new(crate::i18n::t("lumière directionnelle (L + glisser)")).color(p.muted).size(11.0));
                    ui.add(egui::Slider::new(&mut look.light.intensity, 0.0..=5.0).clamping(egui::SliderClamping::Never).text(crate::i18n::t("intensité")));
                    ui.add(egui::Slider::new(&mut look.light.azimuth, -std::f32::consts::PI..=std::f32::consts::PI).text(crate::i18n::t("direction")).show_value(false));
                    ui.add(egui::Slider::new(&mut look.light.elevation, -1.5..=1.5).text(crate::i18n::t("hauteur")).show_value(false));
                    ui.horizontal(|ui| {
                        ui.label(crate::i18n::t("couleur"));
                        let mut c = look.light.color;
                        if ui.color_edit_button_rgb(&mut c).changed() {
                            look.light.color = c;
                        }
                    });
                    if ui.button(crate::i18n::t("réinitialiser l'éclairage")).clicked() {
                        let mat = look.material;
                        let (wire, wc) = (look.wire, look.wire_color);
                        *look = Look { material: mat, wire, wire_color: wc, ..Default::default() };
                    }
                
}


/// Rend chaque image d'une scène (début → fin) et les envoie à ffmpeg, qui écrit la vidéo `out`.
/// La caméra de la scène sert de point de vue ; sans caméra, `view_cam`.
#[allow(clippy::too_many_arguments)]
fn encode_animation(ff: std::path::PathBuf, scene: crate::scene::Scene, path: &std::path::Path, frame_box: (V3, V3), look: &Look, view_cam: Camera, size: (usize, usize), out: &std::path::Path, tick: &dyn Fn()) -> Result<(), String> {
    use std::io::Write;
    let ((lo, hi), (w, h)) = (frame_box, size);
    let (start, end) = (scene.start, scene.end.max(scene.start));
    let fps = scene.fps.max(1.0);
    let mut child = crate::ffmpeg::command(ff)
        .args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{w}x{h}"), "-r", &format!("{fps}"), "-i", "-", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18"])
        .arg(out)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().ok_or("ffmpeg sans entrée")?;
    let longest = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let floor = -((lo[1] + hi[1]) / 2.0) / longest;
    let cam_obj = scene.objects.iter().position(|o| crate::scene::kind(o) == crate::scene::Kind::Camera);
    let mut sc = scene;
    for f in start..=end {
        sc.apply_frame(f as f32);
        let (model, _, ranges) = crate::scene::build_ranges(&sc, path);
        let mut m = model?;
        (m.min, m.max) = (lo, hi);
        let s = m.size();
        let l2 = s[0].max(s[1]).max(s[2]).max(1e-9);
        let mc = [(m.min[0] + m.max[0]) / 2.0, (m.min[1] + m.max[1]) / 2.0, (m.min[2] + m.max[2]) / 2.0];
        let origin = [-mc[0] / l2, -mc[1] / l2, -mc[2] / l2];
        let base = crate::modifiers::Mesh::from_model(&m, m.z_up);
        let mut prep = Prepared::from_mesh_fixed(crate::modifiers::apply(&[], base, origin), floor);
        // Un matériau par objet, comme à l'écran
        if ranges.last().map(|r| r.2) == Some(prep.tris.len()) {
            prep.mat = vec![0; prep.tris.len()];
            for (k, a, b) in &ranges {
                prep.mat[*a..*b].fill(*k as u16);
            }
        }
        // Lumières de la scène à cette image
        let mut lk = look.clone();
        lk.lamps = sc.lights().into_iter().map(|l| lamp_of(&l, c, longest)).collect();
        // Le point de vue : la caméra de la scène si elle existe
        let mut cam = view_cam;
        if let Some(k) = cam_obj {
            let wpos = sc.world(k, [0.0; 3]);
            let o = &sc.objects[k];
            cam.yaw = -o.rot[1].to_radians();
            cam.pitch = -o.rot[0].to_radians();
            cam.ortho = false;
            let r = cam.rotate([(wpos[0] - c[0]) / longest, (wpos[1] - c[1]) / longest, (wpos[2] - c[2]) / longest]);
            cam.pan = [-r[0], -r[1]];
            cam.dist = r[2].max(0.05);
        }
        let frame = render(&prep, &cam, &lk, w, h);
        if stdin.write_all(&frame.rgba).is_err() {
            let mut msg = String::new();
            if let Some(mut e) = child.stderr.take() {
                let _ = std::io::Read::read_to_string(&mut e, &mut msg);
            }
            return Err(format!("ffmpeg : {}", msg.lines().last().unwrap_or("erreur")));
        }
        tick();
    }
    drop(stdin);
    match child.wait() {
        Ok(s) if s.success() => Ok(()),
        _ => Err(crate::i18n::t("ffmpeg n'a pas pu écrire la vidéo")),
    }
}

/// Rendu d'une animation de scène en vidéo, en arrière-plan.
struct AnimJob {
    done: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    finished: std::sync::Arc<std::sync::atomic::AtomicBool>,
    error: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    total: usize,
    out: std::path::PathBuf,
}

/// Projection de l'espace normalisé de la vue vers l'écran.
fn projector(cam: Camera, rect: egui::Rect) -> impl Fn(V3) -> Option<Pos2> {
    let focal = rect.height().min(rect.width()) * 1.2;
    let rc = rect.center();
    move |v: V3| {
        let q = cam.view(v);
        if cam.ortho {
            Some(pos2(rc.x + q[0] * focal / cam.dist, rc.y - q[1] * focal / cam.dist))
        } else {
            (q[2] < -0.05).then(|| pos2(rc.x + q[0] / -q[2] * focal, rc.y - q[1] / -q[2] * focal))
        }
    }
}

/// Pour une poignée posée en `origin` : direction de la caméra, droite et haut de la vue, unités par pixel.
fn gizmo_basis(cam: &Camera, origin: V3, focal: f32) -> (V3, V3, V3, f32) {
    let (right, up) = (cam.unrotate([1.0, 0.0, 0.0]), cam.unrotate([0.0, 1.0, 0.0]));
    if cam.ortho {
        (cam.unrotate([0.0, 0.0, 1.0]), right, up, cam.dist / focal)
    } else {
        let eye = cam.unrotate([-cam.pan[0], -cam.pan[1], cam.dist]);
        let toward = norm(sub(eye, origin));
        let depth = (-cam.view(origin)[2]).max(0.05);
        (toward, right, up, depth / focal)
    }
}

/// Axes propres d'un objet dans la scène (rotation et parents compris), unitaires, et leur échelle.
fn scene_world_axes(scene: &crate::scene::Scene, k: usize) -> [(V3, f32); 3] {
    let o = scene.world(k, [0.0; 3]);
    [0, 1, 2].map(|i| {
        let mut e = [0.0f32; 3];
        e[i] = 1.0;
        let d = sub(scene.world(k, e), o);
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-9);
        ([d[0] / l, d[1] / l, d[2] / l], l)
    })
}

/// Mode édition d'un objet de scène.
struct MeshEdit {
    obj: usize,
    file: std::path::PathBuf,
    mesh: crate::meshedit::EMesh,
    sel: Vec<bool>,
    hist: Vec<(crate::meshedit::EMesh, Vec<bool>)>,
    redo: Vec<(crate::meshedit::EMesh, Vec<bool>)>,
    /// Transformation en cours : genre (0 déplacer, 1 tourner, 2 taille), axe imposé dans la scène, sommets de départ, souris de départ
    modal: Option<(u8, Option<V3>, Vec<V3>, Pos2)>,
    boxing: Option<Option<Pos2>>,
    skip_click: bool,
    /// Ce qu'on choisit au clic : 0 sommets, 1 arêtes, 2 faces (1, 2, 3 comme dans Blender)
    mode: u8,
    /// Dernier sommet cliqué : départ du plus court chemin (Ctrl+clic)
    active: Option<usize>,
    /// Sommets visibles depuis la caméra, avec la clé (caméra, vue, positions) du calcul
    vis: Option<(u64, Vec<bool>)>,
}

/// Sépare les sommets partagés par des faces qui font entre elles un angle marqué : les arêtes vives
/// restent nettes à l'éclairage, les surfaces courbes restent lisses. L'ordre des triangles ne change pas.
fn split_sharp(pos: &mut Vec<V3>, tris: &mut [[u32; 3]], limit_deg: f32) {
    let cos = limit_deg.to_radians().cos();
    let face: Vec<V3> = tris.iter().map(|t| norm(cross(sub(pos[t[1] as usize], pos[t[0] as usize]), sub(pos[t[2] as usize], pos[t[0] as usize])))).collect();
    // Pour chaque sommet : les groupes de faces déjà rencontrés (normale de référence, sommet qui les porte)
    let mut groups: Vec<Vec<(V3, u32)>> = vec![vec![]; pos.len()];
    for (ti, t) in tris.iter_mut().enumerate() {
        for k in 0..3 {
            let v = t[k] as usize;
            let n = face[ti];
            if n == [0.0; 3] {
                continue;
            }
            match groups[v].iter().find(|g| dot(g.0, n) > cos) {
                Some(g) => t[k] = g.1,
                None => {
                    let idx = if groups[v].is_empty() {
                        v as u32
                    } else {
                        pos.push(pos[v]);
                        pos.len() as u32 - 1
                    };
                    groups[v].push((n, idx));
                    t[k] = idx;
                }
            }
        }
    }
}

/// Cadre d'une scène : au moins 12 unités de côté autour de l'origine, pour qu'un cube neuf
/// arrive à une taille raisonnable sur la grille plutôt que de remplir toute la vue.
pub fn scene_bounds(lo: V3, hi: V3) -> (V3, V3) {
    ([lo[0].min(-6.0), lo[1].min(-6.0), lo[2].min(-6.0)], [hi[0].max(6.0), hi[1].max(6.0), hi[2].max(6.0)])
}

/// Image de référence posée dans l'espace : centre et deux côtés (largeur, hauteur), dans l'espace du rendu.
pub struct RefPlane {
    pub center: V3,
    pub ux: V3,
    pub uy: V3,
    pub img: std::sync::Arc<image::RgbaImage>,
    pub opacity: f32,
}

/// Dessine les images de référence dans le rendu, pixel par pixel, en respectant la profondeur :
/// un modèle placé devant une image la cache.
pub fn draw_refs(frame: &mut Frame, cam: &Camera, refs: &[RefPlane]) {
    use rayon::prelude::*;
    if refs.is_empty() {
        return;
    }
    let (w, h) = (frame.w, frame.h);
    let focal = w.min(h) as f32 * 1.2;
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let ortho_scale = focal / cam.dist;
    for r in refs {
        if r.opacity <= 0.0 || r.img.width() == 0 || r.img.height() == 0 {
            continue;
        }
        // Le plan dans l'espace caméra
        let (o, u, v) = (cam.view(r.center), cam.rotate(r.ux), cam.rotate(r.uy));
        let depth = &frame.depth;
        let ortho = cam.ortho;
        frame.rgba.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                // Rayon du pixel : origine et direction
                let (ro, rd) = if ortho { ([(x as f32 + 0.5 - cx) / ortho_scale, -(y as f32 + 0.5 - cy) / ortho_scale, 0.0], [0.0, 0.0, -1.0]) } else { ([0.0; 3], [(x as f32 + 0.5 - cx) / focal, -(y as f32 + 0.5 - cy) / focal, -1.0]) };
                let rhs = [ro[0] - o[0], ro[1] - o[1], ro[2] - o[2]];
                // Intersection du rayon avec le plan : (ro + t·rd − o) · n = 0
                let n = cross(u, v);
                let den = dot(n, rd);
                if den.abs() < 1e-9 {
                    continue;
                }
                let t = -dot(n, rhs) / den;
                // Point d'impact, puis coordonnées dans le plan par projection sur ses côtés
                let hit = [ro[0] + rd[0] * t, ro[1] + rd[1] * t, ro[2] + rd[2] * t];
                if t <= 0.0 || (!ortho && hit[2] > -0.02) {
                    continue;
                }
                let q = [hit[0] - o[0], hit[1] - o[1], hit[2] - o[2]];
                let (a, b) = (dot(q, u) / dot(u, u).max(1e-12), dot(q, v) / dot(v, v).max(1e-12));
                if !(-0.5..=0.5).contains(&a) || !(-0.5..=0.5).contains(&b) {
                    continue;
                }
                if hit[2] <= depth[y * w + x] {
                    continue;
                }
                let px = r.img.get_pixel((((a + 0.5) * r.img.width() as f32) as u32).min(r.img.width() - 1), (((0.5 - b) * r.img.height() as f32) as u32).min(r.img.height() - 1)).0;
                let alpha = px[3] as f32 / 255.0 * r.opacity.clamp(0.0, 1.0);
                let d = &mut row[x * 4..x * 4 + 4];
                if d[3] == 0 {
                    d[..3].copy_from_slice(&px[..3]);
                    d[3] = (alpha * 255.0) as u8;
                } else {
                    for c in 0..3 {
                        d[c] = (d[c] as f32 + (px[c] as f32 - d[c] as f32) * alpha) as u8;
                    }
                }
            }
        });
    }
}

thread_local! {
    static NODE_REGS: std::cell::RefCell<Vec<crate::nodes::V>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Sphère d'aperçu pour l'éditeur de matériaux.
pub fn preview_sphere() -> Prepared {
    let (n, m) = (64usize, 32usize);
    let mut pos = vec![];
    for j in 0..=m {
        let t = j as f32 / m as f32 * std::f32::consts::PI;
        for i in 0..=n {
            let p = i as f32 / n as f32 * std::f32::consts::TAU;
            pos.push([t.sin() * p.cos(), t.cos(), t.sin() * p.sin()]);
        }
    }
    let mut tris = vec![];
    for j in 0..m {
        for i in 0..n {
            let a = (j * (n + 1) + i) as u32;
            let b = a + n as u32 + 1;
            tris.push([a, a + 1, b + 1]);
            tris.push([a, b + 1, b]);
        }
    }
    let model = Model::new(pos, tris, None, false).expect("sphère");
    Prepared::new(&model, false, 0)
}

/// Cube aux arêtes adoucies pour l'aperçu des matériaux : ses arêtes et ses coins montrent
/// l'usure, la saleté des creux et la poussière, qu'une sphère n'a pas.
pub fn preview_box() -> Prepared {
    let pos: Vec<V3> = (0..8).map(|i| [if i & 1 == 0 { -0.6 } else { 0.6 }, if i & 2 == 0 { -0.6 } else { 0.6 }, if i & 4 == 0 { -0.6 } else { 0.6 }]).collect();
    let quads = [[0u32, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    let tris: Vec<[u32; 3]> = quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect();
    let n = tris.len();
    let (p, t, _, _) = split_flat(&pos, &tris, &vec![Color32::from_gray(205); n], &vec![7; n], 5, false);
    let model = Model::new(p, t, None, false).expect("cube");
    Prepared::new(&model, false, 0)
}

/// Rendu de la sphère avec un matériau en nodes (pour l'aperçu de l'éditeur).
pub fn render_material_preview(sphere: &Prepared, prog: std::sync::Arc<crate::nodes::Program>, size: usize) -> crate::nodes::Img {
    render_material_view(sphere, prog, size, 0.4, 0.25, 2.0, 0)
}

/// Aperçu d'un matériau sur un modèle, avec la caméra et l'éclairage choisis.
pub fn render_material_view(model: &Prepared, prog: std::sync::Arc<crate::nodes::Program>, size: usize, yaw: f32, pitch: f32, dist: f32, env: usize) -> crate::nodes::Img {
    let look = Look { nodes: Some(prog), show_bg: true, blur_bg: true, env: Env::preset(env), ..Default::default() };
    let f = render(model, &Camera { yaw, pitch, dist, ..Default::default() }, &look, size, size);
    let px = f.rgba.chunks_exact(4).map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, c[3] as f32 / 255.0]).collect();
    crate::nodes::Img { w: size, h: size, px }
}

/// Charge un modèle du disque, prêt pour l'aperçu des matériaux.
pub fn preview_model(path: &Path) -> Option<Prepared> {
    let m = load(path).ok()?;
    Some(Prepared::new(&m, m.z_up, 0))
}

/// Image du modèle pour les miniatures des boards et des refs.
pub fn thumbnail(model: &Model, w: u32, h: u32, _bg: [u8; 4]) -> image::RgbaImage {
    let prep = Prepared::new(model, model.z_up, 0);
    let look = Look { material: Material::Gray, ..Default::default() };
    let f = render(&prep, &Camera { dist: 2.0, ..Default::default() }, &look, w as usize, h as usize);
    image::RgbaImage::from_raw(w, h, f.rgba).unwrap_or_else(|| image::RgbaImage::new(w, h))
}

// ---------- visionneuse ----------

pub enum State {
    Loading(Receiver<Result<Model, String>>),
    Ready(std::sync::Arc<Model>),
    Failed(String),
}

pub struct Viewer {
    pub state: State,
    path: std::path::PathBuf,
    cam: Camera,
    look: Look,
    /// HDRI du coffre proposés dans le menu (nom, chemin).
    pub hdr_files: Vec<(String, std::path::PathBuf)>,
    /// Matériaux en nodes du coffre (nom, chemin), et celui qui est appliqué.
    pub mat_files: Vec<(String, std::path::PathBuf)>,
    /// Matériaux en nodes des IDs (mode édition) : fichier, date vue, programme
    slot_progs: Vec<Option<(std::path::PathBuf, Option<std::time::SystemTime>, std::sync::Arc<crate::nodes::Program>)>>,
    slot_rev: u64,
    /// Matériau à ouvrir dans l'éditeur de nodes (à côté du modèle)
    pub open_request: Option<std::path::PathBuf>,
    /// Ctrl+S : écrire les changements (modificateurs, retopo) dans le fichier (None) ou dans un autre (Ctrl+Maj+S)
    pub save_request: Option<Option<std::path::PathBuf>>,
    /// Le fichier a été réécrit : à recharger
    pub reload_request: bool,
    pub root: String,
    mat_active: Option<(std::path::PathBuf, Option<std::time::SystemTime>)>,
    mat_error: Option<String>,
    env_loading: Option<Receiver<Result<Env, String>>>,
    env_error: Option<String>,
    /// Pile de modificateurs (enregistrée dans .app/modifiers/ du coffre)
    stack: Vec<crate::modifiers::Modifier>,
    stack_loaded: bool,
    /// Annuler / rétablir dans la pile de modificateurs
    stack_saved: Option<Vec<crate::modifiers::Modifier>>,
    stack_hist: Vec<Vec<crate::modifiers::Modifier>>,
    stack_redo: Vec<Vec<crate::modifiers::Modifier>>,
    /// Maillage de base après « appliquer » (remplace celui du fichier, gardé dans .app/modifiers/)
    applied: Option<std::sync::Arc<crate::modifiers::Mesh>>,
    applied_ver: u64,
    /// Scène `.arya3` ouverte : ses objets, celui qui est sélectionné, les erreurs de chargement
    scene: Option<crate::scene::Scene>,
    /// HDRI choisi depuis l'onglet Monde, chargé par la vue
    pending_hdr: Option<std::path::PathBuf>,
    scene_sel: Option<usize>,
    /// Menu du clic droit sur un objet de la scène, et la commande choisie dedans
    obj_menu: Option<Pos2>,
    obj_action: Option<&'static str>,
    scene_errors: Vec<Option<String>>,
    scene_rx: Option<Receiver<(Result<Model, String>, Vec<Option<String>>, Vec<(usize, usize, usize)>)>>,
    /// Triangles de chaque objet dans le modèle réuni (objet, début, fin)
    scene_ranges: Vec<(usize, usize, usize)>,
    /// Axe de la poignée de déplacement en cours de glissement
    /// Poignée tenue : laquelle, position précédente, point de départ, valeur cumulée
    gizmo_hold: Option<(crate::gizmo::Handle, Pos2, Pos2, [f32; 3])>,
    gizmo_hot: Option<crate::gizmo::Handle>,
    /// Ce que fait la poignée : 0 déplacer, 1 tourner, 2 redimensionner
    gizmo_mode: u8,
    /// Images de référence de la scène, chargées une fois (texture et rapport largeur / hauteur)
    ref_tex: std::collections::HashMap<std::path::PathBuf, Option<std::sync::Arc<image::RgbaImage>>>,
    /// Animation de la scène : image courante, lecture en cours, clé automatique
    anim_frame: f32,
    anim_playing: bool,
    anim_auto: bool,
    anim_last: Option<std::time::Instant>,
    /// Transformation au clavier en cours (G, R, S) : genre, axe imposé, pose de départ, position de la souris au départ
    modal: Option<(u8, Option<usize>, ([f32; 3], [f32; 3], [f32; 3]), Pos2)>,
    /// Menu « ajouter » ouvert sous le curseur (Maj+A)
    add_popup: Option<Pos2>,
    /// Le clic qui vient de valider une transformation : à ignorer pour la sélection, jusqu'au relâchement
    modal_click: bool,
    /// Objets choisis en plus de l'objet actif (`scene_sel`) : Maj + clic, A, sélection au rectangle
    scene_multi: Vec<usize>,
    /// Pose de l'objet actif à l'image précédente : les autres objets choisis suivent ses changements
    multi_follow: Option<(usize, ([f32; 3], [f32; 3], [f32; 3]))>,
    /// Sélection au rectangle (B) : armée, puis coin de départ
    box_sel: Option<Option<Pos2>>,
    /// Mode édition (Tab) : le maillage de l'objet actif, modifiable sommet par sommet
    mesh: Option<MeshEdit>,
    /// Demande d'entrer ou de sortir du mode édition (bouton du panneau)
    mesh_toggle: bool,
    /// Mode édition : voir et choisir aussi les sommets cachés derrière le maillage (rayons X, Alt+Z)
    xray: bool,
    want_anim: bool,
    anim_job: Option<AnimJob>,
    /// La vue de la frame précédente (l'en-tête et les outils flottent par-dessus)
    vp_rect: Option<egui::Rect>,
    hdr_h: f32,
    prop_tab: PropTab,
    /// Éditeur de courbes à la place de la timeline
    anim_graph: bool,
    graph: crate::graph::Graph,
    /// Action d'un menu de l'en-tête, rejouée comme sa touche à l'image suivante (une seule logique pour les deux)
    hdr_key: Option<(Key, egui::Modifiers)>,
    /// Objet à ajouter depuis le menu « ajouter »
    hdr_add: Option<&'static str>,
    /// Triangles, sommets, taille : affichés en bas de la vue
    stats_text: String,
    /// Un champ avait le clavier à l'image précédente (Tab y sert alors à passer au champ suivant)
    kb_busy: bool,
    /// Annuler dans la scène : dernier état enregistré, états précédents, états annulés
    scene_saved: Option<crate::scene::Scene>,
    /// Scène telle qu'à la dernière reconstruction complète (déplacement rapide)
    scene_built: Option<crate::scene::Scene>,
    /// Déplacement rapide en cours : reconstruction complète et écriture du fichier au relâché
    scene_moving: bool,
    scene_hist: Vec<crate::scene::Scene>,
    scene_redo: Vec<crate::scene::Scene>,
    scene_ver: u64,
    scene_dirty: bool,
    /// Cadre fixe de la scène : déplacer un objet ne doit pas recentrer ni redimensionner tout le reste
    scene_frame: Option<(V3, V3)>,
    /// Modèles à ajouter à la scène (glissés sur la fenêtre)
    pub scene_add: Vec<std::path::PathBuf>,
    /// Modèles du coffre proposés dans le panneau de scène
    pub model_files: Vec<(String, std::path::PathBuf)>,
    /// Fenêtre d'export : format choisi et chemin de sortie
    export_dlg: Option<(&'static str, String)>,
    export_dir: Option<std::path::PathBuf>,
    show_mods: bool,
    /// Mode édition : IDs de matériau, coutures, UV
    pub edit: Option<crate::uvedit::Edit>,
    edit_seen: u64,
    /// Triangle visible par pixel de la dernière image (pour cacher les coutures masquées)
    ids: Option<(usize, usize, Vec<u32>)>,
    /// Version de `ids` (change à chaque nouvelle image)
    ids_rev: u64,
    /// Contour orange de la sélection, par-dessus l'image
    outline: Option<(u64, egui::TextureHandle)>,
    /// Calcul en cours : (clé, résultat)
    job: Option<(u64, Receiver<Prepared>)>,
    turntable: bool,
    pub z_up: Option<bool>,
    prepared: Option<(bool, u64, std::sync::Arc<Prepared>)>,
    tex: Option<egui::TextureHandle>,
    last: Option<(Camera, [usize; 2], bool, u64, String)>,
    /// Moteur de la vue : 0 aperçu (rapide), 1 tracé de chemins (réaliste, s'affine à l'arrêt)
    pub engine: u8,
    trace: Option<((Camera, [usize; 2], bool, u64, String), pathtrace::Accum)>,
    pub trace_set: pathtrace::Settings,
    pub trace_max: u32,
    /// Rendu final (F12) : réglages avant de lancer (largeur, hauteur, échantillons), puis le rendu en cours
    render_dialog: Option<(usize, usize, u32)>,
    final_job: Option<pathtrace::Job>,
    final_tex: Option<egui::TextureHandle>,
    oidn_install: Option<std::sync::Arc<std::sync::Mutex<crate::ffmpeg::InstallState>>>,
    /// La scène sur la carte graphique, et le nombre de passes par image
    gpu_scene: Option<gputrace::Scene>,
    gpu_batch: u32,
    capture_msg: Option<String>,
    /// Ouvert depuis une scène pour les UV, matériaux et retopo d'un de ses objets
    pub embedded: bool,
    /// Visionneuse de l'objet de la scène en cours d'édition (UV, matériaux, retopo) et son indice
    sub: Option<(Box<Viewer>, usize)>,
    /// Entrer dans « UV et matériaux » dès que le modèle est chargé
    start_edit: bool,
    /// Demande d'ouvrir les UV, matériaux et retopo de l'objet choisi (scène)
    uv_request: bool,
    /// Scène à ouvrir dans un onglet (« ouvrir dans une scène » depuis la visionneuse)
    pub scene_request: Option<std::path::PathBuf>,
}

impl Viewer {
    pub fn open(path: std::path::PathBuf, ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let p = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(load(&p));
            ctx.request_repaint();
        });
        Self {
            state: State::Loading(rx),
            path,
            cam: Camera::default(),
            look: Look::default(),
            hdr_files: vec![],
            mat_files: vec![],
            slot_progs: vec![],
            slot_rev: 0,
            open_request: None,
            save_request: None,
            reload_request: false,
            root: String::new(),
            mat_active: None,
            mat_error: None,
            env_loading: None,
            env_error: None,
            stack: vec![],
            stack_loaded: false,
            stack_saved: None,
            stack_hist: vec![],
            stack_redo: vec![],
            applied: None,
            applied_ver: 0,
            scene: None,
            pending_hdr: None,
            ids_rev: 0,
            outline: None,
            scene_sel: None,
            obj_menu: None,
            obj_action: None,
            scene_errors: vec![],
            scene_rx: None,
            scene_ranges: vec![],
            gizmo_hold: None,
            gizmo_hot: None,
            gizmo_mode: 0,
            ref_tex: Default::default(),
            anim_frame: 0.0,
            anim_playing: false,
            anim_auto: false,
            anim_last: None,
            modal: None,
            add_popup: None,
            modal_click: false,
            scene_multi: vec![],
            multi_follow: None,
            box_sel: None,
            mesh: None,
            mesh_toggle: false,
            want_anim: false,
            anim_job: None,
            vp_rect: None,
            hdr_h: 30.0,
            prop_tab: PropTab::Object,
            anim_graph: false,
            graph: Default::default(),
            hdr_key: None,
            hdr_add: None,
            stats_text: String::new(),
            kb_busy: false,
            scene_saved: None,
            scene_built: None,
            scene_moving: false,
            scene_hist: vec![],
            scene_redo: vec![],
            scene_ver: 0,
            scene_dirty: false,
            scene_frame: None,
            scene_add: vec![],
            model_files: vec![],
            export_dlg: None,
            export_dir: None,
            show_mods: false,
            edit: None,
            edit_seen: 0,
            ids: None,
            job: None,
            turntable: false,
            z_up: None,
            prepared: None,
            tex: None,
            last: None,
            engine: 0,
            trace: None,
            trace_set: pathtrace::Settings::default(),
            trace_max: 256,
            render_dialog: None,
            final_job: None,
            final_tex: None,
            oidn_install: None,
            gpu_scene: None,
            gpu_batch: 4,
            capture_msg: None,
            xray: false,
            embedded: false,
            sub: None,
            start_edit: false,
            uv_request: false,
            scene_request: None,
        }
    }

    fn load_env(&mut self, path: std::path::PathBuf, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Env::load_file(&path));
            ctx.request_repaint();
        });
        self.env_loading = Some(rx);
        self.env_error = None;
    }

    /// Ce que la vue affiche vraiment : matériaux par objet, lumières de la scène, images de référence.
    /// Les captures passent par ici aussi, pour donner la même image que la vue.
    fn render_setup(&mut self) -> (Look, Vec<RefPlane>, u64) {
        let mut look = self.look.clone();
        if self.edit.is_none() && self.scene.is_some() {
            look.slot_nodes = self.slot_progs.iter().map(|p| p.as_ref().map(|x| x.2.clone())).collect();
            look.slot_rev = self.slot_rev;
        }
        if let Some(e) = &self.edit {
            look.material = Material::Color;
            look.nodes = None;
            look.slot_nodes = self.slot_progs.iter().map(|p| p.as_ref().map(|x| x.2.clone())).collect();
            look.slot_rev = self.slot_rev;
            look.uv_checker = e.checker && e.data.has_uv();
            if look.wire == 0.0 {
                look.wire = 0.35;
            }
        }
        // Scène : sa première lumière remplace celle de la vue, et ses images de référence entrent dans le rendu
        let mut refs: Vec<RefPlane> = vec![];
        let mut refs_key = 0u64;
        if let (Some(scene), Some((lo, hi)), true) = (self.scene.as_ref(), self.scene_frame, self.edit.is_none()) {
            use std::hash::{Hash, Hasher};
            let longest = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
            let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
            look.lamps = scene
                .lights()
                .into_iter()
                .map(|l| lamp_of(&l, c, longest))
                .collect();
            let nrm = |v: V3| [(v[0] - c[0]) / longest, (v[1] - c[1]) / longest, (v[2] - c[2]) / longest];
            let mut hsh = std::collections::hash_map::DefaultHasher::new();
            for (k, o) in scene.objects.iter().enumerate() {
                if crate::scene::kind(o) != crate::scene::Kind::Image || !scene.shown(k) || o.opacity <= 0.0 {
                    continue;
                }
                let file = crate::scene::resolve(&self.path, &o.file);
                let entry = self.ref_tex.entry(file.clone()).or_insert_with(|| {
                    image::open(&file).ok().map(|im| std::sync::Arc::new(if im.width().max(im.height()) > 2048 { im.thumbnail(2048, 2048) } else { im }.to_rgba8()))
                });
                let Some(img) = entry else { continue };
                let aspect = img.width() as f32 / img.height().max(1) as f32;
                let (p0, px, py) = (nrm(scene.world(k, [0.0; 3])), nrm(scene.world(k, [aspect, 0.0, 0.0])), nrm(scene.world(k, [0.0, 1.0, 0.0])));
                let plane = RefPlane { center: p0, ux: [px[0] - p0[0], px[1] - p0[1], px[2] - p0[2]], uy: [py[0] - p0[0], py[1] - p0[1], py[2] - p0[2]], img: img.clone(), opacity: o.opacity };
                file.hash(&mut hsh);
                for v in [plane.center, plane.ux, plane.uy] {
                    v.map(|x| x.to_bits()).hash(&mut hsh);
                }
                plane.opacity.to_bits().hash(&mut hsh);
                refs.push(plane);
            }
            refs_key = hsh.finish();
        }
        (look, refs, refs_key)
    }

    fn capture(&mut self) {
        let (mut look, refs, _) = self.render_setup();
        // Image fixe : on peut se permettre des ombres plus fines qu'à l'écran
        look.shadow_samples = look.shadow_samples.max(16);
        let Some((_, _, prep)) = &self.prepared else { return };
        let mut f = render(prep, &self.cam, &look, 2400, 2400);
        draw_refs(&mut f, &self.cam, &refs);
        let img = image::RgbaImage::from_raw(2400, 2400, f.rgba).unwrap_or_default();
        let img = image::DynamicImage::ImageRgba8(img);
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "modele".into());
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let out = self.path.with_file_name(format!("{stem}-rendu-{ts}.png"));
        self.capture_msg = Some(match img.save(&out) {
            Ok(()) => format!("capture : {}", out.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
            Err(e) => format!("capture impossible : {e}"),
        });
    }

    /// Rend chaque image de la scène (début → fin) et les assemble en MP4 avec ffmpeg, en arrière-plan.
    /// La caméra de la scène sert de point de vue ; sans caméra, la vue actuelle.
    fn render_animation(&mut self, ctx: &egui::Context) {
        if self.anim_job.is_some() {
            return;
        }
        let Some(ff) = crate::ffmpeg::find("ffmpeg") else {
            self.capture_msg = Some(crate::i18n::t("ffmpeg n'est pas installé : il sert à assembler la vidéo."));
            return;
        };
        let (Some(scene), Some((lo, hi))) = (self.scene.clone(), self.scene_frame) else { return };
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "scene".into());
        let mut dlg = rfd::FileDialog::new().set_title(crate::i18n::t("enregistrer l'animation")).set_file_name(format!("{stem}.mp4")).add_filter("MP4", &["mp4"]);
        if let Some(dir) = self.path.parent().filter(|d| !d.to_string_lossy().contains(".app/untitled")) {
            dlg = dlg.set_directory(dir);
        }
        let Some(mut out) = dlg.save_file() else { return };
        if out.extension().is_none() {
            out.set_extension("mp4");
        }
        let (look, _, _) = self.render_setup();
        let view_cam = self.cam;
        let path = self.path.clone();
        let (w, h) = (1280usize, 720usize);
        let job = AnimJob { done: Default::default(), finished: Default::default(), error: Default::default(), total: (scene.end.max(scene.start) - scene.start + 1) as usize, out: out.clone() };
        let (done, finished, error) = (job.done.clone(), job.finished.clone(), job.error.clone());
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = encode_animation(ff, scene, &path, (lo, hi), &look, view_cam, (w, h), &out, &|| {
                done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                ctx.request_repaint();
            });
            match r {
                Ok(()) => finished.store(true, std::sync::atomic::Ordering::Relaxed),
                Err(e) => {
                    if let Ok(mut g) = error.lock() {
                        *g = Some(e);
                    }
                }
            }
            ctx.request_repaint();
        });
        self.anim_job = Some(job);
    }

    fn mods_file(&self) -> std::path::PathBuf {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.path.hash(&mut h);
        std::path::Path::new(&self.root).join(".app/modifiers").join(format!("{:016x}.json", h.finish()))
    }

    /// Tour complet du modèle en GIF animé (48 images, 520 px).
    fn turntable_gif(&mut self) {
        let (mut look, refs, _) = self.render_setup();
        look.show_bg = true;
        let Some((_, _, prep)) = &self.prepared else { return };
        let size = 520;
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "modele".into());
        let out = self.path.with_file_name(format!("{stem}-turntable.gif"));
        let r = (|| -> Result<(), String> {
            let file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            let mut enc = image::codecs::gif::GifEncoder::new_with_speed(file, 10);
            enc.set_repeat(image::codecs::gif::Repeat::Infinite).map_err(|e| e.to_string())?;
            for k in 0..48 {
                let cam = Camera { yaw: self.cam.yaw + k as f32 / 48.0 * std::f32::consts::TAU, ..self.cam };
                let mut f = render(prep, &cam, &look, size, size);
                draw_refs(&mut f, &cam, &refs);
                let img = image::RgbaImage::from_raw(size as u32, size as u32, f.rgba).unwrap_or_default();
                enc.encode_frame(image::Frame::from_parts(img, 0, 0, image::Delay::from_numer_denom_ms(1000, 24))).map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        self.capture_msg = Some(match r {
            Ok(()) => format!("turntable : {}", out.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
            Err(e) => format!("GIF impossible : {e}"),
        });
    }

    /// Contour de la sélection, comme Blender : orange clair pour l'objet actif, plus foncé pour
    /// les autres. Calculé sur l'image des triangles visibles, seulement quand elle ou la sélection change.
    fn draw_outline(&mut self, ui: &egui::Ui, painter: &egui::Painter, rect: egui::Rect) {
        let active = self.scene_sel;
        if self.scene.is_none() || self.edit.is_some() || self.mesh.is_some() || (active.is_none() && self.scene_multi.is_empty()) {
            return;
        }
        let Some((w, h, ids)) = &self.ids else { return };
        let (w, h) = (*w, *h);
        use std::hash::{Hash, Hasher};
        let mut hs = std::collections::hash_map::DefaultHasher::new();
        (self.ids_rev, active, &self.scene_multi).hash(&mut hs);
        let key = hs.finish();
        if self.outline.as_ref().map(|o| o.0) != Some(key) {
            let total = self.scene_ranges.last().map(|r| r.2).unwrap_or(0);
            let mut tri_sel = vec![0u8; total];
            for &(k, a, b) in &self.scene_ranges {
                let v = if Some(k) == active { 2 } else if self.scene_multi.contains(&k) { 1 } else { 0 };
                if v > 0 {
                    tri_sel[a..b.min(total)].fill(v);
                }
            }
            let sel: Vec<u8> = ids.iter().map(|&t| tri_sel.get(t as usize).copied().unwrap_or(0)).collect();
            // Épaisseur : 2 points d'écran, quelle que soit la résolution de l'image
            let r = ((w as f32 / rect.width().max(1.0)) * 2.0).round().clamp(1.0, 4.0) as i32;
            // Dilatation séparable (max sur les lignes puis les colonnes) : rapide même en mouvement
            let r = r as usize;
            let mut row = vec![0u8; w * h];
            for y in 0..h {
                let line = &sel[y * w..(y + 1) * w];
                for x in 0..w {
                    let (lo, hi) = (x.saturating_sub(r), (x + r).min(w - 1));
                    row[y * w + x] = line[lo..=hi].iter().copied().max().unwrap_or(0);
                }
            }
            let mut px = vec![0u8; w * h * 4];
            let (hot, warm) = ([255u8, 170, 64], [235u8, 110, 20]);
            for x in 0..w {
                for y in 0..h {
                    let (lo, hi) = (y.saturating_sub(r), (y + r).min(h - 1));
                    let best = (lo..=hi).map(|yy| row[yy * w + x]).max().unwrap_or(0);
                    if best > sel[y * w + x] {
                        let c = if best == 2 { hot } else { warm };
                        let i = (y * w + x) * 4;
                        px[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
                    }
                }
            }
            let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &px);
            match &mut self.outline {
                Some((k, t)) => {
                    t.set(img, egui::TextureOptions::LINEAR);
                    *k = key;
                }
                None => self.outline = Some((key, ui.ctx().load_texture("model3d-outline", img, egui::TextureOptions::LINEAR))),
            }
        }
        if let Some((_, t)) = &self.outline {
            painter.image(t.id(), rect, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
    }

    pub fn is_mesh_edit(&self) -> bool {
        self.mesh.is_some()
    }

    /// Le maillage propre à la scène de l'objet `k` : une copie en OBJ à côté de la scène,
    /// créée au besoin (le modèle d'origine n'est jamais modifié)
    fn own_mesh_file(&mut self, k: usize) -> Result<std::path::PathBuf, String> {
        let scene = self.scene.as_mut().ok_or_else(|| crate::i18n::t("pas de scène"))?;
        let o = scene.objects.get(k).filter(|o| crate::scene::kind(o) == crate::scene::Kind::Model).ok_or_else(|| crate::i18n::t("seuls les objets à maillage se modifient"))?;
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "scène".into());
        let dir = self.path.parent().unwrap_or(std::path::Path::new("")).to_path_buf();
        let prefix = format!("{stem} - ");
        if o.file.starts_with(&prefix) && o.file.to_lowercase().ends_with(".obj") {
            return Ok(crate::scene::resolve(&self.path, &o.file));
        }
        let model = crate::scene::source_model(&self.path, o)?;
        let mesh = crate::meshedit::EMesh::from_model(&model);
        let mut name = format!("{prefix}{}.obj", o.name);
        let mut n = 2;
        while dir.join(&name).exists() {
            name = format!("{prefix}{} {n}.obj", o.name);
            n += 1;
        }
        let f = dir.join(&name);
        mesh.write_obj(&f)?;
        scene.objects[k].file = name;
        let _ = scene.write(&self.path);
        self.scene_dirty = true;
        Ok(f)
    }

    /// UV, matériaux et retopo de l'objet choisi : sa copie s'ouvre dans la scène, avec les outils
    fn open_uv_edit(&mut self, ctx: &egui::Context) {
        let Some(k) = self.scene_sel else {
            self.capture_msg = Some(crate::i18n::t("choisis d'abord un objet à modifier"));
            return;
        };
        self.mesh = None;
        match self.own_mesh_file(k) {
            Ok(f) => {
                let mut v = Viewer::open(f, ctx);
                v.embedded = true;
                v.start_edit = true;
                v.root = self.root.clone();
                v.hdr_files = self.hdr_files.clone();
                v.mat_files = self.mat_files.clone();
                v.look = self.look.clone();
                self.sub = Some((Box::new(v), k));
                self.capture_msg = None;
            }
            Err(e) => self.capture_msg = Some(e),
        }
    }

    /// « Ouvrir dans une scène » : une scène neuve à côté du modèle, avec lui, une lumière et une caméra
    fn open_in_scene(&mut self) {
        let dir = self.path.parent().unwrap_or(std::path::Path::new("")).to_path_buf();
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "modèle".into());
        let ext = crate::formats::ext("scene");
        let mut out = dir.join(format!("{stem}.{ext}"));
        let mut n = 2;
        while out.exists() {
            out = dir.join(format!("{stem} {n}.{ext}"));
            n += 1;
        }
        let mut sc = crate::scene::Scene::default();
        sc.add(&out, &self.path);
        let l = sc.add_role("soleil", "light");
        (sc.objects[l].pos, sc.objects[l].rot) = ([4.0, 6.0, 3.0], [35.0, 0.0, -35.0]);
        let c = sc.add_role("caméra", "camera");
        (sc.objects[c].pos, sc.objects[c].rot) = ([7.0, 5.0, 7.0], [-28.0, 45.0, 0.0]);
        match sc.write(&out) {
            Ok(()) => self.scene_request = Some(out),
            Err(e) => self.capture_msg = Some(e),
        }
    }

    /// Entre en mode édition sur l'objet actif. Son maillage devient un fichier OBJ à côté de la scène
    /// (le modèle d'origine n'est jamais modifié), que le mode édition réécrit à chaque changement.
    fn enter_mesh_edit(&mut self) {
        let (Some(k), Some(scene)) = (self.scene_sel, self.scene.as_mut()) else {
            self.capture_msg = Some(crate::i18n::t("choisis d'abord un objet à modifier"));
            return;
        };
        let Some(o) = scene.objects.get(k).filter(|o| crate::scene::kind(o) == crate::scene::Kind::Model) else {
            self.capture_msg = Some(crate::i18n::t("seuls les objets à maillage se modifient en mode édition"));
            return;
        };
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "scène".into());
        let dir = self.path.parent().unwrap_or(std::path::Path::new("")).to_path_buf();
        let prefix = format!("{stem} - ");
        let (file, mesh) = if o.file.starts_with(&prefix) && o.file.to_lowercase().ends_with(".obj") {
            // Déjà un maillage propre à cette scène
            let f = crate::scene::resolve(&self.path, &o.file);
            match crate::meshedit::EMesh::read_obj(&f) {
                Ok(m) => (f, m),
                Err(e) => {
                    self.capture_msg = Some(e);
                    return;
                }
            }
        } else {
            let model = match crate::scene::source_model(&self.path, o) {
                Ok(m) => m,
                Err(e) => {
                    self.capture_msg = Some(e);
                    return;
                }
            };
            let mesh = crate::meshedit::EMesh::from_model(&model);
            let mut name = format!("{prefix}{}.obj", o.name);
            let mut n = 2;
            while dir.join(&name).exists() {
                name = format!("{prefix}{} {n}.obj", o.name);
                n += 1;
            }
            let f = dir.join(&name);
            if let Err(e) = mesh.write_obj(&f) {
                self.capture_msg = Some(e);
                return;
            }
            scene.objects[k].file = name;
            let _ = scene.write(&self.path);
            self.scene_dirty = true;
            (f, mesh)
        };
        let n = mesh.pos.len();
        self.mesh = Some(MeshEdit { obj: k, file, mesh, sel: vec![false; n], hist: vec![], redo: vec![], modal: None, boxing: None, skip_click: false, mode: 0, active: None, vis: None });
        self.scene_multi.clear();
        self.capture_msg = None;
    }

    /// Mode édition : affichage des sommets et des arêtes par-dessus la vue, sélection, et outils au clavier.
    fn mesh_edit_ui(&mut self, ui: &mut egui::Ui, rect: egui::Rect, resp: &egui::Response) {
        if !crate::scene::is_scene(&self.path) || self.edit.is_some() {
            return;
        }
        let p = theme::pal();
        let typing = ui.ctx().egui_wants_keyboard_input();
        // Tab : egui s'en sert pour passer d'un champ à l'autre ; sur la vue, sans champ actif, il change de mode
        let tab = resp.hovered() && !self.kb_busy && self.modal.is_none() && ui.input(|i| i.key_pressed(Key::Tab));
        if tab {
            ui.ctx().memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
        self.kb_busy = ui.ctx().memory(|m| m.focused().is_some());
        let typing = typing && !tab;
        if std::mem::take(&mut self.mesh_toggle) || tab {
            if self.mesh.take().is_none() {
                self.enter_mesh_edit();
            }
        }
        let (Some((lo, hi)), Some(scene)) = (self.scene_frame, self.scene.as_ref()) else { return };
        let Some(obj) = self.mesh.as_ref().map(|m| m.obj) else { return };
        if obj >= scene.objects.len() || self.scene_sel != Some(obj) {
            self.mesh = None;
            return;
        }
        // De l'objet vers la scène : origine et axes ; et le chemin inverse
        let origin = scene.world(obj, [0.0; 3]);
        let cols = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(|e| sub(scene.world(obj, e), origin));
        let Some(inv) = crate::meshedit::inverse(cols) else { return };
        let to_world = |v: V3| {
            let w = crate::meshedit::apply(&cols, v);
            [w[0] + origin[0], w[1] + origin[1], w[2] + origin[2]]
        };
        let to_local = |w: V3| crate::meshedit::apply(&inv, sub(w, origin));
        let longest = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
        let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let focal = rect.height().min(rect.width()) * 1.2;
        let (cam, rc) = (self.cam, rect.center());
        let view = |w: V3| cam.view([(w[0] - c[0]) / longest, (w[1] - c[1]) / longest, (w[2] - c[2]) / longest]);
        let project = |w: V3| -> Option<Pos2> {
            let q = view(w);
            if cam.ortho {
                Some(pos2(rc.x + q[0] * focal / cam.dist, rc.y - q[1] * focal / cam.dist))
            } else {
                (q[2] < -0.05).then(|| pos2(rc.x + q[0] / -q[2] * focal, rc.y - q[1] / -q[2] * focal))
            }
        };
        if !typing && resp.hovered() && ui.input(|i| i.modifiers.alt && i.key_pressed(Key::Z)) {
            self.xray = !self.xray;
        }
        let xray = self.xray;
        let me = self.mesh.as_mut().unwrap();
        me.sel.resize(me.mesh.pos.len(), false);
        // Comme Blender : sans rayons X, les sommets cachés derrière le maillage ne se voient
        // pas et ne se choisissent pas. Recalculé seulement quand la vue ou le maillage change.
        let vis: Vec<bool> = if xray {
            vec![true; me.mesh.pos.len()]
        } else {
            let key = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                for x in [cam.yaw, cam.pitch, cam.dist, cam.pan[0], cam.pan[1], rect.width(), rect.height()] {
                    x.to_bits().hash(&mut h);
                }
                cam.ortho.hash(&mut h);
                me.mesh.pos.len().hash(&mut h);
                me.mesh.polys.len().hash(&mut h);
                let mut acc = [0.0f64; 3];
                for (i, v) in me.mesh.pos.iter().enumerate() {
                    let w = (i % 7 + 1) as f64;
                    for k in 0..3 {
                        acc[k] += v[k] as f64 * w;
                    }
                }
                acc.map(|a| a.to_bits()).hash(&mut h);
                h.finish()
            };
            match &me.vis {
                Some((k, v)) if *k == key && v.len() == me.mesh.pos.len() => v.clone(),
                _ => {
                    let vp: Vec<V3> = me.mesh.pos.iter().map(|&v| view(to_world(v))).collect();
                    let mut tris: Vec<[u32; 3]> = vec![];
                    for poly in &me.mesh.polys {
                        for k in 1..poly.len().saturating_sub(1) {
                            tris.push([poly[0], poly[k], poly[k + 1]]);
                        }
                    }
                    let v = if tris.is_empty() {
                        vec![true; vp.len()]
                    } else {
                        let bvh = crate::bvh::Bvh::new(&vp, &tris);
                        let ext = vp.iter().fold(0.0f32, |m, q| m.max(q[0].abs()).max(q[1].abs()));
                        let eps = 2e-3 * ext.max(1e-3);
                        vp.iter()
                            .map(|&q| {
                                // Vers la caméra : l'origine en perspective, tout droit vers +z en orthographique
                                let (d, reach) = if cam.ortho {
                                    ([0.0, 0.0, 1.0], 1e6)
                                } else {
                                    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt().max(1e-9);
                                    ([-q[0] / l, -q[1] / l, -q[2] / l], l)
                                };
                                !bvh.occluded(q, d, eps, reach - eps)
                            })
                            .collect()
                    };
                    me.vis = Some((key, v.clone()));
                    v
                }
            }
        };
        // Position à l'écran d'un sommet visible (None : caché ou derrière la caméra)
        let project_v = |me: &MeshEdit, i: usize| -> Option<Pos2> { if vis.get(i).copied().unwrap_or(true) { project(to_world(me.mesh.pos[i])) } else { None } };
        let mut dirty = false;
        let hovered = resp.hovered();
        let (pressed, secondary, cur) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.secondary_pressed(), i.pointer.hover_pos().or(i.pointer.latest_pos())));
        let key = |k: Key| !typing && ui.input(|i| i.key_pressed(k) && !i.modifiers.any());
        let snapshot = |me: &mut MeshEdit| {
            me.hist.push((me.mesh.clone(), me.sel.clone()));
            me.redo.clear();
            if me.hist.len() > 100 {
                me.hist.remove(0);
            }
        };
        // ---- Transformation en cours
        if let Some((kind, axis, orig, from)) = me.modal.take() {
            let mut axis = axis;
            for (kk, k) in [(Key::X, 0usize), (Key::Y, 2), (Key::Z, 1)] {
                if ui.input(|i| i.key_pressed(kk)) {
                    let mut e = [0.0f32; 3];
                    e[k] = 1.0;
                    axis = if axis == Some(e) { None } else { Some(e) };
                }
            }
            let chosen: Vec<usize> = (0..orig.len()).filter(|&i| me.sel.get(i).copied().unwrap_or(false)).collect();
            let worlds: Vec<V3> = chosen.iter().map(|&i| to_world(orig[i])).collect();
            let n = worlds.len().max(1) as f32;
            let pivot = worlds.iter().fold([0.0f32; 3], |a, w| [a[0] + w[0] / n, a[1] + w[1] / n, a[2] + w[2] / n]);
            let q = view(pivot);
            let center = project(pivot).unwrap_or(rc);
            let cancel = secondary || ui.input(|i| i.key_pressed(Key::Escape));
            let confirm = pressed || ui.input(|i| i.key_pressed(Key::Enter));
            if cancel {
                me.mesh.pos = orig;
                me.hist.pop();
                dirty = true;
                me.skip_click = true;
            } else if let Some(cur) = cur {
                let d = cur - from;
                let toward = cam.unrotate([0.0, 0.0, 1.0]);
                for (&i, w) in chosen.iter().zip(&worlds) {
                    let nw = match kind {
                        0 => {
                            let per_px = if cam.ortho { cam.dist / focal } else { (-q[2]).max(0.05) / focal } * longest;
                            let (right, up) = (cam.unrotate([1.0, 0.0, 0.0]), cam.unrotate([0.0, 1.0, 0.0]));
                            let mv = [0, 1, 2].map(|k| (right[k] * d.x - up[k] * d.y) * per_px);
                            let mv = match axis {
                                Some(a) => {
                                    // Le long de l'axe : on suit sa direction à l'écran
                                    let tip = project([pivot[0] + a[0], pivot[1] + a[1], pivot[2] + a[2]]).unwrap_or(center);
                                    let s = tip - center;
                                    let t = if s.length_sq() > 1e-3 { d.dot(s) / s.length_sq() } else { dot(mv, a) };
                                    [a[0] * t, a[1] * t, a[2] * t]
                                }
                                None => mv,
                            };
                            [w[0] + mv[0], w[1] + mv[1], w[2] + mv[2]]
                        }
                        1 => {
                            let a = axis.unwrap_or_else(|| {
                                let k = (0..3).max_by(|x, y| toward[*x].abs().total_cmp(&toward[*y].abs())).unwrap_or(1);
                                let mut e = [0.0f32; 3];
                                e[k] = 1.0;
                                e
                            });
                            let mut turn = (cur - center).angle() - (from - center).angle();
                            if turn > std::f32::consts::PI {
                                turn -= std::f32::consts::TAU;
                            } else if turn < -std::f32::consts::PI {
                                turn += std::f32::consts::TAU;
                            }
                            let th = -turn * if dot(a, toward) >= 0.0 { 1.0 } else { -1.0 };
                            // Rotation autour de l'axe (formule de Rodrigues)
                            let v = sub(*w, pivot);
                            let (cs, sn) = (th.cos(), th.sin());
                            let (kxv, kdv) = (cross(a, v), dot(a, v));
                            [0, 1, 2].map(|k| pivot[k] + v[k] * cs + kxv[k] * sn + a[k] * kdv * (1.0 - cs))
                        }
                        _ => {
                            let f = ((cur - center).length() / (from - center).length().max(8.0)).max(0.001);
                            let v = sub(*w, pivot);
                            match axis {
                                Some(a) => {
                                    let along = dot(v, a);
                                    [0, 1, 2].map(|k| pivot[k] + v[k] + a[k] * along * (f - 1.0))
                                }
                                None => [pivot[0] + v[0] * f, pivot[1] + v[1] * f, pivot[2] + v[2] * f],
                            }
                        }
                    };
                    me.mesh.pos[i] = to_local(nw);
                }
                dirty = true;
                if confirm {
                    me.skip_click = true;
                } else {
                    me.modal = Some((kind, axis, orig, from));
                }
            } else {
                me.modal = Some((kind, axis, orig, from));
            }
            let what = [crate::i18n::t("déplacer"), crate::i18n::t("tourner"), crate::i18n::t("taille")][kind.min(2) as usize].clone();
            // Axe imposé : sa ligne traverse la vue, à sa couleur
            if let Some(e) = axis {
                let k = (0..3).max_by(|a, b| e[*a].abs().total_cmp(&e[*b].abs())).unwrap_or(0);
                let painter = ui.painter_at(rect);
                let mut last: Option<Pos2> = None;
                for i in -60..=60 {
                    let t = i as f32 / 60.0;
                    let d = t * t.abs() * longest * 20.0;
                    let q = project([pivot[0] + e[0] * d, pivot[1] + e[1] * d, pivot[2] + e[2] * d]);
                    if let (Some(a), Some(b)) = (last, q) {
                        painter.line_segment([a, b], Stroke::new(1.5, crate::gizmo::AXIS[k]));
                    }
                    last = q;
                }
            }
            ui.painter_at(rect).text(rect.left_bottom() + egui::vec2(12.0, -10.0), egui::Align2::LEFT_BOTTOM, format!("{what}   ·   X Y Z : {}   ·   {}   ·   {}", crate::i18n::t("imposer un axe"), crate::i18n::t("clic : valider"), crate::i18n::t("Échap : annuler")), egui::FontId::monospace(11.5), p.accent);
            ui.ctx().request_repaint();
        } else if hovered || me.boxing.is_some() {
            let any = me.sel.iter().any(|s| *s);
            // ---- Outils
            let start = if key(Key::G) { Some(0u8) } else if key(Key::R) { Some(1) } else if key(Key::S) { Some(2) } else { None };
            if let (Some(kind), true, Some(at)) = (start, any, cur) {
                snapshot(me);
                me.modal = Some((kind, None, me.mesh.pos.clone(), at));
            }
            if key(Key::E) && any {
                snapshot(me);
                match me.mesh.extrude(&me.sel) {
                    Some((sel, dir)) => {
                        me.sel = sel;
                        // Les nouvelles faces partent le long de leur normale
                        let w = norm(crate::meshedit::apply(&cols, dir));
                        if let Some(at) = cur {
                            me.modal = Some((0, Some(w), me.mesh.pos.clone(), at));
                        }
                        dirty = true;
                    }
                    None => {
                        me.hist.pop();
                        self.capture_msg = Some(crate::i18n::t("extruder demande au moins une face dont tous les sommets sont choisis"));
                    }
                }
            }
            if (key(Key::X) || key(Key::Delete)) && any {
                snapshot(me);
                let sel = std::mem::take(&mut me.sel);
                me.mesh.delete(&sel);
                me.sel = vec![false; me.mesh.pos.len()];
                dirty = true;
            }
            if key(Key::F) {
                // Les sommets choisis, rangés en tournant autour de leur centre vu d'ici : la face regarde vers nous
                let pts: Vec<(u32, Pos2)> = (0..me.mesh.pos.len()).filter(|&i| me.sel[i]).filter_map(|i| project(to_world(me.mesh.pos[i])).map(|s| (i as u32, s))).collect();
                if pts.len() >= 3 {
                    let n = pts.len() as f32;
                    let mid = pts.iter().fold(egui::Vec2::ZERO, |a, x| a + x.1.to_vec2() / n);
                    let mut order = pts.clone();
                    order.sort_by(|a, b| (b.1.to_vec2() - mid).angle().total_cmp(&(a.1.to_vec2() - mid).angle()));
                    snapshot(me);
                    if me.mesh.fill(&order.iter().map(|x| x.0).collect::<Vec<u32>>()) {
                        dirty = true;
                    } else {
                        me.hist.pop();
                    }
                }
            }
            if key(Key::A) {
                me.sel.iter_mut().for_each(|s| *s = true);
            }
            if !typing && ui.input(|i| i.modifiers.alt && i.key_pressed(Key::A)) {
                me.sel.iter_mut().for_each(|s| *s = false);
            }
            // Modes de sélection : 1 sommets, 2 arêtes, 3 faces
            for (k, kk) in [Key::Num1, Key::Num2, Key::Num3].into_iter().enumerate() {
                if key(kk) {
                    me.mode = k as u8;
                }
            }
            // I : face incrustée dans chaque face choisie
            if key(Key::I) && any {
                snapshot(me);
                match me.mesh.inset(&me.sel, 0.25) {
                    Some(sel) => {
                        me.sel = sel;
                        dirty = true;
                    }
                    None => {
                        me.hist.pop();
                        self.capture_msg = Some(crate::i18n::t("incruster demande au moins une face dont tous les sommets sont choisis"));
                    }
                }
            }
            // M : fusionner au centre
            if key(Key::M) && any {
                snapshot(me);
                match me.mesh.merge(&me.sel) {
                    Some(sel) => {
                        me.sel = sel;
                        dirty = true;
                    }
                    None => {
                        me.hist.pop();
                    }
                }
            }
            // L : la pièce sous le pointeur ; Ctrl+L : tout ce qui touche la sélection
            if !typing && ui.input(|i| i.modifiers.command && i.key_pressed(Key::L)) && any {
                me.sel = me.mesh.linked(&me.sel);
            } else if key(Key::L) {
                if let Some(at) = cur {
                    let near = (0..me.mesh.pos.len()).filter_map(|i| project_v(me, i).map(|s| (i, (s - at).length()))).min_by(|a, b| a.1.total_cmp(&b.1));
                    if let Some((i, _)) = near {
                        let mut one = vec![false; me.mesh.pos.len()];
                        one[i] = true;
                        let add = me.mesh.linked(&one);
                        for (s, a) in me.sel.iter_mut().zip(add) {
                            *s |= a;
                        }
                    }
                }
            }
            if key(Key::B) {
                me.boxing = Some(None);
            }
            // Ctrl+Z annuler, Ctrl+Maj+Z ou Ctrl+Y rétablir
            let (z, y) = ui.input(|i| (i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::Z), i.modifiers.command && (i.modifiers.shift && i.key_pressed(Key::Z) || i.key_pressed(Key::Y))));
            if !typing && me.modal.is_none() && (z || y) {
                let (from, to) = if y { (&mut me.redo, &mut me.hist) } else { (&mut me.hist, &mut me.redo) };
                if let Some((m, s)) = from.pop() {
                    to.push((std::mem::replace(&mut me.mesh, m), std::mem::replace(&mut me.sel, s)));
                    dirty = true;
                }
            }
            // ---- Sélection : rectangle, ou clic sur le sommet le plus proche
            let (shift, ctrl_down, alt_down) = ui.input(|i| (i.modifiers.shift, i.modifiers.command, i.modifiers.alt));
            // Ctrl+I inverser, Ctrl+plus étendre, Ctrl+moins réduire
            if !typing && ctrl_down {
                if ui.input(|i| i.key_pressed(Key::I)) {
                    me.sel.iter_mut().for_each(|s| *s = !*s);
                }
                if ui.input(|i| i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals)) {
                    me.sel = me.mesh.grow(&me.sel);
                }
                if ui.input(|i| i.key_pressed(Key::Minus)) {
                    me.sel = me.mesh.shrink(&me.sel);
                }
            }
            if let Some(start) = me.boxing {
                let down = ui.input(|i| i.pointer.primary_down());
                match (start, cur) {
                    (None, Some(at)) if pressed && rect.contains(at) => me.boxing = Some(Some(at)),
                    (Some(s), Some(at)) => {
                        let area = egui::Rect::from_two_pos(s, at);
                        ui.painter_at(rect).rect_filled(area, 0.0, p.accent.gamma_multiply(0.12));
                        ui.painter_at(rect).rect_stroke(area, 0.0, Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
                        if !down {
                            for i in 0..me.mesh.pos.len() {
                                let inside = project_v(me, i).is_some_and(|s| area.contains(s));
                                // Maj : ajoute ; Ctrl : retire ; sinon : remplace
                                me.sel[i] = if ctrl_down { me.sel[i] && !inside } else if shift { me.sel[i] || inside } else { inside };
                            }
                            me.boxing = None;
                            me.skip_click = true;
                        }
                    }
                    _ => {}
                }
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    me.boxing = None;
                }
                ui.ctx().request_repaint();
            } else if resp.clicked() && !me.skip_click {
                if let Some(at) = resp.interact_pointer_pos() {
                    let scr: Vec<Option<Pos2>> = (0..me.mesh.pos.len()).map(|i| project_v(me, i)).collect();
                    let nearest_edge = |scr: &[Option<Pos2>]| me.mesh.edges().into_iter().filter_map(|(a, b)| {
                        let (sa, sb) = (scr[a as usize]?, scr[b as usize]?);
                        let ab = sb - sa;
                        let t = ((at - sa).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                        let d = (sa + ab * t - at).length();
                        (d < 10.0).then_some(((a, b), d))
                    }).min_by(|x, y| x.1.total_cmp(&y.1)).map(|x| x.0);
                    let nearest_vert = |scr: &[Option<Pos2>]| (0..scr.len()).filter_map(|i| scr[i].map(|s| (i, (s - at).length()))).filter(|x| x.1 < 11.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|x| x.0);
                    // Alt+clic : la boucle d'arêtes ; Ctrl+clic : le plus court chemin depuis le dernier sommet cliqué
                    let special: Option<Vec<usize>> = if alt_down {
                        nearest_edge(&scr).map(|(a, b)| me.mesh.edge_loop(a, b).into_iter().map(|v| v as usize).collect())
                    } else if ctrl_down {
                        match (me.active, nearest_vert(&scr)) {
                            (Some(a), Some(b)) if a < me.mesh.pos.len() => {
                                me.active = Some(b);
                                Some(me.mesh.shortest_path(a as u32, b as u32).into_iter().map(|v| v as usize).collect())
                            }
                            _ => None,
                        }
                    } else {
                        None
                    };
                    if let Some(vs) = special {
                        if !shift && alt_down {
                            me.sel.iter_mut().for_each(|s| *s = false);
                        }
                        for i in vs {
                            if i < me.sel.len() {
                                me.sel[i] = true;
                            }
                        }
                        me.skip_click = true;
                    }
                    if let Some(v) = nearest_vert(&scr).filter(|_| !ctrl_down) {
                        me.active = Some(v);
                    }
                    // Ce qu'on vise : un sommet, une arête (ses deux bouts) ou une face (tous ses sommets)
                    let hit: Option<Vec<usize>> = if me.skip_click { None } else { match me.mode {
                        1 => me.mesh.edges().into_iter().filter_map(|(a, b)| {
                            let (sa, sb) = (scr[a as usize]?, scr[b as usize]?);
                            let ab = sb - sa;
                            let t = ((at - sa).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                            let d = (sa + ab * t - at).length();
                            (d < 8.0).then_some((vec![a as usize, b as usize], d))
                        }).min_by(|x, y| x.1.total_cmp(&y.1)).map(|x| x.0),
                        2 => me.mesh.polys.iter().filter_map(|poly| {
                            let pts: Option<Vec<Pos2>> = poly.iter().map(|&v| scr[v as usize]).collect();
                            let pts = pts?;
                            // Le point est-il dans le polygone (à l'écran) ? Le plus proche de l'œil gagne
                            let mut inside = false;
                            for k in 0..pts.len() {
                                let (a, b) = (pts[k], pts[(k + 1) % pts.len()]);
                                if (a.y > at.y) != (b.y > at.y) && at.x < (b.x - a.x) * (at.y - a.y) / (b.y - a.y) + a.x {
                                    inside = !inside;
                                }
                            }
                            let depth = poly.iter().map(|&v| -view(to_world(me.mesh.pos[v as usize]))[2]).sum::<f32>() / poly.len() as f32;
                            inside.then(|| (poly.iter().map(|&v| v as usize).collect::<Vec<usize>>(), depth))
                        }).min_by(|x, y| x.1.total_cmp(&y.1)).map(|x| x.0),
                        _ => (0..me.mesh.pos.len()).filter_map(|i| scr[i].map(|s| (i, (s - at).length()))).filter(|x| x.1 < 11.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|x| vec![x.0]),
                    } };
                    match hit {
                        Some(vs) if shift => {
                            let all = vs.iter().all(|&i| me.sel[i]);
                            for i in vs {
                                me.sel[i] = !all;
                            }
                        }
                        Some(vs) => {
                            me.sel.iter_mut().for_each(|s| *s = false);
                            for i in vs {
                                me.sel[i] = true;
                            }
                        }
                        None if !shift && !me.skip_click => me.sel.iter_mut().for_each(|s| *s = false),
                        None => {}
                    }
                }
            }
        }
        if !ui.input(|i| i.pointer.any_down()) && !pressed {
            me.skip_click = false;
        }
        // ---- Affichage : arêtes puis sommets (au-delà d'un certain nombre, seulement les sommets choisis)
        let over = ui.painter_at(rect);
        let screen: Vec<Option<Pos2>> = (0..me.mesh.pos.len()).map(|i| project_v(me, i)).collect();
        let light = me.mesh.pos.len() <= 20_000;
        if light && me.mode == 2 {
            for f in me.mesh.faces_in(&me.sel) {
                let pts: Option<Vec<Pos2>> = me.mesh.polys[f].iter().map(|&v| screen[v as usize]).collect();
                if let Some(pts) = pts {
                    // En éventail : juste pour les polygones convexes (quadrilatères, triangles)
                    for k in 1..pts.len().saturating_sub(1) {
                        over.add(egui::Shape::convex_polygon(vec![pts[0], pts[k], pts[k + 1]], p.accent.gamma_multiply(0.22), Stroke::NONE));
                    }
                }
            }
        }
        if light {
            for (a, b) in me.mesh.edges() {
                if let (Some(sa), Some(sb)) = (screen[a as usize], screen[b as usize]) {
                    let on = me.sel[a as usize] && me.sel[b as usize];
                    over.line_segment([sa, sb], Stroke::new(if on { 1.8 } else { 1.0 }, if on { p.accent } else { Color32::from_black_alpha(150) }));
                }
            }
        }
        for (i, s) in screen.iter().enumerate() {
            if let Some(s) = s {
                // En mode face, les sommets se font discrets
                if me.mode == 2 && !me.sel[i] {
                    continue;
                }
                if me.sel[i] {
                    over.circle_filled(*s, 3.5, p.accent);
                } else if light {
                    over.circle_filled(*s, 2.2, Color32::from_black_alpha(200));
                }
            }
        }
        if dirty {
            let _ = me.mesh.write_obj(&me.file);
            self.scene_dirty = true;
            ui.ctx().request_repaint();
        }
    }

    /// F12 : réglages du rendu final, puis sa fenêtre (progression, image, enregistrement).
    fn final_render_ui(&mut self, ctx: &egui::Context, look: &Look) {
        let p = theme::pal();
        if ctx.input(|i| i.key_pressed(egui::Key::F12)) && self.render_dialog.is_none() && self.final_job.is_none() {
            self.render_dialog = Some((1920, 1080, 256));
        }
        if let Some((mut w, mut h, mut spp)) = self.render_dialog {
            let mut go = false;
            let mut close = false;
            egui::Window::new(crate::i18n::t("rendu final")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut w).range(16..=32768).suffix(" px"));
                    ui.label("×");
                    ui.add(egui::DragValue::new(&mut h).range(16..=32768).suffix(" px"));
                });
                ui.horizontal_wrapped(|ui| {
                    for (a, b, l) in [(1280, 720, "720p"), (1920, 1080, "1080p"), (3840, 2160, "4K"), (1080, 1350, "portrait"), (2048, 2048, "carré")] {
                        if ui.selectable_label(w == a && h == b, l).clicked() {
                            (w, h) = (a, b);
                        }
                    }
                });
                ui.add(egui::Slider::new(&mut spp, 8..=4096).clamping(egui::SliderClamping::Never).logarithmic(true).text(crate::i18n::t("échantillons")));
                ui.checkbox(&mut self.trace_set.denoise, crate::i18n::t("débruitage"));
                ui.label(egui::RichText::new(crate::i18n::t("Le point de vue est celui de la vue 3D ; rebonds et sol comme dans le menu « rendu ».")).size(10.5).color(p.muted));
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(egui::RichText::new(crate::i18n::t("rendre")).color(p.on_accent)).fill(p.accent)).clicked() {
                        go = true;
                    }
                    if ui.button(crate::i18n::t("annuler")).clicked() {
                        close = true;
                    }
                });
            });
            self.render_dialog = if go || close { None } else { Some((w, h, spp)) };
            if go {
                if let Some((_, _, prep)) = &self.prepared {
                    self.final_job = Some(pathtrace::Job::start(prep.clone(), self.cam, look.clone(), self.trace_set, w, h, spp, ctx.clone()));
                    self.final_tex = None;
                }
            }
        }
        let Some(job) = &mut self.final_job else { return };
        let done = job.poll();
        if let Some(img) = job.image.take() {
            let ci = egui::ColorImage::from_rgba_unmultiplied([job.w, job.h], &img);
            match &mut self.final_tex {
                Some(t) => t.set(ci, egui::TextureOptions::LINEAR),
                None => self.final_tex = Some(ctx.load_texture("final-render", ci, egui::TextureOptions::LINEAR)),
            }
            job.image = Some(img);
        }
        let (mut close, mut save) = (false, false);
        let mut open = true;
        egui::Window::new(crate::i18n::t("rendu")).open(&mut open).default_size([720.0, 480.0]).resizable(true).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::ProgressBar::new(job.spp as f32 / job.target.max(1) as f32).desired_width(260.0).fill(p.accent).text(format!("{} / {} · {:.0} s", job.spp, job.target, job.elapsed)));
                if !done && ui.button(crate::i18n::t("arrêter")).clicked() {
                    job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                    job.target = job.spp;
                }
                if ui.add_enabled(job.image.is_some(), egui::Button::new(crate::i18n::t("enregistrer…"))).clicked() {
                    save = true;
                }
                if ui.button(crate::i18n::t("fermer")).clicked() {
                    close = true;
                }
            });
            if let Some(t) = &self.final_tex {
                let avail = ui.available_size();
                let k = (avail.x / job.w as f32).min(avail.y / job.h as f32).min(1.0).max(0.05);
                let size = egui::vec2(job.w as f32 * k, job.h as f32 * k);
                let (r, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                // Damier sous les zones transparentes
                ui.painter().rect_filled(r, 0.0, Color32::from_gray(if theme::is_dark() { 40 } else { 225 }));
                ui.painter().image(t.id(), r, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            } else {
                ui.spinner();
            }
        });
        if save {
            if let Some(img) = &job.image {
                if let Some(path) = rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name("rendu.png").save_file() {
                    let path = if path.extension().is_none() { path.with_extension("png") } else { path };
                    if let Some(buf) = image::RgbaImage::from_raw(job.w as u32, job.h as u32, img.clone()) {
                        let _ = buf.save(&path);
                    }
                }
            }
        }
        if close || !open {
            job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            self.final_job = None;
            self.final_tex = None;
        }
        if !done {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    /// Le modèle a-t-il des changements qui ne sont pas dans son fichier ?
    pub fn has_pending(&self) -> bool {
        self.stack.iter().any(|m| m.on) || self.applied.is_some() || self.edit.as_ref().is_some_and(|e| e.retopo.is_some() && e.retopo_show)
    }

    /// Écrit le modèle tel qu'on le voit (retopo affichée, ou modificateurs appliqués) dans `target`
    /// (None : son propre fichier, avec une copie de sécurité de l'original).
    fn write_model(&mut self, model: &Model, z_up: bool, target: Option<std::path::PathBuf>) {
        let same = target.is_none();
        let out = target.unwrap_or_else(|| self.path.clone());
        let ext = out.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !crate::modifiers::EXPORT_FORMATS.contains(&ext.as_str()) {
            self.capture_msg = Some(format!("{} .{ext} : {}", crate::i18n::t("écriture impossible en"), crate::i18n::t("Ctrl+Maj+S pour enregistrer en OBJ, FBX, GLB, STL ou PLY")));
            return;
        }
        if same && !self.has_pending() {
            self.capture_msg = Some(crate::i18n::t("rien à enregistrer : le fichier est à jour"));
            return;
        }
        // Le maillage à écrire : la retopo affichée, sinon la pile de modificateurs appliquée
        let mesh = match self.edit.as_ref().and_then(|e| e.retopo.as_ref().filter(|_| e.retopo_show)) {
            Some(r) => {
                let mut m = crate::modifiers::Mesh { pos: r.pos.clone(), tris: vec![], colors: vec![], edges: vec![] };
                for f in &r.faces {
                    for k in 1..f.len().saturating_sub(1) {
                        m.tris.push([f[0], f[k], f[k + 1]]);
                        // Arêtes réelles : le tour du polygone, pas les diagonales du découpage
                        let mut e = 0u8;
                        if k == 1 {
                            e |= 1;
                        }
                        e |= 2;
                        if k + 2 == f.len() {
                            e |= 4;
                        }
                        m.edges.push(e);
                        m.colors.push(Color32::from_gray(200));
                    }
                }
                m
            }
            None => {
                let s = model.size();
                let longest = s[0].max(s[1]).max(s[2]).max(1e-9);
                let c = [(model.min[0] + model.max[0]) / 2.0, (model.min[1] + model.max[1]) / 2.0, (model.min[2] + model.max[2]) / 2.0];
                let o = [-c[0] / longest, -c[1] / longest, -c[2] / longest];
                let origin = if z_up { [o[0], o[2], -o[1]] } else { o };
                let base = self.applied.as_ref().map(|a| (**a).clone()).unwrap_or_else(|| crate::modifiers::Mesh::from_model(model, z_up));
                crate::modifiers::apply(&self.stack, base, origin)
            }
        };
        // Copie de sécurité de l'original avant de l'écraser
        if same && !self.root.is_empty() {
            let dir = std::path::Path::new(&self.root).join(".app/backup");
            let _ = std::fs::create_dir_all(&dir);
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let name = self.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let _ = std::fs::copy(&self.path, dir.join(format!("{stamp}-{name}")));
        }
        match crate::modifiers::export(&mesh, model, z_up, &out) {
            Ok(()) => {
                let name = out.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if same {
                    // Tout est dans le fichier : la pile, le maillage appliqué et la retopo n'ont plus lieu d'être
                    self.stack.clear();
                    self.applied = None;
                    if !self.root.is_empty() {
                        let f = self.mods_file();
                        let _ = std::fs::remove_file(f.with_extension("mesh"));
                        let _ = crate::vault::atomic_write(&f, "[]");
                    }
                    self.edit = None;
                    self.reload_request = true;
                    self.capture_msg = Some(format!("{} {name} {}", crate::i18n::t("enregistré dans"), crate::i18n::t("(copie de l'original dans .app/backup)")));
                } else {
                    self.open_request = Some(out.clone());
                    self.capture_msg = Some(format!("{} {name}", crate::i18n::t("enregistré sous")));
                }
            }
            Err(e) => self.capture_msg = Some(format!("{} : {e}", crate::i18n::t("écriture impossible"))),
        }
    }

    /// Un modèle ouvert seul est en lecture : on le regarde. On le modifie dans une scène.
    pub fn can_edit(&self) -> bool {
        self.embedded
    }

    pub fn is_scene(&self) -> bool {
        crate::scene::is_scene(&self.path)
    }

    /// Panneau de scène : la liste des objets, et la position, rotation et échelle de celui qui est choisi.
    fn scene_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        if self.scene.is_none() {
            self.scene = Some(crate::scene::Scene::read(&self.path).unwrap_or_default());
            // Les couleurs des objets se voient d'emblée, et un premier passage relève les triangles de chacun
            self.look.material = Material::Color;
            self.scene_dirty = true;
        }
        // Résultat d'une reconstruction lancée après un changement
        if let Some(rx) = &self.scene_rx {
            if let Ok((m, errs, ranges)) = rx.try_recv() {
                self.scene_errors = errs;
                self.scene_ranges = ranges;
                self.state = match m {
                    Ok(mut m) => {
                        match self.scene_frame {
                            Some((lo, hi)) => (m.min, m.max) = (lo, hi),
                            None => self.scene_frame = Some(scene_bounds(m.min, m.max)),
                        }
                        State::Ready(std::sync::Arc::new(m))
                    }
                    Err(e) => {
                        self.prepared = None;
                        State::Failed(e)
                    }
                };
                self.scene_ver += 1;
                self.scene_rx = None;
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
            }
        }
        if let (None, State::Ready(m)) = (self.scene_frame, &self.state) {
            self.scene_frame = Some(scene_bounds(m.min, m.max));
        }
        let path = self.path.clone();
        let scene = self.scene.as_mut().unwrap();
        let mut changed = false;
        let mut reframe = false;
        // ---- Timeline : lecture, image courante, clés de l'objet choisi
        let mut seek: Option<f32> = None;
        if self.anim_playing {
            let now = std::time::Instant::now();
            let dt = self.anim_last.map(|t| now.duration_since(t).as_secs_f32()).unwrap_or(0.0).min(0.25);
            self.anim_last = Some(now);
            let (a, b) = (scene.start as f32, scene.end.max(scene.start + 1) as f32);
            let mut f = self.anim_frame + dt * scene.fps.max(1.0);
            if f > b {
                f = a + (f - b) % (b - a);
            }
            seek = Some(f);
            ui.ctx().request_repaint();
        } else {
            self.anim_last = None;
        }
        // Timeline (une bande) ou éditeur de courbes (un panneau qu'on agrandit)
        let tl_frame = egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(10, 4));
        let tl_panel = if self.anim_graph { egui::Panel::bottom("scene-graph").resizable(true).default_size(300.0).min_size(160.0).max_size((ui.available_height() * 0.7).max(180.0)) } else { egui::Panel::bottom("scene-timeline").exact_size(58.0) };
        let sub_body = |ui: &mut egui::Ui| {
            ui.horizontal(|ui| {
                if ui.button(if self.anim_playing { "⏸" } else { "▶" }).on_hover_text(crate::i18n::t("lire / arrêter (Espace)")).clicked() {
                    self.anim_playing = !self.anim_playing;
                }
                if ui.button("⏮").on_hover_text(crate::i18n::t("revenir au début")).clicked() {
                    seek = Some(scene.start as f32);
                }
                let mut fr = self.anim_frame.round() as i32;
                if ui.add(egui::DragValue::new(&mut fr).prefix(format!("{} ", crate::i18n::t("image")))).changed() {
                    seek = Some(fr as f32);
                }
                ui.separator();
                let sel = self.scene_sel.filter(|&k| k < scene.objects.len());
                let here = self.anim_frame.round() as i32;
                let on_key = sel.is_some_and(|k| scene.objects[k].keys.iter().any(|x| x.frame == here));
                if ui.add_enabled(sel.is_some(), egui::Button::new(egui::RichText::new(if on_key { "◆" } else { "◇" }).color(if on_key { p.accent } else { p.text }))).on_hover_text(crate::i18n::t("Poser une clé pour l'objet choisi à cette image (K)")).clicked() {
                    if let Some(k) = sel {
                        scene.objects[k].set_key(here);
                        changed = true;
                    }
                }
                if ui.add_enabled(on_key, egui::Button::new("✕")).on_hover_text(crate::i18n::t("Retirer la clé de cette image")).clicked() {
                    if let Some(k) = sel {
                        scene.objects[k].keys.retain(|x| x.frame != here);
                        changed = true;
                    }
                }
                if let Some(key) = sel.and_then(|k| scene.objects[k].keys.iter_mut().find(|x| x.frame == here)) {
                    // Passage vers la clé suivante
                    let cur = key.mode().to_string();
                    egui::ComboBox::from_id_salt("key-interp").width(90.0).selected_text(crate::i18n::t(match cur.as_str() { "linear" => "linéaire", "constant" => "constant", _ => "Bézier" })).show_ui(ui, |ui| {
                        for (m, l) in [("bezier", "Bézier"), ("linear", "linéaire"), ("constant", "constant")] {
                            if ui.selectable_label(cur == m, crate::i18n::t(l)).clicked() {
                                key.interp = m.to_string();
                                changed = true;
                            }
                        }
                    });
                }
                ui.toggle_value(&mut self.anim_auto, egui::RichText::new(crate::i18n::t("clé auto")).size(11.0)).on_hover_text(crate::i18n::t("Chaque déplacement pose une clé à l'image courante"));
                ui.separator();
                if ui.toggle_value(&mut self.anim_graph, egui::RichText::new(crate::i18n::t("courbes")).size(11.0)).on_hover_text(crate::i18n::t("Éditeur de courbes : clés et poignées de Bézier par canal (Maj+F6)")).changed() {
                    self.graph.reset_view();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    changed |= ui.add(egui::DragValue::new(&mut scene.fps).range(1.0..=1000.0).suffix(" i/s")).changed();
                    changed |= ui.add(egui::DragValue::new(&mut scene.end).prefix(format!("{} ", crate::i18n::t("fin")))).changed();
                    changed |= ui.add(egui::DragValue::new(&mut scene.start).prefix(format!("{} ", crate::i18n::t("début")))).changed();
                });
            });
            if self.anim_graph {
                let sel = self.scene_sel.filter(|&k| k < scene.objects.len());
                if self.graph.who != sel {
                    self.graph.who = sel;
                    self.graph.reset_view();
                }
                match sel {
                    Some(k) => {
                        let mut keys = std::mem::take(&mut scene.objects[k].keys);
                        let out = self.graph.ui(ui, &mut keys, self.anim_frame, (scene.start, scene.end));
                        scene.objects[k].keys = keys;
                        if out.changed {
                            // La pose suit la courbe à l'image courante
                            if let Some((pos, rot, scale)) = scene.objects[k].pose_at(self.anim_frame) {
                                (scene.objects[k].pos, scene.objects[k].rot, scene.objects[k].scale) = (pos, rot, scale);
                            }
                            changed = true;
                        }
                        if out.seek.is_some() {
                            seek = out.seek;
                        }
                    }
                    None => {
                        ui.centered_and_justified(|ui| ui.label(egui::RichText::new(crate::i18n::t("choisis un objet pour voir ses courbes")).color(p.muted)));
                    }
                }
                return;
            }
            // La règle : un clic ou un glissement déplace la tête de lecture ; les losanges sont les clés de l'objet choisi
            let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 20.0), egui::Sense::click_and_drag());
            let (a, b) = (scene.start as f32, scene.end.max(scene.start + 1) as f32);
            let x_of = |f: f32| r.left() + 6.0 + (r.width() - 12.0) * ((f - a) / (b - a)).clamp(0.0, 1.0);
            let painter = ui.painter();
            painter.rect_filled(r, 3.0, p.bg);
            let step = [1, 2, 5, 10, 20, 50, 100, 200, 500].into_iter().find(|s| (r.width() - 12.0) / (b - a) * *s as f32 > 46.0).unwrap_or(1000);
            let mut f = (scene.start as f32 / step as f32).ceil() as i32 * step;
            while f <= scene.end {
                let x = x_of(f as f32);
                painter.line_segment([egui::pos2(x, r.bottom() - 5.0), egui::pos2(x, r.bottom())], Stroke::new(1.0, p.muted));
                painter.text(egui::pos2(x + 3.0, r.top() + 1.0), egui::Align2::LEFT_TOP, f.to_string(), egui::FontId::monospace(9.0), p.muted);
                f += step;
            }
            for (k, o) in scene.objects.iter().enumerate() {
                let on = self.scene_sel == Some(k);
                for key in &o.keys {
                    let c = egui::pos2(x_of(key.frame as f32), r.center().y + 3.0);
                    let d = if on { 4.5 } else { 2.5 };
                    painter.add(egui::Shape::convex_polygon(vec![c + egui::vec2(0.0, -d), c + egui::vec2(d, 0.0), c + egui::vec2(0.0, d), c + egui::vec2(-d, 0.0)], if on { p.accent } else { p.muted }, Stroke::NONE));
                }
            }
            let x = x_of(self.anim_frame);
            painter.line_segment([egui::pos2(x, r.top()), egui::pos2(x, r.bottom())], Stroke::new(2.0, p.accent));
            if let Some(at) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
                seek = Some((a + (b - a) * ((at.x - r.left() - 6.0) / (r.width() - 12.0)).clamp(0.0, 1.0)).round());
            }
        };
        if let Some(sub_r) = crate::layout::offer("3d-timeline", "timeline 3D") {
            crate::layout::draw_in(ui, sub_r, p.panel, 6.0, sub_body);
        } else {
            tl_panel.frame(tl_frame).show(ui, sub_body);
        }
        // Menu « ajouter » de l'en-tête
        if let Some(id) = self.hdr_add.take() {
            let k = match id {
                "light" => {
                    let k = scene.add_role("soleil", "light");
                    (scene.objects[k].pos, scene.objects[k].rot) = ([0.0, 3.0, 0.0], [35.0, 0.0, -35.0]);
                    k
                }
                "light-point" | "light-spot" | "light-area" => {
                    let (name, shape) = match id {
                        "light-point" => ("ponctuelle", 1),
                        "light-spot" => ("spot", 2),
                        _ => ("rectangle", 3),
                    };
                    let k = scene.add_role(name, "light");
                    let o = &mut scene.objects[k];
                    o.shape = shape;
                    o.point = true;
                    o.pos = [0.0, 3.0, 0.0];
                    o.energy = 2.0;
                    k
                }
                "camera" => scene.add_role("caméra", "camera"),
                "empty" => scene.add_role("groupe", ""),
                prim => scene.add_primitive(prim),
            };
            self.scene_sel = Some(k);
            self.scene_multi.clear();
            changed = true;
        }
        // Menu « ajouter » sous le curseur (Maj+A), comme dans Blender
        if let Some(at) = self.add_popup {
            let mut close = false;
            let area = egui::Area::new(egui::Id::unique("scene-add-popup")).order(egui::Order::Foreground).fixed_pos(at).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(170.0);
                    ui.label(crate::theme::section(&crate::i18n::t("AJOUTER")));
                    for (id, label) in crate::scene::PRIMITIVES {
                        if ui.button(crate::i18n::t(label)).clicked() {
                            self.scene_sel = Some(scene.add_primitive(id));
                            changed = true;
                            close = true;
                        }
                    }
                    ui.separator();
                    for (label, base, role) in [("lumière", "soleil", "light"), ("caméra (point de vue)", "caméra", "camera"), ("groupe vide", "groupe", "")] {
                        if ui.button(crate::i18n::t(label)).clicked() {
                            let k = scene.add_role(base, role);
                            if role == "light" {
                                (scene.objects[k].pos, scene.objects[k].rot) = ([0.0, 3.0, 0.0], [35.0, 0.0, -35.0]);
                            }
                            self.scene_sel = Some(k);
                            changed = true;
                            close = true;
                        }
                    }
                });
            });
            if close || ui.input(|i| i.key_pressed(Key::Escape)) || (ui.input(|i| i.pointer.any_pressed()) && !area.response.contains_pointer()) {
                self.add_popup = None;
            }
        }
        if !ui.ctx().egui_wants_keyboard_input() && self.edit.is_none() && self.modal.is_none() && self.mesh.is_none() {
            let sel = self.scene_sel.filter(|&k| k < scene.objects.len());
            let (mut del, mut dup, mut hide, mut show_all, add) = ui.input(|i| {
                let plain = !i.modifiers.any();
                (plain && (i.key_pressed(Key::X) || i.key_pressed(Key::Delete)), i.modifiers.shift && i.key_pressed(Key::D), plain && i.key_pressed(Key::H), i.modifiers.alt && i.key_pressed(Key::H), i.modifiers.shift && i.key_pressed(Key::A))
            });
            // Commande choisie dans le menu du clic droit
            let act = self.obj_action.take();
            match act {
                Some("delete") => del = true,
                Some("duplicate") => dup = true,
                Some("hide") => hide = true,
                Some("show-all") => show_all = true,
                Some("edit") => self.mesh_toggle = true,
                Some(m @ ("move" | "rotate" | "scale")) => {
                    if let (Some(k), Some(at)) = (sel, ui.input(|i| i.pointer.hover_pos())) {
                        let o = &scene.objects[k];
                        let kind = match m {
                            "move" => 0,
                            "rotate" => 1,
                            _ => 2,
                        };
                        self.modal = Some((kind, None, (o.pos, o.rot, o.scale), at));
                        self.gizmo_mode = kind;
                    }
                }
                Some("reset") => {
                    if let Some(k) = sel {
                        let o = &mut scene.objects[k];
                        (o.pos, o.rot, o.scale) = ([0.0; 3], [0.0; 3], [1.0; 3]);
                        changed = true;
                    }
                }
                Some("no-material") => {
                    if let Some(k) = sel {
                        scene.objects[k].material.clear();
                        changed = true;
                    }
                }
                _ => {}
            }
            if add {
                self.add_popup = ui.input(|i| i.pointer.hover_pos()).or(Some(ui.ctx().content_rect().center()));
            }
            let (all, none, parent, unparent, boxk) = ui.input(|i| (!i.modifiers.any() && i.key_pressed(Key::A), i.modifiers.alt && i.key_pressed(Key::A), !i.modifiers.any() && i.key_pressed(Key::P), i.modifiers.alt && i.key_pressed(Key::P), !i.modifiers.any() && i.key_pressed(Key::B)));
            if all {
                self.scene_multi = (0..scene.objects.len()).filter(|&k| scene.shown(k)).collect();
                if sel.is_none() {
                    self.scene_sel = self.scene_multi.last().copied();
                }
                self.multi_follow = None;
            }
            if none {
                self.scene_multi.clear();
                self.scene_sel = None;
            }
            if ui.input(|i| i.modifiers.command && i.key_pressed(Key::I)) {
                let mut cur: Vec<usize> = self.scene_multi.clone();
                cur.extend(self.scene_sel);
                let inv: Vec<usize> = (0..scene.objects.len()).filter(|k| !cur.contains(k) && scene.shown(*k)).collect();
                self.scene_sel = inv.last().copied();
                self.scene_multi = if inv.len() > 1 { inv } else { vec![] };
                self.multi_follow = None;
            }
            if boxk {
                self.box_sel = Some(None);
            }
            // Tous les objets choisis, l'actif en dernier
            let mut chosen: Vec<usize> = self.scene_multi.iter().copied().filter(|&k| Some(k) != sel).collect();
            chosen.extend(sel);
            if let Some(k) = sel {
                if parent && chosen.len() > 1 {
                    // P : les autres objets choisis deviennent les enfants de l'actif (Alt+P : plus de parent)
                    let name = scene.objects[k].name.clone();
                    for &c in chosen.iter().filter(|&&c| c != k) {
                        if !scene.descends(k, c) {
                            scene.objects[c].parent = name.clone();
                        }
                    }
                    changed = true;
                } else if unparent {
                    for &c in &chosen {
                        scene.objects[c].parent.clear();
                    }
                    changed = true;
                } else if del {
                    let mut order = chosen.clone();
                    order.sort_unstable_by(|a, b| b.cmp(a));
                    for c in order {
                        scene.remove(c);
                    }
                    self.scene_sel = None;
                    self.scene_multi.clear();
                    changed = true;
                } else if dup {
                    // Les copies vont à la fin, deviennent la sélection, et partent tout de suite en déplacement
                    let first = scene.objects.len();
                    for &c in &chosen {
                        let mut copy = scene.objects[c].clone();
                        copy.name = format!("{} copie", copy.name);
                        scene.objects.push(copy);
                    }
                    self.scene_multi = if chosen.len() > 1 { (first..scene.objects.len()).collect() } else { vec![] };
                    self.scene_sel = Some(scene.objects.len() - 1);
                    self.multi_follow = None;
                    changed = true;
                    if let Some(at) = ui.input(|i| i.pointer.hover_pos()) {
                        let o = &scene.objects[scene.objects.len() - 1];
                        self.modal = Some((0, None, (o.pos, o.rot, o.scale), at));
                    }
                } else if hide {
                    for &c in &chosen {
                        scene.objects[c].visible = false;
                    }
                    changed = true;
                }
            }
            if show_all {
                scene.objects.iter_mut().for_each(|o| o.visible = true);
                changed = true;
            }
        }
        if !ui.ctx().egui_wants_keyboard_input() {
            if ui.input(|i| i.key_pressed(Key::I) && !i.modifiers.any()) {
                if let Some(k) = self.scene_sel.filter(|&k| k < scene.objects.len()) {
                    scene.objects[k].set_key(self.anim_frame.round() as i32);
                    changed = true;
                }
            }
            if ui.input(|i| i.key_pressed(Key::Space)) {
                self.anim_playing = !self.anim_playing;
            }
            if ui.input(|i| i.key_pressed(Key::K) && !i.modifiers.any()) {
                if let Some(k) = self.scene_sel.filter(|&k| k < scene.objects.len()) {
                    scene.objects[k].set_key(self.anim_frame.round() as i32);
                    changed = true;
                }
            }
        }
        if let Some(f) = seek {
            if f != self.anim_frame {
                self.anim_frame = f;
                if scene.apply_frame(f) {
                    self.scene_dirty = true;
                    // Changer d'image n'est pas une action à annuler
                    self.scene_saved = Some(scene.clone());
                }
            }
        }
        // Sélection multiple : ce qui arrive à l'objet actif (poignée, G R S, champs) arrive aux autres objets choisis
        self.scene_multi.retain(|&k| k < scene.objects.len());
        if seek.is_some() {
            self.multi_follow = None;
        }
        if let Some(a) = self.scene_sel.filter(|&k| k < scene.objects.len()) {
            let cur = (scene.objects[a].pos, scene.objects[a].rot, scene.objects[a].scale);
            if let Some((who, last)) = self.multi_follow {
                if who == a && last != cur {
                    for &k in self.scene_multi.iter().filter(|&&k| k != a) {
                        let o = &mut scene.objects[k];
                        for c in 0..3 {
                            o.pos[c] += cur.0[c] - last.0[c];
                            o.rot[c] += cur.1[c] - last.1[c];
                            if last.2[c].abs() > 1e-6 {
                                o.scale[c] *= cur.2[c] / last.2[c];
                            }
                        }
                        self.scene_dirty = true;
                    }
                }
            }
            self.multi_follow = Some((a, cur));
        } else {
            self.multi_follow = None;
        }
        let pose_before = self.scene_sel.and_then(|k| scene.objects.get(k)).map(|o| (o.pos, o.rot, o.scale));
        let mut renamed: Option<(String, String)> = None;
        // Caméra à poser sur la vue actuelle, ou vue à aligner sur une caméra
        let mut set_camera: Option<usize> = None;
        let mut look_through: Option<usize> = None;
        for f in std::mem::take(&mut self.scene_add) {
            let image = crate::scene::kind(&crate::scene::Object::new(String::new(), f.to_string_lossy().to_string())) == crate::scene::Kind::Image;
            if (is_model(&f.to_string_lossy()) || image) && !crate::scene::is_scene(&f) {
                scene.add(&path, &f);
                self.scene_sel = Some(scene.objects.len() - 1);
                changed = true;
            }
        }
        let sub_body = |ui: &mut egui::Ui| {
            ui.spacing_mut().slider_width = 80.0;
            ui.horizontal(|ui| {
                ui.label(crate::theme::section(&crate::i18n::t("SCÈNE")));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::containers::menu::MenuButton::new(crate::i18n::t("＋ ajouter ▾")).ui(ui, |ui| {
                        ui.set_min_width(240.0);
                        ui.menu_button(crate::i18n::t("forme de base"), |ui| {
                            for (id, label) in crate::scene::PRIMITIVES {
                                if ui.button(crate::i18n::t(label)).clicked() {
                                    self.scene_sel = Some(scene.add_primitive(id));
                                    changed = true;
                                    ui.close();
                                }
                            }
                        });
                        if ui.button(crate::i18n::t("choisir un fichier…")).clicked() {
                            if let Some(files) = rfd::FileDialog::new().add_filter(crate::i18n::t("modèles et images"), &["obj", "fbx", "glb", "gltf", "stl", "3ds", "ply", "ma", "png", "jpg", "jpeg", "webp"]).pick_files() {
                                for f in files {
                                    scene.add(&path, &f);
                                    changed = true;
                                }
                                self.scene_sel = scene.objects.len().checked_sub(1);
                            }
                            ui.close();
                        }
                        for (label, base, role) in [("lumière", "soleil", "light"), ("caméra (point de vue)", "caméra", "camera")] {
                            if ui.button(crate::i18n::t(label)).clicked() {
                                let k = scene.add_role(base, role);
                                if role == "light" {
                                    // Posée en hauteur, penchée : un éclairage de trois quarts
                                    scene.objects[k].pos = [0.0, 3.0, 0.0];
                                    scene.objects[k].rot = [35.0, 0.0, -35.0];
                                } else {
                                    set_camera = Some(k);
                                }
                                self.scene_sel = Some(k);
                                changed = true;
                                ui.close();
                            }
                        }
                        if ui.button(crate::i18n::t("groupe vide")).on_hover_text(crate::i18n::t("Un objet sans forme, pour déplacer plusieurs objets ensemble")).clicked() {
                            self.scene_sel = Some(scene.add_empty());
                            changed = true;
                            ui.close();
                        }
                        if !self.model_files.is_empty() {
                            ui.separator();
                            ui.label(egui::RichText::new(crate::i18n::t("modèles du coffre")).size(10.5).color(p.muted));
                            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                                for (name, f) in &self.model_files {
                                    if ui.button(name).clicked() {
                                        scene.add(&path, f);
                                        self.scene_sel = Some(scene.objects.len() - 1);
                                        changed = true;
                                        ui.close();
                                    }
                                }
                            });
                        }
                    });
                });
            });
            if ui.add(egui::Button::new(egui::RichText::new(crate::i18n::t("recadrer la vue sur la scène")).size(10.5).color(p.muted)).frame(false)).clicked() {
                reframe = true;
            }
            if scene.objects.is_empty() {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(crate::i18n::t("Scène vide. Glisse des modèles sur la fenêtre, ou ajoute-en avec le bouton.")).color(p.muted).size(11.0));
            }
            // (index, action) : 0 supprimer, 1 dupliquer
            let mut action: Option<(usize, u8)> = None;
            egui::ScrollArea::vertical().id_salt("scene-objects").max_height((ui.available_height() * 0.36).max(110.0)).show(ui, |ui| {
                for (i, depth) in scene.tree() {
                    let o = &mut scene.objects[i];
                    ui.horizontal(|ui| {
                        ui.add_space(depth as f32 * 14.0);
                        let icon = match crate::scene::kind(o) {
                            crate::scene::Kind::Model => "▣",
                            crate::scene::Kind::Image => "▨",
                            crate::scene::Kind::Empty => "◇",
                            crate::scene::Kind::Light => "☀",
                            crate::scene::Kind::Camera => "◎",
                        };
                        let (ir, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                        crate::icons::glyph(ui.painter(), ir, icon, p.muted);
                        if crate::icons::button(ui, if o.visible { crate::icons::I::Eye } else { crate::icons::I::EyeOff }, false, 18.0, &crate::i18n::t("afficher / cacher")).clicked() {
                            o.visible = !o.visible;
                            changed = true;
                        }
                        let on = self.scene_sel == Some(i);
                        let also = self.scene_multi.contains(&i);
                        let bad = self.scene_errors.get(i).and_then(|e| e.as_ref());
                        let text = egui::RichText::new(&o.name).size(12.0).color(if bad.is_some() { p.missing } else if on { p.accent } else if o.visible { p.text } else { p.muted });
                        let text = if also && !on { text.color(p.accent.gamma_multiply(0.75)) } else { text };
                        let r = ui.add(egui::Button::new(if on { text.family(theme::bold()) } else { text }).frame(false));
                        let r = match bad {
                            Some(e) => r.on_hover_text(format!("{} : {e}", o.file)),
                            None => r.on_hover_text(&o.file),
                        };
                        if r.clicked() {
                            if ui.input(|x| x.modifiers.shift || x.modifiers.command) {
                                // Maj ou Ctrl : ajoute à la sélection (l'objet cliqué devient l'actif), ou l'en retire
                                if self.scene_multi.is_empty() {
                                    self.scene_multi.extend(self.scene_sel);
                                }
                                if let Some(at) = self.scene_multi.iter().position(|&k| k == i) {
                                    self.scene_multi.remove(at);
                                    if on {
                                        self.scene_sel = self.scene_multi.last().copied();
                                    }
                                } else {
                                    self.scene_multi.push(i);
                                    self.scene_sel = Some(i);
                                }
                            } else {
                                self.scene_multi.clear();
                                self.scene_sel = if on { None } else { Some(i) };
                            }
                            self.multi_follow = None;
                        }
                        r.context_menu(|ui| {
                            if ui.button(crate::i18n::t("dupliquer")).clicked() {
                                action = Some((i, 1));
                                ui.close();
                            }
                            if ui.button(egui::RichText::new(crate::i18n::t("retirer de la scène")).color(p.missing)).clicked() {
                                action = Some((i, 0));
                                ui.close();
                            }
                        });
                    });
                }
            });
            if let Some((i, a)) = action {
                self.scene_multi.clear();
                if a == 0 {
                    scene.remove(i);
                    self.scene_sel = None;
                } else {
                    let mut c = scene.objects[i].clone();
                    c.name = format!("{} copie", c.name);
                    c.pos[0] += 1.0;
                    scene.objects.insert(i + 1, c);
                    self.scene_sel = Some(i + 1);
                }
                changed = true;
            }
            // Parents possibles pour l'objet choisi : tout sauf lui-même et ses propres enfants
            let parents: Vec<String> = match self.scene_sel {
                Some(s) if s < scene.objects.len() => (0..scene.objects.len()).filter(|&k| k != s && !scene.descends(k, s)).map(|k| scene.objects[k].name.clone()).collect(),
                _ => vec![],
            };
            // ---- Propriétés, comme l'éditeur de Blender : une colonne d'onglets, et le contenu à côté
            ui.separator();
            let tab = self.prop_tab;
            let mut new_tab = tab;
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    for (t, icon, tip) in PROP_TABS {
                        let on = tab == t;
                        // Tous les onglets à la même taille, alignés, quel que soit le glyphe
                        let (r, resp) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
                        let fill = if on { p.accent } else if resp.hovered() { p.line } else { p.panel };
                        ui.painter().rect_filled(r, 3.0, fill);
                        crate::icons::glyph(ui.painter(), r.shrink(6.0), icon, if on { p.on_accent } else { p.text });
                        if resp.on_hover_text(crate::i18n::t(tip)).clicked() {
                            new_tab = t;
                        }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    let title = PROP_TABS.iter().find(|x| x.0 == tab).map(|x| x.2).unwrap_or("");
                    ui.label(crate::theme::section(&crate::i18n::t(title)));
                    egui::ScrollArea::vertical().id_salt("scene-props").show(ui, |ui| {
                        match tab {
                            PropTab::Scene => {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(crate::i18n::t("images/s")).size(11.0).color(p.muted));
                                    changed |= ui.add(egui::DragValue::new(&mut scene.fps).range(1.0..=1000.0)).changed();
                                });
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(crate::i18n::t("début")).size(11.0).color(p.muted));
                                    changed |= ui.add(egui::DragValue::new(&mut scene.start)).changed();
                                    ui.label(egui::RichText::new(crate::i18n::t("fin")).size(11.0).color(p.muted));
                                    changed |= ui.add(egui::DragValue::new(&mut scene.end)).changed();
                                });
                                ui.label(egui::RichText::new(format!("{} {}", scene.objects.len(), crate::i18n::t_mid("objet(s)"))).size(11.0).color(p.muted));
                                ui.separator();
                                ui.label(egui::RichText::new(crate::i18n::t("SUPERPOSITIONS")).size(10.5).color(p.muted));
                                overlays_ui(&mut self.look, ui);
                            }
                            PropTab::World => {
                                lighting_ui(&mut self.look, &self.hdr_files, self.env_loading.is_some(), &self.env_error, &mut self.pending_hdr, ui);
                            }
                            _ => match self.scene_sel.and_then(|i| scene.objects.get_mut(i)) {
                                None => {
                                    ui.label(egui::RichText::new(crate::i18n::t("Choisis un objet dans la liste ou dans la vue.")).size(11.0).color(p.muted));
                                }
                                Some(o) => match tab {
                                    PropTab::Object => {
                                        let before = o.name.clone();
                                        if ui.add(egui::TextEdit::singleline(&mut o.name).desired_width(f32::INFINITY)).changed() {
                                            renamed = Some((before, o.name.clone()));
                                            changed = true;
                                        }
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(crate::i18n::t("parent")).size(11.0).color(p.muted));
                                            let cur = if o.parent.is_empty() { crate::i18n::t("aucun") } else { o.parent.clone() };
                                            egui::ComboBox::from_id_salt("scene-object-parent").selected_text(egui::RichText::new(cur).size(11.0)).width(130.0).show_ui(ui, |ui| {
                                                if ui.selectable_label(o.parent.is_empty(), crate::i18n::t("aucun")).clicked() {
                                                    o.parent.clear();
                                                    changed = true;
                                                }
                                                for name in &parents {
                                                    if ui.selectable_label(&o.parent == name, name).clicked() {
                                                        o.parent = name.clone();
                                                        changed = true;
                                                    }
                                                }
                                            });
                                        });

                                        if crate::scene::kind(o) == crate::scene::Kind::Image {
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new(crate::i18n::t("opacité")).size(11.0).color(p.muted));
                                                changed |= ui.add(egui::Slider::new(&mut o.opacity, 0.0..=1.0).show_value(false)).changed();
                                            });
                                        }

                                        if crate::scene::kind(o) == crate::scene::Kind::Model {
                                            let on = self.mesh.is_some();
                                            if ui.add(egui::Button::new(egui::RichText::new(crate::i18n::t(if on { "sortir du mode édition (Tab)" } else { "mode édition (Tab)" })).size(11.5).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { p.panel })).on_hover_text(crate::i18n::t("Modifier le maillage : sommets, extrusion, faces. Le maillage devient un fichier OBJ à côté de la scène ; le modèle d'origine n'est pas touché.")).clicked() {
                                                self.mesh_toggle = true;
                                            }
                                        }

                                        let mut row = |ui: &mut egui::Ui, label: &str, v: &mut [f32; 3], speed: f64, reset: [f32; 3]| {
                                            ui.horizontal(|ui| {
                                                if ui.add(egui::Button::new(egui::RichText::new(crate::i18n::t(label)).size(11.0).color(p.muted)).frame(false)).on_hover_text(crate::i18n::t("cliquer pour remettre à zéro")).clicked() {
                                                    *v = reset;
                                                    changed = true;
                                                }
                                            });
                                            // Montrés avec Z vers le haut ; l'échelle n'a pas de sens
                                            let signed = reset != [1.0; 3];
                                            ui.horizontal(|ui| {
                                                for (shown, axis) in ["x", "y", "z"].iter().enumerate() {
                                                    let (k, sign) = crate::gizmo::SHOWN[shown];
                                                    let sign = if signed { sign } else { 1.0 };
                                                    // Position : dans l'unité choisie (la scène est en mètres)
                                                    let (uname, uf) = if label == "position" { crate::settings::unit() } else { ("", 1.0) };
                                                    let mut val = v[k] * sign * uf + 0.0;
                                                    if val == 0.0 {
                                                        val = 0.0;
                                                    }
                                                    if ui.add(egui::DragValue::new(&mut val).speed(speed * uf as f64).prefix(format!("{axis} ")).suffix(if uname.is_empty() { String::new() } else { format!(" {uname}") }).max_decimals(3)).changed() {
                                                        v[k] = val * sign / uf;
                                                        changed = true;
                                                    }
                                                }
                                            });
                                        };
                                        row(ui, "position", &mut o.pos, 0.01, [0.0; 3]);
                                        row(ui, "rotation °", &mut o.rot, 0.5, [0.0; 3]);
                                        row(ui, "échelle", &mut o.scale, 0.005, [1.0; 3]);
                                        ui.horizontal(|ui| {
                                            let mut uniform = o.scale[0];
                                            if ui.add(egui::DragValue::new(&mut uniform).speed(0.005).prefix(format!("{} ", crate::i18n::t("uniforme")))).changed() {
                                                o.scale = [uniform; 3];
                                                changed = true;
                                            }
                                        });
                                        ui.horizontal(|ui| {
                                            let mut tinted = o.color.is_some();
                                            if ui.checkbox(&mut tinted, crate::i18n::t("couleur")).changed() {
                                                o.color = tinted.then_some([200, 200, 200]);
                                                changed = true;
                                            }
                                            if let Some(c) = &mut o.color {
                                                changed |= ui.color_edit_button_srgb(c).changed();
                                            }
                                        });

                                    }
                                    PropTab::Data => {
                                        match crate::scene::kind(o) {
                                            crate::scene::Kind::Light => {
                                                ui.horizontal(|ui| {
                                                    ui.label(egui::RichText::new(crate::i18n::t("force")).size(11.0).color(p.muted));
                                                    changed |= ui.add(egui::Slider::new(&mut o.energy, 0.0..=5.0).clamping(egui::SliderClamping::Never)).changed();
                                                });
                                                ui.horizontal_wrapped(|ui| {
                                                    let cur = crate::scene::light_shape(o);
                                                    for (k, name, tip) in [
                                                        (0u8, "soleil", "Une direction : tourne-la pour changer d'où vient la lumière"),
                                                        (1, "ponctuelle", "Elle éclaire autour d'elle : déplace-la"),
                                                        (2, "spot", "Un cône de lumière, vers le bas de son axe Y : tourne-le pour viser"),
                                                        (3, "rectangle", "Un panneau lumineux (comme une boîte à lumière) : ombres douces"),
                                                    ] {
                                                        if ui.selectable_label(cur == k, egui::RichText::new(crate::i18n::t(name)).size(11.0)).on_hover_text(crate::i18n::t(tip)).clicked() && cur != k {
                                                            o.shape = k;
                                                            o.point = k != 0;
                                                            changed = true;
                                                        }
                                                    }
                                                });
                                                match crate::scene::light_shape(o) {
                                                    2 => {
                                                        ui.horizontal(|ui| {
                                                            ui.label(egui::RichText::new(crate::i18n::t("ouverture °")).size(11.0).color(p.muted));
                                                            changed |= ui.add(egui::DragValue::new(&mut o.spot).speed(0.5).range(1.0..=179.0)).changed();
                                                            ui.label(egui::RichText::new(crate::i18n::t("fondu")).size(11.0).color(p.muted));
                                                            changed |= ui.add(egui::DragValue::new(&mut o.blend).speed(0.01).range(0.0..=1.0)).changed();
                                                        });
                                                    }
                                                    3 => {
                                                        ui.horizontal(|ui| {
                                                            ui.label(egui::RichText::new(crate::i18n::t("taille")).size(11.0).color(p.muted));
                                                            changed |= ui.add(egui::DragValue::new(&mut o.size[0]).speed(0.02).range(0.001..=f32::MAX).prefix("x ")).changed();
                                                            changed |= ui.add(egui::DragValue::new(&mut o.size[1]).speed(0.02).range(0.001..=f32::MAX).prefix("y ")).changed();
                                                        });
                                                    }
                                                    _ => {}
                                                }
                                                if o.point {
                                                    ui.horizontal(|ui| {
                                                        ui.label(egui::RichText::new(crate::i18n::t("portée")).size(11.0).color(p.muted));
                                                        changed |= ui.add(egui::DragValue::new(&mut o.range).speed(0.05).range(0.01..=f32::MAX)).changed();
                                                    });
                                                }
                                                ui.label(egui::RichText::new(crate::i18n::t("Sa couleur se règle dans l'onglet objet.")).size(10.0).color(p.muted));
                                            }
                                            crate::scene::Kind::Camera => {
                                                ui.horizontal(|ui| {
                                                    if ui.button(crate::i18n::t("regarder par ici")).on_hover_text(crate::i18n::t("Aligne la vue sur cette caméra")).clicked() {
                                                        look_through = self.scene_sel;
                                                    }
                                                    if ui.button(crate::i18n::t("poser sur la vue")).on_hover_text(crate::i18n::t("Enregistre la vue actuelle dans cette caméra")).clicked() {
                                                        set_camera = self.scene_sel;
                                                    }
                                                });
                                            }
                                            _ => {}
                                        }

                                        if !matches!(crate::scene::kind(o), crate::scene::Kind::Light | crate::scene::Kind::Camera) {
                                            ui.label(egui::RichText::new(&o.file).size(10.5).color(p.muted));
                                        }
                                    }
                                    PropTab::Material => {
                                        ui.label(egui::RichText::new(&o.file).size(10.0).color(p.muted));
                                        // Matériau en nodes de l'objet
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(crate::i18n::t("matériau")).size(11.0).color(p.muted));
                                            let cur = if o.material.is_empty() { crate::i18n::t("aucun") } else { crate::formats::strip(o.material.rsplit('/').next().unwrap_or(&o.material)).to_string() };
                                            egui::ComboBox::from_id_salt("scene-object-material").selected_text(egui::RichText::new(cur).size(11.0)).width(110.0).show_ui(ui, |ui| {
                                                if ui.selectable_label(o.material.is_empty(), crate::i18n::t("aucun")).clicked() {
                                                    o.material.clear();
                                                    changed = true;
                                                }
                                                for (name, mpath) in &self.mat_files {
                                                    let rel = mpath.strip_prefix(&self.root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| mpath.to_string_lossy().to_string());
                                                    if ui.selectable_label(o.material == rel, name).clicked() {
                                                        o.material = rel;
                                                        changed = true;
                                                    }
                                                }
                                            });
                                            if o.material.is_empty() {
                                                if ui.small_button("＋").on_hover_text(crate::i18n::t("Créer un matériau en nodes pour cet objet")).clicked() && !self.root.is_empty() {
                                                    let dir = std::path::Path::new(&self.root).join("nodes");
                                                    let _ = std::fs::create_dir_all(&dir);
                                                    let mut file = dir.join(format!("{}.{}", o.name, crate::formats::ext("mat")));
                                                    let mut n = 2;
                                                    while file.exists() {
                                                        file = dir.join(format!("{} {n}.{}", o.name, crate::formats::ext("mat")));
                                                        n += 1;
                                                    }
                                                    if crate::nodes::Graph::new_material().save(&file).is_ok() {
                                                        o.material = file.strip_prefix(&self.root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
                                                        self.mat_files.push((o.name.clone(), file.clone()));
                                                        self.open_request = Some(file);
                                                        changed = true;
                                                    }
                                                }
                                            } else if ui.small_button("✎").on_hover_text(crate::i18n::t("Ouvrir ce matériau dans l'éditeur de nodes")).clicked() {
                                                let raw = std::path::PathBuf::from(&o.material);
                                                self.open_request = Some(if raw.is_absolute() { raw } else { std::path::Path::new(&self.root).join(raw) });
                                            }
                                        });

                                    }
                                    PropTab::Modifiers => {
                                        // Pile de modificateurs de cet objet seulement
                                        let mut req = None;
                                        changed |= crate::modifiers::panel_with(ui, &mut o.mods, false, &mut req, false);
                                    }
                                    _ => {}
                                },
                            },
                        }
                    });
                });
            });
            self.prop_tab = new_tab;
        };
        if let Some(sub_r) = crate::layout::offer("3d-outliner", "outliner et propriétés") {
            crate::layout::draw_in(ui, sub_r, p.panel, 8.0, sub_body);
        } else {
            egui::Panel::right("scene-outliner").resizable(true).default_size(270.0).min_size(220.0).max_size((ui.available_width() * 0.45).max(240.0)).frame(egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).inner_margin(8)).show(ui, sub_body);
        }
        if let Some((lo, hi)) = self.scene_frame {
            let longest = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
            let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
            if let Some(o) = set_camera.and_then(|k| scene.objects.get_mut(k)) {
                // Où se trouve l'œil de la vue, et dans quel sens il regarde
                let eye = self.cam.unrotate([-self.cam.pan[0], -self.cam.pan[1], self.cam.dist]);
                o.pos = [eye[0] * longest + c[0], eye[1] * longest + c[1], eye[2] * longest + c[2]];
                o.rot = [-self.cam.pitch.to_degrees(), -self.cam.yaw.to_degrees(), 0.0];
                o.parent.clear();
                changed = true;
            }
            if let Some(k) = look_through.filter(|&k| k < scene.objects.len()) {
                let w = scene.world(k, [0.0; 3]);
                let o = &scene.objects[k];
                self.cam.yaw = -o.rot[1].to_radians();
                self.cam.pitch = -o.rot[0].to_radians();
                self.cam.ortho = false;
                let r = self.cam.rotate([(w[0] - c[0]) / longest, (w[1] - c[1]) / longest, (w[2] - c[2]) / longest]);
                self.cam.pan = [-r[0], -r[1]];
                self.cam.dist = r[2].max(0.05);
            }
        }
        if let Some((old, new)) = renamed {
            // Les enfants suivent le nouveau nom de leur parent
            if let Some(k) = self.scene_sel.filter(|&k| k < scene.objects.len()) {
                scene.objects[k].name = old;
                scene.rename(k, &new);
            }
        }
        // Clé automatique : l'objet choisi vient d'être déplacé à la main
        if self.anim_auto {
            if let Some(o) = self.scene_sel.and_then(|k| scene.objects.get_mut(k)) {
                if pose_before.is_some_and(|b| b != (o.pos, o.rot, o.scale)) {
                    o.set_key(self.anim_frame.round() as i32);
                }
            }
        }
        if reframe {
            self.scene_frame = None;
            self.scene_dirty = true;
        }
        if changed {
            self.scene_dirty = true;
            // Pendant un glisser, le fichier s'écrit au relâché (pas à chaque image)
            if !ui.input(|i| i.pointer.any_down()) {
                let _ = scene.write(&path);
            }
        }
        // Annuler (Ctrl+Z) et rétablir (Ctrl+Maj+Z) : un pas par action terminée, une fois la souris relâchée
        let (undo, redo, down) = ui.input(|i| (i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::Z), i.modifiers.command && (i.modifiers.shift && i.key_pressed(Key::Z) || i.key_pressed(Key::Y)), i.pointer.any_down()));
        if self.scene_saved.is_none() {
            self.scene_saved = Some(scene.clone());
        }
        if !down && self.modal.is_none() && self.scene_saved.as_ref() != Some(scene) {
            if let Some(prev) = self.scene_saved.replace(scene.clone()) {
                self.scene_hist.push(prev);
                if self.scene_hist.len() > 200 {
                    self.scene_hist.remove(0);
                }
                self.scene_redo.clear();
            }
        }
        if !ui.ctx().egui_wants_keyboard_input() && self.edit.is_none() && self.mesh.is_none() && self.modal.is_none() && (undo || redo) {
            let (from, to) = if redo { (&mut self.scene_redo, &mut self.scene_hist) } else { (&mut self.scene_hist, &mut self.scene_redo) };
            if let Some(prev) = from.pop() {
                to.push(std::mem::replace(scene, prev));
                self.scene_saved = Some(scene.clone());
                self.scene_sel = self.scene_sel.filter(|&k| k < scene.objects.len());
                self.scene_multi.clear();
                self.multi_follow = None;
                self.scene_dirty = true;
                let _ = scene.write(&path);
            }
        }
        // Déplacement d'objets seulement (glisser, poignées) : on déplace leurs sommets tout de suite,
        // sans reconstruire la scène ; plus de retard ni d'image qui se recharge pendant le geste.
        // La reconstruction complète (et l'écriture du fichier) se fait au relâché.
        let down = ui.input(|i| i.pointer.any_down());
        if self.scene_dirty && down && self.scene_rx.is_none() && std::env::var("NOFASTMOVE").is_err() {
            if let (Some(built), Some(prep), State::Ready(m)) = (&self.scene_built, &self.prepared, &self.state) {
                if let Some(np) = (!prep.0).then(|| fast_move(built, scene, &self.scene_ranges, &prep.2, m)).flatten() {
                    self.prepared = Some((prep.0, prep.1, std::sync::Arc::new(np)));
                    self.scene_built = Some(scene.clone());
                    self.last = None;
                    self.trace = None;
                    self.scene_dirty = false;
                    self.scene_moving = true;
                }
            }
        }
        if self.scene_moving && !down {
            self.scene_moving = false;
            self.scene_dirty = true;
            let _ = scene.write(&path);
        }
        // Reconstruction en arrière-plan, une à la fois : l'ancienne image reste affichée entre-temps
        if self.scene_dirty && !self.scene_moving && self.scene_rx.is_none() && !matches!(self.state, State::Loading(_)) {
            self.scene_dirty = false;
            self.scene_built = Some(scene.clone());
            let (tx, rx) = channel();
            let (sc, ctx) = (scene.clone(), ui.ctx().clone());
            std::thread::spawn(move || {
                let _ = tx.send(crate::scene::build_ranges(&sc, &path));
                ctx.request_repaint();
            });
            self.scene_rx = Some(rx);
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        if std::mem::take(&mut self.uv_request) {
            self.open_uv_edit(&ui.ctx().clone());
        }
        if let Some((sub, k)) = &mut self.sub {
            let k = *k;
            let name = self.scene.as_ref().and_then(|s| s.objects.get(k)).map(|o| o.name.clone()).unwrap_or_default();
            let mut back = false;
            ui.horizontal(|ui| {
                if ui.button(format!("◀ {}", crate::i18n::t("retour à la scène"))).on_hover_text(crate::i18n::t("Les UV et matériaux sont gardés avec la copie de l'objet propre à la scène")).clicked() || (ui.input(|i| i.key_pressed(Key::Escape)) && sub.edit.is_none()) {
                    back = true;
                }
                ui.label(egui::RichText::new(format!("{} · {name}", crate::i18n::t("UV, matériaux et retopo"))).family(theme::bold()));
            });
            sub.show(ui);
            if let Some(f) = sub.open_request.take() {
                if is_model(&f.to_string_lossy()) {
                    // Retopo exportée : elle remplace le maillage de l'objet dans la scène
                    if let Some(scene) = self.scene.as_mut() {
                        let dir = self.path.parent().unwrap_or(std::path::Path::new(""));
                        scene.objects[k].file = f.strip_prefix(dir).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| f.to_string_lossy().to_string());
                        let _ = scene.write(&self.path);
                        self.capture_msg = Some(crate::i18n::t("retopo appliquée à l'objet"));
                    }
                    back = true;
                } else {
                    self.open_request = Some(f);
                }
            }
            if back {
                self.sub = None;
                // L'objet a pu changer (retopo, maillage) : la scène se recharge
                self.reload_request = true;
            }
            return;
        }
        if let Some((key, modifiers)) = self.hdr_key.take() {
            ui.ctx().input_mut(|i| {
                i.modifiers = modifiers;
                i.events.push(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
            });
        }
        if let State::Loading(rx) = &self.state {
            if let Ok(r) = rx.try_recv() {
                self.state = match r {
                    Ok(m) => State::Ready(std::sync::Arc::new(m)),
                    Err(e) => State::Failed(e),
                };
            }
        }
        if crate::scene::is_scene(&self.path) {
            self.scene_panel(ui);
        }
        if let Some(rx) = &self.env_loading {
            if let Ok(r) = rx.try_recv() {
                match r {
                    Ok(e) => {
                        self.look.env = std::sync::Arc::new(e);
                        self.look.show_bg = true;
                    }
                    Err(e) => self.env_error = Some(e),
                }
                self.env_loading = None;
            }
        }
        let model = match &self.state {
            State::Loading(_) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(egui::RichText::new(crate::i18n::t("chargement du modèle… (les gros fichiers peuvent prendre quelques secondes)")).color(p.muted));
                });
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
            State::Failed(e) => {
                ui.label(egui::RichText::new(e).color(p.missing));
                return;
            }
            State::Ready(m) => m.clone(),
        };
        let model = &model;
        let z_up = *self.z_up.get_or_insert(model.z_up);
        // Pile de modificateurs : chargée une fois, recalculée en arrière-plan quand elle change
        if !self.stack_loaded && !self.root.is_empty() && self.can_edit() {
            self.stack = std::fs::read_to_string(self.mods_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
            self.applied = crate::modifiers::Mesh::load(&self.mods_file().with_extension("mesh")).map(std::sync::Arc::new);
            self.stack_loaded = true;
        }
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_string(&self.stack.iter().filter(|m| m.on).map(|m| m.kind).collect::<Vec<_>>()).unwrap_or_default().hash(&mut h);
            z_up.hash(&mut h);
            self.applied_ver.hash(&mut h);
            self.scene_ver.hash(&mut h);
            h.finish()
        };
        if let Some(e) = &mut self.edit {
            if e.version != self.edit_seen {
                let (mesh, uvs) = e.display(theme::pal().accent);
                let mut prep = Prepared::from_mesh(mesh);
                prep.uv = uvs;
                prep.mat = e.tri_mat.clone();
                self.edit_seen = e.version;
                self.prepared = Some((z_up, 0xED17_0000_0000 + e.version, std::sync::Arc::new(prep)));
                self.last = None;
            }
        } else if let Some((k, rx)) = &self.job {
            if let Ok(p) = rx.try_recv() {
                self.prepared = Some((z_up, *k, std::sync::Arc::new(p)));
                self.last = None;
                self.job = None;
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
            }
        }
        let current = self.prepared.as_ref().map(|p| p.1);
        let pending = self.job.as_ref().map(|j| j.0);
        if self.edit.is_none() && current != Some(key) && pending != Some(key) && self.job.is_none() {
            let (tx, rx) = channel();
            let (m, stack, ctx) = (model.clone(), self.stack.clone(), ui.ctx().clone());
            let applied = self.applied.clone();
            // Scène : le sol est le plan y = 0 du monde, et rien n'est recentré quand un objet bouge
            let fixed_floor = self.scene_frame.filter(|_| self.is_scene()).map(|(lo, hi)| {
                let longest = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
                -((lo[1] + hi[1]) / 2.0) / longest
            });
            std::thread::spawn(move || {
                let s = m.size();
                let longest = s[0].max(s[1]).max(s[2]).max(1e-9);
                let c = [(m.min[0] + m.max[0]) / 2.0, (m.min[1] + m.max[1]) / 2.0, (m.min[2] + m.max[2]) / 2.0];
                let o = [-c[0] / longest, -c[1] / longest, -c[2] / longest];
                let origin = if z_up { [o[0], o[2], -o[1]] } else { o };
                let base = applied.map(|a| (*a).clone()).unwrap_or_else(|| crate::modifiers::Mesh::from_model(&m, z_up));
                let mesh = crate::modifiers::apply(&stack, base, origin);
                let _ = tx.send(match fixed_floor {
                    Some(f) => Prepared::from_mesh_fixed(mesh, f),
                    None => Prepared::from_mesh(mesh),
                });
                ctx.request_repaint();
            });
            self.job = Some((key, rx));
        }
        if self.prepared.is_none() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(crate::i18n::t("préparation du modèle…"));
            });
            return;
        }
        // Scène : un matériau par objet, rechargé dès que son fichier change ; chaque triangle sait à quel objet il appartient
        if self.edit.is_none() {
            if let Some(scene) = self.scene.as_ref().filter(|_| crate::scene::is_scene(&self.path)) {
                self.slot_progs.resize(scene.objects.len(), None);
                for (k, o) in scene.objects.iter().enumerate() {
                    if o.material.is_empty() {
                        if self.slot_progs[k].take().is_some() {
                            self.slot_rev += 1;
                        }
                        continue;
                    }
                    let raw = std::path::PathBuf::from(&o.material);
                    let path = if raw.is_absolute() { raw } else { std::path::Path::new(&self.root).join(raw) };
                    let mt = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                    let fresh = self.slot_progs[k].as_ref().is_some_and(|(p, seen, _)| *p == path && *seen == mt);
                    if !fresh {
                        match crate::nodes::Graph::load(&path).and_then(|g| crate::nodes::Program::compile(&g, &self.root, 1024).map_err(|e| e.1)) {
                            Ok(prog) => self.slot_progs[k] = Some((path, mt, std::sync::Arc::new(prog))),
                            Err(_) => self.slot_progs[k] = None,
                        }
                        self.slot_rev += 1;
                    }
                }
                if let Some(prep) = self.prepared.as_mut().and_then(|p| std::sync::Arc::get_mut(&mut p.2)) {
                    let total = self.scene_ranges.last().map(|r| r.2);
                    if prep.mat.len() != prep.tris.len() && total == Some(prep.tris.len()) {
                        prep.mat = vec![0; prep.tris.len()];
                        for (k, a, b) in &self.scene_ranges {
                            prep.mat[*a..*b].fill(*k as u16);
                        }
                        self.last = None;
                    }
                }
            }
        }
        // Matériaux en nodes des IDs : créés à la demande, rechargés dès que leur fichier change
        if let Some(e) = &mut self.edit {
            if let Some(k) = e.mat_request.take() {
                if e.data.slots.get(k).is_some_and(|s| s.material.is_empty()) && !self.root.is_empty() {
                    let dir = std::path::Path::new(&self.root).join("nodes");
                    let _ = std::fs::create_dir_all(&dir);
                    let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Matériau".into());
                    let base = format!("{stem} - {}", e.data.slots[k].name);
                    let mut path = dir.join(format!("{base}.{}", crate::formats::ext("mat")));
                    let mut n = 2;
                    while path.exists() {
                        path = dir.join(format!("{base} {n}.{}", crate::formats::ext("mat")));
                        n += 1;
                    }
                    if crate::nodes::Graph::new_material().save(&path).is_ok() {
                        e.data.slots[k].material = path.to_string_lossy().to_string();
                        e.save();
                    }
                }
                if let Some(s) = e.data.slots.get(k).filter(|s| !s.material.is_empty()) {
                    self.open_request = Some(std::path::PathBuf::from(&s.material));
                }
            }
            if std::mem::take(&mut e.bake_request) {
                let progs: Vec<Option<std::sync::Arc<crate::nodes::Program>>> = self.slot_progs.iter().map(|p| p.as_ref().map(|x| x.2.clone())).collect();
                let dir = std::path::Path::new(&self.root).join("textures");
                let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "modele".into());
                let t0 = std::time::Instant::now();
                // Modèle haute définition : même mise à l'échelle que le modèle ouvert
                let high = (!e.bake_high.trim().is_empty())
                    .then(|| {
                        let raw = std::path::PathBuf::from(e.bake_high.trim());
                        let path = if raw.is_absolute() { raw } else { std::path::Path::new(&self.root).join(raw) };
                        load(&path).ok().map(|m| {
                            let mesh = crate::modifiers::Mesh::from_model(&m, z_up);
                            (mesh.pos, mesh.tris)
                        })
                    })
                    .flatten();
                e.message = Some(match e.bake(&progs, &dir, &stem, high.as_ref()) {
                    Ok(files) => format!("{} textures écrites dans textures/ ({} px, {:.1} s)", files.len(), e.bake_size, t0.elapsed().as_secs_f32()),
                    Err(err) => format!("bake impossible : {err}"),
                });
            }
            self.slot_progs.resize(e.data.slots.len(), None);
            for (k, s) in e.data.slots.iter().enumerate() {
                if s.material.is_empty() {
                    if self.slot_progs[k].take().is_some() {
                        self.slot_rev += 1;
                    }
                    continue;
                }
                let path = std::path::PathBuf::from(&s.material);
                let mt = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                let fresh = self.slot_progs[k].as_ref().is_some_and(|(p, seen, _)| *p == path && *seen == mt);
                if !fresh {
                    if let Ok(prog) = crate::nodes::Graph::load(&path).and_then(|g| crate::nodes::Program::compile(&g, &self.root, 1024).map_err(|e| e.1)) {
                        self.slot_progs[k] = Some((path, mt, std::sync::Arc::new(prog)));
                        self.slot_rev += 1;
                    }
                }
            }
        }
        // Matériau en nodes : rechargé quand le fichier change (édité dans un autre onglet)
        if let Some((path, seen)) = &self.mat_active {
            let mt = std::fs::metadata(path).and_then(|m| m.modified()).ok();
            if mt != *seen {
                let path = path.clone();
                match crate::nodes::Graph::load(&path).and_then(|g| crate::nodes::Program::compile(&g, &self.root, 1024).map_err(|e| e.1)) {
                    Ok(prog) => {
                        self.look.nodes = Some(std::sync::Arc::new(prog));
                        self.mat_error = None;
                    }
                    Err(e) => self.mat_error = Some(e),
                }
                self.mat_active = Some((path, mt));
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(700));
        }
        let shown_tris = self.prepared.as_ref().map(|p| p.2.tri_count()).unwrap_or(0);
        let ctx = ui.ctx().clone();
        let pick_hdr: Option<std::path::PathBuf> = None;
        let mut do_capture = false;
        let mut export: Option<(&'static str, std::path::PathBuf)> = None;
        let mut ask_export: Option<&'static str> = None;
        let mut mod_request: Option<crate::modifiers::Request> = None;
        let mut do_gif = false;

        // ---------- barre d'outils : trois menus et quelques boutons ----------
        let stay = egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
        // ---------- en-tête de la vue, à la manière de Blender ----------
        // Les éléments viennent d'une liste : en ajouter un, c'est une ligne dans HEADER et un bras dans le match.
        let is_scene = crate::scene::is_scene(&self.path);
        let mut want_edit: Option<bool> = if std::mem::take(&mut self.start_edit) { Some(true) } else { None };
        // L'en-tête flotte par-dessus le haut de la vue, comme dans Blender : la vue garde toute la hauteur
        let vp = self.vp_rect.unwrap_or(ui.available_rect_before_wrap());
        let hdr_area = egui::Area::new(egui::Id::unique(("hdr3d", self.path.to_string_lossy().to_string()))).order(egui::Order::Middle).fixed_pos(vp.min + egui::vec2(6.0, 6.0)).show(ui.ctx(), |ui| {
        ui.set_max_width((vp.width() - 12.0).max(200.0));
        // Compact, comme l'en-tête de Blender
        ui.spacing_mut().button_padding = egui::vec2(5.0, 1.0);
        ui.spacing_mut().interact_size.y = 20.0;
        egui::Frame::new().fill(p.panel.gamma_multiply(0.94)).corner_radius(6).stroke(Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(5, 3)).show(ui, |ui| {
        egui::ScrollArea::horizontal().id_salt("hdr3d-scroll").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).max_width((vp.width() - 12.0).max(120.0)).show(ui, |ui| ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for item in HEADER {
                match item {
                    Hdr::Mode if !is_scene && !self.can_edit() => {
                        if ui.add(egui::Button::new(egui::RichText::new(format!("✚ {}", crate::i18n::t("ouvrir dans une scène"))).family(theme::bold()).size(12.0)).fill(p.panel)).on_hover_text(crate::i18n::t("Le modèle seul se regarde. Pour le modifier (édition, modificateurs, UV, retopo), on le place dans une scène.")).clicked() {
                            self.open_in_scene();
                        }
                    }
                    Hdr::Mode => {
                        let mode = if self.mesh.is_some() { "mode édition" } else if self.edit.is_some() { "UV et matériaux" } else { "mode objet" };
                        if let Some(me) = self.mesh.as_mut() {
                        // Sommets, arêtes, faces : les trois modes de sélection, comme dans Blender
                        for (k, (icon, tip)) in [("•", "sommets (1)"), ("⁄", "arêtes (2)"), ("▱", "faces (3)")].into_iter().enumerate() {
                            let on = me.mode == k as u8;
                            let b = egui::Button::new(egui::RichText::new(icon).size(13.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { p.panel }).min_size(egui::vec2(24.0, 20.0));
                            if ui.add(b).on_hover_text(crate::i18n::t(tip)).clicked() {
                                me.mode = k as u8;
                            }
                        }
                        let xb = egui::Button::new(egui::RichText::new("◫").size(13.0).color(if self.xray { p.on_accent } else { p.text })).fill(if self.xray { p.accent } else { p.panel }).min_size(egui::vec2(24.0, 20.0));
                        if ui.add(xb).on_hover_text(crate::i18n::t("rayons X (Alt+Z) : voir et choisir aussi les sommets cachés derrière le maillage")).clicked() {
                            self.xray = !self.xray;
                        }
                    }
                    egui::containers::menu::MenuButton::from_button(egui::Button::new(egui::RichText::new(format!("◆ {} ▾", crate::i18n::t(mode))).family(theme::bold()).size(12.0)).fill(p.panel)).ui(ui, |ui| {
                            ui.set_min_width(220.0);
                            if ui.selectable_label(self.mesh.is_none() && self.edit.is_none(), crate::i18n::t("mode objet")).clicked() {
                                if self.mesh.is_some() {
                                    self.mesh_toggle = true;
                                }
                                want_edit = Some(false);
                                ui.close();
                            }
                            if is_scene {
                                if ui.selectable_label(self.mesh.is_some(), format!("{}   Tab", crate::i18n::t("mode édition"))).on_hover_text(crate::i18n::t("Sommets, extrusion, faces de l'objet choisi")).clicked() {
                                    if self.mesh.is_none() {
                                        self.mesh_toggle = true;
                                    }
                                    ui.close();
                                }
                                if ui.selectable_label(false, crate::i18n::t("UV, matériaux et retopo")).on_hover_text(crate::i18n::t("IDs de matériau, coutures, UV et retopo de l'objet choisi")).clicked() {
                                    self.uv_request = true;
                                    ui.close();
                                }
                            } else if ui.selectable_label(self.edit.is_some(), crate::i18n::t("UV et matériaux")).on_hover_text(crate::i18n::t("IDs de matériau, coutures, UV, retopo")).clicked() {
                                want_edit = Some(true);
                                ui.close();
                            }
                        });
                    }
                    Hdr::View => {
                        egui::containers::menu::MenuButton::new(crate::i18n::t("vue")).config(stay.clone()).ui(ui, |ui| {
                            ui.set_min_width(220.0);
                            let half = std::f32::consts::FRAC_PI_2;
                            for (label, yaw, pitch) in [("face (1)", 0.0, 0.0), ("arrière (Maj+1)", std::f32::consts::PI, 0.0), ("droite (3)", -half, 0.0), ("gauche (Maj+3)", half, 0.0), ("dessus (7)", 0.0, half), ("dessous (Maj+7)", 0.0, -half)] {
                                if ui.button(crate::i18n::t(label)).clicked() {
                                    self.cam.yaw = yaw;
                                    self.cam.pitch = pitch;
                                }
                            }
                            ui.separator();
                            if ui.button(crate::i18n::t("cadrer (Origine ou .)")).clicked() {
                                self.cam = Camera { ortho: self.cam.ortho, ..Default::default() };
                            }

                        let mut z = z_up;
                        if ui.checkbox(&mut z, crate::i18n::t("axe Z vers le haut")).changed() {
                            self.z_up = Some(z);
                        }
                        ui.checkbox(&mut self.cam.ortho, crate::i18n::t("vue orthographique (5)"));
                            ui.checkbox(&mut self.turntable, crate::i18n::t("rotation automatique (platine)"));
                        });
                    }
                    Hdr::Select if is_scene => {
                        egui::containers::menu::MenuButton::new(crate::i18n::t("sélection")).ui(ui, |ui| {
                            ui.set_min_width(200.0);
                            for (label, key, m) in [("tout   A", Key::A, egui::Modifiers::NONE), ("rien   Alt+A", Key::A, egui::Modifiers::ALT), ("rectangle   B", Key::B, egui::Modifiers::NONE)] {
                                if ui.button(crate::i18n::t(label)).clicked() {
                                    self.hdr_key = Some((key, m));
                                    ui.close();
                                }
                            }
                        });
                    }
                    Hdr::Add if is_scene => {
                        egui::containers::menu::MenuButton::new(crate::i18n::t("ajouter")).ui(ui, |ui| {
                            ui.set_min_width(200.0);
                            ui.label(egui::RichText::new(crate::i18n::t("FORMES")).size(10.0).color(p.muted));
                            for (id, label) in crate::scene::PRIMITIVES {
                                if ui.button(crate::i18n::t(label)).clicked() {
                                    self.hdr_add = Some(id);
                                    ui.close();
                                }
                            }
                            ui.separator();
                            for (label, id) in [("lumière soleil", "light"), ("lumière ponctuelle", "light-point"), ("spot", "light-spot"), ("lumière rectangle", "light-area"), ("caméra", "camera"), ("groupe vide", "empty")] {
                                if ui.button(crate::i18n::t(label)).clicked() {
                                    self.hdr_add = Some(id);
                                    ui.close();
                                }
                            }
                            ui.separator();
                            ui.label(egui::RichText::new(crate::i18n::t("Maj+A dans la vue : le même menu sous le curseur")).size(10.5).color(p.muted));
                        });
                    }
                    Hdr::Object => {
                        egui::containers::menu::MenuButton::new(crate::i18n::t("objet")).ui(ui, |ui| {
                            ui.set_min_width(230.0);
                            if is_scene {
                                for (label, key, m) in [
                                    ("dupliquer   Maj+D", Key::D, egui::Modifiers::SHIFT),
                                    ("supprimer   X", Key::X, egui::Modifiers::NONE),
                                    ("cacher   H", Key::H, egui::Modifiers::NONE),
                                    ("tout afficher   Alt+H", Key::H, egui::Modifiers::ALT),
                                    ("lier au parent actif   P", Key::P, egui::Modifiers::NONE),
                                    ("retirer le parent   Alt+P", Key::P, egui::Modifiers::ALT),
                                    ("poser une clé   I", Key::I, egui::Modifiers::NONE),
                                ] {
                                    if ui.button(crate::i18n::t(label)).clicked() {
                                        self.hdr_key = Some((key, m));
                                        ui.close();
                                    }
                                }
                                ui.separator();
                            }
                            if (is_scene || self.can_edit()) && ui.button(crate::i18n::t("modificateurs…")).clicked() {
                                self.show_mods = true;
                                self.edit = None;
                                self.prepared = None;
                                ui.close();
                            }
                            ui.menu_button(crate::i18n::t("capture"), |ui| {
                                if ui.button(crate::i18n::t("image PNG (2400 px)")).clicked() {
                                    do_capture = true;
                                }
                                if is_scene && ui.button(crate::i18n::t("animation en vidéo (MP4)…")).on_hover_text(crate::i18n::t("Chaque image de la scène, du début à la fin, vue par sa caméra (ou par la vue actuelle s'il n'y en a pas)")).clicked() {
                                    self.want_anim = true;
                                }
                                if ui.button(crate::i18n::t("turntable GIF (48 images)")).clicked() {
                                    do_gif = true;
                                }
                                ui.label(egui::RichText::new(crate::i18n::t("enregistré à côté du modèle")).color(p.muted).size(10.5));
                            });
                            if ui.button(crate::i18n::t("exporter…")).clicked() {
                                ask_export = Some("obj");
                                ui.close();
                            }
                        });
                    }
                    Hdr::Tools if is_scene => {
                        ui.separator();
                        for (k, (icon, label, key)) in [("✥", "déplacer", "G"), ("⟳", "tourner", "R"), ("⤡", "taille", "S")].iter().enumerate() {
                            let on = self.gizmo_mode == k as u8;
                            let b = egui::Button::new(egui::RichText::new(*icon).size(14.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { p.panel }).min_size(egui::vec2(26.0, 22.0));
                            if ui.add(b).on_hover_text(format!("{} ({key})", crate::i18n::t(label))).clicked() {
                                self.gizmo_mode = k as u8;
                            }
                        }
                    }
                    Hdr::Fill => {
                        ui.separator();
                    }
                    Hdr::Shading => {
                        // Ombrage : une pastille par rendu, comme les sphères de Blender ; le nom au survol
                        use crate::icons::I;
                        for ((m, l), icon) in Material::ALL.into_iter().zip([I::ShadeSolid, I::ShadeFlat, I::ShadeWhite, I::ShadeChrome, I::ShadeNormals, I::ShadeColor]) {
                            let on = self.look.material == m && self.mat_active.is_none();
                            if crate::icons::button(ui, icon, on, 22.0, &crate::i18n::t(l)).clicked() {
                                self.look.material = m;
                                self.mat_active = None;
                                self.look.nodes = None;
                            }
                        }
                        // Moteur de rendu de la vue
                        let traced = self.engine == 1;
                        egui::containers::menu::MenuButton::from_button(egui::Button::new(egui::RichText::new(crate::i18n::t("rendu ▾")).size(11.0).color(if traced { p.on_accent } else { p.text })).fill(if traced { p.accent } else { p.panel })).config(stay.clone()).ui(ui, |ui| {
                            ui.set_min_width(260.0);
                            ui.label(egui::RichText::new(crate::i18n::t("moteur de la vue")).color(p.muted).size(11.0));
                            if ui.radio(self.engine == 0, crate::i18n::t("aperçu (rapide)")).on_hover_text(crate::i18n::t("Comme Workbench / Eevee : instantané, ombres et reflets approchés")).clicked() {
                                self.engine = 0;
                                self.trace = None;
                                self.last = None;
                            }
                            if ui.radio(self.engine == 1, crate::i18n::t("tracé de chemins (réaliste)")).on_hover_text(crate::i18n::t("Comme Cycles : la lumière rebondit, l'image s'affine quand la vue ne bouge plus")).clicked() {
                                self.engine = 1;
                            }
                            if self.engine == 1 {
                                ui.separator();
                                let mut ch = false;
                                ch |= ui.add(egui::Slider::new(&mut self.trace_set.bounces, 1..=16).clamping(egui::SliderClamping::Never).text(crate::i18n::t("rebonds"))).changed();
                                ch |= ui.add(egui::Slider::new(&mut self.trace_max, 16..=4096).clamping(egui::SliderClamping::Never).logarithmic(true).text(crate::i18n::t("échantillons"))).changed();
                                ch |= ui.checkbox(&mut self.trace_set.floor, crate::i18n::t("sol (ombres et rebonds)")).changed();
                                ch |= ui.add_enabled(self.trace_set.floor, egui::Checkbox::new(&mut self.trace_set.floor_visible, crate::i18n::t("sol visible"))).changed();
                                ch |= ui.add(egui::Slider::new(&mut self.trace_set.clamp, 1.0..=100.0).clamping(egui::SliderClamping::Never).logarithmic(true).text(crate::i18n::t("limite des points brillants"))).changed();
                                ch |= ui.checkbox(&mut self.trace_set.denoise, crate::i18n::t("débruitage")).changed();
                                ui.checkbox(&mut self.trace_set.live, crate::i18n::t("tracé de chemins pendant le mouvement")).on_hover_text(crate::i18n::t("Décoché : l'aperçu rapide prend le relais quand on tourne autour de la scène."));
                                ch |= ui.add(egui::Slider::new(&mut self.trace_set.aperture, 0.0..=0.2).text(crate::i18n::t("flou de profondeur"))).on_hover_text(crate::i18n::t("Ouverture de l'objectif : 0 tout est net")).changed();
                                if self.trace_set.aperture > 0.0 {
                                    let mut f = if self.trace_set.focus > 0.0 { self.trace_set.focus } else { self.cam.dist };
                                    if ui.add(egui::Slider::new(&mut f, 0.05..=10.0).clamping(egui::SliderClamping::Never).logarithmic(true).text(crate::i18n::t("distance nette"))).changed() {
                                        self.trace_set.focus = f;
                                        ch = true;
                                    }
                                }
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(crate::i18n::t("couleurs")).size(11.0).color(p.muted));
                                    ch |= ui.radio_value(&mut self.trace_set.agx, true, "AgX").changed();
                                    ch |= ui.radio_value(&mut self.trace_set.agx, false, crate::i18n::t("filmique")).changed();
                                });
                                // Carte graphique : bien plus rapide quand il y en a une
                                if gputrace::hardware() {
                                    ch |= ui.checkbox(&mut self.trace_set.gpu, format!("{} : {}", crate::i18n::t("carte graphique"), gputrace::device_name().unwrap_or_default())).changed();
                                } else {
                                    ui.label(egui::RichText::new(crate::i18n::t("pas de carte graphique utilisable : calcul sur le processeur")).size(10.5).color(p.muted));
                                }
                                // Débruiteur par IA (Intel Open Image Denoise), téléchargé une fois
                                if crate::oidn::installed() {
                                    ui.label(egui::RichText::new(crate::i18n::t("débruiteur IA : Intel Open Image Denoise")).size(10.5).color(p.muted));
                                } else {
                                    match &self.oidn_install {
                                        None => {
                                            if ui.button(crate::i18n::t("installer le débruiteur IA (~65 Mo)")).on_hover_text(crate::i18n::t("Intel Open Image Denoise, le débruiteur de Cycles, Arnold et V-Ray. Sans droits admin.")).clicked() {
                                                self.oidn_install = Some(crate::oidn::install(ui.ctx().clone()));
                                            }
                                        }
                                        Some(j) => {
                                            let st = j.lock().map(|s| s.clone()).unwrap_or_default();
                                            if let Some(e) = &st.error {
                                                ui.label(egui::RichText::new(e).color(p.missing).size(10.5));
                                            } else if st.done {
                                                self.oidn_install = None;
                                                ch = true;
                                            } else {
                                                ui.horizontal(|ui| {
                                                    ui.spinner();
                                                    ui.label(egui::RichText::new(&st.message).size(10.5));
                                                });
                                            }
                                        }
                                    }
                                }
                                if ch {
                                    self.trace = None;
                                }
                            }
                            ui.separator();
                            if ui.button(crate::i18n::t("rendre l'image…   F12")).on_hover_text(crate::i18n::t("Rendu final en tracé de chemins, à la taille choisie, puis enregistrement en PNG")).clicked() {
                                self.render_dialog = Some((1920, 1080, 256));
                                ui.close();
                            }
                        });
                        let nodes_on = self.mat_active.is_some();
                        egui::containers::menu::MenuButton::from_button(egui::Button::new(egui::RichText::new(crate::i18n::t("nodes ▾")).size(11.0).color(if nodes_on { p.on_accent } else { p.text })).fill(if nodes_on { p.accent } else { p.panel })).config(stay.clone()).ui(ui, |ui| {

                            ui.set_min_width(240.0);
                            ui.label(egui::RichText::new(crate::i18n::t("matériaux en nodes")).color(p.muted).size(11.0));
                            for (name, path) in self.mat_files.clone() {
                                let on = self.mat_active.as_ref().is_some_and(|m| m.0 == path);
                                ui.horizontal(|ui| {
                                    if ui.radio(on, name).clicked() {
                                        self.mat_active = Some((path.clone(), None));
                                    }
                                    if ui.small_button(crate::i18n::t("éditer")).on_hover_text(crate::i18n::t("ouvrir les nodes à côté du modèle")).clicked() {
                                        self.mat_active = Some((path.clone(), None));
                                        self.open_request = Some(path.clone());
                                        ui.close();
                                    }
                                });
                            }
                            if ui.button(crate::i18n::t("＋ nouveau matériau…")).on_hover_text(crate::i18n::t("crée un matériau en nodes, l'applique et l'ouvre à côté")).clicked() && !self.root.is_empty() {
                                let dir = std::path::Path::new(&self.root).join("nodes");
                                let _ = std::fs::create_dir_all(&dir);
                                let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Matériau".into());
                                let mut path = dir.join(format!("{stem}.{}", crate::formats::ext("mat")));
                                let mut n = 2;
                                while path.exists() {
                                    path = dir.join(format!("{stem} {n}.{}", crate::formats::ext("mat")));
                                    n += 1;
                                }
                                match crate::nodes::Graph::new_material().save(&path) {
                                    Ok(()) => {
                                        let name = path.file_name().map(|s| s.to_string_lossy().to_string()).map(|n| crate::formats::strip(&n).to_string()).unwrap_or_default();
                                        self.mat_files.push((name, path.clone()));
                                        self.mat_active = Some((path.clone(), None));
                                        self.open_request = Some(path);
                                    }
                                    Err(e) => self.mat_error = Some(e),
                                }
                                ui.close();
                            }
                            if let Some(e) = &self.mat_error {
                                ui.label(egui::RichText::new(e).color(p.missing).size(10.5));
                            }
                        });
                    }
                    Hdr::Overlays => {
                        if !self.is_scene() {
                            egui::containers::menu::MenuButton::new(crate::i18n::t("superpositions")).config(stay.clone()).ui(ui, |ui| {
                                ui.set_min_width(240.0);
                                overlays_ui(&mut self.look, ui);
                            });
                        }
                    }
                    Hdr::Light => {
                        if !self.is_scene() {
                            egui::containers::menu::MenuButton::new(crate::i18n::t("éclairage")).config(stay.clone()).ui(ui, |ui| {
                                ui.set_min_width(280.0);
                                lighting_ui(&mut self.look, &self.hdr_files, self.env_loading.is_some(), &self.env_error, &mut self.pending_hdr, ui);
                            });
                        }
                    }
                    Hdr::Modifiers if self.is_scene() || !self.can_edit() => {}
                    Hdr::Modifiers => {
                        let n_mods = self.stack.iter().filter(|m| m.on).count();
                        let label = if n_mods > 0 { format!("{} ({n_mods})", crate::i18n::t("modificateurs")) } else { crate::i18n::t("modificateurs") };
                        if ui.toggle_value(&mut self.show_mods, label).on_hover_text(crate::i18n::t("Pile de modificateurs, comme dans Blender et 3ds Max")).changed() && self.show_mods {
                            self.edit = None;
                            self.prepared = None;
                        }
                    }
                    Hdr::Busy => {
                        if self.job.is_some() {
                            ui.spinner();
                        }
                    }
                    _ => {}
                }
            }
        }));
        });
        });
        self.hdr_h = hdr_area.response.rect.height();
        if let Some(on) = want_edit {
            let editing = on;
            if editing != self.edit.is_some() {
                    if editing {
                        let mesh = self.applied.as_ref().map(|a| (**a).clone()).unwrap_or_else(|| crate::modifiers::Mesh::from_model(model, z_up));
                        let s = model.size();
                        let longest = s[0].max(s[1]).max(s[2]).max(1e-9);
                        let file = std::path::Path::new(&self.root).join(".app/meshdata").join(self.mods_file().file_name().unwrap_or_default());
                        self.edit = Some(crate::uvedit::Edit::open(&mesh, file, longest));
                        self.edit_seen = 0;
                        self.show_mods = false;
                    } else {
                        self.edit = None;
                        self.prepared = None;
                    }
            }
        }
        let stats = {
            let s = model.size();
            let shown = if shown_tris != model.total_tris { format!(" · affiché : {}", fmt_n(shown_tris)) } else { String::new() };
            if model.total_vertices <= 2 {
                crate::i18n::t("scène vide")
            } else {
                // Dimensions avec Z vers le haut, dans l'unité choisie
                let (un, uf) = crate::settings::unit();
                format!("{} triangles{shown} · {} sommets · {:.2} × {:.2} × {:.2} {un}", fmt_n(model.total_tris), fmt_n(model.total_vertices), s[0] * uf, s[2] * uf, s[1] * uf)
            }
        };
        self.stats_text = stats;
        if let Some(path) = pick_hdr.or(self.pending_hdr.take()) {
            self.load_env(path, &ctx);
        }
        if do_capture {
            self.capture();
        }
        if do_gif {
            self.turntable_gif();
        }
        if std::mem::take(&mut self.want_anim) {
            self.render_animation(&ctx);
        }
        // Rendu d'animation en cours : où il en est
        if let Some(job) = &self.anim_job {
            let done = job.done.load(std::sync::atomic::Ordering::Relaxed);
            let err = job.error.lock().ok().and_then(|e| e.clone());
            if let Some(e) = err {
                self.capture_msg = Some(e);
                self.anim_job = None;
            } else if job.finished.load(std::sync::atomic::Ordering::Relaxed) {
                self.capture_msg = Some(format!("{} {}", crate::i18n::t("animation enregistrée :"), job.out.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
                self.anim_job = None;
            } else {
                self.capture_msg = Some(format!("{} {done} / {}", crate::i18n::t("rendu de l'animation…"), job.total));
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }

        // Panneaux de droite : de vrais panneaux qui prennent leur place, la vue occupe ce qui reste
        let side_frame = egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).inner_margin(8);
        if let Some(e) = &mut self.edit {
            egui::Panel::right("model-edit-panel").resizable(true).default_size(430.0).min_size(300.0).max_size((ui.available_width() * 0.5).max(320.0)).frame(side_frame).show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("edit-panel").show(ui, |ui| {
                    e.panel(ui);
                });
            });
        }
        if self.show_mods {
            let mut changed = false;
            {
                egui::Panel::right("model-mods-panel").resizable(true).default_size(290.0).min_size(240.0).max_size((ui.available_width() * 0.45).max(260.0)).frame(side_frame).show(ui, |ui| {
                    egui::ScrollArea::vertical().id_salt("mods").show(ui, |ui| {
                        changed = crate::modifiers::panel(ui, &mut self.stack, self.applied.is_some(), &mut mod_request);
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(crate::i18n::t("exporter le résultat :")).size(11.0));
                            for f in crate::modifiers::EXPORT_FORMATS {
                                if ui.small_button(*f).clicked() {
                                    ask_export = Some(*f);
                                }
                            }
                        });
                    });
                });
            }
            if changed && !self.root.is_empty() {
                let f = self.mods_file();
                let _ = std::fs::create_dir_all(f.parent().unwrap());
                let _ = crate::vault::atomic_write(f, serde_json::to_string(&self.stack).unwrap_or_default());
            }
        }
        // Annuler / rétablir dans la pile (hors édition des UV, qui a son propre historique)
        if self.stack_loaded && self.edit.is_none() {
            let (z, y, down) = ui.input(|i| (i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::Z), i.modifiers.command && (i.modifiers.shift && i.key_pressed(Key::Z) || i.key_pressed(Key::Y)), i.pointer.any_down()));
            if self.stack_saved.is_none() {
                self.stack_saved = Some(self.stack.clone());
            }
            if !down && self.stack_saved.as_ref() != Some(&self.stack) {
                if let Some(prev) = self.stack_saved.replace(self.stack.clone()) {
                    self.stack_hist.push(prev);
                    if self.stack_hist.len() > 100 {
                        self.stack_hist.remove(0);
                    }
                    self.stack_redo.clear();
                }
            }
            if !ui.ctx().egui_wants_keyboard_input() && (z || y) {
                let (from, to) = if y { (&mut self.stack_redo, &mut self.stack_hist) } else { (&mut self.stack_hist, &mut self.stack_redo) };
                if let Some(prev) = from.pop() {
                    to.push(std::mem::replace(&mut self.stack, prev));
                    self.stack_saved = Some(self.stack.clone());
                    if !self.root.is_empty() {
                        let f = self.mods_file();
                        let _ = std::fs::create_dir_all(f.parent().unwrap());
                        let _ = crate::vault::atomic_write(f, serde_json::to_string(&self.stack).unwrap_or_default());
                    }
                }
            }
        }
        if let Some(e) = &mut self.edit {
            e.retopo_poll();
        }
        if let Some(target) = self.save_request.take() {
            self.write_model(model, z_up, target);
        }
        if let (Some(e), Some(model)) = (&mut self.edit, Some(&*model)) {
            if std::mem::take(&mut e.retopo_export) {
                if let Some(r) = &e.retopo {
                    let s = model.size();
                    let longest = s[0].max(s[1]).max(s[2]).max(1e-9);
                    let c = [(model.min[0] + model.max[0]) / 2.0, (model.min[1] + model.max[1]) / 2.0, (model.min[2] + model.max[2]) / 2.0];
                    let world = move |p: V3| -> V3 {
                        let o = if z_up { [p[0], -p[2], p[1]] } else { p };
                        [o[0] * longest + c[0], o[1] * longest + c[1], o[2] * longest + c[2]]
                    };
                    let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "modele".into());
                    let out = self.path.with_file_name(format!("{stem}-retopo.obj"));
                    self.capture_msg = Some(match crate::retopo::export_obj(r, &world, &out) {
                        Ok(()) => {
                            self.open_request = Some(out.clone());
                            format!("retopo exportée : {}", out.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
                        }
                        Err(e) => format!("export impossible : {e}"),
                    });
                }
            }
        }
        // Appliquer des modificateurs : leur résultat devient le maillage de base
        if let Some(req) = mod_request.take() {
            let file = self.mods_file().with_extension("mesh");
            match req {
                crate::modifiers::Request::Restore => {
                    let _ = std::fs::remove_file(&file);
                    self.applied = None;
                    self.capture_msg = Some(crate::i18n::t("maillage du fichier rétabli"));
                }
                _ => {
                    let upto = match req {
                        crate::modifiers::Request::ApplyUpTo(i) => (i + 1).min(self.stack.len()),
                        _ => self.stack.len(),
                    };
                    let s = model.size();
                    let longest = s[0].max(s[1]).max(s[2]).max(1e-9);
                    let c = [(model.min[0] + model.max[0]) / 2.0, (model.min[1] + model.max[1]) / 2.0, (model.min[2] + model.max[2]) / 2.0];
                    let o = [-c[0] / longest, -c[1] / longest, -c[2] / longest];
                    let origin = if z_up { [o[0], o[2], -o[1]] } else { o };
                    let base = self.applied.as_ref().map(|a| (**a).clone()).unwrap_or_else(|| crate::modifiers::Mesh::from_model(model, z_up));
                    let mesh = crate::modifiers::apply(&self.stack[..upto], base, origin);
                    let n = self.stack[..upto].iter().filter(|m| m.on).count();
                    self.capture_msg = Some(match mesh.save(&file) {
                        Ok(()) => {
                            // Les modificateurs appliqués quittent la pile ; ceux qui étaient désactivés restent
                            let mut k = 0;
                            self.stack.retain(|m| {
                                k += 1;
                                k > upto || !m.on
                            });
                            self.applied = Some(std::sync::Arc::new(mesh));
                            format!("{n} {}", crate::i18n::t_mid("modificateur(s) appliqué(s)"))
                        }
                        Err(e) => format!("{} : {e}", crate::i18n::t("application impossible")),
                    });
                }
            }
            self.applied_ver += 1;
            self.edit = None;
            self.prepared = None;
            self.job = None;
            let f = self.mods_file();
            let _ = std::fs::create_dir_all(f.parent().unwrap());
            let _ = crate::vault::atomic_write(f, serde_json::to_string(&self.stack).unwrap_or_default());
            ui.ctx().request_repaint();
        }
        // Fenêtre d'export : la liste des formats, puis l'endroit où enregistrer
        if let Some(fmt) = ask_export.take() {
            let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "modele".into());
            let suffix = if self.stack.iter().any(|m| m.on) || self.applied.is_some() { "-modifié" } else { "-export" };
            let dir = self.export_dir.clone().or_else(|| self.path.parent().map(|p| p.to_path_buf())).unwrap_or_default();
            self.export_dlg = Some((fmt, dir.join(format!("{stem}{suffix}.{fmt}")).to_string_lossy().to_string()));
        }
        if let Some((fmt, path)) = &mut self.export_dlg {
            let mut close = false;
            let mut go = false;
            egui::Window::new(crate::i18n::t("exporter le modèle")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ui.ctx(), |ui| {
                ui.set_min_width(460.0);
                ui.label(crate::theme::section(&crate::i18n::t("FORMAT")));
                for (f, help) in [("obj", "OBJ : quads et n-gones gardés"), ("fbx", "FBX : Maya, 3ds Max, Blender, Unity, Unreal (quads, UV, matériaux)"), ("glb", "glTF binaire : Blender, Unity, Unreal, web"), ("stl", "STL binaire : impression 3D"), ("ply", "PLY : scans, MeshLab")] {
                    ui.horizontal(|ui| {
                        if ui.radio(*fmt == f, egui::RichText::new(f.to_uppercase()).family(theme::bold())).clicked() && *fmt != f {
                            // Le chemin suit le format choisi
                            let mut pb = std::path::PathBuf::from(path.as_str());
                            pb.set_extension(f);
                            *path = pb.to_string_lossy().to_string();
                            *fmt = f;
                        }
                        ui.label(egui::RichText::new(crate::i18n::t(help)).color(p.muted).size(11.0));
                    });
                }
                ui.add_space(6.0);
                ui.label(crate::theme::section(&crate::i18n::t("EMPLACEMENT")));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(path).desired_width(350.0));
                    if ui.button(crate::i18n::t("parcourir…")).clicked() {
                        let cur = std::path::PathBuf::from(path.as_str());
                        let mut d = rfd::FileDialog::new().add_filter(fmt.to_uppercase(), &[*fmt]);
                        if let Some(dir) = cur.parent().filter(|d| d.is_dir()) {
                            d = d.set_directory(dir);
                        }
                        if let Some(name) = cur.file_name() {
                            d = d.set_file_name(name.to_string_lossy());
                        }
                        if let Some(mut picked) = d.save_file() {
                            if picked.extension().is_none() {
                                picked.set_extension(*fmt);
                            }
                            *path = picked.to_string_lossy().to_string();
                        }
                    }
                });
                ui.label(egui::RichText::new(crate::i18n::t("Les modificateurs actifs sont inclus dans le fichier.")).color(p.muted).size(10.5));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(egui::RichText::new(crate::i18n::t("exporter")).color(p.on_accent)).fill(p.accent)).clicked() && !path.trim().is_empty() {
                        go = true;
                    }
                    if ui.button(crate::i18n::t("annuler")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        close = true;
                    }
                });
            });
            if go {
                let mut out = std::path::PathBuf::from(path.trim());
                if out.extension().map(|e| e.to_string_lossy().to_lowercase()) != Some(fmt.to_string()) {
                    out.set_extension(*fmt);
                }
                export = Some((*fmt, out));
                close = true;
            }
            if close {
                self.export_dlg = None;
            }
        }
        if let Some((fmt, out)) = export.take() {
            if let Some(d) = out.parent() {
                let _ = std::fs::create_dir_all(d);
                self.export_dir = Some(d.to_path_buf());
            }
            let s = model.size();
            let longest = s[0].max(s[1]).max(s[2]).max(1e-9);
            let c = [(model.min[0] + model.max[0]) / 2.0, (model.min[1] + model.max[1]) / 2.0, (model.min[2] + model.max[2]) / 2.0];
            let o = [-c[0] / longest, -c[1] / longest, -c[2] / longest];
            let origin = if z_up { [o[0], o[2], -o[1]] } else { o };
            let base = self.applied.as_ref().map(|a| (**a).clone()).unwrap_or_else(|| crate::modifiers::Mesh::from_model(model, z_up));
            let mesh = crate::modifiers::apply(&self.stack, base, origin);
            let s = model.size();
            let longest = s[0].max(s[1]).max(s[2]).max(1e-9);
            let c = [(model.min[0] + model.max[0]) / 2.0, (model.min[1] + model.max[1]) / 2.0, (model.min[2] + model.max[2]) / 2.0];
            let world = move |p: V3| -> V3 {
                let o = if z_up { [p[0], -p[2], p[1]] } else { p };
                [o[0] * longest + c[0], o[1] * longest + c[1], o[2] * longest + c[2]]
            };
            let with_edit = self.edit.as_ref().filter(|_| fmt == "obj" || fmt == "glb" || fmt == "fbx");
            self.capture_msg = Some(match with_edit.map(|e| match fmt {
                "obj" => crate::uvedit::export_obj(e, &world, &out),
                "fbx" => crate::fbxwrite::export(e, &world, &out),
                _ => crate::uvedit::export_glb(e, &world, &out),
            }).unwrap_or_else(|| crate::modifiers::export(&mesh, model, z_up, &out)) {
                Ok(()) => format!("exporté : {}", out.to_string_lossy()),
                Err(e) => format!("export impossible : {e}"),
            });
        }
        let avail = ui.available_size();
        let (full, _) = ui.allocate_exact_size(avail.max(egui::vec2(50.0, 50.0)), Sense::hover());
        let mut rect = full;
        // Onglet UV : l'éditeur UV à gauche de la vue 3D, comme l'espace « UV Editing » de Blender
        if let Some(e) = self.edit.as_mut().filter(|e| e.tab == 2 && !e.tools.no_split && full.width() > 500.0) {
            let w = (full.width() * 0.45).round();
            let uv_rect = egui::Rect::from_min_size(full.min, egui::vec2(w - 3.0, full.height()));
            e.uv_editor(ui, uv_rect);
            ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(full.min.x + w - 3.0, full.min.y), egui::pos2(full.min.x + w, full.max.y)), 0.0, crate::theme::gutter());
            rect = egui::Rect::from_min_max(egui::pos2(full.min.x + w, full.min.y), full.max);
        }
        let resp = ui.interact(rect, ui.scope_id().with("viewport-3d"), Sense::click_and_drag());
        if let Some(e) = self.edit.as_mut() {
            // Raccourcis façon Blender (A, Alt+A, Ctrl+I, Ctrl+E, U, Ctrl+L) et menu du clic droit
            if resp.hovered() {
                e.keys_3d(ui, ui.input(|i| i.pointer.hover_pos()));
            }
            if resp.secondary_clicked() {
                if let Some(at) = resp.interact_pointer_pos() {
                    e.open_menu_3d(at);
                }
            }
            e.context_menu(ui.ctx());
        }
        // Scène, mode objet : clic droit sur un objet = il devient l'actif, et son menu s'ouvre (comme Blender)
        if self.edit.is_none() && self.mesh.is_none() && self.scene.is_some() {
            if resp.secondary_clicked() {
                if let (Some(at), Some((_, _, prep))) = (resp.interact_pointer_pos(), &self.prepared) {
                    let hit = pick(prep, &self.cam, rect.width(), rect.height(), at.x - rect.left(), at.y - rect.top());
                    if let Some(k) = hit.and_then(|(tri, _)| self.scene_ranges.iter().find(|r| (r.1..r.2).contains(&(tri as usize))).map(|r| r.0)) {
                        if self.scene_sel != Some(k) && !self.scene_multi.contains(&k) {
                            self.scene_multi.clear();
                        }
                        self.scene_sel = Some(k);
                    }
                    self.obj_menu = Some(at);
                }
            }
            if let Some(at) = self.obj_menu {
                let has = self.scene_sel.is_some();
                let mut act: Option<&'static str> = None;
                let area = egui::Area::new(egui::Id::unique("object-context-menu")).order(egui::Order::Foreground).fixed_pos(at).constrain(true).show(ui.ctx(), |ui| {
                    egui::Frame::menu(ui.style()).show(ui, |ui| {
                        ui.set_min_width(210.0);
                        let name = self.scene_sel.and_then(|k| self.scene.as_ref().and_then(|s| s.objects.get(k))).map(|o| o.name.clone()).unwrap_or_else(|| crate::i18n::t("objet"));
                        ui.label(egui::RichText::new(name).size(10.5).color(crate::theme::pal().muted));
                        for (label, key, id, needs) in [
                            ("mode édition", "Tab", "edit", true),
                            ("déplacer", "G", "move", true),
                            ("tourner", "R", "rotate", true),
                            ("redimensionner", "S", "scale", true),
                            ("remettre à zéro (position, rotation, taille)", "", "reset", true),
                            ("dupliquer", "Maj+D", "duplicate", true),
                            ("cacher", "H", "hide", true),
                            ("tout montrer", "Alt+H", "show-all", false),
                            ("retirer le matériau", "", "no-material", true),
                            ("supprimer", "X", "delete", true),
                        ] {
                            if matches!(id, "show-all" | "duplicate" | "no-material") {
                                ui.separator();
                            }
                            let text = if key.is_empty() { crate::i18n::t(label) } else { format!("{}    {key}", crate::i18n::t(label)) };
                            if ui.add_enabled(has || !needs, egui::Button::new(text)).clicked() {
                                act = Some(id);
                            }
                        }
                    });
                });
                let outside = ui.input(|i| i.pointer.any_pressed()) && ui.ctx().pointer_latest_pos().is_some_and(|p| !area.response.rect.contains(p));
                if act.is_some() || outside || ui.input(|i| i.key_pressed(Key::Escape)) {
                    self.obj_menu = None;
                }
                if act.is_some() {
                    self.obj_action = act;
                }
            }
        }
        // Un .hdr ou .exr glissé depuis l'explorateur devient l'éclairage de la vue
        if let Some(rel) = crate::drop_rel(&resp, |r| {
            let r = r.to_lowercase();
            r.ends_with(".hdr") || r.ends_with(".exr")
        }) {
            self.pending_hdr = Some(std::path::Path::new(&self.root).join(rel));
        }
        if self.vp_rect != Some(rect) {
            self.vp_rect = Some(rect);
            ui.ctx().request_repaint();
        }
        // Barre d'outils à gauche de la vue (scènes) : sélection, déplacer, tourner, taille, ajouter
        if crate::scene::is_scene(&self.path) && self.edit.is_none() {
            let top = rect.min + egui::vec2(6.0, self.hdr_h + 14.0);
            egui::Area::new(egui::Id::unique(("tools3d", self.path.to_string_lossy().to_string()))).order(egui::Order::Middle).fixed_pos(top).show(ui.ctx(), |ui| {
                egui::Frame::new().fill(p.panel.gamma_multiply(0.94)).corner_radius(6).stroke(Stroke::new(1.0, p.line)).inner_margin(3).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    // Des cases toutes de la même taille, alignées
                    let tool = |ui: &mut egui::Ui, icon: &str, on: bool, tip: String| {
                        let (r, resp) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
                        let fill = if on { p.accent } else if resp.hovered() { p.line } else { egui::Color32::TRANSPARENT };
                        ui.painter().rect_filled(r, 3.0, fill);
                        crate::icons::glyph(ui.painter(), r.shrink(6.0), icon, if on { p.on_accent } else { p.text });
                        resp.on_hover_text(tip).clicked()
                    };
                    if tool(ui, "⬚", self.box_sel.is_some(), format!("{} (B)", crate::i18n::t("sélection rectangle"))) {
                        self.hdr_key = Some((Key::B, egui::Modifiers::NONE));
                    }
                    ui.separator();
                    for (k, (icon, label, key)) in [("✥", "déplacer", "G"), ("⟳", "tourner", "R"), ("⤡", "taille", "S")].iter().enumerate() {
                        if tool(ui, icon, self.gizmo_mode == k as u8, format!("{} ({key})", crate::i18n::t(label))) {
                            self.gizmo_mode = k as u8;
                        }
                    }
                    ui.separator();
                    if tool(ui, "＋", self.add_popup.is_some(), format!("{} (Maj+A)", crate::i18n::t("ajouter"))) {
                        self.add_popup = Some(top + egui::vec2(40.0, 120.0));
                    }
                    if tool(ui, "⌂", false, format!("{} (Origine)", crate::i18n::t("cadrer la scène"))) {
                        self.cam = Camera { ortho: self.cam.ortho, ..Default::default() };
                    }
                });
            });
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, p.panel);

        // ---------- navigation façon Blender ----------
        let (mods, lkey, hkey) = ui.input(|i| (i.modifiers, i.key_down(Key::L), i.key_down(Key::H)));
        // Dans une scène, H cache l'objet choisi (comme dans Blender) : il ne fait plus tourner le HDRI
        let hkey = hkey && !crate::scene::is_scene(&self.path);
        let guiding = self.edit.as_ref().is_some_and(|e| e.tool == crate::uvedit::Tool::Guides) && !mods.shift;
        if guiding && resp.dragged_by(egui::PointerButton::Primary) {
            if let (Some(e), Some((_, _, prep)), Some(pt)) = (&mut self.edit, &self.prepared, resp.interact_pointer_pos()) {
                if let Some((tri, bary)) = pick(prep, &self.cam, rect.width(), rect.height(), pt.x - rect.left(), pt.y - rect.top()) {
                    e.guide_point(prep, tri, bary);
                }
            }
        }
        if guiding && resp.drag_stopped() {
            if let Some(e) = &mut self.edit {
                e.guide_end();
            }
        }
        // Scène : poignée de déplacement de l'objet choisi (trois axes à attraper)
        let mut gizmo: Option<([f32; 3], [[f32; 3]; 3])> = None;
        // Le clic qui valide une transformation au clavier ne doit pas aussi choisir un autre objet
        let mut modal_ended = false;
        let mut marker_pick: Option<usize> = None;
        let mut boxed_pick: Option<Vec<usize>> = None;
        if self.edit.is_none() && self.mesh.is_none() && crate::scene::is_scene(&self.path) {
            if resp.hovered() && !ui.ctx().egui_wants_keyboard_input() {
                // G, R, S : déplacer, tourner, redimensionner l'objet choisi à la souris, sans rien tenir
                if self.modal.is_none() {
                    let start = ui.input(|i| if i.modifiers.any() { None } else if i.key_pressed(Key::G) { Some(0u8) } else if i.key_pressed(Key::R) { Some(1) } else if i.key_pressed(Key::S) { Some(2) } else { None });
                    if let (Some(kind), Some(o), Some(at)) = (start, self.scene_sel.and_then(|k| self.scene.as_ref().and_then(|s| s.objects.get(k))), ui.input(|i| i.pointer.hover_pos())) {
                        self.modal = Some((kind, None, (o.pos, o.rot, o.scale), at));
                        self.gizmo_mode = kind;
                    }
                }
            }
            if let (Some((kind, axis, orig, from)), Some(sel), Some((lo, hi))) = (self.modal, self.scene_sel, self.scene_frame) {
                let longest = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
                let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
                let focal = rect.height().min(rect.width()) * 1.2;
                let cam = self.cam;
                // X, Y, Z imposent un axe (le même une seconde fois le libère)
                let mut axis = axis;
                // (Z est l'axe qui monte : l'axe interne 1)
                for (key, k) in [(Key::X, 0usize), (Key::Y, 2), (Key::Z, 1)] {
                    if ui.input(|i| i.key_pressed(key)) {
                        axis = if axis == Some(k) { None } else { Some(k) };
                    }
                }
                let (confirm, cancel, cur) = ui.input(|i| (i.pointer.primary_pressed() || i.key_pressed(Key::Enter), i.pointer.secondary_pressed() || i.key_pressed(Key::Escape), i.pointer.hover_pos().or(i.pointer.latest_pos())));
                let world = self.scene.as_ref().filter(|s| sel < s.objects.len()).map(|s| s.from_parent(sel, orig.0));
                if let (Some(scene), Some(cur), Some(world)) = (self.scene.as_mut(), cur, world) {
                    if let Some(o) = scene.objects.get_mut(sel) {
                        let q = cam.view([(world[0] - c[0]) / longest, (world[1] - c[1]) / longest, (world[2] - c[2]) / longest]);
                        let center = if cam.ortho { pos2(rect.center().x + q[0] * focal / cam.dist, rect.center().y - q[1] * focal / cam.dist) } else { pos2(rect.center().x + q[0] / (-q[2]).max(0.05) * focal, rect.center().y - q[1] / (-q[2]).max(0.05) * focal) };
                        let before = (o.pos, o.rot, o.scale);
                        (o.pos, o.rot, o.scale) = orig;
                        if cancel {
                            self.modal = None;
                        } else {
                            let d = cur - from;
                            match kind {
                                0 => {
                                    // Dans le plan de l'écran, à la profondeur de l'objet
                                    let per_px = if cam.ortho { cam.dist / focal } else { (-q[2]).max(0.05) / focal } * longest;
                                    let (right, up) = (cam.unrotate([1.0, 0.0, 0.0]), cam.unrotate([0.0, 1.0, 0.0]));
                                    let mv = [0, 1, 2].map(|k| (right[k] * d.x - up[k] * d.y) * per_px);
                                    match axis {
                                        Some(k) => o.pos[k] += mv[k],
                                        None => o.pos = [orig.0[0] + mv[0], orig.0[1] + mv[1], orig.0[2] + mv[2]],
                                    }
                                }
                                1 => {
                                    // Autour de l'axe imposé, sinon de celui qui regarde le plus vers la caméra
                                    let toward = cam.unrotate([0.0, 0.0, 1.0]);
                                    let k = axis.unwrap_or_else(|| (0..3).max_by(|a, b| toward[*a].abs().total_cmp(&toward[*b].abs())).unwrap_or(1));
                                    let (a0, a1) = ((from - center).angle(), (cur - center).angle());
                                    let mut turn = (a1 - a0).to_degrees();
                                    if turn > 180.0 {
                                        turn -= 360.0;
                                    } else if turn < -180.0 {
                                        turn += 360.0;
                                    }
                                    o.rot[k] = orig.1[k] - turn * if toward[k] >= 0.0 { 1.0 } else { -1.0 };
                                }
                                _ => {
                                    let f = ((cur - center).length() / (from - center).length().max(8.0)).max(0.001);
                                    match axis {
                                        Some(k) => o.scale[k] = orig.2[k] * f,
                                        None => o.scale = [orig.2[0] * f, orig.2[1] * f, orig.2[2] * f],
                                    }
                                }
                            }
                            self.modal = if confirm { None } else { Some((kind, axis, orig, from)) };
                        }
                        if before != (o.pos, o.rot, o.scale) {
                            self.scene_dirty = true;
                        }
                        if self.modal.is_none() {
                            if !cancel && self.anim_auto {
                                o.set_key(self.anim_frame.round() as i32);
                            }
                            let _ = scene.write(&self.path);
                            modal_ended = true;
                            self.modal_click = true;
                        }
                        ui.ctx().request_repaint();
                    }
                }
                // Axe imposé : une ligne de sa couleur traverse toute la vue, comme dans Blender
                if let (Some(k), Some(scene)) = (axis, self.scene.as_ref()) {
                    if sel < scene.objects.len() {
                        let w0 = scene.from_parent(sel, scene.objects[sel].pos);
                        let proj = |w: V3| -> Option<Pos2> {
                            let q = cam.view([(w[0] - c[0]) / longest, (w[1] - c[1]) / longest, (w[2] - c[2]) / longest]);
                            if cam.ortho {
                                Some(pos2(rect.center().x + q[0] * focal / cam.dist, rect.center().y - q[1] * focal / cam.dist))
                            } else {
                                (q[2] < -0.02).then(|| pos2(rect.center().x + q[0] / -q[2] * focal, rect.center().y - q[1] / -q[2] * focal))
                            }
                        };
                        let painter = ui.painter_at(rect);
                        let col = crate::gizmo::AXIS[k];
                        let mut last: Option<Pos2> = None;
                        for i in -60..=60 {
                            let mut w = w0;
                            w[k] += i as f32 / 60.0 * longest * 20.0 * (i as f32 / 60.0).abs();
                            let q = proj(w);
                            if let (Some(a), Some(b)) = (last, q) {
                                painter.line_segment([a, b], Stroke::new(1.5, col));
                            }
                            last = q;
                        }
                    }
                }
                // Rappel des touches, en bas de la vue
                let what = [crate::i18n::t("déplacer"), crate::i18n::t("tourner"), crate::i18n::t("taille")][kind.min(2) as usize].clone();
                let ax = axis.map(|k| format!(" · {} {}", crate::i18n::t("axe"), crate::gizmo::LETTER[k])).unwrap_or_default();
                ui.painter_at(rect).text(rect.left_bottom() + egui::vec2(12.0, -10.0), egui::Align2::LEFT_BOTTOM, format!("{what}{ax}   ·   X Y Z : {}   ·   {}   ·   {}", crate::i18n::t("imposer un axe"), crate::i18n::t("clic : valider"), crate::i18n::t("Échap : annuler")), egui::FontId::monospace(11.5), p.accent);
            }
            if let (Some(sel), Some((lo, hi)), Some(scene)) = (self.scene_sel, self.scene_frame, self.scene.as_mut()) {
                // Place de l'objet dans la scène et direction de ses axes, parents compris
                let placed = scene.objects.get(sel).map(|o| {
                    let base = scene.from_parent(sel, o.pos);
                    let axes = [0, 1, 2].map(|k| {
                        let mut q = o.pos;
                        q[k] += 1.0;
                        let e = scene.from_parent(sel, q);
                        let d = [e[0] - base[0], e[1] - base[1], e[2] - base[2]];
                        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-9);
                        ([d[0] / l, d[1] / l, d[2] / l], l)
                    });
                    (base, axes)
                });
                let own_axes = (sel < scene.objects.len()).then(|| scene_world_axes(scene, sel).map(|(d, _)| d));
                let shown = sel < scene.objects.len() && scene.shown(sel);
                if let (Some(o), Some((base, axes))) = (scene.objects.get_mut(sel).filter(|_| shown), placed) {
                    let longest = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
                    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
                    let origin = [(base[0] - c[0]) / longest, (base[1] - c[1]) / longest, (base[2] - c[2]) / longest];
                    let focal = rect.height().min(rect.width()) * 1.2;
                    let (ortho, dist, rc) = (self.cam.ortho, self.cam.dist, rect.center());
                    let cam = &self.cam;
                    let project = |v: V3| -> Option<Pos2> {
                        let q = cam.view(v);
                        if ortho {
                            Some(pos2(rc.x + q[0] * focal / dist, rc.y - q[1] * focal / dist))
                        } else {
                            (q[2] < -0.05).then(|| pos2(rc.x + q[0] / -q[2] * focal, rc.y - q[1] / -q[2] * focal))
                        }
                    };
                    let _ = (ortho, rc);
                    let mode = crate::gizmo::Mode::from_u8(self.gizmo_mode);
                    // Taille : selon les axes propres de l'objet ; déplacer et tourner : selon ceux de son parent
                    let gaxes = if mode == crate::gizmo::Mode::Scale {
                        own_axes.unwrap_or([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]])
                    } else {
                        axes.map(|x| x.0)
                    };
                    let (toward, right, up, per_px) = gizmo_basis(cam, origin, focal);
                    let rig = crate::gizmo::Rig { origin, axes: gaxes, len: per_px * 92.0, toward, right, up, per_px, project: &project };
                    if let Some(l) = rig.layout(mode) {
                        let pointer = ui.input(|i| i.pointer.hover_pos());
                        if self.gizmo_hold.is_none() {
                            self.gizmo_hot = if self.modal.is_none() && resp.hovered() { pointer.and_then(|p| l.hit(mode, p)) } else { None };
                        }
                        if resp.drag_started_by(egui::PointerButton::Primary) && self.modal.is_none() {
                            if let Some(at) = ui.input(|i| i.pointer.press_origin()) {
                                if let Some(h) = l.hit(mode, at) {
                                    self.gizmo_hold = Some((h, at, at, [0.0; 3]));
                                }
                            }
                        }
                        if let (Some((h, prev, start, mut acc)), true, Some(cur)) = (self.gizmo_hold, resp.dragged_by(egui::PointerButton::Primary), pointer) {
                            match crate::gizmo::drag(&rig, &l, mode, h, prev, cur) {
                                Some(crate::gizmo::Delta::Move(m)) => {
                                    let w = [m[0] * longest, m[1] * longest, m[2] * longest];
                                    for k in 0..3 {
                                        let dk = (w[0] * axes[k].0[0] + w[1] * axes[k].0[1] + w[2] * axes[k].0[2]) / axes[k].1;
                                        o.pos[k] += dk;
                                        acc[k] += dk;
                                    }
                                    self.scene_dirty = true;
                                }
                                Some(crate::gizmo::Delta::Rotate(ax, t)) => {
                                    let pa = [0, 1, 2].map(|k| ax[0] * axes[k].0[0] + ax[1] * axes[k].0[1] + ax[2] * axes[k].0[2]);
                                    crate::scene::rotate_by(o, pa, t);
                                    acc[0] += t;
                                    self.scene_dirty = true;
                                }
                                Some(crate::gizmo::Delta::Scale(f)) => {
                                    if acc == [0.0; 3] {
                                        acc = [1.0; 3];
                                    }
                                    for k in 0..3 {
                                        o.scale[k] = (o.scale[k] * f[k]).max(0.0005);
                                        acc[k] *= f[k];
                                    }
                                    self.scene_dirty = true;
                                }
                                None => {}
                            }
                            self.gizmo_hold = Some((h, cur, start, acc));
                        }
                        gizmo = Some((origin, gaxes));
                    }
                }
            }
            if self.gizmo_hold.is_some() && (resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down())) {
                self.gizmo_hold = None;
                if let Some(scene) = &mut self.scene {
                    if self.anim_auto {
                        if let Some(o) = self.scene_sel.and_then(|k| scene.objects.get_mut(k)) {
                            o.set_key(self.anim_frame.round() as i32);
                        }
                    }
                    let _ = scene.write(&self.path);
                }
            }
            // Clic sur un objet : il devient l'objet choisi (clic dans le vide : plus rien)
            let skip_pick = modal_ended || self.modal_click || self.modal.is_some();
            if !modal_ended && !ui.input(|i| i.pointer.any_down()) {
                self.modal_click = false;
            }
            // Matériau glissé depuis l'explorateur sur un objet : il le reçoit
            let is_mat = |r: &str| {
                let r = r.to_lowercase();
                r.ends_with(".arymat") || r.ends_with(".mat.json")
            };
            if let Some(pt) = ui.input(|i| i.pointer.latest_pos()).filter(|_| egui::DragAndDrop::payload::<String>(ui.ctx()).is_some_and(|r| is_mat(&r))) {
                if let Some((_, _, prep)) = &self.prepared {
                    let hit = pick(prep, &self.cam, rect.width(), rect.height(), pt.x - rect.left(), pt.y - rect.top());
                    let obj = hit.and_then(|(tri, _)| self.scene_ranges.iter().find(|r| (r.1..r.2).contains(&(tri as usize))).map(|r| r.0));
                    if let Some(k) = obj {
                        if let Some(rel) = crate::drop_rel(&resp, is_mat) {
                            if let Some(o) = self.scene.as_mut().and_then(|s| s.objects.get_mut(k)) {
                                o.material = rel;
                                self.scene_dirty = true;
                                self.scene_sel = Some(k);
                                self.slot_rev += 1;
                            }
                        }
                    }
                }
            }
            if resp.clicked() && !skip_pick && !self.stack.iter().any(|m| m.on) && self.applied.is_none() {
                if let (Some((_, _, prep)), Some(pt)) = (&self.prepared, resp.interact_pointer_pos()) {
                    let hit = pick(prep, &self.cam, rect.width(), rect.height(), pt.x - rect.left(), pt.y - rect.top());
                    let hit = hit.and_then(|(tri, _)| self.scene_ranges.iter().find(|r| (r.1..r.2).contains(&(tri as usize))).map(|r| r.0));
                    if ui.input(|i| i.modifiers.shift) {
                        // Maj + clic : ajoute l'objet à la sélection et en fait l'actif, ou le retire s'il y était
                        if let Some(k) = hit {
                            if self.scene_multi.is_empty() {
                                self.scene_multi.extend(self.scene_sel);
                            }
                            if let Some(at) = self.scene_multi.iter().position(|&m| m == k) {
                                self.scene_multi.remove(at);
                                if self.scene_sel == Some(k) {
                                    self.scene_sel = self.scene_multi.last().copied();
                                }
                            } else {
                                self.scene_multi.push(k);
                                self.scene_sel = Some(k);
                            }
                        }
                    } else {
                        self.scene_multi.clear();
                        self.scene_sel = hit;
                    }
                    self.multi_follow = None;
                }
            }
        }
        // Mode édition : la même poignée, posée au centre des sommets choisis, selon les axes de la scène
        if self.edit.is_none() && crate::scene::is_scene(&self.path) {
            if let (Some(me), Some((lo, hi)), Some(scene)) = (self.mesh.as_mut(), self.scene_frame, self.scene.as_ref()) {
                let obj = me.obj;
                if me.modal.is_none() && me.boxing.is_none() && obj < scene.objects.len() && me.sel.iter().any(|s| *s) {
                    let o0 = scene.world(obj, [0.0; 3]);
                    let cols = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(|e| sub(scene.world(obj, e), o0));
                    if let Some(inv) = crate::meshedit::inverse(cols) {
                        let to_world = |v: V3| add3(crate::meshedit::apply(&cols, v), o0);
                        let to_local = |w: V3| crate::meshedit::apply(&inv, sub(w, o0));
                        let longest = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
                        let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
                        let chosen: Vec<usize> = (0..me.mesh.pos.len()).filter(|&i| me.sel.get(i).copied().unwrap_or(false)).collect();
                        let n = chosen.len() as f32;
                        let cen = chosen.iter().fold([0.0f32; 3], |a, &i| add3(a, to_world(me.mesh.pos[i]).map(|x| x / n)));
                        let origin = [(cen[0] - c[0]) / longest, (cen[1] - c[1]) / longest, (cen[2] - c[2]) / longest];
                        let focal = rect.height().min(rect.width()) * 1.2;
                        let project = projector(self.cam, rect);
                        let (toward, right, up, per_px) = gizmo_basis(&self.cam, origin, focal);
                        let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
                        let mode = crate::gizmo::Mode::from_u8(self.gizmo_mode);
                        let rig = crate::gizmo::Rig { origin, axes, len: per_px * 92.0, toward, right, up, per_px, project: &project };
                        if let Some(l) = rig.layout(mode) {
                            let pointer = ui.input(|i| i.pointer.hover_pos());
                            if self.gizmo_hold.is_none() {
                                self.gizmo_hot = if resp.hovered() { pointer.and_then(|p| l.hit(mode, p)) } else { None };
                            }
                            if resp.drag_started_by(egui::PointerButton::Primary) {
                                if let Some(at) = ui.input(|i| i.pointer.press_origin()) {
                                    if let Some(h) = l.hit(mode, at) {
                                        me.hist.push((me.mesh.clone(), me.sel.clone()));
                                        me.redo.clear();
                                        self.gizmo_hold = Some((h, at, at, if mode == crate::gizmo::Mode::Scale { [1.0; 3] } else { [0.0; 3] }));
                                    }
                                }
                            }
                            if let (Some((h, prev, start, mut acc)), true, Some(cur)) = (self.gizmo_hold, resp.dragged_by(egui::PointerButton::Primary), pointer) {
                                let moved = match crate::gizmo::drag(&rig, &l, mode, h, prev, cur) {
                                    Some(crate::gizmo::Delta::Move(m)) => {
                                        let w = m.map(|x| x * longest);
                                        acc = add3(acc, w);
                                        for &i in &chosen {
                                            me.mesh.pos[i] = to_local(add3(to_world(me.mesh.pos[i]), w));
                                        }
                                        true
                                    }
                                    Some(crate::gizmo::Delta::Rotate(a, t)) => {
                                        acc[0] += t;
                                        let (s, co) = t.sin_cos();
                                        for &i in &chosen {
                                            let v = sub(to_world(me.mesh.pos[i]), cen);
                                            let (kxv, kdv) = (cross(a, v), dot(a, v));
                                            let r = [0, 1, 2].map(|k| cen[k] + v[k] * co + kxv[k] * s + a[k] * kdv * (1.0 - co));
                                            me.mesh.pos[i] = to_local(r);
                                        }
                                        true
                                    }
                                    Some(crate::gizmo::Delta::Scale(f)) => {
                                        for k in 0..3 {
                                            acc[k] *= f[k];
                                        }
                                        for &i in &chosen {
                                            let v = sub(to_world(me.mesh.pos[i]), cen);
                                            me.mesh.pos[i] = to_local([cen[0] + v[0] * f[0], cen[1] + v[1] * f[1], cen[2] + v[2] * f[2]]);
                                        }
                                        true
                                    }
                                    None => false,
                                };
                                if moved {
                                    let _ = me.mesh.write_obj(&me.file);
                                    self.scene_dirty = true;
                                }
                                self.gizmo_hold = Some((h, cur, start, acc));
                            }
                            gizmo = Some((origin, axes));
                        }
                    }
                }
                if self.gizmo_hold.is_some() && (resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down())) {
                    self.gizmo_hold = None;
                    me.skip_click = true;
                }
            }
        }
        // Navigation comme dans Blender, dans une scène : glisser à gauche dans le vide = sélection au rectangle,
        // molette enfoncée = tourner (Maj : déplacer, Ctrl : zoom), Alt + glisser à gauche = tourner (sans souris à trois boutons)
        let scene_nav = crate::scene::is_scene(&self.path) && self.edit.is_none();
        if scene_nav && resp.drag_started_by(egui::PointerButton::Primary) && !mods.alt && self.gizmo_hold.is_none() && self.modal.is_none() && !lkey && !hkey {
            let at = ui.input(|i| i.pointer.press_origin());
            match self.mesh.as_mut() {
                Some(me) if me.modal.is_none() => me.boxing = Some(at),
                Some(_) => {}
                None => self.box_sel = Some(at),
            }
        }
        let left_orbits = !scene_nav || mods.alt || lkey || hkey;
        let dragging = self.gizmo_hold.is_none() && self.modal.is_none() && self.box_sel.is_none() && self.mesh.as_ref().is_none_or(|m| m.modal.is_none() && m.boxing.is_none()) && (resp.dragged_by(egui::PointerButton::Middle) || (!guiding && left_orbits && resp.dragged_by(egui::PointerButton::Primary)) || resp.dragged_by(egui::PointerButton::Secondary));
        if resp.clicked() {
            if let (Some(e), Some((_, _, prep)), Some(pt)) = (&mut self.edit, &self.prepared, resp.interact_pointer_pos()) {
                if let Some((tri, bary)) = pick(prep, &self.cam, rect.width(), rect.height(), pt.x - rect.left(), pt.y - rect.top()) {
                    e.click(prep, tri, bary, mods.shift, mods.command, mods.alt);
                } else if !mods.shift {
                    e.sel.clear();
                    e.version += 1;
                }
            }
        }
        if dragging {
            let d = resp.drag_delta();
            let right = resp.dragged_by(egui::PointerButton::Secondary);
            let middle = resp.dragged_by(egui::PointerButton::Middle);
            // Profil Maya : Alt + gauche tourner, Alt + milieu (ou milieu seul) déplacer, Alt + droit zoom
            let maya = crate::settings::nav_maya();
            if maya && !lkey && !hkey && (middle || (right && mods.alt)) {
                if right {
                    self.cam.dist = (self.cam.dist * ((d.y - d.x) * 0.01).exp()).clamp(0.2, 50.0);
                } else {
                    let k = self.cam.dist / (rect.height().min(rect.width()) * 1.2);
                    self.cam.pan[0] += d.x * k;
                    self.cam.pan[1] -= d.y * k;
                }
            } else if lkey {
                self.look.light.azimuth -= d.x * 0.01;
                self.look.light.elevation = (self.look.light.elevation - d.y * 0.01).clamp(-1.5, 1.5);
            } else if hkey {
                self.look.env_rot = (self.look.env_rot + d.x * 0.01 + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            } else if mods.ctrl || mods.command {
                self.cam.dist = (self.cam.dist * (d.y * 0.01).exp()).clamp(0.2, 50.0);
            } else if mods.shift || right {
                let k = self.cam.dist / (rect.height().min(rect.width()) * 1.2);
                self.cam.pan[0] += d.x * k;
                self.cam.pan[1] -= d.y * k;
            } else {
                self.cam.yaw -= d.x * 0.008;
                self.cam.pitch = (self.cam.pitch + d.y * 0.008).clamp(-1.57, 1.57);
            }
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.zoom_delta().ln() * 300.0);
            if scroll != 0.0 {
                self.cam.dist = (self.cam.dist * (-scroll * 0.0015).exp()).clamp(0.2, 50.0);
            }
            let editing_mesh = self.mesh.is_some();
            let keys = ui.input(|i| [Key::Num1, Key::Num3, Key::Num7, Key::Num5, Key::Home, Key::Period].map(|k| i.key_pressed(k) && !i.modifiers.command && !(editing_mesh && matches!(k, Key::Num1 | Key::Num3))));
            let opposite = mods.shift;
            let half = std::f32::consts::FRAC_PI_2;
            if keys[0] {
                self.cam.yaw = if opposite { std::f32::consts::PI } else { 0.0 };
                self.cam.pitch = 0.0;
            }
            if keys[1] {
                self.cam.yaw = if opposite { half } else { -half };
                self.cam.pitch = 0.0;
            }
            if keys[2] {
                self.cam.yaw = 0.0;
                self.cam.pitch = if opposite { -half } else { half };
            }
            if keys[3] {
                self.cam.ortho = !self.cam.ortho;
            }
            if keys[4] || keys[5] {
                self.cam = Camera { ortho: self.cam.ortho, ..Default::default() };
            }
        }
        if self.turntable && !dragging {
            let dt = ui.input(|i| i.stable_dt).min(0.1);
            self.cam.yaw -= dt * 0.6;
        }

        // ---------- rendu (seulement si quelque chose a changé) ----------
        let interacting = dragging || self.turntable || ui.input(|i| i.smooth_scroll_delta.y != 0.0);
        let ppp = ui.ctx().pixels_per_point();
        // Pendant le mouvement : demi-résolution pour rester fluide, pleine résolution à l'arrêt.
        // Un modèle léger reste net pendant le mouvement : passer en basse définition puis revenir
        // se voyait comme un petit chargement à chaque geste
        let heavy = shown_tris > 600_000;
        let light = shown_tris < 150_000;
        let scale = if interacting && !light { if heavy { 0.4 } else { 0.6 } } else { ppp.min(2.0) };
        let size = [(rect.width() * scale).max(16.0) as usize, (rect.height() * scale).max(16.0) as usize];
        let (mut look, refs, refs_key) = self.render_setup();
        // Pendant qu'on tourne autour du modèle : moins de rayons d'ombre, pour rester fluide
        if interacting {
            look.shadow_samples = look.shadow_samples.min(2);
        }
        let look_key = format!("{:?}|{refs_key}", look.key());
        let key = (self.cam, size, z_up, self.prepared.as_ref().map(|p| p.1).unwrap_or(0), look_key);
        let trace_key = key.clone();
        if self.last.as_ref() != Some(&key) || self.tex.is_none() {
            // Le maillage peut être en cours de recalcul (modificateurs, sortie du mode édition)
            let Some(prepared) = self.prepared.as_ref() else {
                ui.ctx().request_repaint();
                return;
            };
            let prep = &prepared.2;
            let mut frame = render(prep, &self.cam, &look, size[0], size[1]);
            draw_refs(&mut frame, &self.cam, &refs);
            let img = egui::ColorImage::from_rgba_unmultiplied([frame.w, frame.h], &frame.rgba);
            self.ids = Some((frame.w, frame.h, frame.ids));
            self.ids_rev += 1;
            match &mut self.tex {
                Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                None => self.tex = Some(ui.ctx().load_texture("model3d", img, egui::TextureOptions::LINEAR)),
            }
            self.last = Some(key);
        }
        if interacting {
            ui.ctx().request_repaint();
        }
        self.final_render_ui(ui.ctx(), &look);
        // Tracé de chemins : à l'arrêt, l'image s'affine passe après passe (l'aperçu sert pendant le mouvement)
        if self.engine == 1 && (!interacting || self.trace_set.live) && self.mesh.is_none() {
            if let Some(prepared) = self.prepared.as_ref() {
                let use_gpu = self.trace_set.gpu && gputrace::hardware();
                let mut fresh = false;
                if self.trace.as_ref().is_none_or(|t| t.0 != trace_key) {
                    // Sur la carte graphique : pleine résolution ; sur le processeur : environ 0,4 mégapixel
                    // (moins pendant le mouvement, pour que la vue suive)
                    let budget = if interacting { 120_000.0 } else { 400_000.0 };
                    let f = if use_gpu { if interacting { 0.6 } else { 1.0 } } else { (budget / (size[0] * size[1]) as f32).sqrt().min(1.0) };
                    let (tw, th) = (((size[0] as f32 * f) as usize).max(8), ((size[1] as f32 * f) as usize).max(8));
                    self.trace = Some((trace_key.clone(), pathtrace::Accum::new(tw, th)));
                    fresh = true;
                }
                if let Some((_, acc)) = &mut self.trace {
                    if acc.spp < self.trace_max {
                        let before = acc.spp;
                        let t0 = std::time::Instant::now();
                        let scene_key = {
                            use std::hash::{Hash, Hasher};
                            let mut hh = std::collections::hash_map::DefaultHasher::new();
                            (trace_key.2, trace_key.3, &trace_key.4).hash(&mut hh);
                            hh.finish()
                        };
                        if use_gpu && gputrace::prepare(&mut self.gpu_scene, &prepared.2, &look, scene_key, acc.w, acc.h) {
                            let s = self.gpu_scene.as_mut().unwrap();
                            if fresh {
                                gputrace::reset(s);
                            }
                            // Quelques passes par image, relues d'un coup
                            let n = (self.trace_max - acc.spp).min(self.gpu_batch);
                            gputrace::passes(s, &prepared.2, &self.cam, &look, &self.trace_set, n, acc);
                            // Ajuste le nombre de passes pour rester autour de 30 ms
                            let ms = t0.elapsed().as_millis() as u32;
                            self.gpu_batch = if ms < 20 { (self.gpu_batch * 2).min(64) } else if ms > 45 { (self.gpu_batch / 2).max(1) } else { self.gpu_batch };
                        } else {
                            // Autant de passes qu'il en tient dans ~40 ms
                            while acc.spp < self.trace_max && (acc.spp == 0 || (!interacting && t0.elapsed().as_millis() < 40)) {
                                pathtrace::pass(&prepared.2, &self.cam, &look, &self.trace_set, acc);
                            }
                        }
                        // L'IA (plus lente) passe quand l'image est finie et toutes les 16 passes ; le filtre rapide entre-temps
                        let ai = acc.spp >= self.trace_max || acc.spp / 16 != before / 16;
                        acc.agx = self.trace_set.agx;
                        let img = egui::ColorImage::from_rgba_unmultiplied([acc.w, acc.h], &acc.image_ex(look.exposure, self.trace_set.denoise, ai));
                        match &mut self.tex {
                            Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                            None => self.tex = Some(ui.ctx().load_texture("model3d", img, egui::TextureOptions::LINEAR)),
                        }
                        ui.ctx().request_repaint();
                    }
                }
            }
        } else if self.engine == 1 && interacting {
            // Le prochain arrêt repart de zéro, et l'aperçu redessine la vue en attendant
            if self.trace.take().is_some() {
                self.last = None;
            }
        }

        // Grille du sol (sous le modèle)
        if let Some((_, _, prep)) = &self.prepared {
            if !self.look.show_bg {
                let focal = rect.height().min(rect.width()) * 1.2;
                let ortho_scale = focal / self.cam.dist;
                let c = rect.center();
                let project = |v: V3| -> Option<Pos2> {
                    let q = self.cam.view(v);
                    if self.cam.ortho {
                        Some(pos2(c.x + q[0] * ortho_scale, c.y - q[1] * ortho_scale))
                    } else {
                        (q[2] < -0.05).then(|| pos2(c.x + q[0] / -q[2] * focal, c.y - q[1] / -q[2] * focal))
                    }
                };
                for k in -5..=5 {
                    let t = k as f32 * 0.2;
                    for (a, b) in [([t, prep.floor, -1.0], [t, prep.floor, 1.0]), ([-1.0, prep.floor, t], [1.0, prep.floor, t])] {
                        if let (Some(pa), Some(pb)) = (project(a), project(b)) {
                            painter.line_segment([pa, pb], Stroke::new(1.0, p.line));
                        }
                    }
                }
            }
        }
        if let Some(t) = &self.tex {
            painter.image(t.id(), rect, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        self.draw_outline(ui, &painter, rect);
        self.mesh_edit_ui(ui, rect, &resp);
        // Repères des lumières et des caméras de la scène
        if let (Some(scene), Some((lo, hi)), true) = (self.scene.as_ref(), self.scene_frame, self.edit.is_none()) {
            let longest = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
            let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
            let focal = rect.height().min(rect.width()) * 1.2;
            let (cam, rc) = (self.cam, rect.center());
            let project = |v: V3| -> Option<Pos2> {
                let q = cam.view([(v[0] - c[0]) / longest, (v[1] - c[1]) / longest, (v[2] - c[2]) / longest]);
                if cam.ortho {
                    Some(pos2(rc.x + q[0] * focal / cam.dist, rc.y - q[1] * focal / cam.dist))
                } else {
                    (q[2] < -0.05).then(|| pos2(rc.x + q[0] / -q[2] * focal, rc.y - q[1] / -q[2] * focal))
                }
            };
            let over = ui.painter_at(rect);
            // Sélection au rectangle (B, puis glisser) : prend les objets dont l'origine tombe dedans
            let mut boxed: Option<egui::Rect> = None;
            if let Some(start) = self.box_sel {
                let (pressed, down, cur) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.latest_pos()));
                match (start, cur) {
                    (None, Some(c)) if pressed && rect.contains(c) => self.box_sel = Some(Some(c)),
                    (Some(s), Some(c)) => {
                        let area = egui::Rect::from_two_pos(s, c);
                        over.rect_filled(area, 0.0, p.accent.gamma_multiply(0.12));
                        over.rect_stroke(area, 0.0, Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
                        if !down {
                            boxed = Some(area);
                            self.box_sel = None;
                            self.modal_click = true;
                        }
                    }
                    _ => {}
                }
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    self.box_sel = None;
                }
                ui.ctx().request_repaint();
            }
            let mut in_box: Vec<usize> = vec![];
            for (k, o) in scene.objects.iter().enumerate() {
                let kind = crate::scene::kind(o);
                if !scene.shown(k) {
                    continue;
                }
                let Some(at) = project(scene.world(k, [0.0; 3])) else { continue };
                if boxed.is_some_and(|b| b.contains(at)) {
                    in_box.push(k);
                }
                // L'origine de chaque objet choisi : un point plein pour l'actif, un anneau pour les autres
                if self.scene_multi.contains(&k) && self.scene_sel != Some(k) {
                    over.circle_stroke(at, 4.0, Stroke::new(2.0, p.accent));
                }
                if !matches!(kind, crate::scene::Kind::Light | crate::scene::Kind::Camera) {
                    continue;
                }
                let on = self.scene_sel == Some(k);
                let col = if on { p.accent } else { p.text };
                // Un trait montre le sens : où va la lumière, où regarde la caméra
                let ahead = if kind == crate::scene::Kind::Light { [0.0, -1.0, 0.0] } else { [0.0, 0.0, -1.0] };
                let (a, b) = (scene.world(k, [0.0; 3]), scene.world(k, ahead));
                let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-9);
                let reach = longest * 0.12;
                if let Some(tip) = project([a[0] + d[0] / l * reach, a[1] + d[1] / l * reach, a[2] + d[2] / l * reach]) {
                    // Une ponctuelle éclaire partout : pas de sens à montrer
                    if !(kind == crate::scene::Kind::Light && crate::scene::light_shape(o) == 1) {
                        over.line_segment([at, tip], Stroke::new(1.5, col));
                        over.circle_filled(tip, 2.5, col);
                    }
                }
                if kind == crate::scene::Kind::Light {
                    let thin = Stroke::new(1.0, col.gamma_multiply(0.8));
                    match crate::scene::light_shape(o) {
                        // Spot : le cône, jusqu'à un cercle au bout du trait
                        2 => {
                            let half = (o.spot.clamp(1.0, 179.0) * 0.5).to_radians();
                            let (dl, rad) = (reach * 1.6, (reach * 1.6) * half.tan().min(4.0));
                            let dir = [d[0] / l, d[1] / l, d[2] / l];
                            let ax = scene.world(k, [1.0, 0.0, 0.0]);
                            let u0 = [ax[0] - a[0], ax[1] - a[1], ax[2] - a[2]];
                            let ul = (u0[0] * u0[0] + u0[1] * u0[1] + u0[2] * u0[2]).sqrt().max(1e-9);
                            let u = [u0[0] / ul, u0[1] / ul, u0[2] / ul];
                            let v = cross(dir, u);
                            let ring: Vec<Option<Pos2>> = (0..=24)
                                .map(|i| {
                                    let t = i as f32 / 24.0 * std::f32::consts::TAU;
                                    let (c, s) = (t.cos() * rad, t.sin() * rad);
                                    project([a[0] + dir[0] * dl + u[0] * c + v[0] * s, a[1] + dir[1] * dl + u[1] * c + v[1] * s, a[2] + dir[2] * dl + u[2] * c + v[2] * s])
                                })
                                .collect();
                            for w in ring.windows(2) {
                                if let (Some(p0), Some(p1)) = (w[0], w[1]) {
                                    over.line_segment([p0, p1], thin);
                                }
                            }
                            for i in [0, 6, 12, 18] {
                                if let Some(q) = ring[i] {
                                    over.line_segment([at, q], thin);
                                }
                            }
                        }
                        // Rectangle : son contour
                        3 => {
                            let (hx, hz) = (o.size[0] * 0.5, o.size[1] * 0.5);
                            let corners: Vec<Option<Pos2>> = [[-hx, 0.0, -hz], [hx, 0.0, -hz], [hx, 0.0, hz], [-hx, 0.0, hz], [-hx, 0.0, -hz]].iter().map(|c| project(scene.world(k, *c))).collect();
                            for w in corners.windows(2) {
                                if let (Some(p0), Some(p1)) = (w[0], w[1]) {
                                    over.line_segment([p0, p1], Stroke::new(1.5, col));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if kind == crate::scene::Kind::Light {
                    over.circle_filled(at, 5.0, Color32::from_rgb(250, 200, 60));
                    over.circle_stroke(at, 8.5, Stroke::new(1.5, col));
                } else {
                    let body = egui::Rect::from_center_size(at, egui::vec2(16.0, 11.0));
                    over.rect_filled(body, 2.0, p.panel);
                    over.rect_stroke(body, 2.0, Stroke::new(1.5, col), egui::StrokeKind::Inside);
                }
                // Un clic sur le repère choisit l'objet
                if resp.clicked() && resp.interact_pointer_pos().is_some_and(|m| (m - at).length() < 11.0) {
                    marker_pick = Some(k);
                }
            }
            if boxed.is_some() {
                boxed_pick = Some(in_box);
            }
        }
        if let Some(k) = marker_pick {
            self.scene_sel = Some(k);
        }
        if let Some(mut list) = boxed_pick {
            // Ctrl : retire de la sélection ; Maj : ajoute
            if mods.command {
                let mut all: Vec<usize> = self.scene_multi.clone();
                all.extend(self.scene_sel);
                all.retain(|k| !list.contains(k));
                all.dedup();
                list = all;
            } else if mods.shift {
                let mut all: Vec<usize> = self.scene_multi.clone();
                all.extend(self.scene_sel);
                all.retain(|k| !list.contains(k));
                all.extend(list);
                list = all;
            }
            self.scene_sel = list.last().copied();
            self.scene_multi = if list.len() > 1 { list } else { vec![] };
            self.multi_follow = None;
        }
        if let (Some((origin, gaxes)), Some((lo, hi))) = (gizmo, self.scene_frame) {
            let _ = (lo, hi);
            let focal = rect.height().min(rect.width()) * 1.2;
            let project = projector(self.cam, rect);
            let (toward, right, up, per_px) = gizmo_basis(&self.cam, origin, focal);
            let mode = crate::gizmo::Mode::from_u8(self.gizmo_mode);
            let rig = crate::gizmo::Rig { origin, axes: gaxes, len: per_px * 92.0, toward, right, up, per_px, project: &project };
            if let Some(l) = rig.layout(mode) {
                let over = ui.painter_at(rect);
                let hold = self.gizmo_hold;
                let swept = hold.filter(|_| mode == crate::gizmo::Mode::Rotate).map(|(_, _, start, acc)| (acc[0], start));
                crate::gizmo::draw(&over, &rig, &l, mode, self.gizmo_hot, hold.map(|x| x.0), swept);
                // Valeur en cours, près du pointeur
                if let (Some((_, cur, _, acc)), true) = (hold, mode != crate::gizmo::Mode::Rotate) {
                    let text = if mode == crate::gizmo::Mode::Move { format!("Δ {:+.2}  {:+.2}  {:+.2}", acc[0], acc[1], acc[2]) } else { format!("× {:.2}  {:.2}  {:.2}", acc[0], acc[1], acc[2]) };
                    let mut sh = vec![];
                    crate::gizmo::text_tag(&mut sh, &over, cur + egui::vec2(18.0, 22.0), &text, Color32::WHITE);
                    over.extend(sh);
                }
                if self.gizmo_hold.is_some() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                } else if self.gizmo_hot.is_some() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
            }
        }
        // Coutures (mode édition), en rouge par-dessus le modèle
        if let (Some(e), Some((_, _, prep))) = (&self.edit, &self.prepared) {
            let focal = rect.height().min(rect.width()) * 1.2;
            let ortho_scale = focal / self.cam.dist;
            let c = rect.center();
            let project = |v: V3| -> Option<Pos2> {
                let q = self.cam.view(v);
                if self.cam.ortho {
                    Some(pos2(c.x + q[0] * ortho_scale, c.y - q[1] * ortho_scale))
                } else {
                    (q[2] < -0.05).then(|| pos2(c.x + q[0] / -q[2] * focal, c.y - q[1] / -q[2] * focal))
                }
            };
            let red = Color32::from_rgb(235, 60, 50);
            // Visible si un pixel le long de l'arête montre une des deux faces qui la bordent
            let visible = |a: Pos2, b: Pos2, faces: &[u32]| -> bool {
                let Some((w, h, ids)) = &self.ids else { return true };
                let (sx, sy) = (*w as f32 / rect.width(), *h as f32 / rect.height());
                [0.25f32, 0.5, 0.75].iter().any(|t| {
                    let q = a + (b - a) * *t;
                    let (x, y) = (((q.x - rect.left()) * sx) as i64, ((q.y - rect.top()) * sy) as i64);
                    (-1..=1).any(|dy| {
                        (-1..=1).any(|dx| {
                            let (xx, yy) = (x + dx, y + dy);
                            if xx < 0 || yy < 0 || xx >= *w as i64 || yy >= *h as i64 {
                                return false;
                            }
                            let id = ids[yy as usize * w + xx as usize];
                            e.owner.get(id as usize).is_some_and(|f| faces.contains(f))
                        })
                    })
                })
            };
            for &(a, b) in e.data.seams.iter().take(40_000) {
                if let (Some(pa), Some(pb)) = (project(prep.vertex(a as usize)), project(prep.vertex(b as usize))) {
                    let faces = e.topo.edge_faces.get(&(a, b)).map(|v| v.as_slice()).unwrap_or(&[]);
                    if visible(pa, pb, faces) {
                        painter.line_segment([pa, pb], Stroke::new(2.0, red));
                    }
                }
            }
            // Arêtes choisies (outil coutures), en orange par-dessus
            let orange = Color32::from_rgb(255, 160, 40);
            for &(a, b) in e.tools.esel.iter().take(40_000) {
                if let (Some(pa), Some(pb)) = (project(prep.vertex(a as usize)), project(prep.vertex(b as usize))) {
                    let faces = e.topo.edge_faces.get(&(a, b)).map(|v| v.as_slice()).unwrap_or(&[]);
                    if visible(pa, pb, faces) {
                        painter.line_segment([pa, pb], Stroke::new(2.6, orange));
                    }
                }
            }
            // Guides de retopologie
            let guide_col = Color32::from_rgb(255, 170, 30);
            let facing = |a: V3, n: V3| -> bool {
                let ca = self.cam.view(a);
                let cn = sub(self.cam.view([a[0] + n[0], a[1] + n[1], a[2] + n[2]]), ca);
                if self.cam.ortho { cn[2] > 0.0 } else { dot(cn, [-ca[0], -ca[1], -ca[2]]) > 0.0 }
            };
            // Touches de densité : des disques, verts pour plus de quads, bleus pour moins
            for d in e.data.density.iter().take(4000) {
                if let (Some(c), Some(edge)) = (project([d[0], d[1], d[2]]), project([d[0] + d[3], d[1], d[2]])) {
                    let col = if d[4] >= 1.0 { Color32::from_rgba_unmultiplied(90, 220, 120, 40) } else { Color32::from_rgba_unmultiplied(90, 150, 255, 40) };
                    painter.circle_filled(c, (edge - c).length().max(2.0), col);
                }
            }
            for g in e.data.guides.iter().chain(std::iter::once(&e.guide_now)) {
                let pts: Vec<Pos2> = g.iter().filter_map(|v| project(*v)).collect();
                if pts.len() >= 2 {
                    painter.add(egui::Shape::line(pts.clone(), Stroke::new(3.0, guide_col)));
                    painter.circle_filled(pts[0], 3.5, guide_col);
                }
            }
            // Résultat de la retopologie, en fil de fer (faces tournées vers la caméra)
            if let Some(r) = e.retopo.as_ref().filter(|_| e.retopo_show) {
                let blue = Color32::from_rgb(70, 160, 255);
                let mut drawn = std::collections::HashSet::new();
                for f in r.faces.iter().take(60_000) {
                    let c = f.iter().fold([0.0f32; 3], |a, &i| [a[0] + r.pos[i as usize][0], a[1] + r.pos[i as usize][1], a[2] + r.pos[i as usize][2]]);
                    let c = [c[0] / f.len() as f32, c[1] / f.len() as f32, c[2] / f.len() as f32];
                    let n = cross(sub(r.pos[f[1] as usize], r.pos[f[0] as usize]), sub(r.pos[f[f.len() - 1] as usize], r.pos[f[0] as usize]));
                    if !facing(c, n) {
                        continue;
                    }
                    for k in 0..f.len() {
                        let (a, b) = (f[k], f[(k + 1) % f.len()]);
                        if !drawn.insert((a.min(b), a.max(b))) {
                            continue;
                        }
                        if let (Some(pa), Some(pb)) = (project(r.pos[a as usize]), project(r.pos[b as usize])) {
                            painter.line_segment([pa, pb], Stroke::new(1.2, blue));
                        }
                    }
                }
            }
        }

        // Repère des axes (en bas à gauche)
        let o = rect.left_bottom() + vec2(36.0, -40.0);
        painter.circle_filled(o, 30.0, p.panel.gamma_multiply(0.6));
        for (axis, col, label) in [([1.0, 0.0, 0.0], crate::gizmo::AXIS[0], "X"), ([0.0, 0.0, -1.0], crate::gizmo::AXIS[2], "Y"), ([0.0, 1.0, 0.0], crate::gizmo::AXIS[1], "Z")] {
            let r = self.cam.rotate(axis);
            let end = o + vec2(r[0], -r[1]) * 22.0;
            painter.line_segment([o, end], Stroke::new(2.0, col));
            painter.text(end, egui::Align2::CENTER_CENTER, label, egui::FontId::monospace(10.0), col);
        }
        // Les raccourcis sont dans la barre du bas ; la vue ne garde que les messages et le mode en cours
        if let Some(msg) = &self.capture_msg {
            let g = painter.layout(msg.clone(), egui::FontId::monospace(11.0), p.text, rect.width() - 40.0);
            let r = egui::Rect::from_min_size(pos2(rect.left() + 12.0, rect.bottom() - g.size().y - 18.0), g.size() + vec2(12.0, 6.0));
            painter.rect_filled(r, 4.0, p.panel.gamma_multiply(0.92));
            painter.galley(r.min + vec2(6.0, 3.0), g, p.text);
        }
        // Statistiques du modèle, discrètes, en bas à gauche (comme la superposition de Blender)
        if let (1, Some((_, acc))) = (self.engine, &self.trace) {
            let col = if self.look.show_bg { Color32::from_white_alpha(200) } else { p.muted };
            painter.text(rect.left_bottom() + vec2(60.0, -10.0), egui::Align2::LEFT_BOTTOM, format!("{} · {} / {}", crate::i18n::t("tracé de chemins"), acc.spp, self.trace_max), egui::FontId::monospace(10.5), col);
        }
        if !self.stats_text.is_empty() {
            let col = if self.look.show_bg { Color32::from_gray(225) } else { p.muted };
            painter.text(rect.right_bottom() + vec2(-12.0, -10.0), egui::Align2::RIGHT_BOTTOM, &self.stats_text, egui::FontId::monospace(10.5), col);
        }
        if self.mesh.is_some() {
            let g = painter.layout_no_wrap(crate::i18n::t("MODE ÉDITION"), egui::FontId::monospace(11.0), p.on_accent);
            let r = egui::Rect::from_min_size(rect.right_top() + vec2(-g.size().x - 26.0, 56.0), g.size() + vec2(14.0, 8.0));
            painter.rect_filled(r, 4.0, p.accent);
            painter.galley(r.min + vec2(7.0, 4.0), g, p.on_accent);
        }
    }
}

fn lin_accent(c: Color32) -> [f32; 3] {
    let l = |x: u8| (x as f32 / 255.0).powf(2.2);
    [l(c.r()), l(c.g()), l(c.b())]
}

pub fn fmt_n(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formes_de_lumiere() {
        let mut spot = Lamp::simple(true, [0.0, 2.0, 0.0], [1.0; 3], 1.0, 100.0);
        spot.shape = 2;
        spot.axis = [0.0, -1.0, 0.0];
        spot.cone = [(20f32).to_radians().cos(), (15f32).to_radians().cos()];
        // Juste dessous : éclairé ; loin sur le côté : hors du cône
        assert!(spot.sample([0.0, 0.0, 0.0], [0.5, 0.5]).2 > 0.9);
        assert_eq!(spot.sample([3.0, 0.0, 0.0], [0.5, 0.5]).2, 0.0);
        let mut rect = Lamp::simple(true, [0.0, 2.0, 0.0], [1.0; 3], 1.0, 100.0);
        rect.shape = 3;
        rect.axis = [0.0, -1.0, 0.0];
        rect.half = [[0.5, 0.0, 0.0], [0.0, 0.0, 0.5]];
        // Dessous : éclairé ; au-dessus (derrière le panneau) : rien
        assert!(rect.sample([0.0, 0.0, 0.0], [0.5, 0.5]).2 > 0.9);
        assert_eq!(rect.sample([0.0, 4.0, 0.0], [0.5, 0.5]).2, 0.0);
        // Deux tirages sur le panneau : deux directions (ombres douces)
        assert!(rect.sample([0.0, 0.0, 0.0], [0.0, 0.0]).0 != rect.sample([0.0, 0.0, 0.0], [1.0, 1.0]).0);
    }

    #[test]
    fn animation_en_video() {
        let Some(ff) = crate::ffmpeg::find("ffmpeg") else { return };
        let dir = std::env::temp_dir().join(format!("aryadne-anim-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.aryscene");
        let mut sc = crate::scene::Scene::starter();
        sc.start = 0;
        sc.end = 5;
        let k = sc.objects.iter().position(|o| o.file == "@cube").unwrap();
        sc.objects[k].set_key(0);
        sc.objects[k].pos[0] = 3.0;
        sc.objects[k].set_key(5);
        sc.write(&path).unwrap();
        let out = dir.join("s.mp4");
        let n = std::sync::atomic::AtomicUsize::new(0);
        encode_animation(ff, sc, &path, ([-6.0; 3], [6.0; 3]), &Look::default(), Camera::default(), (64, 36), &out, &|| {
            n.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        })
        .unwrap();
        assert_eq!(n.load(std::sync::atomic::Ordering::Relaxed), 6);
        assert!(std::fs::metadata(&out).map(|m| m.len() > 0).unwrap_or(false));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn formats_et_simplification() {
        let obj = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1/1/1 2/2/2 3/3/3 4/4/4\nf -4 -3 -2\n";
        let m = load_obj(obj.as_bytes()).unwrap();
        assert_eq!(m.total_tris, 3, "un quad = 2 triangles, plus un triangle en indices négatifs");
        assert_eq!(m.size(), [1.0, 1.0, 0.0]);

        let mut stl = vec![0u8; 80];
        stl.extend(2u32.to_le_bytes());
        for tri in [[[0f32, 0., 0.], [2., 0., 0.], [0., 3., 0.]], [[2., 0., 0.], [2., 3., 0.], [0., 3., 0.]]] {
            stl.extend([0u8; 12]);
            for v in tri {
                for c in v {
                    stl.extend(c.to_le_bytes());
                }
            }
            stl.extend([0u8; 2]);
        }
        let m = load_stl(&stl).unwrap();
        assert_eq!(m.total_tris, 2);
        assert_eq!(m.positions.len(), 4, "sommets partagés fusionnés");

        // Grille dense de 200 x 200 quads = 80 000 triangles, simplifiée sous 5 000
        let n = 200;
        let mut pos = vec![];
        for y in 0..=n {
            for x in 0..=n {
                pos.push([x as f32, y as f32, ((x * y) as f32 * 0.01).sin()]);
            }
        }
        let mut tris = vec![];
        for y in 0..n {
            for x in 0..n {
                let i = (y * (n + 1) + x) as u32;
                tris.push([i, i + 1, i + n as u32 + 2]);
                tris.push([i, i + n as u32 + 2, i + n as u32 + 1]);
            }
        }
        let mut m = Model::new(pos, tris, None, false).unwrap();
        m.simplify(5_000);
        assert!(m.tris.len() <= 5_000 && m.tris.len() > 1_000, "simplifié à {}", m.tris.len());
        let img = thumbnail(&m, 64, 64, [0, 0, 0, 0]);
        assert!(img.pixels().any(|p| p[3] == 255), "la miniature doit contenir le modèle");
        assert!(is_model("assets/Maison.FBX") && !is_model("a.png"));
        assert_eq!(fmt_n(31457508), "31 457 508");
    }
}

#[cfg(test)]
mod look_tests {
    use super::*;

    #[test]
    fn subdivision_et_fil_de_fer() {
        // Cube en quads : 12 triangles, diagonales cachées dans le fil de fer
        let obj = "v -1 -1 -1\nv 1 -1 -1\nv 1 1 -1\nv -1 1 -1\nv -1 -1 1\nv 1 -1 1\nv 1 1 1\nv -1 1 1\nf 1 4 3 2\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n";
        let m = load_obj(obj.as_bytes()).unwrap();
        assert_eq!(m.edges.iter().map(|e| e.count_ones()).sum::<u32>(), 24, "4 arêtes par quad, pas de diagonale");
        let p = Prepared::new(&m, false, 2);
        // Affichage optimal : seulement les 12 arêtes du cube, coupées en 4 (×2 faces)
        let (_, t, _, e) = catmull_clark(&m.positions, &m.tris, &m.colors, &m.edges, 2, true);
        assert_eq!(t.len(), 6 * 16 * 2);
        assert_eq!(e.iter().map(|e| e.count_ones()).sum::<u32>(), 12 * 4 * 2);
        assert_eq!(p.tri_count(), 12 * 16);
        // Après 2 niveaux, chaque quad devient 16 quads : 6 * 16 * 4 arêtes vues des deux côtés
        assert_eq!(p.edges.iter().map(|e| e.count_ones()).sum::<u32>(), 6 * 16 * 4);
        // Sommets lissés : le cube s'arrondit (plus aucun sommet au coin)
        assert!(p.pos.iter().all(|v| v[0].abs() < 0.5 - 1e-3 || v[1].abs() < 0.5 - 1e-3 || v[2].abs() < 0.5 - 1e-3));
        if std::env::var("ARYADNE_SHOTS").is_ok() {
            for (i, (env, wire, bg, mat)) in [(0, 0.8, false, Material::Clay), (2, 0.0, true, Material::Chrome), (3, 0.5, true, Material::White)].into_iter().enumerate() {
                let look = Look { env: Env::preset(env), wire, show_bg: bg, material: mat, ..Default::default() };
                let f = render(&p, &Camera::default(), &look, 400, 300);
                image::RgbaImage::from_raw(400, 300, f.rgba).unwrap().save(format!("/tmp/look{i}.png")).unwrap();
            }
        }
    }
}

/// Retrouve les polygones d'origine (quads, n-gones) à partir des triangles et de leurs
/// arêtes visibles. Les sommets confondus sont soudés pour éviter les fentes aux coutures UV.
pub(crate) fn rebuild_polygons(pos: &[V3], tris: &[[u32; 3]], edges: &[u8]) -> (Vec<V3>, Vec<Vec<u32>>, Vec<usize>) {
    // Soudure des positions identiques
    let mut weld: HashMap<[u32; 3], u32> = HashMap::with_capacity(pos.len());
    let mut remap = vec![0u32; pos.len()];
    let mut wpos: Vec<V3> = Vec::with_capacity(pos.len());
    for (i, p) in pos.iter().enumerate() {
        let k = [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
        remap[i] = *weld.entry(k).or_insert_with(|| {
            wpos.push(*p);
            (wpos.len() - 1) as u32
        });
    }
    let tris: Vec<[u32; 3]> = tris.iter().map(|t| [remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize]]).collect();
    // Triangles reliés par une arête cachée (diagonale) = même polygone
    let mut parent: Vec<usize> = (0..tris.len()).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let key = |a: u32, b: u32| if a < b { (a, b) } else { (b, a) };
    let mut hidden: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (ti, t) in tris.iter().enumerate() {
        let m = edges.get(ti).copied().unwrap_or(7);
        for k in 0..3 {
            if m & (1 << k) == 0 {
                hidden.entry(key(t[k], t[(k + 1) % 3])).or_default().push(ti);
            }
        }
    }
    for v in hidden.values() {
        if v.len() == 2 {
            let (a, b) = (find(&mut parent, v[0]), find(&mut parent, v[1]));
            parent[a] = b;
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for ti in 0..tris.len() {
        let r = find(&mut parent, ti);
        groups.entry(r).or_default().push(ti);
    }
    let mut polys = vec![];
    let mut first_tri = vec![];
    let mut keys: Vec<usize> = groups.keys().copied().collect();
    keys.sort_unstable();
    for g in keys {
        let ts = &groups[&g];
        if ts.len() == 1 {
            polys.push(tris[ts[0]].to_vec());
            first_tri.push(ts[0]);
            continue;
        }
        // Contour : arêtes orientées qui n'ont pas leur inverse dans le groupe
        let mut dir: HashMap<(u32, u32), u32> = HashMap::new();
        for &ti in ts {
            let t = tris[ti];
            for k in 0..3 {
                *dir.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
            }
        }
        let mut next: HashMap<u32, u32> = HashMap::new();
        let mut ok = true;
        for (&(a, b), _) in dir.iter().filter(|((a, b), _)| !dir.contains_key(&(*b, *a))) {
            if next.insert(a, b).is_some() {
                ok = false;
            }
        }
        let mut loop_ = vec![];
        if ok && !next.is_empty() {
            let start = *next.keys().min().unwrap();
            let mut v = start;
            loop {
                loop_.push(v);
                match next.get(&v) {
                    Some(&n) if n == start => break,
                    Some(&n) if loop_.len() <= next.len() => v = n,
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            ok &= loop_.len() == next.len();
        }
        if ok && loop_.len() >= 3 {
            polys.push(loop_);
            first_tri.push(ts[0]);
        } else {
            for &ti in ts {
                polys.push(tris[ti].to_vec());
                first_tri.push(ti);
            }
        }
    }
    (wpos, polys, first_tri)
}

/// Subdivision Catmull-Clark (comme Blender), sur les vrais polygones du modèle.
/// `optimal` : le fil de fer ne montre que les arêtes issues de la cage d'origine
/// (« Optimal Display » de Blender). Les diagonales de triangulation restent cachées.
pub fn catmull_clark(pos: &[V3], tris: &[[u32; 3]], colors: &[Color32], edges: &[u8], levels: u8, optimal: bool) -> (Vec<V3>, Vec<[u32; 3]>, Vec<Color32>, Vec<u8>) {
    subdiv_poly(pos, tris, colors, edges, levels, optimal, true)
}

/// Subdivision « simple » : même découpage en quads, sans lissage.
pub fn split_flat(pos: &[V3], tris: &[[u32; 3]], colors: &[Color32], edges: &[u8], levels: u8, optimal: bool) -> (Vec<V3>, Vec<[u32; 3]>, Vec<Color32>, Vec<u8>) {
    subdiv_poly(pos, tris, colors, edges, levels, optimal, false)
}

fn subdiv_poly(pos: &[V3], tris: &[[u32; 3]], colors: &[Color32], edges: &[u8], levels: u8, optimal: bool, smooth: bool) -> (Vec<V3>, Vec<[u32; 3]>, Vec<Color32>, Vec<u8>) {
    if levels == 0 || tris.is_empty() {
        return (pos.to_vec(), tris.to_vec(), colors.to_vec(), edges.to_vec());
    }
    let (mut p, mut faces, first) = rebuild_polygons(pos, tris, edges);
    let mut fcol: Vec<Color32> = first.iter().map(|&t| colors.get(t).copied().unwrap_or(Color32::WHITE)).collect();
    // Pour chaque face, l'arête i (v_i → v_i+1) vient-elle de la cage d'origine ?
    let mut forig: Vec<Vec<bool>> = faces.iter().map(|f| vec![true; f.len()]).collect();
    let key = |a: u32, b: u32| if a < b { (a, b) } else { (b, a) };
    let add = |a: V3, b: V3| [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
    let mul = |a: V3, s: f32| [a[0] * s, a[1] * s, a[2] * s];
    for _ in 0..levels.min(5) {
        let quads: usize = faces.iter().map(|f| f.len()).sum();
        if quads * 2 > MAX_SUBDIV_TRIS {
            break;
        }
        let nv = p.len();
        // Points de face
        let fp: Vec<V3> = faces.iter().map(|f| mul(f.iter().fold([0.0; 3], |s, &v| add(s, p[v as usize])), 1.0 / f.len() as f32)).collect();
        // Arêtes → faces voisines
        let mut emap: HashMap<(u32, u32), (u32, Vec<usize>)> = HashMap::with_capacity(quads);
        for (fi, f) in faces.iter().enumerate() {
            for i in 0..f.len() {
                let k = key(f[i], f[(i + 1) % f.len()]);
                let n = emap.len() as u32;
                emap.entry(k).or_insert((n, vec![])).1.push(fi);
            }
        }
        let ne = emap.len();
        let mut ep = vec![[0.0f32; 3]; ne];
        let mut boundary_v = vec![false; nv];
        let mut v_faces = vec![[0.0f32; 3]; nv];
        let mut v_nf = vec![0u32; nv];
        let mut v_mid = vec![[0.0f32; 3]; nv];
        let mut v_ne = vec![0u32; nv];
        let mut v_bsum = vec![[0.0f32; 3]; nv];
        for (&(a, b), (ei, fs)) in &emap {
            let (pa, pb) = (p[a as usize], p[b as usize]);
            let mid = mul(add(pa, pb), 0.5);
            if fs.len() == 2 && smooth {
                ep[*ei as usize] = mul(add(add(pa, pb), add(fp[fs[0]], fp[fs[1]])), 0.25);
            } else {
                ep[*ei as usize] = mid;
                boundary_v[a as usize] = true;
                boundary_v[b as usize] = true;
                v_bsum[a as usize] = add(v_bsum[a as usize], pb);
                v_bsum[b as usize] = add(v_bsum[b as usize], pa);
            }
            for v in [a, b] {
                v_mid[v as usize] = add(v_mid[v as usize], mid);
                v_ne[v as usize] += 1;
            }
        }
        for (fi, f) in faces.iter().enumerate() {
            for &v in f {
                v_faces[v as usize] = add(v_faces[v as usize], fp[fi]);
                v_nf[v as usize] += 1;
            }
        }
        let mut np: Vec<V3> = (0..nv)
            .map(|v| {
                let pv = p[v];
                if v_nf[v] == 0 || !smooth {
                    pv
                } else if boundary_v[v] {
                    add(mul(pv, 0.75), mul(v_bsum[v], 0.125))
                } else {
                    let n = v_nf[v] as f32;
                    let f = mul(v_faces[v], 1.0 / n);
                    let r = mul(v_mid[v], 1.0 / v_ne[v].max(1) as f32);
                    mul(add(add(f, mul(r, 2.0)), mul(pv, n - 3.0)), 1.0 / n)
                }
            })
            .collect();
        let e_base = np.len() as u32;
        np.extend(ep);
        let f_base = np.len() as u32;
        np.extend(fp);
        let mut nfaces = Vec::with_capacity(quads);
        let mut norig = Vec::with_capacity(quads);
        let mut ncol = Vec::with_capacity(quads);
        for (fi, f) in faces.iter().enumerate() {
            let n = f.len();
            for i in 0..n {
                let prev = (i + n - 1) % n;
                let e_next = e_base + emap[&key(f[i], f[(i + 1) % n])].0;
                let e_prev = e_base + emap[&key(f[prev], f[i])].0;
                nfaces.push(vec![f[i], e_next, f_base + fi as u32, e_prev]);
                norig.push(vec![forig[fi][i], false, false, forig[fi][prev]]);
                ncol.push(fcol[fi]);
            }
        }
        p = np;
        faces = nfaces;
        forig = norig;
        fcol = ncol;
    }
    // Triangulation des quads, diagonale cachée
    let mut ot = Vec::with_capacity(faces.len() * 2);
    let mut oc = Vec::with_capacity(faces.len() * 2);
    let mut oe = Vec::with_capacity(faces.len() * 2);
    for ((f, o), c) in faces.iter().zip(&forig).zip(&fcol) {
        let show = |i: usize| -> u8 { (!optimal || o[i]) as u8 };
        let n = f.len();
        for k in 1..n - 1 {
            ot.push([f[0], f[k], f[k + 1]]);
            oc.push(*c);
            let mut m = show(k) << 1;
            if k == 1 {
                m |= show(0);
            }
            if k + 2 == n {
                m |= show(n - 1) << 2;
            }
            oe.push(m);
        }
    }
    (p, ot, oc, oe)
}

#[cfg(test)]
mod apercu_cc {
    use super::*;
    /// Rendu de contrôle : ARYADNE_CC_OBJ=chemin.obj cargo test apercu
    #[test]
    fn apercu() {
        let Ok(p) = std::env::var("ARYADNE_CC_OBJ") else { return };
        let m = load(std::path::Path::new(&p)).unwrap();
        for (i, (lv, opt)) in [(0u8, false), (1, false), (2, true)].iter().enumerate() {
            let mesh = crate::modifiers::Mesh::from_model(&m, false);
            let (pos, tris, colors, edges) = catmull_clark(&mesh.pos, &mesh.tris, &mesh.colors, &mesh.edges, *lv, *opt);
            let prep = Prepared::from_mesh(crate::modifiers::Mesh { pos, tris, colors, edges });
            let look = Look { material: Material::Gray, wire: 1.0, ..Default::default() };
            let f = render(&prep, &Camera { yaw: 0.6, pitch: 0.4, dist: 0.55, ..Default::default() }, &look, 700, 500);
            image::RgbaImage::from_raw(700, 500, f.rgba).unwrap().save(format!("/tmp/cc{i}.png")).unwrap();
        }
    }
}

#[cfg(test)]
mod apercu_materiaux {
    /// Rend chaque préréglage sur la sphère : aucun pixel invalide, et le verre laisse voir le fond.
    #[test]
    fn prereglages() {
        let sphere = super::preview_sphere();
        let mut means = std::collections::HashMap::new();
        for name in crate::nodes::MATERIAL_PRESETS {
            let g = crate::nodes::material_preset(name).unwrap();
            let prog = crate::nodes::Program::compile(&g, "", 64).ok().unwrap();
            let img = super::render_material_preview(&sphere, std::sync::Arc::new(prog), 96);
            assert!(img.px.iter().all(|p| p.iter().all(|x| x.is_finite())), "{name}");
            if let Ok(dir) = std::env::var("DUMP_PRESETS") {
                let bytes: Vec<u8> = img.px.iter().flat_map(|c| [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, 255]).collect();
                image::RgbaImage::from_raw(96, 96, bytes).unwrap().save(format!("{dir}/{name}.png")).unwrap();
            }
            let c = img.px[48 * 96 + 48];
            means.insert(name, (c[0] + c[1] + c[2]) / 3.0);
        }
        // Le néon éclaire, le toon et le verre ne sont pas noirs
        assert!(means["néon"] > 0.3 && means["verre"] > 0.05 && means["toon 3 bandes"] > 0.05);
    }
}

#[cfg(test)]
mod apercu_smart {
    #[test]
    fn metal_use_sur_un_modele() {
        let Ok(path) = std::env::var("SMART_MODEL") else { return };
        let m = super::preview_model(std::path::Path::new(&path)).unwrap();
        for (k, name) in ["métal peint usé", "pierre aux creux sombres"].iter().enumerate() {
            let prog = crate::nodes::Program::compile(&crate::nodes::material_preset(name).unwrap(), "", 64).ok().unwrap();
            let img = super::render_material_view(&m, std::sync::Arc::new(prog), 420, 0.7, 0.45, 2.2, 0);
            let bytes: Vec<u8> = img.px.iter().flat_map(|c| [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, 255]).collect();
            image::RgbaImage::from_raw(420, 420, bytes).unwrap().save(format!("/tmp/smart{k}.png")).unwrap();
        }
    }
}

#[cfg(test)]
mod ombres {
    use super::*;

    /// Un petit carré au-dessus d'un grand : avec les ombres, le grand s'assombrit sous le petit.
    #[test]
    fn ombre_portee_et_lumiere_ponctuelle() {
        let quad = |y: f32, r: f32, base: u32| (vec![[-r, y, -r], [r, y, -r], [r, y, r], [-r, y, r]], vec![[base, base + 2, base + 1], [base, base + 3, base + 2]]);
        let (mut pos, mut tris) = quad(0.0, 0.5, 0);
        let (p2, t2) = quad(0.2, 0.12, 4);
        pos.extend(p2);
        tris.extend(t2);
        let n = tris.len();
        let prep = Prepared::from_mesh_fixed(crate::modifiers::Mesh { pos, tris, colors: vec![Color32::from_gray(200); n], edges: vec![7; n] }, 0.0);
        let cam = Camera { yaw: 0.0, pitch: 1.2, dist: 2.0, ..Default::default() };
        let mut look = Look::default();
        look.env_strength = 0.05;
        look.lamps = vec![Lamp::simple(false, [0.0, 1.0, 0.0], [1.0; 3], 2.0, 0.0)];
        let lum = |f: &Frame| f.rgba.chunks(4).map(|p| p[0] as u64).sum::<u64>();
        let open = render(&prep, &cam, &look, 160, 160);
        look.shadows = true;
        let shaded = render(&prep, &cam, &look, 160, 160);
        let darker = open.rgba.chunks(4).zip(shaded.rgba.chunks(4)).filter(|(a, b)| a[0] as i32 > b[0] as i32 + 40).count();
        assert!(darker > 150 && darker < 4000, "{darker}");
        assert!(lum(&shaded) < lum(&open));
        // Ponctuelle : près du centre, elle éclaire plus le milieu du sol que ses coins, et rien hors de portée
        look.shadows = false;
        look.lamps = vec![Lamp::simple(true, [0.4, 0.05, 0.4], [1.0; 3], 3.0, 0.5)];
        let near = render(&prep, &cam, &look, 160, 160);
        look.lamps[0].at = [5.0, 5.0, 5.0];
        let far = render(&prep, &cam, &look, 160, 160);
        assert!(lum(&near) > lum(&far));
    }
}

#[cfg(test)]
mod fast_move_tests {
    use super::*;

    fn prep_of(sc: &crate::scene::Scene, lo: V3, hi: V3) -> (Prepared, Model, Vec<(usize, usize, usize)>) {
        let (m, _, ranges) = crate::scene::build_ranges(sc, std::path::Path::new("/tmp/x.aryscene"));
        let mut m = m.unwrap();
        (m.min, m.max) = (lo, hi);
        let s = m.size();
        let l = s[0].max(s[1]).max(s[2]).max(1e-9);
        let c = [(m.min[0] + m.max[0]) / 2.0, (m.min[1] + m.max[1]) / 2.0, (m.min[2] + m.max[2]) / 2.0];
        let mesh = crate::modifiers::apply(&[], crate::modifiers::Mesh::from_model(&m, false), [-c[0] / l, -c[1] / l, -c[2] / l]);
        (Prepared::from_mesh_fixed(mesh, 0.0), m, ranges)
    }

    /// Déplacer, tourner et agrandir un objet sur place donne les mêmes sommets qu'une reconstruction
    #[test]
    fn deplacement_rapide_identique() {
        let mut sc = crate::scene::Scene::default();
        sc.objects.push(crate::scene::Object::new("a".into(), "@cube".into()));
        sc.objects.push(crate::scene::Object::new("b".into(), "@sphere".into()));
        sc.objects[1].pos = [2.0, 0.0, 0.0];
        let (lo, hi) = ([-5.0; 3], [5.0; 3]);
        let (p0, m0, r0) = prep_of(&sc, lo, hi);
        let mut sc2 = sc.clone();
        sc2.objects[1].pos = [1.0, 2.0, -1.0];
        sc2.objects[1].rot = [30.0, 45.0, 10.0];
        sc2.objects[1].scale = [1.5, 0.5, 2.0];
        let fast = fast_move(&sc, &sc2, &r0, &p0, &m0).expect("déplacement rapide");
        let (full, _, _) = prep_of(&sc2, lo, hi);
        // Les coins de chaque triangle (les sommets peuvent être dédoublés autrement aux arêtes vives)
        assert_eq!(fast.tris.len(), full.tris.len());
        for (ta, tb) in fast.tris.iter().zip(&full.tris) {
            for c in 0..3 {
                let (a, b) = (fast.pos[ta[c] as usize], full.pos[tb[c] as usize]);
                for k in 0..3 {
                    assert!((a[k] - b[k]).abs() < 1e-4, "{a:?} {b:?}");
                }
            }
        }
        // Un autre changement que la pose : pas de raccourci
        let mut sc3 = sc.clone();
        sc3.objects[0].visible = false;
        assert!(fast_move(&sc, &sc3, &r0, &p0, &m0).is_none());
    }
    #[test]
    fn inverse_3x3() {
        let m = [[2.0, 0.0, 1.0], [0.5, 3.0, 0.0], [0.0, 1.0, 1.5]];
        let i = inv3(m).unwrap();
        for v in [[1.0, 2.0, 3.0], [-0.5, 0.25, 4.0]] {
            let back = mulm(&i, mulm(&m, v));
            for k in 0..3 {
                assert!((back[k] - v[k]).abs() < 1e-4, "{back:?} {v:?}");
            }
        }
    }
}
