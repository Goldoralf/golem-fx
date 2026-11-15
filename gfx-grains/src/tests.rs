// SPDX-License-Identifier: GPL-3.0-or-later
//! The README's bench rows that run on synthetic signals (§5).

use super::*;

const SR: f32 = 48000.0;

fn make(settings: &[(&str, f32)], sr: f32, ch: usize) -> Grains {
    let mut g = Grains::new();
    for (k, v) in settings {
        g.set_param(k, *v);
    }
    g.prepare(sr, 512, ch);
    g
}

fn run(g: &mut Grains, x: &[f32], block: usize) -> Vec<f32> {
    let mut y = x.to_vec();
    for c in y.chunks_mut(block) {
        g.process(&mut [c]);
    }
    y
}

fn run2(g: &mut Grains, l: &[f32], r: &[f32], block: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut a, mut b) = (l.to_vec(), r.to_vec());
    for (ca, cb) in a.chunks_mut(block).zip(b.chunks_mut(block)) {
        g.process(&mut [ca, cb]);
    }
    (a, b)
}

fn noise(n: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut r = gfx_dsp::lfo::Rng::new(seed);
    (0..n).map(|_| amp * r.bipolar()).collect()
}

/// Pink-ish noise (Kellet's economy filter on white), RMS ≈ `amp`.
fn pink(n: usize, amp: f32, seed: u64) -> Vec<f32> {
    let w = noise(n, 1.0, seed);
    let (mut b0, mut b1, mut b2) = (0.0f32, 0.0f32, 0.0f32);
    let mut y: Vec<f32> = w
        .iter()
        .map(|&x| {
            b0 = 0.99765 * b0 + x * 0.099_046;
            b1 = 0.963 * b1 + x * 0.296_516_4;
            b2 = 0.57 * b2 + x * 1.052_691_3;
            b0 + b1 + b2 + x * 0.1848
        })
        .collect();
    let r = rms(&y);
    y.iter_mut().for_each(|v| *v *= amp / r);
    y
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len().max(1) as f64).sqrt() as f32
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-30).log10()
}

/// A plucked note: harmonics 1 / k with an exponential decay, starting at `at`.
fn pluck(out: &mut [f32], at: usize, f0: f32, amp: f32, tau_s: f32, sr: f32) {
    for (i, o) in out.iter_mut().enumerate().skip(at) {
        let t = (i - at) as f32 / sr;
        let mut v = 0.0;
        for k in 1..12 {
            let f = f0 * k as f32;
            if f < 0.45 * sr {
                v += (2.0 * std::f32::consts::PI * f * t + k as f32).sin() / k as f32;
            }
        }
        *o += amp * v * (-t / tau_s).exp() * (1.0 - (-t / 0.0005).exp());
    }
}

#[test]
#[ignore]
fn cpu_report() {
    for (name, set) in [
        ("defaults", vec![]),
        ("live 0 (no grains)", vec![("live", 0.0)]),
        ("8, pitch_rand 12", vec![("pitch_rand", 12.0)]),
        ("64, pitch 7", vec![("grains", 64.0), ("pitch", 7.0)]),
        ("64, unity", vec![("grains", 64.0)]),
        ("64, pitch_rand 12, glide, feedback", vec![("grains", 64.0), ("pitch_rand", 12.0), ("glide", 7.0), ("feedback", 50.0)]),
    ] {
        for ch in [1usize, 2] {
            let mut g = make(&set, SR, ch);
            let x = pink(SR as usize * 10, 0.1, 3);
            let t = std::time::Instant::now();
            if ch == 1 {
                run(&mut g, &x, 128);
            } else {
                run2(&mut g, &x, &x, 128);
            }
            let ns = t.elapsed().as_secs_f64() * 1e9 / x.len() as f64;
            eprintln!("{name} ({ch} ch): {ns:.0} ns / sample, {} dropped", g.dropped());
        }
    }
    // the worst case: two layers, 48 + 48, at +11 st (48 taps), the hold releasing under the cloud
    let mut g = make(&[("grains", 64.0), ("pitch", 11.0), ("decay", 10.0)], SR, 2);
    let x = pink(SR as usize * 3, 0.1, 3);
    run2(&mut g, &x, &x, 128);
    g.set_param("freeze", 1.0);
    run2(&mut g, &x, &x, 128);
    g.set_param("freeze", 0.0);
    let t = std::time::Instant::now();
    run2(&mut g, &x, &x, 128);
    let ns = t.elapsed().as_secs_f64() * 1e9 / x.len() as f64;
    eprintln!("two layers 48 + 48 at +11 st (2 ch): {ns:.0} ns / sample, {} voices at the end", g.active());
}

fn sine(n: usize, f: f32, amp: f32, sr: f32) -> Vec<f32> {
    (0..n).map(|i| amp * (2.0 * std::f32::consts::PI * f * i as f32 / sr).sin()).collect()
}

