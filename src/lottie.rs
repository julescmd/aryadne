//! Export en Lottie (JSON, pour le web et les applications) et en SVG animé (CSS + SMIL, lisible par un
//! navigateur) des compositions Motion et des transitions entre plans du vecteur.
//!
//! Les deux formats partent de la même scène « cuite » : chaque calque est échantillonné image par image
//! (parenté, expressions, tremblement, IK et bouclage compris), puis les clés inutiles sont retirées.

use crate::motion::{self, Compo, Kind, Prop, Xform};
use serde_json::{Value, json};
use std::fmt::Write as _;

/// Un contour de Bézier : points, poignées d'entrée et de sortie (relatives au point)
#[derive(Clone, Debug, PartialEq)]
pub struct Bez {
    pub v: Vec<[f32; 2]>,
    pub i: Vec<[f32; 2]>,
    pub o: Vec<[f32; 2]>,
    pub closed: bool,
}

impl Bez {
    /// Une ligne brisée (poignées nulles)
    pub fn poly(pts: &[[f32; 2]], closed: bool) -> Bez {
        Bez { v: pts.to_vec(), i: vec![[0.0; 2]; pts.len()], o: vec![[0.0; 2]; pts.len()], closed }
    }

    fn d(&self) -> String {
        let mut d = String::new();
        let n = self.v.len();
        for k in 0..n {
            let p = self.v[k];
            if k == 0 {
                let _ = write!(d, "M{} {}", num(p[0]), num(p[1]));
                continue;
            }
            self.seg(&mut d, k - 1, k);
        }
        if self.closed && n > 1 {
            self.seg(&mut d, n - 1, 0);
            d.push('Z');
        }
        d
    }

    fn seg(&self, d: &mut String, a: usize, b: usize) {
        let (p, q) = (self.v[a], self.v[b]);
        if self.o[a] == [0.0; 2] && self.i[b] == [0.0; 2] {
            let _ = write!(d, "L{} {}", num(q[0]), num(q[1]));
        } else {
            let _ = write!(d, "C{} {} {} {} {} {}", num(p[0] + self.o[a][0]), num(p[1] + self.o[a][1]), num(q[0] + self.i[b][0]), num(q[1] + self.i[b][1]), num(q[0]), num(q[1]));
        }
    }

    fn json(&self) -> Value {
        let r = |p: &[f32; 2]| json!([round(p[0]), round(p[1])]);
        json!({ "i": self.i.iter().map(r).collect::<Vec<_>>(), "o": self.o.iter().map(r).collect::<Vec<_>>(), "v": self.v.iter().map(r).collect::<Vec<_>>(), "c": self.closed })
    }
}

/// La géométrie d'un élément, dans le repère de son calque
#[derive(Clone, Debug, PartialEq)]
pub enum Geo {
    /// Centré sur l'origine
    Rect { w: f32, h: f32, r: f32 },
    Ellipse { w: f32, h: f32 },
    /// Des contours : une seule pose, ou une par image (forme animée)
    Paths(Vec<Vec<Bez>>),
    /// Image centrée sur l'origine, en URI de données
    Image { w: f32, h: f32, uri: String },
}

/// Un élément dessiné d'un calque
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub geo: Geo,
    pub fill: Option<[u8; 4]>,
    pub stroke: Option<([u8; 4], f32)>,
    /// Animation propre (lettres qui arrivent) : décalage x, y, échelle, opacité, par image ; vide : aucune
    pub anim: Vec<[f32; 4]>,
    /// Centre de l'échelle de `anim`
    pub center: [f32; 2],
}

/// Un calque cuit. Les listes par image n'ont qu'une valeur quand rien ne bouge.
#[derive(Clone, Debug, PartialEq)]
pub struct SLayer {
    pub name: String,
    /// Première image visible, et première image où il ne l'est plus
    pub ip: u32,
    pub op: u32,
    pub parent: Option<usize>,
    pub local: Vec<Xform>,
    pub world: Vec<Xform>,
    /// 0..1
    pub opacity: Vec<f32>,
    /// Tracé progressif : début et fin en %, par image ; vide : entier
    pub trim: Vec<[f32; 2]>,
    pub items: Vec<Item>,
}

/// La scène cuite : le premier calque est au-dessus
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Scene {
    pub w: f32,
    pub h: f32,
    pub fps: f32,
    pub frames: u32,
    pub bg: Option<[u8; 3]>,
    pub layers: Vec<SLayer>,
    /// Ce qui n'a pas pu être exporté (à montrer après l'export)
    pub notes: Vec<String>,
}

fn round(v: f32) -> f64 {
    if !v.is_finite() {
        return 0.0;
    }
    (v as f64 * 100.0).round() / 100.0
}

