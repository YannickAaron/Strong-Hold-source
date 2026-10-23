//! Start-up loading off the UI thread. `Atlases`: the map atlases' GPU upload (15 BC7 atlases + 14 masks, 1.3 GB)
//! on a worker thread, so the menu shows at once; `tables`: load_tables' parse of sprites.tsv cached as binary.
use std::sync::mpsc::{channel, Receiver};

/// Bytes per write_texture call: a memcpy into wgpu's staging memory, then the worker's own submit queues the GPU
/// copy, so a UI frame waits at most for one chunk's copy (not for 1.3 GB at its first submit).
const CHUNK: usize = 8 << 20;

/// The map atlases uploading on a worker thread (read, create_texture, write_texture in `CHUNK`s through the cloned
/// Device / Queue). Until an atlas is in, its slot in Gfx.groups is `placeholder` (transparent: nothing drawn).
pub struct Atlases { rx: Receiver<(usize, wgpu::BindGroup)>, pub placeholder: wgpu::BindGroup, done: usize, total: usize }

impl Atlases {
    /// `bgl` / `screen` / `sampler` / `black` (the no-mask texture) as init_gfx builds the sprite bind groups.
    pub fn start(device: &wgpu::Device, queue: &wgpu::Queue, bgl: &wgpu::BindGroupLayout, screen: &wgpu::Buffer, sampler: &wgpu::Sampler,
        black: &wgpu::TextureView, atlases: &[crate::Atlas]) -> Self {
        let clear = wgpu::util::DeviceExt::create_texture_with_data(device, queue, &wgpu::TextureDescriptor {
            label: Some("not loaded"), size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        }, Default::default(), &[0; 4]).create_view(&Default::default());
        let placeholder = group(device, bgl, screen, sampler, &clear, black);
        // masks.tsv: atlas index, w, h, unity format (10 = DXT1, 12 = DXT5). Missing file = old export, no masks.
        let masks: Vec<(usize, u32, u32, u32)> = std::fs::read_to_string(crate::cache_dir().join("masks.tsv")).unwrap_or_default().lines()
            .map(|l| { let c: Vec<u32> = l.split('\t').map(|v| v.parse().unwrap()).collect(); (c[0] as usize, c[1], c[2], c[3]) }).collect();
        // atlases.tsv column 4, unity format: 25 = BC7 (DE's), 12 = DXT5 / BC3 (extract::sc2's renders)
        let formats: Vec<u32> = std::fs::read_to_string(crate::cache_dir().join("atlases.tsv")).unwrap_or_default().lines()
            .map(|l| l.split('\t').nth(4).and_then(|v| v.parse().ok()).unwrap_or(25)).collect();
        let jobs: Vec<(String, u32, u32, u32)> = atlases.iter().enumerate().map(|(i, a)| (a.name.clone(), a.w, a.h, formats.get(i).copied().unwrap_or(25))).collect();
        let (tx, rx) = channel();
        let dev = (device.clone(), queue.clone());
        let (bgl, screen, sampler, black, total) = (bgl.clone(), screen.clone(), sampler.clone(), black.clone(), jobs.len());
        std::thread::spawn(move || {
            let t0 = std::time::Instant::now();
            for (i, (name, w, h, format)) in jobs.into_iter().enumerate() {
                let mask = masks.iter().find(|m| m.0 == i).map(|&(_, mw, mh, f)| {
                    let (f, b) = if f == 12 { (wgpu::TextureFormat::Bc3RgbaUnorm, 16) } else { (wgpu::TextureFormat::Bc1RgbaUnorm, 8) };
                    upload(&dev, &format!("{name}_m"), mw, mh, f, b)
                });
                let f = if format == 12 { wgpu::TextureFormat::Bc3RgbaUnormSrgb } else { wgpu::TextureFormat::Bc7RgbaUnormSrgb };
                let tex = upload(&dev, &name, w, h, f, 16);
                if tx.send((i, group(&dev.0, &bgl, &screen, &sampler, &tex, mask.as_ref().unwrap_or(&black)))).is_err() { return; }
            }
            println!("uploaded {total} atlases + {} masks in {:.2}s (worker thread)", masks.len(), t0.elapsed().as_secs_f32());
        });
        Atlases { rx, placeholder, done: 0, total }
    }

