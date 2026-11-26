// SPDX-License-Identifier: GPL-3.0-or-later
//! The pitch shifter's own bench suite (README.md §6).

use super::*;

const SR: f32 = 48000.0;

fn ps(s: Settings) -> Pitch {
    let mut q = Pitch::new();
    q.prepare(SR, 4800, 1);
    q.set(s);
    q.reset();
    q
}

fn run(q: &mut Pitch, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for c in y.chunks_mut(256) {
        q.process(&mut [c]);
    }
    y
}

fn freq_of(y: &[f32]) -> f64 {
    let mut t = Vec::new();
    for i in 1..y.len() {
        if y[i - 1] < 0.0 && y[i] >= 0.0 {
            t.push((i - 1) as f64 + (-y[i - 1] / (y[i] - y[i - 1])) as f64);
        }
    }
    (t.len() - 1) as f64 / (t[t.len() - 1] - t[0]) * SR as f64
}

fn sine(f: f32, n: usize) -> Vec<f32> {
    (0..n).map(|i| 0.3 * (std::f32::consts::TAU * f * i as f32 / SR).sin()).collect()
}

fn probe(v: &[f32], f: f64) -> f64 {
    let n = v.len();
    let w = std::f64::consts::TAU * f / SR as f64;
    let (mut re, mut im, mut ws) = (0.0, 0.0, 0.0);
    for (k, s) in v.iter().enumerate() {
        let h = 0.5 - 0.5 * (std::f64::consts::TAU * k as f64 / n as f64).cos();
        re += *s as f64 * h * (w * k as f64).cos();
        im += *s as f64 * h * (w * k as f64).sin();
        ws += h;
    }
    2.0 * (re * re + im * im).sqrt() / ws
}

#[test]
fn whammy_is_in_tune_at_every_integer_shift() {
    // within 0.2 cents over the whole range (the target was ±1; before the splice-order fix the error
    // grew with the shift, +1.5 cents at +12 and +6.8 at +24)
    for f in [82.41f32, 220.0, 880.0] {
        for st in [-24, -12, -7, -5, -2, -1, 1, 2, 4, 5, 7, 12, 19, 24] {
            let mut q = ps(Settings::default().with(p::SHIFT, st as f32).with(p::MIX, 100.0));
            let y = run(&mut q, &sine(f, 96000));
            let fo = freq_of(&y[24000..]);
            let off = 1200.0 * (fo / f as f64).log2() - 100.0 * st as f64;
            assert!(off.abs() < 0.2, "{f} Hz, {st} st: {off:+.3} cents");
        }
    }
}

#[test]
fn latency_is_the_engines() {
    // Whammy: its 12 ms centre; Poly: the vocoder's synthesis window (2m − 1: Fast m 160, Stable 240)
    for (e, t, n) in [(0.0, 0.0, 576), (1.0, 0.0, 319), (1.0, 1.0, 479)] {
        let q = ps(Settings::default().with(p::ENGINE, e).with(p::TRACKING, t));
        assert_eq!(q.latency(), n);
    }
}

#[test]
fn whammy_sweep_is_continuous() {
    // shift 0 → +12 in 300 ms on a held note: no splice click (no sample-to-sample jump beyond the sine's own)
    let mut q = ps(Settings::default().with(p::SHIFT, 0.0).with(p::MIX, 100.0).with(p::GLIDE, 300.0));
    let x = sine(220.0, 96000);
    let mut y = x.clone();
    let (a, b) = y.split_at_mut(24000);
    for c in a.chunks_mut(256) {
        q.process(&mut [c]);
    }
    q.set(q.settings().with(p::SHIFT, 12.0));
    for c in b.chunks_mut(256) {
        q.process(&mut [c]);
    }
    // the largest step a 0.3 sine at 440 Hz can take is 0.3 · 2π · 440 / 48000 ≈ 0.017
    let jump = y[1000..].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    assert!(jump < 0.03, "biggest step {jump}");
}

#[test]
fn poly_shifts_a_chord() {
    let mut q = ps(Settings::default().with(p::ENGINE, 1.0).with(p::SHIFT, 12.0).with(p::MIX, 100.0));
    let x: Vec<f32> = sine(110.0, 96000).iter().zip(sine(164.8, 96000)).zip(sine(329.6, 96000)).map(|((a, b), c)| (a + b + c) / 3.0).collect();
    let y = run(&mut q, &x);
    let seg = &y[48000..];
    // each partial on its target at nearly its level (the vocoder: each peak moves its own region; the
    // band-split it replaced kept −11 to −20 dB on average over ten chords, poly_metric)
    let mut mean = 0.0;
    for f in [220.0, 329.6, 659.2] {
        let up = probe(seg, f);
        let db = 20.0 * (up / 0.1).log10();
        eprintln!("{f} Hz: {db:+.1} dB");
        assert!(db > -4.0, "{f} Hz: {db:.1} dB");
        mean += db / 3.0;
    }
    assert!(mean > -2.5, "mean {mean:.1} dB");
    // the originals are gone (−20 dB at least)
    let orig = probe(seg, 110.0);
    assert!(orig < 0.1 * probe(seg, 220.0), "110 Hz left: {orig:.3}");
}

