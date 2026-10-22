//! Lists the calls between RunTick #a and #b (indices among RunTicks). Usage: zz_calls_between <rec> a b
fn main() {
    let v: Vec<String> = std::env::args().collect();
    let calls = shc_formats::shcrec::read(&v[1]).unwrap();
    let (a, b): (usize, usize) = (v[2].parse().unwrap(), v[3].parse().unwrap());
    let mut k = 0usize;
    for c in &calls {
        if c.name == "DLL_RunTick" {
            if k >= a && k <= b {
                println!("RunTick #{k} proxy {} ints {:?}", c.tick, c.ints);
            }
            k += 1;
        } else if k > a && k <= b {
            println!("  {} {:?} blobs {:?}", c.name, c.ints, c.blobs.iter().map(|x| x.len()).collect::<Vec<_>>());
        }
    }
}
