// SPDX-License-Identifier: GPL-3.0-or-later
//! The envelope filter's own bench suite (README.md §6).

use super::*;

const SR: f32 = 48000.0;

fn aw(f: impl FnOnce(&mut Settings)) -> Autowah {
    let mut a = Autowah::new();
    a.prepare(SR, 4800, 1);
    let mut s = Settings::default();
    f(&mut s);
    a.set(s);
    a.reset();
    a
}

/// Plucked notes: a 220 Hz tone with a fast attack and exponential decay, every 500 ms.
fn plucks(n: usize, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let t = (i % 24000) as f32 / SR;
            amp * (-t * 6.0).exp() * (std::f32::consts::TAU * 220.0 * i as f32 / SR).sin()
        })
        .collect()
}

#[test]
fn position_is_relative_to_the_playing_level() {
    for &(slow, lvl) in &[(-30.0f32, -24.0f32), (-10.0, -4.0)] {
        let p = Autowah::position_of(lvl, slow, 0.3);
        let p0 = Autowah::position_of(-24.0, -30.0, 0.3);
        assert!((p - p0).abs() < 1e-6, "{p} vs {p0}");
    }
}

#[test]
fn same_sweep_at_any_input_level() {
    // the envelope position's average over the last 3 s, at −30 and −10 dBFS
    let spectrum_centroid = |amp: f32| {
        let mut a = aw(|s| s.ftype = FilterType::BandPass);
        let x = plucks(48000 * 6, amp);
        let mut y = x.clone();
        for c in y.chunks_mut(480) {
            a.process(&mut [c]);
        }
        // energy ratio above / below 1 kHz in the output, last 3 s (level-normalised)
        let tail = &y[48000 * 3..];
        let mut hp = Svf::new();
        hp.set(SvfMode::HighPass, 1000.0, 0.707, 0.0, SR);
        let hi: f64 = tail.iter().map(|v| (hp.process(*v) as f64).powi(2)).sum();
        let all: f64 = tail.iter().map(|v| (*v as f64).powi(2)).sum();
        hi / all
    };
    let soft = spectrum_centroid(0.03);
    let loud = spectrum_centroid(0.3);
    assert!((soft / loud - 1.0).abs() < 0.15, "soft {soft:.3}, loud {loud:.3}");
}

#[test]
fn vowel_table_is_continuous() {
    let (a0, b0) = formants(0.0);
    assert!((a0 - VOWELS[0].0).abs() < 1e-2 && (b0 - VOWELS[0].1).abs() < 1e-2);
    let (a, b) = formants(1.0);
    assert!((a - VOWELS[4].0).abs() < 1e-3 && (b - VOWELS[4].1).abs() < 1e-3);
    let mut last = formants(0.0);
    for k in 1..=100 {
        let f = formants(k as f32 / 100.0);
        assert!((f.0 / last.0).ln().abs() < 0.05 && (f.1 / last.1).ln().abs() < 0.05);
        last = f;
    }
}

#[test]
fn stable_at_extreme_resonance() {
    for t in FilterType::ALL {
        let mut a = aw(|s| {
            s.ftype = t;
            s.peak = 20.0;
            s.four_poles = true;
            s.drive = 24.0;
            s.source = 1;
            s.rate = 10.0;
        });
        let mut rng = Rng::new(2);
        let mut y: Vec<f32> = (0..48000).map(|_| rng.bipolar()).collect();
        for c in y.chunks_mut(64) {
            a.process(&mut [c]);
        }
        assert!(y.iter().all(|v| v.is_finite() && v.abs() < 100.0), "{t:?}");
    }
}

#[test]
fn bass_keep_leaves_the_fundamental() {
    let mut a = aw(|s| {
        s.bass_keep = 150.0;
        s.ftype = FilterType::HighPass;
        s.low = 1500.0;
        s.high = 4000.0;
    });
    let x: Vec<f32> = (0..48000).map(|i| 0.3 * (std::f32::consts::TAU * 41.2 * i as f32 / SR).sin()).collect();
    let mut y = x.clone();
    for c in y.chunks_mut(480) {
        a.process(&mut [c]);
    }
    let e = |v: &[f32]| v[24000..].iter().map(|s| (*s as f64).powi(2)).sum::<f64>();
    let db = 10.0 * (e(&y) / e(&x)).log10();
    assert!(db.abs() < 0.5, "{db:.2} dB at 41 Hz");
}
