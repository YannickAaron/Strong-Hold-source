//! First run: make sure the game's data is here before anything loads it. The app carries none of Firefly's files:
//! it reads the user's own copy (CrossOver/Steam install, a chosen folder, or a fresh download from Steam with their
//! account, steam.rs) and exports what the viewer, audio and sim read (extract/, ~3 min). macOS: a setup window
//! (window.rs), then the app restarts itself into the game. Elsewhere: the export runs in the terminal.
pub mod steam;
#[cfg(target_os = "macos")]
mod window;

use std::path::{Path, PathBuf};
use crate::paths;

pub struct Dirs { pub cache: PathBuf, pub audio: PathBuf }

pub fn dirs() -> Dirs {
    Dirs { cache: crate::cache_dir(), audio: std::env::var_os("SHC_AUDIO").map(PathBuf::from).unwrap_or_else(|| paths::cache("shc-audio")) }
}

/// A folder holding the game (the one with `<game>_Data`), with the files the export and the game read.
pub fn is_game(g: &Path) -> bool {
    let d = g.join(shc_extract::GAME_DATA);
    d.join("resources.assets").is_file() && d.join("StreamingAssets/Maps").is_dir() && d.join("Managed/Assembly-CSharp.dll").is_file()
}

/// The game folder for a folder the user picked: the game folder itself, its _Data folder, or a folder holding it.
pub fn game_in(p: &Path) -> Option<PathBuf> {
    [p.to_path_buf(), p.parent().map(Path::to_path_buf).unwrap_or_default(), p.join("Stronghold Crusader Definitive Edition")]
        .into_iter().find(|g| is_game(g))
}

/// A copy already on this machine: SHC_GAME / the chosen folder / an earlier download / Steam's (CrossOver's).
pub fn find_game() -> Option<PathBuf> { Some(paths::game()).filter(|g| is_game(g)).or_else(|| paths::steam_game().filter(|g| is_game(g))) }

fn ready(d: &Dirs) -> bool { shc_extract::is_complete(&d.cache) && is_game(&paths::game()) }

/// Called first in main. Returns when the game's data is ready (possibly after restarting the process).
pub fn ensure() {
    let d = dirs();
    #[cfg(feature = "live")]
    if d.cache.join("dll").is_dir() { shc_sim::data::set_dir(d.cache.join("dll")); }
    // the main menu's "New game files available — Download" (start_update): the setup window in update mode, then the game
    #[cfg(target_os = "macos")]
    if std::env::var_os("SHC_SETUP_UPDATE").is_some() {
        if let Some(u) = update_todo() { if !window::run(&d, Some(&u)) { std::process::exit(0); } }
        restart("SHC_SETUP_UPDATE", None);
    }
    #[cfg(target_os = "macos")]
    std::thread::spawn(|| { let _ = PENDING.set(update_todo()); });
    // SHC_CACHE: a developer's own export folder (re/scripts/export_*.py or `extract`): never re-exported
    if std::env::var_os("SHC_CACHE").is_some() || ready(&d) { more(&d); sc2(&d); return; }
    if std::env::var_os("SHC_SETUP_DONE").is_some() {
        eprintln!("setup: finished but the data still isn't complete ({}); giving up", d.cache.display());
        std::process::exit(1);
    }
    #[cfg(target_os = "macos")]
    {
        if !window::run(&d, None) { std::process::exit(0); }
        // a fresh process for the game: winit starts its own NSApplication launch cleanly
        restart("SHC_SETUP_DONE", Some("1"));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let Some(g) = find_game() else {
            eprintln!("Stronghold Crusader: Definitive Edition not found. Install it with Steam (or set SHC_GAME to its folder), then start again.");
            std::process::exit(1);
        };
        eprintln!("First start: reading the game's files from {} (a few minutes, once)", g.display());
        if let Err(e) = shc_extract::run(&g, &d.cache, &d.audio, &mut |l, f| eprint!("\r{:3.0}% {l:30}", f * 100.0)) {
            eprintln!("\nsetup failed: {e}");
            std::process::exit(1);
        }
        eprintln!();
    }
}

/// This process again, with env `k` set to `v` (None: removed).
#[cfg(target_os = "macos")]
fn restart(k: &str, v: Option<&str>) -> ! {
    use std::os::unix::process::CommandExt;
    let mut c = std::process::Command::new(std::env::current_exe().unwrap());
    c.args(std::env::args_os().skip(1));
    match v { Some(v) => { c.env(k, v); } None => { c.env_remove(k); } }
    eprintln!("setup: restart failed: {}", c.exec());
    std::process::exit(1);
}

/// Every file steam_files.txt picks from the pinned build (steam::DEPOTS), "<bytes> <path>"; the test
/// `steam_index_current` checks it against the CrossOver install (SHC_WRITE_INDEX=1 rewrites it). Names and sizes only.
const INDEX: &str = include_str!("../../steam_index.txt");

