//! Dump AIV castle layouts.
//!   cargo run --release --example aiv_dump -- <rec.shcrec> [lord] [id]        recorded DLL_ImportAIV blobs
//!   cargo run --release --example aiv_dump -- re/csharp/AIVLoader.cs <array> [id]   e.g. rat 0, Evrey_wolf 3
//!   add `--survey` to print one summary line per layout instead of the full dump.
use shc_formats::aiv::{self, Aiv};
use std::collections::BTreeMap;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).filter(|a| a != "--survey").collect();
    let survey = std::env::args().any(|a| a == "--survey");
    let Some(src) = args.first() else {
        eprintln!("usage: aiv_dump <rec.shcrec | AIVLoader.cs> [lord|array] [id] [--survey]");
        return;
    };
    let layouts: Vec<(String, Vec<i16>)> = if src.ends_with(".cs") {
        cs_arrays(&std::fs::read_to_string(src).unwrap())
    } else {
        shc_formats::shcrec::read(src)
            .unwrap()
            .into_iter()
            .filter(|c| c.name == "DLL_ImportAIV" && c.ints[0] >= 0)
            .map(|c| (format!("lord{} id{}", c.ints[0], c.ints[1]), aiv::from_le_bytes(&c.blobs[0])))
            .collect()
    };
    let want = |name: &str| match (args.get(1), args.get(2)) {
        (None, _) => true,
        (Some(l), None) => name.starts_with(l.as_str()) && name[l.len()..].starts_with(|c: char| c == ' ' || c == '[') || name == *l,
        (Some(l), Some(i)) => name == format!("{l} id{i}") || name == format!("{l}[{i}]"),
    };
    for (name, d) in layouts.iter().filter(|(n, _)| want(n)) {
        match aiv::parse(d) {
            Err(e) => println!("{name}: ERROR {e}"),
            Ok(a) if survey => println!("{name}\t{}", summary(&a)),
            Ok(a) => dump(name, &a),
        }
    }
}

fn summary(a: &Aiv) -> String {
    let mut kinds: BTreeMap<i16, usize> = BTreeMap::new();
    for s in &a.steps {
        *kinds.entry(s.kind).or_default() += s.tiles.len();
    }
    let mut slots: BTreeMap<i16, usize> = BTreeMap::new();
    for t in &a.troops {
        *slots.entry(t.slot).or_default() += 1;
    }
    let k: Vec<String> = kinds.iter().filter(|(k, _)| ![25, 26].contains(*k)).map(|(k, n)| format!("{}×{n}", aiv::kind_name(*k))).collect();
    let s: Vec<String> = slots.iter().map(|(k, n)| format!("s{k}×{n}")).collect();
    format!("steps={} walls={} pauses={:?} troops=[{}] {}", a.steps.len(), kinds.get(&25).unwrap_or(&0) + kinds.get(&26).unwrap_or(&0), a.pause_steps, s.join(" "), k.join(" "))
}

fn dump(name: &str, a: &Aiv) {
    println!("== {name}: pause_delay={} pause_steps={:?} steps={} troops={}", a.pause_delay, a.pause_steps, a.steps.len(), a.troops.len());
    for (i, s) in a.steps.iter().enumerate() {
        let fp = aiv::footprint(s.kind);
        let tiles: Vec<String> = s.tiles.iter().map(|(x, y)| format!("{x},{y}")).collect();
        println!("  {i:3} {:>3} {:<16} {}x{}  {}", s.kind, aiv::kind_name(s.kind), fp.max(1), fp.max(1), tiles.join(" "));
    }
    for t in &a.troops {
        println!("  troop slot {:>2} {:<20} #{} at {},{}", t.slot, aiv::slot_name(t.slot), t.index, t.x, t.y);
    }
    print!("{}", a.render());
}

/// Pull `private static short[][] NAME = new short[8][] { new short[N] { ... }, ... }` out of AIVLoader.cs.
fn cs_arrays(src: &str) -> Vec<(String, Vec<i16>)> {
    let mut out = vec![];
    for (name, body) in src.split("private static short[][] ").skip(1).filter_map(|s| s.split_once(" = new short[8][]")) {
        let body = body.split("};").next().unwrap_or("");
        for (id, arr) in body.split("new short[").skip(1).enumerate() {
            let inner = arr.split_once('{').map(|(_, r)| r).unwrap_or("");
            let inner = inner.split('}').next().unwrap_or("");
            let vals: Vec<i16> = inner.split(',').filter_map(|t| t.trim().parse().ok()).collect();
            out.push((format!("{name}[{id}]"), vals));
        }
    }
    out
}
