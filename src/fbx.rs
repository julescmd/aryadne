//! Lecteur FBX binaire (versions 7.x, celles qu'exportent Blender, Maya, 3ds Max, ZBrush).
//! On lit la géométrie (sommets + polygones) de chaque objet et sa transformation locale.
use flate2::read::ZlibDecoder;
use std::collections::HashMap;
use std::io::Read;

#[derive(Debug, Clone)]
pub enum Prop {
    Int(i64),
    Float(f64),
    Str(String),
    Ints(Vec<i64>),
    Floats(Vec<f64>),
    Other,
}

#[derive(Debug, Default)]
pub struct Node {
    pub name: String,
    pub props: Vec<Prop>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
}

struct Reader<'a> {
    d: &'a [u8],
    wide: bool,
}

impl Reader<'_> {
    fn u8(&self, o: usize) -> Result<u8, String> {
        self.d.get(o).copied().ok_or_else(|| "FBX tronqué.".into())
    }
    fn u32(&self, o: usize) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.d.get(o..o + 4).ok_or("FBX tronqué.")?.try_into().unwrap()))
    }
    fn u64(&self, o: usize) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.d.get(o..o + 8).ok_or("FBX tronqué.")?.try_into().unwrap()))
    }
    fn word(&self, o: usize) -> Result<u64, String> {
        if self.wide { self.u64(o) } else { self.u32(o).map(|v| v as u64) }
    }

    /// Lit un nœud à `o`. Retourne None pour l'enregistrement nul de fin de liste.
    fn node(&self, o: usize) -> Result<(Option<Node>, usize), String> {
        let w = if self.wide { 8 } else { 4 };
        let end = self.word(o)? as usize;
        let nprops = self.word(o + w)? as usize;
        let plen = self.word(o + 2 * w)? as usize;
        let nlen = self.u8(o + 3 * w)? as usize;
        let head = o + 3 * w + 1;
        if end == 0 {
            return Ok((None, head + nlen));
        }
        let name = String::from_utf8_lossy(self.d.get(head..head + nlen).ok_or("FBX tronqué.")?).to_string();
        let mut p = head + nlen;
        let mut props = Vec::with_capacity(nprops);
        for _ in 0..nprops {
            let (prop, next) = self.prop(p)?;
            props.push(prop);
            p = next;
        }
        let mut c = head + nlen + plen;
        let mut children = vec![];
        while c < end {
            let (n, next) = self.node(c)?;
            c = next;
            match n {
                Some(n) => children.push(n),
                None => break,
            }
        }
        Ok((Some(Node { name, props, children }), end))
    }

    fn prop(&self, o: usize) -> Result<(Prop, usize), String> {
        let t = self.u8(o)?;
        let o = o + 1;
        let le = |b: &[u8]| -> [u8; 8] {
            let mut a = [0u8; 8];
            a[..b.len()].copy_from_slice(b);
            a
        };
        Ok(match t {
            b'Y' => (Prop::Int(i16::from_le_bytes(self.d[o..o + 2].try_into().unwrap()) as i64), o + 2),
            b'C' => (Prop::Int(self.u8(o)? as i64), o + 1),
            b'I' => (Prop::Int(self.u32(o)? as i32 as i64), o + 4),
            b'F' => (Prop::Float(f32::from_le_bytes(self.d[o..o + 4].try_into().unwrap()) as f64), o + 4),
            b'D' => (Prop::Float(f64::from_le_bytes(le(&self.d[o..o + 8]))), o + 8),
            b'L' => (Prop::Int(self.u64(o)? as i64), o + 8),
            b'S' | b'R' => {
                let n = self.u32(o)? as usize;
                let bytes = self.d.get(o + 4..o + 4 + n).ok_or("FBX tronqué.")?;
                let prop = if t == b'S' { Prop::Str(String::from_utf8_lossy(bytes).replace("\0\u{1}", "::")) } else { Prop::Other };
                (prop, o + 4 + n)
            }
            b'f' | b'd' | b'l' | b'i' | b'b' => {
                let count = self.u32(o)? as usize;
                let enc = self.u32(o + 4)?;
                let clen = self.u32(o + 8)? as usize;
                let raw = self.d.get(o + 12..o + 12 + clen).ok_or("FBX tronqué.")?;
                let bytes = if enc == 1 {
                    let mut out = Vec::with_capacity(count * 8);
                    ZlibDecoder::new(raw).read_to_end(&mut out).map_err(|e| format!("FBX : tableau compressé illisible ({e})"))?;
                    out
                } else {
                    raw.to_vec()
                };
                let prop = match t {
                    b'f' => Prop::Floats(bytes.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap()) as f64).collect()),
                    b'd' => Prop::Floats(bytes.chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect()),
                    b'l' => Prop::Ints(bytes.chunks_exact(8).map(|c| i64::from_le_bytes(c.try_into().unwrap())).collect()),
                    b'i' => Prop::Ints(bytes.chunks_exact(4).map(|c| i32::from_le_bytes(c.try_into().unwrap()) as i64).collect()),
                    _ => Prop::Other,
                };
                (prop, o + 12 + clen)
            }
            other => return Err(format!("FBX : type de propriété inconnu « {} ».", other as char)),
        })
    }
}

