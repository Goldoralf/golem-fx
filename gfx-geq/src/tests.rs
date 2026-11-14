// SPDX-License-Identifier: GPL-3.0-or-later
//! The graphic EQ's own bench suite (README §7): the accuracy table of README §3.3 reproduced on the
//! solver, then the running processor (flat, realisation, modulation, switches, auto gain, cost).
//!
//! `GEQ_DRAWS=10000 cargo test -p gfx-geq --release -- --nocapture accuracy` runs the bench's full
//! draw count (1,000 by default) and prints the table.

use super::*;
use gfx_dsp::geq::Plan;
use std::f64::consts::PI;

const RATES: [f64; 4] = [44100.0, 48000.0, 96000.0, 192000.0];

fn draws() -> usize {
    std::env::var("GEQ_DRAWS").ok().and_then(|v| v.parse().ok()).unwrap_or(1000)
}

struct Rng(gfx_dsp::lfo::Rng);
impl Rng {
    fn new(seed: u64) -> Self {
        Self(gfx_dsp::lfo::Rng::new(seed))
    }
    fn uniform(&mut self, amp: f64) -> f64 {
        amp * self.0.bipolar() as f64
    }
}

/// Measured on one fader setting: worst error at a fader, worst excursion outside the two
/// neighbouring fader values (1/48 octave from the first to the last fader, up to 0.45 fs), worst
/// distance to `flat` on that grid (all-up), largest filter gain, cold-start corrections.
struct Eval {
    at_faders: f64,
    outside: f64,
    vs_flat: f64,
    gmax: f64,
    corrections: usize,
}

fn evaluate(plan: &Plan, fad: &[f64], flat: f64) -> Eval {
    let mut t = [0.0; MAX_POINTS];
    let mut g = [0.0; MAX_SECTIONS];
    let mut d = [UNITY; MAX_SECTIONS];
    plan.targets(fad, &mut t);
    let corrections = plan.solve(&t, &mut g, &mut d);
    let bands = plan.bands();
    let mut at_faders = 0.0f64;
    for (k, &f) in bands.iter().enumerate() {
        at_faders = at_faders.max((plan.response_db(&d, f) - fad[k]).abs());
    }
    let (lo, hi) = (bands[0], bands[bands.len() - 1].min(0.45 * plan.sample_rate()));
    let steps = (48.0 * (hi / lo).log2()).floor() as usize;
    let (mut outside, mut vs_flat) = (0.0f64, 0.0f64);
    for s in 0..=steps {
        let f = lo * (s as f64 / 48.0).exp2();
        let r = plan.response_db(&d, f);
        let k = bands.iter().rposition(|&b| b <= f).unwrap_or(0).min(bands.len() - 2);
        let (a, b) = (fad[k].min(fad[k + 1]), fad[k].max(fad[k + 1]));
        outside = outside.max((a - r).max(r - b).max(0.0));
        vs_flat = vs_flat.max((r - flat).abs());
    }
    let gmax = g[..plan.len()].iter().fold(0.0f64, |m, v| m.max(v.abs()));
    Eval { at_faders, outside, vs_flat, gmax, corrections }
}

