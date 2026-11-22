//! Tracé de chemins sur la carte graphique (wgpu, shader de calcul) : le même rendu que
//! `pathtrace`, des dizaines de fois plus vite. Le BVH, les normales et les matériaux (évalués
//! aux coins de chaque triangle) partent sur le GPU une fois ; ensuite chaque passe ajoute un
//! échantillon par pixel, relu dans une `Accum` pour l'affichage et le débruitage.
use super::pathtrace::{Accum, Settings};
use super::*;
use std::sync::{Mutex, OnceLock};

const SHADER: &str = r#"
struct Lamp { at: vec4<f32>, color: vec4<f32>, extra: vec4<f32> };
struct Params {
  size: vec4<u32>,
  flags: vec4<u32>,
  eye: vec4<f32>,
  rx: vec4<f32>,
  ry: vec4<f32>,
  rz: vec4<f32>,
  cam: vec4<f32>,
  misc: vec4<f32>,
  env: vec4<u32>,
  extra: vec4<f32>,
  lamps: array<Lamp, 16>,
};
@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var<storage, read> nodes: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> tris: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> nrm: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> idx: array<vec4<u32>>;
@group(0) @binding(5) var<storage, read> mats: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read> envmap: array<vec4<f32>>;
@group(0) @binding(7) var<storage, read_write> acc: array<vec4<f32>>;

const PI: f32 = 3.14159265;
var<private> seed: u32;
fn rnd() -> f32 {
  seed = seed * 747796405u + 2891336453u;
  var w = ((seed >> ((seed >> 28u) + 4u)) ^ seed) * 277803737u;
  w = (w >> 22u) ^ w;
  return f32(w >> 8u) / 16777216.0;
}

struct Hit { t: f32, tri: u32, u: f32, v: f32 };

fn trace(o: vec3<f32>, d: vec3<f32>, tmax0: f32, any: bool) -> Hit {
  var h = Hit(-1.0, 0u, 0.0, 0.0);
  var tmax = tmax0;
  let inv = 1.0 / d;
  var stack: array<u32, 48>;
  var sp = 1u;
  stack[0] = 0u;
  loop {
    if (sp == 0u) { break; }
    sp = sp - 1u;
    let ni = stack[sp];
    let a = nodes[ni * 2u];
    let b = nodes[ni * 2u + 1u];
    let t0 = (a.xyz - o) * inv;
    let t1 = (b.xyz - o) * inv;
    let lo = max(max(min(t0.x, t1.x), min(t0.y, t1.y)), max(min(t0.z, t1.z), 1e-4));
    let hi = min(min(max(t0.x, t1.x), max(t0.y, t1.y)), min(max(t0.z, t1.z), tmax));
    if (lo > hi) { continue; }
    let start = bitcast<u32>(a.w);
    let count = bitcast<u32>(b.w);
    if (count > 0u) {
      for (var k = start; k < start + count; k = k + 1u) {
        let v0 = tris[k * 3u];
        let e1 = tris[k * 3u + 1u].xyz;
        let e2 = tris[k * 3u + 2u].xyz;
        let p = cross(d, e2);
        let det = dot(e1, p);
        if (abs(det) < 1e-12) { continue; }
        let f = 1.0 / det;
        let s = o - v0.xyz;
        let u = f * dot(s, p);
        if (u < 0.0 || u > 1.0) { continue; }
        let q = cross(s, e1);
        let v = f * dot(d, q);
        if (v < 0.0 || u + v > 1.0) { continue; }
        let t = f * dot(e2, q);
        if (t > 1e-4 && t < tmax) {
          tmax = t;
          h = Hit(t, bitcast<u32>(v0.w), u, v);
          if (any) { return h; }
        }
      }
    } else if (sp + 2u <= 48u) {
      stack[sp] = start;
      stack[sp + 1u] = start + 1u;
      sp = sp + 2u;
    }
  }
  return h;
}

