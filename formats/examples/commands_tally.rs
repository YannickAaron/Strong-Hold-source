//! Tally player-command calls in recorder logs. `cargo run --release --example commands_tally -- a.shcrec b.shcrec [-v]`
use std::collections::BTreeMap;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let verbose = args.iter().any(|a| a == "-v");
    let mut total: BTreeMap<(String, i32), [usize; 4]> = BTreeMap::new();
    let files: Vec<&String> = args.iter().filter(|a| *a != "-v").collect();
    for (fi, f) in files.iter().enumerate() {
        for c in shc_formats::shcrec::read(f).unwrap() {
            let n = c.name.trim_start_matches("DLL_");
            let key = match n {
                "GameAction" | "StartMapAction" => c.ints[0],
                // action*10 + mouseState (0 hover, 1 down, 2 drag, 3 up)
                "MapAction" => c.ints[0] * 10 + c.ints[7],
                "TroopSelection" => c.ints[0], // mouseState
                "TroopSelectionChanged" | "ReceiveChore" => -1,
                _ => continue,
            };
            total.entry((n.to_string(), key)).or_default()[fi] += 1;
            if verbose && n != "MapAction" {
                let lens: Vec<usize> = c.blobs.iter().map(|b| b.len()).collect();
                println!("f{fi} t{} {n} {:?} blobs{:?}", c.tick, c.ints, lens);
            }
        }
    }
    for ((n, k), v) in total {
        println!("{n}\t{k}\t{}\t{:?}", v.iter().sum::<usize>(), &v[..files.len()]);
    }
}
