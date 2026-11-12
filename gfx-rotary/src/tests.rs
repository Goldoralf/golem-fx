// SPDX-License-Identifier: GPL-3.0-or-later
//! The rotary's own bench suite (README.md §6).

use super::*;

const SR: f32 = 48000.0;

fn rot(s: Settings) -> Rotary {
    let mut r = Rotary::new();
    r.prepare(SR, 4800, 2);
    r.set(s);
    r.reset();
    r
}

fn run(r: &mut Rotary, secs: f32) -> (Vec<f32>, Vec<f32>) {
    let n = (SR * secs) as usize;
    let mut l: Vec<f32> = (0..n).map(|i| 0.3 * (std::f32::consts::TAU * 1000.0 * i as f32 / SR).sin()).collect();
    let mut rr = l.clone();
    for (a, b) in l.chunks_mut(256).zip(rr.chunks_mut(256)) {
        r.process(&mut [a, b]);
    }
    (l, rr)
}

#[test]
fn speeds_settle_on_the_setting() {
    let mut r = rot(Settings::default().with(p::SPEED, 2.0).with(p::WOBBLE, 0.0));
    run(&mut r, 1.0);
    assert!((r.horn().hz / 6.7 - 1.0).abs() < 0.01 && (r.drum().hz / 5.7 - 1.0).abs() < 0.01);
}

#[test]
fn each_rotor_has_its_own_inertia() {
    // slow → fast: 63 % of the way at the accelerate constant
    for (tau, horn) in [(0.16f32, true), (4.1, false)] {
        let mut r = rot(Settings::default().with(p::WOBBLE, 0.0));
        r.set(r.settings().with(p::SPEED, 2.0));
        run(&mut r, tau);
        let (hz, slow, fast) = if horn { (r.horn().hz, 0.8, 6.7) } else { (r.drum().hz, 0.67, 5.7) };
        let frac = (hz - slow) / (fast - slow);
        assert!((frac - 0.632).abs() < 0.03, "{} at τ: {frac:.3}", if horn { "horn" } else { "drum" });
    }
}

#[test]
fn brake_parks_the_horn_at_the_stop_angle() {
    let mut r = rot(Settings::default().with(p::SPEED, 2.0).with(p::WOBBLE, 0.0));
    run(&mut r, 1.0);
    r.set(r.settings().with(p::SPEED, 0.0).with(p::STOP_ANGLE, 90.0));
    run(&mut r, 12.0);
    let a = r.horn().angle;
    assert!((a - 0.25).abs() < 0.01, "parked at {:.1}°", a * 360.0);
    assert!(r.drum().hz.abs() < 0.01);
}

#[test]
fn zero_spacing_is_exactly_mono() {
    let mut r = rot(Settings::default().with(p::SPACING, 0.0).with(p::SPEED, 2.0));
    let (l, rr) = run(&mut r, 1.0);
    assert_eq!(l, rr);
}

#[test]
fn sub_bass_is_unmodulated() {
    let mut r = rot(Settings::default().with(p::SPEED, 2.0).with(p::DRIVE, 0.0).with(p::BASS_KEEP, 200.0));
    let n = (SR * 2.0) as usize;
    let mut l: Vec<f32> = (0..n).map(|i| 0.3 * (std::f32::consts::TAU * 40.0 * i as f32 / SR).sin()).collect();
    let mut rr = l.clone();
    for (a, b) in l.chunks_mut(256).zip(rr.chunks_mut(256)) {
        r.process(&mut [a, b]);
    }
    // per-cycle peaks over the last second: a 40 Hz tone barely moves (the crossover leaks a little)
    let peaks: Vec<f32> = l[n / 2..].chunks(1200).map(|c| c.iter().fold(0.0f32, |m, v| m.max(v.abs()))).collect();
    let (lo, hi) = peaks.iter().fold((f32::MAX, 0.0f32), |(a, b), v| (a.min(*v), b.max(*v)));
    assert!(20.0 * (hi / lo).log10() < 1.5, "40 Hz swings {:.2} dB", 20.0 * (hi / lo).log10());
}
