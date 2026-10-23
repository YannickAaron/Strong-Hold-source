//! Load-time and game-speed measurements. They print, and assert only what wall time can't break: the machine runs
//! many builds in parallel, which slows everything several times. `cpu` = this thread's CPU time.

fn cpu() -> f64 {
    #[cfg(unix)]
    {
        extern "C" { fn clock_gettime(clock: i32, t: *mut [i64; 2]) -> i32; }
        let mut t = [0i64; 2];
        // CLOCK_THREAD_CPUTIME_ID: 16 on macOS, 3 on Linux
        unsafe { clock_gettime(if cfg!(target_os = "macos") { 16 } else { 3 }, &mut t) };
        t[0] as f64 + t[1] as f64 * 1e-9
    }
    #[cfg(not(unix))]
    0.0 // ponytail: no thread CPU clock on Windows; CPU columns print 0
}

/// Prints wall / CPU time since the last call.
struct Lap(std::time::Instant, f64);
impl Lap {
    fn new() -> Self { Lap(std::time::Instant::now(), cpu()) }
    fn lap(&mut self, what: &str) -> f64 {
        let (w, c) = (self.0.elapsed().as_secs_f64(), cpu() - self.1);
        println!("{what:34} wall {w:6.3}s  cpu {c:6.3}s");
        *self = Self::new();
        c
    }
}

fn save() -> Option<String> {
    let p = crate::paths::test_save("Crater Lake-1test.sav");
    std::path::Path::new(&p).exists().then_some(p)
}

/// Where a game start spends its time: the sim setup per start path, then the first full draw list.
#[cfg(feature = "live")]
#[test]
fn start_paths() {
    let map = crate::skirmish::maps_dir().join("Crater Lake.map");
    let Some(sav) = save().filter(|_| map.exists()) else { return };
    let mut t = Lap::new();
    let m = crate::skirmish::start(&crate::skirmish::Lobby::new(map)).unwrap();
    t.lap("skirmish::start (Crater Lake)");
    let mut l = crate::live::Live::from_mem(m);
    t.lap("Live::from_mem");
    let n = l.records().len();
    t.lap(&format!("first full draw list ({n} rec)"));
    drop(crate::live::Live::load(&sav));
    t.lap("Live::load (Crater Lake-1test.sav)");
    let ok = crate::skirmish::start_mission(1, 1).is_ok();
    t.lap(&format!("start_mission(1) ok {ok}"));
}

/// sprites.tsv parse vs the binary cache (loader::tables): same tables, the cache several times faster.
#[test]
fn tables_cache() {
    let dir = crate::cache_dir();
    if !dir.join("sprites.tsv").exists() { return; }
    let _ = std::fs::remove_file(dir.join("tables.bin")); // parse once without the cache
    let mut t = Lap::new();
    let a = crate::export_tables();
    let parse = t.lap(&format!("load_tables, no cache ({} sprites)", a.1.len()));
    let b = crate::loader::tables::load(&dir).expect("load_tables wrote no cache");
    let cached = t.lap("loader::tables::load");
    assert_eq!((a.0.len(), a.1.len()), (b.0.len(), b.1.len()));
    for ((na, sa), (nb, sb)) in a.1.iter().zip(&b.1) {
        assert!(na == nb && sa.atlas == sb.atlas && [sa.x, sa.y, sa.w, sa.h, sa.px, sa.py] == [sb.x, sb.y, sb.w, sb.h, sb.px, sb.py] && sa.mesh == sb.mesh, "{na}");
    }
    println!("tables: parse {parse:.3}s cpu -> cache {cached:.3}s cpu");
    assert!(cached < parse, "cache slower than parsing");
}

/// The map atlases' upload, headless: init_gfx's old way (all on the UI thread, then the first submit copies 1.3 GB)
/// vs loader::Atlases (worker thread, the UI thread only submits frames). Prints the UI thread's longest frame.
#[test]
fn atlas_upload() {
    let dir = crate::cache_dir();
    if !dir.join("atlases.tsv").exists() { return; }
    let inst = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let Ok(ad) = pollster::block_on(inst.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })) else { return };
    let (dev, q) = pollster::block_on(ad.request_device(&wgpu::DeviceDescriptor { required_features: wgpu::Features::TEXTURE_COMPRESSION_BC, required_limits: ad.limits(), ..Default::default() })).unwrap();
    let (atlases, _) = crate::export_tables(); // the files in the export (BUNDLED ones are not)
    // old: read + one write_texture per atlas on this thread, then the first frame's submit waits for the copy
    let mut t = Lap::new();
    let mut texs = vec![];
    for a in &atlases {
        let d = std::fs::read(dir.join(format!("{}.bc", a.name))).unwrap();
        let tex = dev.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: a.w, height: a.h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Bc7RgbaUnormSrgb, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[] });
        q.write_texture(wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All }, &d,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(a.w / 4 * 16), rows_per_image: Some(a.h / 4) }, tex.size());
        texs.push(tex);
    }
    let w0 = std::time::Instant::now();
    t.lap("old: read + write_texture (UI thread)");
    q.submit([]);
    dev.poll(wgpu::PollType::Wait).unwrap();
    let first = w0.elapsed().as_secs_f64();
    t.lap("old: first submit until copied");
    drop(texs);
    // new: the worker uploads; this thread submits and waits like a frame loop
    let bgl = dev.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[
        wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
        wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
        wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
        wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
    ] });
    let screen = dev.create_buffer(&wgpu::BufferDescriptor { label: None, size: 32, usage: wgpu::BufferUsages::UNIFORM, mapped_at_creation: false });
    let sampler = dev.create_sampler(&Default::default());
    let black = dev.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
        dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] }).create_view(&Default::default());
    let mut t = Lap::new();
    let w0 = std::time::Instant::now();
    let mut l = crate::loader::Atlases::start(&dev, &q, &bgl, &screen, &sampler, &black, &atlases);
    let mut groups = vec![l.placeholder.clone(); atlases.len()];
    let (mut worst, mut frames) = (0f64, 0);
    while l.progress().is_some() {
        let f = std::time::Instant::now();
        l.take(&mut groups);
        q.submit([]);
        dev.poll(wgpu::PollType::Wait).unwrap();
        worst = worst.max(f.elapsed().as_secs_f64());
        frames += 1;
        std::thread::sleep(std::time::Duration::from_millis(8)); // the rest of a 120 Hz frame
    }
    let all = w0.elapsed().as_secs_f64();
    t.lap("new: UI thread while the worker uploads");
    println!("atlas upload: old {first:.3}s first frame (after the UI thread's own read + write); new: all in after {all:.3}s, {frames} frames, longest {:.1} ms", worst * 1000.0);
}

