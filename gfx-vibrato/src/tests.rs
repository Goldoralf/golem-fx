// SPDX-License-Identifier: GPL-3.0-or-later
//! The vibrato's own bench suite (README.md §6).

use super::*;

const SR: f32 = 48000.0;

fn vib(f: impl FnOnce(&mut Settings)) -> Vibrato {
    let mut v = Vibrato::new();
    v.prepare(SR, 4800, 1);
    let mut s = Settings::default();
    f(&mut s);
    v.set(s);
    v.reset();
    v
}

fn sine(f: f32, n: usize) -> Vec<f32> {
    (0..n).map(|i| 0.5 * (std::f32::consts::TAU * f * i as f32 / SR).sin()).collect()
}

fn run(v: &mut Vibrato, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for c in y.chunks_mut(256) {
        v.process(&mut [c]);
    }
    y
}

/// Instantaneous frequency (Hz) from upward zero crossings, linearly interpolated.
fn inst_freq(y: &[f32]) -> Vec<f64> {
    let mut t = Vec::new();
    for i in 1..y.len() {
        if y[i - 1] < 0.0 && y[i] >= 0.0 {
            t.push((i - 1) as f64 + (-y[i - 1] / (y[i] - y[i - 1])) as f64);
        }
    }
    t.windows(2).map(|w| SR as f64 / (w[1] - w[0])).collect()
}

fn peak_cents(y: &[f32], f0: f64) -> f64 {
    inst_freq(y).iter().map(|f| (1200.0 * (f / f0).log2()).abs()).fold(0.0, f64::max)
}

#[test]
fn depth_is_in_cents_at_every_speed() {
    for &speed in &[0.5f32, 5.0, 14.0] {
        let mut v = vib(|s| {
            s.depth = 0.5;
            s.speed = speed;
        });
        let n = (SR * 4.0) as usize;
        let y = run(&mut v, &sine(1000.0, n));
        let c = peak_cents(&y[n / 2..], 1000.0);
        assert!((c / 50.0 - 1.0).abs() < 0.05, "{speed} Hz: {c:.1} cents");
    }
}

#[test]
fn mean_delay_is_the_least_the_depth_needs() {
    let v = vib(|s| s.speed = 5.0);
    let need = swing_for(100.0, 5.0, peak_slope(0)) + 4.0 / SR as f64 + Bbd::new(1024).min_delay();
    assert!((v.centre() - need).abs() < 1e-9 && v.centre() < 0.003, "{}", v.centre());
    let slow = vib(|s| s.speed = 0.1);
    assert!(slow.centre() <= MAX_DELAY + 1e-12);
}

#[test]
fn depth_changes_never_move_the_mean_delay() {
    let mut v = vib(|s| s.depth = 0.0);
    let c0 = v.centre();
    let mut s = v.settings();
    s.depth = 1.0;
    v.set(s);
    run(&mut v, &sine(440.0, 24000));
    assert_eq!(v.centre(), c0);
}

#[test]
fn parameter_changes_never_move_the_phase() {
    let mut v = vib(|_| {});
    run(&mut v, &sine(440.0, 4800));
    let p = v.phase();
    let mut s = v.settings();
    s.model = Model::Vibe;
    s.shape = 1;
    s.depth = 0.9;
    v.set(s);
    assert_eq!(v.phase(), p);
}

#[test]
fn varistor_is_zero_latency_and_swings_most_phase_mid_band() {
    let mut v = vib(|s| {
        s.model = Model::Varistor;
        s.depth = 0.0;
    });
    let mut x = vec![0.0f32; 64];
    x[10] = 1.0;
    let y = run(&mut v, &x);
    assert!(y[10].abs() > 0.1 && y[..10].iter().all(|s| *s == 0.0));
    let dev = |f: f32| {
        let mut v = vib(|s| {
            s.model = Model::Varistor;
            s.depth = 0.8;
            s.speed = 5.0;
        });
        let n = (SR * 2.0) as usize;
        let y = run(&mut v, &sine(f, n));
        // phase swing ∝ frequency deviation in Hz (phase modulation: its depth is a phase, not cents)
        f as f64 * ((peak_cents(&y[n / 2..], f as f64) / 1200.0).exp2() - 1.0)
    };
    let (lo, mid) = (dev(100.0), dev(1000.0));
    assert!(mid > 2.0 * lo, "100 Hz ±{lo:.1} Hz, 1 kHz ±{mid:.1} Hz");
}

