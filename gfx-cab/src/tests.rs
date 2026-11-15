// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5): the rows that run on synthetic signals, the plumbing rows (index,
//! meters, background loads, activation) through `host` and `core` as the wrapper drives them, and the
//! rows on golem's own IR files (`~/golem/assets/irs`, skipped when absent).

use gfx_dsp::ir_engine::{self, Host, Worker};
use crate::{builtin, Cab, PARAMS};
use gfx_dsp::ir::{self, Align, ChannelMap, Normalize, Settings, Source};
use gfx_dsp::ir_host::{self, Job, Loader};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::Effect;
use realfft::RealFftPlanner;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

// ---- helpers ----

fn rng(seed: u64) -> impl FnMut() -> f32 {
    let mut s = seed | 1;
    move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        ((s >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
    }
}

fn noise(n: usize, seed: u64, amp: f32) -> Vec<f32> {
    let mut r = rng(seed);
    (0..n).map(|_| amp * r()).collect()
}

fn sine(n: usize, f: f32, a: f32, sr: f32) -> Vec<f32> {
    (0..n).map(|k| a * (2.0 * std::f32::consts::PI * f * k as f32 / sr).sin()).collect()
}

/// A decaying noise IR (a synthetic cab or room), `len` samples.
fn decaying(len: usize, seed: u64, tau: f32) -> Vec<f32> {
    let mut r = rng(seed);
    (0..len).map(|k| r() * (-(k as f32) / tau).exp()).collect()
}

fn cab(sr: f32, ch: usize, irs: &[(u32, Vec<f32>)], set: &[(&str, f32)]) -> Cab {
    let mut c = Cab::new();
    for (slot, h) in irs {
        c.load_ir(*slot, std::slice::from_ref(h), sr);
    }
    c.prepare(sr, 512, ch);
    for &(id, v) in set {
        c.set_param(id, v);
    }
    c.reset();
    c
}

fn render(c: &mut Cab, x: &[f32], block: usize) -> Vec<f32> {
    let mut y = x.to_vec();
    for b in y.chunks_mut(block) {
        c.process(&mut [b]);
    }
    y
}

fn fft_conv(x: &[f32], h: &[f32]) -> Vec<f64> {
    let n = (x.len() + h.len()).next_power_of_two();
    let mut pl = RealFftPlanner::<f64>::new();
    let f = pl.plan_fft_forward(n);
    let i = pl.plan_fft_inverse(n);
    let mut a = vec![0.0; n];
    let mut b = vec![0.0; n];
    a.iter_mut().zip(x).for_each(|(d, s)| *d = *s as f64);
    b.iter_mut().zip(h).for_each(|(d, s)| *d = *s as f64);
    let (mut fa, mut fb) = (f.make_output_vec(), f.make_output_vec());
    f.process(&mut a, &mut fa).unwrap();
    f.process(&mut b, &mut fb).unwrap();
    fa.iter_mut().zip(&fb).for_each(|(p, q)| *p *= *q / n as f64);
    let mut y = vec![0.0; n];
    i.process(&mut fa, &mut y).unwrap();
    y.truncate(x.len());
    y
}

fn err_db(y: &[f32], r: &[f64]) -> f64 {
    let e: f64 = y.iter().zip(r).map(|(a, b)| (*a as f64 - b).powi(2)).sum();
    let p: f64 = r.iter().map(|b| b * b).sum();
    10.0 * (e / p.max(1e-300)).log10()
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-300).log10()
}

fn hp10k(x: &[f32], sr: f32) -> Vec<f32> {
    let mut a = Svf::new();
    let mut b = Svf::new();
    a.set(SvfMode::HighPass, 10000.0, 0.541_196, 0.0, sr);
    b.set(SvfMode::HighPass, 10000.0, 1.306_563, 0.0, sr);
    x.iter().map(|&v| b.process(a.process(v))).collect()
}

/// |H(f)| (dB) of an IR.
fn mag_db(h: &[f64], sr: f64, f: f64) -> f64 {
    let (mut re, mut im) = (0.0, 0.0);
    for (k, v) in h.iter().enumerate() {
        let w = -2.0 * std::f64::consts::PI * f * k as f64 / sr;
        re += v * w.cos();
        im += v * w.sin();
    }
    10.0 * (re * re + im * im).max(1e-300).log10()
}

fn raw_settings() -> [(&'static str, f32); 2] {
    [("align", 0.0), ("normalize", 0.0)]
}

/// golem's IR library (None when this machine has none).
fn assets() -> Option<PathBuf> {
    let d = PathBuf::from(std::env::var_os("HOME")?).join("golem/assets/irs");
    d.is_dir().then_some(d)
}

fn wavs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map(|r| r.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "wav")).collect()).unwrap_or_default();
    v.sort();
    v
}

fn read(p: &Path) -> Option<Source> {
    ir_host::decode_wav(&std::fs::read(p).ok()?).ok()
}

fn cab_settings(align: Align, normalize: Normalize) -> Settings {
    Settings {
        align,
        normalize,
        min_phase: false,
        reverse: false,
        length_s: 2.0,
        fade: 0.1,
        max_s: 2.0,
        band: builtin::BAND,
        ref_db: builtin::REF_DB,
        map: ChannelMap::Cab,
        out_channels: 1,
    }
}

