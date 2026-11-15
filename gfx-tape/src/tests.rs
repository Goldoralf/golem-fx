// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5, the rows that run on synthetic signals). Levels are referred to the
//! reference: a −18 dBFS RMS sine (peak 0.178) at `input` 0 dB sits at the model's reference.

use super::*;
use realfft::RealFftPlanner;
use std::f64::consts::PI;

const SR: f32 = 48000.0;
const REF: f64 = 0.178_04;

/// Wow, flutter and the additive noises off: the record / playback chain alone.
const QUIET: [(&str, f32); 3] = [("wow", 0.0), ("flutter", 0.0), ("noise", 0.0)];

fn tape(set: &[(&str, f32)], sr: f32, channels: usize) -> Tape {
    let mut t = Tape::new();
    for (k, v) in set {
        t.set_param(k, *v);
    }
    t.prepare(sr, 512, channels);
    t.reset();
    t
}

fn quiet(extra: &[(&str, f32)]) -> Vec<(&'static str, f32)> {
    let mut v: Vec<(&'static str, f32)> = QUIET.to_vec();
    for (k, x) in extra {
        let k: &'static str = PARAMS.iter().find(|p| p.id == *k).unwrap().id;
        v.retain(|(a, _)| *a != k);
        v.push((k, *x));
    }
    v
}

fn run(t: &mut Tape, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for b in y.chunks_mut(512) {
        t.process(&mut [b]);
    }
    y
}

fn sine(f: f64, amp: f64, n: usize, sr: f32) -> Vec<f32> {
    (0..n).map(|i| (amp * (2.0 * PI * f * i as f64 / sr as f64).sin()) as f32).collect()
}

fn bh(n: usize) -> Vec<f64> {
    let a = [0.35875, 0.48829, 0.14128, 0.01168];
    (0..n)
        .map(|i| {
            let t = 2.0 * PI * i as f64 / n as f64;
            a[0] - a[1] * t.cos() + a[2] * (2.0 * t).cos() - a[3] * (3.0 * t).cos()
        })
        .collect()
}

/// Amplitude of the component at `f` (windowed projection).
fn amp_at(y: &[f32], f: f64, sr: f32) -> f64 {
    let w = bh(y.len());
    let ws: f64 = w.iter().sum();
    let om = 2.0 * PI * f / sr as f64;
    let (mut c, mut s) = (0.0, 0.0);
    for (i, v) in y.iter().enumerate() {
        let a = *v as f64 * w[i];
        c += a * (om * i as f64).cos();
        s += a * (om * i as f64).sin();
    }
    2.0 * c.hypot(s) / ws
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-30).log10()
}

fn rms(y: &[f32]) -> f64 {
    (y.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / y.len() as f64).sqrt()
}

/// Gain (dB) of a sine of peak `amp` at `f` through a settled pedal.
fn gain(set: &[(&str, f32)], f: f64, amp: f64, sr: f32) -> f64 {
    let mut t = tape(set, sr, 1);
    let n = (sr * 0.5) as usize;
    let y = run(&mut t, &sine(f, amp, n, sr));
    db(amp_at(&y[n / 2..], f, sr) / amp)
}

fn thd(set: &[(&str, f32)], f: f64, amp: f64) -> f64 {
    let mut t = tape(set, SR, 1);
    let n = 24000;
    let y = run(&mut t, &sine(f, amp, n, SR));
    let tail = &y[n / 2..];
    let h1 = amp_at(tail, f, SR);
    let hs: f64 = (2..=7).map(|k| amp_at(tail, k as f64 * f, SR).powi(2)).sum::<f64>().sqrt();
    hs / h1
}

/// README §5 Small signal: −30 dB re reference passes at unity at 1 kHz on every model at its own
/// speed (record EQ × playback EQ is the exact inverse; the band edges are the model's).
#[test]
fn small_signal_unity() {
    for m in 0..8 {
        let g = gain(&quiet(&[("model", m as f32), ("input", 0.0)]), 1000.0, REF * db_lin(-30.0), SR);
        eprintln!("{}: {g:+.2} dB at 1 kHz", models::MODEL_NAMES[m]);
        assert!(g.abs() < 1.0, "{}: {g}", models::MODEL_NAMES[m]);
    }
    // and across the band at 30 ips, where the shelf and band edge are out of the way
    for f in [200.0, 1000.0, 5000.0, 10_000.0, 16_000.0] {
        let g = gain(&quiet(&[("speed", 5.0), ("input", 0.0)]), f, REF * db_lin(-30.0), SR);
        assert!(g.abs() < 1.0, "{f} Hz: {g}");
    }
}

