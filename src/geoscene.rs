//! Géométrie de nodes dans une scène 3D : un objet de scène dont le fichier est un `.arygeo`.
//!
//! La scène pilote le graphe : son image courante donne l'image de la simulation (`sim_frame`),
//! le résultat du graphe devient un maillage comme les autres (`model`), et les objets de la scène
//! que le graphe regarde (node « Objet de la scène ») lui sont donnés sous forme de boîtes
//! (`inputs`). Un moteur par fichier est gardé d'un appel à l'autre : lire la timeline dans
//! l'ordre ne calcule qu'un pas de simulation par image.

use crate::geo::{Engine, Geo, SceneInputs};
use crate::nodeflow::TimeRange;
use crate::nodes::Graph;
use crate::scene::{Kind, Scene};
use eframe::egui::Color32;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Ce fichier est-il une géométrie de nodes ?
pub fn is_geo(file: &str) -> bool {
    crate::formats::has(file, "geo")
}

/// L'image de la simulation qui correspond à une image de la scène. La simulation part à l'image
/// `scene_start + offset` de la scène ; avant, elle reste à sa première image ; après sa dernière
/// image, elle y reste. Si les deux n'ont pas la même cadence, c'est le même instant qui est montré.
pub fn sim_frame(scene_frame: f32, scene_start: i32, scene_fps: f32, offset: i32, time: TimeRange) -> i32 {
    let rel = scene_frame - (scene_start + offset) as f32;
    let f = time.start as f32 + rel * time.fps.max(1e-3) / scene_fps.max(1e-3);
    (f.round() as i32).clamp(time.start.min(time.end), time.end.max(time.start))
}

/// L'inverse : l'image de la scène où se trouve une image de la simulation.
pub fn scene_frame_of(sim: i32, scene_start: i32, scene_fps: f32, offset: i32, time: TimeRange) -> f32 {
    (scene_start + offset) as f32 + (sim - time.start) as f32 * scene_fps.max(1e-3) / time.fps.max(1e-3)
}

struct Entry {
    mtime: Option<std::time::SystemTime>,
    graph: Graph,
    engine: Engine,
}

static ENGINES: Mutex<Option<HashMap<PathBuf, Entry>>> = Mutex::new(None);
/// Durée de la simulation de chaque fichier connu. À part des moteurs : l'interface la lit à chaque
/// image, et ne doit pas attendre la fin d'un calcul qui tient les moteurs.
static TIMES: Mutex<Option<HashMap<PathBuf, TimeRange>>> = Mutex::new(None);

