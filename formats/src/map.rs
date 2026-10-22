//! `.map` / `.sav` container, as read by CrusaderDE.dll FUN_1800087d0.
//!
//! Layout:
//!   i32 version             (< 0: header chain follows)
//!   header chain            up to 7 blocks of [u32 len][len bytes]; a zero len ends it.
//!                           block 0 = packed preview, 7th block is followed by an extra u32.
//!   i32 dir_size            0x7f4 (100 entries) or 0xbdc (150 entries)
//!   directory               7 x i32 header (data_size, section_count, ...), then 5 arrays of N i32:
//!                           uncompressed_size, compressed_size, type_id, is_compressed, offset
//!   data                    data_size bytes; sections live at `offset` inside it
//! Compressed section = [u32 unpacked][u32 packed][u32 crc][PKWARE DCL stream].

use crate::blast;

#[derive(Debug)]
pub struct Section {
    pub type_id: i32,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct MapFile {
    pub version: i32,
    pub header_blocks: Vec<Vec<u8>>,
    pub sections: Vec<Section>,
    /// Sections whose unpack failed. The engine ignores the unpack result (FUN_1800087d0),
    /// so the game just leaves them unloaded; we do the same. Seen: type 1190 in 19 shipped maps.
    pub broken: Vec<(i32, Error)>,
}

#[derive(Debug)]
pub enum Error {
    Truncated(&'static str),
    BadDirectory(i32),
    Unpack { type_id: i32, err: blast::Error },
    Crc { type_id: i32 },
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], Error> {
        let s = self.b.get(self.pos..self.pos + n).ok_or(Error::Truncated(what))?;
        self.pos += n;
        Ok(s)
    }
    fn i32(&mut self, what: &'static str) -> Result<i32, Error> {
        Ok(i32::from_le_bytes(self.take(4, what)?.try_into().unwrap()))
    }
}

fn le(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}

impl MapFile {
    /// Map preview as 200x200 RGBA, top row first (`DLL_UnpackRadarToARGB` -> FUN_1800f6120).
    /// Header block 0 unpacks to a 256-entry RGB555 palette (u16) followed by 200x200 palette indices.
    pub fn preview_rgba(&self) -> Option<Vec<u8>> {
        let raw = unpack(self.header_blocks.first()?, -1).ok()?;
        if raw.len() != 512 + 200 * 200 {
            return None;
        }
        let rgb = |i: u8| {
            let c = u16::from_le_bytes([raw[i as usize * 2], raw[i as usize * 2 + 1]]);
            let x = |shift: u16| (((c >> shift) & 31) << 3) as u8;
            [x(10), x(5), x(0), 255]
        };
        Some(raw[512..].iter().flat_map(|&i| rgb(i)).collect())
    }

    /// File name stored in header block 4 (`[u32; 3][u32 len][len bytes]`, FUN_1800087d0).
    pub fn file_name(&self) -> Option<String> {
        let b = self.header_blocks.get(4)?;
        let len = le(b.get(..16)?, 12) as usize;
        let name = b.get(16..16 + len)?;
        Some(String::from_utf8_lossy(name).trim_end_matches('\0').to_string())
    }
}

/// Unpack a `[unpacked][packed][crc][stream]` blob and verify its CRC.
pub fn unpack(blob: &[u8], type_id: i32) -> Result<Vec<u8>, Error> {
    if blob.len() < 12 {
        return Err(Error::Truncated("packed header"));
    }
    let (unpacked, packed, crc) = (le(blob, 0) as usize, le(blob, 4) as usize, le(blob, 8));
    let stream = blob.get(12..12 + packed).ok_or(Error::Truncated("packed stream"))?;
    let out = blast::explode(stream, unpacked).map_err(|err| Error::Unpack { type_id, err })?;
    if crc32(&out) != crc {
        return Err(Error::Crc { type_id });
    }
    Ok(out)
}

pub fn parse(bytes: &[u8]) -> Result<MapFile, Error> {
    let mut r = Reader { b: bytes, pos: 0 };
    let version = r.i32("version")?;
    let mut header_blocks = Vec::new();
    let mut dir_size = version;
    if version < 0 {
        for i in 0..7 {
            let len = r.i32("header len")? as usize;
            if len == 0 {
                break;
            }
            header_blocks.push(r.take(len, "header block")?.to_vec());
            if i == 6 {
                r.i32("trailer")?;
            }
        }
        dir_size = r.i32("dir size")?;
    }
    if !(0x24..0x10000).contains(&dir_size) {
        return Err(Error::BadDirectory(dir_size));
    }
    let dir = r.take(dir_size as usize - 4, "directory")?;
    let n = (dir_size as usize - 4 - 28) / 20; // entries per array
    let field = |arr: usize, i: usize| le(dir, 28 + (arr * n + i) * 4) as i32;
    let (data_size, count) = (le(dir, 0) as usize, le(dir, 4) as usize);
    if count > n {
        return Err(Error::BadDirectory(dir_size));
    }
    let data = r.take(data_size, "data")?;

    let mut sections = Vec::with_capacity(count);
    let mut broken = Vec::new();
    for i in 0..count {
        let (size, type_id, compressed, offset) = (field(0, i) as usize, field(2, i), field(3, i) != 0, field(4, i) as usize);
        let raw = data.get(offset..).ok_or(Error::Truncated("section offset"))?;
        if !compressed {
            sections.push(Section { type_id, data: raw.get(..size).ok_or(Error::Truncated("section"))?.to_vec() });
            continue;
        }
        match unpack(raw, type_id) {
            Ok(data) => sections.push(Section { type_id, data }),
            Err(e) => broken.push((type_id, e)),
        }
    }
    Ok(MapFile { version, header_blocks, sections, broken })
}

/// Standard CRC-32 (IEEE, reflected), as used by PKWARE DCL.
pub fn crc32(data: &[u8]) -> u32 {
    !crc32_update(!0, data)
}

/// The engine's sync checksum, `DLL_CRC` (FUN_180097910): IEEE CRC-32 table, start 0, no final inversion.
/// `DLL_CRCS(ptr, n)` is the same over `2 * n` bytes.
pub fn engine_crc(data: &[u8]) -> u32 {
    // crc32fast (SIMD) with the standard inversions undone: register init 0, no final xor (as recorder/log.rs)
    let mut h = crc32fast::Hasher::new_with_initial(!0);
    h.update(data);
    !h.finalize()
}

const CRC_TABLE: [u32; 256] = {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = (c >> 1) ^ (0xEDB8_8320 & (c & 1).wrapping_neg());
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    for &b in data {
        crc = (crc >> 8) ^ CRC_TABLE[((crc ^ b as u32) & 0xff) as usize];
    }
    crc
}

#[cfg(test)]
mod tests {
    #[test]
    fn crc32_check_value() {
        assert_eq!(super::crc32(b"123456789"), 0xCBF4_3926);
    }
}

#[test]
fn engine_crc_fast_matches_table() {
    let data: Vec<u8> = (0..100_003u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
    assert_eq!(engine_crc(&data), crc32_update(0, &data));
    assert_eq!(engine_crc(&[]), 0);
}
