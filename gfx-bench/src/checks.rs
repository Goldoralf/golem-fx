// SPDX-License-Identifier: GPL-3.0-or-later
//! The measurements common to every effect (README.md §4).

use crate::{fresh, render, signals};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::util::FtzGuard;
use gfx_dsp::ParamKind;
use std::time::Instant;

pub const SAMPLE_RATES: [f32; 4] = [44100.0, 48000.0, 96000.0, 192000.0];

#[derive(Clone, Debug)]
pub struct CheckResult {
    pub name: String,
    pub sample_rate: f32,
    pub pass: bool,
    pub detail: String,
}

impl CheckResult {
    fn new(name: &str, sr: f32, pass: bool, detail: String) -> Self {
        Self { name: name.to_string(), sample_rate: sr, pass, detail }
    }
}

/// Options an effect may need beyond the defaults (bounds it documents in its design).
#[derive(Clone, Debug)]
pub struct Options {
    /// High-frequency burst allowed after a parameter step, relative to the output peak (dB).
    pub click_bound_db: f32,
    /// Floor allowed after 3 s of silence (dBFS); effects adding noise on purpose raise it.
    pub silence_floor_db: f32,
    /// Stereo by nature: skip the L = R = mono check.
    pub stereo_by_nature: bool,
    /// Sample rates to run.
    pub sample_rates: Vec<f32>,
}

impl Default for Options {
    fn default() -> Self {
        Self { click_bound_db: -50.0, silence_floor_db: -140.0, stereo_by_nature: false, sample_rates: SAMPLE_RATES.to_vec() }
    }
}

fn max_abs_diff(a: &[Vec<f32>], b: &[Vec<f32>]) -> f32 {
    a.iter()
        .zip(b)
        .flat_map(|(x, y)| x.iter().zip(y).map(|(p, q)| (p - q).abs()))
        .fold(0.0, f32::max)
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-12).log10()
}

/// 4th-order Butterworth high-pass (two SVF sections).
fn highpass4(x: &[f32], fc: f32, sr: f32) -> Vec<f32> {
    let mut a = Svf::new();
    let mut b = Svf::new();
    a.set(SvfMode::HighPass, fc, 0.541_196, 0.0, sr);
    b.set(SvfMode::HighPass, fc, 1.306_563, 0.0, sr);
    x.iter().map(|&v| b.process(a.process(v))).collect()
}

/// Latency by cross-correlation of noise in and out (an impulse would be shaped by gates and
/// envelopes): the lag of the correlation peak, over the second half of a 1 s pink-noise render.
pub fn latency(name: &str, sr: f32) -> CheckResult {
    let mut e = fresh(name, sr, 1, 512);
    let n = sr as usize;
    let x = signals::pink(n, 0.1, 17);
    let y = render(e.as_mut(), std::slice::from_ref(&x), 512);
    let reported = e.latency();
    let from = n / 2;
    let max_lag = 4096.min(n / 4);
    let (mut best, mut best_lag) = (0.0f64, 0usize);
    for lag in 0..max_lag {
        let mut c = 0.0f64;
        for i in from..n - max_lag {
            c += x[i] as f64 * y[0][i + lag] as f64;
        }
        if c.abs() > best {
            best = c.abs();
            best_lag = lag;
        }
    }
    if best < 1e-9 {
        return CheckResult::new("latency", sr, false, "no output at default settings".into());
    }
    let pass = best_lag == reported;
    CheckResult::new("latency", sr, pass, format!("measured {best_lag} samples, reported {reported}"))
}

pub fn determinism(name: &str, sr: f32) -> CheckResult {
    let x = vec![signals::pink((sr * 2.0) as usize, 0.1, 7)];
    let mut a = fresh(name, sr, 1, 512);
    let mut b = fresh(name, sr, 1, 512);
    let ya = render(a.as_mut(), &x, 512);
    let yb = render(b.as_mut(), &x, 512);
    // and after reset() of a used instance
    a.reset();
    let yc = render(a.as_mut(), &x, 512);
    let pass = ya == yb && ya == yc;
    CheckResult::new("determinism", sr, pass, if pass { "bit-identical (fresh ×2, after reset)".into() } else { format!("max diff {:.3e}", max_abs_diff(&ya, &yb).max(max_abs_diff(&ya, &yc))) })
}

pub fn block_size(name: &str, sr: f32) -> CheckResult {
    let x = vec![signals::pink(sr as usize, 0.1, 11)];
    let mut r = fresh(name, sr, 1, 4096);
    let reference = render(r.as_mut(), &x, 512);
    let mut worst = 0.0f32;
    for b in [1usize, 32, 64, 4096] {
        let mut e = fresh(name, sr, 1, 4096);
        worst = worst.max(max_abs_diff(&reference, &render(e.as_mut(), &x, b)));
    }
    let pass = worst <= 1e-6;
    CheckResult::new("block size", sr, pass, format!("max diff {:.1} dB (blocks 1, 32, 64, 512, 4096)", db(worst)))
}

