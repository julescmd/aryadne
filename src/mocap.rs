//! Capture de mouvement BVH : lecture du squelette et du mouvement, puis import dans une scène
//! (un os par articulation, une clé par image, ramenée à la cadence de la scène).

use crate::scene::{rot_euler, Key, Scene};

type M3 = [[f32; 3]; 3];
type V3 = [f32; 3];

#[derive(Clone, Debug, Default)]
pub struct Joint {
    pub name: String,
    pub parent: Option<usize>,
    pub offset: V3,
    /// Canaux dans l'ordre du fichier : 0..2 position X Y Z, 3..5 rotation X Y Z
    pub channels: Vec<u8>,
    /// Bout de la chaîne (End Site), s'il y en a un
    pub end: Option<V3>,
}

#[derive(Clone, Debug, Default)]
pub struct Bvh {
    pub joints: Vec<Joint>,
    pub frame_time: f32,
    pub frames: Vec<Vec<f32>>,
}

pub fn parse(text: &str) -> Result<Bvh, String> {
    let mut tok = text.split_whitespace().peekable();
    let mut b = Bvh::default();
    let mut stack: Vec<usize> = vec![];
    let num = |t: Option<&str>| -> Result<f32, String> { t.ok_or("fin de fichier")?.parse::<f32>().map_err(|e| e.to_string()) };
    let mut in_end = false;
    while let Some(t) = tok.next() {
        match t {
            "HIERARCHY" => {}
            "ROOT" | "JOINT" => {
                let name = tok.next().ok_or("nom d'articulation manquant")?.to_string();
                b.joints.push(Joint { name, parent: stack.last().copied(), ..Default::default() });
            }
            "End" => {
                tok.next();
                in_end = true;
            }
            "{" => {
                if !in_end {
                    stack.push(b.joints.len().checked_sub(1).ok_or("accolade sans articulation")?);
                }
            }
            "}" => {
                if in_end {
                    in_end = false;
                } else {
                    stack.pop();
                }
            }
            "OFFSET" => {
                let o = [num(tok.next())?, num(tok.next())?, num(tok.next())?];
                let j = if in_end { stack.last().copied() } else { b.joints.len().checked_sub(1) };
                let j = j.ok_or("OFFSET sans articulation")?;
                if in_end {
                    b.joints[j].end = Some(o);
                } else {
                    b.joints[j].offset = o;
                }
            }
            "CHANNELS" => {
                let n = num(tok.next())? as usize;
                let j = b.joints.len().checked_sub(1).ok_or("CHANNELS sans articulation")?;
                for _ in 0..n {
                    let c = match tok.next().unwrap_or("") {
                        "Xposition" => 0,
                        "Yposition" => 1,
                        "Zposition" => 2,
                        "Xrotation" => 3,
                        "Yrotation" => 4,
                        "Zrotation" => 5,
                        o => return Err(format!("canal inconnu : {o}")),
                    };
                    b.joints[j].channels.push(c);
                }
            }
            "MOTION" => {
                let mut nframes = 0usize;
                while let Some(&t) = tok.peek() {
                    match t {
                        "Frames:" => {
                            tok.next();
                            nframes = num(tok.next())? as usize;
                        }
                        "Frame" => {
                            tok.next();
                            tok.next();
                            b.frame_time = num(tok.next())?;
                            break;
                        }
                        _ => {
                            tok.next();
                        }
                    }
                }
                let per: usize = b.joints.iter().map(|j| j.channels.len()).sum();
                for _ in 0..nframes {
                    let mut row = Vec::with_capacity(per);
                    for _ in 0..per {
                        match tok.next() {
                            Some(v) => row.push(v.parse::<f32>().map_err(|e| e.to_string())?),
                            None => break,
                        }
                    }
                    if row.len() < per {
                        break;
                    }
                    b.frames.push(row);
                }
                break;
            }
            _ => {}
        }
    }
    if b.joints.is_empty() {
        return Err("aucune articulation".into());
    }
    if b.frame_time <= 0.0 {
        b.frame_time = 1.0 / 30.0;
    }
    Ok(b)
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
fn tr(a: M3) -> M3 {
    [[a[0][0], a[1][0], a[2][0]], [a[0][1], a[1][1], a[2][1]], [a[0][2], a[1][2], a[2][2]]]
}
fn apply(m: M3, v: V3) -> V3 {
    [m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2], m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2], m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2]]
}
fn axis(c: u8, deg: f32) -> M3 {
    let (s, co) = deg.to_radians().sin_cos();
    match c {
        3 => [[1.0, 0.0, 0.0], [0.0, co, -s], [0.0, s, co]],
        4 => [[co, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, co]],
        _ => [[co, -s, 0.0], [s, co, 0.0], [0.0, 0.0, 1.0]],
    }
}
const ID: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// La rotation qui amène +Y (l'axe d'un os) sur `d`
fn align_y(d: V3) -> M3 {
    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if l < 1e-6 {
        return ID;
    }
    let d = [d[0] / l, d[1] / l, d[2] / l];
    let c = d[1];
    if c > 0.9999 {
        return ID;
    }
    if c < -0.9999 {
        return axis(5, 180.0);
    }
    // Rodrigues autour de Y × d
    let a = [d[2], 0.0, -d[0]];
    let al = (a[0] * a[0] + a[2] * a[2]).sqrt();
    let a = [a[0] / al, 0.0, a[2] / al];
    let (s, k) = ((1.0 - c * c).sqrt(), 1.0 - c);
    [
        [c + a[0] * a[0] * k, -a[2] * s, a[0] * a[2] * k],
        [a[2] * s, c, -a[0] * s],
        [a[2] * a[0] * k, a[0] * s, c + a[2] * a[2] * k],
    ]
}