fn sky(d0: vec3<f32>) -> vec3<f32> {
  let s = sin(-P.misc.y);
  let c = cos(-P.misc.y);
  let d = vec3<f32>(d0.x * c + d0.z * s, d0.y, -d0.x * s + d0.z * c);
  let w = P.env.x;
  let hh = P.env.y;
  let u = atan2(d.x, -d.z) / (2.0 * PI) + 0.5;
  let v = acos(clamp(d.y, -1.0, 1.0)) / PI;
  let x = u * f32(w) - 0.5;
  let y = clamp(v * f32(hh) - 0.5, 0.0, f32(hh) - 1.0);
  let x0 = floor(x);
  let y0 = floor(y);
  let fx = x - x0;
  let fy = y - y0;
  let xa = u32((i32(x0) % i32(w) + i32(w)) % i32(w));
  let xb = (xa + 1u) % w;
  let ya = min(u32(y0), hh - 1u);
  let yb = min(ya + 1u, hh - 1u);
  let c00 = envmap[ya * w + xa].xyz;
  let c10 = envmap[ya * w + xb].xyz;
  let c01 = envmap[yb * w + xa].xyz;
  let c11 = envmap[yb * w + xb].xyz;
  return mix(mix(c00, c10, fx), mix(c01, c11, fx), fy) * P.misc.z;
}

struct Surf { albedo: vec3<f32>, rough: f32, metal: f32, emit: vec3<f32>, n: vec3<f32> };

fn basis(n: vec3<f32>) -> mat3x3<f32> {
  var up = vec3<f32>(0.0, 1.0, 0.0);
  if (abs(n.y) > 0.9) { up = vec3<f32>(1.0, 0.0, 0.0); }
  let t = normalize(cross(n, up));
  let b = cross(n, t);
  return mat3x3<f32>(t, b, n);
}

fn spec_prob(s: Surf) -> f32 { return clamp(0.15 + 0.7 * s.metal + 0.25 * (1.0 - s.rough), 0.1, 0.9); }

fn brdf_cos(s: Surf, wo: vec3<f32>, wi: vec3<f32>) -> vec3<f32> {
  let nl = dot(s.n, wi);
  let nv = max(dot(s.n, wo), 1e-4);
  if (nl <= 0.0) { return vec3<f32>(0.0); }
  let h = normalize(wo + wi);
  let nh = max(dot(s.n, h), 0.0);
  let vh = max(dot(wo, h), 0.0);
  let f0 = mix(vec3<f32>(0.04), s.albedo, s.metal);
  let f = f0 + (1.0 - f0) * pow(1.0 - vh, 5.0);
  let a = s.rough * s.rough;
  let a2 = a * a;
  let dd = nh * nh * (a2 - 1.0) + 1.0;
  let D = a2 / max(PI * dd * dd, 1e-7);
  let k = a / 2.0;
  let G = (nl / (nl * (1.0 - k) + k)) * (nv / (nv * (1.0 - k) + k));
  let spec = D * G / max(4.0 * nl * nv, 1e-6);
  let diff = (1.0 - s.metal) / PI;
  return (s.albedo * diff * (1.0 - f) + spec * f) * nl;
}

fn pdf(s: Surf, wo: vec3<f32>, wi: vec3<f32>) -> f32 {
  let nl = dot(s.n, wi);
  if (nl <= 0.0) { return 0.0; }
  let ps = spec_prob(s);
  let h = normalize(wo + wi);
  let nh = max(dot(s.n, h), 1e-4);
  let vh = max(dot(wo, h), 1e-4);
  let a = s.rough * s.rough;
  let a2 = a * a;
  let dd = nh * nh * (a2 - 1.0) + 1.0;
  let D = a2 / max(PI * dd * dd, 1e-7);
  return ps * D * nh / (4.0 * vh) + (1.0 - ps) * nl / PI;
}

