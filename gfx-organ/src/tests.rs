// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5) on synthetic strings (the prototype's model, with a −100 dBFS noise
//! floor) and the pedal's own paths. The sieve's rows are in `sieve_bench.rs`.

use super::*;
use crate::sieve_bench::{note_hz, play, plain, render};

const SR: f32 = 48000.0;

fn make(sr: f32, ch: usize, set: &[(&str, f32)]) -> Organ {
    let mut o = Organ::new();
    for &(id, v) in set {
        o.set_param(id, v);
    }
    o.prepare(sr, 512, ch);
    o.reset();
    o
}

fn run(o: &mut Organ, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for c in y.chunks_mut(256) {
        o.process(&mut [c]);
    }
    y
}

/// Amplitude of the component at `f` (Hann-windowed DFT over `x`).
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

/// The largest component within ±12 ¢ of `f` (three probes).
fn level_near(x: &[f32], f: f64, sr: f64) -> f64 {
    [-8.0f64, -4.0, 0.0, 4.0, 8.0].iter().map(|c| amp_at(x, f * (c / 1200.0).exp2(), sr)).fold(0.0, f64::max)
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-12).log10()
}

#[test]
fn latency_dry_at_sample_zero() {
    for sr in [44100.0f32, 48000.0, 96000.0, 192000.0] {
        let mut o = make(sr, 1, &[("dry", 0.0)]);
        assert_eq!(o.latency(), 0);
        let mut x = vec![0.0f32; 512];
        x[0] = 0.5;
        o.process(&mut [&mut x]);
        assert!((x[0] - 0.5).abs() < 1e-6, "{sr}: {}", x[0]);
    }
}

/// README §5 "Drawbar accuracy": single notes D3 … A4 (above the fold), Pure tuning, each sounding
/// drawbar within ±1 dB of its target after one gain.
#[test]
fn drawbar_accuracy() {
    let sr = SR as f64;
    let mut worst: f64 = 0.0;
    for reg in ["888000000", "888800000", "838000000", "808808008", "888888888", "668848588"] {
        let mut line = format!("{reg}:");
        for nm in ["D3", "G3", "B3", "E4", "A4"] {
            let f0 = note_hz(nm) * (1.0f64 + 1e-4).sqrt();
            let x = play(&[(0.05, note_hz(nm), 0)], 1.6, 3, sr, false);
            let mut o = plain(reg);
            o.set_param("tuning", 1.0);
            o.reset();
            let (y, _) = render(&mut o, &x, None);
            let body = &y[(0.5 * sr) as usize..(1.5 * sr) as usize];
            let d: Vec<u8> = reg.bytes().map(|c| c - b'0').collect();
            let mut errs = vec![];
            for (k, &r) in law::RATIOS.iter().enumerate() {
                if d[k] == 0 || r * f0 > law::FOLD_HI * 0.97 {
                    continue;
                }
                errs.push(db(level_near(body, r * f0, sr)) - db(drawbar_gain(d[k])));
            }
            let g = errs.iter().sum::<f64>() / errs.len() as f64;
            let w = errs.iter().map(|e| (e - g).abs()).fold(0.0, f64::max);
            worst = worst.max(w);
            line += &format!(" {nm} {w:.2}");
        }
        println!("{line} dB (worst drawbar off its target after one gain)");
    }
    assert!(worst <= 1.0, "{worst}");
}

/// README §5 "Drawbar step": 16' at 8 against the 8' stepped 8 → 1, −3 dB per step ±0.3 dB.
#[test]
fn drawbar_step() {
    let sr = SR as f64;
    let f0 = note_hz("A3") * (1.0f64 + 1e-4).sqrt();
    let x = play(&[(0.05, note_hz("A3"), 0)], 1.4, 5, sr, false);
    let mut prev: Option<f64> = None;
    let mut worst: f64 = 0.0;
    for step in (1..=8).rev() {
        let reg = format!("80{step}000000");
        let mut o = plain(&reg);
        o.set_param("tuning", 1.0);
        o.reset();
        let (y, _) = render(&mut o, &x, None);
        let body = &y[(0.5 * sr) as usize..(1.3 * sr) as usize];
        let rel = db(level_near(body, f0, sr)) - db(level_near(body, 0.5 * f0, sr));
        if let Some(p) = prev {
            worst = worst.max((rel - p + 3.0f64).abs());
        }
        prev = Some(rel);
    }
    println!("8' stepped 8 → 1: worst step off −3 dB by {worst:.2} dB");
    assert!(worst <= 0.3, "{worst}");
}

