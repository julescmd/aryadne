//! Moteur « tracé de chemins » (dans l'esprit de Cycles) : chaque passe ajoute un échantillon par pixel,
//! l'image s'affine au fil des passes. Rebonds de lumière, ombres douces, reflets brillants ou mats,
//! métaux, émission, ciel HDRI et lumières de la scène. Les matériaux en nodes sont lus comme dans la vue.
use super::*;
use rayon::prelude::*;

/// Image en cours d'accumulation.
pub struct Accum {
    pub w: usize,
    pub h: usize,
    /// Somme des échantillons (lumière linéaire) et couverture (0 : fond transparent)
    pub(super) sum: Vec<[f32; 3]>,
    pub(super) cover: Vec<f32>,
    /// Normale et couleur de surface du premier impact (guident le débruitage)
    pub(super) nrm: Vec<[f32; 3]>,
    pub(super) alb: Vec<[f32; 3]>,
    pub spp: u32,
    /// Rendu des couleurs AgX à l'affichage
    pub agx: bool,
}

#[derive(Clone, Copy)]
pub struct Settings {
    pub bounces: u32,
    /// Sol infini sous le modèle (il reçoit les ombres et renvoie la lumière)
    pub floor: bool,
    /// Plafond d'un échantillon (évite les points blancs isolés)
    pub clamp: f32,
    /// Sol visible (sinon il ne montre que les ombres, sur le fond de la vue)
    pub floor_visible: bool,
    /// Débruitage de l'image affichée
    pub denoise: bool,
    /// Calcul sur la carte graphique (si elle est là)
    pub gpu: bool,
    /// Profondeur de champ : rayon de l'ouverture (0 : tout net) et distance de mise au point (0 : le pivot de la vue)
    pub aperture: f32,
    pub focus: f32,
    /// Rendu des couleurs AgX (comme Blender), sinon filmique
    pub agx: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { bounces: 6, floor: true, clamp: 12.0, floor_visible: false, denoise: true, gpu: true, aperture: 0.0, focus: 0.0, agx: true }
    }
}

impl Accum {
    pub fn new(w: usize, h: usize) -> Accum {
        Accum { w, h, sum: vec![[0.0; 3]; w * h], cover: vec![0.0; w * h], nrm: vec![[0.0; 3]; w * h], alb: vec![[0.0; 3]; w * h], spp: 0, agx: true }
    }

    /// Lumière moyenne de chaque pixel (couleur non prémultipliée) et couverture.
    fn mean(&self) -> (Vec<[f32; 3]>, Vec<f32>) {
        let n = self.spp.max(1) as f32;
        let cover: Vec<f32> = self.cover.iter().map(|c| (c / n).clamp(0.0, 1.0)).collect();
        let col = self.sum.iter().zip(&cover).map(|(s, a)| if *a > 0.0 { [s[0] / n / a, s[1] / n / a, s[2] / n / a] } else { [0.0; 3] }).collect();
        (col, cover)
    }

    /// Débruitage : filtre « à trous » guidé par les normales, les couleurs de surface et la lumière
    /// (les bords d'objets et de matériaux restent nets, le grain des surfaces lisses disparaît).
    fn denoised(&self) -> (Vec<[f32; 3]>, Vec<f32>) {
        let (mut col, cover) = self.mean();
        let n = self.spp.max(1) as f32;
        let (w, h) = (self.w, self.h);
        let nrm: Vec<V3> = self.nrm.iter().map(|v| { let l = dot(*v, *v).sqrt(); if l > 1e-6 { scale(*v, 1.0 / l) } else { [0.0; 3] } }).collect();
        let alb: Vec<V3> = self.alb.iter().map(|a| scale(*a, 1.0 / n)).collect();
        // Moins d'échantillons : plus de bruit, donc un filtre plus permissif
        let sigma_l = (4.0 / n.sqrt()).clamp(0.05, 4.0);
        let k = [1.0 / 16.0, 1.0 / 4.0, 3.0 / 8.0, 1.0 / 4.0, 1.0 / 16.0];
        for step in [1usize, 2, 4, 8] {
            let src = col.clone();
            col = (0..w * h)
                .into_par_iter()
                .map(|i| {
                    if cover[i] <= 0.0 {
                        return src[i];
                    }
                    let (x, y) = ((i % w) as i64, (i / w) as i64);
                    let (c0, n0, a0) = (src[i], nrm[i], alb[i]);
                    let l0 = lum(c0);
                    let (mut acc, mut wsum) = ([0.0f32; 3], 0.0f32);
                    for dy in -2i64..=2 {
                        for dx in -2i64..=2 {
                            let (xx, yy) = (x + dx * step as i64, y + dy * step as i64);
                            if xx < 0 || yy < 0 || xx >= w as i64 || yy >= h as i64 {
                                continue;
                            }
                            let j = yy as usize * w + xx as usize;
                            if cover[j] <= 0.0 {
                                continue;
                            }
                            let wn = dot(n0, nrm[j]).max(0.0).powi(32);
                            let da = sub(a0, alb[j]);
                            let wa = (-dot(da, da) * 40.0).exp();
                            let wl = (-(l0 - lum(src[j])).abs() / (sigma_l * step as f32 * 0.5 + 1e-3)).exp();
                            let wk = k[(dx + 2) as usize] * k[(dy + 2) as usize] * wn * wa * wl;
                            acc = add(acc, scale(src[j], wk));
                            wsum += wk;
                        }
                    }
                    if wsum > 1e-6 { scale(acc, 1.0 / wsum) } else { c0 }
                })
                .collect();
        }
        (col, cover)
    }

