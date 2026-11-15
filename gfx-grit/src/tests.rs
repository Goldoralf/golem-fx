// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5) on synthetic signals. The guitar / drum DIs and the listening rows stay
//! with the bench; the drum loop here is synthetic (kick, snare, hats, fixed seed).

use super::*;
use crate::curves::Curve;
use gfx_dsp::lfo::Rng;
use std::f64::consts::PI;

const SR: f32 = 48000.0;

fn idx(id: &str) -> usize {
    PARAMS.iter().position(|p| p.id == id).unwrap_or_else(|| panic!("{id}"))
}

fn kind_val(name: &str) -> f32 {
    MODULE_NAMES.iter().position(|n| *n == name).unwrap_or_else(|| panic!("{name}")) as f32
}

fn dut(set: &[(&str, f32)], sr: f32, channels: usize) -> Grit {
    let mut d = Grit::new();
    for (k, v) in set {
        assert!(PARAMS.iter().any(|p| p.id == *k), "{k}");
        d.set_param(k, *v);
    }
    d.prepare(sr, 512, channels);
    d.reset();
    d
}

fn run(d: &mut Grit, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for b in y.chunks_mut(512) {
        d.process(&mut [b]);
    }
    y
}

fn sine(f: f64, amp: f64, n: usize, sr: f32) -> Vec<f32> {
    (0..n).map(|i| (amp * (2.0 * PI * f * i as f64 / sr as f64).sin()) as f32).collect()
}

fn rms(v: &[f32]) -> f64 {
    (v.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-30).log10()
}

/// |DTFT| of `h` at `f` (dB).
fn dtft_db(h: &[f32], f: f64, sr: f32) -> f64 {
    let w = 2.0 * PI * f / sr as f64;
    let (mut re, mut im) = (0.0, 0.0);
    for (n, &v) in h.iter().enumerate() {
        let a = w * n as f64;
        re += v as f64 * a.cos();
        im -= v as f64 * a.sin();
    }
    db(re.hypot(im))
}

fn impulse(n: usize, amp: f32) -> Vec<f32> {
    let mut x = vec![0.0; n];
    x[0] = amp;
    x
}

fn log_freqs(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    (0..n).map(|i| lo * (hi / lo).powf(i as f64 / (n - 1) as f64)).collect()
}

/// 4th-order Butterworth high-pass (the bench's click detector).
fn highpass4(x: &[f32], fc: f32, sr: f32) -> Vec<f32> {
    use gfx_dsp::svf::{Svf, SvfMode};
    let mut a = Svf::new();
    let mut b = Svf::new();
    a.set(SvfMode::HighPass, fc, 0.541_196, 0.0, sr);
    b.set(SvfMode::HighPass, fc, 1.306_563, 0.0, sr);
    x.iter().map(|&v| b.process(a.process(v))).collect()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

/// HF burst (dB re peak) in the 100 ms after sample `at`, above what the steady signal already has.
fn burst_db(y: &[f32], at: usize, sr: f32) -> f64 {
    let hp = highpass4(y, 10000.0f32.min(0.4 * sr), sr);
    let n = (0.1 * sr) as usize;
    let after = peak(&hp[at..at + n]);
    let steady = peak(&hp[at / 2..at]).max(peak(&hp[y.len() - y.len() / 8..]));
    db((after - steady).max(0.0) as f64) - db(peak(y) as f64)
}

/// Magnitude spectrum (Hann window), linear.
fn spectrum(y: &[f32]) -> Vec<f64> {
    let n = y.len();
    let mut planner = realfft::RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);
    let mut buf: Vec<f64> = y.iter().enumerate().map(|(i, &v)| v as f64 * (0.5 - 0.5 * (2.0 * PI * i as f64 / n as f64).cos())).collect();
    let mut out = fft.make_output_vec();
    fft.process(&mut buf, &mut out).unwrap();
    out.iter().map(|c| c.norm()).collect()
}

/// Strongest aliased component below 20 kHz relative to the fundamental (dB). `k0`: the sine's bin.
fn alias_db(y: &[f32], k0: usize, sr: f32) -> f64 {
    let n = y.len();
    let s = spectrum(y);
    let bin_hz = sr as f64 / n as f64;
    let top = (20000.0 / bin_hz) as usize;
    let fund = s[k0];
    let mut worst = 0.0f64;
    for (k, &v) in s.iter().enumerate().take(top).skip(8) {
        let h = k as f64 / k0 as f64;
        let near = (k as f64 - h.round() * k0 as f64).abs() <= 4.0 && h.round() >= 1.0;
        if !near {
            worst = worst.max(v);
        }
    }
    db(worst / fund)
}

/// A bin-centred odd-bin frequency near `f` for an `n`-point analysis.
fn bin_freq(f: f64, n: usize, sr: f32) -> (f64, usize) {
    let mut k = (f * n as f64 / sr as f64).round() as usize;
    if k.is_multiple_of(2) {
        k += 1;
    }
    (k as f64 * sr as f64 / n as f64, k)
}

fn all_bands(kind: &str, amount: f32) -> Vec<(&'static str, f32)> {
    let k = kind_val(kind);
    vec![("lo_m1", k), ("lo_v1", amount), ("lm_m1", k), ("lm_v1", amount), ("hm_m1", k), ("hm_v1", amount), ("hi_m1", k), ("hi_v1", amount)]
}

/// One band holding everything below 13.3 kHz (crossovers at the top of their windows).
const ONE_BAND: [(&str, f32); 3] = [("x1", 94.0), ("x2", 96.0), ("x3", 98.0)];

