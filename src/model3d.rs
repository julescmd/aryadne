//! Visionneuse 3D : OBJ, glTF / GLB et STL. Rendu logiciel (triangles ombrés triés
//! par profondeur) directement avec egui, sans dépendance graphique supplémentaire.
use crate::theme;
use eframe::egui::{self, epaint::Vertex, pos2, vec2, Color32, Mesh, Pos2, Rect, Sense, Shape, Stroke};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver};

pub const EXT: [&str; 4] = ["obj", "glb", "gltf", "stl"];
/// Au-delà, on n'affiche qu'une partie des triangles pour rester fluide.
const MAX_TRIS: usize = 120_000;

pub fn is_model(rel: &str) -> bool {
    let e = rel.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    EXT.contains(&e.as_str())
}

type V3 = [f32; 3];

pub struct Model {
    pub tris: Vec<[V3; 3]>,
    pub colors: Vec<Color32>,
    pub min: V3,
    pub max: V3,
    pub total_tris: usize,
    pub vertices: usize,
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
    fn new(tris: Vec<[V3; 3]>, colors: Vec<Color32>, vertices: usize) -> Result<Self, String> {
        if tris.is_empty() {
            return Err("Aucun triangle dans ce fichier.".into());
        }
        let total = tris.len();
        let (tris, colors) = if total > MAX_TRIS {
            let step = total.div_ceil(MAX_TRIS);
            (tris.into_iter().step_by(step).collect(), colors.into_iter().step_by(step).collect())
        } else {
            (tris, colors)
        };
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for t in &tris {
            for v in t {
                for k in 0..3 {
                    min[k] = min[k].min(v[k]);
                    max[k] = max[k].max(v[k]);
                }
            }
        }
        Ok(Self { tris, colors, min, max, total_tris: total, vertices })
    }

    pub fn size(&self) -> V3 {
        sub(self.max, self.min)
    }
}

// ---------- lecture des formats ----------

pub fn load(path: &Path) -> Result<Model, String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "obj" => load_obj(&std::fs::read_to_string(path).map_err(|e| e.to_string())?),
        "stl" => load_stl(&std::fs::read(path).map_err(|e| e.to_string())?),
        "glb" | "gltf" => load_gltf(path),
        _ => Err("Format 3D non pris en charge.".into()),
    }
}

pub fn load_obj(text: &str) -> Result<Model, String> {
    let mut verts: Vec<V3> = vec![];
    let mut tris = vec![];
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let c: Vec<f32> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                if c.len() == 3 {
                    verts.push([c[0], c[1], c[2]]);
                }
            }
            Some("f") => {
                // « f 1/2/3 4/5/6 ... » : on garde l'indice du sommet, indices négatifs acceptés.
                let idx: Vec<usize> = it
                    .filter_map(|tok| tok.split('/').next()?.parse::<i64>().ok())
                    .filter_map(|i| if i > 0 { Some(i as usize - 1) } else if i < 0 { verts.len().checked_sub((-i) as usize) } else { None })
                    .collect();
                for k in 1..idx.len().saturating_sub(1) {
                    if let (Some(a), Some(b), Some(c)) = (verts.get(idx[0]), verts.get(idx[k]), verts.get(idx[k + 1])) {
                        tris.push([*a, *b, *c]);
                    }
                }
            }
            _ => {}
        }
    }
    let n = tris.len();
    Model::new(tris, vec![Color32::from_gray(200); n], verts.len())
}

