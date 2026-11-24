//! Lecture et écriture de fichiers Photoshop (.psd), 8 bits RVB : calques, masques de fusion, groupes,
//! opacité, fond, modes de fusion, visibilité, écrêtage et noms (Unicode). Écrit à partir de la
//! spécification publique du format ; aucune bibliothèque.

use crate::paint::{Blend, Doc, Group, Layer};

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
                        _ => {}
                    }
                    r.at = data_end;
                }
                r.at = end;
                recs.push(Rec { rect, chans, blend: blend_of(&key).unwrap_or_default(), opacity, clip, hidden: flags & 2 != 0, name, mask, section, pass, fill });
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
                            groups.push(Group { id, name: rec.name, visible: !rec.hidden, opacity: rec.opacity as f32 / 255.0, blend: if rec.pass { None } else { Some(rec.blend) }, collapsed: rec.section == 2 });
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
        }
    };
    let empty = |name: &str, section: (u32, bool, Blend), hidden: bool, opacity: u8| Out { rect: [0; 4], chans: vec![(-1, vec![]), (0, vec![]), (1, vec![]), (2, vec![])], blend: section.2, opacity, fill: 255, clip: false, hidden, name: name.to_string(), mask: None, section: Some(section) };
    while i < doc.layers.len() {
        let gid = doc.layers[i].group;
        let mut j = i + 1;
        while j < doc.layers.len() && doc.layers[j].group == gid {
            j += 1;
        }
        let g = group(gid).filter(|_| gid != 0);
        if g.is_some() {
            outs.push(empty("</Layer group>", (3, false, Blend::Normal), false, 255));
        }
        for l in &doc.layers[i..j] {
            if l.adj.is_some() {
                lost.push(format!("{} ({})", l.name, crate::i18n::t("calque de réglage")));
                continue;
            }
            if l.fx.active() {
                lost.push(format!("{} ({})", l.name, crate::i18n::t("styles")));
            }
            outs.push(to_out(l));
        }
        if let Some(g) = g {
            outs.push(empty(&g.name, (if g.collapsed { 2 } else { 1 }, g.blend.is_none(), g.blend.unwrap_or_default()), !g.visible, (g.opacity * 255.0).round() as u8));
        }
        i = j;
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
        d.layers.push(top);
        d.groups.push(Group { id: 3, name: "Dossier".into(), visible: true, opacity: 1.0, blend: None, collapsed: false });
        let (bytes, lost) = write(&d);
        assert!(lost.is_empty());
        let back = read(&bytes).unwrap();
        assert_eq!((back.w, back.h, back.layers.len()), (6, 4, 2));
        let l = &back.layers[1];
        assert_eq!(l.name, "Rouge été");
        assert_eq!((l.off, l.w, l.h), ([1, 1], 3, 2));
        assert_eq!(l.blend, Blend::Multiply);
        assert!((l.opacity - 0.5).abs() < 0.01);
        assert!(l.mask.is_some() && !l.mask_on);
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
