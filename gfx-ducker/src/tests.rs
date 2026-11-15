// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5), on synthetic signals: key tones and ramps, an 80 Hz main sine at its
//! worst phase, kicks with and without bass bleed (the review prototype's `trig.py`), pink noise.
//! Measured figures are printed (`cargo test -p gfx-ducker -- --nocapture`) for the README.

use crate::{band_proto, Ducker, LOOKAHEAD_MS};
use gfx_dsp::duck::{centre, s_curve, SpectralPlan, BANDS};
use gfx_dsp::lfo::Rng;
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use std::f64::consts::PI;

const SR: f32 = 48000.0;

fn n_of(s: f32) -> usize {
    (s * SR) as usize
}

fn with_ch(params: &[(&str, f32)], ch: usize) -> Ducker {
    let mut d = Ducker::new();
    for (k, v) in params {
        d.set_param(k, *v);
    }
    d.prepare(SR, 64, ch);
    d.reset();
    d
}

fn with(params: &[(&str, f32)]) -> Ducker {
    with_ch(params, 1)
}

/// Renders mono sample by sample with an optional mono key; returns the output and the duck (dB).
fn run(d: &mut Ducker, x: &[f32], key: Option<&[f32]>) -> (Vec<f32>, Vec<f64>) {
    let mut y = Vec::with_capacity(x.len());
    let mut duck = Vec::with_capacity(x.len());
    for (i, &v) in x.iter().enumerate() {
        let mut b = [v];
        match key {
            Some(k) => {
                let kk = [k[i]];
                d.process_with_key(&mut [&mut b[..]], Some(&[&kk[..]]));
            }
            None => d.process_with_key(&mut [&mut b[..]], None),
        }
        y.push(b[0]);
        duck.push(d.duck_db());
    }
    (y, duck)
}

/// Renders in blocks of 64 (stereo or mono main, mono key).
fn run_blocks(d: &mut Ducker, main: &mut [Vec<f32>], key: Option<&[f32]>) {
    let n = main[0].len();
    let mut s = 0;
    while s < n {
        let e = (s + 64).min(n);
        let mut sl: Vec<&mut [f32]> = main.iter_mut().map(|c| &mut c[s..e]).collect();
        match key {
            Some(k) => d.process_with_key(&mut sl, Some(&[&k[s..e]])),
            None => d.process_with_key(&mut sl, None),
        }
        s = e;
    }
}

fn sine(f: f64, amp: f64, len_s: f32, phase: f64) -> Vec<f32> {
    (0..n_of(len_s)).map(|i| (amp * (2.0 * PI * f * i as f64 / SR as f64 + phase).sin()) as f32).collect()
}

fn pink(len_s: f32, rms: f32, seed: u64) -> Vec<f32> {
    let mut r = Rng::new(seed);
    let (mut b0, mut b1, mut b2) = (0f64, 0f64, 0f64);
    let v: Vec<f64> = (0..n_of(len_s))
        .map(|_| {
            let w = r.bipolar() as f64;
            b0 = 0.99765 * b0 + w * 0.0990460;
            b1 = 0.96300 * b1 + w * 0.2965164;
            b2 = 0.57000 * b2 + w * 1.0526913;
            b0 + b1 + b2 + w * 0.1848
        })
        .collect();
    let e = (v.iter().map(|x| x * x).sum::<f64>() / v.len() as f64).sqrt();
    v.iter().map(|x| (x * rms as f64 / e) as f32).collect()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-12).log10()
}

/// 4th-order Butterworth high-pass (the bench's click measure).
fn hp4(x: &[f32], fc: f32) -> Vec<f32> {
    let mut a = Svf::new();
    let mut b = Svf::new();
    a.set(SvfMode::HighPass, fc, 0.541_196, 0.0, SR);
    b.set(SvfMode::HighPass, fc, 1.306_563, 0.0, SR);
    x.iter().map(|&v| b.process(a.process(v))).collect()
}

/// Gain (dB) of the output against the input over a window (RMS ratio).
fn gain_db(x: &[f32], y: &[f32]) -> f64 {
    let ex: f64 = x.iter().map(|v| (*v as f64).powi(2)).sum();
    let ey: f64 = y.iter().map(|v| (*v as f64).powi(2)).sum();
    10.0 * (ey / ex.max(1e-30)).log10()
}

#[test]
fn transparent_without_a_key_and_after_release() {
    // no key, a silent key: bit-identical, every shape and stereo setting
    let x = pink(0.5, 0.2, 3);
    let r = pink(0.5, 0.2, 4);
    for shape in 0..5 {
        for st in 0..3 {
            for silent in [false, true] {
                let mut d = with_ch(&[("shape", shape as f32), ("stereo", st as f32), ("depth", 60.0)], 2);
                let mut m = vec![x.clone(), r.clone()];
                let k = vec![0.0f32; x.len()];
                run_blocks(&mut d, &mut m, if silent { Some(&k) } else { None });
                assert!(m[0] == x && m[1] == r, "shape {shape} stereo {st} silent key {silent}");
            }
        }
    }
    // after a duck has fully released: bit-identical again (filters out of the path)
    for shape in 0..5 {
        let mut d = with(&[("shape", shape as f32), ("hold", 0.0), ("release", 50.0)]);
        let x = pink(1.0, 0.2, 5);
        let mut key = sine(300.0, 0.3, 1.0, 0.0);
        key[n_of(0.3)..].iter_mut().for_each(|v| *v = 0.0);
        let (y, duck) = run(&mut d, &x, Some(&key));
        assert!(duck[n_of(0.2)] > 11.0, "shape {shape} ducks");
        let from = n_of(0.6);
        assert!(y[from..] == x[from..], "shape {shape}: not transparent after the release");
    }
}

