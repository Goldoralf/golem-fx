// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5). Targets come from the model's own equations.

use super::*;

const SR: f32 = 48000.0;

fn met(s: Settings) -> Met {
    let mut m = Met::new();
    m.set(s);
    m.prepare(SR, 512, 1);
    m.reset();
    m
}

fn sine(freq: f64, amp: f64, n: usize, start: usize) -> Vec<f32> {
    (0..n).map(|i| (amp * (2.0 * std::f64::consts::PI * freq * (i + start) as f64 / SR as f64).sin()) as f32).collect()
}

/// Runs main = key = x in blocks; returns the gain trace (dB) per sample.
fn gains(m: &mut Met, x: &[f32]) -> Vec<f64> {
    let mut out = Vec::with_capacity(x.len());
    for chunk in x.chunks(256) {
        let mut main = vec![1.0f32; chunk.len()];
        let key: &[f32] = chunk;
        let k = [key];
        m.process_with_key(&mut [&mut main[..]], Some(&k[..]));
        out.extend(main.iter().map(|&g| if g <= 0.0 { -200.0 } else { 20.0 * (g as f64).log10() }));
    }
    out
}

#[test]
fn expansion_slope_is_the_zuuls() {
    let mut m = met(Settings { thresh: -30.0, ..Settings::default() });
    // a sine whose RMS is 10 dB under the threshold: RMS = −40 dB → peak −37 dB
    let amp = 10f64.powf(-40.0 / 20.0) * 2f64.sqrt();
    let g = gains(&mut m, &sine(500.0, amp, 48000, 0));
    let tail = &g[40000..];
    let mean = tail.iter().sum::<f64>() / tail.len() as f64;
    assert!((mean + 18.0).abs() < 0.5, "attenuation {mean} dB, expected −18");
}

#[test]
fn attack_is_immediate() {
    let mut m = met(Settings { thresh: -40.0, ..Settings::default() });
    let mut x = vec![0.0f32; 4800];
    x.extend(sine(1000.0, 10f64.powf(-20.0 / 20.0), 4800, 0));
    let g = gains(&mut m, &x);
    // within 1.5 ms of the note, the gain is open (S-shaped onset, no click)
    assert!(g[4800 + 72] > -1.0, "gain 1.5 ms after the note: {} dB", g[4800 + 72]);
}

#[test]
fn adaptive_timing_plunges_after_the_clamp() {
    // the level after a −30 dB step: with adaptive, it reaches −25 dB far sooner than without
    let run = |adaptive: bool| {
        let mut m = met(Settings { thresh: -90.0, adaptive, ..Settings::default() });
        let mut x = sine(200.0, 0.5, 48000, 0);
        x.extend(sine(200.0, 0.5 * 10f64.powf(-30.0 / 20.0), 48000, 48000));
        let mut t = None;
        let mut start = 0.0;
        for (i, chunk) in x.chunks(64).enumerate() {
            let mut main = chunk.to_vec();
            m.process_with_key(&mut [&mut main[..]], None);
            if i * 64 == 47936 {
                start = m.level_db();
            }
            if i * 64 > 48000 && t.is_none() && m.level_db() < start - 25.0 {
                t = Some(i * 64 - 48000);
            }
        }
        t.expect("level never fell 25 dB") as f64 / SR as f64 * 1000.0
    };
    let fast = run(true);
    let slow = run(false);
    assert!(fast < slow / 2.5, "adaptive {fast} ms vs fixed {slow} ms");
    // the first 6.5 dB still fall at the slow rate (≈ 85 ms): not instant
    assert!(fast > 80.0, "adaptive fall {fast} ms is too fast to be the circuit's");
}

#[test]
fn no_chatter_on_a_decaying_note() {
    let mut m = met(Settings { thresh: -40.0, ..Settings::default() });
    let x: Vec<f32> = (0..96000)
        .map(|i| {
            let t = i as f64 / SR as f64;
            (0.3 * (-t * 3.0).exp() * (2.0 * std::f64::consts::PI * 82.0 * t).sin()) as f32
        })
        .collect();
    let g = gains(&mut m, &x);
    let mut worst: f64 = 0.0;
    let mut low = f64::INFINITY;
    for &v in &g[9600..] {
        low = low.min(v);
        worst = worst.max(v - low);
    }
    assert!(worst <= 0.2, "gain rose {worst} dB while the note decayed");
}

#[test]
fn hold_then_minimum_fall() {
    let mut m = met(Settings { thresh: -40.0, hold: 50.0, release: 0.0, ..Settings::default() });
    let mut x = sine(1000.0, 0.3, 24000, 0);
    x.extend(vec![0.0f32; 24000]);
    let g = gains(&mut m, &x);
    // find when the level drops under the threshold, then: open for ~50 ms, then no step steeper
    // than the 5 ms minimum fall (90 dB / 5 ms per sample)
    let max_step = 90.0 / (MIN_FALL_MS * 0.001 * SR as f64) + 1e-6;
    for w in g[24000..].windows(2) {
        assert!(w[0] - w[1] <= max_step + 1.0, "step of {} dB", w[0] - w[1]);
    }
}

#[test]
fn key_fallback_latches() {
    let mut m = met(Settings { thresh: -40.0, ..Settings::default() });
    let loud = sine(1000.0, 0.3, 4800, 0);
    let silent = vec![0.0f32; 4800];
    // no key seen yet: the main input drives the detector, the gate opens on a loud main
    let mut main = loud.clone();
    let k = [&silent[..]];
    m.process_with_key(&mut [&mut main[..]], Some(&k[..]));
    assert!(m.is_open(), "before any key, the main input must drive the detector");
    // a key burst, then a silent key under a loud (hissing) main: stays closed
    let mut main = loud.clone();
    let k = [&loud[..]];
    m.process_with_key(&mut [&mut main[..]], Some(&k[..]));
    for _ in 0..20 {
        let mut main = loud.clone();
        let k = [&silent[..]];
        m.process_with_key(&mut [&mut main[..]], Some(&k[..]));
    }
    assert!(!m.is_open(), "a silent key after a burst must keep the gate closed");
}

#[test]
fn deterministic() {
    let x = sine(150.0, 0.2, 48000, 0);
    let run = || {
        let mut m = met(Settings::default());
        let mut b = x.clone();
        m.process(&mut [&mut b[..]]);
        b
    };
    assert_eq!(run(), run());
}
