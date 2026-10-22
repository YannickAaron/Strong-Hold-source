//! Game replays (ours; the original has none). A replay = the start state (the whole sim memory, lz4) + every
//! DLL_RunTick call's input: the `TickInput` fields, the chores it ran (in run order), and the memory the front end
//! wrote since the last tick (`patches`: DLL_GameAction / MapAction / SetMapRotation write engine state directly, e.g.
//! GameAction 1005 rations a player record, besides queueing chores). Playback feeds them back through the same RunTick
//! (`Tap::Play` in `Live`), so the sim runs the same ticks and its save sections match the original run every tick
//! (test `replay_matches_original`). The sim is untouched: the tap reads the chore slots after a tick and files the
//! recorded ones before it, like DLL_ReceiveChore does for MP peers; writes between ticks come from
//! `Mem::track_writes` (a page bitmap, on only between ticks) diffed against a shadow of the pages written before.
//!
//! Why raw memory and not a native .sav as the start: save → load → tick differs from the game that kept running in
//! ~14 sections at once (runtime state the save does not carry), so a replay from a .sav would drift.
//!
//! Chores are the engine's MP wire packet `[type u8][exec u24][id i32][payload]` (FUN_1800237d0, sim mp.rs) plus the
//! sender: LAN games record the same way (every peer runs the same chores), so MP games replay too.
//! Snapshots (lz4 memory, every `SNAP_EVERY` frames as playback passes them) make seeking back cheap (restore + the
//! frames in between headless, ~1.75 ms CPU each); seeking forward plays on at full speed (`Controls`).
//! The draw rect is replayed too: the engine's draw pass raises building heights (+0xec) it draws, so the camera is a
//! sim input and the replay's camera follows the recording.
use shc_sim::{commands::CHORES, core::TICK_COUNTER, frontend::TickInput, mem::{self, Mem}};
use std::sync::{Arc, Mutex};

/// Snapshot interval during playback, in RunTick frames.
pub const SNAP_EVERY: usize = 2000;
/// A section hash goes into the file every this many stepping ticks (desync check during playback).
const HASH_EVERY: i32 = 100;
const MAGIC: &[u8] = b"SHCREPLAY1\n";

// ---------------------------------------------------------------- TickInput <-> i32 array

trait Field: Copy { fn put(self) -> i32; fn get(v: i32) -> Self; }
impl Field for bool { fn put(self) -> i32 { self as i32 } fn get(v: i32) -> Self { v != 0 } }
impl Field for i32 { fn put(self) -> i32 { self } fn get(v: i32) -> Self { v } }
impl Field for u32 { fn put(self) -> i32 { self as i32 } fn get(v: i32) -> Self { v as u32 } }

macro_rules! fields {
    ($($f:ident),*) => {
        const FIELDS: &[&str] = &[$(stringify!($f)),*];
        fn pack(i: &TickInput) -> Vec<i32> { vec![$(Field::put(i.$f)),*] }
        fn unpack(v: &[i32], i: &mut TickInput) { let mut k = 0; $( i.$f = Field::get(v[k]); k += 1; )* let _ = k; }
    };
}
// every field: the sim reads most of them (sound flags feed the music manager's RNG draw, now_ms the clocks)
fields!(flattened, mouse_x, mouse_y, shift, ctrl, alt, paused, ambient1, ambient2, speech1, speech2, music_playing,
    music_about_to_loop, bink_playing, centre_pos_x, centre_pos_y, tiles_wide, tiles_high, radar_w, radar_h, radar_zoom,
    screen_zoom, sh1_rts, centre_tile_x, centre_tile_y, no_step, building_over_depth, troop_over_depth, scr_left,
    scr_right, scr_top, scr_bottom, now_ms);

/// Whether RunTick runs the sim step (and the chores) with this input.
fn steps(i: &TickInput) -> bool { !i.no_step && !i.paused }

// ---------------------------------------------------------------- chores

/// One chore as it ran: the sender and the MP wire packet (payload without its zero tail).
#[derive(Clone, Debug, PartialEq)]
pub struct Chore { pub player: u8, pub packet: Vec<u8> }

impl Chore {
    pub fn new(player: i32, ty: u8, exec: i32, id: i32, payload: &[u8]) -> Self {
        let mut p = vec![ty];
        p.extend_from_slice(&exec.to_le_bytes()[..3]);
        p.extend_from_slice(&id.to_le_bytes());
        let n = payload.iter().rposition(|&b| b != 0).map_or(0, |k| k + 1);
        p.extend_from_slice(&payload[..n]);
        Chore { player: player as u8, packet: p }
    }
    pub fn ty(&self) -> u8 { self.packet[0] }
    pub fn id(&self) -> i32 { i32::from_le_bytes(self.packet[4..8].try_into().unwrap()) }
    pub fn payload(&self) -> &[u8] { &self.packet[8..] }
}

const SLOTS: i32 = 500;
const SLOT_PAYLOAD: usize = 0x4ec;
/// Chore slot i (commands.rs): +0 exec, +4 player, +8 type, +9 state (1..9 pending, 10 done), +0xc id, +0x11 payload.
fn slot(i: i32) -> u64 { CHORES + 0xb0bf8 + i as u64 * 0x500 }
/// FUN_180021eb0's due list: (slot, id) pairs at +0x370c00, count +0x372b40; scan start +0x370bf0.
const DUE: u64 = CHORES + 0x370c00;
const DUE_N: u64 = CHORES + 0x372b40;
const SCAN_FROM: u64 = CHORES + 0x370bf0;

fn read_slot(m: &Mem, i: i32) -> Chore {
    let s = slot(i);
    Chore::new(m.i32(s + 4), m.u8(s + 8), m.i32(s), m.i32(s + 0xc), m.slice(s + 0x11, SLOT_PAYLOAD))
}
fn pending(m: &Mem, i: i32) -> bool { matches!(m.u8(slot(i) + 9), 1..=9) }

/// Before a RunTick (recording): the pending chores, so one that runs and whose slot is reused in the same tick is
/// still known; the due count cleared, so after the tick it says what this tick ran (run_due may not run).
fn before_record(m: &mut Mem) -> Vec<Chore> {
    m.set_i32(DUE_N, 0);
    (0..SLOTS).filter(|&i| pending(m, i)).map(|i| read_slot(m, i)).collect()
}

/// After a RunTick: the chores run_due ran (FUN_18001f7b0 marks them 10), in its order (by id).
fn ran(m: &Mem, before: &[Chore]) -> Vec<Chore> {
    (0..m.i32(DUE_N).clamp(0, 1000) as u64).filter_map(|k| {
        let (i, id) = (m.i32(DUE + k * 8), m.i32(DUE + k * 8 + 4));
        if m.i32(slot(i) + 0xc) == id { (m.u8(slot(i) + 9) >= 10).then(|| read_slot(m, i)) } else { before.iter().find(|c| c.id() == id).cloned() }
    }).collect()
}

