//! Tissu en « Vertex Block Descent » (Chen, Macklin, Kim, Yuksel 2024), sur carte graphique, avec
//! sa version processeur qui sert de repli et de référence.
//!
//! Le pas de temps implicite est une minimisation : pour chaque sommet, on cherche la position qui
//! équilibre son inertie et les forces de ses voisins. VBD le fait un sommet à la fois, par un pas
//! de Newton sur un bloc 3×3, les voisins étant figés. Deux sommets sans lien entre eux peuvent
//! être traités en même temps : on colorie donc les sommets au chargement, et chaque couleur est
//! un lancement sur la carte.
//!
//! Énergies :
//! - membrane : un ressort par arête (raideur en N/m). Les diagonales d'un carré sont reconnues
//!   au chargement et bien plus souples : un tissu tissé résiste peu en biais, c'est ce qui lui
//!   permet d'épouser une forme ronde (à raideur égale, il se comporte comme du papier) ;
//! - flexion : pour chaque sommet et chaque paire d'arêtes opposées (i au milieu de j et k),
//!   ½·D·|(1−t)·xj + t·xk − xi|². Nulle quand les trois points sont alignés, quadratique, donc sa
//!   dérivée seconde est constante : c'est la moins chère des flexions pour ce solveur ;
//! - auto-collision (en option) : deux sommets qui ne sont pas voisins dans le tissu se
//!   repoussent en dessous d'une épaisseur (voir « auto-collision »).
//!
//! Ce qui garde la mémoire sobre (c'est elle qui borne la vitesse), et les lancements rares (ce
//! sont eux qui coûtent sur un petit tissu) :
//! - les sommets sont rangés par couleur, puis le long d'une courbe de Morton : un lancement lit
//!   une plage contiguë, et ses voisins sont proches en mémoire ;
//! - le voisinage est une seule liste par sommet (rassemblement, aucune écriture concurrente), de
//!   12 octets par entrée : le voisin, le sommet qui prolonge l'arête au-delà de lui (pour la
//!   flexion), et les deux longueurs au repos sur 16 bits ;
//! - seuls les ressorts comptent pour colorier : les deux bouts d'un terme de flexion, ou deux
//!   sommets qui se repoussent, peuvent avoir la même couleur, et se lisent alors l'un l'autre
//!   dans les positions du début de la passe. Une grille avec diagonales a quatre couleurs ;
//! - la dernière passe d'un sous-pas écrit aussi la vitesse, et la prédiction du sous-pas suivant
//!   fait l'amortissement interne : aucun lancement n'est fait rien que pour les vitesses ;
//! - les positions sont relatives au centre du tissu : en f32, un tissu posé à 100 m de l'origine
//!   garde la précision d'un tissu posé à l'origine ;
//! - la relecture rend positions et vitesses dans l'ordre de la géométrie, 24 octets par sommet.
//! (Un rangement par grappes de 64 sommets traitées en mémoire partagée a été essayé : moins
//! d'octets lus, mais deux fois plus lent sur une vraie carte. Il a été retiré.)
//!
//! Raideur à petit budget : une correction grossière multi-niveaux au début de chaque sous-pas
//! (voir « niveaux grossiers »), qui donne en quelques lancements la raideur qu'il faudrait des
//! centaines de passes pour obtenir sommet par sommet. Sans elle, accélération de Tchebychev
//! entre les passes (sauf sur les sommets au contact, où elle est instable).
//!
//! Les forces de l'image (gravité, vent) arrivent comme un changement de vitesse, réparti sur les
//! sous-pas. Obstacles : sol, boule, boîte, résolus dans le bloc du sommet, avec un frottement de
//! Coulomb.

use crate::gpusim::{self, Bind, Column, Gpu, Kernels};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use rayon::prelude::*;

/// Masse surfacique prise quand la géométrie ne donne pas de masse (kg/m², un coton léger)
pub const DENSITY: f32 = 0.2;

/// Rayon spectral supposé pour l'accélération de Tchebychev
pub const RHO: f32 = 0.9;

/// Réglages d'un pas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// Raideur des arêtes, en N/m
    pub stiffness: f32,
    /// Raideur des diagonales, en part de celle des arêtes : c'est la résistance au cisaillement.
    /// Un tissu tissé se déforme bien plus facilement en biais que dans le sens des fils ; c'est
    /// ce qui lui permet d'épouser une forme ronde. À 1, il se comporte comme du papier.
    pub shear: f32,
    /// Raideur de flexion, en N·m
    pub bending: f32,
    /// Part de vitesse perdue par image (0 : aucune) : le frein de l'air, en gros
    pub damping: f32,
    /// Amortissement interne, de 0 à 1 : à chaque sous-pas, la vitesse de chaque sommet se
    /// rapproche d'autant de celle de ses voisins. Les plis cessent de frémir, sans que le tissu
    /// tombe ou se balance moins vite (un mouvement d'ensemble n'est pas freiné).
    pub calm: f32,
    /// Passes sur tous les sommets, par image (réparties entre les sous-pas)
    pub iterations: usize,
    pub substeps: usize,
    /// Accélération de Tchebychev : rayon spectral supposé (0 : sans)
    pub rho: f32,
    pub floor: Option<f32>,
    /// Boule : centre, rayon
    pub ball: Option<([f32; 3], f32)>,
    /// Boîte : centre, demi-tailles
    pub cube: Option<([f32; 3], [f32; 3])>,
    /// Frottement au contact des obstacles (coefficient de Coulomb : 0 glisse, 0,4 un coton sur du bois, 1 accroche fort)
    pub friction: f32,
    /// Auto-collision : distance minimale gardée entre deux sommets du tissu qui ne sont pas
    /// voisins, en mètres (0 : choisie d'après la taille des mailles). `None` : le tissu se traverse.
    pub thickness: Option<f32>,
    /// Niveaux grossiers utilisés (0 : aucun, le VBD seul). Voir `Cycle`.
    pub levels: usize,
    pub cycle: Cycle,
}

/// La correction grossière faite au début de chaque sous-pas, avant les passes du niveau fin :
/// un cycle en V, une passe de Jacobi sur chaque niveau en descendant, une en remontant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cycle {
    /// Passes de plus sur le niveau le plus grossier
    pub bottom: usize,
}

impl Default for Cycle {
    fn default() -> Self {
        Cycle { bottom: 8 }
    }
}

impl Default for Params {
    fn default() -> Self {
        Params { stiffness: 1000.0, shear: 0.1, bending: 1e-4, damping: 0.02, calm: 0.3, iterations: 16, substeps: 8, rho: RHO, floor: None, ball: None, cube: None, friction: 0.4, thickness: None, levels: 0, cycle: Cycle::default() }
    }
}

impl Params {
    /// Sous-pas et itérations par sous-pas réellement faits
    pub fn split(&self) -> (usize, usize) {
        let sub = self.substeps.clamp(1, 256);
        (sub, self.iterations.max(1).div_ceil(sub).min(MAX_PER))
    }

    /// Les mêmes réglages avec la correction grossière à chaque sous-pas, pour à peu près le même
    /// temps de calcul : une correction coûte deux à trois passes, qui sont retirées des passes de
    /// chaque sous-pas (il en reste toujours au moins une, qui lisse la correction).
    pub fn with_levels(mut self) -> Params {
        let (sub, per) = self.split();
        self.levels = MAX_LEVELS;
        self.iterations = sub * per.saturating_sub(2).max(1);
        self
    }

    /// Rayon spectral réellement pris pour l'accélération. Avec peu d'itérations par sous-pas,
    /// l'extrapolation n'a pas le temps de se stabiliser et le tissu explose : on la bride, et on
    /// s'en passe en dessous de quatre itérations.
    pub fn rho_used(&self) -> f32 {
        let per = self.split().1;
        // Avec des niveaux grossiers non plus : la correction d'ensemble fait ce travail, et les deux se gênent
        if per < 4 || self.rho <= 0.0 || self.levels > 0 { 0.0 } else { self.rho.min(1.0 - 2.4 / per as f32).clamp(0.0, 0.98) }
    }
}

/// Les réglages tels que les noyaux les lisent (mêmes champs sur la carte et sur le processeur).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Consts {
    h: f32,
    inv_h2: f32,
    k: f32,
    d: f32,
    keep: f32,
    mu: f32,
    floor_y: f32,
    floor_on: f32,
    /// Centre, rayon (0 : pas de boule)
    ball: [f32; 4],
    /// Centre, puis 1 si la boîte existe
    cube_c: [f32; 4],
    cube_h: [f32; 4],
    /// Longueur au repos = code² × cette échelle (voir `Topo::rest`)
    rest_scale: f32,
    /// Part de l'impulsion de l'image appliquée à chaque sous-pas (0 : pas d'impulsion)
    kick: f32,
    /// Raideur des diagonales, en part de celle des arêtes
    shear: f32,
    /// Amortissement interne
    calm: f32,
    /// Centre du tissu : les positions des noyaux lui sont relatives, la relecture le rajoute
    origin: [f32; 4],
    /// Auto-collision : distance minimale entre deux sommets (0 : pas d'auto-collision), distance
    /// jusqu'à laquelle on cherche les voisins (c'est aussi le côté d'une case de la grille),
    /// raideur de la répulsion, et masque du numéro de case
    thick: f32,
    reach: f32,
    kc: f32,
    mask: u32,
}

impl Consts {
    fn new(p: &Params, dt: f32, origin: [f32; 3], t: &Topo) -> Consts {
        let rest_scale = t.rest_scale;
        let (sub, _) = p.split();
        let h = dt / sub as f32;
        let rel = |c: [f32; 3]| [c[0] - origin[0], c[1] - origin[1], c[2] - origin[2]];
        let mut c = Consts { h, inv_h2: 1.0 / (h * h), k: p.stiffness.max(0.0), d: p.bending.max(0.0), keep: (1.0 - p.damping).clamp(0.0, 1.0).powf(1.0 / sub as f32), mu: p.friction.max(0.0), shear: p.shear.clamp(0.0, 1.0), calm: p.calm.clamp(0.0, 1.0), rest_scale, origin: [origin[0], origin[1], origin[2], 0.0], ..Default::default() };
        if let Some(d) = p.thickness {
            c.thick = if d > 0.0 { d } else { AUTO_THICK * t.spacing };
            c.reach = c.thick * REACH;
            c.kc = c.k;
            c.mask = t.cells() as u32 - 1;
        }
        if let Some(y) = p.floor {
            c.floor_y = y - origin[1];
            c.floor_on = 1.0;
        }
        if let Some((ctr, r)) = p.ball.filter(|b| b.1 > 0.0) {
            let q = rel(ctr);
            c.ball = [q[0], q[1], q[2], r];
        }
        if let Some((ctr, half)) = p.cube.filter(|b| b.1.iter().all(|h| *h > 0.0)) {
            let q = rel(ctr);
            c.cube_c = [q[0], q[1], q[2], 1.0];
            c.cube_h = [half[0], half[1], half[2], 0.0];
        }
        c
    }
}

/// Poids de Tchebychev de l'itération `k` (à partir de 1), connaissant le précédent.
fn omega(k: usize, prev: f32, rho: f32) -> f32 {
    match k {
        0 | 1 => 1.0,
        2 => 2.0 / (2.0 - rho * rho),
        _ => 4.0 / (4.0 - rho * rho * prev),
    }
}

// ---------- topologie ----------

/// Niveaux grossiers construits au plus : au-delà, le dernier a déjà moins de quelques dizaines de sommets
pub const MAX_LEVELS: usize = 10;

/// Auto-collision : voisins gardés par sommet (les plus proches)
pub const NEAR: usize = 8;
/// Épaisseur prise quand le réglage est à zéro, en part de la maille. Il en faut au moins 0,71 :
/// en dessous, un sommet passe au milieu d'un carré de quatre sommets sans en toucher aucun.
pub const AUTO_THICK: f32 = 0.9;
/// On cherche les voisins jusqu'à cette part de l'épaisseur : la marge couvre ce dont deux sommets se rapprochent en un sous-pas
const REACH: f32 = 1.5;
/// Deux sommets plus proches au repos que cette part de l'épaisseur ne se repoussent jamais : ce sont des voisins dans le tissu
const REST_APART: f32 = 2.1;
/// Sommets lus au plus dans une case de la grille (au-delà, le tissu est en boule dans un coin : le résultat dépend alors de l'ordre de rangement)
const BUCKET: u32 = 128;
/// Cases par bloc, pour la somme des occupations (un fil par bloc sur la carte)
const BLOCK: usize = 256;

/// Marque d'une entrée de ressort : elle forme un terme de flexion avec la suivante
const PAIR: u32 = 1 << 31;
/// Marque d'une entrée de ressort : c'est une diagonale (elle ne résiste qu'au cisaillement)
const DIAG: u32 = 1 << 30;
/// Le rang du voisin, sans les marques
const RANK: u32 = !(PAIR | DIAG);
const NONE: u32 = u32::MAX;
/// Couleur d'un sommet fixe
const FIXED: u32 = 255;

/// Tout ce qui ne dépend que du maillage au repos : ordre des sommets, couleurs, voisinages.
/// Construit une fois au chargement ; les mêmes tableaux servent au processeur et à la carte.
pub struct Topo {
    pub n: usize,
    /// Rang d'origine du sommet rangé en `k`
    pub perm: Vec<u32>,
    /// Rang de rangement du sommet d'origine `i`
    pub inv: Vec<u32>,
    /// Plages traitées l'une après l'autre, une par couleur : début, nombre. Les sommets fixes sont après la dernière.
    pub groups: Vec<[u32; 2]>,
    /// Nombre de couleurs de sommets
    pub phases: u32,
    /// Par sommet : début de ses ressorts, puis leur nombre (16 bits) et sa couleur (à partir du bit 16 ; 255 : fixe)
    pub off: Vec<[u32; 2]>,
    /// Ressort vers le voisin j : son rang (bit 31 : forme un terme de flexion avec l'entrée
    /// suivante, le sommet au milieu), le sommet k qui prolonge l'arête au-delà de j (terme de
    /// flexion dont j est le milieu ; `NONE` s'il n'y en a pas), et les longueurs au repos de
    /// i–j et de j–k, sur 16 bits chacune (voir `Topo::rest`).
    pub springs: Vec<[u32; 3]>,
    /// Longueur au repos = code² × cette échelle
    pub rest_scale: f32,
    /// Masse (0 : sommet fixe)
    pub mass: Vec<f32>,
    /// Nombre de termes de flexion
    pub trips: usize,
    /// Le même tissu en plus grossier, et de quoi passer de l'un à l'autre
    pub coarse: Option<Coarse>,
    /// Taille en dessous de laquelle les niveaux grossiers sont traités d'un seul lancement (`TAIL`, sauf pour les essais)
    pub tail: usize,
    /// Taille moyenne d'une maille (la plus courte arête de chaque sommet)
    pub spacing: f32,
    /// Positions au repos, dans l'ordre de rangement, relatives à leur centre (auto-collision : deux sommets proches au repos ne se repoussent pas)
    pub rest_pos: Vec<[f32; 4]>,
}

/// Les niveaux grossiers, rangés bout à bout dans les mêmes tableaux : le niveau 0 a environ un
/// sommet sur quatre du tissu, le niveau 1 un sur quatre du niveau 0, et ainsi de suite. Les
/// sommets non gardés suivent le déplacement de leurs voisins gardés. Ces niveaux servent à
/// corriger d'un coup les déformations d'ensemble, que les passes sommet par sommet ne propagent
/// que d'une maille à la fois. La matrice d'un niveau est celle du niveau d'au-dessus vue à
/// travers ces déplacements (Galerkine) ; elle ne dépend que du maillage au repos, et se calcule
/// une fois (voir « niveaux grossiers »).
#[derive(Default)]
pub struct Coarse {
    /// Par niveau : premier sommet, nombre
    pub levels: Vec<[u32; 2]>,
    /// Par sommet : parts de sa diagonale (masse, étirement, flexion : bits de f32, à multiplier
    /// par 1/h², la raideur, la raideur de flexion), début et nombre de ses voisins, début et
    /// nombre de ses enfants, une case libre
    pub vert: Vec<[u32; 8]>,
    /// Voisin, puis les trois mêmes parts hors diagonale (bits de f32)
    pub ent: Vec<[u32; 4]>,
    /// Enfants : les sommets d'au-dessus qui suivent ce sommet (pour le niveau 0, des sommets du tissu), et leur poids (bits de f32)
    pub kids: Vec<[u32; 2]>,
    /// Par sommet : jusqu'à quatre parents dans le niveau d'en dessous et leurs poids (bits de f32). Tout à zéro au dernier niveau.
    pub par: Vec<[u32; 8]>,
    /// Par sommet du tissu : ses parents dans le niveau 0
    pub up: Vec<[u32; 8]>,
}

/// En dessous de ce nombre de sommets, un niveau de plus ne sert à rien
const COARSEST: usize = 4;

/// Poids des passes de Jacobi sur les niveaux grossiers. En dessous de 1, une passe fait toujours
/// baisser l'énergie (la diagonale de ces matrices pèse au moins autant que le reste de la ligne).
const JACOBI: f32 = 0.8;

/// Jusqu'à cette taille, un niveau et tous les suivants sont traités d'un seul lancement sur la
/// carte, par un seul groupe de fils : à cette taille, ce qui coûte est le lancement, pas le calcul.
pub const TAIL: usize = 2048;

/// Un niveau tout juste construit : ses sommets, les parents de chaque sommet d'au-dessus, sa matrice par lignes
struct Built {
    n: usize,
    up: Vec<[u32; 8]>,
    starts: Vec<u32>,
    cols: Vec<u32>,
    vals: Vec<[f32; 3]>,
}

/// Construit le niveau grossier d'un niveau de `n` sommets. `row(i, put)` donne la ligne `i` de sa
/// matrice : `put(j, [masse, étirement, flexion])`, diagonale comprise, plusieurs fois le même `j`
/// si besoin. `free(i)` : faux pour un sommet fixe. `order` : ordre de visite, qui doit suivre le
/// tissu de proche en proche.
fn coarsen(n: usize, row: &dyn Fn(usize, &mut dyn FnMut(u32, [f32; 3])), free: &dyn Fn(usize) -> bool, order: &mut dyn Iterator<Item = u32>) -> Option<Built> {
    if n < COARSEST * 2 {
        return None;
    }
    // La matrice d'au-dessus, lignes triées et sans doublons
    let mut starts = vec![0u32; n + 1];
    let mut cols: Vec<u32> = Vec::new();
    let mut vals: Vec<[f32; 3]> = Vec::new();
    let mut line: Vec<(u32, [f32; 3])> = Vec::new();
    let merge = |line: &mut Vec<(u32, [f32; 3])>, cols: &mut Vec<u32>, vals: &mut Vec<[f32; 3]>| {
        line.sort_unstable_by_key(|x| x.0);
        let first = cols.len();
        for (j, v) in line.drain(..) {
            if cols.len() > first && cols.last() == Some(&j) {
                let last = vals.last_mut().unwrap();
                for c in 0..3 {
                    last[c] += v[c];
                }
            } else {
                cols.push(j);
                vals.push(v);
            }
        }
    };
    for i in 0..n {
        if free(i) {
            row(i, &mut |j, v| line.push((j, v)));
            merge(&mut line, &mut cols, &mut vals);
        }
        starts[i + 1] = cols.len() as u32;
    }
    let at = |i: usize| starts[i] as usize..starts[i + 1] as usize;
    // Sommets gardés : aucun n'est lié à un autre, et tout sommet libre en a un pour voisin
    let mut kept = vec![NONE; n];
    let mut taken = vec![false; n];
    let mut cn = 0u32;
    for i in order {
        let i = i as usize;
        if free(i) && !taken[i] {
            kept[i] = cn;
            cn += 1;
            for e in at(i) {
                if vals[e][1] < 0.0 {
                    taken[cols[e] as usize] = true;
                }
            }
            taken[i] = true;
        }
    }
    let cn = cn as usize;
    if cn < COARSEST || cn * 10 > n * 7 {
        return None;
    }
    // Chaque sommet suit ses voisins gardés, à la mesure du lien qui les unit (les quatre plus
    // forts). Près d'un bord fixe, la somme des poids est inférieure à 1 : il suit aussi le bord, qui ne bouge pas.
    let mut up = vec![[0u32; 8]; n];
    let mut near: Vec<(f32, u32)> = Vec::new();
    for i in 0..n {
        if !free(i) {
            continue;
        }
        if kept[i] != NONE {
            up[i] = [kept[i], 0, 0, 0, 1.0f32.to_bits(), 0, 0, 0];
            continue;
        }
        near.clear();
        let (mut all, mut own) = (0.0f32, 0.0f32);
        for e in at(i) {
            let j = cols[e] as usize;
            if j == i {
                own = vals[e][1];
            } else {
                all += (-vals[e][1]).max(0.0);
                if kept[j] != NONE && vals[e][1] < 0.0 {
                    near.push((-vals[e][1], kept[j]));
                }
            }
        }
        near.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        near.truncate(4);
        let strong: f32 = near.iter().map(|x| x.0).sum();
        if strong <= 0.0 || own <= 0.0 {
            continue;
        }
        let scale = (all / own).min(1.0) / strong;
        for (k, x) in near.iter().enumerate() {
            up[i][k] = x.1;
            up[i][4 + k] = (x.0 * scale).to_bits();
        }
    }
    let parents = |i: usize| {
        let u = up[i];
        (0..4).map(move |k| (u[k], f32::from_bits(u[4 + k]))).filter(|p| p.1 > 0.0)
    };
    let (down_off, down) = csr(cn, (0..n).flat_map(|i| parents(i).map(move |(a, w)| (a, [i as u32, w.to_bits()]))).collect::<Vec<_>>().into_iter());
    // Matrice de ce niveau : Pᵀ·A·P, ligne par ligne
    let mut cstart = vec![0u32; cn + 1];
    let (mut ccols, mut cvals) = (Vec::new(), Vec::new());
    for a in 0..cn {
        for d in &down[down_off[a] as usize..down_off[a + 1] as usize] {
            let (i, wi) = (d[0] as usize, f32::from_bits(d[1]));
            for e in at(i) {
                for (b, wj) in parents(cols[e] as usize) {
                    line.push((b, [wi * wj * vals[e][0], wi * wj * vals[e][1], wi * wj * vals[e][2]]));
                }
            }
        }
        merge(&mut line, &mut ccols, &mut cvals);
        cstart[a + 1] = ccols.len() as u32;
    }
    Some(Built { n: cn, up, starts: cstart, cols: ccols, vals: cvals })
}

impl Coarse {
    /// Construit jusqu'à `max` niveaux au-dessous d'un tissu de `n` sommets (mêmes arguments que `coarsen`).
    fn build(n: usize, row: &dyn Fn(usize, &mut dyn FnMut(u32, [f32; 3])), free: &dyn Fn(usize) -> bool, order: &[u32], max: usize) -> Option<Coarse> {
        if max == 0 {
            return None;
        }
        let mut built = coarsen(n, row, free, &mut order.iter().copied())?;
        let mut co = Coarse { up: std::mem::take(&mut built.up), ..Default::default() };
        loop {
            let base = co.vert.len() as u32;
            // Enfants : les sommets d'au-dessus, par parent
            let above: &[[u32; 8]] = match co.levels.last() {
                None => &co.up,
                Some(l) => &co.par[l[0] as usize..][..l[1] as usize],
            };
            let shift = co.levels.last().map_or(0, |l| l[0]);
            let (kid_off, kids) = csr(built.n, above.iter().enumerate().flat_map(|(i, u)| (0..4).filter(|k| f32::from_bits(u[4 + k]) > 0.0).map(move |k| (u[k] - base, [i as u32 + shift, u[4 + k]]))).collect::<Vec<_>>().into_iter());
            for a in 0..built.n {
                let first = co.ent.len() as u32;
                let mut own = [0.0f32; 3];
                for e in built.starts[a] as usize..built.starts[a + 1] as usize {
                    let v = built.vals[e];
                    if built.cols[e] as usize == a {
                        own = v;
                    } else {
                        co.ent.push([built.cols[e] + base, v[0].to_bits(), v[1].to_bits(), v[2].to_bits()]);
                    }
                }
                co.vert.push([own[0].to_bits(), own[1].to_bits(), own[2].to_bits(), first, co.ent.len() as u32 - first, co.kids.len() as u32 + kid_off[a], kid_off[a + 1] - kid_off[a], 0]);
            }
            co.kids.extend(kids);
            co.levels.push([base, built.n as u32]);
            co.par.resize(co.vert.len(), [0; 8]);
            if co.levels.len() >= max {
                break;
            }
            let next = {
                let row = |a: usize, put: &mut dyn FnMut(u32, [f32; 3])| {
                    for e in built.starts[a] as usize..built.starts[a + 1] as usize {
                        put(built.cols[e], built.vals[e]);
                    }
                };
                coarsen(built.n, &row, &|_| true, &mut (0..built.n as u32))
            };
            let Some(next) = next else { break };
            // Les parents des sommets de ce niveau sont dans le suivant, qui commencera juste après
            let after = co.vert.len() as u32;
            for (a, u) in next.up.iter().enumerate() {
                let mut u = *u;
                for k in 0..4 {
                    if f32::from_bits(u[4 + k]) > 0.0 {
                        u[k] += after;
                    }
                }
                co.par[base as usize + a] = u;
            }
            built = next;
        }
        Some(co)
    }