/// README §3.3's accuracy table, one layout, every rate: random draws at ±12 and ±15 dB, all faders
/// up, zigzags. Asserts the bench's thresholds (README §7) and prints the measured row.
fn accuracy(l: usize, outside_bound: f64, allup_bound: f64, zig15_bound: f64) {
    let n = draws();
    let freqs = layout(l).0;
    let m = freqs.len();
    let (mut c12, mut out12, mut g12, mut c15, mut g15, mut its, mut allup, mut zig12, mut zig15) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0usize, 0.0f64, 0.0f64, 0.0f64);
    for &sr in &RATES {
        let plan = Plan::new(freqs, sr);
        let mut rng = Rng::new(7 + l as u64);
        for _ in 0..n {
            let fad: Vec<f64> = (0..m).map(|_| rng.uniform(12.0)).collect();
            let e = evaluate(&plan, &fad, 0.0);
            c12 = c12.max(e.at_faders);
            out12 = out12.max(e.outside);
            g12 = g12.max(e.gmax);
            its = its.max(e.corrections);
            let fad: Vec<f64> = (0..m).map(|_| rng.uniform(15.0)).collect();
            let e = evaluate(&plan, &fad, 0.0);
            c15 = c15.max(e.at_faders);
            g15 = g15.max(e.gmax);
            its = its.max(e.corrections);
        }
        allup = allup.max(evaluate(&plan, &vec![12.0; m], 12.0).vs_flat);
        let zz = |a: f64| (0..m).map(|k| if k % 2 == 0 { a } else { -a }).collect::<Vec<f64>>();
        zig12 = zig12.max(evaluate(&plan, &zz(12.0), 0.0).at_faders);
        zig15 = zig15.max(evaluate(&plan, &zz(15.0), 0.0).at_faders);
    }
    println!(
        "| {} | {c12:.2} dB | ≤ {out12:.2} dB | +12 ± {allup:.2} dB | {zig12:.2} dB | {g12:.1} dB (±15: {g15:.1}) | {c15:.2} dB (zigzag {zig15:.2}) | {its} |",
        LAYOUTS[l]
    );
    assert!(c12 <= 0.25, "{}: {c12:.3} dB at a fader (±12)", LAYOUTS[l]);
    assert!(c15 <= 0.35, "{}: {c15:.3} dB at a fader (±15)", LAYOUTS[l]);
    assert!(out12 <= outside_bound, "{}: {out12:.3} dB outside the neighbouring faders", LAYOUTS[l]);
    assert!(allup <= allup_bound, "{}: all up +12 ± {allup:.3}", LAYOUTS[l]);
    assert!(zig12 <= 0.3, "{}: zigzag ±12 {zig12:.3}", LAYOUTS[l]);
    assert!(zig15 <= zig15_bound, "{}: zigzag ±15 {zig15:.3}", LAYOUTS[l]);
    assert!(its <= 8, "{}: {its} cold-start corrections", LAYOUTS[l]);
    assert!(g12 < gfx_dsp::geq::GAIN_CLAMP_DB, "{}: a ±12 draw hits the clamp", LAYOUTS[l]);
}

#[test]
fn accuracy_octave_11() {
    accuracy(0, 0.75, 0.75, 0.4);
}

#[test]
fn accuracy_guitar_7() {
    accuracy(1, 0.75, 0.5, 0.4);
}

#[test]
fn accuracy_bass_7() {
    accuracy(2, 1.25, 0.75, 1.5);
}

#[test]
fn lone_fader() {
    for &sr in &RATES {
        let plan = Plan::new(&OCTAVE, sr);
        let mut fad = [0.0; 11];
        fad[5] = 12.0;
        let mut t = [0.0; MAX_POINTS];
        let mut g = [0.0; MAX_SECTIONS];
        let mut d = [UNITY; MAX_SECTIONS];
        plan.targets(&fad, &mut t);
        plan.solve(&t, &mut g, &mut d);
        let at = |f: f64| plan.response_db(&d, f);
        assert!((at(1000.0) - 12.0).abs() <= 0.1, "{sr}: {:.3} at 1 kHz", at(1000.0));
        assert!(at(500.0).abs() <= 0.1 && at(2000.0).abs() <= 0.1, "{sr}: {:.3} / {:.3}", at(500.0), at(2000.0));
        let mut low = 0.0f64;
        let mut f = 20.0;
        while f < 20000.0f64.min(0.45 * sr) {
            low = low.min(at(f));
            f *= 1.01;
        }
        assert!(low >= -0.5, "{sr}: dips to {low:.3}");
    }
}

