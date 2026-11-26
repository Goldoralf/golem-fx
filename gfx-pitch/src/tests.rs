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

// ---- Bend unused (README §6): the reference renders, taken before the triggered bend touched lib.rs ----

/// The factory presets of `presets/gfx-pitch.json` as they stood before the bend (enums as indices), then
/// the extra cases of the bench row: shift 0, a shift sweep through 0 and a `reset()` mid-render, each on
/// both engines with harmony Off and Scale.
const REF_CASES: [(&str, &str, u8); 24] = [
    ("octave-up-bends", "engine=0,shift=12,mix=100,glide=0,fine=0,harmony=0,voice2=-60", 0),
    ("two-octaves-up-solo", "engine=0,shift=24,mix=100,glide=300,fine=0,harmony=0,voice2=-60", 0),
    ("octave-down-riff", "engine=0,shift=-12,mix=100,glide=0,harmony=0,voice2=-60", 0),
    ("two-octave-dive", "engine=0,shift=-24,mix=100,glide=600,harmony=0,voice2=-60", 0),
    ("octave-up-squeal-riff", "engine=0,shift=12,mix=100,glide=0,harmony=0,voice2=-60", 0),
    ("octave-down-thickener", "engine=0,shift=-12,mix=40,glide=0,harmony=0,voice2=-60", 0),
    ("doom-octave-chords", "engine=1,tracking=1,shift=-12,mix=50,harmony=0,voice2=-60", 0),
    ("drop-half-step", "engine=1,tracking=0,shift=-1,mix=100,fine=0,harmony=0,voice2=-60", 0),
    ("drop-whole-step", "engine=1,tracking=0,shift=-2,mix=100,harmony=0,voice2=-60", 0),
    ("baritone-drop", "engine=1,tracking=1,shift=-5,mix=100,harmony=0,voice2=-60", 0),
    ("twelve-string", "engine=1,tracking=0,shift=12,mix=35,fine=5,harmony=0,voice2=-60", 0),
    ("power-fifth", "engine=0,shift=7,mix=45,harmony=0,voice2=-60", 0),
    ("twin-lead-third", "engine=0,harmony=1,key=4,scale=1,shift=4,mix=50,voice2=-60", 0),
    ("three-voice-harmony", "engine=0,harmony=1,key=4,scale=1,shift=4,shift2=7,voice2=-4,mix=55", 0),
    ("stacked-octaves", "engine=1,tracking=1,shift=-12,shift2=12,voice2=-6,mix=50,harmony=0", 0),
    ("bass-octave-up", "engine=0,shift=12,mix=60,glide=0,harmony=0,voice2=-60", 0),
    ("whammy-shift0-reset", "engine=0,shift=0,mix=50", 1),
    ("whammy-sweep", "engine=0,shift=7,mix=50", 2),
    ("whammy-scale-sweep", "engine=0,harmony=1,shift=7,shift2=4,voice2=-6,mix=50", 2),
    ("whammy-scale-reset", "engine=0,harmony=1,key=9,scale=1,shift=3,mix=100", 1),
    ("poly-shift0-reset", "engine=1,shift=0,mix=50", 1),
    ("poly-sweep", "engine=1,tracking=1,shift=7,shift2=-12,voice2=-3,mix=50", 2),
    ("poly-scale-sweep", "engine=1,harmony=1,shift=7,mix=50", 2),
    ("poly-scale-reset", "engine=1,harmony=1,key=4,scale=1,shift=4,mix=100", 1),
];

fn ref_settings(kv: &str) -> Settings {
    let mut s = Settings::default();
    for f in kv.split(',') {
        let (k, v) = f.split_once('=').unwrap();
        s = s.with(PARAMS.iter().position(|pi| pi.id == k).unwrap(), v.parse().unwrap());
    }
    s
}

