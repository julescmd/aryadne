//! JSON Canvas (.canvas, le format ouvert d'Obsidian) : import en board et export d'un board.
//! Les nœuds texte, fichier, lien et groupe deviennent des éléments ; les liens entre nœuds, des flèches.

use crate::board::{BoardFile, Item, Kind, TextLook};
use serde_json::{json, Value};

const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"];

fn is_image(rel: &str) -> bool {
    let e = rel.rsplit('.').next().unwrap_or("").to_lowercase();
    IMAGE_EXT.contains(&e.as_str())
}

/// Couleur d'un nœud ou d'un lien : préréglage « 1 » à « 6 » ou #rrggbb
fn color(v: Option<&Value>) -> [u8; 4] {
    match v.and_then(|c| c.as_str()) {
        Some("1") => [233, 73, 97, 255],
        Some("2") => [233, 151, 63, 255],
        Some("3") => [224, 222, 113, 255],
        Some("4") => [68, 207, 110, 255],
        Some("5") => [83, 223, 221, 255],
        Some("6") => [168, 130, 255, 255],
        Some(h) if h.starts_with('#') && h.len() == 7 => {
            let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(150);
            [c(1), c(3), c(5), 255]
        }
        _ => [150, 150, 150, 255],
    }
}

/// Un trait droit (ou une ligne brisée) en élément du board
fn polyline(pts: &[[f32; 2]], col: [u8; 4], width: f32) -> Item {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in pts {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (x0, y0) = (cx - w / 2.0, cy - h / 2.0);
    let points = pts.iter().map(|p| [(p[0] - x0) / w, (p[1] - y0) / h]).collect();
    Item { kind: Kind::Stroke { points, color: col, width, widths: vec![] }, x: x0, y: y0, w, h }
}

/// Point d'attache d'un côté d'un nœud ; sans côté, le bord le plus proche de l'autre nœud
fn anchor(n: [f32; 4], side: Option<&str>, toward: [f32; 2]) -> [f32; 2] {
    let [x, y, w, h] = n;
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let side = side.map(|s| s.to_string()).unwrap_or_else(|| {
        let (dx, dy) = ((toward[0] - cx) / w.max(1.0), (toward[1] - cy) / h.max(1.0));
        if dx.abs() > dy.abs() { if dx > 0.0 { "right" } else { "left" } } else if dy > 0.0 { "bottom" } else { "top" }.to_string()
    });
    match side.as_str() {
        "top" => [cx, y],
        "bottom" => [cx, y + h],
        "left" => [x, cy],
        _ => [x + w, cy],
    }
}

/// Un fichier .canvas en board. Les chemins de fichiers sont relatifs au coffre, comme dans Obsidian.
pub fn import(text: &str) -> Result<BoardFile, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("canvas illisible : {e}"))?;
    let mut items = vec![];
    let mut boxes: std::collections::HashMap<String, [f32; 4]> = Default::default();
    let nodes = v["nodes"].as_array().cloned().unwrap_or_default();
    // Les groupes d'abord : ils passent sous le reste
    let mut order: Vec<&Value> = nodes.iter().filter(|n| n["type"] == "group").collect();
    order.extend(nodes.iter().filter(|n| n["type"] != "group"));
    for n in order {
        let f = |k: &str| n[k].as_f64().unwrap_or(0.0) as f32;
        let (x, y, w, h) = (f("x"), f("y"), f("width").max(20.0), f("height").max(20.0));
        if let Some(id) = n["id"].as_str() {
            boxes.insert(id.to_string(), [x, y, w, h]);
        }
        let text_item = |t: String, size: f32| Item { kind: Kind::Text { text: t, size, look: TextLook::default() }, x, y, w, h };
        match n["type"].as_str().unwrap_or("") {
            "text" => items.push(text_item(n["text"].as_str().unwrap_or("").to_string(), 18.0)),
            "file" => {
                let rel = n["file"].as_str().unwrap_or("").trim_start_matches('/').to_string();
                if is_image(&rel) {
                    items.push(Item { kind: Kind::Image { rel, flip: false, gray: false, crop: None, guide: 0 }, x, y, w, h });
                } else if rel.to_lowercase().ends_with(".md") {
                    items.push(Item { kind: Kind::Note { rel }, x, y, w, h });
                } else {
                    items.push(text_item(format!("[[{rel}]]"), 16.0));
                }
            }
            "link" => items.push(text_item(n["url"].as_str().unwrap_or("").to_string(), 16.0)),
            "group" => {
                let col = color(n.get("color"));
                items.push(polyline(&[[x, y], [x + w, y], [x + w, y + h], [x, y + h], [x, y]], col, 2.0));
                if let Some(l) = n["label"].as_str().filter(|l| !l.is_empty()) {
                    items.push(Item { kind: Kind::Text { text: l.to_string(), size: 20.0, look: TextLook::default() }, x, y: y - 34.0, w: w.min(400.0), h: 30.0 });
                }
            }
            _ => {}
        }
    }
    for e in v["edges"].as_array().cloned().unwrap_or_default() {
        let (Some(a), Some(b)) = (e["fromNode"].as_str().and_then(|k| boxes.get(k)), e["toNode"].as_str().and_then(|k| boxes.get(k))) else { continue };
        let center = |n: &[f32; 4]| [n[0] + n[2] / 2.0, n[1] + n[3] / 2.0];
        let p0 = anchor(*a, e["fromSide"].as_str(), center(b));
        let p1 = anchor(*b, e["toSide"].as_str(), center(a));
        let col = color(e.get("color"));
        items.push(polyline(&[p0, p1], col, 2.0));
        // Pointe (par défaut au bout, comme dans Obsidian)
        if e["toEnd"].as_str() != Some("none") {
            let (dx, dy) = (p1[0] - p0[0], p1[1] - p0[1]);
            let ang = dy.atan2(dx) + std::f32::consts::PI;
            let s = 14.0f32.min((dx * dx + dy * dy).sqrt() * 0.45);
            let wing = |sg: f32| [p1[0] + (ang + sg * 0.45).cos() * s, p1[1] + (ang + sg * 0.45).sin() * s];
            items.push(polyline(&[wing(-1.0), p1, wing(1.0)], col, 2.0));
        }
        if let Some(l) = e["label"].as_str().filter(|l| !l.is_empty()) {
            let m = [(p0[0] + p1[0]) / 2.0, (p0[1] + p1[1]) / 2.0];
            items.push(Item { kind: Kind::Text { text: l.to_string(), size: 14.0, look: TextLook::default() }, x: m[0] - 60.0, y: m[1] - 12.0, w: 120.0, h: 24.0 });
        }
    }
    let mut b = BoardFile { items, ..Default::default() };
    b.zoom = 1.0;
    Ok(b)
}

