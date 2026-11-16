//! The art's GPU formats. The export is BC (BC7 colour, BC1 / BC3 team-colour masks); GPUs before the A17 Pro / M1
//! can't read BC, so the iPhone Builder's "all devices" IPA carries it converted to ASTC (shc_extract::astc: colour
//! 4×4 sRGB, masks 8×8, `MARKER` beside them). BC that isn't converted (the app's own Sassanid atlas, custom lords,
//! videos, sketches) goes up as it is, or decoded to RGBA8 (4× the bytes) where the GPU can't read BC.
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use wgpu::{AstcBlock, AstcChannel, TextureFormat as F};

/// shc_extract::astc::MARKER (that module is Mac-only).
pub const MARKER: &str = "astc.txt";
static BC: AtomicBool = AtomicBool::new(true);

/// The export folder holds ASTC (an "all devices" IPA).
pub fn astc_files() -> bool {
    static A: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *A.get_or_init(|| crate::cache_dir().join(MARKER).is_file())
}

/// The device features to ask for: BC and ASTC where the adapter has them. Err: it can't read this export's art.
pub fn features(adapter: &wgpu::Adapter) -> Result<wgpu::Features, &'static str> {
    let mut has = adapter.features() & (wgpu::Features::TEXTURE_COMPRESSION_BC | wgpu::Features::TEXTURE_COMPRESSION_ASTC);
    if std::env::var_os("SHC_NO_BC").is_some() { has.remove(wgpu::Features::TEXTURE_COMPRESSION_BC); } // test an older iPhone's path on a Mac
    BC.store(has.contains(wgpu::Features::TEXTURE_COMPRESSION_BC), Relaxed);
    match (astc_files(), bc()) {
        (true, _) if !has.contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC) => Err("this GPU can't read ASTC textures: build the IPA for \"New devices\""),
        (false, false) => Err("this GPU can't read BC textures (the game's art): build the IPA for \"All devices\" in the iPhone Builder"),
        _ => Ok(has),
    }
}

/// The GPU reads BC.
pub fn bc() -> bool { BC.load(Relaxed) }

/// The export's colour atlases (map and UI).
pub fn colour() -> F { if astc_files() { F::Astc { block: AstcBlock::B4x4, channel: AstcChannel::UnormSrgb } } else { F::Bc7RgbaUnormSrgb } }
/// The export's team-colour masks (masks.tsv format 12 = BC3, else BC1).
pub fn mask(bc3: bool) -> F {
    if astc_files() { F::Astc { block: AstcBlock::B8x8, channel: AstcChannel::Unorm } } else if bc3 { F::Bc3RgbaUnorm } else { F::Bc1RgbaUnorm }
}

/// BC blocks that weren't converted: as they are, or RGBA8 where the GPU can't read BC.
pub fn fit<'a>(f: F, data: &'a [u8], w: u32, h: u32) -> (F, Cow<'a, [u8]>) {
    if bc() || !matches!(f, F::Bc1RgbaUnorm | F::Bc1RgbaUnormSrgb | F::Bc3RgbaUnorm | F::Bc3RgbaUnormSrgb | F::Bc7RgbaUnorm | F::Bc7RgbaUnormSrgb) {
        return (f, Cow::Borrowed(data));
    }
    let (pw, ph) = (w.next_multiple_of(4) as usize, h.next_multiple_of(4) as usize);
    let mut px = vec![0u32; pw * ph];
    let r = match f {
        F::Bc1RgbaUnorm | F::Bc1RgbaUnormSrgb => texture2ddecoder::decode_bc1(data, pw, ph, &mut px),
        F::Bc3RgbaUnorm | F::Bc3RgbaUnormSrgb => texture2ddecoder::decode_bc3(data, pw, ph, &mut px),
        _ => texture2ddecoder::decode_bc7(data, pw, ph, &mut px),
    };
    if let Err(e) = r { eprintln!("BC decode: {e}"); }
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h as usize { for p in &px[y * pw..y * pw + w as usize] { let [b, g, r, a] = p.to_le_bytes(); out.extend([r, g, b, a]); } }
    (if f.is_srgb() { F::Rgba8UnormSrgb } else { F::Rgba8Unorm }, Cow::Owned(out))
}

/// One mip level of `f` at `w`×`h`: (bytes per block row, block rows, bytes).
pub fn layout(f: F, w: u32, h: u32) -> (u32, u32, usize) {
    let (bw, bh) = f.block_dimensions();
    let row = w.div_ceil(bw) * f.block_copy_size(None).unwrap();
    let rows = h.div_ceil(bh);
    (row, rows, row as usize * rows as usize)
}

#[test]
fn layouts() {
    assert_eq!(layout(F::Bc7RgbaUnormSrgb, 8192, 8192), (32768, 2048, 64 << 20));
    assert_eq!(layout(F::Astc { block: AstcBlock::B8x8, channel: AstcChannel::Unorm }, 8192, 8192), (16384, 1024, 16 << 20));
    assert_eq!(layout(F::Bc1RgbaUnorm, 2, 2), (8, 1, 8)); // a mip under one block: one block
    assert_eq!(layout(F::Rgba8Unorm, 3, 2), (12, 2, 24));
}