/// Playback: drop the chores the front end queued itself (the draw pass's 0x4f, stray UI input); the recorded ones
/// are the truth. Slots are not in any save section.
fn purge(m: &mut Mem) {
    for i in 0..SLOTS { if pending(m, i) { m.set_u8(slot(i) + 9, 0); } }
}

/// Playback: file a recorded chore as due now (exec = tick), like FUN_180023ee0 files a peer's.
fn inject(m: &mut Mem, c: &Chore) {
    let from = m.i32(SCAN_FROM).clamp(0, SLOTS - 1);
    let Some(i) = (from..SLOTS).find(|&i| !pending(m, i)) else { eprintln!("replay: no free chore slot"); return };
    let s = slot(i);
    m.fill(s + 0x11, SLOT_PAYLOAD, 0);
    m.set_i32(s, m.i32(TICK_COUNTER));
    m.set_i32(s + 4, c.player as i32);
    m.set_u8(s + 8, c.ty());
    m.set_u8(s + 9, 1);
    m.set_i32(s + 0xc, c.id());
    m.set_u8(s + 0x10, 0);
    m.slice_mut(s + 0x11, c.payload().len()).copy_from_slice(c.payload());
}

// ---------------------------------------------------------------- state

/// The whole sim memory, lz4 (~13 MB, ~0.2 s).
pub fn snapshot(m: &Mem) -> Vec<u8> { lz4_flex::block::compress_prepend_size(m.slice(mem::BASE, (mem::END - mem::BASE) as usize)) }
pub fn restore(snap: &[u8]) -> Mem {
    let mut m = Mem::default();
    lz4_flex::block::decompress_into(&snap[4..], m.raw_mut()).expect("replay snapshot");
    m
}

/// Hash of every save section (the state a desync would show in); 80 MB, on 8 threads.
pub fn state_hash(m: &Mem) -> u64 {
    let secs = sections();
    let one = |&(at, n): &(u64, usize)| {
        let mut s = 0x9e3779b97f4a7c15u64 ^ n as u64 ^ at;
        let mut w = m.slice(at, n).chunks_exact(8);
        for c in &mut w { s = (s ^ u64::from_le_bytes(c.try_into().unwrap())).wrapping_mul(0xff51afd7ed558ccd).rotate_left(31); }
        for &x in w.remainder() { s = (s ^ x as u64).wrapping_mul(0xc4ceb9fe1a85ec53); }
        s
    };
    let parts: Vec<Vec<u64>> = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..8).map(|k| sc.spawn(move || secs.iter().skip(k).step_by(8).map(one).collect())).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    (0..secs.len()).fold(0xcbf29ce484222325u64, |h, i| (h ^ parts[i % 8][i / 8]).wrapping_mul(0x100000001b3))
}

// ---------------------------------------------------------------- the stream

/// One RunTick call.
#[derive(Clone, Debug, Default)]
pub struct Frame { pub inp: Vec<i32>, pub chores: Vec<Chore>, pub hash: Option<u64>, pub patches: Vec<(u32, Vec<u8>)> }

impl Frame {
    pub fn steps(&self) -> bool { let mut i = TickInput::default(); unpack(&self.inp, &mut i); steps(&i) }
}

/// Append a frame: flags (1 hash, 2 inputs changed, 4 patches), changed-field mask + values, chores, patches
/// (`[u32 offset from mem::BASE][u16 n][n bytes]`), hash.
fn encode(out: &mut Vec<u8>, prev: &[i32], f: &Frame) {
    let mask = f.inp.iter().zip(prev).enumerate().fold(0u64, |m, (k, (a, b))| if a != b { m | 1 << k } else { m });
    out.push(f.hash.is_some() as u8 | ((mask != 0) as u8) << 1 | (!f.patches.is_empty() as u8) << 2);
    if mask != 0 {
        out.extend_from_slice(&mask.to_le_bytes());
        for (k, v) in f.inp.iter().enumerate() { if mask >> k & 1 != 0 { out.extend_from_slice(&v.to_le_bytes()); } }
    }
    out.extend_from_slice(&(f.chores.len() as u16).to_le_bytes());
    for c in &f.chores {
        out.push(c.player);
        out.extend_from_slice(&(c.packet.len() as u16).to_le_bytes());
        out.extend_from_slice(&c.packet);
    }
    if !f.patches.is_empty() {
        out.extend_from_slice(&(f.patches.len() as u32).to_le_bytes());
        for (at, b) in &f.patches {
            out.extend_from_slice(&at.to_le_bytes());
            out.extend_from_slice(&(b.len() as u16).to_le_bytes());
            out.extend_from_slice(b);
        }
    }
    if let Some(h) = f.hash { out.extend_from_slice(&h.to_le_bytes()); }
}

fn decode(b: &[u8]) -> Option<Vec<Frame>> {
    let mut r = Rd(b, 0);
    let (mut prev, mut out) = (vec![0; FIELDS.len()], vec![]);
    while r.1 < b.len() {
        let flags = r.u8()?;
        let mut inp = prev.clone();
        if flags & 2 != 0 {
            let mask = u64::from_le_bytes(r.take(8)?.try_into().ok()?);
            for (k, v) in inp.iter_mut().enumerate() { if mask >> k & 1 != 0 { *v = r.i32()?; } }
        }
        let n = u16::from_le_bytes(r.take(2)?.try_into().ok()?);
        let chores = (0..n).map(|_| {
            let player = r.u8()?;
            let len = u16::from_le_bytes(r.take(2)?.try_into().ok()?) as usize;
            Some(Chore { player, packet: r.take(len)?.to_vec() })
        }).collect::<Option<Vec<_>>>()?;
        let mut patches = vec![];
        if flags & 4 != 0 {
            for _ in 0..r.i32()? {
                let at = r.i32()? as u32;
                let n = u16::from_le_bytes(r.take(2)?.try_into().ok()?) as usize;
                patches.push((at, r.take(n)?.to_vec()));
            }
        }
        let hash = if flags & 1 != 0 { Some(u64::from_le_bytes(r.take(8)?.try_into().ok()?)) } else { None };
        prev = inp.clone();
        out.push(Frame { inp, chores, hash, patches });
    }
    Some(out)
}

struct Rd<'a>(&'a [u8], usize);
impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> { let s = self.0.get(self.1..self.1 + n)?; self.1 += n; Some(s) }
    fn u8(&mut self) -> Option<u8> { Some(self.take(1)?[0]) }
    fn i32(&mut self) -> Option<i32> { Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?)) }
    fn blob(&mut self) -> Option<&'a [u8]> { let n = u32::from_le_bytes(self.take(4)?.try_into().ok()?) as usize; self.take(n) }
}

