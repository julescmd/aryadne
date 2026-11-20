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

pub const EXT: [&str; 5] = ["obj", "fbx", "glb", "gltf", "stl"];
/// Nombre de triangles visé pour l'affichage interactif.
const MAX_TRIS: usize = 150_000;

pub fn is_model(rel: &str) -> bool {
    let e = rel.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    EXT.contains(&e.as_str())
}

type V3 = [f32; 3];

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

    fn with_edges(positions: Vec<V3>, tris: Vec<[u32; 3]>, colors: Option<Vec<Color32>>, edges: Option<Vec<u8>>, z_up: bool) -> Result<Self, String> {
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
        m.simplify(MAX_TRIS);
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
        "fbx" => {
            let m = crate::fbx::load(&std::fs::read(path).map_err(|e| e.to_string())?)?;
            Model::with_edges(m.positions, m.tris, None, Some(m.edges), false)
        }
        "glb" | "gltf" => load_gltf(path),
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

/// Subdivision de Loop (lisse) : chaque triangle devient 4.
pub fn subdivide(pos: &[V3], tris: &[[u32; 3]], colors: &[Color32], edges: &[u8]) -> (Vec<V3>, Vec<[u32; 3]>, Vec<Color32>, Vec<u8>) {
    let key = |a: u32, b: u32| if a < b { (a, b) } else { (b, a) };
    // Arête → (sommets opposés)
    let mut opp: HashMap<(u32, u32), Vec<u32>> = HashMap::with_capacity(tris.len() * 2);
    for t in tris {
        for k in 0..3 {
            opp.entry(key(t[k], t[(k + 1) % 3])).or_default().push(t[(k + 2) % 3]);
        }
    }
    let mut neigh: Vec<Vec<u32>> = vec![vec![]; pos.len()];
    let mut boundary = vec![false; pos.len()];
    for (&(a, b), o) in &opp {
        neigh[a as usize].push(b);
        neigh[b as usize].push(a);
        if o.len() == 1 {
            boundary[a as usize] = true;
            boundary[b as usize] = true;
        }
    }
    // Nouvelles positions des anciens sommets
    let mut out: Vec<V3> = pos
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let ns = &neigh[i];
            if ns.is_empty() {
                return *v;
            }
            if boundary[i] {
                let b: Vec<&u32> = ns.iter().filter(|&&j| opp.get(&key(i as u32, j)).is_some_and(|o| o.len() == 1)).collect();
                if b.len() == 2 {
                    let (p, q) = (pos[*b[0] as usize], pos[*b[1] as usize]);
                    return [0.75 * v[0] + 0.125 * (p[0] + q[0]), 0.75 * v[1] + 0.125 * (p[1] + q[1]), 0.75 * v[2] + 0.125 * (p[2] + q[2])];
                }
                return *v;
            }
            let n = ns.len() as f32;
            let beta = if ns.len() == 3 { 3.0 / 16.0 } else { 3.0 / (8.0 * n) };
            let mut s = [0.0; 3];
            for &j in ns {
                let q = pos[j as usize];
                s[0] += q[0];
                s[1] += q[1];
                s[2] += q[2];
            }
            [v[0] * (1.0 - n * beta) + s[0] * beta, v[1] * (1.0 - n * beta) + s[1] * beta, v[2] * (1.0 - n * beta) + s[2] * beta]
        })
        .collect();
    // Sommets au milieu des arêtes
    let mut mid: HashMap<(u32, u32), u32> = HashMap::with_capacity(opp.len());
    for (&(a, b), o) in &opp {
        let (p, q) = (pos[a as usize], pos[b as usize]);
        let v = if o.len() == 2 {
            let (c, d) = (pos[o[0] as usize], pos[o[1] as usize]);
            [0.375 * (p[0] + q[0]) + 0.125 * (c[0] + d[0]), 0.375 * (p[1] + q[1]) + 0.125 * (c[1] + d[1]), 0.375 * (p[2] + q[2]) + 0.125 * (c[2] + d[2])]
        } else {
            [(p[0] + q[0]) * 0.5, (p[1] + q[1]) * 0.5, (p[2] + q[2]) * 0.5]
        };
        out.push(v);
        mid.insert((a, b), (out.len() - 1) as u32);
    }
    let mut nt = Vec::with_capacity(tris.len() * 4);
    let mut nc = Vec::with_capacity(tris.len() * 4);
    let mut ne = Vec::with_capacity(tris.len() * 4);
    for ((t, c), &e) in tris.iter().zip(colors).zip(edges) {
        let m01 = mid[&key(t[0], t[1])];
        let m12 = mid[&key(t[1], t[2])];
        let m20 = mid[&key(t[2], t[0])];
        let (b0, b1, b2) = (e & 1, (e >> 1) & 1, (e >> 2) & 1);
        // Arêtes d'origine gardées. Une arête interne qui tourne autour d'un sommet n'est
        // visible que si le côté opposé à ce sommet l'est : un quad subdivisé reste une
        // grille de quads, sans diagonales.
        nt.push([t[0], m01, m20]);
        ne.push(b0 | (b1 << 1) | (b2 << 2));
        nt.push([m01, t[1], m12]);
        ne.push(b0 | (b1 << 1) | (b2 << 2));
        nt.push([m20, m12, t[2]]);
        ne.push(b0 | (b1 << 1) | (b2 << 2));
        nt.push([m01, m12, m20]);
        ne.push(b2 | (b0 << 1) | (b1 << 2));
        for _ in 0..4 {
            nc.push(*c);
        }
    }
    (out, nt, nc, ne)
}

