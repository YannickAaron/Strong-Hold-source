//! Lists every call of a recording before the first DLL_RunTick, in order, with all ints and blob sizes.
fn main() {
    let calls = shc_formats::shcrec::read(&std::env::args().nth(1).expect("rec")).expect("read");
    for c in calls.iter().take_while(|c| c.name != "DLL_RunTick") {
        let bl: Vec<usize> = c.blobs.iter().map(|b| b.len()).collect();
        println!("{} {:?} blobs {:?}", c.name, c.ints, bl);
    }
}