/// Un nombre court pour le SVG (2 décimales, sans zéros inutiles)
fn num(v: f32) -> String {
    let s = format!("{:.2}", if v.is_finite() { v } else { 0.0 });
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

fn at<T: Copy>(v: &[T], f: usize) -> Option<T> {
    v.get(f.min(v.len().saturating_sub(1))).copied()
}

/// Retire les valeurs en double : une liste où rien ne bouge n'en garde qu'une
fn collapse<T: PartialEq>(v: &mut Vec<T>) {
    if v.len() > 1 && v.iter().all(|x| *x == v[0]) {
        v.truncate(1);
    }
}

/// Les images à garder pour qu'une interpolation linéaire entre elles reste à moins de `eps` (par composante)
/// de chaque échantillon (Ramer-Douglas-Peucker, sans récursion)
pub fn simplify(samples: &[Vec<f32>], eps: &[f32]) -> Vec<usize> {
    let n = samples.len();
    if n <= 2 {
        return (0..n).collect();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0usize, n - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (mut worst, mut wi) = (0.0f32, a);
        for k in a + 1..b {
            let f = (k - a) as f32 / (b - a) as f32;
            for (c, (va, vb)) in samples[a].iter().zip(&samples[b]).enumerate() {
                let e = eps.get(c).or(eps.last()).copied().unwrap_or(0.01).max(1e-6);
                let d = (samples[k].get(c).copied().unwrap_or(0.0) - (va + (vb - va) * f)).abs() / e;
                if d > worst {
                    (worst, wi) = (d, k);
                }
            }
        }
        if worst > 1.0 {
            keep[wi] = true;
            stack.push((a, wi));
            stack.push((wi, b));
        }
    }
    (0..n).filter(|&k| keep[k]).collect()
}

/// Angles (degrés) rendus continus d'une image à l'autre : pas de saut de 359° à 0°
fn unwrap_angles(a: &mut [f32]) {
    for k in 1..a.len() {
        let d = a[k] - a[k - 1];
        a[k] -= (d / 360.0).round() * 360.0;
    }
}

// ---------- Depuis Motion ----------

/// Les contours d'une lettre, posée à (x, y) (pied de la lettre), à la taille de la police mise à l'échelle
fn glyph_contours(f: &ab_glyph::FontArc, ch: char, size: f32, x: f32, y: f32) -> Vec<Bez> {
    use ab_glyph::{Font, ScaleFont};
    let sf = f.as_scaled(ab_glyph::PxScale::from(size.max(1.0)));
    let (kx, ky) = (sf.h_scale_factor(), sf.v_scale_factor());
    let Some(ol) = f.outline(f.glyph_id(ch)) else { return vec![] };
    let tp = |p: ab_glyph::Point| [x + p.x * kx, y - p.y * ky];
    let mut out: Vec<Bez> = vec![];
    let mut last: Option<[f32; 2]> = None;
    for c in &ol.curves {
        use ab_glyph::OutlineCurve::*;
        // Tout en cubiques : (départ, poignée de sortie, poignée d'entrée, arrivée)
        let (a, c1, c2, b) = match c {
            Line(a, b) => (tp(*a), tp(*a), tp(*b), tp(*b)),
            Quad(a, q, b) => {
                let (a, q, b) = (tp(*a), tp(*q), tp(*b));
                (a, [a[0] + (q[0] - a[0]) * 2.0 / 3.0, a[1] + (q[1] - a[1]) * 2.0 / 3.0], [b[0] + (q[0] - b[0]) * 2.0 / 3.0, b[1] + (q[1] - b[1]) * 2.0 / 3.0], b)
            }
            Cubic(a, c1, c2, b) => (tp(*a), tp(*c1), tp(*c2), tp(*b)),
        };
        let near = |p: [f32; 2], q: [f32; 2]| (p[0] - q[0]).abs() < 0.01 && (p[1] - q[1]).abs() < 0.01;
        if !last.is_some_and(|l| near(l, a)) {
            out.push(Bez { v: vec![a], i: vec![[0.0; 2]], o: vec![[0.0; 2]], closed: true });
        }
        let Some(cur) = out.last_mut() else { continue };
        let n = cur.v.len();
        cur.o[n - 1] = [c1[0] - a[0], c1[1] - a[1]];
        if near(b, cur.v[0]) {
            // Retour au départ : le contour se ferme sur son premier point
            cur.i[0] = [c2[0] - b[0], c2[1] - b[1]];
        } else {
            cur.v.push(b);
            cur.i.push([c2[0] - b[0], c2[1] - b[1]]);
            cur.o.push([0.0; 2]);
        }
        last = Some(b);
    }
    out.retain(|b| b.v.len() > 1);
    out
}

fn rgba(c: [u8; 3]) -> [u8; 4] {
    [c[0], c[1], c[2], 255]
}

/// Cuit une composition Motion. `with_bg` : le fond de la composition est gardé (sinon transparent).
pub fn bake_compo(c: &Compo, cache: &mut motion::Cache, with_bg: bool) -> Scene {
    c.publish_controls();
    let fps = c.fps.max(1.0);
    let n = c.frames().max(1);
    let t = crate::i18n::t;
    let mut notes: Vec<String> = vec![];
    let note = |notes: &mut Vec<String>, name: &str, what: &str| {
        let s = format!("{name} : {}", t(what));
        if !notes.contains(&s) {
            notes.push(s);
        }
    };
    if c.blur.is_some() {
        notes.push(t("Le flou de mouvement n'est pas exporté."));
    }
    let mut layers = vec![];
    for (i, l) in c.layers.iter().enumerate() {
        let times = (0..n).map(|f| f as f32 / fps);
        let mut local: Vec<Xform> = times.clone().map(|tt| c.local(i, tt)).collect();
        let mut world: Vec<Xform> = times.clone().map(|tt| c.world(i, tt)).collect();
        let mut opacity: Vec<f32> = times.clone().map(|tt| (l.eval_prop(Prop::Opacity, tt)[0] / 100.0).clamp(0.0, 1.0)).collect();
        collapse(&mut local);
        collapse(&mut world);
        collapse(&mut opacity);
        let mut items = vec![];
        let mut trim: Vec<[f32; 2]> = vec![];
        let style_stroke = l.style.stroke.as_ref().filter(|s| s.width > 0.0).map(|s| (rgba(s.color), s.width));
        if l.visible {
            match &l.kind {
                Kind::Rect { w, h, radius, color } => items.push(Item { geo: Geo::Rect { w: *w, h: *h, r: *radius }, fill: Some(rgba(*color)), stroke: style_stroke, anim: vec![], center: [0.0; 2] }),
                Kind::Ellipse { w, h, color } => items.push(Item { geo: Geo::Ellipse { w: *w, h: *h }, fill: Some(rgba(*color)), stroke: style_stroke, anim: vec![], center: [0.0; 2] }),
                Kind::Path { points, closed, width, color, arrow, fill, keys } => {
                    let pose = |tt: f32| vec![Bez::poly(&motion::smooth_path(&motion::points_at(points, keys, tt), *closed), *closed)];
                    let mut poses: Vec<Vec<Bez>> = if keys.is_empty() { vec![pose(0.0)] } else { times.clone().map(pose).collect() };
                    collapse(&mut poses);
                    items.push(Item { geo: Geo::Paths(poses), fill: fill.filter(|_| *closed).map(rgba), stroke: (*width > 0.0).then_some((rgba(*color), *width)), anim: vec![], center: [0.0; 2] });
                    trim = times.clone().map(|tt| { let v = l.value(&l.trim, tt); [v[1].min(v[0]).clamp(0.0, 100.0), v[0].max(v[1]).clamp(0.0, 100.0)] }).collect();
                    collapse(&mut trim);
                    if trim == [[0.0, 100.0]] {
                        trim.clear();
                    }
                    if *arrow {
                        note(&mut notes, &l.name, "la pointe de flèche n'est pas exportée");
                    }
                }
                Kind::Text { text, size, color, font: fname, tracking, align } => {
                    use ab_glyph::{Font, ScaleFont};
                    let spec = motion::font_spec(l, fname, l.start);
                    let by = l.reveal.as_ref().map(|r| r.by).unwrap_or_default();
                    let lay = motion::layout_text(text, *size, &spec, *tracking, *align, by);
                    let f = motion::font(&spec);
                    let sf = f.as_scaled(ab_glyph::PxScale::from(size.max(1.0)));
                    let (ox, oy) = (-lay.w / 2.0, -lay.h / 2.0);
                    for g in &lay.glyphs {
                        let contours = glyph_contours(f, g.ch, *size, g.x + ox, g.y + oy);
                        if contours.is_empty() {
                            continue;
                        }
                        let mut anim: Vec<[f32; 4]> = match &l.reveal {
                            Some(r) => times
                                .clone()
                                .map(|tt| {
                                    let x = (tt - l.start - r.rank(g.unit, lay.units) * r.delay) / r.dur.max(1e-3);
                                    let e = r.ease.apply(x);
                                    [r.slide * (1.0 - e), r.rise * (1.0 - e), r.scale + (1.0 - r.scale) * e, motion::Ease::Linear.apply(x * 1.6).clamp(0.0, 1.0)]
                                })
                                .collect(),
                            None => vec![],
                        };
                        collapse(&mut anim);
                        if anim == [[0.0, 0.0, 1.0, 1.0]] {
                            anim.clear();
                        }
                        let center = [g.x + sf.h_advance(f.glyph_id(g.ch)) / 2.0 + ox, g.y - size * 0.35 + oy];
                        items.push(Item { geo: Geo::Paths(vec![contours]), fill: Some(rgba(*color)), stroke: style_stroke, anim, center });
                    }
                    if !l.font_axes.is_empty() && l.font_axes.values().any(|a| a.animated()) {
                        note(&mut notes, &l.name, "les axes animés de la police sont figés");
                    }
                }
                Kind::Image { path } => match cache.image(path) {
                    Some(img) => {
                        let mut png = std::io::Cursor::new(vec![]);
                        if img.write_to(&mut png, image::ImageFormat::Png).is_ok() {
                            use base64::Engine;
                            let uri = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png.get_ref()));
                            items.push(Item { geo: Geo::Image { w: img.width() as f32, h: img.height() as f32, uri }, fill: None, stroke: None, anim: vec![], center: [0.0; 2] });
                        }
                    }
                    None => note(&mut notes, &l.name, "image introuvable"),
                },
                Kind::Null => {}
                Kind::Noise { .. } => note(&mut notes, &l.name, "le bruit fractal n'est pas exporté"),
                Kind::Compo { .. } => note(&mut notes, &l.name, "la précomposition n'est pas exportée (exportez-la à part)"),
            }
            if !l.masks.is_empty() {
                note(&mut notes, &l.name, "les masques ne sont pas exportés");
            }
            if l.matte.is_some() {
                note(&mut notes, &l.name, "le cache de piste n'est pas exporté");
            }
            if !l.fx.is_empty() {
                note(&mut notes, &l.name, "les effets de déformation ne sont pas exportés");
            }
            if l.repeat.is_some() {
                note(&mut notes, &l.name, "le répétiteur n'est pas exporté");
            }
            if l.style.shadow.is_some() || l.style.glow.is_some() {
                note(&mut notes, &l.name, "l'ombre et la lueur ne sont pas exportées");
            }
            if l.depth.is_some() {
                note(&mut notes, &l.name, "la profondeur 3D est ignorée");
            }
        }
        let ip = (l.start * fps).round().max(0.0) as u32;
        let op = ((l.end * fps).round().max(0.0) as u32).min(n).max(ip);
        layers.push(SLayer { name: l.name.clone(), ip: ip.min(n), op, parent: l.parent.and_then(|p| c.index_of(p)).filter(|p| *p != i), local, world, opacity, trim, items });
    }
    Scene { w: c.w as f32, h: c.h as f32, fps, frames: n, bg: with_bg.then_some(c.bg), layers, notes }
}