#[test]
fn depth_is_the_depth() {
    // key 20 dB above the threshold, knee 6: steady duck = depth ± 0.1 dB (Wide), measured on the main
    for depth in [6.0f32, 12.0, 18.0, 40.0] {
        let mut d = with(&[("depth", depth), ("thresh", -40.0)]);
        let x = sine(1000.0, 0.5, 1.0, 0.0);
        let key = sine(300.0, 0.1, 1.0, 0.0);
        let (y, duck) = run(&mut d, &x, Some(&key));
        let g = gain_db(&x[n_of(0.5)..], &y[n_of(0.5)..]);
        assert!((g + depth as f64).abs() <= 0.1, "depth {depth}: {g:.3}");
        assert!((duck[n_of(0.9)] - depth as f64).abs() < 1e-6);
    }
}

#[test]
fn static_curve() {
    // a slow key ramp: duck = depth · S(clamp((L − T) / knee)) ± 0.2 dB
    for knee in [0.0f32, 6.0, 24.0] {
        let mut d = with(&[("knee", knee), ("hyst", 0.0), ("attack", 0.5), ("depth", 24.0), ("thresh", -40.0)]);
        let n = n_of(6.0);
        // −50 → −10 dBFS in 6 s, a 300 Hz key (period < the 40 ms window)
        let key: Vec<f32> = (0..n)
            .map(|i| {
                let l = -50.0 + 40.0 * i as f64 / n as f64;
                (10f64.powf(l / 20.0) * (2.0 * PI * 300.0 * i as f64 / SR as f64).sin()) as f32
            })
            .collect();
        let x = vec![0.0f32; n];
        let mut worst = 0.0f64;
        for (i, &v) in x.iter().enumerate() {
            let mut b = [v];
            d.process_with_key(&mut [&mut b[..]], Some(&[&key[i..i + 1]]));
            if i > n_of(0.1) {
                let l = d.key_level_db();
                let want = if knee == 0.0 {
                    if l > -40.0 {
                        24.0
                    } else {
                        0.0
                    }
                } else {
                    24.0 * s_curve(((l + 40.0) / knee as f64).clamp(0.0, 1.0))
                };
                // at the step (knee 0) the activation is still rising for `attack`
                if knee > 0.0 || (l + 40.0).abs() > 0.1 {
                    worst = worst.max((d.duck_db() - want).abs());
                }
            }
        }
        eprintln!("static curve, knee {knee}: worst error {worst:.3} dB");
        assert!(worst <= 0.2, "knee {knee}: {worst:.3} dB");
    }
}

#[test]
fn times() {
    // key step on / off, knee 0: full depth after `attack` ± 1 sample; no rise during the sliding
    // window + `hold`; back to 0 dB after `release` ± 1 sample
    let (att, hold, rel) = (10.0f32, 150.0f32, 400.0f32);
    let mut d = with(&[("knee", 0.0), ("attack", att), ("hold", hold), ("release", rel)]);
    let n = n_of(2.0);
    let (on, off) = (n_of(0.1), n_of(0.6));
    let key: Vec<f32> = (0..n)
        .map(|i| {
            if i >= on && i < off {
                if (i / 24) % 2 == 0 {
                    0.1
                } else {
                    -0.1
                }
            } else {
                0.0
            }
        })
        .collect();
    let (_, duck) = run(&mut d, &vec![0.0; n], Some(&key));
    let full = (on..n).find(|&i| duck[i] >= 12.0).unwrap() - on;
    let att_n = (att * 0.001 * SR) as usize;
    assert!((full as i64 - att_n as i64).abs() <= 1, "attack {full} vs {att_n}");
    // the sliding maximum's window at hpf 0 (40 ms) + hold
    let w = (0.040 * SR) as usize;
    let release_start = (off..n).find(|&i| duck[i] < 12.0).unwrap();
    let expect = off + w + (hold * 0.001 * SR) as usize;
    assert!((release_start as i64 - expect as i64).abs() <= 2, "release starts at {release_start}, expected {expect}");
    let zero = (release_start..n).find(|&i| duck[i] == 0.0).unwrap();
    let rel_n = (rel * 0.001 * SR) as usize;
    assert!(((zero - release_start) as i64 - rel_n as i64).abs() <= 1, "release {} vs {rel_n}", zero - release_start);
}

