//! PowerPoint (.pptx) affiché comme dans PowerPoint : chaque forme, image et zone de texte
//! à sa place, avec ses tailles, couleurs et le fond du masque. Double-clic sur un texte
//! pour le modifier, glisser une forme pour la déplacer, puis « enregistrer » réécrit le
//! fichier .pptx (le reste du fichier n'est pas touché).

use crate::office::{attr, tokens, Archive, Tok};
use eframe::egui::{self, pos2, vec2, Color32, Rect, RichText, Sense, Stroke, TextureHandle};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const EMU_PT: f32 = 12700.0;

#[derive(Clone, Default, Debug)]
pub struct Run {
    pub text: String,
    pub size: Option<f32>,
    pub bold: bool,
    pub italic: bool,
    pub color: Option<Color32>,
}

#[derive(Clone, Default, Debug)]
pub struct Para {
    pub runs: Vec<Run>,
    pub align: u8,
    pub align_set: bool,
    pub bullet: bool,
    pub level: u8,
    pub size: Option<f32>,
}

impl Para {
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

#[derive(Clone, Debug)]
pub enum Kind {
    Text(Vec<Para>),
    Image(usize),
    Shape,
}

#[derive(Clone, Debug)]
pub struct Shape {
    /// x, y, largeur, hauteur en EMU
    pub rect: [f32; 4],
    pub has_xfrm: bool,
    pub fill: Option<Color32>,
    pub line: Option<Color32>,
    pub ellipse: bool,
    pub kind: Kind,
    pub anchor: u8,
    pub ph: Option<(String, String)>,
    /// Rang du <p:sp> dans le fichier de la diapo (pour réécrire le texte).
    pub sp_index: Option<usize>,
    pub in_group: bool,
    /// Valeurs par défaut de la forme (lstStyle) : alignement et taille.
    pub def_align: Option<u8>,
    pub def_size: Option<f32>,
    /// Réduction automatique du texte (normAutofit fontScale).
    pub font_scale: f32,
}

pub struct Slide {
    pub part: String,
    pub bg: Option<Color32>,
    /// Formes du masque et de la disposition (décor), dessinées sous la diapo.
    pub under: Vec<Shape>,
    pub shapes: Vec<Shape>,
    pub media: Vec<Vec<u8>>,
}

pub struct Deck {
    pub size: [f32; 2],
    pub slides: Vec<Slide>,
}

// ---------- couleurs du thème ----------

fn theme_colors(ar: &mut Archive) -> HashMap<String, Color32> {
    let mut out = HashMap::new();
    let names: Vec<String> = ar.names().into_iter().filter(|n| n.starts_with("ppt/theme/theme") && n.ends_with(".xml")).collect();
    let Some(first) = names.iter().min_by_key(|n| n.len()).cloned() else { return out };
    let xml = ar.text(&first).unwrap_or_default();
    let mut cur: Option<String> = None;
    let mut in_scheme = false;
    for t in tokens(&xml) {
        match t {
            Tok::Open("clrScheme", _) => in_scheme = true,
            Tok::Close("clrScheme") => in_scheme = false,
            Tok::Open(n, _) if in_scheme && matches!(n, "dk1" | "lt1" | "dk2" | "lt2" | "accent1" | "accent2" | "accent3" | "accent4" | "accent5" | "accent6" | "hlink" | "folHlink") => cur = Some(n.to_string()),
            Tok::Open("srgbClr", a) if cur.is_some() => {
                if let Some(c) = attr(a, "val").and_then(hex) {
                    out.insert(cur.take().unwrap(), c);
                }
            }
            Tok::Open("sysClr", a) if cur.is_some() => {
                if let Some(c) = attr(a, "lastClr").and_then(hex) {
                    out.insert(cur.take().unwrap(), c);
                }
            }
            _ => {}
        }
    }
    for (alias, real) in [("tx1", "dk1"), ("bg1", "lt1"), ("tx2", "dk2"), ("bg2", "lt2")] {
        if let Some(c) = out.get(real).copied() {
            out.insert(alias.into(), c);
        }
    }
    out
}

fn hex(s: &str) -> Option<Color32> {
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

// ---------- lecture d'une partie (diapo, disposition ou masque) ----------

struct Parsed {
    shapes: Vec<Shape>,
    bg: Option<Color32>,
    /// Styles du masque : (alignement, taille) du titre, tailles du corps par niveau.
    title_style: (Option<u8>, Option<f32>),
    body_sizes: [Option<f32>; 9],
    other_size: Option<f32>,
}

#[derive(Clone, Copy)]
struct GroupXf {
    off: [f32; 2],
    ext: [f32; 2],
    ch_off: [f32; 2],
    ch_ext: [f32; 2],
}

fn parse_part(xml: &str, rels: &HashMap<String, String>, ar: &mut Archive, media: &mut Vec<Vec<u8>>, colors: &HashMap<String, Color32>) -> Parsed {
    let mut shapes = vec![];
    let mut bg = None;
    let mut title_style: (Option<u8>, Option<f32>) = (None, None);
    let mut body_sizes: [Option<f32>; 9] = [None; 9];
    let mut other_size = None;
    let mut lvl_ctx: Option<usize> = None;
    let mut stack: Vec<&str> = vec![];
    let mut cur: Option<Shape> = None;
    let mut para: Option<Para> = None;
    let mut run: Option<Run> = None;
    let mut sp_count = 0usize;
    let mut groups: Vec<GroupXf> = vec![];
    let mut pending_group: Option<GroupXf> = None;
    // Où va la prochaine couleur trouvée
    #[derive(PartialEq, Clone, Copy)]
    enum ColorTarget {
        None,
        Fill,
        Line,
        Run,
        Bg,
    }
    let mut target = ColorTarget::None;

    let apply_group = |r: [f32; 4], groups: &[GroupXf]| -> [f32; 4] {
        let mut r = r;
        for g in groups.iter().rev() {
            let sx = if g.ch_ext[0] > 0.0 { g.ext[0] / g.ch_ext[0] } else { 1.0 };
            let sy = if g.ch_ext[1] > 0.0 { g.ext[1] / g.ch_ext[1] } else { 1.0 };
            r = [g.off[0] + (r[0] - g.ch_off[0]) * sx, g.off[1] + (r[1] - g.ch_off[1]) * sy, r[2] * sx, r[3] * sy];
        }
        r
    };
    let num = |a: &str, k: &str| attr(a, k).and_then(|v| v.parse::<f32>().ok());

    for t in tokens(xml) {
        match t {
            Tok::Open(name, a) => {
                let parent = stack.last().copied().unwrap_or("");
                let in_tx = stack.contains(&"txBody");
                match name {
                    "sp" | "pic" | "cxnSp" => {
                        if name == "sp" {
                            sp_count += 1;
                        }
                        cur = Some(Shape {
                            rect: [0.0; 4],
                            has_xfrm: false,
                            fill: None,
                            line: None,
                            ellipse: false,
                            kind: Kind::Shape,
                            anchor: 0,
                            ph: None,
                            sp_index: (name == "sp").then_some(sp_count - 1),
                            in_group: !groups.is_empty(),
                            def_align: None,
                            def_size: None,
                            font_scale: 1.0,
                        });
                    }
                    "grpSp" if !stack.is_empty() => {}
                    "off" | "chOff" | "ext" | "chExt" if parent == "xfrm" => {
                        let in_grp_pr = stack.iter().rev().nth(1) == Some(&"grpSpPr");
                        if in_grp_pr && cur.is_none() {
                            let g = pending_group.get_or_insert(GroupXf { off: [0.0; 2], ext: [1.0; 2], ch_off: [0.0; 2], ch_ext: [1.0; 2] });
                            match name {
                                "off" => g.off = [num(a, "x").unwrap_or(0.0), num(a, "y").unwrap_or(0.0)],
                                "chOff" => g.ch_off = [num(a, "x").unwrap_or(0.0), num(a, "y").unwrap_or(0.0)],
                                "ext" => g.ext = [num(a, "cx").unwrap_or(1.0), num(a, "cy").unwrap_or(1.0)],
                                _ => g.ch_ext = [num(a, "cx").unwrap_or(1.0), num(a, "cy").unwrap_or(1.0)],
                            }
                        } else if let Some(s) = &mut cur {
                            s.has_xfrm = true;
                            match name {
                                "off" => {
                                    s.rect[0] = num(a, "x").unwrap_or(0.0);
                                    s.rect[1] = num(a, "y").unwrap_or(0.0);
                                }
                                "ext" => {
                                    s.rect[2] = num(a, "cx").unwrap_or(0.0);
                                    s.rect[3] = num(a, "cy").unwrap_or(0.0);
                                }
                                _ => {}
                            }
                        }
                    }
                    "ph" => {
                        if let Some(s) = &mut cur {
                            let ty = attr(a, "type").unwrap_or("body").to_string();
                            let idx = attr(a, "idx").unwrap_or("").to_string();
                            s.ph = Some((ty, idx));
                        }
                    }
                    "prstGeom" => {
                        if let Some(s) = &mut cur {
                            s.ellipse = attr(a, "prst") == Some("ellipse");
                        }
                    }
                    "solidFill" => {
                        target = if stack.contains(&"bgPr") {
                            ColorTarget::Bg
                        } else if stack.contains(&"rPr") || parent == "rPr" || stack.contains(&"defRPr") {
                            ColorTarget::Run
                        } else if parent == "ln" {
                            ColorTarget::Line
                        } else if parent == "spPr" {
                            ColorTarget::Fill
                        } else {
                            ColorTarget::None
                        };
                    }
                    "srgbClr" | "schemeClr" | "sysClr" | "prstClr" => {
                        let c = match name {
                            "srgbClr" => attr(a, "val").and_then(hex),
                            "sysClr" => attr(a, "lastClr").and_then(hex),
                            "schemeClr" => attr(a, "val").and_then(|v| colors.get(v).copied()),
                            _ => match attr(a, "val") {
                                Some("white") => Some(Color32::WHITE),
                                Some("black") => Some(Color32::BLACK),
                                _ => None,
                            },
                        };
                        if let Some(c) = c {
                            match target {
                                ColorTarget::Fill => {
                                    if let Some(s) = &mut cur {
                                        s.fill = Some(c);
                                    }
                                }
                                ColorTarget::Line => {
                                    if let Some(s) = &mut cur {
                                        s.line = Some(c);
                                    }
                                }
                                ColorTarget::Run => {
                                    if let Some(r) = &mut run {
                                        r.color = Some(c);
                                    }
                                }
                                ColorTarget::Bg => bg = Some(c),
                                ColorTarget::None => {}
                            }
                        }
                        target = ColorTarget::None;
                    }
                    "bodyPr" if in_tx => {
                        if let Some(s) = &mut cur {
                            s.anchor = match attr(a, "anchor") {
                                Some("ctr") => 1,
                                Some("b") => 2,
                                _ => 0,
                            };
                        }
                    }
                    "p" if in_tx => para = Some(Para::default()),
                    "normAutofit" => {
                        if let (Some(s), Some(f)) = (&mut cur, num(a, "fontScale")) {
                            s.font_scale = f / 100_000.0;
                        }
                    }
                    n if n.len() == 7 && n.starts_with("lvl") && n.ends_with("pPr") => {
                        let lvl = n[3..4].parse::<usize>().unwrap_or(1).saturating_sub(1);
                        lvl_ctx = Some(lvl);
                        let al = attr(a, "algn").map(|v| match v {
                            "ctr" => 1,
                            "r" => 2,
                            _ => 0,
                        });
                        if stack.contains(&"titleStyle") && lvl == 0 {
                            title_style.0 = al.or(title_style.0);
                        } else if lvl == 0 {
                            if let Some(s) = &mut cur {
                                if stack.contains(&"lstStyle") {
                                    s.def_align = al.or(s.def_align);
                                }
                            }
                        }
                    }
                    "defRPr" if lvl_ctx.is_some() => {
                        let lvl = lvl_ctx.unwrap();
                        if let Some(sz) = num(a, "sz").map(|v| v / 100.0) {
                            if stack.contains(&"titleStyle") {
                                if lvl == 0 {
                                    title_style.1 = Some(sz);
                                }
                            } else if stack.contains(&"bodyStyle") {
                                body_sizes[lvl.min(8)] = Some(sz);
                            } else if stack.contains(&"otherStyle") {
                                if lvl == 0 {
                                    other_size = Some(sz);
                                }
                            } else if let Some(s) = &mut cur {
                                if lvl == 0 && stack.contains(&"lstStyle") {
                                    s.def_size = Some(sz);
                                }
                            }
                        }
                    }
                    "pPr" => {
                        if let Some(p) = &mut para {
                            p.align_set = attr(a, "algn").is_some();
                            p.align = match attr(a, "algn") {
                                Some("ctr") => 1,
                                Some("r") => 2,
                                _ => 0,
                            };
                            p.level = attr(a, "lvl").and_then(|v| v.parse().ok()).unwrap_or(0);
                        }
                    }
                    "buChar" | "buAutoNum" => {
                        if let Some(p) = &mut para {
                            p.bullet = true;
                        }
                    }
                    "r" | "fld" if para.is_some() => run = Some(Run::default()),
                    "br" if para.is_some() => {
                        if let Some(p) = &mut para {
                            p.runs.push(Run { text: "\n".into(), ..Default::default() });
                        }
                    }
                    "rPr" | "endParaRPr" => {
                        let size = num(a, "sz").map(|v| v / 100.0);
                        if let Some(r) = &mut run {
                            r.size = size;
                            r.bold = attr(a, "b") == Some("1");
                            r.italic = attr(a, "i") == Some("1");
                        } else if let Some(p) = &mut para {
                            p.size = size.or(p.size);
                        }
                    }
                    "blip" => {
                        if let (Some(s), Some(target)) = (&mut cur, attr(a, "embed").and_then(|id| rels.get(id))) {
                            if let Some(b) = ar.bytes(target) {
                                media.push(b);
                                s.kind = Kind::Image(media.len() - 1);
                            }
                        } else if stack.contains(&"bgPr") {
                            // image de fond : ignorée (on garde la couleur)
                        }
                    }
                    _ => {}
                }
                stack.push(name);
            }
            Tok::Close(name) => {
                if let Some(i) = stack.iter().rposition(|n| *n == name) {
                    stack.truncate(i);
                }
                match name {
                    n if n.len() == 7 && n.starts_with("lvl") && n.ends_with("pPr") => lvl_ctx = None,
                    "grpSpPr" => {
                        if let Some(g) = pending_group.take() {
                            groups.push(g);
                        }
                    }
                    "grpSp" => {
                        if !stack.is_empty() {
                            groups.pop();
                        }
                    }
                    "r" | "fld" => {
                        if let (Some(r), Some(p)) = (run.take(), &mut para) {
                            p.runs.push(r);
                        }
                    }
                    "p" => {
                        if let Some(p) = para.take() {
                            if let Some(s) = &mut cur {
                                match &mut s.kind {
                                    Kind::Text(v) => v.push(p),
                                    Kind::Shape => s.kind = Kind::Text(vec![p]),
                                    Kind::Image(_) => {}
                                }
                            }
                        }
                    }
                    "sp" | "pic" | "cxnSp" => {
                        if let Some(mut s) = cur.take() {
                            if s.has_xfrm {
                                s.rect = apply_group(s.rect, &groups);
                            }
                            shapes.push(s);
                        }
                    }
                    _ => {}
                }
            }
            Tok::Text(txt) => {
                if stack.last() == Some(&"t") {
                    if let Some(r) = &mut run {
                        r.text.push_str(&txt);
                    }
                }
            }
        }
    }
    Parsed { shapes, bg, title_style, body_sizes, other_size }
}

fn rel_of_type(ar: &mut Archive, part: &str, ty: &str) -> Option<String> {
    let dir = part.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let file = part.rsplit('/').next().unwrap_or(part);
    let xml = ar.text(&format!("{dir}/_rels/{file}.rels"))?;
    for t in tokens(&xml) {
        if let Tok::Open("Relationship", a) = t {
            if attr(a, "Type").is_some_and(|v| v.ends_with(ty)) {
                return attr(a, "Target").map(|tg| crate::office::resolve(dir, tg));
            }
        }
    }
    None
}

fn ph_key(ph: &(String, String)) -> (String, String) {
    let ty = match ph.0.as_str() {
        "ctrTitle" => "title",
        "subTitle" => "body",
        t => t,
    };
    (ty.to_string(), ph.1.clone())
}

pub fn load(path: &Path) -> Result<Deck, String> {
    let mut ar = Archive::open(path)?;
    let pres = ar.text("ppt/presentation.xml").unwrap_or_default();
    let mut size = [9_144_000.0, 6_858_000.0];
    for t in tokens(&pres) {
        if let Tok::Open("sldSz", a) = t {
            if let (Some(cx), Some(cy)) = (attr(a, "cx").and_then(|v| v.parse().ok()), attr(a, "cy").and_then(|v| v.parse().ok())) {
                size = [cx, cy];
            }
        }
    }
    // Ordre des diapos : celui de presentation.xml, sinon le numéro du fichier.
    let pres_rels = crate::office::rels(&mut ar, "ppt/presentation.xml");
    let mut order: Vec<String> = tokens(&pres)
        .into_iter()
        .filter_map(|t| match t {
            Tok::Open("sldId", a) => {
                // l'attribut r:id (et pas id)
                let rid = a.split_whitespace().find(|kv| kv.contains(":id="))?.split(['"', '\'']).nth(1)?;
                pres_rels.get(rid).cloned()
            }
            _ => None,
        })
        .collect();
    if order.is_empty() {
        order = ar.names().into_iter().filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml")).collect();
        order.sort_by_key(|n| n.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse::<usize>().unwrap_or(0));
    }
    let colors = theme_colors(&mut ar);
    let mut cache: HashMap<String, (Vec<Shape>, Option<Color32>, Vec<Vec<u8>>, (Option<u8>, Option<f32>), [Option<f32>; 9], Option<f32>)> = HashMap::new();
    let mut slides = vec![];
    for part in order {
        let Some(xml) = ar.text(&part) else { continue };
        let rels = crate::office::rels(&mut ar, &part);
        let mut media = vec![];
        let parsed = parse_part(&xml, &rels, &mut ar, &mut media, &colors);
        // Disposition et masque : positions des espaces réservés et décor.
        let mut under: Vec<Shape> = vec![];
        let mut placeholders: HashMap<(String, String), Shape> = HashMap::new();
        let mut by_type: HashMap<String, Shape> = HashMap::new();
        let mut bg = parsed.bg;
        let mut title_style: (Option<u8>, Option<f32>) = (None, None);
        let mut body_sizes: [Option<f32>; 9] = [None; 9];
        let mut other_size: Option<f32> = None;
        let layout = rel_of_type(&mut ar, &part, "/slideLayout");
        let master = layout.as_ref().and_then(|l| rel_of_type(&mut ar, l, "/slideMaster"));
        for p in [master.clone(), layout.clone()].into_iter().flatten() {
            if !cache.contains_key(&p) {
                let x = ar.text(&p).unwrap_or_default();
                let r = crate::office::rels(&mut ar, &p);
                let mut m = vec![];
                let pp = parse_part(&x, &r, &mut ar, &mut m, &colors);
                cache.insert(p.clone(), (pp.shapes, pp.bg, m, pp.title_style, pp.body_sizes, pp.other_size));
            }
            let (shapes, pbg, m, ts, bs, os) = cache.get(&p).cloned().unwrap();
            if ts.0.is_some() {
                title_style.0 = ts.0;
            }
            if ts.1.is_some() {
                title_style.1 = ts.1;
            }
            for k in 0..9 {
                if bs[k].is_some() {
                    body_sizes[k] = bs[k];
                }
            }
            if os.is_some() {
                other_size = os;
            }
            if parsed.bg.is_none() && pbg.is_some() {
                bg = pbg;
            }
            let base = media.len();
            media.extend(m);
            for mut s in shapes {
                if let Kind::Image(i) = &mut s.kind {
                    *i += base;
                }
                match &s.ph {
                    Some(ph) => {
                        let k = ph_key(ph);
                        by_type.insert(k.0.clone(), s.clone());
                        placeholders.insert(k, s);
                    }
                    None => under.push(s),
                }
            }
        }
        let mut shapes = parsed.shapes;
        for s in &mut shapes {
            if let Some(ph) = &s.ph {
                let k = ph_key(ph);
                let from = placeholders.get(&k).or_else(|| if k.1.is_empty() { by_type.get(&k.0) } else { None }).or_else(|| by_type.get(&k.0));
                if let Some(f) = from {
                    if !s.has_xfrm {
                        s.rect = f.rect;
                    }
                    if s.fill.is_none() {
                        s.fill = f.fill;
                    }
                    if s.anchor == 0 {
                        s.anchor = f.anchor;
                    }
                    if s.def_align.is_none() {
                        s.def_align = f.def_align;
                    }
                    if s.def_size.is_none() {
                        s.def_size = f.def_size;
                    }
                }
                if s.rect[2] <= 0.0 {
                    // Aucune position connue : disposition par défaut
                    let title = k.0 == "title";
                    s.rect = if title { [size[0] * 0.07, size[1] * 0.06, size[0] * 0.86, size[1] * 0.2] } else { [size[0] * 0.07, size[1] * 0.3, size[0] * 0.86, size[1] * 0.6] };
                }
                let title = k.0 == "title";
                let def_align = s.def_align.or(if title { title_style.0 } else { None });
                let (def_size, scale) = (s.def_size, s.font_scale);
                if let Kind::Text(ps) = &mut s.kind {
                    for p in ps.iter_mut() {
                        let lvl = (p.level as usize).min(8);
                        let base = def_size.or(if title { title_style.1 } else if k.0 == "body" { body_sizes[lvl] } else { other_size.or(Some(18.0)) }).unwrap_or(if title { 44.0 } else { 28.0 });
                        if p.size.is_none() {
                            p.size = Some(base);
                        }
                        if !p.align_set {
                            if let Some(a) = def_align {
                                p.align = a;
                            }
                        }
                        if k.0 == "body" && ph.0 == "body" && !p.text().is_empty() {
                            p.bullet = true;
                        }
                    }
                    if scale < 0.999 {
                        for p in ps.iter_mut() {
                            p.size = p.size.map(|v| v * scale);
                            for r in &mut p.runs {
                                r.size = r.size.map(|v| v * scale);
                            }
                        }
                    }
                }
            }
        }
        // Formes hors espaces réservés : taille par défaut et réduction auto.
        for s in &mut shapes {
            if s.ph.is_some() {
                continue;
            }
            let (def_size, def_align, scale) = (s.def_size.or(other_size).unwrap_or(18.0), s.def_align, s.font_scale);
            if let Kind::Text(ps) = &mut s.kind {
                for p in ps.iter_mut() {
                    if p.size.is_none() {
                        p.size = Some(def_size);
                    }
                    if !p.align_set {
                        if let Some(a) = def_align {
                            p.align = a;
                        }
                    }
                    if scale < 0.999 {
                        p.size = p.size.map(|v| v * scale);
                        for r in &mut p.runs {
                            r.size = r.size.map(|v| v * scale);
                        }
                    }
                }
            }
        }
        // Décor des dispositions : seulement les formes sans texte d'exemple
        under.retain(|s| !matches!(&s.kind, Kind::Text(ps) if ps.iter().any(|p| p.text().contains('‹'))));
        slides.push(Slide { part, bg, under, shapes, media });
    }
    if slides.is_empty() {
        return Err("Aucune diapositive.".into());
    }
    Ok(Deck { size, slides })
}

// ---------- réécriture du fichier ----------

pub struct Edit {
    pub part: String,
    pub sp_index: usize,
    pub lines: Option<Vec<String>>,
    pub off: Option<[i64; 2]>,
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Position [début, fin) du n-ième <p:sp> (fin = après </p:sp>).
fn find_sp(xml: &str, n: usize) -> Option<(usize, usize)> {
    let re = regex::Regex::new(r"<(?:\w+:)?sp(?:\s[^>]*)?>").ok()?;
    let m = re.find_iter(xml).nth(n)?;
    let close = regex::Regex::new(r"</(?:\w+:)?sp>").ok()?;
    let e = close.find_at(xml, m.end())?;
    Some((m.start(), e.end()))
}

fn rewrite_text(sp: &str, lines: &[String]) -> Option<String> {
    let tb = regex::Regex::new(r"(?s)<(\w+:)?txBody>(.*)</(?:\w+:)?txBody>").ok()?;
    let c = tb.captures(sp)?;
    let whole = c.get(0)?;
    let body = c.get(2)?.as_str();
    let pre = c.get(1).map(|m| m.as_str()).unwrap_or("");
    let para_re = regex::Regex::new(r"(?s)<a:p>.*?</a:p>|<a:p/>").ok()?;
    let first_para = para_re.find(body).map(|m| m.start()).unwrap_or(body.len());
    let head = &body[..first_para]; // bodyPr + lstStyle
    let paras: Vec<&str> = para_re.find_iter(body).map(|m| m.as_str()).collect();
    let ppr_re = regex::Regex::new(r"(?s)<a:pPr[^>]*/>|<a:pPr[^>]*>.*?</a:pPr>").ok()?;
    let rpr_re = regex::Regex::new(r"(?s)<a:rPr[^>]*/>|<a:rPr[^>]*>.*?</a:rPr>").ok()?;
    let mut out = String::from(head);
    for (k, line) in lines.iter().enumerate() {
        let tpl = paras.get(k).or(paras.last()).copied().unwrap_or("");
        let ppr = ppr_re.find(tpl).map(|m| m.as_str()).unwrap_or("");
        let rpr = rpr_re.find(tpl).map(|m| m.as_str()).unwrap_or("");
        out.push_str("<a:p>");
        out.push_str(ppr);
        if !line.is_empty() {
            out.push_str(&format!("<a:r>{rpr}<a:t>{}</a:t></a:r>", escape(line)));
        }
        out.push_str("</a:p>");
    }
    let new_tb = format!("<{pre}txBody>{out}</{pre}txBody>");
    Some(format!("{}{}{}", &sp[..whole.start()], new_tb, &sp[whole.end()..]))
}

fn rewrite_off(sp: &str, off: [i64; 2]) -> Option<String> {
    let re = regex::Regex::new(r#"<a:off\s+x="-?\d+"\s+y="-?\d+"\s*/>"#).ok()?;
    let m = re.find(sp)?;
    Some(format!("{}<a:off x=\"{}\" y=\"{}\"/>{}", &sp[..m.start()], off[0], off[1], &sp[m.end()..]))
}

pub fn save(path: &Path, edits: &[Edit]) -> Result<(), String> {
    let mut ar = Archive::open(path)?;
    let mut changed: HashMap<String, String> = HashMap::new();
    for e in edits {
        let xml = match changed.get(&e.part) {
            Some(x) => x.clone(),
            None => ar.text(&e.part).ok_or("diapo introuvable")?,
        };
        let (a, b) = find_sp(&xml, e.sp_index).ok_or("forme introuvable dans la diapo")?;
        let mut sp = xml[a..b].to_string();
        if let Some(lines) = &e.lines {
            sp = rewrite_text(&sp, lines).ok_or("texte introuvable dans la forme")?;
        }
        if let Some(off) = e.off {
            sp = rewrite_off(&sp, off).unwrap_or(sp);
        }
        changed.insert(e.part.clone(), format!("{}{}{}", &xml[..a], sp, &xml[b..]));
    }
    // Réécrit l'archive à côté, puis remplace l'original.
    let tmp = path.with_extension("pptx.aryadne-tmp");
    {
        let src = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut zin = zip::ZipArchive::new(std::io::BufReader::new(src)).map_err(|e| e.to_string())?;
        let mut zout = zip::ZipWriter::new(std::fs::File::create(&tmp).map_err(|e| e.to_string())?);
        for i in 0..zin.len() {
            let f = zin.by_index(i).map_err(|e| e.to_string())?;
            let name = f.name().map(|n| n.to_string()).unwrap_or_default();
            if let Some(new) = changed.get(&name) {
                drop(f);
                zout.start_file(name, zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)).map_err(|e| e.to_string())?;
                std::io::Write::write_all(&mut zout, new.as_bytes()).map_err(|e| e.to_string())?;
            } else {
                zout.raw_copy_file(f).map_err(|e| e.to_string())?;
            }
        }
        zout.finish().map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Impossible d'enregistrer (le fichier est-il ouvert dans PowerPoint ?) : {e}")
    })
}

// ---------- affichage et édition ----------

pub struct Viewer {
    path: PathBuf,
    pub deck: Result<Deck, String>,
    tex: HashMap<(usize, usize), Option<TextureHandle>>,
    current: usize,
    editing: Option<(usize, usize, String)>,
    selected: Option<(usize, usize)>,
    edits: Vec<Edit>,
    drag: Option<(usize, usize, [f32; 2])>,
    pub message: Option<String>,
}

impl Viewer {
    pub fn open(path: &Path) -> Self {
        Self { path: path.to_path_buf(), deck: load(path), tex: HashMap::new(), current: 0, editing: None, selected: None, edits: vec![], drag: None, message: None }
    }

    fn texture(&mut self, ctx: &egui::Context, key: (usize, usize), bytes: &[u8]) -> Option<TextureHandle> {
        self.tex
            .entry(key)
            .or_insert_with(|| {
                let img = image::load_from_memory(bytes).ok()?;
                let img = if img.width() > 2000 || img.height() > 2000 { img.thumbnail(2000, 2000) } else { img };
                let rgba = img.to_rgba8();
                Some(ctx.load_texture(format!("slide{key:?}"), egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()), egui::TextureOptions::LINEAR))
            })
            .clone()
    }

    fn record(&mut self, part: &str, sp: usize, lines: Option<Vec<String>>, off: Option<[i64; 2]>) {
        if let Some(e) = self.edits.iter_mut().find(|e| e.part == part && e.sp_index == sp) {
            if lines.is_some() {
                e.lines = lines;
            }
            if off.is_some() {
                e.off = off;
            }
        } else {
            self.edits.push(Edit { part: part.to_string(), sp_index: sp, lines, off });
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let ctx = ui.ctx().clone();
        let n = match &self.deck {
            Ok(d) => d.slides.len(),
            Err(e) => {
                ui.label(RichText::new(e.as_str()).color(p.missing));
                return;
            }
        };
        // Barre : navigation + enregistrer
        ui.horizontal(|ui| {
            if ui.button("‹").clicked() || (self.editing.is_none() && ui.input(|i| i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::PageUp))) {
                self.current = self.current.saturating_sub(1);
            }
            ui.label(RichText::new(format!("diapo {} / {n}", self.current + 1)).size(12.0));
            if ui.button("›").clicked() || (self.editing.is_none() && ui.input(|i| i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::PageDown))) {
                self.current = (self.current + 1).min(n - 1);
            }
            ui.label(RichText::new(crate::i18n::t("double-clic : modifier un texte · glisser : déplacer")).color(p.muted).size(11.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !self.edits.is_empty() {
                    let b = egui::Button::new(RichText::new(format!("enregistrer ({})", self.edits.len())).color(p.on_accent).family(crate::theme::bold())).fill(p.accent).stroke(Stroke::NONE);
                    if ui.add(b).on_hover_text(crate::i18n::t("Écrit les changements dans le fichier .pptx")).clicked() {
                        self.commit_edit();
                        match save(&self.path, &self.edits) {
                            Ok(()) => {
                                self.edits.clear();
                                self.message = Some("enregistré".into());
                            }
                            Err(e) => self.message = Some(e),
                        }
                    }
                    if ui.button(crate::i18n::t("annuler")).clicked() {
                        self.edits.clear();
                        self.editing = None;
                        self.deck = load(&self.path);
                        self.tex.clear();
                    }
                }
                if let Some(m) = &self.message {
                    ui.label(RichText::new(m).color(p.muted).size(11.0));
                }
            });
        });
        ui.add_space(6.0);

        // Pellicule à gauche, diapo à droite
        let avail = ui.available_rect_before_wrap();
        let strip_w = 150.0_f32.min(avail.width() * 0.22);
        let deck_size = self.deck.as_ref().map(|d| d.size).unwrap_or([16.0, 9.0]);
        let aspect = deck_size[1] / deck_size[0];
        let strip = Rect::from_min_size(avail.min, vec2(strip_w, avail.height()));
        let main = Rect::from_min_max(pos2(avail.min.x + strip_w + 12.0, avail.min.y), avail.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(strip), |ui| {
            egui::ScrollArea::vertical().id_salt("slide-strip").auto_shrink(false).show(ui, |ui| {
                for i in 0..n {
                    let w = strip_w - 14.0;
                    let (r, resp) = ui.allocate_exact_size(vec2(w, w * aspect + 16.0), Sense::click());
                    let thumb = Rect::from_min_size(r.min + vec2(14.0, 0.0), vec2(w - 14.0, (w - 14.0) * aspect));
                    ui.painter().text(r.min + vec2(0.0, 2.0), egui::Align2::LEFT_TOP, format!("{}", i + 1), egui::FontId::monospace(10.0), p.muted);
                    self.paint_slide(ui, &ctx, i, thumb, false);
                    let st = if i == self.current { Stroke::new(2.0, p.accent) } else { Stroke::new(1.0, p.line) };
                    ui.painter().rect_stroke(thumb, 0.0, st, egui::StrokeKind::Outside);
                    if resp.clicked() {
                        self.commit_edit();
                        self.current = i;
                    }
                }
            });
        });
        let w = main.width().min((main.height() - 4.0) / aspect).max(100.0);
        let slide_rect = Rect::from_min_size(pos2(main.center().x - w / 2.0, main.min.y), vec2(w, w * aspect));
        ui.scope_builder(egui::UiBuilder::new().max_rect(main), |ui| {
            let cur = self.current;
            self.paint_slide(ui, &ctx, cur, slide_rect, true);
            ui.painter().rect_stroke(slide_rect, 0.0, Stroke::new(1.0, p.line), egui::StrokeKind::Outside);
        });
        ui.allocate_rect(avail, Sense::hover());
    }

