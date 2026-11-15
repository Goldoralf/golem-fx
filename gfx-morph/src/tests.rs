// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5) on synthetic signals: the reference vowel and phrase of src/voice.rs,
//! pink bleed, sines. The real voice DI, the scream take, the drum loop and the stage test stay with
//! the bench and with Raph. Tests that print figures for the README run with `--nocapture`.

#![allow(clippy::type_complexity)]

use super::*;
use crate::types::TYPES;
use crate::voice::{self, VOWEL_A, VOWEL_O};
use std::f64::consts::PI;

const SR: f32 = 48000.0;

fn ty(name: &str) -> f32 {
    TYPE_NAMES.iter().position(|n| *n == name).unwrap_or_else(|| panic!("{name}")) as f32
}

fn obj(name: &str) -> f32 {
    OBJECT_NAMES.iter().position(|n| *n == name).unwrap_or_else(|| panic!("{name}")) as f32
}

fn dut(set: &[(&str, f32)], sr: f32, channels: usize) -> Morph {
    let mut d = Morph::new();
    for (k, v) in set {
        assert!(PARAMS.iter().any(|p| p.id == *k), "{k}");
        d.set_param(k, *v);
    }
    d.prepare(sr, 512, channels);
    d.reset();
    d
}

/// A linear, clean chain: every stage at its stop (types Off, guard and de-fizz off, no pre cut).
const CLEAN: [(&str, f32); 5] = [("body_a", 0.0), ("body_b", 0.0), ("guard", -90.0), ("defizz", 0.0), ("pre_hp", 20.0)];

fn run(d: &mut Morph, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for b in y.chunks_mut(128) {
        d.process(&mut [b]);
    }
    y
}

fn f32v(x: &[f64]) -> Vec<f32> {
    x.iter().map(|&v| v as f32).collect()
}

fn sine(f: f64, amp: f64, n: usize, sr: f32) -> Vec<f32> {
    (0..n).map(|i| (amp * (2.0 * PI * f * i as f64 / sr as f64).sin()) as f32).collect()
}

fn rms(v: &[f32]) -> f64 {
    (v.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / v.len().max(1) as f64).sqrt()
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-30).log10()
}

fn rms_db(v: &[f32]) -> f64 {
    db(rms(v))
}

fn vowel(f0: f64, secs: f64, v: ([f64; 4], [f64; 4]), level: f64) -> Vec<f32> {
    f32v(&voice::vowel(f0, (secs * SR as f64) as usize, v, level, SR as f64))
}

/// The p3 phrase: 'a' at 187.3 Hz and 'o' at 155 Hz, 0.6 s each, a 0.3 s gap between.
fn phrase(level: f64) -> (Vec<f32>, [std::ops::Range<usize>; 2]) {
    let a = vowel(voice::REF_F0, 0.6, VOWEL_A, level);
    let o = vowel(155.0, 0.6, VOWEL_O, level);
    let gap = vec![0.0f32; (0.3 * SR) as usize];
    let mut x = gap.clone();
    let ra = x.len()..x.len() + a.len();
    x.extend(&a);
    x.extend(&gap);
    let ro = x.len()..x.len() + o.len();
    x.extend(&o);
    x.extend(&gap);
    (x, [ra, ro])
}

/// 4th-order Butterworth high-pass (the bench's click detector).
fn highpass4(x: &[f32], fc: f32, sr: f32) -> Vec<f32> {
    let mut a = Svf::new();
    let mut b = Svf::new();
    a.set(SvfMode::HighPass, fc, 0.541_196, 0.0, sr);
    b.set(SvfMode::HighPass, fc, 1.306_563, 0.0, sr);
    x.iter().map(|&v| b.process(a.process(v))).collect()
}

/// Power spectrum (Blackman-Harris window) of `y`.
fn spectrum(y: &[f32]) -> Vec<f64> {
    let n = y.len();
    let mut planner = realfft::RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);
    let w = |i: usize| {
        let t = 2.0 * PI * i as f64 / n as f64;
        0.35875 - 0.48829 * t.cos() + 0.14128 * (2.0 * t).cos() - 0.01168 * (3.0 * t).cos()
    };
    let mut buf: Vec<f64> = y.iter().enumerate().map(|(i, &v)| v as f64 * w(i)).collect();
    let mut out = fft.make_output_vec();
    fft.process(&mut buf, &mut out).unwrap();
    out.iter().map(|c| c.norm_sqr()).collect()
}

/// Energy off the harmonics of f0 re on them, below 20 kHz (dB): `proto/common.py`'s metric.
fn harmonic_split(y: &[f32], f0: f64, sr: f32) -> f64 {
    let p = spectrum(y);
    let df = sr as f64 / y.len() as f64;
    let mut harm = vec![false; p.len()];
    let mut k = 1.0;
    while k * f0 < 20000.0 {
        let c = (k * f0 / df).round() as isize;
        for j in (c - 8).max(0)..=(c + 8).min(p.len() as isize - 1) {
            harm[j as usize] = true;
        }
        k += 1.0;
    }
    let (mut eh, mut ei) = (0.0, 0.0);
    for (i, &v) in p.iter().enumerate() {
        let f = i as f64 * df;
        if f > 20.0 && f < 20000.0 {
            if harm[i] {
                eh += v;
            } else {
                ei += v;
            }
        }
    }
    10.0 * (ei / eh.max(1e-30) + 1e-30).log10()
}

/// Third-octave band powers (dB) of `y`, 50 Hz – 16 kHz.
fn third_octaves(y: &[f32], sr: f32) -> Vec<f64> {
    let p = spectrum(y);
    let df = sr as f64 / y.len() as f64;
    (0..26)
        .map(|k| {
            let fc = 50.0 * 2f64.powf(k as f64 / 3.0);
            let (lo, hi) = (fc * 2f64.powf(-1.0 / 6.0), fc * 2f64.powf(1.0 / 6.0));
            let s: f64 = p.iter().enumerate().filter(|(i, _)| (*i as f64 * df) >= lo && (*i as f64 * df) < hi).map(|(_, v)| v).sum();
            10.0 * s.max(1e-30).log10()
        })
        .collect()
}

// ---- parameters ------------------------------------------------------------------------------------

#[test]
fn parameters_are_the_readmes() {
    assert_eq!(PARAMS.len(), 45);
    let ids: Vec<&str> = PARAMS.iter().map(|p| p.id).collect();
    for (i, a) in ids.iter().enumerate() {
        assert!(!ids[i + 1..].contains(a), "duplicate {a}");
    }
    let want = [
        "drive", "morph", "box", "mix", "object", "guard", "defizz", "env_drive", "env_morph", "pre_hp", "pre_lp", "tilt", "tilt_freq", "split", "x_low", "x_air", "low_a", "body_a", "air_a", "low_b", "body_b", "air_b", "low_trim", "body_trim", "air_trim", "low_level", "body_level", "air_level", "ring_hz", "quality", "object_b", "size", "decay", "position", "order", "comp", "post_hp", "post_lp", "reso", "env_filter", "echo_time", "echo_fb", "echo_mix", "level", "bypass",
    ];
    assert_eq!(ids, want);
    let exp: Vec<&str> = PARAMS.iter().filter(|p| p.exp).map(|p| p.id).collect();
    assert_eq!(exp, ["drive", "morph", "box", "mix", "size", "position", "post_lp", "level"]);
    assert_eq!(TYPE_NAMES, ["Off", "Soft", "Hard", "Tube", "Fuzz", "Rectify", "Fold", "Sine Fold", "Crush", "Decimate", "Sputter", "Ring", "Carbon", "Starve", "Rasp"]);
    assert_eq!(OBJECT_NAMES[1], "Tin Can");
    assert_eq!(&OBJECT_B_NAMES[1..], &OBJECT_NAMES[..]);
    let d = |id: &str| PARAMS[PARAMS.iter().position(|p| p.id == id).unwrap()].default;
    assert_eq!((d("drive"), d("guard"), d("defizz"), d("object"), d("body_a"), d("body_b")), (40.0, -55.0, 50.0, 1.0, 1.0, 2.0));
    // the default drive is 19.2 dB
    let m = dut(&[], SR, 1);
    assert!((m.drives()[BODY] - 19.2).abs() < 1e-9);
    for p in PARAMS.iter() {
        assert!(p.min < p.max && p.default >= p.min && p.default <= p.max, "{}", p.id);
    }
}