fn sample_dir(s: Surf, wo: vec3<f32>) -> vec3<f32> {
  let u1 = rnd();
  let u2 = rnd();
  let B = basis(s.n);
  if (rnd() < spec_prob(s)) {
    let a = s.rough * s.rough;
    let phi = 2.0 * PI * u1;
    let ct = sqrt(max((1.0 - u2) / (1.0 + (a * a - 1.0) * u2), 0.0));
    let st = sqrt(max(1.0 - ct * ct, 0.0));
    let h = normalize(B * vec3<f32>(st * cos(phi), st * sin(phi), ct));
    return 2.0 * dot(wo, h) * h - wo;
  }
  let r = sqrt(u1);
  let phi = 2.0 * PI * u2;
  return normalize(B * vec3<f32>(r * cos(phi), r * sin(phi), sqrt(max(1.0 - u1, 0.0))));
}

fn surface(h: Hit, view: vec3<f32>) -> Surf {
  let ti = h.tri;
  let w1 = h.u;
  let w2 = h.v;
  let w0 = 1.0 - w1 - w2;
  let ix = idx[ti];
  let pa = nrm[ix.x];
  let pb = nrm[ix.y];
  let pc = nrm[ix.z];
  let n = normalize(pa.xyz * w0 + pb.xyz * w1 + pc.xyz * w2);
  let a0 = mats[ti * 6u + 0u]; let m0 = mats[ti * 6u + 1u];
  let a1 = mats[ti * 6u + 2u]; let m1 = mats[ti * 6u + 3u];
  let a2 = mats[ti * 6u + 4u]; let m2 = mats[ti * 6u + 5u];
  let al = a0 * w0 + a1 * w1 + a2 * w2;
  let me = m0 * w0 + m1 * w1 + m2 * w2;
  return Surf(al.xyz, clamp(al.w, 0.04, 1.0), clamp(me.w, 0.0, 1.0), me.xyz, n);
}

fn lamp_dir(i: u32, p: vec3<f32>) -> vec4<f32> {
  let L = P.lamps[i];
  if (L.at.w > 0.5) {
    let to = L.at.xyz - p;
    let d = max(length(to), 1e-6);
    let x = min(d / max(L.extra.x, 1e-6), 1.0);
    let fall = (1.0 - x * x) * (1.0 - x * x);
    return vec4<f32>(to / d, d * select(0.0, 1.0, fall > 0.0) + select(-1.0, 0.0, fall > 0.0));
  }
  let B = basis(L.at.xyz);
  let a = rnd() * 2.0 * PI;
  let rr = sqrt(rnd()) * 0.03;
  return vec4<f32>(normalize(L.at.xyz + B[0] * cos(a) * rr + B[1] * sin(a) * rr), 1e6);
}