fn fnv(y: &[Vec<f32>]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for v in y.iter().flatten() {
        h ^= v.to_bits() as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Renders a case in stereo blocks of 256; event 1 = `reset()` at 3/5 of the signal, 2 = `shift` to +12 at
/// 1/5, 0 at 2/5 (the heel, ratio 1), −5 at 3/5.
fn ref_render(kv: &str, ev: u8, x: &[f32]) -> u64 {
    let mut q = Pitch::new();
    q.prepare(SR, 4800, 2);
    q.set(ref_settings(kv));
    q.reset();
    let mut y = vec![x.to_vec(), x.to_vec()];
    let nb = x.len().div_ceil(256);
    for b in 0..nb {
        match (ev, b * 5 / nb, (b * 5) % nb < 5) {
            (1, 3, true) => q.reset(),
            (2, 1, true) => q.set(q.settings().with(p::SHIFT, 12.0)),
            (2, 2, true) => q.set(q.settings().with(p::SHIFT, 0.0)),
            (2, 3, true) => q.set(q.settings().with(p::SHIFT, -5.0)),
            _ => {}
        }
        let (l, r) = y.split_at_mut(1);
        let e = ((b + 1) * 256).min(x.len());
        q.process(&mut [&mut l[0][b * 256..e], &mut r[0][b * 256..e]]);
    }
    fnv(&y)
}

/// Synthetic material: a lead line (rich tones with vibrato and slides between notes) and a strummed chord
/// sequence, 2 s each.
fn ref_signals() -> [Vec<f32>; 2] {
    let n = 96000;
    let midi = |m: f64| 440.0 * ((m - 69.0) / 12.0).exp2();
    let (mut lead, mut ph) = (Vec::with_capacity(n), 0.0f64);
    let notes = [64.0, 67.0, 69.0, 71.0, 69.0, 74.0, 76.0, 72.0];
    for i in 0..n {
        let t = i as f64 / SR as f64;
        let k = (t * 4.0) as usize % notes.len();
        let f = midi(notes[k] + 0.3 * (std::f64::consts::TAU * 5.5 * t).sin());
        ph += std::f64::consts::TAU * f / SR as f64;
        let env = (-(t * 4.0).fract() * 3.0).exp();
        lead.push((0.3 * env * (1..=5).map(|h| (ph * h as f64).sin() / h as f64).sum::<f64>()) as f32);
    }
    let chords: [&[f64]; 4] = [&[40.0, 47.0, 52.0, 56.0, 59.0, 64.0], &[45.0, 52.0, 57.0, 60.0, 64.0], &[43.0, 47.0, 50.0, 55.0, 59.0, 67.0], &[50.0, 57.0, 62.0, 66.0]];
    let clean = (0..n)
        .map(|i| {
            let t = i as f64 / SR as f64;
            let c = chords[(t * 2.0) as usize % 4];
            let env = (-(t * 2.0).fract() * 2.0).exp();
            (0.08 * env * c.iter().map(|&m| (1..=4).map(|h| (std::f64::consts::TAU * midi(m) * h as f64 * t).sin() / h as f64).sum::<f64>()).sum::<f64>()) as f32
        })
        .collect();
    [lead, clean]
}

/// The hashes of v0.1.0 with the ratio-1 fix (commit bc34ffa), lib.rs before the triggered bend: (case, lead,
/// clean). Taken on x86-64 Linux; float results of libm (sin, exp2, powf) may differ on other systems, where
/// the test only prints.
const REF_HASHES: [(&str, u64, u64); 24] = [
    ("octave-up-bends", 0x99efd03054aa3115, 0x4cd1c63074b04a99),
    ("two-octaves-up-solo", 0x1a26da8324e87f1d, 0x4c01c44c065f4249),
    ("octave-down-riff", 0xf37516c7e2fa2e05, 0xc21c076c996e6f65),
    ("two-octave-dive", 0xa55c6549818e4545, 0x0f348779ca863ed1),
    ("octave-up-squeal-riff", 0x99efd03054aa3115, 0x4cd1c63074b04a99),
    ("octave-down-thickener", 0x664f4fa59663175d, 0x8e3fc12cdb3015bd),
    ("doom-octave-chords", 0xe3368f08f79b1f09, 0xf8a9227fba0e4195),
    ("drop-half-step", 0xbbf7cc82e89925f5, 0xcab7527231ee99fd),
    ("drop-whole-step", 0xba687dbae9443c41, 0x3fcfc643cd423541),
    ("baritone-drop", 0x015601651c3a549d, 0x829d66782315a559),
    ("twelve-string", 0x889f40573d5fe29d, 0x77817f44544ed135),
    ("power-fifth", 0x920e7cd1f41099d1, 0xd3d49f55d4c7ce85),
    ("twin-lead-third", 0x9b98728e9d0b6f65, 0xe3606a6faae517f5),
    ("three-voice-harmony", 0x90c42363336e3761, 0x581cd774ac6e2fbd),
    ("stacked-octaves", 0xc3e9dd62079967d5, 0xbd7fea067794a7f9),
    ("bass-octave-up", 0x623010e04c8f878d, 0xb869e85d4c396239),
    ("whammy-shift0-reset", 0x595ba88e694f2bfd, 0xf95501f80e9c4fc5),
    ("whammy-sweep", 0x24625785966ec3b1, 0x6f0434516dd08621),
    ("whammy-scale-sweep", 0x514610d8e4ffa991, 0x4880c5b538c44701),
    ("whammy-scale-reset", 0x93487dfaa2cacfa5, 0xc4af656e42728385),
    ("poly-shift0-reset", 0xf3a34304e3d436d1, 0xbd84a21b43148cbd),
    ("poly-sweep", 0x40ad8b37cf973285, 0x0ad207946b7e5871),
    ("poly-scale-sweep", 0x55f7d86959ed7af5, 0xe02b72dad8a05c25),
    ("poly-scale-reset", 0xce97c7e63e095225, 0xbb8d5aaa1cc9a6dd),
];

#[test]
fn bend_unused_is_bit_identical_to_the_reference() {
    let sig = ref_signals();
    let mut bad = Vec::new();
    for (name, kv, ev) in REF_CASES {
        let h = [ref_render(kv, ev, &sig[0]), ref_render(kv, ev, &sig[1])];
        eprintln!("    (\"{name}\", 0x{:016x}, 0x{:016x}),", h[0], h[1]);
        if let Some(r) = REF_HASHES.iter().find(|r| r.0 == name) {
            if (r.1, r.2) != (h[0], h[1]) {
                bad.push(name);
            }
        }
    }
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        assert_eq!(REF_HASHES.len(), REF_CASES.len());
        assert!(bad.is_empty(), "renders changed: {bad:?}");
    }
}

/// The same on golem's DIs (lead.wav, clean.wav, first 8 s), when they are on this machine (ignored: 8 s of
/// stereo per case).
/// The 16 presets' hashes here are also v0.1.0's own (before the ratio-1 fix: none sits at shift 0).
const REF_HASHES_DI: [(&str, u64, u64); 24] = [
    ("octave-up-bends", 0x4b1ed227fc77d965, 0xdbd30306f026fc51),
    ("two-octaves-up-solo", 0xdec598f5ec9c2aed, 0x158669eb2f2ca189),
    ("octave-down-riff", 0x9dbbfad2b8cad369, 0x1c23593544604375),
    ("two-octave-dive", 0x78fdda6e31102eb1, 0xce1d3df26e5cfde1),
    ("octave-up-squeal-riff", 0x4b1ed227fc77d965, 0xdbd30306f026fc51),
    ("octave-down-thickener", 0x5c179d5164e2f2a5, 0x135c96fb66dd90fd),
    ("doom-octave-chords", 0xed3363aae50d2005, 0x0d42ee0bd3ad61dd),
    ("drop-half-step", 0x2f60802baa50c21d, 0x6a07499cdf3e8f25),
    ("drop-whole-step", 0x1087fa8a0b9c6f71, 0xb9025257e59fb40d),
    ("baritone-drop", 0x2c344299c8ab5cad, 0xf61699d70f61dc2d),
    ("twelve-string", 0xda4597752f99d2c5, 0xd496312b0c8f07a1),
    ("power-fifth", 0xe980a807da146efd, 0x94800388baeac901),
    ("twin-lead-third", 0x43af5a8f2a62d635, 0xa6193bd240d6bd15),
    ("three-voice-harmony", 0x19c27487755d6201, 0xfb40d49691abea25),
    ("stacked-octaves", 0x99230887ec10e775, 0xe3c4f2811971a9f5),
    ("bass-octave-up", 0x8189b6247769e4bd, 0xc14b922e84d02bd1),
    ("whammy-shift0-reset", 0x508c6013e610ca9d, 0x235d51ad906ea019),
    ("whammy-sweep", 0xa5120735076f3cb9, 0x2e77cf978d40d465),
    ("whammy-scale-sweep", 0x32ba9dca667bf335, 0xc44b34a34cffe4a5),
    ("whammy-scale-reset", 0xe4023c96541412f5, 0xb2b0626fc73dd10d),
    ("poly-shift0-reset", 0xf2674d619f431e7d, 0xc022b9d1d3d43ec5),
    ("poly-sweep", 0x44d6cc8a4383dc05, 0x437de1d60372b425),
    ("poly-scale-sweep", 0x7930bde886cdd051, 0x7c49fc44156508f5),
    ("poly-scale-reset", 0x10dc825d336d0ca5, 0xafa89d9173cf46e5),
];

#[test]
#[ignore]
fn bend_unused_is_bit_identical_on_the_dis() {
    let dir = std::path::Path::new(env!("HOME")).join("golem/library/di-refs/guitar");
    let read = |name: &str| -> Option<Vec<f32>> {
        let d = std::fs::read(dir.join(name)).ok()?;
        // 16 / 24-bit PCM or 32-bit float, first channel
        let mut pos = 12;
        let (mut ch, mut bits, mut fmt) = (1usize, 16usize, 1u16);
        while pos + 8 <= d.len() {
            let size = u32::from_le_bytes(d[pos + 4..pos + 8].try_into().ok()?) as usize;
            let body = &d[pos + 8..(pos + 8 + size).min(d.len())];
            if &d[pos..pos + 4] == b"fmt " {
                fmt = u16::from_le_bytes([body[0], body[1]]);
                ch = u16::from_le_bytes([body[2], body[3]]) as usize;
                bits = u16::from_le_bytes([body[14], body[15]]) as usize;
            } else if &d[pos..pos + 4] == b"data" {
                let w = bits / 8;
                let n = (body.len() / (w * ch)).min(8 * 48000);
                return Some(
                    (0..n)
                        .map(|i| {
                            let s = &body[i * w * ch..i * w * ch + w];
                            match (fmt, bits) {
                                (3, 32) | (0xFFFE, 32) => f32::from_le_bytes([s[0], s[1], s[2], s[3]]),
                                (_, 24) => (i32::from_le_bytes([0, s[0], s[1], s[2]]) >> 8) as f32 / 8_388_608.0,
                                (_, 32) => i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f32 / 2_147_483_648.0,
                                _ => i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0,
                            }
                        })
                        .collect(),
                );
            }
            pos += 8 + size + (size & 1);
        }
        None
    };
    let (Some(lead), Some(clean)) = (read("lead.wav"), read("clean.wav")) else {
        eprintln!("golem's DIs are not on this machine: skipped");
        return;
    };
    let mut bad = Vec::new();
    for (name, kv, ev) in REF_CASES {
        let h = [ref_render(kv, ev, &lead), ref_render(kv, ev, &clean)];
        eprintln!("    (\"{name}\", 0x{:016x}, 0x{:016x}),", h[0], h[1]);
        if let Some(r) = REF_HASHES_DI.iter().find(|r| r.0 == name) {
            if (r.1, r.2) != (h[0], h[1]) {
                bad.push(name);
            }
        }
    }
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        assert_eq!(REF_HASHES_DI.len(), REF_CASES.len());
        assert!(bad.is_empty(), "renders changed: {bad:?}");
    }
}

