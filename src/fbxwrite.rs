//! Écriture FBX binaire 7.4 (lu par Blender, Maya, 3ds Max, Unity, Unreal) :
//! maillage en polygones, UV, un matériau par ID.
use crate::uvedit::{Edit, V3};
use std::io::Write;

pub(crate) enum P {
    I(i32),
    L(i64),
    D(f64),
    S(Vec<u8>),
    C(bool),
    Di(Vec<f64>),
    Ii(Vec<i32>),
    Fi(Vec<f32>),
    Li(Vec<i64>),
}

pub(crate) struct Node {
    name: &'static str,
    props: Vec<P>,
    kids: Vec<Node>,
}

pub(crate) fn n(name: &'static str, props: Vec<P>, kids: Vec<Node>) -> Node {
    Node { name, props, kids }
}
pub(crate) fn s(x: &str) -> P {
    P::S(x.as_bytes().to_vec())
}
/// Nom d'objet FBX : « nom\0\x01Classe »
pub(crate) fn oname(name: &str, class: &str) -> P {
    let mut v = name.as_bytes().to_vec();
    v.extend([0u8, 1u8]);
    v.extend(class.as_bytes());
    P::S(v)
}
/// Propriété « P » d'un Properties70
pub(crate) fn pp(name: &str, ty: &str, label: &str, flags: &str, vals: Vec<P>) -> Node {
    let mut props = vec![s(name), s(ty), s(label), s(flags)];
    props.extend(vals);
    n("P", props, vec![])
}

fn write_props(out: &mut Vec<u8>, props: &[P]) {
    for p in props {
        match p {
            P::I(v) => {
                out.push(b'I');
                out.extend(v.to_le_bytes());
            }
            P::L(v) => {
                out.push(b'L');
                out.extend(v.to_le_bytes());
            }
            P::D(v) => {
                out.push(b'D');
                out.extend(v.to_le_bytes());
            }
            P::C(v) => {
                out.push(b'C');
                out.push(*v as u8);
            }
            P::S(v) => {
                out.push(b'S');
                out.extend((v.len() as u32).to_le_bytes());
                out.extend(v);
            }
            P::Di(v) => {
                out.push(b'd');
                out.extend((v.len() as u32).to_le_bytes());
                out.extend(0u32.to_le_bytes());
                out.extend(((v.len() * 8) as u32).to_le_bytes());
                for x in v {
                    out.extend(x.to_le_bytes());
                }
            }
            P::Fi(v) => {
                out.push(b'f');
                out.extend((v.len() as u32).to_le_bytes());
                out.extend(0u32.to_le_bytes());
                out.extend(((v.len() * 4) as u32).to_le_bytes());
                for x in v {
                    out.extend(x.to_le_bytes());
                }
            }
            P::Li(v) => {
                out.push(b'l');
                out.extend((v.len() as u32).to_le_bytes());
                out.extend(0u32.to_le_bytes());
                out.extend(((v.len() * 8) as u32).to_le_bytes());
                for x in v {
                    out.extend(x.to_le_bytes());
                }
            }
            P::Ii(v) => {
                out.push(b'i');
                out.extend((v.len() as u32).to_le_bytes());
                out.extend(0u32.to_le_bytes());
                out.extend(((v.len() * 4) as u32).to_le_bytes());
                for x in v {
                    out.extend(x.to_le_bytes());
                }
            }
        }
    }
}

pub(crate) fn write_node(out: &mut Vec<u8>, node: &Node) {
    let start = out.len();
    out.extend([0u8; 12]); // fin, nombre de propriétés, longueur des propriétés
    out.push(node.name.len() as u8);
    out.extend(node.name.as_bytes());
    let pstart = out.len();
    write_props(out, &node.props);
    let plen = out.len() - pstart;
    if !node.kids.is_empty() {
        for k in &node.kids {
            write_node(out, k);
        }
        out.extend([0u8; 13]);
    }
    let end = out.len() as u32;
    out[start..start + 4].copy_from_slice(&end.to_le_bytes());
    out[start + 4..start + 8].copy_from_slice(&(node.props.len() as u32).to_le_bytes());
    out[start + 8..start + 12].copy_from_slice(&(plen as u32).to_le_bytes());
}

