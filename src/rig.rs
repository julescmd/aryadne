//! Squelettes : os (des objets de la scène au rôle « bone »), liaison d'un modèle avec poids automatiques,
//! déformation par les os (skinning linéaire) et cinématique inverse (CCD).

use crate::scene::{rot_euler, rot_matrix, Kind, Scene};

type M3 = [[f32; 3]; 3];
type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}
fn norm(a: V3) -> V3 {
    let l = len(a).max(1e-12);
    [a[0] / l, a[1] / l, a[2] / l]
}
fn mul(a: M3, b: M3) -> M3 {
    let mut m = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    m
}
fn apply(m: M3, v: V3) -> V3 {
    [dot(m[0], v), dot(m[1], v), dot(m[2], v)]
}
fn transpose(m: M3) -> M3 {
    [[m[0][0], m[1][0], m[2][0]], [m[0][1], m[1][1], m[2][1]], [m[0][2], m[1][2], m[2][2]]]
}
fn inverse(m: M3) -> Option<M3> {
    let d = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if d.abs() < 1e-12 {
        return None;
    }
    let c = |a: usize, b: usize, e: usize, f: usize| m[a][b] * m[e][f] - m[a][f] * m[e][b];
    Some([
        [c(1, 1, 2, 2) / d, -c(0, 1, 2, 2) / d, c(0, 1, 1, 2) / d],
        [-c(1, 0, 2, 2) / d, c(0, 0, 2, 2) / d, -c(0, 0, 1, 2) / d],
        [c(1, 0, 2, 1) / d, -c(0, 0, 2, 1) / d, c(0, 0, 1, 1) / d],
    ])
}
/// Rotation d'un angle (radians) autour d'un axe unitaire.
fn axis_angle(a: V3, t: f32) -> M3 {
    let (s, c) = t.sin_cos();
    let k = 1.0 - c;
    [
        [c + a[0] * a[0] * k, a[0] * a[1] * k - a[2] * s, a[0] * a[2] * k + a[1] * s],
        [a[1] * a[0] * k + a[2] * s, c + a[1] * a[1] * k, a[1] * a[2] * k - a[0] * s],
        [a[2] * a[0] * k - a[1] * s, a[2] * a[1] * k + a[0] * s, c + a[2] * a[2] * k],
    ]
}

/// Repère d'un objet dans la scène : partie linéaire (colonnes = ses axes) et origine.
pub fn affine(scene: &Scene, k: usize) -> (M3, V3) {
    let o = scene.world(k, [0.0; 3]);
    let x = sub(scene.world(k, [1.0, 0.0, 0.0]), o);
    let y = sub(scene.world(k, [0.0, 1.0, 0.0]), o);
    let z = sub(scene.world(k, [0.0, 0.0, 1.0]), o);
    ([[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]], o)
}

/// Rotation pure d'un repère (taille retirée de chaque axe).
fn rotation_part(m: M3) -> M3 {
    let t = transpose(m);
    let cols = [norm(t[0]), norm(t[1]), norm(t[2])];
    transpose(cols)
}

pub fn is_bone(scene: &Scene, k: usize) -> bool {
    scene.objects.get(k).is_some_and(|o| crate::scene::kind(o) == Kind::Bone)
}

/// Les os d'un squelette : la racine (si c'est un os) et tous les os qui en descendent.
pub fn bones_of(scene: &Scene, root: usize) -> Vec<usize> {
    (0..scene.objects.len()).filter(|&k| is_bone(scene, k) && (k == root || scene.descends(k, root))).collect()
}

/// L'os racine d'un os : en remontant tant que le parent est un os.
pub fn root_of(scene: &Scene, mut k: usize) -> usize {
    for _ in 0..64 {
        match scene.parent_of(k) {
            Some(up) if is_bone(scene, up) => k = up,
            _ => break,
        }
    }
    k
}

/// La scène avec chaque os dans sa pose de repos.
fn at_rest(scene: &Scene) -> Scene {
    let mut s = scene.clone();
    for o in &mut s.objects {
        if let (Some(r), true) = (o.rest, o.role == "bone") {
            (o.pos, o.rot, o.scale) = (r[0], r[1], r[2]);
        }
    }
    s
}

/// Enregistre la pose actuelle comme pose de repos de tout le squelette.
pub fn set_rest(scene: &mut Scene, root: usize) {
    for b in bones_of(scene, root) {
        let o = &mut scene.objects[b];
        o.rest = Some([o.pos, o.rot, o.scale]);
    }
}

