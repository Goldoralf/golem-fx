// SPDX-License-Identifier: GPL-3.0-or-later
//! The reverb's own bench suite (README.md §6).

use super::*;

const SR: f32 = 48000.0;

fn rv(s: Settings) -> Reverb {
    let mut r = Reverb::new();
    r.prepare(SR, 4800, 2);
    r.set(s);
    r.reset();
    r
}

/// Wet only, flat: the tail alone.
fn wet() -> Settings {
    Settings::default().with(p::MIX, 100.0).with(p::KILL_DRY, 1.0).with(p::EARLY, 0.0).with(p::HICUT, 20000.0).with(p::LOWCUT, 20.0)
}

fn ir(r: &mut Reverb, secs: f32) -> Vec<f32> {
    let n = (SR * secs) as usize;
    let mut l = vec![0.0f32; n];
    l[0] = 1.0;
    let mut rr = l.clone();
    for (a, b) in l.chunks_mut(256).zip(rr.chunks_mut(256)) {
        r.process(&mut [a, b]);
    }
    l.iter().zip(&rr).map(|(a, b)| 0.5 * (a + b)).collect()
}

/// RT60 by Schroeder integration, fitted between −5 and −25 dB.
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

#[test]
fn decay_is_the_rt60() {
    for algo in [Algo::Hall, Algo::Room, Algo::Chamber, Algo::Cathedral, Algo::Plate, Algo::Cloud] {
        // the plate at golem's pinned size (15 m: Dattorro's own tank), from 1 s (one half-loop is 0.37 s)
        let rts: &[f32] = if algo == Algo::Plate { &[1.0, 2.0, 8.0] } else { &[0.5, 2.0, 8.0] };
        for &rt in rts {
            let size = if algo == Algo::Plate { 15.0 } else { 24.0 };
            let mut r = rv(wet().with(p::ALGO, algo as u8 as f32).with(p::DECAY, rt).with(p::SIZE, size));
            let y = ir(&mut r, rt * 1.6 + 0.5);
            let got = rt60(&y);
            eprintln!("{algo:?} {rt} s: {got:.3}");
            assert!((got / rt as f64 - 1.0).abs() < 0.15, "{algo:?} {rt} s: {got:.3}");
        }
    }
}

#[test]
fn bass_decay_ratio() {
    // bass_mult 2: lows ring twice as long (Hall)
    let low_rt = |bm: f32| {
        let mut r = rv(wet().with(p::DECAY, 1.0).with(p::BASS_MULT, bm).with(p::BASS_XOVER, 300.0));
        let y = ir(&mut r, 3.5);
        let mut lp = Svf::new();
        lp.set(SvfMode::LowPass, 100.0, 0.707, 0.0, SR);
        let yl: Vec<f32> = y.iter().map(|v| lp.process(*v)).collect();
        rt60(&yl)
    };
    let ratio = low_rt(2.0) / low_rt(1.0);
    assert!(ratio > 1.6 && ratio < 2.3, "{ratio:.2}");
}

#[test]
fn freeze_holds_and_shimmer_does_not_climb() {
    for shimmer in [0.0f32, 60.0] {
        let mut r = rv(wet().with(p::DECAY, 3.0).with(p::SHIMMER, shimmer));
        let n = 48000;
        // noise: a frozen sine keeps a few modes that beat, so its level swings although nothing is lost
        let mut rng = Rng::new(11);
        let mut l: Vec<f32> = (0..n).map(|_| 0.2 * rng.bipolar()).collect();
        let mut rr = l.clone();
        for (a, b) in l.chunks_mut(256).zip(rr.chunks_mut(256)) {
            r.process(&mut [a, b]);
        }
        r.set(r.settings().with(p::FREEZE, 1.0));
        let mut l = vec![0.0f32; 48000 * 30];
        let mut rr = l.clone();
        for (a, b) in l.chunks_mut(256).zip(rr.chunks_mut(256)) {
            r.process(&mut [a, b]);
        }
        // energy of both sides, 4 s windows
        let en = |a: usize, b: usize| (a..b).map(|k| (l[k] as f64).powi(2) + (rr[k] as f64).powi(2)).sum::<f64>();
        let db = 10.0 * (en(48000 * 25, 48000 * 29) / en(48000 * 2, 48000 * 6)).log10();
        assert!(db.abs() < 0.5, "shimmer {shimmer}: freeze drifts {db:.2} dB");
    }
}