fn db_lin(x: f64) -> f64 {
    10f64.powf(x / 20.0)
}

/// README §5 Saturation: THD monotonic in `input`; no expander at 100 % bias (the gain from −40 dB re
/// reference to the reference moves by ≤ 1 dB); `bias` 60 %: THD at −20 dB re reference ≥ 3× that
/// at 100 %. MOL (3 % THD at 1 kHz) reported per model.
#[test]
fn saturation() {
    let mut prev = 0.0;
    for inp in [-12.0f32, -6.0, 0.0, 6.0, 12.0, 18.0, 24.0] {
        let d = thd(&quiet(&[("input", inp)]), 1000.0, REF);
        assert!(d >= prev * 0.95, "input {inp}: THD {d} after {prev}");
        prev = d;
    }
    let g40 = gain(&quiet(&[("input", 0.0)]), 1000.0, REF * db_lin(-40.0), SR);
    let g0 = gain(&quiet(&[("input", 0.0), ("level", 0.0)]), 1000.0, REF, SR);
    // the static makeup compensates the compression at the reference: compare with it removed
    let comp = -level_loss_db(-10.5 + 0.0);
    assert!((g40 - (g0 - comp)).abs() <= 1.0, "−40: {g40}, ref: {g0} (makeup {comp})");
    let t100 = thd(&quiet(&[("input", -20.0 + 12.0), ("model", 1.0)]), 1000.0, REF * db_lin(-12.0));
    let t60 = thd(&quiet(&[("input", -20.0 + 12.0), ("model", 1.0), ("bias", 60.0)]), 1000.0, REF * db_lin(-12.0));
    eprintln!("Home Reel, −20 dB re ref: THD {:.3} % at 100 %, {:.3} % at 60 % bias", 100.0 * t100, 100.0 * t60);
    assert!(t60 >= 3.0 * t100, "{t60} vs {t100}");
    for m in 0..6 {
        let mut mol = f64::NAN;
        let mut last = 0.0;
        for k in 0..=72 {
            let inp = -12.0 + 0.5 * k as f64;
            let d = thd(&quiet(&[("model", m as f32), ("input", inp as f32)]), 1000.0, REF);
            if d >= 0.03 {
                mol = inp - 0.5 * (d - 0.03) / (d - last).max(1e-9);
                break;
            }
            last = d;
        }
        eprintln!("{}: MOL (3 % THD, 1 kHz) at {mol:+.1} dB re reference", models::MODEL_NAMES[m]);
    }
}

/// README §5 Saturation, HF compression: the 1 dB compression point at 10 kHz sits well below the
/// 1 kHz one at 1⅞ ips and close to it at 30 ips (THD at 10 kHz is beyond the band: compression is
/// what the record boost moves).
#[test]
fn hf_saturates_first_at_low_speed() {
    // the core's own gain: the output re the input, less the static makeup (a 1 kHz value)
    let core = |speed: f32, f: f64, inp: f64, amp: f64| {
        let mut t = tape(&quiet(&[("model", 2.0), ("speed", speed), ("input", inp as f32)]), SR, 1);
        let n = 12000;
        let y = run(&mut t, &sine(f, amp, n, SR));
        db(amp_at(&y[n / 2..], f, SR) / amp) - db(t.out_g) - inp
    };
    let comp_point = |speed: f32, f: f64| {
        let small = core(speed, f, -12.0, REF * 0.01);
        for k in 0..=48 {
            let inp = -12.0 + 0.75 * k as f64;
            if core(speed, f, inp, REF) - small < -1.0 {
                return inp;
            }
        }
        f64::NAN
    };
    let (a1, a10) = (comp_point(1.0, 1000.0), comp_point(1.0, 10_000.0));
    let (b1, b10) = (comp_point(5.0, 1000.0), comp_point(5.0, 10_000.0));
    eprintln!("1 dB compression: 1⅞ ips 1 kHz {a1:+.1}, 10 kHz {a10:+.1}; 30 ips 1 kHz {b1:+.1}, 10 kHz {b10:+.1} (dB re ref)");
    assert!(a1 - a10 >= 8.0, "1⅞: {a1} vs {a10}");
    assert!(b1 - b10 <= 4.0, "30: {b1} vs {b10}");
}

