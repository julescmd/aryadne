//! Géométrie et simulation en nodes : le moteur.
//!
//! Trois idées tiennent tout le reste.
//!
//! 1. **Données en colonnes.** Une géométrie est une table : une colonne par attribut (position,
//!    vitesse, couleur…), et dans chaque colonne un tableau par composante (x, y, z). Un node qui
//!    ne touche que la position ne copie ni ne relit les autres colonnes : elles sont partagées.
//!    C'est la disposition qu'attend un solveur sur carte graphique (un buffer par attribut).
//!
//! 2. **Calcul à la demande, incrémental.** Rien ne se calcule tant que la sortie ne le demande
//!    pas. Chaque résultat est rangé sous une clé : le node, ses réglages, et l'empreinte de ce
//!    qui y entre. L'image de l'animation n'entre dans la clé que si le node en dépend. Quand un
//!    réglage change, seul ce qui est en aval est refait ; et si un node refait donne les mêmes
//!    colonnes qu'avant, elles gardent leur numéro de version, donc l'aval reste valide.
//!
//! 3. **Simulation = état qui passe d'une image à l'autre.** Une zone de simulation garde un état
//!    par image. Demander l'image 80 repart du dernier état connu avant elle. Les solveurs sont
//!    des nodes comme les autres, posés dans la zone ; ils reçoivent l'état en colonnes.

use crate::nodeflow::TimeRange;
use crate::nodes::{Ctx, Graph, Link, Node, Param, Program, Sock, V};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

type Fail = (u32, String);

static VERSION: AtomicU64 = AtomicU64::new(1);

fn next_version() -> u64 {
    VERSION.fetch_add(1, Ordering::Relaxed)
}

// ---------- données ----------

/// Une colonne d'attribut : un tableau par composante. `version` change à chaque écriture, ce qui
/// permet de savoir sans relire les données si deux colonnes sont la même.
#[derive(Clone, Debug)]
pub struct Column {
    pub data: Vec<Vec<f32>>,
    pub version: u64,
}

impl Column {
    pub fn new(data: Vec<Vec<f32>>) -> Column {
        Column { data, version: next_version() }
    }

    pub fn width(&self) -> usize {
        self.data.len()
    }

    pub fn len(&self) -> usize {
        self.data.first().map(|c| c.len()).unwrap_or(0)
    }

    /// Valeur d'un élément, étendue à quatre nombres comme partout dans les nodes.
    pub fn get(&self, i: usize) -> V {
        match self.data.len() {
            0 => [0.0; 4],
            1 => [self.data[0][i]; 4],
            2 => [self.data[0][i], self.data[1][i], 0.0, 1.0],
            3 => [self.data[0][i], self.data[1][i], self.data[2][i], 1.0],
            _ => [self.data[0][i], self.data[1][i], self.data[2][i], self.data[3][i]],
        }
    }

    fn same(&self, o: &Column) -> bool {
        self.version == o.version || self.data == o.data
    }

    fn bytes(&self) -> usize {
        self.data.iter().map(|c| c.len() * 4).sum()
    }
}

/// Quatrième indice d'une face à trois sommets.
pub const NO_VERT: u32 = u32::MAX;

/// Une géométrie : des points, leurs attributs, des arêtes qui les relient (fils, tissu), et des
/// faces qui en font une surface (ce qu'une scène 3D ombre et rend).
#[derive(Clone, Default, Debug)]
pub struct Geo {
    pub n: usize,
    pub cols: BTreeMap<String, Arc<Column>>,
    pub edges: Arc<Vec<[u32; 2]>>,
    /// Attributs des arêtes (longueur au repos, raideur…)
    pub ecols: BTreeMap<String, Arc<Column>>,
    edges_version: u64,
    /// Faces : des quads, ou des triangles (quatrième indice : `NO_VERT`). Elles ne servent pas
    /// aux solveurs ; elles suivent les points à travers les nodes et la simulation.
    pub faces: Arc<Vec<[u32; 4]>>,
    faces_version: u64,
}

impl Geo {
    pub fn points(p: [Vec<f32>; 3]) -> Geo {
        let n = p[0].len();
        let mut g = Geo { n, ..Default::default() };
        g.set("P", Column::new(p.into()));
        g
    }

    pub fn set(&mut self, name: &str, c: Column) {
        debug_assert_eq!(c.len(), self.n);
        self.cols.insert(name.to_string(), Arc::new(c));
    }

    pub fn col(&self, name: &str) -> Option<&Column> {
        self.cols.get(name).map(|c| c.as_ref())
    }

    pub fn set_edges(&mut self, e: Vec<[u32; 2]>) {
        self.edges = Arc::new(e);
        self.edges_version = next_version();
        self.ecols.clear();
    }

    pub fn set_faces(&mut self, f: Vec<[u32; 4]>) {
        self.faces = Arc::new(f);
        self.faces_version = next_version();
    }

    /// Les faces en triangles (un quad en donne deux), avec pour chacun ses vrais côtés (pas la
    /// diagonale du quad) : bit 0 v0-v1, bit 1 v1-v2, bit 2 v2-v0. Les faces qui pointent hors des
    /// points sont ignorées.
    pub fn triangles(&self) -> (Vec<[u32; 3]>, Vec<u8>) {
        let mut tris = Vec::with_capacity(self.faces.len() * 2);
        let mut sides = Vec::with_capacity(self.faces.len() * 2);
        let n = self.n as u32;
        for f in self.faces.iter() {
            if f[0] >= n || f[1] >= n || f[2] >= n {
                continue;
            }
            if f[3] == NO_VERT {
                tris.push([f[0], f[1], f[2]]);
                sides.push(0b111);
            } else if f[3] < n {
                tris.push([f[0], f[1], f[2]]);
                sides.push(0b011);
                tris.push([f[0], f[2], f[3]]);
                sides.push(0b110);
            }
        }
        (tris, sides)
    }

    /// Les trois composantes d'un attribut vectoriel, copiées (zéros s'il manque).
    pub fn vec3(&self, name: &str) -> [Vec<f32>; 3] {
        match self.col(name) {
            Some(c) if c.width() >= 3 => [c.data[0].clone(), c.data[1].clone(), c.data[2].clone()],
            _ => [vec![0.0; self.n], vec![0.0; self.n], vec![0.0; self.n]],
        }
    }

    pub fn pos(&self, i: usize) -> [f32; 3] {
        match self.col("P") {
            Some(c) if c.width() >= 3 => [c.data[0][i], c.data[1][i], c.data[2][i]],
            _ => [0.0; 3],
        }
    }

    /// Empreinte : change dès qu'une colonne, le nombre de points, les arêtes ou les faces changent.
    pub fn stamp(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.n.hash(&mut h);
        for (k, c) in &self.cols {
            k.hash(&mut h);
            c.version.hash(&mut h);
        }
        for (k, c) in &self.ecols {
            k.hash(&mut h);
            c.version.hash(&mut h);
        }
        self.edges_version.hash(&mut h);
        self.faces_version.hash(&mut h);
        h.finish()
    }

    pub fn bytes(&self) -> usize {
        self.cols.values().chain(self.ecols.values()).map(|c| c.bytes()).sum::<usize>() + self.edges.len() * 8 + self.faces.len() * 16
    }

    /// Coupure anticipée : chaque colonne identique à celle de `old` reprend l'ancienne (et son
    /// numéro de version). Ce qui lisait l'ancienne n'a alors rien à refaire.
    pub fn adopt(&mut self, old: &Geo) {
        if self.n != old.n {
            return;
        }
        for (k, c) in self.cols.iter_mut() {
            if let Some(o) = old.cols.get(k) {
                if c.version != o.version && c.same(o) {
                    *c = o.clone();
                }
            }
        }
        for (k, c) in self.ecols.iter_mut() {
            if let Some(o) = old.ecols.get(k) {
                if c.version != o.version && c.same(o) {
                    *c = o.clone();
                }
            }
        }
        if self.edges_version != old.edges_version && (Arc::ptr_eq(&self.edges, &old.edges) || *self.edges == *old.edges) {
            self.edges = old.edges.clone();
            self.edges_version = old.edges_version;
        }
        if self.faces_version != old.faces_version && (Arc::ptr_eq(&self.faces, &old.faces) || *self.faces == *old.faces) {
            self.faces = old.faces.clone();
            self.faces_version = old.faces_version;
        }
    }

    /// Boîte englobante (min, max), ou rien sans point.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let c = self.col("P").filter(|c| c.width() >= 3 && self.n > 0)?;
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for k in 0..3 {
            for &x in &c.data[k] {
                if x.is_finite() {
                    lo[k] = lo[k].min(x);
                    hi[k] = hi[k].max(x);
                }
            }
        }
        (lo[0] <= hi[0]).then_some((lo, hi))
    }

    /// Cache disque d'une image : en-tête, puis chaque colonne en nombres bruts. Les colonnes se
    /// relisent telles quelles, sans conversion. Les faces viennent à la fin, dans un bloc « FACE »
    /// facultatif : un cache écrit avant elles se relit toujours (sans faces), et une ancienne
    /// version de l'application relit un cache récent en ignorant ce bloc.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(self.bytes() + 64);
        b.extend_from_slice(b"ARYGEO1\0");
        let put = |b: &mut Vec<u8>, x: u32| b.extend_from_slice(&x.to_le_bytes());
        put(&mut b, self.n as u32);
        put(&mut b, self.edges.len() as u32);
        for (cols, count) in [(&self.cols, self.n), (&self.ecols, self.edges.len())] {
            put(&mut b, cols.len() as u32);
            for (name, c) in cols {
                put(&mut b, name.len() as u32);
                b.extend_from_slice(name.as_bytes());
                put(&mut b, c.width() as u32);
                for comp in &c.data {
                    debug_assert_eq!(comp.len(), count);
                    for x in comp {
                        b.extend_from_slice(&x.to_le_bytes());
                    }
                }
            }
        }
        for e in self.edges.iter() {
            put(&mut b, e[0]);
            put(&mut b, e[1]);
        }
        if !self.faces.is_empty() {
            b.extend_from_slice(b"FACE");
            put(&mut b, self.faces.len() as u32);
            for f in self.faces.iter() {
                for i in f {
                    put(&mut b, *i);
                }
            }
        }
        b
    }

    pub fn from_bytes(b: &[u8]) -> Result<Geo, String> {
        let bad = || "cache de géométrie illisible".to_string();
        if b.len() < 16 || &b[..8] != b"ARYGEO1\0" {
            return Err(bad());
        }
        let mut at = 8;
        let u32_at = |at: &mut usize| -> Result<u32, String> {
            let s = b.get(*at..*at + 4).ok_or_else(bad)?;
            *at += 4;
            Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        };
        let n = u32_at(&mut at)? as usize;
        let ne = u32_at(&mut at)? as usize;
        let mut g = Geo { n, ..Default::default() };
        for (which, count) in [(0, n), (1, ne)] {
            let ncols = u32_at(&mut at)? as usize;
            for _ in 0..ncols {
                let len = u32_at(&mut at)? as usize;
                let name = std::str::from_utf8(b.get(at..at + len).ok_or_else(bad)?).map_err(|_| bad())?.to_string();
                at += len;
                let width = u32_at(&mut at)? as usize;
                if width > 16 {
                    return Err(bad());
                }
                let mut data = Vec::with_capacity(width);
                for _ in 0..width {
                    let raw = b.get(at..at + count * 4).ok_or_else(bad)?;
                    at += count * 4;
                    data.push(raw.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect());
                }
                let col = Arc::new(Column::new(data));
                if which == 0 {
                    g.cols.insert(name, col);
                } else {
                    g.ecols.insert(name, col);
                }
            }
        }
        let mut edges = Vec::with_capacity(ne);
        for _ in 0..ne {
            edges.push([u32_at(&mut at)?, u32_at(&mut at)?]);
        }
        g.edges = Arc::new(edges);
        g.edges_version = next_version();
        if b.get(at..at + 4) == Some(b"FACE") {
            at += 4;
            let nf = u32_at(&mut at)? as usize;
            if b.len() < at + nf.saturating_mul(16) {
                return Err(bad());
            }
            let mut faces = Vec::with_capacity(nf);
            for _ in 0..nf {
                faces.push([u32_at(&mut at)?, u32_at(&mut at)?, u32_at(&mut at)?, u32_at(&mut at)?]);
            }
            g.set_faces(faces);
        }
        Ok(g)
    }
}

// ---------- solveurs ----------

/// Réglages communs passés à un solveur.
#[derive(Clone, Copy, Debug)]
pub struct SolverParams {
    pub iterations: usize,
    /// Souplesse des contraintes (0 : rigide)
    pub compliance: f32,
    pub damping: f32,
    /// Obstacles simples, traités comme des contraintes pendant la résolution : le sol, une boule
    pub floor: Option<f32>,
    pub ball: Option<([f32; 3], f32)>,
    /// Frottement au contact des obstacles. XPBD : part du glissement retenue (0 : glisse,
    /// 1 : colle). Tissu sur carte graphique : coefficient de Coulomb.
    pub grip: f32,
    /// Boîte : centre et demi-tailles
    pub cube: Option<([f32; 3], [f32; 3])>,
    /// Réglages propres au tissu sur carte graphique : raideur des arêtes (N/m), raideur de
    /// flexion (N·m), nombre de sous-pas par image
    pub stiffness: f32,
    pub bending: f32,
    pub substeps: usize,
    /// Raideur des diagonales en part de celle des arêtes, amortissement interne (0 à 1)
    pub shear: f32,
    pub calm: f32,
    /// Auto-collision : distance minimale entre deux sommets du tissu (0 : choisie d'après la maille), ou rien
    pub thickness: Option<f32>,
}

/// Un solveur fait avancer un état d'un pas de temps. Il reçoit l'état en colonnes et le modifie
/// sur place : c'est le même contrat qu'aura un solveur sur carte graphique (XPBD puis VBD pour le
/// tissu, fumée, fluides), dont les colonnes seront des buffers.
pub trait Solver: Send + Sync {
    /// Nom montré dans le node « Solveur »
    fn name(&self) -> &'static str;
    fn step(&self, state: &mut Geo, dt: f32, p: &SolverParams);
}

/// Les solveurs disponibles. Pour en ajouter un : l'écrire, et l'inscrire ici et dans la liste du node.
pub fn solvers() -> &'static [&'static dyn Solver] {
    &[&XpbdCloth, &VbdCloth]
}

pub const SOLVER_NAMES: &[&str] = &["tissu et fils, XPBD (processeur)", "tissu (carte graphique)"];

/// Ce solveur lit-il cette entrée du node « Solveur » ? Les autres sont montrées en retrait.
pub fn solver_uses(solver: &str, input: &str) -> bool {
    let gpu = solver == SOLVER_NAMES[1];
    match input {
        "souplesse" => !gpu,
        "raideur" | "flexion" | "sous-pas" | "cisaillement" | "amortissement des plis" | "épaisseur" => gpu,
        _ => true,
    }
}

/// Contraintes de distance le long des arêtes, façon XPBD, sur le processeur. Petit et lisible :
/// il sert de référence et d'exemple pour les solveurs à venir.
/// Attributs lus : `P`, `v`, `pin` (point fixe si > 0,5). Attribut d'arête : `rest`, la longueur
/// au repos, mesurée au premier pas.
struct XpbdCloth;

