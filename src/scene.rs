//! Scène 3D (`.aryscene`) : une liste d'objets, chacun pointant vers un fichier de modèle avec
//! sa position, sa rotation et son échelle. Le fichier est du JSON lisible ; les modèles
//! restent à leur place sur le disque, la scène ne fait que les réunir.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};


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
fn spot_deg() -> f32 {
    45.0
}
fn spot_blend() -> f32 {
    0.15
}
fn area_size() -> [f32; 2] {
    [1.0, 1.0]
}
fn range() -> f32 {
    8.0
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
    /// Peinture sur l'objet (texture PNG sur ses UV, chemin dans le coffre ; vide = aucune)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub paint: String,
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
    /// Lumière ponctuelle (elle éclaire autour d'elle) plutôt que soleil (une direction)
    #[serde(default)]
    pub point: bool,
    /// Portée d'une lumière ponctuelle, dans les unités de la scène
    #[serde(default = "range")]
    pub range: f32,
    /// Forme d'une lumière : 0 selon `point` (soleil ou ponctuelle), 1 ponctuelle, 2 spot, 3 rectangle
    #[serde(default)]
    pub shape: u8,
    /// Spot : ouverture du cône (degrés) et part du bord en fondu
    #[serde(default = "spot_deg")]
    pub spot: f32,
    #[serde(default = "spot_blend")]
    pub blend: f32,
    /// Rectangle : largeur et hauteur
    #[serde(default = "area_size")]
    pub size: [f32; 2],
    /// Clés d'animation, triées par image
    #[serde(default)]
    pub keys: Vec<Key>,
    /// Avant la première clé et après la dernière : constante, en boucle, en va-et-vient
    #[serde(default, skip_serializing_if = "crate::curves::Loop::is_none")]
    pub cycle: crate::curves::Loop,
    /// Contrainte « viser » : le nom de l'objet vers lequel celui-ci se tourne toujours (son avant : -Z)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub aim: String,
    /// Os : longueur, le long de son axe Y
    #[serde(default = "bone_len", skip_serializing_if = "is_bone_len")]
    pub length: f32,
    /// Os : pose de repos (position, rotation, taille) enregistrée à la liaison du squelette
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest: Option<[[f32; 3]; 3]>,
    /// Modèle : nom de l'os racine du squelette qui le déforme (vide = aucun)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub skin: String,
    /// Os : cinématique inverse, le bout de la chaîne cherche à atteindre cet objet
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ik: String,
    /// Os : nombre d'os de la chaîne de cinématique inverse (cet os compris)
    #[serde(default = "ik_chain")]
    pub ik_chain: u8,
    /// Modèle lié : portée de l'influence des os (1 = normale)
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub reach: f32,
    /// Formes (blendshapes) : d'autres versions du même maillage, mélangées selon leur valeur
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub morphs: Vec<Morph>,
    /// Contraintes : copier la position, la rotation d'un autre objet (avec un décalage), limiter la position
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub copy_loc: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub copy_rot: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<[[f32; 3]; 2]>,
}

/// Une forme : un fichier au même nombre de sommets que le modèle, et sa part dans le mélange.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Morph {
    pub name: String,
    pub file: String,
    #[serde(default)]
    pub value: f32,
    /// Clés (image, valeur), triées ; entre deux clés la valeur passe en ligne droite
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<(i32, f32)>,
}

impl Morph {
    pub fn value_at(&self, f: f32) -> Option<f32> {
        let (first, last) = (self.keys.first()?, self.keys.last()?);
        if f <= first.0 as f32 {
            return Some(first.1);
        }
        if f >= last.0 as f32 {
            return Some(last.1);
        }
        let w = self.keys.windows(2).find(|w| f >= w[0].0 as f32 && f <= w[1].0 as f32)?;
        let t = (f - w[0].0 as f32) / (w[1].0 - w[0].0).max(1) as f32;
        Some(w[0].1 + (w[1].1 - w[0].1) * t)
    }
    /// Pose (ou remplace) une clé à l'image `f`.
    pub fn set_key(&mut self, f: i32) {
        self.keys.retain(|k| k.0 != f);
        self.keys.push((f, self.value));
        self.keys.sort_by_key(|k| k.0);
    }
}

fn bone_len() -> f32 {
    1.0
}
fn is_bone_len(v: &f32) -> bool {
    (*v - 1.0).abs() < 1e-6
}
fn ik_chain() -> u8 {
    2
}
fn one() -> f32 {
    1.0
}
fn is_one(v: &f32) -> bool {
    (*v - 1.0).abs() < 1e-6
}

/// Une clé : la pose complète de l'objet à une image donnée.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Key {
    pub frame: i32,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub scale: [f32; 3],
    /// Ancien réglage : départ et arrivée en douceur (sert quand `interp` est vide)
    #[serde(default = "yes")]
    pub ease: bool,
    /// Passage vers la clé suivante : « bezier », « linear » ou « constant » (vide : d'après `ease`)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub interp: String,
    /// Type des poignées : 0 automatique, 1 alignées, 2 libres, 3 vecteur (comme dans Blender)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub handle: u8,
    /// Poignées de Bézier par canal (9 : position, rotation, taille ; X Y Z) :
    /// [gauche Δimage, gauche Δvaleur, droite Δimage, droite Δvaleur]. Vide : automatiques.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handles: Vec<[f32; 4]>,
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

/// Les neuf canaux animables : (groupe, axe).
pub const CHANNELS: [(&str, &str); 9] = [("position", "X"), ("position", "Y"), ("position", "Z"), ("rotation", "X"), ("rotation", "Y"), ("rotation", "Z"), ("taille", "X"), ("taille", "Y"), ("taille", "Z")];

impl Key {
    pub fn get(&self, c: usize) -> f32 {
        match c {
            0..=2 => self.pos[c],
            3..=5 => self.rot[c - 3],
            _ => self.scale[(c - 6).min(2)],
        }
    }

    pub fn set(&mut self, c: usize, v: f32) {
        match c {
            0..=2 => self.pos[c] = v,
            3..=5 => self.rot[c - 3] = v,
            _ => self.scale[(c - 6).min(2)] = v,
        }
    }

    /// Le lissage commun (None : Bézier, avec ses poignées)
    pub fn ease(&self) -> Option<crate::keys::Ease> {
        crate::keys::Ease::from_word(self.mode())
    }

