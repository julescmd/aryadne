//! Géométrie et simulation en nodes : la vue. Un aperçu 3D des points et des arêtes, la lecture
//! image par image avec sa barre de cache, les miniatures des nodes et le tableau des attributs.

use crate::geo::{Engine, Geo, Stats};
use crate::nodeflow::TimeRange;
use crate::nodes::Graph;
use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, RichText, Sense, Stroke};
use std::collections::HashMap;

/// Miniature d'un node : quelques centaines de points déjà projetés, entre 0 et 1.
struct Thumb {
    key: (u64, i32),
    /// Les points montrés datent d'une image d'avant : la géométrie était encore sur la carte
    stale: bool,
    pts: Vec<([f32; 2], Color32)>,
    lines: Vec<[[f32; 2]; 2]>,
    count: usize,
    error: bool,
}

pub struct GeoView {
    pub engine: Engine,
    pub frame: i32,
    pub playing: bool,
    last_tick: Option<std::time::Instant>,
    acc: f32,
    yaw: f32,
    pitch: f32,
    dist: f32,
    target: [f32; 3],
    framed: bool,
    /// Tant que l'artiste n'a pas touché à la caméra, le cadre s'élargit pour suivre ce qui grandit
    auto: bool,
    /// Boîte englobante de tout ce qui a été vu depuis le dernier recadrage
    seen: Option<([f32; 3], [f32; 3])>,
    result: Option<Geo>,
    key: Option<(u64, i32, Option<u32>)>,
    pub error: Option<(u32, String)>,
    stats: Stats,
    ms: f32,
    thumbs: HashMap<u32, Thumb>,
    /// Node montré à la place de la sortie (« prévisualiser la node »)
    pub solo: Option<u32>,
    /// Zones de simulation du graphe (node de fin), pour la barre de cache
    zones: Vec<u32>,
    table: Option<((u64, i32, u32), Result<Geo, String>)>,
    /// Obstacles des solveurs du graphe, dessinés en fil de fer
    obstacles: Vec<Obstacle>,
    /// Les surfaces dessinées sur la carte graphique, et l'image qu'elles donnent
    canvas: crate::gpudraw::Canvas,
    /// Le cadre attend la boîte englobante d'une géométrie encore sur la carte
    want_frame: bool,
    /// Résultat calculé mais pas encore lisible sans attendre la carte : montré dès qu'il l'est
    pending: Option<Geo>,
    /// La dernière image a-t-elle été dessinée depuis la carte, sans relecture ?
    direct: bool,
    /// Triangles de la géométrie montrée, et pour chaque sommet s'il appartient à une face : refaits
    /// seulement quand les faces changent (clé des faces, nombre de points)
    tris: Option<(u64, usize, std::sync::Arc<Vec<[u32; 3]>>, std::sync::Arc<Vec<bool>>)>,
    /// Le volume (fumée, feu) dessiné par la carte, et l'image qu'il donne
    vol_canvas: crate::gpuvolume::Canvas,
    /// Sans dessin par la carte : le volume rendu en petit sur le processeur, avec la clé de ce rendu
    vol_soft: Option<(u64, egui::TextureHandle)>,
}

/// Un obstacle réglé dans un node « Solveur » : boule (centre, rayon) ou boîte (centre, demi-tailles).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Obstacle {
    Ball([f32; 3], f32),
    Cube([f32; 3], [f32; 3]),
}

/// Rangs des entrées du node « Solveur »
mod inp {
    pub const ITERATIONS: usize = 1;
    pub const COMPLIANCE: usize = 2;
    pub const DAMPING: usize = 3;
    pub const BALL: usize = 6;
    pub const BALL_RADIUS: usize = 7;
    pub const GRIP: usize = 8;
    pub const STIFFNESS: usize = 9;
    pub const BENDING: usize = 10;
    pub const SUBSTEPS: usize = 11;
    pub const CUBE: usize = 12;
    pub const CUBE_SIZE: usize = 13;
    pub const SHEAR: usize = 14;
    pub const CALM: usize = 15;
    pub const THICKNESS: usize = 16;
}

/// Valeur réglée à la main d'une entrée, ou rien si elle est reliée.
fn const_input(g: &Graph, n: &crate::nodes::Node, k: usize) -> Option<[f32; 4]> {
    if g.links.iter().any(|l| l.to == n.id && l.inp == k) {
        return None;
    }
    n.values.get(k).copied().or_else(|| n.def().and_then(|d| d.inputs.get(k)).map(|i| i.2))
}

fn obstacles_of(g: &Graph) -> Vec<Obstacle> {
    let mut out = vec![];
    for n in g.nodes.iter().filter(|n| (n.op == "sim_solver" || n.op == "sim_smoke") && !n.muted) {
        // Le solveur de fumée range ses obstacles à d'autres places
        let (ball, ball_r, cube, cube_s) = if n.op == "sim_smoke" { (10, 11, 12, 13) } else { (inp::BALL, inp::BALL_RADIUS, inp::CUBE, inp::CUBE_SIZE) };
        if let (Some(c), Some(r)) = (const_input(g, n, ball), const_input(g, n, ball_r)) {
            if r[0] > 0.0 {
                out.push(Obstacle::Ball([c[0], c[1], c[2]], r[0]));
            }
        }
        if let (Some(c), Some(s)) = (const_input(g, n, cube), const_input(g, n, cube_s)) {
            if s[..3].iter().all(|x| *x > 0.0) {
                out.push(Obstacle::Cube([c[0], c[1], c[2]], [s[0] * 0.5, s[1] * 0.5, s[2] * 0.5]));
            }
        }
    }
    out
}

/// Couleur d'un point d'après son attribut `Cd` (ou la teinte par défaut).
fn point_color(g: &Geo, i: usize, base: Color32) -> Color32 {
    match g.col("Cd") {
        Some(c) => {
            let v = c.get(i);
            let q = |x: f32| (x.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0 + 0.5) as u8;
            Color32::from_rgb(q(v[0]), q(v[1]), q(v[2]))
        }
        None => base,
    }
}

impl GeoView {
    pub fn new(g: &Graph) -> GeoView {
        let start = g.time.unwrap_or_default().start;
        GeoView { engine: Engine::default(), frame: start, playing: false, last_tick: None, acc: 0.0, yaw: 0.6, pitch: 0.35, dist: 5.0, target: [0.0, 0.8, 0.0], framed: false, auto: true, seen: None, result: None, key: None, error: None, stats: Stats::default(), ms: 0.0, thumbs: HashMap::new(), solo: None, zones: vec![], table: None, obstacles: vec![], canvas: Default::default(), want_frame: false, pending: None, direct: false, tris: None, vol_canvas: Default::default(), vol_soft: None }
    }

