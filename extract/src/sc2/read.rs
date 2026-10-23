//! Stronghold Crusader 2 file formats (re/SC2_MODELS.md): the zip containers, Vision Engine VBIN chunk files
//! (.model meshes / skeletons / weights, .anim clips) and DDS textures.
use crate::unity::read_range;
use crate::Result;
use std::{collections::HashMap, path::{Path, PathBuf}};

/// A zip read in place: `assets.v` (every byte XOR 0x55) or a `textures.*.pak` (plain). Entry names lowercase, `/`.
pub struct Zip { path: PathBuf, xor: u8, entries: HashMap<String, (u64, usize, usize, u16)> }

impl Zip {
    pub fn open(path: &Path, xor: u8) -> Result<Zip> {
        let len = std::fs::metadata(path)?.len();
        let read = |off: u64, n: usize| -> Result<Vec<u8>> { Ok(read_range(path, off, n)?.into_iter().map(|b| b ^ xor).collect()) };
        let tail_len = len.min(66 << 10) as usize;
        let tail = read(len - tail_len as u64, tail_len)?;
        let eocd = (0..tail.len().saturating_sub(21)).rev().find(|&i| tail[i..i + 4] == [0x50, 0x4b, 5, 6]).ok_or("zip: no end record")?;
        let (u16_, u32_) = (|b: &[u8], o: usize| u16::from_le_bytes([b[o], b[o + 1]]), |b: &[u8], o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()));
        let (n, cd_len, cd_off) = (u16_(&tail, eocd + 10) as usize, u32_(&tail, eocd + 12) as usize, u32_(&tail, eocd + 16) as u64);
        let cd = read(cd_off, cd_len)?;
        let mut entries = HashMap::with_capacity(n);
        let mut o = 0;
        for _ in 0..n {
            if cd.get(o..o + 4) != Some(&[0x50, 0x4b, 1, 2]) { return Err("zip: bad central directory".into()); }
            let (method, csize, size) = (u16_(&cd, o + 10), u32_(&cd, o + 20) as usize, u32_(&cd, o + 24) as usize);
            let (nl, el, cl) = (u16_(&cd, o + 28) as usize, u16_(&cd, o + 30) as usize, u16_(&cd, o + 32) as usize);
            let name = String::from_utf8_lossy(&cd[o + 46..o + 46 + nl]).replace('\\', "/").to_lowercase();
            entries.insert(name, (u32_(&cd, o + 42) as u64, csize, size, method));
            o += 46 + nl + el + cl;
        }
        Ok(Zip { path: path.to_path_buf(), xor, entries })
    }

    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        let key = name.trim_start_matches(['/', '\\']).replace('\\', "/").to_lowercase();
        let &(off, csize, size, method) = self.entries.get(&key).ok_or_else(|| format!("{}: no {key}", self.path.display()))?;
        let x = |v: Vec<u8>| -> Vec<u8> { v.into_iter().map(|b| b ^ self.xor).collect() };
        let local = x(read_range(&self.path, off, 30)?);
        let skip = 30 + u16::from_le_bytes([local[26], local[27]]) as u64 + u16::from_le_bytes([local[28], local[29]]) as u64;
        let raw = x(read_range(&self.path, off + skip, csize)?);
        let out = match method {
            0 => raw,
            8 => miniz_oxide::inflate::decompress_to_vec(&raw).map_err(|e| format!("{key}: inflate {e:?}"))?,
            m => return Err(format!("{key}: zip method {m}").into()),
        };
        if out.len() != size { return Err(format!("{key}: {} bytes, want {size}", out.len()).into()); }
        Ok(out)
    }
}

/// DDS level 0 → RGBA8 rows top-down. DXT1 / DXT5 only (all SC2 character textures).
pub fn dds(b: &[u8]) -> Result<(usize, usize, Vec<u8>)> {
    if b.get(..4) != Some(b"DDS ") || b.len() < 128 { return Err("dds: bad header".into()); }
    let (h, w) = (u32le(b, 12) as usize, u32le(b, 16) as usize);
    let mut buf = vec![0u32; w * h];
    let r = match &b[84..88] {
        b"DXT5" => texture2ddecoder::decode_bc3(&b[128..], w, h, &mut buf),
        b"DXT1" => texture2ddecoder::decode_bc1(&b[128..], w, h, &mut buf),
        f => return Err(format!("dds: format {:?}", String::from_utf8_lossy(f)).into()),
    };
    r.map_err(|e| format!("dds: {e}"))?;
    Ok((w, h, buf.into_iter().flat_map(|p| { let [b, g, r, a] = p.to_le_bytes(); [r, g, b, a] }).collect()))
}