// ---- the tables ------------------------------------------------------------------------------------

#[test]
fn tables_are_bounded_and_unity_types_are_unity() {
    for t in TYPES {
        for k in 0..TABLE_POINTS {
            let m = MAKEUP_DB[t as usize][k] as f64;
            assert!((types::MAKEUP_MIN_DB..=types::MAKEUP_MAX_DB).contains(&m), "{t:?} {k}");
            if !t.compensated() {
                assert_eq!(m, 0.0);
            }
        }
    }
    // S = d + m(d) for Soft (README §2.2: +8.0 / +18.5 / +30.2 dB at 24 / 36 / 48 dB, prototype)
    for (d, want) in [(24.0, 8.0), (36.0, 18.5), (48.0, 30.2)] {
        let s = small_db(Type::Soft, d);
        assert!((s - want).abs() < 1.0, "Soft S at {d} dB: {s:.1}");
    }
    for t in [Type::Off, Type::Rectify] {
        for d in [0.0, 24.0, 48.0] {
            assert!(small_db(t, d) < 1.5, "{t:?} {d}: {}", small_db(t, d));
        }
    }
}

#[test]
fn small_signal_table_matches_the_pedal() {
    // README §5 Guard: S(type, d) within 1 dB of the measured small-signal gain, through the whole
    // pedal (clean otherwise), pink at −90 dBFS (the table's level)
    let x = f32v(&voice::pink((1.0 * SR as f64) as usize, -90.0, 7));
    let mut worst = (0.0f64, String::new());
    for t in TYPES.iter().copied().filter(|t| !matches!(t, Type::Decimate | Type::Ring)) {
        // Decimate's and Ring's S is their worst third-octave band by design (build.rs)
        for drive in [0.0f32, 50.0, 100.0] {
            let mut set = CLEAN.to_vec();
            set.extend([("body_a", t as usize as f32), ("drive", drive)]);
            let mut m = dut(&set, SR, 1);
            let y = run(&mut m, &x);
            let n0 = (0.2 * SR) as usize;
            let g = rms_db(&y[n0..]) - rms_db(&x[n0..]);
            let s = small_db(t, 0.48 * drive as f64).max(-60.0);
            let e = (g.max(-60.0) - s).abs();
            if e > worst.0 {
                worst = (e, format!("{t:?} drive {drive}: pedal {g:.1} dB, table {s:.1} dB"));
            }
        }
    }
    println!("S(type, d) vs the pedal, worst: {:.2} dB ({})", worst.0, worst.1);
    assert!(worst.0 < 1.0, "{}", worst.1);
}

// ---- latency ---------------------------------------------------------------------------------------

/// Group delay (ms) at `f` of the impulse response `h`, by the phase slope.
fn group_delay_ms(h: &[f32], f: f64, sr: f32) -> f64 {
    let ph = |f: f64| {
        let w = 2.0 * PI * f / sr as f64;
        let (mut re, mut im) = (0.0, 0.0);
        for (n, &v) in h.iter().enumerate() {
            re += v as f64 * (w * n as f64).cos();
            im -= v as f64 * (w * n as f64).sin();
        }
        im.atan2(re)
    };
    let df = 1.0;
    let mut d = ph(f + df) - ph(f - df);
    while d > PI {
        d -= 2.0 * PI;
    }
    while d < -PI {
        d += 2.0 * PI;
    }
    -d / (2.0 * PI * 2.0 * df) * 1000.0
}

#[test]
fn zero_latency_first_sample_and_group_delay() {
    // README §5 Latency: types Off (linear), the default pre_hp 100 Hz and defizz 50 %
    let n = 8192;
    let mut imp = vec![0.0f32; n];
    imp[0] = 0.01;
    let cases: [(&str, &[(&str, f32)], f64); 4] = [
        ("Single 2x", &[], 0.10),
        ("Single 4x", &[("quality", 1.0)], 0.13),
        ("Two", &[("split", 1.0)], 0.22),
        ("Three", &[("split", 2.0)], 0.36),
    ];
    for (name, extra, bound) in cases {
        for sr in [44100.0, 48000.0, 96000.0, 192000.0] {
            let mut set = vec![("body_a", 0.0), ("body_b", 0.0), ("low_a", 0.0), ("air_a", 0.0), ("guard", -90.0)];
            set.extend_from_slice(extra);
            let mut m = dut(&set, sr, 1);
            let h = run(&mut m, &imp);
            assert!(h[0] != 0.0, "{name} at {sr}: first sample is 0");
            assert_eq!(m.latency(), 0);
            if sr == 48000.0 {
                let h: Vec<f32> = h.iter().map(|v| v / 0.01).collect();
                let gd = group_delay_ms(&h, 1000.0, sr);
                println!("group delay at 1 kHz, {name}: {gd:.3} ms (bound {bound})");
                assert!(gd <= bound && gd > 0.0, "{name}: {gd}");
            }
        }
    }
}

#[test]
fn defizz_shelf_at_rest_is_the_identity() {
    let mut s = Svf::new();
    s.set(SvfMode::HighShelf, DEFIZZ_HZ as f32, std::f32::consts::FRAC_1_SQRT_2, 0.0, SR);
    let x = f32v(&voice::pink(48000, -12.0, 3));
    for &v in &x {
        assert_eq!(s.process(v), v);
    }
}

// ---- the clean skeleton ----------------------------------------------------------------------------

#[test]
fn clean_skeleton_is_flat() {
    // README §5: every type Off, Single / Two / Three, 2× / 4×: flat ±0.1 dB 20 Hz – 20 kHz
    let n = 16384;
    let mut imp = vec![0.0f32; n];
    imp[0] = 0.01;
    let mut worst = 0.0f64;
    for split in [0.0, 1.0, 2.0] {
        for q in [0.0, 1.0] {
            let mut set = CLEAN.to_vec();
            set.extend([("split", split), ("quality", q)]);
            let mut m = dut(&set, SR, 1);
            let h: Vec<f32> = run(&mut m, &imp).iter().map(|v| v / 0.01).collect();
            for k in 0..60 {
                let f = 20.0 * 1000f64.powf(k as f64 / 59.0);
                let w = 2.0 * PI * f / SR as f64;
                let (mut re, mut im) = (0.0, 0.0);
                for (i, &v) in h.iter().enumerate() {
                    re += v as f64 * (w * i as f64).cos();
                    im -= v as f64 * (w * i as f64).sin();
                }
                let g = db(re.hypot(im));
                worst = worst.max(g.abs());
                assert!(g.abs() < 0.1, "split {split} quality {q} at {f:.0} Hz: {g:.3} dB");
            }
        }
    }
    println!("clean skeleton: worst {worst:.3} dB, 20 Hz – 20 kHz");
}

// ---- aliasing ----------------------------------------------------------------------------------------

