//! Montage d'animation (NLA) : des actions réutilisables (les clés de plusieurs objets, par exemple
//! un cycle de marche) posées en bandes sur la timeline, répétées, étirées, fondues et empilées.
//! Ordre du calcul d'une pose : les clés de base de l'objet, puis les bandes (rangée après rangée),
//! puis les calques d'animation additifs de la scène.

use crate::scene::{sample_keys, Key, Object, Scene};
use serde::{Deserialize, Serialize};

/// Position, rotation, taille
pub type Pose = ([f32; 3], [f32; 3], [f32; 3]);

/// Les clés d'un objet dans une action (il est retrouvé par son nom)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ActionTrack {
    pub object: String,
    pub keys: Vec<Key>,
}

/// Une action : une animation nommée, réutilisable dans plusieurs bandes
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Action {
    pub name: String,
    pub tracks: Vec<ActionTrack>,
}

impl Action {
    /// Première et dernière image de l'action (None si elle n'a aucune clé)
    pub fn range(&self) -> Option<(i32, i32)> {
        let all = || self.tracks.iter().flat_map(|t| t.keys.iter().map(|k| k.frame));
        Some((all().min()?, all().max()?))
    }

    /// Durée en images (au moins 1 : une pose seule se tient)
    pub fn len(&self) -> f32 {
        self.range().map(|(a, b)| (b - a).max(1) as f32).unwrap_or(1.0)
    }
}

fn one() -> f32 {
    1.0
}
fn is_one(v: &f32) -> bool {
    (*v - 1.0).abs() < 1e-6
}
fn is_zero(v: &i32) -> bool {
    *v == 0
}
fn no(v: &bool) -> bool {
    !*v
}

/// Une bande : une action posée sur la timeline
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Strip {
    pub action: String,
    /// Image de la scène où la bande commence
    pub start: i32,
    /// Nombre de répétitions (2,5 : deux cycles et demi)
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub repeat: f32,
    /// Étirement dans le temps (2 : deux fois plus lent)
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub scale: f32,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub weight: f32,
    /// Fondus d'entrée et de sortie, en images
    #[serde(default, skip_serializing_if = "is_zero")]
    pub blend_in: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub blend_out: i32,
    /// Ajoute le mouvement de l'action (par rapport à sa première pose) au lieu de remplacer la pose
    #[serde(default, skip_serializing_if = "no")]
    pub additive: bool,
    /// À chaque cycle, repart d'où le précédent est arrivé (un cycle de marche avance)
    #[serde(default, skip_serializing_if = "no")]
    pub travel: bool,
    /// Après la fin, garde la dernière pose
    #[serde(default, skip_serializing_if = "no")]
    pub hold: bool,
    #[serde(default, skip_serializing_if = "no")]
    pub mute: bool,
    /// Rangée dans la timeline : les rangées du haut passent après celles du bas
    #[serde(default)]
    pub row: u8,
}

impl Strip {
    pub fn new(action: &str, start: i32, row: u8) -> Strip {
        Strip { action: action.to_string(), start, repeat: 1.0, scale: 1.0, weight: 1.0, blend_in: 0, blend_out: 0, additive: false, travel: false, hold: false, mute: false, row }
    }

    /// Durée de la bande sur la timeline
    pub fn len(&self, action: &Action) -> f32 {
        action.len() * self.scale.max(0.01) * self.repeat.max(0.01)
    }

