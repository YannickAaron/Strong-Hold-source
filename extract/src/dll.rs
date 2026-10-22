//! The sim's game-derived bytes, rebuilt from the user's own download (re/scripts/gen_dll_delta.py):
//! - image_data.bin / image_rdata2.bin (sim/src/mem.rs, V2.8.1 layout) from CrusaderDE.dll + data/crusaderde.delta
//! - aiv.tsv (the front end's AI castle layouts, AIVLoader.cs) carved from Managed/Assembly-CSharp.dll by signature
//!   (re/data/aiv_sig.tsv): lord \t layout \t comma-separated shorts.
use crate::Result;
use std::{fmt::Write as _, fs, path::Path};

const DELTA: &[u8] = include_bytes!("../data/crusaderde.delta");
const AIV_SIG: &str = include_str!("../../re/data/aiv_sig.tsv");

struct Rd<'a>(&'a [u8], usize);
impl Rd<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> { let s = self.0.get(self.1..self.1 + n).ok_or("delta: truncated")?; self.1 += n; Ok(s) }
    fn u32(&mut self) -> Result<u32> { Ok(u32::from_le_bytes(self.take(4)?.try_into()?)) }
    fn u8(&mut self) -> Result<u8> { Ok(self.take(1)?[0]) }
}

/// The game DLL the delta was built from: Plugins/x86_64/CrusaderDE.dll (CrusaderDE_orig.dll when our recorder
/// proxy sits in its place).
fn game_dll(data: &Path, crc: u32) -> Result<Vec<u8>> {
    for n in ["CrusaderDE.dll", "CrusaderDE_orig.dll"] {
        if let Ok(b) = fs::read(data.join("Plugins/x86_64").join(n)) {
            if crc32fast::hash(&b) == crc { return Ok(b); }
        }
    }
    Err("CrusaderDE.dll is not the game version this build of the port supports".into())
}

pub fn export(data: &Path, out: &Path) -> Result<()> {
    fs::create_dir_all(out)?;
    let mut r = Rd(DELTA, 0);
    if r.take(4)? != b"SHCD" { return Err("delta: magic".into()); }
    let (crc, _len, n) = (r.u32()?, r.u32()?, r.u32()?);
    let src = game_dll(data, crc)?;
    for _ in 0..n {
        let name = String::from_utf8(r.take(r.0[r.1] as usize + 1)?[1..].to_vec())?;
        let (len, want, ops) = (r.u32()? as usize, r.u32()?, r.u32()?);
        let mut o = Vec::with_capacity(len);
        for _ in 0..ops {
            match r.u8()? {
                0 => { let (s, n) = (r.u32()? as usize, r.u32()? as usize); o.extend_from_slice(src.get(s..s + n).ok_or("delta: copy range")?); }
                1 => { let n = r.u32()? as usize; o.extend_from_slice(r.take(n)?); }
                _ => { let n = r.u32()? as usize; o.resize(o.len() + n, 0); }
            }
        }
        if o.len() != len || crc32fast::hash(&o) != want { return Err(format!("delta: {name} does not verify").into()); }
        fs::write(out.join(&name), o)?;
    }
    // AIV layouts
    let asm = fs::read(data.join("Managed/Assembly-CSharp.dll"))?;
    let mut tsv = String::new();
    for l in AIV_SIG.lines() {
        let f: Vec<&str> = l.split('\t').collect();
        let [name, k, len, crc, head] = f[..] else { return Err("aiv_sig.tsv: bad row".into()) };
        let (len, crc) = (len.parse::<usize>()? * 2, u32::from_str_radix(crc, 16)?);
        let prefix: Vec<u8> = head.split(',').flat_map(|v| v.parse::<i16>().unwrap_or(0).to_le_bytes()).collect();
        let raw = asm.windows(prefix.len()).enumerate().filter(|(_, w)| *w == prefix.as_slice())
            .map(|(i, _)| &asm[i..(i + len).min(asm.len())]).find(|r| r.len() == len && crc32fast::hash(r) == crc)
            .ok_or_else(|| format!("AIV layout {name} {k} not found in Assembly-CSharp.dll"))?;
        let vals: Vec<String> = raw.chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]]).to_string()).collect();
        writeln!(tsv, "{name}\t{k}\t{}", vals.join(","))?;
    }
    fs::write(out.join("aiv.tsv"), tsv)?;
    Ok(())
}
