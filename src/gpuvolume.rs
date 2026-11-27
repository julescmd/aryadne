//! Dessin d'un volume (fumée, feu) directement par la carte graphique.
//!
//! Un triangle couvre l'image ; pour chaque pixel, le noyau lance un rayon à travers la grille
//! et additionne ce qu'il rencontre (*ray marching*) :
//!
//! - **La fumée** cache ce qui est derrière elle, selon sa densité.
//! - **Les flammes** brillent : leur couleur vient de la température, leur force du champ `flamme`.
//! - **L'ombre** : à chaque pas dans la fumée, quelques pas vers la lumière principale.
//! - **La profondeur** d'une image calculée ailleurs (`gpudraw::Soft`) arrête le rayon : un objet
//!   devant la fumée la cache, un objet dedans y est noyé.
//!
//! Le tampon lu est celui du solveur, tel quel (`Source::Live`) : **aucune relecture**. Un volume
//! repris du cache est envoyé à la carte quand sa version change (`Source::Data`), pour le même aspect.
//! La même formule, sur le processeur, sert au rendu final : `volume::Sampler::march`.

use crate::gpusim::{self, Gpu};
use crate::smoke::Live;
use crate::volume::{Grids, Look, Mat};
use eframe::egui;
use std::sync::{Arc, OnceLock};

const SHADER: &str = r#"
struct U {
    inv_vp: mat4x4<f32>,
    to_grid: mat4x4<f32>,
    depth_row: vec4<f32>,
    light: vec4<f32>,
    res: vec4<f32>,
    look: vec4<f32>,
    color: vec4<f32>,
    soft: vec4<f32>,
    size: vec4<f32>,
}
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> sca: array<vec2<u32>>;
@group(0) @binding(2) var soft_depth: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    return vec4<f32>(p, 0.0, 1.0);
}

fn cell(q: vec3<i32>) -> vec4<f32> {
    let n = vec3<i32>(u.res.xyz);
    let m = clamp(q, vec3<i32>(0), n - vec3<i32>(1));
    let v = sca[u32(m.x) + u32(n.x) * (u32(m.y) + u32(n.y) * u32(m.z))];
    return vec4<f32>(unpack2x16float(v.x), unpack2x16float(v.y));
}

// Densité, température, carburant, flamme en un point de la grille (en voxels) ; zéro dehors
fn at(g: vec3<f32>) -> vec4<f32> {
    let q = g - 0.5;
    if (any(q < vec3<f32>(-0.5)) || any(q > u.res.xyz - 0.5)) { return vec4<f32>(0.0); }
    let f = floor(q);
    let t = q - f;
    let i = vec3<i32>(f);
    let c00 = mix(cell(i), cell(i + vec3<i32>(1, 0, 0)), t.x);
    let c10 = mix(cell(i + vec3<i32>(0, 1, 0)), cell(i + vec3<i32>(1, 1, 0)), t.x);
    let c01 = mix(cell(i + vec3<i32>(0, 0, 1)), cell(i + vec3<i32>(1, 0, 1)), t.x);
    let c11 = mix(cell(i + vec3<i32>(0, 1, 1)), cell(i + vec3<i32>(1, 1, 1)), t.x);
    return mix(mix(c00, c10, t.y), mix(c01, c11, t.y), t.z);
}

