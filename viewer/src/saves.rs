//! Saves around the native save writer (ours): the Load screen's browser (preview, map, date, play time from the
//! .sav header without loading it), quick save / load, autosave rotation, and saving on a copy of the sim memory in
//! the background (`save_game` takes ~1-2 s; the game must not stall).
use crate::skirmish::{crossover_saves, list, saves_dir};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Autosave every this many stepping ticks (5 minutes at the default 40 ticks/s); `SHC_AUTOSAVE_TICKS` overrides.
pub fn autosave_ticks() -> i32 { std::env::var("SHC_AUTOSAVE_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(12_000) }
/// Autosaves kept.
pub const AUTOSAVES: usize = 5;

pub fn quick_path() -> PathBuf { saves_dir().join("Quicksave.sav") }
pub fn replays_dir() -> PathBuf { saves_dir().join("Replays") }
pub fn is_replay(p: &Path) -> bool { p.extension().is_some_and(|e| e == "shcreplay") }

fn secs() -> u64 { SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs()) }
/// A file name from a map name (no path separators).
fn clean(s: &str) -> String { s.chars().map(|c| if c.is_alphanumeric() || " -_".contains(c) { c } else { '_' }).collect::<String>().trim().to_string() }

/// The next autosave's path (`Autosave-<unix secs>.sav`); the oldest beyond `AUTOSAVES` - 1 are deleted first.
pub fn autosave_path() -> PathBuf { rotate(&saves_dir(), secs()) }

fn rotate(dir: &Path, now: u64) -> PathBuf {
    // only ours (`Autosave-<unix secs>`): a player's own "Autosave-mine" save is never rotated away
    let time = |p: &PathBuf| p.file_stem().and_then(|s| s.to_str()?.strip_prefix("Autosave-")?.parse::<u64>().ok());
    let mut old: Vec<(u64, PathBuf)> = list(dir, "sav").into_iter().filter_map(|p| Some((time(&p)?, p))).collect();
    old.sort(); // the name carries the time
    for (_, p) in old.iter().rev().skip(AUTOSAVES - 1) { let _ = std::fs::remove_file(p); let _ = std::fs::remove_file(crate::koth::Hill::sidecar(p)); }
    dir.join(format!("Autosave-{now}.sav"))
}

pub fn replay_path(map: &str) -> PathBuf {
    replays_dir().join(format!("{}-{}.shcreplay", if map.is_empty() { "Game".into() } else { clean(map) }, secs()))
}

/// DLL_SaveSaveGame on a copy of the memory, written on a worker thread: the caller only pays the copy (~60 ms).
/// `elapsed_ms` = play time since the game was loaded / started (`save::save_game`). `done` runs with the result.
#[cfg(feature = "live")]
pub fn save_async(m: &shc_sim::mem::Mem, path: PathBuf, elapsed_ms: u32, done: impl FnOnce(std::io::Result<()>) + Send + 'static) {
    let mut c = m.clone();
    PENDING.lock().unwrap().push(path.clone());
    std::thread::spawn(move || {
        let r = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| crate::paths::write_atomic(&path, shc_sim::save::save_game(&mut c, elapsed_ms)))
            .and_then(|_| crate::extended::write_beside(&path, shc_sim::ext::on(&c))); // the rules are not in the save (extended.rs)
        PENDING.lock().unwrap().retain(|p| *p != path);
        match &r { Ok(()) => println!("saved {}", path.display()), Err(e) => eprintln!("save {}: {e}", path.display()) }
        done(r);
    });
}

/// Saves still being written (save_async): the Load list shows them at once ("Saving…"), they can't be picked yet.
static PENDING: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());
pub fn pending(p: &Path) -> bool { PENDING.lock().unwrap().iter().any(|q| q == p) }
/// `v` plus the pending saves with extension `ext` in `dir` it doesn't hold yet.
fn with_pending(mut v: Vec<PathBuf>, dir: &Path, ext: &str) -> Vec<PathBuf> {
    for p in PENDING.lock().unwrap().iter() {
        if p.parent() == Some(dir) && p.extension().is_some_and(|e| e == ext) && !v.contains(p) { v.push(p.clone()); }
    }
    v
}