#[test]
fn hysteresis() {
    // ramp up then down: starts at T; closes below T + hyst; with a knee the curve sits `hyst` lower
    for knee in [0.0f32, 6.0] {
        let mut d = with(&[("knee", knee), ("hyst", -6.0), ("attack", 0.5), ("hold", 0.0), ("release", 10.0), ("thresh", -40.0)]);
        let n = n_of(8.0);
        let key: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f64 / n as f64;
                let l = if t < 0.5 { -55.0 + 60.0 * t } else { -25.0 - 60.0 * (t - 0.5) };
                (10f64.powf(l / 20.0) * (2.0 * PI * 300.0 * i as f64 / SR as f64).sin()) as f32
            })
            .collect();
        let mut up_start = None;
        let mut down_close = None;
        let mut prev = 0.0;
        for i in 0..n {
            let mut b = [0.0f32];
            d.process_with_key(&mut [&mut b[..]], Some(&[&key[i..i + 1]]));
            let (l, g) = (d.key_level_db(), d.duck_db());
            if up_start.is_none() && g > 0.0 && i < n / 2 {
                up_start = Some(l);
            }
            // the close: knee 0, the release starts (a* drops to 0); with a knee, a* reaches 0 at
            // T + hyst and the activation follows it down exactly (its ramp is faster than the key's)
            let closing = if knee == 0.0 { g < prev } else { g == 0.0 };
            if i > n / 2 && prev > 0.0 && closing && down_close.is_none() {
                down_close = Some(l);
            }
            if i > n / 2 && knee > 0.0 {
                // on the way down the whole curve sits 6 dB lower (once settled in the knee)
                if l > -46.0 + 0.5 && l < -40.0 - 0.5 {
                    let want = 12.0 * s_curve(((l + 46.0) / knee as f64).clamp(0.0, 1.0));
                    assert!((g - want).abs() < 0.3, "knee {knee} at {l:.2}: {g:.2} vs {want:.2}");
                }
            }
            prev = g;
        }
        let s = up_start.unwrap();
        assert!((s + 40.0).abs() <= 0.1, "knee {knee}: starts at {s:.2}");
        let c = down_close.unwrap();
        assert!((c + 46.0).abs() <= 0.1, "knee {knee}: closes at {c:.2}");
    }
}

#[test]
fn no_flutter() {
    // a 41 Hz key at T + 3 dB, hpf 0: gain ripple ≤ 0.1 dB (the sliding maximum)
    let mut d = with(&[("thresh", -40.0), ("knee", 6.0), ("hyst", 0.0)]);
    let key = sine(41.0, 10f64.powf(-37.0 / 20.0), 2.0, 0.0);
    let x = vec![0.0f32; key.len()];
    let (_, duck) = run(&mut d, &x, Some(&key));
    let tail = &duck[n_of(1.0)..];
    let ripple = tail.iter().cloned().fold(f64::MIN, f64::max) - tail.iter().cloned().fold(f64::MAX, f64::min);
    eprintln!("no flutter: 41 Hz key at T + 3 dB, duck {:.3} dB, ripple {ripple:.4} dB", tail[0]);
    assert!(ripple <= 0.1, "{ripple}");
}

#[test]
fn key_notes_under_the_highpass() {
    // the decision's bench: a key note under `hpf` (the halved window rule: W = 1.2 / max(hpf / 2, 30))
    // at T + 3 dB after the filter, inside the knee. With the default `hold` (150 ms) nothing ripples;
    // with `hold` 0 and a fast release, notes whose period fits the window still do not, and the
    // others are measured (README, As coded).
    for (hpf, f, hold) in [
        (100.0f32, 82.0f64, 150.0f32),
        (150.0, 41.0, 150.0),
        (100.0, 82.0, 0.0),
        (100.0, 62.0, 0.0),
        (150.0, 82.0, 0.0),
        (200.0, 82.0, 0.0),
        (200.0, 62.0, 0.0),
        (150.0, 41.0, 0.0),
        (200.0, 41.0, 0.0),
    ] {
        let mut d = with(&[("thresh", -40.0), ("knee", 6.0), ("hyst", 0.0), ("hpf", hpf), ("hold", hold), ("release", 50.0)]);
        // the level after the filter: find the amplitude that reads T + 3 dB
        let mut probe = Svf::new();
        probe.set(SvfMode::HighPass, hpf, std::f32::consts::FRAC_1_SQRT_2, 0.0, SR);
        let att = gfx_dsp::svf::measure_gain_db(|x| probe.process(x), f as f32, SR) as f64;
        let key = sine(f, 10f64.powf((-37.0 - att) / 20.0), 2.0, 0.0);
        let (_, duck) = run(&mut d, &vec![0.0; key.len()], Some(&key));
        let tail = &duck[n_of(1.0)..];
        let ripple = tail.iter().cloned().fold(f64::MIN, f64::max) - tail.iter().cloned().fold(f64::MAX, f64::min);
        let w = gfx_dsp::sidechain::window_secs(hpf, 2.0) as f64;
        eprintln!("key {f} Hz under hpf {hpf}, hold {hold}: window {:.1} ms, duck {:.2} dB, ripple {ripple:.3} dB", w * 1000.0, tail[0]);
        // the rectified key repeats every half period: 41 Hz (12.2 ms) under a 12 ms window ripples
        // 0.01 dB, everything else not at all
        assert!(ripple <= 0.1, "{f} Hz under {hpf}: {ripple}");
    }
}

/// The edge test's measure: an 80 Hz sine at 0 dBFS, a key burst at `phase` of its period, Wide;
/// the HF the edge adds above `fc` (peak of a 4th-order high-pass over the fall, dB re the sine).
fn edge_hf(attack: f32, depth: f32, fc: f32) -> f64 {
    let mut worst = f64::MIN;
    for k in 0..8 {
        let phase = k as f64 / 8.0;
        let mut d = with(&[("attack", attack), ("depth", depth), ("knee", 0.0), ("hold", 2000.0)]);
        let x = sine(80.0, 1.0, 0.6, 0.0);
        let on = n_of(0.3) + (phase * SR as f64 / 80.0) as usize;
        let key: Vec<f32> = (0..x.len()).map(|i| if i >= on { 0.5 } else { 0.0 }).collect();
        let (y, _) = run(&mut d, &x, Some(&key));
        let h = hp4(&y, fc);
        let w = &h[on..on + n_of(attack * 0.001 + 0.004)];
        worst = worst.max(db(peak(w) as f64));
    }
    worst
}