fn flame(t0: f32) -> vec3<f32> {
    let t = max(t0, 0.0);
    return vec3<f32>(min(t * 1.6, 1.0), min(t * t * 0.42, 1.0), min(max(t - 1.6, 0.0) * 0.55, 1.0));
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let ndc = vec2<f32>(pos.x / u.size.x * 2.0 - 1.0, 1.0 - pos.y / u.size.y * 2.0);
    let a4 = u.inv_vp * vec4<f32>(ndc, 0.0, 1.0);
    let b4 = u.inv_vp * vec4<f32>(ndc, 1.0, 1.0);
    let o = a4.xyz / a4.w;
    let e = b4.xyz / b4.w;
    let len = length(e - o);
    let dir = (e - o) / len;
    var tmax = len;
    // Ce qui est déjà dessiné arrête le rayon
    if (u.soft.z > 0.5) {
        let px = clamp(vec2<i32>(pos.xy / u.size.xy * u.soft.xy), vec2<i32>(0), vec2<i32>(u.soft.xy) - vec2<i32>(1));
        let d = textureLoad(soft_depth, px, 0).r;
        let d0 = dot(u.depth_row, vec4<f32>(o, 1.0));
        let dd = dot(u.depth_row.xyz, dir);
        if (dd > 1e-9 && d < 1e30) { tmax = min(tmax, (d + u.soft.w - d0) / dd); }
    }
    let og = (u.to_grid * vec4<f32>(o, 1.0)).xyz;
    let dg = (u.to_grid * vec4<f32>(dir, 0.0)).xyz;
    let safe = select(dg, vec3<f32>(1e-9), abs(dg) < vec3<f32>(1e-9));
    let ta = (vec3<f32>(0.0) - og) / safe;
    let tb = (u.res.xyz - og) / safe;
    let lo = min(ta, tb);
    let hi = max(ta, tb);
    let t0 = max(max(max(lo.x, lo.y), lo.z), 0.0);
    let t1 = min(min(min(hi.x, hi.y), hi.z), tmax);
    if (t1 <= t0) { return vec4<f32>(0.0); }
    let gl = length(dg);
    var dt = 0.9 / gl;
    if ((t1 - t0) / dt > 128.0) { dt = (t1 - t0) / 128.0; }
    let ds = dt * gl * u.res.w;
    let jitter = fract(sin(dot(pos.xy, vec2<f32>(12.9898, 78.233))) * 43758.5453);
    let lg = normalize((u.to_grid * vec4<f32>(u.light.xyz, 0.0)).xyz) * 2.0;
    var col = vec3<f32>(0.0);
    var trans = 1.0;
    var t = t0 + dt * jitter;
    for (var k = 0; k < 130; k++) {
        if (t >= t1 || trans < 0.01) { break; }
        let p = og + dg * t;
        let s = at(p);
        if (s.x > 1e-3 || s.w > 1e-3) {
            let a = 1.0 - exp(-s.x * u.look.x * ds);
            var shade = 1.0;
            if (u.look.z > 0.0 && a > 1e-3) {
                var d = 0.0;
                for (var j = 1; j <= 6; j++) { d += at(p + lg * f32(j)).x; }
                shade = exp(-d * u.look.x * 2.0 * u.res.w * u.look.z * 0.6);
            }
            let lit = 0.18 + 0.82 * shade;
            let glow = s.w * u.look.y * ds / u.res.w * 0.25;
            col += trans * (a * u.color.rgb * lit + glow * flame(s.y));
            trans *= 1.0 - a;
        }
        t += dt;
    }
    let alpha = clamp(max(1.0 - trans, max(col.r, max(col.g, col.b))), 0.0, 1.0);
    if (alpha <= 0.0) { return vec4<f32>(0.0); }
    return vec4<f32>(pow(clamp(col / alpha, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / 2.2)) * alpha, alpha);
}
"#;

const U_SIZE: u64 = 64 * 2 + 16 * 7;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

struct Programs {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    blank: wgpu::TextureView,
}

