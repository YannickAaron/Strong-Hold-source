//! Usage: implodecheck <file.sav|.map>...
//! Re-compresses every packed blob of a save (header block 0/1 and each compressed section) with our implode
//! and byte-compares it with the stream the game wrote (byte-exact saves need the same encoder).
use shc_formats::{implode, map};

fn main() {
    let (mut same, mut diff) = (0, 0);
    for path in std::env::args().skip(1) {
        let b = std::fs::read(&path).expect("read");
        let f = map::parse(&b).expect("parse");
        let le = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
        // packed blobs: header blocks 0 and 1 (block 1 carries 8 bytes before it), then compressed sections
        let mut blobs: Vec<(String, &[u8])> = vec![];
        if let Some(h) = f.header_blocks.first() { blobs.push(("preview".into(), h)); }
        if let Some(h) = f.header_blocks.get(1) { blobs.push(("block1".into(), &h[8..])); }
        let mut o = 4 + f.header_blocks.iter().map(|h| 4 + h.len()).sum::<usize>() + 4;
        let ds = le(o);
        let n = (ds - 32) / 20;
        let data = o + ds;
        o += 4;
        for i in 0..le(o + 4) {
            let at = |a: usize| le(o + 28 + (a * n + i) * 4);
            if at(3) != 0 {
                blobs.push((format!("section {}", at(2) as i32), &b[data + at(4)..]));
            }
        }
        for (name, blob) in blobs {
            let (un, pk) = (le_at(blob, 0), le_at(blob, 4));
            let Ok(raw) = map::unpack(blob, 0) else { println!("{path}: {name}: does not unpack (stale header), skipped"); continue };
            let mine = implode::implode(&raw, 4096);
            if mine == blob[12..12 + pk] { same += 1 } else {
                diff += 1;
                let at = mine.iter().zip(&blob[12..]).position(|(a, b)| a != b);
                println!("{path}: {name}: {un} B -> game {pk} B, ours {} B, first diff at {at:?}", mine.len());
            }
        }
    }
    println!("{same} identical, {diff} different");
    std::process::exit((diff != 0) as i32);
}

fn le_at(b: &[u8], o: usize) -> usize { u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize }
