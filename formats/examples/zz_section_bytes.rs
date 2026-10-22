//! Print bytes of a map/save section: zz_section_bytes <file> <id> <offset-hex> <len>
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let f = std::fs::read(&a[1]).unwrap();
    let m = shc_formats::map::parse(&f).unwrap();
    let id: i32 = a[2].parse().unwrap();
    let o = usize::from_str_radix(a[3].trim_start_matches("0x"), 16).unwrap();
    let n: usize = a[4].parse().unwrap();
    let s = m.sections.iter().find(|s| s.type_id == id).expect("section");
    println!("section {id}: {} bytes; at {o:#x}: {:02x?}", s.data.len(), &s.data[o.min(s.data.len())..(o + n).min(s.data.len())]);
}
