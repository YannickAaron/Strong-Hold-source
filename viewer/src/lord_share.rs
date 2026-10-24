//! Sharing custom lords: importing a lord's .zip, downloading / updating the published lords, and custom lords in
//! LAN games.
//!
//! Import: a .zip with `CustomLords/<Name>/…` (required: a .lordjson) and optionally `CustomMedia/<Name>/…`, the
//! same layout as the player folder (any top folder above them is fine, e.g. a zipped "Lord X/" folder). Every file
//! is checked before anything is written: no path outside the lord's folders, no links, no encrypted entries, only
//! the file types a lord uses, size caps, the .lordjson and every .aivjson must parse. An existing lord of the same
//! name is replaced only when asked (`import` returns `Exists`). Files land in a temp folder first, then move in.
//!
//! Online: the public repo's `lords` release (scripts/publish_lord.sh) holds one zip per lord, labelled with the
//! lord's name. Every import notes its zip's SHA-256 in `CustomLords/<Name>/.zip-sha256`; a lord whose online zip
//! has another one has an update. Downloads are checked against GitHub's SHA-256 before they're imported.
//!
//! LAN: a lobby's AI that is a custom lord travels as `custom_lords::BASE + i`, i into the lobby's `customs` table of
//! (name, checksum); every machine maps that to its own list by name and checksum of the sim-relevant data (config
//! + castles: media and sprite are local looks and cannot desync). A peer missing one reports it (`missing`), and
//! the host cannot start until nobody misses any.
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::custom_lords::{self, Custom, BASE};

const MAX_FILE: u64 = 64 << 20;
const MAX_TOTAL: u64 = 512 << 20;
const MAX_FILES: usize = 4000;
const TYPES: [&str; 11] = ["lordjson", "aivjson", "png", "wav", "webm", "txt", "tsv", "bc", "json", "ldata", "data"];

#[derive(Debug, PartialEq)]
pub enum ImportErr {
    /// Lords of these names are already installed: call again with `replace` to overwrite them.
    Exists(Vec<String>),
    Bad(String),
}

