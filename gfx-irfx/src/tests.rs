// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (gfx-cab README §5, the gfx-irfx rows): accuracy up to the 20 s class limit, the
//! channel mapping, predelay, reverse, width, trails, and golem's effect IRs (`~/golem/assets/irs`,
//! skipped when absent).

use crate::{IrFx, BAND, REF_DB};
use gfx_dsp::ir::{self, Align, ChannelMap, Normalize, Settings};
use gfx_dsp::ir_host;
use gfx_dsp::Effect;
use realfft::RealFftPlanner;
use std::path::PathBuf;

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

fn decaying(len: usize, seed: u64, tau: f32) -> Vec<f32> {
    let mut r = rng(seed);
    (0..len).map(|k| r() * (-(k as f32) / tau).exp()).collect()
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

/// An instance with IRs registered and settings applied; wet at 0 dB and the dry off unless set.
fn fx(sr: f32, ch: usize, irs: &[(u32, Vec<Vec<f32>>)], set: &[(&str, f32)]) -> IrFx {
    let mut f = IrFx::new();
    for (slot, h) in irs {
        f.load_ir(*slot, h, sr);
    }
    f.prepare(sr, 512, ch);
    f.set_param("wet", 0.0);
    f.set_param("dry", -60.0);
    f.set_param("normalize", 0.0);
    for &(id, v) in set {
        f.set_param(id, v);
    }
    f.reset();
    f
}

fn render2(f: &mut IrFx, l: &[f32], r: &[f32], block: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut a, mut b) = (l.to_vec(), r.to_vec());
    for (x, y) in a.chunks_mut(block).zip(b.chunks_mut(block)) {
        f.process(&mut [x, y]);
    }
    (a, b)
}

#[test]
fn defaults_pass_the_dry() {
    // slot 0 = none: the input at unity, exactly (no IR, no latency)
    let mut f = IrFx::new();
    f.prepare(48000.0, 512, 2);
    let l = noise(10000, 1, 0.3);
    let r = noise(10000, 2, 0.3);
    let (a, b) = render2(&mut f, &l, &r, 333);
    assert_eq!(a, l);
    assert_eq!(b, r);
    assert_eq!(f.loaded(), 0.0);
}

#[test]
fn accuracy_up_to_the_class_limit() {
    let sr = 48000.0;
    for (secs, seed) in [(0.2f32, 1u64), (2.0, 2), (20.0, 3)] {
        let len = (secs * sr) as usize;
        let h = decaying(len, seed, len as f32 / 4.0);
        let x = noise(len.min(96000) + 48000, 9, 0.3);
        let mut f = fx(sr, 1, &[(1, vec![h.clone()])], &[("ir_a", 1.0)]);
        let mut y = x.clone();
        for b in y.chunks_mut(512) {
            f.process(&mut [b]);
        }
        let e = err_db(&y, &fft_conv(&x, &h));
        eprintln!("irfx: accuracy, {secs} s IR: {e:.1} dB re output");
        assert!(e < -110.0);
        assert_eq!(f.core_ref().late_count(), 0);
    }
}