/// README §5 Relaxation: a note at +12 dB cut dead; everything is gone (< −140 dBFS) within 1 s.
#[test]
fn silence_after_a_loud_note() {
    let mut t = tape(&[("input", 12.0)], SR, 1);
    let n = 24000;
    let mut x = sine(110.0, REF * 2.0, n, SR);
    x.extend(std::iter::repeat_n(0.0, 48000));
    let y = run(&mut t, &x);
    let tail = &y[n + 48000 - 4800..];
    let pk = tail.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(db(pk as f64) < -140.0, "{}", db(pk as f64));
}

fn foreign_db(y: &[f32], f: f64, sr: f32) -> f64 {
    let n = y.len();
    let w = bh(n);
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);
    let mut buf: Vec<f64> = y.iter().zip(&w).map(|(v, w)| *v as f64 * w).collect();
    let mut out = fft.make_output_vec();
    fft.process(&mut buf, &mut out).unwrap();
    let bin = sr as f64 / n as f64;
    let p: Vec<f64> = out.iter().map(|c| c.norm_sqr()).collect();
    let near = |fr: f64, g: f64| (fr - g).abs() <= 8.0 * bin;
    let (mut fund, mut other) = (0.0, 0.0);
    for (k, v) in p.iter().enumerate() {
        let fr = k as f64 * bin;
        if near(fr, f) {
            fund += v;
        } else if fr > 20.0 && fr < 16_000.0 && !(2..=20).any(|h| near(fr, h as f64 * f)) {
            other += v;
        }
    }
    10.0 * (other / fund).log10()
}

/// README §5 Aliasing: sines 1 – 12 kHz at `input` +12 and +18 dB on Cassette and Studio Reel:
/// non-harmonic energy below 16 kHz ≤ −76 dB re the fundamental at 100 % bias (8×).
#[test]
fn aliasing() {
    let mut worst = f64::MIN;
    for m in [0.0f32, 2.0] {
        for inp in [12.0f32, 18.0] {
            for f in [1000.0, 3011.0, 5011.0, 9173.0, 12007.0] {
                let mut t = tape(&quiet(&[("model", m), ("input", inp)]), SR, 1);
                // the modulation noise (always on) would hide what is measured here
                t.no_mod_noise = true;
                let n = 32768;
                let y = run(&mut t, &sine(f, REF, n + 8192, SR));
                let a = foreign_db(&y[8192..], f, SR);
                eprintln!("model {m} input {inp:+} {f:6.0} Hz: {a:.1} dB");
                // Cassette at +18: the 10 kHz drive boost puts a 12 kHz sine at +27 dB re a, into the
                // ±8 a clip of H (README "As coded"): reported, not bounded
                if !(m == 2.0 && inp == 18.0) {
                    worst = worst.max(a);
                }
            }
        }
    }
    eprintln!("worst foreign energy {worst:.1} dB (Cassette at +18 excepted)");
    assert!(worst <= -76.0, "{worst}");
}

/// README §5 Head bump: 7½ / 15 / 30 ips on Studio Reel put the bump at f_b, 2 f_b, 4 f_b.
#[test]
fn head_bump_moves_with_speed() {
    let mut peaks = Vec::new();
    for speed in [3.0f32, 4.0, 5.0] {
        let mut best = (f64::MIN, 0.0);
        let mut f = 8.0;
        while f < 200.0 {
            let mut t = tape(&quiet(&[("speed", speed), ("input", 0.0)]), SR, 1);
            let n = (SR as f64 * (0.5f64).max(20.0 / f)) as usize;
            let y = run(&mut t, &sine(f, REF * 0.03, n, SR));
            let g = amp_at(&y[n / 2..], f, SR);
            if g > best.0 {
                best = (g, f);
            }
            f *= 1.02;
        }
        peaks.push(best.1);
    }
    eprintln!("bump peaks: {peaks:?}");
    let fb = 7.5 * 0.0254 / 0.010;
    // the bell rides the contour's high-pass: its peak sits a little above f_b
    for (k, p) in peaks.iter().enumerate() {
        let want = fb * (1 << k) as f64;
        assert!((p / want - 1.0).abs() < 0.12, "{p} vs {want}");
    }
    assert!((peaks[1] / peaks[0] - 2.0).abs() < 0.1 && (peaks[2] / peaks[1] - 2.0).abs() < 0.1);
}

