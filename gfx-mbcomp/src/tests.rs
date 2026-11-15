// SPDX-License-Identifier: GPL-3.0-or-later
//! The multiband compressor's bench suite (README.md §5), on synthetic signals.

use super::*;
use gfx_dsp::crossover::Twin;
use gfx_dsp::lfo::Rng;

const SR: f32 = 48000.0;

fn mb_ch(set: &[(&str, f32)], sr: f32, ch: usize) -> MbComp {
    let mut m = MbComp::new();
    m.prepare(sr, 512, ch);
    for (k, v) in set {
        m.set_param(k, *v);
    }
    m.reset();
    m
}

fn mb(set: &[(&str, f32)]) -> MbComp {
    mb_ch(set, SR, 1)
}

fn ms(t: f32) -> usize {
    (t * 0.001 * SR) as usize
}

fn sine(n: usize, f: f64, amp: f64, sr: f32) -> Vec<f32> {
    (0..n).map(|i| (amp * (2.0 * PI * f * i as f64 / sr as f64).sin()) as f32).collect()
}

fn noise(n: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut r = Rng::new(seed);
    (0..n).map(|_| amp * r.bipolar()).collect()
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-30).log10()
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| v as f64 * v as f64).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

/// Renders mono in blocks, returning the output and the meters after each block.
fn run(m: &mut MbComp, x: &[f32], block: usize) -> (Vec<f32>, Vec<Meters>) {
    let mut y = x.to_vec();
    let mut me = Vec::new();
    for chunk in y.chunks_mut(block) {
        m.process(&mut [chunk]);
        me.push(m.meters());
    }
    (y, me)
}

/// DTFT of `h` at `freqs` (complex, f64).
fn dtft(h: &[f64], freqs: &[f64], sr: f64) -> Vec<(f64, f64)> {
    freqs
        .iter()
        .map(|&f| {
            let w = 2.0 * PI * f / sr;
            let (mut re, mut im) = (0.0, 0.0);
            for (n, &v) in h.iter().enumerate() {
                let a = w * n as f64;
                re += v * a.cos();
                im -= v * a.sin();
            }
            (re, im)
        })
        .collect()
}

fn mag_db(c: (f64, f64)) -> f64 {
    10.0 * (c.0 * c.0 + c.1 * c.1).max(1e-300).log10()
}

fn log_freqs(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    (0..n).map(|i| lo * (hi / lo).powf(i as f64 / (n - 1) as f64)).collect()
}

/// Amplitude of the component at `f` (Blackman-Harris windowed, over the whole of `x`).
fn tone(x: &[f32], f: f64, sr: f64) -> f64 {
    let n = x.len();
    let (mut re, mut im, mut wsum) = (0.0, 0.0, 0.0);
    for (i, &v) in x.iter().enumerate() {
        let t = 2.0 * PI * i as f64 / (n - 1) as f64;
        let w = 0.35875 - 0.48829 * t.cos() + 0.14128 * (2.0 * t).cos() - 0.01168 * (3.0 * t).cos();
        let a = 2.0 * PI * f * i as f64 / sr;
        re += v as f64 * w * a.cos();
        im += v as f64 * w * a.sin();
        wsum += w;
    }
    2.0 * (re * re + im * im).sqrt() / wsum
}

/// The tree's all-pass on `x` in f64 (the reference of the flat sum).
fn all_pass_ref(x: &[f32], order: Order, xs: &[f32], sr: f32) -> Vec<f64> {
    let mut t: Vec<Twin> = xs
        .iter()
        .map(|&f| {
            let mut tw = Twin::new(order);
            tw.prepare(sr, 0.0);
            tw.set_freq_now(f);
            tw
        })
        .collect();
    x.iter().map(|&v| t.iter_mut().fold(v as f64, |a, tw| tw.process(a))).collect()
}

fn null_db(y: &[f32], r: &[f64], from: usize) -> f64 {
    let e = y[from..].iter().zip(&r[from..]).map(|(&a, &b)| (a as f64 - b).abs()).fold(0.0, f64::max);
    let p = r[from..].iter().fold(0.0f64, |m, v| m.max(v.abs()));
    db(e / p)
}

const CORNERS: [[f32; 3]; 4] = [[120.0, 600.0, 2500.0], [40.0, 150.0, 800.0], [500.0, 3000.0, 12000.0], [100.0, 150.0, 225.0]];

#[test]
fn params_are_the_documented_54() {
    assert_eq!(PARAMS.len(), 54);
    let mut ids: Vec<&str> = PARAMS.iter().map(|p| p.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 54);
    for b in 0..BANDS {
        let pre = ["lo_", "lm_", "hm_", "hi_"][b];
        for (i, s) in ["thresh", "ratio", "lift_thresh", "lift", "attack", "release", "gain", "sc"].iter().enumerate() {
            assert_eq!(PARAMS[6 + 8 * b + i].id, format!("{pre}{s}"));
        }
    }
    assert_eq!(PARAMS[p::BYPASS].id, "bypass");
    assert_eq!(PARAMS[p::SOLO].max as usize + 1, SOLOS.len());
}

