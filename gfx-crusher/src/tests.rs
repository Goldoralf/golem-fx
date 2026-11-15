// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5). The reference for aliasing is the ideal converter: the input sampled
//! at each instant, held, band-limited at fs / 2 — its components are at |f0 + m·R| with amplitude
//! A·|sinc(f / R)|. Anything else in the output is foreign.

use super::*;
use realfft::RealFftPlanner;
use std::f64::consts::PI;

const SR: f32 = 48000.0;

fn crusher(set: &[(&str, f32)], sr: f32, channels: usize) -> Crusher {
    let mut c = Crusher::new();
    for (k, v) in set {
        c.set_param(k, *v);
    }
    c.prepare(sr, 512, channels);
    c.reset();
    c
}

fn run(c: &mut Crusher, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for b in y.chunks_mut(512) {
        c.process(&mut [b]);
    }
    y
}

fn sine(f: f64, amp: f64, n: usize, sr: f32) -> Vec<f32> {
    (0..n).map(|i| (amp * (2.0 * PI * f * i as f64 / sr as f64).sin()) as f32).collect()
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

/// 4-term Blackman-Harris window (side lobes −92 dB).
fn bh(n: usize) -> Vec<f64> {
    let a = [0.35875, 0.48829, 0.14128, 0.01168];
    (0..n)
        .map(|i| {
            let t = 2.0 * PI * i as f64 / n as f64;
            a[0] - a[1] * t.cos() + a[2] * (2.0 * t).cos() - a[3] * (3.0 * t).cos()
        })
        .collect()
}

/// Amplitude of the component at `f` (windowed projection, no scalloping).
fn amp_at(y: &[f32], f: f64, sr: f32) -> f64 {
    let w = bh(y.len());
    let ws: f64 = w.iter().sum();
    let om = 2.0 * PI * f / sr as f64;
    let (mut c, mut s) = (0.0, 0.0);
    for (i, v) in y.iter().enumerate() {
        let a = *v as f64 * w[i];
        c += a * (om * i as f64).cos();
        s += a * (om * i as f64).sin();
    }
    2.0 * c.hypot(s) / ws
}

/// Windowed magnitude spectrum (amplitude per bin, a sine of amplitude A reads ≈ A at its bin).
fn spectrum(y: &[f32]) -> Vec<f64> {
    let n = y.len();
    let w = bh(n);
    let ws: f64 = w.iter().sum();
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);
    let mut buf: Vec<f64> = y.iter().zip(&w).map(|(v, w)| *v as f64 * w).collect();
    let mut out = fft.make_output_vec();
    fft.process(&mut buf, &mut out).unwrap();
    out.iter().map(|c| 2.0 * c.norm() / ws).collect()
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-30).log10()
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
}

/// The predicted components below `limit`: (frequency, ideal amplitude re A).
fn predicted(f0: f64, r: f64, limit: f64, avg: bool) -> Vec<(f64, f64)> {
    let mut v = Vec::new();
    let mmax = (limit / r).ceil() as i64 + 2;
    for m in -mmax..=mmax {
        let f = (f0 + m as f64 * r).abs();
        if f < limit {
            let mut a = sinc(f / r).abs();
            if avg {
                a *= sinc(f0 / r).abs();
            }
            v.push((f, a));
        }
    }
    v
}

/// The worst foreign component (dB re the peak bin) below and above 0.4 fs, the predicted ones
/// masked (±8 bins).
fn foreign(y: &[f32], f0: f64, r: f64, sr: f32) -> (f64, f64) {
    let sp = spectrum(y);
    let n = y.len();
    let bin = sr as f64 / n as f64;
    let peak = sp.iter().cloned().fold(0.0, f64::max);
    let pred = predicted(f0, r, sr as f64 * 0.6, false);
    let (mut lo, mut hi) = (f64::MIN, f64::MIN);
    for (k, a) in sp.iter().enumerate().skip(8) {
        let f = k as f64 * bin;
        // the BLEP's transition band folds 0.5 – 0.6 fs onto 0.4 – 0.5 fs: those images' mirrors too
        let near = pred.iter().any(|(p, _)| (p - f).abs() < 8.0 * bin || (sr as f64 - p - f).abs() < 8.0 * bin);
        if near {
            continue;
        }
        let l = db(*a) - db(peak);
        if f < 0.4 * sr as f64 {
            lo = lo.max(l);
        } else {
            hi = hi.max(l);
        }
    }
    (lo, hi)
}