    /// Débruitage par IA (Intel Open Image Denoise), s'il est installé.
    fn denoised_ai(&self) -> Option<(Vec<[f32; 3]>, Vec<f32>)> {
        let (col, cover) = self.mean();
        let n = self.spp.max(1) as f32;
        let nrm: Vec<V3> = self.nrm.iter().map(|v| { let l = dot(*v, *v).sqrt(); if l > 1e-6 { scale(*v, 1.0 / l) } else { [0.0; 3] } }).collect();
        let alb: Vec<V3> = self.alb.iter().map(|a| { let a = scale(*a, 1.0 / n); [a[0].clamp(0.0, 1.0), a[1].clamp(0.0, 1.0), a[2].clamp(0.0, 1.0)] }).collect();
        crate::oidn::denoise(&col, &alb, &nrm, self.w, self.h).ok().map(|c| (c, cover))
    }

    /// Image finale en 8 bits (RGBA), avec ou sans débruitage (l'IA si `ai` et qu'elle est là).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn image(&self, exposure: f32, denoise: bool) -> Vec<u8> {
        self.image_ex(exposure, denoise, false)
    }

    pub fn image_ex(&self, exposure: f32, denoise: bool, ai: bool) -> Vec<u8> {
        let ai_out = if denoise && ai && self.spp >= 1 { self.denoised_ai() } else { None };
        let (col, cover) = match ai_out {
            Some(x) => x,
            None if denoise && self.spp >= 2 => self.denoised(),
            None => self.mean(),
        };
        let e = 2f32.powf(exposure);
        let mut out = vec![0u8; self.w * self.h * 4];
        out.par_chunks_mut(4).enumerate().for_each(|(i, o)| {
            if cover[i] <= 0.0 {
                return;
            }
            o[..3].copy_from_slice(&if self.agx { agx(col[i], e) } else { tone(col[i], e) });
            o[3] = (cover[i] * 255.0) as u8;
        });
        out
    }

}

/// Rendu final (F12) en arrière-plan : une image à la taille demandée, jusqu'au nombre d'échantillons voulu.
pub struct Job {
    pub w: usize,
    pub h: usize,
    pub target: u32,
    pub spp: u32,
    pub rx: std::sync::mpsc::Receiver<(u32, Vec<u8>)>,
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub image: Option<Vec<u8>>,
    pub started: std::time::Instant,
    pub elapsed: f32,
}