/// What the Load screen shows for a file.
#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub map: String,
    pub date: Option<SystemTime>,
    pub play_ms: u64,
    pub ticks: i32,                // replays: length
    pub preview: Option<Vec<u8>>,  // 200×200 RGBA
}

impl Entry {
    /// "Quick save" / "Autosave" / "Replay" / "" (a plain save).
    pub fn kind(p: &Path) -> &'static str {
        let n = p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
        if is_replay(p) { "Replay" } else if n.starts_with("Autosave-") { "Autosave" } else if n == "Quicksave.sav" { "Quick save" } else { "" }
    }
    pub fn date_text(&self) -> String {
        let Some(d) = self.date else { return String::new() };
        let s = d.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64) + tz();
        let (days, rem) = (s.div_euclid(86400), s.rem_euclid(86400));
        let (y, mo, da) = civil(days);
        const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        format!("{da} {} {y} {:02}:{:02}", M[(mo - 1) as usize], rem / 3600, rem / 60 % 60)
    }
    pub fn play_text(&self) -> String { hms(self.play_ms) }
}

/// h:mm:ss (or m:ss).
pub fn hms(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

/// The local UTC offset in seconds. ponytail: std has no time zones; `date +%z` once (UTC if that fails).
pub fn tz() -> i64 {
    static T: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *T.get_or_init(|| {
        let o = std::process::Command::new("date").arg("+%z").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        let v: i64 = o.trim_start_matches(['+', '-']).parse().unwrap_or(0);
        (v / 100 * 3600 + v % 100 * 60) * if o.starts_with('-') { -1 } else { 1 }
    })
}

/// Days since 1970-01-01 → (year, month, day) (H. Hinnant's civil_from_days).
fn civil(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

static SHOWN: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// The Load screen shows `p` now: true when that is a change (its preview needs uploading).
pub fn shown(p: &Path) -> bool {
    let mut l = SHOWN.lock().unwrap();
    if l.as_deref() == Some(p) { return false; }
    *l = Some(p.to_path_buf());
    true
}

/// The Load screen's files, newest first: own saves (quick, auto, plain), the real game's when they are a different
/// folder, and replays.
pub fn files() -> Vec<PathBuf> {
    *SHOWN.lock().unwrap() = None; // a reopened screen uploads its preview again
    let mut v = with_pending(list(&saves_dir(), "sav"), &saves_dir(), "sav"); // a save just made, still writing, too
    if saves_dir() != crossover_saves() { v.extend(list(&crossover_saves(), "sav")); }
    v.extend(list(&replays_dir(), "shcreplay"));
    sort(&mut v, 1, false);
    v
}

/// The Save dialog's list: the .sav files in our save folder (quick / auto saves too: they can be overwritten).
pub fn own_saves() -> Vec<PathBuf> {
    *SHOWN.lock().unwrap() = None;
    with_pending(list(&saves_dir(), "sav"), &saves_dir(), "sav")
}

/// The browser's Delete: only files in our save folder (incl. Replays); the real game's folder, when it is another
/// one (SHC_SAVES), stays read-only.
pub fn deletable(p: &Path) -> bool { p.starts_with(saves_dir()) }

/// Sorts a list like HUD_LoadSaveRequester's column headers: `by` 0 = name (case-insensitive), 1 = date; `asc`.
pub fn sort(v: &mut [PathBuf], by: u8, asc: bool) {
    let t = |p: &PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).unwrap_or_else(|_| SystemTime::now()); // pending = now
    let n = |p: &PathBuf| p.file_stem().map_or(String::new(), |s| s.to_string_lossy().to_lowercase());
    if by == 0 { v.sort_by_key(n) } else { v.sort_by_key(t) }
    if !asc { v.reverse() }
}

/// The Save dialog's file for a typed name: `<saves>/<name>.sav`, characters a file name can't hold replaced
/// (FileNameValidationTextBox drops them as they're typed). None = nothing left of the name.
pub fn path_for(name: &str) -> Option<PathBuf> {
    let n = clean(name);
    (!n.is_empty()).then(|| saves_dir().join(format!("{n}.sav")))
}

/// A file's entry, cached by path + modification time (the Load screen asks every frame).
pub fn entry(p: &Path) -> Entry {
    use std::{collections::HashMap, sync::Mutex};
    static CACHE: Mutex<Option<HashMap<PathBuf, (Option<SystemTime>, Entry)>>> = Mutex::new(None);
    let date = std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let mut c = CACHE.lock().unwrap();
    let c = c.get_or_insert_with(HashMap::new);
    if let Some((d, e)) = c.get(p) { if *d == date { return e.clone(); } }
    let mut e = if is_replay(p) { replay_entry(p) } else { sav_entry(p) }.unwrap_or_default();
    e.date = date;
    c.insert(p.to_path_buf(), (date, e.clone()));
    e
}

#[cfg(feature = "live")]
fn replay_entry(p: &Path) -> Option<Entry> {
    let i = crate::game_replay::Info::read(p)?;
    let preview = (!i.preview.is_empty()).then(|| block0_rgba(&i.preview)).flatten();
    Some(Entry { map: i.map, play_ms: i.play_ms, ticks: i.ticks, preview, date: None })
}
#[cfg(not(feature = "live"))]
fn replay_entry(_: &Path) -> Option<Entry> { None }

/// Header block 0 (packed palette + 200×200 indices) → RGBA.
fn block0_rgba(b: &[u8]) -> Option<Vec<u8>> {
    shc_formats::map::MapFile { version: -1, header_blocks: vec![b.to_vec()], sections: vec![], broken: vec![] }.preview_rgba()
}

/// A .sav's header: preview (block 0), map name (block 4), play time (section 1126), without reading the sections.
/// Layout: formats/src/map.rs.
fn sav_entry(p: &Path) -> Option<Entry> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(p).ok()?;
    let mut head = vec![0u8; 512 << 10];
    let n = f.read(&mut head).ok()?;
    head.truncate(n);
    let le = |b: &[u8], i: usize| -> Option<i32> { Some(i32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?)) };
    let mut pos = 4;
    let mut blocks = vec![];
    if le(&head, 0)? < 0 {
        for i in 0..7 {
            let len = le(&head, pos)? as usize;
            pos += 4;
            if len == 0 { break; }
            blocks.push(head.get(pos..pos + len)?.to_vec());
            pos += len + if i == 6 { 4 } else { 0 };
        }
    }
    let map = shc_formats::map::MapFile { version: -1, header_blocks: blocks, sections: vec![], broken: vec![] };
    let mut e = Entry { preview: map.preview_rgba(), map: map.file_name().unwrap_or_default(), ..Default::default() };
    if let Some(stem) = Path::new(&e.map).file_stem() { e.map = stem.to_string_lossy().into_owned(); }
    // directory: [i32 dir_size][7 i32][5 arrays of n i32: size, packed size, type, packed?, offset], then the data
    let dir_size = le(&head, pos)? as usize;
    let dir = head.get(pos + 4..pos + dir_size)?;
    let k = dir_size.checked_sub(4 + 28)? / 20; // a corrupt size: no play time, not a panic
    let field = |arr: usize, i: usize| le(dir, 28 + (arr * k + i) * 4);
    let count = (le(dir, 4)? as usize).min(k);
    if let Some(i) = (0..count).find(|&i| field(2, i) == Some(1126)) {
        if field(3, i)? == 0 {
            let mut b = [0u8; 4];
            f.seek(SeekFrom::Start((pos + dir_size) as u64 + field(4, i)? as u64)).ok()?;
            f.read_exact(&mut b).ok()?;
            e.play_ms = u32::from_le_bytes(b) as u64;
        }
    }
    Some(e)
}