    /// Mode de passage vers la clé suivante
    pub fn mode(&self) -> &str {
        match self.interp.as_str() {
            "" => if self.ease { "bezier" } else { "linear" },
            m => m,
        }
    }
}

/// Poignées d'une clé sur un canal : (gauche, droite), en (Δimage, Δvaleur).
/// Automatiques : pente des voisines, à plat sur un sommet ou un creux (comme « auto clamped » de Blender) ;
/// vecteur : pointées vers les clés voisines.
pub fn handles_of(keys: &[Key], i: usize, c: usize) -> ([f32; 2], [f32; 2]) {
    let k = &keys[i];
    let prev = (i > 0).then(|| &keys[i - 1]);
    let next = keys.get(i + 1);
    let (f, v) = (k.frame as f32, k.get(c));
    let left_len = prev.map(|p| (f - p.frame as f32) / 3.0).unwrap_or(4.0);
    let right_len = next.map(|n| (n.frame as f32 - f) / 3.0).unwrap_or(4.0);
    if k.handle != 0 {
        if let Some(h) = k.handles.get(c) {
            return ([h[0], h[1]], [h[2], h[3]]);
        }
    }
    if k.handle == 3 {
        let l = prev.map(|p| [(p.frame as f32 - f) / 3.0, (p.get(c) - v) / 3.0]).unwrap_or([-left_len, 0.0]);
        let r = next.map(|n| [(n.frame as f32 - f) / 3.0, (n.get(c) - v) / 3.0]).unwrap_or([right_len, 0.0]);
        return (l, r);
    }
    let slope = match (prev, next) {
        (Some(p), Some(n)) => {
            let (a, b) = (p.get(c), n.get(c));
            // Sommet ou creux : on garde la courbe à plat pour ne pas dépasser
            if (v >= a && v >= b) || (v <= a && v <= b) {
                0.0
            } else {
                (b - a) / ((n.frame - p.frame).max(1) as f32)
            }
        }
        _ => 0.0,
    };
    ([-left_len, -slope * left_len], [right_len, slope * right_len])
}

/// Valeur du canal `c` à l'image `f`, d'après les clés (Bézier, linéaire ou constant).
pub fn channel_at(keys: &[Key], c: usize, f: f32) -> f32 {
    let (first, last) = (&keys[0], &keys[keys.len() - 1]);
    if f <= first.frame as f32 {
        return first.get(c);
    }
    if f >= last.frame as f32 {
        return last.get(c);
    }
    let i = (0..keys.len() - 1).find(|&i| f < keys[i + 1].frame as f32).unwrap_or(0);
    let (a, b) = (&keys[i], &keys[i + 1]);
    let (fa, fb) = (a.frame as f32, b.frame as f32);
    let (va, vb) = (a.get(c), b.get(c));
    match a.mode() {
        "constant" => va,
        "linear" => va + (vb - va) * (f - fa) / (fb - fa).max(1e-6),
        // Les lissages communs à toute l'app (départ doux, rebond…)
        m if crate::keys::Ease::from_word(m).is_some() => va + (vb - va) * crate::keys::Ease::from_word(m).unwrap().apply((f - fa) / (fb - fa).max(1e-6)),
        _ => {
            let (_, r) = handles_of(keys, i, c);
            let (l, _) = handles_of(keys, i + 1, c);
            // Les poignées ne dépassent pas le segment : la courbe reste une fonction du temps
            let span = (fb - fa).max(1e-6);
            let x1 = fa + r[0].clamp(0.0, span);
            let x2 = fb + l[0].clamp(-span, 0.0);
            let (y1, y2) = (va + r[1] * if r[0] > span { span / r[0] } else { 1.0 }, vb + l[1] * if -l[0] > span { span / -l[0] } else { 1.0 });
            let bez = |p0: f32, p1: f32, p2: f32, p3: f32, t: f32| {
                let u = 1.0 - t;
                u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3
            };
            // x(t) = f par dichotomie (x croît avec t)
            let (mut lo, mut hi) = (0.0f32, 1.0f32);
            for _ in 0..30 {
                let m = (lo + hi) / 2.0;
                if bez(fa, x1, x2, fb, m) < f {
                    lo = m;
                } else {
                    hi = m;
                }
            }
            bez(va, y1, y2, vb, (lo + hi) / 2.0)
        }
    }
}

impl Object {
    /// Pose de l'objet à l'image `f` d'après ses clés (None s'il n'en a pas).
    pub fn pose_at(&self, f: f32) -> Option<([f32; 3], [f32; 3], [f32; 3])> {
        let k = &self.keys;
        // Bouclée : on lit dans l'animation, et le cycle avec décalage ajoute le chemin des boucles d'avant
        if k.len() >= 2 && !self.cycle.is_none() {
            let (a, b) = (&k[0], &k[k.len() - 1]);
            let (f2, n) = self.cycle.remap(f, a.frame as f32, b.frame as f32);
            if f2 != f || n != 0.0 {
                let raw = Object { cycle: Default::default(), keys: k.clone(), ..Object::new(String::new(), String::new()) };
                let (mut p, mut r, mut s) = raw.pose_at(f2)?;
                for i in 0..3 {
                    p[i] += n * (b.pos[i] - a.pos[i]);
                    r[i] += n * (b.rot[i] - a.rot[i]);
                    s[i] += n * (b.scale[i] - a.scale[i]);
                }
                return Some((p, r, s));
            }
        }
        let (first, last) = (k.first()?, k.last()?);
        if f <= first.frame as f32 {
            return Some((first.pos, first.rot, first.scale));
        }
        if f >= last.frame as f32 {
            return Some((last.pos, last.rot, last.scale));
        }
        let ch = |c: usize| channel_at(k, c, f);
        Some(([ch(0), ch(1), ch(2)], [ch(3), ch(4), ch(5)], [ch(6), ch(7), ch(8)]))
    }