// ---- §5 rows ----

#[test]
fn dirac_is_exact() {
    for sr in [44100.0f32, 48000.0, 96000.0, 192000.0] {
        let x = noise(20000, 3, 0.5);
        let mut set = raw_settings().to_vec();
        set.push(("ir_a", 1.0));
        let mut c = cab(sr, 1, &[(1, vec![1.0])], &set);
        assert_eq!(c.loaded(0), 1.0);
        assert_eq!(render(&mut c, &x, 333), x, "δ at {sr}");
        let mut d = vec![0.0f32; 38];
        d[37] = 1.0;
        let mut c = cab(sr, 1, &[(1, d)], &set);
        let y = render(&mut c, &x, 1000);
        for n in 0..x.len() {
            assert_eq!(y[n], if n >= 37 { x[n - 37] } else { 0.0 });
        }
    }
}

#[test]
fn zero_latency_first_sample() {
    let h = decaying(9600, 5, 800.0);
    let mut set = raw_settings().to_vec();
    set.push(("ir_a", 2.0));
    let mut c = cab(48000.0, 1, &[(2, h.clone())], &set);
    let mut x = [1.0f32];
    c.process(&mut [&mut x]);
    assert_eq!(x[0], h[0], "output[0] = h[0] from a first block of 1 sample");
    // the built-in: tap 0 is its first sample, the noise correlation peak is its group delay
    let src = builtin::render(48000.0);
    let mut c = cab(48000.0, 1, &[], &[]);
    let mut x = [1.0f32];
    c.process(&mut [&mut x]);
    assert!((x[0] as f64 - src.channels[0][0]).abs() < 1e-6, "{} vs {}", x[0], src.channels[0][0]);
}

#[test]
fn accuracy_against_f64_reference() {
    let sr = 48000.0;
    for (secs, seed) in [(0.2f32, 1u64), (2.0, 2)] {
        let len = (secs * sr) as usize;
        let h = decaying(len, seed, len as f32 / 4.0);
        let x = noise(len + 48000, 9, 0.3);
        let mut set = raw_settings().to_vec();
        set.push(("ir_a", 7.0));
        let mut c = cab(sr, 1, &[(7, h.clone())], &set);
        let y = render(&mut c, &x, 256);
        let e = err_db(&y, &fft_conv(&x, &h));
        eprintln!("cab: accuracy, {secs} s IR: {e:.1} dB re output");
        assert!(e < -110.0);
    }
}

#[test]
fn block_size_independent_with_two_irs_and_a_swap() {
    let sr = 48000.0;
    let irs = [(1u32, decaying(9600, 1, 1500.0)), (2, decaying(30000, 2, 6000.0)), (3, decaying(4000, 3, 700.0))];
    let x = noise(96000, 4, 0.3);
    let run = |blocks: &mut dyn FnMut() -> usize| {
        let mut c = cab(sr, 1, &irs, &[("ir_a", 1.0), ("ir_b", 2.0), ("blend", 40.0)]);
        let mut y = x.clone();
        let mut i = 0;
        let mut swapped = false;
        while i < y.len() {
            let mut b = blocks().min(y.len() - i).max(1);
            if !swapped && i + b > 40000 {
                b = 40000 - i;
                if b == 0 {
                    // the swap request lands at the same sample whatever the blocks
                    c.set_param("ir_a", 3.0);
                    swapped = true;
                    continue;
                }
            }
            c.process(&mut [&mut y[i..i + b]]);
            i += b;
        }
        y
    };
    let reference = run(&mut || 64);
    for bs in [1usize, 7, 1000, 4096] {
        assert!(run(&mut || bs) == reference, "block {bs}");
    }
    let mut r = rng(77);
    assert!(run(&mut || 1 + ((r() + 1.0) * 600.0) as usize) == reference, "random blocks");
}

