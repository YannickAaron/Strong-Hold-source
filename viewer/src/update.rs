//! "New version available": at every start, off the UI thread, the public repo's latest release tag (GitHub API via
//! the system curl); the main menu shows a Download button when it's newer than this build. Offline / any error: the
//! last fetched tag (`update_check.txt`), else nothing. The Extras screen checks again on request (`check_now`) and
//! shows `status`. (Releases come several times a day: a once-a-day cache showed them a day late.)
//!
//! Install (macOS, `bundle()`): the button downloads the release's Strong-Hold-update.tar.gz (the .app without the
//! pinned DepotDownloader, ~13 MB; the DMG if that fails), checks it against the SHA-256 GitHub lists for the asset,
//! unpacks the .app next to the running one (hidden ".<name> update"), checks its version and signature, then
//! "Restart to update" swaps the folders and starts the new app once this one is gone. The DMG stays the first
//! install only. ponytail: trust = GitHub over TLS + its SHA-256; with a Developer ID, also check the Team ID.
//! Elsewhere (Windows / Linux, the app run from the DMG or Gatekeeper's translocated copy, a folder we may not write)
//! the button opens the releases page as before.

use std::sync::Mutex;

/// This build's version: bundle.sh bakes SHC_VERSION (scripts/publish.sh sets it), else Cargo.toml's.
pub const VERSION: &str = match option_env!("SHC_VERSION") { Some(v) => v, None => env!("CARGO_PKG_VERSION") };
const API: &str = "https://api.github.com/repos/YannickAaron/Strong-Hold/releases/latest";
/// The release to check / install: SHC_UPDATE_API (tests: a file:// release JSON) wins; then nothing is cached.
fn api() -> String { std::env::var("SHC_UPDATE_API").unwrap_or_else(|_| API.into()) }
pub const PAGE: &str = "https://github.com/YannickAaron/Strong-Hold/releases/latest";

/// Where the check stands (the Extras screen's line).
#[derive(Clone, Debug, PartialEq)]
pub enum Status { Idle, Checking, Latest(String), Newer(String), Offline }
static STATUS: Mutex<Status> = Mutex::new(Status::Idle);

pub fn status() -> Status { STATUS.lock().unwrap().clone() }
/// The newer release's tag ("v0.3.0"), once a check found one.
pub fn available() -> Option<String> { if let Status::Newer(t) = status() { Some(t) } else { None } }

/// `release_notes.txt` in app_data(): the latest release's tag on the first line, its notes (Markdown) below.
fn notes_file() -> std::path::PathBuf { crate::paths::app_data().join("release_notes.txt") }

/// Options > View Patch Notes: the latest release's (tag, notes), as the last check fetched them.
pub fn notes() -> Option<(String, String)> {
    static N: std::sync::Mutex<Option<(std::time::SystemTime, Option<(String, String)>)>> = std::sync::Mutex::new(None);
    let t = std::fs::metadata(notes_file()).and_then(|m| m.modified()).ok()?;
    let mut n = N.lock().unwrap();
    if n.as_ref().is_none_or(|c| c.0 != t) { // read once per change (the screen asks every frame)
        let s = std::fs::read_to_string(notes_file()).ok()?;
        let (tag, body) = s.split_once('\n').unwrap_or((&s, ""));
        *n = Some((t, Some((tag.trim().to_string(), body.to_string()))));
    }
    n.as_ref()?.1.clone()
}

/// Starts the check in the background. `update_check.txt` in app_data(): "<unix secs> <tag>" of the last fetch, the
/// answer when GitHub cannot be reached (and the notes saved).
pub fn start() { check() }
/// Extras > Check for updates: the same check (not while one runs).
pub fn check_now() { check() }

fn check() {
    let test = std::env::var_os("SHC_UPDATE_API").is_some();
    {
        let mut s = STATUS.lock().unwrap();
        if *s == Status::Checking { return; }
        *s = Status::Checking;
    }
    std::thread::spawn(move || {
        let file = crate::paths::app_data().join("update_check.txt");
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let cached = || std::fs::read_to_string(&file).ok().filter(|_| notes_file().exists()).and_then(|s| Some(s.trim().split_once(' ')?.1.to_string()));
        let tag = match fetch() {
            Some((t, body)) => {
                if !test {
                    let _ = std::fs::create_dir_all(file.parent().unwrap());
                    let _ = std::fs::write(&file, format!("{now} {t}\n"));
                    let _ = std::fs::write(notes_file(), format!("{t}\n{body}"));
                }
                t
            }
            None => match if test { None } else { cached() } { Some(t) => t, None => { *STATUS.lock().unwrap() = Status::Offline; return } },
        };
        *STATUS.lock().unwrap() = if newer(&tag, VERSION) { Status::Newer(tag) } else { Status::Latest(tag) };
    });
}

