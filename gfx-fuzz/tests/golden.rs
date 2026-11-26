// SPDX-License-Identifier: GPL-3.0-or-later
//! Bit-identity of gfx-fuzz across changes to gfx-dsp's shared `circuit_nl` (gfx-dist README §5, review
//! of 2026-11-26: the JFET and JFET-pair devices are added for gfx-dist's Discrete OD; gfx-fuzz's renders
//! must not move). Every factory preset at 44.1 – 192 kHz and a stereo render, hashed (FNV-1a over the
//! f32 bits); the hashes were recorded before the devices were added.
//! `GOLDEN_PRINT=1 cargo test -p gfx-fuzz --no-default-features --test golden -- --nocapture` prints
//! them.

use gfx_dsp::Effect;
use gfx_fuzz::Fuzz;

struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn eat(&mut self, bits: u64) {
        for k in 0..8 {
            self.0 ^= (bits >> (8 * k)) & 0xff;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

/// A guitar-like test signal: plucked notes with harmonics, then a burst of noise, ≈ −6 dBFS peak.
fn signal(sr: f64, secs: f64) -> Vec<f32> {
    let n = (sr * secs) as usize;
    let notes = [82.41, 196.0, 329.63];
    let len = n / 4;
    let mut s = 12345u32;
    (0..n)
        .map(|i| {
            if i >= 3 * len {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                return 0.3 * ((s >> 8) as f32 / 8388608.0 - 1.0);
            }
            let f = notes[i / len];
            let t = (i % len) as f64 / sr;
            let mut v = 0.0;
            for k in 1..=8 {
                v += (2.0 * std::f64::consts::PI * f * k as f64 * t).sin() / k as f64 * (-t * (4.0 + k as f64)).exp();
            }
            (0.35 * v) as f32
        })
        .collect()
}

/// The factory presets (presets/gfx-fuzz.json at 0.7), enums as indices (model: Ge Pair 0, Si Four
/// 1; pickup: Single Coil 0 … Active 3; stack: Classic 0, Early 1, Triangle 2, Russian 3, Bypass 4).
const PRESETS: [&[(&str, f32)]; 18] = [
    &[("model", 0.0), ("fuzz", 100.0), ("guitar_vol", 100.0), ("pickup", 0.0)],
    &[("model", 0.0), ("fuzz", 100.0), ("guitar_vol", 100.0)],
    &[("model", 0.0), ("fuzz", 100.0), ("buffer", 1.0)],
    &[("model", 0.0), ("fuzz", 100.0), ("temp", 35.0)],
    &[("model", 0.0), ("fuzz", 90.0), ("temp", 12.0)],
    &[("model", 0.0), ("fuzz", 100.0), ("leakage", 250.0)],
    &[("model", 0.0), ("fuzz", 100.0), ("bias", -70.0)],
    &[("model", 0.0), ("fuzz", 100.0), ("voltage", 4.0), ("bias", -40.0), ("sag", 40.0)],
    &[("model", 0.0), ("fuzz", 100.0), ("voltage", 9.0), ("sag", 30.0)],
    &[("model", 0.0), ("fuzz", 100.0), ("mix", 50.0)],
    &[("model", 0.0), ("fuzz", 60.0)],
    &[("model", 1.0), ("stack", 0.0), ("fuzz", 70.0), ("tone", 0.0)],
    &[("model", 1.0), ("stack", 3.0), ("fuzz", 90.0), ("tone", -40.0), ("mids", 20.0)],
    &[("model", 1.0), ("fuzz", 65.0), ("mids", 80.0)],
    &[("model", 1.0), ("stack", 4.0), ("fuzz", 70.0)],
    &[("model", 1.0), ("fuzz", 70.0), ("voltage", 18.0)],
    &[("model", 1.0), ("stack", 2.0), ("fuzz", 80.0), ("tone", 20.0)],
    &[("model", 1.0), ("stack", 3.0), ("pickup", 3.0), ("bass_keep", 120.0), ("mix", 70.0), ("fuzz", 75.0)],
];

fn render(set: &[(&str, f32)], sr: f32, channels: usize, moves: bool) -> u64 {
    let mut e = Fuzz::new();
    for (id, v) in set {
        e.set_param(id, *v);
    }
    e.prepare(sr, 512, channels);
    let x = signal(sr as f64, 0.15);
    let mut h = Fnv::new();
    if channels == 1 {
        let mut y = x.clone();
        let n = y.len();
        for (b, c) in y.chunks_mut(128).enumerate() {
            if moves {
                let t = (b * 128) as f32 / n as f32;
                e.set_param("fuzz", 30.0 + 60.0 * t);
                e.set_param("guitar_vol", 100.0 - 50.0 * t);
                e.set_param("bias", -30.0 + 50.0 * t);
                e.set_param("voltage", 9.0 - 3.0 * t);
            }
            e.process(&mut [c]);
        }
        y.iter().for_each(|v| h.eat(v.to_bits() as u64));
    } else {
        let mut l = x.clone();
        let mut r: Vec<f32> = x.iter().enumerate().map(|(i, v)| if i < x.len() / 2 { *v } else { 0.9 * v }).collect();
        for (a, b) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            e.process(&mut [a, b]);
        }
        l.iter().chain(&r).for_each(|v| h.eat(v.to_bits() as u64));
    }
    h.0
}

const RATES: [f32; 4] = [44100.0, 48000.0, 96000.0, 192000.0];

fn check(name: &str, got: &[u64], want: &[u64]) {
    if std::env::var("GOLDEN_PRINT").is_ok() {
        eprintln!("const {name}: [u64; {}] = {got:#018x?};", got.len());
        return;
    }
    assert_eq!(got.len(), want.len(), "{name}");
    let bad: Vec<usize> = (0..got.len()).filter(|&i| got[i] != want[i]).collect();
    assert!(bad.is_empty(), "{name}: renders moved at {bad:?}");
}

#[test]
fn presets_are_bit_identical() {
    let mut got = Vec::new();
    for p in PRESETS {
        for &sr in &RATES {
            got.push(render(p, sr, 1, false));
        }
        got.push(render(p, 48000.0, 2, false));
    }
    check("PRESET_HASHES", &got, &PRESET_HASHES);
}

#[test]
fn moves_are_bit_identical() {
    let mut got = Vec::new();
    for m in [0.0, 1.0] {
        for &sr in &[48000.0, 192000.0] {
            got.push(render(&[("model", m)], sr, 1, true));
        }
    }
    check("MOVE_HASHES", &got, &MOVE_HASHES);
}

const PRESET_HASHES: [u64; 90] = [
    0xaa7a434f0c891414, 0x62fa8f79e26e5f85, 0xe35c812fea27241d, 0x366f5b24a43aabcd, 0x86c6329540af4d7a, 0xaa7a434f0c891414,
    0x62fa8f79e26e5f85, 0xe35c812fea27241d, 0x366f5b24a43aabcd, 0x86c6329540af4d7a, 0xf64ee705f961f834, 0x91b941c8a40fae26,
    0x146e762cbbffd7e7, 0x6819f4d9bec72f42, 0xb60ffaa719a17491, 0x1caedd7e16ef7e91, 0x8df72ac186758c69, 0x3794849ba4013fae,
    0xe94a42b014000a76, 0xad3f7b49b5f1e230, 0xeca12106158d8899, 0xdbf85983f5030b60, 0xc6a5c345adea3caa, 0xd5e7948f0dd9eecd,
    0x0f1f4763914de7f5, 0x4c1f9da4cf41c4cc, 0x3eeb6693daeb665c, 0xdae619af1ccf99ed, 0xe92a39342f11f1e3, 0x1470469d8409af07,
    0xbf47400d1f43c5ed, 0x4100e8da14a790e5, 0x806d6a7204842705, 0xcfd9c772f72c3e01, 0x4ecb7646e7edb500, 0x28e47ec652d38c27,
    0xc290e63842d390f8, 0x48f41f8e16cf615d, 0xc8b6681525129524, 0xe0dce792715ad9a2, 0x74d051c6f173f661, 0x5fb41e7d40089283,
    0x6fd597337131718c, 0x61a1c8c9b33b1d3a, 0xf47d70899feb1e28, 0x5c003cffff763d1c, 0xe3dc74825bb92ee9, 0xc0453b77535b86d2,
    0xf6e4ae1214acce0a, 0x8b4eff484dca395f, 0x8c4cdd8f39bcce0f, 0xd4bb3bb7e28da31f, 0x6659fa156b05ed28, 0xc95e6b4ec89222ec,
    0xb24a4e2af2bd62ba, 0x621ee73e72b5cff7, 0x34b3e37f43536e46, 0x8e450823f966586d, 0x2d3ee5695059f814, 0x817e103cb1332196,
    0x4babf7c9eb78cbcd, 0x26b1d05669b10b66, 0x483d03b7029d4ab1, 0xe928060083fdc1fa, 0x21995d6170b225dd, 0x1ce6dd569fca93c8,
    0x30e4450d585f362b, 0x2bb28ba72d42c4e0, 0x9b4cab36f16cf244, 0x82f7682a370ffffe, 0x2b052c24146ad8f5, 0xd80e45afe76ed7b0,
    0x5fdb094a110b5e20, 0xc52da116e3d5236a, 0x95a341454bc0f3d2, 0xc956b2373b64b0e7, 0xa73a13ad64975970, 0xf2d744d20c4c064b,
    0xe8456a53d400f2fc, 0x8bb3311219c9d19f, 0xa8c4de7379238fae, 0x8c4678539d7684cf, 0xa461a2ccea8a922b, 0x88cfc68c175b8db7,
    0x8afb384955ac0a58, 0x0818195af1158057, 0x94e1146d2007dcb5, 0xe05a884f8f8732ba, 0xa9a67cb198b8b3a0, 0x29949e212a99b6ff,
];
const MOVE_HASHES: [u64; 4] = [
    0x83170a083c2b879f, 0x07969716feacfd41, 0x20cd0fd3ef80936f, 0xdcb2814c0b8c4eb0,
];
