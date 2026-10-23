//! Pre-decoded videos (re/scripts/export_video.py → `<cache>/video/*.vid`: BC1 / BC3 frames, each LZ4 + XOR to the
//! previous one), streamed from disk into two HUD textures (main.rs uploads them): `TEX` (BC1, full screen) for the
//! start-up intro (IntroSequence) and the main-menu backdrop loop (FrontendMenus.FrontendBackVideo); `SMALL` (BC3,
//! alpha) for the in-game ones: the minimap video (building panels, advisors, lords), the end banner, plus the building
//! panel sketch picture; `TEX` again for the mission-over video. No video decoder in the app.
//! ponytail: 1920×1080 = the original's `_low` files (it takes the 4K ones above 1920×1080 screens).
use crate::hud::HudTri;
use std::{cell::RefCell, fs::File, io::{Read, Seek, SeekFrom}, sync::atomic::{AtomicBool, AtomicU32, Ordering}, time::{Duration, Instant}};

/// HUD texture index of the full-screen video frame (main.rs: `video::texture` as the 5th HUD bind group).
pub const TEX: u32 = 4;
pub const W: u32 = 1920;
pub const H: u32 = 1080;
/// HUD texture index of the in-game videos (main.rs: `video::small_texture` as the 7th, after menu::SCREEN = 5): the end banner at (0, 0),
/// the minimap video at (BANNER[0], 0), the building panel's sketch picture at (0, BANNER[1]) (`sketch`).
pub const SMALL: u32 = 6;
const BANNER: [u32; 2] = [528, 308]; // IngameUIScreens.xaml MediaElement Width 528 (307 px tall + 1 transparent row)
const BLD: u32 = 176; // MainHUD RadarME is 124 design px; the 348 px source halved
const SKETCH: [u32; 2] = [248, 220]; // BuildingSketch is 123×109 design px: the 492×436 PNG halved (DoLoadImages' halfSize), 4-aligned
const SMALL_W: u32 = BANNER[0] + BLD;
const SMALL_H: u32 = BANNER[1] + SKETCH[1];

fn tex(device: &wgpu::Device, label: &str, w: u32, h: u32, format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
        dimension: wgpu::TextureDimension::D2, format, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
    })
}
/// The full-screen video texture for main.rs's HUD bind groups (BC1 sRGB like the atlases: the device has BC support).
pub fn texture(device: &wgpu::Device) -> wgpu::Texture { tex(device, "video", W, H, wgpu::TextureFormat::Bc1RgbaUnormSrgb) }
/// The in-game video texture (`SMALL`): BC3 for the banners' alpha.
pub fn small_texture(device: &wgpu::Device) -> wgpu::Texture { tex(device, "video2", SMALL_W, SMALL_H, wgpu::TextureFormat::Bc3RgbaUnormSrgb) }

/// After the last frame: start over, stop, or keep showing it.
#[derive(Clone, Copy, PartialEq)]
enum Mode { Loop, Once, Hold }

/// One .vid file, played by the wall clock from the first `update`.
pub struct Player {
    file: File,
    index: Vec<(u64, usize, bool)>, // per frame: file offset, LZ4 length, key frame
    fps: f32,
    start: usize,      // first frame of the webm (intro.vid: after the logos)
    size: [u32; 3],    // w, h, bytes per 4×4 block (8 BC1, 16 BC3)
    frame: Vec<u8>,    // BC1 / BC3 of frame `at`
    at: Option<usize>,
    t0: Option<Instant>,
    mode: Mode,
}

impl Player {
    /// `<cache>/video/<name>`, which must fit `size` (w, h, bytes per block) of its texture (smaller is drawn from the
    /// top-left: `trailer`); None = not exported.
    fn open(name: &str, mode: Mode, size: [u32; 3]) -> Option<Self> {
        let mut file = File::open(crate::cache_dir().join("video").join(name)).ok()?;
        let mut h = [0u8; 24];
        file.read_exact(&mut h).ok()?;
        let u = |i: usize| u32::from_le_bytes(h[i..i + 4].try_into().unwrap());
        let bpb = match &h[..4] { b"SHCV" => 8, b"SHC3" => 16, _ => return None };
        if u(4) > size[0] || u(8) > size[1] || bpb != size[2] { return None; }
        let size = [u(4), u(8), bpb];
        let n = u(16) as usize;
        let mut lens = vec![0u8; n * 4];
        file.read_exact(&mut lens).ok()?;
        let mut off = 24 + lens.len() as u64;
        let index = lens.chunks(4).map(|c| {
            let l = u32::from_le_bytes(c.try_into().unwrap());
            off += (l & 0x7fff_ffff) as u64;
            (off - (l & 0x7fff_ffff) as u64, (l & 0x7fff_ffff) as usize, l >> 31 == 1)
        }).collect();
        Some(Player { file, index, fps: u(12) as f32, start: u(20) as usize, size, frame: vec![], at: None, t0: None, mode })
    }