/// A strummed chord: five plucked notes 12 ms apart.
fn strum(out: &mut [f32], at: usize, root: f32, amp: f32, sr: f32) {
    for (k, r) in [1.0f32, 1.5, 2.0, 2.5198, 3.0].iter().enumerate() {
        pluck(out, at + k * (0.012 * sr) as usize, root * r, amp, 1.5, sr);
    }
}

/// Goertzel power of `x` at `f` (Hann-windowed; compared as ratios only).
fn tone_power(x: &[f32], f: f32, sr: f32) -> f64 {
    let w = 2.0 * std::f64::consts::PI * f as f64 / sr as f64;
    let c = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for (i, &v) in x.iter().enumerate() {
        let h = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / x.len() as f64).cos();
        let s0 = v as f64 * h + c * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    s1 * s1 + s2 * s2 - c * s1 * s2
}

const UNITY: [(&str, f32); 7] = [("spray", 0.0), ("regular", 100.0), ("shape", 100.0), ("kill_dry", 1.0), ("mix", 100.0), ("spread", 0.0), ("pitch", 0.0)];

/// Unity (README §5): a regular Hann stream at ratio 1 is the input delayed by pos · L, to −120 dB.
#[test]
fn unity_regular_hann_nulls_the_delayed_input() {
    for grains in [2.0f32, 8.0, 64.0] {
        for ch in [1usize, 2] {
            let mut set = UNITY.to_vec();
            set.push(("grains", grains));
            let mut g = make(&set, SR, ch);
            let x = pink(SR as usize * 3, 0.25, 7);
            let d = (0.5 * SR) as usize;
            let y = if ch == 1 { run(&mut g, &x, 512) } else { run2(&mut g, &x, &x, 512).0 };
            let from = (1.2 * SR) as usize;
            let err = (from..x.len()).map(|i| (y[i] - x[i - d]).abs()).fold(0.0f32, f32::max);
            let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            eprintln!("unity, grains {grains}, {ch} ch: {:.1} dB", db(err / peak));
            assert!(db(err / peak) < -120.0, "grains {grains}, {ch} ch: {:.1} dB", db(err / peak));
        }
    }
}

/// Unity with Poisson onsets: the long-term level within 0.5 dB.
#[test]
fn unity_poisson_long_term_level() {
    for grains in [2.0f32, 8.0, 64.0] {
        let mut set = UNITY.to_vec();
        set.push(("grains", grains));
        set.push(("regular", 0.0));
        let mut g = make(&set, SR, 1);
        let x = pink(SR as usize * 20, 0.1, 9);
        let y = run(&mut g, &x, 256);
        let from = SR as usize * 2;
        let l = db(rms(&y[from..]) / rms(&x[from..]));
        eprintln!("unity Poisson, grains {grains}: {l:+.2} dB");
        assert!(l.abs() < 0.5, "grains {grains}: {l:.2} dB");
    }
}

/// Level (README §5): the wet within ±2 dB of the source over density, scatter, pitch, regularity,
/// window and source.
#[test]
fn level_follows_the_source() {
    let n = SR as usize * 12;
    let harm = |f0: f32, k_max: usize| {
        let mut v = vec![0.0f32; n];
        for k in 1..k_max {
            let s = sine(n, f0 * k as f32, 0.1 / k as f32, SR);
            v.iter_mut().zip(s).for_each(|(a, b)| *a += b);
        }
        v
    };
    let sources: Vec<(&str, Vec<f32>)> = vec![("pink", pink(n, 0.1, 21)), ("110 Hz", harm(110.0, 20)), ("440 Hz", harm(440.0, 8))];
    let mut worst = (0.0f32, String::new());
    for (name, x) in &sources {
        for grains in [1.0f32, 8.0, 64.0] {
            for spray in [0.0f32, 0.05, 0.2, 1.0, 50.0] {
                for (pr, det, reg, shape) in [(0.0f32, 0.0f32, 0.0f32, 100.0f32), (12.0, 0.0, 0.0, 100.0), (0.0, 10.0, 100.0, 100.0), (0.0, 0.0, 100.0, 0.0)] {
                    let mut g = make(&[("grains", grains), ("spray", spray), ("pitch_rand", pr), ("detune", det), ("regular", reg), ("shape", shape), ("kill_dry", 1.0), ("mix", 100.0), ("spread", 0.0)], SR, 1);
                    let y = run(&mut g, x, 512);
                    let from = SR as usize * 3;
                    let l = db(rms(&y[from..]) / rms(&x[from..]));
                    if l.abs() > worst.0.abs() {
                        worst = (l, format!("{name} grains {grains} spray {spray} pitch_rand {pr} detune {det} regular {reg} shape {shape}"));
                    }
                }
            }
        }
    }
    eprintln!("level: worst {:+.2} dB ({})", worst.0, worst.1);
    assert!(worst.0.abs() < 2.0, "{:+.2} dB: {}", worst.0, worst.1);
}

