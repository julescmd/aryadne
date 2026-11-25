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

#[derive(Clone)]
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
        // Une coordonnée infinie ou NaN (fichier abîmé) fausserait les bornes, donc tout le cadrage
        let mut positions = positions;
        for v in positions.iter_mut() {
            for x in v.iter_mut().filter(|x| !x.is_finite()) {
                *x = 0.0;
            }
        }
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

/// À l'import : les sommets au même endroit ne font plus qu'un. Les exportateurs dédoublent les
/// sommets le long des coutures d'UV, des arêtes dures et des groupes de lissage ; ici le maillage
/// redevient continu (Aryadne calcule son propre lissage et ses propres coutures). Les triangles
/// devenus plats sont retirés ; `keep` dit lesquels restent, pour les données par triangle.
pub fn weld(positions: &[V3], tris: &[[u32; 3]]) -> (Vec<V3>, Vec<[u32; 3]>, Vec<bool>) {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    // Un sommet infini (fichier abîmé) rendrait la tolérance infinie : tous les sommets fusionneraient en un seul
    for p in positions.iter().filter(|p| p.iter().all(|x| x.is_finite())) {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let diag = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt();
    let tol = (if diag.is_finite() { diag.max(1e-9) } else { 1.0 } * 1e-6) as f64;
    let mut ids: HashMap<[i64; 3], u32> = HashMap::new();
    let mut out = Vec::with_capacity(positions.len());
    let remap: Vec<u32> = positions
        .iter()
        .map(|p| {
            let key = [(p[0] as f64 / tol).round() as i64, (p[1] as f64 / tol).round() as i64, (p[2] as f64 / tol).round() as i64];
            *ids.entry(key).or_insert_with(|| {
                out.push(*p);
                (out.len() - 1) as u32
            })
        })
        .collect();
    let mut nt = Vec::with_capacity(tris.len());
    let mut keep = Vec::with_capacity(tris.len());
    for t in tris {
        let m = [remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize]];
        let ok = m[0] != m[1] && m[1] != m[2] && m[0] != m[2];
        keep.push(ok);
        if ok {
            nt.push(m);
        }
    }
    (out, nt, keep)
}

/// Garde les données par triangle dont le triangle reste après `weld`
pub fn keep_per_tri<T: Copy>(v: &[T], keep: &[bool]) -> Vec<T> {
    v.iter().zip(keep).filter(|(_, k)| **k).map(|(x, _)| *x).collect()
}

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
            let (pos, tris, keep) = weld(&m.positions, &m.tris);
            Model::with_edges(pos, tris, None, Some(keep_per_tri(&m.edges, &keep)), false)
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
    // Un OBJ écrit par Aryadne garde ses sommets tels quels (une détache ou une déchirure en dédouble exprès)
    let mut ours = false;
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim_start();
        if line.starts_with('#') && line.contains("Aryadne") {
            ours = true;
        }
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
    // Une face qui cite un sommet absent (fichier abîmé) est écartée ici : la fusion des sommets lit ces indices
    let nv = verts.len();
    let ok: Vec<bool> = tris.iter().map(|t: &[u32; 3]| t.iter().all(|&i| (i as usize) < nv)).collect();
    if ok.iter().any(|k| !k) {
        tris = keep_per_tri(&tris, &ok);
        edges = keep_per_tri(&edges, &ok);
    }
    // OBJ : les « s » (groupes de lissage), « vn » et « vt » sont ignorés ; on refusionne aussi les sommets en double
    if ours {
        return Model::with_edges(verts, tris, None, Some(edges), false);
    }
    let (verts, tris, keep) = weld(&verts, &tris);
    Model::with_edges(verts, tris, None, Some(keep_per_tri(&edges, &keep)), false)
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
    // glTF dédouble les sommets aux coutures et aux arêtes dures : on les refusionne
    let (positions, tris, keep) = weld(&positions, &tris);
    let colors = keep_per_tri(&colors, &keep);
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
    /// Roulis (autour de l'axe de visée), pour une caméra de scène penchée
    pub roll: f32,
    /// Focale par rapport à celle de la vue (1 : environ 45° sur le petit côté)
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self { yaw: 0.6, pitch: 0.35, dist: 2.2, pan: [0.0, 0.0], ortho: false, roll: 0.0, zoom: 1.0 }
    }
}

/// Focale de la vue quand le zoom vaut 1, en part du petit côté de l'image
pub const BASE_FOCAL: f32 = 1.2;

/// Zoom d'une caméra dont le champ sur le petit côté de l'image est `fov` degrés (0 : celui de la vue)
pub fn zoom_of_fov(fov: f32) -> f32 {
    if fov.is_finite() && fov > 0.5 && fov < 175.0 { 0.5 / (fov.to_radians() * 0.5).tan() / BASE_FOCAL } else { 1.0 }
}

/// Champ (degrés, sur le petit côté) d'une focale donnée en part du petit côté de l'image
pub fn fov_of_focal(focal: f32) -> f32 {
    (2.0 * (0.5 / focal.max(1e-6)).atan()).to_degrees()
}

impl Camera {
    fn rotate(&self, v: V3) -> V3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let x = v[0] * cy + v[2] * sy;
        let z = -v[0] * sy + v[2] * cy;
        let y = v[1] * cp - z * sp;
        let z = v[1] * sp + z * cp;
        if self.roll == 0.0 {
            return [x, y, z];
        }
        let (sr, cr) = self.roll.sin_cos();
        [x * cr - y * sr, x * sr + y * cr, z]
    }
    /// Inverse de `rotate` : de l'espace caméra vers le monde.
    fn unrotate(&self, v: V3) -> V3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let (sr, cr) = self.roll.sin_cos();
        let v = [v[0] * cr + v[1] * sr, -v[0] * sr + v[1] * cr, v[2]];
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

/// Projection d'une caméra, préparée une fois : la rotation est une matrice (pas de sinus par sommet).
#[derive(Clone, Copy)]
pub(crate) struct Proj {
    r: [V3; 3],
    pan: [f32; 2],
    dist: f32,
    ortho: bool,
    cx: f32,
    cy: f32,
    focal: f32,
    scale: f32,
}

impl Proj {
    pub(crate) fn new(cam: &Camera, w: f32, h: f32) -> Proj {
        let (sy, cy) = cam.yaw.sin_cos();
        let (sp, cp) = cam.pitch.sin_cos();
        let focal = w.min(h) * 1.2 * cam.zoom;
        let (sr, cr) = cam.roll.sin_cos();
        let (a, b) = ([cy, 0.0, sy], [sy * sp, cp, -cy * sp]);
        let mix = |k: f32, l: f32| [a[0] * k + b[0] * l, a[1] * k + b[1] * l, a[2] * k + b[2] * l];
        Proj { r: [mix(cr, -sr), mix(sr, cr), [-sy * cp, sp, cy * cp]], pan: cam.pan, dist: cam.dist, ortho: cam.ortho, cx: w / 2.0, cy: h / 2.0, focal, scale: focal / cam.dist }
    }
    /// Espace caméra (la caméra regarde vers -Z)
    #[inline]
    pub(crate) fn view(&self, v: V3) -> V3 {
        [dot(self.r[0], v) + self.pan[0], dot(self.r[1], v) + self.pan[1], dot(self.r[2], v) - self.dist]
    }
    /// De l'espace caméra vers le monde (rotation seule)
    #[inline]
    pub(crate) fn unrotate(&self, v: V3) -> V3 {
        let r = &self.r;
        [r[0][0] * v[0] + r[1][0] * v[1] + r[2][0] * v[2], r[0][1] * v[0] + r[1][1] * v[1] + r[2][1] * v[2], r[0][2] * v[0] + r[1][2] * v[1] + r[2][2] * v[2]]
    }
    /// Pixel et profondeur de vue ; x et y valent NaN derrière la caméra
    #[inline]
    pub(crate) fn screen(&self, v: V3) -> [f32; 3] {
        let c = self.view(v);
        if self.ortho {
            [self.cx + c[0] * self.scale, self.cy - c[1] * self.scale, c[2]]
        } else if c[2] < -0.02 {
            let k = self.focal / -c[2];
            [self.cx + c[0] * k, self.cy - c[1] * k, c[2]]
        } else {
            [f32::NAN, f32::NAN, c[2]]
        }
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

/// Maj+touche, lu sur l'événement lui-même (l'état des modificateurs à la fin de l'image peut déjà être relâché)
fn shift_key(i: &egui::InputState, key: Key) -> bool {
    i.events.iter().any(|e| matches!(e, egui::Event::Key { key: k, pressed: true, repeat: false, modifiers, .. } if *k == key && modifiers.shift && !modifiers.command && !modifiers.alt))
}

/// Le mode édition 3D est sous la souris : ses raccourcis (Ctrl+B, Ctrl+R…) passent avant ceux de l'app.
static MESH_KEYS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Crans de molette de cette image (positif vers le haut).
fn wheel_of(i: &egui::InputState) -> f32 {
    i.events.iter().map(|e| if let egui::Event::MouseWheel { delta, .. } = e { delta.y } else { 0.0 }).sum()
}

/// Aide de l'outil en cours, en bas de la vue, sur un fond lisible (au-dessus des statistiques).
fn tool_hint(ui: &egui::Ui, rect: egui::Rect, text: String) {
    let p = theme::pal();
    let painter = ui.painter_at(rect);
    let galley = painter.layout(text, egui::FontId::monospace(11.5), p.text, rect.width() - 40.0);
    let at = pos2(rect.center().x - galley.size().x / 2.0, rect.bottom() - 30.0 - galley.size().y);
    painter.rect_filled(egui::Rect::from_min_size(at, galley.size()).expand2(egui::vec2(8.0, 5.0)), 4.0, p.raised.gamma_multiply(0.94));
    painter.galley(at, galley, p.text);
}

pub fn mesh_keys() -> bool {
    MESH_KEYS.load(std::sync::atomic::Ordering::Relaxed)
}

pub enum PieResult {
    Open,
    Cancel,
    Chose(usize),
}

/// Menu circulaire, comme ceux de Blender : les choix autour du point d'ouverture ; on vise en
/// bougeant la souris dans leur direction, puis clic, ou relâcher la touche qui l'a ouvert.
/// Ordre des places : gauche, droite, bas, haut, puis les diagonales.
pub fn pie_menu(ui: &egui::Ui, id: egui::Id, center: Pos2, items: &[&str], current: Option<usize>, key: Key) -> PieResult {
    let p = theme::pal();
    let dirs = [(-1.0f32, 0.0f32), (1.0, 0.0), (0.0, 1.0), (0.0, -1.0), (-0.72, -0.72), (0.72, -0.72), (-0.72, 0.72), (0.72, 0.72)];
    let (cur, clicked, cancel, released) = ui.input(|i| (i.pointer.hover_pos(), i.pointer.primary_pressed(), i.key_pressed(Key::Escape) || i.pointer.secondary_pressed(), i.key_released(key)));
    let off = cur.map(|c| c - center).unwrap_or_default();
    let hovered = (off.length() > 22.0).then(|| {
        let a = off.normalized();
        (0..items.len().min(8)).max_by(|&x, &y| (dirs[x].0 * a.x + dirs[x].1 * a.y).total_cmp(&(dirs[y].0 * a.x + dirs[y].1 * a.y)))
    }).flatten();
    egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(center).interactable(false).show(ui.ctx(), |ui| {
        let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, id));
        painter.circle(center, 18.0, p.raised.gamma_multiply(0.9), Stroke::new(1.0, p.line));
        if off.length() > 4.0 {
            painter.line_segment([center, center + off.normalized() * 16.0], Stroke::new(2.0, p.accent));
        }
        for (k, label) in items.iter().enumerate().take(8) {
            let (dx, dy) = dirs[k];
            let text = crate::i18n::t(label);
            let galley = ui.painter().layout_no_wrap(text, egui::FontId::proportional(12.5), p.text);
            let size = galley.size() + egui::vec2(18.0, 10.0);
            // Bord intérieur sur le cercle : les étiquettes s'écartent du centre au lieu de se chevaucher
            let at = center + egui::vec2(dx, dy * 1.15) * 70.0;
            let r = if dx < -0.1 {
                egui::Rect::from_min_size(pos2(at.x - size.x, at.y - size.y / 2.0), size)
            } else if dx > 0.1 {
                egui::Rect::from_min_size(pos2(at.x, at.y - size.y / 2.0), size)
            } else {
                egui::Rect::from_center_size(at + egui::vec2(0.0, dy * size.y / 2.0), size)
            };
            let on = hovered == Some(k);
            let fill = if on { p.accent } else if current == Some(k) { p.selection } else { p.raised };
            painter.rect(r, 4.0, fill, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
            let col = if on { p.on_accent } else { p.text };
            painter.galley_with_override_text_color(r.center() - galley.size() / 2.0, galley, col);
        }
    });
    ui.ctx().request_repaint();
    if cancel {
        PieResult::Cancel
    } else if clicked || (released && off.length() > 40.0) {
        match hovered {
            Some(k) => PieResult::Chose(k),
            None if clicked => PieResult::Cancel,
            None => PieResult::Open,
        }
    } else {
        PieResult::Open
    }
}


/// Transformation d'un objet de la scène, parents compris, sous forme affine (colonnes, origine).
/// Arêtes de l'objet `k` (sommets déjà posés dans `prep`, scène `built`) replacées aux images données ;
/// le côté vaut -1, -2 pour les clés d'avant (la plus proche d'abord), 1, 2 pour celles d'après
fn ghost_edges(built: &crate::scene::Scene, scene: &crate::scene::Scene, k: usize, prep: &Prepared, m: &Model, (t0, t1): (usize, usize), before: &[i32], after: &[i32]) -> Vec<(i32, Vec<[V3; 2]>)> {
    let s = m.size();
    let scale = 1.0 / s[0].max(s[1]).max(s[2]).max(1e-9);
    let c = [(m.min[0] + m.max[0]) / 2.0, (m.min[1] + m.max[1]) / 2.0, (m.min[2] + m.max[2]) / 2.0];
    let (la, oa) = object_affine(built, k);
    let Some(ia) = inv3(la) else { return vec![] };
    // Arêtes uniques, en espace local de l'objet ; au plus ~2500 pour rester fluide
    let mut seen = std::collections::HashSet::new();
    let mut edges: Vec<[V3; 2]> = vec![];
    let local = |v: u32| {
        let n = prep.pos[v as usize];
        let w = [n[0] / scale + c[0], n[1] / scale + c[1], n[2] / scale + c[2]];
        mulm(&ia, [w[0] - oa[0], w[1] - oa[1], w[2] - oa[2]])
    };
    let tris = prep.tris.get(t0..t1).unwrap_or(&[]);
    let stride = (tris.len() / 1200).max(1);
    for t in tris.iter().step_by(stride) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            if seen.insert((a.min(b), a.max(b))) {
                edges.push([local(a), local(b)]);
            }
        }
    }
    let mut sc = scene.clone();
    let mut out = vec![];
    for (side, f) in before.iter().enumerate().map(|(i, f)| (-(i as i32) - 1, *f)).chain(after.iter().enumerate().map(|(i, f)| (i as i32 + 1, *f))) {
        sc.apply_frame(f as f32);
        let (lb, ob) = object_affine(&sc, k);
        let put = |q: V3| {
            let w = mulm(&lb, q);
            [w[0] + ob[0], w[1] + ob[1], w[2] + ob[2]]
        };
        out.push((side, edges.iter().map(|[a, b]| [put(*a), put(*b)]).collect()));
    }
    out
}

/// Le point de vue de la caméra `k` de la scène, dans le repère de la vue (bornes de la scène `lo`, `hi`)
fn camera_view(sc: &crate::scene::Scene, k: usize, (lo, hi): (V3, V3), base: Camera) -> Camera {
    let longest = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let wpos = sc.world(k, [0.0; 3]);
    let o = &sc.objects[k];
    let mut cam = base;
    // La vue est l'inverse de l'orientation de l'objet dans la scène (parents compris) : ses axes
    // sont les lignes de la rotation de vue, d'où le lacet, le tangage et le roulis
    let (ax, _) = object_affine(sc, k);
    let (rx, ry, rz) = (norm(ax[0]), norm(ax[1]), norm(ax[2]));
    if [rx, ry, rz].iter().flatten().all(|x| x.is_finite()) && dot(rz, rz) > 0.5 {
        cam.pitch = rz[1].clamp(-1.0, 1.0).asin();
        cam.yaw = (-rz[0]).atan2(rz[2]);
        cam.roll = (-rx[1]).atan2(ry[1]);
    } else {
        cam.yaw = -o.rot[1].to_radians();
        cam.pitch = -o.rot[0].to_radians();
        cam.roll = 0.0;
    }
    cam.zoom = zoom_of_fov(o.fov);
    cam.ortho = false;
    let r = cam.rotate([(wpos[0] - c[0]) / longest, (wpos[1] - c[1]) / longest, (wpos[2] - c[2]) / longest]);
    cam.pan = [-r[0], -r[1]];
    // Le centre de la scène peut être derrière la caméra (caméra suivie qui regarde ailleurs) : la
    // distance devient négative, ce que la perspective accepte
    cam.dist = if r[2].abs() < 1e-4 { 1e-4 } else { r[2] };
    cam
}

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

/// Mode édition : pour chaque sommet affiché de l'objet (triangles t0..t1), le sommet du maillage édité
/// au même endroit (positions d'avant le geste, en espace normalisé). None si un sommet n'a pas de jumeau.
fn map_edit_vertices(prep: &Prepared, t0: usize, t1: usize, before: &[V3]) -> Option<Vec<(u32, u32)>> {
    let tris = prep.tris.get(t0..t1)?;
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in before {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let diag = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt().max(1e-9);
    let eps = diag * 1e-4;
    let cell = |p: V3| [(p[0] / eps).floor() as i64, (p[1] / eps).floor() as i64, (p[2] / eps).floor() as i64];
    let mut grid: std::collections::HashMap<[i64; 3], Vec<u32>> = Default::default();
    for (i, p) in before.iter().enumerate() {
        grid.entry(cell(*p)).or_default().push(i as u32);
    }
    let mut seen = std::collections::HashSet::new();
    let mut out = vec![];
    for t in tris {
        for &v in t {
            if !seen.insert(v) {
                continue;
            }
            let p = prep.pos[v as usize];
            let c = cell(p);
            let mut best: Option<(f32, u32)> = None;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        for &i in grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]).map(|v| v.as_slice()).unwrap_or(&[]) {
                            let q = before[i as usize];
                            let d = (q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2);
                            if best.is_none_or(|b| d < b.0) {
                                best = Some((d, i));
                            }
                        }
                    }
                }
            }
            let (d, i) = best?;
            if d > eps * eps * 4.0 {
                return None;
            }
            out.push((v, i));
        }
    }
    Some(out)
}

