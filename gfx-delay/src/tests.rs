// SPDX-License-Identifier: GPL-3.0-or-later
//! The delay's own bench suite (README.md §5).

use super::*;

const SR: f32 = 48000.0;

fn dl(s: Settings) -> Delay {
    let mut d = Delay::new();
    d.prepare(SR, 4800, 2);
    d.set(s);
    d.set_transport(TransportInfo { tempo: Some(120.0), pos_beats: None, playing: false });
    d.reset();
    d
}

fn run(d: &mut Delay, l: &mut [f32], r: &mut [f32]) {
    for (a, b) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        d.process(&mut [a, b]);
    }
}

/// A wet-only, flat digital delay with no feedback, to measure the line itself.
fn plain() -> Settings {
    Settings::default().with(p::FEEDBACK, 0.0).with(p::MIX, 100.0).with(p::KILL_DRY, 1.0).with(p::MOD, 0.0).with(p::LOW_CUT, 20.0)
}

fn first_peak(v: &[f32]) -> usize {
    v.iter().enumerate().fold((0, 0.0f32), |(bi, bv), (i, x)| if x.abs() > bv { (i, x.abs()) } else { (bi, bv) }).0
}

#[test]
fn sync_lands_on_the_sample() {
    for t in 0..9 {
        let mut d = dl(plain().with(p::TIME, t as f32));
        let n = 4 * 48000;
        let mut l = vec![0.0f32; n];
        l[100] = 1.0;
        let mut r = l.clone();
        run(&mut d, &mut l, &mut r);
        let want = 100.0 + NOTES[t] * 24000.0; // 120 BPM: 24,000 samples per beat
        let got = first_peak(&l) as f64;
        assert!((got - want).abs() <= 1.0, "time {t}: peak at {got}, want {want}");
    }
}

#[test]
fn self_oscillation_is_bounded_and_recovers() {
    let mut d = dl(Settings::default().with(p::FEEDBACK, 110.0).with(p::MIX, 100.0).with(p::TIME, 9.0).with(p::TIME_MS, 50.0));
    let mut r = Rng::new(1);
    let mut l: Vec<f32> = (0..48000 * 10).map(|i| if i < 4800 { 0.5 * r.bipolar() } else { 0.0 }).collect();
    let mut rr = l.clone();
    run(&mut d, &mut l, &mut rr);
    let pk = l[48000 * 8..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(pk.is_finite() && pk < 2.0 && pk > 0.01, "level {pk}");
    d.set(d.settings().with(p::FEEDBACK, 30.0));
    let mut l2 = vec![0.0f32; 48000 * 3];
    let mut r2 = l2.clone();
    run(&mut d, &mut l2, &mut r2);
    let tail = l2[48000 * 2..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(tail < 1e-3, "does not recover: {tail}");
}

#[test]
fn hold_sustains_flat() {
    let mut d = dl(Settings::default().with(p::FEEDBACK, 50.0).with(p::MIX, 100.0).with(p::KILL_DRY, 1.0).with(p::TIME, 9.0).with(p::TIME_MS, 300.0));
    let n = 48000;
    let mut l: Vec<f32> = (0..n).map(|i| 0.3 * (TAU as f32 * 440.0 * i as f32 / SR).sin()).collect();
    let mut r = l.clone();
    run(&mut d, &mut l, &mut r);
    d.set(d.settings().with(p::HOLD, 1.0));
    let mut l: Vec<f32> = (0..48000 * 30).map(|i| 0.3 * (TAU as f32 * 440.0 * i as f32 / SR).sin()).collect();
    let mut r = l.clone();
    run(&mut d, &mut l, &mut r);
    let rms = |v: &[f32]| (v.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
    let (a, b) = (rms(&l[48000..48000 * 3]), rms(&l[48000 * 27..48000 * 29]));
    assert!((20.0 * (b / a).log10()).abs() < 0.5, "hold drifts {:.2} dB", 20.0 * (b / a).log10());
}

#[test]
fn mono_mode_is_mono() {
    let mut d = dl(Settings::default());
    let mut r0 = Rng::new(2);
    let mut l: Vec<f32> = (0..48000).map(|_| 0.3 * r0.bipolar()).collect();
    let mut r: Vec<f32> = (0..48000).map(|_| 0.3 * r0.bipolar()).collect();
    let (dl_, dr_) = (l.clone(), r.clone());
    run(&mut d, &mut l, &mut r);
    // wet identical on both sides: out − dry is the same
    for i in 0..48000 {
        assert!(((l[i] - dl_[i]) - (r[i] - dr_[i])).abs() < 1e-6);
    }
}

#[test]
fn ducking_holds_the_repeats_down_while_playing() {
    let s = Settings::default().with(p::DUCK, 12.0).with(p::FEEDBACK, 50.0).with(p::MIX, 100.0).with(p::KILL_DRY, 1.0).with(p::TIME, 9.0).with(p::TIME_MS, 100.0);
    let level = |duck: f32| {
        let mut d = dl(s.with(p::DUCK, duck));
        let mut l: Vec<f32> = (0..48000).map(|i| 0.3 * (TAU as f32 * 330.0 * i as f32 / SR).sin()).collect();
        let mut r = l.clone();
        run(&mut d, &mut l, &mut r);
        l[24000..].iter().fold(0.0f32, |m, v| m.max(v.abs()))
    };
    let db = 20.0 * (level(12.0) / level(0.0)).log10();
    assert!((db + 12.0).abs() < 1.0, "{db:.2} dB");
}

#[test]
fn reverse_plays_windows_backwards() {
    // a rising ramp comes back falling
    let mut d = dl(plain().with(p::DIRECTION, 1.0).with(p::TIME, 9.0).with(p::TIME_MS, 200.0));
    let n = 48000;
    let mut l: Vec<f32> = (0..n).map(|i| ((i % 9600) as f32 / 9600.0) * 0.5).collect();
    let mut r = l.clone();
    run(&mut d, &mut l, &mut r);
    let slope: f32 = l[24000..40000].windows(2).map(|w| (w[1] - w[0]).signum()).sum();
    assert!(slope < 0.0, "mostly falling: {slope}");
}

#[test]
fn loop_pitch_climbs_each_repeat() {
    let mut d = dl(plain().with(p::LOOP_PITCH, 12.0).with(p::FEEDBACK, 60.0).with(p::TIME, 9.0).with(p::TIME_MS, 250.0).with(p::HIGH_CUT, 20000.0));
    let n = 48000;
    let mut l: Vec<f32> = (0..n).map(|i| if i < 4800 { 0.3 * (TAU as f32 * 220.0 * i as f32 / SR).sin() } else { 0.0 }).collect();
    let mut r = l.clone();
    run(&mut d, &mut l, &mut r);
    // the 2nd repeat (≈ 500 ms + the shifter's window) is near 880 Hz: count zero crossings
    let seg = &l[26400..29000];
    let zc = seg.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count() as f32;
    let f = zc / (seg.len() as f32 / SR);
    assert!((f / 880.0 - 1.0).abs() < 0.05, "{f} Hz");
}