    /// Pose une clé à l'image `frame` avec la pose actuelle (ou remplace celle qui s'y trouve).
    pub fn set_key(&mut self, frame: i32) {
        let key = Key { frame, pos: self.pos, rot: self.rot, scale: self.scale, ease: true, interp: String::new(), handle: 0, handles: vec![] };
        match self.keys.iter().position(|k| k.frame == frame) {
            // Une clé qui existe garde son passage et ses poignées : seule la pose change
            Some(i) => {
                let old = self.keys[i].clone();
                self.keys[i] = Key { ease: old.ease, interp: old.interp, handle: old.handle, handles: old.handles, ..key };
            }
            None => {
                self.keys.push(key);
                self.keys.sort_by_key(|k| k.frame);
            }
        }
    }

    pub fn new(name: String, file: String) -> Object {
        Object { name, file, pos: [0.0; 3], rot: [0.0; 3], scale: [1.0; 3], visible: true, color: None, mods: vec![], material: String::new(), paint: String::new(), parent: String::new(), opacity: 0.5, role: String::new(), energy: 1.2, point: false, range: 8.0, shape: 0, spot: spot_deg(), blend: spot_blend(), size: area_size(), keys: vec![], cycle: Default::default(), aim: String::new(), length: 1.0, rest: None, skin: String::new(), ik: String::new(), ik_chain: 2, reach: 1.0, morphs: vec![], copy_loc: String::new(), copy_rot: String::new(), limit: None }
    }
}

/// Une lumière de la scène, prête pour le rendu.
#[derive(Clone, Copy, Debug)]
pub struct SceneLight {
    /// 0 soleil, 1 ponctuelle, 2 spot, 3 rectangle
    pub shape: u8,
    /// Spot et rectangle : direction éclairée ; cône du spot (cosinus) ; demi-côtés du rectangle
    pub axis: [f32; 3],
    pub cone: [f32; 2],
    pub half: [[f32; 3]; 2],
    pub point: bool,
    /// Soleil : direction vers la lumière. Ponctuelle : sa position dans la scène.
    pub at: [f32; 3],
    pub color: [f32; 3],
    pub energy: f32,
    pub range: f32,
}

/// Forme effective d'une lumière (les anciennes scènes n'ont que `point`).
pub fn light_shape(o: &Object) -> u8 {
    if o.shape > 0 { o.shape.min(3) } else if o.point { 1 } else { 0 }
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
    /// Os d'un squelette
    Bone,
}

pub fn kind(o: &Object) -> Kind {
    let ext = o.file.rsplit('.').next().unwrap_or("").to_lowercase();
    if o.role == "light" {
        Kind::Light
    } else if o.role == "camera" {
        Kind::Camera
    } else if o.role == "bone" {
        Kind::Bone
    } else if o.file.trim().is_empty() {
        Kind::Empty
    } else if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "tga" | "tif" | "tiff") {
        Kind::Image
    } else {
        Kind::Model
    }
}