    fn commit_edit(&mut self) {
        let Some((si, shi, text)) = self.editing.take() else { return };
        let Ok(deck) = &mut self.deck else { return };
        let slide = &mut deck.slides[si];
        let shape = &mut slide.shapes[shi];
        let lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
        if let Kind::Text(ps) = &mut shape.kind {
            let before: Vec<String> = ps.iter().map(|p| p.text()).collect();
            if before == lines {
                return;
            }
            let tpl = ps.clone();
            *ps = lines
                .iter()
                .enumerate()
                .map(|(k, l)| {
                    let t = tpl.get(k).or(tpl.last()).cloned().unwrap_or_default();
                    let mut r = t.runs.first().cloned().unwrap_or_default();
                    r.text = l.clone();
                    Para { runs: vec![r], ..t }
                })
                .collect();
        }
        let (part, sp) = (slide.part.clone(), shape.sp_index);
        if let Some(sp) = sp {
            self.record(&part, sp, Some(lines), None);
        }
    }

    fn paint_slide(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, i: usize, rect: Rect, interactive: bool) {
        let p = crate::theme::pal();
        let Ok(deck) = &self.deck else { return };
        let scale = rect.width() / deck.size[0];
        let px_per_pt = scale * EMU_PT;
        let slide = &deck.slides[i];
        let bg = slide.bg.unwrap_or(Color32::WHITE);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, bg);
        let to_rect = |r: [f32; 4]| Rect::from_min_size(rect.min + vec2(r[0] * scale, r[1] * scale), vec2(r[2] * scale, r[3] * scale));
        let shapes: Vec<(bool, Shape)> = slide.under.iter().cloned().map(|s| (false, s)).chain(slide.shapes.iter().cloned().map(|s| (true, s))).collect();
        let media: Vec<Vec<u8>> = slide.media.clone();
        let mut double: Option<usize> = None;
        let mut idx = 0usize;
        for (own, s) in &shapes {
            let r = to_rect(s.rect);
            let shape_i = if *own { Some(idx) } else { None };
            if *own {
                idx += 1;
            }
            if let Some(f) = s.fill {
                if s.ellipse {
                    painter.add(egui::Shape::ellipse_filled(r.center(), r.size() / 2.0, f));
                } else {
                    painter.rect_filled(r, 0.0, f);
                }
            }
            if let Some(l) = s.line {
                painter.rect_stroke(r, 0.0, Stroke::new((px_per_pt * 0.75).max(0.5), l), egui::StrokeKind::Inside);
            }
            match &s.kind {
                Kind::Image(m) => {
                    if let Some(b) = media.get(*m) {
                        if let Some(t) = self.texture(ctx, (i, *m), b) {
                            painter.image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                        }
                    }
                }
                Kind::Text(paras) => {
                    let editing_this = interactive && self.editing.as_ref().is_some_and(|e| e.0 == i && Some(e.1) == shape_i);
                    if !editing_this {
                        paint_text(&painter, ui, paras, r, s.anchor, px_per_pt, bg);
                    }
                }
                Kind::Shape => {}
            }
            if interactive {
                if let Some(si) = shape_i {
                    let resp = ui.interact(r, ui.scope_id().with(("shape", i, si)), Sense::click_and_drag());
                    if resp.hovered() && self.editing.is_none() {
                        painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.accent.gamma_multiply(0.6)), egui::StrokeKind::Outside);
                    }
                    if resp.double_clicked() && matches!(s.kind, Kind::Text(_)) && s.sp_index.is_some() {
                        double = Some(si);
                    }
                    if resp.clicked() {
                        self.selected = Some((i, si));
                    }
                    let movable = s.has_xfrm && !s.in_group && s.sp_index.is_some();
                    if resp.drag_started() && movable && self.editing.is_none() {
                        self.drag = Some((i, si, [s.rect[0], s.rect[1]]));
                    }
                    if resp.dragged() {
                        if let Some((di, dsi, _)) = self.drag {
                            if di == i && dsi == si {
                                let d = resp.drag_delta() / scale;
                                if let Ok(deck) = &mut self.deck {
                                    let sh = &mut deck.slides[i].shapes[si];
                                    sh.rect[0] += d.x;
                                    sh.rect[1] += d.y;
                                }
                            }
                        }
                    }
                    if resp.drag_stopped() {
                        if let Some((di, dsi, _)) = self.drag.take() {
                            if let Ok(deck) = &self.deck {
                                let sh = &deck.slides[di].shapes[dsi];
                                let (part, sp, off) = (deck.slides[di].part.clone(), sh.sp_index, [sh.rect[0].round() as i64, sh.rect[1].round() as i64]);
                                if let Some(sp) = sp {
                                    self.record(&part, sp, None, Some(off));
                                }
                            }
                        }
                    }
                    if self.selected == Some((i, si)) && self.editing.is_none() {
                        painter.rect_stroke(r, 0.0, Stroke::new(1.5, p.accent), egui::StrokeKind::Outside);
                    }
                }
            }
        }
        if !interactive {
            return;
        }
        if let Some(si) = double {
            self.commit_edit();
            if let Ok(deck) = &self.deck {
                if let Kind::Text(ps) = &deck.slides[i].shapes[si].kind {
                    let text = ps.iter().map(|p| p.text()).collect::<Vec<_>>().join("\n");
                    self.editing = Some((i, si, text));
                }
            }
        }
        // Zone d'édition par-dessus la forme
        let mut done = false;
        if let Some((ei, esi, text)) = &mut self.editing {
            if *ei == i {
                let Ok(deck) = &self.deck else { return };
                let s = &deck.slides[i].shapes[*esi];
                let r = to_rect(s.rect).expand(2.0);
                let size = match &s.kind {
                    Kind::Text(ps) => ps.first().and_then(|p| p.runs.first().and_then(|r| r.size).or(p.size)).unwrap_or(18.0),
                    _ => 18.0,
                };
                let font = egui::FontId::new((size * px_per_pt).clamp(8.0, 96.0), egui::FontFamily::Name("slide".into()));
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r));
                egui::Frame::new().fill(Color32::WHITE).stroke(Stroke::new(1.5, p.accent)).show(&mut child, |ui| {
                    let resp = ui.add(egui::TextEdit::multiline(text).font(font).text_color(Color32::from_gray(20)).frame(egui::Frame::NONE).desired_width(r.width() - 4.0).desired_rows(1));
                    if !resp.has_focus() && !resp.lost_focus() {
                        resp.request_focus();
                    }
                    if resp.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        done = true;
                    }
                });
            }
        }
        if done {
            self.commit_edit();
        }
    }
}

