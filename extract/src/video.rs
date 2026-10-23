//! = re/scripts/export_video.py: cursors, menu.vid, intro.vid/.wav, building panel videos, end-game banners; plus
//! (`export_more`) the minimap's character / event / lord videos and the mission-over videos
//! (.vid format: that docstring). VP8 via libvpx (vpx.rs), BC1/BC3 by bc.rs, LZ4 blocks like lz4.block.
use crate::tex::{Rgba, Textures};
use crate::{bc, vpx, webm, Result};
use std::{fs, path::Path};

const W: usize = 1920;
const H: usize = 1080;
const FPS: u32 = 30;

/// Decoded frames of a .webm (RGBA, alpha from the VP8 alpha stream when present).
pub struct Frames { pub w: usize, pub h: usize, frames: std::vec::IntoIter<(Vec<u8>, Option<Vec<u8>>)>, d: vpx::Decoder, da: Option<vpx::Decoder> }

pub fn frames(path: &Path) -> Result<(Frames, Option<(webm::Track, Vec<Vec<u8>>)>)> {
    let m = webm::parse(&fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)?;
    if m.video.codec != "V_VP8" { return Err(format!("{}: codec {}", path.display(), m.video.codec).into()); }
    let alpha = m.frames.iter().any(|f| f.1.is_some());
    let audio = m.audio.map(|a| (a, m.audio_packets));
    Ok((Frames { w: m.video.w as usize, h: m.video.h as usize, frames: m.frames.into_iter(), d: vpx::Decoder::vp8()?, da: if alpha { Some(vpx::Decoder::vp8()?) } else { None } }, audio))
}

impl Frames {
    /// The last frame, converting only that one (VP8 still decodes every frame before it).
    pub fn last_rgba(mut self) -> Result<Rgba> {
        let (mut last, mut last_a) = (None, None);
        for (f, a) in self.frames.by_ref() {
            last = Some(self.d.decode(&f)?);
            if let (Some(d), Some(a)) = (&mut self.da, a) { last_a = Some(d.decode(&a)?); }
        }
        let y = last.ok_or("no frames")?;
        Ok(Rgba { w: y.w, h: y.h, px: y.rgba(last_a.as_ref()) })
    }
}

impl Iterator for Frames {
    type Item = Result<Rgba>;
    fn next(&mut self) -> Option<Result<Rgba>> {
        let (f, a) = self.frames.next()?;
        Some((|| {
            let y = self.d.decode(&f)?;
            let alpha = match (&mut self.da, a) { (Some(d), Some(a)) => Some(d.decode(&a)?), _ => None };
            Ok(Rgba { w: y.w, h: y.h, px: y.rgba(alpha.as_ref()) })
        })())
    }
}

/// Interleaved s16 of a webm's Vorbis track, (channels, rate, pcm).
pub fn vorbis_track(t: &webm::Track, packets: &[Vec<u8>]) -> Result<(u16, u32, Vec<u8>)> {
    use lewton::{audio::{read_audio_packet, PreviousWindowRight}, header::{read_header_ident, read_header_setup}};
    if t.codec != "A_VORBIS" { return Err(format!("audio codec {}", t.codec).into()); }
    // CodecPrivate: Xiph lacing of the three header packets
    let p = &t.private;
    let n = *p.first().ok_or("vorbis: no headers")? as usize + 1;
    let (mut q, mut sizes) = (1, vec![]);
    for _ in 0..n - 1 {
        let mut s = 0;
        loop { let c = *p.get(q).ok_or("vorbis: headers")?; q += 1; s += c as usize; if c != 255 { break; } }
        sizes.push(s);
    }
    let id = p.get(q..q + sizes[0]).ok_or("vorbis: headers")?;
    let setup_at = q + sizes[0] + sizes.get(1).copied().unwrap_or(0);
    let ident = read_header_ident(id).map_err(|e| format!("vorbis id: {e:?}"))?;
    let setup = read_header_setup(&p[setup_at..], ident.audio_channels, (ident.blocksize_0, ident.blocksize_1)).map_err(|e| format!("vorbis setup: {e:?}"))?;
    let (mut pwr, mut pcm) = (PreviousWindowRight::new(), vec![]);
    for pk in packets {
        let ch: Vec<Vec<i16>> = read_audio_packet(&ident, &setup, pk, &mut pwr).map_err(|e| format!("vorbis audio: {e:?}"))?;
        for i in 0..ch.first().map_or(0, Vec::len) { for c in &ch { pcm.extend_from_slice(&c[i].to_le_bytes()); } }
    }
    Ok((ident.audio_channels as u16, ident.audio_sample_rate, pcm))
}