#[test]
fn ideal_model_residue() {
    let n = 1 << 16;
    let warm = 4096;
    for model in [model::RAW, model::AVERAGE] {
        let mut worst = (f64::MIN, f64::MIN, 0.0f64);
        for r in [3000.0, 8000.0, 26040.0, 37300.0] {
            for f0 in [97.0, 440.0, 1234.5, 2950.0, 7000.0, 11000.0, 15900.0] {
                let mut c = crusher(&[("quant", 16.0), ("rate", r as f32), ("tails", 2.0), ("model", model as f32)], SR, 1);
                let x = sine(f0, 0.5, n + warm, SR);
                let y = run(&mut c, &x);
                let y = &y[warm..];
                // the box nulls a partial near a multiple of R (Average, f0 ≈ 2R: −44 dB): the 16-bit
                // floor then stands "re peak" — measured only where the reference peak is ≥ −30 dB
                let top = predicted(f0, r, 0.5 * SR as f64, model == model::AVERAGE).iter().map(|p| p.1).fold(0.0, f64::max);
                if top < 0.03 {
                    continue;
                }
                let (lo, hi) = foreign(y, f0, r, SR);
                worst.0 = worst.0.max(lo);
                worst.1 = worst.1.max(hi);
                if model == model::RAW {
                    // the components themselves, below 0.35 fs, where they stand above −60 dB
                    let pred = predicted(f0, r, 0.35 * SR as f64, false);
                    for (k, (f, a)) in pred.iter().enumerate() {
                        let close = pred.iter().enumerate().any(|(j, (g, _))| j != k && (g - f).abs() < 12.0);
                        if *a < 1e-3 || close || *f < 5.0 {
                            continue;
                        }
                        let got = amp_at(y, *f, SR) / 0.5;
                        let e = (db(got) - db(*a)).abs();
                        worst.2 = worst.2.max(e);
                        assert!(e <= 0.5, "R {r}, f0 {f0}: component {f} Hz {:.2} dB vs {:.2} dB", db(got), db(*a));
                    }
                }
            }
        }
        eprintln!("{}: foreign {:.1} dB below 0.4 fs, {:.1} dB above; components within {:.2} dB", MODELS[model], worst.0, worst.1, worst.2);
        if model == model::RAW {
            assert!(worst.0 <= -80.0 && worst.1 <= -40.0, "{worst:?}");
        } else {
            assert!(worst.0 <= -60.0, "{worst:?}");
        }
    }
}

#[test]
fn dry_alignment_is_the_kernels_delay() {
    let c = crusher(&[], SR, 1);
    let k = Kernel::get();
    assert!((c.dry_align() - 2.0 * k.delay).abs() < 0.01, "{}", c.dry_align());
    eprintln!("kernel delay {:.3} samples, the dry's {:.3}", k.delay, c.dry_align());
}

#[test]
fn off_is_transparent() {
    for sr in [44100.0f32, 48000.0, 96000.0] {
        let x: Vec<f32> = (0..sr as usize).map(|i| (0.3 * (i as f64 * 0.0123).sin() + 0.2 * (i as f64 * 0.311).sin()) as f32).collect();
        let mut c = crusher(&[("rate", 48000.0), ("quant", 16.0), ("tails", 2.0)], sr, 1);
        let y = run(&mut c, &x);
        let worst = x.iter().zip(&y).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(db(worst as f64) <= -96.0 + 6.0, "{sr}: {} dBFS", db(worst as f64));
        // latency 0: the error is the quantiser's, not a delay
        assert!(worst <= 2f32.powi(-15) * 0.5 + 1e-7, "{worst}");
        let mut c = crusher(&[("rate", 48000.0), ("mix", 0.0)], sr, 1);
        assert_eq!(run(&mut c, &x), x, "{sr}: mix 0 is the dry, bit for bit");
    }
}