    /// À l'image `f` de la scène : l'image de l'action à lire, le nombre de cycles déjà finis, et
    /// l'influence (poids et fondus). None hors de la bande.
    pub fn local(&self, f: f32, action: &Action) -> Option<(f32, f32, f32)> {
        let (first, _) = action.range()?;
        let (alen, len) = (action.len(), self.len(action));
        let mut x = f - self.start as f32;
        if x < 0.0 || !x.is_finite() {
            return None;
        }
        if x > len {
            if !self.hold {
                return None;
            }
            x = len;
        }
        let u = x / self.scale.max(0.01);
        // À la toute fin, on lit la fin du dernier cycle (et pas le début du suivant)
        let total = alen * self.repeat.max(0.01);
        let (cycles, t) = if u >= total - 1e-4 {
            let done = (self.repeat.max(0.01) - 1e-4).floor().max(0.0);
            (done, total - done * alen)
        } else {
            ((u / alen).floor(), u % alen)
        };
        let ramp = |d: f32, n: i32| if n > 0 { (d / n as f32).clamp(0.0, 1.0) } else { 1.0 };
        let w = self.weight.clamp(0.0, 1.0) * ramp(x, self.blend_in) * if self.hold { 1.0 } else { ramp(len - x, self.blend_out) };
        Some((first as f32 + t, cycles, w))
    }
}

fn mix(a: [f32; 3], b: [f32; 3], w: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * w, a[1] + (b[1] - a[1]) * w, a[2] + (b[2] - a[2]) * w]
}

/// Passe les bandes sur la pose de base de l'objet `name` à l'image `f`. `strips` dans l'ordre de
/// la liste, rangée après rangée. Un objet sans clé de base prend la pose de sa première bande.
pub fn apply_strips(mut pose: Option<Pose>, name: &str, f: f32, actions: &[Action], strips: &[Strip]) -> Option<Pose> {
    if strips.is_empty() {
        return pose;
    }
    let mut order: Vec<&Strip> = strips.iter().filter(|s| !s.mute).collect();
    order.sort_by_key(|s| s.row);
    for s in order {
        let Some(action) = actions.iter().find(|a| a.name == s.action) else { continue };
        let Some(track) = action.tracks.iter().find(|t| t.object == name) else { continue };
        let Some((t, cycles, w)) = s.local(f, action) else { continue };
        let Some((mut p, r, sc)) = sample_keys(&track.keys, t) else { continue };
        let (Some(k0), Some(k1)) = (track.keys.first(), track.keys.last()) else { continue };
        if s.travel && cycles > 0.0 {
            for i in 0..3 {
                p[i] += (k1.pos[i] - k0.pos[i]) * cycles;
            }
        }
        pose = match pose {
            Some((bp, br, bs)) if s.additive => {
                let add = |b: [f32; 3], v: [f32; 3], r0: [f32; 3]| [b[0] + (v[0] - r0[0]) * w, b[1] + (v[1] - r0[1]) * w, b[2] + (v[2] - r0[2]) * w];
                Some((add(bp, p, k0.pos), add(br, r, k0.rot), add(bs, sc, k0.scale)))
            }
            Some((bp, br, bs)) => Some((mix(bp, p, w), mix(br, r, w), mix(bs, sc, w))),
            // Rien dessous : une bande additive n'a rien à quoi s'ajouter, une bande normale donne sa pose
            None if s.additive => None,
            None => Some((p, r, sc)),
        };
    }
    pose
}

/// Un nom d'action libre dans la scène
pub fn free_name(scene: &Scene, base: &str) -> String {
    let mut name = base.to_string();
    let mut n = 2;
    while scene.actions.iter().any(|a| a.name == name) {
        name = format!("{base} {n}");
        n += 1;
    }
    name
}

/// La première rangée où une bande de `start` à `end` ne chevauche aucune autre
pub fn free_row(scene: &Scene, start: f32, end: f32) -> u8 {
    (0..=u8::MAX)
        .find(|row| {
            !scene.strips.iter().filter(|s| s.row == *row).any(|s| {
                let len = scene.actions.iter().find(|a| a.name == s.action).map(|a| s.len(a)).unwrap_or(1.0);
                (s.start as f32) < end && s.start as f32 + len > start
            })
        })
        .unwrap_or(0)
}

