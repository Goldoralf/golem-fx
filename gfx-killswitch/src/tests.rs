// SPDX-License-Identifier: GPL-3.0-or-later
//! The kill switch's own bench suite (README.md §6).

use super::*;

const SR: f32 = 48000.0;

fn ks(s: Settings, tempo: f64) -> Killswitch {
    let mut k = Killswitch::new();
    k.prepare(SR, 4800, 1);
    k.set(s);
    k.set_transport(TransportInfo { tempo: Some(tempo), pos_beats: None, playing: false });
    k.reset();
    k
}

/// The gain the switch applies: a constant input of 1.
fn gains(k: &mut Killswitch, n: usize) -> Vec<f32> {
    let mut y = vec![1.0f32; n];
    for c in y.chunks_mut(128) {
        k.process(&mut [c]);
    }
    y
}

#[test]
fn edges_reach_their_target_even_when_short() {
    // 1/32 at 200 BPM is a 9.4 ms step; smooth 20 ms would never arrive with a slew limit
    let mut k = ks(Settings::default().with(p::SYNC, 6.0).with(p::SMOOTH, 20.0).with(p::DEPTH, 100.0), 200.0);
    let g = gains(&mut k, 96000);
    let step = (0.125 * 60.0 / 200.0 * SR as f64) as usize;
    for s in 2..40 {
        let seg = &g[s * step..(s + 1) * step];
        let (lo, hi) = seg.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        assert!(hi > 0.999 && lo < 0.001, "step {s}: {lo} … {hi}");
    }
}

#[test]
fn smooth_zero_is_a_hard_cut() {
    let mut k = ks(Settings::default().with(p::SMOOTH, 0.0), 120.0);
    let g = gains(&mut k, 24000);
    let edges = g.windows(2).filter(|w| (w[1] - w[0]).abs() > 0.99).count();
    let partial = g.iter().filter(|v| **v > 0.01 && **v < 0.99).count();
    // four 1/16 steps in 0.5 s at 120 BPM: an opening and a closing each (the first opening is the start)
    assert!(edges >= 7 && partial == 0, "{edges} hard edges, {partial} in-between samples");
}

#[test]
fn beat_sync_puts_step_one_on_the_downbeat() {
    // the host plays from bar 3.2 (beat 9) at 120 BPM: 1/16 steps, pattern = step 1 only
    let mut k = ks(Settings::default().with(p::TRIGGER, 2.0).with(p::SYNC, 4.0).with(p::PATTERN, 1.0).with(p::SMOOTH, 0.0).with(p::LENGTH, 50.0), 120.0);
    let spb = 24000.0;
    let mut y = vec![1.0f32; 4 * 24000];
    let mut pos = 9.0f64;
    for c in y.chunks_mut(128) {
        k.set_transport(TransportInfo { tempo: Some(120.0), pos_beats: Some(pos), playing: true });
        k.process(&mut [c]);
        pos += c.len() as f64 / spb;
    }
    // the open steps start every bar of 16 sixteenths... with a 16-step pattern, every 4 beats (bar 4 = beat 12)
    let first_open = y.iter().position(|v| *v > 0.5).unwrap();
    assert_eq!(first_open, (3.0 * spb) as usize, "step 1 opens on beat 12");
}

#[test]
fn swing_lengthens_the_first_of_each_pair() {
    let mut k = ks(Settings::default().with(p::SWING, 30.0).with(p::SMOOTH, 0.0).with(p::LENGTH, 50.0).with(p::SYNC, 2.0), 120.0);
    let g = gains(&mut k, 48000 * 2);
    let rises: Vec<usize> = g.windows(2).enumerate().filter(|(_, w)| w[0] < 0.5 && w[1] >= 0.5).map(|(i, _)| i + 1).collect();
    let t = 12000.0; // an eighth at 120 BPM
    let (a, b) = ((rises[1] - rises[0]) as f64, (rises[2] - rises[1]) as f64);
    assert!((a / t - 1.3).abs() < 0.01 && (b / t - 0.7).abs() < 0.01 || (a / t - 0.7).abs() < 0.01 && (b / t - 1.3).abs() < 0.01, "{a} {b}");
}

#[test]
fn engage_restarts_and_kill_crossfades() {
    let mut k = ks(Settings::default().with(p::BYPASS, 1.0), 120.0);
    gains(&mut k, 7000);
    k.set(k.settings().with(p::BYPASS, 0.0));
    let g = gains(&mut k, 2000);
    // just engaged: step 1 is open (after the 5 ms engage fade)
    assert!(g[480..1000].iter().all(|v| *v > 0.99));
    k.set(k.settings().with(p::KILL, 1.0));
    let g = gains(&mut k, 480);
    let steps = g.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    assert!(g[470] < 1e-3 && steps < 0.1, "kill: last {}, biggest step {steps}", g[470]);
}