/// The build the setup's Steam download is (written after each download): DEPOTS' manifests, or "latest".
fn build_file() -> PathBuf { paths::downloaded().join(".strong_hold_build") }
fn pinned_build() -> String { steam::DEPOTS.map(|d| d.1).join(" ") }
pub fn mark_build(pinned: bool) { let _ = std::fs::write(build_file(), if pinned { pinned_build() } else { "latest".into() }); }

/// What the setup's Steam download lacks after an app update added files to steam_files.txt (or moved DEPOTS).
pub struct Update {
    /// Missing (or wrong-sized) files, manifest paths.
    pub files: Vec<String>,
    pub bytes: u64,
    /// The download is an older pinned build: everything is refreshed (DepotDownloader fetches only what changed).
    pub new_build: bool,
}

/// None when the game comes from elsewhere (CrossOver, a chosen folder, SHC_GAME: those have every file) or is complete.
pub fn update_todo() -> Option<Update> {
    let g = paths::downloaded();
    if paths::game() != g || !is_game(&g) { return None; }
    // ponytail: no marker = downloaded by a release before markers existed; their DEPOTS are today's
    let build = std::fs::read_to_string(build_file()).map(|s| s.trim().to_string()).unwrap_or_else(|_| pinned_build());
    let (pinned, latest) = (build == pinned_build(), build == "latest");
    let mut u = Update { files: vec![], bytes: 0, new_build: !pinned && !latest };
    for (size, path) in INDEX.lines().filter_map(|l| l.split_once(' ')).filter_map(|(n, p)| Some((n.parse::<u64>().ok()?, p))) {
        // sizes only hold for the pinned build; a "latest" copy only needs the file to be there
        let have = std::fs::metadata(g.join(path)).is_ok_and(|m| latest || m.len() == size);
        if !have { u.files.push(path.to_string()); u.bytes += size; }
    }
    (u.new_build || !u.files.is_empty()).then_some(u)
}

static PENDING: std::sync::OnceLock<Option<Update>> = std::sync::OnceLock::new();

/// The main menu's notice: what an update would fetch (worked out in the background at start).
pub fn pending() -> Option<&'static Update> { PENDING.get()?.as_ref() }

/// The notice's Download: restart into the setup window's update mode.
pub fn start_update() {
    #[cfg(target_os = "macos")]
    restart("SHC_SETUP_UPDATE", Some("1"));
}

/// DepotDownloader's -filelist for exactly `files`.
pub fn filelist(files: &[String]) -> String {
    let esc = |p: &str| p.chars().fold(String::new(), |mut s, c| { if r"\.+*?()|[]{}^$#".contains(c) { s.push('\\'); } s.push(c); s });
    files.iter().map(|f| format!("regex:^{}$\n", esc(f))).collect()
}

/// Files added to the export after the first release (extract::video::export_more: the minimap's character / event /
/// lord videos, the mission-over videos), made in the background on an export that lacks them; the game runs
/// meanwhile and shows each one once its file is there. Never a full re-setup. Returns the thread (tests join it).
/// ponytail: a second app instance would redo the same files (each lands whole: .part + rename).
pub fn more(d: &Dirs) -> Option<std::thread::JoinHandle<()>> {
    if !shc_extract::is_complete(&d.cache) { return None; }
    let data = find_game()?.join(shc_extract::GAME_DATA);
    let out = d.cache.join("video");
    if shc_extract::video::more_todo(&data, &out).is_empty() { return None; }
    Some(std::thread::spawn(move || {
        // utility QoS (its worker threads inherit it): the game's frames come first
        #[cfg(target_os = "macos")]
        unsafe { libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0); }
        let t = std::time::Instant::now();
        let r = shc_extract::video::export_more(&data, &out, &mut |k, n| if k < n { eprintln!("setup: more videos {}/{n}", k + 1) });
        match r { Ok(()) => eprintln!("setup: more videos done in {:.0} s", t.elapsed().as_secs_f32()), Err(e) => eprintln!("setup: more videos: {e}") }
    }))
}

/// Crusader 2's Sassanid Knight sheet (extract::sc2, ~10 s) when SC2 is installed and the export lacks it: in the
/// background like `more`; the viewer loads it from the next start. No SC2 = nothing happens.
/// ponytail: the .tsv files are replaced (renamed) under a viewer that may be reading them at that moment; it reads
/// them in its first second, the renders take ~10 s.
pub fn sc2(d: &Dirs) -> Option<std::thread::JoinHandle<()>> {
    if shc_extract::sc2::is_complete(&d.cache) || !d.cache.join("atlases.tsv").is_file() { return None; }
    let (g, cache) = (paths::sc2_game()?, d.cache.clone());
    Some(std::thread::spawn(move || {
        #[cfg(target_os = "macos")]
        unsafe { libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0); }
        if let Err(e) = shc_extract::sc2::export(&g, &cache) { eprintln!("setup: Crusader 2 units: {e}"); }
    }))
}

