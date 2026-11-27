//! Dessin d'un maillage directement sur la carte graphique.
//!
//! Sert à montrer l'état d'une simulation **sans le relire** : le tampon de positions du solveur
//! est le tampon de sommets (`Source::Live`). Le même dessin sait aussi partir de positions venues
//! du processeur (`Source::Data`), pour qu'une image reprise du cache ait le même aspect.
//!
//! - **Normales sur la carte.** Un noyau de calcul fait, pour chaque sommet, la somme des normales
//!   de ses triangles (pondérées par leur aire). Il relit le tampon de positions, rien d'autre.
//! - **Ombrage simple.** Une lumière attachée à la vue, un peu de ciel, les deux faces éclairées.
//! - **Profondeur.** Le maillage a son tampon de profondeur. Pour se mêler à une image calculée
//!   ailleurs (la vue 3D de la scène est un rasteriseur logiciel), on lui donne la profondeur de
//!   cette image (`Soft`) : un pixel du maillage caché par elle n'est pas dessiné.
//! - **Sortie.** Une texture (`Canvas`) que l'interface affiche comme une image. Elle reste sur
//!   la carte de bout en bout. Sans interface (banc, tests), on peut la relire avec `pixels`.
//!
//! Tout passe par la carte de `gpusim`. Quand l'interface dessine avec wgpu, c'est la sienne.

use crate::gpucloth::Live;
use crate::gpusim::{self, Bind, Gpu, Kernels};
use eframe::egui;
use std::sync::{Mutex, OnceLock};

/// D'où viennent les positions d'un morceau
pub enum Source<'a> {
    /// Les tampons d'un tissu, tels que le solveur vient de les écrire
    Live(&'a Live),
    /// Des positions du processeur : leur version (elles ne sont renvoyées que si elle change),
    /// et de quoi les obtenir, dans l'ordre de la géométrie
    Data(u64, &'a dyn Fn() -> Vec<[f32; 4]>),
}

/// Un morceau de géométrie à dessiner
pub struct Part<'a> {
    /// Nombre de sommets
    pub n: usize,
    pub source: Source<'a>,
    /// Clé de la topologie, et ses triangles dans l'ordre de la géométrie (demandés une fois par clé)
    pub tris: (u64, &'a dyn Fn() -> Vec<[u32; 3]>),
    /// Couleur par sommet (linéaire, 8 bits), avec sa version ; sinon `base`
    pub colors: Option<(u64, &'a dyn Fn() -> Vec<[u8; 4]>)>,
    /// Couleur de base, linéaire
    pub base: [f32; 3],
    /// Place du morceau dans le monde (matrice 4×4, colonnes d'abord)
    pub model: [[f32; 4]; 4],
}

/// Profondeur d'une image calculée ailleurs, pour s'y mêler : une valeur par pixel, la même
/// grandeur que `Cam::depth_row` appliquée à un point du monde, plus petit = plus près.
pub struct Soft<'a> {
    pub w: u32,
    pub h: u32,
    pub version: u64,
    pub depth: &'a dyn Fn() -> Vec<f32>,
    /// Tolérance, dans l'unité de la profondeur
    pub bias: f32,
}

/// La caméra, en matrices (colonnes d'abord, profondeur de 0 à 1)
pub struct Cam<'a> {
    pub view_proj: [[f32; 4]; 4],
    /// Profondeur d'un point du monde `p` : `depth_row · (p, 1)`
    pub depth_row: [f32; 4],
    /// Direction vers la lumière, dans le monde
    pub light: [f32; 3],
    /// Position de l'œil, dans le monde
    pub eye: [f32; 3],
    pub soft: Option<Soft<'a>>,
}

pub const IDENTITY: [[f32; 4]; 4] = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

/// Matrice à partir de ses quatre lignes
pub fn from_rows(r: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut m = [[0.0; 4]; 4];
    for (i, row) in r.iter().enumerate() {
        for (j, x) in row.iter().enumerate() {
            m[j][i] = *x;
        }
    }
    m
}

const NORMALS: &str = r#"
struct NU { count: u32, p0: u32, p1: u32, p2: u32 }
@group(0) @binding(0) var<uniform> s: NU;
@group(0) @binding(1) var<storage, read> pos: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> off: array<u32>;
@group(0) @binding(3) var<storage, read> adj: array<vec2<u32>>;
@group(0) @binding(4) var<storage, read_write> nrm: array<vec4<f32>>;