#[test]
fn swap_is_smooth_and_never_dips() {
    // ir_a changed every 500 ms between two captures of one (synthetic) cab, under a 1 kHz sine
    let sr = 48000.0;
    let a = decaying(9600, 11, 900.0);
    let mut b = decaying(9600, 11, 900.0);
    let mut r = rng(5);
    b.iter_mut().for_each(|v| *v = *v * 0.8 + 0.05 * r() * v.abs());
    let x = sine((sr * 3.0) as usize, 1000.0, 0.25, sr);
    let mut c = cab(sr, 1, &[(1, a.clone()), (2, b.clone())], &[("ir_a", 1.0), ("normalize", 0.0)]);
    let mut y = x.clone();
    let half = (sr * 0.5) as usize;
    for (k, seg) in y.chunks_mut(half).enumerate() {
        if k > 0 {
            c.set_param("ir_a", if k % 2 == 1 { 2.0 } else { 1.0 });
        }
        c.process(&mut [seg]);
    }
    // levels of each IR alone
    let lvl = |h: &[f32]| {
        let mut c = cab(sr, 1, &[(1, h.to_vec())], &[("ir_a", 1.0), ("normalize", 0.0)]);
        rms(&render(&mut c, &x, 512)[half..])
    };
    let floor = lvl(&a).min(lvl(&b));
    let w = (0.005 * sr) as usize;
    let worst = y[half / 2..].chunks(w).map(|s| db(rms(s) / floor)).fold(f64::MAX, f64::min);
    eprintln!("cab: swap every 500 ms, lowest 5 ms window {worst:.2} dB re the quieter IR");
    assert!(worst > -1.0);
    // no step: HF (> 10 kHz) after each swap no higher than −60 dB re the signal
    let hp = hp10k(&y, sr);
    let pk = y.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let steady = hp[half / 2..half].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let burst = hp[half..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let excess = db(((burst - steady).max(0.0) / pk) as f64);
    eprintln!("cab: swap HF excess {excess:.1} dB re peak");
    assert!(excess < -60.0);
}

#[test]
fn no_cold_start_through_the_cab() {
    // a 100 ms IR, then a 2 s one: once every level has crossfaded, the full convolution of the
    // whole input history
    let sr = 48000.0;
    let short = decaying(4800, 1, 800.0);
    let long = decaying(96000, 2, 20000.0);
    let x = noise(200000, 3, 0.3);
    let mut set = raw_settings().to_vec();
    set.push(("ir_a", 1.0));
    let mut c = cab(sr, 1, &[(1, short), (2, long.clone())], &set);
    let mut y = x.clone();
    let at = 70000;
    c.process(&mut [&mut y[..at]]);
    c.set_param("ir_a", 2.0);
    c.process(&mut [&mut y[at..]]);
    let settled = at + 2 * 8192 + 64 + (0.031 * sr) as usize;
    let e = err_db(&y[settled..], &fft_conv(&x, &long)[settled..]);
    eprintln!("cab: after a swap to a 2 s IR, from its last level on: {e:.1} dB re output");
    assert!(e < -110.0);
    assert_eq!(c.core_ref().late_count(), 0);
}

#[test]
fn failures_keep_playing() {
    let sr = 48000.0;
    let h = decaying(4800, 1, 800.0);
    let mut c = cab(sr, 1, &[(1, h.clone())], &[("ir_a", 1.0)]);
    let x = noise(48000, 2, 0.3);
    let before = render(&mut c, &x, 512);
    c.set_param("ir_a", 99.0);
    assert_eq!(c.loaded(0), -1.0);
    let after = render(&mut c, &x, 512);
    assert!(rms(&after) > 0.5 * rms(&before), "the previous IR keeps playing");
    // the previous slot again: meter back at once, no load
    let preps = ir_host::preparations();
    c.set_param("ir_a", 1.0);
    assert_eq!(c.loaded(0), 1.0);
    assert_eq!(ir_host::preparations(), preps);
    // a slot written before its IR exists loads when it arrives
    c.set_param("ir_a", 5.0);
    assert_eq!(c.loaded(0), -1.0);
    c.load_ir(5, &[decaying(2000, 9, 300.0)], sr);
    assert_eq!(c.loaded(0), 5.0);
}

#[test]
fn correlation_aligns_a_pair_through_the_cab() {
    // B = −A delayed 1.37 ms: after the onset trim and the correlation, B is A again (sign and
    // lag recovered), so a 50 / 50 blend is A itself; the cab plays exactly the prepared pair
    let sr = 48000.0;
    let a = decaying(4800, 21, 500.0);
    let lag = (0.00137 * sr) as usize;
    let mut b = vec![0.0f32; lag];
    b.extend(a.iter().map(|v| -v));
    let to64 = |h: &[f32]| Source::mono(sr as f64, h.iter().map(|v| *v as f64).collect());
    let s = cab_settings(Align::Correlation, Normalize::Loudness);
    let (pa, pb, al) = ir::prepare_pair(&to64(&a), &to64(&b), sr as f64, &s);
    let al = al.unwrap();
    // the onset trim and the auto-polarity already gave B the sign of A: the correlation need not flip
    assert!(!al.flip);
    let n = pa[0].len().max(pb[0].len());
    let get = |v: &[f32], k: usize| *v.get(k).unwrap_or(&0.0) as f64;
    let diff: f64 = (0..n).map(|k| (get(&pa[0], k) - get(&pb[0], k)).powi(2)).sum();
    let pow: f64 = (0..n).map(|k| get(&pa[0], k).powi(2)).sum();
    eprintln!("cab: Correlation, B = −A late by 1.37 ms: residual lag {:.3} samples, B vs A after alignment {:.1} dB", al.lag, 10.0 * (diff / pow).log10());
    assert!(10.0 * (diff / pow).log10() < -60.0);
    let x = noise(48000, 1, 0.3);
    let mut c = cab(sr, 1, &[(1, a), (2, b)], &[("ir_a", 1.0), ("ir_b", 2.0), ("blend", 50.0), ("align", 2.0)]);
    let y = render(&mut c, &x, 512);
    let mix: Vec<f32> = (0..n).map(|k| (0.5 * (get(&pa[0], k) + get(&pb[0], k))) as f32).collect();
    let e = err_db(&y, &fft_conv(&x, &mix));
    assert!(e < -110.0, "{e}");
}

#[test]
fn delay_b_static_and_sweep() {
    let sr = 48000.0;
    // B alone (blend 100), δ, half a sample late: magnitude within 0.5 dB to 10 kHz
    let set = [("ir_a", 1.0), ("ir_b", 1.0), ("blend", 100.0), ("align", 0.0), ("normalize", 0.0), ("delay_b", 0.5 / 48.0)];
    let mut c = cab(sr, 1, &[(1, vec![1.0])], &set);
    let mut imp = vec![0.0f32; 512];
    imp[0] = 1.0;
    let h: Vec<f64> = render(&mut c, &imp, 512).iter().map(|v| *v as f64).collect();
    let worst = [1000.0, 5000.0, 8000.0, 10000.0].iter().map(|&f| mag_db(&h, sr as f64, f).abs()).fold(0.0, f64::max);
    eprintln!("cab: delay_b at half a sample: worst {worst:.2} dB to 10 kHz");
    assert!(worst < 0.5);
    // a ±10 ms sweep in 2 s under a sine: no click
    let n = (sr * 2.5) as usize;
    let x = sine(n, 1000.0, 0.25, sr);
    let mut c = cab(sr, 1, &[(1, vec![1.0])], &set[..5]);
    let mut y = x.clone();
    let steps = 200;
    let per = (2.0 * sr) as usize / steps;
    for (k, b) in y.chunks_mut(per).enumerate() {
        let d = -10.0 + 20.0 * (k.min(steps) as f32 / steps as f32);
        c.set_param("delay_b", d);
        c.process(&mut [b]);
    }
    let hp = hp10k(&y, sr);
    let pk = y.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let worst = hp[4800..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    eprintln!("cab: delay_b sweep ±10 ms in 2 s, HF {:.1} dB re peak", db((worst / pk) as f64));
    assert!(db((worst / pk) as f64) < -50.0);
}

#[test]
fn bass_keep_sum_is_flat_with_a_dirac() {
    let sr = 48000.0;
    let mut c = cab(sr, 1, &[(1, vec![1.0])], &[("ir_a", 1.0), ("align", 0.0), ("normalize", 0.0), ("bass_keep", 120.0)]);
    let mut imp = vec![0.0f32; 48000];
    imp[0] = 1.0;
    let h: Vec<f64> = render(&mut c, &imp, 512).iter().map(|v| *v as f64).collect();
    let worst = [30.0, 60.0, 100.0, 120.0, 150.0, 300.0, 1000.0, 5000.0].iter().map(|&f| mag_db(&h, sr as f64, f).abs()).fold(0.0, f64::max);
    eprintln!("cab: bass_keep 120 Hz with IR = δ: flat within {worst:.3} dB");
    assert!(worst < 0.1);
}

#[test]
fn builtin_plays_at_the_reference() {
    for sr in [44100.0, 48000.0, 96000.0, 192000.0] {
        let s = builtin::render(sr);
        let p = ir::weighted_power(&s.channels, sr, builtin::BAND);
        assert!((10.0 * p.log10() - builtin::REF_DB).abs() < 1e-6);
        // Loudness leaves it where it is (Off and Loudness play alike)
        let prep = ir::prepare(&s, sr, &cab_settings(Align::Onset, Normalize::Loudness));
        let off = ir::prepare(&s, sr, &cab_settings(Align::Onset, Normalize::Off));
        let d = prep[0].iter().zip(&off[0]).fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
        assert!(d < 1e-5, "{d}");
        assert_eq!(s.channels[0].len(), (0.1 * sr) as usize);
    }
}

#[test]
fn stereo_split_and_linked() {
    let sr = 48000.0;
    let a = decaying(2400, 1, 300.0);
    let b = decaying(2400, 2, 300.0);
    let l = noise(24000, 3, 0.3);
    let r = noise(24000, 4, 0.3);
    let base = [("ir_a", 1.0), ("ir_b", 2.0), ("align", 0.0), ("normalize", 0.0), ("blend", 50.0)];
    let mut set = base.to_vec();
    set.push(("stereo", 1.0));
    let mut c = cab(sr, 2, &[(1, a.clone()), (2, b.clone())], &set);
    let (mut yl, mut yr) = (l.clone(), r.clone());
    for (x, y) in yl.chunks_mut(512).zip(yr.chunks_mut(512)) {
        c.process(&mut [x, y]);
    }
    // Split at 50 %: L = A(L), R = B(R), both at unity
    assert!(err_db(&yl, &fft_conv(&l, &a)) < -110.0);
    assert!(err_db(&yr, &fft_conv(&r, &b)) < -110.0);
    // Linked: (A + B) / 2 on both
    let mut c = cab(sr, 2, &[(1, a.clone()), (2, b.clone())], &base);
    let (mut yl, mut yr) = (l.clone(), r.clone());
    for (x, y) in yl.chunks_mut(512).zip(yr.chunks_mut(512)) {
        c.process(&mut [x, y]);
    }
    let ab: Vec<f32> = a.iter().zip(&b).map(|(p, q)| 0.5 * (p + q)).collect();
    assert!(err_db(&yl, &fft_conv(&l, &ab)) < -110.0);
    assert!(err_db(&yr, &fft_conv(&r, &ab)) < -110.0);
}

#[test]
fn length_and_fade_trim_without_ripple() {
    // a 200 ms cab-like IR (the built-in's colour on a noise decay that is still at −20 dB at
    // 40 ms) cut at 40 ms with a 20 % fade: above 200 Hz its 1/12-octave magnitude never rises more
    // than 0.5 dB over the whole IR's
    let sr = 48000.0f64;
    let n = (0.2 * sr) as usize;
    let tone = builtin::render(sr).channels.remove(0);
    let mut r = rng(31);
    let dec: Vec<f32> = (0..n).map(|k| r() * (-(k as f32) / (0.0087 * sr as f32)).exp()).collect();
    let tone32: Vec<f32> = tone.iter().map(|v| *v as f32).collect();
    let h: Vec<f64> = fft_conv(&dec, &tone32);
    let src = Source::mono(sr, h.clone());
    let smooth = |x: &[f64], f: f64| {
        let p: f64 = (0..12).map(|i| 10f64.powf(mag_db(x, sr, f * 2f64.powf((i as f64 - 5.5) / 12.0 / 12.0)) / 10.0)).sum::<f64>() / 12.0;
        10.0 * p.log10()
    };
    let rise = |fade: f64| {
        let s = Settings { length_s: 0.04, fade, ..cab_settings(Align::Raw, Normalize::Off) };
        let cut: Vec<f64> = ir::prepare(&src, sr, &s)[0].iter().map(|v| *v as f64).collect();
        let mut worst = f64::MIN;
        let mut f = 200.0;
        while f < 16000.0 {
            worst = worst.max(smooth(&cut, f) - smooth(&h, f));
            f *= 1.05;
        }
        worst
    };
    let (hard, faded) = (rise(0.0), rise(0.2));
    eprintln!("cab: 200 ms IR cut at 40 ms: rises {hard:.2} dB over the whole IR hard, {faded:.2} dB with a 20 % fade");
    assert!(faded < 0.5);
}

#[test]
fn parameter_round_trip_through_the_normalised_value() {
    // nih-plug's IntRange::Linear in f32, as REAPER writes (v − min) / (max − min)
    for slot in [1i32, 2, 255, 256, 32767, 32768, 65534, 65535] {
        let n = slot as f32 / 65535.0;
        let back = (n * 65535.0).round() as i32;
        assert_eq!(back, slot);
    }
    let pi = PARAMS.iter().find(|p| p.id == "ir_a").unwrap();
    assert_eq!((pi.min, pi.max), (0.0, 65535.0));
}

// ---- plumbing: the wrapper's path, without nih-plug ----

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gfx-cab-test-{}-{name}", std::process::id()));
    let _ = std::fs::create_dir_all(&d);
    d
}

/// A plugin-mode instance: the DSP in async mode, its host, a worker on a fixed index.
fn plugin(index: &Path) -> (Cab, Host, Arc<Worker>) {
    let mut loader = Loader::new(Some(builtin::render));
    loader.index = Some(index.to_path_buf());
    let (h, w) = ir_engine::pair(loader);
    let mut c = Cab::new();
    c.core().set_async();
    (c, h, w)
}

/// `initialize()` as the wrapper does it.
fn initialize(c: &mut Cab, h: &mut Host, w: &Worker, sr: f32) {
    c.prepare(sr, 512, 1);
    let want = c.want();
    if !c.core_ref().installed_matches(&want) {
        let job = c.core_ref().job(&want, 0);
        let done = w.run_now(&job);
        c.core().apply(done, true);
    }
    h.settle(want);
    w.collect();
}

/// One host block: tick (jobs collected), process.
fn block(c: &mut Cab, h: &mut Host, jobs: &mut Vec<Job>, x: &mut [f32]) {
    let now = c.clock();
    h.tick(c.core(), now, |j| jobs.push(j));
    c.process(&mut [x]);
}

#[test]
fn index_slot_meter_and_background_load() {
    let d = tmp("idx");
    let f = d.join("cab.wav");
    ir_host::write_wav(&f, 44100, &[decaying(4000, 3, 600.0)]).unwrap();
    let idx = d.join("ir-index.json");
    ir_host::write_index(&idx, &[(65535, &f)]).unwrap();
    let (mut c, mut h, w) = plugin(&idx);
    initialize(&mut c, &mut h, &w, 48000.0);
    assert_eq!(c.loaded(0), 0.0, "the built-in");
    let mut jobs = Vec::with_capacity(16);
    let mut x = vec![0.0f32; 128];
    c.set_param("ir_a", 65535.0);
    block(&mut c, &mut h, &mut jobs, &mut x);
    assert_eq!(jobs.len(), 1, "posted at once");
    assert_eq!(c.loaded(0), 0.0, "previous value while pending");
    for j in jobs.drain(..) {
        w.run(&j);
    }
    block(&mut c, &mut h, &mut jobs, &mut x);
    assert_eq!(c.loaded(0), 65535.0);
    // unknown slot: −1, re-asked every 0.5 s, loads once its entry exists
    c.set_param("ir_a", 300.0);
    block(&mut c, &mut h, &mut jobs, &mut x);
    for j in jobs.drain(..) {
        w.run(&j);
    }
    block(&mut c, &mut h, &mut jobs, &mut x);
    assert_eq!(c.loaded(0), -1.0);
    ir_host::write_index(&idx, &[(65535, &f), (300, &f)]).unwrap();
    let mut waited = 0;
    while c.loaded(0) != 300.0 && waited < 400 {
        block(&mut c, &mut h, &mut jobs, &mut x);
        for j in jobs.drain(..) {
            w.run(&j);
        }
        waited += 1;
    }
    let t = waited as f64 * 128.0 / 48000.0;
    eprintln!("cab: slot written before its entry: loaded {t:.2} s after the entry");
    assert!(t <= 0.6);
    // hash mismatch: −2, the IR keeps playing
    let g = d.join("bad.wav");
    ir_host::write_wav(&g, 48000, &[vec![0.5; 100]]).unwrap();
    ir_host::write_index(&idx, &[(65535, &f), (300, &f), (7, &g)]).unwrap();
    std::fs::write(&g, b"RIFF").unwrap();
    c.set_param("ir_a", 7.0);
    block(&mut c, &mut h, &mut jobs, &mut x);
    for j in jobs.drain(..) {
        w.run(&j);
    }
    block(&mut c, &mut h, &mut jobs, &mut x);
    assert_eq!(c.loaded(0), -2.0);
    // the previous slot again: meter back at once, no job
    c.set_param("ir_a", 300.0);
    block(&mut c, &mut h, &mut jobs, &mut x);
    assert!(jobs.is_empty());
    assert_eq!(c.loaded(0), 300.0);
}

#[test]
fn index_rewritten_during_loads() {
    let d = tmp("race");
    let f = d.join("a.wav");
    ir_host::write_wav(&f, 48000, &[decaying(3000, 8, 500.0)]).unwrap();
    let idx = d.join("ir-index.json");
    ir_host::write_index(&idx, &[(1, &f)]).unwrap();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (i2, f2, s2) = (idx.clone(), f.clone(), stop.clone());
    let writer = std::thread::spawn(move || {
        let mut n = 0;
        while n < 1000 && !s2.load(std::sync::atomic::Ordering::Relaxed) {
            ir_host::write_index(&i2, &[(1, &f2)]).unwrap();
            n += 1;
        }
        n
    });
    let mut l = Loader::new(None);
    l.index = Some(idx);
    let mut fails = 0;
    for k in 0..300 {
        let mut s = cab_settings(Align::Onset, Normalize::Loudness);
        s.fade = 0.1 + k as f64 * 1e-6; // a fresh preparation each time
        let job = Job { gen: 1, voices: [ir_host::SlotSource::Slot(1), ir_host::SlotSource::None], nvoices: 1, settings: s, sample_rate: 48000.0 };
        if l.run(&job).results[0].is_err() {
            fails += 1;
        }
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let writes = writer.join().unwrap();
    eprintln!("cab: {writes} index rewrites during 300 loads: {fails} failed");
    assert_eq!(fails, 0);
}

#[test]
fn activation_reuses_kernels_and_shared_cache() {
    let d = tmp("act");
    let f = d.join("a.wav");
    ir_host::write_wav(&f, 48000, &[decaying(6000, 12, 900.0)]).unwrap();
    let idx = d.join("ir-index.json");
    ir_host::write_index(&idx, &[(4, &f)]).unwrap();
    let (mut c, mut h, w) = plugin(&idx);
    c.set_param("ir_a", 4.0);
    initialize(&mut c, &mut h, &w, 48000.0);
    assert_eq!(c.loaded(0), 4.0, "initialize() loads synchronously on a miss");
    let k = c.core_ref().kernel(0).cloned().unwrap();
    // a second instance on the same IR: the same kernel, from the process cache
    let (mut c2, mut h2, w2) = plugin(&idx);
    c2.set_param("ir_a", 4.0);
    initialize(&mut c2, &mut h2, &w2, 48000.0);
    assert!(Arc::ptr_eq(&k, c2.core_ref().kernel(0).unwrap()), "one kernel in memory for two instances");
    // with the file and the index gone, deactivate / activate × 20 still plays the same kernel: no
    // I/O and no preparation
    std::fs::remove_file(&f).unwrap();
    std::fs::remove_file(&idx).unwrap();
    for _ in 0..20 {
        initialize(&mut c, &mut h, &w, 48000.0);
        c.reset();
        assert_eq!(c.loaded(0), 4.0);
        assert!(Arc::ptr_eq(&k, c.core_ref().kernel(0).unwrap()));
    }
    let mut x = noise(64, 1, 0.3);
    c.process(&mut [&mut x]);
    assert!(rms(&x) > 0.0, "processing within the first block");
    // removing one instance leaves the other playing
    drop(c);
    let mut x = noise(4096, 1, 0.3);
    c2.process(&mut [&mut x]);
    assert!(rms(&x) > 0.0);
}

#[test]
fn render_determinism_with_slots_set_before_initialize() {
    let d = tmp("det");
    let f = d.join("a.wav");
    ir_host::write_wav(&f, 96000, &[decaying(20000, 13, 3000.0)]).unwrap();
    let idx = d.join("ir-index.json");
    ir_host::write_index(&idx, &[(9, &f)]).unwrap();
    let x = noise(48000, 2, 0.3);
    let run = |block: usize| {
        let (mut c, mut h, w) = plugin(&idx);
        c.set_param("ir_a", 9.0);
        initialize(&mut c, &mut h, &w, 48000.0);
        c.reset();
        let mut y = x.clone();
        let mut jobs = Vec::new();
        for b in y.chunks_mut(block) {
            let now = c.clock();
            h.tick(c.core(), now, |j| jobs.push(j));
            c.process(&mut [b]);
        }
        assert!(jobs.is_empty());
        y
    };
    assert!(run(64) == run(1000), "identical from sample 0 at two block sizes");
}

/// nih-plug's `assert_process_allocs` allocator is this binary's global allocator (plugin feature):
/// an allocation or a free inside `assert_no_alloc` aborts the test.
#[cfg(feature = "plugin")]
#[test]
fn no_allocation_in_process_over_50_swaps() {
    let d = tmp("alloc");
    let fa = d.join("a.wav");
    let fb = d.join("b.wav");
    ir_host::write_wav(&fa, 48000, &[decaying(9600, 1, 1500.0)]).unwrap();
    ir_host::write_wav(&fb, 44100, &[decaying(30000, 2, 6000.0)]).unwrap();
    let idx = d.join("ir-index.json");
    ir_host::write_index(&idx, &[(1, &fa), (2, &fb)]).unwrap();
    let (mut c, mut h, w) = plugin(&idx);
    c.set_param("ir_b", 2.0);
    c.set_param("blend", 30.0);
    initialize(&mut c, &mut h, &w, 48000.0);
    let mut jobs: Vec<Job> = Vec::with_capacity(256);
    let mut x = noise(256, 3, 0.3);
    let mut installed = 0;
    for k in 0..50 {
        c.set_param("ir_a", if k % 2 == 0 { 1.0 } else { 2.0 });
        c.set_param("align", (k % 3) as f32);
        for _ in 0..40 {
            assert_no_alloc::assert_no_alloc(|| {
                let now = c.clock();
                h.tick(c.core(), now, |j| jobs.push(j));
                c.process(&mut [&mut x]);
            });
            for j in jobs.drain(..) {
                w.run(&j);
            }
            w.collect();
        }
        installed += (c.loaded(0) == if k % 2 == 0 { 1.0 } else { 2.0 }) as usize;
    }
    assert_eq!(installed, 50, "every swap installed");
}

// ---- golem's library ----

/// Groups of captures of one cab (several mics or positions).
fn same_cab_groups(root: &Path) -> Vec<Vec<PathBuf>> {
    let mut g = Vec::new();
    for sub in ["maestrium/t3k-45022", "maestrium/t3k-45023", "resington/47185_dauntless-ir-pack-celestion-v-type-prs-cab", "primalnerd/70612_glockenklang-duo-2x10"] {
        g.push(wavs(&root.join(sub)));
    }
    let mut palmer = wavs(&root.join("elgringoloco/5004_palmer-2x12-g12m-65-creamback---shure-sm57"));
    palmer.extend(wavs(&root.join("elgringoloco/5006_palmer-2x12-g12m-65-creamback---aston-origin")));
    g.push(palmer);
    g.into_iter().filter(|v| v.len() >= 2).collect()
}

#[test]
fn polarity_on_real_pairs() {
    let Some(root) = assets() else { return };
    let s = cab_settings(Align::Onset, Normalize::Loudness);
    let sr = 48000.0;
    let (mut pairs, mut opposite) = (0, 0);
    for group in same_cab_groups(&root) {
        let shaped: Vec<Vec<f64>> = group.iter().filter_map(|p| read(p)).map(|src| ir::shape(&src, sr, &s).remove(0)).collect();
        for i in 0..shaped.len() {
            for j in i + 1..shaped.len() {
                pairs += 1;
                if ir::align_correlation(&shaped[i], &shaped[j], sr).flip {
                    opposite += 1;
                }
            }
        }
    }
    eprintln!("cab: auto-polarity on {pairs} same-cab pairs of golem's library: {opposite} left opposite");
    assert!(pairs > 100);
    assert_eq!(opposite, 0);
}

#[test]
fn loudness_on_the_library() {
    let Some(root) = assets() else { return };
    let mut files = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                // effect IRs (echo, reverb) and bass preamp captures are not cabs
                let n = p.to_string_lossy();
                if !n.contains("roland-re-301") && !n.contains("arlingtonaudio") {
                    stack.push(p);
                }
            } else if p.extension().is_some_and(|x| x == "wav") {
                files.push(p);
            }
        }
    }
    files.sort();
    let sr = 48000.0;
    let mut raw = Vec::new();
    let mut after = Vec::new();
    let x = {
        // pink noise (Kellet's filter) band-limited to the reference band: what the law aims at
        let w = noise(96000, 5, 0.3);
        let mut f = [Svf::new(), Svf::new(), Svf::new(), Svf::new()];
        f[0].set(SvfMode::HighPass, 80.0, 0.541, 0.0, sr as f32);
        f[1].set(SvfMode::HighPass, 80.0, 1.307, 0.0, sr as f32);
        f[2].set(SvfMode::LowPass, 6000.0, 0.541, 0.0, sr as f32);
        f[3].set(SvfMode::LowPass, 6000.0, 1.307, 0.0, sr as f32);
        let mut b = [0.0f64; 7];
        w.iter()
            .map(|&v| {
                let x = v as f64;
                b[0] = 0.99886 * b[0] + x * 0.0555179;
                b[1] = 0.99332 * b[1] + x * 0.0750759;
                b[2] = 0.96900 * b[2] + x * 0.1538520;
                b[3] = 0.86650 * b[3] + x * 0.3104856;
                b[4] = 0.55000 * b[4] + x * 0.5329522;
                b[5] = -0.7616 * b[5] - x * 0.0168980;
                let p = b.iter().sum::<f64>() + x * 0.5362;
                b[6] = x * 0.115926;
                f.iter_mut().fold(p as f32, |y, s| s.process(y))
            })
            .collect::<Vec<f32>>()
    };
    let xr = rms(&x);
    for p in &files {
        let Some(src) = read(p) else { continue };
        let off = ir::prepare(&src, sr, &cab_settings(Align::Onset, Normalize::Off));
        let raw_db = 10.0 * ir::weighted_power(&off.iter().map(|c| c.iter().map(|v| *v as f64).collect()).collect::<Vec<_>>(), sr, builtin::BAND).log10();
        raw.push(raw_db);
        let loud = ir::prepare(&src, sr, &cab_settings(Align::Onset, Normalize::Loudness));
        let y = fft_conv(&x, &loud[0]);
        let yr = (y[24000..].iter().map(|v| v * v).sum::<f64>() / (y.len() - 24000) as f64).sqrt();
        after.push(db(yr / xr));
    }
    raw.sort_by(|a, b| a.partial_cmp(b).unwrap());
    after.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = raw[raw.len() / 2];
    let spread = |v: &[f64]| v[v.len() * 95 / 100] - v[v.len() * 5 / 100];
    eprintln!(
        "cab: {} cab IRs of golem's library: raw power gain median {med:+.1} dB, 5 – 95 % spread {:.1} dB; after Loudness spread {:.2} dB on band-limited pink noise",
        raw.len(),
        spread(&raw),
        spread(&after)
    );
    assert!(raw.len() > 60);
    assert!(spread(&after) < 1.0);
}