#[test]
fn rate_glides_never_steps() {
    // rate_shift 0 → −72 st by EXP steps at 50 Hz (128 values): R moves every sample, never jumps
    let mut c = crusher(&[("rate", 48000.0), ("quant", 16.0)], SR, 1);
    let x = sine(1000.0, 0.5, 4 * SR as usize, SR);
    let mut y = x.clone();
    let mut rs = Vec::new();
    let mut step = 0;
    for (b, blk) in y.chunks_mut(32).enumerate() {
        if b % 30 == 0 && step <= 127 {
            c.set_param("rate_shift", -72.0 * step as f32 / 127.0);
            step += 1;
        }
        for s in blk.chunks_mut(1) {
            c.process(&mut [s]);
            rs.push(c.r_now);
        }
    }
    let mut worst: f64 = 0.0;
    for w in rs.windows(2) {
        worst = worst.max((w[1] / w[0]).log2().abs() * 1200.0);
    }
    // a 7-bit step is 57 cents: smoothed, no sample moves R by more than a few cents
    assert!(worst < 5.0, "largest R step {worst:.2} cents per sample");
    assert!((rs.last().unwrap() - 48000.0 * 2f64.powf(-6.0)).abs() < 1.0);
    assert!(y.iter().all(|v| v.is_finite()));
}

#[test]
fn rate_independence() {
    let mut levels: Vec<Vec<f64>> = Vec::new();
    let freqs = [1000.0, 7000.0, 9000.0];
    for sr in [44100.0f32, 48000.0, 96000.0] {
        let n = sr as usize;
        let mut c = crusher(&[("rate", 8000.0), ("quant", 16.0), ("tails", 2.0)], sr, 1);
        let y = run(&mut c, &sine(1000.0, 0.5, n + 4096, sr));
        let y = &y[4096..];
        let mut l = Vec::new();
        for f in freqs {
            // the peak sits on the predicted frequency (±1 Hz)
            let best = (-20..=20).map(|d| (d, amp_at(y, f + d as f64 * 0.25, sr))).fold((0, 0.0), |m, v| if v.1 > m.1 { v } else { m });
            assert!((best.0 as f64 * 0.25).abs() <= 1.0, "{sr}: {f} off by {}", best.0 as f64 * 0.25);
            l.push(db(amp_at(y, f, sr)));
        }
        levels.push(l);
    }
    for k in 0..freqs.len() {
        let lo = levels.iter().map(|l| l[k]).fold(f64::MAX, f64::min);
        let hi = levels.iter().map(|l| l[k]).fold(f64::MIN, f64::max);
        assert!(hi - lo <= 1.0, "{} Hz: {:?}", freqs[k], levels);
    }
}

#[test]
fn quantiser_shapes_on_silence() {
    let x = vec![0.0f32; 4800];
    let step = 2f32.powi(-7);
    // Tread: silence stays silent
    let mut c = crusher(&[("tails", 2.0)], SR, 1);
    assert!(run(&mut c, &x).iter().all(|v| *v == 0.0));
    // Riser: ½ step on silence with tails Raw, 0 with Clean
    let mut c = crusher(&[("quantizer", 1.0), ("tails", 2.0), ("rate", 48000.0)], SR, 1);
    let y = run(&mut c, &x);
    assert!(y[100..].iter().all(|v| (v.abs() - 0.5 * step).abs() < 1e-7), "{}", y[200]);
    let mut c = crusher(&[("quantizer", 1.0), ("tails", 0.0)], SR, 1);
    assert!(run(&mut c, &x).iter().all(|v| *v == 0.0));
}