// ---------- Depuis le vecteur ----------

/// Les formes d'un plan en éléments (repère du plan), du fond vers le dessus
fn vector_items(doc: &crate::vector::Doc, board: &crate::vector::Board, notes: &mut Vec<String>) -> Vec<Item> {
    use crate::vector::{Kind as VK, Paint};
    let t = crate::i18n::t;
    let mut note = |s: String| {
        if !notes.contains(&s) {
            notes.push(s);
        }
    };
    let mut items = vec![];
    if let Some(bg) = board.bg {
        items.push(Item { geo: Geo::Paths(vec![vec![Bez::poly(&[[0.0, 0.0], [board.w, 0.0], [board.w, board.h], [0.0, board.h]], true)]]), fill: Some(bg), stroke: None, anim: vec![], center: [0.0; 2] });
    }
    for s in &doc.shapes {
        // Opacité et visibilité héritées des groupes
        let (mut alpha, mut shown, mut cur) = (s.opacity, s.visible, s.parent);
        for _ in 0..32 {
            let Some(p) = doc.get(cur).filter(|_| cur != 0) else { break };
            alpha *= p.opacity;
            shown &= p.visible;
            if matches!(&p.kind, VK::Group { op, .. } if *op != 0) {
                note(format!("{} : {}", p.name, t("l'opération booléenne du groupe n'est pas exportée en Lottie")));
            }
            cur = p.parent;
        }
        if !shown || alpha <= 0.001 {
            continue;
        }
        // Hors du plan : rien à dessiner
        let r = doc.bounds(s);
        if r[2] < board.x || r[0] > board.x + board.w || r[3] < board.y || r[1] > board.y + board.h {
            continue;
        }
        let subs = match &s.kind {
            VK::Group { .. } => continue,
            VK::Image { .. } => {
                note(format!("{} : {}", s.name, t("les images ne sont pas exportées en Lottie")));
                continue;
            }
            VK::Text { .. } => {
                let (d, _) = crate::vector::text_outline(s);
                let mut tmp = crate::vector::Doc { shapes: vec![], ..crate::vector::Doc::starter() };
                let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg"><path d="{d}" fill="black"/></svg>"#);
                if d.is_empty() || crate::vector::import_svg(&mut tmp, &svg, [0.0, 0.0]).is_err() {
                    continue;
                }
                tmp.shapes.into_iter().filter_map(|sh| match sh.kind { VK::Path { nodes, closed } => Some((nodes, closed)), _ => None }).collect()
            }
            _ => crate::vector::shape_subpaths(s),
        };
        let (sn, cs) = s.rot.to_radians().sin_cos();
        let tp = |p: [f32; 2]| [s.x + p[0] * cs - p[1] * sn - board.x, s.y + p[0] * sn + p[1] * cs - board.y];
        let tv = |p: [f32; 2], q: [f32; 2]| { let (a, b) = (tp(p), tp(q)); [a[0] - b[0], a[1] - b[1]] };
        let contours: Vec<Bez> = subs.iter().filter(|(n, _)| n.len() > 1).map(|(nodes, closed)| Bez { v: nodes.iter().map(|n| tp(n.p)).collect(), i: nodes.iter().map(|n| tv(n.i, n.p)).collect(), o: nodes.iter().map(|n| tv(n.o, n.p)).collect(), closed: *closed }).collect();
        if contours.is_empty() {
            continue;
        }
        let fade = |c: [u8; 4]| [c[0], c[1], c[2], (c[3] as f32 * alpha).round().clamp(0.0, 255.0) as u8];
        if s.fills.iter().chain(s.strokes.iter().map(|k| &k.paint)).any(|p| !matches!(p, Paint::Solid { .. })) {
            note(format!("{} : {}", s.name, t("les dégradés et motifs deviennent une couleur unie en Lottie")));
        }
        let open_only = contours.iter().all(|b| !b.closed);
        let fill = s.fills.last().map(|p| fade(p.first())).filter(|_| !open_only || !matches!(s.kind, VK::Line { .. }));
        let stroke = s.strokes.first().filter(|k| k.width > 0.0).map(|k| (fade(k.paint.first()), k.width));
        if fill.is_none() && stroke.is_none() {
            continue;
        }
        items.push(Item { geo: Geo::Paths(vec![contours]), fill, stroke, anim: vec![], center: [0.0; 2] });
    }
    items
}

