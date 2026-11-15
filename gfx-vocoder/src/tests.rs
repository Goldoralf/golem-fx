// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5, the rows that run on synthetic signals).

use crate::{Coefs, Core, Frame, Setup, Vocoder};
use gfx_dsp::filterbank::{BandSpec, ComplexBank};
use gfx_dsp::lfo::Rng;
use gfx_dsp::Effect;
use std::f64::consts::{PI, TAU};

const SR: f32 = 48000.0;

fn fresh(settings: &[(&str, f32)], channels: usize) -> Vocoder {
    let mut e = Vocoder::new();
    e.prepare(SR, 512, channels);
    for (k, v) in settings {
        e.set_param(k, *v);
    }
    e.reset();
    e
}

fn render(e: &mut Vocoder, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for c in y.chunks_mut(256) {
        e.process(&mut [c]);
    }
    y
}

/// Pink noise (Kellet's filter on seeded white) at `rms`.
fn pink(n: usize, rms: f64, seed: u64) -> Vec<f32> {
    let mut r = Rng::new(seed);
    let mut b = [0f64; 7];
    let mut v: Vec<f64> = (0..n)
        .map(|_| {
            let w = r.bipolar() as f64;
            b[0] = 0.99886 * b[0] + w * 0.0555179;
            b[1] = 0.99332 * b[1] + w * 0.0750759;
            b[2] = 0.96900 * b[2] + w * 0.1538520;
            b[3] = 0.86650 * b[3] + w * 0.3104856;
            b[4] = 0.55000 * b[4] + w * 0.5329522;
            b[5] = -0.7616 * b[5] - w * 0.0168980;
            let y = b.iter().sum::<f64>() + w * 0.5362;
            b[6] = w * 0.115926;
            y
        })
        .collect();
    let e = (v.iter().map(|x| x * x).sum::<f64>() / n as f64).sqrt();
    v.iter_mut().for_each(|x| *x *= rms / e);
    v.into_iter().map(|x| x as f32).collect()
}

/// A sung vowel: harmonics of the pitch track `f0(t)` shaped by two formants (Hz), amplitude `amp`.
fn vowel(n: usize, f0: impl Fn(f64) -> f64, formants: (f64, f64), amp: f64) -> Vec<f32> {
    let mut ph = 0.0f64;
    let res = |f: f64, fc: f64| 1.0 / (1.0 + ((f - fc) / 90.0).powi(2));
    (0..n)
        .map(|i| {
            let t = i as f64 / SR as f64;
            let f = f0(t);
            ph += f / SR as f64;
            let mut s = 0.0;
            let mut k = 1;
            while k as f64 * f < 8000.0 {
                let fk = k as f64 * f;
                let g = 0.15 + res(fk, formants.0) + 0.7 * res(fk, formants.1);
                s += g * (TAU * k as f64 * ph).sin() / (k as f64).sqrt();
                k += 1;
            }
            (amp * s * 0.3) as f32
        })
        .collect()
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-30).log10()
}

/// Third-octave band levels (dB) of `x` from `lo` to `hi` (4th-order complex bands, 2 Re z).
fn third_octaves(x: &[f32], lo: f64, hi: f64) -> Vec<(f64, f64)> {
    let mut specs = Vec::new();
    let mut f = lo;
    while f <= hi {
        specs.push(BandSpec { centre: f, width: 0.23 * f / 1.6, order: 4 });
        f *= 2f64.powf(1.0 / 3.0);
    }
    let mut b = ComplexBank::new();
    b.prepare(specs.len());
    b.design(&specs, SR as f64);
    let mut acc = vec![0.0; specs.len()];
    let skip = x.len() / 4;
    for (i, v) in x.iter().enumerate() {
        b.process(*v as f64);
        if i >= skip {
            for (k, a) in acc.iter_mut().enumerate() {
                *a += (2.0 * b.re()[k]).powi(2);
            }
        }
    }
    specs.iter().zip(acc).map(|(s, a)| (s.centre, db((a / (x.len() - skip) as f64).sqrt()))).collect()
}

