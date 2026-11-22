//! Autres formats 3D en lecture : 3DS (3D Studio), PLY (scans) et Maya ASCII (.ma, maillages).
use super::*;

/// Polygones → triangles en éventail, avec les bords d'origine (quads et n-gones gardés à l'affichage).
fn from_polys(positions: Vec<V3>, polys: &[Vec<u32>], z_up: bool) -> Result<Model, String> {
    let mut tris = vec![];
    let mut edges = vec![];
    for p in polys {
        let n = p.len();
        for k in 1..n.saturating_sub(1) {
            tris.push([p[0], p[k], p[k + 1]]);
            edges.push(fan_mask(k, n));
        }
    }
    Model::with_edges(positions, tris, None, Some(edges), z_up)
}

// ---------- 3DS ----------

/// 3D Studio (.3ds) : blocs (identifiant, longueur). On lit chaque maillage (sommets, faces) et
/// on le place avec sa matrice locale. Les 3DS ont Z vers le haut.
pub fn load_3ds(b: &[u8]) -> Result<Model, String> {
    let u16_at = |i: usize| -> Option<u16> { b.get(i..i + 2).map(|s| u16::from_le_bytes([s[0], s[1]])) };
    let u32_at = |i: usize| -> Option<u32> { b.get(i..i + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])) };
    let f32_at = |i: usize| -> Option<f32> { u32_at(i).map(f32::from_bits) };
    if u16_at(0) != Some(0x4D4D) {
        return Err("Ce n'est pas un fichier 3DS.".into());
    }
    let mut positions: Vec<V3> = vec![];
    let mut polys: Vec<Vec<u32>> = vec![];
    // Parcours récursif des blocs qui en contiennent d'autres
    fn walk(b: &[u8], start: usize, end: usize, out: &mut Vec<(usize, usize)>) {
        let mut i = start;
        while i + 6 <= end {
            let id = u16::from_le_bytes([b[i], b[i + 1]]);
            let len = u32::from_le_bytes([b[i + 2], b[i + 3], b[i + 4], b[i + 5]]) as usize;
            if len < 6 || i + len > end {
                break;
            }
            match id {
                0x4D4D | 0x3D3D => walk(b, i + 6, i + len, out),
                0x4000 => {
                    // Nom (chaîne terminée par 0), puis les blocs de l'objet
                    let mut j = i + 6;
                    while j < i + len && b[j] != 0 {
                        j += 1;
                    }
                    walk(b, j + 1, i + len, out);
                }
                0x4100 => out.push((i + 6, i + len)),
                _ => {}
            }
            i += len;
        }
    }
    let mut meshes = vec![];
    walk(b, 0, b.len(), &mut meshes);
    for (start, end) in meshes {
        let base = positions.len() as u32;
        let mut verts: Vec<V3> = vec![];
        let mut faces: Vec<[u32; 3]> = vec![];
        let mut i = start;
        while i + 6 <= end {
            let id = u16_at(i).unwrap_or(0);
            let len = u32_at(i + 2).unwrap_or(0) as usize;
            if len < 6 || i + len > end {
                break;
            }
            match id {
                0x4110 => {
                    let n = u16_at(i + 6).unwrap_or(0) as usize;
                    for k in 0..n {
                        let o = i + 8 + k * 12;
                        if let (Some(x), Some(y), Some(z)) = (f32_at(o), f32_at(o + 4), f32_at(o + 8)) {
                            verts.push([x, y, z]);
                        }
                    }
                }
                0x4120 => {
                    let n = u16_at(i + 6).unwrap_or(0) as usize;
                    for k in 0..n {
                        let o = i + 8 + k * 8;
                        if let (Some(a), Some(bb), Some(c)) = (u16_at(o), u16_at(o + 2), u16_at(o + 4)) {
                            faces.push([a as u32, bb as u32, c as u32]);
                        }
                    }
                }
                _ => {}
            }
            i += len;
        }
        let nv = verts.len() as u32;
        positions.extend(verts);
        for f in faces {
            if f.iter().all(|&v| v < nv) {
                polys.push(vec![base + f[0], base + f[1], base + f[2]]);
            }
        }
    }
    if polys.is_empty() {
        return Err("Aucun maillage dans ce fichier 3DS.".into());
    }
    from_polys(positions, &polys, true)
}