/// §5 Flat sum: Amount 0 %, and separately every ratio at 1 with Lift off: the output nulls against the
/// f64 all-pass A(x) · in (≤ −90 dB) and its magnitude is flat within ±0.001 dB, every slope, 3 and 4
/// bands, crossovers at the defaults, at minimum spacing and at both ends.
#[test]
fn flat_sum_is_the_all_pass() {
    let x = noise(ms(400.0), 0.3, 5);
    let freqs = log_freqs(20.0, 20000.0, 24);
    for slope in [0.0, 1.0] {
        let order = if slope > 0.5 { Order::Lr8 } else { Order::Lr4 };
        for bands in [0.0, 1.0] {
            for xs in CORNERS {
                for mode in 0..2 {
                    let mut set = vec![("slope", slope), ("bands", bands), ("x_lo", xs[0]), ("x_mid", xs[1]), ("x_hi", xs[2])];
                    if mode == 0 {
                        set.push(("amount", 0.0));
                    } else {
                        for b in ["lo", "lm", "hm", "hi"] {
                            set.push((Box::leak(format!("{b}_ratio").into_boxed_str()), 1.0));
                        }
                    }
                    let mut m = mb(&set);
                    let e = m.crossovers();
                    let used: Vec<f32> = if bands > 0.5 { e.to_vec() } else { vec![e[0], e[2]] };
                    let (y, _) = run(&mut m, &x, 256);
                    let r = all_pass_ref(&x, order, &used, SR);
                    let nd = null_db(&y, &r, 0);
                    assert!(nd < -90.0, "{order:?} bands {bands} {xs:?} mode {mode}: null {nd:.1} dB");
                    if mode == 0 {
                        let mut m = mb(&set);
                        let mut imp = vec![0.0f32; 32768];
                        imp[0] = 1.0;
                        let (h, _) = run(&mut m, &imp, 512);
                        let h: Vec<f64> = h.iter().map(|&v| v as f64).collect();
                        for c in dtft(&h, &freqs, SR as f64) {
                            assert!(mag_db(c).abs() < 1e-3, "{order:?} {xs:?}: {:.4} dB", mag_db(c));
                        }
                    }
                }
            }
        }
    }
}

/// §5 Phase independent of gain: 50 random sets of `<b>_gain`, no dynamics: the output nulls against
/// Σ g_b band_b (f64 tree) and its magnitude stays between the smallest and the largest gain.
#[test]
fn phase_is_independent_of_the_gains() {
    let mut rng = Rng::new(77);
    let x = noise(ms(200.0), 0.2, 9);
    let freqs = log_freqs(20.0, 20000.0, 16);
    for t in 0..50 {
        let four = t % 2 == 0;
        let order = if t % 4 < 2 { Order::Lr4 } else { Order::Lr8 };
        let g: [f32; 4] = core::array::from_fn(|_| (24.0 * rng.bipolar()).round());
        let set = [
            ("amount", 0.0),
            ("bands", if four { 1.0 } else { 0.0 }),
            ("slope", if order == Order::Lr8 { 1.0 } else { 0.0 }),
            ("lo_gain", g[0]),
            ("lm_gain", g[1]),
            ("hm_gain", g[2]),
            ("hi_gain", g[3]),
        ];
        let mut m = mb(&set);
        let (y, _) = run(&mut m, &x, 128);
        let map: &[usize] = if four { &[0, 1, 2, 3] } else { &[0, 1, 3] };
        let e = m.crossovers();
        let used: Vec<f32> = if four { e.to_vec() } else { vec![e[0], e[2]] };
        let mut tree = Tree::new(order, map.len());
        tree.prepare(SR, 0.0);
        for (k, &f) in used.iter().enumerate() {
            tree.set_freq_now(k, f);
        }
        let gl: Vec<f64> = map.iter().map(|&b| 10f64.powf(g[b] as f64 / 20.0)).collect();
        let mut out = [0.0; MAX_BANDS];
        let r: Vec<f64> = x
            .iter()
            .map(|&v| {
                tree.process(v as f64, &mut out);
                (0..map.len()).map(|k| gl[k] * out[k]).sum()
            })
            .collect();
        let nd = null_db(&y, &r, 0);
        assert!(nd < -90.0, "set {t} {g:?}: null {nd:.1} dB");
        if t < 10 {
            let mut m = mb(&set);
            let mut imp = vec![0.0f32; 32768];
            imp[0] = 1.0;
            let (h, _) = run(&mut m, &imp, 512);
            let h: Vec<f64> = h.iter().map(|&v| v as f64).collect();
            let gmin = map.iter().map(|&b| g[b] as f64).fold(f64::MAX, f64::min);
            let gmax = map.iter().map(|&b| g[b] as f64).fold(f64::MIN, f64::max);
            for c in dtft(&h, &freqs, SR as f64) {
                let v = mag_db(c);
                assert!(v > gmin - 1e-3 && v < gmax + 1e-3, "set {t}: {v:.3} outside [{gmin}, {gmax}]");
            }
        }
    }
}

/// The settled dynamic gain of the High Mid band for a 1 kHz sine of RMS `level` (4 bands, High Mid
/// 150 Hz – 6 kHz: 1 kHz carries 0.13 % of its amplitude in the neighbours, which pass it at unity),
/// and the output's gain.
fn band_static(set: &[(&str, f32)], level: f64) -> (f32, f64) {
    let mut base = vec![("bands", 1.0), ("x_lo", 100.0), ("x_mid", 150.0), ("x_hi", 6000.0), ("auto_gain", 0.0), ("knee", 0.0)];
    base.extend_from_slice(set);
    let mut m = mb(&base);
    let amp = 10f64.powf(level / 20.0) * std::f64::consts::SQRT_2;
    let x = sine(ms(1500.0), 1000.0, amp, SR);
    let (y, me) = run(&mut m, &x, 480);
    let tail = ms(1000.0);
    (me[me.len() - 1].dyn_db[2], db(rms(&y[tail..]) / rms(&x[tail..])))
}