#[test]
fn real_cab_through_the_engine() {
    // a library cab at 44.1 kHz into a 48 kHz cab: resampled, trimmed, normalised; accuracy of the
    // engine against the prepared IR, and the CPU of one and two IRs
    let Some(root) = assets() else { return };
    let files = wavs(&root.join("resington/47185_dauntless-ir-pack-celestion-v-type-prs-cab"));
    let (Some(a), Some(b)) = (files.first().and_then(|p| read(p)), files.get(1).and_then(|p| read(p))) else { return };
    let sr = 48000.0f32;
    let to32 = |s: &Source| s.channels[0].iter().map(|v| *v as f32).collect::<Vec<f32>>();
    let mut c = Cab::new();
    c.load_ir(1, &[to32(&a)], a.rate as f32);
    c.load_ir(2, &[to32(&b)], b.rate as f32);
    c.prepare(sr, 512, 1);
    c.set_param("ir_a", 1.0);
    c.reset();
    let x = noise(48000, 7, 0.3);
    let y = render(&mut c, &x, 128);
    let prepared = ir::prepare(&a, sr as f64, &cab_settings(Align::Onset, Normalize::Loudness));
    let e = err_db(&y, &fft_conv(&x, &prepared[0]));
    eprintln!("cab: library cab (44.1 kHz file) through the engine: {e:.1} dB re output");
    assert!(e < -110.0);
}

