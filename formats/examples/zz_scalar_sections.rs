//! Recovers the values of the small (4 / 8 byte) save sections from a recording's per-RunTick section hashes
//! by brute force over i32 values. Usage: zz_scalar_sections <rec.shcrec> <sections.tsv> [runticks]
use shc_formats::map::engine_crc;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let calls = shc_formats::shcrec::read(&a[1]).unwrap();
    let n: usize = a.get(3).map_or(3, |v| v.parse().unwrap());
    let sizes: std::collections::HashMap<i16, usize> = std::fs::read_to_string(&a[2]).unwrap().lines().skip(1)
        .map(|l| { let c: Vec<&str> = l.split('\t').collect(); (c[0].parse().unwrap(), c[3].parse().unwrap()) }).collect();
    let cand: Vec<i64> = (-200000..20_000_000).chain([i32::MIN as i64, i32::MAX as i64]).collect();
    for (k, rt) in calls.iter().filter(|c| c.name == "DLL_RunTick").take(n).enumerate() {
        let h = &rt.blobs[3];
        print!("RunTick {k} (engine tick {}):", u32::from_le_bytes(h[0..4].try_into().unwrap()));
        for r in h[8..].chunks_exact(6) {
            let id = i16::from_le_bytes([r[0], r[1]]);
            let want = u32::from_le_bytes(r[2..6].try_into().unwrap());
            let sz = sizes.get(&id).copied().unwrap_or(0);
            if sz != 4 && sz != 8 { continue; }
            let v = cand.iter().find(|&&v| {
                let b = v.to_le_bytes();
                engine_crc(&b[..sz]) == want
            });
            print!(" {id}={}", v.map_or("?".into(), |v| v.to_string()));
        }
        println!();
    }
}