/// A synthetic drum loop at 120 BPM: kick (55 → 45 Hz), snare (noise + 190 Hz), closed hats.
fn drum_loop(n: usize, sr: f32) -> Vec<f32> {
    let mut r = Rng::new(77);
    let beat = (0.5 * sr) as usize;
    let mut y = vec![0.0f32; n];
    let mut ph = 0.0f64;
    for i in 0..n {
        let t_beat = i % beat;
        let bar_pos = (i / beat) % 4;
        let t = t_beat as f64 / sr as f64;
        let noise = r.bipolar() as f64;
        let mut v = 0.0;
        if bar_pos == 0 || bar_pos == 2 {
            let f = 45.0 + 10.0 * (-t / 0.03).exp();
            ph += f / sr as f64;
            v += 0.7 * (2.0 * PI * ph).sin() * (-t / 0.25).exp();
        } else {
            v += (0.35 * noise + 0.3 * (2.0 * PI * 190.0 * t).sin()) * (-t / 0.08).exp();
        }
        let t8 = (i % (beat / 2)) as f64 / sr as f64;
        v += 0.08 * noise * (-t8 / 0.015).exp();
        y[i] = v as f32;
    }
    y
}

// ---- the curves ------------------------------------------------------------------------------------

#[test]
fn antiderivatives_match_their_curves() {
    let curves = [
        Curve::Lin,
        Curve::Tanh(3.0),
        Curve::Clip(4.0),
        Curve::Heavy(0.3),
        Curve::Asym(0.3),
        Curve::Rectify(0.6, 1.0),
        Curve::Rectify(0.6, -1.0),
        Curve::Fold(5.0),
        Curve::SineFold(5.0),
        Curve::ZeroCross(0.05),
        Curve::Crush(0.125),
        Curve::Tube(4.0),
        Curve::BiasClip(3.0, 0.2),
    ];
    for c in curves {
        for i in -1500..=1500 {
            let x = i as f64 * 0.001 + 3.7e-5;
            let h = 1e-7;
            let d = (c.af(x + h) - c.af(x - h)) / (2.0 * h);
            assert!((d - c.f(x)).abs() < 2e-4, "{c:?} at {x}: F' {d} vs f {}", c.f(x));
        }
    }
}

#[test]
fn constants_hold() {
    assert!((curves::TANH_TUBE_BIAS - curves::TUBE_BIAS.tanh()).abs() < 1e-15);
    for i in -400..400 {
        let x = i as f64 * 0.05;
        assert!((curves::ln_cosh(x) - x.cosh().ln()).abs() < 1e-13, "{x}");
    }
}

#[test]
fn enum_keeps_the_catalogue_order_then_appends() {
    let catalogue = [
        "Off", "Soft clip", "Hard clip", "Heavy", "Asym", "Rectify", "Fold", "Sine fold", "Zero cross", "Bit crush", "Sample divide", "DC", "Width", "Gain", "Pan", "Lowpass", "Highpass", "Peak", "Dry/wet",
        "Stereo rectify", "Stereo DC", "Ring mod", "Cabinet",
    ];
    assert_eq!(&MODULE_NAMES[..23], &catalogue[..]);
    assert_eq!(&MODULE_NAMES[23..], &["Tube", "Fuzz"]);
    assert_eq!(PARAMS.len(), 50);
    for b in ["lo", "lm", "hm", "hi"] {
        for s in ["m1", "m2"] {
            assert_eq!(PARAMS[idx(&format!("{b}_{s}"))].kind, gfx_dsp::ParamKind::Enum(25));
        }
    }
}

// ---- crossovers and the clean sum ------------------------------------------------------------------

/// Impulse responses of the engine (all slots Off) and of the reference (the tree's all-pass on one
/// band): flat magnitude, and a null.
#[test]
fn clean_sum_is_flat_and_nulls_the_reference() {
    let settings: [[f32; 3]; 4] = [[25.0, 50.0, 75.0], [25.0, 27.0, 29.0], [2.0, 4.0, 6.0], [94.0, 96.0, 98.0]];
    let n = 96000;
    let freqs = log_freqs(20.0, 20000.0, 36);
    for quality in [0.0, 1.0] {
        for slope in [0.0, 1.0] {
            for xs in settings {
                let set = [("quality", quality), ("slope", slope), ("x1", xs[0]), ("x2", xs[1]), ("x3", xs[2])];
                let mut d = dut(&set, SR, 1);
                let h = run(&mut d, &impulse(n, 1.0));
                let mut r = dut(&set, SR, 1);
                r.set_test(true, true);
                r.reset();
                let href = run(&mut r, &impulse(n, 1.0));
                let tol = if quality > 0.5 { 0.05 } else { 0.1 };
                let mut worst = 0.0f64;
                for &f in &freqs {
                    worst = worst.max(dtft_db(&h, f, SR).abs());
                }
                let e: f64 = h.iter().zip(&href).map(|(a, b)| ((a - b) as f64).powi(2)).sum();
                let null = 10.0 * (e / h.iter().map(|&a| (a as f64).powi(2)).sum::<f64>()).log10();
                eprintln!("quality {quality} slope {slope} x {xs:?}: flat ±{worst:.3} dB, null {null:.0} dB");
                assert!(worst <= tol, "quality {quality} slope {slope} x {xs:?}: {worst:.3} dB off flat");
                assert!(null <= -60.0, "quality {quality} slope {slope} x {xs:?}: null {null:.0} dB");
            }
        }
    }
}

#[test]
fn droop_correction_is_doing_the_work() {
    let mut d = dut(&[], SR, 1);
    d.set_test(false, false);
    d.reset();
    let h = run(&mut d, &impulse(96000, 1.0));
    let (g10, g20) = (dtft_db(&h, 10000.0, SR), dtft_db(&h, 20000.0, SR));
    eprintln!("without correction: {g10:.2} dB at 10 kHz, {g20:.2} dB at 20 kHz");
    assert!((g10 + 0.95).abs() <= 0.1, "{g10}");
    assert!((g20 + 4.0).abs() <= 0.1 + 0.05, "{g20}");
}

