//! Section ids / sizes of a map or save, and parse errors. Usage: zz_map_sections <file>
fn main() {
    let f = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
    let m = shc_formats::map::parse(&f).unwrap();
    println!("version {} headers {}", m.version, m.header_blocks.len());
    for s in &m.sections {
        print!("{}:{} ", s.type_id, s.data.len());
    }
    println!("\nbroken {:?}", m.broken.iter().map(|b| b.0).collect::<Vec<_>>());
}