/// Ticks per second the sim thread reaches at each single-player speed step, with the window camera and zoomed
/// out (PerfectPixelWithZoom's 0.25 × native on a 1728×1117 Retina window), plus the frames the UI gets.
#[cfg(feature = "live")]
#[test]
fn speed_steps() {
    let Some(sav) = save() else { return };
    use crate::live::{Cmd, Live, Thread};
    let mut steps = vec![crate::live::SPEED_DEFAULT, 90.0];
    loop { let (s, n) = (*steps.last().unwrap(), crate::live::speed_step(*steps.last().unwrap(), true)); if n == s { break; } steps.push(n); }
    steps.push(f32::INFINITY); // Tab fast-forward
    println!("speed steps: {steps:?}");
    let k = |x: f32, y: f32, w: f32, h: f32| [x - w / 2.0, y - h / 2.0, x + w / 2.0, y + h / 2.0];
    let (cx, cy) = (-29158.0, -1608.0); // around the keep, as the --live camera starts (live::live_runs_crater_lake)
    for (cam, rect) in [("window", k(cx, cy, 1684.0, 784.0)), ("zoomed out", k(cx, cy, 1728.0 * 2.0 / 0.25, 1117.0 * 2.0 / 0.25))] {
        for &rate in &steps {
            let mut l = Live::load(&sav);
            l.records();
            let mut th = Thread::spawn(l, true, rate);
            th.send(Cmd::View([400, 400, 30, 30], rect));
            let (t0, w0) = (th.tick, std::time::Instant::now());
            let mut frames = 0;
            while w0.elapsed().as_secs_f32() < 2.0 { frames += th.frames().len(); std::thread::sleep(std::time::Duration::from_millis(8)); }
            let secs = w0.elapsed().as_secs_f32();
            println!("{cam:10} speed {rate:>7}: {:6.0} ticks/s, {:4.0} frames/s to the UI", (th.tick - t0) as f32 / secs, frames as f32 / secs);
        }
    }
}

/// The sim thread's CPU cost per tick (load-independent, unlike `speed_steps`' wall time): the step (sim + the draw
/// pass, which must run every tick: it writes unit / building draw fields and the first-sighting latch), and what a
/// Frame to the UI adds (HudState; the radar every 8th Frame). Capacity = ticks/s one free core reaches: one Frame
/// per tick (≤ 90 ticks/s, and before) vs one Frame per 8 ms (above 90).
#[cfg(feature = "live")]
#[test]
fn tick_cost() {
    let Some(sav) = save() else { return };
    let k = |x: f32, y: f32, w: f32, h: f32| [x - w / 2.0, y - h / 2.0, x + w / 2.0, y + h / 2.0];
    let (cx, cy) = (-29158.0, -1608.0);
    for (cam, rect) in [("window", k(cx, cy, 1684.0, 784.0)), ("zoomed out", k(cx, cy, 1728.0 * 2.0 / 0.25, 1117.0 * 2.0 / 0.25))] {
        let mut l = crate::live::Live::load(&sav);
        l.records();
        l.view = Some(crate::live::camera(rect));
        for _ in 0..20 { l.step(); }
        let n = 200;
        let c = cpu();
        for _ in 0..n { l.step(); }
        let step = (cpu() - c) / n as f64;
        let c = cpu();
        for _ in 0..n { l.hud(); }
        let hud = (cpu() - c) / n as f64;
        let c = cpu();
        for _ in 0..10 { l.radar([400, 400, 30, 30]); }
        let radar = (cpu() - c) / 10.0;
        let frame = hud + radar / 8.0;
        println!("{cam:10}: step {:.3} ms, HudState {:.3} ms, radar {:.2} ms -> per Frame {:.3} ms", step * 1e3, hud * 1e3, radar * 1e3, frame * 1e3);
        let (per_tick, merged) = (1.0 / (step + frame), (1.0 - 125.0 * frame) / step); // merged: 125 Frames/s at most
        println!("{cam:10}: capacity one Frame per tick {per_tick:.0} ticks/s; one Frame per 8 ms {:.0} ticks/s", per_tick.max(merged));
    }
}

/// SHC_PERF_LOOP=1: 60 s of zoomed-out ticks, for a profiler (`sample <pid>`).
#[cfg(feature = "live")]
#[test]
fn zoomed_out_loop() {
    let Some(sav) = save().filter(|_| std::env::var_os("SHC_PERF_LOOP").is_some()) else { return };
    let mut l = crate::live::Live::load(&sav);
    l.records();
    let (cx, cy, w, h) = (-29158.0, -1608.0, 1728.0 * 8.0, 1117.0 * 8.0);
    l.view = Some(crate::live::camera([cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0]));
    let t = std::time::Instant::now();
    while t.elapsed().as_secs() < 60 { l.step(); }
}