    /// Puts the atlases that finished since the last call into `groups`. Once per frame.
    pub fn take(&mut self, groups: &mut [wgpu::BindGroup]) {
        for (i, g) in self.rx.try_iter() { groups[i] = g; self.done += 1; }
    }
    /// 0..1 while uploading, None when all are in.
    pub fn progress(&self) -> Option<f32> { (self.done < self.total).then(|| self.done as f32 / self.total as f32) }
}

/// A sprite bind group as init_gfx makes them: screen uniform, atlas, sampler, team-colour mask.
fn group(d: &wgpu::Device, bgl: &wgpu::BindGroupLayout, screen: &wgpu::Buffer, sampler: &wgpu::Sampler, tex: &wgpu::TextureView, mask: &wgpu::TextureView) -> wgpu::BindGroup {
    d.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None, layout: bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: screen.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(tex) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(mask) },
        ],
    })
}

/// Raw BC blocks from the export cache → texture, in `CHUNK`-sized bands of block rows. Unity row order = wgpu v
/// order (README).
fn upload(dq: &(wgpu::Device, wgpu::Queue), name: &str, w: u32, h: u32, format: wgpu::TextureFormat, block: u32) -> wgpu::TextureView {
    let (device, queue) = dq;
    let data = std::fs::read(crate::cache_dir().join(format!("{name}.bc"))).unwrap();
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(name), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
    });
    let row = (w / 4 * block) as usize; // bytes per row of 4×4 blocks
    let band = (CHUNK / row).max(1) as u32;
    let mut y = 0;
    while y < h / 4 {
        let n = band.min(h / 4 - y);
        let off = y as usize * row;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: y * 4, z: 0 }, aspect: wgpu::TextureAspect::All },
            &data[off..off + n as usize * row],
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row as u32), rows_per_image: Some(n) },
            wgpu::Extent3d { width: w, height: n * 4, depth_or_array_layers: 1 },
        );
        y += n;
        queue.submit([]);
        std::thread::sleep(std::time::Duration::from_millis(2)); // ponytail: fixed pacing (~4 GB/s ceiling); tune if the menu stutters
    }
    tex.create_view(&Default::default())
}

/// `load_tables`' result from `tables.bin` next to the export (written by `save`), when it was made from this
/// sprites.tsv / atlases.tsv (their length and hash). Text parsing of the 20 MB sprites.tsv: 0.18 s; this: ~0.03 s.
pub mod tables {
    use crate::{Atlas, Sprite};
    use std::hash::{Hash, Hasher};

    fn key(dir: &std::path::Path) -> Option<u64> {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for f in ["atlases.tsv", "sprites.tsv"] { std::fs::read(dir.join(f)).ok()?.hash(&mut h); }
        Some(h.finish())
    }

    pub fn load(dir: &std::path::Path) -> Option<(Vec<Atlas>, Vec<(String, Sprite)>)> {
        let b = std::fs::read(dir.join("tables.bin")).ok()?;
        let mut r = Rd(&b, 0);
        if r.u64()? != key(dir)? { return None; }
        let atlases = (0..r.u32()?).map(|_| Some(Atlas { name: r.str()?, w: r.u32()?, h: r.u32()? })).collect::<Option<Vec<_>>>()?;
        let sprites = (0..r.u32()?).map(|_| {
            let name = r.str()?;
            let (atlas, [x, y, w, h, px, py]) = (r.u32()?, [r.f32()?, r.f32()?, r.f32()?, r.f32()?, r.f32()?, r.f32()?]);
            let mesh = (0..r.u32()?).map(|_| Some([r.f32()?, r.f32()?])).collect::<Option<Vec<_>>>()?;
            Some((name, Sprite { atlas, x, y, w, h, px, py, mesh }))
        }).collect::<Option<Vec<_>>>()?;
        Some((atlases, sprites))
    }

