//! Leftovers of the removed Crusader 2 pipeline: up to 0.11 the app rendered Crusader 2's Sassanid Knight from the
//! player's own SC2 install into the export cache (re/SC2_MODELS.md). It no longer reads Crusader 2 at all; our own
//! art ships in the app (viewer loader::BUNDLED). `drop_export` takes the old rows and files back out of the cache.
use std::{fs, io, path::Path};

/// The old atlas / sprite-name prefix and its done marker.
const NAME: &str = "sc2_sassanid_knight";
const DONE: &str = "sc2.done";

/// Removes the old Crusader 2 knight from the export `cache`: its atlas, mask and sprite rows (atlases / masks /
/// sprites.tsv, rewritten atomically) and its files. True if anything was there. Cheap when nothing is: one read
/// of atlases.tsv and three file deletes.
/// ponytail: its atlas was always the last (appended; DE's re-export drops it), so no other atlas index shifts.
pub fn drop_export(cache: &Path) -> io::Result<bool> {
    let atlases = fs::read_to_string(cache.join("atlases.tsv")).unwrap_or_default();
    let ours = |l: &str| l.split('\t').nth(1) == Some(NAME);
    let mut any = false;
    if let Some(index) = atlases.lines().find(|l| ours(l)).and_then(|l| l.split('\t').next()).map(str::to_string) {
        let pre = format!("{NAME}-");
        rewrite(cache, "atlases.tsv", |l| !ours(l))?;
        rewrite(cache, "masks.tsv", |l| l.split('\t').next() != Some(index.as_str()))?;
        rewrite(cache, "sprites.tsv", |l| !l.starts_with(&pre))?;
        any = true;
    }
    for f in [format!("{NAME}.bc"), format!("{NAME}_m.bc"), DONE.to_string()] {
        match fs::remove_file(cache.join(f)) {
            Ok(()) => any = true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(any)
}

/// `name`'s lines that `keep`, written atomically (tmp + rename); a missing file stays missing.
fn rewrite(cache: &Path, name: &str, keep: impl Fn(&str) -> bool) -> io::Result<()> {
    let Ok(old) = fs::read_to_string(cache.join(name)) else { return Ok(()) };
    let s: String = old.lines().filter(|l| keep(l)).flat_map(|l| [l, "\n"]).collect();
    let tmp = cache.join(format!("{name}.tmp"));
    fs::write(&tmp, s)?;
    fs::rename(tmp, cache.join(name))
}

#[test]
fn drop_export_removes_rows_and_files() {
    let d = std::env::temp_dir().join(format!("shc_sc2_drop_{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    let w = |f: &str, s: &str| fs::write(d.join(f), s).unwrap();
    w("atlases.tsv", "0\tAllTileSprites\t8192\t8192\t25\n1\tsc2_sassanid_knight\t8192\t1760\t12\n");
    w("masks.tsv", "0\t8192\t8192\t10\n1\t8192\t1760\t10\n");
    w("sprites.tsv", "body_knight-0\t0\t1\t2\t3\t4\t0.5\t0.5\t0,0\nsc2_sassanid_knight-0\t1\t1\t2\t3\t4\t0.5\t0.5\t0,0\n");
    let files = ["sc2_sassanid_knight.bc", "sc2_sassanid_knight_m.bc", "sc2.done"];
    for f in files { w(f, "x"); }
    assert!(drop_export(&d).unwrap());
    let r = |f: &str| fs::read_to_string(d.join(f)).unwrap();
    assert_eq!((r("atlases.tsv"), r("masks.tsv")), ("0\tAllTileSprites\t8192\t8192\t25\n".to_string(), "0\t8192\t8192\t10\n".to_string()));
    assert_eq!(r("sprites.tsv"), "body_knight-0\t0\t1\t2\t3\t4\t0.5\t0.5\t0,0\n");
    assert!(files.iter().all(|f| !d.join(f).exists()));
    assert!(!drop_export(&d).unwrap(), "a second call finds nothing");
    let _ = fs::remove_dir_all(&d);
}