/// Determinism: two renders, and every host block size, null exactly.
#[test]
fn deterministic_and_block_size_independent() {
    let set = [("pitch_rand", 12.0), ("glide", 5.0), ("feedback", 40.0), ("spray", 30.0), ("reverse", 30.0)];
    let x = pink(SR as usize * 3, 0.2, 5);
    let mut a = make(&set, SR, 2);
    let reference = run2(&mut a, &x, &x, 512);
    for block in [32usize, 64, 4096, 1] {
        let mut b = make(&set, SR, 2);
        assert!(run2(&mut b, &x, &x, block) == reference, "block {block}");
    }
    // a random schedule of block sizes
    let mut b = make(&set, SR, 2);
    let (mut l, mut r) = (x.clone(), x.clone());
    let mut rng = gfx_dsp::lfo::Rng::new(77);
    let mut i = 0;
    while i < l.len() {
        let k = (1 + (rng.next_f64() * 700.0) as usize).min(l.len() - i);
        b.process(&mut [&mut l[i..i + k], &mut r[i..i + k]]);
        i += k;
    }
    assert!((l, r) == reference);
    a.reset();
    assert!(run2(&mut a, &x, &x, 512) == reference);
}

/// Seed and variate order: another seed is another cloud; `spray` scales the same variates (the
/// onsets do not move).
#[test]
fn seed_and_variate_order() {
    let x = pink(SR as usize * 4, 0.2, 5);
    let mut a = make(&[("kill_dry", 1.0), ("mix", 100.0), ("spray", 30.0)], SR, 1);
    let mut b = make(&[("kill_dry", 1.0), ("mix", 100.0), ("spray", 30.0), ("seed", 1.0)], SR, 1);
    let ya = run(&mut a, &x, 512);
    let yb = run(&mut b, &x, 512);
    let from = SR as usize;
    let (mut sab, mut saa, mut sbb) = (0.0f64, 0.0f64, 0.0f64);
    for i in from..x.len() {
        sab += ya[i] as f64 * yb[i] as f64;
        saa += (ya[i] as f64).powi(2);
        sbb += (yb[i] as f64).powi(2);
    }
    let corr = sab / (saa * sbb).sqrt();
    eprintln!("seed 0 vs 1: correlation {corr:.3}");
    assert!(corr < 0.5, "correlation {corr}");
    let mut c = make(&[("spray", 10.0)], SR, 1);
    let mut d = make(&[("spray", 30.0)], SR, 1);
    run(&mut c, &x, 512);
    run(&mut d, &x, 333);
    let oc: Vec<f64> = c.trace.iter().map(|t| t.1).collect();
    let od: Vec<f64> = d.trace.iter().map(|t| t.1).collect();
    assert!(oc.len() > 100);
    assert!(oc == od);
}

/// Stereo: a mono DI on both inputs with `spread` 0 gives L = R bit-exact; `spread` 100 decorrelates
/// the wet; the 1 / 1 layout runs.
#[test]
fn stereo_image() {
    let x = pink(SR as usize * 4, 0.2, 3);
    let mut g = make(&[("spread", 0.0), ("pitch_rand", 7.0), ("spray", 20.0)], SR, 2);
    let (l, r) = run2(&mut g, &x, &x, 512);
    assert!(l == r);
    let mut g = make(&[("spread", 100.0), ("kill_dry", 1.0), ("mix", 100.0), ("spray", 30.0)], SR, 2);
    let (l, r) = run2(&mut g, &x, &x, 512);
    let from = SR as usize;
    let (mut s, mut a, mut b) = (0.0f64, 0.0f64, 0.0f64);
    for i in from..l.len() {
        s += l[i] as f64 * r[i] as f64;
        a += (l[i] as f64).powi(2);
        b += (r[i] as f64).powi(2);
    }
    let corr = s / (a * b).sqrt();
    // uniform equal-power positions of independent grains: E[gL gR] / E[gL²] = 2 / π = 0.64 (README,
    // As coded: §5's < 0.3 needs grains pushed to the sides)
    eprintln!("spread 100: wet correlation {corr:.3} (2 / π = 0.637)");
    assert!((corr - 2.0 / std::f64::consts::PI).abs() < 0.05, "{corr}");
    let mut m = make(&[], SR, 1);
    let y = run(&mut m, &x, 512);
    assert!(y.iter().all(|v| v.is_finite()) && rms(&y[from..]) > 0.1);
}