impl Job {
    #[allow(clippy::too_many_arguments)]
    pub fn start(m: std::sync::Arc<Prepared>, cam: Camera, look: Look, set: Settings, w: usize, h: usize, target: u32, ctx: eframe::egui::Context) -> Job {
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = cancel.clone();
        std::thread::spawn(move || {
            let mut acc = Accum::new(w, h);
            acc.agx = set.agx;
            let mut last = std::time::Instant::now();
            let mut gpu = None;
            let on_gpu = set.gpu && super::gputrace::hardware() && super::gputrace::prepare(&mut gpu, &m, &look, 0xF12, w, h);
            while acc.spp < target && !stop.load(std::sync::atomic::Ordering::Relaxed) {
                match gpu.as_mut().filter(|_| on_gpu) {
                    Some(s) => super::gputrace::passes(s, &m, &cam, &look, &set, (target - acc.spp).min(8), &mut acc),
                    None => pass(&m, &cam, &look, &set, &mut acc),
                }
                // Une image intermédiaire par seconde environ, et la dernière
                if acc.spp == 1 || acc.spp == target || last.elapsed().as_secs_f32() > 1.0 {
                    last = std::time::Instant::now();
                    if tx.send((acc.spp, acc.image_ex(look.exposure, set.denoise, true))).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            }
        });
        Job { w, h, target, spp: 0, rx, cancel, image: None, started: std::time::Instant::now(), elapsed: 0.0 }
    }

    /// Reçoit les images arrivées ; vrai quand le rendu est fini.
    pub fn poll(&mut self) -> bool {
        while let Ok((spp, img)) = self.rx.try_recv() {
            self.spp = spp;
            self.image = Some(img);
        }
        if self.spp < self.target {
            self.elapsed = self.started.elapsed().as_secs_f32();
        }
        self.spp >= self.target
    }
}

/// Rendu des couleurs AgX (approximation de celui de Blender) : les hautes lumières saturées
/// tournent au blanc doucement au lieu de virer, puis sRGB.
pub fn agx(c: [f32; 3], exposure: f32) -> [u8; 3] {
    let v = [c[0].max(0.0) * exposure, c[1].max(0.0) * exposure, c[2].max(0.0) * exposure];
    let m = [[0.842_479, 0.078_434, 0.079_224], [0.042_328, 0.878_469, 0.079_166], [0.042_376, 0.078_434, 0.879_143]];
    let (min_ev, max_ev) = (-12.473_931, 4.026_069);
    let mut x = [0.0f32; 3];
    for k in 0..3 {
        let t = m[k][0] * v[0] + m[k][1] * v[1] + m[k][2] * v[2];
        let l = t.max(1e-10).log2().clamp(min_ev, max_ev);
        let n = (l - min_ev) / (max_ev - min_ev);
        let (x2, x4) = (n * n, n * n * n * n);
        x[k] = 15.5 * x4 * x2 - 40.14 * x4 * n + 31.96 * x4 - 6.868 * x2 * n + 0.4298 * x2 + 0.1191 * n - 0.00232;
    }
    let inv = [[1.196_879, -0.098_021, -0.099_030], [-0.052_897, 1.151_903, -0.098_961], [-0.052_972, -0.098_043, 1.151_074]];
    let mut out = [0u8; 3];
    for k in 0..3 {
        let y = inv[k][0] * x[0] + inv[k][1] * x[1] + inv[k][2] * x[2];
        out[k] = (y.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    }
    out
}

/// Petit générateur pseudo-aléatoire (rapide, sans état partagé entre pixels).
struct Rng(u32);
impl Rng {
    fn new(x: usize, y: usize, s: u32) -> Rng {
        let mut h = (x as u32).wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B) ^ s.wrapping_mul(0xC2B2_AE35);
        h ^= h >> 16;
        h = h.wrapping_mul(0x7FEB_352D);
        h ^= h >> 15;
        Rng(h | 1)
    }
    fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / 16_777_216.0
    }
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V3, b: V3) -> V3 {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}
fn scale(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn lum(c: V3) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Repère autour de `n` : deux tangentes.
fn basis(n: V3) -> (V3, V3) {
    let t = norm(cross(n, if n[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] }));
    (t, cross(n, t))
}

fn local(n: V3, x: f32, y: f32, z: f32) -> V3 {
    let (t, b) = basis(n);
    norm([t[0] * x + b[0] * y + n[0] * z, t[1] * x + b[1] * y + n[1] * z, t[2] * x + b[2] * y + n[2] * z])
}

/// Ce qu'une surface fait de la lumière.
pub(super) struct Surf {
    pub(super) albedo: V3,
    pub(super) rough: f32,
    pub(super) metal: f32,
    pub(super) emit: V3,
    pub(super) n: V3,
    /// Verre : part de lumière qui traverse, indice de réfraction, et si le rayon entre dans l'objet
    pub(super) trans: f32,
    pub(super) ior: f32,
    pub(super) entering: bool,
}

