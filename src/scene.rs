//! Scène 3D (`.arya3`) : une liste d'objets, chacun pointant vers un fichier de modèle avec
//! sa position, sa rotation et son échelle. Le fichier est du JSON lisible ; les modèles
//! restent à leur place sur le disque, la scène ne fait que les réunir.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const EXT: &str = "arya3";

fn one3() -> [f32; 3] {
    [1.0; 3]
}
fn yes() -> bool {
    true
}
fn half() -> f32 {
    0.5
}
fn energy() -> f32 {
    1.2
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Object {
    pub name: String,
    /// Chemin du modèle, relatif au dossier de la scène quand c'est possible
    pub file: String,
    #[serde(default)]
    pub pos: [f32; 3],
    /// Rotation en degrés (X, puis Y, puis Z)
    #[serde(default)]
    pub rot: [f32; 3],
    #[serde(default = "one3")]
    pub scale: [f32; 3],
    #[serde(default = "yes")]
    pub visible: bool,
    /// Couleur d'affichage ; None = celles du fichier
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// Pile de modificateurs propre à cet objet (appliquée avant de le placer dans la scène)
    #[serde(default)]
    pub mods: Vec<crate::modifiers::Modifier>,
    /// Matériau en nodes de l'objet : chemin dans le coffre (vide = aucun)
    #[serde(default)]
    pub material: String,
    /// Nom de l'objet parent (vide = aucun) : l'enfant suit ses déplacements, rotations et changements de taille
    #[serde(default)]
    pub parent: String,
    /// Opacité d'une image de référence
    #[serde(default = "half")]
    pub opacity: f32,
    /// « light » ou « camera » pour un objet sans fichier qui n'est pas un simple groupe
    #[serde(default)]
    pub role: String,
    /// Force d'une lumière
    #[serde(default = "energy")]
    pub energy: f32,
}

impl Object {
    pub fn new(name: String, file: String) -> Object {
        Object { name, file, pos: [0.0; 3], rot: [0.0; 3], scale: [1.0; 3], visible: true, color: None, mods: vec![], material: String::new(), parent: String::new(), opacity: 0.5, role: String::new(), energy: 1.2 }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Model,
    /// Image de référence : un plan dans la scène, affiché par-dessus le rendu
    Image,
    /// Sans fichier : sert à regrouper d'autres objets
    Empty,
    /// Lumière directionnelle : elle éclaire dans le sens de son axe Y vers le bas
    Light,
    /// Point de vue enregistré
    Camera,
}

pub fn kind(o: &Object) -> Kind {
    let ext = o.file.rsplit('.').next().unwrap_or("").to_lowercase();
    if o.role == "light" {
        Kind::Light
    } else if o.role == "camera" {
        Kind::Camera
    } else if o.file.trim().is_empty() {
        Kind::Empty
    } else if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "tga" | "tif" | "tiff") {
        Kind::Image
    } else {
        Kind::Model
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Scene {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub objects: Vec<Object>,
}

pub fn is_scene(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(EXT))
}

impl Scene {
    pub fn read(path: &Path) -> Result<Scene, String> {
        let t = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        if t.trim().is_empty() {
            return Ok(Scene::default());
        }
        serde_json::from_str(&t).map_err(|e| format!("Scène illisible : {e}"))
    }

    pub fn write(&self, path: &Path) -> Result<(), String> {
        let s = Scene { version: 1, objects: self.objects.clone() };
        std::fs::write(path, serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }

    /// Ajoute un modèle à la scène, posé à l'origine.
    pub fn add(&mut self, scene_path: &Path, model: &Path) {
        let dir = scene_path.parent().unwrap_or(Path::new(""));
        let file = model.strip_prefix(dir).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| model.to_string_lossy().to_string());
        let stem = model.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "objet".into());
        let mut name = stem.clone();
        let mut n = 2;
        while self.objects.iter().any(|o| o.name == name) {
            name = format!("{stem}.{n:03}");
            n += 1;
        }
        self.objects.push(Object::new(name, file));
    }

    /// Ajoute un groupe vide et renvoie son indice.
    pub fn add_empty(&mut self) -> usize {
        self.add_role("groupe", "")
    }

    /// Ajoute un objet sans fichier : groupe (rôle vide), « light » ou « camera ».
    pub fn add_role(&mut self, base: &str, role: &str) -> usize {
        let mut name = base.to_string();
        let mut n = 2;
        while self.objects.iter().any(|o| o.name == name) {
            name = format!("{base}.{n:03}");
            n += 1;
        }
        let mut o = Object::new(name, String::new());
        o.role = role.to_string();
        self.objects.push(o);
        self.objects.len() - 1
    }

    /// La première lumière affichée : direction vers elle (pour l'éclairage), couleur et force.
    pub fn light(&self) -> Option<([f32; 3], [f32; 3], f32)> {
        let k = (0..self.objects.len()).find(|&k| kind(&self.objects[k]) == Kind::Light && self.shown(k))?;
        let (a, b) = (self.world(k, [0.0; 3]), self.world(k, [0.0, 1.0, 0.0]));
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if l < 1e-9 {
            return None;
        }
        let o = &self.objects[k];
        let c = o.color.map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]).unwrap_or([1.0, 0.97, 0.92]);
        Some(([d[0] / l, d[1] / l, d[2] / l], c, o.energy.max(0.0)))
    }

    pub fn parent_of(&self, k: usize) -> Option<usize> {
        let p = &self.objects.get(k)?.parent;
        if p.is_empty() {
            return None;
        }
        self.objects.iter().position(|o| &o.name == p).filter(|&i| i != k)
    }

    /// Fait passer un point de l'espace du parent de `k` à celui de la scène.
    pub fn from_parent(&self, k: usize, mut p: [f32; 3]) -> [f32; 3] {
        let mut cur = k;
        // La limite protège d'une boucle de parents écrite à la main dans le fichier
        for _ in 0..32 {
            let Some(up) = self.parent_of(cur) else { break };
            p = transform(&self.objects[up], p);
            cur = up;
        }
        p
    }

    /// Place un point de l'objet `k` dans la scène, parents compris.
    pub fn world(&self, k: usize, p: [f32; 3]) -> [f32; 3] {
        self.from_parent(k, transform(&self.objects[k], p))
    }

    /// Un objet est affiché s'il est visible et que ses parents le sont.
    pub fn shown(&self, k: usize) -> bool {
        let mut cur = k;
        for _ in 0..32 {
            if !self.objects[cur].visible {
                return false;
            }
            match self.parent_of(cur) {
                Some(up) => cur = up,
                None => break,
            }
        }
        true
    }

    /// `k` a-t-il `anc` parmi ses parents ? (pour refuser de créer une boucle)
    pub fn descends(&self, k: usize, anc: usize) -> bool {
        let mut cur = k;
        for _ in 0..32 {
            match self.parent_of(cur) {
                Some(up) if up == anc => return true,
                Some(up) => cur = up,
                None => break,
            }
        }
        false
    }

    /// Les objets dans l'ordre de l'arbre, avec leur profondeur.
    pub fn tree(&self) -> Vec<(usize, usize)> {
        fn walk(s: &Scene, parent: Option<usize>, depth: usize, out: &mut Vec<(usize, usize)>) {
            if depth > 32 {
                return;
            }
            for k in 0..s.objects.len() {
                if s.parent_of(k) == parent && !out.iter().any(|x| x.0 == k) {
                    out.push((k, depth));
                    walk(s, Some(k), depth + 1, out);
                }
            }
        }
        let mut out = vec![];
        walk(self, None, 0, &mut out);
        // Objets pris dans une boucle : affichés quand même, à plat
        for k in 0..self.objects.len() {
            if !out.iter().any(|x| x.0 == k) {
                out.push((k, 0));
            }
        }
        out
    }

    /// Renomme un objet sans perdre ses enfants.
    pub fn rename(&mut self, k: usize, name: &str) {
        let old = std::mem::replace(&mut self.objects[k].name, name.to_string());
        for o in &mut self.objects {
            if o.parent == old && !old.is_empty() {
                o.parent = name.to_string();
            }
        }
    }

    /// Retire un objet ; ses enfants remontent d'un cran et restent où ils sont dans la scène… à leur parent près.
    pub fn remove(&mut self, k: usize) {
        let gone = self.objects.remove(k);
        for o in &mut self.objects {
            if o.parent == gone.name {
                o.parent = gone.parent.clone();
            }
        }
    }
}

