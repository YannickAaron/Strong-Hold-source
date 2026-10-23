//! = re/scripts/export_atlases.py: sprite atlases as raw BC blocks + atlases/masks/sprites.tsv (formats: that docstring).
use crate::classes::{self, SPRITE, TEXTURE2D};
use crate::unity::{read_range, SFile};
use crate::{fmt_g, Result};
use std::{collections::HashMap, fmt::Write as _, fs, path::Path};

/// Bytes per 4x4 block of the BC formats the atlases use (25 BC7, 10 DXT1, 12 DXT5).
pub fn block_bytes(format: i32) -> Option<usize> { match format { 25 | 12 => Some(16), 10 => Some(8), _ => None } }

/// Level-0 block data of a streamed texture, from the .resS next to the asset file.
fn stream_blob(data: &Path, t: &classes::Texture2D, bpb: usize) -> Result<Vec<u8>> {
    let size = (t.w / 4) * (t.h / 4) * bpb;
    let (off, len, path) = &t.stream;
    if size > *len as usize { return Err(format!("{}: stream too short", t.name).into()); }
    Ok(read_range(&data.join(path.rsplit('/').next().unwrap_or(path)), *off, size)?)
}

pub fn export(data: &Path, out: &Path) -> Result<()> {
    fs::create_dir_all(out)?;
    let f = SFile::open(&data.join("resources.assets"))?;
    let sprites: Vec<classes::Sprite> = f.each(SPRITE).map(|r| r.map(|(_, b)| classes::sprite(&b))).collect::<std::io::Result<_>>()?;
    let used: std::collections::HashSet<i64> = sprites.iter().map(|s| s.texture).collect();
    let (mut atlas_index, mut names, mut atlases, mut masks) = (HashMap::new(), HashMap::new(), String::new(), String::new());
    let mut texes = vec![];
    for r in f.each(TEXTURE2D) {
        let (o, b) = r?;
        let t = classes::texture2d(&b);
        if used.contains(&o.path_id) && !t.name.ends_with("_m") && t.w >= 1024 {
            if let Some(bpb) = block_bytes(t.format) {
                fs::write(out.join(format!("{}.bc", t.name)), stream_blob(data, &t, bpb)?)?;
                let i = atlas_index.len();
                atlas_index.insert(o.path_id, i);
                names.insert(t.name.clone(), i);
                writeln!(atlases, "{i}\t{}\t{}\t{}\t{}", t.name, t.w, t.h, t.format)?;
            }
        }
        texes.push(t);
    }
    for t in &texes {
        let (Some(base), Some(bpb)) = (t.name.strip_suffix("_m").and_then(|n| names.get(n)), block_bytes(t.format)) else { continue };
        fs::write(out.join(format!("{}.bc", t.name)), stream_blob(data, t, bpb)?)?;
        writeln!(masks, "{base}\t{}\t{}\t{}", t.w, t.h, t.format)?;
    }
    fs::write(out.join("masks.tsv"), masks)?;
    fs::write(out.join("atlases.tsv"), atlases)?;
    let mut rows = String::with_capacity(16 << 20);
    for s in &sprites {
        let Some(a) = atlas_index.get(&s.texture) else { continue };
        let [x, y, w, h] = s.rect.map(|v| fmt_g(v as f64));
        let mesh: Vec<String> = s.tris.iter().map(|&i| {
            let v = s.verts.get(i as usize).copied().unwrap_or_default();
            format!("{},{}", fmt_g(v[0] as f64 * s.ppu as f64), fmt_g(v[1] as f64 * s.ppu as f64))
        }).collect();
        writeln!(rows, "{}\t{a}\t{x}\t{y}\t{w}\t{h}\t{:.6}\t{:.6}\t{}", s.name, s.pivot[0] as f64, s.pivot[1] as f64, mesh.join(";"))?;
    }
    fs::write(out.join("sprites.tsv"), rows)?;
    crate::sc2::invalidate(out); // the .tsv files no longer hold the SC2 rows
    Ok(())
}
