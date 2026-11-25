//! Export d'une scène en glTF binaire (.glb) avec ses squelettes : les os deviennent des
//! articulations, les modèles liés sont « skinnés » (quatre os par sommet) et l'animation
//! des os est échantillonnée image par image. Blender, Unity, Unreal et Godot le lisent.

use crate::scene::Scene;
use std::path::Path;

type V3 = [f32; 3];
/// Matrice 4 × 4, en colonnes (comme glTF)
type M4 = [f32; 16];

fn m4_of(scene: &Scene, k: usize) -> M4 {
    let (m, o) = crate::rig::affine(scene, k);
    // m : lignes ; colonnes = axes
    [m[0][0], m[1][0], m[2][0], 0.0, m[0][1], m[1][1], m[2][1], 0.0, m[0][2], m[1][2], m[2][2], 0.0, o[0], o[1], o[2], 1.0]
}

fn mul4(a: &M4, b: &M4) -> M4 {
    let mut r = [0.0; 16];
    for c in 0..4 {
        for row in 0..4 {
            r[c * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[c * 4 + k]).sum();
        }
    }
    r
}

/// Inverse d'une transformation affine (partie linéaire inversible)
fn inv4(m: &M4) -> M4 {
    let a = [[m[0], m[4], m[8]], [m[1], m[5], m[9]], [m[2], m[6], m[10]]];
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0]) + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    let d = if det.abs() < 1e-12 { 1e-12 } else { det };
    let i = [
        [(a[1][1] * a[2][2] - a[1][2] * a[2][1]) / d, (a[0][2] * a[2][1] - a[0][1] * a[2][2]) / d, (a[0][1] * a[1][2] - a[0][2] * a[1][1]) / d],
        [(a[1][2] * a[2][0] - a[1][0] * a[2][2]) / d, (a[0][0] * a[2][2] - a[0][2] * a[2][0]) / d, (a[0][2] * a[1][0] - a[0][0] * a[1][2]) / d],
        [(a[1][0] * a[2][1] - a[1][1] * a[2][0]) / d, (a[0][1] * a[2][0] - a[0][0] * a[2][1]) / d, (a[0][0] * a[1][1] - a[0][1] * a[1][0]) / d],
    ];
    let t = [m[12], m[13], m[14]];
    let it = [-(i[0][0] * t[0] + i[0][1] * t[1] + i[0][2] * t[2]), -(i[1][0] * t[0] + i[1][1] * t[1] + i[1][2] * t[2]), -(i[2][0] * t[0] + i[2][1] * t[1] + i[2][2] * t[2])];
    [i[0][0], i[1][0], i[2][0], 0.0, i[0][1], i[1][1], i[2][1], 0.0, i[0][2], i[1][2], i[2][2], 0.0, it[0], it[1], it[2], 1.0]
}

pub fn mul4_pub(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    mul4(a, b)
}

pub fn inv4_pub(m: &[f32; 16]) -> [f32; 16] {
    inv4(m)
}

/// Translation, rotation (quaternion x y z w) et taille d'une matrice sans cisaillement
pub fn decompose(m: &M4) -> (V3, [f32; 4], V3) {
    let t = [m[12], m[13], m[14]];
    let col = |c: usize| [m[c * 4], m[c * 4 + 1], m[c * 4 + 2]];
    let len = |v: V3| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
    let (c0, c1, c2) = (col(0), col(1), col(2));
    let s = [len(c0), len(c1), len(c2)];
    let r = [[c0[0] / s[0], c1[0] / s[1], c2[0] / s[2]], [c0[1] / s[0], c1[1] / s[1], c2[1] / s[2]], [c0[2] / s[0], c1[2] / s[1], c2[2] / s[2]]];
    let tr = r[0][0] + r[1][1] + r[2][2];
    let q = if tr > 0.0 {
        let w = (tr + 1.0).sqrt() * 2.0;
        [(r[2][1] - r[1][2]) / w, (r[0][2] - r[2][0]) / w, (r[1][0] - r[0][1]) / w, w / 4.0]
    } else if r[0][0] > r[1][1] && r[0][0] > r[2][2] {
        let w = (1.0 + r[0][0] - r[1][1] - r[2][2]).sqrt() * 2.0;
        [w / 4.0, (r[0][1] + r[1][0]) / w, (r[0][2] + r[2][0]) / w, (r[2][1] - r[1][2]) / w]
    } else if r[1][1] > r[2][2] {
        let w = (1.0 + r[1][1] - r[0][0] - r[2][2]).sqrt() * 2.0;
        [(r[0][1] + r[1][0]) / w, w / 4.0, (r[1][2] + r[2][1]) / w, (r[0][2] - r[2][0]) / w]
    } else {
        let w = (1.0 + r[2][2] - r[0][0] - r[1][1]).sqrt() * 2.0;
        [(r[0][2] + r[2][0]) / w, (r[1][2] + r[2][1]) / w, w / 4.0, (r[1][0] - r[0][1]) / w]
    };
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt().max(1e-9);
    (t, [q[0] / n, q[1] / n, q[2] / n, q[3] / n], s)
}