#[test]
fn coherence_with_a_driven_band_at_zero() {
    let n = 96000;
    let freqs = log_freqs(20.0, 20000.0, 48);
    // Soft clip 0 % in the high-mid band, at −40 dBFS (tanh linear there): flat
    let mut d = dut(&[("hm_m1", kind_val("Soft clip"))], SR, 1);
    let h: Vec<f32> = run(&mut d, &impulse(n, 0.01)).iter().map(|v| v * 100.0).collect();
    let worst = freqs.iter().map(|&f| dtft_db(&h, f, SR).abs()).fold(0.0, f64::max);
    eprintln!("Soft clip 0 % in one band: ±{worst:.3} dB");
    assert!(worst <= 0.1, "{worst}");
    // wet 50 %, band mix 50 %: no notch deeper than 0.2 dB
    let mut d = dut(&[("hm_m1", kind_val("Soft clip")), ("wet", 50.0), ("hm_mix", 50.0)], SR, 1);
    let h: Vec<f32> = run(&mut d, &impulse(n, 0.01)).iter().map(|v| v * 100.0).collect();
    let notch = freqs.iter().map(|&f| dtft_db(&h, f, SR)).fold(0.0, f64::min);
    eprintln!("wet 50 %, mix 50 %: deepest dip {notch:.3} dB");
    assert!(notch >= -0.2, "{notch}");
    // Fuzz 0 %, blended: the accepted half sample, ≤ 0.6 dB at 20 kHz
    let mut d = dut(&[("hi_m1", kind_val("Fuzz")), ("hi_mix", 50.0)], SR, 1);
    let h: Vec<f32> = run(&mut d, &impulse(n, 0.01)).iter().map(|v| v * 100.0).collect();
    let g20 = dtft_db(&h, 20000.0, SR);
    let g5 = dtft_db(&h, 5000.0, SR);
    eprintln!("Fuzz 0 % blended 50 %: {g5:.3} dB at 5 kHz, {g20:.3} dB at 20 kHz");
    assert!((g20 - g5).abs() <= 0.6, "{g20} vs {g5}");
}

#[test]
fn crossover_laws_and_ordering() {
    // the −6 dB point of the low band sits on each crossover's law (LR4 band edge)
    for (x, f) in [(25.0f32, 112.47), (50.0, 632.46), (75.0, 3556.6)] {
        let mut d = dut(&[("x1", x), ("x2", 98.0), ("x3", 98.0), ("lo_solo", 1.0)], SR, 1);
        let h = run(&mut d, &impulse(96000, 1.0));
        let g = dtft_db(&h, f, SR);
        assert!((g + 6.02).abs() < 0.17, "x1 {x}: {g:.2} dB at {f} Hz");
    }
    assert_eq!(effective_crossovers([30.0, 20.0, 10.0]), [30.0, 32.0, 34.0]);
    assert_eq!(effective_crossovers([0.0, 0.0, 0.0]), [2.0, 4.0, 6.0]);
    assert_eq!(effective_crossovers([100.0, 100.0, 100.0]), [94.0, 96.0, 98.0]);
    assert!((curves::freq_up(25.0) - 112.47).abs() < 0.01);
}

// ---- laws ------------------------------------------------------------------------------------------

#[test]
fn prisma_laws_kept_and_ours() {
    for v in [0.0, 0.1, 0.37, 0.5, 0.9, 1.0] {
        let a = 100.0 * v;
        assert!((curves::hold_rate(a) / (40.0 * 1000f64.powf(1.0 - v)) - 1.0).abs() < 1e-3);
        assert!((curves::freq_up(a) / (20.0 * 1000f64.powf(v)) - 1.0).abs() < 1e-3);
        assert!((curves::freq_down(a) / (20.0 * 1000f64.powf(1.0 - v)) - 1.0).abs() < 1e-3);
    }
    // ours: dB, bits, dBFS linear in Amount
    assert!((curves::drive_gain(50.0) - 10f64.powf(24.0 / 20.0)).abs() < 1e-9);
    assert_eq!(curves::crush_bits(100.0), 1.0);
    assert_eq!(curves::zc_threshold_db(100.0), -6.0);
    assert_eq!(curves::gain_db(50.0), 0.0);
    // Ring mod: a 1 kHz sine at Amount 50 % (632 Hz) gives 368 Hz and 1,632 Hz
    let mut d = dut(&[("hm_m1", kind_val("Ring mod")), ("hm_v1", 50.0), ("x1", 2.0), ("x2", 4.0), ("x3", 98.0)], SR, 1);
    let n = 48000;
    let y = run(&mut d, &sine(1000.0, 0.25, n, SR));
    let s = spectrum(&y[n / 2..]);
    let bin = SR as f64 / (n / 2) as f64;
    let pk = |f: f64| s[(f / bin).round() as usize];
    let fc = curves::freq_up(50.0);
    assert!(pk(1000.0 - fc) > 20.0 * pk(1000.0), "difference tone");
    assert!(pk(1000.0 + fc) > 20.0 * pk(1000.0), "sum tone");
    // Sample divide at the hold rate: a held DC-free signal steps at R
    let mut d = dut(&[("lo_m1", kind_val("Sample divide")), ("lo_v1", 60.0)].iter().chain(ONE_BAND.iter()).copied().collect::<Vec<_>>(), SR, 1);
    let y = run(&mut d, &sine(97.0, 0.25, n, SR));
    let r = curves::hold_rate(60.0);
    // the image of 97 Hz around the hold rate
    let s = spectrum(&y[n / 2..]);
    let img = s[((r - 97.0) / bin).round() as usize].max(s[((r - 97.0) / bin).round() as usize + 1]);
    assert!(img > 1e-3 * s[(97.0 / bin).round() as usize], "image at R − f");
}

