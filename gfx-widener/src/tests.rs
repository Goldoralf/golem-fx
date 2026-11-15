// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5, the rows that run on synthetic signals).

use crate::{p, Widener, PARAMS};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::Effect;
use realfft::RealFftPlanner;

const RATES: [f32; 4] = [44100.0, 48000.0, 96000.0, 192000.0];

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-300).log10()
}

fn fresh(sr: f32, settings: &[(&str, f32)]) -> Widener {
    let mut w = Widener::new();
    w.prepare(sr, 512, 2);
    for &(id, v) in settings {
        w.set_param(id, v);
    }
    w.reset();
    w
}

fn render(w: &mut Widener, l: &[f32], r: &[f32], block: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut a, mut b) = (l.to_vec(), r.to_vec());
    for (x, y) in a.chunks_mut(block).zip(b.chunks_mut(block)) {
        w.process(&mut [x, y]);
    }
    (a, b)
}

fn white(n: usize, seed: u64) -> Vec<f32> {
    let mut r = gfx_dsp::lfo::Rng::new(seed);
    (0..n).map(|_| r.bipolar()).collect()
}

/// Pink noise (Paul Kellet's filter), RMS ≈ 0.1.
fn pink(n: usize, seed: u64) -> Vec<f32> {
    let w = white(n, seed);
    let mut b = [0.0f64; 7];
    let mut y: Vec<f32> = w
        .iter()
        .map(|&x| {
            let x = x as f64;
            b[0] = 0.99886 * b[0] + x * 0.0555179;
            b[1] = 0.99332 * b[1] + x * 0.0750759;
            b[2] = 0.96900 * b[2] + x * 0.1538520;
            b[3] = 0.86650 * b[3] + x * 0.3104856;
            b[4] = 0.55000 * b[4] + x * 0.5329522;
            b[5] = -0.7616 * b[5] - x * 0.0168980;
            let v = b.iter().sum::<f64>() + x * 0.5362;
            b[6] = x * 0.115926;
            v as f32
        })
        .collect();
    let rms = (y.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / n as f64).sqrt();
    y.iter_mut().for_each(|v| *v = (*v as f64 * 0.1 / rms) as f32);
    y
}

/// Band-limits to `lo` – `hi` Hz (two 2nd-order Butterworth sections each side).
fn band(x: &[f32], lo: f32, hi: f32, sr: f32) -> Vec<f32> {
    let mut f = [Svf::new(), Svf::new(), Svf::new(), Svf::new()];
    f[0].set(SvfMode::HighPass, lo, 0.541, 0.0, sr);
    f[1].set(SvfMode::HighPass, lo, 1.307, 0.0, sr);
    f[2].set(SvfMode::LowPass, hi, 0.541, 0.0, sr);
    f[3].set(SvfMode::LowPass, hi, 1.307, 0.0, sr);
    x.iter().map(|&v| f.iter_mut().fold(v, |y, s| s.process(y))).collect()
}

fn corr(a: &[f32], b: &[f32]) -> f64 {
    let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        ab += *x as f64 * *y as f64;
        aa += (*x as f64).powi(2);
        bb += (*y as f64).powi(2);
    }
    ab / (aa * bb).sqrt()
}

/// The (L, R) impulse responses for a mono impulse (L = R = δ), `len` samples.
fn impulse(sr: f32, settings: &[(&str, f32)], len: usize) -> (Vec<f32>, Vec<f32>) {
    let mut w = fresh(sr, settings);
    let mut x = vec![0.0f32; len];
    x[0] = 1.0;
    render(&mut w, &x, &x, 512)
}

/// Complex spectrum of a response (zero-padded to a power of two ≥ 2^16), with its bin width.
fn spectrum(h: &[f32], sr: f32) -> (Vec<realfft::num_complex::Complex<f64>>, f64) {
    let n = h.len().next_power_of_two().max(1 << 16);
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);
    let mut buf = vec![0.0f64; n];
    for (b, v) in buf.iter_mut().zip(h) {
        *b = *v as f64;
    }
    let mut out = fft.make_output_vec();
    fft.process(&mut buf, &mut out).unwrap();
    (out, sr as f64 / n as f64)
}

fn erb_bands() -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut fc = 200.0f64;
    while fc <= 16000.0 {
        let bw = 24.7 * (4.37 * fc / 1000.0 + 1.0);
        out.push((fc - bw / 2.0, fc + bw / 2.0));
        fc += bw / 2.0;
    }
    out
}

fn third_bands() -> Vec<(f64, f64)> {
    (-7..13)
        .map(|k| 1000.0 * 2f64.powf(k as f64 / 3.0))
        .filter(|c| (200.0..=16000.0).contains(c))
        .map(|c| (c * 2f64.powf(-1.0 / 6.0), c * 2f64.powf(1.0 / 6.0)))
        .collect()
}

/// Band levels (dB) of a power spectrum, re its mean power over 200 Hz – 16 kHz.
fn band_devs(pw: &[f64], df: f64, bands: &[(f64, f64)]) -> Vec<f64> {
    let bin = |f: f64| (f / df).round() as usize;
    let mean = pw[bin(200.0)..bin(16000.0)].iter().sum::<f64>() / (bin(16000.0) - bin(200.0)) as f64;
    bands
        .iter()
        .map(|&(lo, hi)| {
            let (a, b) = (bin(lo), bin(hi).max(bin(lo) + 1));
            10.0 * (pw[a..b].iter().sum::<f64>() / (b - a) as f64 / mean).log10()
        })
        .collect()
}