    fn frame_len(&self) -> usize { (self.size[0] / 4 * self.size[1] / 4 * self.size[2]) as usize }

    /// Frame due by the clock; None once a `Once` video is over.
    fn due(&mut self) -> Option<usize> {
        let (k, n) = ((self.t0.get_or_insert_with(Instant::now).elapsed().as_secs_f32() * self.fps) as usize, self.index.len());
        match self.mode { _ if k < n => Some(k), Mode::Loop => Some(k % n), Mode::Hold => Some(n - 1), Mode::Once => None }
    }

    /// Restart the clock at frame `k`.
    fn jump(&mut self, k: usize) { self.t0 = Some(Instant::now() - Duration::from_secs_f32(k as f32 / self.fps)); }

    /// Decodes on to frame `k`: from the current frame, or from the last key frame at or before `k`.
    fn seek(&mut self, k: usize) -> Option<()> {
        let key = (0..=k).rev().find(|&i| self.index[i].2)?;
        let from = match self.at { Some(a) if a < k && a >= key => a + 1, Some(a) if a == k => return Some(()), _ => key };
        let mut lz = vec![];
        for i in from..=k {
            let (off, len, key) = self.index[i];
            lz.resize(len, 0);
            self.file.seek(SeekFrom::Start(off)).ok()?;
            self.file.read_exact(&mut lz).ok()?;
            let d = lz4_flex::block::decompress(&lz, self.frame_len()).ok()?;
            if key { self.frame = d } else { self.frame.iter_mut().zip(d).for_each(|(f, x)| *f ^= x) }
            self.at = Some(i);
        }
        Some(())
    }

    /// Brings `tex` at `x` (texels from the left) to the frame due now; None = over (or a broken file).
    fn update(&mut self, queue: &wgpu::Queue, tex: &wgpu::Texture, x: u32) -> Option<usize> {
        let k = self.due()?;
        if self.at != Some(k) {
            self.seek(k)?;
            let [w, h, bpb] = self.size;
            queue.write_texture(wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d { x, y: 0, z: 0 }, aspect: wgpu::TextureAspect::All },
                &self.frame, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w / 4 * bpb), rows_per_image: None },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
        }
        Some(k)
    }
}

/// Texture `t`'s part `uv` (u0, v0, u1, v1) into screen rect `p` (x0, y0, x1, y1); colour alpha 0 = the shader's flat colour.
fn quad(out: &mut Vec<HudTri>, t: u32, p: [f32; 4], uv: [f32; 4], color: [f32; 4]) {
    let v = |x: f32, y: f32, u: f32, s: f32| crate::Vert { pos: [x, y], uv: [u, s], color, team: [0.0; 4] };
    let (tl, tr, bl, br) = (v(p[0], p[1], uv[0], uv[1]), v(p[2], p[1], uv[2], uv[1]), v(p[0], p[3], uv[0], uv[3]), v(p[2], p[3], uv[2], uv[3]));
    out.extend([(t, [tl, tr, bl]), (t, [tr, br, bl])]);
}
const ALL: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const FULL: [u32; 3] = [W, H, 8];

static MENU_SHOWN: AtomicBool = AtomicBool::new(false); // the texture holds a menu-loop frame

/// FrontendMenus.FrontendBackVideo: front_end_background_low, looped (NoesisMediaPlayer loops a uri without '*';
/// FrontendBackVideo_Ended's own restart is off). Starts at its first frame on the first `update`.
pub struct MenuLoop(Option<Player>);

impl MenuLoop {
    pub fn new() -> Self { MenuLoop(Player::open("menu.vid", Mode::Loop, FULL)) }
    /// Every frame the front end (with its backdrop) is shown.
    pub fn update(&mut self, queue: &wgpu::Queue, tex: &wgpu::Texture) {
        let shown = self.0.as_mut().and_then(|p| p.update(queue, tex, 0)).is_some();
        MENU_SHOWN.store(shown, Ordering::Relaxed);
    }
}

/// menu.rs: the backdrop video's current frame into screen rect `p`; false = no video (draw the still instead).
pub fn backdrop(out: &mut Vec<HudTri>, p: [f32; 4]) -> bool {
    let shown = MENU_SHOWN.load(Ordering::Relaxed);
    if shown { quad(out, TEX, p, ALL, [1.0; 4]) }
    shown
}

/// IntroSequence at start-up: FadeInLogos (5.5 s, composed by the export) then intro-low.webm with its sound
/// (StartVideo: speech volume), letterboxed on black (Viewbox, Stretch Uniform), over FatControler.Start's music 8.
/// ButtonClicked(fromClick): a click during the logos starts the video, a click during the video ends it; Escape
/// (KeyManager, main.rs drops the Intro) ends it all. The original's `skipintro` setting is main.rs's `--skip-intro`.
/// ponytail: no loading bar (nothing to wait for), no first-run Enter Your Name panel.
pub struct Intro { p: Player, sound: bool }

