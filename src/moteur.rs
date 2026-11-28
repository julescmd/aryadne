//! Mode serveur du moteur : `aryadne --moteur`, sans interface.
//!
//! Un autre programme (l'add-on Blender) pilote le rasteriseur « pixols » d'Aryadne :
//! commandes en lignes JSON sur l'entrée standard, une réponse JSON par ligne sur la sortie
//! standard. Les données lourdes passent par des fichiers projetés en mémoire (memmap2) :
//! - à l'aller, le client écrit sommets et triangles dans un fichier et en donne le chemin ;
//! - au retour, le moteur écrit chaque image (RGBA8 et profondeur f32) dans un fichier à deux
//!   cases : l'image n va dans la case n % 2, pendant que le client lit encore l'autre.
//!
//! La caméra est donnée par des matrices quelconques (celles de Blender telles quelles), et la
//! profondeur rendue suit la convention de la fenêtre OpenGL de Blender : 0 au plan proche,
//! 1 au plan lointain, lignes de bas en haut. Le protocole complet est décrit dans
//! docs/technique/blender.md.
use crate::model3d::{persp_bary, vertex_normals, visibility_pass};
use memmap2::{Mmap, MmapMut};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

type V3 = [f32; 3];
/// Matrice 4 × 4 rangée par lignes : m[ligne][colonne], vecteurs colonnes (comme mathutils)
pub type M4 = [[f32; 4]; 4];

/// Version du protocole : augmente quand une commande change de sens
pub const PROTOCOLE: u32 = 1;
/// Signature au début du fichier d'images
pub const MAGIE: &[u8; 4] = b"ARYI";
/// Taille de l'en-tête du fichier d'images
pub const ENTETE: usize = 64;

const IDENTITE: M4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

fn mul(a: &M4, b: &M4) -> M4 {
    let mut r = [[0.0f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            r[i][j] = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

/// Inverse transposée de la partie 3 × 3 (pour les normales) ; identité si la matrice est plate
fn normal_matrix(m: &M4) -> [V3; 3] {
    let a = [[m[0][0], m[0][1], m[0][2]], [m[1][0], m[1][1], m[1][2]], [m[2][0], m[2][1], m[2][2]]];
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0]) + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    if det.abs() < 1e-20 {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    let i = 1.0 / det;
    // Cofacteurs : (A⁻¹)ᵀ = cof(A) / det
    [
        [(a[1][1] * a[2][2] - a[1][2] * a[2][1]) * i, (a[1][2] * a[2][0] - a[1][0] * a[2][2]) * i, (a[1][0] * a[2][1] - a[1][1] * a[2][0]) * i],
        [(a[0][2] * a[2][1] - a[0][1] * a[2][2]) * i, (a[0][0] * a[2][2] - a[0][2] * a[2][0]) * i, (a[0][1] * a[2][0] - a[0][0] * a[2][1]) * i],
        [(a[0][1] * a[1][2] - a[0][2] * a[1][1]) * i, (a[0][2] * a[1][0] - a[0][0] * a[1][2]) * i, (a[0][0] * a[1][1] - a[0][1] * a[1][0]) * i],
    ]
}

/// Un sommet à l'écran : x et y en pixels (origine en bas à gauche, centres des pixels en +0,5),
/// clé de profondeur pour le buffer de visibilité (-w en perspective, -z NDC en orthographique),
/// et z NDC. x vaut NaN pour un sommet derrière la caméra ou devant le plan proche.
#[inline]
pub fn projeter(mvp: &M4, p: V3, w: f32, h: f32, persp: bool) -> ([f32; 3], f32) {
    let c = |r: usize| mvp[r][0] * p[0] + mvp[r][1] * p[1] + mvp[r][2] * p[2] + mvp[r][3];
    let (x, y, z, cw) = (c(0), c(1), c(2), c(3));
    if !(cw > 1e-7) {
        return ([f32::NAN, f32::NAN, 0.0], 0.0);
    }
    let (nx, ny, nz) = (x / cw, y / cw, z / cw);
    if nz < -1.0 {
        return ([f32::NAN, f32::NAN, 0.0], nz);
    }
    let (sx, sy) = ((nx * 0.5 + 0.5) * w, (ny * 0.5 + 0.5) * h);
    ([sx, sy, if persp { -cw } else { -nz }], nz)
}

/// Un matériau simple, comme l'affichage de vue d'un matériau Blender
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat {
    /// Couleur de base linéaire et opacité
    pub c: [f32; 4],
    pub rugosite: f32,
    pub metal: f32,
}

impl Default for Mat {
    fn default() -> Self {
        Mat { c: [0.8, 0.8, 0.8, 1.0], rugosite: 0.5, metal: 0.0 }
    }
}

/// Un objet chargé : sommets et normales dans son propre repère, matrice monde à part
pub struct Objet {
    pos: Vec<V3>,
    nrm: Vec<V3>,
    tris: Vec<[u32; 3]>,
    /// Matériau de chaque triangle (vide : le premier pour tous)
    mat: Vec<u16>,
    /// Arêtes réelles de chaque triangle (bit 0 v0-v1, bit 1 v1-v2, bit 2 v2-v0) ; vide : toutes
    aretes: Vec<u8>,
    pub matrice: M4,
    /// Couleurs par emplacement de matériau (au moins une)
    pub mats: Vec<Mat>,
    /// Couleur des arêtes quand le fil de fer est montré (sRGB, opacité)
    pub fil: [f32; 4],
    pub visible: bool,
    /// Numéro écrit dans le tampon d'identifiants (choisi par le client, jamais 0)
    pub num: u32,
}

impl Objet {
    pub fn new(pos: Vec<V3>, tris: Vec<[u32; 3]>, matrice: M4, couleur: [f32; 4]) -> Objet {
        Self::complet(pos, tris, vec![], vec![], matrice, vec![Mat { c: couleur, ..Default::default() }])
    }
    /// Avec matériaux et arêtes par triangle (ignorés s'ils n'ont pas la bonne longueur)
    pub fn complet(pos: Vec<V3>, tris: Vec<[u32; 3]>, mat: Vec<u16>, aretes: Vec<u8>, matrice: M4, mats: Vec<Mat>) -> Objet {
        let n = pos.len() as u32;
        let nt = tris.len();
        let keep: Vec<bool> = tris.iter().map(|t| t.iter().all(|&i| i < n)).collect();
        fn filt<T>(v: Vec<T>, keep: &[bool]) -> Vec<T> {
            if v.len() == keep.len() { v.into_iter().zip(keep).filter(|(_, k)| **k).map(|(x, _)| x).collect() } else { vec![] }
        }
        let _ = nt;
        let mat: Vec<u16> = filt(mat, &keep);
        let aretes: Vec<u8> = filt(aretes, &keep);
        let tris: Vec<[u32; 3]> = tris.into_iter().zip(&keep).filter(|(_, k)| **k).map(|(t, _)| t).collect();
        let nrm = vertex_normals(&pos, &tris);
        let mats = if mats.is_empty() { vec![Mat::default()] } else { mats };
        Objet { pos, nrm, tris, mat, aretes, matrice, mats, fil: [0.0, 0.0, 0.0, 1.0], visible: true, num: 1 }
    }
    pub fn tri_count(&self) -> usize {
        self.tris.len()
    }
}

/// Façon d'éclairer, comme le menu « Lighting » de l'ombrage Solid de Blender
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Studio,
    Matcap,
    Plat,
    /// Fil de fer seul (ombrage Wireframe) : seules les arêtes sont écrites
    Fil,
}

/// Une lumière de studio : direction vers la lumière dans le repère de la vue, couleurs linéaires
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lampe {
    pub direction: V3,
    pub diffuse: [f32; 3],
    pub speculaire: [f32; 3],
    /// Douceur (0 : lumière nette, 1 : très enveloppante)
    pub douceur: f32,
}

/// Image de matcap en valeurs linéaires (lignes de haut en bas) : passe diffuse (teintée par la
/// couleur de l'objet) et passe spéculaire (ajoutée), comme les matcaps EXR de Blender
pub struct Matcap {
    w: usize,
    h: usize,
    diffuse: Vec<[f32; 3]>,
    speculaire: Vec<[f32; 3]>,
}

impl Matcap {
    pub fn charger(chemin: &str) -> Result<Matcap, String> {
        let p = std::path::Path::new(chemin);
        if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exr")) {
            let d = crate::nodes::load_exr_pass(p, "diffuse", 1024).or_else(|_| crate::nodes::load_exr_pass(p, "", 1024)).map_err(|e| format!("matcap illisible ({chemin}) : {e}"))?;
            let s = crate::nodes::load_exr_pass(p, "specular", 1024).ok().filter(|s| s.w == d.w && s.h == d.h);
            let rgb = |v: &[f32; 4]| [v[0], v[1], v[2]];
            return Ok(Matcap { w: d.w, h: d.h, diffuse: d.px.iter().map(rgb).collect(), speculaire: s.map(|s| s.px.iter().map(rgb).collect()).unwrap_or_default() });
        }
        let im = image::open(chemin).map_err(|e| format!("matcap illisible ({chemin}) : {e}"))?.to_rgb32f();
        let (w, h) = (im.width() as usize, im.height() as usize);
        Ok(Matcap { w, h, diffuse: im.pixels().map(|p| p.0).collect(), speculaire: vec![] })
    }
    /// Couleur pour une normale de vue (x à droite, y en haut) et une couleur de base
    fn sample(&self, n: V3, miroir: bool, base: [f32; 3]) -> [f32; 3] {
        let u = (if miroir { -n[0] } else { n[0] }) * 0.5 + 0.5;
        let v = 0.5 - n[1] * 0.5;
        let x = ((u * self.w as f32) as usize).min(self.w - 1);
        let y = ((v * self.h as f32) as usize).min(self.h - 1);
        let d = self.diffuse[y * self.w + x];
        let s = self.speculaire.get(y * self.w + x).copied().unwrap_or([0.0; 3]);
        [0, 1, 2].map(|k| d[k] * base[k] + s[k])
    }
}

