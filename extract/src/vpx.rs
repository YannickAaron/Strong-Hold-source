//! libvpx VP8 decoder (statically linked, see build.rs): only the five calls we need, declared by hand
//! (vpx/vpx_decoder.h, vpx/vpx_image.h of libvpx 1.x; VPX_DECODER_ABI_VERSION = 3 + 4 + 5).
use crate::Result;
use std::{ffi::c_void, ptr};

#[repr(C)]
struct Image {
    fmt: i32, cs: i32, range: i32,
    w: u32, h: u32, bit_depth: u32, d_w: u32, d_h: u32, r_w: u32, r_h: u32, x_chroma_shift: u32, y_chroma_shift: u32,
    planes: [*const u8; 4],
    stride: [i32; 4],
}

unsafe extern "C" {
    fn vpx_codec_vp8_dx() -> *const c_void;
    fn vpx_codec_dec_init_ver(ctx: *mut c_void, iface: *const c_void, cfg: *const c_void, flags: std::ffi::c_long, ver: i32) -> i32;
    fn vpx_codec_decode(ctx: *mut c_void, data: *const u8, size: u32, user: *mut c_void, deadline: std::ffi::c_long) -> i32;
    fn vpx_codec_get_frame(ctx: *mut c_void, iter: *mut *const c_void) -> *const Image;
    fn vpx_codec_destroy(ctx: *mut c_void) -> i32;
}

const ABI: i32 = 12;
const I420: i32 = 0x100 | 2;

pub struct Decoder { ctx: Box<[u64; 16]> } // vpx_codec_ctx_t is 56 bytes

/// A decoded I420 frame: Y, U, V planes (tight), luma size.
pub struct Frame { pub w: usize, pub h: usize, pub y: Vec<u8>, pub u: Vec<u8>, pub v: Vec<u8> }

impl Decoder {
    pub fn vp8() -> Result<Decoder> {
        let mut ctx = Box::new([0u64; 16]);
        let e = unsafe { vpx_codec_dec_init_ver(ctx.as_mut_ptr() as *mut c_void, vpx_codec_vp8_dx(), ptr::null(), 0, ABI) };
        if e != 0 { return Err(format!("vpx init: error {e}").into()); }
        Ok(Decoder { ctx })
    }

    pub fn decode(&mut self, data: &[u8]) -> Result<Frame> {
        let c = self.ctx.as_mut_ptr() as *mut c_void;
        let e = unsafe { vpx_codec_decode(c, data.as_ptr(), data.len() as u32, ptr::null_mut(), 0) };
        if e != 0 { return Err(format!("vpx decode: error {e}").into()); }
        let mut it = ptr::null();
        let img = unsafe { vpx_codec_get_frame(c, &mut it) };
        let img = unsafe { img.as_ref() }.ok_or("vpx: no frame")?;
        if img.fmt != I420 { return Err(format!("vpx: image format {:#x}", img.fmt).into()); }
        let (w, h) = (img.d_w as usize, img.d_h as usize);
        let plane = |i: usize, pw: usize, ph: usize| -> Vec<u8> {
            let mut out = Vec::with_capacity(pw * ph);
            for row in 0..ph {
                let s = unsafe { std::slice::from_raw_parts(img.planes[i].offset(row as isize * img.stride[i] as isize), pw) };
                out.extend_from_slice(s);
            }
            out
        };
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
        Ok(Frame { w, h, y: plane(0, w, h), u: plane(1, cw, ch), v: plane(2, cw, ch) })
    }
}

impl Drop for Decoder {
    fn drop(&mut self) { unsafe { vpx_codec_destroy(self.ctx.as_mut_ptr() as *mut c_void); } }
}

impl Frame {
    /// RGBA8, BT.601 limited range (VP8's), chroma nearest; alpha = 255 or the luma of `alpha`'s frame.
    pub fn rgba(&self, alpha: Option<&Frame>) -> Vec<u8> {
        let (w, h, cw) = (self.w, self.h, self.w.div_ceil(2));
        let mut out = vec![0u8; w * h * 4];
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let rows = h.div_ceil(threads).max(1);
        std::thread::scope(|s| for (t, chunk) in out.chunks_mut(rows * w * 4).enumerate() {
            s.spawn(move || for (i, px) in chunk.chunks_mut(4).enumerate() {
                let (y, x) = (t * rows + i / w, i % w);
                let yy = (self.y[y * w + x] as i32 - 16) * 298;
                let (u, v) = (self.u[(y / 2) * cw + x / 2] as i32 - 128, self.v[(y / 2) * cw + x / 2] as i32 - 128);
                px[0] = ((yy + 409 * v + 128) >> 8).clamp(0, 255) as u8;
                px[1] = ((yy - 100 * u - 208 * v + 128) >> 8).clamp(0, 255) as u8;
                px[2] = ((yy + 516 * u + 128) >> 8).clamp(0, 255) as u8;
                px[3] = alpha.map_or(255, |a| a.y.get(y * a.w + x).copied().unwrap_or(255));
            });
        });
        out
    }
}
