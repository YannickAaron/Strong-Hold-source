//! Lists the distinct call names of a recording with their first tick and string blobs (map names etc.).
fn main() {
    let calls = shc_formats::shcrec::read(&std::env::args().nth(1).expect("rec")).expect("read");
    let mut seen = std::collections::BTreeSet::new();
    for c in &calls {
        if seen.insert(c.name.clone()) {
            let txt: Vec<String> = c.blobs.iter().filter(|b| b.len() < 300).map(|b| String::from_utf8_lossy(b).replace('\0', "")).collect();
            println!("{:5} {} ints {:?} {:?}", c.tick, c.name, &c.ints[..c.ints.len().min(6)], txt);
        }
    }
    let rt: Vec<_> = calls.iter().filter(|c| c.name == "DLL_RunTick").collect();
    println!("{} RunTicks, blobs per RunTick {}", rt.len(), rt[0].blobs.len());
}
