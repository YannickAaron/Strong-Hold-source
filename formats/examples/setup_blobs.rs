// scratch: compact view of pre-tick ApplyMultiplayerSetupData blobs (i32 words 0..0x58)
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let calls = shc_formats::shcrec::read(&a[1]).unwrap();
    for c in calls.iter().take_while(|c| c.name != "DLL_RunTick") {
        if c.name != "DLL_ApplyMultiplayerSetupData" { if c.name.contains("Register") || c.name.contains("LoadMap") { println!("{} {:?}", c.name, c.ints); } continue; }
        let b = &c.blobs[0];
        let w: Vec<i32> = b[..0x58].chunks(4).map(|x| i32::from_le_bytes(x.try_into().unwrap())).collect();
        println!("{:?}", w);
    }
}