/// §5 Static curve per band: downward as gfx-comp's curve in every character (±0.1 dB); Lift slopes
/// +75 % and −100 % (±0.1 dB); the caps; the guard; the output level non-decreasing in the input's.
#[test]
fn static_curve_per_band() {
    let (t, r) = (-30.0f32, 4.0f32);
    for (ci, ch) in CHARACTERS.iter().enumerate() {
        let mut prev_out = f64::MIN;
        for level in [-40.0f64, -34.0, -28.0, -24.0, -18.0, -12.0, -6.0] {
            let set = [("character", ci as f32), ("hm_thresh", t), ("hm_ratio", r), ("hm_attack", 2.0), ("hm_release", 40.0), ("range", 60.0)];
            let (dy, out) = band_static(&set, level);
            let want = static_curve(level as f32, t, r, 0.0) - level as f32;
            assert!((dy - want).abs() < 0.1, "{ch} at {level}: dyn {dy:.2}, curve {want:.2}");
            // the meter is the applied gain (the neighbours' 0.13 % at unity: +0.09 dB at 18 dB down)
            assert!((out - dy as f64).abs() < 0.15, "{ch} at {level}: output {out:.2} vs meter {dy:.2}");
            assert!(level + out >= prev_out - 1e-3, "{ch}: output level fell at {level}");
            prev_out = level + out;
        }
    }
    // Lift: +75 % (OTT's 4:1 upward) and −100 % (1:2 expansion) below −40 dB; the caps
    for (lift, lr, range, guard) in [(75.0f32, 24.0f32, 60.0f32, -96.0f32), (-100.0, 24.0, 60.0, -96.0), (75.0, 6.0, 60.0, -96.0), (-100.0, 24.0, 9.0, -96.0)] {
        let mut prev_out = f64::MIN;
        for level in [-70.0f64, -62.0, -55.0, -48.0, -44.0, -40.0, -36.0] {
            let set = [("hm_ratio", 1.0), ("hm_lift", lift), ("hm_lift_thresh", -40.0), ("lift_range", lr), ("range", range), ("guard", guard), ("hm_attack", 2.0), ("hm_release", 40.0)];
            let (dy, out) = band_static(&set, level);
            let want = lift_gain(level as f32, -40.0, lift / 100.0, 0.0, lr, guard, range);
            assert!((dy - want).abs() < 0.1, "lift {lift} range {lr}/{range} at {level}: dyn {dy:.2}, want {want:.2}");
            assert!(level + out >= prev_out - 1e-3, "lift {lift}: output level fell at {level}");
            prev_out = level + out;
        }
    }
    // the guard: boost = level − guard where that is the smallest term, 0 at and under the guard
    for level in [-80.0f64, -72.0, -70.0, -66.0] {
        let set = [("hm_ratio", 1.0), ("hm_lift", 100.0), ("hm_lift_thresh", -40.0), ("lift_range", 24.0), ("guard", -72.0), ("hm_attack", 2.0), ("hm_release", 40.0)];
        let (dy, _) = band_static(&set, level);
        let want = (level as f32 + 72.0).max(0.0);
        assert!((dy - want).abs() < 0.2, "guard at {level}: boost {dy:.2}, want {want:.2}");
    }
}

/// §5 Threshold sag: a sine at a crossover reads −3 dB in each band (BW4 detection): at threshold
/// + 6 dB and ratio 4 each band reduces by (6 − 3) · 0.75 = 2.25 dB (±0.3).
#[test]
fn threshold_sag_at_a_crossover() {
    for (k, f) in [(0usize, 120.0f64), (1, 2500.0)] {
        let set = [("auto_gain", 0.0), ("knee", 0.0), ("lo_thresh", -30.0), ("lm_thresh", -30.0), ("hi_thresh", -30.0), ("lo_ratio", 4.0), ("lm_ratio", 4.0), ("hi_ratio", 4.0), ("lo_attack", 2.0), ("lm_attack", 2.0), ("hi_attack", 2.0)];
        let mut m = mb(&set);
        let x = sine(ms(1500.0), f, 10f64.powf(-24.0 / 20.0) * std::f64::consts::SQRT_2, SR);
        let (_, me) = run(&mut m, &x, 480);
        let d = me[me.len() - 1].dyn_db;
        let slots = [[0usize, 1], [1, 3]][k];
        for s in slots {
            assert!((d[s] + 2.25).abs() < 0.3, "{f} Hz: band {s} reduces {:.2} dB", -d[s]);
        }
    }
}

/// §5 Neighbour leakage: a 41 Hz sine at −10 dBFS leaves the Mid band (threshold −24 dB) alone.
#[test]
fn neighbour_leakage() {
    let mut m = mb(&[]);
    let x = sine(ms(1500.0), 41.0, 10f64.powf(-10.0 / 20.0), SR);
    let (_, me) = run(&mut m, &x, 480);
    let d = me[me.len() - 1].dyn_db;
    assert!(d[0] < -3.0, "the low band works: {:.2}", d[0]);
    assert!(d[1] > -0.1, "Mid reduces {:.3} dB", -d[1]);
}

