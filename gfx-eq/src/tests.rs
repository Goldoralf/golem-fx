// SPDX-License-Identifier: GPL-3.0-or-later
//! The EQ's own bench suite (README.md §5).

use super::*;
use gfx_dsp::matched::Analog;

const SR: f32 = 48000.0;

fn eq(settings: &[(usize, f32)]) -> Eq {
    let mut e = Eq::new();
    e.prepare(SR, 4800, 2);
    for &(i, v) in settings {
        e.set_index(i, v);
    }
    e.reset();
    e
}

fn gain_at(e: &mut Eq, f: f32) -> f32 {
    let n = 24000;
    let x: Vec<f32> = (0..n).map(|i| (2.0 * std::f32::consts::PI * f * i as f32 / SR).sin() * 0.1).collect();
    let mut y = x.clone();
    for c in y.chunks_mut(480) {
        e.process(&mut [c]);
    }
    let en = |v: &[f32]| v[n / 2..].iter().map(|s| (*s as f64).powi(2)).sum::<f64>();
    (10.0 * (en(&y) / en(&x)).log10()) as f32
}

#[test]
fn flat_is_bit_transparent() {
    let mut e = eq(&[(p::LOWCUT, 10.0)]);
    let x: Vec<f32> = (0..9600).map(|i| ((i * 7919) % 2000) as f32 / 2000.0 - 0.5).collect();
    let mut y = x.clone();
    for c in y.chunks_mut(256) {
        e.process(&mut [c]);
    }
    assert_eq!(y, x);
}

#[test]
fn bell_matches_the_analog_curve_up_to_the_top() {
    for &(f0, g) in &[(1000.0f32, 9.0f32), (8000.0, -9.0), (8000.0, 12.0)] {
        let mut e = eq(&[(p::LOWCUT, 10.0), (p::MIDFREQ, f0), (p::MIDGAIN, g), (p::MIDQ, 1.0), (p::MIDPROP, 0.0)]);
        let proto = Analog::bell(1.0, g as f64);
        for &f in &[200.0f32, 1000.0, 4000.0, 8000.0, 12000.0, 16000.0, 20000.0] {
            let want = 10.0 * proto.mag2((f / f0) as f64).log10() as f32;
            let got = gain_at(&mut e, f);
            assert!((got - want).abs() < 1.0, "bell {f0} Hz {g} dB at {f} Hz: {got:.2} vs {want:.2}");
        }
    }
}

#[test]
fn proportional_q_narrows_with_gain() {
    // relative gain one octave above the centre: smaller with 100 % proportional Q at 12 dB
    let shape = |prop: f32| {
        let mut e = eq(&[(p::LOWCUT, 10.0), (p::MIDFREQ, 1000.0), (p::MIDGAIN, 12.0), (p::MIDQ, 0.7), (p::MIDPROP, prop)]);
        gain_at(&mut e, 2000.0)
    };
    let constant = shape(0.0);
    let proportional = shape(100.0);
    assert!(proportional < constant - 1.5, "constant {constant:.2}, proportional {proportional:.2}");
}

#[test]
fn deharsh_is_level_independent() {
    // pink-ish broadband bed + a loud 3.5 kHz component; the same mix 20 dB lower must be cut the same
    let cut = |level: f32| {
        let mut e = eq(&[(p::LOWCUT, 10.0), (p::HARSH_THRESH, -20.0), (p::HARSH_RANGE, 12.0)]);
        let mut rng = gfx_dsp::lfo::Rng::new(9);
        let n = 48000;
        let x: Vec<f32> = (0..n)
            .map(|i| level * (0.2 * rng.bipolar() + 0.5 * (2.0 * std::f32::consts::PI * 3500.0 * i as f32 / SR).sin()))
            .collect();
        let mut y = x;
        let mut m = 0.0f32;
        for c in y.chunks_mut(480) {
            e.process(&mut [c]);
            m = e.harsh_reduction_db();
        }
        m
    };
    let loud = cut(1.0);
    let soft = cut(0.1);
    assert!(loud > 3.0, "no cut ({loud})");
    assert!((loud - soft).abs() < 1.0, "loud {loud:.2} soft {soft:.2}");
}

#[test]
fn auto_gain_keeps_loudness() {
    let level = |settings: &[(usize, f32)]| {
        let mut e = eq(settings);
        // pink noise (Paul Kellet's filter on seeded white)
        let mut rng = gfx_dsp::lfo::Rng::new(4);
        let mut b = [0.0f64; 7];
        let x: Vec<f32> = (0..96000)
            .map(|_| {
                let w = rng.bipolar() as f64;
                b[0] = 0.99886 * b[0] + w * 0.0555179;
                b[1] = 0.99332 * b[1] + w * 0.0750759;
                b[2] = 0.96900 * b[2] + w * 0.1538520;
                b[3] = 0.86650 * b[3] + w * 0.3104856;
                b[4] = 0.55000 * b[4] + w * 0.5329522;
                b[5] = -0.7616 * b[5] - w * 0.0168980;
                let y = b.iter().take(6).sum::<f64>() + b[6] + w * 0.5362;
                b[6] = w * 0.115926;
                (0.05 * y) as f32
            })
            .collect();
        let mut y = x.clone();
        for c in y.chunks_mut(480) {
            e.process(&mut [c]);
        }
        // measured as a loudness meter would: below 40 Hz does not count (K-weighting's high-pass)
        let en = |v: &[f32]| {
            let mut hp = gfx_dsp::svf::Svf::new();
            hp.set(gfx_dsp::svf::SvfMode::HighPass, 40.0, 0.707, 0.0, SR);
            v.iter().map(|s| hp.process(*s)).skip(48000).map(|s| (s as f64).powi(2)).sum::<f64>()
        };
        (10.0 * (en(&y) / en(&x)).log10()) as f32
    };
    for s in [vec![(p::TILT, 6.0)], vec![(p::MIDGAIN, 6.0), (p::MIDFREQ, 800.0)], vec![(p::LOWS, -6.0)]] {
        let mut with = s.clone();
        with.push((p::AUTO_GAIN, 1.0));
        with.push((p::LOWCUT, 10.0));
        let l = level(&with);
        assert!(l.abs() < 0.5, "{s:?}: {l:.2} dB with auto gain");
    }
}

#[test]
fn high_cut_24_is_steeper() {
    let at = |slope: f32| {
        let mut e = eq(&[(p::LOWCUT, 10.0), (p::HIGHCUT, 4000.0), (p::HIGHCUT_SLOPE, slope)]);
        gain_at(&mut e, 8000.0)
    };
    let s12 = at(0.0);
    let s24 = at(1.0);
    assert!((s12 + 12.3).abs() < 1.5, "12 dB / oct: {s12:.2}");
    assert!((s24 + 24.1).abs() < 2.0, "24 dB / oct: {s24:.2}");
}
