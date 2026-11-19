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
        let tris: Vec<[u32; 3]> = tris.into_iter().filter(|t| t.iter().all(|&i| (i as usize) < positions.len())).collect();
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
        let mut m = Self { positions, tris, colors, min, max, total_tris, total_vertices, z_up };
        m.simplify(MAX_TRIS);
        Ok(m)
    }

    pub fn size(&self) -> V3 {
        sub(self.max, self.min)
    }

    /// Simplification par grille : tous les sommets d'une même case fusionnent.
    /// On resserre la grille jusqu'à passer sous `target` triangles.
    fn simplify(&mut self, target: usize) {
        if self.tris.len() <= target {
            return;
        }
        let size = self.size();
        let longest = size[0].max(size[1]).max(size[2]).max(1e-9);
        let mut cells = 600.0_f32;
        loop {
            let cell = longest / cells;
            let key = |v: &V3| -> (i32, i32, i32) {
                (((v[0] - self.min[0]) / cell) as i32, ((v[1] - self.min[1]) / cell) as i32, ((v[2] - self.min[2]) / cell) as i32)
            };
            let mut ids: HashMap<(i32, i32, i32), u32> = HashMap::new();
            let mut sums: Vec<([f64; 3], u32)> = vec![];
            let remap: Vec<u32> = self
                .positions
                .iter()
                .map(|v| {
                    let k = key(v);
                    let id = *ids.entry(k).or_insert_with(|| {
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
            for (t, c) in self.tris.iter().zip(&self.colors) {
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
            if tris.len() <= target || cells < 20.0 {
                self.positions = sums.iter().map(|(s, n)| [(s[0] / *n as f64) as f32, (s[1] / *n as f64) as f32, (s[2] / *n as f64) as f32]).collect();
                self.tris = tris;
                self.colors = colors;
                return;
            }
            cells *= (target as f32 / tris.len() as f32).sqrt().clamp(0.4, 0.92);
        }
    }

    /// Sommets centrés, mis à l'échelle (taille 1) et remis en Y vers le haut.
    fn normalized(&self, z_up: bool) -> Vec<V3> {
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

// ---------- lecture des formats ----------

pub fn load(path: &Path) -> Result<Model, String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "obj" => load_obj(std::io::BufReader::new(std::fs::File::open(path).map_err(|e| e.to_string())?)),
        "stl" => load_stl(&std::fs::read(path).map_err(|e| e.to_string())?),
        "fbx" => {
            let m = crate::fbx::load(&std::fs::read(path).map_err(|e| e.to_string())?)?;
            Model::new(m.positions, m.tris, None, false)
        }
        "glb" | "gltf" => load_gltf(path),
        _ => Err("Format 3D non pris en charge.".into()),
    }
}

/// OBJ lu ligne par ligne (les fichiers de plusieurs Go ne sont pas chargés d'un bloc).
pub fn load_obj(reader: impl BufRead) -> Result<Model, String> {
    let mut verts: Vec<V3> = vec![];
    let mut tris = vec![];
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
            for k in 1..poly.len().saturating_sub(1) {
                tris.push([poly[0], poly[k], poly[k + 1]]);
            }
        }
    }
    Model::new(verts, tris, None, false)
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
// puis affiché comme une texture. Pas de tri de triangles, pas de trous, et on ne
// recalcule l'image que quand la vue, la lumière ou la taille changent.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Material {
    Clay,
    Gray,
    White,
    Normals,
    Color,
}

impl Material {
    pub const ALL: [(Material, &'static str); 5] =
        [(Material::Clay, "argile"), (Material::Gray, "gris"), (Material::White, "blanc"), (Material::Normals, "normales"), (Material::Color, "couleurs")];
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
    pub ambient: f32,
}

impl Default for Light {
    fn default() -> Self {
        Self { azimuth: 0.6, elevation: 0.7, ambient: 0.28 }
    }
}

impl Light {
    fn dir(&self) -> V3 {
        let (sa, ca) = self.azimuth.sin_cos();
        let (se, ce) = self.elevation.sin_cos();
        norm([sa * ce, se, ca * ce])
    }
}

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

/// Données prêtes pour le rendu (modèle centré, mis à l'échelle, normales).
pub struct Prepared {
    pos: Vec<V3>,
    nrm: Vec<V3>,
    tris: Vec<[u32; 3]>,
    colors: Vec<Color32>,
    pub floor: f32,
}

impl Prepared {
    pub fn new(model: &Model, z_up: bool) -> Self {
        let pos = model.normalized(z_up);
        let nrm = vertex_normals(&pos, &model.tris);
        let floor = pos.iter().map(|v| v[1]).fold(f32::MAX, f32::min);
        Self { pos, nrm, tris: model.tris.clone(), colors: model.colors.clone(), floor }
    }
}

pub struct Frame {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

/// Rendu z-buffer en parallèle (une bande d'image par cœur).
pub fn render(m: &Prepared, cam: &Camera, light: &Light, mat: Material, smooth: bool, w: usize, h: usize, bg: [u8; 4]) -> Frame {
    use rayon::prelude::*;
    let focal = w.min(h) as f32 * 1.2;
    let ortho_scale = focal / cam.dist;
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    // Sommets → écran (x, y, profondeur) + normale en espace caméra.
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
    let ld = cam.rotate(light.dir());
    let shade_n = |n: V3, base: Color32| -> [f32; 3] {
        let n = cam.rotate(n);
        let n = if n[2] < 0.0 { [-n[0], -n[1], -n[2]] } else { n };
        match mat {
            Material::Normals => [(n[0] * 0.5 + 0.5) * 255.0, (n[1] * 0.5 + 0.5) * 255.0, (n[2] * 0.5 + 0.5) * 255.0],
            _ => {
                let base = match mat {
                    Material::Clay => [196.0, 120.0, 98.0],
                    Material::Gray => [175.0, 175.0, 178.0],
                    Material::White => [235.0, 233.0, 228.0],
                    _ => [base.r() as f32, base.g() as f32, base.b() as f32],
                };
                let diff = dot(n, ld).max(0.0);
                // Reflet (Blinn) et léger contour, comme un matcap.
                let half = norm([ld[0], ld[1], ld[2] + 1.0]);
                let spec = dot(n, half).max(0.0).powf(if mat == Material::Clay { 24.0 } else { 40.0 }) * 0.35;
                let rim = (1.0 - n[2]).powf(3.0) * 0.18;
                let k = light.ambient + (1.0 - light.ambient) * diff;
                [base[0] * k + 255.0 * (spec + rim), base[1] * k + 255.0 * (spec + rim), base[2] * k + 255.0 * (spec + rim)]
            }
        }
    };
    // Couleur par sommet (Gouraud) ou par face (plat).
    let vcol: Vec<[f32; 3]> = if smooth { m.nrm.par_iter().map(|n| shade_n(*n, Color32::from_gray(205))).collect() } else { vec![] };
    let face: Vec<Option<([usize; 3], [[f32; 3]; 3])>> = m
        .tris
        .par_iter()
        .zip(m.colors.par_iter())
        .map(|(t, col)| {
            let idx = [t[0] as usize, t[1] as usize, t[2] as usize];
            if idx.iter().any(|&i| screen[i][0].is_nan()) {
                return None;
            }
            let cols = if smooth && !matches!(mat, Material::Color) {
                [vcol[idx[0]], vcol[idx[1]], vcol[idx[2]]]
            } else {
                let fnrm = norm(cross(sub(m.pos[idx[1]], m.pos[idx[0]]), sub(m.pos[idx[2]], m.pos[idx[0]])));
                let c = if smooth {
                    let avg = norm([m.nrm[idx[0]][0] + m.nrm[idx[1]][0] + m.nrm[idx[2]][0], m.nrm[idx[0]][1] + m.nrm[idx[1]][1] + m.nrm[idx[2]][1], m.nrm[idx[0]][2] + m.nrm[idx[1]][2] + m.nrm[idx[2]][2]]);
                    shade_n(avg, *col)
                } else {
                    shade_n(fnrm, *col)
                };
                [c, c, c]
            };
            Some((idx, cols))
        })
        .collect();

    let band = 32usize;
    let mut rgba = vec![0u8; w * h * 4];
    rgba.par_chunks_mut(w * 4 * band).enumerate().for_each(|(bi, chunk)| {
        let y0 = bi * band;
        let rows = chunk.len() / (w * 4);
        let mut depth = vec![f32::MIN; w * rows];
        for px in chunk.chunks_exact_mut(4) {
            px.copy_from_slice(&bg);
        }
        for f in face.iter().flatten() {
            let (a, b, c) = (screen[f.0[0]], screen[f.0[1]], screen[f.0[2]]);
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
                        let col = f.1;
                        let o = k * 4;
                        for ch in 0..3 {
                            chunk[o + ch] = (w0 * col[0][ch] + w1 * col[1][ch] + w2 * col[2][ch]).clamp(0.0, 255.0) as u8;
                        }
                        chunk[o + 3] = 255;
                    }
                }
            }
        }
    });
    Frame { w, h, rgba }
}

/// Image du modèle pour les miniatures des boards et des refs.
pub fn thumbnail(model: &Model, w: u32, h: u32, bg: [u8; 4]) -> image::RgbaImage {
    let prep = Prepared::new(model, model.z_up);
    let f = render(&prep, &Camera { dist: 2.0, ..Default::default() }, &Light::default(), Material::Gray, true, w as usize, h as usize, bg);
    image::RgbaImage::from_raw(w, h, f.rgba).unwrap_or_else(|| image::RgbaImage::new(w, h))
}

// ---------- visionneuse ----------

pub enum State {
    Loading(Receiver<Result<Model, String>>),
    Ready(Box<Model>),
    Failed(String),
}

pub struct Viewer {
    pub state: State,
    cam: Camera,
    light: Light,
    material: Material,
    smooth: bool,
    pub z_up: Option<bool>,
    prepared: Option<(bool, Prepared)>,
    tex: Option<egui::TextureHandle>,
    /// Paramètres du dernier rendu (on ne recalcule que si ça change).
    last: Option<(Camera, Light, Material, bool, [usize; 2], bool)>,
    show_light: bool,
}

impl Viewer {
    pub fn open(path: std::path::PathBuf, ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(load(&path));
            ctx.request_repaint();
        });
        Self {
            state: State::Loading(rx),
            cam: Camera::default(),
            light: Light::default(),
            material: Material::Clay,
            smooth: true,
            z_up: None,
            prepared: None,
            tex: None,
            last: None,
            show_light: false,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        if let State::Loading(rx) = &self.state {
            if let Ok(r) = rx.try_recv() {
                self.state = match r {
                    Ok(m) => State::Ready(Box::new(m)),
                    Err(e) => State::Failed(e),
                };
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
            State::Ready(m) => m,
        };
        let z_up = *self.z_up.get_or_insert(model.z_up);
        if self.prepared.as_ref().is_none_or(|(z, _)| *z != z_up) {
            self.prepared = Some((z_up, Prepared::new(model, z_up)));
            self.last = None;
        }

        // Barre d'outils
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("cadrer").on_hover_text("Home ou .").clicked() {
                    self.cam = Camera { ortho: self.cam.ortho, ..Default::default() };
                }
                ui.toggle_value(&mut self.show_light, "lumière");
                let mut z = z_up;
                if ui.checkbox(&mut z, "Z en haut").changed() {
                    self.z_up = Some(z);
                }
                ui.checkbox(&mut self.cam.ortho, "ortho").on_hover_text("5");
                ui.checkbox(&mut self.smooth, "lisse");
                egui::ComboBox::from_id_salt("mat").selected_text(Material::ALL.iter().find(|m| m.0 == self.material).map(|m| m.1).unwrap_or("")).width(90.0).show_ui(ui, |ui| {
                    for (m, l) in Material::ALL {
                        ui.selectable_value(&mut self.material, m, l);
                    }
                });
                let s = model.size();
                let simplified = if model.tris.len() < model.total_tris { format!(" · affiché : {}", fmt_n(model.tris.len())) } else { String::new() };
                let text = format!("{} triangles · {} sommets · {:.2} × {:.2} × {:.2}{simplified}", fmt_n(model.total_tris), fmt_n(model.total_vertices), s[0], s[1], s[2]);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(text).color(p.muted).size(11.5)).truncate());
                });
            });
        });
        if self.show_light {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("lumière").color(p.muted).size(11.5));
                ui.add(egui::Slider::new(&mut self.light.azimuth, -std::f32::consts::PI..=std::f32::consts::PI).text("direction").show_value(false));
                ui.add(egui::Slider::new(&mut self.light.elevation, -1.5..=1.5).text("hauteur").show_value(false));
                ui.add(egui::Slider::new(&mut self.light.ambient, 0.0..=0.9).text("ambiance").show_value(false));
                if ui.small_button("défaut").clicked() {
                    self.light = Light::default();
                }
            });
        }

        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, p.panel);

        // ---------- navigation façon Blender ----------
        // Molette : zoom · bouton du milieu (ou gauche) : tourner · Maj + glisser : déplacer
        // Ctrl + glisser : zoom · bouton droit : déplacer · L + glisser : lumière
        // 1 / 3 / 7 : face, droite, dessus (Maj : opposé) · 5 : ortho · Home ou . : cadrer
        let (mods, lkey) = ui.input(|i| (i.modifiers, i.key_down(Key::L)));
        let dragging = resp.dragged_by(egui::PointerButton::Middle) || resp.dragged_by(egui::PointerButton::Primary) || resp.dragged_by(egui::PointerButton::Secondary);
        if dragging {
            let d = resp.drag_delta();
            let right = resp.dragged_by(egui::PointerButton::Secondary);
            if lkey {
                self.light.azimuth -= d.x * 0.01;
                self.light.elevation = (self.light.elevation - d.y * 0.01).clamp(-1.5, 1.5);
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
            let keys = ui.input(|i| {
                [Key::Num1, Key::Num3, Key::Num7, Key::Num5, Key::Home, Key::Period].map(|k| i.key_pressed(k) && !i.modifiers.command)
            });
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

        // ---------- rendu (seulement si quelque chose a changé) ----------
        let interacting = dragging || ui.input(|i| i.smooth_scroll_delta.y != 0.0);
        let ppp = ui.ctx().pixels_per_point();
        // Pendant le mouvement : demi-résolution pour rester fluide, pleine résolution à l'arrêt.
        let scale = if interacting { 0.5 } else { ppp.min(2.0) };
        let size = [(rect.width() * scale).max(16.0) as usize, (rect.height() * scale).max(16.0) as usize];
        let key = (self.cam, self.light, self.material, self.smooth, size, z_up);
        if self.last != Some(key) || self.tex.is_none() {
            let prep = &self.prepared.as_ref().unwrap().1;
            // Fond transparent : la grille du sol reste visible autour du modèle.
            let bg = [0, 0, 0, 0];
            let frame = render(prep, &self.cam, &self.light, self.material, self.smooth, size[0], size[1], bg);
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
        if let Some((_, prep)) = &self.prepared {
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
        if let Some(t) = &self.tex {
            painter.image(t.id(), rect, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }

        // Repère des axes (en bas à gauche)
        let o = rect.left_bottom() + vec2(36.0, -40.0);
        for (axis, col, label) in [([1.0, 0.0, 0.0], Color32::from_rgb(230, 70, 70), "X"), ([0.0, 1.0, 0.0], Color32::from_rgb(110, 200, 80), if z_up { "Z" } else { "Y" }), ([0.0, 0.0, 1.0], Color32::from_rgb(80, 140, 240), if z_up { "-Y" } else { "Z" })] {
            let r = self.cam.rotate(axis);
            let end = o + vec2(r[0], -r[1]) * 22.0;
            painter.line_segment([o, end], Stroke::new(2.0, col));
            painter.text(end, egui::Align2::CENTER_CENTER, label, egui::FontId::monospace(10.0), col);
        }
        painter.text(
            rect.right_bottom() + vec2(-12.0, -10.0),
            egui::Align2::RIGHT_BOTTOM,
            "glisser : tourner · Maj ou clic droit : déplacer · Ctrl / molette : zoom · L + glisser : lumière · 1 3 7 : vues · 5 : ortho",
            egui::FontId::monospace(10.5),
            p.muted,
        );
    }
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