/// Lie un modèle à un squelette : la pose actuelle devient la pose de repos.
pub fn bind(scene: &mut Scene, model: usize, root: usize) {
    let root = root_of(scene, root);
    set_rest(scene, root);
    scene.objects[model].skin = scene.objects[root].name.clone();
}

/// Ramène le squelette à sa pose de repos.
pub fn to_rest(scene: &mut Scene, root: usize) {
    for b in bones_of(scene, root) {
        let o = &mut scene.objects[b];
        if let Some(r) = o.rest {
            (o.pos, o.rot, o.scale) = (r[0], r[1], r[2]);
        }
    }
}

/// Distance d'un point à un segment.
fn seg_dist(p: V3, a: V3, b: V3) -> f32 {
    let ab = sub(b, a);
    let t = (dot(sub(p, a), ab) / dot(ab, ab).max(1e-12)).clamp(0.0, 1.0);
    len(sub(p, [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t]))
}

/// Un os prêt pour la déformation : segment au repos et passage repos → pose.
struct Prepared {
    head: V3,
    tail: V3,
    size: f32,
    m: M3,
    t: V3,
}

fn prepare(scene: &Scene, root: usize) -> Vec<Prepared> {
    let rest = at_rest(scene);
    bones_of(scene, root)
        .into_iter()
        .filter_map(|b| {
            let l = scene.objects[b].length.max(1e-4);
            let (rm, ro) = affine(&rest, b);
            let (cm, co) = affine(scene, b);
            let inv = inverse(rm)?;
            let m = mul(cm, inv);
            // v' = C (R⁻¹ (v - ro)) + co = M v + (co - M ro)
            let mr = apply(m, ro);
            let head = ro;
            let tail = rest.world(b, [0.0, l, 0.0]);
            Some(Prepared { head, tail, size: len(sub(tail, head)).max(1e-4), m, t: sub(co, mr) })
        })
        .collect()
}

/// Poids automatiques : les quatre os les plus proches, l'influence tombant avec la distance.
fn weights(bones: &[Prepared], v: V3, reach: f32) -> Vec<(usize, f32)> {
    let mut w: Vec<(usize, f32)> = bones
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let d = seg_dist(v, b.head, b.tail) / (b.size * 0.25 * reach.max(0.05));
            (i, 1.0 / (1e-4 + d * d * d * d))
        })
        .collect();
    w.sort_by(|a, b| b.1.total_cmp(&a.1));
    w.truncate(4);
    let sum: f32 = w.iter().map(|x| x.1).sum::<f32>().max(1e-12);
    for x in &mut w {
        x.1 /= sum;
    }
    w
}

/// Déforme les sommets (déjà placés dans la scène) d'un modèle lié à un squelette.
pub fn deform(scene: &Scene, model: usize, pos: &mut [V3]) {
    let o = &scene.objects[model];
    if o.skin.is_empty() {
        return;
    }
    let Some(root) = scene.objects.iter().position(|x| x.name == o.skin && x.role == "bone") else { return };
    let bones = prepare(scene, root);
    if bones.is_empty() {
        return;
    }
    // Rien n'a bougé depuis le repos : on garde les sommets tels quels
    let still = bones.iter().all(|b| (0..3).all(|i| (0..3).all(|j| (b.m[i][j] - if i == j { 1.0 } else { 0.0 }).abs() < 1e-5)) && len(b.t) < 1e-5);
    if still {
        return;
    }
    let reach = o.reach;
    for v in pos.iter_mut() {
        let mut out = [0.0; 3];
        for (i, w) in weights(&bones, *v, reach) {
            let b = &bones[i];
            let q = apply(b.m, *v);
            for a in 0..3 {
                out[a] += (q[a] + b.t[a]) * w;
            }
        }
        *v = out;
    }
}

/// Le poids de l'os `bone` sur chaque sommet.
#[cfg(test)]
pub fn bone_weights(scene: &Scene, model: usize, bone: usize, pos: &[V3]) -> Vec<f32> {
    let o = &scene.objects[model];
    let Some(root) = scene.objects.iter().position(|x| x.name == o.skin && x.role == "bone") else { return vec![0.0; pos.len()] };
    let list = bones_of(scene, root);
    let Some(idx) = list.iter().position(|&b| b == bone) else { return vec![0.0; pos.len()] };
    let bones = prepare(scene, root);
    if bones.len() != list.len() {
        return vec![0.0; pos.len()];
    }
    pos.iter().map(|v| weights(&bones, *v, o.reach).iter().find(|x| x.0 == idx).map(|x| x.1).unwrap_or(0.0)).collect()
}