pub fn export(e: &Edit, world: &dyn Fn(V3) -> V3, path: &std::path::Path) -> Result<(), String> {
    let name = path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "objet".into());
    let t = &e.topo;
    let d = &e.data;
    let vertices: Vec<f64> = t.pos.iter().flat_map(|p| {
        let q = world(*p);
        [q[0] as f64, q[1] as f64, q[2] as f64]
    }).collect();
    let mut pvi: Vec<i32> = vec![];
    for poly in &t.polys {
        for (k, v) in poly.iter().enumerate() {
            pvi.push(if k + 1 == poly.len() { -(*v as i32) - 1 } else { *v as i32 });
        }
    }
    let (geo_id, model_id) = (1_000_001i64, 1_000_002i64);
    let mut layer_elems = vec![];
    let mut geo_kids = vec![
        n("Vertices", vec![P::Di(vertices)], vec![]),
        n("PolygonVertexIndex", vec![P::Ii(pvi)], vec![]),
        n("GeometryVersion", vec![P::I(124)], vec![]),
    ];
    if d.has_uv() {
        let mut uv: Vec<f64> = vec![];
        let mut idx: Vec<i32> = vec![];
        for f in &d.uv {
            for q in f {
                idx.push((uv.len() / 2) as i32);
                uv.push(q[0] as f64);
                uv.push(q[1] as f64);
            }
        }
        geo_kids.push(n(
            "LayerElementUV",
            vec![P::I(0)],
            vec![
                n("Version", vec![P::I(101)], vec![]),
                n("Name", vec![s("UVMap")], vec![]),
                n("MappingInformationType", vec![s("ByPolygonVertex")], vec![]),
                n("ReferenceInformationType", vec![s("IndexToDirect")], vec![]),
                n("UV", vec![P::Di(uv)], vec![]),
                n("UVIndex", vec![P::Ii(idx)], vec![]),
            ],
        ));
        layer_elems.push(n("LayerElement", vec![], vec![n("Type", vec![s("LayerElementUV")], vec![]), n("TypedIndex", vec![P::I(0)], vec![])]));
    }
    let mats: Vec<i32> = d.face_mat.iter().map(|m| *m as i32).collect();
    geo_kids.push(n(
        "LayerElementMaterial",
        vec![P::I(0)],
        vec![
            n("Version", vec![P::I(101)], vec![]),
            n("Name", vec![s("")], vec![]),
            n("MappingInformationType", vec![s("ByPolygon")], vec![]),
            n("ReferenceInformationType", vec![s("IndexToDirect")], vec![]),
            n("Materials", vec![P::Ii(mats)], vec![]),
        ],
    ));
    layer_elems.push(n("LayerElement", vec![], vec![n("Type", vec![s("LayerElementMaterial")], vec![]), n("TypedIndex", vec![P::I(0)], vec![])]));
    let mut layer = vec![n("Version", vec![P::I(100)], vec![])];
    layer.extend(layer_elems);
    geo_kids.push(n("Layer", vec![P::I(0)], layer));
    let mut objects = vec![
        n("Geometry", vec![P::L(geo_id), oname(&name, "Geometry"), s("Mesh")], geo_kids),
        n(
            "Model",
            vec![P::L(model_id), oname(&name, "Model"), s("Mesh")],
            vec![
                n("Version", vec![P::I(232)], vec![]),
                n("Properties70", vec![], vec![pp("DefaultAttributeIndex", "int", "Integer", "", vec![P::I(0)])]),
                n("Shading", vec![P::C(true)], vec![]),
                n("Culling", vec![s("CullingOff")], vec![]),
            ],
        ),
    ];
    let mut conns = vec![n("C", vec![s("OO"), P::L(model_id), P::L(0)], vec![]), n("C", vec![s("OO"), P::L(geo_id), P::L(model_id)], vec![])];
    for (i, slot) in d.slots.iter().enumerate() {
        let id = 2_000_000 + i as i64;
        let c = |x: u8| (x as f64 / 255.0).powf(2.2);
        objects.push(n(
            "Material",
            vec![P::L(id), oname(&slot.name, "Material"), s("")],
            vec![
                n("Version", vec![P::I(102)], vec![]),
                n("ShadingModel", vec![s("phong")], vec![]),
                n("MultiLayer", vec![P::I(0)], vec![]),
                n("Properties70", vec![], vec![pp("DiffuseColor", "Color", "", "A", vec![P::D(c(slot.color[0])), P::D(c(slot.color[1])), P::D(c(slot.color[2]))]), pp("DiffuseFactor", "Number", "", "A", vec![P::D(1.0)])]),
            ],
        ));
        conns.push(n("C", vec![s("OO"), P::L(id), P::L(model_id)], vec![]));
    }
    let header = n(
        "FBXHeaderExtension",
        vec![],
        vec![n("FBXHeaderVersion", vec![P::I(1003)], vec![]), n("FBXVersion", vec![P::I(7400)], vec![]), n("Creator", vec![s("Aryadne")], vec![])],
    );
    let global = n(
        "GlobalSettings",
        vec![],
        vec![
            n("Version", vec![P::I(1000)], vec![]),
            n(
                "Properties70",
                vec![],
                vec![
                    pp("UpAxis", "int", "Integer", "", vec![P::I(1)]),
                    pp("UpAxisSign", "int", "Integer", "", vec![P::I(1)]),
                    pp("FrontAxis", "int", "Integer", "", vec![P::I(2)]),
                    pp("FrontAxisSign", "int", "Integer", "", vec![P::I(1)]),
                    pp("CoordAxis", "int", "Integer", "", vec![P::I(0)]),
                    pp("CoordAxisSign", "int", "Integer", "", vec![P::I(1)]),
                    pp("UnitScaleFactor", "double", "Number", "", vec![P::D(100.0)]),
                ],
            ),
        ],
    );
    let top = vec![header, global, n("Documents", vec![], vec![n("Count", vec![P::I(1)], vec![])]), n("References", vec![], vec![]), n("Objects", vec![], objects), n("Connections", vec![], conns)];
    write_file(&top, path)
}

