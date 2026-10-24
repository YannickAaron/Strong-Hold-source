//! Sharing maps (ours; docs/BEYOND_DE.md): custom maps in LAN games, and the published maps.
//!
//! LAN: the lobby's map travels by file name and, for a map that is not one of the game's, the checksum of its file
//! (`to_wire`). Every machine looks for that file in its user map folders (`find`: name and checksum, so another map
//! of the same name does not count). A peer without it says so in its Msg::Have (`MISSING` + name), the host sends
//! the file (Msg::MapFile) and the peer keeps it in the player folder's Maps (`receive`) once its size, checksum and
//! the map parser passed it. The host's Start waits until nobody misses the map, as for custom lords.
//!
//! Online: the public repo's `maps` release (scripts/publish_map.sh) holds one .map per map, labelled with the map's
//! name; Extras lists them with Get / Update. A download is checked against GitHub's SHA-256 (update.rs), parsed, and
//! saved as `<name>.map` in the player folder's Maps; `.online-sha256` there notes each one's SHA-256 for Update.
//! A .map dropped on the window is checked and copied there the same way (`import`).
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::skirmish;

/// Larger than any map the editor writes (800 × 800 is a few MB).
pub const MAX_MAP: usize = 32 << 20;
/// A Msg::Have entry for the lobby's map: this prefix + its file name.
pub const MISSING: &str = "map: ";

/// FNV-1a 64 of a map file.
pub fn checksum(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf29ce484222325u64, |h, &x| (h ^ x as u64).wrapping_mul(0x100000001b3))
}

/// Where received and downloaded maps go: the player folder's Maps (one of `skirmish::user_map_dirs`).
fn dir() -> PathBuf { skirmish::user_map_dirs()[0].clone() }

/// The lookups so far ((name, checksum) → path), and the last `to_wire` (path, mtime, checksum): the lobby asks every
/// frame, a user map is read once. `forget` after a map arrives.
static FOUND: Mutex<Option<HashMap<(String, u64), Option<PathBuf>>>> = Mutex::new(None);
static WIRE: Mutex<Option<(PathBuf, std::time::SystemTime, u64)>> = Mutex::new(None);

pub fn forget() { *FOUND.lock().unwrap() = None; }

/// The lobby's map for the wire: its file name and its checksum, 0 for one of the game's maps.
pub fn to_wire(map: &Path) -> (String, u64) {
    let name = map.file_name().map_or(String::new(), |f| f.to_string_lossy().into_owned());
    if map.parent() == Some(skirmish::maps_dir().as_path()) { return (name, 0); }
    let t = std::fs::metadata(map).and_then(|m| m.modified()).ok();
    let mut w = WIRE.lock().unwrap();
    if let (Some((p, at, sum)), Some(t)) = (w.as_ref(), t) { if p == map && *at == t { return (name, *sum); } }
    let sum = std::fs::read(map).map_or(0, |b| checksum(&b));
    if let Some(t) = t { *w = Some((map.to_path_buf(), t, sum)); }
    (name, sum)
}

/// This machine's copy of the lobby's map: a game map by name; a user map by name and checksum in the user map
/// folders (the same name first, then any file with that checksum).
pub fn find(name: &str, sum: u64) -> Option<PathBuf> {
    if sum == 0 { let p = skirmish::maps_dir().join(name); return p.exists().then_some(p); }
    let key = (name.to_string(), sum);
    if let Some(Some(r)) = FOUND.lock().unwrap().as_ref().map(|f| f.get(&key).cloned()) { return r.filter(|p| p.exists()); }
    let dirs = skirmish::user_map_dirs();
    let r = dirs.iter().map(|d| d.join(name)).chain(dirs.iter().flat_map(|d| skirmish::list(d, "map")))
        .find(|p| std::fs::metadata(p).is_ok_and(|m| m.len() as usize <= MAX_MAP) && std::fs::read(p).is_ok_and(|b| checksum(&b) == sum));
    FOUND.lock().unwrap().get_or_insert_with(HashMap::new).insert(key, r.clone());
    r
}

/// A map file name from elsewhere (LAN, a download label): a plain `<name>.map`, nothing that leaves the folder.
fn safe_name(name: &str) -> Option<String> {
    let n = name.trim();
    let ok = !n.is_empty() && !n.starts_with('.') && n.len() <= 200 && !n.contains(['/', '\\', ':', '\0']) && n.to_ascii_lowercase().ends_with(".map");
    ok.then(|| n.to_string())
}