impl Solver for XpbdCloth {
    fn name(&self) -> &'static str {
        SOLVER_NAMES[0]
    }

    fn step(&self, g: &mut Geo, dt: f32, p: &SolverParams) {
        let n = g.n;
        if n == 0 || dt <= 0.0 {
            return;
        }
        let p0 = g.vec3("P");
        let v = g.vec3("v");
        let w: Vec<f32> = match g.col("pin") {
            Some(c) => c.data[0].iter().map(|x| if *x > 0.5 { 0.0 } else { 1.0 }).collect(),
            None => vec![1.0; n],
        };
        let edges = g.edges.clone();
        if !g.ecols.contains_key("rest") {
            let rest: Vec<f32> = edges.iter().map(|e| {
                let (a, b) = (e[0] as usize, e[1] as usize);
                ((p0[0][a] - p0[0][b]).powi(2) + (p0[1][a] - p0[1][b]).powi(2) + (p0[2][a] - p0[2][b]).powi(2)).sqrt()
            }).collect();
            g.ecols.insert("rest".into(), Arc::new(Column::new(vec![rest])));
        }
        let rest = g.ecols["rest"].data[0].clone();
        // Petits pas : le pas de temps est coupé en sous-pas, chacun avec peu de passes sur les
        // contraintes. À nombre de passes égal, le tissu s'étire bien moins qu'avec un seul grand pas.
        let passes = p.iterations.max(1);
        let sub = (passes / 2).clamp(1, 16);
        let per = passes.div_ceil(sub);
        let h = dt / sub as f32;
        let alpha = p.compliance.max(0.0) / (h * h);
        let keep = (1.0 - p.damping).clamp(0.0, 1.0).powf(1.0 / sub as f32);
        let mut x = [p0[0].clone(), p0[1].clone(), p0[2].clone()];
        let mut nv = v;
        let mut prev = x.clone();
        let mut lambda = vec![0.0f32; edges.len()];
        for _ in 0..sub {
            // Prédiction, puis projection des contraintes, puis vitesse tirée du déplacement
            for k in 0..3 {
                prev[k].copy_from_slice(&x[k]);
                for i in 0..n {
                    if w[i] > 0.0 {
                        x[k][i] += nv[k][i] * h;
                    }
                }
            }
            lambda.iter_mut().for_each(|l| *l = 0.0);
            for _ in 0..per {
                for (ei, e) in edges.iter().enumerate() {
                    let (a, b) = (e[0] as usize, e[1] as usize);
                    if a >= n || b >= n {
                        continue;
                    }
                    let wsum = w[a] + w[b];
                    if wsum == 0.0 {
                        continue;
                    }
                    let d = [x[0][b] - x[0][a], x[1][b] - x[1][a], x[2][b] - x[2][a]];
                    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                    if len < 1e-9 {
                        continue;
                    }
                    let c = len - rest[ei];
                    let dl = (-c - alpha * lambda[ei]) / (wsum + alpha);
                    lambda[ei] += dl;
                    for k in 0..3 {
                        let nk = d[k] / len;
                        x[k][a] -= w[a] * dl * nk;
                        x[k][b] += w[b] * dl * nk;
                    }
                }
                // Les obstacles sont des contraintes comme les autres : résolus dans la même boucle,
                // ils n'étirent pas le tissu après coup
                let slide = 1.0 - p.grip.clamp(0.0, 1.0);
                for i in 0..n {
                    if w[i] == 0.0 {
                        continue;
                    }
                    if let Some(y) = p.floor {
                        if x[1][i] < y {
                            x[1][i] = y;
                            // Au contact, le glissement depuis le début du sous-pas est freiné
                            x[0][i] = prev[0][i] + (x[0][i] - prev[0][i]) * slide;
                            x[2][i] = prev[2][i] + (x[2][i] - prev[2][i]) * slide;
                        }
                    }
                    if let Some((c, r)) = p.ball {
                        let d = [x[0][i] - c[0], x[1][i] - c[1], x[2][i] - c[2]];
                        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                        if len < r && len > 1e-9 {
                            let nrm = [d[0] / len, d[1] / len, d[2] / len];
                            let mv = [x[0][i] - prev[0][i], x[1][i] - prev[1][i], x[2][i] - prev[2][i]];
                            let along = mv[0] * nrm[0] + mv[1] * nrm[1] + mv[2] * nrm[2];
                            for k in 0..3 {
                                // Sorti de la boule, puis freiné le long de sa surface
                                let tangent = mv[k] - along * nrm[k];
                                x[k][i] = c[k] + nrm[k] * r - tangent * (1.0 - slide);
                            }
                            // Le frein ne doit pas faire rentrer le point : on le repose sur la surface
                            let d = [x[0][i] - c[0], x[1][i] - c[1], x[2][i] - c[2]];
                            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                            if len < r && len > 1e-9 {
                                for k in 0..3 {
                                    x[k][i] = c[k] + d[k] / len * r;
                                }
                            }
                        }
                    }
                    if let Some((c, half)) = p.cube {
                        let q = [x[0][i] - c[0], x[1][i] - c[1], x[2][i] - c[2]];
                        let pen = [half[0] - q[0].abs(), half[1] - q[1].abs(), half[2] - q[2].abs()];
                        if pen.iter().all(|d| *d > 0.0) {
                            // Sorti par la face la plus proche, freiné le long de cette face
                            let axis = (0..3).min_by(|a, b| pen[*a].total_cmp(&pen[*b])).unwrap_or(1);
                            for k in 0..3 {
                                x[k][i] = prev[k][i] + (x[k][i] - prev[k][i]) * slide;
                            }
                            x[axis][i] = c[axis] + half[axis].copysign(q[axis]);
                        }
                    }
                }
            }
            for k in 0..3 {
                for i in 0..n {
                    nv[k][i] = (x[k][i] - prev[k][i]) / h * keep;
                }
            }
        }
        g.set("P", Column::new(x.into()));
        g.set("v", Column::new(nv.into()));
    }
}

/// Tissu en « Vertex Block Descent » sur la carte graphique (voir `gpucloth`). Sans carte, le même
/// calcul se fait sur le processeur. Lit `P`, `v`, `pin`, et `mass` s'il existe ; garde la forme au
/// repos dans `repos` (points) et `rest` (arêtes), mesurées au premier pas.
struct VbdCloth;

/// Un tissu déjà préparé : sa topologie rangée et coloriée, ses tampons sur la carte.
struct ClothSession {
    key: u64,
    cloth: crate::gpucloth::Cloth,
    /// Versions des colonnes `P` et `v` rendues au dernier pas : si elles reviennent telles
    /// quelles, la carte a déjà cet état et on ne renvoie rien
    out: (u64, u64),
    used: u64,
    /// Un pas envoyé à la carte, dont le résultat n'est pas encore revenu
    waiting: Option<ClothWait>,
}

/// Un pas de tissu en cours sur la carte : pour quelles entrées, et de quoi finir quand il revient.
struct ClothWait {
    /// Versions de `P` et de `v` reçues, pas de temps, nombre de points
    inputs: (u64, u64, u32, usize),
    par: crate::gpucloth::Params,
    pending: crate::gpucloth::Pending,
    /// La forme au repos rangée dans l'état à ce pas (le même état doit ressortir quand le pas revient)
    repos: Option<Arc<Column>>,
    rest: Option<Arc<Column>>,
    since: std::time::Instant,
}

thread_local! {
    /// Le moteur accepte-t-il qu'un solveur rende la main avant d'avoir fini (calcul en tranches) ?
    static SIM_DEFER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Un solveur a rendu la main : son pas tourne encore sur la carte, l'image n'est pas prête
    static SIM_WAIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl ClothSession {
    /// Range le résultat du pas dans l'état, et note ce qu'il a coûté.
    fn finish(&mut self, g: &mut Geo, par: &crate::gpucloth::Params, since: std::time::Instant) {
        let (x, v) = self.cloth.read();
        let (x, v) = (Column::new(x.into()), Column::new(v.into()));
        self.out = (x.version, v.version);
        g.set("P", x);
        g.set("v", v);
        let (substeps, per) = par.split();
        let on_gpu = self.cloth.on_gpu();
        let status = ClothStatus { on_gpu, card: if on_gpu { crate::gpusim::gpu().map(|c| c.name.clone()).unwrap_or_default() } else { String::new() }, points: g.n, colors: self.cloth.topo.colors(), substeps, per, ms: since.elapsed().as_secs_f32() * 1000.0 };
        *CLOTH_STATUS.lock().unwrap_or_else(|e| e.into_inner()) = Some(status);
    }
}

static CLOTHS: std::sync::Mutex<Vec<ClothSession>> = std::sync::Mutex::new(Vec::new());
static CLOTH_STATUS: std::sync::Mutex<Option<ClothStatus>> = std::sync::Mutex::new(None);

/// Ce qu'a fait le dernier pas de tissu, pour le montrer dans l'aperçu.
#[derive(Clone, Debug, PartialEq)]
pub struct ClothStatus {
    /// Calculé sur la carte (sinon : sur le processeur)
    pub on_gpu: bool,
    /// Nom de la carte, si elle a servi
    pub card: String,
    pub points: usize,
    pub colors: usize,
    pub substeps: usize,
    /// Itérations par sous-pas
    pub per: usize,
    /// Temps du pas, relecture comprise
    pub ms: f32,
}

pub fn cloth_status() -> Option<ClothStatus> {
    CLOTH_STATUS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

impl Solver for VbdCloth {
    fn name(&self) -> &'static str {
        SOLVER_NAMES[1]
    }

    fn step(&self, g: &mut Geo, dt: f32, p: &SolverParams) {
        let n = g.n;
        if n == 0 || dt <= 0.0 {
            return;
        }
        let t0 = std::time::Instant::now();
        let par = crate::gpucloth::Params { stiffness: p.stiffness, shear: p.shear, bending: p.bending, damping: p.damping, calm: p.calm, iterations: p.iterations, substeps: p.substeps, rho: crate::gpucloth::RHO, floor: p.floor, ball: p.ball, cube: p.cube, friction: p.grip, thickness: p.thickness, ..Default::default() }.with_levels();
        let version = |g: &Geo, name: &str| g.col(name).map(|c| c.version).unwrap_or(0);
        let inputs = (version(g, "P"), version(g, "v"), dt.to_bits(), n);
        // Le moteur calcule par tranches : plutôt que d'attendre la carte, on lui rend la main, et
        // il redemandera ce même pas à la prochaine image de l'interface
        let defer = SIM_DEFER.with(|d| d.get());
        let mut all = CLOTHS.lock().unwrap_or_else(|e| e.into_inner());
        // Ce pas est déjà parti sur la carte : est-il revenu ?
        if let Some(s) = all.iter_mut().find(|s| s.waiting.as_ref().is_some_and(|w| w.inputs == inputs && w.par == par)) {
            let Some(w) = s.waiting.take() else { return };
            if defer && !w.pending.ready() {
                s.waiting = Some(w);
                SIM_WAIT.with(|x| x.set(true));
                return;
            }
            if let Some(c) = w.repos {
                g.cols.insert("repos".into(), c);
            }
            if let Some(c) = w.rest {
                g.ecols.insert("rest".into(), c);
            }
            s.cloth.accept(Some(w.pending));
            s.finish(g, &par, w.since);
            return;
        }
        // Forme au repos : celle du premier pas, gardée dans l'état pour qu'une reprise au milieu
        // de la simulation retrouve le même tissu
        if !g.cols.get("repos").is_some_and(|c| c.width() >= 3) {
            match g.cols.get("P").filter(|c| c.width() >= 3).cloned() {
                Some(c) => {
                    g.cols.insert("repos".into(), c);
                }
                None => return,
            }
        }
        let edges = g.edges.clone();
        if !g.ecols.contains_key("rest") {
            let rest = crate::gpucloth::rest_lengths(&edges, &g.vec3("repos"));
            g.ecols.insert("rest".into(), Arc::new(Column::new(vec![rest])));
        }
        let key = {
            let mut h = hasher();
            (n, g.edges_version, g.ecols["rest"].version, version(g, "repos"), version(g, "pin"), version(g, "mass")).hash(&mut h);
            h.finish()
        };
        let clock = all.iter().map(|s| s.used).max().unwrap_or(0) + 1;
        let at = match all.iter().position(|s| s.key == key) {
            Some(k) => k,
            None => {
                let pinned: Vec<bool> = match g.col("pin") {
                    Some(c) => c.data[0].iter().map(|x| *x > 0.5).collect(),
                    None => vec![false; n],
                };
                let mass = g.col("mass").filter(|c| c.width() == 1).map(|c| c.data[0].as_slice());
                let topo = crate::gpucloth::Topo::build(n, &edges, &g.ecols["rest"].data[0], &g.vec3("repos"), &pinned, mass);
                // Quelques tissus restent prêts ; le plus ancien cède sa place (et sa mémoire sur la carte)
                if all.len() >= 4 {
                    if let Some(old) = (0..all.len()).min_by_key(|k| all[*k].used) {
                        all.swap_remove(old);
                    }
                }
                all.push(ClothSession { key, cloth: crate::gpucloth::Cloth::new(topo, &g.vec3("P"), true), out: (0, 0), used: 0, waiting: None });
                all.len() - 1
            }
        };
        let s = &mut all[at];
        s.used = clock;
        // Un pas parti pour d'autres entrées (on a changé d'image ou de réglages entre-temps) :
        // son résultat ne sert plus, et la carte n'est plus dans l'état qu'on croyait
        if let Some(w) = s.waiting.take() {
            s.cloth.accept(Some(w.pending));
            s.out = (0, 0);
        }
        // Positions venues d'ailleurs que du dernier pas : un nouvel état, vitesses imposées. Sinon
        // les vitesses n'ont changé que par les forces de l'image : le solveur les répartit sur ses sous-pas
        let fresh = s.out.0 == 0 || inputs.0 != s.out.0;
        if fresh {
            s.cloth.set_positions(&g.vec3("P"));
        }
        if s.out.1 == 0 || inputs.1 != s.out.1 {
            if fresh {
                s.cloth.set_velocities(&g.vec3("v"));
            } else {
                s.cloth.push_velocities(&g.vec3("v"));
            }
        }
        s.cloth.step(dt, &par);
        if defer {
            if let Some(pending) = s.cloth.request() {
                if !pending.ready() {
                    s.waiting = Some(ClothWait { inputs, par, pending, repos: g.cols.get("repos").cloned(), rest: g.ecols.get("rest").cloned(), since: t0 });
                    SIM_WAIT.with(|x| x.set(true));
                    return;
                }
                s.cloth.accept(Some(pending));
            }
        }
        s.finish(g, &par, t0);
    }
}

/// VBD contre le XPBD de ce fichier, à nombre égal de passes par image (voir `gpucloth::compare`).
pub fn cloth_comparison(n: usize, budgets: &[usize]) -> crate::gpucloth::Comparison {
    crate::gpucloth::compare(n, budgets, &|pos, vel, edges, rest, pinned, dt, par, compliance| {
        let mut g = Geo::points(pos.clone());
        g.set_edges(edges.to_vec());
        g.ecols.insert("rest".into(), Arc::new(Column::new(vec![rest.to_vec()])));
        g.set("v", Column::new(vel.clone().into()));
        g.set("pin", Column::new(vec![pinned.iter().map(|p| *p as u8 as f32).collect()]));
        let p = SolverParams { iterations: par.iterations, compliance, damping: par.damping, floor: par.floor, ball: par.ball, grip: 0.6, cube: par.cube, stiffness: 0.0, bending: 0.0, substeps: 1, shear: 1.0, calm: 0.0, thickness: None };
        XpbdCloth.step(&mut g, dt, &p);
        *pos = g.vec3("P");
        *vel = g.vec3("v");
    })
}

// ---------- entrées de la scène ----------

/// Ce qu'une scène 3D montre au graphe qu'elle calcule : la boîte de ses objets (centre, tailles),
/// dans le repère de la géométrie, à chaque image de la simulation depuis `start`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneInputs {
    pub start: i32,
    pub objects: BTreeMap<String, Vec<([f32; 3], [f32; 3])>>,
}

impl SceneInputs {
    /// La boîte d'un objet à une image de la simulation (la première ou la dernière connue au-delà).
    pub fn at(&self, name: &str, frame: i32) -> Option<([f32; 3], [f32; 3])> {
        let track = self.objects.get(name).filter(|t| !t.is_empty())?;
        Some(track[((frame - self.start).max(0) as usize).min(track.len() - 1)])
    }

    /// Empreinte de toute la trajectoire d'un objet : elle entre dans la clé d'une zone de simulation.
    fn track_hash(&self, name: &str, h: &mut impl Hasher) {
        self.start.hash(h);
        for (c, s) in self.objects.get(name).map(|t| t.as_slice()).unwrap_or(&[]) {
            for x in c.iter().chain(s) {
                x.to_bits().hash(h);
            }
        }
    }
}

thread_local! {
    /// Les entrées de la scène et l'image du calcul en cours sur ce fil
    static SCENE_NOW: std::cell::RefCell<Option<(Arc<SceneInputs>, i32)>> = const { std::cell::RefCell::new(None) };
}

/// La boîte (centre, tailles) d'un objet de la scène à l'image que le moteur calcule en ce moment.
/// Rien hors d'une scène, ou si l'objet n'y est pas.
pub fn scene_object(name: &str) -> Option<([f32; 3], [f32; 3])> {
    SCENE_NOW.with(|s| s.borrow().as_ref().and_then(|(inputs, frame)| inputs.at(name, *frame)))
}

fn scene_now(inputs: &Option<Arc<SceneInputs>>, frame: i32) {
    SCENE_NOW.with(|s| *s.borrow_mut() = inputs.clone().map(|i| (i, frame)));
}

// ---------- moteur ----------

/// Ce qu'a coûté le dernier calcul : de quoi montrer à l'artiste que seul le nécessaire est refait.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Stats {
    /// Nodes réellement exécutés
    pub computed: usize,
    /// Résultats repris du cache
    pub reused: usize,
    /// Nodes refaits dont le résultat n'a pas changé (l'aval est épargné)
    pub cutoffs: usize,
    /// Pas de simulation calculés
    pub steps: usize,
}

struct Memo {
    out: Vec<Geo>,
    bytes: usize,
    /// Temps de calcul, en millisecondes
    cost: f32,
    /// Priorité : les moins chers à refaire par octet partent les premiers
    h: f64,
}

