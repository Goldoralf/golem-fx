// SPDX-License-Identifier: GPL-3.0-or-later
//! The wah's own bench suite (README.md §6).

use super::*;

const SR: f32 = 48000.0;

fn wah(f: impl FnOnce(&mut Settings)) -> Wah {
    let mut w = Wah::new();
    w.prepare(SR, 4800, 1);
    let mut s = Settings { drive: 0.0, ..Settings::default() };
    f(&mut s);
    w.set(s);
    w.reset();
    w
}

fn gain_db(w: &mut Wah, f: f32) -> f32 {
    let n = 12000;
    let x: Vec<f32> = (0..n).map(|i| 0.01 * (std::f32::consts::TAU * f * i as f32 / SR).sin()).collect();
    let mut y = x.clone();
    for c in y.chunks_mut(480) {
        w.process(&mut [c]);
    }
    let e = |v: &[f32]| v[n / 2..].iter().map(|s| (*s as f64).powi(2)).sum::<f64>();
    (10.0 * (e(&y) / e(&x)).log10()) as f32
}

/// Peak frequency and gain of the response, found on a fine log grid.
fn peak(model: usize, pedal: f32) -> (f32, f32) {
    let mut best = (0.0, f32::MIN);
    let mut f = 150.0f32;
    while f < 4000.0 {
        let mut w = wah(|s| {
            s.model = model;
            s.wah = pedal;
        });
        let g = gain_db(&mut w, f);
        if g > best.1 {
            best = (f, g);
        }
        f *= 1.03;
    }
    best
}

#[test]
fn every_model_hits_its_measured_peak() {
    for (mi, m) in MODELS.iter().enumerate().take(8) {
        for pedal in [0.3f32, 0.7] {
            let w = m.law.apply(pedal);
            let (fc, _q, gpk, _fl) = lookup(m, w);
            if fc < 300.0 || fc < 2.0 * m.dc {
                continue; // under the model's own DC blocker (the tables exclude it, the pedal does not)
            }
            let (f, g) = peak(mi, pedal);
            assert!((f / fc - 1.0).abs() < 0.08, "{} at {pedal}: peak at {f:.0} Hz, table {fc:.0}", m.name);
            assert!((g - gpk).abs() < 2.0, "{} at {pedal}: peak {g:.1} dB, table {gpk:.1}", m.name);
        }
    }
}

#[test]
fn heel_is_low_on_every_model() {
    for (mi, m) in MODELS.iter().enumerate() {
        let lo = peak(mi, 0.0).0;
        let hi = peak(mi, 1.0).0;
        assert!(lo < hi, "{}: heel {lo:.0} Hz, toe {hi:.0} Hz", m.name);
    }
}

#[test]
fn each_model_keeps_its_own_pot_law() {
    // Selmer is linear, GCB-95 log: half-way, their positions in their tables differ
    let selmer = MODELS[5].law.apply(0.5);
    let gcb = MODELS[GCB95].law.apply(0.5);
    assert!((selmer - 0.5).abs() < 1e-6);
    assert!((gcb - 0.5).abs() > 0.2, "GCB-95 half-way at W = {gcb}");
}

#[test]
fn auto_engage_with_hysteresis() {
    let mut w = wah(|s| {
        s.auto_engage = true;
        s.engage_wait = 50.0;
        s.wah = 0.0;
    });
    // heel down for 100 ms: disengaged → the dry passes untouched
    let x: Vec<f32> = (0..9600).map(|i| 0.1 * (std::f32::consts::TAU * 1000.0 * i as f32 / SR).sin()).collect();
    let mut y = x.clone();
    for c in y.chunks_mut(480) {
        w.process(&mut [c]);
    }
    assert_eq!(&y[7200..], &x[7200..], "still engaged at the heel");
    // a move to 1 % (inside the hysteresis) keeps it off; 10 % engages
    let mut s = w.settings();
    s.wah = 0.04;
    w.set(s);
    let mut y = x.clone();
    for c in y.chunks_mut(480) {
        w.process(&mut [c]);
    }
    assert_eq!(&y[7200..], &x[7200..], "engaged inside the hysteresis");
    s.wah = 0.2;
    w.set(s);
    let mut y = x.clone();
    for c in y.chunks_mut(480) {
        w.process(&mut [c]);
    }
    assert!(y[7200..].iter().zip(&x[7200..]).any(|(a, b)| a != b), "did not engage");
}

#[test]
fn mix_zero_is_the_dry() {
    let mut w = wah(|s| s.mix = 0.0);
    let x: Vec<f32> = (0..4800).map(|i| ((i * 7919) % 1000) as f32 / 1000.0 - 0.5).collect();
    let mut y = x.clone();
    w.process(&mut [&mut y[..]]);
    assert_eq!(y, x);
}