/// README §5 "Percussion": 3rd fast decays to −60 dB in 1.0 s ±10 % (slow 4 s); Normal drops the
/// drawbars 4.4 dB.
#[test]
fn percussion() {
    let sr = SR as f64;
    let f0 = note_hz("A3") * (1.0f64 + 1e-4).sqrt();
    let x = play(&[(0.05, note_hz("A3"), 0)], 2.0, 2, sr, false);
    let level = |set: &[(&str, f32)], f: f64, a: f64, b: f64| {
        let mut o = plain("008000000");
        for &(id, v) in set {
            o.set_param(id, v);
        }
        o.reset();
        let (y, _) = render(&mut o, &x, None);
        db(level_near(&y[(a * sr) as usize..(b * sr) as usize], f, sr))
    };
    // the 3rd harmonic of the note: the percussion alone (8' only otherwise); its decay between two
    // windows once the clean bank holds the note (the fast bank's attack window adds to it before)
    let p1 = level(&[("perc", 1.0), ("perc_harm", 1.0), ("perc_decay", 1.0)], 3.0 * f0, 0.35, 0.45);
    let p2 = level(&[("perc", 1.0), ("perc_harm", 1.0), ("perc_decay", 1.0)], 3.0 * f0, 0.65, 0.75);
    let per_s = (p1 - p2) / 0.3;
    let t60 = 60.0 / per_s;
    let s1 = level(&[("perc", 1.0), ("perc_harm", 1.0), ("perc_decay", 0.0)], 3.0 * f0, 0.45, 0.65);
    let s2 = level(&[("perc", 1.0), ("perc_harm", 1.0), ("perc_decay", 0.0)], 3.0 * f0, 1.05, 1.25);
    let t60s = 60.0 / ((s1 - s2) / 0.6);
    let on = level(&[("perc", 1.0), ("perc_vol", 0.0)], f0, 0.5, 1.5);
    let off = level(&[("perc", 0.0)], f0, 0.5, 1.5);
    println!("percussion fast: T60 {t60:.2} s, slow {t60s:.2} s; Normal drop {:.2} dB", off - on);
    assert!((t60 - 1.0).abs() <= 0.1, "{t60}");
    assert!((t60s - 4.0).abs() <= 0.4, "{t60s}");
    assert!((off - on - 4.4).abs() <= 0.3, "{}", off - on);
}

#[test]
fn foldback_low_e() {
    // E2 with 16' only: it plays at 82.4 Hz (folded), nothing at 41 Hz
    let sr = SR as f64;
    let f0 = note_hz("E2");
    let x = play(&[(0.05, f0, 0)], 1.4, 1, sr, false);
    let mut o = plain("800000000");
    o.set_param("tuning", 1.0);
    o.reset();
    let (y, _) = render(&mut o, &x, None);
    let body = &y[(0.5 * sr) as usize..(1.3 * sr) as usize];
    let (at82, at41) = (db(level_near(body, f0, sr)), db(level_near(body, 0.5 * f0, sr)));
    println!("E2, 16' only: {at82:.1} dB at 82 Hz, {at41:.1} dB at 41 Hz");
    assert!(at82 - at41 > 30.0);
}

fn strum(sr: f64, dur: f64) -> Vec<f32> {
    let fs: Vec<f64> = ["E2", "B2", "E3", "G#3", "B3", "E4"].iter().map(|n| note_hz(n)).collect();
    let mut ev = vec![];
    for j in 0..(dur / 2.0) as usize {
        for (i, &f) in fs.iter().enumerate() {
            ev.push((0.05 + 2.0 * j as f64 + 0.004 * i as f64, f, i));
        }
    }
    play(&ev, dur, 7, sr, true)
}

