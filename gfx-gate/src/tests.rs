// SPDX-License-Identifier: GPL-3.0-or-later
//! The gate's own bench suite (README.md §5), on synthetic signals.

use super::*;

const SR: f32 = 48000.0;

fn gate(f: impl FnOnce(&mut Settings)) -> Gate {
    let mut g = Gate::new();
    g.prepare(SR, 1, 2);
    let mut s = Settings::default();
    f(&mut s);
    g.set(s);
    g.reset();
    g
}

/// Runs mono, one sample at a time, returning (output, open flag) per sample.
fn run(g: &mut Gate, x: &[f32]) -> (Vec<f32>, Vec<bool>) {
    let mut y = Vec::with_capacity(x.len());
    let mut open = Vec::with_capacity(x.len());
    for &v in x {
        let mut b = [v];
        g.process(&mut [&mut b[..]]);
        y.push(b[0]);
        open.push(g.is_open());
    }
    (y, open)
}

fn ms(t: f32) -> usize {
    (t * 0.001 * SR) as usize
}

fn sine(n: usize, f: f32, amp: impl Fn(usize) -> f32) -> Vec<f32> {
    (0..n).map(|i| amp(i) * (2.0 * std::f32::consts::PI * f * i as f32 / SR).sin()).collect()
}

#[test]
fn opens_in_attack_time() {
    let mut g = gate(|s| s.attack = 3.0);
    let mut x = vec![0.0; ms(10.0)];
    x.extend(vec![0.5; ms(20.0)]);
    let (y, _) = run(&mut g, &x);
    let step = ms(10.0);
    let len = ms(3.0);
    assert!(y[step + len - 2] < 0.5, "opened too early");
    assert_eq!(y[step + len + 1], 0.5, "not fully open after the attack");
    // S-curve: half way at half the attack
    let mid = y[step + len / 2] / 0.5;
    assert!((mid - 0.5).abs() < 0.05, "{mid}");
}

#[test]
fn transparent_when_open() {
    let mut g = gate(|s| s.thresh = -90.0);
    let x = sine(ms(200.0), 440.0, |_| 0.3);
    let (y, _) = run(&mut g, &x);
    let start = ms(10.0);
    assert_eq!(&y[start..], &x[start..]);
}

#[test]
fn closes_after_window_plus_hold_then_releases_linearly_in_db() {
    // DC 0.5 for 200 ms, then −60 dBFS (below the −50 dB threshold)
    let mut g = gate(|s| {
        s.hold = 20.0;
        s.release = 150.0;
    });
    let low = 0.001f32;
    let mut x = vec![0.5; ms(200.0)];
    x.extend(vec![low; ms(400.0)]);
    let (y, _) = run(&mut g, &x);
    let step = ms(200.0);
    // window 40 ms (no hpf) + hold 20 ms
    let drop = (step..x.len()).find(|&i| y[i] < low * 0.999).unwrap() - step;
    assert!((drop as i64 - ms(60.0) as i64).abs() <= ms(1.0) as i64, "starts closing after {} ms", drop as f32 / 48.0);
    // linear in dB: −45 dB (half of 90) half way through the release
    let half = step + drop + ms(75.0);
    let db = 20.0 * (y[half] / low).log10();
    assert!((db + 45.0).abs() < 1.0, "{db} dB half way");
    assert_eq!(y[step + drop + ms(152.0)], 0.0, "silent after the release");
}

#[test]
fn hysteresis() {
    // slow ramp up then down, thresh −40, hyst −10: opens at −40, closes at −50
    let mut g = gate(|s| {
        s.thresh = -40.0;
        s.hyst = -10.0;
        s.hold = 0.0;
    });
    let n = ms(4000.0);
    let lvl = |i: usize| {
        let t = i as f32 / n as f32;
        let db = if t < 0.5 { -70.0 + 80.0 * t } else { -30.0 - 80.0 * (t - 0.5) };
        10f32.powf(db / 20.0)
    };
    let x = sine(n, 200.0, lvl);
    let (_, open) = run(&mut g, &x);
    let o = open.iter().position(|&v| v).unwrap();
    let c = o + open[o..].iter().position(|&v| !v).unwrap();
    let db_at = |i: usize| 20.0 * lvl(i).log10();
    assert!((db_at(o) + 40.0).abs() < 0.5, "opened at {}", db_at(o));
    assert!((db_at(c) + 50.0).abs() < 1.0, "closed at {}", db_at(c));
}

#[test]
fn no_chatter_on_low_notes() {
    for f in [82.4f32, 30.9] {
        let mut g = gate(|s| {
            s.thresh = -40.0;
            s.hold = 0.0;
            s.hyst = 0.0;
            s.attack = 0.5;
            s.release = 50.0;
        });
        // decays from −10 dBFS to −80 dBFS over 3 s
        let n = ms(3000.0);
        let x = sine(n, f, |i| 10f32.powf((-10.0 - 70.0 * i as f32 / n as f32) / 20.0));
        let (_, open) = run(&mut g, &x);
        let openings = open.windows(2).filter(|w| !w[0] && w[1]).count() + usize::from(open[0]);
        assert_eq!(openings, 1, "{f} Hz chattered");
    }
}

