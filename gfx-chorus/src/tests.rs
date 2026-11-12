// SPDX-License-Identifier: GPL-3.0-or-later
//! The chorus's own bench suite.

use super::*;
use voicing::Mode;

const SR: f32 = 48000.0;

fn ch(s: Settings, nch: usize) -> Chorus {
    let mut c = Chorus::new();
    c.prepare(SR, 4800, nch);
    c.set(s);
    c.reset();
    c
}

fn noise(n: usize, seed: u64) -> Vec<f32> {
    let mut r = Rng::new(seed);
    (0..n).map(|_| 0.3 * r.bipolar()).collect()
}

fn rms(v: &[f32]) -> f64 {
    (v.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
}

#[test]
fn depth_is_the_peak_to_peak_swing() {
    for &(mode, depth) in &[(Mode::Classic, 5.0f32), (Mode::DualI, 3.7), (Mode::Dim3, 2.4)] {
        let s = Settings::default().with(p::MODE, mode as u8 as f32).with(p::DEPTH, depth).with(p::RATE, 2.0).with(p::MOD_CUSTOM, 1.0).with(p::IMPERFECTION, 0.0).with(p::DRIFT, 0.0).with(p::LFO_SHAPE, 0.0).with(p::DELAY_BASE, 8.0);
        let mut c = ch(s, 1);
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        let mut x = vec![0.0f32; 1];
        for _ in 0..48000 {
            c.process(&mut [&mut x[..]]);
            let d = c.delays_ms()[0];
            lo = lo.min(d);
            hi = hi.max(d);
        }
        assert!(((hi - lo) - depth as f64).abs() < 0.02, "{mode:?}: swing {:.3} ms", hi - lo);
    }
}

#[test]
fn every_mode_profile_and_engine_is_stable() {
    for profile in 0..5 {
        for mode in 0..9 {
            for engine in [None, Some(0.0), Some(2.0)] {
                let mut s = Settings::default().with(p::PROFILE, profile as f32).with(p::MODE, mode as f32).with(p::DEPTH, 12.0).with(p::RATE, 10.0).with(p::FEEDBACK, 50.0).with(p::MIX, 1.0);
                if let Some(e) = engine {
                    s = s.with(p::COLOR_CUSTOM, 1.0).with(p::ENGINE, e).with(p::WOW, 100.0).with(p::FLUTTER, 100.0).with(p::INTERP, 2.0);
                }
                let mut c = ch(s, 2);
                let mut l = noise(12000, 1);
                let mut r = noise(12000, 2);
                for (a, b) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
                    c.process(&mut [a, b]);
                }
                assert!(l.iter().chain(r.iter()).all(|v| v.is_finite() && v.abs() < 8.0), "profile {profile} mode {mode} engine {engine:?}");
            }
        }
    }
}

#[test]
fn crossover_keeps_the_lows_dry_and_mono() {
    // Bass profile: crossover on at 200 Hz. A 40 Hz tone on the left only comes out on both sides at half.
    let s = Settings::default().with(p::PROFILE, 2.0).with(p::MIX, 1.0);
    let mut c = ch(s, 2);
    let n = 48000;
    let mut l: Vec<f32> = (0..n).map(|i| 0.5 * (std::f32::consts::TAU * 40.0 * i as f32 / SR).sin()).collect();
    let mut r = vec![0.0f32; n];
    for (a, b) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        c.process(&mut [a, b]);
    }
    let (rl, rr) = (rms(&l[24000..]), rms(&r[24000..]));
    let want = 0.25 / 2f64.sqrt();
    // level comp at mix 1 (Add law) is −3 dB
    let lc = 1.0 / 2f64.sqrt();
    assert!((rl / (want * lc) - 1.0).abs() < 0.1 && (rr / (want * lc) - 1.0).abs() < 0.1, "L {rl:.4} R {rr:.4}");
}

#[test]
fn level_comp_holds_loudness() {
    for mode in [0.0f32, 1.0, 4.0, 7.0, 8.0] {
        let s = Settings::default().with(p::MODE, mode).with(p::MIX, 1.0).with(p::PROFILE, 4.0);
        let mut c = ch(s, 1);
        let x = noise(96000, 5);
        let mut y = x.clone();
        for b in y.chunks_mut(256) {
            c.process(&mut [b]);
        }
        let db = 20.0 * (rms(&y[48000..]) / rms(&x[48000..])).log10();
        assert!(db.abs() < 2.0, "mode {mode}: {db:.2} dB");
    }
}

#[test]
fn mono_safe_dimension_keeps_the_highs_in_mono() {
    let hf = |safe: bool| {
        let s = Settings::default().with(p::MODE, 4.0).with(p::MIX, 1.0).with(p::MIX_LAW, 1.0).with(p::STEREO_CUSTOM, 1.0).with(p::MONO_SAFE, if safe { 1.0 } else { 0.0 }).with(p::WIDTH, 100.0);
        let mut c = ch(s, 2);
        let x = noise(48000, 9);
        let (mut l, mut r) = (x.clone(), x.clone());
        for (a, b) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            c.process(&mut [a, b]);
        }
        let m: Vec<f32> = l.iter().zip(&r).map(|(a, b)| 0.5 * (a + b)).collect();
        let mut hp = Svf::new();
        hp.set(SvfMode::HighPass, 1000.0, 0.707, 0.0, SR);
        let h: Vec<f32> = m.iter().map(|v| hp.process(*v)).collect();
        rms(&h[24000..])
    };
    let (safe, unsafe_) = (hf(true), hf(false));
    assert!(safe > 1.5 * unsafe_, "mono highs: safe {safe:.4}, not safe {unsafe_:.4}");
}

#[test]
fn structural_change_keeps_the_dry() {
    // a mode change dips only the wet: with mix 0 nothing moves
    let s = Settings::default().with(p::MIX, 0.0).with(p::LEVEL_COMP, 0.0);
    let mut c = ch(s, 1);
    let x = noise(9600, 3);
    let mut y = x.clone();
    let (a, b) = y.split_at_mut(4800);
    c.process(&mut [a]);
    c.set(c.settings().with(p::MODE, 4.0));
    c.process(&mut [b]);
    let err = x.iter().zip(&y).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(err < 1e-5, "{err}");
}
