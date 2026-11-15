// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5, the rows that run on synthetic signals).

use crate::FreqShift;
use gfx_dsp::ssb::HilbertDesign;
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::Effect;
use std::f64::consts::PI;

const RATES: [f32; 4] = [44100.0, 48000.0, 96000.0, 192000.0];

fn fresh(sr: f32, ch: usize, settings: &[(&str, f32)]) -> FreqShift {
    let mut e = FreqShift::new();
    e.prepare(sr, 512, ch);
    for &(id, v) in settings {
        e.set_param(id, v);
    }
    e.reset();
    e
}

fn render(e: &mut FreqShift, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for c in y.chunks_mut(512) {
        e.process(&mut [c]);
    }
    y
}

fn render2(e: &mut FreqShift, l: &[f32], r: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let (mut a, mut b) = (l.to_vec(), r.to_vec());
    for (ca, cb) in a.chunks_mut(512).zip(b.chunks_mut(512)) {
        e.process(&mut [ca, cb]);
    }
    (a, b)
}

fn sine(n: usize, f: f64, amp: f32, sr: f32) -> Vec<f32> {
    (0..n).map(|i| amp * (2.0 * PI * f * i as f64 / sr as f64).sin() as f32).collect()
}

fn noise(n: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut r = gfx_dsp::lfo::Rng::new(seed);
    (0..n).map(|_| amp * r.bipolar()).collect()
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-300).log10()
}

