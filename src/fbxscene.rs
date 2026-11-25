//! Export d'une scène en FBX binaire avec ses squelettes : os (LimbNode), peau (Skin et
//! Clusters), pose de liaison et animation des os, pour Maya, 3ds Max, Blender, Unity, Unreal.

use crate::fbxwrite::{header_nodes, n, oname, pp, s, write_file, Node, P};
use crate::scene::Scene;
use std::path::Path;

/// Une seconde en unités de temps FBX
const KTIME: i64 = 46_186_158_000;

/// Angles d'Euler (degrés) d'une rotation dans l'ordre FBX par défaut (X, puis Y, puis Z : R = Rz·Ry·Rx)
fn euler_xyz(q: [f32; 4]) -> [f64; 3] {
    let (x, y, z, w) = (q[0] as f64, q[1] as f64, q[2] as f64, q[3] as f64);
    let r20 = 2.0 * (x * z - w * y);
    let ry = (-r20).clamp(-1.0, 1.0).asin();
    let (rx, rz) = if r20.abs() < 0.9999 {
        ((2.0 * (y * z + w * x)).atan2(1.0 - 2.0 * (x * x + y * y)), (2.0 * (x * y + w * z)).atan2(1.0 - 2.0 * (y * y + z * z)))
    } else {
        // Blocage de cardan : tout passe sur X
        ((-2.0 * (y * z - w * x)).atan2(1.0 - 2.0 * (x * x + z * z)), 0.0)
    };
    [rx.to_degrees(), ry.to_degrees(), rz.to_degrees()]
}

/// Matrice monde d'un objet, en doubles (colonnes)
fn world_m(scene: &Scene, k: usize) -> Vec<f64> {
    let (m, o) = crate::rig::affine(scene, k);
    vec![m[0][0], m[1][0], m[2][0], 0.0, m[0][1], m[1][1], m[2][1], 0.0, m[0][2], m[1][2], m[2][2], 0.0, o[0], o[1], o[2], 1.0].into_iter().map(|x| x as f64).collect()
}

fn identity() -> Vec<f64> {
    vec![1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]
}

fn trs_props(t: [f32; 3], r: [f64; 3], sc: [f32; 3]) -> Vec<Node> {
    vec![
        pp("Lcl Translation", "Lcl Translation", "", "A", vec![P::D(t[0] as f64), P::D(t[1] as f64), P::D(t[2] as f64)]),
        pp("Lcl Rotation", "Lcl Rotation", "", "A", vec![P::D(r[0]), P::D(r[1]), P::D(r[2])]),
        pp("Lcl Scaling", "Lcl Scaling", "", "A", vec![P::D(sc[0] as f64), P::D(sc[1] as f64), P::D(sc[2] as f64)]),
    ]
}

fn c_oo(a: i64, b: i64) -> Node {
    n("C", vec![s("OO"), P::L(a), P::L(b)], vec![])
}
fn c_op(a: i64, b: i64, prop: &str) -> Node {
    n("C", vec![s("OP"), P::L(a), P::L(b), s(prop)], vec![])
}