#[test]
fn top_band_at_44k1() {
    let plan = Plan::new(&OCTAVE, 44100.0);
    for &a in &[12.0, -12.0] {
        let mut fad = [0.0; 11];
        fad[10] = a;
        let e = evaluate(&plan, &fad, 0.0);
        assert!(e.at_faders <= 0.25, "16 kHz {a}: {:.3}", e.at_faders);
    }
}

// ---------- the running processor ----------

fn geq(sr: f32, settings: &[(&str, f32)]) -> Geq {
    let mut e = Geq::new();
    e.prepare(sr, 4096, 2);
    for &(id, v) in settings {
        e.set_param(id, v);
    }
    e.reset();
    e
}

fn render(e: &mut Geq, x: &[f32], block: usize) -> Vec<f32> {
    let mut y = x.to_vec();
    for c in y.chunks_mut(block) {
        e.process(&mut [c]);
    }
    y
}

fn noise(n: usize, seed: u64) -> Vec<f32> {
    let mut r = gfx_dsp::lfo::Rng::new(seed);
    (0..n).map(|_| 0.25 * r.bipolar()).collect()
}

fn sine(n: usize, f: f64, sr: f64) -> Vec<f32> {
    (0..n).map(|i| (0.25 * (2.0 * PI * f * i as f64 / sr).sin()) as f32).collect()
}

const IDS: [&str; 11] = ["g1", "g2", "g3", "g4", "g5", "g6", "g7", "g8", "g9", "g10", "g11"];

#[test]
fn flat_is_bit_transparent() {
    let x = noise(48000, 3);
    // every layout at 0; amount 0 under a curve
    for l in 0..3 {
        let mut e = geq(48000.0, &[("layout", l as f32)]);
        assert_eq!(render(&mut e, &x, 256), x, "layout {l}");
    }
    let mut set: Vec<(&str, f32)> = IDS.iter().enumerate().map(|(k, id)| (*id, if k % 2 == 0 { 9.0 } else { -6.0 })).collect();
    set.push(("amount", 0.0));
    let mut e = geq(48000.0, &set);
    assert_eq!(render(&mut e, &x, 256), x, "amount 0");
    // back to flat while running: the cascade fades out after 50 ms, then the output is the input
    let mut e = geq(48000.0, &[("g6", 6.0), ("gt_800", 4.0)]);
    let _ = render(&mut e, &x, 256);
    e.set_param("g6", 0.0);
    let _ = render(&mut e, &x, 256);
    assert_eq!(render(&mut e, &x, 256), x, "after returning to flat");
}

/// The SVF realises the matched designs: the running plugin's impulse response, by DFT, against the
/// designed cascade (README §7 Realisation), including the large low sections of a ±15 zigzag at
/// 192 kHz and a 30 dB section at 31.25 Hz.
#[test]
fn realisation_matches_the_designs() {
    for &sr in &[48000.0f64, 192000.0] {
        let set: Vec<(&str, f32)> = IDS.iter().enumerate().map(|(k, id)| (*id, if k % 2 == 0 { 15.0 } else { -15.0 })).collect();
        let mut e = geq(sr as f32, &set);
        let n = sr as usize * 2;
        let mut x = vec![0.0f32; n];
        x[0] = 1.0;
        let y = render(&mut e, &x, 512);
        let s = &e.slots[e.cur];
        let plan = &e.plans[0];
        let mut worst = 0.0f64;
        let mut f = 25.0;
        while f < 0.45 * sr {
            let w = 2.0 * PI * f / sr;
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (i, v) in y.iter().enumerate() {
                re += *v as f64 * (w * i as f64).cos();
                im -= *v as f64 * (w * i as f64).sin();
            }
            let got = 10.0 * (re * re + im * im).log10();
            worst = worst.max((got - plan.response_db(&s.designs, f)).abs());
            f *= 1.25;
        }
        assert!(worst <= 0.05, "{sr}: impulse response {worst:.3} dB off the designs");
    }
    // one 30 dB section at 31.25 Hz, 192 kHz
    let sr = 192000.0;
    let b = gfx_dsp::geq::FixedBell::new(31.25, sr).design(gfx_dsp::geq::bell_q(30.0, 2f64.sqrt(), 0.4), 30.0);
    let mut s = Svf::new();
    s.set_biquad(&b);
    for &f in &[20.0f32, 31.25, 50.0, 200.0] {
        let got = gfx_dsp::svf::measure_gain_db(|x| s.process(x), f, sr as f32) as f64;
        let want = 10.0 * b.mag2(f as f64, sr).log10();
        assert!((got - want).abs() <= 0.05, "30 dB at 31.25 Hz, {f} Hz: {got:.3} vs {want:.3}");
    }
}