#[test]
fn channel_mapping() {
    let sr = 48000.0;
    let hl = decaying(3000, 1, 400.0);
    let hr = decaying(3000, 2, 400.0);
    let l = noise(20000, 3, 0.3);
    let r = noise(20000, 4, 0.3);
    // 2 channels: L → ch 1, R → ch 2
    let mut f = fx(sr, 2, &[(1, vec![hl.clone(), hr.clone()])], &[("ir_a", 1.0)]);
    let (a, b) = render2(&mut f, &l, &r, 256);
    assert!(err_db(&a, &fft_conv(&l, &hl)) < -110.0);
    assert!(err_db(&b, &fft_conv(&r, &hr)) < -110.0);
    // 1 channel: the same IR on both sides
    let mut f = fx(sr, 2, &[(1, vec![hl.clone()])], &[("ir_a", 1.0)]);
    let (a, b) = render2(&mut f, &l, &r, 256);
    assert!(err_db(&a, &fft_conv(&l, &hl)) < -110.0);
    assert!(err_db(&b, &fft_conv(&r, &hl)) < -110.0);
    // 1 / 1: the two channels averaged
    let mut f = fx(sr, 1, &[(1, vec![hl.clone(), hr.clone()])], &[("ir_a", 1.0)]);
    let mut y = l.clone();
    for c in y.chunks_mut(256) {
        f.process(&mut [c]);
    }
    let avg: Vec<f32> = hl.iter().zip(&hr).map(|(a, b)| 0.5 * (a + b)).collect();
    assert!(err_db(&y, &fft_conv(&l, &avg)) < -110.0);
    // 4 channels: 1 and 4
    let h4 = vec![hl.clone(), decaying(3000, 7, 400.0), decaying(3000, 8, 400.0), hr.clone()];
    let mut f = fx(sr, 2, &[(1, h4)], &[("ir_a", 1.0)]);
    let (a, b) = render2(&mut f, &l, &r, 256);
    assert!(err_db(&a, &fft_conv(&l, &hl)) < -110.0);
    assert!(err_db(&b, &fft_conv(&r, &hr)) < -110.0);
}

#[test]
fn predelay_reverse_width() {
    let sr = 48000.0;
    let h = decaying(4800, 5, 900.0);
    let x = noise(30000, 6, 0.3);
    // predelay 10 ms = 480 samples: the wet delayed exactly
    let mut f = fx(sr, 1, &[(1, vec![h.clone()])], &[("ir_a", 1.0), ("predelay", 10.0)]);
    let mut y = x.clone();
    for c in y.chunks_mut(256) {
        f.process(&mut [c]);
    }
    let mut hd = vec![0.0f32; 480];
    hd.extend_from_slice(&h);
    assert!(err_db(&y, &fft_conv(&x, &hd)) < -110.0);
    // reverse: the IR reversed (after the tail trim)
    let mut f = fx(sr, 1, &[(1, vec![h.clone()])], &[("ir_a", 1.0), ("reverse", 1.0)]);
    let mut y = x.clone();
    for c in y.chunks_mut(256) {
        f.process(&mut [c]);
    }
    let mut hr = h.clone();
    hr.reverse();
    assert!(err_db(&y, &fft_conv(&x, &hr)) < -100.0);
    // width 0 %: both sides carry the mid of the wet
    let hb = decaying(4800, 6, 900.0);
    let mut f = fx(sr, 2, &[(1, vec![h.clone(), hb])], &[("ir_a", 1.0), ("width", 0.0)]);
    let (a, b) = render2(&mut f, &x, &x, 256);
    assert_eq!(a, b);
}

#[test]
fn trails_ring_out_on_bypass() {
    let sr = 48000.0;
    let h = decaying(48000, 7, 9600.0);
    let x = noise(48000, 8, 0.3);
    let mut f = fx(sr, 1, &[(1, vec![h])], &[("ir_a", 1.0), ("dry", 0.0)]);
    let mut y = x.clone();
    let (a, b) = y.split_at_mut(24000);
    f.process(&mut [a]);
    f.set_param("bypass", 1.0);
    f.process(&mut [b]);
    // 100 ms after the bypass: the input passes at unity and the tail still rings on top of it
    let tail: Vec<f32> = y[30000..].iter().zip(&x[30000..]).map(|(p, q)| p - q).collect();
    eprintln!("irfx: trails, tail 125 ms after bypass at {:.1} dB re input", 20.0 * (rms(&tail) / rms(&x)).log10());
    assert!(rms(&tail) > 0.05 * rms(&x));
    // trails off: the input alone after 10 ms
    let mut f = fx(sr, 1, &[(1, vec![decaying(48000, 7, 9600.0)])], &[("ir_a", 1.0), ("dry", 0.0), ("trails", 0.0)]);
    let mut y = x.clone();
    let (a, b) = y.split_at_mut(24000);
    f.process(&mut [a]);
    f.set_param("bypass", 1.0);
    f.process(&mut [b]);
    assert_eq!(&y[24000 + 480 + 64..], &x[24000 + 480 + 64..]);
}