/// Données prêtes pour le rendu (modèle centré, mis à l'échelle, normales).
pub struct Prepared {
    pos: Vec<V3>,
    nrm: Vec<V3>,
    tris: Vec<[u32; 3]>,
    colors: Vec<[f32; 3]>,
    edges: Vec<u8>,
    pub floor: f32,
}

impl Prepared {
    pub fn new(model: &Model, z_up: bool, levels: u8) -> Self {
        let mut pos = model.normalized(z_up);
        let mut tris = model.tris.clone();
        let mut colors = model.colors.clone();
        let mut edges = model.edges.clone();
        for _ in 0..levels {
            if tris.len() * 4 > MAX_SUBDIV_TRIS {
                break;
            }
            let r = subdivide(&pos, &tris, &colors, &edges);
            (pos, tris, colors, edges) = r;
        }
        let nrm = vertex_normals(&pos, &tris);
        let floor = pos.iter().map(|v| v[1]).fold(f32::MAX, f32::min);
        let lin = |c: u8| (c as f32 / 255.0).powf(2.2);
        let colors = colors.iter().map(|c| [lin(c.r()), lin(c.g()), lin(c.b())]).collect();
        Self { pos, nrm, tris, colors, edges, floor }
    }
    pub fn tri_count(&self) -> usize {
        self.tris.len()
    }

    /// Depuis un maillage modifié : recentré et remis à l'échelle pour tenir dans la vue.
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
        Self { pos, nrm, tris: m.tris, colors, edges: m.edges, floor }
    }
}

pub const MAX_SUBDIV_TRIS: usize = 2_500_000;

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
        }
    }
}

impl Look {
    /// Clé de comparaison : l'image n'est recalculée que si elle change.
    fn key(&self) -> (Material, bool, usize, [u32; 4], Light, bool, bool, [u32; 4], bool, usize) {
        (
            self.material,
            self.smooth,
            std::sync::Arc::as_ptr(&self.env) as usize,
            [self.env_rot.to_bits(), self.env_strength.to_bits(), self.exposure.to_bits(), 0],
            self.light,
            self.show_bg,
            self.blur_bg,
            [self.wire.to_bits(), self.wire_color[0].to_bits(), self.wire_color[1].to_bits(), self.wire_color[2].to_bits()],
            self.wire_only,
            self.nodes.as_ref().map(|n| std::sync::Arc::as_ptr(n) as usize).unwrap_or(0),
        )
    }
}

pub struct Frame {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
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
    let (albedo0, rough, metal) = look.material.params();
    let wire = look.wire;

