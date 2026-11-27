//! Export des textures d'un matériau de l'Atelier : la pile de calques, ramenée à un graphe, est cuite
//! sur un maillage (un carreau plat, la sphère ou le cube de l'aperçu, le modèle déposé dans l'aperçu,
//! ou un objet d'une scène du coffre) en textures PBR : couleur, rugosité, métal, normale, hauteur,
//! occlusion, émission. Les fichiers suivent un préréglage de nommage (Unreal, Unity, Blender, glTF…),
//! avec les canaux empaquetés que ces logiciels attendent (occlusion, rugosité et métal dans une image).
//! Un maillage dont les UV occupent plusieurs tuiles donne une série d'images par tuile (UDIM).
use crate::nodes::Program;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::Arc;

type V3 = [f32; 3];
type Uv3 = [[f32; 2]; 3];

/// Le maillage sur lequel on cuit.
pub struct BakeMesh {
    pub pos: Vec<V3>,
    pub tris: Vec<[u32; 3]>,
    /// UV par coin de triangle (v vers le haut) : où la texture s'écrit
    pub uv: Vec<Uv3>,
    /// UV que lit le matériau, si le maillage en apporte à l'affichage. Sinon : la projection par
    /// défaut depuis la position, comme dans la vue 3D
    pub mat_uv: Option<Vec<Uv3>>,
}

impl BakeMesh {
    /// Un carreau plat d'un côté de 1, face à la caméra : la texture est le matériau à plat,
    /// telle qu'un graphe en UV la décrit.
    pub fn plane() -> BakeMesh {
        BakeMesh {
            pos: vec![[-0.5, -0.5, 0.0], [0.5, -0.5, 0.0], [0.5, 0.5, 0.0], [-0.5, 0.5, 0.0]],
            tris: vec![[0, 1, 2], [0, 2, 3]],
            uv: vec![[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], [[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]]],
            mat_uv: None,
        }
    }

    /// La sphère de l'aperçu, dépliée en longitude et latitude.
    pub fn sphere() -> BakeMesh {
        let (n, m) = (64usize, 32usize);
        let (mut pos, mut uvs) = (vec![], vec![]);
        for j in 0..=m {
            let t = j as f32 / m as f32 * std::f32::consts::PI;
            for i in 0..=n {
                let p = i as f32 / n as f32 * std::f32::consts::TAU;
                pos.push([t.sin() * p.cos(), t.cos(), t.sin() * p.sin()]);
                uvs.push([i as f32 / n as f32, 1.0 - j as f32 / m as f32]);
            }
        }
        let (mut tris, mut uv) = (vec![], vec![]);
        for j in 0..m {
            for i in 0..n {
                let a = (j * (n + 1) + i) as u32;
                let b = a + n as u32 + 1;
                for t in [[a, a + 1, b + 1], [a, b + 1, b]] {
                    tris.push(t);
                    uv.push(t.map(|v| uvs[v as usize]));
                }
            }
        }
        BakeMesh { pos, tris, uv, mat_uv: None }
    }

    /// Un maillage sans UV : il reçoit les UV automatiques de la peinture (les mêmes pour un même maillage).
    pub fn auto(pos: &[V3], tris: &[[u32; 3]], edges: &[u8]) -> BakeMesh {
        // Seuls les sommets utilisés, renumérotés
        let mut map: std::collections::HashMap<u32, u32> = Default::default();
        let mut p2 = vec![];
        let t2: Vec<[u32; 3]> = tris
            .iter()
            .map(|t| {
                t.map(|v| {
                    *map.entry(v).or_insert_with(|| {
                        p2.push(pos.get(v as usize).copied().unwrap_or([0.0; 3]));
                        (p2.len() - 1) as u32
                    })
                })
            })
            .collect();
        let edges: Vec<u8> = (0..t2.len()).map(|k| edges.get(k).copied().unwrap_or(7)).collect();
        let uv = crate::paint3d::auto_uv(&p2, &t2, &edges);
        BakeMesh { pos: p2, tris: t2, uv, mat_uv: None }
    }

    /// Un modèle déplié dans l'onglet UV : ses polygones et leurs UV. `place` met un point du modèle
    /// (espace ramené à 1) là où l'objet se trouve.
    pub fn from_edit(topo: &crate::uvedit::Topo, data: &crate::uvedit::EditData, place: &dyn Fn(V3) -> V3) -> BakeMesh {
        let (mut tris, mut uv) = (vec![], vec![]);
        for (f, p) in topo.polys.iter().enumerate() {
            let Some(u) = data.uv.get(f).filter(|u| u.len() == p.len()) else { continue };
            for tr in crate::uvedit::poly_tris(&topo.pos, p) {
                tris.push([p[tr[0]], p[tr[1]], p[tr[2]]]);
                uv.push([u[tr[0]], u[tr[1]], u[tr[2]]]);
            }
        }
        BakeMesh { pos: topo.pos.iter().map(|p| place(*p)).collect(), tris, uv, mat_uv: None }
    }
}

/// Les cartes cuites d'une tuile d'UV, en nombres à virgule (une valeur par texel, rangée du haut d'abord).
pub struct Maps {
    pub n: usize,
    /// Tuile d'UV (colonne, rangée) : (0, 0) est la tuile 1001
    pub tile: (i32, i32),
    /// Couleur et émission telles que le matériau les donne (sRGB)
    pub color: Vec<[f32; 3]>,
    pub emit: Vec<[f32; 3]>,
    pub rough: Vec<f32>,
    pub metal: Vec<f32>,
    pub height: Vec<f32>,
    pub ao: Vec<f32>,
    /// Normale en espace tangent (x vers u, y vers v, z hors de la surface), tirée de la hauteur
    pub normal: Vec<[f32; 3]>,
    /// Texels couverts par un triangle (les autres sont remplis par le voisin le plus proche, dans la marge)
    pub covered: Vec<bool>,
}

/// Ce que le matériau donne en un texel
#[derive(Clone, Copy)]
struct Texel {
    color: [f32; 3],
    emit: [f32; 3],
    rough: f32,
    metal: f32,
    height: f32,
    bump_k: f32,
    ao: f32,
}

const EMPTY: Texel = Texel { color: [0.5; 3], emit: [0.0; 3], rough: 0.5, metal: 0.0, height: 0.5, bump_k: 0.0, ao: 1.0 };

/// La pente de la hauteur devient une normale en espace tangent. `h` : hauteur, `k` : force du relief par
/// texel, `scale` : pente dans le monde pour une pente de 1 par texel (côté de la texture × densité des UV).
/// Même mesure que la vue 3D : la normale penche de 0,02 × force × pente.
pub fn normals_from_height(h: &[f32], k: &[f32], n: usize, scale: f32) -> Vec<[f32; 3]> {
    use rayon::prelude::*;
    (0..n * n)
        .into_par_iter()
        .map(|i| {
            let (x, y) = (i % n, i / n);
            let at = |x: usize, y: usize| h[y * n + x];
            let (x0, x1, y0, y1) = (x.saturating_sub(1), (x + 1).min(n - 1), y.saturating_sub(1), (y + 1).min(n - 1));
            // u augmente vers la droite ; v augmente vers le haut de l'image
            let du = (at(x1, y) - at(x0, y)) / (x1 - x0).max(1) as f32;
            let dv = (at(x, y0) - at(x, y1)) / (y1 - y0).max(1) as f32;
            let s = scale * 0.02 * k[i];
            let v = [-du * s, -dv * s, 1.0];
            let l = (v[0] * v[0] + v[1] * v[1] + 1.0).sqrt();
            [v[0] / l, v[1] / l, v[2] / l]
        })
        .collect()
}