impl Intro {
    pub fn new() -> Option<Self> {
        let i = Intro { p: Player::open("intro.vid", Mode::Once, FULL)?, sound: false };
        if i.p.start >= i.p.index.len() || !i.p.index[i.p.start].2 { return None; } // the webm's first frame is a key frame
        crate::audio::startup();
        Some(i)
    }

    /// Left-button release (IntroSkipCommand: a full-window Button, ClickMode Release): during the logos the video
    /// starts; during the video the intro ends (false).
    pub fn click(&mut self) -> bool {
        let logos = self.p.due().is_some_and(|k| k < self.p.start);
        if logos { self.p.jump(self.p.start) }
        logos
    }

    /// This frame for a `w`×`h` window (logical px); None = over (drop it, that stops its sound).
    pub fn draw(&mut self, queue: &wgpu::Queue, tex: &wgpu::Texture, w: f32, h: f32) -> Option<Vec<HudTri>> {
        let k = self.p.update(queue, tex, 0)?;
        MENU_SHOWN.store(false, Ordering::Relaxed);
        if k >= self.p.start && !self.sound {
            self.sound = true;
            crate::audio::video(Some(&crate::cache_dir().join("video/intro.wav")));
        }
        let s = (w / W as f32).min(h / H as f32);
        let (x, y) = ((w - W as f32 * s) / 2.0, (h - H as f32 * s) / 2.0);
        let mut out = vec![];
        quad(&mut out, TEX, [0.0, 0.0, w, h], ALL, [0.0; 4]);
        quad(&mut out, TEX, [x, y, w - x, h - y], ALL, [1.0; 4]);
        Some(out)
    }
}

impl Drop for Intro {
    fn drop(&mut self) { if self.sound { crate::audio::video(None) } }
}

/// In-game videos, asked for by the HUD / end screen while they draw and played by `pump`: slot 0 the end banner and
/// slot 1 the engine's video over the minimap (on `SMALL`), slot 2 the mission-over video (full screen, on `TEX`:
/// free in a game). A slot not asked for in a frame stops (and its sound with it).
struct Want { file: String, key: i32, mode: Mode }
struct Now { file: String, key: i32, p: Player, last: Option<usize>, done: bool } // last frame shown; done: a `Once` ended
struct Slot { want: Option<Want>, now: Option<Now> }
const NO: Slot = Slot { want: None, now: None };
thread_local! {
    static SLOTS: RefCell<[Slot; 3]> = const { RefCell::new([NO, NO, NO]) };
    // the building panel's sketch picture: (asked for this frame, in the texture), Assets/GUI/Images file names
    static SKETCHES: RefCell<(Option<String>, Option<String>)> = const { RefCell::new((None, None)) };
}
static PUMPED: AtomicBool = AtomicBool::new(false); // main.rs calls `pump`
/// The serial of the engine's last video that ended or could not play (`Engine::playing`, read on the sim thread).
static ENDED: AtomicU32 = AtomicU32::new(0);

/// Ask for `file` (`key`: a new one restarts it) in `slot` this frame. Some(true) = its frame is in the texture,
/// Some(false) = starting (the first frame comes with this frame's `pump`) or a `Once` video is over, None = can't play
/// (not exported, or main.rs doesn't pump: the caller draws its fallback).
fn want(slot: usize, file: String, key: i32, mode: Mode) -> Option<bool> {
    SLOTS.with(|s| {
        let s = &mut s.borrow_mut()[slot];
        let now = s.now.as_ref().filter(|n| n.file == file && n.key == key);
        let shown = now.is_some_and(|n| n.last.is_some() && !n.done);
        let ok = PUMPED.load(Ordering::Relaxed) && (now.is_some() || crate::cache_dir().join("video").join(&file).exists());
        if ok { s.want = Some(Want { file, key, mode }) }
        ok.then_some(shown)
    })
}

