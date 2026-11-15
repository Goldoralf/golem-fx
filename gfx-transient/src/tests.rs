// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5), on synthetic signals: tone bursts, decaying strings (harmonic sums
//! with a pick click, as the review's prototype), noise snares, a drum loop.

use crate::{AttackStage, Transient, LOOKAHEAD_MS};
use gfx_dsp::lfo::Rng;
use gfx_dsp::svf::{measure_gain_db, OnePoleTpt};
use gfx_dsp::Effect;
use std::f32::consts::PI;

const SR: f32 = 48000.0;

fn n_of(s: f32) -> usize {
    (s * SR) as usize
}

fn with_ch(params: &[(&str, f32)], ch: usize) -> Transient {
    let mut t = Transient::new();
    for (k, v) in params {
        t.set_param(k, *v);
    }
    t.prepare(SR, 64, ch);
    t.reset();
    t
}

fn with(params: &[(&str, f32)]) -> Transient {
    with_ch(params, 1)
}

/// Renders sample by sample; returns the output and channel 0's attack and sustain gains (dB).
fn run(t: &mut Transient, x: &[f32]) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut y = Vec::with_capacity(x.len());
    let mut ga = Vec::with_capacity(x.len());
    let mut gs = Vec::with_capacity(x.len());
    for &v in x {
        let mut b = [v];
        t.process(&mut [&mut b[..]]);
        y.push(b[0]);
        let (a, s) = t.gains(0);
        ga.push(a);
        gs.push(s);
    }
    (y, ga, gs)
}

/// Renders a stereo pair in blocks of 64.
fn run2(t: &mut Transient, l: &[f32], r: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let (mut a, mut b) = (l.to_vec(), r.to_vec());
    for (ca, cb) in a.chunks_mut(64).zip(b.chunks_mut(64)) {
        t.process(&mut [ca, cb]);
    }
    (a, b)
}

fn sine(f: f32, amp: f32, len_s: f32) -> Vec<f32> {
    (0..n_of(len_s)).map(|i| amp * (2.0 * PI * f * i as f32 / SR).sin()).collect()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

fn ptp(x: &[f32]) -> f32 {
    x.iter().cloned().fold(f32::MIN, f32::max) - x.iter().cloned().fold(f32::MAX, f32::min)
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-12).log10()
}

/// A decaying string: harmonics of a plucked spectrum, each decaying a little faster than the one
/// under it, plus a 2 ms pick click (the review prototype's `ks_string`).
fn string(f: f32, dur: f32, decay_db_s: f32, amp: f32, pick: f32, asym: f32, seed: u64) -> Vec<f32> {
    let n = n_of(dur);
    let mut y = vec![0.0f32; n];
    for k in 1..40 {
        let kf = k as f32;
        if kf * f > 0.45 * SR {
            break;
        }
        let a = (PI * kf * pick).sin() / kf.powf(1.5);
        let rate = decay_db_s * (1.0 + 0.15 * (kf - 1.0)) / 8.686;
        for (i, v) in y.iter_mut().enumerate() {
            let t = i as f32 / SR;
            *v += a * (-t * rate).exp() * (2.0 * PI * kf * f * t + asym * kf).sin();
        }
    }
    let m = peak(&y);
    let mut rng = Rng::new(seed);
    let mut prev = 0.0f32;
    for (i, v) in y.iter_mut().enumerate() {
        let t = i as f32 / SR;
        let c = rng.bipolar() * 1.7 * (-t / 0.002).exp() * 0.6;
        let click = c - 0.95 * prev;
        prev = c;
        *v = *v / m + 0.5 * click;
    }
    let m = peak(&y);
    y.iter().map(|v| amp * v / m).collect()
}

/// A noise snare with a 190 Hz body (the prototype's `snare`).
fn snare(dur: f32, decay_db_s: f32, amp: f32, seed: u64) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    let y: Vec<f32> = (0..n_of(dur))
        .map(|i| {
            let t = i as f32 / SR;
            rng.bipolar() * 1.7 * (-t * decay_db_s / 8.686).exp() * 0.6 + (2.0 * PI * 190.0 * t).sin() * (-t * decay_db_s * 0.7 / 8.686).exp()
        })
        .collect();
    let m = peak(&y);
    y.iter().map(|v| amp * v / m).collect()
}

fn place(total: f32, items: &[(f32, Vec<f32>)]) -> Vec<f32> {
    let mut y = vec![0.0f32; n_of(total)];
    for (t0, s) in items {
        let i = n_of(*t0);
        for (k, v) in s.iter().enumerate() {
            if i + k < y.len() {
                y[i + k] += v;
            }
        }
    }
    y
}

/// Gaussian white noise of RMS `rms` (Box-Muller).
fn noise(len_s: f32, rms: f32, seed: u64) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    (0..n_of(len_s))
        .map(|_| {
            let u1 = rng.next_f64().max(1e-300);
            let u2 = rng.next_f64();
            rms * ((-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()) as f32
        })
        .collect()
}