    let band = 16usize;
    let mut rgba = vec![0u8; w * h * 4];
    rgba.par_chunks_mut(w * 4 * band).enumerate().for_each(|(bi, chunk)| {
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
                if let Some(prog) = &look.nodes {
                    let pos = [pa[0] * w0 + pb[0] * w1 + pc[0] * w2, pa[1] * w0 + pb[1] * w1 + pc[1] * w2, pa[2] * w0 + pb[2] * w1 + pc[2] * w2];
                    let vc = m.colors[ti as usize];
                    let g = |x: f32| x.max(0.0).powf(1.0 / 2.2);
                    let c = crate::nodes::Ctx { uv: [pos[0] + 0.5, 0.5 - pos[1]], pos, nrm, view: v, vcol: [g(vc[0]), g(vc[1]), g(vc[2]), 1.0], res: [w as f32, h as f32] };
                    let (col, r, mt, em) = NODE_REGS.with(|regs| prog.material(&c, &mut regs.borrow_mut()));
                    let l = |x: f32| x.clamp(0.0, 1.0).powf(2.2);
                    albedo = [l(col[0]), l(col[1]), l(col[2])];
                    rough = r;
                    metal = mt;
                    emit = [em[0].max(0.0).powf(2.2), em[1].max(0.0).powf(2.2), em[2].max(0.0).powf(2.2)];
                }
                let albedo = if metal > 0.5 && look.nodes.is_some() { albedo } else { albedo };
                let ndv = dot(nrm, v).clamp(0.0, 1.0);
                let ne = rot_y(nrm, -look.env_rot);
                let s = look.env_strength;
                let irr = env.irradiance(ne);
                let r = sub([2.0 * dot(nrm, v) * nrm[0], 2.0 * dot(nrm, v) * nrm[1], 2.0 * dot(nrm, v) * nrm[2]], v);
                let refl = env.reflect(rot_y(r, -look.env_rot), rough);
                let f0 = if look.nodes.is_some() {
                    [0.04 + (albedo[0] - 0.04) * metal, 0.04 + (albedo[1] - 0.04) * metal, 0.04 + (albedo[2] - 0.04) * metal]
                } else if metal > 0.5 {
                    [0.95, 0.93, 0.88]
                } else {
                    [0.04; 3]
                };
                let fres = (1.0 - ndv).powf(5.0);
                let ndl = dot(nrm, ld).max(0.0);
                let half = norm([ld[0] + v[0], ld[1] + v[1], ld[2] + v[2]]);
                let shin = 2.0 / (rough * rough).max(0.002) - 2.0;
                let spec_l = dot(nrm, half).max(0.0).powf(shin) * (shin + 8.0) / 25.0;
                let li = look.light.intensity;
                let lc = look.light.color;
                let mut out = [0.0f32; 3];
                for ch in 0..3 {
                    let f = f0[ch] + (1.0 - f0[ch]) * fres * (1.0 - rough).powf(2.0);
                    let diff = albedo[ch] * (irr[ch] * s + lc[ch] * li * ndl) * (1.0 - metal);
                    let spec = (refl[ch] * s + lc[ch] * li * spec_l * 0.25) * f;
                    out[ch] = diff * (1.0 - f) + spec + emit[ch];
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
    });
    Frame { w, h, rgba }
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

/// Rendu de la sphère avec un matériau en nodes (pour l'aperçu de l'éditeur).
pub fn render_material_preview(sphere: &Prepared, prog: std::sync::Arc<crate::nodes::Program>, size: usize) -> crate::nodes::Img {
    let look = Look { nodes: Some(prog), show_bg: true, blur_bg: true, env: Env::preset(0), ..Default::default() };
    let f = render(sphere, &Camera { yaw: 0.4, pitch: 0.25, dist: 2.0, ..Default::default() }, &look, size, size);
    let px = f.rgba.chunks_exact(4).map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, c[3] as f32 / 255.0]).collect();
    crate::nodes::Img { w: size, h: size, px }
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
    pub root: String,
    mat_active: Option<(std::path::PathBuf, Option<std::time::SystemTime>)>,
    mat_error: Option<String>,
    env_loading: Option<Receiver<Result<Env, String>>>,
    env_error: Option<String>,
    /// Pile de modificateurs (enregistrée dans .app/modifiers/ du coffre)
    stack: Vec<crate::modifiers::Modifier>,
    stack_loaded: bool,
    show_mods: bool,
    /// Calcul en cours : (clé, résultat)
    job: Option<(u64, Receiver<Prepared>)>,
    turntable: bool,
    pub z_up: Option<bool>,
    prepared: Option<(bool, u64, Prepared)>,
    tex: Option<egui::TextureHandle>,
    last: Option<(Camera, [usize; 2], bool, u64, String)>,
    capture_msg: Option<String>,
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
            root: String::new(),
            mat_active: None,
            mat_error: None,
            env_loading: None,
            env_error: None,
            stack: vec![],
            stack_loaded: false,
            show_mods: false,
            job: None,
            turntable: false,
            z_up: None,
            prepared: None,
            tex: None,
            last: None,
            capture_msg: None,
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