fn write_vid(out: &Path, name: &str, frames: impl Iterator<Item = Result<Rgba>>, keys: &[usize], start: u32, (w, h): (usize, usize), fps: u32, bc3: bool) -> Result<()> {
    let (mut lens, mut blobs, mut prev): (Vec<u32>, Vec<Vec<u8>>, Option<Vec<u8>>) = (vec![], vec![], None);
    for (i, f) in frames.enumerate() {
        let f = f?;
        if (f.w, f.h) != (w, h) { return Err(format!("{name}: frame {}x{}, want {w}x{h}", f.w, f.h).into()); }
        let blocks = bc::encode(&f.px, w, h, bc3);
        let key = keys.contains(&i) || prev.is_none();
        let data = match (&prev, key) { (Some(p), false) => blocks.iter().zip(p).map(|(a, b)| a ^ b).collect(), _ => blocks.clone() };
        let blob = lz4_flex::block::compress(&data);
        lens.push(blob.len() as u32 | if key { 1 << 31 } else { 0 });
        blobs.push(blob);
        prev = Some(blocks);
    }
    let mut f = Vec::with_capacity(24 + lens.len() * 4 + blobs.iter().map(Vec::len).sum::<usize>());
    f.extend_from_slice(if bc3 { b"SHC3" } else { b"SHCV" });
    for v in [w as u32, h as u32, fps, lens.len() as u32, start] { f.extend_from_slice(&v.to_le_bytes()); }
    for l in &lens { f.extend_from_slice(&l.to_le_bytes()); }
    for b in &blobs { f.extend_from_slice(b); }
    let tmp = out.join(format!("{name}.part"));
    fs::write(&tmp, f)?;
    fs::rename(tmp, out.join(name))?; // a killed run never leaves a half file that "exists"
    Ok(())
}

/// Scale every frame to w x ih (Lanczos), padded with transparent rows to w x h.
fn fit(frames: Frames, w: usize, h: usize, ih: usize) -> impl Iterator<Item = Result<Rgba>> {
    frames.map(move |f| f.map(|f| {
        let s = f.resize(w, ih);
        if ih == h { return s; }
        let mut o = Rgba::new(w, h);
        o.paste(&s, 0, 0);
        o
    }))
}

/// The "Presents" TextBlock (IntroSequence.xaml): 80 px Junicode, white, centred, SkewX -25 then X + 20.
fn presents(font: &fontdue::Font) -> Rgba {
    let text = "Presents"; // TEXT_NEW_TEXT2_139 (English)
    let lm = font.horizontal_line_metrics(80.0).unwrap();
    let (asc, desc) = (lm.ascent.round(), (-lm.descent).round());
    let tw: f32 = text.chars().map(|c| font.metrics(c, 80.0).advance_width).sum();
    let (x0, y0) = ((W as f32 - tw) / 2.0, (H as f32 - (asc + desc)) / 2.0);
    let mut cov = vec![0f32; W * H];
    let mut pen = x0;
    for c in text.chars() {
        let (m, bmp) = font.rasterize(c, 80.0);
        let (gx, gy) = ((pen + m.xmin as f32).round() as i64, (y0 + asc - m.ymin as f32 - m.height as f32).round() as i64);
        for y in 0..m.height { for x in 0..m.width {
            let (px, py) = (gx + x as i64, gy + y as i64);
            if (0..W as i64).contains(&px) && (0..H as i64).contains(&py) {
                let d = &mut cov[py as usize * W + px as usize];
                *d = (*d + bmp[y * m.width + x] as f32 / 255.0).min(1.0);
            }
        } }
        pen += m.advance_width;
    }
    let k = 25f32.to_radians().tan();
    let mut o = Rgba::new(W, H);
    for y in 0..H { for x in 0..W {
        let sx = x as f32 - 20.0 + k * (y as f32 - y0); // bilinear along x (the skew keeps rows)
        let (i, f) = (sx.floor() as i64, sx - sx.floor());
        let at = |i: i64| if (0..W as i64).contains(&i) { cov[y * W + i as usize] } else { 0.0 };
        let a = at(i) * (1.0 - f) + at(i + 1) * f;
        o.px[(y * W + x) * 4..][..4].copy_from_slice(&[255, 255, 255, (a * 255.0 + 0.5) as u8]);
    } }
    o
}

