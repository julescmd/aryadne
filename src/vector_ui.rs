//! L'éditeur de dessin vectoriel : outils à gauche, calques et plans de travail, propriétés à droite,
//! toile infinie au centre avec repères magnétiques, poignées de transformation et édition des points.

use crate::lucide as l;
use crate::vector::{Board, Doc, Kind, Node, Paint, Shadow, Shape, Stroke as VStroke};
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, Sense, Stroke};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
    Select,
    Direct,
    Pen,
    Rect,
    Ellipse,
    Polygon,
    Star,
    Line,
    Text,
    Board,
    Width,
    Builder,
    Pencil,
}

const TOOLS: [(Tool, &str, &str, &str); 13] = [
    (Tool::Select, l::MOUSE_POINTER_2, "sélection", "V"),
    (Tool::Direct, l::MOUSE_POINTER, "sélection directe : les points d'un tracé", "A"),
    (Tool::Pen, l::PEN_TOOL, "plume : clic pour un point, glisser pour une courbe", "P"),
    (Tool::Pencil, l::PENCIL, "crayon : un trait à main levée, lissé (finir près du départ : fermé)", "N"),
    (Tool::Rect, l::SQUARE, "rectangle (Maj : carré)", "R"),
    (Tool::Ellipse, l::CIRCLE, "ellipse (Maj : cercle)", "E"),
    (Tool::Polygon, l::HEXAGON, "polygone", "Y"),
    (Tool::Star, l::STAR, "étoile", "S"),
    (Tool::Line, l::MINUS, "ligne (Maj : angles de 45°)", "L"),
    (Tool::Text, l::TYPE, "texte : clic pour un texte, glisser pour une zone", "T"),
    (Tool::Board, l::FRAME, "plan de travail", "F"),
    (Tool::Width, l::SPLINE, "largeur : glisser sur un contour pour l'épaissir à cet endroit (Alt+clic : retirer le point)", "W"),
    (Tool::Builder, l::SHAPES, "concepteur de formes : glisser sur les zones des formes choisies pour les fusionner (Alt : effacer)", "M"),
];

enum Drag {
    Pan,
    Marquee(Pos2),
    /// Déplacement : point de départ (monde) et positions de départ
    Move { from: [f32; 2], orig: Vec<Shape>, bounds: [f32; 4] },
    /// Mise à l'échelle par une poignée : cadre de départ, coin fixe (0..8), formes de départ
    Scale { handle: usize, b0: [f32; 4], orig: Vec<Shape> },
    Rotate { center: [f32; 2], a0: f32, orig: Vec<Shape> },
    /// Nouvelle forme qu'on tire depuis un point
    Create { from: [f32; 2], id: u64 },
    /// Point (0), poignée d'entrée (1) ou de sortie (2) d'un tracé
    Node { id: u64, k: usize, part: u8 },
    /// La plume : on tire la poignée du dernier point posé
    PenHandle { id: u64 },
    BoardMove { i: usize, from: [f32; 2], orig: Board, shapes: Vec<(u64, f32, f32)> },
    BoardCreate { from: [f32; 2] },
    /// Crayon : le tracé et les points du geste (monde)
    Pencil { id: u64, pts: Vec<[f32; 2]> },
}

pub struct Editor {
    pub path: PathBuf,
    pub doc: Doc,
    root: String,
    pub sel: Vec<u64>,
    sel_board: Option<usize>,
    tool: Tool,
    /// Vue : point du monde au centre, et pixels d'écran par unité
    center: [f32; 2],
    zoom: f32,
    fit: bool,
    tex: Option<egui::TextureHandle>,
    tex_key: u64,
    drag: Option<Drag>,
    /// Tracé en cours avec la plume
    pen: Option<u64>,
    /// Points choisis en sélection directe (forme, indice)
    nodes_sel: Vec<(u64, usize)>,
    undo: Vec<Doc>,
    redo: Vec<Doc>,
    pub save_due: Option<Instant>,
    pub message: Option<String>,
    /// Lignes d'aimantation affichées pendant un geste (verticale ?, position monde)
    snaps: Vec<(bool, f32)>,
    /// Le champ de texte doit prendre le clavier
    focus_text: bool,
    clip: Vec<Shape>,
    export_scale: f32,
    /// Vectorisation d'une image : seuil (0 à 1) et tolérance (px)
    trace_thr: f32,
    trace_detail: f32,
    /// Outil Largeur : forme et point de largeur qu'on tire
    width_edit: Option<(u64, usize)>,
    /// Concepteur de formes : zones des formes choisies (avec leur empreinte) et zones balayées
    builder: Option<(u64, crate::vgeom::Regions)>,
    picked: Vec<u32>,
    /// Surbrillance des zones (zones affichées, texture)
    builder_tex: Option<(Vec<u32>, egui::TextureHandle)>,
    /// Décalage de tracé (unités)
    offset_by: f32,
    /// Bibliothèque d'icônes ouverte : la recherche
    icon_pick: Option<String>,
    /// Présentation du prototype : le plan affiché et son image (clé, texture)
    present: Option<usize>,
    present_tex: Option<(u64, egui::TextureHandle)>,
    /// Un fichier à ouvrir (la composition Motion tirée d'un plan de travail)
    pub open_request: Option<PathBuf>,
}

