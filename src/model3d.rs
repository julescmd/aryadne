//! Visionneuse 3D : OBJ, FBX (binaire), glTF / GLB et STL. Rendu logiciel avec egui.
//!
//! Les modèles très lourds sont simplifiés par regroupement de sommets (grille 3D) :
//! la forme reste complète, avec moins de triangles, pour rester fluide.
//! `thumbnail` rend une image du modèle (z-buffer) pour les boards et les refs.
use crate::theme;
use eframe::egui::{self, epaint::Vertex, pos2, vec2, Color32, Mesh, Pos2, Sense, Shape, Stroke};
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

// ---------- rendu ----------

struct Camera {
    yaw: f32,
    pitch: f32,
    dist: f32,
}

impl Camera {
    fn view(&self, v: V3) -> V3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let x = v[0] * cy + v[2] * sy;
        let z = -v[0] * sy + v[2] * cy;
        let y = v[1] * cp - z * sp;
        let z = v[1] * sp + z * cp;
        [x, y, z - self.dist]
    }
}

const LIGHT: V3 = [0.4, 0.8, 0.6];

fn shade(col: Color32, a: V3, b: V3, c: V3) -> Color32 {
    let n = norm(cross(sub(b, a), sub(c, a)));
    // Faces vues de dos : éclairées comme si elles étaient retournées (maillages ouverts).
    let nn = if dot(n, norm([-a[0], -a[1], -a[2]])) < 0.0 { [-n[0], -n[1], -n[2]] } else { n };
    let s = 0.32 + 0.68 * dot(nn, norm(LIGHT)).max(0.0);
    Color32::from_rgb((col.r() as f32 * s) as u8, (col.g() as f32 * s) as u8, (col.b() as f32 * s) as u8)
}

/// Image du modèle (z-buffer) pour les miniatures des boards et des refs.
pub fn thumbnail(model: &Model, w: u32, h: u32, bg: [u8; 4]) -> image::RgbaImage {
    let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba(bg));
    let mut depth = vec![f32::MIN; (w * h) as usize];
    let cam = Camera { yaw: 0.6, pitch: 0.35, dist: 2.0 };
    let pts: Vec<V3> = model.normalized(model.z_up).iter().map(|v| cam.view(*v)).collect();
    let focal = w.min(h) as f32 * 1.15;
    let proj = |v: V3| -> [f32; 3] { [w as f32 / 2.0 + v[0] / -v[2] * focal, h as f32 / 2.0 - v[1] / -v[2] * focal, v[2]] };
    for (t, col) in model.tris.iter().zip(&model.colors) {
        let (a, b, c) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
        if a[2] > -0.05 || b[2] > -0.05 || c[2] > -0.05 {
            continue;
        }
        let col = shade(*col, a, b, c);
        let (pa, pb, pc) = (proj(a), proj(b), proj(c));
        let area = (pb[0] - pa[0]) * (pc[1] - pa[1]) - (pc[0] - pa[0]) * (pb[1] - pa[1]);
        if area.abs() < 1e-6 {
            continue;
        }
        let minx = pa[0].min(pb[0]).min(pc[0]).floor().max(0.0) as i32;
        let maxx = pa[0].max(pb[0]).max(pc[0]).ceil().min(w as f32 - 1.0) as i32;
        let miny = pa[1].min(pb[1]).min(pc[1]).floor().max(0.0) as i32;
        let maxy = pa[1].max(pb[1]).max(pc[1]).ceil().min(h as f32 - 1.0) as i32;
        for y in miny..=maxy {
            for x in minx..=maxx {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((pb[0] - px) * (pc[1] - py) - (pc[0] - px) * (pb[1] - py)) / area;
                let w1 = ((pc[0] - px) * (pa[1] - py) - (pa[0] - px) * (pc[1] - py)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * pa[2] + w1 * pb[2] + w2 * pc[2];
                let k = (y as u32 * w + x as u32) as usize;
                if z > depth[k] {
                    depth[k] = z;
                    img.put_pixel(x as u32, y as u32, image::Rgba([col.r(), col.g(), col.b(), 255]));
                }
            }
        }
    }
    img
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
    pan: egui::Vec2,
    pub wireframe: bool,
    pub z_up: Option<bool>,
}