/// A 1 kHz burst starting at `t0`, decaying at `decay_db_s`.
fn burst(total: f32, t0: f32, f: f32, amp: f32, decay_db_s: f32) -> Vec<f32> {
    let i0 = n_of(t0);
    (0..n_of(total))
        .map(|i| {
            if i < i0 {
                0.0
            } else {
                let t = (i - i0) as f32 / SR;
                amp * (-t * decay_db_s / 8.686).exp() * (2.0 * PI * f * t).sin()
            }
        })
        .collect()
}

/// 4th-order high-pass at 10 kHz (the bench's click detector).
fn hf(x: &[f32]) -> Vec<f32> {
    use gfx_dsp::svf::{Svf, SvfMode};
    let mut a = Svf::new();
    let mut b = Svf::new();
    a.set(SvfMode::HighPass, 10000.0, 0.541_196, 0.0, SR);
    b.set(SvfMode::HighPass, 10000.0, 1.306_563, 0.0, SR);
    x.iter().map(|&v| b.process(a.process(v))).collect()
}

/// Transparency: `attack` = `sustain` = 0, any band / mix / link / sens, `ceil_mode` Off and Limit
/// (under the ceiling): bit-identical; with `lookahead` 2 ms, bit-identical delayed by L.
#[test]
fn transparent_at_zero() {
    let l = place(2.0, &[(0.1, string(82.4, 1.5, 8.0, 0.5, 0.25, 0.0, 1)), (0.8, snare(0.8, 60.0, 0.6, 2))]);
    let r = place(2.0, &[(0.3, string(110.0, 1.5, 8.0, 0.4, 0.2, 0.3, 3))]);
    for band in [0.0, 1.0, 2.0] {
        for mix in [50.0, 100.0] {
            for (link, sens) in [(0.0, 0.0), (100.0, 100.0), (50.0, 50.0)] {
                for ceil in [0.0, 2.0] {
                    let mut t = with_ch(&[("att_band", band), ("mix", mix), ("link", link), ("sens", sens), ("ceil_mode", ceil)], 2);
                    let (a, b) = run2(&mut t, &l, &r);
                    assert!(a == l && b == r, "band {band} mix {mix} link {link} ceil {ceil}");
                }
            }
        }
    }
    let mut t = with_ch(&[("lookahead", 2.0), ("att_band", 1.0), ("mix", 50.0), ("ceil_mode", 2.0)], 2);
    let la = t.latency();
    assert_eq!(la, (0.002 * SR) as usize);
    let (a, b) = run2(&mut t, &l, &r);
    assert!(a[la..] == l[..l.len() - la] && b[la..] == r[..r.len() - la]);
    assert!(a[..la].iter().all(|&v| v == 0.0));
}

/// Level independence: strings and snares at 0, −12, −24, −36 dB (`thresh` −96): the gain within
/// ±0.1 dB.
#[test]
fn level_independent() {
    let x = place(
        3.0,
        &[
            (0.1, string(82.4, 1.0, 8.0, 0.5, 0.25, 0.0, 1)),
            (0.6, string(110.0, 1.0, 8.0, 0.35, 0.25, 0.0, 2)),
            (1.2, snare(0.5, 200.0, 0.5, 3)),
            (1.5, snare(0.5, 200.0, 0.2, 4)),
            (1.9, string(329.6, 1.0, 8.0, 0.5, 0.25, 0.0, 5)),
        ],
    );
    let p = [("attack", 12.0), ("sustain", 12.0), ("thresh", -96.0)];
    let (_, a0, s0) = run(&mut with(&p), &x);
    for lv in [-12.0f32, -24.0, -36.0] {
        let g = 10f32.powf(lv / 20.0);
        let xs: Vec<f32> = x.iter().map(|v| v * g).collect();
        let (_, a, s) = run(&mut with(&p), &xs);
        let worst = (0..x.len()).map(|i| ((a[i] + s[i]) - (a0[i] + s0[i])).abs()).fold(0.0f32, f32::max);
        println!("level {lv} dB: G within {worst:.3} dB");
        assert!(worst <= 0.1);
    }
}

/// No ripple on steady notes: sines and an asymmetric wave down to 31 Hz, ±24 dB, `sens` 50 and 100:
/// after 300 ms the gain does not move (so no harmonics are added).
#[test]
fn no_ripple_on_steady_notes() {
    let asym = |f: f32| -> Vec<f32> {
        (0..n_of(0.6))
            .map(|i| {
                let w = 2.0 * PI * f * i as f32 / SR;
                0.3 * (w.sin() + 0.5 * (2.0 * w + 0.7).sin() + 0.33 * (3.0 * w + 1.9).sin())
            })
            .collect()
    };
    let mut cases: Vec<(String, Vec<f32>, f32)> = Vec::new();
    for f in [31.0, 41.0, 82.0, 1000.0] {
        cases.push((format!("sine {f}"), sine(f, 0.5, 0.6), 0.0));
    }
    for f in [31.0, 41.0, 82.0] {
        cases.push((format!("asym {f}"), asym(f), 0.0));
    }
    cases.push(("sine 41, sc_hpf 80".into(), sine(41.0, 0.5, 0.6), 80.0));
    cases.push(("sine 82, sc_hpf 160".into(), sine(82.0, 0.5, 0.6), 160.0));
    for (name, x, hpf) in &cases {
        for g in [24.0, -24.0] {
            for sens in [50.0, 100.0] {
                let mut t = with(&[("attack", g), ("sustain", g), ("sens", sens), ("sc_hpf", *hpf)]);
                let (_, a, s) = run(&mut t, x);
                let from = n_of(0.3);
                let gsum: Vec<f32> = (from..x.len()).map(|i| a[i] + s[i]).collect();
                // a constant gain adds no harmonics
                let pp = ptp(&gsum);
                assert!(pp <= 0.05, "{name} {g} sens {sens}: {pp} dB");
            }
        }
    }
}