/// Le cache et l'état du calcul, gardés d'une image à l'autre et d'une modification à l'autre.
pub struct Engine {
    memo: HashMap<u64, Memo>,
    /// Dernier résultat de chaque node, pour la coupure anticipée
    last: HashMap<(u32, usize), Geo>,
    bytes: usize,
    /// Mémoire accordée au cache
    pub budget: usize,
    clock: f64,
    pub stats: Stats,
    /// Heure limite du calcul en cours : passé ce moment, une simulation rend l'image qu'elle a atteinte
    pub deadline: Option<std::time::Instant>,
    /// Vrai si le dernier calcul s'est arrêté avant l'image demandée
    pub incomplete: bool,
    /// Images de simulation en cache, par zone : (clé de la zone, images)
    sim_frames: HashMap<u32, (u64, std::collections::BTreeSet<i32>)>,
    /// Dossier d'un cache figé sur le disque, par zone (node de fin)
    pub baked: HashMap<u32, std::path::PathBuf>,
    /// Entrées de la scène 3D qui calcule ce graphe (node « Objet de la scène ») ; rien hors d'une scène
    pub scene: Option<Arc<SceneInputs>>,
}

impl Default for Engine {
    fn default() -> Self {
        Engine { memo: HashMap::new(), last: HashMap::new(), bytes: 0, budget: 512 << 20, clock: 0.0, stats: Stats::default(), deadline: None, incomplete: false, sim_frames: HashMap::new(), baked: HashMap::new(), scene: None }
    }
}

fn hasher() -> std::collections::hash_map::DefaultHasher {
    std::collections::hash_map::DefaultHasher::new()
}

fn hash_v(h: &mut impl Hasher, v: &V) {
    for x in v {
        x.to_bits().hash(h);
    }
}

fn hash_params(h: &mut impl Hasher, n: &Node) {
    for (k, v) in &n.params {
        k.hash(h);
        match v {
            Param::F(x) => x.to_bits().hash(h),
            Param::S(s) => s.hash(h),
        }
    }
}

fn pstr<'a>(n: &'a Node, k: &str) -> &'a str {
    match n.params.get(k) {
        Some(Param::S(s)) => s,
        _ => "",
    }
}

/// Valeurs d'un champ sur une géométrie : la même partout, ou une par point.
pub enum Field {
    Const(V),
    Per(Vec<V>),
}

impl Field {
    pub fn at(&self, i: usize) -> V {
        match self {
            Field::Const(v) => *v,
            Field::Per(v) => v[i],
        }
    }
}

impl Engine {
    pub fn clear(&mut self) {
        let (budget, scene) = (self.budget, self.scene.take());
        *self = Engine { budget, scene, ..Default::default() };
    }

    /// Mémoire tenue par le cache, en octets (les colonnes partagées sont comptées à chaque usage).
    pub fn cached_bytes(&self) -> usize {
        self.bytes
    }

    /// Images de simulation encore en cache pour la zone qui finit au node `end`.
    pub fn cached_frames(&self, end: u32) -> Vec<i32> {
        match self.sim_frames.get(&end) {
            Some((base, frames)) => frames.iter().copied().filter(|f| self.memo.contains_key(&sim_key(*base, *f))).collect(),
            None => vec![],
        }
    }

    fn touch(&mut self, key: u64) -> Option<Vec<Geo>> {
        let clock = self.clock;
        let m = self.memo.get_mut(&key)?;
        m.h = clock + m.cost as f64 / m.bytes.max(1) as f64;
        Some(m.out.clone())
    }

    fn put(&mut self, key: u64, out: Vec<Geo>, cost: f32) {
        let bytes = out.iter().map(|g| g.bytes()).sum::<usize>().max(64);
        if let Some(old) = self.memo.insert(key, Memo { out, bytes, cost, h: self.clock + cost as f64 / bytes as f64 }) {
            self.bytes -= old.bytes;
        }
        self.bytes += bytes;
        // Trop plein : le moins cher à refaire par octet s'en va (règle GreedyDual-Size)
        while self.bytes > self.budget && self.memo.len() > 1 {
            let Some((&k, h)) = self.memo.iter().filter(|(k2, _)| **k2 != key).map(|(k2, m)| (k2, m.h)).min_by(|a, b| a.1.total_cmp(&b.1)) else { break };
            self.clock = h;
            if let Some(m) = self.memo.remove(&k) {
                self.bytes -= m.bytes;
            }
        }
    }

    /// Calcule la sortie du graphe à l'image `frame`. `g` : le document tel qu'enregistré.
    pub fn eval(&mut self, g: &Graph, frame: i32, root: &str) -> Result<Geo, Fail> {
        let low = crate::nodeflow::lower(g, false)?;
        let out = low.graph.nodes.iter().find(|n| n.op == "geo_out").map(|n| n.id).ok_or((u32::MAX, "il manque le node de sortie".to_string()))?;
        self.eval_lowered(&low.graph, out, 0, true, frame, root).map_err(|(id, e)| (low.origin(id), e))
    }

    /// Calcule ce qui sort d'un node précis (aperçu, tableau des attributs).
    pub fn eval_node(&mut self, g: &Graph, node: u32, out: usize, frame: i32, root: &str) -> Result<Geo, Fail> {
        let low = crate::nodeflow::lower(g, false)?;
        if !low.graph.nodes.iter().any(|n| n.id == node) {
            return Err((node, "ce node n'a pas de géométrie à montrer".into()));
        }
        self.eval_lowered(&low.graph, node, out, false, frame, root).map_err(|(id, e)| (low.origin(id), e))
    }

    fn eval_lowered(&mut self, g: &Graph, node: u32, out: usize, input: bool, frame: i32, root: &str) -> Result<Geo, Fail> {
        self.stats = Stats::default();
        self.incomplete = false;
        let time = g.time.unwrap_or_default();
        // Le pas de temps d'une zone est le même à chaque image : on l'écrit en dur dans le graphe,
        // les champs le lisent alors comme une constante
        let mut g2 = g.clone();
        let dt = 1.0 / time.fps.max(1e-3);
        for id in g.nodes.iter().filter(|n| n.op == "sim_in").map(|n| n.id).collect::<Vec<u32>>() {
            let users: Vec<(u32, usize)> = g2.links.iter().filter(|l| l.from == id && l.out == 0).map(|l| (l.to, l.inp)).collect();
            g2.links.retain(|l| !(l.from == id && l.out == 0));
            for (to, inp) in users {
                if let Some(n) = g2.nodes.iter_mut().find(|n| n.id == to) {
                    while n.values.len() <= inp {
                        n.values.push([0.0; 4]);
                    }
                    n.values[inp] = [dt; 4];
                }
            }
        }
        let g = &g2;
        let mut into: HashMap<u32, Vec<&Link>> = HashMap::new();
        for l in &g.links {
            into.entry(l.to).or_default().push(l);
        }
        let nodes: HashMap<u32, &Node> = g.nodes.iter().map(|n| (n.id, n)).collect();
        // Le node « Objet de la scène » lit la scène à l'image en cours (rien après le calcul)
        struct Leave;
        impl Drop for Leave {
            fn drop(&mut self) {
                scene_now(&None, 0);
            }
        }
        let _leave = Leave;
        scene_now(&self.scene, frame);
        let mut run = Run { g, into, nodes, frame, time, root, env: None, body: HashSet::new(), e: self, src_hash: HashMap::new(), depth: 0 };
        if input {
            return run.geo_in(node, 0);
        }
        // Un node du contenu d'une zone se montre tel qu'il agit à cette image : avec, en entrée de
        // la zone, l'état de l'image d'avant
        for end in g.nodes.iter().filter(|n| n.op == "sim_out").map(|n| n.id).collect::<Vec<u32>>() {
            let Some(begin) = crate::nodeflow::zone_pair(g, end) else { continue };
            let body = crate::nodeflow::zone_body(g, begin, end);
            if body.contains(&node) && frame > time.start {
                run.set_frame(frame - 1);
                let prev = run.sim_state(end)?;
                run.set_frame(frame);
                run.env = Some(Env { begin, state: prev });
                run.body = body.into_iter().collect();
                break;
            }
        }
        run.geo(node, out)
    }

    /// Fige sur le disque les images de la zone qui finit au node `end`, de la première à la dernière.
    /// Chaque image est un fichier ; relu ensuite sans rien recalculer, même si l'amont change.
    pub fn bake(&mut self, g: &Graph, end: u32, dir: &std::path::Path, root: &str) -> Result<usize, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        self.baked.remove(&end);
        let time = g.time.unwrap_or_default();
        let saved = self.deadline.take();
        let mut count = 0;
        for f in time.start..=time.end {
            let ports = g.nodes.iter().find(|n| n.id == end).map(|n| n.ins.len()).unwrap_or(0);
            for k in 0..ports {
                let geo = self.eval_node(g, end, k, f, root).map_err(|e| e.1)?;
                std::fs::write(bake_file(dir, f, k), geo.to_bytes()).map_err(|e| e.to_string())?;
            }
            count += 1;
        }
        self.deadline = saved;
        self.baked.insert(end, dir.to_path_buf());
        Ok(count)
    }
}

pub fn bake_file(dir: &std::path::Path, frame: i32, port: usize) -> std::path::PathBuf {
    dir.join(format!("{frame:05}_{port}.arygeocache"))
}

fn sim_key(base: u64, frame: i32) -> u64 {
    let mut h = hasher();
    base.hash(&mut h);
    frame.hash(&mut h);
    h.finish()
}

struct Env {
    begin: u32,
    state: Vec<Geo>,
}

struct Run<'a> {
    g: &'a Graph,
    into: HashMap<u32, Vec<&'a Link>>,
    nodes: HashMap<u32, &'a Node>,
    frame: i32,
    time: TimeRange,
    root: &'a str,
    /// Zone de simulation en cours d'exécution : son début rend l'état de l'image d'avant
    env: Option<Env>,
    /// Nodes du contenu de cette zone : leurs résultats changent à chaque image, on ne les garde pas
    body: HashSet<u32>,
    e: &'a mut Engine,
    /// Empreintes des champs déjà calculées pour cette image
    src_hash: HashMap<(u32, usize), u64>,
    depth: usize,
}