impl Default for Scene {
    fn default() -> Scene {
        Scene { version: 1, objects: vec![], fps: 24.0, start: 0, end: 120 }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Scene {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub objects: Vec<Object>,
    /// Animation : images par seconde, première et dernière image
    #[serde(default = "fps")]
    pub fps: f32,
    #[serde(default)]
    pub start: i32,
    #[serde(default = "end")]
    pub end: i32,
}

fn fps() -> f32 {
    24.0
}
fn end() -> i32 {
    120
}

pub fn is_scene(path: &Path) -> bool {
    crate::formats::has(&path.to_string_lossy(), "scene")
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
        let s = Scene { version: 1, ..self.clone() };
        crate::vault::atomic_write(path, serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }

    /// Met chaque objet animé dans sa pose de l'image `f`. Renvoie vrai si quelque chose a bougé.
    pub fn apply_frame(&mut self, f: f32) -> bool {
        let mut moved = false;
        for o in &mut self.objects {
            for m in &mut o.morphs {
                if let Some(v) = m.value_at(f) {
                    if (v - m.value).abs() > 1e-6 {
                        m.value = v;
                        moved = true;
                    }
                }
            }
            if let Some((p, r, s)) = o.pose_at(f) {
                if (o.pos, o.rot, o.scale) != (p, r, s) {
                    (o.pos, o.rot, o.scale) = (p, r, s);
                    moved = true;
                }
            }
        }
        let copied = self.apply_copies();
        let aimed = self.apply_aims();
        moved | copied | aimed | crate::rig::solve_ik(self)
    }

    /// Les contraintes « viser » : chaque objet qui en a une se tourne vers sa cible (le roulis reste le sien).
    /// Les rotations de ses parents ne sont pas compensées.
    pub fn apply_aims(&mut self) -> bool {
        let mut moved = false;
        for k in 0..self.objects.len() {
            if self.objects[k].aim.is_empty() {
                continue;
            }
            let Some(t) = self.objects.iter().position(|o| o.name == self.objects[k].aim).filter(|&t| t != k) else { continue };
            let (p, q) = (self.world(k, [0.0; 3]), self.world(t, [0.0; 3]));
            let mut d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
            // Sous un parent, la direction se ramène dans son repère (sa rotation et sa taille sont compensées)
            if let Some(up) = self.parent_of(k) {
                let o = self.world(up, [0.0; 3]);
                let col = |e: [f32; 3]| {
                    let w = self.world(up, e);
                    [w[0] - o[0], w[1] - o[1], w[2] - o[2]]
                };
                let (a, b, c) = (col([1.0, 0.0, 0.0]), col([0.0, 1.0, 0.0]), col([0.0, 0.0, 1.0]));
                let m = [[a[0], b[0], c[0]], [a[1], b[1], c[1]], [a[2], b[2], c[2]]];
                if let Some(inv) = inverse3(m) {
                    d = [0, 1, 2].map(|i| inv[i][0] * d[0] + inv[i][1] * d[1] + inv[i][2] * d[2]);
                }
            }
            let rot = aim_rotation(d, self.objects[k].rot[2]);
            if let Some(r) = rot {
                if (0..3).any(|i| (r[i] - self.objects[k].rot[i]).abs() > 1e-4) {
                    self.objects[k].rot = r;
                    moved = true;
                }
            }
        }
        moved
    }

    /// Contraintes « copier la position », « copier la rotation » (dans la scène) et « limiter la position ».
    pub fn apply_copies(&mut self) -> bool {
        let mut moved = false;
        for k in 0..self.objects.len() {
            let (cl, cr) = (self.objects[k].copy_loc.clone(), self.objects[k].copy_rot.clone());
            if !cl.is_empty() {
                if let Some(t) = self.objects.iter().position(|o| o.name == cl).filter(|&t| t != k && !self.descends(t, k)) {
                    let w = self.world(t, [0.0; 3]);
                    // La position voulue dans la scène, ramenée dans l'espace du parent
                    let mut p = w;
                    if let Some(up) = self.parent_of(k) {
                        let (m, o) = crate::rig::affine(self, up);
                        if let Some(inv) = inverse3(m) {
                            let d = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
                            p = [0, 1, 2].map(|i| inv[i][0] * d[0] + inv[i][1] * d[1] + inv[i][2] * d[2]);
                        }
                    }
                    if (0..3).any(|i| (p[i] - self.objects[k].pos[i]).abs() > 1e-5) {
                        self.objects[k].pos = p;
                        moved = true;
                    }
                }
            }
            if !cr.is_empty() && self.parent_of(k).is_none() {
                if let Some(t) = self.objects.iter().position(|o| o.name == cr).filter(|&t| t != k) {
                    let (m, _) = crate::rig::affine(self, t);
                    // La rotation pure de la cible (sa taille retirée)
                    let cols = [0, 1, 2].map(|c| {
                        let v = [m[0][c], m[1][c], m[2][c]];
                        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
                        [v[0] / l, v[1] / l, v[2] / l]
                    });
                    let r = rot_euler([[cols[0][0], cols[1][0], cols[2][0]], [cols[0][1], cols[1][1], cols[2][1]], [cols[0][2], cols[1][2], cols[2][2]]]);
                    if (0..3).any(|i| (r[i] - self.objects[k].rot[i]).abs() > 1e-4) {
                        self.objects[k].rot = r;
                        moved = true;
                    }
                }
            }
            if let Some([lo, hi]) = self.objects[k].limit {
                let o = &mut self.objects[k];
                for i in 0..3 {
                    let v = o.pos[i].clamp(lo[i].min(hi[i]), hi[i].max(lo[i]));
                    if v != o.pos[i] {
                        o.pos[i] = v;
                        moved = true;
                    }
                }
            }
        }
        moved
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

    /// Ajoute une forme de base (« @cube », « @sphere »…) et renvoie son indice.
    pub fn add_primitive(&mut self, id: &str) -> usize {
        let base = PRIMITIVES.iter().find(|p| p.0 == id).map(|p| p.1).unwrap_or("forme");
        let mut name = base.to_string();
        let mut n = 2;
        while self.objects.iter().any(|o| o.name == name) {
            name = format!("{base}.{n:03}");
            n += 1;
        }
        self.objects.push(Object::new(name, id.to_string()));
        self.objects.len() - 1
    }

    /// La scène de départ : un cube, une lumière, une caméra.
    pub fn starter() -> Scene {
        let mut s = Scene::default();
        s.add_primitive("@cube");
        let l = s.add_role("soleil", "light");
        (s.objects[l].pos, s.objects[l].rot) = ([4.0, 6.0, 3.0], [35.0, 0.0, -35.0]);
        let c = s.add_role("caméra", "camera");
        (s.objects[c].pos, s.objects[c].rot) = ([7.0, 5.0, 7.0], [-28.0, 45.0, 0.0]);
        s
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
    #[cfg(test)]
    pub fn light(&self) -> Option<([f32; 3], [f32; 3], f32)> {
        self.lights().into_iter().find(|l| !l.point).map(|l| (l.at, l.color, l.energy))
    }

    /// Toutes les lumières affichées, dans l'ordre de la scène.
    pub fn lights(&self) -> Vec<SceneLight> {
        let mut out = vec![];
        for k in 0..self.objects.len() {
            let o = &self.objects[k];
            if kind(o) != Kind::Light || !self.shown(k) {
                continue;
            }
            let color = o.color.map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]).unwrap_or([1.0, 0.97, 0.92]);
            let a = self.world(k, [0.0; 3]);
            let rel = |v: [f32; 3]| {
                let b = self.world(k, v);
                [b[0] - a[0], b[1] - a[1], b[2] - a[2]]
            };
            let unit = |d: [f32; 3]| {
                let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-9);
                [d[0] / l, d[1] / l, d[2] / l]
            };
            let shape = light_shape(o);
            // Spot et rectangle éclairent vers le bas de leur axe Y, comme le soleil
            let down = unit(rel([0.0, -1.0, 0.0]));
            let half_a = (o.spot.clamp(1.0, 179.0) * 0.5).to_radians();
            let inner = half_a * (1.0 - o.blend.clamp(0.0, 1.0));
            let base = SceneLight { shape, axis: down, cone: [half_a.cos(), inner.cos()], half: [rel([o.size[0] * 0.5, 0.0, 0.0]), rel([0.0, 0.0, o.size[1] * 0.5])], point: true, at: a, color, energy: o.energy.max(0.0), range: o.range.max(1e-3) };
            if shape != 0 {
                out.push(base);
                continue;
            }
            let d = rel([0.0, 1.0, 0.0]);
            let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if l > 1e-9 {
                out.push(SceneLight { point: false, at: [d[0] / l, d[1] / l, d[2] / l], range: 0.0, ..base });
            }
        }
        out
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

/// Formes de base : (identifiant rangé dans `file`, nom).
pub const PRIMITIVES: [(&str, &str); 6] = [("@cube", "cube"), ("@sphere", "sphère"), ("@plane", "plan"), ("@cylinder", "cylindre"), ("@cone", "cône"), ("@torus", "tore")];

/// Construit une forme de base. Les faces sont des quads ou des polygones, comme dans un fichier de modèle.
pub fn primitive(id: &str) -> Option<crate::model3d::Model> {
    use std::f32::consts::{PI, TAU};
    let mut pos: Vec<[f32; 3]> = vec![];
    let (mut tris, mut edges): (Vec<[u32; 3]>, Vec<u8>) = (vec![], vec![]);
    // Un polygone devient un éventail de triangles dont seuls les vrais côtés sont marqués
    let mut poly = |p: &[u32]| {
        for k in 1..p.len() - 1 {
            tris.push([p[0], p[k], p[k + 1]]);
            edges.push(crate::model3d::fan_mask(k, p.len()));
        }
    };
    // Face plate : ses propres sommets, pour garder des arêtes nettes à l'éclairage
    fn flat(pos: &mut Vec<[f32; 3]>, pts: &[[f32; 3]]) -> Vec<u32> {
        let base = pos.len() as u32;
        pos.extend_from_slice(pts);
        (base..base + pts.len() as u32).collect()
    }
    match id {
        "@cube" => {
            let v: Vec<[f32; 3]> = (0..8).map(|i| [if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 }, if i & 4 == 0 { -1.0 } else { 1.0 }]).collect();
            for q in [[1usize, 3, 7, 5], [0, 4, 6, 2], [2, 6, 7, 3], [0, 1, 5, 4], [4, 5, 7, 6], [0, 2, 3, 1]] {
                let f = flat(&mut pos, &q.map(|i| v[i]));
                poly(&f);
            }
        }
        "@plane" => {
            pos = vec![[-1.0, 0.0, -1.0], [-1.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 0.0, -1.0]];
            poly(&[0, 1, 2, 3]);
        }
        "@sphere" => {
            let (segs, rings) = (32u32, 16u32);
            pos.push([0.0, 1.0, 0.0]);
            for j in 1..rings {
                let th = PI * j as f32 / rings as f32;
                for i in 0..segs {
                    let ph = TAU * i as f32 / segs as f32;
                    pos.push([th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin()]);
                }
            }
            pos.push([0.0, -1.0, 0.0]);
            let at = |i: u32, j: u32| 1 + (j - 1) * segs + i % segs;
            let last = pos.len() as u32 - 1;
            for i in 0..segs {
                poly(&[0, at(i + 1, 1), at(i, 1)]);
                for j in 1..rings - 1 {
                    poly(&[at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)]);
                }
                poly(&[last, at(i, rings - 1), at(i + 1, rings - 1)]);
            }
        }
        "@cylinder" | "@cone" => {
            let segs = 32u32;
            let cone = id == "@cone";
            for i in 0..segs {
                let ph = TAU * i as f32 / segs as f32;
                pos.push([ph.cos(), -1.0, ph.sin()]);
            }
            if cone {
                pos.push([0.0, 1.0, 0.0]);
                for i in 0..segs {
                    poly(&[segs, (i + 1) % segs, i]);
                }
            } else {
                for i in 0..segs {
                    let ph = TAU * i as f32 / segs as f32;
                    pos.push([ph.cos(), 1.0, ph.sin()]);
                }
                for i in 0..segs {
                    poly(&[segs + i, segs + (i + 1) % segs, (i + 1) % segs, i]);
                }
                let top: Vec<[f32; 3]> = (0..segs).rev().map(|i| pos[(segs + i) as usize]).collect();
                let f = flat(&mut pos, &top);
                poly(&f);
            }
            let bottom: Vec<[f32; 3]> = (0..segs).map(|i| pos[i as usize]).collect();
            let f = flat(&mut pos, &bottom);
            poly(&f);
        }
        "@torus" => {
            let (major, minor, big, small) = (48u32, 16u32, 1.0f32, 0.25f32);
            for i in 0..major {
                let u = TAU * i as f32 / major as f32;
                for j in 0..minor {
                    let v = TAU * j as f32 / minor as f32;
                    pos.push([(big + small * v.cos()) * u.cos(), small * v.sin(), (big + small * v.cos()) * u.sin()]);
                }
            }
            let at = |i: u32, j: u32| (i % major) * minor + j % minor;
            for i in 0..major {
                for j in 0..minor {
                    poly(&[at(i, j), at(i, j + 1), at(i + 1, j + 1), at(i + 1, j)]);
                }
            }
        }
        _ => return None,
    }
    crate::model3d::Model::exact(pos, tris, None, Some(edges)).ok()
}

pub fn resolve(scene_path: &Path, file: &str) -> PathBuf {
    let p = Path::new(file);
    if p.is_absolute() { p.to_path_buf() } else { scene_path.parent().unwrap_or(Path::new("")).join(p) }
}

/// Inverse d'une matrice 3 × 3 (None si elle est plate)
fn inverse3(m: [[f32; 3]; 3]) -> Option<[[f32; 3]; 3]> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-12 {
        return None;
    }
    let c = |r0: usize, c0: usize, r1: usize, c1: usize| m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0];
    let i = 1.0 / det;
    Some([
        [c(1, 1, 2, 2) * i, -c(0, 1, 2, 2) * i, c(0, 1, 1, 2) * i],
        [-c(1, 0, 2, 2) * i, c(0, 0, 2, 2) * i, -c(0, 0, 1, 2) * i],
        [c(1, 0, 2, 1) * i, -c(0, 0, 2, 1) * i, c(0, 0, 1, 1) * i],
    ])
}