fn lamp_fall(i: u32, d: f32) -> f32 {
  let L = P.lamps[i];
  if (L.at.w > 0.5) {
    let x = min(d / max(L.extra.x, 1e-6), 1.0);
    return (1.0 - x * x) * (1.0 - x * x);
  }
  return 1.0;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let w = P.size.x;
  let hgt = P.size.y;
  if (gid.x >= w || gid.y >= hgt) { return; }
  let pix = gid.y * w + gid.x;
  seed = pix * 9781u + P.size.z * 6271u + 1u;
  _ = rnd();
  let jx = rnd();
  let jy = rnd();
  let cx = f32(w) / 2.0;
  let cy = f32(hgt) / 2.0;
  var o: vec3<f32>;
  var d: vec3<f32>;
  let sx = (f32(gid.x) + jx - cx);
  let sy = (f32(gid.y) + jy - cy);
  if (P.eye.w > 0.5) {
    let v = vec3<f32>(sx / P.cam.y - P.cam.z, -sy / P.cam.y - P.cam.w, P.extra.x);
    o = P.rx.xyz * v.x + P.ry.xyz * v.y + P.rz.xyz * v.z;
    d = -P.rz.xyz;
  } else {
    o = P.eye.xyz;
    let v = normalize(vec3<f32>(sx / P.cam.x, -sy / P.cam.x, -1.0));
    d = normalize(P.rx.xyz * v.x + P.ry.xyz * v.y + P.rz.xyz * v.z);
  }
  var L = vec3<f32>(0.0);
  var through = vec3<f32>(1.0);
  var cover = 0.0;
  var first_n = vec3<f32>(0.0);
  var first_a = vec3<f32>(0.0);
  let floor_on = P.flags.x == 1u;
  let floor_vis = P.flags.y == 1u;
  let show_bg = P.flags.z == 1u;
  for (var bounce = 0u; bounce <= P.size.w; bounce = bounce + 1u) {
    let hit = trace(o, d, 1e9, false);
    var ft = -1.0;
    if (floor_on && d.y < -1e-5) {
      let t = (P.misc.x - o.y) / d.y;
      if (t > 1e-4) { ft = t; }
    }
    var s: Surf;
    var p: vec3<f32>;
    if (hit.t > 0.0 && (ft < 0.0 || hit.t < ft)) {
      p = o + d * hit.t;
      s = surface(hit, -d);
      if (dot(s.n, -d) < 0.0) { s.n = -s.n; }
    } else if (ft > 0.0) {
      p = o + d * ft;
      if (bounce == 0u && !floor_vis && !show_bg) {
        // Sol attrape-ombres
        let q = p + vec3<f32>(0.0, 1e-3, 0.0);
        var dark = 0.0;
        var total = 0.0;
        for (var i = 0u; i < P.env.z; i = i + 1u) {
          let ld = lamp_dir(i, q);
          if (ld.w < 0.0) { continue; }
          let e = P.lamps[i].color.w * lamp_fall(i, ld.w) * max(ld.y, 0.0);
          if (e > 0.0) {
            total = total + e;
            if (trace(q, ld.xyz, ld.w, true).t > 0.0) { dark = dark + e; }
          }
        }
        let u1 = rnd();
        let u2 = rnd();
        let up = normalize(basis(vec3<f32>(0.0, 1.0, 0.0)) * vec3<f32>(sqrt(u1) * cos(u2 * 2.0 * PI), sqrt(u1) * sin(u2 * 2.0 * PI), sqrt(1.0 - u1)));
        var ao = 0.0;
        if (trace(q, up, 1e9, true).t > 0.0) { ao = 0.25; }
        cover = min(dark / max(total, 1e-6) * 0.5 + ao, 0.85);
        L = vec3<f32>(0.0);
        first_n = vec3<f32>(0.0, 1.0, 0.0);
        break;
      }
      s = Surf(vec3<f32>(0.55), 0.9, 0.0, vec3<f32>(0.0), vec3<f32>(0.0, 1.0, 0.0));
    } else {
      if (bounce > 0u || show_bg) { L = L + through * sky(d); }
      break;
    }
    cover = 1.0;
    if (bounce == 0u) { first_n = s.n; first_a = s.albedo; }
    let wo = -d;
    L = L + through * s.emit;
    let po = p + s.n * 1e-3;
    for (var i = 0u; i < P.env.z; i = i + 1u) {
      let ld = lamp_dir(i, p);
      if (ld.w < 0.0 || dot(s.n, ld.xyz) <= 0.0) { continue; }
      if (trace(po, ld.xyz, ld.w, true).t > 0.0) { continue; }
      let Lp = P.lamps[i];
      let e = Lp.color.xyz * Lp.color.w * lamp_fall(i, ld.w) * PI;
      L = L + through * brdf_cos(s, wo, ld.xyz) * e;
    }
    let wi = sample_dir(s, wo);
    let pd = pdf(s, wo, wi);
    if (pd <= 1e-6) { break; }
    through = through * brdf_cos(s, wo, wi) / pd;
    if (bounce >= 2u) {
      let q = clamp(dot(through, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.05, 0.95);
      if (rnd() > q) { break; }
      through = through / q;
    }
    o = po;
    d = wi;
  }
  let lum = dot(L, vec3<f32>(0.2126, 0.7152, 0.0722));
  if (lum > P.misc.w) { L = L * (P.misc.w / lum); }
  if (show_bg) { cover = 1.0; }
  let base = pix * 3u;
  acc[base] = acc[base] + vec4<f32>(L * cover, cover);
  acc[base + 1u] = acc[base + 1u] + vec4<f32>(first_a, 0.0);
  acc[base + 2u] = acc[base + 2u] + vec4<f32>(first_n, 0.0);
}
"#;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    soft: bool,
    pub name: String,
}