/// Le maillage affiché avec les sommets de l'objet aux positions `now` (et leurs normales refaites)
fn patch_vertices(prep: &Prepared, t0: usize, t1: usize, map: &[(u32, u32)], now: &[V3]) -> Prepared {
    let mut pos = prep.pos.clone();
    let mut nrm = prep.nrm.clone();
    for &(v, i) in map {
        if let Some(p) = now.get(i as usize) {
            pos[v as usize] = *p;
        }
        nrm[v as usize] = [0.0; 3];
    }
    if let Some(tris) = prep.tris.get(t0..t1) {
        for t in tris {
            let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
            let n = cross(sub(b, a), sub(c, a));
            for &v in t {
                let x = &mut nrm[v as usize];
                *x = [x[0] + n[0], x[1] + n[1], x[2] + n[2]];
            }
        }
    }
    for &(v, _) in map {
        nrm[v as usize] = norm(nrm[v as usize]);
    }
    Prepared { pos, nrm, tris: prep.tris.clone(), colors: prep.colors.clone(), edges: prep.edges.clone(), floor: prep.floor, uv: prep.uv.clone(), mat: prep.mat.clone(), curv: std::sync::OnceLock::new(), bvh: std::sync::OnceLock::new() }
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
    /// Peinture posée sur chaque objet (scène), mélangée à la couleur du matériau
    pub slot_paint: Vec<Option<crate::paint3d::Layer>>,
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
            slot_paint: vec![],
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
/// Le gamma passe par une table (indexée par la racine, pour garder la précision dans les ombres) :
/// trois `powf` par pixel pesaient lourd dans le rendu.
fn tone(c: [f32; 3], exposure: f32) -> [u8; 3] {
    static LUT: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let lut = LUT.get_or_init(|| (0..4096).map(|i| ((i as f32 / 4095.0).powi(2).powf(1.0 / 2.2) * 255.0 + 0.5) as u8).collect());
    let k = exposure;
    let f = |x: f32| {
        let x = (x * k).max(0.0);
        let y = (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14);
        // NaN : `clamp` le laisse passer, la conversion en entier le ramène à 0
        lut[((y.clamp(0.0, 1.0).sqrt() * 4095.0 + 0.5) as usize).min(4095)]
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
/// Un point de la vue (repère du modèle préparé) à l'écran, dans une vue de `w` × `h`
pub fn project_point(cam: &Camera, w: f32, h: f32, v: V3) -> Option<[f32; 2]> {
    let focal = w.min(h) * 1.2 * cam.zoom;
    let c = cam.view(v);
    if cam.ortho {
        let k = focal / cam.dist;
        Some([w / 2.0 + c[0] * k, h / 2.0 - c[1] * k])
    } else if c[2] < -0.02 {
        Some([w / 2.0 + c[0] / -c[2] * focal, h / 2.0 - c[1] / -c[2] * focal])
    } else {
        None
    }
}

pub fn pick(m: &Prepared, cam: &Camera, w: f32, h: f32, px: f32, py: f32) -> Option<(usize, [f32; 3])> {
    let pr = Proj::new(cam, w, h);
    // Maillage dense (ou arbre déjà construit pour les ombres) : un rayon dans l'arbre, quelques
    // dizaines de tests au lieu de tous les triangles. La peinture pointe plusieurs fois par touche.
    if m.tris.len() >= PICK_TREE_MIN || m.bvh.get().is_some() {
        let b = m.bvh.get_or_init(|| crate::bvh::Bvh::new(&m.pos, &m.tris));
        // Un rayon pile sur une arête peut glisser entre deux triangles : on réessaie à un centième de pixel
        for (dx, dy) in [(0.0, 0.0), (0.011, 0.007), (-0.009, 0.012)] {
            let (px, py) = (px + dx, py + dy);
            let (eye, dir) = if pr.ortho {
                (pr.unrotate([(px - pr.cx) / pr.scale - pr.pan[0], -(py - pr.cy) / pr.scale - pr.pan[1], pr.dist]), pr.unrotate([0.0, 0.0, -1.0]))
            } else {
                (pr.unrotate([-pr.pan[0], -pr.pan[1], pr.dist]), pr.unrotate(norm([(px - pr.cx) / pr.focal, -(py - pr.cy) / pr.focal, -1.0])))
            };
            if let Some(hit) = b.closest(eye, dir, if pr.ortho { -1e6 } else { 0.02 }, 1e9) {
                return Some((hit.tri as usize, [1.0 - hit.u - hit.v, hit.u, hit.v]));
            }
        }
        return None;
    }
    pick_scan(m, &pr, px, py)
}

/// À partir de combien de triangles `pick` passe par l'arbre de rayons
const PICK_TREE_MIN: usize = 20_000;

/// Pointage par parcours de tous les triangles projetés à l'écran. Les poids rendus sont ceux de
/// la surface (corrigés de la perspective), pas ceux de l'écran.
fn pick_scan(m: &Prepared, pr: &Proj, px: f32, py: f32) -> Option<(usize, [f32; 3])> {
    let screen: Vec<[f32; 3]> = m.pos.iter().map(|v| pr.screen(*v)).collect();
    let mut best: Option<(usize, [f32; 3], f32)> = None;
    for (ti, t) in m.tris.iter().enumerate() {
        let (Some(a), Some(b), Some(c)) = (screen.get(t[0] as usize), screen.get(t[1] as usize), screen.get(t[2] as usize)) else { continue };
        // Boîte à l'écran d'abord ; les comparaisons avec NaN (derrière la caméra) sont fausses
        if !(a[0].min(b[0]).min(c[0]) <= px && a[0].max(b[0]).max(c[0]) >= px && a[1].min(b[1]).min(c[1]) <= py && a[1].max(b[1]).max(c[1]) >= py) || a[0].is_nan() || b[0].is_nan() || c[0].is_nan() {
            continue;
        }
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
        // Plus grand = plus proche, comme dans le rendu
        let (z, bw) = if pr.ortho { (a[2] * w0 + b[2] * w1 + c[2] * w2, [w0, w1]) } else { persp_bary([w0, w1, w2], [1.0 / -a[2], 1.0 / -b[2], 1.0 / -c[2]]) };
        if best.as_ref().is_none_or(|bb| z > bb.2) {
            best = Some((ti, [bw[0], bw[1], 1.0 - bw[0] - bw[1]], z));
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
    /// Construit l'arbre de rayons si ce n'est pas fait : les pointages suivants passent par lui
    pub fn ensure_tree(&self) {
        self.bvh.get_or_init(|| crate::bvh::Bvh::new(&self.pos, &self.tris));
    }
}

/// Poids à l'écran → poids dans l'espace (perspective juste) : rend 1/profondeur interpolé et les deux premiers poids
fn persp_bary(w: [f32; 3], inv_depth: [f32; 3]) -> (f32, [f32; 2]) {
    let z = w[0] * inv_depth[0] + w[1] * inv_depth[1] + w[2] * inv_depth[2];
    (z, [w[0] * inv_depth[0] / z, w[1] * inv_depth[1] / z])
}

pub fn render(m: &Prepared, cam: &Camera, look: &Look, w: usize, h: usize) -> Frame {
    use rayon::prelude::*;
    let focal = w.min(h) as f32 * 1.2 * cam.zoom;
    let ortho_scale = focal / cam.dist;
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let pr = Proj::new(cam, w as f32, h as f32);
    let screen: Vec<[f32; 3]> = m.pos.par_iter().map(|v| pr.screen(*v)).collect();
    // Rotation de l'environnement : sinus et cosinus une fois, pas à chaque pixel
    let (env_s, env_c) = (-look.env_rot).sin_cos();
    let env_dir = |d: V3| -> V3 { [d[0] * env_c + d[2] * env_s, d[1], -d[0] * env_s + d[2] * env_c] };
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
        let ray = |k: usize| -> bool {
            let r = ((k as f32 + 0.5) / samples as f32).sqrt() * spread;
            let (sa, ca) = (turn + k as f32 * 2.399_963).sin_cos();
            let d = norm([dir[0] + (t1[0] * ca + t2[0] * sa) * r, dir[1] + (t1[1] * ca + t2[1] * sa) * r, dir[2] + (t1[2] * ca + t2[2] * sa) * r]);
            !b.occluded(o, d, 1e-3, reach)
        };
        // D'abord un rayon sur deux, du bord du disque vers le centre : s'ils sont tous d'accord (plein
        // jour ou pleine ombre, presque toute l'image), les autres ne diraient rien de plus
        let mut open = 0;
        let mut probes = 0;
        for k in (0..samples).rev().step_by(2) {
            open += ray(k) as usize;
            probes += 1;
        }
        if samples >= 4 && (open == 0 || open == probes) {
            return if open == 0 { 0.0 } else { 1.0 };
        }
        for k in (0..samples).rev().skip(1).step_by(2) {
            open += ray(k) as usize;
        }
        open as f32 / samples as f32
    };
    let (albedo0, rough, metal) = look.material.params();
    let wire = look.wire;

    let band = 16usize;
    let mut rgba = vec![0u8; w * h * 4];
    // Chaque bande ne parcourt que ses triangles : on range d'abord chaque triangle dans les bandes
    // qu'il touche (avant, toutes les bandes relisaient tout le maillage)
    let n_bands = h.div_ceil(band).max(1);
    let spans: Vec<(u32, u32)> = m
        .tris
        .par_iter()
        .map(|t| {
            let (a, b, c) = (screen[t[0] as usize], screen[t[1] as usize], screen[t[2] as usize]);
            let (y0, y1) = (a[1].min(b[1]).min(c[1]).floor(), a[1].max(b[1]).max(c[1]).ceil());
            let (x0, x1) = (a[0].min(b[0]).min(c[0]), a[0].max(b[0]).max(c[0]));
            // NaN (derrière la caméra) ou hors de l'image : aucune bande
            if !(y1 >= 0.0 && y0 <= h as f32 - 1.0 && x1 >= 0.0 && x0 <= w as f32) || a[0].is_nan() || b[0].is_nan() || c[0].is_nan() {
                return (1, 0);
            }
            ((y0.max(0.0) as usize / band) as u32, ((y1.min(h as f32 - 1.0).max(0.0) as usize / band).min(n_bands - 1)) as u32)
        })
        .collect();
    let mut bin_start = vec![0usize; n_bands + 1];
    for &(b0, b1) in &spans {
        for b in b0..=b1 {
            bin_start[b as usize + 1] += 1;
        }
    }
    for b in 0..n_bands {
        bin_start[b + 1] += bin_start[b];
    }
    let mut bins = vec![0u32; bin_start[n_bands]];
    let mut fill = bin_start.clone();
    for (ti, &(b0, b1)) in spans.iter().enumerate() {
        for b in b0..=b1 {
            bins[fill[b as usize]] = ti as u32;
            fill[b as usize] += 1;
        }
    }
    drop(spans);
    let bands: Vec<(Vec<u32>, Vec<f32>)> = rgba.par_chunks_mut(w * 4 * band).enumerate().map(|(bi, chunk)| {
        let y0 = bi * band;
        let rows = chunk.len() / (w * 4);
        let n = w * rows;
        let mut depth = vec![f32::MIN; n];
        let mut tri = vec![u32::MAX; n];
        let mut bary = vec![[0.0f32; 2]; n];
        // Passe 1 : triangle visible de chaque pixel
        for &ti in &bins[bin_start[bi]..bin_start[bi + 1]] {
            let ti = ti as usize;
            let t = &m.tris[ti];
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
            // En perspective, ce qui varie linéairement à l'écran est 1/profondeur : on interpole ça, et les poids
            // (UV, normales, position) sont corrigés d'autant. Sinon les textures glissent quand la caméra bouge.
            let persp = !cam.ortho;
            let (ia, ib, ic) = if persp { (1.0 / -a[2], 1.0 / -b[2], 1.0 / -c[2]) } else { (1.0, 1.0, 1.0) };
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
                    // Plus grand = plus proche, dans les deux cas
                    let (z, bw) = if persp { persp_bary([w0, w1, w2], [ia, ib, ic]) } else { (w0 * a[2] + w1 * b[2] + w2 * c[2], [w0, w1]) };
                    let k = (y as usize - y0) * w + x as usize;
                    if z > depth[k] {
                        depth[k] = z;
                        tri[k] = ti as u32;
                        bary[k] = bw;
                    }
                }
            }
        }
        // Passe 2 : éclairage
        for k in 0..n {
            let x = k % w;
            let y = y0 + k / w;
            let o = k * 4;
            // Fond vide, sans environnement ni ombre au sol : rien à calculer pour ce pixel
            if tri[k] == u32::MAX && !look.show_bg && bvh.is_none() {
                continue;
            }
            // Direction du regard (du pixel vers la scène) en espace monde
            let ray_view = if cam.ortho { [0.0, 0.0, -1.0] } else { norm([(x as f32 + 0.5 - cx) / focal, -(y as f32 + 0.5 - cy) / focal, -1.0]) };
            let ray = pr.unrotate(ray_view);
            let ti = tri[k];
            if ti == u32::MAX {
                if look.show_bg {
                    let d = env_dir(ray);
                    let c = if look.blur_bg { env.bg_blur.sample(d) } else { env.bg.sample(d) };
                    let s = look.env_strength;
                    let t = tone([c[0] * s, c[1] * s, c[2] * s], exposure);
                    chunk[o..o + 3].copy_from_slice(&t);
                    chunk[o + 3] = 255;
                } else if let Some(b) = bvh {
                    // Sol : là où un objet cache une lumière, une ombre se pose sur le plan du sol
                    let eye = if cam.ortho { pr.unrotate([(x as f32 + 0.5 - cx) / ortho_scale - cam.pan[0], -(y as f32 + 0.5 - cy) / ortho_scale - cam.pan[1], cam.dist]) } else { pr.unrotate([-cam.pan[0], -cam.pan[1], cam.dist]) };
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
                // Peinture sur l'objet : ses UV de peinture en ce point, et les masques peints que lit le matériau
                let layer = m.mat.get(ti as usize).and_then(|&k| look.slot_paint.get(k as usize)).and_then(|p| p.as_ref());
                let paint_uv = layer.and_then(|l| {
                    let u = l.uv.get((ti as usize).checked_sub(l.first)?)?;
                    Some([u[0][0] * w0 + u[1][0] * w1 + u[2][0] * w2, u[0][1] * w0 + u[1][1] * w1 + u[2][1] * w2])
                });
                let masks = match (layer.and_then(|l| l.masks.as_ref()), paint_uv) {
                    (Some(t), Some(uv)) => t.read().map(|img| crate::paint3d::sample(&img, uv)).unwrap_or([0.0; 4]),
                    _ => [0.0; 4],
                };
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
                    let c = crate::nodes::Ctx { masks, curv, uv: tuv.map(|u| [u[0], 1.0 - u[1]]).unwrap_or([pos[0] + 0.5, 0.5 - pos[1]]), pos, nrm, view: v, vcol: [g(vc[0]), g(vc[1]), g(vc[2]), 1.0], res: [w as f32, h as f32] };
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
                // Peinture sur l'objet : par-dessus la couleur, la rugosité et le métal du matériau
                if let (Some(l), Some(uv)) = (layer, paint_uv) {
                    if let Ok(img) = l.tex.read() {
                        let c = crate::paint3d::sample(&img, uv);
                        if c[3] > 0.0 {
                            for k in 0..3 {
                                albedo[k] = albedo[k] * (1.0 - c[3]) + c[k].powf(2.2) * c[3];
                            }
                        }
                    }
                    if let Some(Ok(img)) = l.pbr.as_ref().map(|t| t.read()) {
                        let c = crate::paint3d::sample(&img, uv);
                        rough = rough * (1.0 - c[1]) + c[0] * c[1];
                        metal = metal * (1.0 - c[3]) + c[2] * c[3];
                    }
                }
                let albedo = if metal > 0.5 && look.nodes.is_some() { albedo } else { albedo };
                let ndv = dot(nrm, v).clamp(0.0, 1.0);
                let ne = env_dir(nrm);
                let s = look.env_strength;
                let (ao, sheen, sss, sss_col) = extra;
                let irr = env.irradiance(ne);
                let irr = [irr[0] * ao, irr[1] * ao, irr[2] * ao];
                let r = sub([2.0 * dot(nrm, v) * nrm[0], 2.0 * dot(nrm, v) * nrm[1], 2.0 * dot(nrm, v) * nrm[2]], v);
                let refl = env.reflect(env_dir(r), rough);
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
                    env.reflect(env_dir(d), rough)
                } else {
                    [0.0; 3]
                };
                let coat_refl = if coat > 0.0 { env.reflect(env_dir(r), 0.04) } else { [0.0; 3] };
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
        // Le reste du programme attend la profondeur de vue (négative devant la caméra) : on la redonne
        if !cam.ortho {
            for d in depth.iter_mut().filter(|d| **d > 0.0) {
                *d = -1.0 / *d;
            }
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
                    if ui.button(format!("{}  {}", crate::lucide::UPLOAD, crate::i18n::t("charger un .hdr / .exr…"))).clicked() {
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
                    if ui.button(format!("{}  {}", crate::lucide::ROTATE_CCW, crate::i18n::t("réinitialiser l'éclairage"))).clicked() {
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
    let cam_obj = scene.active_camera();
    // La vidéo de fond, lue image après image à la taille du rendu
    let mut back = match (&scene.backdrop, cam_obj) {
        (Some(b), Some(_)) => crate::ffmpeg::find("ffmpeg").and_then(|ff| {
            crate::ffmpeg::command(ff)
                .args(["-v", "error", "-ss", &format!("{:.3}", b.from.max(0.0)), "-i"])
                .arg(crate::scene::resolve(path, &b.file))
                .args(["-vf", &format!("fps={},scale={w}:{h}", b.fps.max(1.0)), "-pix_fmt", "rgb24", "-f", "rawvideo", "-"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .ok()
                .map(|c| (c, b.clone(), 0usize, vec![0u8; w * h * 3], false))
        }),
        _ => None,
    };
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
        let cam = match cam_obj {
            Some(k) => camera_view(&sc, k, (lo, hi), view_cam),
            None => view_cam,
        };
        let mut frame = render(&prep, &cam, &lk, w, h);
        if let Some((child, b, next, buf, have)) = back.as_mut() {
            // On avance dans la vidéo jusqu'à l'image voulue ; au-delà de sa fin, la dernière image reste
            if let (Some(want), Some(out)) = (b.frame_at(f as f32), child.stdout.as_mut()) {
                while *next <= want {
                    if std::io::Read::read_exact(out, buf).is_err() {
                        break;
                    }
                    *next += 1;
                    *have = true;
                }
                if *have {
                    crate::matchmove::composite(&mut frame.rgba, &frame.ids, w, h, (0.0, 0.0, w as f32, h as f32), buf, w, h);
                }
            }
        }
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
    if let Some((mut c, ..)) = back {
        let _ = c.kill();
        let _ = c.wait();
    }
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
    let focal = rect.height().min(rect.width()) * 1.2 * cam.zoom;
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
    /// Outil réglé à la souris en cours
    op: Option<ParamOp>,
    /// Dernier outil validé, encore réglable dans son panneau
    last: Option<ParamOp>,
    /// Coupe en boucle qui attend qu'on choisisse l'anneau : nombre de coupes
    loopcut: Option<u32>,
    /// Menu du clic droit ouvert, et l'outil qu'on y a choisi (appliqué à l'image suivante)
    /// (position, 0 menu des outils, 1 menu « supprimer » de X)
    menu: Option<(Pos2, u8)>,
    trigger: Option<&'static str>,
    /// Faces séparées (P) qui attendent de devenir un objet
    split: Option<crate::meshedit::EMesh>,
    /// Édition proportionnelle (O), son rayon (dans la scène), et la distance de chaque sommet à la sélection
    prop: bool,
    prop_r: f32,
    prop_d: (Option<Pos2>, Vec<f32>),
    /// Édition en miroir sur X (dans le repère de l'objet), et le reflet de chaque sommet
    mirror: bool,
    /// Aimantation sur les sommets pendant G
    snap_v: bool,
    mirror_m: (Option<Pos2>, Vec<Option<u32>>),
    /// Couteau (K) en cours : les points déjà posés
    knife: Option<Vec<crate::meshedit::KnifePoint>>,
}

/// Mode sculpture : pinceau, réglages, et le geste en cours
struct SculptUi {
    brush: crate::sculpt::Brush,
    /// Rayon à l'écran (pixels) et force
    radius: f32,
    strength: f32,
    sym: bool,
    stroke: Option<SculptStroke>,
    mirror: (usize, Vec<Option<u32>>),
    /// Remaillage dynamique : subdiviser sous le pinceau, et la taille du détail (part du rayon)
    dyntopo: bool,
    detail: f32,
    /// Niveaux de multirésolution du maillage sculpté, et le fichier pour lequel ils ont été lus
    multires: Option<crate::multires::Multires>,
    mr_for: Option<std::path::PathBuf>,
}

/// Un geste de sculpture : positions au départ, sommets pris par « saisir » (et leurs reflets), voisins,
/// souris au départ et à la dernière touche, point de départ sur la surface
struct SculptStroke {
    start: Vec<V3>,
    grabbed: Vec<(Vec<(usize, f32)>, bool)>,
    topo: crate::sculpt::Topo,
    at0: Pos2,
    last: Pos2,
    centre: V3,
}

impl SculptUi {
    fn new() -> SculptUi {
        SculptUi { brush: crate::sculpt::Brush::Draw, radius: 60.0, strength: 0.5, sym: true, stroke: None, mirror: (usize::MAX, vec![]), dyntopo: false, detail: 0.25, multires: None, mr_for: None }
    }
}

/// Outil réglé à la souris (biseau, incrustation, coupe en boucle), rejoué depuis le maillage de
/// départ à chaque changement : pendant l'outil, puis dans le panneau « dernière opération ».
struct ParamOp {
    /// 0 incruster, 1 biseau, 2 coupe en boucle, 3 biseau des sommets, 4 subdiviser, 5 lisser, 6 fusionner par distance
    kind: u8,
    base: crate::meshedit::EMesh,
    base_sel: Vec<bool>,
    from: Pos2,
    center: Pos2,
    /// Incrustation : part du chemin vers le centre ; biseau : part de la largeur permise ; coupe : glissement (-1 à 1)
    amount: f32,
    segments: u32,
    typed: String,
    ring: Vec<(u32, u32)>,
    /// Biseau : la plus grande largeur permise
    limit: f32,
    /// Sélection laissée par l'outil : si elle change, le panneau de la dernière opération se ferme
    result_sel: Vec<bool>,
    /// Incrustation : face par face (I une seconde fois) plutôt que toute la région
    alt: bool,
}

impl ParamOp {
    fn new(kind: u8, me: &MeshEdit, from: Pos2, center: Pos2) -> ParamOp {
        let limit = match kind {
            0 => me.mesh.region_scale(&me.sel) * 0.5,
            1 => me.mesh.bevel_limit(&me.sel),
            3 => me.mesh.bevel_limit(&me.sel) * 0.9,
            6 => {
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for p in &me.mesh.pos {
                    for k in 0..3 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
                ((hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]) * 0.1).max(1e-6)
            }
            7 => me.mesh.border_loop_len(&me.sel) as f32,
            _ => 1.0,
        };
        let mut op = ParamOp { kind, base: me.mesh.clone(), base_sel: me.sel.clone(), from, center, amount: 0.0, segments: 1, typed: String::new(), ring: vec![], limit, result_sel: vec![], alt: false };
        op.amount = op.default_amount();
        op.segments = match kind {
            7 => ((limit as u32) / 4).max(1),
            _ => 1,
        };
        op
    }
    fn run(&self) -> Option<(crate::meshedit::EMesh, Vec<bool>)> {
        let mut m = self.base.clone();
        let s = match self.kind {
            0 if self.alt => m.inset(&self.base_sel, self.amount)?,
            0 => m.inset_region(&self.base_sel, self.amount * self.limit)?,
            1 => m.bevel(&self.base_sel, self.amount * self.limit, self.segments)?,
            2 => m.loop_cut(&self.ring, self.segments, self.amount)?,
            3 => m.bevel_verts(&self.base_sel, self.amount * self.limit)?,
            4 => m.subdivide(&self.base_sel, self.segments)?,
            5 => m.smooth(&self.base_sel, self.amount, self.segments)?,
            7 => m.grid_fill(&self.base_sel, self.segments, self.amount.round().max(0.0) as u32).ok()?,
            _ => m.merge_by_distance(&self.base_sel, self.amount * self.limit)?,
        };
        Some((m, s))
    }
    fn default_amount(&self) -> f32 {
        match self.kind {
            0 | 1 | 3 => 0.25,
            5 => 0.5,
            6 => 0.001,
            _ => 0.0,
        }
    }
    /// L'outil se règle à la souris (les autres s'appliquent tout de suite, réglables dans le panneau)
    fn interactive(&self) -> bool {
        matches!(self.kind, 0 | 1 | 2 | 3)
    }
    fn name(&self) -> &'static str {
        ["incruster", "biseau", "coupe en boucle", "biseau des sommets", "subdiviser", "lisser les sommets", "fusionner par distance", "remplir en grille"][self.kind.min(7) as usize]
    }
    fn amount_label(&self) -> &'static str {
        match self.kind {
            0 => "épaisseur :",
            1 | 3 => "largeur :",
            2 => "glissement :",
            5 => "force :",
            6 => "distance :",
            7 => "décalage :",
            _ => "",
        }
    }
    fn segments_label(&self) -> Option<&'static str> {
        match self.kind {
            1 => Some("segments"),
            2 | 4 => Some("coupes"),
            5 => Some("répétitions"),
            7 => Some("travée"),
            _ => None,
        }
    }
    /// Valeur montrée (et tapée) : la vraie distance quand il y en a une
    fn shown(&self) -> f32 {
        if self.scaled() { self.amount * self.limit } else { self.amount }
    }
    fn scaled(&self) -> bool {
        matches!(self.kind, 1 | 3 | 6) || (self.kind == 0 && !self.alt)
    }
    fn from_shown(&self, v: f32) -> f32 {
        match self.kind {
            2 => v.clamp(-1.0, 1.0),
            5 => v.clamp(0.0, 1.0),
            7 => v.round().clamp(0.0, self.limit),
            0 if self.alt => v.clamp(0.01, 0.95),
            _ => (v / self.limit.max(1e-9)).clamp(0.0, if self.kind == 0 { 4.0 } else { 1.0 }),
        }
    }
    /// Bornes du réglage dans le panneau (valeur montrée) et pas du glissement
    fn range(&self) -> (f32, f32, f32) {
        match self.kind {
            0 if self.alt => (0.01, 0.95, 0.005),
            0 => (0.0, self.limit * 4.0, self.limit * 0.005),
            2 => (-1.0, 1.0, 0.01),
            5 => (0.0, 1.0, 0.01),
            7 => (0.0, self.limit, 0.1),
            _ => (0.0, self.limit, self.limit * 0.005),
        }
    }
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
/// Cadrage serré d'un modèle ouvert seul : le modèle et une petite marge (pas le minimum d'une scène)
pub fn tight_bounds(lo: V3, hi: V3) -> (V3, V3) {
    let m = ((hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]) * 0.15).max(1e-3);
    ([lo[0] - m, lo[1] - m, lo[2] - m], [hi[0] + m, hi[1] + m, hi[2] + m])
}

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
    let focal = w.min(h) as f32 * 1.2 * cam.zoom;
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

/// Miniature d'un matériau pour les bibliothèques : la sphère seule, sur fond transparent
pub fn render_material_thumb(sphere: &Prepared, prog: std::sync::Arc<crate::nodes::Program>, size: usize) -> crate::nodes::Img {
    let mut img = render_material_preview(sphere, prog, size);
    let look = Look { material: Material::Gray, ..Default::default() };
    // Le masque de la sphère : là où un rendu sans fond a quelque chose
    let f = render(sphere, &Camera { yaw: 0.4, pitch: 0.25, dist: 2.0, ..Default::default() }, &look, size, size);
    for (k, p) in img.px.iter_mut().enumerate() {
        let a = f.rgba.get(k * 4 + 3).copied().unwrap_or(255) as f32 / 255.0;
        p[3] = a;
    }
    img
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
    /// Ctrl+J : joindre les objets choisis à l'actif (fait au début de l'image suivante)
    join_request: bool,
    /// Tourner autour de la sélection (comme l'option de Blender), et le point retenu pour ce geste
    orbit_sel: bool,
    orbit_pivot: Option<V3>,
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
    /// Fenêtre de la timeline 3D (première, dernière image affichées) ; None : toute la plage de la scène
    tl_view: Option<(f32, f32)>,
    anim_playing: bool,
    anim_auto: bool,
    anim_last: Option<std::time::Instant>,
    /// Transformation au clavier en cours (G, R, S) : genre, axe imposé, pose de départ, position de la souris au départ
    modal: Option<(u8, Option<usize>, ([f32; 3], [f32; 3], [f32; 3]), Pos2)>,
    /// Menu « ajouter » ouvert sous le curseur (Maj+A)
    add_popup: Option<Pos2>,
    /// Curseur 3D (dans la scène) : point de départ des nouveaux objets et pivot possible
    cursor3d: V3,
    /// Point de pivot des rotations et des tailles : 0 point médian, 1 curseur 3D, 2 origines individuelles, 3 objet actif
    pivot: u8,
    /// Menu circulaire ouvert : 0 curseur (Maj+S), 1 point de pivot (.), et son centre
    pie: Option<(u8, Pos2)>,
    /// Où dessiner le curseur 3D à cette image (calculé avant la vue, dessiné après)
    cursor_screen: Option<Pos2>,
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
    sculpt: Option<SculptUi>,
    /// Mode édition : sommets déplacés pendant le geste, affichés sur place ; écrits au relâché
    mesh_moved: bool,
    /// Trajectoire de l'objet choisi : signature, points (une image sur deux au plus) et positions des clés
    motion_path: Option<(String, Vec<V3>, Vec<(i32, V3)>)>,
    mesh_unsaved: bool,
    /// Correspondance sommets affichés → sommets du maillage édité (objet, nombre de sommets, liste)
    mesh_map: Option<(usize, usize, Vec<(u32, u32)>)>,
    /// Demande d'entrer ou de sortir du mode édition (bouton du panneau)
    mesh_toggle: bool,
    /// Scène créée pour un modèle ouvert seul : la vue cadre le modèle, pas une scène de 12 m
    tight_frame: bool,
    /// Mode édition : voir et choisir aussi les sommets cachés derrière le maillage (rayons X, Alt+Z)
    xray: bool,
    want_anim: bool,
    anim_job: Option<AnimJob>,
    /// Suivi de caméra : demandé depuis le menu, en cours, et les images de la vidéo de fond (avec leur clé)
    want_matchmove: bool,
    /// Réglages avant de lancer : la vidéo, « estimer le champ », champ horizontal connu (degrés)
    mm_dialog: Option<(std::path::PathBuf, bool, f32)>,
    mm_job: Option<crate::matchmove::Job>,
    backdrop: Option<(String, crate::matchmove::BackdropSlot)>,
    /// La vue de la frame précédente (l'en-tête et les outils flottent par-dessus)
    vp_rect: Option<egui::Rect>,
    hdr_h: f32,
    /// Images à côté de la scène, montrées en références sous la liste (lues une fois)
    scene_refs: Option<Vec<std::path::PathBuf>>,
    prop_tab: PropTab,
    /// Éditeur de courbes à la place de la timeline
    anim_graph: bool,
    /// L'éditeur de courbes dans sa propre fenêtre du système (on peut la sortir de l'application)
    graph_window: bool,
    /// Peinture sur le modèle (comme Substance Painter) : le pinceau quand le mode est actif
    paint3d: Option<crate::paint3d::Brush>,
    /// Textures de peinture par objet de la scène (chemin dans le coffre, image), et le trait en cours
    paint_tex: Vec<Option<crate::paint3d::Stack>>,
    /// Valeur peinte sur un calque de rugosité, de métal ou de masque (0 à 1)
    paint_value: f32,
    /// Calque à rendre actif dès que l'objet l'a en mémoire (objet, nom du calque)
    paint_pick: Option<(usize, String)>,
    paint_last: Option<Pos2>,
    /// Rayon du pinceau en 3D (unités de la vue), mesuré sous la souris
    paint_r_uv: f32,
    /// UV automatiques de chaque objet peint (nombre de triangles au calcul, UV par triangle)
    paint_uv: Vec<Option<(usize, std::sync::Arc<Vec<[[f32; 2]; 3]>>)>>,
    paint_dirty: Vec<usize>,
    paint_saved: Vec<usize>,
    /// Annuler, rétablir : (objet, texture avant)
    paint_undo: Vec<(usize, usize, image::RgbaImage)>,
    paint_redo: Vec<(usize, usize, image::RgbaImage)>,
    /// Décalque : une image projetée sur l'objet au clic (chemin, image)
    paint_stamp: Option<(String, std::sync::Arc<image::RgbaImage>)>,
    /// Barre d'outils d'animation (tweener, décaler, poses…)
    anim_tools: bool,
    /// Pelure d'oignon : l'objet choisi aux clés d'avant et d'après, en fil de fer
    onion: bool,
    /// Vue par la caméra de la scène (0) : la vue suit la caméra, même animée
    look_through: bool,
    /// Fantômes calculés : signature, puis segments (monde) par clé avec leur côté (-1 avant, 1 après)
    ghosts: Option<(String, Vec<(i32, Vec<[V3; 2]>)>)>,
    tween_w: f32,
    /// Pose copiée, par nom d'objet
    pose_clip: Vec<(String, crate::animtools::Pose)>,
    /// Bibliothèque de poses : nom à donner, force du mélange
    pose_name: String,
    pose_mix: f32,
    /// Texte 3D à créer (fenêtre ouverte, texte, hauteur, épaisseur, biseau)
    text3d: Option<(String, f32, f32, u32)>,
    /// Clé choisie dans la timeline (objet, image), clé qu'on glisse (objet, image de départ, image actuelle)
    key_sel: Option<(usize, i32)>,
    key_drag: Option<(usize, i32, i32)>,
    key_menu: Option<(usize, i32)>,
    /// Montage d'animation : bande choisie, bande qu'on glisse (indice, par son bord droit, début et répétitions au départ, souris au départ), bande du menu
    strip_sel: Option<usize>,
    strip_drag: Option<(usize, bool, i32, f32, f32)>,
    strip_menu: Option<usize>,
    curve_ed: crate::curves::Editor,
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
        let p = path.clone();
        Self::open_with(path, ctx, move || load(&p))
    }

    /// Un modèle ouvert seul arrive dans une scène toute prête (le modèle, un soleil, une caméra) :
    /// édition, modificateurs, UV et matériaux marchent tout de suite. La scène n'est écrite à côté du
    /// modèle (« robot.aryscene ») qu'au premier changement ; une scène qui le contient déjà est reprise.
    pub fn open_in_new_scene(model: std::path::PathBuf, ctx: &egui::Context) -> Self {
        let dir = model.parent().unwrap_or(std::path::Path::new("")).to_path_buf();
        let stem = model.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "modèle".into());
        let name = model.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ext = crate::formats::ext("scene");
        let mut out = dir.join(format!("{stem}.{ext}"));
        let mut n = 2;
        while out.exists() {
            // Une scène de ce nom qui contient déjà ce modèle : on la rouvre
            if crate::scene::Scene::read(&out).is_ok_and(|sc| sc.objects.iter().any(|o| o.file.rsplit(['/', '\\']).next() == Some(name.as_str()))) {
                return Self::open(out, ctx);
            }
            out = dir.join(format!("{stem} {n}.{ext}"));
            n += 1;
        }
        let mut sc = crate::scene::Scene::default();
        sc.add(&out, &model);
        let l = sc.add_role("soleil", "light");
        (sc.objects[l].pos, sc.objects[l].rot) = ([4.0, 6.0, 3.0], [35.0, 0.0, -35.0]);
        let c = sc.add_role("caméra", "camera");
        (sc.objects[c].pos, sc.objects[c].rot) = ([7.0, 5.0, 7.0], [-28.0, 45.0, 0.0]);
        let (sc2, p) = (sc.clone(), out.clone());
        let mut v = Self::open_with(out, ctx, move || crate::scene::build(&sc2, &p).0);
        v.scene = Some(sc);
        v.scene_sel = Some(0);
        // Comme une scène lue du disque : couleurs des objets, et un premier passage qui cadre la vue
        // sur la scène (sinon le premier cadrage arrivait en mode édition, objet caché : vue trop large)
        v.look.material = Material::Color;
        v.scene_dirty = true;
        v.tight_frame = true;
        v
    }

    fn open_with(path: std::path::PathBuf, ctx: &egui::Context, loader: impl FnOnce() -> Result<Model, String> + Send + 'static) -> Self {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(loader());
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
            join_request: false,
            orbit_sel: true,
            orbit_pivot: None,
            scene_errors: vec![],
            scene_rx: None,
            scene_ranges: vec![],
            gizmo_hold: None,
            gizmo_hot: None,
            gizmo_mode: 0,
            ref_tex: Default::default(),
            anim_frame: 0.0,
            tl_view: None,
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
            mesh_moved: false,
            sculpt: None,
            motion_path: None,
            mesh_unsaved: false,
            mesh_map: None,
            mesh_toggle: false,
            tight_frame: false,
            want_anim: false,
            anim_job: None,
            want_matchmove: false,
            mm_dialog: None,
            mm_job: None,
            backdrop: None,
            vp_rect: None,
            hdr_h: 30.0,
            scene_refs: None,
            prop_tab: PropTab::Object,
            anim_graph: false,
            graph_window: false,
            paint3d: None,
            paint_tex: vec![],
            paint_value: 1.0,
            paint_pick: None,
            paint_last: None,
            paint_r_uv: 0.02,
            paint_uv: vec![],
            paint_dirty: vec![],
            paint_saved: vec![],
            paint_undo: vec![],
            paint_redo: vec![],
            paint_stamp: None,
            anim_tools: false,
            onion: false,
            look_through: false,
            ghosts: None,
            tween_w: 0.5,
            pose_clip: vec![],
            pose_name: String::new(),
            pose_mix: 1.0,
            text3d: None,
            key_sel: None,
            key_drag: None,
            key_menu: None,
            strip_sel: None,
            strip_drag: None,
            strip_menu: None,
            curve_ed: Default::default(),
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
            cursor3d: [0.0; 3],
            pivot: 0,
            pie: None,
            cursor_screen: None,
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

    /// Les UV de peinture d'un objet de la scène, calculées une fois (et refaites si son maillage change)
    fn paint_uvs(&mut self, k: usize) -> Option<std::sync::Arc<Vec<[[f32; 2]; 3]>>> {
        let &(_, a, b) = self.scene_ranges.iter().find(|r| r.0 == k)?;
        if let Some(Some((n, uv))) = self.paint_uv.get(k) {
            if *n == b - a {
                return Some(uv.clone());
            }
        }
        let prep = self.prepared.as_ref()?.2.clone();
        if b > prep.tris.len() {
            return None;
        }
        let edges: Vec<u8> = (a..b).map(|t| prep.edges.get(t).copied().unwrap_or(7)).collect();
        let uv = std::sync::Arc::new(crate::paint3d::auto_uv(&prep.pos, &prep.tris[a..b], &edges));
        if self.paint_uv.len() <= k {
            self.paint_uv.resize(k + 1, None);
        }
        self.paint_uv[k] = Some((b - a, uv.clone()));
        Some(uv)
    }

    /// Peinture sur le modèle : le pinceau sous la souris, les touches dans la texture de l'objet touché,
    /// et la barre du pinceau en bas de la vue
    fn paint_ui(&mut self, ui: &mut egui::Ui, rect: egui::Rect, resp: &egui::Response) {
        let p = theme::pal();
        let Some(mut b) = self.paint3d else { return };
        let typing = ui.ctx().egui_wants_keyboard_input();
        // Un calque choisi ou ajouté dans la liste devient l'actif dès que ses images sont là
        if let Some((k, name)) = self.paint_pick.clone() {
            if let Some(Some(st)) = self.paint_tex.get_mut(k) {
                if let Some(i) = st.layers.iter().position(|l| l.0.name == name) {
                    st.active = i;
                    self.paint_pick = None;
                }
            }
        }
        // Le calque actif de l'objet choisi : hors couleur, le pinceau pose une valeur
        let sel_stack = self.scene_sel.and_then(|k| self.paint_tex.get(k)).and_then(|s| s.as_ref());
        let sel_active = sel_stack.map(|s| s.active).unwrap_or(0);
        let value_layer = sel_stack.and_then(|s| s.layers.get(s.active)).is_some_and(|l| l.0.channel() != crate::paint3d::Channel::Color);
        if !typing && resp.hovered() {
            if ui.input(|i| i.key_pressed(Key::OpenBracket)) {
                b.radius = (b.radius / 1.15).max(2.0);
            }
            if ui.input(|i| i.key_pressed(Key::CloseBracket)) {
                b.radius = (b.radius * 1.15).min(400.0);
            }
            if ui.input(|i| i.key_pressed(Key::E) && !i.modifiers.any()) {
                b.erase = !b.erase;
            }
        }
        if !typing && ui.input(|i| i.modifiers.command && i.key_pressed(Key::Z)) {
            let redo = ui.input(|i| i.modifiers.shift);
            let (from, to) = if redo { (&mut self.paint_redo, &mut self.paint_undo) } else { (&mut self.paint_undo, &mut self.paint_redo) };
            if let Some((k, li, img)) = from.pop() {
                if let Some(Some(st)) = self.paint_tex.get_mut(k) {
                    if let Some((_, t)) = st.layers.get(li) {
                        if let Ok(mut cur) = t.write() {
                            to.push((k, li, std::mem::replace(&mut *cur, img)));
                        }
                    }
                    if let Some(d) = st.dirty.get_mut(li) {
                        *d = true;
                    }
                    st.recompose(None);
                    self.paint_dirty.push(k);
                    self.slot_rev += 1;
                }
            }
        }
        // Le cercle du pinceau
        if let Some(h) = resp.hover_pos() {
            let col = if b.erase { p.text } else { Color32::from_rgb((b.color[0] * 255.0) as u8, (b.color[1] * 255.0) as u8, (b.color[2] * 255.0) as u8) };
            ui.painter_at(rect).circle_stroke(h, b.radius, Stroke::new(1.5, col));
            ui.painter_at(rect).circle_stroke(h, b.radius * b.hardness.clamp(0.05, 1.0), Stroke::new(1.0, col.gamma_multiply(0.5)));
        }
        let alt = ui.input(|i| i.modifiers.alt);
        let stamping = self.paint_stamp.is_some();
        let painting = if stamping {
            // Un décalque se pose une fois, au clic (pas en traînée)
            !alt && (resp.drag_started_by(egui::PointerButton::Primary) || resp.clicked()) && self.paint_last.is_none()
        } else {
            resp.dragged_by(egui::PointerButton::Primary) && !alt || (resp.clicked() && !alt)
        };
        let stamp = self.paint_stamp.as_ref().map(|x| x.1.clone());
        if painting {
            let scene_names: Vec<String> = self.scene.as_ref().map(|sc| sc.objects.iter().map(|o| o.name.clone()).collect()).unwrap_or_default();
            if let (Some(at), Some((_, _, prep))) = (resp.interact_pointer_pos(), self.prepared.clone()) {
                let (w, h) = (rect.width(), rect.height());
                let from = self.paint_last.unwrap_or(at);
                // Les UV de chaque objet sous le trait (calculées une fois par objet)
                let cam = self.cam.clone();
                let ranges = self.scene_ranges.clone();
                // Cinq pointages par touche : l'arbre de rayons, construit une fois par maillage, les rend presque gratuits
                prep.ensure_tree();
                let hit = |q: Pos2| pick(&prep, &cam, w, h, q.x - rect.left(), q.y - rect.top());
                let ks: Vec<usize> = [from, at].iter().filter_map(|q| hit(*q)).filter_map(|(tri, _)| ranges.iter().find(|r| (r.1..r.2).contains(&tri)).map(|r| r.0)).collect();
                let uvs: Vec<(usize, usize, std::sync::Arc<Vec<[[f32; 2]; 3]>>)> = ks.into_iter().filter_map(|k| Some((k, ranges.iter().find(|r| r.0 == k)?.1, self.paint_uvs(k)?))).collect();
                // Le point touché en 3D (objet, position, normale de la face)
                let at3 = |q: Pos2| -> Option<(usize, [f32; 3], [f32; 3])> {
                    let (tri, bary) = hit(q)?;
                    let k = ranges.iter().find(|r| (r.1..r.2).contains(&tri))?.0;
                    let t = prep.tris.get(tri)?;
                    let p = t.map(|i| prep.pos[i as usize]);
                    let c = [0, 1, 2].map(|j| p[0][j] * bary[0] + p[1][j] * bary[1] + p[2][j] * bary[2]);
                    let n = cross(sub(p[1], p[0]), sub(p[2], p[0]));
                    Some((k, c, n))
                };
                let stem = self.path.file_stem().map(|s| crate::formats::strip(&s.to_string_lossy()).to_string()).unwrap_or_else(|| "scène".into());
                for q in crate::paint3d::spaced([from.x, from.y], [at.x, at.y], b.radius) {
                    let q = Pos2::new(q[0], q[1]);
                    let Some((k, c, n)) = at3(q) else { continue };
                    let Some((_, first, uv)) = uvs.iter().find(|x| x.0 == k).cloned() else { continue };
                    // Le rayon en 3D : la distance aux points touchés un rayon plus loin à l'écran (le plus court,
                    // pour ne pas sauter vers un autre pan du modèle)
                    let side = [q + egui::vec2(b.radius, 0.0), q + egui::vec2(0.0, b.radius), q - egui::vec2(b.radius, 0.0), q - egui::vec2(0.0, b.radius)];
                    let r3 = side.iter().filter_map(|s2| at3(*s2).filter(|x| x.0 == k)).map(|x| dot(sub(x.1, c), sub(x.1, c)).sqrt()).fold(f32::MAX, f32::min);
                    if r3 < f32::MAX {
                        self.paint_r_uv = r3;
                    }
                    // La texture de l'objet : créée au premier coup de pinceau
                    if self.paint_tex.len() <= k {
                        self.paint_tex.resize_with(k + 1, || None);
                    }
                    if self.paint_tex[k].is_none() {
                        let name = scene_names.get(k).cloned().unwrap_or_default();
                        let layer = crate::paint3d::PaintLayer { name: "couleur".into(), channel: "couleur".into(), file: format!("textures/{stem} - {name} - peinture.png"), opacity: 1.0, visible: true };
                        // La scène retient tout de suite le calque (sinon son chargement l'oublierait)
                        if let Some(o) = self.scene.as_mut().and_then(|sc| sc.objects.get_mut(k)) {
                            crate::paint3d::set_layers(o, std::slice::from_ref(&layer));
                        }
                        self.paint_tex[k] = Some(crate::paint3d::Stack::new(vec![(layer, std::sync::Arc::new(std::sync::RwLock::new(crate::paint3d::blank())))]));
                    }
                    if let Some(st) = self.paint_tex[k].as_mut() {
                        // Le pinceau peint le calque actif de l'objet touché ; hors couleur, il pose une valeur de gris
                        let li = st.active.min(st.layers.len().saturating_sub(1));
                        let Some((meta, t)) = st.layers.get(li).map(|l| (l.0.clone(), l.1.clone())) else { continue };
                        let b = if meta.channel() == crate::paint3d::Channel::Color { b } else { crate::paint3d::Brush { color: [self.paint_value.clamp(0.0, 1.0); 3], ..b } };
                        // Annuler : l'état du calque avant le trait (une fois par trait et par objet)
                        if !self.paint_dirty.contains(&k) {
                            if let Ok(img) = t.read() {
                                self.paint_undo.push((k, li, img.clone()));
                                if self.paint_undo.len() > 8 {
                                    self.paint_undo.remove(0);
                                }
                                self.paint_redo.clear();
                            }
                        }
                        let mut touched: Option<[u32; 4]> = None;
                        if let Ok(mut img) = t.write() {
                            let (_, _, last) = ranges.iter().find(|r| r.0 == k).copied().unwrap_or((k, first, first + uv.len()));
                            touched = match &stamp {
                                // Décalque : l'image projetée depuis la caméra, centrée sur le point cliqué, dans le cercle du pinceau
                                Some(si) => {
                                    let (sw, sh) = (si.width() as f32, si.height() as f32);
                                    let side = sw.max(sh);
                                    let rr = b.radius;
                                    let f = |pt: [f32; 3]| -> Option<[f32; 4]> {
                                        let s2 = crate::model3d::project_point(&cam, w, h, pt)?;
                                        let (dx, dy) = (s2[0] + rect.left() - q.x, s2[1] + rect.top() - q.y);
                                        let (u, v) = (dx / (2.0 * rr) * side / sw + 0.5, dy / (2.0 * rr) * side / sh + 0.5);
                                        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                                            return None;
                                        }
                                        let p = si.get_pixel((u * sw) as u32, (v * sh) as u32);
                                        Some([p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0, p[3] as f32 / 255.0])
                                    };
                                    crate::paint3d::dab3d_with(&mut img, &prep.pos, &prep.tris[first..last.min(prep.tris.len())], &uv, c, n, self.paint_r_uv * 1.42, &b, Some(&f))
                                }
                                None => crate::paint3d::dab3d(&mut img, &prep.pos, &prep.tris[first..last.min(prep.tris.len())], &uv, c, n, self.paint_r_uv, &b),
                            };
                        }
                        if let Some(d) = st.dirty.get_mut(li) {
                            *d = true;
                        }
                        // Les calques ne sont réunis que là où la touche a peint
                        if touched.is_some() {
                            st.recompose(touched);
                        }
                    }
                    if !self.paint_dirty.contains(&k) {
                        self.paint_dirty.push(k);
                    }
                    self.slot_rev += 1;
                }
                self.paint_last = Some(at);
                ui.ctx().request_repaint();
            }
        }
        // Fin du trait : les textures touchées s'enregistrent, et la scène retient leur chemin
        if (resp.drag_stopped() || resp.clicked() || !ui.input(|i| i.pointer.primary_down())) && !self.paint_dirty.is_empty() {
            self.paint_last = None;
            for k in std::mem::take(&mut self.paint_dirty) {
                let Some(Some(st)) = self.paint_tex.get_mut(k) else { continue };
                for (i, (l, t)) in st.layers.iter().enumerate() {
                    if !st.dirty.get(i).copied().unwrap_or(false) {
                        continue;
                    }
                    let full = std::path::Path::new(&self.root).join(&l.file);
                    if let Some(d) = full.parent() {
                        let _ = std::fs::create_dir_all(d);
                    }
                    if let Ok(img) = t.read() {
                        let _ = img.save(&full);
                    }
                }
                st.dirty.iter_mut().for_each(|d| *d = false);
                // Enregistrer la scène (les calques de l'objet) sans tout reconstruire
                if let Some(sc) = self.scene.as_ref() {
                    if !self.paint_saved.contains(&k) {
                        let _ = sc.write(&self.path);
                        self.paint_saved.push(k);
                    }
                }
            }
        } else if !ui.input(|i| i.pointer.primary_down()) {
            self.paint_last = None;
        }
        // La barre du pinceau, en bas au milieu de la vue
        let bar = egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.bottom() - 30.0), egui::vec2(640.0f32.min(rect.width() - 20.0), 40.0));
        let mut leave = false;
        let mut clear = false;
        let stem = self.path.file_stem().map(|s| crate::formats::strip(&s.to_string_lossy()).to_string()).unwrap_or_else(|| "scène".into());
        let mut bui = ui.new_child(egui::UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)));
        egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).corner_radius(8.0).inner_margin(egui::Margin::symmetric(10, 5)).show(&mut bui, |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(egui::RichText::new(crate::lucide::PAINTBRUSH).size(15.0).color(p.accent));
            if value_layer {
                // Rugosité, métal, masque : une valeur de 0 à 1 à la place de la couleur
                ui.spacing_mut().slider_width = 60.0;
                if ui.add(egui::Slider::new(&mut self.paint_value, 0.0..=1.0).fixed_decimals(2)).on_hover_text(crate::i18n::t("valeur peinte sur ce calque (0 : rien, lisse ou non métallique ; 1 : plein, rugueux ou métallique)")).changed() {
                    b.erase = false;
                }
            } else {
                let mut c = b.color;
                if ui.color_edit_button_rgb(&mut c).on_hover_text(crate::i18n::t("couleur")).changed() {
                    b.color = c;
                    b.erase = false;
                }
            }
            // Calques de peinture de l'objet choisi : couleur, rugosité, métal, masques pour l'Atelier
            egui::containers::menu::MenuButton::new(egui::RichText::new(crate::lucide::LAYERS).size(15.0)).ui(ui, |ui| {
                ui.set_min_width(290.0);
                ui.label(crate::theme::section(&crate::i18n::t("CALQUES DE PEINTURE")));
                let Some((k, oname, before)) = self.scene_sel.and_then(|k| self.scene.as_ref()?.objects.get(k).map(|o| (k, o.name.clone(), crate::paint3d::layers_of(o)))) else {
                    ui.label(egui::RichText::new(crate::i18n::t("Choisis d'abord un objet de la scène.")).size(11.0).color(p.muted));
                    return;
                };
                let mut layers = before.clone();
                let (mut pick, mut remove) = (None, None);
                if layers.is_empty() {
                    ui.label(egui::RichText::new(crate::i18n::t("Pas encore de calque : peins, ou ajoutes-en un.")).size(11.0).color(p.muted));
                }
                // Le calque du dessus en premier, comme dans une pile
                for i in (0..layers.len()).rev() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let l = &mut layers[i];
                        let eye = if l.visible { crate::lucide::EYE } else { crate::lucide::EYE_OFF };
                        if ui.add(egui::Button::new(egui::RichText::new(eye).size(13.0).color(if l.visible { p.text } else { p.muted })).frame(false)).on_hover_text(crate::i18n::t("montrer ou cacher ce calque")).clicked() {
                            l.visible = !l.visible;
                        }
                        let (label, icon) = crate::paint3d::CHANNELS.iter().find(|c| c.0 == l.channel).map(|c| (c.1, c.2)).unwrap_or(("couleur", crate::lucide::PALETTE));
                        ui.label(egui::RichText::new(icon).size(13.0).color(p.muted)).on_hover_text(crate::i18n::t(label));
                        if ui.selectable_label(i == sel_active, egui::RichText::new(&l.name).size(12.0)).on_hover_text(crate::i18n::t("peindre sur ce calque")).clicked() {
                            pick = Some(l.name.clone());
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::TRASH_2).size(12.0).color(p.muted)).frame(false)).on_hover_text(crate::i18n::t("retirer ce calque (son image reste dans le coffre)")).clicked() {
                                remove = Some(i);
                            }
                            let mut pct = l.opacity * 100.0;
                            if ui.add(egui::DragValue::new(&mut pct).range(0.0..=100.0).speed(1.0).suffix(" %")).on_hover_text(crate::i18n::t("opacité du calque")).changed() {
                                l.opacity = (pct / 100.0).clamp(0.0, 1.0);
                            }
                        });
                    });
                }
                ui.separator();
                ui.menu_button(format!("{}  {}", crate::lucide::PLUS, crate::i18n::t("ajouter un calque")), |ui| {
                    for (id, label, icon) in crate::paint3d::CHANNELS {
                        if ui.button(format!("{icon}  {}", crate::i18n::t(label))).clicked() {
                            let name = crate::paint3d::free_name(&layers, id);
                            layers.push(crate::paint3d::PaintLayer { name: name.clone(), channel: id.to_string(), file: format!("textures/{stem} - {oname} - {name}.png"), opacity: 1.0, visible: true });
                            pick = Some(name);
                            ui.close();
                        }
                    }
                });
                if let Some(i) = remove {
                    layers.remove(i);
                }
                if layers != before {
                    if let Some(sc) = self.scene.as_mut() {
                        if let Some(o) = sc.objects.get_mut(k) {
                            crate::paint3d::set_layers(o, &layers);
                        }
                        let _ = sc.write(&self.path);
                    }
                    self.slot_rev += 1;
                }
                if let Some(name) = pick {
                    self.paint_pick = Some((k, name));
                }
            })
            .0
            .on_hover_text(crate::i18n::t("calques de peinture de l'objet choisi : couleur, rugosité, métal, et masques peints pour les calques de l'Atelier"));
            let er = egui::Button::new(egui::RichText::new(crate::lucide::ERASER).size(15.0).color(if b.erase { p.on_accent } else { p.text })).fill(if b.erase { p.accent } else { Color32::TRANSPARENT }).min_size(egui::vec2(28.0, 26.0));
            if ui.add(er).on_hover_text(crate::i18n::t("gomme (E)")).clicked() {
                b.erase = !b.erase;
            }
            // Décalque : une image (logo, étiquette, motif) projetée sur l'objet là où on clique
            let on = self.paint_stamp.is_some();
            let st = egui::Button::new(egui::RichText::new(crate::lucide::STAMP).size(15.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { Color32::TRANSPARENT }).min_size(egui::vec2(28.0, 26.0));
            let tip = match &self.paint_stamp {
                Some((path, _)) => format!("{} : {}  ({})", crate::i18n::t("décalque"), std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), crate::i18n::t("clic : retirer")),
                None => crate::i18n::t("décalque : projeter une image sur l'objet au clic (logo, étiquette)"),
            };
            if ui.add(st).on_hover_text(tip).clicked() {
                if on {
                    self.paint_stamp = None;
                } else {
                    let mut d = rfd::FileDialog::new().add_filter("image", &["png", "jpg", "jpeg", "webp"]);
                    if !self.root.is_empty() {
                        d = d.set_directory(&self.root);
                    }
                    if let Some(f) = d.pick_file() {
                        match image::open(&f) {
                            Ok(im) => {
                                let im = im.to_rgba8();
                                // Assez grand pour être net, pas plus
                                let im = if im.width().max(im.height()) > 1024 { image::imageops::thumbnail(&im, 1024 * im.width() / im.width().max(im.height()), 1024 * im.height() / im.width().max(im.height())) } else { im };
                                self.paint_stamp = Some((f.to_string_lossy().to_string(), std::sync::Arc::new(im)));
                                b.erase = false;
                            }
                            Err(e) => crate::notify(format!("{} : {e}", crate::i18n::t("image illisible"))),
                        }
                    }
                }
            }
            ui.separator();
            ui.label(egui::RichText::new(crate::lucide::CIRCLE).size(13.0).color(p.muted)).on_hover_text(crate::i18n::t("rayon ([ et ])"));
            ui.add(egui::DragValue::new(&mut b.radius).range(2.0..=400.0).speed(1.0).suffix(" px"));
            ui.label(egui::RichText::new(crate::lucide::GAUGE).size(13.0).color(p.muted)).on_hover_text(crate::i18n::t("opacité"));
            ui.spacing_mut().slider_width = 70.0;
            ui.add(egui::Slider::new(&mut b.strength, 0.02..=1.0).show_value(false));
            ui.label(egui::RichText::new(crate::lucide::CIRCLE_DOT).size(13.0).color(p.muted)).on_hover_text(crate::i18n::t("dureté du bord"));
            ui.add(egui::Slider::new(&mut b.hardness, 0.0..=1.0).show_value(false));
            ui.separator();
            if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::TRASH_2).size(15.0)).fill(Color32::TRANSPARENT).min_size(egui::vec2(28.0, 26.0))).on_hover_text(crate::i18n::t("effacer la peinture de l'objet choisi")).clicked() {
                clear = true;
            }
            if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::X).size(15.0)).fill(Color32::TRANSPARENT).min_size(egui::vec2(28.0, 26.0))).on_hover_text(crate::i18n::t("quitter la peinture")).clicked() {
                leave = true;
            }
        });
        if clear {
            if let Some(k) = self.scene_sel {
                if let Some(Some(st)) = self.paint_tex.get_mut(k) {
                    let li = st.active.min(st.layers.len().saturating_sub(1));
                    if let Some((_, t)) = st.layers.get(li) {
                        if let Ok(mut img) = t.write() {
                            let (w, h) = img.dimensions();
                            self.paint_undo.push((k, li, std::mem::replace(&mut *img, image::RgbaImage::new(w, h))));
                        }
                    }
                    if let Some(d) = st.dirty.get_mut(li) {
                        *d = true;
                    }
                    st.recompose(None);
                    self.paint_dirty.push(k);
                    self.slot_rev += 1;
                }
            }
        }
        self.paint3d = if leave { None } else { Some(b) };
    }

    /// Ce que la vue affiche vraiment : matériaux par objet, lumières de la scène, images de référence.
    /// Les captures passent par ici aussi, pour donner la même image que la vue.
    fn render_setup(&mut self) -> (Look, Vec<RefPlane>, u64) {
        let mut look = self.look.clone();
        if self.edit.is_none() && self.scene.is_some() {
            look.slot_nodes = self.slot_progs.iter().map(|p| p.as_ref().map(|x| x.2.clone())).collect();
            let n = self.paint_tex.len();
            look.slot_paint = (0..n).map(|k| {
                let (tex, pbr, masks) = self.paint_tex[k].as_ref()?.render_maps()?;
                let uv = self.paint_uvs(k)?;
                let first = self.scene_ranges.iter().find(|r| r.0 == k)?.1;
                Some(crate::paint3d::Layer { tex, pbr, masks, uv, first })
            }).collect();
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
        // 720 lignes, au format de la vidéo de fond s'il y en a une (16:9 sinon)
        let aspect = scene.frame_aspect();
        let (w, h) = if aspect >= 1.0 { ((720.0 * aspect / 2.0).round() as usize * 2, 720usize) } else { (720usize, (720.0 / aspect / 2.0).round() as usize * 2) };
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

    /// L'image de la vidéo de fond à montrer maintenant (images décodées, indice, format du cadre) ;
    /// lance leur décodage la première fois qu'on regarde par la caméra.
    fn backdrop_frame(&mut self, ctx: &egui::Context) -> Option<(std::sync::Arc<crate::matchmove::BackdropFrames>, usize, f32)> {
        let b = self.scene.as_ref().and_then(|s| s.backdrop.clone()).filter(|_| self.look_through);
        let Some(b) = b else {
            if self.scene.as_ref().is_some_and(|s| s.backdrop.is_none()) {
                self.backdrop = None;
            }
            return None;
        };
        let key = format!("{}|{}|{}|{}", b.file, b.from, b.fps, b.frames);
        if self.backdrop.as_ref().map(|x| &x.0) != Some(&key) {
            let ctx = ctx.clone();
            self.backdrop = Some((key, crate::matchmove::load_backdrop(crate::scene::resolve(&self.path, &b.file), b.clone(), move || ctx.request_repaint())));
        }
        let frames = self.backdrop.as_ref()?.1.lock().ok()?.as_ref()?.as_ref().ok()?.clone();
        let k = b.frame_at(self.anim_frame)?.min(frames.px.len().checked_sub(1)?);
        Some((frames, k, self.scene.as_ref()?.frame_aspect()))
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

    /// Joindre (Ctrl+J) : les objets choisis entrent dans le maillage de l'objet actif, à leur place
    /// dans la scène, puis disparaissent de la liste.
    fn join_selected(&mut self) {
        let Some(a) = self.scene_sel else { return };
        let others: Vec<usize> = {
            let Some(scene) = self.scene.as_ref() else { return };
            let mut v: Vec<usize> = self.scene_multi.iter().copied().filter(|&k| k != a && k < scene.objects.len() && crate::scene::kind(&scene.objects[k]) == crate::scene::Kind::Model).collect();
            v.sort_unstable();
            v
        };
        if others.is_empty() {
            self.capture_msg = Some(crate::i18n::t("joindre : choisis au moins deux objets à maillage (Maj+clic), l'actif en dernier"));
            return;
        }
        let file = match self.own_mesh_file(a) {
            Ok(f) => f,
            Err(e) => {
                self.capture_msg = Some(e);
                return;
            }
        };
        let Some(scene) = self.scene.as_mut() else { return };
        let Ok(mut base) = crate::meshedit::EMesh::read_obj(&file) else { return };
        let origin = scene.world(a, [0.0; 3]);
        let cols = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(|e| sub(scene.world(a, e), origin));
        let Some(inv) = crate::meshedit::inverse(cols) else { return };
        for &k in &others {
            let o = &scene.objects[k];
            let prefix = format!("{} - ", self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
            let m = if o.file.starts_with(&prefix) && o.file.to_lowercase().ends_with(".obj") {
                crate::meshedit::EMesh::read_obj(&crate::scene::resolve(&self.path, &o.file)).ok()
            } else {
                crate::scene::source_model(&self.path, o).ok().map(|m| crate::meshedit::EMesh::from_model(&m))
            };
            let Some(mut m) = m else { continue };
            for p in m.pos.iter_mut() {
                let w = scene.world(k, *p);
                *p = crate::meshedit::apply(&inv, sub(w, origin));
            }
            base.append(&m);
        }
        if let Err(e) = base.write_obj(&file) {
            self.capture_msg = Some(e);
            return;
        }
        // Les maillages propres à cette scène des objets absorbés vont à la corbeille du coffre
        // (récupérables), sauf si un autre objet s'en sert encore
        let stem_prefix = format!("{} - ", self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
        let absorbed: Vec<String> = others.iter().map(|&k| scene.objects[k].file.clone()).filter(|f| f.starts_with(&stem_prefix) && f.to_lowercase().ends_with(".obj")).collect();
        for &k in others.iter().rev() {
            scene.remove(k);
        }
        let root = crate::nodes::vault_root();
        for f in absorbed {
            if scene.objects.iter().any(|o| o.file == f) {
                continue;
            }
            let path = crate::scene::resolve(&self.path, &f);
            if let Some(rel) = crate::vault::inside(&root, &path) {
                let _ = crate::vault::trash(&root, &rel);
            }
        }
        let shift = others.iter().filter(|&&k| k < a).count();
        self.scene_sel = Some(a - shift);
        self.scene_multi.clear();
        let _ = scene.write(&self.path);
        self.scene_dirty = true;
    }

    /// Séparer (P) : les faces choisies en mode édition deviennent un nouvel objet, au même endroit.
    fn add_split_object(&mut self, obj: usize, mesh: crate::meshedit::EMesh) {
        let Some(scene) = self.scene.as_mut() else { return };
        let Some(o) = scene.objects.get(obj) else { return };
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "scène".into());
        let dir = self.path.parent().unwrap_or(std::path::Path::new("")).to_path_buf();
        let base = format!("{} {}", o.name, crate::i18n::t_mid("(séparé)"));
        let (mut name, mut n) = (base.clone(), 2);
        while dir.join(format!("{stem} - {name}.obj")).exists() || scene.objects.iter().any(|x| x.name == name) {
            name = format!("{base} {n}");
            n += 1;
        }
        let file = format!("{stem} - {name}.obj");
        if let Err(e) = mesh.write_obj(&dir.join(&file)) {
            self.capture_msg = Some(e);
            return;
        }
        let mut o2 = o.clone();
        o2.name = name;
        o2.file = file;
        scene.objects.push(o2);
        let _ = scene.write(&self.path);
        self.scene_dirty = true;
    }

    /// Points de la sélection en espace normalisé (celui de la caméra) : sommets choisis en mode
    /// édition, sinon les objets choisis.
    fn selection_points(&self) -> Vec<V3> {
        let Some((lo, hi)) = self.scene_frame else { return vec![] };
        let longest = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
        let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let mut pts: Vec<V3> = vec![];
        if let (Some(me), Some(scene)) = (self.mesh.as_ref(), self.scene.as_ref()) {
            for (i, v) in me.mesh.pos.iter().enumerate() {
                if me.sel.get(i).copied().unwrap_or(false) {
                    let w = scene.world(me.obj, *v);
                    pts.push([0, 1, 2].map(|k| (w[k] - c[k]) / longest));
                }
            }
        } else if let Some((_, _, prep)) = self.prepared.as_ref() {
            let mut chosen: Vec<usize> = self.scene_multi.iter().copied().collect();
            chosen.extend(self.scene_sel);
            for &(o, t0, t1) in &self.scene_ranges {
                if chosen.contains(&o) {
                    for t in prep.tris.get(t0..t1).unwrap_or(&[]) {
                        for &v in t {
                            pts.push(prep.pos[v as usize]);
                        }
                    }
                }
            }
        }
        pts
    }

    /// Centre de la sélection (espace normalisé), s'il y en a une.
    fn selection_center(&self) -> Option<V3> {
        let pts = self.selection_points();
        let n = pts.len() as f32;
        (n > 0.0).then(|| pts.iter().fold([0.0f32; 3], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n, a[2] + p[2] / n]))
    }

    /// Cadre la vue sur la sélection (objets choisis, ou sommets choisis en mode édition) ; sans
    /// sélection, toute la scène.
    fn frame_selection(&mut self) {
        let Some((lo, hi)) = self.scene_frame else {
            self.cam = Camera { ortho: self.cam.ortho, ..Default::default() };
            return;
        };
        let longest = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
        let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let _ = (longest, c);
        let pts = self.selection_points();
        if pts.is_empty() {
            self.cam = Camera { ortho: self.cam.ortho, ..Default::default() };
            return;
        }
        let (mut a, mut b) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &pts {
            for k in 0..3 {
                a[k] = a[k].min(p[k]);
                b[k] = b[k].max(p[k]);
            }
        }
        let mid = [0, 1, 2].map(|k| (a[k] + b[k]) / 2.0);
        let size = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt().max(0.02);
        let r = self.cam.rotate(mid);
        self.cam.pan = [-r[0], -r[1]];
        self.cam.dist = (r[2] + size * 1.9).clamp(0.05, 50.0);
    }

    /// Actions du menu circulaire du curseur 3D (Maj+S).
    fn cursor_action(&mut self, i: usize) {
        let Some(scene) = self.scene.as_mut() else { return };
        // Mode édition : sur les sommets choisis de l'objet modifié
        if let Some(me) = self.mesh.as_mut() {
            let k = me.obj;
            if k >= scene.objects.len() {
                return;
            }
            let chosen: Vec<usize> = (0..me.mesh.pos.len()).filter(|&v| me.sel.get(v).copied().unwrap_or(false)).collect();
            let world = |v: V3| scene.world(k, v);
            let med = (!chosen.is_empty()).then(|| {
                let n = chosen.len() as f32;
                chosen.iter().fold([0.0f32; 3], |a, &v| {
                    let w = world(me.mesh.pos[v]);
                    [a[0] + w[0] / n, a[1] + w[1] / n, a[2] + w[2] / n]
                })
            });
            match i {
                0 => {
                    if let Some(m) = med {
                        self.cursor3d = m;
                    }
                }
                1 | 5 => {
                    // Les sommets vont au curseur (1 : tous au même point ; 5 : ensemble, en gardant leur forme)
                    let origin = scene.world(k, [0.0; 3]);
                    let cols = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(|e| sub(scene.world(k, e), origin));
                    if let (Some(inv), Some(m)) = (crate::meshedit::inverse(cols), med) {
                        me.hist.push((me.mesh.clone(), me.sel.clone()));
                        me.redo.clear();
                        let to_local = |w: V3| crate::meshedit::apply(&inv, sub(w, origin));
                        let target = self.cursor3d;
                        for &v in &chosen {
                            let w = world(me.mesh.pos[v]);
                            let nw = if i == 1 { target } else { [w[0] + target[0] - m[0], w[1] + target[1] - m[1], w[2] + target[2] - m[2]] };
                            me.mesh.pos[v] = to_local(nw);
                        }
                        me.vis = None;
                        let _ = me.mesh.write_obj(&me.file);
                        self.scene_dirty = true;
                    }
                }
                2 => self.cursor3d = [0.0; 3],
                3 => self.cursor3d = scene.world(k, [0.0; 3]),
                4 => self.cursor3d = self.cursor3d.map(|x| x.round()),
                _ => {}
            }
            return;
        }
        let mut sel: Vec<usize> = self.scene_multi.iter().copied().collect();
        if let Some(a) = self.scene_sel {
            if !sel.contains(&a) {
                sel.push(a);
            }
        }
        sel.retain(|&k| k < scene.objects.len());
        let origins: Vec<V3> = sel.iter().map(|&k| scene.world(k, [0.0; 3])).collect();
        let med = (!origins.is_empty()).then(|| {
            let n = origins.len() as f32;
            origins.iter().fold([0.0f32; 3], |a, w| [a[0] + w[0] / n, a[1] + w[1] / n, a[2] + w[2] / n])
        });
        match i {
            0 => {
                if let Some(m) = med {
                    self.cursor3d = m;
                }
            }
            1 | 5 => {
                let target = self.cursor3d;
                for (&k, w) in sel.iter().zip(&origins) {
                    // Seulement les objets sans parent : leur position est celle de la scène
                    if scene.parent_of(k).is_some() {
                        continue;
                    }
                    let nw = match (i, med) {
                        (5, Some(m)) => [w[0] + target[0] - m[0], w[1] + target[1] - m[1], w[2] + target[2] - m[2]],
                        _ => target,
                    };
                    scene.objects[k].pos = nw;
                }
                let _ = scene.write(&self.path);
                self.scene_dirty = true;
            }
            2 => self.cursor3d = [0.0; 3],
            3 => {
                if let Some(a) = self.scene_sel.filter(|&a| a < scene.objects.len()) {
                    self.cursor3d = scene.world(a, [0.0; 3]);
                }
            }
            4 => self.cursor3d = self.cursor3d.map(|x| x.round()),
            _ => {}
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
        self.mesh = Some(MeshEdit { obj: k, file, mesh, sel: vec![false; n], hist: vec![], redo: vec![], modal: None, boxing: None, skip_click: false, mode: 0, active: None, vis: None, op: None, last: None, loopcut: None, menu: None, trigger: None, split: None, prop: false, prop_r: 0.0, prop_d: (None, vec![]), mirror: false, snap_v: false, mirror_m: (None, vec![]), knife: None });
        self.scene_multi.clear();
        self.capture_msg = None;
    }

    /// Mode édition : affichage des sommets et des arêtes par-dessus la vue, sélection, et outils au clavier.
    fn mesh_edit_ui(&mut self, ui: &mut egui::Ui, rect: egui::Rect, resp: &egui::Response) {
        if !crate::scene::is_scene(&self.path) || self.edit.is_some() {
            return;
        }
        let (pivot_mode, cursor3d) = (self.pivot, self.cursor3d);
        let p = theme::pal();
        let typing = ui.ctx().egui_wants_keyboard_input();
        // Tab : egui s'en sert pour passer d'un champ à l'autre ; sur la vue, sans champ actif, il change de mode
        let any_tab = resp.hovered() && !self.kb_busy && self.modal.is_none() && ui.input(|i| i.key_pressed(Key::Tab));
        let tab = any_tab && !ui.input(|i| i.modifiers.shift);
        let shift_tab = any_tab && !tab;
        if any_tab {
            ui.ctx().memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
        self.kb_busy = ui.ctx().memory(|m| m.focused().is_some());
        let typing = typing && !any_tab;
        if std::mem::take(&mut self.mesh_toggle) || tab {
            if self.mesh.take().is_none() {
                self.enter_mesh_edit();
            }
        }
        let (Some((lo, hi)), Some(scene)) = (self.scene_frame, self.scene.as_ref()) else { return };
        let Some(obj) = self.mesh.as_ref().map(|m| m.obj) else {
            self.sculpt = None;
            return;
        };
        if obj >= scene.objects.len() || self.scene_sel != Some(obj) {
            self.mesh = None;
            self.sculpt = None;
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
        let focal = rect.height().min(rect.width()) * 1.2 * self.cam.zoom;
        let (cam, rc) = (self.cam, rect.center());
        // Rotation de la caméra préparée une fois : ces projections tournent sur tous les sommets, à chaque image
        let cam_proj = Proj::new(&cam, rect.width(), rect.height());
        let view = |w: V3| cam_proj.view([(w[0] - c[0]) / longest, (w[1] - c[1]) / longest, (w[2] - c[2]) / longest]);
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
        let freeze_vis = self.sculpt.as_ref().is_some_and(|s| s.stroke.is_some());
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
                Some((k, v)) if (*k == key || freeze_vis) && v.len() == me.mesh.pos.len() => v.clone(),
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
        // Centre de la sélection à l'écran : le repère des outils réglés à la souris
        let sel_center = |me: &MeshEdit| -> Option<Pos2> {
            let ch: Vec<usize> = (0..me.mesh.pos.len()).filter(|&i| me.sel.get(i).copied().unwrap_or(false)).collect();
            let n = ch.len().max(1) as f32;
            project(ch.iter().fold([0.0f32; 3], |a, &i| {
                let w = to_world(me.mesh.pos[i]);
                [a[0] + w[0] / n, a[1] + w[1] / n, a[2] + w[2] / n]
            }))
        };
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
        let idle = me.op.is_none() && me.modal.is_none() && me.loopcut.is_none() && me.boxing.is_none() && me.knife.is_none();
        // ---- Sculpture : le pinceau remplace la sélection et les outils du mode édition
        if let Some(sc) = self.sculpt.as_mut() {
            use crate::sculpt::Brush;
            let (shift, cmd, down, released) = ui.input(|i| (i.modifiers.shift, i.modifiers.command, i.pointer.primary_down(), i.pointer.primary_released()));
            if !typing && hovered {
                for (b, _, _, k) in crate::sculpt::BRUSHES {
                    if Key::from_name(k).is_some_and(|kk| key(kk)) {
                        sc.brush = b;
                    }
                }
                if ui.input(|i| i.key_pressed(Key::OpenBracket)) {
                    sc.radius = (sc.radius / 1.15).max(4.0);
                }
                if ui.input(|i| i.key_pressed(Key::CloseBracket)) {
                    sc.radius = (sc.radius * 1.15).min(600.0);
                }
            }
            // Annuler, rétablir
            if !typing && ui.input(|i| i.modifiers.command && i.key_pressed(Key::Z)) && sc.stroke.is_none() {
                let redo = ui.input(|i| i.modifiers.shift);
                let (from, to) = if redo { (&mut me.redo, &mut me.hist) } else { (&mut me.hist, &mut me.redo) };
                if let Some((m, sl)) = from.pop() {
                    to.push((std::mem::replace(&mut me.mesh, m), std::mem::replace(&mut me.sel, sl)));
                    let _ = me.mesh.write_obj(&me.file);
                    self.scene_dirty = true;
                    self.mesh_map = None;
                }
            }
            if sc.mirror.0 != me.mesh.pos.len() {
                sc.mirror = (me.mesh.pos.len(), me.mesh.mirror_map());
            }
            // Niveaux de multirésolution : lus une fois par maillage, gardés s'ils correspondent encore à lui
            if sc.mr_for.as_ref() != Some(&me.file) {
                sc.mr_for = Some(me.file.clone());
                sc.multires = crate::multires::Multires::load(&me.file).filter(|mr| mr.matches(&me.mesh));
            }
            let brush = if shift { Brush::Smooth } else { sc.brush };
            let r = sc.radius;
            // Le point de la surface sous la souris (le sommet visible le plus proche à l'écran),
            // et le rayon du pinceau ramené dans le repère du maillage, à sa profondeur
            let probe = |me: &MeshEdit, at: Pos2| -> Option<(V3, f32)> {
                let mut best: Option<(usize, f32)> = None;
                for i in 0..me.mesh.pos.len() {
                    if let Some(q) = project_v(me, i) {
                        let d = (q - at).length();
                        if d < r && best.is_none_or(|b| d < b.1) {
                            best = Some((i, d));
                        }
                    }
                }
                let (i, _) = best?;
                let c = me.mesh.pos[i];
                let q = view(to_world(c));
                let per_px = if cam.ortho { cam.dist / focal } else { -q[2] / focal } * longest;
                let v = crate::meshedit::apply(&inv, cam.unrotate([r * per_px, 0.0, 0.0]));
                Some((c, (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6)))
            };
            // Les sommets dans la sphère du pinceau, chacun avec son atténuation ; et son reflet sur X
            let sphere = |pos: &[V3], c: V3, rr: f32| -> Vec<(usize, f32)> {
                pos.iter()
                    .enumerate()
                    .filter_map(|(i, p)| {
                        let d = ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt();
                        (d < rr).then(|| (i, crate::sculpt::falloff(d / rr)))
                    })
                    .collect()
            };
            let sym = sc.sym;
            let sets = |pos: &[V3], c: V3, rr: f32| -> Vec<(Vec<(usize, f32)>, bool)> {
                let mut v = vec![(sphere(pos, c, rr), false)];
                // Pinceau au milieu : un seul passage, sinon il compterait deux fois
                if sym && c[0].abs() > rr * 0.05 {
                    v.push((sphere(pos, [-c[0], c[1], c[2]], rr), true));
                }
                v
            };
            // Un clic sur la vue (pas sur la barre du bas) commence un geste
            let bar = egui::Rect::from_center_size(pos2(rect.center().x, rect.bottom() - 52.0), egui::vec2(760.0f32.min(rect.width() - 16.0).max(200.0), 44.0));
            if let (true, true, Some(at)) = (pressed, hovered, cur) {
                if !bar.contains(at) {
                    if let Some((c, rr)) = probe(me, at) {
                        snapshot(me);
                        let grabbed = sets(&me.mesh.pos, c, rr);
                        sc.stroke = Some(SculptStroke { start: me.mesh.pos.clone(), grabbed, topo: crate::sculpt::Topo::new(&me.mesh), at0: at, last: at, centre: c });
                    }
                }
            }
            if let (Some(st), Some(at), true) = (sc.stroke.as_mut(), cur, down) {
                if brush == Brush::Grab {
                    // La souris bouge dans le plan de la vue, à la profondeur de la zone prise
                    let q = view(to_world(st.centre));
                    let per_px = if cam.ortho { cam.dist / focal } else { -q[2] / focal } * longest;
                    let d = at - st.at0;
                    let dl = crate::meshedit::apply(&inv, cam.unrotate([d.x * per_px, -d.y * per_px, 0.0]));
                    for (set, _) in &st.grabbed {
                        for &(i, _) in set {
                            me.mesh.pos[i] = st.start[i];
                        }
                    }
                    for (set, mirrored) in &st.grabbed {
                        let dd = if *mirrored { [-dl[0], dl[1], dl[2]] } else { dl };
                        crate::sculpt::grab(&mut me.mesh, set, dd);
                    }
                    st.last = at;
                    self.mesh_moved = true;
                } else if (at - st.last).length() > 1.5 || brush == Brush::Smooth {
                    // Des touches régulières le long du trait, quelle que soit la vitesse de la souris
                    let spacing = (r * 0.12).max(2.0);
                    let n = (((at - st.last).length() / spacing).ceil() as usize).clamp(1, 12);
                    for k in 1..=n {
                        let q = st.last + (at - st.last) * (k as f32 / n as f32);
                        let Some((c, rr)) = probe(me, q) else { continue };
                        let step = rr * 0.025 * sc.strength;
                        // Remaillage dynamique : la surface se subdivise sous le pinceau (et sous son reflet) avant la touche
                        if sc.dyntopo && brush != Brush::Smooth && me.mesh.pos.len() < 400_000 {
                            let mut added = crate::multires::refine(&mut me.mesh, c, rr, rr * sc.detail, 20_000);
                            if sym && c[0].abs() > rr * 0.05 {
                                added += crate::multires::refine(&mut me.mesh, [-c[0], c[1], c[2]], rr, rr * sc.detail, 20_000);
                            }
                            if added > 0 {
                                st.topo = crate::sculpt::Topo::new(&me.mesh);
                                // Les niveaux de multirésolution ne décrivent plus ce maillage
                                if sc.multires.take().is_some() {
                                    let _ = std::fs::remove_file(crate::multires::Multires::file_for(&me.file));
                                }
                            }
                        }
                        for (set, _) in sets(&me.mesh.pos, c, rr) {
                            crate::sculpt::dab(&mut me.mesh, &st.topo, brush, &set, step, cmd);
                        }
                    }
                    st.last = at;
                    self.mesh_moved = true;
                }
                ui.ctx().request_repaint();
            }
            if released || (!down && sc.stroke.is_some()) {
                sc.stroke = None;
                self.mesh_unsaved = true;
            }
            // Le cercle du pinceau
            let painter = ui.painter_at(rect);
            if let Some(at) = cur.filter(|c| rect.contains(*c) && !bar.contains(*c)) {
                let col = if cmd && brush != Brush::Smooth { Color32::from_rgb(110, 160, 255) } else { Color32::WHITE };
                painter.circle_stroke(at, r, Stroke::new(1.5, col.gamma_multiply(0.85)));
                painter.circle_stroke(at, r * 0.5, Stroke::new(1.0, col.gamma_multiply(0.35)));
                if hovered {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                }
            }
            // La barre des pinceaux, en bas de la vue
            let mut level_step = 0i32;
            let mut leave = false;
            let (level, top) = sc.multires.as_ref().map(|mr| (mr.current, mr.top())).unwrap_or((0, 0));
            let mut bui = ui.new_child(egui::UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)));
            egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).corner_radius(6.0).inner_margin(egui::Margin::symmetric(8, 5)).show(&mut bui, |ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for (b, icon, name, k) in crate::sculpt::BRUSHES {
                    let on = sc.brush == b;
                    let bt = egui::Button::new(egui::RichText::new(icon).size(15.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { Color32::TRANSPARENT }).min_size(egui::vec2(28.0, 26.0));
                    if ui.add(bt).on_hover_text(format!("{} ({k})", crate::i18n::t(name))).clicked() {
                        sc.brush = b;
                    }
                }
                ui.separator();
                ui.label(egui::RichText::new(crate::lucide::CIRCLE).size(13.0).color(p.muted)).on_hover_text(crate::i18n::t("rayon ([ et ])"));
                ui.add(egui::DragValue::new(&mut sc.radius).range(4.0..=600.0).speed(1.0).suffix(" px"));
                ui.label(egui::RichText::new(crate::lucide::GAUGE).size(13.0).color(p.muted)).on_hover_text(crate::i18n::t("force"));
                ui.add(egui::Slider::new(&mut sc.strength, 0.05..=2.0).show_value(false));
                let on = sc.sym;
                let sb = egui::Button::new(egui::RichText::new(crate::lucide::FLIP_HORIZONTAL_2).size(15.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { Color32::TRANSPARENT }).min_size(egui::vec2(28.0, 26.0));
                if ui.add(sb).on_hover_text(crate::i18n::t("symétrie sur X")).clicked() {
                    sc.sym = !sc.sym;
                }
                ui.separator();
                // Multirésolution : descendre, le niveau courant, monter (au sommet : un niveau de plus)
                let small = |icon: &str| egui::Button::new(egui::RichText::new(icon).size(15.0)).fill(Color32::TRANSPARENT).min_size(egui::vec2(24.0, 26.0));
                if ui.add_enabled(level > 0, small(crate::lucide::CHEVRON_DOWN)).on_hover_text(crate::i18n::t("niveau moins détaillé : les grandes formes, le détail fin est gardé")).clicked() {
                    level_step = -1;
                }
                ui.label(egui::RichText::new(format!("{} {level}/{top}", crate::lucide::LAYERS_2)).size(12.0).color(p.muted)).on_hover_text(crate::i18n::t("niveau de multirésolution (0 : la forme de base)"));
                let up_tip = if level < top { "niveau plus détaillé" } else { "ajouter un niveau : subdivise tout le maillage, pour plus de détails" };
                if ui.add(small(if level < top { crate::lucide::CHEVRON_UP } else { crate::lucide::PLUS })).on_hover_text(crate::i18n::t(up_tip)).clicked() {
                    level_step = 1;
                }
                ui.separator();
                let on = sc.dyntopo;
                let db = egui::Button::new(egui::RichText::new(crate::lucide::GRID_3X3).size(15.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { Color32::TRANSPARENT }).min_size(egui::vec2(28.0, 26.0));
                if ui.add(db).on_hover_text(crate::i18n::t("remaillage dynamique : la surface se subdivise sous le pinceau (remplace les niveaux de multirésolution)")).clicked() {
                    sc.dyntopo = !sc.dyntopo;
                }
                if sc.dyntopo {
                    let mut pct = sc.detail * 100.0;
                    if ui.add(egui::DragValue::new(&mut pct).range(5.0..=100.0).speed(0.5).suffix(" %")).on_hover_text(crate::i18n::t("taille du détail, en part du rayon du pinceau (plus petit : plus fin)")).changed() {
                        sc.detail = (pct / 100.0).clamp(0.05, 1.0);
                    }
                }
                if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::X).size(15.0)).fill(Color32::TRANSPARENT).min_size(egui::vec2(28.0, 26.0))).on_hover_text(crate::i18n::t("quitter la sculpture")).clicked() {
                    leave = true;
                }
            });
            if level_step != 0 {
                // Le maillage sculpté est rangé dans son niveau, puis on passe au niveau voisin (ou on en ajoute un)
                let mut mr = sc.multires.take().filter(|mr| mr.matches(&me.mesh)).unwrap_or_else(|| crate::multires::Multires::new(&me.mesh));
                mr.store(&me.mesh);
                let next = if level_step > 0 {
                    if mr.current < mr.top() { Some(mr.go(mr.current + 1)) } else { mr.add_level() }
                } else if mr.current > 0 {
                    Some(mr.go(mr.current - 1))
                } else {
                    None
                };
                if let Some(mesh) = next {
                    me.mesh = mesh;
                    me.sel = vec![false; me.mesh.pos.len()];
                    // L'historique d'un autre niveau n'a pas le même nombre de sommets : il repart de zéro
                    me.hist.clear();
                    me.redo.clear();
                    let _ = me.mesh.write_obj(&me.file);
                    let _ = mr.save(&me.file);
                    self.scene_dirty = true;
                    self.mesh_map = None;
                }
                sc.multires = Some(mr);
            }
            if leave {
                self.sculpt = None;
            }
            return;
        }
        if idle && hovered && secondary && !ui.input(|i| i.modifiers.shift) {
            me.menu = cur.map(|c| (c, 0));
        }
        // ---- Outil interactif en cours (biseau, incrustation, coupe en boucle) : la souris règle la
        // valeur, la molette les segments, on peut taper un nombre ; clic ou Entrée valide, clic droit
        // prend la valeur par défaut, Échap annule.
        if let Some(mut pts) = me.knife.take() {
            // Couteau : on pose des points sur les sommets ou les arêtes ; chaque segment coupe la face
            // qu'il traverse. Clic : poser ; Retour arrière : enlever le dernier ; Entrée, Espace ou clic
            // droit : couper ; Échap : annuler
            use crate::meshedit::KnifePoint;
            let at_screen = |me: &MeshEdit, kp: &KnifePoint| -> Option<Pos2> {
                match *kp {
                    KnifePoint::Vert(v) => project(to_world(me.mesh.pos[v as usize])),
                    KnifePoint::Edge(a, b, t) => {
                        let (pa, pb) = (me.mesh.pos[a as usize], me.mesh.pos[b as usize]);
                        project(to_world([0, 1, 2].map(|k| pa[k] + (pb[k] - pa[k]) * t)))
                    }
                }
            };
            let target = cur.and_then(|at| {
                let vert = (0..me.mesh.pos.len()).filter_map(|i| project_v(me, i).map(|s| (i, (s - at).length()))).filter(|x| x.1 < 6.0).min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((i, _)) = vert {
                    return Some(KnifePoint::Vert(i as u32));
                }
                me.mesh.edges().into_iter().filter_map(|(a, b)| {
                    let (sa, sb) = (project_v(me, a as usize)?, project_v(me, b as usize)?);
                    let ab = sb - sa;
                    let t = ((at - sa).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.02, 0.98);
                    let d = (sa + ab * t - at).length();
                    (d < 14.0).then_some((KnifePoint::Edge(a, b, t), d))
                }).min_by(|x, y| x.1.total_cmp(&y.1)).map(|x| x.0)
            });
            let (back, finish, esc) = ui.input(|i| (i.key_pressed(Key::Backspace), i.key_pressed(Key::Enter) || i.key_pressed(Key::Space), i.key_pressed(Key::Escape)));
            if back {
                pts.pop();
            }
            if pressed {
                if let Some(t) = target {
                    pts.push(t);
                }
                me.skip_click = true;
            }
            if esc {
                me.skip_click = true;
            } else if finish || secondary {
                me.skip_click = true;
                let mut m = me.mesh.clone();
                match m.knife(&pts) {
                    Some(sl) => {
                        snapshot(me);
                        me.mesh = m;
                        me.sel = sl;
                        dirty = true;
                    }
                    None => self.capture_msg = Some(crate::i18n::t("couteau : chaque segment doit traverser une face (d'un bord à l'autre)")),
                }
            } else {
                let painter = ui.painter_at(rect);
                let col = Color32::from_rgb(120, 230, 120);
                let scr: Vec<Pos2> = pts.iter().filter_map(|k| at_screen(me, k)).collect();
                for w in scr.windows(2) {
                    painter.line_segment([w[0], w[1]], Stroke::new(2.0, col));
                }
                for s in &scr {
                    painter.circle_filled(*s, 3.5, col);
                }
                let tgt = target.as_ref().and_then(|k| at_screen(me, k));
                if let (Some(last), Some(c)) = (scr.last(), tgt.or(cur)) {
                    painter.line_segment([*last, c], Stroke::new(1.2, col.gamma_multiply(0.7)));
                }
                if let Some(t) = tgt {
                    painter.circle_stroke(t, 6.0, Stroke::new(1.5, Color32::WHITE));
                }
                tool_hint(ui, rect, format!("{}   ·   {}   ·   {}   ·   {}   ·   {}", crate::i18n::t("couteau"), crate::i18n::t("clic : poser un point sur un sommet ou une arête"), crate::i18n::t("Retour arrière : enlever le dernier"), crate::i18n::t("Entrée ou clic droit : couper"), crate::i18n::t("Échap : annuler")));
                me.knife = Some(pts);
                ui.ctx().request_repaint();
            }
        } else
        if let Some(cuts) = me.loopcut {
            // Coupe en boucle, premier temps : l'anneau sous la souris se montre, la molette règle le nombre
            let wheel = ui.input(wheel_of);
            let mut cuts = cuts;
            if wheel > 0.0 {
                cuts = (cuts + 1).min(64);
            } else if wheel < 0.0 {
                cuts = cuts.saturating_sub(1).max(1);
            }
            for (k, n) in [(Key::Num1, 1), (Key::Num2, 2), (Key::Num3, 3), (Key::Num4, 4), (Key::Num5, 5), (Key::Num6, 6), (Key::Num7, 7), (Key::Num8, 8), (Key::Num9, 9)] {
                if ui.input(|i| i.key_pressed(k)) {
                    cuts = n;
                }
            }
            me.loopcut = Some(cuts);
            let scr: Vec<Option<Pos2>> = (0..me.mesh.pos.len()).map(|i| project(to_world(me.mesh.pos[i]))).collect();
            let near = cur.and_then(|at| me.mesh.edges().into_iter().filter_map(|(a, b)| {
                let (sa, sb) = (scr[a as usize]?, scr[b as usize]?);
                let ab = sb - sa;
                let t = ((at - sa).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                let d = (sa + ab * t - at).length();
                (d < 40.0).then_some(((a, b), d))
            }).min_by(|x, y| x.1.total_cmp(&y.1)).map(|x| x.0));
            let ring = near.map(|(a, b)| me.mesh.edge_ring(a, b).0);
            if let Some(ring) = &ring {
                // Aperçu : les futures coupes en jaune
                let painter = ui.painter_at(rect);
                let col = Color32::from_rgb(255, 214, 80);
                for j in 0..cuts {
                    let t = (j as f32 + 1.0) / (cuts as f32 + 1.0);
                    let pts: Vec<Option<Pos2>> = ring.iter().map(|&(a, b)| {
                        let (pa, pb) = (me.mesh.pos[a as usize], me.mesh.pos[b as usize]);
                        project(to_world([0, 1, 2].map(|k| pa[k] + (pb[k] - pa[k]) * t)))
                    }).collect();
                    for w in pts.windows(2) {
                        if let (Some(x), Some(y)) = (w[0], w[1]) {
                            painter.line_segment([x, y], Stroke::new(1.6, col));
                        }
                    }
                    if ring.len() > 2 {
                        if let (Some(Some(x)), Some(Some(y))) = (pts.first(), pts.last()) {
                            let (r0, rl) = (ring[0], ring[ring.len() - 1]);
                            // Anneau fermé : la dernière arête rejoint la première par un quadrilatère
                            if me.mesh.polys.iter().any(|p| p.len() == 4 && p.contains(&r0.0) && p.contains(&r0.1) && p.contains(&rl.0) && p.contains(&rl.1)) {
                                painter.line_segment([*x, *y], Stroke::new(1.6, col));
                            }
                        }
                    }
                }
            }
            let cancel = ui.input(|i| i.key_pressed(Key::Escape));
            if cancel {
                me.loopcut = None;
            } else if let (true, Some(ring), Some(at)) = (pressed || secondary, ring, cur) {
                me.loopcut = None;
                me.skip_click = true;
                snapshot(me);
                let mut op = ParamOp::new(2, me, at, at);
                op.segments = cuts;
                op.ring = ring;
                if let Some((m, s)) = op.run() {
                    me.mesh = m;
                    me.sel = s.clone();
                    op.result_sel = s;
                    dirty = true;
                    // Clic droit : la coupe reste au milieu ; clic gauche : on la fait glisser
                    if secondary {
                        me.last = Some(op);
                    } else {
                        me.op = Some(op);
                    }
                } else {
                    me.hist.pop();
                }
            }
            let msg = format!("{}   ·   {} {cuts}   ·   {}   ·   {}   ·   {}", crate::i18n::t("coupe en boucle"), crate::i18n::t("coupes :"), crate::i18n::t("molette ou 1 à 9 : nombre de coupes"), crate::i18n::t("clic : couper puis glisser"), crate::i18n::t("clic droit : couper au milieu"));
            tool_hint(ui, rect, msg);
            ui.ctx().request_repaint();
        } else if let Some(mut op) = me.op.take() {
            let (wheel, ctrl, back, enter, esc) = ui.input(|i| (wheel_of(i), i.modifiers.command, i.key_pressed(Key::Backspace), i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
            // I une seconde fois : face par face, ou toute la région
            if op.kind == 0 && ui.input(|i| i.key_pressed(Key::I) && !i.modifiers.any()) {
                op.alt = !op.alt;
                op.amount = op.default_amount();
            }
            if op.segments_label().is_some() {
                if wheel > 0.0 {
                    op.segments = (op.segments + 1).min(64);
                } else if wheel < 0.0 {
                    op.segments = op.segments.saturating_sub(1).max(1);
                }
            }
            // Saisie au clavier : la valeur exacte
            ui.input(|i| {
                for e in &i.events {
                    if let egui::Event::Text(t) = e {
                        for ch in t.chars() {
                            if ch.is_ascii_digit() || ch == '.' || ch == ',' || (ch == '-' && op.typed.is_empty()) {
                                op.typed.push(if ch == ',' { '.' } else { ch });
                            }
                        }
                    }
                }
            });
            if back {
                op.typed.pop();
            }
            if let Ok(v) = op.typed.parse::<f32>() {
                op.amount = op.from_shown(v);
            } else if let Some(at) = cur {
                let (dist, d0) = ((at - op.center).length(), (op.from - op.center).length().max(24.0));
                op.amount = match op.kind {
                    0 if op.alt => (1.0 - 0.75 * dist / d0).clamp(0.01, 0.95),
                    0 => (0.25 + 1.0 - dist / d0).clamp(0.0, 4.0),
                    1 | 3 => (0.25 * dist / d0).clamp(0.0, 1.0),
                    _ => {
                        let (a, b) = op.ring[0];
                        let (sa, sb) = (project(to_world(op.base.pos[a as usize])), project(to_world(op.base.pos[b as usize])));
                        match (sa, sb) {
                            (Some(sa), Some(sb)) if (sb - sa).length_sq() > 1.0 => (2.0 * (at - op.from).dot(sb - sa) / (sb - sa).length_sq()).clamp(-1.0, 1.0),
                            _ => 0.0,
                        }
                    }
                };
                if ctrl {
                    let step = if op.kind == 2 { 0.1 } else { 0.05 };
                    op.amount = (op.amount / step).round() * step;
                }
            }
            if esc {
                me.mesh = op.base;
                me.sel = op.base_sel;
                me.hist.pop();
                me.skip_click = true;
                dirty = true;
            } else {
                if secondary {
                    // Clic droit : la valeur par défaut, et c'est fait
                    op.amount = op.default_amount();
                    op.typed.clear();
                }
                if let Some((m, s)) = op.run() {
                    me.mesh = m;
                    me.sel = s.clone();
                    op.result_sel = s;
                    dirty = true;
                }
                let shown = op.shown();
                let msg = format!(
                    "{}   {} {}{}   ·   {}   ·   {}   ·   {}   ·   {}",
                    crate::i18n::t(op.name()),
                    crate::i18n::t(op.amount_label()),
                    if op.typed.is_empty() { format!("{shown:.3}") } else { format!("[{}]", op.typed) },
                    match op.segments_label() { Some(l) => format!("   {} : {}", crate::i18n::t(l), op.segments), None => String::new() },
                    crate::i18n::t(match op.kind { 0 if op.alt => "I : toute la région", 0 => "I : face par face", 1 => "molette : segments", _ => "Ctrl : arrondir" }),
                    crate::i18n::t("clic : valider"),
                    crate::i18n::t("clic droit : valeur par défaut"),
                    crate::i18n::t("Échap : annuler"),
                );
                tool_hint(ui, rect, msg);
                if pressed || enter || secondary {
                    me.skip_click = true;
                    me.last = Some(op);
                } else {
                    // Repère : du centre au pointeur, comme dans Blender
                    if let (Some(at), true) = (cur, op.kind != 2) {
                        let painter = ui.painter_at(rect);
                        let dash = egui::Shape::dashed_line(&[op.center, at], Stroke::new(1.0, p.text), 4.0, 4.0);
                        painter.extend(dash);
                    }
                    me.op = Some(op);
                }
            }
            ui.ctx().request_repaint();
        } else
        // ---- Transformation en cours
        if let Some((kind, axis, orig, from)) = me.modal.take() {
            let mut axis = axis;
            let wheel = ui.input(wheel_of);
            if me.prop_r <= 0.0 {
                // Un quart de la taille de l'objet modifié
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for v in &orig {
                    let w = to_world(*v);
                    for k in 0..3 {
                        lo[k] = lo[k].min(w[k]);
                        hi[k] = hi[k].max(w[k]);
                    }
                }
                let size = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt();
                me.prop_r = if size.is_finite() && size > 0.0 { size * 0.25 } else { longest * 0.1 };
            }
            if me.prop && wheel != 0.0 {
                me.prop_r *= if wheel > 0.0 { 1.12 } else { 1.0 / 1.12 };
            }
            if ui.input(|i| i.key_pressed(Key::O) && !i.modifiers.any()) {
                me.prop = !me.prop;
            }
            // G une seconde fois : les sommets glissent le long de leurs arêtes
            let mut kind = kind;
            if kind == 0 && ui.input(|i| i.key_pressed(Key::G) && !i.modifiers.any()) {
                kind = 3;
            }
            // Ctrl : par crans (distance ronde, 5°, dixièmes)
            let snap = ui.input(|i| i.modifiers.command);
            let step = 10f32.powf((longest * 0.02).log10().floor());
            // Aimantation sur les sommets (bouton ⌖, Maj+Tab) : le sommet choisi le plus proche se pose
            // sur le sommet visé, sans tenir compte de la souris entre les deux
            let snap_target: Option<V3> = if me.snap_v && kind == 0 {
                cur.and_then(|at| (0..me.mesh.pos.len()).filter(|&i| !me.sel.get(i).copied().unwrap_or(false)).filter_map(|i| project_v(me, i).map(|s| (i, (s - at).length()))).filter(|x| x.1 < 18.0).min_by(|a, b| a.1.total_cmp(&b.1))).map(|(i, _)| to_world(orig[i]))
            } else {
                None
            };
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
            let pivot = if pivot_mode == 1 { cursor3d } else { worlds.iter().fold([0.0f32; 3], |a, w| [a[0] + w[0] / n, a[1] + w[1] / n, a[2] + w[2] / n]) };
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
                let snap_mv: Option<V3> = snap_target.and_then(|t| {
                    let base = worlds.iter().min_by(|a, b| dot(sub(**a, t), sub(**a, t)).total_cmp(&dot(sub(**b, t), sub(**b, t))))?;
                    let mv = sub(t, *base);
                    Some(match axis {
                        Some(a) => {
                            let k = dot(mv, a);
                            [a[0] * k, a[1] * k, a[2] * k]
                        }
                        None => mv,
                    })
                });
                if let Some(t) = snap_target.and_then(|t| project(t)) {
                    ui.painter_at(rect).circle_stroke(t, 7.0, Stroke::new(2.0, Color32::from_rgb(255, 214, 80)));
                }
                let xf = |w: &V3| -> V3 {
                    if let (0, Some(m)) = (kind, snap_mv) {
                        return [w[0] + m[0], w[1] + m[1], w[2] + m[2]];
                    }
                    match kind {
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
                            let mv = if snap { mv.map(|x| (x / step).round() * step) } else { mv };
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
                            let th = if snap { (th.to_degrees() / 5.0).round() * 5f32.to_radians() } else { th };
                            // Rotation autour de l'axe (formule de Rodrigues)
                            let v = sub(*w, pivot);
                            let (cs, sn) = (th.cos(), th.sin());
                            let (kxv, kdv) = (cross(a, v), dot(a, v));
                            [0, 1, 2].map(|k| pivot[k] + v[k] * cs + kxv[k] * sn + a[k] * kdv * (1.0 - cs))
                        }
                        _ => {
                            let f = ((cur - center).length() / (from - center).length().max(8.0)).max(0.001);
                            let f = if snap { ((f * 10.0).round() / 10.0).max(0.1) } else { f };
                            let v = sub(*w, pivot);
                            match axis {
                                Some(a) => {
                                    let along = dot(v, a);
                                    [0, 1, 2].map(|k| pivot[k] + v[k] + a[k] * along * (f - 1.0))
                                }
                                None => [pivot[0] + v[0] * f, pivot[1] + v[1] * f, pivot[2] + v[2] * f],
                            }
                        }
                    }
                };
                if kind == 3 {
                    // Glisser : chaque sommet suit l'arête (vers un voisin non choisi) qui va le plus
                    // dans le sens de la souris, sans jamais la quitter
                    let nb = me.mesh.neighbors();
                    let dn = if d.length() > 1.0 { d.normalized() } else { egui::Vec2::ZERO };
                    for &i in &chosen {
                        me.mesh.pos[i] = orig[i];
                        let Some(si) = project(to_world(orig[i])) else { continue };
                        let best = nb[i].iter().filter(|&&j| !me.sel.get(j as usize).copied().unwrap_or(false)).filter_map(|&j| {
                            let sj = project(to_world(orig[j as usize]))?;
                            let e = sj - si;
                            (e.length() > 0.5).then(|| (j, e, e.normalized().dot(dn)))
                        }).max_by(|a, b| a.2.total_cmp(&b.2));
                        if let Some((j, e, al)) = best {
                            if al > 0.0 {
                                let t = (d.dot(e) / e.length_sq()).clamp(0.0, 1.0);
                                let t = if snap { (t * 10.0).round() / 10.0 } else { t };
                                let pj = orig[j as usize];
                                me.mesh.pos[i] = [0, 1, 2].map(|k| orig[i][k] + (pj[k] - orig[i][k]) * t);
                            }
                        }
                    }
                } else {
                    for (&i, w) in chosen.iter().zip(&worlds) {
                        me.mesh.pos[i] = to_local(xf(w));
                    }
                }
                // Miroir X : le reflet de chaque sommet bouge en même temps, de l'autre côté
                if me.mirror {
                    if me.mirror_m.0 != Some(from) || me.mirror_m.1.len() != orig.len() {
                        let mut tmp = me.mesh.clone();
                        tmp.pos = orig.clone();
                        me.mirror_m = (Some(from), tmp.mirror_map());
                    }
                    for i in 0..orig.len() {
                        if !me.sel.get(i).copied().unwrap_or(false) {
                            continue;
                        }
                        if let Some(j) = me.mirror_m.1[i] {
                            let j = j as usize;
                            if j == i {
                                // Sur le plan de symétrie : il y reste
                                me.mesh.pos[i][0] = 0.0;
                            } else if !me.sel.get(j).copied().unwrap_or(false) {
                                let q = me.mesh.pos[i];
                                me.mesh.pos[j] = [-q[0], q[1], q[2]];
                            }
                        }
                    }
                }
                // Édition proportionnelle : les sommets voisins suivent, de moins en moins avec la distance
                if me.prop && kind != 3 {
                    let r = me.prop_r.max(1e-6);
                    if me.prop_d.0 != Some(from) || me.prop_d.1.len() != orig.len() {
                        let mut dmin = vec![f32::MAX; orig.len()];
                        for (i, o) in orig.iter().enumerate() {
                            if me.sel.get(i).copied().unwrap_or(false) {
                                continue;
                            }
                            let wi = to_world(*o);
                            for w in &worlds {
                                let e = sub(wi, *w);
                                dmin[i] = dmin[i].min(dot(e, e));
                            }
                            dmin[i] = dmin[i].sqrt();
                        }
                        me.prop_d = (Some(from), dmin);
                    }
                    for i in 0..orig.len() {
                        let dd = me.prop_d.1[i];
                        if dd < r {
                            let t = dd / r;
                            let wgt = 1.0 - t * t * (3.0 - 2.0 * t);
                            let w = to_world(orig[i]);
                            let nw = xf(&w);
                            me.mesh.pos[i] = to_local([0, 1, 2].map(|k| w[k] + (nw[k] - w[k]) * wgt));
                        }
                    }
                    // Le cercle d'influence, autour du pivot
                    let right = cam.unrotate([1.0, 0.0, 0.0]);
                    if let Some(edge) = project([pivot[0] + right[0] * r, pivot[1] + right[1] * r, pivot[2] + right[2] * r]) {
                        ui.painter_at(rect).circle_stroke(center, (edge - center).length(), Stroke::new(1.0, p.text.gamma_multiply(0.7)));
                    }
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
            let what = [crate::i18n::t("déplacer"), crate::i18n::t("tourner"), crate::i18n::t("taille"), crate::i18n::t("glisser (G G)")][kind.min(3) as usize].clone();
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
            let prop_txt = format!("{}   ·   Ctrl : {}", if kind == 0 { format!("   ·   G : {}", crate::i18n::t("glisser")) } else { String::new() }, crate::i18n::t("par crans")) + &if me.prop { format!("   ·   {} {:.3} ({})", crate::i18n::t("proportionnel, rayon"), me.prop_r, crate::i18n::t("molette")) } else { format!("   ·   O : {}", crate::i18n::t("proportionnel")) };
            tool_hint(ui, rect, format!("{what}   ·   X Y Z : {}{prop_txt}   ·   {}   ·   {}", crate::i18n::t("imposer un axe"), crate::i18n::t("clic : valider"), crate::i18n::t("Échap : annuler")));
            ui.ctx().request_repaint();
        } else if hovered || me.boxing.is_some() || me.trigger.is_some() {
            let trig = me.trigger.take();
            let any = me.sel.iter().any(|s| *s);
            // ---- Outils
            let start = if key(Key::G) { Some(0u8) } else if key(Key::R) { Some(1) } else if key(Key::S) { Some(2) } else { None };
            if let (Some(kind), true, Some(at)) = (start, any, cur) {
                snapshot(me);
                me.modal = Some((kind, None, me.mesh.pos.clone(), at));
            }
            if (key(Key::E) || trig == Some("extruder")) && any {
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
            if (key(Key::X) || trig == Some("supprimer ou dissoudre…")) && any {
                me.menu = Some((cur.unwrap_or(rc), 1));
            }
            // Supprimer, dissoudre, pont, séparer : appliqués tout de suite, annulables
            let direct = match trig {
                Some(t @ ("supprimer les arêtes" | "supprimer les faces" | "dissoudre les sommets" | "dissoudre les arêtes" | "dissoudre les faces" | "pont entre boucles")) => Some(t),
                _ => None,
            };
            if let (Some(t), true) = (direct, any) {
                let mut m = me.mesh.clone();
                let r: Result<Vec<bool>, &'static str> = match t {
                    "supprimer les arêtes" => m.delete_edges(&me.sel).ok_or("aucune arête choisie (ses deux bouts)"),
                    "supprimer les faces" => m.delete_faces(&me.sel).ok_or("aucune face choisie (tous ses sommets)"),
                    "dissoudre les sommets" => m.dissolve_verts(&me.sel).ok_or("rien à dissoudre ici"),
                    "dissoudre les arêtes" => m.dissolve_edges(&me.sel).ok_or("rien à dissoudre ici"),
                    "dissoudre les faces" => m.dissolve_faces(&me.sel).ok_or("choisis au moins deux faces voisines"),
                    _ => m.bridge(&me.sel),
                };
                match r {
                    Ok(sl) => {
                        snapshot(me);
                        me.mesh = m;
                        me.sel = sl;
                        me.sel.resize(me.mesh.pos.len(), false);
                        dirty = true;
                    }
                    Err(e) => self.capture_msg = Some(crate::i18n::t(e)),
                }
            }
            // K : couteau
            if key(Key::K) || trig == Some("couteau") {
                me.knife = Some(vec![]);
            }
            // V : déchirer les sommets choisis (les faces du côté de la souris partent avec la copie) ;
            // Y : détacher les faces choisies. Puis on les déplace aussitôt (G)
            let rip = (key(Key::V) || trig == Some("déchirer")) && any;
            let det = (key(Key::Y) || trig == Some("détacher")) && any;
            if rip || det {
                let mut m = me.mesh.clone();
                let r = if det {
                    m.detach(&me.sel)
                } else {
                    let ch: Vec<usize> = (0..m.pos.len()).filter(|&i| me.sel[i]).collect();
                    let n = ch.len().max(1) as f32;
                    let sc = ch.iter().filter_map(|&i| project(to_world(m.pos[i]))).fold(egui::Vec2::ZERO, |a, s| a + s.to_vec2() / n).to_pos2();
                    let toward = cur.unwrap_or(rc) - sc;
                    let faces: Vec<usize> = (0..m.polys.len()).filter(|&f| {
                        let p = &m.polys[f];
                        if !p.iter().any(|&v| me.sel[v as usize]) {
                            return false;
                        }
                        let c = p.len() as f32;
                        let fc = p.iter().filter_map(|&v| project(to_world(m.pos[v as usize]))).fold(egui::Vec2::ZERO, |a, s| a + s.to_vec2() / c).to_pos2();
                        (fc - sc).dot(toward) > 0.0
                    }).collect();
                    m.rip_faces(&faces, Some(&me.sel))
                };
                match r {
                    Some(sl) => {
                        snapshot(me);
                        me.mesh = m;
                        me.sel = sl;
                        if let Some(at) = cur {
                            me.modal = Some((0, None, me.mesh.pos.clone(), at));
                        }
                        dirty = true;
                    }
                    None => self.capture_msg = Some(crate::i18n::t(if det { "détacher : choisis au moins une face" } else { "déchirer : rien à séparer ici (choisis des sommets entre des faces)" })),
                }
            }
            // P : les faces choisies deviennent un autre objet
            if (key(Key::P) || trig == Some("séparer la sélection")) && any {
                let mut m = me.mesh.clone();
                match m.split_off(&me.sel) {
                    Some(other) => {
                        snapshot(me);
                        me.mesh = m;
                        me.sel = vec![false; me.mesh.pos.len()];
                        me.split = Some(other);
                        dirty = true;
                    }
                    None => self.capture_msg = Some(crate::i18n::t("séparer : choisis au moins une face")),
                }
            }
            if (key(Key::Delete) || trig == Some("supprimer les sommets")) && any {
                snapshot(me);
                let sel = std::mem::take(&mut me.sel);
                me.mesh.delete(&sel);
                me.sel = vec![false; me.mesh.pos.len()];
                dirty = true;
            }
            if key(Key::F) || trig == Some("remplir") {
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
            if key(Key::A) || trig == Some("tout choisir") {
                me.sel.iter_mut().for_each(|s| *s = true);
            }
            if trig == Some("inverser la sélection") {
                me.sel.iter_mut().for_each(|s| *s = !*s);
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
            // Outils réglés à la souris (I, Ctrl+B, Ctrl+Maj+B) ou appliqués tout de suite
            // (subdiviser, lisser, fusionner par distance), aussi depuis le menu du clic droit
            let (cmd, shf) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
            let b_key = !typing && ui.input(|i| i.key_pressed(Key::B));
            let wanted: Option<u8> = if key(Key::I) || trig == Some("incruster") {
                Some(0)
            } else if (b_key && cmd && !shf) || trig == Some("biseau") {
                Some(1)
            } else if (b_key && cmd && shf) || trig == Some("biseau des sommets") {
                Some(3)
            } else if trig == Some("subdiviser") {
                Some(4)
            } else if trig == Some("lisser les sommets") {
                Some(5)
            } else if trig == Some("fusionner par distance") {
                Some(6)
            } else if trig == Some("remplir en grille") {
                Some(7)
            } else {
                None
            };
            if let (Some(kind), true) = (wanted, any) {
                let at = cur.unwrap_or(rc);
                let center = sel_center(me).unwrap_or(rc);
                let mut op = ParamOp::new(kind, me, at, center);
                match op.run() {
                    None => {
                        self.capture_msg = Some(crate::i18n::t(match kind {
                            0 | 4 => "il faut au moins une face dont tous les sommets sont choisis",
                            1 => "biseau : choisis au moins une arête entre deux faces",
                            6 => "fusionner par distance : rien d'assez proche",
                            7 => "remplir en grille : choisis une seule boucle de bord, avec un nombre pair de sommets",
                            _ => "rien à faire avec cette sélection",
                        }));
                    }
                    Some((m, sl)) => {
                        snapshot(me);
                        if op.interactive() {
                            me.op = Some(op);
                        } else {
                            me.mesh = m;
                            me.sel = sl.clone();
                            op.result_sel = sl;
                            me.last = Some(op);
                            dirty = true;
                        }
                    }
                }
            }
            // Ctrl+R : coupe en boucle
            if (!typing && ui.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::R))) || trig == Some("coupe en boucle") {
                me.loopcut = Some(1);
            }
            // M : fusionner au centre
            if (key(Key::M) || trig == Some("fusionner au centre")) && any {
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
            if key(Key::O) {
                me.prop = !me.prop;
            }
            if shift_tab {
                me.snap_v = !me.snap_v;
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
        // Menu du clic droit : tous les outils du mode édition, avec leur raccourci
        if let Some((at, menu_kind)) = me.menu {
            let mut close = false;
            let area = egui::Area::new(ui.scope_id().with("menu-edition")).order(egui::Order::Foreground).fixed_pos(at).constrain(true).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(230.0);
                    let tools: &[(&str, &[(&'static str, &str)])] = &[
                        ("SOMMETS", &[("fusionner au centre", "M"), ("fusionner par distance", ""), ("biseau des sommets", "Ctrl+Maj+B"), ("lisser les sommets", "")]),
                        ("ARÊTES", &[("biseau", "Ctrl+B"), ("coupe en boucle", "Ctrl+R"), ("pont entre boucles", "")]),
                        ("FACES", &[("extruder", "E"), ("incruster", "I"), ("subdiviser", ""), ("remplir", "F"), ("remplir en grille", "")]),
                        ("MAILLAGE", &[("couteau", "K"), ("déchirer", "V"), ("détacher", "Y"), ("séparer la sélection", "P"), ("supprimer ou dissoudre…", "X")]),
                        ("SÉLECTION", &[("tout choisir", "A"), ("inverser la sélection", "Ctrl+I")]),
                    ];
                    let del: &[(&str, &[(&'static str, &str)])] = &[
                        ("SUPPRIMER", &[("supprimer les sommets", "Suppr"), ("supprimer les arêtes", ""), ("supprimer les faces", "")]),
                        ("DISSOUDRE", &[("dissoudre les sommets", ""), ("dissoudre les arêtes", ""), ("dissoudre les faces", "")]),
                    ];
                    let groups = if menu_kind == 1 { del } else { tools };
                    for (gi, (title, items)) in groups.iter().enumerate() {
                        if gi > 0 {
                            ui.separator();
                        }
                        ui.label(crate::theme::section(&crate::i18n::t(title)));
                        for (name, sc) in items.iter() {
                            let r = ui.add(egui::Button::new(crate::i18n::t(name)).shortcut_text(crate::i18n::t_mid(sc)).frame(false).min_size(egui::vec2(220.0, 0.0)));
                            if r.clicked() {
                                me.trigger = Some(name);
                                close = true;
                            }
                        }
                    }
                });
            });
            if close || ui.input(|i| i.key_pressed(Key::Escape)) || (ui.input(|i| i.pointer.any_pressed()) && !area.response.contains_pointer() && Some(at) != cur) {
                me.menu = None;
                me.skip_click = true;
            }
            ui.ctx().request_repaint();
        }
        // Panneau de la dernière opération (comme F9 dans Blender) : on la rajuste après coup
        if me.op.is_none() && me.loopcut.is_none() {
            if me.last.as_ref().is_some_and(|l| l.result_sel != me.sel) {
                me.last = None;
            }
            if let Some(mut l) = me.last.take() {
                let mut changed = false;
                let mut close = false;
                let at = rect.left_bottom() + egui::vec2(12.0, -34.0);
                egui::Area::new(ui.scope_id().with("derniere-op")).order(egui::Order::Foreground).pivot(egui::Align2::LEFT_BOTTOM).fixed_pos(at).show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(220.0);
                        ui.horizontal(|ui| {
                            if ui.small_button("×").on_hover_text(crate::i18n::t("fermer")).clicked() {
                                close = true;
                            }
                            ui.label(egui::RichText::new(crate::i18n::t(l.name())).family(theme::bold()));
                        });
                        egui::Grid::new("derniere-op-grille").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                            if !l.amount_label().is_empty() {
                                ui.label(crate::i18n::t(l.amount_label().trim_end_matches(" :")));
                                let mut v = l.shown();
                                let (lo, hi, speed) = l.range();
                                if ui.add(egui::DragValue::new(&mut v).range(lo..=hi).speed(speed).max_decimals(4)).changed() {
                                    l.amount = l.from_shown(v);
                                    changed = true;
                                }
                                ui.end_row();
                            }
                            if l.kind == 0 {
                                ui.label("");
                                if ui.checkbox(&mut l.alt, crate::i18n::t("face par face")).changed() {
                                    l.amount = l.default_amount();
                                    changed = true;
                                }
                                ui.end_row();
                            }
                            if let Some(lab) = l.segments_label() {
                                ui.label(crate::i18n::t(lab));
                                if ui.add(egui::DragValue::new(&mut l.segments).range(1..=64).speed(0.1)).changed() {
                                    changed = true;
                                }
                                ui.end_row();
                            }
                        });
                    });
                });
                if changed {
                    if let Some((m, s)) = l.run() {
                        me.mesh = m;
                        me.sel = s.clone();
                        l.result_sel = s;
                        dirty = true;
                    }
                }
                if !close {
                    me.last = Some(l);
                }
            }
        }
        if dirty {
            // Un geste en cours (G, R, S, glisser) : affichage sur place, écriture à la fin du geste
            let gesture = me.modal.is_some() && me.mesh.pos.len() == me.hist.last().map(|h| h.0.pos.len()).unwrap_or(0);
            if gesture {
                self.mesh_moved = true;
            } else {
                let _ = me.mesh.write_obj(&me.file);
                self.scene_dirty = true;
                self.mesh_map = None;
            }
            ui.ctx().request_repaint();
        }
        let split = me.split.take().map(|m| (me.obj, m));
        if let Some((obj, m)) = split {
            self.add_split_object(obj, m);
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
                    if ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("annuler"))).clicked() {
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
        open = crate::dock::os_window(ctx, "render-3d", &crate::i18n::t("rendu"), egui::vec2(720.0, 480.0), |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::ProgressBar::new(job.spp as f32 / job.target.max(1) as f32).desired_width(260.0).fill(p.accent).text(format!("{} / {} · {:.0} s", job.spp, job.target, job.elapsed)));
                if !done && ui.add(egui::Button::new(egui::RichText::new(crate::lucide::SQUARE).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("arrêter")).clicked() {
                    job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                    job.target = job.spp;
                }
                if ui.add_enabled(job.image.is_some(), egui::Button::new(format!("{}  {}", crate::lucide::SAVE, crate::i18n::t("enregistrer…")))).clicked() {
                    save = true;
                }
                // Le rendu part vers un autre outil (Retouche, Montage, Motion…)
                if let Some(t) = crate::sendto::button(ui, job.image.is_some(), "3d") {
                    if let Some(img) = job.image.as_ref().and_then(|px| image::RgbaImage::from_raw(job.w as u32, job.h as u32, px.clone())) {
                        let name = format!("{} - {}", self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), crate::i18n::t_mid("rendu"));
                        self.capture_msg = Some(match crate::sendto::stage(&crate::nodes::vault_root(), &name, &img) {
                            Ok(rel) => {
                                crate::sendto::push(t, rel);
                                crate::i18n::t("Envoyé.")
                            }
                            Err(e) => e,
                        });
                    }
                }
                if ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("fermer"))).clicked() {
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
                // PNG (l'image finale) ou EXR multicouche (passes en 32 bits pour le compositing)
                if let Some(path) = rfd::FileDialog::new().add_filter("PNG", &["png"]).add_filter(crate::i18n::t("EXR multicouche (passes)"), &["exr"]).set_file_name("rendu.png").save_file() {
                    let path = if path.extension().is_none() { path.with_extension("png") } else { path };
                    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exr")) {
                        let r = match job.acc.lock() {
                            Ok(g) => match g.as_ref() {
                                Some(acc) => pathtrace::write_passes_exr(acc, &job.prep, &job.cam, job.denoise, &path),
                                None => Err(crate::i18n::t("attends la fin du rendu pour les passes EXR")),
                            },
                            Err(e) => Err(e.to_string()),
                        };
                        self.capture_msg = Some(match r {
                            Ok(()) => format!("{} {}", crate::i18n::t("passes EXR enregistrées :"), path.to_string_lossy()),
                            Err(e) => e,
                        });
                    } else if let Some(buf) = image::RgbaImage::from_raw(job.w as u32, job.h as u32, img.clone()) {
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

    /// Le pinceau de peinture sur le modèle est-il actif ?
    pub fn painting(&self) -> bool {
        self.paint3d.is_some()
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
        // Les clés posées cette image vont dans le calque d'animation actif
        if let Some(sc) = &self.scene {
            sc.set_anim_context();
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
                            None => self.scene_frame = Some(if self.tight_frame { tight_bounds(m.min, m.max) } else { scene_bounds(m.min, m.max) }),
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
            self.scene_frame = Some(if self.tight_frame { tight_bounds(m.min, m.max) } else { scene_bounds(m.min, m.max) });
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
        let n_before = scene.objects.len();
        // Capture de mouvement : un squelette neuf, ou le mouvement reciblé sur le squelette de l'os choisi
        if self.hdr_add == Some("bvh") {
            self.hdr_add = None;
            let mut d = rfd::FileDialog::new().add_filter("BVH", &["bvh"]);
            if !self.root.is_empty() {
                d = d.set_directory(&self.root);
            }
            if let Some(f) = d.pick_file() {
                match std::fs::read_to_string(&f).map_err(|e| e.to_string()).and_then(|t| crate::mocap::parse(&t)) {
                    Ok(b) => {
                        let target = self.scene_sel.filter(|&k| crate::rig::is_bone(scene, k)).map(|k| crate::rig::root_of(scene, k));
                        match target {
                            Some(root) => {
                                let n = crate::mocap::retarget(scene, &b, root);
                                crate::notify(if n == 0 {
                                    crate::i18n::t("Aucun os du squelette ne porte le nom d'une articulation du BVH (Hips, Spine, LeftArm…).")
                                } else {
                                    format!("{n} {} · {} {}", crate::i18n::t("os animés"), b.frames.len(), crate::i18n::t("images"))
                                });
                            }
                            None => {
                                self.scene_sel = Some(crate::mocap::import(scene, &b, ""));
                                crate::notify(format!("{} {} · {} {}", b.joints.len(), crate::i18n::t("os importés"), b.frames.len(), crate::i18n::t("images")));
                            }
                        }
                        changed = true;
                    }
                    Err(e) => crate::notify(format!("BVH : {e}")),
                }
            }
        }
        // Suivi de caméra : choisir la vidéo, puis suivre son avancement ; à la fin, la caméra est posée dans la scène
        if std::mem::take(&mut self.want_matchmove) && self.mm_job.is_none() {
            if let Some(f) = rfd::FileDialog::new().add_filter(crate::i18n::t("vidéo"), &["mp4", "mov", "mkv", "webm", "avi", "m4v"]).pick_file() {
                self.mm_dialog = Some((f, true, 60.0));
            }
        }
        if let Some((file, auto, fov)) = self.mm_dialog.as_mut() {
            let (mut go, mut open) = (false, true);
            egui::Window::new(crate::i18n::t("Suivi de caméra")).collapsible(false).resizable(false).open(&mut open).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ui.ctx(), |ui| {
                ui.label(egui::RichText::new(format!("{}  {}", crate::lucide::VIDEO, file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())).strong());
                ui.label(egui::RichText::new(crate::i18n::t("La caméra doit se déplacer dans le plan (un simple panoramique sur trépied ne donne pas de relief). Les premières secondes sont suivies.")).size(11.0).color(p.muted));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.checkbox(auto, crate::i18n::t("estimer le champ de l'objectif"));
                    ui.add_enabled(!*auto, egui::DragValue::new(fov).speed(0.2).range(5.0..=150.0).suffix("°")).on_hover_text(crate::i18n::t("Champ horizontal de l'objectif, s'il est connu : le résultat est plus juste"));
                });
                ui.add_space(4.0);
                if ui.button(format!("{}  {}", crate::lucide::PLAY, crate::i18n::t("suivre"))).clicked() {
                    go = true;
                }
            });
            if go {
                let ctx = ui.ctx().clone();
                self.mm_job = Some(crate::matchmove::start(file.clone(), (!*auto).then_some(*fov), move || ctx.request_repaint()));
            }
            if go || !open {
                self.mm_dialog = None;
            }
        }
        if let Some(job) = &self.mm_job {
            let done = job.result.lock().ok().and_then(|mut g| g.take());
            match done {
                Some(Ok(s)) => {
                    let dir = path.parent().unwrap_or(std::path::Path::new(""));
                    let file = s.video.strip_prefix(dir).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| s.video.to_string_lossy().to_string());
                    let cam = crate::matchmove::to_scene(scene, &s.sol, &s.tracks, s.size, &file, s.fps, s.from, 40);
                    scene.camera = scene.objects[cam].name.clone();
                    self.scene_sel = Some(cam);
                    self.scene_multi.clear();
                    self.look_through = true;
                    self.anim_frame = 0.0;
                    changed = true;
                    let solved = s.sol.poses.iter().flatten().count();
                    crate::notify(format!("{} : {} {} / {}, {} {:.2} px", crate::i18n::t("caméra suivie"), crate::i18n::t("images"), solved, s.sol.poses.len(), crate::i18n::t("erreur moyenne"), s.sol.error));
                    self.capture_msg = None;
                    self.mm_job = None;
                }
                Some(Err(e)) => {
                    self.capture_msg = Some(e);
                    self.mm_job = None;
                }
                None => {
                    let (v, what) = job.progress.lock().map(|g| *g).unwrap_or((0.0, ""));
                    self.capture_msg = Some(format!("{} {:.0} %", crate::i18n::t(what), v * 100.0));
                }
            }
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
                "bone" => crate::rig::add_bone(scene, self.scene_sel),
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
                    for (label, base, role) in [("lumière", "soleil", "light"), ("caméra (point de vue)", "caméra", "camera"), ("groupe vide", "groupe", ""), ("os", "os", "bone")] {
                        if ui.button(crate::i18n::t(label)).clicked() {
                            let k = if role == "bone" { crate::rig::add_bone(scene, self.scene_sel) } else { scene.add_role(base, role) };
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
        // Les nouveaux objets apparaissent au curseur 3D (une lumière garde sa hauteur au-dessus)
        for k in n_before..scene.objects.len() {
            let o = &mut scene.objects[k];
            for a in 0..3 {
                o.pos[a] += self.cursor3d[a];
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
            if act == Some("join") || ui.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::J)) {
                self.join_request = true;
            }
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
            // (en mode édition, I incruste : pas de clé d'animation)
            if self.mesh.is_none() && ui.input(|i| i.key_pressed(Key::I) && !i.modifiers.any()) {
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
        // Les contraintes « viser » suivent leurs cibles même quand on déplace les objets à la main
        if scene.objects.iter().any(|o| !o.copy_loc.is_empty() || !o.copy_rot.is_empty() || o.limit.is_some()) && scene.apply_copies() {
            self.scene_dirty = true;
        }
        if scene.objects.iter().any(|o| !o.aim.is_empty()) && scene.apply_aims() {
            self.scene_dirty = true;
        }
        // La cinématique inverse aussi : déplacer la cible plie la chaîne
        if scene.objects.iter().any(|o| !o.ik.is_empty()) && crate::rig::solve_ik(scene) {
            self.scene_dirty = true;
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
        // Squelette : 1 lier (pose de repos prise), 2 définir le repos, 3 revenir au repos, 4 ajouter un os enfant
        let mut rig_op: Option<(u8, usize)> = None;
        // Formes : 1 garder l'édition en cours comme forme, 2 ajouter depuis un fichier
        let mut morph_op: Option<(u8, usize)> = None;
        let editing_obj = self.mesh.as_ref().map(|m| m.obj);
        let frame_now = self.anim_frame.round() as i32;
        let bone_roots: Vec<String> = (0..scene.objects.len()).filter(|&k| crate::rig::is_bone(scene, k) && crate::rig::root_of(scene, k) == k).map(|k| scene.objects[k].name.clone()).collect();
        for f in std::mem::take(&mut self.scene_add) {
            let image = crate::scene::kind(&crate::scene::Object::new(String::new(), f.to_string_lossy().to_string())) == crate::scene::Kind::Image;
            if (is_model(&f.to_string_lossy()) || image) && !crate::scene::is_scene(&f) {
                scene.add(&path, &f);
                self.scene_sel = Some(scene.objects.len() - 1);
                changed = true;
            }
        }
        // part 0 : la liste de la scène (panneau de gauche) ; part 1 : les propriétés (panneau de droite)
        let mut sub_body = |ui: &mut egui::Ui, part: u8| {
            ui.spacing_mut().slider_width = 80.0;
            if part == 0 {
            // En-tête de 34 px comme la maquette : « Scène », le nombre d'objets, puis recadrer et ajouter
            let (hr, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), egui::Sense::hover());
            ui.painter().hline(hr.x_range(), hr.bottom() - 0.5, Stroke::new(1.0, p.line));
            let mut hui = ui.new_child(egui::UiBuilder::new().max_rect(hr.shrink2(egui::vec2(8.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
            hui.add_space(4.0);
            hui.label(egui::RichText::new(crate::i18n::t("Scène")).size(13.0).family(theme::bold()).color(p.text));
            hui.add_space(4.0);
            let n = scene.objects.len();
            hui.label(egui::RichText::new(format!("{n} {}", crate::i18n::t_mid(if n == 1 { "objet" } else { "objets" }))).size(11.0).color(p.muted));
            hui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::containers::menu::MenuButton::new(egui::RichText::new(crate::lucide::PLUS).size(14.0)).ui(ui, |ui| {
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
                        if ui.button(format!("{}  {}", crate::lucide::TYPE, crate::i18n::t("texte 3D…"))).clicked() {
                            self.text3d = Some((String::new(), 1.0, 0.25, 1));
                            ui.close();
                        }
                        if ui.add_enabled(self.mm_job.is_none(), egui::Button::new(format!("{}  {}", crate::lucide::VIDEO, crate::i18n::t("caméra suivie depuis une vidéo…")))).on_hover_text(crate::i18n::t("Suivi de caméra : retrouve le mouvement de la caméra d'une vidéo et des repères de la scène filmée, avec la vidéo en fond")).clicked() {
                            self.want_matchmove = true;
                            ui.close();
                        }
                        if ui.button(format!("{}  {}", crate::lucide::FOLDER_OPEN, crate::i18n::t("choisir un fichier…"))).clicked() {
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
                        if ui.button(format!("{}  {}", crate::lucide::BONE, crate::i18n::t("os"))).on_hover_text(crate::i18n::t("Un os de squelette ; ajouté au bout de l'os choisi")).clicked() {
                            self.scene_sel = Some(crate::rig::add_bone(scene, self.scene_sel));
                            changed = true;
                            ui.close();
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
                    if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::SCAN).size(14.0).color(p.muted)).frame(false)).on_hover_text(crate::i18n::t("recadrer la vue sur la scène")).clicked() {
                        reframe = true;
                    }
            });
            // Fenêtre du texte 3D
            if let Some((text, height, depth, bevel)) = self.text3d.as_mut() {
                let mut open = true;
                let mut make = false;
                open = crate::dock::os_window(&ui.ctx().clone(), "text3d-win", &crate::i18n::t("Texte 3D"), egui::vec2(340.0, 260.0), |ui| {
                    ui.add(egui::TextEdit::multiline(text).hint_text(crate::i18n::t("le texte (Entrée : nouvelle ligne)")).desired_rows(2).desired_width(260.0));
                    ui.add(egui::Slider::new(height, 0.1..=10.0).logarithmic(true).text(crate::i18n::t("hauteur d'une ligne")));
                    ui.add(egui::Slider::new(depth, 0.01..=2.0).logarithmic(true).text(crate::i18n::t("épaisseur")));
                    ui.add(egui::Slider::new(bevel, 0..=6).text(crate::i18n::t("arrondi des bords")));
                    make = ui.add_enabled(!text.trim().is_empty(), egui::Button::new(format!("{}  {}", crate::lucide::PLUS, crate::i18n::t("créer le texte 3D")))).on_hover_text(crate::i18n::t("Extrudé en volume, enregistré en .obj à côté de la scène")).clicked();
                });
                if make {
                    let (t, hgt, dep, bev) = (text.clone(), *height, *depth, *bevel);
                    match crate::text3d::mesh(&t, hgt, dep, bev) {
                        Some(o) => {
                            let stem: String = t.lines().next().unwrap_or("texte").chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-').take(30).collect();
                            let dir = path.parent().map(|d| d.to_path_buf()).unwrap_or_default();
                            let mut f = dir.join(format!("texte - {}.obj", stem.trim()));
                            let mut n = 2;
                            while f.exists() {
                                f = dir.join(format!("texte - {} {n}.obj", stem.trim()));
                                n += 1;
                            }
                            match crate::text3d::write_obj(&o, &f) {
                                Ok(()) => {
                                    scene.add(&path, &f);
                                    self.scene_sel = scene.objects.len().checked_sub(1);
                                    changed = true;
                                    open = false;
                                }
                                Err(e) => self.capture_msg = Some(e),
                            }
                        }
                        None => self.capture_msg = Some(crate::i18n::t("Texte 3D impossible")),
                    }
                }
                if !open {
                    self.text3d = None;
                }
            }
            ui.add_space(6.0);
            if scene.objects.is_empty() {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(crate::i18n::t("Scène vide. Glisse des modèles sur la fenêtre, ou ajoute-en avec le bouton.")).color(p.muted).size(11.0));
            }
            // (index, action) : 0 supprimer, 1 dupliquer
            let mut action: Option<(usize, u8)> = None;
            // Les références prennent le bas du panneau, comme la maquette : la liste garde le reste
            if self.scene_refs.is_none() {
                let mut v: Vec<std::path::PathBuf> = vec![];
                if let Some(dir) = path.parent() {
                    if let Ok(rd) = std::fs::read_dir(dir) {
                        v = rd.flatten().map(|e| e.path()).filter(|f| matches!(f.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref(), Some("png" | "jpg" | "jpeg" | "webp"))).collect();
                        v.sort();
                        v.truncate(6);
                    }
                }
                self.scene_refs = Some(v);
            }
            let refs = self.scene_refs.clone().unwrap_or_default();
            let refs_h = if refs.is_empty() { 0.0 } else { 34.0 + ((refs.len() + 2) / 3) as f32 * ((ui.available_width() - 36.0) / 3.0 + 6.0) };
            egui::ScrollArea::vertical().id_salt("scene-objects").auto_shrink([false, false]).max_height((ui.available_height() - refs_h).max(60.0)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, depth) in scene.tree() {
                    let o = &mut scene.objects[i];
                    ui.horizontal(|ui| {
                        // Rangée de 24 px : icône Lucide, nom en Inter 12,5, œil à droite
                        let icon = match crate::scene::kind(o) {
                            crate::scene::Kind::Model => crate::lucide::BOX,
                            crate::scene::Kind::Image => crate::lucide::IMAGE,
                            crate::scene::Kind::Empty => crate::lucide::SQUARE,
                            crate::scene::Kind::Light => crate::lucide::SUN,
                            crate::scene::Kind::Camera => crate::lucide::CAMERA,
                            crate::scene::Kind::Bone => crate::lucide::BONE,
                        };
                        let on = self.scene_sel == Some(i);
                        let also = self.scene_multi.contains(&i);
                        let bad = self.scene_errors.get(i).and_then(|e| e.as_ref());
                        let (rect, r) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::click());
                        let eye_r = egui::Rect::from_center_size(egui::pos2(rect.right() - 17.0, rect.center().y), egui::vec2(22.0, 22.0));
                        let eye = ui.interact(eye_r, r.id.with("eye"), egui::Sense::click());
                        let fill = if on || also { p.selection } else if r.hovered() { p.line.linear_multiply(0.45) } else { Color32::TRANSPARENT };
                        ui.painter().rect_filled(rect, 0.0, fill);
                        let x = rect.left() + 8.0 + depth as f32 * 14.0;
                        ui.painter().text(egui::pos2(x + 7.0, rect.center().y), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(14.0), p.muted);
                        let col = if bad.is_some() { p.missing } else if o.visible { p.text } else { p.muted };
                        ui.painter().text(egui::pos2(x + 20.0, rect.center().y), egui::Align2::LEFT_CENTER, &o.name, egui::FontId::proportional(12.5), col);
                        if on && self.mesh.is_some() {
                            ui.painter().text(egui::pos2(eye_r.left() - 4.0, rect.center().y), egui::Align2::RIGHT_CENTER, crate::i18n::t("édition"), egui::FontId::proportional(11.0), p.muted);
                        }
                        let eye_col = if eye.hovered() { p.text } else { p.muted };
                        ui.painter().text(eye_r.center(), egui::Align2::CENTER_CENTER, if o.visible { crate::lucide::EYE } else { crate::lucide::EYE_OFF }, egui::FontId::proportional(13.0), eye_col);
                        if eye.on_hover_text(crate::i18n::t("afficher / cacher")).clicked() {
                            o.visible = !o.visible;
                            changed = true;
                        }
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
                            if ui.button(format!("{}  {}", crate::lucide::COPY, crate::i18n::t("dupliquer"))).clicked() {
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
            if !refs.is_empty() {
                let top = ui.max_rect().bottom() - refs_h;
                ui.painter().hline(ui.max_rect().x_range(), top, Stroke::new(1.0, p.line));
                let mut rui = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_max(egui::pos2(ui.max_rect().left() + 12.0, top + 10.0), ui.max_rect().right_bottom() - egui::vec2(12.0, 0.0))));
                rui.label(egui::RichText::new(crate::i18n::t("Références")).size(11.0).color(p.muted));
                let cell = (rui.available_width() - 12.0) / 3.0;
                egui::Grid::new("scene-refs").spacing(egui::vec2(6.0, 6.0)).show(&mut rui, |ui| {
                    for (k, f) in refs.iter().enumerate() {
                        let img = egui::Image::new(format!("file://{}", f.display())).fit_to_exact_size(egui::vec2(cell, cell)).corner_radius(10).maintain_aspect_ratio(false).sense(egui::Sense::click());
                        let r = ui.add(img).on_hover_text(format!("{}\n{}", f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), crate::i18n::t("cliquer pour la poser dans la scène")));
                        if r.clicked() {
                            scene.add(&path, f);
                            self.scene_sel = Some(scene.objects.len() - 1);
                            changed = true;
                        }
                        if k % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
            }
            }
            if part == 1 {
            // Parents possibles pour l'objet choisi : tout sauf lui-même et ses propres enfants
            let parents: Vec<String> = match self.scene_sel {
                Some(s) if s < scene.objects.len() => (0..scene.objects.len()).filter(|&k| k != s && !scene.descends(k, s)).map(|k| scene.objects[k].name.clone()).collect(),
                _ => vec![],
            };
            // ---- Propriétés, comme la maquette : un en-tête de 34 px, une rangée d'onglets, puis le contenu
            let sel_kind = self.scene_sel.and_then(|i| scene.objects.get(i)).map(crate::scene::kind);
            let has_data = matches!(sel_kind, Some(crate::scene::Kind::Light | crate::scene::Kind::Camera | crate::scene::Kind::Bone));
            let mut tab = self.prop_tab;
            if tab == PropTab::Data && !has_data {
                tab = PropTab::Object;
            }
            let mut new_tab = tab;
            {
                let (hr, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), egui::Sense::hover());
                ui.painter().hline(hr.x_range(), hr.bottom() - 0.5, Stroke::new(1.0, p.line));
                let chip = egui::Rect::from_min_size(hr.left_center() + egui::vec2(6.0, -12.0), egui::vec2(84.0, 24.0));
                ui.painter().rect(chip, 2.0, p.raised, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
                ui.painter().text(chip.center(), egui::Align2::CENTER_CENTER, crate::i18n::t("Propriétés"), egui::FontId::proportional(12.5), p.text);
                // Onglets : Objet, Modificateurs, Matériau, (Lumière ou Caméra), Monde, Scène
                let (tr, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 37.0), egui::Sense::hover());
                ui.painter().hline(tr.x_range(), tr.bottom() - 0.5, Stroke::new(1.0, p.line));
                let mut tui = ui.new_child(egui::UiBuilder::new().max_rect(tr.shrink(6.0)).layout(egui::Layout::left_to_right(egui::Align::Center)));
                tui.spacing_mut().item_spacing.x = 1.0;
                tui.spacing_mut().button_padding = egui::vec2(6.0, 0.0);
                let data_name = match sel_kind {
                    Some(crate::scene::Kind::Camera) => "Caméra",
                    Some(crate::scene::Kind::Bone) => "Os",
                    _ => "Lumière",
                };
                for (t, name) in [(PropTab::Object, "Objet"), (PropTab::Modifiers, "Modificateurs"), (PropTab::Material, "Matériau"), (PropTab::Data, data_name), (PropTab::World, "Monde"), (PropTab::Scene, "Scène")] {
                    if t == PropTab::Data && !has_data {
                        continue;
                    }
                    let on = tab == t;
                    let text = egui::RichText::new(crate::i18n::t(name)).size(12.0).color(if on { p.text } else { p.muted });
                    let b = egui::Button::new(text).fill(if on { p.selection } else { Color32::TRANSPARENT }).stroke(Stroke::NONE).corner_radius(2.0).min_size(egui::vec2(0.0, 24.0));
                    if tui.add(b).clicked() {
                        new_tab = t;
                    }
                }
            }
            let _ = PROP_TABS;
            egui::Frame::new().inner_margin(12).show(ui, |ui| {
                ui.vertical(|ui| {
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
                                // Calques d'animation : la base, puis des calques qui s'ajoutent par-dessus (poids, muet)
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(crate::i18n::t("CALQUES D'ANIMATION")).size(10.5).color(p.muted));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.add(egui::Button::new(crate::lucide::PLUS).frame(false)).on_hover_text(crate::i18n::t("Nouveau calque d'animation : les clés posées dedans s'ajoutent à l'animation de base sans la toucher")).clicked() {
                                            let mut n = scene.layers.len() + 1;
                                            while scene.layers.iter().any(|l| l.name == format!("{} {n}", crate::i18n::t("calque"))) {
                                                n += 1;
                                            }
                                            let name = format!("{} {n}", crate::i18n::t("calque"));
                                            scene.layers.push(crate::scene::AnimLayer { name: name.clone(), weight: 1.0, mute: false });
                                            scene.active_layer = name;
                                            changed = true;
                                        }
                                    });
                                });
                                let base_on = scene.active_layer.is_empty();
                                if ui.selectable_label(base_on, format!("{}  {}", crate::lucide::LAYERS, crate::i18n::t("base"))).on_hover_text(crate::i18n::t("Les clés vont dans l'animation de base")).clicked() {
                                    scene.active_layer.clear();
                                    changed = true;
                                }
                                let mut remove = None;
                                for (i, l) in scene.layers.iter_mut().enumerate() {
                                    let on = scene.active_layer == l.name;
                                    ui.horizontal(|ui| {
                                        let eye = if l.mute { crate::lucide::EYE_OFF } else { crate::lucide::EYE };
                                        if ui.add(egui::Button::new(egui::RichText::new(eye).color(if l.mute { p.muted } else { p.text })).frame(false)).on_hover_text(crate::i18n::t("muet : le calque ne compte plus")).clicked() {
                                            l.mute = !l.mute;
                                            changed = true;
                                        }
                                        if ui.selectable_label(on, &l.name).on_hover_text(crate::i18n::t("Les nouvelles clés (I) vont dans ce calque")).clicked() {
                                            scene.active_layer = l.name.clone();
                                            changed = true;
                                        }
                                        ui.spacing_mut().slider_width = 70.0;
                                        changed |= ui.add(egui::Slider::new(&mut l.weight, 0.0..=1.0).show_value(false)).on_hover_text(crate::i18n::t("poids du calque")).changed();
                                        if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::X).color(p.muted)).frame(false)).on_hover_text(crate::i18n::t("supprimer le calque et ses clés")).clicked() {
                                            remove = Some(i);
                                        }
                                    });
                                }
                                if let Some(i) = remove {
                                    let name = scene.layers.remove(i).name;
                                    for o in &mut scene.objects {
                                        o.layer_keys.retain(|lk| lk.layer != name);
                                    }
                                    if scene.active_layer == name {
                                        scene.active_layer.clear();
                                    }
                                    changed = true;
                                }
                                if changed {
                                    let f = self.anim_frame;
                                    scene.apply_frame(f);
                                }
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
                                        // Titre comme la maquette : icône, nom en 15 px (modifiable), genre à droite
                                        ui.spacing_mut().item_spacing.y = 6.0;
                                        let before = o.name.clone();
                                        let kind = crate::scene::kind(o);
                                        ui.horizontal(|ui| {
                                            let (icon, what) = match kind {
                                                crate::scene::Kind::Model => (crate::lucide::BOX, "maillage"),
                                                crate::scene::Kind::Image => (crate::lucide::IMAGE, "image"),
                                                crate::scene::Kind::Empty => (crate::lucide::SQUARE, "groupe vide"),
                                                crate::scene::Kind::Light => (crate::lucide::SUN, "lumière"),
                                                crate::scene::Kind::Camera => (crate::lucide::CAMERA, "caméra"),
                                                crate::scene::Kind::Bone => (crate::lucide::BONE, "os"),
                                            };
                                            ui.label(egui::RichText::new(icon).size(16.0).color(crate::theme::ws_color("3d").unwrap_or(p.accent)));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                ui.label(egui::RichText::new(crate::i18n::t(what)).size(11.0).color(p.muted));
                                                let te = egui::TextEdit::singleline(&mut o.name).frame(egui::Frame::NONE).font(egui::FontId::new(15.0, theme::bold())).desired_width(ui.available_width());
                                                if ui.add(te).changed() {
                                                    renamed = Some((before.clone(), o.name.clone()));
                                                    changed = true;
                                                }
                                            });
                                        });
                                        ui.add_space(8.0);
                                        ui.label(egui::RichText::new(crate::i18n::t("Transformer")).size(11.0).color(p.muted));
                                        let mut row = |ui: &mut egui::Ui, label: &str, v: &mut [f32; 3], speed: f64, reset: [f32; 3]| {
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 4.0;
                                                let (lr, lresp) = ui.allocate_exact_size(egui::vec2(62.0, 22.0), egui::Sense::click());
                                                ui.painter().text(lr.left_center(), egui::Align2::LEFT_CENTER, crate::i18n::t(label), egui::FontId::proportional(12.0), if lresp.hovered() { p.text } else { p.muted });
                                                if lresp.on_hover_text(crate::i18n::t("cliquer pour remettre à zéro")).clicked() {
                                                    *v = reset;
                                                    changed = true;
                                                }
                                                let signed = reset != [1.0; 3];
                                                let w = ((ui.available_width() - 8.0) / 3.0).max(30.0);
                                                for (shown, axis) in ["X", "Y", "Z"].iter().enumerate() {
                                                    let (k, sign) = crate::gizmo::SHOWN[shown];
                                                    let sign = if signed { sign } else { 1.0 };
                                                    let (_, uf) = if label == "Position" { crate::settings::unit() } else { ("", 1.0) };
                                                    let mut val = v[k] * sign * uf + 0.0;
                                                    let dv = egui::DragValue::new(&mut val).speed(speed * uf as f64).max_decimals(if label == "Rotation" { 1 } else { 2 }).suffix(if label == "Rotation" { "°" } else { "" });
                                                    if ui.add_sized([w, 22.0], dv).on_hover_text(*axis).changed() {
                                                        v[k] = val * sign / uf;
                                                        changed = true;
                                                    }
                                                }
                                            });
                                        };
                                        row(ui, "Position", &mut o.pos, 0.01, [0.0; 3]);
                                        row(ui, "Rotation", &mut o.rot, 0.5, [0.0; 3]);
                                        row(ui, "Taille", &mut o.scale, 0.005, [1.0; 3]);
                                        ui.add_space(8.0);
                                        // Modificateurs en résumé, comme la maquette : une carte par modificateur, l'œil pour l'allumer
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(crate::i18n::t("Modificateurs")).size(11.0).color(p.muted));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                if ui.add(egui::Button::new(egui::RichText::new(format!("{} {}", crate::lucide::PLUS, crate::i18n::t("Ajouter"))).size(12.0)).frame(false)).clicked() {
                                                    new_tab = PropTab::Modifiers;
                                                }
                                            });
                                        });
                                        for m in o.mods.iter_mut() {
                                            let (cr, cresp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::click());
                                            ui.painter().rect(cr, 2.0, p.raised, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
                                            ui.painter().text(cr.left_center() + egui::vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, crate::i18n::t(m.kind.name()), egui::FontId::proportional(12.5), if m.on { p.text } else { p.muted });
                                            let er = egui::Rect::from_center_size(egui::pos2(cr.right() - 14.0, cr.center().y), egui::vec2(20.0, 20.0));
                                            let eye = ui.interact(er, cresp.id.with("eye"), egui::Sense::click());
                                            ui.painter().text(er.center(), egui::Align2::CENTER_CENTER, if m.on { crate::lucide::EYE } else { crate::lucide::EYE_OFF }, egui::FontId::proportional(13.0), if eye.hovered() { p.text } else { p.muted });
                                            if eye.clicked() {
                                                m.on = !m.on;
                                                changed = true;
                                            } else if cresp.clicked() {
                                                new_tab = PropTab::Modifiers;
                                            }
                                        }
                                        if o.mods.is_empty() {
                                            ui.label(egui::RichText::new(crate::i18n::t("Aucun modificateur")).size(12.0).color(p.muted));
                                        }
                                        ui.add_space(8.0);
                                        // Matériau en résumé : une pastille, le nom ; un clic ouvre l'onglet Matériau
                                        ui.label(egui::RichText::new(crate::i18n::t("Matériau")).size(11.0).color(p.muted));
                                        let mname = if o.material.is_empty() { crate::i18n::t("aucun") } else { crate::formats::strip(o.material.rsplit('/').next().unwrap_or(&o.material)).to_string() };
                                        let (mr, mresp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::click());
                                        let c = egui::pos2(mr.left() + 14.0, mr.center().y);
                                        let base = o.color.map(|c| Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(Color32::from_rgb(0x8a, 0x7a, 0x82));
                                        ui.painter().circle_filled(c, 14.0, base.gamma_multiply(0.55));
                                        ui.painter().circle_filled(c + egui::vec2(-3.0, -3.0), 9.0, base);
                                        ui.painter().circle_filled(c + egui::vec2(-5.0, -5.0), 3.5, Color32::from_white_alpha(120));
                                        ui.painter().text(egui::pos2(mr.left() + 38.0, mr.center().y), egui::Align2::LEFT_CENTER, &mname, egui::FontId::proportional(12.5), if mresp.hovered() { p.text } else { p.text.gamma_multiply(0.9) });
                                        if mresp.clicked() {
                                            new_tab = PropTab::Material;
                                        }
                                        ui.add_space(8.0);
                                        ui.label(egui::RichText::new(crate::i18n::t("Objet")).size(11.0).color(p.muted));
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
                                        // Contrainte « viser » : l'objet se tourne toujours vers sa cible (une caméra qui suit un personnage)
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(format!("{}  {}", crate::lucide::CROSSHAIR, crate::i18n::t("viser"))).size(11.0).color(if o.aim.is_empty() { p.muted } else { p.accent }));
                                            let cur = if o.aim.is_empty() { crate::i18n::t("rien") } else { o.aim.clone() };
                                            egui::ComboBox::from_id_salt("scene-object-aim").selected_text(egui::RichText::new(cur).size(11.0)).width(130.0).show_ui(ui, |ui| {
                                                if ui.selectable_label(o.aim.is_empty(), crate::i18n::t("rien")).clicked() {
                                                    o.aim.clear();
                                                    changed = true;
                                                }
                                                for name in &parents {
                                                    if ui.selectable_label(&o.aim == name, name).clicked() {
                                                        o.aim = name.clone();
                                                        changed = true;
                                                    }
                                                }
                                            });
                                        })
                                        .response
                                        .on_hover_text(crate::i18n::t("Contrainte « viser » : l'objet se tourne toujours vers la cible, même animée. Sa rotation X et Y ne se règle plus à la main."));
                                        // Contraintes « copier » : position et rotation d'un autre objet
                                        for (label, icon, which, tip) in [("copier position", crate::lucide::MOVE, 0, "L'objet se place toujours sur l'origine de la cible, même animée"), ("copier rotation", crate::lucide::ROTATE_CW, 1, "L'objet prend toujours la rotation de la cible (objets sans parent)")] {
                                            ui.horizontal(|ui| {
                                                let field = if which == 0 { &mut o.copy_loc } else { &mut o.copy_rot };
                                                ui.label(egui::RichText::new(format!("{icon}  {}", crate::i18n::t(label))).size(11.0).color(if field.is_empty() { p.muted } else { p.accent }));
                                                let cur = if field.is_empty() { crate::i18n::t("rien") } else { field.clone() };
                                                egui::ComboBox::from_id_salt(("scene-object-copy", which)).selected_text(egui::RichText::new(cur).size(11.0)).width(110.0).show_ui(ui, |ui| {
                                                    if ui.selectable_label(field.is_empty(), crate::i18n::t("rien")).clicked() {
                                                        field.clear();
                                                        changed = true;
                                                    }
                                                    for name in &parents {
                                                        if ui.selectable_label(field == name, name).clicked() {
                                                            *field = name.clone();
                                                            changed = true;
                                                        }
                                                    }
                                                });
                                            })
                                            .response
                                            .on_hover_text(crate::i18n::t(tip));
                                        }
                                        // Limites de position : l'objet ne sort pas de cette boîte
                                        let mut lim = o.limit.is_some();
                                        if ui.checkbox(&mut lim, egui::RichText::new(crate::i18n::t("limiter la position")).size(11.0)).changed() {
                                            o.limit = lim.then(|| [[o.pos[0] - 1.0, o.pos[1] - 1.0, o.pos[2] - 1.0], [o.pos[0] + 1.0, o.pos[1] + 1.0, o.pos[2] + 1.0]]);
                                            changed = true;
                                        }
                                        if let Some(l) = &mut o.limit {
                                            for (row, name) in [(0usize, "min"), (1, "max")] {
                                                ui.horizontal(|ui| {
                                                    ui.label(egui::RichText::new(crate::i18n::t(name)).size(11.0).color(p.muted));
                                                    for (shown, _) in ["X", "Y", "Z"].iter().enumerate() {
                                                        let (k, sign) = crate::gizmo::SHOWN[shown];
                                                        let mut v = l[row][k] * sign;
                                                        if ui.add(egui::DragValue::new(&mut v).speed(0.02).max_decimals(2)).changed() {
                                                            l[row][k] = v * sign;
                                                            changed = true;
                                                        }
                                                    }
                                                });
                                            }
                                        }

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
                                        if crate::scene::kind(o) == crate::scene::Kind::Model && !bone_roots.is_empty() {
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new(format!("{} {}", crate::lucide::BONE, crate::i18n::t("squelette"))).size(11.0).color(p.muted));
                                                let cur = if o.skin.is_empty() { crate::i18n::t("aucun") } else { o.skin.clone() };
                                                egui::ComboBox::from_id_salt("scene-object-skin").selected_text(egui::RichText::new(cur).size(11.0)).width(110.0).show_ui(ui, |ui| {
                                                    if ui.selectable_label(o.skin.is_empty(), crate::i18n::t("aucun")).clicked() {
                                                        o.skin.clear();
                                                        changed = true;
                                                    }
                                                    for n in &bone_roots {
                                                        if ui.selectable_label(&o.skin == n, n).on_hover_text(crate::i18n::t("Lier avec des poids automatiques : la pose actuelle du squelette devient sa pose de repos")).clicked() {
                                                            o.skin = n.clone();
                                                            rig_op = self.scene_sel.map(|k| (1, k));
                                                        }
                                                    }
                                                });
                                            });
                                            if !o.skin.is_empty() {
                                                ui.horizontal(|ui| {
                                                    ui.label(egui::RichText::new(crate::i18n::t("portée des os")).size(11.0).color(p.muted));
                                                    changed |= ui.add(egui::Slider::new(&mut o.reach, 0.2..=4.0).logarithmic(true)).on_hover_text(crate::i18n::t("Plus grand : chaque os entraîne la peau plus loin, les plis sont plus doux")).changed();
                                                });
                                            }
                                        }
                                        // Formes (blendshapes)
                                        if crate::scene::kind(o) == crate::scene::Kind::Model {
                                            ui.add_space(4.0);
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new(crate::i18n::t("Formes")).size(11.0).color(p.muted)).on_hover_text(crate::i18n::t("Blendshapes : d'autres versions du même maillage (sourire, clignement…), mélangées par un curseur qui s'anime"));
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::FOLDER_OPEN).size(13.0)).frame(false)).on_hover_text(crate::i18n::t("ajouter une forme depuis un fichier OBJ (même nombre de sommets)")).clicked() {
                                                        morph_op = self.scene_sel.map(|k| (2, k));
                                                    }
                                                    if editing_obj == self.scene_sel && ui.add(egui::Button::new(egui::RichText::new(crate::lucide::PLUS).size(13.0)).frame(false)).on_hover_text(crate::i18n::t("garder l'édition en cours comme forme : le maillage revient à son état de départ, la forme se règle avec son curseur")).clicked() {
                                                        morph_op = self.scene_sel.map(|k| (1, k));
                                                    }
                                                });
                                            });
                                            let mut gone: Option<usize> = None;
                                            for (mi, m) in o.morphs.iter_mut().enumerate() {
                                                ui.horizontal(|ui| {
                                                    ui.add(egui::TextEdit::singleline(&mut m.name).desired_width(70.0).font(egui::FontId::proportional(11.0)));
                                                    changed |= ui.add(egui::Slider::new(&mut m.value, 0.0..=1.0).show_value(false).clamping(egui::SliderClamping::Never)).changed();
                                                    let keyed = m.keys.iter().any(|k| k.0 == frame_now);
                                                    let kb = egui::Button::new(egui::RichText::new(if keyed { crate::lucide::DIAMOND_PLUS } else { crate::lucide::DIAMOND }).size(12.0).color(if keyed { p.warning } else { p.muted })).frame(false);
                                                    if ui.add(kb).on_hover_text(crate::i18n::t("clé à l'image actuelle (clic droit : retirer toutes les clés)")).clicked() {
                                                        m.set_key(frame_now);
                                                        changed = true;
                                                    } else if ui.ctx().input(|i| i.pointer.secondary_clicked()) && ui.rect_contains_pointer(ui.min_rect()) && !m.keys.is_empty() {
                                                        m.keys.clear();
                                                        changed = true;
                                                    }
                                                    if ui.add(egui::Button::new(egui::RichText::new(crate::lucide::X).size(11.0).color(p.muted)).frame(false)).on_hover_text(crate::i18n::t("retirer la forme (le fichier reste)")).clicked() {
                                                        gone = Some(mi);
                                                    }
                                                });
                                            }
                                            if let Some(g) = gone {
                                                o.morphs.remove(g);
                                                changed = true;
                                            }
                                        }

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
                                                    if ui.button(format!("{}  {}", crate::lucide::EYE, crate::i18n::t("regarder par ici"))).on_hover_text(crate::i18n::t("Aligne la vue sur cette caméra")).clicked() {
                                                        look_through = self.scene_sel;
                                                    }
                                                    if ui.button(format!("{}  {}", crate::lucide::CAMERA, crate::i18n::t("poser sur la vue"))).on_hover_text(crate::i18n::t("Enregistre la vue actuelle dans cette caméra")).clicked() {
                                                        set_camera = self.scene_sel;
                                                    }
                                                });
                                                ui.horizontal(|ui| {
                                                    ui.label(egui::RichText::new(crate::i18n::t("champ")).size(11.0).color(p.muted));
                                                    let mut auto = o.fov <= 0.0;
                                                    let mut fov = if auto { fov_of_focal(BASE_FOCAL) } else { o.fov };
                                                    if ui.add_enabled(!auto, egui::DragValue::new(&mut fov).speed(0.2).range(2.0..=160.0).suffix("°")).on_hover_text(crate::i18n::t("Champ de vision sur le petit côté de l'image")).changed() {
                                                        o.fov = fov;
                                                        changed = true;
                                                    }
                                                    if ui.checkbox(&mut auto, crate::i18n::t("celui de la vue")).changed() {
                                                        o.fov = if auto { 0.0 } else { fov };
                                                        changed = true;
                                                    }
                                                });
                                            }
                                            crate::scene::Kind::Bone => {
                                                ui.horizontal(|ui| {
                                                    ui.label(egui::RichText::new(crate::i18n::t("longueur")).size(11.0).color(p.muted));
                                                    changed |= ui.add(egui::DragValue::new(&mut o.length).speed(0.01).range(0.001..=f32::MAX)).changed();
                                                });
                                                ui.add_space(4.0);
                                                ui.label(crate::theme::section(&crate::i18n::t("CINÉMATIQUE INVERSE")));
                                                ui.horizontal(|ui| {
                                                    ui.label(egui::RichText::new(crate::i18n::t("cible")).size(11.0).color(p.muted));
                                                    let cur = if o.ik.is_empty() { crate::i18n::t("aucune") } else { o.ik.clone() };
                                                    egui::ComboBox::from_id_salt("scene-bone-ik").selected_text(egui::RichText::new(cur).size(11.0)).width(120.0).show_ui(ui, |ui| {
                                                        if ui.selectable_label(o.ik.is_empty(), crate::i18n::t("aucune")).clicked() {
                                                            o.ik.clear();
                                                            changed = true;
                                                        }
                                                        for n in &parents {
                                                            if ui.selectable_label(&o.ik == n, n).clicked() {
                                                                o.ik = n.clone();
                                                                changed = true;
                                                            }
                                                        }
                                                    });
                                                })
                                                .response
                                                .on_hover_text(crate::i18n::t("Le bout de l'os cherche à toucher cet objet : déplacez la cible, la chaîne se plie toute seule"));
                                                if !o.ik.is_empty() {
                                                    ui.horizontal(|ui| {
                                                        ui.label(egui::RichText::new(crate::i18n::t("os dans la chaîne")).size(11.0).color(p.muted));
                                                        changed |= ui.add(egui::DragValue::new(&mut o.ik_chain).range(1..=16)).changed();
                                                    });
                                                }
                                                ui.add_space(4.0);
                                                ui.label(crate::theme::section(&crate::i18n::t("SQUELETTE")));
                                                ui.horizontal(|ui| {
                                                    if ui.button(crate::lucide::PLUS).on_hover_text(crate::i18n::t("Ajouter un os au bout de celui-ci")).clicked() {
                                                        rig_op = self.scene_sel.map(|k| (4, k));
                                                    }
                                                    if ui.button(crate::lucide::BOOKMARK).on_hover_text(crate::i18n::t("Enregistrer la pose actuelle comme pose de repos du squelette")).clicked() {
                                                        rig_op = self.scene_sel.map(|k| (2, k));
                                                    }
                                                    if ui.button(crate::lucide::ROTATE_CCW).on_hover_text(crate::i18n::t("Revenir à la pose de repos")).clicked() {
                                                        rig_op = self.scene_sel.map(|k| (3, k));
                                                    }
                                                });
                                                if o.rest.is_none() {
                                                    ui.label(egui::RichText::new(crate::i18n::t("Pas encore de pose de repos : liez un modèle à ce squelette (onglet objet du modèle).")).size(10.0).color(p.muted));
                                                }
                                            }
                                            _ => {}
                                        }

                                        if !matches!(crate::scene::kind(o), crate::scene::Kind::Light | crate::scene::Kind::Camera | crate::scene::Kind::Bone) {
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
            }
        };
        if let Some(sub_r) = crate::layout::offer("3d-outliner", "outliner et propriétés") {
            crate::layout::draw_in(ui, sub_r, p.panel, 8.0, |ui| {
                sub_body(ui, 0);
                sub_body(ui, 1);
            });
        } else {
            // Comme la maquette : la scène à gauche (240 px), les propriétés à droite (290 px)
            let frame = egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line));
            egui::Panel::left("scene-outliner").resizable(true).default_size(240.0).min_size(180.0).max_size((ui.available_width() * 0.35).max(200.0)).frame(frame).show(ui, |ui| sub_body(ui, 0));
            egui::Panel::right("scene-inspector").resizable(true).default_size(290.0).min_size(240.0).max_size((ui.available_width() * 0.45).max(260.0)).frame(frame).show(ui, |ui| sub_body(ui, 1));
        }
        let seek_before = seek;
        // Timeline (une bande) ou éditeur de courbes (un panneau qu'on agrandit)
        let tl_frame = egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line));
        let tl_panel = if self.anim_graph && !self.graph_window { egui::Panel::bottom("scene-graph").resizable(true).default_size(300.0).min_size(160.0).max_size((ui.available_height() * 0.7).max(180.0)) } else { egui::Panel::bottom("scene-timeline").resizable(true).default_size(120.0).min_size(70.0).max_size(320.0) };
        let sub_body = |ui: &mut egui::Ui| {
            // Les clés montrées et éditées dans la timeline : celles du calque d'animation actif, sinon la base
            let tl_layer: String = if scene.layers.iter().any(|l| l.name == scene.active_layer) { scene.active_layer.clone() } else { String::new() };
            // Comme la maquette : une barre de 30 px (lecture, temps, image), puis la règle et les pistes
            let (hr, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::hover());
            ui.painter().hline(hr.x_range(), hr.bottom() - 0.5, Stroke::new(1.0, p.line));
            let mut hui = ui.new_child(egui::UiBuilder::new().max_rect(hr.shrink2(egui::vec2(8.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
            {
                let v = &mut hui.style_mut().visuals.widgets;
                v.inactive.weak_bg_fill = Color32::TRANSPARENT;
                v.inactive.bg_stroke = Stroke::NONE;
                v.hovered.weak_bg_fill = p.raised;
                v.hovered.bg_stroke = Stroke::NONE;
            }
            hui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
            hui.horizontal_centered(|ui| {
                if ui.button(egui::RichText::new(if self.anim_playing { crate::lucide::PAUSE } else { crate::lucide::PLAY }).size(14.0)).on_hover_text(crate::i18n::t("lire / arrêter (Espace)")).clicked() {
                    self.anim_playing = !self.anim_playing;
                }
                if ui.button(egui::RichText::new(crate::lucide::UNDO_2).size(13.0).color(p.muted)).on_hover_text(crate::i18n::t("revenir au début")).clicked() {
                    seek = Some(scene.start as f32);
                }
                let secs = self.anim_frame / scene.fps.max(1.0);
                let tc = format!("{}:{:02}:{:02}", (secs / 60.0) as i32, secs as i32 % 60, (self.anim_frame.round() as i32).rem_euclid(scene.fps.max(1.0).round() as i32));
                ui.label(egui::RichText::new(tc).monospace().size(12.0).color(p.text));
                let mut fr = self.anim_frame.round() as i32;
                {
                    let v = &mut ui.style_mut().visuals.widgets;
                    v.inactive.bg_fill = Color32::TRANSPARENT;
                }
                if ui.add(egui::DragValue::new(&mut fr).prefix(format!("{} ", crate::i18n::t("image"))).suffix(format!(" / {}", scene.end))).changed() {
                    seek = Some(fr as f32);
                }
                ui.add_space(8.0);
                let sel = self.scene_sel.filter(|&k| k < scene.objects.len());
                let here = self.anim_frame.round() as i32;
                let on_key = sel.is_some_and(|k| scene.objects[k].keys_in(&tl_layer).iter().any(|x| x.frame == here));
                if ui.add_enabled(sel.is_some(), egui::Button::new(egui::RichText::new(if on_key { "◆" } else { "◇" }).color(if on_key { p.accent } else { p.text }))).on_hover_text(crate::i18n::t("Poser une clé pour l'objet choisi à cette image (K)")).clicked() {
                    if let Some(k) = sel {
                        scene.objects[k].set_key(here);
                        changed = true;
                    }
                }
                if ui.add_enabled(on_key, egui::Button::new("✕")).on_hover_text(crate::i18n::t("Retirer la clé de cette image")).clicked() {
                    if let Some(ks) = sel.and_then(|k| scene.objects[k].keys_in_mut(&tl_layer)) {
                        ks.retain(|x| x.frame != here);
                        changed = true;
                    }
                }
                if let Some(key) = sel.and_then(|k| scene.objects[k].keys_in_mut(&tl_layer)).and_then(|ks| ks.iter_mut().find(|x| x.frame == here)) {
                    // Passage vers la clé suivante : le même menu que partout dans l'app
                    let cur = key.ease();
                    let label = if key.mode() == "bezier" { crate::i18n::t("Bézier") } else { crate::i18n::t(cur.map(|e| e.name()).unwrap_or("Bézier")) };
                    let r = ui.menu_button(label, |ui| {
                        if let Some(choice) = crate::keys::menu(ui, if key.mode() == "bezier" { None } else { cur }, true) {
                            key.interp = choice.map(|e| e.word()).unwrap_or("bezier").to_string();
                            changed = true;
                        }
                    });
                    r.response.on_hover_text(crate::i18n::t("Passage vers la clé suivante"));
                }
                let auto_col = if self.anim_auto { p.text } else { p.muted };
                let rec_col = if self.anim_auto { Color32::from_rgb(230, 80, 80) } else { auto_col };
                ui.toggle_value(&mut self.anim_auto, egui::RichText::new(crate::lucide::CIRCLE_DOT).size(14.0).color(rec_col)).on_hover_text(crate::i18n::t("Chaque déplacement pose une clé à l'image courante"));
                ui.toggle_value(&mut self.onion, egui::RichText::new(crate::lucide::LAYERS).size(14.0)).on_hover_text(crate::i18n::t("Pelure d'oignon : l'objet choisi aux clés d'avant (rouge) et d'après (vert)"));
                ui.toggle_value(&mut self.anim_tools, egui::RichText::new(crate::lucide::WAND_SPARKLES).size(14.0)).on_hover_text(crate::i18n::t("Outils d'animation : tweener, décaler les clés, poses, miroir…"));
                if ui.toggle_value(&mut self.anim_graph, egui::RichText::new(crate::lucide::SPLINE).size(14.0)).on_hover_text(crate::i18n::t("Éditeur de courbes : clés et poignées de Bézier par canal (Maj+F6)")).changed() {
                }
                if self.anim_graph {
                    ui.toggle_value(&mut self.graph_window, egui::RichText::new(crate::lucide::APP_WINDOW).size(14.0)).on_hover_text(crate::i18n::t("Éditeur de courbes dans sa propre fenêtre, qu'on peut sortir de l'application (sur un autre écran)"));
                }
                // Montage d'animation : ranger les clés de la sélection dans une action, poser une action en bande
                egui::containers::menu::MenuButton::new(egui::RichText::new(crate::lucide::CLAPPERBOARD).size(14.0)).ui(ui, |ui| {
                    ui.set_min_width(280.0);
                    ui.label(crate::theme::section(&crate::i18n::t("MONTAGE D'ANIMATION")));
                    // La sélection et tout ce qui en descend (un squelette entier part d'un coup)
                    let picked: Vec<usize> = self.scene_multi.iter().cloned().chain(self.scene_sel).filter(|&k| k < scene.objects.len()).collect();
                    let targets: Vec<usize> = (0..scene.objects.len()).filter(|&k| picked.iter().any(|&a| a == k || scene.descends(k, a))).collect();
                    let can = targets.iter().any(|&k| scene.objects[k].keys.len() >= 2);
                    if ui.add_enabled(can, egui::Button::new(format!("{}  {}", crate::lucide::SQUARE_PLUS, crate::i18n::t("faire une action des clés de la sélection")))).on_hover_text(crate::i18n::t("Les clés de la sélection (et de ses enfants) deviennent une action réutilisable, posée en bande au même endroit : on peut ensuite la répéter, l'étirer, la fondre ou la reposer ailleurs")).clicked() {
                        let base = sel.map(|k| scene.objects[k].name.clone()).unwrap_or_else(|| "action".into());
                        if let Some(i) = crate::nla::make_action(scene, &targets, &base) {
                            self.strip_sel = Some(i);
                            changed = true;
                        }
                        ui.close();
                    }
                    if !scene.actions.is_empty() {
                        ui.separator();
                        ui.label(egui::RichText::new(crate::i18n::t("poser une bande à l'image courante")).size(11.0).color(p.muted));
                        let mut add = None;
                        for a in &scene.actions {
                            if ui.button(format!("{}  {}", crate::lucide::FILM, a.name)).clicked() {
                                add = Some(a.name.clone());
                                ui.close();
                            }
                        }
                        if let Some(name) = add {
                            let len = scene.actions.iter().find(|a| a.name == name).map(|a| a.len()).unwrap_or(1.0);
                            let row = crate::nla::free_row(scene, here as f32, here as f32 + len);
                            scene.strips.push(crate::nla::Strip::new(&name, here, row));
                            self.strip_sel = Some(scene.strips.len() - 1);
                            changed = true;
                        }
                    }
                })
                .0
                .on_hover_text(crate::i18n::t("Montage d'animation : actions réutilisables posées en bandes (cycles, fondus, empilement)"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    changed |= ui.add(egui::DragValue::new(&mut scene.fps).range(1.0..=1000.0).suffix(" i/s")).changed();
                    changed |= ui.add(egui::DragValue::new(&mut scene.end).prefix(format!("{} ", crate::i18n::t("fin")))).changed();
                    changed |= ui.add(egui::DragValue::new(&mut scene.start).prefix(format!("{} ", crate::i18n::t("début")))).changed();
                });
            });
            if self.anim_tools {
                let (r2, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::hover());
                ui.painter().rect_filled(r2, 0.0, p.bg.gamma_multiply(0.6));
                ui.painter().hline(r2.x_range(), r2.bottom() - 0.5, Stroke::new(1.0, p.line));
                let mut tu = ui.new_child(egui::UiBuilder::new().max_rect(r2.shrink2(egui::vec2(8.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
                tu.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                tu.spacing_mut().item_spacing.x = 4.0;
                let here = self.anim_frame.round() as i32;
                // Les objets visés : la sélection (l'actif compris)
                let mut targets: Vec<usize> = self.scene_multi.iter().cloned().chain(self.scene_sel).filter(|&k| k < scene.objects.len()).collect();
                targets.sort();
                targets.dedup();
                let any = !targets.is_empty();
                let sep = |ui: &mut egui::Ui| {
                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);
                };
                use crate::animtools as at;
                let mut moved_to: Option<i32> = None;
                tu.add_enabled_ui(any, |ui| {
                  egui::ScrollArea::horizontal().id_salt("anim-tools").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                  ui.horizontal_centered(|ui| {
                    // Tweener : la pose entre la clé d'avant et celle d'après, avec dépassement possible
                    ui.spacing_mut().slider_width = 90.0;
                    let r = ui.add(egui::Slider::new(&mut self.tween_w, -0.5..=1.5).show_value(false)).on_hover_text(format!("{} · {:.0} %", crate::i18n::t("Entre la clé d'avant (0 %) et celle d'après (100 %) ; au-delà : dépassement"), self.tween_w * 100.0));
                    let mut apply = r.changed();
                    for (lab, w) in [("0", 0.0), ("¼", 0.25), ("½", 0.5), ("¾", 0.75), ("1", 1.0)] {
                        if ui.small_button(lab).clicked() {
                            self.tween_w = w;
                            apply = true;
                        }
                    }
                    if apply {
                        let mut n = 0;
                        for &k in &targets {
                            n += at::tween(&mut scene.objects[k], here, self.tween_w) as usize;
                        }
                        if n > 0 {
                            changed = true;
                        } else {
                            crate::notify(crate::i18n::t("Le tweener a besoin d'une clé avant et d'une clé après l'image courante"));
                        }
                    }
                    sep(ui);
                    // Décaler les clés de l'image courante (Alt+← / Alt+→), la tête de lecture suit
                    for (icon, d, tip) in [(crate::lucide::CHEVRON_LEFT, -1, "Avancer d'une image les clés de l'image courante (Alt+←)"), (crate::lucide::CHEVRON_RIGHT, 1, "Reculer d'une image les clés de l'image courante (Alt+→)")] {
                        let alt_key = !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.modifiers.alt && !i.modifiers.shift && i.key_pressed(if d < 0 { Key::ArrowLeft } else { Key::ArrowRight }));
                        if ui.small_button(icon).on_hover_text(crate::i18n::t(tip)).clicked() || alt_key {
                            let mut ok = false;
                            for &k in &targets {
                                ok |= at::nudge(&mut scene.objects[k], &[here], d).contains(&(here + d));
                            }
                            if ok {
                                moved_to = Some(here + d);
                                changed = true;
                            }
                        }
                    }
                    sep(ui);
                    // Ajouter ou retirer une image après l'image courante (Alt+Maj+→ / ←)
                    for (lab, d, tip) in [("−1", -1, "Retirer une image après l'image courante : la suite se rapproche (Alt+Maj+←)"), ("+1", 1, "Ajouter une image après l'image courante : la suite s'éloigne (Alt+Maj+→)")] {
                        let k_ = !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.modifiers.alt && i.modifiers.shift && i.key_pressed(if d < 0 { Key::ArrowLeft } else { Key::ArrowRight }));
                        if ui.small_button(lab).on_hover_text(crate::i18n::t(tip)).clicked() || k_ {
                            for &k in &targets {
                                at::shift_after(&mut scene.objects[k], here, d);
                            }
                            changed = true;
                        }
                    }
                    sep(ui);
                    // Poses : copier, coller, coller en miroir, miroir, repos
                    if ui.small_button(crate::lucide::COPY).on_hover_text(crate::i18n::t("Copier la pose")).clicked() {
                        self.pose_clip = targets.iter().map(|&k| (scene.objects[k].name.clone(), at::Pose::of(&scene.objects[k]))).collect();
                        crate::notify(format!("{} · {}", crate::i18n::t("Pose copiée"), self.pose_clip.len()));
                    }
                    let can_paste = !self.pose_clip.is_empty();
                    for (icon, mirrored, tip) in [(crate::lucide::CLIPBOARD_PASTE, false, "Coller la pose (et poser une clé)"), (crate::lucide::FLIP_HORIZONTAL_2, true, "Coller la pose en miroir")] {
                        if ui.add_enabled(can_paste, egui::Button::new(icon).small()).on_hover_text(crate::i18n::t(tip)).clicked() {
                            for (n, &k) in targets.iter().enumerate() {
                                // Même nom d'abord ; sinon dans l'ordre
                                let name = scene.objects[k].name.clone();
                                let src = self.pose_clip.iter().find(|(nm, _)| *nm == name).or(self.pose_clip.get(n.min(self.pose_clip.len() - 1))).map(|x| x.1);
                                if let Some(pz) = src {
                                    at::put(&mut scene.objects[k], here, if mirrored { at::mirror(pz) } else { pz });
                                }
                            }
                            changed = true;
                        }
                    }
                    // Bibliothèque de poses (fichiers .arypose du coffre)
                    egui::containers::menu::MenuButton::new(egui::RichText::new(crate::lucide::LIBRARY_BIG)).ui(ui, |ui| {
                        ui.set_min_width(260.0);
                        let root = crate::nodes::vault_root();
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.pose_name).hint_text(crate::i18n::t("nom de la pose")).desired_width(170.0));
                            if ui.add_enabled(!targets.is_empty(), egui::Button::new(crate::lucide::SAVE)).on_hover_text(crate::i18n::t("Enregistrer la pose des objets choisis")).clicked() {
                                let poses: Vec<(String, at::Pose)> = targets.iter().map(|&k| (scene.objects[k].name.clone(), at::Pose::of(&scene.objects[k]))).collect();
                                let name = if self.pose_name.trim().is_empty() { format!("pose {}", at::library::list(&root).len() + 1) } else { self.pose_name.clone() };
                                match at::library::save(&root, &name, &poses) {
                                    Ok(_) => crate::notify(format!("{} · {name}", crate::i18n::t("Pose enregistrée"))),
                                    Err(e) => crate::notify(e),
                                }
                                self.pose_name.clear();
                            }
                        });
                        ui.add(egui::Slider::new(&mut self.pose_mix, 0.0..=1.0).text(crate::i18n::t("force"))).on_hover_text(crate::i18n::t("Appliquer en partie : 1 remplace la pose, 0,5 va à mi-chemin"));
                        ui.separator();
                        let list = at::library::list(&root);
                        if list.is_empty() {
                            ui.label(egui::RichText::new(crate::i18n::t("Aucune pose enregistrée : choisis des objets, donne un nom, enregistre.")).size(11.5).color(crate::theme::pal().muted));
                        }
                        for (name, path) in list {
                            ui.horizontal(|ui| {
                                let apply = ui.button(&name).on_hover_text(crate::i18n::t("Appliquer aux objets choisis (et poser une clé)")).clicked();
                                let mirror = ui.small_button(crate::lucide::FLIP_HORIZONTAL_2).on_hover_text(crate::i18n::t("Appliquer en miroir")).clicked();
                                if (apply || mirror) && !targets.is_empty() {
                                    if let Some(poses) = at::library::load(&path) {
                                        for (n, &k) in targets.iter().enumerate() {
                                            let nm = scene.objects[k].name.clone();
                                            if let Some(pz) = at::library::pick(&poses, &nm, n) {
                                                let pz = if mirror { at::mirror(pz) } else { pz };
                                                at::blend_to(&mut scene.objects[k], here, pz, self.pose_mix);
                                            }
                                        }
                                        changed = true;
                                    }
                                    ui.close();
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button(crate::lucide::TRASH_2).on_hover_text(crate::i18n::t("Supprimer la pose")).clicked() {
                                        // À la corbeille du coffre, pas effacée
                                        let _ = crate::vault::trash(&root, &format!("poses/{name}.arypose"));
                                    }
                                });
                            });
                        }
                    }).0.on_hover_text(crate::i18n::t("Bibliothèque de poses"));
                    if ui.small_button(crate::lucide::FLIP_HORIZONTAL).on_hover_text(crate::i18n::t("Miroir gauche-droite de la pose (et clé)")).clicked() {
                        for &k in &targets {
                            let pz = at::mirror(at::Pose::of(&scene.objects[k]));
                            at::put(&mut scene.objects[k], here, pz);
                        }
                        changed = true;
                    }
                    if ui.small_button(crate::lucide::ROTATE_CCW).on_hover_text(crate::i18n::t("Pose de repos : sans rotation, taille 1 (Maj : aussi à l'origine)")).clicked() {
                        let all = ui.input(|i| i.modifiers.shift);
                        for &k in &targets {
                            let pz = at::rest(at::Pose::of(&scene.objects[k]), all);
                            at::put(&mut scene.objects[k], here, pz);
                        }
                        changed = true;
                    }
                    sep(ui);
                    // Toute l'animation
                    if ui.small_button(crate::lucide::LINK).on_hover_text(crate::i18n::t("Chaque objet choisi reçoit une clé partout où l'un d'eux en a une")).clicked() {
                        let mut objs: Vec<&mut crate::scene::Object> = scene.objects.iter_mut().enumerate().filter(|(i, _)| targets.contains(i)).map(|(_, o)| o).collect();
                        let n = at::share_keys(&mut objs);
                        crate::notify(format!("{} · {n}", crate::i18n::t("Clés ajoutées")));
                        changed |= n > 0;
                    }
                    if ui.small_button(crate::lucide::MINIMIZE_2).on_hover_text(crate::i18n::t("Retire les clés que les voisines redonnent déjà (l'animation ne change pas)")).clicked() {
                        let n: usize = targets.iter().map(|&k| at::reduce(&mut scene.objects[k], 1e-3)).sum();
                        crate::notify(format!("{} · {n}", crate::i18n::t("Clés retirées")));
                        changed |= n > 0;
                    }
                    let mut off = 0i32;
                    let r = ui.add(egui::DragValue::new(&mut off).speed(0.15).prefix(format!("{} ", crate::lucide::MOVE_HORIZONTAL)).suffix(crate::i18n::t_mid(" im."))).on_hover_text(crate::i18n::t("Glisser : décale toute l'animation des objets choisis"));
                    if r.changed() && off != 0 {
                        for &k in &targets {
                            at::offset_all(&mut scene.objects[k], off);
                        }
                        changed = true;
                    }
                  });
                  });
                });
                if changed {
                    // La pose suit l'animation à l'image (éventuellement nouvelle) de la tête de lecture
                    let f = moved_to.map(|x| x as f32).unwrap_or(self.anim_frame);
                    scene.apply_frame(f);
                    if let Some(m) = moved_to {
                        seek = Some(m as f32);
                    }
                }
            }
            if self.anim_graph {
                // L'éditeur de courbes commun (le même qu'au Motion et au montage), dans le panneau ou dans
                // sa propre fenêtre du système
                let in_window = self.graph_window;
                let ctx_w = ui.ctx().clone();
                let mut draw = |ui: &mut egui::Ui| {
                let sel = self.scene_sel.filter(|&k| k < scene.objects.len());
                match sel {
                    Some(k) => {
                        let fps = scene.fps.max(1.0);
                        self.curve_ed.show_for(&format!("{}-{k}", scene.objects[k].name));
                        let mut cv = crate::scene::object_curves(&scene.objects[k].keys, fps);
                        let lp = scene.objects[k].cycle;
                        self.curve_ed.cycle = Some(lp);
                        let mut out = self.curve_ed.ui(ui, &mut cv, self.anim_frame / fps, (scene.start as f32 / fps, scene.end as f32 / fps), fps);
                        if let Some(new) = self.curve_ed.cycle.filter(|n| *n != lp) {
                            scene.objects[k].cycle = new;
                            out.changed = true;
                        }
                        if out.changed {
                            crate::scene::apply_object_curves(&mut scene.objects[k].keys, &cv, fps);
                            // La pose suit la courbe à l'image courante
                            if let Some((pos, rot, scale)) = scene.objects[k].pose_at(self.anim_frame) {
                                (scene.objects[k].pos, scene.objects[k].rot, scene.objects[k].scale) = (pos, rot, scale);
                            }
                            changed = true;
                        }
                        if let Some(t) = out.seek {
                            seek = Some((t * fps).round());
                        }
                    }
                    None => {
                        ui.centered_and_justified(|ui| ui.label(egui::RichText::new(crate::i18n::t("choisis un objet pour voir ses courbes")).color(p.muted)));
                    }
                }
                };
                if in_window {
                    if !crate::dock::os_window(&ctx_w, "3d-curves", &crate::i18n::t("Éditeur de courbes"), egui::vec2(900.0, 420.0), |ui| draw(ui)) {
                        self.graph_window = false;
                    }
                } else {
                    draw(ui);
                    return;
                }
            }
            // La règle et les pistes : un clic ou un glissement déplace la tête de lecture ; une piste par objet animé
            let full = ui.available_rect_before_wrap();
            let gutter = 96.0;
            let r = egui::Rect::from_min_max(egui::pos2(full.left() + gutter, full.top()), egui::pos2(full.right() - 12.0, full.bottom()));
            let resp = ui.interact(r, ui.scope_id().with("tl-track"), egui::Sense::click_and_drag());
            ui.allocate_rect(full, egui::Sense::hover());
            // Navigation commune à toutes les timelines (voir timenav), quand la souris est sur la timeline
            let (sa, sb) = (scene.start as f32, scene.end.max(scene.start + 1) as f32);
            let (mut a, mut b) = self.tl_view.unwrap_or((sa, sb));
            let span_w = (r.width() - 12.0).max(1.0);
            if let Some(w) = crate::timenav::wheel(ui, r, Some(&resp)) {
                if w.zoom != 1.0 {
                    let at = w.at.map(|q| a + (q.x - r.left() - 6.0) / span_w * (b - a)).unwrap_or((a + b) / 2.0);
                    let span = ((b - a) / w.zoom).clamp(4.0, (sb - sa) * 4.0 + 10.0);
                    let f = (at - a) / (b - a).max(1e-6);
                    a = at - span * f;
                    b = a + span;
                }
                let d = w.time_px / span_w * (b - a);
                (a, b) = (a + d, b + d);
                self.tl_view = Some((a, b));
            }
            if resp.hovered() {
                let k = crate::timenav::keys(ui);
                if k.home {
                    seek = Some(sa);
                }
                if k.end {
                    seek = Some(sb);
                }
                if k.step != 0 {
                    seek = Some((self.anim_frame + k.step as f32).clamp(sa, sb));
                }
                if k.jump != 0 {
                    let mut ks: Vec<i32> = match self.scene_sel.and_then(|i| scene.objects.get(i)) {
                        Some(o) => o.keys_in(&tl_layer).iter().map(|x| x.frame).collect(),
                        None => scene.objects.iter().flat_map(|o| o.keys_in(&tl_layer).iter().map(|x| x.frame)).collect(),
                    };
                    ks.sort();
                    let now = self.anim_frame.round() as i32;
                    let to = if k.jump > 0 { ks.into_iter().find(|f| *f > now) } else { ks.into_iter().rev().find(|f| *f < now) };
                    if let Some(f) = to {
                        seek = Some(f as f32);
                    }
                }
                if k.fit {
                    self.tl_view = None;
                    (a, b) = (sa, sb);
                }
                if k.zoom != 1.0 {
                    let c = self.anim_frame.clamp(a, b);
                    let span = ((b - a) / k.zoom).clamp(4.0, (sb - sa) * 4.0 + 10.0);
                    let f = (c - a) / (b - a).max(1e-6);
                    a = c - span * f;
                    b = a + span;
                    self.tl_view = Some((a, b));
                }
            }
            let x_of = |f: f32| r.left() + 6.0 + (r.width() - 12.0) * ((f - a) / (b - a));
            let painter = ui.painter_at(full);
            let step = [1, 2, 5, 10, 20, 50, 100, 200, 500].into_iter().find(|s| (r.width() - 12.0) / (b - a) * *s as f32 > 56.0).unwrap_or(1000);
            let mut f = (scene.start as f32 / step as f32).ceil() as i32 * step;
            while f <= scene.end {
                let x = x_of(f as f32);
                painter.text(egui::pos2(x, r.top() + 4.0), egui::Align2::LEFT_TOP, f.to_string(), egui::FontId::monospace(11.0), p.muted);
                f += step;
            }
            let mut y = r.top() + 26.0;
            // Montage d'animation : les bandes, la rangée 0 en bas (celles du dessus passent après)
            let mut strip_hits: Vec<(usize, egui::Rect)> = vec![];
            let rows = scene.strips.iter().map(|s| s.row as usize + 1).max().unwrap_or(0);
            if rows > 0 {
                painter.text(egui::pos2(r.left() - 8.0, y + (rows as f32 - 1.0) * 10.0), egui::Align2::RIGHT_CENTER, crate::i18n::t("montage"), egui::FontId::proportional(11.0), p.muted);
                for (i, s) in scene.strips.iter().enumerate() {
                    let Some(act) = scene.actions.iter().find(|x| x.name == s.action) else { continue };
                    let yy = y + (rows - 1 - s.row as usize) as f32 * 20.0;
                    let rc = egui::Rect::from_min_max(egui::pos2(x_of(s.start as f32), yy - 8.0), egui::pos2(x_of(s.start as f32 + s.len(act)).max(x_of(s.start as f32) + 6.0), yy + 8.0));
                    let on = self.strip_sel == Some(i);
                    let col = if s.additive { Color32::from_rgb(120, 190, 140) } else { p.accent };
                    painter.rect_filled(rc, 4.0, col.gamma_multiply(if s.mute { 0.12 } else if on { 0.6 } else { 0.32 }));
                    painter.rect_stroke(rc, 4.0, Stroke::new(1.0, if on { p.text } else { col.gamma_multiply(0.7) }), egui::StrokeKind::Inside);
                    // Un trait à chaque cycle, et les fondus en pente
                    let cyc = act.len() * s.scale.max(0.01);
                    let mut c = 1.0;
                    while c < s.repeat && c < 200.0 {
                        let x = x_of(s.start as f32 + c * cyc);
                        painter.line_segment([egui::pos2(x, rc.top() + 2.0), egui::pos2(x, rc.bottom() - 2.0)], Stroke::new(1.0, p.text.gamma_multiply(0.45)));
                        c += 1.0;
                    }
                    if s.blend_in > 0 {
                        painter.line_segment([rc.left_bottom(), egui::pos2(x_of((s.start + s.blend_in) as f32).min(rc.right()), rc.top())], Stroke::new(1.0, p.text.gamma_multiply(0.6)));
                    }
                    if s.blend_out > 0 {
                        painter.line_segment([egui::pos2(x_of(s.start as f32 + s.len(act) - s.blend_out as f32).max(rc.left()), rc.top()), rc.right_bottom()], Stroke::new(1.0, p.text.gamma_multiply(0.6)));
                    }
                    let label = if (s.repeat - 1.0).abs() > 1e-3 { format!("{} ×{}", s.action, (s.repeat * 100.0).round() / 100.0) } else { s.action.clone() };
                    painter.with_clip_rect(rc.intersect(full)).text(egui::pos2(rc.left() + 5.0, yy), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(10.5), if s.mute { p.muted } else { p.text });
                    strip_hits.push((i, rc));
                }
                y += rows as f32 * 20.0 + 4.0;
            }
            let strip_under = |pt: egui::Pos2| strip_hits.iter().rev().find(|h| h.1.expand(2.0).contains(pt)).map(|h| (h.0, h.1));
            // Les clés : forme selon le lissage, clic pour choisir, glisser pour déplacer, clic droit pour le lissage
            let mut hits: Vec<(usize, i32, egui::Pos2)> = vec![];
            for (k, o) in scene.objects.iter().enumerate() {
                let shown = o.keys_in(&tl_layer);
                if shown.is_empty() || y + 10.0 > full.bottom() {
                    continue;
                }
                let on = self.scene_sel == Some(k);
                painter.text(egui::pos2(r.left() - 8.0, y), egui::Align2::RIGHT_CENTER, &o.name, egui::FontId::proportional(11.0), if on { p.text } else { p.muted });
                // Calque actif : ses clés en couleur de calque, celles de la base en petits repères derrière
                let layer_col = Color32::from_rgb(235, 170, 70);
                if !tl_layer.is_empty() {
                    for key in &o.keys {
                        painter.circle_filled(egui::pos2(x_of(key.frame as f32), y), 1.8, p.muted.gamma_multiply(0.7));
                    }
                }
                for key in shown {
                    let frame = match self.key_drag {
                        Some((dk, from, cur)) if dk == k && from == key.frame => cur,
                        _ => key.frame,
                    };
                    let c = egui::pos2(x_of(frame as f32), y);
                    let chosen = self.key_sel == Some((k, key.frame));
                    let ease = if key.mode() == "bezier" { None } else { key.ease() };
                    crate::keys::draw(&painter, c, 5.0, ease, if !tl_layer.is_empty() { layer_col.gamma_multiply(if on { 1.0 } else { 0.6 }) } else if on { p.accent } else { p.muted }, true, chosen);
                    hits.push((k, key.frame, c));
                }
                y += 22.0;
            }
            let x = x_of(self.anim_frame);
            painter.line_segment([egui::pos2(x, full.top()), egui::pos2(x, full.bottom())], Stroke::new(1.0, p.accent));
            let frame_at = |px: f32| (a + (b - a) * ((px - r.left() - 6.0) / (r.width() - 12.0)).clamp(0.0, 1.0)).round().clamp(sa, sb);
            let key_under = |pt: egui::Pos2| hits.iter().filter(|h| (h.2 - pt).length() < 8.0).min_by(|u, v| (u.2 - pt).length().total_cmp(&(v.2 - pt).length())).map(|h| (h.0, h.1));
            // Une bande se glisse (début) ou s'allonge par son bord droit (répétitions)
            let origin = ui.input(|i| i.pointer.press_origin());
            if resp.drag_started() {
                if let Some((i, rc)) = origin.and_then(strip_under) {
                    let edge = origin.is_some_and(|o| rc.right() - o.x < 7.0 && rc.width() > 16.0);
                    if let Some(s) = scene.strips.get(i) {
                        self.strip_drag = Some((i, edge, s.start, s.repeat, origin.map(|o| o.x).unwrap_or(0.0)));
                        self.strip_sel = Some(i);
                    }
                } else if let Some((k, f)) = origin.and_then(key_under) {
                    self.key_drag = Some((k, f, f));
                    self.key_sel = Some((k, f));
                    self.scene_sel = Some(k);
                }
            }
            if let Some((i, edge, start0, rep0, x0)) = self.strip_drag {
                if resp.dragged() {
                    if let (Some(at), true) = (resp.interact_pointer_pos(), i < scene.strips.len()) {
                        let df = (at.x - x0) / ((r.width() - 12.0) / (b - a)).max(1e-6);
                        let cyc = scene.actions.iter().find(|x| x.name == scene.strips[i].action).map(|x| x.len()).unwrap_or(1.0) * scene.strips[i].scale.max(0.01);
                        let s = &mut scene.strips[i];
                        if edge {
                            // Par quarts de cycle ; Maj : librement
                            let rep = (rep0 + df / cyc).max(0.25);
                            s.repeat = if ui.input(|i| i.modifiers.shift) { rep } else { (rep * 4.0).round() / 4.0 };
                        } else {
                            s.start = start0 + df.round() as i32;
                        }
                        changed = true;
                    }
                } else {
                    self.strip_drag = None;
                    changed = true;
                }
            }
            let on_strip = resp.clicked() && resp.interact_pointer_pos().and_then(strip_under).map(|h| self.strip_sel = Some(h.0)).is_some();
            if self.strip_drag.is_none() && !on_strip {
            match self.key_drag {
                Some((k, from, _)) if resp.dragged() => {
                    if let Some(at) = resp.interact_pointer_pos() {
                        let to = frame_at(at.x) as i32;
                        // Pas deux clés sur la même image
                        let free = scene.objects.get(k).is_some_and(|o| to == from || !o.keys_in(&tl_layer).iter().any(|x| x.frame == to));
                        if free {
                            self.key_drag = Some((k, from, to));
                        }
                    }
                }
                Some((k, from, to)) if !resp.dragged() => {
                    self.key_drag = None;
                    if from != to {
                        if let Some(ks) = scene.objects.get_mut(k).and_then(|o| o.keys_in_mut(&tl_layer)) {
                            if let Some(key) = ks.iter_mut().find(|x| x.frame == from) {
                                key.frame = to;
                            }
                            ks.sort_by_key(|x| x.frame);
                        }
                        self.key_sel = Some((k, to));
                        seek = Some(to as f32);
                        changed = true;
                    }
                }
                _ => {
                    if let Some(at) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
                        match key_under(at).filter(|_| resp.clicked()) {
                            Some((k, f)) => {
                                self.key_sel = Some((k, f));
                                self.scene_sel = Some(k);
                                seek = Some(f as f32);
                            }
                            None => {
                                self.key_sel = None;
                                self.strip_sel = None;
                                seek = Some(frame_at(at.x));
                            }
                        }
                    }
                }
            }
            }
            if resp.secondary_clicked() {
                self.strip_menu = resp.interact_pointer_pos().and_then(strip_under).map(|h| h.0);
                self.strip_sel = self.strip_menu.or(self.strip_sel);
                self.key_menu = if self.strip_menu.is_some() { None } else { resp.interact_pointer_pos().and_then(key_under) };
                if let Some(km) = self.key_menu {
                    self.key_sel = Some(km);
                }
            }
            // Réglages d'une bande : clic droit dessus
            if let Some(i) = self.strip_menu.filter(|&i| i < scene.strips.len()) {
                let (mut dup, mut del, mut del_action) = (false, false, false);
                resp.context_menu(|ui| {
                    ui.set_min_width(250.0);
                    let s = &mut scene.strips[i];
                    ui.label(egui::RichText::new(format!("{}  {}", crate::lucide::FILM, s.action)).strong());
                    changed |= crate::modifiers::row(ui, &crate::i18n::t("répétitions"), |ui| ui.add(egui::DragValue::new(&mut s.repeat).speed(0.05).range(0.1..=1000.0)).changed());
                    changed |= crate::modifiers::row(ui, &crate::i18n::t("durée ×"), |ui| ui.add(egui::DragValue::new(&mut s.scale).speed(0.01).range(0.05..=50.0)).on_hover_text(crate::i18n::t("Étire l'action dans le temps (2 : deux fois plus lente)")).changed());
                    changed |= crate::modifiers::row_slider(ui, "poids", egui::Slider::new(&mut s.weight, 0.0..=1.0));
                    changed |= crate::modifiers::row(ui, &crate::i18n::t("fondu d'entrée"), |ui| ui.add(egui::DragValue::new(&mut s.blend_in).range(0..=10000).suffix(" im.")).changed());
                    changed |= crate::modifiers::row(ui, &crate::i18n::t("fondu de sortie"), |ui| ui.add(egui::DragValue::new(&mut s.blend_out).range(0..=10000).suffix(" im.")).changed());
                    changed |= ui.checkbox(&mut s.additive, crate::i18n::t("additive")).on_hover_text(crate::i18n::t("Ajoute le mouvement de l'action à ce qu'il y a dessous, au lieu de le remplacer")).changed();
                    changed |= ui.checkbox(&mut s.travel, crate::i18n::t("avance à chaque cycle")).on_hover_text(crate::i18n::t("Chaque répétition repart d'où la précédente est arrivée (un cycle de marche qui avance)")).changed();
                    changed |= ui.checkbox(&mut s.hold, crate::i18n::t("tenir la dernière pose")).changed();
                    changed |= ui.checkbox(&mut s.mute, crate::i18n::t("muette")).changed();
                    ui.separator();
                    if ui.button(format!("{}  {}", crate::lucide::COPY_PLUS, crate::i18n::t("dupliquer à la suite"))).clicked() {
                        dup = true;
                        ui.close();
                    }
                    if ui.button(format!("{}  {}", crate::lucide::TRASH_2, crate::i18n::t("retirer la bande"))).clicked() {
                        del = true;
                        ui.close();
                    }
                    if ui.button(format!("{}  {}", crate::lucide::TRASH_2, crate::i18n::t("retirer l'action et toutes ses bandes"))).clicked() {
                        del_action = true;
                        ui.close();
                    }
                });
                if dup {
                    let mut s = scene.strips[i].clone();
                    let len = scene.actions.iter().find(|x| x.name == s.action).map(|x| s.len(x)).unwrap_or(1.0);
                    s.start += len.round() as i32;
                    s.row = crate::nla::free_row(scene, s.start as f32, s.start as f32 + len);
                    scene.strips.push(s);
                    self.strip_sel = Some(scene.strips.len() - 1);
                    changed = true;
                }
                if del {
                    scene.strips.remove(i);
                    changed = true;
                }
                if del_action {
                    let name = scene.strips[i].action.clone();
                    crate::nla::remove_action(scene, &name);
                    changed = true;
                }
                if del || del_action {
                    (self.strip_menu, self.strip_sel) = (None, None);
                }
            }
            if let Some((k, f)) = self.key_menu {
                let cur = scene.objects.get(k).and_then(|o| o.keys_in(&tl_layer).iter().find(|x| x.frame == f)).map(|x| if x.mode() == "bezier" { None } else { x.ease() });
                let mut picked = None;
                let mut remove = false;
                if let Some(cur) = cur {
                    resp.context_menu(|ui| {
                        if let Some(c) = crate::keys::menu(ui, cur, true) {
                            picked = Some(c);
                        }
                        ui.separator();
                        if ui.button(format!("{}  {}", crate::lucide::TRASH_2, crate::i18n::t("retirer la clé"))).clicked() {
                            remove = true;
                            ui.close();
                        }
                    });
                }
                if let Some(c) = picked {
                    if let Some(key) = scene.objects.get_mut(k).and_then(|o| o.keys_in_mut(&tl_layer)).and_then(|ks| ks.iter_mut().find(|x| x.frame == f)) {
                        key.interp = c.map(|e| e.word()).unwrap_or("bezier").to_string();
                        changed = true;
                    }
                    self.key_menu = None;
                }
                if remove {
                    if let Some(ks) = scene.objects.get_mut(k).and_then(|o| o.keys_in_mut(&tl_layer)) {
                        ks.retain(|x| x.frame != f);
                        changed = true;
                    }
                    self.key_menu = None;
                    self.key_sel = None;
                }
            }
            // Suppr : retire la clé choisie, sinon la bande choisie (la souris sur la timeline)
            if resp.hovered() && ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)) {
                if let Some((k, f)) = self.key_sel.take() {
                    if let Some(ks) = scene.objects.get_mut(k).and_then(|o| o.keys_in_mut(&tl_layer)) {
                        ks.retain(|x| x.frame != f);
                        changed = true;
                    }
                } else if let Some(i) = self.strip_sel.take().filter(|&i| i < scene.strips.len()) {
                    scene.strips.remove(i);
                    self.strip_menu = None;
                    changed = true;
                }
            }
        };
        if let Some(sub_r) = crate::layout::offer("3d-timeline", "timeline 3D") {
            crate::layout::draw_in(ui, sub_r, p.panel, 6.0, sub_body);
        } else {
            tl_panel.frame(tl_frame).show(ui, sub_body);
        }
        // La timeline se dessine après les panneaux de côté (elle reste sous la vue, comme la maquette) :
        // une image choisie là s'applique tout de suite
        if seek != seek_before {
            if let Some(f) = seek {
                if f != self.anim_frame {
                    self.anim_frame = f;
                    if scene.apply_frame(f) {
                        self.scene_dirty = true;
                        self.scene_saved = Some(scene.clone());
                    }
                }
            }
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
            if let Some((op, k)) = morph_op.filter(|x| x.1 < scene.objects.len()) {
                let dir = path.parent().unwrap_or(std::path::Path::new("")).to_path_buf();
                let rel = |f: &std::path::Path| f.strip_prefix(&dir).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| f.to_string_lossy().to_string());
                match op {
                    1 => match self.mesh.as_mut().filter(|m| m.obj == k) {
                        Some(me) => match me.hist.first().map(|h| h.0.clone()) {
                            Some(base) if base.pos.len() == me.mesh.pos.len() && base.polys == me.mesh.polys => {
                                let stem = me.file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                                let mut n = scene.objects[k].morphs.len() + 1;
                                let mut f = dir.join(format!("{stem} forme {n}.obj"));
                                while f.exists() {
                                    n += 1;
                                    f = dir.join(format!("{stem} forme {n}.obj"));
                                }
                                match me.mesh.write_obj(&f) {
                                    Ok(()) => {
                                        me.hist.push((me.mesh.clone(), me.sel.clone()));
                                        me.mesh = base;
                                        let _ = me.mesh.write_obj(&me.file);
                                        self.mesh_map = None;
                                        scene.objects[k].morphs.push(crate::scene::Morph { name: format!("{} {n}", crate::i18n::t_mid("forme")), file: rel(&f), value: 0.0, keys: vec![] });
                                        changed = true;
                                    }
                                    Err(e) => self.capture_msg = Some(e),
                                }
                            }
                            _ => self.capture_msg = Some(crate::i18n::t("rien à garder : modifiez d'abord le maillage, sans ajouter ni retirer de sommets")),
                        },
                        None => {}
                    },
                    _ => {
                        if let Some(f) = rfd::FileDialog::new().add_filter("OBJ", &["obj"]).set_directory(&dir).pick_file() {
                            let name = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                            scene.objects[k].morphs.push(crate::scene::Morph { name, file: rel(&f), value: 0.0, keys: vec![] });
                            changed = true;
                        }
                    }
                }
            }
            if let Some((op, k)) = rig_op.filter(|x| x.1 < scene.objects.len()) {
                match op {
                    1 => {
                        if let Some(r) = scene.objects.iter().position(|o| o.name == scene.objects[k].skin) {
                            crate::rig::bind(scene, k, r);
                        }
                    }
                    2 => {
                        let r = crate::rig::root_of(scene, k);
                        crate::rig::set_rest(scene, r);
                    }
                    3 => {
                        let r = crate::rig::root_of(scene, k);
                        crate::rig::to_rest(scene, r);
                    }
                    _ => {
                        let b = crate::rig::add_bone(scene, Some(k));
                        self.scene_sel = Some(b);
                        self.scene_multi.clear();
                    }
                }
                changed = true;
            }
            if let Some(k) = look_through.filter(|&k| k < scene.objects.len()) {
                // Cette caméra devient celle de la scène (vue, rendu d'animation), et la vue la suit
                let name = scene.objects[k].name.clone();
                if scene.camera != name {
                    scene.camera = name;
                    changed = true;
                }
                self.look_through = true;
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
        if !ui.ctx().egui_wants_keyboard_input() && self.edit.is_none() && self.mesh.is_none() && self.paint3d.is_none() && self.modal.is_none() && (undo || redo) {
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
        // Mode édition : les sommets déplacés pendant le geste remplacent ceux de l'affichage, sans reconstruire
        if self.mesh_moved {
            self.mesh_moved = false;
            self.mesh_unsaved = true;
            let ok = match (&self.mesh, &self.prepared, &self.state) {
                (Some(me), Some(prep), State::Ready(m)) if self.scene_rx.is_none() && !prep.0 => {
                    let range = self.scene_ranges.iter().find(|r| r.0 == me.obj).map(|r| (r.1, r.2));
                    let before = me.hist.last().map(|h| &h.0.pos);
                    match (range, before) {
                        (Some((t0, t1)), Some(before)) if before.len() == me.mesh.pos.len() => {
                            let s = m.size();
                            let scale = 1.0 / s[0].max(s[1]).max(s[2]).max(1e-9);
                            let c = [(m.min[0] + m.max[0]) / 2.0, (m.min[1] + m.max[1]) / 2.0, (m.min[2] + m.max[2]) / 2.0];
                            let norm = |w: V3| [(w[0] - c[0]) * scale, (w[1] - c[1]) * scale, (w[2] - c[2]) * scale];
                            let world = |v: V3| scene.world(me.obj, v);
                            // La correspondance se cherche une fois par geste, sur les positions d'avant
                            let fresh = self.mesh_map.as_ref().is_some_and(|mm| mm.0 == me.obj && mm.1 == me.mesh.pos.len());
                            if !fresh {
                                self.mesh_map = map_edit_vertices(&prep.2, t0, t1, &before.iter().map(|v| norm(world(*v))).collect::<Vec<_>>()).map(|l| (me.obj, me.mesh.pos.len(), l));
                            }
                            match &self.mesh_map {
                                Some((_, _, list)) => {
                                    let now: Vec<V3> = me.mesh.pos.iter().map(|v| norm(world(*v))).collect();
                                    let np = patch_vertices(&prep.2, t0, t1, list, &now);
                                    self.prepared = Some((prep.0, prep.1, std::sync::Arc::new(np)));
                                    self.last = None;
                                    self.trace = None;
                                    true
                                }
                                None => false,
                            }
                        }
                        _ => false,
                    }
                }
                _ => false,
            };
            if !ok {
                // Pas de correspondance sûre : on reconstruit comme avant
                if let Some(me) = &self.mesh {
                    let _ = me.mesh.write_obj(&me.file);
                }
                self.mesh_unsaved = false;
                self.scene_dirty = true;
            }
        }
        let gesture = self.gizmo_hold.is_some() || self.mesh.as_ref().is_some_and(|m| m.modal.is_some()) || down;
        if self.mesh_unsaved && !gesture {
            self.mesh_unsaved = false;
            self.mesh_map = None;
            if let Some(me) = &self.mesh {
                let _ = me.mesh.write_obj(&me.file);
            }
            self.scene_dirty = true;
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
        if std::mem::take(&mut self.join_request) {
            self.join_selected();
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
                // Peintures des objets : chargées une fois (puis tenues à jour par le pinceau)
                self.paint_tex.resize_with(scene.objects.len(), || None);
                for (k, o) in scene.objects.iter().enumerate() {
                    let want = crate::paint3d::layers_of(o);
                    if self.paint_tex[k].as_ref().map(|s| s.metas()).unwrap_or_default() != want {
                        // Les images déjà en mémoire sont gardées (même fichier) ; un calque tout neuf part d'une image vide
                        let old = self.paint_tex[k].take();
                        let active = old.as_ref().and_then(|s| s.layers.get(s.active)).map(|l| l.0.name.clone());
                        let dims = old.as_ref().and_then(|s| s.layers.first()).and_then(|l| l.1.read().ok().map(|i| i.dimensions()));
                        let mut dims = dims.filter(|d| d.0 > 0 && d.1 > 0);
                        let layers: Vec<(crate::paint3d::PaintLayer, crate::paint3d::Shared)> = want
                            .into_iter()
                            .map(|l| {
                                let kept = old.as_ref().and_then(|s| s.layers.iter().find(|x| x.0.file == l.file)).map(|x| x.1.clone());
                                let tex = kept.or_else(|| image::open(std::path::Path::new(&self.root).join(&l.file)).ok().map(|i| std::sync::Arc::new(std::sync::RwLock::new(i.to_rgba8())))).unwrap_or_else(|| {
                                    let (w, h) = dims.unwrap_or((crate::paint3d::SIZE, crate::paint3d::SIZE));
                                    std::sync::Arc::new(std::sync::RwLock::new(image::RgbaImage::new(w, h)))
                                });
                                if dims.is_none() {
                                    dims = tex.read().ok().map(|i| i.dimensions());
                                }
                                (l, tex)
                            })
                            .collect();
                        self.paint_tex[k] = (!layers.is_empty()).then(|| {
                            let mut st = crate::paint3d::Stack::new(layers);
                            if let Some(i) = active.and_then(|n| st.layers.iter().position(|l| l.0.name == n)) {
                                st.active = i;
                            }
                            st
                        });
                        self.slot_rev += 1;
                    }
                }
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
        let hdr_area = egui::Area::new(egui::Id::unique(("hdr3d", self.path.to_string_lossy().to_string()))).order(egui::Order::Middle).fixed_pos(vp.min + egui::vec2(12.0, 10.0)).show(ui.ctx(), |ui| {
        ui.set_max_width((vp.width() - 24.0).max(200.0));
        // Comme la maquette : une barre de 34 px sur toute la largeur, boutons sans cadre
        ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
        ui.spacing_mut().interact_size.y = 22.0;
        {
            let v = &mut ui.style_mut().visuals.widgets;
            v.inactive.weak_bg_fill = Color32::TRANSPARENT;
            v.inactive.bg_stroke = Stroke::NONE;
            v.hovered.weak_bg_fill = p.raised;
            v.hovered.bg_stroke = Stroke::NONE;
            v.open.weak_bg_fill = p.raised;
        }
        egui::Frame::new().fill(p.panel).corner_radius(6).stroke(Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(6, 5)).show(ui, |ui| {
        ui.set_min_width((vp.width() - 24.0 - 14.0).max(120.0));
        egui::ScrollArea::horizontal().id_salt("hdr3d-scroll").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).max_width((vp.width() - 24.0 - 14.0).max(120.0)).show(ui, |ui| ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let mut right_x0: Option<f32> = None;
            let hdr_x0 = ui.cursor().min.x;
            let hdr_w = (vp.width() - 24.0 - 14.0).max(120.0);
            for item in HEADER {
                match item {
                    Hdr::Mode if !is_scene && !self.can_edit() => {
                        if ui.add(egui::Button::new(egui::RichText::new(format!("✚ {}", crate::i18n::t("ouvrir dans une scène"))).family(theme::bold()).size(12.0)).fill(p.panel)).on_hover_text(crate::i18n::t("Le modèle seul se regarde. Pour le modifier (édition, modificateurs, UV, retopo), on le place dans une scène.")).clicked() {
                            self.open_in_scene();
                        }
                    }
                    Hdr::Mode => {
                        let mode = if self.paint3d.is_some() { "peinture" } else if self.sculpt.is_some() { "sculpture" } else if self.mesh.is_some() { "mode édition" } else if self.edit.is_some() { "UV et matériaux" } else { "mode objet" };
                    egui::containers::menu::MenuButton::from_button(egui::Button::new(egui::RichText::new(format!("◆ {} ▾", crate::i18n::t(mode))).family(theme::bold()).size(12.0)).fill(p.panel)).ui(ui, |ui| {
                            ui.set_min_width(220.0);
                            if ui.selectable_label(self.mesh.is_none() && self.edit.is_none() && self.paint3d.is_none(), crate::i18n::t("mode objet")).clicked() {
                                self.paint3d = None;
                                if self.mesh.is_some() {
                                    self.mesh_toggle = true;
                                }
                                want_edit = Some(false);
                                ui.close();
                            }
                            if is_scene {
                                if ui.selectable_label(self.mesh.is_some() && self.sculpt.is_none(), format!("{}   Tab", crate::i18n::t("mode édition"))).on_hover_text(crate::i18n::t("Sommets, extrusion, faces de l'objet choisi")).clicked() {
                                    self.sculpt = None;
                                    if self.mesh.is_none() {
                                        self.mesh_toggle = true;
                                    }
                                    ui.close();
                                }
                                if ui.selectable_label(self.sculpt.is_some(), format!("{}  {}", crate::lucide::BRUSH, crate::i18n::t("sculpture"))).on_hover_text(crate::i18n::t("Pinceaux qui modèlent la surface : dessiner, argile, gonfler, lisser, saisir, aplatir, pincer")).clicked() {
                                    if self.mesh.is_none() {
                                        self.mesh_toggle = true;
                                    }
                                    self.sculpt = Some(SculptUi::new());
                                    ui.close();
                                }
                                if ui.selectable_label(self.paint3d.is_some(), format!("{}  {}", crate::lucide::PAINTBRUSH, crate::i18n::t("peinture"))).on_hover_text(crate::i18n::t("Peindre directement sur l'objet, comme dans Substance Painter (il faut des UV)")).clicked() {
                                    self.sculpt = None;
                                    if self.mesh.is_some() {
                                        self.mesh_toggle = true;
                                    }
                                    self.paint3d = Some(crate::paint3d::Brush::default());
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
                        // Toujours à la même place, grisés hors du mode édition : rien ne bouge en changeant de mode
                        if is_scene {
                            let cur = self.mesh.as_ref().map(|m| m.mode);
                            for (k, (icon, tip)) in [("•", "sommets (1)"), ("⁄", "arêtes (2)"), ("▱", "faces (3)")].into_iter().enumerate() {
                                let on = cur == Some(k as u8);
                                let b = egui::Button::new(egui::RichText::new(icon).size(13.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { p.panel }).min_size(egui::vec2(24.0, 20.0));
                                if ui.add_enabled(cur.is_some(), b).on_hover_text(crate::i18n::t(tip)).on_disabled_hover_text(crate::i18n::t("en mode édition (Tab)")).clicked() {
                                    if let Some(me) = self.mesh.as_mut() {
                                        me.mode = k as u8;
                                    }
                                }
                            }
                            let xb = egui::Button::new(egui::RichText::new("◫").size(13.0).color(if self.xray { p.on_accent } else { p.text })).fill(if self.xray { p.accent } else { p.panel }).min_size(egui::vec2(24.0, 20.0));
                            if ui.add(xb).on_hover_text(crate::i18n::t("rayons X (Alt+Z) : voir et choisir aussi les sommets cachés derrière le maillage")).clicked() {
                                self.xray = !self.xray;
                            }
                            let on = self.mesh.as_ref().is_some_and(|m| m.prop);
                            let pb = egui::Button::new(egui::RichText::new("◎").size(13.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { p.panel }).min_size(egui::vec2(24.0, 20.0));
                            if ui.add_enabled(self.mesh.is_some(), pb).on_hover_text(crate::i18n::t("édition proportionnelle (O) : les sommets voisins suivent ; molette pendant G R S pour le rayon")).clicked() {
                                if let Some(m) = self.mesh.as_mut() {
                                    m.prop = !m.prop;
                                }
                            }
                            let on = self.mesh.as_ref().is_some_and(|m| m.snap_v);
                            let sb = egui::Button::new(egui::RichText::new("⌖").size(13.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { p.panel }).min_size(egui::vec2(24.0, 20.0));
                            if ui.add_enabled(self.mesh.is_some(), sb).on_hover_text(crate::i18n::t("aimanter sur les sommets (Maj+Tab) : pendant G, le sommet choisi le plus proche se pose sur le sommet visé")).clicked() {
                                if let Some(m) = self.mesh.as_mut() {
                                    m.snap_v = !m.snap_v;
                                }
                            }
                            let on = self.mesh.as_ref().is_some_and(|m| m.mirror);
                            let mb = egui::Button::new(egui::RichText::new("⇋").size(13.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { p.panel }).min_size(egui::vec2(24.0, 20.0));
                            if ui.add_enabled(self.mesh.is_some(), mb).on_hover_text(crate::i18n::t("miroir X : en déplaçant un sommet, son reflet de l'autre côté de l'objet suit")).clicked() {
                                if let Some(m) = self.mesh.as_mut() {
                                    m.mirror = !m.mirror;
                                }
                            }
                            // Point de pivot (« . » ouvre aussi le menu circulaire)
                            let names = ["point médian", "curseur 3D", "origines individuelles", "objet actif"];
                            let icons = ["⊙", "⊕", "⁘", "◉"];
                            egui::containers::menu::MenuButton::from_button(egui::Button::new(egui::RichText::new(format!("{} ▾", icons[self.pivot as usize & 3])).size(13.0)).fill(p.panel).min_size(egui::vec2(30.0, 20.0)))
                                .ui(ui, |ui| {
                                    ui.label(crate::theme::section(&crate::i18n::t("point de pivot")));
                                    for (k, n) in names.iter().enumerate() {
                                        if ui.selectable_label(self.pivot as usize == k, format!("{}  {}", icons[k], crate::i18n::t(n))).clicked() {
                                            self.pivot = k as u8;
                                            ui.close();
                                        }
                                    }
                                })
                                .0
                                .on_hover_text(format!("{} (.)", crate::i18n::t("point de pivot")));
                        }
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
                            if ui.button(format!("{}  {}", crate::lucide::SCAN, crate::i18n::t("tout cadrer (Origine)"))).clicked() {
                                self.cam = Camera { ortho: self.cam.ortho, ..Default::default() };
                            }
                            if ui.button(format!("{}  {}", crate::lucide::SCAN, crate::i18n::t("cadrer la sélection (Maj+F)"))).clicked() {
                                self.frame_selection();
                            }
                            ui.checkbox(&mut self.orbit_sel, crate::i18n::t("tourner autour de la sélection"));

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
                            for (label, id) in [("lumière soleil", "light"), ("lumière ponctuelle", "light-point"), ("spot", "light-spot"), ("lumière rectangle", "light-area"), ("caméra", "camera"), ("groupe vide", "empty"), ("os", "bone")] {
                                if ui.button(crate::i18n::t(label)).clicked() {
                                    self.hdr_add = Some(id);
                                    ui.close();
                                }
                            }
                            if ui.button(format!("{}  {}", crate::lucide::PERSON_STANDING, crate::i18n::t("capture de mouvement (BVH)…"))).on_hover_text(crate::i18n::t("Un squelette animé depuis un fichier BVH ; si un os est choisi, le mouvement va sur son squelette (os de même nom)")).clicked() {
                                self.hdr_add = Some("bvh");
                                ui.close();
                            }
                            ui.separator();
                            ui.label(egui::RichText::new(crate::i18n::t("Maj+A dans la vue : le même menu sous le curseur")).size(10.5).color(p.muted));
                        });
                    }
                    Hdr::Object => {
                        egui::containers::menu::MenuButton::new(crate::i18n::t("objet")).ui(ui, |ui| {
                            ui.set_min_width(230.0);
                            if is_scene {
                                use crate::lucide as lc;
                                for (icon, label, key, m) in [
                                    (lc::COPY, "dupliquer   Maj+D", Key::D, egui::Modifiers::SHIFT),
                                    (lc::TRASH_2, "supprimer   X", Key::X, egui::Modifiers::NONE),
                                    (lc::EYE_OFF, "cacher   H", Key::H, egui::Modifiers::NONE),
                                    (lc::EYE, "tout afficher   Alt+H", Key::H, egui::Modifiers::ALT),
                                    (lc::LINK, "lier au parent actif   P", Key::P, egui::Modifiers::NONE),
                                    (lc::UNLINK, "retirer le parent   Alt+P", Key::P, egui::Modifiers::ALT),
                                    (lc::DIAMOND_PLUS, "poser une clé   I", Key::I, egui::Modifiers::NONE),
                                ] {
                                    if ui.button(format!("{icon}  {}", crate::i18n::t(label))).clicked() {
                                        self.hdr_key = Some((key, m));
                                        ui.close();
                                    }
                                }
                                ui.separator();
                            }
                            if (is_scene || self.can_edit()) && ui.button(format!("{}  {}", crate::lucide::WRENCH, crate::i18n::t("modificateurs…"))).clicked() {
                                self.show_mods = true;
                                self.edit = None;
                                self.prepared = None;
                                ui.close();
                            }
                            ui.menu_button(format!("{}  {}", crate::lucide::CAMERA, crate::i18n::t("capture")), |ui| {
                                if ui.button(format!("{}  {}", crate::lucide::IMAGE, crate::i18n::t("image PNG (2400 px)"))).clicked() {
                                    do_capture = true;
                                }
                                if is_scene && ui.button(format!("{}  {}", crate::lucide::FILM, crate::i18n::t("animation en vidéo (MP4)…"))).on_hover_text(crate::i18n::t("Chaque image de la scène, du début à la fin, vue par sa caméra (ou par la vue actuelle s'il n'y en a pas)")).clicked() {
                                    self.want_anim = true;
                                }
                                if ui.button(format!("{}  {}", crate::lucide::ROTATE_3D, crate::i18n::t("turntable GIF (48 images)"))).clicked() {
                                    do_gif = true;
                                }
                                ui.label(egui::RichText::new(crate::i18n::t("enregistré à côté du modèle")).color(p.muted).size(10.5));
                            });
                            if ui.button(format!("{}  {}", crate::lucide::DOWNLOAD, crate::i18n::t("exporter…"))).clicked() {
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
                        // Comme la maquette : l'ombrage et le rendu sont poussés à droite (largeur mesurée à l'image d'avant)
                        let id = egui::Id::unique("hdr3d-right");
                        let right_w: f32 = ui.ctx().data(|d| d.get_temp(id)).unwrap_or(0.0);
                        let fill = (hdr_w - (ui.cursor().min.x - hdr_x0) - right_w).max(4.0);
                        ui.add_space(fill);
                        right_x0 = Some(ui.cursor().min.x);
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
                            // Largeur fixe : les curseurs remplissent la ligne sans élargir le menu
                            ui.set_width(320.0);
                            ui.spacing_mut().item_spacing.y = 7.0;
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
                                // Éclairage tout fait, quand la scène n'a pas ses propres lumières
                                ui.label(egui::RichText::new(crate::i18n::t("éclairage du rendu")).color(p.muted).size(11.0));
                                ui.horizontal(|ui| {
                                    for (k, l, tip) in [(0u8, "vue", "La lumière de la vue (menu éclairage)"), (1, "studio", "Clé, contre-jour et débouchage, sur un fond gris : idéal pour présenter un modèle"), (2, "extérieur", "Un soleil franc et le ciel de l'environnement")] {
                                        if ui.selectable_label(self.trace_set.rig == k, crate::i18n::t(l)).on_hover_text(crate::i18n::t(tip)).clicked() {
                                            self.trace_set.rig = k;
                                            ch = true;
                                        }
                                    }
                                });
                                ch |= ui.checkbox(&mut self.trace_set.backdrop, crate::i18n::t("fond de studio (le sol se fond au loin)")).changed();
                                if self.trace_set.backdrop && self.trace_set.rig == 1 {
                                    ch |= crate::modifiers::row_slider(ui, "clarté du fond", egui::Slider::new(&mut self.trace_set.backdrop_gray, 0.0..=0.6));
                                }
                                ch |= crate::modifiers::row_slider(ui, "contraste", egui::Slider::new(&mut self.trace_set.contrast, -0.5..=1.0));
                                ch |= crate::modifiers::row_slider(ui, "saturation", egui::Slider::new(&mut self.trace_set.saturation, 0.0..=2.0));
                                ch |= crate::modifiers::row_slider(ui, "contours (line art)", egui::Slider::new(&mut self.trace_set.outline, 0.0..=6.0).step_by(0.5));
                                if self.trace_set.outline > 0.0 {
                                    ch |= crate::modifiers::row(ui, "couleur des contours", |ui| ui.color_edit_button_srgb(&mut self.trace_set.outline_color).changed());
                                }
                                ui.separator();
                                ch |= crate::modifiers::row_slider(ui, "rebonds", egui::Slider::new(&mut self.trace_set.bounces, 1..=16).clamping(egui::SliderClamping::Never));
                                ch |= crate::modifiers::row_slider(ui, "échantillons", egui::Slider::new(&mut self.trace_max, 16..=4096).clamping(egui::SliderClamping::Never).logarithmic(true));
                                ch |= ui.checkbox(&mut self.trace_set.floor, crate::i18n::t("sol (ombres et rebonds)")).changed();
                                ch |= ui.add_enabled(self.trace_set.floor, egui::Checkbox::new(&mut self.trace_set.floor_visible, crate::i18n::t("sol visible"))).changed();
                                ch |= crate::modifiers::row_slider(ui, "limite des points brillants", egui::Slider::new(&mut self.trace_set.clamp, 1.0..=100.0).clamping(egui::SliderClamping::Never).logarithmic(true));
                                ch |= ui.checkbox(&mut self.trace_set.denoise, crate::i18n::t("débruitage")).changed();
                                ui.checkbox(&mut self.trace_set.live, crate::i18n::t("tracé de chemins pendant le mouvement")).on_hover_text(crate::i18n::t("Décoché : l'aperçu rapide prend le relais quand on tourne autour de la scène."));
                                ch |= crate::modifiers::row_slider(ui, "flou de profondeur", egui::Slider::new(&mut self.trace_set.aperture, 0.0..=0.2));
                                if self.trace_set.aperture > 0.0 {
                                    let mut f = if self.trace_set.focus > 0.0 { self.trace_set.focus } else { self.cam.dist };
                                    if crate::modifiers::row_slider(ui, "distance nette", egui::Slider::new(&mut f, 0.05..=10.0).clamping(egui::SliderClamping::Never).logarithmic(true)) {
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
                                            if ui.button(format!("{}  {}", crate::lucide::DOWNLOAD, crate::i18n::t("installer le débruiteur IA (~65 Mo)"))).on_hover_text(crate::i18n::t("Intel Open Image Denoise, le débruiteur de Cycles, Arnold et V-Ray. Sans droits admin.")).clicked() {
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
                            if ui.button(format!("{}  {}", crate::lucide::CAMERA, crate::i18n::t("rendre l'image…   F12"))).on_hover_text(crate::i18n::t("Rendu final en tracé de chemins, à la taille choisie, puis enregistrement en PNG")).clicked() {
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
                            if ui.button(format!("{}  {}", crate::lucide::PLUS, crate::i18n::t("nouveau matériau…"))).on_hover_text(crate::i18n::t("crée un matériau en nodes, l'applique et l'ouvre à côté")).clicked() && !self.root.is_empty() {
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
                    // Le calcul en arrière-plan se signale dans la ligne d'infos du bas, pas ici : l'en-tête ne bouge plus
                    Hdr::Busy => {}
                    _ => {}
                }
            }
            if let Some(x0) = right_x0 {
                let w = ui.min_rect().right() - x0;
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("hdr3d-right"), w + 2.0));
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
        // Un calcul en cours (modificateurs, scène) : un mot discret à la fin de la ligne d'infos
        self.stats_text = if self.job.is_some() || self.scene_rx.is_some() { format!("{stats} · {}", crate::i18n::t_mid("calcul…")) } else { stats };
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
                for (f, help) in [("obj", "OBJ : quads et n-gones gardés"), ("fbx", "FBX : Maya, 3ds Max, Blender, Unity, Unreal (quads, UV, matériaux ; une scène garde ses os et l'animation)"), ("glb", "glTF binaire : Blender, Unity, Unreal, web (une scène garde ses os, ses poids et l'animation)"), ("stl", "STL binaire : impression 3D"), ("ply", "PLY : scans, MeshLab")] {
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
                    if ui.button(format!("{}  {}", crate::lucide::FOLDER_OPEN, crate::i18n::t("parcourir…"))).clicked() {
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
                    if ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("annuler"))).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
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
            // Une scène en glTF : ses objets, ses squelettes, les poids et l'animation des os
            if let (Some(sc), "glb" | "fbx", None) = (self.scene.as_ref(), fmt, self.edit.as_ref()) {
                let r = if fmt == "glb" { crate::gltfscene::export(sc, &self.path, &out) } else { crate::fbxscene::export(sc, &self.path, &out) };
                self.capture_msg = Some(match r {
                    Ok((bones, skinned)) if bones > 0 => format!("{} {} ({bones} {}, {skinned} {})", crate::i18n::t("exporté :"), out.to_string_lossy(), crate::i18n::t_mid("os"), crate::i18n::t_mid("modèles liés")),
                    Ok(_) => format!("{} {}", crate::i18n::t("exporté :"), out.to_string_lossy()),
                    Err(e) => format!("{} {e}", crate::i18n::t("export impossible :")),
                });
            } else {
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
            if resp.secondary_clicked() && !ui.input(|i| i.modifiers.shift) {
                if let Some(at) = resp.interact_pointer_pos() {
                    e.open_menu_3d(at);
                }
            }
            e.context_menu(ui.ctx());
        }
        // Scène, mode objet : clic droit sur un objet = il devient l'actif, et son menu s'ouvre (comme Blender)
        if self.edit.is_none() && self.mesh.is_none() && self.scene.is_some() {
            if resp.secondary_clicked() && !ui.input(|i| i.modifiers.shift) {
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
                            ("joindre à l'actif", "Ctrl+J", "join", true),
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
        // ---- Curseur 3D, comme dans Blender : Maj+clic droit le pose sur la surface visée,
        // Maj+S et « . » ouvrent les menus circulaires (curseur, point de pivot), Maj+C le remet à
        // l'origine. Les nouveaux objets apparaissent au curseur.
        if self.edit.is_none() && crate::scene::is_scene(&self.path) {
            if let Some((lo, hi)) = self.scene_frame {
                let longest = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
                let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
                let focal = rect.height().min(rect.width()) * 1.2 * self.cam.zoom;
                let cam = self.cam;
                let to_screen = |w: V3| -> Option<Pos2> {
                    let q = cam.view([(w[0] - c[0]) / longest, (w[1] - c[1]) / longest, (w[2] - c[2]) / longest]);
                    if cam.ortho {
                        Some(pos2(rect.center().x + q[0] * focal / cam.dist, rect.center().y - q[1] * focal / cam.dist))
                    } else {
                        (q[2] < -0.05).then(|| pos2(rect.center().x + q[0] / -q[2] * focal, rect.center().y - q[1] / -q[2] * focal))
                    }
                };
                let typing = ui.ctx().egui_wants_keyboard_input();
                let (shift_rmb, at, shift_s, shift_c, period) = ui.input(|i| {
                    (
                        i.pointer.secondary_pressed() && i.modifiers.shift,
                        i.pointer.hover_pos(),
                        shift_key(i, Key::S),
                        shift_key(i, Key::C),
                        i.key_pressed(Key::Period) && !i.modifiers.any(),
                    )
                });
                let free = resp.hovered() && !typing && self.modal.is_none() && self.mesh.as_ref().is_none_or(|m| m.modal.is_none());
                if free && shift_rmb {
                    if let Some(at) = at.filter(|a| rect.contains(*a)) {
                        let hit = self.prepared.as_ref().and_then(|(_, _, prep)| {
                            let (tri, b) = pick(prep, &cam, rect.width(), rect.height(), at.x - rect.left(), at.y - rect.top())?;
                            let t = prep.tris[tri];
                            let (pa, pb, pc) = (prep.pos[t[0] as usize], prep.pos[t[1] as usize], prep.pos[t[2] as usize]);
                            let n = [0, 1, 2].map(|k| pa[k] * b[0] + pb[k] * b[1] + pc[k] * b[2]);
                            Some([n[0] * longest + c[0], n[1] * longest + c[1], n[2] * longest + c[2]])
                        });
                        // Dans le vide : à la profondeur du curseur actuel, sous la souris
                        self.cursor3d = hit.unwrap_or_else(|| {
                            let cur = self.cursor3d;
                            let q = cam.view([(cur[0] - c[0]) / longest, (cur[1] - c[1]) / longest, (cur[2] - c[2]) / longest]);
                            let depth = if cam.ortho { cam.dist } else { (-q[2]).max(0.05) };
                            let per_px = if cam.ortho { cam.dist / focal } else { depth / focal } * longest;
                            let from = to_screen(cur).unwrap_or(rect.center());
                            let d = at - from;
                            let (right, up) = (cam.unrotate([1.0, 0.0, 0.0]), cam.unrotate([0.0, 1.0, 0.0]));
                            [0, 1, 2].map(|k| cur[k] + (right[k] * d.x - up[k] * d.y) * per_px)
                        });
                        self.modal_click = true;
                    }
                }
                if free && shift_c {
                    self.cursor3d = [0.0; 3];
                    self.cam = Camera { ortho: self.cam.ortho, ..Default::default() };
                }
                if free && shift_s {
                    self.pie = at.map(|a| (0, a));
                }
                if free && period {
                    self.pie = at.map(|a| (1, a));
                }
                self.cursor_screen = to_screen(self.cursor3d);
                // Menus circulaires
                if let Some((kind, center)) = self.pie {
                    let items: &[&str] = if kind == 0 {
                        &["curseur sur la sélection", "sélection sur le curseur", "curseur à l'origine du monde", "curseur sur l'objet actif", "curseur sur la grille", "sélection sur le curseur (garder l'écart)"]
                    } else {
                        &["point médian", "curseur 3D", "origines individuelles", "objet actif"]
                    };
                    let current = (kind == 1).then_some(self.pivot as usize);
                    match pie_menu(ui, egui::Id::unique(("pie3d", kind)), center, items, current, if kind == 0 { Key::S } else { Key::Period }) {
                        PieResult::Open => {}
                        PieResult::Cancel => self.pie = None,
                        PieResult::Chose(i) => {
                            self.pie = None;
                            if kind == 1 {
                                self.pivot = i as u8;
                            } else {
                                self.cursor_action(i);
                            }
                        }
                    }
                }
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
                let focal = rect.height().min(rect.width()) * 1.2 * self.cam.zoom;
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
                let (pivot, cursor) = (self.pivot, self.cursor3d);
                let snap = ui.input(|i| i.modifiers.command);
                let step = 10f32.powf((longest * 0.02).log10().floor());
                let has_parent = self.scene.as_ref().is_some_and(|s| s.parent_of(sel).is_some());
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
                                    // Ctrl : par crans ronds
                                    let mv = if snap { mv.map(|x| (x / step).round() * step) } else { mv };
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
                                    if snap {
                                        turn = (turn / 5.0).round() * 5.0;
                                    }
                                    o.rot[k] = orig.1[k] - turn * if toward[k] >= 0.0 { 1.0 } else { -1.0 };
                                    // Pivot au curseur 3D : la position tourne aussi autour de lui
                                    if pivot == 1 && !has_parent {
                                        let (sn, cs) = (o.rot[k] - orig.1[k]).to_radians().sin_cos();
                                        let r = [0, 1, 2].map(|a| orig.0[a] - cursor[a]);
                                        let r = match k {
                                            0 => [r[0], r[1] * cs - r[2] * sn, r[1] * sn + r[2] * cs],
                                            1 => [r[0] * cs + r[2] * sn, r[1], -r[0] * sn + r[2] * cs],
                                            _ => [r[0] * cs - r[1] * sn, r[0] * sn + r[1] * cs, r[2]],
                                        };
                                        o.pos = [0, 1, 2].map(|a| cursor[a] + r[a]);
                                    }
                                }
                                _ => {
                                    let f = ((cur - center).length() / (from - center).length().max(8.0)).max(0.001);
                                    let f = if snap { ((f * 10.0).round() / 10.0).max(0.1) } else { f };
                                    match axis {
                                        Some(k) => o.scale[k] = orig.2[k] * f,
                                        None => o.scale = [orig.2[0] * f, orig.2[1] * f, orig.2[2] * f],
                                    }
                                    if pivot == 1 && !has_parent {
                                        for a in 0..3 {
                                            if axis.is_none_or(|k| k == a) {
                                                o.pos[a] = cursor[a] + (orig.0[a] - cursor[a]) * f;
                                            }
                                        }
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
                tool_hint(ui, rect, format!("{what}{ax}   ·   X Y Z : {}   ·   {}   ·   {}", crate::i18n::t("imposer un axe"), crate::i18n::t("clic : valider"), crate::i18n::t("Échap : annuler")));
            }
            if let (Some(sel), Some((lo, hi)), Some(scene)) = (self.scene_sel.filter(|_| self.paint3d.is_none()), self.scene_frame, self.scene.as_mut()) {
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
                    let focal = rect.height().min(rect.width()) * 1.2 * self.cam.zoom;
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
                        let focal = rect.height().min(rect.width()) * 1.2 * self.cam.zoom;
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
                                    // Affiché tout de suite sur place ; écrit et reconstruit au relâché
                                    self.mesh_moved = true;
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
        if scene_nav && self.paint3d.is_none() && resp.drag_started_by(egui::PointerButton::Primary) && !mods.alt && self.gizmo_hold.is_none() && self.modal.is_none() && !lkey && !hkey {
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
                    let k = self.cam.dist / (rect.height().min(rect.width()) * 1.2 * self.cam.zoom);
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
                let k = self.cam.dist / (rect.height().min(rect.width()) * 1.2 * self.cam.zoom);
                self.cam.pan[0] += d.x * k;
                self.cam.pan[1] -= d.y * k;
            } else {
                // Tourner autour de la sélection : le point choisi garde sa place à l'écran
                if resp.drag_started() {
                    self.orbit_pivot = if self.orbit_sel { self.selection_center() } else { None };
                }
                let before = self.orbit_pivot.map(|m| self.cam.rotate(m));
                self.cam.yaw -= d.x * 0.008;
                self.cam.pitch = (self.cam.pitch + d.y * 0.008).clamp(-1.57, 1.57);
                if let (Some(m), Some(r0)) = (self.orbit_pivot, before) {
                    let r1 = self.cam.rotate(m);
                    self.cam.pan[0] += r0[0] - r1[0];
                    self.cam.pan[1] += r0[1] - r1[1];
                    self.cam.dist -= r0[2] - r1[2];
                }
            }
        }
        if resp.hovered() {
            let busy = self.mesh.as_ref().is_some_and(|m| m.op.is_some() || m.loopcut.is_some() || (m.prop && m.modal.is_some()));
            let scroll = if busy { 0.0 } else { ui.input(|i| i.smooth_scroll_delta.y + i.zoom_delta().ln() * 300.0) };
            if scroll != 0.0 {
                self.cam.dist = (self.cam.dist * (-scroll * 0.0015).exp()).clamp(0.2, 50.0);
            }
            let editing_mesh = self.mesh.is_some();
            let keys = ui.input(|i| [Key::Num1, Key::Num3, Key::Num7, Key::Num5, Key::Home, Key::Period].map(|k| i.key_pressed(k) && !i.modifiers.command && !(k == Key::Period && crate::scene::is_scene(&self.path)) && !(editing_mesh && matches!(k, Key::Num1 | Key::Num3))));
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
            if keys.iter().any(|k| *k) {
                self.look_through = false;
            }
            // 0 : regarder par la caméra de la scène (comme Blender), 0 encore pour revenir
            if crate::scene::is_scene(&self.path) && ui.input(|i| i.key_pressed(Key::Num0) && !i.modifiers.any()) {
                self.look_through = !self.look_through;
                if !self.look_through {
                    self.cam = Camera { ortho: self.cam.ortho, ..Default::default() };
                }
            }
            if crate::scene::is_scene(&self.path) && ui.input(|i| shift_key(i, Key::F)) {
                self.frame_selection();
            }
        }
        if self.look_through {
            if dragging {
                self.look_through = false;
            } else if let (Some(scene), Some(fr)) = (self.scene.as_ref(), self.scene_frame) {
                match scene.active_camera() {
                    Some(k) => {
                        self.cam = camera_view(scene, k, fr, self.cam);
                        // Le champ de la caméra vaut pour son cadre, pas pour toute la vue
                        let (_, _, fw, fh) = crate::matchmove::camera_frame(rect.width(), rect.height(), scene.frame_aspect());
                        self.cam.zoom *= fw.min(fh) / rect.width().min(rect.height()).max(1.0);
                    }
                    None => {
                        self.look_through = false;
                        self.capture_msg = Some(crate::i18n::t("La scène n'a pas de caméra (Ajouter › Caméra)"));
                    }
                }
            }
        }
        if !self.look_through {
            // Le roulis et la focale appartiennent à la caméra de la scène, pas à la vue libre
            self.cam.roll = 0.0;
            self.cam.zoom = 1.0;
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
        // Vue par la caméra : l'image de la vidéo de fond à cette image de l'animation, une fois décodée
        let backdrop = self.backdrop_frame(ui.ctx());
        let look_key = format!("{:?}|{refs_key}|{:?}", look.key(), backdrop.as_ref().map(|b| b.1));
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
            if let Some((b, k, aspect)) = &backdrop {
                let fr = crate::matchmove::camera_frame(frame.w as f32, frame.h as f32, *aspect);
                crate::matchmove::composite(&mut frame.rgba, &frame.ids, frame.w, frame.h, fr, &b.px[*k], b.w, b.h);
            }
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
                        acc.contrast = self.trace_set.contrast;
                        acc.saturation = self.trace_set.saturation;
                        acc.outline = self.trace_set.outline;
                        acc.outline_color = self.trace_set.outline_color;
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
                let focal = rect.height().min(rect.width()) * 1.2 * self.cam.zoom;
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
        if self.paint3d.is_some() {
            self.paint_ui(ui, rect, &resp);
        }
        // Le curseur 3D : un anneau rouge et blanc et sa croix, par-dessus la vue
        if let Some(s) = self.cursor_screen.take().filter(|s| rect.contains(*s)) {
            for k in 0..8 {
                let (a0, a1) = (k as f32 * std::f32::consts::FRAC_PI_4, (k + 1) as f32 * std::f32::consts::FRAC_PI_4);
                let col = if k % 2 == 0 { Color32::from_rgb(230, 60, 60) } else { Color32::WHITE };
                let pts: Vec<Pos2> = (0..=6).map(|j| {
                    let a = a0 + (a1 - a0) * j as f32 / 6.0;
                    s + egui::vec2(a.cos(), a.sin()) * 10.0
                }).collect();
                painter.add(egui::Shape::line(pts, Stroke::new(2.0, col)));
            }
            for d in [egui::vec2(1.0, 0.0), egui::vec2(-1.0, 0.0), egui::vec2(0.0, 1.0), egui::vec2(0.0, -1.0)] {
                painter.line_segment([s + d * 13.0, s + d * 20.0], Stroke::new(1.0, Color32::from_gray(20)));
            }
        }
        // Raccourcis de la vue 3D d'une scène (Ctrl+B, Ctrl+J, Ctrl+R…) avant ceux de l'app
        MESH_KEYS.store(resp.hovered() && crate::scene::is_scene(&self.path), std::sync::atomic::Ordering::Relaxed);
        self.mesh_edit_ui(ui, rect, &resp);
        // Repères des lumières et des caméras de la scène
        if let (Some(scene), Some((lo, hi)), true) = (self.scene.as_ref(), self.scene_frame, self.edit.is_none()) {
            let longest = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
            let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
            let focal = rect.height().min(rect.width()) * 1.2 * self.cam.zoom;
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
            // Vue par la caméra : le cadre 16:9 de l'export d'animation, le reste assombri
            if self.look_through {
                let (_, _, fw, fh) = crate::matchmove::camera_frame(rect.width(), rect.height(), self.scene.as_ref().map(|s| s.frame_aspect()).unwrap_or(16.0 / 9.0));
                let f = egui::Rect::from_center_size(rect.center(), egui::vec2(fw, fh));
                let shade = Color32::from_black_alpha(150);
                for r in [egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, f.min.y)), egui::Rect::from_min_max(egui::pos2(rect.min.x, f.max.y), rect.max), egui::Rect::from_min_max(egui::pos2(rect.min.x, f.min.y), egui::pos2(f.min.x, f.max.y)), egui::Rect::from_min_max(egui::pos2(f.max.x, f.min.y), egui::pos2(rect.max.x, f.max.y))] {
                    over.rect_filled(r, 0.0, shade);
                }
                over.rect_stroke(f, 0.0, Stroke::new(1.0, p.text.gamma_multiply(0.6)), egui::StrokeKind::Outside);
                over.text(f.right_top() + egui::vec2(-8.0, 6.0), egui::Align2::RIGHT_TOP, format!("{}  {}", crate::lucide::CAMERA, crate::i18n::t("Caméra · 0 ou tourner pour sortir")), egui::FontId::proportional(11.5), p.text.gamma_multiply(0.8));
            }
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
            // Trajectoire de l'objet animé choisi : son chemin dans l'espace, ses clés, l'image actuelle
            if let Some(k) = self.scene_sel.filter(|&k| k < scene.objects.len()) {
                // L'objet ou un de ses parents bouge : la signature réunit leurs clés
                let mut chain = vec![k];
                while let Some(pk) = scene.parent_of(*chain.last().unwrap()).filter(|p| !chain.contains(p)) {
                    chain.push(pk);
                }
                let animated = chain.iter().any(|&i| scene.objects[i].keys.len() >= 2);
                if animated {
                    let sig = format!("{k}|{}|{}|{}", scene.start, scene.end, chain.iter().map(|&i| serde_json::to_string(&scene.objects[i].keys).unwrap_or_default() + &format!("{:?}", scene.objects[i].parent)).collect::<String>());
                    if self.motion_path.as_ref().is_none_or(|m| m.0 != sig) {
                        let mut sc = scene.clone();
                        let (a, b) = (scene.start, scene.end.max(scene.start + 1));
                        let step = (((b - a) as f32) / 400.0).max(0.25);
                        let mut pts = vec![];
                        let mut f = a as f32;
                        while f <= b as f32 + 1e-3 {
                            sc.apply_frame(f);
                            pts.push(sc.world(k, [0.0; 3]));
                            f += step;
                        }
                        let mut frames: Vec<i32> = chain.iter().flat_map(|&i| scene.objects[i].keys.iter().map(|x| x.frame)).collect();
                        frames.sort();
                        frames.dedup();
                        let keys = frames
                            .into_iter()
                            .map(|kf| {
                                sc.apply_frame(kf as f32);
                                (kf, sc.world(k, [0.0; 3]))
                            })
                            .collect();
                        self.motion_path = Some((sig.clone(), pts, keys));
                    }
                    if let Some((_, pts, keys)) = &self.motion_path {
                        let line: Vec<Pos2> = pts.iter().filter_map(|v| project(*v)).collect();
                        if line.len() > 1 {
                            over.add(egui::Shape::line(line.clone(), Stroke::new(3.0, Color32::from_black_alpha(90))));
                            over.add(egui::Shape::line(line, Stroke::new(1.5, p.warning.gamma_multiply(0.85))));
                        }
                        // Petits points : une image sur cinq, pour lire la vitesse (serrés = lent)
                        let step = ((scene.end - scene.start).max(1) as f32 / (pts.len().max(2) - 1) as f32).max(1e-3);
                        let every = (5.0 / step).round().max(1.0) as usize;
                        for v in pts.iter().step_by(every) {
                            if let Some(q) = project(*v) {
                                over.circle_filled(q, 1.6, p.warning.gamma_multiply(0.7));
                            }
                        }
                        for (kf, v) in keys {
                            if let Some(q) = project(*v) {
                                let col = if (*kf as f32 - self.anim_frame).abs() < 0.5 { p.accent } else { p.warning };
                                crate::keys::draw(&over, q, 4.5, None, col, true, false);
                                over.text(q + egui::vec2(7.0, -7.0), egui::Align2::LEFT_BOTTOM, kf.to_string(), egui::FontId::proportional(10.0), p.muted);
                            }
                        }
                    }
                    // Pelure d'oignon : deux clés de chaque côté, en fil de fer qui s'efface avec la distance
                    if self.onion {
                        if let (Some(built), Some(prep), State::Ready(m), Some(&(_, t0, t1))) = (&self.scene_built, &self.prepared, &self.state, self.scene_ranges.iter().find(|r| r.0 == k)) {
                            let now = self.anim_frame;
                            let frames: Vec<i32> = self.motion_path.as_ref().map(|mp| mp.2.iter().map(|x| x.0).collect()).unwrap_or_default();
                            let before: Vec<i32> = frames.iter().copied().filter(|&f| (f as f32) < now - 0.5).rev().take(2).collect();
                            let after: Vec<i32> = frames.iter().copied().filter(|&f| (f as f32) > now + 0.5).take(2).collect();
                            let gsig = format!("{sig}|{before:?}|{after:?}|{:p}", std::sync::Arc::as_ptr(&prep.2));
                            if self.ghosts.as_ref().is_none_or(|g| g.0 != gsig) {
                                self.ghosts = Some((gsig, ghost_edges(built, scene, k, &prep.2, m, (t0, t1), &before, &after)));
                            }
                            if let Some((_, gs)) = &self.ghosts {
                                for (side, segs) in gs {
                                    let far = side.unsigned_abs() as f32;
                                    let base = if *side < 0 { Color32::from_rgb(235, 90, 90) } else { Color32::from_rgb(110, 210, 120) };
                                    let st = Stroke::new(1.0, base.gamma_multiply(0.55 / far));
                                    for [a, b] in segs {
                                        if let (Some(qa), Some(qb)) = (project(*a), project(*b)) {
                                            over.line_segment([qa, qb], st);
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    self.motion_path = None;
                }
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
                if kind == crate::scene::Kind::Bone {
                    // Un os : un losange allongé de la tête au bout, comme dans Blender
                    let on = self.scene_sel == Some(k) || self.scene_multi.contains(&k);
                    let col = if on { p.accent } else if !o.ik.is_empty() { Color32::from_rgb(230, 170, 60) } else { Color32::from_rgb(200, 205, 215) };
                    let l = o.length;
                    let r = l * 0.1;
                    let tip = project(scene.world(k, [0.0, l, 0.0]));
                    let ring: Vec<Option<Pos2>> = [[r, r, 0.0], [0.0, r, r], [-r, r, 0.0], [0.0, r, -r]].iter().map(|v| project(scene.world(k, *v))).collect();
                    let st = Stroke::new(if on { 2.0 } else { 1.3 }, col);
                    for i in 0..4 {
                        if let Some(q) = ring[i] {
                            over.line_segment([at, q], st);
                            if let Some(t) = tip {
                                over.line_segment([q, t], st);
                            }
                            if let Some(q2) = ring[(i + 1) % 4] {
                                over.line_segment([q, q2], st);
                            }
                        }
                    }
                    over.circle_stroke(at, 3.0, st);
                    if let Some(t) = tip {
                        over.circle_stroke(t, 2.5, st);
                        // Un clic près de l'os le choisit
                        if resp.clicked() {
                            if let Some(m) = resp.interact_pointer_pos() {
                                let ab = t - at;
                                let u = ((m - at).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                                if (m - (at + ab * u)).length() < 7.0 {
                                    marker_pick = Some(k);
                                }
                            }
                        }
                    }
                    continue;
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
            let focal = rect.height().min(rect.width()) * 1.2 * self.cam.zoom;
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
            let focal = rect.height().min(rect.width()) * 1.2 * self.cam.zoom;
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
            let g = painter.layout_no_wrap(crate::i18n::t(if self.sculpt.is_some() { "SCULPTURE" } else { "MODE ÉDITION" }), egui::FontId::monospace(11.0), p.on_accent);
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
mod tests_weld {
    use super::*;
    #[test]
    fn fusionne_les_coutures() {
        // Deux triangles d'un carré, exportés avec leurs sommets dédoublés le long de la diagonale
        let p = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
        let (pos, tris, keep) = weld(&p, &[[0, 1, 2], [3, 4, 5]]);
        assert_eq!(pos.len(), 4);
        assert_eq!(tris.len(), 2);
        assert!(keep.iter().all(|k| *k));
        // Un triangle devenu plat disparaît
        let (_, t, keep) = weld(&[[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]], &[[0, 1, 2]]);
        assert!(t.is_empty() && !keep[0]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perspective_juste() {
        // Un segment qui fuit : de (0, 0, -1) à (1, 0, -4), vu par un sténopé (x écran = x / profondeur)
        let (a, b) = ([0.0f32, 0.0, -1.0], [1.0f32, 0.0, -4.0]);
        let sx = |p: [f32; 3]| p[0] / -p[2];
        // Le milieu à l'écran n'est pas le milieu dans l'espace : les poids corrigés retrouvent le bon point
        let mid = (sx(a) + sx(b)) / 2.0;
        let (_, w) = persp_bary([0.5, 0.5, 0.0], [1.0 / -a[2], 1.0 / -b[2], 1.0]);
        let p = [a[0] * w[0] + b[0] * w[1], 0.0, a[2] * w[0] + b[2] * w[1]];
        assert!((sx(p) - mid).abs() < 1e-5, "le point retrouvé retombe sous le pixel");
        assert!((w[0] - 0.8).abs() < 1e-5, "et non 0,5 (affine) : {w:?}");
    }

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
    fn vue_par_une_camera_penchee() {
        // Une caméra tournée sur ses trois axes, avec son propre champ, où que soit le centre de la scène
        for (bounds, pos) in [(([-4.0f32, -1.0, -6.0], [4.0f32, 3.0, 2.0]), [1.0f32, 2.0, 5.0]), (([10.0, 0.0, 10.0], [30.0, 5.0, 40.0]), [1.0, 2.0, 5.0])] {
            let mut sc = crate::scene::Scene::default();
            let k = sc.add_role("caméra", "camera");
            (sc.objects[k].pos, sc.objects[k].rot, sc.objects[k].fov) = (pos, [-20.0, 35.0, 25.0], 30.0);
            let cam = camera_view(&sc, k, bounds, Camera::default());
            assert!((cam.zoom - zoom_of_fov(30.0)).abs() < 1e-6 && cam.zoom > 1.0);
            let (lo, hi) = bounds;
            let longest = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]);
            let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
            let to_view = |w: V3| [(w[0] - c[0]) / longest, (w[1] - c[1]) / longest, (w[2] - c[2]) / longest];
            // Un point droit devant la caméra (son axe −Z) tombe au centre de l'image
            let m = crate::scene::rot_matrix(sc.objects[k].rot);
            let axis = |i: usize| [m[0][i], m[1][i], m[2][i]];
            let (right, up, back) = (axis(0), axis(1), axis(2));
            let ahead = [pos[0] - back[0] * 3.0, pos[1] - back[1] * 3.0, pos[2] - back[2] * 3.0];
            let q = project_point(&cam, 400.0, 300.0, to_view(ahead)).unwrap();
            assert!((q[0] - 200.0).abs() < 0.01 && (q[1] - 150.0).abs() < 0.01, "{q:?}");
            // Un point vers le haut de la caméra monte tout droit à l'image, un point à sa droite part à droite : le roulis est respecté
            let q = project_point(&cam, 400.0, 300.0, to_view([ahead[0] + up[0] * 0.3, ahead[1] + up[1] * 0.3, ahead[2] + up[2] * 0.3])).unwrap();
            assert!((q[0] - 200.0).abs() < 0.01 && q[1] < 140.0, "{q:?}");
            // 0,3 de côté à 3 de distance, champ de 30° sur 300 pixels : 0,1 / tan(15°) × 150
            assert!((150.0 - q[1] - 0.1 / 15f32.to_radians().tan() * 150.0).abs() < 0.05, "{q:?}");
            let q = project_point(&cam, 400.0, 300.0, to_view([ahead[0] + right[0] * 0.3, ahead[1] + right[1] * 0.3, ahead[2] + right[2] * 0.3])).unwrap();
            assert!((q[1] - 150.0).abs() < 0.01 && q[0] > 210.0, "{q:?}");
            // rotate et unrotate restent inverses avec le roulis
            let v = [0.3, -0.7, 0.2];
            let back = cam.unrotate(cam.rotate(v));
            assert!((0..3).all(|i| (back[i] - v[i]).abs() < 1e-5));
        }
        assert_eq!(zoom_of_fov(0.0), 1.0);
        assert_eq!(zoom_of_fov(f32::NAN), 1.0);
        assert!((fov_of_focal(BASE_FOCAL) - 45.24).abs() < 0.01);
        // La caméra active : celle qu'on a nommée, sinon la première ; renommer la suit
        let mut sc = crate::scene::Scene::starter();
        let first = sc.active_camera().unwrap();
        let second = sc.add_role("caméra suivie", "camera");
        assert_eq!(sc.active_camera(), Some(first));
        sc.camera = "caméra suivie".into();
        assert_eq!(sc.active_camera(), Some(second));
        sc.rename(second, "plan 1");
        assert_eq!((sc.active_camera(), sc.camera.as_str()), (Some(second), "plan 1"));
        sc.camera = "absente".into();
        assert_eq!(sc.active_camera(), Some(first));
    }

    #[test]
    fn formats_et_simplification() {
        let obj = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1/1/1 2/2/2 3/3/3 4/4/4\nf -4 -3 -2\n";
        let m = load_obj(obj.as_bytes()).unwrap();
        assert_eq!(m.total_tris, 3, "un quad = 2 triangles, plus un triangle en indices négatifs");
        assert_eq!(m.size(), [1.0, 1.0, 0.0]);
        // Fichier abîmé : une face cite un sommet absent, un sommet est infini ; ni panique ni bornes fausses
        let bad = load_obj("v 0 0 0\nv 1 0 0\nv 1 1 0\nv inf 0 nan\nf 1 2 3\nf 1 2 99\n".as_bytes()).unwrap();
        assert_eq!(bad.total_tris, 1);
        assert!(bad.size().iter().all(|x| x.is_finite()));
        assert!(load_obj("f 1 2 3\n".as_bytes()).is_err());

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

#[cfg(test)]
mod thumb_tests {
    #[test]
    fn miniature_fond_transparent() {
        let s = super::preview_sphere();
        let look = super::Look { material: super::Material::Gray, ..Default::default() };
        let f = super::render(&s, &super::Camera { yaw: 0.4, pitch: 0.25, dist: 2.0, ..Default::default() }, &look, 48, 48);
        let a = |x: usize, y: usize| f.rgba[(y * 48 + x) * 4 + 3];
                assert_eq!(a(24, 24), 255);
        assert_eq!(a(1, 1), 0);
    }
}

#[cfg(test)]
mod pointage {
    use super::*;

    /// L'arbre de rayons et le parcours complet désignent le même triangle, avec les mêmes poids, en
    /// perspective comme en orthographique ; et un maillage vide ou des indices faux ne paniquent pas.
    #[test]
    fn arbre_et_parcours_d_accord() {
        let s = preview_sphere();
        for ortho in [false, true] {
            let cam = Camera { yaw: 0.7, pitch: 0.3, dist: 2.4, pan: [0.1, -0.05], ortho, roll: 0.35, zoom: 1.3 };
            let pr = Proj::new(&cam, 320.0, 240.0);
            let mut seen = 0;
            for k in 0..60 {
                let (x, y) = (40.3 + (k % 10) as f32 * 24.7, 30.6 + (k / 10) as f32 * 31.1);
                let a = pick_scan(&s, &pr, x, y);
                s.bvh.get_or_init(|| crate::bvh::Bvh::new(&s.pos, &s.tris));
                let b = pick(&s, &cam, 320.0, 240.0, x, y);
                assert_eq!(a.map(|h| h.0), b.map(|h| h.0), "pixel {x} {y}");
                if let (Some(a), Some(b)) = (a, b) {
                    seen += 1;
                    assert!((0..3).all(|i| (a.1[i] - b.1[i]).abs() < 2e-3), "{:?} {:?}", a.1, b.1);
                    // Le point retrouvé avec ces poids se projette bien sous le pixel
                    let t = s.triangle(a.0);
                    let p = [0, 1, 2].map(|c| (0..3).map(|i| s.vertex(t[i] as usize)[c] * a.1[i]).sum::<f32>());
                    let q = project_point(&cam, 320.0, 240.0, p).unwrap();
                    assert!((q[0] - x).abs() < 0.05 && (q[1] - y).abs() < 0.05, "{q:?} {x} {y}");
                }
            }
            assert!(seen > 10);
        }
        let n = 1;
        let empty = Prepared::from_mesh_fixed(crate::modifiers::Mesh { pos: vec![], tris: vec![], colors: vec![], edges: vec![] }, 0.0);
        assert!(pick(&empty, &Camera::default(), 100.0, 100.0, 50.0, 50.0).is_none());
        let broken = Prepared::from_mesh_fixed(crate::modifiers::Mesh { pos: vec![[0.0; 3], [f32::NAN, 0.0, 0.0], [0.0, 1.0, 0.0]], tris: vec![[0, 1, 2]], colors: vec![Color32::WHITE; n], edges: vec![7; n] }, 0.0);
        assert!(pick(&broken, &Camera::default(), 100.0, 100.0, 50.0, 50.0).is_none());
        let f = render(&broken, &Camera::default(), &Look::default(), 32, 32);
        assert_eq!(f.rgba.len(), 32 * 32 * 4);
    }

    /// Le rendu range les triangles par bande : le résultat est le même pour une image plus haute
    /// qu'une bande, et un triangle à cheval sur plusieurs bandes est dessiné dans chacune.
    #[test]
    fn bandes_du_rendu() {
        let n = 1;
        let tri = Prepared::from_mesh_fixed(crate::modifiers::Mesh { pos: vec![[-0.4, -0.45, 0.0], [0.4, -0.45, 0.0], [0.0, 0.45, 0.0]], tris: vec![[0, 1, 2]], colors: vec![Color32::WHITE; n], edges: vec![7; n] }, -0.5);
        let cam = Camera { yaw: 0.0, pitch: 0.0, dist: 2.0, ..Default::default() };
        let f = render(&tri, &cam, &Look::default(), 160, 100);
        // Une colonne au milieu : couverte sans trou du haut au bas du triangle (plusieurs bandes de 16 lignes)
        let col: Vec<bool> = (0..100).map(|y| f.ids[y * 160 + 80] == 0).collect();
        let (first, last) = (col.iter().position(|c| *c).unwrap(), col.iter().rposition(|c| *c).unwrap());
        assert!(last - first > 40, "{first} {last}");
        assert!(col[first..=last].iter().all(|c| *c));
        // Hauteur qui n'est pas un multiple de la bande, et image minuscule
        assert_eq!(render(&tri, &cam, &Look::default(), 7, 5).ids.len(), 35);
        assert_eq!(render(&tri, &cam, &Look::default(), 1, 1).ids.len(), 1);
    }
}

#[cfg(test)]
mod banc {
    use super::*;

    /// Temps du rendu et du pointage sur un maillage dense :
    /// `cargo test --release --offline banc_rendu -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn banc_rendu_et_pointage() {
        let obj = "v -1 -1 -1\nv 1 -1 -1\nv 1 1 -1\nv -1 1 -1\nv -1 -1 1\nv 1 -1 1\nv 1 1 1\nv -1 1 1\nf 1 4 3 2\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n";
        let m = load_obj(obj.as_bytes()).unwrap();
        for levels in [5u8, 8] {
            let pos = m.normalized(false);
            let (mut pos, mut tris, mut colors, mut edges) = catmull_clark(&pos, &m.tris, &m.colors, &m.edges, levels.min(5), false);
            if levels > 5 {
                (pos, tris, colors, edges) = catmull_clark(&pos, &tris, &colors, &edges, levels - 5, false);
            }
            let p = Prepared::from_mesh_fixed(crate::modifiers::Mesh { pos, tris, colors, edges }, -0.5);
            let cam = Camera::default();
            let pk = |f: &dyn Fn(f32) -> bool| {
                let t = std::time::Instant::now();
                let hits: u32 = (0..50).map(|i| f(400.0 + i as f32 * 9.0) as u32).sum();
                (t.elapsed().as_secs_f64() * 1000.0 / 50.0, hits)
            };
            let pr = Proj::new(&cam, 1280.0, 720.0);
            let (scan, _) = pk(&|x| pick_scan(&p, &pr, x, 360.0).is_some());
            let t = std::time::Instant::now();
            p.bvh.get_or_init(|| crate::bvh::Bvh::new(&p.pos, &p.tris));
            let build = t.elapsed().as_secs_f64() * 1000.0;
            let (tree, _) = pk(&|x| pick(&p, &cam, 1280.0, 720.0, x, 360.0).is_some());
            println!("BANC {} triangles : pointage par parcours {scan:.2} ms, par l'arbre {tree:.4} ms (arbre construit en {build:.0} ms)", p.tri_count());
            for (name, look) in [("simple", Look::default()), ("ombres", Look { shadows: true, ..Default::default() })] {
                let t = std::time::Instant::now();
                let n = 8;
                let sum: u64 = (0..n).map(|_| render(&p, &cam, &look, 1280, 720).rgba[1000] as u64).sum();
                println!("BANC {} triangles, {name} : rendu {:.1} ms ({sum})", p.tri_count(), t.elapsed().as_secs_f64() * 1000.0 / n as f64);
            }
        }
    }
}