fn u16le(b: &[u8], o: usize) -> u16 { u16::from_le_bytes([b[o], b[o + 1]]) }
fn u32le(b: &[u8], o: usize) -> u32 { u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) }
fn f32s<const N: usize>(b: &[u8], o: usize) -> [f32; N] { std::array::from_fn(|i| f32::from_le_bytes(b[o + 4 * i..o + 4 * i + 4].try_into().unwrap())) }

/// VBIN chunks in `b[o..end]`: (id, data range). A chunk = id (reversed on disk), u32 len, data, then a 12-byte end
/// marker (u32 depth, id, u32 depth). Stops at the first thing that isn't a chunk.
fn chunks(b: &[u8], mut o: usize, end: usize) -> Vec<([u8; 4], std::ops::Range<usize>)> {
    let tag = |o: usize| -> Option<[u8; 4]> { let t: [u8; 4] = b.get(o..o + 4)?.try_into().ok()?; t.iter().all(u8::is_ascii_uppercase).then(|| [t[3], t[2], t[1], t[0]]) };
    let mut out = vec![];
    while o + 8 <= end {
        let Some(t) = tag(o) else { break };
        let (ds, de) = (o + 8, o + 8 + u32le(b, o + 4) as usize);
        if de + 12 > b.len() || tag(de + 4) != Some(t) { break; }
        out.push((t, ds..de));
        o = de + 12;
    }
    out
}

pub struct Bone { pub name: String, pub parent: i16, pub inv: ([f32; 3], [f32; 4]), pub local: ([f32; 3], [f32; 4]) }

/// SKEL: u16 0, u16 count, per bone: u32 name length, name, i16 parent, inverse bind (pos, quat), local (pos, quat).
fn skel(b: &[u8], mut o: usize) -> Vec<Bone> {
    let n = u16le(b, o + 2);
    o += 4;
    (0..n).map(|_| {
        let l = u32le(b, o) as usize;
        let name = String::from_utf8_lossy(&b[o + 4..o + 4 + l]).into_owned();
        o += 4 + l;
        let parent = i16::from_le_bytes([b[o], b[o + 1]]);
        let f: [f32; 14] = f32s(b, o + 2);
        o += 58;
        Bone { name, parent, inv: ([f[0], f[1], f[2]], [f[3], f[4], f[5], f[6]]), local: ([f[7], f[8], f[9]], [f[10], f[11], f[12], f[13]]) }
    }).collect()
}

pub struct Model {
    pub pos: Vec<[f32; 3]>, pub nrm: Vec<[f32; 3]>, pub uv: Vec<[f32; 2]>, pub tris: Vec<[u32; 3]>,
    pub bones: Vec<Bone>,
    /// Up to 4 (bone, weight) per vertex, weights summing to 1; empty for rigid props.
    pub weights: Vec<[(u16, f32); 4]>,
}