/// MDA Degrade's quantiser and hold (mono source on both inputs, no post filter, no clip, `nonlin`
/// neutral): bits = 4 + ⌊12q⌋, hold for ⌊e^(18·|r − 0.5|)⌋ host samples.
fn mda(x: &[f32], q: f64, r: f64) -> Vec<f32> {
    let g1 = 2f64.powi(2 + (12.0 * q) as i32);
    let tn = (18.0 * (r - 0.5).abs()).exp() as usize;
    let mut y = vec![0.0f32; x.len()];
    let mut held = 0.0;
    for i in 0..x.len() {
        if i % tn.max(1) == 0 {
            let b0 = 2.0 * x[i] as f64;
            held = (b0 * g1).trunc() / (2.0 * g1);
        }
        y[i] = held as f32;
    }
    y
}

#[test]
fn mda_conversion() {
    // same level set and dead zone
    let mut adc = gfx_dsp::converter::Adc::default();
    for q in [0.35f64, 0.5, 0.65] {
        let bits = 4.0 + (12.0 * q).floor();
        adc.set(Format::Linear, Shape::Truncate, bits, 0.0);
        let g1 = 2f64.powi(2 + (12.0 * q) as i32);
        for i in 0..20000 {
            let x = -0.999 + 1.998 * i as f64 / 20000.0;
            assert_eq!(adc.convert(x).level, (2.0 * x * g1).trunc() / (2.0 * g1), "q {q}, x {x}");
        }
    }
    // the presets converted faithfully: same hold period, the same components below 10 kHz
    for (q, r) in [(0.35f64, 0.25f64), (0.4, 0.3), (0.65, 0.45)] {
        let bits = 4.0 + (12.0 * q).floor();
        let tn = (18.0 * (r - 0.5f64).abs()).exp().floor();
        let rate = 48000.0 / tn;
        let f0 = 1234.5;
        let n = 1 << 16;
        let x = sine(f0, 0.5, n + 4096, SR);
        let mut c = crusher(&[("quant", bits as f32), ("quantizer", 2.0), ("rate", rate as f32), ("tails", 2.0)], SR, 1);
        let ours = run(&mut c, &x);
        let theirs = mda(&x, q, r);
        let mut worst: f64 = 0.0;
        for (f, a) in predicted(f0, rate, 10000.0, false) {
            if a < 0.03 || f < 20.0 {
                continue;
            }
            let o = db(amp_at(&ours[4096..], f, SR));
            let t = db(amp_at(&theirs[4096..], f, SR));
            worst = worst.max((o - t).abs());
        }
        assert!(worst <= 1.5, "q {q}, r {r} (R {rate:.0} Hz): {worst:.2} dB");
    }
}

/// A note decaying 60 dB in 2 s from `peak` (dBFS).
fn decaying(f: f64, peak: f64, secs: f64, sr: f32) -> Vec<f32> {
    let n = (secs * sr as f64) as usize;
    (0..n)
        .map(|i| {
            let t = i as f64 / sr as f64;
            let a = 10f64.powf((peak - 30.0 * t) / 20.0);
            (a * (2.0 * PI * f * t).sin()) as f32
        })
        .collect()
}