/// Matériau du triangle `ti` au point de coordonnées barycentriques (w0, w1).
pub(super) fn surface(m: &Prepared, look: &Look, ti: usize, w0: f32, w1: f32, view: V3, res: [f32; 2]) -> Surf {
    let t = m.tris[ti];
    let w2 = 1.0 - w0 - w1;
    let (pa, pb, pc) = (m.pos[t[0] as usize], m.pos[t[1] as usize], m.pos[t[2] as usize]);
    let face = norm(cross(sub(pb, pa), sub(pc, pa)));
    let mut n = if look.smooth {
        let (na, nb, nc) = (m.nrm[t[0] as usize], m.nrm[t[1] as usize], m.nrm[t[2] as usize]);
        norm([na[0] * w0 + nb[0] * w1 + nc[0] * w2, na[1] * w0 + nb[1] * w1 + nc[1] * w2, na[2] * w0 + nb[2] * w1 + nc[2] * w2])
    } else {
        face
    };
    let entering = dot(face, view) >= 0.0;
    if !entering {
        n = scale(n, -1.0);
    }
    let (albedo0, rough0, metal0) = look.material.params();
    let mut s = Surf { albedo: if look.material == Material::Color { m.colors[ti] } else { albedo0 }, rough: rough0, metal: metal0, emit: [0.0; 3], n, trans: 0.0, ior: 1.45, entering };
    let slot_prog = m.mat.get(ti).and_then(|&k| look.slot_nodes.get(k as usize)).and_then(|p| p.as_ref());
    if let Some(prog) = slot_prog.or(look.nodes.as_ref()) {
        let pos = [pa[0] * w0 + pb[0] * w1 + pc[0] * w2, pa[1] * w0 + pb[1] * w1 + pc[1] * w2, pa[2] * w0 + pb[2] * w1 + pc[2] * w2];
        let tuv = m.uv.get(ti).map(|u| [u[0][0] * w0 + u[1][0] * w1 + u[2][0] * w2, u[0][1] * w0 + u[1][1] * w1 + u[2][1] * w2]);
        let vc = m.colors[ti];
        let g = |x: f32| x.max(0.0).powf(1.0 / 2.2);
        let curv = if prog.uses_curvature {
            let cv = m.curv.get_or_init(|| vertex_curvature(&m.pos, &m.nrm, &m.tris));
            cv[t[0] as usize] * w0 + cv[t[1] as usize] * w1 + cv[t[2] as usize] * w2
        } else {
            0.0
        };
        let c = crate::nodes::Ctx { curv, uv: tuv.map(|u| [u[0], 1.0 - u[1]]).unwrap_or([pos[0] + 0.5, 0.5 - pos[1]]), pos, nrm: n, view, vcol: [g(vc[0]), g(vc[1]), g(vc[2]), 1.0], res };
        let sf = NODE_REGS.with(|regs| prog.material(&c, &mut regs.borrow_mut()));
        let l = |x: f32| x.clamp(0.0, 1.0).powf(2.2);
        s.albedo = [l(sf.color[0]), l(sf.color[1]), l(sf.color[2])];
        s.rough = sf.rough;
        s.metal = sf.metal;
        s.emit = [sf.emit[0].max(0.0).powf(2.2), sf.emit[1].max(0.0).powf(2.2), sf.emit[2].max(0.0).powf(2.2)];
        s.trans = sf.transmission * (1.0 - sf.metal);
        s.ior = sf.ior;
    }
    s.rough = s.rough.clamp(0.04, 1.0);
    s.metal = s.metal.clamp(0.0, 1.0);
    s
}

/// Réflectance (diffus + reflet GGX) pour la lumière qui arrive de `wi` et repart vers `wo`, cosinus compris.
fn brdf_cos(s: &Surf, wo: V3, wi: V3) -> V3 {
    let n = s.n;
    let (nl, nv) = (dot(n, wi), dot(n, wo).max(1e-4));
    if nl <= 0.0 {
        return [0.0; 3];
    }
    let h = norm(add(wo, wi));
    let (nh, vh) = (dot(n, h).max(0.0), dot(wo, h).max(0.0));
    let f0 = [0.04 + (s.albedo[0] - 0.04) * s.metal, 0.04 + (s.albedo[1] - 0.04) * s.metal, 0.04 + (s.albedo[2] - 0.04) * s.metal];
    let fw = (1.0 - vh).powi(5);
    let f = [f0[0] + (1.0 - f0[0]) * fw, f0[1] + (1.0 - f0[1]) * fw, f0[2] + (1.0 - f0[2]) * fw];
    let a = s.rough * s.rough;
    let a2 = a * a;
    let dd = nh * nh * (a2 - 1.0) + 1.0;
    let d = a2 / (std::f32::consts::PI * dd * dd).max(1e-7);
    let k = a / 2.0;
    let g = (nl / (nl * (1.0 - k) + k)) * (nv / (nv * (1.0 - k) + k));
    let spec = d * g / (4.0 * nl * nv).max(1e-6);
    let diff = (1.0 - s.metal) / std::f32::consts::PI;
    [
        (s.albedo[0] * diff * (1.0 - f[0]) + spec * f[0]) * nl,
        (s.albedo[1] * diff * (1.0 - f[1]) + spec * f[1]) * nl,
        (s.albedo[2] * diff * (1.0 - f[2]) + spec * f[2]) * nl,
    ]
}

fn spec_prob(s: &Surf) -> f32 {
    (0.15 + 0.7 * s.metal + 0.25 * (1.0 - s.rough)).clamp(0.1, 0.9)
}

/// Densité de probabilité de tirer `wi` (mélange diffus / reflet).
fn pdf(s: &Surf, wo: V3, wi: V3) -> f32 {
    let n = s.n;
    let nl = dot(n, wi);
    if nl <= 0.0 {
        return 0.0;
    }
    let ps = spec_prob(s);
    let h = norm(add(wo, wi));
    let (nh, vh) = (dot(n, h).max(1e-4), dot(wo, h).max(1e-4));
    let a = s.rough * s.rough;
    let a2 = a * a;
    let dd = nh * nh * (a2 - 1.0) + 1.0;
    let d = a2 / (std::f32::consts::PI * dd * dd).max(1e-7);
    ps * d * nh / (4.0 * vh) + (1.0 - ps) * nl / std::f32::consts::PI
}