/// Fait une action des clés de base des objets donnés (ceux qui en ont) : les clés quittent les
/// objets, qui gardent une seule clé (leur première pose, pour avoir un repos), et une bande est
/// posée au même endroit. L'animation reste donc la même. Rend l'indice de la bande.
pub fn make_action(scene: &mut Scene, objects: &[usize], name: &str) -> Option<usize> {
    let tracks: Vec<ActionTrack> = objects.iter().filter_map(|&k| scene.objects.get(k)).filter(|o| o.keys.len() >= 2).map(|o| ActionTrack { object: o.name.clone(), keys: o.keys.clone() }).collect();
    if tracks.is_empty() {
        return None;
    }
    let action = Action { name: free_name(scene, if name.trim().is_empty() { "action" } else { name.trim() }), tracks };
    let (first, last) = action.range()?;
    for t in &action.tracks {
        if let Some(o) = scene.objects.iter_mut().find(|o| o.name == t.object) {
            o.keys.truncate(1);
            // Un cycle réglé sur l'objet (boucle de ses clés) n'a plus de sens avec une seule clé
            o.cycle = Default::default();
        }
    }
    let row = free_row(scene, first as f32, last as f32);
    scene.strips.push(Strip::new(&action.name, first, row));
    scene.actions.push(action);
    Some(scene.strips.len() - 1)
}

/// Retire une action et ses bandes
pub fn remove_action(scene: &mut Scene, name: &str) {
    scene.actions.retain(|a| a.name != name);
    scene.strips.retain(|s| s.action != name);
}

/// Un objet renommé garde ses pistes dans les actions
pub fn rename_object(scene: &mut Scene, old: &str, new: &str) {
    for t in scene.actions.iter_mut().flat_map(|a| a.tracks.iter_mut()).filter(|t| t.object == old) {
        t.object = new.to_string();
    }
}