// ---- The triggered bend (README §2.1, bench rows of §6) ----

/// Voice 1's applied interval per sample (`applied_semitones`, after the slew limit), with `bend` pressed
/// at sample 0 and released at sample `release` (never if `usize::MAX`), one-sample blocks of a sine.
fn bend_track(s: Settings, release: usize, n: usize) -> Vec<f64> {
    let mut q = ps(s);
    let x = sine(220.0, n);
    let mut out = Vec::with_capacity(n);
    q.set(q.settings().with(p::BEND, 1.0));
    for i in 0..n {
        if i == release {
            q.set(q.settings().with(p::BEND, 0.0));
        }
        let mut b = [x[i]];
        q.process(&mut [&mut b[..]]);
        out.push(q.applied_semitones()[0]);
    }
    out
}

/// ms from sample `from` to the first sample where `f(st)` holds (that sample counted: the first one
/// processed ends at 1 / 48 ms).
fn first_ms(st: &[f64], from: usize, f: impl Fn(f64) -> bool) -> f64 {
    let i = (from..st.len()).find(|&i| f(st[i])).expect("never reached");
    (i + 1 - from) as f64 / 48.0
}

#[test]
fn bend_timing_linear() {
    // `interval` +12 from `shift` 0: 90 % at 0.9 × rise ± 1 ms, half-way at rise / 2 ± 0.1 st, the return in
    // `fall` ± 1 ms; at 0 ms the slew limit sets the time (1 st / ms: 12 ms); no step beyond the slew's
    let lim = 1000.0 / SR as f64 + 1e-12;
    for t in [0.0f32, 50.0, 300.0, 2000.0] {
        let s = Settings::default().with(p::SHIFT, 0.0).with(p::MIX, 100.0).with(p::INTERVAL, 12.0).with(p::RISE, t).with(p::FALL, t);
        let span = if t == 0.0 { 12.0 } else { t as f64 };
        let rel = ((span + 50.0) * 48.0) as usize;
        let st = bend_track(s, rel, rel + ((span + 50.0) * 48.0) as usize);
        let t90 = first_ms(&st, 0, |v| v >= 10.8 - 1e-9);
        assert!((t90 - 0.9 * span).abs() <= 1.0, "rise {t}: 90 % at {t90:.2} ms");
        let half = st[(span * 24.0) as usize - 1];
        assert!((half - 6.0).abs() <= 0.1, "rise {t}: {half:.3} st at half-way");
        let back = first_ms(&st, rel, |v| v <= 0.0);
        assert!((back - span).abs() <= 1.0, "fall {t}: back at rest after {back:.2} ms");
        let step = st.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f64::max);
        assert!(step <= lim, "rise {t}: a step of {step} st");
        eprintln!("linear {t:6} ms: 90 % at {t90:7.2} ms, half-way {half:.3} st, back after {back:7.2} ms");
    }
}

