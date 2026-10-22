//! Unity SerializedFile (format 22, Unity 2021.3, no type trees) and a byte cursor for objects.
//! Layout: UnityPy's SerializedFile.py; field orders of the classes we read: classes.rs.

use std::{fs::File, io, path::Path};

pub struct Obj { pub path_id: i64, pub class: i32, start: u64, pub size: u32 }

pub struct SFile { file: File, pub objects: Vec<Obj> }

fn read_at(f: &File, buf: &mut [u8], off: u64) -> io::Result<()> {
    #[cfg(unix)]
    { use std::os::unix::fs::FileExt; f.read_exact_at(buf, off) }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut done = 0;
        while done < buf.len() {
            let n = f.seek_read(&mut buf[done..], off + done as u64)?;
            if n == 0 { return Err(io::ErrorKind::UnexpectedEof.into()); }
            done += n;
        }
        Ok(())
    }
}

/// Reads `len` bytes at `off` of any file (the .resS / .resource streams).
pub fn read_range(path: &Path, off: u64, len: usize) -> io::Result<Vec<u8>> {
    let mut v = vec![0; len];
    read_at(&File::open(path)?, &mut v, off)?;
    Ok(v)
}

fn bad(what: &str) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, what.to_string()) }

impl SFile {
    pub fn open(path: &Path) -> io::Result<SFile> {
        let file = File::open(path).map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))?;
        let mut h = [0u8; 48];
        read_at(&file, &mut h, 0)?;
        let be32 = |o: usize| u32::from_be_bytes(h[o..o + 4].try_into().unwrap());
        let be64 = |o: usize| u64::from_be_bytes(h[o..o + 8].try_into().unwrap());
        if be32(8) != 22 || h[16] != 0 { return Err(bad("not a Unity 2021 SerializedFile (format 22, little endian)")); }
        let (meta_len, data_off) = (be32(20) as usize, be64(32));
        let mut m = vec![0; meta_len];
        read_at(&file, &mut m, 48)?;
        let mut r = Rd::new(&m);
        r.cstr(); // unity version
        r.i32(); // platform
        if r.u8() != 0 { return Err(bad("type trees present: unexpected build")); }
        let types: Vec<i32> = (0..r.i32()).map(|_| {
            let class = r.i32();
            r.skip(1 + 2 + if class == 114 { 16 } else { 0 } + 16); // stripped, script index, [script id], hash
            class
        }).collect();
        let n = r.i32();
        let mut objects = Vec::with_capacity(n as usize);
        for _ in 0..n {
            r.align();
            let path_id = r.i64();
            let start = r.u64();
            let size = r.u32();
            let class = *types.get(r.i32() as usize).ok_or_else(|| bad("type index"))?;
            objects.push(Obj { path_id, class, start: data_off + start, size });
        }
        Ok(SFile { file, objects })
    }

    pub fn read(&self, o: &Obj) -> io::Result<Vec<u8>> {
        let mut v = vec![0; o.size as usize];
        read_at(&self.file, &mut v, o.start)?;
        Ok(v)
    }

    /// The first `n` bytes of an object (names, headers) without reading big inline pixel data.
    pub fn read_prefix(&self, o: &Obj, n: usize) -> io::Result<Vec<u8>> {
        let mut v = vec![0; n.min(o.size as usize)];
        read_at(&self.file, &mut v, o.start)?;
        Ok(v)
    }

    /// Objects of one class id (28 Texture2D, 213 Sprite, 83 AudioClip, 49 TextAsset, 114 MonoBehaviour, 115 MonoScript),
    /// in object-table order, read.
    pub fn each(&self, class: i32) -> impl Iterator<Item = io::Result<(&Obj, Vec<u8>)>> {
        self.objects.iter().filter(move |o| o.class == class).map(|o| self.read(o).map(|b| (o, b)))
    }
}

/// Little-endian cursor; `align` is relative to the start of the slice (objects start 8-aligned).
pub struct Rd<'a> { pub b: &'a [u8], pub p: usize }

impl<'a> Rd<'a> {
    pub fn new(b: &'a [u8]) -> Self { Rd { b, p: 0 } }
    pub fn take(&mut self, n: usize) -> &'a [u8] {
        let s = self.b.get(self.p..self.p + n).unwrap_or(&[]);
        self.p += n;
        s
    }
    fn arr<const N: usize>(&mut self) -> [u8; N] { self.take(N).try_into().unwrap_or([0; N]) }
    pub fn skip(&mut self, n: usize) { self.p += n; }
    pub fn align(&mut self) { self.p = (self.p + 3) & !3; }
    pub fn u8(&mut self) -> u8 { self.arr::<1>()[0] }
    pub fn i32(&mut self) -> i32 { i32::from_le_bytes(self.arr()) }
    pub fn u32(&mut self) -> u32 { u32::from_le_bytes(self.arr()) }
    pub fn i64(&mut self) -> i64 { i64::from_le_bytes(self.arr()) }
    pub fn u64(&mut self) -> u64 { u64::from_le_bytes(self.arr()) }
    pub fn f32(&mut self) -> f32 { f32::from_le_bytes(self.arr()) }
    /// i32 length + bytes, aligned (Unity `vector<UInt8>` / TypelessData).
    pub fn bytes(&mut self) -> &'a [u8] {
        let n = self.i32().max(0) as usize;
        let s = self.take(n);
        self.align();
        s
    }
    /// Unity `string`: i32 length + UTF-8, aligned.
    pub fn str(&mut self) -> String { String::from_utf8_lossy(self.bytes()).into_owned() }
    /// PPtr: (file id, path id).
    pub fn pptr(&mut self) -> (i32, i64) { (self.i32(), self.i64()) }
    fn cstr(&mut self) -> String {
        let n = self.b[self.p..].iter().position(|&c| c == 0).unwrap_or(0);
        let s = String::from_utf8_lossy(self.take(n)).into_owned();
        self.p += 1;
        s
    }
}
