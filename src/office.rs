//! Aperçu des documents Office sans Office : PowerPoint (.pptx), Excel (.xlsx),
//! Word (.docx) et leurs équivalents LibreOffice (.odp, .ods, .odt).
//! On lit le contenu (textes, images, cellules) directement dans l'archive.
use eframe::egui::{self, RichText, TextureHandle};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

pub const EXT: [&str; 7] = ["pptx", "xlsx", "docx", "odp", "ods", "odt", "key"];

pub fn is_office(rel: &str) -> bool {
    let e = rel.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    EXT[..6].contains(&e.as_str())
}

// ---------- mini lecteur XML ----------

pub enum Tok<'a> {
    Open(&'a str, &'a str),
    Close(&'a str),
    Text(String),
}

/// Découpe un XML en balises et textes (suffisant pour les fichiers Office).
pub fn tokens(xml: &str) -> Vec<Tok<'_>> {
    let mut out = vec![];
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        if start > 0 {
            out.push(Tok::Text(unescape(&rest[..start])));
        }
        let Some(end) = rest[start..].find('>') else { break };
        let tag = &rest[start + 1..start + end];
        rest = &rest[start + end + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            out.push(Tok::Close(local(name.trim())));
        } else {
            let self_closing = tag.ends_with('/');
            let tag = tag.trim_end_matches('/');
            let (name, attrs) = tag.split_once(char::is_whitespace).unwrap_or((tag, ""));
            out.push(Tok::Open(local(name), attrs));
            if self_closing {
                out.push(Tok::Close(local(name)));
            }
        }
    }
    out
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

pub fn attr<'a>(attrs: &'a str, key: &str) -> Option<&'a str> {
    // Cherche « key="…" » ou « prefix:key="…" »
    let mut s = attrs;
    while let Some(eq) = s.find('=') {
        let name = s[..eq].trim().rsplit(char::is_whitespace).next().unwrap_or("");
        let after = s[eq + 1..].trim_start();
        let q = after.chars().next()?;
        let close = after[1..].find(q)?;
        let value = &after[1..1 + close];
        if local(name) == key {
            return Some(value);
        }
        s = &after[close + 2..];
    }
    None
}

pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find(';') else {
            out.push_str(&rest[i..]);
            return out;
        };
        let ent = &rest[i + 1..i + j];
        match ent {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                let code = ent.strip_prefix("#x").and_then(|h| u32::from_str_radix(h, 16).ok()).or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()));
                match code.and_then(char::from_u32) {
                    Some(c) => out.push(c),
                    None => out.push_str(&rest[i..i + j + 1]),
                }
            }
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out
}

// ---------- archive ----------

struct Archive(zip::ZipArchive<std::io::BufReader<std::fs::File>>);

impl Archive {
    fn open(path: &Path) -> Result<Self, String> {
        let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
        zip::ZipArchive::new(std::io::BufReader::new(f)).map(Self).map_err(|_| "Document illisible (archive Office invalide ou protégée).".to_string())
    }
    fn bytes(&mut self, name: &str) -> Option<Vec<u8>> {
        let mut f = self.0.by_name(name).ok()?;
        let mut v = vec![];
        f.read_to_end(&mut v).ok()?;
        Some(v)
    }
    fn text(&mut self, name: &str) -> Option<String> {
        self.bytes(name).map(|b| String::from_utf8_lossy(&b).to_string())
    }
    fn names(&self) -> Vec<String> {
        self.0.file_names().filter_map(|n| n.ok().map(|c| c.to_string())).collect()
    }
}

/// Résout « ../media/image1.png » par rapport au dossier d'une partie.
fn resolve(base_dir: &str, target: &str) -> String {
    let mut parts: Vec<&str> = base_dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn rels(ar: &mut Archive, part: &str) -> HashMap<String, String> {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    let Some(xml) = ar.text(&format!("{dir}/_rels/{file}.rels")) else { return HashMap::new() };
    tokens(&xml)
        .into_iter()
        .filter_map(|t| match t {
            Tok::Open("Relationship", a) => Some((attr(a, "Id")?.to_string(), resolve(dir, attr(a, "Target")?))),
            _ => None,
        })
        .collect()
}

// ---------- documents ----------

pub struct Slide {
    pub texts: Vec<String>,
    pub images: Vec<Vec<u8>>,
}

pub struct Sheet {
    pub name: String,
    pub rows: Vec<Vec<String>>,
}

pub enum Doc {
    Slides(Vec<Slide>),
    Sheets(Vec<Sheet>),
    Text(Vec<(u8, String)>, Vec<Vec<u8>>),
}

fn num_in(name: &str) -> usize {
    name.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)
}