/// One file of the zip, already placed: (CustomLords | CustomMedia, lord name, path inside the lord's folder, bytes).
type Entry = (&'static str, String, PathBuf, Vec<u8>);

/// Import the lord(s) in zip `file` into the player folder; `replace` = overwrite lords of the same name. Returns
/// the lords' names.
pub fn import(file: &Path, replace: bool) -> Result<Vec<String>, ImportErr> {
    import_into(file, &crate::skirmish::profile_dir(), replace)
}

fn import_into(file: &Path, root: &Path, replace: bool) -> Result<Vec<String>, ImportErr> {
    let bad = |e: String| ImportErr::Bad(e);
    // a wrongly picked huge file: refused before it is read into memory
    if std::fs::metadata(file).is_ok_and(|m| m.len() > MAX_TOTAL) { return Err(bad(format!("{}: too big for a lord", file.display()))); }
    let data = std::fs::read(file).map_err(|e| bad(format!("{}: {e}", file.display())))?;
    let entries = read_zip(&data).map_err(bad)?;
    let mut names: Vec<String> = entries.iter().filter(|e| e.0 == "CustomLords").map(|e| e.1.clone()).collect();
    names.sort();
    names.dedup();
    if names.is_empty() { return Err(bad("no CustomLords/<name>/ folder in the zip".into())); }
    for n in &names {
        let mut lords = entries.iter().filter(|e| e.0 == "CustomLords" && &e.1 == n);
        let cfg = lords.clone().find(|e| ext(&e.2) == "lordjson").ok_or_else(|| bad(format!("{n}: no .lordjson")))?;
        let text = String::from_utf8(cfg.3.clone()).map_err(|_| bad(format!("{n}: the .lordjson is not text")))?;
        custom_lords::lordjson(&text).ok_or_else(|| bad(format!("{n}: the .lordjson does not parse")))?;
        for a in lords.by_ref().filter(|e| ext(&e.2) == "aivjson") {
            let t = String::from_utf8(a.3.clone()).ok().and_then(|t| custom_lords::aivjson(&t));
            if t.is_none_or(|b| shc_formats::aiv::parse(&b).is_err()) { return Err(bad(format!("{n}: castle {} does not parse", a.2.display()))); }
        }
    }
    let there: Vec<String> = names.iter().filter(|n| root.join("CustomLords").join(n).exists()).cloned().collect();
    if !there.is_empty() && !replace { return Err(ImportErr::Exists(there)); }
    // write into a temp folder next to the targets, then swap the folders in
    let tmp = root.join(format!(".lord-import-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    for (kind, name, rel, bytes) in &entries {
        let p = tmp.join(kind).join(name).join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).and_then(|_| std::fs::write(&p, bytes)).map_err(|e| bad(format!("{}: {e}", p.display())))?;
    }
    for kind in ["CustomLords", "CustomMedia"] {
        for n in &names {
            let (src, dst) = (tmp.join(kind).join(n), root.join(kind).join(n));
            if !src.exists() { continue; }
            if dst.exists() { std::fs::remove_dir_all(&dst).map_err(|e| bad(format!("{}: {e}", dst.display())))?; }
            std::fs::create_dir_all(dst.parent().unwrap()).and_then(|_| std::fs::rename(&src, &dst)).map_err(|e| bad(format!("{}: {e}", dst.display())))?;
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    if let Some(h) = crate::update::sha256(file) {
        for n in &names { let _ = std::fs::write(root.join("CustomLords").join(n).join(SHA_FILE), &h); }
    }
    custom_lords::rescan();
    Ok(names)
}

/// In a lord's CustomLords folder: the SHA-256 of the zip it came from (pack_zip.py skips dot files).
const SHA_FILE: &str = ".zip-sha256";

fn ext(p: &Path) -> String { p.extension().map_or(String::new(), |e| e.to_string_lossy().to_ascii_lowercase()) }

/// The zip's lord files, checked. ponytail: stored and deflate entries, no ZIP64 (lords are far below 4 GB).
fn read_zip(d: &[u8]) -> Result<Vec<Entry>, String> {
    let u16_ = |o: usize| -> Result<usize, String> { d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).ok_or("truncated zip".into()) };
    let u32_ = |o: usize| -> Result<usize, String> { d.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize).ok_or("truncated zip".into()) };
    let eocd = (0..d.len().saturating_sub(21)).rev().take(65557).find(|&o| d[o..o + 4] == [0x50, 0x4b, 5, 6]).ok_or("not a zip file")?;
    let (count, mut at) = (u16_(eocd + 10)?, u32_(eocd + 16)?);
    if count > MAX_FILES { return Err(format!("{count} files: too many for a lord")); }
    let (mut out, mut total) = (vec![], 0u64);
    for _ in 0..count {
        if u32_(at)? != 0x02014b50 { return Err("broken zip directory".into()); }
        let (flags, method, csize, size) = (u16_(at + 8)?, u16_(at + 10)?, u32_(at + 20)?, u32_(at + 24)? as u64);
        let (nlen, xlen, clen, attrs, local) = (u16_(at + 28)?, u16_(at + 30)?, u16_(at + 32)?, u32_(at + 38)?, u32_(at + 42)?);
        let raw = String::from_utf8_lossy(d.get(at + 46..at + 46 + nlen).ok_or("truncated zip")?).replace('\\', "/");
        at += 46 + nlen + xlen + clen;
        if raw.ends_with('/') { continue; } // a folder
        let parts: Vec<&str> = raw.split('/').collect();
        if raw.starts_with('/') || parts.iter().any(|c| *c == ".." || c.contains(':')) { return Err(format!("{raw}: path outside the lord's folders")); }
        if parts.first() == Some(&"__MACOSX") || parts.last().is_some_and(|f| f.starts_with("._") || *f == ".DS_Store") { continue; }
        let Some(k) = parts.iter().position(|c| c.eq_ignore_ascii_case("CustomLords") || c.eq_ignore_ascii_case("CustomMedia")) else { continue }; // a readme next to them
        // the lord's name too: a "." lord would make Replace delete the whole CustomLords folder
        if parts.len() < k + 3 || parts[k + 1..].iter().any(|c| c.is_empty() || c.starts_with('.')) { return Err(format!("{raw}: not inside a lord's folder")); }
        if (attrs >> 16) & 0o170000 == 0o120000 { return Err(format!("{raw}: links are not allowed")); }
        if flags & 1 != 0 { return Err(format!("{raw}: encrypted")); }
        let rel = PathBuf::from_iter(&parts[k + 2..]);
        if !TYPES.contains(&ext(&rel).as_str()) { return Err(format!("{raw}: a lord has no .{} files", ext(&rel))); }
        if size > MAX_FILE { return Err(format!("{raw}: {} MB, more than a lord's file can be", size >> 20)); }
        total += size;
        if total > MAX_TOTAL { return Err("more than 512 MB: too big for a lord".into()); }
        let lh = local;
        if u32_(lh)? != 0x04034b50 { return Err(format!("{raw}: broken entry")); }
        let start = lh + 30 + u16_(lh + 26)? + u16_(lh + 28)?;
        let comp = d.get(start..start + csize).ok_or("truncated zip")?;
        let bytes = match method {
            0 => comp.to_vec(),
            8 => {
                let mut v = Vec::with_capacity(size as usize);
                std::io::Read::read_to_end(&mut std::io::Read::take(flate2::read::DeflateDecoder::new(comp), MAX_FILE + 1), &mut v).map_err(|e| format!("{raw}: {e}"))?;
                v
            }
            m => return Err(format!("{raw}: compression {m} not supported (use a normal zip)")),
        };
        if bytes.len() as u64 != size { return Err(format!("{raw}: wrong size after unpacking")); }
        let kind = if parts[k].eq_ignore_ascii_case("CustomLords") { "CustomLords" } else { "CustomMedia" };
        out.push((kind, parts[k + 1].to_string(), rel, bytes));
    }
    Ok(out)
}

// ---- the Extras screen's import state (extras.rs draws it)

/// A zip waiting for "replace?" (path, the names already there), and the last import's message (text, ok).
pub static PENDING: Mutex<Option<(PathBuf, Vec<String>)>> = Mutex::new(None);
pub static MESSAGE: Mutex<Option<(String, bool)>> = Mutex::new(None);
/// An import happened outside the Extras screen (a .zip dropped on the window): the menu opens it to show the result.
pub static SHOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Import `file` from the Extras screen or a drop on the window: the result as MESSAGE, or PENDING when lords of the
/// same name are there. A lord imported joins the custom lords list (the skirmish's Add-AI faces) at once.
pub fn import_ui(file: &Path, replace: bool) {
    *PENDING.lock().unwrap() = None;
    SHOW.store(true, std::sync::atomic::Ordering::Relaxed);
    let msg = match import(file, replace) {
        Ok(n) => { custom_lords::rescan(); (format!("Imported {}", n.join(", ")), true) }
        Err(ImportErr::Exists(n)) => { *PENDING.lock().unwrap() = Some((file.to_path_buf(), n)); *MESSAGE.lock().unwrap() = None; return; }
        Err(ImportErr::Bad(e)) => (format!("Not imported: {e}"), false),
    };
    *MESSAGE.lock().unwrap() = Some(msg);
}

/// macOS: the file picker for a lord's .zip.
#[cfg(target_os = "macos")]
pub fn pick_zip() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SHC_PICK") { return Some(p.into()); } // scripted runs: the file the picker would return
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
    let mtm = objc2_foundation::MainThreadMarker::new()?;
    let p = unsafe { NSOpenPanel::openPanel(mtm) };
    unsafe {
        p.setCanChooseFiles(true);
        p.setCanChooseDirectories(false);
        p.setAllowsMultipleSelection(false);
        p.setMessage(Some(&objc2_foundation::NSString::from_str("Choose a custom lord's .zip")));
    }
    if unsafe { p.runModal() } != NSModalResponseOK { return None; }
    unsafe { p.URL() }.and_then(|u| unsafe { u.path() }).map(|s| PathBuf::from(s.to_string()))
}
#[cfg(not(target_os = "macos"))]
pub fn pick_zip() -> Option<PathBuf> { None }

