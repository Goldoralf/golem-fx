// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §6) on a synthetic voice: Rosenberg glottal pulses through /a/ formants.

use super::*;

const SR: f32 = 48000.0;
const PI: f64 = std::f64::consts::PI;

/// A sung vowel at `f0` (Hz), `secs` long, optional vibrato (cents, Hz), peak ≈ 0.3.
fn vowel(f0: f64, secs: f64, vib_ct: f64, vib_hz: f64) -> Vec<f32> {
    let n = (secs * SR as f64) as usize;
    let sr = SR as f64;
    // glottal source: Rosenberg pulses (open 40 %, closing 16 %)
    let mut src = vec![0.0f64; n];
    let mut ph = 0.0f64;
    for i in 0..n {
        let f = f0 * 2f64.powf(vib_ct / 1200.0 * (2.0 * PI * vib_hz * i as f64 / sr).sin());
        ph += f / sr;
        let p = ph.fract();
        src[i] = if p < 0.4 {
            0.5 * (1.0 - (PI * p / 0.4).cos())
        } else if p < 0.56 {
            (PI * (p - 0.4) / 0.32).cos()
        } else {
            0.0
        };
    }
    // differentiate (lip radiation), then three formant resonators
    let mut y: Vec<f64> = (0..n).map(|i| src[i] - if i > 0 { src[i - 1] } else { 0.0 }).collect();
    for (fc, bw) in [(700.0, 110.0), (1220.0, 120.0), (2600.0, 160.0)] {
        let r = (-PI * bw / sr).exp();
        let c = 2.0 * r * (2.0 * PI * fc / sr).cos();
        let (mut y1, mut y2) = (0.0, 0.0);
        for v in y.iter_mut() {
            let o = *v + c * y1 - r * r * y2;
            y2 = y1;
            y1 = o;
            *v = o;
        }
    }
    let pk = y.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    y.iter().map(|v| (0.3 * v / pk) as f32).collect()
}

fn golem(set: &[(&str, f32)]) -> Golem {
    let mut g = Golem::new();
    for (k, v) in set {
        g.set_param(k, *v);
    }
    g.prepare(SR, 512, 1);
    g.reset();
    g
}

/// The doubles alone: the output minus the (untouched) dry.
fn doubles(g: &mut Golem, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for c in y.chunks_mut(256) {
        g.process(&mut [c]);
    }
    y.iter().zip(x).map(|(a, b)| a - b).collect()
}

fn rms_db(x: &[f32]) -> f64 {
    10.0 * (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64 + 1e-30).log10()
}

/// Fundamental by autocorrelation (the lag of the strongest peak in the range), with parabolic refinement.
fn pitch(x: &[f32], fmin: f64, fmax: f64) -> f64 {
    let sr = SR as f64;
    let (lo, hi) = ((sr / fmax) as usize, (sr / fmin) as usize);
    let n = x.len() - hi;
    let ac = |l: usize| (0..n).map(|i| x[i] as f64 * x[i + l] as f64).sum::<f64>();
    let vals: Vec<f64> = (lo..=hi).map(ac).collect();
    let k = (0..vals.len()).max_by(|&a, &b| vals[a].total_cmp(&vals[b])).unwrap();
    let off = if k > 0 && k + 1 < vals.len() {
        let (a, b, c) = (vals[k - 1], vals[k], vals[k + 1]);
        0.5 * (a - c) / (a - 2.0 * b + c)
    } else {
        0.0
    };
    sr / ((lo + k) as f64 + off)
}

#[test]
fn octave_down_is_an_octave() {
    for f0 in [140.0, 220.0, 387.0] {
        let x = vowel(f0, 1.5, 0.0, 0.0);
        let mut g = golem(&[("voice1", 0.0)]);
        let d = doubles(&mut g, &x);
        let tail = &d[(0.7 * SR) as usize..(1.4 * SR) as usize];
        let p = pitch(tail, f0 / 2.0 * 0.8, f0 / 2.0 * 1.25);
        let cents = 1200.0 * (p / (f0 / 2.0)).log2();
        assert!(cents.abs() < 5.0, "{f0} Hz: double at {p:.2} Hz ({cents:.1} ct)");
    }
}