/// Tire une direction de rebond.
fn sample(s: &Surf, wo: V3, r: &mut Rng) -> V3 {
    let (u1, u2) = (r.f(), r.f());
    if r.f() < spec_prob(s) {
        let a = s.rough * s.rough;
        let phi = std::f32::consts::TAU * u1;
        let cos_t = ((1.0 - u2) / (1.0 + (a * a - 1.0) * u2)).max(0.0).sqrt();
        let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
        let h = local(s.n, sin_t * phi.cos(), sin_t * phi.sin(), cos_t);
        let vh = dot(wo, h);
        sub(scale(h, 2.0 * vh), wo)
    } else {
        let rr = u1.sqrt();
        let phi = std::f32::consts::TAU * u2;
        local(s.n, rr * phi.cos(), rr * phi.sin(), (1.0 - u1).max(0.0).sqrt())
    }
}

/// Verre : direction après l'interface et teinte (la couleur du verre teinte ce qui traverse).
fn dielectric(s: &Surf, wo: V3, r: &mut Rng) -> (V3, V3) {
    let a = s.rough * s.rough * 0.5;
    let (u1, u2) = (r.f(), r.f());
    let phi = std::f32::consts::TAU * u1;
    let cos_t = ((1.0 - u2) / (1.0 + (a * a - 1.0) * u2)).max(0.0).sqrt();
    let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
    let h = local(s.n, sin_t * phi.cos(), sin_t * phi.sin(), cos_t);
    let eta = if s.entering { 1.0 / s.ior } else { s.ior };
    let ci = dot(wo, h).clamp(-1.0, 1.0);
    let k = 1.0 - eta * eta * (1.0 - ci * ci);
    // Fresnel (Schlick) ; sous l'angle limite : réflexion totale
    let r0 = ((1.0 - s.ior) / (1.0 + s.ior)).powi(2);
    let f = if k < 0.0 { 1.0 } else { r0 + (1.0 - r0) * (1.0 - ci.abs()).powi(5) };
    if r.f() < f {
        (sub(scale(h, 2.0 * ci), wo), [1.0; 3])
    } else {
        let t = sub(scale(h, eta * ci - k.max(0.0).sqrt()), scale(wo, eta));
        (norm(t), s.albedo)
    }
}

/// Tables pour tirer des directions du ciel en proportion de leur lumière (le soleil d'un HDRI
/// est trouvé tout de suite au lieu d'être touché par hasard).
pub(super) struct EnvCdf {
    pub(super) w: usize,
    pub(super) h: usize,
    /// Fonction de répartition cumulée de chaque rangée (normalisée), puis des rangées
    pub(super) cond: Vec<f32>,
    pub(super) marg: Vec<f32>,
    pub(super) total: f32,
}

pub(super) fn env_cdf(look: &Look) -> std::sync::Arc<EnvCdf> {
    static CACHE: std::sync::Mutex<Option<(usize, std::sync::Arc<EnvCdf>)>> = std::sync::Mutex::new(None);
    let key = std::sync::Arc::as_ptr(&look.env) as usize;
    let mut g = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((k, c)) = &*g {
        if *k == key {
            return c.clone();
        }
    }
    let m = &look.env.bg;
    let (w, h) = (m.w, m.h);
    let mut cond = vec![0.0f32; w * h];
    let mut rows = vec![0.0f32; h];
    for y in 0..h {
        let st = ((y as f32 + 0.5) / h as f32 * std::f32::consts::PI).sin();
        let mut acc = 0.0;
        for x in 0..w {
            acc += lum(m.px[y * w + x]).max(1e-6) * st;
            cond[y * w + x] = acc;
        }
        rows[y] = acc;
        if acc > 0.0 {
            for x in 0..w {
                cond[y * w + x] /= acc;
            }
        }
    }
    let total: f32 = rows.iter().sum();
    let mut marg = vec![0.0f32; h];
    let mut acc = 0.0;
    for y in 0..h {
        acc += rows[y];
        marg[y] = acc / total.max(1e-12);
    }
    let c = std::sync::Arc::new(EnvCdf { w, h, cond, marg, total });
    *g = Some((key, c.clone()));
    c
}

fn search(v: &[f32], u: f32) -> usize {
    v.partition_point(|&x| x < u).min(v.len() - 1)
}

