//! Extended rules (ours: shc_sim::ext) beside a save: the flag is not in the save file (the original must load it),
//! so `<save>.ext` next to it says the game was Extended. Like King of the Hill's `.hill` (koth.rs).
use std::path::{Path, PathBuf};

fn sidecar(save: &Path) -> PathBuf {
    let mut s = save.as_os_str().to_owned();
    s.push(".ext");
    s.into()
}

/// Written with the save when on; an off save removes an old one (a reused name).
pub fn write_beside(save: &Path, on: bool) -> std::io::Result<()> {
    if on { return crate::paths::write_atomic(&sidecar(save), "1"); }
    match std::fs::remove_file(sidecar(save)) { Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e), _ => Ok(()) }
}

pub fn read_beside(save: &Path) -> bool { sidecar(save).exists() }

/// A loaded save's rules: on with its sidecar, or when its units / buildings carry our markers (a save copied
/// without the sidecar must not play half-Sassanid).
#[cfg(feature = "live")]
pub fn after_load(m: &mut shc_sim::mem::Mem, save: &Path) {
    if read_beside(save) { shc_sim::ext::set(m, true); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_roundtrip() {
        let dir = std::env::temp_dir().join(format!("shc-ext-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let save = dir.join("Game.sav");
        write_beside(&save, true).unwrap();
        assert!(read_beside(&save) && dir.join("Game.sav.ext").exists());
        write_beside(&save, false).unwrap();
        assert!(!read_beside(&save) && !dir.join("Game.sav.ext").exists());
        write_beside(&save, false).unwrap(); // nothing to remove
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