#[test]
fn rise_reaches_63_percent_at_rise_time() {
    let mut v = vib(|s| {
        s.depth = 1.0;
        s.speed = 5.0;
        s.rise = 1000.0;
    });
    let n = (SR * 1.1) as usize;
    let y = run(&mut v, &sine(1000.0, n));
    // the cycle around t = 1 s (one LFO period = 0.2 s)
    let c = peak_cents(&y[(SR * 0.9) as usize..(SR * 1.1) as usize], 1000.0);
    assert!(c > 0.55 * 100.0 && c < 0.72 * 100.0, "{c:.1} cents");
}

#[test]
fn every_model_and_shape_is_stable() {
    for m in Model::ALL {
        for shape in 0..3 {
            let mut v = vib(|s| {
                s.model = m;
                s.shape = shape;
                s.depth = 1.0;
                s.speed = 14.0;
                s.drive = 1.0;
                s.tone = 0.0;
            });
            let mut rng = Rng::new(3);
            let x: Vec<f32> = (0..24000).map(|_| rng.bipolar()).collect();
            let y = run(&mut v, &x);
            assert!(y.iter().all(|s| s.is_finite() && s.abs() < 10.0), "{m:?} {shape}");
        }
    }
}

// ---------------------------------------------------------------- gfx-dsp `scanner` extraction: the null

fn fnv(h: &mut u64, x: &[f32]) {
    for v in x {
        for b in v.to_bits().to_le_bytes() {
            *h ^= b as u64;
            *h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
}

/// Scanner renders (V1 / V2 / V3 depths, chorus mix, both channels, a model switch out of and back
/// into Scanner, drive, three rates): the hash written before the extraction into gfx-dsp `scanner`.
fn scanner_null_hash() -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for sr in [44100.0f32, 48000.0, 96000.0] {
        for (depth, speed, mix, drive, shape) in [(0.47f32, 6.9f32, 1.0f32, 0.0f32, 0u32), (0.68, 6.9, 0.5, 0.0, 0), (1.0, 6.9, 1.0, 0.3, 1), (0.3, 2.0, 0.5, 0.0, 2), (1.0, 12.0, 0.7, 0.0, 0)] {
            for nch in [1usize, 2] {
                for switch in [false, true] {
                    let mut v = Vibrato::new();
                    v.prepare(sr, 512, nch);
                    let s = Settings { depth, speed, model: Model::Scanner, shape, mix, drive, stereo_phase: 45.0, ..Settings::default() };
                    v.set(s);
                    v.reset();
                    let n = (0.5 * sr) as usize;
                    let x: Vec<f32> = (0..n).map(|i| 0.3 * (std::f32::consts::TAU * 440.0 * i as f32 / sr).sin() + 0.2 * (std::f32::consts::TAU * 2717.0 * i as f32 / sr).sin()).collect();
                    let mut l = x.clone();
                    let mut r: Vec<f32> = x.iter().map(|a| 0.7 * a).collect();
                    let mut k = 0;
                    while k < n {
                        if switch && k == 256 * 40 {
                            v.set(Settings { model: Model::Clean, ..s });
                        }
                        if switch && k == 256 * 70 {
                            v.set(Settings { model: Model::Scanner, ..s });
                        }
                        let e = (k + 256).min(n);
                        if nch == 2 {
                            v.process(&mut [&mut l[k..e], &mut r[k..e]]);
                        } else {
                            v.process(&mut [&mut l[k..e]]);
                        }
                        k = e;
                    }
                    fnv(&mut h, &l);
                    if nch == 2 {
                        fnv(&mut h, &r);
                    }
                }
            }
        }
    }
    h
}

#[test]
fn scanner_null_against_the_build_before_the_move() {
    let h = scanner_null_hash();
    println!("scanner null hash {h:#018x}");
    assert_eq!(h, SCANNER_NULL);
}

const SCANNER_NULL: u64 = 0xb7182436ca4165dc;

#[test]
#[ignore]
fn scanner_profile() {
    for (name, depth, mix) in [("Scanner V3", 1.0f32, 1.0f32), ("Scanner C2", 0.68, 0.5)] {
        let mut v = vib(|s| {
            s.model = Model::Scanner;
            s.depth = depth;
            s.mix = mix;
            s.speed = 6.9;
        });
        let x = sine(220.0, 48000 * 5);
        let t = std::time::Instant::now();
        let _ = run(&mut v, &x);
        eprintln!("{name:12} {:6.1} ns / sample", t.elapsed().as_secs_f64() * 1e9 / x.len() as f64);
    }
}
