//! The iPhone Builder's "all devices" art: the export's BC atlases re-encoded as ASTC for iPhones / iPads whose GPU
//! can't read BC (everything before the A17 Pro / M1). Colour (BC7, the UI's with its mips) -> ASTC 4×4 sRGB: the
//! same 16 bytes per 4×4 block, so the same size. Team-colour masks (BC1 / BC3) -> ASTC 8×8: a quarter of BC1's
//! bytes per pixel (4×4 would double them; only 4 and 8 divide the atlas sizes). Decoded with texture2ddecoder,
//! encoded with Arm's astcenc on every core. `MARKER` in the output folder tells the viewer (texfmt.rs).
use std::path::Path;
use ctt_astcenc::{bindings::*, config_init, Context, Flags, Preset, Profile, Swizzle};

/// The viewer reads the folder as ASTC when this file is in it.
pub const MARKER: &str = "astc.txt";

enum Bc { Bc1, Bc3, Bc7 }

/// One BC image (`w`×`h`, blocks row-major) -> ASTC `block`×`block` bytes.
fn image(ctx: &mut Context, data: &[u8], bc: &Bc, w: u32, h: u32, block: u32) -> Result<Vec<u8>, String> {
    // under one block (the last mips): decode / encode a whole padded block
    let (pw, ph) = (w.max(4).next_multiple_of(4), h.max(4).next_multiple_of(4));
    let mut px = vec![0u32; (pw * ph) as usize];
    let (pw_, ph_) = (pw as usize, ph as usize);
    match bc {
        Bc::Bc1 => texture2ddecoder::decode_bc1(data, pw_, ph_, &mut px),
        Bc::Bc3 => texture2ddecoder::decode_bc3(data, pw_, ph_, &mut px),
        Bc::Bc7 => texture2ddecoder::decode_bc7(data, pw_, ph_, &mut px),
    }.map_err(|e| e.to_string())?;
    let (ew, eh) = (pw.max(block).next_multiple_of(block), ph.max(block).next_multiple_of(block));
    let mut rgba = vec![0u8; (ew * eh * 4) as usize];
    for y in 0..ph as usize {
        for x in 0..pw as usize {
            let [b, g, r, a] = px[y * pw as usize + x].to_le_bytes();
            rgba[(y * ew as usize + x) * 4..][..4].copy_from_slice(&[r, g, b, a]);
        }
    }
    let mut ptr = rgba.as_mut_ptr() as *mut std::ffi::c_void;
    let mut img = astcenc_image { dim_x: ew, dim_y: eh, dim_z: 1, data_type: astcenc_type_ASTCENC_TYPE_U8, data: &mut ptr };
    let mut out = vec![0u8; ((ew / block) * (eh / block) * 16) as usize];
    ctx.compress(&mut img, Swizzle::IDENTITY, &mut out).map_err(|e| format!("astcenc: {e:?}"))?;
    ctx.compress_reset().map_err(|e| format!("astcenc: {e:?}"))?;
    Ok(out)
}

/// Bytes of one BC mip level.
fn bc_bytes(w: u32, h: u32, block_bytes: u32) -> usize { (w.div_ceil(4) * h.div_ceil(4) * block_bytes) as usize }

