//! Lecture et écriture de fichiers Photoshop (.psd), 8 bits RVB : calques, masques de fusion, groupes,
//! opacité, fond, modes de fusion, visibilité, écrêtage et noms (Unicode). Écrit à partir de la
//! spécification publique du format ; aucune bibliothèque.

use crate::paint::{Adj, Blend, Doc, Group, Layer};

const BLENDS: [(Blend, &[u8; 4]); 23] = [
    (Blend::Normal, b"norm"),
    (Blend::Darken, b"dark"),
    (Blend::Multiply, b"mul "),
    (Blend::ColorBurn, b"idiv"),
    (Blend::LinearBurn, b"lbrn"),
    (Blend::Lighten, b"lite"),
    (Blend::Screen, b"scrn"),
    (Blend::ColorDodge, b"div "),
    (Blend::Add, b"lddg"),
    (Blend::Overlay, b"over"),
    (Blend::SoftLight, b"sLit"),
    (Blend::HardLight, b"hLit"),
    (Blend::VividLight, b"vLit"),
    (Blend::LinearLight, b"lLit"),
    (Blend::PinLight, b"pLit"),
    (Blend::Difference, b"diff"),
    (Blend::Exclusion, b"smud"),
    (Blend::Subtract, b"fsub"),
    (Blend::Divide, b"fdiv"),
    (Blend::Hue, b"hue "),
    (Blend::Saturation, b"sat "),
    (Blend::Color, b"colr"),
    (Blend::Luminosity, b"lum "),
];

fn blend_key(b: Blend) -> [u8; 4] {
    *BLENDS.iter().find(|x| x.0 == b).map(|x| x.1).unwrap_or(b"norm")
}

fn blend_of(k: &[u8]) -> Option<Blend> {
    BLENDS.iter().find(|x| x.1 == k).map(|x| x.0)
}

// ---------- Calques de réglage ----------

const HUE_RANGES: [[i16; 4]; 6] = [[315, 345, 15, 45], [15, 45, 75, 105], [75, 105, 135, 165], [135, 165, 195, 225], [195, 225, 255, 285], [255, 285, 315, 345]];

/// Le bloc Photoshop d'un réglage (clé, données) ; None pour ceux pas encore pris en charge
fn adj_block(a: &Adj) -> Option<([u8; 4], Vec<u8>)> {
    let mut w = W(vec![]);
    let key = match *a {
        Adj::Invert => *b"nvrt",
        Adj::Threshold { t } => {
            w.u16((t * 255.0).round().clamp(1.0, 255.0) as u16);
            w.u16(0);
            *b"thrs"
        }
        Adj::Posterize { levels } => {
            w.u16(levels.round().clamp(2.0, 255.0) as u16);
            w.u16(0);
            *b"post"
        }
        Adj::BrightContrast { b, c } => {
            w.u16(((b * 150.0).round().clamp(-150.0, 150.0) as i16) as u16);
            w.u16(((c * 100.0).round().clamp(-50.0, 100.0) as i16) as u16);
            w.u16(127);
            w.u8(0);
            w.u8(0);
            *b"brit"
        }
        Adj::Exposure { ev } => {
            w.u16(1);
            w.0.extend(ev.to_be_bytes());
            w.0.extend(0f32.to_be_bytes());
            w.0.extend(1f32.to_be_bytes());
            *b"expA"
        }
        Adj::HueSat { h, s, l } => {
            w.u16(2);
            w.u8(0);
            w.u8(0);
            for _ in 0..3 {
                w.u16(0);
            }
            for v in [h * 360.0, s * 100.0, l * 100.0] {
                w.u16((v.round().clamp(-180.0, 180.0) as i16) as u16);
            }
            for r in HUE_RANGES {
                for v in r {
                    w.u16(v as u16);
                }
                for _ in 0..3 {
                    w.u16(0);
                }
            }
            *b"hue2"
        }
        Adj::Levels { black, white, gamma } => {
            w.u16(2);
            for k in 0..29 {
                let (lo, hi, g) = if k == 0 { ((black * 255.0).round() as u16, (white * 255.0).round() as u16, (gamma * 100.0).round() as u16) } else { (0, 255, 100) };
                for v in [lo, hi, 0, 255, g] {
                    w.u16(v);
                }
            }
            *b"levl"
        }
        Adj::BlackWhite => {
            // Réglage « Noir et blanc » de Photoshop, curseurs par défaut (un descripteur)
            let o = |c: &str, items: Vec<(&str, Val)>| Val::Obj(c.into(), items.into_iter().map(|(k, v)| (k.to_string(), v)).collect());
            let d = o(
                "null",
                vec![
                    ("Rd  ", Val::Long(40)),
                    ("Yllw", Val::Long(60)),
                    ("Grn ", Val::Long(40)),
                    ("Cyn ", Val::Long(60)),
                    ("Bl  ", Val::Long(20)),
                    ("Mgnt", Val::Long(80)),
                    ("useTint", Val::Bool(false)),
                    ("tintColor", o("RGBC", vec![("Rd  ", Val::Num(225.0)), ("Grn ", Val::Num(211.0)), ("Bl  ", Val::Num(179.0))])),
                    ("bwPresetKind", Val::Long(1)),
                    ("blackAndWhitePresetFileName", Val::Text(String::new())),
                ],
            );
            w.u32(16);
            put_desc(&mut w, &d);
            *b"blwh"
        }
    };
    Some((key, w.0))
}