/// §5 Bass ripple: 41.2 / 30.9 Hz sines and an asymmetric note, low band ratio 4, attack 1 ms, about
/// 6 dB of reduction, releases 20 and 250 ms, Clean / Bus / FET: no harmonic added above −90 dBc; 90 %
/// of a +12 dB step's reduction within 20 ms; a 41 Hz sine reads within +1 dB of its RMS (+1.5 Bus).
#[test]
fn bass_ripple_free() {
    let n = ms(4000.0);
    for (ci, ch) in [(0usize, "Clean"), (1, "Bus"), (2, "FET")] {
        for rel in [20.0f32, 250.0] {
            for (name, f0, asym) in [("41.2", 41.2f64, false), ("30.9", 30.9, false), ("asym 41.2", 41.2, true)] {
                let mut m = mb(&[("character", ci as f32), ("lo_ratio", 4.0), ("lo_thresh", -26.0), ("lo_attack", 1.0), ("lo_release", rel), ("auto_release", 0.0), ("auto_gain", 0.0), ("knee", 0.0), ("range", 60.0)]);
                let x: Vec<f32> = (0..n)
                    .map(|i| {
                        let t = 2.0 * PI * f0 * i as f64 / SR as f64;
                        let v = if asym { t.sin() + 0.6 * (2.0 * t + 0.9).sin() + 0.3 * (3.0 * t + 2.0).sin() } else { t.sin() };
                        (0.35 * v) as f32
                    })
                    .collect();
                let (y, me) = run(&mut m, &x, 1);
                let gr = -me[me.len() - 1].dyn_db[0];
                assert!(gr > 3.0, "{ch} {name}: reduction {gr:.2}");
                // the gain's ripple, as the side-bands it makes (δ_pp dB → δ_pp · ln 10 / 80 re carrier)
                let g = &me[ms(2000.0)..];
                let pp = g.iter().map(|m| m.dyn_db[0]).fold(f32::MIN, f32::max) - g.iter().map(|m| m.dyn_db[0]).fold(f32::MAX, f32::min);
                let side = db(pp as f64 * std::f64::consts::LN_10 / 80.0);
                assert!(side < -90.0, "{ch} rel {rel} {name}: ripple {pp:.2e} dB, side-bands {side:.0} dBc");
                if !asym {
                    // a pure sine sits in the low band: the output's own harmonics
                    let ys = &y[ms(2000.0)..];
                    let fund = tone(ys, f0, SR as f64);
                    for h in 2..=5 {
                        let added = db(tone(ys, h as f64 * f0, SR as f64) / fund);
                        assert!(added < -90.0, "{ch} rel {rel} {name}: H{h} {added:.0} dBc");
                    }
                }
            }
        }
        // speed: +12 dB step (−21 → −9 dBFS RMS) at 1 s
        let mut m = mb(&[("character", ci as f32), ("lo_ratio", 4.0), ("lo_thresh", -20.0), ("lo_attack", 1.0), ("lo_release", 250.0), ("auto_gain", 0.0), ("knee", 0.0), ("range", 60.0)]);
        let x: Vec<f32> = (0..ms(2500.0))
            .map(|i| {
                let a = if i < ms(1000.0) { -21.0 } else { -9.0 };
                (10f64.powf(a / 20.0) * std::f64::consts::SQRT_2 * (2.0 * PI * 41.2 * i as f64 / SR as f64).sin()) as f32
            })
            .collect();
        let (_, me) = run(&mut m, &x, 1);
        let g: Vec<f32> = me.iter().map(|m| m.dyn_db[0]).collect();
        let fin = g[g.len() - 1];
        let t90 = g[ms(1000.0)..].iter().position(|&v| v <= 0.9 * fin).unwrap();
        assert!(t90 <= ms(20.0), "{ch}: 90 % after {:.1} ms", t90 as f32 / SR * 1000.0);
        // reading: at −9 dBFS RMS against −20 / 4:1, the reduction (L − T) · 0.75 gives L
        let read = -20.0 - fin / 0.75;
        let over = read + 9.0;
        let bound = if ci == 1 { 1.5 } else { 1.0 };
        assert!(over > -0.1 && over < bound, "{ch}: reads {over:+.2} dB over the RMS");
    }
}

/// The rise (dB) of a pick-like burst's peak after 1 s at −60 dBFS, through the High band with Lift
/// +75 % below −42 dB (auto gain off, no downward), for an attack time and an onset rise time.
fn pick_rise(att: f32, rise_ms: f64) -> f64 {
    let pick = |i: usize| -> f32 {
        let t = i as f64 / SR as f64;
        if t < 1.0 {
            (0.001 * (2.0 * PI * 5000.0 * t).sin()) as f32
        } else {
            let u = t - 1.0;
            let onset = if u * 1000.0 < rise_ms { 0.5 - 0.5 * (PI * u * 1000.0 / rise_ms).cos() } else { 1.0 };
            (0.5 * onset * (-u / 0.05).exp() * (2.0 * PI * 5000.0 * u).sin()) as f32
        }
    };
    let x: Vec<f32> = (0..ms(1100.0)).map(pick).collect();
    let mut m = mb(&[("hi_ratio", 1.0), ("hi_lift", 75.0), ("hi_lift_thresh", -42.0), ("hi_attack", att), ("auto_gain", 0.0), ("lift_range", 18.0), ("guard", -96.0)]);
    let (y, _) = run(&mut m, &x, 256);
    let mut r = mb(&[("amount", 0.0)]);
    let (y0, _) = run(&mut r, &x, 256);
    let pk = |v: &[f32]| v[ms(1000.0)..ms(1020.0)].iter().fold(0.0f32, |a, b| a.max(b.abs())) as f64;
    db(pk(&y) / pk(&y0))
}

/// §5 Upward vs transient (README deviation: the boost leaves with the attack's one-pole, so a peak
/// inside the first attack time keeps part of it). Measured and reported for an instant onset and a
/// 2 ms pick-like rise; bound: the rise falls with the attack, and a 0.1 ms attack on a 2 ms onset
/// stays within 1 dB.
#[test]
fn lift_and_transients() {
    let mut rep = Vec::new();
    for rise in [0.0f64, 2.0] {
        let mut prev = f64::MIN;
        for att in [0.1f32, 1.0, 2.0, 8.0, 30.0] {
            let r = pick_rise(att, rise);
            rep.push(format!("onset {rise} ms, attack {att} ms: {r:+.1} dB"));
            assert!(r >= prev - 1e-6, "onset {rise} ms: the rise grows with a shorter attack ({att} ms: {r:.2})");
            prev = r;
        }
    }
    let r = pick_rise(0.1, 2.0);
    assert!(r <= 1.0, "attack 0.1 ms on a 2 ms onset: {r:.2} dB");
    eprintln!("Lift +75 % (15.8 dB of boost before the pick):\n  {}", rep.join("\n  "));
}