/// A map from another machine (the LAN host, a download, a drop): checked (size, `sum` unless 0, the map parser), then
/// written to the player folder's Maps as `name`, or `<name> (2).map` … when a different map has that name. Returns
/// its path (the existing one if this map is there already).
pub fn receive(name: &str, sum: u64, bytes: &[u8]) -> Result<PathBuf, String> {
    let name = safe_name(name).ok_or(format!("{name}: not a map file name"))?;
    if bytes.len() > MAX_MAP { return Err(format!("{name}: too big for a map")); }
    if sum != 0 && checksum(bytes) != sum { return Err(format!("{name}: arrived damaged")); }
    shc_formats::map::parse(bytes).map_err(|e| format!("{name}: not a map ({e:?})"))?;
    receive_into(&dir(), &name, bytes)
}

fn receive_into(dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let stem = name[..name.len() - 4].to_string();
    for k in 1..100 {
        let p = dir.join(if k == 1 { name.to_string() } else { format!("{stem} ({k}).map") });
        match std::fs::read(&p) {
            Ok(b) if b == bytes => return Ok(p),
            Ok(_) => continue,
            Err(_) => {
                let tmp = dir.join(format!(".{name}.part"));
                std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, &p)).map_err(|e| format!("{}: {e}", p.display()))?;
                forget();
                return Ok(p);
            }
        }
    }
    Err(format!("{name}: too many maps of that name"))
}

// ---- the Extras screen's state (extras.rs draws it)

/// The last map import / download's message (text, ok); Extras' right column shows the maps (else the lords).
pub static MESSAGE: Mutex<Option<(String, bool)>> = Mutex::new(None);
pub static SHOW_MAPS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A .map dropped on the window: checked and copied into the player folder's Maps (the result as MESSAGE).
pub fn import(file: &Path) {
    let name = file.file_name().map_or(String::new(), |f| f.to_string_lossy().into_owned());
    let r = match std::fs::metadata(file) {
        Ok(m) if m.len() as usize > MAX_MAP => Err(format!("{name}: too big for a map")),
        _ => std::fs::read(file).map_err(|e| format!("{name}: {e}")).and_then(|b| receive(&name, 0, &b)),
    };
    *MESSAGE.lock().unwrap() = Some(match r { Ok(p) => (format!("Added {} to your maps", p.file_name().unwrap_or_default().to_string_lossy()), true), Err(e) => (format!("Not added: {e}"), false) });
    SHOW_MAPS.store(true, std::sync::atomic::Ordering::Relaxed);
    crate::lord_share::SHOW.store(true, std::sync::atomic::Ordering::Relaxed); // the menu opens Extras to show it
}

// ---- online maps

const MAPS: &str = "https://api.github.com/repos/YannickAaron/Strong-Hold/releases/tags/maps";
const SHA_FILE: &str = ".online-sha256";

/// A published map: its name (the asset's label, without ".map") and its file.
#[derive(Clone, Debug, PartialEq)]
pub struct Online { pub name: String, pub file: crate::update::Asset }

static ONLINE: Mutex<Vec<Online>> = Mutex::new(Vec::new());
pub static GETTING: Mutex<Option<(String, f32)>> = Mutex::new(None);

pub use crate::lord_share::State;

/// Fetches the published maps in the background (Extras opening).
pub fn fetch_online() {
    std::thread::spawn(|| {
        let url = std::env::var("SHC_MAPS_API").unwrap_or_else(|_| MAPS.into()); // tests: another release's maps
        if let Some(v) = crate::update::get_json(&url).map(|j| online_of(&j)) { *ONLINE.lock().unwrap() = v; }
    });
}

/// The maps: .map files with a label (publish_map.sh sets the map's name); any other file of the release is not one.
fn online_of(json: &str) -> Vec<Online> {
    crate::update::assets(json).into_iter().filter(|a| a.file.to_ascii_lowercase().ends_with(".map") && !a.label.trim().is_empty())
        .filter_map(|file| Some(Online { name: safe_name(&format!("{}.map", file.label.trim()))?.trim_end_matches(".map").to_string(), file })).collect()
}

pub fn online() -> Vec<Online> { ONLINE.lock().unwrap().clone() }