    fn capture(&mut self) {
        let Some((_, _, prep)) = &self.prepared else { return };
        let mut look = self.look.clone();
        if !look.show_bg {
            look.show_bg = false;
        }
        let f = render(prep, &self.cam, &look, 2400, 2400);
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

    fn mods_file(&self) -> std::path::PathBuf {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.path.hash(&mut h);
        std::path::Path::new(&self.root).join(".app/modifiers").join(format!("{:016x}.json", h.finish()))
    }

    /// Tour complet du modèle en GIF animé (48 images, 520 px).
    fn turntable_gif(&mut self) {
        let Some((_, _, prep)) = &self.prepared else { return };
        let size = 520;
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "modele".into());
        let out = self.path.with_file_name(format!("{stem}-turntable.gif"));
        let r = (|| -> Result<(), String> {
            let file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            let mut enc = image::codecs::gif::GifEncoder::new_with_speed(file, 10);
            enc.set_repeat(image::codecs::gif::Repeat::Infinite).map_err(|e| e.to_string())?;
            let mut look = self.look.clone();
            look.show_bg = true;
            for k in 0..48 {
                let cam = Camera { yaw: self.cam.yaw + k as f32 / 48.0 * std::f32::consts::TAU, ..self.cam };
                let f = render(prep, &cam, &look, size, size);
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

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        if let State::Loading(rx) = &self.state {
            if let Ok(r) = rx.try_recv() {
                self.state = match r {
                    Ok(m) => State::Ready(std::sync::Arc::new(m)),
                    Err(e) => State::Failed(e),
                };
            }
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
                    ui.label(egui::RichText::new("chargement du modèle… (les gros fichiers peuvent prendre quelques secondes)").color(p.muted));
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
        if !self.stack_loaded && !self.root.is_empty() {
            self.stack = std::fs::read_to_string(self.mods_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
            self.stack_loaded = true;
        }
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_string(&self.stack.iter().filter(|m| m.on).map(|m| m.kind).collect::<Vec<_>>()).unwrap_or_default().hash(&mut h);
            z_up.hash(&mut h);
            h.finish()
        };
        if let Some((k, rx)) = &self.job {
            if let Ok(p) = rx.try_recv() {
                self.prepared = Some((z_up, *k, p));
                self.last = None;
                self.job = None;
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
            }
        }
        let current = self.prepared.as_ref().map(|p| p.1);
        let pending = self.job.as_ref().map(|j| j.0);
        if current != Some(key) && pending != Some(key) && self.job.is_none() {
            let (tx, rx) = channel();
            let (m, stack, ctx) = (model.clone(), self.stack.clone(), ui.ctx().clone());
            std::thread::spawn(move || {
                let s = m.size();
                let longest = s[0].max(s[1]).max(s[2]).max(1e-9);
                let c = [(m.min[0] + m.max[0]) / 2.0, (m.min[1] + m.max[1]) / 2.0, (m.min[2] + m.max[2]) / 2.0];
                let o = [-c[0] / longest, -c[1] / longest, -c[2] / longest];
                let origin = if z_up { [o[0], o[2], -o[1]] } else { o };
                let mesh = crate::modifiers::apply(&stack, crate::modifiers::Mesh::from_model(&m, z_up), origin);
                let _ = tx.send(Prepared::from_mesh(mesh));
                ctx.request_repaint();
            });
            self.job = Some((key, rx));
        }
        if self.prepared.is_none() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("préparation du modèle…");
            });
            return;
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
        let mut pick_hdr: Option<std::path::PathBuf> = None;
        let mut do_capture = false;
        let mut do_gif = false;

        // ---------- barre d'outils : trois menus et quelques boutons ----------
        let stay = egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
        ui.horizontal(|ui| {
            egui::containers::menu::MenuButton::new("affichage ▾").config(stay.clone()).ui(ui, |ui| {
                ui.set_min_width(240.0);
                ui.label(egui::RichText::new("matériau").color(p.muted).size(11.0));
                for (m, l) in Material::ALL {
                    if ui.radio(self.look.material == m && self.mat_active.is_none(), l).clicked() {
                        self.look.material = m;
                        self.mat_active = None;
                        self.look.nodes = None;
                    }
                }
                if !self.mat_files.is_empty() {
                    ui.label(egui::RichText::new("matériaux en nodes").color(p.muted).size(11.0));
                    for (name, path) in self.mat_files.clone() {
                        let on = self.mat_active.as_ref().is_some_and(|m| m.0 == path);
                        if ui.radio(on, name).clicked() {
                            self.mat_active = Some((path, None));
                        }
                    }
                } else {
                    ui.label(egui::RichText::new("crée un « matériau en nodes » (menu du coffre) pour le voir ici").color(p.muted).size(10.5));
                }
                if let Some(e) = &self.mat_error {
                    ui.label(egui::RichText::new(e).color(p.missing).size(10.5));
                }
                ui.separator();
                ui.checkbox(&mut self.look.smooth, "ombrage lisse");
                ui.label(egui::RichText::new("subdivision, miroir, torsion… : bouton « modificateurs »").color(p.muted).size(10.5));
                ui.separator();
                ui.label(egui::RichText::new("fil de fer").color(p.muted).size(11.0));
                ui.add(egui::Slider::new(&mut self.look.wire, 0.0..=1.0).text("opacité"));
                ui.horizontal(|ui| {
                    for (c, l) in [([0.02, 0.02, 0.02], "noir"), ([1.0, 1.0, 1.0], "blanc"), (lin_accent(p.accent), "accent")] {
                        if ui.selectable_label(self.look.wire_color == c, l).clicked() {
                            self.look.wire_color = c;
                            if self.look.wire == 0.0 {
                                self.look.wire = 0.6;
                            }
                        }
                    }
                });
                ui.checkbox(&mut self.look.wire_only, "fil de fer seul");
                ui.separator();
                let mut z = z_up;
                if ui.checkbox(&mut z, "axe Z vers le haut").changed() {
                    self.z_up = Some(z);
                }
                ui.checkbox(&mut self.cam.ortho, "vue orthographique (5)");
            });
            egui::containers::menu::MenuButton::new("éclairage ▾").config(stay.clone()).ui(ui, |ui| {
                ui.set_min_width(280.0);
                ui.label(egui::RichText::new("environnement (HDRI)").color(p.muted).size(11.0));
                for (i, name) in PRESETS.iter().enumerate() {
                    if ui.selectable_label(self.look.env.name == *name, *name).clicked() {
                        self.look.env = Env::preset(i);
                    }
                }
                for (name, path) in self.hdr_files.clone() {
                    if ui.selectable_label(self.look.env.name == std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), format!("{name}")).clicked() {
                        pick_hdr = Some(path);
                    }
                }
                if ui.button("charger un .hdr / .exr…").clicked() {
                    pick_hdr = rfd::FileDialog::new().add_filter("HDRI", &["hdr", "exr"]).pick_file();
                }
                if self.env_loading.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("chargement du HDRI…");
                    });
                }
                if let Some(e) = &self.env_error {
                    ui.label(egui::RichText::new(e).color(p.missing).size(11.0));
                }
                ui.add(egui::Slider::new(&mut self.look.env_rot, -std::f32::consts::PI..=std::f32::consts::PI).text("rotation (H + glisser)").show_value(false));
                ui.add(egui::Slider::new(&mut self.look.env_strength, 0.0..=3.0).text("intensité"));
                ui.add(egui::Slider::new(&mut self.look.exposure, -4.0..=4.0).text("exposition (EV)"));
                ui.checkbox(&mut self.look.show_bg, "afficher le HDRI en fond");
                ui.add_enabled(self.look.show_bg, egui::Checkbox::new(&mut self.look.blur_bg, "fond flou"));
                ui.separator();
                ui.label(egui::RichText::new("lumière directionnelle (L + glisser)").color(p.muted).size(11.0));
                ui.add(egui::Slider::new(&mut self.look.light.intensity, 0.0..=5.0).text("intensité"));
                ui.add(egui::Slider::new(&mut self.look.light.azimuth, -std::f32::consts::PI..=std::f32::consts::PI).text("direction").show_value(false));
                ui.add(egui::Slider::new(&mut self.look.light.elevation, -1.5..=1.5).text("hauteur").show_value(false));
                ui.horizontal(|ui| {
                    ui.label("couleur");
                    let mut c = self.look.light.color;
                    if ui.color_edit_button_rgb(&mut c).changed() {
                        self.look.light.color = c;
                    }
                });
                if ui.button("réinitialiser l'éclairage").clicked() {
                    let mat = self.look.material;
                    let (wire, wc) = (self.look.wire, self.look.wire_color);
                    self.look = Look { material: mat, wire, wire_color: wc, ..Default::default() };
                }
            });
            egui::containers::menu::MenuButton::new("vue ▾").config(stay).ui(ui, |ui| {
                let half = std::f32::consts::FRAC_PI_2;
                for (label, yaw, pitch) in [("face (1)", 0.0, 0.0), ("arrière (Maj+1)", std::f32::consts::PI, 0.0), ("droite (3)", -half, 0.0), ("gauche (Maj+3)", half, 0.0), ("dessus (7)", 0.0, half), ("dessous (Maj+7)", 0.0, -half)] {
                    if ui.button(label).clicked() {
                        self.cam.yaw = yaw;
                        self.cam.pitch = pitch;
                    }
                }
                ui.separator();
                if ui.button("cadrer (Origine ou .)").clicked() {
                    self.cam = Camera { ortho: self.cam.ortho, ..Default::default() };
                }
            });
            ui.toggle_value(&mut self.turntable, "tourner").on_hover_text("Rotation automatique (platine)");
            let n_mods = self.stack.iter().filter(|m| m.on).count();
            let label = if n_mods > 0 { format!("modificateurs ({n_mods})") } else { "modificateurs".into() };
            ui.toggle_value(&mut self.show_mods, label).on_hover_text("Pile de modificateurs, comme dans Blender et 3ds Max");
            if self.job.is_some() {
                ui.spinner();
            }
            egui::containers::menu::MenuButton::new("capture ▾").ui(ui, |ui| {
                if ui.button("image PNG (2400 px)").clicked() {
                    do_capture = true;
                }
                if ui.button("turntable GIF (48 images)").clicked() {
                    do_gif = true;
                }
                ui.label(egui::RichText::new("enregistré à côté du modèle").color(p.muted).size(10.5));
            });
            let s = model.size();
            let shown = if shown_tris != model.total_tris { format!(" · affiché : {}", fmt_n(shown_tris)) } else { String::new() };
            let text = format!("{} triangles{shown} · {} sommets · {:.2} × {:.2} × {:.2}", fmt_n(model.total_tris), fmt_n(model.total_vertices), s[0], s[1], s[2]);
            ui.add(egui::Label::new(egui::RichText::new(text).color(p.muted).size(11.5)).truncate());
        });
        if let Some(path) = pick_hdr {
            self.load_env(path, &ctx);
        }
        if do_capture {
            self.capture();
        }
        if do_gif {
            self.turntable_gif();
        }

        if self.show_mods {
            let avail = ui.available_rect_before_wrap();
            let pr = egui::Rect::from_min_max(egui::pos2(avail.right() - 280.0, avail.top()), avail.max);
            let mut changed = false;
            let mut export = false;
            {
                let ui = &mut ui.new_child(egui::UiBuilder::new().max_rect(pr));
                egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).inner_margin(8).show(ui, |ui| {
                    ui.set_min_size(pr.size() - egui::vec2(16.0, 16.0));
                    egui::ScrollArea::vertical().id_salt("mods").show(ui, |ui| {
                        changed = crate::modifiers::panel(ui, &mut self.stack);
                        ui.add_space(8.0);
                        if !self.stack.is_empty() && ui.button("exporter le résultat en OBJ").on_hover_text("Enregistre le maillage modifié à côté du fichier").clicked() {
                            export = true;
                        }
                    });
                });
            }
            if changed && !self.root.is_empty() {
                let f = self.mods_file();
                let _ = std::fs::create_dir_all(f.parent().unwrap());
                let _ = std::fs::write(f, serde_json::to_string(&self.stack).unwrap_or_default());
            }
            if export {
                let s = model.size();
                let longest = s[0].max(s[1]).max(s[2]).max(1e-9);
                let c = [(model.min[0] + model.max[0]) / 2.0, (model.min[1] + model.max[1]) / 2.0, (model.min[2] + model.max[2]) / 2.0];
                let o = [-c[0] / longest, -c[1] / longest, -c[2] / longest];
                let origin = if z_up { [o[0], o[2], -o[1]] } else { o };
                let mesh = crate::modifiers::apply(&self.stack, crate::modifiers::Mesh::from_model(model, z_up), origin);
                let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "modele".into());
                let out = self.path.with_file_name(format!("{stem}-modifié.obj"));
                self.capture_msg = Some(match crate::modifiers::export_obj(&mesh, model, z_up, &out) {
                    Ok(()) => format!("exporté : {}", out.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
                    Err(e) => format!("export impossible : {e}"),
                });
            }
        }
        let avail = ui.available_size() - if self.show_mods { egui::vec2(288.0, 0.0) } else { egui::Vec2::ZERO };
        let (rect, resp) = ui.allocate_exact_size(avail.max(egui::vec2(50.0, 50.0)), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, p.panel);

        // ---------- navigation façon Blender ----------
        let (mods, lkey, hkey) = ui.input(|i| (i.modifiers, i.key_down(Key::L), i.key_down(Key::H)));
        let dragging = resp.dragged_by(egui::PointerButton::Middle) || resp.dragged_by(egui::PointerButton::Primary) || resp.dragged_by(egui::PointerButton::Secondary);
        if dragging {
            let d = resp.drag_delta();
            let right = resp.dragged_by(egui::PointerButton::Secondary);
            if lkey {
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
            let keys = ui.input(|i| [Key::Num1, Key::Num3, Key::Num7, Key::Num5, Key::Home, Key::Period].map(|k| i.key_pressed(k) && !i.modifiers.command));
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
        let heavy = shown_tris > 600_000;
        let scale = if interacting { if heavy { 0.4 } else { 0.6 } } else { ppp.min(2.0) };
        let size = [(rect.width() * scale).max(16.0) as usize, (rect.height() * scale).max(16.0) as usize];
        let look_key = format!("{:?}", self.look.key());
        let key = (self.cam, size, z_up, self.prepared.as_ref().map(|p| p.1).unwrap_or(0), look_key);
        if self.last.as_ref() != Some(&key) || self.tex.is_none() {
            let prep = &self.prepared.as_ref().unwrap().2;
            let frame = render(prep, &self.cam, &self.look, size[0], size[1]);
            let img = egui::ColorImage::from_rgba_unmultiplied([frame.w, frame.h], &frame.rgba);
            match &mut self.tex {
                Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                None => self.tex = Some(ui.ctx().load_texture("model3d", img, egui::TextureOptions::LINEAR)),
            }
            self.last = Some(key);
        }
        if interacting {
            ui.ctx().request_repaint();
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

        // Repère des axes (en bas à gauche)
        let o = rect.left_bottom() + vec2(36.0, -40.0);
        painter.circle_filled(o, 30.0, p.panel.gamma_multiply(0.6));
        for (axis, col, label) in [([1.0, 0.0, 0.0], Color32::from_rgb(230, 70, 70), "X"), ([0.0, 1.0, 0.0], Color32::from_rgb(110, 200, 80), if z_up { "Z" } else { "Y" }), ([0.0, 0.0, 1.0], Color32::from_rgb(80, 140, 240), if z_up { "-Y" } else { "Z" })] {
            let r = self.cam.rotate(axis);
            let end = o + vec2(r[0], -r[1]) * 22.0;
            painter.line_segment([o, end], Stroke::new(2.0, col));
            painter.text(end, egui::Align2::CENTER_CENTER, label, egui::FontId::monospace(10.0), col);
        }
        let hint = self.capture_msg.clone().unwrap_or_else(|| "glisser : tourner · Maj / clic droit : déplacer · molette : zoom · L : lumière · H : HDRI · 1 3 7 : vues".into());
        painter.text(rect.right_bottom() + vec2(-12.0, -10.0), egui::Align2::RIGHT_BOTTOM, hint, egui::FontId::monospace(10.5), if self.look.show_bg { Color32::from_gray(220) } else { p.muted });
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
