// SPDX-License-Identifier: GPL-3.0-or-later
//! The limiter's own bench suite (README.md §6), on synthetic signals.

use super::*;

const SR: f32 = 48000.0;

fn lim(f: impl FnOnce(&mut Settings)) -> Limiter {
    let mut l = Limiter::new();
    l.prepare(SR, 4800, 2);
    let mut s = Settings::default();
    f(&mut s);
    l.set(s);
    l.reset();
    l
}

fn run(l: &mut Limiter, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for c in y.chunks_mut(256) {
        l.process(&mut [c]);
    }
    y
}

fn sine(n: usize, f: f32, amp: f32, phase: f32) -> Vec<f32> {
    (0..n).map(|i| amp * (2.0 * std::f32::consts::PI * f * i as f32 / SR + phase).sin()).collect()
}

fn noise(n: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut r = gfx_dsp::lfo::Rng::new(seed);
    (0..n).map(|_| amp * r.bipolar()).collect()
}

#[test]
fn never_above_the_threshold() {
    for style in Style::ALL {
        for la in 0..4u32 {
            for tp in [false, true] {
                let mut l = lim(|s| {
                    s.style = style;
                    s.lookahead = la;
                    s.true_peak = tp;
                    s.gain = 24.0;
                    s.threshold = -3.0;
                    s.release = 1.0;
                });
                let c = 10f32.powf(-3.0 / 20.0);
                let x = noise(48000, 1.0, 7);
                let y = run(&mut l, &x);
                let peak = y.iter().fold(0.0f32, |m, v| m.max(v.abs()));
                assert!(peak <= c * (1.0 + 1e-6), "{style:?} la {la} tp {tp}: {peak} > {c}");
            }
        }
    }
}

#[test]
fn transparent_below_the_threshold() {
    for la in 0..4u32 {
        let mut l = lim(|s| s.lookahead = la);
        let d = l.latency_samples();
        let x = sine(48000, 440.0, 0.2, 0.0); // −14 dBFS, threshold −6
        let y = run(&mut l, &x);
        assert_eq!(&y[d..], &x[..x.len() - d], "lookahead {la}");
    }
}

#[test]
fn reported_latency_is_the_delay() {
    for (la, tp) in [(1u32, false), (2, false), (3, true)] {
        let mut l = lim(|s| {
            s.lookahead = la;
            s.true_peak = tp;
        });
        let mut x = vec![0.0f32; 4096];
        x[100] = 0.3;
        let y = run(&mut l, &x);
        let at = y.iter().position(|v| v.abs() > 1e-6).unwrap();
        assert_eq!(at - 100, l.latency_samples(), "lookahead {la} tp {tp}");
    }
}

/// Energy outside the first harmonic, relative to it (dB).
fn distortion_db(y: &[f32], f0: f32) -> f64 {
    let probe = |f: f32| {
        let w = 2.0 * std::f64::consts::PI * f as f64 / SR as f64;
        let (mut re, mut im) = (0.0, 0.0);
        for (i, v) in y.iter().enumerate() {
            let win = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / y.len() as f64).cos();
            re += *v as f64 * win * (w * i as f64).cos();
            im += *v as f64 * win * (w * i as f64).sin();
        }
        re * re + im * im
    };
    let fund = probe(f0);
    let harm: f64 = (2..12).map(|k| probe(f0 * k as f32)).sum();
    10.0 * (harm / fund).log10()
}

#[test]
fn bass_is_not_ridden_cycle_by_cycle() {
    let thd = |auto: bool| {
        let mut l = lim(|s| {
            s.release = 1.0;
            s.auto_release = auto;
            s.threshold = -6.0;
        });
        let x = sine(48000 * 2, 41.2, 1.0, 0.0);
        let y = run(&mut l, &x);
        distortion_db(&y[48000..], 41.2)
    };
    let off = thd(false);
    let on = thd(true);
    assert!(on < off - 15.0, "auto {on:.1} dB, plain {off:.1} dB");
    assert!(on < -40.0, "auto {on:.1} dB");
}

#[test]
fn true_peak_holds_between_samples() {
    // fs/4 at 45°: every sample sits at 0.707 of the true peak — the classic inter-sample over
    let mut l = lim(|s| {
        s.lookahead = 2;
        s.true_peak = true;
        s.threshold = -1.0;
        s.gain = 6.0;
    });
    let x = sine(48000, 12000.0, 0.7, std::f32::consts::FRAC_PI_4);
    let y = run(&mut l, &x);
    // reconstruct between samples (8× windowed sinc) and take the peak
    let tail = &y[24000..];
    let mut tp = 0.0f32;
    for i in 32..tail.len() - 32 {
        for k in 0..8 {
            let t = i as f64 + k as f64 / 8.0;
            let mut v = 0.0f64;
            for j in (i - 32)..(i + 32) {
                let d = t - j as f64;
                let s = if d.abs() < 1e-9 { 1.0 } else { (std::f64::consts::PI * d).sin() / (std::f64::consts::PI * d) };
                let w = 0.5 + 0.5 * (std::f64::consts::PI * d / 32.0).cos();
                v += tail[j] as f64 * s * w;
            }
            tp = tp.max(v.abs() as f32);
        }
    }
    let db = 20.0 * tp.log10();
    assert!(db <= -1.0 + 0.1, "true peak {db:.2} dBTP");
}

#[test]
fn clip_style_aliasing() {
    let mut l = lim(|s| {
        s.style = Style::Clip;
        s.threshold = 0.0;
        s.clip = 50.0;
    });
    let x = sine(16384, 5000.0, 2.0, 0.0);
    let y = run(&mut l, &x);
    let y = &y[4096..];
    let probe = |f: f64| {
        let w = 2.0 * std::f64::consts::PI * f / SR as f64;
        let (mut re, mut im) = (0.0, 0.0);
        for (i, v) in y.iter().enumerate() {
            let win = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / y.len() as f64).cos();
            re += *v as f64 * win * (w * i as f64).cos();
            im += *v as f64 * win * (w * i as f64).sin();
        }
        (re * re + im * im).sqrt()
    };
    let fund = probe(5000.0);
    // 9 × 5 kHz = 45 kHz folds to 3 kHz; 11 × 5 = 55 → 7 kHz; 13 × 5 = 65 → 17 kHz
    let alias = [3000.0, 7000.0, 17000.0].iter().map(|&f| probe(f)).fold(0.0, f64::max);
    let db = 20.0 * (alias / fund).log10();
    println!("clip-style alias: {db:.1} dB re fundamental");
    // soft knee (default 50 %), 6 dB over, a 5 kHz tone: the clipper runs at 2× with ADAA and the
    // final clamp stays idle; what remains (~−57 dB) is the 2× fold of the 15th+ harmonics, the
    // price of zero latency
    assert!(db < -55.0, "{db:.1} dB");
}