    /// Writes the cache; a read-only export (the .app's bundled copy) just keeps parsing.
    pub fn save(dir: &std::path::Path, (atlases, sprites): &(Vec<Atlas>, Vec<(String, Sprite)>)) {
        let Some(k) = key(dir) else { return };
        let mut o = k.to_le_bytes().to_vec();
        let s = |o: &mut Vec<u8>, s: &str| { o.extend((s.len() as u32).to_le_bytes()); o.extend(s.as_bytes()); };
        o.extend((atlases.len() as u32).to_le_bytes());
        for a in atlases { s(&mut o, &a.name); o.extend(a.w.to_le_bytes()); o.extend(a.h.to_le_bytes()); }
        o.extend((sprites.len() as u32).to_le_bytes());
        for (n, p) in sprites {
            s(&mut o, n);
            o.extend(p.atlas.to_le_bytes());
            for v in [p.x, p.y, p.w, p.h, p.px, p.py] { o.extend(v.to_le_bytes()); }
            o.extend((p.mesh.len() as u32).to_le_bytes());
            for v in p.mesh.iter().flatten() { o.extend(v.to_le_bytes()); }
        }
        let tmp = dir.join("tables.bin.tmp");
        if std::fs::write(&tmp, &o).is_ok() { let _ = std::fs::rename(tmp, dir.join("tables.bin")); }
    }

    struct Rd<'a>(&'a [u8], usize);
    impl Rd<'_> {
        fn take<const N: usize>(&mut self) -> Option<[u8; N]> { let v = self.0.get(self.1..self.1 + N)?.try_into().ok()?; self.1 += N; Some(v) }
        fn u32(&mut self) -> Option<u32> { self.take().map(u32::from_le_bytes) }
        fn u64(&mut self) -> Option<u64> { self.take().map(u64::from_le_bytes) }
        fn f32(&mut self) -> Option<f32> { self.take().map(f32::from_le_bytes) }
        fn str(&mut self) -> Option<String> {
            let n = self.u32()? as usize;
            let s = String::from_utf8(self.0.get(self.1..self.1 + n)?.to_vec()).ok()?;
            self.1 += n;
            Some(s)
        }
    }
}

/// A game start whose sim setup runs on a worker thread (`App::load_in_background`), and its result once in.
#[derive(Default)]
pub struct Loading {
    #[cfg(feature = "live")]
    job: Option<(crate::menu::Go, Receiver<Result<shc_sim::mem::Mem, String>>, std::time::Instant)>,
    #[cfg(feature = "live")]
    ready: Option<Result<shc_sim::mem::Mem, String>>,
}

/// A .sav into a fresh sim state (DLL_LoadSaveGame, `world::load_save`): `Live::load` without the panic.
#[cfg(feature = "live")]
pub fn load_save(path: &std::path::Path) -> Result<shc_sim::mem::Mem, String> {
    let file = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut m = shc_sim::mem::Mem::default();
    shc_sim::world::load_save(&mut m, &file).map_err(|e| format!("{}: {e:?}", path.display()))?;
    Ok(m)
}