/// Réglages d'ombrage envoyés par le client
#[derive(Clone)]
pub struct Eclairage {
    pub mode: Mode,
    pub lampes: Vec<Lampe>,
    pub ambiante: [f32; 3],
    pub speculaire: bool,
    pub matcap: Option<std::sync::Arc<Matcap>>,
    pub matcap_miroir: bool,
    /// Faces arrière écartées
    pub culling: bool,
    /// Arêtes dessinées par-dessus (overlay Wireframe), avec leur opacité
    pub fil: bool,
    pub fil_opacite: f32,
}

impl Default for Eclairage {
    fn default() -> Self {
        // Proche du studio « Default » de Blender
        Eclairage {
            mode: Mode::Studio,
            lampes: vec![
                Lampe { direction: norm([-0.41, 0.35, 0.84]), diffuse: [0.52, 0.54, 0.54], speculaire: [0.6; 3], douceur: 0.0 },
                Lampe { direction: norm([0.52, 0.83, 0.21]), diffuse: [0.038, 0.034, 0.05], speculaire: [0.11, 0.13, 0.16], douceur: 0.48 },
                Lampe { direction: norm([0.62, -0.56, -0.54]), diffuse: [0.09, 0.08, 0.07], speculaire: [0.1, 0.08, 0.07], douceur: 0.2 },
                Lampe { direction: norm([-0.35, 0.17, -0.92]), diffuse: [0.033; 3], speculaire: [0.27; 3], douceur: 0.53 },
            ],
            ambiante: [0.0; 3],
            speculaire: true,
            matcap: None,
            matcap_miroir: false,
            culling: false,
            fil: false,
            fil_opacite: 1.0,
        }
    }
}

/// Lit trois nombres
fn v3(v: &Value) -> Option<V3> {
    let a = v.as_array()?;
    Some([a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32])
}

/// Un matériau depuis [r, g, b, a, rugosité, métal] (ce qui manque prend la valeur par défaut)
fn mat_of(v: &Value) -> Mat {
    let a: Vec<f32> = v.as_array().map(|a| a.iter().filter_map(|x| x.as_f64()).map(|x| x as f32).collect()).unwrap_or_default();
    let d = Mat::default();
    let g = |k: usize, def: f32| a.get(k).copied().unwrap_or(def);
    Mat { c: [g(0, d.c[0]), g(1, d.c[1]), g(2, d.c[2]), g(3, 1.0)], rugosite: g(4, d.rugosite), metal: g(5, d.metal) }
}

/// La vue donnée par le client
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vue {
    pub vue: M4,
    pub projection: M4,
    pub w: usize,
    pub h: usize,
    pub persp: bool,
}

/// Fichier d'images à deux cases, partagé avec le client
pub struct Sortie {
    pub chemin: PathBuf,
    map: MmapMut,
    /// Pixels que peut contenir chaque case
    cap: usize,
}

impl Sortie {
    /// Décalage des données de la case `k` : RGBA (cap × 4 octets), profondeur (cap × 4), puis
    /// numéros d'objets (cap × u32, 0 : fond)
    pub fn decalage(cap: usize, k: usize) -> usize {
        ENTETE + k * cap * 12
    }
    fn creer(cap: usize, seq: u32) -> Result<Sortie, String> {
        let chemin = std::env::temp_dir().join(format!("aryadne-moteur-{}-{seq}.img", std::process::id()));
        let f = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&chemin).map_err(|e| e.to_string())?;
        f.set_len((ENTETE + 2 * cap * 12) as u64).map_err(|e| e.to_string())?;
        let mut map = unsafe { MmapMut::map_mut(&f) }.map_err(|e| e.to_string())?;
        map[0..4].copy_from_slice(MAGIE);
        map[4..8].copy_from_slice(&PROTOCOLE.to_le_bytes());
        map[8..12].copy_from_slice(&(cap as u32).to_le_bytes());
        Ok(Sortie { chemin, map, cap })
    }
}