    /// Les caches figés sur le disque, d'après le réglage « cache » des fins de zone.
    fn sync_baked(&mut self, doc: &Graph, root: &str) {
        self.engine.baked.clear();
        for n in doc.nodes.iter().filter(|n| n.op == "sim_out") {
            if let Some(crate::nodes::Param::S(rel)) = n.params.get("cache") {
                let dir = std::path::Path::new(root).join(rel);
                if !rel.is_empty() && dir.is_dir() {
                    self.engine.baked.insert(n.id, dir);
                }
            }
        }
    }

    /// Avance la lecture et recalcule ce qui doit l'être. `version` change à chaque modification du graphe.
    pub fn tick(&mut self, doc: &Graph, version: u64, root: &str, ctx: &egui::Context) {
        let time = doc.time.unwrap_or_default();
        if self.playing {
            let now = std::time::Instant::now();
            let dt = self.last_tick.map(|t| (now - t).as_secs_f32()).unwrap_or(0.0).min(0.25);
            self.last_tick = Some(now);
            // Tant que la simulation n'a pas rattrapé l'image demandée, la lecture l'attend
            if !self.engine.incomplete {
                self.acc += dt * time.fps;
                let step = self.acc.floor();
                self.acc -= step;
                self.frame += step as i32;
                if self.frame > time.end {
                    self.frame = time.start;
                }
            }
            ctx.request_repaint();
        } else {
            self.last_tick = None;
        }
        self.frame = self.frame.clamp(time.start.min(time.end), time.end.max(time.start));
        // Un résultat qui attendait la carte est arrivé : on le montre
        if self.pending.as_ref().is_some_and(|g| g.ready()) {
            if let Some(g) = self.pending.take() {
                self.show_result(g);
            }
        }
        // Le cadre attendait la boîte d'une géométrie encore sur la carte
        if self.want_frame {
            if let Some(g) = self.result.clone() {
                self.want_frame = !self.frame_on(&g);
            }
        }
        if self.want_frame || self.pending.is_some() {
            ctx.request_repaint();
        }
        let key = (version, self.frame, self.solo);
        if self.key == Some(key) && !self.engine.incomplete {
            return;
        }
        if self.key.map(|k| k.0) != Some(version) {
            self.sync_baked(doc, root);
            self.zones = doc.nodes.iter().filter(|n| n.op == "sim_out").map(|n| n.id).collect();
            self.obstacles = obstacles_of(doc);
        }
        self.key = Some(key);
        let t0 = std::time::Instant::now();
        // Une simulation longue à rattraper se calcule par tranches : l'interface reste vivante
        self.engine.deadline = Some(t0 + std::time::Duration::from_millis(30));
        let r = match self.solo {
            Some(id) => self.engine.eval_node(doc, id, 0, self.frame, root),
            None => self.engine.eval(doc, self.frame, root),
        };
        self.stats = self.engine.stats;
        self.ms = t0.elapsed().as_secs_f32() * 1000.0;
        let incomplete = self.engine.incomplete;
        match r {
            Ok(g) => {
                // Sans dessin direct, une géométrie encore sur la carte attend d'être lisible : l'image
                // d'avant reste à l'écran, et l'interface ne se fige pas
                if !crate::gpudraw::shared() && !g.ready() {
                    self.pending = Some(g);
                } else {
                    self.pending = None;
                    self.show_result(g);
                }
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e);
                self.playing = false;
            }
        }
        self.engine.incomplete = incomplete;
        if incomplete {
            ctx.request_repaint();
        }
    }

    /// Montre une géométrie calculée, et cadre la vue dessus si l'artiste n'a pas pris la caméra.
    fn show_result(&mut self, g: Geo) {
        if !self.framed {
            self.seen = None;
            self.auto = true;
        }
        if (!self.framed || self.auto) && (g.n > 0 || g.volume.is_some()) {
            self.want_frame = !self.frame_on(&g);
        }
        self.result = Some(g);
    }

    /// Cadre la vue sur tout ce qui a été vu depuis le dernier recadrage : une fontaine qui grandit
    /// reste entière à l'écran, sans que la caméra saute à chaque image. Rend faux si la boîte
    /// englobante n'est pas encore connue (géométrie sur la carte) : à redemander plus tard.
    fn frame_on(&mut self, g: &Geo) -> bool {
        if g.n == 0 && g.volume.is_none() {
            return true;
        }
        // La boîte d'un volume compte, même vide : le cadre ne saute pas quand la fumée arrive
        let vol = g.volume.as_ref().map(|v| v.bounds());
        let pts = if g.n > 0 { g.bounds_now() } else { None };
        if g.n > 0 && pts.is_none() {
            return false;
        }
        let Some((lo, hi)) = (match (pts, vol) {
            (Some((a, b)), Some((c, d))) => Some(([a[0].min(c[0]), a[1].min(c[1]), a[2].min(c[2])], [b[0].max(d[0]), b[1].max(d[1]), b[2].max(d[2])])),
            (a, b) => a.or(b),
        }) else {
            return false;
        };
        let (lo, hi) = match self.seen {
            Some((a, b)) => ([a[0].min(lo[0]), a[1].min(lo[1]), a[2].min(lo[2])], [b[0].max(hi[0]), b[1].max(hi[1]), b[2].max(hi[2])]),
            None => (lo, hi),
        };
        if self.seen == Some((lo, hi)) && self.framed {
            return true;
        }
        self.seen = Some((lo, hi));
        self.target = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
        let d = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt();
        self.dist = (d * 1.25).clamp(0.5, 500.0);
        self.framed = true;
        true
    }

    /// La caméra de l'aperçu en matrices, pour le dessin sur la carte : la même projection que `project`.
    fn camera(&self, w: f32, h: f32) -> ([[f32; 4]; 4], [f32; 4], [f32; 3], [f32; 3]) {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let t = self.target;
        // Axes de la vue dans le monde : droite, haut, et vers l'œil
        let (x, y, z) = ([cy, 0.0, -sy], [-sy * sp, cp, -cy * sp], [sy * cp, sp, cy * cp]);
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let m = w.min(h) * 0.9;
        let (kx, ky) = (2.0 * m / w.max(1.0), 2.0 * m / h.max(1.0));
        let (near, far) = ((self.dist * 0.02).max(0.01), self.dist * 60.0 + 100.0);
        let f = far / (far - near);
        // Profondeur d'un point : distance à l'œil le long de l'axe de visée
        let depth = [-z[0], -z[1], -z[2], self.dist + dot(z, t)];
        let vp = crate::gpudraw::from_rows([[x[0] * kx, x[1] * kx, x[2] * kx, -dot(x, t) * kx], [y[0] * ky, y[1] * ky, y[2] * ky, -dot(y, t) * ky], [depth[0] * f, depth[1] * f, depth[2] * f, (depth[3] - near) * f], depth]);
        let eye = [t[0] + z[0] * self.dist, t[1] + z[1] * self.dist, t[2] + z[2] * self.dist];
        // La lumière suit la vue : en haut à droite de l'œil
        let l = [0.35f32, 0.6, 0.72];
        let light = [x[0] * l[0] + y[0] * l[1] + z[0] * l[2], x[1] * l[0] + y[1] * l[1] + z[1] * l[2], x[2] * l[0] + y[2] * l[1] + z[2] * l[2]];
        (vp, depth, light, eye)
    }

    /// Dessine sur la carte les morceaux de `g` qui ont des faces. Un tissu dont l'état est encore
    /// dans les tampons du solveur est dessiné **sans relecture** ; un morceau venu du processeur
    /// est envoyé à la carte, pour le même aspect. Rend les plages de sommets dessinées
    /// (premier, nombre), et dit si au moins une l'a été sans relecture.
    fn paint_surfaces(&mut self, ui: &egui::Ui, rect: Rect, g: &Geo) -> (Vec<(usize, usize)>, bool) {
        if !crate::gpudraw::shared() || g.faces.is_empty() || rect.width() < 2.0 || rect.height() < 2.0 {
            return (vec![], false);
        }
        let ppp = ui.ctx().pixels_per_point();
        let size = [(rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32];
        let (vp, depth_row, light, eye) = self.camera(size[0] as f32, size[1] as f32);
        let cd = g.cols.get("Cd").filter(|c| c.width() >= 3 && c.resolved());
        if self.tris.as_ref().is_none_or(|t| t.0 != g.faces_key() || t.1 != g.n) {
            let (tris, _) = g.triangles();
            let mut used = vec![false; g.n];
            for v in tris.iter().flatten() {
                used[*v as usize] = true;
            }
            self.tris = Some((g.faces_key(), g.n, std::sync::Arc::new(tris), std::sync::Arc::new(used)));
        }
        let Some(all_tris) = self.tris.as_ref().map(|t| t.2.clone()) else { return (vec![], false) };
        // Les morceaux : (premier sommet, colonne des positions, tissu dessinable tel quel)
        let parts = g.position_parts();
        let mut ranges = vec![];
        let mut direct = false;
        struct Piece<'a> {
            first: usize,
            col: &'a std::sync::Arc<crate::geo::Column>,
            live: Option<&'a crate::gpucloth::Live>,
        }
        let mut pieces = vec![];
        for (first, col) in &parts {
            let (first, n) = (*first, col.len());
            if !all_tris.iter().any(|t| (t[0] as usize) >= first && (t[0] as usize) < first + n) {
                continue;
            }
            let live = col.positions_of().and_then(|s| s.live.as_ref()).filter(|l| l.current());
            // Ni dans les tampons du solveur, ni encore arrivé : rien à dessiner cette fois-ci
            if live.is_none() && !col.ready() {
                ui.ctx().request_repaint();
                ranges.push((first, n));
                continue;
            }
            direct |= live.is_some();
            ranges.push((first, n));
            pieces.push(Piece { first, col, live });
        }
        if pieces.is_empty() {
            return (ranges, false);
        }
        let key = g.faces_key();
        let tris_of: Vec<Box<dyn Fn() -> Vec<[u32; 3]>>> = pieces
            .iter()
            .map(|p| {
                let (first, n, all) = (p.first as u32, p.col.len() as u32, &all_tris);
                Box::new(move || all.iter().filter(|t| t.iter().all(|v| *v >= first && *v < first + n)).map(|t| [t[0] - first, t[1] - first, t[2] - first]).collect()) as Box<dyn Fn() -> Vec<[u32; 3]>>
            })
            .collect();
        let data_of: Vec<Box<dyn Fn() -> Vec<[f32; 4]>>> = pieces
            .iter()
            .map(|p| {
                let col = p.col;
                Box::new(move || {
                    let d = col.data();
                    (0..col.len()).map(|i| [d[0][i], d[1][i], d[2][i], 0.0]).collect()
                }) as Box<dyn Fn() -> Vec<[f32; 4]>>
            })
            .collect();
        let colors_of: Vec<Box<dyn Fn() -> Vec<[u8; 4]>>> = pieces
            .iter()
            .map(|p| {
                let (first, n) = (p.first, p.col.len());
                Box::new(move || {
                    let Some(d) = cd.map(|c| c.data()) else { return vec![] };
                    let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    (first..first + n).map(|i| [q(d[0][i]), q(d[1][i]), q(d[2][i]), 255]).collect()
                }) as Box<dyn Fn() -> Vec<[u8; 4]>>
            })
            .collect();
        let draw: Vec<crate::gpudraw::Part> = pieces
            .iter()
            .enumerate()
            .map(|(k, p)| crate::gpudraw::Part {
                n: p.col.len(),
                source: match p.live {
                    Some(l) => crate::gpudraw::Source::Live(l),
                    None => crate::gpudraw::Source::Data(p.col.version, &*data_of[k]),
                },
                tris: (key ^ (p.first as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15), &*tris_of[k]),
                colors: cd.map(|c| (c.version ^ (p.first as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15), &*colors_of[k])),
                base: [0.79, 0.71, 0.64],
                model: crate::gpudraw::IDENTITY,
            })
            .collect();
        let cam = crate::gpudraw::Cam { view_proj: vp, depth_row, light, eye, soft: None };
        if !crate::gpudraw::paint(&mut self.canvas, size, &cam, &draw) {
            return (vec![], false);
        }
        (ranges, direct)
    }

    /// Dessine le volume par-dessus l'aperçu. Sur la carte : depuis les tampons du solveur, sans
    /// relecture, à demi-résolution (la fumée est douce). Sans dessin par la carte : un petit
    /// rendu sur le processeur, refait seulement quand le volume ou la caméra changent.
    fn paint_volume(&mut self, ui: &egui::Ui, painter: &egui::Painter, rect: Rect, vol: &crate::volume::Volume) {
        if rect.width() < 2.0 || rect.height() < 2.0 {
            return;
        }
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        if crate::gpudraw::shared() {
            let ppp = ui.ctx().pixels_per_point();
            // Un rendu logiciel n'a pas la force de marcher dans chaque pixel
            let k = if crate::gpusim::hardware() { 0.5 } else { 0.25 };
            let size = [(rect.width() * ppp * k).round().max(1.0) as u32, (rect.height() * ppp * k).round().max(1.0) as u32];
            let (vp, depth_row, light, eye) = self.camera(size[0] as f32, size[1] as f32);
            let cam = crate::gpudraw::Cam { view_proj: vp, depth_row, light, eye, soft: None };
            match crate::gpuvolume::paint_volume(&mut self.vol_canvas, size, &cam, vol, &crate::gpudraw::IDENTITY) {
                Some((_, direct)) => self.direct |= direct,
                // Les voxels ne sont pas encore arrivés : l'image d'avant reste
                None => ui.ctx().request_repaint(),
            }
            if let Some(tex) = self.vol_canvas.texture() {
                painter.image(tex, rect, uv, Color32::WHITE);
            }
            return;
        }
        if !vol.ready() {
            ui.ctx().request_repaint();
        } else {
            let (w, h) = (((rect.width() / 5.0) as usize).clamp(8, 220), ((rect.height() / 5.0) as usize).clamp(8, 160));
            let key = {
                use std::hash::{Hash, Hasher};
                let mut hs = std::collections::hash_map::DefaultHasher::new();
                (vol.version, self.yaw.to_bits(), self.pitch.to_bits(), self.dist.to_bits(), self.target.map(f32::to_bits), w, h).hash(&mut hs);
                hs.finish()
            };
            if self.vol_soft.as_ref().is_none_or(|v| v.0 != key) {
                let (vp, _, light, _) = self.camera(w as f32, h as f32);
                if let Some(inv) = crate::volume::mat_inv(&vp) {
                    let rays = |x: usize, y: usize| {
                        let (nx, ny) = ((x as f32 + 0.5) / w as f32 * 2.0 - 1.0, 1.0 - (y as f32 + 0.5) / h as f32 * 2.0);
                        let (a, b) = (crate::volume::mat_point(&inv, [nx, ny, 0.0]), crate::volume::mat_point(&inv, [nx, ny, 1.0]));
                        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-12);
                        (a, [d[0] / len, d[1] / len, d[2] / len], f32::MAX)
                    };
                    let mut px = vec![0u8; w * h * 4];
                    crate::volume::composite(&mut px, w, h, &[(std::sync::Arc::new(vol.clone()), crate::gpudraw::IDENTITY)], &crate::volume::View { rays: &rays, light });
                    let img = egui::ColorImage::from_rgba_premultiplied([w, h], &px);
                    match &mut self.vol_soft {
                        Some((k, t)) => {
                            t.set(img, egui::TextureOptions::LINEAR);
                            *k = key;
                        }
                        None => self.vol_soft = Some((key, ui.ctx().load_texture("geo-volume", img, egui::TextureOptions::LINEAR))),
                    }
                }
            }
        }
        if let Some((_, t)) = &self.vol_soft {
            painter.image(t.id(), rect, uv, Color32::WHITE);
        }
    }

    /// Projette un point du monde dans le rectangle. Rend aussi sa profondeur (négative : derrière la caméra).
    fn project(&self, rect: Rect, p: [f32; 3]) -> (Pos2, f32) {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let d = [p[0] - self.target[0], p[1] - self.target[1], p[2] - self.target[2]];
        let x = d[0] * cy - d[2] * sy;
        let z = d[0] * sy + d[2] * cy;
        let y = d[1] * cp - z * sp;
        let z = d[1] * sp + z * cp;
        let depth = self.dist - z;
        let f = rect.height().min(rect.width()) * 0.9 / depth.max(0.05);
        (pos2(rect.center().x + x * f, rect.center().y - y * f), depth)
    }

    /// L'aperçu 3D : glisser pour tourner, molette pour s'approcher, double-clic pour recadrer.
    pub fn viewport(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let p = theme::pal();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6.0, Color32::from_gray(26));
        let resp = ui.interact(rect, ui.scope_id().with("geo-view"), Sense::click_and_drag());
        if resp.dragged() || (resp.hovered() && ui.input(|i| i.smooth_scroll_delta.y != 0.0)) {
            self.auto = false;
        }
        if resp.dragged_by(egui::PointerButton::Primary) {
            let d = resp.drag_delta();
            self.yaw += d.x * 0.01;
            self.pitch = (self.pitch + d.y * 0.01).clamp(-1.5, 1.5);
        }
        if resp.dragged_by(egui::PointerButton::Middle) || resp.dragged_by(egui::PointerButton::Secondary) {
            let d = resp.drag_delta();
            let k = self.dist / rect.height().max(1.0);
            let (sy, cy) = self.yaw.sin_cos();
            self.target[0] -= d.x * k * cy;
            self.target[2] += d.x * k * sy;
            self.target[1] += d.y * k;
        }
        if resp.hovered() {
            let sc = ui.input(|i| i.smooth_scroll_delta.y);
            if sc != 0.0 {
                self.dist = (self.dist * (-sc * 0.002).exp()).clamp(0.2, 2000.0);
            }
        }
        if resp.double_clicked() {
            if let Some(g) = self.result.clone() {
                self.seen = None;
                self.framed = false;
                self.want_frame = !self.frame_on(&g);
            }
        }
        // Sol : une grille discrète à la hauteur zéro
        let grid = Color32::from_gray(48);
        let step = 10f32.powf((self.dist * 0.4).max(0.01).log10().round()).max(0.01);
        for k in -5..=5 {
            let t = k as f32 * step;
            let cx = (self.target[0] / step).round() * step;
            let cz = (self.target[2] / step).round() * step;
            for (a, b) in [([cx + t, 0.0, cz - 5.0 * step], [cx + t, 0.0, cz + 5.0 * step]), ([cx - 5.0 * step, 0.0, cz + t], [cx + 5.0 * step, 0.0, cz + t])] {
                let ((pa, da), (pb, db)) = (self.project(rect, a), self.project(rect, b));
                if da > 0.05 && db > 0.05 {
                    painter.line_segment([pa, pb], Stroke::new(1.0, grid));
                }
            }
        }
        // Obstacles des solveurs : en fil de fer, pour voir sur quoi le tissu se pose
        let wire = Stroke::new(1.0, Color32::from_rgba_unmultiplied(110, 150, 220, 120));
        let segment = |a: [f32; 3], b: [f32; 3]| {
            let ((pa, da), (pb, db)) = (self.project(rect, a), self.project(rect, b));
            if da > 0.05 && db > 0.05 {
                painter.line_segment([pa, pb], wire);
            }
        };
        for o in &self.obstacles {
            match *o {
                Obstacle::Cube(c, h) => {
                    let corner = |k: usize| [c[0] + if k & 1 == 0 { -h[0] } else { h[0] }, c[1] + if k & 2 == 0 { -h[1] } else { h[1] }, c[2] + if k & 4 == 0 { -h[2] } else { h[2] }];
                    for k in 0..8usize {
                        for bit in [1, 2, 4] {
                            if k & bit == 0 {
                                segment(corner(k), corner(k | bit));
                            }
                        }
                    }
                }
                Obstacle::Ball(c, r) => {
                    // Trois grands cercles
                    for axis in 0..3 {
                        let at = |t: f32| {
                            let (s, co) = (t * std::f32::consts::TAU).sin_cos();
                            let mut q = c;
                            q[(axis + 1) % 3] += r * co;
                            q[(axis + 2) % 3] += r * s;
                            q
                        };
                        for k in 0..48 {
                            segment(at(k as f32 / 48.0), at((k + 1) as f32 / 48.0));
                        }
                    }
                }
            }
        }
        if let Some(g) = self.result.clone() {
            // Les surfaces d'abord, sur la carte : un tissu encore dans les tampons du solveur ne la quitte pas
            let (on_card, direct) = self.paint_surfaces(ui, rect, &g);
            self.direct = direct;
            let base = Color32::from_rgb(0xe6, 0xdc, 0xd2);
            // Le reste (points, fils, et tout quand la carte ne dessine pas) : ici, morceau par morceau,
            // sans toucher aux morceaux dessinés sur la carte (les lire les ferait revenir au processeur)
            let parts = g.position_parts();
            let part_of = |i: usize| parts.iter().rev().find(|(first, _)| *first <= i).map(|(first, c)| (*first, *c));
            // Un point isolé (sans face) d'un morceau dessiné sur la carte reste à dessiner ici
            let in_face = self.tris.as_ref().filter(|t| t.0 == g.faces_key() && t.1 == g.n).map(|t| t.3.clone());
            let shown = |i: usize| !(on_card.iter().any(|(first, n)| i >= *first && i < first + n) && in_face.as_ref().is_none_or(|m| m[i]));
            let pos = |i: usize| -> [f32; 3] {
                match part_of(i) {
                    Some((first, c)) => {
                        let d = c.data();
                        [d[0][i - first], d[1][i - first], d[2][i - first]]
                    }
                    None => [0.0; 3],
                }
            };
            let cd = g.cols.get("Cd").filter(|c| c.width() >= 3);
            let color = |i: usize| match cd {
                Some(c) => {
                    let v = c.get(i);
                    let q = |x: f32| (x.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0 + 0.5) as u8;
                    Color32::from_rgb(q(v[0]), q(v[1]), q(v[2]))
                }
                None => base,
            };
            let mut mesh = egui::Mesh::default();
            // Arêtes : de fins traits, en un seul maillage
            let stride_e = (g.edges.len() / 60_000).max(1);
            let line = Color32::from_rgba_unmultiplied(150, 160, 175, 150);
            for e in g.edges.iter().step_by(stride_e) {
                let (a, b) = (e[0] as usize, e[1] as usize);
                if a >= g.n || b >= g.n || !shown(a) || !shown(b) {
                    continue;
                }
                let ((pa, da), (pb, db)) = (self.project(rect, pos(a)), self.project(rect, pos(b)));
                if da <= 0.05 || db <= 0.05 {
                    continue;
                }
                let dir = pb - pa;
                let len = dir.length();
                if len < 0.01 {
                    continue;
                }
                let nrm = vec2(-dir.y, dir.x) / len * 0.5;
                let i = mesh.vertices.len() as u32;
                for q in [pa + nrm, pb + nrm, pb - nrm, pa - nrm] {
                    mesh.colored_vertex(q, line);
                }
                mesh.indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
            }
            // Points : de petits carrés, plus gros quand ils sont peu nombreux
            let stride = (g.n / 150_000).max(1);
            let count = g.n / stride;
            let size = if count > 40_000 { 1.0 } else if count > 4_000 { 1.4 } else if g.edges.is_empty() { 2.4 } else { 1.8 };
            for i in (0..g.n).step_by(stride) {
                if !shown(i) {
                    continue;
                }
                let (q, depth) = self.project(rect, pos(i));
                if depth <= 0.05 || !rect.expand(4.0).contains(q) {
                    continue;
                }
                let c = color(i);
                let k = mesh.vertices.len() as u32;
                for o in [vec2(-size, -size), vec2(size, -size), vec2(size, size), vec2(-size, size)] {
                    mesh.colored_vertex(q + o, c);
                }
                mesh.indices.extend_from_slice(&[k, k + 1, k + 2, k, k + 2, k + 3]);
            }
            painter.add(egui::Shape::mesh(mesh));
            // L'image des surfaces, par-dessus
            if let Some(tex) = self.canvas.texture().filter(|_| !on_card.is_empty()) {
                painter.image(tex, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            // La fumée et le feu, par-dessus tout
            if let Some(vol) = g.volume.clone().filter(|v| !v.blank()) {
                self.paint_volume(ui, &painter, rect, &vol);
            }
        }
        // Ce qu'on regarde, et ce que le calcul a coûté
        if let Some((_, e)) = &self.error {
            let gal = painter.layout(format!("{}  {}", crate::lucide::TRIANGLE_ALERT, crate::i18n::t(e)), egui::FontId::proportional(11.5), p.missing, rect.width() - 20.0);
            painter.galley(rect.left_top() + vec2(10.0, 8.0), gal, p.missing);
        } else if let Some(g) = &self.result {
            let mut line = format!("{} {}", g.n, crate::i18n::t_mid("points"));
            if !g.edges.is_empty() {
                line += &format!(" · {} {}", g.edges.len(), crate::i18n::t_mid("arêtes"));
            }
            if let Some(v) = &g.volume {
                if g.n == 0 {
                    line.clear();
                } else {
                    line += " · ";
                }
                line += &format!("{} × {} × {} {}", v.res[0], v.res[1], v.res[2], crate::i18n::t_mid("voxels"));
            }
            painter.text(rect.left_top() + vec2(10.0, 8.0), egui::Align2::LEFT_TOP, line, egui::FontId::proportional(11.5), Color32::from_gray(190));
            let s = self.stats;
            let mut cost = format!("{} {} · {} {}", s.computed, crate::i18n::t_mid("refaits"), s.reused, crate::i18n::t_mid("repris du cache"));
            if s.steps > 0 {
                cost += &format!(" · {} {}", s.steps, crate::i18n::t_mid("pas simulés"));
            }
            cost += &format!(" · {:.1} ms", self.ms);
            if self.direct {
                cost += &format!(" · {}", crate::i18n::t_mid("sans relecture"));
            }
            // La fumée : où elle se calcule, et ce qui reste de divergence après la pression
            if let Some(st) = crate::geo::smoke_status().filter(|_| g.volume.is_some() && s.steps > 0) {
                cost += &format!(" · {} {}", crate::i18n::t_mid("fumée"), crate::i18n::t_mid(if st.on_gpu { "sur la carte graphique" } else { "sur le processeur" }));
                if st.ms > 0.0 {
                    cost += &format!(", {:.1} ms, {} {:.1e}", st.ms, crate::i18n::t_mid("divergence"), st.after);
                }
            }
            painter.text(rect.left_bottom() + vec2(10.0, -8.0), egui::Align2::LEFT_BOTTOM, cost, egui::FontId::proportional(10.5), Color32::from_gray(130));
        }
        if self.engine.incomplete {
            painter.text(rect.right_top() + vec2(-46.0, 10.0), egui::Align2::RIGHT_TOP, crate::i18n::t("simulation en cours…"), egui::FontId::proportional(11.0), p.accent);
        }
        resp.on_hover_text(crate::i18n::t("Glisser : tourner · clic droit glissé : déplacer · molette : zoom · double-clic : recadrer"));
    }

    /// La puce du solveur, en haut à droite de l'aperçu : elle dit où se fait le calcul du tissu
    /// (processeur ou carte graphique) et ouvre ses réglages. Rend vrai si le graphe a changé.
    pub fn solver_chip(&mut self, ui: &mut egui::Ui, rect: Rect, graph: &mut Graph) -> bool {
        let ids: Vec<u32> = graph.nodes.iter().filter(|n| n.op == "sim_solver").map(|n| n.id).collect();
        if ids.is_empty() {
            return false;
        }
        let p = theme::pal();
        let t = crate::i18n::t;
        let on_card = crate::geo::SOLVER_NAMES[1];
        let choice = |n: &crate::nodes::Node| matches!(n.params.get("solveur"), Some(crate::nodes::Param::S(s)) if s == on_card);
        let wants_card = graph.nodes.iter().any(|n| n.op == "sim_solver" && choice(n));
        let status = crate::geo::cloth_status().filter(|_| wants_card);
        let runs_on_card = status.as_ref().is_some_and(|s| s.on_gpu);
        let area = Rect::from_min_size(pos2(rect.right() - 38.0, rect.top() + 5.0), vec2(32.0, 24.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(egui::Layout::right_to_left(egui::Align::Center)));
        let icon = if wants_card { crate::lucide::GPU } else { crate::lucide::CPU };
        let mut changed = false;
        // Le menu reste ouvert quand on change de solveur : on règle, on regarde, on règle encore
        let button = egui::containers::menu::MenuButton::new(RichText::new(icon).size(15.0).color(if runs_on_card { p.accent } else { Color32::from_gray(190) })).config(egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside));
        let (resp, _) = button.ui(&mut child, |ui| {
            ui.set_min_width(270.0);
            ui.set_max_width(290.0);
            ui.spacing_mut().item_spacing.y = 5.0;
            for (rank, id) in ids.iter().enumerate() {
                let linked: Vec<bool> = (0..20).map(|k| graph.links.iter().any(|l| l.to == *id && l.inp == k)).collect();
                let Some(n) = graph.nodes.iter_mut().find(|n| n.id == *id) else { continue };
                if let Some(d) = n.def() {
                    while n.values.len() < d.inputs.len() {
                        n.values.push(d.inputs[n.values.len()].2);
                    }
                }
                if rank > 0 {
                    ui.separator();
                }
                let card = choice(n);
                ui.label(RichText::new(if n.label.is_empty() { t("solveur de tissu") } else { n.label.clone() }).size(12.0).strong());
                // Où calculer : deux puces
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (to_card, icon, label, tip) in [(false, crate::lucide::CPU, "processeur", "XPBD : contraintes de distance, calculées sur le processeur"), (true, crate::lucide::GPU, "carte graphique", "VBD : raideur et flexion réglables, calculé sur la carte graphique (sur le processeur s'il n'y en a pas)")] {
                        let b = egui::Button::new(RichText::new(format!("{icon}  {}", t(label))).size(12.0)).selected(card == to_card).corner_radius(11.0).min_size(vec2(0.0, 24.0));
                        if ui.add(b).on_hover_text(t(tip)).clicked() && card != to_card {
                            n.params.insert("solveur".into(), crate::nodes::Param::S(crate::geo::SOLVER_NAMES[to_card as usize].to_string()));
                            changed = true;
                        }
                    }
                });
                ui.add_space(2.0);
                let mut slide = |ui: &mut egui::Ui, k: usize, label: &str, tip: &str, range: std::ops::RangeInclusive<f32>, log: bool, whole: bool| {
                    if linked.get(k).copied().unwrap_or(false) || k >= n.values.len() {
                        return;
                    }
                    let mut v = n.values[k][0];
                    let mut s = egui::Slider::new(&mut v, range).logarithmic(log).smallest_positive(0.01);
                    if whole {
                        s = s.integer();
                    }
                    let r = ui.scope(|ui| crate::modifiers::row_slider(ui, label, s)).response.on_hover_text(t(tip));
                    let _ = r;
                    if v != n.values[k][0] {
                        n.values[k] = [v; 4];
                        changed = true;
                    }
                };
                slide(ui, inp::ITERATIONS, "itérations", "Passes de calcul par image : plus il y en a, moins le tissu paraît élastique", 1.0..=256.0, true, true);
                if card {
                    slide(ui, inp::SUBSTEPS, "sous-pas", "L'image est coupée en sous-pas : beaucoup de sous-pas et peu d'itérations chacun donnent le tissu le plus ferme", 1.0..=32.0, false, true);
                    slide(ui, inp::STIFFNESS, "raideur", "Résistance à l'étirement, en N/m : 50 pour un jersey, 1000 pour un coton, 5000 pour un jean", 10.0..=20000.0, true, false);
                    slide(ui, inp::BENDING, "flexion", "Résistance au pli : 0 pour une soie, 0,1 pour un coton, 1 pour une toile épaisse, 10 pour un carton", 0.0..=10.0, true, false);
                } else {
                    slide(ui, inp::COMPLIANCE, "souplesse", "0 : le tissu ne s'étire pas ; plus grand : il devient élastique", 0.0..=0.01, false, false);
                }
                if card {
                    slide(ui, inp::SHEAR, "cisaillement", "Résistance à la déformation en biais, en part de la raideur : 0,1 pour un tissu tissé qui épouse les formes, 1 pour du papier", 0.01..=1.0, true, false);
                    slide(ui, inp::CALM, "amortissement des plis", "Les plis cessent de frémir : 0 aucun, 0,3 un coton, 1 très amorti. Ne ralentit ni la chute ni le balancement", 0.0..=1.0, false, false);
                }
                slide(ui, inp::DAMPING, "amortissement", "Part de vitesse perdue à chaque image, comme un frein de l'air", 0.0..=0.5, false, false);
                slide(ui, inp::GRIP, "frottement", if card { "Au contact des obstacles : 0 glisse, 0,4 un coton sur du bois, 1 accroche fort" } else { "Au contact des obstacles : 0 glisse, 1 colle" }, 0.0..=1.0, false, false);
                if card {
                    // Auto-collision : une puce, puis l'épaisseur
                    let on = matches!(n.params.get("auto-collision"), Some(crate::nodes::Param::S(s)) if s == "oui");
                    let b = egui::Button::new(RichText::new(format!("{}  {}", crate::lucide::LAYERS, t("auto-collision"))).size(12.0)).selected(on).corner_radius(11.0).min_size(vec2(0.0, 24.0));
                    let flip = ui.add(b).on_hover_text(t("Le tissu ne se traverse plus lui-même quand il se replie (plus lent)")).clicked();
                    if on {
                        slide(ui, inp::THICKNESS, "épaisseur", "Distance gardée entre deux couches du tissu, en mètres. 0 : choisie d'après la taille des mailles", 0.0..=0.2, false, false);
                    }
                    if flip {
                        n.params.insert("auto-collision".into(), crate::nodes::Param::S(if on { "non" } else { "oui" }.to_string()));
                        changed = true;
                    }
                }
            }
            // Ce qu'a fait le dernier pas
            ui.separator();
            let note = match &status {
                Some(s) if s.on_gpu => format!("{}  {}\n{} {} · {} {} · {} × {} {} · {:.1} ms", crate::lucide::GPU, s.card, s.points, crate::i18n::t_mid("points"), s.colors, crate::i18n::t_mid("couleurs"), s.substeps, s.per, crate::i18n::t_mid("passes"), s.ms),
                Some(s) => format!("{}  {}\n{} {} · {} × {} {} · {:.1} ms", crate::lucide::CPU, t("pas de carte graphique utilisable : tissu calculé sur le processeur"), s.points, crate::i18n::t_mid("points"), s.substeps, s.per, crate::i18n::t_mid("passes"), s.ms),
                None if wants_card => t("lance la lecture pour voir où se fait le calcul"),
                None => format!("{}  {}", crate::lucide::CPU, t("calcul sur le processeur")),
            };
            ui.label(RichText::new(note).size(11.0).color(p.muted));
        });
        resp.on_hover_text(match (&status, wants_card) {
            (Some(s), _) if s.on_gpu => format!("{} : {}", t("Tissu calculé sur la carte graphique"), s.card),
            (_, true) => t("Tissu : solveur pour carte graphique (réglages)"),
            _ => t("Tissu calculé sur le processeur (réglages, passage sur la carte graphique)"),
        });
        changed
    }

    /// La barre de lecture : première image, lecture, image courante, réglages ; dessous, les images en cache.
    /// Rend vrai si les réglages de temps du graphe ont changé.
    pub fn transport(&mut self, ui: &mut egui::Ui, rect: Rect, time: &mut TimeRange) -> bool {
        let p = theme::pal();
        let mut changed = false;
        let bar = Rect::from_min_size(rect.min, vec2(rect.width(), 24.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)));
        child.spacing_mut().item_spacing.x = 2.0;
        let icon = |ui: &mut egui::Ui, icon: &str, tip: &str| ui.add(egui::Button::new(RichText::new(icon).size(14.0)).frame(false).min_size(vec2(24.0, 22.0))).on_hover_text(crate::i18n::t(tip)).clicked();
        if icon(&mut child, crate::lucide::SKIP_BACK, "Revenir à la première image") {
            self.frame = time.start;
            self.playing = false;
        }
        if icon(&mut child, if self.playing { crate::lucide::PAUSE } else { crate::lucide::PLAY }, "Lire ou mettre en pause (Espace, souris sur l'aperçu)") {
            self.playing = !self.playing;
        }
        child.add_space(4.0);
        child.add(egui::DragValue::new(&mut self.frame).range(time.start..=time.end).speed(0.2)).on_hover_text(crate::i18n::t("Image courante (glisser pour parcourir)"));
        child.label(RichText::new(format!("/ {}", time.end)).size(11.0).color(p.muted));
        child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::containers::menu::MenuButton::new(RichText::new(crate::lucide::SETTINGS).size(13.0)).ui(ui, |ui| {
                ui.set_min_width(210.0);
                changed |= crate::modifiers::row(ui, "première image", |ui| ui.add(egui::DragValue::new(&mut time.start).speed(0.2)).changed());
                changed |= crate::modifiers::row(ui, "dernière image", |ui| ui.add(egui::DragValue::new(&mut time.end).speed(0.2)).changed());
                changed |= crate::modifiers::row(ui, "images par seconde", |ui| ui.add(egui::DragValue::new(&mut time.fps).range(1.0..=240.0).speed(0.1)).changed());
                ui.separator();
                ui.label(RichText::new(format!("{} : {:.1} Mo", crate::i18n::t("Cache en mémoire"), self.engine.cached_bytes() as f32 / 1e6)).size(11.0).color(p.muted));
                if ui.button(format!("{}  {}", crate::lucide::TRASH_2, crate::i18n::t("vider le cache"))).clicked() {
                    self.engine.clear();
                    self.key = None;
                    ui.close();
                }
            }).0.on_hover_text(crate::i18n::t("Durée, cadence et cache"));
            if icon(ui, crate::lucide::SCAN, "Recadrer l'aperçu sur la géométrie") {
                self.framed = false;
                self.key = None;
            }
        });
        if time.end < time.start {
            time.end = time.start;
        }
        // Barre de cache : chaque image de simulation déjà calculée est teintée ; le trait marque l'image courante.
        // Cliquer ou glisser dessus déplace la lecture.
        let strip = Rect::from_min_size(pos2(rect.left(), bar.bottom() + 3.0), vec2(rect.width(), 8.0));
        let resp = ui.interact(strip.expand2(vec2(0.0, 3.0)), ui.scope_id().with("geo-cache-bar"), Sense::click_and_drag());
        let span = (time.end - time.start).max(1) as f32;
        ui.painter().rect_filled(strip, 3.0, p.field);
        let x_of = |f: i32| strip.left() + (f - time.start) as f32 / span * strip.width();
        for end in &self.zones {
            for f in self.engine.cached_frames(*end) {
                let x0 = x_of(f - 1).max(strip.left());
                let x1 = x_of(f).min(strip.right());
                if x1 > x0 {
                    ui.painter().rect_filled(Rect::from_min_max(pos2(x0, strip.top() + 2.0), pos2(x1 + 0.5, strip.bottom() - 2.0)), 0.0, if self.engine.baked.contains_key(end) { p.success } else { p.accent.gamma_multiply(0.75) });
                }
            }
            if self.engine.baked.contains_key(end) {
                ui.painter().rect_filled(Rect::from_min_max(pos2(strip.left(), strip.top() + 2.0), pos2(strip.right(), strip.bottom() - 2.0)), 0.0, p.success.gamma_multiply(0.55));
            }
        }
        let x = x_of(self.frame);
        ui.painter().line_segment([pos2(x, strip.top() - 2.0), pos2(x, strip.bottom() + 2.0)], Stroke::new(2.0, p.text));
        if let Some(at) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
            self.frame = (time.start as f32 + ((at.x - strip.left()) / strip.width()).clamp(0.0, 1.0) * span).round() as i32;
            self.playing = false;
        }
        resp.on_hover_text(crate::i18n::t("Images de simulation en cache. Cliquer ou glisser pour se déplacer dans le temps."));
        changed
    }

    /// Miniature d'un node : ce qui en sort, vu de face. Calculée quand le graphe ou l'image changent.
    pub fn thumb(&mut self, painter: &egui::Painter, rect: Rect, doc: &Graph, id: u32, version: u64, root: &str) {
        let p = theme::pal();
        let key = (version, self.frame);
        if self.thumbs.get(&id).is_none_or(|t| t.key != key || (t.stale && !self.playing)) {
            let saved = (self.engine.stats, self.engine.incomplete, self.engine.deadline.take());
            // Une miniature ne lance pas une simulation : elle prend ce qui est déjà en cache
            self.engine.deadline = Some(std::time::Instant::now() + std::time::Duration::from_millis(4));
            let r = self.engine.eval_node(doc, id, 0, self.frame, root);
            (self.engine.stats, self.engine.incomplete, self.engine.deadline) = saved;
            // Géométrie encore sur la carte : pendant la lecture, une miniature ne la fait pas
            // revenir au processeur. Elle garde ses points d'avant, et se met à jour à l'arrêt.
            if let Some(g) = r.as_ref().ok().filter(|g| !g.resolved() && (self.playing || !g.ready())) {
                match self.thumbs.get_mut(&id) {
                    Some(t) => (t.key, t.stale, t.count, t.error) = (key, true, g.n, false),
                    None => {
                        self.thumbs.insert(id, Thumb { key, stale: true, pts: vec![], lines: vec![], count: g.n, error: false });
                    }
                }
                if !self.playing {
                    painter.ctx().request_repaint();
                }
                return self.thumb_paint(painter, rect, id);
            }
            let mut t = Thumb { key, stale: false, pts: vec![], lines: vec![], count: 0, error: r.is_err() };
            if let Ok(g) = r {
                t.count = g.n;
                if let Some((lo, hi)) = g.bounds() {
                    // Vue de trois quarts, cadrée sur la boîte englobante
                    let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
                    let r = (((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt() * 0.5).max(1e-3);
                    let (sy, cy) = 0.6f32.sin_cos();
                    let (sp, cp) = 0.4f32.sin_cos();
                    let flat = |q: [f32; 3]| -> [f32; 2] {
                        let d = [(q[0] - c[0]) / r, (q[1] - c[1]) / r, (q[2] - c[2]) / r];
                        let x = d[0] * cy - d[2] * sy;
                        let z = d[0] * sy + d[2] * cy;
                        [0.5 + x * 0.48, 0.5 - (d[1] * cp - z * sp) * 0.48]
                    };
                    let base = Color32::from_rgb(0xe6, 0xdc, 0xd2);
                    let stride = (g.n / 500).max(1);
                    t.pts = (0..g.n).step_by(stride).map(|i| (flat(g.pos(i)), point_color(&g, i, base))).collect();
                    let stride_e = (g.edges.len() / 500).max(1);
                    t.lines = g.edges.iter().step_by(stride_e).filter(|e| (e[0] as usize) < g.n && (e[1] as usize) < g.n).map(|e| [flat(g.pos(e[0] as usize)), flat(g.pos(e[1] as usize))]).collect();
                }
            }
            self.thumbs.insert(id, t);
        }
        self.thumb_paint(painter, rect, id);
    }

    fn thumb_paint(&self, painter: &egui::Painter, rect: Rect, id: u32) {
        let p = theme::pal();
        let Some(t) = self.thumbs.get(&id) else { return };
        painter.rect_filled(rect, 4.0, Color32::from_gray(30));
        let sq = Rect::from_center_size(rect.center(), vec2(rect.height(), rect.height()));
        let at = |q: [f32; 2]| pos2(sq.left() + q[0] * sq.width(), sq.top() + q[1] * sq.height());
        for l in &t.lines {
            painter.line_segment([at(l[0]), at(l[1])], Stroke::new(0.6, Color32::from_rgba_unmultiplied(150, 160, 175, 110)));
        }
        for (q, c) in &t.pts {
            painter.rect_filled(Rect::from_center_size(at(*q), vec2(1.6, 1.6)), 0.0, *c);
        }
        let note = if t.error { crate::i18n::t("rien à montrer") } else { format!("{} {}", t.count, crate::i18n::t_mid("points")) };
        painter.text(rect.left_bottom() + vec2(5.0, -3.0), egui::Align2::LEFT_BOTTOM, note, egui::FontId::proportional(9.5), p.muted);
    }

    /// Tableau des attributs de ce qui sort d'un node : nom, genre, étendue des valeurs.
    pub fn attributes(&mut self, ui: &mut egui::Ui, doc: &Graph, id: u32, version: u64, root: &str) {
        let p = theme::pal();
        let key = (version, self.frame, id);
        // Pendant la lecture, le tableau ne fait pas revenir au processeur une géométrie encore
        // sur la carte : il garde ses valeurs, et se met à jour à l'arrêt
        let held = self.playing && self.table.as_ref().is_some_and(|t| t.1.is_ok());
        if self.table.as_ref().map(|t| t.0) != Some(key) && !held {
            let saved = (self.engine.stats, self.engine.incomplete, self.engine.deadline.take());
            self.engine.deadline = Some(std::time::Instant::now() + std::time::Duration::from_millis(4));
            let is_out = doc.nodes.iter().any(|n| n.id == id && n.op == "geo_out");
            let r = if is_out { self.engine.eval(doc, self.frame, root) } else { self.engine.eval_node(doc, id, 0, self.frame, root) };
            (self.engine.stats, self.engine.incomplete, self.engine.deadline) = saved;
            self.table = Some((key, r.map_err(|e| e.1)));
        }
        let Some((_, Ok(g))) = &self.table else { return };
        egui::Frame::new().fill(p.field).corner_radius(8).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(theme::section(&crate::i18n::t("ATTRIBUTS")));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{} {}", g.n, crate::i18n::t_mid("points"))).size(11.0).color(p.muted));
                });
            });
            ui.add_space(2.0);
            let row = |ui: &mut egui::Ui, name: &str, kind: &str, range: String| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(RichText::new(name).monospace().size(11.5));
                    ui.label(RichText::new(crate::i18n::t_mid(kind)).size(10.5).color(p.muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(range).size(10.5).color(p.muted));
                    });
                });
            };
            for (name, c) in &g.cols {
                let kind = match c.width() {
                    1 => "nombre à virgule",
                    3 => "vecteur",
                    4 => "couleur",
                    _ => "valeurs",
                };
                let (mut lo, mut hi) = (f32::MAX, f32::MIN);
                for comp in c.data().iter().take(3) {
                    for x in comp {
                        lo = lo.min(*x);
                        hi = hi.max(*x);
                    }
                }
                let range = if lo <= hi { format!("{lo:.2} … {hi:.2}") } else { String::new() };
                row(ui, name, kind, range);
            }
            if !g.edges.is_empty() {
                row(ui, crate::i18n::t_mid("arêtes").as_str(), "", format!("{}", g.edges.len()));
            }
            if g.cols.is_empty() {
                ui.label(RichText::new(crate::i18n::t("Aucun point pour l'instant.")).size(11.0).color(p.muted));
            }
        });
        ui.add_space(6.0);
    }

    /// Oublie ce qui est affiché : au prochain passage, tout est redemandé au moteur.
    pub fn refresh(&mut self) {
        self.key = None;
        self.table = None;
        self.thumbs.clear();
    }
}