/// The latest release: (tag, body).
fn fetch() -> Option<(String, String)> {
    let json = get_json(&api())?;
    Some((tag_name(&json)?, json_str(&json, "body").unwrap_or_default()))
}

/// `"tag_name": "v1.2.3"` out of the release JSON.
fn tag_name(json: &str) -> Option<String> { json_str(json, "tag_name") }

/// The string value of top-level-ish `"key"` in a JSON text, unescaped (\n, \", \\, \uXXXX incl. surrogate pairs).
/// ponytail: the first `"key":` anywhere wins, which is enough for GitHub's release object.
fn json_str(json: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let rest = json[json.find(&pat)? + pat.len()..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let (mut out, mut it) = (String::new(), rest.chars());
    while let Some(c) = it.next() {
        match c {
            '"' => return Some(out),
            '\\' => match it.next()? {
                'n' => out.push('\n'), 't' => out.push('\t'), 'r' => {}, 'b' | 'f' => {},
                'u' => {
                    let hex = |it: &mut std::str::Chars| u32::from_str_radix(&it.by_ref().take(4).collect::<String>(), 16).ok();
                    let mut u = hex(&mut it)?;
                    if (0xd800..0xdc00).contains(&u) && it.next() == Some('\\') && it.next() == Some('u') { u = 0x10000 + ((u - 0xd800) << 10) + hex(&mut it)?.checked_sub(0xdc00)?; }
                    out.push(char::from_u32(u).unwrap_or('\u{fffd}'));
                }
                e => out.push(e),
            },
            c => out.push(c),
        }
    }
    None
}

/// "v1.2.3" -> (1, 2, 3); a "-beta" suffix or missing parts are ignored / 0.
fn semver(v: &str) -> Option<(u64, u64, u64)> {
    let mut p = v.trim().trim_start_matches('v').split(['-', '+']).next()?.split('.').map(|n| n.parse::<u64>());
    Some((p.next()?.ok()?, p.next().unwrap_or(Ok(0)).ok()?, p.next().unwrap_or(Ok(0)).ok()?))
}

/// Whether release `tag` is newer than `ours`; anything unparsable = no.
pub fn newer(tag: &str, ours: &str) -> bool { matches!((semver(tag), semver(ours)), (Some(a), Some(b)) if a > b) }

/// The in-app install's state (the update buttons' label, `button`).
#[derive(Clone, Debug, PartialEq)]
pub enum Install { Idle, Busy(String, f32), Ready(String), Failed(String) }
static INSTALL: Mutex<Install> = Mutex::new(Install::Idle);
pub fn install_state() -> Install { INSTALL.lock().unwrap().clone() }
fn set(i: Install) { *INSTALL.lock().unwrap() = i; }

/// The update button's text for release `tag`, as the install stands.
pub fn button(tag: &str) -> String {
    let v = tag.trim_start_matches('v');
    match install_state() {
        Install::Busy(step, p) if p >= 0.0 => format!("{step} {v}\u{2026} {:.0} %", p * 100.0),
        Install::Busy(step, _) => format!("{step} {v}\u{2026}"),
        Install::Ready(t) => format!("Restart to update to {}", t.trim_start_matches('v')),
        Install::Failed(_) => "Update failed \u{2014} open the download page".into(),
        Install::Idle if bundle().is_some() => format!("Version {v} is available \u{2014} Update"),
        Install::Idle => format!("Version {v} is available \u{2014} Download"),
    }
}

/// The update button: install (or open the page where we can't), then restart once it's ready. True = quit now
/// (the new app starts once this one is gone).
pub fn click() -> bool {
    match install_state() {
        Install::Busy(..) => false,
        Install::Ready(_) => match apply() {
            Ok(()) => true,
            Err(e) => { set(Install::Failed(e)); false }
        },
        Install::Failed(_) => { set(Install::Idle); open_page(); false }
        Install::Idle => {
            match bundle() {
                Some(app) => { set(Install::Busy("Downloading".into(), 0.0)); std::thread::spawn(move || set(match stage(&app) { Ok(t) => Install::Ready(t), Err(e) => { eprintln!("update: {e}"); Install::Failed(e) } })); }
                None => open_page(),
            }
            false
        }
    }
}

/// The running .app, when this build can replace itself: macOS, started from a bundle that is not Gatekeeper's
/// translocated (read-only) copy nor on the DMG, in a folder we may write.
pub fn bundle() -> Option<std::path::PathBuf> {
    if !cfg!(target_os = "macos") { return None; }
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let app = exe.ancestors().nth(3)?; // <x>.app/Contents/MacOS/shc-viewer
    let ok = app.extension().is_some_and(|e| e == "app") && !app.to_string_lossy().contains("/AppTranslocation/") && writable(app) && writable(app.parent()?);
    ok.then(|| app.to_path_buf())
}

#[cfg(unix)]
fn writable(p: &std::path::Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(p.as_os_str().as_bytes()).unwrap_or_default();
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 } // a read-only volume (the DMG) says no too
}
#[cfg(not(unix))]
fn writable(_: &std::path::Path) -> bool { false }

