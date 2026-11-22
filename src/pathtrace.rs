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
    sum: Vec<[f32; 3]>,
    cover: Vec<f32>,
    pub spp: u32,
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
}

impl Default for Settings {
    fn default() -> Self {
        Settings { bounces: 6, floor: true, clamp: 12.0, floor_visible: false }
    }
}

impl Accum {
    pub fn new(w: usize, h: usize) -> Accum {
        Accum { w, h, sum: vec![[0.0; 3]; w * h], cover: vec![0.0; w * h], spp: 0 }
    }

    /// L'image affichable (RGBA, tonemap et exposition de la vue).
    pub fn rgba(&self, exposure: f32) -> Vec<u8> {
        let n = self.spp.max(1) as f32;
        let e = 2f32.powf(exposure);
        let mut out = vec![0u8; self.w * self.h * 4];
        out.par_chunks_mut(4).enumerate().for_each(|(i, o)| {
            let s = self.sum[i];
            let a = (self.cover[i] / n).clamp(0.0, 1.0);
            if a <= 0.0 {
                return;
            }
            // La couleur est prémultipliée par la couverture : on la ramène avant le tonemap
            let c = [s[0] / n / a, s[1] / n / a, s[2] / n / a];
            let t = tone(c, e);
            o[..3].copy_from_slice(&t);
            o[3] = (a * 255.0) as u8;
        });
        out
    }
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
struct Surf {
    albedo: V3,
    rough: f32,
    metal: f32,
    emit: V3,
    n: V3,
}

/// Matériau du triangle `ti` au point de coordonnées barycentriques (w0, w1).
fn surface(m: &Prepared, look: &Look, ti: usize, w0: f32, w1: f32, view: V3, res: [f32; 2]) -> Surf {
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
    if dot(face, view) < 0.0 {
        n = scale(n, -1.0);
    }
    let (albedo0, rough0, metal0) = look.material.params();
    let mut s = Surf { albedo: if look.material == Material::Color { m.colors[ti] } else { albedo0 }, rough: rough0, metal: metal0, emit: [0.0; 3], n };
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

/// Le ciel dans la direction `d`.
fn sky(look: &Look, d: V3) -> V3 {
    let c = look.env.bg.sample(rot_y(d, -look.env_rot));
    scale(c, look.env_strength)
}

/// Lumière qui revient le long d'un rayon de caméra. Renvoie la couleur et si le rayon a touché quelque chose.
#[allow(clippy::too_many_arguments)]
fn radiance(m: &Prepared, bvh: &crate::bvh::Bvh, look: &Look, lamps: &[Lamp], set: &Settings, mut o: V3, mut d: V3, r: &mut Rng, res: [f32; 2]) -> (V3, f32) {
    let mut l = [0.0f32; 3];
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
                return ([0.0; 3], shade);
            }
            (_, Some(ft)) => {
                let p = add(o, scale(d, ft));
                (p, Surf { albedo: [0.55; 3], rough: 0.9, metal: 0.0, emit: [0.0; 3], n: [0.0, 1.0, 0.0] })
            }
            _ => {
                // Rien : le ciel (caché derrière le modèle si on ne montre pas le fond)
                if bounce > 0 || look.show_bg {
                    l = add(l, mul(through, sky(look, d)));
                }
                break;
            }
        };
        hit_any = true;
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
            let e = scale(lamp.color, lamp.intensity * fall * std::f32::consts::PI);
            l = add(l, mul(through, mul(brdf_cos(&s, wo, dir), e)));
        }
        // Rebond
        let wi = sample(&s, wo, r);
        let pd = pdf(&s, wo, wi);
        if pd <= 1e-6 {
            break;
        }
        let f = brdf_cos(&s, wo, wi);
        through = mul(through, scale(f, 1.0 / pd));
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
    (l, if hit_any { 1.0 } else { 0.0 })
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
    let rows: Vec<Vec<(V3, f32)>> = (0..h)
        .into_par_iter()
        .map(|y| {
            (0..w)
                .map(|x| {
                    let mut r = Rng::new(x, y, spp);
                    let (jx, jy) = (r.f(), r.f());
                    let (o, d) = if cam.ortho {
                        (cam.unrotate([(x as f32 + jx - cx) / ortho_scale - cam.pan[0], -(y as f32 + jy - cy) / ortho_scale - cam.pan[1], cam.dist]), cam.unrotate([0.0, 0.0, -1.0]))
                    } else {
                        (cam.unrotate([-cam.pan[0], -cam.pan[1], cam.dist]), cam.unrotate(norm([(x as f32 + jx - cx) / focal, -(y as f32 + jy - cy) / focal, -1.0])))
                    };
                    let (c, cover) = radiance(m, bvh, look, &lamps, set, o, d, &mut r, res);
                    (c, if look.show_bg { 1.0 } else { cover })
                })
                .collect()
        })
        .collect();
    for (y, row) in rows.into_iter().enumerate() {
        for (x, (c, a)) in row.into_iter().enumerate() {
            let k = y * w + x;
            acc.sum[k] = add(acc.sum[k], c);
            acc.cover[k] += a;
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
        let img = acc.rgba(0.0);
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
        for _ in 0..64 {
            pass(&prep, &cam, &look, &Settings::default(), &mut acc);
        }
        image::RgbaImage::from_raw(320, 240, acc.rgba(0.0)).unwrap().save("/tmp/trace.png").unwrap();
    }
}