fn adj_of(key: &[u8], d: &[u8]) -> Option<Adj> {
    let u = |i: usize| d.get(i..i + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let i = |k: usize| u(k).map(|v| v as i16 as f32);
    let f = |k: usize| d.get(k..k + 4).map(|b| f32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    Some(match key {
        b"nvrt" => Adj::Invert,
        b"thrs" => Adj::Threshold { t: u(0)? as f32 / 255.0 },
        b"post" => Adj::Posterize { levels: u(0)? as f32 },
        b"brit" => Adj::BrightContrast { b: i(0)? / 150.0, c: i(2)? / 100.0 },
        b"expA" => Adj::Exposure { ev: f(2)? },
        b"hue2" => Adj::HueSat { h: i(10)? / 360.0, s: i(12)? / 100.0, l: i(14)? / 100.0 },
        b"blwh" => Adj::BlackWhite,
        b"levl" => Adj::Levels { black: u(2)? as f32 / 255.0, white: u(4)? as f32 / 255.0, gamma: u(10)? as f32 / 100.0 },
        _ => return None,
    })
}

// ---------- Lecture ----------

struct R<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let s = self.b.get(self.at..self.at + n).ok_or("fichier PSD coupé")?;
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        let s = self.take(2)?;
        Ok(u16::from_be_bytes([s[0], s[1]]))
    }
    fn i16(&mut self) -> Result<i16, String> {
        Ok(self.u16()? as i16)
    }
    fn u32(&mut self) -> Result<u32, String> {
        let s = self.take(4)?;
        Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }
}

/// Un canal en 8 bits ; en 16 bits, on garde l'octet de poids fort
fn read_channel(r: &mut R, w: usize, h: usize, bps: usize) -> Result<Vec<u8>, String> {
    let raw = read_channel8(r, w * bps, h)?;
    Ok(if bps == 2 { raw.chunks_exact(2).map(|c| c[0]).collect() } else { raw })
}

/// Un canal : brut (0) ou PackBits ligne par ligne (1)
fn read_channel8(r: &mut R, w: usize, h: usize) -> Result<Vec<u8>, String> {
    let comp = r.u16()?;
    match comp {
        0 => Ok(r.take(w * h)?.to_vec()),
        1 => {
            let mut counts = Vec::with_capacity(h);
            for _ in 0..h {
                counts.push(r.u16()? as usize);
            }
            let mut out = Vec::with_capacity(w * h);
            for c in counts {
                let row = r.take(c)?;
                let start = out.len();
                let mut i = 0;
                while i < row.len() && out.len() - start < w {
                    let n = row[i] as i8;
                    i += 1;
                    if n >= 0 {
                        let k = n as usize + 1;
                        out.extend_from_slice(row.get(i..i + k).ok_or("PackBits invalide")?);
                        i += k;
                    } else if n != -128 {
                        let k = (1 - n as i32) as usize;
                        let v = *row.get(i).ok_or("PackBits invalide")?;
                        out.extend(std::iter::repeat_n(v, k));
                        i += 1;
                    }
                }
                out.resize(start + w, 0);
            }
            Ok(out)
        }
        _ => Err("compression PSD non prise en charge (ZIP)".into()),
    }
}

struct Rec {
    rect: [i32; 4],
    chans: Vec<(i16, u32)>,
    blend: Blend,
    opacity: u8,
    clip: bool,
    hidden: bool,
    name: String,
    mask: Option<([i32; 4], u8, bool)>,
    section: u32,
    pass: bool,
    fill: u8,
    adj: Option<Adj>,
    fx: Option<crate::paint::Fx>,
}