#[test]
fn bend_timing_smooth() {
    // smoothstep: half-way at rise / 2, 90 % at 0.804 × rise, the end on time
    for t in [50.0f32, 300.0, 2000.0] {
        let s = Settings::default().with(p::SHIFT, 0.0).with(p::MIX, 100.0).with(p::INTERVAL, 12.0).with(p::RISE, t).with(p::FALL, t).with(p::CURVE, 1.0);
        let n = ((t as f64 + 50.0) * 48.0) as usize;
        let st = bend_track(s, n, 2 * n);
        let half = st[(t as f64 * 24.0) as usize - 1];
        assert!((half - 6.0).abs() <= 0.1, "rise {t}: {half:.3} st at half-way");
        let t90 = first_ms(&st, 0, |v| v >= 10.8 - 1e-9);
        assert!((t90 - 0.804 * t as f64).abs() <= 1.0, "rise {t}: 90 % at {t90:.2} ms");
        let end = first_ms(&st, 0, |v| v >= 12.0);
        assert!((end - t as f64).abs() <= 1.0, "rise {t}: the end at {end:.2} ms");
        let back = first_ms(&st, n, |v| v <= 0.0);
        assert!((back - t as f64).abs() <= 1.0, "fall {t}: back after {back:.2} ms");
        eprintln!("smooth {t:6} ms: half-way {half:.3} st, 90 % at {t90:7.2} ms, end {end:7.2} ms, back {back:7.2} ms");
    }
}