// ---------------------------------------------------------------- recording

/// Records a live game from its first RunTick on.
#[derive(Default)]
pub struct Recorder {
    start: Option<Start>, // snapshot before the first RunTick
    start_tick: i32,
    buf: Vec<u8>,     // encoded frames
    prev: Vec<i32>,   // the last frame's inputs (delta encoding)
    frames: usize,
    pending: Vec<Chore>,
    t0: Option<std::time::Instant>,
    pub hash_all: bool, // test: a section hash after every frame
    // the pages the front end ever wrote between ticks, as of the last tap call (ponytail: never shrinks; a few
    // dozen pages: build tool preview, panels, player records)
    shadow: std::collections::HashMap<u32, Vec<u8>>,
    patches: Vec<(u32, Vec<u8>)>,
    steps: u64, // stepping ticks recorded (autosave)
}

/// The start snapshot: a copy of the memory (~60 ms) compressed on a worker (~200 ms), joined when a replay is made.
enum Start { Pending(std::thread::JoinHandle<Vec<u8>>), Done(Arc<Vec<u8>>) }

/// The save sections as (address, size), by address (merged where they overlap: a few alias each other).
fn sections() -> &'static [(u64, usize)] {
    static S: std::sync::OnceLock<Vec<(u64, usize)>> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        let mut v: Vec<(u64, u64)> = shc_sim::world::section_slots().into_iter().map(|(_, a, n)| (a, a + n as u64)).collect();
        v.sort();
        let mut out: Vec<(u64, u64)> = vec![];
        for (a, b) in v { match out.last_mut() { Some(l) if a <= l.1 => l.1 = l.1.max(b), _ => out.push((a, b)) } }
        out.into_iter().map(|(a, b)| (a, (b - a) as usize)).collect()
    })
}

/// The pages a dirty bitmap (`Mem::track_writes`) marks.
fn pages(dirty: &[u64]) -> impl Iterator<Item = u32> + '_ {
    dirty.iter().enumerate().flat_map(|(w, &bits)| (0..64).filter(move |b| bits >> b & 1 != 0).map(move |b| (w * 64 + b) as u32))
}
fn page(m: &Mem, p: u32) -> &[u8] {
    let at = mem::BASE + p as u64 * mem::PAGE;
    m.slice(at, (mem::PAGE as usize).min((mem::END - at) as usize))
}

impl Recorder {
    fn before(&mut self, m: &mut Mem) {
        if self.start.is_none() {
            let c = m.clone();
            self.start = Some(Start::Pending(std::thread::spawn(move || snapshot(&c))));
            (self.start_tick, self.prev, self.t0) = (m.i32(TICK_COUNTER), vec![0; FIELDS.len()], Some(std::time::Instant::now()));
        }
        // the front end's writes since the last tick (GameAction, MapAction, TroopSelection, rotation, ...): the
        // changed bytes of a page seen before, else the whole page
        for p in pages(&m.track_writes(false)) {
            let (now, at) = (page(m, p), p * mem::PAGE as u32);
            match self.shadow.get_mut(&p) {
                None => { self.patches.push((at, now.to_vec())); self.shadow.insert(p, now.to_vec()); }
                Some(old) => {
                    let mut i = 0;
                    while i < now.len() {
                        if now[i] == old[i] { i += 1; continue; }
                        let mut j = i + 1; // a run ends at 8 equal bytes
                        while j < now.len() && now[j..now.len().min(j + 8)] != old[j..now.len().min(j + 8)] { j += 1; }
                        self.patches.push((at + i as u32, now[i..j].to_vec()));
                        i = j;
                    }
                    old.copy_from_slice(now);
                }
            }
        }
        self.pending = before_record(m);
    }
    fn after(&mut self, m: &mut Mem, inp: &TickInput) {
        for (&p, old) in &mut self.shadow { old.copy_from_slice(page(m, p)); } // the tick's own writes
        m.track_writes(true); // the front end's, until the next tick
        let hash = (self.hash_all || steps(inp) && m.i32(TICK_COUNTER) % HASH_EVERY == 0).then(|| state_hash(m));
        let f = Frame { inp: pack(inp), chores: ran(m, &std::mem::take(&mut self.pending)), hash, patches: std::mem::take(&mut self.patches) };
        encode(&mut self.buf, &self.prev, &f);
        self.prev = f.inp;
        self.frames += 1;
        // autosave rotation (saves.rs), on a copy in the background; never from tests (saves_dir is the user's folder)
        if steps(inp) && !cfg!(test) {
            self.steps += 1;
            if self.steps % crate::saves::autosave_ticks() as u64 == 0 {
                crate::saves::save_async(m, crate::saves::autosave_path(), self.t0.map_or(0, |t| t.elapsed().as_millis() as u32), |_| {});
            }
        }
    }

    /// Everything recorded so far, as a file (`Replay::write`).
    pub fn replay(&mut self, m: &Mem) -> Option<Replay> {
        if let Some(Start::Pending(_)) = self.start {
            let Some(Start::Pending(h)) = self.start.take() else { unreachable!() };
            self.start = Some(Start::Done(Arc::new(h.join().ok()?)));
        }
        let Some(Start::Done(start)) = &self.start else { return None };
        Some(Replay {
            start: start.clone(), frames: Arc::new(decode(&self.buf)?), start_tick: self.start_tick,
            map: map_name(m), play_ms: self.t0.map_or(0, |t| t.elapsed().as_millis() as u64), preview: vec![],
        })
    }
}