/// Cuit l'animation des plans du vecteur (mêmes règles que le GIF : les plans de la taille du premier, tenus
/// `hold` secondes, avec une transition de `trans` secondes) : un calque par image, visible le temps qu'il faut
pub fn bake_boards(doc: &crate::vector::Doc, fps: f32, hold: f32, trans: f32) -> Scene {
    let Some(first) = doc.boards.first() else { return Scene::default() };
    let fps = fps.max(1.0);
    let order: Vec<usize> = (0..doc.boards.len()).filter(|&i| (doc.boards[i].w - first.w).abs() < 0.5 && (doc.boards[i].h - first.h).abs() < 0.5).collect();
    let n_hold = (hold * fps).round().max(1.0) as u32;
    let n_trans = (trans * fps).round().max(1.0) as u32;
    let mut notes = vec![];
    // Du premier au dernier instant, puis retourné : le premier calque de la scène est au-dessus
    let mut layers: Vec<SLayer> = vec![];
    let mut f = 0u32;
    let push = |layers: &mut Vec<SLayer>, name: String, items: Vec<Item>, f: &mut u32, len: u32| {
        layers.push(SLayer { name, ip: *f, op: *f + len, parent: None, local: vec![Xform::ID], world: vec![Xform::ID], opacity: vec![1.0], trim: vec![], items });
        *f += len;
    };
    for (k, &a) in order.iter().enumerate() {
        let b = &doc.boards[a];
        push(&mut layers, b.name.clone(), vector_items(doc, b, &mut notes), &mut f, n_hold);
        if let Some(&nb) = order.get(k + 1) {
            for j in 1..n_trans {
                if let Some(d) = crate::vector::smart_frame(doc, a, nb, j as f32 / n_trans as f32) {
                    if let Some(bd) = d.boards.first() {
                        push(&mut layers, format!("{} › {} {j}", b.name, doc.boards[nb].name), vector_items(&d, bd, &mut notes), &mut f, 1);
                    }
                }
            }
        }
    }
    layers.reverse();
    Scene { w: first.w, h: first.h, fps, frames: f, bg: None, layers, notes }
}

// ---------- Lottie ----------

fn color_json(c: [u8; 4]) -> Value {
    json!([round(c[0] as f32 / 255.0), round(c[1] as f32 / 255.0), round(c[2] as f32 / 255.0), 1])
}

fn fixed(v: Value) -> Value {
    json!({ "a": 0, "k": v })
}

/// Une propriété Lottie : fixe si rien ne bouge, sinon des clés linéaires (une par image gardée)
fn lottie_prop(samples: &[Vec<f32>], eps: &[f32]) -> Value {
    let val = |s: &Vec<f32>| if s.len() == 1 { json!(round(s[0])) } else { json!(s.iter().map(|v| round(*v)).collect::<Vec<_>>()) };
    let Some(first) = samples.first() else { return fixed(json!(0)) };
    if samples.iter().all(|s| s == first) {
        return fixed(val(first));
    }
    let keep = simplify(samples, eps);
    let last = keep.len() - 1;
    let keys: Vec<Value> = keep
        .iter()
        .enumerate()
        .map(|(n, &k)| {
            let s: Vec<f64> = samples[k].iter().map(|v| round(*v)).collect();
            if n == last { json!({ "t": k, "s": s }) } else { json!({ "t": k, "s": s, "i": { "x": [1], "y": [1] }, "o": { "x": [0], "y": [0] } }) }
        })
        .collect();
    json!({ "a": 1, "k": keys })
}