pub fn load_stl(bytes: &[u8]) -> Result<Model, String> {
    let mut tris = vec![];
    let is_ascii = bytes.starts_with(b"solid") && std::str::from_utf8(&bytes[..bytes.len().min(512)]).map(|s| s.contains("facet")).unwrap_or(false);
    if is_ascii {
        let text = String::from_utf8_lossy(bytes);
        let mut cur: Vec<V3> = vec![];
        for line in text.lines() {
            let mut it = line.split_whitespace();
            if it.next() == Some("vertex") {
                let c: Vec<f32> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                if c.len() == 3 {
                    cur.push([c[0], c[1], c[2]]);
                }
                if cur.len() == 3 {
                    tris.push([cur[0], cur[1], cur[2]]);
                    cur.clear();
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
            tris.push([[f(o), f(o + 4), f(o + 8)], [f(o + 12), f(o + 16), f(o + 20)], [f(o + 24), f(o + 28), f(o + 32)]]);
        }
    }
    let n = tris.len();
    Model::new(tris, vec![Color32::from_gray(200); n], n * 3)
}

fn load_gltf(path: &Path) -> Result<Model, String> {
    let (doc, buffers, _images) = gltf::import(path).map_err(|e| format!("glTF illisible : {e}"))?;
    let mut tris = vec![];
    let mut colors = vec![];
    let mut vertices = 0;
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
    let mut stack: Vec<(gltf::Node, [[f32; 4]; 4])> = vec![];
    let identity = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
    let scene = doc.default_scene().or_else(|| doc.scenes().next()).ok_or("Aucune scène.")?;
    for n in scene.nodes() {
        stack.push((n, identity));
    }
    while let Some((node, parent)) = stack.pop() {
        let world = mul(&parent, &node.transform().matrix());
        if let Some(mesh) = node.mesh() {
            for prim in mesh.primitives() {
                if prim.mode() != gltf::mesh::Mode::Triangles {
                    continue;
                }
                let reader = prim.reader(|b| Some(&buffers[b.index()]));
                let Some(pos) = reader.read_positions() else { continue };
                let pos: Vec<V3> = pos.map(|p| apply(&world, p)).collect();
                vertices += pos.len();
                let c = prim.material().pbr_metallic_roughness().base_color_factor();
                let col = Color32::from_rgb((c[0].powf(1.0 / 2.2) * 255.0) as u8, (c[1].powf(1.0 / 2.2) * 255.0) as u8, (c[2].powf(1.0 / 2.2) * 255.0) as u8);
                let idx: Vec<u32> = match reader.read_indices() {
                    Some(i) => i.into_u32().collect(),
                    None => (0..pos.len() as u32).collect(),
                };
                for t in idx.chunks_exact(3) {
                    if let (Some(a), Some(b), Some(cc)) = (pos.get(t[0] as usize), pos.get(t[1] as usize), pos.get(t[2] as usize)) {
                        tris.push([*a, *b, *cc]);
                        colors.push(col);
                    }
                }
            }
        }
        for child in node.children() {
            stack.push((child, world));
        }
    }
    Model::new(tris, colors, vertices)
}

// ---------- visionneuse ----------

pub enum State {
    Loading(Receiver<Result<Model, String>>),
    Ready(Box<Model>),
    Failed(String),
}

pub struct Viewer {
    pub state: State,
    yaw: f32,
    pitch: f32,
    dist: f32,
    pan: egui::Vec2,
    pub wireframe: bool,
    /// Axe vertical : Y (glTF, la plupart des OBJ) ou Z (Blender STL / certains OBJ).
    pub z_up: bool,
}

impl Viewer {
    pub fn open(path: std::path::PathBuf, ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let stl = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("stl"));
        std::thread::spawn(move || {
            let _ = tx.send(load(&path));
            ctx.request_repaint();
        });
        Self { state: State::Loading(rx), yaw: 0.6, pitch: 0.35, dist: 2.2, pan: egui::Vec2::ZERO, wireframe: false, z_up: stl }
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
                    ui.label(egui::RichText::new("chargement du modèle…").color(p.muted));
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

        ui.horizontal(|ui| {
            let s = model.size();
            let shown = if model.tris.len() < model.total_tris { format!(" (affichés : {})", model.tris.len()) } else { String::new() };
            ui.label(egui::RichText::new(format!("{} triangles{shown} · {} sommets · {:.2} × {:.2} × {:.2}", model.total_tris, model.vertices, s[0], s[1], s[2])).color(p.muted).size(11.5));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("recentrer").clicked() {
                    self.yaw = 0.6;
                    self.pitch = 0.35;
                    self.dist = 2.2;
                    self.pan = egui::Vec2::ZERO;
                }
                ui.checkbox(&mut self.z_up, "Z en haut");
                ui.checkbox(&mut self.wireframe, "filaire");
            });
        });

        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, p.panel);

        if resp.dragged_by(egui::PointerButton::Primary) {
            let d = resp.drag_delta();
            self.yaw -= d.x * 0.01;
            self.pitch = (self.pitch + d.y * 0.01).clamp(-1.55, 1.55);
        }
        if resp.dragged_by(egui::PointerButton::Secondary) || resp.dragged_by(egui::PointerButton::Middle) {
            self.pan += resp.drag_delta();
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            self.dist = (self.dist * (-scroll * 0.002).exp()).clamp(0.3, 20.0);
        }

        // Normalise le modèle : centré, taille 1.
        let s = model.size();
        let scale = 1.0 / s[0].max(s[1]).max(s[2]).max(1e-6);
        let c = [(model.min[0] + model.max[0]) / 2.0, (model.min[1] + model.max[1]) / 2.0, (model.min[2] + model.max[2]) / 2.0];
        let z_up = self.z_up;
        let to_view = |v: V3| -> V3 {
            let mut v = [(v[0] - c[0]) * scale, (v[1] - c[1]) * scale, (v[2] - c[2]) * scale];
            if z_up {
                v = [v[0], v[2], -v[1]];
            }
            let (sy, cy) = self.yaw.sin_cos();
            let (sp, cp) = self.pitch.sin_cos();
            let x = v[0] * cy + v[2] * sy;
            let z = -v[0] * sy + v[2] * cy;
            let y = v[1] * cp - z * sp;
            let z = v[1] * sp + z * cp;
            [x, y, z - self.dist]
        };
        let focal = rect.height().min(rect.width()) * 1.2;
        let center = rect.center() + self.pan;
        let project = |v: V3| -> Option<Pos2> { (v[2] < -0.05).then(|| pos2(center.x + v[0] / -v[2] * focal, center.y - v[1] / -v[2] * focal)) };

        // Sol : grille légère
        let grid_y = if z_up { -s[2] * scale / 2.0 } else { -s[1] * scale / 2.0 };
        for k in -5..=5 {
            let t = k as f32 * 0.2;
            for (a, b) in [([t, grid_y, -1.0], [t, grid_y, 1.0]), ([-1.0, grid_y, t], [1.0, grid_y, t])] {
                let tv = |q: V3| -> V3 {
                    let (sy, cy) = self.yaw.sin_cos();
                    let (sp, cp) = self.pitch.sin_cos();
                    let x = q[0] * cy + q[2] * sy;
                    let z = -q[0] * sy + q[2] * cy;
                    let y = q[1] * cp - z * sp;
                    let z = q[1] * sp + z * cp;
                    [x, y, z - self.dist]
                };
                if let (Some(pa), Some(pb)) = (project(tv(a)), project(tv(b))) {
                    painter.line_segment([pa, pb], Stroke::new(1.0, p.line));
                }
            }
        }

        let light = norm([0.4, 0.8, 0.6]);
        let mut faces: Vec<(f32, [Pos2; 3], Color32)> = Vec::with_capacity(model.tris.len());
        for (t, col) in model.tris.iter().zip(&model.colors) {
            let a = to_view(t[0]);
            let b = to_view(t[1]);
            let cc = to_view(t[2]);
            let n = norm(cross(sub(b, a), sub(cc, a)));
            let (Some(pa), Some(pb), Some(pc)) = (project(a), project(b), project(cc)) else { continue };
            // Faces vues de dos : éclairées comme si elles étaient retournées (les modèles ne sont pas toujours fermés).
            let facing = dot(n, norm([-a[0], -a[1], -a[2]]));
            let nn = if facing < 0.0 { [-n[0], -n[1], -n[2]] } else { n };
            let shade = 0.35 + 0.65 * dot(nn, light).max(0.0);
            let col = Color32::from_rgb((col.r() as f32 * shade) as u8, (col.g() as f32 * shade) as u8, (col.b() as f32 * shade) as u8);
            faces.push(((a[2] + b[2] + cc[2]) / 3.0, [pa, pb, pc], col));
        }
        faces.sort_by(|x, y| x.0.total_cmp(&y.0));

        if self.wireframe {
            for (_, pts, _) in &faces {
                painter.add(Shape::closed_line(pts.to_vec(), Stroke::new(0.6, p.text.linear_multiply(0.6))));
            }
        } else {
            // Un seul maillage egui par paquet de 20 000 triangles.
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
        let _ = Rect::NOTHING;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        let obj = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1/1/1 2/2/2 3/3/3 4/4/4\nf -4 -3 -2\n";
        let m = load_obj(obj).unwrap();
        assert_eq!(m.total_tris, 3, "un quad = 2 triangles, plus un triangle en indices négatifs");
        assert_eq!(m.size(), [1.0, 1.0, 0.0]);

        let mut stl = vec![0u8; 80];
        stl.extend(1u32.to_le_bytes());
        stl.extend([0u8; 12]);
        for v in [[0f32, 0., 0.], [2., 0., 0.], [0., 3., 0.]] {
            for c in v {
                stl.extend(c.to_le_bytes());
            }
        }
        stl.extend([0u8; 2]);
        let m = load_stl(&stl).unwrap();
        assert_eq!(m.total_tris, 1);
        assert_eq!(m.size(), [2.0, 3.0, 0.0]);

        let ascii = "solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid t\n";
        assert_eq!(load_stl(ascii.as_bytes()).unwrap().total_tris, 1);
        assert!(is_model("assets/Maison.GLB") && !is_model("a.png"));
    }
}