fn hyper(guard: f32) -> Vec<(&'static str, f32)> {
    vec![
        ("x_lo", 90.0),
        ("x_hi", 2500.0),
        ("lo_ratio", 30.0),
        ("lm_ratio", 30.0),
        ("hi_ratio", 30.0),
        ("lo_thresh", -34.0),
        ("lm_thresh", -30.0),
        ("hi_thresh", -36.0),
        ("lo_lift", 75.0),
        ("lm_lift", 75.0),
        ("hi_lift", 75.0),
        ("lo_lift_thresh", -42.0),
        ("lm_lift_thresh", -42.0),
        ("hi_lift_thresh", -42.0),
        ("lift_range", 18.0),
        ("time", 50.0),
        ("guard", guard),
    ]
}

/// §5 Noise guard: −80 dBFS noise into Hyper with the guard at −72: every band's dynamic gain is 0 dB
/// and the output is the input plus the make-up (±1 dB); with the guard at −96: reported. A decaying
/// note: the boost never exceeds level − guard + 1 dB.
#[test]
fn noise_guard() {
    let x = noise(ms(3000.0), 10f32.powf(-80.0 / 20.0) * 3f32.sqrt(), 3);
    let tail = ms(2000.0);
    let mut m = mb(&hyper(-72.0));
    let (y, me) = run(&mut m, &x, 512);
    let mk = m.effective();
    for b in [0usize, 1, 3] {
        assert!(me[me.len() - 1].dyn_db[b].abs() < 0.05, "band {b}: dyn {}", me[me.len() - 1].dyn_db[b]);
    }
    // output = input + the make-up the bands carry (their weighted mean on white noise: report)
    let out = db(rms(&y[tail..]) / rms(&x[tail..]));
    let mks: Vec<f32> = [0usize, 1, 3].iter().map(|&b| mk[b].makeup).collect();
    assert!(mks.iter().all(|&v| out < v as f64 + 1.0) && mks.iter().any(|&v| out > v as f64 - 1.0) || (out - mks[2] as f64).abs() < 1.0, "output {out:.2} dB vs make-ups {mks:?}");
    let mut m = mb(&hyper(-96.0));
    let (y2, _) = run(&mut m, &x, 512);
    let out2 = db(rms(&y2[tail..]) / rms(&x[tail..]));
    eprintln!("noise −80 dBFS through Hyper: {out:+.1} dB with guard −72 (make-ups {mks:?}), {out2:+.1} dB with guard −96");
    // a decaying note in the Mid band: boost ≤ level − guard + 1 at every instant
    let n = ms(4000.0);
    let x: Vec<f32> = (0..n).map(|i| (0.3 * (-(i as f64) / SR as f64 / 0.4).exp() * (2.0 * PI * 700.0 * i as f64 / SR as f64).sin()) as f32).collect();
    let mut m = mb(&[("lm_ratio", 1.0), ("lm_lift", 100.0), ("lm_lift_thresh", -30.0), ("lift_range", 24.0), ("guard", -72.0), ("auto_gain", 0.0)]);
    let (_, me) = run(&mut m, &x, 64);
    for (k, mt) in me.iter().enumerate() {
        let i = (k + 1) * 64;
        if i >= n || i < ms(50.0) {
            continue;
        }
        let level = db(0.3 * (-(i as f64) / SR as f64 / 0.4).exp() / std::f64::consts::SQRT_2) as f32;
        let boost = mt.dyn_db[1];
        assert!(boost <= (level + 72.0).max(0.0) + 1.0, "at {:.0} ms: boost {boost:.2} at level {level:.1}", i as f32 / SR * 1000.0);
    }
}

/// §5 Amount / Tilt / auto gain: every band's net gain at R (−24 dBFS RMS) stays within ±0.5 dB of 0
/// for Amount 0 – 200 % and Tilt ±100 %; Amount 100 % and Tilt 0 are the bands alone; the ratio never
/// passes 30:1 and the output stays monotone in the input level at Amount 200 %.
#[test]
fn amount_tilt_auto_gain() {
    for (amount, tilt) in [(0.0f32, 0.0f32), (100.0, 0.0), (200.0, 0.0), (100.0, 100.0), (100.0, -100.0), (200.0, 100.0)] {
        for (b, f) in [(0usize, 60.0f64), (1, 700.0), (3, 6000.0)] {
            let mut m = mb(&[("amount", amount), ("tilt", tilt), ("lo_lift", 50.0), ("hi_lift", 50.0)]);
            let x = sine(ms(2000.0), f, 10f64.powf(-24.0 / 20.0) * std::f64::consts::SQRT_2, SR);
            let (y, _) = run(&mut m, &x, 512);
            let tail = ms(1500.0);
            let net = db(rms(&y[tail..]) / rms(&x[tail..]));
            assert!(net.abs() < 0.5, "amount {amount} tilt {tilt} band {b}: net {net:+.2} dB at R");
        }
    }
    let v: [f32; NP] = core::array::from_fn(|i| PARAMS[i].default);
    let e = effective_bands(&v);
    for b in 0..BANDS {
        assert_eq!(e[b].thresh, v[p::thresh(b)]);
        assert_eq!(e[b].ratio, v[p::ratio(b)]);
        assert_eq!(e[b].attack, v[p::attack(b)]);
    }
    for r in [1.0f32, 2.5, 10.0, 30.0] {
        for a in [0.0f32, 50.0, 161.0, 200.0] {
            let rr = effective_ratio(r, a);
            assert!((1.0..=MAX_RATIO + 1e-3).contains(&rr), "{r} at {a}: {rr}");
        }
    }
    assert!((effective_ratio(2.5, 161.0) - 30.0).abs() < 1.0);
    for ci in 0..5 {
        let mut prev = f64::MIN;
        for level in [-40.0f64, -30.0, -20.0, -10.0, -3.0] {
            let (_, out) = band_static(&[("amount", 200.0), ("character", ci as f32), ("hm_ratio", 30.0), ("hm_thresh", -30.0), ("hm_attack", 2.0), ("range", 60.0)], level);
            assert!(level + out >= prev - 0.05, "character {ci}: output fell at {level}");
            prev = level + out;
        }
    }
}

