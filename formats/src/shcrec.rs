//! Reader for recorder logs (`.shcrec`, format: recorder/README.md). Yields every call in order.
use std::io::Read;

pub struct Call {
    pub name: String,
    pub tick: u32,
    pub ints: Vec<i32>,       // scalar args, then the return value if the call has one
    pub blobs: Vec<Vec<u8>>,
}

impl Call {
    /// `DLL_RunTick` draw list as 21-short records.
    pub fn draw_list(&self) -> Vec<[i16; 21]> {
        self.blobs[0].chunks_exact(42).map(|c| std::array::from_fn(|k| i16::from_le_bytes([c[2 * k], c[2 * k + 1]]))).collect()
    }
}

pub fn read(path: &str) -> std::io::Result<Vec<Call>> {
    let mut r = lz4_flex::frame::FrameDecoder::new(std::io::BufReader::new(std::fs::File::open(path)?));
    let u32le = |b: &[u8]| u32::from_le_bytes(b[..4].try_into().unwrap());
    let mut h = [0u8; 12];
    r.read_exact(&mut h)?;
    if &h[..8] != b"SHCREC01" && &h[..8] != b"SHCREC02" {
        return Err(std::io::Error::other("not a recorder log"));
    }
    let mut names = vec![];
    for _ in 0..u32le(&h[8..]) {
        let mut n = [0u8; 1];
        r.read_exact(&mut n)?;
        let mut b = vec![0; n[0] as usize + 2];
        r.read_exact(&mut b)?;
        let ints = b[n[0] as usize] as usize + b[n[0] as usize + 1] as usize;
        b.truncate(n[0] as usize);
        names.push((String::from_utf8_lossy(&b).into_owned(), ints));
    }
    if &h[..8] == b"SHCREC02" {
        // skip_v2_sections: u32 n + n × {i16 type_id, u64 addr, u32 size}; RunTick blob 3 = state hashes
        let mut n = [0u8; 4];
        r.read_exact(&mut n)?;
        let mut skip = vec![0u8; u32le(&n) as usize * 14];
        r.read_exact(&mut skip)?;
    }
    let mut calls = vec![];
    let mut h = [0u8; 9];
    // truncated tail (game killed mid-flush) just ends the log
    while r.read_exact(&mut h).is_ok() {
        let mut p = vec![0; u32le(&h[5..]) as usize];
        if r.read_exact(&mut p).is_err() {
            break;
        }
        let (name, n) = &names[h[0] as usize];
        let ints = (0..*n).map(|i| u32le(&p[i * 4..]) as i32).collect();
        let (mut at, mut blobs) = (n * 4, vec![]);
        while at + 4 <= p.len() {
            let len = u32le(&p[at..]) as usize;
            blobs.push(p[at + 4..at + 4 + len].to_vec());
            at += 4 + len;
        }
        calls.push(Call { name: name.clone(), tick: u32le(&h[1..]), ints, blobs });
    }
    Ok(calls)
}