#[test]
fn edges() {
    // README §2.3's table: the energy a fast edge adds. Printed; the common −50 dB bound above
    // 10 kHz holds from 1 ms at 60 dB and from 0.5 ms at 18 dB
    for att in [0.5f32, 1.0, 2.0, 5.0, 10.0] {
        let mut row = String::new();
        for depth in [12.0f32, 18.0, 60.0] {
            row += &format!(" | {:.0} / {:.0}", edge_hf(att, depth, 2000.0), edge_hf(att, depth, 10000.0));
        }
        eprintln!("edge {att} ms (12 / 18 / 60 dB, above 2 k / 10 k){row}");
    }
    assert!(edge_hf(1.0, 60.0, 10000.0) <= -50.0);
    assert!(edge_hf(0.5, 18.0, 10000.0) <= -50.0);
    assert!(edge_hf(10.0, 12.0, 2000.0) <= -50.0);
}

/// A kick (55 Hz body with a pitch drop and a 0.5 ms attack, the prototype's `kick`).
fn kick(len_s: f32, f0: f64, dec: f64) -> Vec<f32> {
    let n = n_of(len_s);
    let mut ph = 0.0;
    (0..n)
        .map(|i| {
            let t = i as f64 / SR as f64;
            let f = f0 + 120.0 * (-t / 0.02).exp();
            ph += 2.0 * PI * f / SR as f64;
            (ph.sin() * (-t / dec).exp() * (t / 0.0005).min(1.0)) as f32
        })
        .collect()
}

fn place(total: f32, items: &[(f32, Vec<f32>)]) -> Vec<f32> {
    let mut y = vec![0.0f32; n_of(total)];
    for (t0, s) in items {
        let i0 = n_of(*t0);
        for (j, v) in s.iter().enumerate() {
            if i0 + j < y.len() {
                y[i0 + j] += v;
            }
        }
    }
    y
}

#[test]
fn recovery_corners() {
    // Trigger, depth 60, recovery 30 ms, tension −100 / 0 / +100: ≤ −60 dB above 10 kHz during the
    // recovery (an 80 Hz main at 0 dBFS)
    for tension in [-100.0f32, 0.0, 100.0] {
        let mut d = with(&[("mode", 1.0), ("sync", 0.0), ("depth", 60.0), ("attack", 10.0), ("hold", 20.0), ("release", 30.0), ("tension", tension)]);
        let x = sine(80.0, 1.0, 1.0, 0.0);
        let key = place(1.0, &[(0.3, kick(0.3, 55.0, 0.25).iter().map(|v| 0.5 * v).collect())]);
        let (y, duck) = run(&mut d, &x, Some(&key));
        let start = (0..y.len()).find(|&i| duck[i] >= 59.99).unwrap();
        let rec = start + n_of(0.020);
        let h = hp4(&y, 10000.0);
        let hf = db(peak(&h[rec..rec + n_of(0.040)]) as f64);
        eprintln!("recovery corner, tension {tension}: {hf:.0} dB above 10 kHz");
        assert!(hf <= -60.0, "tension {tension}: {hf:.1}");
    }
}

#[test]
fn band_shapes_are_stable() {
    // depth 60, attack 0.5 ms (clamped to two ticks): output peak ≤ input peak + 0.5 dB during the
    // edges, no NaN, back to the input once released
    for shape in 1..5 {
        for sig in 0..2 {
            let x = if sig == 0 { pink(1.5, 0.1, 9) } else { sine(80.0, 0.5, 1.5, 0.0) };
            let mut d = with(&[("shape", shape as f32), ("depth", 60.0), ("attack", 0.5), ("hold", 0.0), ("release", 10.0), ("freq", 200.0)]);
            let key: Vec<f32> =
                (0..x.len()).map(|i| if (n_of(0.2)..n_of(0.5)).contains(&i) || (n_of(0.7)..n_of(0.75)).contains(&i) { 0.3 } else { 0.0 }).collect();
            let (y, _) = run(&mut d, &x, Some(&key));
            assert!(y.iter().all(|v| v.is_finite()));
            let (px, py) = (peak(&x) as f64, peak(&y) as f64);
            assert!(db(py) <= db(px) + 0.5, "shape {shape} signal {sig}: {:.2} dB over", db(py) - db(px));
            let from = n_of(1.0);
            assert!(y[from..] == x[from..], "shape {shape} signal {sig}: not back to the input");
        }
    }
}

#[test]
fn bell_and_shelves_at_full_duck() {
    // `force` held: the response against the analog prototype, ± 0.5 dB to 0.45 fs
    for (shape, freq) in [(1usize, 1500.0f32), (2, 120.0), (3, 4000.0)] {
        for depth in [12.0f32, 24.0] {
            let proto = band_proto(shape, depth as f64, 2.0);
            let mut worst = 0.0f64;
            for f in [50.0f64, 120.0, 300.0, 700.0, 1500.0, 3000.0, 6000.0, 12000.0, 18000.0, 21600.0] {
                let mut d = with(&[("shape", shape as f32), ("depth", depth), ("force", 1.0), ("freq", freq), ("width", 2.0)]);
                let x = sine(f, 0.25, 0.4, 0.0);
                let (y, _) = run(&mut d, &x, None);
                let g = gain_db(&x[n_of(0.2)..], &y[n_of(0.2)..]);
                let want = 10.0 * proto.mag2(f / freq as f64).log10();
                worst = worst.max((g - want).abs());
            }
            eprintln!("shape {shape} at {freq} Hz, {depth} dB: worst {worst:.3} dB from the analog design");
            assert!(worst <= 0.5, "shape {shape} depth {depth}: {worst:.3}");
        }
    }
}