// ---------- PLY ----------

/// PLY (ASCII ou binaire, petit ou gros boutiste) : sommets x y z et faces.
pub fn load_ply(b: &[u8]) -> Result<Model, String> {
    let head_end = b.windows(10).position(|w| w == b"end_header").ok_or("En-tête PLY introuvable.")?;
    let header = String::from_utf8_lossy(&b[..head_end]);
    let mut body = head_end + 10;
    while body < b.len() && (b[body] == b'\r' || b[body] == b'\n') {
        body += 1;
        if b[body - 1] == b'\n' {
            break;
        }
    }
    let mut format = "ascii";
    // Éléments : nom, nombre, propriétés (type, nom ; liste : types du compte et des valeurs)
    let mut elems: Vec<(String, usize, Vec<(String, String, Option<String>)>)> = vec![];
    for line in header.lines() {
        let w: Vec<&str> = line.split_whitespace().collect();
        match w.as_slice() {
            ["format", f, ..] => {
                format = match *f {
                    "binary_little_endian" => "le",
                    "binary_big_endian" => "be",
                    _ => "ascii",
                }
            }
            ["element", name, n] => elems.push((name.to_string(), n.parse().unwrap_or(0), vec![])),
            ["property", "list", ct, vt, name] => {
                if let Some(e) = elems.last_mut() {
                    e.2.push((vt.to_string(), name.to_string(), Some(ct.to_string())));
                }
            }
            ["property", t, name] => {
                if let Some(e) = elems.last_mut() {
                    e.2.push((t.to_string(), name.to_string(), None));
                }
            }
            _ => {}
        }
    }
    let size = |t: &str| match t {
        "char" | "uchar" | "int8" | "uint8" => 1,
        "short" | "ushort" | "int16" | "uint16" => 2,
        "double" | "float64" => 8,
        _ => 4,
    };
    let mut positions: Vec<V3> = vec![];
    let mut polys: Vec<Vec<u32>> = vec![];
    if format == "ascii" {
        let text = String::from_utf8_lossy(&b[body..]);
        let mut lines = text.lines();
        for (name, n, props) in &elems {
            for _ in 0..*n {
                let Some(line) = lines.next() else { break };
                let vals: Vec<f64> = line.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                if name == "vertex" {
                    let idx = |p: &str| props.iter().position(|q| q.1 == p).and_then(|k| vals.get(k)).copied().unwrap_or(0.0) as f32;
                    positions.push([idx("x"), idx("y"), idx("z")]);
                } else if name == "face" && !vals.is_empty() {
                    let c = vals[0] as usize;
                    polys.push(vals.iter().skip(1).take(c).map(|v| *v as u32).collect());
                }
            }
        }
    } else {
        let le = format == "le";
        let mut i = body;
        let read = |i: &mut usize, t: &str| -> f64 {
            let s = size(t);
            let Some(bytes) = b.get(*i..*i + s) else { return 0.0 };
            *i += s;
            let mut a = [0u8; 8];
            a[..s].copy_from_slice(bytes);
            if !le {
                a[..s].reverse();
            }
            match t {
                "char" | "int8" => a[0] as i8 as f64,
                "uchar" | "uint8" => a[0] as f64,
                "short" | "int16" => i16::from_le_bytes([a[0], a[1]]) as f64,
                "ushort" | "uint16" => u16::from_le_bytes([a[0], a[1]]) as f64,
                "int" | "int32" => i32::from_le_bytes([a[0], a[1], a[2], a[3]]) as f64,
                "uint" | "uint32" => u32::from_le_bytes([a[0], a[1], a[2], a[3]]) as f64,
                "double" | "float64" => f64::from_le_bytes(a),
                _ => f32::from_le_bytes([a[0], a[1], a[2], a[3]]) as f64,
            }
        };
        for (name, n, props) in &elems {
            for _ in 0..*n {
                let mut p = [0.0f32; 3];
                for (t, pname, list) in props {
                    match list {
                        Some(ct) => {
                            let c = read(&mut i, ct) as usize;
                            let vals: Vec<u32> = (0..c).map(|_| read(&mut i, t) as u32).collect();
                            if name == "face" {
                                polys.push(vals);
                            }
                        }
                        None => {
                            let v = read(&mut i, t) as f32;
                            match pname.as_str() {
                                "x" => p[0] = v,
                                "y" => p[1] = v,
                                "z" => p[2] = v,
                                _ => {}
                            }
                        }
                    }
                }
                if name == "vertex" {
                    positions.push(p);
                }
            }
        }
    }
    let nv = positions.len() as u32;
    polys.retain(|f| f.len() >= 3 && f.iter().all(|&v| v < nv));
    if polys.is_empty() {
        return Err("Aucune face dans ce fichier PLY (nuage de points ?).".into());
    }
    from_polys(positions, &polys, false)
}