    /// Octets que ces niveaux gardent sur la carte
    fn gpu_bytes(&self) -> u64 {
        (self.vert.len() * (32 + 32 + 64) + self.ent.len() * 16 + self.kids.len() * 8 + self.up.len() * 32) as u64
    }

    /// Octets lus et écrits par une correction sur les `levels` premiers niveaux, `bottom` passes
    /// de plus sur le dernier : par niveau, la descente (force réunie, première passe), la force
    /// restante, la remontée, la dernière passe.
    fn cycle_bytes(&self, levels: usize, bottom: usize) -> u64 {
        let used = levels.min(self.levels.len());
        let mut total = 0u64;
        for (l, lv) in self.levels[..used].iter().enumerate() {
            let verts = &self.vert[lv[0] as usize..][..lv[1] as usize];
            let (n, ents, kids) = (lv[1] as u64, verts.iter().map(|v| v[4] as u64).sum::<u64>(), verts.iter().map(|v| v[6] as u64).sum::<u64>());
            let sweep = n * (32 + 16 + 16 + 16) + ents * (16 + 16);
            let last = l + 1 == used;
            total += n * (32 + 32) + kids * (8 + 16) + sweep * if last { 1 + 2 * bottom as u64 } else { 2 };
            if !last {
                total += n * (32 + 32) + n * 4 * 16;
            }
        }
        total
    }
}

fn part1by2(x: u32) -> u32 {
    let mut x = x & 0x3ff;
    x = (x | (x << 16)) & 0x030000ff;
    x = (x | (x << 8)) & 0x0300f00f;
    x = (x | (x << 4)) & 0x030c30c3;
    (x | (x << 2)) & 0x09249249
}

/// Liste de voisins par sommet à partir de paires orientées : (débuts, voisins).
fn csr<T: Copy + Default>(n: usize, pairs: impl Iterator<Item = (u32, T)> + Clone) -> (Vec<u32>, Vec<T>) {
    let mut start = vec![0u32; n + 1];
    for (a, _) in pairs.clone() {
        start[a as usize + 1] += 1;
    }
    for i in 0..n {
        start[i + 1] += start[i];
    }
    let mut cur = start.clone();
    let mut out = vec![T::default(); start[n] as usize];
    for (a, v) in pairs {
        out[cur[a as usize] as usize] = v;
        cur[a as usize] += 1;
    }
    (start, out)
}

/// Plus petite couleur libre parmi celles déjà prises (au plus 128 couleurs)
fn free_color(used: u128) -> u32 {
    (!used).trailing_zeros().min(127)
}

impl Topo {
    /// `pos` : positions au repos. `rest` : longueur au repos de chaque arête. `mass` : masse par
    /// sommet si la géométrie en donne une, sinon elle est déduite de la taille des mailles.
    pub fn build(n: usize, edges: &[[u32; 2]], rest: &[f32], pos: &[Vec<f32>; 3], pinned: &[bool], mass: Option<&[f32]>) -> Topo {
        Topo::build_with(n, edges, rest, pos, pinned, mass, true).with_levels(MAX_LEVELS)
    }

    /// Ajoute jusqu'à `levels` niveaux grossiers, chacun construit sur le précédent.
    pub fn with_levels(mut self, levels: usize) -> Topo {
        self.coarse = None;
        if levels == 0 {
            return self;
        }
        // La matrice du niveau fin : masse, un ressort de raideur 1 par arête, et pour la flexion
        // deux ressorts par terme, du milieu vers chaque bout (voir « niveaux grossiers »)
        let row = |i: usize, put: &mut dyn FnMut(u32, [f32; 3])| {
            let o = self.off[i];
            let mut own = [self.mass[i], 0.0, 0.0];
            let mut before: Option<(usize, f32)> = None;
            let list = &self.springs[o[0] as usize..][..(o[1] & 0xffff) as usize];
            let mut bend = vec![0.0f32; list.len()];
            for (e, sp) in list.iter().enumerate() {
                let lj = self.rest(sp[2] & 0xffff);
                if sp[1] != NONE {
                    let lk = self.rest(sp[2] >> 16);
                    let sc = (lj + lk) / (std::f32::consts::SQRT_2 * lj * lk);
                    bend[e] += sc * sc * lk / (lj + lk);
                }
                if let Some((ep, lp)) = before.take() {
                    let sc = (lp + lj) / (std::f32::consts::SQRT_2 * lp * lj);
                    bend[ep] += sc * sc * lj / (lp + lj);
                    bend[e] += sc * sc * lp / (lp + lj);
                } else if sp[0] & PAIR != 0 {
                    before = Some((e, lj));
                }
            }
            for (e, sp) in list.iter().enumerate() {
                own[1] += 1.0;
                own[2] += bend[e];
                let j = sp[0] & RANK;
                if self.mass[j as usize] > 0.0 {
                    put(j, [0.0, -1.0, -bend[e]]);
                }
            }
            put(i as u32, own);
        };
        self.coarse = Coarse::build(self.n, &row, &|i| self.mass[i] > 0.0, &self.inv, levels);
        self
    }

    /// Nombre de niveaux grossiers construits
    pub fn levels(&self) -> usize {
        self.coarse.as_ref().map_or(0, |c| c.levels.len())
    }

    /// Même chose sans niveaux grossiers, en choisissant s'il faut des termes de flexion.
    pub fn build_with(n: usize, edges: &[[u32; 2]], rest: &[f32], pos: &[Vec<f32>; 3], pinned: &[bool], mass: Option<&[f32]>, bend: bool) -> Topo {
        let at = |i: u32| [pos[0][i as usize], pos[1][i as usize], pos[2][i as usize]];
        let ok = |e: &[u32; 2]| (e[0] as usize) < n && (e[1] as usize) < n && e[0] != e[1];
        // Longueurs au repos sur 16 bits : code = √(longueur / la plus grande) × 65535. Les deux
        // bouts d'une arête lisent le même code, donc la même longueur : rien ne dérive. Sur un
        // maillage régulier l'erreur est de l'ordre de 0,002 % de la longueur.
        let longest = edges.iter().zip(rest).filter(|(e, _)| ok(e)).map(|(_, r)| *r).fold(0.0f32, f32::max).max(1e-30);
        let rest_scale = longest / (65535.0f32 * 65535.0);
        let code = |r: f32| ((r.max(0.0) / longest).sqrt() * 65535.0).round().clamp(1.0, 65535.0) as u32;
        // Demi-arêtes autour de chaque sommet : voisin, code de la longueur, numéro de l'arête
        let around = edges.iter().zip(rest).enumerate().filter(|(_, (e, _))| ok(e)).flat_map(|(id, (e, r))| [(e[0], (e[1], code(*r), id as u32)), (e[1], (e[0], code(*r), id as u32))]);
        let (nb_start, nb) = csr(n, around);
        let ring = |i: usize| nb_start[i] as usize..nb_start[i + 1] as usize;
        // Diagonales : dans un carré qui a ses deux diagonales, les quatre sommets sont tous liés
        // entre eux. Des trois façons d'apparier ces six arêtes en paires opposées, les diagonales
        // sont la paire la plus longue (dans un quadrilatère convexe, toujours).
        let mut diagonal = vec![false; edges.len()];
        {
            let length = |a: u32, b: u32| ring(a as usize).find(|s| nb[*s].0 == b).map(|s| (nb[s].1, nb[s].2));
            let mut common: Vec<(u32, u32)> = Vec::new();
            for (id, e) in edges.iter().enumerate().filter(|(_, e)| ok(e)) {
                let (a, b) = (e[0], e[1]);
                let Some((ab, _)) = length(a, b) else { continue };
                // Voisins communs de a et de b, avec leurs distances à a et à b
                common.clear();
                for s in ring(a as usize) {
                    if let Some((cb, _)) = length(nb[s].0, b).filter(|_| nb[s].0 != b) {
                        common.push((nb[s].0, nb[s].1 + cb));
                    }
                }
                for x in 0..common.len() {
                    for y in x + 1..common.len() {
                        let (c, d) = (common[x].0, common[y].0);
                        let Some((cd, _)) = length(c, d) else { continue };
                        // Les codes croissent avec les longueurs : comparer leurs carrés revient à comparer les longueurs
                        let len = |code: u32| (code as f32) * (code as f32);
                        let (Some((ac, _)), Some((bd, _)), Some((ad, _)), Some((bc, _))) = (length(a, c), length(b, d), length(a, d), length(b, c)) else { continue };
                        let own = len(ab) + len(cd);
                        if own > (len(ac) + len(bd)) * 1.05 && own > (len(ad) + len(bc)) * 1.05 {
                            diagonal[id] = true;
                        }
                    }
                }
            }
        }
        // Flexion : autour de chaque sommet, les arêtes sont appariées deux à deux quand elles se
        // prolongent (i au milieu de j et k), les mieux alignées d'abord. Une arête ne sert qu'à
        // un terme par bout : le voisinage tient alors tout entier dans la liste des ressorts.
        let mut partner = vec![NONE; nb.len()];
        let mut trips = 0;
        if bend {
            let mut cand: Vec<(f32, u32, u32)> = Vec::new();
            for i in 0..n {
                let (r, pi) = (ring(i), at(i as u32));
                cand.clear();
                for a in r.clone() {
                    for b in a + 1..r.end {
                        let (j, k) = (nb[a].0, nb[b].0);
                        if j == k {
                            continue;
                        }
                        let (pj, pk) = (at(j), at(k));
                        let (dj, dk) = ([pj[0] - pi[0], pj[1] - pi[1], pj[2] - pi[2]], [pk[0] - pi[0], pk[1] - pi[1], pk[2] - pi[2]]);
                        let dot = dj[0] * dk[0] + dj[1] * dk[1] + dj[2] * dk[2];
                        let len = ((dj[0] * dj[0] + dj[1] * dj[1] + dj[2] * dj[2]) * (dk[0] * dk[0] + dk[1] * dk[1] + dk[2] * dk[2])).sqrt();
                        if len > 1e-18 && dot / len < -0.95 {
                            cand.push((dot / len, a as u32, b as u32));
                        }
                    }
                }
                cand.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
                for &(_, a, b) in &cand {
                    if partner[a as usize] == NONE && partner[b as usize] == NONE {
                        partner[a as usize] = b;
                        partner[b as usize] = a;
                        trips += 1;
                    }
                }
            }
        }
        // La demi-arête inverse de chacune (j vers i pour i vers j) : c'est par elle qu'on trouve
        // le sommet qui prolonge l'arête de l'autre côté de j
        let mut twin = vec![NONE; nb.len()];
        {
            let mut first = vec![NONE; edges.len()];
            for (s, h) in nb.iter().enumerate() {
                let f = &mut first[h.2 as usize];
                if *f == NONE {
                    *f = s as u32;
                } else {
                    twin[s] = *f;
                    twin[*f as usize] = s as u32;
                }
            }
        }
        // Couleurs : deux sommets liés par un ressort n'ont jamais la même. Les deux bouts d'un
        // terme de flexion peuvent l'avoir : chacun lit alors de l'autre sa position du début de
        // la passe (voir `forces`), ce qui divise par deux le nombre de couleurs d'une grille.
        let mut color = vec![FIXED; n];
        let mut phases = 0u32;
        for i in 0..n {
            if pinned.get(i).copied().unwrap_or(false) {
                continue;
            }
            let mut used = 0u128;
            for s in ring(i) {
                let j = nb[s].0 as usize;
                if color[j] < 128 {
                    used |= 1 << color[j];
                }
            }
            color[i] = free_color(used);
            phases = phases.max(color[i] + 1);
        }
        // Courbe de Morton : les voisins dans l'espace sont voisins en mémoire
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for c in 0..3 {
            for &x in &pos[c][..n] {
                lo[c] = lo[c].min(x);
                hi[c] = hi[c].max(x);
            }
        }
        let cell = |x: f32, c: usize| if hi[c] > lo[c] { ((x - lo[c]) / (hi[c] - lo[c]) * 1023.0) as u32 } else { 0 };
        let morton = |i: u32| {
            let p = at(i);
            part1by2(cell(p[0], 0)) | part1by2(cell(p[1], 1)) << 1 | part1by2(cell(p[2], 2)) << 2
        };
        let mut order: Vec<(u32, u32, u32)> = (0..n as u32).map(|i| (color[i as usize], morton(i), i)).collect();
        order.sort_unstable();
        let mut groups = vec![[0u32; 2]; phases as usize];
        for (k, o) in order.iter().enumerate() {
            if o.0 != FIXED {
                let g = &mut groups[o.0 as usize];
                if g[1] == 0 {
                    g[0] = k as u32;
                }
                g[1] += 1;
            }
        }
        let perm: Vec<u32> = order.iter().map(|o| o.2).collect();
        let mut inv = vec![0u32; n];
        for (k, &i) in perm.iter().enumerate() {
            inv[i as usize] = k as u32;
        }
        // Voisinages, dans l'ordre de rangement : les entrées appariées d'abord, côte à côte
        let mut off = Vec::with_capacity(n);
        let mut springs = Vec::with_capacity(nb.len());
        let mut m = Vec::with_capacity(n);
        let (mut spacing, mut spaced) = (0.0f64, 0usize);
        let mid = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
        let rest_pos: Vec<[f32; 4]> = perm.iter().map(|&i| at(i)).map(|q| [q[0] - mid[0], q[1] - mid[1], q[2] - mid[2], 0.0]).collect();
        for &i in &perm {
            let i = i as usize;
            let start = springs.len();
            let mut shortest = f32::MAX;
            let mut push = |s: usize, flag: u32| {
                let (j, cj, id) = nb[s];
                let flag = flag | if diagonal[id as usize] { DIAG } else { 0 };
                // Au-delà de j : l'arête appariée à la nôtre autour de j
                let far = if twin[s] != NONE { partner[twin[s] as usize] } else { NONE };
                let (k, ck) = if far != NONE { (inv[nb[far as usize].0 as usize], nb[far as usize].1) } else { (NONE, 0) };
                springs.push([inv[j as usize] | flag, k, cj | ck << 16]);
                shortest = shortest.min(cj as f32 * cj as f32 * rest_scale);
            };
            for s in ring(i) {
                if partner[s] != NONE && (s as u32) < partner[s] {
                    push(s, PAIR);
                    push(partner[s] as usize, 0);
                }
            }
            for s in ring(i) {
                if partner[s] == NONE {
                    push(s, 0);
                }
            }
            // Au-delà de 65 535 ressorts autour d'un sommet, les suivants sont ignorés
            let count = (springs.len() - start).min(0xffff);
            springs.truncate(start + count);
            off.push([start as u32, count as u32 | color[i] << 16]);
            if shortest < f32::MAX {
                spacing += shortest as f64;
                spaced += 1;
            }
            m.push(if color[i] == FIXED {
                0.0
            } else {
                match mass {
                    Some(mm) => mm[i].max(1e-9),
                    None if shortest < f32::MAX => (DENSITY * shortest * shortest).max(1e-9),
                    None => 1.0,
                }
            });
        }
        Topo { n, perm, inv, groups, phases, off, springs, rest_scale, mass: m, trips, coarse: None, tail: TAIL, spacing: if spaced > 0 { (spacing / spaced as f64) as f32 } else { 0.0 }, rest_pos }
    }

    /// Longueur au repos d'un code de 16 bits (le même calcul que dans les noyaux de la carte)
    #[inline]
    fn rest(&self, code: u32) -> f32 {
        let u = code as f32;
        u * u * self.rest_scale
    }

    /// Cases de la grille d'auto-collision : une puissance de deux, au moins deux par sommet
    pub fn cells(&self) -> usize {
        (2 * self.n).next_power_of_two().max(1024)
    }

    /// Octets lus et écrits de plus par sous-pas par l'auto-collision : la grille (compter,
    /// ranger), la recherche des voisins dans 27 cases, puis leurs positions à chaque passe.
    pub fn self_bytes(&self, per: usize) -> u64 {
        let (n, cells) = (self.n as u64, self.cells() as u64);
        // Environ deux sommets et demi par case occupée, neuf cases occupées sur 27 pour un tissu à plat
        let search = n * (16 + 27 * 8 + 9 * 5 * 16 / 2 + NEAR as u64 * 4 + 16);
        n * (16 + 8) + cells * 12 + n * (16 + 8 + 16) + search + per as u64 * n * (NEAR as u64 * 4 + 4 * 16)
    }

    /// Nombre de couleurs de sommets
    pub fn colors(&self) -> usize {
        self.phases as usize
    }

    /// Octets que la carte garde pour ce tissu
    pub fn gpu_bytes(&self) -> u64 {
        (self.n * (16 * 7 + 4 + 24) + self.off.len() * 8 + self.springs.len() * 12) as u64 + self.coarse.as_ref().map_or(0, |c| self.n as u64 * 16 + c.gpu_bytes())
    }

    /// Octets lus et écrits par passe complète sur tous les sommets (pour estimer le débit).
    /// `bending` : la flexion est-elle active ? `accel` : l'accélération de Tchebychev aussi ?
    /// Un sommet au contact d'un obstacle lit 16 octets de plus (sa position d'avant).
    pub fn bytes_per_pass(&self, bending: bool, accel: bool) -> u64 {
        // Par sommet : ses débuts, sa cible, sa position lue et écrite ; avec la flexion, sa
        // position écrite une fois de plus pour la passe suivante (l'accélération s'en charge sinon)
        let mut total = self.n as u64 * (8 + 16 + 32 + if bending && !accel { 16 } else { 0 });
        for o in &self.off {
            if o[1] >> 16 == FIXED {
                continue;
            }
            for sp in &self.springs[o[0] as usize..][..(o[1] & 0xffff) as usize] {
                // L'entrée, la position du voisin, et celle du sommet au-delà s'il y a un terme de flexion
                total += 12 + 16 + 16 * (bending && sp[1] != NONE) as u64;
            }
        }
        total + if accel { self.n as u64 * 64 } else { 0 }
    }

    /// Octets lus et écrits par une correction grossière (tout ce qui s'ajoute aux passes du
    /// niveau fin) : la force restante, qui relit tout le voisinage de chaque sommet, puis les
    /// étapes des niveaux grossiers, puis la remontée vers le tissu.
    pub fn correction_bytes(&self, bending: bool, cy: &Cycle, levels: usize) -> u64 {
        let Some(co) = self.coarse.as_ref().filter(|_| levels > 0) else { return 0 };
        let n = self.n as u64;
        let around: u64 = self.off.iter().filter(|o| o[1] >> 16 != FIXED).map(|o| self.springs[o[0] as usize..][..(o[1] & 0xffff) as usize].iter().map(|sp| 12 + 16 * (1 + (bending && sp[1] != NONE) as u64)).sum::<u64>()).sum();
        n * (8 + 16 + 16 + 16) + around + co.cycle_bytes(levels, cy.bottom) + n * (16 + 32 + 32 + 4 * 16)
    }

    /// Coût d'une correction grossière, en passes du niveau fin
    pub fn correction_cost(&self, bending: bool, cy: &Cycle, levels: usize) -> f64 {
        self.correction_bytes(bending, cy, levels) as f64 / self.bytes_per_pass(bending, false).max(1) as f64
    }

    /// Octets lus et écrits de plus par l'amortissement interne d'un sous-pas : la vitesse de chaque voisin
    pub fn calm_bytes(&self) -> u64 {
        (self.n * 8 + self.springs.len() * (12 + 16)) as u64
    }

    /// Ce que coûte vraiment un sous-pas avec ces réglages, en passes du niveau fin : ses passes,
    /// plus l'amortissement interne et la correction grossière s'ils sont actifs.
    pub fn substep_cost(&self, p: &Params) -> f64 {
        let bending = p.bending > 0.0;
        let mut bytes = p.split().1 as u64 * self.bytes_per_pass(bending, p.rho_used() > 0.0);
        if p.calm > 0.0 {
            bytes += self.calm_bytes();
        }
        if p.thickness.is_some() {
            bytes += self.self_bytes(p.split().1);
        }
        bytes += self.correction_bytes(bending, &p.cycle, p.levels.min(self.levels()));
        bytes as f64 / self.bytes_per_pass(bending, false).max(1) as f64
    }

    /// Ce que le même tissu lisait avant le rangement compact (ressorts de 8 octets, termes de
    /// flexion de 16 octets rangés à part, position d'avant toujours lue) : la référence du banc.
    pub fn bytes_per_pass_before(&self) -> u64 {
        (self.n * (16 + 16 + 16 + 32) + self.springs.len() * (8 + 16) + self.trips * 3 * (16 + 32) + self.n * 64) as u64
    }
}

// ---------- noyaux, version processeur ----------

type V4 = [f32; 4];

/// Sort le point `x` d'un obstacle : `n` est la normale sortante, `depth` l'enfoncement. Puis le
/// frottement de Coulomb, sur les déplacements : l'enfoncement mesure la force d'appui, et le
/// frottement retient un glissement d'au plus µ fois cet enfoncement depuis le début du sous-pas.
/// En dessous, le sommet reste où il était (il colle) ; au-dessus, il glisse d'autant moins.
/// C'est ce qui laisse un tissu glisser d'une pente raide et tenir sur une pente douce.
/// (La loi exacte du bloc, qui divise par la raideur en travers, a été essayée : avec peu de
/// passes, les forces des voisins pas encore à l'équilibre la débordent, et le tissu rampe.)
#[inline]
fn push_out(x: [f32; 3], n: [f32; 3], depth: f32, pv: [f32; 3], mu: f32) -> [f32; 3] {
    let mut q = [x[0] + n[0] * depth, x[1] + n[1] * depth, x[2] + n[2] * depth];
    let mv = [q[0] - pv[0], q[1] - pv[1], q[2] - pv[2]];
    let along = mv[0] * n[0] + mv[1] * n[1] + mv[2] * n[2];
    let tan = [mv[0] - along * n[0], mv[1] - along * n[1], mv[2] - along * n[2]];
    let len = (tan[0] * tan[0] + tan[1] * tan[1] + tan[2] * tan[2]).sqrt();
    let held = mu * depth;
    let back = if len <= held { 1.0 } else { held / len };
    for k in 0..3 {
        q[k] -= tan[k] * back;
    }
    q
}