/// §5 Linear mode: reported latency L per rate family; the impulse peaks at L; flat magnitude (±0.01 dB
/// at the 4-band defaults, ±0.05 dB at every corner, both slopes); the gains are Minimum's, L later.
#[test]
fn linear_mode() {
    for (sr, want) in [(44100.0f32, 2048usize), (48000.0, 2048), (96000.0, 4096), (192000.0, 8192)] {
        let mut m = mb_ch(&[("phase", 1.0), ("amount", 0.0), ("bands", 1.0)], sr, 1);
        assert_eq!(m.latency(), want, "{sr}");
        let mut imp = vec![0.0f32; 2 * want];
        imp[0] = 1.0;
        let (h, _) = run(&mut m, &imp, 256);
        let pk = (0..h.len()).max_by(|&a, &b| h[a].abs().total_cmp(&h[b].abs())).unwrap();
        assert_eq!(pk, want, "{sr}: impulse peak");
        let mut m = mb_ch(&[("phase", 0.0)], sr, 1);
        assert_eq!(m.latency(), 0);
        m.set_param("phase", 1.0);
        let _ = run(&mut m, &vec![0.0; 4096], 512);
        assert_eq!(m.latency(), want, "{sr}: after the switch");
    }
    let freqs = log_freqs(20.0, 20000.0, 48);
    for slope in [0.0f32, 1.0] {
        for (xs, tol) in [([120.0f32, 600.0, 2500.0], 0.01f64), ([40.0, 150.0, 800.0], 0.05), ([500.0, 3000.0, 12000.0], 0.05), ([40.0, 3000.0, 12000.0], 0.05), ([500.0, 750.0, 1125.0], 0.05)] {
            for bands in [0.0f32, 1.0] {
                let mut m = mb(&[("phase", 1.0), ("amount", 0.0), ("bands", bands), ("slope", slope), ("x_lo", xs[0]), ("x_mid", xs[1]), ("x_hi", xs[2])]);
                let mut imp = vec![0.0f32; 2048 + 65536];
                imp[0] = 1.0;
                let (h, _) = run(&mut m, &imp, 512);
                let h: Vec<f64> = h.iter().map(|&v| v as f64).collect();
                let (lo, hi) = dtft(&h, &freqs, SR as f64).into_iter().map(mag_db).fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(v), b.max(v)));
                let tol = if bands < 0.5 && xs[0] == 120.0 { 0.01 } else { tol };
                assert!(lo > -tol && hi < tol, "slope {slope} bands {bands} {xs:?}: {lo:.4} / {hi:.4} dB");
            }
        }
    }
    // dynamics identical: the meters per sample, Linear = Minimum L samples later (bit for bit)
    let x: Vec<f32> = noise(ms(500.0), 0.5, 4).iter().enumerate().map(|(i, v)| if i % 9000 < 3000 { *v } else { 0.05 * v }).collect();
    let set = [("bands", 1.0), ("lo_lift", 60.0), ("hi_ratio", 6.0), ("hm_sc", 1.0)];
    let mut a = mb(&set);
    let mut b = mb(&[set.as_slice(), &[("phase", 1.0)]].concat());
    let (_, ma) = run(&mut a, &x, 1);
    let (_, mbm) = run(&mut b, &x, 1);
    for i in 0..x.len() - 2048 {
        assert_eq!(ma[i].dyn_db, mbm[i + 2048].dyn_db, "at {i}");
    }
}

/// §5 Mix / delta: at Amount 0, `delta` is silence; `mix` 50 % never notches (dry aligned).
#[test]
fn mix_and_delta() {
    let x = noise(ms(500.0), 0.5, 8);
    let mut m = mb(&[("amount", 0.0), ("delta", 1.0), ("lo_gain", 0.0)]);
    let (y, _) = run(&mut m, &x, 128);
    let pk = y.iter().fold(0.0f32, |a, b| a.max(b.abs()));
    assert!(db(pk as f64) <= -120.0, "delta at Amount 0: {:.1} dBFS", db(pk as f64));
    for phase in [0.0f32, 1.0] {
        let mut m = mb(&[("amount", 0.0), ("mix", 50.0), ("lo_gain", -24.0), ("lm_gain", 12.0), ("hi_gain", -12.0), ("phase", phase)]);
        let mut imp = vec![0.0f32; 2048 + 32768];
        imp[0] = 1.0;
        let (h, _) = run(&mut m, &imp, 512);
        let h: Vec<f64> = h.iter().map(|&v| v as f64).collect();
        let lowest = 20.0 * (0.5 + 0.5 * 10f64.powf(-24.0 / 20.0)).log10();
        for c in dtft(&h, &log_freqs(20.0, 20000.0, 48), SR as f64) {
            assert!(mag_db(c) > lowest - 0.2, "phase {phase}: {:.2} dB under the blend's {lowest:.2}", mag_db(c));
        }
    }
}

/// §5 Bypass null: output = input bit for bit (Minimum), the input delayed by L (Linear).
#[test]
fn bypass_is_a_null() {
    let x = noise(ms(300.0), 0.5, 2);
    let mut m = mb(&[("bypass", 1.0), ("hi_lift", 80.0)]);
    let (y, _) = run(&mut m, &x, 100);
    assert_eq!(y, x);
    let mut m = mb(&[("bypass", 1.0), ("phase", 1.0)]);
    let (y, _) = run(&mut m, &x, 100);
    assert!(y[..2048].iter().all(|&v| v == 0.0));
    assert_eq!(&y[2048..], &x[..x.len() - 2048]);
}