/// Amplitude of the component at `f` in `y` (a unit sine reads 1.0): one DFT bin, Blackman–Harris.
fn tone(y: &[f32], f: f64, sr: f32) -> f64 {
    let n = y.len();
    let (mut re, mut im) = (0.0f64, 0.0f64);
    let (mut c, mut s) = (1.0f64, 0.0f64);
    let (dc, ds) = ((2.0 * PI * f / sr as f64).cos(), (2.0 * PI * f / sr as f64).sin());
    for (i, &v) in y.iter().enumerate() {
        let t = 2.0 * PI * i as f64 / n as f64;
        let w = 0.35875 - 0.48829 * t.cos() + 0.14128 * (2.0 * t).cos() - 0.01168 * (3.0 * t).cos();
        re += v as f64 * w * c;
        im += v as f64 * w * s;
        let nc = c * dc - s * ds;
        s = s * dc + c * ds;
        c = nc;
    }
    re.hypot(im) / (0.35875 * n as f64 * 0.5)
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

fn highpass4(x: &[f32], fc: f32, sr: f32) -> Vec<f32> {
    let mut a = Svf::new();
    let mut b = Svf::new();
    a.set(SvfMode::HighPass, fc, 0.541_196, 0.0, sr);
    b.set(SvfMode::HighPass, fc, 1.306_563, 0.0, sr);
    x.iter().map(|&v| b.process(a.process(v))).collect()
}

/// +100 Hz on a 1 kHz sine lands at 1,100 Hz; −100 Hz at 900 Hz (every rate).
#[test]
fn direction() {
    for sr in RATES {
        for (hz, want, img) in [(100.0, 1100.0, 900.0), (-100.0, 900.0, 1100.0)] {
            let mut e = fresh(sr, 1, &[("shift", hz as f32), ("mix", 100.0)]);
            let y = render(&mut e, &sine((1.5 * sr) as usize, 1000.0, 0.5, sr));
            let tail = &y[(0.5 * sr) as usize..];
            let (w, i) = (tone(tail, want, sr), tone(tail, img, sr));
            assert!((db(w / 0.5)).abs() < 0.1, "{sr} {hz}: wanted {:.2} dB", db(w / 0.5));
            assert!(db(i / w) < -80.0, "{sr} {hz}: image {:.1} dB", db(i / w));
        }
    }
}

/// Image rejection by DFT: sines 30 Hz – 15 kHz, shifts ±5, ±100, ±1,000 Hz, `zero_cut` on and off:
/// ≤ −84 dB at 44.1 / 48 kHz (8 + 8), ≤ −88 dB at 96 / 192 kHz (10 + 10).
#[test]
fn image_rejection() {
    for sr in RATES {
        let mut worst = -400.0f64;
        let bound = if sr <= 50000.0 { -84.0 } else { -88.0 };
        for f in [30.0, 82.0, 1000.0, 5000.0, 15000.0] {
            for s in [5.0, -5.0, 100.0, -100.0, 1000.0, -1000.0] {
                let want: f64 = f + s;
                if want < 20.0 {
                    continue;
                }
                let img = (f - s).abs();
                for cut in [1.0, 0.0] {
                    let mut e = fresh(sr, 1, &[("shift", s as f32), ("mix", 100.0), ("zero_cut", cut)]);
                    let y = render(&mut e, &sine((3.0 * sr) as usize, f, 0.5, sr));
                    let tail = &y[sr as usize..];
                    let r = db(tone(tail, img, sr) / tone(tail, want, sr));
                    worst = worst.max(r);
                    assert!(r <= bound, "{sr}: {f} Hz shifted {s} Hz (cut {cut}): image {r:.1} dB");
                }
            }
        }
        println!("{sr}: worst image {worst:.1} dB");
    }
}

/// Bass E (41.2 Hz) and B (30.9 Hz) shifted +3 Hz at 48 kHz: image ≤ −84 dB, level within 1 dB.
#[test]
fn low_end() {
    let sr = 48000.0;
    for f in [41.2, 30.9] {
        let mut e = fresh(sr, 1, &[("shift", 3.0), ("mix", 100.0)]);
        let y = render(&mut e, &sine((5.0 * sr) as usize, f, 0.5, sr));
        let tail = &y[sr as usize..];
        let (w, i) = (tone(tail, f + 3.0, sr), tone(tail, f - 3.0, sr));
        assert!(db(w / 0.5).abs() < 1.0, "{f}: level {:.2} dB", db(w / 0.5));
        println!("low end {f}: level {:.2} dB, image {:.1} dB", db(w / 0.5), db(i / w));
        assert!(db(i / w) <= -84.0, "{f}: image {:.1} dB", db(i / w));
    }
}

/// DC 0.1, shift +50 Hz: nothing at 50 Hz above −100 dBFS once settled. A 5 Hz sine at −20 dBFS,
/// shift +200 Hz: nothing between 150 and 250 Hz above −43 dBFS.
#[test]
fn dc_and_thump() {
    let sr = 48000.0;
    let mut e = fresh(sr, 1, &[("shift", 50.0), ("mix", 100.0)]);
    let y = render(&mut e, &vec![0.1f32; (4.0 * sr) as usize]);
    let tail = &y[(2.0 * sr) as usize..];
    let l = db(tone(tail, 50.0, sr));
    println!("DC → 50 Hz at {l:.1} dBFS");
    assert!(l < -100.0, "DC → 50 Hz at {l:.1} dBFS");
    let mut e = fresh(sr, 1, &[("shift", 200.0), ("mix", 100.0)]);
    let y = render(&mut e, &sine((4.0 * sr) as usize, 5.0, 0.1, sr));
    let tail = &y[(2.0 * sr) as usize..];
    let worst = (150..=250).map(|f| tone(tail, f as f64, sr)).fold(0.0f64, f64::max);
    println!("5 Hz thump → {:.1} dBFS", db(worst));
    assert!(db(worst) < -43.0, "5 Hz thump → {:.1} dBFS", db(worst));
}

/// `shift` 0, LFO 50 Hz deep at 2 Hz on a 440 Hz sine: the shift crosses zero with a flat envelope
/// (±0.1 dB) and no click.
#[test]
fn through_zero() {
    let sr = 48000.0;
    let mut e = fresh(sr, 1, &[("shift", 0.0), ("lfo_depth", 50.0), ("rate", 2.0), ("mix", 100.0)]);
    let y = render(&mut e, &sine((3.0 * sr) as usize, 440.0, 0.5, sr));
    let tail = &y[sr as usize..];
    let w = (0.01 * sr) as usize;
    let pk: Vec<f32> = tail.chunks(w).map(peak).collect();
    let (lo, hi) = pk.iter().fold((f32::MAX, 0.0f32), |(a, b), &v| (a.min(v), b.max(v)));
    println!("through zero: envelope {:.3} dB", db((hi / lo) as f64));
    assert!(db((hi / lo) as f64) < 0.2, "envelope {:.3} dB", db((hi / lo) as f64));
    let hf = peak(&highpass4(tail, 5000.0, sr)[w..]);
    assert!(db((hf / hi) as f64) < -60.0, "HF {:.1} dB", db((hf / hi) as f64));
}

/// 100 Hz shifted −150 Hz: the 50 Hz reflection ≤ −80 dB with `zero_cut`, at the input's level
/// without it. 170 Hz shifted −150 Hz: the 20 Hz result within 0.5 dB.
#[test]
fn zero_cut() {
    for sr in RATES {
        let mut e = fresh(sr, 1, &[("shift", -150.0), ("mix", 100.0)]);
        let y = render(&mut e, &sine((3.0 * sr) as usize, 100.0, 0.5, sr));
        let r = db(tone(&y[sr as usize..], 50.0, sr) / 0.5);
        println!("{sr}: reflection {r:.1} dB");
        assert!(r <= -80.0, "{sr}: reflection {r:.1} dB");
        let mut e = fresh(sr, 1, &[("shift", -150.0), ("mix", 100.0), ("zero_cut", 0.0)]);
        let y = render(&mut e, &sine((3.0 * sr) as usize, 100.0, 0.5, sr));
        let r = db(tone(&y[sr as usize..], 50.0, sr) / 0.5);
        assert!(r.abs() <= 0.5, "{sr}: reflection without the cut {r:.2} dB");
        let mut e = fresh(sr, 1, &[("shift", -150.0), ("mix", 100.0)]);
        let y = render(&mut e, &sine((4.0 * sr) as usize, 170.0, 0.5, sr));
        let r = db(tone(&y[(2.0 * sr) as usize..], 20.0, sr) / 0.5);
        assert!(r.abs() <= 0.5, "{sr}: 20 Hz result {r:.2} dB");
    }
}

/// After a shift history, `shift` back to 0: within 1 s the output is that of a fresh instance at
/// shift 0, within −80 dB (the phase parks at 0).
#[test]
fn parked() {
    let sr = 48000.0;
    let n = (3.0 * sr) as usize;
    let x = noise(n, 0.3, 5);
    let mut e = fresh(sr, 1, &[("shift", 37.3), ("mix", 50.0)]);
    let half = n / 3;
    let mut y = x.clone();
    for c in y[..half].chunks_mut(512) {
        e.process(&mut [c]);
    }
    e.set_param("shift", 0.0);
    for c in y[half..].chunks_mut(512) {
        e.process(&mut [c]);
    }
    let mut r = fresh(sr, 1, &[("shift", 0.0), ("mix", 50.0)]);
    let yr = render(&mut r, &x);
    let from = half + sr as usize;
    let d = y[from..].iter().zip(&yr[from..]).fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
    let rel = db((d / peak(&yr[from..])) as f64);
    println!("parked difference {rel:.1} dB");
    assert!(rel < -80.0, "parked difference {rel:.1} dB");
}

/// A sine at 0.47 fs shifted +2 kHz: its wrap past Nyquist ≤ −80 dBc with `zero_cut` (at the rates
/// where it wraps: 44.1 and 48 kHz).
#[test]
fn wrap_single_pass() {
    for sr in [44100.0, 48000.0] {
        let f = 0.47 * sr as f64;
        let mut e = fresh(sr, 1, &[("shift", 2000.0), ("mix", 100.0)]);
        let y = render(&mut e, &sine((2.0 * sr) as usize, f, 0.5, sr));
        let wrap = sr as f64 - (f + 2000.0);
        let r = db(tone(&y[sr as usize..], wrap, sr) / 0.5);
        println!("{sr}: wrap {r:.1} dBc");
        assert!(r <= -80.0, "{sr}: wrap {r:.1} dBc");
    }
}

/// Noise in a 3 ms loop at 100 %, shift +2 kHz: once the input stops, the band above fs / 2 − 2.5 kHz
/// falls below −80 dBFS within 5 s, with `zero_cut` on and off (nothing stays trapped under Nyquist).
#[test]
fn wrap_in_the_loop() {
    let sr = 48000.0;
    for cut in [1.0, 0.0] {
        let mut e = fresh(sr, 1, &[("shift", 2000.0), ("fbk", 100.0), ("time", 3.0), ("mix", 100.0), ("zero_cut", cut)]);
        let n = (10.0 * sr) as usize;
        let mut x = noise(n, 0.25, 9);
        x[n / 2..].iter_mut().for_each(|v| *v = 0.0);
        let y = render(&mut e, &x);
        let top = highpass4(&y, 0.5 * sr - 2500.0, sr);
        let p = db(peak(&top[n - (0.5 * sr) as usize..]) as f64);
        println!("cut {cut}: top band {p:.1} dBFS 5 s after the input");
        assert!(p < -80.0, "cut {cut}: top band {p:.1} dBFS 5 s after the input");
    }
}

/// `sideband` 0: both sidebands within 0.1 dB; −100: the mirror alone (image ≤ −80 dB).
#[test]
fn sideband() {
    let sr = 48000.0;
    let mut e = fresh(sr, 1, &[("shift", 200.0), ("mix", 100.0), ("sideband", 0.0)]);
    let y = render(&mut e, &sine((2.0 * sr) as usize, 1000.0, 0.5, sr));
    let t = &y[sr as usize..];
    let (u, l) = (tone(t, 1200.0, sr), tone(t, 800.0, sr));
    assert!(db(u / l).abs() < 0.1, "balance {:.2} dB", db(u / l));
    let mut e = fresh(sr, 1, &[("shift", 200.0), ("mix", 100.0), ("sideband", -100.0)]);
    let y = render(&mut e, &sine((2.0 * sr) as usize, 1000.0, 0.5, sr));
    let t = &y[sr as usize..];
    let (u, l) = (tone(t, 1200.0, sr), tone(t, 800.0, sr));
    assert!(db(u / l) < -80.0, "mirror: image {:.1} dB", db(u / l));
}

/// `dry_match` at shift 0, `time` 0, `fbk` 0 on noise: wet − matched dry ≤ −75 dB in every third-octave
/// band from 20 Hz to 20 kHz.
#[test]
fn dry_match_null() {
    use realfft::RealFftPlanner;
    let sr = 48000.0;
    let n = 1 << 18;
    let x = noise(n + sr as usize, 0.3, 21);
    let mut wet = fresh(sr, 1, &[("shift", 0.0), ("mix", 100.0), ("dry_match", 1.0)]);
    let mut dry = fresh(sr, 1, &[("shift", 0.0), ("mix", 0.0), ("dry_match", 1.0)]);
    let yw = render(&mut wet, &x);
    let yd = render(&mut dry, &x);
    let fft = RealFftPlanner::<f64>::new().plan_fft_forward(n);
    let spec = |v: Vec<f64>| {
        let mut b: Vec<f64> = v.iter().enumerate().map(|(i, s)| s * (0.5 - 0.5 * (2.0 * PI * i as f64 / n as f64).cos())).collect();
        let mut o = fft.make_output_vec();
        fft.process(&mut b, &mut o).unwrap();
        o.iter().map(|z| z.norm_sqr()).collect::<Vec<f64>>()
    };
    let from = sr as usize;
    let sd = spec(yd[from..from + n].iter().map(|&v| v as f64).collect());
    let se = spec(yw[from..from + n].iter().zip(&yd[from..]).map(|(&a, &b)| (a - b) as f64).collect());
    let mut f = 20.0f64;
    let mut worst = -400.0f64;
    while f < 20000.0 {
        let (a, b) = ((f / sr as f64 * n as f64) as usize, (f * 2f64.powf(1.0 / 3.0) / sr as f64 * n as f64) as usize);
        let (ed, ee): (f64, f64) = (sd[a..b].iter().sum(), se[a..b].iter().sum());
        worst = worst.max(10.0 * (ee / ed).log10());
        f *= 2f64.powf(1.0 / 3.0);
    }
    assert!(worst <= -75.0, "dry match null {worst:.1} dB");
    println!("dry match null: worst third-octave {worst:.1} dB");
}

/// `dry_match` at +4 Hz, mix 50 %, on a chord: the whole sound beats to silence (trough ≤ −40 dB).
#[test]
fn dry_match_beats() {
    let sr = 48000.0;
    let n = (3.0 * sr) as usize;
    let x: Vec<f32> = (0..n).map(|i| [110.0, 138.6, 164.8, 220.0].iter().map(|f| 0.1 * (2.0 * PI * f * i as f64 / sr as f64).sin() as f32).sum()).collect();
    let mut e = fresh(sr, 1, &[("shift", 4.0), ("mix", 50.0), ("dry_match", 1.0)]);
    let y = render(&mut e, &x);
    let t = &y[sr as usize..];
    let w = (0.0005 * sr) as usize;
    let pk = peak(t);
    let trough = t[..(0.25 * sr) as usize].windows(w).map(peak).fold(f32::MAX, f32::min);
    println!("dry match trough {:.1} dB", db((trough / pk) as f64));
    assert!(db((trough / pk) as f64) <= -40.0, "trough {:.1} dB", db((trough / pk) as f64));
}

/// A 1 kHz tone burst, `time` 100 ms, `fbk` 50 %, shift +10 Hz: repeat k arrives k · (100 ms + 1
/// sample) − 1 sample later (plus the pair's group delay at its frequency), at 0.5^(k−1) ± 0.2 dB,
/// moved up by k · 10 Hz.
#[test]
fn loop_repeats() {
    let sr = 48000.0;
    let n = (0.7 * sr) as usize;
    let bl = (0.02 * sr) as usize;
    let x: Vec<f32> = (0..n)
        .map(|i| if i < bl { (0.5 * (1.0 - (2.0 * PI * i as f64 / bl as f64).cos()) * (2.0 * PI * 1000.0 * i as f64 / sr as f64).sin()) as f32 * 0.5 } else { 0.0 })
        .collect();
    let mut e = fresh(sr, 1, &[("shift", 10.0), ("mix", 100.0), ("fbk", 50.0), ("time", 100.0)]);
    let y = render(&mut e, &x);
    let d = HilbertDesign::for_rate(sr as f64);
    // group delay of path A, twice (zero_cut), at f (samples)
    let gd = |f: f64| {
        let df = 0.05;
        let (a1, _) = d.response(f - df, sr as f64);
        let (a2, _) = d.response(f + df, sr as f64);
        let dphi = (a2.1 * a1.0 - a2.0 * a1.1).atan2(a2.0 * a1.0 + a2.1 * a1.1);
        2.0 * -dphi / (2.0 * PI * 2.0 * df) * sr as f64
    };
    let period = (0.1 * sr) as usize;
    let e0: f64 = x.iter().map(|&v| (v as f64).powi(2)).sum();
    let c0: f64 = x.iter().enumerate().map(|(i, &v)| i as f64 * (v as f64).powi(2)).sum::<f64>() / e0;
    let mut acc_gd = 0.0;
    for k in 1..=5usize {
        let f = 1000.0 + 10.0 * k as f64;
        // the input's frequency at pass j is 1000 + 10 (j − 1)
        acc_gd += gd(f - 10.0);
        let seg = &y[k * period - period / 4..k * period + period / 2];
        let off = (k * period - period / 4) as f64;
        let en: f64 = seg.iter().map(|&v| (v as f64).powi(2)).sum();
        let cen = seg.iter().enumerate().map(|(i, &v)| (off + i as f64) * (v as f64).powi(2)).sum::<f64>() / en;
        let expect = c0 + (k * period) as f64 + (k - 1) as f64 + acc_gd;
        println!("repeat {k}: at {cen:.2}, expected {expect:.2}");
        assert!((cen - expect).abs() < 2.0, "repeat {k}: at {cen:.1}, expected {expect:.1}");
        let lvl = 10.0 * (en / e0).log10();
        let want = 20.0 * 0.5f64.powi(k as i32 - 1).log10();
        assert!((lvl - want).abs() < 0.2, "repeat {k}: {lvl:.2} dB, expected {want:.2}");
        // energy-weighted instantaneous frequency, from the analytic signal
        let mut h = gfx_dsp::ssb::HilbertPair::new(&d);
        let z: Vec<(f64, f64)> = y[..k * period + period / 2].iter().map(|&v| h.analytic(v as f64)).collect();
        let (mut wsum, mut fsum) = (0.0, 0.0);
        for i in (k * period - period / 4 + 1)..(k * period + period / 2) {
            let (a, b) = (z[i], z[i - 1]);
            let m = a.0 * a.0 + a.1 * a.1;
            let dphi = (a.1 * b.0 - a.0 * b.1).atan2(a.0 * b.0 + a.1 * b.1);
            wsum += m;
            fsum += m * dphi * sr as f64 / (2.0 * PI);
        }
        let fm = fsum / wsum;
        assert!((fm - f).abs() < 1.0, "repeat {k}: at {fm:.2} Hz, expected {f}");
    }
}

/// Noise at −12 dBFS, 10 s at `fbk` 110 % in every loop, `time` 0 / 3 / 500 ms, shift 0 / ±7 Hz,
/// `zero_cut` on and off: output ≤ +12 dBFS, no NaN.
#[test]
fn stability_at_110() {
    let sr = 48000.0;
    let x = noise((10.0 * sr) as usize, 0.25 * 3f32.sqrt(), 13);
    let mut worst = 0.0f32;
    for lp in [0.0, 1.0] {
        for t in [0.0, 3.0, 500.0] {
            for s in [0.0, 7.0, -7.0] {
                for cut in [1.0, 0.0] {
                    let mut e = fresh(sr, 1, &[("fbk", 110.0), ("loop", lp), ("time", t), ("shift", s), ("zero_cut", cut), ("mix", 100.0)]);
                    let y = render(&mut e, &x);
                    assert!(y.iter().all(|v| v.is_finite()), "NaN: loop {lp} time {t} shift {s} cut {cut}");
                    let p = peak(&y);
                    worst = worst.max(p);
                    assert!(p <= 4.0, "loop {lp} time {t} shift {s} cut {cut}: peak {:.1} dBFS", db(p as f64));
                }
            }
        }
    }
    println!("stability: worst peak {:.1} dBFS", db(worst as f64));
}

/// After the input stops with `fbk` 100 %, shift ±50 Hz, `time` 3 ms, `zero_cut` on: the tail falls
/// below −80 dBFS within 30 s (the energy leaves by the band's ends).
#[test]
fn loop_drains() {
    let sr = 48000.0;
    for s in [50.0, -50.0] {
        let mut e = fresh(sr, 1, &[("fbk", 100.0), ("time", 3.0), ("shift", s), ("mix", 100.0)]);
        let n = (32.0 * sr) as usize;
        let mut x = noise(n, 0.25, 17);
        x[(2.0 * sr) as usize..].iter_mut().for_each(|v| *v = 0.0);
        let y = render(&mut e, &x);
        let p = db(peak(&y[n - (0.5 * sr) as usize..]) as f64);
        println!("shift {s}: tail {p:.1} dBFS after 30 s");
        assert!(p < -80.0, "shift {s}: tail {p:.1} dBFS after 30 s");
    }
}

/// `shift_r` +100 %, `stereo_phase` 0, equal inputs: L = R bit for bit from reset, and again within
/// 100 ms after `shift_r` moves away and back. −100 %: left up, right down by the same Hz.
#[test]
fn stereo_lock_and_opposite() {
    let sr = 48000.0;
    let n = (2.0 * sr) as usize;
    let x = noise(n, 0.3, 3);
    let mut e = fresh(sr, 2, &[("shift", 3.0), ("fbk", 40.0), ("time", 20.0)]);
    let (l, r) = render2(&mut e, &x, &x);
    assert_eq!(l, r);
    // away and back
    let a = n / 4;
    let (mut l, mut r) = (x.clone(), x.clone());
    let mut pos = 0;
    let mut step = |e: &mut FreqShift, to: usize, l: &mut [f32], r: &mut [f32]| {
        while pos < to {
            let end = (pos + 512).min(to);
            e.process(&mut [&mut l[pos..end], &mut r[pos..end]]);
            pos = end;
        }
    };
    step(&mut e, a, &mut l, &mut r);
    e.set_param("shift_r", 50.0);
    step(&mut e, 2 * a, &mut l, &mut r);
    assert!(l[2 * a - 512..2 * a] != r[2 * a - 512..2 * a]);
    e.set_param("shift_r", 100.0);
    step(&mut e, n, &mut l, &mut r);
    let back = 2 * a + (0.1 * sr) as usize;
    assert_eq!(l[back..], r[back..], "not L = R 100 ms after the lock");
    // opposite shifts
    let mut e = fresh(sr, 2, &[("shift", 150.0), ("shift_r", -100.0), ("mix", 100.0)]);
    let s = sine(n, 1000.0, 0.5, sr);
    let (l, r) = render2(&mut e, &s, &s);
    let (l, r) = (&l[sr as usize..], &r[sr as usize..]);
    assert!(tone(l, 1150.0, sr) > 1e4 * tone(l, 850.0, sr));
    assert!(tone(r, 850.0, sr) > 1e4 * tone(r, 1150.0, sr));
}

/// `time` glided 0 → 1 ms → 0 on a 1 kHz sine: no click (the read below one sample); at `time` 0 the
/// wet is undelayed.
#[test]
fn short_times() {
    let sr = 48000.0;
    let n = (1.5 * sr) as usize;
    let mut e = fresh(sr, 1, &[("shift", 0.0), ("mix", 100.0)]);
    let mut y = sine(n, 1000.0, 0.5, sr);
    let marks = [n / 3, 2 * n / 3];
    let mut pos = 0;
    for (k, &m) in marks.iter().chain([n].iter()).enumerate() {
        for c in y[pos..m].chunks_mut(512) {
            e.process(&mut [c]);
        }
        pos = m;
        if k == 0 {
            e.set_param("time", 1.0);
        } else if k == 1 {
            e.set_param("time", 0.0);
        }
    }
    let hf = highpass4(&y, 8000.0, sr);
    let p = peak(&hf[(0.2 * sr) as usize..]);
    assert!(db((p / peak(&y)) as f64) < -60.0, "glide HF {:.1} dB", db((p / peak(&y)) as f64));
    // impulse at time 0: the wet starts on sample 0
    let mut e = fresh(sr, 1, &[("shift", 0.0), ("mix", 100.0)]);
    let mut imp = vec![0.0f32; 64];
    imp[0] = 1.0;
    let y = render(&mut e, &imp);
    assert!(y[0] != 0.0, "no output on sample 0");
}

/// A 50 Hz-stepped sweep of `shift` 0 → 500 Hz (EXP): control-rate sidebands ≤ −80 dB.
#[test]
fn exp_sweep_is_clean() {
    let sr = 48000.0;
    let n = (3.0 * sr) as usize;
    let mut e = fresh(sr, 1, &[("shift", 0.0), ("mix", 100.0)]);
    let mut y = sine(n, 1000.0, 0.5, sr);
    let step = (0.02 * sr) as usize;
    let mut pos = 0;
    let mut hz = 0.0;
    while pos < n {
        let end = (pos + step).min(n);
        for c in y[pos..end].chunks_mut(512) {
            e.process(&mut [c]);
        }
        pos = end;
        if hz < 500.0 {
            hz += 50.0;
            e.set_param("shift", hz);
        }
    }
    // settled at +500 Hz: one line at 1.5 kHz, nothing else
    let t = &y[(1.5 * sr) as usize..];
    let main = tone(t, 1500.0, sr);
    for f in [1450.0, 1550.0, 1475.0, 1525.0, 500.0, 2000.0] {
        assert!(db(tone(t, f, sr) / main) < -80.0, "sideband at {f} Hz: {:.1} dB", db(tone(t, f, sr) / main));
    }
    // during the sweep (1,000 → 1,500 Hz): nothing far from the moving line
    let s = &y[(0.05 * sr) as usize..(0.25 * sr) as usize];
    let pk = peak(s) as f64;
    for f in [300.0, 2500.0, 3500.0, 5000.0] {
        assert!(db(tone(s, f, sr) / pk) < -80.0, "during the sweep, {f} Hz at {:.1} dB", db(tone(s, f, sr) / pk));
    }
}

/// Bypass with trails: the dry comes back at unity; without trails the wet is silenced too.
#[test]
fn bypass_and_trails() {
    let sr = 48000.0;
    let n = sr as usize;
    let x = noise(n, 0.3, 8);
    for trails in [1.0, 0.0] {
        let mut e = fresh(sr, 1, &[("fbk", 80.0), ("time", 50.0), ("trails", trails), ("bypass", 1.0)]);
        let y = render(&mut e, &x);
        let d = y[n / 2..].iter().zip(&x[n / 2..]).fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
        assert!(d < 1e-6, "bypassed: not the dry ({d})");
    }
    // trails on: the loop rings after bypass
    let mut e = fresh(sr, 1, &[("fbk", 80.0), ("time", 50.0), ("mix", 100.0)]);
    let mut y = x.clone();
    for c in y[..n / 2].chunks_mut(512) {
        e.process(&mut [c]);
    }
    e.set_param("bypass", 1.0);
    let mut z = vec![0.0f32; n / 2];
    for c in z.chunks_mut(512) {
        e.process(&mut [c]);
    }
    assert!(peak(&z[(0.1 * sr) as usize..(0.2 * sr) as usize]) > 0.01, "no trail");
}