/// Le graphe a-t-il un node « Objet de la scène » ? Rend les noms d'objets qu'il regarde.
fn watched(g: &Graph) -> Vec<String> {
    let mut names: Vec<String> = g
        .nodes
        .iter()
        .chain(g.groups.iter().flat_map(|gr| gr.nodes.iter()))
        .filter(|n| n.op == "geo_scene_obj" && !n.muted)
        .filter_map(|n| match n.params.get("objet") {
            Some(crate::nodes::Param::S(s)) if !s.trim().is_empty() => Some(s.clone()),
            _ => None,
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Les boîtes des objets que le graphe regarde, dans le repère de l'objet `k` (la géométrie), à
/// chaque image de la simulation. Un objet est vu par la boîte de son modèle d'origine (sans ses
/// modificateurs ni son squelette), alignée sur les axes de la géométrie.
pub fn inputs(scene: &Scene, scene_path: &Path, k: usize, graph: &Graph) -> Option<Arc<SceneInputs>> {
    let names = watched(graph);
    if names.is_empty() {
        return None;
    }
    let time = graph.time.unwrap_or_default();
    let offset = scene.objects.get(k)?.sim_offset;
    let mut out = SceneInputs { start: time.start, objects: Default::default() };
    // La boîte du modèle de chaque objet regardé, dans son propre repère (Y en haut)
    let mut boxes: Vec<(String, usize, [f32; 3], [f32; 3])> = vec![];
    for name in names {
        let Some(j) = scene.objects.iter().position(|o| o.name == name) else { continue };
        let o = &scene.objects[j];
        // Une géométrie de nodes ne se regarde pas elle-même, ni une autre (elles tourneraient en rond)
        if j == k || crate::scene::kind(o) != Kind::Model || is_geo(&o.file) {
            continue;
        }
        let Ok(m) = crate::scene::source_model(scene_path, o) else { continue };
        let up = |p: [f32; 3]| if m.z_up { [p[0], p[2], -p[1]] } else { p };
        let (a, b) = (up(m.min), up(m.max));
        let lo = [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])];
        let hi = [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])];
        boxes.push((name, j, lo, hi));
    }
    if boxes.is_empty() {
        return Some(Arc::new(out));
    }
    let mut sc = scene.clone();
    for f in time.start..=time.end.max(time.start) {
        sc.apply_frame(scene_frame_of(f, scene.start, scene.fps, offset, time));
        // Du monde vers le repère de la géométrie
        let origin = sc.world(k, [0.0; 3]);
        let cols = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(|e| {
            let w = sc.world(k, e);
            [w[0] - origin[0], w[1] - origin[1], w[2] - origin[2]]
        });
        let inv = crate::meshedit::inverse(cols);
        for (name, j, lo, hi) in &boxes {
            let (mut a, mut b) = ([f32::MAX; 3], [f32::MIN; 3]);
            for corner in 0..8usize {
                let p = [if corner & 1 == 0 { lo[0] } else { hi[0] }, if corner & 2 == 0 { lo[1] } else { hi[1] }, if corner & 4 == 0 { lo[2] } else { hi[2] }];
                let w = sc.world(*j, p);
                let d = [w[0] - origin[0], w[1] - origin[1], w[2] - origin[2]];
                let q = inv.as_ref().map(|m| crate::meshedit::apply(m, d)).unwrap_or(d);
                for a3 in 0..3 {
                    a[a3] = a[a3].min(q[a3]);
                    b[a3] = b[a3].max(q[a3]);
                }
            }
            let center = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0];
            let size = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            out.objects.entry(name.clone()).or_default().push((center, size));
        }
    }
    Some(Arc::new(out))
}

/// Calcule la géométrie d'un fichier `.arygeo` à une image de sa simulation. Le graphe est relu
/// quand son fichier change ; les caches figés sur le disque (« figer » dans l'onglet Simulation)
/// sont lus tels quels quand ils existent.
pub fn eval_file(file: &Path, pick: &dyn Fn(&Graph) -> (i32, Option<Arc<SceneInputs>>)) -> Result<Geo, String> {
    let mtime = std::fs::metadata(file).and_then(|m| m.modified()).ok();
    let mut all = ENGINES.lock().unwrap_or_else(|e| e.into_inner());
    let map = all.get_or_insert_with(HashMap::new);
    if map.get(file).is_none_or(|e| e.mtime != mtime) {
        if map.len() > 16 {
            map.clear();
        }
        let graph = Graph::load(file)?;
        if graph.kind != crate::nodes::GraphKind::Geometry {
            return Err("ce fichier n'est pas une géométrie de nodes".into());
        }
        map.insert(file.to_path_buf(), Entry { mtime, graph, engine: Engine::default() });
    }
    let Some(e) = map.get_mut(file) else { return Err("géométrie introuvable".into()) };
    let root = crate::nodes::vault_root();
    // Caches figés, d'après le réglage « cache » des fins de zone
    e.engine.baked.clear();
    for n in e.graph.nodes.iter().filter(|n| n.op == "sim_out") {
        if let Some(crate::nodes::Param::S(rel)) = n.params.get("cache") {
            let dir = Path::new(&root).join(rel);
            if !rel.is_empty() && !root.is_empty() && dir.is_dir() {
                e.engine.baked.insert(n.id, dir);
            }
        }
    }
    TIMES.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(file.to_path_buf(), e.graph.time.unwrap_or_default());
    let (frame, scene) = pick(&e.graph);
    e.engine.scene = scene;
    e.engine.deadline = None;
    e.engine.eval(&e.graph, frame, &root).map_err(|(_, msg)| msg)
}

/// La durée de la simulation d'un fichier `.arygeo` (pour l'afficher dans la scène).
pub fn time_of(file: &Path) -> Option<TimeRange> {
    TIMES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(file)).copied()
}

/// La géométrie de l'objet `k` de la scène, à l'image courante de la scène (`scene.frame`).
pub fn geo_of(scene: &Scene, scene_path: &Path, k: usize) -> Result<Geo, String> {
    let o = scene.objects.get(k).ok_or("objet introuvable")?;
    let file = crate::scene::resolve(scene_path, &o.file);
    eval_file(&file, &|g| (sim_frame(scene.frame.0, scene.start, scene.fps, o.sim_offset, g.time.unwrap_or_default()), inputs(scene, scene_path, k, g)))
}

