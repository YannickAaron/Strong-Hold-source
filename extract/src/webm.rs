//! Minimal WebM (Matroska) demuxer: the first video and audio track's frames in file order, plus VP8 alpha
//! (BlockAdditional, BlockAddID 1), and the audio CodecPrivate. Enough for the game's ffmpeg-written .webm files.
use crate::Result;

#[derive(Default)]
pub struct Track { pub number: u64, pub kind: u64, pub codec: String, pub private: Vec<u8>, pub w: u32, pub h: u32, pub rate: f64, pub channels: u32 }

#[derive(Default)]
pub struct Webm { pub video: Track, pub audio: Option<Track>, pub frames: Vec<(Vec<u8>, Option<Vec<u8>>)>, pub audio_packets: Vec<Vec<u8>>, raw: Vec<(u64, Vec<u8>, Option<Vec<u8>>)> }

fn vint(b: &[u8], p: &mut usize, keep_marker: bool) -> Result<u64> {
    let first = *b.get(*p).ok_or("webm: eof")?;
    let len = first.leading_zeros() as usize + 1;
    if len > 8 { return Err("webm: bad vint".into()); }
    let mut v = if keep_marker { first as u64 } else { (first as u64) & ((1 << (8 - len)) - 1) };
    let all_ones = !keep_marker && v == (1 << (8 - len)) - 1;
    let mut ones = all_ones;
    for i in 1..len {
        let c = *b.get(*p + i).ok_or("webm: eof")?;
        ones &= c == 0xff;
        v = v << 8 | c as u64;
    }
    *p += len;
    Ok(if ones { u64::MAX } else { v })
}

fn uint(b: &[u8]) -> u64 { b.iter().fold(0, |v, &c| v << 8 | c as u64) }

const SEGMENT: u64 = 0x18538067;
const CLUSTER: u64 = 0x1F43B675;
const TRACKS: u64 = 0x1654AE6B;
const TRACK_ENTRY: u64 = 0xAE;
const BLOCK_GROUP: u64 = 0xA0;
const BLOCK_ADDITIONS: u64 = 0x75A1;
const BLOCK_MORE: u64 = 0xA6;
const VIDEO: u64 = 0xE0;
const AUDIO: u64 = 0xE1;

/// Frames of a block body (track, timecode, flags, lacing).
fn block(body: &[u8]) -> Result<(u64, Vec<Vec<u8>>)> {
    let mut p = 0;
    let track = vint(body, &mut p, false)?;
    let flags = *body.get(p + 2).ok_or("webm: block")?;
    p += 3;
    let lacing = (flags >> 1) & 3;
    if lacing == 0 { return Ok((track, vec![body[p..].to_vec()])); }
    let n = *body.get(p).ok_or("webm: lacing")? as usize + 1;
    p += 1;
    let mut sizes = Vec::with_capacity(n);
    match lacing {
        1 => for _ in 0..n - 1 { // Xiph
            let mut s = 0;
            loop { let c = *body.get(p).ok_or("webm: lacing")?; p += 1; s += c as usize; if c != 255 { break; } }
            sizes.push(s);
        },
        3 => { // EBML
            let mut s = vint(body, &mut p, false)? as i64;
            sizes.push(s as usize);
            for _ in 1..n - 1 {
                let q = p;
                let raw = vint(body, &mut p, false)? as i64;
                let bits = 7 * (p - q) as i64;
                s += raw - ((1 << (bits - 1)) - 1);
                sizes.push(s as usize);
            }
        }
        _ => {} // fixed: filled below
    }
    let rest = body.len().checked_sub(p).ok_or("webm: lacing")?;
    if lacing == 2 { sizes = vec![rest / n; n - 1]; }
    let last = rest.checked_sub(sizes.iter().sum()).ok_or("webm: lacing")?;
    sizes.push(last);
    let mut out = vec![];
    for s in sizes { out.push(body.get(p..p + s).ok_or("webm: lacing")?.to_vec()); p += s; }
    Ok((track, out))
}

pub fn parse(b: &[u8]) -> Result<Webm> {
    let mut w = Webm::default();
    let mut tracks = vec![];
    walk(b, 0, b.len(), &mut w, &mut tracks, &mut Track::default())?;
    w.video = tracks.iter().position(|t| t.kind == 1).map(|i| tracks.swap_remove(i)).ok_or("webm: no video track")?;
    w.audio = tracks.into_iter().find(|t| t.kind == 2);
    // blocks of every track, in file order: split into video frames and audio packets
    let (v, a) = (w.video.number, w.audio.as_ref().map(|t| t.number));
    for (t, f, alpha) in std::mem::take(&mut w.raw) {
        if t == v { w.frames.push((f, alpha)); } else if Some(t) == a { w.audio_packets.push(f); }
    }
    Ok(w)
}

fn walk(b: &[u8], mut p: usize, end: usize, w: &mut Webm, tracks: &mut Vec<Track>, t: &mut Track) -> Result<()> {
    while p < end {
        let id = vint(b, &mut p, true)?;
        let size = vint(b, &mut p, false)?;
        let e = if size == u64::MAX { end } else { (p + size as usize).min(end) };
        let body = &b[p..e];
        match id {
            SEGMENT | CLUSTER | TRACKS | VIDEO | AUDIO => walk(b, p, e, w, tracks, t)?,
            TRACK_ENTRY => { let mut n = Track::default(); walk(b, p, e, w, tracks, &mut n)?; tracks.push(n); }
            0xD7 => t.number = uint(body),
            0x83 => t.kind = uint(body),
            0x86 => t.codec = String::from_utf8_lossy(body).into_owned(),
            0x63A2 => t.private = body.to_vec(),
            0xB0 => t.w = uint(body) as u32,
            0xBA => t.h = uint(body) as u32,
            0x9F => t.channels = uint(body) as u32,
            0xB5 => t.rate = if body.len() == 4 { f32::from_be_bytes(body.try_into()?) as f64 } else { f64::from_be_bytes(body.try_into()?) },
            0xA3 => push(w, body, None)?,
            BLOCK_GROUP => {
                let (mut blk, mut alpha) = (None, None);
                let mut q = p;
                while q < e {
                    let id = vint(b, &mut q, true)?;
                    let s = vint(b, &mut q, false)? as usize;
                    match id {
                        0xA1 => blk = Some(&b[q..q + s]),
                        BLOCK_ADDITIONS => {
                            let mut r = q;
                            while r < q + s {
                                let id = vint(b, &mut r, true)?;
                                let s2 = vint(b, &mut r, false)? as usize;
                                if id == BLOCK_MORE {
                                    let (mut add_id, mut data, mut x) = (1, None, r);
                                    while x < r + s2 {
                                        let id = vint(b, &mut x, true)?;
                                        let s3 = vint(b, &mut x, false)? as usize;
                                        if id == 0xEE { add_id = uint(&b[x..x + s3]); }
                                        if id == 0xA5 { data = Some(b[x..x + s3].to_vec()); }
                                        x += s3;
                                    }
                                    if add_id == 1 { alpha = data; }
                                }
                                r += s2;
                            }
                        }
                        _ => {}
                    }
                    q += s;
                }
                if let Some(blk) = blk { push(w, blk, alpha)?; }
            }
            _ => {}
        }
        p = e;
    }
    Ok(())
}

fn push(w: &mut Webm, body: &[u8], alpha: Option<Vec<u8>>) -> Result<()> {
    let (track, frames) = block(body)?;
    let n = frames.len();
    for (i, f) in frames.into_iter().enumerate() {
        w.raw.push((track, f, if i + 1 == n { alpha.clone() } else { None }));
    }
    Ok(())
}