/// The test holds Prisma's formulas, typed from README §1.1.
mod prisma {
    pub fn clip_drive_db(v: f64) -> f64 {
        -20.0 * crate::convert::prisma_clip_val(v).log10()
    }
    pub fn fold_db(v: f64) -> f64 {
        20.0 * (70.0 * v * v + 1.0).log10()
    }
    pub fn bits(v: f64) -> f64 {
        (2.0 / (0.99 * v.powi(5) + 0.01)).log2()
    }
    pub fn gain_db(v: f64) -> f64 {
        40.0 * (v / 0.35).log10()
    }
    pub fn dc(v: f64) -> f64 {
        0.5 * (2.0 * v - 1.0).powi(5)
    }
    pub fn stereo_dc(v: f64) -> f64 {
        v.powi(5)
    }
    /// The Zero cross curve's gain at input level x.
    pub fn zc_gain(x: f64, v: f64) -> f64 {
        let t = v.powi(10);
        let y0 = (x - x.clamp(-t, t)) / (1.0 - t);
        let a = y0.abs();
        let pw = 3.0 / (1.0 - 0.98 * v) - 3.0;
        let y = if a < 1.0 { y0 * (1.0 - (1.0 - a.min(1.0)).powf(12.5)).powf(pw) } else { y0 };
        y / x
    }
    pub fn sine_fold(x: f64, v: f64) -> f64 {
        let g = 35.0 * v * v + 1.0;
        (std::f64::consts::PI * g * x).sin() * (5.0 * v * v).min(1.0) + x * (1.0 - 5.0 * v * v).max(0.0)
    }
}

#[test]
fn conversions_reproduce_prismas_values() {
    use convert::module_value;
    for i in 0..=100 {
        let v = i as f64 / 100.0;
        let old = 100.0 * v;
        let check = |kind: Kind, ours: f64, theirs: f64, unclamped: bool| {
            if unclamped {
                assert!((ours - theirs).abs() < 0.01, "{kind:?} v {v}: ours {ours} vs Prisma {theirs}");
            }
        };
        let a = module_value(Kind::Soft, old);
        check(Kind::Soft, 0.48 * a, prisma::clip_drive_db(v), a < 100.0 && a > 0.0);
        let a = module_value(Kind::Fold, old);
        check(Kind::Fold, 0.48 * a, prisma::fold_db(v), a < 100.0);
        let a = module_value(Kind::BitCrush, old);
        check(Kind::BitCrush, curves::crush_bits(a), prisma::bits(v), true);
        let a = module_value(Kind::Gain, old);
        check(Kind::Gain, curves::gain_db(a), prisma::gain_db(v), v >= 0.09 && a < 100.0);
        let a = module_value(Kind::Dc, old);
        check(Kind::Dc, a / 100.0 - 0.5, prisma::dc(v), true);
        let a = module_value(Kind::StereoDc, old);
        check(Kind::StereoDc, a / 100.0, prisma::stereo_dc(v), true);
        // Zero cross: our −6 dB level (T + 6 dB) within 3 dB of Prisma's curve
        if (0.1..=0.95).contains(&v) {
            let a = module_value(Kind::ZeroCross, old);
            let ours = curves::zc_threshold_db(a) + 6.0;
            let mut lvl = -120.0;
            while prisma::zc_gain(10f64.powf(lvl / 20.0), v) < 0.5 && lvl < 0.0 {
                lvl += 0.05;
            }
            assert!((ours - lvl).abs() <= 3.0, "Zero cross v {v}: ours −6 dB at {ours:.1}, Prisma at {lvl:.1} dBFS");
        }
        // Sine fold: the same fold count on a ramp 0 → 1 for v ≥ 0.45
        if v >= 0.45 {
            let a = module_value(Kind::SineFold, old);
            let c = Curve::SineFold(curves::drive_gain(a));
            let count = |f: &dyn Fn(f64) -> f64| {
                let mut n = 0;
                let mut prev = f(1e-6);
                // a ramp 0 → 0.999 (both curves are zero at x = 1 when the fold count is whole)
                for k in 1..=199800 {
                    let y = f(k as f64 / 200000.0);
                    if y.signum() != prev.signum() && y != 0.0 {
                        n += 1;
                    }
                    if y != 0.0 {
                        prev = y;
                    }
                }
                n
            };
            assert_eq!(count(&|x| c.f(x)), count(&|x| prisma::sine_fold(x, v)), "Sine fold v {v}");
        }
    }
    // the four catalogue presets (README §3.3)
    let close = |a: f64, b: f64, tol: f64| assert!((a - b).abs() <= tol, "{a} vs {b}");
    close(module_value(Kind::Gain, 53.0), 65.0, 0.5);
    close(module_value(Kind::SampleDivide, 24.0), 24.0, 1e-9);
    close(module_value(Kind::Hard, 40.0), 42.0, 0.5);
    close(0.48 * module_value(Kind::Hard, 40.0), 20.4, 0.1);
    close(curves::heavy_exp(module_value(Kind::Heavy, 55.0)), 0.455, 0.001);
    close(module_value(Kind::Soft, 25.0), 33.0, 0.5);
    close(curves::crush_bits(module_value(Kind::BitCrush, 20.0)), 7.6, 0.05);
    close(curves::crush_bits(module_value(Kind::BitCrush, 45.0)), 6.1, 0.05);
    close(module_value(Kind::Fold, 35.0), 41.0, 0.5);
    close(curves::freq_up(module_value(Kind::RingMod, 30.0)), 159.0, 1.0);
    close(convert::band_gain_db(5.0), -40.0, 0.05);
    close(convert::band_gain_db(15.0), -20.9, 0.05);
    close(convert::band_gain_db(0.0), -60.0, 0.0);
    close(convert::band_gain_db(50.0), 0.0, 1e-9);
}