/// §5 Stereo: equal inputs give L = R bit for bit (and the mono output); `link` 0 % leaves a quiet
/// right channel unprocessed while the left compresses.
#[test]
fn stereo() {
    let x = noise(ms(300.0), 0.5, 12);
    let set = [("bands", 1.0), ("lo_lift", 50.0), ("hi_ratio", 5.0)];
    let mut s = mb_ch(&set, SR, 2);
    let (mut l, mut r) = (x.clone(), x.clone());
    s.process(&mut [&mut l[..], &mut r[..]]);
    assert_eq!(l, r);
    let mut m = mb(&set);
    let (y, _) = run(&mut m, &x, x.len());
    assert_eq!(l, y);
    // link 0: L loud, R quiet (below every threshold)
    let quiet = sine(ms(500.0), 700.0, 0.005, SR);
    let loud = sine(ms(500.0), 700.0, 0.8, SR);
    let mut s = mb_ch(&[("link", 0.0), ("auto_gain", 0.0)], SR, 2);
    let (mut l, mut r) = (loud.clone(), quiet.clone());
    s.process(&mut [&mut l[..], &mut r[..]]);
    let mut m = mb(&[("auto_gain", 0.0)]);
    let (rq, _) = run(&mut m, &quiet, quiet.len());
    assert_eq!(r, rq, "right channel touched by the left");
    assert!(db(rms(&l[ms(300.0)..]) / rms(&loud[ms(300.0)..])) < -3.0);
}

/// §5 Meters: `<b>_dyn` negative for a reduction, positive for a boost; `gr` the largest reduction.
#[test]
fn meters() {
    let x: Vec<f32> = sine(ms(1500.0), 60.0, 0.7, SR).iter().zip(sine(ms(1500.0), 6000.0, 0.0005, SR)).map(|(a, b)| a + b).collect();
    let mut m = mb(&[("hi_lift", 75.0), ("hi_lift_thresh", -40.0), ("guard", -96.0)]);
    let (_, me) = run(&mut m, &x, 512);
    let last = me[me.len() - 1];
    assert!(last.dyn_db[0] < -3.0, "low band reduces: {}", last.dyn_db[0]);
    assert!(last.dyn_db[3] > 3.0, "high band lifted: {}", last.dyn_db[3]);
    assert_eq!(last.dyn_db[2], 0.0, "High Mid reads 0 in 3 bands");
    let gr = -last.dyn_db.iter().cloned().fold(0.0f32, f32::min);
    assert!((last.gr - gr).abs() < 0.5 && last.gr > 0.0, "gr {} vs {gr}", last.gr);
}

/// Solo: one band heard after its gain (5 ms ramp); Low Mid and High Mid both solo Mid in 3 bands.
#[test]
fn solo_hears_one_band() {
    let x = sine(ms(500.0), 30.0, 0.1, SR);
    for (solo, want_heard) in [(1.0f32, true), (2.0, false), (3.0, false), (4.0, false)] {
        let mut m = mb(&[("solo", solo), ("amount", 0.0)]);
        let (y, _) = run(&mut m, &x, 256);
        let r = db(rms(&y[ms(300.0)..]) / rms(&x[ms(300.0)..]));
        if want_heard {
            assert!(r > -0.1, "solo {solo}: {r:.2}");
        } else {
            assert!(r < -30.0, "solo {solo}: {r:.2}");
        }
    }
    let x = sine(ms(500.0), 700.0, 0.1, SR);
    for solo in [2.0f32, 3.0] {
        let mut m = mb(&[("solo", solo), ("amount", 0.0)]);
        let (y, _) = run(&mut m, &x, 256);
        assert!(db(rms(&y[ms(300.0)..]) / rms(&x[ms(300.0)..])) > -0.5, "solo {solo} hears Mid");
    }
}

/// `bands` and `slope` switch by pre-roll and crossfade: no gap, no burst, the new layout plays.
#[test]
fn structural_switches_are_smooth() {
    for (id, to) in [("bands", 1.0f32), ("slope", 1.0)] {
        for phase in [0.0f32, 1.0] {
            let mut m = mb(&[("phase", phase)]);
            let x = sine(ms(600.0), 82.0, 0.3, SR);
            let mut y = x.clone();
            let (a, b) = y.split_at_mut(ms(300.0));
            m.process(&mut [a]);
            m.set_param(id, to);
            for c in b.chunks_mut(128) {
                m.process(&mut [c]);
            }
            assert!(m.config() == Config { order: if id == "slope" { Order::Lr8 } else { Order::Lr4 }, four: id == "bands" });
            // the envelope never dips below half nor exceeds the level by more than 1 dB around the switch
            let from = ms(300.0) + if phase > 0.5 { 2048 } else { 0 };
            let win = ms(13.0);
            let mut k = from;
            while k + win < ms(400.0) + from - ms(300.0) {
                let pk = y[k..k + win].iter().fold(0.0f32, |a, b| a.max(b.abs()));
                assert!(pk > 0.15 && pk < 0.34, "{id} phase {phase} at {k}: peak {pk}");
                k += win;
            }
        }
    }
}