/// Position d'un sommet après obstacles, et 1 s'il en touche un. `pv` : sa position au début du sous-pas.
fn collide(mut x: [f32; 3], pv: [f32; 3], c: &Consts, mu: f32) -> V4 {
    let mut hit = 0.0;
    if c.floor_on > 0.5 && x[1] < c.floor_y {
        x = push_out(x, [0.0, 1.0, 0.0], c.floor_y - x[1], pv, mu);
        x[1] = c.floor_y;
        hit = 1.0;
    }
    if c.ball[3] > 0.0 {
        let (ctr, r) = ([c.ball[0], c.ball[1], c.ball[2]], c.ball[3]);
        let d = [x[0] - ctr[0], x[1] - ctr[1], x[2] - ctr[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if len < r && len > 1e-9 {
            x = push_out(x, [d[0] / len, d[1] / len, d[2] / len], r - len, pv, mu);
            // Le frottement a déplacé le point le long du plan tangent : on le repose sur la boule
            let d = [x[0] - ctr[0], x[1] - ctr[1], x[2] - ctr[2]];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if len > 1e-9 {
                for k in 0..3 {
                    x[k] = ctr[k] + d[k] / len * r;
                }
            }
            hit = 1.0;
        }
    }
    if c.cube_c[3] > 0.5 {
        let q = [x[0] - c.cube_c[0], x[1] - c.cube_c[1], x[2] - c.cube_c[2]];
        let pen = [c.cube_h[0] - q[0].abs(), c.cube_h[1] - q[1].abs(), c.cube_h[2] - q[2].abs()];
        if pen[0] > 0.0 && pen[1] > 0.0 && pen[2] > 0.0 {
            // Sorti par la face la plus proche
            let axis = if pen[0] <= pen[1] && pen[0] <= pen[2] {
                0
            } else if pen[1] <= pen[2] {
                1
            } else {
                2
            };
            let mut n = [0.0; 3];
            n[axis] = if q[axis] < 0.0 { -1.0 } else { 1.0 };
            x = push_out(x, n, pen[axis], pv, mu);
            x[axis] = c.cube_c[axis] + n[axis] * c.cube_h[axis];
            hit = 1.0;
        }
    }
    [x[0], x[1], x[2], hit]
}

/// Normale sortante de l'obstacle que touche le point `x` (à un dixième de millimètre près), s'il en touche un.
#[inline]
fn normal_at(x: [f32; 3], c: &Consts) -> Option<[f32; 3]> {
    const NEAR: f32 = 1e-4;
    if c.ball[3] > 0.0 {
        let d = [x[0] - c.ball[0], x[1] - c.ball[1], x[2] - c.ball[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if len < c.ball[3] + NEAR && len > 1e-9 {
            return Some([d[0] / len, d[1] / len, d[2] / len]);
        }
    }
    if c.cube_c[3] > 0.5 {
        let q = [x[0] - c.cube_c[0], x[1] - c.cube_c[1], x[2] - c.cube_c[2]];
        let pen = [c.cube_h[0] - q[0].abs(), c.cube_h[1] - q[1].abs(), c.cube_h[2] - q[2].abs()];
        if pen[0] > -NEAR && pen[1] > -NEAR && pen[2] > -NEAR {
            let axis = if pen[0] <= pen[1] && pen[0] <= pen[2] {
                0
            } else if pen[1] <= pen[2] {
                1
            } else {
                2
            };
            let mut n = [0.0; 3];
            n[axis] = if q[axis] < 0.0 { -1.0 } else { 1.0 };
            return Some(n);
        }
    }
    (c.floor_on > 0.5 && x[1] < c.floor_y + NEAR).then_some([0.0, 1.0, 0.0])
}

/// Sort un point des obstacles, sans frottement : 1 en quatrième valeur s'il en touchait un.
#[inline]
fn keep_out(x: [f32; 3], c: &Consts) -> V4 {
    collide(x, x, c, 0.0)
}

/// Force sur un sommet et sa dérivée seconde (symétrique : diagonale, puis xy, xz, yz)
struct Force {
    f: [f32; 3],
    hd: [f32; 3],
    ho: [f32; 3],
    /// Le sommet est-il repoussé par un autre sommet du tissu ?
    touch: bool,
}

/// Inertie, ressorts et flexion du sommet `i` placé en `xi`, ses voisins étant figés. `at` lit la
/// position d'un voisin ; `old` celle qu'avait au début de la passe un sommet situé au-delà d'un
/// voisin (il peut être de la même couleur que `i`, donc en cours de calcul). `near` : les
/// sommets proches de `i` sans lui être liés (auto-collision), lus eux aussi au début de la passe.
#[inline]
fn forces(t: &Topo, i: usize, xi: [f32; 3], tgt: V4, c: &Consts, at: &impl Fn(u32) -> [f32; 3], old: &impl Fn(u32) -> [f32; 3], near: &[u32]) -> Force {
    let m = tgt[3] * c.inv_h2;
    let mut f = [-m * (xi[0] - tgt[0]), -m * (xi[1] - tgt[1]), -m * (xi[2] - tgt[2])];
    let mut hd = [m; 3];
    let mut ho = [0.0f32; 3];
    let o = t.off[i];
    // L'entrée d'avant, quand elle forme un terme de flexion avec celle-ci
    let mut before: Option<([f32; 3], f32)> = None;
    for sp in &t.springs[o[0] as usize..][..(o[1] & 0xffff) as usize] {
        let xj = at(sp[0] & RANK);
        let lj = t.rest(sp[2] & 0xffff);
        let d = [xi[0] - xj[0], xi[1] - xj[1], xi[2] - xj[2]];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if l > 1e-12 {
            let nrm = [d[0] / l, d[1] / l, d[2] / l];
            let k = if sp[0] & DIAG != 0 { c.k * c.shear } else { c.k };
            let pull = k * (l - lj);
            // Un ressort comprimé a une dérivée seconde négative en travers : on la coupe à zéro
            let iso = k * (1.0 - lj / l).max(0.0);
            let ani = k - iso;
            for k in 0..3 {
                f[k] -= pull * nrm[k];
                hd[k] += iso + ani * nrm[k] * nrm[k];
            }
            ho[0] += ani * nrm[0] * nrm[1];
            ho[1] += ani * nrm[0] * nrm[2];
            ho[2] += ani * nrm[1] * nrm[2];
        }
        if c.d > 0.0 {
            // Flexion : ½·D·s²·|w·x(bout) + (1−w)·x(autre bout) − x(milieu)|², nulle quand les
            // trois points sont alignés comme au repos. Chaque bout pèse la longueur de l'autre côté.
            if sp[1] != NONE {
                // Ce sommet est un bout, j le milieu, k l'autre bout
                let lk = t.rest(sp[2] >> 16);
                let xk = old(sp[1]);
                let sum = lj + lk;
                let s = sum / (std::f32::consts::SQRT_2 * lj * lk);
                let w = lk / sum;
                let g = c.d * s * s * w;
                for k in 0..3 {
                    f[k] -= g * (w * xi[k] + (1.0 - w) * xk[k] - xj[k]);
                    hd[k] += g * w;
                }
            }
            if let Some((xp, lp)) = before.take() {
                // Ce sommet est le milieu, entre le voisin d'avant et celui-ci
                let sum = lp + lj;
                let s = sum / (std::f32::consts::SQRT_2 * lp * lj);
                let g = c.d * s * s;
                for k in 0..3 {
                    f[k] += g * ((lj * xp[k] + lp * xj[k]) / sum - xi[k]);
                    hd[k] += g;
                }
            } else if sp[0] & PAIR != 0 {
                before = Some((xj, lj));
            }
        }
    }
    // Auto-collision : un ressort qui ne fait que repousser, en dessous de l'épaisseur. L'autre
    // sommet fait le même calcul de son côté pendant la même passe : chacun compte donc la
    // raideur deux fois, ce qui revient à faire la moitié du chemin chacun, sans dépassement.
    let mut touch = false;
    for &j in near {
        if j == NONE {
            break;
        }
        let xj = old(j);
        let d = [xi[0] - xj[0], xi[1] - xj[1], xi[2] - xj[2]];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if l < c.thick && l > 1e-9 {
            let nrm = [d[0] / l, d[1] / l, d[2] / l];
            let push = c.kc * (c.thick - l);
            for k in 0..3 {
                f[k] += push * nrm[k];
                hd[k] += 2.0 * c.kc * nrm[k] * nrm[k];
            }
            ho[0] += 2.0 * c.kc * nrm[0] * nrm[1];
            ho[1] += 2.0 * c.kc * nrm[0] * nrm[2];
            ho[2] += 2.0 * c.kc * nrm[1] * nrm[2];
            touch = true;
        }
    }
    Force { f, hd, ho, touch }
}

/// Pas de Newton sur le bloc 3×3 du sommet, mis à l'échelle pour rester loin des bornes du f32
#[inline]
fn newton(fo: &Force, xi: [f32; 3]) -> [f32; 3] {
    let (hd, ho) = (fo.hd, fo.ho);
    let s = 1.0 / (hd[0] + hd[1] + hd[2]);
    let (a, b, cc, d, e, g) = (hd[0] * s, ho[0] * s, ho[1] * s, hd[1] * s, ho[2] * s, hd[2] * s);
    let f = [fo.f[0] * s, fo.f[1] * s, fo.f[2] * s];
    let (c00, c01, c02) = (d * g - e * e, cc * e - b * g, b * e - cc * d);
    let (c11, c12, c22) = (a * g - cc * cc, b * cc - a * e, a * d - b * b);
    let det = a * c00 + b * c01 + cc * c02;
    let mut x = xi;
    if det > 1e-12 {
        let r = 1.0 / det;
        x[0] += r * (c00 * f[0] + c01 * f[1] + c02 * f[2]);
        x[1] += r * (c01 * f[0] + c11 * f[1] + c12 * f[2]);
        x[2] += r * (c02 * f[0] + c12 * f[1] + c22 * f[2]);
    }
    x
}

/// Une passe : tous les sommets libres mis à jour une fois, couleur après couleur. Dans une
/// couleur, les sommets sont indépendants : chacun ne lit, hors de lui-même, que des sommets
/// d'autres couleurs. Même suite d'opérations que sur la carte.
fn sweep(t: &Topo, pos: &mut [V4], tgt: &[V4], prev: &[V4], old: &[V4], near: &[u32], c: &Consts) {
    for g in &t.groups {
        let (start, count) = (g[0] as usize, g[1] as usize);
        let (lo, rest) = pos.split_at_mut(start);
        let (mid, hi) = rest.split_at_mut(count);
        let (lo, hi) = (&*lo, &*hi);
        // Un petit lot par fil au moins : sur un petit tissu, se partager le travail coûterait plus que le faire
        mid.par_iter_mut().with_min_len(256).enumerate().for_each(|(q, x)| {
            let i = start + q;
            if t.mass[i] <= 0.0 {
                return;
            }
            let at = |j: u32| {
                let j = j as usize;
                let v = if j < start { lo[j] } else { hi[j - start - count] };
                [v[0], v[1], v[2]]
            };
            let xi = [x[0], x[1], x[2]];
            let list = if near.is_empty() { &[][..] } else { &near[i * NEAR..][..NEAR] };
            let fo = forces(t, i, xi, tgt[i], c, &at, &|k: u32| [old[k as usize][0], old[k as usize][1], old[k as usize][2]], list);
            *x = collide(newton(&fo, xi), [prev[i][0], prev[i][1], prev[i][2]], c, c.mu);
            if fo.touch {
                x[3] = 1.0;
            }
        });
    }
}

// ---------- auto-collision ----------
//
// Le tissu est fait de points reliés par des fils : il n'a pas de faces. L'auto-collision se fait
// donc entre sommets : deux sommets qui ne sont pas voisins dans le tissu ne s'approchent pas à
// moins d'une épaisseur. Pour qu'une couche ne passe pas entre les sommets d'une autre, cette
// épaisseur doit être de l'ordre de la maille (voir `AUTO_THICK`).
//
// À chaque sous-pas, après la prédiction : les sommets sont rangés dans une grille de cases de la
// taille de la distance de recherche (comptage par case, somme des occupations, rangement), puis
// chaque sommet garde ses `NEAR` plus proches voisins parmi les 27 cases autour de lui. La liste
// est triée par numéro : le résultat ne dépend ni de l'ordre de rangement dans une case, ni de
// l'ordre des fils. Les passes lisent ensuite ces voisins dans les positions du début de la passe.

/// Numéro de la case d'un point
#[inline]
fn cell_of(x: [f32; 3], reach: f32) -> [i32; 3] {
    [(x[0] / reach).floor() as i32, (x[1] / reach).floor() as i32, (x[2] / reach).floor() as i32]
}

/// Case de la table pour un numéro de case de la grille
#[inline]
fn bucket(q: [i32; 3], mask: u32) -> u32 {
    ((q[0] as u32).wrapping_mul(73856093) ^ (q[1] as u32).wrapping_mul(19349663) ^ (q[2] as u32).wrapping_mul(83492791)) & mask
}

/// La grille : pour chaque case de la table, où commencent ses sommets dans `sorted` (position, rang).
#[derive(Default)]
struct Grid {
    starts: Vec<u32>,
    sorted: Vec<(V4, u32)>,
}

/// Range tous les sommets dans la grille. Même suite que sur la carte : compter, sommer, ranger.
fn grid_build(pos: &[V4], c: &Consts, g: &mut Grid) {
    let cells = c.mask as usize + 1;
    let of = |x: &V4| bucket(cell_of([x[0], x[1], x[2]], c.reach), c.mask) as usize;
    g.starts.clear();
    g.starts.resize(cells + 1, 0);
    for x in pos {
        g.starts[of(x) + 1] += 1;
    }
    for k in 0..cells {
        g.starts[k + 1] += g.starts[k];
    }
    let mut cursor = g.starts.clone();
    g.sorted.clear();
    g.sorted.resize(pos.len(), ([0.0; 4], 0));
    for (i, x) in pos.iter().enumerate() {
        let k = of(x);
        g.sorted[cursor[k] as usize] = (*x, i as u32);
        cursor[k] += 1;
    }
}

/// Les `NEAR` plus proches sommets de `i` à moins de la distance de recherche, qui ne sont pas
/// proches de lui au repos, triés par rang (`NONE` au bout).
fn grid_near(t: &Topo, i: usize, pos: &[V4], c: &Consts, g: &Grid) -> [u32; NEAR] {
    let mut out = [NONE; NEAR];
    if t.mass[i] <= 0.0 {
        return out;
    }
    let (xi, ri) = (pos[i], t.rest_pos[i]);
    let q = cell_of([xi[0], xi[1], xi[2]], c.reach);
    let apart = (REST_APART * c.thick) * (REST_APART * c.thick);
    let mut best = [(0.0f32, NONE); NEAR];
    let mut count = 0;
    let mut seen = [NONE; 27];
    for k in 0..27 {
        let b = bucket([q[0] + (k % 3) as i32 - 1, q[1] + (k / 3 % 3) as i32 - 1, q[2] + (k / 9) as i32 - 1], c.mask);
        // Deux cases de la grille peuvent tomber dans la même case de la table : on ne la lit qu'une fois
        seen[k] = b;
        if seen[..k].contains(&b) {
            continue;
        }
        let (from, to) = (g.starts[b as usize], g.starts[b as usize + 1]);
        for e in from..to.min(from + BUCKET) {
            let (xj, j) = g.sorted[e as usize];
            if j as usize == i {
                continue;
            }
            let d = [xi[0] - xj[0], xi[1] - xj[1], xi[2] - xj[2]];
            let d2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            if d2 >= c.reach * c.reach {
                continue;
            }
            let rj = t.rest_pos[j as usize];
            let r = [ri[0] - rj[0], ri[1] - rj[1], ri[2] - rj[2]];
            if r[0] * r[0] + r[1] * r[1] + r[2] * r[2] < apart {
                continue;
            }
            if count < NEAR {
                best[count] = (d2, j);
                count += 1;
            } else {
                // Le plus lointain des gardés cède sa place (à distance égale, le plus grand rang)
                let mut worst = 0;
                for a in 1..NEAR {
                    if best[a].0 > best[worst].0 || (best[a].0 == best[worst].0 && best[a].1 > best[worst].1) {
                        worst = a;
                    }
                }
                if d2 < best[worst].0 || (d2 == best[worst].0 && j < best[worst].1) {
                    best[worst] = (d2, j);
                }
            }
        }
    }
    // Triés par rang : l'ordre des calculs ne dépend plus de l'ordre de lecture
    for a in 0..count {
        out[a] = best[a].1;
    }
    out[..count].sort_unstable();
    out
}

// ---------- niveaux grossiers ----------
//
// Les passes sommet par sommet ne propagent une déformation que d'une maille à la fois : sur un
// tissu fin, un étirement d'ensemble met des centaines de passes à se résorber, et le tissu paraît
// mou, d'autant plus qu'il a de sommets. La correction grossière règle ce défaut. Au début de
// chaque sous-pas, la force qui reste sur chaque sommet est confiée à un tissu quatre fois moins
// dense, lui-même aidé par un tissu encore quatre fois moins dense, et ainsi de suite ; le
// déplacement d'ensemble trouvé est rendu au niveau fin, que ses passes n'ont plus qu'à lisser.
//
// Ce que résolvent les niveaux grossiers n'est pas l'énergie du tissu mais une énergie plus
// simple qui la borne par le dessus : chaque ressort y est remplacé par un ressort de même
// raideur dans toutes les directions (Liu et al. 2013, « Fast Simulation of Mass-Spring
// Systems » ; c'est aussi la matrice de Projective Dynamics, Bouaziz et al. 2014), et chaque terme
// de flexion par deux ressorts du milieu vers les bouts. Deux conséquences :
// - la matrice ne dépend que du maillage au repos. Celles des niveaux grossiers (Pᵀ·A·P, Galerkine)
//   sont calculées une fois au chargement, et les trois axes se résolvent séparément ;
// - minimiser une borne supérieure fait toujours baisser l'énergie vraie : la correction ne peut
//   pas faire diverger le tissu, même plié, comprimé ou en grande rotation. En échange, elle ne
//   corrige que l'étirement (ce qui manque aux passes) et laisse la flexion et les rotations au
//   niveau fin, qui les traite déjà bien.
// (Multigrille de Galerkine sur cette matrice : Xian, Tong, Liu 2019.)
//
// Les niveaux sont résolus par des passes de Jacobi, qui ne demandent pas de couleurs : une
// passe est un seul lancement sur la carte, et les petits niveaux se traitent tous ensemble dans
// un seul groupe de fils (voir `TAIL`).
//
// Contacts. La correction ne voit pas les obstacles. Un sommet fixe, ou qui vient d'entrer dans
// un obstacle à ce sous-pas, ne reçoit pas de correction ; ses voisins lui sont alors attachés
// comme à un point d'ancrage, ce qui garde la borne valable et arrête le tissu autour du point
// d'impact. Un sommet déjà posé sur un obstacle reçoit la correction, mais la part de sa force qui
// le pousse vers l'obstacle est retirée (l'obstacle la retient déjà) : il peut glisser le long de
// la surface pour relâcher un étirement. Après la correction, tout sommet déplacé est sorti des obstacles.

/// Force qui reste sur le sommet `i` du niveau fin. Quatrième valeur : −1 s'il ne doit pas être
/// déplacé (fixe, ou il vient d'entrer dans un obstacle), sinon la raideur qui l'attache à ceux de
/// ses voisins qui sont dans ce cas.
#[inline]
fn rest_force(t: &Topo, i: usize, pos: &[V4], tgt: &[V4], c: &Consts) -> V4 {
    let (x, y) = (pos[i], tgt[i]);
    if y[3] <= 0.0 || x[3] > 1.5 {
        return [0.0, 0.0, 0.0, -1.0];
    }
    let m = y[3] * c.inv_h2;
    let mut f = [-m * (x[0] - y[0]), -m * (x[1] - y[1]), -m * (x[2] - y[2])];
    let mut anchor = 0.0;
    let o = t.off[i];
    // L'entrée d'avant, quand elle forme un terme de flexion avec celle-ci : position, longueur, ancrage
    let mut before: Option<([f32; 3], f32, bool)> = None;
    for sp in &t.springs[o[0] as usize..][..(o[1] & 0xffff) as usize] {
        let pj = pos[(sp[0] & RANK) as usize];
        let (xj, held) = ([pj[0], pj[1], pj[2]], pj[3] > 1.5);
        let lj = t.rest(sp[2] & 0xffff);
        let d = [x[0] - xj[0], x[1] - xj[1], x[2] - xj[2]];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if l > 1e-12 {
            let k = if sp[0] & DIAG != 0 { c.k * c.shear } else { c.k };
            let pull = k * (l - lj) / l;
            for a in 0..3 {
                f[a] -= pull * d[a];
            }
        }
        if held {
            anchor += c.k;
        }
        if c.d > 0.0 {
            if sp[1] != NONE {
                let lk = t.rest(sp[2] >> 16);
                let pk = pos[sp[1] as usize];
                let sum = lj + lk;
                let s = sum / (std::f32::consts::SQRT_2 * lj * lk);
                let w = lk / sum;
                let g = c.d * s * s * w;
                for a in 0..3 {
                    f[a] -= g * (w * x[a] + (1.0 - w) * pk[a] - xj[a]);
                }
                if held {
                    anchor += g;
                }
            }
            if let Some((xp, lp, held_p)) = before.take() {
                let sum = lp + lj;
                let s = sum / (std::f32::consts::SQRT_2 * lp * lj);
                let g = c.d * s * s;
                for a in 0..3 {
                    f[a] += g * ((lj * xp[a] + lp * xj[a]) / sum - x[a]);
                }
                if held_p {
                    anchor += g * lj / sum;
                }
                if held {
                    anchor += g * lp / sum;
                }
            } else if sp[0] & PAIR != 0 {
                before = Some((xj, lj, held));
            }
        }
    }
    // Posé sur un obstacle : l'obstacle retient déjà ce qui pousse le sommet vers lui
    if x[3] > 0.5 {
        if let Some(n) = normal_at([x[0], x[1], x[2]], c) {
            let into = (f[0] * n[0] + f[1] * n[1] + f[2] * n[2]).min(0.0);
            for a in 0..3 {
                f[a] -= into * n[a];
            }
        }
    }
    [f[0], f[1], f[2], anchor]
}

/// L'état de travail des niveaux grossiers, tous bout à bout. Par sommet : la force à équilibrer
/// (et la raideur d'ancrage ajoutée à sa diagonale), son déplacement, sa force restante, son
/// déplacement après la dernière passe.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Work {
    f: V4,
    d: V4,
    r: V4,
    e: V4,
}

/// Ce qui reste de la force d'un sommet grossier une fois retirée la part de tous les déplacements, et sa diagonale
#[inline]
fn coarse_rest(co: &Coarse, a: usize, w: &[Work], c: &Consts) -> ([f32; 3], f32) {
    let mix = |m: u32, s: u32, b: u32| c.inv_h2 * f32::from_bits(m) + c.k * f32::from_bits(s) + c.d * f32::from_bits(b);
    let (v, f, d) = (co.vert[a], w[a].f, w[a].d);
    let own = mix(v[0], v[1], v[2]) + f[3];
    let mut r = [f[0] - own * d[0], f[1] - own * d[1], f[2] - own * d[2]];
    for e in &co.ent[v[3] as usize..][..v[4] as usize] {
        let (g, db) = (mix(e[1], e[2], e[3]), w[e[0] as usize].d);
        for k in 0..3 {
            r[k] -= g * db[k];
        }
    }
    (r, own)
}

/// Les noyaux d'un niveau grossier, dans l'ordre où un cycle les enchaîne. Chacun lit l'état de
/// travail tel qu'il était avant lui, et n'écrit que les sommets de son niveau : sur la carte, ce
/// sont les mêmes calculs, un sommet par fil.
#[derive(Clone, Copy, PartialEq)]
enum Stage {
    /// Descente : réunit la force restante (et l'ancrage) des enfants, puis fait la première passe (à partir d'un déplacement nul)
    Gather,
    /// Force restante, confiée au niveau suivant
    Remain,
    /// Une passe de plus sur place (à partir de la force restante)
    Apply,
    /// Remontée : ajoute le déplacement des parents
    Prolong,
    /// Dernière passe, rangée à part pour le niveau d'au-dessus
    Post,
}

/// Fait tourner un noyau sur le niveau `l`. `res` : force restante du niveau fin (lue par la descente du niveau 0).
fn stage(co: &Coarse, l: usize, what: Stage, w: &mut [Work], res: &[V4], c: &Consts) {
    let (start, count) = (co.levels[l][0] as usize, co.levels[l][1] as usize);
    // Chaque sommet est calculé d'après l'état d'avant, puis tout le niveau est écrit
    let old = &*w;
    let new: Vec<Work> = (start..start + count).into_par_iter().with_min_len(256).map(|a| {
        let mut x = old[a];
        match what {
            Stage::Gather => {
                let v = co.vert[a];
                let mut sum = [0.0f32; 4];
                for kid in &co.kids[v[5] as usize..][..v[6] as usize] {
                    let (wt, r) = (f32::from_bits(kid[1]), if l == 0 { res[kid[0] as usize] } else { old[kid[0] as usize].r });
                    if r[3] >= 0.0 {
                        for k in 0..4 {
                            sum[k] += wt * r[k];
                        }
                    }
                }
                x.f = sum;
                let own = c.inv_h2 * f32::from_bits(v[0]) + c.k * f32::from_bits(v[1]) + c.d * f32::from_bits(v[2]) + sum[3];
                x.d = if own > 0.0 { [JACOBI * sum[0] / own, JACOBI * sum[1] / own, JACOBI * sum[2] / own, 0.0] } else { [0.0; 4] };
            }
            Stage::Remain => {
                let (r, _) = coarse_rest(co, a, old, c);
                x.r = [r[0], r[1], r[2], x.f[3]];
            }
            Stage::Apply => {
                let own = c.inv_h2 * f32::from_bits(co.vert[a][0]) + c.k * f32::from_bits(co.vert[a][1]) + c.d * f32::from_bits(co.vert[a][2]) + x.f[3];
                if own > 0.0 {
                    for k in 0..3 {
                        x.d[k] += JACOBI * x.r[k] / own;
                    }
                }
            }
            Stage::Prolong => {
                let u = co.par[a];
                for k in 0..4 {
                    let wt = f32::from_bits(u[4 + k]);
                    if wt > 0.0 {
                        for q in 0..3 {
                            x.d[q] += wt * old[u[k] as usize].e[q];
                        }
                    }
                }
            }
            Stage::Post => {
                let (r, own) = coarse_rest(co, a, old, c);
                x.e = x.d;
                if own > 0.0 {
                    for k in 0..3 {
                        x.e[k] += JACOBI * r[k] / own;
                    }
                }
            }
        }
        x
    }).collect();
    w[start..start + count].copy_from_slice(&new);
}

/// Le cycle en V sur les `levels` premiers niveaux, la force restante du niveau fin étant dans `res`.
fn cycle(co: &Coarse, w: &mut [Work], res: &[V4], c: &Consts, cy: &Cycle, levels: usize) {
    let last = levels - 1;
    for l in 0..=last {
        stage(co, l, Stage::Gather, w, res, c);
        if l < last {
            stage(co, l, Stage::Remain, w, res, c);
        }
    }
    for _ in 0..cy.bottom {
        stage(co, last, Stage::Remain, w, res, c);
        stage(co, last, Stage::Apply, w, res, c);
    }
    stage(co, last, Stage::Post, w, res, c);
    for l in (0..last).rev() {
        stage(co, l, Stage::Prolong, w, res, c);
        stage(co, l, Stage::Post, w, res, c);
    }
}

/// La correction grossière d'un sous-pas : force restante du niveau fin, cycle, déplacement rendu
/// aux sommets libres, qui sont ensuite sortis des obstacles. `res` : colonne de travail du niveau fin.
fn correct(t: &Topo, pos: &mut [V4], tgt: &[V4], res: &mut Vec<V4>, c: &Consts, cy: &Cycle, levels: usize, work: &mut Vec<Work>, near: &[u32]) {
    let Some(co) = t.coarse.as_ref().filter(|_| levels > 0) else { return };
    let levels = levels.min(co.levels.len());
    res.resize(t.n, [0.0; 4]);
    work.resize(co.vert.len(), Work::default());
    {
        let pos = &*pos;
        res.par_iter_mut().with_min_len(256).enumerate().for_each(|(i, r)| *r = rest_force(t, i, pos, tgt, c));
    }
    cycle(co, work, res, c, cy, levels);
    let (work, res) = (&*work, &*res);
    pos.par_iter_mut().with_min_len(256).enumerate().for_each(|(i, x)| {
        if res[i][3] >= 0.0 {
            let u = co.up[i];
            let mut d = [0.0f32; 3];
            for k in 0..4 {
                let wt = f32::from_bits(u[4 + k]);
                if wt > 0.0 {
                    for a in 0..3 {
                        d[a] += wt * work[u[k] as usize].e[a];
                    }
                }
            }
            // Près d'une autre couche du tissu, la correction (qui ne la voit pas) ne déplace pas
            // le sommet de plus d'un quart d'épaisseur : deux couches ne peuvent pas se croiser
            if !near.is_empty() && near[i * NEAR] != NONE {
                let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                let most = 0.25 * c.thick;
                if len > most {
                    for a in 0..3 {
                        d[a] *= most / len;
                    }
                }
            }
            *x = keep_out([x[0] + d[0], x[1] + d[1], x[2] + d[2]], c);
        }
    });
}

/// Vitesse du sommet `i` au début d'un sous-pas, après l'amortissement interne : elle se rapproche
/// de la moyenne de celles de ses voisins (un sommet fixe a une vitesse nulle).
#[inline]
fn calmed(t: &Topo, i: usize, vel: &[V4], c: &Consts) -> [f32; 3] {
    let own = vel[i];
    let o = t.off[i];
    let list = &t.springs[o[0] as usize..][..(o[1] & 0xffff) as usize];
    if c.calm <= 0.0 || list.is_empty() {
        return [own[0], own[1], own[2]];
    }
    let mut sum = [0.0f32; 3];
    for sp in list {
        let other = vel[(sp[0] & RANK) as usize];
        for a in 0..3 {
            sum[a] += other[a] - own[a];
        }
    }
    let part = c.calm / list.len() as f32;
    [own[0] + part * sum[0], own[1] + part * sum[1], own[2] + part * sum[2]]
}

/// Un pas d'image sur le processeur. `pos` et `vel` sont dans l'ordre de rangement, positions
/// relatives à l'origine du tissu. `kick` : changement de vitesse demandé de l'extérieur pour
/// cette image (gravité, vent), appliqué par parts égales à chaque sous-pas.
fn step_cpu(t: &Topo, pos: &mut [V4], vel: &mut [V4], kick: Option<&[V4]>, c: &Consts, p: &Params) {
    let n = t.n;
    let (sub, per) = p.split();
    let rho = p.rho_used();
    let mut tgt: Vec<V4> = t.mass.iter().map(|m| [0.0, 0.0, 0.0, *m]).collect();
    let mut prev = vec![[0.0f32; 4]; n];
    let mut s = [vec![[0.0f32; 4]; n], vec![[0.0f32; 4]; n]];
    // Les positions du début de la passe : ce que lisent les termes de flexion au-delà d'un voisin, et l'auto-collision
    let mut old: Vec<V4> = Vec::new();
    let mut grid = Grid::default();
    let mut near: Vec<u32> = Vec::new();
    let levels = p.levels.min(t.levels());
    let mut work = Vec::new();
    let mut res = Vec::new();
    let zero = [[0.0f32; 4]];
    let part = c.kick;
    for _ in 0..sub {
        // Prédiction : où l'inertie seule mènerait chaque sommet (sa vitesse d'abord rapprochée de
        // celle de ses voisins : l'amortissement interne), sorti des obstacles s'il y entre.
        // Un sommet libre garde aussi la marque « touchait un obstacle » de sa dernière passe : la
        // correction grossière ne le déplace pas, et y attache ses voisins.
        let kicks = kick.map_or(rayon::iter::Either::Left(rayon::iter::repeat_n(&zero[0], n)), |k| rayon::iter::Either::Right(k.par_iter()));
        let before = &*vel;
        (pos.par_iter_mut(), tgt.par_iter_mut(), prev.par_iter_mut(), s[0].par_iter_mut(), kicks).into_par_iter().with_min_len(1024).enumerate().for_each(|(i, (x, y, pv, s0, dv))| {
            let m = y[3];
            *pv = [x[0], x[1], x[2], 0.0];
            if m > 0.0 {
                let v = calmed(t, i, before, c);
                let q = [x[0] + c.h * (v[0] + part * dv[0]), x[1] + c.h * (v[1] + part * dv[1]), x[2] + c.h * (v[2] + part * dv[2])];
                *y = [q[0], q[1], q[2], m];
                // Marque : 1 s'il touchait déjà un obstacle à sa dernière passe, 3 s'il vient d'y entrer
                let out = keep_out(q, c);
                *x = [out[0], out[1], out[2], if x[3] > 0.5 { 1.0 } else { 3.0 * out[3] }];
            } else {
                *y = [x[0], x[1], x[2], m];
                x[3] = 2.0;
            }
            *s0 = [x[0], x[1], x[2], 0.0];
        });
        if c.thick > 0.0 {
            grid_build(pos, c, &mut grid);
            near.resize(n * NEAR, NONE);
            let (pos, grid) = (&*pos, &grid);
            near.par_chunks_mut(NEAR).with_min_len(64).enumerate().for_each(|(i, list)| list.copy_from_slice(&grid_near(t, i, pos, c, grid)));
        }
        if levels > 0 {
            correct(t, pos, &tgt, &mut res, c, &p.cycle, levels, &mut work, &near);
        }
        let mut w = 1.0;
        for k in 1..=per {
            if c.d > 0.0 || c.thick > 0.0 {
                old.clear();
                old.extend_from_slice(pos);
            }
            sweep(t, pos, &tgt, &prev, &old, &near, c);
            if rho > 0.0 {
                w = omega(k, w, rho);
                pos.par_iter_mut().zip(s[k % 2].par_iter_mut()).with_min_len(1024).for_each(|(x, o)| {
                    if x[3] < 0.5 {
                        for a in 0..3 {
                            x[a] = o[a] + w * (x[a] - o[a]);
                        }
                    }
                    *o = [x[0], x[1], x[2], 0.0];
                });
            }
        }
        (pos.par_iter(), vel.par_iter_mut(), prev.par_iter(), tgt.par_iter()).into_par_iter().with_min_len(1024).for_each(|(x, v, pv, y)| {
            *v = if y[3] > 0.0 { [(x[0] - pv[0]) / c.h * c.keep, (x[1] - pv[1]) / c.h * c.keep, (x[2] - pv[2]) / c.h * c.keep, 0.0] } else { [0.0; 4] };
        });
    }
}

// ---------- noyaux, version carte ----------

const STRUCTS: &str = r#"
struct C {
    h: f32, inv_h2: f32, k: f32, d: f32,
    keep: f32, mu: f32, floor_y: f32, floor_on: f32,
    ball: vec4<f32>,
    cube_c: vec4<f32>,
    cube_h: vec4<f32>,
    rest_scale: f32, kick: f32, shear: f32, calm: f32,
    origin: vec4<f32>,
    thick: f32, reach: f32, kc: f32, mask: u32,
}
struct S { start: u32, count: u32, omega: f32, arg: u32 }
struct E { j: u32, k: u32, codes: u32 }
"#;

/// Obstacles : les mêmes calculs que `push_out`, `collide`, `keep_out` et `normal_at`. Lit les réglages `c`.
const COLLIDE: &str = r#"
fn push_out(x: vec3<f32>, n: vec3<f32>, depth: f32, pv: vec3<f32>, mu: f32) -> vec3<f32> {
    let q = x + n * depth;
    let mv = q - pv;
    let tan = mv - dot(mv, n) * n;
    let len = length(tan);
    let held = mu * depth;
    var back = 1.0;
    if (len > held) { back = held / len; }
    return q - tan * back;
}

// Le point est-il dans un obstacle ?
fn inside(x: vec3<f32>) -> bool {
    let under = c.floor_on > 0.5 && x.y < c.floor_y;
    let in_ball = c.ball.w > 0.0 && distance(x, c.ball.xyz) < c.ball.w;
    let in_cube = c.cube_c.w > 0.5 && all(abs(x - c.cube_c.xyz) < c.cube_h.xyz);
    return under || in_ball || in_cube;
}

fn collide(x0: vec3<f32>, pv: vec3<f32>, mu: f32) -> vec4<f32> {
    var x = x0;
    var hit = 0.0;
    if (c.floor_on > 0.5 && x.y < c.floor_y) {
        x = push_out(x, vec3<f32>(0.0, 1.0, 0.0), c.floor_y - x.y, pv, mu);
        x.y = c.floor_y;
        hit = 1.0;
    }
    if (c.ball.w > 0.0) {
        let ctr = c.ball.xyz;
        let r = c.ball.w;
        let d = x - ctr;
        let len = length(d);
        if (len < r && len > 1e-9) {
            x = push_out(x, d / len, r - len, pv, mu);
            let d2 = x - ctr;
            let len2 = length(d2);
            if (len2 > 1e-9) { x = ctr + d2 / len2 * r; }
            hit = 1.0;
        }
    }
    if (c.cube_c.w > 0.5) {
        let q = x - c.cube_c.xyz;
        let pen = c.cube_h.xyz - abs(q);
        if (pen.x > 0.0 && pen.y > 0.0 && pen.z > 0.0) {
            let sg = select(vec3<f32>(1.0), vec3<f32>(-1.0), q < vec3<f32>(0.0));
            if (pen.x <= pen.y && pen.x <= pen.z) {
                x = push_out(x, vec3<f32>(sg.x, 0.0, 0.0), pen.x, pv, mu);
                x.x = c.cube_c.x + sg.x * c.cube_h.x;
            } else if (pen.y <= pen.z) {
                x = push_out(x, vec3<f32>(0.0, sg.y, 0.0), pen.y, pv, mu);
                x.y = c.cube_c.y + sg.y * c.cube_h.y;
            } else {
                x = push_out(x, vec3<f32>(0.0, 0.0, sg.z), pen.z, pv, mu);
                x.z = c.cube_c.z + sg.z * c.cube_h.z;
            }
            hit = 1.0;
        }
    }
    return vec4<f32>(x, hit);
}

fn keep_out(x: vec3<f32>) -> vec4<f32> {
    if (!inside(x)) { return vec4<f32>(x, 0.0); }
    return collide(x, x, 0.0);
}

// Normale sortante de l'obstacle touché (quatrième valeur : 1 s'il y en a un)
fn normal_at(x: vec3<f32>) -> vec4<f32> {
    let near = 1e-4;
    if (c.ball.w > 0.0) {
        let d = x - c.ball.xyz;
        let len = length(d);
        if (len < c.ball.w + near && len > 1e-9) { return vec4<f32>(d / len, 1.0); }
    }
    if (c.cube_c.w > 0.5) {
        let q = x - c.cube_c.xyz;
        let pen = c.cube_h.xyz - abs(q);
        if (pen.x > -near && pen.y > -near && pen.z > -near) {
            let sg = select(vec3<f32>(1.0), vec3<f32>(-1.0), q < vec3<f32>(0.0));
            if (pen.x <= pen.y && pen.x <= pen.z) { return vec4<f32>(sg.x, 0.0, 0.0, 1.0); }
            if (pen.y <= pen.z) { return vec4<f32>(0.0, sg.y, 0.0, 1.0); }
            return vec4<f32>(0.0, 0.0, sg.z, 1.0);
        }
    }
    if (c.floor_on > 0.5 && x.y < c.floor_y + near) { return vec4<f32>(0.0, 1.0, 0.0, 1.0); }
    return vec4<f32>(0.0);
}
"#;

/// Le pas de Newton par sommet, une couleur à la fois : les mêmes calculs que `forces`, `newton` et `sweep`.
const SOLVE: &str = r#"
@group(0) @binding(0) var<uniform> c: C;
@group(0) @binding(1) var<uniform> s: S;
@group(0) @binding(2) var<storage, read_write> pos: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> tgt: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> prev: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read> off: array<vec2<u32>>;
@group(0) @binding(6) var<storage, read> springs: array<E>;
// Les positions du début de la passe, lues et écrites dans deux tampons qui s'échangent à chaque passe
@group(0) @binding(7) var<storage, read> old: array<vec4<f32>>;
@group(0) @binding(8) var<storage, read_write> next: array<vec4<f32>>;
@group(0) @binding(9) var<storage, read_write> vel: array<vec4<f32>>;
//SELF-BIND

struct F { f: vec3<f32>, hd: vec3<f32>, ho: vec3<f32>, touch: f32 }

fn rest_of(code: u32) -> f32 { let u = f32(code); return u * u * c.rest_scale; }

fn forces(i: u32, xi: vec3<f32>) -> F {
    let ty = tgt[i];
    let m = ty.w * c.inv_h2;
    var f = -m * (xi - ty.xyz);
    var hd = vec3<f32>(m);
    var ho = vec3<f32>(0.0);
    let o = off[i];
    let count = o.y & 0xffffu;
    var xp = vec3<f32>(0.0);
    var lp = 0.0;
    var paired = false;
    for (var e = 0u; e < count; e++) {
        let sp = springs[o.x + e];
        let xj = pos[sp.j & 0x3fffffffu].xyz;
        let lj = rest_of(sp.codes & 0xffffu);
        let d = xi - xj;
        let l = length(d);
        if (l > 1e-12) {
            let nrm = d / l;
            var k = c.k;
            if ((sp.j & 0x40000000u) != 0u) { k = c.k * c.shear; }
            let iso = k * max(1.0 - lj / l, 0.0);
            let ani = k - iso;
            f -= k * (l - lj) * nrm;
            hd += vec3<f32>(iso) + ani * nrm * nrm;
            ho += ani * vec3<f32>(nrm.x * nrm.y, nrm.x * nrm.z, nrm.y * nrm.z);
        }
        if (c.d > 0.0) {
            if (sp.k != 0xffffffffu) {
                let lk = rest_of(sp.codes >> 16u);
                let xk = old[sp.k].xyz;
                let sum = lj + lk;
                let sc = sum / (1.4142135 * lj * lk);
                let w = lk / sum;
                let g = c.d * sc * sc * w;
                f -= g * (w * xi + (1.0 - w) * xk - xj);
                hd += vec3<f32>(g * w);
            }
            if (paired) {
                let sum = lp + lj;
                let sc = sum / (1.4142135 * lp * lj);
                let g = c.d * sc * sc;
                f += g * ((lj * xp + lp * xj) / sum - xi);
                hd += vec3<f32>(g);
                paired = false;
            } else if ((sp.j & 0x80000000u) != 0u) {
                xp = xj;
                lp = lj;
                paired = true;
            }
        }
    }
    var touch = 0.0;
//SELF-FORCES
    return F(f, hd, ho, touch);
}

fn newton(fo: F, xi: vec3<f32>) -> vec3<f32> {
    let sc = 1.0 / (fo.hd.x + fo.hd.y + fo.hd.z);
    let a = fo.hd.x * sc; let b = fo.ho.x * sc; let cc = fo.ho.y * sc;
    let d = fo.hd.y * sc; let e = fo.ho.z * sc; let g = fo.hd.z * sc;
    let f = fo.f * sc;
    let c00 = d * g - e * e; let c01 = cc * e - b * g; let c02 = b * e - cc * d;
    let c11 = a * g - cc * cc; let c12 = b * cc - a * e; let c22 = a * d - b * b;
    let det = a * c00 + b * c01 + cc * c02;
    var x = xi;
    if (det > 1e-12) {
        x += vec3<f32>(c00 * f.x + c01 * f.y + c02 * f.z, c01 * f.x + c11 * f.y + c12 * f.z, c02 * f.x + c12 * f.y + c22 * f.z) / det;
    }
    return x;
}

@compute @workgroup_size(WG)
fn solve(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let q = lin(gid, nwg);
    if (q >= s.count) { return; }
    let i = s.start + q;
    let xi = pos[i].xyz;
    let fo = forces(i, xi);
    var x = vec4<f32>(newton(fo, xi), 0.0);
    // La position d'avant n'est lue que si le sommet touche un obstacle, ou à la dernière passe du sous-pas
    if (inside(x.xyz)) { x = collide(x.xyz, prev[i].xyz, c.mu); }
    if (fo.touch > 0.5) { x.w = 1.0; }
    pos[i] = x;
    if ((s.arg & 1u) != 0u) { next[i] = x; }
    // Dernière passe : la vitesse du sous-pas
    if ((s.arg & 2u) != 0u) { vel[i] = vec4<f32>((x.xyz - prev[i].xyz) / c.h * c.keep, 0.0); }
}
"#;

/// Auto-collision, ce que la résolution lit en plus : les voisins proches du sommet. Même calcul que la fin de `forces`.
const SOLVE_SELF: [(&str, &str); 2] = [
    ("//SELF-BIND", "@group(0) @binding(10) var<storage, read> cand: array<u32>;"),
    (
        "//SELF-FORCES",
        r#"    if (c.thick > 0.0) {
        for (var q = 0u; q < 8u; q++) {
            let j = cand[i * 8u + q];
            if (j == 0xffffffffu) { break; }
            let d = xi - old[j].xyz;
            let l = length(d);
            if (l < c.thick && l > 1e-9) {
                let nrm = d / l;
                f += c.kc * (c.thick - l) * nrm;
                hd += 2.0 * c.kc * nrm * nrm;
                ho += 2.0 * c.kc * vec3<f32>(nrm.x * nrm.y, nrm.x * nrm.z, nrm.y * nrm.z);
                touch = 1.0;
            }
        }
    }"#,
    ),
];

/// Auto-collision, la grille et les voisins : les mêmes calculs que `grid_build` et `grid_near`.
/// Cinq noyaux : compter les sommets par case (et par bloc de cases), sommer les blocs (un seul
/// groupe de fils), en déduire le début de chaque case, ranger les sommets, chercher les voisins.
const GRID: &str = r#"
struct P { x: f32, y: f32, z: f32, i: u32 }
@group(0) @binding(0) var<uniform> c: C;
@group(0) @binding(1) var<uniform> s: S;
@group(0) @binding(2) var<storage, read> pos: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> cells: array<atomic<u32>>;
@group(0) @binding(4) var<storage, read_write> blocks: array<atomic<u32>>;
@group(0) @binding(5) var<storage, read_write> starts: array<u32>;
@group(0) @binding(6) var<storage, read_write> sorted: array<P>;
@group(0) @binding(7) var<storage, read> restp: array<vec4<f32>>;
@group(0) @binding(8) var<storage, read_write> cand: array<u32>;

const BLOCK: u32 = 256u;

fn bucket(q: vec3<i32>) -> u32 {
    return ((bitcast<u32>(q.x) * 73856093u) ^ (bitcast<u32>(q.y) * 19349663u) ^ (bitcast<u32>(q.z) * 83492791u)) & c.mask;
}

fn cell_of(x: vec3<f32>) -> vec3<i32> { return vec3<i32>(floor(x / c.reach)); }

@compute @workgroup_size(WG)
fn count(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= s.count) { return; }
    let b = bucket(cell_of(pos[i].xyz));
    atomicAdd(&cells[b], 1u);
    atomicAdd(&blocks[b / BLOCK], 1u);
}

var<workgroup> part: array<u32, 64>;

// Un seul groupe de fils : chaque fil somme sa part des blocs, puis y range le début de chacun
@compute @workgroup_size(WG)
fn scan(@builtin(local_invocation_index) li: u32) {
    let nb = (c.mask + 1u) / BLOCK;
    let chunk = (nb + WG - 1u) / WG;
    let lo = min(li * chunk, nb);
    let hi = min(lo + chunk, nb);
    var sum = 0u;
    for (var b = lo; b < hi; b++) { sum += atomicLoad(&blocks[b]); }
    part[li] = sum;
    workgroupBarrier();
    var base = 0u;
    for (var k = 0u; k < li; k++) { base += part[k]; }
    for (var b = lo; b < hi; b++) {
        let n = atomicLoad(&blocks[b]);
        atomicStore(&blocks[b], base);
        base += n;
    }
}

// Un fil par bloc : le début de chaque case, et le compte du bloc remis à zéro pour le sous-pas suivant
@compute @workgroup_size(WG)
fn spread(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let b = lin(gid, nwg);
    if (b >= s.count) { return; }
    var first = atomicLoad(&blocks[b]);
    for (var k = 0u; k < BLOCK; k++) {
        let cell = b * BLOCK + k;
        starts[cell] = first;
        first += atomicLoad(&cells[cell]);
    }
    if (b + 1u == s.count) { starts[c.mask + 1u] = first; }
    atomicStore(&blocks[b], 0u);
}

// Chaque sommet prend une place dans sa case ; le compte de la case retombe à zéro
@compute @workgroup_size(WG)
fn fill(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= s.count) { return; }
    let x = pos[i].xyz;
    let b = bucket(cell_of(x));
    let left = atomicSub(&cells[b], 1u);
    sorted[starts[b] + left - 1u] = P(x.x, x.y, x.z, i);
}