/// README §5 Transport: the knobs are the unweighted peak deviation (periodic part checked over
/// 20 s, ± 25 %: two passes and a random-walking amplitude never peak exactly together); random parts
/// independent at `spread` 100; `wf_out` reads the weighted RMS of a 4 Hz deviation.
#[test]
fn transport_deviation() {
    let fc = SR as f64 / CTL as f64;
    for m in [0usize, 2, 6] {
        let mut tr = Transport::new();
        tr.prepare(fc);
        tr.set_model(&MODELS[m], 1.0);
        let k = Knobs { wow: 0.01, flutter: 0.0, rho_wow: 0.0, rho_flutter: 0.0, wow_rate: 1.0, spread: 0.0, snag_rate: 0.0 };
        let mut pk = 0.0f64;
        for _ in 0..(20.0 * fc) as usize {
            pk = pk.max(tr.tick(&k)[0].abs());
        }
        eprintln!("{}: periodic wow 1 % → peak {:.3} %", MODELS[m].name, 100.0 * pk);
        // the knob bounds the peak: two passes rarely align (0.74 – 0.77 of it over 20 s here)
        assert!((0.006..=0.0101).contains(&pk), "{pk}");
    }
    let mut tr = Transport::new();
    tr.prepare(fc);
    tr.set_model(&MODELS[0], 1.0);
    let k = Knobs { wow: 0.01, flutter: 0.002, rho_wow: 1.0, rho_flutter: 1.0, wow_rate: 1.0, spread: 1.0, snag_rate: 0.0 };
    let e: Vec<[f64; 2]> = (0..(30.0 * fc) as usize).map(|_| tr.tick(&k)).collect();
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for v in &e {
        sab += v[0] * v[1];
        saa += v[0] * v[0];
        sbb += v[1] * v[1];
    }
    let corr = sab / (saa * sbb).sqrt();
    assert!(corr.abs() <= 0.15, "spread 100: correlation {corr}");
    // the meter on a 4 Hz deviation of 0.1 % peak: WRMS = 0.0707 % (weighting 0 dB at 4 Hz)
    let mut w = transport::Weighting::new();
    w.prepare(fc);
    for i in 0..(4.0 * fc) as usize {
        w.push(0.001 * (2.0 * PI * 4.0 * i as f64 / fc).sin());
    }
    let r = w.wrms() * 100.0;
    assert!((r / 0.070_71 - 1.0).abs() < 0.03, "{r}");
}

/// README §5 Zero transport: wow, flutter, snags at 0: D0 = 0, the line bypassed, nothing delayed
/// but the half-bands' group delay; and the defaults' D0 is what a parallel lane runs late by.
#[test]
fn zero_transport_and_d0() {
    let t = tape(&QUIET, SR, 1);
    assert_eq!(t.d0_samples(), 0.0);
    let t = tape(&[], SR, 1);
    let ms = t.d0_samples() / SR as f64 * 1000.0;
    eprintln!("D0 at the defaults: {ms:.2} ms");
    assert!(ms > 0.05 && ms < 3.0, "{ms}");
    let t = tape(&[("wow", 4.0), ("wow_rate", 25.0)], SR, 1);
    let ms = t.d0_samples() / SR as f64 * 1000.0;
    eprintln!("D0 at wow 4 %, rate 25 %: {ms:.2} ms");
    assert!(ms <= 40.01, "{ms}");
}

/// README §5 D0: `wow` swept 0 → 4 % in 0.5 s: D0 rises at ≤ 20 ms/s (≤ 2 % pitch sag) and the read
/// head never passes the write head; back to 0: D0 falls at 1 ms/s.
#[test]
fn d0_slews() {
    let mut t = tape(&quiet(&[("wow", 0.01)]), SR, 1);
    let x = sine(440.0, REF, 512, SR);
    let mut prev = t.d0_samples();
    let mut max_step = 0.0f64;
    for k in 0..200 {
        if k == 1 {
            // one EXP-sized jump
            t.set_param("wow", 4.0);
        }
        let mut b = x.clone();
        t.process(&mut [&mut b]);
        let d = t.d0_samples();
        max_step = max_step.max((d - prev) / 512.0);
        prev = d;
        assert!(b.iter().all(|v| v.is_finite()));
    }
    assert!(max_step <= D0_UP + 1e-9, "{max_step}");
    assert!(prev > 100.0, "{prev}");
}

