// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5). Targets are computed from the circuit equations.

use super::*;

const SR: f32 = 48000.0;
const PI: f64 = std::f64::consts::PI;

fn emet(set: &[(&str, f32)], stage_only: bool) -> Emet {
    let mut e = Emet::new();
    for (k, v) in set {
        e.set_param(k, *v);
    }
    e.prepare(SR, 512, 1);
    e.reset();
    e.stage_only = stage_only;
    e
}

fn run(e: &mut Emet, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for c in y.chunks_mut(512) {
        e.process(&mut [c]);
    }
    y
}

fn sine(freq: f64, amp: f64, n: usize) -> Vec<f32> {
    (0..n).map(|i| (amp * (2.0 * PI * freq * i as f64 / SR as f64).sin()) as f32).collect()
}

fn gain_db(x: &[f32], y: &[f32]) -> f64 {
    let h = x.len() / 2;
    let ei: f64 = x[h..].iter().map(|v| (*v as f64).powi(2)).sum();
    let eo: f64 = y[h..].iter().map(|v| (*v as f64).powi(2)).sum();
    10.0 * (eo / ei).log10()
}

/// Amplitude of the component at `f` (Goertzel-style projection), over the second half.
fn tone_amp(y: &[f32], f: f64) -> f64 {
    let h = y.len() / 2;
    let w = 2.0 * PI * f / SR as f64;
    let (mut c, mut s) = (0.0, 0.0);
    for (i, v) in y[h..].iter().enumerate() {
        let ph = w * (i + h) as f64;
        c += *v as f64 * ph.cos();
        s += *v as f64 * ph.sin();
    }
    2.0 * c.hypot(s) / (y.len() - h) as f64
}

fn warped(f: f64) -> f64 {
    SR as f64 / PI * (PI * f / SR as f64).tan()
}

#[test]
fn gain_law_is_the_808s() {
    for (g, want) in [(0.0, stage_db(0.0, 1000.0)), (100.0, stage_db(100.0, 1000.0))] {
        // small enough to stay linear even at full gain
        let x = sine(1000.0, 1e-4, 96000);
        let mut e = emet(&[("gain", g as f32)], true);
        let got = gain_db(&x, &run(&mut e, &x));
        assert!((got - want).abs() < 0.3, "gain {g}: {got} dB vs {want}");
    }
    // the README's analytic anchors
    assert!((stage_db(0.0, 1000.0) - 8.3).abs() < 0.3, "{}", stage_db(0.0, 1000.0));
    assert!((stage_db(100.0, 1000.0) - 44.4).abs() < 0.5, "{}", stage_db(100.0, 1000.0));
}

#[test]
fn tone_network_matches_its_analysis() {
    for t in [0.0f32, 50.0, 100.0] {
        let net = tone_network(t as f64 / 100.0);
        for f in [100.0, 700.0, 2000.0, 6000.0] {
            let x = sine(f, 1e-4, 96000);
            let mut e = emet(&[("gain", 0.0), ("tone_stack", t)], false);
            let y = run(&mut e, &x);
            let got = gain_db(&x, &y) - stage_db(0.0, f);
            let (re, im) = net.response(warped(f));
            let want = 20.0 * re.hypot(im).log10();
            assert!((got - want).abs() < 0.25, "tone {t}, {f} Hz: {got} vs {want}");
        }
    }
}

#[test]
fn symmetric_by_default() {
    // drive the clipper hard: no even harmonics at asym 0, some at asym 100
    let x = sine(500.0, 0.3, 96000);
    let mut e = emet(&[("gain", 80.0)], true);
    let y = run(&mut e, &x);
    let h1 = tone_amp(&y, 500.0);
    let h2 = tone_amp(&y, 1000.0);
    assert!(20.0 * (h2 / h1).log10() < -80.0, "2nd harmonic {} dBc", 20.0 * (h2 / h1).log10());
    let mut e = emet(&[("gain", 80.0), ("asym", 100.0)], true);
    let y = run(&mut e, &x);
    let h2a = tone_amp(&y, 1000.0) / tone_amp(&y, 500.0);
    assert!(20.0 * h2a.log10() > -40.0, "asym gives no even harmonics");
}

#[test]
fn headroom_moves_the_knee() {
    // the same signal clips far less with the MOSFET knee than with a TS knee (−13 dB)
    let x = sine(1000.0, 0.3, 96000);
    let thd = |h: f32| {
        let mut e = emet(&[("gain", 30.0), ("headroom", h)], true);
        let y = run(&mut e, &x);
        tone_amp(&y, 3000.0) / tone_amp(&y, 1000.0)
    };
    assert!(thd(-13.0) > 3.0 * thd(0.0), "TS knee {} vs MOSFET knee {}", thd(-13.0), thd(0.0));
}

#[test]
fn aliasing_is_low() {
    // −12 dBFS 5 kHz at full gain: inharmonic components (aliases) relative to the fundamental
    let x = sine(5000.0, 0.25, 96000);
    let mut e = emet(&[("gain", 100.0)], true);
    let y = run(&mut e, &x);
    let f0 = tone_amp(&y, 5000.0);
    // where the 7th, 9th, 11th and 13th harmonics fold back into the audible band at 48 kHz (the 5th,
    // 25 kHz, lands at 23 kHz through the decimator's transition band: above 20 kHz, not an alias)
    let mut worst: f64 = -300.0;
    for f in [13000.0, 3000.0, 7000.0, 17000.0] {
        worst = worst.max(20.0 * (tone_amp(&y, f) / f0).log10());
    }
    eprintln!("worst alias {worst:.1} dB re fundamental");
    assert!(worst < -90.0, "worst alias {worst} dB");
}

#[test]
fn bass_keep_sums_flat() {
    for f in [60.0, 120.0, 240.0] {
        let x = sine(f, 1e-4, 96000);
        let mut a = emet(&[("gain", 0.0), ("bass_keep", 120.0)], true);
        let mut b = emet(&[("gain", 0.0)], true);
        let ga = gain_db(&x, &run(&mut a, &x));
        // below the split the signal is clean (0 dB), above it the stage's own gain; at small signal
        // both paths are linear, so the sum must lie between them without a notch
        let gb = gain_db(&x, &run(&mut b, &x));
        assert!(ga > -0.5 && ga < gb + 0.5, "{f} Hz: split {ga} dB, stage {gb} dB");
    }
}

#[test]
fn deterministic() {
    let x = sine(220.0, 0.4, 9600);
    let r = || {
        let mut e = emet(&[("gain", 60.0)], false);
        run(&mut e, &x)
    };
    assert_eq!(r(), r());
}

