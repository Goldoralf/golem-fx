// SPDX-License-Identifier: GPL-3.0-or-later
//! The compressor's own bench suite (README.md §5), on synthetic signals.

use super::*;

const SR: f32 = 48000.0;

fn comp(f: impl FnOnce(&mut Settings)) -> Comp {
    let mut c = Comp::new();
    c.prepare(SR, 4800, 2);
    let mut s = Settings { makeup: 0.0, ..Settings::default() };
    f(&mut s);
    c.set(s);
    c.reset();
    c
}

fn sine(n: usize, f: f32, amp: f32) -> Vec<f32> {
    (0..n).map(|i| amp * (2.0 * std::f32::consts::PI * f * i as f32 / SR).sin()).collect()
}

fn ms(t: f32) -> usize {
    (t * 0.001 * SR) as usize
}

/// Runs in blocks of `block`, returning the output and the gain-reduction meter after each block.
fn run(c: &mut Comp, x: &[f32], block: usize) -> (Vec<f32>, Vec<f32>) {
    let mut y = x.to_vec();
    let mut gr = Vec::new();
    for chunk in y.chunks_mut(block) {
        c.process(&mut [chunk]);
        gr.push(c.gain_reduction_db());
    }
    (y, gr)
}

#[test]
fn one_static_curve_in_every_character() {
    let (t, r, w) = (-20.0, 4.0, 6.0);
    for ch in Character::ALL {
        for level in [-27.0f32, -19.0, -15.0, -7.0, -1.0] {
            let mut c = comp(|s| {
                s.character = ch;
                s.thresh = t;
                s.ratio = r;
                s.knee = w;
                s.attack = 5.0;
                s.release = 50.0;
            });
            let x = sine(ms(2000.0), 1000.0, 10f32.powf(level / 20.0));
            let (_, gr) = run(&mut c, &x, 480);
            let got = gr[gr.len() - 1];
            // levels read as RMS: a sine of peak `level` dBFS reads level − 3 dB
            let l = level - 3.0103;
            let want = l - static_curve(l, t, r, w);
            assert!((got - want).abs() < 0.5, "{ch:?} at {level} dBFS: gr {got:.2}, curve {want:.2}");
        }
    }
}

#[test]
fn clean_is_bit_transparent_below_threshold() {
    let mut c = comp(|s| s.thresh = -6.0);
    let x = sine(ms(300.0), 440.0, 0.05); // −26 dBFS, far below
    let (y, _) = run(&mut c, &x, 256);
    assert_eq!(y, x);
}

#[test]
fn attack_time_constant() {
    let mut c = comp(|s| {
        s.thresh = -30.0;
        s.ratio = 4.0;
        s.knee = 0.0;
        s.attack = 10.0;
        s.release = 200.0;
        s.auto_release = false;
    });
    let mut x = sine(ms(500.0), 1000.0, 0.003);
    x.extend(sine(ms(500.0), 1000.0, 0.5));
    let (_, gr) = run(&mut c, &x, 1);
    let step = ms(500.0);
    let fin = gr[gr.len() - 1];
    let t63 = gr[step..].iter().position(|&g| g >= 0.632 * fin).unwrap() as f32 / 48.0;
    assert!((8.0..=16.0).contains(&t63), "63 % after {t63:.2} ms (attack 10 ms)");
}

#[test]
fn auto_release_is_program_dependent() {
    let recovery = |ch: Character, burst_ms: f32| {
        let mut c = comp(|s| {
            s.character = ch;
            s.thresh = -30.0;
            s.ratio = 6.0;
            s.attack = 2.0;
            s.release = 100.0;
            s.auto_release = true;
        });
        let mut x = sine(ms(burst_ms), 1000.0, 0.5);
        x.extend(sine(ms(4000.0), 1000.0, 0.001));
        let (_, gr) = run(&mut c, &x, 48);
        let start = ms(burst_ms) / 48;
        let peak = gr[start - 1];
        gr[start..].iter().position(|&g| g < 0.1 * peak).unwrap_or(gr.len()) as f32
    };
    for ch in [Character::Clean, Character::Opto] {
        let short = recovery(ch, 50.0);
        let long = recovery(ch, 3000.0);
        assert!(long > 1.5 * short, "{ch:?}: after a burst {short}, after a sustain {long}");
    }
}