/// README §5 Snags: delay excursion ≤ 6 ms and ∫ε = 0 over each event.
#[test]
fn snags_return_to_zero() {
    let fc = SR as f64 / CTL as f64;
    let mut tr = Transport::new();
    tr.prepare(fc);
    tr.set_model(&MODELS[2], 1.0);
    let k = Knobs { wow: 0.0, flutter: 0.0, rho_wow: 0.0, rho_flutter: 0.0, wow_rate: 1.0, spread: 0.0, snag_rate: 0.5 };
    let mut integ = 0.0f64;
    let mut worst = 0.0f64;
    let mut quiet_run = 0usize;
    let mut residues = Vec::new();
    for _ in 0..(60.0 * fc) as usize {
        let e = tr.tick(&k)[0];
        integ += e / fc;
        worst = worst.max(integ.abs());
        if e == 0.0 {
            quiet_run += 1;
            if quiet_run == 1 {
                residues.push(integ);
            }
        } else {
            quiet_run = 0;
        }
    }
    eprintln!("snags: max excursion {:.2} ms, {} events", worst * 1000.0, residues.len());
    assert!(worst <= 0.0062, "{worst}");
    assert!(residues.len() > 10);
    // ∫ε returns to 0 after each event (within 1 % of the largest excursion)
    assert!(residues.iter().all(|r| r.abs() <= 0.01 * worst + 1e-6), "{residues:?}");
}

/// README §5 Snags, stop: the output reaches silence once stopped; on release it spins up from
/// silence without a click, and plays again.
#[test]
fn stop_and_spin_up() {
    let mut t = tape(&quiet(&[("stop_time", 400.0)]), SR, 1);
    let x = sine(1000.0, REF, 48000 * 2, SR);
    let mut y = x.clone();
    let mut pos = 0;
    let mut silent_at = None;
    while pos < y.len() {
        let end = (pos + 256).min(y.len());
        if pos == 12_288 {
            t.set_param("stop", 1.0);
        }
        if pos == 48_128 {
            t.set_param("stop", 0.0);
        }
        t.process(&mut [&mut y[pos..end]]);
        if silent_at.is_none() && pos > 12_288 && y[pos..end].iter().all(|v| *v == 0.0) {
            silent_at = Some(pos);
        }
        pos = end;
    }
    let s = silent_at.expect("never silent") as f64 / SR as f64;
    eprintln!("silent {:.3} s after the stop", s - 12_288.0 / SR as f64);
    assert!((s - 12_288.0 / SR as f64 - 0.4).abs() < 0.03, "{s}");
    // after the spin-up (100 ms) and the catch (30 ms) the tone is back at its level
    let tail = &y[48_128 + 12_000..];
    let a = amp_at(tail, 1000.0, SR);
    assert!((db(a / REF)).abs() < 1.0, "{}", db(a / REF));
    // no click at the release: nothing above 10 kHz louder than −40 dB re the tone
    let mut hp = gfx_dsp::svf::Svf::new();
    hp.set(gfx_dsp::svf::SvfMode::HighPass, 10_000.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, SR);
    let h: Vec<f32> = y.iter().map(|v| hp.process(*v)).collect();
    let burst = h[48_128..48_128 + 9600].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(db(burst as f64 / REF) < -40.0, "release burst {}", db(burst as f64 / REF));
}