/// main.rs, every frame after the HUD is built and before it is drawn: starts, advances and stops the asked-for
/// videos in `small` (`small_texture`) and `full` (`texture`), uploads the asked-for sketch. A looped video's sound
/// (<name>.wav) restarts with each loop, like Unity's.
pub fn pump(queue: &wgpu::Queue, small: &wgpu::Texture, full: &wgpu::Texture) {
    PUMPED.store(true, Ordering::Relaxed);
    SLOTS.with(|s| for (i, slot) in s.borrow_mut().iter_mut().enumerate() {
        let want = slot.want.take();
        if slot.now.as_ref().map(|n| (&n.file, n.key)) != want.as_ref().map(|w| (&w.file, w.key)) {
            if let Some(n) = slot.now.take() {
                if sound(&n.file).is_some() { crate::audio::video(None) }
                if i == 1 { ENDED.store(n.key as u32, Ordering::Relaxed) } // stopped / replaced: RadarME_Ended
            }
            let size = [[BANNER[0], BANNER[1], 16], [BLD, BLD, 16], FULL][i];
            slot.now = want.and_then(|w| Some(Now { p: Player::open(&w.file, w.mode, size)?, file: w.file, key: w.key, last: None, done: false }));
        }
        if let Some(n) = &mut slot.now {
            let (tex, x) = [(small, 0), (small, BANNER[0]), (full, 0)][i];
            let Some(k) = n.p.update(queue, tex, x) else {
                if i == 1 && !n.done { ENDED.store(n.key as u32, Ordering::Relaxed) }
                n.done = true;
                continue;
            };
            if n.last.is_none_or(|l| k < l) { if let Some(w) = sound(&n.file) { crate::audio::video(Some(&w)) } }
            n.last = Some(k);
        }
    });
    SKETCHES.with(|s| {
        let (want, now) = &mut *s.borrow_mut();
        if let Some(f) = want.take().filter(|f| now.as_ref() != Some(f)) {
            if let Some(px) = sketch_blocks(&f) {
                queue.write_texture(wgpu::TexelCopyTextureInfo { texture: small, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: BANNER[1], z: 0 }, aspect: wgpu::TextureAspect::All },
                    &px, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(SKETCH[0] / 4 * 16), rows_per_image: None },
                    wgpu::Extent3d { width: SKETCH[0], height: SKETCH[1], depth_or_array_layers: 1 });
            }
            *now = Some(f); // a missing file isn't retried every frame
        }
    });
}

/// The .wav next to a .vid, if it has sound.
fn sound(file: &str) -> Option<std::path::PathBuf> {
    Some(crate::cache_dir().join("video").join(file.strip_suffix(".vid")?.to_string() + ".wav")).filter(|p| p.exists())
}

/// Sketch `file` (Assets/GUI/Images, read straight from the game folder like the credits pictures) as `SKETCH` BC3.
fn sketch_blocks(file: &str) -> Option<Vec<u8>> {
    let png = shc_extract::tex::Rgba::png(&crate::paths::game().join("Assets/GUI/Images").join(file)).map_err(|e| eprintln!("sketch {file}: {e}")).ok()?;
    let (w, h) = (SKETCH[0] as usize, SKETCH[1] as usize);
    Some(shc_extract::bc::encode(&png.resize(w, h).px, w, h, true))
}

/// reports.rs: the sketch picture `file` (MainViewModel.imageFileNames, e.g. "woodcutter_sketch.png") into rect `p`.
/// False on the frame it loads (the next `pump` uploads it) or when the game folder lacks it.
pub fn sketch(out: &mut Vec<HudTri>, file: &str, p: [f32; 4]) -> bool {
    SKETCHES.with(|s| {
        let (want, now) = &mut *s.borrow_mut();
        *want = Some(file.to_string());
        let shown = now.as_deref() == Some(file) && crate::paths::game().join("Assets/GUI/Images").join(file).is_file();
        let (u, v0, v1) = (SKETCH[0] as f32 / SMALL_W as f32, BANNER[1] as f32 / SMALL_H as f32, 1.0);
        if shown { quad(out, SMALL, p, [0.0, v0, u, v1], [1.0; 4]) }
        shown
    })
}

/// SFXManager.customLordDefaultBinks: an AI's line video "*<player>*<SK_ key>" when the lord's CustomMedia folder has
/// no (exported) reaction video.
fn custom_lord_video(key: &str) -> &'static str {
    match key {
        "sk_nerv_weak" | "sk_die_ally" | "sk_ally_need_help" | "sk_cant_attack" | "sk_team_losing" => "bad_soldier_nevous",
        _ => "bad_soldier_taunt",
    }
}

/// The engine's video channel (SFXManager.playBink → MainHUD RadarME over the minimap), kept per tick from render
/// event 0x15 (GameMap case 21: r[3] < 0 stop, else play retData's binkFileName at +0x704, r[3] > 0 looped, r[11]
/// waitForSpeech). The sim starts building videos when a panel opens (hudmsg::f965b0) and stops them on panel / state
/// changes (game::request_state); advisor and lord messages play theirs (scenario.rs, hudmsg). `now` = file stem,
/// looped, serial (a replay of the same file restarts it); `wait` = a one-shot one counts as playing until speech
/// channel 1 is quiet (playBink's binkWaitForSpeech, RadarME_Ended's state 3).
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Engine { pub now: Option<(String, bool, u32)>, serial: u32, wait: bool }

