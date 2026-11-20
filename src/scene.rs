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
        self.objects.push(Object { name, file, pos: [0.0; 3], rot: [0.0; 3], scale: [1.0; 3], visible: true, color: None });
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
    let (mut pos, mut tris, mut colors, mut edges) = (vec![], vec![], vec![], vec![]);
    let mut errors = vec![None; scene.objects.len()];
    for (k, o) in scene.objects.iter().enumerate() {
        if !o.visible {
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
                pos.extend(m.positions.iter().map(|p| transform(o, if m.z_up { [p[0], p[2], -p[1]] } else { *p })));
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
        return (Err("Scène vide : glisse des modèles ici ou ajoute-en depuis le panneau.".into()), errors);
    }
    (crate::model3d::Model::with_edges(pos, tris, Some(colors), Some(edges), false), errors)
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
        // Le second triangle : (1,0,0) → échelle 2 → tourné de 90° autour de Z → (0,2,0) → déplacé
        let p = m.positions[4];
        assert!((p[0] - 10.0).abs() < 1e-4 && (p[1] - 2.0).abs() < 1e-4, "{p:?}");
        // Objet caché ou fichier manquant : la scène reste lisible
        s.objects[0].visible = false;
        s.objects[1].file = "absent.obj".into();
        let (m, errs) = build(&s, &file);
        assert!(m.is_err() && errs[1].is_some());
    }
}