/// Un board en .canvas. Les traits à main levée n'ont pas d'équivalent et restent dans le board.
pub fn export(b: &BoardFile) -> String {
    let mut nodes = vec![];
    for (i, it) in b.items.iter().enumerate() {
        let base = |kind: &str| json!({ "id": format!("n{i}"), "type": kind, "x": it.x.round() as i64, "y": it.y.round() as i64, "width": it.w.round() as i64, "height": it.h.round() as i64 });
        let node = match &it.kind {
            Kind::Text { text, .. } => {
                let mut n = base("text");
                n["text"] = json!(text);
                n
            }
            Kind::Image { rel, .. } | Kind::Note { rel } => {
                let mut n = base("file");
                n["file"] = json!(rel);
                n
            }
            Kind::Palette { colors } => {
                let mut n = base("text");
                n["text"] = json!(colors.iter().map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])).collect::<Vec<_>>().join("\n"));
                n
            }
            Kind::Stroke { .. } => continue,
        };
        nodes.push(node);
    }
    serde_json::to_string_pretty(&json!({ "nodes": nodes, "edges": [] })).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn json_canvas() {
        let src = r##"{"nodes":[
            {"id":"a","type":"text","text":"Idée","x":0,"y":0,"width":200,"height":80},
            {"id":"b","type":"file","file":"refs/forêt.jpg","x":400,"y":0,"width":300,"height":200},
            {"id":"c","type":"file","file":"notes/Lio.md","x":0,"y":300,"width":250,"height":150,"color":"4"},
            {"id":"g","type":"group","label":"Ambiance","x":-20,"y":-20,"width":760,"height":500,"color":"#ff8800"}],
            "edges":[{"id":"e","fromNode":"a","toNode":"b","label":"inspire"}]}"##;
        let b = super::import(src).unwrap();
        use crate::board::Kind;
        assert!(matches!(&b.items[0].kind, Kind::Stroke { color, .. } if color[0] == 255), "le groupe passe dessous");
        assert!(b.items.iter().any(|i| matches!(&i.kind, Kind::Image { rel, .. } if rel == "refs/forêt.jpg")));
        assert!(b.items.iter().any(|i| matches!(&i.kind, Kind::Note { rel } if rel == "notes/Lio.md")));
        assert!(b.items.iter().any(|i| matches!(&i.kind, Kind::Text { text, .. } if text == "inspire")));
        // La flèche part du bord droit de « a »
        let arrow = b.items.iter().filter(|i| matches!(i.kind, Kind::Stroke { .. })).nth(1).unwrap();
        assert!((arrow.x - 200.0).abs() < 1.0, "{}", arrow.x);
        let out = super::export(&b);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["nodes"].as_array().unwrap().iter().any(|n| n["file"] == "notes/Lio.md"));
        assert!(v["nodes"].as_array().unwrap().iter().all(|n| n["type"] != "group"));
    }
}