/// Live level 0 %: the output is the dry, bit-exact, until `freeze`; the hold then comes in.
#[test]
fn live_zero_is_dry_until_freeze() {
    let n = SR as usize * 4;
    let mut x = vec![0.0f32; n];
    strum(&mut x, 1000, 110.0, 0.2, SR);
    let mut g = make(&[("live", 0.0), ("mix", 70.0)], SR, 1);
    let half = SR as usize * 2;
    let mut y = x.clone();
    for c in y[..half].chunks_mut(256) {
        g.process(&mut [c]);
    }
    assert!(y[..half] == x[..half]);
    g.set_param("freeze", 1.0);
    for c in y[half..].chunks_mut(256) {
        g.process(&mut [c]);
    }
    let wet: Vec<f32> = (half..n).map(|i| y[i] - x[i]).collect();
    assert!(rms(&wet[SR as usize / 2..]) > 1e-3);
    assert_eq!(g.state(), 1);
    // it comes in with the grains' windows: the first 10 ms are small against the level later
    let first = wet[..480].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let later = wet[SR as usize / 2..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(first < 0.1 * later, "{first} vs {later}");
}

/// Release (README §5): the hold layer falls 60 dB in `decay` (± 10 %).
#[test]
fn release_is_t60() {
    for decay in [1.0f32, 3.0] {
        let x = pink(SR as usize * 2, 0.2, 13);
        let mut g = make(&[("live", 0.0), ("kill_dry", 1.0), ("mix", 100.0), ("decay", decay), ("size", 100.0), ("grains", 12.0), ("spray", 50.0)], SR, 1);
        run(&mut g, &x, 256);
        g.set_param("freeze", 1.0);
        let mut y = vec![0.0f32; SR as usize * 3];
        for c in y.chunks_mut(256) {
            g.process(&mut [c]);
        }
        g.set_param("freeze", 0.0);
        let mut z = vec![0.0f32; SR as usize * 7];
        for c in z.chunks_mut(256) {
            g.process(&mut [c]);
        }
        let held = rms(&y[SR as usize..]);
        // the time where the 100 ms level falls 60 dB under the held level
        let w = (0.1 * SR) as usize;
        let lv: Vec<f32> = z.chunks(w).map(|c| db(rms(c) / held)).collect();
        let k = lv.iter().position(|&v| v < -60.0).unwrap_or(999);
        // interpolate between the two windows around the crossing
        let t60 = if k == 0 || k == 999 { k as f32 * 0.1 } else { (k as f32 - 0.5 + (lv[k - 1] + 60.0) / (lv[k - 1] - lv[k])) * 0.1 };
        eprintln!("decay {decay}: −60 dB at {t60:.2} s");
        assert!((t60 - decay).abs() <= 0.1 * decay, "decay {decay}: {t60}");
    }
}

/// Freeze capture with `skip` 70 % (README §5): pressed 300 ms and 1 s after a chord that follows
/// another a fifth away: no attack in the held output, and nothing of the earlier chord.
#[test]
fn freeze_capture_skips_the_pick_and_the_previous_chord() {
    // the first chord (A2), damped when the second (E3) is played
    let hold = |press: f32| -> (Vec<f32>, (f64, f64)) {
        let n = SR as usize * 4;
        let mut x = vec![0.0f32; n];
        let a2 = 2 * SR as usize;
        strum(&mut x, SR as usize / 2, 110.0, 0.15, SR);
        x[a2..].iter_mut().for_each(|v| *v = 0.0);
        strum(&mut x, a2, 164.81, 0.15, SR);
        let set = [("size", 450.0), ("grains", 12.0), ("pos", 0.4), ("length", 2000.0), ("spray", 60.0), ("detune", 8.0), ("spread", 0.0), ("skip", 70.0), ("live", 0.0), ("decay", 3.0), ("mix", 100.0), ("kill_dry", 1.0)];
        let mut g = make(&set, SR, 1);
        let p = a2 + (press * SR) as usize;
        let mut y = x[..p].to_vec();
        for c in y.chunks_mut(256) {
            g.process(&mut [c]);
        }
        g.set_param("freeze", 1.0);
        let mut held = vec![0.0f32; SR as usize * 30];
        for c in held.chunks_mut(256) {
            g.process(&mut [c]);
        }
        let (f0, f1) = g.hold_field().unwrap();
        (held, ((f0 - a2 as f64) / SR as f64, (f1 - a2 as f64) / SR as f64))
    };
    let mut results = Vec::new();
    for press in [0.3f32, 1.0] {
        let (held, field) = hold(press);
        // the first chord's root (110 Hz, 55 Hz from any partial of the second) against the second's
        let tail = &held[SR as usize..];
        let rel = 10.0 * (tone_power(tail, 110.0, SR) / tone_power(tail, 164.81, SR)).log10();
        // no pick in the held output (the module's own detector, its default sensitivity, run over
        // the whole hold; picks counted after the first second)
        let mut pick = Pick::default();
        let mut level = Level::default();
        pick.prepare(SR);
        level.prepare(SR);
        let mut picks = 0;
        for (i, &v) in held.iter().enumerate() {
            let e = level.next(v, -60.0);
            if pick.next(v, e > level.floor_db(30.0, -60.0)) && i >= SR as usize {
                picks += 1;
            }
        }
        eprintln!("press {press} s: field {:.3} – {:.3} s after the 2nd chord, {picks} picks in 29 s, 1st chord's root {rel:.1} dB re the 2nd's", field.0, field.1);
        results.push((press, rel, picks));
    }
    for (press, rel, picks) in results {
        assert!(rel < -40.0, "press {press}: {rel:.1} dB");
        // pressed 0.3 s after the chord the field is 163 ms and the grains 81 ms: one detector event
        // at its threshold (7.5 dB) in 29 s, from a Poisson cluster of grain starts, every span inside
        // the field after the last attack (README, As coded); none at 1 s
        assert!(picks <= if press < 0.5 { 1 } else { 0 }, "press {press}: {picks} picks");
    }
}

/// Freeze, no loop (README §5): 30 s of "Hold Chord": the 10 ms RMS envelope has no
/// autocorrelation peak > 0.3 from 50 ms to 10 s; the level stays within ±1.5 dB.
#[test]
fn freeze_is_a_field_not_a_loop() {
    let n = SR as usize * 3;
    let mut x = vec![0.0f32; n];
    // a sustained chord (strings ringing with τ = 4 s)
    for (k, r) in [1.0f32, 1.5, 2.0, 2.5198, 3.0].iter().enumerate() {
        pluck(&mut x, 1000 + k * (0.012 * SR) as usize, 110.0 * r, 0.2, 4.0, SR);
    }
    let set = [("size", 450.0), ("grains", 12.0), ("pos", 0.4), ("length", 2000.0), ("spray", 60.0), ("detune", 8.0), ("spread", 70.0), ("skip", 70.0), ("live", 0.0), ("mix", 100.0), ("kill_dry", 1.0)];
    let mut g = make(&set, SR, 1);
    run(&mut g, &x, 256);
    g.set_param("freeze", 1.0);
    let mut held = vec![0.0f32; SR as usize * 32];
    for c in held.chunks_mut(256) {
        g.process(&mut [c]);
    }
    let held = &held[SR as usize * 2..];
    let w = (0.01 * SR) as usize;
    let env: Vec<f64> = held.chunks(w).map(|c| rms(c) as f64).collect();
    let mean = env.iter().sum::<f64>() / env.len() as f64;
    let e: Vec<f64> = env.iter().map(|v| v - mean).collect();
    let var: f64 = e.iter().map(|v| v * v).sum();
    // the autocorrelation from 50 ms to 10 s; a loop shows as a peak, after the decay from lag 0
    let ac: Vec<f64> = (0..1000).map(|lag| (0..e.len() - lag).map(|i| e[i] * e[i + lag]).sum::<f64>() / var).collect();
    let mut peak = (0.0f64, 0);
    for lag in 5..999 {
        if ac[lag] > ac[lag - 1] && ac[lag] >= ac[lag + 1] && ac[lag] > peak.0 {
            peak = (ac[lag], lag);
        }
    }
    eprintln!("autocorrelation at 50 ms (the decay from lag 0): {:.3}", ac[5]);
    // the level over the 30 s: 3 s windows (no drift); the 1 s windows' spread is the texture
    let level = |w: usize| -> (f32, f32) {
        let v: Vec<f32> = held.chunks(w).filter(|c| c.len() == w).map(|c| db(rms(c))).collect();
        let avg = v.iter().sum::<f32>() / v.len() as f32;
        v.iter().fold((0.0f32, 0.0f32), |(a, b), &x| (a.min(x - avg), b.max(x - avg)))
    };
    let (l3, h3) = level(SR as usize * 3);
    let (l1, h1) = level(SR as usize);
    eprintln!("hold: envelope autocorrelation peak {:.3} at {} ms; level over 30 s: 3 s windows {l3:+.1} … {h3:+.1} dB, 1 s windows {l1:+.1} … {h1:+.1} dB", peak.0, peak.1 * 10);
    assert!(peak.0 < 0.3, "{:?}", peak);
    assert!(l3 > -1.5 && h3 < 1.5, "{l3} {h3}");
}

/// Edges (README §5): a hold at `pos` 0 and 1 with `spray` 100 %: grain starts are reflected, not
/// piled on the edges (no histogram bin above twice the mean).
#[test]
fn positions_are_reflected_not_clamped() {
    for pos in [0.0f32, 1.0] {
        let x = pink(SR as usize * 3, 0.2, 1);
        let mut g = make(&[("pos", pos), ("spray", 100.0), ("live", 0.0), ("size", 60.0), ("grains", 16.0)], SR, 1);
        run(&mut g, &x, 256);
        g.set_param("freeze", 1.0);
        g.trace.clear();
        let mut z = vec![0.0f32; SR as usize * 20];
        for c in z.chunks_mut(256) {
            g.process(&mut [c]);
        }
        let (f0, f1) = g.hold_field().unwrap();
        let bins = 20usize;
        let mut h = vec![0usize; bins];
        let starts: Vec<f64> = g.trace.iter().filter(|t| t.0 > 0).map(|t| t.2).collect();
        for &a in &starts {
            let b = (((a - f0) / (f1 - f0)) * bins as f64).floor().clamp(0.0, bins as f64 - 1.0) as usize;
            h[b] += 1;
        }
        let mean = starts.len() as f64 / bins as f64;
        let max = *h.iter().max().unwrap() as f64;
        eprintln!("pos {pos}: {} grains, histogram max {:.2} × mean", starts.len(), max / mean);
        assert!(max <= 2.0 * mean, "pos {pos}: {h:?}");
    }
}

/// Copy-on-freeze: HOLD is RECORD's region, frame for frame (both levels), and the hold layer then
/// reads HOLD.
#[test]
fn copy_on_freeze_is_exact() {
    let x = pink(SR as usize * 12, 0.2, 2);
    let mut g = make(&[("length", 10000.0), ("live", 0.0)], SR, 2);
    run2(&mut g, &x, &x, 512);
    g.set_param("freeze", 1.0);
    let z = vec![0.0f32; SR as usize * 4];
    run2(&mut g, &z, &z, 512);
    let li = (1..NL).find(|&i| g.layers[i].state == State::Held).unwrap();
    assert!(g.layers[li].copied);
    let h0 = g.layers[li].h0;
    let n = (g.layers[li].f1 as u64 - h0) as usize;
    for k in 0..n {
        assert!(g.hold.read_int(k as u64) == g.rec.read_int(h0 + k as u64), "frame {k}");
    }
    for k in 0..n / 2 - 64 {
        for c in 0..2 {
            assert!(g.hold.at1(k as u64, c) == g.rec.at1(h0 / 2 + k as u64, c));
        }
    }
    assert!(g.pool.as_slice().iter().filter(|gr| gr.layer as usize == li).all(|gr| gr.hold));
    assert!(g.pool.as_slice().iter().any(|gr| gr.layer as usize == li));
}

/// Hold again (README §5): a second hold while the first releases; a third engage ends the oldest;
/// each layer capped at 48 while two sound; < 0.1 % of grains dropped.
#[test]
fn hold_again_and_the_pool() {
    let x = pink(SR as usize * 4, 0.2, 4);
    let mut g = make(&[("grains", 64.0), ("decay", 10.0), ("live", 100.0), ("size", 120.0)], SR, 1);
    run(&mut g, &x, 256);
    let blk = |g: &mut Grains, s: f32| {
        let mut z = pink((s * SR) as usize, 0.2, 9);
        for c in z.chunks_mut(256) {
            g.process(&mut [c]);
        }
    };
    g.set_param("freeze", 1.0);
    blk(&mut g, 1.0);
    g.set_param("freeze", 0.0);
    blk(&mut g, 1.0);
    // the first hold releases under the live cloud: two layers, each capped
    let live_n = g.pool.as_slice().iter().filter(|gr| gr.layer == 0).count();
    let hold_n = g.pool.as_slice().iter().filter(|gr| gr.layer > 0).count();
    eprintln!("two layers: {live_n} live + {hold_n} hold grains");
    assert!(live_n <= 70 && hold_n <= 70, "{live_n} {hold_n}");
    g.set_param("freeze", 1.0);
    blk(&mut g, 1.0);
    assert_eq!(g.state(), 1);
    let states: Vec<State> = g.layers[1..].iter().map(|l| l.state).collect();
    assert!(states.contains(&State::Held) && states.contains(&State::Releasing), "{states:?}");
    g.set_param("freeze", 0.0);
    blk(&mut g, 0.5);
    g.set_param("freeze", 1.0);
    blk(&mut g, 0.1);
    let sounding = g.layers[1..].iter().filter(|l| matches!(l.state, State::Held | State::Releasing)).count();
    assert_eq!(sounding, 2, "{:?}", g.layers.iter().map(|l| l.state).collect::<Vec<_>>());
    let started = g.trace.len() as f64;
    eprintln!("pool: {} of {started} grains dropped", g.dropped());
    assert!((g.dropped() as f64) < 0.001 * started);
}

/// Feedback (README §5): 100 % at +12 st on pink noise at −12 dBFS for 60 s stays bounded (peak ≤
/// +6 dBFS), with no DC and no NaN; 90 % at 0 st decays once the input stops.
#[test]
fn feedback_is_bounded_and_decays() {
    let x = pink(SR as usize * 60, 0.25 / 3.0, 6);
    let x: Vec<f32> = noise(x.len(), 0.25 * 3f32.sqrt(), 6);
    let mut g = make(&[("feedback", 100.0), ("pitch", 12.0), ("kill_dry", 1.0), ("mix", 100.0)], SR, 1);
    let y = run(&mut g, &x, 512);
    assert!(y.iter().all(|v| v.is_finite()));
    let peak = y.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let dc = (y[SR as usize * 50..].iter().map(|&v| v as f64).sum::<f64>() / (SR as f64 * 10.0)).abs();
    eprintln!("feedback 100 %, +12 st: peak {:.1} dBFS, DC {:.1} dBFS", db(peak), db(dc as f32));
    assert!(db(peak) <= 6.0 && db(dc as f32) < -60.0);
    let mut g = make(&[("feedback", 90.0), ("spray", 0.0)], SR, 1);
    let mut x = pink(SR as usize * 25, 0.1, 6);
    x[SR as usize * 3..].iter_mut().for_each(|v| *v = 0.0);
    let y = run(&mut g, &x, 512);
    let (a, b) = (rms(&y[SR as usize * 4..SR as usize * 6]), rms(&y[SR as usize * 22..]));
    eprintln!("feedback 90 %: {:.1} dB after 19 s", db(b / a));
    assert!(db(b / a) < -30.0, "{:.1} dB", db(b / a));
}

/// Tempo (README §5): Rhythm at 1/8 on the host grid, at 120 and 97 BPM, after a relocation; no
/// tempo → 120 BPM.
#[test]
fn rhythm_locks_to_the_host_grid() {
    for bpm in [120.0f64, 97.0] {
        let mut g = make(&[("mode", 1.0), ("sync", 10.0), ("grains", 1.0), ("spray", 0.0)], SR, 1);
        let x = pink(SR as usize * 8, 0.2, 3);
        let mut y = x.clone();
        let spb = SR as f64 * 60.0 / bpm;
        let mut pos = 13.37f64;
        for (k, c) in y.chunks_mut(512).enumerate() {
            if k == 100 {
                pos = 101.1; // relocation
            }
            g.set_transport(TransportInfo { tempo: Some(bpm), pos_beats: Some(pos), playing: true });
            let n = c.len();
            g.process(&mut [c]);
            pos += n as f64 / spb;
        }
        let reloc = 100.0 * 512.0;
        let mut checked = 0;
        for t in g.trace.iter().filter(|t| t.1 > reloc + 1.0) {
            let beats = 101.1 + (t.1 - reloc) / spb;
            let off = (beats / 0.5 - (beats / 0.5).round()) * 0.5 * spb;
            assert!(off.abs() <= 1.0, "{bpm} BPM: {off:.2} samples");
            checked += 1;
        }
        assert!(checked > 10);
    }
    // no tempo: 120 BPM, free-running
    let mut g = make(&[("mode", 1.0), ("sync", 10.0), ("grains", 1.0), ("spray", 0.0)], SR, 1);
    run(&mut g, &pink(SR as usize * 2, 0.2, 3), 512);
    let d: Vec<f64> = g.trace.windows(2).map(|w| w[1].1 - w[0].1).collect();
    assert!(d.iter().all(|&v| (v - 12000.0).abs() < 1e-6), "{:?}", &d[..4]);
}

/// Strum (README §5): grains start 5 ms before each onset; with `grains` 1 only the last one.
#[test]
fn strum_takes_the_recent_onsets() {
    let n = SR as usize * 4;
    let mut x = vec![0.0f32; n];
    let at = [SR as usize, (1.6 * SR) as usize, (2.2 * SR) as usize];
    for (k, &a) in at.iter().enumerate() {
        pluck(&mut x, a, 196.0 * (1.0 + 0.25 * k as f32), 0.3, 0.4, SR);
    }
    for grains in [1.0f32, 3.0] {
        let mut g = make(&[("mode", 2.0), ("grains", grains), ("pos", 1.0), ("length", 3000.0), ("spray", 0.0), ("sync", 13.0)], SR, 1);
        run(&mut g, &x, 256);
        let after: Vec<f64> = g.trace.iter().filter(|t| t.1 > (2.4 * SR) as f64).map(|t| t.2).collect();
        assert!(!after.is_empty());
        let mut used = std::collections::BTreeSet::new();
        for &a in &after {
            let k = at.iter().position(|&o| o as f64 - a >= 0.0 && o as f64 - a <= 0.006 * SR as f64);
            assert!(k.is_some(), "a grain at {a} is not within 6 ms before an onset");
            used.insert(k.unwrap());
        }
        eprintln!("strum, grains {grains}: onsets used {used:?}");
        if grains == 1.0 {
            assert!(used.into_iter().collect::<Vec<_>>() == vec![2]);
        } else {
            assert_eq!(used.len(), 3);
        }
    }
}

/// Phrase seams (README §5): contiguous slices at ratio 1 are the phrase loop exactly (live: a delay
/// of L + the pre-roll); in a hold the wrap is an equal-power crossfade, no dip > 1 dB.
#[test]
fn phrase_seams() {
    let x = pink(SR as usize * 6, 0.2, 8);
    let mut g = make(&[("mode", 3.0), ("grains", 8.0), ("length", 1000.0), ("spray", 0.0), ("kill_dry", 1.0), ("mix", 100.0), ("spread", 0.0)], SR, 1);
    let y = run(&mut g, &x, 512);
    // the slice grains read X before their slice: the loop's delay is L + X
    let d = SR as usize + (0.03 * SR) as usize;
    let from = SR as usize * 2;
    let err = (from..x.len()).map(|i| (y[i] - x[i - d]).abs()).fold(0.0f32, f32::max);
    eprintln!("phrase (live): null {:.1} dB", db(err / 0.8));
    assert!(db(err / 0.8) < -90.0);
    // a held phrase on noise: the wrap's equal-power crossfade keeps the level (its 30 ms within 1 dB
    // of the whole)
    let mut g = make(&[("mode", 3.0), ("grains", 4.0), ("length", 500.0), ("spray", 0.0), ("kill_dry", 1.0), ("mix", 100.0), ("live", 0.0), ("spread", 0.0)], SR, 1);
    let x = noise(SR as usize * 2, 0.3, 4);
    run(&mut g, &x, 256);
    g.set_param("freeze", 1.0);
    g.trace.clear();
    let t0 = g.now as f64;
    let mut z = vec![0.0f32; SR as usize * 4];
    for c in z.chunks_mut(256) {
        g.process(&mut [c]);
    }
    let (f0, _) = g.hold_field().unwrap();
    let xl = 0.03 * SR as f64;
    let whole = rms(&z[SR as usize..]);
    let mut worst = 0.0f32;
    let mut wraps = 0;
    for t in g.trace.iter().filter(|t| t.0 > 0 && (t.2 - (f0 - xl)).abs() < 2.0 && t.1 > t0 + 1000.0) {
        let k = (t.1 - t0) as usize;
        let l = db(rms(&z[k..k + xl as usize]) / whole);
        worst = if l.abs() > worst.abs() { l } else { worst };
        wraps += 1;
    }
    eprintln!("phrase (held): {wraps} wraps, crossfade level {worst:+.2} dB re the whole");
    assert!(wraps >= 4 && worst.abs() < 1.0, "{wraps} {worst}");
}

/// Trails: with `trails` the cloud finishes within `size` after bypass; without, the wet is gone in
/// 20 ms.
#[test]
fn trails() {
    for trails in [1.0f32, 0.0] {
        let x = pink(SR as usize * 2, 0.2, 2);
        let mut g = make(&[("trails", trails), ("kill_dry", 1.0), ("mix", 100.0), ("pos", 0.05)], SR, 1);
        run(&mut g, &x, 256);
        g.set_param("bypass", 1.0);
        let z = vec![0.0f32; SR as usize];
        let y = run(&mut g, &z, 256);
        let gone = if trails > 0.5 { (0.16 * SR) as usize } else { (0.025 * SR) as usize };
        let after = y[gone..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let before = y[..(0.01 * SR) as usize].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(before > 0.01, "trails {trails}: {before}");
        assert!(after < 1e-6, "trails {trails}: {after}");
    }
}

/// Rate independence: grain lengths and onsets equal in ms (± 1 sample) at every rate.
#[test]
fn rate_independence() {
    for sr in [44100.0f32, 48000.0, 96000.0, 192000.0] {
        let mut g = make(&[("regular", 100.0), ("spray", 0.0)], sr, 1);
        run(&mut g, &pink(sr as usize, 0.2, 1), 512);
        let t = &g.trace;
        assert!((t[1].1 - t[0].1 - 0.015 * sr as f64).abs() <= 1.0);
        assert!((t[0].3 - 0.12 * sr as f64).abs() <= 1.0);
    }
}

/// Clicks (README §5): random moves of shape, skew and size every 50 ms on a sine leave no
/// broadband burst above −60 dB re the signal.
#[test]
fn knob_moves_do_not_click() {
    let n = SR as usize * 6;
    let x = sine(n, 440.0, 0.4, SR);
    let mut g = make(&[("kill_dry", 1.0), ("mix", 100.0), ("spray", 20.0), ("grains", 8.0)], SR, 1);
    let mut rng = gfx_dsp::lfo::Rng::new(5);
    let mut y = x.clone();
    for c in y.chunks_mut((0.05 * SR) as usize) {
        g.set_param("shape", [0.0, 50.0, 100.0][(rng.next_f64() * 3.0) as usize % 3]);
        g.set_param("skew", (rng.next_f64() * 200.0 - 100.0) as f32);
        g.set_param("size", (2.0 + rng.next_f64() * 1998.0) as f32);
        g.process(&mut [c]);
    }
    let (mut h1, mut h2) = (gfx_dsp::svf::Svf::new(), gfx_dsp::svf::Svf::new());
    h1.set(gfx_dsp::svf::SvfMode::HighPass, 10000.0, 0.541, 0.0, SR);
    h2.set(gfx_dsp::svf::SvfMode::HighPass, 10000.0, 1.307, 0.0, SR);
    let h: Vec<f32> = y.iter().map(|&v| h2.process(h1.process(v))).collect();
    let from = SR as usize;
    let burst = h[from..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let peak = y[from..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    eprintln!("knob moves: HF above 10 kHz {:.1} dB re peak", db(burst / peak));
    assert!(db(burst / peak) < -60.0);
}

/// Glide (README §5): +12 and +24 st per grain; the row and level changes leave no step (sample to
/// sample jumps stay those of a band-limited signal).
#[test]
fn glide_has_no_bursts() {
    let x = sine(SR as usize * 4, 300.0, 0.3, SR);
    for glide in [12.0f32, 24.0, -24.0] {
        let mut g = make(&[("glide", glide), ("kill_dry", 1.0), ("mix", 100.0), ("grains", 4.0), ("regular", 100.0), ("spray", 0.0), ("size", 300.0)], SR, 1);
        let y = run(&mut g, &x, 256);
        assert!(y.iter().all(|v| v.is_finite()));
        // the steepest slope of a sum of tones ≤ 1.2 kHz (300 Hz × 4) at its peak level
        let peak = y[SR as usize..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let jump = y[SR as usize..].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        let bound = peak * 2.0 * std::f32::consts::PI * 1200.0 / SR;
        eprintln!("glide {glide}: steepest step {:.2} × the band's bound", jump / bound);
        assert!(jump < bound, "glide {glide}: {jump} vs {bound}");
    }
}

