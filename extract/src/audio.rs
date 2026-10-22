//! = re/scripts/export_audio.py: every AudioClip as fx/<name>.wav (FSB5 in resources.resource; PCM16 copied,
//! Vorbis decoded by vorbis.rs), volume.txt, sounds/ambient.tsv (re/data/cs_sfx_*.tsv), speech/music links.
use crate::classes::{self, AUDIO_CLIP, TEXT_ASSET};
use crate::unity::{read_range, SFile};
use crate::Result;
use std::{fs, path::Path};

/// One FSB5 sample: (channels, rate, frames, mode, data, vorbis setup crc).
pub struct Sample<'a> { pub channels: u16, pub rate: u32, pub frames: u32, pub mode: u32, pub data: &'a [u8], pub crc: u32 }

/// First sample of an FSB5 bank (Unity writes one per clip).
pub fn fsb5(b: &[u8]) -> Result<Sample<'_>> {
    let u32_at = |o: usize| -> Result<u32> { Ok(u32::from_le_bytes(b.get(o..o + 4).ok_or("fsb5: short")?.try_into()?)) };
    if b.get(..4) != Some(b"FSB5") { return Err("not FSB5".into()); }
    let (version, headers, names, size, mode) = (u32_at(4)?, u32_at(12)? as usize, u32_at(16)? as usize, u32_at(20)? as usize, u32_at(24)?);
    let base = if version == 0 { 64 } else { 60 };
    let h = u64::from_le_bytes(b.get(base..base + 8).ok_or("fsb5: short")?.try_into()?);
    let mut rate = [4000, 8000, 11000, 11025, 16000, 22050, 24000, 32000, 44100, 48000, 96000][((h >> 1) & 15) as usize % 11];
    let mut channels = ((h >> 5) & 1) as u16 + 1;
    let off = ((h >> 6) & ((1 << 28) - 1)) as usize * 16;
    let frames = (h >> 34) as u32;
    let (mut p, mut next, mut crc) = (base + 8, h & 1 == 1, 0);
    while next {
        let c = u32_at(p)?;
        let (len, kind) = (((c >> 1) & 0xff_ffff) as usize, c >> 25);
        let body = p + 4;
        match kind {
            1 => channels = *b.get(body).ok_or("fsb5")? as u16,
            2 => rate = u32_at(body)?,
            11 => crc = u32_at(body)?,
            _ => {}
        }
        next = c & 1 == 1;
        p = body + len;
    }
    let start = base + headers + names + off;
    let data = b.get(start..(base + headers + names + size).min(b.len())).ok_or("fsb5: data")?;
    Ok(Sample { channels, rate, frames, mode, data, crc })
}

pub fn wav(channels: u16, rate: u32, pcm: &[u8]) -> Vec<u8> {
    let mut w = Vec::with_capacity(44 + pcm.len());
    let block = channels as u32 * 2;
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    for v in [16u32] { w.extend_from_slice(&v.to_le_bytes()); }
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&channels.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * block).to_le_bytes());
    w.extend_from_slice(&(block as u16).to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(pcm);
    w
}

/// PCM16 bytes of a sample.
fn decode(s: &Sample) -> Result<Vec<u8>> {
    match s.mode {
        2 => Ok(s.data.get(..s.frames as usize * s.channels as usize * 2).unwrap_or(s.data).to_vec()),
        15 => crate::vorbis::decode(s),
        m => Err(format!("fsb5 mode {m} unsupported").into()),
    }
}

/// Returns how many clips could not be decoded (logged; the game plays on without them).
pub fn export(data: &Path, out: &Path) -> Result<usize> {
    fs::create_dir_all(out.join("fx"))?;
    let f = SFile::open(&data.join("resources.assets"))?;
    let mut failed = 0;
    for r in f.each(AUDIO_CLIP) {
        let (_, b) = r?;
        let c = classes::audio_clip(&b);
        let res = data.join(c.source.rsplit('/').next().unwrap_or(&c.source));
        let bank = read_range(&res, c.offset, c.size as usize)?;
        match fsb5(&bank).and_then(|s| Ok((s.channels, s.rate, decode(&s)?))) {
            Ok((ch, rate, pcm)) => fs::write(out.join("fx").join(format!("{}.wav", c.name)), wav(ch, rate, &pcm))?,
            Err(e) => { eprintln!("audio: {}: {e}", c.name); failed += 1; }
        }
    }
    for r in f.each(TEXT_ASSET) {
        let (_, b) = r?;
        let (name, script) = classes::text_asset(&b);
        if name == "volume" { fs::write(out.join("volume.txt"), script)?; }
    }
    fs::write(out.join("sounds.tsv"), include_str!("../../re/data/cs_sfx_main.tsv"))?;
    fs::write(out.join("ambient.tsv"), include_str!("../../re/data/cs_sfx_ambient.tsv"))?;
    for (link, src) in [("speech", "EnglishSpeech"), ("music", "Music")] {
        let p = out.join(link);
        let _ = fs::remove_file(&p);
        #[cfg(unix)]
        std::os::unix::fs::symlink(data.join("StreamingAssets").join(src), &p)?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(data.join("StreamingAssets").join(src), &p)?;
    }
    Ok(failed)
}