#[test]
fn bend_release_half_way_and_the_clamped_travel() {
    // a release half-way returns in fall / 2
    let s = Settings::default().with(p::SHIFT, 0.0).with(p::MIX, 100.0).with(p::INTERVAL, 12.0).with(p::RISE, 300.0).with(p::FALL, 300.0);
    let st = bend_track(s, 150 * 48, 400 * 48);
    let back = first_ms(&st, 150 * 48, |v| v <= 0.0);
    assert!((back - 150.0).abs() <= 1.0, "back after {back:.2} ms");
    // `shift` +12, `interval` +24: the travel is +12 (to +24) over the whole rise, and the pitch leaves +24
    // within 1 ms of the release (the draft's clamped sum reached +24 at φ = 0.5 and then sat there)
    let s = Settings::default().with(p::SHIFT, 12.0).with(p::MIX, 100.0).with(p::INTERVAL, 24.0).with(p::RISE, 300.0).with(p::FALL, 300.0);
    let st = bend_track(s, 400 * 48, 800 * 48);
    let top = first_ms(&st, 0, |v| v >= 24.0);
    assert!((top - 300.0).abs() <= 1.0, "+24 reached at {top:.2} ms");
    assert!(st[285 * 48] < 23.5, "{:.3} st at 285 ms", st[285 * 48]);
    let left = first_ms(&st, 400 * 48, |v| v < 24.0);
    assert!(left <= 1.0, "left +24 after {left:.2} ms");
    // shift −12, interval −24: −12 of travel, to −24
    let s = Settings::default().with(p::SHIFT, -12.0).with(p::MIX, 100.0).with(p::INTERVAL, -24.0).with(p::RISE, 100.0);
    let st = bend_track(s, usize::MAX, 150 * 48);
    assert_eq!(st[st.len() - 1], -24.0);
}

