//! Textures by name across asset files, decoded to RGBA8 rows top-down (UnityPy's `.image`), plus resize/crop.
use crate::classes::{self, TEXTURE2D};
use crate::unity::{read_range, SFile};
use crate::Result;
use std::{collections::HashMap, path::{Path, PathBuf}};

#[derive(Clone)]
pub struct Rgba { pub w: usize, pub h: usize, pub px: Vec<u8> }

impl Rgba {
    pub fn new(w: usize, h: usize) -> Rgba { Rgba { w, h, px: vec![0; w * h * 4] } }
    /// A PNG file of the game's (Assets/GUI/Images/*.png: the credits pictures, logos).
    pub fn png(path: &Path) -> Result<Rgba> {
        let im = image::load_from_memory_with_format(&std::fs::read(path)?, image::ImageFormat::Png)?.to_rgba8();
        Ok(Rgba { w: im.width() as usize, h: im.height() as usize, px: im.into_raw() })
    }
    /// Lanczos3 on premultiplied alpha, like Pillow's LANCZOS on RGBA (no colour halos from transparent pixels);
    /// same size = copy, as Pillow.
    pub fn resize(&self, w: usize, h: usize) -> Rgba {
        if (w, h) == (self.w, self.h) { return self.clone(); }
        let opaque = self.px.chunks(4).all(|p| p[3] == 255);
        let pm: Vec<f32> = self.px.chunks(4).flat_map(|p| {
            let a = p[3] as f32 / 255.0; // image clamps f32 pixels to 0..1
            [p[0] as f32 / 255.0 * a, p[1] as f32 / 255.0 * a, p[2] as f32 / 255.0 * a, a]
        }).collect();
        let img = image::Rgba32FImage::from_raw(self.w as u32, self.h as u32, pm).unwrap();
        let r = image::imageops::resize(&img, w as u32, h as u32, image::imageops::FilterType::Lanczos3);
        let px = r.into_raw().chunks(4).flat_map(|p| {
            let a = if opaque { 1.0 } else { p[3].clamp(0.0, 1.0) };
            let un = |c: f32| if a > 0.0 { (c / a * 255.0).round().clamp(0.0, 255.0) as u8 } else { 0 };
            [un(p[0]), un(p[1]), un(p[2]), (a * 255.0).round() as u8]
        }).collect();
        Rgba { w, h, px }
    }
    pub fn crop(&self, x: usize, y: usize, w: usize, h: usize) -> Rgba {
        let mut o = Rgba::new(w, h);
        for row in 0..h.min(self.h.saturating_sub(y)) {
            let n = w.min(self.w.saturating_sub(x)) * 4;
            let s = ((y + row) * self.w + x) * 4;
            o.px[row * w * 4..][..n].copy_from_slice(&self.px[s..s + n]);
        }
        o
    }
    pub fn paste(&mut self, src: &Rgba, x: usize, y: usize) {
        for row in 0..src.h {
            let d = ((y + row) * self.w + x) * 4;
            self.px[d..d + src.w * 4].copy_from_slice(&src.px[row * src.w * 4..][..src.w * 4]);
        }
    }
}

/// Texture2D pixels (level 0) -> RGBA8 top-down. Formats: 3 RGB24, 4 RGBA32, 10 DXT1, 12 DXT5, 25 BC7.
pub fn decode(data: &Path, t: &classes::Texture2D) -> Result<Rgba> {
    let (w, h) = (t.w, t.h);
    let blocks = w.div_ceil(4) * h.div_ceil(4);
    let size = match t.format { 3 => w * h * 3, 4 => w * h * 4, 10 => blocks * 8, 12 | 25 => blocks * 16, f => return Err(format!("{}: texture format {f}", t.name).into()) };
    let raw = if t.image.is_empty() {
        let (off, _, path) = &t.stream;
        read_range(&data.join(path.rsplit('/').next().unwrap_or(path)), *off, size)?
    } else { t.image.get(..size).ok_or("texture: short image data")?.to_vec() };
    let mut px = vec![0u8; w * h * 4];
    match t.format {
        3 => for (d, s) in px.chunks_mut(4).zip(raw.chunks(3)) { d[..3].copy_from_slice(s); d[3] = 255; },
        4 => px.copy_from_slice(&raw),
        f => {
            let mut buf = vec![0u32; w * h];
            let r = match f { 10 => texture2ddecoder::decode_bc1(&raw, w, h, &mut buf), 12 => texture2ddecoder::decode_bc3(&raw, w, h, &mut buf), _ => texture2ddecoder::decode_bc7(&raw, w, h, &mut buf) };
            r.map_err(|e| format!("{}: {e}", t.name))?;
            for (d, p) in px.chunks_mut(4).zip(buf) { let [b, g, r, a] = p.to_le_bytes(); d.copy_from_slice(&[r, g, b, a]); }
        }
    }
    // Unity rows are bottom-up
    let row = w * 4;
    let mut flipped = vec![0u8; px.len()];
    for y in 0..h { flipped[y * row..][..row].copy_from_slice(&px[(h - 1 - y) * row..][..row]); }
    Ok(Rgba { w, h, px: flipped })
}

/// Every Texture2D of some asset files by name (first one wins, file order then table order).
pub struct Textures { data: PathBuf, files: Vec<SFile>, by_name: HashMap<String, (usize, usize)> }

impl Textures {
    pub fn open(data: &Path, files: &[&str]) -> Result<Textures> {
        let mut t = Textures { data: data.to_path_buf(), files: vec![], by_name: HashMap::new() };
        for f in files {
            let sf = SFile::open(&data.join(f))?;
            for (i, o) in sf.objects.iter().enumerate().filter(|(_, o)| o.class == TEXTURE2D) {
                let name = classes::name(&sf.read_prefix(o, 512)?);
                t.by_name.entry(name).or_insert((t.files.len(), i));
            }
            t.files.push(sf);
        }
        Ok(t)
    }
    pub fn get(&self, name: &str) -> Result<Rgba> {
        let &(f, i) = self.by_name.get(name).ok_or_else(|| format!("texture {name} not found"))?;
        let sf = &self.files[f];
        decode(&self.data, &classes::texture2d(&sf.read(&sf.objects[i])?))
    }
}