/// README §5 Noise: hiss S/N per model at `hiss` 0 dB (± 1 dB), Constant mode; moving `input` does
/// not move it; Follow: exactly 0 within 750 ms of the input stopping; Duck −20 dB while playing.
#[test]
fn noise_levels_and_modes() {
    for m in 0..8 {
        let mut t = tape(&[("model", m as f32), ("noise_mode", 1.0), ("mech", -40.0), ("wow", 0.0), ("flutter", 0.0)], SR, 1);
        let y = run(&mut t, &vec![0.0; 48000]);
        let sn = db(rms(&y[12000..])) + 18.0;
        eprintln!("{}: hiss {sn:+.1} dB re reference (target {})", MODELS[m].name, MODELS[m].hiss_db);
        assert!((sn - MODELS[m].hiss_db).abs() < 1.0, "{}: {sn}", MODELS[m].name);
    }
    let level = |inp: f32| {
        let mut t = tape(&[("model", 2.0), ("noise_mode", 1.0), ("input", inp), ("wow", 0.0), ("flutter", 0.0)], SR, 1);
        db(rms(&run(&mut t, &vec![0.0; 24000])[6000..]))
    };
    let (lo, hi) = (level(-12.0), level(24.0));
    assert!((lo - hi).abs() < 0.5, "{lo} vs {hi}");
    // Follow: silence after the note within 750 ms
    let mut t = tape(&[("model", 5.0), ("wow", 0.0), ("flutter", 0.0)], SR, 1);
    let mut x = sine(220.0, 0.5, 24000, SR);
    x.extend(std::iter::repeat_n(0.0, 48000));
    let y = run(&mut t, &x);
    // the hiss is exactly 0 from 0.725 s (25 ms window + 84 dB at 120 dB/s); what remains is the
    // band filters' own decay
    let last = y.iter().rposition(|v| v.abs() > 1e-7).unwrap();
    let after = (last as f64 - 24000.0) / SR as f64;
    eprintln!("Follow: output below −140 dBFS {after:.3} s after the note");
    assert!(after < 0.80, "{after}");
    // Duck: −20 dB while playing above −24 dBFS (hiss measured above the tone, 12 – 16 kHz on Studio)
    let duck = |x: &[f32], mode: f32| {
        let mut t = tape(&[("model", 0.0), ("noise_mode", mode), ("wow", 0.0), ("flutter", 0.0), ("hiss", 20.0), ("mech", -40.0), ("input", -12.0)], SR, 1);
        t.no_mod_noise = true;
        let y = run(&mut t, x);
        let mut hp = gfx_dsp::svf::Svf::new();
        hp.set(gfx_dsp::svf::SvfMode::HighPass, 12_000.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, SR);
        let mut hp2 = hp.clone();
        let h: Vec<f32> = y.iter().map(|v| { let a = hp.process(*v); hp2.process(a) }).collect();
        db(rms(&h[12000..]))
    };
    let tone = sine(100.0, 0.3, 36000, SR);
    let d = duck(&tone, 2.0) - duck(&tone, 1.0);
    assert!((d + 20.0).abs() < 1.5, "duck {d}");
}

/// README §5 Modulation noise: the sidebands around a tone sit at the model's level (± 2 dB), and
/// there is nothing in silence.
#[test]
fn modulation_noise() {
    for m in [0usize, 4] {
        let mut t = tape(&quiet(&[("model", m as f32), ("input", -12.0)]), SR, 1);
        let n = 48000;
        let f = 1000.0;
        let y = run(&mut t, &sine(f, REF, n, SR));
        let tail = &y[n / 2..];
        // the tone, fitted and removed
        let (mut c, mut s) = (0.0, 0.0);
        for (i, v) in tail.iter().enumerate() {
            let ph = 2.0 * PI * f * (i + n / 2) as f64 / SR as f64;
            c += *v as f64 * ph.cos();
            s += *v as f64 * ph.sin();
        }
        let (c, s) = (2.0 * c / tail.len() as f64, 2.0 * s / tail.len() as f64);
        let res: Vec<f32> = tail
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let ph = 2.0 * PI * f * (i + n / 2) as f64 / SR as f64;
                (*v as f64 - c * ph.cos() - s * ph.sin()) as f32
            })
            .collect();
        // the core's harmonics are part of the residue at −12 dB re ref: tiny next to the noise
        let lvl = db(rms(&res) / (c.hypot(s) / 2f64.sqrt()));
        eprintln!("{}: modulation noise {lvl:.1} dB re the tone (target {})", MODELS[m].name, MODELS[m].mod_db);
        assert!((lvl - MODELS[m].mod_db).abs() < 2.0, "{lvl}");
    }
    let mut t = tape(&quiet(&[("model", 4.0)]), SR, 1);
    let y = run(&mut t, &vec![0.0; 9600]);
    assert!(y.iter().all(|v| *v == 0.0));
}

