// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5) on synthetic strings (§1.3's model: partials |sin(π k p)| / k up to
//! 7 kHz, p = 0.13, B = 10⁻⁴, T60 3 s falling with k, a 4 ms pick noise, random phases; chords
//! strummed over 20 ms and normalised to a 0.5 peak).

use super::*;
use gfx_dsp::lfo::Rng;
use realfft::RealFftPlanner;

const SR: f32 = 48000.0;

fn midi_hz(m: f64) -> f64 {
    440.0 * ((m - 69.0) / 12.0).exp2()
}

/// A gaussian (sum of 4 uniforms, unit variance).
fn gauss(r: &mut Rng) -> f64 {
    (0..4).map(|_| r.next_f64() * 2.0 - 1.0).sum::<f64>() * (3.0f64 / 4.0).sqrt()
}

/// One synthetic string (§1.3), added into `y` from `t0` (s).
fn string_into(y: &mut [f64], r: &mut Rng, f0: f64, t0: f64, t60: f64, sr: f64) {
    let (b, p, fmax) = (1e-4f64, 0.13f64, 7000.0f64);
    let i0 = (t0 * sr) as usize;
    let mut k = 1.0f64;
    loop {
        let fk = f0 * k * (1.0 + b * k * k).sqrt();
        if fk > fmax.min(0.45 * sr) {
            break;
        }
        let a = (PI * k * p).sin().abs() / k;
        let tk = t60 / (1.0 + 0.12 * k);
        let ph = r.next_f64() * TAU;
        let w = TAU * fk / sr;
        let dec = (-6.91 / (tk * sr)).exp();
        let mut env = a;
        for (i, v) in y.iter_mut().enumerate().skip(i0) {
            *v += env * (w * (i - i0) as f64 + ph).sin();
            env *= dec;
        }
        k += 1.0;
    }
    let m = (0.004 * sr) as usize;
    for i in 0..m {
        if i0 + i < y.len() {
            let h = 0.5 - 0.5 * (TAU * i as f64 / m as f64).cos();
            y[i0 + i] += 0.05 * gauss(r) * h;
        }
    }
}

/// A strummed chord (MIDI notes), 20 ms spread from 50 ms, peak 0.5.
fn chord(notes: &[f64], dur: f64, seed: u64, t60: f64, sr: f64) -> Vec<f32> {
    let mut y = vec![0.0f64; (dur * sr) as usize];
    let mut r = Rng::new(seed);
    let n = notes.len().max(2) - 1;
    for (i, &m) in notes.iter().enumerate() {
        string_into(&mut y, &mut r, midi_hz(m), 0.05 + 0.02 * i as f64 / n as f64, t60, sr);
    }
    let pk = y.iter().fold(0.0f64, |a, v| a.max(v.abs()));
    y.iter().map(|v| (v / pk * 0.5) as f32).collect()
}

fn make(sr: f32, ch: usize, set: &[(&str, f32)]) -> Polysynth {
    let mut e = Polysynth::new();
    for &(id, v) in set {
        e.set_param(id, v);
    }
    e.prepare(sr, 512, ch);
    e.reset();
    e
}

fn render(e: &mut Polysynth, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for c in y.chunks_mut(256) {
        e.process(&mut [c]);
    }
    y
}

/// The core alone, as plainly as possible: no dry, the filter wide open and still, Follow without a
/// rise limit, no hold.
fn plain(extra: &[(&str, f32)]) -> Vec<(&'static str, f32)> {
    let mut v: Vec<(&'static str, f32)> = vec![("dry", -60.0), ("synth", 0.0), ("cutoff", 12000.0), ("reso", 0.0), ("filt_env", 0.0), ("attack", 0.0), ("compress", 0.0)];
    for &(id, x) in extra {
        let id: &'static str = PARAMS.iter().find(|p| p.id == id).unwrap().id;
        v.retain(|(k, _)| *k != id);
        v.push((id, x));
    }
    v
}

fn power_spectrum(x: &[f32]) -> Vec<f64> {
    let n = x.len();
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);
    // Blackman
    let mut buf: Vec<f64> = x
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let a = TAU * i as f64 / (n - 1) as f64;
            v as f64 * (0.42 - 0.5 * a.cos() + 0.08 * (2.0 * a).cos())
        })
        .collect();
    let mut spec = fft.make_output_vec();
    fft.process(&mut buf, &mut spec).unwrap();
    spec.iter().map(|c| c.norm_sqr()).collect()
}

/// §1.3's metric: energy off every played note's harmonics (±25 ¢, ≥ 3 Hz), 30 Hz – 8 kHz, over the
/// held body (0.35 – 1.35 s), relative to the total (dB).
fn off_harmonic_db(y: &[f32], notes: &[f64], sr: f64) -> f64 {
    let (a, b) = ((0.35 * sr) as usize, (1.35 * sr) as usize);
    let s = power_spectrum(&y[a..b]);
    let df = sr / (b - a) as f64;
    let (mut off, mut tot) = (0.0, 0.0);
    for (i, p) in s.iter().enumerate() {
        let f = i as f64 * df;
        if !(30.0..8000.0).contains(&f) {
            continue;
        }
        let mut harm = false;
        'n: for &m in notes {
            let f0 = midi_hz(m);
            for k in 1..400 {
                let kk = k as f64;
                let fk = f0 * kk * (1.0 + 1e-4 * kk.min(60.0).powi(2)).sqrt();
                if fk > sr / 2.0 {
                    break;
                }
                let tol = (fk * ((25.0f64 / 1200.0).exp2() - 1.0)).max(3.0);
                if (f - fk).abs() < tol || (f - f0 * kk).abs() < tol {
                    harm = true;
                    break 'n;
                }
            }
        }
        tot += p;
        if !harm {
            off += p;
        }
    }
    10.0 * (off / tot.max(1e-30) + 1e-20).log10()
}

/// A full-band tanh fuzz (gain 30), the reference of §1.3.
fn fuzz(x: &[f32]) -> Vec<f32> {
    x.iter().map(|v| (30.0 * v).tanh()).collect()
}

