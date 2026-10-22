//! Engine tick (RunTick blob 3) per RunTick. Usage: zz_engine_ticks <rec> n
fn main() {
    let v: Vec<String> = std::env::args().collect();
    let calls = shc_formats::shcrec::read(&v[1]).unwrap();
    let n: usize = v[2].parse().unwrap();
    let t: Vec<String> = calls.iter().filter(|c| c.name == "DLL_RunTick").take(n)
        .map(|c| c.blobs.get(3).filter(|h| h.len() >= 8).map_or("-".into(), |h| u32::from_le_bytes(h[0..4].try_into().unwrap()).to_string())).collect();
    println!("{}", t.join(" "));
}
