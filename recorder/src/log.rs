//! Log encoding shared by the proxy (writer) and `dump` (reader). Format: README.md.

use std::io::Write;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

pub const MAGIC: &[u8; 8] = b"SHCREC02";
pub const MAGIC_V1: &[u8; 8] = b"SHCREC01";

/// One entry of the engine's save-section descriptor table (`re/sections.tsv`), with the
/// address as found in the loaded `CrusaderDE_orig.dll` (relocated, runtime value).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section {
    pub type_id: i16,
    pub addr: u64,
    pub size: u32,
}

/// The engine's own checksum (`DLL_CRC`, FUN_180097910): IEEE CRC-32 table, register starts
/// at 0, no final inversion. crc32fast keeps the *inverted* register as its state, so
/// seed with !0 and invert the result.
pub fn engine_crc(data: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new_with_initial(!0);
    h.update(data);
    !h.finalize()
}

/// Per-export metadata written into the log header so the reader needs no copy of the table.
pub struct CallInfo {
    pub name: &'static str,
    pub scalars: u8, // i32 args (bools included); pointers are not counted
    pub has_ret: bool,
}

/// One call record: `u8 call_id, u32 tick, u32 payload_len, payload`.
pub struct Rec(pub Vec<u8>);

impl Rec {
    pub fn new(id: u8, tick: u32) -> Self {
        let mut v = Vec::with_capacity(64);
        v.push(id);
        v.extend_from_slice(&tick.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        Rec(v)
    }
    pub fn i32(&mut self, x: i32) {
        self.0.extend_from_slice(&x.to_le_bytes());
    }
    pub fn blob(&mut self, b: &[u8]) {
        self.0.extend_from_slice(&(b.len() as u32).to_le_bytes());
        self.0.extend_from_slice(b);
    }
    pub fn finish(mut self) -> Vec<u8> {
        let n = (self.0.len() - 9) as u32;
        self.0[5..9].copy_from_slice(&n.to_le_bytes());
        self.0
    }
}

pub fn write_header(w: &mut impl Write, calls: &[CallInfo], secs: &[Section]) -> std::io::Result<()> {
    w.write_all(MAGIC)?;
    w.write_all(&(calls.len() as u32).to_le_bytes())?;
    for c in calls {
        w.write_all(&[c.name.len() as u8])?;
        w.write_all(c.name.as_bytes())?;
        w.write_all(&[c.scalars, c.has_ret as u8])?;
    }
    w.write_all(&(secs.len() as u32).to_le_bytes())?;
    for s in secs {
        w.write_all(&s.type_id.to_le_bytes())?;
        w.write_all(&s.addr.to_le_bytes())?;
        w.write_all(&s.size.to_le_bytes())?;
    }
    Ok(())
}

/// Writer thread body: lz4-frame the whole stream, flush every ~1 s so a
/// crash/kill (the game never unloads plugins cleanly) loses at most ~1 s.
/// No BufWriter: `FrameEncoder::flush` only emits the pending block to its inner
/// writer, so a buffer there would hold up to 1 MiB (minutes of play, and the
/// save/load calls before a quit) that a kill then loses.
pub fn writer_loop(file: std::fs::File, calls: &[CallInfo], secs: Vec<Section>, rx: Receiver<Vec<u8>>) -> std::io::Result<()> {
    let mut w = lz4_flex::frame::FrameEncoder::new(file);
    write_header(&mut w, calls, &secs)?;
    let mut last = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(v) => w.write_all(&v)?,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if last.elapsed() >= Duration::from_secs(1) {
            w.flush()?;
            last = Instant::now();
        }
    }
    w.finish().map_err(std::io::Error::other)?.flush()
}

#[cfg(test)]
mod tests {
    /// Reference: the byte-at-a-time table CRC from `formats/src/map.rs::engine_crc`.
    fn reference(data: &[u8]) -> u32 {
        let mut crc = 0u32;
        for &b in data {
            let mut c = (crc ^ b as u32) & 0xff;
            for _ in 0..8 {
                c = (c >> 1) ^ (0xEDB8_8320 & (c & 1).wrapping_neg());
            }
            crc = c ^ (crc >> 8);
        }
        crc
    }

    #[test]
    fn engine_crc_matches_table_crc() {
        let data: Vec<u8> = (0..100_003u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
        for n in [0, 1, 7, 15, 16, 17, 63, 64, 65, 1000, 4096, 100_003] {
            assert_eq!(super::engine_crc(&data[..n]), reference(&data[..n]), "len {n}");
        }
    }
}
