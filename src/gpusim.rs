//! Socle de simulation sur carte graphique.
//!
//! Tout ce que les solveurs partagent, et rien d'autre :
//!
//! - **Une seule carte pour toutes les simulations.** `gpu()` ouvre la carte à la première
//!   demande, avec les limites réelles de la machine (taille des tampons, nombre de tampons par
//!   noyau), et la garde. L'interface dessine en OpenGL de son côté : les deux ne se gênent pas.
//! - **Tampons en colonnes.** Un attribut = un tampon (`Column`). Un noyau ne lit que les colonnes
//!   dont il a besoin : c'est la mémoire qui borne la vitesse, donc moins d'octets = plus vite.
//!   Une colonne plus grosse que ce que la carte accepte de brancher d'un coup est coupée en pages.
//! - **Lancement en deux dimensions.** Une carte refuse plus de 65 535 groupes par dimension :
//!   `grid` répartit sur deux, et `PRELUDE` donne au noyau de quoi retrouver son numéro d'élément.
//! - **Relecture sans attendre.** `read` rend tout de suite un `Readback` : on demande `ready()`
//!   à chaque image de l'interface, ou `wait()` quand on veut le résultat maintenant.
//! - **Repli.** Pas de carte, carte trop limitée ou erreur du pilote : `gpu()` rend `None` et
//!   chaque solveur passe par sa version processeur, qui sert aussi de référence dans les tests.
//!
//! Règle de la maison : chaque noyau a sa version processeur, et un test qui compare les deux.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

/// Taille d'un groupe de fils pour les noyaux à une dimension
pub const WG: u32 = 64;

/// À coller en tête d'un noyau : `lin(gid, nwg)` donne le numéro d'élément, que le lancement soit
/// sur une ou deux dimensions. Le noyau doit vérifier lui-même que ce numéro est dans les bornes.
pub const PRELUDE: &str = "const WG: u32 = 64u;\nfn lin(gid: vec3<u32>, nwg: vec3<u32>) -> u32 { return gid.x + gid.y * nwg.x * WG; }\n";

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    /// Nom lisible : modèle, moteur (Vulkan, Metal…)
    pub name: String,
    /// Rendu logiciel : le code marche, mais plus lentement que le processeur
    pub soft: bool,
    /// Plus gros tampon qu'un noyau peut brancher, en octets
    pub max_binding: u64,
    pub max_buffer: u64,
    /// Groupes permis par dimension de lancement
    pub max_groups: u32,
    /// Tampons de stockage permis par noyau
    pub max_storage: u32,
    broken: Arc<AtomicBool>,
}

static GPU: OnceLock<Option<Gpu>> = OnceLock::new();

/// La carte des simulations, ouverte à la première demande. `None` : pas de carte utilisable, ou
/// elle a rendu une erreur depuis ; les solveurs passent alors par le processeur.
pub fn gpu() -> Option<&'static Gpu> {
    GPU.get_or_init(|| std::panic::catch_unwind(open).ok().flatten()).as_ref().filter(|g| !g.broken.load(Ordering::Relaxed))
}

/// Une vraie carte (pas un rendu logiciel) est-elle disponible ?
pub fn hardware() -> bool {
    gpu().is_some_and(|g| !g.soft)
}

fn open() -> Option<Gpu> {
    // Pour essayer le repli sur le processeur sans débrancher la carte
    if std::env::var_os("ARY_SANS_GPU").is_some() {
        return None;
    }
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })).ok()?;
    let info = adapter.get_info();
    // On demande tout ce que la carte permet : les valeurs par défaut de WebGPU (128 Mo par
    // branchement, 8 tampons par noyau) plafonneraient vers 600 000 sommets
    let limits = adapter.limits();
    if limits.max_storage_buffers_per_shader_stage < 8 || limits.max_compute_workgroup_size_x < WG {
        return None;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { label: Some("aryadne-simulation"), required_limits: limits.clone(), ..Default::default() })).ok()?;
    let broken = Arc::new(AtomicBool::new(false));
    // Une erreur de la carte ne doit pas fermer le logiciel : on la note, et tout repasse au processeur
    let b = broken.clone();
    device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
        eprintln!("simulation, carte graphique : {e}");
        b.store(true, Ordering::Relaxed);
    }));
    let b = broken.clone();
    device.set_device_lost_callback(move |_, _| b.store(true, Ordering::Relaxed));
    Some(Gpu {
        device,
        queue,
        name: format!("{} ({:?})", info.name, info.backend),
        soft: info.device_type == wgpu::DeviceType::Cpu,
        max_binding: limits.max_storage_buffer_binding_size,
        max_buffer: limits.max_buffer_size,
        max_groups: limits.max_compute_workgroups_per_dimension,
        max_storage: limits.max_storage_buffers_per_shader_stage,
        broken,
    })
}