/// Amplitude of the component at `f` over `x` (Hann-windowed DFT).
fn amp_at(x: &[f32], f: f64, sr: f64) -> f64 {
    let n = x.len();
    let w = TAU * f / sr;
    let (mut re, mut im, mut ws) = (0.0, 0.0, 0.0);
    for (i, &v) in x.iter().enumerate() {
        let h = 0.5 - 0.5 * (TAU * i as f64 / n as f64).cos();
        re += h * v as f64 * (w * i as f64).cos();
        im += h * v as f64 * (w * i as f64).sin();
        ws += h;
    }
    2.0 * (re * re + im * im).sqrt() / ws
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

fn sine(f: f64, amp: f64, n: usize, sr: f64) -> Vec<f32> {
    (0..n).map(|i| (amp * (TAU * f * i as f64 / sr).sin()) as f32).collect()
}

// ---------------------------------------------------------------- band design

#[test]
fn band_design_lags_and_rates() {
    let (c, f) = (spectral::clean_grid(), spectral::fast_grid());
    // 2 kHz down to 24 Hz: the clean bank holds 59 bands from 40 Hz, the fast bank 40
    let from40 = |g: &[gfx_dsp::filterbank::BandSpec]| g.iter().filter(|b| b.centre >= 39.0).count();
    println!("clean {} bands ({} from 40 Hz), fast {} ({})", c.len(), from40(&c), f.len(), from40(&f));
    assert!((58..=60).contains(&from40(&c)) && (39..=41).contains(&from40(&f)));
    // lag N / (π B) at the centre, measured from the phase slope, within 5 % of §2.2's figures
    let fb = 24000.0;
    let lag = |s: &gfx_dsp::filterbank::BandSpec| {
        let d = 0.01;
        let (a, b) = (gfx_dsp::filterbank::band_response(s, s.centre - d, fb), gfx_dsp::filterbank::band_response(s, s.centre + d, fb));
        -(b.1.atan2(b.0) - a.1.atan2(a.0)) / (TAU * 2.0 * d)
    };
    let near = |g: &[gfx_dsp::filterbank::BandSpec], hz: f64| *g.iter().min_by(|a, b| (a.centre - hz).abs().total_cmp(&(b.centre - hz).abs())).unwrap();
    for (g, hz, want) in [(&c, 82.4, 0.064), (&c, 330.0, 0.064), (&c, 1000.0, 0.029), (&c, 2000.0, 0.016), (&f, 330.0, 0.016), (&f, 1000.0, 0.010), (&f, 2000.0, 0.005)] {
        let b = near(g, hz);
        let l = lag(&b);
        println!("order {} at {:.0} Hz: lag {:.1} ms (N/(πB) {:.1}, README {:.0})", b.order, b.centre, l * 1e3, b.order as f64 / (PI * b.width) * 1e3, want * 1e3);
        assert!((l / (b.order as f64 / (PI * b.width)) - 1.0).abs() < 0.05);
        assert!((l / want - 1.0).abs() < 0.12, "{hz}: {l}");
    }
    // the same bank rate (so the same bands) at every host rate
    for (sr, fb) in [(44100.0f32, 22050.0), (48000.0, 24000.0), (88200.0, 22050.0), (96000.0, 24000.0), (176400.0, 22050.0), (192000.0, 24000.0)] {
        let e = make(sr, 1, &[]);
        assert_eq!(e.spectral_core().bank_rate(), fb);
        assert_eq!(e.spectral_core().designs().0.len(), c.len());
    }
}

// ---------------------------------------------------------------- timbre against pitch

/// Harmonic levels (dB re the input sine) of the steady output on a sine at `f`.
fn harmonics(set: &[(&'static str, f32)], f: f64, n: usize) -> Vec<f64> {
    let mut e = make(SR, 1, set);
    let x = sine(f, 0.1, (0.7 * SR as f64) as usize, SR as f64);
    let y = render(&mut e, &x);
    let tail = &y[(0.35 * SR as f64) as usize..];
    // the filter's own response at each harmonic (`plain`: low-pass 12 kHz, Q 0.5, 4 poles) taken out
    let filt = |fh: f64| {
        let mut v = Vcf::new();
        v.set(12000.0, q_of(0.0), VcfType::LowPass, true, SR);
        gfx_dsp::svf::measure_gain_db(|x| v.process(x, 0.0), fh as f32, SR) as f64
    };
    (1..=n).map(|k| to_db(amp_at(tail, f * k as f64, SR as f64) / 0.1) - filt(f * k as f64)).collect()
}

#[test]
fn timbre_against_pitch() {
    // a sine stepped over an octave (1/24 octave apart) from 82, 220 and 660 Hz, each bank, Saw: every
    // harmonic under fcap within ±1 dB of c_n (README §5; the drafted shaper swung 5 – 26 dB). The
    // filter's response is taken out: every harmonic within ±1 dB of c_n, and its swing over the octave
    // within 1 dB
    let c = spectral::coefs(0, 0.5);
    let fcap = spectral::FCAP * 24000.0;
    let mut worst: f64 = 0.0;
    let mut swing: f64 = 0.0;
    for bank in [1.0f32, 2.0] {
        for f0 in [82.4, 220.0, 660.0] {
            let mut lo = [f64::MAX; 9];
            let mut hi = [f64::MIN; 9];
            for i in 0..=24 {
                let f = f0 * (i as f64 / 24.0).exp2();
                let h = harmonics(&plain(&[("bank", bank), ("wave", 0.0)]), f, 9);
                for k in 0..9 {
                    if (k + 1) as f64 * f > (fcap - 500.0).min(6000.0) {
                        continue;
                    }
                    let want = to_db((c[k].0.powi(2) + c[k].1.powi(2)).sqrt());
                    let d = h[k] - want;
                    lo[k] = lo[k].min(d);
                    hi[k] = hi[k].max(d);
                    {
                        worst = worst.max(d.abs());
                    }
                }
            }
            for k in 0..9 {
                if lo[k] < 1e9 {
                    swing = swing.max(hi[k] - lo[k]);
                }
            }
            println!("bank {bank} from {f0} Hz: per harmonic (min, max) dB re c_n: {:?}", lo.iter().zip(&hi).filter(|(a, _)| **a < 1e9).map(|(a, b)| format!("{a:+.2}/{b:+.2}")).collect::<Vec<_>>());
        }
    }
    // Sine: the sweep flat within ±1 dB
    let mut s_lo = f64::MAX;
    let mut s_hi = f64::MIN;
    for i in 0..=48 {
        let f = 82.4 * (i as f64 / 12.0).exp2();
        let h = harmonics(&plain(&[("wave", 3.0)]), f, 1)[0];
        s_lo = s_lo.min(h);
        s_hi = s_hi.max(h);
    }
    println!("Sine sweep 82 Hz – 1.3 kHz: {s_lo:+.2} … {s_hi:+.2} dB; worst harmonic {worst:.2} dB re c_n, worst swing {swing:.2} dB");
    assert!(worst <= 1.0 && swing <= 1.0, "{worst} {swing}");
    assert!(s_lo >= -1.0 && s_hi <= 1.0);
}

// ---------------------------------------------------------------- chords

const CHORDS: [(&str, &[f64], f64); 6] = [
    ("E5 power", &[40.0, 47.0, 52.0], -30.0),
    ("open E", &[40.0, 47.0, 52.0, 56.0, 59.0, 64.0], -30.0),
    ("open C", &[48.0, 52.0, 55.0, 60.0, 64.0], -30.0),
    ("A minor barre", &[45.0, 52.0, 57.0, 60.0, 64.0, 69.0], -30.0),
    ("E4 + G#4", &[64.0, 68.0], -40.0),
    ("E2 + G#2", &[40.0, 44.0], -20.0),
];

#[test]
fn chord_cleanliness() {
    // Pulse 50 %, the §1.3 metric on the body; Clean and Dual under the bounds and ≥ 8 dB under a
    // full-band fuzz on every resolved chord; Fast reported (not worse than the fuzz)
    let sr = SR as f64;
    for (name, notes, bound) in CHORDS {
        let mut line = format!("{name:14}");
        for seed in [1u64, 2] {
            let x = chord(notes, 1.5, seed, 3.0, sr);
            let fz = off_harmonic_db(&fuzz(&x), notes, sr);
            let mut got = [0.0; 3];
            for (j, bank) in [2.0f32, 0.0, 1.0].into_iter().enumerate() {
                let mut e = make(SR, 1, &plain(&[("wave", 1.0), ("bank", bank)]));
                got[j] = off_harmonic_db(&render(&mut e, &x), notes, sr);
            }
            line += &format!("  seed {seed}: clean {:.1} dual {:.1} fast {:.1} fuzz {:.1}", got[0], got[1], got[2], fz);
            assert!(got[0] <= bound && got[1] <= bound, "{name}: {got:?}");
            if name != "E2 + G#2" {
                assert!(got[0] <= fz - 8.0 && got[1] <= fz - 8.0, "{name}: {got:?} vs fuzz {fz}");
            }
        }
        println!("{line}");
    }
}

#[test]
#[ignore]
fn cpu_report() {
    // README §2.6: ≤ 8 % of a core per mono instance, worst case (Dual, a strum, unison, Mono's voice)
    let sr = SR as f64;
    let x = chord(&[40.0, 47.0, 52.0, 56.0, 59.0, 64.0], 10.0, 3, 3.0, sr);
    for (name, set) in [
        ("Spectral defaults, open E strum", vec![]),
        ("Spectral, detune 12, sub, noise", vec![("detune", 12.0f32), ("sub", 30.0), ("noise", 20.0)]),
        ("Mono defaults", vec![("engine", 1.0)]),
        ("Mono, detune 12", vec![("engine", 1.0), ("detune", 12.0)]),
    ] {
        let mut e = make(SR, 1, &set);
        let t = std::time::Instant::now();
        let _ = render(&mut e, &x);
        let ns = t.elapsed().as_secs_f64() * 1e9 / x.len() as f64;
        println!("{name}: {ns:.0} ns / sample ({:.1} % of a core at 48 kHz)", ns * 48000.0 / 1e7);
    }
}

/// 4th-order Butterworth high-pass (two SVF sections), as gfx-bench's step check.
fn highpass4(x: &[f32], fc: f32, sr: f32) -> Vec<f32> {
    let mut a = Svf::new();
    let mut b = Svf::new();
    a.set(SvfMode::HighPass, fc, 0.541_196, 0.0, sr);
    b.set(SvfMode::HighPass, fc, 1.306_563, 0.0, sr);
    x.iter().map(|&v| b.process(a.process(v))).collect()
}

/// gfx-bench's parameter-step measure: the HF burst (over 10 kHz) in the 100 ms after a step of `id`
/// from `from` to `to` on a 1 kHz sine at −12 dBFS, minus the steady HF, re the output peak (dB).
fn step_burst(id: &str, from: f32, to: f32, sr: f32, set: &[(&str, f32)]) -> f64 {
    let n = sr as usize;
    let x = sine(1000.0, 0.25, n, sr as f64);
    let mut e = Polysynth::new();
    for &(k, v) in set {
        e.set_param(k, v);
    }
    e.set_param(id, from);
    e.prepare(sr, 512, 1);
    e.reset();
    let mut y = x.clone();
    let half = n / 2;
    {
        let (a, b) = y.split_at_mut(half);
        for c in a.chunks_mut(512) {
            e.process(&mut [c]);
        }
        e.set_param(id, to);
        for c in b.chunks_mut(512) {
            e.process(&mut [c]);
        }
    }
    let pk = y.iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64;
    let hp = highpass4(&y, 10000.0f32.min(sr * 0.4), sr);
    let peak = |s: &[f32]| s.iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64;
    let after = peak(&hp[half..half + n / 10]);
    let steady = peak(&hp[n / 4..half]).max(peak(&hp[n - n / 4..]));
    to_db((after - steady).max(1e-12)) - to_db(pk)
}

// ---------------------------------------------------------------- Mono

/// A string whose f0 follows `cents(t)` (s → cents re `f0`), plucked at `t0`, partials as §1.3,
/// level `amp` (peak of the first partial's envelope ≈ amp), decaying with `t60`.
fn moving_string(f0: f64, dur: f64, t0: f64, t60: f64, amp: f64, seed: u64, cents: impl Fn(f64) -> f64, sr: f64) -> Vec<f32> {
    let n = (dur * sr) as usize;
    let mut y = vec![0.0f64; n];
    let mut r = Rng::new(seed);
    let i0 = (t0 * sr) as usize;
    let ratio: Vec<f64> = (i0..n).map(|i| (cents((i - i0) as f64 / sr) / 1200.0).exp2()).collect();
    let mut k = 1.0f64;
    loop {
        let fk = f0 * k * (1.0 + 1e-4 * k * k).sqrt();
        if fk > 7000.0 {
            break;
        }
        let a = (PI * k * 0.13).sin().abs() / k;
        let tk = t60 / (1.0 + 0.12 * k);
        let dec = (-6.91 / (tk * sr)).exp();
        let mut env = a;
        let mut ph = r.next_f64() * TAU;
        for (j, v) in y.iter_mut().enumerate().skip(i0) {
            *v += env * ph.sin();
            ph += TAU * fk * ratio[j - i0] / sr;
            env *= dec;
        }
        k += 1.0;
    }
    let m = (0.004 * sr) as usize;
    for i in 0..m.min(n - i0) {
        let h = 0.5 - 0.5 * (TAU * i as f64 / m as f64).cos();
        y[i0 + i] += 0.05 * gauss(&mut r) * h;
    }
    let pk = (PI * 0.13).sin();
    y.iter().map(|v| (v / pk * amp) as f32).collect()
}

/// Renders in 1 ms blocks, recording the voices after each block.
type VoiceLog = Vec<Vec<(f64, f64, f64, u8)>>;

fn render_watch(e: &mut Polysynth, x: &[f32]) -> (Vec<f32>, VoiceLog) {
    let mut y = x.to_vec();
    let mut log = Vec::new();
    for c in y.chunks_mut(48) {
        e.process(&mut [c]);
        log.push(e.voice_info());
    }
    (y, log)
}

#[test]
fn mono_reads_the_note_and_never_its_octave() {
    // lone E2, A2, D3 (and E4), picked: the voice starts on YIN's first readings, never at the
    // octave or the 12th
    let sr = SR as f64;
    for (m, seed) in [(40.0, 1u64), (45.0, 2), (50.0, 3), (64.0, 4)] {
        let f0 = midi_hz(m);
        let x = moving_string(f0, 1.0, 0.1, 3.0, 0.3, seed, |_| 0.0, sr);
        let mut e = make(SR, 1, &[("engine", 1.0)]);
        let (_, log) = render_watch(&mut e, &x);
        let start = log.iter().position(|v| v.iter().any(|w| w.3 == 1)).expect("a voice");
        let ms = start as f64 - 100.0;
        let mut worst: f64 = 0.0;
        for v in log.iter().flatten().filter(|w| w.3 == 1) {
            worst = worst.max(cents(v.0, f0).abs());
        }
        println!("MIDI {m}: voice after {ms:.0} ms, worst {worst:.1} ¢ from the note");
        assert!(ms <= 40.0, "{m}: {ms}");
        assert!(worst < 100.0, "{m}: {worst} ¢");
    }
}

#[test]
fn mono_bridge() {
    // E4 then A2: the voice takes over without a gap or a step; the owned core bands go silent;
    // `residue` 0 is silent until the reading
    let sr = SR as f64;
    for (m, seed) in [(64.0, 5u64), (45.0, 6)] {
        let f0 = midi_hz(m);
        // a held string (T60 30 s): its level is flat after the pick
        let x = moving_string(f0, 0.6, 0.1, 30.0, 0.3, seed, |_| 0.0, sr);
        let mut e = make(SR, 1, &plain(&[("engine", 1.0), ("compress", 0.0)]));
        let (y, log) = render_watch(&mut e, &x);
        let start = log.iter().position(|v| v.iter().any(|w| w.3 == 1)).unwrap();
        // 20 ms RMS every 5 ms (a 5 ms window ripples ±1 dB on E4's period)
        let w = (0.005 * sr) as usize;
        let env: Vec<f64> = (0..y.len() / w - 4).map(|i| to_db(rms(&y[i * w..i * w + 4 * w]))).collect();
        // from 20 ms after the pick to 150 ms after the voice: no gap, no step
        let a = (0.12 * sr) as usize / w;
        let b = (start * 48 + (0.15 * sr) as usize) / w;
        let body = env[b..b + 10].iter().sum::<f64>() / 10.0;
        let gap = env[a..b].iter().map(|v| body - v).fold(f64::MIN, f64::max);
        let step = env[a..b].windows(2).map(|p| (p[1] - p[0]).abs()).fold(0.0, f64::max);
        let owned = e.spectral_core().owned_mask();
        println!("MIDI {m}: voice at {} ms; deepest gap {gap:.2} dB, largest 5 ms step {step:.2} dB; owned core bands at {:.1} dB", start as i64 - 100, to_db(owned));
        assert!(gap <= 3.0 && step <= 1.5, "{m}: gap {gap} step {step}");
        assert!(owned <= 0.0316);
        // residue 0: nothing before the voice
        let mut e = make(SR, 1, &plain(&[("engine", 1.0), ("residue", 0.0)]));
        let (y, log) = render_watch(&mut e, &x);
        let start = log.iter().position(|v| v.iter().any(|w| w.3 == 1)).unwrap();
        let before = y[..start * 48].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(before < 1e-6, "{before}");
    }
}

#[test]
fn mono_bends_and_vibrato() {
    // a 200 ¢ bend in 150 ms and a ±30 ¢ 6 Hz vibrato on E4 and G3: the voice within 10 ¢ RMS of the
    // string, within 3 ¢ 50 ms after the bend stops
    let sr = SR as f64;
    for m in [64.0, 55.0] {
        let f0 = midi_hz(m);
        let bend = |t: f64| if t < 0.3 { 0.0 } else if t < 0.45 { 200.0 * (t - 0.3) / 0.15 } else { 200.0 };
        let vib = |t: f64| if t < 0.3 { 0.0 } else { 30.0 * (TAU * 6.0 * (t - 0.3)).sin() };
        for (name, curve) in [("bend", &bend as &dyn Fn(f64) -> f64), ("vibrato", &vib)] {
            let x = moving_string(f0, 1.2, 0.1, 30.0, 0.3, 7, curve, sr);
            let mut e = make(SR, 1, &[("engine", 1.0)]);
            let (_, log) = render_watch(&mut e, &x);
            let (mut se, mut n) = (0.0f64, 0.0f64);
            let mut after: f64 = 0.0;
            for (i, v) in log.iter().enumerate() {
                let t = i as f64 * 1e-3 - 0.1;
                let Some(w) = v.iter().find(|w| w.3 == 1) else { continue };
                if t < 0.25 {
                    continue;
                }
                let want = f0 * (curve(t) / 1200.0).exp2();
                let c = cents(w.0, want);
                se += c * c;
                n += 1.0;
                if name == "bend" && t > 0.5 {
                    after = after.max(c.abs());
                }
            }
            let rms_c = (se / n.max(1.0)).sqrt();
            println!("MIDI {m} {name}: voice {rms_c:.2} ¢ RMS from the string{}", if name == "bend" { format!(", {after:.2} ¢ at most 50 ms after the bend") } else { String::new() });
            assert!(rms_c <= 10.0, "{m} {name}: {rms_c}");
            if name == "bend" {
                assert!(after <= 3.0, "{m}: {after}");
            }
        }
    }
}

#[test]
fn mono_note_end() {
    // a palm mute (−40 dB in 20 ms): the voice is in release within 60 ms, its frequency held
    let sr = SR as f64;
    let f0 = midi_hz(52.0);
    let mut x = moving_string(f0, 1.0, 0.1, 30.0, 0.3, 8, |_| 0.0, sr);
    let mute = (0.5 * sr) as usize;
    for (i, v) in x.iter_mut().enumerate().skip(mute) {
        let t = (i - mute) as f64 / sr;
        *v *= (10f64.powf(-2.0 * (t / 0.02).min(1.0))) as f32;
    }
    let mut e = make(SR, 1, &[("engine", 1.0), ("release", 300.0)]);
    let (_, log) = render_watch(&mut e, &x);
    let rel = (500..log.len()).find(|&i| log[i].iter().all(|w| w.3 != 1)).unwrap();
    let held: Vec<f64> = log[rel..rel + 100].iter().filter_map(|v| v.first().map(|w| w.0)).collect();
    let drift = held.iter().map(|f| cents(*f, held[0]).abs()).fold(0.0, f64::max);
    println!("mute at 500 ms: release at {rel} ms; frequency held within {drift:.3} ¢");
    assert!(rel - 500 <= 60 && drift < 1.0);
}

// ---------------------------------------------------------------- latency, level, determinism

#[test]
fn latency_dry_at_sample_zero() {
    // 0 reported; the dry's impulse at sample 0 at every `dry`, whatever the synth does
    for dry in [0.0f32, -12.0, -40.0] {
        for engine in [0.0f32, 1.0] {
            let mut e = make(SR, 1, &[("dry", dry), ("engine", engine)]);
            assert_eq!(e.latency(), 0);
            let mut x = vec![0.0f32; 4800];
            x[0] = 0.5;
            let y = render(&mut e, &x);
            let want = 0.5 * 10f32.powf(dry / 20.0);
            assert!((y[0] - want).abs() < 1e-6, "dry {dry}: {} vs {want}", y[0]);
        }
    }
}

#[test]
fn ceiling_and_level_bound() {
    // the ceiling: bit-transparent under −6 dBFS, never past 0 dBFS
    let mut c = Ceiling::default();
    for i in 0..1000 {
        let x = 0.5 * (i as f64 * 0.01).sin();
        assert_eq!(c.process(x), x);
    }
    let mut c = Ceiling::default();
    let mut top: f64 = 0.0;
    for i in 0..48000 {
        top = top.max(c.process(20.0 * (i as f64 * 0.003).sin()).abs());
    }
    assert!(top <= 1.0, "{top}");
    // a −6 dBFS chord, `compress` 100 %, `reso` 100 % swept, `detune` on, `synth` +12 dB: never over
    // 0 dBFS
    let sr = SR as f64;
    let x: Vec<f32> = chord(&[40.0, 47.0, 52.0, 56.0], 2.0, 9, 3.0, sr);
    let mut e = make(SR, 1, &[("dry", -60.0), ("compress", 100.0), ("reso", 100.0), ("detune", 20.0), ("synth", 12.0), ("drive", 50.0)]);
    let mut y = x.clone();
    let mut worst = 0.0f32;
    for (k, c) in y.chunks_mut(480).enumerate() {
        e.set_param("cutoff", 200.0 + 4800.0 * (0.5 + 0.5 * (k as f32 * 0.2).sin()));
        e.process(&mut [c]);
        worst = c.iter().fold(worst, |m, v| m.max(v.abs()));
    }
    println!("level bound: peak {:.2} dBFS", 20.0 * worst.log10());
    assert!(worst <= 1.0);
}

#[test]
fn deterministic_and_stereo() {
    let sr = SR as f64;
    let x = chord(&[45.0, 52.0, 57.0], 1.0, 11, 3.0, sr);
    for engine in [0.0f32, 1.0] {
        let set = [("engine", engine), ("detune", 10.0), ("noise", 30.0), ("sub", 40.0)];
        let mut a = make(SR, 1, &set);
        let mut b = make(SR, 1, &set);
        let ya = render(&mut a, &x);
        assert_eq!(ya, render(&mut b, &x));
        // 2 / 2 at `spread` 0: L = R = the mono output
        let mut s = make(SR, 2, &set);
        let mut l = x.clone();
        let mut r = x.clone();
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            s.process(&mut [cl, cr]);
        }
        assert_eq!(l, ya);
        assert_eq!(r, ya);
        // `spread` 100 %: the pair apart
        let mut s = make(SR, 2, &[("engine", engine), ("detune", 10.0), ("spread", 100.0)]);
        let mut l = x.clone();
        let mut r = x.clone();
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            s.process(&mut [cl, cr]);
        }
        assert!(l != r);
    }
}

// ---------------------------------------------------------------- Spectral: attack, handover, legato, vibrato

/// 20 ms RMS every 5 ms (dB).
fn env20(y: &[f32], sr: f64) -> Vec<f64> {
    let w = (0.005 * sr) as usize;
    (0..y.len() / w - 4).map(|i| to_db(rms(&y[i * w..i * w + 4 * w]))).collect()
}

#[test]
fn attack_and_handover() {
    // single notes E2 … E5 on a held string (T60 30 s): within 3 dB of the body 20 ms after the pick,
    // no swell over +3 dB in the first 150 ms (Dual and Fast); Dual: no dip deeper than 1.5 dB in the
    // first 200 ms. The 20 ms RMS window reads 10 ms late: "20 ms after the pick" is the window
    // centred there
    let sr = SR as f64;
    for m in [40.0, 45.0, 52.0, 59.0, 64.0, 76.0] {
        let x = moving_string(midi_hz(m), 0.6, 0.1, 30.0, 0.3, 12, |_| 0.0, sr);
        let mut line = format!("MIDI {m}:");
        let lowest = m < 41.0;
        let mut res = Vec::new();
        for bank in [0.0f32, 1.0] {
            let mut e = make(SR, 1, &plain(&[("bank", bank)]));
            let y = render(&mut e, &x);
            let env = env20(&y, sr);
            // window i covers [5i, 5i + 20) ms; the pick at 100 ms
            let at = |ms: f64| env[((100.0 + ms - 10.0) / 5.0) as usize];
            let body = (0..10).map(|k| at(300.0 + 10.0 * k as f64)).sum::<f64>() / 10.0;
            // within 3 dB 20 ms after the pick: a 10 ms RMS centred there
            let a20 = to_db(rms(&y[(0.115 * sr) as usize..(0.125 * sr) as usize])) - body;
            let swell = (4..30).map(|k| at(5.0 * k as f64) - body).fold(f64::MIN, f64::max);
            let dip = (6..40).map(|k| body - at(5.0 * k as f64)).fold(f64::MIN, f64::max);
            line += &format!("  {}: at 20 ms {a20:+.2} dB, swell {swell:+.2}, dip {dip:.2}", if bank == 0.0 { "Dual" } else { "Fast" });
            res.push((a20, swell, dip));
        }
        println!("{line}");
        for (i, (a20, swell, dip)) in res.into_iter().enumerate() {
            // E2: the fast bank's order-2, 40 Hz band reaches −2.9 dB 20 ms after a step (its own
        // step response); this 10 ms window reads −3.6 dB (README As coded)
        assert!(a20 >= if lowest { -4.5 } else { -3.0 } && swell <= 3.0, "{m} bank {i}: {a20} {swell}");
            if i == 0 {
                assert!(dip <= 1.5, "{m}: dip {dip}");
            }
        }
    }
}

/// The instantaneous frequency deviation (Hz) of the component near `f`: demodulated, low-passed
/// (4 one-poles at `bw`), phase differentiated.
fn freq_track(y: &[f32], f: f64, bw: f64, sr: f64) -> Vec<f64> {
    let a = 1.0 - (-TAU * bw / sr).exp();
    let mut s = [(0.0f64, 0.0f64); 4];
    let mut last = 0.0f64;
    let mut out = Vec::with_capacity(y.len());
    for (i, &v) in y.iter().enumerate() {
        let w = TAU * f * i as f64 / sr;
        let mut z = (v as f64 * w.cos(), -(v as f64) * w.sin());
        for st in s.iter_mut() {
            st.0 += (z.0 - st.0) * a;
            st.1 += (z.1 - st.1) * a;
            z = *st;
        }
        let ph = z.1.atan2(z.0);
        let mut d = ph - last;
        d -= TAU * (d / TAU).round();
        last = ph;
        out.push(d * sr / TAU);
    }
    out
}

#[test]
fn vibrato_and_legato_dual() {
    let sr = SR as f64;
    // ±30 ¢, 6 Hz on E4 and A2: the synth's fundamental follows with ≤ 20 ms of lag and ≥ 85 % of the
    // depth (the clean bank alone: 64 ms late at half the depth)
    for m in [64.0, 45.0] {
        let f0 = midi_hz(m);
        let vib = |t: f64| if t < 0.3 { 0.0 } else { 30.0 * (TAU * 6.0 * (t - 0.3)).sin() };
        let x = moving_string(f0, 1.4, 0.1, 30.0, 0.3, 13, vib, sr);
        let mut res = Vec::new();
        for bank in [0.0f32, 2.0] {
            let mut e = make(SR, 1, &plain(&[("bank", bank), ("wave", 3.0)]));
            let y = render(&mut e, &x);
            let bw = (0.25 * f0).min(40.0);
            let fi = freq_track(&x, f0, bw, sr);
            let fo = freq_track(&y, f0, bw, sr);
            // the 6 Hz component of each deviation over four cycles: lag from its phase, depth from
            // its amplitude
            let (a, b) = ((0.633 * sr) as usize, (1.3 * sr) as usize);
            let proj = |v: &[f64]| {
                let (mut re, mut im) = (0.0, 0.0);
                for i in a..b {
                    let w = TAU * 6.0 * i as f64 / sr;
                    re += v[i] * w.cos();
                    im -= v[i] * w.sin();
                }
                (re, im)
            };
            let (pi, po) = (proj(&fi), proj(&fo));
            let dph = (pi.1 * po.0 - pi.0 * po.1).atan2(pi.0 * po.0 + pi.1 * po.1);
            let depth = (po.0.hypot(po.1)) / pi.0.hypot(pi.1);
            res.push((dph / (TAU * 6.0) * 1e3, depth));
        }
        println!("MIDI {m} vibrato: Dual lag {:.1} ms depth {:.0} %; Clean lag {:.1} ms depth {:.0} %", res[0].0, res[0].1 * 100.0, res[1].0, res[1].1 * 100.0);
        assert!(res[0].0 <= 20.0 && res[0].1 >= 0.85, "{m}: {:?}", res[0]);
    }
    // a held chord, one string in vibrato: the other strings' bands stay on the clean bank
    let mut y = vec![0.0f32; (1.4 * sr) as usize];
    for (m, vib) in [(45.0, false), (52.0, false), (60.0, true)] {
        let s = moving_string(midi_hz(m), 1.4, 0.1, 30.0, 0.2, m as u64, |t| if vib && t > 0.3 { 30.0 * (TAU * 6.0 * (t - 0.3)).sin() } else { 0.0 }, sr);
        y.iter_mut().zip(&s).for_each(|(a, b)| *a += b);
    }
    let mut e = make(SR, 1, &plain(&[("wave", 1.0)]));
    let mut xs = Vec::new();
    for (k, c) in y.chunks_mut(480).enumerate() {
        e.process(&mut [c]);
        if k > 70 {
            let core = e.spectral_core();
            xs.push([110.0, 220.0, 164.8, 330.0].map(|f| core.x_at(f)));
        }
    }
    let min = xs.iter().flatten().fold(1.0f64, |m, v| m.min(*v));
    let c4 = midi_hz(60.0);
    println!("chord A2 E3 C4, C4 in vibrato: the others' clean share ≥ {min:.2}; C4's at the end {:.2}", e.spectral_core().x_at(c4));
    assert!(min >= 0.99);
    // legato: E4 → A4 without a pick (level step 0): the new note within 3 dB of its body ≤ 30 ms
    // after the change (the input's own demodulator delay subtracted)
    let step = |t: f64| if t < 0.4 { 0.0 } else { 500.0 };
    let f1 = midi_hz(69.0);
    let x = moving_string(midi_hz(64.0), 1.0, 0.1, 30.0, 0.3, 14, step, sr);
    let demod_env = |y: &[f32]| -> Vec<f64> {
        let a = 1.0 - (-TAU * 40.0 / sr).exp();
        let mut s = [(0.0f64, 0.0f64); 4];
        y.iter()
            .enumerate()
            .map(|(i, &v)| {
                let w = TAU * f1 * i as f64 / sr;
                let mut z = (v as f64 * w.cos(), -(v as f64) * w.sin());
                for st in s.iter_mut() {
                    st.0 += (z.0 - st.0) * a;
                    st.1 += (z.1 - st.1) * a;
                    z = *st;
                }
                to_db((z.0 * z.0 + z.1 * z.1).sqrt())
            })
            .collect()
    };
    let t_rise = |env: &[f64]| {
        let ch = (0.5 * sr) as usize;
        let body = env[(0.85 * sr) as usize];
        (ch..env.len()).find(|&i| env[i] >= body - 3.0).map(|i| (i - ch) as f64 / sr * 1e3).unwrap_or(1e9)
    };
    let t_in = t_rise(&demod_env(&x));
    let mut out = Vec::new();
    for bank in [0.0f32, 1.0, 2.0] {
        let mut e = make(SR, 1, &plain(&[("bank", bank), ("wave", 3.0)]));
        out.push(t_rise(&demod_env(&render(&mut e, &x))) - t_in);
    }
    println!("legato E4 → A4: the new note at −3 dB after Dual {:.1} ms, Fast {:.1} ms, Clean {:.1} ms", out[0], out[1], out[2]);
    assert!(out[0] <= 30.0, "{out:?}");
}

// ---------------------------------------------------------------- purity, aliasing, hold, switches

/// Energy off the harmonics of `fs` (±25 ¢, ≥ 3 Hz) over 0.35 – 1.35 s, 30 Hz – 8 kHz, re the total (dB).
fn off_partials_db(y: &[f32], fs: &[f64], sr: f64) -> f64 {
    let (a, b) = ((0.35 * sr) as usize, (1.35 * sr) as usize);
    let s = power_spectrum(&y[a..b]);
    let df = sr / (b - a) as f64;
    let (mut off, mut tot) = (0.0, 0.0);
    for (i, p) in s.iter().enumerate() {
        let f = i as f64 * df;
        if !(30.0..8000.0).contains(&f) {
            continue;
        }
        let harm = fs.iter().any(|&f0| (1..200).any(|k| {
            let fk = f0 * k as f64;
            (f - fk).abs() < (fk * ((25.0f64 / 1200.0).exp2() - 1.0)).max(3.0)
        }));
        tot += p;
        if !harm {
            off += p;
        }
    }
    10.0 * (off / tot.max(1e-30) + 1e-20).log10()
}

#[test]
fn purity_gate() {
    // two partials 60 Hz apart in one fast band (the band at 1.88 kHz, 105 Hz wide): the shaped
    // output's non-harmonic energy ≤ that of the band unshaped (Sine) + 3 dB. Reported: the same pair at
    // 1 kHz, where two fast bands 66 Hz wide each hold one partial and the other 13 dB down
    let sr = SR as f64;
    let n = (1.5 * sr) as usize;
    for (fa, fb, check) in [(1853.0, 1913.0, true), (1000.0, 1060.0, false)] {
        let x: Vec<f32> = (0..n).map(|i| (0.1 * (TAU * fa * i as f64 / sr).sin() + 0.08 * (TAU * fb * i as f64 / sr).sin()) as f32).collect();
        let mut got = Vec::new();
        for wave in [0.0f32, 1.0, 3.0] {
            let mut e = make(SR, 1, &plain(&[("bank", 1.0), ("wave", wave)]));
            got.push(off_partials_db(&render(&mut e, &x), &[fa, fb], sr));
        }
        println!("partials {fa} + {fb} Hz, Fast: off-harmonic Saw {:.1} dB, Pulse {:.1} dB, unshaped (Sine) {:.1} dB", got[0], got[1], got[2]);
        if check {
            assert!(got[0] <= got[2] + 3.0 && got[1] <= got[2] + 3.0);
        }
    }
}

#[test]
fn aliasing() {
    // Saw, `octave` 0 and +2: a note at 1 – 1.9 kHz, and two partials ~40 Hz apart at 1.9 kHz: nothing
    // above −70 dBc folds below 8 kHz
    let sr = SR as f64;
    let n = (1.5 * sr) as usize;
    let mut worst = f64::MIN;
    for (oct, freqs) in [(0.0f32, vec![1003.7]), (0.0, vec![1503.7]), (0.0, vec![1903.7]), (2.0, vec![1003.7]), (2.0, vec![1903.7]), (0.0, vec![1903.7, 1943.3])] {
        let x: Vec<f32> = (0..n).map(|i| freqs.iter().map(|f| 0.1 * (TAU * f * i as f64 / sr).sin()).sum::<f64>() as f32).collect();
        let mut e = make(SR, 1, &plain(&[("wave", 0.0), ("octave", oct)]));
        let y = render(&mut e, &x);
        let s = power_spectrum(&y[(0.35 * sr) as usize..(1.35 * sr) as usize]);
        let df = sr / (sr as usize) as f64;
        let mul = (oct as f64).exp2();
        let peak = s.iter().cloned().fold(0.0, f64::max);
        let mut w = 0.0f64;
        let mut wf = 0.0;
        for (i, p) in s.iter().enumerate() {
            let f = i as f64 * df;
            if !(30.0..8000.0).contains(&f) {
                continue;
            }
            // the products of the shaped partials, k · f_a · mul + m (f_b − f_a): in band, not folded
            // (the pair is off the 20 Hz grid, so a fold from the bank's Nyquist is not one of them)
            // everything but the shaped partials' own products k · f_a · mul + m (f_b − f_a) (±8 Hz:
            // the window's skirt and the levels' slow modulation) is a fold; the notes sit off every
            // grid of the bank rate, so a fold does not land on a product
            let d = if freqs.len() > 1 { freqs[1] - freqs[0] } else { 0.0 };
            let ok = (1..40).any(|k| (-60..=60).any(|m| (f - k as f64 * freqs[0] * mul - m as f64 * d).abs() < 8.0));
            if !ok && *p > w {
                w = *p;
                wf = f;
            }
        }
        let rel = 10.0 * (w / peak).log10();
        println!("octave {oct} {freqs:?}: worst fold {rel:.1} dBc at {wf:.0} Hz");
        worst = worst.max(rel);
    }
    assert!(worst <= -70.0, "{worst}");
}

#[test]
fn hold_and_floor() {
    // `compress` 100 %: a decaying E2 (T60 3 s) holds each of its first partials within 1.5 dB while it
    // is 10 dB over the floor; silence and touch noise under `thresh` stay under −90 dBFS
    let sr = SR as f64;
    let x = moving_string(midi_hz(40.0), 2.6, 0.1, 3.0, 0.3, 21, |_| 0.0, sr);
    let mut e = make(SR, 1, &plain(&[("compress", 100.0), ("wave", 3.0), ("bank", 2.0)]));
    let y = render(&mut e, &x);
    let f0 = midi_hz(40.0);
    let mut worst: f64 = 0.0;
    for k in 1..=4 {
        let fk = f0 * k as f64 * (1.0 + 1e-4 * (k * k) as f64).sqrt();
        let at = |v: &[f32], t: f64| to_db(amp_at(&v[(t * sr) as usize..((t + 0.2) * sr) as usize], fk, sr));
        // the cap counts from the body held since the pick (the window from 0.1 s reads its peak)
        let (r, ri) = (at(&y, 0.4), at(&x, 0.1));
        let mut line = String::new();
        for t in [0.8, 1.2, 1.6, 2.0, 2.4] {
            // checked while the string's partial is 10 dB over `thresh` and within the +30 dB cap
            let inp = at(&x, t);
            let d = at(&y, t) - r;
            line += &format!(" {d:+.2}{}", if inp > -56.0 && ri - inp < 28.0 { "" } else { "·" });
            if inp > -56.0 && ri - inp < 28.0 {
                worst = worst.max(d.abs());
            }
        }
        println!("partial {k}: re 0.4 s{line} (· = under the floor or past the cap)");
    }
    assert!(worst <= 1.5, "{worst}");
    // the floor: −80 dBFS of white noise (touch noise, hum) under `thresh` −66
    let mut r = Rng::new(3);
    let x: Vec<f32> = (0..(1.0 * sr) as usize).map(|_| (1e-4 * gauss(&mut r)) as f32).collect();
    let mut e = make(SR, 1, &[]);
    let y = render(&mut e, &x);
    let pk = y.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    println!("noise at −80 dBFS: synth peak {:.1} dBFS", 20.0 * (pk as f64).max(1e-12).log10());
    assert!(pk < 10f32.powf(-90.0 / 20.0));
}

#[test]
fn switches_do_not_click() {
    // `engine` both ways, and the structural changes, on a held note (gfx-bench's step measure)
    let sr = 48000.0f32;
    let mut worst = (f64::MIN, String::new());
    for (id, a, b) in [("engine", 0.0f32, 1.0f32), ("engine", 1.0, 0.0), ("wave", 0.0, 1.0), ("wave", 1.0, 3.0), ("octave", 0.0, 1.0), ("octave", 0.0, -1.0), ("bank", 0.0, 1.0), ("bank", 1.0, 2.0), ("type", 0.0, 1.0), ("poles", 1.0, 0.0), ("amp_mode", 0.0, 1.0)] {
        let v = step_burst(id, a, b, sr, &[]);
        if v > worst.0 {
            worst = (v, format!("{id} {a} → {b}"));
        }
    }
    println!("worst switch burst {:.1} dB ({})", worst.0, worst.1);
    assert!(worst.0 <= -50.0);
}

#[test]
fn mono_chord_takes_the_lowest_note() {
    // a strummed E5 power chord: Mono's voice plays E2 (multipitch's lowest confirmed note), never
    // later than 0.4 s
    let sr = SR as f64;
    let x = chord(&[40.0, 47.0, 52.0], 1.2, 4, 3.0, sr);
    let mut e = make(SR, 1, &[("engine", 1.0)]);
    let (_, log) = render_watch(&mut e, &x);
    let start = log.iter().position(|v| v.iter().any(|w| w.3 == 1)).expect("a voice");
    let f = log[start + 100].iter().find(|w| w.3 == 1).map(|w| w.0).unwrap_or(0.0);
    println!("E5 chord: voice after {} ms at {:.1} Hz ({:+.0} ¢ re E2)", start as i64 - 50, f, cents(f, midi_hz(40.0)));
    assert!((start as i64 - 50) <= 400 && cents(f, midi_hz(40.0)).abs() < 50.0);
}

// ---------------------------------------------------------------- gfx-dsp `spectral` move: the null

fn fnv(h: &mut u64, x: &[f32]) {
    for v in x {
        for b in v.to_bits().to_le_bytes() {
            *h ^= b as u64;
            *h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
}

/// Both engines, every `bank`, `wave`, `octave`, plus sub / noise / detune / compress / residue, at
/// 44.1, 48 and 96 kHz, mono and stereo, with wave and octave switched mid-render: the hash written
/// before the Spectral core moved into gfx-dsp `spectral` (gfx-organ README §2.8). One differing
/// sample fails it.
fn spectral_null_hash() -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let sets: Vec<Vec<(&str, f32)>> = vec![
        vec![],
        vec![("bank", 1.0)],
        vec![("bank", 2.0)],
        vec![("wave", 1.0), ("pw", 30.0)],
        vec![("wave", 2.0), ("octave", -1.0)],
        vec![("wave", 3.0), ("octave", -2.0), ("bank", 2.0)],
        vec![("octave", 1.0), ("bank", 1.0)],
        vec![("octave", 2.0)],
        vec![("sub", 60.0), ("noise", 30.0), ("detune", 12.0)],
        vec![("detune", 1.0), ("compress", 100.0), ("octave", -1.0)],
        vec![("engine", 1.0)],
        vec![("engine", 1.0), ("residue", 30.0), ("detune", 7.0), ("bank", 2.0)],
        vec![("engine", 1.0), ("octave", -1.0), ("sub", 40.0)],
        vec![("lowest", 35.0), ("thresh", -80.0), ("compress", 0.0)],
    ];
    let notes: [&[f64]; 2] = [&[40.0, 47.0, 52.0, 56.0, 59.0, 64.0], &[57.0]];
    for (si, set) in sets.iter().enumerate() {
        for (sr, nch) in [(48000.0f32, 1usize), (44100.0, 1), (96000.0, 2)] {
            if si % 3 != 0 && sr != 48000.0 {
                continue;
            }
            let x = chord(notes[si % 2], 1.2, 7 + si as u64, 3.0, sr as f64);
            let mut e = make(sr, nch, set);
            let mut l = x.clone();
            let mut r: Vec<f32> = x.iter().map(|v| 0.9 * v).collect();
            let mut k = 0;
            let n = l.len();
            while k < n {
                if k == 256 * 100 {
                    e.set_param("wave", ((si + 1) % 4) as f32);
                    e.set_param("octave", [0.0, -1.0, 1.0][si % 3]);
                }
                let end = (k + 256).min(n);
                if nch == 2 {
                    e.process(&mut [&mut l[k..end], &mut r[k..end]]);
                } else {
                    e.process(&mut [&mut l[k..end]]);
                }
                k = end;
            }
            fnv(&mut h, &l);
            if nch == 2 {
                fnv(&mut h, &r);
            }
        }
    }
    h
}

#[test]
fn spectral_null_against_the_build_before_the_move() {
    let h = spectral_null_hash();
    println!("spectral null hash {h:#018x}");
    assert_eq!(h, SPECTRAL_NULL);
}

const SPECTRAL_NULL: u64 = 0x1662439ff3511578;