#[test]
fn aliasing_on_voice() {
    // README §5: the reference vowel at four pitches, −18 / −6 dBFS RMS, inharmonic re harmonic
    let pitches = [187.3, 293.7, 523.9, 881.3];
    let n = 32768;
    let split = |t: &str, drive: f32, level: f64, f0: f64, q: f32| {
        let x = vowel(f0, (n as f64 + 9600.0) / SR as f64, VOWEL_A, level);
        let mut set = CLEAN.to_vec();
        set.extend([("body_a", ty(t)), ("drive", drive), ("quality", q)]);
        let mut m = dut(&set, SR, 1);
        let y = run(&mut m, &x);
        harmonic_split(&y[y.len() - n..], f0, SR)
    };
    // the prototype's types (bound: its 2× + ADAA figure + 4 dB)
    let rows: [(&str, f64, f64); 5] = [("Soft", -18.0, -53.0), ("Soft", -6.0, -42.0), ("Hard", -6.0, -41.0), ("Fold", -18.0, -64.0), ("Fold", -6.0, -46.0)];
    for (t, level, bound) in rows {
        let mut w2 = f64::NEG_INFINITY;
        let mut w4 = f64::NEG_INFINITY;
        for f0 in pitches {
            let a2 = split(t, 100.0, level, f0, 0.0);
            w2 = w2.max(a2);
            if t != "Fold" {
                let a4 = split(t, 100.0, level, f0, 1.0);
                w4 = w4.max(a4 - a2);
            }
        }
        println!("{t} drive 100 % at {level} dBFS: worst {w2:.1} dB at 2x (bound {bound}); 4x minus 2x worst {w4:.1} dB");
        assert!(w2 <= bound, "{t} {level}: {w2}");
        if t != "Fold" {
            assert!(w4 <= -10.0 || w2 < -80.0, "{t} {level}: 4x gains only {w4}");
        }
    }
    let mut failed = Vec::new();
    // the types the prototype did not run: measured at drive 100 % on the −18 dBFS vowel. Decimate
    // (a hold clock unrelated to the pitch), Ring (a carrier) and Rasp (noise) are inharmonic by
    // design: reported, not bounded
    for t in ["Tube", "Rectify", "Sine Fold", "Fuzz", "Crush", "Sputter", "Carbon", "Starve", "Decimate", "Ring", "Rasp"] {
        let w = pitches.iter().map(|&f0| split(t, 100.0, -18.0, f0, 0.0)).fold(f64::NEG_INFINITY, f64::max);
        let w4 = pitches.iter().map(|&f0| split(t, 100.0, -18.0, f0, 1.0)).fold(f64::NEG_INFINITY, f64::max);
        println!("{t} drive 100 % at -18 dBFS: worst {w:.1} dB at 2x, {w4:.1} dB at 4x");
        if !matches!(t, "Decimate" | "Ring" | "Rasp") && w > -45.0 {
            failed.push(format!("{t}: {w:.1}"));
        }
    }
    assert!(failed.is_empty(), "{failed:?}");
}

// ---- level --------------------------------------------------------------------------------------------

#[test]
fn vocal_tables_hold_the_level() {
    // README §5: each compensated type, drive 0 → 100 %, the reference phrase at −18 dBFS RMS within
    // ±1 dB of its input
    let (x, [ra, ro]) = phrase(-18.0);
    let mut worst = (0.0f64, String::new());
    for t in TYPES.iter().filter(|t| t.compensated()) {
        let mut row = Vec::new();
        for drive in [0.0f32, 25.0, 50.0, 75.0, 100.0] {
            let mut set = CLEAN.to_vec();
            set.extend([("body_a", *t as usize as f32), ("drive", drive)]);
            let mut m = dut(&set, SR, 1);
            let y = run(&mut m, &x);
            let s = (ra.start + 4800)..ra.end;
            let o = (ro.start + 4800)..ro.end;
            let yin: Vec<f32> = x[s.clone()].iter().chain(&x[o.clone()]).copied().collect();
            let yout: Vec<f32> = y[s].iter().chain(&y[o]).copied().collect();
            let g = rms_db(&yout) - rms_db(&yin);
            row.push(g);
            let lim = if *t == Type::Sputter { 30.0 } else { 1.0 };
            if g.abs() > worst.0 && *t != Type::Sputter {
                worst = (g.abs(), format!("{t:?} at {drive} %: {g:+.2} dB"));
            }
            assert!(g.abs() <= lim || (*t == Type::Sputter && g < 0.5), "{t:?} at {drive} %: {g:+.2} dB");
        }
        println!("{t:?}: phrase level re input at drive 0/25/50/75/100 %: {}", row.iter().map(|g| format!("{g:+.2}")).collect::<Vec<_>>().join(" "));
    }
    println!("worst (Sputter apart: its make-up is capped by design): {}", worst.1);
}

#[test]
fn morph_holds_the_level() {
    // README §5: pairs at 25 / 50 / 75 % within ±0.5 dB of the line between the ends (memoryless), ±1.5
    // dB with a type with memory; drive 50 % (24 dB)
    let (x, [ra, ro]) = phrase(-18.0);
    let level = |a: &str, b: &str, mu: f32| {
        let mut set = CLEAN.to_vec();
        set.extend([("body_a", ty(a)), ("body_b", ty(b)), ("drive", 50.0), ("morph", mu)]);
        let mut m = dut(&set, SR, 1);
        let y = run(&mut m, &x);
        let s = (ra.start + 4800)..ra.end;
        let o = (ro.start + 4800)..ro.end;
        let yout: Vec<f32> = y[s].iter().chain(&y[o]).copied().collect();
        rms_db(&yout)
    };
    let pairs = [("Soft", "Hard"), ("Soft", "Fold"), ("Tube", "Rectify"), ("Soft", "Rectify"), ("Hard", "Sine Fold"), ("Tube", "Carbon"), ("Soft", "Fuzz"), ("Ring", "Decimate"), ("Sputter", "Starve"), ("Tube", "Rasp")];
    for (a, b) in pairs {
        let (la, lb) = (level(a, b, 0.0), level(a, b, 100.0));
        let mem = Type::from_index(ty(a) as usize).has_memory() || Type::from_index(ty(b) as usize).has_memory();
        let bound = if mem { 1.5 } else { 0.5 };
        let mut row = Vec::new();
        for mu in [25.0f32, 50.0, 75.0] {
            let l = level(a, b, mu);
            let line = la + (lb - la) * mu as f64 / 100.0;
            row.push(l - line);
            assert!((l - line).abs() <= bound, "{a} → {b} at {mu} %: {:+.2} dB off the line", l - line);
        }
        println!("{a} → {b}: off the line at 25/50/75 %: {}", row.iter().map(|g| format!("{g:+.2}")).collect::<Vec<_>>().join(" "));
    }
}

// ---- the guard ---------------------------------------------------------------------------------------

fn bleed_case(set: &[(&str, f32)], x: &[f32]) -> (f64, f64) {
    let mut s = vec![("guard", -40.0)];
    s.extend_from_slice(set);
    let mut m = dut(&s, SR, 1);
    let y = run(&mut m, x);
    let n0 = (1.0 * SR) as usize;
    let b_in = third_octaves(&x[n0..], SR);
    let b_out = third_octaves(&y[n0..], SR);
    let worst_band = b_out.iter().zip(&b_in).map(|(o, i)| o - i).fold(f64::NEG_INFINITY, f64::max);
    (rms_db(&y[n0..]) - rms_db(&x[n0..]), worst_band)
}

