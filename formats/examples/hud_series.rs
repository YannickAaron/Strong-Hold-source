//! Dev probe: print HUD popularity/population/gold (PlayStateReturnData @108/112/116) for a tick range.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (from, to): (u32, u32) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    for c in shc_formats::shcrec::read(&a[1]).unwrap().iter().filter(|c| c.name == "DLL_RunTick" && (from..=to).contains(&c.tick)) {
        let rd = &c.blobs[1];
        let i = |o: usize| i32::from_le_bytes(rd[o..o + 4].try_into().unwrap());
        println!("{} pop% {} popn {} gold {} housing {} res0..4 {:?}", c.tick, i(108), i(112), i(116), i(120), (0..5).map(|k| i(k * 4)).collect::<Vec<_>>());
    }
}
