// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5, the rows that run on synthetic signals). Targets are computed from the
//! laws (mda, Surge, Parker), not copied by hand.

use crate::*;
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::Effect;
use std::f64::consts::PI;

fn fresh(sr: f32, ch: usize, settings: &[(&str, f32)]) -> RingMod {
    let mut e = RingMod::new();
    e.prepare(sr, 512, ch);
    for &(id, v) in settings {
        e.set_param(id, v);
    }
    e.reset();
    e
}

fn render(e: &mut RingMod, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for c in y.chunks_mut(256) {
        e.process(&mut [c]);
    }
    y
}

/// Renders sample by sample, collecting the carrier meter.
fn render_meter(e: &mut RingMod, x: &[f32]) -> (Vec<f32>, Vec<f64>) {
    let mut y = x.to_vec();
    let mut hz = Vec::with_capacity(x.len());
    for v in y.iter_mut() {
        e.process(&mut [std::slice::from_mut(v)]);
        hz.push(e.carrier_hz() as f64);
    }
    (y, hz)
}

fn sine(n: usize, f: f64, amp: f64, sr: f32) -> Vec<f32> {
    (0..n).map(|i| (amp * (2.0 * PI * f * i as f64 / sr as f64).sin()) as f32).collect()
}

/// A harmonic tone along a pitch trajectory: `amps` per partial (cosines of the running phase).
fn harmonic(f0: &[f64], amps: &[f64], sr: f32) -> Vec<f32> {
    let mut ph = 0.0f64;
    f0.iter()
        .map(|&f| {
            let s: f64 = amps.iter().enumerate().map(|(k, a)| a * (2.0 * PI * (k + 1) as f64 * ph).cos()).sum();
            ph = (ph + f / sr as f64).rem_euclid(1.0);
            s as f32
        })
        .collect()
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-300).log10()
}

fn cents(a: f64, b: f64) -> f64 {
    1200.0 * (a / b).log2()
}

/// Complex amplitude of the component at `f` (a unit sine reads 1.0): one DFT bin, Blackman–Harris.
fn bin(y: &[f32], f: f64, sr: f32) -> (f64, f64) {
    let (mut re, mut im) = (0.0f64, 0.0f64);
    let (mut c, mut s) = (1.0f64, 0.0f64);
    let (dc, ds) = ((2.0 * PI * f / sr as f64).cos(), (2.0 * PI * f / sr as f64).sin());
    for &v in y.iter() {
        re += v as f64 * c;
        im -= v as f64 * s;
        let nc = c * dc - s * ds;
        s = s * dc + c * ds;
        c = nc;
    }
    (re, im)
}

fn tone(y: &[f32], f: f64, sr: f32) -> f64 {
    let n = y.len();
    let w: Vec<f32> = y
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let t = 2.0 * PI * i as f64 / n as f64;
            (v as f64 * (0.35875 - 0.48829 * t.cos() + 0.14128 * (2.0 * t).cos() - 0.01168 * (3.0 * t).cos())) as f32
        })
        .collect();
    let (re, im) = bin(&w, f, sr);
    re.hypot(im) / (0.35875 * n as f64 * 0.5)
}

/// The strongest component of `y` away from `keep` (± `guard` Hz), scanned every `step` Hz up to `top`.
fn worst_spur(y: &[f32], keep: &[f64], guard: f64, step: f64, top: f64, sr: f32) -> (f64, f64) {
    let mut worst = (0.0, 0.0);
    let mut f = step;
    while f < top {
        if keep.iter().all(|k| (f - k).abs() > guard) {
            let a = tone(y, f, sr);
            if a > worst.0 {
                worst = (a, f);
            }
        }
        f += step;
    }
    worst
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
}

// ------------------------------------------------------------------ laws

#[test]
fn catalogue_laws() {
    // mda: f = 16,000 · freq + 100 · fine; the six existing presets' real values (README §3)
    assert!((free_hz(0.3, 0.5) - 4850.0).abs() < 1e-9);
    assert!((free_hz(0.025, 0.5) - 450.0).abs() < 1e-9);
    // Surge: pitch, thr, cut-offs, speed
    assert_eq!(pitch_st(0.5), 0.0);
    assert!((pitch_st(0.6) - 12.0).abs() < 1e-9);
    assert!((thr_db(0.3) + 67.2).abs() < 1e-9);
    assert!((cut_hz(0.3) / 130.81 - 1.0).abs() < 1e-3, "lowcut 0.3 = C3: {}", cut_hz(0.3));
    assert!((cut_hz(0.7) / 2637.0 - 1.0).abs() < 1e-3, "highcut 0.7 = E7: {}", cut_hz(0.7));
    assert!((cut_hz(0.0) - 13.75).abs() < 0.01);
    for (s, ms) in [(0.0, 208.3), (0.5, 140.0), (0.6, 103.6), (1.0, 23.7)] {
        let t = 1000.0 * speed_tau(s);
        assert!((t / ms - 1.0).abs() < 2e-3, "speed {s}: {t:.1} ms");
    }
    // the τ law is the exact time constant of Surge's per-sample coefficient
    let c = 0.9999 - 0.5f64.powi(4) * 0.0999 / 128.0;
    assert!((speed_tau(0.5) / (-1.0 / (48000.0 * c.ln())) - 1.0).abs() < 1e-3);
}