/// Importe le BVH dans la scène : un os par articulation (nommés comme dans le fichier, préfixés si besoin),
/// pointant vers leur premier enfant ; les clés reprennent le mouvement, à la cadence de la scène.
/// Rend l'indice de l'os racine.
pub fn import(scene: &mut Scene, b: &Bvh, prefix: &str) -> usize {
    let n = b.joints.len();
    // Direction de chaque os : vers son premier enfant, sinon son bout
    let dir: Vec<V3> = (0..n).map(|j| b.joints.iter().find(|c| c.parent == Some(j)).map(|c| c.offset).or(b.joints[j].end).unwrap_or([0.0, 1.0, 0.0])).collect();
    let len_of = |d: V3| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let align: Vec<M3> = dir.iter().map(|d| align_y(*d)).collect();
    // Échelle : le squelette au repos fait environ deux unités de haut
    let mut world = vec![[0.0f32; 3]; n];
    for j in 0..n {
        let o = b.joints[j].offset;
        world[j] = match b.joints[j].parent {
            Some(p) => [world[p][0] + o[0], world[p][1] + o[1], world[p][2] + o[2]],
            None => o,
        };
    }
    let (lo, hi) = world.iter().fold((f32::MAX, f32::MIN), |(lo, hi), w| (lo.min(w[1]), hi.max(w[1])));
    let scale = if hi - lo > 1e-4 { 2.0 / (hi - lo) } else { 1.0 };
    let sc = |v: V3| [v[0] * scale, v[1] * scale, v[2] * scale];
    // Les os
    let mut ids = vec![0usize; n];
    for j in 0..n {
        let name = format!("{prefix}{}", b.joints[j].name);
        let k = scene.add_role(&name, "bone");
        ids[j] = k;
        let pa = b.joints[j].parent.map(|p| align[p]).unwrap_or(ID);
        let o = &mut scene.objects[k];
        o.parent = String::new();
        o.length = (len_of(dir[j]) * scale).max(0.02);
        // Repos : pos dans le repère de l'os parent, rotation = alignement relatif
        o.pos = sc(apply(tr(pa), b.joints[j].offset));
        o.rot = rot_euler(mul(tr(pa), align[j]));
    }
    // Les noms réels (add_role peut avoir ajouté un suffixe) : on refait les liens de parenté
    for j in 0..n {
        if let Some(p) = b.joints[j].parent {
            scene.objects[ids[j]].parent = scene.objects[ids[p]].name.clone();
        }
    }
    // Le mouvement : une clé par image de la scène
    let fps = scene.fps.max(1.0);
    let dur = b.frames.len() as f32 * b.frame_time;
    let last = (dur * fps).floor() as i32;
    for f in 0..=last.max(0) {
        let fi = ((f as f32 / fps) / b.frame_time).round() as usize;
        let Some(row) = b.frames.get(fi.min(b.frames.len().saturating_sub(1))) else { break };
        let mut at = 0usize;
        for j in 0..n {
            let mut r = ID;
            let mut t = [0.0f32; 3];
            for &c in &b.joints[j].channels {
                let v = row.get(at).copied().unwrap_or(0.0);
                at += 1;
                if c < 3 {
                    t[c as usize] = v;
                } else {
                    r = mul(r, axis(c, v));
                }
            }
            let pa = b.joints[j].parent.map(|p| align[p]).unwrap_or(ID);
            let off = b.joints[j].offset;
            // Les racines ont une position animée (canaux de position) ; les autres gardent leur décalage
            let has_pos = b.joints[j].channels.iter().any(|c| *c < 3);
            let p = if has_pos && b.joints[j].parent.is_none() { t } else if has_pos { [off[0] + t[0], off[1] + t[1], off[2] + t[2]] } else { off };
            let pos = sc(apply(tr(pa), p));
            let rot = rot_euler(mul(mul(tr(pa), r), align[j]));
            let o = &mut scene.objects[ids[j]];
            o.keys.retain(|k| k.frame != f);
            o.keys.push(Key { frame: f, pos, rot, scale: [1.0; 3], ease: false, interp: "linear".into(), handle: 0, handles: vec![] });
        }
    }
    scene.end = scene.end.max(last);
    for &k in &ids {
        scene.objects[k].keys.sort_by_key(|k| k.frame);
        // La pose de la première image
        if let Some(k0) = scene.objects[k].keys.first().cloned() {
            scene.objects[k].pos = k0.pos;
            scene.objects[k].rot = k0.rot;
        }
    }
    ids[0]
}