impl Engine {
    /// live.rs, after every run_tick: `recs` = its draw records, `ret` = the PlayStateReturnData it filled.
    pub fn scan(&mut self, recs: &[[i16; 21]], ret: &[u8]) {
        for r in recs.iter().filter(|r| r[0] == 0x15) {
            self.serial += 1;
            let name = ret.get(1796..1924).map(|b| b.split(|&c| c == 0).next().unwrap_or(&[])).unwrap_or(&[]);
            let name = String::from_utf8_lossy(name).to_lowercase();
            let mut stem = name.rsplit(['/', '\\']).next().unwrap_or("").trim_end_matches(".bik").trim_end_matches(".webm").to_string();
            if let Some((p, key)) = crate::custom_lords::split(&stem) { stem = crate::custom_lords::video_stem(p, &key).unwrap_or_else(|| custom_lord_video(&format!("sk_{}", key.to_lowercase())).into()) }
            self.now = (r[3] >= 0 && !stem.is_empty()).then_some((stem, r[3] > 0, self.serial));
            self.wait = r[11] != 0 && crate::audio::speech_state().0;
        }
    }

    /// DLL_RunTick's `binkPlaying` (SFXManager.isBinkPlaying: started and looped, or not ended yet): the engine
    /// copies it into its player byte every tick, so its stops only fire while this is true. A one-shot video plays
    /// until `pump` saw it end (or `radar` found it not exported); no `pump` (tests, no window): over at once.
    pub fn playing(&self) -> bool {
        self.now.as_ref().is_some_and(|n| n.1 || PUMPED.load(Ordering::Relaxed)
            && (ENDED.load(Ordering::Relaxed) != n.2 || self.wait && crate::audio::speech_state().0))
    }
}

/// hud.rs: the engine's current video `e` over the minimap rect `p` (RadarME, Stretch UniformToFill); `arab` = an
/// Arab lord (playBink: their wedding video is the mosque's). False = none playing, a one-shot one is over, or not
/// exported (the minimap shows).
pub fn radar(out: &mut Vec<HudTri>, e: &Engine, p: [f32; 4], arab: bool) -> bool {
    let Some((name, looped, serial)) = &e.now else { return false };
    let name = if arab && name.contains("action_marriage") { "st37_mosque1" } else { name };
    let w = want(1, format!("{name}.vid"), *serial as i32, if *looped { Mode::Loop } else { Mode::Once });
    if w.is_none() { ENDED.store(*serial, Ordering::Relaxed) } // MediaElement fails: MediaEnded at once
    if w == Some(true) { quad(out, SMALL, p, [BANNER[0] as f32 / SMALL_W as f32, 0.0, 1.0, BLD as f32 / SMALL_H as f32], [1.0; 4]) }
    w == Some(true)
}

/// lord_info.rs: a custom lord's living portrait `file` (custom_lords::vid "portrait", 528×308 with its alpha) looped
/// into rect `p`, in the end banner's slot (free in the front end). False = not exported yet (or not started: nothing
/// drawn this frame).
pub fn lord(out: &mut Vec<HudTri>, file: &str, p: [f32; 4], alpha: f32) -> bool {
    let w = want(0, file.to_string(), 0, Mode::Loop);
    if w == Some(true) { quad(out, SMALL, p, [0.0, 0.0, BANNER[0] as f32 / SMALL_W as f32, BANNER[1] as f32 / SMALL_H as f32], [1.0, 1.0, 1.0, alpha]) }
    w == Some(true)
}

/// endgame.rs: IngameUIScreens.triggerVideos' banner `key` (`banner_<faction>_<victory|defeat>`, the uri ends in '*':
/// played once, then its last frame stays) into rect `p` (528 × 307 design px). True also on the frame it starts (nothing
/// drawn: the drop begins empty). False = not exported or main.rs doesn't `pump` yet: draw the still.
pub fn banner(out: &mut Vec<HudTri>, key: &str, p: [f32; 4]) -> bool {
    let w = want(0, format!("{key}.vid"), 0, Mode::Hold);
    if w == Some(true) { quad(out, SMALL, p, [0.0, 0.0, BANNER[0] as f32 / SMALL_W as f32, (BANNER[1] - 1) as f32 / SMALL_H as f32], [1.0; 4]) }
    w.is_some()
}

/// lord_info.rs: a custom lord's story trailer `name` (custom_lords::vid "trailer", stored at its own size, e.g.
/// 1280×720) played once in the full-screen slot, scaled to fit the `w`×`h` window over black. As `mission_over`:
/// None = can't play, else Some(over).
pub fn trailer(out: &mut Vec<HudTri>, name: &str, w: f32, h: f32) -> Option<bool> {
    let shown = want(2, format!("{name}.vid"), 0, Mode::Once)?;
    quad(out, TEX, [0.0, 0.0, w, h], ALL, [0.0; 4]);
    let (over, vs) = SLOTS.with(|s| s.borrow()[2].now.as_ref().map_or((false, [W, H, 8]), |n| (n.done, n.p.size)));
    if shown || over {
        let (vw, vh) = (vs[0] as f32, vs[1] as f32);
        let k = (w / vw).min(h / vh);
        let (x, y) = ((w - vw * k) / 2.0, (h - vh * k) / 2.0);
        quad(out, TEX, [x, y, w - x, h - y], [0.0, 0.0, vw / W as f32, vh / H as f32], [1.0; 4]);
    }
    Some(over)
}