pub fn silence(name: &str, sr: f32, opts: &Options) -> CheckResult {
    let burst = (sr * 0.5) as usize;
    let tail = (sr * 3.0) as usize;
    let x = vec![signals::burst_then_silence(burst, tail, 0.5, 3)];
    let mut e = fresh(name, sr, 1, 512);
    // time the first and the last silent second
    let mut out = x.clone();
    let n = out[0].len();
    let sec = sr as usize;
    let mut times = (0.0f64, 0.0f64);
    let mut start = 0;
    while start < n {
        let end = (start + 512).min(n);
        let t = Instant::now();
        e.process(&mut [&mut out[0][start..end]]);
        let dt = t.elapsed().as_secs_f64();
        if start >= burst && start < burst + sec {
            times.0 += dt;
        }
        if start >= n - sec {
            times.1 += dt;
        }
        start = end;
    }
    let finite = out[0].iter().all(|v| v.is_finite());
    let floor = peak(&out[0][n - (sr * 0.5) as usize..]);
    let slow = times.1 > 3.0 * times.0.max(1e-6);
    let pass = finite && db(floor) <= opts.silence_floor_db && !slow;
    CheckResult::new(
        "silence & denormals",
        sr,
        pass,
        format!(
            "{}floor {:.0} dBFS (bound {:.0}), last second {:.1}× the first",
            if finite { "" } else { "NaN/inf! " },
            db(floor),
            opts.silence_floor_db,
            times.1 / times.0.max(1e-9)
        ),
    )
}

pub fn extremes(name: &str, sr: f32) -> CheckResult {
    let params = fresh(name, sr, 1, 512).params();
    let x = vec![signals::white((sr * 0.5) as usize, 4.0, 5)];
    let mut worst = String::new();
    let mut pass = true;
    for p in params {
        for v in [p.min, p.max] {
            let mut e = fresh(name, sr, 1, 512);
            e.set_param(p.id, v);
            e.reset();
            let y = render(e.as_mut(), &x, 512);
            let ok = y[0].iter().all(|s| s.is_finite() && s.abs() < 1e3);
            if !ok {
                pass = false;
                worst = format!("{} = {v}: non-finite or > 1e3", p.id);
            }
        }
    }
    CheckResult::new("extreme values", sr, pass, if pass { "every parameter at min and max, input +12 dBFS: bounded".into() } else { worst })
}

pub fn param_steps(name: &str, sr: f32, opts: &Options) -> CheckResult {
    let params = fresh(name, sr, 1, 512).params();
    let n = sr as usize;
    let x = vec![signals::sine(n, 1000.0, 0.25, sr)];
    let fc = 10000.0f32.min(sr * 0.4);
    let mut worst = (f32::MIN, String::new());
    for p in params {
        for (from, to) in [(p.min, p.max), (p.max, p.min)] {
            let mut e = fresh(name, sr, 1, 512);
            e.set_param(p.id, from);
            e.reset();
            let mut y = x.clone();
            let half = n / 2;
            {
                let (a, b) = y[0].split_at_mut(half);
                e.process(&mut [a]);
                e.set_param(p.id, to);
                e.process(&mut [b]);
            }
            let out_peak = peak(&y[0]);
            if out_peak < 1e-6 {
                continue;
            }
            let hp = highpass4(&y[0], fc, sr);
            // HF in the 100 ms after the step, minus what the steady signal already has
            let after = peak(&hp[half..half + n / 10]);
            let steady = peak(&hp[n / 4..half]).max(peak(&hp[n - n / 4..]));
            let excess = (after - steady).max(0.0);
            let rel = db(excess) - db(out_peak);
            let kind = match p.kind {
                ParamKind::Float => "",
                _ => " (toggle)",
            };
            if rel > worst.0 {
                worst = (rel, format!("{}{kind} {from} → {to}", p.id));
            }
        }
    }
    let pass = worst.0 <= opts.click_bound_db;
    CheckResult::new("parameter steps", sr, pass, format!("worst HF burst {:.0} dB re peak ({}), bound {:.0} dB", worst.0.max(-200.0), worst.1, opts.click_bound_db))
}

pub fn mono_stereo(name: &str, sr: f32, opts: &Options) -> CheckResult {
    if opts.stereo_by_nature {
        return CheckResult::new("mono / stereo", sr, true, "stereo by nature: skipped".into());
    }
    let x = signals::pink(sr as usize / 2, 0.1, 9);
    let mut m = fresh(name, sr, 1, 512);
    let mut s = fresh(name, sr, 2, 512);
    let ym = render(m.as_mut(), std::slice::from_ref(&x), 512);
    let ys = render(s.as_mut(), &[x.clone(), x], 512);
    let d = max_abs_diff(&[ym[0].clone(), ym[0].clone()], &ys);
    CheckResult::new("mono / stereo", sr, d <= 1e-7, format!("max diff L/R vs mono {:.1} dB", db(d)))
}

/// CPU in ns per sample-frame (mono and stereo, blocks of 128, 10 s of noise at 48 kHz).
pub fn cpu(name: &str) -> (f64, f64) {
    let sr = 48000.0;
    let n = (sr * 10.0) as usize;
    let x = signals::pink(n, 0.1, 13);
    let mut res = [0.0f64; 2];
    for (k, ch) in [1usize, 2].into_iter().enumerate() {
        let mut e = fresh(name, sr, ch, 128);
        let input: Vec<Vec<f32>> = (0..ch).map(|_| x.clone()).collect();
        let t = Instant::now();
        let _ = render(e.as_mut(), &input, 128);
        res[k] = t.elapsed().as_secs_f64() * 1e9 / n as f64;
    }
    (res[0], res[1])
}

/// Runs every common check.
pub fn run_all(name: &str, opts: &Options) -> Vec<CheckResult> {
    let _ftz = FtzGuard::new();
    let mut v = Vec::new();
    for &sr in &opts.sample_rates {
        v.push(latency(name, sr));
        v.push(determinism(name, sr));
        v.push(block_size(name, sr));
        v.push(silence(name, sr, opts));
        v.push(extremes(name, sr));
        v.push(param_steps(name, sr, opts));
        v.push(mono_stereo(name, sr, opts));
    }
    v
}