#[test]
fn other_intervals_land() {
    for st in [-7.0f32, 12.0, -24.0] {
        let f0 = 220.0;
        let x = vowel(f0, 1.5, 0.0, 0.0);
        let mut g = golem(&[("shift", st), ("voice1", 0.0)]);
        let d = doubles(&mut g, &x);
        let tail = &d[(0.7 * SR) as usize..(1.4 * SR) as usize];
        let want = f0 * 2f64.powf(st as f64 / 12.0);
        let p = pitch(tail, want * 0.85, want * 1.18);
        let cents = 1200.0 * (p / want).log2();
        assert!(cents.abs() < 8.0, "{st} st: {p:.2} Hz vs {want:.2} ({cents:.1} ct)");
    }
}

#[test]
fn level_is_normalised() {
    let x = vowel(200.0, 1.5, 0.0, 0.0);
    let dry = rms_db(&x[(0.6 * SR) as usize..(1.4 * SR) as usize]);
    for (st, fm) in [(-12.0f32, 0.0f32), (-12.0, -4.0), (12.0, 0.0), (-7.0, 0.0), (-12.0, 4.0)] {
        let mut g = golem(&[("shift", st), ("formant", fm), ("voice1", 0.0), ("lpf", 16000.0), ("hpf", 20.0)]);
        let d = doubles(&mut g, &x);
        let lvl = rms_db(&d[(0.6 * SR) as usize..(1.4 * SR) as usize]) - dry;
        assert!(lvl.abs() < 2.5, "shift {st}, formant {fm}: double at {lvl:.2} dB re dry");
    }
}

#[test]
fn formant_moves_the_timbre() {
    // lowering the formants lowers the double's spectral centroid
    let x = vowel(160.0, 1.5, 0.0, 0.0);
    let centroid = |fm: f32| {
        let mut g = golem(&[("formant", fm), ("voice1", 0.0), ("lpf", 16000.0)]);
        let d = doubles(&mut g, &x);
        let s = &d[(0.6 * SR) as usize..(1.4 * SR) as usize];
        // zero-crossing rate as a cheap centroid proxy
        s.windows(2).filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0)).count() as f64
    };
    let (c0, cm) = (centroid(0.0), centroid(-4.0));
    assert!(cm < 0.9 * c0, "formant −4: {cm} vs {c0}");
}

#[test]
fn unvoiced_and_bleed_stay_out() {
    // white noise: never doubled
    let mut seed = 1u32;
    let noise: Vec<f32> = (0..(SR as usize))
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed as f32 / u32::MAX as f32 - 0.5) * 0.3
        })
        .collect();
    let mut g = golem(&[("voice1", 0.0)]);
    let d = doubles(&mut g, &noise);
    assert!(rms_db(&d) - rms_db(&noise) < -40.0, "noise doubled at {} dB", rms_db(&d) - rms_db(&noise));
    // a quiet pitched signal under the level gate (bleed): never doubled
    let quiet: Vec<f32> = vowel(220.0, 1.0, 0.0, 0.0).iter().map(|v| v * 0.003).collect();
    let mut g = golem(&[("voice1", 0.0), ("thresh", -40.0)]);
    let d = doubles(&mut g, &quiet);
    assert!(rms_db(&d) - rms_db(&quiet) < -60.0, "bleed doubled at {} dB", rms_db(&d) - rms_db(&quiet));
}

#[test]
fn follows_vibrato() {
    let f0 = 220.0;
    let x = vowel(f0, 2.0, 50.0, 6.0);
    let mut g = golem(&[("voice1", 0.0)]);
    let d = doubles(&mut g, &x);
    // the double's pitch stays an octave below the vowel's, both measured the same way over the same
    // window (autocorrelation under vibrato is not the mean pitch: compare like with like)
    let w = (0.6 * SR) as usize..(1.9 * SR) as usize;
    let p = pitch(&d[w.clone()], 90.0, 140.0);
    let px = pitch(&x[w], 180.0, 280.0);
    let cents = 1200.0 * (p / (px / 2.0)).log2();
    assert!(cents.abs() < 10.0, "vibrato: double at {p:.2} Hz ({cents:.1} ct)");
}

#[test]
fn deterministic() {
    let x = vowel(180.0, 1.0, 30.0, 5.0);
    let run = || {
        let mut g = golem(&[("humanize", 50.0), ("voice2", -10.0)]);
        let mut y = x.clone();
        g.process(&mut [&mut y[..]]);
        y
    };
    assert_eq!(run(), run());
}