/// Le maillage de l'objet `k` : ce que la scène affiche, ombre et rend.
pub fn model(scene: &Scene, scene_path: &Path, k: usize) -> Result<Arc<crate::model3d::Model>, String> {
    to_model(&geo_of(scene, scene_path, k)?).map(Arc::new)
}

/// Dernier maillage connu d'un fichier, hors de toute timeline (première image) : sert aux outils
/// qui demandent « le modèle de l'objet » sans connaître la scène.
pub fn still(file: &Path) -> Result<Arc<crate::model3d::Model>, String> {
    to_model(&eval_file(file, &|g| (g.time.unwrap_or_default().start, None))?).map(Arc::new)
}

/// Transforme une géométrie en maillage. Ses faces donnent les triangles (un quad en donne deux,
/// sa diagonale n'est pas une arête), `Cd` la couleur de chaque triangle, `uv` ses UV. Une
/// géométrie sans face (particules, fils) est montrée par un petit tétraèdre à chaque point.
pub fn to_model(g: &Geo) -> Result<crate::model3d::Model, String> {
    if g.n == 0 {
        return Err("cette géométrie est vide à cette image".into());
    }
    let p = g.vec3("P");
    let mut pos: Vec<[f32; 3]> = (0..g.n).map(|i| [p[0][i], p[1][i], p[2][i]]).collect();
    let (mut tris, mut sides) = g.triangles();
    let srgb = |x: f32| (x.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0 + 0.5) as u8;
    let cd = g.col("Cd").filter(|c| c.width() >= 3);
    let color_of = |i: usize| cd.map(|c| [c.data()[0][i], c.data()[1][i], c.data()[2][i]]);
    let mut colors: Option<Vec<Color32>> = None;
    let mut uv = None;
    if tris.is_empty() {
        // Pas de surface : un tétraèdre par point, assez petit pour ne pas se toucher
        let (lo, hi) = g.bounds().unwrap_or(([0.0; 3], [1.0; 3]));
        let diag = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt().max(1e-3);
        let r = (diag * 0.006).max(1e-4);
        let stride = (g.n / 100_000).max(1);
        let centers: Vec<usize> = (0..g.n).step_by(stride).collect();
        let mut out = Vec::with_capacity(centers.len() * 4);
        let mut cols = Vec::with_capacity(centers.len() * 4);
        for &i in &centers {
            let c = pos[i];
            let b = out.len() as u32;
            out.extend([[c[0], c[1] + r, c[2]], [c[0] - r, c[1] - r * 0.6, c[2] - r * 0.6], [c[0] + r, c[1] - r * 0.6, c[2] - r * 0.6], [c[0], c[1] - r * 0.6, c[2] + r]]);
            tris.extend([[b, b + 2, b + 1], [b, b + 3, b + 2], [b, b + 1, b + 3], [b + 1, b + 2, b + 3]]);
            if let Some(c) = color_of(i) {
                cols.extend(std::iter::repeat_n(Color32::from_rgb(srgb(c[0]), srgb(c[1]), srgb(c[2])), 4));
            }
        }
        sides = vec![7; tris.len()];
        pos = out;
        if cols.len() == tris.len() {
            colors = Some(cols);
        }
    } else {
        if cd.is_some() {
            colors = Some(
                tris.iter()
                    .map(|t| {
                        let mut c = [0.0f32; 3];
                        for &v in t {
                            let x = color_of(v as usize).unwrap_or([0.8; 3]);
                            for k in 0..3 {
                                c[k] += x[k] / 3.0;
                            }
                        }
                        Color32::from_rgb(srgb(c[0]), srgb(c[1]), srgb(c[2]))
                    })
                    .collect(),
            );
        }
        if let Some(c) = g.col("uv").filter(|c| c.width() >= 2) {
            uv = Some(Arc::new(tris.iter().map(|t| t.map(|v| [c.data()[0][v as usize], c.data()[1][v as usize]])).collect::<Vec<_>>()));
        }
    }
    let count = tris.len();
    let mut m = crate::model3d::Model::exact(pos, tris, colors, Some(sides))?;
    m.uv = uv.filter(|u: &Arc<Vec<[[f32; 2]; 3]>>| u.len() == count && m.tris.len() == count);
    Ok(m)
}