pub fn resolve(scene_path: &Path, file: &str) -> PathBuf {
    let p = Path::new(file);
    if p.is_absolute() { p.to_path_buf() } else { scene_path.parent().unwrap_or(Path::new("")).join(p) }
}

/// Place un point de l'objet dans la scène : échelle, rotation X Y Z, puis position.
pub fn transform(o: &Object, p: [f32; 3]) -> [f32; 3] {
    let p = [p[0] * o.scale[0], p[1] * o.scale[1], p[2] * o.scale[2]];
    let (sx, cx) = o.rot[0].to_radians().sin_cos();
    let p = [p[0], p[1] * cx - p[2] * sx, p[1] * sx + p[2] * cx];
    let (sy, cy) = o.rot[1].to_radians().sin_cos();
    let p = [p[0] * cy + p[2] * sy, p[1], -p[0] * sy + p[2] * cy];
    let (sz, cz) = o.rot[2].to_radians().sin_cos();
    let p = [p[0] * cz - p[1] * sz, p[0] * sz + p[1] * cz, p[2]];
    [p[0] + o.pos[0], p[1] + o.pos[1], p[2] + o.pos[2]]
}

type Cached = (Option<std::time::SystemTime>, Arc<crate::model3d::Model>);
static CACHE: Mutex<Option<HashMap<PathBuf, Cached>>> = Mutex::new(None);