/// Cuit le matériau sur le maillage : une série de cartes par tuile d'UV occupée.
/// `ao_rays` : rayons par texel pour l'occlusion du modèle (0 : seulement celle du matériau).
pub fn bake(mesh: &BakeMesh, prog: &Program, size: u32, ao_rays: u32, progress: &(dyn Fn(f32) + Sync)) -> Result<Vec<Maps>, String> {
    use rayon::prelude::*;
    if mesh.tris.is_empty() || mesh.uv.len() != mesh.tris.len() {
        return Err(crate::i18n::t("ce modèle n'a pas d'UV : déplie-le d'abord (onglet UV)"));
    }
    let n = size.clamp(16, 8192) as usize;
    let (pos, tris) = (&mesh.pos, &mesh.tris);
    let sub = |a: V3, b: V3| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: V3, b: V3| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let dot = |a: V3, b: V3| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let unit = |a: V3| {
        let l = dot(a, a).sqrt().max(1e-12);
        [a[0] / l, a[1] / l, a[2] / l]
    };
    // Normales lisses par sommet
    let mut nrm = vec![[0.0f32; 3]; pos.len()];
    for t in tris {
        let f = cross(sub(pos[t[1] as usize], pos[t[0] as usize]), sub(pos[t[2] as usize], pos[t[0] as usize]));
        for &i in t {
            for k in 0..3 {
                nrm[i as usize][k] += f[k];
            }
        }
    }
    for x in nrm.iter_mut() {
        *x = unit(*x);
    }
    let curv = if prog.uses_curvature { crate::model3d::vertex_curvature(pos, &nrm, tris) } else { vec![0.0; pos.len()] };
    let diag = {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for q in pos.iter() {
            for k in 0..3 {
                lo[k] = lo[k].min(q[k]);
                hi[k] = hi[k].max(q[k]);
            }
        }
        dot(sub(hi, lo), sub(hi, lo)).sqrt().max(1e-9)
    };
    let rays = ao_rays.min(256) as usize;
    let bvh = (rays > 0).then(|| crate::bvh::Bvh::new(pos, tris));
    let face_tile: Vec<(i32, i32)> = mesh.uv.iter().map(|u| crate::uvedit::udim_tile(u)).collect();
    let tiles: Vec<(i32, i32)> = face_tile.iter().copied().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    let done = std::sync::atomic::AtomicUsize::new(0);
    let total = (tiles.len() * n).max(1);
    let mut out = vec![];
    for tile in tiles {
        // Quel triangle couvre chaque texel, et la densité des UV de la tuile (UV par unité de surface)
        let mut cover: Vec<Option<(u32, [f32; 3])>> = vec![None; n * n];
        let (mut uv_area, mut world_area) = (0.0f64, 0.0f64);
        for (ti, t) in tris.iter().enumerate() {
            if face_tile[ti] != tile {
                continue;
            }
            let u = &mesh.uv[ti];
            let p: [[f32; 2]; 3] = [0, 1, 2].map(|k| [(u[k][0] - tile.0 as f32) * n as f32, (1.0 - (u[k][1] - tile.1 as f32)) * n as f32]);
            let d = (p[1][1] - p[2][1]) * (p[0][0] - p[2][0]) + (p[2][0] - p[1][0]) * (p[0][1] - p[2][1]);
            if d.abs() < 1e-12 {
                continue;
            }
            uv_area += (d.abs() / 2.0 / (n * n) as f32) as f64;
            let c = cross(sub(pos[t[1] as usize], pos[t[0] as usize]), sub(pos[t[2] as usize], pos[t[0] as usize]));
            world_area += (dot(c, c).sqrt() / 2.0) as f64;
            let (x0, x1) = (p.iter().map(|q| q[0]).fold(f32::MAX, f32::min).floor().max(0.0) as usize, (p.iter().map(|q| q[0]).fold(f32::MIN, f32::max).ceil().max(0.0) as usize).min(n - 1));
            let (y0, y1) = (p.iter().map(|q| q[1]).fold(f32::MAX, f32::min).floor().max(0.0) as usize, (p.iter().map(|q| q[1]).fold(f32::MIN, f32::max).ceil().max(0.0) as usize).min(n - 1));
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((p[1][1] - p[2][1]) * (px - p[2][0]) + (p[2][0] - p[1][0]) * (py - p[2][1])) / d;
                    let w1 = ((p[2][1] - p[0][1]) * (px - p[2][0]) + (p[0][0] - p[2][0]) * (py - p[2][1])) / d;
                    let w2 = 1.0 - w0 - w1;
                    if w0 >= -0.02 && w1 >= -0.02 && w2 >= -0.02 {
                        // Un texel dont le centre est dans le triangle l'emporte sur un voisin qui le frôle
                        let inside = w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0;
                        if inside || cover[y * n + x].is_none() {
                            cover[y * n + x] = Some((ti as u32, [w0, w1, w2]));
                        }
                    }
                }
            }
        }
        // Le matériau, texel par texel, une rangée à la fois (pour suivre l'avancement)
        let texels: Vec<Option<Texel>> = cover
            .par_chunks(n)
            .enumerate()
            .flat_map_iter(|(row, line)| {
                let mut regs = vec![];
                let v: Vec<Option<Texel>> = line
                    .iter()
                    .enumerate()
                    .map(|(col, c)| {
                        let (ti, w) = (*c)?;
                        let t = tris[ti as usize];
                        let mix3 = |a: V3, b: V3, c: V3| [a[0] * w[0] + b[0] * w[1] + c[0] * w[2], a[1] * w[0] + b[1] * w[1] + c[1] * w[2], a[2] * w[0] + b[2] * w[1] + c[2] * w[2]];
                        let p = mix3(pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
                        let nn = unit(mix3(nrm[t[0] as usize], nrm[t[1] as usize], nrm[t[2] as usize]));
                        let cv = curv[t[0] as usize] * w[0] + curv[t[1] as usize] * w[1] + curv[t[2] as usize] * w[2];
                        // Les UV que lit le matériau : celles du maillage s'il en montre, sinon la projection par défaut
                        let uv = match mesh.mat_uv.as_ref().and_then(|m| m.get(ti as usize)) {
                            Some(u) => [u[0][0] * w[0] + u[1][0] * w[1] + u[2][0] * w[2], 1.0 - (u[0][1] * w[0] + u[1][1] * w[1] + u[2][1] * w[2])],
                            None => [p[0] + 0.5, 0.5 - p[1]],
                        };
                        let ctx = crate::nodes::Ctx { masks: [0.0; 4], uv, pos: p, nrm: nn, view: nn, vcol: [1.0; 4], res: [n as f32, n as f32], curv: cv };
                        let sf = prog.material(&ctx, &mut regs);
                        let mut ao = sf.ao;
                        if let Some(bvh) = &bvh {
                            // Occlusion du modèle : part des rayons qui sortent sans rien toucher
                            let ng = {
                                let g = unit(cross(sub(pos[t[1] as usize], pos[t[0] as usize]), sub(pos[t[2] as usize], pos[t[0] as usize])));
                                if dot(g, nn) < 0.0 { [-g[0], -g[1], -g[2]] } else { g }
                            };
                            let tg = unit(cross(nn, if nn[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] }));
                            let bt = cross(nn, tg);
                            let mut seed = ((row * n + col) as u32).wrapping_mul(747796405).wrapping_add(2891336453);
                            let mut rnd = || {
                                seed ^= seed << 13;
                                seed ^= seed >> 17;
                                seed ^= seed << 5;
                                (seed >> 8) as f32 / 16777216.0
                            };
                            let (eps, far) = (diag * 1e-4, diag * 0.25);
                            let mut open = 0.0f32;
                            for _ in 0..rays {
                                let (r1, r2) = (rnd(), rnd());
                                let (rad, phi) = (r1.sqrt(), r2 * std::f32::consts::TAU);
                                let (x, y, z) = (rad * phi.cos(), rad * phi.sin(), (1.0 - r1).max(0.0).sqrt());
                                let mut d = [tg[0] * x + bt[0] * y + nn[0] * z, tg[1] * x + bt[1] * y + nn[1] * z, tg[2] * x + bt[2] * y + nn[2] * z];
                                let below = dot(d, ng);
                                if below < 1e-3 {
                                    let r = 2.0 * (1e-3 - below);
                                    d = unit([d[0] + ng[0] * r, d[1] + ng[1] * r, d[2] + ng[2] * r]);
                                }
                                if !bvh.occluded([p[0] + ng[0] * eps, p[1] + ng[1] * eps, p[2] + ng[2] * eps], d, eps, far) {
                                    open += 1.0;
                                }
                            }
                            ao *= open / rays as f32;
                        }
                        Some(Texel { color: [sf.color[0], sf.color[1], sf.color[2]], emit: [sf.emit[0], sf.emit[1], sf.emit[2]], rough: sf.rough, metal: sf.metal, height: sf.bump, bump_k: if prog.uses_bump { sf.bump_k } else { 0.0 }, ao })
                    })
                    .collect();
                let k = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if k % 16 == 0 {
                    progress(k as f32 / total as f32 * 0.95);
                }
                v
            })
            .collect();
        // Marge : chaque texel vide proche d'un îlot prend la valeur du texel rempli le plus proche
        // (en largeur d'abord), pour qu'aucune couture n'apparaisse quand la texture est réduite
        let margin = (n / 64).clamp(4, 64) as u32;
        let mut from: Vec<u32> = (0..(n * n) as u32).collect();
        let mut dist: Vec<u32> = texels.iter().map(|t| if t.is_some() { 0 } else { u32::MAX }).collect();
        let mut queue: std::collections::VecDeque<u32> = (0..(n * n) as u32).filter(|&i| dist[i as usize] == 0).collect();
        while let Some(i) = queue.pop_front() {
            let d = dist[i as usize];
            if d >= margin {
                continue;
            }
            let (x, y) = ((i as usize % n) as i64, (i as usize / n) as i64);
            for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                let (xx, yy) = (x + dx, y + dy);
                if xx < 0 || yy < 0 || xx >= n as i64 || yy >= n as i64 {
                    continue;
                }
                let j = yy as usize * n + xx as usize;
                if dist[j] == u32::MAX {
                    dist[j] = d + 1;
                    from[j] = from[i as usize];
                    queue.push_back(j as u32);
                }
            }
        }
        let get = |i: usize| if dist[i] == u32::MAX { EMPTY } else { texels[from[i] as usize].unwrap_or(EMPTY) };
        let all: Vec<Texel> = (0..n * n).into_par_iter().map(get).collect();
        let height: Vec<f32> = all.iter().map(|t| t.height).collect();
        let bump_k: Vec<f32> = all.iter().map(|t| t.bump_k).collect();
        // Pente dans le monde pour une pente de 1 par texel
        let density = if world_area > 0.0 { (uv_area / world_area).sqrt() as f32 } else { 1.0 };
        let normal = if prog.uses_bump { normals_from_height(&height, &bump_k, n, n as f32 * density) } else { vec![[0.0, 0.0, 1.0]; n * n] };
        out.push(Maps {
            n,
            tile,
            color: all.iter().map(|t| t.color).collect(),
            emit: all.iter().map(|t| t.emit).collect(),
            rough: all.iter().map(|t| t.rough).collect(),
            metal: all.iter().map(|t| t.metal).collect(),
            ao: all.iter().map(|t| t.ao).collect(),
            height,
            normal,
            covered: texels.iter().map(|t| t.is_some()).collect(),
        });
    }
    progress(0.95);
    Ok(out)
}

