//! Pile de modificateurs, comme dans 3ds Max et Blender : chaque modificateur transforme
//! le maillage du précédent, sans toucher au fichier d'origine. On peut les activer,
//! les réordonner, les régler, puis exporter le résultat en OBJ.

use crate::model3d::{fan_mask, Model};
use eframe::egui::{self, Color32, RichText};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

type V3 = [f32; 3];

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum Kind {
    Mirror { axis: u8, merge: bool },
    Array { count: u32, axis: u8, offset: f32 },
    Subdivision {
        levels: u8,
        #[serde(default)]
        optimal: bool,
        #[serde(default)]
        simple: bool,
    },
    Decimate { ratio: f32 },
    Smooth { iterations: u32, factor: f32 },
    Displace { strength: f32, scale: f32, seed: f32 },
    Twist { angle: f32, axis: u8 },
    Bend { angle: f32, axis: u8 },
    Taper { factor: f32, axis: u8 },
    Spherify { factor: f32 },
    Solidify { thickness: f32 },
    Wave { amplitude: f32, frequency: f32, axis: u8 },
    Transform { scale: f32, rotate: [f32; 3], offset: [f32; 3] },
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Modifier {
    pub kind: Kind,
    pub on: bool,
    #[serde(default)]
    pub open: bool,
}

pub const ALL: [(&str, &str); 13] = [
    ("Miroir", "symétrie autour de l'origine de l'objet"),
    ("Réseau (array)", "copies alignées"),
    ("Subdivision", "lisse et densifie (Catmull-Clark, comme Blender)"),
    ("Décimer", "réduit le nombre de triangles"),
    ("Lisser", "adoucit les formes"),
    ("Déplacement (bruit)", "relief procédural"),
    ("Torsion (twist)", "tord autour d'un axe"),
    ("Courbure (bend)", "plie le long d'un axe"),
    ("Effilement (taper)", "rétrécit vers un bout"),
    ("Sphériser (cast)", "tend vers une sphère"),
    ("Épaissir (solidify)", "donne une épaisseur"),
    ("Onde", "vagues le long d'un axe"),
    ("Transformation", "échelle, rotation, décalage"),
];

impl Kind {
    pub fn new(i: usize) -> Kind {
        match i {
            0 => Kind::Mirror { axis: 0, merge: true },
            1 => Kind::Array { count: 3, axis: 0, offset: 1.1 },
            2 => Kind::Subdivision { levels: 2, optimal: true, simple: false },
            3 => Kind::Decimate { ratio: 0.5 },
            4 => Kind::Smooth { iterations: 3, factor: 0.5 },
            5 => Kind::Displace { strength: 0.05, scale: 4.0, seed: 0.0 },
            6 => Kind::Twist { angle: 90.0, axis: 1 },
            7 => Kind::Bend { angle: 90.0, axis: 0 },
            8 => Kind::Taper { factor: -0.5, axis: 1 },
            9 => Kind::Spherify { factor: 0.5 },
            10 => Kind::Solidify { thickness: 0.03 },
            11 => Kind::Wave { amplitude: 0.03, frequency: 4.0, axis: 0 },
            _ => Kind::Transform { scale: 1.0, rotate: [0.0; 3], offset: [0.0; 3] },
        }
    }
    pub fn name(&self) -> &'static str {
        let i = match self {
            Kind::Mirror { .. } => 0,
            Kind::Array { .. } => 1,
            Kind::Subdivision { .. } => 2,
            Kind::Decimate { .. } => 3,
            Kind::Smooth { .. } => 4,
            Kind::Displace { .. } => 5,
            Kind::Twist { .. } => 6,
            Kind::Bend { .. } => 7,
            Kind::Taper { .. } => 8,
            Kind::Spherify { .. } => 9,
            Kind::Solidify { .. } => 10,
            Kind::Wave { .. } => 11,
            Kind::Transform { .. } => 12,
        };
        ALL[i].0
    }
}

/// Maillage de travail (espace normalisé, Y en haut).
#[derive(Clone)]
pub struct Mesh {
    pub pos: Vec<V3>,
    pub tris: Vec<[u32; 3]>,
    pub colors: Vec<Color32>,
    pub edges: Vec<u8>,
}