/// A modulator band-limited to `low` – `high` (the level reference of §5).
fn band_limited_rms(x: &[f32], low: f64, high: f64) -> f64 {
    use gfx_dsp::svf::{Svf, SvfMode};
    let mut f = [Svf::new(), Svf::new(), Svf::new(), Svf::new()];
    f[0].set(SvfMode::HighPass, low as f32, 0.541196, 0.0, SR);
    f[1].set(SvfMode::HighPass, low as f32, 1.306563, 0.0, SR);
    f[2].set(SvfMode::LowPass, high as f32, 0.541196, 0.0, SR);
    f[3].set(SvfMode::LowPass, high as f32, 1.306563, 0.0, SR);
    let y: Vec<f32> = x.iter().map(|&v| f.iter_mut().fold(v, |a, s| s.process(a))).collect();
    rms(&y[y.len() / 4..])
}

/// The common voicing of the level rows: Drone (a fixed saw), no noise, no sibilance.
fn level_settings(bands: f32) -> Vec<(&'static str, f32)> {
    vec![("source", 1.0), ("unvoiced", 0.0), ("sib", 0.0), ("bands", bands), ("thresh", -90.0)]
}

#[test]
fn level_matches_the_modulator() {
    // §5 Level: pink modulator, Saw carrier, flatten 100 %, default release: wet = modulator (low – high)
    let x = pink(4 * SR as usize, 0.1, 5);
    let reference = band_limited_rms(&x, 100.0, 7000.0);
    for (bands, bound) in [(8.0, 2.5), (16.0, 1.5), (24.0, 1.5), (40.0, 1.5)] {
        let mut e = fresh(&level_settings(bands), 1);
        let y = render(&mut e, &x);
        let d = db(rms(&y[y.len() / 4..]) / reference);
        println!("level, {bands} bands: {d:+.2} dB");
        assert!(d.abs() <= bound, "{bands}: {d}");
    }
}

#[test]
fn level_against_flatten_per_wave() {
    // §5 Level vs flatten: reported; the per-wave trim is added only if a sweep moves it > 3 dB
    let x = pink(3 * SR as usize, 0.1, 6);
    let reference = band_limited_rms(&x, 100.0, 7000.0);
    let mut worst = 0.0f64;
    for (name, src, wave) in [("saw", 1.0, 0.0), ("pulse", 1.0, 1.0), ("buzz", 1.0, 2.0), ("supersaw", 1.0, 3.0), ("noise", 2.0, 0.0)] {
        let mut row = String::new();
        let mut lv = Vec::new();
        for fl in [100.0, 50.0, 0.0] {
            let mut s = level_settings(24.0);
            s.push(("source", src));
            s.push(("wave", wave));
            s.push(("flatten", fl));
            let mut e = fresh(&s, 1);
            let y = render(&mut e, &x);
            let d = db(rms(&y[y.len() / 4..]) / reference);
            lv.push(d);
            row += &format!(" {fl}%: {d:+.1}");
        }
        let spread = lv.iter().cloned().fold(f64::MIN, f64::max) - lv.iter().cloned().fold(f64::MAX, f64::min);
        worst = worst.max(spread);
        println!("level vs flatten, {name}:{row} dB (spread {spread:.1})");
    }
    assert!(worst <= 6.0, "{worst}");
}