// ---------- Maya ASCII ----------

/// Maya ASCII (.ma) : chaque nœud `mesh` (sommets .vt, arêtes .ed, faces .fc et décalages .pt),
/// placé par la chaîne de ses nœuds `transform` (translation, rotation XYZ, échelle).
/// Les .mb (binaires) ne sont pas lisibles sans Maya : exporte-les en .ma, FBX ou OBJ.
pub fn load_ma(text: &str) -> Result<Model, String> {
    #[derive(Default)]
    struct Node {
        kind: String,
        parent: String,
        vt: Vec<V3>,
        pt: Vec<(usize, V3)>,
        ed: Vec<[i64; 2]>,
        fc: Vec<Vec<i64>>,
        t: V3,
        r: V3,
        s: Option<V3>,
        intermediate: bool,
    }
    // Les commandes se terminent par « ; » (pas dans les chaînes)
    let mut cmds = vec![];
    let mut cur = String::new();
    let mut in_str = false;
    for ch in text.chars() {
        if ch == '"' {
            in_str = !in_str;
        }
        if ch == ';' && !in_str {
            cmds.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    let mut nodes: Vec<(String, Node)> = vec![];
    let unquote = |s: &str| s.trim_matches('"').to_string();
    let range = |attr: &str| -> Option<(usize, usize)> {
        let inner = attr.split('[').nth(1)?.split(']').next()?;
        let mut it = inner.split(':');
        let a: usize = it.next()?.parse().ok()?;
        let b: usize = it.next().and_then(|x| x.parse().ok()).unwrap_or(a);
        Some((a, b))
    };
    for c in &cmds {
        let toks: Vec<&str> = c.split_whitespace().collect();
        if toks.is_empty() {
            continue;
        }
        if toks[0] == "createNode" && toks.len() >= 2 {
            let mut n = Node { kind: toks[1].to_string(), ..Default::default() };
            let mut name = String::new();
            let mut k = 2;
            while k < toks.len() {
                match toks[k] {
                    "-n" if k + 1 < toks.len() => {
                        name = unquote(toks[k + 1]);
                        k += 1;
                    }
                    "-p" if k + 1 < toks.len() => {
                        n.parent = unquote(toks[k + 1]).trim_start_matches('|').rsplit('|').next().unwrap_or("").to_string();
                        k += 1;
                    }
                    _ => {}
                }
                k += 1;
            }
            nodes.push((name, n));
            continue;
        }
        if toks[0] != "setAttr" {
            continue;
        }
        let Some((_, node)) = nodes.last_mut() else { continue };
        // setAttr [-s n] [-k on] ".attr" [-type "x"] valeurs…
        let Some(ai) = toks.iter().position(|t| t.starts_with("\".")) else { continue };
        let attr = toks[ai].trim_matches('"');
        let mut vals: Vec<&str> = toks[ai + 1..].to_vec();
        if vals.first() == Some(&"-type") {
            vals.drain(..2.min(vals.len()));
        }
        let nums = |v: &[&str]| -> Vec<f64> { v.iter().filter_map(|x| x.parse::<f64>().ok()).collect() };
        let base = attr.split('[').next().unwrap_or("");
        match base {
            ".vt" => {
                let n = nums(&vals);
                let (a, _) = range(attr).unwrap_or((node.vt.len(), 0));
                for (k, q) in n.chunks_exact(3).enumerate() {
                    let idx = a + k;
                    if node.vt.len() <= idx {
                        node.vt.resize(idx + 1, [0.0; 3]);
                    }
                    node.vt[idx] = [q[0] as f32, q[1] as f32, q[2] as f32];
                }
            }
            ".pt" => {
                let n = nums(&vals);
                let (a, _) = range(attr).unwrap_or((0, 0));
                for (k, q) in n.chunks_exact(3).enumerate() {
                    node.pt.push((a + k, [q[0] as f32, q[1] as f32, q[2] as f32]));
                }
            }
            ".ed" => {
                let n = nums(&vals);
                let (a, _) = range(attr).unwrap_or((node.ed.len(), 0));
                for (k, q) in n.chunks_exact(3).enumerate() {
                    let idx = a + k;
                    if node.ed.len() <= idx {
                        node.ed.resize(idx + 1, [0, 0]);
                    }
                    node.ed[idx] = [q[0] as i64, q[1] as i64];
                }
            }
            ".fc" => {
                // Liste de « f n e1 e2 … » (puis mu, mc : UV et couleurs, ignorés)
                let mut k = 0;
                while k < vals.len() {
                    if vals[k] == "f" && k + 1 < vals.len() {
                        let n: usize = vals[k + 1].parse().unwrap_or(0);
                        let e: Vec<i64> = vals[k + 2..(k + 2 + n).min(vals.len())].iter().filter_map(|x| x.parse().ok()).collect();
                        node.fc.push(e);
                        k += 2 + n;
                    } else {
                        k += 1;
                    }
                }
            }
            ".t" => {
                let n = nums(&vals);
                if n.len() >= 3 {
                    node.t = [n[0] as f32, n[1] as f32, n[2] as f32];
                }
            }
            ".r" => {
                let n = nums(&vals);
                if n.len() >= 3 {
                    node.r = [n[0] as f32, n[1] as f32, n[2] as f32];
                }
            }
            ".s" => {
                let n = nums(&vals);
                if n.len() >= 3 {
                    node.s = Some([n[0] as f32, n[1] as f32, n[2] as f32]);
                }
            }
            ".io" => node.intermediate = vals.first().is_some_and(|v| *v == "yes"),
            _ => {}
        }
    }
    let by_name: HashMap<String, usize> = nodes.iter().enumerate().map(|(i, (n, _))| (n.clone(), i)).collect();
    // Place d'un point par la chaîne des transformations (échelle, rotation X puis Y puis Z, translation)
    let place = |mut p: V3, start: &str| -> V3 {
        let mut parent = start.to_string();
        for _ in 0..64 {
            let Some(&i) = by_name.get(&parent) else { break };
            let n = &nodes[i].1;
            if n.kind != "transform" && n.kind != "joint" {
                break;
            }
            let s = n.s.unwrap_or([1.0; 3]);
            p = [p[0] * s[0], p[1] * s[1], p[2] * s[2]];
            let [rx, ry, rz] = n.r.map(f32::to_radians);
            p = [p[0], p[1] * rx.cos() - p[2] * rx.sin(), p[1] * rx.sin() + p[2] * rx.cos()];
            p = [p[0] * ry.cos() + p[2] * ry.sin(), p[1], -p[0] * ry.sin() + p[2] * ry.cos()];
            p = [p[0] * rz.cos() - p[1] * rz.sin(), p[0] * rz.sin() + p[1] * rz.cos(), p[2]];
            p = [p[0] + n.t[0], p[1] + n.t[1], p[2] + n.t[2]];
            parent = n.parent.clone();
        }
        p
    };
    let mut positions: Vec<V3> = vec![];
    let mut polys: Vec<Vec<u32>> = vec![];
    for (_, n) in &nodes {
        if n.kind != "mesh" || n.intermediate || n.fc.is_empty() {
            continue;
        }
        let mut vt = n.vt.clone();
        for &(i, d) in &n.pt {
            if let Some(v) = vt.get_mut(i) {
                *v = [v[0] + d[0], v[1] + d[1], v[2] + d[2]];
            }
        }
        let base = positions.len() as u32;
        positions.extend(vt.iter().map(|v| place(*v, &n.parent)));
        for f in &n.fc {
            // Une arête négative est parcourue à l'envers : -(e+1)
            let poly: Option<Vec<u32>> = f
                .iter()
                .map(|&e| {
                    let (idx, rev) = if e < 0 { ((-e - 1) as usize, true) } else { (e as usize, false) };
                    let ed = n.ed.get(idx)?;
                    Some(base + if rev { ed[1] } else { ed[0] } as u32)
                })
                .collect();
            if let Some(p) = poly.filter(|p| p.len() >= 3) {
                polys.push(p);
            }
        }
    }
    let nv = positions.len() as u32;
    polys.retain(|p| p.iter().all(|&v| v < nv));
    if polys.is_empty() {
        return Err("Aucun maillage lisible dans ce fichier Maya.".into());
    }
    from_polys(positions, &polys, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maya_ascii_cube() {
        // Un plan de 2 quads, déplacé de 5 en X par son transform
        let ma = r#"
createNode transform -n "pPlane1";
	setAttr ".t" -type "double3" 5 0 0 ;
createNode mesh -n "pPlaneShape1" -p "pPlane1";
	setAttr -s 6 ".vt[0:5]"  -1 0 1  0 0 1  1 0 1  -1 0 -1  0 0 -1  1 0 -1;
	setAttr -s 7 ".ed[0:6]"  0 1 0  1 2 0  3 4 0  4 5 0  0 3 0  1 4 0  2 5 0;
	setAttr -s 2 -ch 8 ".fc[0:1]" -type "polyFaces"
		f 4 0 5 -3 -5
		mu 0 4 0 1 4 3
		f 4 1 6 -4 -6
		mu 0 4 1 2 5 4;
"#;
        let m = load_ma(ma).unwrap();
        assert_eq!(m.total_tris, 4);
        assert!((m.min[0] - 4.0).abs() < 1e-4 && (m.max[0] - 6.0).abs() < 1e-4, "{:?} {:?}", m.min, m.max);
    }

    #[test]
    fn ply_ascii() {
        let ply = b"ply\nformat ascii 1.0\nelement vertex 4\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0\n1 0 0\n1 1 0\n0 1 0\n4 0 1 2 3\n";
        let m = load_ply(ply).unwrap();
        assert_eq!(m.total_tris, 2);
    }

    #[test]
    fn trois_ds() {
        // Un triangle : MAIN > EDIT > OBJECT "t" > TRIMESH > VERTEXL + FACEL
        let mut v = vec![];
        let chunk = |id: u16, body: Vec<u8>| -> Vec<u8> {
            let mut c = id.to_le_bytes().to_vec();
            c.extend(((body.len() + 6) as u32).to_le_bytes());
            c.extend(body);
            c
        };
        let mut verts = 3u16.to_le_bytes().to_vec();
        for p in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for x in p {
                verts.extend(x.to_le_bytes());
            }
        }
        let mut faces = 1u16.to_le_bytes().to_vec();
        for x in [0u16, 1, 2, 0] {
            faces.extend(x.to_le_bytes());
        }
        let mut tri = chunk(0x4110, verts);
        tri.extend(chunk(0x4120, faces));
        let mut obj = b"t\0".to_vec();
        obj.extend(chunk(0x4100, tri));
        v.extend(chunk(0x4D4D, chunk(0x3D3D, chunk(0x4000, obj))));
        let m = load_3ds(&v).unwrap();
        assert_eq!(m.total_tris, 1);
    }
}
