//! One-off: find RNG cursors (a, b) whose state matches the recorded section-1040 hash at a RunTick.
//! Table = MSVC srand(seed) + 20000 rand(); layout a@0 b@2 seed@4 table@8 cursor_b@0x9c48 cursor_a@0x9c4c.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let calls = shc_formats::shcrec::read(&a[1]).expect("rec");
    let seed = calls.iter().find(|c| c.name == "DLL_SetMPRandSeed").unwrap().ints[0];
    let mut st = seed as u32;
    let table: Vec<u16> = (0..20000).map(|_| { st = st.wrapping_mul(214013).wrapping_add(2531011); ((st >> 16) & 0x7fff) as u16 }).collect();
    let mut img = vec![0u8; 40016];
    img[4..8].copy_from_slice(&seed.to_le_bytes());
    for (k, v) in table.iter().enumerate() { img[8 + k * 2..10 + k * 2].copy_from_slice(&v.to_le_bytes()); }
    let rts: Vec<_> = calls.iter().filter(|c| c.name == "DLL_RunTick").collect();
    for k in [0usize, 1, 2, 10] {
        let h = &rts[k].blobs[3];
        let want = h[8..].chunks_exact(6).find(|r| i16::from_le_bytes([r[0], r[1]]) == 1040).map(|r| u32::from_le_bytes(r[2..6].try_into().unwrap()));
        let Some(want) = want else { continue };
        let mut found = vec![];
        for ca in 0..400usize { for cb in 0..400usize {
            let mut im = img.clone();
            let av = if ca == 0 { 0 } else { table[ca - 1] };
            let bv = if cb == 0 { 0 } else { table[cb - 1] };
            im[0..2].copy_from_slice(&av.to_le_bytes()); im[2..4].copy_from_slice(&bv.to_le_bytes());
            im[0x9c48..0x9c4c].copy_from_slice(&(cb as i32).to_le_bytes()); im[0x9c4c..0x9c50].copy_from_slice(&(ca as i32).to_le_bytes());
            if shc_formats::map::engine_crc(&im) == want { found.push((ca, cb)); }
        }}
        println!("RunTick {k} (engine tick {}): cursors (a, b) matching: {found:?}", u32::from_le_bytes(h[0..4].try_into().unwrap()));
    }
}
