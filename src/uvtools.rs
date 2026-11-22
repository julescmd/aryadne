//! Outils de coutures et d'UV façon Blender : sélection d'arêtes (boucle avec Alt, plus court
//! chemin avec Ctrl), menus contextuels (clic droit, Ctrl+E, U), et un éditeur UV complet :
//! sommets ou îlots, sélection au rectangle, G R S avec contrainte d'axe, épingles, dépliage de
//! la sélection en gardant les épingles, alignement, miroir, rotation, échelle moyenne.
use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Menu {
    /// Vue 3D, outil coutures (arêtes)
    Edges,
    /// Vue 3D, outil faces
    Faces,
    /// Éditeur UV
    Uv,
    /// U : menu de dépliage
    Unwrap,
}

struct Modal {
    /// 0 déplacer, 1 tourner, 2 échelle
    kind: u8,
    axis: Option<usize>,
    from: Pos2,
    orig: Vec<((u32, u32), V2)>,
    center: V2,
}

#[derive(Default)]
pub struct State {
    /// Arêtes choisies dans la vue 3D
    pub esel: BTreeSet<(u32, u32)>,
    pub last_edge: Option<(u32, u32)>,
    /// Coins d'UV choisis (face, coin)
    pub uvsel: BTreeSet<(u32, u32)>,
    /// Éditeur UV : choisir des îlots entiers plutôt que des sommets
    pub islands: bool,
    /// Cadrage de l'éditeur UV : centre (en UV) et pixels par unité d'UV
    view: Option<(V2, f32)>,
    modal: Option<Modal>,
    boxing: Option<Pos2>,
    /// Menu contextuel ouvert : position et genre
    pub menu: Option<(Pos2, Menu)>,
    /// Dépliage en direct : déplacer des épingles redéplie leur îlot
    pub live: bool,
    /// L'éditeur UV n'est pas à côté de la vue 3D (seulement dans le panneau)
    pub no_split: bool,
}

const EPS_UV: f32 = 1e-5;

impl Edit {
    // ---------- arêtes dans la vue 3D ----------