fn c32(c: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

fn arr(c: Color32) -> [u8; 4] {
    c.to_srgba_unmultiplied()
}

impl Editor {
    pub fn open(path: PathBuf, root: &str) -> Result<Editor, String> {
        let doc = Doc::load(&path)?;
        Ok(Editor {
            path,
            doc,
            root: root.to_string(),
            sel: vec![],
            sel_board: None,
            tool: Tool::Select,
            center: [540.0, 540.0],
            zoom: 0.5,
            fit: true,
            tex: None,
            tex_key: 0,
            drag: None,
            pen: None,
            nodes_sel: vec![],
            undo: vec![],
            redo: vec![],
            save_due: None,
            message: None,
            snaps: vec![],
            focus_text: false,
            clip: vec![],
            export_scale: 2.0,
            trace_thr: 0.5,
            trace_detail: 1.5,
            width_edit: None,
            builder: None,
            picked: vec![],
            builder_tex: None,
            offset_by: 10.0,
            icon_pick: None,
            present: None,
            present_tex: None,
            open_request: None,
        })
    }

    pub fn save_if_due(&mut self, force: bool) {
        if let Some(t) = self.save_due {
            if force || t.elapsed().as_millis() > 600 {
                self.save_due = None;
                if let Err(e) = self.doc.save(&self.path) {
                    self.message = Some(e);
                }
            }
        }
    }

    fn checkpoint(&mut self) {
        self.undo.push(self.doc.clone());
        if self.undo.len() > 80 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn changed(&mut self) {
        self.save_due = Some(Instant::now());
    }

    fn undo(&mut self, redo: bool) {
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        if let Some(d) = from.pop() {
            to.push(std::mem::replace(&mut self.doc, d));
            self.sel.retain(|i| self.doc.get(*i).is_some());
            self.changed();
        }
    }

    /// La forme de plus haut niveau qui contient celle-ci (un clic choisit le groupe entier)
    fn top_of(&self, id: u64) -> u64 {
        let mut cur = id;
        while let Some(s) = self.doc.get(cur) {
            if s.parent == 0 {
                break;
            }
            cur = s.parent;
        }
        cur
    }

    /// La forme sous le point (monde), la plus en avant ; `deep` : sans remonter aux groupes
    fn hit(&self, w: [f32; 2], deep: bool, tol: f32) -> Option<u64> {
        for s in self.doc.shapes.iter().rev() {
            if !s.visible || s.locked || matches!(s.kind, Kind::Group { .. }) {
                continue;
            }
            if self.ancestors_hidden(s) {
                continue;
            }
            let b = crate::vector::local_bounds(s);
            let (cs, sn) = (s.rot.to_radians().cos(), s.rot.to_radians().sin());
            let (dx, dy) = (w[0] - s.x, w[1] - s.y);
            let (lx, ly) = (dx * cs + dy * sn, -dx * sn + dy * cs);
            let sw = s.strokes.iter().map(|k| k.width).fold(0.0f32, f32::max) / 2.0 + tol;
            if lx >= b[0] - sw && lx <= b[2] + sw && ly >= b[1] - sw && ly <= b[3] + sw {
                return Some(if deep { s.id } else { self.top_of(s.id) });
            }
        }
        None
    }

    fn ancestors_hidden(&self, s: &Shape) -> bool {
        let mut p = s.parent;
        while let Some(g) = self.doc.get(p) {
            if !g.visible || g.locked {
                return true;
            }
            p = g.parent;
        }
        false
    }

    fn selection_bounds(&self) -> Option<[f32; 4]> {
        self.doc.bounds_of(&self.sel)
    }

    /// Les lignes où aimanter : bords et centres des autres formes et des plans de travail
    fn snap_lines(&self, exclude: &[u64]) -> (Vec<f32>, Vec<f32>) {
        let mut xs = vec![];
        let mut ys = vec![];
        let mut ex: Vec<u64> = exclude.to_vec();
        for &e in exclude {
            ex.extend(self.doc.descendants(e));
        }
        for s in self.doc.shapes.iter().filter(|s| s.parent == 0 && s.visible && !ex.contains(&s.id)) {
            let b = self.doc.bounds(s);
            xs.extend([b[0], (b[0] + b[2]) / 2.0, b[2]]);
            ys.extend([b[1], (b[1] + b[3]) / 2.0, b[3]]);
        }
        for b in &self.doc.boards {
            xs.extend([b.x, b.x + b.w / 2.0, b.x + b.w]);
            ys.extend([b.y, b.y + b.h / 2.0, b.y + b.h]);
            // Les bords des colonnes de la grille aimantent aussi
            for (a, c) in b.columns() {
                xs.extend([a, c]);
            }
        }
        (xs, ys)
    }

    /// Décalage qui aimante le cadre `b` aux lignes les plus proches (à `tol` près) ; garde les lignes trouvées
    fn snap_box(&mut self, b: [f32; 4], exclude: &[u64]) -> [f32; 2] {
        let tol = 6.0 / self.zoom;
        let (xs, ys) = self.snap_lines(exclude);
        let best = |cands: &[f32], mine: [f32; 3]| -> Option<(f32, f32)> {
            let mut r: Option<(f32, f32)> = None;
            for &c in cands {
                for m in mine {
                    let d = c - m;
                    if d.abs() < tol && r.is_none_or(|(bd, _)| d.abs() < bd.abs()) {
                        r = Some((d, c));
                    }
                }
            }
            r
        };
        let sx = best(&xs, [b[0], (b[0] + b[2]) / 2.0, b[2]]);
        let sy = best(&ys, [b[1], (b[1] + b[3]) / 2.0, b[3]]);
        self.snaps.clear();
        if let Some((_, c)) = sx {
            self.snaps.push((true, c));
        }
        if let Some((_, c)) = sy {
            self.snaps.push((false, c));
        }
        [sx.map(|v| v.0).unwrap_or(0.0), sy.map(|v| v.0).unwrap_or(0.0)]
    }

    fn snap_point(&mut self, p: [f32; 2]) -> [f32; 2] {
        let d = self.snap_box([p[0], p[1], p[0], p[1]], &self.sel.clone());
        [p[0] + d[0], p[1] + d[1]]
    }

    fn new_shape(&mut self, kind: Kind, x: f32, y: f32) -> u64 {
        let id = self.doc.new_id();
        let n = self.doc.shapes.iter().filter(|s| std::mem::discriminant(&s.kind) == std::mem::discriminant(&kind)).count() + 1;
        let mut s = Shape { id, kind, x, y, ..Shape::base() };
        s.name = format!("{} {n}", crate::i18n::t(s.kind_label()));
        match s.kind {
            Kind::Line { .. } => s.strokes = vec![VStroke { paint: Paint::solid([24, 24, 27, 255]), width: 4.0, cap: 1, join: 1, dash: vec![], arrow: false }],
            Kind::Path { .. } => s.strokes = vec![VStroke { paint: Paint::solid([24, 24, 27, 255]), width: 3.0, cap: 1, join: 1, dash: vec![], arrow: false }],
            Kind::Text { .. } => s.fills = vec![Paint::solid([24, 24, 27, 255])],
            _ => s.fills = vec![Paint::solid([236, 108, 89, 255])],
        }
        self.doc.shapes.push(s);
        id
    }

    fn shape_mut(&mut self, id: u64) -> Option<&mut Shape> {
        self.doc.shapes.iter_mut().find(|s| s.id == id)
    }

    /// Le plan de travail passe de `a` à `b` : chaque forme posée dessus suit sa contrainte
    fn apply_constraints(&mut self, a: &Board, b: &Board) {
        let inside = |s: &Shape, d: &Doc| {
            let bb = d.bounds(s);
            let c = [(bb[0] + bb[2]) / 2.0, (bb[1] + bb[3]) / 2.0];
            c[0] >= a.x && c[0] <= a.x + a.w && c[1] >= a.y && c[1] <= a.y + a.h
        };
        let roots: Vec<Shape> = self.doc.shapes.iter().filter(|s| s.parent == 0 && inside(s, &self.doc)).cloned().collect();
        for r in roots {
            let b0 = self.doc.bounds(&r);
            let (x0, x1) = crate::vector::constrain(r.constraint[0], b0[0], b0[2], a.x, a.x + a.w, b.x, b.x + b.w);
            let (y0, y1) = crate::vector::constrain(r.constraint[1], b0[1], b0[3], a.y, a.y + a.h, b.y, b.y + b.h);
            let b1 = [x0, y0, x1, y1];
            let mut ids = vec![r.id];
            ids.extend(self.doc.descendants(r.id));
            for id in ids {
                if let Some(o) = self.doc.get(id).cloned() {
                    self.rescale(&o, b0, b1);
                }
            }
        }
    }

    /// Remet à l'échelle une forme (et un groupe) d'un cadre vers un autre
    fn rescale(&mut self, orig: &Shape, b0: [f32; 4], b1: [f32; 4]) {
        let sx = (b1[2] - b1[0]) / (b0[2] - b0[0]).max(1e-3);
        let sy = (b1[3] - b1[1]) / (b0[3] - b0[1]).max(1e-3);
        let map = |x: f32, y: f32| [b1[0] + (x - b0[0]) * sx, b1[1] + (y - b0[1]) * sy];
        let Some(s) = self.shape_mut(orig.id) else { return };
        let p = map(orig.x, orig.y);
        s.x = p[0];
        s.y = p[1];
        let k = ((sx.abs() * sy.abs()).sqrt()).max(1e-3);
        s.kind = match &orig.kind {
            Kind::Rect { w, h, r } => Kind::Rect { w: (w * sx).abs().max(1.0), h: (h * sy).abs().max(1.0), r: *r },
            Kind::Ellipse { w, h } => Kind::Ellipse { w: (w * sx).abs().max(1.0), h: (h * sy).abs().max(1.0) },
            Kind::Image { path, w, h } => Kind::Image { path: path.clone(), w: (w * sx).abs().max(1.0), h: (h * sy).abs().max(1.0) },
            Kind::Polygon { sides, r } => Kind::Polygon { sides: *sides, r: r * k },
            Kind::Star { points, r1, r2 } => Kind::Star { points: *points, r1: r1 * k, r2: r2 * k },
            Kind::Line { x2, y2 } => Kind::Line { x2: x2 * sx, y2: y2 * sy },
            Kind::Path { nodes, closed } => Kind::Path { nodes: nodes.iter().map(|n| Node { p: [n.p[0] * sx, n.p[1] * sy], i: [n.i[0] * sx, n.i[1] * sy], o: [n.o[0] * sx, n.o[1] * sy] }).collect(), closed: *closed },
            Kind::Text { text, size, font, align, width, spacing, leading } => Kind::Text { text: text.clone(), size: (size * k).max(1.0), font: font.clone(), align: *align, width: width.map(|w| w * sx.abs()), spacing: *spacing, leading: *leading },
            g @ Kind::Group { .. } => g.clone(),
        };
    }

    // ------------------------------------------------------------------ interface

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let key = (self.path.to_string_lossy().to_string(), "vec");
        // Présentation du prototype : seul le plan de travail, les liens se cliquent
        if self.present.is_some() {
            self.present_ui(ui);
            return;
        }
        // Outils à gauche
        egui::Panel::left(egui::Id::unique(("vec-tools", &key))).resizable(false).exact_size(44.0).frame(egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).inner_margin(egui::Margin::symmetric(4, 8))).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for (k, (t, icon, label, hk)) in TOOLS.into_iter().enumerate() {
                    if [2, 3, 8].contains(&k) {
                        ui.add_space(6.0);
                    }
                    let on = self.tool == t;
                    let b = egui::Button::new(egui::RichText::new(icon).size(15.0).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { Color32::TRANSPARENT }).min_size(vec2(32.0, 28.0));
                    if ui.add(b).on_hover_text(format!("{} ({hk})", crate::i18n::t(label))).clicked() {
                        self.set_tool(t);
                    }
                }
            });
        });
        // Propriétés à droite
        egui::Panel::right(egui::Id::unique(("vec-props", &key))).resizable(true).default_size(260.0).min_size(220.0).max_size(420.0).frame(egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).inner_margin(10)).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.props(ui));
        });
        // Calques à gauche de la toile
        egui::Panel::left(egui::Id::unique(("vec-layers", &key))).resizable(true).default_size(200.0).min_size(150.0).max_size(360.0).frame(egui::Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).inner_margin(8)).show(ui, |ui| self.layers(ui));
        self.toolbar(ui);
        self.icon_window(&ui.ctx().clone());
        self.canvas(ui);
        self.save_if_due(false);
        if self.save_due.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(400));
        }
    }

    fn set_tool(&mut self, t: Tool) {
        if self.pen.is_some() && t != Tool::Pen {
            self.pen = None;
        }
        self.tool = t;
        self.nodes_sel.clear();
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        ui.horizontal(|ui| {
            let ib = |ui: &mut egui::Ui, icon: &str, tip: &str, enabled: bool| ui.add_enabled(enabled, egui::Button::new(egui::RichText::new(icon).size(14.0)).min_size(vec2(26.0, 22.0))).on_hover_text(crate::i18n::t(tip)).clicked();
            if ib(ui, l::UNDO_2, "Annuler (Ctrl+Z)", !self.undo.is_empty()) {
                self.undo(false);
            }
            if ib(ui, l::REDO_2, "Rétablir (Ctrl+Maj+Z)", !self.redo.is_empty()) {
                self.undo(true);
            }
            ui.separator();
            if ib(ui, l::PLAY, "Présenter le prototype : les formes liées mènent à leur plan de travail (Échap pour sortir)", !self.doc.boards.is_empty()) {
                self.present = Some(self.sel_board.or_else(|| self.board_of_selection()).unwrap_or(0));
                self.present_tex = None;
            }
            if ib(ui, l::SMILE_PLUS, "Bibliothèque d'icônes Lucide (plus de 2 400)", true) {
                self.icon_pick = if self.icon_pick.is_some() { None } else { Some(String::new()) };
            }
            if ib(ui, l::FILE_UP, "Importer un SVG (aussi : coller un SVG avec Ctrl+V)", true) {
                if let Some(f) = rfd::FileDialog::new().add_filter("SVG", &["svg"]).pick_file() {
                    match std::fs::read_to_string(&f) {
                        Ok(svg) => self.import_svg(&svg),
                        Err(e) => self.message = Some(e.to_string()),
                    }
                }
            }
            ui.label(egui::RichText::new(format!("{} %", (self.zoom * 100.0).round())).size(11.5).color(p.muted));
            if ib(ui, l::SCAN, "Tout cadrer (Ctrl+0)", true) {
                self.fit = true;
            }
            if let Some(m) = &self.message {
                ui.label(egui::RichText::new(m).size(11.0).color(p.muted));
            }
        });
    }

    fn import_svg(&mut self, svg: &str) {
        self.checkpoint();
        let at = self.center;
        let before = self.doc.shapes.len();
        match crate::vector::import_svg(&mut self.doc, svg, [0.0, 0.0]) {
            Ok(n) => {
                // Les formes importées arrivent groupées, centrées dans la vue
                let ids: Vec<u64> = self.doc.shapes[before..].iter().map(|s| s.id).collect();
                if let Some(b) = self.doc.bounds_of(&ids) {
                    let (dx, dy) = (at[0] - (b[0] + b[2]) / 2.0, at[1] - (b[1] + b[3]) / 2.0);
                    for &i in &ids {
                        self.doc.translate(i, dx, dy);
                    }
                }
                let g = if ids.len() > 1 { self.doc.group(&ids, 0) } else { ids.first().copied() };
                self.sel = g.into_iter().collect();
                self.message = Some(format!("{n} {}", crate::i18n::t_mid("tracés importés")));
                self.changed();
            }
            Err(e) => self.message = Some(e),
        }
    }

    // ------------------------------------------------------------------ calques

    fn layers(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        ui.label(egui::RichText::new(crate::i18n::t("Plans de travail")).size(11.5).color(p.muted));
        let mut go_board = None;
        for (i, b) in self.doc.boards.iter().enumerate() {
            let on = self.sel_board == Some(i);
            if ui.add(egui::Button::new(egui::RichText::new(format!("{}  {}", l::FRAME, b.name)).size(12.0)).selected(on).frame(on).min_size(vec2(ui.available_width(), 20.0))).clicked() {
                go_board = Some(i);
            }
        }
        if let Some(i) = go_board {
            self.sel_board = Some(i);
            self.sel.clear();
            let b = &self.doc.boards[i];
            self.center = [b.x + b.w / 2.0, b.y + b.h / 2.0];
        }
        ui.separator();
        ui.label(egui::RichText::new(crate::i18n::t("Calques")).size(11.5).color(p.muted));
        let mut act: Option<(u64, u8)> = None;
        egui::ScrollArea::vertical().id_salt("vec-layers-list").show(ui, |ui| {
            // Du premier plan vers l'arrière, les groupes avec leurs formes en retrait
            let roots: Vec<u64> = self.doc.shapes.iter().rev().filter(|s| s.parent == 0).map(|s| s.id).collect();
            for id in roots {
                self.layer_row(ui, id, 0, &mut act);
            }
            if self.doc.shapes.is_empty() {
                ui.label(egui::RichText::new(crate::i18n::t("Dessine une forme avec les outils de gauche.")).size(11.0).color(p.muted));
            }
        });
        if let Some((id, what)) = act {
            match what {
                0 => {
                    if ui.input(|i| i.modifiers.shift || i.modifiers.command) {
                        if !self.sel.contains(&id) {
                            self.sel.push(id);
                        } else {
                            self.sel.retain(|s| *s != id);
                        }
                    } else {
                        self.sel = vec![id];
                    }
                    self.sel_board = None;
                }
                1 => {
                    self.checkpoint();
                    if let Some(s) = self.shape_mut(id) {
                        s.visible = !s.visible;
                    }
                    self.changed();
                }
                _ => {
                    if let Some(s) = self.shape_mut(id) {
                        s.locked = !s.locked;
                    }
                    self.changed();
                }
            }
        }
    }

    fn layer_row(&self, ui: &mut egui::Ui, id: u64, depth: usize, act: &mut Option<(u64, u8)>) {
        let p = crate::theme::pal();
        let Some(s) = self.doc.get(id) else { return };
        let on = self.sel.contains(&id);
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
        let painter = ui.painter_at(r);
        if on {
            painter.rect_filled(r, 2.0, p.accent.gamma_multiply(0.25));
        } else if resp.hovered() {
            painter.rect_filled(r, 2.0, p.raised);
        }
        let icon = match &s.kind {
            Kind::Rect { .. } => l::SQUARE,
            Kind::Ellipse { .. } => l::CIRCLE,
            Kind::Polygon { .. } => l::HEXAGON,
            Kind::Star { .. } => l::STAR,
            Kind::Line { .. } => l::MINUS,
            Kind::Path { .. } => l::PEN_TOOL,
            Kind::Text { .. } => l::TYPE,
            Kind::Image { .. } => l::IMAGE,
            Kind::Group { .. } if s.component => l::COMPONENT,
            Kind::Group { .. } if s.instance_of != 0 => l::DIAMOND,
            Kind::Group { op: 0, .. } => l::FOLDER,
            Kind::Group { .. } => l::SQUARE_DASHED,
        };
        let x = r.left() + 6.0 + depth as f32 * 14.0;
        painter.text(pos2(x, r.center().y), egui::Align2::LEFT_CENTER, icon, egui::FontId::proportional(12.0), p.muted);
        painter.text(pos2(x + 18.0, r.center().y), egui::Align2::LEFT_CENTER, &s.name, egui::FontId::proportional(12.0), if s.visible { p.text } else { p.muted });
        let eye = Rect::from_center_size(pos2(r.right() - 30.0, r.center().y), vec2(16.0, 16.0));
        let lock = Rect::from_center_size(pos2(r.right() - 10.0, r.center().y), vec2(16.0, 16.0));
        if resp.hovered() || !s.visible || s.locked {
            painter.text(eye.center(), egui::Align2::CENTER_CENTER, if s.visible { l::EYE } else { l::EYE_OFF }, egui::FontId::proportional(11.0), p.muted);
            painter.text(lock.center(), egui::Align2::CENTER_CENTER, if s.locked { l::LOCK } else { l::LOCK_OPEN }, egui::FontId::proportional(11.0), p.muted);
        }
        if resp.clicked() {
            let at = resp.interact_pointer_pos().unwrap_or(r.center());
            *act = Some((id, if eye.contains(at) { 1 } else if lock.contains(at) { 2 } else { 0 }));
        }
        if let Kind::Group { children, .. } = &s.kind {
            for c in children.iter().rev() {
                self.layer_row(ui, *c, depth + 1, act);
            }
        }
    }

    // ------------------------------------------------------------------ propriétés

    fn props(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let head = |ui: &mut egui::Ui, t: &str| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(crate::i18n::t(t)).size(11.5).strong().color(p.muted));
        };
        // Rien de choisi : le plan de travail, ou le document
        if self.sel.is_empty() {
            let bi = self.sel_board.or(if self.doc.boards.len() == 1 { Some(0) } else { None });
            if let Some(bi) = bi.filter(|&b| b < self.doc.boards.len()) {
                head(ui, "Plan de travail");
                let before = self.doc.boards[bi].clone();
                let b = &mut self.doc.boards[bi];
                ui.text_edit_singleline(&mut b.name);
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut b.x).prefix("x "));
                    ui.add(egui::DragValue::new(&mut b.y).prefix("y "));
                });
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut b.w).range(1.0..=20000.0).prefix("l "));
                    ui.add(egui::DragValue::new(&mut b.h).range(1.0..=20000.0).prefix("h "));
                });
                ui.horizontal(|ui| {
                    let mut has = b.bg.is_some();
                    if ui.checkbox(&mut has, crate::i18n::t("couleur de fond")).changed() {
                        b.bg = has.then_some([255, 255, 255, 255]);
                    }
                    if let Some(c) = &mut b.bg {
                        let mut col = c32(*c);
                        if ui.color_edit_button_srgba(&mut col).changed() {
                            *c = arr(col);
                        }
                    }
                });
                egui::ComboBox::from_id_salt("vec-board-preset").selected_text(crate::i18n::t("Format…")).show_ui(ui, |ui| {
                    for (name, w, h) in BOARD_PRESETS {
                        if ui.button(crate::i18n::t(name)).clicked() {
                            b.w = w;
                            b.h = h;
                        }
                    }
                });
                // Grille de mise en page
                ui.horizontal(|ui| {
                    let mut on = b.grid.is_some();
                    if ui.checkbox(&mut on, crate::i18n::t("grille de colonnes")).on_hover_text(crate::i18n::t("Des colonnes qui guident et aimantent la mise en page (jamais exportées)")).changed() {
                        b.grid = on.then(crate::vector::LayoutGrid::default);
                    }
                });
                if let Some(g) = &mut b.grid {
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut g.cols).range(1..=48).prefix(format!("{} ", l::COLUMNS_3))).on_hover_text(crate::i18n::t("colonnes"));
                        ui.add(egui::DragValue::new(&mut g.gutter).range(0.0..=400.0).prefix(format!("{} ", crate::i18n::t("gouttière")))).on_hover_text(crate::i18n::t("espace entre deux colonnes"));
                        ui.add(egui::DragValue::new(&mut g.margin).range(0.0..=2000.0).prefix(format!("{} ", crate::i18n::t("marge"))));
                    });
                }
                // Le plan change de place ou de taille : ses formes suivent leurs contraintes
                let after = self.doc.boards[bi].clone();
                if (after.x, after.y, after.w, after.h) != (before.x, before.y, before.w, before.h) {
                    self.apply_constraints(&before, &after);
                }
                if self.doc.boards[bi] != before {
                    self.undo.push({
                        let mut d = self.doc.clone();
                        d.boards[bi] = before;
                        d
                    });
                    self.changed();
                }
                self.export_ui(ui, bi);
                if self.doc.boards.len() > 1 && ui.button(format!("{}  {}", l::TRASH_2, crate::i18n::t("Supprimer le plan de travail"))).clicked() {
                    self.checkpoint();
                    self.doc.boards.remove(bi);
                    self.sel_board = None;
                    self.changed();
                }
            } else {
                ui.label(egui::RichText::new(crate::i18n::t("Choisis une forme ou un plan de travail.")).color(p.muted));
            }
            head(ui, "Nuances");
            // Nuances globales : changer une nuance la change partout où elle sert ; clic droit pour la retirer
            let mut edit: Option<(usize, [u8; 4], [u8; 4])> = None;
            let mut remove = None;
            ui.horizontal_wrapped(|ui| {
                for (k, c) in self.doc.swatches.clone().into_iter().enumerate() {
                    let mut col = c32(c);
                    let r = ui.color_edit_button_srgba(&mut col).on_hover_text(crate::i18n::t("Nuance globale : la changer change toutes les formes qui l'utilisent (clic droit : retirer)"));
                    if r.changed() {
                        edit = Some((k, c, arr(col)));
                    }
                    if r.secondary_clicked() {
                        remove = Some(k);
                    }
                }
                if ui.small_button(l::PLUS).on_hover_text(crate::i18n::t("Ajouter les couleurs du document comme nuances")).clicked() {
                    for c in self.doc.colors_of(&[]) {
                        if !self.doc.swatches.contains(&c) {
                            self.doc.swatches.push(c);
                        }
                    }
                    self.changed();
                }
            });
            if let Some((k, from, to)) = edit {
                self.checkpoint();
                self.doc.replace_color(&[], from, to);
                self.doc.swatches[k] = to;
                self.changed();
            }
            if let Some(k) = remove {
                self.checkpoint();
                self.doc.swatches.remove(k);
                self.changed();
            }
            return;
        }
        let snapshot = self.doc.clone();
        let mut ch = false;
        let mut outline_text: Option<u64> = None;
        let single = (self.sel.len() == 1).then(|| self.sel[0]);
        let mut path_act: Option<&'static str> = None;
        if let Some(id) = single {
            let root = self.root.clone();
            let _ = root;
            let s = self.doc.shapes.iter_mut().find(|s| s.id == id).unwrap();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(crate::i18n::t(s.kind_label())).size(11.0).color(p.muted));
                ch |= ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(f32::INFINITY)).changed();
            });
            head(ui, "Position");
            ui.horizontal(|ui| {
                ch |= ui.add(egui::DragValue::new(&mut s.x).speed(1.0).max_decimals(1).prefix("x ")).changed();
                ch |= ui.add(egui::DragValue::new(&mut s.y).speed(1.0).max_decimals(1).prefix("y ")).changed();
                ch |= ui.add(egui::DragValue::new(&mut s.rot).speed(0.5).max_decimals(1).suffix("°")).changed();
            });
            // Prototype : un clic sur la forme (en présentation) mène à un autre plan de travail
            if self.doc.boards.len() > 1 {
                let names: Vec<String> = self.doc.boards.iter().map(|b| b.name.clone()).collect();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(l::MOUSE_POINTER_CLICK).color(p.muted)).on_hover_text(crate::i18n::t("Prototype : au clic, aller vers ce plan de travail (bouton Présenter)"));
                    egui::ComboBox::from_id_salt("vec-proto").width(160.0).selected_text(if s.link.is_empty() { crate::i18n::t("aucun lien") } else { s.link.clone() }).show_ui(ui, |ui| {
                        if ui.selectable_label(s.link.is_empty(), crate::i18n::t("aucun lien")).clicked() {
                            s.link.clear();
                            ch = true;
                        }
                        for n in &names {
                            if ui.selectable_label(&s.link == n, n).clicked() {
                                s.link = n.clone();
                                ch = true;
                            }
                        }
                    });
                });
            }
            // Contraintes (comme Figma) : ce que fait la forme quand le plan de travail change de taille
            if s.parent == 0 {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(l::PIN).color(p.muted)).on_hover_text(crate::i18n::t("Contraintes : quand le plan de travail change de taille"));
                    let hl = ["à gauche", "à droite", "à gauche et à droite", "au centre", "à l'échelle"];
                    let vl = ["en haut", "en bas", "en haut et en bas", "au centre", "à l'échelle"];
                    for (axis, labels, salt) in [(0usize, hl, "vec-ch"), (1, vl, "vec-cv")] {
                        egui::ComboBox::from_id_salt(salt).width(96.0).selected_text(crate::i18n::t(labels[s.constraint[axis].min(4) as usize])).show_ui(ui, |ui| {
                            for (k, lab) in labels.iter().enumerate() {
                                if ui.selectable_label(s.constraint[axis] == k as u8, crate::i18n::t(lab)).clicked() {
                                    s.constraint[axis] = k as u8;
                                    ch = true;
                                }
                            }
                        });
                    }
                });
            }
            match &mut s.kind {
                Kind::Rect { w, h, r } => {
                    ui.horizontal(|ui| {
                        ch |= ui.add(egui::DragValue::new(w).range(1.0..=100000.0).prefix("l ")).changed();
                        ch |= ui.add(egui::DragValue::new(h).range(1.0..=100000.0).prefix("h ")).changed();
                    });
                    ui.horizontal(|ui| {
                        ui.label(l::SQUARE_ROUND_CORNER);
                        let mut all = r[0];
                        if ui.add(egui::DragValue::new(&mut all).range(0.0..=10000.0).prefix(crate::i18n::t("coins "))).on_hover_text(crate::i18n::t("Les quatre coins ; chaque coin se règle à droite")).changed() {
                            *r = [all; 4];
                            ch = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        for (k, tip) in ["haut gauche", "haut droit", "bas droit", "bas gauche"].iter().enumerate() {
                            ch |= ui.add(egui::DragValue::new(&mut r[k]).range(0.0..=10000.0).speed(0.5)).on_hover_text(crate::i18n::t(tip)).changed();
                        }
                    });
                }
                Kind::Ellipse { w, h } | Kind::Image { w, h, .. } => {
                    ui.horizontal(|ui| {
                        ch |= ui.add(egui::DragValue::new(w).range(1.0..=100000.0).prefix("l ")).changed();
                        ch |= ui.add(egui::DragValue::new(h).range(1.0..=100000.0).prefix("h ")).changed();
                    });
                }
                Kind::Polygon { sides, r } => {
                    ui.horizontal(|ui| {
                        ch |= ui.add(egui::DragValue::new(sides).range(3..=64).prefix(crate::i18n::t("côtés "))).changed();
                        ch |= ui.add(egui::DragValue::new(r).range(1.0..=100000.0).prefix(crate::i18n::t("rayon "))).changed();
                    });
                }
                Kind::Star { points, r1, r2 } => {
                    ui.horizontal(|ui| {
                        ch |= ui.add(egui::DragValue::new(points).range(2..=64).prefix(crate::i18n::t("branches "))).changed();
                    });
                    ui.horizontal(|ui| {
                        ch |= ui.add(egui::DragValue::new(r1).range(1.0..=100000.0).prefix(crate::i18n::t("ext. "))).changed();
                        ch |= ui.add(egui::DragValue::new(r2).range(1.0..=100000.0).prefix(crate::i18n::t("int. "))).changed();
                    });
                }
                Kind::Line { x2, y2 } => {
                    ui.horizontal(|ui| {
                        ch |= ui.add(egui::DragValue::new(x2).prefix("dx ")).changed();
                        ch |= ui.add(egui::DragValue::new(y2).prefix("dy ")).changed();
                    });
                }
                Kind::Path { nodes, closed } => {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("{} {}", nodes.len(), crate::i18n::t_mid("points"))).color(p.muted));
                        ch |= ui.checkbox(closed, crate::i18n::t("fermé")).changed();
                        if ui.button(l::SPLINE_POINTER).on_hover_text(crate::i18n::t("Simplifier : moins de points, même allure")).clicked() {
                            path_act = Some("simplify");
                        }
                        let one_node = self.nodes_sel.iter().filter(|(i, _)| *i == id).count() == 1;
                        if ui.add_enabled(one_node, egui::Button::new(l::SCISSORS)).on_hover_text(crate::i18n::t("Couper le tracé au point choisi (sélection directe)")).on_disabled_hover_text(crate::i18n::t("Choisis un point avec la sélection directe (A)")).clicked() {
                            path_act = Some("cut");
                        }
                    });
                }
                Kind::Text { text, size, font, align, width, spacing, leading } => {
                    head(ui, "Texte");
                    let te = ui.add(egui::TextEdit::multiline(text).desired_rows(2).desired_width(f32::INFINITY));
                    if self.focus_text {
                        te.request_focus();
                        self.focus_text = false;
                    }
                    ch |= te.changed();
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("vec-font").selected_text(crate::motion::FONTS.iter().find(|f| f.0 == font).map(|f| crate::i18n::t(f.1)).unwrap_or_default()).show_ui(ui, |ui| {
                            for (fid, name) in crate::motion::FONTS {
                                if ui.selectable_label(font == fid, crate::i18n::t(name)).clicked() {
                                    *font = fid.to_string();
                                    ch = true;
                                }
                            }
                        });
                        ch |= ui.add(egui::DragValue::new(size).range(1.0..=2000.0).suffix(" px")).changed();
                    });
                    ui.horizontal(|ui| {
                        for (k, icon) in [l::ALIGN_LEFT, l::ALIGN_CENTER, l::ALIGN_RIGHT].iter().enumerate() {
                            if ui.selectable_label(*align == k as u8, *icon).clicked() {
                                *align = k as u8;
                                ch = true;
                            }
                        }
                        ch |= ui.add(egui::DragValue::new(spacing).speed(0.1).range(-50.0..=200.0).prefix(crate::i18n::t("espace "))).changed();
                        ch |= ui.add(egui::DragValue::new(leading).speed(0.01).range(0.5..=4.0).prefix(crate::i18n::t("interligne "))).changed();
                    });
                    let mut area = width.is_some();
                    if ui.checkbox(&mut area, crate::i18n::t("zone de texte (retour à la ligne)")).changed() {
                        *width = area.then_some(400.0);
                        ch = true;
                    }
                    if let Some(w) = width {
                        ch |= ui.add(egui::DragValue::new(w).range(10.0..=20000.0).prefix(crate::i18n::t("largeur "))).changed();
                    }
                    if ui.button(format!("{}  {}", l::SPLINE, crate::i18n::t("Vectoriser le texte"))).on_hover_text(crate::i18n::t("Le texte devient des tracés modifiables point par point")).clicked() {
                        outline_text = Some(id);
                    }
                }
                Kind::Group { children, op } => {
                    ui.label(egui::RichText::new(format!("{} {}", children.len(), crate::i18n::t_mid("formes"))).color(p.muted));
                    ui.horizontal(|ui| {
                        for (k, (icon, tip)) in BOOLS.iter().enumerate() {
                            if ui.selectable_label(*op == k as u8, *icon).on_hover_text(crate::i18n::t(tip)).clicked() {
                                *op = k as u8;
                                ch = true;
                            }
                        }
                    });
                }
            }
            // Remplissages et contours
            let is_group = matches!(s.kind, Kind::Group { .. });
            if !is_group {
                head(ui, "Remplissage");
                let mut rm = None;
                for (k, f) in s.fills.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ch |= paint_ui(ui, f, k);
                        if ui.small_button(l::X).clicked() {
                            rm = Some(k);
                        }
                    });
                }
                if let Some(k) = rm {
                    s.fills.remove(k);
                    ch = true;
                }
                if ui.small_button(format!("{}  {}", l::PLUS, crate::i18n::t("remplissage"))).clicked() {
                    s.fills.push(Paint::solid([200, 200, 205, 255]));
                    ch = true;
                }
                head(ui, "Contour");
                let mut rm = None;
                for (k, st) in s.strokes.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ch |= paint_ui(ui, &mut st.paint, 100 + k);
                        ch |= ui.add(egui::DragValue::new(&mut st.width).range(0.0..=500.0).speed(0.2).suffix(" px")).changed();
                        if ui.small_button(l::X).clicked() {
                            rm = Some(k);
                        }
                    });
                    ui.horizontal(|ui| {
                        for (v, icon, tip) in [(0u8, "⊢", "extrémité plate"), (1, "◖", "extrémité ronde"), (2, "▭", "extrémité carrée")] {
                            if ui.selectable_label(st.cap == v, icon).on_hover_text(crate::i18n::t(tip)).clicked() {
                                st.cap = v;
                                ch = true;
                            }
                        }
                        ui.separator();
                        for (v, tip) in [(0u8, "angle pointu"), (1, "angle arrondi"), (2, "angle biseauté")] {
                            if ui.selectable_label(st.join == v, ["◤", "◜", "◣"][v as usize]).on_hover_text(crate::i18n::t(tip)).clicked() {
                                st.join = v;
                                ch = true;
                            }
                        }
                        let mut dashed = !st.dash.is_empty();
                        if ui.selectable_label(dashed, "┄").on_hover_text(crate::i18n::t("pointillés")).clicked() {
                            dashed = !dashed;
                            st.dash = if dashed { vec![st.width * 3.0, st.width * 2.0] } else { vec![] };
                            ch = true;
                        }
                        if ui.selectable_label(st.arrow, l::ARROW_RIGHT).on_hover_text(crate::i18n::t("flèche au bout (ligne ou tracé ouvert)")).clicked() {
                            st.arrow = !st.arrow;
                            ch = true;
                        }
                    });
                }
                if let Some(k) = rm {
                    s.strokes.remove(k);
                    ch = true;
                }
                if ui.small_button(format!("{}  {}", l::PLUS, crate::i18n::t("contour"))).clicked() {
                    s.strokes.push(VStroke { paint: Paint::solid([24, 24, 27, 255]), width: 2.0, cap: 1, join: 1, dash: vec![], arrow: false });
                    ch = true;
                }
            }
            head(ui, "Aspect");
            ui.horizontal(|ui| {
                let mut pc = s.opacity * 100.0;
                if ui.add(egui::DragValue::new(&mut pc).range(0.0..=100.0).suffix(" %").prefix(crate::i18n::t("opacité "))).changed() {
                    s.opacity = pc / 100.0;
                    ch = true;
                }
                egui::ComboBox::from_id_salt("vec-blend").width(90.0).selected_text(crate::i18n::t(BLENDS.iter().find(|b| b.0 == s.blend.as_str() || (s.blend.is_empty() && b.0 == "normal")).map(|b| b.1).unwrap_or("Normal"))).show_ui(ui, |ui| {
                    for (css, name) in BLENDS {
                        if ui.selectable_label(s.blend == css, crate::i18n::t(name)).clicked() {
                            s.blend = if css == "normal" { String::new() } else { css.to_string() };
                            ch = true;
                        }
                    }
                });
            });
            ui.horizontal(|ui| {
                let mut sh = s.shadow.is_some();
                if ui.checkbox(&mut sh, crate::i18n::t("ombre portée")).changed() {
                    s.shadow = sh.then_some(Shadow { dx: 0.0, dy: 8.0, blur: 16.0, c: [0, 0, 0, 90] });
                    ch = true;
                }
                ch |= ui.add(egui::DragValue::new(&mut s.blur).range(0.0..=200.0).prefix(crate::i18n::t("flou "))).changed();
            });
            if let Some(d) = &mut s.shadow {
                ui.horizontal(|ui| {
                    ch |= ui.add(egui::DragValue::new(&mut d.dx).prefix("x ")).changed();
                    ch |= ui.add(egui::DragValue::new(&mut d.dy).prefix("y ")).changed();
                    ch |= ui.add(egui::DragValue::new(&mut d.blur).range(0.0..=200.0).prefix(crate::i18n::t("flou "))).changed();
                    let mut c = c32(d.c);
                    if ui.color_edit_button_srgba(&mut c).changed() {
                        d.c = arr(c);
                        ch = true;
                    }
                });
            }
        } else {
            ui.label(egui::RichText::new(format!("{} {}", self.sel.len(), crate::i18n::t_mid("formes choisies"))).color(p.muted));
        }
        // Disposer : aligner, répartir, grouper, booléens
        head(ui, "Aligner");
        ui.horizontal(|ui| {
            for (k, icon, tip) in [(0u8, l::ALIGN_START_VERTICAL, "à gauche"), (1, l::ALIGN_CENTER_VERTICAL, "au centre"), (2, l::ALIGN_END_VERTICAL, "à droite"), (3, l::ALIGN_START_HORIZONTAL, "en haut"), (4, l::ALIGN_CENTER_HORIZONTAL, "au milieu"), (5, l::ALIGN_END_HORIZONTAL, "en bas")] {
                if ui.button(icon).on_hover_text(format!("{} ({})", crate::i18n::t(tip), crate::i18n::t(if self.sel.len() > 1 { "la sélection" } else { "le plan de travail" }))).clicked() {
                    self.doc = snapshot.clone();
                    self.align(k);
                    ch = true;
                }
            }
        });
        if self.sel.len() > 2 {
            ui.horizontal(|ui| {
                if ui.button(format!("{}  {}", l::COLUMNS_3, crate::i18n::t("répartir en largeur"))).clicked() {
                    self.doc = snapshot.clone();
                    self.distribute(true);
                    ch = true;
                }
                if ui.button(format!("{}  {}", l::ROWS_3, crate::i18n::t("en hauteur"))).clicked() {
                    self.doc = snapshot.clone();
                    self.distribute(false);
                    ch = true;
                }
            });
        }
        // Vectoriser une image (croquis, logo scanné) en tracés
        let image_sel = single.and_then(|i| self.doc.get(i)).filter(|s| matches!(s.kind, Kind::Image { .. })).map(|s| s.id);
        if let Some(id) = image_sel {
            head(ui, "Vectoriser");
            ui.add(egui::Slider::new(&mut self.trace_thr, 0.05..=0.95).text(crate::i18n::t("seuil"))).on_hover_text(crate::i18n::t("Plus haut : les gris clairs deviennent aussi des formes"));
            ui.add(egui::Slider::new(&mut self.trace_detail, 0.3..=8.0).logarithmic(true).text(crate::i18n::t("simplifier"))).on_hover_text(crate::i18n::t("Plus haut : moins de points, des courbes plus lisses"));
            if ui.button(format!("{}  {}", l::PEN_TOOL, crate::i18n::t("vectoriser l'image"))).on_hover_text(crate::i18n::t("Les zones sombres de l'image deviennent des tracés, posés par-dessus")).clicked() {
                self.doc = snapshot.clone();
                match self.trace_image(id) {
                    Ok(n) => {
                        self.message = Some(format!("{n} {}", crate::i18n::t_mid("tracés")));
                        ch = true;
                    }
                    Err(e) => self.message = Some(e),
                }
            }
        }
        // Composants (comme Figma) : un groupe maître et ses instances
        if let Some(gid) = single.filter(|i| self.doc.get(*i).is_some_and(|s| matches!(s.kind, Kind::Group { .. }))) {
            let s = self.doc.get(gid).cloned().unwrap();
            head(ui, "Composant");
            ui.horizontal(|ui| {
                if s.instance_of != 0 {
                    let mname = self.doc.get(s.instance_of).map(|m| m.name.clone()).unwrap_or_default();
                    // Variantes : les composants qui partagent le nom avant « / » (Bouton / Normal, Bouton / Survol…)
                    let base = mname.split(" / ").next().unwrap_or(&mname).to_string();
                    let vars: Vec<(u64, String)> = self.doc.shapes.iter().filter(|m| m.component && m.name.split(" / ").next() == Some(base.as_str())).map(|m| (m.id, m.name.clone())).collect();
                    if vars.len() > 1 {
                        let mut pick = None;
                        egui::ComboBox::from_id_salt("vec-variant").width(130.0).selected_text(mname.rsplit(" / ").next().unwrap_or(&mname).to_string()).show_ui(ui, |ui| {
                            for (vid, vn) in &vars {
                                if ui.selectable_label(*vid == s.instance_of, vn.rsplit(" / ").next().unwrap_or(vn)).clicked() {
                                    pick = Some(*vid);
                                }
                            }
                        });
                        if let Some(v) = pick {
                            self.doc = snapshot.clone();
                            if let Some(x) = self.doc.shapes.iter_mut().find(|x| x.id == gid) {
                                x.instance_of = v;
                            }
                            ch = true;
                        }
                    }
                    ui.label(egui::RichText::new(format!("{} {mname}", l::DIAMOND)).weak());
                    if ui.button(l::CROSSHAIR).on_hover_text(crate::i18n::t("aller au composant")).clicked() {
                        self.sel = vec![s.instance_of];
                    }
                    if ui.button(l::UNLINK).on_hover_text(crate::i18n::t("détacher l'instance (redevient un groupe)")).clicked() {
                        self.doc = snapshot.clone();
                        if let Some(x) = self.doc.shapes.iter_mut().find(|x| x.id == gid) {
                            x.instance_of = 0;
                        }
                        ch = true;
                    }
                } else if s.component {
                    ui.label(egui::RichText::new(format!("{} {}", l::COMPONENT, crate::i18n::t("maître"))).weak());
                    if ui.button(l::COPY_PLUS).on_hover_text(crate::i18n::t("nouvelle instance (Alt+glisser marche aussi)")).clicked() {
                        self.doc = snapshot.clone();
                        if let Some(i) = self.doc.instantiate(gid) {
                            self.sel = vec![i];
                        }
                        ch = true;
                    }
                    if ui.button(l::LAYERS_2).on_hover_text(crate::i18n::t("Nouvelle variante (un composant frère : « Nom / Variante 2 »)")).clicked() {
                        self.doc = snapshot.clone();
                        let base = s.name.split(" / ").next().unwrap_or(&s.name).to_string();
                        if !s.name.contains(" / ") {
                            if let Some(x) = self.doc.shapes.iter_mut().find(|x| x.id == gid) {
                                x.name = format!("{base} / {}", crate::i18n::t("Défaut"));
                            }
                        }
                        let n = self.doc.shapes.iter().filter(|m| m.component && m.name.split(" / ").next() == Some(base.as_str())).count() + 1;
                        if let Some(v) = self.clone_tree(gid, 0.0) {
                            let b = self.doc.get(gid).map(|g| self.doc.bounds(g)).unwrap_or([0.0; 4]);
                            self.doc.translate(v, b[2] - b[0] + 40.0, 0.0);
                            if let Some(x) = self.doc.shapes.iter_mut().find(|x| x.id == v) {
                                // Une copie de composant devient une instance : on la remet en composant
                                x.instance_of = 0;
                                x.component = true;
                                x.name = format!("{base} / {} {n}", crate::i18n::t("Variante"));
                            }
                            self.sel = vec![v];
                        }
                        ch = true;
                    }
                    if ui.button(l::X).on_hover_text(crate::i18n::t("n'est plus un composant")).clicked() {
                        self.doc = snapshot.clone();
                        if let Some(x) = self.doc.shapes.iter_mut().find(|x| x.id == gid) {
                            x.component = false;
                        }
                        ch = true;
                    }
                } else if ui.button(format!("{} {}", l::COMPONENT, crate::i18n::t("créer un composant"))).on_hover_text("Ctrl+Alt+K").clicked() {
                    self.doc = snapshot.clone();
                    if let Some(x) = self.doc.shapes.iter_mut().find(|x| x.id == gid) {
                        x.component = true;
                    }
                    ch = true;
                }
            });
        }
        // Mise en page automatique d'un groupe (comme Figma)
        if let Some(gid) = single.filter(|i| self.doc.get(*i).is_some_and(|s| matches!(s.kind, Kind::Group { op: 0, .. }))) {
            head(ui, "Mise en page auto");
            let mut lay = self.doc.get(gid).and_then(|s| s.layout.clone());
            let mut on = lay.is_some();
            let mut lch = false;
            if ui.checkbox(&mut on, crate::i18n::t("les formes se suivent (Maj+A)")).changed() {
                lay = on.then(crate::vector::AutoLayout::default);
                lch = true;
            }
            if let Some(l) = &mut lay {
                ui.horizontal(|ui| {
                    for (d, icon, tip) in [(0u8, l::ARROW_RIGHT, "en ligne"), (1, l::ARROW_DOWN, "en colonne")] {
                        if ui.selectable_label(l.dir == d, icon).on_hover_text(crate::i18n::t(tip)).clicked() {
                            l.dir = d;
                            lch = true;
                        }
                    }
                    ui.separator();
                    for (a, icon, tip) in [(0u8, l::ALIGN_START_VERTICAL, "aligner au début"), (1, l::ALIGN_CENTER_VERTICAL, "centrer"), (2, l::ALIGN_END_VERTICAL, "aligner à la fin")] {
                        if ui.selectable_label(l.align == a, icon).on_hover_text(crate::i18n::t(tip)).clicked() {
                            l.align = a;
                            lch = true;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    lch |= ui.add(egui::DragValue::new(&mut l.gap).speed(0.5).range(-500.0..=2000.0).prefix(format!("{} ", crate::i18n::t("écart")))).changed();
                    lch |= ui.add(egui::DragValue::new(&mut l.pad[0]).speed(0.5).range(0.0..=2000.0).prefix("↔ ")).on_hover_text(crate::i18n::t("marge horizontale")).changed();
                    lch |= ui.add(egui::DragValue::new(&mut l.pad[1]).speed(0.5).range(0.0..=2000.0).prefix("↕ ")).on_hover_text(crate::i18n::t("marge verticale")).changed();
                });
                ui.horizontal(|ui| {
                    let mut has_bg = l.bg.is_some();
                    if ui.checkbox(&mut has_bg, crate::i18n::t("couleur de fond")).changed() {
                        l.bg = has_bg.then_some([232, 236, 242, 255]);
                        lch = true;
                    }
                    if let Some(c) = &mut l.bg {
                        let mut col = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
                        if ui.color_edit_button_srgba(&mut col).changed() {
                            *c = col.to_srgba_unmultiplied();
                            lch = true;
                        }
                        lch |= ui.add(egui::DragValue::new(&mut l.radius).speed(0.5).range(0.0..=1000.0).prefix(format!("{} ", crate::i18n::t("coins")))).changed();
                    }
                });
            }
            if lch {
                self.doc = snapshot.clone();
                if let Some(s) = self.doc.shapes.iter_mut().find(|s| s.id == gid) {
                    s.layout = lay;
                }
                self.doc.apply_layouts();
                ch = true;
            }
        }
        // Styles partagés (comme Figma) : poser, créer, mettre à jour pour toutes les formes, détacher
        if let Some(id) = single.filter(|i| self.doc.get(*i).is_some_and(|s| !matches!(s.kind, Kind::Group { .. }))) {
            head(ui, "Style");
            let cur = self.doc.get(id).map(|s| s.style.clone()).unwrap_or_default();
            let mut act: Option<(u8, String)> = None;
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("vec-style").width(130.0).selected_text(if cur.is_empty() { crate::i18n::t("aucun") } else { cur.clone() }).show_ui(ui, |ui| {
                    for st in &self.doc.styles {
                        if ui.selectable_label(st.name == cur, &st.name).clicked() {
                            act = Some((0, st.name.clone()));
                        }
                    }
                });
                if ui.button(l::PLUS).on_hover_text(crate::i18n::t("Nouveau style depuis cette forme (couleurs, contour, effets, police)")).clicked() {
                    let mut n = self.doc.styles.len() + 1;
                    while self.doc.styles.iter().any(|s| s.name == format!("Style {n}")) {
                        n += 1;
                    }
                    act = Some((1, format!("Style {n}")));
                }
                if !cur.is_empty() {
                    if ui.button(l::REFRESH_CW).on_hover_text(crate::i18n::t("Mettre à jour le style avec cette forme : toutes ses formes changent")).clicked() {
                        act = Some((1, cur.clone()));
                    }
                    if ui.button(l::UNLINK).on_hover_text(crate::i18n::t("Détacher du style")).clicked() {
                        act = Some((2, String::new()));
                    }
                }
            });
            if !cur.is_empty() {
                // Renommer le style
                let mut name = cur.clone();
                if ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY)).lost_focus() && name.trim() != cur && !name.trim().is_empty() {
                    act = Some((3, name.trim().to_string()));
                }
            }
            if let Some((what, name)) = act {
                self.doc = snapshot.clone();
                match what {
                    0 => {
                        if let Some(st) = self.doc.styles.iter().find(|s| s.name == name).cloned() {
                            if let Some(s) = self.shape_mut(id) {
                                st.apply(s);
                            }
                        }
                    }
                    1 => {
                        if let Some(sh) = self.doc.get(id).cloned() {
                            let st = crate::vector::NamedStyle::of(&name, &sh);
                            self.doc.set_style(st);
                            if let Some(s) = self.shape_mut(id) {
                                s.style = name;
                            }
                        }
                    }
                    2 => {
                        if let Some(s) = self.shape_mut(id) {
                            s.style.clear();
                        }
                    }
                    _ => {
                        for st in self.doc.styles.iter_mut().filter(|s| s.name == cur) {
                            st.name = name.clone();
                        }
                        for s in self.doc.shapes.iter_mut().filter(|s| s.style == cur) {
                            s.style = name.clone();
                        }
                    }
                }
                ch = true;
            }
        }
        // Recolorer (comme Illustrator) : chaque couleur de la sélection se change d'un coup
        let cols = self.doc.colors_of(&self.sel);
        if cols.len() > 1 || (cols.len() == 1 && self.sel.len() > 1) {
            head(ui, "Recolorer");
            ui.horizontal_wrapped(|ui| {
                for c in cols.iter().take(24) {
                    let mut col = c32(*c);
                    if ui.color_edit_button_srgba(&mut col).on_hover_text(crate::i18n::t("Change cette couleur dans toute la sélection")).changed() {
                        self.doc = snapshot.clone();
                        let sel = self.sel.clone();
                        self.doc.replace_color(&sel, *c, arr(col));
                        ch = true;
                    }
                }
            });
        }
        head(ui, "Combiner");
        ui.horizontal(|ui| {
            for (k, (icon, tip)) in BOOLS.iter().enumerate() {
                if ui.add_enabled(self.sel.len() > 1 || k == 0, egui::Button::new(*icon)).on_hover_text(crate::i18n::t(tip)).clicked() {
                    self.doc = snapshot.clone();
                    self.combine(k as u8);
                    ch = true;
                }
            }
            let is_group = single.and_then(|i| self.doc.get(i)).is_some_and(|s| matches!(s.kind, Kind::Group { .. }));
            if ui.add_enabled(is_group, egui::Button::new(l::UNGROUP)).on_hover_text(crate::i18n::t("Dissocier (Ctrl+Maj+G)")).clicked() {
                self.doc = snapshot.clone();
                if let Some(id) = single {
                    self.doc.ungroup(id);
                    self.sel.clear();
                }
                ch = true;
            }
        });
        // Décaler le tracé (comme Illustrator) : une nouvelle forme, plus grande ou plus petite, sous la sélection
        if !self.sel.is_empty() {
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.offset_by).speed(0.5).range(-500.0..=500.0)).on_hover_text(crate::i18n::t("distance du décalage (négatif : vers l'intérieur)"));
                if ui.button(format!("{} {}", l::SQUARE_SQUARE, crate::i18n::t("décaler le tracé"))).on_hover_text(crate::i18n::t("une copie agrandie ou rétrécie de la sélection, en tracé")).clicked() {
                    self.doc = snapshot.clone();
                    if let Some(id) = self.offset_selection(self.offset_by) {
                        self.sel = vec![id];
                    }
                    ch = true;
                }
                let strokes = self.sel.iter().any(|i| self.doc.get(*i).is_some_and(|s| !s.strokes.is_empty()));
                if ui.add_enabled(strokes, egui::Button::new(l::PEN_LINE)).on_hover_text(crate::i18n::t("vectoriser le contour : il devient une forme pleine")).clicked() {
                    self.doc = snapshot.clone();
                    self.outline_strokes();
                    ch = true;
                }
            });
        }
        if let Some(id) = outline_text {
            self.outline_text(id);
            ch = true;
        }
        // Outils de tracé : simplifier, couper au point choisi
        if let (Some(act), Some(id)) = (path_act, single) {
            self.doc = snapshot.clone();
            match act {
                "simplify" => {
                    let tol = 1.5 / self.zoom.max(0.01);
                    if let Some(n) = self.doc.simplify_path(id, tol) {
                        self.message = Some(format!("{n} {}", crate::i18n::t_mid("points")));
                    }
                }
                _ => {
                    if let Some(k) = self.nodes_sel.iter().find(|(i, _)| *i == id).map(|x| x.1) {
                        if let Some(n) = self.doc.cut_path(id, k) {
                            self.sel = if n == id { vec![id] } else { vec![id, n] };
                        }
                        self.nodes_sel.clear();
                    }
                }
            }
            ch = true;
        }
        if ch {
            self.undo.push(snapshot);
            self.redo.clear();
            self.changed();
        }
        if let Some(bi) = self.board_of_selection() {
            self.export_ui(ui, bi);
        }
    }

    /// Le plan de travail `bi` en composition Motion (à côté du dessin) : rectangles, ellipses, textes,
    /// tracés et images deviennent des calques du même genre, à la même place
    fn board_to_motion(&mut self, bi: usize) -> Result<PathBuf, String> {
        use crate::motion::{Compo, Kind as MK, Layer as ML};
        let b = self.doc.boards.get(bi).cloned().ok_or("plan introuvable")?;
        self.doc.sync_instances();
        self.doc.apply_layouts();
        let bg = b.bg.map(|c| [c[0], c[1], c[2]]).unwrap_or([255, 255, 255]);
        let mut c = Compo { w: b.w.round().max(2.0) as u32, h: b.h.round().max(2.0) as u32, bg, layers: vec![], ..Compo::default() };
        c.layers.clear();
        let rgb = |s: &Shape| -> ([u8; 3], f32) {
            match s.fills.first().or(s.strokes.first().map(|k| &k.paint)) {
                Some(crate::vector::Paint::Solid { c }) => ([c[0], c[1], c[2]], c[3] as f32 / 255.0),
                Some(p) => {
                    let c = p.first();
                    ([c[0], c[1], c[2]], c[3] as f32 / 255.0)
                }
                None => ([0, 0, 0], 1.0),
            }
        };
        let inside = |bb: [f32; 4]| bb[2] > b.x && bb[0] < b.x + b.w && bb[3] > b.y && bb[1] < b.y + b.h;
        let mut out: Vec<ML> = vec![];
        for s in self.doc.shapes.iter().filter(|s| s.visible && !matches!(s.kind, Kind::Group { .. })) {
            // Visible seulement si ses groupes le sont
            let mut up = s.parent;
            let mut hidden = false;
            while up != 0 {
                match self.doc.get(up) {
                    Some(g) => {
                        hidden |= !g.visible;
                        up = g.parent;
                    }
                    None => break,
                }
            }
            let bb = self.doc.bounds(s);
            if hidden || !inside(bb) {
                continue;
            }
            let (col, alpha) = rgb(s);
            let (sn, cs) = s.rot.to_radians().sin_cos();
            let world = |p: [f32; 2]| [s.x + p[0] * cs - p[1] * sn, s.y + p[0] * sn + p[1] * cs];
            let kind_center = |w: f32, h: f32| world([w / 2.0, h / 2.0]);
            let (kind, center, rot) = match &s.kind {
                Kind::Rect { w, h, r } => (MK::Rect { w: *w, h: *h, radius: r[0], color: col }, kind_center(*w, *h), s.rot),
                Kind::Ellipse { w, h } => (MK::Ellipse { w: *w, h: *h, color: col }, kind_center(*w, *h), s.rot),
                Kind::Text { text, size, font, align, spacing, .. } => {
                    (MK::Text { text: text.clone(), size: *size, color: col, font: font.clone(), tracking: *spacing, align: *align }, [(bb[0] + bb[2]) / 2.0, (bb[1] + bb[3]) / 2.0], 0.0)
                }
                Kind::Image { path, w, h } => (MK::Image { path: path.clone() }, kind_center(*w, *h), s.rot),
                _ => {
                    // Tracés, lignes, polygones, étoiles : un tracé Motion qui passe par des points du contour
                    let nodes = match &s.kind {
                        Kind::Path { nodes, .. } => nodes.clone(),
                        Kind::Line { x2, y2 } => vec![Node::corner([0.0, 0.0]), Node::corner([*x2, *y2])],
                        Kind::Polygon { sides, r } => (0..(*sides).max(3)).map(|k| { let a = -std::f32::consts::FRAC_PI_2 + k as f32 / (*sides).max(3) as f32 * std::f32::consts::TAU; Node::corner([r * a.cos(), r * a.sin()]) }).collect(),
                        Kind::Star { points, r1, r2 } => (0..(*points).max(2) * 2).map(|k| { let a = -std::f32::consts::FRAC_PI_2 + k as f32 / ((*points).max(2) * 2) as f32 * std::f32::consts::TAU; let r = if k % 2 == 0 { r1 } else { r2 }; Node::corner([r * a.cos(), r * a.sin()]) }).collect(),
                        _ => continue,
                    };
                    let closed = !matches!(s.kind, Kind::Line { .. } | Kind::Path { closed: false, .. });
                    let curved = nodes.iter().any(|n| !n.is_corner());
                    let pts: Vec<[f32; 2]> = if curved {
                        let flat = crate::vgeom::flatten(&nodes, closed);
                        let step = (flat.last().map(|x| x.1).unwrap_or(0.0) / 48.0).max(4.0);
                        let mut v = vec![];
                        let mut next = 0.0;
                        for (p, d) in flat {
                            if d >= next {
                                v.push(world(p));
                                next = d + step;
                            }
                        }
                        v
                    } else {
                        nodes.iter().map(|n| world(n.p)).collect()
                    };
                    let cen = [(bb[0] + bb[2]) / 2.0, (bb[1] + bb[3]) / 2.0];
                    let width = s.strokes.first().map(|k| k.width).unwrap_or(0.0);
                    let stroke_col = s.strokes.first().map(|k| k.paint.first()).map(|c| [c[0], c[1], c[2]]).unwrap_or(col);
                    let fill = (closed && !s.fills.is_empty()).then_some(col);
                    (MK::Path { points: pts.iter().map(|p| [p[0] - cen[0], p[1] - cen[1]]).collect(), closed, width, color: stroke_col, arrow: false, fill, keys: vec![] }, cen, 0.0)
                }
            };
            let mut l = ML::new(&s.name, kind, &c);
            l.pos.v = [center[0] - b.x, center[1] - b.y];
            l.rot.v = [rot, 0.0];
            l.opacity.v = [(s.opacity * alpha * 100.0).clamp(0.0, 100.0), 0.0];
            // Une image garde sa taille dans le dessin
            if let Kind::Image { path, w, .. } = &s.kind {
                if let Ok((iw, _)) = image::image_dimensions(std::path::Path::new(&self.root).join(path)) {
                    let k = w / iw.max(1) as f32 * 100.0;
                    l.scale.v = [k, k];
                }
            }
            out.push(l);
        }
        if out.is_empty() {
            return Err(crate::i18n::t("rien à animer sur ce plan de travail"));
        }
        // Motion : le premier calque passe devant
        out.reverse();
        c.layers = out;
        let dir = self.path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let dir = if dir.to_string_lossy().contains(crate::untitled::DIR) { std::path::Path::new(&self.root).join("motion") } else { dir };
        let _ = std::fs::create_dir_all(&dir);
        let stem: String = b.name.chars().map(|ch| if "/\\:*?\"<>|".contains(ch) { '-' } else { ch }).collect();
        let mut path = dir.join(format!("{stem}.{}", crate::formats::ext("motion")));
        let mut n = 2;
        while path.exists() {
            path = dir.join(format!("{stem} {n}.{}", crate::formats::ext("motion")));
            n += 1;
        }
        c.write(&path)?;
        Ok(path)
    }

    /// Mode présentation : le plan de travail en grand ; un clic sur une forme liée change de plan ;
    /// flèches : plan précédent ou suivant ; Échap : retour à l'édition
    fn present_ui(&mut self, ui: &mut egui::Ui) {
        let Some(bi) = self.present.filter(|b| *b < self.doc.boards.len()) else {
            self.present = None;
            return;
        };
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click());
        ui.painter().rect_filled(rect, 0.0, Color32::from_gray(18));
        let b = self.doc.boards[bi].clone();
        let k = ((rect.width() - 40.0) / b.w).min((rect.height() - 60.0) / b.h).max(0.01);
        let r = Rect::from_center_size(rect.center(), vec2(b.w * k, b.h * k));
        // L'image du plan (refaite si le document ou la taille changent)
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_string(&self.doc).unwrap_or_default().hash(&mut h);
            bi.hash(&mut h);
            (k * 100.0).round().to_bits().hash(&mut h);
            h.finish()
        };
        if self.present_tex.as_ref().is_none_or(|t| t.0 != key) {
            let scale = (k * ui.ctx().pixels_per_point()).clamp(0.05, 4.0);
            if let Some(img) = crate::vector::export_png(&self.doc, bi, &self.root, scale) {
                let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                self.present_tex = Some((key, ui.ctx().load_texture("vec-present", ci, egui::TextureOptions::LINEAR)));
            }
        }
        if let Some((_, t)) = &self.present_tex {
            ui.painter().image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        // Zones cliquables : les formes liées posées sur ce plan
        let to_screen = |w: [f32; 2]| pos2(r.left() + (w[0] - b.x) * k, r.top() + (w[1] - b.y) * k);
        let mut go = None;
        for s in self.doc.shapes.iter().filter(|s| !s.link.is_empty() && s.visible) {
            let bb = self.doc.bounds(s);
            let hr = Rect::from_two_pos(to_screen([bb[0], bb[1]]), to_screen([bb[2], bb[3]]));
            if !hr.intersects(r) {
                continue;
            }
            if resp.hover_pos().is_some_and(|h| hr.contains(h)) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                ui.painter().rect_stroke(hr, 2.0, Stroke::new(1.5, Color32::from_rgb(56, 132, 255)), egui::StrokeKind::Outside);
                if resp.clicked() {
                    go = self.doc.boards.iter().position(|x| x.name == s.link);
                }
            }
        }
        ui.painter().text(rect.center_bottom() - vec2(0.0, 16.0), egui::Align2::CENTER_CENTER, format!("{}  ·  {} / {}  ·  {}", b.name, bi + 1, self.doc.boards.len(), crate::i18n::t("Échap : quitter, flèches : plans")), egui::FontId::proportional(12.0), Color32::from_gray(150));
        let (left, right, esc) = ui.input(|i| (i.key_pressed(egui::Key::ArrowLeft), i.key_pressed(egui::Key::ArrowRight), i.key_pressed(egui::Key::Escape)));
        if esc {
            self.present = None;
        } else if let Some(g) = go {
            self.present = Some(g);
        } else if right {
            self.present = Some((bi + 1) % self.doc.boards.len());
        } else if left {
            self.present = Some((bi + self.doc.boards.len() - 1) % self.doc.boards.len());
        }
    }

    /// La bibliothèque d'icônes : recherche par nom (en anglais), un clic pose l'icône au centre de la vue
    fn icon_window(&mut self, ctx: &egui::Context) {
        let Some(q) = self.icon_pick.as_mut() else { return };
        let mut open = true;
        let mut pick = None;
        egui::Window::new(crate::i18n::t("icônes Lucide")).open(&mut open).default_size([360.0, 420.0]).show(ctx, |ui| {
            ui.add(egui::TextEdit::singleline(q).hint_text(crate::i18n::t("chercher (en anglais : heart, arrow, user…)")).desired_width(f32::INFINITY)).request_focus();
            let ql = q.trim().to_lowercase();
            let hits: Vec<&(&str, char)> = crate::lucide_all::ALL.iter().filter(|x| ql.is_empty() || x.0.contains(&ql)).take(240).collect();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (name, ch) in hits {
                        if ui.add(egui::Button::new(egui::RichText::new(ch.to_string()).size(22.0)).min_size(vec2(36.0, 36.0))).on_hover_text(*name).clicked() {
                            pick = Some((name.to_string(), *ch));
                        }
                    }
                });
            });
        });
        if !open {
            self.icon_pick = None;
        }
        if let Some((name, ch)) = pick {
            self.insert_icon(&name, ch);
        }
    }

    fn insert_icon(&mut self, name: &str, ch: char) {
        let size = 96.0;
        let contours = crate::vector::icon_contours(ch, size);
        if contours.is_empty() {
            return;
        }
        self.checkpoint();
        let (x, y) = (self.center[0] - size / 2.0, self.center[1] - size / 2.0);
        let mut ids = vec![];
        for nodes in contours {
            let id = self.doc.new_id();
            self.doc.shapes.push(Shape { id, name: name.to_string(), kind: Kind::Path { nodes, closed: true }, x, y, fills: vec![crate::vector::Paint::solid([24, 24, 27, 255])], ..Shape::base() });
            ids.push(id);
        }
        // Les trous (l'intérieur d'un cercle au trait) restent vides : groupe pair-impair
        let g = if ids.len() > 1 { self.doc.group(&ids, 5) } else { ids.first().copied() };
        if let Some(g) = g {
            if let Some(s) = self.shape_mut(g) {
                s.name = name.to_string();
            }
            self.sel = vec![g];
        }
        self.changed();
    }

    /// Une forme décalée de `d` autour de la sélection, posée juste dessous ; rend son identifiant
    fn offset_selection(&mut self, d: f32) -> Option<u64> {
        let ids: Vec<u64> = self.sel.iter().map(|i| self.top_of(*i)).collect();
        let g = crate::vgeom::Grid::around(&self.doc, &ids, d.max(0.0) + 4.0, 1200)?;
        let mut cells = vec![false; g.w * g.h];
        for id in &ids {
            for (c, m) in cells.iter_mut().zip(g.mask(&self.doc, *id)) {
                *c |= m;
            }
        }
        let cells = crate::vgeom::offset(&cells, &g, d);
        let like = self.doc.get(ids[0])?.clone();
        let mut like = like;
        if like.fills.is_empty() {
            like.fills = crate::vgeom::stroke_as_fill(&like);
            like.strokes.clear();
        }
        let id = crate::vgeom::add_traced(&mut self.doc, &g, g.trace(&cells), &like, &crate::i18n::t("Décalage"))?;
        // Sous la sélection, comme Illustrator
        let at = ids.iter().filter_map(|i| self.doc.index(*i)).min().unwrap_or(0);
        if let Some(i) = self.doc.index(id) {
            let s = self.doc.shapes.remove(i);
            self.doc.shapes.insert(at, s);
        }
        Some(id)
    }

    /// Les contours choisis deviennent des formes pleines (largeur variable comprise)
    fn outline_strokes(&mut self) {
        let mut out = vec![];
        for id in self.sel.clone() {
            let Some(s) = self.doc.get(id).cloned() else { continue };
            if s.strokes.is_empty() || matches!(s.kind, Kind::Group { .. }) {
                out.push(id);
                continue;
            }
            let mut only = s.clone();
            only.fills.clear();
            let tmp = self.doc.new_id();
            only.id = tmp;
            self.doc.shapes.push(only);
            let Some(g) = crate::vgeom::Grid::around(&self.doc, &[tmp], s.strokes[0].width + 4.0, 1200) else { continue };
            let cells = g.mask(&self.doc, tmp);
            self.doc.remove(&[tmp]);
            let mut like = s.clone();
            like.fills = crate::vgeom::stroke_as_fill(&s);
            like.strokes.clear();
            if let Some(n) = crate::vgeom::add_traced(&mut self.doc, &g, g.trace(&cells), &like, &s.name) {
                // Le fond d'origine reste dessous, sans contour
                if let Some(x) = self.shape_mut(id) {
                    x.strokes.clear();
                    x.widths.clear();
                }
                if s.fills.is_empty() || matches!(s.kind, Kind::Line { .. } | Kind::Path { closed: false, .. }) {
                    self.doc.remove(&[id]);
                }
                out.push(n);
            }
        }
        self.sel = out;
    }

    /// Met la sélection dans un groupe en mise en page automatique, rangé dans l'ordre de lecture.
    fn auto_layout_selection(&mut self) {
        let mut ids: Vec<u64> = self.sel.iter().map(|i| self.top_of(*i)).collect();
        ids.sort_by_key(|i| self.doc.index(*i));
        ids.dedup();
        let b = self.doc.bounds_of(&ids).unwrap_or([0.0; 4]);
        // En ligne si la sélection est plus large que haute
        let dir = if b[2] - b[0] >= b[3] - b[1] { 0 } else { 1 };
        // L'ordre de lecture : de gauche à droite, ou de haut en bas
        ids.sort_by(|a, c| {
            let (ba, bc) = (self.doc.get(*a).map(|s| self.doc.bounds(s)).unwrap_or([0.0; 4]), self.doc.get(*c).map(|s| self.doc.bounds(s)).unwrap_or([0.0; 4]));
            ba[dir as usize].total_cmp(&bc[dir as usize])
        });
        if let Some(g) = self.doc.group(&ids, 0) {
            if let Some(s) = self.doc.shapes.iter_mut().find(|s| s.id == g) {
                s.name = crate::i18n::t("Cadre");
                s.layout = Some(crate::vector::AutoLayout { dir, ..Default::default() });
                s.x -= 24.0;
                s.y -= 16.0;
            }
            if let Some(Shape { kind: Kind::Group { children, .. }, .. }) = self.doc.shapes.iter_mut().find(|s| s.id == g) {
                *children = ids.clone();
            }
            self.doc.apply_layouts();
            self.sel = vec![g];
        }
    }

    /// Vectorise l'image `id` : ses contours sombres deviennent des tracés (un groupe pair-impair pour les trous).
    fn trace_image(&mut self, id: u64) -> Result<usize, String> {
        let s = self.doc.get(id).cloned().ok_or("image introuvable")?;
        let Kind::Image { path, w, h } = &s.kind else { return Err("pas une image".into()) };
        let full = std::path::Path::new(&self.root).join(path);
        let img = image::open(&full).map_err(|e| e.to_string())?;
        // Au plus 1200 px : la vectorisation reste rapide, le détail suffit
        let img = if img.width().max(img.height()) > 1200 { img.thumbnail(1200, 1200) } else { img };
        let img = img.to_rgba8();
        let (kx, ky) = (w / img.width().max(1) as f32, h / img.height().max(1) as f32);
        let (loops, col) = crate::vector::trace(&img, self.trace_thr, self.trace_detail);
        if loops.is_empty() {
            return Err(crate::i18n::t("rien à vectoriser : baisse ou monte le seuil"));
        }
        let map = |p: [f32; 2]| [p[0] * kx, p[1] * ky];
        let mut ids = vec![];
        for lp in &loops {
            let nodes: Vec<Node> = lp.iter().map(|n| Node { p: map(n.p), i: map(n.i), o: map(n.o) }).collect();
            let nid = self.doc.new_id();
            self.doc.shapes.push(Shape { id: nid, name: format!("{} {nid}", crate::i18n::t("Tracé")), kind: Kind::Path { nodes, closed: true }, x: s.x, y: s.y, rot: s.rot, fills: vec![crate::vector::Paint::solid([col[0], col[1], col[2], 255])], parent: s.parent, ..Shape::base() });
            ids.push(nid);
        }
        let n = ids.len();
        // Plusieurs contours : un groupe pair-impair, les trous restent vides
        self.sel = match if n > 1 { self.doc.group(&ids, 5) } else { None } {
            Some(g) => {
                if let Some(gs) = self.doc.shapes.iter_mut().find(|x| x.id == g) {
                    gs.name = crate::i18n::t("Vectorisation");
                }
                vec![g]
            }
            None => ids,
        };
        Ok(n)
    }

    /// Un texte devient un groupe de tracés (un par contour de lettre), à la même place
    fn outline_text(&mut self, id: u64) {
        let Some(t) = self.doc.get(id).cloned() else { return };
        let d = crate::vector::text_outline(&t).0;
        let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><path d="{d}" fill="black"/></svg>"#);
        let before = self.doc.shapes.len();
        if crate::vector::import_svg(&mut self.doc, &svg, [0.0, 0.0]).is_err() {
            return;
        }
        let ids: Vec<u64> = self.doc.shapes[before..].iter().map(|s| s.id).collect();
        for s in self.doc.shapes.iter_mut().filter(|s| ids.contains(&s.id)) {
            (s.x, s.y, s.rot) = (t.x, t.y, t.rot);
            s.fills = t.fills.clone();
            s.strokes = t.strokes.clone();
        }
        // Les contours prennent la place du texte dans la pile, puis un groupe « union » les réunit
        let at = self.doc.index(id).unwrap_or(0);
        let moved: Vec<Shape> = self.doc.shapes.drain(before..).collect();
        for (k, s) in moved.into_iter().enumerate() {
            self.doc.shapes.insert(at + 1 + k, s);
        }
        self.doc.remove(&[id]);
        if let Some(g) = self.doc.group(&ids, 0) {
            if let Some(gs) = self.shape_mut(g) {
                gs.name = t.name.clone();
            }
            self.sel = vec![g];
        }
    }

    fn board_of_selection(&self) -> Option<usize> {
        let b = self.selection_bounds()?;
        let c = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        self.doc.boards.iter().position(|bd| c[0] >= bd.x && c[0] <= bd.x + bd.w && c[1] >= bd.y && c[1] <= bd.y + bd.h)
    }

    fn export_ui(&mut self, ui: &mut egui::Ui, bi: usize) {
        let p = crate::theme::pal();
        ui.add_space(6.0);
        ui.label(egui::RichText::new(format!("{} · {}", crate::i18n::t("Exporter"), self.doc.boards[bi].name)).size(11.5).strong().color(p.muted));
        ui.horizontal(|ui| {
            for k in [1.0f32, 2.0, 3.0] {
                if ui.selectable_label(self.export_scale == k, format!("{k}×")).clicked() {
                    self.export_scale = k;
                }
            }
            if ui.button(format!("{}  PNG", l::DOWNLOAD)).clicked() {
                self.export(bi, "png");
            }
            if ui.button(format!("{}  SVG", l::DOWNLOAD)).clicked() {
                self.export(bi, "svg");
            }
            // Le plan de travail devient une composition Motion, chaque forme un calque qu'on anime
            if ui.button(l::FILM).on_hover_text(crate::i18n::t("Animer dans Motion : chaque forme du plan devient un calque modifiable")).clicked() {
                match self.board_to_motion(bi) {
                    Ok(p) => self.open_request = Some(p),
                    Err(e) => self.message = Some(e),
                }
            }
            // Le plan de travail part vers un autre outil
            if let Some(t) = crate::sendto::button(ui, true, "vector") {
                let name = self.doc.boards[bi].name.clone();
                self.message = Some(match crate::vector::export_png(&self.doc, bi, &self.root, self.export_scale).ok_or_else(|| crate::i18n::t("export impossible")).and_then(|img| crate::sendto::stage(&self.root, &name, &img)) {
                    Ok(rel) => {
                        crate::sendto::push(t, rel);
                        crate::i18n::t("Envoyé.")
                    }
                    Err(e) => e,
                });
            }
        });
    }

    /// Reçoit une image du coffre (envoyée d'un autre outil) : posée au milieu du premier plan de travail
    pub fn receive(&mut self, rel: &str) {
        let full = std::path::Path::new(&self.root).join(rel);
        let (w, h) = image::image_dimensions(&full).map(|(w, h)| (w as f32, h as f32)).unwrap_or((512.0, 512.0));
        let b = self.doc.boards.first().map(|b| [b.x, b.y, b.w, b.h]).unwrap_or([0.0, 0.0, 1080.0, 1080.0]);
        // À l'échelle du plan s'il est plus petit que l'image
        let k = (b[2] / w).min(b[3] / h).min(1.0);
        let (w, h) = (w * k, h * k);
        self.checkpoint();
        let id = self.doc.new_id();
        let name = std::path::Path::new(rel).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        self.doc.shapes.push(Shape { id, name, kind: Kind::Image { path: rel.to_string(), w, h }, x: b[0] + (b[2] - w) / 2.0, y: b[1] + (b[3] - h) / 2.0, ..Shape::base() });
        self.sel = vec![id];
        self.changed();
    }

    fn export(&mut self, bi: usize, ext: &str) {
        let name = format!("{}.{ext}", self.doc.boards[bi].name);
        let mut d = rfd::FileDialog::new().add_filter(ext.to_uppercase(), &[ext]).set_file_name(&name);
        if let Some(dir) = self.path.parent() {
            d = d.set_directory(dir);
        }
        let Some(out) = d.save_file() else { return };
        let r = match ext {
            "svg" => crate::vector::export_svg(&self.doc, bi, &self.root).ok_or("export impossible".to_string()).and_then(|s| std::fs::write(&out, s).map_err(|e| e.to_string())),
            _ => crate::vector::export_png(&self.doc, bi, &self.root, self.export_scale).ok_or("export impossible".to_string()).and_then(|img| img.save(&out).map_err(|e| e.to_string())),
        };
        self.message = Some(match r {
            Ok(()) => format!("{} : {}", crate::i18n::t("exporté"), out.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
            Err(e) => e,
        });
    }

    fn align(&mut self, how: u8) {
        let target = if self.sel.len() > 1 {
            self.selection_bounds()
        } else {
            self.board_of_selection().map(|i| {
                let b = &self.doc.boards[i];
                [b.x, b.y, b.x + b.w, b.y + b.h]
            })
        };
        let Some(t) = target else { return };
        for id in self.sel.clone() {
            let Some(s) = self.doc.get(id) else { continue };
            let b = self.doc.bounds(s);
            let (dx, dy) = match how {
                0 => (t[0] - b[0], 0.0),
                1 => ((t[0] + t[2]) / 2.0 - (b[0] + b[2]) / 2.0, 0.0),
                2 => (t[2] - b[2], 0.0),
                3 => (0.0, t[1] - b[1]),
                4 => (0.0, (t[1] + t[3]) / 2.0 - (b[1] + b[3]) / 2.0),
                _ => (0.0, t[3] - b[3]),
            };
            self.doc.translate(id, dx, dy);
        }
    }

    fn distribute(&mut self, horizontal: bool) {
        let mut items: Vec<(u64, [f32; 4])> = self.sel.iter().filter_map(|i| self.doc.get(*i).map(|s| (*i, self.doc.bounds(s)))).collect();
        let k = if horizontal { 0 } else { 1 };
        items.sort_by(|a, b| a.1[k].total_cmp(&b.1[k]));
        let (first, last) = (items[0].1, items[items.len() - 1].1);
        let span = last[k + 2] - first[k];
        let sizes: f32 = items.iter().map(|(_, b)| b[k + 2] - b[k]).sum();
        let gap = (span - sizes) / (items.len() - 1) as f32;
        let mut pos = first[k];
        for (id, b) in items {
            let d = pos - b[k];
            if horizontal {
                self.doc.translate(id, d, 0.0);
            } else {
                self.doc.translate(id, 0.0, d);
            }
            pos += b[k + 2] - b[k] + gap;
        }
    }

    fn combine(&mut self, op: u8) {
        // Les formes dans l'ordre de la pile (de l'arrière vers l'avant)
        let mut ids: Vec<u64> = self.sel.iter().map(|i| self.top_of(*i)).collect();
        ids.sort_by_key(|i| self.doc.index(*i));
        ids.dedup();
        if let Some(g) = self.doc.group(&ids, op) {
            self.sel = vec![g];
        }
    }

    // ------------------------------------------------------------------ toile

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let p = crate::theme::pal();
        let ctx = ui.ctx().clone();
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        // Image du coffre glissée sur la toile : une forme image
        if let Some(rel) = crate::drop_rel(&resp, crate::images::has_preview) {
            self.checkpoint();
            let [w, h] = image::image_dimensions(std::path::Path::new(&self.root).join(&rel)).map(|(w, h)| [w as f32, h as f32]).unwrap_or([400.0, 300.0]);
            let k = (600.0 / w.max(h)).min(1.0);
            let id = self.new_shape(Kind::Image { path: rel, w: w * k, h: h * k }, self.center[0] - w * k / 2.0, self.center[1] - h * k / 2.0);
            if let Some(s) = self.shape_mut(id) {
                s.fills.clear();
            }
            self.sel = vec![id];
            self.changed();
        }
        if self.fit && rect.width() > 10.0 {
            let ids: Vec<u64> = self.doc.shapes.iter().map(|s| s.id).collect();
            let mut b = self.doc.bounds_of(&ids).unwrap_or([f32::MAX, f32::MAX, f32::MIN, f32::MIN]);
            for bd in &self.doc.boards {
                b = [b[0].min(bd.x), b[1].min(bd.y), b[2].max(bd.x + bd.w), b[3].max(bd.y + bd.h)];
            }
            if b[0] < b[2] {
                self.center = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
                self.zoom = ((rect.width() - 80.0) / (b[2] - b[0])).min((rect.height() - 80.0) / (b[3] - b[1])).clamp(0.02, 16.0);
            }
            self.fit = false;
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, p.bg);
        let (z, c) = (self.zoom, self.center);
        let mid = rect.center();
        let to_screen = move |w: [f32; 2]| pos2(mid.x + (w[0] - c[0]) * z, mid.y + (w[1] - c[1]) * z);
        let to_world = move |q: Pos2| [c[0] + (q.x - mid.x) / z, c[1] + (q.y - mid.y) / z];
        // Ombres des plans de travail, sous le rendu
        for b in &self.doc.boards {
            let r = Rect::from_two_pos(to_screen([b.x, b.y]), to_screen([b.x + b.w, b.y + b.h]));
            painter.rect_filled(r.translate(vec2(0.0, 2.0)), 0.0, Color32::from_black_alpha(60));
            if b.bg.is_none() {
                painter.rect_filled(r, 0.0, Color32::from_gray(235));
            }
        }
        // Rendu SVG de la zone visible, refait quand le document ou la vue change
        let ppp = ctx.pixels_per_point();
        let (pw, ph) = ((rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32);
        let area = [c[0] - rect.width() / 2.0 / z, c[1] - rect.height() / 2.0 / z, rect.width() / z, rect.height() / z];
        // Les instances suivent leur composant, puis les cadres en mise en page automatique replacent leurs formes
        self.doc.sync_instances();
        self.doc.apply_layouts();
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_string(&self.doc).unwrap_or_default().hash(&mut h);
            for v in area {
                v.to_bits().hash(&mut h);
            }
            (pw, ph).hash(&mut h);
            h.finish()
        };
        if key != self.tex_key || self.tex.is_none() {
            let svg = crate::vector::to_svg(&self.doc, area, [pw, ph], &self.root, true);
            if let Some(px) = crate::vector::rasterize(&svg, pw, ph) {
                let img = egui::ColorImage::from_rgba_unmultiplied([pw as usize, ph as usize], &px);
                match &mut self.tex {
                    Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                    None => self.tex = Some(ctx.load_texture(format!("vec-{}", self.path.display()), img, egui::TextureOptions::LINEAR)),
                }
            }
            self.tex_key = key;
        }
        if let Some(t) = &self.tex {
            painter.image(t.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        // Noms des plans de travail (on les attrape pour déplacer le plan)
        let mut label_hit = None;
        for (i, b) in self.doc.boards.iter().enumerate() {
            let at = to_screen([b.x, b.y]) - vec2(0.0, 4.0);
            let r = painter.text(at, egui::Align2::LEFT_BOTTOM, &b.name, egui::FontId::proportional(11.0), if self.sel_board == Some(i) { p.accent } else { p.muted });
            if resp.hover_pos().is_some_and(|h| r.expand(3.0).contains(h)) {
                label_hit = Some(i);
            }
            if self.sel_board == Some(i) {
                painter.rect_stroke(Rect::from_two_pos(to_screen([b.x, b.y]), to_screen([b.x + b.w, b.y + b.h])), 0.0, Stroke::new(1.5, p.accent), egui::StrokeKind::Outside);
            }
            // Grille de colonnes (comme Figma), en rouge translucide
            for (a, c) in b.columns() {
                painter.rect_filled(Rect::from_two_pos(to_screen([a, b.y]), to_screen([c, b.y + b.h])), 0.0, Color32::from_rgba_unmultiplied(255, 70, 70, 26));
            }
        }
        self.mouse(ui, &resp, rect, &to_world, &to_screen, label_hit);
        // Sélection : cadre, poignées, rotation ; points en sélection directe
        let sel_col = Color32::from_rgb(40, 140, 255);
        if let Some(b) = self.selection_bounds() {
            let r = Rect::from_two_pos(to_screen([b[0], b[1]]), to_screen([b[2], b[3]]));
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, sel_col), egui::StrokeKind::Middle);
            if self.tool == Tool::Select {
                for h in handles(r) {
                    let hr = Rect::from_center_size(h, vec2(8.0, 8.0));
                    painter.rect_filled(hr, 0.0, Color32::WHITE);
                    painter.rect_stroke(hr, 0.0, Stroke::new(1.0, sel_col), egui::StrokeKind::Middle);
                }
                let rot = r.center_top() - vec2(0.0, 22.0);
                painter.line_segment([r.center_top(), rot], Stroke::new(1.0, sel_col));
                painter.circle(rot, 4.5, Color32::WHITE, Stroke::new(1.0, sel_col));
                // Taille sous le cadre
                painter.text(r.center_bottom() + vec2(0.0, 6.0), egui::Align2::CENTER_TOP, format!("{} × {}", (b[2] - b[0]).round(), (b[3] - b[1]).round()), egui::FontId::proportional(10.5), sel_col);
            }
        }
        if matches!(self.tool, Tool::Direct | Tool::Pen) {
            for id in self.sel.iter().copied().chain(self.pen) {
                let Some(s) = self.doc.get(id) else { continue };
                let Kind::Path { nodes, .. } = &s.kind else { continue };
                let (cs, sn) = (s.rot.to_radians().cos(), s.rot.to_radians().sin());
                let w = |q: [f32; 2]| to_screen([s.x + q[0] * cs - q[1] * sn, s.y + q[0] * sn + q[1] * cs]);
                for (k, n) in nodes.iter().enumerate() {
                    let on = self.nodes_sel.contains(&(id, k));
                    if !n.is_corner() || on {
                        for h in [n.i, n.o] {
                            if h != n.p {
                                painter.line_segment([w(n.p), w(h)], Stroke::new(1.0, sel_col));
                                painter.circle(w(h), 3.0, Color32::WHITE, Stroke::new(1.0, sel_col));
                            }
                        }
                    }
                    let r = Rect::from_center_size(w(n.p), vec2(7.0, 7.0));
                    painter.rect_filled(r, 0.0, if on { sel_col } else { Color32::WHITE });
                    painter.rect_stroke(r, 0.0, Stroke::new(1.0, sel_col), egui::StrokeKind::Middle);
                }
            }
        }
        // Lignes d'aimantation
        for (vertical, at) in &self.snaps {
            let col = Color32::from_rgb(255, 60, 140);
            if *vertical {
                let x = to_screen([*at, 0.0]).x;
                painter.vline(x, rect.y_range(), Stroke::new(1.0, col));
            } else {
                let y = to_screen([0.0, *at]).y;
                painter.hline(rect.x_range(), y, Stroke::new(1.0, col));
            }
        }
        if let Some(Drag::Marquee(a)) = &self.drag {
            if let Some(b) = resp.interact_pointer_pos() {
                let r = Rect::from_two_pos(*a, b);
                painter.rect_filled(r, 0.0, sel_col.gamma_multiply(0.08));
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, sel_col), egui::StrokeKind::Middle);
            }
        }
        self.tool_overlay(&painter, &to_screen, resp.hover_pos().map(&to_world));
        self.keys(ui, &resp);
    }

    /// Le contour (tracé ou ligne) le plus proche du point, et le point le plus proche dessus
    fn width_target(&self, w: [f32; 2], tol: f32) -> Option<(u64, f32, [f32; 2], [f32; 2], f32)> {
        let mut best: Option<(u64, f32, [f32; 2], [f32; 2], f32)> = None;
        for s in self.doc.shapes.iter().filter(|s| s.visible && !s.locked && !s.strokes.is_empty()) {
            let (sn, cs) = (-s.rot.to_radians()).sin_cos();
            let q = [w[0] - s.x, w[1] - s.y];
            let local = [q[0] * cs - q[1] * sn, q[0] * sn + q[1] * cs];
            let Some((t, p, n, d)) = crate::vgeom::nearest(s, local) else { continue };
            let half = crate::vgeom::width_at(&s.widths, s.strokes[0].width, t) / 2.0;
            if d <= half + tol * 3.0 && best.as_ref().is_none_or(|b| d < b.4) {
                best = Some((s.id, t, p, n, d));
            }
        }
        best
    }

    fn width_mouse(&mut self, ui: &egui::Ui, resp: &egui::Response, to_world: &dyn Fn(Pos2) -> [f32; 2], tol: f32, alt: bool) {
        if resp.drag_started_by(egui::PointerButton::Primary) || (alt && resp.clicked()) {
            let o = ui.input(|i| i.pointer.press_origin()).unwrap_or_default();
            if let Some((id, t, ..)) = self.width_target(to_world(o), tol) {
                self.checkpoint();
                let Some(s) = self.shape_mut(id) else { return };
                let near = s.widths.iter().position(|p| (p[0] - t).abs() < 0.04);
                if alt {
                    if let Some(k) = near {
                        s.widths.remove(k);
                    }
                    self.changed();
                    return;
                }
                let k = match near {
                    Some(k) => k,
                    None => {
                        let base = s.strokes[0].width;
                        let cur = crate::vgeom::width_at(&s.widths, base, t);
                        s.widths.push([t, cur]);
                        s.widths.len() - 1
                    }
                };
                self.width_edit = Some((id, k));
                self.sel = vec![self.top_of(id)];
            }
        }
        if let (Some((id, k)), Some(h)) = (self.width_edit, resp.interact_pointer_pos()) {
            if resp.dragged() {
                let w = to_world(h);
                if let Some(s) = self.shape_mut(id) {
                    let t = s.widths.get(k).map(|p| p[0]).unwrap_or(0.0);
                    let (sn, cs) = (-s.rot.to_radians()).sin_cos();
                    let q = [w[0] - s.x, w[1] - s.y];
                    let local = [q[0] * cs - q[1] * sn, q[0] * sn + q[1] * cs];
                    // La distance au tracé, à l'endroit du point de largeur
                    if let Some((_, p, ..)) = crate::vgeom::spine(s).and_then(|(pts, _)| {
                        let total = pts.last()?.1.max(1e-6);
                        pts.iter().min_by(|a, b| (a.1 / total - t).abs().total_cmp(&(b.1 / total - t).abs())).map(|x| (0.0, x.0))
                    }) {
                        let d = ((local[0] - p[0]).powi(2) + (local[1] - p[1]).powi(2)).sqrt();
                        if let Some(wp) = s.widths.get_mut(k) {
                            wp[1] = (d * 2.0).max(0.2);
                        }
                    }
                }
                ui.ctx().request_repaint();
            }
        }
        if resp.drag_stopped() && self.width_edit.take().is_some() {
            self.changed();
        }
    }

    fn builder_mouse(&mut self, ui: &egui::Ui, resp: &egui::Response, to_world: &dyn Fn(Pos2) -> [f32; 2], alt: bool) {
        let ids: Vec<u64> = self.sel.clone();
        if ids.is_empty() {
            if resp.clicked() || resp.drag_started() {
                self.message = Some(crate::i18n::t("choisis d'abord les formes à combiner (outil sélection), puis glisse sur leurs zones"));
            }
            return;
        }
        // Les zones se recalculent quand la sélection ou les formes changent
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            for id in &ids {
                for d in std::iter::once(*id).chain(self.doc.descendants(*id)) {
                    serde_json::to_string(&self.doc.get(d)).unwrap_or_default().hash(&mut h);
                }
            }
            h.finish()
        };
        if self.builder.as_ref().is_none_or(|b| b.0 != key) {
            self.builder = crate::vgeom::Regions::build(&self.doc, &ids).map(|r| (key, r));
            self.picked.clear();
        }
        let Some((_, reg)) = &self.builder else { return };
        if let Some(h) = resp.interact_pointer_pos() {
            if resp.dragged() || resp.drag_started() || resp.clicked() {
                let z = reg.at(to_world(h));
                if z != 0 && !self.picked.contains(&z) {
                    self.picked.push(z);
                }
                ui.ctx().request_repaint();
            }
        }
        if (resp.drag_stopped() || resp.clicked()) && !self.picked.is_empty() {
            self.checkpoint();
            let picked = std::mem::take(&mut self.picked);
            let (made, gone) = self.builder.take().map(|(_, r)| r.apply(&mut self.doc, &picked, alt)).unwrap_or_default();
            // Les morceaux restent choisis : on peut continuer à les combiner
            let mut sel: Vec<u64> = ids.into_iter().filter(|i| !gone.contains(i) && self.doc.get(*i).is_some()).collect();
            sel.extend(made);
            self.sel = sel;
            self.changed();
        }
    }

    /// Surbrillances des outils Largeur et Concepteur de formes
    fn tool_overlay(&mut self, painter: &egui::Painter, to_screen: &dyn Fn([f32; 2]) -> Pos2, hover: Option<[f32; 2]>) {
        let p = crate::theme::pal();
        match self.tool {
            Tool::Width => {
                let target = self.width_edit.map(|(id, _)| id).or_else(|| hover.and_then(|w| self.width_target(w, 4.0 / self.zoom)).map(|t| t.0));
                let Some(s) = target.and_then(|id| self.doc.get(id)) else { return };
                let (sn, cs) = s.rot.to_radians().sin_cos();
                let tw = |q: [f32; 2]| to_screen([s.x + q[0] * cs - q[1] * sn, s.y + q[0] * sn + q[1] * cs]);
                let Some((pts, _)) = crate::vgeom::spine(s) else { return };
                let line: Vec<Pos2> = pts.iter().map(|(q, _)| tw(*q)).collect();
                painter.add(egui::Shape::line(line, Stroke::new(1.0, p.accent)));
                let total = pts.last().map(|x| x.1).unwrap_or(1.0).max(1e-6);
                let base = s.strokes.first().map(|k| k.width).unwrap_or(2.0);
                let mut marks: Vec<f32> = s.widths.iter().map(|w| w[0]).collect();
                if self.width_edit.is_none() {
                    if let Some((_, t, ..)) = hover.and_then(|w| self.width_target(w, 4.0 / self.zoom)) {
                        marks.push(t);
                    }
                }
                for t in marks {
                    let Some(i) = pts.iter().position(|x| x.1 / total >= t) else { continue };
                    let (a, b) = (pts[i.saturating_sub(1)].0, pts[(i + 1).min(pts.len() - 1)].0);
                    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                    let l = (dx * dx + dy * dy).sqrt().max(1e-6);
                    let n = [-dy / l, dx / l];
                    let half = crate::vgeom::width_at(&s.widths, base, t) / 2.0;
                    let c = pts[i].0;
                    let (e1, e2) = (tw([c[0] + n[0] * half, c[1] + n[1] * half]), tw([c[0] - n[0] * half, c[1] - n[1] * half]));
                    painter.line_segment([e1, e2], Stroke::new(1.0, p.accent));
                    for e in [e1, e2] {
                        painter.circle(e, 3.5, Color32::WHITE, Stroke::new(1.0, p.accent));
                    }
                    painter.circle_filled(tw(c), 3.0, p.accent);
                }
            }
            Tool::Builder => {
                let Some((_, reg)) = &self.builder else { return };
                let g = reg.grid.clone();
                let blue = Color32::from_rgb(56, 132, 255);
                // Le contour de chaque zone, fin
                for lp in reg.outlines.iter().flatten() {
                    let pts: Vec<Pos2> = lp.iter().map(|n| to_screen([g.org[0] + n.p[0], g.org[1] + n.p[1]])).collect();
                    if pts.len() > 2 {
                        painter.add(egui::Shape::closed_line(pts, Stroke::new(1.0, blue.gamma_multiply(0.7))));
                    }
                }
                // Les zones balayées et celle sous le pointeur, teintées (une texture de la grille)
                let hov = hover.map(|w| reg.at(w)).unwrap_or(0);
                let mut shown: Vec<u32> = self.picked.clone();
                if hov != 0 && !shown.contains(&hov) {
                    shown.push(hov);
                }
                if shown.is_empty() {
                    return;
                }
                if self.builder_tex.as_ref().is_none_or(|t| t.0 != shown) {
                    let (fill, erase) = (Color32::from_rgba_unmultiplied(56, 132, 255, 90), Color32::from_rgba_unmultiplied(220, 60, 60, 110));
                    let alt = painter.ctx().input(|i| i.modifiers.alt);
                    let px: Vec<Color32> = reg.label.iter().map(|l| if *l != 0 && shown.contains(l) { if alt { erase } else { fill } } else { Color32::TRANSPARENT }).collect();
                    let img = egui::ColorImage { size: [g.w, g.h], pixels: px, source_size: egui::vec2(g.w as f32, g.h as f32) };
                    let tex = painter.ctx().load_texture("concepteur", img, egui::TextureOptions::NEAREST);
                    self.builder_tex = Some((shown.clone(), tex));
                }
                if let Some((_, t)) = &self.builder_tex {
                    let r = Rect::from_two_pos(to_screen(g.org), to_screen([g.org[0] + g.w as f32 / g.k, g.org[1] + g.h as f32 / g.k]));
                    painter.image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                }
                let _ = p;
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn mouse(&mut self, ui: &egui::Ui, resp: &egui::Response, rect: Rect, to_world: &dyn Fn(Pos2) -> [f32; 2], to_screen: &dyn Fn([f32; 2]) -> Pos2, label_hit: Option<usize>) {
        let (shift, alt, cmd) = ui.input(|i| (i.modifiers.shift, i.modifiers.alt, i.modifiers.command));
        let space = ui.input(|i| i.key_down(egui::Key::Space));
        // Molette : zoom autour du pointeur
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta);
            if let Some(h) = resp.hover_pos() {
                if scroll.y != 0.0 && !shift {
                    let before = to_world(h);
                    self.zoom = (self.zoom * (scroll.y * 0.0025).exp()).clamp(0.02, 64.0);
                    self.center = [before[0] - (h.x - rect.center().x) / self.zoom, before[1] - (h.y - rect.center().y) / self.zoom];
                } else if scroll.x != 0.0 || (shift && scroll.y != 0.0) {
                    self.center[0] -= (scroll.x + if shift { scroll.y } else { 0.0 }) / self.zoom;
                }
            }
        }
        let tol = 4.0 / self.zoom;
        if !space && matches!(self.tool, Tool::Width | Tool::Builder) && !resp.drag_started_by(egui::PointerButton::Middle) && !matches!(self.drag, Some(Drag::Pan)) {
            if self.tool == Tool::Width {
                self.width_mouse(ui, resp, to_world, tol, alt);
            } else {
                self.builder_mouse(ui, resp, to_world, alt);
            }
            return;
        }
        if resp.drag_started_by(egui::PointerButton::Middle) || (space && resp.drag_started_by(egui::PointerButton::Primary)) {
            self.drag = Some(Drag::Pan);
        } else if resp.drag_started_by(egui::PointerButton::Primary) {
            let o = ui.input(|i| i.pointer.press_origin()).unwrap_or(rect.center());
            let w = to_world(o);
            self.drag = self.begin(o, w, shift, alt, cmd, tol, to_screen, label_hit);
        }
        if let Some(h) = resp.interact_pointer_pos() {
            if resp.dragged() {
                let w = to_world(h);
                let d = resp.drag_delta();
                self.update(w, d, shift, alt);
            }
        }
        if resp.drag_stopped() {
            self.end(resp, to_world, shift);
        }
        // Clics simples
        if resp.clicked() {
            if let Some(h) = resp.interact_pointer_pos() {
                let w = to_world(h);
                self.click(w, shift, cmd, tol, resp.double_clicked(), label_hit);
            }
        }
        if resp.double_clicked() && self.tool == Tool::Select {
            if let Some(h) = resp.interact_pointer_pos() {
                // Double-clic : un texte s'édite, un tracé passe en sélection directe, un groupe s'ouvre
                if let Some(id) = self.hit(to_world(h), true, tol) {
                    match self.doc.get(id).map(|s| &s.kind) {
                        Some(Kind::Text { .. }) => {
                            self.sel = vec![id];
                            self.focus_text = true;
                        }
                        Some(Kind::Path { .. }) => {
                            self.sel = vec![id];
                            self.tool = Tool::Direct;
                        }
                        _ => self.sel = vec![id],
                    }
                }
            }
        }
        if self.drag.is_some() {
            ui.ctx().request_repaint();
        }
        let _ = rect;
    }

    #[allow(clippy::too_many_arguments)]
    fn begin(&mut self, o: Pos2, w: [f32; 2], shift: bool, alt: bool, cmd: bool, tol: f32, to_screen: &dyn Fn([f32; 2]) -> Pos2, label_hit: Option<usize>) -> Option<Drag> {
        let _ = (shift, alt);
        if let Some(i) = label_hit {
            self.checkpoint();
            self.sel_board = Some(i);
            self.sel.clear();
            let b = &self.doc.boards[i];
            let inside: Vec<(u64, f32, f32)> = self.doc.shapes.iter().filter(|s| s.parent == 0).filter(|s| {
                let bb = self.doc.bounds(s);
                let c = [(bb[0] + bb[2]) / 2.0, (bb[1] + bb[3]) / 2.0];
                c[0] >= b.x && c[0] <= b.x + b.w && c[1] >= b.y && c[1] <= b.y + b.h
            }).map(|s| (s.id, s.x, s.y)).collect();
            return Some(Drag::BoardMove { i, from: w, orig: b.clone(), shapes: inside });
        }
        match self.tool {
            Tool::Select => {
                // Poignées du cadre de sélection
                if let Some(b) = self.selection_bounds() {
                    let r = Rect::from_two_pos(to_screen([b[0], b[1]]), to_screen([b[2], b[3]]));
                    let rot = r.center_top() - vec2(0.0, 22.0);
                    if rot.distance(o) < 8.0 {
                        self.checkpoint();
                        let c = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
                        let orig = self.selected_shapes_deep();
                        return Some(Drag::Rotate { center: c, a0: (w[1] - c[1]).atan2(w[0] - c[0]), orig });
                    }
                    if let Some(k) = handles(r).iter().position(|h| h.distance(o) < 8.0) {
                        self.checkpoint();
                        return Some(Drag::Scale { handle: k, b0: b, orig: self.selected_shapes_deep() });
                    }
                }
                let hit = self.hit(w, cmd, tol);
                match hit {
                    Some(id) => {
                        if !self.sel.contains(&id) {
                            if shift {
                                self.sel.push(id);
                            } else {
                                self.sel = vec![id];
                            }
                        }
                        self.sel_board = None;
                        self.checkpoint();
                        // Alt : glisser une copie
                        if alt {
                            self.duplicate(false);
                        }
                        let bounds = self.selection_bounds().unwrap_or([w[0], w[1], w[0], w[1]]);
                        Some(Drag::Move { from: w, orig: self.selected_shapes_deep(), bounds })
                    }
                    None => {
                        if !shift {
                            self.sel.clear();
                        }
                        Some(Drag::Marquee(o))
                    }
                }
            }
            Tool::Direct => {
                // Un point ou une poignée d'un tracé choisi
                for id in self.sel.clone() {
                    let Some(s) = self.doc.get(id) else { continue };
                    let Kind::Path { nodes, .. } = &s.kind else { continue };
                    let (cs, sn) = (s.rot.to_radians().cos(), s.rot.to_radians().sin());
                    let ws = |q: [f32; 2]| to_screen([s.x + q[0] * cs - q[1] * sn, s.y + q[0] * sn + q[1] * cs]);
                    for (k, n) in nodes.iter().enumerate() {
                        for (part, q) in [(1u8, n.i), (2, n.o), (0, n.p)] {
                            if (part == 0 || q != n.p) && ws(q).distance(o) < 7.0 {
                                self.checkpoint();
                                if part == 0 && !self.nodes_sel.contains(&(id, k)) {
                                    if !shift {
                                        self.nodes_sel.clear();
                                    }
                                    self.nodes_sel.push((id, k));
                                }
                                return Some(Drag::Node { id, k, part });
                            }
                        }
                    }
                }
                let hit = self.hit(w, true, tol);
                self.sel = hit.into_iter().collect();
                self.nodes_sel.clear();
                None
            }
            Tool::Pen => self.pen_point(w).map(|id| Drag::PenHandle { id }),
            Tool::Rect | Tool::Ellipse | Tool::Polygon | Tool::Star | Tool::Line | Tool::Text => {
                self.checkpoint();
                let w = self.snap_point(w);
                let kind = match self.tool {
                    Tool::Rect => Kind::Rect { w: 1.0, h: 1.0, r: [0.0; 4] },
                    Tool::Ellipse => Kind::Ellipse { w: 1.0, h: 1.0 },
                    Tool::Polygon => Kind::Polygon { sides: 6, r: 1.0 },
                    Tool::Star => Kind::Star { points: 5, r1: 1.0, r2: 0.45 },
                    Tool::Line => Kind::Line { x2: 0.0, y2: 0.0 },
                    _ => Kind::Text { text: crate::i18n::t("Texte"), size: 48.0, font: "inter".into(), align: 0, width: Some(1.0), spacing: 0.0, leading: 1.2 },
                };
                let id = self.new_shape(kind, w[0], w[1]);
                self.sel = vec![id];
                Some(Drag::Create { from: w, id })
            }
            Tool::Board => Some(Drag::BoardCreate { from: self.snap_point(w) }),
            Tool::Pencil => {
                self.checkpoint();
                let id = self.new_shape(Kind::Path { nodes: vec![Node::corner([0.0, 0.0])], closed: false }, w[0], w[1]);
                self.sel = vec![id];
                Some(Drag::Pencil { id, pts: vec![w] })
            }
            Tool::Width | Tool::Builder => None,
        }
    }

    /// La plume : un point de plus (ou le premier) ; sur le premier point, le tracé se ferme.
    /// Rend le tracé dont on peut tirer la poignée du dernier point.
    fn pen_point(&mut self, w: [f32; 2]) -> Option<u64> {
        let w = self.snap_point(w);
        match self.pen {
            Some(id) => {
                let first = self.doc.get(id).and_then(|s| match &s.kind {
                    Kind::Path { nodes, .. } if nodes.len() > 2 => nodes.first().map(|n| [n.p[0] + s.x, n.p[1] + s.y]),
                    _ => None,
                });
                if first.is_some_and(|f| (f[0] - w[0]).hypot(f[1] - w[1]) * self.zoom < 9.0) {
                    if let Some(s) = self.shape_mut(id) {
                        if let Kind::Path { closed, .. } = &mut s.kind {
                            *closed = true;
                        }
                        if s.fills.is_empty() {
                            s.fills.push(Paint::solid([236, 108, 89, 255]));
                        }
                    }
                    self.pen = None;
                    self.tool = Tool::Select;
                    self.changed();
                    return None;
                }
                self.checkpoint();
                if let Some(s) = self.shape_mut(id) {
                    let (x, y) = (s.x, s.y);
                    if let Kind::Path { nodes, .. } = &mut s.kind {
                        nodes.push(Node::corner([w[0] - x, w[1] - y]));
                    }
                }
                self.changed();
                Some(id)
            }
            None => {
                self.checkpoint();
                let id = self.new_shape(Kind::Path { nodes: vec![Node::corner([0.0, 0.0])], closed: false }, w[0], w[1]);
                self.pen = Some(id);
                self.sel = vec![id];
                self.changed();
                Some(id)
            }
        }
    }

    /// Les formes choisies et tout leur contenu, telles qu'au début du geste
    fn selected_shapes_deep(&self) -> Vec<Shape> {
        let mut ids: Vec<u64> = self.sel.clone();
        for &i in &self.sel {
            ids.extend(self.doc.descendants(i));
        }
        self.doc.shapes.iter().filter(|s| ids.contains(&s.id)).cloned().collect()
    }

    fn update(&mut self, w: [f32; 2], d: egui::Vec2, shift: bool, alt: bool) {
        // Crayon : un point de plus quand la souris a assez bougé ; le trait brut s'affiche pendant le geste
        if let Some(Drag::Pencil { id, pts }) = &mut self.drag {
            let id = *id;
            if pts.last().is_none_or(|q| (q[0] - w[0]).hypot(q[1] - w[1]) * self.zoom > 1.5) {
                pts.push(w);
                let o = pts[0];
                let nodes: Vec<Node> = pts.iter().map(|q| Node::corner([q[0] - o[0], q[1] - o[1]])).collect();
                if let Some(s) = self.shape_mut(id) {
                    s.kind = Kind::Path { nodes, closed: false };
                }
            }
            return;
        }
        let Some(drag) = self.drag.take() else { return };
        match &drag {
            Drag::Pan => {
                self.center[0] -= d.x / self.zoom;
                self.center[1] -= d.y / self.zoom;
            }
            Drag::Marquee(_) | Drag::Pencil { .. } => {}
            Drag::Move { from, orig, bounds } => {
                let (mut dx, mut dy) = (w[0] - from[0], w[1] - from[1]);
                if shift {
                    if dx.abs() > dy.abs() {
                        dy = 0.0;
                    } else {
                        dx = 0.0;
                    }
                }
                let moved = [bounds[0] + dx, bounds[1] + dy, bounds[2] + dx, bounds[3] + dy];
                let ex = self.sel.clone();
                let s = self.snap_box(moved, &ex);
                for o in orig {
                    if let Some(sh) = self.shape_mut(o.id) {
                        sh.x = o.x + dx + s[0];
                        sh.y = o.y + dy + s[1];
                    }
                }
                self.changed();
            }
            Drag::Scale { handle, b0, orig } => {
                // Le coin ou le bord opposé reste fixe
                let (hx, hy) = HANDLE_SIDES[*handle];
                let mut b1 = *b0;
                let w = self.snap_point(w);
                if hx < 0 {
                    b1[0] = w[0];
                } else if hx > 0 {
                    b1[2] = w[0];
                }
                if hy < 0 {
                    b1[1] = w[1];
                } else if hy > 0 {
                    b1[3] = w[1];
                }
                // Maj : garder les proportions (coins)
                if shift && hx != 0 && hy != 0 {
                    let k = ((b1[2] - b1[0]) / (b0[2] - b0[0]).max(1e-3)).abs().max(((b1[3] - b1[1]) / (b0[3] - b0[1]).max(1e-3)).abs());
                    let (bw, bh) = ((b0[2] - b0[0]) * k, (b0[3] - b0[1]) * k);
                    if hx < 0 {
                        b1[0] = b1[2] - bw;
                    } else {
                        b1[2] = b1[0] + bw;
                    }
                    if hy < 0 {
                        b1[1] = b1[3] - bh;
                    } else {
                        b1[3] = b1[1] + bh;
                    }
                }
                if alt {
                    // Alt : depuis le centre
                    let c = [(b0[0] + b0[2]) / 2.0, (b0[1] + b0[3]) / 2.0];
                    if hx != 0 {
                        let half = ((b1[2] - b1[0]) / 2.0).abs();
                        b1[0] = c[0] - half;
                        b1[2] = c[0] + half;
                    }
                    if hy != 0 {
                        let half = ((b1[3] - b1[1]) / 2.0).abs();
                        b1[1] = c[1] - half;
                        b1[3] = c[1] + half;
                    }
                }
                if (b1[2] - b1[0]).abs() > 0.5 && (b1[3] - b1[1]).abs() > 0.5 {
                    let b1 = [b1[0].min(b1[2]), b1[1].min(b1[3]), b1[0].max(b1[2]), b1[1].max(b1[3])];
                    for o in orig.clone() {
                        self.rescale(&o, *b0, b1);
                    }
                    self.changed();
                }
            }
            Drag::Rotate { center, a0, orig } => {
                let mut da = (w[1] - center[1]).atan2(w[0] - center[0]) - a0;
                if shift {
                    let step = 15f32.to_radians();
                    da = (da / step).round() * step;
                }
                let (cs, sn) = (da.cos(), da.sin());
                for o in orig {
                    if let Some(s) = self.shape_mut(o.id) {
                        let (dx, dy) = (o.x - center[0], o.y - center[1]);
                        s.x = center[0] + dx * cs - dy * sn;
                        s.y = center[1] + dx * sn + dy * cs;
                        if !matches!(s.kind, Kind::Group { .. }) {
                            s.rot = o.rot + da.to_degrees();
                        }
                    }
                }
                self.changed();
            }
            Drag::Create { from, id } => {
                let w = self.snap_point(w);
                let (mut dx, mut dy) = (w[0] - from[0], w[1] - from[1]);
                let tool = self.tool;
                if shift && tool != Tool::Line {
                    let m = dx.abs().max(dy.abs());
                    dx = m * dx.signum();
                    dy = m * dy.signum();
                }
                if shift && tool == Tool::Line {
                    let a = dy.atan2(dx);
                    let step = std::f32::consts::FRAC_PI_4;
                    let a = (a / step).round() * step;
                    let len = (dx * dx + dy * dy).sqrt();
                    dx = a.cos() * len;
                    dy = a.sin() * len;
                }
                let from = *from;
                if let Some(s) = self.shape_mut(*id) {
                    let (x0, y0) = if alt { (from[0] - dx, from[1] - dy) } else { (from[0].min(from[0] + dx), from[1].min(from[1] + dy)) };
                    let (bw, bh) = if alt { (dx.abs() * 2.0, dy.abs() * 2.0) } else { (dx.abs(), dy.abs()) };
                    match &mut s.kind {
                        Kind::Rect { w, h, .. } | Kind::Ellipse { w, h } => {
                            (s.x, s.y) = (x0, y0);
                            (*w, *h) = (bw.max(1.0), bh.max(1.0));
                        }
                        Kind::Polygon { r, .. } => *r = (dx * dx + dy * dy).sqrt().max(1.0),
                        Kind::Star { r1, r2, .. } => {
                            *r1 = (dx * dx + dy * dy).sqrt().max(1.0);
                            *r2 = *r1 * 0.45;
                        }
                        Kind::Line { x2, y2 } => (*x2, *y2) = (dx, dy),
                        Kind::Text { width, .. } => {
                            (s.x, s.y) = (x0, y0);
                            *width = Some(bw.max(1.0));
                        }
                        _ => {}
                    }
                }
                self.changed();
            }
            Drag::Node { id, k, part } => {
                let (id, k, part) = (*id, *k, *part);
                let w = if part == 0 { self.snap_point(w) } else { w };
                if let Some(s) = self.shape_mut(id) {
                    let (cs, sn) = ((-s.rot).to_radians().cos(), (-s.rot).to_radians().sin());
                    let (dx, dy) = (w[0] - s.x, w[1] - s.y);
                    let lp = [dx * cs - dy * sn, dx * sn + dy * cs];
                    if let Kind::Path { nodes, .. } = &mut s.kind {
                        if let Some(n) = nodes.get_mut(k) {
                            match part {
                                0 => {
                                    let (ox, oy) = (lp[0] - n.p[0], lp[1] - n.p[1]);
                                    n.p = lp;
                                    n.i = [n.i[0] + ox, n.i[1] + oy];
                                    n.o = [n.o[0] + ox, n.o[1] + oy];
                                }
                                1 => {
                                    n.i = lp;
                                    // Sans Alt, la poignée opposée suit en miroir (point lisse)
                                    if !alt {
                                        n.o = [2.0 * n.p[0] - lp[0], 2.0 * n.p[1] - lp[1]];
                                    }
                                }
                                _ => {
                                    n.o = lp;
                                    if !alt {
                                        n.i = [2.0 * n.p[0] - lp[0], 2.0 * n.p[1] - lp[1]];
                                    }
                                }
                            }
                        }
                    }
                }
                self.changed();
            }
            Drag::PenHandle { id } => {
                let zoom = self.zoom;
                if let Some(s) = self.shape_mut(*id) {
                    let (x, y) = (s.x, s.y);
                    if let Kind::Path { nodes, .. } = &mut s.kind {
                        if let Some(n) = nodes.last_mut() {
                            let lp = [w[0] - x, w[1] - y];
                            if (lp[0] - n.p[0]).hypot(lp[1] - n.p[1]) * zoom > 3.0 {
                                n.o = lp;
                                n.i = [2.0 * n.p[0] - lp[0], 2.0 * n.p[1] - lp[1]];
                            }
                        }
                    }
                }
                self.changed();
            }
            Drag::BoardMove { i, from, orig, shapes } => {
                let (dx, dy) = (w[0] - from[0], w[1] - from[1]);
                if let Some(b) = self.doc.boards.get_mut(*i) {
                    b.x = orig.x + dx;
                    b.y = orig.y + dy;
                }
                for (id, x, y) in shapes.clone() {
                    let cur = self.doc.get(id).map(|s| (s.x, s.y));
                    if let Some((cx, cy)) = cur {
                        self.doc.translate(id, x + dx - cx, y + dy - cy);
                    }
                }
                self.changed();
            }
            Drag::BoardCreate { .. } => {}
        }
        self.drag = Some(drag);
    }

    fn end(&mut self, resp: &egui::Response, to_world: &dyn Fn(Pos2) -> [f32; 2], _shift: bool) {
        let drag = self.drag.take();
        self.snaps.clear();
        match drag {
            Some(Drag::Marquee(a)) => {
                if let Some(b) = resp.interact_pointer_pos() {
                    let (wa, wb) = (to_world(a), to_world(b));
                    let r = [wa[0].min(wb[0]), wa[1].min(wb[1]), wa[0].max(wb[0]), wa[1].max(wb[1])];
                    for s in self.doc.shapes.iter().filter(|s| s.parent == 0 && s.visible && !s.locked) {
                        let bb = self.doc.bounds(s);
                        if bb[0] < r[2] && bb[2] > r[0] && bb[1] < r[3] && bb[3] > r[1] && !self.sel.contains(&s.id) {
                            self.sel.push(s.id);
                        }
                    }
                }
            }
            Some(Drag::Pencil { id, pts }) => {
                // Lissé : peu de nœuds, courbes douces ; fini près du départ, le tracé se ferme
                let o = pts[0];
                let local: Vec<[f32; 2]> = pts.iter().map(|q| [q[0] - o[0], q[1] - o[1]]).collect();
                let near = pts.len() > 8 && (pts[0][0] - pts[pts.len() - 1][0]).hypot(pts[0][1] - pts[pts.len() - 1][1]) * self.zoom < 12.0;
                if local.len() < 2 {
                    self.doc.remove(&[id]);
                    self.sel.clear();
                } else {
                    let nodes = crate::vector::fit_nodes(&local, 2.5 / self.zoom, near);
                    if let Some(s) = self.shape_mut(id) {
                        s.kind = Kind::Path { nodes, closed: near };
                    }
                }
                self.changed();
            }
            Some(Drag::Create { id, .. }) => {
                // Un simple clic : une forme de taille par défaut
                let tiny = self.doc.get(id).map(|s| self.doc.bounds(s)).is_some_and(|b| (b[2] - b[0]) < 3.0 && (b[3] - b[1]) < 3.0);
                if let Some(s) = self.shape_mut(id) {
                    if tiny {
                        match &mut s.kind {
                            Kind::Rect { w, h, .. } | Kind::Ellipse { w, h } => (*w, *h) = (160.0, 160.0),
                            Kind::Polygon { r, .. } => *r = 80.0,
                            Kind::Star { r1, r2, .. } => (*r1, *r2) = (80.0, 36.0),
                            Kind::Line { x2, .. } => *x2 = 160.0,
                            Kind::Text { width, .. } => *width = None,
                            _ => {}
                        }
                    } else if let Kind::Text { width: Some(w), .. } = &mut s.kind {
                        if *w < 20.0 {
                            *w = 300.0;
                        }
                    }
                }
                if matches!(self.doc.get(id).map(|s| &s.kind), Some(Kind::Text { .. })) {
                    self.focus_text = true;
                }
                // Après une forme, on revient à la sélection (comme Figma)
                self.tool = Tool::Select;
                self.changed();
            }
            Some(Drag::BoardCreate { from }) => {
                if let Some(b) = resp.interact_pointer_pos() {
                    let w = to_world(b);
                    let (x0, y0, x1, y1) = (from[0].min(w[0]), from[1].min(w[1]), from[0].max(w[0]), from[1].max(w[1]));
                    self.checkpoint();
                    let n = self.doc.boards.len() + 1;
                    let (bw, bh) = if x1 - x0 < 4.0 { (1080.0, 1080.0) } else { (x1 - x0, y1 - y0) };
                    self.doc.boards.push(Board { name: format!("{} {n}", crate::i18n::t("Plan de travail")), x: x0, y: y0, w: bw, h: bh, bg: Some([255, 255, 255, 255]), grid: None });
                    self.sel_board = Some(self.doc.boards.len() - 1);
                    self.sel.clear();
                    self.tool = Tool::Select;
                    self.changed();
                }
            }
            Some(Drag::PenHandle { .. }) | Some(Drag::Node { .. }) | Some(Drag::Move { .. }) | Some(Drag::Scale { .. }) | Some(Drag::Rotate { .. }) | Some(Drag::BoardMove { .. }) => self.changed(),
            _ => {}
        }
    }

    fn click(&mut self, w: [f32; 2], shift: bool, cmd: bool, tol: f32, double: bool, label_hit: Option<usize>) {
        if let Some(i) = label_hit {
            self.sel_board = Some(i);
            self.sel.clear();
            return;
        }
        match self.tool {
            Tool::Select => {
                match self.hit(w, cmd, tol) {
                    Some(id) => {
                        if shift {
                            if self.sel.contains(&id) {
                                self.sel.retain(|s| *s != id);
                            } else {
                                self.sel.push(id);
                            }
                        } else if !double {
                            self.sel = vec![id];
                        }
                        self.sel_board = None;
                    }
                    None => {
                        if !shift {
                            self.sel.clear();
                        }
                        // Clic dans un plan de travail vide : on le choisit
                        self.sel_board = self.doc.boards.iter().position(|b| w[0] >= b.x && w[0] <= b.x + b.w && w[1] >= b.y && w[1] <= b.y + b.h);
                    }
                }
            }
            Tool::Direct => {
                self.sel = self.hit(w, true, tol).into_iter().collect();
                self.nodes_sel.clear();
            }
            Tool::Pen => {
                let _ = self.pen_point(w);
            }
            // Un clic avec un outil de forme : une forme de taille par défaut, centrée sur le clic
            Tool::Rect | Tool::Ellipse | Tool::Polygon | Tool::Star | Tool::Line | Tool::Text => {
                self.checkpoint();
                let w = self.snap_point(w);
                let (kind, at) = match self.tool {
                    Tool::Rect => (Kind::Rect { w: 160.0, h: 160.0, r: [0.0; 4] }, [w[0] - 80.0, w[1] - 80.0]),
                    Tool::Ellipse => (Kind::Ellipse { w: 160.0, h: 160.0 }, [w[0] - 80.0, w[1] - 80.0]),
                    Tool::Polygon => (Kind::Polygon { sides: 6, r: 80.0 }, w),
                    Tool::Star => (Kind::Star { points: 5, r1: 80.0, r2: 36.0 }, w),
                    Tool::Line => (Kind::Line { x2: 160.0, y2: 0.0 }, w),
                    _ => (Kind::Text { text: crate::i18n::t("Texte"), size: 48.0, font: "inter".into(), align: 0, width: None, spacing: 0.0, leading: 1.2 }, w),
                };
                let id = self.new_shape(kind, at[0], at[1]);
                self.sel = vec![id];
                if self.tool == Tool::Text {
                    self.focus_text = true;
                }
                self.tool = Tool::Select;
                self.changed();
            }
            _ => {}
        }
    }

    fn duplicate(&mut self, offset: bool) {
        let mut new_sel = vec![];
        for id in self.sel.clone() {
            if let Some(nid) = self.clone_tree(id, if offset { 20.0 } else { 0.0 }) {
                new_sel.push(nid);
            }
        }
        // L'original reste sous la copie, la copie bouge
        self.sel = new_sel;
        self.changed();
    }

    /// Copie une forme (et le contenu d'un groupe) ; rend l'identifiant de la copie
    fn clone_tree(&mut self, id: u64, off: f32) -> Option<u64> {
        let s = self.doc.get(id)?.clone();
        let nid = self.doc.new_id();
        let mut c = s.clone();
        c.id = nid;
        c.x += off;
        c.y += off;
        // Copier un composant donne une instance (comme Figma)
        if s.component {
            c.component = false;
            c.instance_of = s.id;
            c.name = format!("{} · {}", s.name, crate::i18n::t("instance"));
        }
        if let Kind::Group { children, op } = &s.kind {
            let mut kids = vec![];
            for k in children {
                if let Some(kid) = self.clone_tree(*k, off) {
                    if let Some(ks) = self.shape_mut(kid) {
                        ks.parent = nid;
                    }
                    kids.push(kid);
                }
            }
            c.kind = Kind::Group { children: kids, op: *op };
        }
        let at = self.doc.index(id).map(|i| i + 1).unwrap_or(self.doc.shapes.len());
        // Le groupe passe après ses enfants copiés (ils ont été poussés à la fin)
        if matches!(c.kind, Kind::Group { .. }) {
            self.doc.shapes.push(c);
        } else {
            self.doc.shapes.insert(at.min(self.doc.shapes.len()), c);
        }
        if s.parent != 0 {
            if let Some(Shape { kind: Kind::Group { children, .. }, .. }) = self.shape_mut(s.parent) {
                children.push(nid);
            }
        }
        Some(nid)
    }

    fn keys(&mut self, ui: &egui::Ui, resp: &egui::Response) {
        let ctx = ui.ctx();
        if ctx.egui_wants_keyboard_input() || !(resp.hovered() || resp.has_focus() || self.drag.is_some()) {
            return;
        }
        let (cmd, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
        let k = |key: egui::Key| ui.input(|i| i.key_pressed(key));
        if cmd && k(egui::Key::Z) {
            self.undo(shift);
        } else if cmd && k(egui::Key::Y) {
            self.undo(true);
        } else if cmd && k(egui::Key::Num0) {
            self.fit = true;
        } else if cmd && k(egui::Key::D) {
            self.checkpoint();
            self.duplicate(true);
        } else if cmd && shift && k(egui::Key::G) {
            self.checkpoint();
            for id in self.sel.clone() {
                self.doc.ungroup(id);
            }
            self.sel.clear();
            self.changed();
        } else if !self.sel.is_empty() && ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::A, pressed: true, modifiers, .. } if modifiers.shift && !modifiers.command))) {
            // Maj+A : la sélection dans un cadre en mise en page automatique (comme Figma)
            self.checkpoint();
            self.auto_layout_selection();
            self.changed();
        } else if !self.sel.is_empty() && ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::K, pressed: true, modifiers, .. } if modifiers.command && modifiers.alt))) {
            // Ctrl+Alt+K : la sélection devient un composant (groupée si besoin)
            self.checkpoint();
            let gid = match self.sel.as_slice() {
                [one] if matches!(self.doc.get(*one).map(|s| &s.kind), Some(Kind::Group { .. })) => Some(*one),
                _ => {
                    let ids: Vec<u64> = self.sel.iter().map(|i| self.top_of(*i)).collect();
                    self.doc.group(&ids, 0)
                }
            };
            if let Some(g) = gid {
                if let Some(s) = self.doc.shapes.iter_mut().find(|s| s.id == g) {
                    s.component = true;
                    s.instance_of = 0;
                    if s.name.is_empty() || s.name == "Groupe" {
                        s.name = crate::i18n::t("Composant");
                    }
                }
                self.sel = vec![g];
            }
            self.changed();
        } else if cmd && k(egui::Key::J) && !self.sel.is_empty() {
            // Ctrl+J : joindre deux tracés ouverts, ou fermer un tracé
            let paths: Vec<u64> = self.sel.iter().copied().filter(|i| matches!(self.doc.get(*i).map(|s| &s.kind), Some(Kind::Path { .. }))).collect();
            if !paths.is_empty() && paths.len() <= 2 {
                self.checkpoint();
                if let Some(j) = self.doc.join_paths(paths[0], paths.get(1).copied()) {
                    self.sel = vec![j];
                }
                self.changed();
            }
        } else if cmd && k(egui::Key::G) && !self.sel.is_empty() {
            self.checkpoint();
            self.combine(0);
            self.changed();
        } else if cmd && k(egui::Key::A) {
            self.sel = self.doc.shapes.iter().filter(|s| s.parent == 0).map(|s| s.id).collect();
        } else if cmd && k(egui::Key::C) {
            self.clip = self.selected_shapes_deep();
        } else if (k(egui::Key::Delete) || k(egui::Key::Backspace)) && !self.nodes_sel.is_empty() && self.tool == Tool::Direct {
            self.checkpoint();
            let mut ns = std::mem::take(&mut self.nodes_sel);
            ns.sort_by(|a, b| b.1.cmp(&a.1));
            for (id, kk) in ns {
                if let Some(Shape { kind: Kind::Path { nodes, .. }, .. }) = self.shape_mut(id) {
                    if nodes.len() > 2 {
                        nodes.remove(kk);
                    }
                }
            }
            self.changed();
        } else if (k(egui::Key::Delete) || k(egui::Key::Backspace)) && !self.sel.is_empty() {
            self.checkpoint();
            let ids = self.sel.clone();
            self.doc.remove(&ids);
            self.sel.clear();
            self.changed();
        } else if k(egui::Key::Escape) || k(egui::Key::Enter) {
            self.pen = None;
            if self.tool == Tool::Pen {
                self.tool = Tool::Select;
            }
            if k(egui::Key::Escape) {
                self.sel.clear();
            }
        } else if !cmd {
            for (t, _, _, hk) in TOOLS {
                if let Some(kk) = egui::Key::from_name(hk) {
                    if k(kk) {
                        self.set_tool(t);
                    }
                }
            }
            // Flèches : décaler (Maj : de 10)
            let step = if shift { 10.0 } else { 1.0 };
            let mut d = [0.0f32, 0.0];
            if k(egui::Key::ArrowLeft) {
                d[0] -= step;
            }
            if k(egui::Key::ArrowRight) {
                d[0] += step;
            }
            if k(egui::Key::ArrowUp) {
                d[1] -= step;
            }
            if k(egui::Key::ArrowDown) {
                d[1] += step;
            }
            if d != [0.0, 0.0] && !self.sel.is_empty() {
                self.checkpoint();
                for id in self.sel.clone() {
                    self.doc.translate(id, d[0], d[1]);
                }
                self.changed();
            }
            // ] et [ : avancer, reculer d'un cran
            for (key, up) in [(egui::Key::CloseBracket, true), (egui::Key::OpenBracket, false)] {
                if k(key) && !self.sel.is_empty() {
                    self.checkpoint();
                    for id in self.sel.clone() {
                        if let Some(i) = self.doc.index(id) {
                            let j = if up { (i + 1).min(self.doc.shapes.len() - 1) } else { i.saturating_sub(1) };
                            self.doc.shapes.swap(i, j);
                        }
                    }
                    self.changed();
                }
            }
        }
        // Coller : des formes copiées, ou un SVG venu d'ailleurs (Recraft, navigateur)
        let pasted = ui.input(|i| i.events.iter().find_map(|e| if let egui::Event::Paste(t) = e { Some(t.clone()) } else { None }));
        if let Some(t) = pasted {
            if t.trim_start().starts_with("<svg") || t.trim_start().starts_with("<?xml") {
                self.import_svg(&t);
            } else if !self.clip.is_empty() {
                self.checkpoint();
                let roots: Vec<u64> = self.clip.iter().filter(|s| !self.clip.iter().any(|o| o.id == s.parent)).map(|s| s.id).collect();
                let mut new_sel = vec![];
                for id in roots {
                    if self.doc.get(id).is_some() {
                        if let Some(n) = self.clone_tree(id, 20.0) {
                            new_sel.push(n);
                        }
                    }
                }
                self.sel = new_sel;
                self.changed();
            }
        } else if cmd && k(egui::Key::V) && !self.clip.is_empty() {
            self.checkpoint();
            let roots: Vec<u64> = self.clip.iter().filter(|s| !self.clip.iter().any(|o| o.id == s.parent)).map(|s| s.id).collect();
            let mut new_sel = vec![];
            for id in roots {
                if let Some(n) = self.clone_tree(id, 20.0) {
                    new_sel.push(n);
                }
            }
            self.sel = new_sel;
            self.changed();
        }
    }
}

