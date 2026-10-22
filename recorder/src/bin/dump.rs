//! `cargo run --bin dump -- <log.shcrec> [-v]` — per-tick summary of a recorder log.
//! -v prints every non-RunTick call with its scalar args.

#[path = "../log.rs"]
#[allow(dead_code)]
mod log;

use std::collections::BTreeMap;
use std::io::Read;

struct Call {
    id: u8,
    tick: u32,
    scalars: Vec<i32>,
    ret: Option<i32>,
    blobs: Vec<Vec<u8>>,
}

struct Reader<R> {
    r: R,
    calls: Vec<(String, u8, bool)>,
    secs: Vec<log::Section>,
}

/// Blob 3 of a RunTick (SHCREC02): engine tick, hash cost, (type_id, crc) pairs.
fn state_hashes(b: &[u8]) -> Option<(u32, u32, Vec<(i16, u32)>)> {
    if b.len() < 8 {
        return None;
    }
    let pairs = b[8..].chunks_exact(6).map(|c| (i16::from_le_bytes([c[0], c[1]]), u32le(&c[2..]))).collect();
    Some((u32le(b), u32le(&b[4..]), pairs))
}

fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes(b[..4].try_into().unwrap())
}

impl<R: Read> Reader<R> {
    fn new(mut r: R) -> std::io::Result<Self> {
        let mut h = [0u8; 12];
        r.read_exact(&mut h)?;
        let v2 = &h[..8] == log::MAGIC;
        assert!(v2 || &h[..8] == log::MAGIC_V1, "not a recorder log");
        let mut calls = Vec::new();
        for _ in 0..u32le(&h[8..]) {
            let mut n = [0u8; 1];
            r.read_exact(&mut n)?;
            let mut name = vec![0; n[0] as usize];
            r.read_exact(&mut name)?;
            let mut m = [0u8; 2];
            r.read_exact(&mut m)?;
            calls.push((String::from_utf8_lossy(&name).into_owned(), m[0], m[1] != 0));
        }
        let mut secs = Vec::new();
        if v2 {
            let mut n = [0u8; 4];
            r.read_exact(&mut n)?;
            for _ in 0..u32le(&n) {
                let mut e = [0u8; 14];
                r.read_exact(&mut e)?;
                secs.push(log::Section {
                    type_id: i16::from_le_bytes([e[0], e[1]]),
                    addr: u64::from_le_bytes(e[2..10].try_into().unwrap()),
                    size: u32le(&e[10..]),
                });
            }
        }
        Ok(Reader { r, calls, secs })
    }