/// Vue en octets d'un tableau de nombres simples (f32, u32 et leurs petits tableaux : pas de remplissage).
pub fn bytes_of<T: Copy>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

/// L'inverse : des octets relus vers un tableau de nombres simples.
pub fn from_bytes<T: Copy + Default>(b: &[u8]) -> Vec<T> {
    let n = b.len() / std::mem::size_of::<T>();
    let mut v = vec![T::default(); n];
    unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), v.as_mut_ptr() as *mut u8, n * std::mem::size_of::<T>()) };
    v
}

/// Nombre de groupes à lancer pour `count` éléments : sur une dimension tant que ça tient, sur
/// deux au-delà. `None` si même deux dimensions ne suffisent pas (plus de 2⁴⁰ éléments, en pratique jamais).
pub fn grid(count: u64, max_groups: u32) -> Option<[u32; 2]> {
    let groups = count.div_ceil(WG as u64).max(1);
    let max = max_groups.max(1) as u64;
    if groups <= max {
        return Some([groups as u32, 1]);
    }
    let rows = groups.div_ceil(max);
    (rows <= max).then_some([max as u32, rows as u32])
}

/// Une colonne d'attribut sur la carte : un tampon, ou plusieurs pages si elle dépasse ce qu'un
/// noyau peut brancher d'un coup. Les noyaux « élément par élément » se lancent page par page ;
/// ceux qui vont lire leurs voisins n'importe où (tissu) exigent une seule page.
pub struct Column {
    pub pages: Vec<wgpu::Buffer>,
    /// Nombre d'éléments
    pub len: usize,
    /// Octets par élément
    pub stride: usize,
    /// Éléments par page (la dernière peut en avoir moins)
    pub page_len: usize,
}

impl Column {
    /// Nombre d'éléments de la page `k`
    pub fn page_elems(&self, k: usize) -> usize {
        self.len.saturating_sub(k * self.page_len).min(self.page_len)
    }

    /// Le tampon, si la colonne tient en une page
    pub fn single(&self) -> Option<&wgpu::Buffer> {
        (self.pages.len() == 1).then(|| &self.pages[0])
    }

    pub fn bytes(&self) -> u64 {
        (self.len * self.stride) as u64
    }
}

/// Ce qu'un noyau branche à chaque place
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Bind {
    /// Réglages, les mêmes pour tout le lancement
    Uniform,
    /// Réglages rangés par cases dans un même tampon : chaque lancement choisit sa case (taille d'une case en octets)
    Slot(u64),
    /// Colonne lue
    Read,
    /// Colonne lue et écrite
    Write,
}

/// Un jeu de noyaux écrits dans le même module, qui partagent leurs branchements.
pub struct Kernels {
    pub layout: wgpu::BindGroupLayout,
    pub pipelines: Vec<wgpu::ComputePipeline>,
    binds: Vec<Bind>,
}

impl Gpu {
    /// La carte a-t-elle rendu une erreur depuis son ouverture ?
    pub fn broken(&self) -> bool {
        self.broken.load(Ordering::Relaxed)
    }

    /// Éléments par page pour une colonne de ce pas, selon les limites de la carte
    pub fn page_len(&self, stride: usize) -> usize {
        ((self.max_binding.min(self.max_buffer) / stride as u64) as usize).max(1)
    }

    pub fn column(&self, label: &str, len: usize, stride: usize) -> Column {
        self.column_paged(label, len, stride, self.page_len(stride))
    }

    /// Colonne coupée en pages de `page_len` éléments (les essais forcent de petites pages)
    pub fn column_paged(&self, label: &str, len: usize, stride: usize, page_len: usize) -> Column {
        let page_len = page_len.max(1);
        let count = len.div_ceil(page_len).max(1);
        let mut c = Column { pages: Vec::with_capacity(count), len, stride, page_len };
        for k in 0..count {
            let size = ((c.page_elems(k) * stride) as u64).next_multiple_of(4).max(16);
            c.pages.push(self.device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }));
        }
        c
    }

    /// Envoie des données dans une colonne (elles doivent avoir sa taille)
    pub fn write<T: Copy>(&self, c: &Column, data: &[T]) {
        let b = bytes_of(data);
        debug_assert_eq!(b.len(), c.len * c.stride);
        for (k, page) in c.pages.iter().enumerate() {
            let (from, to) = (k * c.page_len * c.stride, ((k * c.page_len + c.page_elems(k)) * c.stride).min(b.len()));
            if from < to {
                self.queue.write_buffer(page, 0, &b[from..to]);
            }
        }
    }

    /// Petit tampon de réglages
    pub fn uniform(&self, label: &str, bytes: u64) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: bytes.next_multiple_of(16).max(16), usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
    }

    /// Taille d'une case de réglages : arrondie à l'alignement que la carte exige
    pub fn slot_size(&self, bytes: u64) -> u64 {
        bytes.next_multiple_of(self.device.limits().min_uniform_buffer_offset_alignment.max(16) as u64)
    }

    /// Compile un module et ses noyaux. Les branchements suivent l'ordre de `binds` (place 0, 1, 2…).
    pub fn kernels(&self, label: &str, wgsl: &str, binds: &[Bind], entries: &[&str]) -> Kernels {
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(format!("{PRELUDE}{wgsl}").into()) });
        let entry = |(i, b): (usize, &Bind)| {
            let (ty, dynamic, size) = match b {
                Bind::Uniform => (wgpu::BufferBindingType::Uniform, false, None),
                Bind::Slot(s) => (wgpu::BufferBindingType::Uniform, true, std::num::NonZeroU64::new(*s)),
                Bind::Read => (wgpu::BufferBindingType::Storage { read_only: true }, false, None),
                Bind::Write => (wgpu::BufferBindingType::Storage { read_only: false }, false, None),
            };
            wgpu::BindGroupLayoutEntry { binding: i as u32, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: dynamic, min_binding_size: size }, count: None }
        };
        let layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(label), entries: &binds.iter().enumerate().map(entry).collect::<Vec<_>>() });
        let pl = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipelines = entries.iter().map(|e| self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(e), layout: Some(&pl), module: &module, entry_point: Some(e), compilation_options: Default::default(), cache: None })).collect();
        Kernels { layout, pipelines, binds: binds.to_vec() }
    }

    /// Branche des tampons sur un jeu de noyaux, dans l'ordre des places
    pub fn group(&self, k: &Kernels, bufs: &[&wgpu::Buffer]) -> wgpu::BindGroup {
        let entries: Vec<wgpu::BindGroupEntry> = bufs
            .iter()
            .zip(&k.binds)
            .enumerate()
            .map(|(i, (buf, b))| {
                let size = if let Bind::Slot(s) = b { std::num::NonZeroU64::new(*s) } else { None };
                wgpu::BindGroupEntry { binding: i as u32, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: buf, offset: 0, size }) }
            })
            .collect();
        self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &k.layout, entries: &entries })
    }

    /// Lance un noyau sur `count` éléments, dans une passe déjà ouverte. `slots` : la case de
    /// réglages choisie pour chaque branchement `Bind::Slot`, en octets.
    pub fn launch(&self, pass: &mut wgpu::ComputePass, pipeline: &wgpu::ComputePipeline, group: &wgpu::BindGroup, slots: &[u32], count: u64) {
        self.launch_capped(pass, pipeline, group, slots, count, self.max_groups);
    }

    /// Même chose avec un plafond de groupes imposé (les essais forcent le lancement sur deux dimensions)
    pub fn launch_capped(&self, pass: &mut wgpu::ComputePass, pipeline: &wgpu::ComputePipeline, group: &wgpu::BindGroup, slots: &[u32], count: u64, max_groups: u32) {
        let Some([gx, gy]) = grid(count, max_groups) else { return };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, slots);
        pass.dispatch_workgroups(gx, gy, 1);
    }

    /// Demande la relecture d'une colonne. Rend la main tout de suite : la copie se fera quand la
    /// carte aura fini le travail déjà envoyé.
    pub fn read(&self, c: &Column) -> Readback {
        let left = Arc::new(AtomicUsize::new(c.pages.len()));
        let failed = Arc::new(AtomicBool::new(false));
        let mut enc = self.device.create_command_encoder(&Default::default());
        let mut bufs = Vec::with_capacity(c.pages.len());
        for (k, page) in c.pages.iter().enumerate() {
            let bytes = c.page_elems(k) * c.stride;
            let size = (bytes as u64).next_multiple_of(4).max(16);
            let out = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("relecture"), size, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            enc.copy_buffer_to_buffer(page, 0, &out, 0, size);
            bufs.push((out, bytes));
        }
        self.queue.submit([enc.finish()]);
        for (out, _) in &bufs {
            let (left, failed) = (left.clone(), failed.clone());
            out.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                if r.is_err() {
                    failed.store(true, Ordering::Relaxed);
                }
                left.fetch_sub(1, Ordering::Release);
            });
        }
        Readback { device: self.device.clone(), bufs, left, failed, broken: self.broken.clone() }
    }

    /// Attend que la carte ait fini tout ce qui lui a été envoyé
    pub fn finish(&self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }
}