#[test]
fn tails_window() {
    for bits in [4.0f32, 8.0, 12.0] {
        for f in [440.0, 82.0] {
            let s = 2f64.powf(1.0 - bits as f64);
            let peak = db(s) + 20.0;
            let x = decaying(f, peak, 2.0, SR);
            let mut c = crusher(&[("quant", bits), ("rate", 48000.0)], SR, 1);
            let mut gs = Vec::new();
            let mut y = x.clone();
            for b in y.chunks_mut(1) {
                c.process(&mut [b]);
                gs.push(c.ch[0].g);
            }
            // monotone once the note is crushed: no modulation at the note's period
            let k0 = gs.iter().position(|g| *g == 0.0).expect("crushed");
            for w in gs[k0..].windows(2) {
                assert!(w[1] >= w[0] - 1e-12, "{bits} bits, {f} Hz: g falls {} → {}", w[0], w[1]);
            }
            // the window's middle (g = ½) at s·√2, within 1 dB; the level is the note's peak over the
            // detector's 25 ms (what a sliding maximum measures), so only the 20 ms smoothing lags
            let k = k0 + gs[k0..].iter().position(|g| *g >= 0.5).expect("reaches the tail");
            let level = peak - 30.0 * (k as f64 / SR as f64 - 0.025);
            let want = db(s * 2f64.sqrt());
            assert!((level - want).abs() <= 1.0, "{bits} bits, {f} Hz: g = ½ at {level:.1} dBFS, want {want:.1}");
            // below one step, the unquantised path exactly: no quantisation component left
            let k1 = k0 + gs[k0..].iter().position(|g| *g >= 1.0).unwrap();
            assert!(k1 < y.len() - 1000);
            let mut r = crusher(&[("quant", 16.0), ("rate", 48000.0), ("tails", 2.0)], SR, 1);
            let yr = run(&mut r, &x);
            let d = y[k1..].iter().zip(&yr[k1..]).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(db(d as f64) < -90.0, "{bits} bits, {f} Hz: {} dBFS", db(d as f64));
        }
    }
    // Gate: true silence below one step
    let x = decaying(440.0, -22.0, 2.0, SR);
    let mut c = crusher(&[("quant", 8.0), ("tails", 1.0)], SR, 1);
    let y = run(&mut c, &x);
    assert!(y[y.len() - 9600..].iter().all(|v| *v == 0.0));
    // a note from silence: fully crushed within 2 ms
    let mut c = crusher(&[("quant", 8.0), ("rate", 48000.0)], SR, 1);
    let mut x = vec![0.0f32; 4800];
    x.extend(sine(440.0, 0.3, 4800, SR));
    let mut gs = Vec::new();
    for b in x.chunks_mut(1) {
        c.process(&mut [b]);
        gs.push(c.ch[0].g);
    }
    assert_eq!(gs[4799], 1.0);
    assert!(gs[4800 + 96] < 0.01, "{}", gs[4800 + 96]);
}

#[test]
fn tails_fade_without_a_burst() {
    // HF (> 10 kHz) through the window never exceeds what the crushed note had before it
    use gfx_dsp::svf::{Svf, SvfMode};
    let x = decaying(440.0, -22.0, 2.0, SR);
    let mut c = crusher(&[("quant", 8.0), ("rate", 48000.0)], SR, 1);
    let y = run(&mut c, &x);
    let (mut h1, mut h2) = (Svf::new(), Svf::new());
    h1.set(SvfMode::HighPass, 10000.0, 0.541_196, 0.0, SR);
    h2.set(SvfMode::HighPass, 10000.0, 1.306_563, 0.0, SR);
    let hp: Vec<f32> = y.iter().map(|v| h2.process(h1.process(*v))).collect();
    let frame = 480;
    let pk: Vec<f32> = hp.chunks(frame).map(|c| c.iter().fold(0.0f32, |m, v| m.max(v.abs()))).collect();
    // the window spans −36 … −42 dBFS: 0.47 – 0.67 s
    let before = pk[20..46].iter().cloned().fold(0.0f32, f32::max);
    let during = pk[47..80].iter().cloned().fold(0.0f32, f32::max);
    assert!(during <= before * 1.12, "{} vs {} dB", db(during as f64), db(before as f64));
}