impl Viewer {
    pub fn open(path: std::path::PathBuf, ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(load(&path));
            ctx.request_repaint();
        });
        Self { state: State::Loading(rx), cam: Camera { yaw: 0.6, pitch: 0.35, dist: 2.2 }, pan: egui::Vec2::ZERO, wireframe: false, z_up: None }
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

        // Barre : réglages à droite, statistiques tronquées dans l'espace restant.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("recentrer").clicked() {
                    self.cam = Camera { yaw: 0.6, pitch: 0.35, dist: 2.2 };
                    self.pan = egui::Vec2::ZERO;
                }
                let mut z = z_up;
                if ui.checkbox(&mut z, "Z en haut").changed() {
                    self.z_up = Some(z);
                }
                ui.checkbox(&mut self.wireframe, "filaire");
                let s = model.size();
                let simplified = if model.tris.len() < model.total_tris { format!(" · simplifié à {} pour l'affichage", fmt_n(model.tris.len())) } else { String::new() };
                let text = format!("{} triangles · {} sommets · {:.2} × {:.2} × {:.2}{simplified}", fmt_n(model.total_tris), fmt_n(model.total_vertices), s[0], s[1], s[2]);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(text).color(p.muted).size(11.5)).truncate());
                });
            });
        });

        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, p.panel);
        if resp.dragged_by(egui::PointerButton::Primary) {
            let d = resp.drag_delta();
            self.cam.yaw -= d.x * 0.01;
            self.cam.pitch = (self.cam.pitch + d.y * 0.01).clamp(-1.55, 1.55);
        }
        if resp.dragged_by(egui::PointerButton::Secondary) || resp.dragged_by(egui::PointerButton::Middle) {
            self.pan += resp.drag_delta();
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            self.cam.dist = (self.cam.dist * (-scroll * 0.002).exp()).clamp(0.3, 20.0);
        }

        let focal = rect.height().min(rect.width()) * 1.2;
        let center = rect.center() + self.pan;
        let project = |v: V3| -> Option<Pos2> { (v[2] < -0.05).then(|| pos2(center.x + v[0] / -v[2] * focal, center.y - v[1] / -v[2] * focal)) };

        // Sol
        let norm_pts = model.normalized(z_up);
        let floor = norm_pts.iter().map(|v| v[1]).fold(f32::MAX, f32::min);
        for k in -5..=5 {
            let t = k as f32 * 0.2;
            for (a, b) in [([t, floor, -1.0], [t, floor, 1.0]), ([-1.0, floor, t], [1.0, floor, t])] {
                if let (Some(pa), Some(pb)) = (project(self.cam.view(a)), project(self.cam.view(b))) {
                    painter.line_segment([pa, pb], Stroke::new(1.0, p.line));
                }
            }
        }

        let view: Vec<V3> = norm_pts.iter().map(|v| self.cam.view(*v)).collect();
        let screen: Vec<Option<Pos2>> = view.iter().map(|v| project(*v)).collect();
        let mut faces: Vec<(f32, [Pos2; 3], Color32)> = Vec::with_capacity(model.tris.len());
        for (t, col) in model.tris.iter().zip(&model.colors) {
            let (i, j, k) = (t[0] as usize, t[1] as usize, t[2] as usize);
            let (Some(pa), Some(pb), Some(pc)) = (screen[i], screen[j], screen[k]) else { continue };
            faces.push(((view[i][2] + view[j][2] + view[k][2]) / 3.0, [pa, pb, pc], shade(*col, view[i], view[j], view[k])));
        }
        faces.sort_by(|x, y| x.0.total_cmp(&y.0));
        if self.wireframe {
            for (_, pts, _) in &faces {
                painter.add(Shape::closed_line(pts.to_vec(), Stroke::new(0.5, p.text.linear_multiply(0.5))));
            }
        } else {
            for chunk in faces.chunks(20_000) {
                let mut mesh = Mesh::default();
                for (_, pts, col) in chunk {
                    let base = mesh.vertices.len() as u32;
                    for pt in pts {
                        mesh.vertices.push(Vertex { pos: *pt, uv: egui::epaint::WHITE_UV, color: *col });
                    }
                    mesh.indices.extend([base, base + 1, base + 2]);
                }
                painter.add(Shape::mesh(mesh));
            }
        }
        painter.text(rect.left_bottom() + vec2(12.0, -10.0), egui::Align2::LEFT_BOTTOM, "glisser : tourner · clic droit : déplacer · molette : zoom", egui::FontId::monospace(11.0), p.muted);
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
