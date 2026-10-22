//! `play <rec.shcrec> [cache dir]`: plays a recording's sound events in real time (38 ticks/s), no graphics.
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(rec) = args.get(1) else { eprintln!("usage: play <rec.shcrec> [cache dir]"); std::process::exit(2) };
    let cache = args.get(2).map(Into::into).unwrap_or_else(|| std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Library/Caches/shc-audio"));
    let calls = shc_formats::shcrec::read(rec).expect("read recording");
    let mut audio = shc_audio::Audio::new(&cache);
    let start = Instant::now();
    for c in calls.iter().filter(|c| c.name == "DLL_RunTick") {
        let due = Duration::from_secs_f64(c.tick as f64 / 38.0);
        if let Some(d) = due.checked_sub(start.elapsed()) { std::thread::sleep(d) }
        let ret = c.blobs.get(1).map(Vec::as_slice).unwrap_or(&[]);
        for r in c.draw_list() {
            if let Some(line) = audio.play_record(&r, ret) { println!("t{:5} {line}", c.tick) }
        }
    }
    std::thread::sleep(Duration::from_secs(2));
}