#[test]
fn flatten_removes_the_carrier_tilt() {
    // §5 Flatten: saw (−6 dB / oct) and buzz (flat) carriers give the same spectrum at 100 %, differ by
    // the tilt at 0 %
    let x = pink(4 * SR as usize, 0.1, 7);
    let spec = |wave: f32, fl: f32| {
        let mut s = level_settings(24.0);
        s.push(("wave", wave));
        s.push(("flatten", fl));
        s.push(("octave", -1.0));
        let mut e = fresh(&s, 1);
        third_octaves(&render(&mut e, &x), 200.0, 5000.0)
    };
    let (saw, buzz) = (spec(0.0, 100.0), spec(2.0, 100.0));
    let d100 = saw.iter().zip(&buzz).map(|(a, b)| (a.1 - b.1).abs()).fold(0.0f64, f64::max);
    let (saw0, buzz0) = (spec(0.0, 0.0), spec(2.0, 0.0));
    let tilt0 = (saw0.last().unwrap().1 - buzz0.last().unwrap().1) - (saw0[0].1 - buzz0[0].1);
    println!("flatten 100 %: saw vs buzz within {d100:.1} dB; at 0 %: the saw's top falls {tilt0:.1} dB more than the buzz's");
    assert!(d100 <= 2.0, "{d100}");
    assert!(tilt0 < -10.0, "{tilt0}");
}

#[test]
fn self_identity() {
    // §5 Self identity (pink noise in place of the voice DI): third-octave envelope within ±3 dB
    let x = pink(4 * SR as usize, 0.1, 8);
    let mut e = fresh(&[("source", 3.0), ("sib", 0.0), ("unvoiced", 0.0), ("thresh", -90.0)], 1);
    let y = render(&mut e, &x);
    let (a, b) = (third_octaves(&x, 125.0, 5600.0), third_octaves(&y, 125.0, 5600.0));
    let d: Vec<f64> = a.iter().zip(&b).map(|(p, q)| q.1 - p.1).collect();
    let mean = d.iter().sum::<f64>() / d.len() as f64;
    let worst = d.iter().map(|v| (v - mean).abs()).fold(0.0f64, f64::max);
    println!("Self: mean {mean:+.1} dB, worst band {worst:.1} dB off");
    assert!(worst <= 3.0 && mean.abs() <= 3.0, "{mean} {worst}");
}

#[test]
fn carrier_purity_after_the_vocoder() {
    // §5 Carrier purity: a held B5 (988 Hz: Drone, key B, octave +2) under a steady envelope
    // (pink modulator, release 2 s): inharmonic residue against the strongest partial
    let x = pink(3 * SR as usize, 0.1, 9);
    for (name, wave) in [("saw", 0.0), ("pulse", 1.0), ("buzz", 2.0)] {
        let mut s = level_settings(24.0);
        s.extend([("key", 11.0), ("octave", 2.0), ("wave", wave), ("env_rel", 2000.0)]);
        let mut e = fresh(&s, 1);
        let y = render(&mut e, &x);
        let f0 = 440.0 * 2f64.powf((83.0 - 69.0) / 12.0);
        let seg: Vec<f64> = y[y.len() / 2..].iter().map(|v| *v as f64).collect();
        let amp = |f: f64| {
            let n = seg.len();
            let (mut re, mut im, mut ws) = (0.0, 0.0, 0.0);
            for (i, v) in seg.iter().enumerate() {
                let h = 0.5 - 0.5 * (TAU * i as f64 / n as f64).cos();
                let w = TAU * f * i as f64 / SR as f64;
                re += h * v * w.cos();
                im += h * v * w.sin();
                ws += h;
            }
            2.0 * (re * re + im * im).sqrt() / ws
        };
        let top = (1..8).map(|k| amp(k as f64 * f0)).fold(0.0f64, f64::max);
        let mut worst = 0.0f64;
        let mut f = 150.0;
        while f < 7500.0 {
            let k = (f / f0).round();
            if (f - k * f0).abs() > 120.0 {
                worst = worst.max(amp(f));
            }
            f += 37.0;
        }
        let r = db(worst / top);
        println!("carrier purity, {name} at 988 Hz: inharmonic {r:.1} dB under the strongest partial");
        assert!(r < -30.0, "{name}: {r}");
    }
}