#[test]
fn bend_lands_in_tune_and_both_voices_move() {
    // Whammy, a sine at 220 Hz bent +12: the end pitch within 0.2 cents of 440 Hz (as the plain shift)
    let s = Settings::default().with(p::SHIFT, 0.0).with(p::MIX, 100.0).with(p::INTERVAL, 12.0).with(p::RISE, 50.0);
    let mut q = ps(s);
    q.set(s.with(p::BEND, 1.0));
    let y = run(&mut q, &sine(220.0, 96000));
    let f = freq_of(&y[24000..]);
    let cents = 1200.0 * (f / 440.0).log2();
    assert!(cents.abs() < 0.2, "{cents:+.3} cents");
    // voice 2 bends with voice 1, in Scale too (chromatic semitones on the in-key interval: C major, a
    // third and a fifth above C, +2 each)
    let s = Settings::default().with(p::SHIFT, 4.0).with(p::SHIFT2, 7.0).with(p::VOICE2, -6.0).with(p::INTERVAL, 2.0).with(p::RISE, 0.0).with(p::BEND, 1.0);
    for h in [0.0, 1.0] {
        let mut q = ps(s.with(p::HARMONY, h));
        run(&mut q, &sine(261.63, 4800));
        let st = q.applied_semitones();
        assert!((st[0] - 6.0).abs() < 1e-9 && (st[1] - 9.0).abs() < 1e-9, "harmony {h}: {st:?}");
    }
    // reset() with `bend` stored on: bent at once, no ramp
    let q = ps(Settings::default().with(p::SHIFT, 0.0).with(p::BEND, 1.0).with(p::INTERVAL, -5.0));
    assert_eq!(q.applied_semitones()[0], -5.0);
    assert_eq!(q.bend_state(), 1.0);
}