@compute @workgroup_size(WG)
fn nearby(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= s.count) { return; }
    let cur = pos[i];
    var bd: array<f32, 8>;
    var bj: array<u32, 8>;
    var count = 0u;
    // Un sommet fixe (marque 2) ne cherche pas de voisins
    if (cur.w < 1.5 || cur.w > 2.5) {
        let xi = cur.xyz;
        let ri = restp[i].xyz;
        let q = cell_of(xi);
        let apart = (2.1 * c.thick) * (2.1 * c.thick);
        var seen: array<u32, 27>;
        for (var k = 0u; k < 27u; k++) {
            let b = bucket(q + vec3<i32>(i32(k % 3u) - 1, i32((k / 3u) % 3u) - 1, i32(k / 9u) - 1));
            seen[k] = b;
            var twice = false;
            for (var a = 0u; a < k; a++) { if (seen[a] == b) { twice = true; } }
            if (twice) { continue; }
            let lo = starts[b];
            let hi = min(starts[b + 1u], lo + 128u);
            for (var e = lo; e < hi; e++) {
                let pj = sorted[e];
                if (pj.i == i) { continue; }
                let d = xi - vec3<f32>(pj.x, pj.y, pj.z);
                let d2 = d.x * d.x + d.y * d.y + d.z * d.z;
                if (d2 >= c.reach * c.reach) { continue; }
                let r = ri - restp[pj.i].xyz;
                if (r.x * r.x + r.y * r.y + r.z * r.z < apart) { continue; }
                if (count < 8u) {
                    bd[count] = d2;
                    bj[count] = pj.i;
                    count++;
                } else {
                    var worst = 0u;
                    for (var a = 1u; a < 8u; a++) {
                        if (bd[a] > bd[worst] || (bd[a] == bd[worst] && bj[a] > bj[worst])) { worst = a; }
                    }
                    if (d2 < bd[worst] || (d2 == bd[worst] && pj.i < bj[worst])) {
                        bd[worst] = d2;
                        bj[worst] = pj.i;
                    }
                }
            }
        }
        // Triés par rang
        for (var a = 1u; a < count; a++) {
            let v = bj[a];
            var b = a;
            while (b > 0u && bj[b - 1u] > v) {
                bj[b] = bj[b - 1u];
                b--;
            }
            bj[b] = v;
        }
    }
    for (var k = 0u; k < 8u; k++) {
        var j = 0xffffffffu;
        if (k < count) { j = bj[k]; }
        cand[i * 8u + k] = j;
    }
}
"#;

/// Force restante du niveau fin : le même calcul que `rest_force`.
const REST: &str = r#"
@group(0) @binding(0) var<uniform> c: C;
@group(0) @binding(1) var<uniform> s: S;
@group(0) @binding(2) var<storage, read> pos: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> tgt: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> off: array<vec2<u32>>;
@group(0) @binding(5) var<storage, read> springs: array<E>;
@group(0) @binding(6) var<storage, read_write> res: array<vec4<f32>>;

fn rest_of(code: u32) -> f32 { let u = f32(code); return u * u * c.rest_scale; }