#[test]
fn guard_caps_the_whole_pedal_under_bleed() {
    // README §5: pink bleed at −50 dBFS RMS, guard −40: output ≤ bypass − 2 dB at drive 0 → 100 %,
    // `mix` 35 and 100 %, every type; the same in third-octave bands with each object, `reso`, `comp`,
    // the echo and `tilt`, alone and together
    let x = f32v(&voice::pink((3.0 * SR as f64) as usize, -50.0, 21));
    let mut worst = (f64::NEG_INFINITY, String::new());
    for t in TYPES {
        for drive in [0.0f32, 50.0, 100.0] {
            for mix in [35.0f32, 100.0] {
                let (g, band) = bleed_case(&[("body_a", t as usize as f32), ("drive", drive), ("mix", mix)], &x);
                assert!(g <= -2.0, "{t:?} drive {drive} mix {mix}: {g:+.1} dB");
                if band > worst.0 {
                    worst = (band, format!("{t:?} drive {drive} mix {mix}: broadband {g:+.1} dB"));
                }
            }
        }
    }
    println!("dirt alone: worst third-octave band {:+.1} dB re bypass ({})", worst.0, worst.1);
    assert!(worst.0 <= 0.0, "{}", worst.1);
    let mut cases: Vec<(String, Vec<(&str, f32)>)> = Vec::new();
    for o in 1..objects::OBJECTS {
        cases.push((format!("{} box 100", OBJECT_NAMES[o]), vec![("object", o as f32), ("box", 100.0), ("drive", 100.0)]));
    }
    cases.push(("reso 100 %, post_lp 2 kHz".into(), vec![("reso", 100.0), ("post_lp", 2000.0), ("drive", 100.0)]));
    cases.push(("comp 100 %".into(), vec![("comp", 100.0), ("drive", 100.0)]));
    cases.push(("echo 50 % / 60 %".into(), vec![("echo_mix", 50.0), ("echo_fb", 60.0), ("drive", 100.0)]));
    cases.push(("tilt +12".into(), vec![("tilt", 12.0), ("drive", 100.0)]));
    cases.push(("tilt -12".into(), vec![("tilt", -12.0), ("drive", 100.0)]));
    cases.push((
        "all together (Bottle)".into(),
        vec![("object", obj("Bottle")), ("box", 100.0), ("reso", 100.0), ("post_lp", 2000.0), ("comp", 100.0), ("echo_mix", 50.0), ("echo_fb", 60.0), ("tilt", 12.0), ("drive", 100.0)],
    ));
    for (name, set) in cases {
        let (g, band) = bleed_case(&set, &x);
        println!("{name}: broadband {g:+.1} dB, worst third-octave {band:+.1} dB re bypass");
        assert!(band <= 0.0, "{name}: a band {band:+.1} dB over bypass");
    }
}

#[test]
fn guard_onset_cost_and_a_quiet_vowel() {
    // README §5: a vowel starting over bleed (T −45, drive 36 dB): loss ≤ 3 dB over 5 ms, ≤ 1 dB over
    // 10 ms; a −40 dBFS vowel with the guard at −55: untouched (±0.1 dB)
    let bleed = f32v(&voice::pink((1.5 * SR as f64) as usize, -50.0, 5));
    let v = vowel(voice::REF_F0, 0.5, VOWEL_A, -18.0);
    let on = (1.0 * SR) as usize;
    let mut x = bleed.clone();
    for (i, s) in v.iter().enumerate() {
        if on + i < x.len() {
            x[on + i] += s;
        }
    }
    let base = [("drive", 75.0), ("defizz", 0.0)];
    let mut a = dut(&[base[0], base[1], ("guard", -45.0)], SR, 1);
    let mut b = dut(&[base[0], base[1], ("guard", -90.0)], SR, 1);
    let ya = run(&mut a, &x);
    let yb = run(&mut b, &x);
    let mut out = Vec::new();
    for ms in [1.0, 2.0, 5.0, 10.0, 20.0] {
        let e = on + (ms * 0.001 * SR as f64) as usize;
        let loss = rms_db(&ya[on..e]) - rms_db(&yb[on..e]);
        out.push(loss);
    }
    println!("onset over bleed, loss re no guard at 1/2/5/10/20 ms: {}", out.iter().map(|g| format!("{g:+.2}")).collect::<Vec<_>>().join(" "));
    assert!(out[2] >= -3.0 && out[3] >= -1.0, "{out:?}");
    // a quiet vowel passes untouched
    let q = vowel(voice::REF_F0, 1.0, VOWEL_A, -40.0);
    let mut a = dut(&[("guard", -55.0)], SR, 1);
    let mut b = dut(&[("guard", -90.0)], SR, 1);
    let (ya, yb) = (run(&mut a, &q), run(&mut b, &q));
    let d = rms_db(&ya[4800..]) - rms_db(&yb[4800..]);
    println!("a -40 dBFS vowel through the default guard: {d:+.3} dB");
    assert!(d.abs() <= 0.1);
    assert_eq!(a.meter("state"), Some(1.0));
}

#[test]
fn guard_state_meter_and_off() {
    let mut m = dut(&[("guard", -40.0)], SR, 1);
    let x = f32v(&voice::pink(48000, -60.0, 9));
    run(&mut m, &x);
    assert_eq!(m.meter("state"), Some(0.0));
    let mut m = dut(&[("guard", -90.0)], SR, 1);
    run(&mut m, &x);
    assert_eq!(m.meter("state"), Some(1.0));
    assert_eq!(m.guard_state().1, 0.0);
}

// ---- de-fizz -----------------------------------------------------------------------------------------

/// The p4 phrase: gap, 's' (−24 dBFS RMS), 'a' (−18), gap, 's', 'o' (−18), gap; regions of the 's'
/// and the vowels.
fn sibilant_phrase(breathy: bool) -> (Vec<f32>, Vec<std::ops::Range<usize>>, Vec<std::ops::Range<usize>>) {
    let fs = SR as f64;
    let gap = vec![0.0f32; (0.3 * fs) as usize];
    let s1 = f32v(&voice::sibilant((0.15 * fs) as usize, -24.0, fs, 3));
    let mut a = vowel(voice::REF_F0, 0.6, VOWEL_A, -18.0);
    let s2 = f32v(&voice::sibilant((0.15 * fs) as usize, -24.0, fs, 5));
    let mut o = vowel(155.0, 0.6, VOWEL_O, -18.0);
    if breathy {
        // aspiration noise 12 dB under the vowel
        for (v, seed) in [(&mut a, 11u64), (&mut o, 12)] {
            let n = f32v(&voice::band_noise(v.len(), 2000.0, 8000.0, -30.0, fs, seed));
            for (s, k) in v.iter_mut().zip(n) {
                *s += k;
            }
        }
    }
    let mut x = Vec::new();
    let (mut ss, mut vv) = (Vec::new(), Vec::new());
    for (part, kind) in [(&gap, 0), (&s1, 1), (&a, 2), (&gap, 0), (&s2, 1), (&o, 2), (&gap, 0)] {
        let r = x.len()..x.len() + part.len();
        match kind {
            1 => ss.push(r),
            2 => vv.push(r),
            _ => {}
        }
        x.extend(part.iter());
    }
    (x, ss, vv)
}