/// Une relecture en cours.
pub struct Readback {
    device: wgpu::Device,
    bufs: Vec<(wgpu::Buffer, usize)>,
    left: Arc<AtomicUsize>,
    failed: Arc<AtomicBool>,
    broken: Arc<AtomicBool>,
}

impl Readback {
    /// Les données sont-elles arrivées ? Ne bloque pas : à appeler à chaque image de l'interface.
    pub fn ready(&self) -> bool {
        let _ = self.device.poll(wgpu::PollType::Poll);
        self.left.load(Ordering::Acquire) == 0 || self.broken.load(Ordering::Relaxed)
    }

    /// Les octets relus, en attendant la carte s'il le faut.
    pub fn wait(self) -> Result<Vec<u8>, String> {
        while self.left.load(Ordering::Acquire) > 0 {
            if self.broken.load(Ordering::Relaxed) || self.device.poll(wgpu::PollType::wait_indefinitely()).is_err() {
                return Err("la carte graphique a renvoyé une erreur".into());
            }
        }
        if self.failed.load(Ordering::Relaxed) || self.broken.load(Ordering::Relaxed) {
            return Err("la carte graphique a renvoyé une erreur".into());
        }
        let mut out = Vec::with_capacity(self.bufs.iter().map(|b| b.1).sum());
        for (buf, bytes) in &self.bufs {
            match buf.slice(..).get_mapped_range() {
                Ok(d) => out.extend_from_slice(&d[..*bytes]),
                Err(_) => return Err("la carte graphique a renvoyé une erreur".into()),
            }
            buf.unmap();
        }
        Ok(out)
    }

    /// Les données relues, dans leur type
    pub fn take<T: Copy + Default>(self) -> Result<Vec<T>, String> {
        self.wait().map(|b| from_bytes(&b))
    }
}

// ---------- noyau d'essai du socle ----------

/// y ← a·x + y, version processeur : la référence du noyau d'essai ci-dessous.
pub fn saxpy_cpu(a: f32, x: &[f32], y: &mut [f32]) {
    for (y, x) in y.iter_mut().zip(x) {
        *y = a * *x + *y;
    }
}