#[cfg(feature = "live")]
impl crate::App {
    /// menu_go's game starts (Start / Load / Mission / Brief at a new difficulty / FreeBuild / Scenario / Trail): their sim
    /// setup (0.3-0.75 s) goes to a worker thread and the menu stays live with "Loading..."; `loading_tick` runs the
    /// same `go` again once it is in, and the branch's `loaded` call takes the result. None = started (or another
    /// start is still loading); Some = run `go` now.
    pub(crate) fn load_in_background(&mut self, go: crate::menu::Go) -> Option<crate::menu::Go> {
        use crate::{menu::Go, skirmish};
        type Job = Box<dyn FnOnce() -> Result<shc_sim::mem::Mem, String> + Send>;
        if self.loading.ready.is_some() { return Some(go); }
        if self.loading.job.is_some() { return if matches!(go, Go::Start(_) | Go::Load(_) | Go::Mission(_) | Go::Brief(_) | Go::FreeBuild(..) | Go::Scenario(..) | Go::Trail(..)) { None } else { Some(go) }; }
        let job: Job = match &go {
            Go::Start(l) => { let l = l.clone(); Box::new(move || skirmish::start(&l)) }
            Go::Load(f) => { let f = f.clone(); Box::new(move || load_save(&f)) }
            Go::Mission(id) => { let id = *id; Box::new(move || skirmish::start_mission(id, 1)) }
            Go::Brief(d) => match self.campaign {
                Some((id, d0, _)) if *d != d0 => { let d = *d; Box::new(move || skirmish::start_mission(id, d)) }
                _ => return Some(go),
            },
            Go::FreeBuild(map, fb) => { let (map, fb) = (map.clone(), *fb); Box::new(move || skirmish::start_solo(&map, &fb, None)) }
            Go::Scenario(map, d) => { let (map, d) = (map.clone(), *d); Box::new(move || skirmish::start_solo(&map, &Default::default(), Some(d))) }
            Go::Trail(t, id, d) => { let (t, id, d) = (*t, *id, *d); Box::new(move || crate::menu::trail_menu::start(t, id, d)) }
            _ => return Some(go),
        };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = job();
            crate::audio::forward(shc_sim::sound::drain()); // anything the setup queued plays on the UI thread
            let _ = tx.send(r);
        });
        self.loading.job = Some((go, rx, std::time::Instant::now()));
        None
    }

    /// The sim setup of the game start menu_go is running: the worker's result when it was loaded in the background,
    /// else `f` right here.
    pub(crate) fn loaded(&mut self, f: impl FnOnce() -> Result<shc_sim::mem::Mem, String>) -> Result<shc_sim::mem::Mem, String> {
        self.loading.ready.take().unwrap_or_else(f)
    }
}

impl crate::App {
    /// Once per frame: finished map atlases into the bind groups; a background game start that is in continues.
    pub(crate) fn loading_tick(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        if let Some(g) = &mut self.gfx {
            if let Some(l) = &mut g.loading { l.take(&mut g.groups); }
            if g.loading.as_ref().is_some_and(|l| l.progress().is_none()) { g.loading = None; }
        }
        #[cfg(feature = "live")]
        if let Some(r) = self.loading.job.as_ref().and_then(|j| j.1.try_recv().ok()) {
            let (go, _, t0) = self.loading.job.take().unwrap();
            println!("game setup on the worker thread: {:.2}s", t0.elapsed().as_secs_f32());
            self.loading.ready = Some(r);
            self.menu_go(go, el);
            self.loading.ready = None; // a branch that didn't load (error path) must not leave it for the next start
        }
        let _ = el;
    }
}

/// "Loading..." while a game start loads, "Loading graphics N %" while map atlases upload (bottom left).
pub fn overlay(l: &Loading, g: &crate::Gfx, ui: Option<&mut crate::hud::Ui>, mut hud: Vec<crate::hud::HudTri>) -> Vec<crate::hud::HudTri> {
    let Some(ui) = ui else { return hud };
    #[cfg(feature = "live")]
    let busy = l.job.is_some();
    #[cfg(not(feature = "live"))]
    let busy = { let _ = l; false };
    let gfx = g.loading.as_ref().and_then(|a| a.progress());
    if !busy && gfx.is_none() { return hud; }
    let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
    let s = crate::ux::hud_scale(sz.width, sz.height); // the HUD's scale (Ui::draw drops glyphs when ui.s changes)
    (ui.s, ui.w, ui.h) = (s, sz.width, sz.height);
    let text = match gfx { Some(p) if busy => format!("Loading...   graphics {:.0} %", p * 100.0), Some(p) => format!("Loading graphics {:.0} %", p * 100.0), None => "Loading...".into() };
    let (x, y) = (24.0 * s, sz.height - 24.0 * s);
    ui.panel(&mut hud, [x - 10.0 * s, y - 40.0 * s, x + 420.0 * s, y + 16.0 * s], s);
    ui.text_sh(&mut hud, &text, x, y, 26.0, 0.0, [0xef as f32 / 255.0, 0xf3 as f32 / 255.0, 0xc6 as f32 / 255.0, 1.0]);
    hud
}
