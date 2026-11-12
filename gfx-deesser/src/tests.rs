// SPDX-License-Identifier: GPL-3.0-or-later
//! The de-esser's own bench suite (README.md §6), on synthetic voice-like signals.

use super::*;
use gfx_dsp::lfo::Rng;

const SR: f32 = 48000.0;

fn deesser(f: impl FnOnce(&mut Settings)) -> Deesser {
    let mut d = Deesser::new();
    d.prepare(SR, 4800, 1);
    let mut s = Settings { relative: true, ..Settings::default() };
    f(&mut s);
    d.set(s);
    d.reset();
    d
}

/// A vowel: harmonics of 180 Hz with formants around 700 Hz and 2.5 kHz.
fn vowel(n: usize, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let mut v = 0.0;
            for h in 1..25 {
                let f = 180.0 * h as f32;
                let a = (-((f - 700.0) / 300.0).powi(2)).exp() + 0.6 * (-((f - 2500.0) / 500.0).powi(2)).exp() + 0.05;
                v += a * (2.0 * std::f32::consts::PI * f * t).sin();
            }
            amp * 0.2 * v
        })
        .collect()
}

/// An "s": white noise high-passed at 5 kHz.
fn sibilant(n: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut r = Rng::new(seed);
    let mut hp = Svf::new();
    hp.set(SvfMode::HighPass, 5000.0, 0.707, 0.0, SR);
    (0..n).map(|_| amp * hp.process(r.bipolar())).collect()
}

fn reduction(d: &mut Deesser, x: &[f32]) -> f32 {
    let mut y = x.to_vec();
    let mut m = 0.0f32;
    for c in y.chunks_mut(240) {
        d.process(&mut [c]);
        m = m.max(d.gain_reduction_db());
    }
    m
}

#[test]
fn transparent_without_sibilance() {
    let mut d = deesser(|_| {});
    // faded in over 20 ms: an abrupt start is a broadband click, which a de-esser rightly answers
    let x: Vec<f32> = vowel(48000, 0.5).iter().enumerate().map(|(i, v)| v * (i as f32 / 960.0).min(1.0)).collect();
    let mut y = x.clone();
    for c in y.chunks_mut(256) {
        d.process(&mut [c]);
    }
    assert_eq!(y, x);
}

#[test]
fn level_independent_reduction() {
    let red = |level: f32| {
        let mut d = deesser(|_| {});
        let mut x = vowel(24000, level);
        x.extend(sibilant(9600, 0.5 * level, 3));
        x.extend(vowel(24000, level));
        reduction(&mut d, &x)
    };
    let (a, b, c) = (red(1.0), red(0.3), red(0.1));
    assert!(a > 4.0, "no reduction on the s ({a})");
    assert!((a - b).abs() < 1.0 && (a - c).abs() < 1.0, "−10 / −20 dB: {a:.2} {b:.2} {c:.2}");
}

#[test]
fn air_above_the_band_does_not_trigger() {
    let mut d = deesser(|s| s.freq = 6000.0);
    let mut r = Rng::new(8);
    // air 20 dB under the voice, above 18 kHz (8th-order edge)
    let mut hps: Vec<Svf> = (0..4)
        .map(|_| {
            let mut f = Svf::new();
            f.set(SvfMode::HighPass, 18000.0, 0.707, 0.0, SR);
            f
        })
        .collect();
    let v: Vec<f32> = vowel(48000, 0.5).iter().enumerate().map(|(i, v)| v * (i as f32 / 960.0).min(1.0)).collect();
    let x: Vec<f32> = v
        .iter()
        .map(|s| {
            let mut a = r.bipolar();
            for f in hps.iter_mut() {
                a = f.process(a);
            }
            s + 0.05 * a
        })
        .collect();
    let m = reduction(&mut d, &x);
    assert!(m < 1.0, "air triggered {m:.2} dB");
}

#[test]
fn bright_vowel_is_left_alone_in_vocal_focus() {
    let mut d = deesser(|s| s.thresh = -20.0);
    let m = reduction(&mut d, &vowel(48000, 1.0));
    assert!(m <= 1.0, "vowel reduced {m:.2} dB");
}

#[test]
fn lookahead_is_reported_and_exact() {
    let mut d = deesser(|s| s.lookahead = 5.0);
    let mut x = vec![0.0f32; 4800];
    x[100] = 0.2;
    let mut y = x.clone();
    d.process(&mut [&mut y[..]]);
    let at = y.iter().position(|v| v.abs() > 1e-6).unwrap();
    assert_eq!(at - 100, d.latency_samples());
    assert_eq!(d.latency_samples(), 240);
}