/// One-channel colour of a setting: (third-octave min, max, ERB min, max) over L and R.
fn one_side(sr: f32, settings: &[(&str, f32)]) -> (f64, f64, f64, f64) {
    let (l, r) = impulse(sr, settings, (0.1 * sr) as usize);
    let mut o = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for h in [l, r] {
        let (s, df) = spectrum(&h, sr);
        let pw: Vec<f64> = s.iter().map(|c| c.norm_sqr()).collect();
        for v in band_devs(&pw, df, &third_bands()) {
            o.0 = o.0.min(v);
            o.1 = o.1.max(v);
        }
        for v in band_devs(&pw, df, &erb_bands()) {
            o.2 = o.2.min(v);
            o.3 = o.3.max(v);
        }
    }
    o
}

/// The generated side alone (S = (L − R) / 2 for a mono impulse).
fn side_ir(sr: f32, settings: &[(&str, f32)], len: usize) -> Vec<f32> {
    let (l, r) = impulse(sr, settings, len);
    l.iter().zip(&r).map(|(a, b)| 0.5 * (a - b)).collect()
}

const PLAIN: [(&str, f32); 2] = [("bass_keep", 0.0), ("amount", 60.0)];

fn velvet(variant: usize, time: f32) -> Vec<(&'static str, f32)> {
    vec![("mode", 0.0), ("bass_keep", 0.0), ("amount", 60.0), ("variant", variant as f32), ("time", time)]
}

// ------------------------------------------------------------------------------------------------
// §5 Exact fold