// ---- online lords

const LORDS: &str = "https://api.github.com/repos/YannickAaron/Strong-Hold/releases/tags/lords";

/// A published lord: its name (the asset's label) and its zip.
#[derive(Clone, Debug, PartialEq)]
pub struct Online { pub name: String, pub zip: crate::update::Asset }

/// The published lords, once fetched (empty offline / none published); the lord downloading now and how far.
static ONLINE: Mutex<Vec<Online>> = Mutex::new(Vec::new());
pub static GETTING: Mutex<Option<(String, f32)>> = Mutex::new(None);

/// What a lord's row offers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State { Get, Update, Current }

/// Fetches the published lords in the background (start, Extras opening).
pub fn fetch_online() {
    std::thread::spawn(|| {
        let url = std::env::var("SHC_LORDS_API").unwrap_or_else(|_| LORDS.into()); // tests: another release's lords
        if let Some(v) = crate::update::get_json(&url).map(|j| online_of(&j)) { *ONLINE.lock().unwrap() = v; }
        custom_lords::rescan(); // the main menu's notice compares the installed ones
    });
}

/// The lords: zips with a label (publish_lord.sh sets the lord's name); any other file of the release is not one.
fn online_of(json: &str) -> Vec<Online> {
    crate::update::assets(json).into_iter().filter(|a| a.file.to_ascii_lowercase().ends_with(".zip") && !a.label.trim().is_empty())
        .map(|zip| Online { name: zip.label.trim().to_string(), zip }).collect()
}