/// FadeInLogos: linear key frames (s, opacity) per layer, over black, 5.5 s at 30 fps.
fn logos(tex: &Textures, font: &fontdue::Font) -> Result<Vec<Rgba>> {
    type Keys = &'static [(f32, f32)];
    let mut layers: Vec<(Rgba, Keys)> = vec![];
    for (n, keys) in [("loading background", &[(0.0, 1.0), (1.0, 0.0)][..]), ("logo1", &[(0.0, 0.0), (1.5, 1.0), (2.0, 1.0), (3.5, 0.0)]),
                      ("Partner_Logos", &[(0.0, 0.0), (1.5, 0.0), (2.0, 1.0), (3.5, 0.0)])] {
        layers.push((tex.get(n)?.resize(W, H), keys));
    }
    layers.push((presents(font), &[(0.0, 0.0), (3.2, 0.0), (4.2, 1.0), (5.0, 1.0), (5.5, 0.0)]));
    let at = |keys: Keys, t: f32| {
        for w in keys.windows(2) {
            let ((t0, a0), (t1, a1)) = (w[0], w[1]);
            if t <= t1 { return if t1 > t0 { a0 + (a1 - a0) * (t - t0) / (t1 - t0) } else { a1 }; }
        }
        keys.last().unwrap().1
    };
    let n = (5.5 * FPS as f32) as usize;
    Ok((0..n).map(|i| {
        let t = i as f32 / FPS as f32;
        let ops: Vec<f32> = layers.iter().map(|(_, k)| at(k, t)).collect();
        let mut o = Rgba::new(W, H);
        for (p, d) in o.px.chunks_mut(4).enumerate() {
            let mut f = [0f32; 3];
            for ((l, _), op) in layers.iter().zip(&ops) {
                let s = &l.px[p * 4..p * 4 + 4];
                let a = s[3] as f32 / 255.0 * op;
                for c in 0..3 { f[c] = f[c] * (1.0 - a) + s[c] as f32 / 255.0 * a; }
            }
            d.copy_from_slice(&[(f[0] * 255.0 + 0.5) as u8, (f[1] * 255.0 + 0.5) as u8, (f[2] * 255.0 + 0.5) as u8, 255]);
        }
        o
    }).collect())
}

fn write_wav(out: &Path, name: &str, (ch, rate, pcm): (u16, u32, Vec<u8>), vol: f32) -> Result<()> {
    let pcm: Vec<u8> = if vol == 1.0 { pcm } else {
        pcm.chunks(2).flat_map(|s| ((i16::from_le_bytes([s[0], s[1]]) as f32 * vol).round().clamp(-32768.0, 32767.0) as i16).to_le_bytes()).collect()
    };
    let tmp = out.join(format!("{name}.part"));
    fs::write(&tmp, crate::audio::wav(ch, rate, &pcm))?;
    Ok(fs::rename(tmp, out.join(name))?) // the viewer may be running (export_more): never a half file
}

/// SFXManager.getBinkVolume (playBink passes it, times SFX x master): baked into the videos' .wav; 1 for the rest.
const VOL: &[(&str, f32)] = &[("wf_vict1", 0.27559054), ("wf_vict2", 0.35433072), ("wf_taunt1", 0.23622048), ("wf_taunt2", 0.27559054),
    ("wf_plead1", 0.27559054), ("wf_anger1", 0.23622048), ("rt_taunt1", 0.31496063), ("rt_vict1", 0.31496063), ("rt_plead3", 0.5905512),
    ("rt_plead2", 0.31496063), ("rt_anger1", 0.31496063), ("rt_vict2", 0.31496063), ("rt_plead1", 0.31496063), ("rt_taunt2", 0.31496063),
    ("st17_bakers_workshop", 0.62992126), ("st16_tanners_workshop", 0.511811), ("st15_armourers_workshop", 0.39370078),
    ("st14_poleturners_workshop", 0.43307087), ("st12_fletchers_workshop", 1.0), ("st07_hunters_hut", 0.62992126),
    ("st03_woodcutters_hut", 0.62992126), ("action_steal_bread", 0.62992126), ("action_rabbits", 0.62992126), ("action_plague", 0.62992126),
    ("action_mad_cows", 0.62992126), ("action_fire", 0.62992126), ("st13_blacksmiths_workshop", 0.43307087), ("action_apples_die", 0.62992126)];