/// `cache` (the export) -> `out`: every map atlas, mask and UI atlas as ASTC under the same file names, then
/// `MARKER`. `progress(0..1)`. Kept when `out` already holds a finished conversion.
pub fn convert(cache: &Path, out: &Path, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
    if out.join(MARKER).is_file() { return Ok(()); }
    let e = |p: &Path, err: std::io::Error| format!("{}: {err}", p.display());
    std::fs::create_dir_all(out.join("ui")).map_err(|err| e(out, err))?;
    let read = |rel: &str| std::fs::read(cache.join(rel)).map_err(|err| e(&cache.join(rel), err));
    let tsv = |rel: &str| std::fs::read_to_string(cache.join(rel)).unwrap_or_default();
    // (file, BC kind, w, h, mips, ASTC block)
    let mut jobs: Vec<(String, Bc, u32, u32, u32, u32)> = vec![];
    let atlases: Vec<Vec<String>> = tsv("atlases.tsv").lines().map(|l| l.split('\t').map(String::from).collect()).collect();
    for a in &atlases { jobs.push((format!("{}.bc", a[1]), Bc::Bc7, a[2].parse().unwrap(), a[3].parse().unwrap(), 1, 4)); }
    for m in tsv("masks.tsv").lines() {
        let c: Vec<u32> = m.split('\t').map(|v| v.parse().unwrap()).collect();
        let name = &atlases.iter().find(|a| a[0] == c[0].to_string()).ok_or("masks.tsv: no such atlas")?[1];
        jobs.push((format!("{name}_m.bc"), if c[3] == 12 { Bc::Bc3 } else { Bc::Bc1 }, c[1], c[2], 1, 8));
    }
    for ui in ["UI-MasterAtlas", "UI-Colour1Atlas", "UI-Colour2Atlas"] {
        // the master atlas is 8192² (main.rs); the colour atlases say "#size\tw\tmips" in their .tsv
        let (w, mips) = tsv(&format!("ui/{ui}.tsv")).lines().find_map(|l| {
            let c: Vec<&str> = l.strip_prefix("#size\t")?.split('\t').collect();
            Some((c[0].parse().ok()?, c[1].parse().ok()?))
        }).unwrap_or((8192, 14));
        if cache.join(format!("ui/{ui}.bc")).is_file() { jobs.push((format!("ui/{ui}.bc"), Bc::Bc7, w, w, mips, 4)); }
    }
    let px: f64 = jobs.iter().map(|j| (j.2 as f64) * (j.3 as f64)).sum();
    let mut done = 0.0;
    let mut ctxs: Vec<(u32, Profile, Context)> = vec![];
    for (file, bc, w, h, mips, block) in jobs {
        let profile = if matches!(bc, Bc::Bc7) { Profile::LdrSrgb } else { Profile::Ldr };
        if !ctxs.iter().any(|c| c.0 == block && c.1 == profile) {
            let cfg = config_init(profile, block, block, 1, Preset::Fast, Flags::empty()).map_err(|err| format!("astcenc: {err:?}"))?;
            ctxs.push((block, profile, Context::new_parallel(&cfg).map_err(|err| format!("astcenc: {err:?}"))?));
        }
        let ctx = &mut ctxs.iter_mut().find(|c| c.0 == block && c.1 == profile).unwrap().2;
        let data = read(&file)?;
        let bb = if matches!(bc, Bc::Bc1) { 8 } else { 16 };
        let (mut off, mut res) = (0, vec![]);
        for m in 0..mips {
            let (mw, mh) = ((w >> m).max(1), (h >> m).max(1));
            let n = bc_bytes(mw, mh, bb);
            res.extend(image(ctx, data.get(off..off + n).ok_or(format!("{file}: shorter than its mips"))?, &bc, mw, mh, block)?);
            off += n;
        }
        let dst = out.join(&file);
        std::fs::write(&dst, res).map_err(|err| e(&dst, err))?;
        done += (w as f64) * (h as f64);
        progress((done / px) as f32);
    }
    std::fs::write(out.join(MARKER), "colour atlases ASTC 4x4 sRGB, masks ASTC 8x8\n").map_err(|err| e(out, err))
}

/// A 4×4 BC7 / BC1 block of one colour survives the round trip (sRGB, within ASTC's error).
#[test]
fn round_trip() {
    // BC7 mode 6, every pixel (200, 100, 50, 255): decode it, encode, decode the ASTC back
    let solid = [200u8, 100, 50, 255];
    let bc1 = { // BC1: both endpoints the colour in 565, indices 0
        let c = ((solid[0] as u16 >> 3) << 11) | ((solid[1] as u16 >> 2) << 5) | (solid[2] as u16 >> 3);
        let mut b = [0u8; 8]; b[..2].copy_from_slice(&c.to_le_bytes()); b[2..4].copy_from_slice(&c.to_le_bytes()); b
    };
    let cfg = config_init(Profile::Ldr, 8, 8, 1, Preset::Fast, Flags::empty()).unwrap();
    let mut ctx = Context::new(&cfg).unwrap();
    let a = image(&mut ctx, &[bc1; 4].concat(), &Bc::Bc1, 8, 8, 8).unwrap();
    assert_eq!(a.len(), 16);
    let mut back = vec![0u32; 64];
    texture2ddecoder::decode_astc_8_8(&a, 8, 8, &mut back).unwrap();
    let [b, g, r, al] = back[27].to_le_bytes();
    let want = { let mut p = [0u32; 16]; texture2ddecoder::decode_bc1(&bc1, 4, 4, &mut p).unwrap(); p[0].to_le_bytes() };
    for (x, y) in [(r, want[2]), (g, want[1]), (b, want[0]), (al, want[3])] { assert!((x as i32 - y as i32).abs() <= 2, "{x} vs {y}"); }
}
