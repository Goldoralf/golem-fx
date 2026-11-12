// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal WAV I/O: reads PCM 16 / 24 / 32-bit and 32-bit float, writes 32-bit float.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub struct Audio {
    pub sample_rate: u32,
    /// One vector per channel.
    pub channels: Vec<Vec<f32>>,
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

pub fn read(path: &Path) -> io::Result<Audio> {
    let data = fs::read(path)?;
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(bad("not a RIFF / WAVE file"));
    }
    let mut pos = 12;
    let (mut format, mut channels, mut rate, mut bits) = (0u16, 0u16, 0u32, 0u16);
    let mut samples: Option<&[u8]> = None;
    while pos + 8 <= data.len() {
        let id = &data[pos..pos + 4];
        let size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body = &data[pos + 8..(pos + 8 + size).min(data.len())];
        if id == b"fmt " {
            format = u16::from_le_bytes(body[0..2].try_into().unwrap());
            channels = u16::from_le_bytes(body[2..4].try_into().unwrap());
            rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
            bits = u16::from_le_bytes(body[14..16].try_into().unwrap());
            if format == 0xFFFE && body.len() >= 26 {
                format = u16::from_le_bytes(body[24..26].try_into().unwrap());
            }
        } else if id == b"data" {
            samples = Some(body);
        }
        pos += 8 + size + (size & 1);
    }
    let raw = samples.ok_or_else(|| bad("no data chunk"))?;
    if channels == 0 {
        return Err(bad("no fmt chunk"));
    }
    let nch = channels as usize;
    let bytes = (bits / 8) as usize;
    let frames = raw.len() / (bytes * nch);
    let mut out = vec![Vec::with_capacity(frames); nch];
    for f in 0..frames {
        for (c, ch) in out.iter_mut().enumerate() {
            let o = (f * nch + c) * bytes;
            let s = &raw[o..o + bytes];
            let v = match (format, bits) {
                (1, 16) => i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0,
                (1, 24) => (i32::from_le_bytes([0, s[0], s[1], s[2]]) >> 8) as f32 / 8_388_608.0,
                (1, 32) => i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f32 / 2_147_483_648.0,
                (3, 32) => f32::from_le_bytes([s[0], s[1], s[2], s[3]]),
                _ => return Err(bad(&format!("unsupported format {format} / {bits} bits"))),
            };
            ch.push(v);
        }
    }
    Ok(Audio { sample_rate: rate, channels: out })
}

pub fn write(path: &Path, audio: &Audio) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let nch = audio.channels.len();
    let frames = audio.channels.first().map_or(0, |c| c.len());
    let data_len = (frames * nch * 4) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes());
    b.extend_from_slice(&(nch as u16).to_le_bytes());
    b.extend_from_slice(&audio.sample_rate.to_le_bytes());
    b.extend_from_slice(&(audio.sample_rate * nch as u32 * 4).to_le_bytes());
    b.extend_from_slice(&((nch * 4) as u16).to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for f in 0..frames {
        for ch in &audio.channels {
            b.extend_from_slice(&ch[f].to_le_bytes());
        }
    }
    fs::File::create(path)?.write_all(&b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_round_trip() {
        let dir = std::env::temp_dir().join("gfx-bench-wav-test");
        let p = dir.join("t.wav");
        let a = Audio { sample_rate: 48000, channels: vec![vec![0.0, 0.5, -0.25], vec![1.0, -1.0, 0.125]] };
        write(&p, &a).unwrap();
        let b = read(&p).unwrap();
        assert_eq!(b.sample_rate, 48000);
        assert_eq!(b.channels, a.channels);
        let _ = fs::remove_dir_all(dir);
    }
}