/// A .model's VMSH (vertices, 16-bit triangle list), SKEL and WGHT.
pub fn model(b: &[u8]) -> Result<Model> {
    let top = chunks(b, 12, b.len());
    let get = |id: &[u8; 4]| top.iter().find(|c| &c.0 == id).map(|c| c.1.clone());
    let v = get(b"VMSH").ok_or("model: no VMSH")?;
    let ds = v.start;
    if u32le(b, ds + 0x10) != 0x01020a0b || u32le(b, ds + 0x48) != 0x01020a0b { return Err("model: unknown VMSH layout".into()); }
    let stride = u16le(b, ds + 0x18) as usize;
    let attr = |k: usize| { let c = u16le(b, ds + 0x1a + 2 * k); (c != 0xffff).then_some((c & 0xfff) as usize) };
    let (Some(p), Some(n), Some(t)) = (attr(0), attr(2), attr(3)) else { return Err("model: missing position / normal / uv".into()) };
    let (nv, ni) = (u32le(b, ds + 0x4c) as usize, u32le(b, ds + 0x55) as usize);
    let vo = ds + 0x6c;
    if vo + nv * stride + ni * 2 > v.end { return Err("model: VMSH too short".into()); }
    let at = |i: usize, o: usize| vo + i * stride + o;
    let pos = (0..nv).map(|i| f32s(b, at(i, p))).collect();
    let nrm = (0..nv).map(|i| f32s(b, at(i, n))).collect();
    let uv = (0..nv).map(|i| f32s(b, at(i, t))).collect();
    let io = vo + nv * stride;
    let tris = (0..ni / 3).map(|k| std::array::from_fn(|j| u16le(b, io + 2 * (3 * k + j)) as u32)).collect();
    let bones = get(b"SKEL").map(|s| skel(b, s.start)).unwrap_or_default();
    let mut weights = vec![];
    if let Some(w) = get(b"WGHT") {
        let mut o = w.start + 4;
        for _ in 0..nv {
            let k = u16le(b, o) as usize;
            let mut e = [(0u16, 0f32); 4];
            for j in 0..k {
                if j < 4 { e[j] = (u16le(b, o + 2 + 4 * j), u16le(b, o + 4 + 4 * j) as f32 / 65535.0); }
            }
            o += 2 + 4 * k;
            let s: f32 = e.iter().map(|x| x.1).sum();
            if s > 0.0 { for x in &mut e { x.1 /= s; } }
            weights.push(e);
        }
        if o != w.end { return Err("model: WGHT size mismatch".into()); }
    }
    Ok(Model { pos, nrm, uv, tris, bones, weights })
}

/// One animation clip: key times and per-bone keys (bone order = the .anim's own skeleton).
pub struct Clip { pub pos: (Vec<f32>, Vec<Vec<[f32; 3]>>), pub rot: (Vec<f32>, Vec<Vec<[f32; 4]>>) }
impl Clip { pub fn duration(&self) -> f32 { self.pos.0.last().copied().unwrap_or(0.0).max(self.rot.0.last().copied().unwrap_or(0.0)) } }

/// A master .anim: `ANIM { HEAD, SKEL, BANI × clips }`, each BANI = u32 0, u16 bones, u32 name length, name, u32 2,
/// then VSBX / BPOS / BROT chunks (plain floats).
pub fn anim(b: &[u8]) -> Result<(Vec<Bone>, HashMap<String, Clip>)> {
    let top = chunks(b, 12, b.len());
    let a = top.iter().find(|c| &c.0 == b"ANIM").ok_or("anim: no ANIM")?.1.clone();
    let kids = (0..16).map(|s| chunks(b, a.start + s, a.end)).find(|k| !k.is_empty()).unwrap_or_default();
    let sk = kids.iter().find(|c| &c.0 == b"SKEL").ok_or("anim: no SKEL")?;
    let (skel, mut clips) = (skel(b, sk.1.start), HashMap::new());
    for (_, r) in kids.iter().filter(|c| &c.0 == b"BANI") {
        let nb = u16le(b, r.start + 4) as usize;
        let l = u32le(b, r.start + 6) as usize;
        let name = String::from_utf8_lossy(&b[r.start + 10..r.start + 10 + l]).into_owned();
        let mut clip = Clip { pos: (vec![], vec![]), rot: (vec![], vec![]) };
        for (id, c) in chunks(b, r.start + 14 + l, r.end) {
            let n = u32le(b, c.start) as usize;
            let k = match &id { b"BPOS" => 3, b"BROT" => 4, _ => continue };
            let row = 4 + nb * k * 4;
            if c.len() != 4 + n * row { return Err(format!("anim {name}: {} size", String::from_utf8_lossy(&id)).into()); }
            let o = |i: usize| c.start + 4 + i * row;
            let times = (0..n).map(|i| f32s::<1>(b, o(i))[0]).collect();
            if k == 3 { clip.pos = (times, (0..n).map(|i| (0..nb).map(|j| f32s(b, o(i) + 4 + 12 * j)).collect()).collect()); }
            else { clip.rot = (times, (0..n).map(|i| (0..nb).map(|j| f32s(b, o(i) + 4 + 16 * j)).collect()).collect()); }
        }
        if clip.pos.0.is_empty() || clip.rot.0.is_empty() { return Err(format!("anim {name}: no keys").into()); }
        clips.insert(name, clip);
    }
    Ok((skel, clips))
}
