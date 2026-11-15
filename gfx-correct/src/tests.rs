// SPDX-License-Identifier: GPL-3.0-or-later
//! The README's bench rows that run on synthetic signals (§5): a Rosenberg-pulse /a/ (formants 700,
//! 1,220, 2,600 Hz) with a pitch contour, cycle jitter, vibrato, noise.

use super::*;
use gfx_dsp::svf::{Svf, SvfMode};

const SR: f32 = 48000.0;

/// A sung /a/ whose pitch (cents from A4) follows `contour(t)`, with cycle jitter (fraction).
fn voice(n: usize, sr: f32, jitter: f64, contour: impl Fn(f64) -> f64) -> Vec<f32> {
    let mut fs = [Svf::new(), Svf::new(), Svf::new()];
    for (s, (f, q)) in fs.iter_mut().zip([(700.0, 8.0), (1220.0, 10.0), (2600.0, 12.0)]) {
        s.set(SvfMode::BandPass, f, q, 0.0, sr);
    }
    let mut ph = 0.0f64;
    let mut seed = 0x1234_5678u32;
    let mut jit = 1.0;
    (0..n)
        .map(|i| {
            let t = i as f64 / sr as f64;
            let f = 440.0 * (contour(t) / 1200.0).exp2() * jit;
            ph += f / sr as f64;
            if ph >= 1.0 {
                ph -= 1.0;
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                jit = 1.0 + jitter * ((seed as f64 / u32::MAX as f64) * 2.0 - 1.0) * 1.732;
            }
            let g = if ph < 0.6 { (std::f64::consts::PI * ph / 0.6).sin().powi(2) } else { 0.0 };
            let x = g as f32;
            0.5 * (fs[0].process(x) + 0.6 * fs[1].process(x) + 0.3 * fs[2].process(x))
        })
        .collect()
}

fn noise(n: usize, amp: f32, seed: u32) -> Vec<f32> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            amp * ((s as f32 / u32::MAX as f32) * 2.0 - 1.0)
        })
        .collect()
}

fn make(settings: &[(&str, f32)], sr: f32, ch: usize) -> Correct {
    let mut c = Correct::new();
    for (k, v) in settings {
        c.set_param(k, *v);
    }
    c.prepare(sr, 512, ch);
    c
}

fn run(c: &mut Correct, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for blk in y.chunks_mut(256) {
        c.process(&mut [blk]);
    }
    y
}

/// The output's pitch per cycle, (time s, cents from A4), by the epoch tracker itself.
fn pitch_of(y: &[f32], sr: f32) -> Vec<(f64, f64)> {
    let mut tr = Tracker::new();
    tr.prepare(sr, 0.4);
    let mut out = Vec::new();
    let mut last_m: Option<f64> = None;
    for &v in y {
        let ev = tr.push(v);
        if ev.mark {
            let m = tr.marks();
            let mk = m[m.len() - 1];
            if let Some(l) = last_m {
                if m.len() >= 2 && m[m.len() - 2] == l {
                    let per = mk - l;
                    out.push((mk / sr as f64, 1200.0 * (sr as f64 / per / 440.0).log2()));
                }
            }
            last_m = Some(mk);
        } else if !tr.active() {
            last_m = None;
        }
    }
    out
}

/// Mean and (max − min) / 2 of the pitch between t0 and t1 (s), averaged over `avg` cycles.
fn stats(p: &[(f64, f64)], t0: f64, t1: f64) -> (f64, f64) {
    let v: Vec<f64> = p.iter().filter(|(t, _)| *t >= t0 && *t <= t1).map(|x| x.1).collect();
    assert!(v.len() > 4, "no pitch between {t0} and {t1}");
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    // smooth over 3 cycles against the jitter
    let sm: Vec<f64> = v.windows(3).map(|w| (w[0] + w[1] + w[2]) / 3.0).collect();
    let (lo, hi) = sm.iter().fold((f64::MAX, f64::MIN), |(a, b), &x| (a.min(x), b.max(x)));
    (mean, 0.5 * (hi - lo))
}