/// Les huit poignées d'un cadre : coins puis milieux des bords
fn handles(r: Rect) -> [Pos2; 8] {
    [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.center_top(), r.right_center(), r.center_bottom(), r.left_center()]
}

/// Pour chaque poignée : quel côté elle déplace (-1 gauche/haut, 1 droite/bas, 0 aucun)
const HANDLE_SIDES: [(i8, i8); 8] = [(-1, -1), (1, -1), (1, 1), (-1, 1), (0, -1), (1, 0), (0, 1), (-1, 0)];

const BOOLS: [(&str, &str); 5] = [
    (l::GROUP, "Grouper (Ctrl+G)"),
    (l::SQUARES_UNITE, "Union : une seule forme, la couleur de celle du dessous"),
    (l::SQUARES_SUBTRACT, "Soustraire : les formes du dessus creusent celle du dessous"),
    (l::SQUARES_INTERSECT, "Intersection : seulement la partie commune"),
    (l::SQUARES_EXCLUDE, "Exclure : tout sauf la partie commune"),
];

const BLENDS: [(&str, &str); 9] = [("normal", "Normal"), ("multiply", "Produit"), ("screen", "Superposition"), ("overlay", "Incrustation"), ("darken", "Obscurcir"), ("lighten", "Éclaircir"), ("color-dodge", "Densité couleur −"), ("difference", "Différence"), ("color", "Couleur")];