// ---- antialiasing ----------------------------------------------------------------------------------

#[test]
fn aliasing_bounds() {
    let n = 16384;
    // (frequency, bound Off at 50 %, On at 50 %, Off at 100 %, On at 100 %)  [README §5]
    let rows = [(2000.0, -55.0, -75.0, -40.0, -55.0), (5000.0, -48.0, -60.0, -38.0, -46.0), (9000.0, -33.0, -63.0, -30.0, -44.0)];
    for kind in ["Soft clip", "Hard clip", "Tube"] {
        for (f, off50, on50, off100, on100) in rows {
            let (f0, k0) = bin_freq(f, n, SR);
            for (q, amount, bound) in [(0.0, 50.0, off50), (1.0, 50.0, on50), (0.0, 100.0, off100), (1.0, 100.0, on100)] {
                let mut set = all_bands(kind, amount);
                set.push(("quality", q));
                let mut d = dut(&set, SR, 1);
                let y = run(&mut d, &sine(f0, 0.5, n + 8192, SR));
                let a = alias_db(&y[8192..], k0, SR);
                eprintln!("{kind} {amount} % at {f} Hz, quality {q}: {a:.1} dB (bound {bound})");
                assert!(a <= bound, "{kind} {amount} % at {f} Hz, quality {q}: {a:.1} dB > {bound}");
            }
        }
    }
    // a 500 Hz partial in the low-mid band, Hard clip 100 %
    let (f0, k0) = bin_freq(500.0, n, SR);
    for (q, bound) in [(0.0, -54.0), (1.0, -80.0)] {
        let mut d = dut(&[("lm_m1", kind_val("Hard clip")), ("lm_v1", 100.0), ("quality", q)], SR, 1);
        let y = run(&mut d, &sine(f0, 0.5, n + 8192, SR));
        let a = alias_db(&y[8192..], k0, SR);
        eprintln!("Hard clip 100 % on 500 Hz, quality {q}: {a:.1} dB (bound {bound})");
        assert!(a <= bound, "500 Hz quality {q}: {a:.1} dB");
    }
}

/// Every ADAA curve against the same curve without ADAA at the same factor (2×, 5 kHz, Amount 50 %,
/// the gfx-dsp half-bands): ≥ 15 dB better. Reported for every family.
#[test]
fn adaa_beats_the_naive_curve() {
    use gfx_dsp::adaa::{Adaa1Fn, Antiderivative};
    use gfx_dsp::oversample::Oversampler2x;
    struct Ad(Curve);
    impl Antiderivative for Ad {
        fn f(&self, x: f64) -> f64 {
            self.0.f(x)
        }
        fn antiderivative(&self, x: f64) -> f64 {
            self.0.af(x)
        }
    }
    let n = 16384;
    let (f0, k0) = bin_freq(5000.0, n, SR);
    let x = sine(f0, 0.5, n + 4096, SR);
    let render = |c: Curve, adaa: bool| {
        let mut os = Oversampler2x::new();
        let mut a = Adaa1Fn::default();
        let ad = Ad(c);
        let y: Vec<f32> = x
            .iter()
            .map(|&v| {
                let u = os.up(v);
                let w = u.map(|s| if adaa { a.process(&ad, s as f64) as f32 } else { c.f(s as f64) as f32 });
                os.down(w)
            })
            .collect();
        alias_db(&y[4096..], k0, SR)
    };
    for comp in curves::COMPS {
        let c = curves::comp_curve(comp, 50.0);
        let (naive, adaa) = (render(c, false), render(c, true));
        eprintln!("{comp:?} 50 % at 5 kHz, 2×: naive {naive:.1} dB, ADAA1 {adaa:.1} dB");
        if matches!(comp, curves::Comp::Soft | curves::Comp::Hard | curves::Comp::Tube | curves::Comp::Fold) {
            assert!(adaa <= naive - 15.0, "{comp:?}: naive {naive:.1}, ADAA {adaa:.1}");
        }
    }
}

// ---- level -----------------------------------------------------------------------------------------

#[test]
fn level_matched_at_the_reference() {
    let amp = 10f64.powf(curves::REF_PEAK_DB / 20.0);
    let n = 48000;
    let x = sine(1000.0, amp, n, SR);
    let rin = rms(&x[n / 2..]);
    for quality in [0.0, 1.0] {
        for kind in ["Soft clip", "Hard clip", "Heavy", "Asym", "Rectify", "Fold", "Sine fold", "Zero cross", "Tube", "Fuzz"] {
            let top = if kind == "Zero cross" { 60 } else { 100 };
            let (mut worst, mut worst_top) = (0.0f64, 0.0f64);
            for a in (0..=top).step_by(10) {
                let mut set = vec![("lo_m1", kind_val(kind)), ("lo_v1", a as f32), ("quality", quality)];
                set.extend_from_slice(&ONE_BAND);
                let mut d = dut(&set, SR, 1);
                let y = run(&mut d, &x);
                let g = db(rms(&y[n / 2..]) / rin);
                if std::env::var("GRIT_VERBOSE").is_ok() {
                    eprintln!("  {kind} {a} %: {g:+.2} dB");
                }
                if a >= 100 {
                    worst_top = g.abs();
                } else {
                    worst = worst.max(g.abs());
                }
            }
            eprintln!("quality {quality} {kind}: within ±{worst:.2} dB at the reference, Amount 0 – 90 %; {worst_top:.2} dB at 100 %");
            assert!(worst <= 0.5, "{kind}: {worst:.2} dB");
            // the folders at full drive at 2×: what aliases back below 20 kHz adds energy the table
            // (harmonics below 20 kHz) does not count: measured +1.1 dB (Fold, 2×), +0.6 dB (4×)
            let top_bound = if matches!(kind, "Fold" | "Sine fold") { 1.5 } else { 0.5 };
            assert!(worst_top <= top_bound, "{kind} at 100 %: {worst_top:.2} dB");
        }
    }
}