#[test]
#[ignore]
fn activation_cost() {
    use std::time::Instant;
    let mut q = Pitch::new();
    let t = Instant::now();
    q.prepare(SR, 512, 2);
    eprintln!("prepare  {:8.3} ms", t.elapsed().as_secs_f64() * 1e3);
    q.set(Settings::default());
    let t = Instant::now();
    q.reset();
    eprintln!("reset    {:8.3} ms", t.elapsed().as_secs_f64() * 1e3);
    let mut l = sine(220.0, 512);
    let mut r = l.clone();
    for k in 0..6 {
        let t = Instant::now();
        q.process(&mut [&mut l[..], &mut r[..]]);
        eprintln!("block {k}  {:8.3} ms", t.elapsed().as_secs_f64() * 1e3);
    }
    // the worst block over 10 s
    let mut worst = 0.0f64;
    for _ in 0..940 {
        let t = Instant::now();
        q.process(&mut [&mut l[..], &mut r[..]]);
        worst = worst.max(t.elapsed().as_secs_f64() * 1e3);
    }
    eprintln!("worst block over 10 s {:8.3} ms (512 samples = 10.7 ms of budget)", worst);
}

/// Harmony (phase 2): the wet follows the scale above the detected note.
#[test]
fn harmony_keeps_thirds_in_key() {
    let midi = |m: f32| 440.0 * ((m - 69.0) / 12.0).exp2();
    // C major, a third above, wet only: D4 → F4 (3 st), E4 → G4 (3), F4 → A4 (4), G4 → B4 (4)
    let s = Settings::default()
        .with(p::SHIFT, 4.0)
        .with(p::MIX, 100.0)
        .with(p::HARMONY, 1.0)
        .with(p::KEY, 0.0)
        .with(p::SCALE, 0.0);
    for (played, expect) in [(62.0, 65.0), (64.0, 67.0), (65.0, 69.0), (67.0, 71.0)] {
        let mut q = ps(s);
        let y = run(&mut q, &sine(midi(played), 24000));
        let f = freq_of(&y[12000..]);
        let cents = 1200.0 * (f / midi(expect) as f64).log2();
        assert!(cents.abs() < 5.0, "played {played}: {f:.2} Hz, {cents:+.1} cents from the expected {expect}");
    }
}

#[test]
fn harmony_follows_a_note_change() {
    let midi = |m: f32| 440.0 * ((m - 69.0) / 12.0).exp2();
    // A minor, a third above: A3 then C4. A → C (+3), C → E (+4)
    let s = Settings::default()
        .with(p::SHIFT, 3.0)
        .with(p::MIX, 100.0)
        .with(p::HARMONY, 1.0)
        .with(p::KEY, 9.0)
        .with(p::SCALE, 1.0);
    let mut q = ps(s);
    let mut x = sine(midi(57.0), 19200);
    // phase-continuous change of note
    let ph0 = std::f32::consts::TAU * midi(57.0) * 19200.0 / SR;
    x.extend((0..19200).map(|i| 0.3 * (ph0 + std::f32::consts::TAU * midi(60.0) * i as f32 / SR).sin()));
    let y = run(&mut q, &x);
    let f1 = freq_of(&y[9600..19200]);
    let f2 = freq_of(&y[28800..]);
    assert!((1200.0 * (f1 / midi(60.0) as f64).log2()).abs() < 5.0, "A3 → {f1:.2} Hz");
    assert!((1200.0 * (f2 / midi(64.0) as f64).log2()).abs() < 5.0, "C4 → {f2:.2} Hz");
}

#[test]
fn harmony_off_keeps_the_fixed_interval() {
    let midi = |m: f32| 440.0 * ((m - 69.0) / 12.0).exp2();
    // Off: +4 on D stays a major third (F#), whatever the key
    let s = Settings::default().with(p::SHIFT, 4.0).with(p::MIX, 100.0).with(p::KEY, 0.0);
    let mut q = ps(s);
    let y = run(&mut q, &sine(midi(62.0), 24000));
    let f = freq_of(&y[12000..]);
    assert!((1200.0 * (f / midi(66.0) as f64).log2()).abs() < 5.0, "{f:.2} Hz");
}