@compute @workgroup_size(WG)
fn normals(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= s.count) { return; }
    let p = pos[i].xyz;
    var n = vec3<f32>(0.0);
    let end = off[i + 1u];
    for (var e = off[i]; e < end; e++) {
        let ab = adj[e];
        n += cross(pos[ab.x].xyz - p, pos[ab.y].xyz - p);
    }
    let l = length(n);
    var o = vec3<f32>(0.0, 1.0, 0.0);
    if (l > 1e-30) { o = n / l; }
    nrm[i] = vec4<f32>(o, 0.0);
}
"#;

const SHADER: &str = r#"
struct U {
    vp: mat4x4<f32>,
    model: mat4x4<f32>,
    origin: vec4<f32>,
    depth_row: vec4<f32>,
    light: vec4<f32>,
    eye: vec4<f32>,
    base: vec4<f32>,
    soft: vec4<f32>,
    size: vec4<f32>,
}
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var soft: texture_2d<f32>;

struct VO {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) vd: f32,
}

@vertex
fn vs(@location(0) p: vec3<f32>, @location(1) n: vec3<f32>, @location(2) c: vec4<f32>) -> VO {
    var o: VO;
    let w = u.model * vec4<f32>(p + u.origin.xyz, 1.0);
    o.clip = u.vp * w;
    o.world = w.xyz;
    o.nrm = (u.model * vec4<f32>(n, 0.0)).xyz;
    o.color = mix(u.base.rgb, c.rgb, u.base.a);
    o.vd = dot(u.depth_row, w);
    return o;
}

@fragment
fn fs(i: VO, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    // Caché par l'image calculée ailleurs : rien à dessiner ici
    if (u.soft.z > 0.5) {
        let q = vec2<i32>(i.clip.xy / u.size.xy * u.soft.xy);
        let d = textureLoad(soft, clamp(q, vec2<i32>(0), vec2<i32>(u.soft.xy) - vec2<i32>(1)), 0).r;
        if (i.vd > d + u.soft.w) { discard; }
    }
    var n = normalize(i.nrm);
    let v = normalize(u.eye.xyz - i.world);
    // Les deux faces sont éclairées : la normale regarde toujours l'œil
    if (dot(n, v) < 0.0) { n = -n; }
    let l = normalize(u.light.xyz);
    let diffuse = max(dot(n, l), 0.0);
    let sky = 0.5 + 0.5 * n.y;
    let h = normalize(l + v);
    let shine = pow(max(dot(n, h), 0.0), 24.0) * 0.08;
    // Le dos d'un tissu est un peu plus sombre : on lit les plis
    var back = 1.0;
    if (!front) { back = 0.82; }
    let lin = i.color * (0.22 + 0.16 * sky + 0.72 * diffuse) * back + vec3<f32>(shine);
    // L'interface attend des couleurs déjà en gamma
    let g = pow(clamp(lin, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / 2.2));
    return vec4<f32>(g, 1.0);
}
"#;

/// Octets des réglages du dessin (`U`)
const U_SIZE: u64 = 64 * 2 + 16 * 7;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

struct Programs {
    normals: Kernels,
    layout: wgpu::BindGroupLayout,
    /// Un pipeline par nombre d'échantillons (1 ou 4)
    pipelines: [wgpu::RenderPipeline; 2],
    /// Texture d'un pixel, branchée quand il n'y a pas de profondeur extérieure
    blank: wgpu::TextureView,
    /// Lissage ×4 permis par la carte ?
    msaa: bool,
}