/// Autosave rotation: the 5 newest are kept (the next one counted), whatever else is in the folder stays.
#[test]
fn autosave_rotation() {
    let dir = std::env::temp_dir().join(format!("shc-autosave-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Mine.sav"), b"x").unwrap();
    std::fs::write(dir.join("Autosave-mine.sav"), b"x").unwrap(); // a player's own name: never rotated
    for t in [900, 1000, 99, 5000, 7, 60000] {
        let p = rotate(&dir, t);
        std::fs::write(&p, b"x").unwrap();
    }
    let mut left: Vec<String> = list(&dir, "sav").iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    left.sort();
    assert_eq!(left, ["Autosave-1000.sav", "Autosave-5000.sav", "Autosave-60000.sav", "Autosave-900.sav", "Autosave-99.sav", "Autosave-mine.sav", "Mine.sav"]);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// save_async: the game goes on at once; the worker writes a .sav the browser reads (preview, map, play time).
#[cfg(feature = "live")]
#[test]
fn background_save() {
    let map = crate::skirmish::maps_dir().join("Crater Lake.map");
    if !map.exists() { return; } // needs the game install
    let m = crate::skirmish::start(&crate::skirmish::Lobby { seed: 5, ..crate::skirmish::Lobby::new(map) }).unwrap();
    let path = std::env::temp_dir().join(format!("shc-bg-save-{}.sav", std::process::id()));
    let (tx, rx) = std::sync::mpsc::channel();
    let t = std::time::Instant::now();
    save_async(&m, path.clone(), 61_000, move |r| tx.send(r.is_ok()).unwrap());
    println!("caller back after {:?}", t.elapsed());
    assert!(rx.recv().unwrap());
    let e = entry(&path);
    std::fs::remove_file(&path).ok();
    assert_eq!((e.map.as_str(), e.preview.is_some(), e.play_ms), ("Crater Lake", true, 61_000));
}

/// Save dialog names and the browser's column sort.
#[test]
fn names_and_sort() {
    assert_eq!(path_for("  My Castle/2 ").map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).as_deref(), Some("My Castle_2.sav"));
    assert_eq!(path_for("   "), None);
    let dir = std::env::temp_dir().join(format!("shc-sort-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut v = vec![];
    for (n, age) in [("beta", 30), ("Alpha", 10), ("gamma", 20)] {
        let p = dir.join(format!("{n}.sav"));
        std::fs::write(&p, b"x").unwrap();
        std::fs::File::options().write(true).open(&p).unwrap().set_modified(SystemTime::now() - std::time::Duration::from_secs(age)).unwrap();
        v.push(p);
    }
    let names = |v: &[PathBuf]| v.iter().map(|p| p.file_stem().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>();
    sort(&mut v, 0, true);
    assert_eq!(names(&v), ["Alpha", "beta", "gamma"]);
    sort(&mut v, 1, false);
    assert_eq!(names(&v), ["Alpha", "gamma", "beta"]); // newest first
    sort(&mut v, 1, true);
    assert_eq!(names(&v), ["beta", "gamma", "Alpha"]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn civil_dates() {
    assert_eq!(civil(0), (1970, 1, 1));
    assert_eq!(civil(20746), (2026, 10, 20));
    assert_eq!(hms(3_725_000), "1:02:05");
    assert_eq!(hms(65_000), "1:05");
}

/// The real game's saves (if installed): every header parses, previews are 200×200, play times are sane.
#[test]
fn reads_save_headers() {
    let saves = list(&crossover_saves(), "sav");
    for p in &saves {
        let e = entry(p);
        println!("{}: map {:?}, play {}, preview {}, {}", p.display(), e.map, e.play_text(), e.preview.is_some(), e.date_text());
        assert!(e.preview.as_ref().is_some_and(|px| px.len() == 200 * 200 * 4), "{}: no preview", p.display());
        assert!(e.play_ms < 1000 * 3600 * 1000, "{}: play time {}", p.display(), e.play_ms);
    }
}