/// A video's soundtrack (`<name>.wav`) at its getBinkVolume, if it has one.
fn bink_wav(out: &Path, name: &str, audio: Option<(webm::Track, Vec<Vec<u8>>)>) -> Result<()> {
    let Some((t, p)) = audio else { return Ok(()) };
    let vol = VOL.iter().find(|v| v.0 == name).map_or(1.0, |v| v.1);
    write_wav(out, &format!("{name}.wav"), vorbis_track(&t, &p)?, vol)
}

/// Every frame at half size, 2x2 box (the 4K end videos to 1920x1080 like the `_low` files; Lanczos at 4K is far slower).
fn half(frames: Frames) -> impl Iterator<Item = Result<Rgba>> { frames.map(|f| f.map(|f| halve(&f))) }

fn halve(f: &Rgba) -> Rgba {
    let (w, h) = (f.w / 2, f.h / 2);
    let mut o = Rgba::new(w, h);
    for (i, d) in o.px.iter_mut().enumerate() {
        let (p, c) = (i / 4, i % 4);
        let at = |dx: usize, dy: usize| f.px[((2 * (p / w) + dy) * f.w + 2 * (p % w) + dx) * 4 + c] as u32;
        *d = ((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8;
    }
    o
}

#[test]
fn halve_averages_2x2() {
    // 4x2: left block 0 / 100 / 200 / 100 (avg 100), right block all 7; channel c adds c
    let v = [0u8, 100, 7, 7, 200, 100, 7, 7];
    let f = Rgba { w: 4, h: 2, px: v.iter().flat_map(|&x| [x, x + 1, x + 2, x + 3]).collect() };
    assert_eq!(halve(&f).px, [100, 101, 102, 103, 7, 8, 9, 10]);
}

/// Added after the first release, so the viewer also runs this lazily on an older export (setup::more); only what is
/// missing is written, short clips first: the engine's videos over the minimap (SFXManager characterBinks /
/// eventBinks / lordBinks for RadarME: 176x176 BC3 like the building ones, + sound) and the mission-over videos
/// (HUD_MissionOver victory_videos / defeat_videos, the trails' looped "Desert Background": 1920x1080 BC1).
pub fn export_more(data: &Path, out: &Path, progress: &mut dyn FnMut(usize, usize)) -> Result<()> {
    fs::create_dir_all(out)?;
    let todo = more_todo(data, out);
    for (k, (src, name, big)) in todo.iter().enumerate() {
        progress(k, todo.len());
        let (fr, audio) = frames(src)?;
        let file = format!("{name}.vid");
        match (*big, (fr.w, fr.h) == (2 * W, 2 * H)) {
            (true, true) => write_vid(out, &file, half(fr), &[0], 0, (W, H), FPS, false)?,
            (true, false) => write_vid(out, &file, fit(fr, W, H, H), &[0], 0, (W, H), FPS, false)?,
            _ => { write_vid(out, &file, fit(fr, 176, 176, 176), &[0], 0, (176, 176), FPS, true)?; bink_wav(out, name, audio)?; }
        }
    }
    progress(todo.len(), todo.len());
    Ok(())
}

/// A custom lord's reaction video (CustomMedia/<lord>/angry.webm, 348x348 VP8 + Vorbis) as `<out>/<name>.vid` +
/// `<name>.wav`, the minimap clip format of `export_more`. Existing files are kept.
pub fn export_custom(webm: &Path, out: &Path, name: &str) -> Result<()> {
    if out.join(format!("{name}.vid")).exists() { return Ok(()); }
    fs::create_dir_all(out)?;
    let (fr, audio) = frames(webm)?;
    write_vid(out, &format!("{name}.vid"), fit(fr, 176, 176, 176), &[0], 0, (176, 176), FPS, true)?;
    bink_wav(out, name, audio)
}

/// `export_more`'s missing files: (source webm, output stem, full screen), the minimap clips first.
pub fn more_todo(data: &Path, out: &Path) -> Vec<(std::path::PathBuf, String, bool)> {
    let vids = data.join("Assets/GUI/Video");
    let mut v = vec![];
    for (dir, big) in [("Characters", false), ("Events", false), ("Lords", false), ("Victory", true), ("Defeat", true)] {
        let mut files: Vec<_> = fs::read_dir(vids.join(dir)).into_iter().flatten().flatten().map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "webm")).collect();
        files.sort();
        for p in files {
            let name = p.file_stem().unwrap_or_default().to_string_lossy().to_lowercase().replace(' ', "_");
            if !out.join(format!("{name}.vid")).exists() { v.push((p, name, big)); }
        }
    }
    v
}