fn programs(g: &Gpu) -> &'static Programs {
    static P: OnceLock<Programs> = OnceLock::new();
    P.get_or_init(|| {
        let d = &g.device;
        let module = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("volume"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("volume"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            ],
        });
        let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("volume"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("volume"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some("fs"), targets: &[Some(wgpu::ColorTargetState { format: FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let blank = d.create_texture(&wgpu::TextureDescriptor { label: Some("volume-sans-profondeur"), size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::R32Float, usage: wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] }).create_view(&Default::default());
        Programs { layout, pipeline, blank }
    })
}

/// D'où viennent les voxels
pub enum Source<'a> {
    /// Le tampon du solveur, tel qu'il vient de l'écrire
    Live(&'a Live),
    /// Des voxels du processeur : leur version (ils ne sont renvoyés que si elle change), et de quoi les obtenir
    Data(u64, &'a dyn Fn() -> Arc<Grids>),
}

/// Un volume à dessiner
pub struct Item<'a> {
    pub source: Source<'a>,
    pub res: [usize; 3],
    pub voxel: f32,
    pub look: Look,
    /// Du monde vers la grille, en voxels (colonnes d'abord)
    pub to_grid: Mat,
}

/// La cible du dessin d'un volume : sa texture, et l'image que l'interface affiche
#[derive(Default)]
pub struct Canvas {
    size: [u32; 2],
    color: Option<wgpu::Texture>,
    view: Option<wgpu::TextureView>,
    id: Option<egui::TextureId>,
    uniform: Option<wgpu::Buffer>,
    /// Voxels envoyés par le processeur : tampon, version
    own: Option<(wgpu::Buffer, u64)>,
    soft: Option<(wgpu::Texture, wgpu::TextureView, [u32; 2], u64)>,
}

impl Drop for Canvas {
    fn drop(&mut self) {
        if let (Some(id), Some(ui)) = (self.id.take(), crate::gfx::ui()) {
            ui.renderer.write().free_texture(&id);
        }
    }
}

impl Canvas {
    fn ensure(&mut self, g: &Gpu, size: [u32; 2]) {
        if self.size == size && self.color.is_some() {
            return;
        }
        let d = &g.device;
        let color = d.create_texture(&wgpu::TextureDescriptor { label: Some("volume-couleur"), size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: FORMAT, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
        let view = color.create_view(&Default::default());
        if let Some(ui) = crate::gfx::ui().filter(|_| g.shared) {
            let mut r = ui.renderer.write();
            match self.id {
                Some(id) => r.update_egui_texture_from_wgpu_texture(d, &view, wgpu::FilterMode::Linear, id),
                None => self.id = Some(r.register_native_texture(d, &view, wgpu::FilterMode::Linear)),
            }
        }
        if let Some(old) = self.color.replace(color) {
            old.destroy();
        }
        self.view = Some(view);
        self.size = size;
    }

    /// L'image à montrer dans l'interface
    pub fn texture(&self) -> Option<egui::TextureId> {
        self.id
    }

    /// Relit l'image (RGBA 8 bits, couleur déjà multipliée par l'opacité). Pour les tests et le banc.
    pub fn pixels(&self) -> Option<Vec<u8>> {
        let (g, color) = (gpusim::gpu()?, self.color.as_ref()?);
        let [w, h] = self.size;
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let out = g.device.create_buffer(&wgpu::BufferDescriptor { label: Some("volume-relecture"), size: (row * h) as u64, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mut enc = g.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &out, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        g.queue.submit([enc.finish()]);
        let done = Arc::new(std::sync::atomic::AtomicU8::new(0));
        let d2 = done.clone();
        out.slice(..).map_async(wgpu::MapMode::Read, move |r| d2.store(if r.is_ok() { 1 } else { 2 }, std::sync::atomic::Ordering::Release));
        while done.load(std::sync::atomic::Ordering::Acquire) == 0 {
            if g.broken() || g.device.poll(wgpu::PollType::wait_indefinitely()).is_err() {
                return None;
            }
        }
        if done.load(std::sync::atomic::Ordering::Acquire) != 1 {
            return None;
        }
        let data = out.slice(..).get_mapped_range().ok()?;
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            px.extend_from_slice(&data[(y * row) as usize..][..(w * 4) as usize]);
        }
        Some(px)
    }
}

/// Dessine le volume dans la cible, à la taille donnée (en pixels). La caméra est celle de
/// `gpudraw` (sa profondeur extérieure arrête les rayons). Rend faux si rien n'a pu être dessiné.
pub fn paint(canvas: &mut Canvas, size: [u32; 2], cam: &crate::gpudraw::Cam, item: &Item) -> bool {
    let Some(g) = gpusim::gpu() else { return false };
    let cells = item.res[0] * item.res[1] * item.res[2];
    let Some(inv) = crate::volume::mat_inv(&cam.view_proj) else { return false };
    let size = [size[0].clamp(1, 8192), size[1].clamp(1, 8192)];
    let progs = programs(g);
    canvas.ensure(g, size);
    let Some(view) = canvas.view.clone() else { return false };
    // Les voxels : le tampon du solveur, ou ceux du processeur quand ils ont changé
    let buf = match &item.source {
        Source::Live(l) => l.sca.clone(),
        Source::Data(version, get) => {
            if canvas.own.as_ref().is_none_or(|o| o.1 != *version || o.0.size() != (cells * 8) as u64) {
                let data = get();
                if data.sca.len() != cells {
                    return false;
                }
                let b = match canvas.own.take() {
                    Some((b, _)) if b.size() == (cells * 8) as u64 => b,
                    _ => g.device.create_buffer(&wgpu::BufferDescriptor { label: Some("volume-voxels"), size: (cells * 8) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }),
                };
                g.queue.write_buffer(&b, 0, gpusim::bytes_of(&data.sca));
                canvas.own = Some((b, *version));
            }
            match &canvas.own {
                Some((b, _)) => b.clone(),
                None => return false,
            }
        }
    };
    if buf.size() < (cells * 8) as u64 {
        return false;
    }
    // Profondeur extérieure : renvoyée seulement quand elle change
    let mut soft_u = [0.0f32; 4];
    if let Some(s) = cam.soft.as_ref().filter(|s| s.w > 0 && s.h > 0) {
        if canvas.soft.as_ref().is_none_or(|c| c.2 != [s.w, s.h]) {
            let t = g.device.create_texture(&wgpu::TextureDescriptor { label: Some("volume-profondeur-exterieure"), size: wgpu::Extent3d { width: s.w, height: s.h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::R32Float, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[] });
            let v = t.create_view(&Default::default());
            if let Some(old) = canvas.soft.replace((t, v, [s.w, s.h], u64::MAX)) {
                old.0.destroy();
            }
        }
        if let Some(c) = canvas.soft.as_mut() {
            if c.3 != s.version {
                let data = (s.depth)();
                if data.len() == (s.w * s.h) as usize {
                    g.queue.write_texture(wgpu::TexelCopyTextureInfo { texture: &c.0, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All }, gpusim::bytes_of(&data), wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(s.w * 4), rows_per_image: Some(s.h) }, wgpu::Extent3d { width: s.w, height: s.h, depth_or_array_layers: 1 });
                    c.3 = s.version;
                }
            }
            if c.3 == s.version {
                soft_u = [s.w as f32, s.h as f32, 1.0, s.bias];
            }
        }
    }
    let soft_view = match (&canvas.soft, soft_u[2] > 0.5) {
        (Some(c), true) => c.1.clone(),
        _ => progs.blank.clone(),
    };
    let uniform = canvas.uniform.get_or_insert_with(|| g.uniform("volume-reglages", U_SIZE)).clone();
    let mut u = [0.0f32; (U_SIZE / 4) as usize];
    let mut put = |at: usize, v: &[f32]| u[at..at + v.len()].copy_from_slice(v);
    put(0, inv.as_flattened());
    put(16, item.to_grid.as_flattened());
    put(32, &cam.depth_row);
    put(36, &[cam.light[0], cam.light[1], cam.light[2], 0.0]);
    put(40, &[item.res[0] as f32, item.res[1] as f32, item.res[2] as f32, item.voxel]);
    put(44, &[item.look.density, item.look.glow, item.look.shadow, 0.0]);
    put(48, &[item.look.color[0], item.look.color[1], item.look.color[2], 0.0]);
    put(52, &soft_u);
    put(56, &[size[0] as f32, size[1] as f32, 0.0, 0.0]);
    g.queue.write_buffer(&uniform, 0, gpusim::bytes_of(&u));
    let group = g.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &progs.layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: buf.as_entire_binding() }, wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&soft_view) }],
    });
    let mut enc = g.device.create_command_encoder(&Default::default());
    {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("dessin-volume"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&progs.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
    g.queue.submit([enc.finish()]);
    !g.broken()
}

/// Dessine un volume de nodes : depuis les tampons du solveur s'ils montrent encore cet état,
/// sinon depuis ses voxels s'ils sont lisibles sans attendre. `model` : place de la géométrie
/// dans le monde. Rend (dessiné, sans relecture) ; rien si les voxels ne sont pas encore arrivés.
pub fn paint_volume(canvas: &mut Canvas, size: [u32; 2], cam: &crate::gpudraw::Cam, vol: &crate::volume::Volume, model: &Mat) -> Option<(bool, bool)> {
    let live = vol.live();
    if live.is_none() && !vol.ready() {
        return None;
    }
    let get = || vol.grids();
    let item = Item { source: match live { Some(l) => Source::Live(l), None => Source::Data(vol.version, &get) }, res: vol.res, voxel: vol.voxel, look: vol.look, to_grid: vol.to_grid(model)? };
    Some((paint(canvas, size, cam, &item), live.is_some()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volume::{pack, Volume};

    /// Une boule de fumée au milieu : la carte et le processeur dessinent la même image, et une
    /// surface placée devant la fumée la cache.
    #[test]
    fn la_carte_dessine_comme_le_processeur() {
        if gpusim::gpu().is_none() {
            return;
        }
        let n = 24usize;
        let mut g = Grids::zero(n * n * n);
        for (i, v) in g.sca.iter_mut().enumerate() {
            let (x, y, z) = ((i % n) as f32, ((i / n) % n) as f32, (i / (n * n)) as f32);
            let d = ((x - 11.5).powi(2) + (y - 11.5).powi(2) + (z - 11.5).powi(2)).sqrt();
            if d < 7.0 {
                *v = pack([0.6, 1.5, 0.0, if d < 3.0 { 1.0 } else { 0.0 }]);
            }
        }
        let vol = Volume::from_grids([n, n, n], [-0.5, -0.5, -0.5], 1.0 / n as f32, g);
        // Caméra orthographique sur l'axe z : x et y de −1 à 1, profondeur = 2 − z
        let depth_row = [0.0, 0.0, -1.0, 2.0];
        let vp = crate::gpudraw::from_rows([[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, -0.25, 0.5], [0.0, 0.0, 0.0, 1.0]]);
        let (w, h) = (64u32, 64u32);
        let mut canvas = Canvas::default();
        let cam = crate::gpudraw::Cam { view_proj: vp, depth_row, light: [0.0, 1.0, 0.0], eye: [0.0, 0.0, 2.0], soft: None };
        assert_eq!(paint_volume(&mut canvas, [w, h], &cam, &vol, &crate::gpudraw::IDENTITY), Some((true, false)));
        let px = canvas.pixels().unwrap();
        let at = |x: u32, y: u32| px[((y * w + x) * 4) as usize..][..4].to_vec();
        assert!(at(32, 32)[3] > 200 && at(32, 32)[0] > 150, "du feu au centre : {:?}", at(32, 32));
        assert_eq!(at(2, 2)[3], 0, "rien dans le coin");
        // Le processeur : la même formule
        let mut cpu = vec![0u8; (w * h * 4) as usize];
        let view = crate::volume::View { rays: &|x, y| ([(x as f32 + 0.5) / w as f32 * 2.0 - 1.0, 1.0 - (y as f32 + 0.5) / h as f32 * 2.0, 2.0], [0.0, 0.0, -1.0], f32::MAX), light: [0.0, 1.0, 0.0] };
        crate::volume::composite(&mut cpu, w as usize, h as usize, &[(Arc::new(vol.clone()), crate::gpudraw::IDENTITY)], &view);
        // Les deux décalent le départ des rayons au hasard, chacun à sa façon : un pixel peut
        // différer d'un pas de marche, la moyenne non
        let (mut worst, mut sum, mut count) = (0, 0, 0);
        for y in (4..60).step_by(2) {
            for x in (4..60).step_by(2) {
                let (a, b) = (at(x, y), &cpu[((y * w + x) * 4) as usize..][..4]);
                // Carte : couleur multipliée par l'opacité, sur fond transparent. Processeur : posée sur du noir.
                for k in 0..3 {
                    let d = (a[k] as i32 - b[k] as i32).abs();
                    worst = worst.max(d);
                    sum += d;
                    count += 1;
                }
            }
        }
        println!("carte contre processeur : écart moyen {:.1}, pire {worst}", sum as f32 / count as f32);
        assert!(worst < 90 && sum < count * 8, "écart moyen {}, pire {worst}", sum as f32 / count as f32);
        // Une surface à la profondeur 1,5 (devant la boule, qui commence vers 1,7) cache tout
        let wall = || vec![1.5f32; 16];
        let cam = crate::gpudraw::Cam { soft: Some(crate::gpudraw::Soft { w: 4, h: 4, version: 1, depth: &wall, bias: 0.0 }), ..cam };
        assert!(paint_volume(&mut canvas, [w, h], &cam, &vol, &crate::gpudraw::IDENTITY).is_some());
        let px = canvas.pixels().unwrap();
        assert_eq!(px[((32 * w + 32) * 4 + 3) as usize], 0);
    }
}