#[test]
fn make_up_never_chases_silence() {
    for c in curves::COMPS {
        for k in 0..curves::COMP_POINTS {
            let v = COMP_TABLES[c as usize][k];
            assert!((-30.0..=12.0).contains(&v), "{c:?}[{k}] = {v}");
        }
    }
    let amp = 10f64.powf(curves::REF_PEAK_DB / 20.0);
    for (kind, a) in [("Zero cross", 100.0), ("Bit crush", 100.0)] {
        let mut set = vec![("lo_m1", kind_val(kind)), ("lo_v1", a)];
        set.extend_from_slice(&ONE_BAND);
        let mut d = dut(&set, SR, 1);
        let y = run(&mut d, &sine(1000.0, amp, 48000, SR));
        let out = db(rms(&y[24000..]));
        assert!(out < -100.0, "{kind} {a} %: {out:.1} dBFS");
    }
}

#[test]
fn heavy_knee_keeps_hiss_down() {
    let mut r = Rng::new(3);
    let amp = 10f64.powf(-90.0 / 20.0) as f32;
    let x: Vec<f32> = (0..48000).map(|_| amp * r.bipolar()).collect();
    let mut set = vec![("lo_m1", kind_val("Heavy")), ("lo_v1", 100.0)];
    set.extend_from_slice(&ONE_BAND);
    let mut d = dut(&set, SR, 1);
    let y = run(&mut d, &x);
    let out = db(peak(&y[24000..]) as f64);
    eprintln!("Heavy 100 % on −90 dBFS noise: {out:.1} dBFS peak");
    assert!(out <= -60.0, "{out}");
}

// ---- Drive and Focus -------------------------------------------------------------------------------

#[test]
fn drive_weights_per_family() {
    let sd = effective_amount(Kind::SampleDivide, 40.0, 100.0, 0.0, 0);
    let oct = (curves::hold_rate(40.0) / curves::hold_rate(sd)).log2();
    assert!((oct - 2.0).abs() < 0.02 * 2.0, "{oct} octaves");
    let bc = effective_amount(Kind::BitCrush, 40.0, 100.0, 0.0, 0);
    assert!((curves::crush_bits(40.0) - curves::crush_bits(bc) - 3.0).abs() < 0.03);
    let sc = effective_amount(Kind::Soft, 20.0, 100.0, 0.0, 0);
    assert!((0.48 * (sc - 20.0) - 24.0).abs() < 1e-9);
    let zc = effective_amount(Kind::ZeroCross, 20.0, 100.0, 0.0, 0);
    assert!((curves::zc_threshold_db(zc) - curves::zc_threshold_db(20.0) - 12.0).abs() < 1e-9);
    // Focus: ±12 dB on a saturator in the outer bands
    assert!((0.48 * (effective_amount(Kind::Hard, 50.0, 0.0, 100.0, 3) - 50.0) - 12.0).abs() < 1e-9);
    assert!((0.48 * (effective_amount(Kind::Hard, 50.0, 0.0, 100.0, 0) - 50.0) + 12.0).abs() < 1e-9);
    assert_eq!(effective_amount(Kind::Lowpass, 30.0, 100.0, 100.0, 3), 30.0);
}

fn preset(name: &str) -> Vec<(&'static str, f32)> {
    let k = kind_val;
    match name {
        "grit" => vec![("hm_m1", k("Sample divide")), ("hm_v1", 24.0), ("hm_m2", k("Gain")), ("hm_v2", 65.0), ("x1", 25.0), ("x2", 63.0), ("x3", 70.0)],
        "megaphone" => vec![
            ("lm_m1", k("Hard clip")),
            ("lm_v1", 42.0),
            ("hm_m1", k("Heavy")),
            ("hm_v1", 55.0),
            ("hm_m2", k("Peak")),
            ("hm_v2", 60.0),
            ("lo_gain", -40.0),
            ("hi_gain", -20.9),
            ("x1", 49.0),
            ("x2", 70.0),
            ("x3", 77.0),
        ],
        "telephone" => vec![
            ("lm_m1", k("Soft clip")),
            ("lm_v1", 33.0),
            ("hm_m1", k("Bit crush")),
            ("hm_v1", 56.0),
            ("hm_m2", k("Sample divide")),
            ("hm_v2", 15.0),
            ("lo_gain", -60.0),
            ("hi_gain", -60.0),
            ("x1", 39.0),
            ("x2", 50.0),
            ("x3", 74.0),
        ],
        "broken" => vec![
            ("lm_m1", k("Fold")),
            ("lm_v1", 41.0),
            ("hm_m1", k("Bit crush")),
            ("hm_v1", 66.0),
            ("hm_m2", k("Ring mod")),
            ("hm_v2", 30.0),
            ("hi_m1", k("Sample divide")),
            ("hi_v1", 40.0),
            ("x1", 25.0),
            ("x2", 50.0),
            ("x3", 75.0),
            ("wet", 80.0),
        ],
        _ => unreachable!(),
    }
}

#[test]
fn full_drive_never_silences_a_preset() {
    let x = drum_loop(96000, SR);
    for name in ["grit", "telephone", "broken", "megaphone"] {
        let mut set = preset(name);
        set.push(("drive", 100.0));
        let mut d = dut(&set, SR, 1);
        let y = run(&mut d, &x);
        let out = db(rms(&y));
        let mut d0 = dut(&preset(name), SR, 1);
        let y0 = run(&mut d0, &x);
        eprintln!("{name}: {:.1} dBFS RMS at Drive 0, {out:.1} at Drive 100 (drum loop {:.1})", db(rms(&y0)), db(rms(&x)));
        assert!(out > -40.0, "{name}: {out:.1} dBFS");
    }
}

