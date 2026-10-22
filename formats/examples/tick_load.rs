//! Per-tick draw-list load of a recording (viewer stutter hunt): tiles, objects, rotations per RunTick.
//! Usage: cargo run --release -p shc-formats --example tick_load -- <rec.shcrec>
fn main() {
    let path = std::env::args().nth(1).expect("rec");
    let calls = shc_formats::shcrec::read(&path).expect("read");
    let rots: Vec<u32> = calls.iter().filter(|c| c.name == "DLL_SetMapRotation").map(|c| c.tick).collect();
    println!("rotations at ticks {rots:?}");
    let ticks: Vec<_> = calls.iter().filter(|c| c.name == "DLL_RunTick").map(|c| c.draw_list()).collect();
    let mut rows: Vec<(usize, usize, usize, usize)> = ticks.iter().enumerate().skip(1)
        .map(|(t, d)| (t, d.iter().filter(|r| r[0] == 0).count(), d.len(), d.iter().filter(|r| r[0] == 99 || r[0] < 0).count())).collect();
    let n = rows.len();
    let mean = rows.iter().map(|r| r.1).sum::<usize>() as f64 / n as f64;
    println!("{n} ticks, mean tile recs/tick {mean:.0}");
    rows.sort_by_key(|r| std::cmp::Reverse(r.1));
    for r in rows.iter().take(15) { println!("tick {:5}: tiles {:6} all {:6} other-marker {}", r.0, r.1, r.2, r.3); }
}