#[test]
fn level_bound_and_determinism() {
    // −6 dBFS strum, 88 8888 888, percussion Normal, drive 100 %, organ +12: never over 0 dBFS
    let x = strum(SR as f64, 3.0);
    let set = [("register", 9.0f32), ("db4", 8.0), ("db223", 8.0), ("db2", 8.0), ("db135", 8.0), ("db113", 8.0), ("db1", 8.0), ("perc", 1.0), ("drive", 100.0), ("organ", 12.0), ("dry", 0.0)];
    let mut a = make(SR, 1, &set);
    let ya = run(&mut a, &x);
    let pk = ya.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    println!("level bound: peak {:.2} dBFS (dry 0 dB adds the input's −6)", 20.0 * pk.log10());
    let mut b = make(SR, 1, &set);
    let yb = run(&mut b, &x);
    assert!(ya.iter().zip(&yb).all(|(p, q)| p.to_bits() == q.to_bits()), "not deterministic");
    // the organ alone never past 0 dBFS
    let mut c = make(SR, 1, &[("register", 9.0f32), ("db4", 8.0), ("db223", 8.0), ("db2", 8.0), ("db135", 8.0), ("db113", 8.0), ("db1", 8.0), ("perc", 1.0), ("drive", 100.0), ("organ", 12.0)]);
    let yc = run(&mut c, &x);
    let pc = yc.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(pc <= 1.0, "{pc}");
    assert!(ya.iter().all(|v| v.is_finite()));
}

#[test]
fn every_register_plays_stereo_and_mono() {
    let x = strum(SR as f64, 2.0);
    for r in 0..10 {
        for ch in [1usize, 2] {
            let mut o = make(SR, ch, &[("register", r as f32)]);
            let mut l = x.clone();
            let mut rr = x.clone();
            for (a, b) in l.chunks_mut(256).zip(rr.chunks_mut(256)) {
                if ch == 2 {
                    o.process(&mut [a, b]);
                } else {
                    o.process(&mut [a]);
                }
            }
            let e = l[48000..].iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / l.len() as f64;
            assert!(l.iter().all(|v| v.is_finite() && v.abs() <= 1.5), "{}", REGISTERS[r]);
            assert!(e > 1e-6, "{} ({ch} ch) silent", REGISTERS[r]);
            if r == 1 && ch == 1 {
                println!("Trio mono RMS {:.1} dBFS, roots meter at the end {}", 10.0 * e.log10(), o.roots());
            }
        }
    }
}

#[test]
fn spacing_zero_is_mono() {
    let x = strum(SR as f64, 1.5);
    let mut o = make(SR, 2, &[("register", 1.0), ("spacing", 0.0)]);
    let mut l = x.clone();
    let mut r = x.clone();
    for (a, b) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        o.process(&mut [a, b]);
    }
    let d = l.iter().zip(&r).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    println!("2 / 2 at spacing 0: max |L − R| {d:.2e}");
    assert!(d < 1e-6);
}

#[test]
#[ignore]
fn cpu_report() {
    // README §2.6: ≤ 10 % of a core at 44.1 / 48 kHz worst case (Dual, a strum, 88 8888 888 with
    // percussion, scanner and Leslie); reported per rate and per register
    let worst: Vec<(&str, f32)> = vec![("register", 9.0), ("db4", 8.0), ("db223", 8.0), ("db2", 8.0), ("db135", 8.0), ("db113", 8.0), ("db1", 8.0), ("perc", 1.0), ("vibrato", 6.0), ("speaker", 1.0), ("leslie", 2.0), ("drive", 30.0)];
    for sr in [44100.0f32, 48000.0, 96000.0, 192000.0] {
        let x = strum(sr as f64, 8.0);
        let mut best = f64::MAX;
        for _ in 0..3 {
            let mut o = make(sr, 1, &worst);
            let t = std::time::Instant::now();
            let _ = run(&mut o, &x);
            best = best.min(t.elapsed().as_secs_f64());
        }
        let ns = best * 1e9 / x.len() as f64;
        println!("worst case at {:.1} kHz: {ns:.0} ns / sample, {:.1} % of a core", sr / 1000.0, ns * sr as f64 / 1e7);
    }
    let x = strum(48000.0, 8.0);
    for r in 0..10 {
        let mut best = f64::MAX;
        for _ in 0..3 {
            let mut o = make(SR, 1, &[("register", r as f32)]);
            let t = std::time::Instant::now();
            let _ = run(&mut o, &x);
            best = best.min(t.elapsed().as_secs_f64());
        }
        let ns = best * 1e9 / x.len() as f64;
        println!("{:8} at 48 kHz: {ns:.0} ns / sample, {:.1} %", REGISTERS[r], ns * 48000.0 / 1e7);
    }
}