#[test]
fn neutral_face_and_linear_bands_ignore_drive() {
    let x = drum_loop(48000, SR);
    // drive 0 / focus 0: bit-identical to the slots alone (after a trip to 60 % and back)
    let mut a = dut(&preset("broken"), SR, 1);
    let mut b = Grit::new();
    for (k, v) in preset("broken") {
        b.set_param(k, v);
    }
    b.set_param("drive", 60.0);
    b.set_param("focus", -40.0);
    b.set_param("drive", 0.0);
    b.set_param("focus", 0.0);
    b.prepare(SR, 512, 1);
    b.reset();
    assert_eq!(run(&mut a, &x), run(&mut b, &x));
    // a band of linear modules only: identical at any Drive / Focus
    let lin = [("lo_m1", kind_val("Lowpass")), ("lo_v1", 40.0), ("hm_m1", kind_val("Gain")), ("hm_v1", 70.0), ("hi_m2", kind_val("Peak")), ("hi_v2", 30.0)];
    let mut c = dut(&lin, SR, 1);
    let mut set = lin.to_vec();
    set.extend_from_slice(&[("drive", 100.0), ("focus", 100.0)]);
    let mut d = dut(&set, SR, 1);
    assert_eq!(run(&mut c, &x), run(&mut d, &x));
}

// ---- splitting a driven signal (reports) -----------------------------------------------------------

#[test]
fn intermodulation_report() {
    // E2 + B2 power chord: one band Soft clip 30 dB against four bands Soft clip 30 dB
    let n = 65536;
    let (e2, b2) = (82.41, 123.47);
    let x: Vec<f32> = (0..n).map(|i| {
        let t = i as f64 / SR as f64;
        (0.25 * (2.0 * PI * e2 * t).sin() + 0.25 * (2.0 * PI * b2 * t).sin()) as f32
    }).collect();
    let a = 62.5; // 30 dB
    let mut one = vec![("lo_m1", kind_val("Soft clip")), ("lo_v1", a)];
    one.extend_from_slice(&ONE_BAND);
    let mut four = all_bands("Soft clip", a);
    four.extend_from_slice(&[("x1", 28.0), ("x2", 50.0), ("x3", 75.0)]);
    let im = |set: &[(&str, f32)]| {
        let mut d = dut(set, SR, 1);
        let y = run(&mut d, &x);
        let s = spectrum(&y[n / 2..]);
        let bin = SR as f64 / (n / 2) as f64;
        // difference and sum products: b2 − e2, 2 e2 − b2 … against the fundamentals
        let pk = |f: f64| s[(f / bin).round() as usize];
        let prod = [b2 - e2, 2.0 * e2 - b2, 2.0 * b2 - e2, e2 + b2].iter().map(|&f| pk(f).powi(2)).sum::<f64>();
        10.0 * (prod / (pk(e2).powi(2) + pk(b2).powi(2))).log10()
    };
    let (i1, i4) = (im(&one), im(&four));
    eprintln!("power chord intermodulation: one band {i1:.1} dB, four bands {i4:.1} dB (re fundamentals)");
}

// ---- clicks and switches ---------------------------------------------------------------------------

#[test]
fn slot_changes_do_not_click() {
    let n = 48000;
    let half = n / 2;
    let x = sine(220.0, 0.3, n, SR);
    type Case<'a> = (&'a [(&'a str, f32)], (&'a str, f32));
    let cases: [Case; 6] = [
        (&[("lm_m1", 1.0), ("lm_v1", 60.0)], ("lm_m1", 2.0)),
        (&[("lm_m1", 0.0), ("lm_v1", 60.0)], ("lm_m1", 23.0)),
        (&[("lm_m1", 22.0), ("lm_v1", 10.0)], ("lm_v1", 90.0)),
        (&[("lm_solo", 0.0)], ("lm_solo", 1.0)),
        (&[("lm_m1", 5.0), ("lm_v1", 100.0)], ("lm_gain", -60.0)),
        (&[("lm_m1", 5.0), ("lm_v1", 100.0)], ("lm_bypass", 1.0)),
    ];
    for (set, (id, v)) in cases {
        let mut d = dut(set, SR, 1);
        let mut y = x.clone();
        let (a, b) = y.split_at_mut(half);
        d.process(&mut [a]);
        d.set_param(id, v);
        d.process(&mut [b]);
        let burst = burst_db(&y, half, SR);
        eprintln!("{set:?} → {id} {v}: HF burst {burst:.1} dB re peak");
        assert!(burst <= -30.0, "{id} → {v}: {burst:.1} dB");
    }
}

#[test]
fn quality_and_slope_switch_without_gap() {
    let n = 48000;
    let half = n / 2;
    let x = sine(82.41, 0.3, n, SR);
    for (from, (id, v)) in [(("quality", 0.0), ("quality", 1.0)), (("quality", 1.0), ("quality", 0.0)), (("slope", 0.0), ("slope", 1.0)), (("slope", 1.0), ("slope", 0.0))] {
        let set = [from, ("lo_m1", kind_val("Soft clip")), ("lo_v1", 40.0)];
        let mut d = dut(&set, SR, 1);
        let mut y = x.clone();
        let (a, b) = y.split_at_mut(half);
        d.process(&mut [a]);
        d.set_param(id, v);
        d.process(&mut [b]);
        // no gap: the envelope over the switch never drops below 80 % of the steady level
        let win = 600;
        let steady = rms(&y[half - 4800..half]);
        let mut low = f64::MAX;
        for k in (half..half + 4800).step_by(win / 2) {
            low = low.min(rms(&y[k..k + win]));
        }
        let burst = burst_db(&y, half, SR);
        eprintln!("{id} {} → {v}: burst {burst:.1} dB, lowest window {:.2} of steady", from.1, low / steady);
        assert!(burst <= -30.0, "{id} → {v}: {burst:.1} dB");
        // a quality switch crossfades two near-identical signals; a slope switch two dispersions
        // (LR4 / LR8 are ~120° apart at 82 Hz under a 112 Hz crossover): an equal-power dip of
        // −3 dB for a few ms, inherent to any crossfade between them (measured 0.71)
        let floor = if id == "slope" { 0.6 } else { 0.9 };
        assert!(low > floor * steady, "{id} → {v}: gap ({:.2})", low / steady);
        let want = d.want_cfg();
        assert_eq!(d.config(), want, "{id} → {v}: the new engine plays");
    }
}