/// L'état du moteur : la scène, la vue, le fichier d'images
pub struct Moteur {
    pub objets: BTreeMap<String, Objet>,
    pub vue: Option<Vue>,
    pub sortie: Option<Sortie>,
    /// Numéro de la dernière image écrite (0 : aucune)
    pub numero: u64,
    pub eclairage: Eclairage,
    matcaps: std::collections::HashMap<String, std::sync::Arc<Matcap>>,
    fichiers: u32,
    anciens: Vec<PathBuf>,
}

impl Default for Moteur {
    fn default() -> Self {
        Moteur { objets: BTreeMap::new(), vue: None, sortie: None, numero: 0, eclairage: Eclairage::default(), matcaps: Default::default(), fichiers: 0, anciens: vec![] }
    }
}

impl Drop for Moteur {
    fn drop(&mut self) {
        if let Some(s) = self.sortie.take() {
            let p = s.chemin.clone();
            drop(s);
            let _ = std::fs::remove_file(p);
        }
        for p in &self.anciens {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn norm(a: V3) -> V3 {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-20);
    [a[0] / l, a[1] / l, a[2] / l]
}

fn lin_to_srgb(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u8
}

fn m4(v: &Value) -> Result<M4, String> {
    let a = v.as_array().ok_or("matrice attendue (16 nombres, par lignes)")?;
    let f: Vec<f32> = a.iter().flat_map(|x| x.as_array().cloned().unwrap_or_else(|| vec![x.clone()])).filter_map(|x| x.as_f64()).map(|x| x as f32).collect();
    if f.len() != 16 {
        return Err("matrice attendue (16 nombres, par lignes)".into());
    }
    Ok([[f[0], f[1], f[2], f[3]], [f[4], f[5], f[6], f[7]], [f[8], f[9], f[10], f[11]], [f[12], f[13], f[14], f[15]]])
}

fn couleur(v: &Value) -> [f32; 4] {
    let a: Vec<f32> = v.as_array().map(|a| a.iter().filter_map(|x| x.as_f64()).map(|x| x as f32).collect()).unwrap_or_default();
    [a.first().copied().unwrap_or(0.8), a.get(1).copied().unwrap_or(0.8), a.get(2).copied().unwrap_or(0.8), a.get(3).copied().unwrap_or(1.0)]
}

impl Moteur {
    /// Lit sommets (f32 × 3) puis triangles (u32 × 3) dans un fichier écrit par le client
    pub fn lire_maillage(chemin: &str, sommets: usize, triangles: usize, decalage_tris: Option<usize>) -> Result<(Vec<V3>, Vec<[u32; 3]>), String> {
        use rayon::prelude::*;
        let f = std::fs::File::open(chemin).map_err(|e| format!("{chemin} : {e}"))?;
        let map = unsafe { Mmap::map(&f) }.map_err(|e| e.to_string())?;
        let dt = decalage_tris.unwrap_or(sommets * 12);
        if map.len() < sommets * 12 || map.len() < dt + triangles * 12 {
            return Err(format!("fichier trop court ({} octets) pour {sommets} sommets et {triangles} triangles", map.len()));
        }
        let rd = |b: &[u8], i: usize| [b[i], b[i + 1], b[i + 2], b[i + 3]];
        let pos: Vec<V3> = map[..sommets * 12].par_chunks_exact(12).map(|c| [f32::from_le_bytes(rd(c, 0)), f32::from_le_bytes(rd(c, 4)), f32::from_le_bytes(rd(c, 8))]).map(|p| p.map(|x| if x.is_finite() { x } else { 0.0 })).collect();
        let tris: Vec<[u32; 3]> = map[dt..dt + triangles * 12].par_chunks_exact(12).map(|c| [u32::from_le_bytes(rd(c, 0)), u32::from_le_bytes(rd(c, 4)), u32::from_le_bytes(rd(c, 8))]).collect();
        Ok((pos, tris))
    }

    /// Matériau (u32) et arêtes (u8) par triangle, quand le client les a écrits dans le même fichier
    fn lire_extras(chemin: &str, triangles: usize, dm: Option<usize>, da: Option<usize>) -> Result<(Vec<u16>, Vec<u8>), String> {
        if dm.is_none() && da.is_none() {
            return Ok((vec![], vec![]));
        }
        let f = std::fs::File::open(chemin).map_err(|e| format!("{chemin} : {e}"))?;
        let map = unsafe { Mmap::map(&f) }.map_err(|e| e.to_string())?;
        let mut mat = vec![];
        if let Some(o) = dm {
            let b = map.get(o..o + triangles * 4).ok_or("fichier trop court pour les matériaux")?;
            mat = b.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]).min(u16::MAX as u32) as u16).collect();
        }
        let mut ar = vec![];
        if let Some(o) = da {
            ar = map.get(o..o + triangles).ok_or("fichier trop court pour les arêtes")?.to_vec();
        }
        Ok((mat, ar))
    }

    /// Réglages d'ombrage (commande « eclairage », ou champ du même nom dans « image »)
    fn cmd_eclairage(&mut self, v: &Value) -> Result<(), String> {
        let e = &mut self.eclairage;
        if let Some(m) = v["mode"].as_str() {
            e.mode = match m {
                "studio" => Mode::Studio,
                "matcap" => Mode::Matcap,
                "plat" => Mode::Plat,
                "fil" => Mode::Fil,
                m => return Err(format!("mode d'éclairage inconnu : {m}")),
            };
        }
        if let Some(ls) = v["lampes"].as_array() {
            e.lampes = ls
                .iter()
                .filter_map(|l| Some(Lampe { direction: norm(v3(&l["direction"])?), diffuse: v3(&l["diffuse"]).unwrap_or([1.0; 3]), speculaire: v3(&l["speculaire"]).unwrap_or([0.0; 3]), douceur: l["douceur"].as_f64().unwrap_or(0.0) as f32 }))
                .collect();
        }
        if let Some(a) = v3(&v["ambiante"]) {
            e.ambiante = a;
        }
        if let Some(b) = v["speculaire"].as_bool() {
            e.speculaire = b;
        }
        if let Some(b) = v["matcap_miroir"].as_bool() {
            e.matcap_miroir = b;
        }
        if let Some(b) = v["culling"].as_bool() {
            e.culling = b;
        }
        if let Some(b) = v["fil"].as_bool() {
            e.fil = b;
        }
        if let Some(x) = v["fil_opacite"].as_f64() {
            e.fil_opacite = x.clamp(0.0, 1.0) as f32;
        }
        if let Some(p) = v["matcap"].as_str() {
            let m = match self.matcaps.get(p) {
                Some(m) => m.clone(),
                None => match Matcap::charger(p) {
                    Ok(m) => {
                        let m = std::sync::Arc::new(m);
                        self.matcaps.insert(p.to_string(), m.clone());
                        m
                    }
                    Err(e) => {
                        // Sans son image, le matcap prend la couleur de base : l'image se fait quand même
                        eprintln!("{e}");
                        self.eclairage.matcap = None;
                        return Ok(());
                    }
                },
            };
            self.eclairage.matcap = Some(m);
        }
        Ok(())
    }

    /// Une commande JSON, sa réponse JSON
    pub fn commande(&mut self, ligne: &str) -> (Value, bool) {
        let t0 = Instant::now();
        let v: Value = match serde_json::from_str(ligne) {
            Ok(v) => v,
            Err(e) => return (json!({"ok": false, "erreur": format!("JSON illisible : {e}")}), false),
        };
        let cmd = v["cmd"].as_str().unwrap_or("").to_string();
        let mut quitter = false;
        let r = match cmd.as_str() {
            "bonjour" => Ok(json!({"moteur": "aryadne", "version": env!("CARGO_PKG_VERSION"), "protocole": PROTOCOLE})),
            "objet" => self.cmd_objet(&v),
            "modifier" => self.cmd_modifier(&v),
            "retirer" => {
                let id = v["id"].as_str().unwrap_or("");
                if self.objets.remove(id).is_some() { Ok(json!({"id": id})) } else { Err(format!("objet inconnu : {id}")) }
            }
            "vider" => {
                self.objets.clear();
                Ok(json!({}))
            }
            "vue" => self.cmd_vue(&v).map(|_| json!({})),
            "eclairage" => self.cmd_eclairage(&v).map(|_| json!({})),
            "image" => self.cmd_image(&v),
            "scene" => Ok(json!({
                "objets": self.objets.len(),
                "triangles": self.objets.values().map(|o| o.tris.len()).sum::<usize>(),
                "sommets": self.objets.values().map(|o| o.pos.len()).sum::<usize>(),
            })),
            "quitter" => {
                quitter = true;
                Ok(json!({}))
            }
            "" => Err("champ « cmd » manquant".into()),
            c => Err(format!("commande inconnue : {c}")),
        };
        let mut out = match r {
            Ok(mut o) => {
                o["ok"] = json!(true);
                o
            }
            Err(e) => json!({"ok": false, "erreur": e}),
        };
        out["cmd"] = json!(cmd);
        if let Some(n) = v.get("n") {
            out["n"] = n.clone();
        }
        out["ms"] = json!((t0.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0);
        (out, quitter)
    }

    fn cmd_objet(&mut self, v: &Value) -> Result<Value, String> {
        let id = v["id"].as_str().ok_or("champ « id » manquant")?.to_string();
        let chemin = v["fichier"].as_str().ok_or("champ « fichier » manquant")?;
        let ns = v["sommets"].as_u64().ok_or("champ « sommets » manquant")? as usize;
        let nt = v["triangles"].as_u64().ok_or("champ « triangles » manquant")? as usize;
        let t0 = Instant::now();
        let (pos, tris) = Self::lire_maillage(chemin, ns, nt, v["decalage_triangles"].as_u64().map(|x| x as usize))?;
        let lu = t0.elapsed().as_secs_f64() * 1000.0;
        let ou = |k: &str| v[k].as_u64().map(|x| x as usize);
        let (mat, aretes) = Self::lire_extras(chemin, nt, ou("decalage_materiaux"), ou("decalage_aretes"))?;
        let matrice = if v["matrice"].is_null() { IDENTITE } else { m4(&v["matrice"])? };
        let mats: Vec<Mat> = match v["couleurs"].as_array() {
            Some(a) if !a.is_empty() => a.iter().map(mat_of).collect(),
            _ => vec![Mat { c: couleur(&v["couleur"]), ..Default::default() }],
        };
        let mut o = Objet::complet(pos, tris, mat, aretes, matrice, mats);
        if let Some(c) = v["fil_couleur"].as_array() {
            o.fil = couleur(&Value::Array(c.clone()));
        }
        // Sans numéro donné : le premier libre
        o.num = match v["num"].as_u64() {
            Some(n) if n > 0 && n < u32::MAX as u64 => n as u32,
            _ => (1..).find(|n| !self.objets.iter().any(|(k, x)| x.num == *n && *k != id)).unwrap_or(1),
        };
        let n = o.tris.len();
        let num = o.num;
        self.objets.insert(id.clone(), o);
        Ok(json!({"id": id, "num": num, "triangles": n, "lecture_ms": (lu * 100.0).round() / 100.0}))
    }

    fn cmd_modifier(&mut self, v: &Value) -> Result<Value, String> {
        let id = v["id"].as_str().unwrap_or("");
        let o = self.objets.get_mut(id).ok_or_else(|| format!("objet inconnu : {id}"))?;
        if !v["matrice"].is_null() {
            o.matrice = m4(&v["matrice"])?;
        }
        if !v["couleur"].is_null() {
            o.mats = vec![Mat { c: couleur(&v["couleur"]), ..o.mats[0] }];
        }
        if let Some(a) = v["couleurs"].as_array().filter(|a| !a.is_empty()) {
            o.mats = a.iter().map(mat_of).collect();
        }
        if !v["fil_couleur"].is_null() {
            o.fil = couleur(&v["fil_couleur"]);
        }
        if let Some(b) = v["visible"].as_bool() {
            o.visible = b;
        }
        Ok(json!({"id": id}))
    }

    fn cmd_vue(&mut self, v: &Value) -> Result<(), String> {
        let vue = m4(&v["vue"])?;
        let projection = m4(&v["projection"])?;
        let w = v["largeur"].as_u64().ok_or("champ « largeur » manquant")? as usize;
        let h = v["hauteur"].as_u64().ok_or("champ « hauteur » manquant")? as usize;
        if w == 0 || h == 0 || w > 16384 || h > 16384 {
            return Err(format!("taille impossible : {w} × {h}"));
        }
        // Sans indication : une projection perspective a sa dernière ligne en (0, 0, -1, 0)
        let persp = v["perspective"].as_bool().unwrap_or(projection[3][3].abs() < 1e-6);
        self.vue = Some(Vue { vue, projection, w, h, persp });
        Ok(())
    }

    fn cmd_image(&mut self, v: &Value) -> Result<Value, String> {
        // La vue peut venir avec la demande (un seul message par image)
        if !v["vue"].is_null() {
            self.cmd_vue(v)?;
        }
        if v["eclairage"].is_object() {
            self.cmd_eclairage(&v["eclairage"])?;
        }
        let vue = self.vue.ok_or("pas encore de vue : envoie d'abord « vue »")?;
        let need = vue.w * vue.h;
        if self.sortie.as_ref().is_none_or(|s| s.cap < need) {
            // Un peu de marge, pour ne pas recréer le fichier à chaque agrandissement de la fenêtre
            let cap = need + need / 4;
            self.fichiers += 1;
            let s = Sortie::creer(cap, self.fichiers)?;
            if let Some(old) = self.sortie.replace(s) {
                let p = old.chemin.clone();
                drop(old);
                // Sous Windows, le client peut encore le tenir ouvert : on réessaiera en partant
                if std::fs::remove_file(&p).is_err() {
                    self.anciens.push(p);
                }
            }
        }
        let numero = self.numero + 1;
        let k = (numero % 2) as usize;
        let t0 = Instant::now();
        let s = self.sortie.as_mut().unwrap();
        let cap = s.cap;
        let off = Sortie::decalage(cap, k);
        let head = 16 + k * 16;
        s.map[head + 8..head + 12].copy_from_slice(&(vue.w as u32).to_le_bytes());
        s.map[head + 12..head + 16].copy_from_slice(&(vue.h as u32).to_le_bytes());
        let (rgba, rest) = s.map[off..off + cap * 12].split_at_mut(cap * 4);
        let (depth, ids) = rest.split_at_mut(cap * 4);
        let stats = rendre(&self.objets, &vue, &self.eclairage, &mut rgba[..need * 4], &mut depth[..need * 4], &mut ids[..need * 4]);
        // Le numéro en dernier : une case dont le numéro est celui annoncé est complète
        std::sync::atomic::fence(Ordering::Release);
        s.map[head..head + 8].copy_from_slice(&numero.to_le_bytes());
        self.numero = numero;
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        Ok(json!({
            "image": numero,
            "case": k,
            "largeur": vue.w,
            "hauteur": vue.h,
            "fichier": s.chemin.to_string_lossy(),
            "decalage_rgba": off,
            "decalage_profondeur": off + cap * 4,
            "decalage_ids": off + cap * 8,
            "triangles": stats.0,
            "trace_ms": (stats.1 * 100.0).round() / 100.0,
            "rendu_ms": (ms * 100.0).round() / 100.0,
        }))
    }
}

