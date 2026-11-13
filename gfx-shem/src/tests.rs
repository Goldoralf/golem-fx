// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5). The reference is the analog network, solved by nodal analysis.

use super::*;

const SR: f32 = 48000.0;
const PI: f64 = std::f64::consts::PI;

fn shem(grind: f32, full: bool) -> Shem {
    let mut s = Shem::new();
    s.set_grind(grind);
    s.set_voice(full);
    s.prepare(SR, 512, 1);
    s.reset();
    s
}

/// Steady-state gain in dB of the plugin at `freq`.
fn measured(s: &mut Shem, freq: f64) -> f64 {
    let n = 96000;
    let w = 2.0 * PI * freq / SR as f64;
    let x: Vec<f32> = (0..n).map(|i| (0.1 * (w * i as f64).sin()) as f32).collect();
    let mut y = x.clone();
    for c in y.chunks_mut(512) {
        s.process(&mut [c]);
    }
    let (ei, eo) = (x[n / 2..].iter().map(|v| (*v as f64).powi(2)).sum::<f64>(), y[n / 2..].iter().map(|v| (*v as f64).powi(2)).sum::<f64>());
    10.0 * (eo / ei).log10()
}

fn warped(freq: f64) -> f64 {
    SR as f64 / PI * (PI * freq / SR as f64).tan()
}

#[test]
fn analysis_anchors() {
    // the design review's figures, before the level pot (Focused)
    for (f, want) in [(100.0, 3.7), (300.0, 12.3), (1000.0, 22.5), (4000.0, 20.5)] {
        let g = network_db(ARMS[0], f);
        assert!((g - want).abs() < 0.5, "{f} Hz: {g} dB, review says {want}");
    }
}

#[test]
fn response_matches_the_analog_network() {
    for (arm, full) in [(0, false), (1, true)] {
        // grind = the Focused plateau: the plugin's gain = network − plateau(Focused) + grind
        let mut s = shem(0.0, full);
        let plateau = network_db(ARMS[0], PLATEAU_HZ);
        for f in [30.0, 100.0, 300.0, 1000.0, 3000.0, 10000.0] {
            // the network, the 5 Hz DC blocker standing for the coupling capacitors
            let dc = 10.0 * (1.0 / (1.0 + (DC_HZ / f).powi(2))).log10();
            let want = network_db(ARMS[arm], warped(f)) - plateau + dc;
            let got = measured(&mut s, f);
            assert!((got - want).abs() < 0.25, "arm {arm}, {f} Hz: {got} vs {want}");
        }
    }
}

#[test]
fn grind_is_the_plateau_gain() {
    for g in [-8.0f32, 0.0, 10.0, 20.0] {
        let mut s = shem(g, false);
        let got = measured(&mut s, PLATEAU_HZ);
        let want = g as f64 + network_db(ARMS[0], warped(PLATEAU_HZ)) - network_db(ARMS[0], PLATEAU_HZ);
        assert!((got - want).abs() < 0.1, "grind {g}: {got} dB");
    }
}

#[test]
fn full_is_lower_at_the_plateau() {
    let d = network_db(ARMS[0], PLATEAU_HZ) - network_db(ARMS[1], PLATEAU_HZ);
    assert!((3.0..6.5).contains(&d), "Full is {d} dB under Focused at the plateau");
}

#[test]
fn voice_switch_does_not_click() {
    let mut s = shem(10.0, false);
    let w = 2.0 * PI * 200.0 / SR as f64;
    let mut prev = 0.0f32;
    let mut worst = 0.0f32;
    let mut steady = 0.0f32;
    for k in 0..200 {
        if k == 100 {
            s.set_voice(true);
        }
        let mut b: Vec<f32> = (0..480).map(|i| (0.1 * (w * (k * 480 + i) as f64).sin()) as f32).collect();
        s.process(&mut [&mut b[..]]);
        for &v in &b {
            let d = (v - prev).abs();
            if k < 100 && k > 50 {
                steady = steady.max(d);
            }
            if k >= 100 {
                worst = worst.max(d);
            }
            prev = v;
        }
    }
    assert!(worst < steady * 1.2, "switch step {worst} vs steady {steady}");
}

#[test]
fn linear() {
    let mut s = shem(10.0, false);
    let w = 2.0 * PI * 1000.0 / SR as f64;
    let x: Vec<f32> = (0..96000).map(|i| (0.5 * (w * i as f64).sin()) as f32).collect();
    let mut y = x.clone();
    for c in y.chunks_mut(512) {
        s.process(&mut [c]);
    }
    // project onto the fundamental; the residual is distortion
    let tail = &y[48000..];
    let (mut c, mut si) = (0.0, 0.0);
    for (i, v) in tail.iter().enumerate() {
        let ph = w * (i + 48000) as f64;
        c += *v as f64 * ph.cos();
        si += *v as f64 * ph.sin();
    }
    let a = 2.0 / tail.len() as f64;
    let (c, si) = (c * a, si * a);
    let mut res = 0.0;
    let mut tot = 0.0;
    for (i, v) in tail.iter().enumerate() {
        let ph = w * (i + 48000) as f64;
        let fit = c * ph.cos() + si * ph.sin();
        res += (*v as f64 - fit).powi(2);
        tot += (*v as f64).powi(2);
    }
    let thd = 10.0 * (res / tot).log10();
    assert!(thd < -100.0, "THD {thd} dB");
}

#[test]
fn deterministic() {
    let x: Vec<f32> = (0..9600).map(|i| ((i * 7919 % 1000) as f32 / 1000.0 - 0.5) * 0.2).collect();
    let run = || {
        let mut s = shem(12.0, false);
        let mut b = x.clone();
        s.process(&mut [&mut b[..]]);
        b
    };
    assert_eq!(run(), run());
}