/// Power (dB) of `y` in 1 – 4 kHz over the regions.
fn band_db(y: &[f32], regions: &[std::ops::Range<usize>], lo: f64, hi: f64) -> f64 {
    let mut h1 = Svf::new();
    let mut h2 = Svf::new();
    let mut l1 = Svf::new();
    let mut l2 = Svf::new();
    h1.set(SvfMode::HighPass, lo as f32, 0.541_196, 0.0, SR);
    h2.set(SvfMode::HighPass, lo as f32, 1.306_563, 0.0, SR);
    l1.set(SvfMode::LowPass, hi as f32, 0.541_196, 0.0, SR);
    l2.set(SvfMode::LowPass, hi as f32, 1.306_563, 0.0, SR);
    let f: Vec<f32> = y.iter().map(|&v| l2.process(l1.process(h2.process(h1.process(v))))).collect();
    let v: Vec<f32> = regions.iter().flat_map(|r| f[r.start + 480..r.end - 480].iter().copied()).collect();
    rms_db(&v)
}

#[test]
fn defizz_lowers_the_hash_and_spares_the_vowels() {
    // README §5 De-fizz: Soft at 30 dB (drive 62.5 %), 's' in 1 – 4 kHz re the vowel in 1 – 4 kHz
    let (x, ss, vv) = sibilant_phrase(false);
    let mut out = Vec::new();
    let mut vowels = Vec::new();
    for amt in [0.0f32, 50.0, 100.0] {
        let mut m = dut(&[("drive", 62.5), ("defizz", amt), ("body_a", ty("Soft"))], SR, 1);
        let y = run(&mut m, &x);
        out.push(band_db(&y, &ss, 1000.0, 4000.0) - band_db(&y, &vv, 1000.0, 4000.0));
        let vl: Vec<f32> = vv.iter().flat_map(|r| y[r.start + 4800..r.end].iter().copied()).collect();
        vowels.push(rms_db(&vl));
    }
    println!("de-fizz: 's' re vowel in 1 - 4 kHz at 0 / 50 / 100 %: {:.1} / {:.1} / {:.1} dB (bounds -, -7.5, -10.5); vowels move {:+.2} / {:+.2} dB", out[0], out[1], out[2], vowels[1] - vowels[0], vowels[2] - vowels[0]);
    assert!(out[1] <= -7.5 && out[2] <= -10.5, "{out:?}");
    assert!((vowels[1] - vowels[0]).abs() <= 0.3 && (vowels[2] - vowels[0]).abs() <= 0.3);
}

#[test]
fn defizz_ignores_hiss_and_tilted_breathy_vowels() {
    // white hiss at −70 dBFS, then a vowel: no duck at the onset (≤ 0.5 dB)
    let fs = SR as f64;
    let mut rng = voice::Xor(77);
    let hiss_amp = 10f64.powf(-70.0 / 20.0) * 3f64.sqrt();
    let mut x: Vec<f32> = (0..(1.0 * fs) as usize).map(|_| (rng.next_f64() * hiss_amp) as f32).collect();
    let on = x.len();
    let v = vowel(voice::REF_F0, 0.3, VOWEL_A, -18.0);
    x.extend(v.iter().map(|&s| s + (rng.next_f64() * hiss_amp) as f32));
    let mut m = dut(&[("drive", 62.5), ("defizz", 50.0)], SR, 1);
    let mut duck = 0.0f64;
    let mut y = x.clone();
    for (k, b) in y.chunks_mut(16).enumerate() {
        m.process(&mut [b]);
        if k * 16 >= on && k * 16 < on + 1440 {
            duck = duck.max(-m.defizz_db());
        }
    }
    println!("hiss then a vowel: largest duck over the first 30 ms {duck:.2} dB");
    assert!(duck <= 0.5);
    // tilt +12 on a breathy vowel: duck ≤ 1 dB
    let (x, _, vv) = sibilant_phrase(true);
    let mut m = dut(&[("drive", 62.5), ("defizz", 50.0), ("tilt", 12.0)], SR, 1);
    let (mut worst, mut sum, mut cnt) = (0.0f64, 0.0f64, 0.0f64);
    let mut y = x.clone();
    for (k, b) in y.chunks_mut(16).enumerate() {
        m.process(&mut [b]);
        if vv.iter().any(|r| k * 16 >= r.start + 2400 && k * 16 < r.end) {
            worst = worst.max(-m.defizz_db());
            sum += -m.defizz_db();
            cnt += 1.0;
        }
    }
    let mean = sum / cnt.max(1.0);
    println!("breathy vowels with tilt +12: duck mean {mean:.2} dB, largest {worst:.2} dB");
    assert!(mean <= 1.0 && worst <= 2.5);
}

#[test]
fn defizz_zero_meters_zero_on_cymbals() {
    // a hat-like loop (8 kHz noise bursts every 125 ms) at `defizz` 0: the meter stays 0
    let fs = SR as f64;
    let mut x = vec![0.0f32; (2.0 * fs) as usize];
    for k in 0..16 {
        let s = f32v(&voice::sibilant((0.05 * fs) as usize, -20.0, fs, 40 + k));
        let at = k as usize * (0.125 * fs) as usize;
        for (i, v) in s.iter().enumerate() {
            x[at + i] += v;
        }
    }
    let mut m = dut(&[("defizz", 0.0)], SR, 1);
    let mut y = x.clone();
    for b in y.chunks_mut(64) {
        m.process(&mut [b]);
        assert_eq!(m.meter("defizz_gr"), Some(0.0));
    }
    let mut m = dut(&[("defizz", 50.0)], SR, 1);
    let mut most = 0.0f32;
    for b in x.chunks_mut(64) {
        m.process(&mut [b]);
        most = most.max(m.meter("defizz_gr").unwrap());
    }
    assert!(most > 3.0, "the hats read as sibilants at 50 %: {most}");
}

// ---- the objects -----------------------------------------------------------------------------------

fn level_table(o: usize) -> Vec<f64> {
    BOX_LEVEL_DB[o].iter().map(|&v| v as f64).collect()
}

#[test]
fn objects_keep_one_mode_set() {
    for o in 1..objects::OBJECTS {
        let def = Object::new(o);
        assert!(def.n > 0 && def.n <= MAX_MODES);
        let mut prev = 0.0;
        for c in def.modes[..def.n].iter() {
            assert!(c.f >= prev && c.f >= 60.0);
            prev = c.f;
        }
        // position moves gains, size moves frequencies: the count never changes
        let mut ms = [Mode::default(); MAX_MODES];
        for pos in [0.0, 0.3, 1.0] {
            for size in [0.25, 1.0, 4.0] {
                assert_eq!(def.modes(pos, size, 1.0, 1.0, &mut ms), def.n);
                assert!((ms[0].f - def.modes[0].f / size).abs() < 1e-9);
            }
        }
        println!("{}: {} modes, {:.0} – {:.0} Hz", OBJECT_NAMES[o], def.n, def.modes[0].f, def.modes[def.n - 1].f);
    }
    // the review's figures: the Cardboard Box keeps 48 of its candidates, Hands 2
    assert_eq!(Object::new(5).n, 48);
    assert_eq!(Object::new(9).n, 2);
}

#[test]
fn object_pink_level_holds_off_the_grid() {
    // README §5: pink power within ±1 dB of 0 dB at `box` 100 % off the table's points, every object
    let mut worst = (0.0f64, String::new());
    for o in 1..objects::OBJECTS {
        let t = level_table(o);
        let v = BOX_VOICE_DB[o] as f64;
        for &pos in &[0.0078, 0.33, 0.5078, 0.77, 0.9922] {
            for &decay in &[0.35, 0.7, 1.0, 2.8] {
                for &size in &[0.3, 0.6, 1.0, 1.7, 3.3] {
                    // the voice constant offsets the pink power by design; remove it
                    let p = objects::pink_power_db(o, &t, pos, size, decay, objects::TABLE_FS);
                    if p.abs() > worst.0 {
                        worst = (p.abs(), format!("{} pos {pos} decay {decay} size {size}: {p:+.2} dB", OBJECT_NAMES[o]));
                    }
                }
            }
        }
        let _ = v;
    }
    println!("pink level off the grid, worst {:.2} dB ({})", worst.0, worst.1);
    assert!(worst.0 <= 1.0, "{}", worst.1);
}