/// No ripple under the sidechain filter: decaying strings at 41 Hz (`sc_hpf` 80), 62, 73, 82 Hz (120),
/// 82 Hz (160), `attack` ±24, `sens` 100, 0.3 – 0.8 s after the pick: G_att within 0.05 dB.
#[test]
fn no_ripple_under_the_sidechain_filter() {
    for (f, hpf) in [(41.2, 80.0), (61.7, 120.0), (73.4, 120.0), (82.4, 120.0), (82.4, 160.0)] {
        let x = string(f, 1.0, 6.0, 0.5, 0.13, 0.7, 7);
        for g in [24.0, -24.0] {
            let (_, a, _) = run(&mut with(&[("attack", g), ("sens", 100.0), ("sc_hpf", hpf)]), &x);
            let pp = ptp(&a[n_of(0.3)..n_of(0.8)]);
            println!("string {f} Hz, sc_hpf {hpf}, attack {g}: G_att p-p {pp:.4} dB");
            assert!(pp <= 0.05);
        }
    }
}

/// Steady noise, `attack` ±12: |G_att| ≤ 0.3 dB at `sens` 50 and ≤ 1.5 dB at `sens` 100, read as the
/// 99th percentile over four noises. The maximum is an extreme-value statistic: the README's
/// prototype figures (0.10 / 0.56 dB) are its luckiest seed; its other seeds give 0.6 – 1.3 /
/// 3.0 – 5.9 dB (99th percentile 0.04 / 0.22 dB over eight seeds), the bound on the maximum here
/// (1.5 / 6 dB). A ringing six-string chord: 0.
#[test]
fn steady_noise_and_chord() {
    for (sens, bound, max_bound) in [(50.0, 0.3, 1.5), (100.0, 1.5, 6.0)] {
        for g in [12.0, -12.0] {
            let mut all: Vec<f32> = Vec::new();
            for seed in [11, 12, 13, 14] {
                let x = noise(3.0, 0.1, seed);
                let (_, a, _) = run(&mut with(&[("attack", g), ("sens", sens)]), &x);
                all.extend(a[n_of(1.0)..].iter().map(|v| v.abs()));
            }
            all.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let p99 = all[all.len() * 99 / 100];
            let m = *all.last().unwrap();
            println!("white noise, sens {sens}, attack {g}: |G_att| 99 % {p99:.3} dB, max {m:.3} dB");
            assert!(p99 <= bound && m <= max_bound);
        }
    }
    // an E major chord ringing (the prototype's: six sines, equal levels)
    let x: Vec<f32> = (0..n_of(2.0))
        .map(|i| {
            let t = i as f32 / SR;
            0.05 * [82.4f32, 123.5, 164.8, 207.6, 246.9, 329.6].iter().enumerate().map(|(k, f)| (2.0 * PI * f * t + k as f32).sin()).sum::<f32>()
        })
        .collect();
    let (_, a, _) = run(&mut with(&[("attack", 12.0)]), &x);
    let m = a[n_of(0.5)..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    println!("ringing chord: |G_att| max {m:.4} dB");
    assert_eq!(m, 0.0);
}

/// A decaying bass note: 41 Hz, `sustain` ±24, `sens` 100, `sus_time` 300 and 2,000, `sc_hpf` 80: the
/// sustain gain's ripple inside one period ≤ 0.5 dB (drift included).
#[test]
fn decaying_bass_note() {
    let f = 41.2;
    let x = string(f, 1.0, 6.0, 0.5, 0.13, 0.7, 7);
    let per = (SR / f) as usize;
    for sus in [300.0, 2000.0] {
        for g in [24.0, -24.0] {
            let (_, _, s) = run(&mut with(&[("sustain", g), ("sens", 100.0), ("sus_time", sus), ("sc_hpf", 80.0)]), &x);
            let seg = &s[n_of(0.3)..n_of(0.8)];
            let worst = seg.chunks(per).filter(|c| c.len() == per).map(ptp).fold(0.0f32, f32::max);
            println!("41 Hz, sus_time {sus}, sustain {g}: G_sus ripple per period {worst:.3} dB");
            assert!(worst <= 0.5);
        }
    }
}

/// Attack law: a 1 kHz burst from silence, `attack` +12, `smooth` 0.05: ≥ 79 % of the gain within the
/// first 0.1 `att_time`; the §2.3 table (prototype 5.11 / 2.26 / 0.68 dB at 0.5 / 1 / 1.5 `att_time`)
/// within ±0.5 dB; 0 from 2 `att_time`.
#[test]
fn attack_law() {
    let x = burst(0.3, 0.1, 1000.0, 0.5, 0.0);
    let (_, a, _) = run(&mut with(&[("attack", 12.0), ("smooth", 0.05)]), &x);
    let i0 = n_of(0.1);
    let at = |k: f32| a[i0 + (k * 0.015 * SR) as usize];
    let first = a[i0..i0 + n_of(0.0015)].iter().cloned().fold(0.0f32, f32::max);
    println!("attack law: first 0.1 att_time {first:.2} dB; 0.5 / 1 / 1.5 / 2: {:.2} {:.2} {:.2} {:.2}", at(0.5), at(1.0), at(1.5), at(2.0));
    assert!((12.0 * 0.79..=12.0).contains(&first));
    for (k, want) in [(0.5, 5.11), (1.0, 2.26), (1.5, 0.68)] {
        assert!((at(k) - want).abs() <= 0.5, "{k}");
    }
    assert!(a[i0 + (2.05 * 0.015 * SR) as usize..].iter().all(|&v| v == 0.0));
}

/// Sustain law: a 1 kHz burst decaying at 60 dB / s, `sustain` +12, `sus_time` 1,000: G_sus as the
/// prototype (1.29 / 5.26 / 9.22 / 10.83 dB at 100 / 200 / 400 / 600 ms) ± 0.3 dB.
#[test]
fn sustain_law() {
    let x = burst(1.0, 0.1, 1000.0, 0.5, 60.0);
    let (_, _, s) = run(&mut with(&[("sustain", 12.0), ("sus_time", 1000.0)]), &x);
    for (ms, want) in [(100.0, 1.29), (200.0, 5.26), (400.0, 9.22), (600.0, 10.83)] {
        let g = s[n_of(0.1 + ms * 0.001)];
        println!("sustain law at {ms} ms: {g:.2} dB (prototype {want})");
        assert!((g - want).abs() <= 0.3);
    }
}

/// Floor bound: notes with hiss at −70 dBFS in the gaps, `thresh` −66, `sustain` +24: the hiss gets
/// 0 dB; a tail is never lifted above its height over the floor.
#[test]
fn floor_bound() {
    let hiss_peak = 10f32.powf(-70.0 / 20.0);
    let hiss: Vec<f32> = {
        let mut rng = Rng::new(5);
        (0..n_of(4.0)).map(|_| hiss_peak * rng.bipolar()).collect()
    };
    // a note cut after 0.3 s, then a long decaying one
    let mut cut = string(329.6, 0.3, 20.0, 0.5, 0.25, 0.0, 6);
    let len = cut.len();
    for (i, v) in cut.iter_mut().enumerate().skip(len - 240) {
        *v *= (len - i) as f32 / 240.0;
    }
    let decaying = burst(2.5, 0.0, 330.0, 0.5, 40.0);
    let mut x = place(4.0, &[(0.1, cut), (1.5, decaying)]);
    for (v, h) in x.iter_mut().zip(&hiss) {
        *v += h;
    }
    let (_, _, s) = run(&mut with(&[("sustain", 24.0), ("thresh", -66.0)]), &x);
    let gap = s[n_of(0.6)..n_of(1.5)].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    println!("floor bound: gain on the hiss between notes {gap:.4} dB");
    assert!(gap <= 0.1);
    // the decaying note: −6 dBFS falling 40 dB / s, floor at −6 − 48 = −54 dBFS. Its height over the
    // floor bounds the boost: the peak level is the note 40 ms back (W) plus the hiss on its peaks,
    // and E_rf's 10 ms release lags 0.4 dB over it (0.5 dB allowed for both)
    let i0 = n_of(1.5);
    let floor = 0.5 * 10f32.powf(-48.0 / 20.0);
    for i in i0 + n_of(0.05)..x.len() {
        let t = (i - i0) as f32 / SR;
        let level = 0.5 * 10f32.powf(-40.0 * (t - 0.045) / 20.0) + hiss_peak;
        let height = db(level / floor).max(0.0);
        assert!(s[i] <= height + 0.5, "at {t:.2} s: {} dB over a {height:.1} dB height", s[i]);
    }
    let late = s[i0 + n_of(1.4)..].iter().fold(0.0f32, |m, v| m.max(*v));
    println!("floor bound: the tail under the floor gets {late:.4} dB");
    assert!(late <= 0.1);
}

/// Sensitivity: re-picks rising 3 / 6 / 12 / 24 dB over a ringing note get 11 / 30 / 57 / 82 % (± 3)
/// of `attack` at the default; `sens` 100 gives ≥ 85 % at 6 dB; a 1 dB rise gives 0.
#[test]
fn sensitivity() {
    let share = |rise: f32, sens: f32| -> f32 {
        let i0 = n_of(0.3);
        let x: Vec<f32> = sine(1000.0, 0.05, 0.6).iter().enumerate().map(|(i, v)| if i >= i0 { v * 10f32.powf(rise / 20.0) } else { *v }).collect();
        let (_, a, _) = run(&mut with(&[("attack", 12.0), ("sens", sens), ("smooth", 0.05)]), &x);
        a[i0..i0 + 500].iter().cloned().fold(0.0f32, f32::max) / 12.0
    };
    for (rise, want) in [(3.0, 0.11), (6.0, 0.30), (12.0, 0.57), (24.0, 0.82)] {
        let s = share(rise, 50.0);
        println!("re-pick +{rise} dB: {:.1} % of attack", 100.0 * s);
        assert!((s - want).abs() <= 0.03);
    }
    let s = share(6.0, 100.0);
    println!("re-pick +6 dB at sens 100: {:.1} %", 100.0 * s);
    assert!(s >= 0.85);
    assert_eq!(share(1.0, 100.0), 0.0);
}

/// An onset over a shaped tail: the gain never moves by more than 1 dB per sample (`sustain` +12 and
/// −24).
#[test]
fn onset_over_a_shaped_tail() {
    let x = place(1.5, &[(0.1, string(82.4, 1.4, 40.0, 0.5, 0.25, 0.0, 1)), (0.7, string(110.0, 0.8, 40.0, 0.5, 0.25, 0.0, 2))]);
    for g in [12.0, -24.0] {
        let (_, a, s) = run(&mut with(&[("sustain", g), ("attack", 0.0)]), &x);
        let step = (1..x.len()).map(|i| ((a[i] + s[i]) - (a[i - 1] + s[i - 1])).abs()).fold(0.0f32, f32::max);
        println!("onset over a tail shaped {g} dB: largest step {step:.3} dB / sample");
        assert!(step <= 1.0);
    }
}

/// A soft hit after a loud one: the §2.2 table within ±10 points (documents the limit; guards W).
#[test]
fn soft_hit_after_a_loud_one() {
    let table: [(f32, f32, [f32; 4]); 6] = [
        (0.0, -6.0, [0.0, 0.10, 0.42, 0.86]),
        (200.0, -6.0, [0.0, 0.23, 0.57, 0.92]),
        (0.0, -12.0, [0.0, 0.0, 0.10, 0.79]),
        (200.0, -12.0, [0.0, 0.0, 0.37, 0.85]),
        (0.0, -18.0, [0.0, 0.0, 0.0, 0.69]),
        (200.0, -18.0, [0.0, 0.0, 0.11, 0.73]),
    ];
    for (hpf, rel, want) in table {
        let mut got = [0.0f32; 4];
        for (k, dt) in [0.05f32, 0.08, 0.125, 0.25].iter().enumerate() {
            let x = place(1.0, &[(0.1, snare(0.5, 200.0, 0.5, 2)), (0.1 + dt, snare(0.5, 200.0, 0.5 * 10f32.powf(rel / 20.0), 5))]);
            let (_, a, _) = run(&mut with(&[("attack", 12.0), ("att_time", 10.0), ("smooth", 0.05), ("sc_hpf", hpf)]), &x);
            let i1 = n_of(0.1);
            let first = a[i1..i1 + 200].iter().cloned().fold(0.0f32, f32::max);
            let i = n_of(0.1 + dt);
            got[k] = ((a[i..i + n_of(0.02)].iter().cloned().fold(0.0f32, f32::max) - a[i - 1]) / first).max(0.0);
        }
        println!("second hit {rel} dB, sc_hpf {hpf}: {:.0} / {:.0} / {:.0} / {:.0} % (README {:.0} / {:.0} / {:.0} / {:.0})", got[0] * 100.0, got[1] * 100.0, got[2] * 100.0, got[3] * 100.0, want[0] * 100.0, want[1] * 100.0, want[2] * 100.0, want[3] * 100.0);
        for k in 0..4 {
            assert!((got[k] - want[k]).abs() <= 0.10, "{rel} {hpf} {k}");
        }
    }
}

/// The attack stage with a fixed gain, as a filter: its magnitude at `freq` (mix `m`).
fn stage_gain_db(band: usize, g_db: f64, f0: f64, freq: f32, m: f64) -> f32 {
    let mut st = AttackStage::default();
    let ga = 10f64.powf(g_db / 20.0);
    let g0 = OnePoleTpt::coef(f0, SR as f64);
    measure_gain_db(
        |x| {
            let xv = x as f64;
            let (hp, lp) = st.filters(xv, ga, f0, g0, SR as f64);
            let wet = AttackStage::out(band, ga, xv, hp, lp);
            (xv + m * (wet - xv)) as f32
        },
        freq,
        SR,
    )
}

/// Attack band: Highs at 1 kHz, +12 dB: 80 Hz ≤ +0.2 dB, 3 kHz +10.5 ± 0.3 dB; monotonic at ±24 dB,
/// between 0 and the gain, Highs and Lows; with `mix` 50 % still between 0 and the gain (no comb).
#[test]
fn attack_band_shelf() {
    let lo = stage_gain_db(1, 12.0, 1000.0, 80.0, 1.0);
    let hi = stage_gain_db(1, 12.0, 1000.0, 3000.0, 1.0);
    println!("Highs +12 dB at 1 kHz: 80 Hz {lo:+.2} dB, 3 kHz {hi:+.2} dB");
    assert!(lo <= 0.2 && (hi - 10.5).abs() <= 0.3);
    let freqs: Vec<f32> = (0..16).map(|k| 30.0 * 1.5f32.powi(k)).filter(|&f| f < 20000.0).collect();
    for band in [1, 2] {
        for g in [24.0, -24.0] {
            for m in [1.0, 0.5] {
                let r: Vec<f32> = freqs.iter().map(|&f| stage_gain_db(band, g, 1000.0, f, m)).collect();
                let (lo_b, hi_b) = if g > 0.0 { (0.0, g as f32) } else { (g as f32, 0.0) };
                assert!(r.iter().all(|&v| v >= lo_b - 0.05 && v <= hi_b + 0.05), "band {band} {g} mix {m}: {r:?}");
                let up = r.windows(2).all(|w| w[1] >= w[0] - 0.02);
                let down = r.windows(2).all(|w| w[1] <= w[0] + 0.02);
                assert!(up || down, "band {band} {g} mix {m}: {r:?}");
            }
        }
    }
}

/// Lookahead: a 1 kHz burst, `att_time` 15, `smooth` 0.5. `attack` −24: the first cycle ≥ 20 dB down.
/// `attack` +12: no gain until R before the onset, ≥ 11.5 dB at it, ≤ 1.5 dB of motion inside the
/// first cycle; 1, 2 and 5 ms give the same gain. The zero-latency figures are recorded.
#[test]
fn lookahead_gain_arrives_first() {
    let x = burst(0.3, 0.1, 1000.0, 0.5, 0.0);
    let i0 = n_of(0.1);
    let cyc = n_of(0.001);
    let mut shapes: Vec<Vec<f32>> = Vec::new();
    for la in [0usize, 1, 2, 3] {
        let l = (LOOKAHEAD_MS[la] * 0.001 * SR) as usize;
        let mut t = with(&[("attack", -24.0), ("lookahead", la as f32)]);
        assert_eq!(t.latency(), l);
        let (y, _, _) = run(&mut t, &x);
        let cut = db(peak(&y[i0 + l..i0 + l + cyc])) - db(peak(&x[i0..i0 + cyc]));
        let (_, a, _) = run(&mut with(&[("attack", 12.0), ("lookahead", la as f32)]), &x);
        let at = a[i0 + l];
        let motion = ptp(&a[i0 + l..i0 + l + cyc]);
        println!("lookahead {} ms: first cycle {cut:.1} dB at −24; at +12 the gain at the onset {at:.2} dB, motion in the first cycle {motion:.2} dB", LOOKAHEAD_MS[la]);
        if la > 0 {
            assert!(cut <= -20.0);
            assert!(at >= 11.5 && motion <= 1.5);
            let r = (0.0005 * SR) as usize;
            assert!(a[..i0 + l - r].iter().all(|&v| v == 0.0));
            shapes.push(a[i0 + l - r..i0 + l + n_of(0.05)].to_vec());
        }
    }
    for s in &shapes[1..] {
        let d = s.iter().zip(&shapes[0]).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(d <= 1e-4, "{d}");
    }
}

/// A drum loop.
fn drums(len_s: f32) -> Vec<f32> {
    let kick: Vec<f32> = {
        let mut ph = 0.0f32;
        (0..n_of(0.4))
            .map(|i| {
                let t = i as f32 / SR;
                ph += 2.0 * PI * (50.0 + 90.0 * (-t / 0.03).exp()) / SR;
                ph.sin() * (-t * 30.0 / 8.686).exp()
            })
            .collect()
    };
    let mut items = Vec::new();
    let mut t = 0.05;
    let mut k = 0;
    while t < len_s - 0.5 {
        items.push((t, if k % 2 == 0 { kick.clone() } else { snare(0.4, 60.0, 0.8, k as u64) }));
        items.push((t + 0.125, snare(0.2, 200.0, 0.3, 100 + k as u64)));
        t += 0.25;
        k += 1;
    }
    let x = place(len_s, &items);
    let m = peak(&x);
    let g = 10f32.powf(-1.0 / 20.0) / m;
    x.iter().map(|v| v * g).collect()
}

/// Ceiling: a drum loop at −1 dBFS, `attack` +24. Limit at every lookahead and Soft: no sample above
/// `ceiling`; the reported latency is L.
#[test]
fn ceiling_holds() {
    let x = drums(2.0);
    for ceiling in [0.0f32, -6.0] {
        let lim = 10f32.powf(ceiling / 20.0);
        for la in [0.0, 1.0, 2.0, 3.0] {
            let mut t = with(&[("attack", 24.0), ("ceil_mode", 2.0), ("ceiling", ceiling), ("lookahead", la)]);
            assert_eq!(t.latency(), (LOOKAHEAD_MS[la as usize] * 0.001 * SR) as usize);
            let mut y = x.clone();
            for c in y.chunks_mut(64) {
                t.process(&mut [c]);
            }
            let pk = peak(&y);
            println!("Limit, ceiling {ceiling} dB, lookahead {la}: peak {:.2} dBFS, gr {:.1} dB", db(pk), t.gr_db());
            assert!(pk <= lim * (1.0 + 1e-6));
        }
        let mut t = with(&[("attack", 24.0), ("ceil_mode", 1.0), ("ceiling", ceiling)]);
        let mut y = x.clone();
        for c in y.chunks_mut(64) {
            t.process(&mut [c]);
        }
        let pk = peak(&y);
        println!("Soft, ceiling {ceiling} dB: peak {:.2} dBFS", db(pk));
        assert!(pk <= lim * (1.0 + 1e-6));
    }
    // without a ceiling the boosted loop goes far over
    let (y, _, _) = run(&mut with(&[("attack", 24.0)]), &x);
    println!("Off: peak {:.1} dBFS", db(peak(&y)));
    assert!(peak(&y) > 2.0);
}

/// Stereo link: a signal on L only. `link` 100: the same gain on R. `link` 0: R bit-identical.
#[test]
fn stereo_link() {
    let l = place(1.5, &[(0.4, snare(0.5, 60.0, 0.5, 1)), (0.9, snare(0.5, 60.0, 0.5, 2))]);
    // R: a steady note from the start (its own onset is over by 0.3 s)
    let r = sine(220.0, 0.2, 1.5);
    let mut t = with_ch(&[("attack", 12.0), ("sustain", -12.0), ("link", 0.0)], 2);
    let (_, b) = run2(&mut t, &l, &r);
    assert!(b[n_of(0.3)..] == r[n_of(0.3)..]);
    let mut t = with_ch(&[("attack", 12.0), ("sustain", -12.0), ("link", 100.0)], 2);
    let (mut a, mut b) = (l.clone(), r.clone());
    for i in 0..l.len() {
        t.process(&mut [&mut a[i..i + 1], &mut b[i..i + 1]]);
        assert_eq!(t.gains(0), t.gains(1));
    }
    assert!(b[n_of(0.4)..n_of(0.45)] != r[n_of(0.4)..n_of(0.45)]);
}

/// Key: `source` Key with nothing connected works as Main; a key drives the shaping of the main.
#[test]
fn key_input() {
    let x = place(1.0, &[(0.1, snare(0.5, 60.0, 0.5, 1))]);
    let p = [("attack", 12.0), ("sustain", -6.0)];
    let (y_main, _, _) = run(&mut with(&p), &x);
    let mut t = with(&[p[0], p[1], ("source", 1.0)]);
    let mut y = x.clone();
    for c in y.chunks_mut(64) {
        t.process_with_key(&mut [c], None);
    }
    assert!(y == y_main);
    // a steady main keyed by a snare: the main is shaped at the key's hits
    let main = sine(220.0, 0.2, 1.0);
    let mut t = with(&[p[0], p[1], ("source", 1.0)]);
    let mut y = main.clone();
    for (k, c) in y.chunks_mut(64).enumerate() {
        let key = &x[k * 64..k * 64 + c.len()];
        t.process_with_key(&mut [c], Some(&[key]));
    }
    let boost = db(peak(&y[n_of(0.1)..n_of(0.11)])) - db(0.2);
    println!("keyed boost on a steady main: {boost:.1} dB");
    assert!(boost > 6.0);
}

/// Clicks: `att_band` switch mid-note, `attack` swept by EXP through 0, `sc_hpf` swept 0 → 300 Hz on a
/// held chord: the HF burst stays under −50 dB re the output peak.
#[test]
fn no_clicks() {
    let chord: Vec<(f32, Vec<f32>)> = [82.4, 110.0, 146.8, 196.0].iter().enumerate().map(|(k, &f)| (0.0, string(f, 3.0, 4.0, 0.2, 0.25, 0.3 * k as f32, 30 + k as u64))).collect();
    let x = place(3.0, &chord);
    let burst_db = |y: &[f32], from: f32, to: f32| -> f32 {
        let h = hf(y);
        db(peak(&h[n_of(from)..n_of(to)]) / peak(y))
    };
    let render = |base: &[(&str, f32)], events: &dyn Fn(usize) -> Vec<(&'static str, f32)>| -> Vec<f32> {
        let mut t = with(base);
        let mut y = x.clone();
        for (k, c) in y.chunks_mut(64).enumerate() {
            for (id, v) in events(k * 64) {
                t.set_param(id, v);
            }
            t.process(&mut [c]);
        }
        y
    };
    // the reference: the same chord unprocessed (its own pick clicks)
    let base = burst_db(&x, 1.0, 2.5);
    // band switches while the attack gain is held high by re-picks: a re-picked chord
    let y = render(&[("attack", 12.0), ("att_time", 100.0), ("sens", 100.0)], &|i| {
        if i == n_of(1.2) {
            vec![("att_band", 1.0)]
        } else if i == n_of(1.6) {
            vec![("att_band", 2.0)]
        } else if i == n_of(2.0) {
            vec![("att_band", 0.0)]
        } else {
            vec![]
        }
    });
    let b = burst_db(&y, 1.0, 2.5);
    println!("att_band switches: HF {b:.1} dB (input {base:.1} dB)");
    assert!(b <= -50.0);
    // `attack` swept through 0 by EXP steps (every 5 ms, −24 → +24 over 1 s)
    let y = render(&[("attack", -24.0), ("sustain", 6.0)], &|i| {
        if i >= n_of(1.0) && i < n_of(2.0) && i % 256 == 0 {
            vec![("attack", -24.0 + 48.0 * (i - n_of(1.0)) as f32 / SR)]
        } else {
            vec![]
        }
    });
    let b = burst_db(&y, 1.0, 2.5);
    println!("attack sweep through 0: HF {b:.1} dB");
    assert!(b <= -50.0);
    // `sc_hpf` swept 0 → 300 Hz
    let y = render(&[("attack", 12.0), ("sustain", 12.0)], &|i| {
        if i >= n_of(1.0) && i < n_of(2.0) && i % 256 == 0 {
            vec![("sc_hpf", 300.0 * (i - n_of(1.0)) as f32 / SR)]
        } else {
            vec![]
        }
    });
    let b = burst_db(&y, 1.0, 2.5);
    println!("sc_hpf sweep: HF {b:.1} dB");
    assert!(b <= -50.0);
}

/// Latency: 0 at default; a lookahead change is reported and crossfades without a click.
#[test]
fn lookahead_change_crossfades() {
    let t = with(&[]);
    assert_eq!(t.latency(), 0);
    let x = sine(220.0, 0.5, 1.0);
    let mut t = with(&[("attack", 6.0)]);
    let mut y = x.clone();
    for (k, c) in y.chunks_mut(64).enumerate() {
        if k * 64 == n_of(0.5) {
            t.set_param("lookahead", 3.0);
            assert_eq!(t.latency(), n_of(0.005));
        }
        t.process(&mut [c]);
    }
    let b = db(peak(&hf(&y)[n_of(0.4)..]) / peak(&y));
    println!("lookahead change: HF {b:.1} dB");
    assert!(b <= -50.0);
}

/// `listen` outputs the filtered sidechain; `delta` what the shaper adds or removes; `bypass` the
/// (delayed) dry.
#[test]
fn listen_delta_bypass() {
    let x = place(1.0, &[(0.1, snare(0.5, 60.0, 0.5, 1))]);
    // delta with nothing shaped: silence
    let (y, _, _) = run(&mut with(&[("delta", 1.0)]), &x);
    assert!(y.iter().all(|&v| v == 0.0));
    // delta + dry = shaped
    let p = [("attack", 9.0), ("sustain", -6.0)];
    let (shaped, _, _) = run(&mut with(&p), &x);
    let (d, _, _) = run(&mut with(&[p[0], p[1], ("delta", 1.0)]), &x);
    let err = (0..x.len()).map(|i| (d[i] + x[i] - shaped[i]).abs()).fold(0.0f32, f32::max);
    assert!(err < 1e-6, "{err}");
    // listen: the sidechain high-passed at 1 kHz has lost the snare's 190 Hz body
    let (l, _, _) = run(&mut with(&[p[0], ("listen", 1.0), ("sc_hpf", 1000.0)]), &x);
    let mut f = gfx_dsp::sidechain::ScFilter::default();
    f.set(1000.0, 22000.0, SR);
    let want: Vec<f32> = x.iter().map(|&v| f.process(v)).collect();
    assert!(l == want);
    // bypass with lookahead: the input delayed by L
    let mut t = with(&[p[0], p[1], ("lookahead", 1.0), ("bypass", 1.0)]);
    let la = t.latency();
    let (b, _, _) = run(&mut t, &x);
    assert!(b[la..] == x[..x.len() - la]);
}

/// Meters: `shaping` follows the gain, `gr` the ceiling.
#[test]
fn meters() {
    let x = burst(0.3, 0.1, 1000.0, 0.9, 0.0);
    let mut t = with(&[("attack", 12.0), ("ceil_mode", 2.0)]);
    let mut y = x.clone();
    let (mut sh, mut gr) = (0.0f32, 0.0f32);
    for c in y.chunks_mut(64) {
        t.process(&mut [c]);
        sh = sh.max(t.shaping_db());
        gr = gr.max(t.gr_db());
    }
    println!("meters: shaping {sh:.1} dB, gr {gr:.1} dB");
    assert!(sh > 8.0 && sh <= 12.0 && gr > 3.0);
}

/// Determinism and robustness: two renders are identical; NaN / inf / huge input never makes NaN.
#[test]
fn deterministic_and_finite() {
    let x = drums(1.0);
    let p = [("attack", 12.0), ("sustain", 12.0), ("att_band", 1.0), ("ceil_mode", 1.0), ("lookahead", 2.0), ("mix", 70.0)];
    let (a, _, _) = run(&mut with(&p), &x);
    let (b, _, _) = run(&mut with(&p), &x);
    assert!(a == b);
    let mut z = x.clone();
    z[1000] = f32::NAN;
    z[2000] = f32::INFINITY;
    z[3000] = 1e30;
    z[3001] = -1e30;
    for band in [0.0, 1.0, 2.0] {
        let (y, _, _) = run(&mut with(&[("attack", 24.0), ("sustain", 24.0), ("att_band", band), ("att_freq", 8000.0)]), &z);
        assert!(y.iter().all(|v| v.is_finite()));
    }
}