/// The tuning table (ignored: a diagnostic). Cents off at each frequency and shift, Whammy, wet only.
#[test]
#[ignore]
fn tuning_table() {
    for f in [82.41f32, 110.0, 220.0, 440.0, 880.0] {
        let mut row = format!("{f:7.2} Hz:");
        for st in [-12, -7, -5, -3, -1, 1, 3, 4, 5, 7, 12, 19, 24] {
            let mut q = ps(Settings::default().with(p::SHIFT, st as f32).with(p::MIX, 100.0));
            let y = run(&mut q, &sine(f, 144000));
            let fo = freq_of(&y[24000..]);
            row += &format!(" {:+5.2}", 1200.0 * (fo / f as f64).log2() - 100.0 * st as f64);
        }
        eprintln!("{row}");
    }
}

/// Poly tuning per band (ignored: a diagnostic).
#[test]
#[ignore]
fn poly_tuning_table() {
    for t in [0.0, 1.0] {
        for f in [82.41f32, 146.8, 220.0, 440.0, 1046.5, 2093.0] {
            let mut row = format!("tracking {t} {f:7.2} Hz:");
            for st in [-12, -5, 4, 7, 12] {
                let mut q = ps(Settings::default().with(p::ENGINE, 1.0).with(p::TRACKING, t).with(p::SHIFT, st as f32).with(p::MIX, 100.0));
                let y = run(&mut q, &sine(f, 144000));
                let fo = freq_of(&y[24000..]);
                row += &format!(" {:+5.2}", 1200.0 * (fo / f as f64).log2() - 100.0 * st as f64);
            }
            eprintln!("{row}");
        }
    }
}

/// At unison every band splices from the same positions: the chord comes out whole (before the all-pass
/// compensation and the shared grain timing, two of its notes were 14 – 17 dB down).
#[test]
fn poly_is_transparent_at_unison() {
    for st in [0.0f32] {
        let mut q = ps(Settings::default().with(p::ENGINE, 1.0).with(p::SHIFT, st).with(p::MIX, 100.0));
        let x: Vec<f32> = sine(110.0, 96000).iter().zip(sine(164.8, 96000)).zip(sine(329.6, 96000)).map(|((a, b), c)| (a + b + c) / 3.0).collect();
        let y = run(&mut q, &x);
        let seg = &y[48000..];
        let k = (st / 12.0).exp2();
        for f in [110.0, 164.8, 329.6] {
            let db = 20.0 * (probe(seg, f * k as f64) / probe(&x[48000..], f)).log10();
            assert!(db.abs() < 0.5, "{f} Hz: {db:+.2} dB");
        }
    }
}

#[test]
fn poly_at_unison_is_the_delayed_input() {
    // the vocoder's phase advance is scaled from peak to peak: at ratio 1 it hands back the input itself,
    // so Poly at shift 0 mixes with the dry without combing (noise: every bin, every kind of peak)
    for t in [0.0f32, 1.0] {
        let mut q = ps(Settings::default().with(p::ENGINE, 1.0).with(p::TRACKING, t).with(p::SHIFT, 0.0).with(p::MIX, 100.0));
        let lat = q.latency_samples();
        let mut rng = gfx_dsp::lfo::Rng::new(9);
        let x: Vec<f32> = (0..48000).map(|_| 0.5 * rng.bipolar()).collect();
        let y = run(&mut q, &x);
        let err = (24000..48000).map(|i| (y[i] - x[i - lat]).abs()).fold(0.0f32, f32::max);
        assert!(err < 1e-3, "tracking {t}: {err}");
    }
}

#[test]
#[ignore]
fn poly_metric() {
    let midi = |m: f32| 440.0 * ((m - 69.0) / 12.0).exp2();
    // power chords, triads and open chords over the guitar's range
    let chords: [&[f32]; 10] = [
        &[40.0, 47.0, 52.0], &[45.0, 52.0, 57.0], &[50.0, 57.0, 62.0], &[43.0, 47.0, 50.0, 55.0],
        &[48.0, 52.0, 55.0, 60.0], &[52.0, 56.0, 59.0, 64.0], &[57.0, 60.0, 64.0], &[60.0, 64.0, 67.0, 72.0],
        &[64.0, 67.0, 71.0], &[69.0, 72.0, 76.0, 81.0],
    ];
    for tr in [0.0, 1.0] {
        for st in [-12.0f32, -5.0, 4.0, 7.0, 12.0] {
            let (mut sum, mut n, mut worst) = (0.0, 0, 0.0f64);
            for ch in chords.iter() {
                let mut q = ps(Settings::default().with(p::ENGINE, 1.0).with(p::TRACKING, tr).with(p::SHIFT, st).with(p::MIX, 100.0));
                let mut x = vec![0.0f32; 96000];
                for &m in ch.iter() {
                    for (i, v) in sine(midi(m), 96000).iter().enumerate() {
                        x[i] += v / ch.len() as f32;
                    }
                }
                let y = run(&mut q, &x);
                let k = (st as f64 / 12.0).exp2();
                for &m in ch.iter() {
                    let f = midi(m) as f64;
                    let db = 20.0 * (probe(&y[48000..], f * k) / probe(&x[48000..], f)).log10();
                    sum += db;
                    n += 1;
                    worst = worst.min(db);
                }
            }
            eprintln!("tracking {tr} shift {st:+5}: mean {:+6.1} dB, worst {:+6.1} dB", sum / n as f64, worst);
        }
    }
}