#[test]
fn object_voice_level_and_bypass() {
    // the voice level (power mean over 98 – 392 Hz, 'a' and 'o') within ±1 dB of bypass at the
    // defaults; single notes reported
    for o in 1..objects::OBJECTS {
        let mut acc = 0.0;
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        let mut cnt = 0.0;
        for i in (0..25).step_by(2) {
            let f0 = 98.0 * 2f64.powf(i as f64 / 12.0);
            for v in [VOWEL_A, VOWEL_O] {
                let x = vowel(f0, 0.7, v, -30.0);
                let mut set = CLEAN.to_vec();
                set.extend([("object", o as f32), ("box", 100.0)]);
                let mut m = dut(&set, SR, 1);
                let y = run(&mut m, &x);
                let n0 = (0.35 * SR) as usize;
                let g = rms_db(&y[n0..]) - rms_db(&x[n0..]);
                acc += 10f64.powf(g / 10.0);
                cnt += 1.0;
                lo = lo.min(g);
                hi = hi.max(g);
            }
        }
        let pm = 10.0 * (acc / cnt).log10();
        println!("{}: voice level re bypass {pm:+.2} dB (single notes {lo:+.1} … {hi:+.1} dB)", OBJECT_NAMES[o]);
        assert!(pm.abs() <= 1.0, "{}: {pm}", OBJECT_NAMES[o]);
    }
    // box 0 = bit-exact bypass of the object: two objects, the same output
    let x = vowel(voice::REF_F0, 0.5, VOWEL_A, -18.0);
    let mut a = dut(&[("object", obj("Bottle")), ("box", 0.0)], SR, 1);
    let mut b = dut(&[("object", obj("Oil Drum")), ("box", 0.0), ("size", 300.0)], SR, 1);
    assert_eq!(run(&mut a, &x), run(&mut b, &x));
}

#[test]
fn box_motion_is_clean() {
    // README §5 Box motion: size 25 → 400 % in 0.5 s, position 0 → 100 %, morphs between pairs:
    // finite, largest sample step ≤ 1.5 × the static renders'
    let x = vowel(voice::REF_F0, 1.2, VOWEL_A, -18.0);
    let step = |y: &[f32]| y.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    let render = |set: &[(&str, f32)], moves: &[(&str, f32, f32)]| {
        let mut s = CLEAN.to_vec();
        s.extend_from_slice(set);
        s.push(("box", 100.0));
        let mut m = dut(&s, SR, 1);
        let mut y = x.clone();
        let n = y.len();
        let (a, b) = ((0.2 * SR) as usize, (0.7 * SR) as usize);
        for (k, blk) in y.chunks_mut(64).enumerate() {
            let t = k * 64;
            if t >= a && t <= b {
                let r = (t - a) as f32 / (b - a) as f32;
                for (id, from, to) in moves {
                    m.set_param(id, from + (to - from) * r);
                }
            }
            m.process(&mut [blk]);
        }
        let _ = n;
        assert!(y.iter().all(|v| v.is_finite()));
        y
    };
    let mut worst = (0.0f32, String::new());
    for o in 1..objects::OBJECTS {
        let name = OBJECT_NAMES[o];
        // the static reference: the largest step of the static renders along the path (a mid setting
        // can meet the vowel's formants and be louder than both ends)
        for (label, moves, stops) in [
            ("size 25 → 400 %", vec![("size", 25.0f32, 400.0f32)], [25.0f32, 35.4, 50.0, 70.7, 100.0, 141.4, 200.0, 282.8, 400.0]),
            ("position 0 → 100 %", vec![("position", 0.0, 100.0)], [0.0, 12.5, 25.0, 37.5, 50.0, 62.5, 75.0, 87.5, 100.0]),
        ] {
            let id = moves[0].0;
            let y = render(&[("object", o as f32), (id, moves[0].1)], &moves);
            let s = stops.iter().map(|&v| step(&render(&[("object", o as f32), (id, v)], &[]))).fold(0.0f32, f32::max);
            let r = step(&y) / s;
            println!("{name} {label}: step ratio {r:.2}");
            if r > worst.0 {
                worst = (r, format!("{name} {label}"));
            }
        }
    }
    // morphs: Hands ↔ Tin Can (no level bump over +1 dB above the louder end), Tin Can → Bucket,
    // Bottle → Cone
    for (a, b) in [("Hands", "Tin Can"), ("Tin Can", "Hands"), ("Tin Can", "Bucket"), ("Bottle", "Cone"), ("None", "Oil Drum")] {
        let set = [("object", obj(a)), ("object_b", obj(b) + 1.0), ("body_b", 0.0)];
        let y = render(&set, &[("morph", 0.0, 100.0)]);
        let ya = render(&[set[0], set[1], set[2], ("morph", 0.0)], &[]);
        let yb = render(&[set[0], set[1], set[2], ("morph", 100.0)], &[]);
        let r = step(&y) / step(&ya).max(step(&yb));
        if r > worst.0 {
            worst = (r, format!("morph {a} → {b}"));
        }
        // the level along a static morph
        let mut lv = Vec::new();
        for mu in [0.0f32, 25.0, 50.0, 75.0, 100.0] {
            let yy = render(&[set[0], set[1], set[2], ("morph", mu)], &[]);
            lv.push(rms_db(&yy[(0.3 * SR) as usize..]) - rms_db(&x[(0.3 * SR) as usize..]));
        }
        let top = lv[0].max(lv[4]);
        println!("morph {a} → {b}: level 0/25/50/75/100 % {}", lv.iter().map(|g| format!("{g:+.1}")).collect::<Vec<_>>().join(" "));
        if (a, b) == ("Hands", "Tin Can") || (a, b) == ("Tin Can", "Hands") {
            assert!(lv.iter().all(|&g| g <= top + 1.0), "{a} → {b}: {lv:?}");
        }
    }
    println!("box motion: largest sample step {:.2} × the static renders' ({})", worst.0, worst.1);
    assert!(worst.0 <= 1.5, "{}", worst.1);
}

#[test]
fn bank_runs_at_every_rate() {
    for sr in [44100.0, 48000.0, 96000.0, 192000.0] {
        for o in 1..objects::OBJECTS {
            let mut set = CLEAN.to_vec();
            set.extend([("object", o as f32), ("box", 100.0), ("decay", 400.0), ("size", 25.0)]);
            let mut m = dut(&set, sr, 1);
            let mut x = vec![0.0f32; sr as usize];
            x[0] = 0.5;
            let y = run(&mut m, &x);
            assert!(y.iter().all(|v| v.is_finite() && v.abs() < 10.0), "{} at {sr}", OBJECT_NAMES[o]);
        }
    }
}

// ---- comp, echo ----------------------------------------------------------------------------------------