/// Écrit une géométrie en OBJ : ses points passés par `place` (le repère de la scène), ses UV, ses
/// faces telles quelles (les quads restent des quads).
pub fn obj_text(g: &Geo, place: &dyn Fn([f32; 3]) -> [f32; 3]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(g.n * 40 + g.faces.len() * 30);
    let _ = writeln!(s, "# Aryadne : géométrie de nodes, {} points, {} faces", g.n, g.faces.len());
    for i in 0..g.n {
        let q = place(g.pos(i));
        let _ = writeln!(s, "v {} {} {}", q[0], q[1], q[2]);
    }
    let uv = g.col("uv").filter(|c| c.width() >= 2);
    if let Some(c) = uv {
        for i in 0..g.n {
            let _ = writeln!(s, "vt {} {}", c.data()[0][i], c.data()[1][i]);
        }
    }
    let n = g.n as u32;
    for f in g.faces.iter() {
        let count = if f[3] == crate::geo::NO_VERT { 3 } else { 4 };
        if f[..count].iter().any(|i| *i >= n) {
            continue;
        }
        s.push('f');
        for i in &f[..count] {
            let _ = if uv.is_some() { write!(s, " {0}/{0}", i + 1) } else { write!(s, " {}", i + 1) };
        }
        s.push('\n');
    }
    // Sans face : les arêtes deviennent des lignes (fils), que la plupart des logiciels relisent
    if g.faces.is_empty() {
        for e in g.edges.iter().filter(|e| e[0] < n && e[1] < n) {
            let _ = writeln!(s, "l {} {}", e[0] + 1, e[1] + 1);
        }
    }
    s
}