#[test]
fn cpu_and_spikes() {
    // 200 ms cab, 48 kHz mono, blocks of 64: mean per sample and the worst block
    let measure = |sr: f32, secs: f32, ch: usize, two: bool| {
        let len = (secs * sr) as usize;
        let mut c = Cab::new();
        c.load_ir(1, &[decaying(len, 1, len as f32 / 5.0)], sr);
        c.load_ir(2, &[decaying(len, 2, len as f32 / 5.0)], sr);
        c.prepare(sr, 64, ch);
        c.set_param("ir_a", 1.0);
        if two {
            c.set_param("ir_b", 2.0);
            c.set_param("blend", 40.0);
        }
        c.reset();
        let n = (sr * 4.0) as usize;
        let x = noise(n, 3, 0.3);
        let mut bufs: Vec<Vec<f32>> = (0..ch).map(|_| x.clone()).collect();
        let mut times = Vec::with_capacity(n / 64);
        let mut i = 0;
        while i + 64 <= n {
            let mut sl: Vec<&mut [f32]> = bufs.iter_mut().map(|b| &mut b[i..i + 64]).collect();
            let t = Instant::now();
            c.process(&mut sl);
            times.push(t.elapsed().as_secs_f64());
            i += 64;
        }
        let skip = times.len() / 4;
        let t = &times[skip..];
        let mean = t.iter().sum::<f64>() / t.len() as f64;
        let mut sorted = t.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // the 99.9th percentile: one-off OS preemptions are not the engine's spikes
        let p999 = sorted[sorted.len() * 999 / 1000];
        let period = 64.0 / sr as f64;
        (mean / period * 100.0, p999 / mean, p999 / period * 100.0)
    };
    for (sr, secs, ch, two, budget) in [(48000.0, 0.2, 1, false, 1.0), (48000.0, 0.2, 1, true, 1.7), (48000.0, 2.0, 1, false, 2.0), (192000.0, 0.2, 2, true, 12.0)] {
        let (cpu, spike, spike_pct) = measure(sr, secs, ch, two);
        eprintln!("cab: CPU {secs} s IR{} {}ch {sr} Hz: {cpu:.2} % of a core (budget {budget} %); worst block (99.9 %) {spike:.1}× the mean, {spike_pct:.1} % of the period", if two { "s A+B" } else { "" }, ch);
    }
}