pub fn online() -> Vec<Online> { ONLINE.lock().unwrap().clone() }

/// Lord `name`'s row: None when it isn't published.
pub fn state(name: &str) -> Option<State> { state_in(name, &crate::skirmish::profile_dir()) }

fn state_in(name: &str, root: &Path) -> Option<State> {
    let o = online().into_iter().find(|o| o.name == name)?;
    let dir = root.join("CustomLords").join(name);
    Some(if !dir.is_dir() { State::Get } else if std::fs::read_to_string(dir.join(SHA_FILE)).is_ok_and(|h| h.trim() == o.zip.sha) { State::Current } else { State::Update })
}

/// Installed lords with a newer zip online (the main menu's notice).
pub fn updates() -> usize { custom_lords::names().iter().filter(|n| state(n) == Some(State::Update)).count() }

/// Downloads lord `name`'s zip in the background and imports it (replacing the installed one); the result as MESSAGE.
pub fn get(name: &str) {
    let Some(o) = online().into_iter().find(|o| o.name == name) else { return };
    {
        let mut g = GETTING.lock().unwrap();
        if g.is_some() { return; }
        *g = Some((o.name.clone(), 0.0));
    }
    std::thread::spawn(move || {
        let file = crate::paths::app_data().join("downloads").join(&o.zip.file);
        let got = crate::update::download(&o.zip, &file, &|p| *GETTING.lock().unwrap() = Some((o.name.clone(), p)));
        match got {
            Ok(()) => import_ui(&file, true),
            Err(e) => { *MESSAGE.lock().unwrap() = Some((format!("{}: {e}", o.name), false)); }
        }
        let _ = std::fs::remove_file(&file);
        *GETTING.lock().unwrap() = None;
    });
}

// ---- LAN

/// The sim-relevant data's checksum (config + castles), FNV-1a 64.
pub fn checksum(c: &Custom) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    let mut eat = |b: &[u8]| for &x in b { h = (h ^ x as u64).wrapping_mul(0x100000001b3); };
    for v in &c.config { eat(&v.to_le_bytes()); }
    for a in &c.aivs {
        eat(&(a.len() as u32).to_le_bytes());
        for v in a { eat(&v.to_le_bytes()); }
    }
    h
}

/// The local custom lord with this name and checksum (the list as scanned; `missing` rescans).
pub fn find(name: &str, sum: u64) -> Option<usize> {
    (0..custom_lords::count()).find(|&k| custom_lords::get(k).is_some_and(|c| c.name == name && checksum(&c) == sum))
}

/// Host: the lobby's AIs (local lord indexes) for the wire, and its customs table.
pub fn to_wire(ais: &[(usize, i32)]) -> (Vec<(usize, i32)>, Vec<(String, u64)>) {
    let mut customs: Vec<(String, u64)> = vec![];
    let ais = ais.iter().map(|&(lord, team)| match custom_lords::of_lord(lord) {
        Some(c) => {
            let key = (c.name.clone(), checksum(&c));
            let i = customs.iter().position(|k| *k == key).unwrap_or_else(|| { customs.push(key); customs.len() - 1 });
            (BASE + i, team)
        }
        None => (lord, team),
    }).collect();
    (ais, customs)
}