/// En-tête, en Y vers le haut et en centimètres
pub(crate) fn header_nodes() -> (Node, Node) {
    let header = n(
        "FBXHeaderExtension",
        vec![],
        vec![n("FBXHeaderVersion", vec![P::I(1003)], vec![]), n("FBXVersion", vec![P::I(7400)], vec![]), n("Creator", vec![s("Aryadne")], vec![])],
    );
    let global = n(
        "GlobalSettings",
        vec![],
        vec![
            n("Version", vec![P::I(1000)], vec![]),
            n(
                "Properties70",
                vec![],
                vec![
                    pp("UpAxis", "int", "Integer", "", vec![P::I(1)]),
                    pp("UpAxisSign", "int", "Integer", "", vec![P::I(1)]),
                    pp("FrontAxis", "int", "Integer", "", vec![P::I(2)]),
                    pp("FrontAxisSign", "int", "Integer", "", vec![P::I(1)]),
                    pp("CoordAxis", "int", "Integer", "", vec![P::I(0)]),
                    pp("CoordAxisSign", "int", "Integer", "", vec![P::I(1)]),
                    pp("UnitScaleFactor", "double", "Number", "", vec![P::D(100.0)]),
                ],
            ),
        ],
    );
    (header, global)
}

/// Écrit les nœuds du haut dans un FBX binaire complet
pub(crate) fn write_file(top: &[Node], path: &std::path::Path) -> Result<(), String> {
    let mut out = b"Kaydara FBX Binary  \x00\x1a\x00".to_vec();
    out.extend(7400u32.to_le_bytes());
    for node in top {
        write_node(&mut out, node);
    }
    out.extend([0u8; 13]);
    // Pied de page standard
    out.extend([0xfa, 0xbc, 0xab, 0x09, 0xd0, 0xc8, 0xd4, 0x66, 0xb1, 0x76, 0xfb, 0x83, 0x1c, 0xf7, 0x26, 0x7e]);
    while out.len() % 16 != 0 {
        out.push(0);
    }
    out.extend([0u8; 4]);
    out.extend(7400u32.to_le_bytes());
    out.extend([0u8; 120]);
    out.extend([0xf8, 0x5a, 0x8c, 0x6a, 0xde, 0xf5, 0xd9, 0x7e, 0xec, 0xe9, 0x0c, 0xe3, 0x75, 0x8f, 0x29, 0x0b]);
    let mut f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    f.write_all(&out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn relu_par_aryadne() {
        let dir = std::env::temp_dir().join(format!("aryadne-fbx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("cube.obj");
        crate::vault::atomic_write(&src, "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nv 1 0 1\nv 1 1 1\nv 0 1 1\nf 1 4 3 2\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n").unwrap();
        let model = crate::model3d::load(&src).unwrap();
        let mesh = crate::modifiers::Mesh::from_model(&model, false);
        let e = crate::uvedit::Edit::open(&mesh, dir.join("d.json"), 1.0);
        super::export(&e, &|p| p, &dir.join("c.fbx")).unwrap();
        let back = crate::model3d::load(&dir.join("c.fbx")).unwrap();
        assert_eq!(back.total_tris, 12);
        let _ = std::fs::remove_dir_all(dir);
    }
}