/// Densité (par angle solide) de tirer la direction `d` du ciel.
fn env_pdf(look: &Look, cdf: &EnvCdf, d: V3) -> f32 {
    let dm = rot_y(d, -look.env_rot);
    let u = dm[0].atan2(-dm[2]) / std::f32::consts::TAU + 0.5;
    let v = dm[1].clamp(-1.0, 1.0).acos() / std::f32::consts::PI;
    let x = ((u * cdf.w as f32) as usize).min(cdf.w - 1);
    let y = ((v * cdf.h as f32) as usize).min(cdf.h - 1);
    let l = lum(look.env.bg.px[y * cdf.w + x]).max(1e-6);
    l * (cdf.w * cdf.h) as f32 / (cdf.total.max(1e-12) * 2.0 * std::f32::consts::PI * std::f32::consts::PI)
}

/// Tire une direction du ciel ; renvoie la direction et sa densité.
fn env_sample(look: &Look, cdf: &EnvCdf, r: &mut Rng) -> (V3, f32) {
    let y = search(&cdf.marg, r.f());
    let row = &cdf.cond[y * cdf.w..(y + 1) * cdf.w];
    let x = search(row, r.f());
    let dm = look.env.bg.dir_of(x as f32 + r.f(), y as f32 + r.f());
    let d = rot_y(dm, look.env_rot);
    (d, env_pdf(look, cdf, d))
}

/// Le ciel dans la direction `d`.
fn sky(look: &Look, d: V3) -> V3 {
    let c = look.env.bg.sample(rot_y(d, -look.env_rot));
    scale(c, look.env_strength)
}