/// Rotation (degrés) qui tourne l'avant d'un objet (-Z) vers la direction `d`, la tête vers le haut (+Y)
pub fn aim_rotation(d: [f32; 3], roll: f32) -> Option<[f32; 3]> {
    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if l < 1e-6 {
        return None;
    }
    let d = [d[0] / l, d[1] / l, d[2] / l];
    // -Z tourné de a autour de X puis de b autour de Y : (-cos a sin b, sin a, -cos a cos b)
    let a = d[1].clamp(-1.0, 1.0).asin();
    let b = (-d[0]).atan2(-d[2]);
    Some([a.to_degrees(), b.to_degrees(), roll])
}

/// Matrice de rotation d'un objet (X, puis Y, puis Z, en degrés), lignes par lignes.
pub fn rot_matrix(rot: [f32; 3]) -> [[f32; 3]; 3] {
    let col = |e: [f32; 3]| {
        let o = Object { rot, scale: [1.0; 3], pos: [0.0; 3], ..Object::new(String::new(), String::new()) };
        transform(&o, e)
    };
    let (x, y, z) = (col([1.0, 0.0, 0.0]), col([0.0, 1.0, 0.0]), col([0.0, 0.0, 1.0]));
    [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]]
}

/// Angles X Y Z (degrés) d'une matrice de rotation, dans l'ordre de `transform`.
pub fn rot_euler(m: [[f32; 3]; 3]) -> [f32; 3] {
    let sy = (-m[2][0]).clamp(-1.0, 1.0);
    let b = sy.asin();
    let (a, c) = if sy.abs() < 0.9999 {
        (m[2][1].atan2(m[2][2]), m[1][0].atan2(m[0][0]))
    } else {
        // Blocage de cardan : seule la somme compte, Z prend tout
        (0.0, (-m[0][1]).atan2(m[1][1]))
    };
    [a.to_degrees(), b.to_degrees(), c.to_degrees()]
}