/// Une vraie carte graphique est-elle là ?
pub fn hardware() -> bool {
    gpu().is_some_and(|g| !g.soft)
}

static GPU: OnceLock<Option<Gpu>> = OnceLock::new();

fn gpu() -> Option<&'static Gpu> {
    GPU.get_or_init(|| {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })).ok()?;
        let info = adapter.get_info();
        let limits = adapter.limits();
        if limits.max_storage_buffers_per_shader_stage < 7 {
            return None;
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { label: Some("aryadne-trace"), required_limits: limits, ..Default::default() })).ok()?;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("trace"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let storage = |b: u32, ro: bool| wgpu::BindGroupLayoutEntry { binding: b, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: ro }, has_dynamic_offset: false, min_binding_size: None }, count: None };
        let mut entries = vec![wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }];
        for b in 1..=6 {
            entries.push(storage(b, true));
        }
        entries.push(storage(7, false));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &entries });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("trace"), layout: Some(&pl), module: &module, entry_point: Some("main"), compilation_options: Default::default(), cache: None });
        // Un rendu logiciel (pas de vraie carte) serait plus lent que le moteur CPU : on le signale
        let soft = info.device_type == wgpu::DeviceType::Cpu;
        Some(Gpu { device, queue, pipeline, layout, soft, name: format!("{} ({:?})", info.name, info.backend) })
    })
    .as_ref()
}

/// La carte graphique utilisée, si le rendu GPU marche ici.
pub fn device_name() -> Option<String> {
    gpu().map(|g| g.name.clone())
}

/// Une scène envoyée au GPU, et l'image en cours.
pub struct Scene {
    key: u64,
    group: wgpu::BindGroup,
    params: wgpu::Buffer,
    acc: wgpu::Buffer,
    read: wgpu::Buffer,
    w: usize,
    h: usize,
    spp: u32,
}

fn as_bytes<T>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

fn buffer(g: &Gpu, data: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
    use wgpu::util::DeviceExt;
    let data = if data.is_empty() { &[0u8; 16][..] } else { data };
    g.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: data, usage })
}