/// An EXP-stepped sweep of `g6` −15 → +15 dB in 50 ms on a busy curve: the warm-started gains stay
/// within 0.1 dB of the converged ones, no fade is triggered when a gain crosses 0 dB, and the
/// control-rate sidebands of a 1 kHz sine stay low. Then `amount` 0 → 100 % on a zigzag.
#[test]
fn modulation_tracks_without_fades() {
    let sr = 48000.0f64;
    let base = [3.0, -6.0, 8.0, -4.0, 6.0, -15.0, -9.0, 5.0, -3.0, 7.0, -5.0];
    for sweep in ["g6", "amount"] {
        let mut set: Vec<(&str, f32)> = IDS.iter().enumerate().map(|(k, id)| (*id, base[k] as f32)).collect();
        if sweep == "amount" {
            set = IDS.iter().enumerate().map(|(k, id)| (*id, if k % 2 == 0 { 12.0 } else { -12.0 })).collect();
            set.push(("amount", 0.0));
        }
        let mut e = geq(sr as f32, &set);
        let tick = e.tick_len as usize;
        let n = sr as usize / 2;
        let x = sine(n, 1000.0, sr);
        let mut y = x.clone();
        let mut lag = 0.0f64;
        let mut faded = false;
        let (start, len) = (n / 5, (0.05 * sr) as usize);
        let steps = 50;
        let mut pos = 0;
        while pos < n {
            if pos >= start && pos < start + len {
                // one EXP value per millisecond
                let k = (pos - start) * steps / len;
                let v = if sweep == "g6" { -15.0 + 30.0 * (k + 1) as f32 / steps as f32 } else { 100.0 * (k + 1) as f32 / steps as f32 };
                e.set_param(sweep, v);
            }
            let end = (pos + tick).min(n);
            e.process(&mut [&mut y[pos..end]]);
            let s = &e.slots[e.cur];
            // (`amount` starts at 0: the cascade engages with the sweep's first value and fades in)
            if pos >= start + (0.006 * sr) as usize || sweep == "g6" {
                faded |= !s.engaged || s.m < 1.0;
            }
            if pos >= start {
                let plan = &e.plans[0];
                let mut t = [0.0; MAX_POINTS];
                let mut g = [0.0; MAX_SECTIONS];
                let mut d = [UNITY; MAX_SECTIONS];
                plan.targets(&s.faders, &mut t);
                plan.solve(&t, &mut g, &mut d);
                for m in 0..s.n {
                    lag = lag.max((g[m] - s.g[m]).abs());
                }
            }
            pos = end;
        }
        assert!(!faded, "{sweep}: a fade was triggered");
        assert!(lag <= 0.1, "{sweep}: warm-started gains {lag:.3} dB from the converged ones");
        // control-rate sidebands (750 Hz from the carrier), Blackman–Harris window over the sweep
        let w0 = start - len;
        let wl = 4 * len;
        let bin = |f: f64| {
            let w = 2.0 * PI * f / sr;
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for i in 0..wl {
                let a = 2.0 * PI * i as f64 / (wl - 1) as f64;
                let win = 0.35875 - 0.48829 * a.cos() + 0.14128 * (2.0 * a).cos() - 0.01168 * (3.0 * a).cos();
                let v = y[w0 + i] as f64 * win;
                re += v * (w * i as f64).cos();
                im -= v * (w * i as f64).sin();
            }
            10.0 * (re * re + im * im).max(1e-30).log10()
        };
        let carrier = bin(1000.0);
        let side = bin(250.0).max(bin(1750.0)) - carrier;
        println!("{sweep} sweep: lag {lag:.3} dB, control-rate sidebands {side:.1} dB");
        assert!(side <= -80.0, "{sweep}: sidebands {side:.1} dB");
    }
}