#[test]
fn latency_by_mode_and_register() {
    // Live T_max / 2 + 1 ms, Studio 2 · T_max + 4 ms (README §2.7): 9.3 / 6.6 / 4.3 and 37.3 / 26.2 / 17.3 ms
    let want = [(true, [9.3, 6.6, 4.3]), (false, [37.3, 26.2, 17.3])];
    for (live, ms) in want {
        for reg in 0..3 {
            let c = make(&[("fastmode", if live { 1.0 } else { 0.0 }), ("register", reg as f32)], SR, 1);
            let l = c.latency() as f64 / 48.0;
            assert!((l - ms[reg]).abs() < 0.05, "live {live} register {reg}: {l:.2} ms");
        }
    }
}

#[test]
fn identity_without_correction() {
    // corr 0, offset 0: the input delayed by the latency, voiced and unvoiced, both modes, 1 and 2 ch
    let mut x = voice(48000, SR, 0.007, |t| -1200.0 + 35.0 + 30.0 * (6.0 * std::f64::consts::TAU * t).sin());
    x.extend(noise(12000, 0.1, 9));
    x.extend(voice(24000, SR, 0.007, |_| -900.0));
    for live in [1.0, 0.0] {
        for reg in [0.0, 1.0, 2.0] {
            let mut c = make(&[("corr", 0.0), ("fastmode", live), ("register", reg)], SR, 1);
            let y = run(&mut c, &x);
            let l = c.latency();
            let (mut err, mut at) = (0.0f32, 0);
            for i in l..x.len() {
                let e = (y[i] - x[i - l]).abs();
                if e > err {
                    err = e;
                    at = i - l;
                }
            }
            assert!(err < 1e-5, "live {live} register {reg}: max error {err} at {at}");
        }
    }
}

#[test]
fn snap_to_the_note() {
    // a steady vowel 35 ¢ sharp of A3, filter 0, transition 0: on A3 within ±3 ¢ (both modes)
    let x = voice(72000, SR, 0.0, |_| -1200.0 + 35.0);
    for live in [1.0, 0.0] {
        let mut c = make(&[("filter", 0.0), ("fastmode", live)], SR, 1);
        let y = run(&mut c, &x);
        let p = pitch_of(&y, SR);
        let (m, _) = stats(&p, 0.4, 1.4);
        assert!((m + 1200.0).abs() < 3.0, "live {live}: {m:.2} ¢");
        assert_eq!(c.note(), 57.0);
    }
}

#[test]
fn corr_is_a_fraction_and_offset_transposes() {
    let x = voice(72000, SR, 0.0, |_| -1200.0 + 40.0);
    for live in [1.0, 0.0] {
        let mut c = make(&[("filter", 0.0), ("corr", 0.5), ("fastmode", live)], SR, 1);
        let (m, _) = stats(&pitch_of(&run(&mut c, &x), SR), 0.4, 1.4);
        let removed = (40.0 - (m + 1200.0)) / 40.0;
        assert!((removed - 0.5).abs() < 0.03, "live {live}: removed {removed:.3}");
        // corr 0, offset +1 st: a plain transposition
        let mut c = make(&[("corr", 0.0), ("offset", 1.0), ("fastmode", live)], SR, 1);
        let (m, _) = stats(&pitch_of(&run(&mut c, &x), SR), 0.4, 1.4);
        assert!((m - (-1200.0 + 40.0 + 100.0)).abs() < 3.0, "live {live}: offset {m:.2}");
    }
}

#[test]
fn filter_is_the_held_note_time_constant() {
    // a held note, then a 30 ¢ drift step within the note: the correction follows with τ = filter
    let step_t = 0.8;
    let x = voice(96000, SR, 0.0, |t| -1200.0 + if t < step_t { 10.0 } else { 40.0 });
    let tau = 0.1;
    let mut c = make(&[("filter", tau as f32), ("fastmode", 0.0), ("hysteresis", 50.0)], SR, 1);
    let p = pitch_of(&run(&mut c, &x), SR);
    // output error vs A3 after the step: 40 · exp(−t / τ) (the step itself enters at 30 ¢)
    let lat = c.latency() as f64 / SR as f64;
    let at = |dt: f64| {
        let t = step_t + lat + dt;
        let v: Vec<f64> = p.iter().filter(|(u, _)| (*u - t).abs() < 0.006).map(|x| x.1 + 1200.0).collect();
        v.iter().sum::<f64>() / v.len().max(1) as f64
    };
    let e0 = at(0.02);
    let e1 = at(0.02 + tau);
    // after τ the remaining error is e^−1 of the step's part (≥ 10 ¢ steady error corrected as well)
    let ratio = e1 / e0;
    assert!((ratio - (-1.0f64).exp()).abs() < 0.1, "e0 {e0:.1} e1 {e1:.1} ratio {ratio:.3}");
}