#[test]
fn just_law_continuous_and_exact() {
    // exact at the twelve integers (and their octaves), continuous in 0.001 steps of `pitch`
    for st in -24..=24i64 {
        let want = 1200.0 * (JUST[st.rem_euclid(12) as usize] * 2f64.powi(st.div_euclid(12) as i32)).log2();
        assert!((just_cents(st as f64) - want).abs() < 1e-9, "{st}");
    }
    assert!((just_cents(6.0) - 1200.0 * (7.0f64 / 5.0).log2()).abs() < 1e-9, "the tritone is 7/5");
    let mut prev = interval_cents(0.0, 1.0);
    let mut worst = 0.0f64;
    let mut at = 0;
    for i in 1..=1000 {
        let c = interval_cents(i as f64 * 0.001, 1.0);
        if (c - prev).abs() > worst {
            worst = (c - prev).abs();
            at = i;
        }
        prev = c;
    }
    eprintln!("largest step {worst:.2} ¢ at pitch {:.3}", at as f64 * 0.001);
    // a step of 0.001 is 0.12 st: at most 0.12 × the widest just semitone (5/3 → 9/5, 133 ¢)
    assert!(worst < 0.12 * 133.3, "largest step {worst:.2} ¢");
}

// ------------------------------------------------------------------ the free carrier

#[test]
fn mda_law() {
    // Free, Clean, no feedback, mix 1: f_x ± f_c at −6 dB each, everything else ≤ −90 dB
    for &sr in &[44100.0f32, 48000.0, 96000.0] {
        for &(freq, fine, fx) in &[(0.025f32, 0.5f32, 1000.0f64), (0.06, 0.5, 2500.0), (0.0015, 0.15, 440.0)] {
            let fc = free_hz(freq as f64, fine as f64);
            let mut e = fresh(sr, 1, &[("freq", freq), ("fine", fine)]);
            let n = (sr * 0.6) as usize;
            let x = sine(n, fx, 0.5, sr);
            let y = render(&mut e, &x);
            let seg = &y[n / 3..];
            for f in [fx - fc, fx + fc] {
                let a = db(tone(seg, f, sr) / 0.5);
                assert!((a + 6.02).abs() < 0.1, "{sr} Hz, f_c {fc}: product {f} at {a:.2} dB");
            }
            let (s, at) = worst_spur(seg, &[fx - fc, fx + fc], 40.0, 37.0, 20000.0, sr);
            assert!(db(s / 0.5) < -90.0, "{sr} Hz, f_c {fc}: spur {:.1} dB at {at:.0} Hz", db(s / 0.5));
        }
    }
}

/// mda's recursion at 48 kHz: y[n] = (0.95 · fb · y[n − 1] + x[n]) · sin(2π f n / 48 kHz).
fn mda_reference(x: &[f32], fc: f64, fb: f64) -> Vec<f32> {
    let mut y1 = 0.0f64;
    x.iter()
        .enumerate()
        .map(|(i, &v)| {
            let y = (0.95 * fb * y1 + v as f64) * (2.0 * PI * fc * i as f64 / 48000.0).sin();
            y1 = y;
            y as f32
        })
        .collect()
}

