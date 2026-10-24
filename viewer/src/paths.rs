//! Where the game and our own files live, per OS. The SHC_* env vars still win at the call sites (SHC_MAPS, SHC_SAVES,
//! SHC_CACHE, SHC_AUDIO, SHC_PROFILE), plus SHC_GAME for the game's install folder.
//! - Every OS: the game the first-run setup downloaded (app_data()/game) wins; the exports (shc-viewer, shc-audio) live
//!   in app_data() too, so the OS never purges them.
//! - macOS: else the game in CrossOver's "Steam" bottle; our files in ~/Library/Application Support.
//! - Linux: the game in Steam (it runs through Proton, app 3024040: install under steamapps/common, its LocalLow folder
//!   in the Proton prefix); our files in $XDG_CACHE_HOME / $XDG_DATA_HOME (~/.cache, ~/.local/share).
//! - Windows: Steam's default library, the real LocalLow; our files in %APPDATA%.
//! ponytail: Steam libraries on other drives (libraryfolders.vdf) aren't searched: set SHC_GAME.

use std::path::PathBuf;

const GAME: &str = "Stronghold Crusader Definitive Edition";
const APP: &str = "Strong Hold";
#[cfg(not(any(target_os = "macos", windows)))]
const STEAM_APP: &str = "3024040";

/// `fs::write` through a temp file and a rename: a crash or a full disk mid-write leaves the old file whole, never an
/// empty or half one (saves, settings.cfg with the campaign progress, maps).
pub fn write_atomic(path: &std::path::Path, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".part");
    std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, path)).inspect_err(|_| { let _ = std::fs::remove_file(&tmp); })
}

fn env_dir(k: &str) -> Option<PathBuf> { std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from) }
pub fn home() -> PathBuf { env_dir("HOME").or_else(|| env_dir("USERPROFILE")).unwrap_or_default() }

/// Steam's folder: the first candidate that has the game, else the first.
fn steam() -> PathBuf {
    let roots = steam_roots();
    roots.iter().find(|r| r.join("steamapps/common").join(GAME).is_dir()).unwrap_or(&roots[0]).clone()
}
fn steam_roots() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    let roots = vec![home().join("Library/Application Support/CrossOver/Bottles/Steam/drive_c/Program Files (x86)/Steam")];
    #[cfg(windows)]
    let roots = vec![PathBuf::from(r"C:\Program Files (x86)\Steam"), PathBuf::from(r"C:\Program Files\Steam")];
    #[cfg(not(any(target_os = "macos", windows)))]
    let roots = { let h = home(); vec![h.join(".steam/steam"), h.join(".local/share/Steam"), h.join(".var/app/com.valvesoftware.Steam/.local/share/Steam")] };
    roots
}

/// The game's install folder: SHC_GAME, else the setup's download, else Steam's (CrossOver's on macOS).
pub fn game() -> PathBuf {
    let has = |g: &PathBuf| g.join(format!("{GAME}_Data")).is_dir();
    env_dir("SHC_GAME").or_else(|| Some(downloaded()).filter(has))
        .or_else(|| std::fs::read_to_string(chosen_file()).ok().map(|s| PathBuf::from(s.trim())).filter(has))
        .unwrap_or_else(|| steam().join("steamapps/common").join(GAME))
}
/// The game folder the user picked in setup ("I have the game files…"), one line.
pub fn chosen_file() -> PathBuf { app_data().join("game_path.txt") }
/// Where the first-run setup puts the game it downloads from Steam.
pub fn downloaded() -> PathBuf { app_data().join("game") }
/// Steam's copy of the game, if installed there (macOS: in the CrossOver bottle "Steam").
pub fn steam_game() -> Option<PathBuf> { Some(steam().join("steamapps/common").join(GAME)).filter(|g| g.join(format!("{GAME}_Data")).is_dir()) }
/// The game's StreamingAssets: Maps, Help, ...
pub fn streaming_assets() -> PathBuf { game().join(format!("{GAME}_Data/StreamingAssets")) }