    /// L'arête du polygone touché la plus proche du point cliqué.
    pub fn edge_at(&self, prep: &crate::model3d::Prepared, tri: usize, bary: [f32; 3]) -> Option<(u32, u32)> {
        let &f = self.owner.get(tri)?;
        let t = prep.triangle(tri);
        let p: Vec<V3> = t.iter().map(|&i| prep.vertex(i as usize)).collect();
        let hit = [p[0][0] * bary[0] + p[1][0] * bary[1] + p[2][0] * bary[2], p[0][1] * bary[0] + p[1][1] * bary[1] + p[2][1] * bary[2], p[0][2] * bary[0] + p[1][2] * bary[1] + p[2][2] * bary[2]];
        let poly = self.topo.polys.get(f as usize)?;
        let mut best = (f32::MAX, (0, 0));
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            let (pa, pb) = (prep.vertex(a as usize), prep.vertex(b as usize));
            let ab = sub(pb, pa);
            let tt = (dot(sub(hit, pa), ab) / dot(ab, ab).max(1e-12)).clamp(0.0, 1.0);
            let dd = len(sub(hit, add(pa, mul(ab, tt))));
            if dd < best.0 {
                best = (dd, key(a, b));
            }
        }
        Some(best.1)
    }

    fn neighbors(&self) -> HashMap<u32, Vec<u32>> {
        let mut nb: HashMap<u32, Vec<u32>> = HashMap::new();
        for &(a, b) in self.topo.edge_faces.keys() {
            nb.entry(a).or_default().push(b);
            nb.entry(b).or_default().push(a);
        }
        nb
    }

    /// Boucle d'arêtes passant par `e` (comme Alt+clic dans Blender) : à chaque sommet de
    /// valence 4, on continue par l'arête qui ne touche aucune face de l'arête d'arrivée.
    pub fn edge_loop(&self, e: (u32, u32)) -> Vec<(u32, u32)> {
        let nb = self.neighbors();
        let faces_of = |x: (u32, u32)| self.topo.edge_faces.get(&key(x.0, x.1)).cloned().unwrap_or_default();
        let mut out: BTreeSet<(u32, u32)> = [key(e.0, e.1)].into_iter().collect();
        for (from, to) in [(e.0, e.1), (e.1, e.0)] {
            let (mut u, mut v) = (from, to);
            for _ in 0..100_000 {
                let around = nb.get(&v).cloned().unwrap_or_default();
                if around.len() != 4 {
                    break;
                }
                let here = faces_of((u, v));
                let next: Vec<u32> = around.into_iter().filter(|&w| w != u && !faces_of((v, w)).iter().any(|f| here.contains(f))).collect();
                if next.len() != 1 {
                    break;
                }
                let k = key(v, next[0]);
                if !out.insert(k) {
                    break;
                }
                (u, v) = (v, next[0]);
            }
        }
        out.into_iter().collect()
    }

    /// Plus court chemin d'arêtes entre deux arêtes (Ctrl+clic dans Blender).
    pub fn edge_path(&self, a: (u32, u32), b: (u32, u32)) -> Vec<(u32, u32)> {
        let nb = self.neighbors();
        let p = |v: u32| self.topo.pos[v as usize];
        // Départ : les deux bouts de `a` ; arrivée : le premier bout de `b` atteint
        let mut dist: HashMap<u32, f32> = HashMap::new();
        let mut prev: HashMap<u32, u32> = HashMap::new();
        let mut heap = std::collections::BinaryHeap::new();
        for s in [a.0, a.1] {
            dist.insert(s, 0.0);
            heap.push((std::cmp::Reverse(0u64), s));
        }
        let goal = [b.0, b.1];
        let mut reached = None;
        while let Some((std::cmp::Reverse(dk), v)) = heap.pop() {
            let d = dk as f32 / 1e6;
            if d > dist.get(&v).copied().unwrap_or(f32::MAX) + 1e-6 {
                continue;
            }
            if goal.contains(&v) {
                reached = Some(v);
                break;
            }
            for &w in nb.get(&v).map(|x| x.as_slice()).unwrap_or(&[]) {
                let nd = d + len(sub(p(v), p(w)));
                if nd < dist.get(&w).copied().unwrap_or(f32::MAX) {
                    dist.insert(w, nd);
                    prev.insert(w, v);
                    heap.push((std::cmp::Reverse((nd * 1e6) as u64), w));
                }
            }
        }
        let mut out = vec![key(b.0, b.1)];
        if let Some(mut v) = reached {
            while let Some(&u) = prev.get(&v) {
                out.push(key(u, v));
                v = u;
            }
        }
        out
    }

    /// Clic sur une arête : la choisir (Maj : ajouter/retirer, Alt : sa boucle, Ctrl : chemin depuis la dernière).
    pub fn click_edge(&mut self, e: (u32, u32), shift: bool, ctrl: bool, alt: bool) {
        let e = key(e.0, e.1);
        let st = &mut self.tools;
        if alt {
            let lp = self.edge_loop(e);
            let st = &mut self.tools;
            if !shift {
                st.esel.clear();
            }
            // Maj+Alt sur une boucle déjà prise : on la retire
            if shift && lp.iter().all(|x| st.esel.contains(x)) {
                for x in lp {
                    st.esel.remove(&x);
                }
            } else {
                st.esel.extend(lp);
            }
        } else if ctrl {
            match st.last_edge {
                Some(l) if l != e => {
                    let path = self.edge_path(l, e);
                    self.tools.esel.extend(path);
                }
                _ => {
                    st.esel.insert(e);
                }
            }
        } else if shift {
            if !st.esel.remove(&e) {
                st.esel.insert(e);
            }
        } else {
            st.esel = [e].into_iter().collect();
        }
        self.tools.last_edge = Some(e);
        self.version += 1;
    }

    /// Les arêtes choisies deviennent des coutures (ou ne le sont plus).
    pub fn mark_seams(&mut self, on: bool) {
        // Sans arête choisie : le contour des faces choisies
        let edges: Vec<(u32, u32)> = if self.tools.esel.is_empty() { self.selection_border() } else { self.tools.esel.iter().copied().collect() };
        for e in edges {
            if on {
                self.data.seams.insert(e);
            } else {
                self.data.seams.remove(&e);
            }
        }
        self.changed();
    }

    fn selection_border(&self) -> Vec<(u32, u32)> {
        self.topo.edge_faces.iter().filter(|(_, fs)| fs.len() == 2 && fs.iter().filter(|f| self.sel.contains(f)).count() == 1).map(|(e, _)| *e).collect()
    }

    /// Coutures là où les îlots d'UV actuels se séparent (Blender : « Seams from Islands »).
    pub fn seams_from_islands(&mut self) {
        if !self.data.has_uv() {
            return;
        }
        let mut seams = BTreeSet::new();
        for (e, fs) in &self.topo.edge_faces {
            if fs.len() != 2 {
                continue;
            }
            let (f, g) = (fs[0] as usize, fs[1] as usize);
            if self.data.chart.get(f) != self.data.chart.get(g) {
                seams.insert(*e);
                continue;
            }
            // Même îlot mais les UV ne se touchent pas le long de l'arête : couture aussi
            let uv_of = |face: usize, v: u32| self.topo.polys[face].iter().position(|&x| x == v).map(|i| self.data.uv[face][i]);
            for v in [e.0, e.1] {
                if let (Some(a), Some(b)) = (uv_of(f, v), uv_of(g, v)) {
                    if (a[0] - b[0]).abs() > EPS_UV || (a[1] - b[1]).abs() > EPS_UV {
                        seams.insert(*e);
                    }
                }
            }
        }
        self.data.seams = seams;
        self.changed();
    }

    fn select_all(&mut self, on: bool) {
        match self.tool {
            Tool::Seams => {
                self.tools.esel = if on { self.topo.edge_faces.keys().copied().collect() } else { BTreeSet::new() };
            }
            _ => {
                self.sel = if on { (0..self.topo.polys.len() as u32).collect() } else { BTreeSet::new() };
            }
        }
        self.version += 1;
    }

    fn invert(&mut self) {
        match self.tool {
            Tool::Seams => {
                self.tools.esel = self.topo.edge_faces.keys().filter(|e| !self.tools.esel.contains(e)).copied().collect();
            }
            _ => {
                self.sel = (0..self.topo.polys.len() as u32).filter(|f| !self.sel.contains(f)).collect();
            }
        }
        self.version += 1;
    }

    /// Raccourcis de la vue 3D en édition (souris sur la vue).
    pub fn keys_3d(&mut self, ui: &egui::Ui, at: Option<Pos2>) {
        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        let (a, alt_a, ctrl_i, ctrl_e, u, ctrl_l) = ui.input(|i| {
            let m = i.modifiers;
            (
                i.key_pressed(egui::Key::A) && !m.alt && !m.command && !m.shift,
                i.key_pressed(egui::Key::A) && m.alt,
                i.key_pressed(egui::Key::I) && m.command,
                i.key_pressed(egui::Key::E) && m.command,
                i.key_pressed(egui::Key::U) && !m.any(),
                i.key_pressed(egui::Key::L) && m.command,
            )
        });
        if a {
            self.select_all(true);
        }
        if alt_a {
            self.select_all(false);
        }
        if ctrl_i {
            self.invert();
        }
        if ctrl_l {
            self.select_linked();
            self.version += 1;
        }
        if let Some(at) = at {
            if ctrl_e {
                self.tools.menu = Some((at, Menu::Edges));
            }
            if u {
                self.tools.menu = Some((at, Menu::Unwrap));
            }
        }
    }

    /// Ouvre le menu contextuel de la vue 3D (clic droit).
    pub fn open_menu_3d(&mut self, at: Pos2) {
        self.tools.menu = Some((at, if self.tool == Tool::Seams { Menu::Edges } else { Menu::Faces }));
    }

    /// Menu contextuel ouvert, s'il y en a un (à appeler une fois par image).
    pub fn context_menu(&mut self, ctx: &egui::Context) {
        let Some((at, kind)) = self.tools.menu else { return };
        let mut close = false;
        let mut act: Option<&'static str> = None;
        let area = egui::Area::new(egui::Id::unique("uv-context-menu")).order(egui::Order::Foreground).fixed_pos(at).constrain(true).show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(220.0);
                let mut item = |ui: &mut egui::Ui, label: &str, shortcut: &str, id: &'static str| {
                    let text = if shortcut.is_empty() { crate::i18n::t(label) } else { format!("{}    {shortcut}", crate::i18n::t(label)) };
                    if ui.button(text).clicked() {
                        act = Some(id);
                    }
                };
                match kind {
                    Menu::Edges => {
                        ui.label(RichText::new(crate::i18n::t("arêtes")).size(10.5).color(crate::theme::pal().muted));
                        item(ui, "marquer comme couture", "Ctrl+E", "mark");
                        item(ui, "effacer la couture", "", "clear");
                        ui.separator();
                        item(ui, "choisir les coutures", "", "sel-seams");
                        item(ui, "coutures depuis les îlots", "", "from-islands");
                        ui.separator();
                        item(ui, "tout choisir", "A", "all");
                        item(ui, "ne rien choisir", "Alt+A", "none");
                        item(ui, "inverser", "Ctrl+I", "invert");
                        ui.separator();
                        item(ui, "déplier", "U", "unwrap");
                    }
                    Menu::Faces => {
                        ui.label(RichText::new(crate::i18n::t("faces")).size(10.5).color(crate::theme::pal().muted));
                        item(ui, "déplier la sélection", "U", "unwrap-sel");
                        item(ui, "couture autour de la sélection", "", "mark");
                        item(ui, "îlot (faces reliées)", "Ctrl+L", "linked");
                        item(ui, "agrandir la sélection", "", "grow");
                        ui.separator();
                        item(ui, "tout choisir", "A", "all");
                        item(ui, "ne rien choisir", "Alt+A", "none");
                        item(ui, "inverser", "Ctrl+I", "invert");
                        if self.sel.len() > 0 && !self.data.slots.is_empty() {
                            ui.separator();
                            ui.label(RichText::new(crate::i18n::t("donner l'ID")).size(10.5).color(crate::theme::pal().muted));
                            for k in 0..self.data.slots.len().min(12) {
                                let s = &self.data.slots[k];
                                let c = Color32::from_rgb(s.color[0], s.color[1], s.color[2]);
                                if ui.button(RichText::new(format!("■ {}", s.name)).color(c)).clicked() {
                                    for &f in &self.sel {
                                        if let Some(m) = self.data.face_mat.get_mut(f as usize) {
                                            *m = k as u16;
                                        }
                                    }
                                    act = Some("ids");
                                }
                            }
                        }
                    }
                    Menu::Unwrap => {
                        ui.label(RichText::new(crate::i18n::t("UV")).size(10.5).color(crate::theme::pal().muted));
                        item(ui, "déplier", "", "unwrap");
                        item(ui, "déplier la sélection", "", "unwrap-sel");
                        item(ui, "coutures automatiques + déplier", "", "auto");
                        item(ui, "coutures intelligentes (projection)", "", "smart");
                        item(ui, "ranger les îlots", "", "pack");
                    }
                    Menu::Uv => {
                        ui.label(RichText::new(crate::i18n::t("UV")).size(10.5).color(crate::theme::pal().muted));
                        item(ui, "épingler", "P", "pin");
                        item(ui, "désépingler", "Alt+P", "unpin");
                        item(ui, "déplier la sélection", "U", "unwrap-sel");
                        ui.separator();
                        item(ui, "aligner à la verticale", "", "align-u");
                        item(ui, "aligner à l'horizontale", "", "align-v");
                        item(ui, "miroir horizontal", "", "flip-u");
                        item(ui, "miroir vertical", "", "flip-v");
                        item(ui, "tourner de 90°", "", "rot90");
                        ui.separator();
                        item(ui, "choisir l'îlot", "L", "island");
                        item(ui, "échelle moyenne des îlots", "", "average");
                        item(ui, "ranger les îlots", "", "pack");
                        item(ui, "coutures depuis les îlots", "", "from-islands");
                        ui.separator();
                        item(ui, "tout choisir", "A", "uv-all");
                        item(ui, "ne rien choisir", "Alt+A", "uv-none");
                    }
                }
            });
        });
        // Clic ailleurs ou Échap : on ferme
        let outside = ctx.input(|i| i.pointer.any_pressed()) && ctx.pointer_latest_pos().is_some_and(|p| !area.response.rect.contains(p));
        if outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if let Some(a) = act {
            self.run(a);
            close = true;
        }
        if close {
            self.tools.menu = None;
        }
    }

    /// Exécute une commande des menus.
    pub fn run(&mut self, act: &str) {
        match act {
            "mark" => self.mark_seams(true),
            "clear" => self.mark_seams(false),
            "sel-seams" => {
                self.tools.esel = self.data.seams.clone();
                self.version += 1;
            }
            "from-islands" => self.seams_from_islands(),
            "all" => self.select_all(true),
            "none" => self.select_all(false),
            "invert" => self.invert(),
            "linked" => {
                self.select_linked();
                self.version += 1;
            }
            "grow" => {
                self.grow(30.0);
                self.version += 1;
            }
            "ids" => self.changed(),
            "unwrap" => {
                self.tab = 2;
                self.unwrap_now();
            }
            "unwrap-sel" => {
                self.tab = 2;
                self.unwrap_selection();
            }
            "auto" => {
                self.tab = 2;
                self.auto_now(true);
            }
            "smart" => {
                self.data.seams = smart_seams(&self.topo, self.smart_angle, self.min_faces);
                self.tab = 2;
                self.unwrap_now();
            }
            "pack" => {
                if self.data.has_uv() {
                    pack(&self.topo, &mut self.data, &PackOptions { optimize: self.optimize, rotate: self.rotate, fixed_scale: None });
                    self.changed();
                }
            }
            "pin" => {
                let s: Vec<(u32, u32)> = self.tools.uvsel.iter().copied().collect();
                self.data.pins.extend(s);
                self.changed();
            }
            "unpin" => {
                for c in self.tools.uvsel.clone() {
                    self.data.pins.remove(&c);
                }
                self.changed();
            }
            "align-u" | "align-v" => {
                let k = if act == "align-u" { 0 } else { 1 };
                let sel: Vec<(u32, u32)> = self.tools.uvsel.iter().copied().collect();
                if !sel.is_empty() {
                    let avg = sel.iter().map(|&(f, i)| self.data.uv[f as usize][i as usize][k]).sum::<f32>() / sel.len() as f32;
                    for (f, i) in sel {
                        self.data.uv[f as usize][i as usize][k] = avg;
                    }
                    self.changed();
                }
            }
            "flip-u" | "flip-v" | "rot90" => {
                let sel: Vec<(u32, u32)> = self.tools.uvsel.iter().copied().collect();
                let c = self.uv_center(&sel);
                for (f, i) in sel {
                    let q = &mut self.data.uv[f as usize][i as usize];
                    let d = [q[0] - c[0], q[1] - c[1]];
                    *q = match act {
                        "flip-u" => [c[0] - d[0], q[1]],
                        "flip-v" => [q[0], c[1] - d[1]],
                        _ => [c[0] - d[1], c[1] + d[0]],
                    };
                }
                self.changed();
            }
            "island" => {
                let faces: BTreeSet<u32> = self.tools.uvsel.iter().map(|c| c.0).collect();
                self.select_islands_of(&faces);
            }
            "average" => {
                if self.data.has_uv() {
                    let dens = densities(&self.topo, &self.data, self.scale);
                    let avg = dens.iter().sum::<f32>() / dens.len().max(1) as f32;
                    set_density(&self.topo, &mut self.data, avg, self.scale);
                    self.changed();
                }
            }
            "uv-all" => {
                self.tools.uvsel = self.all_corners();
                self.version += 1;
            }
            "uv-none" => {
                self.tools.uvsel.clear();
                self.version += 1;
            }
            _ => {}
        }
    }

    // ---------- éditeur UV ----------

    fn all_corners(&self) -> BTreeSet<(u32, u32)> {
        self.data.uv.iter().enumerate().flat_map(|(f, c)| (0..c.len()).map(move |i| (f as u32, i as u32))).collect()
    }

    fn uv_center(&self, sel: &[(u32, u32)]) -> V2 {
        let n = sel.len().max(1) as f32;
        let s = sel.iter().fold([0.0f32; 2], |a, &(f, i)| {
            let q = self.data.uv[f as usize][i as usize];
            [a[0] + q[0], a[1] + q[1]]
        });
        [s[0] / n, s[1] / n]
    }

    /// Tous les coins qui forment le même sommet d'UV que (f, i) : même sommet du maillage, même
    /// îlot, même place.
    fn uv_twins(&self, f: u32, i: u32, by_vert: &HashMap<u32, Vec<(u32, u32)>>) -> Vec<(u32, u32)> {
        let v = self.topo.polys[f as usize][i as usize];
        let q = self.data.uv[f as usize][i as usize];
        let ch = self.data.chart.get(f as usize).copied();
        by_vert
            .get(&v)
            .map(|l| {
                l.iter()
                    .copied()
                    .filter(|&(g, j)| {
                        let r = self.data.uv[g as usize][j as usize];
                        self.data.chart.get(g as usize).copied() == ch && (r[0] - q[0]).abs() < EPS_UV && (r[1] - q[1]).abs() < EPS_UV
                    })
                    .collect()
            })
            .unwrap_or_else(|| vec![(f, i)])
    }

    fn corners_by_vertex(&self) -> HashMap<u32, Vec<(u32, u32)>> {
        let mut m: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
        for (f, p) in self.topo.polys.iter().enumerate() {
            for (i, &v) in p.iter().enumerate() {
                m.entry(v).or_default().push((f as u32, i as u32));
            }
        }
        m
    }

    /// Choisit les îlots entiers des faces données (coins d'UV et faces).
    fn select_islands_of(&mut self, faces: &BTreeSet<u32>) {
        let charts_: BTreeSet<u32> = faces.iter().filter_map(|f| self.data.chart.get(*f as usize).copied()).collect();
        for (f, c) in self.data.chart.iter().enumerate() {
            if charts_.contains(c) {
                self.sel.insert(f as u32);
                for i in 0..self.data.uv[f].len() {
                    self.tools.uvsel.insert((f as u32, i as u32));
                }
            }
        }
        self.version += 1;
    }

    /// Déplie seulement les faces choisies (ou les îlots des coins choisis). Les coins épinglés
    /// restent en place ; sans épingles, chaque îlot garde sa place et sa taille.
    pub fn unwrap_selection(&mut self) {
        if !self.data.has_uv() {
            self.unwrap_now();
            return;
        }
        let mut faces: BTreeSet<u32> = self.sel.clone();
        faces.extend(self.tools.uvsel.iter().map(|c| c.0));
        if faces.is_empty() {
            self.unwrap_now();
            return;
        }
        let chart = charts(&self.topo, &self.data.seams);
        let mut groups: HashMap<u32, Vec<usize>> = HashMap::new();
        for &f in &faces {
            groups.entry(chart[f as usize]).or_default().push(f as usize);
        }
        for (_, fs) in groups {
            let (corners, cvert) = chart_vertices(&self.topo, &fs, &self.data.seams);
            let mut pins: HashMap<usize, V2> = HashMap::new();
            for (k, &(f, i)) in corners.iter().enumerate() {
                if self.data.pins.contains(&(f as u32, i as u32)) {
                    pins.insert(cvert[k], self.data.uv[f][i]);
                }
            }
            let u = lscm_pinned(&self.topo, &fs, &corners, &cvert, &pins);
            if pins.len() < 2 {
                // Même centre et même surface qu'avant
                let old: Vec<V2> = corners.iter().map(|&(f, i)| self.data.uv[f][i]).collect();
                let new: Vec<V2> = (0..corners.len()).map(|k| u[cvert[k]]).collect();
                let bb = |v: &[V2]| v.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), q| ([lo[0].min(q[0]), lo[1].min(q[1])], [hi[0].max(q[0]), hi[1].max(q[1])]));
                let ((olo, ohi), (nlo, nhi)) = (bb(&old), bb(&new));
                let osz = ((ohi[0] - olo[0]) * (ohi[1] - olo[1])).max(1e-12).sqrt();
                let nsz = ((nhi[0] - nlo[0]) * (nhi[1] - nlo[1])).max(1e-12).sqrt();
                let k = if osz > 1e-6 { osz / nsz } else { 0.25 / nsz };
                let (oc, nc) = ([(olo[0] + ohi[0]) / 2.0, (olo[1] + ohi[1]) / 2.0], [(nlo[0] + nhi[0]) / 2.0, (nlo[1] + nhi[1]) / 2.0]);
                for (j, &(f, i)) in corners.iter().enumerate() {
                    let q = new[j];
                    self.data.uv[f][i] = [oc[0] + (q[0] - nc[0]) * k, oc[1] + (q[1] - nc[1]) * k];
                }
            } else {
                for (k, &(f, i)) in corners.iter().enumerate() {
                    self.data.uv[f][i] = u[cvert[k]];
                }
            }
        }
        self.data.chart = chart;
        self.changed();
    }

    /// L'éditeur UV dans `rect` : cadrage (molette, glisser du milieu), sélection, G R S,
    /// épingles et menu contextuel.
    pub fn uv_editor(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let p = crate::theme::pal();
        let resp = ui.interact(rect, ui.scope_id().with("uv-editor"), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(38));
        let (mut c, mut z) = self.tools.view.unwrap_or(([0.5, 0.5], rect.width().min(rect.height()) * 0.85));
        // Molette : zoom autour du pointeur ; glisser avec le bouton du milieu : déplacer la vue
        if resp.hovered() {
            let sc = ui.input(|i| i.smooth_scroll_delta.y);
            if sc != 0.0 {
                if let Some(m) = ui.input(|i| i.pointer.hover_pos()) {
                    let before = [c[0] + (m.x - rect.center().x) / z, c[1] - (m.y - rect.center().y) / z];
                    z = (z * (sc * 0.002).exp()).clamp(20.0, 50_000.0);
                    c = [before[0] - (m.x - rect.center().x) / z, before[1] + (m.y - rect.center().y) / z];
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Home)) {
                c = [0.5, 0.5];
                z = rect.width().min(rect.height()) * 0.85;
            }
        }
        if resp.dragged_by(egui::PointerButton::Middle) {
            let d = resp.drag_delta();
            c = [c[0] - d.x / z, c[1] + d.y / z];
        }
        self.tools.view = Some((c, z));
        let to = |q: V2| pos2(rect.center().x + (q[0] - c[0]) * z, rect.center().y - (q[1] - c[1]) * z);
        let from = |s: Pos2| -> V2 { [c[0] + (s.x - rect.center().x) / z, c[1] - (s.y - rect.center().y) / z] };
        // Grille et carré 0-1
        let sq = Rect::from_two_pos(to([0.0, 0.0]), to([1.0, 1.0]));
        painter.rect_filled(sq, 0.0, Color32::from_gray(46));
        for k in 0..=8 {
            let t = k as f32 / 8.0;
            let col = if k % 4 == 0 { Color32::from_gray(78) } else { Color32::from_gray(56) };
            painter.line_segment([to([t, 0.0]), to([t, 1.0])], Stroke::new(1.0, col));
            painter.line_segment([to([0.0, t]), to([1.0, t])], Stroke::new(1.0, col));
        }
        if !self.data.has_uv() {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, crate::i18n::t("pas encore d'UV : U → déplier"), egui::FontId::proportional(13.0), p.muted);
            if resp.secondary_clicked() {
                if let Some(at) = resp.interact_pointer_pos() {
                    self.tools.menu = Some((at, Menu::Unwrap));
                }
            }
            return;
        }
        let n = self.topo.polys.len();
        let step = (n / 80_000).max(1);
        let mut shapes = vec![];
        for f in (0..n).step_by(step) {
            let uv = &self.data.uv[f];
            if uv.len() < 3 {
                continue;
            }
            let pts: Vec<Pos2> = uv.iter().map(|q| to(*q)).collect();
            let selected = self.sel.contains(&(f as u32));
            let m = self.data.face_mat.get(f).copied().unwrap_or(0) as usize;
            let col = self.data.slots.get(m).map(|s| s.color).unwrap_or(PALETTE[0]);
            let fill = if selected { p.accent.gamma_multiply(0.35) } else { Color32::from_rgb(col[0], col[1], col[2]).gamma_multiply(0.22) };
            for k in 1..pts.len() - 1 {
                shapes.push(egui::Shape::convex_polygon(vec![pts[0], pts[k], pts[k + 1]], fill, Stroke::NONE));
            }
            shapes.push(egui::Shape::closed_line(pts, Stroke::new(0.7, Color32::from_gray(150))));
        }
        // Bords d'îlots (coutures) un peu plus marqués
        for &(a, b) in self.data.seams.iter().take(60_000) {
            if let Some(fs) = self.topo.edge_faces.get(&(a, b)) {
                for &f in fs {
                    let poly = &self.topo.polys[f as usize];
                    if let (Some(i), Some(j)) = (poly.iter().position(|&x| x == a), poly.iter().position(|&x| x == b)) {
                        shapes.push(egui::Shape::line_segment([to(self.data.uv[f as usize][i]), to(self.data.uv[f as usize][j])], Stroke::new(1.6, Color32::from_rgb(235, 80, 60))));
                    }
                }
            }
        }
        painter.extend(shapes);
        // Sommets choisis et épingles
        for &(f, i) in self.tools.uvsel.iter().take(200_000) {
            if let Some(q) = self.data.uv.get(f as usize).and_then(|u| u.get(i as usize)) {
                painter.circle_filled(to(*q), 2.6, p.accent);
            }
        }
        for &(f, i) in self.data.pins.iter() {
            if let Some(q) = self.data.uv.get(f as usize).and_then(|u| u.get(i as usize)) {
                painter.circle_filled(to(*q), 3.6, Color32::from_rgb(230, 50, 50));
            }
        }
        let hover = ui.input(|i| i.pointer.hover_pos()).filter(|h| rect.contains(*h));
        let mods = ui.input(|i| i.modifiers);
        // ---- transformation en cours (G, R, S)
        if let Some(m) = &mut self.tools.modal {
            for (k, kk) in [(0usize, egui::Key::X), (1, egui::Key::Y)] {
                if ui.input(|i| i.key_pressed(kk)) {
                    m.axis = if m.axis == Some(k) { None } else { Some(k) };
                }
            }
            let cur = ui.input(|i| i.pointer.latest_pos()).unwrap_or(m.from);
            let (kind, axis, center, from_s) = (m.kind, m.axis, m.center, m.from);
            let orig = m.orig.clone();
            let cs = to(center);
            for ((f, i), q) in &orig {
                let r = match kind {
                    0 => {
                        let d = [(cur.x - from_s.x) / z, -(cur.y - from_s.y) / z];
                        let d = match axis {
                            Some(0) => [d[0], 0.0],
                            Some(_) => [0.0, d[1]],
                            None => d,
                        };
                        [q[0] + d[0], q[1] + d[1]]
                    }
                    1 => {
                        let a0 = (from_s - cs).angle();
                        let a1 = (cur - cs).angle();
                        let t = -(a1 - a0);
                        let d = [q[0] - center[0], q[1] - center[1]];
                        [center[0] + d[0] * t.cos() - d[1] * t.sin(), center[1] + d[0] * t.sin() + d[1] * t.cos()]
                    }
                    _ => {
                        let k = (cur - cs).length() / (from_s - cs).length().max(6.0);
                        let d = [q[0] - center[0], q[1] - center[1]];
                        let (kx, ky) = match axis {
                            Some(0) => (k, 1.0),
                            Some(_) => (1.0, k),
                            None => (k, k),
                        };
                        [center[0] + d[0] * kx, center[1] + d[1] * ky]
                    }
                };
                self.data.uv[*f as usize][*i as usize] = r;
            }
            let (ok, cancel) = ui.input(|i| (i.pointer.primary_pressed() || i.key_pressed(egui::Key::Enter), i.pointer.secondary_pressed() || i.key_pressed(egui::Key::Escape)));
            let what = [crate::i18n::t("déplacer"), crate::i18n::t("tourner"), crate::i18n::t("échelle")][kind as usize].clone();
            let ax = axis.map(|k| format!(" · {}", ["U", "V"][k])).unwrap_or_default();
            painter.text(rect.left_bottom() + vec2(10.0, -8.0), egui::Align2::LEFT_BOTTOM, format!("{what}{ax}   ·   X Y : {}   ·   {}   ·   {}", crate::i18n::t("imposer un axe"), crate::i18n::t("clic : valider"), crate::i18n::t("Échap : annuler")), egui::FontId::monospace(11.0), p.accent);
            if cancel {
                for ((f, i), q) in orig {
                    self.data.uv[f as usize][i as usize] = q;
                }
                self.tools.modal = None;
                self.version += 1;
            } else if ok {
                self.tools.modal = None;
                // Dépliage en direct : les îlots aux épingles déplacées se redéplient
                if self.tools.live && self.tools.uvsel.iter().any(|c| self.data.pins.contains(c)) {
                    let keep_sel = std::mem::take(&mut self.sel);
                    self.sel = self.tools.uvsel.iter().map(|c| c.0).collect();
                    let faces: BTreeSet<u32> = self.sel.clone();
                    self.sel.clear();
                    self.select_islands_of(&faces);
                    let uvsel = self.tools.uvsel.clone();
                    self.unwrap_selection();
                    self.tools.uvsel = uvsel;
                    self.sel = keep_sel;
                }
                self.changed();
            } else {
                self.version += 1;
            }
            ui.ctx().request_repaint();
            return;
        }
        // ---- raccourcis (souris sur l'éditeur)
        if hover.is_some() && !ui.ctx().egui_wants_keyboard_input() {
            let k = |key: egui::Key| ui.input(|i| i.key_pressed(key));
            let plain = !mods.any();
            let start = if plain && k(egui::Key::G) {
                Some(0u8)
            } else if plain && k(egui::Key::R) {
                Some(1)
            } else if plain && k(egui::Key::S) {
                Some(2)
            } else {
                None
            };
            if let (Some(kind), Some(at)) = (start, hover) {
                self.begin_uv_modal(kind, at);
            }
            if plain && k(egui::Key::A) {
                self.run("uv-all");
            }
            if mods.alt && k(egui::Key::A) {
                self.run("uv-none");
            }
            if mods.command && k(egui::Key::I) {
                let all = self.all_corners();
                self.tools.uvsel = all.difference(&self.tools.uvsel).copied().collect();
                self.version += 1;
            }
            if plain && k(egui::Key::P) {
                self.run("pin");
            }
            if mods.alt && k(egui::Key::P) {
                self.run("unpin");
            }
            if plain && k(egui::Key::U) {
                self.run("unwrap-sel");
            }
            if plain && k(egui::Key::L) {
                if let Some(f) = hover.and_then(|h| self.face_under(from(h))) {
                    self.select_islands_of(&[f as u32].into_iter().collect());
                }
            }
        }
        // ---- souris
        if resp.secondary_clicked() {
            if let Some(at) = resp.interact_pointer_pos() {
                self.tools.menu = Some((at, Menu::Uv));
            }
        }
        if resp.double_clicked() {
            if let Some(f) = resp.interact_pointer_pos().and_then(|h| self.face_under(from(h))) {
                self.select_islands_of(&[f as u32].into_iter().collect());
            }
        } else if resp.clicked() {
            if let Some(at) = resp.interact_pointer_pos() {
                self.uv_pick(at, &to, &from, mods.shift, mods.command);
            }
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            let at = ui.input(|i| i.pointer.press_origin()).unwrap_or(rect.center());
            // Sur un sommet choisi : on le déplace ; ailleurs : sélection au rectangle
            let on_sel = self.tools.uvsel.iter().any(|&(f, i)| self.data.uv.get(f as usize).and_then(|u| u.get(i as usize)).is_some_and(|q| (to(*q) - at).length() < 7.0));
            let on_island = self.tools.islands && self.face_under(from(at)).is_some_and(|f| self.sel.contains(&(f as u32)));
            if on_sel || on_island {
                self.begin_uv_modal(0, at);
            } else {
                self.tools.boxing = Some(at);
            }
        }
        if let Some(start) = self.tools.boxing {
            if let Some(cur) = ui.input(|i| i.pointer.latest_pos()) {
                let r = Rect::from_two_pos(start, cur);
                painter.rect_filled(r, 0.0, p.accent.gamma_multiply(0.12));
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
                if !ui.input(|i| i.pointer.primary_down()) {
                    self.tools.boxing = None;
                    if !mods.shift && !mods.command {
                        self.tools.uvsel.clear();
                    }
                    let mut hit = vec![];
                    for (f, uv) in self.data.uv.iter().enumerate() {
                        for (i, q) in uv.iter().enumerate() {
                            if r.contains(to(*q)) {
                                hit.push((f as u32, i as u32));
                            }
                        }
                    }
                    if self.tools.islands {
                        let faces: BTreeSet<u32> = hit.iter().map(|c| c.0).collect();
                        self.select_islands_of(&faces);
                    } else if mods.command {
                        for c in hit {
                            self.tools.uvsel.remove(&c);
                        }
                    } else {
                        self.tools.uvsel.extend(hit);
                    }
                    self.version += 1;
                }
            }
        }
        // Barre du haut : mode sommets / îlots, dépliage en direct
        let bar = Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(rect.width() - 12.0, 22.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)));
        if child.selectable_label(!self.tools.islands, crate::i18n::t("sommets")).clicked() {
            self.tools.islands = false;
        }
        if child.selectable_label(self.tools.islands, crate::i18n::t("îlots")).clicked() {
            self.tools.islands = true;
        }
        child.checkbox(&mut self.tools.live, crate::i18n::t("dépliage en direct")).on_hover_text(crate::i18n::t("Déplacer des sommets épinglés redéplie leur îlot autour d'eux"));
        child.label(RichText::new(format!("{} {}", self.tools.uvsel.len(), crate::i18n::t("coin(s)"))).size(10.5).color(p.muted));
        if hover.is_some() && self.tools.modal.is_none() {
            painter.text(rect.left_bottom() + vec2(10.0, -8.0), egui::Align2::LEFT_BOTTOM, crate::i18n::t("clic : choisir · glisser : rectangle ou déplacer · G R S · P épingler · U déplier · L îlot · clic droit : menu"), egui::FontId::proportional(10.5), p.muted);
        }
    }

    fn face_under(&self, q: V2) -> Option<usize> {
        (0..self.data.uv.len()).rev().find(|&f| self.data.uv[f].len() >= 3 && point_in_poly(q, &self.data.uv[f]))
    }

    fn begin_uv_modal(&mut self, kind: u8, at: Pos2) {
        if self.tools.uvsel.is_empty() {
            return;
        }
        let sel: Vec<(u32, u32)> = self.tools.uvsel.iter().copied().collect();
        let center = self.uv_center(&sel);
        let orig = sel.iter().map(|&(f, i)| ((f, i), self.data.uv[f as usize][i as usize])).collect();
        self.tools.modal = Some(Modal { kind, axis: None, from: at, orig, center });
    }

    /// Clic dans l'éditeur UV : le sommet le plus proche (ou l'îlot sous la souris).
    fn uv_pick(&mut self, at: Pos2, to: &dyn Fn(V2) -> Pos2, from: &dyn Fn(Pos2) -> V2, shift: bool, ctrl: bool) {
        if self.tools.islands {
            match self.face_under(from(at)) {
                Some(f) => {
                    if !shift {
                        self.sel.clear();
                        self.tools.uvsel.clear();
                    }
                    self.select_islands_of(&[f as u32].into_iter().collect());
                }
                None if !shift => {
                    self.sel.clear();
                    self.tools.uvsel.clear();
                    self.version += 1;
                }
                None => {}
            }
            return;
        }
        let mut best: Option<((u32, u32), f32)> = None;
        for (f, uv) in self.data.uv.iter().enumerate() {
            for (i, q) in uv.iter().enumerate() {
                let d = (to(*q) - at).length();
                if d < 10.0 && best.is_none_or(|b| d < b.1) {
                    best = Some(((f as u32, i as u32), d));
                }
            }
        }
        let by_vert = self.corners_by_vertex();
        match best {
            Some(((f, i), _)) => {
                let twins = self.uv_twins(f, i, &by_vert);
                if ctrl {
                    for t in twins {
                        self.tools.uvsel.remove(&t);
                    }
                } else if shift && twins.iter().all(|t| self.tools.uvsel.contains(t)) {
                    for t in twins {
                        self.tools.uvsel.remove(&t);
                    }
                } else {
                    if !shift {
                        self.tools.uvsel.clear();
                    }
                    self.tools.uvsel.extend(twins);
                }
            }
            None if !shift && !ctrl => self.tools.uvsel.clear(),
            None => {}
        }
        self.version += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(n: u32) -> Topo {
        // Grille n × n de quads, sommets soudés
        let mut pos = vec![];
        for j in 0..=n {
            for i in 0..=n {
                pos.push([i as f32, 0.0, j as f32]);
            }
        }
        let id = |i: u32, j: u32| j * (n + 1) + i;
        let mut tris = vec![];
        let mut edges = vec![];
        for j in 0..n {
            for i in 0..n {
                let (a, b, c, d) = (id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1));
                tris.push([a, b, c]);
                tris.push([a, c, d]);
                edges.push(3);
                edges.push(6);
            }
        }
        Topo::new(&pos, &tris, &edges)
    }

    #[test]
    fn boucle_et_chemin() {
        let t = grid(4);
        let mesh = crate::modifiers::Mesh { pos: t.pos.clone(), tris: vec![], colors: vec![], edges: vec![] };
        let _ = mesh;
        let mut e = Edit { topo: t, ..Edit::open(&crate::modifiers::Mesh { pos: vec![], tris: vec![], colors: vec![], edges: vec![] }, std::env::temp_dir().join("aryadne-uvtools-test.json"), 1.0) };
        // Une arête au milieu de la grille : sa boucle traverse toute la grille (4 arêtes)
        let id = |i: u32, j: u32| j * 5 + i;
        let lp = e.edge_loop(key(id(2, 1), id(2, 2)));
        assert_eq!(lp.len(), 4, "{lp:?}");
        // Chemin d'un coin à l'autre : au moins 8 arêtes de grille
        let path = e.edge_path(key(id(0, 0), id(1, 0)), key(id(3, 4), id(4, 4)));
        assert!(path.len() >= 7, "{path:?}");
        // Marquer : les arêtes choisies deviennent des coutures
        e.tools.esel = lp.iter().copied().collect();
        e.mark_seams(true);
        assert_eq!(e.data.seams.len(), 4);
    }
}
