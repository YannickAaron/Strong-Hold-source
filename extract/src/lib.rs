//! First-run asset export from the user's own game install (replaces re/scripts/export_*.py; same files, same
//! columns, so viewer/ and audio/ read them unchanged).
pub mod atlases;
pub mod audio;
pub mod bc;
pub mod video;
pub mod vorbis;
pub mod vpx;
pub mod webm;
pub mod classes;
pub mod dll;
pub mod frontend;
pub mod tex;
pub mod ui;
pub mod unity;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

use std::path::Path;

/// Bump when an output changes: setup re-runs the export once.
pub const VERSION: &str = "1";
pub const GAME_DATA: &str = "Stronghold Crusader Definitive Edition_Data";

/// Done marker in the cache folder.
pub fn is_complete(cache: &Path) -> bool { std::fs::read_to_string(cache.join("extract.done")).is_ok_and(|v| v.trim() == VERSION) }

/// Everything the viewer, audio and sim read, from the game folder `game` (the one holding the .exe):
/// `cache` = shc-viewer's folder (atlases, ui/, video/, dll/), `audio` = shc-audio's. Steps whose files exist are
/// redone except videos (slowest; per file). `progress(step label, 0..1 overall)`.
pub fn run(game: &Path, cache: &Path, audio: &Path, progress: &mut dyn FnMut(&str, f32)) -> Result<()> {
    let data = game.join(GAME_DATA);
    // (label, share of the total time on an M1: dll 4 s, ui 1, sounds 1, sprites 8, menu art 36, videos ~100)
    let steps = [("Reading the game files", 3.0), ("Interface", 1.0), ("Sounds", 1.0), ("Sprites", 6.0), ("Menu pictures", 25.0), ("Videos", 64.0)];
    let total: f32 = steps.iter().map(|s| s.1).sum();
    let mut before = 0.0;
    for (i, (label, w)) in steps.iter().enumerate() {
        progress(label, before / total);
        match i {
            0 => dll::export(&data, &cache.join("dll"))?,
            1 => ui::export(game, &data, &cache.join("ui"))?,
            2 => { audio::export(&data, audio)?; }
            3 => atlases::export(&data, cache)?,
            4 => frontend::export(game, &data, &ui::noesis(&data)?, &cache.join("ui"))?,
            _ => video::export(&data, &cache.join("ui/font.ttf"), &cache.join("video"), &mut |d, n| progress(label, (before + w * d as f32 / n as f32) / total))?,
        }
        before += w;
    }
    progress("Done", 1.0);
    std::fs::write(cache.join("extract.done"), VERSION)?;
    Ok(())
}

/// Python's `f"{v:g}"`: 6 significant digits, trailing zeros dropped, exponent form outside 1e-4 .. 1e6.
pub fn fmt_g(v: f64) -> String {
    if v == 0.0 { return if v.is_sign_negative() { "-0".into() } else { "0".into() }; }
    if !v.is_finite() { return if v.is_nan() { "nan".into() } else if v > 0.0 { "inf".into() } else { "-inf".into() }; }
    let e = format!("{v:.5e}"); // d.ddddde±x, correctly rounded like Python
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let trim = |s: String| if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if (-4..6).contains(&exp) {
        trim(format!("{v:.*}", (5 - exp) as usize))
    } else {
        format!("{}e{}{:02}", trim(mant.to_string()), if exp < 0 { '-' } else { '+' }, exp.abs())
    }
}

#[test]
fn g_matches_python() {
    for (v, s) in [(0.0, "0"), (1.0, "1"), (123.5, "123.5"), (-64.25, "-64.25"), (1234567.0, "1.23457e+06"), (0.0001, "0.0001"),
                   (0.00001234, "1.234e-05"), (100000.0, "100000"), (99999.95, "99999.9"), (0.30000001192092896, "0.3")] {
        assert_eq!(fmt_g(v), s, "{v}");
    }
}