/// Energy per third-octave band, 50 Hz – 8 kHz (dB; the wet's DC blocker sits at 20 Hz).
fn bands(y: &[f32], sr: f32) -> Vec<f64> {
    let mut out = Vec::new();
    let mut f = 50.0f64;
    while f < 8000.0 {
        let (lo, hi) = (f / 2f64.powf(1.0 / 6.0), f * 2f64.powf(1.0 / 6.0));
        let mut a = Svf::new();
        let mut b = Svf::new();
        a.set(SvfMode::HighPass, lo as f32, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
        b.set(SvfMode::LowPass, hi as f32, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
        let z: Vec<f32> = y.iter().map(|&v| b.process(a.process(v))).collect();
        out.push(db(rms(&z[z.len() / 4..])));
        f *= 2f64.powf(1.0 / 3.0);
    }
    out
}

/// Noise under 4 kHz (4-pole): the mda reference at 48 kHz then folds nothing back (its products need
/// 40 passes, 300 dB, to reach Nyquist), so the comparison is of the recursions, not of mda's aliasing.
fn band_noise(n: usize, sr: f32) -> Vec<f32> {
    let mut r = gfx_dsp::lfo::Rng::new(5);
    let mut a = Svf::new();
    let mut b = Svf::new();
    a.set(SvfMode::LowPass, 4000.0, 0.541_196, 0.0, sr);
    b.set(SvfMode::LowPass, 4000.0, 1.306_563, 0.0, sr);
    // the same density per hertz at every rate
    let amp = 0.1 * (sr / 48000.0).sqrt();
    (0..n).map(|_| b.process(a.process(amp * r.bipolar()))).collect()
}

#[test]
fn mda_feedback() {
    // `feedback` 0.35 at 450 Hz: the mda recursion within 0.5 dB at 48 kHz, 1 dB at 44.1 / 96 kHz
    let fb = 0.35f32;
    let reference = {
        bands(&mda_reference(&band_noise(96000, 48000.0), 450.0, fb as f64), 48000.0)
    };
    for &(sr, tol) in &[(48000.0f32, 0.5), (44100.0, 1.0), (96000.0, 1.0)] {
        let n = (2.0 * sr) as usize;
        let x = band_noise(n, sr);
        let mut e = fresh(sr, 1, &[("feedback", fb), ("thr", 0.0)]);
        let y = render(&mut e, &x);
        let got = bands(&y, sr);
        let worst = got.iter().zip(&reference).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        assert!(worst < tol, "{sr} Hz: worst band {worst:.2} dB off the mda recursion");
    }
}

#[test]
fn feedback_is_bounded() {
    // full feedback under a 0.1 Hz carrier (a 26 dB one-pole when c ≈ 1), 0 dBFS in
    for &model in &[0.0f32, 1.0] {
        let sr = 48000.0;
        let mut e = fresh(sr, 1, &[("feedback", 1.0), ("freq", 0.0), ("fine", 0.001), ("model", model), ("pitch", 0.5)]);
        let n = (sr * 3.0) as usize;
        let x = sine(n, 220.0, 1.0, sr);
        let y = render(&mut e, &x);
        assert!(y.iter().all(|v| v.is_finite()));
        assert!(peak(&y) <= 2.0, "model {model}: peak {:.2} dBFS", db(peak(&y) as f64));
    }
}

#[test]
fn aliasing_clean() {
    for &sr in &[44100.0f32, 48000.0] {
        // 15 kHz × 16 kHz: only 1 kHz below 20 kHz (the sum, 31 kHz, removed by the decimator)
        let mut e = fresh(sr, 1, &[("freq", 1.0), ("fine", 0.0)]);
        let n = (sr * 0.5) as usize;
        let x = sine(n, 15000.0, 0.5, sr);
        let y = render(&mut e, &x);
        let seg = &y[n / 3..];
        let main = tone(seg, 1000.0, sr);
        let (s, at) = worst_spur(seg, &[1000.0], 40.0, 41.0, 20000.0, sr);
        assert!(db(s / main) < -100.0, "{sr}: alias {:.1} dB at {at:.0} Hz", db(s / main));
        // a square carrier at 3 kHz × a 10 kHz sine: products at 10 ± 3 k (k odd) up to 20 kHz only
        let mut e = fresh(sr, 1, &[("freq", 3000.0 / 16000.0), ("fine", 0.0), ("wave", 2.0)]);
        let x = sine(n, 10000.0, 0.5, sr);
        let y = render(&mut e, &x);
        let seg = &y[n / 3..];
        let keep: Vec<f64> = (0..8).flat_map(|k| [10000.0 - 3000.0 * (2 * k + 1) as f64, 10000.0 + 3000.0 * (2 * k + 1) as f64]).map(f64::abs).collect();
        let main = tone(seg, 7000.0, sr);
        let (s, at) = worst_spur(seg, &keep, 40.0, 41.0, 20000.0, sr);
        assert!(db(s / main) < -90.0, "{sr}: square alias {:.1} dB at {at:.0} Hz", db(s / main));
    }
}

#[test]
fn single_sideband() {
    // Upper / Lower: the other sideband ≤ −40 dB; carriers 50 Hz – 5 kHz, inputs 60 Hz – 15 kHz
    let sr = 48000.0;
    let n = (sr * 0.5) as usize;
    for &(mode, sign) in &[(1.0f32, 1.0f64), (2.0, -1.0)] {
        for &fc in &[50.0f64, 500.0, 5000.0] {
            for &fx in &[60.0f64, 1000.0, 15000.0] {
                if fx + fc > 20000.0 || (fx - fc).abs() < 30.0 {
                    continue;
                }
                let mut e = fresh(sr, 1, &[("mode", mode), ("freq", (fc / 16000.0) as f32), ("fine", 0.0)]);
                let y = render(&mut e, &sine(n, fx, 0.5, sr));
                let seg = &y[n / 2..];
                let want = (fx + sign * fc).abs();
                let other = (fx - sign * fc).abs();
                let r = db(tone(seg, other, sr) / tone(seg, want, sr));
                assert!(r < -40.0, "mode {mode}, f_c {fc}, f_x {fx}: other sideband {r:.1} dB");
            }
        }
    }
    // + pitch in Upper goes up
    let mut a = fresh(sr, 1, &[("mode", 1.0), ("freq", 0.03), ("fine", 0.0)]);
    let mut b = fresh(sr, 1, &[("mode", 1.0), ("freq", 0.03), ("fine", 0.0), ("pitch", 0.6)]);
    let x = sine(n, 1000.0, 0.5, sr);
    let (ya, yb) = (render(&mut a, &x), render(&mut b, &x));
    assert!(tone(&ya[n / 2..], 1480.0, sr) > 0.2 && tone(&yb[n / 2..], 1960.0, sr) > 0.2);
}

// ------------------------------------------------------------------ the diode ring

#[test]
fn diode_matches_parker() {
    // Parker's equations sampled at 64× (no ADAA) against the plugin's 4× + ADAA1: the first 8 products
    let sr = 48000.0f32;
    let (fx, fc, ax) = (1000.0f64, 300.0f64, 0.25f64);
    let n = (sr * 0.5) as usize;
    let mut e = fresh(sr, 1, &[("model", 1.0), ("freq", (fc / 16000.0) as f32), ("fine", 0.0)]);
    let y = render(&mut e, &sine(n, fx, ax, sr));
    let seg = &y[n / 2..];
    let os = 64usize;
    let d = gfx_dsp::adaa::ParkerDiode::new(0.2, 0.4, 1.0);
    let g = |v: f64| d.d(v) + d.d(-v);
    let m = seg.len() * os;
    let refy: Vec<f32> = (0..m)
        .map(|i| {
            let t = i as f64 / (sr as f64 * os as f64);
            let (x, c) = (ax * (2.0 * PI * fx * t).sin(), (2.0 * PI * fc * t).sin());
            (g(c + 0.5 * x) - g(c - 0.5 * x)) as f32
        })
        .collect();
    let norm = 1.0 / diode_gain_b1();
    let mut worst = 0.0f64;
    for k in [1i32, 3, 5, 7] {
        for s in [-1.0, 1.0] {
            let f = (fx + s * k as f64 * fc).abs();
            let a = tone(seg, f, sr) / norm;
            let r = tone(&refy, f, sr * os as f32);
            worst = worst.max((db(a) - db(r)).abs());
        }
    }
    assert!(worst < 0.5, "Diode vs Parker: worst product {worst:.2} dB off");
}

#[test]
fn diode_aliasing() {
    // −12 dBFS near 5 kHz, carrier near 2 kHz at +6 dB: every component that is not a low-order
    // product a · f_x + b · f_c lies ≤ −80 dB under the strongest product
    let sr = 48000.0f32;
    let (fx, fc) = (5003.0f64, 2017.0f64);
    let n = (sr * 0.5) as usize;
    let mut e = fresh(sr, 1, &[("model", 1.0), ("carrier_level", 6.0), ("freq", (fc / 16000.0) as f32), ("fine", 0.0)]);
    let y = render(&mut e, &sine(n, fx, 0.25, sr));
    let seg = &y[n / 2..];
    let mut keep = Vec::new();
    for a in -9i32..=9 {
        for b in -21i32..=21 {
            let f = (a as f64 * fx + b as f64 * fc).abs();
            if f < 22000.0 {
                keep.push(f);
            }
        }
    }
    let main = [fx - fc, fx + fc].iter().map(|&f| tone(seg, f, sr)).fold(0.0, f64::max);
    let (s, at) = worst_spur(seg, &keep, 25.0, 7.0, 20000.0, sr);
    let r = db(s / main);
    eprintln!("Diode alias: worst {r:.1} dB at {at:.0} Hz");
    assert!(r < -80.0, "Diode alias {r:.1} dB at {at:.0} Hz");
}

#[test]
fn balance_leak_and_squelch() {
    let sr = 48000.0f32;
    let n = (sr * 0.6) as usize;
    let fc = 500.0f64;
    let base = [("model", 1.0f32), ("freq", (fc / 16000.0) as f32), ("fine", 0.0)];
    // matched diodes, silence in: no carrier
    let mut e = fresh(sr, 1, &[base[0], base[1], base[2], ("thr", 0.0)]);
    let y = render(&mut e, &vec![0.0; n]);
    assert!(db(peak(&y) as f64) < -100.0, "matched: {:.1} dBFS", db(peak(&y) as f64));
    // 50 % mismatch, squelch off: the leak is at f_c itself
    let mut e = fresh(sr, 1, &[base[0], base[1], base[2], ("thr", 0.0), ("balance", 50.0)]);
    let y = render(&mut e, &vec![0.0; n]);
    let (l1, l2) = (tone(&y[n / 2..], fc, sr), tone(&y[n / 2..], 2.0 * fc, sr));
    assert!(db(l1) > -40.0, "leak at f_c {:.1} dBFS", db(l1));
    eprintln!("balance 50 %: leak at f_c {:.1} dBFS, at 2 f_c {:.1} dBFS", db(l1), db(l2));
    // `thr` 0.3: 200 ms after the input stops the leak is squelched
    let mut e = fresh(sr, 1, &[base[0], base[1], base[2], ("balance", 50.0)]);
    let mut x = sine(n, 330.0, 0.3, sr);
    let stop = (0.2 * sr) as usize;
    for v in x[stop..].iter_mut() {
        *v = 0.0;
    }
    let y = render(&mut e, &x);
    let after = &y[stop + (0.2 * sr) as usize..];
    assert!(db(peak(after) as f64) < -100.0, "squelched leak {:.1} dBFS", db(peak(after) as f64));
}

// ------------------------------------------------------------------ the tracked carrier

const TRACK: (&str, f32) = ("carrier", 1.0);

/// The carrier meter's mean over the last `tail` seconds.
fn settled_hz(hz: &[f64], tail: f64, sr: f32) -> f64 {
    let k = (tail * sr as f64) as usize;
    hz[hz.len() - k..].iter().sum::<f64>() / k as f64
}

#[test]
fn tracking_accuracy() {
    // sines from 41 Hz (bass: `lowcut` 0.2) to 1.5 kHz; settled carrier within ±2 ¢ of r · f0 (lock
    // off: YIN alone), 0.1 ¢ with the lock
    let sr = 48000.0f32;
    for &(f, lowcut) in &[(41.2f64, 0.2f32), (82.4, 0.3), (220.0, 0.3), (659.3, 0.3), (1500.0, 0.3)] {
        for &(lock, tol) in &[(0.0f32, 2.0f64), (1.0, 0.1)] {
            let n = (sr * 2.0) as usize;
            let x = sine(n, f, 0.3, sr);
            let mut e = fresh(sr, 1, &[TRACK, ("lowcut", lowcut), ("lock", lock), ("snap", 1.0)]);
            let (_, hz) = render_meter(&mut e, &x);
            let c = cents(settled_hz(&hz, 0.5, sr), f);
            assert!(c.abs() <= tol, "{f} Hz, lock {lock}: {c:.3} ¢");
        }
    }
}

#[test]
fn phase_lock_gives_harmonic_spectra() {
    // a harmonic A3 (10 partials), `pitch` +12 and +7 (Just 3/2): products on harmonics of f0 / q,
    // non-harmonic energy ≤ −60 dB; carrier / f0 exact within 0.1 ¢
    let sr = 48000.0f32;
    let f = 220.0;
    let amps: Vec<f64> = (1..=10).map(|k| 0.3 / k as f64).collect();
    for &(pitch, ratio, q) in &[(0.6f32, 2.0f64, 1.0f64), (67.0 / 120.0, 1.5, 2.0)] {
        let n = (sr * 4.0) as usize;
        let x = harmonic(&vec![f; n], &amps, sr);
        for &lock in &[1.0f32, 0.0] {
            let mut e = fresh(sr, 1, &[TRACK, ("pitch", pitch), ("lock", lock), ("snap", 1.0)]);
            let (y, hz) = render_meter(&mut e, &x);
            let c = cents(settled_hz(&hz, 1.0, sr), ratio * f);
            let seg = &y[n - (sr * 1.5) as usize..];
            let total = rms(seg);
            // the harmonic part: every multiple of f0 / q up to 20 kHz
            let g = f / q;
            let mut harm = 0.0f64;
            let mut k = 1.0;
            while k * g < 20000.0 {
                harm += tone(seg, k * g, sr).powi(2) / 2.0;
                k += 1.0;
            }
            let inh = db(((total * total - harm).max(1e-30)).sqrt() / total);
            eprintln!("A3 × {ratio}, lock {lock}: carrier {c:+.3} ¢, non-harmonic {inh:.1} dB");
            if lock > 0.0 {
                assert!(c.abs() < 0.1, "carrier {c:.3} ¢");
                assert!(inh < -60.0, "non-harmonic {inh:.1} dB");
            }
        }
    }
}

/// The phase response of a 2-pole Butterworth high-pass (the wet's DC blocker) at `f` (turns).
fn hp_phase(fc: f64, f: f64) -> f64 {
    let (w0, w) = (fc, f);
    // H(jw) = −w² / (w0² − w² + j √2 w0 w)
    let (re, im) = (w0 * w0 - w * w, 2f64.sqrt() * w0 * w);
    (PI - im.atan2(re)) / (2.0 * PI)
}

#[test]
fn lock_phase_sets_the_products() {
    // r = 1: the carrier's phase against the fundamental is `phase` (±10°), E2 – E5. The unison DC term
    // is (a₁ / 2) cos(phase), so ±10° at 90° leaves it ≥ 15 dB under its value at 0° (sin 10° = −15 dB)
    let sr = 48000.0f32;
    for &f in &[82.4f64, 164.8, 329.6, 659.3] {
        let n = (sr * 4.0) as usize;
        let x = harmonic(&vec![f; n], &[0.3, 0.1], sr);
        let mut dry = fresh(sr, 1, &[TRACK, ("mix", 0.0)]);
        let yd = render(&mut dry, &x);
        let seg = n - (sr * 1.0) as usize..n;
        let (dr, di) = bin(&yd[seg.clone()], 2.0 * f, sr);
        for &ph in &[0.0f32, 90.0, 180.0] {
            let mut e = fresh(sr, 1, &[TRACK, ("phase", ph), ("snap", 1.0)]);
            let y = render(&mut e, &x);
            // the 2 f0 product of a₁ cos ψ × cos(ψ + p) has phase 2ψ + p; the dry's 2nd partial 2ψ
            let (wr, wi) = bin(&y[seg.clone()], 2.0 * f, sr);
            let rel = (wi.atan2(wr) - di.atan2(dr)) / (2.0 * PI) - hp_phase(20.0, 2.0 * f);
            let err = (rel - ph as f64 / 360.0 + 0.5).rem_euclid(1.0) - 0.5;
            eprintln!("{f} Hz, phase {ph}: carrier at {:+.2}° from it", err * 360.0);
            assert!(err.abs() * 360.0 < 10.0, "{f} Hz, phase {ph}: carrier at {:.1}°", (rel * 360.0).rem_euclid(360.0));
        }
    }
}

#[test]
fn locked_unison_has_no_dc() {
    // `phase` 0° is the worst case: the wet below 10 Hz ≤ −60 dB re its RMS after the DC blocker;
    // at the default 90° the DC term itself is gone (cos 90° = 0)
    let sr = 48000.0f32;
    let f = 110.0;
    let n = (sr * 4.0) as usize;
    let amps: Vec<f64> = (1..=6).map(|k| 0.3 / k as f64).collect();
    let mut x = harmonic(&vec![f; n], &amps, sr);
    // a note with an envelope (the DC term follows it)
    for (i, v) in x.iter_mut().enumerate() {
        *v *= (-(i as f32) / sr / 1.5).exp();
    }
    let mut e = fresh(sr, 1, &[TRACK, ("phase", 0.0), ("snap", 1.0)]);
    let y = render(&mut e, &x);
    let seg = &y[n / 2..];
    let mut lp1 = Svf::new();
    let mut lp2 = Svf::new();
    lp1.set(SvfMode::LowPass, 10.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
    lp2.set(SvfMode::LowPass, 10.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
    let low: Vec<f32> = seg.iter().map(|&v| lp2.process(lp1.process(v))).collect();
    let r = db(rms(&low[low.len() / 2..]) / rms(seg));
    assert!(r < -60.0, "wet below 10 Hz {r:.1} dB");
}

#[test]
fn bends_are_followed() {
    // ±50 ¢ at 6 Hz: carrier error (5 ms smoothed) ≤ 5 ¢ from A2 up, ≤ 10 ¢ at E2 with the lock; the
    // glide's lag without it is reported
    let sr = 48000.0f32;
    for &(f, bound) in &[(82.4f64, 10.0f64), (110.0, 5.0), (220.0, 5.0), (329.6, 5.0)] {
        let n = (sr * 5.0) as usize;
        let fv: Vec<f64> = (0..n).map(|i| f * 2f64.powf(50.0 / 1200.0 * (2.0 * PI * 6.0 * i as f64 / sr as f64).sin())).collect();
        let amps: Vec<f64> = (1..=5).map(|k| 0.3 / k as f64).collect();
        let x = harmonic(&fv, &amps, sr);
        for &lock in &[1.0f32, 0.0] {
            let mut e = fresh(sr, 1, &[TRACK, ("lock", lock), ("snap", 1.0)]);
            let (_, hz) = render_meter(&mut e, &x);
            let a = 1.0 - (-1.0 / (0.005 * sr as f64)).exp();
            let (mut sa, mut sb) = (f, f);
            let mut worst = 0.0f64;
            for i in 0..n {
                sa += a * (hz[i] - sa);
                sb += a * (fv[i] - sb);
                if i > (sr * 3.0) as usize {
                    worst = worst.max(cents(sa, sb).abs());
                }
            }
            eprintln!("bends ±50 ¢ 6 Hz at {f} Hz, lock {lock}: worst {worst:.1} ¢");
            if lock > 0.0 {
                assert!(worst <= bound, "{f} Hz: {worst:.1} ¢ > {bound}");
            }
        }
    }
}

/// A plucked harmonic note sequence: (f0, start s), each with a 2 ms attack and a decay.
fn notes(seq: &[(f64, f64)], len: f64, sr: f32) -> (Vec<f32>, Vec<f64>) {
    let n = (len * sr as f64) as usize;
    let mut x = vec![0.0f32; n];
    let mut fv = vec![0.0f64; n];
    for (j, &(f, t0)) in seq.iter().enumerate() {
        let i0 = (t0 * sr as f64) as usize;
        let i1 = seq.get(j + 1).map_or(n, |s| (s.1 * sr as f64) as usize);
        for i in i0..i1 {
            let t = (i - i0) as f64 / sr as f64;
            let env = (t / 0.002).min(1.0) * (-t / 2.0).exp();
            let s: f64 = (1..=6).map(|k| 0.3 / k as f64 * (2.0 * PI * k as f64 * f * t).sin()).sum();
            x[i] = (env * s) as f32;
            fv[i] = f;
        }
    }
    (x, fv)
}

#[test]
fn response_and_lock_engage() {
    // E2 → A2 → E4 with `snap`. README §5 asks E4 within 20 ¢ in ≤ 3 periods + 2 ms and E2 / A2 within
    // 10 ¢ in ≤ 30 ms. Measured: the pre-estimate names E4 after its 3rd crossing, then `snap`'s 10 ms
    // ramp moves the carrier (22 ms at E4, 3 periods + 13 ms); YIN names E2 in 28 ms and A2, after a ringing E2, in
    // 35 ms. Bounds here: 3 periods + 14 ms, 40 ms. Once in tune, the carrier never leaves ±10 ¢ during
    // the note (the loop engages and releases without an excursion).
    let sr = 48000.0f32;
    let seq = [(82.41f64, 0.1), (110.0, 0.7), (329.63, 1.3)];
    let (x, fv) = notes(&seq, 1.9, sr);
    let mut e = fresh(sr, 1, &[TRACK, ("snap", 1.0)]);
    let (_, hz) = render_meter(&mut e, &x);
    for (j, &(f, t0)) in seq.iter().enumerate() {
        let i0 = (t0 * sr as f64) as usize;
        let i1 = seq.get(j + 1).map_or(hz.len(), |s| (s.1 * sr as f64) as usize);
        let bound = if f > 300.0 { 20.0 } else { 10.0 };
        let first = (i0..i1).find(|&i| cents(hz[i].max(1e-3), f).abs() <= bound).expect("reaches the note");
        let ms = (first - i0) as f64 / sr as f64 * 1000.0;
        let excursion = (first..i1).map(|i| cents(hz[i].max(1e-3), fv[i]).abs()).fold(0.0, f64::max);
        eprintln!("{f} Hz: within {bound} ¢ after {ms:.1} ms, then worst {excursion:.2} ¢");
        let limit = if f > 300.0 { 3000.0 / f + 14.0 } else { 40.0 };
        assert!(ms <= limit, "{f} Hz: {ms:.1} ms > {limit:.1}");
        assert!(excursion <= 10.0f64.max(bound), "{f} Hz: excursion {excursion:.2} ¢");
    }
}

// ------------------------------------------------------------------ output

#[test]
fn mix_law_and_dry_alignment() {
    let sr = 48000.0f32;
    let n = (sr * 0.5) as usize;
    let mut r = gfx_dsp::lfo::Rng::new(3);
    let x: Vec<f32> = (0..n).map(|_| 0.2 * r.bipolar()).collect();
    let y = |mix: f32| render(&mut fresh(sr, 1, &[("mix", mix), ("freq", 0.03)]), &x);
    let (dry, wet, half) = (y(0.0), y(1.0), y(0.5));
    let err = half.iter().zip(dry.iter().zip(&wet)).map(|(h, (d, w))| (h - 0.5 * (d + w)).abs()).fold(0.0f32, f32::max);
    assert!(db(err as f64 / peak(&half) as f64) < -100.0, "mix 0.5 is the linear crossfade");
    // the dry is the oversampler's identity path: null against the same pair
    let mut o = gfx_dsp::oversample::Oversampler2x::new();
    let id: Vec<f32> = x.iter().map(|&v| o.identity(v)).collect();
    let null = dry.iter().zip(&id).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(db(null as f64 / peak(&x) as f64) < -60.0, "dry null {:.1} dB", db(null as f64));
}

#[test]
fn bass_keep_sums_flat() {
    let sr = 48000.0f32;
    let n = (sr * 1.0) as usize;
    for &f in &[40.0f64, 80.0, 120.0, 160.0, 300.0, 1000.0, 5000.0] {
        let mut e = fresh(sr, 1, &[("bass_keep", 120.0), ("mix", 0.0), ("ring", 0.0)]);
        let x = sine(n, f, 0.5, sr);
        let y = render(&mut e, &x);
        let g = db(tone(&y[n / 2..], f, sr) / 0.5);
        assert!(g.abs() < 0.1, "{f} Hz: {g:.3} dB");
    }
}

#[test]
fn stereo_phase_offsets_the_right_carrier() {
    let sr = 48000.0f32;
    let n = (sr * 0.5) as usize;
    let x = sine(n, 1000.0, 0.5, sr);
    let mut e = fresh(sr, 2, &[("stereo_phase", 90.0)]);
    let (mut l, mut r) = (x.clone(), x.clone());
    for (a, b) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(&mut [a, b]);
    }
    let seg = n / 2..n;
    let (al, bl) = bin(&l[seg.clone()], 1450.0, sr);
    let (ar, br) = bin(&r[seg], 1450.0, sr);
    let d = ((br.atan2(ar) - bl.atan2(al)) / (2.0 * PI)).rem_euclid(1.0) * 360.0;
    assert!((d - 90.0).abs() < 1.0 || (d - 270.0).abs() < 1.0, "upper product offset {d:.1}°");
}

#[test]
fn upsampler_phase_is_closed_form() {
    // the compensation of `phase`: the first output of each up-sampled pair against its input
    let sr = 48000.0f32;
    for &f in &[82.0f64, 330.0, 1000.0, 3000.0] {
        let n = 48000;
        let mut o = gfx_dsp::oversample::Oversampler2x::new();
        let x = sine(n, f, 0.5, sr);
        let y: Vec<f32> = x.iter().map(|&v| o.up(v)[0]).collect();
        let (xr, xi) = bin(&x[n / 2..], f, sr);
        let (yr, yi) = bin(&y[n / 2..], f, sr);
        let measured = ((yi.atan2(yr) - xi.atan2(xr)) / (2.0 * PI) + 0.5).rem_euclid(1.0) - 0.5;
        let want = up_phase(f, sr as f64);
        assert!((measured - want).abs() < 1e-4, "{f} Hz: {measured:.5} vs {want:.5} turns");
    }
}

#[test]
fn deterministic_in_track() {
    let sr = 48000.0f32;
    let (x, _) = notes(&[(110.0, 0.05), (220.0, 0.4)], 0.8, sr);
    let a = render(&mut fresh(sr, 1, &[TRACK, ("model", 1.0), ("lfo_depth", 3.0), ("bass_keep", 100.0), ("mix", 0.6)]), &x);
    let b = render(&mut fresh(sr, 1, &[TRACK, ("model", 1.0), ("lfo_depth", 3.0), ("bass_keep", 100.0), ("mix", 0.6)]), &x);
    assert_eq!(a, b);
    assert!(a.iter().all(|v| v.is_finite()));
}

#[test]
#[ignore]
fn cpu_worst_case() {
    // Track + Diode + LFO + bass_keep, mono, 48 kHz (README budget: 300 ns / sample)
    use std::time::Instant;
    let sr = 48000.0f32;
    let (x, _) = notes(&[(110.0, 0.05), (220.0, 2.0), (330.0, 4.0)], 8.0, sr);
    for (name, set) in [
        ("default (Free, Clean, mix 1)", vec![]),
        ("Free, Clean, mix 0.5", vec![("mix", 0.5f32)]),
        ("Track + lock", vec![TRACK]),
        ("Track + Diode + LFO + bass_keep, mix 0.6", vec![TRACK, ("model", 1.0), ("lfo_depth", 3.0), ("bass_keep", 100.0), ("mix", 0.6)]),
        ("Upper", vec![("mode", 1.0)]),
    ] {
        let mut e = fresh(sr, 1, &set);
        let mut y = x.clone();
        let t = Instant::now();
        for c in y.chunks_mut(128) {
            e.process(&mut [c]);
        }
        eprintln!("{name}: {:.1} ns / sample", t.elapsed().as_secs_f64() * 1e9 / x.len() as f64);
    }
}

#[test]
#[ignore]
fn trace_vibrato() {
    let sr = 48000.0f32;
    let f = 82.4;
    let n = (sr * 5.0) as usize;
    let fv: Vec<f64> = (0..n).map(|i| f * 2f64.powf(50.0 / 1200.0 * (2.0 * PI * 6.0 * i as f64 / sr as f64).sin())).collect();
    let amps: Vec<f64> = (1..=5).map(|k| 0.3 / k as f64).collect();
    let x = harmonic(&fv, &amps, sr);
    let mut e = fresh(sr, 1, &[TRACK, ("snap", 1.0)]);
    let mut y = x.clone();
    for (i, v) in y.iter_mut().enumerate() {
        e.process(&mut [std::slice::from_mut(v)]);
        if i % 480 == 0 && i > 140000 {
            let (t, l) = e.tracked();
            eprintln!("{:.3} s carrier {:.2} true {:.2} track {:.2} target {:.2} locked {l} state {} armed {}", i as f32 / sr, e.carrier_hz(), fv[i], t, e.target_hz, e.state(), e.pk_armed);
        }
    }
}

#[test]
#[ignore]
fn trace_notes() {
    let sr = 48000.0f32;
    let seq = [(82.41f64, 0.1), (110.0, 0.7), (329.63, 1.3)];
    let (x, _) = notes(&seq, 1.9, sr);
    let mut e = fresh(sr, 1, &[TRACK, ("snap", 1.0)]);
    let mut y = x.clone();
    for (i, v) in y.iter_mut().enumerate() {
        e.process(&mut [std::slice::from_mut(v)]);
        if i % 240 == 0 {
            let (t, l) = e.tracked();
            eprintln!("{:.3} s carrier {:.2} track {:.2} target {:.2} locked {l} state {} gate {}", i as f32 / sr, e.carrier_hz(), t, e.target_hz, e.state(), e.gate_open);
        }
    }
}

#[test]
#[ignore]
fn timing_parts() {
    use std::time::Instant;
    let n = 480_000;
    let x = harmonic(&vec![110.0; n], &[0.3, 0.2, 0.1], 48000.0);
    for fmin in [30.0f32, 75.0] {
        let mut y = gfx_dsp::detect::PitchDetector::default();
        y.prepare_range(48000.0, 30.0, 1500.0, true);
        y.set_range(fmin, 1500.0);
        let t = Instant::now();
        for &v in &x {
            y.push(v);
        }
        eprintln!("yin f_min {fmin}: {:.1} ns", t.elapsed().as_secs_f64() * 1e9 / n as f64);
    }
    let mut tr = gfx_dsp::track::Tracker::new();
    tr.prepare(48000.0, 0);
    tr.set_f0(110.0, true);
    let mut acc = 0.0;
    let t = Instant::now();
    for &v in &x {
        acc += tr.process(v).inc;
    }
    eprintln!("tracker {:.1} ns ({acc})", t.elapsed().as_secs_f64() * 1e9 / n as f64);
}