impl Mesh {
    pub fn from_model(m: &Model, z_up: bool) -> Mesh {
        Mesh { pos: m.normalized(z_up), tris: m.tris.clone(), colors: m.colors.clone(), edges: m.edges.clone() }
    }
    fn bbox(&self) -> (V3, V3) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for p in &self.pos {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        (lo, hi)
    }
}

/// Applique la pile (de haut en bas). `origin` : origine de l'objet dans l'espace normalisé.
pub fn apply(stack: &[Modifier], mut m: Mesh, origin: V3) -> Mesh {
    for md in stack.iter().filter(|m| m.on) {
        if m.tris.is_empty() {
            break;
        }
        m = apply_one(md.kind, m, origin);
    }
    m
}

fn rot(p: V3, axis: usize, a: f32) -> V3 {
    let (s, c) = a.sin_cos();
    let (i, j) = ((axis + 1) % 3, (axis + 2) % 3);
    let mut q = p;
    q[i] = p[i] * c - p[j] * s;
    q[j] = p[i] * s + p[j] * c;
    q
}

fn neighbors(n: usize, tris: &[[u32; 3]]) -> Vec<Vec<u32>> {
    let mut nb: Vec<Vec<u32>> = vec![vec![]; n];
    for t in tris {
        for k in 0..3 {
            let (a, b) = (t[k] as usize, t[(k + 1) % 3]);
            if !nb[a].contains(&b) {
                nb[a].push(b);
            }
            if !nb[b as usize].contains(&t[k]) {
                nb[b as usize].push(t[k]);
            }
        }
    }
    nb
}

fn normals(pos: &[V3], tris: &[[u32; 3]]) -> Vec<V3> {
    let mut n = vec![[0.0f32; 3]; pos.len()];
    for t in tris {
        let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let f = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        for &i in t {
            for k in 0..3 {
                n[i as usize][k] += f[k];
            }
        }
    }
    n.iter()
        .map(|v| {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-12);
            [v[0] / l, v[1] / l, v[2] / l]
        })
        .collect()
}

/// Inverse le sens d'un triangle (et réordonne ses arêtes visibles).
fn flip(t: [u32; 3], e: u8) -> ([u32; 3], u8) {
    ([t[0], t[2], t[1]], ((e >> 2) & 1) | (e & 2) | ((e & 1) << 2))
}