/// §5 Clicks: under a held 82 Hz note, switches and sweeps leave no high-frequency burst above the
/// bench's default bound (−50 dB re peak, energy above 10 kHz in the 100 ms after the change).
#[test]
fn no_clicks_under_a_held_note() {
    use gfx_dsp::svf::{Svf, SvfMode};
    let hp = |x: &[f32]| -> Vec<f32> {
        let mut a = Svf::new();
        let mut b = Svf::new();
        a.set(SvfMode::HighPass, 10000.0, 0.541_196, 0.0, SR);
        b.set(SvfMode::HighPass, 10000.0, 1.306_563, 0.0, SR);
        x.iter().map(|&v| b.process(a.process(v))).collect()
    };
    type Case = (&'static str, &'static [(&'static str, f32)], &'static str, f32);
    let cases: [Case; 12] = [
        ("bands 3 → 4", &[], "bands", 1.0),
        ("bands 4 → 3", &[("bands", 1.0)], "bands", 0.0),
        ("slope", &[], "slope", 1.0),
        ("phase → Linear", &[], "phase", 1.0),
        ("phase → Minimum", &[("phase", 1.0)], "phase", 0.0),
        ("x_lo across 80 Hz, Linear LR8", &[("phase", 1.0), ("slope", 1.0), ("x_lo", 100.0)], "x_lo", 60.0),
        ("x_lo 50 Hz step", &[("x_lo", 150.0)], "x_lo", 200.0),
        ("character", &[], "character", 2.0),
        ("solo", &[], "solo", 1.0),
        ("bypass", &[], "bypass", 1.0),
        ("amount 0 → 200", &[("amount", 0.0)], "amount", 200.0),
        ("amount 200 → 0", &[("amount", 200.0)], "amount", 0.0),
    ];
    let n = ms(1000.0);
    let x = sine(n, 82.0, 0.5, SR);
    let mut rep = Vec::new();
    for (name, set, id, to) in cases {
        let mut m = mb(set);
        let mut y = x.clone();
        let half = n / 2;
        let (a, b) = y.split_at_mut(half);
        m.process(&mut [a]);
        m.set_param(id, to);
        for c in b.chunks_mut(128) {
            m.process(&mut [c]);
        }
        let pk = y.iter().fold(0.0f32, |a, b| a.max(b.abs())) as f64;
        let h = hp(&y);
        let after = h[half..half + n / 5].iter().fold(0.0f32, |a, b| a.max(b.abs())) as f64;
        let steady = h[n / 4..half].iter().fold(0.0f32, |a, b| a.max(b.abs())) as f64;
        let rel = db((after - steady).max(1e-12) / pk);
        rep.push(format!("{name}: {rel:.0} dB"));
        assert!(rel < -50.0, "{name}: HF burst {rel:.1} dB re peak");
    }
    eprintln!("clicks under 82 Hz: {}", rep.join(", "));
}

/// §5 Attack splatter (reported): 82 Hz tone bursts, the low band reducing by about 6 dB, attack 0.1,
/// 1 and 30 ms: the energy above 1 kHz relative to the burst (a split's side-bands are not filtered
/// back into the band, §2.2). Faster attacks splatter more.
#[test]
fn attack_splatter() {
    use gfx_dsp::svf::{Svf, SvfMode};
    let n = ms(2000.0);
    // bursts with 2 ms raised-cosine edges, 0.6 peak over a 0.03 floor
    let x: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f64 / SR as f64;
            let u = t % 0.5;
            let e = |v: f64| if v <= 0.0 { 0.0 } else if v >= 0.002 { 1.0 } else { 0.5 - 0.5 * (PI * v / 0.002).cos() };
            let env = 0.03 + 0.57 * e(u).min(e(0.25 - u));
            (env * (2.0 * PI * 82.0 * t).sin()) as f32
        })
        .collect();
    let mut r0 = mb(&[("amount", 0.0)]);
    let (y0, _) = run(&mut r0, &x, 256);
    let mut rep = Vec::new();
    let mut prev = f64::MIN;
    for att in [30.0f32, 1.0, 0.1] {
        let mut m = mb(&[("lo_thresh", -20.0), ("lo_ratio", 4.0), ("lo_attack", att), ("lo_release", 100.0), ("auto_gain", 0.0)]);
        let (y, _) = run(&mut m, &x, 256);
        let mut a = Svf::new();
        let mut b = Svf::new();
        a.set(SvfMode::HighPass, 1000.0, 0.541_196, 0.0, SR);
        b.set(SvfMode::HighPass, 1000.0, 1.306_563, 0.0, SR);
        // what the dynamics added above 1 kHz, re the output
        let h: Vec<f32> = y.iter().zip(&y0).map(|(&v, &w)| b.process(a.process(v - w))).collect();
        let r = db(rms(&h[ms(500.0)..]) / rms(&y[ms(500.0)..]));
        rep.push(format!("attack {att} ms: {r:.0} dB"));
        assert!(r >= prev - 0.5, "splatter falls with a faster attack");
        prev = r;
    }
    eprintln!("82 Hz bursts, energy above 1 kHz: {}", rep.join(", "));
}

/// Every parameter at its ends, a burst and silence: finite, and the tail falls silent.
#[test]
fn extremes_stay_finite() {
    let x = noise(ms(500.0), 4.0, 1);
    for pi in PARAMS.iter() {
        for v in [pi.min, pi.max] {
            let mut m = mb(&[(pi.id, v)]);
            let (y, _) = run(&mut m, &x, 512);
            assert!(y.iter().all(|s| s.is_finite() && s.abs() < 1e3), "{} = {v}", pi.id);
        }
    }
}

/// CPU per sample (release: `cargo test --release -p gfx-mbcomp cpu -- --ignored --nocapture`).
#[test]
#[ignore]
fn cpu() {
    let n = (SR * 10.0) as usize;
    let x = noise(n, 0.3, 13);
    for (name, set) in [
        ("3-band LR4 Minimum", vec![]),
        ("4-band LR4 Minimum", vec![("bands", 1.0)]),
        ("4-band LR8 Minimum", vec![("bands", 1.0), ("slope", 1.0)]),
        ("4-band LR4 Linear", vec![("bands", 1.0), ("phase", 1.0)]),
        ("4-band LR8 Linear", vec![("bands", 1.0), ("slope", 1.0), ("phase", 1.0)]),
    ] {
        let mut m = mb(&set);
        let t = std::time::Instant::now();
        let _ = run(&mut m, &x, 128);
        eprintln!("{name}: {:.0} ns / sample mono", t.elapsed().as_secs_f64() * 1e9 / n as f64);
    }
}