fn lottie_item(it: &Item, trim: &[[f32; 2]], frames: usize, assets: &mut Vec<Value>) -> Option<Value> {
    let mut parts: Vec<Value> = vec![];
    match &it.geo {
        Geo::Rect { w, h, r } => parts.push(json!({ "ty": "rc", "d": 1, "p": fixed(json!([0, 0])), "s": fixed(json!([round(*w), round(*h)])), "r": fixed(json!(round(*r))) })),
        Geo::Ellipse { w, h } => parts.push(json!({ "ty": "el", "d": 1, "p": fixed(json!([0, 0])), "s": fixed(json!([round(*w), round(*h)])) })),
        Geo::Paths(poses) => {
            let n = poses.first().map(|p| p.len()).unwrap_or(0);
            for ci in 0..n {
                let ks = if poses.len() <= 1 || poses.iter().any(|p| p.len() != n) {
                    fixed(poses[0][ci].json())
                } else {
                    // Une pose par image : des clés tenues quand le nombre de points change, linéaires sinon
                    let same = poses.iter().all(|p| p[ci].v.len() == poses[0][ci].v.len());
                    let last = poses.len() - 1;
                    let keys: Vec<Value> = poses.iter().enumerate().map(|(k, p)| if k == last { json!({ "t": k, "s": [p[ci].json()] }) } else if same { json!({ "t": k, "s": [p[ci].json()], "i": { "x": 1, "y": 1 }, "o": { "x": 0, "y": 0 } }) } else { json!({ "t": k, "s": [p[ci].json()], "h": 1 }) }).collect();
                    json!({ "a": 1, "k": keys })
                };
                parts.push(json!({ "ty": "sh", "ks": ks }));
            }
        }
        Geo::Image { .. } => return None,
    }
    let _ = assets;
    if !trim.is_empty() {
        let s: Vec<Vec<f32>> = (0..frames.max(1)).map(|f| vec![at(trim, f).unwrap_or([0.0, 100.0])[0]]).collect();
        let e: Vec<Vec<f32>> = (0..frames.max(1)).map(|f| vec![at(trim, f).unwrap_or([0.0, 100.0])[1]]).collect();
        parts.push(json!({ "ty": "tm", "s": lottie_prop(&s, &[0.2]), "e": lottie_prop(&e, &[0.2]), "o": fixed(json!(0)), "m": 1 }));
    }
    if let Some(c) = it.fill {
        parts.push(json!({ "ty": "fl", "c": fixed(color_json(c)), "o": fixed(json!(round(c[3] as f32 / 2.55))), "r": 1 }));
    }
    if let Some((c, w)) = it.stroke {
        parts.push(json!({ "ty": "st", "c": fixed(color_json(c)), "o": fixed(json!(round(c[3] as f32 / 2.55))), "w": fixed(json!(round(w))), "lc": 2, "lj": 2 }));
    }
    // La transformation du groupe : l'animation propre de l'élément (lettres qui arrivent)
    let fr = if it.anim.len() > 1 { frames.max(1) } else { 1 };
    let a = |f: usize| at(&it.anim, f).unwrap_or([0.0, 0.0, 1.0, 1.0]);
    let p: Vec<Vec<f32>> = (0..fr).map(|f| vec![it.center[0] + a(f)[0], it.center[1] + a(f)[1]]).collect();
    let s: Vec<Vec<f32>> = (0..fr).map(|f| vec![a(f)[2] * 100.0, a(f)[2] * 100.0]).collect();
    let o: Vec<Vec<f32>> = (0..fr).map(|f| vec![a(f)[3] * 100.0]).collect();
    parts.push(json!({ "ty": "tr", "a": fixed(json!([round(it.center[0]), round(it.center[1])])), "p": lottie_prop(&p, &[0.25]), "s": lottie_prop(&s, &[0.3]), "r": fixed(json!(0)), "o": lottie_prop(&o, &[0.5]) }));
    Some(json!({ "ty": "gr", "it": parts }))
}

/// La scène en Lottie (format Bodymovin 5)
pub fn to_lottie(s: &Scene, name: &str) -> Value {
    let frames = s.frames.max(1) as usize;
    let mut assets: Vec<Value> = vec![];
    let mut layers: Vec<Value> = vec![];
    for (i, l) in s.layers.iter().enumerate() {
        let fr = if l.local.len() > 1 { frames } else { 1 };
        let x = |f: usize| at(&l.local, f).unwrap_or(Xform::ID);
        let pos: Vec<Vec<f32>> = (0..fr).map(|f| vec![x(f).e, x(f).f]).collect();
        // Sans cisaillement : rotation, puis échelle par axe (négative en y pour un miroir)
        let mut ang: Vec<f32> = (0..fr).map(|f| x(f).angle()).collect();
        unwrap_angles(&mut ang);
        let rot: Vec<Vec<f32>> = ang.iter().map(|a| vec![*a]).collect();
        let scale: Vec<Vec<f32>> = (0..fr).map(|f| { let m = x(f); let sx = m.a.hypot(m.b); let sy = m.c.hypot(m.d) * if m.a * m.d - m.b * m.c < 0.0 { -1.0 } else { 1.0 }; vec![sx * 100.0, sy * 100.0] }).collect();
        let op: Vec<Vec<f32>> = (0..if l.opacity.len() > 1 { frames } else { 1 }).map(|f| vec![at(&l.opacity, f).unwrap_or(1.0) * 100.0]).collect();
        let mut ks = json!({ "o": lottie_prop(&op, &[0.5]), "r": lottie_prop(&rot, &[0.1]), "p": lottie_prop(&pos, &[0.25]), "a": fixed(json!([0, 0])), "s": lottie_prop(&scale, &[0.2]) });
        let mut v = json!({ "ddd": 0, "ind": i + 1, "nm": l.name, "sr": 1, "ao": 0, "ip": l.ip, "op": l.op, "st": 0, "bm": 0 });
        if let Some(p) = l.parent {
            v["parent"] = json!(p + 1);
        }
        match l.items.first().map(|it| &it.geo) {
            Some(Geo::Image { w, h, uri }) => {
                let id = format!("img_{}", assets.len());
                assets.push(json!({ "id": id, "w": w.round(), "h": h.round(), "u": "", "p": uri, "e": 1 }));
                ks["a"] = fixed(json!([round(w / 2.0), round(h / 2.0)]));
                v["ty"] = json!(2);
                v["refId"] = json!(id);
            }
            Some(_) => {
                v["ty"] = json!(4);
                // Dans un groupe Lottie, le premier élément est au-dessus
                v["shapes"] = json!(l.items.iter().rev().filter_map(|it| lottie_item(it, &l.trim, frames, &mut assets)).collect::<Vec<_>>());
            }
            None => v["ty"] = json!(3),
        }
        v["ks"] = ks;
        layers.push(v);
    }
    if let Some(bg) = s.bg {
        layers.push(json!({ "ddd": 0, "ind": s.layers.len() + 1, "ty": 1, "nm": "fond", "sr": 1, "ao": 0, "ip": 0, "op": s.frames, "st": 0, "bm": 0,
            "sw": s.w.round(), "sh": s.h.round(), "sc": format!("#{:02x}{:02x}{:02x}", bg[0], bg[1], bg[2]),
            "ks": { "o": fixed(json!(100)), "r": fixed(json!(0)), "p": fixed(json!([0, 0])), "a": fixed(json!([0, 0])), "s": fixed(json!([100, 100])) } }));
    }
    json!({ "v": "5.7.4", "fr": round(s.fps), "ip": 0, "op": s.frames, "w": s.w.round(), "h": s.h.round(), "nm": name, "ddd": 0, "assets": assets, "layers": layers, "markers": [] })
}