/// A layout change under a sine with curves on both layouts: the 20 ms crossfade leaves no click.
#[test]
fn layout_switch_is_click_free() {
    let sr = 48000.0f64;
    let mut e = geq(sr as f32, &[("g5", 6.0), ("g6", -4.0), ("gt_800", 5.0), ("gt_400", -3.0), ("gb_120", 4.0)]);
    let n = sr as usize;
    let x = sine(n, 1000.0, sr);
    let mut y = x.clone();
    let half = n / 2;
    let mut pos = 0;
    for c in y.chunks_mut(256) {
        if pos <= half && half < pos + 256 {
            e.set_param("layout", 1.0);
        }
        if pos <= half + 512 && half + 512 < pos + 256 {
            // while the first crossfade runs: it finishes, then this one starts
            e.set_param("layout", 2.0);
        }
        e.process(&mut [c]);
        pos += 256;
    }
    assert_eq!(e.slots[e.cur].layout, 2);
    // HF burst (10 kHz high-pass) after the switches against the steady part
    let mut a = Svf::new();
    let mut b = Svf::new();
    a.set(gfx_dsp::svf::SvfMode::HighPass, 10000.0, 0.541, 0.0, sr as f32);
    b.set(gfx_dsp::svf::SvfMode::HighPass, 10000.0, 1.307, 0.0, sr as f32);
    let hp: Vec<f32> = y.iter().map(|&v| b.process(a.process(v))).collect();
    let peak = |s: &[f32]| s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let burst = (peak(&hp[half..half + 12000]) - peak(&hp[n / 4..half])).max(1e-9);
    let rel = 20.0 * (burst / peak(&y)).log10();
    println!("layout switch: HF burst {rel:.0} dB re peak");
    assert!(rel <= -60.0, "switch burst {rel:.1} dB");
}

#[test]
fn auto_gain_keeps_pink_loudness() {
    let sr = 48000.0f32;
    let pink = {
        let mut rng = gfx_dsp::lfo::Rng::new(4);
        let mut b = [0.0f64; 7];
        (0..96000)
            .map(|_| {
                let w = rng.bipolar() as f64;
                b[0] = 0.99886 * b[0] + w * 0.0555179;
                b[1] = 0.99332 * b[1] + w * 0.0750759;
                b[2] = 0.96900 * b[2] + w * 0.1538520;
                b[3] = 0.86650 * b[3] + w * 0.3104856;
                b[4] = 0.55000 * b[4] + w * 0.5329522;
                b[5] = -0.7616 * b[5] - w * 0.0168980;
                let y = b.iter().take(6).sum::<f64>() + b[6] + w * 0.5362;
                b[6] = w * 0.115926;
                (0.05 * y) as f32
            })
            .collect::<Vec<f32>>()
    };
    // measured as a loudness meter would: below 40 Hz does not count (K-weighting's high-pass)
    let en = |v: &[f32]| {
        let mut hp = Svf::new();
        hp.set(gfx_dsp::svf::SvfMode::HighPass, 40.0, 0.707, 0.0, sr);
        v.iter().map(|s| hp.process(*s)).skip(48000).map(|s| (s as f64).powi(2)).sum::<f64>()
    };
    let mut rng = Rng::new(11);
    let mut worst = 0.0f64;
    for l in 0..3 {
        for _ in 0..3 {
            let (freqs, first) = layout(l);
            let mut set: Vec<(&str, f32)> = (0..freqs.len()).map(|k| (PARAMS[first + k].id, rng.uniform(12.0) as f32)).collect();
            set.push(("layout", l as f32));
            set.push(("auto_gain", 1.0));
            let mut e = geq(sr, &set);
            let y = render(&mut e, &pink, 480);
            let d = 10.0 * (en(&y) / en(&pink)).log10();
            println!("auto gain, {}: {d:+.2} dB", LAYOUTS[l]);
            worst = worst.max(d.abs());
        }
    }
    println!("auto gain: pink loudness within {worst:.2} dB");
    assert!(worst <= 0.5, "auto gain off by {worst:.2} dB");
}