/// The map's file name (save header block 4, 0x183665f70; set at every map load).
pub fn map_name(m: &Mem) -> String {
    let n = m.i32(0x183666164).clamp(0, 260) as usize;
    let s = String::from_utf8_lossy(m.slice(0x183665f70, n)).trim_end_matches('\0').to_string();
    std::path::Path::new(&s).file_stem().map_or(s.clone(), |x| x.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------- the file

#[derive(Clone)]
pub struct Replay {
    pub start: Arc<Vec<u8>>,
    pub frames: Arc<Vec<Frame>>,
    pub start_tick: i32,
    pub map: String,
    pub play_ms: u64,
    pub preview: Vec<u8>, // packed save header block 0 (palette + 200×200 indices), empty = none
}

impl Replay {
    pub fn ticks(&self) -> usize { self.frames.iter().filter(|f| f.steps()).count() }

    /// `SHCREPLAY1\n`, then u32-length blobs: info (`key=value` lines), preview, start (lz4), frames (lz4).
    pub fn write(&self, path: &std::path::Path) -> std::io::Result<()> {
        let (mut buf, mut prev) = (vec![], vec![0; FIELDS.len()]);
        for f in self.frames.iter() { encode(&mut buf, &prev, f); prev = f.inp.clone(); }
        let info = format!("map={}\nstart_tick={}\nticks={}\nframes={}\nplay_ms={}\n", self.map, self.start_tick, self.ticks(), self.frames.len(), self.play_ms);
        let mut out = MAGIC.to_vec();
        for b in [info.as_bytes(), &self.preview, &self.start, &lz4_flex::block::compress_prepend_size(&buf)] {
            out.extend_from_slice(&(b.len() as u32).to_le_bytes());
            out.extend_from_slice(b);
        }
        if let Some(d) = path.parent() { std::fs::create_dir_all(d)?; }
        std::fs::write(path, out)
    }

    pub fn read(path: &std::path::Path) -> Result<Self, String> {
        let b = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let bad = || format!("{}: not a replay", path.display());
        let mut r = Rd(b.strip_prefix(MAGIC).ok_or_else(bad)?, 0);
        let (info, preview, start, frames) = (r.blob().ok_or_else(bad)?, r.blob().ok_or_else(bad)?, r.blob().ok_or_else(bad)?, r.blob().ok_or_else(bad)?);
        let info = Info::parse(info);
        let frames = lz4_flex::block::decompress_size_prepended(frames).ok().and_then(|f| decode(&f)).ok_or_else(bad)?;
        Ok(Replay { start: Arc::new(start.to_vec()), frames: Arc::new(frames), start_tick: info.start_tick, map: info.map, play_ms: info.play_ms, preview: preview.to_vec() })
    }
}

/// A replay file's info block and preview, without decoding the game (save browser).
#[derive(Clone, Debug, Default)]
pub struct Info { pub map: String, pub start_tick: i32, pub ticks: i32, pub play_ms: u64, pub preview: Vec<u8> }

impl Info {
    fn parse(b: &[u8]) -> Self {
        let mut i = Info::default();
        for l in String::from_utf8_lossy(b).lines() {
            let Some((k, v)) = l.split_once('=') else { continue };
            match k { "map" => i.map = v.into(), "start_tick" => i.start_tick = v.parse().unwrap_or(0), "ticks" => i.ticks = v.parse().unwrap_or(0),
                "play_ms" => i.play_ms = v.parse().unwrap_or(0), _ => {} }
        }
        i
    }
    pub fn read(path: &std::path::Path) -> Option<Self> {
        use std::io::Read;
        let mut f = std::fs::File::open(path).ok()?;
        let mut head = vec![0; 1 << 16]; // info + preview fit easily
        let n = f.read(&mut head).ok()?;
        let mut r = Rd(head[..n].strip_prefix(MAGIC)?, 0);
        let mut i = Info::parse(r.blob()?);
        i.preview = r.blob().map(<[u8]>::to_vec).unwrap_or_default();
        Some(i)
    }
}

// ---------------------------------------------------------------- playback

/// Plays a replay through `Live`'s RunTick. `at` = the next frame. Snapshots are shared with the UI (seeking makes a
/// new `Live` from one, `seek`).
pub struct Player {
    pub replay: Replay,
    pub at: usize,
    pub snaps: Arc<Mutex<Vec<(usize, Arc<Vec<u8>>)>>>,
    pub pos: Arc<AtomicUsize>, // `at`, for the UI
    pub desync: Option<i32>, // first tick whose section hash differs from the recording's
    pub check_every_tick: Option<Vec<u64>>, // test: the original run's hash after every frame
    primed: bool,             // the viewer's first (no-step) draw happened
    stash: Option<Mem>,       // the state before that draw, put back after it
    centre: (i32, i32),       // the recorded camera centre last frame
    follow: Option<(i32, i32)>,
}

use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

impl Player {
    pub fn new(replay: Replay) -> Self {
        let snaps = Arc::new(Mutex::new(vec![(0, replay.start.clone())]));
        Self::at(replay, 0, snaps, Default::default())
    }
    fn at(replay: Replay, at: usize, snaps: Arc<Mutex<Vec<(usize, Arc<Vec<u8>>)>>>, pos: Arc<AtomicUsize>) -> Self {
        pos.store(at, Relaxed);
        Player { replay, at, snaps, pos, desync: None, check_every_tick: None, primed: false, stash: None, centre: (-1, -1), follow: None }
    }
    pub fn done(&self) -> bool { self.at >= self.replay.frames.len() }

    /// A viewer RunTick without the step (`no_step`: the first draw of a new Live, paused redraws) runs no recorded
    /// frame: the first one draws the whole map from scratch on the state, which `after` then puts back (a draw pass
    /// changes building heights, see below); later ones are skipped. A stepping one plays the next frame: the recorded
    /// input over `inp`, the draw rect too (the engine's draw pass raises the height +0xec of some buildings it draws,
    /// draw.rs / FUN_180041150, and units spawn at it: the camera is a sim input), the recorded writes, the front
    /// end's own chores dropped, the recorded ones filed. False = no RunTick (skipped, or the replay is over).
    fn before(&mut self, m: &mut Mem, inp: &mut TickInput) -> bool {
        if inp.no_step {
            if std::mem::replace(&mut self.primed, true) { return false; }
            self.stash = Some(m.clone());
            shc_sim::draw::reset(m);
            (inp.scr_left, inp.scr_right, inp.scr_top, inp.scr_bottom) = (0, 400, 0, 800);
            return true;
        }
        let Some(f) = self.replay.frames.get(self.at) else { return false };
        if self.at % SNAP_EVERY == 0 && self.at > 0 {
            let mut s = self.snaps.lock().unwrap();
            if !s.iter().any(|x| x.0 == self.at) { s.push((self.at, Arc::new(snapshot(m)))); s.sort_by_key(|x| x.0); }
        }
        unpack(&f.inp, inp);
        for (at, b) in &f.patches { m.slice_mut(mem::BASE + *at as u64, b.len()).copy_from_slice(b); }
        purge(m);
        for c in &f.chores { inject(m, c); }
        if (inp.centre_tile_x, inp.centre_tile_y) != self.centre {
            self.centre = (inp.centre_tile_x, inp.centre_tile_y);
            self.follow = Some(self.centre);
        }
        true
    }
    fn after(&mut self, m: &mut Mem) {
        if let Some(s) = self.stash.take() { *m = s; return; }
        purge(m);
        let f = &self.replay.frames[self.at];
        let want = self.check_every_tick.as_ref().and_then(|v| v.get(self.at).copied()).or(f.hash);
        if let (Some(h), None) = (want, self.desync) {
            if state_hash(m) != h {
                self.desync = Some(m.i32(TICK_COUNTER));
                eprintln!("replay: desync at tick {} (frame {})", m.i32(TICK_COUNTER), self.at);
            }
        }
        self.at += 1;
        self.pos.store(self.at, Relaxed);
    }

    /// The memory at frame `to`: the nearest snapshot at or before it, then the frames in between headless. The new
    /// player reports its position to `pos`.
    pub fn seek(&self, to: usize, pos: Arc<AtomicUsize>) -> (Mem, Player) {
        let to = to.min(self.replay.frames.len());
        let (at, snap) = self.snaps.lock().unwrap().iter().rev().find(|s| s.0 <= to).cloned().unwrap_or((0, self.replay.start.clone()));
        let mut m = restore(&snap);
        let mut p = Player::at(self.replay.clone(), at, self.snaps.clone(), pos);
        let mut ret = vec![0; shc_sim::hudstate::RET_LEN];
        while p.at < to { p.run(&mut m, &mut ret); }
        p.follow = None;
        (m, p)
    }

    /// One frame without a viewer (seeking, tests).
    pub fn run(&mut self, m: &mut Mem, ret: &mut [u8]) -> bool {
        let mut inp = TickInput::default();
        if !self.before(m, &mut inp) { return false; }
        shc_sim::frontend::run_tick(m, &inp, ret, None);
        self.after(m);
        true
    }
}

// ---------------------------------------------------------------- Live's hook

/// What `Live` runs around every DLL_RunTick: recording a game, or playing one back.
pub enum Tap { Rec(Recorder), Play(Player) }

impl Tap {
    pub fn record() -> Self { Tap::Rec(Recorder::default()) }
    /// Before RunTick (may change `inp`). False = no RunTick (a replay's skipped redraw, or its end).
    pub fn before(&mut self, m: &mut Mem, inp: &mut TickInput) -> bool {
        match self { Tap::Rec(r) => { r.before(m); true } Tap::Play(p) => p.before(m, inp) }
    }
    pub fn after(&mut self, m: &mut Mem, inp: &TickInput) {
        match self { Tap::Rec(r) => r.after(m, inp), Tap::Play(p) => p.after(m) }
    }
    pub fn playing(&self) -> bool { matches!(self, Tap::Play(_)) }
    /// A replay's camera: the game tile the recorded view centred on, when it moved (Frame::goto).
    pub fn follow(&mut self) -> Option<(i32, i32)> { if let Tap::Play(p) = self { p.follow.take() } else { None } }
}

/// The sim thread's Cmd::SaveReplay: the game so far as a replay file (Saves/Replays/<map>-<time>.shcreplay), with
/// the save preview of the current state (save header block 0), written on a worker thread.
pub fn save_replay(l: &mut crate::live::Live) {
    let Some(mut tap) = l.tap.take() else { return };
    if let Tap::Rec(r) = &mut tap {
        let c = l.mem().clone();
        let path = crate::saves::replay_path(&map_name(&c));
        if let Some(rep) = r.replay(&c) { std::thread::spawn(move || write_with_preview(rep, c, &path)); }
    }
    l.tap = Some(tap);
}

/// `rep` into `path` with the preview the save writer makes of `m` (header block 0: `[-2][u32 n][n bytes]`).
fn write_with_preview(mut rep: Replay, mut m: Mem, path: &std::path::Path) {
    let sav = shc_sim::save::save_game(&mut m, 0);
    let n = u32::from_le_bytes(sav[4..8].try_into().unwrap()) as usize;
    rep.preview = sav.get(8..8 + n).map(<[u8]>::to_vec).unwrap_or_default();
    match rep.write(path) { Ok(()) => println!("replay saved: {}", path.display()), Err(e) => eprintln!("replay {}: {e}", path.display()) }
}

/// A replay file ready to play: a `Live` on its start state, and the controls (with the shared position).
pub fn open(path: &std::path::Path) -> Result<(crate::live::Live, Controls), String> {
    let p = Player::new(Replay::read(path)?);
    let c = Controls::new(&p);
    let mut l = crate::live::Live::from_mem(restore(&p.replay.start));
    l.tap = Some(Tap::Play(p));
    Ok((l, c))
}

// ---------------------------------------------------------------- the replay bar

#[derive(Clone, Copy, PartialEq)]
enum Btn { Back, Toggle, Fwd, Speed(f32), Bar }

/// The replay bar over a playing replay (top centre): back / play-pause / forward, speed, timeline (click = seek),
/// time. A jump forward plays on at full speed (no state to restore); a jump back (or far ahead of a snapshot) builds
/// the state from the nearest snapshot on a worker thread, and `poll` hands over the new `Live`.
pub struct Controls {
    replay: Replay,
    snaps: Arc<Mutex<Vec<(usize, Arc<Vec<u8>>)>>>,
    pos: Arc<AtomicUsize>,
    steps: Vec<u32>, // stepping ticks before each frame (the timeline is in ticks)
    seeking: Option<(usize, Arc<AtomicUsize>, std::thread::JoinHandle<crate::live::Live>)>,
    hits: Vec<([f32; 4], Btn)>,
    base: f32, // the normal game speed (settings.cfg GameSpeed): 1x
    ff: Option<(usize, f32)>, // fast-forwarding to this frame, then back to this rate
}

impl Controls {
    fn new(p: &Player) -> Self {
        let mut steps = vec![0u32];
        for f in p.replay.frames.iter() { steps.push(steps.last().unwrap() + f.steps() as u32); }
        Controls { replay: p.replay.clone(), snaps: p.snaps.clone(), pos: p.pos.clone(), steps, seeking: None, hits: vec![], base: crate::skirmish::Settings::load().game_speed(), ff: None }
    }

    fn ticks(&self) -> u32 { *self.steps.last().unwrap() }
    /// The frame where tick `t` starts.
    fn frame_of(&self, t: u32) -> usize { self.steps.partition_point(|&s| s < t).min(self.replay.frames.len()) }

    /// Seek to frame `to`: ahead and no snapshot nearer than the current frame = play on at full speed; else on a
    /// worker thread from the nearest snapshot (`poll` picks the result up).
    fn seek(&mut self, to: usize, rate: &mut f32) {
        if self.seeking.is_some() { return; }
        let now = self.pos.load(Relaxed);
        let snap = self.snaps.lock().unwrap().iter().rev().find(|s| s.0 <= to).map_or(0, |s| s.0);
        if to > now && snap <= now + SNAP_EVERY / 4 {
            self.ff = Some((to, self.ff.map_or(*rate, |f| f.1)));
            *rate = crate::speed::MAX;
            return;
        }
        if let Some((_, r)) = self.ff.take() { *rate = r; }
        let p = Player::at(self.replay.clone(), 0, self.snaps.clone(), Default::default());
        let pos = Arc::new(AtomicUsize::new(to));
        let pos2 = pos.clone();
        self.seeking = Some((to, pos, std::thread::spawn(move || {
            let t = std::time::Instant::now();
            let (m, p) = p.seek(to, pos2);
            println!("replay: seek to frame {to} in {:.2} s", t.elapsed().as_secs_f32());
            let mut l = crate::live::Live::from_mem(m);
            l.tap = Some(Tap::Play(p));
            l
        })));
    }
    /// A finished seek: the `Live` to run instead (main.rs: start_live).
    pub fn poll(&mut self) -> Option<crate::live::Live> {
        if !self.seeking.as_ref()?.2.is_finished() { return None; }
        let (_, pos, h) = self.seeking.take()?;
        self.pos = pos;
        h.join().ok()
    }

    pub fn over(&self, c: [f32; 2]) -> bool { self.hits.iter().any(|(r, _)| inside(*r, c)) }

    /// Left press. `playing` / `rate` = the game's (play.playing / play.rate).
    pub fn click(&mut self, c: [f32; 2], playing: &mut bool, rate: &mut f32) {
        let base = self.base;
        let Some(&(r, b)) = self.hits.iter().find(|(r, _)| inside(*r, c)) else { return };
        let at = self.steps[self.pos.load(Relaxed).min(self.steps.len() - 1)];
        let jump = 60 * base as u32; // a minute at normal speed
        match b {
            Btn::Toggle => *playing = !*playing,
            Btn::Speed(k) => (*rate, *playing, self.ff) = (base * k, true, None),
            Btn::Back => self.seek(self.frame_of(at.saturating_sub(jump)), rate),
            Btn::Fwd => self.seek(self.frame_of(at + jump), rate),
            Btn::Bar => self.seek(self.frame_of((((c[0] - r[0]) / (r[2] - r[0])).clamp(0.0, 1.0) * self.ticks() as f32) as u32), rate),
        }
    }

    /// Once per frame: a fast-forward that got there ends (`rate` back to the one before it).
    pub fn update(&mut self, rate: &mut f32) {
        if let Some((to, r)) = self.ff { if self.pos.load(Relaxed) >= to { (*rate, self.ff) = (r, None); } }
    }

    /// The bar's triangles (window logical px `w` × `h`).
    pub fn draw(&mut self, ui: &mut crate::hud::Ui, w: f32, h: f32, cursor: [f32; 2], playing: bool, rate: f32) -> Vec<crate::hud::HudTri> {
        use crate::hud::Ui;
        let base = self.base;
        let s = (w / 1920.0).min(h / 1080.0);
        (ui.s, ui.w, ui.h) = (s, w, h);
        self.hits.clear();
        let mut out = vec![];
        let (bw, bh) = (1100.0 * s, 64.0 * s);
        let (x0, y0) = ((w - bw) / 2.0, 10.0 * s);
        Ui::fill(&mut out, [x0 - 2.0 * s, y0 - 2.0 * s, x0 + bw + 2.0 * s, y0 + bh + 2.0 * s], [0.08, 0.05, 0.02, 0.92]);
        ui.img(&mut out, "Panel_BG", [x0, y0, x0 + bw, y0 + bh], [1.0, 1.0, 1.0, 0.95]);
        const CREAM: [f32; 4] = [0.863, 0.896, 0.565, 1.0];
        const GOLD: [f32; 4] = [1.0, 0.672, 0.171, 1.0];
        let mut x = x0 + 12.0 * s;
        let mut button = |ui: &mut Ui, out: &mut Vec<_>, x: &mut f32, wd: f32, label: &str, b: Btn, on: bool| {
            let r = [*x, y0 + 12.0 * s, *x + wd * s, y0 + 52.0 * s];
            let hot = inside(r, cursor);
            Ui::fill(out, r, if hot { [0.55, 0.42, 0.18, 0.95] } else if on { [0.42, 0.3, 0.12, 0.95] } else { [0.18, 0.13, 0.08, 0.9] });
            Ui::fill(out, [r[0] + 2.0 * s, r[1] + 2.0 * s, r[2] - 2.0 * s, r[3] - 2.0 * s], if on { [0.36, 0.26, 0.12, 0.95] } else { [0.27, 0.2, 0.12, 0.95] });
            ui.text_sh(out, label, (r[0] + r[2]) / 2.0, r[1] + 28.0 * s, 20.0, 0.5, if hot || on { GOLD } else { CREAM });
            self.hits.push((r, b));
            *x += (wd + 8.0) * s;
        };
        button(ui, &mut out, &mut x, 64.0, "-1:00", Btn::Back, false);
        button(ui, &mut out, &mut x, 84.0, if playing { "Pause" } else { "Play" }, Btn::Toggle, !playing);
        button(ui, &mut out, &mut x, 64.0, "+1:00", Btn::Fwd, false);
        x += 10.0 * s;
        for (k, label) in [(1.0, "1x"), (2.0, "2x"), (4.0, "4x"), (8.0, "8x"), (crate::speed::MAX / base, "Max")] {
            button(ui, &mut out, &mut x, if label == "Max" { 56.0 } else { 42.0 }, label, Btn::Speed(k), playing && self.ff.is_none() && (rate - base * k).abs() < 0.5);
        }
        // timeline: played part, snapshot marks (instant seeks back), the seek target
        let (at, total) = (self.steps[self.pos.load(Relaxed).min(self.steps.len() - 1)], self.ticks().max(1));
        let time = format!("{} / {}", crate::saves::hms(at as u64 * 1000 / base as u64), crate::saves::hms(total as u64 * 1000 / base as u64));
        let tw = 190.0 * s;
        let r = [x + 6.0 * s, y0 + 24.0 * s, x0 + bw - tw - 16.0 * s, y0 + 40.0 * s];
        Ui::fill(&mut out, r, [0.12, 0.08, 0.04, 0.9]);
        let fx = |t: u32| r[0] + (r[2] - r[0]) * t as f32 / total as f32;
        Ui::fill(&mut out, [r[0], r[1], fx(at), r[3]], [0.75, 0.55, 0.2, 0.95]);
        for &(f, _) in self.snaps.lock().unwrap().iter() {
            let sx = fx(self.steps[f.min(self.steps.len() - 1)]);
            Ui::fill(&mut out, [sx - 1.0 * s, r[1] - 4.0 * s, sx + 1.0 * s, r[1]], CREAM);
        }
        if let Some(f) = self.seeking.as_ref().map(|s| s.0).or(self.ff.map(|f| f.0)) {
            let sx = fx(self.steps[f.min(self.steps.len() - 1)]);
            Ui::fill(&mut out, [sx - 2.0 * s, r[1] - 6.0 * s, sx + 2.0 * s, r[3] + 6.0 * s], GOLD);
        }
        if inside([r[0], r[1] - 10.0 * s, r[2], r[3] + 10.0 * s], cursor) { Ui::fill(&mut out, [cursor[0] - 1.0 * s, r[1] - 6.0 * s, cursor[0] + 1.0 * s, r[3] + 6.0 * s], [1.0, 1.0, 1.0, 0.7]); }
        self.hits.push(([r[0], r[1] - 10.0 * s, r[2], r[3] + 10.0 * s], Btn::Bar));
        let label = if self.seeking.is_some() || self.ff.is_some() { "Seeking...".to_string() } else { time };
        ui.text_sh(&mut out, &label, x0 + bw - 14.0 * s, y0 + 30.0 * s, 20.0, 1.0, CREAM);
        ui.text_sh(&mut out, &format!("Replay: {}", self.replay.map), x0 + bw - 14.0 * s, y0 + 54.0 * s, 14.0, 1.0, [0.8, 0.75, 0.6, 1.0]);
        out
    }
}

fn inside(r: [f32; 4], c: [f32; 2]) -> bool { c[0] >= r[0] && c[0] < r[2] && c[1] >= r[1] && c[1] < r[3] }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::Live;

    /// Per-section hashes (to name the sections a desync is in).
    fn sections(m: &Mem) -> Vec<(i32, u64)> {
        shc_sim::world::section_slots().into_iter().map(|(id, at, n)| (id, m.slice(at, n).iter().fold(0u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3)))).collect()
    }

    /// A scripted skirmish (Crater Lake vs the Rat, seed 11) the way the viewer drives it: build tool hover every tick,
    /// hovels clicked near the keep, a wall drag, a delete, tax / rations (GameAction and the direct path), a map
    /// rotation, pause + paused frames. Returns the Live after `ticks` stepping ticks.
    fn scripted(ticks: i32, hash_all: bool) -> Option<Live> { scripted_at(ticks, hash_all, usize::MAX).map(|x| x.0) }

    /// `scripted`, plus a copy of the memory after frame `keep` (desync hunting).
    fn scripted_at(ticks: i32, hash_all: bool, keep: usize) -> Option<(Live, Option<Mem>)> {
        let mut kept = None;
        let map = crate::skirmish::maps_dir().join("Crater Lake.map");
        if !map.exists() { return None; } // needs the game install
        let lobby = crate::skirmish::Lobby { ais: vec![(0, 0)], seed: 11, ..crate::skirmish::Lobby::new(map) };
        let mut l = Live::from_mem(crate::skirmish::start(&lobby).unwrap());
        if let Some(Tap::Rec(r)) = &mut l.tap { r.hash_all = hash_all; }
        let (kx, ky) = l.keep().map(|(x, y)| (x as i32, y as i32)).unwrap();
        // the camera a 1684×784 window on the keep sends (main.rs Cmd::View: centre tile + world px rect)
        let (wx, wy) = crate::replay::to_west_pub(l.rotation(), kx as i16, ky as i16);
        let [px, py] = crate::replay::cell_px(wx, wy);
        (l.view, l.centre) = (Some(crate::live::camera([px - 842.0, py - 392.0, px + 842.0, py + 392.0])), [kx, ky, 26, 24]);
        l.records(); // start_live's first draw (no step)
        let mut placed = 0;
        for k in 0..ticks {
            l.mouse = [kx + k % 7, ky - k % 5];
            match k % 400 {
                50 if placed < 8 => {
                    l.set_tool(54); // hovel
                    let spot = (-25..=25).flat_map(|d| (-25..=25).map(move |e| (kx + d, ky + e))).find(|&(x, y)| l.hover(54, x, y).ok);
                    if let Some((x, y)) = spot { if l.click(54, x, y).1 > 0 { placed += 1; } }
                }
                120 => { l.set_tool(0); l.clear_preview(); }
                200 => { l.game_action(1004, (k / 400) % 6, (k / 400) % 6); l.game_action(1005, 2, 2); }
                260 if k / 400 == 3 => { l.set_tax(3); l.set_rations(1); }
                300 if k / 400 == 2 => { l.set_paused(true); l.records(); l.records(); l.set_paused(false); }
                320 if k / 400 == 4 => l.rotate((l.rotation() as i32 + 2) % 8, kx, ky),
                330 if k / 400 == 5 => {
                    use shc_sim::buildings as b;
                    if let Some(id) = (1..b::SLOTS).find(|&i| b::state(l.mem_ref(), i) == 2 && b::owner(l.mem_ref(), i) == 1 && b::kind(l.mem_ref(), i) == 0xa) { l.delete_building(id); }
                }
                340 if k / 400 == 6 => {
                    if l.set_tool(25) {
                        let (x, y) = (kx + 8, ky - 6);
                        l.drag_input(25, 0, x, y);
                        l.drag_input(25, 1, x, y);
                        for d in 1..=5 { l.drag_input(25, 2, x, y + d); }
                        l.drag_input(25, 3, x, y + 5);
                    }
                    l.set_tool(0);
                }
                _ => {}
            }
            if (50..120).contains(&(k % 400)) && placed < 8 { l.hover(54, kx + 3, ky + 3); } // the held tool's hover, repeated
            l.step();
            if let Some(Tap::Rec(r)) = &l.tap { if r.frames == keep + 1 { kept = Some(l.mem_ref().clone()); } }
        }
        println!("scripted: {ticks} ticks, {placed} hovels, tick {}", l.tick());
        Some((l, kept))
    }

    /// Playback through `Live` as the viewer runs it: the first (no-step) draw of the whole map on a copy, then
    /// stepping ticks (draws in the recorded rect): every frame matches the original run; the first draw sent the map.
    #[test]
    #[cfg_attr(debug_assertions, ignore = "thousands of ticks + a hash per frame: cargo test --release")]
    fn live_playback_matches() {
        let Some(mut l) = scripted(1300, true) else { return };
        let m = l.mem_ref().clone();
        let Some(Tap::Rec(r)) = &mut l.tap else { panic!() };
        let rep = r.replay(&m).unwrap();
        let want: Vec<u64> = rep.frames.iter().map(|f| f.hash.unwrap()).collect();
        let mut p = Player::new(rep);
        p.check_every_tick = Some(want);
        let mut v = Live::from_mem(restore(&p.replay.start));
        v.tap = Some(Tap::Play(p));
        let first = v.records();
        assert!(first.len() > 50_000, "first draw: {} records", first.len());
        let mut follow = 0;
        loop {
            v.view = Some([0, 10, 0, 10]); // the viewer's own camera: ignored
            v.step();
            follow += v.tap.as_mut().unwrap().follow().is_some() as usize;
            let Some(Tap::Play(p)) = &v.tap else { panic!() };
            assert_eq!(p.desync, None);
            if p.done() { break; }
        }
        assert_eq!(state_hash(v.mem_ref()), state_hash(l.mem_ref()));
        println!("live playback ok, {} camera moves", follow);
    }

    /// SHC_SAVES=<dir> (screenshots): a 3,000-tick replay of the scripted skirmish, a quick save and an autosave there.
    #[test]
    #[ignore]
    fn write_demo_files() {
        if std::env::var_os("SHC_SAVES").is_none() { return; }
        let Some(mut l) = scripted(3000, false) else { return };
        let m = l.mem_ref().clone();
        let Some(Tap::Rec(r)) = &mut l.tap else { panic!() };
        let rep = r.replay(&m).unwrap();
        let path = crate::saves::replay_path(&map_name(l.mem_ref()));
        write_with_preview(rep, l.mem_ref().clone(), &path);
        assert!(Info::read(&path).is_some_and(|i| !i.preview.is_empty() && i.ticks == 3000));
        for p in [crate::saves::quick_path(), crate::saves::autosave_path()] {
            std::fs::write(&p, shc_sim::save::save_game(&mut l.mem().clone(), 95_000)).unwrap();
            assert!(crate::saves::entry(&p).preview.is_some());
        }
    }

    /// SHC_SAVES=<dir> (screenshots): `Ended.sav`, a skirmish vs the Rat just lost (your lord killed), for the end
    /// screen's Save Replay.
    #[test]
    #[ignore]
    fn write_ended_save() {
        if std::env::var_os("SHC_SAVES").is_none() { return; }
        let map = crate::skirmish::maps_dir().join("Crater Lake.map");
        if !map.exists() { return; }
        let mut m = crate::skirmish::start(&crate::skirmish::Lobby { ais: vec![(0, 0)], seed: 1234, ..crate::skirmish::Lobby::new(map) }).unwrap();
        let mut ret = vec![0u8; shc_sim::hudstate::RET_LEN];
        let mut tick = |m: &mut Mem| { shc_sim::frontend::run_tick(m, &TickInput::default(), &mut ret, None); };
        for _ in 0..300 { tick(&mut m); }
        let (me, rat) = (m.i32(shc_sim::game::player(1) + 0x21f8), m.i32(shc_sim::game::player(2) + 0x21f8));
        shc_sim::units::helpers::apply_hit(&mut m, rat, me, 10_000_000);
        for _ in 0..400 { tick(&mut m); }
        assert!(crate::endgame::read(&m).is_some_and(|e| e.result == 2));
        std::fs::write(crate::skirmish::saves_dir().join("Ended.sav"), shc_sim::save::save_game(&mut m, 0)).unwrap();
    }

    /// SHC_REPLAY=<file>: headless playback speed (frames per second; run under `time` for CPU per frame).
    #[test]
    #[ignore]
    fn replay_speed() {
        let Some(f) = std::env::var_os("SHC_REPLAY") else { return };
        let mut p = Player::new(Replay::read(std::path::Path::new(&f)).unwrap());
        let (mut m, mut ret) = (restore(&p.replay.start), vec![0; shc_sim::hudstate::RET_LEN]);
        let t = std::time::Instant::now();
        while p.run(&mut m, &mut ret) {}
        println!("{} frames in {:.2} s: {:.0} frames/s, desync {:?}", p.at, t.elapsed().as_secs_f32(), p.at as f32 / t.elapsed().as_secs_f32(), p.desync);
        assert_eq!(p.desync, None);
    }

    /// SHC_REPLAY_AT=frame: the sections (and first bytes) where playback differs from the original after that frame.
    #[test]
    #[ignore]
    fn replay_diff_at() {
        let at: usize = std::env::var("SHC_REPLAY_AT").ok().and_then(|v| v.parse().ok()).unwrap_or(802);
        let Some((mut l, Some(orig))) = scripted_at(at as i32 + 5, false, at) else { return };
        let m = l.mem_ref().clone();
        let Some(Tap::Rec(r)) = &mut l.tap else { panic!() };
        let mut p = Player::new(r.replay(&m).unwrap());
        let (mut m, mut ret) = (restore(&p.replay.start), vec![0; shc_sim::hudstate::RET_LEN]);
        while p.at <= at { p.run(&mut m, &mut ret); }
        for (id, a, n) in shc_sim::world::section_slots() {
            let (x, y) = (orig.slice(a, n), m.slice(a, n));
            let d: Vec<usize> = (0..n).filter(|&i| x[i] != y[i]).collect();
            if !d.is_empty() { println!("section {id} @{a:#x}: {} bytes differ, first +{:#x}: {:?}", d.len(), d[0], d.iter().take(8).map(|&i| (i, x[i], y[i])).collect::<Vec<_>>()); }
        }
    }

    /// Record 5,000 ticks of a scripted skirmish, write + read the file, play it back headless: every frame's
    /// save-section hash matches the original run. Then seek back into the middle (snapshot + frames) and on.
    #[test]
    #[cfg_attr(debug_assertions, ignore = "thousands of ticks + a hash per frame: cargo test --release")]
    fn replay_matches_original() {
        let t = std::time::Instant::now();
        let Some(mut l) = scripted(5000, true) else { return };
        let m = l.mem_ref().clone();
        let Some(Tap::Rec(r)) = &mut l.tap else { panic!() };
        let rep = r.replay(&m).unwrap();
        let chores: usize = rep.frames.iter().map(|f| f.chores.len()).sum();
        println!("recorded {} frames, {} ticks, {chores} chores in {:?}", rep.frames.len(), rep.ticks(), t.elapsed());
        assert!(chores >= 10, "too few chores recorded: {chores}");
        let path = std::env::temp_dir().join(format!("shc-replay-test-{}.shcreplay", std::process::id()));
        rep.write(&path).unwrap();
        println!("file {} KB (start {} KB)", std::fs::metadata(&path).unwrap().len() >> 10, rep.start.len() >> 10);
        let back = Replay::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(back.frames.len(), rep.frames.len());
        let want: Vec<u64> = rep.frames.iter().map(|f| f.hash.unwrap()).collect();
        let mut p = Player::new(back);
        p.check_every_tick = Some(want.clone());
        let (mut m, mut ret) = (restore(&p.replay.start), vec![0; shc_sim::hudstate::RET_LEN]);
        let t = std::time::Instant::now();
        while p.run(&mut m, &mut ret) { if p.desync.is_some() { break; } }
        println!("played {} frames in {:?}, desync {:?}", p.at, t.elapsed(), p.desync);
        if let Some(tick) = p.desync {
            let k = p.at - 1;
            println!("first differing frame {k} (tick {tick}); chores that frame {:?}", p.replay.frames[k].chores);
            panic!("replay desync at tick {tick}");
        }
        assert_eq!(m.i32(TICK_COUNTER), l.tick(), "same final tick");
        assert_eq!(sections(&m), sections(l.mem_ref()), "final sections");
        // seek back to the middle: snapshot at 2000 + headless frames, then on to the end
        let (mut m2, mut q) = p.seek(3100, Default::default());
        q.check_every_tick = Some(want);
        while q.run(&mut m2, &mut ret) { if q.desync.is_some() { break; } }
        assert_eq!(q.desync, None, "desync after seeking");
        assert_eq!(state_hash(&m2), state_hash(l.mem_ref()));
        println!("{} snapshots", p.snaps.lock().unwrap().len());
    }
}
