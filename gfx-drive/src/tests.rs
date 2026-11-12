// SPDX-License-Identifier: GPL-3.0-or-later
//! The drive's own bench suite (README.md §6).

use super::*;

const SR: f32 = 48000.0;

fn dr(s: Settings) -> Drive {
    let mut d = Drive::new();
    d.prepare(SR, 4800, 1);
    d.set(s);
    d.reset();
    d
}

fn run(d: &mut Drive, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for b in y.chunks_mut(256) {
        d.process(&mut [b]);
    }
    y
}

fn sine(f: f32, amp: f32, n: usize) -> Vec<f32> {
    (0..n).map(|i| amp * (std::f32::consts::TAU * f * i as f32 / SR).sin()).collect()
}

fn rms(v: &[f32]) -> f64 {
    (v.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
}

/// Magnitude at frequency f (Hann-windowed DFT probe), linear.
fn probe(v: &[f32], f: f64) -> f64 {
    let n = v.len();
    let w = std::f64::consts::TAU * f / SR as f64;
    let (mut re, mut im, mut ws) = (0.0, 0.0, 0.0);
    for (k, s) in v.iter().enumerate() {
        let h = 0.5 - 0.5 * (std::f64::consts::TAU * k as f64 / n as f64).cos();
        re += *s as f64 * h * (w * k as f64).cos();
        im += *s as f64 * h * (w * k as f64).sin();
        ws += h;
    }
    2.0 * (re * re + im * im).sqrt() / ws
}

#[test]
fn legacy_is_the_js_curve() {
    let s = Settings::default().with(p::VOICE, 3.0).with(p::GAIN, 20.0).with(p::HARDNESS, 6.0).with(p::CEILING, -12.0);
    let mut d = dr(s);
    let f0 = 220.0;
    let x = sine(f0, 0.3, 48000);
    let y = run(&mut d, &x);
    let (pre, m, soft) = (2f64.powf(20.0 / 6.0), 2f64.powf(-2.0), 2f64.powf(6.0));
    let want: Vec<f32> = x.iter().map(|v| legacy_f(*v as f64 * pre, m, soft) as f32).collect();
    // the harmonics' amplitudes match the JS curve within 0.3 dB (phase aside: filters, ADAA)
    for k in [1.0, 3.0, 5.0] {
        let (a, b) = (probe(&y[24000..], f0 as f64 * k), probe(&want[24000..], f0 as f64 * k));
        assert!((20.0 * (a / b).log10()).abs() < 0.3, "harmonic {k}: {a} vs {b}");
    }
}

#[test]
fn aliasing_stays_low_at_full_gain() {
    for voice in 0..4 {
        let s = Settings::default().with(p::VOICE, voice as f32).with(p::GAIN, 50.0).with(p::HARDNESS, 10.0).with(p::CEILING, 0.0);
        let mut d = dr(s);
        let f0 = 3700.0;
        let y = run(&mut d, &sine(f0, 0.5, 48000));
        let seg = &y[16000..48000];
        // the worst inharmonic component between 500 Hz and 20 kHz (aliases of harmonics above Nyquist)
        let mut worst = 0.0f64;
        let mut f = 500.0;
        while f < 20000.0 {
            let k = (f / f0 as f64).round();
            if (f - k * f0 as f64).abs() > 150.0 {
                worst = worst.max(probe(seg, f));
            }
            f += 97.0;
        }
        let db = 20.0 * worst.log10();
        eprintln!("voice {voice}: worst alias {db:.1} dBFS");
        assert!(db < -85.0, "voice {voice}: worst alias {db:.1} dBFS");
    }
}

#[test]
fn level_holds_across_the_gain_sweep() {
    for voice in 0..3 {
        let lv = |g: f32| {
            let mut d = dr(Settings::default().with(p::VOICE, voice as f32).with(p::GAIN, g));
            let y = run(&mut d, &sine(220.0, 0.2, 24000));
            20.0 * rms(&y[12000..]).log10()
        };
        let (a, b) = (lv(30.0), lv(50.0));
        assert!((a - b).abs() < 2.0, "voice {voice}: {a:.1} vs {b:.1} dB");
    }
}

#[test]
fn full_blend_is_flat() {
    let mut d = dr(Settings::default().with(p::BLEND, 100.0).with(p::GAIN, 40.0));
    let mut r = gfx_dsp::lfo::Rng::new(5);
    let x: Vec<f32> = (0..48000).map(|_| 0.2 * r.bipolar()).collect();
    let y = run(&mut d, &x);
    for f in [100.0, 1000.0, 5000.0, 12000.0] {
        let db = 20.0 * (probe(&y[8000..], f) / probe(&x[8000..], f)).log10();
        assert!(db.abs() < 1.0, "{f} Hz: {db:.2} dB");
    }
}

#[test]
fn bass_keep_leaves_the_fundamental_clean() {
    let mut d = dr(Settings::default().with(p::BASS_KEEP, 150.0).with(p::GAIN, 50.0).with(p::VOICE, 1.0));
    let y = run(&mut d, &sine(41.2, 0.3, 48000));
    let seg = &y[24000..];
    let (f1, f3) = (probe(seg, 41.2), probe(seg, 123.6));
    assert!((f1 / 0.3 - 1.0).abs() < 0.1, "fundamental {f1}");
    assert!(f3 < 0.05 * f1, "3rd harmonic {:.1} dB", 20.0 * (f3 / f1).log10());
}