// ---------- écriture ----------

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Format {
    Png8,
    Png16,
    Exr,
}

impl Format {
    pub const ALL: [(Format, &'static str, &'static str); 3] = [
        (Format::Png8, "PNG 8 bits", "Le plus léger, lu partout. 256 niveaux par canal : assez pour la couleur, la rugosité et le métal."),
        (Format::Png16, "PNG 16 bits", "65 536 niveaux par canal : pour la hauteur et la normale, sans marches visibles."),
        (Format::Exr, "EXR 32 bits", "Nombres à virgule, couleur en linéaire : pour le compositing et les rendus de film. Fichiers lourds."),
    ];
    pub fn ext(self) -> &'static str {
        if self == Format::Exr { "exr" } else { "png" }
    }
}

/// Ce qu'un canal d'image contient
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Src {
    Color,
    Emit,
    /// Normale, vert vers le haut (OpenGL : Blender, Unity, glTF)
    NormalGl,
    /// Normale, vert vers le bas (DirectX : Unreal)
    NormalDx,
    Rough,
    /// 1 − rugosité (Unity)
    Smooth,
    Metal,
    Height,
    Ao,
}

/// Une image à écrire : une carte entière, ou des canaux empaquetés (3 ou 4)
#[derive(Clone, Copy, Debug)]
pub enum Pack {
    Map(Src),
    Chans(&'static [Src]),
}

/// Quand l'image s'écrit
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum When {
    Always,
    /// Le matériau émet de la lumière
    Emit,
    /// Le matériau branche un relief
    Height,
    /// Le matériau branche une occlusion, ou celle du modèle est calculée
    Ao,
}

#[derive(Clone, Copy, Debug)]
pub struct Out {
    pub suffix: &'static str,
    pub pack: Pack,
    pub when: When,
}

pub struct Preset {
    pub name: &'static str,
    pub tip: &'static str,
    pub prefix: &'static str,
    pub outs: &'static [Out],
}

const fn o(suffix: &'static str, pack: Pack, when: When) -> Out {
    Out { suffix, pack, when }
}

/// Préréglages de nommage : les noms et les canaux que chaque logiciel reconnaît.
pub const PRESETS: [Preset; 5] = [
    Preset {
        name: "Aryadne",
        tip: "Une image par carte, noms en français : couleur, rugosité, métal, normale, hauteur, occlusion, émission.",
        prefix: "",
        outs: &[o("_couleur", Pack::Map(Src::Color), When::Always), o("_rugosite", Pack::Map(Src::Rough), When::Always), o("_metal", Pack::Map(Src::Metal), When::Always), o("_normale", Pack::Map(Src::NormalGl), When::Always), o("_hauteur", Pack::Map(Src::Height), When::Height), o("_occlusion", Pack::Map(Src::Ao), When::Ao), o("_emission", Pack::Map(Src::Emit), When::Emit)],
    },
    Preset {
        name: "Unreal",
        tip: "T_Nom_BaseColor, T_Nom_Normal (vert vers le bas, DirectX) et T_Nom_ORM : occlusion en rouge, rugosité en vert, métal en bleu.",
        prefix: "T_",
        outs: &[o("_BaseColor", Pack::Map(Src::Color), When::Always), o("_Normal", Pack::Map(Src::NormalDx), When::Always), o("_ORM", Pack::Chans(&[Src::Ao, Src::Rough, Src::Metal]), When::Always), o("_Emissive", Pack::Map(Src::Emit), When::Emit), o("_Height", Pack::Map(Src::Height), When::Height)],
    },
    Preset {
        name: "Unity",
        tip: "Nom_Albedo, Nom_Normal, Nom_MetallicSmoothness (métal en RVB, lissé dans l'alpha : 1 − rugosité), Nom_AO.",
        prefix: "",
        outs: &[o("_Albedo", Pack::Map(Src::Color), When::Always), o("_Normal", Pack::Map(Src::NormalGl), When::Always), o("_MetallicSmoothness", Pack::Chans(&[Src::Metal, Src::Metal, Src::Metal, Src::Smooth]), When::Always), o("_AO", Pack::Map(Src::Ao), When::Ao), o("_Height", Pack::Map(Src::Height), When::Height), o("_Emission", Pack::Map(Src::Emit), When::Emit)],
    },
    Preset {
        name: "Blender",
        tip: "Les noms des prises du Principled BSDF : Base_Color, Roughness, Metallic, Normal, Displacement. Node Wrangler (Ctrl+Maj+T) les branche tout seul.",
        prefix: "",
        outs: &[o("_Base_Color", Pack::Map(Src::Color), When::Always), o("_Roughness", Pack::Map(Src::Rough), When::Always), o("_Metallic", Pack::Map(Src::Metal), When::Always), o("_Normal", Pack::Map(Src::NormalGl), When::Always), o("_Displacement", Pack::Map(Src::Height), When::Height), o("_Ambient_Occlusion", Pack::Map(Src::Ao), When::Ao), o("_Emission", Pack::Map(Src::Emit), When::Emit)],
    },
    Preset {
        name: "glTF",
        tip: "Nom_baseColor, Nom_normal et Nom_occlusionRoughnessMetallic : occlusion en rouge, rugosité en vert, métal en bleu, comme le format glTF le demande.",
        prefix: "",
        outs: &[o("_baseColor", Pack::Map(Src::Color), When::Always), o("_normal", Pack::Map(Src::NormalGl), When::Always), o("_occlusionRoughnessMetallic", Pack::Chans(&[Src::Ao, Src::Rough, Src::Metal]), When::Always), o("_emissive", Pack::Map(Src::Emit), When::Emit)],
    },
];

/// Les cartes facultatives que ce matériau donne : (émission, hauteur, occlusion)
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Has {
    pub emit: bool,
    pub height: bool,
    pub ao: bool,
}

impl Has {
    fn wants(self, w: When) -> bool {
        match w {
            When::Always => true,
            When::Emit => self.emit,
            When::Height => self.height,
            When::Ao => self.ao,
        }
    }
}

/// Le nom d'une image : préfixe, nom du matériau, suffixe, tuile UDIM s'il y en a plusieurs, extension.
pub fn file_name(preset: &Preset, out: &Out, stem: &str, udim: Option<u32>, format: Format) -> String {
    match udim {
        Some(u) => format!("{}{stem}{}.{u}.{}", preset.prefix, out.suffix, format.ext()),
        None => format!("{}{stem}{}.{}", preset.prefix, out.suffix, format.ext()),
    }
}

/// Les noms des images qu'un export écrira (pour les montrer avant de lancer).
pub fn planned(preset: &Preset, stem: &str, format: Format, has: Has) -> Vec<String> {
    preset.outs.iter().filter(|o| has.wants(o.when)).map(|o| file_name(preset, o, stem, None, format)).collect()
}

/// La valeur d'un canal en un texel. `linear` : couleur et émission en linéaire (EXR), sinon en sRGB.
fn scalar(m: &Maps, s: Src, i: usize) -> f32 {
    match s {
        Src::Rough => m.rough[i],
        Src::Smooth => 1.0 - m.rough[i],
        Src::Metal => m.metal[i],
        Src::Height => m.height[i],
        Src::Ao => m.ao[i],
        // Une carte en couleurs rangée dans un canal : sa luminance
        Src::Color => (m.color[i][0] + m.color[i][1] + m.color[i][2]) / 3.0,
        Src::Emit => (m.emit[i][0] + m.emit[i][1] + m.emit[i][2]) / 3.0,
        Src::NormalGl | Src::NormalDx => m.normal[i][2] * 0.5 + 0.5,
    }
}

/// Les canaux d'une image en un texel : (valeurs, nombre de canaux : 1, 3 ou 4)
fn texel(m: &Maps, pack: Pack, i: usize, linear: bool) -> ([f32; 4], usize) {
    let lin = |c: [f32; 3]| if linear { c.map(|x| x.max(0.0).powf(2.2)) } else { c };
    match pack {
        Pack::Map(Src::Color) => {
            let c = lin(m.color[i]);
            ([c[0], c[1], c[2], 1.0], 3)
        }
        Pack::Map(Src::Emit) => {
            let c = lin(m.emit[i]);
            ([c[0], c[1], c[2], 1.0], 3)
        }
        Pack::Map(Src::NormalGl) => ([m.normal[i][0] * 0.5 + 0.5, m.normal[i][1] * 0.5 + 0.5, m.normal[i][2] * 0.5 + 0.5, 1.0], 3),
        Pack::Map(Src::NormalDx) => ([m.normal[i][0] * 0.5 + 0.5, 0.5 - m.normal[i][1] * 0.5, m.normal[i][2] * 0.5 + 0.5, 1.0], 3),
        Pack::Map(s) => ([scalar(m, s, i), 0.0, 0.0, 1.0], 1),
        Pack::Chans(c) => {
            let mut v = [0.0, 0.0, 0.0, 1.0];
            for (k, s) in c.iter().take(4).enumerate() {
                v[k] = scalar(m, *s, i);
            }
            (v, c.len().clamp(3, 4))
        }
    }
}

/// Écrit une image des cartes dans `path`, au format demandé.
pub fn write_image(m: &Maps, pack: Pack, format: Format, path: &Path) -> Result<(), String> {
    use rayon::prelude::*;
    let (n, count) = (m.n as u32, m.n * m.n);
    let chans = texel(m, pack, 0, false).1;
    let err = |e: image::ImageError| e.to_string();
    let bad = || crate::i18n::t("image invalide");
    match format {
        Format::Png8 => {
            let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            let bytes: Vec<u8> = (0..count).into_par_iter().flat_map_iter(|i| { let (v, c) = texel(m, pack, i, false); (0..c).map(move |k| q(v[k])) }).collect();
            match chans {
                1 => image::GrayImage::from_raw(n, n, bytes).ok_or_else(bad)?.save(path).map_err(err),
                3 => image::RgbImage::from_raw(n, n, bytes).ok_or_else(bad)?.save(path).map_err(err),
                _ => image::RgbaImage::from_raw(n, n, bytes).ok_or_else(bad)?.save(path).map_err(err),
            }
        }
        Format::Png16 => {
            let q = |x: f32| (x.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
            let data: Vec<u16> = (0..count).into_par_iter().flat_map_iter(|i| { let (v, c) = texel(m, pack, i, false); (0..c).map(move |k| q(v[k])) }).collect();
            match chans {
                1 => image::ImageBuffer::<image::Luma<u16>, _>::from_raw(n, n, data).ok_or_else(bad)?.save(path).map_err(err),
                3 => image::ImageBuffer::<image::Rgb<u16>, _>::from_raw(n, n, data).ok_or_else(bad)?.save(path).map_err(err),
                _ => image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(n, n, data).ok_or_else(bad)?.save(path).map_err(err),
            }
        }
        Format::Exr => {
            // L'EXR s'écrit en RVB ou RVBA : une carte en gris y répète sa valeur
            let out = if chans == 4 { 4 } else { 3 };
            let data: Vec<f32> = (0..count)
                .into_par_iter()
                .flat_map_iter(|i| {
                    let (v, c) = texel(m, pack, i, true);
                    let v = if c == 1 { [v[0], v[0], v[0], 1.0] } else { v };
                    (0..out).map(move |k| v[k])
                })
                .collect();
            if out == 4 {
                image::Rgba32FImage::from_raw(n, n, data).ok_or_else(bad)?.save(path).map_err(err)
            } else {
                image::Rgb32FImage::from_raw(n, n, data).ok_or_else(bad)?.save(path).map_err(err)
            }
        }
    }
}

/// Réglages d'un export
#[derive(Clone, Debug)]
pub struct Settings {
    pub size: u32,
    pub format: Format,
    /// Indice dans `PRESETS`
    pub preset: usize,
    pub dir: PathBuf,
    /// Nom du matériau, au cœur du nom de chaque image
    pub stem: String,
    pub ao_rays: u32,
}

/// Cuit et écrit toutes les images. Rend les fichiers écrits.
pub fn export(mesh: &BakeMesh, prog: &Program, s: &Settings, progress: &(dyn Fn(f32) + Sync)) -> Result<Vec<PathBuf>, String> {
    let preset = PRESETS.get(s.preset).unwrap_or(&PRESETS[0]);
    let maps = bake(mesh, prog, s.size, s.ao_rays, progress)?;
    let has = Has { emit: maps.iter().any(|m| m.emit.iter().any(|e| e.iter().any(|x| *x > 0.003))), height: prog.uses_bump, ao: s.ao_rays > 0 || maps.iter().any(|m| m.ao.iter().any(|a| *a < 0.999)) };
    std::fs::create_dir_all(&s.dir).map_err(|e| e.to_string())?;
    let multi = maps.len() > 1 || maps.iter().any(|m| m.tile != (0, 0));
    let mut files = vec![];
    for m in &maps {
        let udim = multi.then(|| crate::uvedit::udim_number(m.tile));
        for o in preset.outs.iter().filter(|o| has.wants(o.when)) {
            let path = s.dir.join(file_name(preset, o, &s.stem, udim, s.format));
            write_image(m, o.pack, s.format, &path)?;
            files.push(path);
        }
    }
    progress(1.0);
    Ok(files)
}

// ---------- sur quoi cuire ----------

/// Sur quoi le matériau est cuit
#[derive(Clone, PartialEq, Debug)]
pub enum Target {
    /// Carreau plat : le matériau à plat, en UV
    Plane,
    Sphere,
    Cube,
    /// Le modèle déposé dans l'aperçu de l'Atelier
    Dropped(String),
    /// Un objet d'une scène du coffre. `uses` : il porte ce matériau
    Object { scene: String, index: usize, name: String, uses: bool },
}

impl Target {
    pub fn label(&self) -> String {
        match self {
            Target::Plane => crate::i18n::t("carreau plat (le matériau en UV)"),
            Target::Sphere => crate::i18n::t("sphère de l'aperçu"),
            Target::Cube => crate::i18n::t("cube de l'aperçu"),
            Target::Dropped(n) => format!("{} : {n}", crate::i18n::t("modèle de l'aperçu")),
            Target::Object { scene, name, .. } => format!("{name}  ·  {}", crate::formats::strip(scene.rsplit('/').next().unwrap_or(scene))),
        }
    }
}

/// Les objets à maillage des scènes du coffre, ceux qui portent le matériau `material` (chemin dans le coffre) d'abord.
pub fn scene_objects(root: &str, material: &str) -> Vec<Target> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>, depth: usize) {
        if depth > 6 || out.len() >= 80 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if name.starts_with('.') {
                continue;
            }
            if p.is_dir() {
                walk(&p, root, out, depth + 1);
            } else if let Ok(r) = p.strip_prefix(root) {
                let rel = r.to_string_lossy().replace('\\', "/");
                if crate::formats::has(&rel, "scene") {
                    out.push(rel);
                }
            }
        }
    }
    if root.is_empty() {
        return vec![];
    }
    let mut scenes = vec![];
    walk(Path::new(root), Path::new(root), &mut scenes, 0);
    let mut out = vec![];
    for rel in scenes {
        let Ok(sc) = crate::scene::Scene::read(&Path::new(root).join(&rel)) else { continue };
        for (k, ob) in sc.objects.iter().enumerate() {
            if crate::scene::kind(ob) == crate::scene::Kind::Model {
                out.push(Target::Object { scene: rel.clone(), index: k, name: ob.name.clone(), uses: !material.is_empty() && ob.material == material });
            }
        }
    }
    out.sort_by_key(|t| !matches!(t, Target::Object { uses: true, .. }));
    out
}