/// Nom d'articulation ramené à l'essentiel : minuscules, sans préfixe « mixamorig: », sans séparateurs
pub fn norm_name(n: &str) -> String {
    let n = n.to_lowercase();
    let n = n.rsplit([':', '|']).next().unwrap_or(&n).to_string();
    let n = n.strip_prefix("mixamorig").unwrap_or(&n).to_string();
    n.chars().filter(|c| c.is_alphanumeric()).collect()
}

fn rot_only(m: M3) -> M3 {
    let t = tr(m);
    let nz = |v: V3| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
        [v[0] / l, v[1] / l, v[2] / l]
    };
    tr([nz(t[0]), nz(t[1]), nz(t[2])])
}

/// Transfère le mouvement sur un squelette existant (reciblage) : chaque os dont le nom correspond à une
/// articulation (Hips, Spine, LeftArm… ; préfixes Mixamo ignorés) suit sa rotation dans le monde, à partir de
/// sa propre pose de repos. Le BVH et le squelette doivent partir de la même pose (T-pose ou A-pose).
/// Rend le nombre d'os animés.
pub fn retarget(scene: &mut Scene, b: &Bvh, root: usize) -> usize {
    let rest = crate::rig::rest_pose(scene);
    let mut bones = crate::rig::bones_of(scene, root);
    let depth = |k: usize| {
        let mut d = 0;
        let mut x = k;
        while let Some(p) = scene.parent_of(x) {
            d += 1;
            x = p;
            if d > 64 {
                break;
            }
        }
        d
    };
    bones.sort_by_key(|&k| depth(k));
    let names: Vec<String> = b.joints.iter().map(|j| norm_name(&j.name)).collect();
    let map: Vec<Option<usize>> = bones.iter().map(|&k| names.iter().position(|n| *n == norm_name(&scene.objects[k].name))).collect();
    let n_mapped = map.iter().filter(|m| m.is_some()).count();
    if n_mapped == 0 {
        return 0;
    }
    // Repos : rotation dans le monde et rotation locale de chaque os ; rotation du parent de la racine
    let e0: Vec<M3> = bones.iter().map(|&k| rot_only(crate::rig::affine(&rest, k).0)).collect();
    let l0: Vec<M3> = bones.iter().map(|&k| crate::scene::rot_matrix(rest.objects[k].rot)).collect();
    let top = scene.parent_of(root).map(|p| rot_only(crate::rig::affine(&rest, p).0)).unwrap_or(ID);
    // Hauteur des hanches : BVH et squelette, pour ramener le déplacement de la racine à la bonne taille
    let root_j = map.iter().zip(&bones).find(|(_, k)| **k == root).and_then(|(m, _)| *m);
    let bvh_h = b.joints.first().map(|j| j.offset[1].abs()).unwrap_or(1.0).max(1e-3);
    let our_h = crate::rig::affine(&rest, root).1[1].abs().max(1e-3);
    let fps = scene.fps.max(1.0);
    let last = ((b.frames.len() as f32 * b.frame_time) * fps).floor() as i32;
    let mut first_t: Option<V3> = None;
    for f in 0..=last.max(0) {
        let fi = (((f as f32 / fps) / b.frame_time).round() as usize).min(b.frames.len().saturating_sub(1));
        let Some(row) = b.frames.get(fi) else { break };
        // Rotations des articulations dans le monde (le BVH est au repos quand tout vaut zéro)
        let mut w: Vec<M3> = vec![ID; b.joints.len()];
        let mut root_t = [0.0f32; 3];
        let mut at = 0usize;
        for (j, jt) in b.joints.iter().enumerate() {
            let mut r = ID;
            for &c in &jt.channels {
                let v = row.get(at).copied().unwrap_or(0.0);
                at += 1;
                if c < 3 {
                    if jt.parent.is_none() {
                        root_t[c as usize] = v;
                    }
                } else {
                    r = mul(r, axis(c, v));
                }
            }
            w[j] = match jt.parent {
                Some(p) => mul(w[p], r),
                None => r,
            };
        }
        let t0 = *first_t.get_or_insert(root_t);
        let mut world: Vec<M3> = vec![ID; bones.len()];
        for (i, &k) in bones.iter().enumerate() {
            let pw = scene.parent_of(k).and_then(|p| bones.iter().position(|x| *x == p)).map(|pi| world[pi]).unwrap_or(top);
            let e = match map[i] {
                Some(j) => mul(w[j], e0[i]),
                None => mul(pw, l0[i]),
            };
            world[i] = e;
            let local = mul(tr(pw), e);
            let mut pos = rest.objects[k].pos;
            if k == root && root_j.is_some() {
                // Déplacement de la racine, à l'échelle du squelette, dans le repère de son parent
                let s = our_h / bvh_h;
                let d = apply(tr(top), [(root_t[0] - t0[0]) * s, (root_t[1] - t0[1]) * s, (root_t[2] - t0[2]) * s]);
                pos = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
            }
            let o = &mut scene.objects[k];
            o.keys.retain(|x| x.frame != f);
            o.keys.push(Key { frame: f, pos, rot: rot_euler(local), scale: rest.objects[k].scale, ease: false, interp: "linear".into(), handle: 0, handles: vec![] });
        }
    }
    for &k in &bones {
        scene.objects[k].keys.sort_by_key(|x| x.frame);
    }
    scene.end = scene.end.max(last);
    scene.apply_frame(0.0);
    n_mapped
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "HIERARCHY
ROOT Hips
{
  OFFSET 0 90 0
  CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation
  JOINT Spine
  {
    OFFSET 0 10 0
    CHANNELS 3 Zrotation Xrotation Yrotation
    JOINT Arm
    {
      OFFSET 0 0 20
      CHANNELS 3 Zrotation Xrotation Yrotation
      End Site
      {
        OFFSET 0 0 15
      }
    }
  }
}
MOTION
Frames: 3
Frame Time: 0.0333333
0 90 0 0 0 0 0 0 0 0 0 0
0 90 0 0 0 0 0 0 0 0 0 90
5 90 0 0 0 0 0 0 0 0 0 90
";

    #[test]
    fn lecture_et_import() {
        let b = parse(SAMPLE).unwrap();
        assert_eq!(b.joints.len(), 3);
        assert_eq!(b.joints[2].parent, Some(1));
        assert_eq!(b.joints[2].end, Some([0.0, 0.0, 15.0]));
        assert_eq!(b.frames.len(), 3);
        let mut sc = Scene::default();
        sc.fps = 30.0;
        let root = import(&mut sc, &b, "");
        assert_eq!(sc.objects[root].name, "Hips");
        assert_eq!(sc.objects[2].parent, "Spine");
        // Au repos (image 0), le bout du bras pointe vers +Z, à sa place dans le monde
        sc.apply_frame(0.0);
        let arm = 2;
        let base = sc.world(arm, [0.0; 3]);
        let tip = sc.world(arm, [0.0, sc.objects[arm].length, 0.0]);
        let d = [tip[0] - base[0], tip[1] - base[1], tip[2] - base[2]];
        assert!(d[2] > 0.9 * (d[0].abs() + d[1].abs() + d[2].abs()), "le bras pointe vers +Z : {d:?}");
        // Image 1 : le bras tourne de 90° autour de Y (rotation BVH du bras) : il pointe vers +X
        sc.apply_frame(1.0);
        let base = sc.world(arm, [0.0; 3]);
        let tip = sc.world(arm, [0.0, sc.objects[arm].length, 0.0]);
        let d = [tip[0] - base[0], tip[1] - base[1], tip[2] - base[2]];
        assert!(d[0] > 0.9 * (d[0].abs() + d[1].abs() + d[2].abs()), "le bras pointe vers +X : {d:?}");
    }

    #[test]
    fn reciblage_par_nom() {
        assert_eq!(norm_name("mixamorig:LeftArm"), "leftarm");
        assert_eq!(norm_name("Left_Arm"), "leftarm");
        let b = parse(SAMPLE).unwrap();
        // Un squelette maison : mêmes noms, autres longueurs, bras le long de +Z au repos
        let mut sc = Scene::default();
        sc.fps = 30.0;
        let hips = crate::rig::add_bone(&mut sc, None);
        sc.rename(hips, "Hips");
        let spine = crate::rig::add_bone(&mut sc, Some(hips));
        sc.rename(spine, "Spine");
        let arm = crate::rig::add_bone(&mut sc, Some(spine));
        sc.rename(arm, "Arm");
        sc.objects[arm].rot = [90.0, 0.0, 0.0];
        crate::rig::set_rest(&mut sc, hips);
        let dir = |sc: &Scene| {
            let a = sc.world(arm, [0.0; 3]);
            let t = sc.world(arm, [0.0, 1.0, 0.0]);
            [t[0] - a[0], t[1] - a[1], t[2] - a[2]]
        };
        let rest_dir = dir(&sc);
        assert_eq!(retarget(&mut sc, &b, hips), 3);
        sc.apply_frame(0.0);
        let d0 = dir(&sc);
        assert!((d0[0] - rest_dir[0]).abs() < 1e-3 && (d0[2] - rest_dir[2]).abs() < 1e-3, "{d0:?} {rest_dir:?}");
        // Image 1 : 90° autour de Y dans le monde : +Z devient +X
        sc.apply_frame(1.0);
        let d1 = dir(&sc);
        assert!(d1[0] > 0.9 && d1[2].abs() < 0.1, "{d1:?}");
    }
}