/// Lumière qui revient le long d'un rayon de caméra. Renvoie la couleur et si le rayon a touché quelque chose.
#[allow(clippy::too_many_arguments)]
fn radiance(m: &Prepared, bvh: &crate::bvh::Bvh, look: &Look, lamps: &[Lamp], set: &Settings, mut o: V3, mut d: V3, r: &mut Rng, res: [f32; 2]) -> (V3, f32, V3, V3) {
    let mut first = ([0.0f32; 3], [0.0f32; 3]);
    let mut l = [0.0f32; 3];
    let cdf = env_cdf(look);
    // Densité du dernier rebond tiré par la surface (0 : verre ou rayon de caméra, pas de pondération)
    let mut last_pdf = 0.0f32;
    let mut through = [1.0f32; 3];
    let mut hit_any = false;
    for bounce in 0..=set.bounces {
        let hit = bvh.closest(o, d, 1e-4, 1e9);
        // Sol : un plan sous le modèle
        let floor_t = if set.floor && d[1] < -1e-5 { let t = (m.floor - o[1]) / d[1]; (t > 1e-4).then_some(t) } else { None };
        let (p, s) = match (hit, floor_t) {
            (Some(h), f) if f.is_none_or(|ft| h.t < ft) => {
                let p = add(o, scale(d, h.t));
                (p, surface(m, look, h.tri as usize, 1.0 - h.u - h.v, h.u, scale(d, -1.0), res))
            }
            (_, Some(ft)) if bounce == 0 && !set.floor_visible && !look.show_bg => {
                // Sol « attrape-ombres » : seule l'ombre se pose sur le fond de la vue
                let p = add(add(o, scale(d, ft)), [0.0, 1e-3, 0.0]);
                let (mut dark, mut total) = (0.0f32, 0.0f32);
                for lamp in lamps {
                    let (dir, reach, fall) = if lamp.point {
                        let to = sub(lamp.at, p);
                        let dd = dot(to, to).sqrt().max(1e-6);
                        (scale(to, 1.0 / dd), dd, (1.0 - (dd / lamp.range.max(1e-6)).min(1.0).powi(2)).powi(2))
                    } else {
                        let (t1, t2) = basis(lamp.at);
                        let (a, rr) = (r.f() * std::f32::consts::TAU, r.f().sqrt() * 0.03);
                        (norm(add(lamp.at, add(scale(t1, a.cos() * rr), scale(t2, a.sin() * rr)))), 1e6, 1.0)
                    };
                    let e = lamp.intensity * fall * dir[1].max(0.0);
                    if e > 0.0 {
                        total += e;
                        if bvh.occluded(p, dir, 1e-4, reach) {
                            dark += e;
                        }
                    }
                }
                // Le ciel aussi : un rayon vers le haut, au hasard (occlusion douce au pied des objets)
                let (u1, u2) = (r.f(), r.f());
                let up = local([0.0, 1.0, 0.0], u1.sqrt() * (u2 * std::f32::consts::TAU).cos(), u1.sqrt() * (u2 * std::f32::consts::TAU).sin(), (1.0 - u1).sqrt());
                let ao = if bvh.occluded(p, up, 1e-4, 1e9) { 0.25 } else { 0.0 };
                let shade = (dark / total.max(1e-6) * 0.5 + ao).min(0.85);
                return ([0.0; 3], shade, [0.0, 1.0, 0.0], [0.0; 3]);
            }
            (_, Some(ft)) => {
                let p = add(o, scale(d, ft));
                (p, Surf { albedo: [0.55; 3], rough: 0.9, metal: 0.0, emit: [0.0; 3], n: [0.0, 1.0, 0.0], trans: 0.0, ior: 1.45, entering: true })
            }
            _ => {
                // Rien : le ciel (caché derrière le modèle si on ne montre pas le fond)
                if bounce > 0 || look.show_bg {
                    // Pondération (MIS) avec le tirage direct du ciel fait au rebond d'avant
                    let wgt = if last_pdf > 0.0 {
                        let pl = env_pdf(look, &cdf, d);
                        last_pdf * last_pdf / (last_pdf * last_pdf + pl * pl)
                    } else {
                        1.0
                    };
                    l = add(l, scale(mul(through, sky(look, d)), wgt));
                }
                break;
            }
        };
        hit_any = true;
        if bounce == 0 {
            first = (s.n, s.albedo);
        }
        let wo = scale(d, -1.0);
        l = add(l, mul(through, s.emit));
        // Lumières de la scène : un rayon d'ombre vers chacune
        let po = add(p, scale(s.n, 1e-3));
        for lamp in lamps {
            let (dir, reach, fall) = if lamp.point {
                let to = sub(lamp.at, p);
                let dd = dot(to, to).sqrt().max(1e-6);
                (scale(to, 1.0 / dd), dd, (1.0 - (dd / lamp.range.max(1e-6)).min(1.0).powi(2)).powi(2))
            } else {
                // Soleil un peu large : ombres douces
                let (t1, t2) = basis(lamp.at);
                let (a, rr) = (r.f() * std::f32::consts::TAU, r.f().sqrt() * 0.03);
                (norm(add(lamp.at, add(scale(t1, a.cos() * rr), scale(t2, a.sin() * rr)))), 1e6, 1.0)
            };
            if fall <= 0.0 || dot(s.n, dir) <= 0.0 || bvh.occluded(po, dir, 1e-4, reach) {
                continue;
            }
            let e = scale(lamp.color, lamp.intensity * fall * std::f32::consts::PI * (1.0 - s.trans));
            l = add(l, mul(through, mul(brdf_cos(&s, wo, dir), e)));
        }
        // Le ciel, tiré en proportion de sa lumière, pondéré avec le tirage de la surface (MIS)
        if s.trans < 1.0 {
            let (dir, pl) = env_sample(look, &cdf, r);
            if pl > 0.0 && dot(s.n, dir) > 0.0 && !bvh.occluded(po, dir, 1e-4, 1e9) && !(set.floor && dir[1] < 0.0 && (m.floor - po[1]) / dir[1] > 0.0) {
                let f = brdf_cos(&s, wo, dir);
                let pb = pdf(&s, wo, dir);
                let wgt = pl * pl / (pl * pl + pb * pb);
                l = add(l, scale(mul(through, mul(f, sky(look, dir))), wgt * (1.0 - s.trans) / pl));
            }
        }
        // Verre : réflexion ou réfraction selon Fresnel, autour d'une micro-normale (verre dépoli si rugueux)
        if s.trans > 0.0 && r.f() < s.trans {
            last_pdf = 0.0;
            let (dir, tint) = dielectric(&s, wo, r);
            through = mul(through, tint);
            // La surface d'en face, pour un rayon qui traverse : on repart de l'autre côté
            o = add(p, scale(dir, 2e-3));
            d = dir;
            if bounce >= 2 {
                let q = lum(through).clamp(0.05, 0.95);
                if r.f() > q {
                    break;
                }
                through = scale(through, 1.0 / q);
            }
            continue;
        }
        // Rebond
        let wi = sample(&s, wo, r);
        let pd = pdf(&s, wo, wi);
        if pd <= 1e-6 {
            break;
        }
        let f = brdf_cos(&s, wo, wi);
        through = mul(through, scale(f, 1.0 / pd));
        last_pdf = pd;
        // Roulette russe : les chemins qui ne portent presque plus rien s'arrêtent
        if bounce >= 2 {
            let q = lum(through).clamp(0.05, 0.95);
            if r.f() > q {
                break;
            }
            through = scale(through, 1.0 / q);
        }
        o = po;
        d = wi;
    }
    let k = lum(l);
    if k > set.clamp {
        l = scale(l, set.clamp / k);
    }
    (l, if hit_any { 1.0 } else { 0.0 }, first.0, first.1)
}