pub fn read(bytes: &[u8]) -> Result<Doc, String> {
    let mut r = R { b: bytes, at: 0 };
    if r.take(4)? != b"8BPS" || r.u16()? != 1 {
        return Err("Ce n'est pas un fichier Photoshop (.psd).".into());
    }
    r.take(6)?;
    let nch = r.u16()? as usize;
    let h = r.u32()? as usize;
    let w = r.u32()? as usize;
    let depth = r.u16()?;
    let mode = r.u16()?;
    if !(depth == 8 || depth == 16) || mode != 3 {
        return Err("Seuls les PSD RVB 8 ou 16 bits sont pris en charge pour l'instant.".into());
    }
    let bps = depth as usize / 8;
    let n = r.u32()? as usize;
    r.take(n)?;
    let n = r.u32()? as usize;
    r.take(n)?;
    let lm_len = r.u32()? as usize;
    let lm_end = r.at + lm_len;
    let mut layers: Vec<Layer> = vec![];
    let mut groups: Vec<Group> = vec![];
    if lm_len > 0 {
        let li_len = r.u32()? as usize;
        if li_len > 0 {
            let count = r.i16()?.unsigned_abs() as usize;
            let mut recs = vec![];
            for _ in 0..count {
                let rect = [r.i32()?, r.i32()?, r.i32()?, r.i32()?];
                let nc = r.u16()? as usize;
                let mut chans = vec![];
                for _ in 0..nc {
                    chans.push((r.i16()?, r.u32()?));
                }
                r.take(4)?;
                let key = r.take(4)?.to_vec();
                let opacity = r.u8()?;
                let clip = r.u8()? != 0;
                let flags = r.u8()?;
                r.u8()?;
                let extra = r.u32()? as usize;
                let end = r.at + extra;
                let ml = r.u32()? as usize;
                let mask = if ml >= 20 {
                    let mr = [r.i32()?, r.i32()?, r.i32()?, r.i32()?];
                    let def = r.u8()?;
                    let mf = r.u8()?;
                    r.take(ml - 18)?;
                    Some((mr, def, mf & 2 != 0))
                } else {
                    r.take(ml)?;
                    None
                };
                let bl = r.u32()? as usize;
                r.take(bl)?;
                let nl = r.u8()? as usize;
                let mut name = String::from_utf8_lossy(r.take(nl)?).to_string();
                let pad = (4 - (nl + 1) % 4) % 4;
                r.take(pad)?;
                let (mut section, mut pass, mut fill) = (0, false, 255u8);
                let mut adj = None;
                let mut fx = None;
                while r.at + 12 <= end {
                    let sig = r.take(4)?;
                    if sig != b"8BIM" && sig != b"8B64" {
                        break;
                    }
                    let k = r.take(4)?.to_vec();
                    let len = r.u32()? as usize;
                    let data_end = r.at + len;
                    match &k[..] {
                        b"luni" => {
                            let c = r.u32()? as usize;
                            let units: Vec<u16> = (0..c).map(|_| r.u16()).collect::<Result<_, _>>()?;
                            name = String::from_utf16_lossy(&units).trim_end_matches('\0').to_string();
                        }
                        b"lsct" | b"lsdk" => {
                            section = r.u32()?;
                            if len >= 12 {
                                r.take(4)?;
                                pass = r.take(4)? == b"pass";
                            }
                        }
                        b"iOpa" => fill = r.u8()?,
                        b"lfx2" | b"lmfx" => fx = fx_of(r.b.get(r.at..data_end).unwrap_or(&[])).or(fx),
                        k => {
                            if let Some(a) = adj_of(k, r.b.get(r.at..data_end).unwrap_or(&[])) {
                                adj = Some(a);
                            }
                        }
                    }
                    r.at = data_end;
                }
                r.at = end;
                recs.push(Rec { rect, chans, blend: blend_of(&key).unwrap_or_default(), opacity, clip, hidden: flags & 2 != 0, name, mask, section, pass, fill, adj, fx });
            }
            // Pixels de chaque calque, dans l'ordre des enregistrements
            let mut stack: Vec<u32> = vec![];
            let mut next_id = 1u32;
            for rec in recs {
                let [t, l, b, rr] = rec.rect;
                let (lw, lh) = ((rr - l).max(0) as usize, (b - t).max(0) as usize);
                let mut px = vec![255u8; lw * lh * 4];
                let mut mask_px: Option<Layer> = None;
                for (id, len) in &rec.chans {
                    let start = r.at;
                    match *id {
                        -2 => {
                            if let Some(([mt, ml, mb, mr], def, _)) = rec.mask {
                                let (mw, mh) = ((mr - ml).max(0) as usize, (mb - mt).max(0) as usize);
                                let data = read_channel(&mut r, mw, mh, bps)?;
                                // Masque à la taille du document : hors de son rectangle, sa couleur par défaut
                                let mut m = Layer::filled("masque", w, h, [def, def, def, 255]);
                                for y in 0..mh {
                                    for x in 0..mw {
                                        let (dx, dy) = (x as i32 + ml, y as i32 + mt);
                                        if dx >= 0 && dy >= 0 && (dx as usize) < w && (dy as usize) < h {
                                            let v = data[y * mw + x];
                                            let i = (dy as usize * w + dx as usize) * 4;
                                            m.px[i..i + 3].copy_from_slice(&[v, v, v]);
                                        }
                                    }
                                }
                                mask_px = Some(m);
                            }
                        }
                        c @ -1..=2 if lw * lh > 0 => {
                            let data = read_channel(&mut r, lw, lh, bps)?;
                            let k = if c == -1 { 3 } else { c as usize };
                            for (i, v) in data.into_iter().enumerate() {
                                px[i * 4 + k] = v;
                            }
                        }
                        _ => {}
                    }
                    r.at = start + *len as usize;
                }
                match rec.section {
                    // Fin de groupe (en bas dans la pile) : les calques suivants sont dedans
                    3 => {
                        stack.push(next_id);
                        next_id += 1;
                    }
                    1 | 2 => {
                        if let Some(id) = stack.pop() {
                            // Dossier dans un dossier : son parent est celui qui reste en haut de la pile
                            let mut g = Group { opacity: rec.opacity as f32 / 255.0, blend: if rec.pass { None } else { Some(rec.blend) }, collapsed: rec.section == 2, visible: !rec.hidden, ..Group::new(id, rec.name, *stack.last().unwrap_or(&0)) };
                            if let Some(m) = mask_px.take() {
                                g.mask_on = !rec.mask.is_some_and(|m| m.2);
                                g.has_mask = true;
                                g.mask = crate::paint::GroupMask(Some(Box::new(m)));
                            }
                            groups.push(g);
                        }
                    }
                    _ => {
                        let mut layer = Layer::blank(&rec.name, lw.max(1), lh.max(1));
                        if lw * lh > 0 {
                            layer.px = px;
                        } else {
                            layer.px = vec![0; 4];
                        }
                        layer.off = [l, t];
                        layer.visible = !rec.hidden;
                        layer.opacity = rec.opacity as f32 / 255.0;
                        layer.fill = rec.fill as f32 / 255.0;
                        layer.blend = rec.blend;
                        layer.clip = rec.clip;
                        layer.group = *stack.last().unwrap_or(&0);
                        if let Some(fx) = rec.fx.clone() {
                            layer.fx = fx;
                        }
                        if rec.adj.is_some() {
                            layer.adj = rec.adj;
                            layer.px = vec![0; 4];
                            (layer.w, layer.h, layer.off) = (1, 1, [0, 0]);
                        }
                        if let Some(m) = mask_px {
                            layer.mask_on = !rec.mask.is_some_and(|m| m.2);
                            layer.mask = Some(Box::new(m));
                        }
                        layers.push(layer);
                    }
                }
            }
        }
    }
    r.at = lm_end.min(bytes.len());
    if layers.is_empty() {
        // Sans calques : l'image fusionnée
        let comp = r.u16()?;
        let mut px = vec![255u8; w * h * 4];
        let planes = if comp == 1 {
            let counts: Vec<usize> = (0..h * nch).map(|_| r.u16().map(|v| v as usize)).collect::<Result<_, _>>()?;
            let mut out = vec![];
            for c in 0..nch {
                let mut plane = vec![];
                for row in 0..h {
                    let data = r.take(counts[c * h + row])?;
                    let row = unpack(data, w * bps);
                    plane.extend(if bps == 2 { row.chunks_exact(2).map(|c| c[0]).collect() } else { row });
                }
                out.push(plane);
            }
            out
        } else {
            (0..nch).map(|_| r.take(w * h * bps).map(|s| if bps == 2 { s.chunks_exact(2).map(|c| c[0]).collect() } else { s.to_vec() })).collect::<Result<_, _>>()?
        };
        for (c, plane) in planes.iter().enumerate().take(4) {
            for (i, v) in plane.iter().enumerate().take(w * h) {
                px[i * 4 + c] = *v;
            }
        }
        layers.push(Layer { px, ..Layer::blank("calque 1", w, h) });
    }
    let n = layers.len();
    Ok(Doc { w, h, layers, active: n - 1, groups })
}