const SAXPY: &str = r#"
struct P { a: f32, n: u32, p0: u32, p1: u32 }
@group(0) @binding(0) var<uniform> p: P;
@group(0) @binding(1) var<storage, read> x: array<f32>;
@group(0) @binding(2) var<storage, read_write> y: array<f32>;
@compute @workgroup_size(WG)
fn saxpy(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= p.n) { return; }
    y[i] = p.a * x[i] + y[i];
}
"#;

/// y ← a·x + y sur la carte, page par page. Sert d'essai au socle : pages, lancement sur deux
/// dimensions, relecture. `page_len` et `max_groups` permettent de forcer ces cas sur de petites données.
pub fn saxpy_gpu(g: &Gpu, a: f32, x: &[f32], y: &[f32], page_len: usize, max_groups: u32) -> Readback {
    let k = g.kernels("saxpy", SAXPY, &[Bind::Uniform, Bind::Read, Bind::Write], &["saxpy"]);
    let (cx, cy) = (g.column_paged("x", x.len(), 4, page_len), g.column_paged("y", y.len(), 4, page_len));
    g.write(&cx, x);
    g.write(&cy, y);
    let mut enc = g.device.create_command_encoder(&Default::default());
    // Les réglages et les branchements doivent vivre jusqu'à l'envoi
    let mut keep = vec![];
    for page in 0..cx.pages.len() {
        let n = cx.page_elems(page) as u32;
        let u = g.uniform("saxpy", 16);
        g.queue.write_buffer(&u, 0, bytes_of(&[a.to_bits(), n, 0, 0]));
        let group = g.group(&k, &[&u, &cx.pages[page], &cy.pages[page]]);
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            g.launch_capped(&mut pass, &k.pipelines[0], &group, &[], n as u64, max_groups);
        }
        keep.push((u, group));
    }
    g.queue.submit([enc.finish()]);
    g.read(&cy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grille_de_lancement() {
        assert_eq!(grid(1, 65535), Some([1, 1]));
        assert_eq!(grid(64, 65535), Some([1, 1]));
        assert_eq!(grid(65, 65535), Some([2, 1]));
        assert_eq!(grid(65535 * 64, 65535), Some([65535, 1]));
        // Au-delà de 4,2 millions d'éléments : deux dimensions, et assez de fils pour tout couvrir
        let [gx, gy] = grid(20_000_000, 65535).unwrap();
        assert!(gy > 1 && gx as u64 * gy as u64 * 64 >= 20_000_000);
        assert_eq!(grid(1000, 4), Some([4, 4]));
        assert_eq!(grid(u64::MAX, 4), None);
    }

    #[test]
    fn octets_aller_retour() {
        let v = [[1.0f32, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]];
        let back: Vec<[f32; 4]> = from_bytes(bytes_of(&v));
        assert_eq!(back, v);
    }

    /// Le socle entier sur un petit cas : pages forcées (5 pages), lancement forcé sur deux
    /// dimensions (3 groupes par rangée), relecture sans attendre, comparaison au processeur.
    #[test]
    fn saxpy_carte_contre_processeur() {
        let Some(g) = gpu() else { return };
        let n = 2000;
        let x: Vec<f32> = (0..n).map(|i| (i as f32 * 0.37).sin()).collect();
        let y: Vec<f32> = (0..n).map(|i| (i as f32 * 0.11).cos()).collect();
        let mut want = y.clone();
        saxpy_cpu(2.5, &x, &mut want);
        for (page, groups) in [(n, g.max_groups), (450, 3)] {
            let rb = saxpy_gpu(g, 2.5, &x, &y, page, groups);
            // Relecture asynchrone : on tourne sans bloquer jusqu'à ce qu'elle soit prête
            let t0 = std::time::Instant::now();
            while !rb.ready() && t0.elapsed().as_secs() < 20 {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert!(rb.ready(), "relecture jamais prête");
            let got: Vec<f32> = rb.take().unwrap();
            assert_eq!(got.len(), n);
            let worst = got.iter().zip(&want).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
            assert!(worst < 1e-6, "écart {worst} (pages de {page}, {groups} groupes)");
        }
        assert!(!g.broken());
    }
}