fn programs(g: &Gpu) -> &'static Programs {
    static P: OnceLock<Programs> = OnceLock::new();
    P.get_or_init(|| {
        use Bind::{Read, Uniform, Write};
        let d = &g.device;
        let normals = g.kernels("dessin-normales", NORMALS, &[Uniform, Read, Read, Read, Write], &["normals"]);
        let module = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("dessin-maillage"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dessin-maillage"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            ],
        });
        let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("dessin-maillage"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = |count: u32| {
            d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("dessin-maillage"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    buffers: &[
                        // Positions et normales : des `vec4`, dont on lit les trois premiers nombres
                        Some(wgpu::VertexBufferLayout { array_stride: 16, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x3] }),
                        Some(wgpu::VertexBufferLayout { array_stride: 16, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![1 => Float32x3] }),
                        Some(wgpu::VertexBufferLayout { array_stride: 4, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![2 => Unorm8x4] }),
                    ],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(true), depth_compare: Some(wgpu::CompareFunction::Less), stencil: Default::default(), bias: Default::default() }),
                multisample: wgpu::MultisampleState { count, mask: !0, alpha_to_coverage_enabled: false },
                fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some("fs"), targets: &[Some(wgpu::ColorTargetState { format: FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
                multiview_mask: None,
                cache: None,
            })
        };
        let blank = d.create_texture(&wgpu::TextureDescriptor { label: Some("dessin-sans-profondeur"), size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::R32Float, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[] });
        // Le lissage ×4 est garanti par WebGPU, pas par tous les pilotes OpenGL : on l'a demandé à la carte
        let msaa = g.msaa4;
        Programs { normals, layout, pipelines: [pipeline(1), pipeline(4)], blank: blank.create_view(&Default::default()), msaa }
    })
}

/// Ce qu'un morceau garde sur la carte d'une image à l'autre
struct Mesh {
    /// Clé de la topologie, et numéro du tissu (0 : positions venues du processeur)
    key: (u64, u64),
    n: usize,
    index: wgpu::Buffer,
    index_count: u32,
    normals: wgpu::Buffer,
    colors: wgpu::Buffer,
    colors_for: Option<u64>,
    /// Positions envoyées par le processeur (`Source::Data`), et leur version
    own_pos: Option<(wgpu::Buffer, u64)>,
    off: wgpu::Buffer,
    adj: wgpu::Buffer,
    count: wgpu::Buffer,
    uniform: wgpu::Buffer,
    /// Branchement du noyau des normales, et le tampon de positions pour lequel il est fait
    group: Option<(wgpu::BindGroup, wgpu::Buffer)>,
    used: u64,
}

static MESHES: Mutex<Vec<Mesh>> = Mutex::new(Vec::new());
static CLOCK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Mesh {
    /// Tampons d'une topologie. `tris` : dans l'ordre de la géométrie ; `inv` : rang de chaque
    /// sommet dans le tampon de positions (un tissu range ses sommets autrement).
    fn new(g: &Gpu, key: (u64, u64), n: usize, tris: &[[u32; 3]], inv: Option<&[u32]>) -> Option<Mesh> {
        let rank = |v: u32| inv.map_or(v, |inv| inv.get(v as usize).copied().unwrap_or(0));
        let tris: Vec<[u32; 3]> = tris.iter().filter(|t| t.iter().all(|v| (*v as usize) < n)).map(|t| [rank(t[0]), rank(t[1]), rank(t[2])]).collect();
        if n == 0 || tris.is_empty() {
            return None;
        }
        // Voisinage : pour chaque sommet, les deux autres sommets de chacun de ses triangles
        let mut off = vec![0u32; n + 1];
        for t in &tris {
            for v in t {
                off[*v as usize + 1] += 1;
            }
        }
        for i in 0..n {
            off[i + 1] += off[i];
        }
        let mut at = off.clone();
        let mut adj = vec![[0u32; 2]; tris.len() * 3];
        for t in &tris {
            for k in 0..3 {
                let v = t[k] as usize;
                adj[at[v] as usize] = [t[(k + 1) % 3], t[(k + 2) % 3]];
                at[v] += 1;
            }
        }
        let limit = g.max_binding.min(g.max_buffer);
        if (adj.len() * 8) as u64 > limit || (n * 16) as u64 > limit {
            return None;
        }
        let d = &g.device;
        let make = |label: &str, bytes: &[u8], usage: wgpu::BufferUsages| {
            let b = d.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: (bytes.len() as u64).next_multiple_of(4).max(16), usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            if !bytes.is_empty() {
                g.queue.write_buffer(&b, 0, bytes);
            }
            b
        };
        let index = make("dessin-triangles", gpusim::bytes_of(&tris), wgpu::BufferUsages::INDEX);
        let off_b = make("dessin-voisinage-debuts", gpusim::bytes_of(&off), wgpu::BufferUsages::STORAGE);
        let adj_b = make("dessin-voisinage", gpusim::bytes_of(&adj), wgpu::BufferUsages::STORAGE);
        let normals = d.create_buffer(&wgpu::BufferDescriptor { label: Some("dessin-normales"), size: (n * 16) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX, mapped_at_creation: false });
        let colors = make("dessin-couleurs", &vec![255u8; n * 4], wgpu::BufferUsages::VERTEX);
        let count = g.uniform("dessin-nombre", 16);
        g.queue.write_buffer(&count, 0, gpusim::bytes_of(&[n as u32, 0, 0, 0]));
        let uniform = g.uniform("dessin-reglages", U_SIZE);
        Some(Mesh { key, n, index, index_count: (tris.len() * 3) as u32, normals, colors, colors_for: None, own_pos: None, off: off_b, adj: adj_b, count, uniform, group: None, used: 0 })
    }
}

