// SPDX-License-Identifier: GPL-3.0-or-later
//! The shimmer's own bench suite (README.md §7).

use super::*;
use gfx_dsp::lfo::Rng;

const SR: f32 = 48000.0;

fn sh(s: Settings) -> Shimmer {
    let mut r = Shimmer::new();
    r.prepare(SR, 4800, 2);
    r.set(s);
    r.reset();
    r
}

fn wet() -> Settings {
    Settings::default().with(p::MIX, 100.0).with(p::KILL_DRY, 1.0).with(p::PREDELAY, 0.0)
}

fn run(r: &mut Shimmer, l: &mut [f32], rr: &mut [f32]) {
    for (a, b) in l.chunks_mut(256).zip(rr.chunks_mut(256)) {
        r.process(&mut [a, b]);
    }
}

fn rt60(ir: &[f32]) -> f64 {
    let mut e: Vec<f64> = ir.iter().map(|v| (*v as f64).powi(2)).collect();
    for i in (0..e.len() - 1).rev() {
        e[i] += e[i + 1];
    }
    let e0 = e[0];
    let db: Vec<f64> = e.iter().map(|v| 10.0 * (v / e0).max(1e-30).log10()).collect();
    let t5 = db.iter().position(|v| *v < -5.0).unwrap_or(0) as f64;
    let t25 = db.iter().position(|v| *v < -25.0).unwrap_or(db.len() - 1) as f64;
    (t25 - t5) / SR as f64 * 3.0
}

/// Magnitude at f (Hann-windowed probe).
fn probe(v: &[f32], f: f64) -> f64 {
    let n = v.len();
    let w = std::f64::consts::TAU * f / SR as f64;
    let (mut re, mut im) = (0.0, 0.0);
    for (k, s) in v.iter().enumerate() {
        let h = 0.5 - 0.5 * (std::f64::consts::TAU * k as f64 / n as f64).cos();
        re += *s as f64 * h * (w * k as f64).cos();
        im += *s as f64 * h * (w * k as f64).sin();
    }
    (re * re + im * im).sqrt()
}

#[test]
fn decay_without_shimmer_is_the_rt60() {
    for space in 0..4 {
        for rt in [2.0f32, 8.0] {
            // `decay` is the mid-band T60: the high band is damped above `hicut`, so measure with it open
            let mut r = sh(wet().with(p::SHIMMER, 0.0).with(p::DECAY, rt).with(p::SPACE, space as f32).with(p::BLOOM, 50.0).with(p::HICUT, 16000.0));
            let n = (SR * (rt * 1.6 + 0.5)) as usize;
            let mut l = vec![0.0f32; n];
            l[0] = 1.0;
            let mut rr = l.clone();
            run(&mut r, &mut l, &mut rr);
            let m: Vec<f32> = l.iter().zip(&rr).map(|(a, b)| 0.5 * (a + b)).collect();
            let got = rt60(&m);
            assert!((got / rt as f64 - 1.0).abs() < 0.15, "space {space}, {rt} s: {got:.2}");
        }
    }
}

#[test]
fn the_first_pass_is_an_octave_up() {
    let mut r = sh(wet().with(p::SHIMMER, 100.0).with(p::DECAY, 4.0).with(p::HICUT, 16000.0).with(p::LOWCUT, 20.0).with(p::DETUNE, 0.0));
    let n = 48000 * 3;
    let mut l: Vec<f32> = (0..n).map(|i| if i < 48000 { 0.2 * (std::f32::consts::TAU * 440.0 * i as f32 / SR).sin() } else { 0.0 }).collect();
    let mut rr = l.clone();
    run(&mut r, &mut l, &mut rr);
    let seg = &l[48000..96000];
    // after the note: 880 Hz is there, well above its neighbours (the climb has begun)
    let (o, side) = (probe(seg, 880.0), probe(seg, 700.0).max(probe(seg, 1050.0)));
    assert!(o > 4.0 * side, "880 Hz {o:.3}, neighbours {side:.3}");
}

#[test]
fn stays_bounded_at_full_shimmer_in_every_space() {
    for space in 0..4 {
        for character in 0..2 {
            let mut r = sh(wet().with(p::SHIMMER, 100.0).with(p::DECAY, 20.0).with(p::SPACE, space as f32).with(p::CHARACTER, character as f32).with(p::VOICE2, 100.0).with(p::PITCH2, -12.0));
            let n = 48000 * 30;
            let mut l: Vec<f32> = (0..n).map(|i| {
                let t = i as f32 / SR;
                0.2 * (std::f32::consts::TAU * (100.0 + 50.0 * t) * t).sin()
            }).collect();
            let mut rr = l.clone();
            run(&mut r, &mut l, &mut rr);
            let pk = l[n - 48000..].iter().chain(rr[n - 48000..].iter()).fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(pk.is_finite() && pk < 4.0, "space {space} character {character}: {pk}");
        }
    }
}

#[test]
fn freeze_stops_the_climb() {
    let mut r = sh(wet().with(p::SHIMMER, 80.0).with(p::DECAY, 6.0));
    let mut rng = Rng::new(3);
    let mut l: Vec<f32> = (0..48000).map(|_| 0.2 * rng.bipolar()).collect();
    let mut rr = l.clone();
    run(&mut r, &mut l, &mut rr);
    r.set(r.settings().with(p::FREEZE, 1.0));
    let n = 48000 * 30;
    let mut l = vec![0.0f32; n];
    let mut rr = l.clone();
    run(&mut r, &mut l, &mut rr);
    // spectral centroid (8 probes) early vs late: the held pad does not move up
    let centroid = |v: &[f32]| {
        let fs = [250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0];
        let m: Vec<f64> = fs.iter().map(|f| probe(v, *f)).collect();
        fs.iter().zip(&m).map(|(f, a)| f * a).sum::<f64>() / m.iter().sum::<f64>()
    };
    let (a, b) = (centroid(&l[48000 * 2..48000 * 4]), centroid(&l[48000 * 26..48000 * 28]));
    assert!((b / a - 1.0).abs() < 0.15, "centroid {a:.0} → {b:.0} Hz");
    let en = |x: usize, y: usize| (x..y).map(|k| (l[k] as f64).powi(2) + (rr[k] as f64).powi(2)).sum::<f64>();
    let db = 10.0 * (en(48000 * 24, 48000 * 28) / en(48000 * 2, 48000 * 6)).log10();
    assert!(db.abs() < 1.0, "level drifts {db:.2} dB");
}