fn apply_one(k: Kind, mut m: Mesh, origin: V3) -> Mesh {
    let (lo, hi) = m.bbox();
    let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let ax = |a: u8| (a as usize).min(2);
    match k {
        Kind::Mirror { axis, merge } => {
            let a = ax(axis);
            let n = m.pos.len();
            let tol = size[0].max(size[1]).max(size[2]) * 1e-4;
            // Les sommets sur le plan de symétrie sont partagés (soudure)
            let mut map = Vec::with_capacity(n);
            for i in 0..n {
                let p = m.pos[i];
                if merge && (p[a] - origin[a]).abs() <= tol {
                    map.push(i as u32);
                } else {
                    let mut q = p;
                    q[a] = 2.0 * origin[a] - p[a];
                    m.pos.push(q);
                    map.push((m.pos.len() - 1) as u32);
                }
            }
            let nt = m.tris.len();
            for i in 0..nt {
                let t = m.tris[i];
                let (ft, fe) = flip([map[t[0] as usize], map[t[1] as usize], map[t[2] as usize]], m.edges[i]);
                m.tris.push(ft);
                m.edges.push(fe);
                m.colors.push(m.colors[i]);
            }
        }
        Kind::Array { count, axis, offset } => {
            let a = ax(axis);
            let n = m.pos.len() as u32;
            let nt = m.tris.len();
            let step = size[a] * offset;
            let count = count.clamp(1, 64);
            if (nt as u64) * count as u64 > crate::model3d::MAX_SUBDIV_TRIS as u64 {
                return m;
            }
            for c in 1..count {
                for i in 0..n as usize {
                    let mut p = m.pos[i];
                    p[a] += step * c as f32;
                    m.pos.push(p);
                }
                for i in 0..nt {
                    let t = m.tris[i];
                    m.tris.push([t[0] + n * c, t[1] + n * c, t[2] + n * c]);
                    m.edges.push(m.edges[i]);
                    m.colors.push(m.colors[i]);
                }
            }
        }
        Kind::Subdivision { levels, optimal, simple } => {
            if simple {
                // « Simple » : densifie sans lisser (comme Blender)
                let (p, t, c, e) = crate::model3d::split_flat(&m.pos, &m.tris, &m.colors, &m.edges, levels.min(5), optimal);
                m = Mesh { pos: p, tris: t, colors: c, edges: e };
            } else {
                let (p, t, c, e) = crate::model3d::catmull_clark(&m.pos, &m.tris, &m.colors, &m.edges, levels.min(5), optimal);
                m = Mesh { pos: p, tris: t, colors: c, edges: e };
            }
        }
        Kind::Decimate { ratio } => {
            let target = ((m.tris.len() as f32) * ratio.clamp(0.01, 1.0)) as usize;
            let (p, t, c) = crate::model3d::simplify_arrays(&m.pos, &m.tris, &m.colors, target.max(4));
            m.edges = vec![7; t.len()];
            m.pos = p;
            m.tris = t;
            m.colors = c;
        }
        Kind::Smooth { iterations, factor } => {
            let nb = neighbors(m.pos.len(), &m.tris);
            for _ in 0..iterations.min(50) {
                let old = m.pos.clone();
                for (i, ns) in nb.iter().enumerate() {
                    if ns.is_empty() {
                        continue;
                    }
                    let mut avg = [0.0; 3];
                    for &j in ns {
                        for k in 0..3 {
                            avg[k] += old[j as usize][k];
                        }
                    }
                    for k in 0..3 {
                        avg[k] /= ns.len() as f32;
                        m.pos[i][k] = old[i][k] + (avg[k] - old[i][k]) * factor;
                    }
                }
            }
        }
        Kind::Displace { strength, scale, seed } => {
            let n = normals(&m.pos, &m.tris);
            for (p, nn) in m.pos.iter_mut().zip(n) {
                let d = (crate::nodes::fbm(p[0] * scale + seed, p[1] * scale, p[2] * scale - seed, 5, 0.5) - 0.5) * 2.0 * strength;
                for k in 0..3 {
                    p[k] += nn[k] * d;
                }
            }
        }
        Kind::Twist { angle, axis } => {
            let a = ax(axis);
            let len = size[a].max(1e-6);
            for p in &mut m.pos {
                let t = (p[a] - lo[a]) / len - 0.5;
                *p = rot(*p, a, angle.to_radians() * t);
            }
        }
        Kind::Bend { angle, axis } => {
            let a = ax(axis);
            let u = (a + 1) % 3;
            let len = size[a].max(1e-6);
            let ang = angle.to_radians();
            if ang.abs() > 1e-4 {
                let r = len / ang;
                let mid = (lo[a] + hi[a]) / 2.0;
                for p in &mut m.pos {
                    let th = (p[a] - mid) / len * ang;
                    let d = r - (p[u] - lo[u]);
                    p[a] = mid + d * th.sin();
                    p[u] = lo[u] + r - d * th.cos();
                }
            }
        }
        Kind::Taper { factor, axis } => {
            let a = ax(axis);
            let len = size[a].max(1e-6);
            let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
            for p in &mut m.pos {
                let t = (p[a] - lo[a]) / len;
                let s = (1.0 + factor * t).max(0.0);
                for k in 0..3 {
                    if k != a {
                        p[k] = c[k] + (p[k] - c[k]) * s;
                    }
                }
            }
        }
        Kind::Spherify { factor } => {
            let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
            let r = m.pos.iter().map(|p| ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt()).sum::<f32>() / m.pos.len().max(1) as f32;
            for p in &mut m.pos {
                let d = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
                let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
                for k in 0..3 {
                    let target = c[k] + d[k] / l * r;
                    p[k] += (target - p[k]) * factor.clamp(-1.0, 1.0);
                }
            }
        }
        Kind::Solidify { thickness } => {
            let n = m.pos.len() as u32;
            let nrm = normals(&m.pos, &m.tris);
            let inner: Vec<V3> = m.pos.iter().zip(&nrm).map(|(p, q)| [p[0] - q[0] * thickness, p[1] - q[1] * thickness, p[2] - q[2] * thickness]).collect();
            m.pos.extend(inner);
            let nt = m.tris.len();
            // Arêtes de bord (un seul triangle) : on y ferme l'épaisseur
            let mut count: HashMap<(u32, u32), i32> = HashMap::new();
            for t in &m.tris {
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    *count.entry((a.min(b), a.max(b))).or_default() += 1;
                }
            }
            for i in 0..nt {
                let t = m.tris[i];
                let (ft, fe) = flip([t[0] + n, t[1] + n, t[2] + n], m.edges[i]);
                m.tris.push(ft);
                m.edges.push(fe);
                m.colors.push(m.colors[i]);
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    if count.get(&(a.min(b), a.max(b))) == Some(&1) {
                        m.tris.push([b, a, a + n]);
                        m.tris.push([b, a + n, b + n]);
                        m.edges.push(fan_mask(1, 4));
                        m.edges.push(fan_mask(2, 4));
                        m.colors.push(m.colors[i]);
                        m.colors.push(m.colors[i]);
                    }
                }
            }
        }
        Kind::Wave { amplitude, frequency, axis } => {
            let a = ax(axis);
            let u = (a + 1) % 3;
            let len = size[a].max(1e-6);
            for p in &mut m.pos {
                p[u] += amplitude * ((p[a] - lo[a]) / len * frequency * std::f32::consts::TAU).sin();
            }
        }
        Kind::Transform { scale, rotate, offset } => {
            for p in &mut m.pos {
                let mut q = [(p[0] - origin[0]) * scale, (p[1] - origin[1]) * scale, (p[2] - origin[2]) * scale];
                for a in 0..3 {
                    if rotate[a] != 0.0 {
                        q = rot(q, a, rotate[a].to_radians());
                    }
                }
                *p = [q[0] + origin[0] + offset[0], q[1] + origin[1] + offset[1], q[2] + origin[2] + offset[2]];
            }
        }
    }
    m
}