/// Exporte la scène ; rend le nombre d'os et de modèles liés
pub fn export(scene: &Scene, scene_path: &Path, out: &Path) -> Result<(usize, usize), String> {
    let rest = crate::rig::rest_pose(scene);
    let (model, _, ranges) = crate::scene::build_ranges(&rest, scene_path);
    let model = model?;
    let mut objects: Vec<Node> = vec![];
    let mut conns: Vec<Node> = vec![];
    let mut next = 3_000_000i64;
    let mut id = || {
        next += 1;
        next
    };
    let bones: Vec<usize> = (0..scene.objects.len()).filter(|&k| crate::rig::is_bone(scene, k)).collect();
    let bone_parent = |k: usize| scene.parent_of(k).filter(|p| crate::rig::is_bone(scene, *p));
    let local = |sc: &Scene, b: usize| -> ([f32; 3], [f32; 4], [f32; 3]) {
        let w = |k: usize| -> [f32; 16] {
            let (m, o) = crate::rig::affine(sc, k);
            [m[0][0], m[1][0], m[2][0], 0.0, m[0][1], m[1][1], m[2][1], 0.0, m[0][2], m[1][2], m[2][2], 0.0, o[0], o[1], o[2], 1.0]
        };
        let m = match bone_parent(b) {
            Some(p) => crate::gltfscene::mul4_pub(&crate::gltfscene::inv4_pub(&w(p)), &w(b)),
            None => w(b),
        };
        crate::gltfscene::decompose(&m)
    };
    // Os
    let mut bone_ids = std::collections::HashMap::new();
    let mut pose_nodes: Vec<Node> = vec![];
    for &b in &bones {
        let (attr, mid) = (id(), id());
        let name = &scene.objects[b].name;
        objects.push(n("NodeAttribute", vec![P::L(attr), oname(name, "NodeAttribute"), s("LimbNode")], vec![n("Properties70", vec![], vec![pp("Size", "double", "Number", "", vec![P::D(scene.objects[b].length as f64 * 100.0)])]), n("TypeFlags", vec![s("Skeleton")], vec![])]));
        let (t, q, sc) = local(&rest, b);
        let mut props = trs_props(t, euler_xyz(q), sc);
        props.push(pp("DefaultAttributeIndex", "int", "Integer", "", vec![P::I(0)]));
        objects.push(n("Model", vec![P::L(mid), oname(name, "Model"), s("LimbNode")], vec![n("Version", vec![P::I(232)], vec![]), n("Properties70", vec![], props), n("Shading", vec![P::C(true)], vec![]), n("Culling", vec![s("CullingOff")], vec![])]));
        conns.push(c_oo(attr, mid));
        bone_ids.insert(b, mid);
        pose_nodes.push(n("PoseNode", vec![], vec![n("Node", vec![P::L(mid)], vec![]), n("Matrix", vec![P::Di(world_m(&rest, b))], vec![])]));
    }
    for &b in &bones {
        let parent = bone_parent(b).and_then(|p| bone_ids.get(&p).copied()).unwrap_or(0);
        conns.push(c_oo(bone_ids[&b], parent));
    }
    // Modèles
    let mut skinned = 0;
    for (k, t0, t1) in ranges {
        let o = &scene.objects[k];
        let mut remap = std::collections::HashMap::new();
        let mut pos: Vec<[f32; 3]> = vec![];
        let mut pvi: Vec<i32> = vec![];
        for t in &model.tris[t0..t1] {
            for (c, &v) in t.iter().enumerate() {
                let i = *remap.entry(v).or_insert_with(|| {
                    pos.push(model.positions[v as usize]);
                    pos.len() as i32 - 1
                });
                pvi.push(if c == 2 { -i - 1 } else { i });
            }
        }
        if pos.is_empty() {
            continue;
        }
        let (geo, mid, mat) = (id(), id(), id());
        let verts: Vec<f64> = pos.iter().flat_map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
        objects.push(n("Geometry", vec![P::L(geo), oname(&o.name, "Geometry"), s("Mesh")], vec![
            n("Vertices", vec![P::Di(verts)], vec![]),
            n("PolygonVertexIndex", vec![P::Ii(pvi)], vec![]),
            n("GeometryVersion", vec![P::I(124)], vec![]),
            n("Layer", vec![P::I(0)], vec![n("Version", vec![P::I(100)], vec![])]),
        ]));
        objects.push(n("Model", vec![P::L(mid), oname(&o.name, "Model"), s("Mesh")], vec![
            n("Version", vec![P::I(232)], vec![]),
            n("Properties70", vec![], vec![pp("DefaultAttributeIndex", "int", "Integer", "", vec![P::I(0)])]),
            n("Shading", vec![P::C(true)], vec![]),
            n("Culling", vec![s("CullingOff")], vec![]),
        ]));
        let col = o.color.unwrap_or([200, 200, 200]);
        let lin = |x: u8| (x as f64 / 255.0).powf(2.2);
        objects.push(n("Material", vec![P::L(mat), oname(&o.name, "Material"), s("")], vec![
            n("Version", vec![P::I(102)], vec![]),
            n("ShadingModel", vec![s("phong")], vec![]),
            n("MultiLayer", vec![P::I(0)], vec![]),
            n("Properties70", vec![], vec![pp("DiffuseColor", "Color", "", "A", vec![P::D(lin(col[0])), P::D(lin(col[1])), P::D(lin(col[2]))])]),
        ]));
        conns.push(c_oo(mid, 0));
        conns.push(c_oo(geo, mid));
        conns.push(c_oo(mat, mid));
        pose_nodes.push(n("PoseNode", vec![], vec![n("Node", vec![P::L(mid)], vec![]), n("Matrix", vec![P::Di(identity())], vec![])]));
        // Peau : un cluster par os, avec les sommets qu'il tire et leurs poids
        if let Some((joints, w)) = crate::rig::skin_weights(&rest, k, &pos) {
            let skin = id();
            objects.push(n("Deformer", vec![P::L(skin), oname(&o.name, "Deformer"), s("Skin")], vec![n("Version", vec![P::I(101)], vec![]), n("Link_DeformAcuracy", vec![P::D(50.0)], vec![])]));
            conns.push(c_oo(skin, geo));
            for (j, b) in joints.iter().enumerate() {
                let (mut idx, mut wt) = (vec![], vec![]);
                for (v, vw) in w.iter().enumerate() {
                    if let Some((_, x)) = vw.iter().find(|(jj, x)| *jj == j && *x > 1e-4) {
                        idx.push(v as i32);
                        wt.push(*x as f64);
                    }
                }
                let cl = id();
                objects.push(n("Deformer", vec![P::L(cl), oname(&scene.objects[*b].name, "SubDeformer"), s("Cluster")], vec![
                    n("Version", vec![P::I(100)], vec![]),
                    n("UserData", vec![s(""), s("")], vec![]),
                    n("Indexes", vec![P::Ii(idx)], vec![]),
                    n("Weights", vec![P::Di(wt)], vec![]),
                    n("Transform", vec![P::Di(identity())], vec![]),
                    n("TransformLink", vec![P::Di(world_m(&rest, *b))], vec![]),
                ]));
                conns.push(c_oo(cl, skin));
                if let Some(bm) = bone_ids.get(b) {
                    conns.push(c_oo(*bm, cl));
                }
            }
            skinned += 1;
        }
    }
    // Pose de liaison
    if !bones.is_empty() {
        let pid = id();
        let mut kids = vec![n("Type", vec![s("BindPose")], vec![]), n("Version", vec![P::I(100)], vec![]), n("NbPoseNodes", vec![P::I(pose_nodes.len() as i32)], vec![])];
        kids.extend(pose_nodes);
        objects.push(n("Pose", vec![P::L(pid), oname("BindPose", "Pose"), s("BindPose")], kids));
    }
    // Animation des os, image par image
    let animated = scene.objects.iter().any(|o| !o.keys.is_empty());
    if animated && !bones.is_empty() && scene.end > scene.start {
        let (stack, layer) = (id(), id());
        let fps = scene.fps.max(1.0) as f64;
        let stop = ((scene.end - scene.start) as f64 / fps * KTIME as f64) as i64;
        objects.push(n("AnimationStack", vec![P::L(stack), oname("Animation", "AnimStack"), s("")], vec![n("Properties70", vec![], vec![pp("LocalStop", "KTime", "Time", "", vec![P::L(stop)]), pp("ReferenceStop", "KTime", "Time", "", vec![P::L(stop)])])]));
        objects.push(n("AnimationLayer", vec![P::L(layer), oname("BaseLayer", "AnimLayer"), s("")], vec![]));
        conns.push(c_oo(layer, stack));
        let frames: Vec<i32> = (scene.start..=scene.end).collect();
        let times: Vec<i64> = frames.iter().map(|f| ((f - scene.start) as f64 / fps * KTIME as f64) as i64).collect();
        // [os][T R S][x y z] → valeurs par image
        let mut vals = vec![[[Vec::<f32>::new(), vec![], vec![]], [vec![], vec![], vec![]], [vec![], vec![], vec![]]]; bones.len()];
        let mut prev_r: Vec<Option<[f64; 3]>> = vec![None; bones.len()];
        for f in &frames {
            let mut sc = scene.clone();
            sc.apply_frame(*f as f32);
            for (i, &b) in bones.iter().enumerate() {
                let (t, q, z) = local(&sc, b);
                let mut r = euler_xyz(q);
                // Pas de saut de 360° d'une image à l'autre
                if let Some(p) = prev_r[i] {
                    for a in 0..3 {
                        while r[a] - p[a] > 180.0 {
                            r[a] -= 360.0;
                        }
                        while r[a] - p[a] < -180.0 {
                            r[a] += 360.0;
                        }
                    }
                }
                prev_r[i] = Some(r);
                for a in 0..3 {
                    vals[i][0][a].push(t[a]);
                    vals[i][1][a].push(r[a] as f32);
                    vals[i][2][a].push(z[a]);
                }
            }
        }
        for (i, &b) in bones.iter().enumerate() {
            for (c, prop, short) in [(0usize, "Lcl Translation", "T"), (1, "Lcl Rotation", "R"), (2, "Lcl Scaling", "S")] {
                let cn = id();
                let v0 = [vals[i][c][0][0] as f64, vals[i][c][1][0] as f64, vals[i][c][2][0] as f64];
                objects.push(n("AnimationCurveNode", vec![P::L(cn), oname(short, "AnimCurveNode"), s("")], vec![n("Properties70", vec![], vec![pp("d|X", "Number", "", "A", vec![P::D(v0[0])]), pp("d|Y", "Number", "", "A", vec![P::D(v0[1])]), pp("d|Z", "Number", "", "A", vec![P::D(v0[2])])])]));
                conns.push(c_oo(cn, layer));
                conns.push(c_op(cn, bone_ids[&b], prop));
                for (a, axis) in ["d|X", "d|Y", "d|Z"].iter().enumerate() {
                    let cv = id();
                    let nk = times.len() as i32;
                    objects.push(n("AnimationCurve", vec![P::L(cv), oname("", "AnimCurve"), s("")], vec![
                        n("Default", vec![P::D(v0[a])], vec![]),
                        n("KeyVer", vec![P::I(4009)], vec![]),
                        n("KeyTime", vec![P::Li(times.clone())], vec![]),
                        n("KeyValueFloat", vec![P::Fi(vals[i][c][a].clone())], vec![]),
                        // Interpolation linéaire pour toutes les clés
                        n("KeyAttrFlags", vec![P::Ii(vec![0x0000_0004])], vec![]),
                        n("KeyAttrDataFloat", vec![P::Fi(vec![0.0, 0.0, 0.0, 0.0])], vec![]),
                        n("KeyAttrRefCount", vec![P::Ii(vec![nk])], vec![]),
                    ]));
                    conns.push(c_op(cv, cn, axis));
                }
            }
        }
    }
    let (header, global) = header_nodes();
    let top = vec![header, global, n("Documents", vec![], vec![n("Count", vec![P::I(1)], vec![])]), n("References", vec![], vec![]), n("Objects", vec![], objects), n("Connections", vec![], conns)];
    write_file(&top, out)?;
    Ok((bones.len(), skinned))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_fbx_avec_squelette() {
        let dir = std::env::temp_dir().join(format!("ary-fbxs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Scene::default();
        let b0 = crate::rig::add_bone(&mut s, None);
        let b1 = crate::rig::add_bone(&mut s, Some(b0));
        let m = s.add_primitive("@cube");
        crate::rig::bind(&mut s, m, b0);
        s.objects[b1].set_key(0);
        s.objects[b1].rot = [0.0, 0.0, 30.0];
        s.objects[b1].set_key(5);
        s.end = 5;
        let out = dir.join("s.fbx");
        assert_eq!(export(&s, &dir.join("s.aryscene"), &out).unwrap(), (2, 1));
        // Notre propre lecteur FBX retrouve le maillage
        let back = crate::model3d::load(&out).unwrap();
        assert_eq!(back.total_tris, 12);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn euler() {
        // 90° autour de Z
        let h = std::f32::consts::FRAC_1_SQRT_2;
        let e = euler_xyz([0.0, 0.0, h, h]);
        assert!((e[2] - 90.0).abs() < 1e-3 && e[0].abs() < 1e-3 && e[1].abs() < 1e-3);
    }
}