#[test]
fn range_sets_the_closed_level() {
    let mut g = gate(|s| s.range = -30.0);
    let x = sine(ms(500.0), 1000.0, |_| 0.001);
    let (y, _) = run(&mut g, &x);
    let peak = y[ms(300.0)..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let db = 20.0 * (peak / 0.001).log10();
    assert!((db + 30.0).abs() < 0.2, "{db}");
}

#[test]
fn expander_curve() {
    // ratio 2, knee 0, thresh −30: a signal 10 dB under comes out 20 dB under
    let mut g = gate(|s| {
        s.thresh = -30.0;
        s.ratio = 2.0;
    });
    let x = sine(ms(600.0), 1000.0, |_| 10f32.powf(-40.0 / 20.0));
    let (y, _) = run(&mut g, &x);
    let peak = y[ms(400.0)..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let db = 20.0 * peak.log10();
    assert!((db + 50.0).abs() < 0.3, "{db}");
}

#[test]
fn sidechain_highpass_ignores_rumble() {
    let mut g = gate(|s| {
        s.thresh = -30.0;
        s.hpf = 120.0;
    });
    let x = sine(ms(300.0), 40.0, |_| 0.1); // −20 dBFS at 40 Hz
    let (_, open) = run(&mut g, &x);
    assert!(!open.iter().any(|&o| o), "opened on 40 Hz rumble");
}

#[test]
fn hum_notch_keeps_it_closed() {
    let mut g = gate(|s| {
        s.thresh = -50.0;
        s.hum = 1;
    });
    let x = sine(ms(500.0), 50.0, |_| 10f32.powf(-45.0 / 20.0));
    let (_, open) = run(&mut g, &x);
    assert!(!open[ms(100.0)..].iter().any(|&o| o), "hum opened the gate");
}

#[test]
fn stereo_link_same_gain() {
    let mut g = gate(|s| s.thresh = -40.0);
    let n = ms(200.0);
    let mut l = sine(n, 500.0, |_| 0.5);
    let r_in = sine(n, 700.0, |_| 0.001);
    let mut r = r_in.clone();
    g.process(&mut [&mut l[..], &mut r[..]]);
    // the loud left channel opens the gate for both
    assert_eq!(&r[ms(50.0)..], &r_in[ms(50.0)..]);
}

#[test]
fn key_input_drives_the_gate() {
    let mut g = gate(|s| {
        s.thresh = -40.0;
        s.source = 1;
        s.hold = 0.0;
        s.release = 20.0;
    });
    let n = ms(400.0);
    let main_in = sine(n, 300.0, |_| 0.5); // loud all along
    let key: Vec<f32> = sine(n, 300.0, |i| if i < ms(100.0) { 0.5 } else { 0.0 });
    let mut main = main_in.clone();
    g.process_with_key(&mut [&mut main[..]], Some(&[&key[..]]));
    assert_eq!(main[ms(50.0)], main_in[ms(50.0)], "open while the key plays");
    assert!(main[ms(300.0)..].iter().all(|&v| v == 0.0), "closed once the key stops");
}

#[test]
fn tail_damp_takes_the_highs_first() {
    let level = |f: f32| {
        let mut g = gate(|s| {
            s.range = -30.0;
            s.damp = 100.0;
        });
        let x = sine(ms(500.0), f, |_| 0.001);
        let (y, _) = run(&mut g, &x);
        let peak = y[ms(300.0)..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        20.0 * (peak / 0.001).log10()
    };
    let low = level(200.0);
    let high = level(8000.0);
    assert!((low + 18.0).abs() < 0.5, "broadband {low}");
    assert!(high < low - 10.0, "highs {high} vs lows {low}");
}

#[test]
fn follow_decay_cuts_staccato_and_lets_rings_ring() {
    // time from close to −60 dB re the note, abrupt stop vs slow decay
    let release_time = |abrupt: bool| {
        let mut g = gate(|s| {
            s.thresh = -40.0;
            s.hold = 0.0;
            s.release = 100.0;
            s.follow = true;
        });
        let n = ms(3000.0);
        let x = sine(n, 1000.0, |i| {
            let t = i as f32 / SR;
            let db = if abrupt {
                if t < 0.5 { -10.0 } else { -80.0 }
            } else {
                -10.0 - 15.0 * t // −15 dB / s
            };
            10f32.powf(db / 20.0)
        });
        run(&mut g, &x);
        g.rel_mult
    };
    let fast = release_time(true);
    let slow = release_time(false);
    // rel_mult closer to 1 = slower release
    assert!(slow > fast, "slow {slow} fast {fast}");
    let ratio = (fast.ln() / slow.ln()).abs();
    assert!(ratio > 8.0, "release ratio {ratio} (expected ~16)");
}