/// Next to `app`: the new version waiting (".<name> update") and the old one on its way out (".<name> old").
fn beside(app: &std::path::Path, what: &str) -> std::path::PathBuf {
    app.with_file_name(format!(".{} {what}", app.file_name().unwrap_or_default().to_string_lossy()))
}

/// The release's update archive: the .app without DepotDownloader (pinned, 85 MB, the same in every release; the
/// running app's copy goes back in, and the signature check proves it's the one the build sealed).
const ARCHIVE: &str = "Strong-Hold-update.tar.gz";

/// Gets the latest release's app to `beside(app, "update")`, checked: the small archive, else (an older release, a
/// new DepotDownloader) the DMG. Returns the tag.
fn stage(app: &std::path::Path) -> Result<String, String> {
    let json = get_json(&api()).ok_or("couldn't reach GitHub")?;
    let tag = tag_name(&json).ok_or("no release found")?;
    let list = assets(&json);
    let dir = crate::paths::app_data().join("update");
    let staged = beside(app, "update");
    let _ = std::fs::remove_dir_all(&dir);
    let small = list.iter().find(|a| a.file == ARCHIVE).ok_or_else(|| "no update archive".to_string())
        .and_then(|a| from_archive(a, app, &dir, &staged)).and_then(|_| verify(&staged, &tag));
    let done = small.or_else(|e| {
        eprintln!("update: {e}; the DMG instead");
        let dmg = list.iter().find(|a| a.file == "Strong-Hold.dmg").ok_or("the release has no Strong-Hold.dmg")?;
        from_dmg(dmg, &dir, &staged).and_then(|_| verify(&staged, &tag))
    });
    let _ = std::fs::remove_dir_all(&dir);
    if done.is_err() { let _ = std::fs::remove_dir_all(&staged); }
    done.map(|_| tag)
}

fn from_archive(a: &Asset, app: &std::path::Path, dir: &std::path::Path, staged: &std::path::Path) -> Result<(), String> {
    let file = dir.join(&a.file);
    download(a, &file, &|p| set(Install::Busy("Downloading".into(), p)))?;
    set(Install::Busy("Installing".into(), -1.0));
    let x = dir.join("x");
    std::fs::create_dir_all(&x).map_err(|e| e.to_string())?;
    run("tar", &["-xzf".as_ref(), file.as_os_str(), "-C".as_ref(), x.as_os_str()])?;
    let new = app_in(&x)?;
    let dd = "Contents/MacOS/DepotDownloader";
    if !new.join(dd).exists() { std::fs::copy(app.join(dd), new.join(dd)).map_err(|e| format!("DepotDownloader: {e}"))?; }
    let _ = std::fs::remove_dir_all(staged);
    run("ditto", &[new.as_os_str(), staged.as_os_str()])
}