fn unpack(row: &[u8], w: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(w);
    let mut i = 0;
    while i < row.len() && out.len() < w {
        let n = row[i] as i8;
        i += 1;
        if n >= 0 {
            let k = n as usize + 1;
            out.extend_from_slice(&row[i..(i + k).min(row.len())]);
            i += k;
        } else if n != -128 {
            let k = (1 - n as i32) as usize;
            if let Some(v) = row.get(i) {
                out.extend(std::iter::repeat_n(*v, k));
            }
            i += 1;
        }
    }
    out.resize(w, 0);
    out
}

// ---------- Écriture ----------

struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend(v.to_be_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend(v.to_be_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.0.extend(v.to_be_bytes());
    }
    fn bytes(&mut self, b: &[u8]) {
        self.0.extend_from_slice(b);
    }
}

/// Un calque à écrire : rectangle, canaux (identifiant, données brutes), réglages
struct Out {
    rect: [i32; 4],
    chans: Vec<(i16, Vec<u8>)>,
    blend: Blend,
    opacity: u8,
    fill: u8,
    clip: bool,
    hidden: bool,
    name: String,
    mask: Option<([i32; 4], bool)>,
    section: Option<(u32, bool, Blend)>,
    adj: Option<([u8; 4], Vec<u8>)>,
    fx: Option<Vec<u8>>,
}

fn record(w: &mut W, o: &Out) {
    for v in o.rect {
        w.i32(v);
    }
    w.u16(o.chans.len() as u16);
    for (id, data) in &o.chans {
        w.u16(*id as u16);
        w.u32(data.len() as u32 + 2);
    }
    w.bytes(b"8BIM");
    w.bytes(&blend_key(o.blend));
    w.u8(o.opacity);
    w.u8(o.clip as u8);
    w.u8(if o.hidden { 2 } else { 0 } | 8);
    w.u8(0);
    let mut extra = W(vec![]);
    match o.mask {
        Some((r, disabled)) => {
            extra.u32(20);
            for v in r {
                extra.i32(v);
            }
            extra.u8(255);
            extra.u8(if disabled { 2 } else { 0 });
            extra.u16(0);
        }
        None => extra.u32(0),
    }
    extra.u32(0);
    // Nom en ASCII (Pascal, aligné sur 4), puis en Unicode
    let ascii: Vec<u8> = o.name.chars().map(|c| if c.is_ascii() { c as u8 } else { b'_' }).take(255).collect();
    extra.u8(ascii.len() as u8);
    extra.bytes(&ascii);
    for _ in 0..(4 - (ascii.len() + 1) % 4) % 4 {
        extra.u8(0);
    }
    let units: Vec<u16> = o.name.encode_utf16().collect();
    extra.bytes(b"8BIMluni");
    let len = 4 + units.len() * 2;
    let padded = len + len % 2;
    extra.u32(padded as u32);
    extra.u32(units.len() as u32);
    for u in &units {
        extra.u16(*u);
    }
    if len % 2 == 1 {
        extra.u8(0);
    }
    extra.bytes(b"8BIMiOpa");
    extra.u32(4);
    extra.u8(o.fill);
    extra.bytes(&[0, 0, 0]);
    if let Some((key, data)) = &o.adj {
        extra.bytes(b"8BIM");
        extra.bytes(key);
        extra.u32((data.len() + data.len() % 2) as u32);
        extra.bytes(data);
        if data.len() % 2 == 1 {
            extra.u8(0);
        }
    }
    if let Some(data) = &o.fx {
        extra.bytes(b"8BIMlfx2");
        extra.u32((data.len() + data.len() % 2) as u32);
        extra.bytes(data);
        if data.len() % 2 == 1 {
            extra.u8(0);
        }
    }
    if let Some((kind, pass, b)) = o.section {
        extra.bytes(b"8BIMlsct");
        extra.u32(12);
        extra.u32(kind);
        extra.bytes(b"8BIM");
        let key = if pass { *b"pass" } else { blend_key(b) };
        extra.bytes(&key);
    }
    w.u32(extra.0.len() as u32);
    w.bytes(&extra.0);
}