pub fn load(path: &Path) -> Result<Doc, String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut ar = Archive::open(path)?;
    match ext.as_str() {
        "pptx" => {
            let mut names: Vec<String> = ar.names().into_iter().filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml")).collect();
            names.sort_by_key(|n| num_in(n));
            let mut slides = vec![];
            for n in names {
                let xml = ar.text(&n).unwrap_or_default();
                let r = rels(&mut ar, &n);
                let mut texts = vec![];
                let mut para = String::new();
                let mut in_t = false;
                let mut images = vec![];
                for t in tokens(&xml) {
                    match t {
                        Tok::Open("t", _) => in_t = true,
                        Tok::Close("t") => in_t = false,
                        Tok::Text(s) if in_t => para.push_str(&s),
                        Tok::Close("p") => {
                            if !para.trim().is_empty() {
                                texts.push(std::mem::take(&mut para));
                            }
                            para.clear();
                        }
                        Tok::Open("blip", a) => {
                            if let Some(target) = attr(a, "embed").and_then(|id| r.get(id)) {
                                if let Some(b) = ar.bytes(target) {
                                    images.push(b);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                slides.push(Slide { texts, images });
            }
            if slides.is_empty() {
                return Err("Aucune diapositive.".into());
            }
            Ok(Doc::Slides(slides))
        }
        "xlsx" => {
            let shared: Vec<String> = {
                let xml = ar.text("xl/sharedStrings.xml").unwrap_or_default();
                let mut out = vec![];
                let (mut cur, mut in_t) = (String::new(), false);
                for t in tokens(&xml) {
                    match t {
                        Tok::Open("si", _) => cur.clear(),
                        Tok::Close("si") => out.push(std::mem::take(&mut cur)),
                        Tok::Open("t", _) => in_t = true,
                        Tok::Close("t") => in_t = false,
                        Tok::Text(s) if in_t => cur.push_str(&s),
                        _ => {}
                    }
                }
                out
            };
            let wb = ar.text("xl/workbook.xml").unwrap_or_default();
            let wb_rels = rels(&mut ar, "xl/workbook.xml");
            let sheets_meta: Vec<(String, String)> = tokens(&wb)
                .into_iter()
                .filter_map(|t| match t {
                    Tok::Open("sheet", a) => Some((unescape(attr(a, "name")?), wb_rels.get(attr(a, "id")?)?.clone())),
                    _ => None,
                })
                .collect();
            let mut sheets = vec![];
            for (name, part) in sheets_meta {
                let xml = ar.text(&part).unwrap_or_default();
                let mut rows: Vec<Vec<String>> = vec![];
                let (mut col, mut kind, mut val, mut in_v) = (0usize, String::new(), String::new(), false);
                for t in tokens(&xml) {
                    match t {
                        Tok::Open("row", _) => {
                            if rows.len() >= 500 {
                                break;
                            }
                            rows.push(vec![]);
                        }
                        Tok::Open("c", a) => {
                            col = attr(a, "r").map(col_index).unwrap_or_else(|| rows.last().map(|r| r.len()).unwrap_or(0));
                            kind = attr(a, "t").unwrap_or("").to_string();
                            val.clear();
                        }
                        Tok::Open("v", _) | Tok::Open("t", _) => in_v = true,
                        Tok::Close("v") | Tok::Close("t") => in_v = false,
                        Tok::Text(s) if in_v => val.push_str(&s),
                        Tok::Close("c") => {
                            let shown = match kind.as_str() {
                                "s" => val.trim().parse::<usize>().ok().and_then(|i| shared.get(i).cloned()).unwrap_or_default(),
                                "b" => if val.trim() == "1" { "VRAI".into() } else { "FAUX".into() },
                                _ => val.clone(),
                            };
                            if let Some(r) = rows.last_mut() {
                                if col < 60 {
                                    while r.len() < col {
                                        r.push(String::new());
                                    }
                                    r.push(shown);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                sheets.push(Sheet { name, rows });
            }
            if sheets.is_empty() {
                return Err("Aucune feuille.".into());
            }
            Ok(Doc::Sheets(sheets))
        }
        "docx" => {
            let xml = ar.text("word/document.xml").ok_or("document.xml introuvable.")?;
            let r = rels(&mut ar, "word/document.xml");
            let mut paras = vec![];
            let mut images = vec![];
            let (mut cur, mut level, mut in_t) = (String::new(), 0u8, false);
            for t in tokens(&xml) {
                match t {
                    Tok::Open("p", _) => {
                        cur.clear();
                        level = 0;
                    }
                    Tok::Open("pStyle", a) => {
                        let v = attr(a, "val").unwrap_or("").to_lowercase();
                        if v.contains("title") || v.contains("titre") {
                            level = 1;
                        } else if let Some(n) = v.strip_prefix("heading").or_else(|| v.strip_prefix("titre")) {
                            level = n.trim().parse().unwrap_or(2);
                        }
                    }
                    Tok::Open("t", _) => in_t = true,
                    Tok::Close("t") => in_t = false,
                    Tok::Open("tab", _) => cur.push('\t'),
                    Tok::Text(s) if in_t => cur.push_str(&s),
                    Tok::Close("p") => paras.push((level, std::mem::take(&mut cur))),
                    Tok::Open("blip", a) => {
                        if let Some(target) = attr(a, "embed").and_then(|id| r.get(id)) {
                            if let Some(b) = ar.bytes(target) {
                                images.push(b);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Doc::Text(paras, images))
        }
        "odp" | "ods" | "odt" => {
            // LibreOffice : textes de content.xml, une « page » par diapo ou feuille.
            let xml = ar.text("content.xml").ok_or("content.xml introuvable.")?;
            let mut paras = vec![];
            let mut cur = String::new();
            let mut depth_p = 0;
            for t in tokens(&xml) {
                match t {
                    Tok::Open("p", _) | Tok::Open("h", _) => depth_p += 1,
                    Tok::Close("p") | Tok::Close("h") => {
                        depth_p -= 1;
                        if depth_p == 0 && !cur.trim().is_empty() {
                            paras.push((0, std::mem::take(&mut cur)));
                        }
                    }
                    Tok::Open("page", a) | Tok::Open("table", a) => paras.push((1, unescape(attr(a, "name").unwrap_or("")))),
                    Tok::Text(s) if depth_p > 0 => cur.push_str(&s),
                    _ => {}
                }
            }
            let images = ar.bytes("Thumbnails/thumbnail.png").into_iter().collect();
            Ok(Doc::Text(paras, images))
        }
        _ => Err("Format non pris en charge.".into()),
    }
}

/// « AB12 » → index de colonne 27
fn col_index(r: &str) -> usize {
    let mut n = 0usize;
    for c in r.chars().take_while(|c| c.is_ascii_alphabetic()) {
        n = n * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1);
    }
    n.saturating_sub(1)
}

/// Miniature pour les boards et les refs : celle enregistrée par Office/LibreOffice,
/// sinon la première image du document.
pub fn thumbnail(path: &Path) -> Result<image::DynamicImage, String> {
    let mut ar = Archive::open(path)?;
    for name in ["docProps/thumbnail.jpeg", "docProps/thumbnail.jpg", "docProps/thumbnail.png", "Thumbnails/thumbnail.png"] {
        if let Some(b) = ar.bytes(name) {
            if let Ok(img) = image::load_from_memory(&b) {
                return Ok(img);
            }
        }
    }
    let mut media: Vec<String> = ar.names().into_iter().filter(|n| n.contains("/media/") || n.starts_with("Pictures/")).collect();
    media.sort_by_key(|n| num_in(n));
    for m in media {
        if let Some(img) = ar.bytes(&m).and_then(|b| image::load_from_memory(&b).ok()) {
            return Ok(img);
        }
    }
    Err("pas d'aperçu enregistré".into())
}

// ---------- affichage ----------

pub struct Viewer {
    pub doc: Result<Doc, String>,
    tex: HashMap<(usize, usize), Option<TextureHandle>>,
    sheet: usize,
}

impl Viewer {
    pub fn open(path: &Path) -> Self {
        Self { doc: load(path), tex: HashMap::new(), sheet: 0 }
    }

    fn texture(&mut self, ctx: &egui::Context, key: (usize, usize), bytes: &[u8]) -> Option<TextureHandle> {
        self.tex
            .entry(key)
            .or_insert_with(|| {
                let img = image::load_from_memory(bytes).ok()?;
                let img = if img.width() > 1600 || img.height() > 1600 { img.thumbnail(1600, 1600) } else { img };
                let rgba = img.to_rgba8();
                Some(ctx.load_texture(format!("office{key:?}"), egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()), egui::TextureOptions::LINEAR))
            })
            .clone()
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let ctx = ui.ctx().clone();
        let doc = match &self.doc {
            Ok(d) => d,
            Err(e) => {
                ui.label(RichText::new(e).color(p.missing));
                return;
            }
        };
        match doc {
            Doc::Slides(slides) => {
                let n = slides.len();
                let data: Vec<(Vec<String>, Vec<Vec<u8>>)> = slides.iter().map(|s| (s.texts.clone(), s.images.clone())).collect();
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    egui::Frame::new().inner_margin(24).show(ui, |ui| {
                        ui.label(RichText::new(format!("{n} diapositives")).color(p.muted).size(11.5));
                        for (i, (texts, imgs)) in data.iter().enumerate() {
                            ui.add_space(12.0);
                            let w = ui.available_width().min(820.0);
                            egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(20).show(ui, |ui| {
                                ui.set_width(w - 40.0);
                                ui.label(RichText::new(format!("diapo {}", i + 1)).color(p.accent).size(11.0));
                                for (k, t) in texts.iter().enumerate() {
                                    let rt = RichText::new(t);
                                    ui.label(if k == 0 { rt.size(20.0).family(crate::theme::bold()) } else { rt.size(13.5) });
                                }
                                ui.horizontal_wrapped(|ui| {
                                    for (k, b) in imgs.iter().enumerate() {
                                        if let Some(t) = self.texture(&ctx, (i, k), b) {
                                            let s = t.size_vec2();
                                            let scale = (260.0 / s.y).min((w - 60.0) / s.x).min(1.0);
                                            ui.add(egui::Image::new(&t).fit_to_exact_size(s * scale));
                                        }
                                    }
                                });
                            });
                        }
                    });
                });
            }
            Doc::Sheets(sheets) => {
                let names: Vec<String> = sheets.iter().map(|s| s.name.clone()).collect();
                ui.horizontal(|ui| {
                    for (i, n) in names.iter().enumerate() {
                        if ui.selectable_label(self.sheet == i, n).clicked() {
                            self.sheet = i;
                        }
                    }
                });
                let Doc::Sheets(sheets) = &self.doc.as_ref().unwrap() else { return };
                let Some(sheet) = sheets.get(self.sheet) else { return };
                let cols = sheet.rows.iter().map(|r| r.len()).max().unwrap_or(0);
                egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                    egui::Grid::new("xlsx").striped(true).min_col_width(60.0).spacing([0.0, 0.0]).show(ui, |ui| {
                        ui.label("");
                        for c in 0..cols {
                            ui.label(RichText::new(col_name(c)).color(p.muted).size(11.0));
                        }
                        ui.end_row();
                        for (ri, row) in sheet.rows.iter().enumerate() {
                            ui.label(RichText::new(format!("{} ", ri + 1)).color(p.muted).size(11.0));
                            for c in 0..cols {
                                let v = row.get(c).map(String::as_str).unwrap_or("");
                                egui::Frame::new().stroke(egui::Stroke::new(0.5, p.line)).inner_margin(egui::Margin::symmetric(6, 3)).show(ui, |ui| {
                                    ui.set_min_width(80.0);
                                    ui.add(egui::Label::new(RichText::new(v).size(12.0)).truncate());
                                });
                            }
                            ui.end_row();
                        }
                    });
                    if sheet.rows.len() >= 500 {
                        ui.label(RichText::new("(500 premières lignes)").color(p.muted).size(11.0));
                    }
                });
            }
            Doc::Text(paras, images) => {
                let paras = paras.clone();
                let images = images.clone();
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(40, 24)).show(ui, |ui| {
                        ui.set_max_width(780.0_f32.min(ui.available_width()));
                        for (k, b) in images.iter().enumerate().take(1) {
                            if let Some(t) = self.texture(&ctx, (0, k), b) {
                                let s = t.size_vec2();
                                ui.add(egui::Image::new(&t).fit_to_exact_size(s * (300.0 / s.y).min(1.0)));
                            }
                        }
                        for (level, t) in paras {
                            if t.trim().is_empty() {
                                ui.add_space(6.0);
                                continue;
                            }
                            let rt = RichText::new(t);
                            ui.label(match level {
                                1 => rt.size(22.0).family(crate::theme::bold()),
                                2 => rt.size(17.0).family(crate::theme::bold()),
                                3 => rt.size(15.0).family(crate::theme::bold()),
                                _ => rt.size(13.5),
                            });
                        }
                        for (k, b) in images.iter().enumerate().skip(1).take(30) {
                            if let Some(t) = self.texture(&ctx, (0, k), b) {
                                let s = t.size_vec2();
                                ui.add(egui::Image::new(&t).fit_to_exact_size(s * (400.0 / s.x).min(1.0)));
                            }
                        }
                    });
                });
            }
        }
    }
}

fn col_name(mut c: usize) -> String {
    let mut s = String::new();
    c += 1;
    while c > 0 {
        let r = (c - 1) % 26;
        s.insert(0, (b'A' + r as u8) as char);
        c = (c - 1) / 26;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip(path: &Path, files: &[(&str, &[u8])]) {
        let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (n, b) in files {
            z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(b).unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn pptx_xlsx_docx() {
        let dir = std::env::temp_dir().join("aryadne-office-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut png = vec![];
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(4, 3, image::Rgb([200, 10, 10]))).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();

        let slide = br#"<p:sld xmlns:a="a" xmlns:r="r"><a:p><a:r><a:t>Pitch &amp; d&#233;mo</a:t></a:r></a:p><a:p><a:r><a:t>Point 1</a:t></a:r></a:p><p:pic><a:blip r:embed="rId2"/></p:pic></p:sld>"#;
        let rels = br#"<Relationships><Relationship Id="rId2" Target="../media/image1.png"/></Relationships>"#;
        let pptx = dir.join("a.pptx");
        make_zip(&pptx, &[("ppt/slides/slide2.xml", b"<p:sld><a:p><a:r><a:t>Fin</a:t></a:r></a:p></p:sld>"), ("ppt/slides/slide1.xml", slide), ("ppt/slides/_rels/slide1.xml.rels", rels), ("ppt/media/image1.png", &png)]);
        let Doc::Slides(s) = load(&pptx).unwrap() else { panic!() };
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].texts, vec!["Pitch & démo", "Point 1"]);
        assert_eq!(s[0].images.len(), 1);
        assert_eq!(s[1].texts, vec!["Fin"]);
        assert!(thumbnail(&pptx).is_ok(), "la première image sert de miniature");

        let xlsx = dir.join("b.xlsx");
        make_zip(&xlsx, &[
            ("xl/workbook.xml", br#"<workbook><sheets><sheet name="Budget" sheetId="1" r:id="rId1"/></sheets></workbook>"#),
            ("xl/_rels/workbook.xml.rels", br#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#),
            ("xl/sharedStrings.xml", br#"<sst><si><t>Poste</t></si><si><t>Rendu</t></si></sst>"#),
            ("xl/worksheets/sheet1.xml", br#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="C1"><v>42</v></c></row><row r="2"><c r="A2" t="s"><v>1</v></c></row></sheetData></worksheet>"#),
        ]);
        let Doc::Sheets(sh) = load(&xlsx).unwrap() else { panic!() };
        assert_eq!(sh[0].name, "Budget");
        assert_eq!(sh[0].rows[0], vec!["Poste", "", "42"]);
        assert_eq!(sh[0].rows[1], vec!["Rendu"]);

        let docx = dir.join("c.docx");
        make_zip(&docx, &[("word/document.xml", br#"<w:document><w:body><w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Titre</w:t></w:r></w:p><w:p><w:r><w:t xml:space="preserve">Bonjour </w:t></w:r><w:r><w:t>monde</w:t></w:r></w:p></w:body></w:document>"#)]);
        let Doc::Text(paras, _) = load(&docx).unwrap() else { panic!() };
        assert_eq!(paras, vec![(1, "Titre".to_string()), (0, "Bonjour monde".to_string())]);
        assert_eq!(col_index("AB12"), 27);
        assert_eq!(col_name(27), "AB");
    }
}