fn paint_text(painter: &egui::Painter, ui: &egui::Ui, paras: &[Para], r: Rect, anchor: u8, px_per_pt: f32, bg: Color32) {
    let inset = vec2(91440.0 / EMU_PT * px_per_pt, 45720.0 / EMU_PT * px_per_pt);
    let inner = r.shrink2(inset);
    let dark_bg = bg.r() as u32 + bg.g() as u32 + bg.b() as u32 <= 3 * 110;
    let default_color = if dark_bg { Color32::WHITE } else { Color32::from_gray(25) };
    let mut galleys = vec![];
    let mut total = 0.0;
    for p in paras {
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = (inner.width() - p.level as f32 * 18.0 * px_per_pt).max(4.0);
        job.halign = match p.align {
            1 => egui::Align::Center,
            2 => egui::Align::Max,
            _ => egui::Align::Min,
        };
        let base = p.size.unwrap_or(18.0);
        if p.bullet {
            let sz = (p.runs.first().and_then(|r| r.size).unwrap_or(base) * px_per_pt).max(1.0);
            job.append("•  ", 0.0, egui::TextFormat { font_id: egui::FontId::new(sz, egui::FontFamily::Name("slide".into())), color: p.runs.first().and_then(|r| r.color).unwrap_or(default_color), ..Default::default() });
        }
        if p.runs.is_empty() || p.text().is_empty() {
            job.append(" ", 0.0, egui::TextFormat { font_id: egui::FontId::new((base * px_per_pt).max(1.0), egui::FontFamily::Name("slide".into())), ..Default::default() });
        }
        for run in &p.runs {
            let sz = (run.size.unwrap_or(base) * px_per_pt).max(1.0);
            let fam = if run.bold { "slide-bold" } else { "slide" };
            job.append(&run.text, 0.0, egui::TextFormat { font_id: egui::FontId::new(sz, egui::FontFamily::Name(fam.into())), color: run.color.unwrap_or(default_color), italics: run.italic, ..Default::default() });
        }
        let g = ui.fonts_mut(|f| f.layout_job(job));
        // Espace avant chaque paragraphe (comme PowerPoint, ~20 % de la taille)
        let gap = if galleys.is_empty() { 0.0 } else { base * px_per_pt * 0.2 };
        total += g.size().y + gap;
        galleys.push((g, p.level, p.align, gap));
    }
    let mut y = match anchor {
        1 => inner.center().y - total / 2.0,
        2 => inner.bottom() - total,
        _ => inner.top(),
    };
    for (g, level, align, gap) in galleys {
        y += gap;
        let x = match align {
            1 => inner.center().x,
            2 => inner.right(),
            _ => inner.left() + level as f32 * 18.0 * px_per_pt,
        };
        let h = g.size().y;
        painter.galley(pos2(x, y), g, default_color);
        y += h;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn layout_and_edit_roundtrip() {
        let dir = std::env::temp_dir().join(format!("aryadne-slides-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("d.pptx");
        let slide = r#"<p:sld xmlns:a="a" xmlns:p="p"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="100" y="200"/><a:ext cx="5000" cy="1000"/></a:xfrm><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></p:spPr><p:txBody><a:bodyPr anchor="ctr"/><a:p><a:pPr algn="ctr"/><a:r><a:rPr sz="3200" b="1"/><a:t>Bonjour</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#;
        let mut z = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for (n, b) in [("ppt/presentation.xml", r#"<p:presentation><p:sldSz cx="10000" cy="5000"/></p:presentation>"#), ("ppt/slides/slide1.xml", slide)] {
            z.start_file(n, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(b.as_bytes()).unwrap();
        }
        z.finish().unwrap();
        let d = load(&path).unwrap();
        assert_eq!(d.size, [10000.0, 5000.0]);
        let s = &d.slides[0].shapes[0];
        assert_eq!(s.rect, [100.0, 200.0, 5000.0, 1000.0]);
        assert_eq!(s.fill, Some(Color32::from_rgb(255, 0, 0)));
        assert_eq!(s.anchor, 1);
        let Kind::Text(ps) = &s.kind else { panic!() };
        assert_eq!(ps[0].align, 1);
        assert_eq!(ps[0].runs[0].size, Some(32.0));
        assert!(ps[0].runs[0].bold);
        save(&path, &[Edit { part: "ppt/slides/slide1.xml".into(), sp_index: 0, lines: Some(vec!["Salut & toi".into(), "deux".into()]), off: Some([300, 400]) }]).unwrap();
        let d = load(&path).unwrap();
        let s = &d.slides[0].shapes[0];
        assert_eq!(s.rect[0], 300.0);
        let Kind::Text(ps) = &s.kind else { panic!() };
        assert_eq!(ps.iter().map(|p| p.text()).collect::<Vec<_>>(), vec!["Salut & toi", "deux"]);
        assert_eq!(ps[1].runs[0].size, Some(32.0), "le style du premier paragraphe est repris");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