impl Run<'_> {
    fn link(&self, node: u32, inp: usize) -> Option<&Link> {
        self.into.get(&node).and_then(|v| v.iter().find(|l| l.inp == inp)).copied()
    }

    /// Géométrie qui arrive sur une entrée (vide si rien n'y est branché).
    fn geo_in(&mut self, node: u32, inp: usize) -> Result<Geo, Fail> {
        match self.link(node, inp).map(|l| (l.from, l.out)) {
            Some((from, out)) => self.geo(from, out),
            None => Ok(Geo::default()),
        }
    }

    /// Empreinte de ce qui alimente une entrée de champ : sa valeur, ou toute la chaîne de nodes
    /// en amont (types, réglages, valeurs). L'image y entre seulement si un node « Temps » s'y trouve.
    fn field_hash(&mut self, node: u32, inp: usize) -> u64 {
        match self.link(node, inp).map(|l| (l.from, l.out)) {
            Some((from, out)) => self.source_hash(from, out),
            None => {
                let mut h = hasher();
                if let Some(n) = self.nodes.get(&node) {
                    let v = n.values.get(inp).copied().or_else(|| n.def().and_then(|d| d.inputs.get(inp)).map(|i| i.2)).unwrap_or([0.0; 4]);
                    hash_v(&mut h, &v);
                }
                h.finish()
            }
        }
    }

    fn source_hash(&mut self, id: u32, out: usize) -> u64 {
        if let Some(h) = self.src_hash.get(&(id, out)) {
            return *h;
        }
        let mut h = hasher();
        out.hash(&mut h);
        if let Some(n) = self.nodes.get(&id).copied() {
            n.op.hash(&mut h);
            hash_params(&mut h, n);
            if n.op == "time" {
                self.frame.hash(&mut h);
            }
            if n.op == "geo_scene_obj" {
                // L'objet bouge d'une image à l'autre : sa boîte du moment entre dans l'empreinte
                if let Some((c, size)) = scene_object(pstr(n, "objet")) {
                    for x in c.iter().chain(&size) {
                        x.to_bits().hash(&mut h);
                    }
                }
            }
            let count = n.def().map(|d| d.inputs.len()).unwrap_or(0);
            for i in 0..count {
                self.field_hash(id, i).hash(&mut h);
            }
        }
        let r = h.finish();
        self.src_hash.insert((id, out), r);
        r
    }

    /// Calcule un champ sur une géométrie : compile la chaîne de nodes en amont de l'entrée, puis
    /// l'exécute point par point. Les attributs de la géométrie y sont lisibles par leur nom.
    fn field(&mut self, node: u32, inp: usize, geo: &Geo) -> Result<Field, Fail> {
        if self.link(node, inp).is_none() {
            let n = self.nodes[&node];
            return Ok(Field::Const(n.values.get(inp).copied().or_else(|| n.def().and_then(|d| d.inputs.get(inp)).map(|i| i.2)).unwrap_or([0.0; 4])));
        }
        // Sans point, rien à calculer (et un attribut pas encore né n'est pas une erreur)
        if geo.n == 0 {
            return Ok(Field::Per(vec![]));
        }
        let mut attrs: Vec<String> = vec!["index".into(), "P".into()];
        attrs.extend(geo.cols.keys().filter(|k| k.as_str() != "P").cloned());
        let prog = Program::compile_field(self.g, node, inp, &attrs, self.root, self.frame as f32, self.time.fps)?;
        if let Some(v) = prog.is_const() {
            return Ok(Field::Const(v));
        }
        let cols: Vec<Option<&Column>> = attrs.iter().map(|a| geo.col(a)).collect();
        let (nrm, cd) = (geo.col("N"), geo.col("Cd"));
        let eval = |regs: &mut Vec<V>, i: usize| -> V {
            if regs.len() < cols.len() {
                regs.resize(cols.len(), [0.0; 4]);
            }
            regs[0] = [i as f32; 4];
            for (k, c) in cols.iter().enumerate().skip(1) {
                regs[k] = c.map(|c| c.get(i)).unwrap_or([0.0; 4]);
            }
            let p = geo.pos(i);
            let n = nrm.map(|c| c.get(i)).unwrap_or([0.0, 1.0, 0.0, 0.0]);
            let c = Ctx { pos: p, uv: [p[0], p[2]], nrm: [n[0], n[1], n[2]], view: [0.0, 0.0, 1.0], vcol: cd.map(|c| c.get(i)).unwrap_or([1.0; 4]), res: [1.0, 1.0], ..Default::default() };
            prog.field(&c, regs)
        };
        use rayon::prelude::*;
        let vals: Vec<V> = if geo.n >= 4096 { (0..geo.n).into_par_iter().map_init(Vec::new, |regs, i| eval(regs, i)).collect() } else {
            let mut regs = Vec::new();
            (0..geo.n).map(|i| eval(&mut regs, i)).collect()
        };
        Ok(Field::Per(vals))
    }

    /// Réglage qui vaut pour toute la géométrie : la valeur de l'entrée, ou celle de la chaîne de
    /// nodes branchée dessus, calculée une fois (elle peut lire le temps, pas les points).
    fn uniform(&mut self, node: u32, inp: usize) -> Result<V, Fail> {
        if self.link(node, inp).is_none() {
            let n = self.nodes[&node];
            return Ok(n.values.get(inp).copied().or_else(|| n.def().and_then(|d| d.inputs.get(inp)).map(|i| i.2)).unwrap_or([0.0; 4]));
        }
        let attrs = ["index".to_string(), "P".to_string()];
        let prog = Program::compile_field(self.g, node, inp, &attrs, self.root, self.frame as f32, self.time.fps)?;
        if let Some(v) = prog.is_const() {
            return Ok(v);
        }
        let mut regs = vec![[0.0; 4]; 2];
        Ok(prog.field(&Ctx { nrm: [0.0, 1.0, 0.0], view: [0.0, 0.0, 1.0], vcol: [1.0; 4], res: [1.0, 1.0], ..Default::default() }, &mut regs))
    }

    /// Ce qui sort d'un node : repris du cache si la clé est connue, calculé sinon.
    fn geo(&mut self, id: u32, out: usize) -> Result<Geo, Fail> {
        let Some(n) = self.nodes.get(&id).copied() else { return Ok(Geo::default()) };
        self.depth += 1;
        if self.depth > 2000 {
            self.depth -= 1;
            return Err((id, "graphe trop profond".into()));
        }
        let r = self.geo_inner(n, out);
        self.depth -= 1;
        r
    }

    fn geo_inner(&mut self, n: &Node, out: usize) -> Result<Geo, Fail> {
        let id = n.id;
        match n.op.as_str() {
            "sim_in" => {
                // Dans sa zone : l'état de l'image d'avant. Vu du dehors (aperçu) : ce qui y entre.
                return match &self.env {
                    Some(env) if env.begin == id => Ok(env.state.get(out.wrapping_sub(1)).cloned().unwrap_or_default()),
                    _ => self.geo_in(id, out.wrapping_sub(1)),
                };
            }
            "sim_out" => return Ok(self.sim_state(id)?.get(out).cloned().unwrap_or_default()),
            _ => {}
        }
        let Some(d) = n.def() else { return Err((id, format!("node inconnu : {}", n.op))) };
        if d.outputs.get(out).map(|o| o.1) != Some(Sock::Geo) {
            return Err((id, "cette sortie ne donne pas une géométrie".into()));
        }
        // Entrées : les géométries d'abord (leur empreinte entre dans la clé), les champs par leur empreinte
        let mut gin: Vec<Geo> = Vec::with_capacity(d.inputs.len());
        let mut h = hasher();
        n.op.hash(&mut h);
        out.hash(&mut h);
        hash_params(&mut h, n);
        for (i, inp) in d.inputs.iter().enumerate() {
            if inp.1 == Sock::Geo {
                let g = self.geo_in(id, i)?;
                g.stamp().hash(&mut h);
                gin.push(g);
            } else {
                self.field_hash(id, i).hash(&mut h);
                gin.push(Geo::default());
            }
        }
        let key = h.finish();
        let volatile = self.body.contains(&id);
        if !volatile {
            if let Some(o) = self.e.touch(key) {
                self.e.stats.reused += 1;
                return Ok(o.into_iter().next().unwrap_or_default());
            }
        }
        let t0 = std::time::Instant::now();
        let mut r = self.exec(n, &gin)?;
        self.e.stats.computed += 1;
        if let Some(old) = self.e.last.get(&(id, out)) {
            let before = old.stamp();
            r.adopt(old);
            if r.stamp() == before {
                self.e.stats.cutoffs += 1;
            }
        }
        self.e.last.insert((id, out), r.clone());
        if !volatile {
            self.e.put(key, vec![r.clone()], t0.elapsed().as_secs_f32() * 1000.0);
        }
        Ok(r)
    }

    /// État d'une zone de simulation à l'image courante.
    fn sim_state(&mut self, end: u32) -> Result<Vec<Geo>, Fail> {
        let begin = crate::nodeflow::zone_pair(self.g, end).ok_or((end, "cette fin de simulation a perdu son début".to_string()))?;
        if self.env.is_some() {
            return Err((end, "une zone de simulation ne peut pas en contenir une autre".into()));
        }
        let ports = self.nodes[&end].ins.len();
        let (frame, start) = (self.frame, self.time.start);
        // Cache figé sur le disque : on le relit tel quel, sans regarder l'amont
        if let Some(dir) = self.e.baked.get(&end).cloned() {
            let f = frame.clamp(start, self.time.end);
            let mut state = Vec::with_capacity(ports);
            for k in 0..ports {
                let key = {
                    let mut h = hasher();
                    ("baked", &dir, f, k).hash(&mut h);
                    h.finish()
                };
                let geo = match self.e.touch(key) {
                    Some(o) => o.into_iter().next().unwrap_or_default(),
                    None => {
                        let bytes = std::fs::read(bake_file(&dir, f, k)).map_err(|e| (end, format!("cache figé illisible : {e}")))?;
                        let geo = Geo::from_bytes(&bytes).map_err(|e| (end, e))?;
                        self.e.put(key, vec![geo.clone()], 1.0);
                        geo
                    }
                };
                state.push(geo);
            }
            return Ok(state);
        }
        // État de départ : ce qui entre dans la zone à la première image
        self.set_frame(start);
        let init: Result<Vec<Geo>, Fail> = (0..ports).map(|k| self.geo_in(begin, k)).collect();
        self.set_frame(frame);
        let init = init?;
        // Clé de la zone : tout ce qui est en amont de sa fin (nodes, réglages, liens) et l'état de départ.
        // Toucher à quoi que ce soit là-dedans invalide ses images ; toucher à l'aval ne les invalide pas.
        let base = {
            let mut h = hasher();
            self.cone_hash(end).hash(&mut h);
            for g in &init {
                g.stamp().hash(&mut h);
            }
            self.time.fps.to_bits().hash(&mut h);
            start.hash(&mut h);
            // Les objets de la scène que le graphe regarde : toute leur trajectoire compte
            if let Some(inputs) = self.e.scene.clone() {
                for n in self.g.nodes.iter().filter(|n| n.op == "geo_scene_obj") {
                    inputs.track_hash(pstr(n, "objet"), &mut h);
                }
            }
            h.finish()
        };
        match self.e.sim_frames.get_mut(&end) {
            Some(x) if x.0 == base => {}
            _ => {
                self.e.sim_frames.insert(end, (base, Default::default()));
            }
        }
        if frame <= start {
            return Ok(init);
        }
        // Dernier état connu avant l'image demandée
        let mut f0 = frame;
        let mut state = None;
        while f0 > start {
            if let Some(s) = self.e.touch(sim_key(base, f0)) {
                state = Some(s);
                break;
            }
            f0 -= 1;
        }
        if f0 == frame {
            self.e.stats.reused += 1;
        }
        let mut state = state.unwrap_or(init);
        let saved_body = std::mem::replace(&mut self.body, crate::nodeflow::zone_body(self.g, begin, end).into_iter().collect());
        let mut fail = None;
        for f in f0 + 1..=frame {
            if self.e.deadline.is_some_and(|d| std::time::Instant::now() > d) {
                self.e.incomplete = true;
                break;
            }
            let t0 = std::time::Instant::now();
            self.set_frame(f);
            self.env = Some(Env { begin, state: state.clone() });
            // Quand le calcul se fait par tranches, un solveur sur carte graphique peut rendre la
            // main avant d'avoir fini : l'image n'est alors pas rangée, et sera redemandée
            SIM_DEFER.with(|d| d.set(self.e.deadline.is_some()));
            let next: Result<Vec<Geo>, Fail> = (0..ports)
                .map(|k| match self.link(end, k).map(|l| (l.from, l.out)) {
                    Some((from, out)) => self.geo(from, out),
                    // Rien de branché sur la fin : l'état traverse la zone sans changer
                    None => Ok(state[k].clone()),
                })
                .collect();
            self.env = None;
            SIM_DEFER.with(|d| d.set(false));
            if SIM_WAIT.with(|w| w.replace(false)) {
                self.e.incomplete = true;
                break;
            }
            match next {
                Ok(s) => state = s,
                Err(e) => {
                    fail = Some(e);
                    break;
                }
            }
            self.e.stats.steps += 1;
            self.e.put(sim_key(base, f), state.clone(), t0.elapsed().as_secs_f32() * 1000.0);
            if let Some(x) = self.e.sim_frames.get_mut(&end) {
                x.1.insert(f);
            }
        }
        self.body = saved_body;
        self.set_frame(frame);
        match fail {
            Some(e) => Err(e),
            None => Ok(state),
        }
    }

    fn set_frame(&mut self, f: i32) {
        if self.frame != f {
            self.frame = f;
            self.src_hash.clear();
            scene_now(&self.e.scene, f);
        }
    }

    /// Empreinte de tout ce qui est en amont d'un node : types, réglages, valeurs, liens.
    fn cone_hash(&self, id: u32) -> u64 {
        let mut seen: Vec<u32> = vec![];
        let mut stack = vec![id];
        let mut set = HashSet::new();
        while let Some(n) = stack.pop() {
            if set.insert(n) {
                seen.push(n);
                if let Some(v) = self.into.get(&n) {
                    stack.extend(v.iter().map(|l| l.from));
                }
            }
        }
        seen.sort_unstable();
        let mut h = hasher();
        for id in seen {
            id.hash(&mut h);
            if let Some(n) = self.nodes.get(&id) {
                n.op.hash(&mut h);
                hash_params(&mut h, n);
                for v in &n.values {
                    hash_v(&mut h, v);
                }
                for p in &n.ins {
                    p.name.hash(&mut h);
                }
            }
            if let Some(v) = self.into.get(&id) {
                let mut ls: Vec<(usize, u32, usize)> = v.iter().map(|l| (l.inp, l.from, l.out)).collect();
                ls.sort_unstable();
                ls.hash(&mut h);
            }
        }
        h.finish()
    }

    /// Exécute un node de géométrie. `gin` : ses entrées de géométrie, au rang de chaque entrée.
    fn exec(&mut self, n: &Node, gin: &[Geo]) -> Result<Geo, Fail> {
        let id = n.id;
        // Réglage uniforme : valeur d'une entrée, la même pour toute la géométrie
        macro_rules! num {
            ($k:expr) => {
                self.uniform(id, $k)?
            };
        }
        let v3 = |v: V| [v[0], v[1], v[2]];
        Ok(match n.op.as_str() {
            "geo_out" => gin[0].clone(),
            "geo_grid" => {
                let (w, d) = (num!(0)[0], num!(1)[0]);
                let (nx, nz) = ((num!(2)[0].round() as usize).clamp(1, 4096), (num!(3)[0].round() as usize).clamp(1, 4096));
                let wall = pstr(n, "plan") == "mur (XY)";
                let total = nx * nz;
                let mut p = [Vec::with_capacity(total), Vec::with_capacity(total), Vec::with_capacity(total)];
                for j in 0..nz {
                    for i in 0..nx {
                        let u = if nx > 1 { i as f32 / (nx - 1) as f32 - 0.5 } else { 0.0 };
                        let v = if nz > 1 { j as f32 / (nz - 1) as f32 - 0.5 } else { 0.0 };
                        if wall {
                            p[0].push(u * w);
                            p[1].push(-v * d);
                            p[2].push(0.0);
                        } else {
                            p[0].push(u * w);
                            p[1].push(0.0);
                            p[2].push(v * d);
                        }
                    }
                }
                let mut g = Geo::points(p);
                if pstr(n, "arêtes") != "non" {
                    let mut e = Vec::with_capacity(total * 2);
                    for j in 0..nz {
                        for i in 0..nx {
                            let a = (j * nx + i) as u32;
                            if i + 1 < nx {
                                e.push([a, a + 1]);
                            }
                            if j + 1 < nz {
                                e.push([a, a + nx as u32]);
                            }
                        }
                    }
                    // Diagonales : sans elles, un tissu se cisaille sans résistance
                    if pstr(n, "arêtes") == "avec diagonales" {
                        for j in 0..nz.saturating_sub(1) {
                            for i in 0..nx.saturating_sub(1) {
                                let a = (j * nx + i) as u32;
                                e.push([a, a + nx as u32 + 1]);
                                e.push([a + 1, a + nx as u32]);
                            }
                        }
                    }
                    g.set_edges(e);
                }
                // Faces (des quads tournés vers +Y pour le sol, vers +Z pour le mur) et UV : c'est
                // ce qui fait de la grille une surface qu'une scène 3D ombre, texture et rend
                if pstr(n, "faces") != "non" && nx > 1 && nz > 1 {
                    let mut faces = Vec::with_capacity((nx - 1) * (nz - 1));
                    for j in 0..nz - 1 {
                        for i in 0..nx - 1 {
                            let a = (j * nx + i) as u32;
                            faces.push([a, a + nx as u32, a + nx as u32 + 1, a + 1]);
                        }
                    }
                    g.set_faces(faces);
                    let mut uv = [Vec::with_capacity(total), Vec::with_capacity(total)];
                    for j in 0..nz {
                        for i in 0..nx {
                            uv[0].push(i as f32 / (nx - 1) as f32);
                            uv[1].push(1.0 - j as f32 / (nz - 1) as f32);
                        }
                    }
                    g.set("uv", Column::new(uv.into()));
                }
                g
            }
            "geo_sphere" => {
                let (count, r) = ((num!(0)[0].round() as usize).clamp(1, 4_000_000), num!(1)[0]);
                let mut p = [Vec::with_capacity(count), Vec::with_capacity(count), Vec::with_capacity(count)];
                // Spirale de Fibonacci : des points bien répartis sur la sphère
                let golden = std::f32::consts::PI * (3.0 - 5.0f32.sqrt());
                for i in 0..count {
                    let y = 1.0 - 2.0 * (i as f32 + 0.5) / count as f32;
                    let rad = (1.0 - y * y).max(0.0).sqrt();
                    let a = golden * i as f32;
                    p[0].push(a.cos() * rad * r);
                    p[1].push(y * r);
                    p[2].push(a.sin() * rad * r);
                }
                Geo::points(p)
            }
            "geo_circle" => {
                let (count, r) = ((num!(0)[0].round() as usize).clamp(3, 1_000_000), num!(1)[0]);
                let mut p = [Vec::with_capacity(count), Vec::with_capacity(count), Vec::with_capacity(count)];
                for i in 0..count {
                    let a = i as f32 / count as f32 * std::f32::consts::TAU;
                    p[0].push(a.cos() * r);
                    p[1].push(0.0);
                    p[2].push(a.sin() * r);
                }
                let mut g = Geo::points(p);
                g.set_edges((0..count as u32).map(|i| [i, (i + 1) % count as u32]).collect());
                g
            }
            "geo_line" => {
                let count = (num!(0)[0].round() as usize).clamp(2, 1_000_000);
                let (a, b) = (v3(num!(1)), v3(num!(2)));
                let mut p = [Vec::with_capacity(count), Vec::with_capacity(count), Vec::with_capacity(count)];
                for i in 0..count {
                    let t = i as f32 / (count - 1) as f32;
                    for k in 0..3 {
                        p[k].push(a[k] + (b[k] - a[k]) * t);
                    }
                }
                let mut g = Geo::points(p);
                g.set_edges((0..count as u32 - 1).map(|i| [i, i + 1]).collect());
                g
            }
            "geo_scatter" => {
                let count = (num!(0)[0].round().max(0.0) as usize).min(4_000_000);
                let (size, seed) = (v3(num!(1)), num!(2)[0]);
                let mut p = [Vec::with_capacity(count), Vec::with_capacity(count), Vec::with_capacity(count)];
                for i in 0..count {
                    for k in 0..3 {
                        p[k].push((rand(i as u32, k as u32, seed) - 0.5) * size[k]);
                    }
                }
                Geo::points(p)
            }
            "geo_transform" => {
                let mut g = gin[0].clone();
                let (t, r, s) = (v3(num!(1)), v3(num!(2)), v3(num!(3)));
                let (sx, cx) = r[0].to_radians().sin_cos();
                let (sy, cy) = r[1].to_radians().sin_cos();
                let (sz, cz) = r[2].to_radians().sin_cos();
                let mut p = g.vec3("P");
                for i in 0..g.n {
                    let q = [p[0][i] * s[0], p[1][i] * s[1], p[2][i] * s[2]];
                    let q = [q[0], q[1] * cx - q[2] * sx, q[1] * sx + q[2] * cx];
                    let q = [q[0] * cy + q[2] * sy, q[1], -q[0] * sy + q[2] * cy];
                    let q = [q[0] * cz - q[1] * sz, q[0] * sz + q[1] * cz, q[2]];
                    for k in 0..3 {
                        p[k][i] = q[k] + t[k];
                    }
                }
                g.set("P", Column::new(p.into()));
                g
            }
            "geo_set_pos" => {
                let mut g = gin[0].clone();
                let sel = self.field(id, 1, &g)?;
                let pos = if self.link(id, 2).is_some() { Some(self.field(id, 2, &g)?) } else { None };
                let off = self.field(id, 3, &g)?;
                let mut p = g.vec3("P");
                for i in 0..g.n {
                    if sel.at(i)[0] > 0.5 {
                        let (o, q) = (off.at(i), pos.as_ref().map(|f| f.at(i)));
                        for k in 0..3 {
                            p[k][i] = q.map(|q| q[k]).unwrap_or(p[k][i]) + o[k];
                        }
                    }
                }
                g.set("P", Column::new(p.into()));
                g
            }
            "geo_attr" => {
                let mut g = gin[0].clone();
                let name = pstr(n, "nom").trim();
                if name.is_empty() {
                    return Err((id, "donne un nom à l'attribut".into()));
                }
                let width = match pstr(n, "type") {
                    "nombre" => 1,
                    "vecteur" => 3,
                    _ => 4,
                };
                if name == "P" && width != 3 {
                    return Err((id, "la position « P » est un vecteur : choisis le type vecteur".into()));
                }
                let val = self.field(id, 1, &g)?;
                let sel = self.field(id, 2, &g)?;
                let mut data: Vec<Vec<f32>> = match g.col(name) {
                    Some(c) if c.width() == width => c.data.clone(),
                    _ => (0..width).map(|k| vec![if k == 3 { 1.0 } else { 0.0 }; g.n]).collect(),
                };
                for i in 0..g.n {
                    if sel.at(i)[0] > 0.5 {
                        let v = val.at(i);
                        for k in 0..width {
                            data[k][i] = v[k];
                        }
                    }
                }
                g.set(name, Column::new(data));
                g
            }
            "geo_merge" => merge(&gin[0], &gin[1]),
            "geo_delete" => {
                let g = &gin[0];
                let sel = self.field(id, 1, g)?;
                let keep: Vec<bool> = (0..g.n).map(|i| sel.at(i)[0] <= 0.5).collect();
                filter(g, &keep)
            }
            "geo_copy" => {
                let (pts, model) = (&gin[0], &gin[1]);
                let scale = self.field(id, 2, pts)?;
                let total = pts.n.saturating_mul(model.n);
                if total > 8_000_000 {
                    return Err((id, "cette copie ferait plus de 8 millions de points : réduis le modèle ou le nombre de points".into()));
                }
                let mp = model.vec3("P");
                let mut p = [Vec::with_capacity(total), Vec::with_capacity(total), Vec::with_capacity(total)];
                let mut edges = Vec::with_capacity(pts.n * model.edges.len());
                let mut faces = Vec::with_capacity(pts.n * model.faces.len());
                for i in 0..pts.n {
                    let (c, s) = (pts.pos(i), scale.at(i)[0]);
                    for j in 0..model.n {
                        for k in 0..3 {
                            p[k].push(c[k] + mp[k][j] * s);
                        }
                    }
                    let base = (i * model.n) as u32;
                    edges.extend(model.edges.iter().map(|e| [e[0] + base, e[1] + base]));
                    faces.extend(model.faces.iter().map(|f| shift_face(*f, base)));
                }
                let mut g = Geo::points(p);
                // Chaque copie garde les attributs de son point
                for (name, c) in pts.cols.iter().filter(|(k, _)| *k != "P") {
                    g.set(name, Column::new(c.data.iter().map(|comp| comp.iter().flat_map(|x| std::iter::repeat_n(*x, model.n)).collect()).collect()));
                }
                if !edges.is_empty() {
                    g.set_edges(edges);
                }
                if !faces.is_empty() {
                    g.set_faces(faces);
                    // Chaque copie garde les UV du modèle
                    if let (Some(c), false) = (model.col("uv"), pts.cols.contains_key("uv")) {
                        g.set("uv", Column::new(c.data.iter().map(|comp| comp.iter().copied().cycle().take(total).collect()).collect()));
                    }
                }
                g
            }
            "sim_forces" => {
                let mut g = gin[0].clone();
                let gravity = v3(num!(1));
                let wind = self.field(id, 2, &g)?;
                let (drag, dt) = (num!(3)[0].max(0.0), num!(4)[0]);
                let mut v = g.vec3("v");
                let pin = g.col("pin").map(|c| c.data[0].clone());
                let keep = (1.0 - drag * dt).clamp(0.0, 1.0);
                for i in 0..g.n {
                    if pin.as_ref().is_some_and(|p| p[i] > 0.5) {
                        for k in 0..3 {
                            v[k][i] = 0.0;
                        }
                        continue;
                    }
                    let w = wind.at(i);
                    for k in 0..3 {
                        v[k][i] = (v[k][i] + (gravity[k] + w[k]) * dt) * keep;
                    }
                }
                g.set("v", Column::new(v.into()));
                g
            }
            "sim_move" => {
                let mut g = gin[0].clone();
                let dt = num!(1)[0];
                let v = g.vec3("v");
                let mut p = g.vec3("P");
                for k in 0..3 {
                    for i in 0..g.n {
                        p[k][i] += v[k][i] * dt;
                    }
                }
                g.set("P", Column::new(p.into()));
                g
            }
            "sim_collide" => {
                let mut g = gin[0].clone();
                let (floor, bounce, grip) = (num!(1)[0], num!(2)[0].clamp(0.0, 1.0), num!(3)[0].clamp(0.0, 1.0));
                let (c, r) = (v3(num!(4)), num!(5)[0]);
                let mut p = g.vec3("P");
                let mut v = g.vec3("v");
                for i in 0..g.n {
                    if p[1][i] < floor {
                        p[1][i] = floor;
                        if v[1][i] < 0.0 {
                            v[1][i] = -v[1][i] * bounce;
                        }
                        v[0][i] *= 1.0 - grip;
                        v[2][i] *= 1.0 - grip;
                    }
                    if r > 0.0 {
                        let d = [p[0][i] - c[0], p[1][i] - c[1], p[2][i] - c[2]];
                        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                        if len < r && len > 1e-9 {
                            let nrm = [d[0] / len, d[1] / len, d[2] / len];
                            let vn = v[0][i] * nrm[0] + v[1][i] * nrm[1] + v[2][i] * nrm[2];
                            for k in 0..3 {
                                p[k][i] = c[k] + nrm[k] * r;
                                // La vitesse qui rentrait dans la sphère rebondit ; celle qui glisse dessus est freinée
                                let tangent = v[k][i] - vn * nrm[k];
                                v[k][i] = tangent * (1.0 - grip) + if vn < 0.0 { -vn * bounce * nrm[k] } else { vn * nrm[k] };
                            }
                        }
                    }
                }
                g.set("P", Column::new(p.into()));
                g.set("v", Column::new(v.into()));
                g
            }
            "sim_emit" => {
                let (state, src) = (&gin[0], &gin[1]);
                let per = num!(2)[0].round().max(0.0) as usize;
                let (spread, life, dt) = (num!(4)[0], num!(5)[0], num!(6)[0]);
                // Les points vieillissent ; ceux qui ont fait leur temps disparaissent
                let mut g = state.clone();
                if g.n > 0 {
                    let age: Vec<f32> = match g.col("age") {
                        Some(c) => c.data[0].iter().map(|a| a + dt).collect(),
                        None => vec![dt; g.n],
                    };
                    let keep: Vec<bool> = age.iter().map(|a| life <= 0.0 || *a <= life).collect();
                    g.set("age", Column::new(vec![age]));
                    if keep.iter().any(|k| !k) {
                        g = filter(&g, &keep);
                    }
                }
                if per == 0 || src.n == 0 {
                    return Ok(g);
                }
                if g.n + per > 4_000_000 {
                    return Err((id, "plus de 4 millions de particules : baisse le débit ou la durée de vie".into()));
                }
                // Chaque particule a un numéro unique : son hasard ne change pas quand d'autres naissent ou meurent
                let first = g.col("id").map(|c| c.data[0].iter().fold(-1.0f32, |m, x| m.max(*x)) + 1.0).unwrap_or(0.0);
                let mut born = [Vec::with_capacity(per), Vec::with_capacity(per), Vec::with_capacity(per)];
                let mut ids = Vec::with_capacity(per);
                for k in 0..per {
                    let pid = first + k as f32;
                    let s = (rand(pid as u32, 7, 0.0) * src.n as f32) as usize % src.n;
                    let q = src.pos(s);
                    for c in 0..3 {
                        born[c].push(q[c]);
                    }
                    ids.push(pid);
                }
                let mut fresh = Geo::points(born);
                fresh.set("id", Column::new(vec![ids.clone()]));
                fresh.set("age", Column::new(vec![vec![0.0; per]]));
                // La vitesse de départ est un champ, calculé sur les nouveaux points (il peut lire leur position, leur numéro)
                let vel = self.field(id, 3, &fresh)?;
                let mut v = [Vec::with_capacity(per), Vec::with_capacity(per), Vec::with_capacity(per)];
                for k in 0..per {
                    let base = vel.at(k);
                    for c in 0..3 {
                        v[c].push(base[c] + (rand(ids[k] as u32, 11 + c as u32, 0.0) - 0.5) * 2.0 * spread);
                    }
                }
                fresh.set("v", Column::new(v.into()));
                merge(&g, &fresh)
            }
            "sim_solver" => {
                let mut g = gin[0].clone();
                let (floor, ball_c, ball_r) = (num!(5)[0], v3(num!(6)), num!(7)[0]);
                let p = SolverParams {
                    iterations: (num!(1)[0].round() as usize).clamp(1, 2000),
                    compliance: num!(2)[0],
                    damping: num!(3)[0],
                    floor: (pstr(n, "sol") != "non").then_some(floor),
                    ball: (ball_r > 0.0).then_some((ball_c, ball_r)),
                    grip: num!(8)[0],
                    cube: {
                        let (c, size) = (v3(num!(12)), v3(num!(13)));
                        size.iter().all(|s| *s > 0.0).then_some((c, [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5]))
                    },
                    stiffness: num!(9)[0].max(0.0),
                    // Le réglage est en millièmes de N·m : 0,1 pour un coton, 1 pour une toile épaisse
                    bending: num!(10)[0].max(0.0) * 1e-3,
                    substeps: (num!(11)[0].round() as usize).clamp(1, 64),
                    shear: num!(14)[0].clamp(0.0, 1.0),
                    calm: num!(15)[0].clamp(0.0, 1.0),
                    thickness: (pstr(n, "auto-collision") == "oui").then_some(num!(16)[0].max(0.0)),
                };
                let dt = num!(4)[0];
                let name = pstr(n, "solveur");
                let solver = solvers().iter().find(|s| s.name() == name).or(solvers().first()).ok_or((id, "aucun solveur".to_string()))?;
                solver.step(&mut g, dt, &p);
                g
            }
            other => return Err((id, format!("ce node ne donne pas de géométrie : {other}"))),
        })
    }
}

