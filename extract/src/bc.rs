//! BC1 / BC3 block encoder for our .vid frames (format: re/scripts/export_video.py). Inset bounding box with a
//! diagonal pick (van Waveren, "Real-Time DXT Compression"), nearest palette entry per pixel; 4-colour mode only.
//! ponytail: no cluster fit; good enough for video, swap in a PCA fit if banding shows.

fn pack565(c: [i32; 3]) -> u16 { ((c[0] as u16 >> 3) << 11) | ((c[1] as u16 >> 2) << 5) | (c[2] as u16 >> 3) }
fn unpack565(c: u16) -> [i32; 3] {
    let (r, g, b) = ((c >> 11) as i32 & 31, (c >> 5) as i32 & 63, c as i32 & 31);
    [r << 3 | r >> 2, g << 2 | g >> 4, b << 3 | b >> 2]
}

fn color_block(px: &[[u8; 4]; 16], out: &mut [u8]) {
    let (mut lo, mut hi) = ([255i32; 3], [0i32; 3]);
    for p in px { for c in 0..3 { lo[c] = lo[c].min(p[c] as i32); hi[c] = hi[c].max(p[c] as i32); } }
    let mid: Vec<i32> = (0..3).map(|c| lo[c] + hi[c]).collect(); // x2
    let (mut rg, mut bg) = (0i64, 0i64);
    for p in px {
        let d = |c: usize| (2 * p[c] as i32 - mid[c]) as i64;
        rg += d(0) * d(1);
        bg += d(2) * d(1);
    }
    if rg < 0 { std::mem::swap(&mut lo[0], &mut hi[0]); }
    if bg < 0 { std::mem::swap(&mut lo[2], &mut hi[2]); }
    for c in 0..3 { let inset = (hi[c] - lo[c]) / 16; hi[c] -= inset; lo[c] += inset; }
    let (mut c0, mut c1) = (pack565(hi.map(|v| v.clamp(0, 255))), pack565(lo.map(|v| v.clamp(0, 255))));
    let mut idx = 0u32;
    if c0 != c1 {
        if c0 < c1 { std::mem::swap(&mut c0, &mut c1); }
        let (a, b) = (unpack565(c0), unpack565(c1));
        let pal = [a, b, [0, 1, 2].map(|c| (2 * a[c] + b[c]) / 3), [0, 1, 2].map(|c| (a[c] + 2 * b[c]) / 3)];
        for (i, p) in px.iter().enumerate() {
            let d = |q: &[i32; 3]| (0..3).map(|c| (p[c] as i32 - q[c]).pow(2)).sum::<i32>();
            let best = (0..4).min_by_key(|&k| d(&pal[k])).unwrap() as u32;
            idx |= best << (2 * i);
        }
    }
    out[..2].copy_from_slice(&c0.to_le_bytes());
    out[2..4].copy_from_slice(&c1.to_le_bytes());
    out[4..8].copy_from_slice(&idx.to_le_bytes());
}

fn alpha_block(px: &[[u8; 4]; 16], out: &mut [u8]) {
    let a0 = px.iter().map(|p| p[3]).max().unwrap() as i32;
    let a1 = px.iter().map(|p| p[3]).min().unwrap() as i32;
    let pal: Vec<i32> = (0..8).map(|k| match k { 0 => a0, 1 => a1, k => ((8 - k) * a0 + (k - 1) * a1) / 7 }).collect();
    let mut bits = 0u64;
    if a0 != a1 {
        for (i, p) in px.iter().enumerate() {
            let best = (0..8).min_by_key(|&k| (p[3] as i32 - pal[k]).abs()).unwrap() as u64;
            bits |= best << (3 * i);
        }
    }
    out[0] = a0 as u8;
    out[1] = a1 as u8;
    out[2..8].copy_from_slice(&bits.to_le_bytes()[..6]);
}

/// RGBA8 rows top-down -> BC1 (8 B/block) or BC3 (16 B/block), block rows top-down. w, h multiples of 4.
pub fn encode(rgba: &[u8], w: usize, h: usize, bc3: bool) -> Vec<u8> {
    let bpb = if bc3 { 16 } else { 8 };
    let (bw, bh) = (w / 4, h / 4);
    let mut out = vec![0u8; bw * bh * bpb];
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let rows_per = bh.div_ceil(threads).max(1);
    std::thread::scope(|s| {
        for (t, chunk) in out.chunks_mut(rows_per * bw * bpb).enumerate() {
            s.spawn(move || {
                for (k, blk) in chunk.chunks_mut(bpb).enumerate() {
                    let (by, bx) = (t * rows_per + k / bw, k % bw);
                    let mut px = [[0u8; 4]; 16];
                    for y in 0..4 { for x in 0..4 {
                        let o = ((by * 4 + y) * w + bx * 4 + x) * 4;
                        px[y * 4 + x].copy_from_slice(&rgba[o..o + 4]);
                    } }
                    if bc3 { alpha_block(&px, &mut blk[..8]); color_block(&px, &mut blk[8..]); } else { color_block(&px, blk); }
                }
            });
        }
    });
    out
}

#[test]
fn flat_and_gradient_blocks_round_trip() {
    // flat colour -> c0 == c1, every index 0; gradient -> distinct endpoints with c0 > c1
    let flat = [200u8, 100, 50, 255].repeat(16);
    let b = encode(&flat, 4, 4, false);
    assert_eq!(&b[4..8], &[0; 4]);
    let grad: Vec<u8> = (0..16).flat_map(|i| [i as u8 * 16, i as u8 * 16, i as u8 * 16, i as u8 * 16]).collect();
    let b = encode(&grad, 4, 4, true);
    assert!(b[0] > b[1]);
    assert!(u16::from_le_bytes([b[8], b[9]]) > u16::from_le_bytes([b[10], b[11]]));
}