#[test]
fn level_holds_across_bits_and_curve() {
    // a guitar-like chord at −12 dBFS RMS, rate Off, Tread, tails Raw: within ±3 dB from 4 to 16 bits
    let n = SR as usize;
    let mut x: Vec<f32> = vec![0.0; n];
    for (k, f) in [82.4, 123.5, 164.8, 207.7, 246.9, 329.6].iter().enumerate() {
        for h in 1..12 {
            let a = 1.0 / (h as f64 * (k + 1) as f64).sqrt();
            for i in 0..n {
                x[i] += (a * (2.0 * PI * f * h as f64 * i as f64 / SR as f64 + h as f64).sin()) as f32;
            }
        }
    }
    let g = 10f64.powf(-12.0 / 20.0) / rms(&x);
    x.iter_mut().for_each(|v| *v *= g as f32);
    for bits in [4.0f32, 6.0, 8.0, 12.0, 16.0] {
        let mut c = crusher(&[("quant", bits), ("rate", 48000.0), ("tails", 2.0)], SR, 1);
        let y = run(&mut c, &x);
        let d = db(rms(&y) / rms(&x));
        assert!(d.abs() <= 3.0, "{bits} bits: {d:.2} dB");
    }
    // the DAC curve is level-neutral at −18 dBFS
    let s = sine(1000.0, 10f64.powf(-18.0 / 20.0) * 2f64.sqrt(), n, SR);
    let mut c = crusher(&[("quant", 16.0), ("rate", 48000.0), ("nonlin", 100.0)], SR, 1);
    let d = db(rms(&run(&mut c, &s)) / rms(&s));
    assert!(d.abs() <= 1.5, "nonlin 100 %: {d:.2} dB");
}

#[test]
fn model_filters() {
    use filters::Proto;
    let sr = SR as f64;
    let r = 8000.0;
    let warp = |f: f64, fc: f64| (PI * f / sr).tan() / (PI * fc / sr).tan();
    for (m, proto) in [(model::SAMPLER, Proto::butterworth(6)), (model::DRUM, Proto::ladder(LADDER_K))] {
        let (pre, post) = model_edges(m);
        for e in [pre, post].into_iter().flatten() {
            let fc = e * r;
            let mut cas = filters::Cascade::default();
            cas.design(&proto, fc, sr, 0);
            for f in [200.0, 1000.0, 0.7 * fc, fc, 1.3 * fc] {
                let got = gfx_dsp::svf::measure_gain_db(|x| cas.process(x), f as f32, SR) as f64;
                let want = db(proto.mag(warp(f, fc)));
                assert!((got - want).abs() <= 0.5, "{}: {f} Hz {got:.2} vs {want:.2}", MODELS[m]);
            }
        }
    }
    // Console: the 3-tap FIR on the virtual samples (here the host's: rate Off)
    for f in [1000.0, 6000.0, 12000.0, 20000.0] {
        let mut c = crusher(&[("model", model::CONSOLE as f32), ("rate", 48000.0), ("quant", 16.0), ("tails", 2.0)], SR, 1);
        let x = sine(f, 0.5, 24000, SR);
        let y = run(&mut c, &x);
        let got = db(amp_at(&y[4800..], f, SR) / 0.5);
        let w = 2.0 * PI * f / sr;
        let want = db(CONSOLE_FIR[1] + 2.0 * CONSOLE_FIR[0] * w.cos());
        assert!((got - want).abs() <= 0.2, "{f}: {got:.2} vs {want:.2}");
    }
    // Clean: the elliptic edges follow R within 2 % while an LFO sweeps it (±3 octaves at 10 Hz)
    let mut c = crusher(&[("model", 1.0), ("rate", 8000.0), ("mod", 1.0), ("mod_depth", 100.0), ("mod_rate", 10.0)], SR, 1);
    let x = sine(500.0, 0.3, 24000, SR);
    let mut y = x.clone();
    let mut worst: f64 = 0.0;
    // each design (every 16 samples) aims at R where its coefficient ramp ends
    for b in y.chunks_mut(16) {
        c.process(&mut [b]);
        worst = worst.max((c.designed_r / c.r_now - 1.0).abs());
    }
    eprintln!("Clean's edges track R within {:.2} %", worst * 100.0);
    assert!(worst <= 0.02, "{worst}");
}