fn assets() -> Option<PathBuf> {
    let d = PathBuf::from(std::env::var_os("HOME")?).join("golem/assets/irs/aaaa1333/101878_roland-re-301");
    d.is_dir().then_some(d)
}

#[test]
fn effect_irs_of_the_library() {
    // the tape echo / reverb captures (stereo, 1.5 – 9 s): raw levels, Loudness at 0 dB, and one of
    // them through the engine against its prepared IR
    let Some(dir) = assets() else { return };
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "wav")).collect();
    files.sort();
    let sr = 48000.0f64;
    let s = |n| Settings { align: Align::Raw, normalize: n, min_phase: false, reverse: false, length_s: 20.0, fade: 0.1, max_s: 20.0, band: BAND, ref_db: REF_DB, map: ChannelMap::Fx, out_channels: 2 };
    let mut raw = Vec::new();
    for p in &files {
        let src = ir_host::decode_wav(&std::fs::read(p).unwrap()).unwrap();
        let off = ir::prepare(&src, sr, &s(Normalize::Off));
        let chs: Vec<Vec<f64>> = off.iter().map(|c| c.iter().map(|v| *v as f64).collect()).collect();
        raw.push(10.0 * ir::weighted_power(&chs, sr, BAND).log10());
        let loud = ir::prepare(&src, sr, &s(Normalize::Loudness));
        let chs: Vec<Vec<f64>> = loud.iter().map(|c| c.iter().map(|v| *v as f64).collect()).collect();
        assert!((10.0 * ir::weighted_power(&chs, sr, BAND).log10() - REF_DB).abs() < 1e-6);
    }
    raw.sort_by(|a, b| a.partial_cmp(b).unwrap());
    eprintln!("irfx: {} effect IRs: raw power gain {:+.1} … {:+.1} dB, median {:+.1} dB", raw.len(), raw[0], raw[raw.len() - 1], raw[raw.len() / 2]);
    // the longest through the engine
    let longest = files.iter().max_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)).unwrap();
    let src = ir_host::decode_wav(&std::fs::read(longest).unwrap()).unwrap();
    let chs32: Vec<Vec<f32>> = src.channels.iter().map(|c| c.iter().map(|v| *v as f32).collect()).collect();
    let mut f = IrFx::new();
    f.load_ir(1, &chs32, src.rate as f32);
    f.prepare(48000.0, 512, 2);
    f.set_param("ir_a", 1.0);
    f.set_param("wet", 0.0);
    f.set_param("dry", -60.0);
    f.reset();
    let l = noise(96000, 1, 0.3);
    let r = noise(96000, 2, 0.3);
    let (a, b) = render2(&mut f, &l, &r, 128);
    let prep = ir::prepare(&src, sr, &s(Normalize::Loudness));
    let e = err_db(&a, &fft_conv(&l, &prep[0])).max(err_db(&b, &fft_conv(&r, &prep[1])));
    eprintln!("irfx: {:.1} s stereo effect IR through the engine: {e:.1} dB re output", src.len() as f64 / src.rate);
    assert!(e < -110.0);
}

#[test]
fn cpu_spring_and_long_ir() {
    // 4 s stereo spring at 48 kHz (budget 3 % of a core), and a 20 s stereo IR
    for secs in [4.0f32, 20.0] {
        let sr = 48000.0;
        let len = (secs * sr) as usize;
        let mut f = fx(sr, 2, &[(1, vec![decaying(len, 1, len as f32 / 6.0), decaying(len, 2, len as f32 / 6.0)])], &[("ir_a", 1.0)]);
        let n = (sr * 4.0) as usize;
        let (mut l, mut r) = (noise(n, 3, 0.3), noise(n, 4, 0.3));
        let t = std::time::Instant::now();
        for (a, b) in l.chunks_mut(128).zip(r.chunks_mut(128)) {
            f.process(&mut [a, b]);
        }
        let pct = t.elapsed().as_secs_f64() / 4.0 * 100.0;
        eprintln!("irfx: CPU {secs} s stereo IR at 48 kHz: {pct:.2} % of a core");
    }
}