// ---------- SVG animé ----------

fn css_color(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn matrix(m: Xform) -> String {
    format!("matrix({},{},{},{},{},{})", num4(m.a), num4(m.b), num4(m.c), num4(m.d), num(m.e), num(m.f))
}

fn num4(v: f32) -> String {
    let s = format!("{:.4}", if v.is_finite() { v } else { 0.0 });
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

fn pct(f: usize, frames: usize) -> String {
    num4(f as f32 * 100.0 / frames.max(1) as f32)
}

/// Les arrêts CSS des images gardées : l'image k est à k / frames de la durée, et la dernière tient jusqu'à 100 %
fn key_stops(keep: &[usize], frames: usize) -> Vec<(usize, String)> {
    let mut v: Vec<(usize, String)> = keep.iter().map(|&k| (k, pct(k, frames))).collect();
    if let Some(&last) = keep.last() {
        if last < frames {
            v.push((last, "100".into()));
        }
    }
    v
}

fn at_f(v: &[[f32; 2]], f: usize) -> [f32; 2] {
    at(v, f).unwrap_or([0.0, 100.0])
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// La scène en SVG animé : les transformations et opacités en animations CSS, les formes qui changent en SMIL.
/// L'animation tourne en boucle.
pub fn to_svg(s: &Scene) -> String {
    let frames = s.frames.max(1) as usize;
    let dur = num4(frames as f32 / s.fps.max(1.0));
    let (mut css, mut body) = (String::new(), String::new());
    for (i, l) in s.layers.iter().enumerate().rev() {
        if l.items.is_empty() || l.op <= l.ip {
            continue;
        }
        let mut anims: Vec<String> = vec![];
        // Transformation et opacité du calque
        let moving = l.world.len() > 1 || l.opacity.len() > 1;
        let state = |f: usize| (at(&l.world, f).unwrap_or(Xform::ID), at(&l.opacity, f).unwrap_or(1.0));
        if moving {
            let samples: Vec<Vec<f32>> = (0..frames).map(|f| { let (m, o) = state(f); vec![m.a, m.b, m.c, m.d, m.e, m.f, o] }).collect();
            let keep = simplify(&samples, &[0.002, 0.002, 0.002, 0.002, 0.25, 0.25, 0.005]);
            let _ = write!(css, "@keyframes a{i}{{");
            for (k, at) in key_stops(&keep, frames) {
                let (m, o) = state(k);
                let _ = write!(css, "{at}%{{transform:{};opacity:{}}}", matrix(m), num4(o));
            }
            css.push_str("}\n");
            anims.push(format!("a{i} {dur}s linear infinite"));
        }
        // Visible seulement entre ses deux bornes
        if l.ip > 0 || (l.op as usize) < frames {
            let _ = write!(css, "@keyframes v{i}{{0%{{visibility:{}}}", if l.ip == 0 { "visible" } else { "hidden" });
            if l.ip > 0 {
                let _ = write!(css, "{}%{{visibility:visible}}", pct(l.ip as usize, frames));
            }
            if (l.op as usize) < frames {
                let _ = write!(css, "{}%{{visibility:hidden}}", pct(l.op as usize, frames));
            }
            css.push_str("}\n");
            anims.push(format!("v{i} {dur}s step-end infinite"));
        }
        let (m, o) = state(0);
        let mut style = String::new();
        if m != Xform::ID {
            let _ = write!(style, "transform:{};", matrix(m));
        }
        if o < 1.0 {
            let _ = write!(style, "opacity:{};", num4(o));
        }
        if !anims.is_empty() {
            let _ = write!(css, "#l{i}{{animation:{}}}\n", anims.join(","));
        }
        let _ = write!(body, "<g id=\"l{i}\"");
        if !style.is_empty() {
            let _ = write!(body, " style=\"{style}\"");
        }
        let _ = write!(body, "><title>{}</title>", xml(&l.name));
        for (j, it) in l.items.iter().enumerate() {
            let mut paint = String::new();
            match it.fill {
                Some(c) => {
                    let _ = write!(paint, " fill=\"{}\"", css_color(c));
                    if c[3] < 255 {
                        let _ = write!(paint, " fill-opacity=\"{}\"", num4(c[3] as f32 / 255.0));
                    }
                }
                None => paint.push_str(" fill=\"none\""),
            }
            if let Some((c, w)) = it.stroke {
                let _ = write!(paint, " stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"", css_color(c), num(w));
                if c[3] < 255 {
                    let _ = write!(paint, " stroke-opacity=\"{}\"", num4(c[3] as f32 / 255.0));
                }
            }
            // Tracé progressif : un seul tiret, long comme la partie visible
            let mut trim_anim = String::new();
            if !l.trim.is_empty() && it.stroke.is_some() {
                let dash = |t: [f32; 2]| (format!("{} 100", num((t[1] - t[0]).max(0.0))), num(-t[0]));
                let (da, off) = dash(l.trim[0]);
                let _ = write!(paint, " pathLength=\"100\" stroke-dasharray=\"{da}\" stroke-dashoffset=\"{off}\"");
                if l.trim.len() > 1 {
                    let samples: Vec<Vec<f32>> = (0..frames).map(|f| at(&l.trim, f).unwrap_or([0.0, 100.0]).to_vec()).collect();
                    let keep = simplify(&samples, &[0.2]);
                    let _ = write!(css, "@keyframes t{i}_{j}{{");
                    for (k, at) in key_stops(&keep, frames) {
                        let (da, off) = dash(at_f(&l.trim, k));
                        let _ = write!(css, "{at}%{{stroke-dasharray:{da};stroke-dashoffset:{off}}}");
                    }
                    css.push_str("}\n");
                    trim_anim = format!(" style=\"animation:t{i}_{j} {dur}s linear infinite\"");
                }
            }
            // L'animation propre de l'élément (lettres qui arrivent) : un groupe de plus
            let own = it.anim.len() > 1;
            if own {
                let st = |f: usize| { let a = at(&it.anim, f).unwrap_or([0.0, 0.0, 1.0, 1.0]); (Xform { a: a[2], b: 0.0, c: 0.0, d: a[2], e: it.center[0] * (1.0 - a[2]) + a[0], f: it.center[1] * (1.0 - a[2]) + a[1] }, a[3]) };
                let samples: Vec<Vec<f32>> = (0..frames).map(|f| { let (m, o) = st(f); vec![m.a, m.e, m.f, o] }).collect();
                let keep = simplify(&samples, &[0.002, 0.25, 0.25, 0.005]);
                let _ = write!(css, "@keyframes g{i}_{j}{{");
                for (k, at) in key_stops(&keep, frames) {
                    let (m, o) = st(k);
                    let _ = write!(css, "{at}%{{transform:{};opacity:{}}}", matrix(m), num4(o));
                }
                css.push_str("}\n");
                let (m, o) = st(0);
                let _ = write!(body, "<g style=\"transform:{};opacity:{};animation:g{i}_{j} {dur}s linear infinite\">", matrix(m), num4(o));
            }
            match &it.geo {
                Geo::Rect { w, h, r } => {
                    let _ = write!(body, "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"{paint}{trim_anim}/>", num(-w / 2.0), num(-h / 2.0), num(*w), num(*h), num(r.min(w / 2.0).min(h / 2.0)));
                }
                Geo::Ellipse { w, h } => {
                    let _ = write!(body, "<ellipse rx=\"{}\" ry=\"{}\"{paint}{trim_anim}/>", num(w / 2.0), num(h / 2.0));
                }
                Geo::Paths(poses) => {
                    let d = |p: &Vec<Bez>| p.iter().map(|b| b.d()).collect::<String>();
                    let Some(first) = poses.first() else { continue };
                    let _ = write!(body, "<path d=\"{}\"{paint}{trim_anim}", d(first));
                    if poses.len() > 1 {
                        // Une pose par image, la dernière tenue jusqu'au bout
                        let mut values: Vec<String> = poses.iter().map(d).collect();
                        let mut times: Vec<String> = (0..values.len()).map(|k| num4((k as f32 / frames as f32).min(1.0))).collect();
                        if values.len() <= frames {
                            values.push(values[values.len() - 1].clone());
                            times.push("1".into());
                        }
                        let _ = write!(body, "><animate attributeName=\"d\" dur=\"{dur}s\" repeatCount=\"indefinite\" values=\"{}\" keyTimes=\"{}\"/></path>", values.join(";"), times.join(";"));
                    } else {
                        body.push_str("/>");
                    }
                }
                Geo::Image { w, h, uri } => {
                    let _ = write!(body, "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" href=\"{uri}\"/>", num(-w / 2.0), num(-h / 2.0), num(*w), num(*h));
                }
            }
            if own {
                body.push_str("</g>");
            }
        }
        body.push_str("</g>\n");
    }
    let bg = s.bg.map(|c| format!("<rect width=\"100%\" height=\"100%\" fill=\"#{:02x}{:02x}{:02x}\"/>\n", c[0], c[1], c[2])).unwrap_or_default();
    format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\">\n<style>\n{css}</style>\n{bg}{body}</svg>\n", w = num(s.w), h = num(s.h))
}

/// Écrit la scène : `.json` en Lottie, tout le reste en SVG animé. Rend ce qui n'a pas pu être exporté.
pub fn write(s: &Scene, out: &std::path::Path) -> Result<Vec<String>, String> {
    if s.frames == 0 {
        return Err(crate::i18n::t("Rien à exporter."));
    }
    let name = out.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let text = if out.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) { to_lottie(s, &name).to_string() } else { to_svg(s) };
    crate::vault::atomic_write(out, &text).map_err(|e| e.to_string())?;
    Ok(s.notes.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion::{Key, Layer, Track};

    fn compo() -> Compo {
        let mut c = Compo { w: 400, h: 300, fps: 10.0, duration: 2.0, layers: vec![], ..Compo::default() };
        // Un carré qui glisse et tourne, enfant d'un nul décalé
        let mut nul = Layer::new("nul", Kind::Null, &c);
        nul.pos = Track::new(100.0, 50.0);
        nul.end = 2.0;
        let mut sq = Layer::new("carré", Kind::Rect { w: 40.0, h: 20.0, radius: 4.0, color: [200, 30, 30] }, &c);
        sq.parent = Some(nul.id);
        sq.end = 2.0;
        sq.pos = Track { v: [0.0, 0.0], keys: vec![Key { t: 0.0, v: [0.0, 0.0], ease: motion::Ease::Linear, tan: vec![] }, Key { t: 1.0, v: [100.0, 0.0], ease: motion::Ease::Linear, tan: vec![] }] };
        sq.rot = Track { v: [0.0, 0.0], keys: vec![Key { t: 0.0, v: [0.0, 0.0], ease: motion::Ease::Linear, tan: vec![] }, Key { t: 2.0, v: [720.0, 0.0], ease: motion::Ease::Linear, tan: vec![] }] };
        let mut line = Layer::new("ligne", Kind::Path { points: vec![[-50.0, 0.0], [0.0, 30.0], [50.0, 0.0]], closed: false, width: 4.0, color: [0, 0, 0], arrow: false, fill: None, keys: vec![] }, &c);
        line.start = 0.5;
        line.end = 1.5;
        line.trim = Track { v: [100.0, 0.0], keys: vec![Key { t: 0.5, v: [0.0, 0.0], ease: motion::Ease::Linear, tan: vec![] }, Key { t: 1.5, v: [100.0, 0.0], ease: motion::Ease::Linear, tan: vec![] }] };
        let mut txt = Layer::new("titre", Kind::Text { text: "Ab".into(), size: 40.0, color: [10, 20, 30], font: "inter".into(), tracking: 0.0, align: 0 }, &c);
        txt.end = 2.0;
        txt.reveal = Some(motion::Reveal::default());
        c.layers = vec![txt, line, sq, nul];
        c
    }

    #[test]
    fn simplifier_garde_les_coudes() {
        // Une rampe puis un plateau : trois clés suffisent
        let s: Vec<Vec<f32>> = (0..21).map(|k| vec![(k as f32).min(10.0)]).collect();
        assert_eq!(simplify(&s, &[0.01]), vec![0, 10, 20]);
        assert_eq!(simplify(&s[..1], &[0.01]), vec![0]);
        let mut a = vec![350.0, 359.0, 8.0, 17.0];
        unwrap_angles(&mut a);
        assert_eq!(a, vec![350.0, 359.0, 368.0, 377.0]);
    }

    #[test]
    fn lottie_d_une_composition() {
        let c = compo();
        let mut cache = motion::Cache::new(std::path::Path::new("/nulle-part"));
        let s = bake_compo(&c, &mut cache, true);
        assert_eq!((s.frames, s.layers.len()), (20, 4));
        // Le carré garde son parent, sa position locale est animée, celle du nul est fixe
        assert_eq!(s.layers[2].parent, Some(3));
        assert_eq!((s.layers[2].local.len(), s.layers[3].local.len()), (20, 1));
        assert_eq!((s.layers[1].ip, s.layers[1].op), (5, 15));
        let v = to_lottie(&s, "essai");
        assert_eq!(v["fr"], 10.0);
        assert_eq!(v["op"], 20);
        let ls = v["layers"].as_array().unwrap();
        // Quatre calques et le fond
        assert_eq!(ls.len(), 5);
        assert_eq!(ls[4]["ty"], 1);
        assert_eq!(ls[3]["ty"], 3);
        let sq = &ls[2];
        assert_eq!((sq["ty"].as_i64(), sq["parent"].as_i64()), (Some(4), Some(4)));
        // Position : linéaire jusqu'à 1 s puis fixe, donc 3 clés ; rotation continue jusqu'à 2 tours sans retour à 0
        let p = sq["ks"]["p"]["k"].as_array().unwrap();
        assert_eq!(p.len(), 3, "{p:?}");
        assert_eq!(p[1]["t"], 10);
        assert_eq!(p[1]["s"][0], 100.0);
        let r = sq["ks"]["r"]["k"].as_array().unwrap();
        assert!(r.last().unwrap()["s"][0].as_f64().unwrap() > 600.0, "{r:?}");
        assert_eq!(sq["shapes"][0]["it"][0]["ty"], "rc");
        // La ligne : tracé progressif animé, sans remplissage
        let it = ls[1]["shapes"][0]["it"].as_array().unwrap();
        let kinds: Vec<&str> = it.iter().map(|x| x["ty"].as_str().unwrap()).collect();
        assert_eq!(kinds, vec!["sh", "tm", "st", "tr"]);
        assert_eq!(it[1]["e"]["a"], 1);
        // Le texte : une lettre par groupe, en contours, chacune avec son arrivée
        let letters = ls[0]["shapes"].as_array().unwrap();
        assert_eq!(letters.len(), 2);
        assert_eq!(letters[0]["it"].as_array().unwrap().last().unwrap()["o"]["a"], 1);
        // Tout est du JSON relisible, sans valeur non finie
        let back: Value = serde_json::from_str(&v.to_string()).unwrap();
        assert_eq!(back, v);
        assert!(!v.to_string().contains("null"));
    }

    #[test]
    fn svg_anime_d_une_composition() {
        let c = compo();
        let mut cache = motion::Cache::new(std::path::Path::new("/nulle-part"));
        let s = bake_compo(&c, &mut cache, false);
        let svg = to_svg(&s);
        assert!(svg.starts_with("<svg") && svg.contains("@keyframes a2{") && svg.contains("#l2{animation:a2 2s linear infinite}"), "{svg}");
        // La ligne n'est visible qu'entre ses bornes, et son tiret s'allonge
        assert!(svg.contains("@keyframes v1{0%{visibility:hidden}25%{visibility:visible}75%{visibility:hidden}}"), "{svg}");
        assert!(svg.contains("stroke-dasharray") && svg.contains("@keyframes t1_0{"));
        // Sans fond : pas de rectangle plein cadre ; et le rendu par resvg accepte le fichier
        assert!(!svg.contains("width=\"100%\""));
        assert!(crate::vector::rasterize(&svg, 400, 300).is_some());
        // Le nul n'a rien à dessiner
        assert!(!svg.contains("id=\"l3\""));
    }

    #[test]
    fn plans_du_vecteur_en_lottie_et_svg() {
        use crate::vector::{Board, Doc, Kind as VK, Paint, Shape};
        let mut d = Doc::default();
        for (i, x) in [0.0f32, 200.0].iter().enumerate() {
            d.boards.push(Board { name: format!("P{i}"), x: *x, y: 0.0, w: 40.0, h: 30.0, bg: Some([255; 4]), grid: None, persp: None, device: 0, mode: 0 });
            d.shapes.push(Shape { id: i as u64 + 1, name: "carré".into(), x: x + 5.0 + i as f32 * 20.0, y: 5.0, kind: VK::Rect { w: 10.0, h: 10.0, r: [0.0; 4] }, fills: vec![Paint::Solid { c: [200, 0, 0, 255] }], ..Shape::base() });
        }
        let s = bake_boards(&d, 10.0, 0.5, 0.5);
        // 5 images tenues par plan, 4 de transition : comme le GIF
        assert_eq!(s.frames, 14);
        assert_eq!(s.layers.len(), 6);
        // Le dernier calque de la scène est le premier plan, tenu 5 images, fond et carré
        let first = s.layers.last().unwrap();
        assert_eq!((first.ip, first.op, first.items.len()), (0, 5, 2));
        // Pendant la transition le carré est entre ses deux places (5 et 25)
        let mid = &s.layers[3];
        let Geo::Paths(p) = &mid.items[1].geo else { panic!() };
        let x = p[0][0].v.iter().map(|v| v[0]).fold(f32::MAX, f32::min);
        assert!(x > 6.0 && x < 24.0, "{x}");
        let v = to_lottie(&s, "plans");
        assert_eq!(v["layers"].as_array().unwrap().len(), 6);
        assert_eq!(v["layers"][5]["shapes"].as_array().unwrap().len(), 2);
        let svg = to_svg(&s);
        assert!(svg.contains("step-end infinite") && crate::vector::rasterize(&svg, 40, 30).is_some());
        assert!(bake_boards(&Doc::default(), 10.0, 0.5, 0.5).frames == 0 || !Doc::default().boards.is_empty());
    }
}
