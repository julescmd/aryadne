//! Outils d'animation (dans l'esprit d'Animbot) : tweener, décaler les clés, insérer ou retirer une image,
//! décaler toute l'animation, copier et coller une pose, miroir, retour au repos, partager les clés.
//! Ils travaillent sur les objets d'une scène 3D ; l'interface est dans model3d.rs.

use crate::scene::{Key, Object};

/// Une pose : position, rotation (degrés), taille
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub scale: [f32; 3],
}

impl Pose {
    pub fn of(o: &Object) -> Pose {
        Pose { pos: o.pos, rot: o.rot, scale: o.scale }
    }

    fn get(&self, c: usize) -> f32 {
        match c {
            0..=2 => self.pos[c],
            3..=5 => self.rot[c - 3],
            _ => self.scale[(c - 6).min(2)],
        }
    }

    fn set(&mut self, c: usize, v: f32) {
        match c {
            0..=2 => self.pos[c] = v,
            3..=5 => self.rot[c - 3] = v,
            _ => self.scale[(c - 6).min(2)] = v,
        }
    }

    fn of_key(k: &Key) -> Pose {
        Pose { pos: k.pos, rot: k.rot, scale: k.scale }
    }
}

/// Donne la pose à l'objet et la garde en clé à l'image `f`
pub fn put(o: &mut Object, f: i32, p: Pose) {
    (o.pos, o.rot, o.scale) = (p.pos, p.rot, p.scale);
    o.set_key(f);
}

/// Les clés qui entourent l'image `f` (strictement avant, strictement après)
fn around(o: &Object, f: i32) -> (Option<&Key>, Option<&Key>) {
    (o.keys.iter().rev().find(|k| k.frame < f), o.keys.iter().find(|k| k.frame > f))
}

/// Tweener : la pose à l'image `f` entre la clé d'avant (w = 0) et celle d'après (w = 1).
/// Au-delà de 0..1, la pose dépasse (anticipation, dépassement). Rend false sans clé de chaque côté.
pub fn tween(o: &mut Object, f: i32, w: f32) -> bool {
    let (Some(a), Some(b)) = around(o, f) else { return false };
    let (a, b) = (Pose::of_key(a), Pose::of_key(b));
    let mut p = a;
    for c in 0..9 {
        p.set(c, a.get(c) + (b.get(c) - a.get(c)) * w);
    }
    put(o, f, p);
    true
}

/// Mélange vers une pose donnée (la pose copiée, celle de repos…) : w = 0 garde la pose, w = 1 la remplace
pub fn blend_to(o: &mut Object, f: i32, target: Pose, w: f32) {
    let now = o.pose_at(f as f32).map(|(pos, rot, scale)| Pose { pos, rot, scale }).unwrap_or(Pose::of(o));
    let mut p = now;
    for c in 0..9 {
        p.set(c, now.get(c) + (target.get(c) - now.get(c)) * w);
    }
    put(o, f, p);
}

/// Décale de `d` images les clés posées aux images `frames`. Une clé ne prend pas la place d'une autre :
/// celles qui se heurteraient restent. Rend les images où sont arrivées les clés déplacées.
pub fn nudge(o: &mut Object, frames: &[i32], d: i32) -> Vec<i32> {
    if d == 0 {
        return frames.to_vec();
    }
    let moving: Vec<i32> = frames.to_vec();
    let fixed: Vec<i32> = o.keys.iter().map(|k| k.frame).filter(|f| !moving.contains(f)).collect();
    let mut out = vec![];
    // Toutes ensemble ou aucune : sinon l'espacement entre elles changerait
    if moving.iter().any(|f| fixed.contains(&(f + d))) {
        return moving.iter().filter(|f| o.keys.iter().any(|k| k.frame == **f)).cloned().collect();
    }
    for k in &mut o.keys {
        if moving.contains(&k.frame) {
            k.frame += d;
            out.push(k.frame);
        }
    }
    o.keys.sort_by_key(|k| k.frame);
    out
}

/// Ajoute (`d` > 0) ou retire (`d` < 0) des images après `f` : les clés plus loin glissent. On ne retire pas
/// plus que l'écart jusqu'à la clé suivante. Rend le décalage vraiment appliqué.
pub fn shift_after(o: &mut Object, f: i32, d: i32) -> i32 {
    let d = if d < 0 {
        match o.keys.iter().find(|k| k.frame > f) {
            // La clé suivante peut venir jusqu'à f + 1
            Some(next) => d.max(-(next.frame - f - 1)),
            None => 0,
        }
    } else {
        d
    };
    for k in o.keys.iter_mut().filter(|k| k.frame > f) {
        k.frame += d;
    }
    d
}

/// Décale toute l'animation de l'objet
pub fn offset_all(o: &mut Object, d: i32) {
    for k in &mut o.keys {
        k.frame += d;
    }
}

/// Miroir gauche-droite (axe X) : la position X change de signe, les rotations autour de Y et de Z aussi
pub fn mirror(p: Pose) -> Pose {
    Pose { pos: [-p.pos[0], p.pos[1], p.pos[2]], rot: [p.rot[0], -p.rot[1], -p.rot[2]], scale: p.scale }
}