/// Nombre pseudo-aléatoire stable entre 0 et 1, d'après un numéro d'élément, un canal et une graine.
fn rand(i: u32, k: u32, seed: f32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B) ^ seed.to_bits().wrapping_mul(0xC2B2_AE35);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Une face dont les sommets sont décalés de `base` (le quatrième indice d'un triangle reste vide).
fn shift_face(f: [u32; 4], base: u32) -> [u32; 4] {
    [f[0] + base, f[1] + base, f[2] + base, if f[3] == NO_VERT { NO_VERT } else { f[3] + base }]
}

/// Réunit deux géométries. Un attribut absent d'un côté y vaut zéro.
fn merge(a: &Geo, b: &Geo) -> Geo {
    if b.n == 0 {
        return a.clone();
    }
    if a.n == 0 {
        return b.clone();
    }
    let mut g = Geo { n: a.n + b.n, ..Default::default() };
    let names: std::collections::BTreeSet<&String> = a.cols.keys().chain(b.cols.keys()).collect();
    for name in names {
        let (ca, cb) = (a.cols.get(name), b.cols.get(name));
        let width = ca.map(|c| c.width()).unwrap_or(0).max(cb.map(|c| c.width()).unwrap_or(0));
        let data = (0..width)
            .map(|k| {
                let mut v = Vec::with_capacity(g.n);
                let part = |c: Option<&Arc<Column>>, n: usize, v: &mut Vec<f32>| match c.and_then(|c| c.data.get(k)) {
                    Some(d) => v.extend_from_slice(d),
                    None => v.extend(std::iter::repeat_n(if k == 3 { 1.0 } else { 0.0 }, n)),
                };
                part(ca, a.n, &mut v);
                part(cb, b.n, &mut v);
                v
            })
            .collect();
        g.set(name, Column::new(data));
    }
    if !a.edges.is_empty() || !b.edges.is_empty() {
        let off = a.n as u32;
        g.set_edges(a.edges.iter().copied().chain(b.edges.iter().map(|e| [e[0] + off, e[1] + off])).collect());
    }
    if !a.faces.is_empty() || !b.faces.is_empty() {
        let off = a.n as u32;
        g.set_faces(a.faces.iter().copied().chain(b.faces.iter().map(|f| shift_face(*f, off))).collect());
    }
    g
}

/// Ne garde que les points marqués ; les arêtes et les faces dont un sommet disparaît partent avec lui.
fn filter(g: &Geo, keep: &[bool]) -> Geo {
    let n = keep.iter().filter(|k| **k).count();
    let mut out = Geo { n, ..Default::default() };
    for (name, c) in &g.cols {
        out.set(name, Column::new(c.data.iter().map(|comp| comp.iter().zip(keep).filter(|(_, k)| **k).map(|(x, _)| *x).collect()).collect()));
    }
    let mut map = vec![u32::MAX; g.n];
    if !g.edges.is_empty() || !g.faces.is_empty() {
        let mut k = 0;
        for (i, keep) in keep.iter().enumerate() {
            if *keep {
                map[i] = k;
                k += 1;
            }
        }
    }
    if !g.faces.is_empty() {
        let at = |i: u32| map.get(i as usize).copied().unwrap_or(u32::MAX);
        let faces: Vec<[u32; 4]> = g
            .faces
            .iter()
            .filter(|f| f.iter().enumerate().all(|(k, i)| (k == 3 && *i == NO_VERT) || at(*i) != u32::MAX))
            .map(|f| [at(f[0]), at(f[1]), at(f[2]), if f[3] == NO_VERT { NO_VERT } else { at(f[3]) }])
            .collect();
        out.set_faces(faces);
    }
    if !g.edges.is_empty() {
        let kept: Vec<bool> = g.edges.iter().map(|e| map[e[0] as usize] != u32::MAX && map[e[1] as usize] != u32::MAX).collect();
        out.set_edges(g.edges.iter().zip(&kept).filter(|(_, k)| **k).map(|(e, _)| [map[e[0] as usize], map[e[1] as usize]]).collect());
        for (name, c) in &g.ecols {
            out.ecols.insert(name.clone(), Arc::new(Column::new(c.data.iter().map(|comp| comp.iter().zip(&kept).filter(|(_, k)| **k).map(|(x, _)| *x).collect()).collect())));
        }
    }
    out
}

// ---------- graphes de départ ----------

fn set_value(g: &mut Graph, id: u32, k: usize, v: V) {
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) {
        while n.values.len() <= k {
            n.values.push([0.0; 4]);
        }
        n.values[k] = v;
    }
}

fn set_param(g: &mut Graph, id: u32, k: &str, v: &str) {
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) {
        n.params.insert(k.into(), Param::S(v.into()));
    }
}

/// Graphe de départ : un tissu accroché par le haut, qui tombe sur une boule. Il montre d'un coup
/// une source, un champ (ce qui est épinglé), une zone de simulation et un solveur.
pub fn starter() -> Graph {
    let mut g = Graph::empty(crate::nodes::GraphKind::Geometry);
    g.time = Some(TimeRange { fps: 24.0, start: 1, end: 120 });
    // Rangée du haut : la forme de départ. Rangée du bas : la simulation.
    let grid = g.add("geo_grid", [0.0, 0.0]);
    set_value(&mut g, grid, 0, [2.0; 4]);
    set_value(&mut g, grid, 1, [2.0; 4]);
    set_value(&mut g, grid, 2, [28.0; 4]);
    set_value(&mut g, grid, 3, [28.0; 4]);
    set_param(&mut g, grid, "arêtes", "avec diagonales");
    let lift = g.add("geo_transform", [250.0, 0.0]);
    set_value(&mut g, lift, 1, [0.0, 1.6, 0.0, 0.0]);
    // Épinglés : les points dont z est tout au fond
    let pos = g.add("position", [250.0, 330.0]);
    let sep = g.add("sepxyz", [480.0, 330.0]);
    let cmp = g.add("compare", [710.0, 330.0]);
    set_param(&mut g, cmp, "comparaison", "inférieur");
    set_value(&mut g, cmp, 1, [-0.98; 4]);
    let pin = g.add("geo_attr", [960.0, 0.0]);
    set_param(&mut g, pin, "nom", "pin");
    set_param(&mut g, pin, "type", "nombre");
    let (begin, end) = crate::nodeflow::add_zone(&mut g, "sim", [0.0, 620.0]);
    let forces = g.add("sim_forces", [250.0, 620.0]);
    let solver = g.add("sim_solver", [500.0, 620.0]);
    set_value(&mut g, solver, 6, [0.0, 0.7, 0.0, 0.0]);
    set_value(&mut g, solver, 7, [0.55; 4]);
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == end) {
        n.pos = [750.0, 620.0];
    }
    // La boule sur laquelle le tissu retombe, montrée en points
    let ball = g.add("geo_sphere", [500.0, 1010.0]);
    set_value(&mut g, ball, 0, [400.0; 4]);
    set_value(&mut g, ball, 1, [0.53; 4]);
    let ball_at = g.add("geo_transform", [750.0, 1010.0]);
    set_value(&mut g, ball_at, 1, [0.0, 0.7, 0.0, 0.0]);
    let ball_tint = g.add("geo_attr", [1010.0, 1010.0]);
    set_value(&mut g, ball_tint, 1, [0.25, 0.42, 0.75, 1.0]);
    let tint = g.add("geo_attr", [1010.0, 620.0]);
    set_value(&mut g, tint, 1, [0.9, 0.84, 0.78, 1.0]);
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == tint) {
        n.preview = false;
    }
    let both = g.add("geo_merge", [1260.0, 620.0]);
    let out = g.add("geo_out", [1510.0, 620.0]);
    g.connect(ball, 0, ball_at, 0);
    g.connect(end, 0, tint, 0);
    g.connect(tint, 0, both, 0);
    g.connect(ball_at, 0, ball_tint, 0);
    g.connect(ball_tint, 0, both, 1);
    for id in [ball, ball_at, ball_tint] {
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) {
            n.preview = false;
        }
    }
    for id in [pos, sep, cmp] {
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) {
            n.preview = false;
        }
    }
    g.connect(grid, 0, lift, 0);
    g.connect(lift, 0, pin, 0);
    g.connect(pos, 0, sep, 0);
    g.connect(sep, 2, cmp, 0);
    g.connect(cmp, 0, pin, 1);
    g.connect(pin, 0, begin, 0);
    g.connect(begin, 1, forces, 0);
    g.connect(begin, 0, forces, 4);
    g.connect(forces, 0, solver, 0);
    g.connect(begin, 0, solver, 4);
    g.connect(solver, 0, end, 0);
    g.connect(both, 0, out, 0);
    g.frames.push(crate::nodes::Frame { rect: [230.0, 290.0, 690.0, 190.0], label: "ce qui est épinglé : les points du fond".into(), color: [90, 120, 200], note: String::new() });
    g
}