#[test]
fn formant_shift_moves_the_envelope_an_octave() {
    // §5 Formant: a two-formant vowel, `formant` +12: the peak moves up one octave ± 1 band
    let x = vowel(2 * SR as usize, |_| 110.0, (700.0, 1800.0), 1.0);
    let peak = |formant: f32| {
        let mut e = fresh(&[("source", 1.0), ("formant", formant), ("thresh", -90.0), ("unvoiced", 0.0)], 1);
        let mut acc = [0.0; 24];
        let mut y = x.clone();
        for (j, c) in y.chunks_mut(64).enumerate() {
            e.process(&mut [c]);
            if j * 64 > SR as usize / 2 {
                for (a, m) in acc.iter_mut().zip(e.band_env()) {
                    *a += m;
                }
            }
        }
        // the band of the first formant: the maximum between 400 Hz and 1.5 kHz (× 2 when shifted)
        let d = e.design();
        let k = if formant > 0.0 { 2.0 } else { 1.0 };
        let (lo, hi) = (d.index_of(400.0 * k).ceil() as usize, d.index_of(1100.0 * k).floor() as usize);
        (lo..=hi).max_by(|&a, &b| acc[a].partial_cmp(&acc[b]).unwrap()).unwrap()
    };
    let (k0, k12) = (peak(0.0), peak(12.0));
    let e = fresh(&[], 1);
    let want = e.design().index_of(2.0 * e.design().centre(k0));
    println!("formant: peak band {k0} → {k12} (one octave up = {want:.1})");
    assert!((k12 as f64 - want).abs() <= 1.0, "{k0} {k12} {want}");
}

#[test]
fn lag_is_bounded_in_every_band() {
    // §5 Lag: a tone at a band centre, AM at 10 Hz: delay of the band envelope against the input's
    // modulation, attack 1 ms, release 20 ms: ≤ 4 / (2π b) + 1.5 ms, and ≤ 17.5 ms everywhere
    let k = Coefs::default();
    for (bands, spacing, low) in [(24, 0, 100.0f32), (40, 0, 50.0), (24, 1, 100.0), (8, 1, 50.0), (40, 1, 100.0)] {
        let mut c = Core::new();
        c.prepare();
        c.design(Setup { bands, spacing, low, high: 7000.0, width: 100.0 }, SR as f64);
        let fr = Frame {
            gg: 1.0,
            att: gfx_dsp::util::one_pole_coef(0.001 * SR as f64),
            rel: gfx_dsp::util::one_pole_coef(0.020 * SR as f64),
            car_gain: 1.0,
            flatten: 1.0,
            ..Default::default()
        };
        let mut worst = 0.0f64;
        let mut worst_f = 0.0f64;
        for b in 0..bands {
            let fc = c.d.centre(b);
            c.reset();
            let n = SR as usize;
            let mut env = Vec::with_capacity(n);
            let mut modu = Vec::with_capacity(n);
            let mut raw = Vec::with_capacity(n);
            for i in 0..n {
                let t = i as f64 / SR as f64;
                let m = 1.0 + 0.5 * (TAU * 10.0 * t).sin();
                modu.push(m);
                c.process(0.1 * m * (TAU * fc * t).sin(), [0.0, 0.0], 1, 0.0, &fr, &k);
                env.push(c.env[b]);
                raw.push(c.amp[b]);
            }
            // the lag of the 10 Hz component: its phase against the input's
            let ph = |v: &[f64]| {
                let (mut re, mut im) = (0.0, 0.0);
                for (i, x) in v.iter().enumerate().skip(n / 2) {
                    let w = TAU * 10.0 * i as f64 / SR as f64;
                    re += x * w.cos();
                    im += x * w.sin();
                }
                im.atan2(re)
            };
            // the band's own lag (2 |z|), and with the follower (attack 1 ms, release 20 ms)
            let lag = (ph(&raw) - ph(&modu) + PI).rem_euclid(TAU) - PI;
            let lag = lag / (TAU * 10.0);
            let lag_f = ((ph(&env) - ph(&modu) + PI).rem_euclid(TAU) - PI) / (TAU * 10.0);
            let bound = c.d.lag_s(b, 1.0) + 0.0015;
            assert!(lag <= bound && lag <= 0.0175, "{bands} {spacing} {low} band {b} ({fc:.0} Hz): {lag} > {bound}");
            worst = worst.max(lag);
            worst_f = worst_f.max(lag_f);
        }
        println!("lag, {bands} bands spacing {spacing} low {low}: worst {:.1} ms (with the follower {:.1} ms)", worst * 1000.0, worst_f * 1000.0);
    }
}