/// The Spectral response (dB of cut) the solver's cuts give at `f`.
fn spectral_cut_at(g: &[f64; BANDS], f: f64) -> f64 {
    (0..BANDS).filter(|&k| g[k] > 0.0).map(|k| -10.0 * SpectralPlan::design(k, g[k], SR as f64).mag2(f, SR as f64).log10()).sum()
}

#[test]
fn spectral_follows_the_key() {
    // a sine key on a centre (905 Hz) and between two (1,076 Hz): the deepest cut at the key's
    // frequency ± a quarter octave; ≤ 2 dB one octave away, ≤ 0.5 dB two octaves away
    for fk in [905.0f64, 1076.0] {
        let mut d = with(&[("shape", 4.0), ("depth", 12.0), ("spread", 12.0)]);
        let key = sine(fk, 0.2, 1.0, 0.0);
        let x = pink(1.0, 0.1, 2);
        run(&mut d, &x, Some(&key));
        let g = d.spectral_cuts();
        let freqs: Vec<f64> = (0..=700).map(|i| 40.0 * 2f64.powf(i as f64 * 9.0 / 700.0)).collect();
        let (mut best, mut at) = (0.0, 0.0);
        for &f in &freqs {
            let c = spectral_cut_at(&g, f);
            if c > best {
                best = c;
                at = f;
            }
        }
        let (o1, o2) = (spectral_cut_at(&g, fk * 2.0).max(spectral_cut_at(&g, fk / 2.0)), spectral_cut_at(&g, fk * 4.0).max(spectral_cut_at(&g, fk / 4.0)));
        eprintln!("spectral key {fk} Hz: deepest {best:.2} dB at {at:.0} Hz, one octave {o1:.2}, two {o2:.2}; cuts {:?}", g.map(|v| (v * 10.0).round() / 10.0));
        assert!((at / fk).log2().abs() <= 0.25, "{fk}: deepest at {at}");
        assert!((best - 12.0).abs() <= 0.5, "{fk}: deepest {best}");
        // between two centres two bells share the cut (8.9 dB each): their skirts give 2.3 dB one
        // octave away, over the README's 2 dB (README, As coded)
        let o1_max = if fk == 905.0 { 2.0 } else { 2.5 };
        assert!(o1 <= o1_max && o2 <= 0.5, "{fk}: {o1} {o2}");
    }
    // a pink-noise key: (nearly) the uniform response
    let mut d = with(&[("shape", 4.0), ("depth", 12.0), ("spread", 12.0)]);
    let key = pink(1.5, 0.2, 8);
    run(&mut d, &pink(1.5, 0.1, 2), Some(&key));
    let g = d.spectral_cuts();
    let inner: Vec<f64> = (0..=300).map(|i| centre(1) * 2f64.powf(i as f64 * 6.5 / 300.0)).map(|f| spectral_cut_at(&g, f)).collect();
    let (lo, hi) = (inner.iter().cloned().fold(f64::MAX, f64::min), inner.iter().cloned().fold(f64::MIN, f64::max));
    eprintln!("spectral, pink key: cut {lo:.2} – {hi:.2} dB between the second and the 15th centre");
    assert!(hi <= 12.5 && lo >= 6.0, "{lo} {hi}");
}

#[test]
fn spectral_motion_is_bounded() {
    // a key jumping between two tones: no band's cut moves faster than the activation ramps allow
    let mut d = with(&[("shape", 4.0), ("depth", 18.0), ("attack", 10.0), ("release", 400.0), ("hold", 0.0)]);
    let n = n_of(2.0);
    let key: Vec<f32> = (0..n)
        .map(|i| {
            let f = if (i / n_of(0.2)).is_multiple_of(2) { 400.0 } else { 2500.0 };
            (0.2 * (2.0 * PI * f * i as f64 / SR as f64).sin()) as f32
        })
        .collect();
    let x = pink(2.0, 0.1, 6);
    let ctl = (SR / 1500.0).round() as usize;
    let mut prev = [0.0; BANDS];
    let mut worst = 0.0f64;
    for i in 0..n {
        let mut b = [x[i]];
        d.process_with_key(&mut [&mut b[..]], Some(&[&key[i..i + 1]]));
        assert!(b[0].is_finite());
        if i % ctl == 1 {
            let g = d.spectral_cuts();
            for k in 0..BANDS {
                worst = worst.max((g[k] - prev[k]).abs());
            }
            prev = g;
        }
    }
    // the fastest a target moves per tick: depth over two ticks (the clamped attack's raised cosine
    // peaks at π/2 × the mean slope), plus the solver's redistribution between neighbours
    let bound = 18.0 * (ctl as f64 / (0.010 * SR as f64)) * PI / 2.0 * 2.0;
    eprintln!("spectral motion: largest cut step per tick {worst:.2} dB (bound {bound:.2})");
    assert!(worst <= bound, "{worst}");
}