#[test]
fn shimmer_stays_bounded() {
    let mut r = rv(wet().with(p::DECAY, 20.0).with(p::SHIMMER, 100.0).with(p::SHIMMER_PITCH, 12.0));
    let mut rng = Rng::new(4);
    let n = 48000 * 60;
    let mut l: Vec<f32> = (0..n).map(|_| 0.2 * rng.bipolar()).collect();
    let mut rr = l.clone();
    for (a, b) in l.chunks_mut(512).zip(rr.chunks_mut(512)) {
        r.process(&mut [a, b]);
    }
    let pk = l[n - 48000..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(pk.is_finite() && pk < 4.0, "{pk}");
}

#[test]
fn gated_burst_has_its_length() {
    // size 24 m → 0.05 + 0.55 · 19/55 s
    let want = (0.05 + 0.55 * 19.0 / 55.0) * SR as f64;
    let mut r = rv(wet().with(p::ALGO, 9.0).with(p::DIFFUSION, 0.0));
    let y = ir(&mut r, 1.5);
    let pk = y.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let last = y.iter().rposition(|v| v.abs() > 0.05 * pk).unwrap() as f64;
    // within the taps' spacing (240 ms / 128) plus the end fade's 5 % point
    assert!((last - want).abs() < 0.0025 * SR as f64, "ends at {:.1} ms, want {:.1}", last / 48.0, want / 48.0);
}

#[test]
fn every_algorithm_is_finite_and_stable() {
    for a in 0..11 {
        let mut r = rv(Settings::default().with(p::ALGO, a as f32).with(p::DECAY, 60.0).with(p::DRIVE, 100.0).with(p::SHIMMER, 50.0).with(p::MOD, 100.0).with(p::SIZE, 60.0).with(p::ERA, 2.0));
        let mut rng = Rng::new(9);
        let mut l: Vec<f32> = (0..96000).map(|_| 0.3 * rng.bipolar()).collect();
        let mut rr = l.clone();
        for (x, y) in l.chunks_mut(256).zip(rr.chunks_mut(256)) {
            r.process(&mut [x, y]);
        }
        assert!(l.iter().chain(rr.iter()).all(|v| v.is_finite() && v.abs() < 10.0), "algo {a}");
    }
}

#[test]
#[ignore]
fn profile_blocks() {
    use gfx_dsp::reverb::{Diffuser, Fdn, Taps};
    use std::time::Instant;
    let n = 48000 * 10;
    let mut rng = Rng::new(1);
    let x: Vec<f32> = (0..n).map(|_| 0.1 * rng.bipolar()).collect();
    let ns = |t: Instant| t.elapsed().as_secs_f64() * 1e9 / n as f64;
    let mut d = Diffuser::default();
    d.prepare(SR, 4, 40.0, 1);
    let t = Instant::now();
    let mut acc = 0.0f32;
    for v in &x {
        let mut a = [*v, *v, -*v, -*v, *v, -*v, *v, -*v];
        d.process(&mut a);
        acc += a[0];
    }
    eprintln!("diffuser   {:6.1} ns", ns(t));
    let mut f = Fdn::default();
    f.prepare(SR, 3.0);
    f.set_decay(2.0, 2.0, 1.0, 300.0, 7000.0);
    f.set_mod(0.5, 0.5);
    f.settle();
    let t = Instant::now();
    for v in &x {
        let (l, _) = f.process(&[*v; 16]);
        acc += l;
    }
    eprintln!("fdn        {:6.1} ns", ns(t));
    let mut tp = Taps::default();
    tp.prepare(48000, 24);
    for k in 0..24 {
        tp.set(k, 100.0 + 300.0 * k as f64, 0.1);
    }
    tp.settle();
    let t = Instant::now();
    for v in &x {
        tp.line.push(*v);
        acc += tp.read(0, 1);
    }
    eprintln!("taps x24   {:6.1} ns", ns(t));
    let mut r = rv(Settings::default());
    let t = Instant::now();
    for c in 0..(n / 128) {
        let _ = c;
        r.control();
    }
    eprintln!("control /128 samples, per sample {:6.1} ns", t.elapsed().as_secs_f64() * 1e9 / n as f64 * 8.0);
    let mut r = rv(Settings::default());
    let mut l = x.clone();
    let mut rr = x.clone();
    let t = Instant::now();
    for (a, b) in l.chunks_mut(128).zip(rr.chunks_mut(128)) {
        r.process(&mut [a, b]);
    }
    eprintln!("whole hall {:6.1} ns   ({acc})", ns(t));
}

fn energy(y: &[f32]) -> f64 {
    y.iter().map(|v| (*v as f64).powi(2)).sum()
}

#[test]
fn every_tail_is_heard_at_a_comparable_level() {
    // the plate's input filter once let 1e-6 of the energy through: every algorithm within 10 dB of the hall
    let hall = energy(&ir(&mut rv(wet()), 2.0));
    for algo in Algo::ALL {
        let e = energy(&ir(&mut rv(wet().with(p::ALGO, algo as u8 as f32)), 2.0));
        let db = 10.0 * (e / hall).log10();
        eprintln!("{algo:?}: {db:+.1} dB");
        assert!(db.abs() < 10.0, "{algo:?}: {db:+.1} dB against the hall");
    }
}

#[test]
fn era_keeps_the_decay_and_the_level() {
    for algo in [Algo::Hall, Algo::Plate, Algo::Spring, Algo::Bloom] {
        let modern = ir(&mut rv(wet().with(p::ALGO, algo as u8 as f32).with(p::DECAY, 2.0)), 3.5);
        for era in [1.0f32, 2.0] {
            let y = ir(&mut rv(wet().with(p::ALGO, algo as u8 as f32).with(p::DECAY, 2.0).with(p::ERA, era)), 3.5);
            let (rm, re) = (rt60(&modern), rt60(&y));
            // the level in the band every era keeps (the eras drop the air above it, by design)
            let low = |x: &[f32]| {
                let mut lp = [Svf::new(), Svf::new()];
                for f in lp.iter_mut() {
                    f.set(SvfMode::LowPass, 4000.0, 0.707, 0.0, SR);
                }
                let y: Vec<f32> = x.iter().map(|v| { let a = lp[0].process(*v); lp[1].process(a) }).collect();
                energy(&y)
            };
            let db = 10.0 * (low(&y) / low(&modern)).log10();
            eprintln!("{algo:?} era {era}: rt60 {rm:.2} → {re:.2} s, level {db:+.1} dB");
            assert!((re / rm - 1.0).abs() < 0.15, "{algo:?} era {era}: rt60 {rm:.2} → {re:.2}");
            assert!(db.abs() < 3.0, "{algo:?} era {era}: {db:+.1} dB");
        }
    }
}

#[test]
fn era_runs_the_tail_in_the_period_band() {
    // white noise in, the wet's share above the era's band: Modern keeps its air, 80s and 70s lose it
    let band = |era: f32| {
        let mut r = rv(wet().with(p::ERA, era));
        let mut rng = Rng::new(7);
        let n = 48000;
        let mut l: Vec<f32> = (0..n).map(|_| 0.25 * rng.bipolar()).collect();
        let mut rr = l.clone();
        for (a, b) in l.chunks_mut(256).zip(rr.chunks_mut(256)) {
            r.process(&mut [a, b]);
        }
        // energy above 14 kHz over 1 – 4 kHz, by a 4-pole high-pass and band-pass (Svf)
        let mut hp = [Svf::new(), Svf::new()];
        let mut bp = Svf::new();
        for h in hp.iter_mut() {
            h.set(SvfMode::HighPass, 14000.0, 0.707, 0.0, SR);
        }
        bp.set(SvfMode::BandPass, 2000.0, 0.7, 0.0, SR);
        let (mut eh, mut eb) = (0.0f64, 0.0f64);
        for &v in &l[n / 4..] {
            let a = hp[0].process(v);
            eh += (hp[1].process(a) as f64).powi(2);
            eb += (bp.process(v) as f64).powi(2);
        }
        10.0 * (eh / eb).log10()
    };
    let (m, e80, e70) = (band(0.0), band(1.0), band(2.0));
    eprintln!("above 14 kHz vs 2 kHz: modern {m:.1} dB, 80s {e80:.1} dB, 70s {e70:.1} dB");
    assert!(e80 < m - 15.0 && e70 < e80, "{m:.1} / {e80:.1} / {e70:.1}");
}

#[test]
fn era_switch_is_clean() {
    // switching era mid-tail dips through the gate: no step, no NaN
    let mut r = rv(Settings::default().with(p::MIX, 50.0));
    let mut rng = Rng::new(3);
    let mut prev = 0.0f32;
    let mut worst = 0.0f32;
    for k in 0..60 {
        if k % 20 == 10 {
            let e = ((k / 20) % 3) as f32 + 1.0;
            r.set(r.settings().with(p::ERA, e % 3.0));
        }
        let mut l: Vec<f32> = (0..256).map(|_| 0.0).collect();
        // a slow sine plus a little noise: any jump stands out against the smooth signal
        for (i, v) in l.iter_mut().enumerate() {
            let t = (k * 256 + i) as f32 / SR;
            *v = 0.3 * (TAU_F * 220.0 * t).sin() + 0.001 * rng.bipolar();
        }
        let mut rr = l.clone();
        r.process(&mut [&mut l, &mut rr]);
        for &v in &l {
            assert!(v.is_finite());
            worst = worst.max((v - prev).abs());
            prev = v;
        }
    }
    // the sine alone moves up to 0.3 · 2π · 220 / 48000 ≈ 0.009 per sample; the wet adds its own
    assert!(worst < 0.05, "largest step {worst}");
}

#[test]
#[ignore]
fn profile_era() {
    use std::time::Instant;
    let n = 48000 * 10;
    let mut rng = Rng::new(1);
    let x: Vec<f32> = (0..n).map(|_| 0.1 * rng.bipolar()).collect();
    for algo in [Algo::Hall, Algo::Plate, Algo::Spring] {
        for era in [0.0f32, 1.0, 2.0] {
            let mut r = rv(Settings::default().with(p::ALGO, algo as u8 as f32).with(p::ERA, era));
            let (mut l, mut rr) = (x.clone(), x.clone());
            let t = Instant::now();
            for (a, b) in l.chunks_mut(128).zip(rr.chunks_mut(128)) {
                r.process(&mut [a, b]);
            }
            eprintln!("{algo:?} era {era}: {:6.1} ns / frame", t.elapsed().as_secs_f64() * 1e9 / n as f64);
        }
    }
}