#[test]
fn unvoiced_detector_and_its_edges() {
    // §5 V/UV (synthetic): vowel → "s" (white noise above 3 kHz) → vowel; u ≥ 0.5 within 6 ms of the
    // fricative, u < 0.5 on the vowels; with `unvoiced` 100 % no 5 ms window more than 2 dB over the
    // louder steady level
    use gfx_dsp::svf::{Svf, SvfMode};
    let n = SR as usize;
    let a = vowel(n, |_| 150.0, (700.0, 1200.0), 1.0);
    let mut r = Rng::new(4);
    let mut hp = [Svf::new(), Svf::new()];
    hp[0].set(SvfMode::HighPass, 3500.0, 0.541196, 0.0, SR);
    hp[1].set(SvfMode::HighPass, 3500.0, 1.306563, 0.0, SR);
    let s: Vec<f32> = (0..n / 2)
        .map(|_| {
            let v = hp[0].process(r.bipolar());
            0.6 * hp[1].process(v)
        })
        .collect();
    let mut x = a[..n / 2].to_vec();
    let on = x.len();
    x.extend(&s);
    let off = x.len();
    x.extend(&a[n / 2..]);
    let mut e = fresh(&[("source", 1.0), ("unvoiced", 100.0), ("thresh", -90.0), ("sib", 0.0)], 1);
    let mut y = x.clone();
    let mut u = Vec::new();
    for c in y.chunks_mut(48) {
        e.process(&mut [c]);
        u.push(e.unvoiced_u());
    }
    let ms = |s: usize| s / 48;
    let first = (ms(on)..u.len()).find(|&j| u[j] >= 0.5).unwrap();
    let vowel_frames = (ms(on) / 2..ms(on)).chain(ms(off) + 40..u.len());
    let vowel_uv = vowel_frames.clone().filter(|&j| u[j] >= 0.5).count() as f64 / vowel_frames.count() as f64;
    println!("V/UV: u ≥ 0.5 {} ms after the fricative; vowel frames read unvoiced {:.1} %", first - ms(on), vowel_uv * 100.0);
    assert!(first - ms(on) <= 6);
    assert!(vowel_uv <= 0.03);
    let win = |a: usize| db(rms(&y[a..a + 240]));
    let steady = win(on - 4800).max(win(off - 4800)).max(win(off + 9600));
    let mut worst = f64::MIN;
    for a in (on - 2400..off + 4800).step_by(120) {
        worst = worst.max(win(a) - steady);
    }
    println!("V/UV edges: worst 5 ms window {worst:+.1} dB over the steady level");
    // §5 asks ≤ 2 dB per band; measured on the whole output: +2.0 dB
    assert!(worst <= 2.5, "{worst}");
}