#[test]
fn feedback_is_stable_at_extremes() {
    for ch in [Character::Fet, Character::Opto, Character::Pedal] {
        let mut c = comp(|s| {
            s.character = ch;
            s.thresh = -50.0;
            s.ratio = 30.0;
            s.knee = 12.0;
            s.attack = 0.02;
            s.release = 5.0;
            s.drive = 100.0;
        });
        let mut rng = gfx_dsp::lfo::Rng::new(3);
        let x: Vec<f32> = (0..ms(1000.0)).map(|_| 2.0 * rng.bipolar()).collect();
        let (y, gr) = run(&mut c, &x, 64);
        assert!(y.iter().all(|v| v.is_finite() && v.abs() < 8.0), "{ch:?} output");
        assert!(gr.iter().all(|g| g.is_finite() && *g <= 60.0 + 1e-3), "{ch:?} gr");
    }
}

#[test]
fn sidechain_low_cut_ignores_the_lows() {
    let gr_at = |hpf: f32| {
        let mut c = comp(|s| {
            s.thresh = -20.0;
            s.sc_hpf = hpf;
        });
        let x = sine(ms(1000.0), 50.0, 0.5);
        let (_, gr) = run(&mut c, &x, 480);
        gr[gr.len() - 1]
    };
    let without = gr_at(0.0);
    let with = gr_at(150.0);
    assert!(without > 8.0 && with < without - 6.0, "without {without}, with {with}");
}

#[test]
fn range_caps_the_reduction() {
    let mut c = comp(|s| {
        s.thresh = -40.0;
        s.ratio = 30.0;
        s.range = 6.0;
    });
    let x = sine(ms(1000.0), 1000.0, 0.9);
    let (_, gr) = run(&mut c, &x, 480);
    assert!((gr[gr.len() - 1] - 6.0).abs() < 0.05, "{}", gr[gr.len() - 1]);
}

#[test]
fn parallel_mix_does_not_comb() {
    // FET (colour path at 2×), no reduction, small signal: 50 % mix must stay flat
    let gain_at = |f: f32| {
        let mut c = comp(|s| {
            s.character = Character::Fet;
            s.thresh = 0.0;
            s.mix = 50.0;
        });
        let x = sine(ms(500.0), f, 0.01);
        let (y, _) = run(&mut c, &x, 480);
        let e = |v: &[f32]| v[ms(250.0)..].iter().map(|s| (*s as f64).powi(2)).sum::<f64>();
        10.0 * (e(&y) / e(&x)).log10()
    };
    let g = [gain_at(200.0), gain_at(5000.0), gain_at(15000.0)];
    let spread = g.iter().cloned().fold(f64::MIN, f64::max) - g.iter().cloned().fold(f64::MAX, f64::min);
    assert!(spread < 0.2, "{g:?}");
}

#[test]
fn fet_drive_stays_clean_of_aliasing() {
    // 5 kHz near full scale, FET + drive 100: little energy far from the harmonics of 5 kHz
    let mut c = comp(|s| {
        s.character = Character::Fet;
        s.thresh = -60.0;
        s.ratio = 20.0;
        s.attack = 0.02;
        s.drive = 100.0;
    });
    let x = sine(8192 + 4096, 5000.0, 0.9);
    let (y, _) = run(&mut c, &x, 256);
    let y = &y[4096..];
    // energy at 1 kHz (an alias of 5 kHz harmonics folding at 48 kHz would land on multiples of 1 kHz
    // that are not multiples of 5 kHz, e.g. 23·5 = 115 → |115 − 2·48| = 19 kHz, 9·5 = 45 → 3 kHz)
    let probe = |f: f64| {
        let w = 2.0 * std::f64::consts::PI * f / SR as f64;
        let (mut re, mut im) = (0.0, 0.0);
        for (i, v) in y.iter().enumerate() {
            let win = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / y.len() as f64).cos();
            re += *v as f64 * win * (w * i as f64).cos();
            im += *v as f64 * win * (w * i as f64).sin();
        }
        (re * re + im * im).sqrt()
    };
    let fund = probe(5000.0);
    let alias = probe(3000.0).max(probe(19000.0));
    let db = 20.0 * (alias / fund).log10();
    assert!(db < -80.0, "alias {db:.1} dB re fundamental");
}