// ---- stereo, determinism ---------------------------------------------------------------------------

#[test]
fn stereo_modules_are_inert_in_mono_and_mono_modules_keep_l_equal_r() {
    let x = drum_loop(24000, SR);
    let mut off = dut(&[], SR, 1);
    let y_off = run(&mut off, &x);
    for kind in ["Width", "Pan", "Stereo rectify", "Stereo DC"] {
        let mut d = dut(&[("lm_m1", kind_val(kind)), ("lm_v1", 90.0)], SR, 1);
        assert_eq!(run(&mut d, &x), y_off, "{kind} in mono");
    }
    for kind in ["Soft clip", "Fold", "Bit crush", "Sample divide", "Ring mod", "Cabinet", "Fuzz", "Zero cross"] {
        let set = [("lm_m1", kind_val(kind)), ("lm_v1", 70.0), ("hm_m2", kind_val("Heavy")), ("hm_v2", 40.0)];
        let mut m = dut(&set, SR, 1);
        let ym = run(&mut m, &x);
        let mut s = dut(&set, SR, 2);
        let mut l = x.clone();
        let mut r = x.clone();
        for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
            s.process(&mut [a, b]);
        }
        assert_eq!(l, r, "{kind}: L = R");
        assert_eq!(l, ym, "{kind}: stereo = mono");
    }
    // the stereo modules do act in 2 / 2
    let mut s = dut(&[("lm_m1", kind_val("Stereo DC")), ("lm_v1", 100.0), ("lm_m2", kind_val("Soft clip")), ("lm_v2", 60.0)], SR, 2);
    let mut l = x.clone();
    let mut r = x.clone();
    for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
        s.process(&mut [a, b]);
    }
    assert_ne!(l, r);
}

#[test]
fn phases_start_from_zero_on_reset() {
    let x = drum_loop(24000, SR);
    let set = [("hm_m1", kind_val("Ring mod")), ("hm_v1", 40.0), ("lm_m1", kind_val("Sample divide")), ("lm_v1", 70.0)];
    let mut a = dut(&set, SR, 1);
    let ya = run(&mut a, &x);
    let mut b = dut(&set, SR, 1);
    assert_eq!(ya, run(&mut b, &x));
    a.reset();
    assert_eq!(ya, run(&mut a, &x));
}

#[test]
fn every_module_stays_finite_and_bounded() {
    let mut r = Rng::new(8);
    let x: Vec<f32> = (0..9600).map(|_| 2.0 * r.bipolar()).collect();
    for k in 0..25 {
        for a in [0.0, 50.0, 100.0] {
            for sr in [44100.0, 192000.0] {
                let mut d = dut(&[("hm_m1", k as f32), ("hm_v1", a), ("lm_m2", k as f32), ("lm_v2", 100.0 - a), ("quality", 1.0), ("drive", 100.0)], sr, 2);
                let mut l = x.clone();
                let mut rr = x.clone();
                for (p, q) in l.chunks_mut(512).zip(rr.chunks_mut(512)) {
                    d.process(&mut [p, q]);
                }
                assert!(l.iter().chain(rr.iter()).all(|v| v.is_finite() && v.abs() < 100.0), "{} at {a} %", MODULE_NAMES[k]);
            }
        }
    }
}

/// CPU per sample on the README's cases (run with `--release -- --ignored`).
#[test]
#[ignore]
fn cpu_report() {
    let x = drum_loop(480000, SR);
    let mut worst = Vec::new();
    for b in ["lo", "lm", "hm", "hi"] {
        for (s, k) in [("1", "Soft clip"), ("2", "Tube")] {
            worst.push((Box::leak(format!("{b}_m{s}").into_boxed_str()) as &str, kind_val(k)));
            worst.push((Box::leak(format!("{b}_v{s}").into_boxed_str()) as &str, 50.0));
        }
    }
    worst.extend_from_slice(&[("slope", 1.0), ("quality", 1.0)]);
    let mut grit_on = preset("grit");
    grit_on.push(("quality", 1.0));
    let lr8_on = vec![("slope", 1.0), ("quality", 1.0)];
    let mut soft8 = worst.clone();
    for k in soft8.iter_mut() {
        if k.1 == kind_val("Tube") {
            k.1 = kind_val("Soft clip");
        }
    }
    for (name, set) in [("all Off", vec![]), ("Grit, Off", preset("grit")), ("Grit, On", grit_on), ("all Off, LR8, On", lr8_on), ("8 Soft clip, LR8, On", soft8), ("worst (8 saturators, LR8, On)", worst)] {
        let mut d = dut(&set, SR, 1);
        let mut y = x.clone();
        let t = std::time::Instant::now();
        for c in y.chunks_mut(128) {
            d.process(&mut [c]);
        }
        let ns = t.elapsed().as_secs_f64() * 1e9 / x.len() as f64;
        eprintln!("CPU {name}: {ns:.0} ns / sample mono");
    }
}