/// La cible du dessin d'une vue : sa texture de couleur, sa profondeur, et l'image que l'interface affiche.
pub struct Canvas {
    size: [u32; 2],
    samples: u32,
    color: Option<wgpu::Texture>,
    view: Option<wgpu::TextureView>,
    msaa: Option<wgpu::TextureView>,
    depth: Option<wgpu::TextureView>,
    id: Option<egui::TextureId>,
    /// Profondeur extérieure : texture, taille, version
    soft: Option<(wgpu::Texture, wgpu::TextureView, [u32; 2], u64)>,
}

impl Default for Canvas {
    fn default() -> Self {
        Canvas { size: [0, 0], samples: 1, color: None, view: None, msaa: None, depth: None, id: None, soft: None }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        if let (Some(id), Some(ui)) = (self.id.take(), crate::gfx::ui()) {
            ui.renderer.write().free_texture(&id);
        }
    }
}

impl Canvas {
    fn ensure(&mut self, g: &Gpu, size: [u32; 2], samples: u32) {
        if self.size == size && self.samples == samples && self.color.is_some() {
            return;
        }
        let d = &g.device;
        let extent = wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 };
        let tex = |label: &str, format, count: u32, usage| d.create_texture(&wgpu::TextureDescriptor { label: Some(label), size: extent, mip_level_count: 1, sample_count: count, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[] });
        let color = tex("dessin-couleur", FORMAT, 1, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC);
        let view = color.create_view(&Default::default());
        self.msaa = (samples > 1).then(|| tex("dessin-couleur-lissee", FORMAT, samples, wgpu::TextureUsages::RENDER_ATTACHMENT).create_view(&Default::default()));
        self.depth = Some(tex("dessin-profondeur", DEPTH, samples, wgpu::TextureUsages::RENDER_ATTACHMENT).create_view(&Default::default()));
        // L'image montrée par l'interface suit la nouvelle texture
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
        self.samples = samples;
    }

    /// L'image à montrer dans l'interface (rien tant que rien n'a été dessiné, ou sans interface sur cette carte)
    pub fn texture(&self) -> Option<egui::TextureId> {
        self.id
    }

    pub fn size(&self) -> [u32; 2] {
        self.size
    }

    /// Relit l'image dessinée (RGBA, 8 bits, déjà en gamma). Pour les tests et le banc : l'interface
    /// n'en a jamais besoin.
    pub fn pixels(&self) -> Option<Vec<u8>> {
        let (g, color) = (gpusim::gpu()?, self.color.as_ref()?);
        let [w, h] = self.size;
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let out = g.device.create_buffer(&wgpu::BufferDescriptor { label: Some("dessin-relecture"), size: (row * h) as u64, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mut enc = g.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &out, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        g.queue.submit([enc.finish()]);
        let done = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
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

/// Peut-on dessiner sur la carte ? (Une carte de calcul est ouverte, et elle n'a pas rendu d'erreur.)
pub fn available() -> bool {
    gpusim::gpu().is_some()
}

/// Peut-on montrer ce dessin dans l'interface sans relecture ? Il faut que l'interface et les
/// simulations soient sur la même carte (affichage wgpu, voir `gfx`).
pub fn shared() -> bool {
    gpusim::gpu().is_some_and(|g| g.shared)
}

/// Dessine les morceaux dans la cible, à la taille donnée (en pixels). Rend faux si rien n'a pu
/// être dessiné (pas de carte, maillage trop gros pour elle) : l'appelant dessine alors autrement.
pub fn paint(canvas: &mut Canvas, size: [u32; 2], cam: &Cam, parts: &[Part]) -> bool {
    let Some(g) = gpusim::gpu() else { return false };
    let size = [size[0].clamp(1, 8192), size[1].clamp(1, 8192)];
    let progs = programs(g);
    let samples = if progs.msaa { 4 } else { 1 };
    canvas.ensure(g, size, samples);
    let (Some(view), Some(depth)) = (canvas.view.clone(), canvas.depth.clone()) else { return false };
    // Profondeur extérieure : renvoyée seulement quand elle change
    let mut soft_u = [0.0f32; 4];
    if let Some(s) = cam.soft.as_ref().filter(|s| s.w > 0 && s.h > 0) {
        if canvas.soft.as_ref().is_none_or(|c| c.2 != [s.w, s.h]) {
            let t = g.device.create_texture(&wgpu::TextureDescriptor { label: Some("dessin-profondeur-exterieure"), size: wgpu::Extent3d { width: s.w, height: s.h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::R32Float, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[] });
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
    let clock = CLOCK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut meshes = MESHES.lock().unwrap_or_else(|e| e.into_inner());
    let mut enc = g.device.create_command_encoder(&Default::default());
    // Ce qu'on va dessiner : (rang du maillage, tampon de positions, branchement des réglages)
    let mut draws: Vec<(usize, wgpu::Buffer, wgpu::BindGroup)> = vec![];
    for p in parts {
        let key = (p.tris.0, match &p.source { Source::Live(l) => l.cloth, Source::Data(..) => 0 });
        let at = match meshes.iter().position(|m| m.key == key && m.n == p.n) {
            Some(k) => k,
            None => {
                let tris = (p.tris.1)();
                let inv = match &p.source { Source::Live(l) => Some(&l.inv[..]), Source::Data(..) => None };
                let Some(m) = Mesh::new(g, key, p.n, &tris, inv) else { continue };
                // Quelques maillages restent prêts ; le plus ancien cède sa place et sa mémoire
                if meshes.len() >= 6 {
                    if let Some(old) = (0..meshes.len()).min_by_key(|k| meshes[*k].used) {
                        meshes.swap_remove(old);
                    }
                }
                meshes.push(m);
                meshes.len() - 1
            }
        };
        let m = &mut meshes[at];
        m.used = clock;
        // Positions : le tampon du solveur tel quel, ou celles du processeur quand elles ont changé
        let (pos, origin) = match &p.source {
            Source::Live(l) => (l.pos.clone(), l.origin),
            Source::Data(version, get) => {
                if m.own_pos.as_ref().is_none_or(|o| o.1 != *version) {
                    let data = get();
                    if data.len() != p.n {
                        continue;
                    }
                    let buf = match m.own_pos.take() {
                        Some((b, _)) => b,
                        None => g.device.create_buffer(&wgpu::BufferDescriptor { label: Some("dessin-positions"), size: (p.n * 16) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }),
                    };
                    g.queue.write_buffer(&buf, 0, gpusim::bytes_of(&data));
                    m.own_pos = Some((buf, *version));
                }
                let Some((b, _)) = &m.own_pos else { continue };
                (b.clone(), [0.0; 3])
            }
        };
        if pos.size() < (p.n * 16) as u64 {
            continue;
        }
        // Couleurs par sommet, dans l'ordre du tampon de positions
        if let Some((version, get)) = &p.colors {
            if m.colors_for != Some(*version) {
                let c = get();
                if c.len() == p.n {
                    let sorted: Vec<[u8; 4]> = match &p.source {
                        Source::Live(l) => l.perm.iter().map(|i| c.get(*i as usize).copied().unwrap_or([255; 4])).collect(),
                        Source::Data(..) => c,
                    };
                    g.queue.write_buffer(&m.colors, 0, gpusim::bytes_of(&sorted));
                }
                m.colors_for = Some(*version);
            }
        }
        // Normales : un lancement du noyau, sur le tampon de positions
        if m.group.as_ref().is_none_or(|x| x.1 != pos) {
            m.group = Some((g.group(&progs.normals, &[&m.count, &pos, &m.off, &m.adj, &m.normals]), pos.clone()));
        }
        if let Some((group, _)) = &m.group {
            let mut pass = enc.begin_compute_pass(&Default::default());
            g.launch(&mut pass, &progs.normals.pipelines[0], group, &[], p.n as u64);
        }
        let mut u = [0.0f32; (U_SIZE / 4) as usize];
        let mut put = |at: usize, v: &[f32]| u[at..at + v.len()].copy_from_slice(v);
        put(0, cam.view_proj.as_flattened());
        put(16, p.model.as_flattened());
        put(32, &[origin[0], origin[1], origin[2], 0.0]);
        put(36, &cam.depth_row);
        put(40, &[cam.light[0], cam.light[1], cam.light[2], 0.0]);
        put(44, &[cam.eye[0], cam.eye[1], cam.eye[2], 0.0]);
        put(48, &[p.base[0], p.base[1], p.base[2], if p.colors.is_some() { 1.0 } else { 0.0 }]);
        put(52, &soft_u);
        put(56, &[size[0] as f32, size[1] as f32, 0.0, 0.0]);
        g.queue.write_buffer(&m.uniform, 0, gpusim::bytes_of(&u));
        let group = g.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &progs.layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: m.uniform.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&soft_view) }] });
        draws.push((at, pos, group));
    }
    {
        let (target, resolve) = match &canvas.msaa {
            Some(m) => (m, Some(&view)),
            None => (&view, None),
        };
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("dessin-maillage"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target, resolve_target: resolve, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }), stencil_ops: None }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&progs.pipelines[if samples > 1 { 1 } else { 0 }]);
        for (at, pos, group) in &draws {
            let m = &meshes[*at];
            pass.set_bind_group(0, group, &[]);
            pass.set_vertex_buffer(0, pos.slice(..));
            pass.set_vertex_buffer(1, m.normals.slice(..));
            pass.set_vertex_buffer(2, m.colors.slice(..));
            pass.set_index_buffer(m.index.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..m.index_count, 0, 0..1);
        }
    }
    g.queue.submit([enc.finish()]);
    !draws.is_empty() && !g.broken()
}