/// A band-limited sawtooth with a frequency law `f(t)`.
fn saw(f: impl Fn(f64) -> f64, amp: f64, n: usize, sr: f32) -> Vec<f32> {
    let mut ph = 0.0;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let fi = f(i as f64 / sr as f64);
        ph += fi / sr as f64;
        let kmax = ((0.45 * sr as f64) / fi) as usize;
        let mut v = 0.0;
        for k in 1..=kmax.min(60) {
            v += (2.0 * PI * k as f64 * ph).sin() / k as f64;
        }
        out.push((amp * v * 0.5) as f32);
    }
    out
}

#[test]
fn track_locks_to_the_harmonics() {
    for f0 in [110.0, 220.0, 440.0] {
        let mut c = crusher(&[("track", 1.0), ("rate", 4000.0), ("quant", 16.0)], SR, 1);
        let x = saw(|_| f0, 0.4, SR as usize, SR);
        let mut y = x.clone();
        let mut rs = Vec::new();
        for b in y.chunks_mut(64) {
            c.process(&mut [b]);
            rs.push(c.r_now);
        }
        let r = *rs.last().unwrap();
        let n = (r / f0).round();
        let beat = (r - n * f0).abs();
        assert!(beat <= 6.0, "{f0} Hz: R {r:.1} = {n} × {:.2}: beat {beat:.2} Hz", r / n);
        // the aliases sit on the harmonic series within ±5 cents
        let cents = 1200.0 * (r / (n * f0)).log2();
        assert!(cents.abs() <= 5.0, "{cents}");
    }
    // a ±50-cent vibrato across a rounding boundary (R / f0 = 18.5) keeps N
    let fc = 4000.0 / 18.5;
    let x = saw(|t| fc * 2f64.powf(0.5 / 12.0 * (2.0 * PI * 5.0 * t).sin()), 0.4, 2 * SR as usize, SR);
    let mut c = crusher(&[("track", 1.0), ("rate", 4000.0), ("quant", 16.0)], SR, 1);
    let mut y = x.clone();
    let mut ns = Vec::new();
    for b in y.chunks_mut(64) {
        c.process(&mut [b]);
        ns.push(c.track.n);
    }
    let settled = &ns[ns.len() / 4..];
    let lo = settled.iter().cloned().fold(f64::MAX, f64::min);
    let hi = settled.iter().cloned().fold(f64::MIN, f64::max);
    assert!(lo == hi, "N moves between {lo} and {hi}");
    // an unvoiced burst (pick noise) holds R
    let mut x = saw(|_| 220.0, 0.4, SR as usize / 2, SR);
    let mut rng = gfx_dsp::lfo::Rng::new(9);
    x.extend((0..SR as usize / 8).map(|_| 0.3 * rng.bipolar()));
    x.extend(saw(|_| 220.0, 0.4, SR as usize / 4, SR));
    let mut c = crusher(&[("track", 1.0), ("rate", 4000.0)], SR, 1);
    let mut rs = Vec::new();
    for b in x.chunks_mut(64) {
        c.process(&mut [b]);
        rs.push(c.r_now);
    }
    let held = rs[(SR as usize / 2) / 64 - 1];
    let worst = rs[(SR as usize / 2) / 64..].iter().map(|r| (r / held - 1.0).abs()).fold(0.0, f64::max);
    assert!(worst <= 0.05, "R moved {:.1} %", worst * 100.0);
}

#[test]
fn mix_has_no_deep_notch() {
    for r in [6000.0, 16000.0] {
        let mut worst = 0.0f64;
        let mut f = 100.0;
        while f < 0.4 * r {
            let mut c = crusher(&[("mix", 50.0), ("quant", 16.0), ("rate", r as f32), ("tails", 2.0)], SR, 1);
            let y = run(&mut c, &sine(f, 0.5, 16384, SR));
            let g = db(amp_at(&y[4096..], f, SR) / 0.5);
            worst = worst.min(g);
            f *= 1.07;
        }
        eprintln!("mix 50 % at R {r}: deepest {worst:.2} dB below 0.4 R");
        assert!(worst >= -3.0, "R {r}: {worst:.2} dB");
    }
}