/// Le maillage d'un objet de scène, à sa place dans la scène (le matériau y lit les mêmes positions
/// que la vue 3D). Ses UV : celles dépliées dans l'onglet UV si l'objet en a ; sinon celles qu'il apporte
/// (géométrie de nodes) ; sinon les UV automatiques de la peinture.
pub fn object_mesh(root: &str, scene_path: &Path, k: usize) -> Result<BakeMesh, String> {
    use crate::scene::{kind, Kind, Scene};
    let mut scene = Scene::read(scene_path)?;
    let ob = scene.objects.get(k).cloned().ok_or_else(|| crate::i18n::t("cet objet n'est plus dans la scène"))?;
    if kind(&ob) != Kind::Model {
        return Err(crate::i18n::t("seuls les objets à maillage se cuisent"));
    }
    let plain = !ob.file.starts_with('@') && !crate::geoscene::is_geo(&ob.file) && !ob.mods.iter().any(|m| m.on);
    if plain {
        if let Ok(m) = crate::scene::source_model(scene_path, &ob) {
            let topo = crate::uvedit::Topo::new(&m.normalized(false), &m.tris, &m.edges);
            let side = crate::model3d::meshdata_file(root, &crate::scene::resolve(scene_path, &ob.file));
            let data = std::fs::read_to_string(side).ok().and_then(|t| serde_json::from_str::<crate::uvedit::EditData>(&t).ok()).filter(|d| d.fits(&topo) && d.has_uv() && d.uv.len() == topo.polys.len());
            if let Some(d) = data {
                let s = m.size();
                let l = s[0].max(s[1]).max(s[2]).max(1e-9);
                let c = [(m.min[0] + m.max[0]) / 2.0, (m.min[1] + m.max[1]) / 2.0, (m.min[2] + m.max[2]) / 2.0];
                let z_up = m.z_up;
                let place = |p: V3| {
                    let f = [p[0] * l + c[0], p[1] * l + c[1], p[2] * l + c[2]];
                    scene.world(k, if z_up { [f[0], f[2], -f[1]] } else { f })
                };
                return Ok(BakeMesh::from_edit(&topo, &d, &place));
            }
        }
    }
    // Le maillage tel que la scène le montre (modificateurs appliqués). Un objet caché se cuit quand même
    let mut cur = k;
    for _ in 0..32 {
        scene.objects[cur].visible = true;
        let Some(up) = scene.parent_of(cur) else { break };
        cur = up;
    }
    let (model, errors, ranges) = crate::scene::build_ranges(&scene, scene_path);
    let model = model?;
    let Some(&(_, a, b)) = ranges.iter().find(|r| r.0 == k) else {
        return Err(errors.get(k).cloned().flatten().unwrap_or_else(|| crate::i18n::t("cet objet n'a pas de maillage")));
    };
    let mut mesh = BakeMesh::auto(&model.positions, &model.tris[a..b], model.edges.get(a..b).unwrap_or(&[]));
    // Une géométrie de nodes apporte ses UV : la texture s'y écrit, et le matériau les lit
    if let Some(u) = model.uv.as_ref().and_then(|u| u.get(a..b)).filter(|u| u.iter().all(|t| t.iter().all(|c| c[0].is_finite() && c[1].is_finite()))) {
        mesh.uv = u.to_vec();
        mesh.mat_uv = Some(u.to_vec());
    }
    Ok(mesh)
}