/// Pose complète d'un objet de la scène à l'image `f` : base, bandes, puis calques additifs
/// (sauf le calque `skip`). None si rien ne l'anime.
pub fn full_pose(o: &Object, f: f32, scene_layers: &[crate::scene::AnimLayer], actions: &[Action], strips: &[Strip], skip: Option<&str>) -> Option<Pose> {
    let base = apply_strips(o.pose_at(f), &o.name, f, actions, strips);
    o.add_layers(base, f, scene_layers, skip)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk() -> (Scene, usize) {
        // Un cube qui avance de 4 en X sur 10 images, en montant à mi-chemin
        let mut sc = Scene::default();
        let k = sc.add_primitive("@cube");
        for (f, x, y) in [(0, 0.0, 0.0), (5, 2.0, 1.0), (10, 4.0, 0.0)] {
            sc.objects[k].pos = [x, y, 0.0];
            sc.objects[k].set_base_key(f);
        }
        for key in &mut sc.objects[k].keys {
            key.interp = "linear".into();
        }
        (sc, k)
    }

    #[test]
    fn action_et_bande() {
        let (mut sc, k) = walk();
        sc.apply_frame(5.0);
        let before = sc.objects[k].pos;
        let s = make_action(&mut sc, &[k], "marche").unwrap();
        // Les clés sont parties dans l'action, l'objet garde son repos, et l'animation n'a pas changé
        assert_eq!((sc.objects[k].keys.len(), sc.actions[0].tracks[0].keys.len(), sc.strips[s].start), (1, 3, 0));
        for f in [0.0, 2.5, 5.0, 10.0] {
            sc.apply_frame(f);
            let want = [f * 0.4, if f <= 5.0 { f * 0.2 } else { 2.0 - f * 0.2 }, 0.0];
            assert!((0..3).all(|i| (sc.objects[k].pos[i] - want[i]).abs() < 1e-4), "{f} {:?}", sc.objects[k].pos);
        }
        sc.apply_frame(5.0);
        assert_eq!(sc.objects[k].pos, before);
        // Après la bande : retour au repos ; avec « tenir », la dernière pose reste
        sc.apply_frame(20.0);
        assert_eq!(sc.objects[k].pos, [0.0; 3]);
        sc.strips[s].hold = true;
        sc.apply_frame(20.0);
        assert!((sc.objects[k].pos[0] - 4.0).abs() < 1e-4);
        // Un objet sans piste dans l'action n'est pas touché ; une action sans clé ne se crée pas
        let other = sc.add_primitive("@sphere");
        sc.objects[other].pos = [7.0, 0.0, 0.0];
        sc.apply_frame(5.0);
        assert_eq!(sc.objects[other].pos, [7.0, 0.0, 0.0]);
        assert!(make_action(&mut sc, &[other, 99], "vide").is_none());
        // Deux actions du même nom : la seconde reçoit un autre nom
        assert_eq!(free_name(&sc, "marche"), "marche 2");
    }

    #[test]
    fn cycles_etirement_et_fondus() {
        let (mut sc, k) = walk();
        let s = make_action(&mut sc, &[k], "marche").unwrap();
        // Trois cycles : le mouvement recommence à chaque fois
        sc.strips[s].repeat = 3.0;
        let a = sc.actions[0].clone();
        assert_eq!(sc.strips[s].len(&a), 30.0);
        sc.apply_frame(15.0);
        assert!((sc.objects[k].pos[0] - 2.0).abs() < 1e-4 && (sc.objects[k].pos[1] - 1.0).abs() < 1e-4);
        // À la toute fin on lit la fin du cycle, pas le début du suivant
        sc.apply_frame(30.0);
        assert!((sc.objects[k].pos[0] - 4.0).abs() < 1e-4);
        // « Avance à chaque cycle » : chaque cycle repart d'où le précédent est arrivé
        sc.strips[s].travel = true;
        sc.apply_frame(15.0);
        assert!((sc.objects[k].pos[0] - 6.0).abs() < 1e-4, "{:?}", sc.objects[k].pos);
        sc.apply_frame(30.0);
        assert!((sc.objects[k].pos[0] - 12.0).abs() < 1e-4);
        // Deux cycles et demi : la bande s'arrête au milieu du troisième
        sc.strips[s].repeat = 2.5;
        sc.apply_frame(25.0);
        assert!((sc.objects[k].pos[0] - 10.0).abs() < 1e-4 && (sc.objects[k].pos[1] - 1.0).abs() < 1e-4, "{:?}", sc.objects[k].pos);
        // Deux fois plus lent, décalé de 100 images
        sc.strips[s] = Strip { scale: 2.0, start: 100, ..Strip::new("marche", 0, 0) };
        sc.apply_frame(110.0);
        assert!((sc.objects[k].pos[0] - 2.0).abs() < 1e-4);
        sc.apply_frame(99.0);
        assert_eq!(sc.objects[k].pos, [0.0; 3]);
        // Fondu d'entrée de 4 images : à 2 images, la moitié de l'action sur le repos
        sc.strips[s] = Strip { blend_in: 4, blend_out: 4, ..Strip::new("marche", 0, 0) };
        sc.apply_frame(2.0);
        assert!((sc.objects[k].pos[0] - 0.8 * 0.5).abs() < 1e-4, "{:?}", sc.objects[k].pos);
        sc.apply_frame(9.0);
        assert!((sc.objects[k].pos[0] - 3.6 * 0.25).abs() < 1e-4);
        // Poids à moitié, muet
        sc.strips[s] = Strip { weight: 0.5, ..Strip::new("marche", 0, 0) };
        sc.apply_frame(10.0);
        assert!((sc.objects[k].pos[0] - 2.0).abs() < 1e-4);
        sc.strips[s].mute = true;
        sc.apply_frame(10.0);
        assert_eq!(sc.objects[k].pos, [0.0; 3]);
        // Valeurs absurdes : pas de panique, pas de NaN
        sc.strips[s] = Strip { scale: 0.0, repeat: -3.0, ..Strip::new("marche", 0, 0) };
        sc.apply_frame(0.5);
        assert!(sc.objects[k].pos.iter().all(|x| x.is_finite()));
        assert!(sc.strips[s].local(f32::NAN, &a).is_none());
        sc.strips[s].action = "absente".into();
        sc.apply_frame(5.0);
    }

    #[test]
    fn bandes_empilees_et_calques() {
        let (mut sc, k) = walk();
        let s = make_action(&mut sc, &[k], "marche").unwrap();
        // Une seconde action « saut » (monte de 3 en Y), additive, sur la rangée du dessus
        let key = |f: i32, y: f32| Key { frame: f, pos: [50.0, y, 0.0], rot: [0.0; 3], scale: [1.0; 3], ease: false, interp: "linear".into(), handle: 0, handles: vec![] };
        sc.actions.push(Action { name: "saut".into(), tracks: vec![ActionTrack { object: sc.objects[k].name.clone(), keys: vec![key(0, 0.0), key(4, 3.0)] }] });
        let row = free_row(&sc, 3.0, 7.0);
        assert_eq!(row, 1, "la rangée 0 est prise par la marche");
        sc.strips.push(Strip { additive: true, ..Strip::new("saut", 3, row) });
        sc.apply_frame(5.0);
        // Marche à l'image 5 (x 2, y 1), plus la moitié du saut (y +1,5) ; le x de l'action additive ne compte pas (seul son écart)
        assert!((sc.objects[k].pos[0] - 2.0).abs() < 1e-4 && (sc.objects[k].pos[1] - 2.5).abs() < 1e-4, "{:?}", sc.objects[k].pos);
        // Non additive : elle remplace la pose du dessous
        sc.strips[1].additive = false;
        sc.apply_frame(5.0);
        assert!((sc.objects[k].pos[0] - 50.0).abs() < 1e-4);
        // L'ordre de la liste ne compte pas, seulement la rangée
        sc.strips.swap(0, 1);
        sc.apply_frame(5.0);
        assert!((sc.objects[k].pos[0] - 50.0).abs() < 1e-4);
        sc.strips.swap(0, 1);
        sc.strips[1].additive = true;
        // Un calque d'animation additif passe par-dessus les bandes, et sa clé tient compte d'elles
        sc.layers.push(crate::scene::AnimLayer { name: "retouche".into(), weight: 1.0, mute: false });
        sc.active_layer = "retouche".into();
        sc.set_anim_context();
        sc.apply_frame(5.0);
        sc.objects[k].pos[2] = 9.0;
        sc.objects[k].set_key(5);
        crate::scene::set_layer_context(&[], "");
        crate::scene::set_nla_context(&[], &[]);
        let lk = &sc.objects[k].layer_keys[0].keys[0];
        assert!(lk.pos[0].abs() < 1e-4 && lk.pos[1].abs() < 1e-4 && (lk.pos[2] - 9.0).abs() < 1e-4, "seul l'écart est dans le calque : {:?}", lk.pos);
        sc.apply_frame(5.0);
        assert!((sc.objects[k].pos[1] - 2.5).abs() < 1e-4 && (sc.objects[k].pos[2] - 9.0).abs() < 1e-4);
        // Les clés qu'on édite : celles du calque actif, sinon la base
        assert_eq!(sc.objects[k].keys_in("retouche").len(), 1);
        assert_eq!(sc.objects[k].keys_in("").len(), 1);
        assert_eq!(sc.objects[k].keys_in("retouche")[0].frame, 5);
        assert!(sc.objects[k].keys_in("autre").is_empty());
        sc.objects[k].keys_in_mut("retouche").unwrap().clear();
        assert!(sc.objects[k].keys_in("retouche").is_empty());
        // Renommer l'objet : l'action le suit ; retirer l'action retire ses bandes
        let old = sc.objects[k].name.clone();
        sc.rename(k, "héros");
        assert!(sc.actions.iter().all(|a| a.tracks.iter().all(|t| t.object == "héros")), "{old}");
        sc.apply_frame(5.0);
        assert!((sc.objects[k].pos[0] - 2.0).abs() < 1e-4);
        remove_action(&mut sc, "marche");
        assert_eq!((sc.actions.len(), sc.strips.len()), (1, 1));
        let _ = s;
        // Aller-retour par le fichier
        let json = serde_json::to_string(&sc).unwrap();
        let back: Scene = serde_json::from_str(&json).unwrap();
        assert_eq!((back.actions, back.strips), (sc.actions.clone(), sc.strips.clone()));
    }
}
