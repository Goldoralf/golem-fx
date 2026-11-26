// SPDX-License-Identifier: GPL-3.0-or-later
//! Bit-identity of the four circuits coded in v0.1.0 (README §5 "Old presets, old circuits", roadmap
//! 0.8 item 3): every factory preset of v0.1.0 and the 16 reference cases, rendered at 44.1 – 192 kHz,
//! hashed (FNV-1a over the f32 / f64 bits). The hashes were recorded **before** the Discrete OD, the
//! Studio OD, the Plexi Dist and gfx-dsp's JFET / pair devices were added; they must never move.
//! `GOLDEN_PRINT=1 cargo test -p gfx-dist --no-default-features --test golden -- --nocapture` prints
//! them.

use gfx_dist::circuits::{IN_SIG, IN_VB, IN_VS};
use gfx_dist::{build, Dist, CIRCUITS, DIODES, OPAMPS};
use gfx_dsp::circuit_nl::NonlinearCircuit;
use gfx_dsp::Effect;

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

/// The v0.1.0 factory presets (presets/gfx-dist.json), every parameter as stored.
#[allow(clippy::type_complexity)]
const PRESETS: [(&str, &str, f32, f32, f32, &str, &str, f32, bool, f32, f32, f32); 16] = [
    ("slew-wall", "Op-Amp Dist", 70.0, 60.0, -2.0, "Circuit", "Circuit", 100.0, false, 20.0, 0.0, 0.0),
    ("filter-lead", "Op-Amp Dist", 85.0, 45.0, 0.0, "Circuit", "Circuit", 100.0, false, 20.0, 0.0, 0.0),
    ("opamp-crunch", "Op-Amp Dist", 30.0, 70.0, 0.0, "Circuit", "Circuit", 100.0, false, 20.0, 0.0, 0.0),
    ("turbo-lights", "Op-Amp Dist", 75.0, 50.0, -6.0, "LED", "Circuit", 100.0, false, 20.0, 0.0, 0.0),
    ("opamp-only", "Op-Amp Dist", 90.0, 50.0, -9.0, "None", "Circuit", 100.0, false, 20.0, 0.0, 0.0),
    ("dying-battery", "Op-Amp Dist", 80.0, 50.0, 0.0, "Circuit", "Circuit", 70.0, false, 20.0, 0.0, 0.0),
    ("opamp-bass", "Op-Amp Dist", 60.0, 55.0, 0.0, "Circuit", "Circuit", 100.0, false, 20.0, 0.0, 120.0),
    ("orange-crunch-stack", "Booster Dist", 55.0, 60.0, 0.0, "Circuit", "Circuit", 100.0, false, 120.0, 0.0, 0.0),
    ("booster-lead", "Booster Dist", 85.0, 45.0, 0.0, "Circuit", "Circuit", 100.0, false, 20.0, 0.0, 0.0),
    ("transparent-push", "Blend OD", 10.0, 50.0, 0.0, "Circuit", "Circuit", 100.0, false, 20.0, 0.0, 0.0),
    ("gritty-core", "Blend OD", 50.0, 55.0, 0.0, "Circuit", "Circuit", 100.0, false, 20.0, 0.0, 0.0),
    ("blend-stack-front", "Blend OD", 15.0, 50.0, 0.0, "Circuit", "Circuit", 100.0, false, 20.0, 0.0, 0.0),
    ("amp-in-a-box", "MOSFET OD", 55.0, 50.0, 0.0, "Circuit", "Circuit", 100.0, false, 20.0, 0.0, 0.0),
    ("asymmetric-amp-box", "MOSFET OD", 55.0, 50.0, 0.0, "MOSFET + Ge", "Circuit", 100.0, false, 20.0, 0.0, 0.0),
    ("hp-roar", "MOSFET OD", 70.0, 50.0, -3.0, "Circuit", "Circuit", 100.0, true, 20.0, 0.0, 0.0),
    ("germanium-opamp", "Op-Amp Dist", 65.0, 50.0, -3.0, "Germanium", "Circuit", 100.0, false, 20.0, 0.0, 0.0),
];

fn index(list: &[&str], name: &str) -> f32 {
    list.iter().position(|x| *x == name).unwrap() as f32
}