/// Le maillage de la cible. `dropped` : le modèle déposé dans l'aperçu, s'il y en a un.
pub fn target_mesh(target: &Target, root: &str, dropped: Option<&crate::model3d::Prepared>) -> Result<BakeMesh, String> {
    match target {
        Target::Plane => Ok(BakeMesh::plane()),
        Target::Sphere => Ok(BakeMesh::sphere()),
        Target::Cube => {
            let b = crate::model3d::preview_box();
            let (pos, tris, edges) = b.mesh_parts();
            Ok(BakeMesh::auto(pos, tris, edges))
        }
        Target::Dropped(_) => {
            let m = dropped.ok_or_else(|| crate::i18n::t("le modèle de l'aperçu n'est plus là"))?;
            let (pos, tris, edges) = m.mesh_parts();
            Ok(BakeMesh::auto(pos, tris, edges))
        }
        Target::Object { scene, index, .. } => object_mesh(root, &Path::new(root).join(scene), *index),
    }
}

// ---------- fenêtre d'export ----------

struct Job {
    rx: std::sync::mpsc::Receiver<Result<Vec<PathBuf>, String>>,
    progress: Arc<std::sync::atomic::AtomicU32>,
    started: std::time::Instant,
}

/// La fenêtre « Exporter les textures » de l'Atelier
pub struct Dialog {
    targets: Vec<Target>,
    target: usize,
    size: u32,
    format: Format,
    preset: usize,
    /// Dossier d'arrivée, tel qu'on l'écrit : dans le coffre (chemin court) ou ailleurs (chemin complet)
    dir: String,
    stem: String,
    ao: bool,
    job: Option<Job>,
    /// Dernier export : (nombre d'images, dossier, secondes), ou l'erreur
    result: Option<Result<(usize, PathBuf, f32), String>>,
}

/// Le dossier d'arrivée par défaut : « textures », à côté du matériau.
pub fn default_dir(root: &str, material: &Path) -> String {
    let dir = material.parent().unwrap_or(Path::new("")).join("textures");
    short_dir(root, &dir)
}

fn short_dir(root: &str, dir: &Path) -> String {
    match dir.strip_prefix(root) {
        Ok(r) if !root.is_empty() => r.to_string_lossy().replace('\\', "/"),
        _ => dir.to_string_lossy().to_string(),
    }
}

fn full_dir(root: &str, dir: &str) -> PathBuf {
    let p = Path::new(dir.trim());
    if p.is_absolute() || root.is_empty() { p.to_path_buf() } else { Path::new(root).join(p) }
}

