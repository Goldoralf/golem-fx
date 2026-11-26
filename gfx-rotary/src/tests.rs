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

#[test]
#[ignore]
fn profile() {
    use std::time::Instant;
    for (name, s) in [
        ("default", Settings::default()),
        ("drive 0", Settings::default().with(p::DRIVE, 0.0)),
        ("vibratone (no horn)", Settings::default().with(p::CABINET, 3.0)),
        ("bass_keep off", Settings::default().with(p::BASS_KEEP, 0.0)),
    ] {
        let mut r = rot(s);
        let n = 48000 * 5;
        let mut l: Vec<f32> = (0..n).map(|i| 0.2 * (std::f32::consts::TAU * 220.0 * i as f32 / SR).sin()).collect();
        let mut rr = l.clone();
        let t = Instant::now();
        for (a, b) in l.chunks_mut(128).zip(rr.chunks_mut(128)) {
            r.process(&mut [a, b]);
        }
        eprintln!("{name:22} {:6.1} ns", t.elapsed().as_secs_f64() * 1e9 / n as f64);
    }
}

// ---------------------------------------------------------------- gfx-dsp `rotary` move: the null

/// FNV-1a over the output samples' bits.
fn fnv(h: &mut u64, x: &[f32]) {
    for v in x {
        for b in v.to_bits().to_le_bytes() {
            *h ^= b as u64;
            *h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
}

/// Every cabinet × speed × transition (slow → fast, fast → stop), mono and stereo, at 48 and 96 kHz,
/// drive and depth moved: the renders' hash, written before the move into gfx-dsp `rotary` (gfx-organ
/// README §2.5). Any arithmetic change fails it.
fn null_hash() -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for sr in [48000.0f32, 96000.0] {
        for cab in 0..5 {
            for (from, to) in [(0.0f32, 0.0f32), (1.0, 1.0), (2.0, 2.0), (1.0, 2.0), (2.0, 0.0)] {
                for nch in [1usize, 2] {
                    let mut r = Rotary::new();
                    r.prepare(sr, 512, nch);
                    let s = Settings::default().with(p::CABINET, cab as f32).with(p::SPEED, from).with(p::DRIVE, if cab % 2 == 0 { 20.0 } else { 70.0 }).with(p::DEPTH, 40.0 + 10.0 * cab as f32);
                    r.set(s);
                    r.reset();
                    let n = (0.6 * sr) as usize;
                    let x: Vec<f32> = (0..n)
                        .map(|i| {
                            let t = i as f32 / sr;
                            0.25 * (std::f32::consts::TAU * 110.0 * t).sin() + 0.15 * (std::f32::consts::TAU * 1234.5 * t).sin() + 0.1 * (std::f32::consts::TAU * 3321.0 * t).sin()
                        })
                        .collect();
                    let mut l = x.clone();
                    let mut rr: Vec<f32> = x.iter().map(|v| -0.5 * v).collect();
                    let mut k = 0;
                    while k < n {
                        if k == 256 * 60 {
                            r.set(r.settings().with(p::SPEED, to));
                        }
                        let e = (k + 256).min(n);
                        if nch == 2 {
                            r.process(&mut [&mut l[k..e], &mut rr[k..e]]);
                        } else {
                            r.process(&mut [&mut l[k..e]]);
                        }
                        k = e;
                    }
                    fnv(&mut h, &l);
                    if nch == 2 {
                        fnv(&mut h, &rr);
                    }
                }
            }
        }
    }
    h
}

#[test]
fn null_against_the_build_before_the_move() {
    let h = null_hash();
    println!("rotary null hash {h:#018x}");
    assert_eq!(h, ROTARY_NULL);
}

const ROTARY_NULL: u64 = 0x4bdc92c401da4958;