@compute @workgroup_size(WG)
fn rest(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= s.count) { return; }
    let cur = pos[i];
    let ty = tgt[i];
    if (ty.w <= 0.0 || cur.w > 1.5) { res[i] = vec4<f32>(0.0, 0.0, 0.0, -1.0); return; }
    let x = cur.xyz;
    var f = -ty.w * c.inv_h2 * (x - ty.xyz);
    var anchor = 0.0;
    let o = off[i];
    let count = o.y & 0xffffu;
    var xp = vec3<f32>(0.0);
    var lp = 0.0;
    var held_p = false;
    var paired = false;
    for (var e = 0u; e < count; e++) {
        let sp = springs[o.x + e];
        let pj = pos[sp.j & 0x3fffffffu];
        let xj = pj.xyz;
        let held = pj.w > 1.5;
        let lj = rest_of(sp.codes & 0xffffu);
        let d = x - xj;
        let l = length(d);
        var k = c.k;
        if ((sp.j & 0x40000000u) != 0u) { k = c.k * c.shear; }
        if (l > 1e-12) { f -= k * (l - lj) / l * d; }
        if (held) { anchor += c.k; }
        if (c.d > 0.0) {
            if (sp.k != 0xffffffffu) {
                let lk = rest_of(sp.codes >> 16u);
                let xk = pos[sp.k].xyz;
                let sum = lj + lk;
                let sc = sum / (1.4142135 * lj * lk);
                let w = lk / sum;
                let g = c.d * sc * sc * w;
                f -= g * (w * x + (1.0 - w) * xk - xj);
                if (held) { anchor += g; }
            }
            if (paired) {
                let sum = lp + lj;
                let sc = sum / (1.4142135 * lp * lj);
                let g = c.d * sc * sc;
                f += g * ((lj * xp + lp * xj) / sum - x);
                if (held_p) { anchor += g * lj / sum; }
                if (held) { anchor += g * lp / sum; }
                paired = false;
            } else if ((sp.j & 0x80000000u) != 0u) {
                xp = xj;
                lp = lj;
                held_p = held;
                paired = true;
            }
        }
    }
    if (cur.w > 0.5) {
        let n = normal_at(x);
        if (n.w > 0.5) { f -= min(dot(f, n.xyz), 0.0) * n.xyz; }
    }
    res[i] = vec4<f32>(f, anchor);
}
"#;

/// Les niveaux grossiers : les mêmes calculs que `stage` et `cycle`. Un noyau par étape pour les
/// grands niveaux (un sommet par fil), et `tail` qui enchaîne toutes les étapes des petits niveaux
/// dans un seul groupe de fils.
const LEVELS: &str = r#"
struct V { m: f32, s: f32, b: f32, ent: u32, ents: u32, kid: u32, kids: u32, pad: u32 }
struct N { j: u32, m: f32, s: f32, b: f32 }
struct W { f: vec4<f32>, d: vec4<f32>, r: vec4<f32>, e: vec4<f32> }
struct U { who: vec4<u32>, w: vec4<f32> }
struct L { range: array<vec4<u32>, 16> }
@group(0) @binding(0) var<uniform> c: C;
@group(0) @binding(1) var<uniform> s: S;
@group(0) @binding(2) var<uniform> lv: L;
@group(0) @binding(3) var<storage, read> vert: array<V>;
@group(0) @binding(4) var<storage, read> ent: array<N>;
@group(0) @binding(5) var<storage, read> kids: array<vec2<u32>>;
@group(0) @binding(6) var<storage, read> par: array<U>;
@group(0) @binding(7) var<storage, read_write> work: array<W>;
@group(0) @binding(8) var<storage, read> res: array<vec4<f32>>;

const JACOBI: f32 = 0.8;

fn own_of(v: V, anchor: f32) -> f32 { return c.inv_h2 * v.m + c.k * v.s + c.d * v.b + anchor; }

// Ce qui reste de la force une fois retirée la part de tous les déplacements, et la diagonale
fn remains(a: u32) -> vec4<f32> {
    let v = vert[a];
    let f = work[a].f;
    let own = own_of(v, f.w);
    var r = f.xyz - own * work[a].d.xyz;
    for (var e = v.ent; e < v.ent + v.ents; e++) {
        let n = ent[e];
        r -= (c.inv_h2 * n.m + c.k * n.s + c.d * n.b) * work[n.j].d.xyz;
    }
    return vec4<f32>(r, own);
}

fn do_gather(a: u32, fine: bool) {
    let v = vert[a];
    var sum = vec4<f32>(0.0);
    for (var e = v.kid; e < v.kid + v.kids; e++) {
        let k = kids[e];
        var r = vec4<f32>(0.0);
        if (fine) { r = res[k.x]; } else { r = work[k.x].r; }
        if (r.w >= 0.0) { sum += bitcast<f32>(k.y) * r; }
    }
    work[a].f = sum;
    let own = own_of(v, sum.w);
    var d = vec4<f32>(0.0);
    if (own > 0.0) { d = vec4<f32>(JACOBI * sum.xyz / own, 0.0); }
    work[a].d = d;
}

fn do_remain(a: u32) {
    let r = remains(a);
    work[a].r = vec4<f32>(r.xyz, work[a].f.w);
}

fn do_apply(a: u32) {
    let own = own_of(vert[a], work[a].f.w);
    if (own > 0.0) {
        let d = work[a].d;
        work[a].d = vec4<f32>(d.xyz + JACOBI * work[a].r.xyz / own, d.w);
    }
}

fn do_prolong(a: u32) {
    let u = par[a];
    var d = work[a].d;
    for (var k = 0u; k < 4u; k++) {
        if (u.w[k] > 0.0) { d = vec4<f32>(d.xyz + u.w[k] * work[u.who[k]].e.xyz, d.w); }
    }
    work[a].d = d;
}

fn do_post(a: u32) {
    let r = remains(a);
    var e = work[a].d;
    if (r.w > 0.0) { e = vec4<f32>(e.xyz + JACOBI * r.xyz / r.w, e.w); }
    work[a].e = e;
}

@compute @workgroup_size(WG)
fn gather(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let q = lin(gid, nwg);
    if (q < s.count) { do_gather(s.start + q, s.arg == 0u); }
}

@compute @workgroup_size(WG)
fn remain(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let q = lin(gid, nwg);
    if (q < s.count) { do_remain(s.start + q); }
}

@compute @workgroup_size(WG)
fn apply(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let q = lin(gid, nwg);
    if (q < s.count) { do_apply(s.start + q); }
}

@compute @workgroup_size(WG)
fn prolong(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let q = lin(gid, nwg);
    if (q < s.count) { do_prolong(s.start + q); }
}

@compute @workgroup_size(WG)
fn post(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let q = lin(gid, nwg);
    if (q < s.count) { do_post(s.start + q); }
}

// Une étape sur tout un niveau, partagée entre les fils du groupe
fn run(l: u32, what: u32, li: u32) {
    let r = lv.range[l];
    for (var a = r.x + li; a < r.x + r.y; a += WG) {
        switch what {
            case 0u: { do_gather(a, l == 0u); }
            case 1u: { do_remain(a); }
            case 2u: { do_apply(a); }
            case 3u: { do_prolong(a); }
            default: { do_post(a); }
        }
    }
}

// Tout le bas du cycle, du niveau `s.start` au niveau `s.count`, avec `s.arg` passes de plus sur
// le dernier : un seul groupe de fils, qui attend entre deux étapes que tous aient écrit.
@compute @workgroup_size(WG)
fn tail(@builtin(local_invocation_index) li: u32, @builtin(workgroup_id) wid: vec3<u32>) {
    let first = s.start;
    let last = s.count;
    let mine = wid.x == 0u && wid.y == 0u;
    for (var l = first; l <= last; l++) {
        if (mine) { run(l, 0u, li); }
        storageBarrier();
        if (l < last) {
            if (mine) { run(l, 1u, li); }
            storageBarrier();
        }
    }
    for (var b = 0u; b < s.arg; b++) {
        if (mine) { run(last, 1u, li); }
        storageBarrier();
        if (mine) { run(last, 2u, li); }
        storageBarrier();
    }
    if (mine) { run(last, 4u, li); }
    storageBarrier();
    for (var l = last; l > first; l--) {
        if (mine) { run(l - 1u, 3u, li); }
        storageBarrier();
        if (mine) { run(l - 1u, 4u, li); }
        storageBarrier();
    }
}
"#;

/// Remontée vers le tissu : le déplacement du niveau 0, rendu aux sommets libres, qui sont
/// ensuite sortis des obstacles. Même calcul que la fin de `correct`.
const UP: &str = r#"
struct W { f: vec4<f32>, d: vec4<f32>, r: vec4<f32>, e: vec4<f32> }
struct U { who: vec4<u32>, w: vec4<f32> }
@group(0) @binding(0) var<uniform> c: C;
@group(0) @binding(1) var<uniform> s: S;
@group(0) @binding(2) var<storage, read> up: array<U>;
@group(0) @binding(3) var<storage, read> work: array<W>;
@group(0) @binding(4) var<storage, read> res: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read_write> pos: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> old: array<vec4<f32>>;
//SELF-BIND

@compute @workgroup_size(WG)
fn lift(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= s.count) { return; }
    if (res[i].w < 0.0) { return; }
    let u = up[i];
    var d = vec3<f32>(0.0);
    for (var k = 0u; k < 4u; k++) {
        if (u.w[k] > 0.0) { d += u.w[k] * work[u.who[k]].e.xyz; }
    }
//SELF-CLAMP
    let out = keep_out(pos[i].xyz + d);
    pos[i] = out;
    old[i] = out;
}
"#;

/// Auto-collision, ce que la remontée lit en plus : près d'une autre couche, la correction est bornée (voir `correct`).
const UP_SELF: [(&str, &str); 2] = [
    ("//SELF-BIND", "@group(0) @binding(7) var<storage, read> cand: array<u32>;"),
    (
        "//SELF-CLAMP",
        r#"    if (c.thick > 0.0 && cand[i * 8u] != 0xffffffffu) {
        let len = length(d);
        let most = 0.25 * c.thick;
        if (len > most) { d *= most / len; }
    }"#,
    ),
];

/// Prédiction : le même calcul qu'au début d'un sous-pas de `step_cpu`, amortissement interne compris.
const PREDICT: &str = r#"
@group(0) @binding(0) var<uniform> c: C;
@group(0) @binding(1) var<uniform> s: S;
@group(0) @binding(2) var<storage, read_write> pos: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> vel: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> tgt: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read_write> prev: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> sa: array<vec4<f32>>;
@group(0) @binding(7) var<storage, read> kick: array<vec4<f32>>;
@group(0) @binding(8) var<storage, read> off: array<vec2<u32>>;
@group(0) @binding(9) var<storage, read> springs: array<E>;

@compute @workgroup_size(WG)
fn predict(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= s.count) { return; }
    let cur = pos[i];
    let x = cur.xyz;
    let m = tgt[i].w;
    prev[i] = vec4<f32>(x, 0.0);
    var out = vec4<f32>(x, 2.0);
    var y = x;
    if (m > 0.0) {
        var v = vel[i].xyz;
        // Amortissement interne : la vitesse se rapproche de celle des voisins
        if (c.calm > 0.0) {
            let o = off[i];
            let count = o.y & 0xffffu;
            if (count > 0u) {
                var sum = vec3<f32>(0.0);
                for (var e = 0u; e < count; e++) {
                    sum += vel[springs[o.x + e].j & 0x3fffffffu].xyz - v;
                }
                v += c.calm / f32(count) * sum;
            }
        }
        if (c.kick > 0.0) { v += c.kick * kick[i].xyz; }
        y = x + c.h * v;
        // Marque : 1 s'il touchait déjà un obstacle à sa dernière passe, 3 s'il vient d'y entrer
        let k = keep_out(y);
        var mark = 3.0 * k.w;
        if (cur.w > 0.5) { mark = 1.0; }
        out = vec4<f32>(k.xyz, mark);
    }
    tgt[i] = vec4<f32>(y, m);
    pos[i] = out;
    sa[i] = vec4<f32>(out.xyz, 0.0);
}
"#;

/// Accélération de Tchebychev entre deux passes : le même calcul que dans `step_cpu`. À la
/// dernière passe d'un sous-pas, elle écrit aussi la vitesse.
const ACCEL: &str = r#"
@group(0) @binding(0) var<uniform> c: C;
@group(0) @binding(1) var<uniform> s: S;
@group(0) @binding(2) var<storage, read_write> pos: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> vel: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> tgt: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read> prev: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> sb: array<vec4<f32>>;

@compute @workgroup_size(WG)
fn accel(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= s.count) { return; }
    let cur = pos[i];
    var x = cur.xyz;
    if (cur.w < 0.5) {
        let o = sb[i].xyz;
        x = o + s.omega * (x - o);
        pos[i] = vec4<f32>(x, cur.w);
    }
    sb[i] = vec4<f32>(x, 0.0);
    if ((s.arg & 2u) != 0u && tgt[i].w > 0.0) { vel[i] = vec4<f32>((x - prev[i].xyz) / c.h * c.keep, 0.0); }
}
"#;

/// Impulsion de l'image quand les forces sont les mêmes pour tous les sommets (gravité, vent
/// constant, frein) : calculée sur la carte à partir des vitesses qui y sont déjà, sans rien relire
/// ni renvoyer. Le même calcul que `uniform_kick`.
const KICK: &str = r#"
struct K { add: vec3<f32>, keep: f32 }
@group(0) @binding(0) var<uniform> k: K;
@group(0) @binding(1) var<uniform> s: S;
@group(0) @binding(2) var<storage, read> vel: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> kick: array<vec4<f32>>;

@compute @workgroup_size(WG)
fn fill(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= s.count) { return; }
    // Écrit comme le node « Forces » le calcule sur le processeur : mêmes arrondis, même tissu
    let v = vel[i].xyz;
    kick[i] = vec4<f32>((v + k.add) * k.keep - v, 0.0);
}
"#;

/// Écart de vitesse qu'apportent des forces uniformes : `(v + add) × keep − v`, sommet par sommet.
fn uniform_kick(vel: &[V4], add: [f32; 3], keep: f32) -> Vec<V4> {
    vel.par_iter().with_min_len(1024).map(|v| [(v[0] + add[0]) * keep - v[0], (v[1] + add[1]) * keep - v[1], (v[2] + add[2]) * keep - v[2], 0.0]).collect()
}

/// Relecture : positions (centre rajouté) et vitesses, dans l'ordre de la géométrie et composante
/// par composante, comme les colonnes de l'état. 24 octets par sommet au lieu de 32, et rien à
/// réordonner du côté du processeur.
const PACK: &str = r#"
@group(0) @binding(0) var<uniform> c: C;
@group(0) @binding(1) var<uniform> s: S;
@group(0) @binding(2) var<storage, read> pos: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> vel: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> perm: array<u32>;
@group(0) @binding(5) var<storage, read_write> packed: array<f32>;

@compute @workgroup_size(WG)
fn pack(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let k = lin(gid, nwg);
    let n = s.count;
    if (k >= n) { return; }
    let i = perm[k];
    let p = pos[k].xyz + c.origin.xyz;
    let v = vel[k].xyz;
    packed[i] = p.x;
    packed[n + i] = p.y;
    packed[2u * n + i] = p.z;
    packed[3u * n + i] = v.x;
    packed[4u * n + i] = v.y;
    packed[5u * n + i] = v.z;
}
"#;

/// Le tissu sur la carte : ses colonnes, ses noyaux, ses branchements.
struct GpuCloth {
    pos: Column,
    vel: Column,
    kick: Column,
    /// Les deux tampons des positions du début de la passe : les sommets fixes y sont écrits avec les positions
    lag: [Column; 2],
    /// Ce qui est relu : positions et vitesses dans l'ordre de la géométrie
    packed: Column,
    g_pack: wgpu::BindGroup,
    consts: wgpu::Buffer,
    slots: wgpu::Buffer,
    slot: u64,
    /// Ce pour quoi les cases de réglages sont remplies : itérations par sous-pas, rayon de Tchebychev, marques des passes
    slots_for: (usize, f32, u32),
    /// Les deux branchements de la résolution et de l'accélération : les tampons du début de passe s'y échangent
    g_solve: [wgpu::BindGroup; 2],
    g_accel: [wgpu::BindGroup; 2],
    g_predict: wgpu::BindGroup,
    /// Impulsion uniforme : ses réglages (forces, frein), et son branchement
    kick_k: wgpu::Buffer,
    g_kick: wgpu::BindGroup,
    levels: Option<GpuLevels>,
    /// Créée au premier pas qui demande l'auto-collision
    selfc: Option<GpuSelf>,
    /// Lancements du dernier pas d'image envoyé
    launches: usize,
    /// La case de réglages « tous les sommets », telle que le dernier pas l'a rangée
    all_slot: u32,
    /// Colonnes gardées en vie (branchées, jamais relues ici)
    _keep: Vec<Column>,
}

/// Itérations par sous-pas que les cases de réglages peuvent décrire
const MAX_PER: usize = 4096;

/// Les noyaux, les mêmes pour tous les tissus : compilés une fois, gardés
struct Programs {
    solve: Kernels,
    predict: Kernels,
    accel: Kernels,
    /// Niveaux grossiers : force restante du niveau fin, étapes d'un niveau, remontée vers le tissu
    rest: Kernels,
    levels: Kernels,
    up: Kernels,
    pack: Kernels,
    kick: Kernels,
}

fn programs(g: &Gpu) -> &'static Programs {
    static PROGRAMS: std::sync::OnceLock<Programs> = std::sync::OnceLock::new();
    PROGRAMS.get_or_init(|| {
        use Bind::{Read, Slot, Uniform, Write};
        Programs {
            solve: g.kernels("tissu-vbd", &format!("{STRUCTS}{SOLVE}{COLLIDE}"), &[Uniform, Slot(16), Write, Read, Read, Read, Read, Read, Write, Write], &["solve"]),
            predict: g.kernels("tissu-prediction", &format!("{STRUCTS}{PREDICT}{COLLIDE}"), &[Uniform, Slot(16), Write, Read, Write, Write, Write, Read, Read, Read], &["predict"]),
            accel: g.kernels("tissu-acceleration", &format!("{STRUCTS}{ACCEL}"), &[Uniform, Slot(16), Write, Write, Read, Read, Write], &["accel"]),
            rest: g.kernels("tissu-reste", &format!("{STRUCTS}{REST}{COLLIDE}"), &[Uniform, Slot(16), Read, Read, Read, Read, Write], &["rest"]),
            levels: g.kernels("tissu-niveaux", &format!("{STRUCTS}{LEVELS}"), &[Uniform, Slot(16), Uniform, Read, Read, Read, Read, Write, Read], &["gather", "remain", "apply", "prolong", "post", "tail"]),
            up: g.kernels("tissu-remontee", &format!("{STRUCTS}{UP}{COLLIDE}"), &[Uniform, Slot(16), Read, Read, Read, Write, Write], &["lift"]),
            pack: g.kernels("tissu-relecture", &format!("{STRUCTS}{PACK}"), &[Uniform, Slot(16), Read, Read, Read, Write], &["pack"]),
            kick: g.kernels("tissu-impulsion", &format!("{STRUCTS}{KICK}"), &[Uniform, Uniform, Read, Write], &["fill"]),
        }
    })
}

/// Les noyaux de l'auto-collision : compilés à la première demande
struct SelfPrograms {
    grid: Kernels,
    solve: Kernels,
    up: Kernels,
}

/// `None` si la carte ne permet pas de brancher assez de tampons sur un noyau
fn self_programs(g: &Gpu) -> Option<&'static SelfPrograms> {
    static PROGRAMS: std::sync::OnceLock<Option<SelfPrograms>> = std::sync::OnceLock::new();
    PROGRAMS
        .get_or_init(|| {
            use Bind::{Read, Slot, Uniform, Write};
            if g.max_storage < 9 {
                return None;
            }
            let with = |text: &str, parts: &[(&str, &str); 2]| parts.iter().fold(text.to_string(), |t, (mark, code)| t.replace(mark, code));
            Some(SelfPrograms {
                grid: g.kernels("tissu-grille", &format!("{STRUCTS}{GRID}"), &[Uniform, Slot(16), Read, Write, Write, Write, Write, Read, Write], &["count", "scan", "spread", "fill", "nearby"]),
                solve: g.kernels("tissu-vbd-auto-collision", &format!("{STRUCTS}{}{COLLIDE}", with(SOLVE, &SOLVE_SELF)), &[Uniform, Slot(16), Write, Read, Read, Read, Read, Read, Write, Write, Read], &["solve"]),
                up: g.kernels("tissu-remontee-auto-collision", &format!("{STRUCTS}{}{COLLIDE}", with(UP, &UP_SELF)), &[Uniform, Slot(16), Read, Read, Read, Write, Write, Read], &["lift"]),
            })
        })
        .as_ref()
}

/// L'auto-collision sur la carte : la grille, les voisins, et les branchements qui les lisent
struct GpuSelf {
    g_grid: wgpu::BindGroup,
    g_solve: [wgpu::BindGroup; 2],
    g_up: Option<wgpu::BindGroup>,
    /// Nombre de blocs de cases
    blocks: u64,
    _keep: Vec<Column>,
}

/// Les niveaux grossiers sur la carte
struct GpuLevels {
    /// Par niveau : nombre de sommets
    counts: Vec<u64>,
    /// Premier niveau traité par `tail` avec tous les suivants
    tail: usize,
    /// Cases de réglages : une par niveau, puis celle de `tail`
    slots: wgpu::Buffer,
    /// (niveaux utilisés, passes du bas) pour lesquels la case de `tail` est remplie
    tail_for: (usize, usize),
    g_rest: wgpu::BindGroup,
    g_levels: wgpu::BindGroup,
    g_up: wgpu::BindGroup,
    _keep: Vec<Column>,
    _table: wgpu::Buffer,
}

impl GpuCloth {
    /// `None` si ce tissu ne tient pas sur cette carte (une colonne dépasserait ce qu'un noyau peut brancher).
    fn new(g: &Gpu, t: &Topo) -> Option<GpuCloth> {
        let biggest = (t.n * 16).max(t.off.len() * 8).max(t.springs.len() * 12) as u64;
        if t.n == 0 || biggest > g.max_binding.min(g.max_buffer) || t.gpu_bytes() > g.max_buffer.saturating_mul(8) {
            return None;
        }
        let v4 = |label: &str| g.column(label, t.n, 16);
        let (pos, vel, tgt, prev, s0, s1) = (v4("tissu-position"), v4("tissu-vitesse"), v4("tissu-cible"), v4("tissu-precedent"), v4("tissu-debut-de-passe-0"), v4("tissu-debut-de-passe-1"));
        let off = g.column("tissu-debuts", t.off.len(), 8);
        let springs = g.column("tissu-ressorts", t.springs.len().max(1), 12);
        let masses: Vec<V4> = t.mass.iter().map(|m| [0.0, 0.0, 0.0, *m]).collect();
        g.write(&tgt, &masses);
        g.write(&off, &t.off);
        if !t.springs.is_empty() {
            g.write(&springs, &t.springs);
        }
        let slot = g.slot_size(16);
        let consts = g.uniform("tissu-reglages", std::mem::size_of::<Consts>() as u64);
        let slots = g.uniform("tissu-cases", slot * (2 * t.groups.len() + MAX_PER + 2) as u64);
        let progs = programs(g);
        let one = |c: &Column| c.pages[0].clone();
        let (bp, bv, bt, bpr, b0, b1) = (one(&pos), one(&vel), one(&tgt), one(&prev), one(&s0), one(&s1));
        let solve = |old: &wgpu::Buffer, next: &wgpu::Buffer| g.group(&progs.solve, &[&consts, &slots, &bp, &bt, &bpr, &off.pages[0], &springs.pages[0], old, next, &bv]);
        // La passe k lit le tampon de la passe k−1 et écrit le sien
        let g_solve = [solve(&b1, &b0), solve(&b0, &b1)];
        let kick = v4("tissu-impulsion");
        let bk = one(&kick);
        // L'accélération de la passe k relit l'itéré k−2 et y écrit le sien
        let g_accel = [g.group(&progs.accel, &[&consts, &slots, &bp, &bv, &bt, &bpr, &b0]), g.group(&progs.accel, &[&consts, &slots, &bp, &bv, &bt, &bpr, &b1])];
        let g_predict = g.group(&progs.predict, &[&consts, &slots, &bp, &bv, &bt, &bpr, &b0, &bk, &off.pages[0], &springs.pages[0]]);
        // Impulsion uniforme : ses deux petits tampons de réglages (forces et frein ; nombre de sommets)
        let kick_k = g.uniform("tissu-impulsion-forces", 16);
        let kick_s = g.uniform("tissu-impulsion-plage", 16);
        g.queue.write_buffer(&kick_s, 0, gpusim::bytes_of(&[0u32, t.n as u32, 1.0f32.to_bits(), 0]));
        let g_kick = g.group(&progs.kick, &[&kick_k, &kick_s, &bv, &bk]);
        let perm = g.column("tissu-ordre", t.n, 4);
        let packed = g.column("tissu-relecture", t.n * 6, 4);
        if packed.pages.len() != 1 {
            return None;
        }
        g.write(&perm, &t.perm);
        let g_pack = g.group(&progs.pack, &[&consts, &slots, &bp, &bv, &perm.pages[0], &packed.pages[0]]);
        let mut levels = None;
        if let Some(co) = t.coarse.as_ref().filter(|co| co.levels.len() <= 16) {
            let col = |label: &str, len: usize, stride: usize| g.column(label, len.max(1), stride);
            let res = col("tissu-reste", t.n, 16);
            let (vert, ent, kids, par) = (col("tissu-niveaux-sommets", co.vert.len(), 32), col("tissu-niveaux-voisins", co.ent.len(), 16), col("tissu-niveaux-enfants", co.kids.len(), 8), col("tissu-niveaux-parents", co.par.len(), 32));
            let (work, up) = (col("tissu-niveaux-travail", co.vert.len(), 64), col("tissu-remontee", co.up.len(), 32));
            if [&vert, &ent, &kids, &par, &work, &up].iter().any(|c| c.pages.len() != 1) {
                return None;
            }
            g.write(&vert, &co.vert);
            g.write(&par, &co.par);
            g.write(&up, &co.up);
            if !co.ent.is_empty() {
                g.write(&ent, &co.ent);
            }
            if !co.kids.is_empty() {
                g.write(&kids, &co.kids);
            }
            // La table des niveaux, pour `tail`, et une case de réglages par niveau
            let mut table = [[0u32; 4]; 16];
            for (l, r) in co.levels.iter().enumerate() {
                table[l] = [r[0], r[1], 0, 0];
            }
            let tb = g.uniform("tissu-niveaux-table", 256);
            g.queue.write_buffer(&tb, 0, gpusim::bytes_of(&table));
            let ls = g.uniform("tissu-niveaux-cases", slot * (co.levels.len() + 1) as u64);
            let mut data = vec![0u8; slot as usize * (co.levels.len() + 1)];
            for (l, r) in co.levels.iter().enumerate() {
                data[l * slot as usize..][..16].copy_from_slice(gpusim::bytes_of(&[r[0], r[1], 0, l as u32]));
            }
            g.queue.write_buffer(&ls, 0, &data);
            let g_rest = g.group(&progs.rest, &[&consts, &slots, &bp, &bt, &off.pages[0], &springs.pages[0], &res.pages[0]]);
            let g_levels = g.group(&progs.levels, &[&consts, &ls, &tb, &vert.pages[0], &ent.pages[0], &kids.pages[0], &par.pages[0], &work.pages[0], &res.pages[0]]);
            let g_up = g.group(&progs.up, &[&consts, &slots, &up.pages[0], &work.pages[0], &res.pages[0], &bp, &b0]);
            let tail = co.levels.iter().position(|r| r[1] as usize <= t.tail).unwrap_or(co.levels.len());
            levels = Some(GpuLevels { counts: co.levels.iter().map(|r| r[1] as u64).collect(), tail, slots: ls, tail_for: (0, 0), g_rest, g_levels, g_up, _keep: vec![res, vert, ent, kids, par, work, up], _table: tb });
        }
        Some(GpuCloth { pos, vel, kick, lag: [s0, s1], packed, g_pack, consts, slots, slot, slots_for: (0, -1.0, 0), g_solve, g_accel, g_predict, kick_k, g_kick, levels, selfc: None, launches: 0, all_slot: 0, _keep: vec![tgt, prev, off, springs, perm] })
    }

