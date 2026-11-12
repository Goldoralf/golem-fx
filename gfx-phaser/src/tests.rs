// SPDX-License-Identifier: GPL-3.0-or-later
//! The phaser's own bench suite (README.md §6).

use super::*;

const SR: f32 = 48000.0;

fn ph(s: Settings) -> Phaser {
    let mut f = Phaser::new();
    f.prepare(SR, 4800, 1);
    f.set(s);
    f.reset();
    f
}

fn run(f: &mut Phaser, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for b in y.chunks_mut(256) {
        f.process(&mut [b]);
    }
    y
}

fn sine(freq: f32, n: usize) -> Vec<f32> {
    (0..n).map(|i| 0.3 * (std::f32::consts::TAU * freq * i as f32 / SR).sin()).collect()
}

fn peak(v: &[f32]) -> f32 {
    v.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

#[test]
fn notches_are_exact() {
    // 4 equal stages at fc: notches where each stage turns π/4 and 3π/4
    let s = Settings::default().with(p::FEEDBACK, -60.0).with(p::DEPTH, 0.0).with(p::MANUAL, 50.0).with(p::SWEEP_SRC, 2.0);
    let fc = (440.0f64 * 1600.0).sqrt();
    let gain = |f: f64| {
        let mut x = ph(s);
        let y = run(&mut x, &sine(f as f32, 24000));
        peak(&y[12000..]) / 0.3
    };
    // through the bilinear map (fc prewarped, so exact): tan(πf / fs) = tan(πfc / fs) · tan(kπ / 8)
    let g = (std::f64::consts::PI * fc / SR as f64).tan();
    let warp = |k: f64| SR as f64 / std::f64::consts::PI * (g * (k * std::f64::consts::PI / 8.0).tan()).atan();
    for nf in [warp(1.0), warp(3.0)] {
        assert!(gain(nf) < 0.01, "{nf:.0} Hz: {}", gain(nf));
        // 1 % off the notch is already clearly out of it
        assert!(gain(nf * 1.05) > 3.0 * gain(nf));
    }
}

#[test]
fn feedback_is_bounded_in_every_model() {
    for model in 0..9 {
        for pol in [0.0, 1.0] {
            let s = Settings::default().with(p::MODEL, model as f32).with(p::FEEDBACK, -1.0).with(p::FB_POLARITY, pol).with(p::RATE, 8.0).with(p::STAGES, 6.0).with(p::COLOR, 100.0).with(p::WET, 6.0);
            let mut f = ph(s);
            let mut r = Rng::new(3);
            let x: Vec<f32> = (0..48000).map(|_| 0.5 * r.bipolar()).collect();
            let y = run(&mut f, &x);
            assert!(y.iter().all(|v| v.is_finite() && v.abs() < 30.0), "model {model} pol {pol}: {}", peak(&y));
        }
    }
}

#[test]
fn parameter_changes_never_move_the_phase() {
    let mut f = ph(Settings::default());
    run(&mut f, &sine(440.0, 4800));
    let p0 = f.phase();
    f.set(f.settings().with(p::MODEL, 4.0).with(p::LOW, 200.0).with(p::FEEDBACK, -6.0).with(p::SHAPE, 3.0));
    assert_eq!(f.phase(), p0);
}

#[test]
fn audio_rate_sweep_stays_clean() {
    let s = Settings::default().with(p::RATE, 8.0).with(p::LOW, 20.0).with(p::HIGH, 16000.0).with(p::FEEDBACK, -2.0);
    let mut f = ph(s);
    let y = run(&mut f, &sine(1000.0, 48000));
    assert!(y.iter().all(|v| v.is_finite()) && peak(&y) < 5.0, "{}", peak(&y));
}

#[test]
fn barberpole_has_no_reset_dips() {
    let s = Settings::default().with(p::MODEL, 7.0).with(p::RATE, 0.5).with(p::DRY, -60.0).with(p::FEEDBACK, -60.0).with(p::LOW, 100.0).with(p::HIGH, 8000.0);
    let mut f = ph(s);
    let mut r = Rng::new(8);
    let x: Vec<f32> = (0..48000 * 6).map(|_| 0.3 * r.bipolar()).collect();
    let y = run(&mut f, &x);
    let w = 2400;
    let rms: Vec<f64> = y[48000..].chunks(w).map(|c| (c.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / w as f64).sqrt()).collect();
    let (lo, hi) = rms.iter().fold((f64::MAX, 0.0f64), |(a, b), v| (a.min(*v), b.max(*v)));
    assert!(20.0 * (hi / lo).log10() < 4.0, "level swings {:.1} dB", 20.0 * (hi / lo).log10());
}

#[test]
fn vibe_vibrato_has_no_dry() {
    // a tone through the Vibe in vibrato mode wobbles in pitch but never adds the dry: no notches
    let s = Settings::default().with(p::MODEL, 4.0).with(p::VIBE_MODE, 1.0).with(p::RATE, 5.0);
    let mut f = ph(s);
    let y = run(&mut f, &sine(800.0, 48000));
    let w = 480;
    let min = y[24000..].chunks(w).map(peak).fold(f32::MAX, f32::min);
    assert!(min > 0.1, "level dipped to {min}");
}