#[test]
fn scale_mask_and_hysteresis() {
    let a = allowed_notes([true; 12], 0, 1);
    assert_eq!(a, [true, false, true, false, true, true, false, true, false, true, false, true]);
    // mask ∩ scale: both golem paths give the same notes
    let mut mask = [false; 12];
    for (i, m) in mask.iter_mut().enumerate() {
        *m = a[i];
    }
    assert_eq!(allowed_notes(mask, 0, 0), a);
    assert_eq!(allowed_notes(mask, 0, 1), a);
    // nothing allowed: no correction
    let x = voice(48000, SR, 0.0, |_| -1200.0 + 30.0);
    let mut s: Vec<(&str, f32)> = (0..12).map(|i| (PARAMS[p::M00 + i].id, 0.0)).collect();
    s.push(("filter", 0.0));
    let mut c = make(&s, SR, 1);
    let y = run(&mut c, &x);
    let l = c.latency();
    let err = (l..x.len()).map(|i| (y[i] - x[i - l]).abs()).fold(0.0f32, f32::max);
    assert!(err < 1e-5);
    // hysteresis: from A3, a held note switches at 50 + hysteresis ¢
    let mut c = Correct::new();
    c.set_param("hysteresis", 23.0);
    c.prepare(SR, 512, 1);
    c.held = Some(-12);
    assert_eq!(c.quantise(-1200.0 + 72.0), Some(-12));
    assert_eq!(c.quantise(-1200.0 + 74.0), Some(-11));
    // in C major from C, C# is skipped
    c.set_param("scale", 1.0);
    c.held = None;
    assert_eq!(c.quantise(-900.0 + 100.0 + 10.0), Some(-7));
}

#[test]
fn flex_leaves_far_pitches() {
    // 15 ¢ off A3 in Chromatic; 70 ¢ above A3 in C major (A♯ is not in it: A stays the target)
    for (off, scale, want_removed_min, want_removed_max) in [(15.0, 0.0, 0.9, 1.01), (70.0, 1.0, -0.01, 0.1)] {
        let x = voice(72000, SR, 0.0, move |_| -1200.0 + off);
        let mut c = make(&[("filter", 0.0), ("flex", 100.0), ("fastmode", 0.0), ("scale", scale)], SR, 1);
        let (m, _) = stats(&pitch_of(&run(&mut c, &x), SR), 0.4, 1.4);
        let removed = (off - (m + 1200.0)) / off;
        assert!(removed >= want_removed_min && removed <= want_removed_max, "{off} ¢: removed {removed:.3}");
    }
}

#[test]
fn vibrato_kept_and_removed() {
    // ±50 ¢ at 6 Hz on a note 20 ¢ off A3 (220 Hz)
    let vib = |t: f64| -1200.0 + 20.0 + 50.0 * (std::f64::consts::TAU * 6.0 * t).sin() * (t / 0.2).min(1.0);
    let x = voice(144000, SR, 0.0, vib);
    for (live, keep) in [(1.0, 100.0), (0.0, 100.0), (1.0, 0.0), (0.0, 0.0)] {
        let mut c = make(&[("filter", 0.0), ("fastmode", live), ("vibrato", keep)], SR, 1);
        let (m, depth) = stats(&pitch_of(&run(&mut c, &x), SR), 1.0, 2.9);
        eprintln!("vibrato {keep} % live {live}: centre {:+.2} ¢, depth {:.1} % of the input's", m + 1200.0, depth / 50.0 * 100.0);
        if keep > 0.0 {
            assert!((m + 1200.0).abs() < 3.0, "live {live} kept: centre {m:.1}");
            assert!((depth / 50.0 - 1.0).abs() < 0.1, "live {live} kept: depth {depth:.1}");
        } else {
            let bound = if live > 0.5 { 0.15 } else { 0.05 };
            assert!(depth / 50.0 <= bound, "live {live} removed: residual depth {:.3}", depth / 50.0);
        }
    }
}