/// Transformation locale de l'os `b` (par rapport à son parent s'il est un os)
fn local(scene: &Scene, b: usize, bone_parent: Option<usize>) -> M4 {
    let w = m4_of(scene, b);
    match bone_parent {
        Some(p) => mul4(&inv4(&m4_of(scene, p)), &w),
        None => w,
    }
}

/// Le tampon binaire et ses accesseurs
struct Bin {
    data: Vec<u8>,
    views: Vec<serde_json::Value>,
    accessors: Vec<serde_json::Value>,
}

impl Bin {
    fn push(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        while self.data.len() % 4 != 0 {
            self.data.push(0);
        }
        let off = self.data.len();
        self.data.extend_from_slice(bytes);
        let mut v = serde_json::json!({ "buffer": 0, "byteOffset": off, "byteLength": bytes.len() });
        if let Some(t) = target {
            v["target"] = t.into();
        }
        self.views.push(v);
        self.views.len() - 1
    }

    fn floats(&mut self, v: &[f32], ty: &str, count: usize, target: Option<u32>, minmax: bool) -> usize {
        let bytes: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
        let view = self.push(&bytes, target);
        let mut a = serde_json::json!({ "bufferView": view, "componentType": 5126, "count": count, "type": ty });
        if minmax && count > 0 {
            let n = v.len() / count;
            let lo: Vec<f32> = (0..n).map(|k| v.iter().skip(k).step_by(n).fold(f32::MAX, |a, b| a.min(*b))).collect();
            let hi: Vec<f32> = (0..n).map(|k| v.iter().skip(k).step_by(n).fold(f32::MIN, |a, b| a.max(*b))).collect();
            a["min"] = serde_json::json!(lo);
            a["max"] = serde_json::json!(hi);
        }
        self.accessors.push(a);
        self.accessors.len() - 1
    }
}