    /// Prépare l'auto-collision (grille, voisins, noyaux). Faux si cette carte ne peut pas la faire.
    fn ensure_self(&mut self, g: &Gpu, t: &Topo) -> bool {
        if self.selfc.is_some() {
            return true;
        }
        let Some(progs) = self_programs(g) else { return false };
        let cells = t.cells();
        let (cellc, blocks, starts) = (g.column("tissu-grille-cases", cells, 4), g.column("tissu-grille-blocs", cells / BLOCK, 4), g.column("tissu-grille-debuts", cells + 1, 4));
        let (sorted, restp, cand) = (g.column("tissu-grille-sommets", t.n, 16), g.column("tissu-repos", t.n, 16), g.column("tissu-voisins-proches", t.n * NEAR, 4));
        if [&cellc, &starts, &sorted, &restp, &cand].iter().any(|c| c.pages.len() != 1) {
            return false;
        }
        g.write(&restp, &t.rest_pos);
        fn one(c: &Column) -> &wgpu::Buffer {
            &c.pages[0]
        }
        let (bp, bv, b0, b1) = (one(&self.pos), one(&self.vel), one(&self.lag[0]), one(&self.lag[1]));
        let (bt, bpr, off, springs) = (one(&self._keep[0]), one(&self._keep[1]), one(&self._keep[2]), one(&self._keep[3]));
        let g_grid = g.group(&progs.grid, &[&self.consts, &self.slots, bp, one(&cellc), one(&blocks), one(&starts), one(&sorted), one(&restp), one(&cand)]);
        let solve = |old: &wgpu::Buffer, next: &wgpu::Buffer| g.group(&progs.solve, &[&self.consts, &self.slots, bp, bt, bpr, off, springs, old, next, bv, one(&cand)]);
        let g_solve = [solve(b1, b0), solve(b0, b1)];
        let g_up = self.levels.as_ref().map(|lv| g.group(&progs.up, &[&self.consts, &self.slots, one(&lv._keep[6]), one(&lv._keep[5]), one(&lv._keep[0]), bp, b0, one(&cand)]));
        self.selfc = Some(GpuSelf { g_grid, g_solve, g_up, blocks: (cells / BLOCK) as u64, _keep: vec![cellc, blocks, starts, sorted, restp, cand] });
        // Les cases de réglages changent de forme : à refaire
        self.slots_for = (0, -1.0, 0);
        true
    }

    /// Envoie les positions (les sommets fixes sont aussi recopiés dans les tampons du début de passe, où les passes ne les écrivent jamais)
    /// Remplit le tampon d'impulsion à partir des vitesses de la carte : forces uniformes, puis frein.
    fn fill_kick(&self, g: &Gpu, n: usize, add: [f32; 3], keep: f32) {
        g.queue.write_buffer(&self.kick_k, 0, gpusim::bytes_of(&[add[0], add[1], add[2], keep]));
        let mut enc = g.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            g.launch(&mut pass, &programs(g).kick.pipelines[0], &self.g_kick, &[], n as u64);
        }
        g.queue.submit([enc.finish()]);
    }

    fn send_positions(&self, g: &Gpu, pos: &[V4]) {
        g.write(&self.pos, pos);
        g.write(&self.lag[0], pos);
        g.write(&self.lag[1], pos);
    }

    /// Le cycle en V, dans une passe déjà ouverte. Même suite que `cycle`. Rend le nombre de lancements.
    fn cycle(&mut self, g: &Gpu, pass: &mut wgpu::ComputePass, depth: usize, cy: &Cycle) -> usize {
        let Some(lv) = self.levels.as_mut() else { return 0 };
        let progs = programs(g);
        let slot = self.slot;
        let at = |k: usize| [(k as u64 * slot) as u32];
        let last = depth - 1;
        let tail = lv.tail.min(depth);
        // La case de `tail` : premier niveau, dernier niveau, passes du bas
        if tail < depth && lv.tail_for != (depth, cy.bottom) {
            g.queue.write_buffer(&lv.slots, lv.counts.len() as u64 * slot, gpusim::bytes_of(&[tail as u32, last as u32, 0, cy.bottom as u32]));
            lv.tail_for = (depth, cy.bottom);
        }
        let mut count = 0;
        let mut run = |pass: &mut wgpu::ComputePass, what: usize, l: usize| {
            g.launch(pass, &progs.levels.pipelines[what], &lv.g_levels, &at(l), lv.counts[l]);
            count += 1;
        };
        let (gather, remain, apply, prolong, post) = (0, 1, 2, 3, 4);
        for l in 0..tail {
            run(pass, gather, l);
            if l < last {
                run(pass, remain, l);
            }
        }
        if tail < depth {
            // Un seul groupe de fils
            pass.set_pipeline(&progs.levels.pipelines[5]);
            pass.set_bind_group(0, &lv.g_levels, &at(lv.counts.len()));
            pass.dispatch_workgroups(1, 1, 1);
        } else {
            for _ in 0..cy.bottom {
                run(pass, remain, last);
                run(pass, apply, last);
            }
            run(pass, post, last);
        }
        for l in (0..tail.min(last)).rev() {
            run(pass, prolong, l);
            run(pass, post, l);
        }
        count + (tail < depth) as usize
    }

    /// Envoie à la carte tout un pas d'image : sous-pas, itérations, une passe par plage.
    fn step(&mut self, g: &Gpu, t: &Topo, c: &Consts, p: &Params) {
        let (sub, per) = p.split();
        let rho = p.rho_used();
        let groups = t.groups.len();
        // Les passes écrivent-elles les positions du début de la passe suivante ? Seulement si
        // quelqu'un les lit (la flexion), et si l'accélération ne s'en charge pas déjà.
        let marks = ((c.d > 0.0 || c.thick > 0.0) && rho <= 0.0) as u32;
        // Cases de réglages : une par plage, les mêmes pour la dernière passe d'un sous-pas (qui
        // écrit aussi la vitesse), puis une par itération (son poids de Tchebychev), puis tous les sommets
        if self.slots_for != (per, rho, marks) {
            let mut data = vec![0u8; (self.slot as usize) * (2 * groups + per + 2)];
            let mut put = |k: usize, v: [u32; 4]| data[k * self.slot as usize..][..16].copy_from_slice(gpusim::bytes_of(&v));
            for (k, r) in t.groups.iter().enumerate() {
                put(k, [r[0], r[1], 1.0f32.to_bits(), marks]);
                // Avec l'accélération, c'est elle qui écrit la vitesse, après avoir déplacé le sommet
                put(groups + k, [r[0], r[1], 1.0f32.to_bits(), marks | if rho > 0.0 { 0 } else { 2 }]);
            }
            let mut w = 1.0;
            for k in 1..=per {
                w = omega(k, w, rho);
                put(2 * groups + k - 1, [0, t.n as u32, w.to_bits(), if k == per { 2 } else { 0 }]);
            }
            put(2 * groups + per, [0, t.n as u32, 1.0f32.to_bits(), 0]);
            // Auto-collision : un fil par bloc de cases
            put(2 * groups + per + 1, [0, self.selfc.as_ref().map_or(0, |s| s.blocks as u32), 1.0f32.to_bits(), 0]);
            g.queue.write_buffer(&self.slots, 0, &data);
            self.slots_for = (per, rho, marks);
        }
        g.queue.write_buffer(&self.consts, 0, gpusim::bytes_of(std::slice::from_ref(c)));
        let slot = self.slot;
        let at = |k: usize| [(k as u64 * slot) as u32];
        let all = at(2 * groups + per);
        let progs = programs(g);
        let mut enc = g.device.create_command_encoder(&Default::default());
        let depth = p.levels.min(self.levels.as_ref().map_or(0, |l| l.counts.len()));
        let mut count = 0;
        // L'auto-collision, si ce pas la demande (elle a été préparée avant) : sortie le temps du pas, puis remise
        let taken = if c.thick > 0.0 { self.selfc.take() } else { None };
        {
            let selfc = taken.as_ref().zip(self_programs(g));
            let mut pass = enc.begin_compute_pass(&Default::default());
            for _ in 0..sub {
                g.launch(&mut pass, &progs.predict.pipelines[0], &self.g_predict, &all, t.n as u64);
                count += 1;
                // Auto-collision : la grille des positions prédites, puis les voisins proches de chaque sommet
                if let Some((sc, sp)) = selfc {
                    g.launch(&mut pass, &sp.grid.pipelines[0], &sc.g_grid, &all, t.n as u64);
                    pass.set_pipeline(&sp.grid.pipelines[1]);
                    pass.set_bind_group(0, &sc.g_grid, &all);
                    pass.dispatch_workgroups(1, 1, 1);
                    g.launch(&mut pass, &sp.grid.pipelines[2], &sc.g_grid, &at(2 * groups + per + 1), sc.blocks);
                    g.launch(&mut pass, &sp.grid.pipelines[3], &sc.g_grid, &all, t.n as u64);
                    g.launch(&mut pass, &sp.grid.pipelines[4], &sc.g_grid, &all, t.n as u64);
                    count += 5;
                }
                if depth > 0 {
                    if let Some(lv) = &self.levels {
                        g.launch(&mut pass, &progs.rest.pipelines[0], &lv.g_rest, &all, t.n as u64);
                    }
                    count += 2 + self.cycle(g, &mut pass, depth, &p.cycle);
                    match (&self.levels, selfc.and_then(|(sc, sp)| Some((sc.g_up.as_ref()?, sp)))) {
                        (_, Some((up, sp))) => g.launch(&mut pass, &sp.up.pipelines[0], up, &all, t.n as u64),
                        (Some(lv), None) => g.launch(&mut pass, &progs.up.pipelines[0], &lv.g_up, &all, t.n as u64),
                        _ => {}
                    }
                }
                for k in 1..=per {
                    let first = if k == per { groups } else { 0 };
                    let (kernel, group) = match selfc {
                        Some((sc, sp)) => (&sp.solve.pipelines[0], &sc.g_solve[k % 2]),
                        None => (&progs.solve.pipelines[0], &self.g_solve[k % 2]),
                    };
                    for (gi, r) in t.groups.iter().enumerate() {
                        g.launch(&mut pass, kernel, group, &at(first + gi), r[1] as u64);
                    }
                    count += groups;
                    if rho > 0.0 {
                        // L'itération k relit l'itéré k−2 et y écrit le sien : c'est le tampon que lira la passe k+1
                        g.launch(&mut pass, &progs.accel.pipelines[0], &self.g_accel[k % 2], &at(2 * groups + k - 1), t.n as u64);
                        count += 1;
                    }
                }
            }
        }
        if taken.is_some() {
            self.selfc = taken;
        }
        self.launches = count;
        self.all_slot = all[0];
        g.queue.submit([enc.finish()]);
    }
}

// ---------- le tissu, vu de l'extérieur ----------

/// Un tissu prêt à simuler : sa topologie, son état, et sa copie sur la carte quand il y en a une.
/// L'état est gardé des deux côtés : si la carte rend une erreur, le pas est refait sur le processeur.
pub struct Cloth {
    pub topo: Topo,
    /// Centre du tissu au chargement : les positions des noyaux lui sont relatives
    origin: [f32; 3],
    /// État côté processeur, dans l'ordre de rangement. À jour seulement si `sorted_fresh` :
    /// tant que la carte calcule, c'est `geom` qui tient l'état.
    pos: Vec<V4>,
    vel: Vec<V4>,
    sorted_fresh: bool,
    /// Dernier état relu de la carte : positions et vitesses dans l'ordre de la géométrie
    geom: Option<State>,
    /// Changement de vitesse demandé pour le prochain pas (vide : aucun)
    kick: Vec<V4>,
    /// Forces uniformes demandées pour le prochain pas : (vitesse ajoutée, part gardée après le frein)
    uniform: Option<([f32; 3], f32)>,
    gpu: Option<GpuCloth>,
    /// Ce que la carte n'a pas encore reçu (positions, vitesses, impulsion)
    send: (bool, bool, bool),
    /// Pas envoyés à la carte et pas encore relus : de quoi les refaire sur le processeur si elle échoue
    ahead: Vec<(Consts, Params, Kick)>,
    /// Numéro de ce tissu, et compteur de ses pas : une vue sait ainsi si les tampons de la carte
    /// montrent encore l'image qu'elle veut dessiner
    id: u64,
    generation: Arc<AtomicU64>,
    /// Ordre de rangement, partagé avec les vues (créé au premier cliché)
    order: Option<(Arc<Vec<u32>>, Arc<Vec<u32>>)>,
    /// Clichés demandés à la carte et pas encore arrivés, avec le nombre de pas en attente à ce moment-là
    shots: std::collections::VecDeque<(Arc<Shot>, usize, std::time::Instant)>,
    /// Le cliché de l'état courant, s'il a déjà été pris
    shot_now: Option<Arc<Shot>>,
    /// Dernier cliché arrivé entier : l'état d'où repartir si la carte échoue ensuite
    base: Option<Arc<Shot>>,
    /// Dernier état relu sans erreur, partagé avec les clichés (ce qu'ils rendent si leur relecture échoue)
    good: Arc<Mutex<Option<Arc<State>>>>,
    /// Durée du dernier pas fini sur la carte, de l'envoi à l'arrivée de son cliché, en millisecondes
    last_ms: f32,
}

/// L'impulsion d'un pas : aucune, une par sommet, ou des forces uniformes (ajout, part gardée)
enum Kick {
    None,
    Each(Vec<V4>),
    Uniform([f32; 3], f32),
}

/// Ce qu'une vue doit savoir pour dessiner un tissu directement depuis la carte.
#[derive(Clone)]
pub struct Live {
    /// Positions (`vec4<f32>`), dans l'ordre de rangement, relatives à `origin`
    pub pos: wgpu::Buffer,
    pub origin: [f32; 3],
    /// Rang d'origine du sommet rangé en k, et l'inverse
    pub perm: Arc<Vec<u32>>,
    pub inv: Arc<Vec<u32>>,
    /// Numéro du tissu : deux clichés du même tissu partagent leurs tampons
    pub cloth: u64,
    generation: u64,
    now: Arc<AtomicU64>,
}

impl Live {
    /// Les tampons de la carte montrent-ils encore cet état ? Faux dès que le tissu a avancé.
    pub fn current(&self) -> bool {
        self.now.load(Ordering::Acquire) == self.generation
    }
}

/// L'état d'un tissu à la fin d'un pas : un « cliché ». Sur la carte, c'est une relecture demandée
/// mais pas attendue : les données ne sont copiées vers le processeur que si quelqu'un les lit
/// (`state`). Tant que le tissu n'a pas avancé, une vue peut le dessiner sans rien lire (`live`).
pub struct Shot {
    n: usize,
    pending: Mutex<Option<Pending>>,
    state: OnceLock<Arc<State>>,
    failed: AtomicBool,
    good: Arc<Mutex<Option<Arc<State>>>>,
    /// De quoi dessiner depuis la carte (rien si le calcul se fait sur le processeur, ou si
    /// l'interface n'utilise pas la même carte)
    pub live: Option<Live>,
}

impl std::fmt::Debug for Shot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Shot({} sommets, {})", self.n, if self.resolved() { "lu" } else { "sur la carte" })
    }
}

impl Shot {
    pub fn len(&self) -> usize {
        self.n
    }

    /// Les données sont-elles déjà du côté du processeur ?
    pub fn resolved(&self) -> bool {
        self.state.get().is_some()
    }

    /// `state` rendrait-il la main sans attendre la carte ? Ne bloque pas.
    pub fn ready(&self) -> bool {
        self.resolved() || self.pending.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_none_or(|p| p.ready())
    }

    /// Attend que la carte ait fini ce pas, sans rien copier.
    pub fn wait(&self) {
        if !self.resolved() {
            if let Some(p) = self.pending.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                p.packed.block();
            }
        }
    }

    /// La relecture a-t-elle échoué ? (`state` rend alors le dernier état connu du tissu.)
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    /// Positions et vitesses, dans l'ordre de la géométrie. La première demande attend la carte
    /// s'il le faut et copie les données ; les suivantes sont gratuites.
    pub fn state(&self) -> Arc<State> {
        self.state
            .get_or_init(|| {
                let n = self.n;
                let pending = self.pending.lock().unwrap_or_else(|e| e.into_inner()).take();
                match pending.and_then(|p| p.packed.wait().ok()).filter(|b| b.len() == n * 24) {
                    Some(b) => {
                        let part = |k: usize| gpusim::from_bytes::<f32>(&b[k * n * 4..(k + 1) * n * 4]);
                        let st = Arc::new(([part(0), part(1), part(2)], [part(3), part(4), part(5)]));
                        *self.good.lock().unwrap_or_else(|e| e.into_inner()) = Some(st.clone());
                        st
                    }
                    None => {
                        // La carte a échoué : plutôt qu'un tissu en miettes, le dernier état lu
                        self.failed.store(true, Ordering::Relaxed);
                        let last = self.good.lock().unwrap_or_else(|e| e.into_inner()).clone().filter(|s| s.0[0].len() == n);
                        last.unwrap_or_else(|| Arc::new(([vec![0.0; n], vec![0.0; n], vec![0.0; n]], [vec![0.0; n], vec![0.0; n], vec![0.0; n]])))
                    }
                }
            })
            .clone()
    }

    /// Boîte englobante approchée (un sommet sur `stride`), si les données sont arrivées. Ne bloque
    /// pas et ne copie rien : de quoi cadrer une vue pendant la lecture.
    pub fn bounds_now(&self, stride: usize) -> Option<([f32; 3], [f32; 3])> {
        let (n, stride) = (self.n, stride.max(1));
        let fold = |get: &dyn Fn(usize, usize) -> f32| {
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for c in 0..3 {
                for i in (0..n).step_by(stride) {
                    let x = get(c, i);
                    if x.is_finite() {
                        lo[c] = lo[c].min(x);
                        hi[c] = hi[c].max(x);
                    }
                }
            }
            (lo[0] <= hi[0]).then_some((lo, hi))
        };
        if let Some(st) = self.state.get() {
            return fold(&|c, i| st.0[c][i]);
        }
        let g = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        g.as_ref()?.packed.peek(|b| if b.len() == n * 24 { fold(&|c, i| f32::from_le_bytes([b[(c * n + i) * 4], b[(c * n + i) * 4 + 1], b[(c * n + i) * 4 + 2], b[(c * n + i) * 4 + 3]])) } else { None })?
    }
}

static CLOTH_ID: AtomicU64 = AtomicU64::new(1);

/// Positions et vitesses, dans l'ordre de la géométrie
pub type State = ([Vec<f32>; 3], [Vec<f32>; 3]);

impl Cloth {
    /// `positions` : l'état de départ, dans l'ordre de la géométrie. `on_gpu` : essayer la carte.
    pub fn new(topo: Topo, positions: &[Vec<f32>; 3], on_gpu: bool) -> Cloth {
        let n = topo.n;
        let mut origin = [0.0; 3];
        for c in 0..3 {
            let (lo, hi) = positions[c][..n].iter().fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
            if n > 0 {
                origin[c] = (lo + hi) * 0.5;
            }
        }
        let gpu = if on_gpu { gpusim::gpu().and_then(|g| GpuCloth::new(g, &topo)) } else { None };
        let mut cloth = Cloth {
            topo,
            origin,
            pos: vec![[0.0; 4]; n],
            vel: vec![[0.0; 4]; n],
            sorted_fresh: true,
            geom: None,
            kick: Vec::new(),
            uniform: None,
            gpu,
            send: (true, true, false),
            ahead: Vec::new(),
            id: CLOTH_ID.fetch_add(1, Ordering::Relaxed),
            generation: Arc::new(AtomicU64::new(0)),
            order: None,
            shots: Default::default(),
            shot_now: None,
            base: None,
            good: Arc::new(Mutex::new(None)),
            last_ms: 0.0,
        };
        cloth.set_positions(positions);
        cloth
    }

    /// Lancements de noyaux du dernier pas d'image fait sur la carte (0 sur le processeur)
    pub fn launches(&self) -> usize {
        self.gpu.as_ref().map_or(0, |g| g.launches)
    }

    /// Le calcul se fait-il sur la carte ?
    pub fn on_gpu(&self) -> bool {
        self.gpu.is_some()
    }

    /// Rend la topologie (pour préparer le même tissu autrement sans la reconstruire)
    pub fn into_topo(self) -> Topo {
        self.topo
    }

    /// Met à jour l'état rangé à partir du dernier état relu de la carte
    fn sort_state(&mut self) {
        if let Some((p, v)) = self.geom.as_ref().filter(|_| !self.sorted_fresh) {
            let o = self.origin;
            (self.pos.par_iter_mut(), self.vel.par_iter_mut(), self.topo.perm.par_iter()).into_par_iter().with_min_len(1024).for_each(|(x, u, &i)| {
                let i = i as usize;
                *x = [p[0][i] - o[0], p[1][i] - o[1], p[2][i] - o[2], 0.0];
                *u = [v[0][i], v[1][i], v[2][i], 0.0];
            });
        }
        self.sorted_fresh = true;
    }

    pub fn set_positions(&mut self, p: &[Vec<f32>; 3]) {
        self.sync();
        self.sort_state();
        self.geom = None;
        let o = self.origin;
        let perm = &self.topo.perm;
        self.pos.par_iter_mut().zip(perm.par_iter()).for_each(|(x, &i)| {
            let i = i as usize;
            *x = [p[0][i] - o[0], p[1][i] - o[1], p[2][i] - o[2], 0.0];
        });
        self.send.0 = true;
    }

    /// Impose les vitesses : pour un état de départ, ou une reprise.
    pub fn set_velocities(&mut self, v: &[Vec<f32>; 3]) {
        self.sync();
        self.sort_state();
        self.geom = None;
        let perm = &self.topo.perm;
        let mass = &self.topo.mass;
        (self.vel.par_iter_mut(), perm.par_iter(), mass.par_iter()).into_par_iter().for_each(|(x, &i, m)| {
            let i = i as usize;
            // Un sommet fixe n'a pas de vitesse : ses voisins la lisent (amortissement interne)
            *x = if *m > 0.0 { [v[0][i], v[1][i], v[2][i], 0.0] } else { [0.0; 4] };
        });
        self.kick.clear();
        self.uniform = None;
        self.send.1 = true;
    }

