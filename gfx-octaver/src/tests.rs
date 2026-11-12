// SPDX-License-Identifier: GPL-3.0-or-later
//! The octaver's own bench suite (README.md §6).

use super::*;

const SR: f32 = 48000.0;

fn oc(s: Settings) -> Octaver {
    let mut o = Octaver::new();
    o.prepare(SR, 4800, 1);
    o.set(s);
    o.reset();
    o
}

/// Sub only.
fn sub(mode: f32) -> Settings {
    Settings::default().with(p::MODE, mode).with(p::DIRECT, 0.0).with(p::OCTAVE1, 1.0).with(p::SUB_TONE, 2000.0)
}

fn run(o: &mut Octaver, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for c in y.chunks_mut(256) {
        o.process(&mut [c]);
    }
    y
}

fn sine(f: f32, amp: f32, n: usize) -> Vec<f32> {
    (0..n).map(|i| amp * (std::f32::consts::TAU * f * i as f32 / SR).sin()).collect()
}

fn probe(v: &[f32], f: f64) -> f64 {
    let n = v.len();
    let w = std::f64::consts::TAU * f / SR as f64;
    let (mut re, mut im, mut ws) = (0.0, 0.0, 0.0);
    for (k, s) in v.iter().enumerate() {
        let h = 0.5 - 0.5 * (std::f64::consts::TAU * k as f64 / n as f64).cos();
        re += *s as f64 * h * (w * k as f64).cos();
        im += *s as f64 * h * (w * k as f64).sin();
        ws += h;
    }
    2.0 * (re * re + im * im).sqrt() / ws
}

fn rms(v: &[f32]) -> f64 {
    (v.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
}

#[test]
fn clean_sub_is_a_near_pure_sine() {
    let mut o = oc(sub(1.0).with(p::INSTRUMENT, 1.0));
    let y = run(&mut o, &sine(110.0, 0.3, 96000));
    let seg = &y[48000..];
    let (f1, f3) = (probe(seg, 55.0), probe(seg, 165.0));
    let db = 20.0 * (f3 / f1).log10();
    assert!(f1 > 0.1 && db < -50.0, "sub {f1:.3}, 3rd harmonic {db:.1} dBc");
}

#[test]
fn every_mode_makes_the_octave_below() {
    for mode in [0.0f32, 1.0, 2.0, 3.0] {
        let mut o = oc(sub(mode));
        let y = run(&mut o, &sine(220.0, 0.3, 96000));
        let seg = &y[48000..];
        let (f_sub, f_in) = (probe(seg, 110.0), probe(seg, 220.0));
        assert!(f_sub > 0.03 && f_sub > f_in, "mode {mode}: 110 Hz {f_sub:.3}, 220 Hz {f_in:.3}");
    }
}

#[test]
fn bass_sub_level_holds_over_the_octave() {
    let level = |f: f32| {
        let mut o = oc(sub(1.0).with(p::INSTRUMENT, 1.0).with(p::SUB_TONE, 250.0));
        let y = run(&mut o, &sine(f, 0.3, 96000));
        20.0 * rms(&y[48000..]).log10()
    };
    let (lo, hi) = (level(41.2), level(82.4));
    assert!((lo - hi).abs() < 3.0, "E1 {lo:.1} dB, E2 {hi:.1} dB");
}

#[test]
fn legato_does_not_jump() {
    for mode in [0.0f32, 1.0] {
        let mut o = oc(sub(mode));
        let mut x = sine(110.0, 0.3, 48000);
        let mut ph = std::f32::consts::TAU * 110.0 * 48000.0 / SR;
        for _ in 0..48000 {
            ph += std::f32::consts::TAU * 130.8 / SR;
            x.push(0.3 * ph.sin());
        }
        let y = run(&mut o, &x);
        let jump = y[2000..].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(jump < 0.05, "mode {mode}: biggest step {jump}");
    }
}

#[test]
fn poly_octaves_each_note_of_a_chord() {
    let mut o = oc(sub(3.0));
    let x: Vec<f32> = sine(110.0, 0.2, 96000).iter().zip(sine(165.0, 0.2, 96000)).map(|(a, b)| a + b).collect();
    let y = run(&mut o, &x);
    let seg = &y[48000..];
    let (a, b) = (probe(seg, 55.0), probe(seg, 82.5));
    assert!(a > 0.03 && b > 0.03, "55 Hz {a:.3}, 82.5 Hz {b:.3}");
}
