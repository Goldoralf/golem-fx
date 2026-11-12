// SPDX-License-Identifier: GPL-3.0-or-later
//! The tremolo's own bench suite (README.md §6).

use super::*;

const SR: f32 = 48000.0;

fn trem(f: impl FnOnce(&mut Settings)) -> Tremolo {
    let mut t = Tremolo::new();
    t.prepare(SR, 4800, 2);
    let mut s = Settings::default();
    f(&mut s);
    t.set(s);
    t.reset();
    t
}

fn run_dc(t: &mut Tremolo, n: usize, amp: f32) -> Vec<f32> {
    let mut y = vec![amp; n];
    for c in y.chunks_mut(480) {
        t.process(&mut [c]);
    }
    y
}

#[test]
fn clean_follows_calfs_law() {
    let mut t = trem(|s| {
        s.depth = 0.6;
        s.rate = 4.0;
    });
    let y = run_dc(&mut t, 48000, 0.5);
    let max = y.iter().cloned().fold(f32::MIN, f32::max);
    let min = y.iter().cloned().fold(f32::MAX, f32::min);
    assert!((max - 0.5).abs() < 1e-3, "top {max}");
    assert!((min - 0.5 * 0.4).abs() < 1e-3, "trough {min} (1 − depth)");
}

#[test]
fn lfo_shapes() {
    assert!((lfo_value(0.25, 0, 0.5, 0.01) - 1.0).abs() < 1e-6);
    assert!((lfo_value(0.75, 1, 0.5, 0.01) - 0.0).abs() < 1e-6);
    // duty leans the waveform: the top arrives earlier with a short first half
    assert!((lfo_value(0.1, 0, 0.2, 0.01) - 1.0).abs() < 1e-6);
    // square: flat top and bottom, edges inside the edge width
    assert_eq!(lfo_value(0.3, 2, 0.5, 0.01), 1.0);
    assert_eq!(lfo_value(0.8, 2, 0.5, 0.01), 0.0);
    let mid = lfo_value(0.005, 2, 0.5, 0.01);
    assert!(mid > 0.0 && mid < 1.0);
}

#[test]
fn parameter_changes_never_move_the_phase() {
    let mut t = trem(|s| s.rate = 5.0);
    run_dc(&mut t, 4800, 0.1);
    let p = t.phase();
    let mut s = t.settings();
    s.depth = 0.9;
    s.model = Model::Optical;
    s.wave = 2;
    t.set(s);
    assert_eq!(t.phase(), p);
}

#[test]
fn bias_distorts_near_the_trough_and_with_level() {
    // distortion of a 1 kHz tone, gain held at the trough: grows with input level
    let thd = |amp: f32| {
        let mut t = trem(|s| {
            s.model = Model::Bias;
            s.depth = 0.9;
            s.rate = 0.5;
            s.drive = 0.6;
        });
        // half a cycle in: the LFO sits at its trough around 0.75 of the cycle (t = 1.5 s)
        let n = 96000;
        let x: Vec<f32> = (0..n).map(|i| amp * (std::f32::consts::TAU * 1000.0 * i as f32 / SR).sin()).collect();
        let mut y = x.clone();
        for c in y.chunks_mut(480) {
            t.process(&mut [c]);
        }
        let seg = &y[70000..74000];
        let probe = |f: f32| {
            let w = std::f64::consts::TAU * f as f64 / SR as f64;
            let (mut re, mut im) = (0.0, 0.0);
            for (k, v) in seg.iter().enumerate() {
                let win = 0.5 - 0.5 * (std::f64::consts::TAU * k as f64 / seg.len() as f64).cos();
                re += *v as f64 * win * (w * k as f64).cos();
                im += *v as f64 * win * (w * k as f64).sin();
            }
            (re * re + im * im).sqrt()
        };
        20.0 * (probe(3000.0) / probe(1000.0)).log10()
    };
    let soft = thd(0.02);
    let loud = thd(0.5);
    assert!(loud > soft + 10.0, "soft {soft:.1} dB, loud {loud:.1} dB");
}

#[test]
fn harmonic_bands_move_in_antiphase() {
    let level = |f: f32, at: usize| {
        let mut t = trem(|s| {
            s.model = Model::Harmonic;
            s.blend = 1.0;
            s.depth = 1.0;
            s.rate = 1.0;
            s.drive = 0.0;
        });
        let n = 48000;
        let x: Vec<f32> = (0..n).map(|i| 0.1 * (std::f32::consts::TAU * f * i as f32 / SR).sin()).collect();
        let mut y = x.clone();
        for c in y.chunks_mut(480) {
            t.process(&mut [c]);
        }
        y[at - 480..at + 480].iter().fold(0.0f32, |m, v| m.max(v.abs()))
    };
    // at the LFO top (t = 0.25 s) lows are up and highs down; at the trough (0.75 s) the reverse
    let (lo_top, hi_top) = (level(100.0, 12000), level(3000.0, 12000));
    let (lo_bot, hi_bot) = (level(100.0, 36000), level(3000.0, 36000));
    assert!(lo_top > 2.0 * lo_bot, "lows {lo_top} / {lo_bot}");
    assert!(hi_bot > 2.0 * hi_top, "highs {hi_bot} / {hi_top}");
}

#[test]
fn stereo_phase_zero_is_mono() {
    let mut t = trem(|s| s.model = Model::Optical);
    let mut l = vec![0.3f32; 9600];
    let mut r = l.clone();
    t.process(&mut [&mut l[..], &mut r[..]]);
    assert_eq!(l, r);
}

#[test]
fn sync_follows_the_host() {
    let mut t = trem(|s| s.sync = 10); // 1/8
    t.set_transport(TransportInfo { tempo: Some(120.0), pos_beats: Some(0.25), playing: true });
    let mut y = vec![0.1f32; 1];
    t.process(&mut [&mut y[..]]);
    // an eighth note = 0.5 beat: beat 0.25 is half a cycle
    assert!((t.phase() - 0.5).abs() < 1e-6, "{}", t.phase());
}