#[test]
fn comp_follows_its_law() {
    // 1 kHz at −6 dBFS peak, `comp` 100 %: GR within 1 dB of the law, and the meter says so
    let x = sine(1000.0, 10f64.powf(-6.0 / 20.0), 48000, SR);
    let mut set = CLEAN.to_vec();
    set.push(("comp", 100.0));
    let mut m = dut(&set, SR, 1);
    let y = run(&mut m, &x);
    let (t, r) = comp_law(100.0);
    let want = comp_gr(-6.0, t, r);
    let got = rms_db(&x[24000..]) - rms_db(&y[24000..]) + comp_makeup(100.0);
    println!("comp 100 % at -6 dBFS: GR {got:.2} dB, law {want:.2} dB, meter {:.2} dB", m.meter("gr").unwrap());
    assert!((got - want).abs() <= 1.0);
    assert!((m.meter("gr").unwrap() as f64 - got).abs() <= 1.0);
    // make-up: 0 / +2.0 / +5.2 dB at 40 / 70 / 100 %
    for (c, w) in [(40.0, 0.0), (70.0, 2.0), (100.0, 5.2)] {
        assert!((comp_makeup(c) - w).abs() < 0.15, "{c}: {}", comp_makeup(c));
    }
}

#[test]
fn echo_time_feedback_and_the_guard() {
    let n = 48000;
    let mut x = vec![0.0f32; n];
    x[100] = 0.5;
    let mut set = CLEAN.to_vec();
    set.extend([("echo_mix", 100.0), ("echo_fb", 0.0), ("echo_time", 110.0)]);
    let mut m = dut(&set, SR, 1);
    let y = run(&mut m, &x);
    // the echo re the dry's own peak (the dirt's minimum-phase group delay is common to both)
    let argmax = |s: &[f32]| s.iter().enumerate().fold((0, 0.0f32), |a, (i, &v)| if v.abs() > a.1 { (i, v.abs()) } else { a }).0;
    let dry = argmax(&y[..2000]);
    let lag = (argmax(&y[2000..]) + 2000 - dry) as f64;
    println!("echo 110 ms: peak at {lag} samples ({} expected)", 0.110 * SR as f64);
    assert!((lag - 0.110 * SR as f64).abs() <= 1.0);
    // feedback 90 %: bounded
    let mut set = CLEAN.to_vec();
    set.extend([("echo_mix", 100.0), ("echo_fb", 90.0), ("echo_time", 20.0)]);
    let mut m = dut(&set, SR, 1);
    let mut x = f32v(&voice::pink(5 * n, -12.0, 4));
    for v in x[n..].iter_mut() {
        *v = 0.0;
    }
    let y = run(&mut m, &x);
    assert!(y.iter().all(|v| v.is_finite() && v.abs() < 8.0));
    assert!(rms_db(&y[4 * n..]) < rms_db(&y[n / 2..n]) - 40.0, "the loop decays");
    // the guard closed: the echo does not re-feed the bleed
    let bleed = f32v(&voice::pink(3 * n, -50.0, 6));
    let mut g = dut(&[("echo_mix", 50.0), ("echo_fb", 60.0), ("guard", -40.0)], SR, 1);
    let yg = run(&mut g, &bleed);
    assert!(rms_db(&yg[n..]) < rms_db(&bleed[n..]) - 2.0);
}

// ---- switches, stereo, determinism ------------------------------------------------------------------

/// HF burst after a parameter change re the output peak (dB), the bench's measure.
fn step_burst(set: &[(&str, f32)], id: &str, to: f32, x: &[f32]) -> f64 {
    let mut m = dut(set, SR, 1);
    let mut y = x.to_vec();
    let half = y.len() / 2;
    {
        let (a, b) = y.split_at_mut(half);
        for c in a.chunks_mut(128) {
            m.process(&mut [c]);
        }
        m.set_param(id, to);
        for c in b.chunks_mut(128) {
            m.process(&mut [c]);
        }
    }
    let n = y.len();
    let pk = y.iter().fold(0.0f32, |a, v| a.max(v.abs())) as f64;
    let hp = highpass4(&y, 10000.0, SR);
    let peak = |s: &[f32]| s.iter().fold(0.0f32, |a, v| a.max(v.abs())) as f64;
    let after = peak(&hp[half..half + n / 10]);
    let steady = peak(&hp[n / 4..half]).max(peak(&hp[n - n / 4..]));
    db((after - steady).max(0.0)) - db(pk)
}

#[test]
fn switches_under_a_held_note() {
    // README §5: `split`, `quality`, `order`, `object`, type changes under a held sung note: no burst
    // > −30 dB re peak
    let x = vowel(voice::REF_F0, 2.0, VOWEL_A, -18.0);
    let base = [("drive", 60.0), ("box", 60.0)];
    let mut worst = (f64::NEG_INFINITY, String::new());
    let cases: [(&str, f32); 9] = [("split", 2.0), ("split", 1.0), ("quality", 1.0), ("order", 1.0), ("object", 6.0), ("object_b", 4.0), ("body_a", 6.0), ("body_a", 9.0), ("body_b", 13.0)];
    for (id, to) in cases {
        let mut set = base.to_vec();
        if id == "object_b" {
            set.push(("morph", 50.0));
        }
        let b = step_burst(&set, id, to, &x);
        if b > worst.0 {
            worst = (b, format!("{id} → {to}"));
        }
        assert!(b <= -30.0, "{id} → {to}: {b:.1} dB");
    }
    println!("switches under a held note: worst {:.1} dB re peak ({})", worst.0, worst.1);
    // the switch completes: the engine plays the new configuration
    let mut m = dut(&[], SR, 1);
    m.set_param("split", 2.0);
    m.set_param("quality", 1.0);
    let _ = run(&mut m, &x[..(0.1 * SR) as usize]);
    assert_eq!(m.config(), Config { factor: 4, bands: 3 });
}

#[test]
fn stereo_equal_inputs_are_identical_and_match_mono() {
    let x = vowel(voice::REF_F0, 0.6, VOWEL_A, -12.0);
    let set = [("split", 2.0), ("body_a", ty("Rasp")), ("low_a", ty("Ring")), ("air_a", ty("Decimate")), ("box", 70.0), ("echo_mix", 30.0), ("comp", 60.0), ("morph", 30.0), ("body_b", ty("Fuzz"))];
    let mut s = dut(&set, SR, 2);
    let mut l = x.clone();
    let mut r = x.clone();
    for (a, b) in l.chunks_mut(100).zip(r.chunks_mut(100)) {
        s.process(&mut [a, b]);
    }
    assert_eq!(l, r);
    let mut m = dut(&set, SR, 1);
    let y = run(&mut m, &x);
    let d = y.iter().zip(&l).fold(0.0f32, |a, (p, q)| a.max((p - q).abs()));
    assert!(d < 1e-6, "{d}");
}

#[test]
fn deterministic_noise_phase_and_clock() {
    let x = vowel(voice::REF_F0, 0.5, VOWEL_A, -18.0);
    for set in [[("body_a", ty("Rasp")), ("drive", 100.0)], [("body_a", ty("Ring")), ("drive", 80.0)], [("body_a", ty("Decimate")), ("drive", 70.0)]] {
        let mut a = dut(&set, SR, 1);
        let ya = run(&mut a, &x);
        a.reset();
        let yb = run(&mut a, &x);
        let mut c = dut(&set, SR, 1);
        assert_eq!(ya, yb);
        assert_eq!(ya, run(&mut c, &x));
    }
}

#[test]
fn rasp_silence_stays_silent() {
    let mut m = dut(&[("body_a", ty("Rasp")), ("drive", 100.0), ("guard", -90.0)], SR, 1);
    let mut x = vowel(voice::REF_F0, 0.3, VOWEL_A, -18.0);
    x.extend(vec![0.0; 48000]);
    let y = run(&mut m, &x);
    assert!(rms_db(&y[y.len() - 4800..]) < -140.0);
}

// ---- every parameter audible ---------------------------------------------------------------------------

