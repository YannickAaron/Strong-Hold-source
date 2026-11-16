//! "Strong Hold iPhone Builder" (bundle.sh --ios-builder): the Mac app with the iOS app shell in its Resources
//! (bundle_ios.sh). It runs the usual first-run setup (Steam download or this Mac's copy, the export), then packs the
//! shell + the player's own exports + the game files the game reads while playing into an unsigned .ipa. "All
//! devices": the export's atlases go in converted to ASTC (shc_extract::astc; GPUs before the A17 Pro / M1 read no BC).
//! The .ipa never leaves the Mac by us: the player signs and installs it (Sideloadly, AltStore, Xcode).
//! On iOS paths.rs reads them from the app: shc-viewer, shc-audio, game/ beside the binary.
use std::path::{Path, PathBuf};
use std::process::Command;

/// The iOS app shell this builder packs: SHC_IOS_TEMPLATE, else Contents/Resources/ios/Strong Hold.app.
/// None = the ordinary Mac app.
pub fn template() -> Option<PathBuf> {
    std::env::var_os("SHC_IOS_TEMPLATE").map(PathBuf::from)
        .or_else(|| Some(std::env::current_exe().ok()?.parent()?.parent()?.join("Resources/ios/Strong Hold.app")))
        .filter(|p| p.join("Info.plist").is_file())
}

/// The game's files read at runtime (paths::streaming_assets / game() callers), relative to the game folder.
/// ponytail: credits (no ui/credits.tsv yet) and the Crusader Trail weasel read Unity files that aren't packed: those
/// two pictures/lists stay empty on the phone.
const GAME_FILES: [&str; 5] = [
    "Stronghold Crusader Definitive Edition_Data/StreamingAssets/Maps",
    "Stronghold Crusader Definitive Edition_Data/StreamingAssets/Help",
    "Stronghold Crusader Definitive Edition_Data/StreamingAssets/CustomMediaExample",
    "Assets/GUI/Images",
    "Assets/GUI/Avatars",
];

/// After setup::ensure: the packing window, then exit.
pub fn run(template: PathBuf) -> ! {
    crate::setup::wait_more(); // the background videos belong in the app too
    // not Downloads/Desktop: macOS asks before a new app may write there (and the progress bar couldn't see the file)
    let dir = crate::paths::app_data();
    crate::setup::window::pack(move |all, p| build(&template, &dir.join(if all { "Strong Hold (all devices).ipa" } else { "Strong Hold.ipa" }), all, p));
    std::process::exit(0);
}

/// `cp -R` that clones on APFS (instant, no extra disk) and follows symlinks (shc-audio's music/speech point into
/// the game); a plain copy where cloning can't (another disk).
fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(p) = to.parent() { std::fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))?; }
    for flags in ["-cRL", "-RL"] {
        let _ = std::fs::remove_dir_all(to);
        if Command::new("cp").arg(flags).arg(from).arg(to).status().is_ok_and(|s| s.success()) { return Ok(()); }
    }
    Err(format!("couldn't copy {}", from.display()))
}

fn bytes(p: &Path) -> u64 {
    let Ok(m) = std::fs::metadata(p) else { return 0 }; // follows symlinks, like copy()
    if !m.is_dir() { return m.len(); }
    std::fs::read_dir(p).map_or(0, |d| d.flatten().map(|e| bytes(&e.path())).sum())
}

/// `all`: ASTC art (every device since 2017) instead of the export's BC (A17 Pro / M chips).
pub fn build(template: &Path, out: &Path, all: bool, progress: &mut dyn FnMut(&str, f32)) -> Result<PathBuf, String> {
    let d = crate::setup::dirs();
    let game = crate::setup::find_game().ok_or("the game's files are gone (run the setup again)")?;
    if !shc_extract::is_complete(&d.cache) { return Err("the export is incomplete (run the setup again)".into()); }
    // the conversion is kept for the next build (ponytail: not redone when the export changes; delete the folder then)
    let (astc, conv) = (crate::paths::app_data().join("shc-viewer-astc"), if all { 0.45 } else { 0.0 });
    if all { shc_extract::astc::convert(&d.cache, &astc, &mut |f| progress("Converting the art for older devices", f * conv))?; }
    let progress = &mut |l: &str, f: f32| progress(l, conv + f * (1.0 - conv));
    let stage = crate::paths::app_data().join("ipa-build");
    let _ = std::fs::remove_dir_all(&stage);
    let app = stage.join("Payload/Strong Hold.app");
    let mut parts: Vec<(PathBuf, PathBuf)> = vec![(template.to_path_buf(), app.clone()), (d.cache.clone(), app.join("shc-viewer")), (d.audio.clone(), app.join("shc-audio"))];
    parts.extend(GAME_FILES.iter().map(|f| (game.join(f), app.join("game").join(f))));
    for (i, (from, to)) in parts.iter().enumerate() {
        progress("Copying the game files", i as f32 / parts.len() as f32 * 0.1);
        copy(from, to)?;
    }
    if all { // the converted atlases over the export's (same names), and the marker the game looks for
        for sub in ["", "ui"] {
            for e in std::fs::read_dir(astc.join(sub)).map_err(|e| format!("{}: {e}", astc.display()))?.flatten().filter(|e| e.path().is_file()) {
                let to = app.join("shc-viewer").join(sub).join(e.file_name());
                let _ = std::fs::remove_file(&to); // a clone of the export's: replace, don't write through
                std::fs::copy(e.path(), &to).map_err(|err| format!("{}: {err}", to.display()))?;
            }
        }
    }
    // Finder's .DS_Store files don't belong in the app
    let _ = Command::new("find").arg(&stage).args(["-name", ".DS_Store", "-delete"]).status();

    // zip (Info-ZIP does zip64 past 4 GB); -1: the atlases are compressed already, speed wins
    let total = bytes(&stage.join("Payload")).max(1);
    let part = out.with_extension("ipa.part");
    let _ = std::fs::remove_file(&part);
    let mut zip = Command::new("zip").args(["-q", "-r", "-1", "-X"]).arg(&part).arg("Payload").current_dir(&stage)
        .spawn().map_err(|e| format!("zip: {e}"))?;
    let ok = loop {
        if let Some(s) = zip.try_wait().map_err(|e| format!("zip: {e}"))? { break s.success(); }
        // compressed output: the file grows a bit slower than the input it has read
        let f = std::fs::metadata(&part).map_or(0, |m| m.len()) as f32 / total as f32;
        progress("Packing the app", 0.1 + 0.88 * f.min(1.0));
        std::thread::sleep(std::time::Duration::from_millis(300));
    };
    let _ = std::fs::remove_dir_all(&stage);
    if !ok { let _ = std::fs::remove_file(&part); return Err("packing the app failed (is the disk full?)".into()); }
    std::fs::rename(&part, out).map_err(|e| format!("{}: {e}", out.display()))?;
    progress("Done", 1.0);
    Ok(out.to_path_buf())
}