/// HF burst (4th-order high-pass at 10 kHz, as gfx-bench's `param_steps`): the peak in the 100 ms after
/// `at`, minus the steady signal's in the 250 ms before it, in dB re the output's peak.
fn hf_burst(y: &[f32], at: usize) -> f64 {
    use gfx_dsp::svf::{Svf, SvfMode};
    let (mut a, mut b) = (Svf::new(), Svf::new());
    a.set(SvfMode::HighPass, 10000.0, 0.541_196, 0.0, SR);
    b.set(SvfMode::HighPass, 10000.0, 1.306_563, 0.0, SR);
    let hp: Vec<f32> = y.iter().map(|&v| b.process(a.process(v))).collect();
    let pk = |s: &[f32]| s.iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64;
    let after = pk(&hp[at..at + 4800]);
    let steady = pk(&hp[at - 12000..at]);
    20.0 * ((after - steady).max(1e-12) / pk(y)).log10()
}

/// Renders `x` with `s` in blocks of 256, `bend` on at sample `on` and off at `off` (multiples of 256).
fn bend_render(s: Settings, x: &[f32], on: usize, off: usize) -> Vec<f32> {
    let mut q = ps(s);
    let mut y = x.to_vec();
    for (k, c) in y.chunks_mut(256).enumerate() {
        if k * 256 == on {
            q.set(q.settings().with(p::BEND, 1.0));
        }
        if k * 256 == off {
            q.set(q.settings().with(p::BEND, 0.0));
        }
        q.process(&mut [c]);
    }
    y
}

#[test]
fn bend_clicks_within_the_step_bound() {
    // gfx-bench's step bound is −50 dB re peak; the Whammy's own EXP sweep (0 → +12, `glide` 300 ms) for
    // reference
    let x = sine(220.0, 96000);
    let mut q = ps(Settings::default().with(p::SHIFT, 0.0).with(p::MIX, 100.0).with(p::GLIDE, 300.0));
    let mut y = x.clone();
    let (a, b) = y.split_at_mut(48128);
    for c in a.chunks_mut(256) {
        q.process(&mut [c]);
    }
    q.set(q.settings().with(p::SHIFT, 12.0));
    for c in b.chunks_mut(256) {
        q.process(&mut [c]);
    }
    let sweep = hf_burst(&y, 48128);
    let mut worst = f64::MIN;
    for rest in [0.0, 1.0] {
        let s = Settings::default().with(p::SHIFT, 0.0).with(p::MIX, 100.0).with(p::INTERVAL, 12.0).with(p::RISE, 0.0).with(p::FALL, 0.0).with(p::REST, rest);
        let y = bend_render(s, &x, 48128, 72192);
        let (up, down) = (hf_burst(&y, 48128), hf_burst(&y, 72192));
        eprintln!("Whammy +12 in 0 ms, rest {rest}: up {up:.1} dB, down {down:.1} dB (the EXP sweep: {sweep:.1} dB)");
        worst = worst.max(up).max(down);
    }
    // Poly, a chord, −2 in 20 ms (the whole-step stab)
    let chord = rich(&[110.0, 164.81, 220.0], 96000);
    for rest in [0.0, 1.0] {
        let s = Settings::default().with(p::ENGINE, 1.0).with(p::SHIFT, 0.0).with(p::MIX, 100.0).with(p::INTERVAL, -2.0).with(p::RISE, 20.0).with(p::FALL, 40.0).with(p::REST, rest);
        let y = bend_render(s, &chord, 48128, 72192);
        let (up, down) = (hf_burst(&y, 48128), hf_burst(&y, 72192));
        eprintln!("Poly −2 in 20 ms, rest {rest}: up {up:.1} dB, down {down:.1} dB");
        worst = worst.max(up).max(down);
    }
    assert!(worst <= -50.0, "worst HF burst {worst:.1} dB");
}