pub fn parse(d: &[u8]) -> Result<Vec<Node>, String> {
    if d.len() < 27 || !d.starts_with(b"Kaydara FBX Binary") {
        if d.starts_with(b"; FBX") || d.iter().take(200).all(|b| b.is_ascii()) {
            return Err("FBX texte (ASCII) non pris en charge : réexporte en FBX binaire.".into());
        }
        return Err("Ce n'est pas un fichier FBX binaire.".into());
    }
    let version = u32::from_le_bytes(d[23..27].try_into().unwrap());
    let r = Reader { d, wide: version >= 7500 };
    let mut o = 27;
    let mut nodes = vec![];
    while o < d.len() {
        let (n, next) = r.node(o)?;
        match n {
            Some(n) => nodes.push(n),
            None => break,
        }
        o = next;
    }
    Ok(nodes)
}

pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub tris: Vec<[u32; 3]>,
    /// Arêtes réelles de chaque triangle (bit 0 : v0-v1, bit 1 : v1-v2, bit 2 : v2-v0).
    pub edges: Vec<u8>,
}

fn id_of(n: &Node) -> Option<i64> {
    match n.props.first() {
        Some(Prop::Int(i)) => Some(*i),
        _ => None,
    }
}

/// Lcl Translation / Rotation (degrés, XYZ) / Scaling d'un objet « Model ».
fn transform(model: &Node) -> [[f32; 3]; 3] {
    let mut t = [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
    if let Some(p70) = model.child("Properties70") {
        for p in p70.children.iter().filter(|c| c.name == "P") {
            let Some(Prop::Str(name)) = p.props.first() else { continue };
            let v: Vec<f32> = p.props.iter().skip(4).filter_map(|x| if let Prop::Float(f) = x { Some(*f as f32) } else { None }).collect();
            if v.len() < 3 {
                continue;
            }
            match name.as_str() {
                "Lcl Translation" => t[0] = [v[0], v[1], v[2]],
                "Lcl Rotation" => t[1] = [v[0], v[1], v[2]],
                "Lcl Scaling" => t[2] = [v[0], v[1], v[2]],
                _ => {}
            }
        }
    }
    t
}

fn apply(t: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    let mut p = [v[0] * t[2][0], v[1] * t[2][1], v[2] * t[2][2]];
    let (rx, ry, rz) = (t[1][0].to_radians(), t[1][1].to_radians(), t[1][2].to_radians());
    // Ordre XYZ : X puis Y puis Z
    let (s, c) = rx.sin_cos();
    p = [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c];
    let (s, c) = ry.sin_cos();
    p = [p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c];
    let (s, c) = rz.sin_cos();
    p = [p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2]];
    [p[0] + t[0][0], p[1] + t[0][1], p[2] + t[0][2]]
}