#[test]
fn cuts_and_level() {
    let sr = 48000.0f32;
    let gain = |set: &[(&str, f32)], f: f32| {
        let mut e = geq(sr, set);
        gfx_dsp::svf::measure_gain_db(
            |x| {
                let mut b = [x];
                e.process(&mut [&mut b]);
                b[0]
            },
            f,
            sr,
        )
    };
    assert!((gain(&[("lowcut", 100.0)], 100.0) + 3.01).abs() < 0.1);
    assert!((gain(&[("highcut", 4000.0), ("highcut_slope", 1.0)], 8000.0) + 24.1).abs() < 1.5);
    assert!((gain(&[("level", 6.0)], 1000.0) - 6.0).abs() < 0.01);
    assert!((gain(&[("gt_800", 4.0), ("layout", 1.0)], 800.0) - 4.0).abs() < 0.1);
}

/// The solver's tick (one correction on Octave 11, the largest plan) and the audio path's CPU, at
/// rest and during a sweep, at 48 and 192 kHz (README §7 Solver cost; printed, the bound asserted
/// loosely: a test machine is not a bench).
#[test]
fn solver_and_audio_cost() {
    use std::time::Instant;
    let plan = Plan::new(&OCTAVE, 48000.0);
    let fad = [3.0, -6.0, 8.0, -4.0, 6.0, 0.0, -9.0, 5.0, -3.0, 7.0, -5.0];
    let mut t = [0.0; MAX_POINTS];
    let mut g = [0.0; MAX_SECTIONS];
    let mut d = [UNITY; MAX_SECTIONS];
    plan.targets(&fad, &mut t);
    plan.solve(&t, &mut g, &mut d);
    let reps = 2000;
    let mut worst = 0.0f64;
    let t0 = Instant::now();
    for k in 0..reps {
        t[k % 39] += 1e-3;
        let s = Instant::now();
        std::hint::black_box(plan.correct(&t, &mut g, &mut d));
        worst = worst.max(s.elapsed().as_secs_f64());
    }
    let mean = t0.elapsed().as_secs_f64() / reps as f64;
    println!("solver tick (Octave 11): mean {:.1} µs, worst {:.1} µs", mean * 1e6, worst * 1e6);
    assert!(mean < 200e-6);
    for &sr in &[48000.0f32, 192000.0] {
        let set: Vec<(&str, f32)> = IDS.iter().enumerate().map(|(k, id)| (*id, fad[k] as f32)).collect();
        let x = noise(sr as usize * 2, 5);
        let mut e = geq(sr, &set);
        let mut y = x.clone();
        let s = Instant::now();
        for c in y.chunks_mut(128) {
            e.process(&mut [c]);
        }
        let rest = s.elapsed().as_secs_f64() * 1e9 / x.len() as f64;
        let mut y = x.clone();
        let s = Instant::now();
        for (k, c) in y.chunks_mut(128).enumerate() {
            // a fader always moving
            e.set_param("g6", (k as f32 * 0.37).sin() * 12.0);
            e.process(&mut [c]);
        }
        let moving = s.elapsed().as_secs_f64() * 1e9 / x.len() as f64;
        println!("audio at {sr} Hz, Octave 11 busy curve, mono: {rest:.0} ns / sample at rest, {moving:.0} ns while a fader moves");
    }
}