/// The real game's own folder (Unity's LocalLow: settings.cfg, Saves, user Maps).
pub fn game_user() -> PathBuf {
    let tail = format!("AppData/LocalLow/Firefly Studios/{GAME}");
    #[cfg(target_os = "macos")]
    return home().join("Library/Application Support/CrossOver/Bottles/Steam/drive_c/users/crossover").join(tail);
    #[cfg(windows)]
    return home().join(tail);
    #[cfg(not(any(target_os = "macos", windows)))]
    return steam().join(format!("steamapps/compatdata/{STEAM_APP}/pfx/drive_c/users/steamuser")).join(tail);
}

/// Our export folder `name` (shc-viewer, shc-audio): written once by the first-run setup.
pub fn cache(name: &str) -> PathBuf { app_data().join(name) }

/// Our own data folder (settings.cfg, campaign progress, saves).
pub fn app_data() -> PathBuf {
    #[cfg(target_os = "macos")]
    return home().join("Library/Application Support").join(APP);
    #[cfg(windows)]
    return env_dir("APPDATA").unwrap_or_else(|| home().join("AppData/Roaming")).join(APP);
    #[cfg(not(any(target_os = "macos", windows)))]
    return env_dir("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local/share")).join(APP);
}

/// Started from the .app (Finder): what the bundle's shell script did before 0.8.1 (a script in the bundle made
/// Microsoft Defender's ML guess "trojan"): move the pre-rename folder over, saves in app_data()/Saves, stdout and
/// stderr into ~/Library/Logs/Strong Hold.log. Set env vars win (tests); `cargo run` keeps the terminal.
#[cfg(target_os = "macos")]
pub fn app_bundle_start() {
    use std::os::fd::AsRawFd;
    if !std::env::current_exe().is_ok_and(|e| e.to_string_lossy().contains(".app/Contents/MacOS/")) { return; }
    // Renamed from "Stronghold Crusader (Rust)" after v0.1.1, per entry so an existing "Strong Hold" folder doesn't
    // block it (ponytail: drop after a few more releases)
    let (old, new) = (home().join("Library/Application Support/Stronghold Crusader (Rust)"), app_data());
    if let Ok(rd) = std::fs::read_dir(&old) {
        let _ = std::fs::create_dir_all(&new);
        for e in rd.flatten() { if !new.join(e.file_name()).exists() { let _ = std::fs::rename(e.path(), new.join(e.file_name())); } }
        let _ = std::fs::remove_file(old.join("update_check.txt"));
        let _ = std::fs::remove_dir(&old);
    }
    if std::env::var_os("SHC_SAVES").is_none() { std::env::set_var("SHC_SAVES", app_data().join("Saves")); }
    let _ = std::fs::create_dir_all(std::env::var_os("SHC_SAVES").unwrap());
    let logs = home().join("Library/Logs");
    let _ = std::fs::create_dir_all(&logs);
    if let Ok(f) = std::fs::File::create(logs.join("Strong Hold.log")) {
        unsafe extern "C" { fn dup2(from: i32, to: i32) -> i32; }
        unsafe { dup2(f.as_raw_fd(), 1); dup2(f.as_raw_fd(), 2); } // the fds stay open after `f` drops
    }
}
#[cfg(not(target_os = "macos"))]
pub fn app_bundle_start() {}

/// Tests: a save of the real game's (e.g. "Crater Lake-1test.sav").
#[allow(dead_code)]
pub fn test_save(name: &str) -> String { game_user().join("Saves").join(name).to_string_lossy().into_owned() }

/// macOS keeps today's paths exactly.
#[test]
#[cfg(target_os = "macos")]
fn mac_paths_unchanged() {
    let h = home().to_string_lossy().into_owned();
    let bottle = format!("{h}/Library/Application Support/CrossOver/Bottles/Steam/drive_c");
    if std::env::var_os("SHC_GAME").is_none() && !downloaded().is_dir() {
        assert_eq!(streaming_assets().join("Maps"), PathBuf::from(format!("{bottle}/Program Files (x86)/Steam/steamapps/common/{GAME}/{GAME}_Data/StreamingAssets/Maps")));
    }
    assert_eq!(game_user().join("Saves"), PathBuf::from(format!("{bottle}/users/crossover/AppData/LocalLow/Firefly Studios/{GAME}/Saves")));
    assert_eq!(cache("shc-viewer"), PathBuf::from(format!("{h}/Library/Application Support/{APP}/shc-viewer")));
    assert_eq!(app_data(), PathBuf::from(format!("{h}/Library/Application Support/{APP}")));
}