/// Pose de repos : sans rotation, à la taille 1 ; avec `position`, revient aussi à l'origine
pub fn rest(p: Pose, position: bool) -> Pose {
    Pose { pos: if position { [0.0; 3] } else { p.pos }, rot: [0.0; 3], scale: [1.0; 3] }
}

/// Partage les clés : chaque objet reçoit une clé (sa pose de l'instant) à toutes les images où l'un d'eux en a une
pub fn share_keys(objs: &mut [&mut Object]) -> usize {
    let mut frames: Vec<i32> = objs.iter().flat_map(|o| o.keys.iter().map(|k| k.frame)).collect();
    frames.sort();
    frames.dedup();
    let mut n = 0;
    for o in objs.iter_mut() {
        if o.keys.is_empty() {
            continue;
        }
        for &f in &frames {
            if o.keys.iter().any(|k| k.frame == f) {
                continue;
            }
            if let Some((pos, rot, scale)) = o.pose_at(f as f32) {
                let keep = Pose::of(o);
                put(o, f, Pose { pos, rot, scale });
                (o.pos, o.rot, o.scale) = (keep.pos, keep.rot, keep.scale);
                n += 1;
            }
        }
    }
    n
}

/// Retire les clés qui n'apportent rien : celles que les voisines redonnent déjà (à `tol` près sur chaque canal)
pub fn reduce(o: &mut Object, tol: f32) -> usize {
    let mut removed = 0;
    loop {
        let mut drop = None;
        for i in 1..o.keys.len().saturating_sub(1) {
            let mut without = o.clone();
            let k = without.keys.remove(i);
            if let Some((pos, rot, scale)) = without.pose_at(k.frame as f32) {
                let p = Pose { pos, rot, scale };
                if (0..9).all(|c| (p.get(c) - k.get(c)).abs() <= tol) {
                    drop = Some(i);
                    break;
                }
            }
        }
        match drop {
            Some(i) => {
                o.keys.remove(i);
                removed += 1;
            }
            None => return removed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(keys: &[(i32, f32)]) -> Object {
        let mut o = Object::new("o".into(), String::new());
        for &(f, x) in keys {
            o.pos = [x, 0.0, 0.0];
            o.set_key(f);
        }
        o
    }

    #[test]
    fn tweener_et_depassement() {
        let mut o = obj(&[(0, 0.0), (10, 10.0)]);
        assert!(tween(&mut o, 5, 0.25));
        assert_eq!(o.keys.iter().find(|k| k.frame == 5).unwrap().pos[0], 2.5);
        // Dépassement au-delà de la clé suivante
        assert!(tween(&mut o, 5, 1.2));
        assert!((o.keys[1].pos[0] - 12.0).abs() < 1e-4);
        assert!(!tween(&mut o, 20, 0.5), "pas de clé après");
    }

    #[test]
    fn decaler_inserer_retirer() {
        let mut o = obj(&[(0, 0.0), (5, 1.0), (10, 2.0)]);
        assert_eq!(nudge(&mut o, &[5], 2), vec![7]);
        assert_eq!(o.keys.iter().map(|k| k.frame).collect::<Vec<_>>(), vec![0, 7, 10]);
        // Elle ne saute pas sur une autre clé
        assert_eq!(nudge(&mut o, &[7], 3), vec![7]);
        assert_eq!(shift_after(&mut o, 0, 4), 4);
        assert_eq!(o.keys.iter().map(|k| k.frame).collect::<Vec<_>>(), vec![0, 11, 14]);
        // On ne retire pas plus que l'écart jusqu'à la clé suivante
        assert_eq!(shift_after(&mut o, 0, -20), -10);
        assert_eq!(o.keys[1].frame, 1);
    }

    #[test]
    fn miroir_repos_partage_reduction() {
        let p = Pose { pos: [2.0, 1.0, 0.0], rot: [10.0, 20.0, 30.0], scale: [2.0; 3] };
        assert_eq!(mirror(p), Pose { pos: [-2.0, 1.0, 0.0], rot: [10.0, -20.0, -30.0], scale: [2.0; 3] });
        assert_eq!(mirror(mirror(p)), p);
        assert_eq!(rest(p, false).pos, p.pos);
        let mut a = obj(&[(0, 0.0), (10, 10.0)]);
        let mut b = obj(&[(0, 0.0), (4, 3.0)]);
        assert_eq!(share_keys(&mut [&mut a, &mut b]), 2);
        assert_eq!(a.keys.iter().map(|k| k.frame).collect::<Vec<_>>(), vec![0, 4, 10]);
        // Une clé au milieu d'une ligne droite (linéaire) ne sert à rien
        let mut c = obj(&[(0, 0.0), (5, 5.0), (10, 10.0)]);
        for k in &mut c.keys {
            k.interp = "linear".into();
        }
        assert_eq!(reduce(&mut c, 1e-3), 1);
        assert_eq!(c.keys.len(), 2);
    }
}