/// Tourne un objet d'un angle (radians) autour d'un axe exprimé dans l'espace de son parent.
pub fn rotate_by(o: &mut Object, axis: [f32; 3], angle: f32) {
    let l = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if l < 1e-9 {
        return;
    }
    let k = [axis[0] / l, axis[1] / l, axis[2] / l];
    let (s, c) = angle.sin_cos();
    let t = 1.0 - c;
    let r = [
        [c + k[0] * k[0] * t, k[0] * k[1] * t - k[2] * s, k[0] * k[2] * t + k[1] * s],
        [k[1] * k[0] * t + k[2] * s, c + k[1] * k[1] * t, k[1] * k[2] * t - k[0] * s],
        [k[2] * k[0] * t - k[1] * s, k[2] * k[1] * t + k[0] * s, c + k[2] * k[2] * t],
    ];
    let m = rot_matrix(o.rot);
    let mut out = [[0.0f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = (0..3).map(|n| r[i][n] * m[n][j]).sum();
        }
    }
    o.rot = rot_euler(out);
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

/// Le modèle d'origine d'un objet (forme de base ou fichier), tel qu'il est avant d'être placé dans la scène.
pub fn source_model(scene_path: &Path, o: &Object) -> Result<Arc<crate::model3d::Model>, String> {
    if o.file.starts_with('@') {
        primitive(&o.file).map(Arc::new).ok_or_else(|| format!("forme inconnue : {}", o.file))
    } else {
        cached(&resolve(scene_path, &o.file))
    }
}

/// Réunit les objets visibles en un seul modèle, Y en haut.
/// Renvoie aussi, pour chaque objet, l'erreur de chargement éventuelle.
pub fn build(scene: &Scene, scene_path: &Path) -> (Result<crate::model3d::Model, String>, Vec<Option<String>>) {
    let (m, e, _) = build_ranges(scene, scene_path);
    (m, e)
}

/// Mélange les formes d'un objet dans son modèle (seulement celles qui ont autant de sommets que lui).
pub fn blend_morphs(scene_path: &Path, o: &Object, m: Arc<crate::model3d::Model>) -> Arc<crate::model3d::Model> {
    let active: Vec<&Morph> = o.morphs.iter().filter(|x| x.value.abs() > 1e-4).collect();
    if active.is_empty() {
        return m;
    }
    let mut out = (*m).clone();
    for mo in active {
        let Ok(t) = cached(&resolve(scene_path, &mo.file)) else { continue };
        if t.positions.len() != m.positions.len() {
            continue;
        }
        for (i, p) in out.positions.iter_mut().enumerate() {
            for k in 0..3 {
                p[k] += (t.positions[i][k] - m.positions[i][k]) * mo.value;
            }
        }
    }
    Arc::new(out)
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
        let loaded = if o.file.starts_with('@') { primitive(&o.file).map(Arc::new).ok_or_else(|| format!("forme inconnue : {}", o.file)) } else { cached(&path) };
        let loaded = loaded.map(|m| blend_morphs(scene_path, o, m));
        match loaded {
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
                    crate::rig::deform(scene, k, &mut pos[base as usize..]);
                    continue;
                }
                ranges.push((k, tris.len(), tris.len() + m.tris.len()));
                pos.extend(m.positions.iter().map(|p| scene.world(k, if m.z_up { [p[0], p[2], -p[1]] } else { *p })));
                crate::rig::deform(scene, k, &mut pos[base as usize..]);
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
        // Scène vide : une scène vide qu'on peut quand même parcourir (grille, caméra)
        let others = if others.is_empty() { vec![[0.0; 3]] } else { others };
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
    fn contraintes_copier() {
        let mut sc = Scene::default();
        let a = sc.add_role("cible", "");
        let b = sc.add_role("suiveur", "");
        sc.objects[a].pos = [1.0, 2.0, 3.0];
        sc.objects[a].rot = [0.0, 45.0, 0.0];
        sc.objects[b].copy_loc = "cible".into();
        sc.objects[b].copy_rot = "cible".into();
        assert!(sc.apply_copies());
        assert_eq!(sc.objects[b].pos, [1.0, 2.0, 3.0]);
        assert!((sc.objects[b].rot[1] - 45.0).abs() < 0.01);
        sc.objects[b].copy_loc.clear();
        sc.objects[b].limit = Some([[0.0; 3], [0.5, 0.5, 0.5]]);
        assert!(sc.apply_copies());
        assert_eq!(sc.objects[b].pos, [0.5, 0.5, 0.5]);
        assert!(!sc.apply_copies());
    }

    #[test]
    fn formes() {
        let mut m = Morph { name: "sourire".into(), file: String::new(), value: 0.0, keys: vec![] };
        assert_eq!(m.value_at(5.0), None);
        m.set_key(0);
        m.value = 1.0;
        m.set_key(10);
        assert!((m.value_at(5.0).unwrap() - 0.5).abs() < 1e-6);
        assert_eq!(m.value_at(20.0), Some(1.0));
        let mut sc = Scene::default();
        let k = sc.add_primitive("@cube");
        sc.objects[k].morphs.push(m);
        assert!(sc.apply_frame(2.0));
        assert!((sc.objects[k].morphs[0].value - 0.2).abs() < 1e-6);
    }

    #[test]
    fn courbes() {
        let k = |frame: i32, x: f32| Key { frame, pos: [x, 0.0, 0.0], rot: [0.0; 3], scale: [1.0; 3], ease: true, interp: String::new(), handle: 0, handles: vec![] };
        let mut keys = vec![k(0, 0.0), k(10, 10.0), k(20, 0.0)];
        // Passe par les clés, reste dans les bornes au sommet (poignées automatiques à plat)
        assert!((channel_at(&keys, 0, 10.0) - 10.0).abs() < 1e-3);
        assert!((0..=200).all(|i| channel_at(&keys, 0, i as f32 / 10.0) <= 10.0 + 1e-3));
        // Linéaire : à mi-chemin, la moitié ; constant : la valeur de départ
        keys[0].interp = "linear".into();
        assert!((channel_at(&keys, 0, 5.0) - 5.0).abs() < 1e-3);
        keys[0].interp = "constant".into();
        assert!(channel_at(&keys, 0, 9.0).abs() < 1e-3);
        // Poignées libres : une poignée qui monte fait monter la courbe au départ
        keys[0].interp = "bezier".into();
        keys[0].handle = 2;
        keys[0].handles = vec![[-3.0, 0.0, 3.0, 9.0]; 9];
        let free = channel_at(&keys, 0, 2.0);
        keys[0].handle = 0;
        assert!(free > channel_at(&keys, 0, 2.0));
        // Ancien fichier sans « interp » : se relit
        let old: Key = serde_json::from_str(r#"{"frame":3,"pos":[0,0,0],"rot":[0,0,0],"scale":[1,1,1],"ease":false}"#).unwrap();
        assert_eq!(old.mode(), "linear");
    }

    #[test]
    fn rotations() {
        for rot in [[0.0, 0.0, 0.0], [30.0, -40.0, 120.0], [-170.0, 10.0, 5.0]] {
            let back = rot_euler(rot_matrix(rot));
            let (a, b) = (rot_matrix(rot), rot_matrix(back));
            for i in 0..3 {
                for j in 0..3 {
                    assert!((a[i][j] - b[i][j]).abs() < 1e-4, "{rot:?} -> {back:?}");
                }
            }
        }
        let mut o = Object::new("a".into(), "@cube".into());
        rotate_by(&mut o, [0.0, 1.0, 0.0], std::f32::consts::FRAC_PI_2);
        assert!((o.rot[1] - 90.0).abs() < 1e-3);
    }

    #[test]
    fn scene_aller_retour_et_fusion() {
        let dir = std::env::temp_dir().join("aryadne-scene-test");
        let _ = std::fs::create_dir_all(&dir);
        crate::vault::atomic_write(dir.join("tri.obj"), "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
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
        // Formes de base : fermées (chaque arête partagée par deux faces), tournées vers l'extérieur
        for (id, _) in PRIMITIVES {
            let m = primitive(id).unwrap();
            let mut count = std::collections::HashMap::new();
            let mut vol = 0.0f32;
            for t in &m.tris {
                for k in 0..3 {
                    // Les faces plates ont leurs propres sommets : on compare les positions, pas les indices
                    let key = |i: u32| m.positions[i as usize].map(|x| (x * 1000.0).round() as i32);
                    let (a, b) = (key(t[k]), key(t[(k + 1) % 3]));
                    *count.entry(if a < b { (a, b) } else { (b, a) }).or_insert(0) += 1;
                }
                let (a, b, c) = (m.positions[t[0] as usize], m.positions[t[1] as usize], m.positions[t[2] as usize]);
                vol += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0]);
            }
            if id != "@plane" {
                assert!(count.values().all(|n| *n == 2), "{id} n'est pas fermé");
                assert!(vol > 0.0, "{id} est tourné vers l'intérieur");
            }
        }
        let st = Scene::starter();
        assert!(build_ranges(&st, &file).0.unwrap().tris.len() == 12 && st.lights().len() == 1);
        // Animation : deux clés, la pose au milieu est entre les deux, et en douceur aux extrémités
        let mut a = Scene::default();
        let k = a.add_role("cube", "");
        a.objects[k].set_key(0);
        a.objects[k].pos = [10.0, 0.0, 0.0];
        a.objects[k].set_key(20);
        assert_eq!(a.objects[k].keys.len(), 2);
        assert!(a.apply_frame(10.0));
        assert!((a.objects[k].pos[0] - 5.0).abs() < 1e-4);
        a.apply_frame(2.0);
        assert!(a.objects[k].pos[0] < 0.5, "départ en douceur");
        a.apply_frame(99.0);
        assert_eq!(a.objects[k].pos[0], 10.0);
        // Reposer une clé sur la même image la remplace
        a.objects[k].pos = [4.0, 0.0, 0.0];
        a.objects[k].set_key(20);
        assert!(a.objects[k].keys.len() == 2 && a.objects[k].keys[1].pos[0] == 4.0);
        // Lumière : au repos elle vient d'en haut ; tournée de 90° autour de Z, elle vient de -X
        let mut l = Scene::default();
        let k = l.add_role("soleil", "light");
        assert!((l.light().unwrap().0[1] - 1.0).abs() < 1e-5);
        l.objects[k].rot = [0.0, 0.0, 90.0];
        assert!((l.light().unwrap().0[0] + 1.0).abs() < 1e-5);
        // Une ponctuelle donne sa position, et n'est pas prise pour le soleil
        let q = l.add_role("lampe", "light");
        l.objects[q].point = true;
        l.objects[q].pos = [1.0, 2.0, 3.0];
        let all = l.lights();
        assert!(all.len() == 2 && all[1].point && all[1].at == [1.0, 2.0, 3.0]);
        // Sans modèle, la scène s'ouvre quand même
        assert!(build_ranges(&l, &file).0.is_ok());
        // Objet caché ou fichier manquant : la scène reste lisible
        s.objects[0].visible = false;
        s.objects[1].file = "absent.obj".into();
        let (m, errs) = build(&s, &file);
        assert!(m.is_ok() && errs[1].is_some());
    }
}

/// Les courbes d'un objet pour l'éditeur de courbes commun : une par canal (position, rotation, taille ; X Y Z),
/// en secondes. Les poignées automatiques sont données telles quelles ; elles restent automatiques au retour
/// tant qu'on ne les tire pas.
pub fn object_curves(keys: &[Key], fps: f32) -> Vec<crate::curves::Curve> {
    use crate::curves::{Curve, CK, X, Y, Z};
    let fps = fps.max(1.0);
    CHANNELS
        .iter()
        .enumerate()
        .map(|(c, (group, axis))| Curve {
            label: format!("{} {axis}", crate::i18n::t(group)),
            color: [X, Y, Z][c % 3],
            link: 0,
            keys: (0..keys.len())
                .map(|i| {
                    let k = &keys[i];
                    let (ease, tan) = match k.mode() {
                        "bezier" => {
                            let (l, r) = handles_of(keys, i, c);
                            (crate::keys::Ease::Smooth, Some([l[0] / fps, l[1], r[0] / fps, r[1]]))
                        }
                        m => (crate::keys::Ease::from_word(m).unwrap_or(crate::keys::Ease::Linear), None),
                    };
                    CK { t: k.frame as f32 / fps, v: k.get(c), ease, tan }
                })
                .collect(),
        })
        .collect()
}

/// Remet les courbes éditées dans les clés de l'objet
pub fn apply_object_curves(keys: &mut Vec<Key>, curves: &[crate::curves::Curve], fps: f32) {
    let fps = fps.max(1.0);
    let before = object_curves(keys, fps);
    let Some(first) = curves.first() else { return };
    let mut out = vec![];
    for i in 0..first.keys.len() {
        let frame = (first.keys[i].t * fps).round() as i32;
        let mut k = keys.iter().find(|x| x.frame == frame).cloned().or_else(|| keys.get(i).cloned()).unwrap_or(Key { frame, pos: [0.0; 3], rot: [0.0; 3], scale: [1.0; 3], ease: true, interp: String::new(), handle: 0, handles: vec![] });
        k.frame = frame;
        for (c, cv) in curves.iter().enumerate().take(9) {
            if let Some(x) = cv.keys.get(i) {
                k.set(c, x.v);
            }
        }
        // Passage : Bézier (poignées auto ou tirées), sinon le lissage choisi
        let ck = first.keys[i];
        let custom: Vec<Option<[f32; 4]>> = curves
            .iter()
            .enumerate()
            .take(9)
            .map(|(c, cv)| {
                let t = cv.keys.get(i)?.tan?;
                let was = before.get(c).and_then(|b| b.keys.get(i)).and_then(|b| b.tan);
                // Inchangée depuis qu'on l'a donnée : elle reste automatique
                if was.is_some_and(|w| (0..4).all(|j| (w[j] - t[j]).abs() < 1e-4)) && keys.get(i).is_some_and(|o| o.frame == frame) {
                    None
                } else {
                    Some(t)
                }
            })
            .collect();
        let bezier = ck.tan.is_some() || (ck.tan.is_none() && ck.ease == crate::keys::Ease::Smooth);
        if bezier {
            if k.mode() != "bezier" {
                k.interp = "bezier".into();
            }
            if custom.iter().any(|x| x.is_some()) {
                k.handle = 2;
                k.handles = (0..9)
                    .map(|c| {
                        let t = curves.get(c).and_then(|cv| cv.keys.get(i)).and_then(|x| x.tan).unwrap_or([-1.0 / fps, 0.0, 1.0 / fps, 0.0]);
                        [t[0] * fps, t[1], t[2] * fps, t[3]]
                    })
                    .collect();
            } else if k.handle == 2 && curves.iter().any(|cv| cv.keys.get(i).is_some_and(|x| x.tan.is_none())) {
                // Remis en « auto » dans l'éditeur
                k.handle = 0;
                k.handles.clear();
            }
        } else {
            k.interp = ck.ease.word().to_string();
            k.handle = 0;
            k.handles.clear();
        }
        out.push(k);
    }
    *keys = out;
}

#[cfg(test)]
mod curve_tests {
    use super::*;

    #[test]
    fn viser() {
        let mut sc = Scene::default();
        let mut cam = Object::new("cam".into(), "@camera".into());
        cam.pos = [0.0, 2.0, 10.0];
        cam.aim = "cible".into();
        let mut cible = Object::new("cible".into(), "x.obj".into());
        cible.pos = [3.0, 0.0, 0.0];
        sc.objects = vec![cam, cible];
        assert!(sc.apply_aims());
        // L'avant (-Z) de la caméra pointe vers la cible
        let o = &sc.objects[0];
        let f = transform(&Object { pos: [0.0; 3], scale: [1.0; 3], ..o.clone() }, [0.0, 0.0, -1.0]);
        let d = [3.0, -2.0, -10.0];
        let l = (9.0f32 + 4.0 + 100.0).sqrt();
        for i in 0..3 {
            assert!((f[i] - d[i] / l).abs() < 1e-4, "{f:?}");
        }
        assert!(!sc.apply_aims());
        // Sous un parent tourné de 90° autour de Y : l'avant pointe toujours vers la cible
        let mut rig = Object::new("rig".into(), "@empty".into());
        rig.rot = [0.0, 90.0, 0.0];
        rig.pos = [0.0, 2.0, 10.0];
        sc.objects[0].pos = [0.0; 3];
        sc.objects[0].parent = "rig".into();
        sc.objects.push(rig);
        sc.apply_aims();
        let w0 = sc.world(0, [0.0; 3]);
        let w1 = sc.world(0, [0.0, 0.0, -1.0]);
        let f = [w1[0] - w0[0], w1[1] - w0[1], w1[2] - w0[2]];
        for i in 0..3 {
            assert!((f[i] - d[i] / l).abs() < 1e-3, "{f:?}");
        }
    }

    #[test]
    fn courbes_de_scene_aller_retour() {
        let k = |frame: i32, x: f32| Key { frame, pos: [x, 0.0, 0.0], rot: [0.0; 3], scale: [1.0; 3], ease: true, interp: String::new(), handle: 0, handles: vec![] };
        let mut keys = vec![k(0, 0.0), k(12, 4.0), k(24, 0.0)];
        let orig = keys.clone();
        let cv = object_curves(&keys, 24.0);
        // Sans rien toucher : les clés reviennent identiques (poignées auto comprises)
        apply_object_curves(&mut keys, &cv, 24.0);
        assert_eq!(keys, orig);
        // La courbe donne la même valeur que la scène
        for f in [3.0f32, 9.0, 17.0] {
            assert!((crate::curves::eval(&cv[0].keys, f / 24.0) - channel_at(&orig, 0, f)).abs() < 1e-3);
        }
        // Une valeur changée garde des poignées automatiques ; un passage linéaire devient « linear »
        let mut cv2 = cv.clone();
        cv2[0].keys[1].v = 6.0;
        cv2[0].keys[0].ease = crate::keys::Ease::Linear;
        cv2[0].keys[0].tan = None;
        apply_object_curves(&mut keys, &cv2, 24.0);
        assert_eq!(keys[1].pos[0], 6.0);
        assert_eq!(keys[1].handle, 0);
        assert_eq!(keys[0].mode(), "linear");
    }
}
