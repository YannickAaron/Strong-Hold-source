//! Usage: mapinfo <file.map>...   Parses each map, prints its sections, exits non-zero on any failure.
//! Set MAPINFO_BENCH=<n> to parse each file n times and print the average parse time.
//! Set MAPINFO_DUMP=<dir> to write each section of a single map to <dir>/<type_id>.bin.
fn main() {
    let mut failed = 0;
    let paths: Vec<String> = std::env::args().skip(1).collect();
    let verbose = paths.len() == 1;
    for path in &paths {
        let bytes = std::fs::read(path).expect("read");
        if let Some(n) = std::env::var("MAPINFO_BENCH").ok().and_then(|v| v.parse::<u32>().ok()) {
            let t = std::time::Instant::now();
            for _ in 0..n {
                std::hint::black_box(shc_formats::map::parse(&bytes).ok());
            }
            println!("{path}: {:?} per parse", t.elapsed() / n);
        }
        match shc_formats::map::parse(&bytes) {
            Ok(m) => {
                let total: usize = m.sections.iter().map(|s| s.data.len()).sum();
                println!("OK   {path}: version {}, {} header blocks, {} sections, {total} bytes, broken {:?}", m.version, m.header_blocks.len(), m.sections.len(), m.broken.iter().map(|b| b.0).collect::<Vec<_>>());
                if verbose {
                    let dump = std::env::var("MAPINFO_DUMP").ok();
                    for s in &m.sections {
                        if let Some(dir) = &dump {
                            std::fs::create_dir_all(dir).expect("mkdir");
                            std::fs::write(format!("{dir}/{}.bin", s.type_id), &s.data).expect("write");
                        }
                        println!("  type {:#06x} ({}) {:>9} bytes", s.type_id, s.type_id, s.data.len());
                    }
                }
            }
            Err(e) => {
                failed += 1;
                println!("FAIL {path}: {e:?}");
            }
        }
    }
    println!("{} ok, {failed} failed", paths.len() - failed);
    std::process::exit((failed > 0) as i32);
}