#[test]
fn live_recentres_after_the_voice() {
    // a corrected note, then noise: 20 ms after the voicing closes the output is the input delayed by C
    let mut x = voice(48000, SR, 0.0, |_| -1200.0 + 45.0);
    let n0 = x.len();
    x.extend(noise(24000, 0.05, 3));
    let mut c = make(&[("filter", 0.0)], SR, 1);
    let y = run(&mut c, &x);
    let l = c.latency();
    let start = n0 + (0.12 * SR) as usize;
    let exact_from = (n0..x.len()).rev().find(|&i| (y[i] - x[i - l]).abs() > 1e-5).map_or(n0, |i| i + 1);
    eprintln!("Live re-centred: the delayed input exactly from {:.1} ms after the voice ends", (exact_from - n0) as f32 / SR * 1000.0);
    let err = (start..x.len()).map(|i| (y[i] - x[i - l]).abs()).fold(0.0f32, f32::max);
    assert!(err < 1e-5, "after the voice: {err}");
    assert_eq!(c.head_offset(), 0.0);
}

#[test]
fn bleed_under_threshold_is_not_corrected() {
    let x: Vec<f32> = voice(48000, SR, 0.0, |_| -1200.0 + 40.0).iter().map(|v| v * 0.02).collect();
    let mut c = make(&[("thresh", -30.0), ("filter", 0.0)], SR, 1);
    let y = run(&mut c, &x);
    let l = c.latency();
    let err = (l..x.len()).map(|i| (y[i] - x[i - l]).abs()).fold(0.0f32, f32::max);
    assert!(err < 1e-7, "{err}");
    assert_eq!(c.state(), 2);
}

#[test]
fn studio_corrects_from_the_first_cycle() {
    // a note sung 40 ¢ flat after silence: its first cycles already on the target (filter 0, transition 0)
    for f_cents in [-1200.0 - 40.0, -2600.0 - 40.0 + 100.0] {
        let mut x = vec![0.0f32; 9600];
        x.extend(voice(28800, SR, 0.0, move |_| f_cents));
        let mut c = make(&[("filter", 0.0), ("fastmode", 0.0)], SR, 1);
        let y = run(&mut c, &x);
        let l = c.latency();
        let p = pitch_of(&y[l..], SR);
        // the tracker on the output needs a few cycles itself: its first readings
        let first: Vec<f64> = p.iter().take(3).map(|x| x.1).collect();
        let target = (f_cents / 100.0).round() * 100.0;
        eprintln!("Studio onset {f_cents} ¢: first cycles {:?} ¢ off the target", first.iter().map(|v| ((v - target) * 10.0).round() / 10.0).collect::<Vec<_>>());
        for v in &first {
            assert!((v - target).abs() < 5.0, "{f_cents}: first cycles {first:?}");
        }
    }
}

#[test]
fn stereo_equal_channels_and_determinism() {
    let x = voice(48000, SR, 0.007, |t| -1200.0 + 30.0 * (std::f64::consts::TAU * 5.0 * t).sin());
    for live in [1.0, 0.0] {
        let mut c = make(&[("fastmode", live), ("filter", 0.02)], SR, 2);
        let (mut l, mut r) = (x.clone(), x.clone());
        for (a, b) in l.chunks_mut(300).zip(r.chunks_mut(300)) {
            c.process(&mut [a, b]);
        }
        assert_eq!(l, r);
        let mut m = make(&[("fastmode", live), ("filter", 0.02)], SR, 1);
        let y = run(&mut m, &x);
        assert_eq!(l, y);
        assert!(y.iter().all(|v| v.is_finite()));
    }
}