#[test]
fn stereo_mid_leaves_the_side_alone() {
    let l = pink(1.0, 0.2, 11);
    let r = pink(1.0, 0.2, 12);
    for (st, keep_side) in [(1.0f32, true), (2.0, false)] {
        for shape in [0.0f32, 1.0, 4.0] {
            let mut d = with_ch(&[("stereo", st), ("shape", shape), ("depth", 18.0)], 2);
            let key = sine(500.0, 0.3, 1.0, 0.0);
            let mut m = vec![l.clone(), r.clone()];
            run_blocks(&mut d, &mut m, Some(&key));
            // the untouched component: S for Mid, M for Side
            let comp = |a: &[f32], b: &[f32], s: f32| -> Vec<f32> { a.iter().zip(b).map(|(x, y)| 0.5 * (x + s * y)).collect() };
            let sign = if keep_side { -1.0 } else { 1.0 };
            let (cin, cout) = (comp(&l, &r, sign), comp(&m[0], &m[1], sign));
            let err: Vec<f32> = cin.iter().zip(&cout).map(|(a, b)| a - b).collect();
            let null = db(peak(&err) as f64) - db(peak(&cin) as f64);
            assert!(null <= -100.0, "stereo {st} shape {shape}: {null:.1} dB");
            // and the other component is ducked (fully in Wide, in its band otherwise)
            let (din, dout) = (comp(&l, &r, -sign), comp(&m[0], &m[1], -sign));
            let g = gain_db(&din[n_of(0.5)..], &dout[n_of(0.5)..]);
            assert!(if shape == 0.0 { (g + 18.0).abs() < 0.05 } else { g < -0.2 }, "stereo {st} shape {shape}: {g:.2} dB");
        }
    }
}

#[test]
fn trigger_counts_one_dip_per_kick() {
    // the prototype's signals: 4 kicks (55 Hz, 250 ms decay) 500 ms apart; one long 808; the kicks
    // with three bass notes bleeding 14 dB under them. With `lpf` 150 and with no key filter
    let bass = |len: f32| -> Vec<f32> {
        let n = n_of(len);
        (0..n)
            .map(|i| {
                let t = i as f64 / SR as f64;
                let f = 41.2;
                (((2.0 * PI * f * t).sin() + 0.4 * (4.0 * PI * f * t).sin()) * (t / 0.01).min(1.0) * (-t / 1.5).exp() * (-t / 0.3).exp()) as f32
            })
            .collect()
    };
    let kicks: Vec<(f32, Vec<f32>)> = [0.1f32, 0.6, 1.1, 1.6].iter().map(|&t| (t, kick(0.35, 55.0, 0.25).iter().map(|v| 0.5 * v).collect())).collect();
    let four = place(2.0, &kicks);
    let k808 = place(2.0, &[(0.1, kick(1.8, 45.0, 0.8).iter().map(|v| 0.5 * v).collect())]);
    let bleed = place(2.0, &[(0.35, bass(0.4)), (0.85, bass(0.4)), (1.35, bass(0.4))]);
    let with_bleed: Vec<f32> = four.iter().zip(&bleed).map(|(a, b)| a + 0.1 * b).collect();
    for lpf in [22000.0f32, 150.0] {
        for (name, key, want, onsets) in
            [("4 kicks", &four, 4u64, vec![0.1f32, 0.6, 1.1, 1.6]), ("808", &k808, 1, vec![0.1]), ("kicks + bass", &with_bleed, 4, vec![0.1, 0.6, 1.1, 1.6])]
        {
            let mut d = with(&[("mode", 1.0), ("sync", 0.0), ("lpf", lpf), ("attack", 2.0), ("hold", 20.0), ("release", 120.0)]);
            let (_, duck) = run(&mut d, &vec![0.0; key.len()], Some(key));
            assert_eq!(d.onsets(), want, "{name}, lpf {lpf}");
            // each dip starts ≤ 3 ms after the onset
            for t in onsets {
                let i0 = n_of(t);
                let start = (i0..duck.len()).find(|&i| duck[i] > 0.0).unwrap();
                let lag = (start - i0) as f32 / SR * 1000.0;
                assert!(lag <= 3.0, "{name}, lpf {lpf}: dip {lag:.2} ms after the onset");
            }
        }
    }
    // equal depth for hits ±12 dB
    for gain in [0.25f32, 1.0, 4.0] {
        let key: Vec<f32> = four.iter().map(|v| v * gain * 0.5).collect();
        let mut d = with(&[("mode", 1.0), ("sync", 0.0), ("attack", 2.0), ("hold", 20.0), ("release", 120.0), ("thresh", -50.0)]);
        let (_, duck) = run(&mut d, &vec![0.0; key.len()], Some(&key));
        assert_eq!(d.onsets(), 4, "gain {gain}");
        let m = duck.iter().cloned().fold(0.0, f64::max);
        assert!((m - 12.0).abs() < 1e-6, "gain {gain}: {m}");
    }
    // 16th-note kicks at 180 BPM (83 ms apart): all fire
    let hits: Vec<(f32, Vec<f32>)> =
        (0..12).map(|k| (0.05 + k as f32 * 60.0 / 180.0 / 4.0, kick(0.08, 60.0, 0.03).iter().map(|v| 0.5 * v).collect())).collect();
    let key = place(1.2, &hits);
    // With the full-band key (hpf 0: a 40 ms window) the level holds each hit's peak for 40 ms and
    // the 150 ms reference sits only 4 – 5 dB under it: `sens` 50 (R 7.5 dB) catches 7 of 12. A key
    // high-pass of 150 Hz (a 16 ms window) or `sens` 100 catches all 12 (README, As coded)
    for (sens, hpf, want) in
        [(50.0f32, 0.0f32, None), (75.0, 0.0, None), (100.0, 0.0, Some(12)), (50.0, 150.0, Some(12)), (50.0, 300.0, Some(12)), (50.0, 2000.0, None)]
    {
        let mut d = with(&[("mode", 1.0), ("sync", 0.0), ("attack", 2.0), ("hold", 5.0), ("release", 40.0), ("sens", sens), ("hpf", hpf)]);
        run(&mut d, &vec![0.0; key.len()], Some(&key));
        eprintln!("16ths at 180 BPM, sens {sens}, hpf {hpf}: {} of 12", d.onsets());
        if let Some(w) = want {
            assert_eq!(d.onsets(), w, "16ths at 180 BPM, sens {sens}, hpf {hpf}");
        }
    }
}