/// Oublie les maillages gardés sur la carte (leur mémoire est rendue).
pub fn clear() {
    MESHES.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un tissu simulé sur la carte, dessiné depuis les tampons du solveur : l'image montre un
    /// tissu éclairé, et aucune donnée n'est revenue au processeur.
    #[test]
    fn tissu_dessine_sans_relecture() {
        if !available() {
            return;
        }
        let n = 24;
        let (pos, edges) = crate::gpucloth::grid(n, n, 1.0, 1.0);
        let rest = crate::gpucloth::rest_lengths(&edges, &pos);
        let pinned: Vec<bool> = (0..n * n).map(|i| i < n).collect();
        let topo = crate::gpucloth::Topo::build(n * n, &edges, &rest, &pos, &pinned, None);
        let mut cloth = crate::gpucloth::Cloth::new(topo, &pos, true);
        if !cloth.on_gpu() {
            return;
        }
        let par = crate::gpucloth::Params::default();
        for _ in 0..3 {
            cloth.push_uniform([0.0, -9.81 / 24.0, 0.0], 1.0);
            cloth.step(1.0 / 24.0, &par);
        }
        let shot = cloth.shot();
        let live = shot.live.as_ref().expect("tissu sur la carte");
        assert!(live.current());
        // Triangles de la grille, dans l'ordre de la géométrie
        let tris = || {
            let mut t = vec![];
            for z in 0..n - 1 {
                for x in 0..n - 1 {
                    let i = (z * n + x) as u32;
                    t.push([i, i + 1, i + n as u32 + 1]);
                    t.push([i, i + n as u32 + 1, i + n as u32]);
                }
            }
            t
        };
        // Vue de dessus : l'œil à 3 m au-dessus du centre, regard vers le bas
        let (near, far) = (0.1f32, 20.0f32);
        let f = far / (far - near);
        let depth = [0.0, -1.0, 0.0, 3.0];
        let vp = from_rows([[1.2, 0.0, 0.0, 0.0], [0.0, 0.0, -1.2, 0.0], [0.0, -f, 0.0, (3.0 - near) * f], depth]);
        let cam = Cam { view_proj: vp, depth_row: depth, light: [0.2, 1.0, 0.3], eye: [0.0, 3.0, 0.0], soft: None };
        let part = Part { n: n * n, source: Source::Live(live), tris: (0xBEEF, &tris), colors: None, base: [0.2, 0.6, 0.9], model: IDENTITY };
        let mut canvas = Canvas::default();
        assert!(paint(&mut canvas, [96, 96], &cam, &[part]));
        let px = canvas.pixels().expect("image relue");
        let covered = px.chunks(4).filter(|p| p[3] == 255).count();
        let lit = px.chunks(4).filter(|p| p[3] == 255 && p[2] > 60 && p[2] > p[0]).count();
        // Le tissu (1 m de côté vu à 3 m, focale 1,2) couvre quelques centaines de pixels au milieu
        assert!(covered > 300 && covered < 1500, "{covered} pixels couverts");
        assert_eq!(px[(48 * 96 + 48) * 4 + 3], 255, "le milieu de l'image est sur le tissu");
        assert_eq!(px[3], 0, "le coin de l'image est vide");
        assert!(lit * 10 > covered * 9, "{lit} pixels éclairés sur {covered}");
        // Rien n'a été relu pour dessiner
        assert!(!shot.resolved());
        // Le tissu avance : ce cliché ne décrit plus les tampons de la carte
        cloth.step(1.0 / 24.0, &par);
        assert!(!live.current());
    }

    /// Un carré face à la caméra : il couvre le milieu de l'image, pas les coins ; et une
    /// profondeur extérieure plus proche que lui le cache.
    #[test]
    fn un_carre_dessine_et_cache() {
        if !available() {
            return;
        }
        let quad = || vec![[-0.5f32, -0.5, 0.0, 0.0], [0.5, -0.5, 0.0, 0.0], [0.5, 0.5, 0.0, 0.0], [-0.5, 0.5, 0.0, 0.0]];
        let tris = || vec![[0u32, 1, 2], [0, 2, 3]];
        // Caméra en z = 2, qui regarde vers −z : profondeur = 2 − z
        let (near, far) = (0.1f32, 10.0f32);
        let vp = from_rows([[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, -far / (far - near), far * (2.0 - near) / (far - near)], [0.0, 0.0, -1.0, 2.0]]);
        let part = |version: u64| Part { n: 4, source: Source::Data(version, &quad), tris: (0xC0FFEE, &tris), colors: None, base: [0.8, 0.2, 0.2], model: IDENTITY };
        let cam = |soft| Cam { view_proj: vp, depth_row: [0.0, 0.0, -1.0, 2.0], light: [0.3, 0.5, 1.0], eye: [0.0, 0.0, 2.0], soft };
        let mut canvas = Canvas::default();
        assert!(paint(&mut canvas, [64, 64], &cam(None), &[part(1)]));
        let px = canvas.pixels().expect("image relue");
        let at = |x: usize, y: usize| [px[(y * 64 + x) * 4], px[(y * 64 + x) * 4 + 1], px[(y * 64 + x) * 4 + 2], px[(y * 64 + x) * 4 + 3]];
        // Le carré (demi-côté 0,5 à la distance 2) couvre le quart central : rouge au milieu, vide au coin
        assert!(at(32, 32)[3] == 255 && at(32, 32)[0] > at(32, 32)[1] + 40, "milieu {:?}", at(32, 32));
        assert_eq!(at(2, 2)[3], 0, "coin {:?}", at(2, 2));
        // Une image extérieure à la profondeur 1 partout (le carré est à 2) : il est caché
        let near_all = || vec![1.0f32; 16 * 16];
        let hidden = Soft { w: 16, h: 16, version: 1, depth: &near_all, bias: 0.01 };
        assert!(paint(&mut canvas, [64, 64], &cam(Some(hidden)), &[part(1)]));
        assert_eq!(canvas.pixels().unwrap()[(32 * 64 + 32) * 4 + 3], 0);
        // À la profondeur 3 partout : il passe devant
        let far_all = || vec![3.0f32; 16 * 16];
        let shown = Soft { w: 16, h: 16, version: 2, depth: &far_all, bias: 0.01 };
        assert!(paint(&mut canvas, [64, 64], &cam(Some(shown)), &[part(1)]));
        assert_eq!(canvas.pixels().unwrap()[(32 * 64 + 32) * 4 + 3], 255);
    }
}