/// Free bytes on the disk holding `p` (its nearest existing ancestor).
pub fn free_bytes(p: &Path) -> Option<u64> {
    let p = p.ancestors().find(|a| a.exists())?;
    #[cfg(unix)]
    {
        let c = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()).ok()?;
        let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 { return None; }
        Some(s.f_bavail as u64 * s.f_frsize as u64)
    }
    #[cfg(not(unix))]
    { let _ = p; None }
}

/// Can we reach Steam? (3 s)
pub fn online() -> bool {
    use std::net::ToSocketAddrs;
    ("api.steampowered.com", 443).to_socket_addrs().ok().and_then(|mut a| a.next())
        .is_some_and(|a| std::net::TcpStream::connect_timeout(&a, std::time::Duration::from_secs(3)).is_ok())
}

/// Bytes the setup needs: download (6.2 GB) + exports (3.1 GB, 0.9 of it the minimap / mission-over videos), or the exports alone.
pub fn needed(download: bool) -> u64 { if download { 9_500_000_000 } else { 3_400_000_000 } }

/// 24-bit BMP (NSImage reads it; no image crate needed): `px(x, y)` -> rgb, rows top-down.
pub fn bmp(w: usize, h: usize, px: impl Fn(usize, usize) -> [u8; 3]) -> Vec<u8> {
    let row = (w * 3 + 3) & !3;
    let size = 54 + row * h;
    let mut b = Vec::with_capacity(size);
    b.extend_from_slice(b"BM");
    for v in [size as u32, 0, 54, 40, w as u32, h as u32] { b.extend_from_slice(&v.to_le_bytes()); }
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&24u16.to_le_bytes());
    for v in [0u32, (row * h) as u32, 2835, 2835, 0, 0] { b.extend_from_slice(&v.to_le_bytes()); }
    for y in (0..h).rev() {
        for x in 0..w { let [r, g, bl] = px(x, y); b.extend_from_slice(&[bl, g, r]); }
        b.resize(b.len() + row - w * 3, 0);
    }
    b
}

#[test]
fn update_filelist() {
    assert_eq!(filelist(&["A b/c(1).webm".into(), "x".into()]), "regex:^A b/c\\(1\\)\\.webm$\nregex:^x$\n");
    let r = INDEX.lines().next().unwrap();
    assert!(r.split_once(' ').is_some_and(|(n, p)| n.parse::<u64>().is_ok() && !p.is_empty()), "{r}");
}

/// steam_index.txt = steam_files.txt applied to the CrossOver install, when that is the pinned build.
#[test]
#[cfg(target_os = "macos")]
fn steam_index_current() {
    let Some(g) = paths::steam_game() else { return eprintln!("skipped: no CrossOver install") };
    let acf = std::fs::read_to_string(g.join("../../appmanifest_3024040.acf")).unwrap_or_default();
    if !steam::DEPOTS.iter().all(|d| acf.contains(&format!("\"manifest\"\t\t\"{}\"", d.1))) { return eprintln!("skipped: CrossOver has another build"); }
    let res: Vec<regex::Regex> = include_str!("../../steam_files.txt").lines().filter_map(|l| l.strip_prefix("regex:"))
        .map(|r| regex::RegexBuilder::new(r).case_insensitive(true).build().unwrap()).collect(); // DepotDownloader: IgnoreCase
    fn walk(d: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() { walk(&p, out) } else { out.push(p) }
        }
    }
    let mut all = vec![];
    walk(&g, &mut all);
    let mut rows: Vec<(String, u64)> = all.iter().filter_map(|p| {
        let rel = p.strip_prefix(&g).ok()?.to_string_lossy().into_owned();
        res.iter().any(|r| r.is_match(&rel)).then(|| (rel, p.metadata().unwrap().len()))
    }).collect();
    rows.sort();
    let want: String = rows.iter().map(|(p, n)| format!("{n} {p}\n")).collect();
    if std::env::var_os("SHC_WRITE_INDEX").is_some() { std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/steam_index.txt"), &want).unwrap(); }
    else { assert!(INDEX == want, "steam_index.txt is stale: SHC_WRITE_INDEX=1 cargo test steam_index_current"); }
}

#[test]
fn bmp_layout() {
    let b = bmp(3, 2, |x, y| [x as u8, y as u8, 7]);
    assert_eq!(b.len(), 54 + 12 * 2);
    assert_eq!(&b[54..57], &[7, 1, 0]); // first stored row = bottom row (y 1), BGR
}
