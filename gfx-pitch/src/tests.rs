// SPDX-License-Identifier: GPL-3.0-or-later
//! The pitch shifter's own bench suite (README.md §6).

use super::*;

const SR: f32 = 48000.0;

fn ps(s: Settings) -> Pitch {
    let mut q = Pitch::new();
    q.prepare(SR, 4800, 1);
    q.set(s);
    q.reset();
    q
}

fn run(q: &mut Pitch, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for c in y.chunks_mut(256) {
        q.process(&mut [c]);
    }
    y
}

fn freq_of(y: &[f32]) -> f64 {
    let mut t = Vec::new();
    for i in 1..y.len() {
        if y[i - 1] < 0.0 && y[i] >= 0.0 {
            t.push((i - 1) as f64 + (-y[i - 1] / (y[i] - y[i - 1])) as f64);
        }
    }
    (t.len() - 1) as f64 / (t[t.len() - 1] - t[0]) * SR as f64
}

fn sine(f: f32, n: usize) -> Vec<f32> {
    (0..n).map(|i| 0.3 * (std::f32::consts::TAU * f * i as f32 / SR).sin()).collect()
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

#[test]
fn whammy_is_in_tune_at_every_integer_shift() {
    for st in [-12, -7, -5, -2, 2, 5, 7, 12] {
        let mut q = ps(Settings::default().with(p::SHIFT, st as f32).with(p::MIX, 100.0));
        let y = run(&mut q, &sine(220.0, 96000));
        let f = freq_of(&y[24000..]);
        let cents = 1200.0 * (f / 220.0).log2();
        assert!((cents - 100.0 * st as f64).abs() < 2.0, "{st} st: {cents:.2} cents");
    }
}

#[test]
fn latency_is_the_engines_centre() {
    for (e, t, ms) in [(0.0, 0.0, 12.0), (1.0, 0.0, 14.0), (1.0, 1.0, 22.0)] {
        let q = ps(Settings::default().with(p::ENGINE, e).with(p::TRACKING, t));
        assert_eq!(q.latency(), (ms * 48.0f64) as usize);
    }
}

#[test]
fn whammy_sweep_is_continuous() {
    // shift 0 → +12 in 300 ms on a held note: no splice click (no sample-to-sample jump beyond the sine's own)
    let mut q = ps(Settings::default().with(p::SHIFT, 0.0).with(p::MIX, 100.0).with(p::GLIDE, 300.0));
    let x = sine(220.0, 96000);
    let mut y = x.clone();
    let (a, b) = y.split_at_mut(24000);
    for c in a.chunks_mut(256) {
        q.process(&mut [c]);
    }
    q.set(q.settings().with(p::SHIFT, 12.0));
    for c in b.chunks_mut(256) {
        q.process(&mut [c]);
    }
    // the largest step a 0.3 sine at 440 Hz can take is 0.3 · 2π · 440 / 48000 ≈ 0.017
    let jump = y[1000..].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    assert!(jump < 0.03, "biggest step {jump}");
}

#[test]
fn poly_shifts_a_chord() {
    let mut q = ps(Settings::default().with(p::ENGINE, 1.0).with(p::SHIFT, 12.0).with(p::MIX, 100.0));
    let x: Vec<f32> = sine(110.0, 96000).iter().zip(sine(164.8, 96000)).zip(sine(329.6, 96000)).map(|((a, b), c)| (a + b + c) / 3.0).collect();
    let y = run(&mut q, &x);
    let seg = &y[48000..];
    for f in [220.0, 329.6, 659.2] {
        let up = probe(seg, f);
        assert!(up > 0.03, "{f} Hz: {up:.3}");
    }
    // the originals are gone (−20 dB at least)
    let orig = probe(seg, 110.0);
    assert!(orig < 0.1 * probe(seg, 220.0), "110 Hz left: {orig:.3}");
}

#[test]
#[ignore]
fn activation_cost() {
    use std::time::Instant;
    let mut q = Pitch::new();
    let t = Instant::now();
    q.prepare(SR, 512, 2);
    eprintln!("prepare  {:8.3} ms", t.elapsed().as_secs_f64() * 1e3);
    q.set(Settings::default());
    let t = Instant::now();
    q.reset();
    eprintln!("reset    {:8.3} ms", t.elapsed().as_secs_f64() * 1e3);
    let mut l = sine(220.0, 512);
    let mut r = l.clone();
    for k in 0..6 {
        let t = Instant::now();
        q.process(&mut [&mut l[..], &mut r[..]]);
        eprintln!("block {k}  {:8.3} ms", t.elapsed().as_secs_f64() * 1e3);
    }
    // the worst block over 10 s
    let mut worst = 0.0f64;
    for _ in 0..940 {
        let t = Instant::now();
        q.process(&mut [&mut l[..], &mut r[..]]);
        worst = worst.max(t.elapsed().as_secs_f64() * 1e3);
    }
    eprintln!("worst block over 10 s {:8.3} ms (512 samples = 10.7 ms of budget)", worst);
}