    /// None at end of stream (or a truncated tail after a crash).
    fn next(&mut self) -> Option<Call> {
        let mut h = [0u8; 9];
        self.r.read_exact(&mut h).ok()?;
        let mut p = vec![0; u32le(&h[5..]) as usize];
        self.r.read_exact(&mut p).ok()?;
        let (_, ns, has_ret) = self.calls[h[0] as usize];
        let ints: Vec<i32> = p.chunks(4).take(ns as usize + has_ret as usize).map(|c| u32le(c) as i32).collect();
        let mut at = ints.len() * 4;
        let mut blobs = Vec::new();
        while at < p.len() {
            let n = u32le(&p[at..]) as usize;
            blobs.push(p[at + 4..at + 4 + n].to_vec());
            at += 4 + n;
        }
        Some(Call {
            id: h[0],
            tick: u32le(&h[1..]),
            scalars: ints[..ns as usize].to_vec(),
            ret: has_ret.then(|| ints[ns as usize]),
            blobs,
        })
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: dump <log.shcrec> [-v] [-s <tick>]");
    let verbose = args.iter().any(|a| a == "-v");
    let size = std::fs::metadata(path).unwrap().len();
    let f = std::io::BufReader::new(std::fs::File::open(path).unwrap());
    let mut rd = Reader::new(lz4_flex::frame::FrameDecoder::new(f)).unwrap();

    let mut per_call: BTreeMap<u8, u32> = BTreeMap::new();
    // (call, blob index) -> (min, max) bytes, so a missing or short blob is visible in the summary
    let mut blob_sizes: BTreeMap<(u8, usize), (usize, usize)> = BTreeMap::new();
    let mut types: BTreeMap<i16, u64> = BTreeMap::new();
    let (mut ticks, mut recs_total, mut recs_max, mut raw) = (0u32, 0u64, 0usize, 0u64);
    let mut crcs = Vec::new();
    // type_id -> (last crc, number of changes, samples); hash cost per tick in µs
    let mut hist_state: BTreeMap<i16, (u32, u32, u32)> = BTreeMap::new();
    let mut cost: Vec<u32> = Vec::new();
    let show = args.iter().position(|a| a == "-s").map(|i| args[i + 1].parse::<u32>().unwrap());
    while let Some(c) = rd.next() {
        *per_call.entry(c.id).or_default() += 1;
        for (i, b) in c.blobs.iter().enumerate() {
            let e = blob_sizes.entry((c.id, i)).or_insert((usize::MAX, 0));
            *e = (e.0.min(b.len()), e.1.max(b.len()));
        }
        raw += 9 + c.scalars.len() as u64 * 4 + c.blobs.iter().map(|b| 4 + b.len() as u64).sum::<u64>();
        let name = rd.calls[c.id as usize].0.as_str();
        match name {
            "DLL_RunTick" => {
                ticks += 1;
                let draw = &c.blobs[0];
                let n = draw.len() / 42;
                recs_total += n as u64;
                recs_max = recs_max.max(n);
                let mut hist: BTreeMap<i16, u32> = BTreeMap::new();
                for rec in draw.chunks_exact(42) {
                    *hist.entry(i16::from_le_bytes([rec[0], rec[1]])).or_default() += 1;
                }
                for (t, k) in &hist {
                    *types.entry(*t).or_default() += *k as u64;
                }
                let st = c.blobs.get(3).and_then(|b| state_hashes(b));
                if let Some((eng, us, pairs)) = &st {
                    cost.push(*us);
                    for &(id, crc) in pairs {
                        let e = hist_state.entry(id).or_insert((crc, 0, 0));
                        e.1 += (e.0 != crc) as u32;
                        e.0 = crc;
                        e.2 += 1;
                    }
                    if show == Some(c.tick) {
                        println!("tick {} engine_tick {eng} hash {us} us", c.tick);
                        for (id, crc) in pairs {
                            println!("  {id:5} {crc:#010x}");
                        }
                    }
                }
                // every 100th tick (and the first) to keep output readable
                if c.tick % 100 == 0 || verbose {
                    let h: Vec<String> = hist.iter().map(|(t, k)| format!("{t}:{k}")).collect();
                    let find = |id: i16| st.as_ref().and_then(|(_, _, p)| p.iter().find(|x| x.0 == id)).map_or("-".into(), |x| format!("{:08x}", x.1));
                    let (eng, us) = st.as_ref().map_or((0, 0), |s| (s.0, s.1));
                    println!(
                        "tick {:6} eng {:6} hash {:5}us rng {} units {} bld {} ret {:6} recs {:6} paused {} sel {:3} types [{}]",
                        c.tick, eng, us, find(1040), find(1015), find(1013), c.ret.unwrap(), n, c.scalars[6], c.blobs[2].len() / 8, h.join(" ")
                    );
                }
            }
            "DLL_CRC" | "DLL_CRCS" => {
                crcs.push((c.tick, name.to_string(), c.scalars[0], c.ret.unwrap()));
                if verbose {
                    println!("tick {:6} {name}(size {}) = {:#010x}", c.tick, c.scalars[0], c.ret.unwrap() as u32);
                }
            }
            _ => {
                let interesting = matches!(
                    name,
                    "DLL_SetMPRandSeed" | "DLL_LoadMapToPlay" | "DLL_LoadSaveGame" | "DLL_PreInitMap_Multiplayer"
                        | "DLL_RegisterSkirmishUser" | "DLL_StartMultiplayerGame" | "DLL_StartMultiplayerGameSynced"
                        | "DLL_SetPath"
                );
                if verbose || interesting {
                    let blobs: Vec<String> = c
                        .blobs
                        .iter()
                        .map(|b| match std::str::from_utf8(b) {
                            Ok(s) if b.len() < 200 && !s.contains('\0') => format!("{s:?}"),
                            _ => format!("<{} B>", b.len()),
                        })
                        .collect();
                    println!("tick {:6} {name}{:?} -> {:?} {}", c.tick, c.scalars, c.ret, blobs.join(" "));
                }
            }
        }
    }

    println!("\n== summary ==");
    println!("file {size} B compressed, {raw} B raw, {ticks} ticks");
    if ticks > 0 {
        println!("draw records: {recs_total} total, {:.0}/tick avg, {recs_max} max", recs_total as f64 / ticks as f64);
    }
    println!("record types: {}", types.iter().map(|(t, k)| format!("{t}:{k}")).collect::<Vec<_>>().join(" "));
    if !cost.is_empty() {
        let mut sorted = cost.clone();
        sorted.sort_unstable();
        println!(
            "state hashes: {} sections in table, {} hashed; cost/tick median {} us, p90 {} us, max {} us",
            rd.secs.len(), hist_state.len(), sorted[sorted.len() / 2], sorted[sorted.len() * 9 / 10], sorted[sorted.len() - 1]
        );
        let changing: Vec<String> = hist_state.iter().filter(|(_, v)| v.1 > 0).map(|(k, v)| format!("{k}:{}/{}", v.1, v.2)).collect();
        let constant: Vec<String> = hist_state.iter().filter(|(_, v)| v.1 == 0).map(|(k, _)| k.to_string()).collect();
        println!("  changing (id:changes/samples): {}", changing.join(" "));
        println!("  constant: {}", constant.join(" "));
    }
    println!("CRC calls: {}", crcs.len());
    for (t, n, sz, v) in crcs.iter().take(10) {
        println!("  tick {t} {n}(size {sz}) = {:#010x}", *v as u32);
    }
    println!("calls (blob sizes min..max B):");
    for (id, k) in per_call {
        let blobs: Vec<String> = blob_sizes
            .range((id, 0)..(id, usize::MAX))
            .map(|(_, (lo, hi))| if lo == hi { format!("{lo}") } else { format!("{lo}..{hi}") })
            .collect();
        println!("  {:6} {:32} {}", k, rd.calls[id as usize].0, blobs.join(" "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let calls = [
            log::CallInfo { name: "DLL_SetMPRandSeed", scalars: 1, has_ret: false },
            log::CallInfo { name: "DLL_RunTick", scalars: 2, has_ret: true },
        ];
        let mut raw = Vec::new();
        log::write_header(&mut raw, &calls, &[log::Section { type_id: 1046, addr: 0x183663ee8, size: 4 }]).unwrap();
        let mut r = log::Rec::new(0, 0);
        r.i32(1234);
        raw.extend(r.finish());
        let mut r = log::Rec::new(1, 7);
        r.i32(-1);
        r.i32(5);
        r.i32(2); // ret
        r.blob(&[0u8; 84]);
        r.blob(b"");
        raw.extend(r.finish());

        let mut enc = lz4_flex::frame::FrameEncoder::new(Vec::new());
        std::io::Write::write_all(&mut enc, &raw).unwrap();
        let z = enc.finish().unwrap();
        let mut rd = Reader::new(lz4_flex::frame::FrameDecoder::new(&z[..])).unwrap();
        assert_eq!(rd.secs[0].type_id, 1046);
        let a = rd.next().unwrap();
        assert_eq!((a.id, a.scalars.clone(), a.ret, a.blobs.len()), (0, vec![1234], None, 0));
        let b = rd.next().unwrap();
        assert_eq!((b.tick, b.scalars.clone(), b.ret), (7, vec![-1, 5], Some(2)));
        assert_eq!((b.blobs[0].len(), b.blobs[1].len()), (84, 0));
        assert!(rd.next().is_none());
        let mut b = 7u32.to_le_bytes().to_vec();
        b.extend(300u32.to_le_bytes());
        b.extend(1040i16.to_le_bytes());
        b.extend(0xdeadbeefu32.to_le_bytes());
        assert_eq!(state_hashes(&b), Some((7, 300, vec![(1040, 0xdeadbeef)])));
    }

    /// A record must reach the file within ~1 s without `finish()` (the game never
    /// unloads the DLL cleanly). Fails if a buffer sits between the encoder and the file.
    #[test]
    fn flush_reaches_disk() {
        let calls = [log::CallInfo { name: "DLL_SaveSaveGame", scalars: 1, has_ret: true }];
        let path = std::env::temp_dir().join(format!("shcrec-flush-{}.shcrec", std::process::id()));
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn({
            let f = std::fs::File::create(&path).unwrap();
            move || log::writer_loop(f, &calls, Vec::new(), rx)
        });
        let mut r = log::Rec::new(0, 42);
        r.i32(7);
        r.i32(1);
        r.blob(b"C:\\save.sav");
        tx.send(r.finish()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2500));
        let f = std::io::BufReader::new(std::fs::File::open(&path).unwrap());
        let mut rd = Reader::new(lz4_flex::frame::FrameDecoder::new(f)).unwrap();
        let c = rd.next().expect("record not on disk after flush");
        assert_eq!((c.tick, c.scalars, c.ret, c.blobs[0].len()), (42, vec![7], Some(1), 11));
        drop(tx);
        let _ = std::fs::remove_file(path);
    }
}