/// Prépare (ou garde) la scène sur le GPU pour ce maillage, ce rendu et cette taille.
pub fn prepare(slot: &mut Option<Scene>, m: &Prepared, look: &Look, key: u64, w: usize, h: usize) -> bool {
    let Some(g) = gpu() else { return false };
    if slot.as_ref().is_some_and(|s| s.key == key && s.w == w && s.h == h) {
        return true;
    }
    let bvh = m.bvh.get_or_init(|| crate::bvh::Bvh::new(&m.pos, &m.tris));
    let (nodes, tris) = bvh.flat();
    let nrm: Vec<[f32; 4]> = m.nrm.iter().map(|n| [n[0], n[1], n[2], 0.0]).collect();
    // Indices des sommets ; 4e valeur inutilisée (la normale de face se recalcule si besoin)
    let idx: Vec<[u32; 4]> = m.tris.iter().map(|t| [t[0], t[1], t[2], 0]).collect();
    // Matériaux aux trois coins de chaque triangle : (couleur, rugosité), (émission, métal)
    let res = [w as f32, h as f32];
    let mats: Vec<[f32; 4]> = {
        use rayon::prelude::*;
        (0..m.tris.len())
            .into_par_iter()
            .flat_map_iter(|ti| {
                let corners = [(1.0, 0.0), (0.0, 1.0), (0.0, 0.0)];
                corners.into_iter().flat_map(move |(w0, w1)| {
                    let n = {
                        let t = m.tris[ti];
                        let (na, nb, nc) = (m.nrm[t[0] as usize], m.nrm[t[1] as usize], m.nrm[t[2] as usize]);
                        let w2 = 1.0 - w0 - w1;
                        norm([na[0] * w0 + nb[0] * w1 + nc[0] * w2, na[1] * w0 + nb[1] * w1 + nc[1] * w2, na[2] * w0 + nb[2] * w1 + nc[2] * w2])
                    };
                    let s = super::pathtrace::surface(m, look, ti, w0, w1, n, res);
                    [[s.albedo[0], s.albedo[1], s.albedo[2], s.rough], [s.emit[0], s.emit[1], s.emit[2], s.metal]]
                })
            })
            .collect()
    };
    let env = &look.env.bg;
    let envd: Vec<[f32; 4]> = env.px.iter().map(|c| [c[0], c[1], c[2], 0.0]).collect();
    let st = wgpu::BufferUsages::STORAGE;
    let bufs = [buffer(g, as_bytes(&nodes), st), buffer(g, as_bytes(&tris), st), buffer(g, as_bytes(&nrm), st), buffer(g, as_bytes(&idx), st), buffer(g, as_bytes(&mats), st), buffer(g, as_bytes(&envd), st)];
    let acc_size = (w * h * 3 * 16) as u64;
    let acc = g.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: acc_size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    let read = g.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: acc_size, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    let params = g.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of::<Params>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    let mut entries = vec![wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() }];
    for (k, b) in bufs.iter().enumerate() {
        entries.push(wgpu::BindGroupEntry { binding: k as u32 + 1, resource: b.as_entire_binding() });
    }
    entries.push(wgpu::BindGroupEntry { binding: 7, resource: acc.as_entire_binding() });
    let group = g.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &g.layout, entries: &entries });
    // Les tampons restent vivants tant que le groupe les référence
    std::mem::forget(bufs);
    *slot = Some(Scene { key, group, params, acc, read, w, h, spp: 0 });
    true
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GLamp {
    at: [f32; 4],
    color: [f32; 4],
    extra: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Params {
    size: [u32; 4],
    flags: [u32; 4],
    eye: [f32; 4],
    rx: [f32; 4],
    ry: [f32; 4],
    rz: [f32; 4],
    cam: [f32; 4],
    misc: [f32; 4],
    env: [u32; 4],
    extra: [f32; 4],
    lamps: [GLamp; 16],
}

/// Recommence l'image (la vue a bougé).
pub fn reset(s: &mut Scene) {
    if let Some(g) = gpu() {
        g.queue.write_buffer(&s.acc, 0, &vec![0u8; s.w * s.h * 48]);
    }
    s.spp = 0;
}

/// Lance `n` passes, puis relit l'image dans `out`.
pub fn passes(s: &mut Scene, m: &Prepared, cam: &Camera, look: &Look, set: &Settings, n: u32, out: &mut Accum) {
    let Some(g) = gpu() else { return };
    let (w, h) = (s.w, s.h);
    let lamps: Vec<Lamp> = if look.lamps.is_empty() { vec![Lamp { point: false, at: look.light.dir(), color: look.light.color, intensity: look.light.intensity, range: 0.0 }] } else { look.lamps.clone() };
    let mut gl = [GLamp { at: [0.0; 4], color: [0.0; 4], extra: [0.0; 4] }; 16];
    for (k, l) in lamps.iter().take(16).enumerate() {
        gl[k] = GLamp { at: [l.at[0], l.at[1], l.at[2], if l.point { 1.0 } else { 0.0 }], color: [l.color[0], l.color[1], l.color[2], l.intensity], extra: [l.range, 0.0, 0.0, 0.0] };
    }
    let focal = w.min(h) as f32 * 1.2;
    let col = |v: V3| {
        let r = cam.unrotate(v);
        [r[0], r[1], r[2], 0.0]
    };
    let eye = cam.unrotate([-cam.pan[0], -cam.pan[1], cam.dist]);
    let env = &look.env.bg;
    for _ in 0..n {
        let p = Params {
            size: [w as u32, h as u32, s.spp, set.bounces],
            flags: [set.floor as u32, set.floor_visible as u32, look.show_bg as u32, look.smooth as u32],
            eye: [eye[0], eye[1], eye[2], if cam.ortho { 1.0 } else { 0.0 }],
            rx: col([1.0, 0.0, 0.0]),
            ry: col([0.0, 1.0, 0.0]),
            rz: col([0.0, 0.0, 1.0]),
            cam: [focal, focal / cam.dist, cam.pan[0], cam.pan[1]],
            misc: [m.floor, look.env_rot, look.env_strength, set.clamp],
            env: [env.w as u32, env.h as u32, lamps.len().min(16) as u32, 0],
            extra: [cam.dist, 0.0, 0.0, 0.0],
            lamps: gl,
        };
        g.queue.write_buffer(&s.params, 0, as_bytes(std::slice::from_ref(&p)));
        let mut enc = g.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&g.pipeline);
            pass.set_bind_group(0, &s.group, &[]);
            pass.dispatch_workgroups((w as u32).div_ceil(8), (h as u32).div_ceil(8), 1);
        }
        g.queue.submit([enc.finish()]);
        s.spp += 1;
    }
    // Relecture
    let mut enc = g.device.create_command_encoder(&Default::default());
    enc.copy_buffer_to_buffer(&s.acc, 0, &s.read, 0, (w * h * 48) as u64);
    g.queue.submit([enc.finish()]);
    let slice = s.read.slice(..);
    let done = std::sync::Arc::new(Mutex::new(None));
    let d2 = done.clone();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        if let Ok(mut g) = d2.lock() {
            *g = Some(r.is_ok());
        }
    });
    let _ = g.device.poll(wgpu::PollType::wait_indefinitely());
    if done.lock().ok().and_then(|g| *g) == Some(true) {
        if let Ok(data) = slice.get_mapped_range() {
            let f: &[f32] = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const f32, w * h * 12) };
            if out.w != w || out.h != h {
                *out = Accum::new(w, h);
            }
            for i in 0..w * h {
                let b = i * 12;
                out.sum[i] = [f[b], f[b + 1], f[b + 2]];
                out.cover[i] = f[b + 3];
                out.alb[i] = [f[b + 4], f[b + 5], f[b + 6]];
                out.nrm[i] = [f[b + 8], f[b + 9], f[b + 10]];
            }
        }
        s.read.unmap();
        out.spp = s.spp;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_gpu() {
        // Seulement là où une carte graphique (ou un rendu logiciel) est disponible
        if gpu().is_none() {
            return;
        }
        let model = crate::scene::primitive("@sphere").unwrap();
        let prep = Prepared::new(&model, false, 0);
        let look = Look { shadows: true, ..Default::default() };
        let mut slot = None;
        assert!(prepare(&mut slot, &prep, &look, 1, 64, 48));
        let s = slot.as_mut().unwrap();
        let mut acc = Accum::new(64, 48);
        passes(s, &prep, &Camera::default(), &look, &Settings::default(), 8, &mut acc);
        assert_eq!(acc.spp, 8);
        let img = acc.image(0.0, false);
        let px = |x: usize, y: usize| &img[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert!(px(32, 24)[3] == 255 && px(32, 24)[0] > 40, "{:?}", px(32, 24));
        assert_eq!(px(1, 1)[3], 0);
    }
}

#[cfg(test)]
mod look_test {
    use super::*;
    #[test]
    #[ignore]
    fn gpu_png() {
        let model = crate::scene::primitive("@sphere").unwrap();
        let prep = Prepared::new(&model, false, 0);
        let look = Look { shadows: true, ..Default::default() };
        let mut slot = None;
        assert!(prepare(&mut slot, &prep, &look, 1, 320, 240));
        let mut acc = Accum::new(320, 240);
        let t = std::time::Instant::now();
        passes(slot.as_mut().unwrap(), &prep, &Camera::default(), &look, &Settings::default(), 16, &mut acc);
        eprintln!("16 passes : {:?} sur {}", t.elapsed(), device_name().unwrap_or_default());
        image::RgbaImage::from_raw(320, 240, acc.image(0.0, false)).unwrap().save("/tmp/gpu.png").unwrap();
        image::RgbaImage::from_raw(320, 240, acc.image(0.0, true)).unwrap().save("/tmp/gpu_dn.png").unwrap();
    }
}