/// The SHA-256 each downloaded map had (name → hash), from Maps/.online-sha256.
fn shas(dir: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(dir.join(SHA_FILE)).unwrap_or_default().lines().filter_map(|l| l.split_once('\t')).map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

/// Map `name`'s row: Get (not here), Update (here from an older upload), Up to date.
pub fn state(o: &Online) -> State { state_in(o, &dir()) }

fn state_in(o: &Online, dir: &Path) -> State {
    if !dir.join(format!("{}.map", o.name)).exists() { return State::Get; }
    if shas(dir).iter().any(|(n, h)| *n == o.name && *h == o.file.sha) { State::Current } else { State::Update }
}

/// Downloads map `name` in the background into the player folder's Maps (replacing an older download of it).
pub fn get(name: &str) {
    let Some(o) = online().into_iter().find(|o| o.name == name) else { return };
    {
        let mut g = GETTING.lock().unwrap();
        if g.is_some() { return; }
        *g = Some((o.name.clone(), 0.0));
    }
    std::thread::spawn(move || {
        let tmp = crate::paths::app_data().join("downloads").join(format!("{}.map", o.name));
        let r = crate::update::download(&o.file, &tmp, &|p| *GETTING.lock().unwrap() = Some((o.name.clone(), p)))
            .and_then(|_| std::fs::read(&tmp).map_err(|e| e.to_string()))
            .and_then(|b| install(&dir(), &o, &b));
        *MESSAGE.lock().unwrap() = Some(match r { Ok(()) => (format!("Got {}: it is in the skirmish's map list", o.name), true), Err(e) => (format!("{}: {e}", o.name), false) });
        let _ = std::fs::remove_file(&tmp);
        *GETTING.lock().unwrap() = None;
    });
}

/// A downloaded map (already checked against GitHub's SHA-256): parsed, written as `<name>.map` over an older
/// download, its SHA-256 noted.
fn install(dir: &Path, o: &Online, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_MAP { return Err("too big for a map".into()); }
    shc_formats::map::parse(bytes).map_err(|e| format!("not a map ({e:?})"))?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let (p, tmp) = (dir.join(format!("{}.map", o.name)), dir.join(format!(".{}.map.part", o.name)));
    std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, &p)).map_err(|e| format!("{}: {e}", p.display()))?;
    let mut s = shas(dir);
    s.retain(|(n, _)| *n != o.name);
    s.push((o.name.clone(), o.file.sha.clone()));
    let _ = std::fs::write(dir.join(SHA_FILE), s.iter().map(|(n, h)| format!("{n}\t{h}\n")).collect::<String>());
    forget();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A received map lands under its name; the same map again is the same file; another map of that name gets
    /// " (2)"; names that would leave the folder are refused.
    #[test]
    fn receive_names() {
        let d = std::env::temp_dir().join(format!("shc-map-share-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let a = receive_into(&d, "Hill Fort.map", b"one").unwrap();
        assert_eq!(a, d.join("Hill Fort.map"));
        assert_eq!(receive_into(&d, "Hill Fort.map", b"one").unwrap(), a, "already there");
        assert_eq!(receive_into(&d, "Hill Fort.map", b"two").unwrap(), d.join("Hill Fort (2).map"));
        for bad in ["../x.map", "a/b.map", ".hidden.map", "x.sav", "", "C:x.map"] { assert!(safe_name(bad).is_none(), "{bad}"); }
        assert!(receive("Hill Fort.map", checksum(b"one") ^ 1, b"one").is_err_and(|e| e.contains("damaged")));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Online rows: labelled .map assets only; Get, then Up to date after an install, Update when the upload changed.
    #[test]
    fn online_states() {
        let json = r#"{"assets":[{"name":"Hill.Fort.map","label":"Hill Fort","size":3,"digest":"sha256:aa","browser_download_url":"u1"},
            {"name":"notes.txt","label":"x","size":1,"digest":"sha256:bb","browser_download_url":"u2"}],"tarball_url":""}"#;
        let v = online_of(json);
        assert_eq!(v.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(), ["Hill Fort"]);
        let d = std::env::temp_dir().join(format!("shc-map-online-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(state_in(&v[0], &d), State::Get);
        let map = std::fs::read(skirmish::maps_dir().join("Crater Lake.map"));
        let Ok(map) = map else { return }; // the rest needs the game install
        install(&d, &v[0], &map).unwrap();
        assert_eq!(state_in(&v[0], &d), State::Current);
        let newer = Online { file: crate::update::Asset { sha: "cc".into(), ..v[0].file.clone() }, ..v[0].clone() };
        assert_eq!(state_in(&newer, &d), State::Update);
        assert!(install(&d, &v[0], b"not a map").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