#[test]
fn exact_fold_every_mode_and_through_changes() {
    for sr in RATES {
        let n = (0.6 * sr) as usize;
        let mono = pink(n, 1);
        let (sl, sr2) = (pink(n, 2), pink(n, 3));
        for mode in 0..5 {
            for (k, (l, r)) in [(&mono, &mono), (&sl, &sr2)].into_iter().enumerate() {
                let mut w = fresh(sr, &[("mode", mode as f32), ("amount", 100.0), ("width", 200.0), ("transient", 50.0), ("balance", 35.0)]);
                let peak = l.iter().chain(r.iter()).fold(0.0f32, |m, v| m.max(v.abs())) as f64;
                let (mut a, mut b) = (l.clone(), r.clone());
                let mut worst = 0.0f64;
                let blk = 480;
                for (j, (x, y)) in a.chunks_mut(blk).zip(b.chunks_mut(blk)).enumerate() {
                    // a change every 10 ms: structure, bypass, filters, balance
                    match j % 12 {
                        1 => w.set_param("variant", 1.0 + (j % 8) as f32),
                        3 => w.set_param("mode", ((mode + j / 12) % 5) as f32),
                        5 => w.set_param("bypass", if j % 24 == 5 { 1.0 } else { 0.0 }),
                        7 => w.set_param("time", 5.0 + (j % 30) as f32),
                        8 => w.set_param("bass_keep", if j % 24 == 8 { 300.0 } else { 0.0 }),
                        9 => w.set_param("high_cut", if j % 24 == 9 { 4000.0 } else { 20000.0 }),
                        10 => w.set_param("balance", if j % 24 == 10 { -100.0 } else { 100.0 }),
                        _ => {}
                    }
                    let start = j * blk;
                    w.process(&mut [x, y]);
                    for i in 0..x.len() {
                        let fold = 0.5 * (x[i] as f64 + y[i] as f64);
                        let dry = 0.5 * (l[start + i] as f64 + r[start + i] as f64);
                        worst = worst.max((fold - dry).abs());
                    }
                }
                let e = db(worst / peak);
                assert!(e <= -120.0, "{sr} Hz, mode {mode}, {}: fold error {e:.1} dB", ["mono", "stereo"][k]);
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------
// §5 Correlation law

#[test]
fn correlation_follows_the_law() {
    let sr = 48000.0;
    let x = band(&pink((4.0 * sr) as usize, 7), 200.0, 12000.0, sr);
    for (mode, tol) in [(2.0, 0.03), (1.0, 0.05), (0.0, 0.08)] {
        for (a, wd) in [(30.0, 100.0), (60.0, 100.0), (100.0, 100.0), (100.0, 150.0)] {
            let g = (a / 100.0) * (wd / 100.0);
            let mut w = fresh(sr, &[("mode", mode), ("bass_keep", 0.0), ("amount", a), ("width", wd)]);
            let (l, r) = render(&mut w, &x, &x, 512);
            let from = sr as usize;
            let rr = corr(&l[from..], &r[from..]);
            let law = (1.0 - g * g) / (1.0 + g * g);
            eprintln!("mode {} g {g:.1}: r {rr:+.3}, law {law:+.3}", crate::MODES[mode as usize]);
            assert!((rr - law as f64).abs() <= tol, "mode {mode}, g {g}: r {rr:.3} vs {law:.3}");
        }
    }
}

// ------------------------------------------------------------------------------------------------
// §5 Power sum, one-side timbre, regularity, smear, variants

#[test]
fn power_sum_is_flat() {
    let sr = 48000.0;
    let g: f64 = 0.6;
    for mode in [1.0, 2.0] {
        let (l, r) = impulse(sr, &[("mode", mode), ("bass_keep", 0.0), ("amount", 60.0)], 1 << 15);
        let ((sl, df), (sr_, _)) = (spectrum(&l, sr), spectrum(&r, sr));
        let mut worst = 0.0f64;
        for k in (50.0 / df) as usize..(18000.0 / df) as usize {
            let p = sl[k].norm_sqr() + sr_[k].norm_sqr();
            worst = worst.max((10.0 * (p / (2.0 * (1.0 + g * g))).log10()).abs());
        }
        eprintln!("{}: power sum within ±{worst:.4} dB", crate::MODES[mode as usize]);
        assert!(worst <= 0.1, "mode {mode}: {worst:.3} dB");
    }
    // Velvet: in ERB bands
    for variant in 1..=8 {
        let (l, r) = impulse(sr, &velvet(variant, 20.0), (0.1 * sr) as usize);
        let ((sl, df), (sr_, _)) = (spectrum(&l, sr), spectrum(&r, sr));
        let pw: Vec<f64> = sl.iter().zip(&sr_).map(|(a, b)| (a.norm_sqr() + b.norm_sqr()) / (2.0 * (1.0 + g * g))).collect();
        let bin = |f: f64| (f / df).round() as usize;
        let worst = erb_bands().iter().map(|&(lo, hi)| (10.0 * (pw[bin(lo)..bin(hi)].iter().sum::<f64>() / (bin(hi) - bin(lo)) as f64).log10()).abs()).fold(0.0, f64::max);
        eprintln!("Velvet v{variant}: power sum within ±{worst:.2} dB (ERB)");
        assert!(worst <= 1.0, "variant {variant}: {worst:.2} dB");
    }
}

/// The review's first coding task: the one-channel colour of every variant, 20 – 40 ms, 44.1 – 96 kHz.
/// The README's bound: ≤ ±2 dB third-octave, ≤ ±2.5 dB ERB (the design script's tables measure
/// better: see `vnd_tables.rs`).
#[test]
fn velvet_one_side_timbre_holds_for_every_variant() {
    let mut all = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for sr in [44100.0, 48000.0, 96000.0] {
        for time in [20.0, 30.0, 40.0] {
            for variant in 1..=8 {
                let o = one_side(sr, &velvet(variant, time));
                all = (all.0.min(o.0), all.1.max(o.1), all.2.min(o.2), all.3.max(o.3));
                assert!(o.0 >= -2.0 && o.1 <= 2.0, "v{variant} {time} ms {sr}: third-octave [{:+.2}, {:+.2}]", o.0, o.1);
                assert!(o.2 >= -2.5 && o.3 <= 2.5, "v{variant} {time} ms {sr}: ERB [{:+.2}, {:+.2}]", o.2, o.3);
            }
        }
    }
    eprintln!("Velvet one side, all: third-octave [{:+.2}, {:+.2}] dB, ERB [{:+.2}, {:+.2}] dB", all.0, all.1, all.2, all.3);
    // reported, not bounded
    for time in [10.0, 15.0] {
        let mut o = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for variant in 1..=8 {
            let q = one_side(48000.0, &velvet(variant, time));
            o = (o.0.min(q.0), o.1.max(q.1), o.2.min(q.2), o.3.max(q.3));
        }
        eprintln!("Velvet {time} ms: third-octave [{:+.2}, {:+.2}] dB, ERB [{:+.2}, {:+.2}] dB", o.0, o.1, o.2, o.3);
    }
    for (name, s) in [("Allpass v1", vec![("mode", 1.0), ("bass_keep", 0.0)]), ("Comb 20 ms", vec![("mode", 2.0), ("bass_keep", 0.0)]), ("Comb 5 ms", vec![("mode", 2.0), ("bass_keep", 0.0), ("time", 5.0)])] {
        let o = one_side(48000.0, &s);
        eprintln!("{name}: third-octave [{:+.2}, {:+.2}] dB, ERB [{:+.2}, {:+.2}] dB", o.0, o.1, o.2, o.3);
    }
}

fn regularity(h: &[f32], sr: f32) -> f64 {
    let (s, _) = spectrum(h, sr);
    let n = 2 * (s.len() - 1);
    let mut planner = RealFftPlanner::<f64>::new();
    let ifft = planner.plan_fft_inverse(n);
    let mut p: Vec<_> = s.iter().map(|c| realfft::num_complex::Complex::new(c.norm_sqr(), 0.0)).collect();
    let mut out = vec![0.0f64; n];
    ifft.process(&mut p, &mut out).unwrap();
    let (lo, hi) = ((0.0005 * sr) as usize, (0.040 * sr) as usize);
    out[lo..hi].iter().fold(0.0f64, |m, v| m.max(v.abs())) / out[0]
}

#[test]
fn velvet_is_irregular_and_comb_is_not() {
    let sr = 48000.0;
    let mut worst = 0.0f64;
    for variant in 1..=8 {
        for time in [20.0, 30.0, 40.0] {
            let (l, r) = impulse(sr, &velvet(variant, time), (0.06 * sr) as usize);
            worst = worst.max(regularity(&l, sr)).max(regularity(&r, sr));
        }
    }
    let (l, _) = impulse(sr, &[("mode", 2.0), ("bass_keep", 0.0)], (0.06 * sr) as usize);
    let comb = regularity(&l, sr);
    eprintln!("regularity: Velvet ≤ {worst:.3}, Comb {comb:.3} (g / (1 + g²) = 0.441)");
    assert!(worst <= 0.2, "{worst}");
    assert!((comb - 0.441).abs() < 0.01, "{comb}");
}

fn t90(x: &[f32], sr: f32, high: bool) -> f64 {
    let mut f = [Svf::new(), Svf::new()];
    let mode = if high { SvfMode::HighPass } else { SvfMode::LowPass };
    let fc = if high { 2000.0 } else { 500.0 };
    f[0].set(mode, fc, 0.541, 0.0, sr);
    f[1].set(mode, fc, 1.307, 0.0, sr);
    let y: Vec<f64> = x.iter().map(|&v| f.iter_mut().fold(v, |y, s| s.process(y)) as f64).collect();
    let total: f64 = y.iter().map(|v| v * v).sum();
    let mut acc = 0.0;
    for (i, v) in y.iter().enumerate() {
        acc += v * v;
        if acc >= 0.9 * total {
            return i as f64 / sr as f64 * 1000.0;
        }
    }
    f64::INFINITY
}

#[test]
fn velvet_smear_highs_inside_the_fusion_window() {
    let sr = 48000.0;
    let (mut hi, mut lo) = (0.0f64, 0.0f64);
    for variant in 1..=8 {
        let s = side_ir(sr, &velvet(variant, 20.0), (0.1 * sr) as usize);
        hi = hi.max(t90(&s, sr, true));
        lo = lo.max(t90(&s, sr, false));
    }
    eprintln!("Velvet 20 ms side: 90 % of the energy above 2 kHz within {hi:.1} ms, below 500 Hz within {lo:.1} ms");
    assert!(hi <= 5.5, "{hi}");
    assert!(lo <= 20.0, "{lo}");
}

#[test]
fn variants_are_distinct() {
    let sr = 48000.0;
    for (mode, bound) in [(0.0, 0.2), (1.0, 0.65)] {
        let sides: Vec<_> = (1..=8)
            .map(|v| spectrum(&side_ir(sr, &[("mode", mode), ("bass_keep", 0.0), ("amount", 100.0), ("variant", v as f32)], (0.06 * sr) as usize), sr))
            .collect();
        let df = sides[0].1;
        let (a, b) = ((200.0 / df) as usize, (12000.0 / df) as usize);
        let mut worst = 0.0f64;
        for i in 0..8 {
            for j in 0..i {
                let (mut x, mut ii, mut jj) = (0.0, 0.0, 0.0);
                for k in a..b {
                    let w = 1.0 / (k as f64 * df);
                    let (p, q) = (sides[i].0[k], sides[j].0[k]);
                    x += w * (p * q.conj()).re;
                    ii += w * p.norm_sqr();
                    jj += w * q.norm_sqr();
                }
                worst = worst.max((x / (ii * jj).sqrt()).abs());
            }
        }
        eprintln!("{}: variants pairwise |r| ≤ {worst:.3}", crate::MODES[mode as usize]);
        assert!(worst <= bound, "mode {mode}: {worst:.3}");
    }
}

// ------------------------------------------------------------------------------------------------
// §5 Bass keep, high cut

fn gain_at(h: &[f32], f: f64, sr: f32) -> f64 {
    let (s, df) = spectrum(h, sr);
    s[(f / df).round() as usize].norm()
}

#[test]
fn bass_keep_on_the_side_only() {
    for sr in [44100.0, 48000.0, 96000.0] {
        let fc = 120.0;
        let len = (0.5 * sr) as usize;
        // generated side: the LR4 shape is the ratio to the same side without bass_keep (Comb: flat)
        let on = side_ir(sr, &[("mode", 2.0), ("bass_keep", fc), ("amount", 100.0)], len);
        let off = side_ir(sr, &[("mode", 2.0), ("bass_keep", 0.0), ("amount", 100.0)], len);
        let h = |f: f64| gain_at(&on, f, sr) / gain_at(&off, f, sr);
        let rel = db(h(fc as f64 / 2.0) / h(4.0 * fc as f64));
        assert!(rel <= -24.0, "{sr}: generated side at fc/2 {rel:.1} dB re 4 fc");
        // source side: 12 dB / octave
        let mut x = vec![0.0f32; len];
        x[0] = 1.0;
        let neg: Vec<f32> = x.iter().map(|v| -v).collect();
        let mut w = fresh(sr, &[("bass_keep", fc), ("amount", 0.0)]);
        let (l, r) = render(&mut w, &x, &neg, 512);
        let s: Vec<f32> = l.iter().zip(&r).map(|(a, b)| 0.5 * (a - b)).collect();
        let half = db(gain_at(&s, fc as f64 / 2.0, sr));
        assert!((half + 12.0).abs() <= 0.5, "{sr}: source side at fc/2 {half:.2} dB");
        // hard-panned source: crosstalk at 4 fc
        let zero = vec![0.0f32; len];
        let mut w = fresh(sr, &[("bass_keep", fc), ("amount", 0.0)]);
        let (l, r) = render(&mut w, &x, &zero, 512);
        let xt = db(gain_at(&r, 4.0 * fc as f64, sr) / gain_at(&l, 4.0 * fc as f64, sr));
        assert!(xt <= -14.0, "{sr}: crosstalk at 4 fc {xt:.1} dB");
        // the mid sees no filter: L = R = δ, amount 0 → exactly δ
        let mut w = fresh(sr, &[("bass_keep", fc), ("amount", 0.0)]);
        let (l, r) = render(&mut w, &x, &x, 512);
        assert_eq!(l, x);
        assert_eq!(r, x);
        eprintln!("{sr}: gen side fc/2 {rel:.1} dB re 4 fc, source side fc/2 {half:.2} dB, crosstalk 4 fc {xt:.1} dB");
    }
}

#[test]
fn high_cut_on_the_generated_side_only() {
    let sr = 48000.0;
    let len = (0.2 * sr) as usize;
    for fc in [2000.0, 5000.0, 12000.0] {
        let on = side_ir(sr, &[("mode", 2.0), ("bass_keep", 0.0), ("high_cut", fc), ("amount", 100.0)], len);
        let off = side_ir(sr, &[("mode", 2.0), ("bass_keep", 0.0), ("amount", 100.0)], len);
        let g = db(gain_at(&on, fc as f64, sr) / gain_at(&off, fc as f64, sr));
        assert!((g + 3.0).abs() <= 0.5, "{fc}: {g:.2} dB");
    }
    // the source's side is untouched
    let x = pink(len, 4);
    let neg: Vec<f32> = x.iter().map(|v| -v).collect();
    let mut w = fresh(sr, &[("bass_keep", 0.0), ("high_cut", 3000.0), ("amount", 0.0)]);
    let (l, r) = render(&mut w, &x, &neg, 512);
    assert_eq!(l, x);
    assert_eq!(r, neg);
}

// ------------------------------------------------------------------------------------------------
// §5 Transient

fn note(n: usize, f0: f32, sr: f32, at: usize, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| {
            if i < at {
                return 0.0;
            }
            let t = (i - at) as f32 / sr;
            let s: f32 = (1..=8).map(|k| (std::f32::consts::TAU * f0 * k as f32 * t + k as f32).sin() / k as f32 * (-t * (1.0 + 0.6 * k as f32)).exp()).sum();
            amp * s
        })
        .collect()
}

fn energy(x: &[f32]) -> f64 {
    x.iter().map(|v| (*v as f64).powi(2)).sum()
}

#[test]
fn transient_focus_ducks_attacks_and_spares_sustains() {
    let sr = 48000.0;
    let n = (1.6 * sr) as usize;
    let (t1, t2) = ((0.2 * sr) as usize, (0.9 * sr) as usize);
    let a = note(n, 110.0, sr, t1, 0.5);
    let b = note(n, 220.0, sr, t2, 0.5);
    let x: Vec<f32> = a.iter().zip(&b).map(|(p, q)| p + q).collect();
    let side = |tr: f32| {
        let mut w = fresh(sr, &[("transient", tr), ("bass_keep", 0.0)]);
        let (l, r) = render(&mut w, &x, &x, 64);
        l.iter().zip(&r).map(|(p, q)| 0.5 * (p - q)).collect::<Vec<f32>>()
    };
    let (s0, s1) = (side(0.0), side(100.0));
    let win = (0.020 * sr) as usize;
    let first = 10.0 * (energy(&s1[t1..t1 + win]) / energy(&s0[t1..t1 + win])).log10();
    let second = 10.0 * (energy(&s1[t2..t2 + win]) / energy(&s0[t2..t2 + win])).log10();
    eprintln!("transient 100 %: pick after silence {first:.1} dB in the first 20 ms, pick over a ringing note {second:.1} dB");
    assert!(first <= -20.0, "{first}");
    assert!(second <= -6.0, "{second}");

    // a sustained 82 Hz note: the duck rests at ≥ 0.95 without a ripple at the note's rate
    let x = note((1.5 * sr) as usize, 82.4, sr, t1, 0.5);
    let mut w = fresh(sr, &[("transient", 100.0)]);
    let mut d = Vec::new();
    for v in &x {
        let (mut l, mut r) = ([*v], [*v]);
        w.process(&mut [&mut l, &mut r]);
        d.push(w.duck());
    }
    let sus = &d[t1 + (0.25 * sr) as usize..];
    let lo = sus.iter().fold(1.0f32, |a, &v| a.min(v));
    // the duck's component at the note's rate (and its first harmonics)
    let mut ripple = 0.0f64;
    for k in 1..=3 {
        let wk = std::f64::consts::TAU * 82.4 * k as f64 / sr as f64;
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &v) in sus.iter().enumerate() {
            re += v as f64 * (wk * i as f64).cos();
            im -= v as f64 * (wk * i as f64).sin();
        }
        ripple = ripple.max(2.0 * (re * re + im * im).sqrt() / sus.len() as f64);
    }
    eprintln!("sustained 82 Hz: duck ≥ {lo:.4}, component at the note's rate {:.1} dB", db(ripple));
    assert!(lo >= 0.95, "{lo}");
    assert!(db(ripple) <= -40.0, "ripple {ripple}");
}

// ------------------------------------------------------------------------------------------------
// §5 Classic Haas, Double

#[test]
fn classic_haas_folds_to_the_comb() {
    let sr = 48000.0;
    let n = sr as usize;
    let tau = (0.015 * sr) as usize;
    let x = pink(n, 9);
    let mut w = fresh(sr, &[("mode", 2.0), ("mono_law", 1.0), ("time", 15.0), ("amount", 100.0), ("bass_keep", 0.0)]);
    let (l, r) = render(&mut w, &x, &x, 512);
    let mut worst = 0.0f64;
    for i in tau..n {
        let want = 0.5 * (x[i] as f64 + x[i - tau] as f64);
        worst = worst.max((0.5 * (l[i] as f64 + r[i] as f64) - want).abs());
    }
    let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64;
    assert!(db(worst / peak) <= -80.0, "fold vs comb {:.1} dB", db(worst / peak));
    let ld = l.iter().zip(&x).fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
    assert!(ld <= 1e-7, "Haas: L is the dry ({ld:e})");
    // notches at 33.3 / 100 / 166.7 Hz
    let (li, ri) = impulse(sr, &[("mode", 2.0), ("mono_law", 1.0), ("time", 15.0), ("amount", 100.0), ("bass_keep", 0.0)], 1 << 16);
    let fold: Vec<f32> = li.iter().zip(&ri).map(|(a, b)| 0.5 * (a + b)).collect();
    for f in [100.0 / 3.0, 100.0, 500.0 / 3.0] {
        let g = db(gain_at(&fold, f, sr));
        assert!(g < -30.0, "notch at {f:.1} Hz: {g:.1} dB");
    }
    // bass_keep 120: R a pure delay above 240 Hz, the fold flat below 60 Hz
    let (li, ri) = impulse(sr, &[("mode", 2.0), ("mono_law", 1.0), ("time", 15.0), ("amount", 100.0), ("bass_keep", 120.0)], 1 << 16);
    let fold: Vec<f32> = li.iter().zip(&ri).map(|(a, b)| 0.5 * (a + b)).collect();
    let (sr_s, df) = spectrum(&ri, sr);
    let (sf, _) = spectrum(&fold, sr);
    let r_dev = ((240.0 / df) as usize..(18000.0 / df) as usize).map(|k| db(sr_s[k].norm()).abs()).fold(0.0, f64::max);
    let f_dev = ((20.0 / df) as usize..(60.0 / df) as usize).map(|k| db(sf[k].norm()).abs()).fold(0.0, f64::max);
    eprintln!("Haas + bass_keep 120: R within ±{r_dev:.2} dB above 240 Hz, fold within ±{f_dev:.2} dB below 60 Hz");
    assert!(r_dev <= 1.0, "{r_dev}");
    assert!(f_dev <= 0.2, "{f_dev}");
}

fn pitch_cents(y: &[f32], f0: f64, sr: f32) -> f64 {
    let mut t = Vec::new();
    for i in 1..y.len() {
        if y[i - 1] < 0.0 && y[i] >= 0.0 {
            t.push((i - 1) as f64 + (-y[i - 1] / (y[i] - y[i - 1])) as f64);
        }
    }
    1200.0 * ((t.len() - 1) as f64 / (t[t.len() - 1] - t[0]) * sr as f64 / f0).log2()
}

#[test]
fn classic_double_voices_are_detuned_and_late() {
    let sr = 48000.0;
    let n = 2 * sr as usize;
    let x: Vec<f32> = (0..n).map(|i| 0.3 * (std::f32::consts::TAU * 1000.0 * i as f32 / sr).sin()).collect();
    for cents in [9.0, 25.0] {
        let mut w = fresh(sr, &[("mode", 3.0), ("mono_law", 1.0), ("cents", cents), ("amount", 100.0), ("bass_keep", 0.0)]);
        let (l, r) = render(&mut w, &x, &x, 256);
        // L = M + V₊, R = M + V₋
        let vp: Vec<f32> = l.iter().zip(&x).map(|(a, b)| a - b).collect();
        let vm: Vec<f32> = r.iter().zip(&x).map(|(a, b)| a - b).collect();
        let (cp, cm) = (pitch_cents(&vp[12000..], 1000.0, sr), pitch_cents(&vm[12000..], 1000.0, sr));
        eprintln!("Double Classic {cents} ct: L {cp:+.2}, R {cm:+.2} cents");
        assert!((cp - cents as f64).abs() <= 1.0 && (cm + cents as f64).abs() <= 1.0, "{cp:.2} / {cm:.2}");
    }
    // the voices arrive `time` + the 3 ms centre late
    let mut x = vec![0.0f32; sr as usize / 4];
    x[100] = 1.0;
    for time in [12.0, 30.0] {
        let mut w = fresh(sr, &[("mode", 3.0), ("mono_law", 1.0), ("cents", 0.0), ("amount", 100.0), ("bass_keep", 0.0), ("time", time)]);
        let (l, _) = render(&mut w, &x, &x, 256);
        let v: Vec<f32> = l.iter().zip(&x).map(|(a, b)| a - b).collect();
        let at = v.iter().enumerate().fold((0, 0.0f32), |m, (i, s)| if s.abs() > m.1 { (i, s.abs()) } else { m }).0 as f64 - 100.0;
        let want = (time as f64 + 3.0) * 0.001 * sr as f64;
        assert!((at - want).abs() <= 1.0 + 0.5, "time {time}: {at} samples, want {want}");
    }
}

// ------------------------------------------------------------------------------------------------
// §5 Rotation, balance, meters

#[test]
fn rotation_and_balance() {
    let sr = 48000.0;
    let x = pink(sr as usize / 2, 11);
    let rms = |v: &[f32]| (energy(v) / v.len() as f64).sqrt();
    let mut w = fresh(sr, &[("amount", 0.0), ("rotate", 45.0)]);
    let (l, r) = render(&mut w, &x, &x, 512);
    let (gl, gr) = (db(rms(&l) / rms(&x)), db(rms(&r) / rms(&x)));
    assert!(gl <= -100.0, "rotate +45: L {gl:.1} dB");
    assert!((gr - 3.0103).abs() <= 0.01, "rotate +45: R {gr:.3} dB");
    // θ = 0: bit-exact pass (amount 0)
    let mut w = fresh(sr, &[("amount", 0.0)]);
    let (l, r) = render(&mut w, &x, &x, 512);
    assert_eq!(l, x);
    assert_eq!(r, x);
    // balance +100 %: L silent, R +6.02 dB
    let mut w = fresh(sr, &[("amount", 0.0), ("balance", 100.0)]);
    let (l, r) = render(&mut w, &x, &x, 512);
    let (gl, gr) = (db(rms(&l) / rms(&x)), db(rms(&r) / rms(&x)));
    assert!(gl <= -100.0, "balance +100: L {gl:.1} dB");
    assert!((gr - 6.0206).abs() <= 0.01, "balance +100: R {gr:.3} dB");
}

#[test]
fn meters_read_correlation_and_the_fold() {
    let sr = 48000.0;
    let n = 2 * sr as usize;
    let a = pink(n, 21);
    let b: Vec<f32> = white(n, 22).iter().map(|v| 0.1 * v).collect();
    let a2: Vec<f32> = white(n, 23).iter().map(|v| 0.1 * v).collect();
    let neg: Vec<f32> = a.iter().map(|v| -v).collect();
    let run = |s: &[(&str, f32)], l: &[f32], r: &[f32]| {
        let mut w = fresh(sr, s);
        render(&mut w, l, r, 512);
        (w.corr(), w.mono_err_db())
    };
    let plain = [("amount", 0.0), ("bass_keep", 0.0)];
    assert!((run(&plain, &a, &a).0 - 1.0).abs() < 0.005);
    assert!((run(&plain, &a, &neg).0 + 1.0).abs() < 0.005);
    let ind = run(&plain, &a2, &b).0;
    assert!(ind.abs() <= 0.05, "{ind}");
    // the exact law reads the floor, at any balance
    for bal in [0.0, -35.0, 100.0] {
        let e = run(&[("balance", bal), ("amount", 100.0), ("width", 200.0)], &a, &a).1;
        assert_eq!(e, -80.0, "balance {bal}: {e}");
    }
    // Classic Haas: the comb's loss, (M(t − τ) − M) / 2 against M, computed on the same signal
    let e = run(&[("mode", 2.0), ("mono_law", 1.0), ("amount", 100.0), ("bass_keep", 0.0), ("time", 15.0)], &a, &a).1;
    let tau = (0.015 * sr) as usize;
    let from = n - sr as usize;
    let num: f64 = (from..n).map(|i| (0.5 * (a[i - tau] as f64 - a[i] as f64)).powi(2)).sum();
    let want = 10.0 * (num / energy(&a[from..])).log10();
    eprintln!("mono_err, Classic Haas: {e:.2} dB, analytic {want:.2} dB");
    assert!((e as f64 - want).abs() <= 0.5, "Haas: {e:.2} dB vs {want:.2}");
    // rotation at amount 0: 20 log(1 − cos θ)
    for th in [10.0f32, 20.0, 45.0] {
        let e = run(&[("amount", 0.0), ("rotate", th)], &a, &a).1;
        let want = 20.0 * (1.0 - th.to_radians().cos()).log10();
        assert!((e - want).abs() <= 0.5, "rotate {th}: {e:.2} vs {want:.2}");
    }
    // the generated side moves the correlation as the law says (Comb, g = 0.6 → 0.47)
    let x = band(&a, 200.0, 12000.0, sr);
    let c = run(&[("mode", 2.0), ("bass_keep", 0.0)], &x, &x).0;
    assert!((c - 0.4706).abs() <= 0.05, "{c}");
}

// ------------------------------------------------------------------------------------------------
// Structure, robustness

#[test]
fn every_change_is_finite_and_the_mono_lane_is_the_fold() {
    let sr = 48000.0;
    let x = pink(sr as usize, 31);
    for pi in PARAMS.iter() {
        for v in [pi.min, pi.max] {
            let mut w = fresh(sr, &[]);
            let (mut l, mut r) = (x.clone(), x.clone());
            let h = l.len() / 2;
            w.process(&mut [&mut l[..h], &mut r[..h]]);
            w.set_param(pi.id, v);
            w.process(&mut [&mut l[h..], &mut r[h..]]);
            assert!(l.iter().chain(r.iter()).all(|s| s.is_finite() && s.abs() < 10.0), "{} = {v}", pi.id);
        }
    }
    // one channel: the fold
    let mut s = fresh(sr, &[("rotate", 20.0)]);
    let (l, r) = render(&mut s, &x, &x, 512);
    let mut m = fresh(sr, &[("rotate", 20.0)]);
    let mut y = x.clone();
    for c in y.chunks_mut(512) {
        m.process(&mut [c]);
    }
    for i in 0..y.len() {
        assert!((y[i] - 0.5 * (l[i] + r[i])).abs() <= 1e-7);
    }
    let _ = (p::MODE, PLAIN);
}

/// §5 "Structural changes": `mode`, `variant`, `mono_law`, `time` toggled mid-signal, no click on L or
/// R (the common bound: HF burst above 10 kHz ≤ −50 dB re peak in the 100 ms after the change).
#[test]
fn structural_changes_do_not_click() {
    for sr in [48000.0f32, 96000.0] {
        let n = sr as usize;
        let x: Vec<f32> = (0..n).map(|i| 0.25 * (std::f32::consts::TAU * 1000.0 * i as f32 / sr).sin()).collect();
        let mut worst = (f64::MIN, String::new());
        type Case = (&'static str, f32, f32, Vec<(&'static str, f32)>);
        let cases: Vec<Case> = vec![
            ("mode", 0.0, 1.0, vec![]),
            ("mode", 1.0, 2.0, vec![]),
            ("mode", 2.0, 3.0, vec![]),
            ("mode", 3.0, 4.0, vec![]),
            ("mode", 4.0, 0.0, vec![]),
            ("variant", 1.0, 8.0, vec![]),
            ("variant", 3.0, 4.0, vec![("mode", 1.0)]),
            ("mono_law", 0.0, 1.0, vec![("mode", 2.0)]),
            ("mono_law", 1.0, 0.0, vec![("mode", 3.0)]),
            ("time", 10.0, 40.0, vec![]),
            ("time", 40.0, 1.0, vec![("mode", 2.0)]),
            ("time", 5.0, 30.0, vec![("mode", 3.0)]),
        ];
        for (id, from, to, base) in cases {
            let mut s = base.clone();
            s.push((id, from));
            s.push(("amount", 100.0));
            let mut w = fresh(sr, &s);
            let (mut l, mut r) = (x.clone(), x.clone());
            let h = n / 2;
            w.process(&mut [&mut l[..h], &mut r[..h]]);
            w.set_param(id, to);
            w.process(&mut [&mut l[h..], &mut r[h..]]);
            for y in [&l, &r] {
                let pk = y.iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64;
                let mut f = [Svf::new(), Svf::new()];
                f[0].set(SvfMode::HighPass, 10000.0, 0.541, 0.0, sr);
                f[1].set(SvfMode::HighPass, 10000.0, 1.307, 0.0, sr);
                let hp: Vec<f32> = y.iter().map(|&v| f.iter_mut().fold(v, |a, s| s.process(a))).collect();
                let m = |a: usize, b: usize| hp[a..b].iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64;
                let after = m(h, h + n / 10);
                let steady = m(n / 4, h).max(m(n - n / 4, n));
                let rel = db((after - steady).max(0.0)) - db(pk);
                if rel > worst.0 {
                    worst = (rel, format!("{id} {from} → {to} ({base:?})"));
                }
            }
        }
        eprintln!("{sr}: worst structural HF burst {:.0} dB re peak ({})", worst.0.max(-200.0), worst.1);
        assert!(worst.0 <= -50.0, "{sr}: {:.1} dB ({})", worst.0, worst.1);
    }
}

/// CPU per mode (README §5: ≤ 100 ns per stereo frame, ≤ 300 for Double):
/// `cargo test --release -p gfx-widener cpu -- --ignored --nocapture`.
#[test]
#[ignore]
fn cpu_per_mode() {
    let sr = 48000.0;
    let n = 10 * sr as usize;
    let x = pink(n, 41);
    for (name, s) in [
        ("Velvet", vec![]),
        ("Velvet, transient + high cut", vec![("transient", 50.0), ("high_cut", 8000.0)]),
        ("Allpass", vec![("mode", 1.0)]),
        ("Comb", vec![("mode", 2.0)]),
        ("Comb Classic", vec![("mode", 2.0), ("mono_law", 1.0)]),
        ("Double", vec![("mode", 3.0)]),
        ("Double Classic", vec![("mode", 3.0), ("mono_law", 1.0)]),
        ("M/S", vec![("mode", 4.0)]),
    ] {
        let mut w = fresh(sr, &s);
        let t = std::time::Instant::now();
        let _ = render(&mut w, &x, &x, 128);
        eprintln!("{name}: {:.1} ns / stereo frame", t.elapsed().as_secs_f64() * 1e9 / n as f64);
    }
}
