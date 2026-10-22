//! Dev probe: write tick 0's type-0 tile records of a recording as TSV (k0..k20).
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let calls = shc_formats::shcrec::read(&a[1]).unwrap();
    let t0 = calls.iter().find(|c| c.name == "DLL_RunTick").unwrap();
    let mut out = String::new();
    for r in t0.draw_list().iter().filter(|r| r[0] == 0) {
        out += &r.iter().map(|v| v.to_string()).collect::<Vec<_>>().join("\t");
        out.push('\n');
    }
    std::fs::write(&a[2], out).unwrap();
}
