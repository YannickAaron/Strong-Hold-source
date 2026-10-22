//! "New version available": once a day, off the UI thread, the public repo's latest release tag (GitHub API via the
//! system curl); the main menu shows a Download button when it's newer than this build. Offline / any error: nothing.
//! ponytail: notice only, no download / install (needs a Developer ID signed app first).

use std::sync::OnceLock;

/// This build's version: bundle.sh bakes SHC_VERSION (scripts/publish.sh sets it), else Cargo.toml's.
pub const VERSION: &str = match option_env!("SHC_VERSION") { Some(v) => v, None => env!("CARGO_PKG_VERSION") };
const API: &str = "https://api.github.com/repos/YannickAaron/Strong-Hold/releases/latest";
pub const PAGE: &str = "https://github.com/YannickAaron/Strong-Hold/releases/latest";

static NEWER: OnceLock<String> = OnceLock::new();

/// The newer release's tag ("v0.3.0"), once the check found one.
pub fn available() -> Option<&'static str> { NEWER.get().map(String::as_str) }

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

/// Starts the check in the background. `update_check.txt` in app_data(): "<unix secs> <tag>" of the last fetch;
/// younger than a day (and the notes saved) = its tag, no network.
pub fn start() {
    std::thread::spawn(|| {
        let file = crate::paths::app_data().join("update_check.txt");
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let cached = std::fs::read_to_string(&file).ok().filter(|_| notes_file().exists()).and_then(|s| {
            let (t, tag) = s.trim().split_once(' ')?;
            (now.saturating_sub(t.parse().ok()?) < 86_400).then(|| tag.to_string())
        });
        let tag = match cached {
            Some(t) => t,
            None => {
                let Some((t, body)) = fetch() else { return };
                let _ = std::fs::create_dir_all(file.parent().unwrap());
                let _ = std::fs::write(&file, format!("{now} {t}\n"));
                let _ = std::fs::write(notes_file(), format!("{t}\n{body}"));
                t
            }
        };
        if newer(&tag, VERSION) { let _ = NEWER.set(tag); }
    });
}

/// The latest release: (tag, body).
fn fetch() -> Option<(String, String)> {
    let out = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "10", "-H", "Accept: application/vnd.github+json", "-A", &format!("shc-viewer/{VERSION}"), API])
        .stderr(std::process::Stdio::null())
        .output().ok().filter(|o| o.status.success())?;
    let json = String::from_utf8_lossy(&out.stdout);
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
                    if (0xd800..0xdc00).contains(&u) && it.next() == Some('\\') && it.next() == Some('u') { u = 0x10000 + ((u - 0xd800) << 10) + (hex(&mut it)? - 0xdc00); }
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
fn newer(tag: &str, ours: &str) -> bool { matches!((semver(tag), semver(ours)), (Some(a), Some(b)) if a > b) }

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
}
