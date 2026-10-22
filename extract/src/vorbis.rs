//! FSB5 Vorbis (mode 15): FSB keeps only `u16 len + packet` audio packets and the CRC32 of the setup header.
//! The setup header is libvorbisenc's for that (quality, channels, rate): data/vorbis_<crc>.bin
//! (re/scripts/gen_vorbis_setup.py), the id header is rebuilt from the sample header, as python-fsb5 does.
use crate::{audio::Sample, Result};
use lewton::{audio::{read_audio_packet, PreviousWindowRight}, header::{read_header_ident, read_header_setup}};

/// (crc, u16 blocksize_short, u16 blocksize_long, setup packet)
const SETUPS: &[(u32, &[u8])] = &[(0x6d39bf3e, include_bytes!("../data/vorbis_6d39bf3e.bin"))];

/// Interleaved PCM16 bytes, `s.frames` long.
pub fn decode(s: &Sample) -> Result<Vec<u8>> {
    let blob = SETUPS.iter().find(|(c, _)| *c == s.crc).ok_or_else(|| format!("vorbis setup header {:08x} unknown", s.crc))?.1;
    let exp = |o: usize| u16::from_le_bytes([blob[o], blob[o + 1]]).trailing_zeros() as u8;
    let mut id = vec![1];
    id.extend_from_slice(b"vorbis");
    id.extend_from_slice(&0u32.to_le_bytes());
    id.push(s.channels as u8);
    id.extend_from_slice(&s.rate.to_le_bytes());
    id.extend_from_slice(&[0; 12]);
    id.push(exp(0) | exp(2) << 4);
    id.push(1);
    let ident = read_header_ident(&id).map_err(|e| format!("vorbis id: {e:?}"))?;
    let setup = read_header_setup(&blob[4..], ident.audio_channels, (ident.blocksize_0, ident.blocksize_1)).map_err(|e| format!("vorbis setup: {e:?}"))?;
    let (mut pwr, mut pcm, mut p) = (PreviousWindowRight::new(), Vec::new(), 0);
    while let Some(len) = s.data.get(p..p + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize) {
        if len == 0 { break; }
        let packet = s.data.get(p + 2..p + 2 + len).ok_or("vorbis: short packet")?;
        p += 2 + len;
        let ch: Vec<Vec<i16>> = read_audio_packet(&ident, &setup, packet, &mut pwr).map_err(|e| format!("vorbis audio: {e:?}"))?;
        for i in 0..ch.first().map_or(0, Vec::len) {
            for c in &ch { pcm.extend_from_slice(&c[i].to_le_bytes()); }
        }
    }
    pcm.truncate(s.frames as usize * s.channels as usize * 2);
    Ok(pcm)
}