/// Écrit le maillage en OBJ (dans les unités et l'orientation du fichier d'origine).
pub fn export_obj(m: &Mesh, model: &Model, z_up: bool, path: &std::path::Path) -> Result<(), String> {
    use std::io::Write;
    let s = model.size();
    let longest = s[0].max(s[1]).max(s[2]).max(1e-9);
    let c = [(model.min[0] + model.max[0]) / 2.0, (model.min[1] + model.max[1]) / 2.0, (model.min[2] + model.max[2]) / 2.0];
    let f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut w = std::io::BufWriter::new(f);
    writeln!(w, "# exporté par Aryadne (modificateurs appliqués)").map_err(|e| e.to_string())?;
    for p in &m.pos {
        let o = if z_up { [p[0], -p[2], p[1]] } else { *p };
        writeln!(w, "v {} {} {}", o[0] * longest + c[0], o[1] * longest + c[1], o[2] * longest + c[2]).map_err(|e| e.to_string())?;
    }
    for t in &m.tris {
        writeln!(w, "f {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------- panneau ----------

/// Panneau de la pile. Renvoie vrai si la pile a changé.
pub fn panel(ui: &mut egui::Ui, stack: &mut Vec<Modifier>) -> bool {
    let p = crate::theme::pal();
    let mut changed = false;
    ui.spacing_mut().slider_width = 80.0;
    ui.set_max_width(ui.available_width());
    ui.horizontal(|ui| {
        ui.label(RichText::new("MODIFICATEURS").size(10.5).color(p.muted).family(crate::theme::bold()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::containers::menu::MenuButton::new("＋ ajouter ▾").ui(ui, |ui| {
                ui.set_min_width(260.0);
                for (i, (name, help)) in ALL.iter().enumerate() {
                    if ui.button(*name).on_hover_text(*help).clicked() {
                        stack.push(Modifier { kind: Kind::new(i), on: true, open: true });
                        changed = true;
                    }
                }
            });
        });
    });
    if stack.is_empty() {
        ui.label(RichText::new("Aucun modificateur. Ils s'appliquent de haut en bas, sans changer le fichier d'origine.").color(p.muted).size(11.0));
    }
    let mut action: Option<(usize, i32)> = None; // (index, -1 haut, +1 bas, 0 supprimer)
    let n = stack.len();
    for (i, md) in stack.iter_mut().enumerate() {
        egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(6).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if ui.checkbox(&mut md.on, "").on_hover_text("activer").changed() {
                    changed = true;
                }
                let t = RichText::new(md.kind.name()).family(crate::theme::bold()).size(12.0);
                if ui.add(egui::Button::new(if md.on { t } else { t.color(p.muted).strikethrough() }).frame(false)).clicked() {
                    md.open = !md.open;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text("supprimer").clicked() {
                        action = Some((i, 0));
                    }
                    if i + 1 < n && ui.small_button("↓").clicked() {
                        action = Some((i, 1));
                    }
                    if i > 0 && ui.small_button("↑").clicked() {
                        action = Some((i, -1));
                    }
                });
            });
            if md.open {
                changed |= params(ui, &mut md.kind);
            }
        });
        ui.add_space(4.0);
    }
    if let Some((i, a)) = action {
        match a {
            0 => {
                stack.remove(i);
            }
            -1 => stack.swap(i, i - 1),
            _ => stack.swap(i, i + 1),
        }
        changed = true;
    }
    changed
}