/// The full-screen slot is playing (`mission_over`, a custom lord's trailer): main.rs leaves the menu backdrop loop,
/// which shares its texture, alone meanwhile.
pub fn full_busy() -> bool { SLOTS.with(|s| s.borrow()[2].now.is_some()) }

/// endgame.rs: HUD_MissionOver.LoadVideoBackground's video `name` (`crusader_win`, `desert_background`, ...) over the
/// whole `w`×`h` window, black behind it (the Grid's Background; the Viewbox keeps 16:9), its last frame held, or
/// `looped`. None = can't play (not exported yet: the caller goes on without it), else Some(over).
pub fn mission_over(out: &mut Vec<HudTri>, name: &str, looped: bool, w: f32, h: f32) -> Option<bool> {
    let shown = want(2, format!("{name}.vid"), 0, if looped { Mode::Loop } else { Mode::Once })?;
    quad(out, TEX, [0.0, 0.0, w, h], ALL, [0.0; 4]);
    let over = SLOTS.with(|s| s.borrow()[2].now.as_ref().is_some_and(|n| n.done));
    if shown || over { // a `Once` video's last frame stays in the texture
        let s = (w / W as f32).min(h / H as f32);
        let (x, y) = ((w - W as f32 * s) / 2.0, (h - H as f32 * s) / 2.0);
        quad(out, TEX, [x, y, w - x, h - y], ALL, [1.0; 4]);
    }
    Some(over)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exported files decode end to end: right frame counts, key frames where seeking needs them, the logos'
    /// first frame bright (loading background), the menu loop wraps (skips if not exported).
    #[test]
    fn videos_decode() {
        let (Some(mut m), Some(mut p)) = (Player::open("menu.vid", Mode::Loop, FULL), Player::open("intro.vid", Mode::Once, FULL)) else { return };
        assert_eq!((m.index.len(), m.fps, p.start, p.index.len()), (241, 30.0, 165, 165 + 338));
        assert!(p.index[0].2 && p.index[p.start].2 && !p.index[1].2, "key frames: first frame, first webm frame");
        // every frame decodes, in order, and seeking back lands on the same bytes
        let mut sums = vec![];
        for k in 0..p.index.len() { p.seek(k).unwrap(); sums.push(p.frame.iter().map(|&b| b as u64).sum::<u64>()); }
        p.seek(p.start + 10).unwrap();
        assert_eq!(p.frame.iter().map(|&b| b as u64).sum::<u64>(), sums[p.start + 10]);
        // BC1 colour0 (RGB565) of a centre block: frame 0 = the loading background, not black
        let c0 = |f: &[u8]| { let b = ((H / 8) * (W / 4) + W / 8) as usize * 8; u16::from_le_bytes([f[b], f[b + 1]]) };
        p.seek(0).unwrap();
        assert!(c0(&p.frame) > 0x4000, "{:x}", c0(&p.frame));
        m.seek(240).unwrap();
        m.seek(0).unwrap(); // wrap: back to the key frame
        assert_eq!(m.at, Some(0));
        m.t0 = Some(Instant::now() - Duration::from_secs(9)); // 8.03 s loop
        assert!(m.due().unwrap() < 241);
        // ButtonClicked: a click in the logos jumps to the webm, the next one ends the intro (sound off: Drop is silent)
        let mut i = Intro { p, sound: false };
        i.p.t0 = None;
        assert!(i.click() && (165..170).contains(&i.p.due().unwrap()));
        assert!(!i.click());
    }

    /// The in-game files: every building video and banner decodes at its slot's size, a banner holds its last frame,
    /// a building video loops; the engine's channel follows render event 0x15; HUD quads land on their part of `SMALL`.
    #[test]
    fn ingame_videos() {
        let Some(mut b) = Player::open("banner_crusader_victory.vid", Mode::Hold, [528, 308, 16]) else { return };
        assert_eq!((b.index.len(), b.fps), (420, 60.0));
        b.t0 = Some(Instant::now() - Duration::from_secs(60));
        assert_eq!(b.due(), Some(419));
        b.seek(419).unwrap();
        let a = (0..b.frame.len() / 16).filter(|i| b.frame[i * 16] == 0 && b.frame[i * 16 + 1] == 0).count(); // BC3 alpha endpoints 0 = clear
        assert!(a > 1000 && a < b.frame.len() / 16 - 1000, "{a} transparent blocks: the banner's alpha is lost");
        // SFXManager.buildingBinks
        for f in ["st03_woodcutters_hut", "st07_hunters_hut", "st12_fletchers_workshop", "st13_blacksmiths_workshop", "st14_poleturners_workshop",
                  "st15_armourers_workshop", "st16_tanners_workshop", "st17_bakers_workshop", "st22_inn", "st26_tradepost", "st30_wheatfarm",
                  "st32_applefarm", "st33_cattlefarm", "st34_mill", "st36_church1", "st37_mosque1"] {
            let mut p = Player::open(&format!("{f}.vid"), Mode::Loop, [BLD, BLD, 16]).unwrap_or_else(|| panic!("{f}"));
            let n = p.index.len();
            for k in 0..n { p.seek(k).unwrap() }
            p.t0 = Some(Instant::now() - Duration::from_secs_f32((n + 2) as f32 / p.fps));
            assert!(p.due().unwrap() < 4, "{f} loops");
        }
        assert!(sound("st13_blacksmiths_workshop.vid").is_some() && sound("st22_inn.vid").is_none());
        // render event 0x15 + retData's binkFileName (+0x704): play looped, stop, play once, a replay restarts
        let mut e = Engine::default();
        let (mut ret, rec) = (vec![0u8; 2836], |r3: i16| { let mut r = [0i16; 21]; (r[0], r[3]) = (0x15, r3); r });
        ret[1796..1796 + 29].copy_from_slice(b"st13_blacksmiths_workshop.bik");
        e.scan(&[[0; 21], rec(1)], &ret);
        assert_eq!(e.now, Some(("st13_blacksmiths_workshop".into(), true, 1)));
        e.scan(&[rec(-1)], &ret);
        assert_eq!(e.now, None);
        e.scan(&[rec(-1), rec(0)], &ret);
        assert_eq!(e.now, Some(("st13_blacksmiths_workshop".into(), false, 4))); // serial counts every 0x15
        assert!(!e.playing(), "a one-shot counts as over");
        e.scan(&[], &ret); // ticks without an event keep it
        assert!(e.now.is_some());
        // no `pump` in main.rs: the callers keep their fallbacks (still banner, minimap)
        let (mut out, r) = (vec![], [10.0, 10.0, 134.0, 134.0]);
        e.scan(&[rec(1)], &ret);
        assert!(!radar(&mut out, &e, r, false) && !banner(&mut out, "banner_arab_defeat", r));
        // pumped: the first frame asks (nothing drawn yet), once a frame is up the quads land on their parts of SMALL
        PUMPED.store(true, Ordering::Relaxed);
        assert!(!radar(&mut out, &e, r, false) && !radar(&mut out, &Engine::default(), r, false) && banner(&mut out, "banner_arab_defeat", r) && out.is_empty());
        SLOTS.with(|s| for (i, s) in s.borrow_mut().iter_mut().enumerate().take(2) {
            let w = s.want.take().expect("asked for");
            let size = if i == 0 { [528, 308, 16] } else { [BLD, BLD, 16] };
            s.now = Some(Now { p: Player::open(&w.file, w.mode, size).unwrap(), file: w.file, key: w.key, last: Some(0), done: false });
        });
        assert!(radar(&mut out, &e, r, false) && banner(&mut out, "banner_arab_defeat", r));
        assert_eq!((out.len(), out[0].0, out[0].1[0].uv, out[2].1[1].uv), (4, SMALL, [0.75, 0.0], [0.75, 0.0]));
        // a one-shot video that ended: nothing over the minimap
        SLOTS.with(|s| s.borrow_mut()[1].now.as_mut().unwrap().done = true);
        assert!(!radar(&mut out, &e, r, false));
        // pumped: a one-shot one plays until `pump` saw it end (RadarME_Ended); one not exported is over at once
        e.scan(&[rec(0)], &ret);
        assert!(e.playing());
        ENDED.store(e.now.as_ref().unwrap().2, Ordering::Relaxed);
        assert!(!e.playing());
        e.scan(&[rec(0)], &ret);
        assert!(e.playing(), "the next one plays");
        ret[1796..1796 + 14].copy_from_slice(b"no_such_video\0");
        e.scan(&[rec(0)], &ret);
        assert!(!radar(&mut out, &e, r, false) && !e.playing(), "not exported");
        // an AI lord's line "*<player>*<key>": customLordDefaultBinks; an Arab lord's wedding is the mosque's
        ret[1796..1796 + 16].copy_from_slice(b"*2*SK_NERV_WEAK\0");
        e.scan(&[rec(0)], &ret);
        assert_eq!(e.now.as_ref().unwrap().0, "bad_soldier_nevous");
        ret[1796..1796 + 21].copy_from_slice(b"action_marriage.bik\0\0");
        e.scan(&[rec(0)], &ret);
        radar(&mut out, &e, r, true);
        assert_eq!(SLOTS.with(|s| s.borrow()[1].want.as_ref().map(|w| w.file.clone())).as_deref(), Some("st37_mosque1.vid"));
        // export_more's files (when exported): the minimap clips at its size, the mission-over videos full screen
        for (f, size, n) in [("pg_vict1", [BLD, BLD, 16], 101), ("abbot_angry", [BLD, BLD, 16], 122), ("crusader_win", FULL, 270), ("desert_background", FULL, 129)] {
            let Some(mut p) = Player::open(&format!("{f}.vid"), Mode::Once, size) else { continue };
            assert_eq!(p.index.len(), n, "{f}");
            p.seek(n - 1).unwrap();
        }
        assert!(Player::open("crusader_win.vid", Mode::Once, FULL).is_none() || sound("pg_vict1.vid").is_some(), "a character video's sound");
    }

    /// Campaign mission 3 to its first message video (scenario.rs msg_show: the sergeant's, one-shot, at tick 7999), the
    /// ticks run with plain DLL_RunTick (Live::step is ~10x slower). SHC_SAVES set: saved a few ticks before as
    /// `Advisor.sav` (screenshots: `--live` it and the video plays over the minimap).
    #[cfg(feature = "live")]
    #[test]
    fn mission_video_live() {
        let Ok(mut m) = crate::skirmish::start_mission(3, 1) else { return };
        let mut ret = vec![0u8; shc_sim::hudstate::RET_LEN];
        for t in 0..7990 {
            if t == 7900 && std::env::var_os("SHC_SAVES").is_some() {
                std::fs::write(crate::skirmish::saves_dir().join("Advisor.sav"), shc_sim::save::save_game(&mut m, 0)).unwrap();
            }
            shc_sim::frontend::run_tick(&mut m, &shc_sim::frontend::TickInput::default(), &mut ret, None);
        }
        let mut l = crate::live::Live::from_mem(m);
        let v = (0..60).find_map(|_| { l.step(); l.hud().ev.video.now.clone() });
        assert_eq!(v.as_ref().map(|v| (v.0.as_str(), v.1)), Some(("good_soldier_nervous", false)), "{v:?}");
        if Player::open("crusader_win.vid", Mode::Once, FULL).is_some() { // export_more ran here
            assert!(Player::open("good_soldier_nervous.vid", Mode::Once, [BLD, BLD, 16]).is_some());
        }
    }

    /// The real sim drives the channel (needs the user's Crater Lake save): a clicked woodcutter's panel plays its
    /// video looped (hudmsg::f965b0), right-click closes the panel and stops it (game::request_state), the hunter's
    /// panel plays the hunter's, the woodcutter's again restarts it. Without `bink_playing` or the stops the second
    /// video would never start.
    #[cfg(feature = "live")]
    #[test]
    fn engine_channel_live() {
        use crate::live::TroopIn;
        use shc_sim::buildings as b;
        let p = crate::paths::test_save("Crater Lake-1test.sav");
        if !std::path::Path::new(&p).exists() { return; }
        let mut l = crate::live::Live::load(&p);
        l.step();
        l.records();
        let (kx, ky) = l.keep().map(|(x, y)| (x as i32, y as i32)).unwrap();
        let mut placed = vec![];
        for kind in [3, 7] { // woodcutter, hunter
            let mapper = (0..400).find(|&m| b::mapper_kind(m) == kind).unwrap();
            assert!(l.set_tool(mapper));
            let (x, y) = (-30..=30).flat_map(|d| (-30..=30).map(move |e| (kx + d, ky + e))).find(|&(x, y)| l.hover(mapper, x, y).ok).unwrap();
            l.click(mapper, x, y);
            for _ in 0..20 { l.step(); }
            l.set_tool(0);
            placed.push((1..b::SLOTS).find(|&i| b::state(l.mem_ref(), i) != 0 && b::owner(l.mem_ref(), i) == 1 && b::kind(l.mem_ref(), i) == kind).unwrap());
        }
        // a click on a building (military.rs' tests): the mouse tile on it, no unit under the cursor
        let click = |l: &mut crate::live::Live, id: i32| {
            let r = b::rec(id);
            let (n, x, y) = (l.mem_ref().i32(r + 0xf8), l.mem_ref().i16(r + 0xee) as i32, l.mem_ref().i16(r + 0xf0) as i32);
            l.mouse = [x + n / 2, y + n / 2];
            let (w, s) = ([-1e6f32, -1e6], [1.0f32, 1.0]);
            l.troop_event(TroopIn::Cursor(w, s, [false; 3]));
            l.troop_event(TroopIn::Press(w, s, [w[0] - 900.0, w[1] - 500.0, w[0] + 900.0, w[1] + 500.0]));
            l.step();
            l.troop_event(TroopIn::Release);
            for _ in 0..3 { l.step(); }
            l.hud().ev.video.now.map(|v| (v.0, v.1))
        };
        let w = Some(("st03_woodcutters_hut".to_string(), true));
        assert_eq!(click(&mut l, placed[0]), w);
        l.troop_event(TroopIn::Right(true));
        l.step();
        l.troop_event(TroopIn::Right(false));
        for _ in 0..3 { l.step(); }
        assert_eq!(l.hud().ev.video.now, None, "closing the panel stops the video");
        assert_eq!(click(&mut l, placed[1]), Some(("st07_hunters_hut".to_string(), true)));
        assert_eq!(click(&mut l, placed[0]), w);
    }
}