/// Exporte la scène : modèles (dans leur pose de repos), squelettes, poids et animation des os
pub fn export(scene: &Scene, scene_path: &Path, out: &Path) -> Result<(usize, usize), String> {
    let rest = crate::rig::rest_pose(scene);
    let (model, _, ranges) = crate::scene::build_ranges(&rest, scene_path);
    let model = model?;
    let mut bin = Bin { data: vec![], views: vec![], accessors: vec![] };
    let mut nodes: Vec<serde_json::Value> = vec![];
    let mut meshes = vec![];
    let mut materials = vec![];
    let mut skins = vec![];
    let mut roots: Vec<usize> = vec![];

    // Les os : un nœud par os, rangés sous leur os parent
    let bones: Vec<usize> = (0..scene.objects.len()).filter(|&k| crate::rig::is_bone(scene, k)).collect();
    let bone_parent = |k: usize| scene.parent_of(k).filter(|p| crate::rig::is_bone(scene, *p));
    let node_of: std::collections::HashMap<usize, usize> = bones.iter().enumerate().map(|(i, b)| (*b, i)).collect();
    for &b in &bones {
        let (t, r, s) = decompose(&local(&rest, b, bone_parent(b)));
        let kids: Vec<usize> = bones.iter().filter(|c| bone_parent(**c) == Some(b)).map(|c| node_of[c]).collect();
        let mut n = serde_json::json!({ "name": scene.objects[b].name, "translation": t, "rotation": r, "scale": s });
        if !kids.is_empty() {
            n["children"] = serde_json::json!(kids);
        }
        nodes.push(n);
        if bone_parent(b).is_none() {
            roots.push(node_of[&b]);
        }
    }

    // Les modèles : sommets à leur place dans la scène au repos ; liés à un squelette, avec leurs poids
    let mut skinned = 0;
    for (k, t0, t1) in ranges {
        let o = &scene.objects[k];
        let mut remap = std::collections::HashMap::new();
        let mut pos: Vec<V3> = vec![];
        let mut idx: Vec<u32> = vec![];
        for t in &model.tris[t0..t1] {
            for &v in t {
                let n = *remap.entry(v).or_insert_with(|| {
                    pos.push(model.positions[v as usize]);
                    pos.len() as u32 - 1
                });
                idx.push(n);
            }
        }
        if pos.is_empty() {
            continue;
        }
        // Normales lisses
        let mut nrm = vec![[0.0f32; 3]; pos.len()];
        for t in idx.chunks(3) {
            let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
            let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
            let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            for &i in t {
                for a in 0..3 {
                    nrm[i as usize][a] += n[a];
                }
            }
        }
        for n in &mut nrm {
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            *n = if l > 1e-12 { [n[0] / l, n[1] / l, n[2] / l] } else { [0.0, 1.0, 0.0] };
        }
        let flat: Vec<f32> = pos.iter().flatten().copied().collect();
        let pa = bin.floats(&flat, "VEC3", pos.len(), Some(34962), true);
        let nflat: Vec<f32> = nrm.iter().flatten().copied().collect();
        let na = bin.floats(&nflat, "VEC3", nrm.len(), Some(34962), false);
        let ibytes: Vec<u8> = idx.iter().flat_map(|i| i.to_le_bytes()).collect();
        let iv = bin.push(&ibytes, Some(34963));
        bin.accessors.push(serde_json::json!({ "bufferView": iv, "componentType": 5125, "count": idx.len(), "type": "SCALAR" }));
        let ia = bin.accessors.len() - 1;
        // Couleur : celle de l'objet, sinon la moyenne de ses faces
        let col = match o.color {
            Some(c) => [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0],
            None => {
                let cs = &model.colors[t0..t1.min(model.colors.len())];
                let n = cs.len().max(1) as f32;
                let s = cs.iter().fold([0.0f32; 3], |a, c| [a[0] + c.r() as f32, a[1] + c.g() as f32, a[2] + c.b() as f32]);
                [s[0] / n / 255.0, s[1] / n / 255.0, s[2] / n / 255.0]
            }
        };
        materials.push(serde_json::json!({ "name": o.name, "pbrMetallicRoughness": { "baseColorFactor": [col[0], col[1], col[2], 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.6 } }));
        let mut attrs = serde_json::json!({ "POSITION": pa, "NORMAL": na });
        let mut skin_ix = None;
        if let Some((joints, w)) = crate::rig::skin_weights(&rest, k, &pos) {
            let js: Vec<usize> = joints.iter().filter_map(|b| node_of.get(b).copied()).collect();
            if js.len() == joints.len() && !js.is_empty() {
                let mut jb: Vec<u8> = vec![];
                let mut wf: Vec<f32> = vec![];
                for vw in &w {
                    for s in 0..4 {
                        let (j, x) = vw.get(s).copied().unwrap_or((0, 0.0));
                        jb.extend((j as u16).to_le_bytes());
                        wf.push(x);
                    }
                }
                let jv = bin.push(&jb, Some(34962));
                bin.accessors.push(serde_json::json!({ "bufferView": jv, "componentType": 5123, "count": w.len(), "type": "VEC4" }));
                attrs["JOINTS_0"] = (bin.accessors.len() - 1).into();
                attrs["WEIGHTS_0"] = bin.floats(&wf, "VEC4", w.len(), Some(34962), false).into();
                let ibm: Vec<f32> = joints.iter().flat_map(|b| inv4(&m4_of(&rest, *b))).collect();
                let ibm_a = bin.floats(&ibm, "MAT4", joints.len(), None, false);
                skins.push(serde_json::json!({ "name": o.skin, "joints": js, "inverseBindMatrices": ibm_a }));
                skin_ix = Some(skins.len() - 1);
                skinned += 1;
            }
        }
        meshes.push(serde_json::json!({ "name": o.name, "primitives": [ { "attributes": attrs, "indices": ia, "material": materials.len() - 1, "mode": 4 } ] }));
        let mut n = serde_json::json!({ "name": o.name, "mesh": meshes.len() - 1 });
        if let Some(s) = skin_ix {
            n["skin"] = s.into();
        }
        nodes.push(n);
        roots.push(nodes.len() - 1);
    }

    // Animation des os : chaque image de la plage, en local
    let mut animations = vec![];
    let animated = scene.objects.iter().any(|o| !o.keys.is_empty());
    if animated && !bones.is_empty() && scene.end > scene.start {
        let frames: Vec<i32> = (scene.start..=scene.end).collect();
        let times: Vec<f32> = frames.iter().map(|f| (f - scene.start) as f32 / scene.fps.max(1.0)).collect();
        let ta = bin.floats(&times, "SCALAR", times.len(), None, true);
        let mut tr: Vec<Vec<f32>> = vec![vec![]; bones.len()];
        let mut ro: Vec<Vec<f32>> = vec![vec![]; bones.len()];
        let mut sc: Vec<Vec<f32>> = vec![vec![]; bones.len()];
        for f in &frames {
            let mut s = scene.clone();
            s.apply_frame(*f as f32);
            for (i, &b) in bones.iter().enumerate() {
                let (t, r, z) = decompose(&local(&s, b, bone_parent(b)));
                // Quaternions du même côté que le précédent : pas de demi-tour à l'interpolation
                let r = match ro[i].len() {
                    n if n >= 4 => {
                        let p = &ro[i][n - 4..];
                        if p[0] * r[0] + p[1] * r[1] + p[2] * r[2] + p[3] * r[3] < 0.0 {
                            [-r[0], -r[1], -r[2], -r[3]]
                        } else {
                            r
                        }
                    }
                    _ => r,
                };
                tr[i].extend(t);
                ro[i].extend(r);
                sc[i].extend(z);
            }
        }
        let mut samplers = vec![];
        let mut channels = vec![];
        for i in 0..bones.len() {
            for (path, data, ty) in [("translation", &tr[i], "VEC3"), ("rotation", &ro[i], "VEC4"), ("scale", &sc[i], "VEC3")] {
                let a = bin.floats(data, ty, times.len(), None, false);
                samplers.push(serde_json::json!({ "input": ta, "output": a, "interpolation": "LINEAR" }));
                channels.push(serde_json::json!({ "sampler": samplers.len() - 1, "target": { "node": i, "path": path } }));
            }
        }
        animations.push(serde_json::json!({ "name": "Animation", "samplers": samplers, "channels": channels }));
    }

    while bin.data.len() % 4 != 0 {
        bin.data.push(0);
    }
    let mut json = serde_json::json!({
        "asset": { "version": "2.0", "generator": "Aryadne" },
        "scene": 0,
        "scenes": [ { "nodes": roots } ],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "buffers": [ { "byteLength": bin.data.len() } ],
        "bufferViews": bin.views,
        "accessors": bin.accessors,
    });
    if !skins.is_empty() {
        json["skins"] = serde_json::json!(skins);
    }
    if !animations.is_empty() {
        json["animations"] = serde_json::json!(animations);
    }
    let mut js = serde_json::to_vec(&json).map_err(|e| e.to_string())?;
    while js.len() % 4 != 0 {
        js.push(b' ');
    }
    let total = 12 + 8 + js.len() + 8 + bin.data.len();
    let mut o: Vec<u8> = Vec::with_capacity(total);
    o.extend_from_slice(b"glTF");
    o.extend(2u32.to_le_bytes());
    o.extend((total as u32).to_le_bytes());
    o.extend((js.len() as u32).to_le_bytes());
    o.extend_from_slice(b"JSON");
    o.extend(js);
    o.extend((bin.data.len() as u32).to_le_bytes());
    o.extend_from_slice(b"BIN\0");
    o.extend(bin.data);
    std::fs::write(out, o).map_err(|e| e.to_string())?;
    Ok((bones.len(), skinned))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decomposition() {
        // Rotation de 90° autour de Z, taille 2, déplacement
        let m: M4 = [0.0, 2.0, 0.0, 0.0, -2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 1.0, 2.0, 3.0, 1.0];
        let (t, q, s) = decompose(&m);
        assert_eq!(t, [1.0, 2.0, 3.0]);
        assert!((s[0] - 2.0).abs() < 1e-5 && (s[2] - 2.0).abs() < 1e-5);
        let h = std::f32::consts::FRAC_1_SQRT_2;
        assert!((q[2] - h).abs() < 1e-4 && (q[3] - h).abs() < 1e-4, "{q:?}");
        let i = inv4(&m);
        let id = mul4(&m, &i);
        for c in 0..4 {
            for r in 0..4 {
                assert!((id[c * 4 + r] - if c == r { 1.0 } else { 0.0 }).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn scene_avec_squelette() {
        let dir = std::env::temp_dir().join(format!("ary-gltf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Scene::default();
        let b0 = crate::rig::add_bone(&mut s, None);
        let b1 = crate::rig::add_bone(&mut s, Some(b0));
        let m = s.add_primitive("@cube");
        crate::rig::bind(&mut s, m, b0);
        // Deux clés de rotation sur le second os
        s.objects[b1].set_key(0);
        s.objects[b1].rot = [0.0, 0.0, 45.0];
        s.objects[b1].set_key(10);
        s.start = 0;
        s.end = 10;
        let out = dir.join("t.glb");
        let (bones, skinned) = export(&s, &dir.join("s.aryscene"), &out).unwrap();
        assert_eq!((bones, skinned), (2, 1));
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(&bytes[0..4], b"glTF");
        let jl = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let j: serde_json::Value = serde_json::from_slice(&bytes[20..20 + jl]).unwrap();
        assert_eq!(j["skins"][0]["joints"].as_array().unwrap().len(), 2);
        assert!(j["meshes"][0]["primitives"][0]["attributes"]["JOINTS_0"].is_number());
        assert_eq!(j["animations"][0]["channels"].as_array().unwrap().len(), 6);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