/// Exporte l'objet `k` en une suite d'OBJ, un fichier par image de la scène (début → fin), dans
/// `dir` : `<nom>_0001.obj`… Les points sont dans le repère de la scène (Y en haut), transformation
/// et animation de l'objet comprises. Rend le nombre de fichiers écrits.
pub fn export_obj_sequence(scene: &Scene, scene_path: &Path, k: usize, dir: &Path, tick: &dyn Fn(usize, usize)) -> Result<usize, String> {
    let o = scene.objects.get(k).filter(|o| is_geo(&o.file)).ok_or("choisis une géométrie de nodes")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let stem: String = o.name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let (start, end) = (scene.start, scene.end.max(scene.start));
    let total = (end - start + 1) as usize;
    let mut sc = scene.clone();
    for (done, f) in (start..=end).enumerate() {
        sc.apply_frame(f as f32);
        let geo = geo_of(&sc, scene_path, k)?;
        let text = obj_text(&geo, &|p| sc.world(k, p));
        std::fs::write(dir.join(format!("{stem}_{:04}.obj", f.max(0))), text).map_err(|e| e.to_string())?;
        tick(done + 1, total);
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Object;

    fn flag_file(dir: &Path) -> PathBuf {
        let f = dir.join("drapeau.arygeo");
        crate::geo::example("drapeau dans le vent").unwrap().save(&f).unwrap();
        f
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ary-geoscene-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Écrit dans le dossier donné par ARY_DEMO de quoi essayer à l'écran : un drapeau simulé et un
    /// matériau à bandes qui suit ses UV.
    #[test]
    #[ignore]
    fn ecrire_la_demo() {
        let Some(dir) = std::env::var_os("ARY_DEMO") else { return };
        let dir = Path::new(&dir);
        crate::geo::example("drapeau dans le vent").unwrap().save(&dir.join("Drapeau.arygeo")).unwrap();
        let mut g = Graph::empty(crate::nodes::GraphKind::Material);
        let uv = g.add("texcoord", [0.0, 0.0]);
        let bands = g.add("stripes", [220.0, 0.0]);
        let mix = g.add("mix", [440.0, 0.0]);
        let out = g.add("mat_out", [660.0, 0.0]);
        for n in g.nodes.iter_mut() {
            if n.id == bands {
                n.params.insert("axe".into(), crate::nodes::Param::S("y".into()));
                n.values = vec![[3.0; 4], [0.5; 4], [0.0; 4]];
            }
            if n.id == mix {
                n.values = vec![[0.5; 4], [0.92, 0.9, 0.84, 1.0], [0.1, 0.25, 0.6, 1.0]];
            }
        }
        g.connect(uv, 1, bands, 2);
        g.connect(bands, 0, mix, 0);
        g.connect(mix, 0, out, 0);
        g.save(&dir.join("Drapeau rayé.arymat")).unwrap();
        // Une nappe qui voit l'objet « boule » de la scène (sans la boule en points de l'exemple)
        let mut g = crate::geo::example("tissu sur une boule").unwrap();
        let id = |g: &Graph, op: &str| g.nodes.iter().find(|n| n.op == op).map(|n| n.id).unwrap();
        let (solver, out, end) = (id(&g, "sim_solver"), id(&g, "geo_out"), id(&g, "sim_out"));
        let tint = g.links.iter().find(|l| l.from == end).map(|l| l.to).unwrap();
        g.links.retain(|l| l.to != out);
        g.connect(tint, 0, out, 0);
        let obj = g.add("geo_scene_obj", [250.0, 1010.0]);
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == obj) {
            n.params.insert("objet".into(), crate::nodes::Param::S("boule".into()));
        }
        g.connect(obj, 0, solver, 6);
        g.connect(obj, 1, solver, 7);
        g.save(&dir.join("Nappe.arygeo")).unwrap();
    }

    #[test]
    fn scene_frame_drives_the_simulation_frame() {
        let t = TimeRange { fps: 24.0, start: 1, end: 120 };
        // Même cadence : une image de scène, une image de simulation ; la scène part de 0, la simulation de 1
        assert_eq!(sim_frame(0.0, 0, 24.0, 0, t), 1);
        assert_eq!(sim_frame(10.0, 0, 24.0, 0, t), 11);
        // Décalage de départ : la simulation attend l'image 20 de la scène
        assert_eq!(sim_frame(10.0, 0, 24.0, 20, t), 1);
        assert_eq!(sim_frame(25.0, 0, 24.0, 20, t), 6);
        // Décalage négatif : la simulation a déjà tourné quand la scène commence
        assert_eq!(sim_frame(0.0, 0, 24.0, -30, t), 31);
        // Au-delà de la fin : la dernière image reste
        assert_eq!(sim_frame(500.0, 0, 24.0, 0, t), 120);
        // Cadences différentes : le même instant (une seconde de scène à 12 i/s = 24 images de simulation)
        assert_eq!(sim_frame(12.0, 0, 12.0, 0, t), 25);
        // Et l'inverse retombe sur l'image de scène
        assert_eq!(scene_frame_of(6, 0, 24.0, 20, t), 25.0);
        assert_eq!(scene_frame_of(25, 0, 12.0, 0, t), 12.0);
    }

    #[test]
    fn a_grid_becomes_a_shaded_mesh_with_uv() {
        let mut g = Graph::empty(crate::nodes::GraphKind::Geometry);
        let grid = g.add("geo_grid", [0.0, 0.0]);
        let out = g.add("geo_out", [200.0, 0.0]);
        g.connect(grid, 0, out, 0);
        let geo = Engine::default().eval(&g, 1, "").unwrap();
        let m = to_model(&geo).unwrap();
        // 20 × 20 points : 19 × 19 quads, deux triangles chacun
        assert_eq!((m.positions.len(), m.tris.len()), (400, 19 * 19 * 2));
        let uv = m.uv.as_ref().expect("la grille donne des UV");
        assert_eq!(uv.len(), m.tris.len());
        assert!(uv.iter().flatten().all(|c| (0.0..=1.0).contains(&c[0]) && (0.0..=1.0).contains(&c[1])));
        // La diagonale d'un quad n'est pas une arête
        assert_eq!((m.edges[0], m.edges[1]), (0b011, 0b110));
        // Le sol regarde vers le haut
        let t = m.tris[0];
        let (a, b, c) = (m.positions[t[0] as usize], m.positions[t[1] as usize], m.positions[t[2] as usize]);
        let ny = (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]);
        assert!(ny > 0.0, "normale vers +Y");
    }

    #[test]
    fn points_without_faces_still_show() {
        let mut g = Graph::empty(crate::nodes::GraphKind::Geometry);
        let pts = g.add("geo_scatter", [0.0, 0.0]);
        let out = g.add("geo_out", [200.0, 0.0]);
        g.connect(pts, 0, out, 0);
        let geo = Engine::default().eval(&g, 1, "").unwrap();
        let m = to_model(&geo).unwrap();
        assert_eq!(m.tris.len(), geo.n * 4);
        assert!(m.uv.is_none());
    }

    #[test]
    fn a_scene_shows_the_simulation_at_its_frame() {
        let dir = tmp("scene");
        flag_file(&dir);
        let path = dir.join("plan.aryscene");
        let mut sc = Scene::default();
        let k = sc.add_geo(&path, &dir.join("drapeau.arygeo"));
        assert!(is_geo(&sc.objects[k].file) && crate::scene::kind(&sc.objects[k]) == Kind::Model);
        // Image 0 : le drapeau au repos, plat
        sc.apply_frame(0.0);
        let (m0, errs, ranges) = crate::scene::build_ranges(&sc, &path);
        let m0 = m0.unwrap();
        assert!(errs[k].is_none() && ranges == vec![(k, 0, m0.tris.len())]);
        assert!(m0.uv.as_ref().is_some_and(|u| u.len() == m0.tris.len()));
        let depth = |m: &crate::model3d::Model| m.positions.iter().map(|p| p[2].abs()).fold(0.0f32, f32::max);
        assert!(depth(&m0) < 1e-4, "au repos le drapeau est dans son plan");
        // Changer d'image suffit à redemander la scène, et le tissu a bougé
        assert!(sc.apply_frame(20.0), "une géométrie de nodes suit la timeline");
        let m20 = crate::scene::build(&sc, &path).0.unwrap();
        assert_eq!(m20.tris.len(), m0.tris.len());
        assert!(depth(&m20) > 0.01, "le vent a poussé le tissu : {}", depth(&m20));
        // Décalage de départ : à l'image 20 de la scène, la simulation n'a pas commencé
        sc.objects[k].sim_offset = 20;
        let late = crate::scene::build(&sc, &path).0.unwrap();
        assert!(depth(&late) < 1e-4);
        // La transformation de l'objet s'applique comme aux autres
        sc.objects[k].sim_offset = 0;
        sc.objects[k].pos = [0.0, 5.0, 0.0];
        let moved = crate::scene::build(&sc, &path).0.unwrap();
        assert!((moved.positions[0][1] - m20.positions[0][1] - 5.0).abs() < 1e-4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_object_keeps_its_offset_in_the_file() {
        let mut o = Object::new("drapeau".into(), "sim/drapeau.arygeo".into());
        // Sans décalage, rien de plus dans le fichier : les anciennes scènes se relisent telles quelles
        assert!(!serde_json::to_string(&o).unwrap().contains("sim_offset"));
        o.sim_offset = -12;
        o.material = "nodes/Tissu.arymat".into();
        let text = serde_json::to_string(&o).unwrap();
        let back: Object = serde_json::from_str(&text).unwrap();
        assert_eq!(back, o);
        assert_eq!((back.sim_offset, back.material.as_str()), (-12, "nodes/Tissu.arymat"));
        // L'image courante n'est pas écrite dans la scène
        let mut sc = Scene::default();
        sc.objects.push(o);
        sc.apply_frame(33.0);
        let text = serde_json::to_string(&sc).unwrap();
        assert!(!text.contains("frame\""));
        let back: Scene = serde_json::from_str(&text).unwrap();
        assert_eq!(back, sc);
        assert_eq!(back.frame.0, 0.0);
    }

    #[test]
    fn a_scene_object_becomes_an_obstacle() {
        let dir = tmp("obstacle");
        // Une nappe lâchée au-dessus de l'objet « boule » de la scène
        let mut g = crate::geo::example("tissu sur une boule").unwrap();
        let solver = g.nodes.iter().find(|n| n.op == "sim_solver").map(|n| n.id).unwrap();
        let obj = g.add("geo_scene_obj", [0.0, 900.0]);
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == obj) {
            n.params.insert("objet".into(), crate::nodes::Param::S("boule".into()));
            // Hors scène : pas d'obstacle
            n.values = vec![[0.0; 4], [0.0; 4], [0.0; 4]];
        }
        g.connect(obj, 0, solver, 6);
        g.connect(obj, 1, solver, 7);
        let file = dir.join("nappe.arygeo");
        g.save(&file).unwrap();
        let path = dir.join("plan.aryscene");
        let mut sc = Scene::default();
        let ball = sc.add_primitive("@sphere");
        sc.objects[ball].name = "boule".into();
        sc.objects[ball].pos = [0.0, 0.7, 0.0];
        sc.objects[ball].scale = [0.55; 3];
        let k = sc.add_geo(&path, &file);
        // Ce que le graphe voit : la boîte de la boule, dans le repère de la nappe
        let inp = inputs(&sc, &path, k, &g).unwrap();
        let (c, size) = inp.at("boule", 1).unwrap();
        assert!((c[1] - 0.7).abs() < 1e-4 && (size[0] - 1.1).abs() < 0.02, "{c:?} {size:?}");
        // La nappe déplacée : la boule est vue de son repère
        sc.objects[k].pos = [1.0, 0.0, 0.0];
        let (c2, _) = inputs(&sc, &path, k, &g).unwrap().at("boule", 1).unwrap();
        assert!((c2[0] + 1.0).abs() < 1e-4);
        sc.objects[k].pos = [0.0; 3];
        // Simulée dans la scène : la nappe se pose sur la boule (hauteur moyenne de ses points)
        sc.apply_frame(40.0);
        let on = geo_of(&sc, &path, k).unwrap();
        // (les 28 × 28 premiers points sont le tissu ; la suite, la boule montrée en points par l'exemple)
        let center = |g: &Geo| (0..28 * 28).map(|i| g.pos(i)[1]).sum::<f32>() / 784.0;
        // Sans la boule dans la scène, le graphe retombe sur ses valeurs hors scène : rien ne retient la nappe
        sc.objects[ball].name = "autre".into();
        let off = geo_of(&sc, &path, k).unwrap();
        assert!(center(&off) < center(&on) - 0.1, "sans obstacle la nappe pend plus bas : {} puis {}", center(&on), center(&off));
        // Une boule qui bouge : la simulation voit sa position à chaque image
        sc.objects[ball].name = "boule".into();
        sc.objects[ball].keys = [(0, 0.7), (40, 1.2)].map(|(frame, y)| crate::scene::Key { frame, pos: [0.0, y, 0.0], rot: [0.0; 3], scale: [0.55; 3], ease: false, interp: "linear".into(), handle: 0, handles: vec![] }).to_vec();
        let inp = inputs(&sc, &path, k, &g).unwrap();
        assert!(inp.at("boule", 41).unwrap().0[1] > inp.at("boule", 1).unwrap().0[1] + 0.4);
        sc.apply_frame(40.0);
        let lifted = geo_of(&sc, &path, k).unwrap();
        assert!(center(&lifted) > center(&on) + 0.1, "la boule monte, la nappe aussi : {} puis {}", center(&on), center(&lifted));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_simulation_exports_as_an_obj_sequence() {
        let dir = tmp("obj");
        flag_file(&dir);
        let path = dir.join("plan.aryscene");
        let mut sc = Scene { end: 5, ..Default::default() };
        let k = sc.add_geo(&path, &dir.join("drapeau.arygeo"));
        sc.objects[k].pos = [0.0, 2.0, 0.0];
        let out = dir.join("suite");
        let n = export_obj_sequence(&sc, &path, k, &out, &|_, _| {}).unwrap();
        assert_eq!(n, 6);
        let first = std::fs::read_to_string(out.join("drapeau_0000.obj")).unwrap();
        let last = std::fs::read_to_string(out.join("drapeau_0005.obj")).unwrap();
        let count = |t: &str, tag: &str| t.lines().filter(|l| l.starts_with(tag)).count();
        // Mêmes points, mêmes UV, mêmes quads à chaque image : seule la position change
        assert!(count(&first, "v ") > 100 && count(&first, "v ") == count(&last, "v "));
        assert_eq!(count(&first, "vt "), count(&first, "v "));
        assert_eq!(count(&first, "f "), count(&last, "f "));
        assert!(first.lines().find(|l| l.starts_with("f ")).unwrap().split_whitespace().count() == 5, "les quads restent des quads");
        assert_ne!(first, last);
        // Le fichier se relit comme un modèle, à la hauteur de l'objet dans la scène
        let m = crate::model3d::load(&out.join("drapeau_0005.obj")).unwrap();
        assert!(m.min[1] > 0.5, "transformation de l'objet comprise : {:?}", m.min);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