/// Cycle-to-cycle jitter (rms of successive differences, cents) between t0 and t1.
fn jitter_of(p: &[(f64, f64)], t0: f64, t1: f64) -> f64 {
    let v: Vec<f64> = p.iter().filter(|(t, _)| *t >= t0 && *t <= t1).map(|x| x.1).collect();
    let d: Vec<f64> = v.windows(2).map(|w| w[1] - w[0]).collect();
    (d.iter().map(|x| x * x).sum::<f64>() / d.len() as f64).sqrt()
}

#[test]
fn jitter_gain() {
    // a steady vowel with 0.7 % cycle jitter, 20 ¢ off: the output's jitter against the input's
    let x = voice(96000, SR, 0.007, |_| -1200.0 + 20.0);
    let j_in = jitter_of(&pitch_of(&x, SR), 0.3, 1.9);
    for (live, filter, bound) in [(1.0, 0.0, 1.25), (1.0, 0.02, 1.05), (0.0, 0.0, 1.05)] {
        let mut c = make(&[("filter", filter), ("fastmode", live)], SR, 1);
        let j = jitter_of(&pitch_of(&run(&mut c, &x), SR), 0.4, 1.9);
        eprintln!("jitter live {live} filter {filter}: {:.3} × the input's ({j_in:.1} ¢)", j / j_in);
        assert!(j / j_in <= bound, "live {live} filter {filter}: {:.3}", j / j_in);
    }
}

#[test]
fn no_warble_on_a_midpoint_vibrato() {
    // ±60 and ±100 ¢ at 6 Hz centred on the midpoint between A3 and A♯3, vibrato 100 %: few target
    // flips while the model engages, none after 0.6 s
    for depth in [60.0, 100.0] {
        let x = voice(144000, SR, 0.003, move |t| -1200.0 + 50.0 + depth * (std::f64::consts::TAU * 6.0 * t).sin());
        let mut c = make(&[("filter", 0.0), ("vibrato", 100.0)], SR, 1);
        let (mut early, mut late, mut last) = (0, 0, -1.0f32);
        for (k, blk) in x.clone().chunks_mut(64).enumerate() {
            c.process(&mut [blk]);
            let n = c.note();
            if n > 0.0 && last > 0.0 && n != last {
                if (k * 64) as f32 / SR < 0.6 {
                    early += 1;
                } else {
                    late += 1;
                }
            }
            if n > 0.0 {
                last = n;
            }
        }
        eprintln!("vibrato ±{depth} ¢ on a midpoint: {early} flips before 0.6 s, {late} after");
        assert!(early <= 6 && late == 0, "±{depth}: {early} / {late}");
    }
}

#[test]
fn live_onset_and_level() {
    // a note sung 40 ¢ flat after silence (220 Hz, Mid): corrected within 10 ¢ by 25 ms (as heard)
    let mut x = vec![0.0f32; 4800];
    x.extend(voice(48000, SR, 0.0, |_| -1240.0));
    let mut c = make(&[("filter", 0.0)], SR, 1);
    let y = run(&mut c, &x);
    let l = c.latency();
    let p = pitch_of(&y[l..], SR);
    let onset = 0.1;
    let ok = p.iter().find(|(t, v)| *t > onset && (v + 1200.0).abs() < 10.0).map(|x| x.0 - onset).unwrap();
    eprintln!("Live onset at 220 Hz: within 10 ¢ {:.1} ms after the onset", 1000.0 * ok);
    assert!(ok <= 0.025 + 0.005, "{ok}");
    // level: a 1 st correction keeps the RMS within ±0.5 dB
    for live in [1.0, 0.0] {
        let x = voice(96000, SR, 0.0, |_| -1200.0 + 45.0);
        let mut c = make(&[("filter", 0.0), ("offset", 1.0), ("fastmode", live)], SR, 1);
        let y = run(&mut c, &x);
        let rms = |v: &[f32]| (v.iter().map(|x| (x * x) as f64).sum::<f64>() / v.len() as f64).sqrt();
        let db = 20.0 * (rms(&y[24000..]) / rms(&x[24000..])).log10();
        eprintln!("level live {live}, +1.45 st: {db:+.2} dB");
        assert!(db.abs() < 0.5, "{db}");
    }
}