    /// Les vitesses voulues au prochain pas, après les forces de l'image (gravité, vent, frein).
    /// L'écart avec les vitesses courantes est appliqué par parts égales à chaque sous-pas, comme
    /// une force qui agit pendant toute l'image : appliqué d'un coup, il secouerait le tissu à
    /// chaque image, d'autant plus fort que le tissu est raide.
    pub fn push_velocities(&mut self, v: &[Vec<f32>; 3]) {
        self.sync();
        self.uniform = None;
        let perm = &self.topo.perm;
        self.kick.resize(self.topo.n, [0.0; 4]);
        match self.geom.as_ref().filter(|_| !self.sorted_fresh) {
            Some((_, old)) => self.kick.par_iter_mut().zip(perm.par_iter()).with_min_len(1024).for_each(|(k, &i)| {
                let i = i as usize;
                *k = [v[0][i] - old[0][i], v[1][i] - old[1][i], v[2][i] - old[2][i], 0.0];
            }),
            None => self.kick.par_iter_mut().zip(perm.par_iter()).zip(self.vel.par_iter()).with_min_len(1024).for_each(|((k, &i), old)| {
                let i = i as usize;
                *k = [v[0][i] - old[0], v[1][i] - old[1], v[2][i] - old[2], 0.0];
            }),
        }
        self.send.2 = true;
    }

    /// Les mêmes forces pour tous les sommets au prochain pas : chaque vitesse devient
    /// `(v + add) × keep` (gravité et vent d'une image, puis frein), répartie sur les sous-pas comme
    /// avec `push_velocities`. Rien n'est relu ni envoyé : sur la carte, un noyau s'en charge.
    pub fn push_uniform(&mut self, add: [f32; 3], keep: f32) {
        self.kick.clear();
        self.uniform = Some((add, keep));
    }

    /// Avance d'une image. Sur la carte, rend la main dès que le travail est envoyé : `read`
    /// attend le résultat, `request` le demande sans attendre.
    pub fn step(&mut self, dt: f32, p: &Params) {
        if self.topo.n == 0 || dt <= 0.0 {
            return;
        }
        let mut c = Consts::new(p, dt, self.origin, &self.topo);
        let kick = match self.uniform.take() {
            Some((add, keep)) => Kick::Uniform(add, keep),
            None if !self.kick.is_empty() => Kick::Each(std::mem::take(&mut self.kick)),
            None => Kick::None,
        };
        if !matches!(kick, Kick::None) {
            c.kick = 1.0 / p.split().0 as f32;
        }
        // L'auto-collision demande plus de tampons par noyau que certaines cartes n'en permettent : le tissu passe alors sur le processeur
        if p.thickness.is_some() {
            let ready = match (self.gpu.as_mut(), gpusim::gpu()) {
                (Some(gc), Some(g)) => gc.ensure_self(g, &self.topo),
                _ => true,
            };
            if !ready {
                self.sync();
                self.sort_state();
                self.geom = None;
                self.gpu = None;
            }
        }
        self.settle();
        self.shot_now = None;
        if let (Some(gc), Some(g)) = (self.gpu.as_mut(), gpusim::gpu()) {
            if self.send.0 {
                gc.send_positions(g, &self.pos);
            }
            if self.send.1 {
                g.write(&gc.vel, &self.vel);
            }
            match &kick {
                Kick::Each(k) if self.send.2 => g.write(&gc.kick, k),
                Kick::Uniform(add, keep) => gc.fill_kick(g, self.topo.n, *add, *keep),
                _ => {}
            }
            self.send = (false, false, false);
            gc.step(g, &self.topo, &c, p);
            self.ahead.push((c, *p, kick));
            self.generation.fetch_add(1, Ordering::AcqRel);
            return;
        }
        // La carte a lâché avec des pas en route : on les refait d'abord ici
        if !self.ahead.is_empty() {
            self.recover();
        }
        self.gpu = None;
        self.sort_state();
        self.geom = None;
        self.generation.fetch_add(1, Ordering::AcqRel);
        Self::step_on_cpu(&self.topo, &mut self.pos, &mut self.vel, &kick, &c, p);
    }

    /// Un pas sur le processeur, avec son impulsion telle qu'elle a été demandée.
    fn step_on_cpu(t: &Topo, pos: &mut [V4], vel: &mut [V4], kick: &Kick, c: &Consts, p: &Params) {
        let made;
        let k = match kick {
            Kick::None => None,
            Kick::Each(k) => Some(&k[..]),
            Kick::Uniform(add, keep) => {
                made = uniform_kick(vel, *add, *keep);
                Some(&made[..])
            }
        };
        step_cpu(t, pos, vel, k, c, p);
    }

    /// Range les clichés que la carte a finis : le dernier devient l'état d'où repartir en cas
    /// d'erreur, et les pas qu'il couvre n'ont plus à être gardés. Ne bloque pas.
    fn settle(&mut self) {
        while let Some((shot, count, since)) = self.shots.front() {
            if !shot.ready() {
                break;
            }
            let (count, ms) = (*count, since.elapsed().as_secs_f32() * 1000.0);
            let Some((shot, _, _)) = self.shots.pop_front() else { break };
            if shot.failed() || self.gpu.as_ref().is_none() || gpusim::gpu().is_none() {
                // La carte a échoué : on garde les pas en attente, `accept` les refera
                continue;
            }
            let done = count.min(self.ahead.len());
            self.ahead.drain(..done);
            for s in self.shots.iter_mut() {
                s.1 = s.1.saturating_sub(done);
            }
            self.base = Some(shot);
            self.last_ms = ms;
        }
    }

    /// La carte calcule-t-elle encore un pas déjà envoyé ? Ne bloque pas.
    pub fn busy(&mut self) -> bool {
        self.settle();
        !self.shots.is_empty()
    }

    /// Attend que la carte ait fini les pas envoyés, sans rien relire.
    pub fn wait(&mut self) {
        if let Some((shot, _, _)) = self.shots.back() {
            shot.wait();
        }
        self.settle();
    }

    /// Durée du dernier pas fini sur la carte (envoi → arrivée de son cliché), en millisecondes.
    /// Lue une fois par image de l'interface : elle est arrondie au temps d'une image.
    pub fn last_ms(&self) -> f32 {
        self.last_ms
    }

    /// Le cliché de l'état courant. Sur la carte, la relecture est demandée mais pas attendue :
    /// rien ne bloque, et rien n'est copié tant que personne ne lit l'état.
    pub fn shot(&mut self) -> Arc<Shot> {
        if let Some(s) = &self.shot_now {
            return s.clone();
        }
        let n = self.topo.n;
        let pending = if self.ahead.is_empty() { None } else { self.request() };
        let order = pending.as_ref().map(|_| self.order.get_or_insert_with(|| (Arc::new(self.topo.perm.clone()), Arc::new(self.topo.inv.clone()))).clone());
        let shot = match (pending.zip(order), self.gpu.as_ref(), gpusim::gpu()) {
            (Some((pending, (perm, inv))), Some(gc), Some(g)) => {
                // De quoi dessiner ces tampons tels quels (une vue de l'interface ne le peut que si elle est sur la même carte)
                let _ = g;
                let live = (gc.pos.pages.len() == 1).then(|| {
                    Live { pos: gc.pos.pages[0].clone(), origin: self.origin, perm, inv, cloth: self.id, generation: self.generation.load(Ordering::Acquire), now: self.generation.clone() }
                });
                let shot = Arc::new(Shot { n, pending: Mutex::new(Some(pending)), state: OnceLock::new(), failed: AtomicBool::new(false), good: self.good.clone(), live });
                self.shots.push_back((shot.clone(), self.ahead.len(), std::time::Instant::now()));
                shot
            }
            _ => {
                let state = OnceLock::new();
                let _ = state.set(Arc::new(self.read()));
                Arc::new(Shot { n, pending: Mutex::new(None), state, failed: AtomicBool::new(false), good: self.good.clone(), live: None })
            }
        };
        self.shot_now = Some(shot.clone());
        shot
    }

    /// Demande à la carte l'état courant, sans attendre. `None` : rien à relire (calcul sur le processeur).
    pub fn request(&self) -> Option<Pending> {
        let (gc, g) = (self.gpu.as_ref()?, gpusim::gpu()?);
        if self.ahead.is_empty() {
            return None;
        }
        let mut enc = g.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            g.launch(&mut pass, &programs(g).pack.pipelines[0], &gc.g_pack, &[gc.all_slot], self.topo.n as u64);
        }
        g.queue.submit([enc.finish()]);
        Some(Pending { packed: g.read(&gc.packed) })
    }

    /// Range dans l'état ce que la carte a rendu. Si elle a rendu une erreur, les pas en attente
    /// sont refaits sur le processeur, et le tissu y reste.
    pub fn accept(&mut self, pending: Option<Pending>) {
        if self.ahead.is_empty() {
            return;
        }
        let n = self.topo.n;
        match pending.and_then(|p| p.packed.wait().ok()).filter(|b| b.len() == n * 24) {
            Some(b) => {
                let part = |k: usize| gpusim::from_bytes::<f32>(&b[k * n * 4..(k + 1) * n * 4]);
                self.geom = Some(([part(0), part(1), part(2)], [part(3), part(4), part(5)]));
                self.sorted_fresh = false;
                self.ahead.clear();
                self.shots.clear();
                self.base = None;
            }
            None => self.recover(),
        }
    }

    /// La carte a échoué : le tissu repart du dernier état arrivé entier, refait sur le processeur
    /// les pas qui manquent, et y reste.
    fn recover(&mut self) {
        self.gpu = None;
        self.shots.clear();
        self.shot_now = None;
        if let Some(base) = self.base.take() {
            let st = base.state();
            if !base.failed() {
                self.geom = Some((*st).clone());
                self.sorted_fresh = false;
            }
        }
        self.sort_state();
        self.geom = None;
        self.send = (true, true, false);
        for (c, p, kick) in std::mem::take(&mut self.ahead) {
            Self::step_on_cpu(&self.topo, &mut self.pos, &mut self.vel, &kick, &c, &p);
        }
    }

    /// Met l'état du processeur à jour (attend la carte s'il le faut).
    pub fn sync(&mut self) {
        if self.ahead.is_empty() {
            // Rien en route, mais le dernier cliché arrivé est plus récent que l'état gardé ici
            if let Some(base) = self.base.take() {
                let st = base.state();
                if !base.failed() {
                    self.geom = Some((*st).clone());
                    self.sorted_fresh = false;
                }
            }
            return;
        }
        // Le cliché de l'état courant a déjà demandé cette relecture : on la prend
        if let Some(shot) = self.shot_now.clone() {
            let st = shot.state();
            if shot.failed() {
                self.recover();
            } else {
                self.geom = Some((*st).clone());
                self.sorted_fresh = false;
                self.ahead.clear();
                self.shots.clear();
                self.base = None;
            }
            return;
        }
        let pending = self.request();
        self.accept(pending);
    }

    /// Positions et vitesses courantes, dans l'ordre de la géométrie.
    pub fn read(&mut self) -> State {
        self.sync();
        if let Some(state) = self.geom.as_ref().filter(|_| !self.sorted_fresh) {
            return state.clone();
        }
        let (n, o) = (self.topo.n, self.origin);
        let mut p = [vec![0.0; n], vec![0.0; n], vec![0.0; n]];
        let mut v = [vec![0.0; n], vec![0.0; n], vec![0.0; n]];
        for (k, &i) in self.topo.perm.iter().enumerate() {
            let i = i as usize;
            for c in 0..3 {
                p[c][i] = self.pos[k][c] + o[c];
                v[c][i] = self.vel[k][c];
            }
        }
        (p, v)
    }
}

/// Une relecture de l'état demandée à la carte.
pub struct Pending {
    packed: gpusim::Readback,
}

impl Pending {
    /// Le résultat est-il arrivé ? Ne bloque pas.
    pub fn ready(&self) -> bool {
        self.packed.ready()
    }
}

// ---------- mesures ----------

/// Allongement des arêtes par rapport au repos : (le pire, la moyenne), 0,05 = 5 % trop long.
pub fn stretch(edges: &[[u32; 2]], rest: &[f32], p: &[Vec<f32>; 3]) -> (f32, f32) {
    let (mut worst, mut sum, mut count) = (0.0f32, 0.0f64, 0usize);
    for (e, r) in edges.iter().zip(rest) {
        let (a, b) = (e[0] as usize, e[1] as usize);
        if *r < 1e-9 || a >= p[0].len() || b >= p[0].len() {
            continue;
        }
        let l = ((p[0][a] - p[0][b]).powi(2) + (p[1][a] - p[1][b]).powi(2) + (p[2][a] - p[2][b]).powi(2)).sqrt();
        let s = l / r - 1.0;
        worst = worst.max(s);
        sum += s.abs() as f64;
        count += 1;
    }
    (worst, if count > 0 { (sum / count as f64) as f32 } else { 0.0 })
}

/// Longueur au repos de chaque arête, mesurée sur ces positions.
pub fn rest_lengths(edges: &[[u32; 2]], p: &[Vec<f32>; 3]) -> Vec<f32> {
    let n = p[0].len();
    edges.iter().map(|e| {
        let (a, b) = (e[0] as usize, e[1] as usize);
        if a >= n || b >= n { 0.0 } else { ((p[0][a] - p[0][b]).powi(2) + (p[1][a] - p[1][b]).powi(2) + (p[2][a] - p[2][b]).powi(2)).sqrt() }
    }).collect()
}

/// Une grille de tissu de `nx` × `nz` sommets, de côté `size`, à la hauteur `y`, avec ses diagonales :
/// positions et arêtes. Sert aux essais et au banc de mesure.
pub fn grid(nx: usize, nz: usize, size: f32, y: f32) -> ([Vec<f32>; 3], Vec<[u32; 2]>) {
    let mut p = [Vec::with_capacity(nx * nz), Vec::with_capacity(nx * nz), Vec::with_capacity(nx * nz)];
    let mut e = Vec::with_capacity(nx * nz * 4);
    for j in 0..nz {
        for i in 0..nx {
            p[0].push((i as f32 / (nx - 1).max(1) as f32 - 0.5) * size);
            p[1].push(y);
            p[2].push((j as f32 / (nz - 1).max(1) as f32 - 0.5) * size * (nz - 1).max(1) as f32 / (nx - 1).max(1) as f32);
            let a = (j * nx + i) as u32;
            if i + 1 < nx {
                e.push([a, a + 1]);
            }
            if j + 1 < nz {
                e.push([a, a + nx as u32]);
            }
            if i + 1 < nx && j + 1 < nz {
                e.push([a, a + nx as u32 + 1]);
                e.push([a + 1, a + nx as u32]);
            }
        }
    }
    (p, e)
}

// ---------- comparaison à budget égal ----------

/// Un solveur de référence branché de l'extérieur (le XPBD du processeur) : positions, vitesses,
/// arêtes, longueurs au repos, points fixes, pas de temps, réglages, souplesse équivalente à la raideur demandée.
pub type Other<'a> = &'a dyn Fn(&mut [Vec<f32>; 3], &mut [Vec<f32>; 3], &[[u32; 2]], &[f32], &[bool], f32, &Params, f32);