/// Exemples proposés dans l'éditeur : chacun montre une façon de se servir des nodes de géométrie.
pub const EXAMPLES: [&str; 6] = ["tissu sur une boule", "tissu sur la carte graphique", "tissu qui se replie", "drapeau dans le vent", "fontaine de particules", "vagues animées"];

pub fn example(name: &str) -> Option<Graph> {
    match name {
        "tissu sur une boule" => Some(starter()),
        "tissu sur la carte graphique" => Some(cloth_gpu()),
        "tissu qui se replie" => Some(cloth_fold()),
        "drapeau dans le vent" => Some(flag()),
        "fontaine de particules" => Some(fountain()),
        "vagues animées" => Some(waves()),
        _ => None,
    }
}

/// Le tissu de départ, plus fin et sans point fixe, calculé par le solveur sur carte graphique :
/// une nappe lâchée sur une boule et une boîte, qui les épouse en retombant.
fn cloth_gpu() -> Graph {
    let mut g = starter();
    let id = |g: &Graph, op: &str| g.nodes.iter().find(|n| n.op == op).map(|n| n.id).unwrap_or(0);
    let (grid, solver) = (id(&g, "geo_grid"), id(&g, "sim_solver"));
    set_value(&mut g, grid, 2, [48.0; 4]);
    set_value(&mut g, grid, 3, [48.0; 4]);
    unpin(&mut g);
    set_param(&mut g, solver, "solveur", SOLVER_NAMES[1]);
    set_value(&mut g, solver, 1, [32.0; 4]);
    set_value(&mut g, solver, 6, [0.25, 0.7, 0.1, 0.0]);
    set_value(&mut g, solver, 7, [0.5; 4]);
    set_value(&mut g, solver, 12, [-0.55, 0.3, 0.0, 0.0]);
    set_value(&mut g, solver, 13, [0.5, 0.6, 0.8, 0.0]);
    // La boule montrée suit celle du solveur
    let shown: Vec<u32> = g.nodes.iter().filter(|n| n.op == "geo_transform").map(|n| n.id).collect();
    if let Some(ball_at) = shown.last().copied() {
        set_value(&mut g, ball_at, 1, [0.25, 0.7, 0.1, 0.0]);
    }
    let ball = id(&g, "geo_sphere");
    set_value(&mut g, ball, 1, [0.48; 4]);
    g
}

/// Un tissu debout, un peu penché, lâché sur le sol : il s'affaisse et se replie sur lui-même,
/// sans se traverser (auto-collision), calculé par le solveur sur carte graphique.
fn cloth_fold() -> Graph {
    let mut g = starter();
    let id = |g: &Graph, op: &str| g.nodes.iter().find(|n| n.op == op).map(|n| n.id).unwrap_or(0);
    let (grid, lift, solver) = (id(&g, "geo_grid"), id(&g, "geo_transform"), id(&g, "sim_solver"));
    for k in 0..2 {
        set_value(&mut g, grid, k, [1.6; 4]);
        set_value(&mut g, grid, 2 + k, [40.0; 4]);
    }
    // Debout, penché de dix degrés, le bas juste au-dessus du sol
    set_value(&mut g, lift, 1, [0.0, 0.9, 0.0, 0.0]);
    set_value(&mut g, lift, 2, [80.0, 0.0, 0.0, 0.0]);
    set_param(&mut g, solver, "solveur", SOLVER_NAMES[1]);
    set_param(&mut g, solver, "auto-collision", "oui");
    set_value(&mut g, solver, 7, [0.0; 4]);
    // Ni boule ni points fixes dans cet exemple
    let ball = id(&g, "geo_sphere");
    g.remove(ball);
    let shown: Vec<u32> = g.nodes.iter().filter(|n| n.op == "geo_transform" && n.id != lift).map(|n| n.id).collect();
    for node in shown {
        g.remove(node);
    }
    // La teinte de la boule n'a plus rien en entrée ; l'attribut `pin` reste, à zéro partout
    let orphans: Vec<u32> = g.nodes.iter().filter(|n| n.op == "geo_attr" && !g.links.iter().any(|l| l.to == n.id && l.inp == 0)).map(|n| n.id).collect();
    for node in orphans {
        g.remove(node);
    }
    unpin(&mut g);
    g
}

/// Un drapeau debout, tenu par son bord gauche, dans un vent qui souffle par rafales. Rien d'autre
/// dans le graphe (ni sol, ni boule) : il est fait pour être posé dans une scène 3D, où ses faces
/// sont ombrées et ses UV reçoivent un matériau.
fn flag() -> Graph {
    let mut g = Graph::empty(crate::nodes::GraphKind::Geometry);
    g.time = Some(TimeRange { fps: 24.0, start: 1, end: 120 });
    let grid = g.add("geo_grid", [0.0, 0.0]);
    set_value(&mut g, grid, 0, [3.0; 4]);
    set_value(&mut g, grid, 1, [2.0; 4]);
    set_value(&mut g, grid, 2, [36.0; 4]);
    set_value(&mut g, grid, 3, [24.0; 4]);
    set_param(&mut g, grid, "plan", "mur (XY)");
    set_param(&mut g, grid, "arêtes", "avec diagonales");
    // Le bas du drapeau à un mètre du sol
    let lift = g.add("geo_transform", [250.0, 0.0]);
    set_value(&mut g, lift, 1, [0.0, 2.0, 0.0, 0.0]);
    // Épinglés : les points du bord gauche (la hampe)
    let pos = g.add("position", [250.0, 330.0]);
    let sep = g.add("sepxyz", [480.0, 330.0]);
    let cmp = g.add("compare", [710.0, 330.0]);
    set_param(&mut g, cmp, "comparaison", "inférieur");
    set_value(&mut g, cmp, 1, [-1.49; 4]);
    let pin = g.add("geo_attr", [960.0, 0.0]);
    set_param(&mut g, pin, "nom", "pin");
    set_param(&mut g, pin, "type", "nombre");
    let (begin, end) = crate::nodeflow::add_zone(&mut g, "sim", [0.0, 620.0]);
    // Le vent : une poussée le long du drapeau, et des rafales de côté qui vont et viennent
    let time = g.add("time", [-500.0, 900.0]);
    let gust = g.add("expr", [-250.0, 900.0]);
    set_param(&mut g, gust, "formule", "sin(a * 0.31) * 7 + sin(a * 0.13 + y * 2) * 4");
    let wind = g.add("combxyz", [0.0, 900.0]);
    set_value(&mut g, wind, 0, [22.0; 4]);
    let forces = g.add("sim_forces", [250.0, 620.0]);
    set_value(&mut g, forces, 3, [0.6; 4]);
    let solver = g.add("sim_solver", [500.0, 620.0]);
    set_param(&mut g, solver, "sol", "non");
    set_value(&mut g, solver, 1, [24.0; 4]);
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == end) {
        n.pos = [750.0, 620.0];
    }
    let tint = g.add("geo_attr", [1010.0, 620.0]);
    set_value(&mut g, tint, 1, [0.8, 0.16, 0.14, 1.0]);
    let out = g.add("geo_out", [1260.0, 620.0]);
    g.connect(grid, 0, lift, 0);
    g.connect(lift, 0, pin, 0);
    g.connect(pos, 0, sep, 0);
    g.connect(sep, 0, cmp, 0);
    g.connect(cmp, 0, pin, 1);
    g.connect(pin, 0, begin, 0);
    g.connect(time, 0, gust, 0);
    g.connect(gust, 0, wind, 2);
    g.connect(wind, 0, forces, 2);
    g.connect(begin, 1, forces, 0);
    g.connect(begin, 0, forces, 4);
    g.connect(forces, 0, solver, 0);
    g.connect(begin, 0, solver, 4);
    g.connect(solver, 0, end, 0);
    g.connect(end, 0, tint, 0);
    g.connect(tint, 0, out, 0);
    hide_thumbs(&mut g, &[pos, sep, cmp, time, gust, wind, tint]);
    g.frames.push(crate::nodes::Frame { rect: [230.0, 290.0, 690.0, 190.0], label: "ce qui est épinglé : le bord gauche, la hampe".into(), color: [90, 120, 200], note: String::new() });
    g.frames.push(crate::nodes::Frame { rect: [-520.0, 860.0, 730.0, 190.0], label: "le vent : il pousse le long du drapeau, avec des rafales de côté".into(), color: [80, 160, 110], note: String::new() });
    g
}

/// Retire du graphe de départ ce qui épingle les points du fond : l'attribut `pin` reste, à zéro partout.
fn unpin(g: &mut Graph) {
    for op in ["position", "sepxyz", "compare"] {
        if let Some(node) = g.nodes.iter().find(|n| n.op == op).map(|n| n.id) {
            g.remove(node);
        }
    }
    let pin = g.nodes.iter().find(|n| n.op == "geo_attr" && n.params.get("nom").is_some_and(|p| matches!(p, crate::nodes::Param::S(s) if s == "pin"))).map(|n| n.id).unwrap_or(0);
    set_value(g, pin, 1, [0.0; 4]);
    g.frames.clear();
}

fn hide_thumbs(g: &mut Graph, ids: &[u32]) {
    for n in g.nodes.iter_mut().filter(|n| ids.contains(&n.id)) {
        n.preview = false;
    }
}

/// Des particules jaillissent d'un petit cercle, retombent, rebondissent, et changent de couleur en vieillissant.
fn fountain() -> Graph {
    let mut g = Graph::empty(crate::nodes::GraphKind::Geometry);
    g.time = Some(TimeRange { fps: 24.0, start: 1, end: 160 });
    let src = g.add("geo_circle", [0.0, 300.0]);
    set_value(&mut g, src, 0, [24.0; 4]);
    set_value(&mut g, src, 1, [0.12; 4]);
    let (begin, end) = crate::nodeflow::add_zone(&mut g, "sim", [0.0, 0.0]);
    let emit = g.add("sim_emit", [250.0, 0.0]);
    set_value(&mut g, emit, 2, [40.0; 4]);
    set_value(&mut g, emit, 3, [0.0, 5.5, 0.0, 0.0]);
    set_value(&mut g, emit, 4, [0.9; 4]);
    set_value(&mut g, emit, 5, [2.6; 4]);
    let forces = g.add("sim_forces", [500.0, 0.0]);
    set_value(&mut g, forces, 3, [0.05; 4]);
    let mv = g.add("sim_move", [750.0, 0.0]);
    let hit = g.add("sim_collide", [1000.0, 0.0]);
    set_value(&mut g, hit, 2, [0.45; 4]);
    set_value(&mut g, hit, 3, [0.15; 4]);
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == end) {
        n.pos = [1250.0, 0.0];
    }
    // La couleur suit l'âge : du jaune chaud au bleu
    let age = g.add("attribute", [750.0, 330.0]);
    set_param(&mut g, age, "nom", "age");
    let ratio = g.add("math", [1000.0, 330.0]);
    set_param(&mut g, ratio, "opération", "diviser");
    set_value(&mut g, ratio, 1, [2.6; 4]);
    let ramp = g.add("colorramp", [1250.0, 330.0]);
    set_param(&mut g, ramp, "dégradé", "0,1,0.85,0.35,1;0.5,0.95,0.4,0.25,1;1,0.2,0.3,0.9,1");
    let tint = g.add("geo_attr", [1500.0, 0.0]);
    let out = g.add("geo_out", [1750.0, 0.0]);
    g.connect(begin, 1, emit, 0);
    g.connect(src, 0, emit, 1);
    g.connect(emit, 0, forces, 0);
    g.connect(forces, 0, mv, 0);
    g.connect(mv, 0, hit, 0);
    g.connect(hit, 0, end, 0);
    for node in [emit, forces, mv] {
        let k = g.nodes.iter().find(|n| n.id == node).and_then(|n| n.def()).map(|d| d.inputs.len() - 1).unwrap_or(0);
        g.connect(begin, 0, node, k);
    }
    g.connect(end, 0, tint, 0);
    g.connect(age, 0, ratio, 0);
    g.connect(ratio, 0, ramp, 0);
    g.connect(ramp, 0, tint, 1);
    g.connect(tint, 0, out, 0);
    hide_thumbs(&mut g, &[age, ratio, ramp]);
    g.frames.push(crate::nodes::Frame { rect: [730.0, 290.0, 730.0, 250.0], label: "la couleur suit l'âge".into(), color: [200, 140, 60], note: String::new() });
    g
}