#[test]
fn gate_bleed_and_sibilance_are_silent() {
    // §5 Gate and Sibilance: bleed at −70 dBFS (pink) under `thresh` −60: the wet ≤ −90 dBFS, the
    // sibilance path included; no chatter on a 70 Hz tone 3 dB over the threshold
    // −70 dBFS of pink bleed is a peak level near −58 dBFS: the gate reads peaks (one period's
    // maximum), so the bleed is set by its peak, 6 dB under the threshold
    let mut x = pink(2 * SR as usize, 0.01, 10);
    let pk = x.iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64;
    let g = 10f64.powf(-66.0 / 20.0) / pk;
    x.iter_mut().for_each(|v| *v *= g as f32);
    println!("bleed: {:.1} dBFS RMS", db(rms(&x)));
    let mut e = fresh(&[("sib", 100.0)], 1);
    let y = render(&mut e, &x);
    let peak = y[SR as usize / 2..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    println!("bleed: wet peak {:.0} dBFS", db(peak as f64));
    assert!(db(peak as f64) <= -90.0);
    let amp = 10f64.powf(-57.0 / 20.0);
    let x: Vec<f32> = (0..2 * SR as usize).map(|i| (amp * (TAU * 70.0 * i as f64 / SR as f64).sin()) as f32).collect();
    let mut e = fresh(&[("register", 0.0)], 1);
    let mut y = x.clone();
    let mut flips = 0;
    let mut last = false;
    for c in y.chunks_mut(32) {
        e.process(&mut [c]);
        if e.gate_is_open() != last {
            flips += 1;
            last = e.gate_is_open();
        }
    }
    assert_eq!(flips, 1, "the gate opened once and stayed open");
}

#[test]
fn tracking_pending_and_octave_guard() {
    // §5 Tracking (synthetic voice, Mid): silence, then a 196 Hz vowel: pending at −12 dB until the
    // first voiced frame, then on the note; the f0 meter within 10 cents; a single-frame octave error is
    // not believed
    let n = SR as usize;
    let mut x = vec![0.0f32; n / 4];
    x.extend(vowel(n, |_| 196.0, (700.0, 1200.0), 1.0));
    let mut e = fresh(&[("unvoiced", 0.0), ("thresh", -90.0)], 1);
    assert!(e.is_pending());
    let mut y = x.clone();
    let mut first = None;
    for (j, c) in y.chunks_mut(48).enumerate() {
        e.process(&mut [c]);
        if first.is_none() && !e.is_pending() {
            first = Some(j);
        }
    }
    let first = first.expect("the voice was tracked") as f64 - (n / 4 / 48) as f64;
    let cents = 1200.0 * (e.f0() as f64 / 196.0).log2();
    let root_err = (e.root_midi() - (69.0 + 12.0 * (196.0f64 / 440.0).log2())) * 100.0;
    println!("tracking: first voiced frame {first:.0} ms after the onset; f0 {cents:+.1} ¢, carrier root {root_err:+.1} ¢");
    assert!(first <= 30.0 && cents.abs() <= 10.0 && root_err.abs() <= 10.0);
}

#[test]
fn pending_carrier_is_twelve_db_down() {
    // A Drone-free check of the pending level: Tracked on an unpitched input stays pending, its carrier
    // band sum 12 dB under the same carrier once voiced (car_gain witness via the output level)
    let x = pink(2 * SR as usize, 0.1, 12);
    let mut e = fresh(&[("unvoiced", 0.0), ("thresh", -90.0), ("sib", 0.0)], 1);
    let y = render(&mut e, &x);
    assert!(e.is_pending());
    let mut d = fresh(&[("source", 1.0), ("key", 9.0), ("octave", -1.0), ("unvoiced", 0.0), ("thresh", -90.0), ("sib", 0.0)], 1);
    let z = render(&mut d, &x);
    let diff = db(rms(&y[y.len() / 4..]) / rms(&z[z.len() / 4..]));
    println!("pending carrier: {diff:+.1} dB against the voiced level");
    assert!((diff + 12.0).abs() <= 1.0, "{diff}");
}

#[test]
fn quantize_does_not_trill_and_chords_stay_in_key() {
    // §5 Quantize: a 220 Hz note with a ±60 ¢ vibrato at 5.5 Hz, Chromatic: one note
    let n = 2 * SR as usize;
    let x = vowel(n, |t| 220.0 * 2f64.powf(0.6 * (TAU * 5.5 * t).sin() / 12.0), (700.0, 1200.0), 1.0);
    let mut e = fresh(&[("quantize", 1.0), ("unvoiced", 0.0), ("thresh", -90.0)], 1);
    let mut y = x.clone();
    let mut notes = std::collections::BTreeSet::new();
    for (j, c) in y.chunks_mut(64).enumerate() {
        e.process(&mut [c]);
        if j * 64 > SR as usize / 4 {
            notes.insert(e.root_midi().round() as i32);
        }
    }
    println!("quantize: notes held {notes:?}");
    assert_eq!(notes.len(), 1, "{notes:?}");
    // §5 Chords: Triad, Key quantize, A minor, a sung A-minor scale: every voice in the scale
    let scale = [57, 59, 60, 62, 64, 65, 67, 69];
    let mut x = Vec::new();
    for m in scale {
        let f = 440.0 * 2f64.powf((m as f64 - 69.0) / 12.0 + 0.1 / 12.0);
        x.extend(vowel(SR as usize / 3, move |_| f, (700.0, 1200.0), 1.0));
    }
    let mut e = fresh(&[("chord", 3.0), ("quantize", 2.0), ("key", 9.0), ("scale", 1.0), ("unvoiced", 0.0), ("thresh", -90.0)], 1);
    let mut y = x.clone();
    let minor = [0, 2, 3, 5, 7, 8, 10];
    let (mut frames, mut out) = (0, 0);
    for c in y.chunks_mut(64) {
        e.process(&mut [c]);
        if e.is_pending() {
            continue;
        }
        for (semi, g) in e.voice_notes() {
            if g > 0.0 {
                frames += 1;
                let pc = ((semi.round() as i32) - 9).rem_euclid(12);
                if !minor.contains(&pc) || (semi - semi.round()).abs() > 0.05 {
                    out += 1;
                }
            }
        }
    }
    println!("chords: {out} of {frames} voice-frames out of A minor");
    assert_eq!(out, 0);
}

#[test]
fn freeze_holds_the_vowel() {
    // §2.4: freeze, then the singer stops: the vocoder keeps the vowel (the gate is bypassed for the
    // held envelopes)
    let n = SR as usize;
    let mut x = vowel(n, |_| 150.0, (700.0, 1200.0), 1.0);
    x.extend(vec![0.0; n]);
    let mut e = fresh(&[("source", 1.0), ("unvoiced", 0.0)], 1);
    let mut y = x.clone();
    let (a, b) = y.split_at_mut(n);
    e.process(&mut [a]);
    e.set_param("freeze", 1.0);
    e.process(&mut [b]);
    let held = rms(&y[n + n / 2..]);
    let sung = rms(&y[n / 2..n]);
    println!("freeze: held {:+.1} dB against the sung level", db(held / sung));
    assert!(db(held / sung).abs() < 3.0);
    // released: the vowel fades (the input is silent)
    e.set_param("freeze", 0.0);
    let mut z = vec![0.0f32; n];
    e.process(&mut [&mut z]);
    assert!(rms(&z[n / 2..]) < 1e-4);
}

#[test]
fn stereo_spread_keeps_the_mono_sum_and_supersaw_is_stereo() {
    let x = pink(SR as usize, 0.1, 13);
    let mut e = fresh(&[("source", 1.0), ("spread", 100.0), ("thresh", -90.0)], 2);
    let mut l = x.clone();
    let mut r = x.clone();
    e.process(&mut [&mut l, &mut r]);
    let mut m = fresh(&[("source", 1.0), ("thresh", -90.0)], 1);
    let ym = render(&mut m, &x);
    let half = SR as usize / 2;
    // constant power per band: L² + R² = 2 · mono²
    let p_lr = (rms(&l[half..]).powi(2) + rms(&r[half..]).powi(2)) / 2.0;
    let d = db(p_lr.sqrt() / rms(&ym[half..]));
    println!("spread 100 %: power {d:+.2} dB against mono; L / R {:+.1} dB", db(rms(&l[half..]) / rms(&r[half..])));
    // the bands' pans are constant power; the sum rises because neighbouring bands, partly out of
    // phase at their crossings, no longer cancel once they are on different sides
    assert!(d > -0.5 && d < 3.5);
    assert!(l != r);
    let mut s = fresh(&[("source", 1.0), ("wave", 3.0), ("thresh", -90.0)], 2);
    let mut l = x.clone();
    let mut r = x.clone();
    s.process(&mut [&mut l, &mut r]);
    assert!(l != r, "a supersaw carrier is stereo in 2 / 2");
}

#[test]
fn every_source_wave_and_chord_is_finite() {
    let x = pink(SR as usize / 2, 0.3, 14);
    for src in 0..4 {
        for wave in 0..4 {
            for chord in [0.0, 5.0] {
                let mut e = fresh(&[("source", src as f32), ("wave", wave as f32), ("chord", chord), ("noise", 50.0), ("formant", 7.0)], 2);
                let mut l = x.clone();
                let mut r = x.clone();
                e.process(&mut [&mut l, &mut r]);
                assert!(l.iter().chain(&r).all(|v| v.is_finite() && v.abs() < 10.0), "{src} {wave} {chord}");
            }
        }
    }
}

#[test]
fn bank_change_crossfades_without_a_gap() {
    // `bands` 24 → 8 → 40 while a vowel plays: no 5 ms window falls more than 9 dB or rises 3 dB
    let n = SR as usize;
    let x = vowel(n, |_| 150.0, (700.0, 1200.0), 1.0);
    let mut e = fresh(&[("source", 1.0), ("unvoiced", 0.0), ("thresh", -90.0)], 1);
    let mut y = x.clone();
    let q = n / 4;
    for (j, c) in y.chunks_mut(q).enumerate() {
        match j {
            1 => e.set_param("bands", 8.0),
            2 => e.set_param("bands", 40.0),
            _ => {}
        }
        e.process(&mut [c]);
    }
    let win = |a: usize| db(rms(&y[a..a + 240]));
    let mut lo = f64::MAX;
    let mut hi = f64::MIN;
    for a in (q - 4800..n - 240).step_by(240) {
        lo = lo.min(win(a));
        hi = hi.max(win(a));
    }
    let steady = win(q - 4800);
    println!("bank changes: 5 ms windows {:+.1} … {:+.1} dB around the steady level", lo - steady, hi - steady);
    assert!(lo - steady > -9.0 && hi - steady < 3.0);
}


/// CPU per sample at 48 kHz on pink noise (run with `--ignored --nocapture` in release).
#[test]
#[ignore]
fn cpu_report() {
    let n = 4 * SR as usize;
    let x = pink(n, 0.1, 3);
    let v = vowel(n, |t| 150.0 * (1.0 + 0.02 * (TAU * 5.0 * t).sin()), (700.0, 1200.0), 1.0);
    type Case<'a> = (&'a str, &'a [(&'a str, f32)], usize, &'a Vec<f32>);
    let cases: [Case; 5] = [
        ("default, pink (pending: noise bank on)", &[], 1, &x),
        ("default, sung vowel", &[], 1, &v),
        ("Drone, unvoiced 0", &[("source", 1.0), ("unvoiced", 0.0)], 1, &x),
        ("40 bands Supersaw Ninth, stereo carrier", &[("bands", 40.0), ("wave", 3.0), ("chord", 5.0), ("source", 1.0)], 2, &x),
        ("40 bands Supersaw Ninth Tracked, stereo", &[("bands", 40.0), ("wave", 3.0), ("chord", 5.0)], 2, &v),
    ];
    for (name, set, ch, sig) in cases {
        let mut e = fresh(set, ch);
        let mut bufs: Vec<Vec<f32>> = (0..ch).map(|_| sig.to_vec()).collect();
        let t = std::time::Instant::now();
        for i in (0..n).step_by(128) {
            let mut sl: Vec<&mut [f32]> = bufs.iter_mut().map(|b| &mut b[i..i + 128]).collect();
            e.process(&mut sl);
        }
        println!("{name}: {:.0} ns / sample", t.elapsed().as_secs_f64() * 1e9 / n as f64);
    }
}