/// Une ligne de la comparaison : un solveur, un budget de passes par image.
#[derive(Clone, Debug)]
pub struct Row {
    /// « XPBD », « VBD », « VBD + niveaux »
    pub solver: &'static str,
    pub budget: usize,
    /// Ce que le pas coûte vraiment, en passes du niveau fin (octets lus et écrits) : les
    /// corrections grossières sont comptées, le budget est donc le même pour tous
    pub cost: f32,
    pub substeps: usize,
    /// Tissu pendu : allongement total sous son poids (0,01 = 1 %)
    pub elongation: f32,
    /// Raideur obtenue, en part de la raideur demandée (1 : convergé)
    pub stiffness: f32,
    /// Tissu qui tombe sur une boule : pire allongement d'une arête pendant la chute, et moyenne à la fin
    pub worst: f32,
    pub mean: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Comparison {
    pub points: usize,
    pub stiffness: f32,
    /// Niveaux grossiers du tissu comparé
    pub levels: usize,
    /// Tissu pendu calculé jusqu'à convergence : allongement et énergie élastique
    pub reference: (f32, f32),
    pub rows: Vec<Row>,
}

/// Le découpage conseillé pour un budget par image, compté en passes du niveau fin : les mêmes
/// réglages avec autant de sous-pas d'une passe que le budget en paie, tout compris (correction
/// grossière et amortissement interne s'ils sont actifs). Sans niveaux, cela fait beaucoup de
/// sous-pas ; avec, chaque sous-pas coûte plus cher et il y en a moins.
/// (Une correction seulement de temps en temps a été essayée : elle rattrape d'un coup l'erreur
/// de plusieurs sous-pas, ce qui se retrouve dans les vitesses, et le tissu rebondit.)
pub fn split_budget(t: &Topo, budget: usize, p: &Params) -> Params {
    let one = Params { iterations: 1, substeps: 1, ..*p };
    let sub = ((budget as f64 / t.substep_cost(&one)).round() as usize).clamp(1, 32);
    Params { iterations: sub, substeps: sub, ..*p }
}

/// VBD (sans, puis avec niveaux grossiers) contre un autre solveur, à nombre égal de passes par
/// image, sur un tissu de `n` × `n` sommets et de 2 m de côté. Deux scènes : le tissu pendu par un
/// bord (quelle raideur obtient-on vraiment ?), et le tissu qui tombe sur une boule (de combien
/// s'étire-t-il ?). Le VBD tourne ici sur le processeur : la carte donne le même résultat (essai `carte_contre_processeur`).
pub fn compare(n: usize, budgets: &[usize], other: Other) -> Comparison {
    let k = 1000.0;
    let dt = 1.0 / 24.0;
    let dx = 2.0 / (n - 1) as f32;
    // Même problème pour l'autre solveur, qui compte en masses unité : souplesse = masse / raideur
    let compliance = DENSITY * dx * dx / k;
    let pinned: Vec<bool> = (0..n * n).map(|i| i / n == 0).collect();
    let (flat, edges) = grid(n, n, 2.0, 1.6);
    let rest = rest_lengths(&edges, &flat);
    // Le même tissu, pendu : la première rangée en haut, les autres dessous
    let hung = [flat[0].clone(), flat[2].iter().map(|z| 2.0 - (z + 1.0)).collect(), vec![0.0; n * n]];
    let build = |start: &[Vec<f32>; 3]| Topo::build_with(n * n, &edges, &rest, start, &pinned, None, true).with_levels(MAX_LEVELS);
    let run = |vbd: bool, start: &[Vec<f32>; 3], par: &Params, frames: usize, see: &mut dyn FnMut(usize, &[Vec<f32>; 3])| {
        let mut pos = start.clone();
        let mut vel = [vec![0.0; n * n], vec![0.0; n * n], vec![0.0; n * n]];
        let mut cloth = vbd.then(|| Cloth::new(build(start), start, false));
        for f in 0..frames {
            for (i, y) in vel[1].iter_mut().enumerate() {
                if !pinned[i] {
                    *y -= 9.8 * dt;
                }
            }
            match cloth.as_mut() {
                Some(c) => {
                    c.push_velocities(&vel);
                    c.step(dt, par);
                    (pos, vel) = c.read();
                }
                None => other(&mut pos, &mut vel, &edges, &rest, &pinned, dt, par, compliance),
            }
            see(f, &pos);
        }
        pos
    };
    let elongation = |p: &[Vec<f32>; 3]| {
        let bottom: f32 = p[1][n * (n - 1)..].iter().sum::<f32>() / n as f32;
        (2.0 - bottom) / 2.0 - 1.0
    };
    let energy = |p: &[Vec<f32>; 3]| {
        edges.iter().zip(&rest).map(|(e, r)| {
            let (a, b) = (e[0] as usize, e[1] as usize);
            let l = ((p[0][a] - p[0][b]).powi(2) + (p[1][a] - p[1][b]).powi(2) + (p[2][a] - p[2][b]).powi(2)).sqrt();
            0.5 * k * (l - r) * (l - r)
        }).sum::<f32>()
    };
    // Tissu pendu : 72 images pour qu'il se pose, mesure moyennée sur les 12 dernières
    let hang = |vbd: bool, par: &Params| {
        let (mut e, mut j) = (0.0, 0.0);
        run(vbd, &hung, par, 72, &mut |f, p| {
            if f >= 60 {
                e += elongation(p) / 12.0;
                j += energy(p) / 12.0;
            }
        });
        (e, j)
    };
    // Diagonales aussi raides que les arêtes : on mesure ici l'étirement, pas le cisaillement
    let still = Params { stiffness: k, shear: 1.0, bending: 0.0, damping: 0.3, ..Default::default() };
    let fall = Params { stiffness: k, shear: 1.0, bending: 0.0, damping: 0.02, floor: Some(0.0), ball: Some(([0.0, 0.7, 0.0], 0.55)), ..Default::default() };
    let topo = build(&flat);
    // Référence : le tissu pendu, avec assez de passes pour que le résultat ne bouge plus
    let mut out = Comparison { points: n * n, stiffness: k, levels: topo.levels(), reference: hang(true, &Params { iterations: 2048, substeps: 16, ..still }), rows: vec![] };
    for &budget in budgets {
        for (solver, vbd, levels) in [("XPBD", false, 0), ("VBD", true, 0), ("VBD + niveaux", true, MAX_LEVELS)] {
            if levels > 0 && topo.levels() == 0 {
                continue;
            }
            // L'autre solveur découpe lui-même ses passes en sous-pas ; le VBD prend le découpage qui lui réussit le mieux
            let with = |p: Params| if vbd { split_budget(&topo, budget, &Params { levels, ..p }) } else { Params { iterations: budget, substeps: substeps_for(budget), ..p } };
            let substeps = with(fall).substeps;
            let cost = if vbd { (substeps as f64 * topo.substep_cost(&with(fall))) as f32 } else { budget as f32 };
            let e = hang(vbd, &with(still)).0;
            let mut worst = 0.0f32;
            let q = run(vbd, &flat, &with(fall), 60, &mut |_, p| worst = worst.max(stretch(&edges, &rest, p).0));
            out.rows.push(Row { solver, budget, cost, substeps, elongation: e, stiffness: if e > 1e-9 { out.reference.0 / e } else { 1.0 }, worst, mean: stretch(&edges, &rest, &q).1 });
        }
    }
    out
}

/// Nombre de sous-pas conseillé pour un budget de passes par image, sans niveaux grossiers.
pub fn substeps_for(budget: usize) -> usize {
    budget.clamp(1, 32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un tissu accroché par un bord, avec ses réglages d'essai
    fn hung(n: usize, on_gpu: bool) -> (Cloth, Vec<[u32; 2]>, Vec<f32>) {
        let (p, e) = grid(n, n, 2.0, 1.6);
        let rest = rest_lengths(&e, &p);
        let pinned: Vec<bool> = (0..n * n).map(|i| i / n == 0).collect();
        let topo = Topo::build_with(n * n, &e, &rest, &p, &pinned, None, true).with_levels(MAX_LEVELS);
        (Cloth::new(topo, &p, on_gpu), e, rest)
    }

    /// Avance de `frames` images sous la gravité
    fn fall(c: &mut Cloth, frames: usize, par: &Params) -> [Vec<f32>; 3] {
        for _ in 0..frames {
            let (_, mut v) = c.read();
            v[1].iter_mut().for_each(|y| *y -= 9.8 / 24.0);
            c.push_velocities(&v);
            c.step(1.0 / 24.0, par);
        }
        c.read().0
    }

    fn worst_gap(a: &[Vec<f32>; 3], b: &[Vec<f32>; 3]) -> f32 {
        (0..3).flat_map(|k| a[k].iter().zip(&b[k]).map(|(x, y)| (x - y).abs())).fold(0.0, f32::max)
    }

    #[test]
    fn topologie_rangee_et_coloriee() {
        let (c, e, rest) = hung(12, false);
        let t = &c.topo;
        let n = 144;
        // Rangement : une vraie permutation, et son inverse
        let mut seen = vec![false; n];
        for (k, &i) in t.perm.iter().enumerate() {
            assert!(!std::mem::replace(&mut seen[i as usize], true));
            assert_eq!(t.inv[i as usize], k as u32);
        }
        // Chaque arête apparaît chez ses deux sommets, avec sa longueur au repos à 0,01 % près
        assert_eq!(t.springs.len(), e.len() * 2);
        for k in 0..n {
            for sp in &t.springs[t.off[k][0] as usize..][..(t.off[k][1] & 0xffff) as usize] {
                let (a, b) = (t.perm[k], t.perm[(sp[0] & RANK) as usize]);
                let id = e.iter().position(|x| *x == [a, b] || *x == [b, a]).expect("un ressort sans arête");
                assert!((t.rest(sp[2] & 0xffff) / rest[id] - 1.0).abs() < 1e-4);
            }
        }
        // Une grille avec diagonales : quatre couleurs
        assert!(t.trips > 0 && t.phases == 4, "{} couleurs", t.phases);
        // Les diagonales sont reconnues : deux par carré, et aucune arête droite
        let diagonals = t.springs.iter().filter(|sp| sp[0] & DIAG != 0).count();
        assert_eq!(diagonals, 2 * 2 * 11 * 11);
        for k in 0..n {
            for sp in &t.springs[t.off[k][0] as usize..][..(t.off[k][1] & 0xffff) as usize] {
                let long = t.rest(sp[2] & 0xffff) > 2.0 / 11.0 * 1.2;
                assert_eq!(sp[0] & DIAG != 0, long);
            }
        }
        let colour = |k: usize| t.off[k][1] >> 16;
        // Les plages couvrent tous les sommets libres ; les 12 fixes sont après
        let covered: u32 = t.groups.iter().map(|g| g[1]).sum();
        assert_eq!(covered as usize, n - 12);
        assert_eq!((0..n).filter(|k| colour(*k) == FIXED).count(), 12);
        let group_of = |k: u32| t.groups.iter().position(|g| k >= g[0] && k < g[0] + g[1]);
        let mut flex = 0;
        for k in 0..n {
            assert_eq!(colour(k) == FIXED, t.mass[k] == 0.0);
            let list = &t.springs[t.off[k][0] as usize..][..(t.off[k][1] & 0xffff) as usize];
            for (a, sp) in list.iter().enumerate() {
                let near = sp[0] & RANK;
                // Aucun voisin (ressort ou flexion) de la même couleur : c'est ce qui permet de les traiter ensemble
                assert!(colour(k) == FIXED || colour(near as usize) != colour(k));
                // Donc aucun dans la même plage : personne ne l'écrit pendant qu'on le lit
                let apart = |j: u32| group_of(j) != group_of(k as u32) || group_of(j).is_none();
                assert!(apart(near));
                // Le sommet au-delà du voisin peut être de la même couleur : il est lu dans les positions du début de la passe
                if sp[1] != NONE {
                    flex += 1;
                }
                if sp[0] & PAIR != 0 {
                    assert!(a + 1 < list.len() && list[a + 1][0] & PAIR == 0);
                    flex += 1;
                }
            }
        }
        // Chaque terme de flexion est vu de ses trois sommets
        assert_eq!(flex, t.trips * 3);
    }

    /// Le rangement compact divise presque par deux les octets lus par passe.
    #[test]
    fn octets_par_passe() {
        let (p, e) = grid(96, 96, 2.0, 1.6);
        let rest = rest_lengths(&e, &p);
        let t = Topo::build_with(96 * 96, &e, &rest, &p, &[], None, true);
        let per = |b: u64| b as f64 / t.n as f64;
        let before = per(t.bytes_per_pass_before());
        assert!(before > 850.0 && before < 920.0, "{before}");
        let now = per(t.bytes_per_pass(true, true));
        assert!(now < before * 0.55, "{now} octets par sommet contre {before}");
    }

    /// Les niveaux grossiers : poids, matrices, et ce qu'ils gardent du niveau d'au-dessus.
    #[test]
    fn niveaux_grossiers_coherents() {
        let (c, _, _) = hung(40, false);
        let t = &c.topo;
        let co = t.coarse.as_ref().expect("des niveaux");
        assert!(t.levels() >= 3, "{} niveaux", t.levels());
        assert_eq!(co.up.len(), t.n);
        assert_eq!(co.par.len(), co.vert.len());
        let f = f32::from_bits;
        let mut mass: f32 = t.mass.iter().sum();
        let mut above = (0, t.n);
        for (l, lv) in co.levels.iter().enumerate() {
            let (start, n) = (lv[0] as usize, lv[1] as usize);
            assert!(n * 2 < above.1, "{n} sommets pour {}", above.1);
            // Les niveaux se suivent sans trou
            assert_eq!(start, if l == 0 { 0 } else { above.0 + above.1 });
            let inside = |a: u32| (a as usize) >= start && (a as usize) < start + n;
            let mut total = 0.0;
            let mut kids = 0;
            for a in start..start + n {
                let v = co.vert[a];
                let list = &co.ent[v[3] as usize..][..v[4] as usize];
                // Les voisins sont du même niveau, et jamais le sommet lui-même
                assert!(list.iter().all(|e| inside(e[0]) && e[0] as usize != a));
                // Étirement : la diagonale domine (un déplacement d'ensemble ne coûte que près des bords fixes)
                let s_off: f32 = list.iter().map(|e| f(e[2])).sum();
                assert!(f(v[1]) > 0.0 && f(v[1]) + s_off > -1e-3 * f(v[1]), "ligne {a} : {} + {s_off}", f(v[1]));
                total += f(v[0]) + list.iter().map(|e| f(e[1])).sum::<f32>();
                // Ses enfants sont dans le niveau d'au-dessus, et le comptent parmi leurs parents avec le même poids
                for kid in &co.kids[v[5] as usize..][..v[6] as usize] {
                    let i = kid[0] as usize;
                    assert!(i >= above.0 && i < above.0 + above.1);
                    let u = if l == 0 { co.up[i] } else { co.par[i] };
                    assert!((0..4).any(|k| u[k] as usize == a && u[4 + k] == kid[1]));
                    kids += 1;
                }
            }
            // Chaque sommet d'au-dessus suit des sommets de ce niveau, avec des poids dont la somme ne dépasse pas 1
            let ups: &[[u32; 8]] = if l == 0 { &co.up } else { &co.par[above.0..above.0 + above.1] };
            let mut links = 0;
            for u in ups {
                let w: f32 = (0..4).map(|k| f(u[4 + k])).sum();
                assert!((0.0..=1.0 + 1e-5).contains(&w));
                for k in 0..4 {
                    if f(u[4 + k]) > 0.0 {
                        assert!(inside(u[k]));
                        links += 1;
                    }
                }
            }
            assert_eq!(links, kids);
            // La masse vue par le niveau grossier ne dépasse pas celle d'au-dessus (elle baisse un peu près des bords fixes)
            assert!(total <= mass * 1.0001 && total > mass * 0.7, "masse {total} pour {mass}");
            mass = total;
            above = (start, n);
        }
        // Le dernier niveau n'a pas de parents
        assert!(co.par[above.0..].iter().all(|u| u.iter().all(|x| *x == 0)));
    }

    /// Tissu pendu : à nombre de passes égal, la correction grossière donne un tissu bien plus raide.
    #[test]
    fn niveaux_raidissent_a_petit_budget() {
        let n = 32;
        let sag = |par: &Params| {
            let (mut c, _, _) = hung(n, false);
            // Le même tissu, pendu : la première rangée en haut, les autres dessous
            let (flat, _) = grid(n, n, 2.0, 1.6);
            c.set_positions(&[flat[0].clone(), flat[2].iter().map(|z| 1.0 - z).collect(), vec![0.0; n * n]]);
            let p = fall(&mut c, 48, par);
            (2.0 - p[1][n * (n - 1)..].iter().sum::<f32>() / n as f32) / 2.0 - 1.0
        };
        let base = Params { bending: 0.0, damping: 0.3, ..Default::default() };
        let exact = sag(&Params { iterations: 1024, substeps: 8, ..base });
        let plain = sag(&Params { iterations: 16, substeps: 16, ..base });
        // Quatre sous-pas d'une passe et d'une correction : le même coût que seize passes
        let multi = Params { iterations: 4, substeps: 4, levels: MAX_LEVELS, ..base };
        let (c, _, _) = hung(n, false);
        let cost = 4.0 * c.topo.substep_cost(&multi);
        assert!(cost < 19.0 && split_budget(&c.topo, 16, &multi).substeps >= 3, "coût {cost} passes");
        let levels = sag(&multi);
        assert!(exact > 0.0 && plain > exact * 8.0, "sans niveaux : {plain} pour {exact}");
        assert!(levels < exact * 2.5 && levels > exact * 0.9, "avec niveaux : {levels} pour {exact}");
    }

    /// La correction grossière ne fait jamais diverger le tissu : il se balance, se plie, se
    /// comprime et tombe sur des obstacles avec peu de sous-pas, et reste fini et peu étiré.
    #[test]
    fn niveaux_stables_tissu_plie() {
        for (floor, ball) in [(None, None), (Some(0.0), Some(([0.0, 0.7, 0.0], 0.55)))] {
            for (it, sub, bending) in [(2, 2, 0.0), (4, 4, 1e-4), (8, 4, 1e-2)] {
                let (mut c, e, rest) = hung(28, false);
                let par = Params { iterations: it, substeps: sub, bending, floor, ball, levels: MAX_LEVELS, ..Default::default() };
                let mut worst = 0.0f32;
                for _ in 0..12 {
                    let p = fall(&mut c, 5, &par);
                    assert!(p.iter().all(|a| a.iter().all(|x| x.is_finite())));
                    worst = worst.max(stretch(&e, &rest, &p).0);
                }
                assert!(worst < 1.0, "arête étirée de {} % ({it} passes, {sub} sous-pas)", worst * 100.0);
            }
        }
    }

    #[test]
    fn tchebychev_bride_quand_peu_d_iterations() {
        let p = |it, sub| Params { iterations: it, substeps: sub, rho: 0.9, ..Default::default() };
        assert_eq!(p(16, 8).rho_used(), 0.0);
        assert!(p(64, 8).rho_used() > 0.5 && p(64, 8).rho_used() < 0.9);
        assert_eq!(p(256, 4).rho_used(), 0.9);
        assert_eq!(omega(1, 1.0, 0.9), 1.0);
        assert!(omega(2, 1.0, 0.9) > 1.0 && omega(3, omega(2, 1.0, 0.9), 0.9) < omega(2, 1.0, 0.9));
    }

    /// Le tissu tombe sur une boule et une boîte posées sur le sol : il ne les traverse pas, ne
    /// s'étire pas démesurément, et reste fini.
    #[test]
    fn tombe_et_drape_sans_traverser() {
        let (mut c, e, rest) = hung(24, false);
        let (ball, cube) = (([0.3, 0.7, 0.2], 0.5), ([-0.6, 0.3, 0.0], [0.25, 0.3, 0.4]));
        let par = Params { floor: Some(0.0), ball: Some(ball), cube: Some(cube), ..Default::default() };
        let p = fall(&mut c, 60, &par);
        let mut touched = 0;
        for i in 0..p[0].len() {
            let q = [p[0][i], p[1][i], p[2][i]];
            assert!(q.iter().all(|x| x.is_finite()));
            assert!(q[1] >= -1e-4, "sous le sol : {q:?}");
            let d = ((q[0] - ball.0[0]).powi(2) + (q[1] - ball.0[1]).powi(2) + (q[2] - ball.0[2]).powi(2)).sqrt();
            assert!(d >= ball.1 - 1e-3, "dans la boule : {d}");
            touched += (d < ball.1 + 1e-3) as usize;
            let inside = (0..3).all(|k| (q[k] - cube.0[k]).abs() < cube.1[k] - 1e-3);
            assert!(!inside, "dans la boîte : {q:?}");
        }
        assert!(touched > 10, "le tissu devrait reposer sur la boule ({touched} points)");
        let (worst, mean) = stretch(&e, &rest, &p);
        assert!(worst < 0.6 && mean < 0.05, "allongement {worst} / {mean}");
    }

    /// Plus de passes, moins d'allongement : la descente par blocs converge.
    #[test]
    fn plus_de_passes_moins_d_allongement() {
        let run = |it, sub| {
            let (mut c, e, rest) = hung(20, false);
            let p = fall(&mut c, 36, &Params { stiffness: 1e5, iterations: it, substeps: sub, ..Default::default() });
            stretch(&e, &rest, &p).1
        };
        let (few, some, many) = (run(8, 4), run(32, 8), run(128, 16));
        assert!(few > some * 1.5 && some > many * 1.5, "{few} {some} {many}");
    }

    /// Positions relatives : le même tissu posé à un kilomètre de l'origine se simule pareil.
    #[test]
    fn loin_de_l_origine_meme_resultat() {
        let run = |shift: [f32; 3]| {
            let (mut p, e) = grid(16, 16, 2.0, 1.6);
            for k in 0..3 {
                p[k].iter_mut().for_each(|x| *x += shift[k]);
            }
            let rest = rest_lengths(&grid(16, 16, 2.0, 1.6).1, &grid(16, 16, 2.0, 1.6).0);
            let pinned: Vec<bool> = (0..256).map(|i| i / 16 == 0).collect();
            let mut c = Cloth::new(Topo::build(256, &e, &rest, &p, &pinned, None), &p, false);
            let par = Params { floor: Some(shift[1]), ball: Some(([shift[0], shift[1] + 0.7, shift[2]], 0.5)), ..Default::default() };
            let mut out = fall(&mut c, 24, &par);
            for k in 0..3 {
                out[k].iter_mut().for_each(|x| *x -= shift[k]);
            }
            out
        };
        let gap = worst_gap(&run([0.0; 3]), &run([1000.0, 200.0, -500.0]));
        // À un kilomètre, un f32 ne distingue plus que 0,06 mm : l'écart vient de l'aller-retour, que la
        // chute sur la boule amplifie, pas du calcul (fait en positions absolues, il dépasserait le décimètre)
        assert!(gap < 1.5e-2, "écart {gap}");
    }

    /// La règle du socle : le noyau de la carte et sa version processeur donnent le même tissu,
    /// avec et sans niveaux grossiers.
    #[test]
    fn carte_contre_processeur() {
        if gpusim::gpu().is_none() {
            return;
        }
        // Peu d'itérations (sans accélération), beaucoup (avec), puis avec un, deux, trois et tous les niveaux
        for (it, sub, levels) in [(16, 8, 0), (48, 4, 0), (4, 4, 1), (4, 4, 2), (4, 4, 3), (8, 4, MAX_LEVELS)] {
            // Les niveaux grossiers d'un seul lancement, puis un lancement par étape (comme sur un grand tissu), puis moitié-moitié
            for tail in if levels > 0 { vec![TAIL, 0, 30] } else { vec![TAIL] } {
                for obstacles in [false, true] {
                    let mut par = Params { iterations: it, substeps: sub, levels, ..Default::default() };
                    if obstacles {
                        (par.floor, par.ball, par.cube) = (Some(0.0), Some(([0.3, 0.9, 0.2], 0.5)), Some(([-0.6, 1.0, 0.0], [0.25, 0.3, 0.4])));
                    }
                    let (mut cpu, _, _) = hung(20, false);
                    let mut topo = hung(20, false).0.into_topo();
                    topo.tail = tail;
                    let mut gpu = Cloth::new(topo, &grid(20, 20, 2.0, 1.6).0, true);
                    assert!(gpu.on_gpu() && !cpu.on_gpu());
                    let gap = worst_gap(&fall(&mut cpu, 8, &par), &fall(&mut gpu, 8, &par));
                    // Sans obstacle, le même calcul à l'arrondi près. Un contact qui bascule amplifie
                    // l'arrondi, davantage quand la correction grossière le propage à tout le tissu.
                    let allowed = if !obstacles { 2e-5 } else if levels == 0 { 5e-4 } else { 3e-3 };
                    assert!(gap < allowed, "écart {gap} ({it} passes, {sub} sous-pas, {levels} niveaux, petits niveaux sous {tail}, obstacles : {obstacles})");
                    assert!(gpu.on_gpu());
                }
            }
        }
    }

    /// Le nombre de lancements d'une image : c'est ce qui coûte sur un petit tissu.
    #[test]
    fn lancements_comptes() {
        if gpusim::gpu().is_none() {
            return;
        }
        let run = |par: &Params, tail: usize| {
            let mut topo = hung(20, false).0.into_topo();
            topo.tail = tail;
            let levels = topo.coarse.as_ref().map_or(0, |c| c.levels.iter().filter(|l| l[1] as usize > tail).count());
            let mut c = Cloth::new(topo, &grid(20, 20, 2.0, 1.6).0, true);
            c.step(1.0 / 24.0, par);
            (c.launches(), c.topo.colors(), levels, c.topo.levels())
        };
        // Sans niveaux : par sous-pas, la prédiction et une passe par couleur (la dernière écrit la vitesse)
        let (count, colors, _, _) = run(&Params { iterations: 8, substeps: 8, ..Default::default() }, TAIL);
        assert_eq!((count, colors), (8 * (1 + 4), 4));
        // Avec l'accélération : un lancement de plus par passe
        let (count, _, _, _) = run(&Params { iterations: 32, substeps: 4, ..Default::default() }, TAIL);
        assert_eq!(count, 4 * (1 + 8 * 5));
        // Avec les niveaux, tous petits : trois lancements de plus par sous-pas (force restante, cycle, remontée)
        let par = Params { iterations: 4, substeps: 4, levels: MAX_LEVELS, ..Default::default() };
        assert_eq!(run(&par, TAIL).0, 4 * (1 + 3 + 4));
        // Un grand niveau coûte quatre lancements, le reste toujours un seul
        let (count, _, big, all) = run(&par, 30);
        assert!(big >= 1 && big < all, "{big} grands niveaux sur {all}");
        assert_eq!(count, 4 * (1 + 3 + 4 * big + 4));
    }

    /// Un tissu debout, un peu penché, lâché sur le sol : il s'affaisse et se replie sur lui-même.
    /// Rend le tissu, sa position de départ, et la distance entre deux sommets au repos.
    fn folding(n: usize, on_gpu: bool) -> (Cloth, [Vec<f32>; 3], [Vec<f32>; 3]) {
        let (flat, e) = grid(n, n, 1.6, 0.0);
        let rest = rest_lengths(&e, &flat);
        let start = [flat[0].clone(), flat[2].iter().map(|z| 0.1 + (z + 0.8) * 0.98).collect(), flat[2].iter().map(|z| (z + 0.8) * 0.2).collect()];
        (Cloth::new(Topo::build(n * n, &e, &rest, &flat, &[], None), &start, on_gpu), start, flat)
    }

    /// Plus petite distance entre deux sommets qui ne sont pas proches au repos
    fn closest(c: &Cloth, flat: &[Vec<f32>; 3], x: &[Vec<f32>; 3], thick: f32) -> f32 {
        let n = c.topo.n;
        let mut best = f32::MAX;
        for a in 0..n {
            for b in a + 1..n {
                let r2 = (flat[0][a] - flat[0][b]).powi(2) + (flat[2][a] - flat[2][b]).powi(2);
                if r2 >= (REST_APART * thick).powi(2) {
                    best = best.min(((x[0][a] - x[0][b]).powi(2) + (x[1][a] - x[1][b]).powi(2) + (x[2][a] - x[2][b]).powi(2)).sqrt());
                }
            }
        }
        best
    }

    /// Auto-collision : le tissu qui se replie sur lui-même garde ses couches à une épaisseur
    /// l'une de l'autre ; sans elle, elles se traversent.
    #[test]
    fn auto_collision_garde_l_epaisseur() {
        for thickness in [None, Some(0.0)] {
            let (mut c, _, flat) = folding(24, false);
            let thick = AUTO_THICK * c.topo.spacing;
            assert!((thick - AUTO_THICK * 1.6 / 23.0).abs() < 1e-4, "épaisseur {thick}");
            let par = Params { floor: Some(0.0), thickness, ..Default::default() }.with_levels();
            let mut least = f32::MAX;
            for _ in 0..10 {
                let x = fall(&mut c, 9, &par);
                assert!(x.iter().all(|a| a.iter().all(|v| v.is_finite())));
                least = least.min(closest(&c, &flat, &x, thick));
            }
            match thickness {
                Some(_) => assert!(least > 0.8 * thick, "deux couches à {least} m pour une épaisseur de {thick}"),
                None => assert!(least < 0.2 * thick, "sans auto-collision, les couches devraient se traverser ({least})"),
            }
        }
    }

    /// Les voisins proches : les mêmes qu'une recherche exhaustive, quel que soit l'ordre de
    /// rangement dans les cases de la grille (sur la carte, il change d'une fois à l'autre).
    #[test]
    fn voisins_proches_deterministes() {
        let (mut c, _, _) = folding(20, false);
        let par = Params { floor: Some(0.0), thickness: Some(0.0), ..Default::default() }.with_levels();
        fall(&mut c, 40, &par);
        let k = Consts::new(&par, 1.0 / 24.0, c.origin, &c.topo);
        let (t, pos) = (&c.topo, &c.pos);
        let mut g = Grid::default();
        grid_build(pos, &k, &mut g);
        let lists: Vec<[u32; NEAR]> = (0..t.n).map(|i| grid_near(t, i, pos, &k, &g)).collect();
        assert!(lists.iter().filter(|l| l[0] != NONE).count() > 50, "le tissu devrait s'être replié");
        // Recherche exhaustive
        for i in 0..t.n {
            let mut all: Vec<(f32, u32)> = (0..t.n as u32).filter(|j| *j as usize != i).filter_map(|j| {
                let (a, b, ra, rb) = (pos[i], pos[j as usize], t.rest_pos[i], t.rest_pos[j as usize]);
                let d2 = (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);
                let r2 = (ra[0] - rb[0]).powi(2) + (ra[1] - rb[1]).powi(2) + (ra[2] - rb[2]).powi(2);
                (d2 < k.reach * k.reach && r2 >= (REST_APART * k.thick).powi(2)).then_some((d2, j))
            }).collect();
            all.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mut want: Vec<u32> = all.iter().take(NEAR).map(|x| x.1).collect();
            want.sort_unstable();
            want.resize(NEAR, NONE);
            assert_eq!(lists[i].to_vec(), want, "sommet {i}");
        }
        // Chaque case rangée à l'envers : les mêmes listes
        for b in 0..g.starts.len() - 1 {
            g.sorted[g.starts[b] as usize..g.starts[b + 1] as usize].reverse();
        }
        for i in 0..t.n {
            assert_eq!(grid_near(t, i, pos, &k, &g), lists[i]);
        }
    }

    /// Auto-collision sur la carte : une moitié du tissu repliée au-dessus de l'autre lui tombe
    /// dessus. Le même tissu que sur le processeur, et les couches gardées à distance.
    #[test]
    fn auto_collision_carte_contre_processeur() {
        if gpusim::gpu().is_none() {
            return;
        }
        let n = 20;
        let (flat, e) = grid(n, n, 1.6, 0.0);
        let rest = rest_lengths(&e, &flat);
        let thick = AUTO_THICK * 1.6 / (n - 1) as f32;
        // La moitié z > 0 est rabattue sur l'autre, une épaisseur et demie au-dessus
        let start = [flat[0].clone(), flat[2].iter().map(|z| (z * 2.0).clamp(0.0, 1.5 * thick)).collect(), flat[2].iter().map(|z| -z.abs()).collect()];
        for levels in [0, MAX_LEVELS] {
            let par = Params { iterations: 16, substeps: 8, floor: Some(0.0), thickness: Some(0.0), levels, ..Default::default() };
            let build = |on_gpu: bool| Cloth::new(Topo::build(n * n, &e, &rest, &flat, &[], None), &start, on_gpu);
            let (mut cpu, mut gpu) = (build(false), build(true));
            assert!(gpu.on_gpu());
            let (a, b) = (fall(&mut cpu, 6, &par), fall(&mut gpu, 6, &par));
            assert!(gpu.on_gpu(), "la carte a rendu une erreur");
            // Les deux couches se touchent, et se tiennent à distance
            let least = closest(&cpu, &flat, &a, thick);
            assert!(least < thick && least > 0.8 * thick, "couches à {least} m pour une épaisseur de {thick}");
            let gap = worst_gap(&a, &b);
            assert!(gap < 1e-3, "écart {gap} ({levels} niveaux)");
            let x = fall(&mut gpu, 30, &par);
            assert!(gpu.on_gpu());
            let least = closest(&gpu, &flat, &x, thick);
            assert!(least > 0.8 * thick, "deux couches à {least} m pour une épaisseur de {thick}");
            // Cinq lancements de plus par sous-pas : la grille et les voisins
            let plain = if levels > 0 { 1 + 3 + 2 * 4 } else { 1 + 2 * 4 };
            assert_eq!(gpu.launches(), 8 * (plain + 5));
        }
    }

    /// Si la carte ne rend pas son résultat, le pas est refait sur le processeur, sans rien perdre.
    #[test]
    fn repli_sur_le_processeur() {
        if gpusim::gpu().is_none() {
            return;
        }
        let par = Params::default();
        let (mut cpu, _, _) = hung(12, false);
        let (mut gpu, _, _) = hung(12, true);
        let want = fall(&mut cpu, 3, &par);
        fall(&mut gpu, 2, &par);
        let (_, mut v) = gpu.read();
        v[1].iter_mut().for_each(|y| *y -= 9.8 / 24.0);
        gpu.push_velocities(&v);
        gpu.step(1.0 / 24.0, &par);
        // La relecture échoue : on fait comme si la carte n'avait rien rendu
        gpu.accept(None);
        assert!(!gpu.on_gpu());
        let got = gpu.read().0;
        assert!(worst_gap(&want, &got) < 5e-4);
        // Et la suite se fait sur le processeur
        fall(&mut gpu, 1, &par);
        assert!(gpu.read().0[1].iter().all(|y| y.is_finite()));
    }

    /// Relecture sans attendre : on envoie le pas, on demande le résultat, on tourne jusqu'à ce qu'il soit là.
    #[test]
    fn relecture_sans_attendre() {
        if gpusim::gpu().is_none() {
            return;
        }
        let (mut c, _, _) = hung(12, true);
        c.step(1.0 / 24.0, &Params::default());
        let pending = c.request().expect("un pas est en attente");
        let t0 = std::time::Instant::now();
        while !pending.ready() && t0.elapsed().as_secs() < 30 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(pending.ready());
        c.accept(Some(pending));
        assert!(c.on_gpu() && c.request().is_none());
    }
}