/// Toute la géométrie du fichier, transformée et réunie en un seul maillage.
pub fn load(d: &[u8]) -> Result<Mesh, String> {
    let nodes = parse(d)?;
    let objects = nodes.iter().find(|n| n.name == "Objects").ok_or("FBX sans objets.")?;
    let models: HashMap<i64, &Node> = objects.children.iter().filter(|n| n.name == "Model").filter_map(|n| Some((id_of(n)?, n))).collect();
    // Connexions « OO » : géométrie → objet
    let mut parent: HashMap<i64, i64> = HashMap::new();
    if let Some(conns) = nodes.iter().find(|n| n.name == "Connections") {
        for c in &conns.children {
            if let (Some(Prop::Str(kind)), Some(Prop::Int(a)), Some(Prop::Int(b))) = (c.props.first(), c.props.get(1), c.props.get(2)) {
                if kind == "OO" {
                    parent.entry(*a).or_insert(*b);
                }
            }
        }
    }
    let mut positions = vec![];
    let mut tris = vec![];
    let mut edges = vec![];
    for g in objects.children.iter().filter(|n| n.name == "Geometry") {
        let (Some(Prop::Floats(v)), Some(Prop::Ints(idx))) = (g.child("Vertices").and_then(|n| n.props.first()), g.child("PolygonVertexIndex").and_then(|n| n.props.first())) else {
            continue;
        };
        let tf = id_of(g).and_then(|id| parent.get(&id)).and_then(|m| models.get(m)).map(|m| transform(m));
        let base = positions.len() as u32;
        for c in v.chunks_exact(3) {
            let p = [c[0] as f32, c[1] as f32, c[2] as f32];
            positions.push(match &tf {
                Some(t) => apply(t, p),
                None => p,
            });
        }
        // Un indice négatif ferme le polygone : vrai indice = -i - 1.
        let mut poly: Vec<u32> = vec![];
        for &i in idx {
            let (real, last) = if i < 0 { ((-i - 1) as u32, true) } else { (i as u32, false) };
            poly.push(base + real);
            if last {
                let n = poly.len();
                for k in 1..n.saturating_sub(1) {
                    tris.push([poly[0], poly[k], poly[k + 1]]);
                    edges.push(crate::model3d::fan_mask(k, n));
                }
                poly.clear();
            }
        }
    }
    if tris.is_empty() {
        return Err("Aucun maillage dans ce FBX.".into());
    }
    Ok(Mesh { positions, tris, edges })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Écrit un petit FBX binaire (7400 ou 7500) pour les tests.
    pub fn write_node(out: &mut Vec<u8>, wide: bool, name: &str, props: &[Vec<u8>], children: &[Vec<u8>]) -> Vec<u8> {
        let _ = out;
        let plist: Vec<u8> = props.concat();
        let w = if wide { 8 } else { 4 };
        let head = 3 * w + 1 + name.len();
        let mut body = plist.clone();
        for c in children {
            body.extend(c);
        }
        if !children.is_empty() {
            body.extend(vec![0u8; head - name.len()]); // enregistrement nul
        }
        let num = |v: usize| -> Vec<u8> { if wide { (v as u64).to_le_bytes().to_vec() } else { (v as u32).to_le_bytes().to_vec() } };
        let mut n = vec![];
        n.extend(num(0)); // fin, corrigée plus bas (relative)
        n.extend(num(props.len()));
        n.extend(num(plist.len()));
        n.push(name.len() as u8);
        n.extend(name.as_bytes());
        n.extend(body);
        n
    }

    /// Recalcule les offsets de fin (absolus) d'une liste de nœuds construits relativement.
    pub fn build(wide: bool, top: Vec<(String, Vec<Vec<u8>>, Vec<(String, Vec<Vec<u8>>)>)>) -> Vec<u8> {
        let mut d = b"Kaydara FBX Binary  \x00\x1a\x00".to_vec();
        d.extend((if wide { 7500u32 } else { 7400u32 }).to_le_bytes());
        let w = if wide { 8 } else { 4 };
        let put = |d: &mut Vec<u8>, at: usize, v: usize| {
            if wide {
                d[at..at + 8].copy_from_slice(&(v as u64).to_le_bytes());
            } else {
                d[at..at + 4].copy_from_slice(&(v as u32).to_le_bytes());
            }
        };
        let write = |d: &mut Vec<u8>, name: &str, props: &[Vec<u8>]| -> usize {
            let start = d.len();
            let plist: Vec<u8> = props.concat();
            d.extend(vec![0u8; w]);
            d.extend(if wide { (props.len() as u64).to_le_bytes().to_vec() } else { (props.len() as u32).to_le_bytes().to_vec() });
            d.extend(if wide { (plist.len() as u64).to_le_bytes().to_vec() } else { (plist.len() as u32).to_le_bytes().to_vec() });
            d.push(name.len() as u8);
            d.extend(name.as_bytes());
            d.extend(plist);
            start
        };
        for (name, props, children) in top {
            let s = write(&mut d, &name, &props);
            for (cn, cp) in children {
                let cs = write(&mut d, &cn, &cp);
                let end = d.len();
                put(&mut d, cs, end);
            }
            d.extend(vec![0u8; 3 * w + 1]);
            let end = d.len();
            put(&mut d, s, end);
        }
        d.extend(vec![0u8; 3 * w + 1]);
        d
    }

    pub fn arr_d(v: &[f64], compress: bool) -> Vec<u8> {
        let raw: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
        arr(b'd', v.len(), raw, compress)
    }
    pub fn arr_i(v: &[i32], compress: bool) -> Vec<u8> {
        let raw: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
        arr(b'i', v.len(), raw, compress)
    }
    fn arr(t: u8, n: usize, raw: Vec<u8>, compress: bool) -> Vec<u8> {
        let data = if compress {
            use std::io::Write;
            let mut e = flate2::write::ZlibEncoder::new(vec![], flate2::Compression::default());
            e.write_all(&raw).unwrap();
            e.finish().unwrap()
        } else {
            raw
        };
        let mut p = vec![t];
        p.extend((n as u32).to_le_bytes());
        p.extend((compress as u32).to_le_bytes());
        p.extend((data.len() as u32).to_le_bytes());
        p.extend(data);
        p
    }
    pub fn long(v: i64) -> Vec<u8> {
        let mut p = vec![b'L'];
        p.extend(v.to_le_bytes());
        p
    }

    #[test]
    fn quad_fbx() {
        for (wide, compress) in [(false, false), (true, true)] {
            let geo = vec![
                ("Vertices".to_string(), vec![arr_d(&[0., 0., 0., 1., 0., 0., 1., 2., 0., 0., 2., 0.], compress)]),
                ("PolygonVertexIndex".to_string(), vec![arr_i(&[0, 1, 2, -4], compress)]),
            ];
            let d = build(wide, vec![("Objects".to_string(), vec![], vec![]), ("Objects".to_string(), vec![], vec![])]);
            let _ = d;
            // Objets → Géométrie (3 niveaux) : construit à la main
            let d = build_geometry(wide, geo);
            let m = load(&d).unwrap();
            assert_eq!(m.positions.len(), 4);
            assert_eq!(m.tris.len(), 2, "un quad = 2 triangles");
        }
        assert!(parse(b"; FBX 7.4.0 project file").is_err());
    }

    /// FBX minimal : Objects { Geometry { Vertices, PolygonVertexIndex } }
    pub fn build_geometry(wide: bool, geo: Vec<(String, Vec<Vec<u8>>)>) -> Vec<u8> {
        let w = if wide { 8 } else { 4 };
        let mut d = b"Kaydara FBX Binary  \x00\x1a\x00".to_vec();
        d.extend((if wide { 7500u32 } else { 7400u32 }).to_le_bytes());
        let num = |v: usize| -> Vec<u8> { if wide { (v as u64).to_le_bytes().to_vec() } else { (v as u32).to_le_bytes().to_vec() } };
        fn header(d: &mut Vec<u8>, num: &dyn Fn(usize) -> Vec<u8>, name: &str, props: &[Vec<u8>]) -> usize {
            let s = d.len();
            let plist: Vec<u8> = props.concat();
            d.extend(num(0));
            d.extend(num(props.len()));
            d.extend(num(plist.len()));
            d.push(name.len() as u8);
            d.extend(name.as_bytes());
            d.extend(plist);
            s
        }
        let set = |d: &mut Vec<u8>, at: usize| {
            let end = d.len();
            let b = num(end);
            d[at..at + w].copy_from_slice(&b);
        };
        let objects = header(&mut d, &num, "Objects", &[]);
        let g = header(&mut d, &num, "Geometry", &[long(1)]);
        for (name, props) in &geo {
            let c = header(&mut d, &num, name, props);
            set(&mut d, c);
        }
        d.extend(vec![0u8; 3 * w + 1]);
        set(&mut d, g);
        d.extend(vec![0u8; 3 * w + 1]);
        set(&mut d, objects);
        d.extend(vec![0u8; 3 * w + 1]);
        d
    }
}

#[cfg(test)]
mod conv {
    #[test]
    #[ignore]
    fn fbx_en_obj() {
        let i = std::env::var("FBX_IN").unwrap();
        let m = super::load(&std::fs::read(&i).unwrap()).unwrap();
        let mut o = String::new();
        for p in &m.positions {
            o += &format!("v {} {} {}\n", p[0], p[1], p[2]);
        }
        for t in &m.tris {
            o += &format!("f {} {} {}\n", t[0] + 1, t[1] + 1, t[2] + 1);
        }
        std::fs::write(std::env::var("FBX_OUT").unwrap(), o).unwrap();
    }
}