fn from_dmg(a: &Asset, dir: &std::path::Path, staged: &std::path::Path) -> Result<(), String> {
    let file = dir.join(&a.file);
    download(a, &file, &|p| set(Install::Busy("Downloading".into(), p)))?;
    set(Install::Busy("Installing".into(), -1.0));
    let mnt = dir.join("mnt");
    std::fs::create_dir_all(&mnt).map_err(|e| e.to_string())?;
    run("hdiutil", &["attach".as_ref(), "-nobrowse".as_ref(), "-readonly".as_ref(), "-noautoopen".as_ref(), "-mountpoint".as_ref(), mnt.as_os_str(), file.as_os_str()])?;
    let copied = app_in(&mnt).and_then(|src| { let _ = std::fs::remove_dir_all(staged); run("ditto", &[src.as_os_str(), staged.as_os_str()]) });
    let _ = run("hdiutil", &["detach".as_ref(), "-force".as_ref(), mnt.as_os_str()]);
    copied
}

/// The .app in folder `d`.
fn app_in(d: &std::path::Path) -> Result<std::path::PathBuf, String> {
    std::fs::read_dir(d).map_err(|e| e.to_string())?.flatten().map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "app")).ok_or_else(|| "no app in the download".into())
}

/// The staged app is release `tag`'s and its signature seals every file (DepotDownloader included).
fn verify(staged: &std::path::Path, tag: &str) -> Result<(), String> {
    let plist = std::fs::read_to_string(staged.join("Contents/Info.plist")).unwrap_or_default();
    let want = format!("<key>CFBundleShortVersionString</key><string>{}</string>", tag.trim_start_matches('v'));
    if !plist.contains(&want) { return Err(format!("the download holds another version than {tag}")); }
    run("codesign", &["--verify".as_ref(), "--deep".as_ref(), "--strict".as_ref(), staged.as_os_str()]).map_err(|e| format!("the new app's signature: {e}"))
}