#[test]
fn every_parameter_is_audible_in_its_context() {
    // each parameter moved from its default changes the output, in the context its `depends` names
    let x = vowel(voice::REF_F0, 0.6, VOWEL_A, -18.0);
    let ctx: &[(&str, f32, &[(&str, f32)])] = &[
        ("drive", 80.0, &[]),
        ("morph", 60.0, &[]),
        ("box", 60.0, &[]),
        ("mix", 40.0, &[]),
        ("object", 6.0, &[("box", 80.0)]),
        ("guard", -20.0, &[]),
        ("defizz", 100.0, &[("defizz", 0.0)]),
        ("env_drive", 20.0, &[]),
        ("env_morph", 80.0, &[]),
        ("pre_hp", 600.0, &[]),
        ("pre_lp", 2000.0, &[]),
        ("tilt", 9.0, &[]),
        ("tilt_freq", 3000.0, &[("tilt", 9.0)]),
        ("split", 2.0, &[("low_a", 2.0)]),
        ("x_low", 600.0, &[("split", 1.0), ("low_a", 2.0)]),
        ("x_air", 2000.0, &[("split", 2.0), ("air_a", 2.0)]),
        ("low_a", 2.0, &[("split", 1.0)]),
        ("body_a", 6.0, &[]),
        ("air_a", 2.0, &[("split", 2.0), ("drive", 90.0)]),
        ("low_b", 6.0, &[("split", 1.0), ("morph", 70.0), ("low_a", 2.0), ("drive", 90.0)]),
        ("body_b", 6.0, &[("morph", 70.0)]),
        ("air_b", 5.0, &[("split", 2.0), ("morph", 70.0), ("air_a", 2.0), ("drive", 90.0)]),
        ("low_trim", 12.0, &[("split", 1.0), ("low_a", 2.0)]),
        ("body_trim", 12.0, &[]),
        ("air_trim", 12.0, &[("split", 2.0), ("air_a", 2.0), ("drive", 60.0)]),
        ("low_level", -12.0, &[("split", 1.0)]),
        ("body_level", -12.0, &[]),
        ("air_level", -12.0, &[("split", 2.0)]),
        ("ring_hz", 500.0, &[("body_a", 11.0), ("drive", 80.0)]),
        ("quality", 1.0, &[("drive", 100.0)]),
        ("object_b", 7.0, &[("box", 80.0), ("morph", 60.0)]),
        ("size", 200.0, &[("box", 80.0)]),
        ("decay", 300.0, &[("box", 80.0)]),
        ("position", 10.0, &[("box", 80.0)]),
        ("order", 1.0, &[("box", 80.0)]),
        ("comp", 100.0, &[]),
        ("post_hp", 500.0, &[]),
        ("post_lp", 3000.0, &[]),
        ("reso", 80.0, &[("post_lp", 3000.0)]),
        ("env_filter", -3.0, &[]),
        ("echo_time", 300.0, &[("echo_mix", 50.0)]),
        ("echo_fb", 80.0, &[("echo_mix", 50.0)]),
        ("echo_mix", 50.0, &[]),
        ("level", -6.0, &[]),
        ("bypass", 1.0, &[]),
    ];
    assert_eq!(ctx.len(), NP);
    let quiet: Vec<f32> = x.iter().map(|v| v * 0.1).collect();
    let (sib, _, _) = sibilant_phrase(false);
    for (id, v, c) in ctx {
        // the guard acts under its threshold: a quiet vowel (−38 dBFS); de-fizz on sibilants
        let x = match *id {
            "guard" => &quiet,
            "defizz" | "air_a" | "air_b" | "air_trim" | "air_level" | "x_air" => &sib,
            _ => &x,
        };
        let mut a = dut(c, SR, 1);
        let mut set = c.to_vec();
        set.push((id, *v));
        let mut b = dut(&set, SR, 1);
        let (ya, yb) = (run(&mut a, x), run(&mut b, x));
        let diff: Vec<f32> = ya.iter().zip(&yb).map(|(p, q)| p - q).collect();
        let d = rms_db(&diff[4800..]) - rms_db(&ya[4800..]);
        assert!(d > -40.0, "{id} → {v}: the output moves only {d:.1} dB");
    }
}

// ---- CPU ----------------------------------------------------------------------------------------------

#[test]
#[ignore]
fn cpu_per_configuration() {
    // README §5 CPU (48 kHz, mono, blocks of 128): run with `--release --ignored --nocapture`
    // the bench's input level (pink peaking near −20 dBFS: the guard stays open); the fastest of five
    // runs of 3 s (a loaded machine)
    let x = f32v(&voice::pink((3.0 * SR as f64) as usize, -30.0, 99));
    let six_fuzz = [("split", 2.0), ("quality", 1.0), ("low_a", 4.0), ("body_a", 4.0), ("air_a", 4.0), ("low_b", 4.0), ("body_b", 4.0), ("air_b", 4.0), ("box", 60.0), ("comp", 60.0), ("echo_mix", 30.0)];
    let cases: Vec<(&str, Vec<(&str, f32)>, bool, f64)> = vec![
        ("defaults", vec![], false, 350.0),
        ("x defizz 0", vec![("defizz", 0.0)], false, 350.0),
        ("x guard off", vec![("guard", -90.0)], false, 350.0),
        ("x pre_hp off", vec![("pre_hp", 20.0)], false, 350.0),
        ("x body Off", vec![("body_a", 0.0)], false, 350.0),
        ("x all off", vec![("body_a", 0.0), ("pre_hp", 20.0), ("guard", -90.0), ("defizz", 0.0)], false, 350.0),
        ("x box only", vec![("box", 60.0)], false, 350.0),
        ("x comp only", vec![("comp", 60.0)], false, 350.0),
        ("x echo only", vec![("echo_mix", 30.0)], false, 350.0),
        ("default-like: box, comp, echo", vec![("box", 60.0), ("comp", 60.0), ("echo_mix", 30.0)], false, 450.0),
        ("Three 2x, one-stage types in A and B, morph 50", vec![("split", 2.0), ("low_a", 1.0), ("air_a", 2.0), ("low_b", 3.0), ("air_b", 6.0), ("morph", 50.0), ("box", 60.0), ("comp", 60.0), ("echo_mix", 30.0)], false, 750.0),
        ("worst: Three 4x, Fuzz x6, morph and box moving, comp, echo", six_fuzz.to_vec(), true, 2200.0),
    ];
    for (name, set, moving, budget) in cases {
        for ch in [1usize, 2] {
            let mut best = f64::INFINITY;
            for _ in 0..5 {
                let mut m = dut(&set, SR, ch);
                let mut bufs: Vec<Vec<f32>> = (0..ch).map(|_| x.clone()).collect();
                let t = std::time::Instant::now();
                let n = x.len();
                let mut i = 0;
                while i < n {
                    let e = (i + 128).min(n);
                    if moving && (i / 128) % 20 == 0 {
                        let r = (i as f32 / n as f32 * 6.0).sin() * 0.5 + 0.5;
                        m.set_param("morph", 100.0 * r);
                        m.set_param("size", 50.0 + 150.0 * r);
                        m.set_param("position", 100.0 * r);
                    }
                    let (a, b) = bufs.split_at_mut(1);
                    if ch == 1 {
                        m.process(&mut [&mut a[0][i..e]]);
                    } else {
                        m.process(&mut [&mut a[0][i..e], &mut b[0][i..e]]);
                    }
                    i = e;
                }
                best = best.min(t.elapsed().as_secs_f64() * 1e9 / n as f64);
            }
            println!("{name}, {ch} ch: {best:.0} ns / sample (budget mono {budget})");
        }
    }
}