/// `font`: the HUD font (ui step's font.ttf). `progress(done, total)` per file. Existing files are kept.
pub fn export(data: &Path, font: &Path, out: &Path, progress: &mut dyn FnMut(usize, usize)) -> Result<()> {
    fs::create_dir_all(out)?;
    let vids = data.join("Assets/GUI/Video");
    let done = |n: &str| out.join(n).exists();
    let tex = Textures::open(data, &["sharedassets0.assets", "sharedassets1.assets"])?;
    // 1. cursors (Director.cs: CursorStyle 0 / 2 / 3)
    for (px, sfx, file) in [(32, "32", "cursors.rgba"), (64, "", "cursors64.rgba"), (128, "X", "cursors128.rgba")] {
        let mut cur = vec![];
        for n in ["sword", "hand", "delete", "delete_not", "jester_page_0010", "scimitar"] {
            let name = if n == "jester_page_0010" && sfx == "32" { format!("{n}-32") } else { format!("{n}{sfx}") };
            let im = tex.get(&name)?;
            if (im.w, im.h) != (px, px) { return Err(format!("cursor {name}: {}x{}", im.w, im.h).into()); }
            cur.extend(im.px);
        }
        fs::write(out.join(file), cur)?;
    }
    let mut buildings: Vec<String> = fs::read_dir(vids.join("Buildings"))?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|f| f.ends_with(".webm")).collect();
    buildings.sort();
    let more = more_todo(data, out).len();
    let total = 3 + buildings.len() + 6 + more;
    let mut step = 0;
    let mut tick = |p: &mut dyn FnMut(usize, usize)| { step += 1; p(step, total) };
    // 2. menu backdrop loop, intro (logos + film) and its sound
    if !done("menu.vid") {
        let (f, _) = frames(&vids.join("front_end_background_low.webm"))?;
        write_vid(out, "menu.vid", f, &[0], 0, (W, H), FPS, false)?;
    }
    tick(progress);
    let (film, audio) = frames(&vids.join("intro-low.webm"))?;
    if !done("intro.vid") {
        let font = fontdue::Font::from_bytes(fs::read(font)?, fontdue::FontSettings::default())?;
        let logo = logos(&tex, &font)?;
        let start = logo.len();
        write_vid(out, "intro.vid", logo.into_iter().map(Ok).chain(film), &[0, start], start as u32, (W, H), FPS, false)?;
    }
    tick(progress);
    if !done("intro.wav") {
        if let Some((t, p)) = audio { write_wav(out, "intro.wav", vorbis_track(&t, &p)?, 1.0)?; }
    }
    tick(progress);
    // 3. building panel videos (176x176 BC3; 29.97 fps stored as 30) + sound
    for f in &buildings {
        let name = f.trim_end_matches(".webm");
        let (fr, audio) = frames(&vids.join("Buildings").join(f))?;
        if !done(&format!("{name}.vid")) { write_vid(out, &format!("{name}.vid"), fit(fr, 176, 176, 176), &[0], 0, (176, 176), FPS, true)?; }
        if !done(&format!("{name}.wav")) { bink_wav(out, name, audio)?; }
        tick(progress);
    }
    // 4. end-game banners: VP8 + alpha, 528 px wide (+1 transparent row), 60 fps
    for fac in ["Crusader", "Arab", "Bedouin"] {
        for res in ["Victory", "Defeat"] {
            let name = format!("banner_{}_{}.vid", fac.to_lowercase(), res.to_lowercase());
            if !done(&name) {
                let (fr, _) = frames(&vids.join(format!("Banner-{fac}-{res}.webm")))?;
                write_vid(out, &name, fit(fr, 528, 308, 307), &[0], 0, (528, 308), 60, true)?;
            }
            tick(progress);
        }
    }
    // 5. the minimap's character / event / lord videos, the mission-over videos
    let at = total - more;
    export_more(data, out, &mut |d, _| progress(at + d, total))
}