fn render_preset(k: usize, sr: f32, channels: usize) -> u64 {
    let p = PRESETS[k];
    let mut e = Dist::new();
    let set: [(&str, f32); 11] = [
        ("circuit", index(&CIRCUITS, p.1)),
        ("gain", p.2),
        ("tone_stack", p.3),
        ("level", p.4),
        ("diodes", index(&DIODES, p.5)),
        ("opamp", index(&OPAMPS, p.6)),
        ("supply", p.7),
        ("hp", if p.8 { 1.0 } else { 0.0 }),
        ("tight", p.9),
        ("input", p.10),
        ("bass_keep", p.11),
    ];
    for (id, v) in set {
        e.set_param(id, v);
    }
    e.prepare(sr, 512, channels);
    let x = signal(sr as f64, 0.2);
    let mut h = Fnv::new();
    if channels == 1 {
        let mut y = x.clone();
        for c in y.chunks_mut(256) {
            e.process(&mut [c]);
        }
        y.iter().for_each(|v| h.eat(v.to_bits() as u64));
    } else {
        // L == R for the first half (linked), then R differs (the unlink)
        let mut l = x.clone();
        let mut r: Vec<f32> = x.iter().enumerate().map(|(i, v)| if i < x.len() / 2 { *v } else { 0.9 * v }).collect();
        for (a, b) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            e.process(&mut [a, b]);
        }
        l.iter().chain(&r).for_each(|v| h.eat(v.to_bits() as u64));
    }
    h.0
}

/// Knob moves and every discrete switch on the way (Woodbury, crossfades, part swaps).
fn render_moves(circuit: usize, sr: f32) -> u64 {
    let mut e = Dist::new();
    e.set_param("circuit", circuit as f32);
    e.prepare(sr, 512, 1);
    let x = signal(sr as f64, 0.3);
    let mut y = x.clone();
    let n = y.len();
    let mut h = Fnv::new();
    for (b, c) in y.chunks_mut(128).enumerate() {
        let t = (b * 128) as f32 / n as f32;
        e.set_param("gain", 20.0 + 70.0 * t);
        e.set_param("tone_stack", 80.0 - 60.0 * t);
        e.set_param("supply", 80.0 + 60.0 * t);
        if b == 40 {
            e.set_param("diodes", 3.0);
        }
        if b == 80 {
            e.set_param("opamp", 4.0);
        }
        if b == 120 {
            e.set_param("hp", 1.0);
            e.set_param("tight", 150.0);
        }
        if b == 160 {
            e.set_param("bass_keep", 100.0);
        }
        e.process(&mut [c]);
    }
    y.iter().for_each(|v| h.eat(v.to_bits() as u64));
    h.0
}

/// The 16 reference cases (fixtures/reference.txt), solved by `circuit_nl` directly: the cut graph
/// at the circuit's rate and tolerance 10⁻⁶, and the monolithic solve.
fn render_case(k: usize, gain: f64, freq: f64, amp: f64, mono: bool) -> u64 {
    let def = build(k);
    let fs = 48000.0 * (1 << def.stages) as f64;
    let mut nc = if mono { NonlinearCircuit::monolithic(&def.net, fs) } else { NonlinearCircuit::new(&def.net, fs) };
    for &(pt, rev) in &def.gain {
        nc.set_pot(pt, if rev { 1.0 - gain } else { gain });
    }
    for &(pt, _) in &def.tone {
        nc.set_pot(pt, 0.5);
    }
    if let Some((v, _)) = def.hp {
        nc.set_var(v, 1e9);
    }
    nc.set_input(IN_VS, def.supply);
    nc.set_input(IN_VB, 0.5 * def.supply);
    nc.set_supply(def.supply);
    nc.init_dc();
    let mut h = Fnv::new();
    for i in 0..(0.03 * fs) as usize {
        nc.set_input(IN_SIG, amp * (2.0 * std::f64::consts::PI * freq * i as f64 / fs).sin());
        nc.process();
        h.eat(nc.output(def.out).to_bits());
    }
    h.eat(nc.stats().iterations);
    h.0
}

const CASES: [(f64, f64, f64); 4] = [(0.5, 1000.0, 0.25), (0.5, 100.0, 0.25), (1.0, 1000.0, 0.063), (0.2, 1000.0, 1.0)];

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
    for k in 0..PRESETS.len() {
        for &sr in &RATES {
            got.push(render_preset(k, sr, 1));
        }
        got.push(render_preset(k, 48000.0, 2));
    }
    check("PRESET_HASHES", &got, &PRESET_HASHES);
}

#[test]
fn moves_and_switches_are_bit_identical() {
    let mut got = Vec::new();
    for c in 0..4 {
        for &sr in &[48000.0, 192000.0] {
            got.push(render_moves(c, sr));
        }
    }
    check("MOVE_HASHES", &got, &MOVE_HASHES);
}

#[test]
fn reference_cases_are_bit_identical() {
    let mut got = Vec::new();
    for k in 0..4 {
        for &(g, f, a) in &CASES {
            got.push(render_case(k, g, f, a, false));
            got.push(render_case(k, g, f, a, true));
        }
    }
    check("CASE_HASHES", &got, &CASE_HASHES);
}