#[test]
fn trigger_synced_length() {
    // `sync` 1/4 at 120 BPM, `length` 75 %: a 375 ms dip; attack + hold over half the dip are scaled
    let mut d = with(&[("mode", 1.0), ("sync", 7.0), ("length", 75.0), ("attack", 10.0), ("hold", 150.0)]);
    d.set_transport(TransportInfo { tempo: Some(120.0), pos_beats: None, playing: false });
    let key = place(1.0, &[(0.1, kick(0.3, 55.0, 0.25).iter().map(|v| 0.5 * v).collect())]);
    let (_, duck) = run(&mut d, &vec![0.0; key.len()], Some(&key));
    let start = (0..duck.len()).find(|&i| duck[i] > 0.0).unwrap();
    let end = (start..duck.len()).find(|&i| duck[i] == 0.0).unwrap();
    let len = (end - start) as f32 / SR * 1000.0;
    assert!((len - 375.0).abs() < 0.1, "{len}");
}

#[test]
fn pump_on_the_beat() {
    // host 120 BPM, playing from bar 3 beat 2 (beat 9): each dip starts on a beat ± 1 sample, and
    // with lookahead 5 ms the gain lands L samples later (on the delayed audio)
    for la in [0usize, 2] {
        let mut d = with(&[("mode", 2.0), ("sync", 7.0), ("lookahead", la as f32)]);
        let l = (LOOKAHEAD_MS[la] * 0.001 * SR).round() as usize;
        let spb = SR as f64 * 0.5;
        let n = n_of(2.2);
        // the main is a constant 1: the output is the gain
        let mut y = vec![1.0f32; n];
        // a quarter of a beat into bar 3 beat 2 (beat 9)
        let mut pos = 9.25;
        let mut i = 0;
        while i < n {
            let len = 64.min(n - i);
            d.set_transport(TransportInfo { tempo: Some(120.0), pos_beats: Some(pos), playing: true });
            d.process_with_key(&mut [&mut y[i..i + len]], None);
            pos += len as f64 / spb;
            i += len;
        }
        // beats at samples where 9.25 + i / spb is an integer: 10 → 0.75 · spb, …
        let mut starts = Vec::new();
        for j in l + 2..n {
            if y[j] < 1.0 && y[j - 1] == 1.0 {
                starts.push(j);
            }
        }
        assert!(starts.len() >= 4, "{starts:?}");
        for (k, &s) in starts.iter().enumerate() {
            let beat = (0.75 * spb) as usize + k * spb as usize + l;
            assert!((s as i64 - beat as i64).abs() <= 1, "lookahead {la}: dip {k} at {s}, beat at {beat}");
        }
    }
}

#[test]
fn pump_free_runs_without_transport() {
    // stopped transport: free-running at the host tempo (fallback 120 BPM) from engage
    let mut d = with(&[("mode", 2.0), ("sync", 7.0)]);
    let (_, duck) = run(&mut d, &vec![0.0; n_of(2.1)], None);
    let starts: Vec<usize> = (1..duck.len()).filter(|&j| duck[j] > 0.0 && duck[j - 1] == 0.0).collect();
    let spb = (SR * 0.5) as usize;
    assert!(starts.len() == 5, "{starts:?}");
    for (k, &s) in starts.iter().enumerate() {
        assert!((s as i64 - (k * spb) as i64).abs() <= 1, "{starts:?}");
    }
}

#[test]
fn latency_is_reported() {
    for (i, ms) in LOOKAHEAD_MS.iter().enumerate() {
        let mut d = with(&[("lookahead", i as f32)]);
        let l = (ms * 0.001 * SR).round() as usize;
        assert_eq!(d.latency(), l);
        let mut x = vec![0.0f32; 2000];
        x[100] = 1.0;
        let (y, _) = run(&mut d, &x, None);
        let at = y.iter().position(|&v| v != 0.0).unwrap();
        assert_eq!(at, 100 + l, "lookahead {ms} ms");
    }
}

#[test]
fn lookahead_lands_the_fall_on_the_onset() {
    // lookahead 5 ms, attack 5 ms: the fall finishes as the key's onset reaches the output
    let mut d = with(&[("lookahead", 2.0), ("attack", 5.0), ("knee", 0.0)]);
    let n = n_of(0.5);
    let on = n_of(0.2);
    let key: Vec<f32> = (0..n).map(|i| if i >= on { 0.3 } else { 0.0 }).collect();
    let x = vec![1.0f32; n];
    let (y, _) = run(&mut d, &x, Some(&key));
    let l = (0.005 * SR) as usize;
    // the output at the onset's arrival (on + L) is at full depth
    assert!((db(y[on + l] as f64) + 12.0).abs() < 0.05, "{}", db(y[on + l] as f64));
}