/// Swaps the staged app in (the old one back if that fails) and starts it once this process is gone.
fn apply() -> Result<(), String> {
    let app = bundle().ok_or("can't replace the app where it is")?;
    let old = swap(&app)?;
    let pid = std::process::id().to_string();
    std::process::Command::new("/bin/sh")
        .args(["-c", r#"while kill -0 "$0" 2>/dev/null; do sleep 0.2; done; rm -rf "$1"; open "$2""#, &pid])
        .arg(&old).arg(&app).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// `beside(app, "update")` becomes `app`; the old one moves to `beside(app, "old")`, returned.
fn swap(app: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let (new, old) = (beside(app, "update"), beside(app, "old"));
    if !new.join("Contents/Info.plist").exists() { return Err("the downloaded update is gone; try again".into()); }
    let _ = std::fs::remove_dir_all(&old);
    std::fs::rename(app, &old).map_err(|e| format!("{}: {e}", app.display()))?;
    if let Err(e) = std::fs::rename(&new, app) {
        let _ = std::fs::rename(&old, app);
        return Err(format!("{}: {e}", app.display()));
    }
    Ok(old)
}

fn run(cmd: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let out = std::process::Command::new(cmd).args(args).output().map_err(|e| format!("{cmd}: {e}"))?;
    if out.status.success() { Ok(()) } else { Err(format!("{cmd}: {}", String::from_utf8_lossy(&out.stderr).trim())) }
}

/// A release asset: file name, label (GitHub's display name, may be empty), size, SHA-256 (hex), download URL.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Asset { pub file: String, pub label: String, pub size: u64, pub sha: String, pub url: String }

/// A release's assets. ponytail: each asset's keys sit between two "browser_download_url"s (GitHub's key order;
/// the uploader object inside has no name / size / digest).
pub fn assets(json: &str) -> Vec<Asset> {
    let Some(start) = json.find("\"assets\"") else { return vec![] };
    let json = &json[start..json.find("\"tarball_url\"").filter(|&e| e > start).unwrap_or(json.len())];
    let parts: Vec<&str> = json.split("\"browser_download_url\"").collect();
    parts.windows(2).filter_map(|w| {
        let num = |key: &str| -> Option<u64> {
            let pat = format!("\"{key}\"");
            let r = w[0][w[0].find(&pat)? + pat.len()..].trim_start().strip_prefix(':')?.trim_start();
            r[..r.find(|c: char| !c.is_ascii_digit())?].parse().ok()
        };
        Some(Asset {
            file: json_str(w[0], "name")?,
            label: json_str(w[0], "label").unwrap_or_default(),
            size: num("size").unwrap_or(0),
            sha: json_str(w[0], "digest")?.strip_prefix("sha256:")?.to_ascii_lowercase(),
            url: json_str(&format!("\"u\"{}", w[1]), "u")?,
        })
    }).collect()
}

/// GET a GitHub API URL (the JSON), None offline / not found.
pub fn get_json(url: &str) -> Option<String> {
    let out = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "10", "-H", "Accept: application/vnd.github+json", "-A", &format!("shc-viewer/{VERSION}"), url])
        .stderr(std::process::Stdio::null())
        .output().ok().filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Downloads `a` to `to`, `progress` 0..1 meanwhile; Err (and nothing left) unless its SHA-256 is the listed one.
pub fn download(a: &Asset, to: &std::path::Path, progress: &dyn Fn(f32)) -> Result<(), String> {
    let _ = std::fs::create_dir_all(to.parent().unwrap());
    let mut c = std::process::Command::new("curl")
        .args(["-fsSL", "--retry", "2", "--connect-timeout", "15", "--speed-limit", "1000", "--speed-time", "60", "-A", &format!("shc-viewer/{VERSION}"), "-o"])
        .arg(to).arg(&a.url).stderr(std::process::Stdio::null()).spawn().map_err(|e| format!("curl: {e}"))?;
    let ok = loop {
        if let Some(s) = c.try_wait().map_err(|e| e.to_string())? { break s.success(); }
        if a.size > 0 { progress((std::fs::metadata(to).map_or(0, |m| m.len()) as f32 / a.size as f32).min(1.0)); }
        std::thread::sleep(std::time::Duration::from_millis(200));
    };
    let fail = |e: &str| { let _ = std::fs::remove_file(to); Err(e.to_string()) };
    if !ok { return fail("the download failed (offline?)"); }
    match sha256(to) {
        Some(h) if h == a.sha => Ok(()),
        Some(_) => fail("the download is damaged (checksum mismatch)"),
        None => fail("couldn't check the download (no shasum)"),
    }
}

/// A file's SHA-256 (lowercase hex), via the system's shasum / sha256sum.
pub fn sha256(f: &std::path::Path) -> Option<String> {
    [("shasum", &["-a", "256"][..]), ("sha256sum", &[][..])].iter().find_map(|(cmd, args)| {
        let out = std::process::Command::new(cmd).args(*args).arg(f).stderr(std::process::Stdio::null()).output().ok().filter(|o| o.status.success())?;
        let h = String::from_utf8_lossy(&out.stdout).split_whitespace().next()?.to_ascii_lowercase();
        (h.len() == 64).then_some(h)
    })
}

/// Opens the releases page in the browser.
pub fn open_page() {
    let cmd = if cfg!(target_os = "macos") { "open" } else if cfg!(windows) { "explorer" } else { "xdg-open" };
    let _ = std::process::Command::new(cmd).arg(PAGE).spawn();
}

#[test]
fn version_compare() {
    assert!(newer("v0.2.0", "0.1.0"));
    assert!(newer("v1.0.0", "0.9.9"));
    assert!(newer("v0.1.10", "0.1.9"));
    assert!(newer("0.2", "0.1.5"));
    assert!(!newer("v0.1.0", "0.1.0"));
    assert!(!newer("v0.1.0", "0.2.0"));
    assert!(!newer("v0.2.0-beta", "0.2.0"));
    assert!(!newer("nightly", "0.1.0"));
    assert_eq!(tag_name(r#"{"url":"x","tag_name" : "v0.3.1","name":"y"}"#).as_deref(), Some("v0.3.1"));
    assert_eq!(tag_name(r#"{"message":"Not Found"}"#), None);
    let json = format!(r#"{{"body": "New\r\n- \"Save\" dialog \\ {}e9 {}d83d{}de00"}}"#, "\\u00", "\\u", "\\u");
    assert_eq!(json_str(&json, "body").as_deref(), Some("New\n- \"Save\" dialog \\ \u{e9} \u{1f600}"));
    assert_eq!(json_str(r#"{"body": "\ud83d\u0041"}"#, "body"), None); // a broken surrogate pair: no panic
}

/// GitHub's release JSON (keys in its order, an uploader object inside each asset): the assets with their SHA-256.
/// The start check asks the release API every time (no day-old cache): a newer tag shows at once; unreachable = Offline.
#[test]
fn start_asks_every_time() {
    let wait = || { for _ in 0..200 { if status() != Status::Checking { break } std::thread::sleep(std::time::Duration::from_millis(50)); } status() };
    let f = std::env::temp_dir().join(format!("shc-update-{}.json", std::process::id()));
    std::fs::write(&f, r#"{"tag_name":"v99.0.0","body":"notes"}"#).unwrap();
    std::env::set_var("SHC_UPDATE_API", format!("file://{}", f.display()));
    start();
    assert_eq!(wait(), Status::Newer("v99.0.0".into()));
    std::fs::write(&f, r#"{"tag_name":"v0.0.1","body":""}"#).unwrap();
    start();
    assert_eq!(wait(), Status::Latest("v0.0.1".into()), "the second start fetched again");
    std::fs::remove_file(&f).unwrap();
    start();
    assert_eq!(wait(), Status::Offline);
    std::env::remove_var("SHC_UPDATE_API");
}

#[test]
fn release_assets() {
    let up = r#""uploader":{"login":"x","id":1,"url":"https://api.github.com/users/x","type":"User"}"#;
    let json = format!(r#"{{"tag_name":"v0.14.0","name":"Strong Hold 0.14.0","assets":[
        {{"url":"a","id":1,"name":"Strong-Hold.dmg","label":"",{up},"content_type":"application/x-apple-diskimage","size":49840164,"digest":"sha256:4261AB","browser_download_url":"https://github.com/o/r/releases/download/v0.14.0/Strong-Hold.dmg"}},
        {{"url":"b","id":2,"name":"Lord-X.zip","label":"Lord X-Y",{up},"size":12,"digest":"sha256:ff","browser_download_url":"https://e/Lord-X.zip"}},
        {{"url":"c","id":3,"name":"old.zip","label":null,{up},"size":3,"digest":null,"browser_download_url":"https://e/old.zip"}}],
        "tarball_url":"t","body":"see \"browser_download_url\": x"}}"#);
    let a = assets(&json);
    assert_eq!(a.len(), 2, "no digest = not offered: {a:?}");
    assert_eq!(a[0], Asset { file: "Strong-Hold.dmg".into(), label: "".into(), size: 49840164, sha: "4261ab".into(), url: "https://github.com/o/r/releases/download/v0.14.0/Strong-Hold.dmg".into() });
    assert_eq!((a[1].label.as_str(), a[1].size, a[1].url.as_str()), ("Lord X-Y", 12, "https://e/Lord-X.zip"));
    assert!(assets(r#"{"message":"Not Found"}"#).is_empty());
}

/// "Restart to update": the staged app takes the bundle's place, the old one moves aside; nothing staged = refused,
/// the bundle untouched.
#[test]
fn swap_bundles() {
    let d = std::env::temp_dir().join(format!("shc-update-swap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let app = d.join("Strong Hold.app");
    std::fs::create_dir_all(app.join("Contents")).unwrap();
    std::fs::write(app.join("Contents/Info.plist"), "old").unwrap();
    assert!(swap(&app).is_err() && std::fs::read_to_string(app.join("Contents/Info.plist")).unwrap() == "old");
    let new = beside(&app, "update");
    assert_eq!(new, d.join(".Strong Hold.app update"));
    std::fs::create_dir_all(new.join("Contents")).unwrap();
    std::fs::write(new.join("Contents/Info.plist"), "new").unwrap();
    let old = swap(&app).unwrap();
    assert_eq!(std::fs::read_to_string(app.join("Contents/Info.plist")).unwrap(), "new");
    assert_eq!(std::fs::read_to_string(old.join("Contents/Info.plist")).unwrap(), "old");
    assert!(!new.exists());
    let _ = std::fs::remove_dir_all(&d);
}

/// The whole install against the real latest release (network, ~50 MB): download, SHA-256, DMG, copy, version and
/// signature, then the swap. `cargo test --release update_from_github -- --ignored --nocapture`
#[test]
#[ignore]
#[cfg(target_os = "macos")]
fn update_from_github() {
    let d = std::env::temp_dir().join(format!("shc-update-real-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let app = d.join("Strong Hold.app");
    std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    let tag = stage(&app).unwrap();
    println!("staged {tag}");
    swap(&app).unwrap();
    assert!(app.join("Contents/MacOS/shc-viewer").exists());
    run("codesign", &["--verify".as_ref(), "--deep".as_ref(), "--strict".as_ref(), app.as_os_str()]).unwrap();
    let _ = std::fs::remove_dir_all(&d);
}