/// Une grille dont la hauteur et la couleur suivent une formule qui lit le temps : pas de simulation,
/// seulement un champ recalculé à chaque image.
fn waves() -> Graph {
    let mut g = Graph::empty(crate::nodes::GraphKind::Geometry);
    g.time = Some(TimeRange { fps: 24.0, start: 1, end: 96 });
    let grid = g.add("geo_grid", [0.0, 0.0]);
    set_value(&mut g, grid, 0, [3.0; 4]);
    set_value(&mut g, grid, 1, [3.0; 4]);
    set_value(&mut g, grid, 2, [60.0; 4]);
    set_value(&mut g, grid, 3, [60.0; 4]);
    set_param(&mut g, grid, "arêtes", "non");
    let time = g.add("time", [0.0, 330.0]);
    let wave = g.add("expr", [250.0, 330.0]);
    set_param(&mut g, wave, "formule", "sin(x * 4 + a * 0.26) * cos(z * 3 - a * 0.13) * 0.18");
    let up = g.add("combxyz", [500.0, 330.0]);
    let lift = g.add("geo_set_pos", [750.0, 0.0]);
    let level = g.add("map", [500.0, 520.0]);
    set_value(&mut g, level, 1, [-0.18; 4]);
    set_value(&mut g, level, 2, [0.18; 4]);
    let ramp = g.add("colorramp", [750.0, 520.0]);
    set_param(&mut g, ramp, "dégradé", "0,0.05,0.15,0.45,1;0.6,0.15,0.6,0.75,1;1,0.95,0.98,1,1");
    let tint = g.add("geo_attr", [1000.0, 0.0]);
    let out = g.add("geo_out", [1250.0, 0.0]);
    g.connect(time, 0, wave, 0);
    g.connect(wave, 0, up, 1);
    g.connect(grid, 0, lift, 0);
    g.connect(up, 0, lift, 3);
    g.connect(wave, 0, level, 0);
    g.connect(level, 0, ramp, 0);
    g.connect(lift, 0, tint, 0);
    g.connect(ramp, 0, tint, 1);
    g.connect(tint, 0, out, 0);
    hide_thumbs(&mut g, &[time, wave, up, level, ramp]);
    g.frames.push(crate::nodes::Frame { rect: [-20.0, 290.0, 960.0, 420.0], label: "le champ : une hauteur par point, qui lit le temps".into(), color: [80, 160, 110], note: String::new() });
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une petite grille (4 × 3 points, 6 quads) suivie d'une chaîne de nodes, jusqu'à la sortie.
    fn grid_then(build: impl FnOnce(&mut Graph, u32) -> u32) -> Geo {
        let mut g = Graph::empty(crate::nodes::GraphKind::Geometry);
        let grid = g.add("geo_grid", [0.0, 0.0]);
        set_value(&mut g, grid, 2, [4.0; 4]);
        set_value(&mut g, grid, 3, [3.0; 4]);
        let last = build(&mut g, grid);
        let out = g.add("geo_out", [900.0, 0.0]);
        g.connect(last, 0, out, 0);
        Engine::default().eval(&g, 1, "").unwrap()
    }

    #[test]
    fn grid_has_quads_and_uv() {
        let g = grid_then(|_, grid| grid);
        assert_eq!((g.n, g.faces.len()), (12, 6));
        assert!(g.faces.iter().all(|f| f.iter().all(|i| (*i as usize) < g.n)), "des quads, tous dans les points");
        let (tris, sides) = g.triangles();
        assert_eq!((tris.len(), sides[0], sides[1]), (12, 0b011, 0b110));
        // UV : un coin à (0, 1), le coin opposé à (1, 0)
        let uv = g.col("uv").unwrap();
        assert_eq!((uv.width(), uv.get(0)[0], uv.get(0)[1], uv.get(11)[0], uv.get(11)[1]), (2, 0.0, 1.0, 1.0, 0.0));
        // Le réglage « faces : non » rend l'ancienne grille : des points et des arêtes, rien d'autre
        let bare = grid_then(|g, grid| {
            set_param(g, grid, "faces", "non");
            grid
        });
        assert!(bare.faces.is_empty() && bare.col("uv").is_none() && !bare.edges.is_empty());
    }

    #[test]
    fn an_old_grid_without_the_setting_still_loads() {
        // Un graphe enregistré avant les faces : sa grille n'a pas le réglage « faces »
        let mut g = Graph::empty(crate::nodes::GraphKind::Geometry);
        let grid = g.add("geo_grid", [0.0, 0.0]);
        let out = g.add("geo_out", [200.0, 0.0]);
        g.connect(grid, 0, out, 0);
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == grid) {
            n.params.remove("faces");
        }
        let text = serde_json::to_string(&g).unwrap();
        assert!(!text.contains("faces"));
        let old: Graph = serde_json::from_str(&text).unwrap();
        let geo = Engine::default().eval(&old, 1, "").unwrap();
        // Mêmes points et mêmes arêtes qu'avant, avec en plus de quoi l'ombrer
        assert_eq!((geo.n, geo.edges.len(), geo.faces.len()), (400, 2 * 20 * 19, 19 * 19));
    }

    #[test]
    fn faces_follow_the_points_through_the_nodes() {
        let base = grid_then(|_, grid| grid);
        // Déplacer, placer les points, définir un attribut : les faces passent telles quelles
        let moved = grid_then(|g, grid| {
            let t = g.add("geo_transform", [200.0, 0.0]);
            set_value(g, t, 1, [0.0, 3.0, 0.0, 0.0]);
            let p = g.add("geo_set_pos", [400.0, 0.0]);
            set_value(g, p, 3, [1.0, 0.0, 0.0, 0.0]);
            let a = g.add("geo_attr", [600.0, 0.0]);
            g.connect(grid, 0, t, 0);
            g.connect(t, 0, p, 0);
            g.connect(p, 0, a, 0);
            a
        });
        assert_eq!(*moved.faces, *base.faces);
        assert!(moved.col("uv").is_some() && moved.col("Cd").is_some());
        assert!((moved.pos(0)[1] - 3.0).abs() < 1e-6 && (moved.pos(0)[0] - base.pos(0)[0] - 1.0).abs() < 1e-6);
        // Réunir : les faces de la seconde géométrie sont décalées du nombre de points de la première
        let both = grid_then(|g, grid| {
            let m = g.add("geo_merge", [200.0, 0.0]);
            g.connect(grid, 0, m, 0);
            g.connect(grid, 0, m, 1);
            m
        });
        assert_eq!((both.n, both.faces.len()), (24, 12));
        assert_eq!(both.faces[6], base.faces[0].map(|i| i + 12));
        // Supprimer des points : les faces qui perdent un sommet partent, les autres sont renumérotées
        let cut = grid_then(|g, grid| {
            let idx = g.add("index", [0.0, 300.0]);
            let cmp = g.add("compare", [200.0, 300.0]);
            set_param(g, cmp, "comparaison", "inférieur");
            set_value(g, cmp, 1, [4.0; 4]);
            let d = g.add("geo_delete", [400.0, 0.0]);
            g.connect(idx, 0, cmp, 0);
            g.connect(grid, 0, d, 0);
            g.connect(cmp, 0, d, 1);
            d
        });
        // La première rangée de points (4) disparaît : il reste une rangée de quads (3)
        assert_eq!((cut.n, cut.faces.len()), (8, 3));
        assert!(cut.faces.iter().all(|f| f.iter().all(|i| (*i as usize) < cut.n)));
        // Copier sur des points : chaque copie garde les faces et les UV du modèle
        let copies = grid_then(|g, grid| {
            let pts = g.add("geo_line", [0.0, 300.0]);
            set_value(g, pts, 0, [3.0; 4]);
            let c = g.add("geo_copy", [300.0, 0.0]);
            g.connect(pts, 0, c, 0);
            g.connect(grid, 0, c, 1);
            c
        });
        assert_eq!((copies.n, copies.faces.len()), (36, 18));
        assert_eq!(copies.faces[17], base.faces[5].map(|i| i + 24));
        assert_eq!(copies.col("uv").map(|c| c.get(35)), base.col("uv").map(|c| c.get(11)));
    }

    #[test]
    fn faces_survive_the_cloth_simulation_and_its_cache() {
        for name in ["tissu sur une boule", "drapeau dans le vent"] {
            let g = example(name).unwrap();
            let mut e = Engine::default();
            let rest = e.eval(&g, 1, "").unwrap();
            let later = e.eval(&g, 12, "").unwrap();
            assert!(!rest.faces.is_empty(), "{name} : la grille apporte des faces");
            assert_eq!(*later.faces, *rest.faces, "{name} : mêmes faces après douze images");
            assert!(later.col("uv").is_some());
            assert!((0..rest.n).any(|i| later.pos(i) != rest.pos(i)), "{name} : le tissu a bougé");
            // Le cache d'une image garde les faces
            let back = Geo::from_bytes(&later.to_bytes()).unwrap();
            assert_eq!((*back.faces).clone(), (*later.faces).clone());
            assert_eq!(back.pos(5), later.pos(5));
        }
        // Un cache écrit avant les faces (rien après les arêtes) se relit, sans faces
        let mut old = Geo::points([vec![0.0, 1.0], vec![0.0; 2], vec![0.0; 2]]);
        old.set_edges(vec![[0, 1]]);
        let bytes = old.to_bytes();
        assert!(!bytes.windows(4).any(|w| w == b"FACE"));
        let back = Geo::from_bytes(&bytes).unwrap();
        assert!(back.faces.is_empty() && back.edges.len() == 1);
        // Un bloc de faces tronqué est refusé plutôt que lu de travers
        let mut tri = old.clone();
        tri.set_faces(vec![[0, 1, 0, NO_VERT]]);
        let full = tri.to_bytes();
        assert_eq!(Geo::from_bytes(&full).unwrap().faces[0], [0, 1, 0, NO_VERT]);
        assert!(Geo::from_bytes(&full[..full.len() - 3]).is_err());
    }
    use crate::nodes::GraphKind;

    #[test]
    fn exemples_se_calculent() {
        for name in EXAMPLES {
            let g = example(name).unwrap();
            let mut e = Engine::default();
            let a = e.eval(&g, 1, "").map_err(|x| format!("{name} : {}", x.1)).unwrap();
            let b = e.eval(&g, 30, "").map_err(|x| format!("{name} : {}", x.1)).unwrap();
            assert!(b.n > 0, "{name}");
            assert!((0..b.n).all(|i| b.pos(i).iter().all(|x| x.is_finite())), "{name}");
            assert!(b.col("Cd").is_some(), "{name} : chaque exemple colore ses points");
            // Chaque exemple bouge : l'image 30 n'est pas l'image 1
            assert!(a.n != b.n || (0..a.n).any(|i| a.pos(i) != b.pos(i)), "{name}");
            // Enregistré puis relu, il donne la même chose
            let back: Graph = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
            let mut e2 = Engine::default();
            assert_eq!(e2.eval(&back, 30, "").map_err(|x| x.1).unwrap().n, b.n);
        }
        // La fontaine : pas de particule sous le sol
        let g = example("fontaine de particules").unwrap();
        let r = Engine::default().eval(&g, 120, "").map_err(|x| x.1).unwrap();
        assert!(r.n > 1000);
        assert!((0..r.n).all(|i| r.pos(i)[1] >= -1e-4));
    }

    fn base() -> (Graph, u32, u32) {
        let mut g = Graph::empty(GraphKind::Geometry);
        g.time = Some(TimeRange { fps: 24.0, start: 1, end: 48 });
        let grid = g.add("geo_grid", [0.0, 0.0]);
        set_value(&mut g, grid, 2, [10.0; 4]);
        set_value(&mut g, grid, 3, [10.0; 4]);
        let out = g.add("geo_out", [900.0, 0.0]);
        (g, grid, out)
    }

    #[test]
    fn colonnes_partagees_et_cache_disque() {
        let (mut g, grid, out) = base();
        let tr = g.add("geo_transform", [300.0, 0.0]);
        set_value(&mut g, tr, 1, [0.0, 2.0, 0.0, 0.0]);
        let col = g.add("geo_attr", [600.0, 0.0]);
        set_value(&mut g, col, 1, [1.0, 0.5, 0.25, 1.0]);
        g.connect(grid, 0, tr, 0);
        g.connect(tr, 0, col, 0);
        g.connect(col, 0, out, 0);
        let mut e = Engine::default();
        let r = e.eval(&g, 1, "").map_err(|x| x.1).unwrap();
        assert_eq!(r.n, 100);
        assert_eq!(r.edges.len(), 180);
        assert!((r.pos(0)[1] - 2.0).abs() < 1e-6);
        assert_eq!(r.col("Cd").unwrap().get(42), [1.0, 0.5, 0.25, 1.0]);
        // Le node qui pose la couleur n'a pas touché la position : c'est la même colonne, pas une copie
        let moved = e.eval_node(&g, tr, 0, 1, "").map_err(|x| x.1).unwrap();
        assert!(Arc::ptr_eq(&moved.cols["P"], &r.cols["P"]));
        // Cache disque : relu à l'identique
        let back = Geo::from_bytes(&r.to_bytes()).unwrap();
        assert_eq!(back.n, r.n);
        assert_eq!(back.cols["P"].data, r.cols["P"].data);
        assert_eq!(back.cols["Cd"].data, r.cols["Cd"].data);
        assert_eq!(*back.edges, *r.edges);
        assert!(Geo::from_bytes(b"n'importe quoi").is_err());
    }

    #[test]
    fn seul_l_aval_est_refait_et_coupure_anticipee() {
        let (mut g, grid, out) = base();
        let tr = g.add("geo_transform", [300.0, 0.0]);
        let col = g.add("geo_attr", [600.0, 0.0]);
        g.connect(grid, 0, tr, 0);
        g.connect(tr, 0, col, 0);
        g.connect(col, 0, out, 0);
        let mut e = Engine::default();
        e.eval(&g, 1, "").map_err(|x| x.1).unwrap();
        assert_eq!(e.stats.computed, 3);
        // Même graphe, autre image : rien ne dépend du temps, tout vient du cache
        e.eval(&g, 7, "").map_err(|x| x.1).unwrap();
        assert_eq!((e.stats.computed, e.stats.reused), (0, 3));
        // On change la couleur : la grille et le déplacement ne sont pas refaits
        set_value(&mut g, col, 1, [0.0, 1.0, 0.0, 1.0]);
        e.eval(&g, 1, "").map_err(|x| x.1).unwrap();
        assert_eq!(e.stats.computed, 1);
        // On change le milieu de la chaîne : la grille reste, le reste est refait
        set_value(&mut g, tr, 1, [1.0, 0.0, 0.0, 0.0]);
        e.eval(&g, 1, "").map_err(|x| x.1).unwrap();
        assert_eq!(e.stats.computed, 2);
        // Coupure anticipée : un réglage qui ne change rien au résultat (rotation d'un tour complet
        // à zéro près… ici une échelle remise à 1) refait le node, mais pas l'aval
        set_value(&mut g, tr, 3, [1.0, 1.0, 1.0, 0.5]);
        e.eval(&g, 1, "").map_err(|x| x.1).unwrap();
        assert_eq!((e.stats.computed, e.stats.cutoffs), (1, 1), "le déplacement est refait, identique : la couleur vient du cache");
    }

    #[test]
    fn champs_attributs_et_temps() {
        let (mut g, grid, out) = base();
        // Soulever les points selon leur x : décalage = (0, x, 0)
        let pos = g.add("position", [0.0, 300.0]);
        let sep = g.add("sepxyz", [200.0, 300.0]);
        let comb = g.add("combxyz", [400.0, 300.0]);
        let set = g.add("geo_set_pos", [600.0, 0.0]);
        g.connect(pos, 0, sep, 0);
        g.connect(sep, 0, comb, 1);
        g.connect(grid, 0, set, 0);
        g.connect(comb, 0, set, 3);
        g.connect(set, 0, out, 0);
        let mut e = Engine::default();
        let r = e.eval(&g, 1, "").map_err(|x| x.1).unwrap();
        for i in 0..r.n {
            let p = r.pos(i);
            assert!((p[1] - p[0]).abs() < 1e-6);
        }
        // Un attribut posé se relit par son nom ; un nom inconnu donne une erreur claire
        let attr = g.add("geo_attr", [800.0, 0.0]);
        set_param(&mut g, attr, "nom", "poids");
        set_param(&mut g, attr, "type", "nombre");
        set_value(&mut g, attr, 1, [3.0; 4]);
        let read = g.add("attribute", [800.0, 300.0]);
        set_param(&mut g, read, "nom", "poids");
        let del = g.add("geo_delete", [1000.0, 0.0]);
        g.connect(set, 0, attr, 0);
        g.connect(attr, 0, del, 0);
        let cmp = g.add("compare", [900.0, 300.0]);
        set_param(&mut g, cmp, "comparaison", "supérieur");
        set_value(&mut g, cmp, 1, [2.0; 4]);
        g.connect(read, 0, cmp, 0);
        g.connect(cmp, 0, del, 1);
        g.connect(del, 0, out, 0);
        assert_eq!(e.eval(&g, 1, "").map_err(|x| x.1).unwrap().n, 0, "poids = 3 > 2 : tout est supprimé");
        set_param(&mut g, read, "nom", "inconnu");
        let err = e.eval(&g, 1, "").err().unwrap();
        assert_eq!(err.0, read);
        assert!(err.1.contains("inconnu"));
        // Le temps : un champ qui lit l'image est refait à chaque image, le reste non
        let (mut g, grid, out) = base();
        let time = g.add("time", [0.0, 300.0]);
        let comb = g.add("combxyz", [200.0, 300.0]);
        let set = g.add("geo_set_pos", [400.0, 0.0]);
        g.connect(time, 0, comb, 1);
        g.connect(comb, 0, set, 3);
        g.connect(grid, 0, set, 0);
        g.connect(set, 0, out, 0);
        let mut e = Engine::default();
        assert!((e.eval(&g, 5, "").map_err(|x| x.1).unwrap().pos(0)[1] - 5.0).abs() < 1e-6);
        assert_eq!(e.stats.computed, 2);
        assert!((e.eval(&g, 9, "").map_err(|x| x.1).unwrap().pos(0)[1] - 9.0).abs() < 1e-6);
        assert_eq!(e.stats.computed, 1, "la grille ne dépend pas du temps");
        e.eval(&g, 5, "").map_err(|x| x.1).unwrap();
        assert_eq!(e.stats.computed, 0, "l'image 5 est encore en cache");
    }

    /// Des points qui tombent : zone de simulation avec forces et déplacement.
    fn falling() -> (Graph, u32, u32, u32) {
        let (mut g, grid, out) = base();
        let (b, e) = crate::nodeflow::add_zone(&mut g, "sim", [300.0, 0.0]);
        let f = g.add("sim_forces", [500.0, 0.0]);
        set_value(&mut g, f, 3, [0.0; 4]);
        let m = g.add("sim_move", [700.0, 0.0]);
        g.connect(grid, 0, b, 0);
        g.connect(b, 1, f, 0);
        g.connect(b, 0, f, 4);
        g.connect(f, 0, m, 0);
        g.connect(b, 0, m, 1);
        g.connect(m, 0, e, 0);
        g.connect(e, 0, out, 0);
        (g, f, e, out)
    }

    #[test]
    fn simulation_etat_par_image_et_cache() {
        let (mut g, forces, end, _) = falling();
        let mut e = Engine::default();
        // Première image : l'état de départ, sans rien simuler
        let r = e.eval(&g, 1, "").map_err(|x| x.1).unwrap();
        assert_eq!(r.pos(0)[1], 0.0);
        assert_eq!(e.stats.steps, 0);
        // Image 11 : dix pas. Chute libre, intégration d'Euler : y = -g × dt² × (1 + 2 + … + 10)
        let r = e.eval(&g, 11, "").map_err(|x| x.1).unwrap();
        assert_eq!(e.stats.steps, 10);
        let dt = 1.0f32 / 24.0;
        assert!((r.pos(0)[1] + 9.8 * dt * dt * 55.0).abs() < 1e-3, "{}", r.pos(0)[1]);
        assert_eq!(e.cached_frames(end), (2..=11).collect::<Vec<i32>>());
        // Revenir en arrière, ou redemander la même image : rien à simuler
        e.eval(&g, 6, "").map_err(|x| x.1).unwrap();
        assert_eq!(e.stats.steps, 0);
        // Avancer de trois images : trois pas seulement, depuis le dernier état connu
        e.eval(&g, 14, "").map_err(|x| x.1).unwrap();
        assert_eq!(e.stats.steps, 3);
        // Changer la gravité invalide la simulation : tout repart de la première image
        set_value(&mut g, forces, 1, [0.0, -1.0, 0.0, 0.0]);
        let r = e.eval(&g, 4, "").map_err(|x| x.1).unwrap();
        assert_eq!(e.stats.steps, 3);
        assert!((r.pos(0)[1] + dt * dt * 6.0).abs() < 1e-4);
        // Un node ajouté après la zone ne la relance pas
        let out = g.nodes.iter().find(|n| n.op == "geo_out").unwrap().id;
        let tr = g.add("geo_transform", [1000.0, 0.0]);
        g.connect(end, 0, tr, 0);
        g.connect(tr, 0, out, 0);
        e.eval(&g, 4, "").map_err(|x| x.1).unwrap();
        assert_eq!((e.stats.steps, e.stats.computed), (0, 1));
    }

    #[test]
    fn simulation_interrompue_reprend_ou_elle_en_etait() {
        let (g, _, _, _) = falling();
        let mut e = Engine::default();
        // Délai déjà dépassé : aucun pas, et le moteur le dit
        e.deadline = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));
        e.eval(&g, 30, "").map_err(|x| x.1).unwrap();
        assert!(e.incomplete);
        assert_eq!(e.stats.steps, 0);
        e.deadline = None;
        e.eval(&g, 30, "").map_err(|x| x.1).unwrap();
        assert!(!e.incomplete);
        assert_eq!(e.stats.steps, 29);
    }

    #[test]
    fn cache_fige_sur_le_disque() {
        let (mut g, forces, end, _) = falling();
        g.time = Some(TimeRange { fps: 24.0, start: 1, end: 6 });
        let dir = std::env::temp_dir().join(format!("aryadne-geo-test-{}", std::process::id()));
        let mut e = Engine::default();
        assert_eq!(e.bake(&g, end, &dir, "").unwrap(), 6);
        let y = e.eval(&g, 6, "").map_err(|x| x.1).unwrap().pos(0)[1];
        // Figé : changer l'amont ne change plus rien, et rien n'est simulé
        set_value(&mut g, forces, 1, [0.0, 5.0, 0.0, 0.0]);
        let mut fresh = Engine::default();
        fresh.baked.insert(end, dir.clone());
        let r = fresh.eval(&g, 6, "").map_err(|x| x.1).unwrap();
        assert_eq!(r.pos(0)[1], y);
        assert_eq!(fresh.stats.steps, 0);
        // Libéré : la simulation suit de nouveau ses réglages
        fresh.baked.clear();
        assert!(fresh.eval(&g, 6, "").map_err(|x| x.1).unwrap().pos(0)[1] > 0.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn emission_collision_et_solveur() {
        // Émission : dix particules par image, qui meurent après un quart de seconde (six images)
        let mut g = Graph::empty(GraphKind::Geometry);
        g.time = Some(TimeRange { fps: 24.0, start: 1, end: 48 });
        let src = g.add("geo_sphere", [0.0, 200.0]);
        set_value(&mut g, src, 0, [50.0; 4]);
        let out = g.add("geo_out", [900.0, 0.0]);
        let (b, e) = crate::nodeflow::add_zone(&mut g, "sim", [300.0, 0.0]);
        let emit = g.add("sim_emit", [500.0, 0.0]);
        set_value(&mut g, emit, 2, [10.0; 4]);
        set_value(&mut g, emit, 5, [0.26; 4]);
        g.connect(b, 1, emit, 0);
        g.connect(src, 0, emit, 1);
        g.connect(b, 0, emit, 6);
        g.connect(emit, 0, e, 0);
        g.connect(e, 0, out, 0);
        let mut eng = Engine::default();
        assert_eq!(eng.eval(&g, 1, "").map_err(|x| x.1).unwrap().n, 0);
        assert_eq!(eng.eval(&g, 4, "").map_err(|x| x.1).unwrap().n, 30);
        let r = eng.eval(&g, 40, "").map_err(|x| x.1).unwrap();
        assert_eq!(r.n, 70, "régime permanent : sept générations vivantes");
        // Les numéros sont uniques
        let mut ids: Vec<i64> = r.col("id").unwrap().data[0].iter().map(|x| *x as i64).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 70);
        // Le tissu de départ : épinglé d'un côté, il pend sans s'étirer et ne traverse ni le sol ni la boule
        let g = starter();
        let mut eng = Engine::default();
        let end = g.nodes.iter().find(|n| n.op == "sim_out").unwrap().id;
        assert_eq!(eng.eval(&g, 1, "").map_err(|x| x.1).unwrap().n, 28 * 28 + 400, "le tissu et la boule");
        let first = eng.eval_node(&g, end, 0, 1, "").map_err(|x| x.1).unwrap();
        let r = eng.eval_node(&g, end, 0, 60, "").map_err(|x| x.1).unwrap();
        assert_eq!(r.n, 28 * 28);
        let pins: Vec<usize> = (0..r.n).filter(|i| r.col("pin").unwrap().data[0][*i] > 0.5).collect();
        assert_eq!(pins.len(), 28);
        for i in &pins {
            assert_eq!(r.pos(*i), first.pos(*i), "un point épinglé ne bouge pas");
        }
        let rest = &r.ecols["rest"].data[0];
        let mut worst = 0.0f32;
        for (k, e) in r.edges.iter().enumerate() {
            let (a, b) = (r.pos(e[0] as usize), r.pos(e[1] as usize));
            let len = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
            worst = worst.max((len / rest[k] - 1.0).abs());
        }
        assert!(worst < 0.25, "étirement maximal : {worst}");
        for i in 0..r.n {
            let p = r.pos(i);
            assert!(p[1] >= -1e-4, "sous le sol : {p:?}");
            let d = (p[0].powi(2) + (p[1] - 0.7).powi(2) + p[2].powi(2)).sqrt();
            assert!(d > 0.5, "dans la boule : {p:?}");
            assert!(p.iter().all(|x| x.is_finite()));
        }
        assert!((0..r.n).any(|i| r.pos(i)[1] < 1.2), "le tissu est tombé");
    }

    /// Le solveur sur carte graphique, par les nodes : l'exemple de tissu bascule dessus d'un
    /// réglage, et l'exemple dédié drape sur la boule et la boîte sans les traverser.
    #[test]
    fn tissu_sur_carte_par_les_nodes() {
        let check = |g: &Graph, side: usize, pinned: usize, frame: i32, ball: ([f32; 3], f32), cube: Option<([f32; 3], [f32; 3])>| {
            let mut eng = Engine::default();
            let end = g.nodes.iter().find(|n| n.op == "sim_out").unwrap().id;
            let first = eng.eval_node(g, end, 0, 1, "").map_err(|x| x.1).unwrap();
            let r = eng.eval_node(g, end, 0, frame, "").map_err(|x| x.1).unwrap();
            assert_eq!(r.n, side * side);
            assert_eq!(eng.stats.steps as i32, frame - 1);
            // La forme au repos est gardée dans l'état : une reprise retrouve le même tissu
            assert_eq!(r.vec3("repos"), first.vec3("P"));
            let mut pins = 0;
            let mut low = f32::MAX;
            for i in 0..r.n {
                let p = r.pos(i);
                assert!(p.iter().all(|x| x.is_finite()));
                if r.col("pin").unwrap().data[0][i] > 0.5 {
                    assert_eq!(p, first.pos(i), "un point épinglé ne bouge pas");
                    pins += 1;
                }
                assert!(p[1] >= -1e-4, "sous le sol : {p:?}");
                let d = ((p[0] - ball.0[0]).powi(2) + (p[1] - ball.0[1]).powi(2) + (p[2] - ball.0[2]).powi(2)).sqrt();
                assert!(d > ball.1 - 1e-3, "dans la boule : {p:?}");
                if let Some((c, h)) = cube {
                    assert!(!(0..3).all(|k| (p[k] - c[k]).abs() < h[k] - 1e-3), "dans la boîte : {p:?}");
                }
                low = low.min(p[1]);
            }
            assert_eq!(pins, pinned);
            assert!(low < 1.2, "le tissu est tombé");
            let (worst, mean) = crate::gpucloth::stretch(&r.edges, &r.ecols["rest"].data[0], &r.vec3("P"));
            assert!(worst < 0.35 && mean < 0.03, "allongement {worst} / {mean}");
            let st = cloth_status().expect("le solveur a tourné");
            assert_eq!(st.on_gpu, crate::gpusim::gpu().is_some());
            // Revenir en arrière ne recalcule rien, et la suite repart de l'image connue
            eng.eval_node(g, end, 0, frame - 5, "").map_err(|x| x.1).unwrap();
            assert_eq!(eng.stats.steps, 0);
            eng.eval_node(g, end, 0, frame + 1, "").map_err(|x| x.1).unwrap();
            assert_eq!(eng.stats.steps, 1);
        };
        // L'exemple existant, basculé sur l'autre solveur
        let mut g = starter();
        let solver = g.nodes.iter().find(|n| n.op == "sim_solver").unwrap().id;
        set_param(&mut g, solver, "solveur", SOLVER_NAMES[1]);
        check(&g, 28, 28, 30, ([0.0, 0.7, 0.0], 0.55), None);
        // L'exemple dédié : plus fin, sans point fixe, avec une boîte
        let g = example("tissu sur la carte graphique").unwrap();
        check(&g, 48, 0, 24, ([0.25, 0.7, 0.1], 0.5), Some(([-0.55, 0.3, 0.0], [0.25, 0.3, 0.4])));
    }

    /// Calcul par tranches : tant que la carte n'a pas fini son pas, le tissu rend la main au
    /// moteur, qui redemande l'image plus tard. Le résultat est le même qu'en attendant.
    #[test]
    fn tissu_sans_bloquer_le_moteur() {
        if crate::gpusim::gpu().is_none() {
            return;
        }
        let g = example("tissu sur la carte graphique").unwrap();
        let end = g.nodes.iter().find(|n| n.op == "sim_out").unwrap().id;
        let want = Engine::default().eval_node(&g, end, 0, 6, "").map_err(|x| x.1).unwrap();
        let mut eng = Engine::default();
        let (mut rounds, mut waited, mut steps) = (0, 0, 0);
        let got = loop {
            eng.deadline = Some(std::time::Instant::now() + std::time::Duration::from_millis(30));
            let r = eng.eval_node(&g, end, 0, 6, "").map_err(|x| x.1).unwrap();
            steps += eng.stats.steps;
            if !eng.incomplete {
                break r;
            }
            // Une image inachevée rend l'état de la dernière image finie, jamais un état à moitié calculé
            assert!(r.n == want.n && (0..r.n).all(|i| r.pos(i).iter().all(|x| x.is_finite())));
            waited += 1;
            rounds += 1;
            assert!(rounds < 200_000, "la simulation ne finit jamais");
            std::thread::sleep(std::time::Duration::from_micros(200));
        };
        eprintln!("{waited} attentes pour {steps} pas");
        // Chaque image n'est simulée qu'une fois, même redemandée plusieurs fois
        assert_eq!(steps, 5);
        let gap = (0..want.n).flat_map(|i| (0..3).map(move |k| (i, k))).map(|(i, k)| (want.pos(i)[k] - got.pos(i)[k]).abs()).fold(0.0f32, f32::max);
        assert!(gap < 1e-6, "écart {gap}");
        // Sans tranches (cuisson, rendu), le même moteur attend la carte comme avant
        eng.deadline = None;
        let more = eng.eval_node(&g, end, 0, 8, "").map_err(|x| x.1).unwrap();
        assert!(!eng.incomplete && eng.stats.steps == 2 && more.n == want.n);
    }

    /// Le XPBD connaît aussi la boîte : les deux solveurs se posent sur les mêmes obstacles.
    #[test]
    fn xpbd_ne_traverse_pas_la_boite() {
        let mut g = starter();
        let solver = g.nodes.iter().find(|n| n.op == "sim_solver").unwrap().id;
        set_value(&mut g, solver, 7, [0.0; 4]);
        set_value(&mut g, solver, 12, [0.0, 0.5, 0.0, 0.0]);
        set_value(&mut g, solver, 13, [0.8, 1.0, 0.8, 0.0]);
        let end = g.nodes.iter().find(|n| n.op == "sim_out").unwrap().id;
        let r = Engine::default().eval_node(&g, end, 0, 40, "").map_err(|x| x.1).unwrap();
        let mut on_top = 0;
        for i in 0..r.n {
            let p = r.pos(i);
            assert!(!(p[0].abs() < 0.399 && p[1] < 0.999 && p[2].abs() < 0.399), "dans la boîte : {p:?}");
            on_top += (p[0].abs() < 0.4 && (p[1] - 1.0).abs() < 1e-3 && p[2].abs() < 0.4) as usize;
        }
        assert!(on_top > 20, "le tissu repose sur la boîte ({on_top} points)");
    }

    /// Écrit les exemples dans le dossier donné par ARY_EXEMPLES, pour les essais à l'écran.
    #[test]
    #[ignore]
    fn ecrire_les_exemples() {
        let Some(dir) = std::env::var_os("ARY_EXEMPLES") else { return };
        for name in EXAMPLES {
            example(name).unwrap().save(&std::path::Path::new(&dir).join(format!("{name}.arygeo"))).unwrap();
        }
    }

    /// L'exemple du tissu qui se replie : il s'affaisse sur le sol, et ses couches restent à une
    /// épaisseur l'une de l'autre (sans auto-collision, elles se traversent).
    #[test]
    fn exemple_du_tissu_qui_se_replie() {
        let closest = |on: bool| {
            let mut g = example("tissu qui se replie").unwrap();
            let solver = g.nodes.iter().find(|n| n.op == "sim_solver").unwrap().id;
            set_param(&mut g, solver, "auto-collision", if on { "oui" } else { "non" });
            let end = g.nodes.iter().find(|n| n.op == "sim_out").unwrap().id;
            let mut eng = Engine::default();
            let first = eng.eval_node(&g, end, 0, 1, "").map_err(|x| x.1).unwrap();
            let r = eng.eval_node(&g, end, 0, 70, "").map_err(|x| x.1).unwrap();
            assert_eq!(r.n, 1600);
            assert!(r.col("pin").is_none_or(|c| c.data[0].iter().all(|p| *p < 0.5)), "aucun point fixe");
            let (a, b) = (first.vec3("P"), r.vec3("P"));
            assert!(b[1].iter().all(|y| y.is_finite() && *y >= -1e-4));
            assert!(b[1].iter().cloned().fold(0.0, f32::max) < 0.6, "le tissu s'est affaissé");
            let thick = crate::gpucloth::AUTO_THICK * 1.6 / 39.0;
            let mut least = f32::MAX;
            for i in 0..r.n {
                for j in i + 1..r.n {
                    let rest = (0..3).map(|k| (a[k][i] - a[k][j]).powi(2)).sum::<f32>().sqrt();
                    if rest >= 2.1 * thick {
                        least = least.min((0..3).map(|k| (b[k][i] - b[k][j]).powi(2)).sum::<f32>().sqrt());
                    }
                }
            }
            least / thick
        };
        let (with, without) = (closest(true), closest(false));
        assert!(with > 0.75, "couches à {with} épaisseur avec l'auto-collision");
        assert!(without < 0.3, "sans auto-collision, les couches devraient se traverser ({without})");
    }

    /// La comparaison à budget égal donne des nombres sensés (les vrais chiffres : `aryadne --banc-gpu`).
    #[test]
    #[ignore]
    fn comparaison_vbd_xpbd() {
        let c = cloth_comparison(40, &[16, 32, 64]);
        eprintln!("{c:#?}");
        assert!(c.rows.iter().all(|r| r.elongation.is_finite() && r.worst < 1.0));
    }

    #[test]
    fn budget_de_memoire_respecte() {
        let (g, _, _, _) = falling();
        let mut e = Engine::default();
        e.budget = 40_000;
        e.eval(&g, 48, "").map_err(|x| x.1).unwrap();
        assert!(e.cached_bytes() <= 40_000 + 8_000, "{}", e.cached_bytes());
        // Malgré l'éviction, le résultat reste juste : ce qui manque est recalculé
        let a = e.eval(&g, 20, "").map_err(|x| x.1).unwrap().pos(3);
        let mut fresh = Engine::default();
        assert_eq!(fresh.eval(&g, 20, "").map_err(|x| x.1).unwrap().pos(3), a);
    }

    #[test]
    fn groupes_et_repetitions_dans_la_geometrie() {
        // Une répétition qui déplace trois fois de 1 en x, rangée dans un groupe
        let (mut g, grid, out) = base();
        let (b, e) = crate::nodeflow::add_zone(&mut g, "repeat", [300.0, 0.0]);
        set_value(&mut g, b, 0, [3.0; 4]);
        let tr = g.add("geo_transform", [500.0, 0.0]);
        set_value(&mut g, tr, 1, [1.0, 0.0, 0.0, 0.0]);
        g.connect(grid, 0, b, 1);
        g.connect(b, 1, tr, 0);
        g.connect(tr, 0, e, 0);
        g.connect(e, 0, out, 0);
        let mut eng = Engine::default();
        let x0 = eng.eval_node(&g, grid, 0, 1, "").map_err(|x| x.1).unwrap().pos(0)[0];
        assert!((eng.eval(&g, 1, "").map_err(|x| x.1).unwrap().pos(0)[0] - x0 - 3.0).abs() < 1e-5);
        crate::nodeflow::make_group(&mut g, &[b, tr, e].into_iter().collect(), "Décaler").unwrap();
        assert!((eng.eval(&g, 1, "").map_err(|x| x.1).unwrap().pos(0)[0] - x0 - 3.0).abs() < 1e-5);
        // Une géométrie ne se branche pas sur un nombre : le node de couleur n'a pas de géométrie à donner
        let noise = g.add("noise", [0.0, 500.0]);
        g.links.retain(|l| l.to != out);
        g.links.push(Link { from: noise, out: 0, to: out, inp: 0 });
        assert!(eng.eval(&g, 1, "").is_err());
    }
}