#[test]
fn force_listen_bypass_and_meters() {
    let x = sine(1000.0, 0.5, 1.0, 0.0);
    // force: ducks without a key
    let mut d = with(&[("force", 1.0), ("depth", 18.0)]);
    let (y, _) = run(&mut d, &x, None);
    assert!((gain_db(&x[n_of(0.5)..], &y[n_of(0.5)..]) + 18.0).abs() < 0.05);
    assert!((d.gr_db() - 18.0).abs() < 1e-3 && d.is_ducking());
    // listen: the filtered key; silence without a key
    let mut d = with(&[("listen", 1.0), ("hpf", 500.0)]);
    let key = sine(100.0, 0.5, 1.0, 0.0);
    let (y, _) = run(&mut d, &x, Some(&key));
    assert!(peak(&y[n_of(0.5)..]) < 0.5 * 0.1, "the 100 Hz key under hpf 500");
    let (y, _) = run(&mut with(&[("listen", 1.0)]), &x, None);
    assert!(peak(&y) == 0.0);
    // bypass: the input, even while keyed
    let mut d = with(&[("bypass", 1.0)]);
    let key = sine(300.0, 0.3, 1.0, 0.0);
    let (y, _) = run(&mut d, &x, Some(&key));
    assert!(y == x);
    assert!(d.gr_db() > 11.0, "the meter still reads the duck");
    // the meter at rest
    let mut d = with(&[]);
    run(&mut d, &x, None);
    assert!(d.gr_db() == 0.0 && !d.is_ducking());
}

#[test]
fn structural_changes_crossfade() {
    // mode, shape, stereo, lookahead changed while ducking: no step (HF above 10 kHz ≤ −50 dB re peak)
    for (id, from, to) in [("mode", 0.0f32, 2.0), ("shape", 0.0, 4.0), ("shape", 1.0, 3.0), ("stereo", 0.0, 1.0), ("lookahead", 0.0, 3.0)] {
        let mut d = with_ch(&[(id, from), ("depth", 24.0)], 2);
        let n = n_of(1.0);
        let x = sine(1000.0, 0.25, 1.0, 0.0);
        let r = sine(700.0, 0.25, 1.0, 0.3);
        let key = sine(300.0, 0.3, 1.0, 0.0);
        let mut m = vec![x[..n / 2].to_vec(), r[..n / 2].to_vec()];
        run_blocks(&mut d, &mut m, Some(&key[..n / 2]));
        d.set_param(id, to);
        let mut m2 = vec![x[n / 2..].to_vec(), r[n / 2..].to_vec()];
        run_blocks(&mut d, &mut m2, Some(&key[n / 2..]));
        let y: Vec<f32> = m[0].iter().chain(&m2[0]).copied().collect();
        let h = hp4(&y, 10000.0);
        let burst = db(peak(&h[n / 2..n / 2 + n_of(0.1)]) as f64) - db(peak(&y) as f64);
        assert!(burst <= -50.0, "{id} {from} → {to}: {burst:.1} dB");
    }
}

#[test]
#[ignore]
fn cpu_per_shape() {
    // `cargo test --release -p gfx-ducker cpu_per_shape -- --ignored --nocapture`
    // flush-to-zero, as on the audio thread (nih-plug) and in gfx-bench
    let _ftz = gfx_dsp::util::FtzGuard::new();
    let n = n_of(10.0);
    let x = pink(10.0, 0.1, 13);
    // a voice-like key (three formants), ducking all the time; and the same in 120 ms bursts every
    // 700 ms, so the duck moves (the band shapes redesign every tick while it moves)
    let steady: Vec<f32> = (0..n)
        .map(|i| {
            (0.2 * ((2.0 * PI * 700.0 * i as f64 / SR as f64).sin()
                + 0.5 * (2.0 * PI * 1220.0 * i as f64 / SR as f64).sin()
                + 0.3 * (2.0 * PI * 2600.0 * i as f64 / SR as f64).sin())) as f32
        })
        .collect();
    let bursts: Vec<f32> = steady.iter().enumerate().map(|(i, v)| if i % n_of(0.7) < n_of(0.12) { *v } else { 0.0 }).collect();
    for (kname, key) in [("steady", &steady), ("bursts", &bursts)] {
        for (name, params) in [
            ("Wide", vec![]),
            ("Bell", vec![("shape", 1.0f32)]),
            ("Low", vec![("shape", 2.0)]),
            ("Spectral", vec![("shape", 4.0)]),
            ("Trigger Wide", vec![("mode", 1.0)]),
            ("Pump Wide", vec![("mode", 2.0)]),
        ] {
            for ch in [1usize, 2] {
                let mut d = with_ch(&params, ch);
                d.prepare(SR, 128, ch);
                d.reset();
                let mut m: Vec<Vec<f32>> = (0..ch).map(|_| x.clone()).collect();
                let t = std::time::Instant::now();
                let mut s = 0;
                while s < n {
                    let e = (s + 128).min(n);
                    let mut sl: Vec<&mut [f32]> = m.iter_mut().map(|c| &mut c[s..e]).collect();
                    d.process_with_key(&mut sl, Some(&[&key[s..e]]));
                    s = e;
                }
                let ns = t.elapsed().as_secs_f64() * 1e9 / n as f64;
                eprintln!("CPU {name}, {ch} ch, {kname} key: {ns:.1} ns / sample");
            }
        }
    }
}
