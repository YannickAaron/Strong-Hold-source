//! Dev probe: print a map's stored name and write its 200x200 preview as a PPM.
//! Usage: cargo run --release --example headers -- <file.map> <out.ppm>
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let m = shc_formats::map::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    println!("name: {:?}", m.file_name());
    let rgba = m.preview_rgba().expect("preview");
    let rgb: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    std::fs::write(&a[2], [b"P6\n200 200\n255\n".as_slice(), &rgb].concat()).unwrap();
}
