// SPDX-License-Identifier: GPL-3.0-or-later
//! The flanger's own bench suite (README.md §6).

use super::*;
use gfx_dsp::lfo::Rng;
use gfx_dsp::Effect;

const SR: f32 = 48000.0;

fn fl(s: Settings) -> Flanger {
    let mut f = Flanger::new();
    f.prepare(SR, 4800, 1);
    f.set(s);
    f.reset();
    f
}

fn run(f: &mut Flanger, x: &[f32]) -> Vec<f32> {
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

fn pedal(manual: f32, pol: f32) -> Settings {
    Settings::default().with(p::MODE, 8.0).with(p::MANUAL, manual).with(p::FEEDBACK, -60.0).with(p::WET, 0.0).with(p::DRY, 0.0).with(p::FB_POLARITY, pol)
}

#[test]
fn notches_where_the_comb_says() {
    // τ = 1 ms: same polarity → notches at 500, 1500 Hz; peaks at 1 kHz
    let gain = |f: f32, s: Settings| {
        let mut fl = fl(s);
        let y = run(&mut fl, &sine(f, 24000));
        peak(&y[12000..]) / 0.3
    };
    let s = pedal(1.0, 0.0);
    assert!(gain(500.0, s) < 0.02, "500 Hz {}", gain(500.0, s));
    assert!(gain(1500.0, s) < 0.03, "1500 Hz {}", gain(1500.0, s));
    assert!((gain(1000.0, s) - 2.0).abs() < 0.05, "1 kHz {}", gain(1000.0, s));
}

#[test]
fn tape_tz_cancels_through_zero_and_reports_1_ms() {
    let s = Settings::default().with(p::MODE, 4.0).with(p::WET, 0.0).with(p::DRY, 0.0).with(p::FEEDBACK, -60.0).with(p::RATE, 0.05).with(p::MANUAL, 0.5).with(p::LENGTH, 2.0);
    let mut f = fl(s);
    assert_eq!(f.latency(), 48);
    let mut r = Rng::new(4);
    let x: Vec<f32> = (0..48000 * 12).map(|_| 0.3 * r.bipolar()).collect();
    let y = run(&mut f, &x);
    // the quietest 1 ms window vs the input (the sweep crosses zero slowly)
    let w = 48;
    let worst = y[4800..].chunks(w).map(|c| (c.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / w as f64).sqrt()).fold(f64::MAX, f64::min);
    let db = 20.0 * (worst / (0.3 / 3f64.sqrt())).log10();
    assert!(db < -40.0, "{db:.1} dB");
    let other = fl(Settings::default());
    assert_eq!(other.latency(), 0);
}

#[test]
fn loop_stays_bounded() {
    for mode in 0..9 {
        let s = Settings::default().with(p::MODE, mode as f32).with(p::FEEDBACK, -1.0).with(p::DRIVE, 100.0).with(p::WET, 0.0).with(p::RATE, 5.0).with(p::MANUAL, 0.05);
        let mut f = fl(s);
        let mut r = Rng::new(7);
        let x: Vec<f32> = (0..48000).map(|_| 0.5 * r.bipolar()).collect();
        let y = run(&mut f, &x);
        assert!(y.iter().all(|v| v.is_finite() && v.abs() < 20.0), "mode {mode}: {}", peak(&y));
    }
}

#[test]
fn parameter_changes_never_move_the_phase() {
    let mut f = fl(Settings::default());
    run(&mut f, &sine(440.0, 4800));
    let ph = f.phase();
    f.set(f.settings().with(p::MODE, 2.0).with(p::SHAPE, 3.0).with(p::FEEDBACK, -3.0));
    assert_eq!(f.phase(), ph);
}

#[test]
fn barberpole_has_no_reset_dips() {
    let s = Settings::default().with(p::MODE, 6.0).with(p::RATE, 0.5).with(p::WET, 0.0).with(p::DRY, -60.0).with(p::FEEDBACK, -60.0);
    let mut f = fl(s);
    let mut r = Rng::new(8);
    let x: Vec<f32> = (0..48000 * 6).map(|_| 0.3 * r.bipolar()).collect();
    let y = run(&mut f, &x);
    let w = 2400;
    let rms: Vec<f64> = y[48000..].chunks(w).map(|c| (c.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / w as f64).sqrt()).collect();
    let (lo, hi) = rms.iter().fold((f64::MAX, 0.0f64), |(a, b), v| (a.min(*v), b.max(*v)));
    assert!(20.0 * (hi / lo).log10() < 4.0, "level swings {:.1} dB", 20.0 * (hi / lo).log10());
}