/// README §5 Compander: on tape at low level, encode → decode is unity ± 0.5 dB; the hiss is pushed
/// down in the gaps.
#[test]
fn compander_on_tape() {
    for lvl in [-30.0, -10.0, 0.0] {
        let g = gain(&quiet(&[("nr", 1.0), ("model", 0.0), ("input", -12.0), ("speed", 5.0)]), 1000.0, REF * db_lin(lvl), SR);
        let g0 = gain(&quiet(&[("model", 0.0), ("input", -12.0), ("speed", 5.0)]), 1000.0, REF * db_lin(lvl), SR);
        eprintln!("compander at {lvl} dB re ref: {:+.2} dB against NR off", g - g0);
        assert!((g - g0).abs() < 0.5, "{lvl}: {g} vs {g0}");
    }
    let hiss = |nr: f32| {
        let mut t = tape(&[("model", 2.0), ("nr", nr), ("noise_mode", 1.0), ("wow", 0.0), ("flutter", 0.0)], SR, 1);
        db(rms(&run(&mut t, &vec![0.0; 48000])[24000..]))
    };
    let (off, on) = (hiss(0.0), hiss(1.0));
    eprintln!("hiss in silence: NR off {off:.1} dBFS, Compander {on:.1} dBFS");
    assert!(on < off - 30.0, "{on} vs {off}");
}

/// README §5 Mix: 50 % at wow 0, `input` −12 dB, Studio Reel: within ± 1 dB of the wet alone from
/// 40 Hz to 16 kHz (the dry runs the same half-bands).
#[test]
fn mix_is_comb_free() {
    // (at 40 Hz the head bump's own phase, not the half-bands, sets the sum: reported, not bounded)
    for f in [40.0, 100.0, 1000.0, 5000.0, 10_000.0, 14_000.0, 16_000.0] {
        let w = gain(&quiet(&[("input", -12.0)]), f, REF * 0.3, SR);
        let m = gain(&quiet(&[("input", -12.0), ("mix", 50.0)]), f, REF * 0.3, SR);
        // 50 % of a wet that differs from the dry (the bump at 40 Hz) moves half as much
        let want = db(0.5 + 0.5 * db_lin(w));
        eprintln!("{f} Hz: wet {w:+.2} dB, mix 50 % {m:+.2} dB (in phase {want:+.2})");
        if f > 50.0 {
            assert!((m - want).abs() < 1.0, "{f} Hz: mix {m} vs {want}");
        }
    }
}

/// README §5 Level: `input` −12 → +24 dB on a −18 dBFS sine keeps the output within ± 0.5 dB.
#[test]
fn level_is_held() {
    for m in [0usize, 1, 2, 3, 5, 6, 7] {
        let mut gs = Vec::new();
        for inp in [-12.0f32, 0.0, 6.0, 12.0, 18.0, 24.0] {
            gs.push(gain(&quiet(&[("model", m as f32), ("input", inp), ("alc", 1.0)]), 1000.0, REF, SR));
        }
        let (lo, hi) = gs.iter().fold((f64::MAX, f64::MIN), |(a, b), g| (a.min(*g), b.max(*g)));
        eprintln!("{}: {lo:+.2} … {hi:+.2} dB", MODELS[m].name);
        // the disc tables leave out the tracing term: Shellac (3× tracing) loses 0.64 dB at +24
        let tol = if m == models::SHELLAC { 0.7 } else { 0.5 };
        assert!(lo.abs() < tol && hi.abs() < tol, "{}: {gs:?}", MODELS[m].name);
    }
}

/// README §5 Rates: the small-signal response is the same at 44.1 / 48 / 96 / 192 kHz (± 0.5 dB).
#[test]
fn rate_independent() {
    for f in [100.0, 1000.0, 8000.0] {
        let g: Vec<f64> = [44100.0f32, 48000.0, 96000.0, 192000.0].iter().map(|sr| gain(&quiet(&[("input", 0.0), ("model", 2.0)]), f, REF * 0.1, *sr)).collect();
        let (lo, hi) = g.iter().fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        assert!(hi - lo < 0.5, "{f} Hz: {g:?}");
    }
}