#[test]
fn jitter_and_lfo_are_deterministic() {
    let set = [("jitter", 60.0), ("mod", 1.0), ("mod_depth", 40.0), ("quant", 10.0), ("rate", 12000.0), ("model", 3.0)];
    let x = sine(330.0, 0.4, 48000, SR);
    let a = run(&mut crusher(&set, SR, 1), &x);
    let b = run(&mut crusher(&set, SR, 1), &x);
    assert_eq!(a, b);
    let mut c = crusher(&set, SR, 1);
    let _ = run(&mut c, &x);
    c.reset();
    assert_eq!(run(&mut c, &x), a);
}

#[test]
fn stereo_shares_one_clock() {
    let set = [("jitter", 50.0), ("rate", 9000.0)];
    let x = sine(440.0, 0.4, 9600, SR);
    let mono = run(&mut crusher(&set, SR, 1), &x);
    let mut c = crusher(&set, SR, 2);
    let (mut l, mut r) = (x.clone(), x.clone());
    for (a, b) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        c.process(&mut [a, b]);
    }
    assert_eq!(l, mono);
    assert_eq!(r, mono);
}

/// The common bench's parameter-step measure (HF above 10 kHz in the 100 ms after a min ↔ max step,
/// minus the steady signal's), for every parameter but the four that sweep the converter itself
/// (`quant`, `rate`, `rate_shift`, `input`: their glides pass through real images and clipping).
#[test]
fn parameter_steps_are_click_free() {
    use gfx_dsp::svf::{Svf, SvfMode};
    for sr in [48000.0f32, 96000.0] {
        let n = sr as usize;
        let x = sine(1000.0, 0.25, n, sr);
        for p in PARAMS.iter().filter(|p| !matches!(p.id, "quant" | "rate" | "rate_shift" | "input")) {
            for (from, to) in [(p.min, p.max), (p.max, p.min)] {
                let mut c = crusher(&[(p.id, from)], sr, 1);
                let mut y = x.clone();
                let (a, b) = y.split_at_mut(n / 2);
                c.process(&mut [a]);
                c.set_param(p.id, to);
                c.process(&mut [b]);
                let pk = y.iter().fold(0.0f32, |m, v| m.max(v.abs()));
                let (mut h1, mut h2) = (Svf::new(), Svf::new());
                h1.set(SvfMode::HighPass, 10000.0, 0.541_196, 0.0, sr);
                h2.set(SvfMode::HighPass, 10000.0, 1.306_563, 0.0, sr);
                let hp: Vec<f32> = y.iter().map(|v| h2.process(h1.process(*v))).collect();
                let pkf = |s: &[f32]| s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
                let after = pkf(&hp[n / 2..n / 2 + n / 10]);
                let steady = pkf(&hp[n / 4..n / 2]).max(pkf(&hp[n - n / 4..]));
                let rel = db((after - steady).max(0.0) as f64) - db(pk as f64);
                assert!(rel <= -35.0, "{sr}: {} {from} → {to}: {rel:.1} dB", p.id);
            }
        }
    }
}

/// CPU of the worst case (Clean + track + LFO + jitter), mono, 48 kHz. Run with `--release --ignored`.
#[test]
#[ignore]
fn cpu_worst_case() {
    let set = [("model", 1.0), ("track", 1.0), ("mod", 1.0), ("mod_depth", 50.0), ("jitter", 30.0), ("rate", 8000.0), ("law", 1.0)];
    let mut c = crusher(&set, SR, 1);
    let x = saw(|_| 196.0, 0.3, 10 * SR as usize, SR);
    let t = std::time::Instant::now();
    let _ = run(&mut c, &x);
    eprintln!("worst case: {:.1} ns / sample", t.elapsed().as_secs_f64() * 1e9 / x.len() as f64);
}