/// Rendu de la scène : `rgba` (w × h × 4 octets, sRGB), `depth` (w × h f32 petit-boutistes,
/// 0 proche, 1 lointain, 1 aussi pour le fond) et `ids` (numéro de l'objet visible, u32, 0 pour
/// le fond), lignes de bas en haut. Rend (triangles tracés, ms du tracé).
pub fn rendre(objets: &BTreeMap<String, Objet>, vue: &Vue, ecl: &Eclairage, rgba: &mut [u8], depth: &mut [u8], ids: &mut [u8]) -> (usize, f64) {
    use rayon::prelude::*;
    let (w, h) = (vue.w, vue.h);
    let t0 = Instant::now();
    let vp = mul(&vue.projection, &vue.vue);
    let vis: Vec<AtomicU64> = (0..w * h).into_par_iter().map(|_| AtomicU64::new(0)).collect();
    // Par objet visible : premier numéro de triangle global, sommets projetés, z NDC par sommet
    struct Passe<'a> {
        first: u32,
        o: &'a Objet,
        screen: Vec<[f32; 3]>,
        ndc: Vec<f32>,
        nmat: [V3; 3],
    }
    let mut passes: Vec<Passe> = vec![];
    let mut first = 0u64;
    for o in objets.values().filter(|o| o.visible && !o.tris.is_empty()) {
        if first + o.tris.len() as u64 >= u32::MAX as u64 {
            break;
        }
        let mvp = mul(&vp, &o.matrice);
        let (screen, ndc): (Vec<[f32; 3]>, Vec<f32>) = o.pos.par_iter().map(|p| projeter(&mvp, *p, w as f32, h as f32, vue.persp)).unzip();
        // Faces avant : sens direct à l'écran (origine en bas), inversé si la matrice retourne l'objet
        let mv = mul(&vue.vue, &o.matrice);
        let det = mv[0][0] * (mv[1][1] * mv[2][2] - mv[1][2] * mv[2][1]) - mv[0][1] * (mv[1][0] * mv[2][2] - mv[1][2] * mv[2][0]) + mv[0][2] * (mv[1][0] * mv[2][1] - mv[1][1] * mv[2][0]);
        let cull = if ecl.culling && ecl.mode != Mode::Fil { if det >= 0.0 { 1 } else { -1 } } else { 0 };
        visibility_pass(&screen, &o.tris, w, h, vue.persp, first as u32, cull, &vis);
        passes.push(Passe { first: first as u32, o, screen, ndc, nmat: normal_matrix(&mv) });
        first += o.tris.len() as u64;
    }
    let tris = first as usize;
    let trace = t0.elapsed().as_secs_f64() * 1000.0;
    let lampes: Vec<(Lampe, V3)> = ecl.lampes.iter().map(|l| (*l, norm([l.direction[0], l.direction[1], l.direction[2] + 1.0]))).collect();
    let fil = ecl.fil || ecl.mode == Mode::Fil;
    rgba.par_chunks_mut(w * 4).zip(depth.par_chunks_mut(w * 4)).zip(ids.par_chunks_mut(w * 4)).enumerate().for_each(|(y, ((rgba, depth), ids))| {
        for x in 0..w {
            let v = vis[y * w + x].load(Ordering::Relaxed);
            let (px, dp, id) = (&mut rgba[x * 4..x * 4 + 4], &mut depth[x * 4..x * 4 + 4], &mut ids[x * 4..x * 4 + 4]);
            let vide = |px: &mut [u8], dp: &mut [u8], id: &mut [u8]| {
                px.copy_from_slice(&[0, 0, 0, 0]);
                dp.copy_from_slice(&1.0f32.to_le_bytes());
                id.copy_from_slice(&0u32.to_le_bytes());
            };
            if v == 0 {
                vide(px, dp, id);
                continue;
            }
            let g = (v & 0xFFFF_FFFF) as u32;
            let pi = passes.partition_point(|p| p.first <= g) - 1;
            let p = &passes[pi];
            let ti = (g - p.first) as usize;
            let t = p.o.tris[ti];
            let (a, b, c) = (p.screen[t[0] as usize], p.screen[t[1] as usize], p.screen[t[2] as usize]);
            let (pxf, pyf) = (x as f32 + 0.5, y as f32 + 0.5);
            let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
            let inv = 1.0 / area;
            let mut w0 = ((b[0] - pxf) * (c[1] - pyf) - (c[0] - pxf) * (b[1] - pyf)) * inv;
            let mut w1 = ((c[0] - pxf) * (a[1] - pyf) - (a[0] - pxf) * (c[1] - pyf)) * inv;
            let mut w2 = 1.0 - w0 - w1;
            // Distance (pixels) aux arêtes réelles, avant de ramener le point dans le triangle
            let arete = if fil {
                let flags = p.o.aretes.get(ti).copied().unwrap_or(7);
                let len = |u: [f32; 3], v: [f32; 3]| ((u[0] - v[0]).powi(2) + (u[1] - v[1]).powi(2)).sqrt().max(1e-6);
                let a2 = area.abs();
                // bit 0 : v0-v1 (en face de v2), bit 1 : v1-v2 (en face de v0), bit 2 : v2-v0 (en face de v1)
                let mut d = f32::MAX;
                if flags & 1 != 0 {
                    d = d.min(w2 * a2 / len(a, b));
                }
                if flags & 2 != 0 {
                    d = d.min(w0 * a2 / len(b, c));
                }
                if flags & 4 != 0 {
                    d = d.min(w1 * a2 / len(c, a));
                }
                // Couverture d'une ligne d'un pixel, adoucie
                (1.0 - d).clamp(0.0, 1.0) * 0.8
            } else {
                0.0
            };
            if !(w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0) {
                (w0, w1, w2) = (w0.max(0.0), w1.max(0.0), w2.max(0.0));
                let s = w0 + w1 + w2;
                if !(s > 1e-9) {
                    (w0, w1, w2) = (1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0);
                } else {
                    (w0, w1, w2) = (w0 / s, w1 / s, w2 / s);
                }
            }
            // Le z NDC varie linéairement à l'écran (perspective comme orthographique)
            let nz = w0 * p.ndc[t[0] as usize] + w1 * p.ndc[t[1] as usize] + w2 * p.ndc[t[2] as usize];
            let d = nz * 0.5 + 0.5;
            if !(d <= 1.0) || (ecl.mode == Mode::Fil && arete <= 0.0) {
                vide(px, dp, id);
                continue;
            }
            dp.copy_from_slice(&d.max(0.0).to_le_bytes());
            id.copy_from_slice(&p.o.num.to_le_bytes());
            let fc = p.o.fil;
            if ecl.mode == Mode::Fil {
                let a = (arete * fc[3] * ecl.fil_opacite).clamp(0.0, 1.0);
                px.copy_from_slice(&[(fc[0] * 255.0) as u8, (fc[1] * 255.0) as u8, (fc[2] * 255.0) as u8, (a * 255.0 + 0.5) as u8]);
                continue;
            }
            // Normale corrigée de la perspective, ramenée dans le repère de la vue
            let bw = if vue.persp {
                let (_, b2) = persp_bary([w0, w1, w2], [1.0 / -a[2], 1.0 / -b[2], 1.0 / -c[2]]);
                [b2[0], b2[1], 1.0 - b2[0] - b2[1]]
            } else {
                [w0, w1, w2]
            };
            let nl = (0..3).fold([0.0f32; 3], |s, k| {
                let n = p.o.nrm[t[k] as usize];
                [s[0] + n[0] * bw[k], s[1] + n[1] * bw[k], s[2] + n[2] * bw[k]]
            });
            let m = &p.nmat;
            let mut n = norm([m[0][0] * nl[0] + m[0][1] * nl[1] + m[0][2] * nl[2], m[1][0] * nl[0] + m[1][1] * nl[1] + m[1][2] * nl[2], m[2][0] * nl[0] + m[2][1] * nl[1] + m[2][2] * nl[2]]);
            // Deux faces : une face vue de dos est éclairée comme de face
            if n[2] < 0.0 {
                n = [-n[0], -n[1], -n[2]];
            }
            let mat = p.o.mats[(p.o.mat.get(ti).copied().unwrap_or(0) as usize).min(p.o.mats.len() - 1)];
            let base = [mat.c[0], mat.c[1], mat.c[2]];
            let col = match ecl.mode {
                Mode::Plat | Mode::Fil => base,
                Mode::Matcap => match &ecl.matcap {
                    Some(mc) => mc.sample(n, ecl.matcap_miroir, base),
                    None => base,
                },
                Mode::Studio => ombrer(&lampes, ecl, n, &mat),
            };
            let mut out = [lin_to_srgb(col[0]), lin_to_srgb(col[1]), lin_to_srgb(col[2])];
            if arete > 0.0 {
                let a = arete * fc[3] * ecl.fil_opacite;
                for k in 0..3 {
                    out[k] = (out[k] as f32 * (1.0 - a) + fc[k] * 255.0 * a + 0.5) as u8;
                }
            }
            px.copy_from_slice(&[out[0], out[1], out[2], (mat.c[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8]);
        }
    });
    (tris, trace)
}

/// Éclairage de studio : lumières fixées à la vue, enveloppe douce pour les lumières larges,
/// reflet brillant qui s'élargit avec la rugosité. Couleur linéaire en sortie.
fn ombrer(lampes: &[(Lampe, V3)], ecl: &Eclairage, n: V3, mat: &Mat) -> [f32; 3] {
    let base = [mat.c[0], mat.c[1], mat.c[2]];
    let metal = mat.metal.clamp(0.0, 1.0);
    let diff_c = base.map(|c| c * (1.0 - metal));
    let spec_c = [0, 1, 2].map(|k| 0.05 * (1.0 - metal) + base[k] * metal);
    let mut dl = ecl.ambiante;
    let mut sl = [0.0f32; 3];
    let nv = n[2].clamp(0.0, 1.0);
    for (l, hv) in lampes {
        let nl = n[0] * l.direction[0] + n[1] * l.direction[1] + n[2] * l.direction[2];
        let s = l.douceur.clamp(0.0, 0.99);
        // Lambert enveloppé : la lumière déborde derrière la ligne d'ombre quand elle est large
        let d = ((nl + s) / (1.0 + s)).max(0.0);
        let d = d * d * (3.0 - 2.0 * d) * s + d * (1.0 - s);
        for k in 0..3 {
            dl[k] += l.diffuse[k] * d;
        }
        if ecl.speculaire && nl > -s {
            let gloss = (1.0 - mat.rugosite.clamp(0.0, 1.0)) * (1.0 - s);
            let shin = (10.0 * gloss + 1.0).exp2();
            let nh = (n[0] * hv[0] + n[1] * hv[1] + n[2] * hv[2]).max(0.0);
            let sp = nh.powf(shin) * (shin + 2.0) / 32.0 * d.min(1.0);
            for k in 0..3 {
                sl[k] += l.speculaire[k] * sp;
            }
        }
    }
    // Un peu plus de reflet à angle rasant (approximation de Fresnel)
    let fres = 1.0 + 3.0 * (1.0 - nv).powi(5);
    [0, 1, 2].map(|k| diff_c[k] * dl[k] + spec_c[k] * fres * sl[k])
}

/// Boucle du serveur : une commande par ligne, une réponse par ligne. S'arrête sur « quitter »
/// ou quand l'entrée se ferme (le client est parti).
pub fn servir(entree: impl BufRead, mut sortie: impl Write) -> std::io::Result<()> {
    let mut m = Moteur::default();
    for ligne in entree.lines() {
        let ligne = ligne?;
        if ligne.trim().is_empty() {
            continue;
        }
        let (r, quitter) = m.commande(&ligne);
        writeln!(sortie, "{r}")?;
        sortie.flush()?;
        if quitter {
            break;
        }
    }
    Ok(())
}

/// `aryadne --moteur`
pub fn run() {
    eprintln!("aryadne {} : moteur prêt (protocole {PROTOCOLE}), commandes JSON sur l'entrée standard", env!("CARGO_PKG_VERSION"));
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    if let Err(e) = servir(stdin.lock(), stdout.lock()) {
        eprintln!("moteur : {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Projection perspective façon OpenGL / Blender (champ vertical, rapport, proche, lointain)
    fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> M4 {
        let f = 1.0 / (fov_y * 0.5).tan();
        [[f / aspect, 0.0, 0.0, 0.0], [0.0, f, 0.0, 0.0], [0.0, 0.0, (far + near) / (near - far), 2.0 * far * near / (near - far)], [0.0, 0.0, -1.0, 0.0]]
    }
    fn translation(t: V3) -> M4 {
        [[1.0, 0.0, 0.0, t[0]], [0.0, 1.0, 0.0, t[1]], [0.0, 0.0, 1.0, t[2]], [0.0, 0.0, 0.0, 1.0]]
    }
    fn flat(m: &M4) -> Vec<f32> {
        m.iter().flatten().copied().collect()
    }
    fn tmp(nom: &str) -> PathBuf {
        std::env::temp_dir().join(format!("aryadne-test-{}-{nom}", std::process::id()))
    }
    /// Écrit un maillage au format du protocole
    fn ecrire(nom: &str, pos: &[V3], tris: &[[u32; 3]]) -> PathBuf {
        let p = tmp(nom);
        let mut b = vec![];
        for v in pos {
            for x in v {
                b.extend_from_slice(&x.to_le_bytes());
            }
        }
        for t in tris {
            for i in t {
                b.extend_from_slice(&i.to_le_bytes());
            }
        }
        std::fs::write(&p, b).unwrap();
        p
    }
    fn lire_image(m: &Moteur, r: &Value) -> (Vec<u8>, Vec<f32>, u64) {
        let f = std::fs::File::open(r["fichier"].as_str().unwrap()).unwrap();
        let map = unsafe { Mmap::map(&f) }.unwrap();
        let (w, h) = (r["largeur"].as_u64().unwrap() as usize, r["hauteur"].as_u64().unwrap() as usize);
        let k = r["case"].as_u64().unwrap() as usize;
        let num = u64::from_le_bytes(map[16 + k * 16..24 + k * 16].try_into().unwrap());
        let o = r["decalage_rgba"].as_u64().unwrap() as usize;
        let od = r["decalage_profondeur"].as_u64().unwrap() as usize;
        let rgba = map[o..o + w * h * 4].to_vec();
        let depth = map[od..od + w * h * 4].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
        let _ = m;
        (rgba, depth, num)
    }

    #[test]
    fn un_point_tombe_au_bon_pixel() {
        // Caméra à (0, 0, 5) qui regarde vers -Z : la vue est une translation de -5 en z
        let (near, far) = (0.1, 100.0);
        let p = perspective(60f32.to_radians(), 2.0, near, far);
        let v = translation([0.0, 0.0, -5.0]);
        let mvp = mul(&p, &v);
        let (w, h) = (200.0, 100.0);
        // Le centre du monde est au centre de l'image
        let (s, nz) = projeter(&mvp, [0.0, 0.0, 0.0], w, h, true);
        assert!((s[0] - 100.0).abs() < 1e-3 && (s[1] - 50.0).abs() < 1e-3, "{s:?}");
        // Profondeur de fenêtre OpenGL pour une distance de 5
        let ze = -5.0f32;
        let ndc = ((far + near) / (near - far) * ze + 2.0 * far * near / (near - far)) / -ze;
        assert!((nz - ndc).abs() < 1e-5);
        // Un point en haut du champ (tan 30° × 5 au-dessus) tombe en haut de l'image (origine en bas)
        let (s, _) = projeter(&mvp, [0.0, 5.0 * 30f32.to_radians().tan(), 0.0], w, h, true);
        assert!((s[1] - 100.0).abs() < 1e-2, "{s:?}");
        // À droite : x NDC = 0,5 → trois quarts de la largeur
        let x = 0.5 * 5.0 * 30f32.to_radians().tan() * 2.0;
        let (s, _) = projeter(&mvp, [x, 0.0, 0.0], w, h, true);
        assert!((s[0] - 150.0).abs() < 1e-2, "{s:?}");
        // Derrière la caméra : écarté
        let (s, _) = projeter(&mvp, [0.0, 0.0, 6.0], w, h, true);
        assert!(s[0].is_nan());
        // Orthographique : z NDC affine
        let o: M4 = [[0.5, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, -2.0 / (far - near), -(far + near) / (far - near)], [0.0, 0.0, 0.0, 1.0]];
        let (s, nz) = projeter(&mul(&o, &v), [1.0, 0.5, 0.0], w, h, false);
        assert!((s[0] - 150.0).abs() < 1e-3 && (s[1] - 75.0).abs() < 1e-3);
        assert!((nz - ((-2.0 / (far - near)) * -5.0 - (far + near) / (far - near))).abs() < 1e-5);
    }

    #[test]
    fn image_avec_profondeur_de_fenetre() {
        // Un carré face à la caméra, à 5 unités, qui couvre le centre de l'image
        let path = ecrire("carre.bin", &[[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]], &[[0, 1, 2], [0, 2, 3]]);
        let mut m = Moteur::default();
        let (r, _) = m.commande(&json!({"cmd": "objet", "id": "carre", "fichier": path.to_string_lossy(), "sommets": 4, "triangles": 2, "couleur": [0.8, 0.2, 0.1, 1.0]}).to_string());
        assert_eq!(r["ok"], true, "{r}");
        let _ = std::fs::remove_file(&path);
        let (near, far) = (0.1f32, 100.0f32);
        let p = perspective(60f32.to_radians(), 1.0, near, far);
        let req = json!({"cmd": "image", "n": 7, "vue": flat(&translation([0.0, 0.0, -5.0])), "projection": flat(&p), "largeur": 64, "hauteur": 64});
        let (r, _) = m.commande(&req.to_string());
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["n"], 7);
        assert_eq!(r["image"], 1);
        let (rgba, depth, num) = lire_image(&m, &r);
        assert_eq!(num, 1);
        let c = 32 * 64 + 32;
        assert_eq!(rgba[c * 4 + 3], 255);
        assert!(rgba[c * 4] > rgba[c * 4 + 2]);
        let ze = -5.0f32;
        let ndc = ((far + near) / (near - far) * ze + 2.0 * far * near / (near - far)) / -ze;
        assert!((depth[c] - (ndc * 0.5 + 0.5)).abs() < 1e-5, "{} / {}", depth[c], ndc * 0.5 + 0.5);
        // Le coin est vide : transparent, profondeur 1
        assert_eq!(rgba[3], 0);
        assert_eq!(depth[0], 1.0);
        // Déplacer l'objet ne demande pas de renvoyer la géométrie : il sort du champ
        let (r, _) = m.commande(&json!({"cmd": "modifier", "id": "carre", "matrice": flat(&translation([50.0, 0.0, 0.0]))}).to_string());
        assert_eq!(r["ok"], true);
        let (r, _) = m.commande(&json!({"cmd": "image"}).to_string());
        let (rgba, _, _) = lire_image(&m, &r);
        assert_eq!(rgba[c * 4 + 3], 0);
    }

    #[test]
    fn tampon_double() {
        let path = ecrire("tri.bin", &[[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]], &[[0, 1, 2]]);
        let mut m = Moteur::default();
        m.commande(&json!({"cmd": "objet", "id": "t", "fichier": path.to_string_lossy(), "sommets": 3, "triangles": 1}).to_string());
        let _ = std::fs::remove_file(&path);
        let p = perspective(1.0, 1.0, 0.1, 10.0);
        let vue = flat(&translation([0.0, 0.0, -3.0]));
        let r1 = m.commande(&json!({"cmd": "image", "vue": vue, "projection": flat(&p), "largeur": 32, "hauteur": 32}).to_string()).0;
        // Le client tient encore l'image 1 pendant que le moteur écrit la 2 dans l'autre case
        let f = std::fs::File::open(r1["fichier"].as_str().unwrap()).unwrap();
        let map = unsafe { Mmap::map(&f) }.unwrap();
        assert_eq!(&map[0..4], MAGIE);
        let o1 = r1["decalage_rgba"].as_u64().unwrap() as usize;
        let avant = map[o1..o1 + 32 * 32 * 4].to_vec();
        let r2 = m.commande(&json!({"cmd": "image", "vue": flat(&translation([0.5, 0.0, -3.0])), "projection": flat(&p), "largeur": 32, "hauteur": 32}).to_string()).0;
        assert_ne!(r1["case"], r2["case"]);
        assert_eq!(r1["fichier"], r2["fichier"]);
        assert_eq!(r2["image"], 2);
        assert_eq!(&map[o1..o1 + 32 * 32 * 4], &avant[..], "la case lue n'a pas bougé");
        let o2 = r2["decalage_rgba"].as_u64().unwrap() as usize;
        assert_ne!(&map[o2..o2 + 32 * 32 * 4], &avant[..]);
        let (k1, k2) = (r1["case"].as_u64().unwrap() as usize, r2["case"].as_u64().unwrap() as usize);
        assert_eq!(u64::from_le_bytes(map[16 + k1 * 16..24 + k1 * 16].try_into().unwrap()), 1);
        assert_eq!(u64::from_le_bytes(map[16 + k2 * 16..24 + k2 * 16].try_into().unwrap()), 2);
        // Une image plus grande que le fichier : un nouveau fichier, annoncé dans la réponse
        let r3 = m.commande(&json!({"cmd": "image", "vue": vue, "projection": flat(&p), "largeur": 200, "hauteur": 200}).to_string()).0;
        assert_eq!(r3["ok"], true);
        assert_ne!(r1["fichier"], r3["fichier"]);
    }

    #[test]
    fn protocole() {
        let entree = "{\"cmd\":\"bonjour\",\"n\":1}\n\n pas du json\n{\"cmd\":\"retirer\",\"id\":\"rien\"}\n{\"cmd\":\"image\"}\n{\"cmd\":\"scene\"}\n{\"cmd\":\"quitter\"}\n{\"cmd\":\"bonjour\"}\n";
        let mut out = vec![];
        servir(entree.as_bytes(), &mut out).unwrap();
        let lignes: Vec<Value> = String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lignes.len(), 6, "s'arrête après « quitter »");
        assert_eq!(lignes[0]["ok"], true);
        assert_eq!(lignes[0]["n"], 1);
        assert_eq!(lignes[0]["protocole"], PROTOCOLE);
        assert_eq!(lignes[1]["ok"], false);
        assert_eq!(lignes[2]["ok"], false);
        assert_eq!(lignes[3]["ok"], false, "pas d'image sans vue");
        assert_eq!(lignes[4]["objets"], 0);
        assert_eq!(lignes[5]["cmd"], "quitter");
    }

    #[test]
    fn deux_objets_se_cachent() {
        // Deux carrés l'un derrière l'autre : le plus proche gagne, quel que soit l'ordre
        let path = ecrire("deux.bin", &[[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]], &[[0, 1, 2], [0, 2, 3]]);
        let mut m = Moteur::default();
        for (id, z, c) in [("a_loin", -1.0, [0.0, 0.0, 1.0, 1.0]), ("b_pres", 1.0, [1.0, 0.0, 0.0, 1.0])] {
            let r = m.commande(&json!({"cmd": "objet", "id": id, "fichier": path.to_string_lossy(), "sommets": 4, "triangles": 2, "matrice": flat(&translation([0.0, 0.0, z])), "couleur": c}).to_string()).0;
            assert_eq!(r["ok"], true);
        }
        let _ = std::fs::remove_file(&path);
        let p = perspective(1.0, 1.0, 0.1, 10.0);
        let r = m.commande(&json!({"cmd": "image", "vue": flat(&translation([0.0, 0.0, -5.0])), "projection": flat(&p), "largeur": 16, "hauteur": 16}).to_string()).0;
        let (rgba, _, _) = lire_image(&m, &r);
        let c = (8 * 16 + 8) * 4;
        assert!(rgba[c] > 100 && rgba[c + 2] < rgba[c] / 3, "rouge devant : {:?}", &rgba[c..c + 4]);
        // Le tampon d'identifiants dit quel objet est sous chaque pixel
        let f = std::fs::File::open(r["fichier"].as_str().unwrap()).unwrap();
        let map = unsafe { Mmap::map(&f) }.unwrap();
        let oi = r["decalage_ids"].as_u64().unwrap() as usize;
        let id = |k: usize| u32::from_le_bytes(map[oi + k * 4..oi + k * 4 + 4].try_into().unwrap());
        assert_eq!(id(8 * 16 + 8), m.objets["b_pres"].num);
        assert_eq!(id(0), 0);
        assert_ne!(m.objets["a_loin"].num, m.objets["b_pres"].num);
        assert_eq!(r["triangles"], 4);
    }

    #[test]
    fn faces_arriere_et_fil_de_fer() {
        // Triangle dans le sens horaire vu de la caméra : une face arrière
        let path = ecrire("dos.bin", &[[-1.0, -1.0, 0.0], [0.0, 1.0, 0.0], [1.0, -1.0, 0.0]], &[[0, 1, 2]]);
        let mut m = Moteur::default();
        m.commande(&json!({"cmd": "objet", "id": "d", "fichier": path.to_string_lossy(), "sommets": 3, "triangles": 1, "fil_couleur": [1.0, 1.0, 0.0, 1.0]}).to_string());
        let _ = std::fs::remove_file(&path);
        let p = flat(&perspective(1.0, 1.0, 0.1, 10.0));
        let vue = flat(&translation([0.0, 0.0, -3.0]));
        let img = |m: &mut Moteur, ecl: Value| {
            let r = m.commande(&json!({"cmd": "image", "vue": vue, "projection": p, "largeur": 64, "hauteur": 64, "eclairage": ecl}).to_string()).0;
            assert_eq!(r["ok"], true, "{r}");
            lire_image(m, &r).0
        };
        let c = (28 * 64 + 32) * 4;
        assert_eq!(img(&mut m, json!({"culling": false}))[c + 3], 255);
        assert_eq!(img(&mut m, json!({"culling": true}))[c + 3], 0, "face arrière écartée");
        // Fil de fer seul : le centre est vide, le bord du triangle est tracé
        let im = img(&mut m, json!({"culling": false, "mode": "fil"}));
        assert_eq!(im[c + 3], 0);
        assert!((0..64).any(|x| im[(32 * 64 + x) * 4 + 3] > 0 && im[(32 * 64 + x) * 4] == 255));
        let (r, _) = m.commande(&json!({"cmd": "eclairage", "mode": "nimporte"}).to_string());
        assert_eq!(r["ok"], false);
    }
}
