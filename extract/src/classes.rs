//! Hand parsers for the classes we read, field order from Unity 2021.3.45f2's type trees (UnityPy's TPK).
//! Only the fields up to the last one we need are read.
use crate::unity::Rd;

pub const TEXTURE2D: i32 = 28;
pub const TEXT_ASSET: i32 = 49;
pub const AUDIO_CLIP: i32 = 83;
pub const MONO_BEHAVIOUR: i32 = 114;
pub const MONO_SCRIPT: i32 = 115;
pub const SPRITE: i32 = 213;

/// m_Name: the first field of every named object (Texture2D, Sprite, AudioClip, TextAsset, MonoScript).
pub fn name(b: &[u8]) -> String { Rd::new(b).str() }

pub struct Texture2D {
    pub name: String,
    pub w: usize,
    pub h: usize,
    pub format: i32,
    pub mips: i32,
    /// Inline pixels (`image data`); empty when streamed.
    pub image: Vec<u8>,
    /// m_StreamData: offset, size, path (`archive:/…/resources.assets.resS`), when streamed.
    pub stream: (u64, u32, String),
}

pub fn texture2d(b: &[u8]) -> Texture2D {
    let mut r = Rd::new(b);
    let name = r.str();
    r.i32(); // m_ForcedFallbackFormat
    r.skip(2); r.align(); // DownscaleFallback, IsAlphaChannelOptional
    let w = r.i32() as usize;
    let h = r.i32() as usize;
    r.u32(); // m_CompleteImageSize
    r.i32(); // m_MipsStripped
    let format = r.i32();
    let mips = r.i32();
    r.skip(4); r.align(); // IsReadable, IsPreProcessed, IgnoreMasterTextureLimit, StreamingMipmaps
    r.skip(4 + 4 + 4 + 24 + 4 + 4); // StreamingMipmapsPriority, ImageCount, TextureDimension, TextureSettings, LightmapFormat, ColorSpace
    r.bytes(); // m_PlatformBlob
    let image = r.bytes().to_vec();
    let stream = (r.u64(), r.u32(), r.str());
    Texture2D { name, w, h, format, mips, image, stream }
}

pub struct Sprite {
    pub name: String,
    pub rect: [f32; 4],
    pub ppu: f32,
    pub pivot: [f32; 2],
    /// m_RD.texture path id (file id 0: same file).
    pub texture: i64,
    /// Vertex positions (x, y) of channel 0 and the triangles' indices (sub meshes in order, topology 0).
    pub verts: Vec<[f32; 2]>,
    pub tris: Vec<u16>,
}

pub fn sprite(b: &[u8]) -> Sprite {
    let mut r = Rd::new(b);
    let name = r.str();
    let rect = [r.f32(), r.f32(), r.f32(), r.f32()];
    r.skip(8 + 16); // m_Offset, m_Border
    let ppu = r.f32();
    let pivot = [r.f32(), r.f32()];
    r.u32(); // m_Extrude
    r.skip(1); r.align(); // m_IsPolygon
    r.skip(24); // m_RenderDataKey
    for _ in 0..r.i32() { r.str(); } // m_AtlasTags
    r.pptr(); r.align(); // m_SpriteAtlas
    // m_RD
    let texture = r.pptr().1;
    r.pptr(); // alphaTexture
    for _ in 0..r.i32() { r.pptr(); r.str(); } // secondaryTextures
    let subs: Vec<(u32, u32, i32)> = (0..r.i32()).map(|_| {
        let s = (r.u32(), r.u32(), r.i32()); // firstByte, indexCount, topology
        r.skip(12 + 24);
        s
    }).collect();
    let index = r.bytes();
    let count = r.u32() as usize;
    let channels: Vec<[u8; 4]> = (0..r.i32()).map(|_| r.take(4).try_into().unwrap_or([0; 4])).collect(); // stream, offset, format, dimension
    r.align();
    let data = r.bytes();
    // MeshHelper.get_streams: stride per stream = sum of its channels' (dimension & 15) * component size; streams 16-aligned
    let size = |fmt: u8| match fmt { 0 | 4 | 5 | 10 | 11 => 4, 1 | 6 | 7 | 8 | 9 => 2, _ => 1 }; // float, half/16-bit, 8-bit
    let nstreams = channels.iter().map(|c| c[0] as usize + 1).max().unwrap_or(0);
    let (mut offs, mut strides, mut off) = (vec![0; nstreams], vec![0; nstreams], 0);
    for s in 0..nstreams {
        let stride: usize = channels.iter().filter(|c| c[0] as usize == s && c[3] > 0).map(|c| (c[3] & 15) as usize * size(c[2])).sum();
        offs[s] = off;
        strides[s] = stride;
        off = (off + count * stride + 15) & !15;
    }
    let verts = match channels.first() {
        Some(&[s, o, 0, d]) if d & 15 >= 2 => (0..count).map(|i| {
            let p = offs[s as usize] + i * strides[s as usize] + o as usize;
            let f = |k: usize| data.get(p + k..p + k + 4).map_or(0.0, |x| f32::from_le_bytes(x.try_into().unwrap()));
            [f(0), f(4)]
        }).collect(),
        _ => vec![],
    };
    let idx = |i: usize| index.get(i * 2..i * 2 + 2).map_or(0, |x| u16::from_le_bytes(x.try_into().unwrap()));
    let mut tris = vec![];
    for (first, n, topology) in subs {
        if topology == 0 {
            let first = first as usize / 2;
            tris.extend((first..first + n as usize / 3 * 3).map(idx));
        }
    }
    Sprite { name, rect, ppu, pivot, texture, verts, tris }
}

pub struct AudioClip { pub name: String, pub channels: i32, pub freq: i32, pub source: String, pub offset: u64, pub size: u64, pub compression: i32 }

pub fn audio_clip(b: &[u8]) -> AudioClip {
    let mut r = Rd::new(b);
    let name = r.str();
    r.i32(); // m_LoadType
    let channels = r.i32();
    let freq = r.i32();
    r.skip(4 + 4); // BitsPerSample, Length
    r.skip(2); r.align(); // IsTrackerFormat, Ambisonic
    r.i32(); // SubsoundIndex
    r.skip(3); r.align(); // Preload, LoadInBackground, Legacy3D
    let source = r.str();
    let offset = r.u64();
    let size = r.u64();
    let compression = r.i32();
    AudioClip { name, channels, freq, source, offset, size, compression }
}

/// TextAsset: (m_Name, m_Script bytes).
pub fn text_asset(b: &[u8]) -> (String, Vec<u8>) {
    let mut r = Rd::new(b);
    (r.str(), r.bytes().to_vec())
}

/// MonoBehaviour head: m_Script path id and a cursor after m_Name (the script's own fields follow).
pub fn mono_behaviour(b: &[u8]) -> (i64, String, Rd<'_>) {
    let mut r = Rd::new(b);
    r.pptr(); // m_GameObject
    r.skip(1); r.align(); // m_Enabled
    let script = r.pptr().1;
    let name = r.str();
    (script, name, r)
}