/// CPU of the bend (ignored: timing). ns per sample, mono, 7 s of a chord in blocks of 128: at rest
/// (`bend` off) and bending all the time (`rise` 8 s, Smooth, `rest` Dry: φ, the curve, two offsets, the gate).
#[test]
#[ignore]
fn bend_cost() {
    use std::time::Instant;
    let x = rich(&[110.0, 164.81, 220.0], 336000);
    for engine in [0.0f32, 1.0] {
        for (name, s) in [
            ("rest", Settings::default().with(p::ENGINE, engine)),
            // the shifter's own cost of a moving ratio, for comparison: `shift` 0 → +24 with `glide` 1 s
            // (still moving after 7 s), no bend
            ("glide", Settings::default().with(p::ENGINE, engine).with(p::GLIDE, 1000.0)),
            ("bending", Settings::default().with(p::ENGINE, engine).with(p::RISE, 8000.0).with(p::CURVE, 1.0).with(p::REST, 1.0).with(p::BEND, 1.0)),
        ] {
            let mut best = f64::MAX;
            for _ in 0..3 {
                let mut q = Pitch::new();
                q.prepare(SR, 128, 1);
                q.set(s.with(p::BEND, 0.0).with(p::SHIFT, if name == "glide" { 0.0 } else { s.v[p::SHIFT] }));
                q.reset();
                q.set(s.with(p::SHIFT, if name == "glide" { 24.0 } else { s.v[p::SHIFT] }));
                let mut y = x.clone();
                let t = Instant::now();
                for c in y.chunks_mut(128) {
                    q.process(&mut [c]);
                }
                best = best.min(t.elapsed().as_secs_f64() * 1e9 / x.len() as f64);
                assert!(name != "bending" || q.bend_state() < 1.0);
            }
            eprintln!("engine {engine} {name:8}: {best:6.1} ns / sample");
        }
    }
}

#[test]
fn rest_dry_is_the_delayed_input_after_ten_bends() {
    // `rest` Dry, `shift` 0: at rest the output is the input delayed by the latency (the wet gated out),
    // both engines, `mix` 50 % and 100 %
    for engine in [0.0f32, 1.0] {
        for mix in [50.0f32, 100.0] {
            let s = Settings::default().with(p::ENGINE, engine).with(p::SHIFT, 0.0).with(p::MIX, mix).with(p::REST, 1.0).with(p::INTERVAL, 7.0).with(p::RISE, 30.0).with(p::FALL, 60.0);
            let mut q = ps(s);
            let lat = q.latency_samples();
            let x = rich(&[110.0, 164.81, 220.0], 48000 * 6);
            let mut y = x.clone();
            for (k, c) in y.chunks_mut(256).enumerate() {
                // ten bends of 200 ms in the first 4 s
                let t = k * 256;
                let on = t < 192000 && (t % 19200) < 9600;
                if on != (q.settings().v[p::BEND] >= 0.5) {
                    q.set(q.settings().with(p::BEND, if on { 1.0 } else { 0.0 }));
                }
                q.process(&mut [c]);
            }
            let (mut e, mut r) = (0.0f64, 0.0f64);
            for i in 240000..288000 {
                e += (y[i] as f64 - x[i - lat] as f64).powi(2);
                r += (x[i - lat] as f64).powi(2);
            }
            let null = 10.0 * (e.max(1e-30) / r).log10();
            eprintln!("rest Dry, engine {engine}, mix {mix}: null {null:.1} dB");
            assert!(null <= -90.0, "engine {engine} mix {mix}: {null:.1} dB");
        }
    }
}