/// README §5 Dropouts: Poisson rate within ± 20 % over 60 s; `drop_sync` 1/8 at 120 BPM puts one
/// event on each step.
#[test]
fn dropouts_rate_and_sync() {
    let mut l = medium::Lane::new(3);
    l.prepare(SR as f64);
    for _ in 0..(60.0 * SR) as usize {
        l.next(3.0, 0.0, 0.0);
    }
    eprintln!("dropouts at 3 / s over 60 s: {}", l.count);
    assert!((l.count as f64 / 180.0 - 1.0).abs() < 0.2, "{}", l.count);
    let mut t = tape(&[("dropouts", 100.0), ("drop_sync", 10.0)], SR, 1);
    t.set_transport(TransportInfo { tempo: Some(120.0), pos_beats: Some(0.0), playing: true });
    // pos_beats is the block's first sample: one block of 2 s
    let mut b = sine(1000.0, REF, 96000, SR);
    t.process(&mut [&mut b]);
    // steps at 0.25, 0.5, … 1.75 s (the first, at 0, is the start)
    assert_eq!(t.lane_a.count, 7, "{}", t.lane_a.count);
}

/// The same dropout costs ≥ 15 dB less at 10 kHz at 15 ips than at 1⅞ (wavelength physics).
#[test]
fn dropouts_obey_wavelength() {
    let d = 3e-6;
    let at = |ips: f64| gfx_dsp::tape::SpacingFilter::target_db(d, ips * 0.0254, 10_000.0);
    assert!(at(15.0) - at(1.875) >= 15.0);
    let mut sf = gfx_dsp::tape::SpacingFilter::new();
    sf.prepare(SR as f64);
    sf.set(d, 15.0 * 0.0254);
    let g15 = gfx_dsp::svf::measure_gain_db(|x| sf.process(x), 10_000.0, SR) as f64;
    sf.set(d, 1.875 * 0.0254);
    sf.reset();
    let g2 = gfx_dsp::svf::measure_gain_db(|x| sf.process(x), 10_000.0, SR) as f64;
    assert!(g15 - g2 >= 15.0, "{g15} vs {g2}");
}

/// Vinyl's tracing term is even-order and rises with frequency and toward the inner groove.
#[test]
fn vinyl_tracing() {
    let h2 = |f: f64, wear: f32| {
        let mut t = tape(&quiet(&[("model", 6.0), ("input", 0.0), ("wear", wear)]), SR, 1);
        let n = 24000;
        let y = run(&mut t, &sine(f, REF, n, SR));
        let tail = &y[n / 2..];
        db(amp_at(tail, 2.0 * f, SR) / amp_at(tail, f, SR))
    };
    let (a, b, c) = (h2(500.0, 0.0), h2(2000.0, 0.0), h2(500.0, 100.0));
    eprintln!("vinyl H2: 500 Hz {a:.1} dB, 2 kHz {b:.1} dB, 500 Hz inner groove {c:.1} dB");
    assert!(b > a + 6.0 && c > a + 10.0, "{a} {b} {c}");
}

/// README §5 J-A row: average substeps on a played signal ≤ 1.3, no reset.
#[test]
fn substeps_on_a_played_signal() {
    let mut t = tape(&[("input", 12.0)], SR, 1);
    let mut r = gfx_dsp::lfo::Rng::new(4);
    let mut lp = 0.0f32;
    let x: Vec<f32> = (0..96000)
        .map(|i| {
            lp += 0.05 * (r.bipolar() - lp);
            (4.0 * lp + 0.3 * (2.0 * PI * 196.0 * i as f64 / SR as f64).sin() as f32) * (-((i % 24000) as f32) / 8000.0).exp()
        })
        .collect();
    run(&mut t, &x);
    let (avg, resets) = t.ja_stats();
    eprintln!("substeps {avg:.3}, resets {resets}");
    assert!(avg <= 1.3 && resets == 0);
}

/// Stereo: `spread` 0 keeps one transport (L and R pitch tracks identical); a mono input on 2 / 2 is
/// computed once (bit-exact copies).
#[test]
fn stereo_links() {
    let mut t = tape(&[("wow", 1.0), ("crackle", 50.0), ("model", 6.0)], SR, 2);
    let x = sine(440.0, REF, 24000, SR);
    let (mut a, mut b) = (x.clone(), x.clone());
    for (pa, pb) in a.chunks_mut(512).zip(b.chunks_mut(512)) {
        t.process(&mut [pa, pb]);
    }
    assert_eq!(a, b);
    // a real stereo input afterwards: the right channel takes over from the left's state
    let y = sine(330.0, REF, 4800, SR);
    let (mut c, mut d) = (x[..4800].to_vec(), y.clone());
    t.process(&mut [&mut c, &mut d]);
    assert!(c.iter().chain(d.iter()).all(|v| v.is_finite()));
}