/// Cinématique inverse : chaque os qui a une cible tourne sa chaîne pour que son bout l'atteigne.
pub fn solve_ik(scene: &mut Scene) -> bool {
    let mut moved = false;
    for k in 0..scene.objects.len() {
        if scene.objects[k].ik.is_empty() || !is_bone(scene, k) {
            continue;
        }
        let Some(target) = scene.objects.iter().position(|o| o.name == scene.objects[k].ik).filter(|&t| t != k && !scene.descends(t, k)) else { continue };
        let mut chain = vec![k];
        while chain.len() < scene.objects[k].ik_chain.clamp(1, 16) as usize {
            match scene.parent_of(*chain.last().unwrap()) {
                Some(up) if is_bone(scene, up) => chain.push(up),
                _ => break,
            }
        }
        let goal = scene.world(target, [0.0; 3]);
        for _ in 0..12 {
            let tip = scene.world(k, [0.0, scene.objects[k].length, 0.0]);
            if len(sub(tip, goal)) < 1e-4 {
                break;
            }
            for &j in &chain {
                let tip = scene.world(k, [0.0, scene.objects[k].length, 0.0]);
                let joint = scene.world(j, [0.0; 3]);
                let (e, t) = (sub(tip, joint), sub(goal, joint));
                if len(e) < 1e-6 || len(t) < 1e-6 {
                    continue;
                }
                let (e, t) = (norm(e), norm(t));
                let c = dot(e, t).clamp(-1.0, 1.0);
                let axis = cross(e, t);
                if len(axis) < 1e-7 || c > 0.999_999 {
                    continue;
                }
                let q = axis_angle(norm(axis), c.acos());
                // Dans le repère du parent : R' = Pᵀ Q P R
                let p = scene.parent_of(j).map(|up| rotation_part(affine(scene, up).0)).unwrap_or([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
                let r = rot_matrix(scene.objects[j].rot);
                let nr = mul(transpose(p), mul(q, mul(p, r)));
                let e = rot_euler(nr);
                if (0..3).any(|i| (e[i] - scene.objects[j].rot[i]).abs() > 1e-4) {
                    scene.objects[j].rot = e;
                    moved = true;
                }
            }
        }
    }
    moved
}

/// Ajoute un os au bout de `parent` (ou à l'origine) et renvoie son indice.
pub fn add_bone(scene: &mut Scene, parent: Option<usize>) -> usize {
    let k = scene.add_role("os", "bone");
    if let Some(p) = parent.filter(|&p| is_bone(scene, p)) {
        let (name, l) = (scene.objects[p].name.clone(), scene.objects[p].length);
        let o = &mut scene.objects[k];
        o.parent = name;
        o.pos = [0.0, l, 0.0];
        o.length = l;
    }
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squelette() {
        let mut sc = Scene::default();
        let a = add_bone(&mut sc, None);
        let b = add_bone(&mut sc, Some(a));
        let m = sc.add_primitive("@cube");
        bind(&mut sc, m, b);
        assert_eq!(sc.objects[m].skin, sc.objects[a].name);
        // Au repos : rien ne bouge
        let mut pts = vec![[0.0, 0.2, 0.0], [0.0, 1.8, 0.0]];
        deform(&sc, m, &mut pts);
        assert!((pts[1][1] - 1.8).abs() < 1e-4);
        // On plie le second os de 90° autour de Z : le point du haut part sur le côté, celui du bas reste
        sc.objects[b].rot = [0.0, 0.0, 90.0];
        let mut pts = vec![[0.0, 0.2, 0.0], [0.0, 1.8, 0.0]];
        deform(&sc, m, &mut pts);
        assert!((pts[0][1] - 0.2).abs() < 0.05, "{:?}", pts[0]);
        assert!(pts[1][0] < -0.6 && (pts[1][1] - 1.0).abs() < 0.1, "{:?}", pts[1]);
        // Poids : le point du haut appartient surtout au second os
        let w = bone_weights(&sc, m, b, &[[0.0, 1.8, 0.0]]);
        assert!(w[0] > 0.9);
        // Cinématique inverse : le bout de la chaîne rejoint la cible
        sc.objects[b].rot = [0.0; 3];
        let t = sc.add_role("cible", "");
        sc.objects[t].pos = [1.2, 1.2, 0.0];
        sc.objects[b].ik = sc.objects[t].name.clone();
        assert!(solve_ik(&mut sc));
        let tip = sc.world(b, [0.0, 1.0, 0.0]);
        assert!(len(sub(tip, [1.2, 1.2, 0.0])) < 0.02, "{tip:?}");
        // Retour au repos
        to_rest(&mut sc, a);
        assert_eq!(sc.objects[b].rot, [0.0; 3]);
    }
}