/// Écrit le document en PSD. Rend aussi ce qui n'a pas pu passer (calques de réglage, styles).
pub fn write(doc: &Doc) -> (Vec<u8>, Vec<String>) {
    let (dw, dh) = (doc.w, doc.h);
    let mut lost = vec![];
    let mut outs: Vec<Out> = vec![];
    let group = |id: u32| doc.groups.iter().find(|g| g.id == id);
    let mut i = 0;
    let to_out = |l: &Layer| -> Out {
        let n = l.w * l.h;
        let plane = |k: usize| (0..n).map(|p| l.px[p * 4 + k]).collect::<Vec<u8>>();
        let mut chans = vec![(-1i16, plane(3)), (0, plane(0)), (1, plane(1)), (2, plane(2))];
        let mask = l.mask.as_deref().map(|m| {
            chans.push((-2, (0..m.w * m.h).map(|p| m.px[p * 4]).collect()));
            ([m.off[1], m.off[0], m.off[1] + m.h as i32, m.off[0] + m.w as i32], !l.mask_on)
        });
        Out {
            rect: [l.off[1], l.off[0], l.off[1] + l.h as i32, l.off[0] + l.w as i32],
            chans,
            blend: l.blend,
            opacity: (l.opacity * 255.0).round() as u8,
            fill: (l.fill * 255.0).round() as u8,
            clip: l.clip,
            hidden: !l.visible,
            name: l.name.clone(),
            mask,
            section: None,
            adj: None,
            fx: l.fx.active().then(|| fx_block(&l.fx)),
        }
    };
    let empty = |name: &str, section: (u32, bool, Blend), hidden: bool, opacity: u8| Out { rect: [0; 4], chans: vec![(-1, vec![]), (0, vec![]), (1, vec![]), (2, vec![])], blend: section.2, opacity, fill: 255, clip: false, hidden, name: name.to_string(), mask: None, section: Some(section), adj: None, fx: None };
    // En-tête de dossier (au-dessus de son contenu), avec son masque s'il en a un
    let header = |g: &crate::paint::Group| -> Out {
        let mut o = empty(&g.name, (if g.collapsed { 2 } else { 1 }, g.blend.is_none(), g.blend.unwrap_or_default()), !g.visible, (g.opacity * 255.0).round() as u8);
        if let Some(m) = g.mask.0.as_deref() {
            o.chans.push((-2, (0..m.w * m.h).map(|p| m.px[p * 4]).collect()));
            o.mask = Some(([m.off[1], m.off[0], m.off[1] + m.h as i32, m.off[0] + m.w as i32], !g.mask_on));
        }
        o
    };
    // Les dossiers ouverts au fil des calques (de bas en haut) : on ferme ceux qu'on quitte, on ouvre les nouveaux
    let mut open: Vec<u32> = vec![];
    while i < doc.layers.len() {
        let l = &doc.layers[i];
        let chain: Vec<u32> = doc.chain(l.group).into_iter().filter(|&g| group(g).is_some()).collect();
        let keep = open.iter().zip(chain.iter()).take_while(|(a, b)| a == b).count();
        while open.len() > keep {
            let g = open.pop().unwrap_or(0);
            if let Some(g) = group(g) {
                outs.push(header(g));
            }
        }
        for g in &chain[keep..] {
            outs.push(empty("</Layer group>", (3, false, Blend::Normal), false, 255));
            open.push(*g);
        }
        if let Some(a) = &l.adj {
            match adj_block(a) {
                Some(block) => {
                    // Calque de réglage : pas de pixels, son masque et son bloc de réglage
                    let mut o = to_out(l);
                    o.rect = [0; 4];
                    for c in o.chans.iter_mut().filter(|c| c.0 != -2) {
                        c.1.clear();
                    }
                    o.adj = Some(block);
                    outs.push(o);
                }
                None => lost.push(format!("{} ({})", l.name, crate::i18n::t("calque de réglage"))),
            }
        } else {
            outs.push(to_out(l));
        }
        i += 1;
    }
    while let Some(g) = open.pop() {
        if let Some(g) = group(g) {
            outs.push(header(g));
        }
    }
    let mut w = W(vec![]);
    w.bytes(b"8BPS");
    w.u16(1);
    w.bytes(&[0; 6]);
    w.u16(4);
    w.u32(dh as u32);
    w.u32(dw as u32);
    w.u16(8);
    w.u16(3);
    w.u32(0);
    w.u32(0);
    // Calques
    let mut li = W(vec![]);
    li.u16((-(outs.len() as i16)) as u16);
    for o in &outs {
        record(&mut li, o);
    }
    for o in &outs {
        for (_, data) in &o.chans {
            li.u16(0);
            li.bytes(data);
        }
    }
    if li.0.len() % 2 == 1 {
        li.u8(0);
    }
    w.u32(li.0.len() as u32 + 8);
    w.u32(li.0.len() as u32);
    w.bytes(&li.0);
    w.u32(0);
    // Image fusionnée (pour les logiciels qui ne lisent pas les calques)
    let flat = doc.flatten();
    w.u16(0);
    for c in 0..4 {
        for p in flat.pixels() {
            w.u8(p.0[c]);
        }
    }
    (w.0, lost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_retour_psd() {
        let mut d = Doc::new(6, 4, true);
        let mut top = Layer::filled("Rouge été", 3, 2, [255, 0, 0, 255]);
        top.off = [1, 1];
        top.opacity = 0.5;
        top.blend = Blend::Multiply;
        top.mask = Some(Layer::new_mask(6, 4, 255));
        top.mask_on = false;
        top.group = 3;
        top.fx = crate::paint::Fx {
            on: true,
            shadow: Some(crate::paint::FxShadow { dx: 3.0, dy: 4.0, blur: 6.0, color: [10, 20, 30], opacity: 0.6 }),
            glow: Some(crate::paint::FxGlow { size: 9.0, color: [255, 236, 170], opacity: 0.75 }),
            overlay: Some(crate::paint::FxOverlay { color: [220, 60, 60], opacity: 0.5 }),
            stroke: Some(crate::paint::FxStroke { size: 2.0, color: [0, 0, 255], opacity: 1.0 }),
        };
        d.layers.push(top);
        d.groups.push(Group::new(3, "Dossier".into(), 0));
        d.layers.push(Layer { adj: Some(Adj::HueSat { h: 0.25, s: 0.5, l: 0.0 }), mask: Some(Layer::new_mask(6, 4, 255)), ..Layer::blank("Teinte", 1, 1) });
        d.layers.push(Layer { adj: Some(Adj::Levels { black: 0.1, white: 0.9, gamma: 1.2 }), ..Layer::blank("Niveaux", 1, 1) });
        let (nb, _) = write(&Doc { layers: vec![Layer { adj: Some(Adj::BlackWhite), ..Layer::blank("NB", 1, 1) }], ..Doc::new(6, 4, true) });
        assert!(read(&nb).unwrap().layers.iter().any(|l| l.adj == Some(Adj::BlackWhite)));
        if let Ok(p) = std::env::var("PSD_OUT_NB") {
            std::fs::write(p, &nb).unwrap();
        }
        let (bytes, lost) = write(&d);
        assert!(lost.is_empty(), "{lost:?}");
        d.layers.truncate(2);
        let mut back = read(&bytes).unwrap();
        assert_eq!((back.w, back.h, back.layers.len()), (6, 4, 4));
        match back.layers[2].adj {
            Some(Adj::HueSat { h, s, .. }) => assert!((h - 0.25).abs() < 0.01 && (s - 0.5).abs() < 0.01),
            ref o => panic!("{o:?}"),
        }
        assert!(back.layers[2].mask.is_some());
        assert_eq!(back.layers[3].adj, Some(Adj::Levels { black: 26.0 / 255.0, white: 230.0 / 255.0, gamma: 1.2 }));
        back.layers.truncate(2);
        let l = &back.layers[1];
        assert_eq!(l.name, "Rouge été");
        assert_eq!((l.off, l.w, l.h), ([1, 1], 3, 2));
        assert_eq!(l.blend, Blend::Multiply);
        assert!((l.opacity - 0.5).abs() < 0.01);
        assert!(l.mask.is_some() && !l.mask_on);
        // Styles : l'ombre passe par un angle et une distance, et revient au même décalage
        let (fx, want) = (&l.fx, &d.layers[1].fx);
        assert!(fx.on);
        let (s, w) = (fx.shadow.unwrap(), want.shadow.unwrap());
        assert!((s.dx - w.dx).abs() < 1e-3 && (s.dy - w.dy).abs() < 1e-3 && s.blur == w.blur && s.color == w.color && (s.opacity - w.opacity).abs() < 1e-4);
        assert_eq!(fx.glow, want.glow);
        assert_eq!(fx.overlay, want.overlay);
        assert_eq!(fx.stroke, want.stroke);
        assert_eq!(back.groups.len(), 1);
        assert_eq!(back.groups[0].name, "Dossier");
        assert_eq!(l.group, back.groups[0].id);
        let (a, b) = (back.flatten().get_pixel(2, 2).0, d.flatten().get_pixel(2, 2).0);
        assert!(a.iter().zip(b.iter()).all(|(x, y)| (*x as i32 - *y as i32).abs() <= 1), "{a:?} {b:?}");
        // Pour vérifier avec un autre logiciel
        if let Ok(p) = std::env::var("PSD_OUT") {
            std::fs::write(p, &bytes).unwrap();
        }
    }
}

#[cfg(test)]
mod im_tests {
    #[test]
    fn lit_un_psd_d_imagemagick() {
        let Ok(b) = std::fs::read("/tmp/im.psd") else { return };
        let d = super::read(&b).unwrap();
        assert_eq!(d.layers.len(), 3);
        let img = d.flatten();
        assert_eq!(img.get_pixel(10, 8).0, [0, 0, 255, 255]);
        assert_eq!(img.get_pixel(25, 20).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(1, 1).0, [255, 255, 255, 255]);
    }
}

// ------------------------------------------------------------------ styles de calque (lfx2)

/// Une valeur de descripteur Photoshop (le sous-ensemble des styles de calque)
#[derive(Clone, Debug, PartialEq)]
enum Val {
    Obj(String, Vec<(String, Val)>),
    Num(f64),
    Unit(String, f64),
    Bool(bool),
    Enum(String, String),
    Long(i32),
    Text(String),
    List(Vec<Val>),
}

impl Val {
    fn get(&self, key: &str) -> Option<&Val> {
        match self {
            Val::Obj(_, items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn num(&self, key: &str) -> Option<f64> {
        match self.get(key)? {
            Val::Num(v) | Val::Unit(_, v) => Some(*v),
            Val::Long(v) => Some(*v as f64),
            _ => None,
        }
    }

    fn on(&self) -> bool {
        !matches!(self.get("enab"), Some(Val::Bool(false)))
    }

    fn color(&self) -> [u8; 3] {
        let c = self.get("Clr ");
        let ch = |k: &str| c.and_then(|c| c.num(k)).unwrap_or(0.0).round().clamp(0.0, 255.0) as u8;
        [ch("Rd  "), ch("Grn "), ch("Bl  ")]
    }
}

/// Un identifiant de descripteur : longueur puis texte, ou 0 puis quatre caractères
fn read_id(r: &mut R) -> Result<String, String> {
    let n = r.u32()? as usize;
    let n = if n == 0 { 4 } else { n };
    Ok(String::from_utf8_lossy(r.take(n)?).to_string())
}

fn read_ustr(r: &mut R) -> Result<String, String> {
    let n = r.u32()? as usize;
    let u: Vec<u16> = (0..n).map(|_| r.u16()).collect::<Result<_, _>>()?;
    Ok(String::from_utf16_lossy(&u).trim_end_matches('\0').to_string())
}

fn read_f64(r: &mut R) -> Result<f64, String> {
    let b = r.take(8)?;
    Ok(f64::from_be_bytes(b.try_into().map_err(|_| "f64")?))
}

fn read_val(r: &mut R, ty: &[u8], depth: u32) -> Result<Val, String> {
    if depth > 16 {
        return Err("descripteur trop profond".into());
    }
    Ok(match ty {
        b"Objc" | b"GlbO" => read_desc(r, depth + 1)?,
        b"doub" => Val::Num(read_f64(r)?),
        b"UntF" => {
            let u = String::from_utf8_lossy(r.take(4)?).to_string();
            Val::Unit(u, read_f64(r)?)
        }
        b"bool" => Val::Bool(r.u8()? != 0),
        b"enum" => {
            let t = read_id(r)?;
            Val::Enum(t, read_id(r)?)
        }
        b"long" => Val::Long(r.i32()?),
        b"TEXT" => Val::Text(read_ustr(r)?),
        b"VlLs" => {
            let n = r.u32()? as usize;
            let mut v = Vec::with_capacity(n.min(256));
            for _ in 0..n {
                let t = r.take(4)?.to_vec();
                v.push(read_val(r, &t, depth + 1)?);
            }
            Val::List(v)
        }
        b"UnFl" => {
            r.take(4)?;
            let n = r.u32()? as usize;
            r.take(n.checked_mul(8).ok_or("UnFl")?)?;
            Val::List(vec![])
        }
        b"tdta" | b"alis" => {
            let n = r.u32()? as usize;
            r.take(n)?;
            Val::Text(String::new())
        }
        t => return Err(format!("type de descripteur inconnu {}", String::from_utf8_lossy(t))),
    })
}

fn read_desc(r: &mut R, depth: u32) -> Result<Val, String> {
    let _name = read_ustr(r)?;
    let class = read_id(r)?;
    let n = r.u32()? as usize;
    let mut items = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        let k = read_id(r)?;
        let t = r.take(4)?.to_vec();
        items.push((k, read_val(r, &t, depth)?));
    }
    Ok(Val::Obj(class, items))
}

/// Les styles d'un bloc lfx2 (ou lmfx) : ombre portée, lueur externe, incrustation de couleur, contour
fn fx_of(d: &[u8]) -> Option<crate::paint::Fx> {
    use crate::paint::{Fx, FxGlow, FxOverlay, FxShadow, FxStroke};
    let mut r = R { b: d, at: 0 };
    r.u32().ok()?;
    if r.u32().ok()? != 16 {
        return None;
    }
    let root = read_desc(&mut r, 0).ok()?;
    // Un style peut aussi venir en liste (plusieurs ombres…) : on garde la première
    let pick = |a: &str, multi: &str| -> Option<Val> {
        root.get(a).cloned().or_else(|| match root.get(multi) {
            Some(Val::List(l)) => l.first().cloned(),
            _ => None,
        })
    };
    let op = |v: &Val| (v.num("Opct").unwrap_or(100.0) / 100.0).clamp(0.0, 1.0) as f32;
    let mut fx = Fx { on: !matches!(root.get("masterFXSwitch"), Some(Val::Bool(false))), ..Default::default() };
    if let Some(v) = pick("DrSh", "dropShadowMulti").filter(|v| v.on()) {
        let a = v.num("lagl").unwrap_or(120.0).to_radians();
        let dist = v.num("Dstn").unwrap_or(5.0);
        fx.shadow = Some(FxShadow { dx: (-dist * a.cos()) as f32, dy: (dist * a.sin()) as f32, blur: v.num("blur").unwrap_or(5.0) as f32, color: v.color(), opacity: op(&v) });
    }
    if let Some(v) = pick("OrGl", "outerGlowMulti").filter(|v| v.on()) {
        fx.glow = Some(FxGlow { size: v.num("blur").unwrap_or(10.0) as f32, color: v.color(), opacity: op(&v) });
    }
    if let Some(v) = pick("SoFi", "solidFillMulti").filter(|v| v.on()) {
        fx.overlay = Some(FxOverlay { color: v.color(), opacity: op(&v) });
    }
    if let Some(v) = pick("FrFX", "frameFXMulti").filter(|v| v.on()) {
        fx.stroke = Some(FxStroke { size: v.num("Sz  ").unwrap_or(3.0) as f32, color: v.color(), opacity: op(&v) });
    }
    (fx.shadow.is_some() || fx.glow.is_some() || fx.overlay.is_some() || fx.stroke.is_some()).then_some(fx)
}

fn put_id(w: &mut W, id: &str) {
    if id.len() == 4 {
        w.u32(0);
    } else {
        w.u32(id.len() as u32);
    }
    w.bytes(id.as_bytes());
}

fn put_val(w: &mut W, v: &Val) {
    match v {
        Val::Obj(..) => {
            w.bytes(b"Objc");
            put_desc(w, v);
        }
        Val::Num(x) => {
            w.bytes(b"doub");
            w.bytes(&x.to_be_bytes());
        }
        Val::Unit(u, x) => {
            w.bytes(b"UntF");
            w.bytes(u.as_bytes());
            w.bytes(&x.to_be_bytes());
        }
        Val::Bool(b) => {
            w.bytes(b"bool");
            w.u8(*b as u8);
        }
        Val::Enum(t, e) => {
            w.bytes(b"enum");
            put_id(w, t);
            put_id(w, e);
        }
        Val::Long(x) => {
            w.bytes(b"long");
            w.i32(*x);
        }
        Val::Text(t) => {
            w.bytes(b"TEXT");
            let u: Vec<u16> = t.encode_utf16().chain(std::iter::once(0)).collect();
            w.u32(u.len() as u32);
            for c in u {
                w.u16(c);
            }
        }
        Val::List(l) => {
            w.bytes(b"VlLs");
            w.u32(l.len() as u32);
            for x in l {
                put_val(w, x);
            }
        }
    }
}

fn put_desc(w: &mut W, v: &Val) {
    let Val::Obj(class, items) = v else { return };
    w.u32(1);
    w.u16(0);
    put_id(w, class);
    w.u32(items.len() as u32);
    for (k, x) in items {
        put_id(w, k);
        put_val(w, x);
    }
}

/// Le bloc lfx2 des styles d'un calque
fn fx_block(fx: &crate::paint::Fx) -> Vec<u8> {
    let o = |c: &str, items: Vec<(&str, Val)>| Val::Obj(c.into(), items.into_iter().map(|(k, v)| (k.to_string(), v)).collect());
    let px = |x: f32| Val::Unit("#Pxl".into(), x as f64);
    let pc = |x: f32| Val::Unit("#Prc".into(), (x * 100.0) as f64);
    let clr = |c: [u8; 3]| o("RGBC", vec![("Rd  ", Val::Num(c[0] as f64)), ("Grn ", Val::Num(c[1] as f64)), ("Bl  ", Val::Num(c[2] as f64))]);
    let mode = |m: &str| Val::Enum("BlnM".into(), m.into());
    let mut items = vec![("Scl ", Val::Unit("#Prc".into(), 100.0)), ("masterFXSwitch", Val::Bool(fx.on))];
    if let Some(s) = fx.shadow {
        let dist = s.dx.hypot(s.dy);
        let ang = if dist > 0.0 { s.dy.atan2(-s.dx).to_degrees() } else { 120.0 };
        items.push(("DrSh", o("DrSh", vec![("enab", Val::Bool(true)), ("present", Val::Bool(true)), ("showInDialog", Val::Bool(true)), ("Md  ", mode("Mltp")), ("Clr ", clr(s.color)), ("Opct", pc(s.opacity)), ("uglg", Val::Bool(false)), ("lagl", Val::Unit("#Ang".into(), ang as f64)), ("Dstn", px(dist)), ("Ckmt", px(0.0)), ("blur", px(s.blur)), ("Nose", pc(0.0)), ("AntA", Val::Bool(false)), ("layerConceals", Val::Bool(true))])));
    }
    if let Some(g) = fx.glow {
        items.push(("OrGl", o("OrGl", vec![("enab", Val::Bool(true)), ("present", Val::Bool(true)), ("showInDialog", Val::Bool(true)), ("Md  ", mode("Scrn")), ("Clr ", clr(g.color)), ("Opct", pc(g.opacity)), ("GlwT", Val::Enum("BETE".into(), "SfBL".into())), ("Ckmt", px(0.0)), ("blur", px(g.size)), ("Nose", pc(0.0)), ("ShdN", pc(0.0)), ("AntA", Val::Bool(false)), ("Inpr", pc(0.5))])));
    }
    if let Some(f) = fx.overlay {
        items.push(("SoFi", o("SoFi", vec![("enab", Val::Bool(true)), ("present", Val::Bool(true)), ("showInDialog", Val::Bool(true)), ("Md  ", mode("Nrml")), ("Opct", pc(f.opacity)), ("Clr ", clr(f.color))])));
    }
    if let Some(st) = fx.stroke {
        items.push(("FrFX", o("FrFX", vec![("enab", Val::Bool(true)), ("present", Val::Bool(true)), ("showInDialog", Val::Bool(true)), ("Styl", Val::Enum("FStl".into(), "OutF".into())), ("PntT", Val::Enum("FrFl".into(), "SClr".into())), ("Md  ", mode("Nrml")), ("Opct", pc(st.opacity)), ("Sz  ", px(st.size)), ("Clr ", clr(st.color))])));
    }
    let mut w = W(vec![]);
    w.u32(0);
    w.u32(16);
    put_desc(&mut w, &o("null", items));
    w.0
}