/// Charge un modèle une seule fois tant que son fichier ne change pas :
/// déplacer un objet ne relit pas le disque.
fn cached(path: &Path) -> Result<Arc<crate::model3d::Model>, String> {
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    {
        let g = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((t, m)) = g.as_ref().and_then(|c| c.get(path)) {
            if *t == mtime {
                return Ok(m.clone());
            }
        }
    }
    let m = Arc::new(crate::model3d::load(path)?);
    let mut g = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let c = g.get_or_insert_with(HashMap::new);
    if c.len() > 64 {
        c.clear();
    }
    c.insert(path.to_path_buf(), (mtime, m.clone()));
    Ok(m)
}

/// Réunit les objets visibles en un seul modèle, Y en haut.
/// Renvoie aussi, pour chaque objet, l'erreur de chargement éventuelle.
pub fn build(scene: &Scene, scene_path: &Path) -> (Result<crate::model3d::Model, String>, Vec<Option<String>>) {
    let (m, e, _) = build_ranges(scene, scene_path);
    (m, e)
}

/// Comme `build`, avec en plus la plage de triangles de chaque objet (objet, début, fin) :
/// c'est ce qui permet de retrouver l'objet sous le curseur.
pub fn build_ranges(scene: &Scene, scene_path: &Path) -> (Result<crate::model3d::Model, String>, Vec<Option<String>>, Vec<(usize, usize, usize)>) {
    let mut ranges = vec![];
    let (mut pos, mut tris, mut colors, mut edges) = (vec![], vec![], vec![], vec![]);
    let mut errors = vec![None; scene.objects.len()];
    for (k, o) in scene.objects.iter().enumerate() {
        if !scene.shown(k) || kind(o) != Kind::Model {
            continue;
        }
        let path = resolve(scene_path, &o.file);
        // Une scène dans une scène tournerait en rond
        if is_scene(&path) {
            errors[k] = Some("une scène ne peut pas en contenir une autre".into());
            continue;
        }
        match cached(&path) {
            Ok(m) => {
                let base = pos.len() as u32;
                if o.mods.iter().any(|md| md.on) {
                    // Modificateurs de l'objet : calculés dans son espace ramené à 1, puis remis à sa taille
                    let sz = m.size();
                    let longest = sz[0].max(sz[1]).max(sz[2]).max(1e-9);
                    let c = [(m.min[0] + m.max[0]) / 2.0, (m.min[1] + m.max[1]) / 2.0, (m.min[2] + m.max[2]) / 2.0];
                    let up = |v: [f32; 3]| if m.z_up { [v[0], v[2], -v[1]] } else { v };
                    let (c, origin) = (up(c), up([-c[0] / longest, -c[1] / longest, -c[2] / longest]));
                    let mesh = crate::modifiers::apply(&o.mods, crate::modifiers::Mesh::from_model(&m, m.z_up), origin);
                    ranges.push((k, tris.len(), tris.len() + mesh.tris.len()));
                    pos.extend(mesh.pos.iter().map(|p| scene.world(k, [p[0] * longest + c[0], p[1] * longest + c[1], p[2] * longest + c[2]])));
                    tris.extend(mesh.tris.iter().map(|t| [t[0] + base, t[1] + base, t[2] + base]));
                    match o.color {
                        Some(c) => colors.extend(std::iter::repeat_n(eframe::egui::Color32::from_rgb(c[0], c[1], c[2]), mesh.tris.len())),
                        None => colors.extend(mesh.colors.iter().copied()),
                    }
                    edges.extend(mesh.edges.iter().copied());
                    continue;
                }
                ranges.push((k, tris.len(), tris.len() + m.tris.len()));
                pos.extend(m.positions.iter().map(|p| scene.world(k, if m.z_up { [p[0], p[2], -p[1]] } else { *p })));
                tris.extend(m.tris.iter().map(|t| [t[0] + base, t[1] + base, t[2] + base]));
                match o.color {
                    Some(c) => colors.extend(std::iter::repeat_n(eframe::egui::Color32::from_rgb(c[0], c[1], c[2]), m.tris.len())),
                    None => colors.extend(m.colors.iter().copied()),
                }
                edges.extend(m.edges.iter().copied());
            }
            Err(e) => errors[k] = Some(e),
        }
    }
    if tris.is_empty() {
        // Rien que des images, des lumières, des caméras ou des groupes : la scène s'ouvre quand même,
        // avec un cadre autour d'eux (et un triangle sans surface, que le rendu ignore)
        let others: Vec<[f32; 3]> = (0..scene.objects.len()).filter(|&k| scene.shown(k) && kind(&scene.objects[k]) != Kind::Model && errors[k].is_none()).map(|k| scene.world(k, [0.0; 3])).collect();
        if others.is_empty() {
            return (Err("Scène vide : glisse des modèles ici ou ajoute-en depuis le panneau.".into()), errors, ranges);
        }
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &others {
            for a in 0..3 {
                lo[a] = lo[a].min(p[a] - 2.0);
                hi[a] = hi[a].max(p[a] + 2.0);
            }
        }
        return (crate::model3d::Model::exact(vec![lo, hi], vec![[0, 0, 0]], None, None), errors, vec![]);
    }
    (crate::model3d::Model::exact(pos, tris, Some(colors), Some(edges)), errors, ranges)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_aller_retour_et_fusion() {
        let dir = std::env::temp_dir().join("aryadne-scene-test");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("tri.obj"), "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let file = dir.join("essai.arya3");
        let mut s = Scene::default();
        s.add(&file, &dir.join("tri.obj"));
        s.add(&file, &dir.join("tri.obj"));
        assert_eq!(s.objects[0].file, "tri.obj");
        assert_eq!(s.objects[1].name, "tri.002");
        s.objects[1].pos = [10.0, 0.0, 0.0];
        s.objects[1].scale = [2.0; 3];
        s.objects[1].rot = [0.0, 0.0, 90.0];
        s.write(&file).unwrap();
        assert_eq!(Scene::read(&file).unwrap().objects, s.objects);
        let (m, errs) = build(&s, &file);
        let m = m.unwrap();
        assert!(errs.iter().all(|e| e.is_none()));
        assert_eq!(m.tris.len(), 2);
        assert_eq!(build_ranges(&s, &file).2, vec![(0, 0, 1), (1, 1, 2)]);
        // Le second triangle : (1,0,0) → échelle 2 → tourné de 90° autour de Z → (0,2,0) → déplacé
        let p = m.positions[4];
        assert!((p[0] - 10.0).abs() < 1e-4 && (p[1] - 2.0).abs() < 1e-4, "{p:?}");
        // Un modificateur sur un seul objet : lui change, l'autre non
        let mut s2 = s.clone();
        s2.objects[0].mods = vec![crate::modifiers::Modifier::new(crate::modifiers::Kind::Subdivision { levels: 1, optimal: false, simple: true })];
        let (m2, _, r2) = build_ranges(&s2, &file);
        let m2 = m2.unwrap();
        assert_eq!(m2.tris.len(), 7);
        assert_eq!(r2, vec![(0, 0, 6), (1, 6, 7)]);
        // Le modificateur ne déplace pas l'objet : mêmes limites qu'avant
        for k in 0..3 {
            assert!((m2.min[k] - m.min[k]).abs() < 1e-4 && (m2.max[k] - m.max[k]).abs() < 1e-4);
        }
        // Hiérarchie : l'enfant suit son parent, et un groupe vide ou une image n'ajoute aucun triangle
        let mut h = s.clone();
        let g = h.add_empty();
        h.objects[g].pos = [0.0, 5.0, 0.0];
        h.objects[0].parent = h.objects[g].name.clone();
        h.add(&file, &dir.join("croquis.png"));
        assert_eq!(kind(&h.objects[3]), Kind::Image);
        let (mh, eh, rh) = build_ranges(&h, &file);
        assert!(eh.iter().all(|e| e.is_none()) && rh.len() == 2);
        assert!((mh.unwrap().positions[1][1] - 5.0).abs() < 1e-4);
        assert_eq!(h.tree(), vec![(1, 0), (2, 0), (0, 1), (3, 0)]);
        h.rename(g, "décor");
        assert_eq!(h.objects[0].parent, "décor");
        // Une boucle de parents ne bloque rien
        h.objects[g].parent = h.objects[0].name.clone();
        assert!(h.world(0, [0.0; 3]).iter().all(|x| x.is_finite()));
        assert_eq!(h.tree().len(), 4);
        h.remove(g);
        assert!(h.objects.iter().all(|o| o.parent != "décor"));
        // Lumière : au repos elle vient d'en haut ; tournée de 90° autour de Z, elle vient de -X
        let mut l = Scene::default();
        let k = l.add_role("soleil", "light");
        assert!((l.light().unwrap().0[1] - 1.0).abs() < 1e-5);
        l.objects[k].rot = [0.0, 0.0, 90.0];
        assert!((l.light().unwrap().0[0] + 1.0).abs() < 1e-5);
        // Sans modèle, la scène s'ouvre quand même
        assert!(build_ranges(&l, &file).0.is_ok());
        // Objet caché ou fichier manquant : la scène reste lisible
        s.objects[0].visible = false;
        s.objects[1].file = "absent.obj".into();
        let (m, errs) = build(&s, &file);
        assert!(m.is_err() && errs[1].is_some());
    }
}