impl Dialog {
    /// `material` : le fichier du matériau ; `dropped` : le nom du modèle déposé dans l'aperçu.
    pub fn new(root: &str, material: &Path, dropped: Option<&str>) -> Dialog {
        let rel = material.strip_prefix(root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
        let mut targets = vec![];
        let objects = scene_objects(root, &rel);
        // Les objets qui portent ce matériau d'abord, puis l'aperçu, puis les autres objets
        let mine = objects.iter().filter(|t| matches!(t, Target::Object { uses: true, .. })).count();
        targets.extend(objects.iter().take(mine).cloned());
        if let Some(n) = dropped {
            targets.push(Target::Dropped(n.to_string()));
        }
        let first_preview = targets.len();
        targets.extend([Target::Plane, Target::Sphere, Target::Cube]);
        targets.extend(objects.into_iter().skip(mine));
        let stem = material.file_name().map(|s| crate::formats::strip(&s.to_string_lossy()).to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "matériau".into());
        // Un objet porte le matériau : c'est lui qu'on veut cuire ; sinon le modèle de l'aperçu, sinon le carreau
        let target = if mine > 0 || dropped.is_some() { 0 } else { first_preview };
        Dialog { targets, target, size: 2048, format: Format::Png8, preset: 0, dir: default_dir(root, material), stem, ao: false, job: None, result: None }
    }

    fn settings(&self, root: &str) -> Settings {
        Settings { size: self.size, format: self.format, preset: self.preset, dir: full_dir(root, &self.dir), stem: self.stem.clone(), ao_rays: if self.ao { 16 } else { 0 } }
    }

    /// Dessine la fenêtre. Rend faux quand on la ferme. `prog` : le matériau compilé (rien : il a une erreur).
    pub fn ui(&mut self, ctx: &egui::Context, root: &str, prog: Option<&Program>, dropped: Option<&Arc<crate::model3d::Prepared>>) -> bool {
        use egui::{vec2, RichText};
        let p = crate::theme::pal();
        // Le résultat du calcul en cours
        if let Some(j) = &self.job {
            match j.rx.try_recv() {
                Ok(r) => {
                    let secs = j.started.elapsed().as_secs_f32();
                    self.result = Some(r.map(|f| (f.len(), f.first().and_then(|x| x.parent()).map(|d| d.to_path_buf()).unwrap_or_default(), secs)));
                    self.job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.result = Some(Err(crate::i18n::t("l'export s'est arrêté")));
                    self.job = None;
                }
                Err(_) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }
        let mut open = true;
        let mut go = false;
        let title = crate::i18n::t("exporter les textures");
        crate::dock::window(ctx, "atelier-export", &title, egui::Window::new(&title).collapsible(false).resizable(false).open(&mut open).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]), |ui| {
            ui.set_width(430.0);
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            let card = |ui: &mut egui::Ui, title: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
                egui::Frame::new().fill(p.field).stroke(egui::Stroke::new(1.0, p.line)).corner_radius(8).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing = vec2(6.0, 8.0);
                    ui.label(RichText::new(crate::i18n::t(title)).strong().size(12.0));
                    body(ui);
                });
            };
            // Puce arrondie : un choix parmi quelques-uns
            let chip = |ui: &mut egui::Ui, on: bool, text: &str, tip: &str| {
                let b = egui::Button::new(RichText::new(text).size(11.5).color(if on { p.on_accent } else { p.text })).fill(if on { p.accent } else { p.panel }).corner_radius(11).min_size(vec2(0.0, 24.0));
                ui.add(b).on_hover_text(crate::i18n::t(tip)).clicked()
            };
            let busy = self.job.is_some();
            ui.add_enabled_ui(!busy, |ui| {
                card(ui, "Modèle à cuire", &mut |ui| {
                    crate::modifiers::row(ui, "cuire sur", |ui| {
                        let cur = self.targets.get(self.target).map(|t| t.label()).unwrap_or_default();
                        egui::ComboBox::from_id_salt("atelier-export-target").selected_text(RichText::new(cur).size(11.5)).width(ui.available_width() - 4.0).show_ui(ui, |ui| {
                            for (k, t) in self.targets.iter().enumerate() {
                                let icon = match t {
                                    Target::Object { uses: true, .. } => crate::lucide::LINK,
                                    Target::Object { .. } => crate::lucide::FOLDER,
                                    _ => crate::lucide::IMAGE,
                                };
                                let tip = match t {
                                    Target::Object { uses: true, scene, .. } => format!("{} ({scene})", crate::i18n::t("Cet objet porte ce matériau")),
                                    Target::Object { scene, .. } => scene.clone(),
                                    Target::Plane => crate::i18n::t("Une texture carrée du matériau à plat, à répéter sur n'importe quel modèle"),
                                    _ => crate::i18n::t("Le modèle montré dans l'aperçu de l'Atelier"),
                                };
                                if ui.selectable_label(self.target == k, format!("{icon}  {}", t.label())).on_hover_text(tip).clicked() {
                                    self.target = k;
                                }
                            }
                        });
                    });
                    ui.checkbox(&mut self.ao, RichText::new(crate::i18n::t("occlusion du modèle")).size(11.5)).on_hover_text(crate::i18n::t("Calcule l'ombre que le modèle se fait à lui-même (creux, plis) par lancer de rayons, et la multiplie à l'occlusion du matériau. Plus long."));
                    let note = match self.targets.get(self.target) {
                        Some(Target::Object { .. }) => "L'objet est cuit à sa place dans la scène, sur ses UV (onglet UV), ou sur des UV automatiques s'il n'est pas déplié. Les masques peints à la main ne sont pas cuits.",
                        Some(Target::Plane) => "Le matériau est cuit à plat, en UV : une texture carrée à répéter.",
                        _ => "Le modèle de l'aperçu est cuit sur des UV automatiques.",
                    };
                    ui.label(RichText::new(crate::i18n::t(note)).size(10.5).color(p.muted));
                });
                card(ui, "Images", &mut |ui| {
                    crate::modifiers::row(ui, "résolution", |ui| {
                        for (s, l) in [(1024u32, "1K"), (2048, "2K"), (4096, "4K")] {
                            if chip(ui, self.size == s, l, match s { 1024 => "1024 × 1024 pixels", 2048 => "2048 × 2048 pixels", _ => "4096 × 4096 pixels" }) {
                                self.size = s;
                            }
                        }
                    });
                    crate::modifiers::row(ui, "fichier", |ui| {
                        for (f, l, tip) in Format::ALL {
                            if chip(ui, self.format == f, &crate::i18n::t(l), tip) {
                                self.format = f;
                            }
                        }
                    });
                });
                card(ui, "Nommage", &mut |ui| {
                    crate::modifiers::row(ui, "pour", |ui| {
                        for (k, pr) in PRESETS.iter().enumerate() {
                            if chip(ui, self.preset == k, pr.name, pr.tip) {
                                self.preset = k;
                            }
                        }
                    });
                    crate::modifiers::row(ui, "nom", |ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.stem).desired_width(ui.available_width() - 4.0));
                    });
                    // Les images qui seront écrites (celles en option selon le matériau, si on le connaît)
                    let has = Has { emit: true, height: prog.is_none_or(|p| p.uses_bump), ao: true };
                    let preset = &PRESETS[self.preset.min(PRESETS.len() - 1)];
                    let names: Vec<String> = preset.outs.iter().filter(|o| has.wants(o.when)).map(|o| {
                        let n = file_name(preset, o, self.stem.trim(), None, self.format);
                        if matches!(o.when, When::Emit | When::Ao) { format!("{n} *") } else { n }
                    }).collect();
                    ui.label(RichText::new(names.join("\n")).size(10.5).color(p.muted).family(egui::FontFamily::Monospace));
                    ui.label(RichText::new(crate::i18n::t("* seulement si le matériau la donne (émission, occlusion)")).size(10.0).color(p.muted));
                });
                card(ui, "Dossier d'arrivée", &mut |ui| {
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new(crate::lucide::FOLDER_OPEN).size(14.0)).min_size(vec2(28.0, 24.0)).corner_radius(8)).on_hover_text(crate::i18n::t("Choisir un autre dossier")).clicked() {
                            if let Some(d) = rfd::FileDialog::new().set_directory(full_dir(root, &self.dir).parent().unwrap_or(Path::new(root))).pick_folder() {
                                self.dir = short_dir(root, &d);
                            }
                        }
                        ui.add(egui::TextEdit::singleline(&mut self.dir).desired_width(ui.available_width() - 4.0)).on_hover_text(crate::i18n::t("Dossier où les images s'écrivent : dans le coffre (chemin court) ou ailleurs (chemin complet). Par défaut : « textures », à côté du matériau."));
                    });
                });
            });
            // Le bouton, l'avancement, le résultat
            ui.horizontal(|ui| {
                let ready = !busy && prog.is_some() && !self.stem.trim().is_empty() && !self.dir.trim().is_empty();
                let b = egui::Button::new(RichText::new(format!("{}  {}", crate::lucide::IMAGE_DOWN, crate::i18n::t("exporter"))).strong().color(p.on_accent)).fill(p.accent).corner_radius(8).min_size(vec2(0.0, 28.0));
                if ui.add_enabled(ready, b).on_hover_text(crate::i18n::t("Cuit le matériau et écrit les images dans le dossier")).clicked() {
                    go = true;
                }
                if let Some(j) = &self.job {
                    let f = j.progress.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0;
                    ui.add(egui::ProgressBar::new(f).desired_width(ui.available_width() - 4.0).text(format!("{}… {} s", crate::i18n::t("cuisson"), j.started.elapsed().as_secs())));
                } else if prog.is_none() {
                    ui.label(RichText::new(crate::i18n::t("le matériau a une erreur : corrige-la d'abord")).size(11.0).color(p.missing));
                }
            });
            match &self.result {
                Some(Ok((count, dir, secs))) => {
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new(crate::lucide::FOLDER_OPEN).size(13.0)).frame(false)).on_hover_text(crate::i18n::t("Ouvrir le dossier")).clicked() {
                            let _ = open::that_detached(dir);
                        }
                        ui.add(egui::Label::new(RichText::new(format!("{count} {} {} ({:.1} s)", crate::i18n::t_mid("images écrites dans"), short_dir(root, dir), secs)).size(11.0).color(p.text)).wrap());
                    });
                }
                Some(Err(e)) => {
                    ui.add(egui::Label::new(RichText::new(format!("{} : {e}", crate::i18n::t("export impossible"))).size(11.0).color(p.missing)).wrap());
                }
                None => {}
            }
        });
        if go {
            if let (Some(prog), Some(target)) = (prog, self.targets.get(self.target).cloned()) {
                let (prog, settings, root, dropped) = (prog.clone(), self.settings(root), root.to_string(), dropped.cloned());
                let progress = Arc::new(std::sync::atomic::AtomicU32::new(0));
                let (pr, ctx2) = (progress.clone(), ctx.clone());
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    // Une erreur dans le calcul ne doit pas laisser la barre tourner pour toujours
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let mesh = target_mesh(&target, &root, dropped.as_deref())?;
                        export(&mesh, &prog, &settings, &|f| {
                            pr.store((f.clamp(0.0, 1.0) * 1000.0) as u32, std::sync::atomic::Ordering::Relaxed);
                            ctx2.request_repaint();
                        })
                    }));
                    let _ = tx.send(r.unwrap_or_else(|_| Err(crate::i18n::t("l'export s'est arrêté"))));
                    ctx2.request_repaint();
                });
                self.result = None;
                self.job = Some(Job { rx, progress, started: std::time::Instant::now() });
            }
        }
        open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::{Graph, GraphKind};

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("aryadne-texexport-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Un matériau à calques : un fond gris, et dessus un calque uni (couleur, rugosité, métal réglés)
    fn uni(color: [f32; 3], rough: f32, metal: f32) -> Program {
        let mut top = crate::atelier::Layer::own("dessus", crate::atelier::fill_graph([0.2, 0.2, 0.2]));
        top.params.insert("couleur".into(), [color[0], color[1], color[2], 1.0]);
        top.params.insert("rugosité".into(), [rough; 4]);
        top.params.insert("métal".into(), [metal; 4]);
        let stack = crate::atelier::Stack { layers: vec![crate::atelier::Layer::own("fond", crate::atelier::fill_graph([0.5, 0.5, 0.5])), top] };
        let g = Graph { stack: Some(stack), ..Graph::empty(GraphKind::Material) };
        Program::compile(&g, "", 64).map_err(|e| e.1).unwrap()
    }

    #[test]
    fn materiau_uni_pixels_attendus() {
        let d = dir("uni");
        let prog = uni([1.0, 0.0, 0.0], 0.3, 1.0);
        let s = |preset: usize, format: Format| Settings { size: 32, format, preset, dir: d.clone(), stem: "rouge".into(), ao_rays: 0 };
        // Aryadne, PNG 8 bits : une image par carte, sans hauteur, occlusion ni émission (le matériau n'en donne pas)
        let files = export(&BakeMesh::plane(), &prog, &s(0, Format::Png8), &|_| {}).unwrap();
        let names: Vec<String> = files.iter().map(|f| f.file_name().unwrap().to_string_lossy().to_string()).collect();
        assert_eq!(names, ["rouge_couleur.png", "rouge_rugosite.png", "rouge_metal.png", "rouge_normale.png"]);
        assert_eq!(names, planned(&PRESETS[0], "rouge", Format::Png8, Has::default()));
        let px = |name: &str| image::open(d.join(name)).unwrap();
        let col = px("rouge_couleur.png").to_rgb8();
        assert_eq!(col.dimensions(), (32, 32));
        assert!(col.pixels().all(|p| p.0 == [255, 0, 0]), "{:?}", col.get_pixel(5, 5));
        // 0,3 × 255 = 76,5 : arrondi à 76 ou 77 selon le dernier chiffre du nombre à virgule
        let rough = px("rouge_rugosite.png");
        assert_eq!(rough.color(), image::ColorType::L8);
        assert!(rough.to_luma8().pixels().all(|p| p.0[0] == 76 || p.0[0] == 77), "{:?}", rough.to_luma8().get_pixel(3, 3));
        assert!(px("rouge_metal.png").to_luma8().pixels().all(|p| p.0[0] == 255));
        assert!(px("rouge_normale.png").to_rgb8().pixels().all(|p| p.0 == [128, 128, 255]));
        // Unreal : préfixe T_, normale DirectX, occlusion (blanche), rugosité et métal dans une image
        let files = export(&BakeMesh::plane(), &prog, &s(1, Format::Png8), &|_| {}).unwrap();
        let names: Vec<String> = files.iter().map(|f| f.file_name().unwrap().to_string_lossy().to_string()).collect();
        assert_eq!(names, ["T_rouge_BaseColor.png", "T_rouge_Normal.png", "T_rouge_ORM.png"]);
        let orm = px("T_rouge_ORM.png").to_rgb8();
        assert!(orm.pixels().all(|p| p.0[0] == 255 && (p.0[1] == 76 || p.0[1] == 77) && p.0[2] == 255), "{:?}", orm.get_pixel(1, 1));
        // Unity : métal en RVB, lissé (1 − rugosité) dans l'alpha
        export(&BakeMesh::plane(), &prog, &s(2, Format::Png8), &|_| {}).unwrap();
        let ms = px("rouge_MetallicSmoothness.png");
        assert_eq!(ms.color(), image::ColorType::Rgba8);
        assert!(ms.to_rgba8().pixels().all(|p| p.0[0] == 255 && p.0[2] == 255 && (p.0[3] == 178 || p.0[3] == 179)), "{:?}", ms.to_rgba8().get_pixel(1, 1));
        // Blender et glTF : leurs noms
        let names = |k: usize| planned(&PRESETS[k], "rouge", Format::Png8, Has { emit: true, height: true, ao: true });
        assert_eq!(names(3), ["rouge_Base_Color.png", "rouge_Roughness.png", "rouge_Metallic.png", "rouge_Normal.png", "rouge_Displacement.png", "rouge_Ambient_Occlusion.png", "rouge_Emission.png"]);
        assert_eq!(names(4), ["rouge_baseColor.png", "rouge_normal.png", "rouge_occlusionRoughnessMetallic.png", "rouge_emissive.png"]);
        // PNG 16 bits : 65 535 niveaux
        export(&BakeMesh::plane(), &prog, &s(0, Format::Png16), &|_| {}).unwrap();
        let rough = px("rouge_rugosite.png");
        assert_eq!(rough.color(), image::ColorType::L16);
        let v = rough.to_luma16().get_pixel(4, 4).0[0];
        assert!((v as i32 - 19661).abs() <= 1, "{v}");
        assert_eq!(px("rouge_couleur.png").to_rgb16().get_pixel(4, 4).0, [65535, 0, 0]);
        // EXR : nombres à virgule, couleur en linéaire
        let prog2 = uni([0.5, 0.25, 1.0], 0.3, 0.0);
        let files = export(&BakeMesh::plane(), &prog2, &s(0, Format::Exr), &|_| {}).unwrap();
        assert!(files.iter().all(|f| f.extension().unwrap() == "exr"));
        let c = px("rouge_couleur.exr").to_rgb32f().get_pixel(7, 9).0;
        assert!((c[0] - 0.5f32.powf(2.2)).abs() < 1e-3 && (c[1] - 0.25f32.powf(2.2)).abs() < 1e-3 && (c[2] - 1.0).abs() < 1e-3, "{c:?}");
        let r = px("rouge_rugosite.exr").to_rgb32f().get_pixel(7, 9).0;
        assert!((r[0] - 0.3).abs() < 1e-4 && r[0] == r[1] && r[1] == r[2], "{r:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn masque_en_uv_et_udim() {
        // Deux calques : rouge partout, bleu posé par un masque en nodes qui lit les UV (moitié droite)
        let d = dir("udim");
        let mut mask = Graph::empty(GraphKind::Material);
        let out = mask.add("mat_out", [300.0, 0.0]);
        let uv = mask.add("texcoord", [0.0, 0.0]);
        let lv = mask.add("levels", [150.0, 0.0]);
        // Un seuil franc à u = 0,5
        let def = crate::nodes::def("levels").unwrap();
        for (name, v) in [("noir", 0.49), ("blanc", 0.51)] {
            let k = def.inputs.iter().position(|i| i.0 == name).unwrap();
            mask.nodes.iter_mut().find(|n| n.id == lv).unwrap().values[k] = [v; 4];
        }
        mask.connect(uv, 1, lv, 0);
        mask.connect(lv, 0, out, 0);
        let mut top = crate::atelier::Layer::own("bleu", crate::atelier::fill_graph([0.0, 0.0, 1.0]));
        top.mask = crate::atelier::Mask { kind: "graphe".into(), graph: Some(crate::atelier::Inner(Box::new(mask))), ..Default::default() };
        let stack = crate::atelier::Stack { layers: vec![crate::atelier::Layer::own("rouge", crate::atelier::fill_graph([1.0, 0.0, 0.0])), top] };
        let prog = Program::compile(&Graph { stack: Some(stack), ..Graph::empty(GraphKind::Material) }, "", 64).map_err(|e| e.1).unwrap();
        let maps = bake(&BakeMesh::plane(), &prog, 64, 0, &|_| {}).unwrap();
        assert_eq!(maps.len(), 1);
        let m = &maps[0];
        assert!(m.covered.iter().all(|c| *c));
        // À gauche le rouge du dessous, à droite le bleu du calque masqué
        let at = |x: usize, y: usize| m.color[y * 64 + x];
        assert!(at(8, 20)[0] > 0.95 && at(8, 20)[2] < 0.05, "{:?}", at(8, 20));
        assert!(at(56, 40)[2] > 0.95 && at(56, 40)[0] < 0.05, "{:?}", at(56, 40));
        // UDIM : le même carreau, une moitié dans la tuile 1001 et l'autre dans la 1002
        let mut two = BakeMesh::plane();
        two.uv[1] = two.uv[1].map(|c| [c[0] + 1.0, c[1]]);
        let s = Settings { size: 32, format: Format::Png8, preset: 4, dir: d.clone(), stem: "mur".into(), ao_rays: 0 };
        let files = export(&two, &prog, &s, &|_| {}).unwrap();
        let names: Vec<String> = files.iter().map(|f| f.file_name().unwrap().to_string_lossy().to_string()).collect();
        assert_eq!(names, ["mur_baseColor.1001.png", "mur_normal.1001.png", "mur_occlusionRoughnessMetallic.1001.png", "mur_baseColor.1002.png", "mur_normal.1002.png", "mur_occlusionRoughnessMetallic.1002.png"]);
        // Chaque tuile ne porte que sa moitié (un triangle), le reste est la marge puis le gris du vide
        let t1 = image::open(d.join("mur_baseColor.1001.png")).unwrap().to_rgb8();
        assert_eq!(t1.get_pixel(28, 4).0, [0, 0, 255]);
        assert_eq!(t1.get_pixel(2, 4).0, [128, 128, 128]);
        // Dans la marge, le long de la diagonale : la valeur du texel couvert le plus proche (ici le rouge de gauche)
        assert_eq!(t1.get_pixel(5, 24).0, [255, 0, 0]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn normale_depuis_la_hauteur() {
        // Une rampe qui monte vers la droite : la normale penche vers la gauche (x négatif), pas en y
        let n = 16;
        let h: Vec<f32> = (0..n * n).map(|i| (i % n) as f32 / n as f32).collect();
        let nm = normals_from_height(&h, &vec![1.0; n * n], n, 400.0);
        let c = nm[8 * n + 8];
        assert!(c[0] < -0.3 && c[1].abs() < 1e-6 && c[2] > 0.5, "{c:?}");
        assert!((c[0] * c[0] + c[1] * c[1] + c[2] * c[2] - 1.0).abs() < 1e-5);
        // Une rampe qui monte vers le haut de l'image : la normale penche vers le bas (y négatif) en OpenGL
        let h: Vec<f32> = (0..n * n).map(|i| 1.0 - (i / n) as f32 / n as f32).collect();
        let c = normals_from_height(&h, &vec![1.0; n * n], n, 400.0)[8 * n + 8];
        assert!(c[1] < -0.3 && c[0].abs() < 1e-6, "{c:?}");
        // Sans force de relief, ou à plat : droit devant
        assert_eq!(normals_from_height(&h, &vec![0.0; n * n], n, 400.0)[20], [0.0, 0.0, 1.0]);
        assert_eq!(normals_from_height(&vec![0.5; n * n], &vec![1.0; n * n], n, 400.0)[20], [0.0, 0.0, 1.0]);
        // Écrite en OpenGL et en DirectX : le vert s'inverse
        let m = Maps { n: 1, tile: (0, 0), color: vec![[0.0; 3]], emit: vec![[0.0; 3]], rough: vec![0.5], metal: vec![0.0], height: vec![0.5], ao: vec![1.0], normal: vec![[0.0, 0.6, 0.8]], covered: vec![true] };
        let (gl, dx) = (texel(&m, Pack::Map(Src::NormalGl), 0, false).0, texel(&m, Pack::Map(Src::NormalDx), 0, false).0);
        assert!((gl[1] - 0.8).abs() < 1e-6 && (dx[1] - 0.2).abs() < 1e-6 && gl[0] == dx[0] && gl[2] == dx[2]);
    }

    #[test]
    fn relief_du_materiau_et_sphere() {
        // « carrelage en relief » branche une hauteur : la normale n'est plus plate, et la hauteur s'écrit
        let d = dir("relief");
        let g = crate::nodes::material_preset("carrelage en relief").unwrap();
        let prog = Program::compile(&g, "", 64).map_err(|e| e.1).unwrap();
        assert!(prog.uses_bump);
        let s = Settings { size: 128, format: Format::Png16, preset: 3, dir: d.clone(), stem: "carrelage".into(), ao_rays: 0 };
        let files = export(&BakeMesh::plane(), &prog, &s, &|_| {}).unwrap();
        assert!(files.iter().any(|f| f.ends_with("carrelage_Displacement.png")));
        let nm = image::open(d.join("carrelage_Normal.png")).unwrap().to_rgb16();
        let tilted = nm.pixels().filter(|p| (p.0[0] as i32 - 32768).abs() > 600 || (p.0[1] as i32 - 32768).abs() > 600).count();
        assert!(tilted > 50 && tilted < 128 * 128, "{tilted} texels penchés");
        let h = image::open(d.join("carrelage_Displacement.png")).unwrap().to_luma16();
        let (lo, hi) = h.pixels().fold((u16::MAX, 0u16), |a, p| (a.0.min(p.0[0]), a.1.max(p.0[0])));
        assert!(hi > lo + 2000, "hauteur de {lo} à {hi}");
        // La sphère de l'aperçu : toute la texture est couverte, et l'occlusion du modèle d'une sphère est blanche
        let prog = uni([0.2, 0.6, 0.4], 0.5, 0.0);
        let maps = bake(&BakeMesh::sphere(), &prog, 64, 8, &|_| {}).unwrap();
        assert_eq!(maps.len(), 1);
        assert!(maps[0].covered.iter().filter(|c| **c).count() > 64 * 64 * 95 / 100);
        assert!(maps[0].ao.iter().filter(|a| **a > 0.9).count() > 64 * 64 * 9 / 10);
        // Le cube de l'aperçu, sans UV : il reçoit des UV automatiques
        let b = crate::model3d::preview_box();
        let (pos, tris, edges) = b.mesh_parts();
        let cube = BakeMesh::auto(pos, tris, edges);
        assert_eq!(cube.uv.len(), cube.tris.len());
        let maps = bake(&cube, &prog, 64, 0, &|_| {}).unwrap();
        assert!(maps[0].covered.iter().filter(|c| **c).count() > 64 * 64 / 5);
        // Un maillage sans UV du tout : un message clair, pas d'image
        let none = BakeMesh { pos: vec![], tris: vec![], uv: vec![], mat_uv: None };
        assert!(bake(&none, &prog, 64, 0, &|_| {}).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn objet_de_scene_et_dossier() {
        // Un coffre : un matériau, une scène dont un objet le porte
        let root = dir("scene");
        let rs = root.to_string_lossy().to_string();
        std::fs::create_dir_all(root.join("nodes")).unwrap();
        std::fs::create_dir_all(root.join("3d")).unwrap();
        let mat = root.join("nodes/peinture.arymat");
        crate::atelier::fill_graph([0.0, 1.0, 0.0]).save(&mat).unwrap();
        let mut sc = crate::scene::Scene::default();
        let a = sc.add_primitive("@cube");
        sc.objects[a].name = "caisse".into();
        sc.objects[a].pos = [3.0, 0.0, 0.0];
        sc.objects[a].material = "nodes/peinture.arymat".into();
        let b = sc.add_primitive("@sphere");
        sc.objects[b].name = "boule".into();
        sc.add_role("soleil", "light");
        let scene_path = root.join(format!("3d/décor.{}", crate::formats::ext("scene")));
        sc.write(&scene_path).unwrap();
        // Le dossier par défaut : « textures », à côté du matériau
        assert_eq!(default_dir(&rs, &mat), "nodes/textures");
        assert_eq!(full_dir(&rs, "nodes/textures"), root.join("nodes/textures"));
        assert_eq!(full_dir(&rs, &root.join("ailleurs").to_string_lossy()), root.join("ailleurs"));
        // La fenêtre propose d'abord l'objet qui porte le matériau, puis l'aperçu, puis les autres objets
        let dlg = Dialog::new(&rs, &mat, None);
        assert_eq!(dlg.stem, "peinture");
        assert_eq!(dlg.dir, "nodes/textures");
        assert!(matches!(&dlg.targets[0], Target::Object { name, uses: true, .. } if name == "caisse"));
        assert_eq!(dlg.target, 0);
        assert_eq!(&dlg.targets[1..4], &[Target::Plane, Target::Sphere, Target::Cube]);
        assert!(matches!(&dlg.targets[4], Target::Object { name, uses: false, .. } if name == "boule"));
        assert_eq!(dlg.targets.len(), 5, "la lumière n'est pas un maillage");
        // Sans objet qui le porte : le carreau plat
        let other = root.join("nodes/autre.arymat");
        crate::atelier::fill_graph([1.0, 1.0, 1.0]).save(&other).unwrap();
        let dlg2 = Dialog::new(&rs, &other, None);
        assert_eq!(dlg2.targets[dlg2.target], Target::Plane);
        assert_eq!(Dialog::new(&rs, &other, Some("vase.obj")).targets[0], Target::Dropped("vase.obj".into()));
        // L'objet est cuit à sa place dans la scène, sur des UV automatiques
        let mesh = target_mesh(&dlg.targets[0], &rs, None).unwrap();
        assert!(mesh.uv.len() == mesh.tris.len() && !mesh.tris.is_empty());
        let cx = mesh.pos.iter().map(|p| p[0]).sum::<f32>() / mesh.pos.len() as f32;
        assert!((cx - 3.0).abs() < 0.2, "centre en x : {cx}");
        let prog = Program::compile(&Graph::load(&mat).unwrap(), &rs, 64).map_err(|e| e.1).unwrap();
        let files = export(&mesh, &prog, &Settings { ao_rays: 0, size: 64, ..dlg.settings(&rs) }, &|_| {}).unwrap();
        assert!(files[0].starts_with(root.join("nodes/textures")));
        let col = image::open(root.join("nodes/textures/peinture_couleur.png")).unwrap().to_rgb8();
        let green = col.pixels().filter(|p| p.0 == [0, 255, 0]).count();
        assert!(green > 64 * 64 / 5 && col.pixels().all(|p| p.0 == [0, 255, 0] || p.0 == [128, 128, 128]), "{green} texels verts");
        let _ = std::fs::remove_dir_all(&root);
    }
}