const PRESET_HASHES: [u64; 80] = [
    0x9cb5b518c0f1437d, 0xae196dcf351b9c00, 0x1b295fcff1bc59c9, 0x845d6695a8db895b, 0x31b18164cf0ca2cc, 0x87f89ad687c9a772,
    0x0b090d9456d47171, 0xc3077e11646f296f, 0x58ae64667820927d, 0x5250cee50b38b674, 0x402766bf8296e4ff, 0x6a0b1e24c7a9b8a3,
    0x22fa9833d754cb0d, 0x872dbcaf63f87021, 0xa1aa915ac161022b, 0xfda32f46a20d0113, 0x5f461ebbfa0da0a4, 0xbef7df4889c0e589,
    0x5327529550d0e882, 0xc3d7f74b9aa249ae, 0xec0d785856ab016b, 0xd9b56db797ad7fcb, 0x6ecf0d0dd855c019, 0x3680655bc4664f29,
    0xcc5aacf5a08903ba, 0xa3b89018abb32815, 0x41818ad326f3dc43, 0xa530d98f6c3ba1df, 0x9e4fba51f1f69ae4, 0xb9d321e7fe1f0072,
    0xad1cb7934b266682, 0x7acb67e2d733c062, 0xafaa6d98c9e82a52, 0x85c101df2108ce79, 0x0a2210548535b77d, 0xc5ac6e1fe7bcfbea,
    0x96a399ba18384539, 0xf9e758b96e3b21b7, 0x216073c6f1f3406c, 0xcfa0c56e3366eea5, 0x6bd770f9d71aa137, 0xed0ef5bc7c8e669b,
    0x35d9deb25d144b37, 0x95e76879838fa863, 0x946400f5d2204253, 0x253b145cc764b9eb, 0x8f356edcf8c18880, 0x57fa69a079600b0c,
    0x554399095332edc4, 0x69f2e184c40b95f2, 0xfc400dad31c1068e, 0x53d4352ae19a7d2c, 0x03b5e3d8a93f8080, 0x49c70e260229c508,
    0x67f833c5d6d6b621, 0x589b00029c4537cf, 0xdb38a268cad52415, 0x4dfbbf026062dc46, 0xcaafd72bb2e16cf7, 0xa3ce2df9f22e1e8a,
    0xebce00a7294d16d7, 0x6d47ee9b37db31ff, 0xc8468a0a6774d761, 0x31a6d2adb2d64d19, 0x40ef79518fac311c, 0x813532a4e27dac4e,
    0xc130754aeb06e60a, 0xe5d2d26c7a9da340, 0x850e35611496e5e8, 0x6f3d67d001ba9478, 0xcfa8b2f339f8c7ac, 0x5c43b258d8d3c910,
    0x6fe887c97edf7097, 0x11c2b13a75a4e421, 0x90d5006c4e9c6b79, 0xe5fe676d27f9b924, 0xe34c08b88889985c, 0xe7beffbf3e225ea2,
    0xdc97a346d3ac3aed, 0x202e400ca45d5603,
];
const MOVE_HASHES: [u64; 8] = [
    0x82dc7ab502bb281a, 0xb576390bb3976592, 0xb8da70dc04e1fe4e, 0xcc55ef781764545e, 0xc9e21212bba7318b, 0x9f617d5517a8ad07,
    0x6e899c95f5665771, 0xc550aea1aaca8f5a,
];
const CASE_HASHES: [u64; 32] = [
    0xf41a61eb1cf9d4c2, 0x66b2be91e5b299c6, 0xe03519432fd64f96, 0x30748b51e18c7ca3, 0xb252ed78411ce963, 0xc067e95980d62298,
    0xd6030ddf008f5380, 0xd6aa0bc528265451, 0xd6e0738e64158896, 0xcec7b87b0f5214b3, 0x03cd95319dc2894e, 0xed4bced77aa54da0,
    0x3bd16dbf089ec432, 0xc2425a52313de79b, 0x39214be1f10e3191, 0xfbddfdb940d61b52, 0x0effbbaf3d6b5624, 0x1464c79404c526a3,
    0x748da22342079c13, 0x8634f3014b2931fd, 0xfb355a180ccd11f4, 0xc5eb98e0429111fe, 0x0956db83322a1838, 0x205fa615c6406853,
    0x155749b988124d9b, 0xcf4550977a1fc911, 0xadb2acc8cd233348, 0xe109be7552b6db19, 0x0d43dd065ca613f6, 0xc48e72f56f27abf5,
    0x045226bae0b857ef, 0x70ee4a654ec5f304,
];