/// Ajoute un échantillon par pixel à `acc`.
pub fn pass(m: &Prepared, cam: &Camera, look: &Look, set: &Settings, acc: &mut Accum) {
    let (w, h) = (acc.w, acc.h);
    let bvh = m.bvh.get_or_init(|| crate::bvh::Bvh::new(&m.pos, &m.tris));
    let lamps: Vec<Lamp> = if look.lamps.is_empty() { vec![Lamp { point: false, at: look.light.dir(), color: look.light.color, intensity: look.light.intensity, range: 0.0 }] } else { look.lamps.clone() };
    let focal = w.min(h) as f32 * 1.2;
    let ortho_scale = focal / cam.dist;
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let spp = acc.spp;
    let res = [w as f32, h as f32];
    let rows: Vec<Vec<(V3, f32, V3, V3)>> = (0..h)
        .into_par_iter()
        .map(|y| {
            (0..w)
                .map(|x| {
                    let mut r = Rng::new(x, y, spp);
                    let (jx, jy) = (r.f(), r.f());
                    let (mut o, mut d) = if cam.ortho {
                        (cam.unrotate([(x as f32 + jx - cx) / ortho_scale - cam.pan[0], -(y as f32 + jy - cy) / ortho_scale - cam.pan[1], cam.dist]), cam.unrotate([0.0, 0.0, -1.0]))
                    } else {
                        (cam.unrotate([-cam.pan[0], -cam.pan[1], cam.dist]), cam.unrotate(norm([(x as f32 + jx - cx) / focal, -(y as f32 + jy - cy) / focal, -1.0])))
                    };
                    // Profondeur de champ : le rayon part d'un point de l'objectif et passe par le plan net
                    if set.aperture > 0.0 && !cam.ortho {
                        let f = if set.focus > 0.0 { set.focus } else { cam.dist };
                        let fwd = cam.unrotate([0.0, 0.0, -1.0]);
                        let pf = add(o, scale(d, f / dot(d, fwd).max(1e-4)));
                        let (r1, r2) = (r.f().sqrt() * set.aperture, r.f() * std::f32::consts::TAU);
                        o = add(o, cam.unrotate([r1 * r2.cos(), r1 * r2.sin(), 0.0]));
                        d = norm(sub(pf, o));
                    }
                    let (c, cover, n, a) = radiance(m, bvh, look, &lamps, set, o, d, &mut r, res);
                    (c, if look.show_bg { 1.0 } else { cover }, n, a)
                })
                .collect()
        })
        .collect();
    for (y, row) in rows.into_iter().enumerate() {
        for (x, (c, a, n, al)) in row.into_iter().enumerate() {
            let k = y * w + x;
            acc.sum[k] = add(acc.sum[k], c);
            acc.cover[k] += a;
            acc.nrm[k] = add(acc.nrm[k], n);
            acc.alb[k] = add(acc.alb[k], al);
        }
    }
    acc.spp += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_de_chemins() {
        // Une sphère éclairée par le soleil : de la lumière au centre, une ombre sur le sol sous elle
        let model = crate::scene::primitive("@sphere").unwrap();
        let prep = Prepared::new(&model, false, 0);
        let look = Look { shadows: true, ..Default::default() };
        let cam = Camera::default();
        let mut acc = Accum::new(64, 48);
        for _ in 0..8 {
            pass(&prep, &cam, &look, &Settings::default(), &mut acc);
        }
        assert_eq!(acc.spp, 8);
        let img = acc.image(0.0, true);
        let px = |x: usize, y: usize| &img[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert!(px(32, 24)[3] == 255 && px(32, 24)[0] > 40, "{:?}", px(32, 24));
        // Les coins du haut : ni modèle ni sol, fond transparent
        assert_eq!(px(1, 1)[3], 0);
    }
}

#[cfg(test)]
mod look_test {
    use super::*;
    #[test]
    #[ignore]
    fn comparer_png() {
        let model = crate::scene::primitive("@sphere").unwrap();
        let prep = Prepared::new(&model, false, 0);
        let look = Look { shadows: true, ..Default::default() };
        let cam = Camera::default();
        let f = render(&prep, &cam, &look, 320, 240);
        image::RgbaImage::from_raw(320, 240, f.rgba).unwrap().save("/tmp/raster.png").unwrap();
        let mut acc = Accum::new(320, 240);
        for _ in 0..4 {
            pass(&prep, &cam, &look, &Settings::default(), &mut acc);
        }
        image::RgbaImage::from_raw(320, 240, acc.image_ex(0.0, true, true)).unwrap().save("/tmp/trace_ai.png").unwrap();
        image::RgbaImage::from_raw(320, 240, acc.image(0.0, false)).unwrap().save("/tmp/trace.png").unwrap();
        image::RgbaImage::from_raw(320, 240, acc.image(0.0, true)).unwrap().save("/tmp/trace_dn.png").unwrap();
    }
}