fn axis_ui(ui: &mut egui::Ui, a: &mut u8) -> bool {
    let mut c = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("axe").size(11.0));
        for (k, l) in ["X", "Y", "Z"].iter().enumerate() {
            c |= ui.selectable_value(a, k as u8, *l).changed();
        }
    });
    c
}

fn params(ui: &mut egui::Ui, k: &mut Kind) -> bool {
    let mut c = false;
    fn sl(ui: &mut egui::Ui, v: &mut f32, r: std::ops::RangeInclusive<f32>, t: &str) -> bool {
        ui.add(egui::Slider::new(v, r).text(t)).changed()
    }
    macro_rules! s {
        ($ui:expr, $v:expr, $r:expr, $t:expr) => {
            c |= sl($ui, $v, $r, $t)
        };
    }
    match k {
        Kind::Mirror { axis, merge } => {
            c |= axis_ui(ui, axis);
            c |= ui.checkbox(merge, "souder au centre").changed();
        }
        Kind::Array { count, axis, offset } => {
            c |= ui.add(egui::Slider::new(count, 1..=32).text("copies")).changed();
            c |= axis_ui(ui, axis);
            s!(ui, offset, -3.0..=3.0, "écart (× taille)");
        }
        Kind::Subdivision { levels, optimal, simple } => {
            c |= ui.add(egui::Slider::new(levels, 0..=5).text("niveaux")).changed();
            ui.horizontal(|ui| {
                c |= ui.selectable_value(simple, false, "Catmull-Clark").changed();
                c |= ui.selectable_value(simple, true, "simple").changed();
            });
            c |= ui.checkbox(optimal, "affichage optimal").on_hover_text("Le fil de fer montre seulement les arêtes de la cage d'origine").changed();
        }
        Kind::Decimate { ratio } => {
            s!(ui, ratio, 0.01..=1.0, "proportion gardée");
        }
        Kind::Smooth { iterations, factor } => {
            c |= ui.add(egui::Slider::new(iterations, 1..=50).text("passes")).changed();
            s!(ui, factor, 0.0..=1.0, "force");
        }
        Kind::Displace { strength, scale, seed } => {
            s!(ui, strength, -0.3..=0.3, "force");
            s!(ui, scale, 0.2..=40.0, "échelle");
            s!(ui, seed, 0.0..=100.0, "variante");
        }
        Kind::Twist { angle, axis } => {
            s!(ui, angle, -720.0..=720.0, "angle (°)");
            c |= axis_ui(ui, axis);
        }
        Kind::Bend { angle, axis } => {
            s!(ui, angle, -360.0..=360.0, "angle (°)");
            c |= axis_ui(ui, axis);
        }
        Kind::Taper { factor, axis } => {
            s!(ui, factor, -1.0..=2.0, "facteur");
            c |= axis_ui(ui, axis);
        }
        Kind::Spherify { factor } => {
            s!(ui, factor, -1.0..=1.0, "facteur");
        }
        Kind::Solidify { thickness } => {
            s!(ui, thickness, -0.2..=0.2, "épaisseur");
        }
        Kind::Wave { amplitude, frequency, axis } => {
            s!(ui, amplitude, -0.2..=0.2, "amplitude");
            s!(ui, frequency, 0.1..=20.0, "fréquence");
            c |= axis_ui(ui, axis);
        }
        Kind::Transform { scale, rotate, offset } => {
            s!(ui, scale, 0.05..=5.0, "échelle");
            for (a, l) in ["rotation X", "rotation Y", "rotation Z"].iter().enumerate() {
                s!(ui, &mut rotate[a], -180.0..=180.0, l);
            }
            for (a, l) in ["décalage X", "décalage Y", "décalage Z"].iter().enumerate() {
                s!(ui, &mut offset[a], -1.0..=1.0, l);
            }
        }
    }
    let _ = Color32::WHITE;
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pile() {
        let obj = "v -1 -1 -1\nv 1 -1 -1\nv 1 1 -1\nv -1 1 -1\nv -1 -1 1\nv 1 -1 1\nv 1 1 1\nv -1 1 1\nf 1 4 3 2\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n";
        let model = crate::model3d::load_obj(obj.as_bytes()).unwrap();
        let base = Mesh::from_model(&model, false);
        let mk = |k| Modifier { kind: k, on: true, open: false };
        // Réseau de 3 : trois fois plus de tout
        let m = apply(&[mk(Kind::Array { count: 3, axis: 0, offset: 1.0 })], base.clone(), [0.0; 3]);
        assert_eq!((m.pos.len(), m.tris.len()), (24, 36));
        // Miroir sur un cube centré : aucun sommet sur le plan (x = ±0.5), donc doublé
        let m = apply(&[mk(Kind::Mirror { axis: 0, merge: true })], base.clone(), [0.0; 3]);
        assert_eq!(m.tris.len(), 24);
        // Épaissir un plan ouvert : 2 faces + 4 côtés de 2 triangles
        let plane = crate::model3d::load_obj("v 0 0 0\nv 1 0 0\nv 1 0 1\nv 0 0 1\nf 1 2 3 4\n".as_bytes()).unwrap();
        let m = apply(&[mk(Kind::Solidify { thickness: 0.1 })], Mesh::from_model(&plane, false), [0.0; 3]);
        assert_eq!(m.tris.len(), 2 + 2 + 8);
        // Désactivé = sans effet ; sous-division ×4 ; décimer réduit
        let off = Modifier { kind: Kind::Subdivision { levels: 2, optimal: false, simple: false }, on: false, open: false };
        assert_eq!(apply(&[off], base.clone(), [0.0; 3]).tris.len(), 12);
        let m = apply(&[mk(Kind::Subdivision { levels: 2, optimal: false, simple: false }), mk(Kind::Decimate { ratio: 0.5 })], base.clone(), [0.0; 3]);
        assert!(m.tris.len() < 192 && m.tris.len() > 20, "{}", m.tris.len());
        // Toutes les déformations gardent un maillage valide
        for i in 0..ALL.len() {
            let m = apply(&[mk(Kind::new(i))], base.clone(), [0.0; 3]);
            assert!(m.pos.iter().all(|p| p.iter().all(|v| v.is_finite())), "{}", ALL[i].0);
            assert!(m.tris.iter().all(|t| t.iter().all(|&k| (k as usize) < m.pos.len())));
            assert_eq!(m.tris.len(), m.edges.len());
            assert_eq!(m.tris.len(), m.colors.len());
        }
        // JSON aller-retour
        let st = vec![mk(Kind::Twist { angle: 45.0, axis: 1 })];
        let t = serde_json::to_string(&st).unwrap();
        let back: Vec<Modifier> = serde_json::from_str(&t).unwrap();
        assert_eq!(back, st);
    }
}