/// Lobby lord index of a custom lord this machine lacks: shown by name, cannot start.
const MISSING: usize = BASE + 900;
static MISSING_NAMES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Any machine: the wire AIs as local lord indexes; custom lords this machine lacks become `MISSING + i`.
pub fn from_wire(ais: &[(usize, i32)], customs: &[(String, u64)]) -> Vec<(usize, i32)> {
    *MISSING_NAMES.lock().unwrap() = customs.iter().map(|c| c.0.clone()).collect();
    ais.iter().map(|&(lord, team)| match lord.checked_sub(BASE).and_then(|i| customs.get(i)) {
        Some((name, sum)) => (find(name, *sum).map_or(MISSING + (lord - BASE), |k| BASE + k), team),
        None => (lord, team),
    }).collect()
}

/// The lobby row's name of lord index `n` ≥ BASE: the custom lord's, or "<name> (missing)".
pub fn name_of(n: usize) -> String {
    if let Some(c) = custom_lords::of_lord(n).filter(|_| n < MISSING) { return c.name; }
    let i = n.saturating_sub(MISSING);
    format!("{} (missing)", MISSING_NAMES.lock().unwrap().get(i).cloned().unwrap_or_else(|| "custom lord".into()))
}

/// The customs this machine lacks (by name), for `Msg::Have`.
pub fn missing(customs: &[(String, u64)]) -> Vec<String> {
    let lack = || -> Vec<String> { customs.iter().filter(|(n, s)| find(n, *s).is_none()).map(|c| c.0.clone()).collect() };
    let first = lack();
    if first.is_empty() { return first; }
    custom_lords::rescan(); // installed by hand since the last scan
    lack()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A zip (stored entries) of `files` (path, bytes), the way `zip -0` writes it.
    pub fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let (mut out, mut cd) = (vec![], vec![]);
        for (name, data) in files {
            let crc = { let mut c = !0u32; for &b in *data { c ^= b as u32; for _ in 0..8 { c = if c & 1 != 0 { (c >> 1) ^ 0xedb88320 } else { c >> 1 }; } } !c };
            let off = out.len() as u32;
            let head = |sig: u32, central: bool| {
                let mut h = sig.to_le_bytes().to_vec();
                if central { h.extend(20u16.to_le_bytes()); }
                h.extend([20u16, 0, 0, 0, 0].iter().flat_map(|v| v.to_le_bytes()));
                h.extend(crc.to_le_bytes());
                h.extend((data.len() as u32).to_le_bytes());
                h.extend((data.len() as u32).to_le_bytes());
                h.extend((name.len() as u16).to_le_bytes());
                h.extend(0u16.to_le_bytes());
                if central { h.extend([0u16, 0, 0].iter().flat_map(|v| v.to_le_bytes())); h.extend(0u32.to_le_bytes()); h.extend(off.to_le_bytes()); }
                h.extend(name.as_bytes());
                h
            };
            out.extend(head(0x04034b50, false));
            out.extend(*data);
            cd.extend(head(0x02014b50, true));
        }
        let (cd_off, cd_len, n) = (out.len() as u32, cd.len() as u32, files.len() as u16);
        out.extend(cd);
        out.extend(0x06054b50u32.to_le_bytes());
        out.extend([0u16, 0, n, n].iter().flat_map(|v| v.to_le_bytes()));
        out.extend(cd_len.to_le_bytes());
        out.extend(cd_off.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    fn lordjson() -> Vec<u8> { br#"{"lord":{"opponent_type":9,"opponent_type_for_speech":9,"lord_gfx_type":0,"flag_type":13}}"#.to_vec() }
    fn castle() -> &'static [u8] { br#"{"pauseDelayAmount":100,"frames":[{"itemType":61,"tilePositionOfsets":[5643],"shouldPause":false}],"miscItems":[]}"# }

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("shc-lord-import-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn imports_and_asks_before_replacing() {
        let d = dir("ok");
        let lj = lordjson();
        let z = zip(&[("Lord T/CustomLords/Lord T/Lord T.lordjson", &lj), ("Lord T/CustomLords/Lord T/c1.aivjson", castle()),
            ("Lord T/CustomMedia/Lord T/TAUNT1.wav", b"RIFF"), ("Lord T/readme.txt", b"hi"), ("__MACOSX/._x", b"")]);
        let f = d.join("lord.zip");
        std::fs::write(&f, &z).unwrap();
        let root = d.join("profile");
        assert_eq!(import_into(&f, &root, false), Ok(vec!["Lord T".into()]));
        assert!(root.join("CustomLords/Lord T/c1.aivjson").exists() && root.join("CustomMedia/Lord T/TAUNT1.wav").exists());
        let sha = crate::update::sha256(&f).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("CustomLords/Lord T").join(SHA_FILE)).unwrap(), sha, "notes the zip it came from");
        // published: the same zip = up to date, another one = update, not installed = get
        let pub_ = |sha: &str| Online { name: "Lord T".into(), zip: crate::update::Asset { sha: sha.into(), ..Default::default() } };
        *ONLINE.lock().unwrap() = vec![pub_(&sha)];
        assert_eq!(state_in("Lord T", &root), Some(State::Current));
        assert_eq!(state_in("Lord T", &d.join("elsewhere")), Some(State::Get));
        assert_eq!(state_in("Lord U", &root), None);
        *ONLINE.lock().unwrap() = vec![pub_("00")];
        assert_eq!(state_in("Lord T", &root), Some(State::Update));
        *ONLINE.lock().unwrap() = vec![];
        assert_eq!(import_into(&f, &root, false), Err(ImportErr::Exists(vec!["Lord T".into()])));
        std::fs::write(root.join("CustomMedia/Lord T/old.wav"), b"x").unwrap();
        assert_eq!(import_into(&f, &root, true), Ok(vec!["Lord T".into()]));
        assert!(!root.join("CustomMedia/Lord T/old.wav").exists(), "replace swaps the whole folder");
        assert!(std::fs::read_dir(&root).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with(".lord-import")));
        // the game's path (Extras' Import / a dropped zip): the result line, the Extras screen to show it, and he is in
        // the custom lords list (the Add-AI faces) at once
        crate::skirmish::set_test_profile(root.clone());
        import_ui(&f, true);
        assert_eq!(MESSAGE.lock().unwrap().clone().map(|m| m.1), Some(true));
        assert!(SHOW.swap(false, std::sync::atomic::Ordering::Relaxed) && custom_lords::find("Lord T").is_some());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Lord David's release zip (re/lindschied/pack_zip.py, uploaded to the release) imports whole, his sprite/ too.
    #[test]
    fn lord_david_zip_imports() {
        let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../re/lindschied/Lord-David-von-Lindschied.zip");
        if !f.exists() { return; }
        let root = dir("david").join("profile");
        assert_eq!(import_into(&f, &root, false), Ok(vec!["Lord David von Lindschied".into()]));
        let (l, m) = (root.join("CustomLords/Lord David von Lindschied"), root.join("CustomMedia/Lord David von Lindschied"));
        assert!(l.join("trailer.webm").exists() && l.join("face.png").exists() && l.join("sprite/sprites.tsv").exists());
        assert_eq!(std::fs::read_dir(&m).unwrap().count(), 37, "32 wav (30 lines + lobby add / kick) + 4 webm + text.txt");
        let c = custom_lords::load(&l).expect("loads");
        assert_eq!(c.aivs.len(), 8);
    }

    /// The `lords` release's lords: labelled zips, named by the label; unlabelled files (a source zip) are not lords.
    #[test]
    fn online_names() {
        let json = r#"{"tag_name":"lords","assets":[{"name":"Lord-David-von-Lindschied.zip","label":"","size":5,"digest":"sha256:aa","browser_download_url":"https://e/a.zip"},
            {"name":"x.zip","label":"Lady Anne-Marie","size":5,"digest":"sha256:bb","browser_download_url":"https://e/x.zip"},
            {"name":"notes.txt","label":"","size":5,"digest":"sha256:cc","browser_download_url":"https://e/n.txt"}]}"#;
        let v: Vec<String> = online_of(json).into_iter().map(|o| o.name).collect();
        assert_eq!(v, ["Lady Anne-Marie"]);
    }

    /// Lord David's published zip (network, 34 MB): downloaded, checked and imported, then up to date.
    /// `cargo test --release lord_from_github -- --ignored`
    #[test]
    #[ignore]
    fn lord_from_github() {
        let json = crate::update::get_json("https://api.github.com/repos/YannickAaron/Strong-Hold/releases/tags/v0.12.1").unwrap();
        let zip = crate::update::assets(&json).into_iter().find(|a| a.file == "Lord-David-von-Lindschied.zip").expect("published");
        let o = Online { name: "Lord David von Lindschied".into(), zip }; // v0.12.1's zip has no label (publish_lord.sh sets it)
        let d = dir("online");
        let f = d.join(&o.zip.file);
        crate::update::download(&o.zip, &f, &|_| {}).unwrap();
        let root = d.join("profile");
        assert_eq!(import_into(&f, &root, false), Ok(vec![o.name.clone()]));
        *ONLINE.lock().unwrap() = vec![o.clone()];
        assert_eq!(state_in(&o.name, &root), Some(State::Current));
        let bad = crate::update::Asset { sha: "0".repeat(64), ..o.zip.clone() };
        assert!(crate::update::download(&bad, &f, &|_| {}).is_err() && !f.exists(), "a wrong checksum leaves nothing");
        *ONLINE.lock().unwrap() = vec![];
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn refuses_bad_zips() {
        let d = dir("bad");
        let root = d.join("profile");
        let lj = lordjson();
        let cases: Vec<(Vec<(&str, &[u8])>, &str)> = vec![
            (vec![("CustomLords/../../evil.lordjson", &lj)], "outside"),
            (vec![("CustomLords/X/run.sh", b"#!/bin/sh")], "no .sh"),
            (vec![("CustomLords/X/castle.aivjson", castle())], "no .lordjson"),
            (vec![("CustomLords/X/X.lordjson", b"not json")], "does not parse"),
            (vec![("CustomLords/X/X.lordjson", &lj), ("CustomLords/X/c.aivjson", b"{\"frames\":[]}")], "castle"),
            (vec![("CustomMedia/X/TAUNT1.wav", b"RIFF")], "no CustomLords"),
            (vec![("CustomLords/X.lordjson", &lj)], "not inside"),
            (vec![("CustomLords/X/X.lordjson", &lj), ("CustomLords/./X.lordjson", &lj)], "not inside"), // Replace would wipe CustomLords
        ];
        for (k, (files, want)) in cases.iter().enumerate() {
            let f = d.join(format!("{k}.zip"));
            std::fs::write(&f, zip(files)).unwrap();
            match import_into(&f, &root, false) { Err(ImportErr::Bad(e)) => assert!(e.contains(want), "{k}: {e}"), r => panic!("{k}: {r:?}") }
        }
        std::fs::write(d.join("x.zip"), b"PK no").unwrap();
        assert!(matches!(import_into(&d.join("x.zip"), &root, false), Err(ImportErr::Bad(_))));
        assert!(!root.exists(), "nothing written for a refused zip");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn wire_mapping() {
        let c = Custom { name: "Lord T".into(), config: vec![9; 377], aivs: vec![vec![1, 2, 3]], ..Default::default() };
        let mut other = c.clone();
        other.aivs[0][0] = 5;
        assert_ne!(checksum(&c), checksum(&other), "castles count");
        let mut look = c.clone();
        look.avatar = Some(vec![1]);
        assert_eq!(checksum(&c), checksum(&look), "looks do not");
        // a lord nobody here has: shown by name, marked missing
        let ais = from_wire(&[(3, 0), (BASE, 1)], &[("Nobody Here".into(), 1)]);
        assert_eq!(ais[0], (3, 0));
        assert!(ais[1].0 >= MISSING && name_of(ais[1].0) == "Nobody Here (missing)");
        assert_eq!(missing(&[("Nobody Here".into(), 1)]), vec!["Nobody Here".to_string()]);
    }
}