const BOARD_PRESETS: [(&str, f32, f32); 10] = [
    ("Carré (1080 × 1080)", 1080.0, 1080.0),
    ("Story (1080 × 1920)", 1080.0, 1920.0),
    ("Publication 4:5 (1080 × 1350)", 1080.0, 1350.0),
    ("Bannière YouTube (2560 × 1440)", 2560.0, 1440.0),
    ("Vignette YouTube (1280 × 720)", 1280.0, 720.0),
    ("Écran HD (1920 × 1080)", 1920.0, 1080.0),
    ("iPhone (393 × 852)", 393.0, 852.0),
    ("Bureau (1440 × 1024)", 1440.0, 1024.0),
    ("Lettre US (816 × 1056)", 816.0, 1056.0),
    ("A4 (794 × 1123)", 794.0, 1123.0),
];

/// Une peinture : sa couleur, et le genre (unie, dégradé linéaire, radial)
fn paint_ui(ui: &mut egui::Ui, p: &mut Paint, k: usize) -> bool {
    let mut ch = false;
    let kind = match p {
        Paint::Solid { .. } => 0,
        Paint::Linear { .. } => 1,
        Paint::Radial { .. } => 2,
    };
    let mut new_kind = kind;
    egui::ComboBox::from_id_salt(("vec-paint", k)).width(70.0).selected_text(crate::i18n::t(["uni", "linéaire", "radial"][kind])).show_ui(ui, |ui| {
        for (v, name) in ["uni", "linéaire", "radial"].iter().enumerate() {
            ui.selectable_value(&mut new_kind, v, crate::i18n::t(name));
        }
    });
    if new_kind != kind {
        let c = p.first();
        let stops = vec![(0.0, c), (1.0, [255, 255, 255, 255])];
        *p = match new_kind {
            0 => Paint::Solid { c },
            1 => Paint::Linear { stops, angle: 0.0 },
            _ => Paint::Radial { stops },
        };
        ch = true;
    }
    match p {
        Paint::Solid { c } => {
            let mut col = c32(*c);
            if ui.color_edit_button_srgba(&mut col).changed() {
                *c = arr(col);
                ch = true;
            }
        }
        Paint::Linear { stops, angle } => {
            for s in stops.iter_mut() {
                let mut col = c32(s.1);
                if ui.color_edit_button_srgba(&mut col).changed() {
                    s.1 = arr(col);
                    ch = true;
                }
            }
            ch |= ui.add(egui::DragValue::new(angle).speed(1.0).suffix("°")).changed();
        }
        Paint::Radial { stops } => {
            for s in stops.iter_mut() {
                let mut col = c32(s.1);
                if ui.color_edit_button_srgba(&mut col).changed() {
                    s.1 = arr(col);
                    ch = true;
                }
            }
        }
    }
    ch
}

