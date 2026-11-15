// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic test signals.

use gfx_dsp::lfo::Rng;

pub fn silence(n: usize) -> Vec<f32> {
    vec![0.0; n]
}

pub fn impulse(n: usize, at: usize) -> Vec<f32> {
    let mut v = vec![0.0; n];
    if at < n {
        v[at] = 1.0;
    }
    v
}

pub fn sine(n: usize, freq: f32, amp: f32, sample_rate: f32) -> Vec<f32> {
    let w = 2.0 * std::f64::consts::PI * freq as f64 / sample_rate as f64;
    (0..n).map(|i| (amp as f64 * (w * i as f64).sin()) as f32).collect()
}

/// White noise, uniform, peak `amp`, fixed seed.
pub fn white(n: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut r = Rng::new(seed);
    (0..n).map(|_| amp * r.bipolar()).collect()
}

/// Pink noise (Paul Kellet's filter on seeded white), normalised to an RMS of `rms`.
pub fn pink(n: usize, rms: f32, seed: u64) -> Vec<f32> {
    let mut r = Rng::new(seed);
    let (mut b0, mut b1, mut b2, mut b3, mut b4, mut b5, mut b6) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
    let mut v: Vec<f64> = (0..n)
        .map(|_| {
            let w = r.bipolar() as f64;
            b0 = 0.99886 * b0 + w * 0.0555179;
            b1 = 0.99332 * b1 + w * 0.0750759;
            b2 = 0.96900 * b2 + w * 0.1538520;
            b3 = 0.86650 * b3 + w * 0.3104856;
            b4 = 0.55000 * b4 + w * 0.5329522;
            b5 = -0.7616 * b5 - w * 0.0168980;
            let y = b0 + b1 + b2 + b3 + b4 + b5 + b6 + w * 0.5362;
            b6 = w * 0.115926;
            y
        })
        .collect();
    let e = (v.iter().map(|x| x * x).sum::<f64>() / n.max(1) as f64).sqrt().max(1e-12);
    v.iter_mut().for_each(|x| *x *= rms as f64 / e);
    v.into_iter().map(|x| x as f32).collect()
}

/// A burst of noise followed by silence.
pub fn burst_then_silence(burst: usize, silence: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut v = white(burst, amp, seed);
    v.extend(std::iter::repeat_n(0.0, silence));
    v
}

/// The key the common checks give a keyed effect (gfx-ducker): a 200 Hz sine at −12 dBFS in 120 ms
/// bursts every 700 ms, with 5 ms raised-cosine edges. At gfx-ducker's defaults (threshold −40 dB,
/// attack 10, hold 150, release 400 ms) each burst ducks fully and is released just before the next.
pub fn key_bursts(n: usize, sample_rate: f32) -> Vec<f32> {
    let sr = sample_rate as f64;
    let (period, on, edge) = ((0.7 * sr) as usize, 0.12 * sr, 0.005 * sr);
    let w = 2.0 * std::f64::consts::PI * 200.0 / sr;
    (0..n)
        .map(|i| {
            let t = (i % period) as f64;
            let env = if t >= on {
                0.0
            } else if t < edge {
                0.5 * (1.0 - (std::f64::consts::PI * t / edge).cos())
            } else if t > on - edge {
                0.5 * (1.0 - (std::f64::consts::PI * (on - t) / edge).cos())
            } else {
                1.0
            };
            (0.25 * env * (w * i as f64).sin()) as f32
        })
        .collect()
}