/// Six harmonics (1 / h) at `f`, or a chord of such tones.
fn rich(fs: &[f32], n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            fs.iter().map(|f| (1..=6).map(|h| (std::f32::consts::TAU * f * h as f32 * t).sin() / h as f32).sum::<f32>()).sum::<f32>() * 0.15 / fs.len() as f32
        })
        .collect()
}

/// `shift` 0 for 1 s, `bend` st for 1 s, 0 again (a bend through `shift` itself), `mix` 100 %. Over the last
/// second (2.5 – 3.5 s after the return): (the wet against the input delayed by the latency, dB; the wet's level
/// against it, dB; ms after the return until the wet stays within 1e-5 of the delayed input).
fn after_a_bend(engine: f32, x: &[f32], bend: f32) -> (f64, f64, f64) {
    let mut q = ps(Settings::default().with(p::ENGINE, engine).with(p::SHIFT, 0.0).with(p::MIX, 100.0));
    let lat = q.latency_samples();
    let mut y = x.to_vec();
    for (k, c) in y.chunks_mut(256).enumerate() {
        let t = k * 256;
        let st = if (48000..96000).contains(&t) { bend } else { 0.0 };
        if st != q.settings().v[p::SHIFT] {
            q.set(q.settings().with(p::SHIFT, st));
        }
        q.process(&mut [c]);
    }
    let n = x.len();
    let (mut e, mut r, mut w) = (0.0f64, 0.0f64, 0.0f64);
    for i in n - 48000..n {
        let d = x[i - lat] as f64;
        e += (y[i] as f64 - d).powi(2);
        r += d * d;
        w += (y[i] as f64).powi(2);
    }
    let last = (96000..n).rev().find(|&i| (y[i] - x[i - lat]).abs() > 1e-5).unwrap_or(96000);
    (10.0 * (e / r).log10(), 10.0 * (w / r).log10(), (last - 96000) as f64 / 48.0)
}

/// The rest after a bend (README §2.1, the review's finding 1): once the shift is back at 0, the wet is the
/// delayed input again — the Whammy's heads glide back to their centre (gfx-dsp `CenteredShifter::set_recentre`),
/// the vocoder's phases decay back to the analysis' — so `mix` at `shift` 0 does not comb and Poly keeps its
/// level. Before the fix (v0.1.0) the same measurement gave no null at all (+4.9 dB at worst) and Poly
/// down to −4.2 dB (−2.0 to −3.1 dB on the chords).
#[test]
fn the_wet_returns_to_the_delayed_input_after_a_bend() {
    let sigs: [(&str, &[f32]); 5] = [("82", &[82.41]), ("196", &[196.0]), ("660", &[659.3]), ("E5", &[82.41, 123.47, 164.81]), ("Am", &[110.0, 164.81, 220.0, 261.63, 329.63])];
    for engine in [0.0f32, 1.0] {
        let mut worst = (f64::MIN, 0.0f64, 0.0f64);
        for (name, fs) in sigs {
            let x = rich(fs, 264000);
            for bend in [12.0f32, -12.0, 2.0, 24.0, -5.0] {
                let (null, level, settle) = after_a_bend(engine, &x, bend);
                eprintln!("engine {engine} {name:>4} {bend:+5}: null {null:+7.1} dB, level {level:+6.2} dB, settled {settle:6.0} ms after the return");
                worst.0 = worst.0.max(null);
                worst.1 = if level.abs() > worst.1.abs() { level } else { worst.1 };
                worst.2 = worst.2.max(settle);
            }
        }
        eprintln!("engine {engine}: worst null {:+.1} dB, worst level {:+.3} dB, slowest {:.0} ms", worst.0, worst.1, worst.2);
        assert!(worst.0 < -90.0 && worst.1.abs() < 0.01, "engine {engine}: {worst:?}");
    }
}
